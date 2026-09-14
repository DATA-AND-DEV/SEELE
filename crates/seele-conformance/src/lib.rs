//! End-to-end protocol conformance tests.
//!
//! This crate is deliberately empty of product code. Everything that is tested
//! lives in `tests/`, because the point is not to ship code but to hold the one
//! place where `seele-server` and `seele-core` are allowed to meet.
//!
//! ADR 0002 forbids either from depending on the other: the daemon must not link
//! the headless client, and the client must not link the daemon. An end-to-end
//! test needs both, so it gets its own crate rather than a hole in the rule.
//!
//! A única coisa que mora aqui é o portão de vagas — ver [`vaga`] —, porque ele
//! precisa ser o mesmo objeto para todos os binários de teste deste crate.

use std::fs::{File, OpenOptions};
use std::time::{Duration, Instant};

/// Quanto tempo se espera por uma vaga antes de seguir assim mesmo.
///
/// O portão é uma conveniência, não uma regra: se por qualquer motivo as vagas
/// não forem liberadas — um processo de teste pendurado de uma rodada anterior,
/// um sistema de arquivos sem trava —, é melhor a suíte correr saturada do que
/// travar para sempre esperando uma vaga que não vem.
const ESPERA_MAXIMA: Duration = Duration::from_secs(90);

/// De quanto em quanto tempo se varre o conjunto de vagas outra vez.
const INTERVALO: Duration = Duration::from_millis(20);

thread_local! {
    /// A vaga desta thread, se ela já tiver uma.
    ///
    /// É `thread_local` de propósito, e por dois motivos. O primeiro é a
    /// liberação: o `libtest` roda cada teste na sua própria thread, então a
    /// trava cai sozinha quando o teste termina, sem que ninguém precise
    /// devolvê-la à mão nem mudar a assinatura de nenhum ajudante. O segundo é
    /// `--test-threads=1`: nesse modo o `libtest` roda tudo na thread principal,
    /// e uma vaga por teste esgotaria o conjunto no enésimo teste e travaria a
    /// suíte. Como a vaga é da thread e não da chamada, ali ela é tomada uma vez
    /// e reusada por todos — que é exatamente o que se quer de uma execução que
    /// já é serial.
    static MINHA_VAGA: std::cell::RefCell<Option<File>> = const { std::cell::RefCell::new(None) };
}

/// Quantos servidores de conformidade podem existir ao mesmo tempo nesta máquina.
///
/// Um terço dos núcleos, nunca menos que dois. O número sai da medida da
/// pendência 29: cada teste daqui levanta um servidor QUIC de verdade — aperto
/// de mão, TLS e banco —, e em paralelo total o crate leva *mais* tempo do que
/// em série, porque o que se ganha em sobreposição se perde em contenção. Dois
/// é o piso para que a suíte não vire serial numa máquina pequena.
///
/// `SEELE_VAGAS` manda nisto quando está no ambiente, e `SEELE_VAGAS=0` desliga
/// o portão inteiro. Não é conforto de configuração: é o que torna a prova de
/// reversão deste conserto **reproduzível** por quem vier depois — rodar a
/// suíte com o portão desligado é rodar a suíte de antes dele, sem precisar
/// reverter nada nem confiar na palavra de quem mediu.
fn vagas() -> usize {
    if let Some(pedido) = std::env::var("SEELE_VAGAS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        return pedido;
    }
    std::thread::available_parallelism()
        .map(|n| (n.get() / 3).max(2))
        .unwrap_or(2)
}

/// Toma uma vaga para esta thread, esperando se todas estiverem ocupadas.
///
/// **Por que isto existe.** Pendência 29: `cargo test` do workspace reprovava um
/// teste de conformidade por rodada, sempre um diferente, sempre estourando um
/// prazo de ~20 s — e sempre passando sozinho em menos de um segundo. Não era
/// regressão nenhuma: o `cargo` roda os binários de teste em paralelo, cada
/// binário roda seus testes em paralelo de novo, e o prazo por candidato do
/// `connect` (`PRAZO_POR_CANDIDATO`, em `seele-core/src/enlace.rs`) é relógio de
/// parede. Numa máquina saturada ele queima sem que nada esteja quebrado, e uma
/// suíte que reprova ao acaso deixa de ser evidência.
///
/// **Por que trava de arquivo e não semáforo.** Um semáforo em memória só
/// enxergaria as threads do próprio binário, e a saturação vem sobretudo de
/// binários diferentes correndo ao mesmo tempo, que são processos diferentes. A
/// trava de arquivo é o único cadeado que os três sistemas operacionais
/// compartilham entre processos — e, melhor ainda, o sistema a devolve sozinho
/// quando o processo morre, inclusive se ele morrer de pânico.
///
/// Chamar duas vezes na mesma thread não toma duas vagas: a segunda chamada vê
/// que a thread já tem a sua e volta na hora.
///
/// **Chame daqui de dentro do corpo do teste, nunca de dentro de um
/// `tokio::spawn`.** Nos testes `#[tokio::test(flavor = "multi_thread")]` o
/// `block_on` conduz o futuro na própria thread do `libtest`, então uma chamada
/// aguardada do corpo do teste — ou de um ajudante que o corpo aguarda, que é o
/// caso de todos os pontos de chamada de hoje — acontece nessa thread: uma vaga
/// por teste, devolvida quando a thread acaba. Uma chamada de dentro de uma
/// tarefa disparada correria numa thread de trabalho, tomaria uma segunda vaga
/// para o mesmo teste e esperaria bloqueando dentro do executor. Como toda
/// chamada fica no preparo do servidor, antes do primeiro `spawn`, não há
/// tarefa alguma para essa espera atrasar.
pub fn vaga() {
    MINHA_VAGA.with(|minha| {
        let mut minha = minha.borrow_mut();
        if minha.is_some() {
            return;
        }
        *minha = tomar();
    });
}

/// Varre o conjunto de vagas até conseguir uma, ou até o tempo acabar.
fn tomar() -> Option<File> {
    let total = vagas();
    // Portão desligado — ver `SEELE_VAGAS`. Segue sem vaga em vez de esperar
    // por uma que não existe.
    if total == 0 {
        return None;
    }
    let pasta = std::env::temp_dir().join("seele-conformance-vagas");
    if std::fs::create_dir_all(&pasta).is_err() {
        return None;
    }
    let limite = Instant::now() + ESPERA_MAXIMA;
    loop {
        for indice in 0..total {
            let caminho = pasta.join(format!("vaga-{indice}.lock"));
            let Ok(arquivo) = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&caminho)
            else {
                // Sem poder abrir o arquivo não há portão; segue sem vaga.
                return None;
            };
            match arquivo.try_lock() {
                Ok(()) => return Some(arquivo),
                Err(std::fs::TryLockError::WouldBlock) => continue,
                // Sistema de arquivos sem trava (alguns NFS). Segue sem vaga,
                // que é o comportamento de antes deste portão.
                Err(std::fs::TryLockError::Error(_)) => return None,
            }
        }
        if Instant::now() >= limite {
            return None;
        }
        std::thread::sleep(INTERVALO);
    }
}
