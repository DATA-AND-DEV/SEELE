//! A voz pela fronteira (Tarefa 3.3 da F4, lote A): o JavaScript move bytes e
//! alças, e a conta é a do desktop (`seele-voz`).
//!
//! **O que manda.** O relógio de mídia ([`RelogioDeMidia`]) e o cabeçalho
//! (`seele_proto::media`). A página diz que um quadro de 20 ms passou, pede o
//! carimbo de cada quadro que o portão soltou, e entrega o Opus de cada um com
//! o carimbo dele: a sequência e o cabeçalho são daqui, na mesma ordem do laço
//! do desktop.
//!
//! **O que recebe.** O cabeçalho lido aqui, e a [`TabelaDeFontes`] do desktop,
//! que guarda para cada fonte uma **alça** opaca. A página não conhece SSRC
//! nenhum: «uma casca que sabe nomear um `ssrc` já tem lógica dentro» (ADR
//! 0057).
//!
//! # O portão, o jitter e o mixer (lote C)
//!
//! **O portão mora no [`Envio`]**, na thread da página, e não no worklet: cada
//! quadro que ele solta precisa do carimbo do mesmo relógio, e vai para o
//! `AudioEncoder`, que só existe fora do worklet. [`Envio::capturou`] faz, por
//! quadro de 20 ms, o que o laço do desktop faz: o portão (que roda mesmo no
//! mudo, para o medidor andar), o relógio (que anda saia algo ou não) e o
//! carimbo de cada quadro solto. A régua é a do desktop,
//! [`GateConfig::dos_controles`].
//!
//! **O ganho automático do desktop (`seele_audio::ganho`) não roda aqui.** O
//! ADR 0057 aceitou o DSP da plataforma: o `getUserMedia` pede o cancelamento
//! de eco, a supressão e o ganho do navegador, e um segundo ganho em cima do
//! primeiro bombearia. O ganho do navegador vem **antes** do portão, que é a
//! ordem que o doc do `ganho` desaconselha, e o ADR registra que o efeito no
//! limiar não foi medido.
//!
//! **O jitter e o mixer moram na [`Reproducao`]**, dentro do `AudioWorklet`,
//! cujo `process()` é o relógio da voz (nada da voz depende de temporizador).
//! A ordem é a do navegador, e não a do desktop: lá o buffer guarda Opus e o
//! decodificador roda na hora de tocar, com a ocultação do libopus num quadro
//! que falta. Aqui o `AudioDecoder` não existe no worklet e não tem chamada
//! para ocultar quadro, então a página decodifica na chegada, o buffer guarda
//! o som já decodificado, e **um quadro que falta vira silêncio com fade** (a
//! emenda do ADR 0057 ao ADR 0010). Decodificar na chegada põe um pacote fora
//! de ordem no decodificador fora de ordem; o buffer o devolve ao lugar, e o
//! que se perde é a continuidade do estado do Opus naquele quadro.

use std::collections::BTreeMap;

use seele_proto::media::{MediaError, MediaHeader, HEADER_LEN};
use seele_voz::fontes::TabelaDeFontes;
use seele_voz::gate::{GateConfig, GateMetrics, GateMode, VoiceGate};
use seele_voz::jitter::{Decision, JitterBuffer, JitterConfig};
use seele_voz::mixer::Mixer;
use seele_voz::recepcao::{ContagemDaFonte, JanelaDeRecepcao};
use seele_voz::relogio::{carimbo_do_quadro, RelogioDeMidia};
use seele_voz::FRAME_SAMPLES;

/// A voz que sai.
#[derive(Debug)]
pub struct Envio {
    ssrc: u32,
    relogio: RelogioDeMidia,
    portao: VoiceGate,
    /// A tecla do aperte-para-falar, guardada para o portão novo de um
    /// [`Self::reabrir`].
    tecla: bool,
    a_transmitir: Vec<Vec<f32>>,
}

impl Envio {
    /// O envio de quem o servidor chamou de `ssrc`, com o relógio em zero.
    #[must_use]
    pub fn novo(ssrc: u32) -> Self {
        Self::de(ssrc, 0, 0)
    }

    /// O envio de uma voz **nova** de `ssrc`, com o relógio partindo da hora
    /// da página, `agora_ms` (o `performance.timeOrigin + performance.now()`,
    /// que é monotônico dentro da página), e não do zero. Ver
    /// [`seele_voz::relogio::partida`]: quem saiu sentado e voltou recebe o
    /// mesmo `ssrc`, e do zero ficava muda para quem ficou pela vida inteira
    /// da voz velha. Uma hora que não é número (a página antiga, que não a
    /// manda) dá o zero de antes.
    #[must_use]
    pub fn partindo_de(ssrc: u32, agora_ms: f64) -> Self {
        // `as u64` satura: NaN e negativos dão zero, e o infinito o teto.
        let (seq, carimbo) =
            seele_voz::relogio::partida(std::time::Duration::from_millis(agora_ms as u64));
        Self::de(ssrc, seq, carimbo)
    }

    /// O envio de `ssrc` com o relógio continuando de `seq` e `carimbo`.
    ///
    /// O portão nasce com os controles de partida do desktop
    /// (`Controls::novos`): aperte-para-falar, supressão ligada e a
    /// sensibilidade no padrão. A página veste o que a pessoa escolheu por
    /// cima, com [`Self::regua`] e [`Self::modo`].
    #[must_use]
    pub fn de(ssrc: u32, seq: u16, carimbo: u32) -> Self {
        Self {
            ssrc,
            relogio: RelogioDeMidia::de(seq, carimbo),
            portao: VoiceGate::new(GateConfig::dos_controles(0, true), GateMode::PushToTalk),
            tecla: false,
            a_transmitir: Vec::new(),
        }
    }

    /// A régua do portão, de [`GateConfig::dos_controles`]: troca sem fechar o
    /// microfone, como no desktop (F01).
    pub fn regua(&mut self, abertura_dbfs_milesimos: i32, com_supressao: bool) {
        self.portao.ajustar(GateConfig::dos_controles(
            abertura_dbfs_milesimos,
            com_supressao,
        ));
    }

    /// Como o microfone abre.
    pub fn modo(&mut self, modo: GateMode) {
        self.portao.set_mode(modo);
    }

    /// A tecla do aperte-para-falar.
    pub fn tecla(&mut self, segurando: bool) {
        self.tecla = segurando;
        self.portao.set_key_held(segurando);
    }

    /// O que o portão decidiu por último: o nível, o limiar que vale e as
    /// contas, para o medidor.
    #[must_use]
    pub fn portao(&self) -> GateMetrics {
        self.portao.metrics()
    }

    /// Um quadro de 20 ms do microfone: os quadros a codificar, cada um com o
    /// carimbo dele, em ordem.
    ///
    /// **Na ordem do laço do desktop** (`voice.rs`, «capture, encode, send»):
    /// - o portão roda **mesmo no mudo**, para o medidor andar e quem fala no
    ///   microfone mudo ver que fala;
    /// - o relógio anda saia algo ou não, porque o carimbo conta amostras que
    ///   passaram, e é ele que deixa quem recebe separar silêncio de perda;
    /// - na abertura saem os quadros retidos na frente deste (a primeira
    ///   sílaba de F01), cada um com o instante em que aconteceu.
    ///
    /// Vazio quando o portão está fechado ou o microfone está mudo.
    pub fn capturou(&mut self, quadro: &[f32], mudo: bool) -> Vec<(u32, Vec<f32>)> {
        self.portao
            .quadros_a_transmitir(quadro, &mut self.a_transmitir);
        let falando = !self.a_transmitir.is_empty() && !mudo;
        let agora = self.relogio.quadro_passou();
        if !falando {
            return Vec::new();
        }
        let quantos = self.a_transmitir.len();
        std::mem::take(&mut self.a_transmitir)
            .into_iter()
            .enumerate()
            .map(|(indice, quadro)| (carimbo_do_quadro(agora, quantos, indice), quadro))
            .collect()
    }

    /// Um quadro de 20 ms passou, saia algo dele ou não. Devolve o carimbo de
    /// agora.
    pub fn quadro_passou(&mut self) -> u32 {
        self.relogio.quadro_passou()
    }

    /// O carimbo do quadro `indice` de `quantos` que o portão acabou de soltar.
    #[must_use]
    pub fn carimbo_do_quadro(&self, quantos: usize, indice: usize) -> u32 {
        carimbo_do_quadro(self.relogio.carimbo(), quantos, indice)
    }

    /// O datagrama de um quadro de Opus, com o carimbo dele.
    ///
    /// **Na ordem do laço do desktop:** um Opus vazio (o DTX) não sai e não
    /// anda a sequência; qualquer outro anda a sequência, e só então vira
    /// cabeçalho e datagrama.
    ///
    /// # Errors
    ///
    /// [`MediaError::EmptyPayload`] para o DTX, e o erro do cabeçalho para um
    /// Opus maior que o teto do datagrama.
    pub fn datagrama(&mut self, carimbo: u32, opus: &[u8]) -> Result<Vec<u8>, MediaError> {
        if opus.is_empty() {
            return Err(MediaError::EmptyPayload);
        }
        let seq = self.relogio.pacote_saiu();
        let cabecalho = MediaHeader {
            ssrc: self.ssrc,
            seq,
            timestamp: carimbo,
        };
        let mut datagrama = vec![0_u8; HEADER_LEN + opus.len()];
        cabecalho.encode_datagram(opus, &mut datagrama)?;
        Ok(datagrama)
    }

    /// Outro caminho: outro microfone (o mesmo `ssrc`) ou a reconexão (o
    /// `ssrc` novo). O relógio salta dois quadros, como no desktop.
    ///
    /// **O portão é outro, com a mesma régua, o mesmo modo e a mesma tecla**:
    /// no desktop cada caminho de voz monta o seu portão a partir dos
    /// controles, e o piso medido, os retidos e a sustentação são do caminho
    /// que acabou.
    pub fn reabrir(&mut self, ssrc: u32) {
        self.ssrc = ssrc;
        self.relogio = self.relogio.salto();
        self.portao = VoiceGate::new(self.portao.config(), self.portao.mode());
        self.portao.set_key_held(self.tecla);
    }

    /// O relógio agora: a sequência do último pacote e o carimbo de agora.
    #[must_use]
    pub const fn relogio(&self) -> (u16, u32) {
        (self.relogio.seq(), self.relogio.carimbo())
    }
}

/// Um quadro de voz que chegou, separado por fonte.
#[derive(Debug, PartialEq, Eq)]
pub struct Chegado<'a> {
    /// A alça da fonte: o que a página guarda no lugar do SSRC.
    pub alca: u32,
    /// A sequência do pacote.
    pub seq: u16,
    /// O carimbo do quadro, em amostras.
    pub carimbo: u32,
    /// O Opus.
    pub opus: &'a [u8],
}

/// A voz que chega.
#[derive(Debug)]
pub struct Recepcao {
    tabela: TabelaDeFontes<u32>,
    ultima_alca: u32,
}

impl Recepcao {
    /// A recepção de quem fala com o SSRC `meu`, que nunca entra.
    #[must_use]
    pub const fn nova(meu: u32) -> Self {
        Self {
            tabela: TabelaDeFontes::nova(meu),
            ultima_alca: 0,
        }
    }

    /// Um datagrama chegou em `agora_ms`: a fonte dele, com uma alça nova na
    /// primeira vez, ou `None` para a própria voz.
    ///
    /// # Errors
    ///
    /// O erro do cabeçalho, para um datagrama torto (o desktop o ignora).
    pub fn chegou<'a>(
        &mut self,
        datagrama: &'a [u8],
        agora_ms: f64,
    ) -> Result<Option<Chegado<'a>>, MediaError> {
        let (cabecalho, opus) = MediaHeader::decode(datagrama)?;
        let ultima = &mut self.ultima_alca;
        let alca = self.tabela.chegou(cabecalho.ssrc, agora_ms, || {
            *ultima = ultima.wrapping_add(1);
            Some(*ultima)
        });
        Ok(alca.map(|alca| Chegado {
            alca: *alca,
            seq: cabecalho.seq,
            carimbo: cabecalho.timestamp,
            opus,
        }))
    }

    /// Esquece quem calou, e devolve as alças esquecidas: a página fecha o
    /// decodificador de cada uma.
    pub fn esquecer_caladas(&mut self, agora_ms: f64) -> Vec<u32> {
        let antes: Vec<u32> = self.tabela.iter().map(|(_, alca)| *alca).collect();
        self.tabela.esquecer_caladas(agora_ms);
        let ficaram: Vec<u32> = self.tabela.iter().map(|(_, alca)| *alca).collect();
        antes
            .into_iter()
            .filter(|alca| !ficaram.contains(alca))
            .collect()
    }

    /// A alça de quem fala com `ssrc`, para quem a liga a uma pessoa (o volume
    /// de cada um).
    #[must_use]
    pub fn alca_de(&self, ssrc: u32) -> Option<u32> {
        self.tabela
            .iter()
            .find(|(de_quem, _)| *de_quem == ssrc)
            .map(|(_, alca)| *alca)
    }
}

/// O que o teste do microfone tem a contar, como o `EstadoDoTesteDeMicrofone`
/// do desktop (`seele_ffi`), menos o monitor e a falha, que são da página.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EstadoDoTeste {
    /// O nível do quadro que o portão viu por último, em dBFS.
    pub nivel_dbfs: f32,
    /// Se a transmissão estaria aberta agora.
    pub aberto: bool,
    /// Quantas vezes o portão abriu desde que o teste começou.
    pub aberturas: u64,
    /// Quantos quadros anteriores a uma abertura saíram junto com ela.
    pub quadros_retidos: u64,
    /// Onde o corte está, em dBFS: no padrão ele sobe com o ruído medido.
    pub corte_dbfs: f32,
}

/// O teste do microfone do PC, fora da sala (F01), com **o mesmo portão** da
/// voz: o do [`Envio`], na ativação por voz, como o `teste_de_microfone.rs` do
/// desktop o abre, e com a régua dos controles.
///
/// O caminho é o da conversa menos o codec e a rede, como no desktop. O que
/// fica de fora aqui também é a supressão e o ganho do desktop: no navegador
/// eles são os da plataforma, aplicados antes de o quadro chegar.
#[derive(Debug)]
pub struct TesteDoMicrofone {
    envio: Envio,
    aberto: bool,
}

impl TesteDoMicrofone {
    /// Um teste com a régua dos controles guardados.
    #[must_use]
    pub fn abrir(abertura_dbfs_milesimos: i32, com_supressao: bool) -> Self {
        let mut envio = Envio::novo(0);
        envio.regua(abertura_dbfs_milesimos, com_supressao);
        envio.modo(GateMode::VoiceActivated);
        Self {
            envio,
            aberto: false,
        }
    }

    /// Um quadro de 20 ms do microfone: os que o portão solta, que são o que
    /// o monitor toca (o que os outros ouviriam).
    pub fn quadro(&mut self, quadro: &[f32]) -> Vec<(u32, Vec<f32>)> {
        let saem = self.envio.capturou(quadro, false);
        self.aberto = !saem.is_empty();
        saem
    }

    /// O que o teste tem a contar agora.
    #[must_use]
    pub fn estado(&self) -> EstadoDoTeste {
        let m = self.envio.portao();
        EstadoDoTeste {
            nivel_dbfs: seele_voz::gate::dbfs_de_rms(m.input_rms),
            aberto: self.aberto,
            aberturas: m.openings,
            quadros_retidos: m.quadros_retidos_entregues,
            corte_dbfs: m.abertura_efetiva_dbfs,
        }
    }
}

/// Quantas amostras o fade leva para sair do som e para voltar a ele: 2,5 ms.
///
/// O bastante para tirar o estalo de um degrau no meio de uma onda, e pouco
/// para a voz que volta perder o ataque da sílaba.
pub const AMOSTRAS_DO_FADE: usize = 120;

/// Uma fonte que toca: o buffer de jitter dela e de onde o fade parte.
#[derive(Debug)]
struct Fonte {
    buffer: JitterBuffer<Vec<f32>>,
    /// A última amostra que saiu desta fonte.
    ultima: f32,
    /// Se o último quadro desta fonte foi som.
    soando: bool,
}

/// A voz que toca, dentro do `AudioWorklet`: um buffer de jitter por alça e
/// o mixer do desktop.
///
/// Os mesmos do desktop, com a configuração dele (`JitterConfig::default()`),
/// e o mesmo isolamento total: o mestre do mixer em zero, com os buffers
/// andando, para quem volta cair no presente.
#[derive(Debug)]
pub struct Reproducao {
    fontes: BTreeMap<u32, Fonte>,
    mixer: Mixer,
    misturado: Vec<f32>,
    /// Quantas vezes uma fonte saiu do som para o silêncio com fade.
    fades: u64,
    /// Quantos quadros chegaram com outro tamanho que não 20 ms, e foram
    /// acertados com silêncio ou cortados.
    fora_do_quadro: u64,
    /// A janela do relato de recepção, sobre os buffers destas fontes.
    janela: JanelaDeRecepcao,
    /// Quando é a próxima vez do relato, na hora da página. `None` até a
    /// primeira volta, que só marca a partida.
    proximo_relato_ms: Option<f64>,
}

/// O que a voz que toca mede da própria recepção, numa volta da medida.
///
/// São duas contas, com destinos diferentes, como no desktop:
///
/// - `jitter_ms` e `perda` são **o de agora**, para a tela e o sinal desta
///   ponta (`Ligacao::medir`): o pior jitter de chegada (RFC 3550) e a pior
///   perda entre as fontes, a perda contada desde que cada buffer nasceu — as
///   mesmas do `worst_arrival_jitter_ms` e do `worst_loss_fraction` do
///   desktop;
/// - `relato` é **o da janela**, para o servidor
///   (`ClientMessage::RelatoDeRecepcao`): a conta do
///   [`seele_voz::recepcao`], a mesma do desktop, e só na vez dele.
///
/// `None` é «não medido», e nunca zero.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RecepcaoDaVoz {
    /// O pior jitter de chegada entre as fontes, em milissegundos.
    pub jitter_ms: Option<f32>,
    /// A pior perda entre as fontes que já tiveram lugar para tocar, de 0 a 1.
    pub perda: Option<f32>,
    /// O relato desta volta, se é a vez dele e a janela mediu alguma coisa.
    pub relato: Option<seele_proto::signal::Recepcao>,
}

impl RecepcaoDaVoz {
    /// Como a página a lê: `null` no que não foi medido.
    #[must_use]
    pub fn em_json(&self) -> serde_json::Value {
        serde_json::json!({
            "jitter_ms": self.jitter_ms,
            "perda": self.perda,
            "relato": self.relato.map(|relato| serde_json::json!({
                "perda": relato.perda,
                "jitter_ms": relato.jitter_ms,
            })),
        })
    }
}

/// De quanto em quanto tempo a voz que toca solta um relato, em milissegundos:
/// o compasso do protocolo.
#[allow(
    clippy::cast_precision_loss,
    reason = "mil milissegundos cabem exatos num f64"
)]
const INTERVALO_DO_RELATO_MS: f64 = seele_proto::signal::INTERVALO_DO_RELATO_MS as f64;

impl Default for Reproducao {
    fn default() -> Self {
        Self::nova()
    }
}

impl Reproducao {
    /// Sem fonte nenhuma, sem isolamento, os ganhos em um.
    #[must_use]
    pub fn nova() -> Self {
        Self {
            fontes: BTreeMap::new(),
            mixer: Mixer::new(),
            misturado: vec![0.0; FRAME_SAMPLES],
            fades: 0,
            fora_do_quadro: 0,
            janela: JanelaDeRecepcao::nova(),
            proximo_relato_ms: None,
        }
    }

    /// Uma volta da medida da recepção, na hora da página (`agora_ms`).
    ///
    /// O de agora sai a cada chamada. O relato segue a regra do `Relator` do
    /// desktop (`seele-core`, `recepcao`): a primeira vez é um intervalo
    /// depois da primeira chamada, a janela só anda na vez, e a vez seguinte
    /// conta **de agora** — uma página que atrasou não relata em rajada. Sem
    /// voz chegando a janela não mede, e não há relato: nem zero.
    ///
    /// Uma hora que não é número não anda a janela nem solta relato.
    ///
    /// `fora` é a alça que **não é voz** e não entra em nenhuma das contas: o
    /// som da tela, que a página põe na mesma mistura. Ele chega pelo fluxo da
    /// tela, sem número nem carimbo do fio — um fluxo que engasga deixaria
    /// lugares sem quadro, e isso não é perda da voz que se ouve. No desktop
    /// ele também não é fonte da telemetria da voz.
    pub fn recepcao(&mut self, agora_ms: f64, fora: Option<u32>) -> RecepcaoDaVoz {
        let mut medida = RecepcaoDaVoz::default();
        let vozes = || self.fontes.iter().filter(|(alca, _)| Some(**alca) != fora);
        for (_, fonte) in vozes() {
            let m = fonte.buffer.metrics();
            #[allow(
                clippy::cast_possible_truncation,
                reason = "milissegundos e uma fração; f32 é o que o sinal carrega"
            )]
            {
                if m.jitter_ms.is_finite() && m.jitter_ms >= 0.0 {
                    let jitter = m.jitter_ms as f32;
                    medida.jitter_ms =
                        Some(medida.jitter_ms.map_or(jitter, |pior| pior.max(jitter)));
                }
                // Sem lugar nenhum não há fração: é «não medido», e não zero.
                if m.frames_played + m.frames_concealed + m.frames_silenced > 0 {
                    let perda = m.loss_fraction() as f32;
                    medida.perda = Some(medida.perda.map_or(perda, |pior| pior.max(perda)));
                }
            }
        }
        if !agora_ms.is_finite() {
            return medida;
        }
        match self.proximo_relato_ms {
            Some(proximo) if agora_ms >= proximo => {}
            Some(_) => return medida,
            None => {
                self.proximo_relato_ms = Some(agora_ms + INTERVALO_DO_RELATO_MS);
                return medida;
            }
        }
        self.proximo_relato_ms = Some(agora_ms + INTERVALO_DO_RELATO_MS);
        // A alça no lugar do SSRC: é o nome que esta instância tem da fonte, e
        // a janela só precisa que ele seja um por fonte.
        let contagens: Vec<ContagemDaFonte> = vozes()
            .map(|(alca, fonte)| ContagemDaFonte::do_buffer(*alca, &fonte.buffer.metrics()))
            .collect();
        let janela = self.janela.observar(agora_ms, &contagens);
        medida.relato = seele_proto::signal::Recepcao::medida(janela.perda, janela.jitter_ms);
        medida
    }

    /// Um quadro decodificado da fonte `alca`, com o cabeçalho que ele trouxe
    /// e a hora em que o datagrama chegou à página.
    ///
    /// A hora é a da chegada, e não a da decodificação: é a oscilação da rede
    /// que o buffer estima.
    pub fn chegou(
        &mut self,
        alca: u32,
        seq: u16,
        carimbo: u32,
        chegada_ms: f64,
        mut pcm: Vec<f32>,
    ) {
        if pcm.len() != FRAME_SAMPLES {
            self.fora_do_quadro += 1;
            pcm.resize(FRAME_SAMPLES, 0.0);
        }
        self.fontes
            .entry(alca)
            .or_insert_with(|| Fonte {
                buffer: JitterBuffer::new(JitterConfig::default()),
                ultima: 0.0,
                soando: false,
            })
            .buffer
            .push(seq, carimbo, chegada_ms, pcm);
    }

    /// Esquece a fonte `alca`: o buffer e o ganho dela.
    pub fn esquecer(&mut self, alca: u32) {
        self.fontes.remove(&alca);
        self.mixer.forget(alca);
    }

    /// O ganho de quem fala pela alça `alca`. Um é o de origem, zero cala.
    pub fn ganho(&mut self, alca: u32, ganho: f32) {
        self.mixer.set_gain(alca, ganho);
    }

    /// O isolamento total: a mistura inteira em silêncio, com os buffers
    /// andando.
    pub fn isolar(&mut self, isolado: bool) {
        self.mixer.set_master(if isolado { 0.0 } else { 1.0 });
    }

    /// Os próximos 20 ms da mistura.
    ///
    /// Cada fonte anda um quadro no buffer dela. Um quadro que falta, ou o
    /// fim da fala, vira silêncio **com fade** a partir da última amostra que
    /// saiu, e a volta ao som entra com fade também.
    pub fn quadro(&mut self) -> &[f32] {
        let mut tocados: Vec<(u32, Vec<f32>)> = Vec::with_capacity(self.fontes.len());
        for (alca, fonte) in &mut self.fontes {
            match fonte.buffer.tick() {
                Decision::Play(mut pcm) => {
                    if !fonte.soando {
                        for (i, amostra) in pcm.iter_mut().take(AMOSTRAS_DO_FADE).enumerate() {
                            *amostra *= rampa(i + 1);
                        }
                    }
                    fonte.soando = true;
                    fonte.ultima = pcm.last().copied().unwrap_or(0.0);
                    tocados.push((*alca, pcm));
                }
                Decision::Conceal | Decision::Silence | Decision::Comfort | Decision::Starved => {
                    if fonte.soando {
                        fonte.soando = false;
                        self.fades += 1;
                        let mut saida = vec![0.0; FRAME_SAMPLES];
                        for (i, amostra) in saida.iter_mut().take(AMOSTRAS_DO_FADE).enumerate() {
                            *amostra = fonte.ultima * (1.0 - rampa(i + 1));
                        }
                        tocados.push((*alca, saida));
                    }
                }
            }
        }
        let emprestados: Vec<(u32, &[f32])> = tocados
            .iter()
            .map(|(alca, pcm)| (*alca, pcm.as_slice()))
            .collect();
        self.mixer.mix(&emprestados, &mut self.misturado);
        &self.misturado
    }

    /// As contas de cada fonte e da mistura, para quem precisa dizer o que
    /// aconteceu: a página e a telemetria.
    ///
    /// `ocultados` conta o que o buffer pediu para ocultar, e aqui **virou
    /// silêncio com fade**: é o `quadros_ocultados` que o ADR 0057 avisa que
    /// passa a contar outra coisa no navegador.
    #[must_use]
    pub fn contas(&self) -> serde_json::Value {
        let fontes: serde_json::Map<String, serde_json::Value> = self
            .fontes
            .iter()
            .map(|(alca, fonte)| {
                let m = fonte.buffer.metrics();
                (
                    alca.to_string(),
                    serde_json::json!({
                        "recebidos": m.frames_received,
                        "tocados": m.frames_played,
                        "ocultados": m.frames_concealed,
                        "silenciados": m.frames_silenced,
                        "calados": m.frames_comfort,
                        "atrasados": m.late_discards,
                        "transbordados": m.overflow_discards,
                        "repetidos": m.duplicates,
                        "ressincronias": m.resyncs,
                        "alvo_ms": m.target_ms,
                        "profundidade_ms": m.depth_ms,
                        "oscilacao_ms": m.jitter_ms,
                    }),
                )
            })
            .collect();
        let mistura = self.mixer.metrics();
        serde_json::json!({
            "fontes": fontes,
            "fades": self.fades,
            "fora_do_quadro": self.fora_do_quadro,
            "mistura": {
                "pico_antes": mistura.peak_before_clip,
                "pico": mistura.peak_output,
                "cortadas": mistura.clipped_samples,
                "ativas": mistura.active_sources,
            },
        })
    }
}

/// O degrau `i` de [`AMOSTRAS_DO_FADE`], de quase zero a um.
fn rampa(i: usize) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "cento e vinte cabe exato num f32"
    )]
    let fracao = i.min(AMOSTRAS_DO_FADE) as f32 / AMOSTRAS_DO_FADE as f32;
    fracao
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "um teste que não monta o datagrama não tem o que conferir"
)]
mod testes {
    use super::*;

    /// O cabeçalho de um datagrama, `(sequência, carimbo)`.
    fn lido(datagrama: &[u8]) -> (u16, u32) {
        let (cabecalho, _) = MediaHeader::decode(datagrama).expect("um datagrama");
        (cabecalho.seq, cabecalho.timestamp)
    }

    /// **A voz nova da página parte à frente da velha pelo tempo fora, qualquer
    /// que tenha sido a duração da velha** (o CUA de 25/10, e o relato de 15 a
    /// 60 min). Tempo simulado: uma hora são 180 mil quadros, sem esperar.
    /// Com a partida do zero, a de antes, a nova ficava atrás pela vida
    /// inteira da velha, e quem ficou a descartava por esse tempo.
    #[test]
    fn a_voz_nova_da_pagina_parte_a_frente_da_velha_em_15_e_60_min() {
        let opus = [0xAB_u8; 40];
        for minutos in [15_u32, 60] {
            let comeco_ms = 1_792_910_000_000.0;
            let mut velha = Envio::partindo_de(77, comeco_ms);
            for _ in 0..(minutos * 60 * 50) {
                velha.quadro_passou();
            }
            let ultimo = lido(
                &velha
                    .datagrama(velha.carimbo_do_quadro(1, 0), &opus)
                    .expect("o último datagrama da voz velha"),
            );
            let fora_ms = 22_000.0;
            let mut nova =
                Envio::partindo_de(77, comeco_ms + f64::from(minutos) * 60_000.0 + fora_ms);
            let carimbo = nova.quadro_passou();
            let primeiro = lido(
                &nova
                    .datagrama(carimbo, &opus)
                    .expect("o primeiro datagrama da voz nova"),
            );
            let adiante = seele_voz::jitter::ts_delta(primeiro.1, ultimo.1);
            assert!(
                (22 * 48_000..=22 * 48_000 + 2 * 960).contains(&adiante),
                "depois de {minutos} min, a voz nova partiu {adiante} amostras do fim da velha, \
                 e não os 22 s fora"
            );
        }
    }

    /// Uma página antiga, que não manda a hora, chega com NaN: o zero de antes,
    /// e não um pânico.
    #[test]
    fn sem_a_hora_da_pagina_o_relogio_parte_do_zero() {
        assert_eq!(Envio::partindo_de(77, f64::NAN).relogio(), (0, 0));
        assert_eq!(Envio::partindo_de(77, -5.0).relogio(), (0, 0));
    }

    /// **A mesma sequência fixa do desktop** (`seele-core`,
    /// `voice::relogio_gravado`): o navegador manda os mesmos pacotes.
    #[test]
    fn o_envio_manda_os_pacotes_da_sequencia_fixa_do_desktop() {
        let opus = [0xAB_u8; 40];
        let mut envio = Envio::de(77, 65_534, 14_400_000);
        let mut pacotes = Vec::new();
        let mut abre = |envio: &mut Envio, quantos: usize, vazios: &[usize]| {
            envio.quadro_passou();
            for indice in 0..quantos {
                let carimbo = envio.carimbo_do_quadro(quantos, indice);
                let corpo: &[u8] = if vazios.contains(&indice) { &[] } else { &opus };
                if let Ok(datagrama) = envio.datagrama(carimbo, corpo) {
                    pacotes.push(lido(&datagrama));
                }
            }
        };
        envio.quadro_passou(); // calado
        envio.quadro_passou(); // o atraso da supressão
        abre(&mut envio, 3, &[]);
        abre(&mut envio, 1, &[0]);
        envio.quadro_passou(); // mudo
        abre(&mut envio, 1, &[]);
        envio.reabrir(77);
        abre(&mut envio, 2, &[0]);
        assert_eq!(
            pacotes,
            [
                (65_535, 14_400_960),
                (0, 14_401_920),
                (1, 14_402_880),
                (2, 14_405_760),
                (5, 14_408_640),
            ]
        );
        assert_eq!(envio.relogio(), (5, 14_408_640));
    }

    /// **O datagrama é o do protocolo**, com o SSRC de quem manda e o Opus
    /// intacto atrás do cabeçalho.
    #[test]
    fn o_datagrama_e_o_do_protocolo() {
        let mut envio = Envio::novo(0x0102_0304);
        let carimbo = envio.quadro_passou();
        let datagrama = envio.datagrama(carimbo, &[9, 8, 7]).expect("um datagrama");
        assert_eq!(datagrama, [1, 1, 2, 3, 4, 0, 1, 0, 0, 3, 0xC0, 9, 8, 7]);
    }

    fn de(ssrc: u32, seq: u16) -> Vec<u8> {
        let mut datagrama = vec![0_u8; HEADER_LEN + 2];
        MediaHeader {
            ssrc,
            seq,
            timestamp: u32::from(seq) * 960,
        }
        .encode_datagram(&[seq as u8, 0xEE], &mut datagrama)
        .expect("um datagrama");
        datagrama
    }

    /// **A mesma sequência de chegadas do desktop** (`seele-core`,
    /// `voice::fontes_gravadas`), com alças no lugar dos SSRCs.
    #[test]
    fn a_recepcao_separa_por_alca_na_sequencia_fixa_do_desktop() {
        const EU: u32 = 5;
        const ANA: u32 = 7;
        const BIA: u32 = 9;
        let mut recepcao = Recepcao::nova(EU);
        let alca = |r: &mut Recepcao, ssrc: u32, ms: f64| {
            r.chegou(&de(ssrc, 1), ms)
                .expect("um cabeçalho")
                .map(|c| c.alca)
        };
        assert_eq!(alca(&mut recepcao, ANA, 0.0), Some(1));
        assert_eq!(alca(&mut recepcao, BIA, 10.0), Some(2));
        assert_eq!(alca(&mut recepcao, EU, 20.0), None, "a própria voz entrou");
        assert_eq!(alca(&mut recepcao, ANA, 1_000.0), Some(1));
        assert_eq!(recepcao.esquecer_caladas(30_005.0), Vec::<u32>::new());
        assert_eq!(
            recepcao.esquecer_caladas(30_020.0),
            [2],
            "a Bia não foi esquecida"
        );
        assert_eq!(
            alca(&mut recepcao, BIA, 30_030.0),
            Some(3),
            "a Bia voltou com a alça velha"
        );
        assert_eq!(recepcao.esquecer_caladas(31_005.0), [1]);
        assert_eq!(recepcao.alca_de(BIA), Some(3));
        assert_eq!(recepcao.alca_de(ANA), None);
    }

    /// O que chega atravessa inteiro, e um datagrama torto é recusado.
    #[test]
    fn o_que_chega_atravessa_e_o_torto_e_recusado() {
        let mut recepcao = Recepcao::nova(5);
        let datagrama = de(7, 300);
        let chegado = recepcao
            .chegou(&datagrama, 0.0)
            .expect("lido")
            .expect("de alguém");
        assert_eq!(
            chegado,
            Chegado {
                alca: 1,
                seq: 300,
                carimbo: 288_000,
                opus: &[44, 0xEE],
            }
        );
        assert!(recepcao.chegou(&[1, 0, 0, 0, 7], 0.0).is_err());
    }

    // ------------------------------------------------- o portão do envio

    const CALADO: [f32; FRAME_SAMPLES] = [0.0; FRAME_SAMPLES];
    const FALA: [f32; FRAME_SAMPLES] = [0.3; FRAME_SAMPLES];

    fn carimbos(quadros: &[(u32, Vec<f32>)]) -> Vec<u32> {
        quadros.iter().map(|(carimbo, _)| *carimbo).collect()
    }

    /// **A primeira sílaba sai com o instante de cada quadro**, como no laço
    /// do desktop: na abertura, os dois quadros retidos vão na frente, cada um
    /// carimbado 20 ms antes do seguinte, e o relógio andou nos quadros em que
    /// nada saiu.
    #[test]
    fn a_abertura_solta_os_retidos_com_o_carimbo_de_cada_um() {
        let mut envio = Envio::novo(77);
        envio.modo(GateMode::VoiceActivated);
        assert!(envio.capturou(&CALADO, false).is_empty());
        assert!(envio.capturou(&CALADO, false).is_empty());
        let saiu = envio.capturou(&FALA, false);
        assert_eq!(carimbos(&saiu), [960, 1_920, 2_880]);
        assert_eq!(saiu.get(2).map(|(_, q)| q.as_slice()), Some(&FALA[..]));
        assert_eq!(
            carimbos(&envio.capturou(&FALA, false)),
            [3_840],
            "aberto, sai um quadro por quadro"
        );
    }

    /// **No mudo o portão anda e nada sai**, e o relógio também anda: o
    /// medidor mostra que há fala, e quem recebe vê um silêncio, e não perda.
    #[test]
    fn no_mudo_o_portao_e_o_relogio_andam_e_nada_sai() {
        let mut envio = Envio::novo(77);
        envio.modo(GateMode::Open);
        assert!(envio.capturou(&FALA, true).is_empty());
        assert!(
            envio.portao().input_rms > 0.2,
            "o medidor parou no mudo: {:?}",
            envio.portao()
        );
        assert_eq!(envio.relogio(), (0, 960));
        assert_eq!(carimbos(&envio.capturou(&FALA, false)), [1_920]);
    }

    /// **Um `reabrir` monta outro portão com os mesmos controles.** A régua
    /// escolhida, o modo e a tecla atravessam; é a metade da voz do guarda dos
    /// controles na reconexão (Step 1 da Tarefa 3.3).
    #[test]
    fn reabrir_guarda_a_regua_o_modo_e_a_tecla() {
        let mut envio = Envio::novo(77);
        envio.regua(-50_000, true);
        envio.modo(GateMode::PushToTalk);
        envio.tecla(true);
        envio.reabrir(78);
        assert_eq!(
            envio.capturou(&CALADO, false).len(),
            1,
            "a tecla segurada não atravessou o reabrir"
        );
        assert!(
            (envio.portao().abertura_efetiva_dbfs - (-50.0)).abs() < 0.01,
            "a régua escolhida não atravessou o reabrir: {:?}",
            envio.portao()
        );

        envio.modo(GateMode::Open);
        envio.reabrir(79);
        assert_eq!(
            envio.capturou(&CALADO, false).len(),
            1,
            "o modo aberto não atravessou o reabrir"
        );
    }

    // ------------------------------------------------ o teste do microfone

    /// **O teste é o portão da voz na ativação por voz**: o silêncio não abre,
    /// a fala abre com os retidos, e o estado diz o nível, a abertura, as
    /// contas e o corte, como o do desktop.
    #[test]
    fn o_teste_do_microfone_conta_o_que_o_portao_da_voz_decide() {
        let mut teste = TesteDoMicrofone::abrir(0, true);
        assert!(teste.quadro(&CALADO).is_empty());
        assert!(teste.quadro(&CALADO).is_empty());
        let parado = teste.estado();
        assert!(!parado.aberto);
        assert_eq!(parado.aberturas, 0, "o silêncio abriu o portão do teste");

        let saem = teste.quadro(&FALA);
        assert_eq!(saem.len(), 3, "a abertura do teste não levou os retidos");
        let falando = teste.estado();
        assert!(falando.aberto);
        assert_eq!(falando.aberturas, 1);
        assert_eq!(falando.quadros_retidos, 2);
        assert!(
            (falando.nivel_dbfs - seele_voz::gate::dbfs_de_rms(0.3)).abs() < 0.01,
            "o nível não é o do quadro que o portão viu: {falando:?}"
        );
        assert!(
            (falando.corte_dbfs - seele_voz::gate::ABERTURA_COM_SUPRESSAO_DBFS).abs() < 0.01,
            "o corte não é o da régua com supressão: {falando:?}"
        );
    }

    /// A régua escolhida vale no teste como vale na conversa: fixa.
    #[test]
    fn o_teste_usa_a_regua_escolhida() {
        let mut teste = TesteDoMicrofone::abrir(-30_000, false);
        let _ = teste.quadro(&[0.01; FRAME_SAMPLES]);
        assert!(
            (teste.estado().corte_dbfs - (-30.0)).abs() < 0.01,
            "{:?}",
            teste.estado()
        );
        assert!(!teste.estado().aberto, "−40 dBFS abriu um corte em −30");
    }

    // ---------------------------------------------------- a reprodução

    /// O quadro `k` de uma fonte: todas as amostras em `0,01 × (k + 1)`.
    fn quadro_numero(k: u16) -> Vec<f32> {
        vec![0.01 * f32::from(k + 1); FRAME_SAMPLES]
    }

    /// Toca `quantos` quadros da alça 1, um por tique como o worklet, sem os
    /// de `perdidos`; com `isolado` ligado entre os tiques de `isolamento`.
    fn tocar(quantos: u16, perdidos: &[u16], isolamento: std::ops::Range<u16>) -> Vec<Vec<f32>> {
        let mut reproducao = Reproducao::nova();
        let mut saidas = Vec::new();
        for k in 0..quantos + 6 {
            if k < quantos && !perdidos.contains(&k) {
                reproducao.chegou(
                    1,
                    k,
                    u32::from(k) * 960,
                    f64::from(k) * 20.0,
                    quadro_numero(k),
                );
            }
            reproducao.isolar(isolamento.contains(&k));
            saidas.push(reproducao.quadro().to_vec());
        }
        saidas
    }

    /// O meio de cada quadro que saiu, em centésimos: o número dele mais um,
    /// ou zero para silêncio.
    fn meios(saidas: &[Vec<f32>]) -> Vec<u16> {
        saidas
            .iter()
            .map(|q| {
                let meio = q.get(FRAME_SAMPLES / 2).copied().unwrap_or(f32::NAN);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "centésimos de um quadro de teste, entre 0 e 100"
                )]
                let centesimos = (meio * 100.0).round() as u16;
                centesimos
            })
            .collect()
    }

    // ------------------------------------------------ a recepção medida

    /// Uma pessoa falando pela alça 1 por `segundos`, um tique de 20 ms por
    /// quadro como o worklet, com a medida pedida a cada `volta_ms` como a
    /// página a pede. `chega(k)` diz em que tique o quadro `k` chega, ou
    /// `None` se ele se perdeu. Devolve cada volta da medida, com a hora dela.
    fn ouvir(
        reproducao: &mut Reproducao,
        de_ms: f64,
        segundos: u16,
        volta_ms: u16,
        chega: impl Fn(u16) -> Option<u16>,
    ) -> Vec<(f64, RecepcaoDaVoz)> {
        let quadros = segundos * 50;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a hora de um teste, em quadros"
        )]
        let primeiro = (de_ms / 20.0) as u16;
        let mut voltas = Vec::new();
        for tique in 0..quadros {
            // Os quadros que chegam neste tique, na ordem em que chegam: o de
            // número maior primeiro, que é a reordenação.
            for k in (0..quadros).rev() {
                if chega(k) == Some(tique) {
                    let seq = primeiro.wrapping_add(k);
                    reproducao.chegou(
                        1,
                        seq,
                        u32::from(seq) * 960,
                        de_ms + f64::from(tique) * 20.0,
                        quadro_numero(k % 50),
                    );
                }
            }
            let _ = reproducao.quadro();
            let agora = de_ms + f64::from(tique + 1) * 20.0;
            if (tique + 1) % (volta_ms / 20) == 0 {
                voltas.push((agora, reproducao.recepcao(agora, None)));
            }
        }
        voltas
    }

    fn relatos(voltas: &[(f64, RecepcaoDaVoz)]) -> Vec<(f64, seele_proto::signal::Recepcao)> {
        voltas
            .iter()
            .filter_map(|(quando, volta)| volta.relato.map(|relato| (*quando, relato)))
            .collect()
    }

    /// **Sem nada chegando, nada é medido — e nada é dividido por zero**: nem a
    /// voz sem fonte, nem a fonte cujo primeiro quadro acabou de chegar e
    /// ainda não teve lugar para tocar. O que sai é «não medido», e o JSON da
    /// página leva `null`, nunca `0` nem `NaN`.
    #[test]
    fn sem_nada_chegando_nada_e_medido_nem_dividido_por_zero() {
        let mut reproducao = Reproducao::nova();
        for segundo in 0..8_u16 {
            let volta = reproducao.recepcao(f64::from(segundo) * 1000.0, None);
            assert_eq!(volta, RecepcaoDaVoz::default(), "no segundo {segundo}");
        }
        assert_eq!(
            reproducao.recepcao(9_000.0, None).em_json().to_string(),
            r#"{"jitter_ms":null,"perda":null,"relato":null}"#
        );
        // Uma fonte que chegou e ainda não teve tique nenhum: zero lugares.
        reproducao.chegou(1, 0, 0, 10_000.0, quadro_numero(0));
        let volta = reproducao.recepcao(10_000.0, None);
        assert_eq!(volta.perda, None, "zero lugares não é zero de perda");
        assert_eq!(volta.relato, None);
        assert!(volta.jitter_ms.is_some_and(f32::is_finite));
    }

    /// **A perda relatada é a do que faltou**: um quadro perdido em cada dez
    /// vira dez por cento, na unidade do protocolo (uma fração de 0 a 1), com
    /// o jitter em milissegundos, e o relato passa na conferência que o
    /// servidor faz.
    #[test]
    fn um_quadro_perdido_em_dez_vira_dez_por_cento_no_relato() {
        let mut reproducao = Reproducao::nova();
        let voltas = ouvir(&mut reproducao, 0.0, 8, 250, |k| (k % 10 != 5).then_some(k));
        let relatos = relatos(&voltas);
        let (_, ultimo) = relatos
            .last()
            .copied()
            .expect("com fala bastante há relato");
        assert_eq!(ultimo.confere(), Ok(()));
        let perda = ultimo.perda.expect("a perda da janela");
        assert!(
            (perda - 0.1).abs() < 0.02,
            "relatou {perda}, e faltaram 10%"
        );
        let jitter = ultimo.jitter_ms.expect("o jitter da janela");
        assert!((0.0..5.0).contains(&jitter), "jitter de {jitter} ms");
        // O de agora, para a tela, mede a mesma voz.
        let (_, volta) = voltas.last().copied().expect("a última volta");
        assert!(
            volta.perda.is_some_and(|p| (p - 0.1).abs() < 0.02),
            "{volta:?}"
        );
        assert!(
            volta.jitter_ms.is_some_and(|j| (0.0..5.0).contains(&j)),
            "{volta:?}"
        );
    }

    /// **Reordenação não é perda**: os quadros chegam aos pares trocados (o de
    /// número maior antes), nenhum falta, e o relato diz zero de perda — e um
    /// jitter que deixou de ser zero, porque a chegada oscilou.
    #[test]
    fn quadros_fora_de_ordem_nao_contam_como_perda() {
        let mut reproducao = Reproducao::nova();
        // O par (2n, 2n+1) chega junto, no tique do segundo, com o segundo na
        // frente.
        let voltas = ouvir(&mut reproducao, 0.0, 8, 250, |k| Some(k | 1));
        let (_, ultimo) = relatos(&voltas)
            .last()
            .copied()
            .expect("com fala bastante há relato");
        let perda = ultimo.perda.expect("a perda da janela");
        assert!(perda < 0.01, "a reordenação virou {perda} de perda");
        assert!(
            ultimo.jitter_ms.is_some_and(|j| j > 1.0),
            "os pares chegam juntos, e o jitter ficou em {:?}",
            ultimo.jitter_ms
        );
    }

    /// **A janela desliza**: quatro segundos ruins no começo saem do relato
    /// depois de cinco segundos limpos. A perda de agora, que entra no sinal
    /// desta ponta como no desktop, é a contada desde que o buffer nasceu, e
    /// não esquece.
    #[test]
    fn a_perda_velha_sai_da_janela_do_relato() {
        let mut reproducao = Reproducao::nova();
        let ruins = ouvir(&mut reproducao, 0.0, 4, 250, |k| (k % 5 != 2).then_some(k));
        let (_, no_ruim) = relatos(&ruins).last().copied().expect("o relato do começo");
        assert!(
            no_ruim.perda.is_some_and(|p| (p - 0.2).abs() < 0.04),
            "{no_ruim:?}"
        );
        let limpos = ouvir(&mut reproducao, 4_000.0, 8, 250, Some);
        let (_, depois) = relatos(&limpos).last().copied().expect("o relato do fim");
        assert_eq!(
            depois.perda,
            Some(0.0),
            "a perda de 8 s atrás ficou na janela"
        );
        let (_, volta) = limpos.last().copied().expect("a última volta");
        assert!(volta.perda.is_some_and(|p| p > 0.03), "{volta:?}");
    }

    /// **Um relato por segundo, e nunca em rajada**: a página pede a medida
    /// quatro vezes por segundo, e o relato sai uma. A primeira volta só marca
    /// a partida; uma página que atrasou dez segundos não manda dois seguidos;
    /// uma hora que não é número não solta nada.
    #[test]
    fn o_relato_sai_uma_vez_por_segundo_e_nunca_em_rajada() {
        let mut reproducao = Reproducao::nova();
        let voltas = ouvir(&mut reproducao, 0.0, 8, 250, Some);
        let relatos = relatos(&voltas);
        // A janela pede dois segundos de fala; daí em diante, um por segundo.
        assert!(
            (4..=6).contains(&relatos.len()),
            "{} relatos em 8 s",
            relatos.len()
        );
        for par in relatos.windows(2) {
            let [(antes, _), (depois, _)] = par else {
                continue;
            };
            assert!(
                depois - antes >= 1000.0,
                "relatos a {} ms um do outro",
                depois - antes
            );
        }
        // Dez segundos sem pedir, com a voz tocando: uma volta, um relato.
        for _ in 0..500 {
            let _ = reproducao.quadro();
        }
        let tarde = reproducao.recepcao(18_000.0, None);
        let logo = reproducao.recepcao(18_010.0, None);
        assert_eq!(logo.relato, None, "depois de {:?}", tarde.relato);
        assert_eq!(reproducao.recepcao(f64::NAN, None).relato, None);
        assert_eq!(reproducao.recepcao(f64::INFINITY, None).relato, None);
    }

    /// **O som da tela não é voz**: a alça dele, que a página diz qual é, fica
    /// fora das duas contas, por pior que chegue.
    #[test]
    fn o_som_da_tela_nao_entra_na_medida() {
        const TELA: u32 = u32::MAX;
        let mut reproducao = Reproducao::nova();
        let mut ultima = RecepcaoDaVoz::default();
        let mut com_a_tela = None;
        for k in 0..400_u16 {
            reproducao.chegou(
                1,
                k,
                u32::from(k) * 960,
                f64::from(k) * 20.0,
                quadro_numero(k % 50),
            );
            // O som da tela perde um quadro em cada dois.
            if k % 2 == 0 {
                reproducao.chegou(
                    TELA,
                    k,
                    u32::from(k) * 960,
                    f64::from(k) * 20.0,
                    quadro_numero(k % 50),
                );
            }
            let _ = reproducao.quadro();
            if (k + 1) % 50 == 0 {
                let agora = f64::from(k + 1) * 20.0;
                ultima = reproducao.recepcao(agora, Some(TELA));
                if ultima.relato.is_some() {
                    com_a_tela = ultima.relato;
                }
            }
        }
        assert_eq!(ultima.perda, Some(0.0), "{ultima:?}");
        assert_eq!(com_a_tela.and_then(|relato| relato.perda), Some(0.0));
        // Sem dizer qual é a alça da tela, a perda dela entraria.
        let mut tudo = reproducao.recepcao(9_000.0, None);
        assert!(tudo.perda.is_some_and(|p| p > 0.3), "{tudo:?}");
        tudo.relato = None;
        assert_ne!(tudo, ultima);
    }

    /// **Sem voz, o relato para**: a fonte esquecida sai da janela com o que
    /// tinha, e nada é relatado no lugar dela — nem zero.
    #[test]
    fn a_fonte_esquecida_nao_e_mais_relatada() {
        let mut reproducao = Reproducao::nova();
        let voltas = ouvir(&mut reproducao, 0.0, 4, 250, Some);
        assert!(!relatos(&voltas).is_empty());
        reproducao.esquecer(1);
        for segundo in 5..10_u16 {
            let volta = reproducao.recepcao(f64::from(segundo) * 1000.0, None);
            assert_eq!(volta, RecepcaoDaVoz::default(), "no segundo {segundo}");
        }
    }

    /// **Os quadros saem em ordem**, depois da espera do buffer, e sem fade no
    /// meio da fala.
    #[test]
    fn a_reproducao_toca_os_quadros_em_ordem() {
        let tocados: Vec<u16> = meios(&tocar(10, &[], 0..0))
            .into_iter()
            .filter(|c| *c != 0)
            .collect();
        assert_eq!(tocados, (1..=10).collect::<Vec<u16>>());
    }

    /// **Um quadro perdido vira silêncio com fade** (a emenda do ADR 0057 ao
    /// ADR 0010): a saída desce da última amostra a zero em 2,5 ms, e a volta
    /// sobe de zero, sem degrau maior que um passo da rampa.
    #[test]
    fn um_quadro_perdido_vira_silencio_com_fade() {
        let saidas = tocar(10, &[4], 0..0);
        let tocados = meios(&saidas);
        assert!(!tocados.contains(&5), "o quadro perdido tocou: {tocados:?}");
        let continuo: Vec<f32> = saidas.concat();
        let inicio = tocados.iter().position(|c| *c != 0).unwrap_or(0) * FRAME_SAMPLES;
        let fim = (tocados.iter().rposition(|c| *c != 0).unwrap_or(0) + 1) * FRAME_SAMPLES;
        let pior = continuo
            .get(inicio + 1..fim)
            .unwrap_or_default()
            .iter()
            .zip(continuo.get(inicio..fim).unwrap_or_default())
            .map(|(depois, antes)| (depois - antes).abs())
            .fold(0.0_f32, f32::max);
        // O maior degrau legítimo é o de um quadro de teste para o seguinte
        // (0,01). Sem o fade, o buraco teria um degrau de 0,04 para zero.
        assert!(
            pior <= 0.0101,
            "um degrau de {pior} no buraco do quadro perdido: {tocados:?}"
        );
        let silencio = tocados
            .iter()
            .position(|c| *c == 4)
            .map(|i| i + 1)
            .and_then(|i| saidas.get(i))
            .expect("o quadro depois do 4");
        assert!(
            (silencio.first().copied().unwrap_or(0.0) - 0.04 * (1.0 - 1.0 / 120.0)).abs() < 1e-6,
            "o fade não partiu da última amostra"
        );
        assert!(
            silencio.iter().skip(AMOSTRAS_DO_FADE).all(|a| *a == 0.0),
            "o fade não terminou em silêncio"
        );
    }

    /// **O isolamento cala e não para o relógio**: os buffers andam, e quem
    /// volta cai no presente, e não onde parou.
    #[test]
    fn o_isolamento_cala_sem_parar_os_buffers() {
        let livre = meios(&tocar(12, &[], 0..0));
        let isolado = meios(&tocar(12, &[], 4..8));
        for (tique, (l, i)) in livre.iter().zip(&isolado).enumerate() {
            let isolado_aqui = (4..8).contains(&u16::try_from(tique).unwrap_or(0));
            if isolado_aqui {
                assert_eq!(*i, 0, "o isolamento deixou som sair no tique {tique}");
            } else if tique > 8 {
                assert_eq!(l, i, "depois do isolamento a voz ficou atrasada");
            }
        }
    }

    /// **O ganho é por alça**: zero cala quem fala por ela, e não os outros;
    /// esquecer a alça leva o buffer e o ganho.
    #[test]
    fn o_ganho_e_por_alca_e_esquecer_leva_os_dois() {
        let mut reproducao = Reproducao::nova();
        reproducao.ganho(1, 0.0);
        for k in 0..6_u16 {
            reproducao.chegou(
                1,
                k,
                u32::from(k) * 960,
                f64::from(k) * 20.0,
                vec![0.2; FRAME_SAMPLES],
            );
            reproducao.chegou(
                2,
                k,
                u32::from(k) * 960,
                f64::from(k) * 20.0,
                vec![0.05; FRAME_SAMPLES],
            );
            let _ = reproducao.quadro();
        }
        let meio = reproducao.quadro().get(FRAME_SAMPLES / 2).copied();
        assert_eq!(
            meio,
            Some(0.05),
            "o ganho zero da alça 1 não a calou, ou calou a 2"
        );
        reproducao.esquecer(1);
        assert!(
            reproducao.contas().pointer("/fontes/1").is_none(),
            "a alça esquecida continua nas contas"
        );
        reproducao.chegou(1, 0, 0, 200.0, vec![0.2; FRAME_SAMPLES]);
        assert!(
            (reproducao.mixer.gain(1) - 1.0).abs() < f32::EPSILON,
            "a alça esquecida voltou com o ganho velho"
        );
    }

    /// Um quadro de outro tamanho é acertado e contado, e não derruba nada.
    #[test]
    fn um_quadro_de_outro_tamanho_e_acertado_e_contado() {
        let mut reproducao = Reproducao::nova();
        reproducao.chegou(1, 0, 0, 0.0, vec![0.1; 480]);
        assert_eq!(
            reproducao.contas().pointer("/fora_do_quadro"),
            Some(&serde_json::json!(1))
        );
    }
}
