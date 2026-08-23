//! A cola: captura → codificador → teto.
//!
//! `seele-video` sabe tirar pixels da tela e transformá-los em H.264, e
//! [`crate::tela`] sabe quanto disso cabe no fio. **Nenhum dos dois chamava o
//! outro**, e o relatório da onda 1 registrou a falta com todas as letras:
//! *«`seele-core` não depende de `seele-video`, então nada liga captura →
//! encoder → `ajustar_teto` ainda»*. Este módulo é essa ligação, e a aresta que
//! ele usa é a que o [ADR 0002](../../../docs/adr/0002-regra-de-dependencia.md)
//! já permitia — `core` vê `proto`, `audio` e `video`.
//!
//! # Por que a decisão mora deste lado, e não dentro do codec
//!
//! Porque o §3.2 a pôs aqui: o teto é uma fração do caminho medido, pendurada
//! no sinal que a **voz** calcula, e a voz é conta de `seele-core`. O
//! codificador só sabe obedecer a um número — se ele decidisse quanto pode
//! gastar, o produto teria dois medidores discordando no primeiro dia ruim, que
//! é exatamente o que a regra 2 do §3.2 existe para impedir. O comentário do
//! `xtask check-deps` sobre `seele-video` diz a mesma coisa do outro lado: *«se
//! um dia ele precisar de `seele-core`, quer dizer que a decisão de o que
//! transmitir migrou para dentro do codec»*.
//!
//! # O que esta cola **não** faz
//!
//! Está escrito aqui e não num relatório porque é o que alguém precisa saber
//! antes de chamá-la:
//!
//! - **não abre fluxo e não escreve byte nenhum na rede.** Ela devolve
//!   [`QuadroCodificado`], e quem o entrega a [`crate::tela::Transmissao`] é
//!   quem tem a conexão. Juntar as duas coisas poria o codificador — que o §2
//!   manda morar numa thread própria, com prioridade abaixo do normal — dentro
//!   do runtime que carrega os datagramas de voz;
//! - **não cria thread.** Quem escolhe a thread e a prioridade é quem chama, e
//!   o §2 diz qual: própria, abaixo do normal, e **nunca** perto do caminho de
//!   áudio;
//! - **não troca a resolução sozinha.** Ela diz que o degrau mudou
//!   ([`Ajuste::ResolucaoPedida`]) e para por aí. A resolução vai no cabeçalho
//!   de abertura do fluxo (§3.6), então trocá-la é reabrir o fluxo — e isso é
//!   decisão de quem é dono da transmissão, não de uma cola;
//! - **não mede o caminho.** Continua a pergunta 2 do §8, e o padrão continua
//!   sendo o cano das provas.

use seele_video::codec::{
    Cadencia, Codificador, ConfigDoCodificador, QuadroCodificado, QuadroI420, Resolucao,
};
use seele_video::{BibliotecaDeVideo, ErroDeVideo};
use thiserror::Error;

use crate::tela::{menor_resolucao, MotivoDeParada, Teto, TetoDeVideo};
use seele_proto::sync_ratio::SyncBand;

/// De onde os quadros vêm.
///
/// Existe porque as duas capturas do §1 têm a mesma forma e nomes diferentes —
/// `CapturaDaTela::tomar` no macOS, `Captura::pegar` no Windows — e porque uma
/// cola que só compilasse num dos dois sistemas seria metade de uma cola. As
/// implementações estão logo abaixo, atrás de `cfg`, e o resto deste módulo não
/// sabe em que sistema está.
///
/// **`&self` e não `&mut self`, de propósito.** Quem escreve é a thread do
/// sistema operacional e quem lê é a thread do codificador; as duas capturas já
/// resolvem isso por dentro, com a vaga de uma posição só do §1. Pedir `&mut`
/// aqui obrigaria quem chama a pôr um cadeado em cima de algo que já é seguro.
///
/// # `None` não é morte
///
/// É estado normal, e o relatório do Windows mediu por quê: **a WGC só entrega
/// quadro quando a tela muda** — 2,0 a 2,9 quadros por segundo numa área de
/// trabalho parada, sem erro nenhum. Quem transmite não pode ler «nada chegou»
/// como «a captura morreu».
pub trait FonteDeQuadros {
    /// O quadro mais novo, ou `None` se nenhum chegou desde a última chamada.
    ///
    /// **Nunca uma fila.** A regra do §1 é da captura e as duas a cumprem: um
    /// quadro que chega e encontra a vaga ocupada substitui o que estava lá. Um
    /// quadro velho entregue tarde é pior que um quadro perdido.
    fn tomar(&self) -> Option<QuadroI420>;
}

#[cfg(target_os = "macos")]
impl FonteDeQuadros for seele_video::captura::macos::CapturaDaTela {
    fn tomar(&self) -> Option<QuadroI420> {
        // O instante da captura fica para trás aqui, e é uma perda de verdade:
        // é com ele que se mede a **idade** do quadro que o codificador pegou,
        // que é a grandeza com que `spikes/tela-no-codec` decidiu entre
        // enfileirar e descartar. Quem quiser medi-la chama `tomar` do tipo
        // concreto; esta ponte carrega só o que as duas capturas têm em comum,
        // e a do Windows não carimba o instante.
        Self::tomar(self).map(|da_tela| da_tela.quadro)
    }
}

#[cfg(target_os = "windows")]
impl FonteDeQuadros for seele_video::captura::windows::Captura {
    fn tomar(&self) -> Option<QuadroI420> {
        self.pegar()
    }
}

/// Por que o compartilhamento não anda.
#[derive(Debug, Error)]
pub enum ErroDeCompartilhamento {
    /// O teto disse para não transmitir. **Não é falha**: é o §3.2 respondendo
    /// que agora não dá, com o motivo que a interface escreve na língua da
    /// pessoa.
    #[error("the screen share cannot run: {0}")]
    Parado(#[source] MotivoDeParada),
    /// O codec ou o módulo do Cisco recusaram.
    #[error(transparent)]
    Video(#[from] ErroDeVideo),
}

/// O que saiu de um tique.
///
/// Três respostas e não um `Option`, porque «não veio quadro da tela» e «o
/// controle de taxa pulou este quadro» são coisas diferentes que a interface
/// mostra diferente — e um `None` para as duas ensinaria quem chama a tratá-las
/// igual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Passo {
    /// A captura não tinha quadro novo. Estado normal — ver [`FonteDeQuadros`].
    SemQuadro,
    /// O controle de taxa do OpenH264 pulou este quadro para não estourar o
    /// teto.
    ///
    /// **Não é perda e não é erro.** No teto de 1200 kbps que a voz permite são
    /// 16,2% dos quadros em 1080p e 11,1% em 720p, medidos em duas máquinas. É
    /// exatamente o caso para o qual o §5 obriga a tela a mostrar o que está
    /// saindo ao lado do que foi pedido.
    PuladoPeloTeto,
    /// Saiu um quadro, pronto para [`crate::tela::Transmissao::enviar_quadro`].
    Quadro(QuadroCodificado),
}

/// O que mudou quando o teto andou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ajuste {
    /// Nada mudou.
    Igual,
    /// Só o teto de banda mudou, e o codificador continua o mesmo.
    ///
    /// É um `SetOption` e não uma reconstrução, ao contrário do que a voz faz
    /// em `seele_audio::codec::VoiceEncoder::set_bitrate`: aqui refazer o
    /// encoder custaria um quadro-chave inteiro, que são 65 KiB e 446 ms do
    /// orçamento de 1200 kbps.
    TetoNovo {
        /// O teto que passou a valer.
        teto_bps: u32,
    },
    /// O degrau de resolução mudou, e o codificador **continua no antigo**.
    ///
    /// A resolução vai no cabeçalho de abertura do fluxo (§3.6), então trocá-la
    /// no meio é reabrir o fluxo com outro cabeçalho — e quem faz isso é o dono
    /// da transmissão. Esta cola avisa e obedece ao teto de banda mesmo assim,
    /// que é a parte que não pode esperar.
    ResolucaoPedida {
        /// O degrau em uso.
        de: Resolucao,
        /// O degrau que o teto de agora compraria.
        para: Resolucao,
        /// O teto que passou a valer.
        teto_bps: u32,
    },
    /// O vídeo parou, com motivo (§3.2).
    Parou(MotivoDeParada),
}

/// O que o teto manda o codificador fazer, sem tocar em codificador nenhum.
///
/// Separado do resto para poder ser conferido sem o módulo do Cisco na
/// máquina — é a decisão inteira, e ela é aritmética.
///
/// `escolha` é a resolução que a pessoa pediu, e ela é **teto** (§5): o degrau
/// que sai daqui é o menor entre o que ela pediu e o que o orçamento compra.
/// `None` quando o teto mandou parar: não há configuração para uma transmissão
/// que não acontece.
#[must_use]
pub fn config_para(
    teto: Teto,
    escolha: Resolucao,
    cadencia: Cadencia,
) -> Option<ConfigDoCodificador> {
    let degrau = teto.resolucao_estimada()?;
    Some(ConfigDoCodificador {
        resolucao: menor_resolucao(degrau, escolha),
        cadencia,
        teto_bps: teto.bps(),
    })
}

/// Uma transmissão de tela viva, do lado de quem compartilha.
///
/// Guarda o codificador, o teto que está valendo e a escolha da pessoa, e é a
/// única coisa deste crate que conhece os dois lados.
///
/// É `Send` porque o §2 manda o codificador morar numa thread própria, com
/// prioridade abaixo do normal, e **nunca** no runtime que carrega os
/// datagramas de voz. Não é `Sync`, e isso é certo: dois lados codificando no
/// mesmo encoder embaralhariam a predição.
#[derive(Debug)]
pub struct Compartilhamento {
    biblioteca: BibliotecaDeVideo,
    codificador: Codificador,
    escolha_de_resolucao: Resolucao,
    teto: Teto,
}

impl Compartilhamento {
    /// Arma o codificador para o teto de agora.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Parado`] quando o teto disse para não
    /// transmitir — o sinal está crítico ou o que sobrou não sustenta nem o
    /// piso. [`ErroDeCompartilhamento::Video`] quando o OpenH264 recusa a
    /// configuração.
    pub fn abrir(
        biblioteca: BibliotecaDeVideo,
        teto_de_video: &TetoDeVideo,
        faixa: SyncBand,
        escolha_de_resolucao: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Self, ErroDeCompartilhamento> {
        let teto = teto_de_video.teto(faixa);
        let config = match (teto, config_para(teto, escolha_de_resolucao, cadencia)) {
            (_, Some(config)) => config,
            (Teto::Parado(motivo), None) => return Err(ErroDeCompartilhamento::Parado(motivo)),
            // `config_para` só devolve `None` para um teto parado, e um teto
            // parado só tem essa forma. Este braço existe porque o compilador
            // não sabe disso e porque um `unwrap` aqui seria proibido pelo
            // `forbid` do workspace — e com razão.
            (Teto::Bps(_), None) => {
                return Err(ErroDeCompartilhamento::Parado(MotivoDeParada::AbaixoDoPiso))
            }
        };
        let codificador = Codificador::novo(&biblioteca, config)?;
        Ok(Self {
            biblioteca,
            codificador,
            escolha_de_resolucao,
            teto,
        })
    }

    /// O teto que está valendo.
    #[must_use]
    pub const fn teto(&self) -> Teto {
        self.teto
    }

    /// A resolução com que o codificador está armado — a que **está saindo**, e
    /// não a que foi pedida.
    ///
    /// A diferença é a regra do §5: *a tela não promete a escolha*. Quem mostra
    /// «o que está saindo agora ao lado do que foi pedido» lê este número de um
    /// lado e [`Self::escolha_de_resolucao`] do outro.
    #[must_use]
    pub const fn resolucao(&self) -> Resolucao {
        self.codificador.resolucao()
    }

    /// A resolução que a pessoa pediu, que é teto e nunca piso (§5).
    #[must_use]
    pub const fn escolha_de_resolucao(&self) -> Resolucao {
        self.escolha_de_resolucao
    }

    /// Reage a um teto novo: aplica a banda e diz se o degrau mudou.
    ///
    /// **Isto é o §3.2 e o §5.1 virando código.** O teto muda quando o sinal da
    /// voz cai de faixa e também quando alguém entra ou sai da sala — a perna
    /// de quem hospeda é dividida pelo número de espectadores —, e as duas
    /// coisas chegam aqui pelo mesmo caminho, porque N já está dentro do teto.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Video`] se o OpenH264 recusar a banda nova.
    /// Um teto que mandou parar **não** é erro: volta como [`Ajuste::Parou`],
    /// porque parar com motivo é uma resposta do produto e não uma falha.
    pub fn ajustar(
        &mut self,
        teto_de_video: &TetoDeVideo,
        faixa: SyncBand,
    ) -> Result<Ajuste, ErroDeCompartilhamento> {
        let novo = teto_de_video.teto(faixa);
        self.teto = novo;
        let bps = match novo {
            Teto::Parado(motivo) => return Ok(Ajuste::Parou(motivo)),
            Teto::Bps(bps) => bps,
        };

        let mudou_a_banda = bps != self.codificador.teto_bps();
        if mudou_a_banda {
            self.codificador.ajustar_teto(bps)?;
        }

        let degrau = menor_resolucao(
            novo.resolucao_estimada()
                .unwrap_or(self.escolha_de_resolucao),
            self.escolha_de_resolucao,
        );
        if degrau != self.codificador.resolucao() {
            return Ok(Ajuste::ResolucaoPedida {
                de: self.codificador.resolucao(),
                para: degrau,
                teto_bps: bps,
            });
        }
        if mudou_a_banda {
            return Ok(Ajuste::TetoNovo { teto_bps: bps });
        }
        Ok(Ajuste::Igual)
    }

    /// Refaz o codificador num degrau novo, depois de o fluxo ter sido reaberto.
    ///
    /// Custa um quadro-chave inteiro — 65 KiB, 446 ms do orçamento de 1200 kbps
    /// —, e é por isso que [`Self::ajustar`] não faz isto sozinho: um degrau que
    /// oscila entre dois valores queimaria o orçamento em quadros-chave e não
    /// mostraria tela nenhuma.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Video`] se o OpenH264 recusar a configuração.
    pub fn refazer_com(&mut self, resolucao: Resolucao) -> Result<(), ErroDeCompartilhamento> {
        let config = ConfigDoCodificador {
            resolucao,
            cadencia: self.codificador.cadencia(),
            teto_bps: self.teto.bps(),
        };
        self.codificador = Codificador::novo(&self.biblioteca, config)?;
        Ok(())
    }

    /// Um tique: pega o quadro mais novo da captura e codifica.
    ///
    /// `pedido_de_chave` é o §3.3 — quadro-chave **quando quem recebe pede**, e
    /// não de tempos em tempos.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Video`] com
    /// `ErroDeVideo::QuadroDeTamanhoErrado` quando a captura entrega um quadro
    /// de outro tamanho — que é o que acontece se alguém trocar o degrau sem
    /// reconfigurar a captura, e é um erro nomeado justamente para não virar um
    /// borrão sem explicação.
    pub fn passo(
        &mut self,
        fonte: &impl FonteDeQuadros,
        pedido_de_chave: bool,
    ) -> Result<Passo, ErroDeCompartilhamento> {
        let Some(quadro) = fonte.tomar() else {
            return Ok(Passo::SemQuadro);
        };
        match self.codificador.codificar(&quadro, pedido_de_chave)? {
            Some(codificado) => Ok(Passo::Quadro(codificado)),
            None => Ok(Passo::PuladoPeloTeto),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use seele_video::modulo;

    use super::*;
    use crate::tela::{CAMINHO_DA_PROVA_BPS, PISO_DE_BANDA_BPS};

    /// Uma captura de mentira, para provar a cola sem uma tela na frente.
    ///
    /// A regra do §1 não é imitada aqui de propósito: quem descarta é a
    /// captura de verdade, e uma imitação que descartasse provaria a imitação.
    #[derive(Debug, Default)]
    struct FonteDeMentira {
        quadros: Mutex<Vec<QuadroI420>>,
    }

    impl FonteDeMentira {
        fn com(quadros: Vec<QuadroI420>) -> Self {
            Self {
                quadros: Mutex::new(quadros),
            }
        }
    }

    impl FonteDeQuadros for FonteDeMentira {
        fn tomar(&self) -> Option<QuadroI420> {
            self.quadros.lock().ok()?.pop()
        }
    }

    /// Onde procurar o módulo do Cisco, na ordem: o que quem roda apontou,
    /// depois a pasta de build.
    fn pastas() -> Vec<PathBuf> {
        let mut pastas = Vec::new();
        if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
            let caminho = PathBuf::from(apontado);
            pastas.push(if caminho.is_dir() {
                caminho
            } else {
                caminho.parent().map_or(caminho.clone(), PathBuf::from)
            });
        }
        pastas.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target"),
        );
        pastas
    }

    /// A biblioteca, ou `None` com o motivo impresso.
    ///
    /// **Pula em vez de falhar, e o motivo é a licença**, o mesmo de
    /// `seele-video/tests/ida_e_volta.rs`: o módulo do Cisco não pode morar
    /// neste repositório, e um teste que o exigisse seria vermelho em toda
    /// máquina limpa — um teste sempre vermelho é um teste que todo mundo
    /// aprende a ignorar.
    fn biblioteca() -> Option<BibliotecaDeVideo> {
        match BibliotecaDeVideo::procurar_e_carregar(&pastas()) {
            Ok(biblioteca) => Some(biblioteca),
            Err(motivo) => {
                let onde = modulo::publicado_para_este_sistema()
                    .map_or_else(|| "—".to_owned(), |m| m.url());
                eprintln!(
                    "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe \
                     isso.\n  Busque {onde} e aponte-o com SEELE_OPENH264."
                );
                None
            }
        }
    }

    /// Um quadro com bordas duras, que é o conteúdo caro de uma tela de
    /// trabalho. Um quadro chapado sairia com trinta bytes e não provaria nada.
    fn quadro(resolucao: Resolucao, passo: usize) -> QuadroI420 {
        let (largura, altura) = (resolucao.largura(), resolucao.altura());
        let mut y = Vec::with_capacity(largura * altura);
        for linha in 0..altura {
            for coluna in 0..largura {
                let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
                y.push(if claro { 235 } else { 16 });
            }
        }
        let croma = vec![128; largura.div_ceil(2) * altura.div_ceil(2)];
        QuadroI420::novo(largura, altura, y, croma.clone(), croma)
            .expect("os planos de um I420 montado aqui")
    }

    #[test]
    fn o_teto_decide_a_configuracao_do_codificador_sem_precisar_de_codificador() {
        // A decisão inteira é aritmética, e esta é ela: o degrau sai do teto
        // (§5.1) e a escolha da pessoa fica por cima como teto (§5).
        let fibra = Teto::Bps(4_000_000);
        let config = config_para(fibra, Resolucao::P1080, Cadencia::Q30)
            .expect("um teto de 4 Mbps compra alguma coisa");
        assert_eq!(config.resolucao, Resolucao::P1080);
        assert_eq!(config.teto_bps, 4_000_000);

        // A mesma fibra, com quem escolheu 540p: continua 540p.
        let modesto = config_para(fibra, Resolucao::P540, Cadencia::Q8)
            .expect("um teto de 4 Mbps compra alguma coisa");
        assert_eq!(modesto.resolucao, Resolucao::P540);
        assert_eq!(modesto.cadencia, Cadencia::Q8);

        // E o teto apertado não obedece a quem pediu 1080p.
        let apertado = config_para(Teto::Bps(500_000), Resolucao::P1080, Cadencia::Q30)
            .expect("500 kbps ainda compram 540p");
        assert_eq!(apertado.resolucao, Resolucao::P540);

        // Parado não tem configuração: não há como armar um codificador para
        // uma transmissão que o §3.2 acabou de recusar.
        assert!(config_para(
            Teto::Parado(MotivoDeParada::SinalCritico),
            Resolucao::P720,
            Cadencia::Q30
        )
        .is_none());
    }

    #[test]
    fn a_sala_que_cresce_aperta_o_codificador_e_nao_a_voz() {
        // O caminho completo do §5.1, e é o que esta cola existe para fazer:
        // alguém entra na sala → a perna de quem hospeda é dividida por mais um
        // → o teto cai → o codificador obedece. Sem o codificador na mão o
        // teste ainda prova a metade que decide.
        let Some(biblioteca) = biblioteca() else {
            return;
        };

        // Uma casa que hospeda com 6 Mbps de subida, sozinha na sala.
        let sozinho = TetoDeVideo::com_caminho(6_000_000)
            .com_caminho_de_quem_hospeda(6_000_000)
            .com_espectadores(1);
        let mut compartilhamento = Compartilhamento::abrir(
            biblioteca,
            &sozinho,
            SyncBand::Nominal,
            Resolucao::P1080,
            Cadencia::Q30,
        )
        .expect("armar o codificador com 3,6 Mbps de teto");
        assert_eq!(compartilhamento.resolucao(), Resolucao::P1080);
        assert_eq!(compartilhamento.teto(), Teto::Bps(3_600_000));

        // Entra a segunda pessoa: 3,6 Mbps ÷ 2 = 1,8, que ainda compra 1080p.
        let a_dois = sozinho.com_espectadores(2);
        assert_eq!(
            compartilhamento
                .ajustar(&a_dois, SyncBand::Nominal)
                .expect("baixar a banda do codificador"),
            Ajuste::TetoNovo {
                teto_bps: 1_800_000
            }
        );
        assert_eq!(compartilhamento.resolucao(), Resolucao::P1080);

        // Entra a terceira: 1,2 Mbps, e aí o degrau cai — é a linha do §5.1,
        // «a 1200 kbps o 1080p joga fora um sexto do que captura».
        let a_tres = sozinho.com_espectadores(3);
        assert_eq!(
            compartilhamento
                .ajustar(&a_tres, SyncBand::Nominal)
                .expect("baixar a banda do codificador"),
            Ajuste::ResolucaoPedida {
                de: Resolucao::P1080,
                para: Resolucao::P720,
                teto_bps: 1_200_000,
            }
        );
        // E o codificador **continua** em 1080p até alguém reabrir o fluxo: a
        // resolução mora no cabeçalho de abertura (§3.6), e trocá-la por baixo
        // faria quem recebe decodificar lixo.
        assert_eq!(compartilhamento.resolucao(), Resolucao::P1080);

        compartilhamento
            .refazer_com(Resolucao::P720)
            .expect("refazer o codificador em 720p");
        assert_eq!(compartilhamento.resolucao(), Resolucao::P720);

        // E a voz nunca cedeu: os 40% da subida desta casa continuam de pé em
        // toda a escada acima.
        for espectadores in [1, 2, 3] {
            assert_eq!(
                sozinho.com_espectadores(espectadores).reserva_da_voz(),
                2_400_000
            );
        }
    }

    #[test]
    fn a_cola_vai_da_captura_ao_quadro_codificado() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };

        let teto = TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS);
        let mut compartilhamento = Compartilhamento::abrir(
            biblioteca,
            &teto,
            SyncBand::Nominal,
            Resolucao::P720,
            Cadencia::Q30,
        )
        .expect("armar o codificador no cano da prova");

        // Uma captura sem quadro novo não é uma captura morta (§ a WGC só
        // entrega quando a tela muda).
        let vazia = FonteDeMentira::default();
        assert_eq!(
            compartilhamento
                .passo(&vazia, false)
                .expect("um tique sem quadro"),
            Passo::SemQuadro
        );

        // E com quadro, sai H.264 de verdade: o primeiro é chave, com SPS e PPS
        // na frente, que é o que faz quem recebe conseguir abrir o fluxo.
        let fonte =
            FonteDeMentira::com(vec![quadro(Resolucao::P720, 8), quadro(Resolucao::P720, 0)]);
        let Passo::Quadro(primeiro) = compartilhamento
            .passo(&fonte, true)
            .expect("codificar o primeiro quadro")
        else {
            panic!("o primeiro quadro tinha de sair, e sair como chave");
        };
        assert!(primeiro.chave, "o primeiro quadro de um fluxo é chave");
        assert!(
            primeiro.bytes.starts_with(&[0, 0, 0, 1]),
            "Annex-B começa com um código de início"
        );
        assert!(!primeiro.bytes.is_empty());
    }

    #[test]
    fn um_teto_parado_nao_arma_codificador_nenhum() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        // §3.2: quem para é o vídeo, com motivo. Armar um codificador para
        // depois não usá-lo seria gastar memória para dizer «não».
        let teto = TetoDeVideo::com_caminho(PISO_DE_BANDA_BPS);
        let erro = Compartilhamento::abrir(
            biblioteca,
            &teto,
            SyncBand::Nominal,
            Resolucao::P720,
            Cadencia::Q30,
        )
        .expect_err("120 kbps estão abaixo do piso");
        assert!(matches!(
            erro,
            ErroDeCompartilhamento::Parado(MotivoDeParada::AbaixoDoPiso)
        ));
    }
}
