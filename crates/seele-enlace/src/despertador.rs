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
    use super::{Despertador, ALCANCE, COCHILO};
    use std::time::{Duration, Instant};

    /// O tique do relógio do sistema no Windows, que é o que o `tokio` enxerga
    /// lá: o defeito que este módulo existe para tirar do caminho.
    const TIQUE_DO_WINDOWS: Duration = Duration::from_micros(15_600);

    /// O prazo que o teste do toque pede, contado de agora.
    const PRAZO_DO_TESTE: Duration = Duration::from_millis(3);

    /// Até quanto um [`std::thread::sleep`] de um [`COCHILO`] pode estourar, na
    /// mediana, para a máquina contar como uma que **dorme fino**.
    ///
    /// A linha do despertador dorme por esse mesmo `sleep`: numa máquina em que
    /// ele acorda 10–20 ms depois do pedido (o macOS do runner do `ci.yml` em
    /// 12/11/2026), o despertador não tem como tocar perto do prazo, e o teste
    /// do toque não separaria a linha própria do relógio do `tokio` — os dois
    /// chegariam tarde igual.
    ///
    /// **O limite não é gosto**: com ele o teto ([`teto_do_toque`]) chega a 9 ms,
    /// e o tique do Windows atrasa o toque 12,6 ms. Um limite mais largo
    /// aproxima os dois, e `o_tique_do_windows_reprova_em_toda_maquina_que_dorme_fino`
    /// reprova quando eles se encontram.
    const SONO_FINO: Duration = Duration::from_millis(3);

    /// A folga do toque sobre o sono da máquina: o caminho da linha até o laço
    /// (o `Notify`, o despertar da runtime) e a troca de contexto.
    const FOLGA_DO_TOQUE: Duration = Duration::from_millis(3);

    /// O que a medida do toque diz, dado o que esta máquina consegue dormir.
    #[derive(Debug, PartialEq, Eq)]
    enum Toque {
        /// O toque veio perto do prazo, pela régua desta máquina.
        Perto,
        /// O toque veio mais tarde do que o sono desta máquina explica.
        Longe { teto: Duration },
        /// A máquina não dorme fino: aqui a medida não separa nada.
        SemSonoFino,
    }

    /// O teto do atraso mediano do toque, para uma máquina cujo sono puro
    /// estoura `estouro_do_sono` na mediana.
    ///
    /// **Calibrado, e não fixo.** O toque não pode vir antes do que o `sleep` da
    /// própria linha permite: até o prazo ela dorme no máximo dois cochilos
    /// seguidos, e cada um estoura o que a máquina estoura. Daí os dois estouros
    /// mais a folga. Um teto fixo de 5 ms reprovava o despertador pelo relógio
    /// do runner; um teto fixo e largo deixaria passar o tique de 15,6 ms, que é
    /// o que o teste existe para pegar.
    fn teto_do_toque(estouro_do_sono: Duration) -> Duration {
        estouro_do_sono * 2 + FOLGA_DO_TOQUE
    }

    /// Julga o atraso mediano do toque contra o estouro mediano do sono puro.
    fn julgar(estouro_do_sono: Duration, atraso_do_toque: Duration) -> Toque {
        if estouro_do_sono > SONO_FINO {
            return Toque::SemSonoFino;
        }
        let teto = teto_do_toque(estouro_do_sono);
        if atraso_do_toque < teto {
            Toque::Perto
        } else {
            Toque::Longe { teto }
        }
    }

    fn mediana(mut amostras: Vec<Duration>) -> Duration {
        amostras.sort();
        amostras[amostras.len() / 2]
    }

    /// **A calibração não pode matar o guarda.** Um despertador que dormisse
    /// pelo relógio do `tokio` no Windows tocaria no tique seguinte ao prazo:
    /// pedido logo depois de um tique, um prazo de 3 ms toca 12,6 ms tarde. Em
    /// toda máquina que conta como «dorme fino» — até o limite, inclusive — esse
    /// atraso tem de reprovar. Se alguém afrouxar a folga, o limite do sono fino
    /// ou a conta do teto a ponto de o tique caber, é este teste que diz, em
    /// qualquer sistema e sem depender de relógio nenhum.
    #[test]
    fn o_tique_do_windows_reprova_em_toda_maquina_que_dorme_fino() {
        let atraso_do_tique = TIQUE_DO_WINDOWS - PRAZO_DO_TESTE;
        for estouro in [
            Duration::ZERO,
            Duration::from_micros(300),
            Duration::from_millis(1),
            SONO_FINO,
        ] {
            assert!(
                matches!(julgar(estouro, atraso_do_tique), Toque::Longe { .. }),
                "com o sono estourando {estouro:?}, um toque {atraso_do_tique:?} depois do \
                 prazo (o tique do Windows) passou: o teto é de {:?}",
                teto_do_toque(estouro)
            );
            // E o toque de uma linha que dorme pelo `sleep` da máquina passa: um
            // cochilo estourado, mais o caminho até o laço.
            assert_eq!(
                julgar(estouro, estouro + Duration::from_millis(1)),
                Toque::Perto
            );
        }
        // Os números do macOS do runner em 12/11/2026: o sono puro acordava
        // 10–20 ms tarde e o toque mediano veio aos 13 ms. Ali a medida não
        // separa a linha própria do relógio do `tokio`, e o teste não julga.
        assert_eq!(
            julgar(Duration::from_millis(13), Duration::from_micros(13_114)),
            Toque::SemSonoFino
        );
    }

    /// O toque vem perto do prazo pedido — nem antes, nem um tique de 15 ms
    /// depois. Roda em todo sistema; é no Windows que ele separa esta linha
    /// do relógio do `tokio`.
    ///
    /// «Perto» é medido contra **o que esta máquina consegue dormir**: antes do
    /// toque, o teste mede o estouro de um `std::thread::sleep` de um cochilo,
    /// que é o sono da própria linha. Ver [`teto_do_toque`] e [`SONO_FINO`].
    #[tokio::test]
    async fn o_toque_vem_perto_do_prazo() {
        // O sono puro primeiro, e numa volta só dele: as voltas do toque têm de
        // ficar coladas uma na outra. É assim que, com um relógio de tique, cada
        // prazo é pedido logo depois de um tique e toca no seguinte — os 12,6 ms
        // que o teste procura. Um sono no meio das voltas mudaria essa fase e
        // encolheria o atraso que denuncia o tique.
        let mut sonos = Vec::new();
        for _ in 0..20 {
            let antes = Instant::now();
            std::thread::sleep(COCHILO);
            sonos.push(antes.elapsed().saturating_sub(COCHILO));
        }

        let mut despertador = Despertador::ligado();
        let mut atrasos = Vec::new();
        for _ in 0..20 {
            let agora = Instant::now();
            let prazo = agora + PRAZO_DO_TESTE;
            despertador.armar(agora, prazo);
            tokio::time::timeout(Duration::from_secs(2), despertador.tocou())
                .await
                .expect("o despertador não tocou");
            let tocou = Instant::now();
            assert!(tocou >= prazo, "tocou antes do prazo");
            atrasos.push(tocou.duration_since(prazo));
        }
        let estouro = mediana(sonos.clone());
        let mediano = mediana(atrasos.clone());
        atrasos.sort();
        sonos.sort();
        match julgar(estouro, mediano) {
            Toque::Perto => eprintln!(
                "o toque mediano veio {mediano:?} depois do prazo; um sleep de {COCHILO:?} \
                 estoura {estouro:?} aqui, e o teto era de {:?}",
                teto_do_toque(estouro)
            ),
            Toque::Longe { teto } => panic!(
                "o toque mediano veio {mediano:?} depois do prazo, e o teto desta máquina é de \
                 {teto:?} (um sleep de {COCHILO:?} estoura {estouro:?} aqui).\n  toques: \
                 {atrasos:?}\n  sonos: {sonos:?}"
            ),
            // No Windows o sono fino é a premissa do módulo inteiro: se o
            // `std::thread::sleep` não acorda fino lá, a linha própria não
            // adianta nada, e isso não pode sair na cor de quem passou.
            Toque::SemSonoFino if cfg!(windows) => panic!(
                "um std::thread::sleep de {COCHILO:?} estoura {estouro:?} na mediana nesta \
                 máquina Windows, e o despertador dorme por ele: a linha própria não está \
                 ganhando do tique do sistema.\n  toques: {atrasos:?}\n  sonos: {sonos:?}"
            ),
            Toque::SemSonoFino => eprintln!(
                "PULADO: esta máquina não dorme fino — um std::thread::sleep de {COCHILO:?} \
                 estoura {estouro:?} na mediana (o limite é {SONO_FINO:?}), e o despertador \
                 dorme por ele.\n  O toque mediano veio {mediano:?} depois do prazo, e aqui \
                 isso não separa a linha própria do relógio do tokio.\n  toques: \
                 {atrasos:?}\n  sonos: {sonos:?}"
            ),
        }
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
        // O teto é o próprio prazo antigo: quem esperasse por ele não tocaria
        // antes dos 40 ms. Um teto mais curto (eram 25 ms) media também o
        // relógio da máquina — no macOS do runner o toque de um prazo de 3 ms
        // chegou a vir 23 ms tarde (12/11/2026), e aqui isso reprovaria um
        // despertador que trocou de prazo como devia.
        let levou = agora.elapsed();
        assert!(
            levou < Duration::from_millis(40),
            "esperou pelo prazo antigo: tocou aos {levou:?}"
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
