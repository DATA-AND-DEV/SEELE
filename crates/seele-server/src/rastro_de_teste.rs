//! O que os testes deste crate que leem o log precisam para não depender da
//! ordem das threads (pendência 56).
//!
//! Cada teste que lê o log monta o próprio assinante (`tracing_subscriber::fmt`
//! com um escritor que guarda o texto) e o põe na thread corrente com
//! `with_default` ou `set_default`. Antes disso, chama [`segundo_despachante`]:
//! sem ele, a linha que o teste procura pode sumir quando a bateria roda em
//! paralelo.
//!
//! Quem confere que toda captura do crate chama o auxiliar, inclusive as que
//! ainda não existem, é o guarda `xtask/tests/log_com_segundo_despachante.rs`.
//! O teste de integração `tests/desconhecida.rs` é outro binário, que este
//! módulo não alcança, e leva a própria cópia; o mesmo guarda confere que ela,
//! esta e as outras cópias têm o corpo da do app, que é a referência.

/// **Um segundo despachante, vivo o processo inteiro, que não escreve nada.**
/// É ele que tira da captura a dependência da ordem das threads. O mesmo
/// auxiliar do app (`apps/seele-app/src/main.rs`, `rastro_de_teste`).
///
/// O `tracing` decide uma vez só, na primeira passagem de qualquer thread, se
/// uma linha que escreve (o *callsite*) tem quem a ouça, e guarda a resposta.
/// Se, no último registro de despachante, só havia um vivo, a pergunta vai só
/// ao despachante da **thread que passou** (`tracing-core` 0.1.36,
/// `callsite.rs`: `Dispatchers::rebuilder` devolve `Rebuilder::JustOne`, e ele
/// consulta `dispatcher::get_default`). Um teste sem captura que passasse
/// primeiro por uma linha, com a captura de outro teste como único despachante
/// de pé, a deixava marcada como «nunca» — e a mesma linha, no teste que a
/// captura, não escrevia nada. Foi o que a F7 mediu na recepção: 3 em 3
/// rodadas do módulo vermelhas sem ele.
///
/// Com dois vivos, a pergunta vai a todos, e uma linha que a captura ouve fica
/// em «às vezes»: cada evento pergunta de novo, à thread em que acontece. O
/// que uma captura vê não muda — só o que a thread dela escreveu. Chamar
/// `rebuild_interest_cache` ao abrir a captura não basta: a outra thread pode
/// passar pela linha depois, com a captura já aberta (a F7 mediu 10 em 10
/// vermelhas assim).
pub(crate) fn segundo_despachante() {
    static SEMPRE: std::sync::OnceLock<tracing::Dispatch> = std::sync::OnceLock::new();
    SEMPRE.get_or_init(|| tracing::Dispatch::new(tracing::subscriber::NoSubscriber::new()));
}

/// Onde o formatador da prova escreve, dividido com quem lê depois.
#[derive(Clone, Default)]
struct Captura(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captura {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("o rastro capturado")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Marca o processo filho da prova abaixo: com ela, a prova roda a
/// [`ordem_forcada`] em vez de chamar a si mesma.
const PROVA_NO_FILHO: &str = "PROVA56_FILHO";

/// **Uma linha que outra thread tocou primeiro, fora de qualquer captura,
/// ainda é capturada.** A prova de ordem forçada da pendência 56, a do app, com
/// a captura que os testes deste crate montam, rodada num processo filho.
///
/// É a ordem que fazia o teste da recepção falhar por sorte: um teste sem
/// captura passava pela linha antes do teste que a captura, e a linha sumia do
/// rastro deste. A [`ordem_forcada`] põe a outra thread na linha primeiro, com
/// a captura já de pé.
///
/// **Por que num processo filho.** A ordem forçada só decide quando a captura
/// dela é o único despachante vivo: é aí que o `tracing` pergunta só à thread
/// que passou. Rodada junto com outros testes de log deste crate, cujas
/// capturas ficam de pé ao mesmo tempo, ela passava mesmo sem o auxiliar
/// (medido em 09/10, ao lado de `alcance::encontro` e `mods::pedidos`). O filho
/// roda só ela (`--exact`, `--test-threads=1`), e ali a captura é a única. O
/// pai exige `1 passed`, porque um `--exact` que não acha o teste sai com
/// sucesso e `0 passed`, e a prova passaria sem ter rodado.
#[test]
fn uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada() {
    if std::env::var_os(PROVA_NO_FILHO).is_some() {
        return ordem_forcada();
    }
    let saida =
        std::process::Command::new(std::env::current_exe().expect("o binário destes testes"))
            .args([
                "--exact",
                "rastro_de_teste::uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada",
                "--test-threads=1",
            ])
            .env(PROVA_NO_FILHO, "1")
            .output()
            .expect("o processo filho da prova não subiu");
    let fala = String::from_utf8_lossy(&saida.stdout);
    assert!(
        saida.status.success() && fala.contains("ok. 1 passed;"),
        "a ordem forçada reprovou no processo filho, onde a captura é o único despachante \
         vivo, ou o filtro não achou a prova e ela não rodou: {fala}{}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

/// A outra thread toca a linha primeiro, com a captura já de pé, e a linha
/// ainda chega à captura. E a linha da outra thread continua fora: a captura
/// segue sendo só da thread corrente.
///
/// Só decide com a captura dela como único despachante vivo, e por isso roda no
/// processo filho da prova acima.
fn ordem_forcada() {
    fn diz() {
        tracing::warn!("linha tocada primeiro fora da captura");
    }
    segundo_despachante();
    let captura = Captura::default();
    let escreve = captura.clone();
    let assinante = tracing_subscriber::fmt()
        .with_writer(move || escreve.clone())
        .without_time()
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(assinante, || {
        std::thread::spawn(diz)
            .join()
            .expect("a outra thread terminou");
        diz();
    });

    let rastro = String::from_utf8(captura.0.lock().expect("o rastro capturado").clone())
        .expect("o rastro é UTF-8");
    assert!(
        rastro.contains("linha tocada primeiro fora da captura"),
        "uma linha que outra thread tocou primeiro sumiu do rastro, e um guarda que a capture \
         passa ou falha conforme a ordem das threads: {rastro}"
    );
    assert_eq!(
        rastro.lines().count(),
        1,
        "a linha da outra thread entrou no rastro desta captura, e um teste passaria a ler o \
         que outro teste escreveu: {rastro}"
    );
}
