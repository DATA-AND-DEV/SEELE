//! **Um despertador fino para o laço da ponta.**
//!
//! O laço dorme até o próximo prazo do `str0m`, e com mídia esse prazo é o do
//! cadenciador de pacotes: poucos milissegundos. No Windows, o
//! `tokio::time::sleep_until` acorda no tique do relógio do sistema, que é de
//! 15,6 ms: os pacotes de vídeo saíam em rajadas a cada tique, a estimativa de
//! banda lia as rajadas como fila no caminho, e o vídeo ficava num terço da
//! taxa. Medido em 11/11/2026, servidor e dois clientes na mesma máquina
//! Windows: orçamento de 640 a 1004 kbps e 14,6 ms de jitter, contra 2790
//! kbps no Mac.
//!
//! O `std::thread::sleep` do Windows usa o temporizador de alta resolução do
//! sistema. Então uma linha própria dorme por ele e acorda o laço. Fora do
//! Windows o relógio do `tokio` já é de 1 ms, e a mesma linha serviria para
//! nada: lá o despertador não liga ninguém ([`Despertador::desligado`]).

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Notify;

/// Além disto o relógio do `tokio` dá conta, e a linha não é acordada.
pub(crate) const ALCANCE: Duration = Duration::from_millis(50);

/// O maior cochilo da linha entre duas olhadas na fila de prazos: um prazo
/// mais cedo que chega no meio de um cochilo espera no máximo isto.
const COCHILO: Duration = Duration::from_millis(2);

/// O despertador de um laço. Largá-lo encerra a linha.
#[derive(Debug)]
pub(crate) struct Despertador {
    prazos: Option<Sender<Instant>>,
    toque: Arc<Notify>,
    armado: Option<Instant>,
}

impl Despertador {
    /// O despertador desta plataforma: a linha no Windows, nenhuma fora dele.
    pub(crate) fn desta_plataforma() -> Self {
        if cfg!(windows) {
            Self::ligado()
        } else {
            Self::desligado()
        }
    }

    /// Com a linha própria. Se o sistema recusar a linha, fica desligado: o
    /// laço segue com o relógio do `tokio`, como sempre foi.
    pub(crate) fn ligado() -> Self {
        let (prazos, fila) = std::sync::mpsc::channel();
        let toque = Arc::new(Notify::new());
        let da_linha = Arc::clone(&toque);
        let subiu = std::thread::Builder::new()
            .name("seele-despertador".to_owned())
            .spawn(move || linha(&fila, &da_linha));
        match subiu {
            Ok(_) => Self {
                prazos: Some(prazos),
                toque,
                armado: None,
            },
            Err(erro) => {
                tracing::warn!(%erro, "a linha do despertador não subiu; o laço fica com o relógio do tokio");
                Self::desligado()
            }
        }
    }

    /// Sem linha: [`Self::tocou`] nunca termina.
    pub(crate) fn desligado() -> Self {
        Self {
            prazos: None,
            toque: Arc::new(Notify::new()),
            armado: None,
        }
    }

    /// Pede o toque em `quando`, se ele está ao [`ALCANCE`] de `agora`. O
    /// mesmo prazo pedido de novo não vai à linha outra vez.
    pub(crate) fn armar(&mut self, agora: Instant, quando: Instant) {
        let Some(prazos) = self.prazos.as_ref() else {
            return;
        };
        if quando.saturating_duration_since(agora) > ALCANCE || self.armado == Some(quando) {
            return;
        }
        if prazos.send(quando).is_ok() {
            self.armado = Some(quando);
        }
    }

    /// Termina quando a linha toca. Desligado, nunca.
    pub(crate) async fn tocou(&self) {
        if self.prazos.is_none() {
            std::future::pending::<()>().await;
        }
        self.toque.notified().await;
    }
}

/// A linha: espera um prazo, dorme até ele em cochilos curtos, toca. Um prazo
/// novo que chega no meio **substitui** o anterior: o laço só tem um próximo
/// acordar. Acaba quando o despertador é largado.
fn linha(fila: &Receiver<Instant>, toque: &Notify) {
    while let Ok(mut prazo) = fila.recv() {
        loop {
            loop {
                match fila.try_recv() {
                    Ok(novo) => prazo = novo,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            let falta = prazo.saturating_duration_since(Instant::now());
            if falta.is_zero() {
                toque.notify_one();
                break;
            }
            std::thread::sleep(falta.min(COCHILO));
        }
    }
}

#[cfg(test)]
mod testes {
    use super::{Despertador, ALCANCE};
    use std::time::{Duration, Instant};

    /// O toque vem perto do prazo pedido — nem antes, nem um tique de 15 ms
    /// depois. Roda em todo sistema; é no Windows que ele separa esta linha
    /// do relógio do `tokio`.
    #[tokio::test]
    async fn o_toque_vem_perto_do_prazo() {
        let mut despertador = Despertador::ligado();
        let mut atrasos = Vec::new();
        for _ in 0..20 {
            let agora = Instant::now();
            let prazo = agora + Duration::from_millis(3);
            despertador.armar(agora, prazo);
            tokio::time::timeout(Duration::from_secs(2), despertador.tocou())
                .await
                .expect("o despertador não tocou");
            let tocou = Instant::now();
            assert!(tocou >= prazo, "tocou antes do prazo");
            atrasos.push(tocou.duration_since(prazo));
        }
        atrasos.sort();
        let mediano = atrasos[atrasos.len() / 2];
        assert!(
            mediano < Duration::from_millis(5),
            "o toque mediano veio {mediano:?} depois do prazo: {atrasos:?}"
        );
    }

    /// Um prazo mais cedo, pedido com outro já armado, é o que vale.
    #[tokio::test]
    async fn o_prazo_mais_cedo_substitui_o_armado() {
        let mut despertador = Despertador::ligado();
        let agora = Instant::now();
        despertador.armar(agora, agora + Duration::from_millis(40));
        despertador.armar(agora, agora + Duration::from_millis(4));
        tokio::time::timeout(Duration::from_secs(2), despertador.tocou())
            .await
            .expect("o despertador não tocou");
        assert!(
            agora.elapsed() < Duration::from_millis(25),
            "esperou pelo prazo antigo"
        );
    }

    /// Além do alcance a linha não é acordada, e desligado ele nunca toca.
    #[tokio::test]
    async fn longe_ou_desligado_nao_toca() {
        let mut longe = Despertador::ligado();
        let agora = Instant::now();
        longe.armar(agora, agora + ALCANCE + Duration::from_millis(1));
        assert!(longe.armado.is_none());

        let mut desligado = Despertador::desligado();
        desligado.armar(agora, agora);
        let tocou = tokio::time::timeout(Duration::from_millis(30), desligado.tocou()).await;
        assert!(tocou.is_err(), "o despertador desligado tocou");
    }
}
