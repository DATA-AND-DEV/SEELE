//! A espera de [`esperar_o_audio`], pelo que ela **faz** com o relógio.
//!
//! A abertura acontece dentro do laço de `drive`, que roda num runtime de
//! uma thread só. Uma abertura síncrona presa no driver seguraria a conexão
//! inteira — comandos, mensagens, a telemetria — pelo tempo que o driver
//! quisesse. Estes testes não abrem aparelho nenhum: o abridor é uma função
//! que dorme, e o que se afirma é quanto a espera leva e o que ela devolve.

use super::{esperar_o_audio, Atraso};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Uma vez do driver nova, de uma sessão só deste teste.
fn vez() -> Arc<Mutex<()>> {
    Arc::new(Mutex::new(()))
}

/// Um abridor que conta quantas aberturas estão no driver ao mesmo tempo, e
/// quantas aconteceram.
#[derive(Default)]
struct Driver {
    em_voo: AtomicUsize,
    no_maximo: AtomicUsize,
    aberturas: AtomicUsize,
}

impl Driver {
    fn abrir(&self, demora: Duration) -> &'static str {
        self.aberturas.fetch_add(1, Ordering::SeqCst);
        let agora = self.em_voo.fetch_add(1, Ordering::SeqCst) + 1;
        self.no_maximo.fetch_max(agora, Ordering::SeqCst);
        std::thread::sleep(demora);
        self.em_voo.fetch_sub(1, Ordering::SeqCst);
        "a voz"
    }
}

/// M-c: duas aberturas da mesma sessão nunca ficam no driver ao mesmo
/// tempo. Entrar, sair e entrar de novo depressa larga a primeira, e a
/// thread dela segue no driver; a segunda espera a vez.
#[tokio::test(flavor = "multi_thread")]
async fn duas_aberturas_da_mesma_sessao_nunca_ficam_no_driver_ao_mesmo_tempo() {
    let vez = vez();
    let driver = Arc::new(Driver::default());
    let (primeiro, segundo) = (Arc::clone(&driver), Arc::clone(&driver));

    let (a, b) = tokio::join!(
        esperar_o_audio(
            &vez,
            move || primeiro.abrir(Duration::from_millis(150)),
            Duration::from_secs(5)
        ),
        esperar_o_audio(
            &vez,
            move || segundo.abrir(Duration::from_millis(150)),
            Duration::from_secs(5)
        ),
    );

    assert!(
        a.is_ok() && b.is_ok(),
        "as duas aberturas tinham tempo de sobra"
    );
    assert_eq!(
        driver.no_maximo.load(Ordering::SeqCst),
        1,
        "duas aberturas da mesma sessão ficaram no driver ao mesmo tempo"
    );
}

/// E a abertura de que ninguém espera mais desiste quando a vez chega, sem
/// abrir aparelho nenhum.
#[tokio::test(flavor = "multi_thread")]
async fn a_abertura_que_ja_desistiu_nao_abre_quando_chega_a_vez() {
    let vez = vez();
    let driver = Arc::new(Driver::default());
    let (primeiro, segundo) = (Arc::clone(&driver), Arc::clone(&driver));

    let (a, b) = tokio::join!(
        esperar_o_audio(
            &vez,
            move || primeiro.abrir(Duration::from_millis(300)),
            Duration::from_secs(5)
        ),
        async {
            // A segunda pede depois de a primeira tomar a vez, e desiste
            // antes de a primeira terminar.
            tokio::time::sleep(Duration::from_millis(50)).await;
            esperar_o_audio(
                &vez,
                move || segundo.abrir(Duration::from_millis(10)),
                Duration::from_millis(50),
            )
            .await
        },
    );
    assert!(a.is_ok());
    assert_eq!(b, Err(Atraso));

    // Dá tempo de a vez chegar à segunda thread.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        driver.aberturas.load(Ordering::SeqCst),
        1,
        "a abertura de que ninguém esperava mais abriu o aparelho quando chegou a vez"
    );
}

/// (d) da pendência 52: um driver preso não prende quem espera.
///
/// A asserção de tempo vem antes da do valor, e é de propósito: a versão
/// síncrona — abrir na própria thread de quem espera — leva os dois
/// segundos inteiros e devolve a voz, e é o relógio que a reprova, sem
/// pendurar a bateria.
#[tokio::test]
async fn um_audio_que_nao_abre_no_prazo_volta_atrasado_sem_esperar_o_driver() {
    let comeco = Instant::now();
    let veio = esperar_o_audio(
        &vez(),
        || {
            std::thread::sleep(Duration::from_secs(2));
            "a voz"
        },
        Duration::from_millis(200),
    )
    .await;
    let levou = comeco.elapsed();

    assert!(
        levou < Duration::from_secs(1),
        "a espera pelo áudio levou {levou:?} com um prazo de 200 ms: ela esperou o \
             driver, e um driver preso prende a conexão inteira junto"
    );
    assert_eq!(
        veio,
        Err(Atraso),
        "o prazo venceu e a espera não disse que o áudio não veio"
    );
}

#[tokio::test]
async fn a_voz_que_abre_no_prazo_chega_inteira_e_pela_thread_com_nome() {
    let veio = esperar_o_audio(
        &vez(),
        || std::thread::current().name().map(str::to_owned),
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(
        veio,
        Ok(Some("seele-abre-audio".to_owned())),
        "a abertura não correu na thread própria com nome: um pânico ali sai \
             no log sem dizer de onde"
    );
}

/// Uma voz que se larga, avisando que foi largada.
struct Aparelho(Arc<AtomicBool>);

impl Drop for Aparelho {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A voz que chega depois do prazo é largada na thread que a abriu.
///
/// É o `drop` que fecha o aparelho. Uma voz tardia guardada em algum lugar
/// seria um microfone aberto para uma sala que já desistiu dele.
#[tokio::test]
async fn a_voz_que_chega_tarde_e_largada_na_thread_que_a_abriu() {
    let largada = Arc::new(AtomicBool::new(false));
    let marca = Arc::clone(&largada);
    let veio = esperar_o_audio(
        &vez(),
        move || {
            std::thread::sleep(Duration::from_millis(300));
            Aparelho(marca)
        },
        Duration::from_millis(50),
    )
    .await;
    assert!(
        veio.is_err(),
        "o prazo de 50 ms venceu e a voz veio assim mesmo"
    );

    let ate = Instant::now() + Duration::from_secs(5);
    while !largada.load(Ordering::SeqCst) && Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        largada.load(Ordering::SeqCst),
        "a voz que chegou depois do prazo não foi largada: o aparelho fica aberto \
             para uma sala que já seguiu sem ele"
    );
}
