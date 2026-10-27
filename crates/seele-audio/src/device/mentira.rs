//! **O aparelho de mentira**: um microfone e uma saída que não são placa de som
//! nenhuma. Só com o recurso `aparelho-de-mentira`, e só para teste.
//!
//! # Por que ele existe
//!
//! A pendência 61 pediu uma medida que não dava para fazer: «a voz reabre
//! debaixo da tela de fim?». Medir pela FFI abria o microfone de verdade da
//! máquina de quem mede, e o aparelho de mentira que havia (`supervisor.rs`) é
//! uma peça do ciclo de reabertura, que não chega à FFI. Sem ele, o que se sabia
//! da voz numa queda era lido no código, não medido.
//!
//! # Como ele entra
//!
//! Pelo **mesmo** [`super::open`] de sempre, quando o id pedido de um dos lados
//! começa com [`PREFIXO`]. Tudo acima dele é o produto: a porta da voz da FFI, o
//! `Voice::start_preferring`, a reabertura na volta (`Voice::reopen`), a
//! reabertura do laço, o reamostrador, o codificador e o canal de mídia. Só os
//! dois retornos do `cpal` são trocados por uma linha que faz o que eles fazem:
//! entrega um bloco de captura e pede um bloco de saída a cada 10 ms, pelos
//! mesmos [`CaptureSink::on_capture`] e [`PlaybackSource::on_playback`].
//!
//! **O `cpal` nunca é tocado**, nem para enumerar: o id de mentira é decidido
//! antes de o host existir. E os dois lados são de mentira juntos, mesmo que só
//! um tenha sido pedido assim — uma metade de verdade seria um aparelho desta
//! máquina aberto por um teste.
//!
//! # O que um teste lê
//!
//! [`contagem`], por nome: quantos aparelhos com aquele nome estão abertos agora
//! e quantas vezes um abriu. É o que diz «a voz fechou» (zero abertos) e «a voz
//! reabriu» (uma abertura a mais) sem olhar nenhum estado da FFI — o aparelho é
//! a última coisa que fecha.
//!
//! E [`Contagem::blocos_com_som`], que diz «a saída tocou alguma coisa»: a volta
//! da voz, medida no último elo antes do alto-falante.
//!
//! # O modo de pulso (a medida do atraso de ponta a ponta)
//!
//! O tom contínuo não tem começo para marcar. Com `SEELE_MENTIRA_PULSO=<tom>/<período>`
//! (em ms, múltiplos de 10, por exemplo `200/2000`), o microfone de mentira dá
//! o tom por `<tom>` ms a cada `<período>` ms, e silêncio digital no resto; o
//! começo de cada pulso sai no STDERR como uma linha JSON
//! (`{"pulso":n,"utc_us":...,"nome":...}`), com a hora UTC do emissor no
//! instante em que o bloco do começo é entregue à captura. Quem recebe compara
//! o começo que ouve com essa hora. Um valor que não se lê derruba a abertura,
//! alto: uma medida sem pulsos não pode passar por uma medida.
//!
//! [`CaptureSink::on_capture`]: crate::rt::CaptureSink::on_capture
//! [`PlaybackSource::on_playback`]: crate::rt::PlaybackSource::on_playback

use std::collections::BTreeMap;
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::{AudioIo, CaptureDevice, PlaybackDevice, Vivos, Wanted};
use crate::rt::{capacity_for_ms, capture_path, playback_path, StreamCounters};
use crate::SAMPLE_RATE_HZ;

// O recurso nunca entra num build otimizado: um id com o prefixo abriria um
// aparelho que não ouve ninguém, e a pessoa falaria para o nada.
#[cfg(not(debug_assertions))]
compile_error!(
    "o recurso `aparelho-de-mentira` do seele-audio é só de teste e não pode ir num build \
     otimizado: com ele, um id de aparelho com o prefixo abre um microfone que não ouve nada"
);

/// O começo do id que abre o aparelho de mentira em vez de um de verdade.
///
/// O resto do id é o **nome** dele, que separa um teste do outro em
/// [`contagem`].
pub const PREFIXO: &str = "seele-mentira:";

/// O bloco que a linha entrega e pede a cada volta: 10 ms a 48 kHz.
const BLOCO: usize = 480;

/// A cadência da linha, a de um retorno de `cpal` com o bloco acima.
const CADENCIA: Duration = Duration::from_millis(10);

/// A frequência do tom que o microfone de mentira «ouve».
const TOM_HZ: f32 = 440.0;

/// A amplitude do tom: alta o bastante para abrir qualquer portão de voz.
const AMPLITUDE: f32 = 0.3;

/// A variável que liga o modo de pulso: ver o doc do módulo.
pub const VARIAVEL_DO_PULSO: &str = "SEELE_MENTIRA_PULSO";

/// O compasso do modo de pulso, em blocos de [`CADENCIA`].
///
/// **Não é periódico**, de propósito: um compasso fixo de 2 s deixaria quem
/// recebe parear um começo com o pulso errado sempre que o atraso passasse de
/// um período, sem como notar. O intervalo entre um começo e o seguinte varia,
/// por uma sequência determinística ([`Pulso::intervalo`]), entre ¾ e 1¼ do
/// período pedido; a sequência dos intervalos ouvidos identifica, sozinha, quais
/// pulsos são — sem relógio comum e sem janela que esconda um atraso grande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulso {
    /// Quantos blocos com tom no começo de cada pulso.
    pub blocos_com_tom: u64,
    /// O intervalo médio entre dois começos, em blocos.
    pub blocos_no_periodo: u64,
}

/// O silêncio mínimo entre o fim de um tom e o começo seguinte: 500 ms, o que o
/// detector de quem recebe exige para chamar o próximo som de começo.
const BLOCOS_DE_SILENCIO_MINIMO: u64 = 50;

/// A semente da sequência dos intervalos. Fixa: quem analisa refaz a mesma.
const SEMENTE_DO_COMPASSO: u64 = 0x5EE1_E017;

impl Pulso {
    /// Lê `<tom>/<período>` em ms. Os dois múltiplos de 10 ms (o bloco: o
    /// começo de um pulso cai sempre no começo de um bloco, e a hora marcada é
    /// a dele), o tom maior que zero, e o menor intervalo (¾ do período) com
    /// lugar para o tom e mais 500 ms de silêncio.
    ///
    /// # Errors
    /// O texto do que não se leu.
    pub fn ler(texto: &str) -> Result<Self, String> {
        let erro = || {
            format!(
                "{VARIAVEL_DO_PULSO}={texto:?}: esperado <tom>/<período> em ms, múltiplos de 10, \
                 0 < tom e tom + 500 ≤ ¾ do período"
            )
        };
        let (tom, periodo) = texto.trim().split_once('/').ok_or_else(erro)?;
        let tom: u64 = tom.trim().parse().map_err(|_| erro())?;
        let periodo: u64 = periodo.trim().parse().map_err(|_| erro())?;
        let ms_do_bloco = u64::try_from(CADENCIA.as_millis()).unwrap_or(10);
        if tom == 0 || !tom.is_multiple_of(ms_do_bloco) || !periodo.is_multiple_of(ms_do_bloco) {
            return Err(erro());
        }
        let pulso = Self {
            blocos_com_tom: tom / ms_do_bloco,
            blocos_no_periodo: periodo / ms_do_bloco,
        };
        if pulso.blocos_com_tom + BLOCOS_DE_SILENCIO_MINIMO > pulso.menor_intervalo() {
            return Err(erro());
        }
        Ok(pulso)
    }

    /// O pulso pedido pelo ambiente, se algum.
    fn do_ambiente() -> Option<Self> {
        let texto = std::env::var(VARIAVEL_DO_PULSO).ok()?;
        Some(Self::ler(&texto).unwrap_or_else(|erro| panic!("{erro}")))
    }

    fn menor_intervalo(&self) -> u64 {
        self.blocos_no_periodo - self.blocos_no_periodo / 4
    }

    /// Quantos blocos do começo do pulso `n` ao começo do `n + 1`: entre ¾ e
    /// 1¼ do período, pelo `splitmix64` de `n` com a semente fixa.
    #[must_use]
    pub fn intervalo(&self, n: u64) -> u64 {
        let mut z = (n ^ SEMENTE_DO_COMPASSO).wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        self.menor_intervalo() + z % (self.blocos_no_periodo / 2 + 1)
    }
}

/// Onde a linha está no compasso: o número do pulso corrente e os blocos em que
/// o tom dele acaba e o próximo começa.
#[derive(Debug, Clone, Copy)]
struct Compasso {
    pulso: Pulso,
    numero: u64,
    fim_do_tom: u64,
    proximo: u64,
}

impl Compasso {
    fn novo(pulso: Pulso) -> Self {
        Self {
            pulso,
            numero: 0,
            fim_do_tom: 0,
            proximo: 0,
        }
    }

    /// Para o bloco `bloco` (chamado em ordem, de 0 em diante): se ele leva o
    /// tom, e o número do pulso, se um começa nele.
    fn bloco(&mut self, bloco: u64) -> (bool, Option<u64>) {
        let mut comeco = None;
        if bloco == self.proximo {
            comeco = Some(self.numero);
            self.fim_do_tom = bloco + self.pulso.blocos_com_tom;
            self.proximo = bloco + self.pulso.intervalo(self.numero);
            self.numero += 1;
        }
        (bloco < self.fim_do_tom, comeco)
    }
}

/// **A hora dos pulsos**: UMA âncora UTC (o relógio de parede, lido uma vez quando a linha abre) mais o tempo MONOTÔNICO desde ela.
/// Lido a cada pulso, o relógio de parede salta quando o NTP do sistema o corrige (no Windows, ~15 ms entre duas calibrações de
/// 3 min), e numa rodada de 1 h esses saltos pareceriam atraso acumulando. O monotônico não salta; quem recebe usa o
/// `performance.timeOrigin + performance.now()`, que também é uma âncora mais o monotônico.
#[derive(Debug, Clone, Copy)]
struct Ancora {
    parede: std::time::SystemTime,
    mono: Instant,
}

impl Ancora {
    fn agora() -> Self {
        Self {
            parede: std::time::SystemTime::now(),
            mono: Instant::now(),
        }
    }

    /// A hora UTC de `agora`, pela âncora: não lê o relógio de parede.
    fn utc(&self, agora: Instant) -> std::time::SystemTime {
        self.parede + agora.saturating_duration_since(self.mono)
    }
}

fn micros_unix(hora: std::time::SystemTime) -> u128 {
    hora.duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |desde| desde.as_micros())
}

/// A linha JSON do começo de um pulso: `utc_us` pela âncora monotônica (o que a análise usa) e `utc_parede_us`, o relógio de parede
/// lido no mesmo instante, só para diagnóstico (a diferença entre os dois mostra os saltos do NTP).
fn linha_do_pulso(
    nome: &str,
    pulso: u64,
    bloco: u64,
    utc: std::time::SystemTime,
    parede: std::time::SystemTime,
) -> String {
    format!(
        "{{\"pulso\":{pulso},\"bloco\":{bloco},\"utc_us\":{},\"utc_parede_us\":{},\"nome\":{}}}",
        micros_unix(utc),
        micros_unix(parede),
        texto_json(nome)
    )
}

/// Uma string JSON, sem dependência nova: o nome é de teste, mas escapa-se.
fn texto_json(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len() + 2);
    saida.push('"');
    for c in texto.chars() {
        match c {
            '"' => saida.push_str("\\\""),
            '\\' => saida.push_str("\\\\"),
            c if (c as u32) < 0x20 => saida.push_str(&format!("\\u{:04x}", c as u32)),
            c => saida.push(c),
        }
    }
    saida.push('"');
    saida
}

/// O que se sabe dos aparelhos de um nome.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Contagem {
    /// Quantos estão abertos agora.
    pub abertos: usize,
    /// Quantas vezes um abriu, desde o começo do processo.
    pub aberturas: u64,
    /// Quantos blocos de captura foram entregues, somando todos.
    pub blocos: u64,
    /// Quantos blocos **de saída** levavam som: alguma amostra acima de
    /// [`LIMIAR_DE_SOM`].
    ///
    /// É a medida da volta da voz, a metade que faltava: a saída de mentira
    /// descartava o que tocava, e «depois de voltar, eu ouço os outros?» não
    /// tinha como ser respondida sem um alto-falante. Um bloco de silêncio
    /// digital (a saída vazia, que o caminho de reprodução preenche com zero)
    /// não conta.
    pub blocos_com_som: u64,
}

/// A partir de que amplitude um bloco de saída «tem som»: −60 dBFS.
///
/// Acima do silêncio digital e de qualquer arredondamento do reamostrador, e
/// muito abaixo de uma voz decodificada, que passa de −40 dBFS mesmo baixa.
pub const LIMIAR_DE_SOM: f32 = 0.001;

/// Se um bloco de saída leva som.
fn tem_som(bloco: &[f32]) -> bool {
    bloco.iter().any(|amostra| amostra.abs() > LIMIAR_DE_SOM)
}

/// As contagens, por nome. Uma árvore e não um `HashMap`: são poucos nomes, e
/// a ordem não importa a ninguém.
static CONTAGENS: Mutex<BTreeMap<String, Contagem>> = Mutex::new(BTreeMap::new());

/// O que se sabe dos aparelhos de mentira com este nome.
#[must_use]
pub fn contagem(nome: &str) -> Contagem {
    CONTAGENS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(nome)
        .copied()
        .unwrap_or_default()
}

/// O id completo de um aparelho de mentira com este nome.
#[must_use]
pub fn id(nome: &str) -> String {
    format!("{PREFIXO}{nome}")
}

/// Mexe na contagem de um nome.
fn contar(nome: &str, mexer: impl FnOnce(&mut Contagem)) {
    let mut contagens = CONTAGENS.lock().unwrap_or_else(PoisonError::into_inner);
    mexer(contagens.entry(nome.to_owned()).or_default());
}

/// O nome pedido, se um dos lados pediu um aparelho de mentira.
///
/// O da captura vence quando os dois pediram, e um lado só basta para os dois
/// serem de mentira: ver o doc do módulo.
pub(super) fn pedido(wanted: Wanted<'_>) -> Option<&str> {
    [wanted.capture, wanted.playback]
        .into_iter()
        .flatten()
        .find_map(|id| id.strip_prefix(PREFIXO))
}

/// A linha que faz as vezes dos dois retornos do `cpal`.
///
/// Largá-la para a linha e a espera, como largar os dois fluxos para o áudio.
pub(super) struct Linha {
    parar: Arc<AtomicBool>,
    linha: Option<std::thread::JoinHandle<()>>,
    nome: String,
}

impl Drop for Linha {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::Relaxed);
        if let Some(linha) = self.linha.take() {
            let _ = linha.join();
        }
        contar(&self.nome, |contagem| {
            contagem.abertos = contagem.abertos.saturating_sub(1);
        });
    }
}

/// Abre um par de aparelhos de mentira com este nome.
///
/// Nunca falha: um aparelho de mentira que recusasse abrir levaria
/// `open_preferring`, no núcleo, a tentar o da máquina — e é exatamente o que
/// este módulo existe para não deixar acontecer.
pub(super) fn abrir(nome: &str, ring_ms: u32) -> AudioIo {
    let canais = NonZeroU16::MIN;
    let counters = StreamCounters::shared();
    let (mut sink, captured) = capture_path(
        capacity_for_ms(ring_ms, SAMPLE_RATE_HZ),
        canais,
        Arc::clone(&counters),
    );
    let (to_device, mut source) = playback_path(
        capacity_for_ms(ring_ms, SAMPLE_RATE_HZ),
        canais,
        Arc::clone(&counters),
    );

    let parar = Arc::new(AtomicBool::new(false));
    let parar_na_linha = Arc::clone(&parar);
    let nome_na_linha = nome.to_owned();
    let mut compasso = Pulso::do_ambiente().map(Compasso::novo);
    contar(nome, |contagem| {
        contagem.abertos += 1;
        contagem.aberturas += 1;
    });
    let linha = std::thread::Builder::new()
        .name("seele-mentira".into())
        .spawn(move || {
            #[allow(clippy::cast_precision_loss, reason = "48 000 cabe exato num f32")]
            let passo = std::f32::consts::TAU * TOM_HZ / SAMPLE_RATE_HZ as f32;
            let mut fase = 0.0_f32;
            let mut entrada = [0.0_f32; BLOCO];
            let mut saida = [0.0_f32; BLOCO];
            let ancora = Ancora::agora();
            let mut proxima = Instant::now();
            let mut bloco: u64 = 0;
            while !parar_na_linha.load(Ordering::Relaxed) {
                let (com_tom, comeco) = compasso
                    .as_mut()
                    .map_or((true, None), |compasso| compasso.bloco(bloco));
                for amostra in &mut entrada {
                    *amostra = if com_tom { AMPLITUDE * fase.sin() } else { 0.0 };
                    fase = (fase + passo) % std::f32::consts::TAU;
                }
                let mono = Instant::now();
                let parede = std::time::SystemTime::now();
                sink.on_capture::<f32>(&entrada);
                if let Some(numero) = comeco {
                    #[allow(
                        clippy::print_stderr,
                        reason = "o STDERR é a saída da medida: ver o doc do módulo"
                    )]
                    {
                        eprintln!(
                            "{}",
                            linha_do_pulso(&nome_na_linha, numero, bloco, ancora.utc(mono), parede)
                        );
                    }
                }
                bloco += 1;
                source.on_playback::<f32>(&mut saida);
                let com_som = tem_som(&saida);
                contar(&nome_na_linha, |contagem| {
                    contagem.blocos += 1;
                    contagem.blocos_com_som += u64::from(com_som);
                });
                proxima += CADENCIA;
                std::thread::sleep(proxima.saturating_duration_since(Instant::now()));
            }
        })
        .ok();

    AudioIo {
        _vivos: Vivos::Mentira {
            _linha: Linha {
                parar,
                linha,
                nome: nome.to_owned(),
            },
        },
        captured,
        to_device,
        counters,
        capture_rate_hz: SAMPLE_RATE_HZ,
        playback_rate_hz: SAMPLE_RATE_HZ,
        capture: Some(CaptureDevice {
            id: id(nome),
            name: format!("microfone de mentira ({nome})"),
            default: false,
        }),
        playback: Some(PlaybackDevice {
            id: id(nome),
            name: format!("saída de mentira ({nome})"),
            default: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_id_de_mentira_abre_os_dois_lados_de_mentira_e_largar_fecha() {
        let nome = "teste-do-modulo";
        let antes = contagem(nome);
        let pedido = id(nome);
        let io = super::super::open(
            Wanted {
                capture: Some(&pedido),
                playback: None,
            },
            100,
        )
        .unwrap();
        assert_eq!(
            io.capture.as_ref().map(|d| d.id.as_str()),
            Some(pedido.as_str())
        );
        assert_eq!(
            io.playback.as_ref().map(|d| d.id.as_str()),
            Some(pedido.as_str())
        );
        assert_eq!(contagem(nome).abertos, antes.abertos + 1);
        assert_eq!(contagem(nome).aberturas, antes.aberturas + 1);
        std::thread::sleep(Duration::from_millis(60));
        assert!(
            io.captured.slots() > 0,
            "a linha não entregou captura nenhuma em 60 ms"
        );
        drop(io);
        assert_eq!(contagem(nome).abertos, antes.abertos);
    }

    /// **A saída conta o que toca, e só o que tem som.** Sem nada a tocar, os
    /// blocos de saída são silêncio e não contam; com um tom empurrado para a
    /// saída, contam.
    #[test]
    fn a_saida_de_mentira_conta_os_blocos_com_som_e_nao_o_silencio() {
        let nome = "teste-da-saida";
        let pedido = id(nome);
        let mut io = super::super::open(
            Wanted {
                capture: None,
                playback: Some(&pedido),
            },
            200,
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let calado = contagem(nome);
        assert!(
            calado.blocos > 0,
            "a linha não pediu bloco de saída nenhum em 60 ms"
        );
        assert_eq!(
            calado.blocos_com_som, 0,
            "uma saída sem nada a tocar contou blocos com som"
        );

        // 100 ms de tom: dez blocos.
        for indice in 0..(BLOCO * 10) {
            #[allow(clippy::cast_precision_loss, reason = "índices pequenos")]
            let amostra = AMPLITUDE * (indice as f32 * 0.05).sin();
            if io.to_device.push(amostra).is_err() {
                break;
            }
        }
        let prazo = Instant::now() + Duration::from_secs(2);
        while contagem(nome).blocos_com_som == 0 && Instant::now() < prazo {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            contagem(nome).blocos_com_som > 0,
            "o tom empurrado para a saída não contou como som em 2 s"
        );
        drop(io);
    }

    /// **O compasso do pulso**: 200/2000 dá 20 blocos de tom em cada começo,
    /// intervalos entre 150 e 250 blocos que não se repetem num padrão fixo, e
    /// o que não se lê é recusado.
    #[test]
    fn o_pulso_200_2000_nao_e_periodico_e_marca_cada_comeco_uma_vez() {
        let pulso = Pulso::ler("200/2000").unwrap();
        assert_eq!(
            pulso,
            Pulso {
                blocos_com_tom: 20,
                blocos_no_periodo: 200
            }
        );
        let mut compasso = Compasso::novo(pulso);
        let mut comecos = Vec::new();
        let mut com_tom = 0;
        for bloco in 0..20_000 {
            let (tom, comeco) = compasso.bloco(bloco);
            com_tom += u64::from(tom);
            if let Some(numero) = comeco {
                assert_eq!(numero, comecos.len() as u64, "um número pulado ou repetido");
                comecos.push(bloco);
            }
        }
        assert_eq!(comecos.first(), Some(&0));
        let intervalos: Vec<u64> = comecos
            .windows(2)
            .filter_map(|par| Some(par.get(1)? - par.first()?))
            .collect();
        assert!(
            intervalos.iter().all(|i| (150..=250).contains(i)),
            "{intervalos:?}"
        );
        for (n, i) in intervalos.iter().enumerate() {
            assert_eq!(
                *i,
                pulso.intervalo(n as u64),
                "o compasso não seguiu a sequência"
            );
        }
        let distintos: std::collections::BTreeSet<_> = intervalos.iter().collect();
        assert!(
            distintos.len() > 40,
            "intervalos pouco variados: {}",
            distintos.len()
        );
        // O tom: os 20 primeiros blocos de cada começo (o último pode ser cortado no bloco 20 000).
        let esperado: u64 = comecos.iter().map(|c| (20_000 - c).min(20)).sum();
        assert_eq!(
            com_tom, esperado,
            "o tom não ficou só nos 20 blocos de cada começo"
        );
        for ruim in [
            "",
            "200",
            "0/2000",
            "205/2000",
            "200/2005",
            "a/b",
            "300/200",
            "1100/2000",
        ] {
            assert!(Pulso::ler(ruim).is_err(), "{ruim:?} foi aceito");
        }
        // O limite: 1000 ms de tom + 500 de silêncio cabem exato no menor intervalo (1500 ms).
        assert!(
            Pulso::ler("1000/2000").is_ok(),
            "o limite exato foi recusado"
        );
    }

    /// **A sequência é a mesma que o analisador refaz** (`preparacao-0.17/voz/analisar.py`):
    /// os primeiros intervalos de 200/2000, presos aqui.
    /// Os mesmos números de `intervalo()` em `analisar.py`: se um mudar sem o
    /// outro, a análise pareia contra a sequência errada.
    const INTERVALOS_DO_ANALISADOR: [u64; 6] = [205, 190, 158, 206, 215, 194];

    #[test]
    fn os_primeiros_intervalos_de_200_2000_sao_os_do_analisador() {
        let pulso = Pulso::ler("200/2000").unwrap();
        let primeiros: Vec<u64> = (0..6).map(|n| pulso.intervalo(n)).collect();
        assert_eq!(primeiros, INTERVALOS_DO_ANALISADOR);
    }

    #[test]
    fn a_linha_do_pulso_e_json_com_a_hora_em_microssegundos() {
        let hora = std::time::UNIX_EPOCH + Duration::from_micros(1_700_000_000_123_456);
        let parede = std::time::UNIX_EPOCH + Duration::from_micros(1_700_000_000_138_456);
        assert_eq!(
            linha_do_pulso("a\"b", 7, 1234, hora, parede),
            r#"{"pulso":7,"bloco":1234,"utc_us":1700000000123456,"utc_parede_us":1700000000138456,"nome":"a\"b"}"#
        );
    }

    /// **A âncora não salta com o relógio de parede**: a hora de cada pulso é a âncora mais o monotônico, e não lê o relógio de
    /// parede de novo. Uma sequência de pulsos a cada 2 s monotônicos sai a exatamente 2 s, por mais que a parede tenha sido
    /// corrigida no meio (o que, numa rodada longa, pareceria atraso acumulando).
    #[test]
    fn a_hora_dos_pulsos_e_a_ancora_mais_o_monotonico_e_nao_salta_com_a_parede() {
        let inicio = Instant::now();
        let ancora = Ancora {
            parede: std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            mono: inicio,
        };
        let horas: Vec<u128> = (0..30u64)
            .map(|k| micros_unix(ancora.utc(inicio + Duration::from_secs(2 * k))))
            .collect();
        assert!(
            horas.windows(2).all(|par| par
                .get(1)
                .zip(par.first())
                .is_some_and(|(b, a)| b - a == 2_000_000)),
            "{horas:?}"
        );
        assert_eq!(horas.first(), Some(&1_700_000_000_000_000));
        // Um instante anterior à âncora não volta no tempo (satura na âncora).
        assert_eq!(
            Some(micros_unix(ancora.utc(inicio))),
            horas.first().copied()
        );
    }

    #[test]
    fn so_um_bloco_acima_do_limiar_tem_som() {
        assert!(!tem_som(&[0.0; BLOCO]));
        assert!(!tem_som(&[LIMIAR_DE_SOM; BLOCO]));
        let mut um_pico = [0.0_f32; BLOCO];
        um_pico[7] = -0.01;
        assert!(tem_som(&um_pico));
    }

    #[test]
    fn um_id_sem_o_prefixo_nao_e_de_mentira() {
        assert_eq!(
            pedido(Wanted {
                capture: Some("BuiltInMicrophoneDevice"),
                playback: None,
            }),
            None
        );
        assert_eq!(
            pedido(Wanted {
                capture: None,
                playback: Some("seele-mentira:x"),
            }),
            Some("x")
        );
    }
}
