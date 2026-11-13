use std::io::Write;
use std::sync::{Arc, Mutex};

/// Onde o formatador escreve, dividido com quem lê depois.
#[derive(Clone, Default)]
struct Captura(Arc<Mutex<Vec<u8>>>);

impl Write for Captura {
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

/// (e) Um pânico numa thread com nome deixa a linha, com o nome e a
/// mensagem.
///
/// O assinante é por thread (`with_default`), e por isso ele é posto
/// **dentro** da thread que entra em pânico: o gancho roda nela, antes de a
/// pilha desenrolar, e escreve no assinante dela. É o caso da thread que
/// abre o áudio e da thread da voz — threads soltas, cujo pânico antes não
/// deixava rastro nenhum no `seele.log`.
///
/// O segundo despachante vem antes, como em `rastro_de_teste::capturar`:
/// sem ele, a linha pode sumir quando a bateria roda em paralelo
/// (pendência 56).
#[test]
fn um_panico_numa_thread_deixa_linha_no_log() {
    crate::rastro_de_teste::segundo_despachante();
    super::instalar_gancho_de_panico();

    let captura = Captura::default();
    let escreve = captura.clone();
    let caiu = std::thread::Builder::new()
        .name("seele-teste-do-panico".into())
        .spawn(move || {
            let assinante = tracing_subscriber::fmt()
                .with_writer(move || escreve.clone())
                .with_max_level(tracing::Level::TRACE)
                .without_time()
                .with_ansi(false)
                .finish();
            tracing::subscriber::with_default(assinante, || {
                panic!("o driver caiu no meio da abertura");
            });
        })
        .expect("a thread do pânico proposital subiu")
        .join();
    assert!(caiu.is_err(), "o pânico proposital não aconteceu");

    let rastro = String::from_utf8(captura.0.lock().expect("o rastro").clone()).expect("UTF-8");
    assert!(
        rastro.contains("pânico:"),
        "um pânico numa thread não deixou linha no log: quem relata «o áudio \
             não abre» não tem o que mostrar. Rastro: {rastro:?}"
    );
    assert!(
        rastro.contains("o driver caiu no meio da abertura"),
        "a linha do pânico não diz a mensagem dele: {rastro:?}"
    );
    assert!(
        rastro.contains("seele-teste-do-panico"),
        "a linha do pânico não diz em que thread foi: {rastro:?}"
    );
}
