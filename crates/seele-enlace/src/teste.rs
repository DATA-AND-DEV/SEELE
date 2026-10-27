//! Ajudantes para os testes de quem usa o enlace: um par de verdade em loopback.
//!
//! Não é simulação: dois sockets UDP em 127.0.0.1, ICE, DTLS e SCTP inteiros. É
//! o que o `seele-core` e o `seele-server` usam onde antes montavam um par quinn
//! em processo.

use std::net::SocketAddr;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use crate::{CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, Pino, Ponta};

/// Um par aberto: (quem ligou, o anfitrião), e as duas pontas, que precisam
/// viver enquanto o par for usado.
pub struct ParDeTeste {
    /// O lado de quem ligou.
    pub cliente: Conexao,
    /// O lado do anfitrião.
    pub anfitriao: Conexao,
    /// A ponta de quem ligou.
    pub ponta_do_cliente: Ponta,
    /// A ponta do anfitrião.
    pub ponta_do_anfitriao: Ponta,
    /// O certificado do anfitrião.
    pub certificado: CertificadoDtls,
}

/// Abre um par em loopback, com um certificado efêmero no anfitrião.
///
/// # Panics
///
/// Se o par não abrir: é ajudante de teste.
#[allow(clippy::expect_used)]
pub async fn par_em_loopback() -> ParDeTeste {
    let certificado = CertificadoDtls::efemero().expect("certificado de teste");
    par_com(ConfigDoAnfitriao::novo(certificado.clone()), certificado).await
}

/// O mesmo, com a configuração do anfitrião dada.
///
/// # Panics
///
/// Se o par não abrir.
#[allow(clippy::expect_used)]
pub async fn par_com(config: ConfigDoAnfitriao, certificado: CertificadoDtls) -> ParDeTeste {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 0));
    let ponta_do_anfitriao = Ponta::abrir(loopback, Some(config))
        .await
        .expect("ponta do anfitrião");
    let ponta_do_cliente = Ponta::abrir(loopback, None)
        .await
        .expect("ponta do cliente");
    let discagem = Discagem::nova(
        vec![ponta_do_anfitriao.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    let (cliente, anfitriao) = tokio::join!(
        ponta_do_cliente.discar(discagem),
        ponta_do_anfitriao.aceitar()
    );
    ParDeTeste {
        cliente: cliente.expect("a discagem de teste falhou"),
        anfitriao: anfitriao.expect("o anfitrião de teste não aceitou"),
        ponta_do_cliente,
        ponta_do_anfitriao,
        certificado,
    }
}

/// Segura (`true`) ou solta (`false`) os anúncios de crédito (`0x04`) que esta
/// conexão manda ao par: é como um teste atrasa o primeiro crédito, ou o nega.
/// Quem escreve do outro lado espera; fim, corte e apresentação continuam saindo.
pub fn reter_credito(conexao: &Conexao, reter: bool) {
    conexao.reter_credito(reter);
}

/// O que os canais de volume desta conexão seguravam na última revisão do
/// crédito: fila não lida mais crédito concedido e não usado. Nunca passa de
/// [`crate::MAX_CREDITO_DA_CONEXAO`].
#[must_use]
pub fn credito_em_aberto(conexao: &Conexao) -> u64 {
    conexao.credito_em_aberto()
}

/// A barreira de teste entre a soma na fila e o envio de uma mensagem de dados
/// (ver [`pausar_a_proxima_entrega`]).
#[derive(Default)]
pub(crate) struct PausaDeTeste {
    armada: AtomicBool,
    canais: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
}

impl PausaDeTeste {
    /// Na tarefa do enlace: se armada, avisa que parou e espera ser solta (no
    /// máximo 10 s, para um teste quebrado não travar a tarefa para sempre).
    pub(crate) fn talvez_parar(&self) {
        if !self.armada.swap(false, Ordering::AcqRel) {
            return;
        }
        let par = self.canais.lock().ok().and_then(|mut c| c.take());
        if let Some((parou, solta)) = par {
            let _ = parou.send(());
            let _ = solta.recv_timeout(Duration::from_secs(10));
        }
    }
}

/// A próxima entrega de dados desta conexão parou na barreira.
pub struct Pausa {
    parou: mpsc::Receiver<()>,
    solta: mpsc::Sender<()>,
}

impl Pausa {
    /// Espera a tarefa do enlace parar na barreira (bloqueia a thread: chame de
    /// `spawn_blocking`, num runtime de várias threads).
    ///
    /// # Panics
    ///
    /// Se ela não parar em 10 s.
    #[allow(clippy::expect_used)]
    pub fn esperar_parar(&self) {
        self.parou
            .recv_timeout(Duration::from_secs(10))
            .expect("a entrega não chegou à barreira");
    }

    /// Solta a tarefa do enlace.
    pub fn soltar(self) {
        let _ = self.solta.send(());
    }
}

/// Arma a barreira: a próxima mensagem de dados que chegar a esta conexão para
/// **depois** de somada na fila e **antes** de entrar nela. É como um teste
/// monta, sem sorte, a corrida entre a tarefa do enlace e quem larga o `Recebe`
/// (`stop`/`drop`) numa runtime de várias threads.
#[must_use]
pub fn pausar_a_proxima_entrega(conexao: &Conexao) -> Pausa {
    let (parou_tx, parou_rx) = mpsc::channel();
    let (solta_tx, solta_rx) = mpsc::channel();
    let pausa = conexao.pausa_de_teste();
    if let Ok(mut canais) = pausa.canais.lock() {
        *canais = Some((parou_tx, solta_rx));
    }
    pausa.armada.store(true, Ordering::Release);
    Pausa {
        parou: parou_rx,
        solta: solta_tx,
    }
}
