//! **A imagem que chega já codificada, de fora deste processo**: a câmera.
//!
//! # Por que existe
//!
//! A [`crate::bomba`] captura a tela e a codifica, numa thread deste crate. A
//! câmera não tem esse caminho: este workspace proíbe `unsafe`, e não há
//! captura de câmera nativa sem ele. Quem tem a câmera é a janela — o
//! `getUserMedia` e o `VideoEncoder` do sistema, que ela alcança —, e o que
//! chega aqui são quadros H.264 em Annex-B, prontos.
//!
//! O que este módulo faz é entregar esses quadros **pelo mesmo canal** que a
//! bomba usa ([`EventoDaBomba`]), para que quem escreve no fio
//! ([`crate::bomba::escoar_com_espelho`]) seja um só: o cabeçalho de abertura,
//! o balde do teto, o quadro-chave em fatias e o pedido de chave valem para a
//! câmera sem uma segunda cópia de nenhum deles.
//!
//! # A fila tem tamanho, e diz quando encheu
//!
//! Quem empurra é um comando da janela, e quem escoa é a rede: os dois andam
//! em passos diferentes. A fila entre eles tem [`FILA_DE_FORA`] quadros, e um
//! quadro que não cabe é **recusado com nome**
//! ([`RecusaDeFora::FilaCheia`]) e contado. Descartá-lo calado quebraria a
//! imagem de todo mundo sem ninguém saber por quê: o quadro seguinte foi
//! predito do que não chegou. Com a recusa na mão, quem codifica faz do
//! próximo um quadro-chave.
//!
//! # O quadro-chave continua saindo em fatias
//!
//! O §3.3 do desenho da tela espalha o quadro-chave em
//! [`crate::tela::FATIAS_DO_QUADRO_CHAVE`] escritas, uma por tique, porque
//! escrevê-lo de uma vez custa à voz. Na bomba quem dá os tiques é a thread do
//! codificador; aqui é o relógio desta tarefa, no intervalo de um quadro. E um
//! quadro que chegue antes de as fatias acabarem **não é descartado**: as
//! fatias que faltam saem na frente dele, porque descartar um quadro que o
//! codificador de fora já produziu quebraria a predição do seguinte.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use seele_video::codec::Resolucao;
use tokio::sync::mpsc as canal;

use crate::bomba::{EventoDaBomba, Ordem, PedidoDeChave, QuadroParaOFio, CAPACIDADE_DO_CANAL};
use crate::tela::{MotivoDeParada, Teto, FATIAS_DO_QUADRO_CHAVE, MAX_QUADRO_LEN};

/// Quantos quadros podem esperar entre a janela e o fio.
///
/// Oito, o mesmo número do canal da bomba ([`CAPACIDADE_DO_CANAL`]) e pela
/// mesma conta: a 30 quadros por segundo são 266 ms de folga. Mais que isso é
/// a fila que o §1 do desenho recusa — imagem atrasada é pior que imagem
/// pulada.
pub const FILA_DE_FORA: usize = 8;

/// Por que um quadro de fora não entrou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecusaDeFora {
    /// A fila está cheia: a rede não está levando no passo de quem codifica.
    ///
    /// Quem codifica tem de fazer do próximo quadro um quadro-chave — este era
    /// referência do seguinte.
    #[error("the frame queue is full; {recusados} frames refused so far")]
    FilaCheia {
        /// Quantos quadros esta transmissão já recusou por fila cheia.
        recusados: u64,
    },
    /// Não há transmissão do outro lado: ela acabou, ou nunca começou.
    #[error("there is no transmission to receive this frame")]
    Fechada,
    /// Um quadro sem bytes.
    #[error("the frame is empty")]
    Vazio,
    /// Um quadro maior que o fio carrega ([`MAX_QUADRO_LEN`]).
    #[error("the frame has {tamanho} bytes, above what the wire carries")]
    GrandeDemais {
        /// O tamanho recusado.
        tamanho: usize,
    },
}

/// O que a transmissão pede a quem codifica do lado de fora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinalParaFora {
    /// O próximo quadro tem de ser um quadro-chave: alguém começou a assistir,
    /// ou um quadro foi descartado no caminho.
    Chave,
    /// O teto de banda de agora, em bits por segundo, cabeçalhos incluídos.
    ///
    /// É o teto do **fluxo**, e quem codifica mira abaixo dele: um codificador
    /// que mira exatamente no teto o estoura a cada quadro mais gordo, e o
    /// balde do fluxo descarta o que estoura.
    Banda {
        /// O teto.
        bps: u32,
    },
    /// O vídeo parou (§3.2), e volta sozinho com um [`Self::Banda`] novo.
    Parou(MotivoDeParada),
}

/// O que quem tem a conexão manda para a tarefa que cadencia.
#[derive(Debug, Clone, Copy)]
enum Controle {
    /// O teto andou.
    Teto(Teto),
    /// Alguém pediu um quadro-chave.
    Chave,
    /// Acabou.
    Parar,
}

/// Por onde a casca empurra os quadros da câmera.
///
/// Barato de copiar: são três punhos de `Arc`.
#[derive(Debug, Clone)]
pub struct EntradaDeFora {
    quadros: canal::UnboundedSender<(bool, Vec<u8>)>,
    na_fila: Arc<AtomicUsize>,
    recusados: Arc<AtomicU64>,
}

impl EntradaDeFora {
    /// Entrega um quadro codificado, em Annex-B.
    ///
    /// # Errors
    ///
    /// [`RecusaDeFora`], e nenhuma delas é silenciosa: a fila cheia é contada
    /// ([`Self::recusados`]) e volta com a contagem.
    pub fn quadro(&self, chave: bool, bytes: Vec<u8>) -> Result<(), RecusaDeFora> {
        if bytes.is_empty() {
            return Err(RecusaDeFora::Vazio);
        }
        if bytes.len() > MAX_QUADRO_LEN {
            return Err(RecusaDeFora::GrandeDemais {
                tamanho: bytes.len(),
            });
        }
        if self.quadros.is_closed() {
            return Err(RecusaDeFora::Fechada);
        }
        // Reservada antes de entrar, e devolvida se não coube: duas chamadas
        // ao mesmo tempo não passam as duas pela última vaga.
        if self.na_fila.fetch_add(1, Ordering::AcqRel) >= FILA_DE_FORA {
            self.na_fila.fetch_sub(1, Ordering::AcqRel);
            let recusados = self.recusados.fetch_add(1, Ordering::Relaxed) + 1;
            return Err(RecusaDeFora::FilaCheia { recusados });
        }
        if self.quadros.send((chave, bytes)).is_err() {
            self.na_fila.fetch_sub(1, Ordering::AcqRel);
            return Err(RecusaDeFora::Fechada);
        }
        Ok(())
    }

    /// Quantos quadros foram recusados por fila cheia desde o começo.
    #[must_use]
    pub fn recusados(&self) -> u64 {
        self.recusados.load(Ordering::Relaxed)
    }
}

/// O outro lado de [`EntradaDeFora`]: o que vai dentro do pedido de
/// transmissão ([`crate::PedidoDeTela::DeFora`]) e espera o servidor dar nome
/// a ela.
#[derive(Debug)]
pub struct QuadrosDeFora {
    quadros: canal::UnboundedReceiver<(bool, Vec<u8>)>,
    na_fila: Arc<AtomicUsize>,
    medidas: (u16, u16),
}

impl QuadrosDeFora {
    /// As medidas com que quem está de fora codifica: o que vai no cabeçalho
    /// de abertura.
    #[must_use]
    pub const fn medidas(&self) -> (u16, u16) {
        self.medidas
    }
}

/// O par: por onde os quadros entram, e o que o pedido de transmissão leva.
///
/// `largura` e `altura` são as do codificador de fora, em pixels — a abertura
/// do fluxo as declara, e quem assiste arma o decodificador por elas.
#[must_use]
pub fn canal_de_fora(largura: u16, altura: u16) -> (EntradaDeFora, QuadrosDeFora) {
    let (manda, recebe) = canal::unbounded_channel();
    let na_fila = Arc::new(AtomicUsize::new(0));
    (
        EntradaDeFora {
            quadros: manda,
            na_fila: Arc::clone(&na_fila),
            recusados: Arc::new(AtomicU64::new(0)),
        },
        QuadrosDeFora {
            quadros: recebe,
            na_fila,
            medidas: (largura, altura),
        },
    )
}

/// A alça de uma transmissão de fora que está no ar.
///
/// Largá-la a encerra, como a da bomba: a tarefa manda o
/// [`EventoDaBomba::Fim`], e quem escoa fecha o fluxo direito.
#[derive(Debug)]
pub struct AlcaDeFora {
    controle: canal::UnboundedSender<Controle>,
}

impl AlcaDeFora {
    /// Diz que o teto andou. `false` quando a tarefa já acabou.
    pub fn teto(&self, teto: Teto) -> bool {
        self.controle.send(Controle::Teto(teto)).is_ok()
    }

    /// Pede um quadro-chave a quem codifica. `false` quando a tarefa já acabou.
    pub fn chave(&self) -> bool {
        self.controle.send(Controle::Chave).is_ok()
    }

    /// Encerra. Não bloqueia: não há thread a juntar.
    pub fn parar(self) {
        let _ = self.controle.send(Controle::Parar);
    }
}

impl Drop for AlcaDeFora {
    fn drop(&mut self) {
        let _ = self.controle.send(Controle::Parar);
    }
}

/// O que [`ligar_de_fora`] devolve: a alça, o canal que quem escoa lê, e por
/// onde quem escoa pede um quadro-chave.
pub type LigadaDeFora = (AlcaDeFora, canal::Receiver<EventoDaBomba>, PedidoDeChave);

/// Põe a transmissão de fora no ar: abre o fluxo com o teto de agora e passa a
/// entregar os quadros que chegarem.
///
/// `intervalo` é o de um quadro, na cadência de quem codifica: é o passo em
/// que as fatias de um quadro-chave saem. `sinal` é chamado da tarefa, a cada
/// coisa que quem codifica precisa saber ([`SinalParaFora`]).
///
/// Tem de ser chamada de dentro de um runtime do `tokio`: a tarefa nasce aqui.
#[must_use]
pub fn ligar_de_fora(
    quadros: QuadrosDeFora,
    teto: Teto,
    intervalo: Duration,
    sinal: impl Fn(SinalParaFora) + Send + 'static,
) -> LigadaDeFora {
    let (manda_controle, recebe_controle) = canal::unbounded_channel();
    let (manda_evento, recebe_evento) = canal::channel(CAPACIDADE_DO_CANAL);
    let (pedido_de_chave, ordens) = PedidoDeChave::de_fora();
    tokio::spawn(cadenciar(Cadenciador {
        quadros,
        controle: recebe_controle,
        saida: manda_evento,
        ordens,
        intervalo,
        sinal: Box::new(sinal),
        teto,
        geracao: 0,
        sequencia: 0,
        no_ar: false,
        fatias_devidas: 0,
        espera_chave: true,
    }));
    (
        AlcaDeFora {
            controle: manda_controle,
        },
        recebe_evento,
        pedido_de_chave,
    )
}

/// O estado da tarefa que cadencia.
struct Cadenciador {
    quadros: QuadrosDeFora,
    controle: canal::UnboundedReceiver<Controle>,
    saida: canal::Sender<EventoDaBomba>,
    /// Os pedidos de quem escoa. Um canal síncrono, porque é o tipo que
    /// [`PedidoDeChave`] carrega para a bomba; lido sem esperar, a cada volta.
    ordens: std::sync::mpsc::Receiver<Ordem>,
    intervalo: Duration,
    sinal: Box<dyn Fn(SinalParaFora) + Send>,
    teto: Teto,
    geracao: u64,
    sequencia: u64,
    /// Há fluxo aberto. Falso enquanto o teto manda parar.
    no_ar: bool,
    /// Quantas fatias do último quadro-chave ainda não foram escritas.
    fatias_devidas: usize,
    /// O fluxo acabou de abrir e ainda não levou um quadro-chave.
    ///
    /// Quem assiste não decodifica nada antes dele, e um quadro comum que
    /// sobrou de antes de uma parada seria o primeiro do fluxo novo: fica de
    /// fora, e quem codifica ouve o pedido de chave.
    espera_chave: bool,
}

impl Cadenciador {
    /// Abre um fluxo novo com este teto, e diz a banda a quem codifica.
    async fn abrir(&mut self, bps: u32) -> bool {
        self.geracao += 1;
        self.fatias_devidas = 0;
        self.no_ar = true;
        self.espera_chave = true;
        let (largura, altura) = self.quadros.medidas;
        let evento = EventoDaBomba::Fluxo {
            geracao: self.geracao,
            // Quem escoa não lê o degrau, só as medidas: o degrau é o nome que
            // a bomba dá ao que ela mesma armou, e aqui quem armou está de fora.
            resolucao: Resolucao::P720,
            medidas: (usize::from(largura), usize::from(altura)),
            teto_bps: bps,
        };
        if self.saida.send(evento).await.is_err() {
            return false;
        }
        (self.sinal)(SinalParaFora::Banda { bps });
        true
    }

    /// O teto andou: a banda muda dentro do fluxo, a parada o fecha, e a volta
    /// abre outro.
    async fn teto_andou(&mut self, novo: Teto) -> bool {
        let antes = std::mem::replace(&mut self.teto, novo);
        match (self.no_ar, novo) {
            (true, Teto::Bps(bps)) => {
                if antes == novo {
                    return true;
                }
                if self
                    .saida
                    .send(EventoDaBomba::Teto { teto_bps: bps })
                    .await
                    .is_err()
                {
                    return false;
                }
                (self.sinal)(SinalParaFora::Banda { bps });
                true
            }
            (true, Teto::Parado(motivo)) => {
                self.no_ar = false;
                self.fatias_devidas = 0;
                (self.sinal)(SinalParaFora::Parou(motivo));
                self.saida.send(EventoDaBomba::Parou(motivo)).await.is_ok()
            }
            (false, Teto::Bps(bps)) => self.abrir(bps).await,
            (false, Teto::Parado(_)) => true,
        }
    }

    /// Escreve as fatias que o último quadro-chave ainda deve, todas.
    async fn quitar_as_fatias(&mut self) -> bool {
        while self.fatias_devidas > 0 {
            self.fatias_devidas -= 1;
            if self.saida.send(EventoDaBomba::Escoar).await.is_err() {
                return false;
            }
        }
        true
    }

    /// Um quadro que chegou de fora vai para quem escoa.
    async fn quadro(&mut self, chave: bool, bytes: Vec<u8>) -> bool {
        self.quadros.na_fila.fetch_sub(1, Ordering::AcqRel);
        if !self.no_ar {
            // O vídeo está parado pelo teto: não há fluxo. Quem codifica foi
            // avisado ([`SinalParaFora::Parou`]), e o que ainda vinha a caminho
            // não tem para onde ir.
            return true;
        }
        if self.espera_chave && !chave {
            (self.sinal)(SinalParaFora::Chave);
            return true;
        }
        self.espera_chave = false;
        if !self.quitar_as_fatias().await {
            return false;
        }
        self.sequencia += 1;
        let evento = EventoDaBomba::Quadro(QuadroParaOFio {
            sequencia: self.sequencia,
            geracao: self.geracao,
            chave,
            bytes,
        });
        if self.saida.send(evento).await.is_err() {
            return false;
        }
        if chave {
            self.fatias_devidas = FATIAS_DO_QUADRO_CHAVE.saturating_sub(1);
        }
        true
    }

    /// Os pedidos de quem escoa: só o de quadro-chave interessa a quem está de
    /// fora. As confirmações são da bomba, que segura o próprio codificador.
    fn ouvir_quem_escoa(&self) {
        let mut pediu = false;
        while let Ok(ordem) = self.ordens.try_recv() {
            pediu |= ordem == Ordem::Chave;
        }
        if pediu {
            (self.sinal)(SinalParaFora::Chave);
        }
    }
}

/// O laço: os quadros de fora, o controle de quem tem a conexão, e o relógio
/// das fatias.
async fn cadenciar(mut estado: Cadenciador) {
    let primeiro = estado.teto;
    let abriu = match primeiro {
        Teto::Bps(bps) => estado.abrir(bps).await,
        Teto::Parado(motivo) => {
            (estado.sinal)(SinalParaFora::Parou(motivo));
            true
        }
    };
    if !abriu {
        return;
    }
    let mut relogio = tokio::time::interval(estado.intervalo);
    relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let segue = tokio::select! {
            ordem = estado.controle.recv() => match ordem {
                Some(Controle::Teto(teto)) => estado.teto_andou(teto).await,
                Some(Controle::Chave) => {
                    (estado.sinal)(SinalParaFora::Chave);
                    true
                }
                Some(Controle::Parar) | None => false,
            },
            quadro = estado.quadros.quadros.recv() => match quadro {
                Some((chave, bytes)) => estado.quadro(chave, bytes).await,
                // Quem empurrava largou a entrada: não vem mais quadro.
                None => false,
            },
            _ = relogio.tick() => {
                if estado.fatias_devidas > 0 {
                    estado.fatias_devidas -= 1;
                    estado.saida.send(EventoDaBomba::Escoar).await.is_ok()
                } else {
                    true
                }
            }
        };
        estado.ouvir_quem_escoa();
        if !segue {
            break;
        }
    }
    // Fecha a porta de entrada antes de dizer o fim: um quadro empurrado
    // depois disto volta com [`RecusaDeFora::Fechada`], e não some numa fila
    // que ninguém mais lê.
    estado.quadros.quadros.close();
    let _ = estado.quitar_as_fatias().await;
    let _ = estado.saida.send(EventoDaBomba::Fim(None)).await;
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use std::sync::Mutex;

    type Sinais = Arc<Mutex<Vec<SinalParaFora>>>;

    fn ligada(teto: Teto) -> (EntradaDeFora, LigadaDeFora, Sinais) {
        let (entrada, quadros) = canal_de_fora(1280, 720);
        let sinais: Sinais = Arc::default();
        let para = Arc::clone(&sinais);
        let ligada = ligar_de_fora(quadros, teto, Duration::from_millis(33), move |sinal| {
            para.lock().unwrap().push(sinal);
        });
        (entrada, ligada, sinais)
    }

    async fn proximo(eventos: &mut canal::Receiver<EventoDaBomba>) -> EventoDaBomba {
        tokio::time::timeout(Duration::from_secs(5), eventos.recv())
            .await
            .expect("a transmissão de fora não disse nada em cinco segundos")
            .expect("o canal fechou antes do fim")
    }

    /// **O fluxo abre antes do primeiro quadro, com as medidas de quem
    /// codifica, e os quadros chegam na ordem e com os mesmos bytes.**
    #[tokio::test(start_paused = true)]
    async fn o_fluxo_abre_com_as_medidas_de_fora_e_os_quadros_saem_na_ordem() {
        let (entrada, (alca, mut eventos, _chave), sinais) = ligada(Teto::Bps(1_200_000));
        assert_eq!(
            proximo(&mut eventos).await,
            EventoDaBomba::Fluxo {
                geracao: 1,
                resolucao: Resolucao::P720,
                medidas: (1280, 720),
                teto_bps: 1_200_000,
            }
        );
        // Um quadro comum antes de qualquer quadro-chave não abre fluxo
        // nenhum para quem assiste: fica de fora, e quem codifica ouve.
        entrada.quadro(false, vec![0]).unwrap();
        entrada.quadro(true, vec![1, 2, 3]).unwrap();
        entrada.quadro(false, vec![4, 5]).unwrap();
        let mut saiu = Vec::new();
        while saiu.len() < 2 {
            match proximo(&mut eventos).await {
                EventoDaBomba::Quadro(quadro) => {
                    assert_eq!(quadro.geracao, 1);
                    saiu.push((quadro.chave, quadro.bytes));
                }
                EventoDaBomba::Escoar => {}
                outro => panic!("esperava um quadro, veio {outro:?}"),
            }
        }
        assert_eq!(saiu, vec![(true, vec![1, 2, 3]), (false, vec![4, 5])]);
        assert_eq!(
            sinais.lock().unwrap().as_slice(),
            &[
                SinalParaFora::Banda { bps: 1_200_000 },
                SinalParaFora::Chave
            ]
        );
        alca.parar();
        assert_eq!(proximo(&mut eventos).await, EventoDaBomba::Fim(None));
    }

    /// **Um quadro-chave sai em fatias no passo do relógio**, e o quadro que
    /// chega antes de elas acabarem não é descartado: as que faltam saem na
    /// frente dele.
    #[tokio::test(start_paused = true)]
    async fn o_quadro_chave_sai_em_fatias_e_o_quadro_seguinte_nao_e_descartado() {
        let (entrada, (_alca, mut eventos, _chave), _) = ligada(Teto::Bps(1_200_000));
        let _ = proximo(&mut eventos).await;

        entrada.quadro(true, vec![9; 64]).unwrap();
        assert!(matches!(
            proximo(&mut eventos).await,
            EventoDaBomba::Quadro(QuadroParaOFio { chave: true, .. })
        ));
        // Sozinho, o relógio escreve as três fatias que faltam.
        for _ in 1..FATIAS_DO_QUADRO_CHAVE {
            assert_eq!(proximo(&mut eventos).await, EventoDaBomba::Escoar);
        }

        // Outro quadro-chave, e desta vez um quadro comum chega logo atrás.
        entrada.quadro(true, vec![9; 64]).unwrap();
        entrada.quadro(false, vec![7]).unwrap();
        assert!(matches!(
            proximo(&mut eventos).await,
            EventoDaBomba::Quadro(QuadroParaOFio { chave: true, .. })
        ));
        let mut fatias = 0;
        loop {
            match proximo(&mut eventos).await {
                EventoDaBomba::Escoar => fatias += 1,
                EventoDaBomba::Quadro(quadro) => {
                    assert_eq!(quadro.bytes, vec![7]);
                    break;
                }
                outro => panic!("esperava fatia ou quadro, veio {outro:?}"),
            }
        }
        assert_eq!(
            fatias,
            FATIAS_DO_QUADRO_CHAVE - 1,
            "o quadro comum saiu antes de o quadro-chave fechar: quem lê o fluxo o leria como \
             pedaço do quadro-chave"
        );
    }

    /// **A fila cheia é dita e contada, e nunca calada.** Sem ninguém escoando,
    /// o nono quadro volta com o nome e a contagem — e a vaga que ele reservou
    /// é devolvida.
    #[tokio::test(start_paused = true)]
    async fn a_fila_cheia_recusa_com_nome_e_conta() {
        let (entrada, _quadros) = canal_de_fora(640, 360);
        for _ in 0..FILA_DE_FORA {
            entrada.quadro(false, vec![1]).unwrap();
        }
        assert_eq!(
            entrada.quadro(false, vec![1]),
            Err(RecusaDeFora::FilaCheia { recusados: 1 })
        );
        assert_eq!(
            entrada.quadro(true, vec![1]),
            Err(RecusaDeFora::FilaCheia { recusados: 2 })
        );
        assert_eq!(entrada.recusados(), 2);
    }

    /// As outras recusas, cada uma com o nome dela.
    #[tokio::test(start_paused = true)]
    async fn um_quadro_vazio_grande_demais_ou_sem_transmissao_e_recusado_com_nome() {
        let (entrada, quadros) = canal_de_fora(640, 360);
        assert_eq!(entrada.quadro(true, Vec::new()), Err(RecusaDeFora::Vazio));
        assert_eq!(
            entrada.quadro(true, vec![0; MAX_QUADRO_LEN + 1]),
            Err(RecusaDeFora::GrandeDemais {
                tamanho: MAX_QUADRO_LEN + 1
            })
        );
        drop(quadros);
        assert_eq!(entrada.quadro(true, vec![1]), Err(RecusaDeFora::Fechada));
    }

    /// **O pedido de chave chega a quem codifica pelos dois caminhos**: o de
    /// quem tem a conexão (alguém começou a assistir) e o de quem escoa (um
    /// quadro foi descartado).
    #[tokio::test(start_paused = true)]
    async fn o_pedido_de_chave_chega_a_quem_codifica_pelos_dois_caminhos() {
        let (_entrada, (alca, mut eventos, chave), sinais) = ligada(Teto::Bps(1_200_000));
        let _ = proximo(&mut eventos).await;

        assert!(alca.chave());
        assert!(chave.pedir());
        tokio::time::sleep(Duration::from_millis(100)).await;
        let vistos: Vec<SinalParaFora> = sinais.lock().unwrap().clone();
        assert_eq!(
            vistos
                .iter()
                .filter(|sinal| **sinal == SinalParaFora::Chave)
                .count(),
            2,
            "um dos dois pedidos de chave não chegou a quem codifica: {vistos:?}"
        );
    }

    /// **O teto que manda parar fecha o fluxo e avisa; o que volta abre outro.**
    /// Entre os dois, um quadro que ainda vinha a caminho não vai para o fio.
    #[tokio::test(start_paused = true)]
    async fn a_parada_fecha_o_fluxo_avisa_e_a_volta_abre_outro() {
        let (entrada, (alca, mut eventos, _chave), sinais) = ligada(Teto::Bps(1_200_000));
        let _ = proximo(&mut eventos).await;

        assert!(alca.teto(Teto::Bps(900_000)));
        assert_eq!(
            proximo(&mut eventos).await,
            EventoDaBomba::Teto { teto_bps: 900_000 }
        );

        assert!(alca.teto(Teto::Parado(MotivoDeParada::SinalCritico)));
        assert_eq!(
            proximo(&mut eventos).await,
            EventoDaBomba::Parou(MotivoDeParada::SinalCritico)
        );
        entrada.quadro(false, vec![1]).unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        assert!(alca.teto(Teto::Bps(700_000)));
        assert_eq!(
            proximo(&mut eventos).await,
            EventoDaBomba::Fluxo {
                geracao: 2,
                resolucao: Resolucao::P720,
                medidas: (1280, 720),
                teto_bps: 700_000,
            }
        );
        assert_eq!(
            sinais.lock().unwrap().as_slice(),
            &[
                SinalParaFora::Banda { bps: 1_200_000 },
                SinalParaFora::Banda { bps: 900_000 },
                SinalParaFora::Parou(MotivoDeParada::SinalCritico),
                SinalParaFora::Banda { bps: 700_000 },
            ]
        );
    }

    /// **Depois do fim, a porta diz que fechou.** Um quadro empurrado para uma
    /// transmissão que acabou não some numa fila que ninguém lê.
    #[tokio::test(start_paused = true)]
    async fn depois_do_fim_a_entrada_diz_que_fechou() {
        let (entrada, (alca, mut eventos, _chave), _) = ligada(Teto::Bps(1_200_000));
        let _ = proximo(&mut eventos).await;
        alca.parar();
        assert_eq!(proximo(&mut eventos).await, EventoDaBomba::Fim(None));
        assert_eq!(entrada.quadro(true, vec![1]), Err(RecusaDeFora::Fechada));
    }
}
