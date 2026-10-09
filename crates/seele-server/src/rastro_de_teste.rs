//! O que os testes deste crate que leem o log precisam para não depender da
//! ordem das threads (pendência 56).
//!
//! Cada teste que lê o log monta o próprio assinante (`tracing_subscriber::fmt`
//! com um escritor que guarda o texto) e o põe na thread corrente com
//! `with_default` ou `set_default`. Antes disso, chama [`segundo_despachante`]:
//! sem ele, a linha que o teste procura pode sumir quando a bateria roda em
//! paralelo.
//!
//! Quem chama hoje: `admissao.rs`, `mods/pedidos.rs` (`o_registro_da_recusa`),
//! `alcance/encontro.rs` (`Rastro::de_info`) e `recepcao.rs`. O teste de
//! integração `tests/desconhecida.rs` é outro binário, que este módulo não
//! alcança, e leva a própria cópia.

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

/// **Uma linha que outra thread tocou primeiro, fora de qualquer captura,
/// ainda é capturada.** A prova de ordem forçada da pendência 56, a mesma do
/// app, com a captura que os testes deste crate montam.
///
/// É a ordem que fazia o teste da recepção falhar por sorte: um teste sem
/// captura passava pela linha antes do teste que a captura, e a linha sumia do
/// rastro deste. Aqui a ordem é forçada — a outra thread toca a linha
/// primeiro, com a captura já de pé —, e o resultado deixa de depender de quem
/// chega antes. E a linha da outra thread continua fora: a captura segue sendo
/// só da thread corrente.
#[test]
fn uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada() {
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
