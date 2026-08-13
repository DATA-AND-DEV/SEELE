//! O enlace com um Dogma, incluindo o que fazer quando ele cai.
//!
//! [`Client`] é uma conexão: enquanto ela existe, funciona; quando cai, acaba.
//! Isto é a **sessão**, que é outra coisa — ela atravessa quedas. É aqui que
//! mora a bateria interna de `specs/07-tema-evangelion.md`:
//!
//! > Quando a conexão cai, o cliente não fecha nem mostra um spinner. Ele entra
//! > em bateria interna: contagem regressiva de 5 minutos, tentativas de
//! > reconexão listadas, interface esmaecida mas ainda legível.
//!
//! # Por que isto existia pela metade
//!
//! [`crate::Battery`] estava escrita e testada. A TUI sabia desenhar a tela.
//! Nada chamava uma coisa da outra: `Battery::new` não aparecia fora do próprio
//! módulo, e ao cair o cliente ia direto para "ENLACE PERDIDO". Cada peça
//! correta, a junção ausente — que é o tipo de falha que teste de unidade não
//! pega, porque cada unidade passa.
//!
//! # Por que uma tarefa, e não um objeto que a casca conduz
//!
//! As duas cascas chamam o cliente de dentro de um `tokio::select!`, e o
//! `select!` cancela quem perde a corrida. Ler já foi resolvido assim — uma
//! tarefa dona do fluxo entregando por canal. **Escrever tem o mesmo problema**:
//! `frame::write` faz dois `write_all`, e cancelado entre eles deixa meio
//! quadro no fio. Um `Enlace` que a casca conduzisse teria que escrever de
//! dentro do `select!`, e reintroduziria o defeito pelo outro lado.
//!
//! Então a conexão inteira mora numa tarefa. A casca fala por comandos e ouve
//! por avisos, e as duas pontas são canais — seguros de cancelar por contrato.
//!
//! # O que a reconexão restaura, e o que não
//!
//! Restaura o Cage, a Linha, o A.T. Field e o isolamento: é o que a pessoa
//! escolheu, e voltar sem isso seria voltar para outro lugar. **Não** restaura
//! a voz sozinha — a conexão é nova, e com ela o canal de mídia. A casca recebe
//! [`Aviso::Reconectado`] com o canal novo e reabre o áudio. É honesto: o
//! caminho de áudio realmente recomeça.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use seele_proto::control::ServerMessage;
use seele_proto::ids::{CageId, ClientMessageId, LineId};
use tokio::sync::mpsc;

use crate::battery::{Action, Battery, Link};
use crate::client::{Client, ConnectError, MediaChannel, SessionInfo};
use crate::tofu::PinDecision;
use crate::tofu::PinStore;
use crate::tofu::{verdict, Verdict};

/// Onde ficar batendo, e com que credencial.
#[derive(Debug, Clone)]
pub struct Destino {
    /// Endereço do Dogma.
    pub servidor: SocketAddr,
    /// O nome que o TLS recebe. Ver [`Client::connect`].
    pub nome_tls: String,
    /// Sob que chave o pin é arquivado. Ver [`Client::connect`].
    pub chave_do_pin: String,
    /// Como aparecer no roster.
    pub apelido: String,
    /// Convite de uso único ou senha do Dogma.
    pub segredo: Option<String>,
    /// A impressão digital que o convite prometeu, quando veio de um link.
    ///
    /// `None` para quem digitou o endereço à mão — aí não há o que conferir, e
    /// o primeiro contato segue sendo cego, como sempre foi.
    pub impressao_esperada: Option<String>,
}

/// O que a casca precisa saber.
pub enum Aviso {
    /// O Dogma disse algo.
    Mensagem(Box<ServerMessage>),
    /// Onde o enlace está, e quanto resta da bateria.
    ///
    /// Repetido a cada tica enquanto a bateria corre, porque a contagem
    /// regressiva **é** a informação: `specs/07-tema-evangelion.md` pede 04:59
    /// descendo na tela, e um número que só muda quando o estado muda ficaria
    /// parado exatamente durante os cinco minutos em que ele importa.
    Estado {
        /// Online, na bateria, ou descarregado.
        estado: Link,
        /// Quanto falta dos cinco minutos. `None` quando online.
        restante: Option<Duration>,
    },
    /// A conexão voltou, e com ela um canal de mídia novo.
    Reconectado {
        /// O canal de voz da conexão nova.
        media: Box<MediaChannel>,
        /// A sessão nova. O `ssrc` muda a cada conexão (falha G1).
        sessao: Box<SessionInfo>,
    },
    /// Acabou. Ou os cinco minutos passaram, ou não vale a pena tentar.
    Encerrado(Motivo),
}

impl std::fmt::Debug for Aviso {
    /// À mão porque [`MediaChannel`] embrulha uma conexão do quinn, que não
    /// tem `Debug`. O que interessa num log é qual aviso é, não o socket.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mensagem(mensagem) => f.debug_tuple("Mensagem").field(mensagem).finish(),
            Self::Estado { estado, restante } => f
                .debug_struct("Estado")
                .field("estado", estado)
                .field("restante", restante)
                .finish(),
            Self::Reconectado { sessao, .. } => f
                .debug_struct("Reconectado")
                .field("sessao", sessao)
                .finish(),
            Self::Encerrado(motivo) => f.debug_tuple("Encerrado").field(motivo).finish(),
        }
    }
}

/// Por que a sessão acabou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motivo {
    /// A bateria interna descarregou: cinco minutos sem reconectar.
    Descarregou,
    /// O Dogma recusou, e insistir não muda a resposta.
    Recusado(String),
    /// Alguém pediu para sair.
    Pedido,
}

/// O que a casca manda fazer.
#[derive(Debug)]
enum Comando {
    InserirPlug(CageId),
    EjetarPlug,
    AbrirLinha(LineId),
    Dizer {
        linha: LineId,
        corpo: String,
        id: ClientMessageId,
    },
    Historico {
        linha: LineId,
        limite: u16,
    },
    AtField(bool),
    Isolamento(bool),
    Sair,
}

/// A sessão com um Dogma, viva através de quedas.
pub struct Enlace {
    comandos: mpsc::Sender<Comando>,
    avisos: mpsc::UnboundedReceiver<Aviso>,
    /// O que se sabia na última conexão.
    sessao: SessionInfo,
    media: MediaChannel,
    estado: Link,
    /// Quanto resta dos cinco minutos, atualizado a cada aviso.
    restante: Option<Duration>,
    /// O que o TOFU decidiu no primeiro contato. ADR 0003.
    pin: PinDecision,
    /// O que a conferência com o convite concluiu. ADR 0006.
    veredito: Verdict,
    /// O último tempo de ida e volta, em microssegundos. Zero é desconhecido.
    ///
    /// Um átomo e não um aviso: a barra de telemetria lê isto quatro vezes por
    /// segundo, e transformar cada medição num aviso encheria a fila de coisas
    /// que ninguém precisa ver acontecer — só ver o valor atual.
    rtt: Arc<std::sync::atomic::AtomicU64>,
    tarefa: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for Enlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enlace")
            .field("estado", &self.estado)
            .field("dogma", &self.sessao.dogma)
            .finish()
    }
}

/// Fila de comandos. Controle é raro; isto é folga, não capacidade.
const COMANDOS: usize = 32;

impl Enlace {
    /// Conecta pela primeira vez.
    ///
    /// A primeira conexão falha para fora: quem não conseguiu entrar não tem
    /// sessão para segurar, e uma bateria interna antes de haver sessão seria
    /// uma contagem regressiva para reconectar a lugar nenhum.
    ///
    /// # Errors
    ///
    /// Devolve o motivo de não ter conseguido conectar, incluindo
    /// [`ConnectError::ConviteNaoConfere`] quando o link prometia outra
    /// identidade.
    pub async fn conectar(
        destino: Destino,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        let mut cliente = Client::connect(
            destino.servidor,
            &destino.nome_tls,
            &destino.chave_do_pin,
            &destino.apelido,
            &chave,
            Arc::clone(&pins),
            destino.segredo.as_deref(),
        )
        .await?;

        let pin = cliente.pin_decision().clone();
        let veredito = verdict(&pin, destino.impressao_esperada.as_deref());

        // O efeito vem **antes** de qualquer saída, e por isso está aqui e não
        // depois do `if`: o verificador fixa a chave dentro do retorno de
        // chamada do TLS, então devolver o erro sem desfazer o pin deixaria a
        // visita seguinte — sem link para conferir — ver `Matches` e entrar sem
        // hesitar no servidor que acabou de ser rejeitado.
        aplicar_veredito(&veredito, pins.as_ref(), &destino.chave_do_pin);

        if let Verdict::InviteRefused { expected, offered } = &veredito {
            // Derrubar, não só relatar: seguir conectado a quem o convite
            // desmente é exatamente o que a conferência existe para impedir.
            let erro = ConnectError::ConviteNaoConfere {
                esperada: expected.clone(),
                oferecida: offered.clone(),
            };
            cliente.disconnect();
            return Err(erro);
        }

        let sessao = cliente.session().clone();
        let media = cliente.media();
        let rtt = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let (comandos_tx, comandos_rx) = mpsc::channel(COMANDOS);
        let (avisos_tx, avisos_rx) = mpsc::unbounded_channel();

        let motor = Motor {
            destino,
            chave,
            pins,
            cliente: Some(cliente),
            bateria: Battery::new(),
            inicio: Instant::now(),
            cage: None,
            linha: None,
            at_field: false,
            isolamento: false,
            avisos: avisos_tx,
            rtt: Arc::clone(&rtt),
        };
        let tarefa = tokio::spawn(motor.rodar(comandos_rx));

        Ok(Self {
            comandos: comandos_tx,
            avisos: avisos_rx,
            sessao,
            media,
            estado: Link::Online,
            restante: None,
            pin,
            veredito,
            rtt,
            tarefa,
        })
    }

    /// O próximo aviso.
    ///
    /// Seguro de cancelar: é um `recv` de canal. As cascas chamam isto dentro
    /// de um `select!`, e é essa propriedade que faz o resto do desenho ser o
    /// que é.
    pub async fn proximo(&mut self) -> Aviso {
        let aviso = self
            .avisos
            .recv()
            .await
            .unwrap_or(Aviso::Encerrado(Motivo::Pedido));

        match &aviso {
            Aviso::Estado { estado, restante } => {
                self.estado = *estado;
                self.restante = *restante;
            }
            Aviso::Reconectado { media, sessao } => {
                self.estado = Link::Online;
                self.restante = None;
                self.media = (**media).clone();
                self.sessao = (**sessao).clone();
            }
            _ => {}
        }
        aviso
    }

    /// Onde o enlace está.
    #[must_use]
    pub fn estado(&self) -> Link {
        self.estado
    }

    /// Quanto resta dos cinco minutos, enquanto a bateria corre.
    #[must_use]
    pub fn restante(&self) -> Option<Duration> {
        self.restante
    }

    /// O que o TOFU decidiu ao conectar. ADR 0003.
    #[must_use]
    pub fn pin_decision(&self) -> &PinDecision {
        &self.pin
    }

    /// O que a conferência de identidade concluiu nesta conexão.
    #[must_use]
    pub fn veredito(&self) -> &Verdict {
        &self.veredito
    }

    /// O último tempo de ida e volta medido.
    #[must_use]
    pub fn rtt(&self) -> Option<Duration> {
        match self.rtt.load(std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            micros => Some(Duration::from_micros(micros)),
        }
    }

    /// O que se sabe da sessão. Muda a cada reconexão.
    #[must_use]
    pub fn sessao(&self) -> &SessionInfo {
        &self.sessao
    }

    /// O canal de voz da conexão atual.
    #[must_use]
    pub fn media(&self) -> MediaChannel {
        self.media.clone()
    }

    /// Entra num Cage. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn inserir_plug(&self, cage: CageId) -> Result<(), Fechado> {
        self.mandar(Comando::InserirPlug(cage)).await
    }

    /// Sai do Cage.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn ejetar_plug(&self) -> Result<(), Fechado> {
        self.mandar(Comando::EjetarPlug).await
    }

    /// Abre uma Linha. Restaurada depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn abrir_linha(&self, linha: LineId) -> Result<(), Fechado> {
        self.mandar(Comando::AbrirLinha(linha)).await
    }

    /// Diz alguma coisa.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn dizer(
        &self,
        linha: LineId,
        corpo: String,
        id: ClientMessageId,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::Dizer { linha, corpo, id }).await
    }

    /// Pede histórico.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn historico(&self, linha: LineId, limite: u16) -> Result<(), Fechado> {
        self.mandar(Comando::Historico { linha, limite }).await
    }

    /// Liga ou desliga o A.T. Field. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn at_field(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::AtField(ligado)).await
    }

    /// Liga ou desliga o isolamento total. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn isolamento(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Isolamento(ligado)).await
    }

    /// Encerra por vontade própria.
    pub async fn sair(&self) {
        let _ = self.mandar(Comando::Sair).await;
    }

    async fn mandar(&self, comando: Comando) -> Result<(), Fechado> {
        self.comandos.send(comando).await.map_err(|_| Fechado)
    }
}

impl Drop for Enlace {
    fn drop(&mut self) {
        self.tarefa.abort();
    }
}

/// A sessão acabou; não há para quem mandar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fechado;

impl std::fmt::Display for Fechado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a sessão já acabou")
    }
}

impl std::error::Error for Fechado {}

// ------------------------------------------------------------------- o motor

/// O que roda na tarefa: a conexão, a bateria, e a política entre as duas.
struct Motor {
    destino: Destino,
    chave: SigningKey,
    pins: Arc<dyn PinStore>,
    cliente: Option<Client>,
    bateria: Battery,
    inicio: Instant,
    /// O que restaurar ao reconectar.
    cage: Option<CageId>,
    linha: Option<LineId>,
    at_field: bool,
    isolamento: bool,
    avisos: mpsc::UnboundedSender<Aviso>,
    rtt: Arc<std::sync::atomic::AtomicU64>,
}

/// De quanto em quanto tempo a bateria é consultada.
///
/// Menor que o intervalo de ping e muito menor que o menor backoff, para que
/// nem o ping nem uma tentativa de reconexão fiquem esperando a tica seguinte.
const TICA: Duration = Duration::from_millis(200);

impl Motor {
    async fn rodar(mut self, mut comandos: mpsc::Receiver<Comando>) {
        let mut tica = tokio::time::interval(TICA);
        tica.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            // Só há o que ler quando há conexão. Sem ela, a espera é o relógio.
            let houve_evento = match self.cliente.as_mut() {
                Some(cliente) => tokio::select! {
                    evento = cliente.next_event() => Some(evento),
                    comando = comandos.recv() => {
                        match comando {
                            Some(Comando::Sair) | None => return self.encerrar(Motivo::Pedido),
                            Some(comando) => { self.executar(comando).await; None }
                        }
                    }
                    _ = tica.tick() => None,
                },
                None => tokio::select! {
                    comando = comandos.recv() => {
                        match comando {
                            Some(Comando::Sair) | None => return self.encerrar(Motivo::Pedido),
                            // Guardado, não perdido: entrar num Cage durante a
                            // queda é uma intenção que vale quando voltar.
                            Some(comando) => { self.lembrar(&comando); None }
                        }
                    }
                    _ = tica.tick() => None,
                },
            };

            if let Some(evento) = houve_evento {
                match evento {
                    Ok(mensagem) => {
                        if matches!(mensagem, ServerMessage::Pong { .. }) {
                            self.bateria.on_pong();
                            if let Some(medido) = self.cliente.as_ref().and_then(Client::rtt) {
                                let micros = u64::try_from(medido.as_micros()).unwrap_or(u64::MAX);
                                self.rtt
                                    .store(micros.max(1), std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        let _ = self.avisos.send(Aviso::Mensagem(Box::new(mensagem)));
                    }
                    // O fluxo caiu. Não é o fim da sessão: é o começo da
                    // bateria.
                    Err(erro) => {
                        tracing::debug!(%erro, "o enlace caiu");
                        self.cair();
                    }
                }
            }

            if self.passo().await {
                return;
            }
        }
    }

    /// Um passo da bateria. Devolve `true` quando a sessão acabou.
    async fn passo(&mut self) -> bool {
        let agora = self.inicio.elapsed();
        match self.bateria.poll(agora) {
            Action::SendPing => {
                if let Some(cliente) = self.cliente.as_mut() {
                    if cliente.send_ping().await.is_err() {
                        self.cair();
                    }
                }
            }
            Action::Reconnect => self.tentar().await,
            Action::EndSession => {
                self.encerrar(Motivo::Descarregou);
                return true;
            }
            Action::Wait => {}
        }

        // A contagem desce mesmo quando nada acontece, que é a maior parte do
        // tempo em que ela é vista.
        if matches!(self.bateria.state(), Link::InternalBattery { .. }) {
            self.anunciar();
        }
        false
    }

    /// A conexão morreu. Entra na bateria e conta para a casca.
    fn cair(&mut self) {
        self.cliente = None;
        let agora = self.inicio.elapsed();
        let antes = self.bateria.state();
        self.bateria.on_connection_lost(agora);
        if antes != self.bateria.state() {
            self.anunciar();
        }
    }

    /// Conta à casca onde o enlace está e quanto falta.
    fn anunciar(&mut self) {
        let agora = self.inicio.elapsed();
        let _ = self.avisos.send(Aviso::Estado {
            estado: self.bateria.state(),
            restante: self.bateria.remaining(agora),
        });
    }

    /// Uma tentativa de reconexão.
    ///
    /// Bloqueia a tarefa enquanto tenta, e isso é aceitável **aqui**: não há
    /// conexão para ler, e os comandos que chegarem esperam na fila em vez de
    /// se perder. O que não podia acontecer é isto rodar dentro do `select!` da
    /// casca, e não roda.
    async fn tentar(&mut self) {
        let resultado = Client::connect(
            self.destino.servidor,
            &self.destino.nome_tls,
            &self.destino.chave_do_pin,
            &self.destino.apelido,
            &self.chave,
            Arc::clone(&self.pins),
            self.destino.segredo.as_deref(),
        )
        .await;

        let agora = self.inicio.elapsed();
        match resultado {
            Ok(mut cliente) => {
                // Restaurar antes de anunciar. Uma casca que recebesse
                // "reconectado" e perguntasse o Cage antes de ele existir veria
                // uma sala vazia e acharia que perdeu gente.
                if let Some(cage) = self.cage {
                    let _ = cliente.insert_plug(cage).await;
                }
                if let Some(linha) = self.linha {
                    let _ = cliente.join_line(linha).await;
                }
                if self.at_field {
                    let _ = cliente.set_at_field(true).await;
                }
                if self.isolamento {
                    let _ = cliente.set_total_isolation(true).await;
                }

                let sessao = cliente.session().clone();
                let media = cliente.media();
                self.cliente = Some(cliente);
                self.bateria.on_reconnected();

                let _ = self.avisos.send(Aviso::Reconectado {
                    media: Box::new(media),
                    sessao: Box::new(sessao),
                });
            }
            // Uma recusa não melhora com insistência, e insistir contra uma
            // credencial rejeitada é a diferença entre reconectar e martelar.
            Err(erro) if !vale_insistir(&erro) => {
                self.encerrar(Motivo::Recusado(format!("{erro:?}")));
            }
            Err(erro) => {
                tracing::debug!(?erro, "tentativa de reconexão falhou");
                self.bateria.on_reconnect_failed(agora);
                self.anunciar();
            }
        }
    }

    async fn executar(&mut self, comando: Comando) {
        self.lembrar(&comando);
        let Some(cliente) = self.cliente.as_mut() else {
            return;
        };
        let resultado = match comando {
            Comando::InserirPlug(cage) => cliente.insert_plug(cage).await,
            Comando::EjetarPlug => cliente.eject_plug().await,
            Comando::AbrirLinha(linha) => cliente.join_line(linha).await,
            Comando::Dizer { linha, corpo, id } => cliente.send_message(linha, &corpo, id).await,
            Comando::Historico { linha, limite } => {
                cliente.fetch_history(linha, None, limite).await
            }
            Comando::AtField(ligado) => cliente.set_at_field(ligado).await,
            Comando::Isolamento(ligado) => cliente.set_total_isolation(ligado).await,
            Comando::Sair => return,
        };
        if resultado.is_err() {
            self.cair();
        }
    }

    /// Guarda o que a reconexão vai ter que refazer.
    fn lembrar(&mut self, comando: &Comando) {
        match comando {
            Comando::InserirPlug(cage) => self.cage = Some(*cage),
            Comando::EjetarPlug => self.cage = None,
            Comando::AbrirLinha(linha) => self.linha = Some(*linha),
            Comando::AtField(ligado) => self.at_field = *ligado,
            Comando::Isolamento(ligado) => self.isolamento = *ligado,
            _ => {}
        }
    }

    fn encerrar(&mut self, motivo: Motivo) {
        if let Some(mut cliente) = self.cliente.take() {
            cliente.disconnect();
        }
        let _ = self.avisos.send(Aviso::Encerrado(motivo));
    }
}

/// Aplica o que o veredito manda fazer com o pin.
///
/// Separado da decisão porque a decisão é uma tabela pura e isto é um efeito.
/// Só a recusa tem efeito: ela desfaz o pin que o verificador escreveu antes
/// de alguém poder julgar.
fn aplicar_veredito(veredito: &Verdict, pins: &dyn PinStore, chave_do_pin: &str) {
    if matches!(veredito, Verdict::InviteRefused { .. }) {
        pins.unpin(chave_do_pin);
    }
}

/// Se insistir pode dar em outra coisa.
///
/// Uma credencial rejeitada ou um banimento não mudam de resposta por
/// repetição; uma queda de rede muda.
fn vale_insistir(erro: &ConnectError) -> bool {
    !matches!(
        erro,
        ConnectError::PinChanged { .. }
            | ConnectError::Refused { .. }
            | ConnectError::ConviteNaoConfere { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insistir_contra_recusa_nao_muda_a_resposta() {
        // A diferença entre reconectar e martelar. Uma chave trocada é o alerta
        // do ADR 0003 e tentar de novo só o repetiria a cada backoff.
        assert!(!vale_insistir(&ConnectError::PinChanged {
            pinned: "a".into(),
            offered: "b".into(),
        }));
        // Um convite que não confere também não melhora com repetição: seria o
        // mesmo link errado contra o mesmo servidor a cada backoff.
        assert!(!vale_insistir(&ConnectError::ConviteNaoConfere {
            esperada: "a".into(),
            oferecida: "b".into(),
        }));
        assert!(vale_insistir(&ConnectError::Unreachable));
        assert!(vale_insistir(&ConnectError::HandshakeTimeout));
    }

    #[test]
    fn uma_recusa_desfaz_o_pin_que_o_verificador_acabou_de_escrever() {
        // Sem isto a recusa é decorativa: a visita seguinte, sem link para
        // conferir, veria `Matches` e entraria no servidor recém-rejeitado.
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());

        let decisao = PinDecision::FirstContact {
            fingerprint: "aaaa1111".into(),
        };
        let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

        aplicar_veredito(&veredito, &loja, "casa");

        assert_eq!(loja.pinned("casa"), None, "a recusa deixou o pin para trás");
    }

    #[test]
    fn um_veredito_que_nao_recusa_deixa_o_pin_onde_esta() {
        let loja = crate::tofu::MemoryPinStore::new();
        loja.pin("casa", "aaaa1111".into());

        let decisao = PinDecision::Matches {
            fingerprint: "aaaa1111".into(),
        };
        let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

        aplicar_veredito(&veredito, &loja, "casa");

        assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
    }

    #[test]
    fn o_que_a_reconexao_restaura_e_o_que_a_pessoa_escolheu() {
        let mut motor = motor_de_teste();

        motor.lembrar(&Comando::InserirPlug(CageId(2)));
        motor.lembrar(&Comando::AbrirLinha(LineId(7)));
        motor.lembrar(&Comando::AtField(true));
        motor.lembrar(&Comando::Isolamento(true));

        assert_eq!(motor.cage, Some(CageId(2)));
        assert_eq!(motor.linha, Some(LineId(7)));
        assert!(motor.at_field);
        assert!(motor.isolamento);

        // Ejetar não é uma queda: quem saiu do Cage não volta para ele.
        motor.lembrar(&Comando::EjetarPlug);
        assert_eq!(motor.cage, None);
    }

    #[test]
    fn dizer_nao_e_lembrado() {
        // Só estado é restaurado. Reenviar mensagens numa reconexão duplicaria
        // o que a pessoa disse, e a idempotência do protocolo protege contra
        // reenvio do **mesmo** identificador, não contra este erro.
        let mut motor = motor_de_teste();
        motor.lembrar(&Comando::Dizer {
            linha: LineId(1),
            corpo: "oi".into(),
            id: ClientMessageId(1),
        });
        assert_eq!(motor.linha, None);
    }

    fn motor_de_teste() -> Motor {
        let (avisos, _) = mpsc::unbounded_channel();
        Motor {
            destino: Destino {
                servidor: "127.0.0.1:1".parse().expect("endereço"),
                nome_tls: "localhost".into(),
                chave_do_pin: "127.0.0.1:1".into(),
                apelido: "piloto".into(),
                segredo: None,
                impressao_esperada: None,
            },
            chave: SigningKey::from_bytes(&[7; 32]),
            pins: Arc::new(crate::tofu::MemoryPinStore::new()),
            cliente: None,
            bateria: Battery::new(),
            inicio: Instant::now(),
            cage: None,
            linha: None,
            at_field: false,
            isolamento: false,
            avisos,
            rtt: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}
