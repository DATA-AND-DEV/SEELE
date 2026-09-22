//! Ouvir o próprio microfone antes de entrar numa conversa.
//!
//! F01 de `docs/features-v15.md`: «mostrar o nível do microfone e quando a
//! transmissão abre; permitir testar e ouvir o resultado antes de entrar numa
//! conversa».
//!
//! # Por que ele não é uma sessão de voz
//!
//! `Voice` precisa de um `MediaChannel` e de um `Ssrc` — quer dizer, de um
//! servidor. A pergunta que este módulo responde é anterior a haver servidor:
//! *este microfone funciona, e o que sai dele soa como quê?* Fazer dela uma
//! sessão obrigaria a pessoa a entrar em algum lugar para descobrir que o
//! microfone estava mudo, que é a ordem errada.
//!
//! # O que ele exercita, e por que isso importa
//!
//! O **mesmo caminho** de uma conversa, menos o codec e a rede:
//! `captura → supressão → portão → ganho → saída`. Um teste que ouvisse o
//! microfone cru responderia a pergunta errada: a fala que o portão descarta não
//! aparece numa conversa, e é justamente ela que F01 existe para recuperar. Quem
//! calibra a sensibilidade precisa ouvir **o que os outros ouviriam**.
//!
//! O que fica de fora é dito por extenso: o Opus não passa por aqui, e nem a
//! rede. Um artefato de codec ou um corte de jitter não aparecem neste teste, e
//! este módulo não promete que apareçam.
//!
//! # Retorno acústico
//!
//! Tocar o microfone no alto-falante realimenta. É o que a pessoa pediu — ela
//! quer ouvir — e é por isso que [`TesteDeMicrofone::set_monitorar`] existe e
//! começa **desligada**: o nível e a marca de abertura respondem a maior parte
//! da pergunta sem som nenhum, e quem quiser ouvir liga, de preferência de fone.
//! A interface diz isso ao lado do controle.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use seele_audio::device;

use crate::voice::DeviceChoice;
use seele_audio::gate::{GateConfig, GateMode, VoiceGate};
use seele_audio::supressao::Supressao;
use seele_audio::{FRAME_SAMPLES, SAMPLE_RATE_HZ};

/// Quanta folga os anéis do dispositivo guardam, em milissegundos.
///
/// O mesmo de uma sessão de voz: o que se mede aqui tem de ser o que acontece lá.
const FOLGA_MS: u32 = 120;

/// O que este teste tem a contar enquanto roda.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EstadoDoTeste {
    /// O nível do microfone **depois** da supressão, em dBFS.
    ///
    /// Depois, e não antes: é o nível que o portão vê, e é sobre ele que a
    /// sensibilidade é escolhida. Mostrar o nível cru faria a pessoa calibrar
    /// contra um número que não decide nada.
    pub nivel_dbfs: f32,
    /// Se a transmissão estaria aberta agora.
    ///
    /// É a metade que F01 pede junto do nível: um medidor sozinho não diz onde
    /// está o corte, e é o corte que a pessoa está ajustando.
    pub aberto: bool,
    /// Quantas vezes o portão abriu desde que o teste começou.
    ///
    /// O número que responde «o ventilador está abrindo a transmissão?» sem
    /// ninguém precisar ouvir: com a sala vazia, ele tem de ficar parado.
    pub aberturas: u64,
    /// Quantos quadros anteriores a uma abertura foram entregues junto com ela.
    ///
    /// A retenção da primeira sílaba, em número. Ver
    /// `seele_audio::gate::GateMetrics::quadros_retidos_entregues`.
    pub quadros_retidos: u64,
    /// Onde o corte **está**, em dBFS, e não onde a régua foi posta. F01, A03.
    ///
    /// No padrão o limiar acompanha o ruído medido — ver
    /// `seele_audio::gate::MARGEM_SOBRE_O_RUIDO_DB` —, então a régua mostra o alvo
    /// e o portão decide por outro número. Sem este campo o medidor desenharia a
    /// marca no alvo enquanto o corte está 10 dB acima, e quem está tentando
    /// descobrir por que o microfone não abre veria a barra passar de uma marca que
    /// não é a que decide. É o «o produto sabe e não conta» do `CLAUDE.md` na forma
    /// que engana justamente quem foi investigar.
    pub corte_dbfs: f32,
}

/// Um teste de microfone em curso.
///
/// Largar a alça encerra o teste: o [`Drop`] manda parar e espera a thread, do
/// mesmo jeito que a `Bomba` do vídeo — um caminho de erro que esqueça de parar
/// não pode deixar um microfone aberto.
#[derive(Debug)]
pub struct TesteDeMicrofone {
    controles: Arc<Controles>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug)]
struct Controles {
    parar: AtomicBool,
    monitorar: AtomicBool,
    /// O nível em milésimos de dBFS, deslocado para caber num `u32`.
    ///
    /// Deslocado porque dBFS é negativo e não há átomo de `f32`. O deslocamento
    /// é [`DESLOCAMENTO_DO_NIVEL`], e a conversão mora nos dois lados desta
    /// mesma estrutura — um número cru atravessando seria um número que alguém lê
    /// sem desfazer o deslocamento.
    nivel: AtomicU32,
    aberto: AtomicBool,
    aberturas: std::sync::atomic::AtomicU64,
    retidos: std::sync::atomic::AtomicU64,
    /// O corte que está valendo, em milésimos de dBFS deslocados como o nível.
    corte: AtomicU32,
    /// A força da supressão e a sensibilidade, relidas a cada volta.
    supressao: AtomicU32,
    abertura_dbfs_milesimos: std::sync::atomic::AtomicI32,
    /// O que deu errado, se deu.
    falha: Mutex<Option<String>>,
}

/// Quanto o nível é deslocado antes de caber num átomo sem sinal.
///
/// 200 dB de folga: dBFS de áudio real não chega perto disso nem em silêncio
/// digital, e o valor fica positivo em toda a faixa.
const DESLOCAMENTO_DO_NIVEL: f32 = 200.0;

impl TesteDeMicrofone {
    /// Abre o microfone e começa a medir.
    ///
    /// `escolha` é o mesmo par de aparelhos de uma sessão. A saída é aberta junto
    /// mesmo com o monitor desligado: ligá-lo no meio exigiria abrir um
    /// dispositivo com o teste rodando, e abrir um dispositivo leva quase um
    /// segundo — o controle pareceria ter emperrado.
    ///
    /// # Errors
    ///
    /// Falha quando não há microfone, quando o aparelho escolhido sumiu, ou
    /// quando o sistema recusa a configuração.
    pub fn abrir(
        escolha: &DeviceChoice,
        supressao: f32,
        abertura_dbfs: Option<f32>,
    ) -> Result<Self> {
        let io = device::open(escolha.wanted(), FOLGA_MS)?;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "os dois valores são fixados antes da conversão"
        )]
        let controles = Arc::new(Controles {
            parar: AtomicBool::new(false),
            // **Desligado**, e a escolha está no cabeçalho do módulo: tocar o
            // microfone no alto-falante realimenta, e o nível responde a maior
            // parte da pergunta sem som nenhum.
            monitorar: AtomicBool::new(false),
            nivel: AtomicU32::new(0),
            aberto: AtomicBool::new(false),
            aberturas: std::sync::atomic::AtomicU64::new(0),
            retidos: std::sync::atomic::AtomicU64::new(0),
            corte: AtomicU32::new(0),
            supressao: AtomicU32::new((supressao.clamp(0.0, 1.0) * 1000.0) as u32),
            abertura_dbfs_milesimos: std::sync::atomic::AtomicI32::new(
                abertura_dbfs.map_or(0, |dbfs| (dbfs * 1000.0) as i32),
            ),
            falha: Mutex::new(None),
        });

        let para_a_thread = Arc::clone(&controles);
        let thread = std::thread::Builder::new()
            .name("seele-teste-de-microfone".into())
            .spawn(move || laco(io, &para_a_thread))?;

        Ok(Self {
            controles,
            thread: Some(thread),
        })
    }

    /// O que o teste tem a contar agora.
    #[must_use]
    pub fn estado(&self) -> EstadoDoTeste {
        #[allow(
            clippy::cast_precision_loss,
            reason = "milésimos de decibel cabem exatos num f32 nesta faixa"
        )]
        let nivel =
            self.controles.nivel.load(Ordering::Relaxed) as f32 / 1000.0 - DESLOCAMENTO_DO_NIVEL;
        #[allow(
            clippy::cast_precision_loss,
            reason = "milésimos de decibel cabem exatos num f32 nesta faixa"
        )]
        let corte =
            self.controles.corte.load(Ordering::Relaxed) as f32 / 1000.0 - DESLOCAMENTO_DO_NIVEL;
        EstadoDoTeste {
            nivel_dbfs: nivel,
            aberto: self.controles.aberto.load(Ordering::Relaxed),
            aberturas: self.controles.aberturas.load(Ordering::Relaxed),
            quadros_retidos: self.controles.retidos.load(Ordering::Relaxed),
            corte_dbfs: corte,
        }
    }

    /// Liga ou desliga ouvir o próprio microfone.
    ///
    /// Desligado é o padrão — ver o cabeçalho do módulo sobre realimentação.
    pub fn set_monitorar(&self, ligado: bool) {
        self.controles.monitorar.store(ligado, Ordering::Relaxed);
    }

    /// Se o monitor está ligado.
    #[must_use]
    pub fn monitorando(&self) -> bool {
        self.controles.monitorar.load(Ordering::Relaxed)
    }

    /// Troca a força da supressão sem reabrir o aparelho. F02.
    pub fn set_supressao(&self, forca: f32) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "o valor é fixado entre 0 e 1 antes da conversão"
        )]
        let milesimos = (forca.clamp(0.0, 1.0) * 1000.0) as u32;
        self.controles.supressao.store(milesimos, Ordering::Relaxed);
    }

    /// Troca a sensibilidade sem reabrir o aparelho. F01.
    ///
    /// **É o ponto inteiro deste módulo:** arrastar o controle e ouvir a
    /// diferença na hora. Reabrir o aparelho a cada passo do controle daria um
    /// segundo de silêncio por passo.
    pub fn set_abertura_dbfs(&self, dbfs: Option<f32>) {
        let milesimos = dbfs.map_or(0, |valor| {
            let fixado = valor.clamp(
                *seele_audio::gate::FAIXA_DE_ABERTURA_DBFS.start(),
                *seele_audio::gate::FAIXA_DE_ABERTURA_DBFS.end(),
            );
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a faixa é de −72 a −24 dBFS; em milésimos cabe folgado num i32"
            )]
            let convertido = (fixado * 1000.0) as i32;
            convertido
        });
        self.controles
            .abertura_dbfs_milesimos
            .store(milesimos, Ordering::Relaxed);
    }

    /// O que deu errado durante o teste, se deu.
    ///
    /// **Não é o mesmo que falhar ao abrir**: aquilo é o `Err` de
    /// [`Self::abrir`]. Isto é um aparelho que sumiu no meio, e sem esta linha o
    /// sintoma seria um medidor parado — indistinguível de silêncio.
    #[must_use]
    pub fn falha(&self) -> Option<String> {
        self.controles.falha.lock().ok().and_then(|f| f.clone())
    }
}

impl Drop for TesteDeMicrofone {
    fn drop(&mut self) {
        self.controles.parar.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// O laço do teste: captura, trata, mede e — se pedirem — toca de volta.
fn laco(mut io: seele_audio::device::AudioIo, controles: &Arc<Controles>) {
    // Um reamostrador que não abre é o fim do teste, e ele é dito: um medidor
    // parado é indistinguível de silêncio, que é o defeito desta casa.
    let (mut para_48k, mut para_o_aparelho) = match (
        seele_audio::resample::RateConverter::new(io.capture_rate_hz, SAMPLE_RATE_HZ),
        seele_audio::resample::RateConverter::new(SAMPLE_RATE_HZ, io.playback_rate_hz),
    ) {
        (Ok(entrada), Ok(saida)) => (entrada, saida),
        (entrada, saida) => {
            let erro = entrada
                .err()
                .map(|e| e.to_string())
                .or_else(|| saida.err().map(|e| e.to_string()))
                .unwrap_or_else(|| "reamostrador recusado".to_owned());
            if let Ok(mut falha) = controles.falha.lock() {
                *falha = Some(format!(
                    "este aparelho não abriu para o teste ({erro}); tente outro \
                     microfone ou outra saída"
                ));
            }
            return;
        }
    };
    let mut gate = VoiceGate::new(config(controles), GateMode::VoiceActivated);
    let mut supressao = Supressao::nova(forca(controles));
    let mut ganho = seele_audio::ganho::Ganho::novo();

    let (mut capturado, mut em_48k, mut pendente) = (Vec::new(), Vec::new(), Vec::<f32>::new());
    let (mut limpo, mut a_transmitir, mut para_fora) = (Vec::new(), Vec::new(), Vec::new());

    while !controles.parar.load(Ordering::Relaxed) {
        gate.ajustar(config(controles));
        supressao.ajustar(forca(controles));

        capturado.clear();
        while let Ok(amostra) = io.captured.pop() {
            capturado.push(amostra);
        }
        em_48k.clear();
        if para_48k.push(&capturado, &mut em_48k).is_ok() {
            pendente.extend_from_slice(&em_48k);
        }

        while pendente.len() >= FRAME_SAMPLES {
            let bruto: Vec<f32> = pendente.drain(..FRAME_SAMPLES).collect();
            // **A mesma ordem da conversa.** Ver o cabeçalho: um teste que
            // medisse o microfone cru responderia a pergunta errada.
            supressao.processar(&bruto, &mut limpo);
            if limpo.is_empty() {
                continue;
            }
            let quadro: Vec<f32> = std::mem::take(&mut limpo);
            let nivel = seele_audio::gate::dbfs_de_rms(VoiceGate::rms(&quadro));
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "o nível é deslocado para ficar positivo antes da conversão"
            )]
            let guardado = ((nivel + DESLOCAMENTO_DO_NIVEL).max(0.0) * 1000.0) as u32;
            controles.nivel.store(guardado, Ordering::Relaxed);

            gate.quadros_a_transmitir(&quadro, &mut a_transmitir);
            let aberto = !a_transmitir.is_empty();
            controles.aberto.store(aberto, Ordering::Relaxed);
            let medidas = gate.metrics();
            controles
                .aberturas
                .store(medidas.openings, Ordering::Relaxed);
            controles
                .retidos
                .store(medidas.quadros_retidos_entregues, Ordering::Relaxed);
            // **O corte que decidiu este quadro.** Ver `EstadoDoTeste::corte_dbfs`.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "o corte é deslocado para ficar positivo antes da conversão"
            )]
            let corte =
                ((medidas.abertura_efetiva_dbfs + DESLOCAMENTO_DO_NIVEL).max(0.0) * 1000.0) as u32;
            controles.corte.store(corte, Ordering::Relaxed);

            if !aberto || !controles.monitorar.load(Ordering::Relaxed) {
                a_transmitir.clear();
                continue;
            }
            for mut quadro in std::mem::take(&mut a_transmitir) {
                ganho.aplicar(&mut quadro);
                para_fora.clear();
                if para_o_aparelho.push(&quadro, &mut para_fora).is_ok() {
                    for amostra in para_fora.drain(..) {
                        // Um anel cheio aqui é o monitor atrasando, e não um
                        // defeito: a amostra some e a próxima chega em dia. Contar
                        // isto seria contar uma coisa que não muda decisão nenhuma.
                        let _ = io.to_device.push(amostra);
                    }
                }
            }
        }

        // O mesmo compasso do laço de voz: uma volta por quadro.
        std::thread::sleep(std::time::Duration::from_millis(u64::from(
            seele_audio::FRAME_MS,
        )));
    }
}

/// A régua do portão que os controles do teste descrevem.
///
/// A mesma decisão de `voice::configuracao_do_portao`, e a mesma razão: o padrão
/// **acompanha o ruído medido** e um valor escolhido à mão fica onde foi posto. Um
/// teste que calibrasse com outra régua daria confiança num número que a conversa
/// não usa — é o que `a_regua_do_teste_e_a_mesma_da_conversa` guarda.
fn config(controles: &Controles) -> GateConfig {
    let escolhido = controles.abertura_dbfs_milesimos.load(Ordering::Relaxed);
    let dbfs = if escolhido == 0 {
        if controles.supressao.load(Ordering::Relaxed) > 0 {
            seele_audio::gate::ABERTURA_COM_SUPRESSAO_DBFS
        } else {
            seele_audio::gate::ABERTURA_SEM_SUPRESSAO_DBFS
        }
    } else {
        #[allow(
            clippy::cast_precision_loss,
            reason = "milésimos de decibel cabem exatos num f32 nesta faixa"
        )]
        let convertido = escolhido as f32 / 1000.0;
        // **Fixo**, porque foi escolhido.
        return GateConfig::de_dbfs(convertido);
    };
    GateConfig::automatico(dbfs)
}

/// A força da supressão que os controles do teste pedem.
fn forca(controles: &Controles) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "mil milésimos cabem exatos num f32"
    )]
    let forca = controles.supressao.load(Ordering::Relaxed) as f32 / 1000.0;
    forca
}

#[cfg(test)]
mod testes {
    // Sem `use super::*`: os três guardas abaixo leem texto-fonte e não tocam em
    // nada deste módulo. Um import que não é usado é um aviso, e a CI trata aviso
    // como erro.

    /// **A régua do teste é a mesma da conversa.**
    ///
    /// Se as duas divergirem, alguém calibra a sensibilidade num teste e entra
    /// numa sala com outro corte — e o teste passa a ser pior que não ter teste,
    /// porque ele dá confiança num número errado.
    ///
    /// Guarda de texto-fonte: a regra é «estas duas funções decidem igual», e não
    /// há tipo que a expresse. As duas são curtas e a comparação é do corpo delas.
    #[test]
    fn a_regua_do_teste_e_a_mesma_da_conversa() {
        let daqui = include_str!("teste_de_microfone.rs");
        let da_voz = include_str!("voice.rs");
        for pedaco in [
            "ABERTURA_COM_SUPRESSAO_DBFS",
            "ABERTURA_SEM_SUPRESSAO_DBFS",
            // O padrão acompanha o ruído; a escolha manual fica onde foi posta.
            // Os dois caminhos precisam ser os mesmos nos dois lugares — ver a
            // auditoria A03 em `gate::MARGEM_SOBRE_O_RUIDO_DB`.
            "GateConfig::automatico(dbfs)",
            "GateConfig::de_dbfs(convertido)",
        ] {
            assert!(
                daqui.contains(pedaco),
                "o teste de microfone deixou de usar `{pedaco}`: ele passou a \
                 calibrar com uma régua que a conversa não usa"
            );
            assert!(
                da_voz.contains(pedaco),
                "a conversa deixou de usar `{pedaco}`, e o teste continua usando"
            );
        }
    }

    /// **E a ordem do caminho também é a mesma.** F01 e F02.
    ///
    /// Um teste que ouvisse o microfone cru responderia a pergunta errada: a fala
    /// que o portão descarta não aparece numa conversa, e é justamente ela que
    /// F01 existe para recuperar.
    #[test]
    fn o_teste_exercita_o_mesmo_caminho_da_conversa() {
        let fonte = include_str!("teste_de_microfone.rs");
        let supressao = fonte
            .find("supressao.processar(&bruto, &mut limpo)")
            .expect("a supressão sumiu do teste de microfone");
        let portao = fonte
            .find("gate.quadros_a_transmitir(&quadro, &mut a_transmitir)")
            .expect("o portão sumiu do teste de microfone");
        let ganho = fonte
            .find("ganho.aplicar(&mut quadro)")
            .expect("o ganho sumiu do teste de microfone");
        assert!(supressao < portao && portao < ganho);
    }

    /// O monitor começa desligado. Ver o cabeçalho do módulo.
    ///
    /// Guarda de texto-fonte porque abrir o aparelho pede placa de som, e o que
    /// se prende aqui é a escolha — não o comportamento do `cpal`.
    #[test]
    fn o_monitor_comeca_desligado() {
        let fonte = include_str!("teste_de_microfone.rs");
        assert!(
            fonte.contains("monitorar: AtomicBool::new(false)"),
            "o teste de microfone passou a tocar no alto-falante sem ninguém \
             pedir, e isso realimenta"
        );
    }
}
