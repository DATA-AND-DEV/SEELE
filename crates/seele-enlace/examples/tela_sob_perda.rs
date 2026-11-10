//! Mede como um vídeo sintético de tela se comporta sob perda, em três
//! transportes, com uma voz ao lado. É bancada, não produto: imprime tabelas em
//! Markdown, e o relatório é `docs/medidas/2026-11-10-tela-sob-perda.md`.
//!
//! ```text
//! cargo run --release -p seele-enlace --example tela_sob_perda
//! cargo run --release -p seele-enlace --example tela_sob_perda -- \
//!     [--segundos 20] [--rodadas 3] [--semente 20261110] [--ritmo-bps 4000000] \
//!     [--em-serie] [--so <pedaço do nome do cenário>] [--perda 0.003]
//! ```
//!
//! # A carga
//!
//! - Vídeo: 30 quadros/s, deltas de 6 KiB (≈1,47 Mbps), uma chave de 80 KiB no
//!   começo e a cada pedido. Sem codificador: o conteúdo é enchimento.
//! - Voz: 50 pacotes/s de 120 bytes, no canal `voz` de verdade
//!   (`Conexao::send_datagram`), de quem liga para o anfitrião, como o vídeo.
//!
//! # Os transportes do vídeo
//!
//! - **A** (o de hoje): um canal `u`, confiável e ordenado, `write_all` de um
//!   quadro por vez. O emissor captura um quadro quando o anterior acabou de
//!   ser escrito; se o crédito segura a escrita além de um período, os quadros
//!   daquele intervalo não são gerados. **Não há controle de taxa**: nem o
//!   balde do produto (`seele-core/src/tela.rs`), nem a chave em fatias.
//! - **B** (candidato): canal `j` (sem ordem, 0 retransmissões). Cada quadro
//!   vira fragmentos de até 1100 bytes com cabeçalho (id, índice, total, tipo,
//!   chave de base, instante). Um quadro com fragmento faltando está perdido; um
//!   delta só é exibível se a cadeia desde a chave de base estiver inteira. Sem
//!   isso, o receptor pede uma chave pelo `ctl` (confiável), e repete o pedido a
//!   cada 300 ms enquanto precisar. O emissor ignora o pedido que não viu a
//!   chave mais nova. Os fragmentos saem num ritmo (`--ritmo-bps`, com rajada
//!   de até 20 ms): em rajada pura, uma chave de 80 KiB bate no
//!   `LIMIAR_DE_VOLUME` de 64 KiB do próprio `j` e nunca chega inteira.
//! - **B2**: os deltas como em B; a chave vai inteira por um canal `u`
//!   confiável. O emissor não começa outra chave enquanto escreve uma.
//!   A chave é escrita em fatias de 4 KiB no mesmo ritmo: inteira de uma vez,
//!   ela enche o SCTP até o `LIMIAR_DE_VOLUME` e o `j` descarta os deltas.
//! - **teto do j**: só deltas de 6 KiB no `j`, e todo quadro completo conta
//!   como exibível (sem cadeia, sem chave, sem pedido). Não é um desenho: é o
//!   máximo que qualquer desenho sobre o `j` entregaria com esta carga.
//! - **voz**: só a voz, sem vídeo: é a referência do que o vídeo custa a ela.
//!
//! # As métricas
//!
//! - Latência de quadro: do instante da captura até o quadro estar completo e
//!   exibível no receptor.
//! - Congelado: a soma dos intervalos de mais de 200 ms sem quadro novo
//!   exibível, sobre a janela da medida (o começo e o fim da janela contam
//!   como bordas).
//! - A voz: perda (não chegou até 1 s depois do fim) e atraso p95.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    missing_docs
)]

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use seele_enlace::teste::{par_por_cano, ConfigDoCano, ContagemDoSentido, Sentido};
use seele_enlace::{CanalDeTunel, Conexao, Confiabilidade, Datagramas, Envio, Recebe};
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};

const QUADROS_POR_SEGUNDO: u32 = 30;
const DELTA: usize = 6 * 1024;
const CHAVE: usize = 80 * 1024;
const FRAGMENTO: usize = 1100;
/// id (4), índice (2), total (2), tipo (1), chave de base (4), instante (8).
const CABECALHO_DO_FRAGMENTO: usize = 21;
/// tipo (1), id (4), instante (8), tamanho (4): o cabeçalho de um quadro num `u`.
const CABECALHO_DO_QUADRO: usize = 17;
const VOZ: usize = 120;
const VOZ_A_CADA: Duration = Duration::from_millis(20);
const CONGELADO: Duration = Duration::from_millis(200);
const REPETIR_O_PEDIDO: Duration = Duration::from_millis(300);
const ATRASO: Duration = Duration::from_millis(30);

fn agora_us() -> u64 {
    static ZERO: OnceLock<Instant> = OnceLock::new();
    ZERO.get_or_init(Instant::now).elapsed().as_micros() as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Transporte {
    SoVoz,
    A,
    B,
    B2,
    TetoDoJ,
}

impl Transporte {
    const TODOS: [Self; 5] = [Self::SoVoz, Self::A, Self::B, Self::B2, Self::TetoDoJ];

    const fn nome(self) -> &'static str {
        match self {
            Self::SoVoz => "voz",
            Self::A => "A",
            Self::B => "B",
            Self::B2 => "B2",
            Self::TetoDoJ => "teto do j",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Cenario {
    nome: &'static str,
    perda: f64,
    banda_bps: Option<u64>,
}

const CENARIOS: [Cenario; 7] = [
    Cenario {
        nome: "0%",
        perda: 0.0,
        banda_bps: None,
    },
    Cenario {
        nome: "1%",
        perda: 0.01,
        banda_bps: None,
    },
    Cenario {
        nome: "3%",
        perda: 0.03,
        banda_bps: None,
    },
    Cenario {
        nome: "5%",
        perda: 0.05,
        banda_bps: None,
    },
    Cenario {
        nome: "10%",
        perda: 0.10,
        banda_bps: None,
    },
    Cenario {
        nome: "1,0 Mbps, 0%",
        perda: 0.0,
        banda_bps: Some(1_000_000),
    },
    Cenario {
        nome: "2,0 Mbps, 1%",
        perda: 0.01,
        banda_bps: Some(2_000_000),
    },
];

impl Cenario {
    fn sentido(&self) -> Sentido {
        Sentido {
            perda: self.perda,
            atraso: ATRASO,
            jitter: Duration::ZERO,
            banda_bps: self.banda_bps,
            // 250 ms de fila na banda do gargalo.
            fila_max: self
                .banda_bps
                .map_or(64 * 1024, |bps| (bps / 8 / 4) as usize),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Opcoes {
    segundos: u64,
    rodadas: u64,
    semente: u64,
    ritmo_bps: u64,
    em_serie: bool,
}

/// O que uma célula (cenário × transporte × rodada) mediu.
#[derive(Debug, Default, Clone)]
struct Medida {
    /// (quando foi exibido, a latência), em microssegundos.
    exibidos: Vec<(u64, u64)>,
    gerados: u64,
    chaves: u64,
    pedidos: u64,
    bytes_udp: u64,
    j_descartado_no_emissor: u64,
    j_recusado: u64,
    voz_enviada: u64,
    voz_recusada: u64,
    voz_atrasos_us: Vec<u64>,
    ida: ContagemDoSentido,
    comeco_us: u64,
    janela_us: u64,
}

// ---- o emissor --------------------------------------------------------------

#[derive(Default)]
struct ContaDoEmissor {
    gerados: AtomicU64,
    chaves: AtomicU64,
    recusados: AtomicU64,
}

fn quadro_no_fluxo(tipo: u8, id: u32, instante: u64, tamanho: usize) -> Vec<u8> {
    let mut q = Vec::with_capacity(CABECALHO_DO_QUADRO + tamanho);
    q.push(tipo);
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&instante.to_be_bytes());
    q.extend_from_slice(&(tamanho as u32).to_be_bytes());
    q.resize(CABECALHO_DO_QUADRO + tamanho, 0xA5);
    q
}

/// (A): um quadro por vez no `u`. O `interval` com `Skip` é a captura: quem
/// escreve atrasado perde os quadros do intervalo, e o próximo é fresco.
async fn emitir_no_fluxo(mut envio: Envio, conta: Arc<ContaDoEmissor>) {
    let mut captura = tokio::time::interval(Duration::from_secs(1) / QUADROS_POR_SEGUNDO);
    captura.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut id = 0_u32;
    loop {
        captura.tick().await;
        let (tipo, tamanho) = if id == 0 { (1, CHAVE) } else { (0, DELTA) };
        let quadro = quadro_no_fluxo(tipo, id, agora_us(), tamanho);
        if envio.write_all(&quadro).await.is_err() {
            return;
        }
        conta.gerados.fetch_add(1, Ordering::Relaxed);
        if tipo == 1 {
            conta.chaves.fetch_add(1, Ordering::Relaxed);
        }
        id += 1;
    }
}

fn fragmentos(tipo: u8, id: u32, base: u32, instante: u64, tamanho: usize) -> Vec<Vec<u8>> {
    let por_fragmento = FRAGMENTO - CABECALHO_DO_FRAGMENTO;
    let total = tamanho.div_ceil(por_fragmento);
    let mut resta = tamanho;
    (0..total)
        .map(|indice| {
            let corpo = resta.min(por_fragmento);
            resta -= corpo;
            let mut f = Vec::with_capacity(CABECALHO_DO_FRAGMENTO + corpo);
            f.extend_from_slice(&id.to_be_bytes());
            f.extend_from_slice(&(indice as u16).to_be_bytes());
            f.extend_from_slice(&(total as u16).to_be_bytes());
            f.push(tipo);
            f.extend_from_slice(&base.to_be_bytes());
            f.extend_from_slice(&instante.to_be_bytes());
            f.resize(CABECALHO_DO_FRAGMENTO + corpo, 0x5A);
            f
        })
        .collect()
}

/// Para a tarefa que escreve a chave no `u` (id, instante), e se ela está ocupada.
type CanalDasChaves = (mpsc::UnboundedSender<(u32, u64)>, Arc<AtomicBool>);

/// (B) e (B2): os fragmentos no `j`, num ritmo. Em (B2), `chaves` é o canal
/// para a tarefa que escreve a chave no `u`.
async fn emitir_no_j(
    j: Datagramas,
    mut pedidos: mpsc::UnboundedReceiver<u32>,
    chaves: Option<CanalDasChaves>,
    ritmo_bps: u64,
    sem_chave: bool,
    conta: Arc<ContaDoEmissor>,
    // O `j` fecha quando o `Datagramas` é largado: o receptor do par fica vivo.
    _segurar: mpsc::UnboundedSender<()>,
) {
    let periodo = Duration::from_secs(1) / QUADROS_POR_SEGUNDO;
    let mut passo = tokio::time::interval(Duration::from_millis(1));
    passo.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let comeco = Instant::now();
    let comeco_us = agora_us();
    let mut fila: VecDeque<Vec<u8>> = VecDeque::new();
    let mut proximo = 0_u32;
    let mut ultima_chave: Option<u32> = None;
    let mut quer_chave = !sem_chave;
    let por_ms = ritmo_bps as f64 / 8000.0;
    let teto_de_fichas = por_ms * 20.0;
    let mut fichas = teto_de_fichas;
    let mut antes = comeco;
    loop {
        passo.tick().await;
        let agora = Instant::now();
        while let Ok(visto) = pedidos.try_recv() {
            // Um pedido de quem ainda não viu a chave mais nova: ela está a caminho.
            let ja_vai = ultima_chave.is_some_and(|c| c > visto);
            let ocupado = chaves
                .as_ref()
                .is_some_and(|(_, escrevendo)| escrevendo.load(Ordering::Acquire));
            if ja_vai || ocupado {
                continue;
            }
            quer_chave = !sem_chave;
        }
        while comeco + periodo * proximo <= agora {
            let id = proximo;
            let instante = comeco_us + u64::from(id) * periodo.as_micros() as u64;
            proximo += 1;
            conta.gerados.fetch_add(1, Ordering::Relaxed);
            if quer_chave {
                quer_chave = false;
                ultima_chave = Some(id);
                conta.chaves.fetch_add(1, Ordering::Relaxed);
                if let Some((canal, escrevendo)) = &chaves {
                    escrevendo.store(true, Ordering::Release);
                    let _ = canal.send((id, instante));
                } else {
                    fila.extend(fragmentos(1, id, id, instante, CHAVE));
                }
            } else {
                let base = ultima_chave.unwrap_or(0);
                fila.extend(fragmentos(0, id, base, instante, DELTA));
            }
        }
        if ritmo_bps == 0 {
            fichas = f64::MAX;
        } else {
            fichas = (fichas + por_ms * (agora - antes).as_secs_f64() * 1000.0).min(teto_de_fichas);
        }
        antes = agora;
        while fila.front().is_some_and(|f| f.len() as f64 <= fichas) {
            let Some(f) = fila.pop_front() else { break };
            fichas -= f.len() as f64;
            if j.mandar(&f).is_err() {
                conta.recusados.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// (B2): a chave inteira num `u` confiável, uma de cada vez.
async fn escrever_chaves(
    mut envio: Envio,
    mut chaves: mpsc::UnboundedReceiver<(u32, u64)>,
    escrevendo: Arc<AtomicBool>,
    ritmo_bps: u64,
) {
    // Em fatias, no mesmo ritmo dos fragmentos: a chave inteira de uma vez
    // enche o SCTP até o `LIMIAR_DE_VOLUME`, e o `j` descarta os deltas
    // enquanto ela está lá.
    const FATIA: usize = 4096;
    while let Some((id, instante)) = chaves.recv().await {
        let quadro = quadro_no_fluxo(1, id, instante, CHAVE);
        for fatia in quadro.chunks(FATIA) {
            if envio.write_all(fatia).await.is_err() {
                return;
            }
            // Ritmo zero é sem ritmo: a divisão não acontece e nada espera.
            if let Some(espera) = (fatia.len() as u64 * 8_000_000).checked_div(ritmo_bps) {
                tokio::time::sleep(Duration::from_micros(espera)).await;
            }
        }
        escrevendo.store(false, Ordering::Release);
    }
}

async fn emitir_voz(conexao: Conexao, enviada: Arc<AtomicU64>, recusada: Arc<AtomicU64>) {
    let mut passo = tokio::time::interval(VOZ_A_CADA);
    passo.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut n = 0_u32;
    loop {
        passo.tick().await;
        let mut d = Vec::with_capacity(VOZ);
        d.push(1);
        d.extend_from_slice(&n.to_be_bytes());
        d.extend_from_slice(&agora_us().to_be_bytes());
        d.resize(VOZ, 0);
        n += 1;
        if conexao.send_datagram(d).is_ok() {
            enviada.fetch_add(1, Ordering::Relaxed);
        } else {
            recusada.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ---- o receptor -------------------------------------------------------------

async fn receber_voz(conexao: Conexao) -> Vec<u64> {
    let mut atrasos = Vec::new();
    let mut vistos = std::collections::HashSet::new();
    while let Ok(d) = conexao.read_datagram().await {
        if d.len() != VOZ {
            continue;
        }
        let n = u32::from_be_bytes(d[1..5].try_into().unwrap());
        let saiu = u64::from_be_bytes(d[5..13].try_into().unwrap());
        if vistos.insert(n) {
            atrasos.push(agora_us().saturating_sub(saiu));
        }
    }
    atrasos
}

async fn ler_quadro(recebe: &mut Recebe) -> Option<(u8, u32, u64)> {
    let mut cabecalho = [0_u8; CABECALHO_DO_QUADRO];
    recebe.read_exact(&mut cabecalho).await.ok()?;
    let tipo = cabecalho[0];
    let id = u32::from_be_bytes(cabecalho[1..5].try_into().unwrap());
    let instante = u64::from_be_bytes(cabecalho[5..13].try_into().unwrap());
    let tamanho = u32::from_be_bytes(cabecalho[13..17].try_into().unwrap()) as usize;
    let mut corpo = vec![0_u8; tamanho];
    recebe.read_exact(&mut corpo).await.ok()?;
    Some((tipo, id, instante))
}

/// (A): cada quadro lido inteiro é exibível.
async fn receber_do_fluxo(mut recebe: Recebe, exibidos: mpsc::UnboundedSender<(u64, u64)>) {
    while let Some((_, _, instante)) = ler_quadro(&mut recebe).await {
        let agora = agora_us();
        let _ = exibidos.send((agora, agora.saturating_sub(instante)));
    }
}

struct Parcial {
    total: u16,
    chegaram: Vec<bool>,
    faltam: u16,
}

/// O estado de quem remonta os quadros de (B) e (B2) e decide o que é exibível.
struct Tela {
    chave_no_j: bool,
    /// O teto do `j`: todo quadro completo é exibível, sem cadeia e sem chave.
    independente: bool,
    ultimo: Option<u32>,
    /// A chave mais nova de que se soube (pelo `base` de um delta ou por um
    /// fragmento dela).
    base_max: Option<u32>,
    visto_max: Option<u32>,
    parciais: HashMap<u32, Parcial>,
    /// Deltas completos e ainda não exibidos: id → (base, instante).
    deltas: BTreeMap<u32, (u32, u64)>,
    /// Chaves que chegaram inteiras (para saber que uma chave não se perdeu).
    chaves_completas: Vec<u32>,
    exibidos: mpsc::UnboundedSender<(u64, u64)>,
}

impl Tela {
    fn exibir(&mut self, id: u32, instante: u64) {
        let agora = agora_us();
        let _ = self.exibidos.send((agora, agora.saturating_sub(instante)));
        self.ultimo = Some(id);
    }

    fn fragmento(&mut self, f: &[u8]) {
        if f.len() < CABECALHO_DO_FRAGMENTO {
            return;
        }
        let id = u32::from_be_bytes(f[0..4].try_into().unwrap());
        let indice = u16::from_be_bytes(f[4..6].try_into().unwrap());
        let total = u16::from_be_bytes(f[6..8].try_into().unwrap());
        let tipo = f[8];
        let base = u32::from_be_bytes(f[9..13].try_into().unwrap());
        let instante = u64::from_be_bytes(f[13..21].try_into().unwrap());
        self.visto_max = self.visto_max.max(Some(id));
        self.base_max = self.base_max.max(Some(base));
        if !self.independente && self.ultimo.is_some_and(|u| id <= u) {
            return;
        }
        let parcial = self.parciais.entry(id).or_insert_with(|| Parcial {
            total,
            chegaram: vec![false; usize::from(total)],
            faltam: total,
        });
        let Some(chegou) = parcial.chegaram.get_mut(usize::from(indice)) else {
            return;
        };
        if *chegou || parcial.total != total {
            return;
        }
        *chegou = true;
        parcial.faltam -= 1;
        if parcial.faltam > 0 {
            return;
        }
        self.parciais.remove(&id);
        if self.independente {
            let agora = agora_us();
            let _ = self.exibidos.send((agora, agora.saturating_sub(instante)));
            self.parciais.retain(|outro, _| outro + 60 > id);
            return;
        }
        if tipo == 1 {
            self.chave(id, instante);
        } else {
            self.deltas.insert(id, (base, instante));
            self.escoar();
        }
    }

    /// Uma chave inteira: exibe (se for nova) e segue pelos deltas guardados.
    fn chave(&mut self, id: u32, instante: u64) {
        self.base_max = self.base_max.max(Some(id));
        self.visto_max = self.visto_max.max(Some(id));
        self.chaves_completas.push(id);
        if self.ultimo.is_none_or(|u| id > u) {
            self.exibir(id, instante);
        }
        self.escoar();
    }

    /// Exibe os deltas seguidos ao último exibido, enquanto a cadeia vale.
    fn escoar(&mut self) {
        while let Some(ultimo) = self.ultimo {
            let Some((base, instante)) = self.deltas.get(&(ultimo + 1)).copied() else {
                break;
            };
            if base > ultimo {
                break;
            }
            self.deltas.remove(&(ultimo + 1));
            self.exibir(ultimo + 1, instante);
        }
        if let Some(ultimo) = self.ultimo {
            self.deltas.retain(|id, _| *id > ultimo);
            self.parciais.retain(|id, _| *id > ultimo);
            self.chaves_completas.retain(|id| *id >= ultimo);
        }
    }

    /// Só uma chave nova destrava: há prova de que a cadeia quebrou.
    fn precisa_de_chave(&self) -> bool {
        if self.independente {
            return false;
        }
        let Some(visto) = self.visto_max else {
            return false;
        };
        // De onde a cadeia tem de seguir: a chave mais nova, se ainda não foi
        // exibida, ou o último quadro exibido.
        let em_transito = match (self.base_max, self.ultimo) {
            (Some(b), Some(u)) => b > u,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let ancora = if em_transito {
            let Some(base) = self.base_max else {
                return false;
            };
            // No `j` a chave vem em ordem: se já chegou coisa depois dela e ela
            // não está inteira, perdeu-se. No `u` ela chega; é só esperar.
            if self.chave_no_j && visto > base && !self.chaves_completas.contains(&base) {
                return true;
            }
            base
        } else {
            match self.ultimo {
                Some(u) => u,
                // Nada exibido e nenhuma chave conhecida: falta a primeira.
                None => return true,
            }
        };
        // O primeiro delta que falta depois da âncora: se já chegou coisa mais
        // nova que ele, ele não chega mais (o cano não troca a ordem).
        let mut falta = ancora + 1;
        while self.deltas.contains_key(&falta) {
            falta += 1;
        }
        visto > falta
    }
}

/// (B) e (B2): remonta, exibe e pede chave pelo `ctl`.
async fn receber_do_j(
    mut j: Datagramas,
    mut chaves: Option<mpsc::UnboundedReceiver<(u32, u64)>>,
    mut pedir: Envio,
    exibidos: mpsc::UnboundedSender<(u64, u64)>,
    pedidos: Arc<AtomicU64>,
    independente: bool,
) {
    let mut tela = Tela {
        chave_no_j: chaves.is_none(),
        independente,
        ultimo: None,
        base_max: None,
        visto_max: None,
        parciais: HashMap::new(),
        deltas: BTreeMap::new(),
        chaves_completas: Vec::new(),
        exibidos,
    };
    let mut ultimo_pedido: Option<Instant> = None;
    let mut revisar = tokio::time::interval(Duration::from_millis(20));
    revisar.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            f = j.receber() => {
                let Some(f) = f else { return };
                tela.fragmento(&f);
            }
            chave = async {
                match chaves.as_mut() {
                    Some(c) => c.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some((id, instante)) = chave {
                    tela.chave(id, instante);
                }
            }
            _ = revisar.tick() => {}
        }
        if tela.precisa_de_chave()
            && ultimo_pedido.is_none_or(|quando| quando.elapsed() >= REPETIR_O_PEDIDO)
        {
            ultimo_pedido = Some(Instant::now());
            pedidos.fetch_add(1, Ordering::Relaxed);
            let visto = tela.visto_max.unwrap_or(0);
            if pedir.write_all(&visto.to_be_bytes()).await.is_err() {
                return;
            }
        }
    }
}

// ---- uma célula -------------------------------------------------------------

async fn abrir_j(par: &seele_enlace::teste::ParDeTeste) -> (Datagramas, Datagramas) {
    let CanalDeTunel::SemOrdem(emissor) = par
        .cliente
        .abrir_tunel(Confiabilidade::SemOrdem)
        .await
        .unwrap()
    else {
        panic!("o j abriu ordenado");
    };
    let CanalDeTunel::SemOrdem(receptor) = par.anfitriao.aceitar_tunel().await.unwrap() else {
        panic!("o j chegou ordenado");
    };
    (emissor, receptor)
}

async fn medir(cenario: Cenario, transporte: Transporte, semente: u64, op: Opcoes) -> Medida {
    let config = ConfigDoCano::simetrico(cenario.sentido(), semente);
    let (par, cano) = tokio::time::timeout(Duration::from_secs(60), par_por_cano(config))
        .await
        .expect("o par não abriu pelo cano em 60 s");
    let mut tarefas: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let conta = Arc::new(ContaDoEmissor::default());
    let pedidos = Arc::new(AtomicU64::new(0));
    let (exibidos_tx, mut exibidos_rx) = mpsc::unbounded_channel();
    let (segurar_tx, _segurar_rx) = mpsc::unbounded_channel::<()>();

    let mut ctl_parado: Option<(Envio, Recebe)> = None;
    // Os canais abrem antes de o relógio da medida começar.
    match transporte {
        Transporte::SoVoz => {}
        Transporte::A => {
            let envio = par.cliente.open_uni().await.unwrap();
            let recebe = tokio::time::timeout(Duration::from_secs(30), par.anfitriao.accept_uni())
                .await
                .expect("o u não chegou")
                .unwrap();
            tarefas.push(tokio::spawn(receber_do_fluxo(recebe, exibidos_tx.clone())));
            tarefas.push(tokio::spawn(emitir_no_fluxo(envio, Arc::clone(&conta))));
        }
        Transporte::B | Transporte::B2 | Transporte::TetoDoJ => {
            let (j_tx, j_rx) = tokio::time::timeout(Duration::from_secs(30), abrir_j(&par))
                .await
                .expect("o j não abriu");
            // As metades do `ctl` que a medida não usa ficam vivas: largar
            // uma delas fecha o controle, e com ele a conexão.
            let (ctl_envio, mut pedidos_do_par) = par.cliente.controle().unwrap();
            let (pedir, ctl_recebe) = par.anfitriao.controle().unwrap();
            ctl_parado = Some((ctl_envio, ctl_recebe));
            let (pedidos_tx, pedidos_rx) = mpsc::unbounded_channel();
            tarefas.push(tokio::spawn(async move {
                let mut visto = [0_u8; 4];
                while pedidos_do_par.read_exact(&mut visto).await.is_ok() {
                    let _ = pedidos_tx.send(u32::from_be_bytes(visto));
                }
            }));
            let (para_o_emissor, para_o_receptor) = if transporte == Transporte::B2 {
                let envio = par.cliente.open_uni().await.unwrap();
                let mut recebe =
                    tokio::time::timeout(Duration::from_secs(30), par.anfitriao.accept_uni())
                        .await
                        .expect("o u das chaves não chegou")
                        .unwrap();
                let escrevendo = Arc::new(AtomicBool::new(false));
                let (chaves_tx, chaves_rx) = mpsc::unbounded_channel();
                let (lidas_tx, lidas_rx) = mpsc::unbounded_channel();
                tarefas.push(tokio::spawn(escrever_chaves(
                    envio,
                    chaves_rx,
                    Arc::clone(&escrevendo),
                    op.ritmo_bps,
                )));
                tarefas.push(tokio::spawn(async move {
                    while let Some((_, id, instante)) = ler_quadro(&mut recebe).await {
                        let _ = lidas_tx.send((id, instante));
                    }
                }));
                (Some((chaves_tx, escrevendo)), Some(lidas_rx))
            } else {
                (None, None)
            };
            tarefas.push(tokio::spawn(receber_do_j(
                j_rx,
                para_o_receptor,
                pedir,
                exibidos_tx.clone(),
                Arc::clone(&pedidos),
                transporte == Transporte::TetoDoJ,
            )));
            tarefas.push(tokio::spawn(emitir_no_j(
                j_tx,
                pedidos_rx,
                para_o_emissor,
                op.ritmo_bps,
                transporte == Transporte::TetoDoJ,
                Arc::clone(&conta),
                segurar_tx.clone(),
            )));
        }
    }
    drop(exibidos_tx);

    let comeco_us = agora_us();
    let bytes_antes = par.cliente.leitura().bytes_enviados;
    let voz_enviada = Arc::new(AtomicU64::new(0));
    let voz_recusada = Arc::new(AtomicU64::new(0));
    let ouvinte = tokio::spawn(receber_voz(par.anfitriao.clone()));
    let falante = tokio::spawn(emitir_voz(
        par.cliente.clone(),
        Arc::clone(&voz_enviada),
        Arc::clone(&voz_recusada),
    ));

    tokio::time::sleep(Duration::from_secs(op.segundos)).await;
    falante.abort();
    let bytes_udp = par.cliente.leitura().bytes_enviados - bytes_antes;
    let contadores = par.cliente.contadores();
    let gerados = conta.gerados.load(Ordering::Relaxed);
    // Um segundo a mais para a voz que ainda está a caminho.
    tokio::time::sleep(Duration::from_secs(1)).await;
    for tarefa in &tarefas {
        tarefa.abort();
    }
    let ida = cano.ida();
    drop(ctl_parado);
    par.cliente.close(0, b"fim");
    par.anfitriao.close(0, b"fim");
    let voz_atrasos_us = tokio::time::timeout(Duration::from_secs(5), ouvinte)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let mut exibidos = Vec::new();
    while let Ok(e) = exibidos_rx.try_recv() {
        exibidos.push(e);
    }
    Medida {
        exibidos,
        gerados,
        chaves: conta.chaves.load(Ordering::Relaxed),
        pedidos: pedidos.load(Ordering::Relaxed),
        bytes_udp,
        j_descartado_no_emissor: contadores.tunel_descartado,
        j_recusado: conta.recusados.load(Ordering::Relaxed),
        voz_enviada: voz_enviada.load(Ordering::Relaxed),
        voz_recusada: voz_recusada.load(Ordering::Relaxed) + contadores.voz_nao_saiu,
        voz_atrasos_us,
        ida,
        comeco_us,
        janela_us: op.segundos * 1_000_000,
    }
}

// ---- as contas --------------------------------------------------------------

fn percentil(ordenado: &[f64], p: f64) -> f64 {
    if ordenado.is_empty() {
        return f64::NAN;
    }
    let posicao = ((p * ordenado.len() as f64).ceil() as usize).clamp(1, ordenado.len());
    ordenado[posicao - 1]
}

/// Os números de uma célula, na ordem das colunas.
#[derive(Debug, Clone, Copy)]
struct Linha {
    p50: f64,
    p95: f64,
    p99: f64,
    max: f64,
    congelado: f64,
    maior_congelamento: f64,
    qps: f64,
    gerados: f64,
    chaves: f64,
    pedidos: f64,
    megabytes: f64,
    j_descartado: f64,
    voz_perda: f64,
    voz_p50: f64,
    voz_p95: f64,
    perda_do_cano: f64,
    fila_do_cano: f64,
}

const COLUNAS: usize = 17;

impl Linha {
    fn de(m: &Medida) -> Self {
        let fim = m.comeco_us + m.janela_us;
        let mut latencias: Vec<f64> = m
            .exibidos
            .iter()
            .filter(|(quando, _)| *quando <= fim)
            .map(|(_, l)| *l as f64 / 1000.0)
            .collect();
        latencias.sort_by(f64::total_cmp);
        let mut instantes: Vec<u64> = m
            .exibidos
            .iter()
            .map(|(quando, _)| *quando)
            .filter(|quando| *quando <= fim)
            .collect();
        instantes.sort_unstable();
        let mut congelado = 0_u64;
        let mut maior = 0_u64;
        let mut antes = m.comeco_us;
        for quando in instantes.iter().copied().chain([fim]) {
            let intervalo = quando.saturating_sub(antes);
            maior = maior.max(intervalo);
            if intervalo > CONGELADO.as_micros() as u64 {
                congelado += intervalo;
            }
            antes = quando;
        }
        let mut voz: Vec<f64> = m
            .voz_atrasos_us
            .iter()
            .map(|a| *a as f64 / 1000.0)
            .collect();
        voz.sort_by(f64::total_cmp);
        let ida_total = m.ida.repassados + m.ida.descartados();
        Self {
            p50: percentil(&latencias, 0.50),
            p95: percentil(&latencias, 0.95),
            p99: percentil(&latencias, 0.99),
            max: latencias.last().copied().unwrap_or(f64::NAN),
            congelado: 100.0 * congelado as f64 / m.janela_us as f64,
            maior_congelamento: maior as f64 / 1000.0,
            qps: latencias.len() as f64 / (m.janela_us as f64 / 1e6),
            gerados: m.gerados as f64,
            chaves: m.chaves as f64,
            pedidos: m.pedidos as f64,
            megabytes: m.bytes_udp as f64 / 1e6,
            j_descartado: (m.j_descartado_no_emissor + m.j_recusado) as f64,
            voz_perda: 100.0
                * (1.0 - voz.len() as f64 / (m.voz_enviada + m.voz_recusada).max(1) as f64),
            voz_p50: percentil(&voz, 0.50),
            voz_p95: percentil(&voz, 0.95),
            perda_do_cano: 100.0 * m.ida.perdidos_no_sorteio as f64 / ida_total.max(1) as f64,
            fila_do_cano: 100.0 * m.ida.descartados_na_fila as f64 / ida_total.max(1) as f64,
        }
    }

    fn valores(&self) -> [f64; COLUNAS] {
        [
            self.p50,
            self.p95,
            self.p99,
            self.max,
            self.congelado,
            self.maior_congelamento,
            self.qps,
            self.gerados,
            self.chaves,
            self.pedidos,
            self.megabytes,
            self.j_descartado,
            self.voz_perda,
            self.voz_p50,
            self.voz_p95,
            self.perda_do_cano,
            self.fila_do_cano,
        ]
    }
}

const CABECALHO: &str = "| lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | chaves | pedidos | MB UDP | j desc. emissor | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |";
const REGUA: &str = "|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|";
/// As casas decimais de cada coluna.
const CASAS: [usize; COLUNAS] = [0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 2, 0, 2, 0, 0, 2, 2];

fn celula(valor: f64, casas: usize) -> String {
    if valor.is_nan() {
        "—".to_owned()
    } else {
        format!("{valor:.casas$}").replace('.', ",")
    }
}

fn em_colunas(valores: &[f64; COLUNAS], so_voz: bool) -> String {
    valores
        .iter()
        .zip(CASAS)
        .enumerate()
        .map(|(i, (v, casas))| {
            // Sem vídeo, as colunas do vídeo não dizem nada.
            if so_voz && (i < 10 || i == 11) {
                "—".to_owned()
            } else {
                celula(*v, casas)
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn mediana(mut valores: Vec<f64>) -> f64 {
    valores.retain(|v| !v.is_nan());
    valores.sort_by(f64::total_cmp);
    if valores.is_empty() {
        f64::NAN
    } else {
        valores[(valores.len() - 1) / 2]
    }
}

fn faixa(valores: &[f64], casas: usize) -> String {
    let validos: Vec<f64> = valores.iter().copied().filter(|v| !v.is_nan()).collect();
    if validos.is_empty() {
        return "—".to_owned();
    }
    let menor = validos.iter().copied().fold(f64::INFINITY, f64::min);
    let maior = validos.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    format!("{}–{}", celula(menor, casas), celula(maior, casas))
}

fn opcoes() -> (Opcoes, Option<String>, Option<f64>) {
    let mut op = Opcoes {
        segundos: 20,
        rodadas: 3,
        semente: 20_261_110,
        ritmo_bps: 4_000_000,
        em_serie: false,
    };
    let mut so = None;
    let mut perda = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut numero = |nome: &str| -> u64 {
            args.next()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| panic!("{nome} pede um número"))
        };
        match arg.as_str() {
            "--segundos" => op.segundos = numero("--segundos"),
            "--rodadas" => op.rodadas = numero("--rodadas"),
            "--semente" => op.semente = numero("--semente"),
            "--ritmo-bps" => op.ritmo_bps = numero("--ritmo-bps"),
            "--em-serie" => op.em_serie = true,
            "--so" => so = args.next(),
            "--perda" => perda = args.next().and_then(|v| v.parse().ok()),
            outro => panic!("opção desconhecida: {outro}"),
        }
    }
    (op, so, perda)
}

#[tokio::main]
async fn main() {
    let (op, so, perda) = opcoes();
    let cenarios: Vec<Cenario> = match perda {
        // Um cenário só, fora da grade: a perda dada, 30 ms, sem limite de banda.
        Some(perda) => vec![Cenario {
            nome: Box::leak(format!("{}%", perda * 100.0).into_boxed_str()),
            perda,
            banda_bps: None,
        }],
        None => CENARIOS
            .into_iter()
            .filter(|c| so.as_ref().is_none_or(|s| c.nome.contains(s.as_str())))
            .collect(),
    };
    eprintln!("{op:?}, {} cenários", cenarios.len());
    let mut resultados: BTreeMap<(usize, Transporte), Vec<Linha>> = BTreeMap::new();

    println!("## Por rodada\n");
    println!("| cenário | transporte | rodada {CABECALHO}");
    println!("|---|---|--:{REGUA}");
    for rodada in 0..op.rodadas {
        for (indice, cenario) in cenarios.iter().enumerate() {
            let semente = op.semente + rodada;
            let mut medidas = Vec::new();
            if op.em_serie {
                for transporte in Transporte::TODOS {
                    let m = tokio::spawn(medir(*cenario, transporte, semente, op)).await;
                    medidas.push((transporte, m));
                }
            } else {
                let tarefas: Vec<_> = Transporte::TODOS
                    .into_iter()
                    .map(|t| (t, tokio::spawn(medir(*cenario, t, semente, op))))
                    .collect();
                for (transporte, tarefa) in tarefas {
                    medidas.push((transporte, tarefa.await));
                }
            }
            for (transporte, medida) in medidas {
                match medida {
                    Ok(m) => {
                        let linha = Linha::de(&m);
                        println!(
                            "| {} | {} | {} | {} |",
                            cenario.nome,
                            transporte.nome(),
                            rodada + 1,
                            em_colunas(&linha.valores(), transporte == Transporte::SoVoz)
                        );
                        resultados
                            .entry((indice, transporte))
                            .or_default()
                            .push(linha);
                    }
                    Err(erro) => println!(
                        "| {} | {} | {} | falhou: {erro} |",
                        cenario.nome,
                        transporte.nome(),
                        rodada + 1
                    ),
                }
            }
        }
    }

    println!("\n## Mediana das rodadas\n");
    println!("| cenário | transporte {CABECALHO}");
    println!("|---|---{REGUA}");
    for ((indice, transporte), linhas) in &resultados {
        let mut medianas = [f64::NAN; COLUNAS];
        for (coluna, m) in medianas.iter_mut().enumerate() {
            *m = mediana(linhas.iter().map(|l| l.valores()[coluna]).collect());
        }
        println!(
            "| {} | {} | {} |",
            cenarios[*indice].nome,
            transporte.nome(),
            em_colunas(&medianas, *transporte == Transporte::SoVoz)
        );
    }

    println!("\n## Menor e maior valor entre as rodadas\n");
    println!("| cenário | transporte | lat p50 | lat p95 | congelado % | quadros/s | pedidos | voz perda % | voz p95 |");
    println!("|---|---|--:|--:|--:|--:|--:|--:|--:|");
    for ((indice, transporte), linhas) in &resultados {
        let coluna = |c: usize| -> String {
            let valores: Vec<f64> = linhas.iter().map(|l| l.valores()[c]).collect();
            faixa(&valores, CASAS[c])
        };
        let video = |c: usize| -> String {
            if *transporte == Transporte::SoVoz {
                "—".to_owned()
            } else {
                coluna(c)
            }
        };
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            cenarios[*indice].nome,
            transporte.nome(),
            video(0),
            video(1),
            video(4),
            video(6),
            video(9),
            coluna(12),
            coluna(14),
        );
    }
}
