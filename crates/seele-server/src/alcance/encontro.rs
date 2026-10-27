//! Degrau 4 do ADR 0022: furo de NAT com ponto de encontro, **pelo ENC/3**
//! (ADR 0063, a linha 2; o fio do ENC/3 em `seele_proto::encontro` e em
//! `docs/ponto-de-encontro.md`).
//!
//! É o degrau que faz "manda o link e funciona" virar verdade numa casa com
//! CGNAT ou com o UPnP desligado — onde os degraus 2 e 3 não têm o que fazer e a
//! escada parava no 1.
//!
//! # O que acontece, na ordem
//!
//! 1. Pelo socket **do servidor** — o mesmo em que o WebRTC atende, a 8383 —,
//!    o anfitrião registra as duas marcas dele no quarto: `MORA3` num servidor
//!    guardado, `MORO3` assinado na sala pessoal. O ponto responde `DESAFIO`
//!    com um cookie, o registro sai de novo com ele, e o `MOROU` diz o
//!    endereço público deste socket: é ele que entra no convite como mais um
//!    candidato.
//! 2. Quem liga manda `CHAMA` ao ponto (o nativo por UDP, o navegador pelo
//!    `POST /sinal/1`), com a credencial ICE e até dois candidatos próprios.
//! 3. O ponto repassa um `CHAMADO` a este socket. O anfitrião **fura** (Binding
//!    Indications do socket 8383 para os candidatos, `Ponta::furar`) e responde
//!    `ATENDE` com o endereço público. O ponto devolve `AQUI` a quem ligou, e o
//!    ICE de quem liga entra pelo NAT recém-aberto.
//!
//! # Um socket só
//!
//! Até a 0.16 havia dois: o do servidor, que o quinn possuía e não emprestava
//! (daí o «espelho», um descritor só de escrita), e uma escuta de avisos
//! própria, que lia as respostas. O enlace da 0.17 é sans-IO e é dono do
//! socket: o que não é STUN nem DTLS chega por `Ponta::mensagens_enc` (RFC
//! 7983), e as duas marcas saem do mesmo socket, como o fio do ENC/3 prevê.
//!
//! # O que o anfitrião limita (R2 da revisão da 0.17)
//!
//! Os candidatos do `CHAMADO` vêm de quem liga e ninguém os autentica: um
//! terceiro os escolhe, e o anfitrião manda Indications a eles. Por isso:
//!
//! - só os que `encontro::serve_de_candidato` aceita, aqui **e** no enlace;
//! - no máximo [`CHAMADOS_POR_MINUTO`] chamados atendidos por minuto;
//! - e o teto do próprio enlace: até 3 candidatos, 5 Indications de 20 B cada,
//!   30 furos por minuto (`seele_enlace::{CANDIDATOS_POR_FURO,
//!   INDICACOES_POR_CANDIDATO, FUROS_POR_MINUTO}`).
//!
//! Só o ENC/3 sai daqui: nada do ENC/1 nem do ENC/2. Um ponto da 0.16 não
//! acha um servidor da 0.17, e essa é a quebra decidida (D6).
//!
//! # O que isto não faz, e não vai fazer
//!
//! **NAT simétrico dos dois lados não fura.** O srflx de quem liga é medido
//! contra o ponto, e com mapeamento dependente do destino ele não serve. A frase
//! do degrau 4 diz "deve funcionar" e não "funciona". **Nada disto foi medido
//! num NAT real**: os testes são em laço local.
//!
//! **Nada do que é dito passa por aqui.** O ponto de encontro apresenta e sai; o
//! DTLS e o TOFU do ADR 0003 continuam ponta a ponta, e a impressão digital
//! continua sendo conferida contra a do servidor. O que ele aprende é metadado —
//! que endereço falou com que endereço, e quando —, e isso está escrito em
//! `docs/alcance-pela-internet.md` em vez de ficar implícito.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use seele_proto::encontro::{self, Marca, Marcas, Resposta3};
use seele_proto::uri::Bilhete;

use crate::transporte::Ponta;
use crate::CaixaDoEnc;

/// O ponto de encontro do projeto, os endereços de reserva dele e a variável
/// que o troca moram em `seele_proto::encontro`, com a leitura do ambiente
/// (`ponto_escolhido`) e a regra que diz se um texto aponta para a produção
/// (`aponta_para_a_producao`): quem liga também precisa deles, e o
/// `seele-core` não vê o `seele-server` (ADR 0002).
///
/// **Padrão só do caminho de produção** (a decisão (d) do lote G-servidor da
/// F5): quem pede o [`PONTO_PADRAO`] é [`Convocacao::com_o_ponto_padrao`].
/// Sem a variável, [`Convocacao::do_ambiente`] não convoca ponto nenhum, e é
/// com ele que os testes sobem.
pub use seele_proto::encontro::{PONTO_PADRAO, VARIAVEL};

pub(crate) use seele_proto::encontro::REDE_DO_PADRAO;

/// Quanto tempo se espera pelo ponto de encontro antes de seguir a escada.
///
/// Um segundo: uma ida e volta a um servidor na internet custa entre 20 ms e
/// 200 ms numa rede doméstica, e isto roda entre apertar **HOSPEDAR AQUI** e a
/// sala abrir. Com o ponto fora do ar, todo anfitrião de rede difícil paga este
/// número inteiro, e paga exatamente no caminho que já vai terminar em más
/// notícias. O registro da 0.17 são duas idas e voltas (`DESAFIO`, depois
/// `MOROU`), e elas cabem.
pub const PRAZO: Duration = Duration::from_secs(1);

/// De quanto em quanto tempo o registro é repetido enquanto o prazo corre.
const REPETICAO: Duration = Duration::from_millis(300);

/// De quanto em quanto tempo o registro é reavivado.
///
/// Um mapeamento de NAT para UDP some sozinho depois de um tempo de silêncio, e
/// os roteadores mais apertados esquecem em 30 segundos. Quinze é metade disso
/// (o fio do ENC/3: «a cada 15 s»).
const REAVIVAR: Duration = Duration::from_secs(15);

/// Quantos `CHAMADO`s o anfitrião atende por minuto (R2).
///
/// Cada um custa até 3 × 5 Indications de 20 B a endereços que um terceiro
/// escolheu. Trinta por minuto é o mesmo teto de furos do enlace
/// (`seele_enlace::FUROS_POR_MINUTO`): uma entrada legítima é um `CHAMADO` por
/// tentativa, e uma casa com três sessões reconectando cabe com folga.
pub const CHAMADOS_POR_MINUTO: usize = 30;

/// A janela de [`CHAMADOS_POR_MINUTO`].
const JANELA_DOS_CHAMADOS: Duration = Duration::from_secs(60);

/// Por que o degrau 4 não deu.
///
/// Variantes, e não um texto, pelo mesmo motivo do degrau 3: o que a pessoa pode
/// fazer a respeito é diferente em cada uma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaNoEncontro {
    /// Ninguém pediu um ponto de encontro — `$SEELE_ENCONTRO=nao`.
    ///
    /// Não é falha de rede e não vira frase de erro: é uma escolha de quem
    /// hospeda, e ela é respeitada em silêncio.
    Desligado,
    /// O nome do ponto de encontro não resolve.
    NaoResolve(String),
    /// O ponto de encontro não respondeu ao registro no prazo: fora do ar,
    /// firewall de saída, uma rede que não deixa UDP sair, ou um ponto anterior
    /// ao ENC/3 (que cala para a magia nova).
    SemResposta,
    /// O ponto respondeu e recusou o registro.
    Recusado(encontro::Motivo),
    /// O ponto não aceitou o cookie que ele mesmo acabou de dar: esta máquina
    /// parece sair para a internet por mais de um endereço.
    CookieRecusado,
    /// O socket do servidor não entrega os datagramas do ponto a este degrau:
    /// outro encontro ainda está de pé nele.
    SemCaixa,
}

impl std::fmt::Display for FalhaNoEncontro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Desligado => write!(
                f,
                "o ponto de encontro está desligado nesta máquina ({VARIAVEL})"
            ),
            // Duas frases, e a diferença não é zelo: elas apontam para pessoas
            // diferentes. Quando o nome é o **nosso** e ele não resolve, a
            // causa é que ninguém o publicou ainda, e não é desta máquina.
            // Quando é um que a pessoa escolheu, a suspeita é do lado dela.
            Self::NaoResolve(nome) if nome == PONTO_PADRAO => write!(
                f,
                "o serviço que abre caminho pela internet ainda não está no ar — \
                 não é problema desta máquina"
            ),
            Self::NaoResolve(nome) => write!(
                f,
                "não achei «{nome}»: confira o endereço, ou esta máquina está sem \
                 internet"
            ),
            Self::SemResposta => write!(
                f,
                "o serviço não respondeu — pode estar fora do ar, ou esta rede \
                 pode estar bloqueando a saída"
            ),
            Self::Recusado(encontro::Motivo::Relogio) => write!(
                f,
                "o serviço recusou o registro porque o relógio desta máquina e o dele \
                 diferem demais: confira se a data e a hora estão automáticas"
            ),
            Self::Recusado(encontro::Motivo::Cheio) => {
                write!(f, "o serviço está cheio agora e não guardou este servidor")
            }
            Self::Recusado(encontro::Motivo::Taxa) => write!(
                f,
                "o serviço está recusando os registros desta rede por excesso; ele tenta \
                 de novo sozinho"
            ),
            Self::Recusado(encontro::Motivo::Velho) => write!(
                f,
                "o serviço tem um registro mais novo deste mesmo servidor, vindo de outro \
                 endereço"
            ),
            Self::CookieRecusado => write!(
                f,
                "o serviço não aceitou a resposta desta máquina: ela parece sair para a \
                 internet por mais de um endereço"
            ),
            Self::SemCaixa => write!(
                f,
                "o servidor já tem um registro no serviço de pé, e não abriu outro"
            ),
        }
    }
}

impl std::error::Error for FalhaNoEncontro {}

/// O socket do servidor, como o degrau 4 o usa: a ponta do enlace, para
/// escrever o ENC/3 e furar, e a caixa dos datagramas do ENC/3 que chegam a ela.
#[derive(Clone)]
pub struct PeloServidor {
    /// A ponta do servidor (`Daemon::ponta`).
    pub ponta: Ponta,
    /// O que chega a ela e não é WebRTC (`Daemon::mensagens_enc`).
    pub enc: CaixaDoEnc,
}

impl std::fmt::Debug for PeloServidor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeloServidor")
            .field("ponta", &self.ponta)
            .finish_non_exhaustive()
    }
}

/// O que o degrau 4 precisa saber para tentar.
///
/// Existe para o degrau ser **fácil de não usar**: um `None` em
/// [`super::Escada::subir`], e nenhum pacote sai desta máquina para ninguém.
#[derive(Debug)]
pub struct Convocacao {
    /// O socket em que o servidor atende. Tem de ser este e não outro: o NAT
    /// mapeia por porta interna, e o furo abre caminho para o socket de onde
    /// sai.
    pub pelo: PeloServidor,
    /// As marcas deste anfitrião no quarto, **prontas**: um servidor guardado
    /// as tira da impressão digital ([`Marcas::do_servidor`]); a sala, do
    /// código.
    pub marcas: Marcas,
    /// O endereço do ponto de encontro, como texto.
    pub ponto: String,
    /// A escuta do servidor: a porta e as famílias das dicas de rede local que
    /// vão no `ATENDE` (e no `MORO3` da sala).
    pub escuta: super::Escuta,
    /// Quem assina o registro: só a sala pessoal tem (`MORO3`). Um servidor
    /// guardado registra pelo `MORA3`, sem assinatura.
    pub assinante: Option<Assinante>,
}

impl Convocacao {
    /// A convocação da sala pessoal: as marcas saem do código da chave, e o
    /// registro sai assinado por ela, com o cookie do ponto (`MORO3`).
    #[must_use]
    pub fn para_sala(
        pelo: PeloServidor,
        escuta: super::Escuta,
        chave: ed25519_dalek::SigningKey,
        ponto: impl Into<String>,
    ) -> Self {
        let assinante = Assinante::novo(chave);
        Self {
            pelo,
            marcas: assinante.marcas(),
            ponto: ponto.into(),
            escuta,
            assinante: Some(assinante),
        }
    }

    /// Se o registro sai assinado: a sala pessoal.
    ///
    /// A sala registra sempre, mesmo com IPv4 global, e o encontro dela não é
    /// largado na escada: ela só é achável pelo quarto. É a pergunta que a
    /// escada faz.
    #[must_use]
    pub fn assinada(&self) -> bool {
        self.assinante.is_some()
    }

    /// A convocação de um servidor guardado, com as marcas tiradas da
    /// impressão digital.
    ///
    /// Devolve `None` quando a impressão digital não forma marca: sem marca não
    /// há o que registrar, e o degrau não é tentado.
    #[must_use]
    pub fn para_servidor(
        pelo: PeloServidor,
        impressao_digital: &str,
        escuta: super::Escuta,
        ponto: impl Into<String>,
    ) -> Option<Self> {
        Some(Self {
            pelo,
            marcas: Marcas::do_servidor(impressao_digital)?,
            ponto: ponto.into(),
            escuta,
            assinante: None,
        })
    }

    /// O que o ambiente pediu, e **nada** sem ele.
    ///
    /// Lê `$SEELE_ENCONTRO`: um endereço troca o ponto de encontro, e `nao` ou
    /// vazio desligam o degrau. **Sem a variável, `None`**: é o padrão seguro
    /// (decisão (d) do lote G-servidor da F5), e esquecer o ambiente custa «sem
    /// degrau 4», e não um servidor de teste no quarto de produção.
    #[must_use]
    pub fn do_ambiente(
        pelo: PeloServidor,
        impressao_digital: &str,
        escuta: super::Escuta,
    ) -> Option<Self> {
        Self::do_ambiente_ou(pelo, impressao_digital, escuta, None)
    }

    /// O que o ambiente pediu, e o [`PONTO_PADRAO`] quando a variável não
    /// existe. **Só a produção chama isto**: o botão HOSPEDAR do app.
    #[must_use]
    pub fn com_o_ponto_padrao(
        pelo: PeloServidor,
        impressao_digital: &str,
        escuta: super::Escuta,
    ) -> Option<Self> {
        Self::do_ambiente_ou(pelo, impressao_digital, escuta, Some(PONTO_PADRAO))
    }

    fn do_ambiente_ou(
        pelo: PeloServidor,
        impressao_digital: &str,
        escuta: super::Escuta,
        sem_a_variavel: Option<&str>,
    ) -> Option<Self> {
        let escolhido =
            encontro::ponto_escolhido(std::env::var(VARIAVEL).ok().as_deref(), sem_a_variavel)?;
        let convocacao = Self::para_servidor(pelo, impressao_digital, escuta, escolhido);
        if convocacao.is_none() {
            tracing::warn!(
                "a impressão digital deste servidor não forma marca; o degrau 4 fica de fora"
            );
        }
        convocacao
    }
}

/// Quem assina o registro da sala no ponto (`MORO3`): a chave de identidade
/// da pessoa, a mesma do código e da apresentação da sala.
#[derive(Clone)]
pub struct Assinante {
    chave: ed25519_dalek::SigningKey,
}

/// Sem a chave: ela não aparece em log nenhum.
impl std::fmt::Debug for Assinante {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assinante").finish_non_exhaustive()
    }
}

impl Assinante {
    /// O assinante desta chave.
    #[must_use]
    pub fn novo(chave: ed25519_dalek::SigningKey) -> Self {
        Self { chave }
    }

    /// As marcas da sala desta chave (`Marcas::da_sala`).
    #[must_use]
    pub fn marcas(&self) -> Marcas {
        Marcas::da_sala(&seele_proto::codigo::Codigo::da_chave(
            &self.chave.verifying_key().to_bytes(),
        ))
    }
}

/// Como está o registro assinado da sala no ponto, para a casca dizer
/// («SEU CÓDIGO NÃO ESTÁ ACHÁVEL AGORA», e o motivo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoDoRegistro {
    /// Logo depois da subida, antes da primeira resposta.
    Esperando,
    /// O último `MORO3` foi guardado: quem liga pelo código acha esta máquina.
    Registrado,
    /// O ponto recusou porque os relógios diferem além da tolerância
    /// (`TOLERANCIA_DO_CARIMBO`, 300 s).
    RelogioTorto {
        /// O relógio daqui menos o do ponto, em segundos: positivo, o daqui
        /// está adiantado.
        diferenca_s: i64,
    },
    /// O ponto recusou por outro motivo: cheio, excesso de pedidos desta rede,
    /// ou um registro mais novo desta chave vindo de outro endereço.
    Recusado(encontro::Motivo),
    /// O ponto não aceitou o cookie que ele mesmo acabou de dar: esta máquina
    /// parece sair para a internet por mais de um endereço.
    CookieRecusado,
    /// O ponto não respondeu aos dois últimos registros: ou está fora do ar, ou
    /// é anterior ao ENC/3.
    SemResposta,
}

/// Quantos reavivamentos seguidos sem resposta viram
/// [`EstadoDoRegistro::SemResposta`]. Dois: um pacote perdido não vira
/// «o ponto não respondeu».
const SILENCIOS_ATE_DIZER: u8 = 2;

/// O registro das duas marcas no quarto, de um servidor guardado ou da sala:
/// monta os pedidos a cada envio, guarda o cookie e o endereço público, e
/// transforma a resposta do ponto em estado.
struct Registro {
    marcas: Marcas,
    escuta: super::Escuta,
    assinante: Option<Assinante>,
    /// O estado que a casca lê, só na sala.
    estado: Option<tokio::sync::watch::Sender<EstadoDoRegistro>>,
    /// O cookie que o ponto deu a este IP, pelo `DESAFIO` ou pelo `MOROU`.
    cookie: Option<[u8; encontro::BYTES_DO_COOKIE]>,
    /// O carimbo do último `MORO3`, para dizer de quanto é a diferença dos
    /// relógios quando o ponto recusa.
    ultimo_carimbo: u64,
    /// Um registro saiu e a resposta ainda não veio.
    pendente: bool,
    /// Quantos reavivamentos seguidos ficaram sem resposta.
    silencios: u8,
    /// Se o registro já saiu de novo neste tique, por um `DESAFIO`.
    reenviou: bool,
    /// O endereço público do socket, como o `MOROU` da marca do servidor o
    /// disse. É o que vai no `ATENDE` e no convite.
    publico: Option<SocketAddr>,
    /// O mesmo, pela marca da escuta: o `aviso` do bilhete.
    aviso: Option<SocketAddr>,
    /// A última recusa, para `abrir` dizer qual foi.
    recusa: Option<encontro::Motivo>,
    /// O ponto recusou o cookie que deu.
    cookie_recusado: bool,
}

impl Registro {
    fn novo(convocacao: &Convocacao) -> Self {
        Self {
            marcas: convocacao.marcas.clone(),
            escuta: convocacao.escuta,
            assinante: convocacao.assinante.clone(),
            estado: None,
            cookie: None,
            ultimo_carimbo: 0,
            pendente: false,
            silencios: 0,
            reenviou: false,
            publico: None,
            aviso: None,
            recusa: None,
            cookie_recusado: false,
        }
    }

    /// O começo de um reavivamento: um registro anterior sem resposta conta
    /// como silêncio, e [`SILENCIOS_ATE_DIZER`] seguidos viram
    /// [`EstadoDoRegistro::SemResposta`].
    fn no_tique(&mut self) {
        if self.pendente {
            self.silencios = self.silencios.saturating_add(1);
            if self.silencios >= SILENCIOS_ATE_DIZER {
                self.mudar(EstadoDoRegistro::SemResposta);
            }
        }
        self.pendente = true;
        self.reenviou = false;
    }

    /// As dicas de rede local desta máquina, recalculadas a cada envio (uma
    /// troca de Wi-Fi muda a dica no tique seguinte).
    fn dicas(&self) -> Vec<SocketAddr> {
        dicas_desta_maquina(self.escuta, &super::interfaces::descobrir())
    }

    /// Os dois pedidos deste envio, com o cookie que se tem: o da escuta e o
    /// do servidor. Na sala, assinados agora, com o carimbo de agora, e as
    /// dicas só no do servidor.
    fn pedidos(&mut self) -> [Vec<u8>; 2] {
        let Some(assinante) = self.assinante.clone() else {
            return [
                encontro::mora3(self.marcas.escuta(), self.cookie.as_ref()),
                encontro::mora3(self.marcas.servidor(), self.cookie.as_ref()),
            ];
        };
        let carimbo = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |desde| desde.as_secs());
        self.ultimo_carimbo = carimbo;
        let dicas = self.dicas();
        let da_escuta = self.assinado(&assinante, self.marcas.escuta(), carimbo, Vec::new());
        let do_servidor = self.assinado(&assinante, self.marcas.servidor(), carimbo, dicas);
        [encontro::moro3(&da_escuta), encontro::moro3(&do_servidor)]
    }

    fn assinado(
        &self,
        assinante: &Assinante,
        marca: &Marca,
        carimbo: u64,
        dicas: Vec<SocketAddr>,
    ) -> encontro::Moro2 {
        use ed25519_dalek::Signer as _;
        let chave = assinante.chave.verifying_key().to_bytes();
        let mensagem = encontro::mensagem_do_moro3(marca, &chave, carimbo, &dicas);
        encontro::Moro2 {
            marca: marca.clone(),
            chave,
            carimbo,
            dicas,
            assinatura: assinante.chave.sign(&mensagem).to_bytes(),
            cookie: self.cookie,
        }
    }

    /// Ouve uma resposta do ponto ao registro. Devolve se o registro tem de
    /// sair de novo agora: o primeiro `DESAFIO` do tique, com o cookie novo.
    ///
    /// **Só a marca do servidor decide o estado**: as duas marcas recebem a
    /// mesma resposta (o cookie é por IP), e contar as duas faria o segundo
    /// `DESAFIO` do mesmo envio parecer um cookie recusado. A da escuta só
    /// anota o endereço do aviso.
    fn ouviu(&mut self, resposta: &Resposta3) -> bool {
        let servidor = self.marcas.servidor().clone();
        match resposta {
            Resposta3::Morou {
                marca,
                endereco,
                cookie,
            } => {
                self.cookie = Some(*cookie);
                if *marca == servidor {
                    self.pendente = false;
                    self.silencios = 0;
                    self.publico = Some(*endereco);
                    self.recusa = None;
                    self.mudar(EstadoDoRegistro::Registrado);
                } else if marca == self.marcas.escuta() {
                    self.aviso = Some(*endereco);
                }
                false
            }
            Resposta3::Desafio { marca, cookie } if *marca == servidor => {
                self.pendente = false;
                self.silencios = 0;
                self.cookie = Some(*cookie);
                if self.reenviou {
                    self.cookie_recusado = true;
                    self.mudar(EstadoDoRegistro::CookieRecusado);
                    return false;
                }
                self.reenviou = true;
                true
            }
            Resposta3::Recusa {
                marca,
                motivo,
                agora,
            } if *marca == servidor => {
                self.pendente = false;
                self.silencios = 0;
                self.recusa = Some(*motivo);
                if *motivo == encontro::Motivo::Relogio {
                    let diferenca_s =
                        i64::try_from(i128::from(self.ultimo_carimbo) - i128::from(*agora))
                            .unwrap_or(i64::MAX);
                    self.mudar(EstadoDoRegistro::RelogioTorto { diferenca_s });
                } else {
                    self.mudar(EstadoDoRegistro::Recusado(*motivo));
                }
                false
            }
            _ => false,
        }
    }

    /// Muda o estado, e diz no `seele.log` (em `info`, o nível que ele grava)
    /// só quando muda: o reavivamento sai a cada 15 s, e a mesma linha a cada
    /// 15 s inundaria o log. Num servidor guardado não há estado para a casca,
    /// e só o log fala.
    fn mudar(&self, novo: EstadoDoRegistro) {
        if let Some(estado) = &self.estado {
            if *estado.borrow() == novo {
                return;
            }
        }
        match novo {
            EstadoDoRegistro::Esperando => {}
            EstadoDoRegistro::Registrado => tracing::info!(
                "encontro: este servidor está registrado no ponto (ENC/3); quem tem o link, ou \
                 o código, acha esta máquina"
            ),
            EstadoDoRegistro::RelogioTorto { diferenca_s } => tracing::info!(
                diferenca_s,
                "encontro: o ponto recusou o registro porque o relógio desta máquina e o dele \
                 diferem em {diferenca_s} s (a tolerância é de 300 s): confira se a data e a \
                 hora estão automáticas"
            ),
            EstadoDoRegistro::Recusado(motivo) => tracing::info!(
                motivo = motivo.texto(),
                "encontro: o ponto recusou o registro deste servidor"
            ),
            EstadoDoRegistro::CookieRecusado => tracing::info!(
                "encontro: o ponto não aceita o cookie que acabou de dar a esta máquina: ela \
                 parece sair para a internet por mais de um endereço"
            ),
            EstadoDoRegistro::SemResposta => tracing::info!(
                "encontro: o ponto não respondeu ao registro (ENC/3) nos últimos dois \
                 reavivamentos: ou ele está fora do ar, ou é anterior ao ENC/3"
            ),
        }
        if let Some(estado) = &self.estado {
            estado.send_replace(novo);
        }
    }
}

/// As dicas de rede local: os endereços das placas de rede físicas desta
/// máquina que a escuta serve, na porta dela, só os que
/// `encontro::serve_de_dica` aceita (rede de casa, sem escopo), IPv4 primeiro,
/// no máximo `encontro::LIMITE_DE_DICAS`.
///
/// Só as placas físicas: um túnel ou uma ponte de máquina virtual não é a rede
/// da casa de quem liga.
#[must_use]
pub fn dicas_desta_maquina(
    escuta: super::Escuta,
    achados: &[super::interfaces::Achado],
) -> Vec<SocketAddr> {
    let mut dicas: Vec<SocketAddr> = achados
        .iter()
        .filter(|achado| {
            achado.classe() == super::interfaces::Origem::Fisica && escuta.serve(achado.ip)
        })
        .map(|achado| SocketAddr::new(achado.ip, escuta.porta()))
        .filter(|dica| encontro::serve_de_dica(*dica))
        .collect();
    dicas.sort_by_key(|dica| u8::from(dica.is_ipv6()));
    dicas.dedup();
    dicas.truncate(encontro::LIMITE_DE_DICAS);
    dicas
}

/// Um encontro aberto: o que o convite precisa dizer, e a tarefa que o mantém.
pub struct Encontro {
    ponto: String,
    aviso: SocketAddr,
    publico: SocketAddr,
    tarefa: tokio::task::JoinHandle<()>,
    /// O estado do registro assinado, numa sala; `None` num servidor guardado.
    registro: Option<tokio::sync::watch::Receiver<EstadoDoRegistro>>,
}

impl std::fmt::Debug for Encontro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encontro")
            .field("ponto", &self.ponto)
            .field("aviso", &self.aviso)
            .field("publico", &self.publico)
            .finish_non_exhaustive()
    }
}

impl Encontro {
    /// Como está o registro assinado da sala no ponto, para a casca dizer; muda
    /// a cada resposta do ponto. `None` num servidor guardado.
    #[must_use]
    pub fn registro(&self) -> Option<tokio::sync::watch::Receiver<EstadoDoRegistro>> {
        self.registro.clone()
    }

    /// O endereço público do servidor, para entrar no convite como candidato.
    #[must_use]
    pub fn publico(&self) -> SocketAddr {
        self.publico
    }

    /// O bilhete que vai no `seele://`, **com a porta do ponto escrita**: um
    /// nome sem porta não passa por `lookup_host`.
    #[must_use]
    pub fn bilhete(&self) -> Bilhete {
        Bilhete::novo(&self.ponto, self.aviso.to_string())
            .and_then(|bilhete| bilhete.com_porta_escrita())
            .unwrap_or(Bilhete {
                ponto: self.ponto.clone(),
                aviso: self.aviso.to_string(),
            })
    }

    /// Para de reavivar o registro e de atender chamados.
    ///
    /// Nada precisa ser devolvido a ninguém: ao contrário do degrau 3, aqui não
    /// ficou regra nenhuma num roteador.
    pub fn fechar(self) {
        self.tarefa.abort();
    }
}

impl Drop for Encontro {
    /// Para a tarefa mesmo sem [`Encontro::fechar`]: o `JoinHandle` largado
    /// **não** a para, e ela seguraria uma cópia da ponta (o socket do
    /// servidor, e a porta presa) e a caixa do ENC/3 para sempre.
    fn drop(&mut self) {
        self.tarefa.abort();
    }
}

/// Sobe o degrau 4, ou diz por que não deu.
///
/// Nunca demora mais que [`PRAZO`], e essa é a promessa que importa: com o ponto
/// de encontro fora do ar, tudo o que funcionava continua funcionando um segundo
/// depois.
///
/// # Errors
///
/// [`FalhaNoEncontro`], sempre dizendo qual dos casos foi.
pub async fn abrir(convocacao: &Convocacao) -> Result<Encontro, FalhaNoEncontro> {
    let ate = tokio::time::Instant::now() + PRAZO;
    let candidatos = resolver(&convocacao.ponto, ate).await?;

    // A caixa do ENC/3 é uma só por socket, e o encontro é dono dela enquanto
    // vive: a tarefa a leva. Outro encontro de pé nela é um defeito de quem
    // chamou, e é dito em vez de dividir as respostas entre dois leitores.
    let mut caixa = Arc::clone(&convocacao.pelo.enc)
        .try_lock_owned()
        .map_err(|_| FalhaNoEncontro::SemCaixa)?;
    // O que sobrou de um encontro anterior neste socket não é resposta a este.
    while caixa.try_recv().is_ok() {}

    let mut registro = Registro::novo(convocacao);
    let ponta = &convocacao.pelo.ponta;

    // Um de cada vez, até um responder, e **cada um com a sua fatia do
    // prazo**: um nome com A e AAAA vira dois candidatos, e um endereço que
    // engole em silêncio não pode comer a vez do próximo.
    let quantos = candidatos.len().max(1);
    let mut ponto = None;
    for alvo in candidatos {
        let agora = tokio::time::Instant::now();
        let sobra = ate.saturating_duration_since(agora);
        let fatia = agora + sobra / u32::try_from(quantos).unwrap_or(1);
        registro.reenviou = false;
        match tokio::time::timeout_at(fatia, subir_no(ponta, &mut caixa, alvo, &mut registro)).await
        {
            Ok(Ok(())) => {
                ponto = Some(alvo);
                break;
            }
            Ok(Err(())) if registro.recusa.is_some() || registro.cookie_recusado => break,
            Ok(Err(())) => continue,
            Err(_) if tokio::time::Instant::now() < ate => continue,
            Err(_) => break,
        }
    }
    let (Some(ponto), Some(publico)) = (ponto, registro.publico) else {
        if let Some(motivo) = registro.recusa {
            return Err(FalhaNoEncontro::Recusado(motivo));
        }
        if registro.cookie_recusado {
            return Err(FalhaNoEncontro::CookieRecusado);
        }
        return Err(FalhaNoEncontro::SemResposta);
    };
    let aviso = registro.aviso.unwrap_or(publico);

    tracing::info!(%publico, ponto = %convocacao.ponto, "degrau 4: o ponto de encontro nos viu (ENC/3)");

    let leitor = convocacao.assinante.as_ref().map(|_| {
        let (estado, leitor) = tokio::sync::watch::channel(EstadoDoRegistro::Registrado);
        registro.estado = Some(estado);
        leitor
    });
    let tarefa = tokio::spawn(atender(ponta.clone(), caixa, ponto, registro));

    Ok(Encontro {
        ponto: convocacao.ponto.clone(),
        aviso,
        publico,
        tarefa,
        registro: leitor,
    })
}

/// Registra as duas marcas em `alvo` até o `MOROU` da marca do servidor, ou
/// até o ponto recusar. Repete a cada [`REPETICAO`] (um datagrama perdido não
/// custa o degrau), e na hora com o cookie de um `DESAFIO`.
async fn subir_no(
    ponta: &Ponta,
    caixa: &mut tokio::sync::mpsc::Receiver<(SocketAddr, Vec<u8>)>,
    alvo: SocketAddr,
    registro: &mut Registro,
) -> Result<(), ()> {
    loop {
        for pedido in registro.pedidos() {
            // Um envio que **falha** é este endereço que não serve (outra
            // família, sem rota): volta na hora, e o próximo candidato tem a
            // vez dele, em vez de esperar resposta de um pacote que não saiu.
            if let Err(erro) = ponta.enviar_bruto(alvo, &pedido) {
                tracing::debug!(%erro, %alvo, "o registro não saiu; este endereço não serve");
                return Err(());
            }
        }
        let ate = tokio::time::Instant::now() + REPETICAO;
        loop {
            match tokio::time::timeout_at(ate, caixa.recv()).await {
                Err(_) => break,
                Ok(None) => return Err(()),
                Ok(Some((origem, bytes))) => {
                    if !e_do_ponto(origem, alvo) {
                        continue;
                    }
                    let Some(resposta) = encontro::ler_resposta3(&bytes) else {
                        continue;
                    };
                    let reenviar = registro.ouviu(&resposta);
                    if registro.publico.is_some() {
                        return Ok(());
                    }
                    if registro.recusa.is_some() || registro.cookie_recusado {
                        return Err(());
                    }
                    if reenviar {
                        break;
                    }
                }
            }
        }
    }
}

/// Onde o ponto de encontro atende, resolvendo o nome se for um nome.
async fn resolver(
    texto: &str,
    ate: tokio::time::Instant,
) -> Result<Vec<SocketAddr>, FalhaNoEncontro> {
    // A mesma regra que separa o ponto do link separa o do ambiente
    // (`separar_ponto`): a porta padrão de um ponto de encontro não é a de um
    // servidor, e essa regra mora em um lugar só.
    let alvo = seele_proto::uri::separar_ponto(texto)
        .ok()
        .map(|alvo| (alvo.maquina.to_owned(), alvo.porta));
    let Some((maquina, porta)) = alvo else {
        return Err(FalhaNoEncontro::NaoResolve(texto.to_owned()));
    };

    // Nos testes, antes do DNS: ver [`recusar_a_producao`].
    recusar_a_producao(texto, "alcance::encontro::resolver, antes do DNS");

    let procura = tokio::time::timeout_at(ate, tokio::net::lookup_host((maquina, porta)))
        .await
        .map_err(|_| FalhaNoEncontro::NaoResolve(texto.to_owned()))?;

    // **Todos**, e não o primeiro, e **IPv4 primeiro**: o reflexo do primeiro
    // que responde é o que vai no link de outra pessoa, e um reflexo IPv4
    // alcança quase todo mundo. Os IPv6 viajam no `alt=`.
    let mut achados: Vec<SocketAddr> = procura.map(Iterator::collect).unwrap_or_default();
    achados.sort_by_key(|achado| u8::from(achado.is_ipv6()));

    if !achados.is_empty() {
        return Ok(achados);
    }

    // O nome não resolveu. Se ele é o **nosso**, há uma rede embaixo; se é o de
    // outra pessoa, não há — um recuo não atravessa a escolha de ninguém.
    if texto == PONTO_PADRAO {
        if let Some(alvo) = rede_do_padrao() {
            tracing::info!(%texto, %alvo, "o nome não resolveu; usando o endereço de reserva");
            return Ok(vec![alvo]);
        }
    }
    Err(FalhaNoEncontro::NaoResolve(texto.to_owned()))
}

/// O endereço de reserva que serve **esta** máquina, IPv4 primeiro (ver
/// [`resolver`]).
fn rede_do_padrao() -> Option<SocketAddr> {
    let mut candidatos: Vec<SocketAddr> = REDE_DO_PADRAO
        .iter()
        .filter_map(|texto| texto.parse::<SocketAddr>().ok())
        .collect();
    candidatos.sort_by_key(|alvo| u8::from(alvo.is_ipv6()));
    candidatos.first().copied()
}

// A feature do guarda nunca entra num build otimizado: ver o `Cargo.toml`.
#[cfg(all(feature = "recusar-encontro-de-producao", not(debug_assertions)))]
compile_error!(
    "a feature `recusar-encontro-de-producao` é só de teste e não pode ir num build otimizado: \
     com ela, hospedar com o ponto de encontro padrão entra em pânico. Ela vem da \
     `[dev-dependencies]` do `seele-conformance`; quem a ligou num build de release está \
     empacotando o guarda dos testes junto com o produto"
);

/// O guarda dos testes: nenhum deles fala com o ponto de encontro de produção.
///
/// Sem `$SEELE_ENCONTRO`, um teste que subia hospedagem registrava a marca de
/// um servidor de teste no quarto da VPS de produção. Desde a decisão (d) a
/// primeira parede é o padrão seguro; esta é a segunda, e age em
/// [`resolver`] (antes do DNS) e em `Hospedagem::iniciar` (antes da escada).
/// Ligada nos testes deste crate e com a feature `recusar-encontro-de-producao`.
#[cfg(any(test, feature = "recusar-encontro-de-producao"))]
#[track_caller]
pub(crate) fn recusar_a_producao(ponto: &str, onde: &str) {
    assert!(
        !encontro::aponta_para_a_producao(ponto),
        "um teste ia falar com o ponto de encontro de produção («{ponto}», pego em {onde}): cada \
         rodada registraria um servidor de teste no quarto da VPS de produção e deixaria lá o IP \
         desta máquina, e o teste passaria a depender da rede de fora. Aponte um ponto local, ou \
         desligue o degrau 4 com `{VARIAVEL}=nao`, pelo `AmbienteDoEncontro` do teste"
    );
}

/// Fora dos testes, nada: ver a versão ligada logo acima.
#[cfg(not(any(test, feature = "recusar-encontro-de-producao")))]
pub(crate) fn recusar_a_producao(_ponto: &str, _onde: &str) {}

/// Por que um datagrama do degrau 4 não saiu.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NaoSaiu {
    /// Por que, como o log o escreve.
    motivo: String,
    /// A fila de saída estava cheia (`WouldBlock`): passa sozinha, e o tique
    /// seguinte manda de novo.
    passageira: bool,
}

impl std::fmt::Display for NaoSaiu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.motivo)
    }
}

/// Os dois pedidos de cada tique de [`atender`], pelo socket do servidor.
/// Os dois saem sempre, e a falha que conta é a primeira que não é passageira.
fn registrar(ponta: &Ponta, ponto: SocketAddr, registro: &mut Registro) -> Result<(), NaoSaiu> {
    let mut falha: Option<NaoSaiu> = None;
    for pedido in registro.pedidos() {
        if let Err(erro) = ponta.enviar_bruto(ponto, &pedido) {
            let agora = NaoSaiu {
                motivo: format!(
                    "o registro não saiu pelo socket do servidor rumo a {ponto}: {erro}"
                ),
                passageira: erro.kind() == std::io::ErrorKind::WouldBlock,
            };
            if falha
                .as_ref()
                .is_none_or(|antes| antes.passageira && !agora.passageira)
            {
                falha = Some(agora);
            }
        }
    }
    falha.map_or(Ok(()), Err)
}

/// Se o registro no quarto está saindo, para o log dizer só quando isso muda:
/// uma linha na primeira falha depois de um registro bom, e uma na volta. Uma
/// falha passageira (fila cheia) não muda nada e fica em `debug`.
#[derive(Debug, Default)]
struct RegistroNoQuarto {
    falhando: bool,
}

impl RegistroNoQuarto {
    fn anotar(&mut self, ponto: SocketAddr, saiu: Result<(), NaoSaiu>) {
        match saiu {
            Err(falha) if falha.passageira => {
                tracing::debug!(%ponto, motivo = %falha, "encontro: fila de saída cheia; o tique seguinte manda de novo");
            }
            Err(motivo) if !self.falhando => {
                self.falhando = true;
                tracing::info!(
                    %ponto,
                    %motivo,
                    "encontro: o registro no quarto deixou de sair; quem procura este anfitrião \
                     não o acha até ele voltar"
                );
            }
            Ok(()) if self.falhando => {
                self.falhando = false;
                tracing::info!(%ponto, "encontro: o registro no quarto voltou a sair");
            }
            Ok(()) | Err(_) => {}
        }
    }
}

/// Se este datagrama veio de onde o ponto de encontro atende.
///
/// **Compara o endereço, não a porta**: um ponto atrás de um balanceador
/// responde de porta efêmera, e quem consegue forjar um endereço de origem
/// forja a porta junto. Um `CHAMADO` forjado direto neste socket, de fora do
/// ponto, não fura nada.
fn e_do_ponto(origem: SocketAddr, ponto: SocketAddr) -> bool {
    origem.ip().to_canonical() == ponto.ip().to_canonical()
}

/// Se ainda cabe um chamado na janela de um minuto (R2).
fn cabe_mais_um_chamado(chamados: &mut VecDeque<tokio::time::Instant>) -> bool {
    let agora = tokio::time::Instant::now();
    while chamados
        .front()
        .is_some_and(|quando| agora.duration_since(*quando) >= JANELA_DOS_CHAMADOS)
    {
        chamados.pop_front();
    }
    if chamados.len() >= CHAMADOS_POR_MINUTO {
        return false;
    }
    chamados.push_back(agora);
    true
}

/// O que o anfitrião faz com um `CHAMADO` (o fio do ENC/3, `docs/ponto-de-encontro.md`):
/// confere a marca, aplica o teto, fura os candidatos que servem e responde
/// `ATENDE` ao ponto, pelo mesmo socket. Devolve quantos candidatos furou, ou
/// `None` quando não atendeu.
fn atender_chamado(
    ponta: &Ponta,
    origem: SocketAddr,
    chamado: &encontro::Chamado,
    registro: &Registro,
    chamados: &mut VecDeque<tokio::time::Instant>,
) -> Option<usize> {
    if chamado.escuta != *registro.marcas.escuta() {
        // A marca de outra escuta: um ponto confuso, ou alguém chutando.
        tracing::debug!("encontro: um CHAMADO para outra escuta; ignorado");
        return None;
    }
    let publico = registro.publico?;
    if !cabe_mais_um_chamado(chamados) {
        tracing::warn!(
            teto = CHAMADOS_POR_MINUTO,
            "encontro: chamados demais neste minuto; este não foi atendido (o ponto responde \
             CALADO a quem ligou)"
        );
        return None;
    }
    let validos: Vec<SocketAddr> = chamado
        .candidatos
        .iter()
        .copied()
        .filter(|candidato| encontro::serve_de_candidato(*candidato))
        .collect();
    let descartados = chamado.candidatos.len() - validos.len();
    let furados = if validos.is_empty() {
        0
    } else {
        // O furo e o par do CHAMADO, já com estes candidatos: o ICE completo da
        // ponta checa cada um (o NAT de casa que muda a porta conforme o destino só
        // abre o caminho certo assim).
        ponta.chamado(chamado.credencial.texto(), &validos)
    };
    tracing::info!(
        furados,
        descartados,
        "degrau 4: alguém está ligando por aqui; furando e atendendo"
    );
    let atende = encontro::Atende {
        escuta: chamado.escuta.clone(),
        id: chamado.id,
        endereco: publico,
        dicas: registro.dicas(),
    };
    if let Err(erro) = ponta.enviar_bruto(origem, &encontro::atende(&atende)) {
        tracing::debug!(%erro, "encontro: o ATENDE não saiu");
    }
    Some(furados)
}

/// O laço que mantém o degrau 4 de pé enquanto o servidor estiver no ar:
/// reaviva o registro a cada [`REAVIVAR`] e atende os `CHAMADO`s.
///
/// O primeiro tique sai na hora, de propósito: o `interval` completa o
/// primeiro `tick` imediatamente, e o quarto fica sabendo de novo logo depois
/// da subida.
async fn atender(
    ponta: Ponta,
    mut caixa: tokio::sync::OwnedMutexGuard<tokio::sync::mpsc::Receiver<(SocketAddr, Vec<u8>)>>,
    ponto: SocketAddr,
    mut registro: Registro,
) {
    let mut relogio = tokio::time::interval(REAVIVAR);
    let mut chamados = VecDeque::new();
    let mut no_quarto = RegistroNoQuarto::default();
    loop {
        tokio::select! {
            _ = relogio.tick() => {
                registro.no_tique();
                no_quarto.anotar(ponto, registrar(&ponta, ponto, &mut registro));
            }
            recebido = caixa.recv() => {
                let Some((origem, bytes)) = recebido else { break };
                if !e_do_ponto(origem, ponto) {
                    tracing::debug!(%origem, "ENC/3 de fora do ponto de encontro; ignorado");
                    continue;
                }
                let Some(resposta) = encontro::ler_resposta3(&bytes) else { continue };
                if let Resposta3::Chamado(chamado) = &resposta {
                    let _ = atender_chamado(&ponta, origem, chamado, &registro, &mut chamados);
                    continue;
                }
                if registro.ouviu(&resposta) {
                    no_quarto.anotar(ponto, registrar(&ponta, ponto, &mut registro));
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]
mod testes {
    use super::*;
    use crate::alcance::{Escuta, Pilha};
    use seele_proto::encontro::{Pedido3, PORTA_PADRAO};
    use std::net::Ipv6Addr;

    const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";
    const COOKIE: [u8; encontro::BYTES_DO_COOKIE] = [7; encontro::BYTES_DO_COOKIE];

    /// Uma ponta do enlace num socket real: `[::]` de pilha dupla pelo bind
    /// do `seeled` (`alcance::abrir_escuta`), ou 127.0.0.1 onde não há pilha
    /// dupla.
    async fn pelo() -> PeloServidor {
        let ponta = match crate::alcance::abrir_escuta(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)))
        {
            Ok((socket, _)) => Ponta::de_socket(socket, None).expect("ponta"),
            Err(_) => Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
                .await
                .expect("ponta"),
        };
        let enc = Arc::new(tokio::sync::Mutex::new(
            ponta.mensagens_enc().expect("a caixa do ENC/3"),
        ));
        PeloServidor { ponta, enc }
    }

    fn escuta(pelo: &PeloServidor) -> Escuta {
        Escuta::nova(pelo.ponta.local_addr().port(), Pilha::Dupla)
    }

    type Caderno = Arc<std::sync::Mutex<Vec<(Pedido3, SocketAddr)>>>;

    /// Um ponto de teste do ENC/3: a um registro sem o [`COOKIE`] responde
    /// `DESAFIO`; com ele, `MOROU` com o endereço visto, ou o que `recusar`
    /// disser. Anota tudo o que chega, com quem mandou.
    async fn ponto_de_teste(
        recusar: Option<encontro::Motivo>,
    ) -> (Arc<tokio::net::UdpSocket>, SocketAddr, Caderno) {
        let socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let onde = socket.local_addr().unwrap();
        let caderno: Caderno = Arc::new(std::sync::Mutex::new(Vec::new()));
        let anotador = Arc::clone(&caderno);
        let servico = Arc::clone(&socket);
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO3 + 1];
            while let Ok((lidos, de)) = servico.recv_from(&mut balde).await {
                let Some(pedido) = balde.get(..lidos).and_then(encontro::analisar3) else {
                    continue;
                };
                anotador.lock().unwrap().push((pedido.clone(), de));
                let (marca, cookie) = match &pedido {
                    Pedido3::Mora3 { marca, cookie } => (marca.clone(), *cookie),
                    Pedido3::Moro3(moro) => (moro.marca.clone(), moro.cookie),
                    _ => continue,
                };
                let resposta = match (cookie, recusar) {
                    (Some(c), None) if c == COOKIE => encontro::Resposta3::Morou {
                        marca,
                        endereco: de,
                        cookie: COOKIE,
                    },
                    (Some(c), Some(motivo)) if c == COOKIE => encontro::Resposta3::Recusa {
                        marca,
                        motivo,
                        agora: 0,
                    },
                    _ => encontro::Resposta3::Desafio {
                        marca,
                        cookie: COOKIE,
                    },
                };
                let _ = servico.send_to(&encontro::resposta3(&resposta), de).await;
            }
        });
        (socket, onde, caderno)
    }

    /// O endereço como o socket do ponto o vê, em forma canônica.
    fn canonico(endereco: SocketAddr) -> SocketAddr {
        SocketAddr::new(endereco.ip().to_canonical(), endereco.port())
    }

    #[tokio::test]
    async fn o_registro_sai_nas_duas_marcas_e_repete_com_o_cookie_do_desafio() {
        let pelo = pelo().await;
        let local = pelo.ponta.local_addr().port();
        let (_ponto, onde, caderno) = ponto_de_teste(None).await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), onde.to_string())
                .unwrap();
        let aberto = abrir(&convocacao).await.expect("o degrau 4 abriu");
        assert_eq!(
            aberto.publico().port(),
            local,
            "o público não é o do socket do servidor"
        );
        assert!(aberto.publico().ip().to_canonical().is_loopback());

        let anotados = caderno.lock().unwrap().clone();
        let marcas = Marcas::do_servidor(IMPRESSAO).unwrap();
        for marca in [marcas.escuta(), marcas.servidor()] {
            assert!(
                anotados.iter().any(|(p, _)| *p
                    == Pedido3::Mora3 {
                        marca: marca.clone(),
                        cookie: None
                    }),
                "o primeiro MORA3 de {marca} não saiu: {anotados:?}"
            );
            assert!(
                anotados.iter().any(|(p, _)| *p
                    == Pedido3::Mora3 {
                        marca: marca.clone(),
                        cookie: Some(COOKIE)
                    }),
                "o MORA3 de {marca} não repetiu com o cookie do DESAFIO: {anotados:?}"
            );
        }
        // **Só o ENC/3** sai daqui: nenhum ENC/1 nem ENC/2 (D6).
        assert!(anotados
            .iter()
            .all(|(p, _)| matches!(p, Pedido3::Mora3 { .. })));
        // E tudo saiu do socket do servidor, e não de uma escuta própria.
        assert!(anotados.iter().all(|(_, de)| de.port() == local));
    }

    /// Manda um `CHAMADO` do ponto de teste para o anfitrião.
    async fn chamar(ponto: &tokio::net::UdpSocket, para: SocketAddr, chamado: &encontro::Chamado) {
        let destino = canonico(para);
        let resposta = encontro::Resposta3::Chamado(chamado.clone());
        ponto
            .send_to(&encontro::resposta3(&resposta), destino)
            .await
            .unwrap();
    }

    fn chamado(escuta: &Marca, id: u8, candidatos: Vec<SocketAddr>) -> encontro::Chamado {
        encontro::Chamado {
            escuta: escuta.clone(),
            id: [id; encontro::BYTES_DO_ID],
            credencial: encontro::Credencial::nova(&"U".repeat(32)).unwrap(),
            candidatos,
        }
    }

    /// Os `ATENDE` que o ponto de teste anotou.
    fn atendes(caderno: &Caderno) -> Vec<encontro::Atende> {
        caderno
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(p, _)| match p {
                Pedido3::Atende(a) => Some(a.clone()),
                _ => None,
            })
            .collect()
    }

    /// Conta as Binding Indications (20 B, tipo 0x0011) que chegam a `alvo`.
    async fn contar_indicacoes(alvo: &tokio::net::UdpSocket, por: Duration) -> usize {
        let mut contadas = 0;
        let mut balde = [0_u8; 64];
        let ate = tokio::time::Instant::now() + por;
        while let Ok(Ok((lidos, _))) =
            tokio::time::timeout_at(ate, alvo.recv_from(&mut balde)).await
        {
            if lidos == 20 && balde[0] == 0x00 && balde[1] == 0x11 {
                contadas += 1;
            }
        }
        contadas
    }

    #[tokio::test]
    async fn um_chamado_fura_os_candidatos_que_servem_e_atende_com_o_publico() {
        let pelo = pelo().await;
        let (ponto, onde, caderno) = ponto_de_teste(None).await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), onde.to_string())
                .unwrap();
        let aberto = abrir(&convocacao).await.unwrap();
        let alvo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let candidato = alvo.local_addr().unwrap();
        let marcas = Marcas::do_servidor(IMPRESSAO).unwrap();
        chamar(
            &ponto,
            aberto.publico(),
            &chamado(marcas.escuta(), 1, vec![candidato]),
        )
        .await;

        let indicacoes = contar_indicacoes(&alvo, Duration::from_millis(1500)).await;
        assert!(
            (1..=seele_enlace::INDICACOES_POR_CANDIDATO).contains(&indicacoes),
            "o furo mandou {indicacoes} Indications ao candidato (teto de {} por CHAMADO); \
             atendes {:?}; contadores {:?}",
            seele_enlace::INDICACOES_POR_CANDIDATO,
            atendes(&caderno),
            pelo.ponta.contadores()
        );
        let atendidos = atendes(&caderno);
        assert_eq!(atendidos.len(), 1, "{atendidos:?}");
        assert_eq!(atendidos[0].id, [1; encontro::BYTES_DO_ID]);
        assert_eq!(atendidos[0].endereco, aberto.publico());
        assert_eq!(&atendidos[0].escuta, marcas.escuta());
    }

    /// **Um candidato que não serve é descartado aqui, e não só no ponto**
    /// (R2). Pelo fio ele nem chega: o `CHAMADO` com um candidato fora da forma
    /// canônica cala no `ler_resposta3`. Este teste entrega o chamado direto,
    /// como um ponto que deixasse passar o faria.
    #[tokio::test]
    async fn um_candidato_que_nao_serve_e_descartado_pelo_anfitriao() {
        let pelo = pelo().await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), "127.0.0.1:9")
                .unwrap();
        let mut registro = Registro::novo(&convocacao);
        registro.publico = Some(SocketAddr::from(([198, 51, 100, 7], 8383)));
        let alvo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let bom = alvo.local_addr().unwrap();
        let ruins = [
            SocketAddr::from(([127, 0, 0, 1], 80)),
            SocketAddr::from(([0, 0, 0, 0], 4000)),
            SocketAddr::from(([224, 0, 0, 1], 4000)),
        ];
        let mut candidatos = ruins.to_vec();
        candidatos.push(bom);
        let mut chamados = VecDeque::new();
        let origem = SocketAddr::from(([127, 0, 0, 1], 9));
        let furados = atender_chamado(
            &pelo.ponta,
            origem,
            &chamado(convocacao.marcas.escuta(), 9, candidatos),
            &registro,
            &mut chamados,
        );
        assert_eq!(
            furados,
            Some(1),
            "o anfitrião furou candidatos que não servem"
        );
        assert!(contar_indicacoes(&alvo, Duration::from_millis(1200)).await >= 1);
        // Só candidatos ruins: nenhum furo, e o ATENDE ainda sai (ele vai ao
        // ponto, e não a um terceiro).
        let so_ruins = atender_chamado(
            &pelo.ponta,
            origem,
            &chamado(convocacao.marcas.escuta(), 10, ruins.to_vec()),
            &registro,
            &mut chamados,
        );
        assert_eq!(so_ruins, Some(0));
    }

    #[tokio::test]
    async fn um_chamado_de_outra_escuta_ou_de_fora_do_ponto_nao_fura_nem_atende() {
        let pelo = pelo().await;
        let (ponto, onde, caderno) = ponto_de_teste(None).await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), onde.to_string())
                .unwrap();
        let aberto = abrir(&convocacao).await.unwrap();
        let alvo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let candidato = alvo.local_addr().unwrap();

        // Outra escuta, vindo do ponto.
        let outra = Marca::nova("outraescuta").unwrap();
        chamar(
            &ponto,
            aberto.publico(),
            &chamado(&outra, 2, vec![candidato]),
        )
        .await;
        // A marca certa, vinda de fora do ponto (::1, outro endereço).
        let marcas = Marcas::do_servidor(IMPRESSAO).unwrap();
        if let Ok(estranho) = tokio::net::UdpSocket::bind("[::1]:0").await {
            let destino = SocketAddr::from((Ipv6Addr::LOCALHOST, aberto.publico().port()));
            let resposta =
                encontro::Resposta3::Chamado(chamado(marcas.escuta(), 3, vec![candidato]));
            let _ = estranho
                .send_to(&encontro::resposta3(&resposta), destino)
                .await;
        }
        assert_eq!(
            contar_indicacoes(&alvo, Duration::from_millis(800)).await,
            0
        );
        assert!(atendes(&caderno).is_empty(), "{:?}", atendes(&caderno));
    }

    #[tokio::test]
    async fn o_teto_de_chamados_por_minuto_cala_o_excedente() {
        let pelo = pelo().await;
        let (ponto, onde, caderno) = ponto_de_teste(None).await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), onde.to_string())
                .unwrap();
        let aberto = abrir(&convocacao).await.unwrap();
        let marcas = Marcas::do_servidor(IMPRESSAO).unwrap();
        for id in 0..u8::try_from(CHAMADOS_POR_MINUTO + 5).unwrap() {
            chamar(
                &ponto,
                aberto.publico(),
                &chamado(marcas.escuta(), id, Vec::new()),
            )
            .await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            atendes(&caderno).len(),
            CHAMADOS_POR_MINUTO,
            "o anfitrião atendeu além do teto de chamados por minuto (R2)"
        );
    }

    #[tokio::test]
    async fn um_ponto_de_encontro_que_nao_existe_nao_segura_ninguem() {
        // Um endereço de documentação (RFC 5737): aceita o envio e nunca
        // responde.
        let pelo = pelo().await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), "192.0.2.1:8384")
                .unwrap();
        let comeco = std::time::Instant::now();
        let falha = abrir(&convocacao)
            .await
            .expect_err("um buraco negro respondeu");
        assert_eq!(falha, FalhaNoEncontro::SemResposta);
        assert!(comeco.elapsed() < PRAZO + Duration::from_millis(500));
        // E a caixa volta: um encontro novo no mesmo socket abre.
        assert!(pelo.enc.try_lock().is_ok(), "a caixa do ENC/3 ficou presa");
    }

    #[tokio::test]
    async fn a_recusa_do_ponto_e_dita_com_o_motivo() {
        let pelo = pelo().await;
        let (_ponto, onde, _caderno) = ponto_de_teste(Some(encontro::Motivo::Cheio)).await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), onde.to_string())
                .unwrap();
        assert_eq!(
            abrir(&convocacao).await.unwrap_err(),
            FalhaNoEncontro::Recusado(encontro::Motivo::Cheio)
        );
    }

    #[tokio::test]
    async fn a_sala_registra_moro3_assinado_e_o_estado_diz_registrado() {
        let pelo = pelo().await;
        let (_ponto, onde, caderno) = ponto_de_teste(None).await;
        let chave = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
        let publica = chave.verifying_key();
        let convocacao =
            Convocacao::para_sala(pelo.clone(), escuta(&pelo), chave, onde.to_string());
        assert!(convocacao.assinada());
        let aberto = abrir(&convocacao).await.unwrap();
        assert_eq!(
            *aberto.registro().unwrap().borrow(),
            EstadoDoRegistro::Registrado
        );
        let anotados = caderno.lock().unwrap().clone();
        let moros: Vec<_> = anotados
            .iter()
            .filter_map(|(p, _)| match p {
                Pedido3::Moro3(m) => Some(m.clone()),
                _ => None,
            })
            .collect();
        assert!(!moros.is_empty());
        for moro in moros {
            assert_eq!(moro.chave, publica.to_bytes());
            let mensagem =
                encontro::mensagem_do_moro3(&moro.marca, &moro.chave, moro.carimbo, &moro.dicas);
            publica
                .verify_strict(
                    &mensagem,
                    &ed25519_dalek::Signature::from_bytes(&moro.assinatura),
                )
                .expect("o MORO3 não confere com o rótulo do ENC/3");
        }
        assert!(
            anotados
                .iter()
                .all(|(p, _)| !matches!(p, Pedido3::Mora3 { .. })),
            "a sala registrou sem assinatura"
        );
    }

    #[tokio::test]
    async fn o_ponto_do_ambiente_segue_a_regra_do_ponto_do_link() {
        let ate = tokio::time::Instant::now() + PRAZO;
        assert_eq!(
            super::resolver("127.0.0.1", ate).await,
            Ok(vec![SocketAddr::from(([127, 0, 0, 1], PORTA_PADRAO))]),
        );
        for torto in ["tem espaço", ""] {
            assert_eq!(
                super::resolver(torto, ate).await,
                Err(FalhaNoEncontro::NaoResolve(torto.to_owned())),
            );
        }
    }

    #[test]
    fn a_rede_do_padrao_existe_e_e_ipv4() {
        let escolhido = super::rede_do_padrao();
        assert!(
            escolhido.is_some_and(|alvo| alvo.is_ipv4()),
            "{escolhido:?}"
        );
    }

    #[tokio::test]
    async fn o_recuo_nunca_atravessa_o_ponto_que_outra_pessoa_escolheu() {
        let ate = tokio::time::Instant::now() + super::PRAZO;
        assert!(super::resolver("encontro.invalido.invalid:8384", ate)
            .await
            .is_err());
    }

    #[test]
    fn o_ponto_padrao_que_nao_resolve_nao_culpa_a_maquina_de_quem_hospeda() {
        let nosso = FalhaNoEncontro::NaoResolve(PONTO_PADRAO.to_owned()).to_string();
        assert!(!nosso.contains("está sem DNS"), "{nosso}");
        assert!(nosso.contains("não está no ar"), "{nosso}");
        assert!(!nosso.contains(VARIAVEL), "{nosso}");
        let dela = FalhaNoEncontro::NaoResolve("encontro.davi.exemplo".to_owned()).to_string();
        assert!(dela.contains("esta máquina"), "{dela}");
    }

    #[tokio::test]
    async fn as_marcas_saem_da_impressao_digital_e_nada_mais() {
        let pelo = pelo().await;
        let convocacao =
            Convocacao::para_servidor(pelo.clone(), IMPRESSAO, escuta(&pelo), PONTO_PADRAO)
                .unwrap();
        assert_eq!(convocacao.marcas, Marcas::do_servidor(IMPRESSAO).unwrap());
        assert!(!convocacao.assinada());
        assert!(
            Convocacao::para_servidor(pelo.clone(), "curto", escuta(&pelo), PONTO_PADRAO).is_none()
        );
    }

    #[tokio::test]
    async fn desligar_o_degrau_4_e_uma_variavel_de_ambiente() {
        let pelo = pelo().await;
        let e = escuta(&pelo);
        let ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::guardar();
        ambiente.pedir("nao");
        assert!(Convocacao::do_ambiente(pelo.clone(), IMPRESSAO, e).is_none());
        ambiente.pedir("");
        assert!(Convocacao::do_ambiente(pelo.clone(), IMPRESSAO, e).is_none());
        ambiente.pedir("meu.ponto:9000");
        assert_eq!(
            Convocacao::do_ambiente(pelo.clone(), IMPRESSAO, e).map(|c| c.ponto),
            Some("meu.ponto:9000".to_owned())
        );
        ambiente.pedir("nao");
        assert!(Convocacao::com_o_ponto_padrao(pelo.clone(), IMPRESSAO, e).is_none());
        ambiente.tirar();
        assert_eq!(
            Convocacao::com_o_ponto_padrao(pelo.clone(), IMPRESSAO, e).map(|c| c.ponto),
            Some(PONTO_PADRAO.to_owned())
        );
        assert!(
            Convocacao::do_ambiente(pelo, IMPRESSAO, e).is_none(),
            "sem a variável, `do_ambiente` convocou um ponto"
        );
    }

    #[tokio::test]
    async fn o_bilhete_do_link_escreve_a_porta_do_ponto() {
        let aviso = SocketAddr::from(([198, 51, 100, 7], 41234));
        let com = |ponto: &str| Encontro {
            ponto: ponto.to_owned(),
            aviso,
            publico: SocketAddr::from(([198, 51, 100, 7], 8383)),
            tarefa: tokio::spawn(async {}),
            registro: None,
        };
        assert_eq!(com(PONTO_PADRAO).bilhete().ponto, "rendzvous.seele.sh:8384");
        assert_eq!(com("[2001:db8::1]").bilhete().ponto, "[2001:db8::1]:8384");
        assert_eq!(com("meu.ponto:9000").bilhete().ponto, "meu.ponto:9000");
        assert_eq!(com(PONTO_PADRAO).bilhete().aviso, aviso.to_string());
    }

    #[tokio::test]
    #[should_panic(expected = "pego em alcance::encontro::resolver, antes do DNS")]
    async fn resolver_o_ip_da_vps_de_producao_reprova_antes_do_primeiro_pacote() {
        let ate = tokio::time::Instant::now() + PRAZO;
        let _ = super::resolver("216.128.168.216:8384", ate).await;
    }

    #[test]
    fn as_dicas_sao_da_placa_de_rede_local_ipv4_primeiro_e_no_maximo_duas() {
        use crate::alcance::interfaces::{Achado, Origem};
        let achado = |ip: &str, origem| Achado {
            ip: ip.parse().unwrap(),
            mascara: None,
            origem,
        };
        let achados = [
            achado("fd12::5", Origem::Fisica),
            achado("8.8.8.8", Origem::Fisica),
            achado("10.8.0.2", Origem::Tunel),
            achado("192.168.0.10", Origem::Fisica),
            achado("172.17.0.1", Origem::Virtual),
            achado("192.168.1.10", Origem::Fisica),
            achado("192.168.1.11", Origem::Fisica),
        ];
        assert_eq!(
            dicas_desta_maquina(Escuta::nova(8385, Pilha::Dupla), &achados),
            vec![
                SocketAddr::from(([192, 168, 0, 10], 8385)),
                SocketAddr::from(([192, 168, 1, 10], 8385)),
            ],
        );
        assert_eq!(
            dicas_desta_maquina(Escuta::nova(8385, Pilha::SoIpv6), &achados),
            vec![SocketAddr::from((
                "fd12::5".parse::<Ipv6Addr>().unwrap(),
                8385
            ))],
        );
    }

    #[test]
    fn a_janela_de_chamados_fecha_no_teto() {
        let mut chamados = VecDeque::new();
        for _ in 0..CHAMADOS_POR_MINUTO {
            assert!(cabe_mais_um_chamado(&mut chamados));
        }
        assert!(!cabe_mais_um_chamado(&mut chamados));
    }
}
