//! O enlace com um servidor, incluindo o que fazer quando ele cai.
//!
//! [`Client`] é uma conexão: enquanto ela existe, funciona; quando cai, acaba.
//! Isto é a **sessão**, que é outra coisa — ela atravessa quedas. É aqui que
//! mora a bateria interna de `specs/07-estetica.md`:
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
//! Restaura a sala de voz, a Linha, o mudo e o isolamento: é o que a pessoa
//! escolheu, e voltar sem isso seria voltar para outro lugar. **Não** restaura
//! a voz sozinha — a conexão é nova, e com ela o canal de mídia. A casca recebe
//! [`Aviso::Reconectado`] com o canal novo e reabre o áudio. É honesto: o
//! caminho de áudio realmente recomeça.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use seele_proto::control::{
    ClientMessage, ConsentimentoDePar, DisconnectReason, MotivoDeFalhaDePar, ServerMessage,
};
use seele_proto::ids::{
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, ScreenId, VoiceRoomId,
};
use seele_proto::signal::{Recepcao, SignalBand};
use seele_proto::tunel::{ProtocoloDeTunel, RecusaDeTunel};
use tokio::sync::mpsc;

use crate::battery::{Action, Battery, Link};
use crate::client::{Client, ConnectError, MediaChannel, PeloChama, SessionInfo};
// **A regra das despedidas que acabam a sessão mora no `seele-sessao`**, e o
// `Room` chama a mesma: eram duas respostas, e o `Room` tratava toda
// `Disconnecting` como fim enquanto este motor reconectava (pendência 61). A
// explicação de cada razão está lá, ao lado da lista.
use crate::despedida::a_sessao_acabou_aqui;
use crate::par;
use crate::tofu::PinDecision;
use crate::tofu::PinStore;
use crate::tofu::{verdict, Verdict};
use crate::video::{LimitesDeTela, PedidoDeTela};

/// O que quem disca espera de quem atende, conferido **dentro do TLS**, antes
/// do `Hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Esperada {
    /// A impressão do certificado de um servidor: a do link colado nesta
    /// sessão, ou a que a lista de servidores guardou para a entrada.
    Certificado(String),
    /// O código de uma pessoa: quem atende tem de ser a sala dela (ADR 0060),
    /// conferida pelo [`crate::confere_codigo::ConfereCodigo`].
    Codigo(seele_proto::codigo::Codigo),
}

/// Onde ficar batendo, e com que credencial.
///
/// O `Debug` é escrito à mão, logo abaixo, para não imprimir o segredo.
#[derive(Clone)]
pub struct Destino {
    /// Endereço do servidor.
    pub servidor: SocketAddr,
    /// O nome que o TLS recebe. Ver [`Client::connect`].
    pub nome_tls: String,
    /// Sob que chave o pin é arquivado. Ver [`Client::connect`].
    pub chave_do_pin: String,
    /// Como aparecer no roster.
    pub apelido: String,
    /// Convite de uso único ou senha do servidor.
    pub segredo: Option<String>,
    /// O que se espera de quem atende: a impressão digital do link colado
    /// nesta sessão ou da que a lista de servidores guardou para a entrada
    /// (quem escolhe entre as duas é a casca; `seele_ffi::impressao_a_conferir`),
    /// ou o código de uma pessoa. Ver [`Esperada`] e [`Destino::impressao_esperada`].
    ///
    /// `None` quando não há nenhuma das duas — quem digitou à mão um endereço
    /// que a lista não conhece: aí não há o que conferir, e o primeiro contato
    /// segue sendo cego, como sempre foi.
    ///
    /// **Conferida dentro do TLS**, antes do `Hello`: um primeiro contato que
    /// não confere falha o aperto de mão e não leva convite, senha nem apelido
    /// a quem atendeu (`crate::tofu::TofuVerifier::decide`). Onde o pino
    /// prova o servidor, um servidor já fixado passa e é avisado
    /// (`Verdict::InviteDisagrees`), que é a decisão do ADR 0003 escrita no
    /// verificador; onde não prova, é recusado como o primeiro contato (ver
    /// [`Destino::o_pino_prova_o_servidor`]). Vale na entrada e na volta da
    /// bateria interna.
    pub esperada: Option<Esperada>,
    /// Se o pino deste destino prova o servidor.
    ///
    /// O pin é por endereço de candidato, e prova só *este endereço*: este
    /// `IP:porta` já apresentou esta chave. Ele prova **o servidor**, a
    /// continuidade que o ADR 0003 protege, num lugar só: no **alvo** (o
    /// endereço que a pessoa escolheu: o do link, o digitado, ou o da entrada
    /// da lista) cujo endereço resolvido é de escopo público. Ali um pin que
    /// confere passa mesmo com a impressão esperada discordando, e o veredito
    /// avisa (`Verdict::InviteDisagrees`).
    ///
    /// `false` nos outros dois casos, e neles a impressão esperada vale mais
    /// que o pin: a que não confere recusa dentro do TLS, antes do `Hello`, e
    /// nada é fixado nem desfeito.
    ///
    /// - **Um candidato que ninguém escolheu**: um alternativo do convite ou da
    ///   lista, ou o endereço que o quarto devolveu. O pin pode ser de outro
    ///   servidor que esta máquina já fixou ali: o quarto apontou para ele, ou
    ///   um alternativo de LAN é o mesmo de uma casa para outra.
    /// - **Um alvo de escopo local**: privado, link-local, CGNAT ou ULA
    ///   ([`e_privado`]). É o caso comum, e não a borda: o primeiro endereço de
    ///   um link é o da rede de casa do anfitrião, quando ele tem uma
    ///   (`seele_proto::uri`), e é a chave da entrada que o link deixa na
    ///   lista. Para quem visita pela internet, esse endereço se repete de uma
    ///   casa para outra, e na rede em que a pessoa está outro servidor pode
    ///   atender nele, já fixado. Ali, um pin que confere com a esperada
    ///   discordando é, no caso comum, colisão: um servidor que trocou de
    ///   chave dá `Changed`, e não `Matches`. Os outros casos (uma entrada que
    ///   a 0.15.0 gravou com a impressão de outro servidor, um link de antes de
    ///   o servidor trocar de chave, quando esta máquina já fixou a nova) são
    ///   recusados também, e esse é o preço da regra.
    ///
    /// O loopback conta como público: é esta máquina em qualquer rede. Um nome
    /// decide pelo endereço a que resolveu, e não pelo texto: `casa.local` que
    /// resolve para a rede de casa é de escopo local.
    ///
    /// **Quem calcula é a FFI (`seele_ffi`, em `build_destino`), e só ela**: o
    /// alvo e o endereço resolvido estão ali, e a regra fica num ponto só. Chega
    /// ao verificador na entrada e na volta da bateria interna, como a
    /// impressão esperada. É o adendo de 2026-09-29 ao ADR 0003.
    pub o_pino_prova_o_servidor: bool,
    /// A identidade do conjunto de MODs que esta máquina já aceitou para este
    /// servidor. ADR 0045.
    ///
    /// Mora no destino, e não num argumento de `conectar`, porque a bateria
    /// interna reconecta sozinha por cinco minutos: um aceite que só existisse
    /// na primeira chamada faria toda reconexão ser recusada por não ter o que
    /// responder ao anúncio.
    ///
    /// `None` em quem nunca aceitou nada daquele servidor — que é o estado de
    /// toda primeira entrada, e de todo servidor sem MOD.
    pub aceito: Option<String>,
}

/// **O segredo não sai pelo `Debug`** (lote R da F5, rodada 1).
///
/// `segredo` é o convite de uso único ou a senha do servidor, e um
/// `tracing::debug!(?destino)` o levaria ao `seele.log` de quem entra. Aqui ele
/// sai como `Some(<oculto>)` ou `None`: quem investiga uma recusa sabe se a
/// entrada levou um, e não qual. O resto sai como o `derive` o escrevia.
impl std::fmt::Debug for Destino {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// O lugar do segredo, sem ele.
        struct Oculto;
        impl std::fmt::Debug for Oculto {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("<oculto>")
            }
        }
        f.debug_struct("Destino")
            .field("servidor", &self.servidor)
            .field("nome_tls", &self.nome_tls)
            .field("chave_do_pin", &self.chave_do_pin)
            .field("apelido", &self.apelido)
            .field("segredo", &self.segredo.as_ref().map(|_| Oculto))
            .field("esperada", &self.esperada)
            .field("o_pino_prova_o_servidor", &self.o_pino_prova_o_servidor)
            .field("aceito", &self.aceito)
            .finish()
    }
}

impl Destino {
    /// A impressão esperada, quando o que se espera é um certificado.
    ///
    /// `None` também num destino de código: quem liga para uma pessoa espera
    /// uma chave, e não uma impressão. O veredito lê por aqui; a batida no
    /// ponto e a chegada leem [`Destino::marca_do_aviso`], que serve aos dois.
    #[must_use]
    pub fn impressao_esperada(&self) -> Option<&str> {
        match &self.esperada {
            Some(Esperada::Certificado(impressao)) => Some(impressao),
            Some(Esperada::Codigo(_)) | None => None,
        }
    }

    /// As marcas com que o `CHAMA` deste destino procura quem atende (D7).
    ///
    /// As de um servidor saem da impressão esperada, pela mesma função com que
    /// o anfitrião as registra (`Marcas::do_servidor`); as de uma sala saem do
    /// código (`Marcas::da_sala`, ADR 0062). Duas derivações da mesma regra
    /// seriam duas regras para discordarem no dia em que uma mudar.
    ///
    /// `None` sem nada esperado, e com uma impressão que não dá marca (curta, ou
    /// com um símbolo fora do alfabeto): aí não se chama ponto nenhum.
    #[must_use]
    pub(crate) fn marcas(&self) -> Option<crate::encontro::Marcas> {
        match &self.esperada {
            Some(Esperada::Certificado(impressao)) => {
                crate::encontro::Marcas::do_servidor(impressao)
            }
            Some(Esperada::Codigo(codigo)) => Some(crate::encontro::Marcas::da_sala(codigo)),
            None => None,
        }
    }

    /// A marca do aviso de [`Destino::marcas`], para quem só quer saber se
    /// há o que chamar.
    #[must_use]
    pub(crate) fn marca_do_aviso(&self) -> Option<crate::encontro::Marca> {
        self.marcas().map(|marcas| marcas.aviso().clone())
    }
}

/// O que a casca precisa saber.
pub enum Aviso {
    /// O servidor disse algo.
    Mensagem(Box<ServerMessage>),
    /// Onde o enlace está, e quanto resta da bateria.
    ///
    /// Repetido a cada tica enquanto a bateria corre, porque a contagem
    /// regressiva **é** a informação: `specs/07-estetica.md` pede 04:59
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
        /// A sala de voz em que este motor sentou a pessoa de novo, nesta
        /// conexão, antes de avisar. `None` para quem estava fora de sala.
        ///
        /// **A casca não tem outro jeito de saber.** A sessão nova chega sem
        /// assento nenhum, o servidor confirma a entrada por silêncio, e a
        /// fotografia de ocupação que ele manda pula a própria pessoa. Quem lia
        /// só a sessão nova desenhava FORA DE SALA alguém que estava na sala,
        /// falando e ouvindo — e a pendência 52 abre e fecha o áudio por esse
        /// assento.
        voice_room: Option<VoiceRoomId>,
    },
    /// Uma transferência andou. ADR 0027.
    Transferencia(Transferencia),
    /// Uma transmissão de tela alheia começou a chegar.
    ///
    /// Vem antes de qualquer quadro e carrega o que a casca precisa para armar
    /// o decodificador: tamanho e codec. Sem ela, o primeiro quadro chegaria a
    /// uma tela que ainda não sabe de que tamanho é a imagem.
    TelaAbriu {
        /// Qual transmissão, para casar com o que o `Snapshot` já diz.
        tela: ScreenId,
        /// Largura em pixels, como o cabeçalho a declarou.
        largura: u16,
        /// Altura em pixels.
        altura: u16,
        /// **O que é**: monitor, janela ou câmera, como o cabeçalho declarou.
        ///
        /// É por aqui que quem assiste fica sabendo que a imagem é uma câmera
        /// (protocolo 13): o anúncio da transmissão no controle não diz, e a
        /// casca escreve «câmera» ou «tela» pelo que chega aqui.
        origem: seele_proto::screen::ScreenSource,
        /// **Qual leitura abriu**: um número por leitura, crescente no
        /// processo ([`nova_leitura_de_tela`]). O [`Self::TelaFechou`] dela leva
        /// o mesmo número. Sem ele, o fim de uma leitura substituída (o par
        /// renomeado aborta a anterior da mesma tela) não se distinguia do fim
        /// da que está no ar.
        leitura: u64,
    },
    /// Um quadro comprimido de uma tela alheia.
    ///
    /// Os bytes vão crus, como saíram do codificador do outro lado: quem
    /// decodifica é a casca. Esta camada não decodifica de propósito — o
    /// decodificador do sistema, que a janela alcança, é acelerado por hardware
    /// e não exige o módulo do Cisco em quem só assiste. Só quem transmite
    /// precisa dele.
    /// Um pacote de som da tela que se está assistindo.
    ///
    /// Separado do [`Self::TelaQuadro`] porque o destino é outro: a imagem vai
    /// para a casca desenhar, o som vai para a mistura de saída — e é lá que o
    /// isolamento total decide se ele toca.
    TelaSom {
        /// Qual transmissão.
        tela: ScreenId,
        /// Um pacote Opus, como o outro lado o produziu.
        bytes: Vec<u8>,
        /// De qual leitura, como no [`Self::TelaQuadro`]: o som de uma leitura
        /// substituída não pode cair na fila da nova.
        leitura: u64,
    },
    /// Um quadro comprimido de uma tela alheia.
    ///
    /// Os bytes vão crus, como saíram do codificador do outro lado: quem
    /// decodifica é a casca. Esta camada não decodifica de propósito — o
    /// decodificador do sistema, que a janela alcança, é acelerado por hardware
    /// e não exige o módulo do Cisco em quem só assiste. Só quem transmite
    /// precisa dele.
    TelaQuadro {
        /// Qual transmissão.
        tela: ScreenId,
        /// Se dá para começar a decodificar por este.
        chave: bool,
        /// O quadro, em Annex-B.
        bytes: Vec<u8>,
        /// De qual leitura: o número do [`Self::TelaAbriu`] dela. Uma leitura
        /// substituída pode ainda soltar um quadro depois da abertura nova da
        /// mesma tela, e ele não pode chegar ao decodificador da nova.
        leitura: u64,
    },
    /// Chegou uma transmissão e esta versão não sabe lê-la.
    ///
    /// **É a resposta à tela preta.** Quando o cabeçalho de um fluxo de tela não
    /// decodifica — versão do protocolo que este build não fala, campo que mudou
    /// de forma —, este lado não sabe nem o número da transmissão, então não há
    /// `TelaFechou` a mandar. Antes disto ele simplesmente voltava, e a casca
    /// nunca ficava sabendo que houve alguma coisa: nenhum evento, nenhum
    /// desenho, nenhuma frase.
    ///
    /// Foi relatado assim: «quem assiste com uma versão mais velha vê tela
    /// preta, sem mensagem nenhuma, e a sessão morre em ~3 segundos sem dizer
    /// por quê». A parte «sem mensagem nenhuma» é esta variante que não existia.
    ///
    /// O motivo viaja como texto porque é para uma pessoa ler, e porque o que o
    /// causou é justamente um formato que este build não conhece — enumerá-lo
    /// exigiria conhecer de antemão o que ainda não foi inventado.
    TelaIlegivel {
        /// O que o decodificador do cabeçalho respondeu.
        motivo: String,
    },
    /// A transmissão que estava chegando acabou.
    ///
    /// **Sai também quando a leitura é abortada** (a guarda `FimDaLeitura`):
    /// a troca de par e a queda abortam a tarefa no meio do laço, e o fim que
    /// só existia depois do laço não saía.
    TelaFechou {
        /// Qual transmissão.
        tela: ScreenId,
        /// Qual leitura acabou: o número do [`Self::TelaAbriu`] dela.
        leitura: u64,
    },
    /// A transmissão da **própria câmera** pede alguma coisa a quem a
    /// codifica, que está fora deste processo (`crate::de_fora`).
    ///
    /// A tela não precisa disto: o codificador dela mora aqui, e o pedido de
    /// quadro-chave e o teto novo vão direto à bomba. O da câmera mora na
    /// janela, e sem este aviso um `KeyFrameRequested` do servidor morreria
    /// deste lado — quem acabou de entrar esperaria uma imagem que ninguém
    /// mandou refazer.
    Camera {
        /// Qual transmissão.
        tela: ScreenId,
        /// O que ela pede.
        sinal: crate::de_fora::SinalParaFora,
    },
    /// O que um túnel de jogo mediu: caminho, latência, bytes, quedas.
    ///
    /// A cada segundo enquanto há tráfego, e sempre que o caminho ou o estado
    /// muda. O último de um túnel traz `encerrado: Some(..)`.
    Tunel(crate::tunel::RelatorioDoTunel),
    /// O servidor pede a esta pessoa que rode uma máquina de jogo (API 8 parte
    /// 3). Só sai de um enlace com tela ([`Enlace::aceitar_pedidos_de_maquina`]);
    /// sem ela, o pedido é recusado aqui e a casca não fica sabendo.
    ///
    /// **`resumo` é o que a casca compara e guarda**, já calculado por
    /// [`seele_proto::maquina::resumo_da_oferta`]: é ele que volta em
    /// [`Enlace::aceitar_maquina`] e que vai para
    /// [`crate::aceites_de_maquina::AceiteDeMaquina`]. Ninguém mais o recalcula
    /// — duas contas do mesmo resumo são duas contas para discordarem.
    ///
    /// O mesmo pedido (mesmo MOD, servidor de jogo e resumo) sai uma vez só
    /// enquanto a casca não responde, mesmo que o servidor o repita.
    PedidoDeMaquina {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O hash do pacote ligado, em hex.
        hash: String,
        /// O servidor de jogo.
        servidor: String,
        /// Quem pediu. `PersonId(0)` quando foi um momento do MOD, sem pessoa.
        pedido_por: PersonId,
        /// Tudo o que a pessoa vê antes de aceitar.
        oferta: Box<seele_proto::maquina::OfertaDeMaquina>,
        /// Em que papel a pessoa é chamada: a tela diz que é a reserva, e o
        /// resumo o amarra.
        papel: seele_proto::maquina::PapelDaMaquina,
        /// O resumo da oferta, em hex: o que se aceita.
        resumo: String,
    },
    /// A metade de servidor do MOD avisou a máquina deste servidor de jogo.
    AvisoParaMaquina {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// O que o MOD mandou.
        carga: String,
    },
    /// O servidor dispensou a máquina, e por quê. Pode chegar a toda sessão da
    /// pessoa, e não só à que aceitou: quem não roda aquela máquina ignora.
    MaquinaDispensada {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// O motivo.
        motivo: seele_proto::maquina::MotivoDaDispensa,
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
            Self::Tunel(relatorio) => f.debug_tuple("Tunel").field(relatorio).finish(),
            Self::Estado { estado, restante } => f
                .debug_struct("Estado")
                .field("estado", estado)
                .field("restante", restante)
                .finish(),
            Self::Reconectado { sessao, .. } => f
                .debug_struct("Reconectado")
                .field("sessao", sessao)
                .finish(),
            Self::Transferencia(estado) => f.debug_tuple("Transferencia").field(estado).finish(),
            Self::TelaAbriu {
                tela,
                largura,
                altura,
                origem,
                leitura,
            } => f
                .debug_struct("TelaAbriu")
                .field("tela", tela)
                .field("largura", largura)
                .field("altura", altura)
                .field("origem", origem)
                .field("leitura", leitura)
                .finish(),
            Self::TelaIlegivel { motivo } => f
                .debug_struct("TelaIlegivel")
                .field("motivo", motivo)
                .finish(),
            // Pelo mesmo motivo do quadro: os bytes viram um número.
            Self::TelaSom {
                tela,
                bytes,
                leitura,
            } => f
                .debug_struct("TelaSom")
                .field("tela", tela)
                .field("bytes", &bytes.len())
                .field("leitura", leitura)
                .finish(),
            // Os bytes viram um número: um quadro-chave de 1080p tem 65 KiB, e
            // despejá-los num log é apagar o log.
            Self::TelaQuadro {
                tela,
                chave,
                bytes,
                leitura,
            } => f
                .debug_struct("TelaQuadro")
                .field("tela", tela)
                .field("chave", chave)
                .field("bytes", &bytes.len())
                .field("leitura", leitura)
                .finish(),
            Self::TelaFechou { tela, leitura } => f
                .debug_struct("TelaFechou")
                .field("tela", tela)
                .field("leitura", leitura)
                .finish(),
            Self::Camera { tela, sinal } => f
                .debug_struct("Camera")
                .field("tela", tela)
                .field("sinal", sinal)
                .finish(),
            // A oferta fica de fora: o log diz qual pedido, e o resumo a
            // identifica inteira.
            Self::PedidoDeMaquina {
                mod_id,
                servidor,
                pedido_por,
                papel,
                resumo,
                ..
            } => f
                .debug_struct("PedidoDeMaquina")
                .field("mod_id", mod_id)
                .field("servidor", servidor)
                .field("pedido_por", pedido_por)
                .field("papel", papel)
                .field("resumo", resumo)
                .finish_non_exhaustive(),
            Self::AvisoParaMaquina {
                mod_id,
                servidor,
                carga,
            } => f
                .debug_struct("AvisoParaMaquina")
                .field("mod_id", mod_id)
                .field("servidor", servidor)
                .field("carga", &carga.len())
                .finish(),
            Self::MaquinaDispensada {
                mod_id,
                servidor,
                motivo,
            } => f
                .debug_struct("MaquinaDispensada")
                .field("mod_id", mod_id)
                .field("servidor", servidor)
                .field("motivo", motivo)
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
    /// O servidor recusou, e insistir não muda a resposta.
    ///
    /// Leva o [`ConnectError`] inteiro, e não um texto dele: a casca traduz a
    /// recusa pelo tipo (`fim_da_recusa`, na FFI). Um texto só servia ao log,
    /// e a tela recebia `CredentialRejected` para toda recusa — a chave que
    /// mudou, o servidor que não é o do convite e os MODs que mudaram viravam a
    /// mesma «credencial recusada».
    Recusado(ConnectError),
    /// Alguém pediu para sair.
    Pedido,
    /// Alguém do outro lado acabou com esta sessão, e o motivo veio no fio: um
    /// operador que expulsou ou baniu, ou **quem hospeda fechando o servidor**
    /// ([`DisconnectReason::ServerShuttingDown`]). Ver
    /// `a_sessao_acabou_aqui` para a lista inteira.
    ///
    /// Separado de [`Self::Recusado`] porque a casca precisa dizer **qual** foi:
    /// «você foi expulso» e «você foi banido» são frases diferentes. `Recusado`
    /// é a volta da bateria que o servidor (ou o TLS) não aceitou, e a FFI a
    /// traduz pelo [`ConnectError`] que ela leva; nenhum desses motivos é
    /// `Kicked` nem `Banned`, e cair numa frase de credencial mandaria a pessoa
    /// conferir a senha de uma conta que está certa.
    Moderado(DisconnectReason),
}

/// Um erro de um comando que é **recusa local**, e não queda: o quadro dele
/// não coube no fio (M2 da revisão do P61).
///
/// O `encode` do `seele-proto` confere cada campo contra o teto do contrato,
/// em bytes, e devolve `ControlError` antes de escrever um byte. `frame::write`
/// o passa adiante pelo `?`, e o `anyhow` o guarda inteiro. Qualquer outro
/// erro de escrita é do transporte, e é queda.
fn recusa_local(erro: &anyhow::Error) -> Option<&seele_proto::control::ControlError> {
    erro.downcast_ref::<seele_proto::control::ControlError>()
}

/// O que a casca manda fazer.
#[derive(Debug)]
enum Comando {
    ImagemDeMod {
        pedido: seele_proto::volume::PedidoDeImagem,
        bytes: Vec<u8>,
        resposta: tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>,
    },

    ModRequest {
        request: u32,
        id: String,
        channel: ChannelId,
        payload: String,
    },
    /// Uma ação declarada de MOD (API 7): o `ModRequest` com o nome da ação.
    AcaoDeMod {
        request: u32,
        id: String,
        channel: ChannelId,
        acao: String,
        payload: String,
    },
    /// Semeia a sonda com o caminho medido da última vez com este servidor.
    ///
    /// Comando e não parâmetro de `conectar`: os quatro `conectar*` públicos
    /// passam por um funil privado, e acrescentar um argumento a todos eles
    /// para um número opcional obrigaria cada chamador a dizer «não tenho» —
    /// enquanto quem tem é um só, o app, que é quem abre a lista de conhecidos.
    ///
    /// Chegar depois da conexão não custa nada: a sonda só começa a medir
    /// quando a tela transmite, e ninguém transmite antes de entrar.
    LembrarCaminho(u32),
    EntrarNaVoiceRoom(VoiceRoomId, Option<String>),
    SairDaVoiceRoom,
    AbrirLinha(ChannelId),
    Dizer {
        linha: ChannelId,
        corpo: String,
        /// A mensagem a que esta responde, se responde a alguma.
        responde_a: Option<MessageId>,
        id: ClientMessageId,
    },
    Historico {
        linha: ChannelId,
        limite: u16,
    },
    /// Uma página atrás de `cursor`, e logo atrás o `Ping` com `carimbo`. Ver
    /// [`Enlace::historico_com_cerca`].
    HistoricoComCerca {
        linha: ChannelId,
        cursor: Option<MessageId>,
        limite: u16,
        carimbo: u64,
    },
    Muted(bool),
    Isolamento(bool),
    CriarVoiceRoom {
        nome: String,
        limite: u16,
        linha: Option<ChannelId>,
    },
    CriarLinha {
        nome: String,
    },
    RenomearVoiceRoom {
        voice_room: VoiceRoomId,
        nome: String,
    },
    RenomearLinha {
        linha: ChannelId,
        nome: String,
    },
    RenomearServer {
        nome: String,
    },
    /// A imagem de perfil de quem está usando este cliente.
    /// O apelido de quem está usando este cliente.
    MeuApelido {
        /// O nome novo.
        nome: String,
    },
    MinhaImagem {
        /// A figura, ou `None` para não ter.
        icone: Option<Vec<u8>>,
    },
    /// O perfil público de quem está usando este cliente (0.19, protocolo 12):
    /// o `ClientMessage::GravarPerfil` já montado e validado por
    /// `seele_sessao::perfil::montar_gravar`.
    GravarPerfil(seele_proto::control::ClientMessage),
    /// Tira a foto ou a capa do perfil de quem está usando este cliente (0.19).
    TirarImagemDoPerfil {
        /// Qual das duas.
        lado: seele_proto::perfil::LadoDoPerfil,
    },
    IconeDoServer {
        icone: Option<Vec<u8>>,
    },
    Expulsar {
        pessoa: PersonId,
    },
    /// Um verbo de cargo com a revisão vista, ou `None` para o `PedirCargos`
    /// (ADR 0064). Não volta na reconexão: ver `Motor::lembrar`.
    Cargo(Option<(u64, seele_proto::cargos::PedidoDeCargo)>),
    Banir {
        pessoa: PersonId,
        motivo: Option<String>,
        expira_em: Option<i64>,
    },
    RemoverMensagem {
        mensagem: MessageId,
    },
    /// Os três verbos do protocolo 13. Nenhum volta na reconexão, pelo motivo
    /// de `Banir` e `RemoverMensagem`: ver `Motor::lembrar`.
    EditarMensagem {
        mensagem: MessageId,
        corpo: String,
    },
    Desbanir {
        pessoa: PersonId,
    },
    ListarBanidos,
    /// O que esta ponta mede da voz que ouve (protocolo 13). Não volta na
    /// reconexão: é uma medida de agora, e a seguinte sai em um segundo.
    RelatarRecepcao(Recepcao),
    MoverPersono {
        pessoa: PersonId,
        voice_room: VoiceRoomId,
    },
    ApagarVoiceRoom {
        voice_room: VoiceRoomId,
    },
    ApagarLinha {
        linha: ChannelId,
    },
    PesarLinha {
        linha: ChannelId,
    },
    Anexar(Box<Anexo>),
    /// Baixar um anexo para `pasta`, com o nome que veio com ele.
    ///
    /// A pasta e o nome, e não um caminho: quem junta os dois é
    /// [`crate::anexo_no_disco`] — [`crate::anexo_no_disco::abrir_parcial`]
    /// confere o nome e cria o parcial, e
    /// [`crate::anexo_no_disco::Parcial::nomear`] grava ao lado em vez de por
    /// cima.
    SalvarAnexo {
        anexo: AttachmentId,
        pasta: std::path::PathBuf,
        nome: String,
    },
    /// Baixar um anexo pequeno **para a memória**, para olhar os bytes dele.
    ///
    /// O segundo comando deste enum que carrega para onde responder, e pelo
    /// mesmo motivo do `PesarLinha`: a resposta só serve a quem perguntou,
    /// enquanto a caixa que ela enche estiver aberta. Um `Aviso` levaria
    /// megabytes pelo barramento de eventos, que existe para presença e
    /// andamento.
    PreverAnexo {
        anexo: AttachmentId,
        resposta: tokio::sync::oneshot::Sender<Previa>,
    },
    /// Comece a transmitir esta fonte, com estes tetos.
    ///
    /// Boxeado porque carrega o módulo do Cisco carregado e a captura numa
    /// caixa, e um enum é do tamanho do maior braço dele: sem a caixa, toda
    /// tecla digitada pagaria por eles na fila.
    CompartilharTela {
        fonte: Box<PedidoDeTela>,
        limites: LimitesDeTela,
    },
    /// Troque os tetos da transmissão que já está de pé.
    ///
    /// Sem caixa, ao contrário de [`Self::CompartilharTela`]: aqui não vai
    /// módulo nem captura, só três números.
    AjustarLimitesDaTela {
        /// Os tetos novos, como a pessoa os escolheu.
        limites: LimitesDeTela,
    },
    /// Pare de transmitir.
    PararDeCompartilhar,
    /// Peça um quadro-chave a quem está compartilhando.
    ///
    /// De quem **recebe**, e é o que faz alguém que entra no meio de uma
    /// transmissão ver alguma coisa: sem ele chegam só diferenças de um quadro
    /// que nunca se viu, e o decodificador as descarta.
    /// Pede para receber, ou parar de receber, a imagem de uma transmissão.
    ///
    /// **Um comando com uma bandeira, e não dois comandos.** Os dois lados
    /// carregam a mesma coisa — qual tela — e diferem numa palavra; separá-los
    /// duplicaria o caminho inteiro até o `Client` para trocar um verbo no fim.
    Assistir {
        /// Qual transmissão.
        tela: ScreenId,
        /// `true` para receber, `false` para parar.
        quero: bool,
    },
    PedirQuadroChave {
        /// Qual transmissão. O servidor confere se ela é mesmo a da sala.
        tela: ScreenId,
    },
    /// Declare ao servidor a identidade efêmera deste par.
    ///
    /// **Só existe para atravessar o canal de comandos.** Quem manda isto é
    /// [`Enlace::declarar_identidade_de_par`], chamado uma vez, do lado de
    /// fora, sobre o `Enlace` que já venceu a corrida de candidatos — nunca de
    /// dentro de `Motor::rodar`/`executar` por conta própria. Ver o doc de
    /// [`Motor::declarar_identidade_de_par`] para o porquê de precisar de
    /// `&mut self` inteiro, e não só do `&mut Client` que
    /// [`Motor::executar`] empresta para todo o resto deste `enum`.
    DeclararIdentidadeDePar,
    /// «Isto é o que eu consinto no caminho entre pares», inclusive quando a
    /// resposta nova é «nada».
    ///
    /// Substituiu um `EmprestarSubida(bool)`, que misturava as duas razões
    /// independentes do §5 — ver [`ConsentimentoDePar`]. Guardado em
    /// [`Motor::consentimento`] e redito a cada reconexão.
    ConsentirNoCaminhoEntrePares(ConsentimentoDePar),
    /// Quem joga pede para alcançar um túnel. A resposta é a porta local.
    EntrarNoTunel {
        mod_id: String,
        nome: String,
        resposta: tokio::sync::oneshot::Sender<Result<crate::tunel::PortaLocal, RecusaDeTunel>>,
    },
    /// Quem joga deixa um túnel.
    SairDoTunel {
        mod_id: String,
        nome: String,
    },
    /// Quem joga deixa todos os túneis de um MOD.
    SairDosTuneisDoMod {
        mod_id: String,
    },
    /// A máquina passa a servir um túnel.
    ServirTunel {
        mod_id: String,
        nome: String,
        protocolo: ProtocoloDeTunel,
        porta: u16,
        resposta: tokio::sync::oneshot::Sender<Result<(), RecusaDeTunel>>,
    },
    /// A máquina para de servir um túnel.
    PararTunel {
        mod_id: String,
        nome: String,
    },
    /// O aceite, a recusa ou a parada de uma máquina, já como verbo do fio.
    ///
    /// Um comando só para os três porque o motor faz o mesmo com eles: confere
    /// a versão, escreve, e esquece o pedido daquela máquina que já avisou à
    /// casca.
    VerboDaMaquina(ClientMessage),
    /// Um pedido da máquina à metade de servidor do MOD; a resposta vai por
    /// `resposta`.
    PedidoDaMaquina {
        mod_id: String,
        servidor: String,
        payload: String,
        resposta: tokio::sync::oneshot::Sender<Result<String, FalhaDoPedidoDaMaquina>>,
    },
    Sair,
}

impl Comando {
    /// O nome do comando, para o registro: **nunca** o que ele carrega.
    const fn nome(&self) -> &'static str {
        match self {
            Self::ImagemDeMod { .. } => "ImagemDeMod",
            Self::ModRequest { .. } => "ModRequest",
            Self::AcaoDeMod { .. } => "AcaoDeMod",
            Self::LembrarCaminho(_) => "LembrarCaminho",
            Self::EntrarNaVoiceRoom(..) => "EntrarNaVoiceRoom",
            Self::SairDaVoiceRoom => "SairDaVoiceRoom",
            Self::AbrirLinha(_) => "AbrirLinha",
            Self::Dizer { .. } => "Dizer",
            Self::Historico { .. } => "Historico",
            Self::HistoricoComCerca { .. } => "HistoricoComCerca",
            Self::Muted(_) => "Muted",
            Self::Isolamento(_) => "Isolamento",
            Self::CriarVoiceRoom { .. } => "CriarVoiceRoom",
            Self::CriarLinha { .. } => "CriarLinha",
            Self::RenomearVoiceRoom { .. } => "RenomearVoiceRoom",
            Self::RenomearLinha { .. } => "RenomearLinha",
            Self::RenomearServer { .. } => "RenomearServer",
            Self::MeuApelido { .. } => "MeuApelido",
            Self::MinhaImagem { .. } => "MinhaImagem",
            Self::GravarPerfil(..) => "GravarPerfil",
            Self::TirarImagemDoPerfil { .. } => "TirarImagemDoPerfil",
            Self::IconeDoServer { .. } => "IconeDoServer",
            Self::Expulsar { .. } => "Expulsar",
            Self::Cargo(_) => "Cargo",
            Self::Banir { .. } => "Banir",
            Self::RemoverMensagem { .. } => "RemoverMensagem",
            Self::EditarMensagem { .. } => "EditarMensagem",
            Self::Desbanir { .. } => "Desbanir",
            Self::ListarBanidos => "ListarBanidos",
            Self::RelatarRecepcao(_) => "RelatarRecepcao",
            Self::MoverPersono { .. } => "MoverPersono",
            Self::ApagarVoiceRoom { .. } => "ApagarVoiceRoom",
            Self::ApagarLinha { .. } => "ApagarLinha",
            Self::PesarLinha { .. } => "PesarLinha",
            Self::Anexar(_) => "Anexar",
            Self::SalvarAnexo { .. } => "SalvarAnexo",
            Self::PreverAnexo { .. } => "PreverAnexo",
            Self::CompartilharTela { .. } => "CompartilharTela",
            Self::AjustarLimitesDaTela { .. } => "AjustarLimitesDaTela",
            Self::PararDeCompartilhar => "PararDeCompartilhar",
            Self::Assistir { .. } => "Assistir",
            Self::PedirQuadroChave { .. } => "PedirQuadroChave",
            Self::DeclararIdentidadeDePar => "DeclararIdentidadeDePar",
            Self::ConsentirNoCaminhoEntrePares(_) => "ConsentirNoCaminhoEntrePares",
            Self::EntrarNoTunel { .. } => "EntrarNoTunel",
            Self::SairDoTunel { .. } => "SairDoTunel",
            Self::SairDosTuneisDoMod { .. } => "SairDosTuneisDoMod",
            Self::ServirTunel { .. } => "ServirTunel",
            Self::PararTunel { .. } => "PararTunel",
            Self::VerboDaMaquina(_) => "VerboDaMaquina",
            Self::PedidoDaMaquina { .. } => "PedidoDaMaquina",
            Self::Sair => "Sair",
        }
    }

    /// A máquina de que este comando fala, se ele é dela.
    fn da_maquina(&self) -> Option<(&str, &str)> {
        match self {
            Self::VerboDaMaquina(
                ClientMessage::AceiteDeMaquina {
                    mod_id, servidor, ..
                }
                | ClientMessage::RecusaDeMaquina {
                    mod_id, servidor, ..
                }
                | ClientMessage::MaquinaParada {
                    mod_id, servidor, ..
                },
            )
            | Self::PedidoDaMaquina {
                mod_id, servidor, ..
            } => Some((mod_id, servidor)),
            _ => None,
        }
    }
}

/// Quanto tempo se espera o servidor abrir o fluxo de um anexo pedido.
///
/// Uma recusa nunca abre fluxo nenhum — a razão vem pelo controle —, então sem
/// prazo esta espera seria para sempre. Dez segundos é muito mais do que um
/// servidor doméstico leva para começar a mandar e pouco para deixar uma tela
/// esperando por bytes que não vêm.
const ESPERA_DE_ANEXO: Duration = Duration::from_secs(10);

/// Quanto tempo se dá ao caminho entre pares antes de cair para o servidor.
///
/// Generoso o bastante para um furo de NAT em rede doméstica — o roteiro de
/// duas máquinas de `docs/teste-duas-maquinas.md` é quem mede o real — e curto
/// o bastante para quem está esperando a imagem não ficar olhando para uma
/// tela parada além do razoável: passado ele, `crate::par::por_onde` cai para o
/// servidor sem drama, e é exatamente esse o ponto da malha ser alívio e nunca
/// dependência.
const PRAZO_DO_PAR: Duration = Duration::from_secs(3);

/// O que uma tentativa do caminho entre pares, resolvida numa tarefa solta,
/// devolve ao laço de [`Motor::rodar`] para ele agir.
///
/// Só existe porque a tentativa é assíncrona e pode levar até
/// [`PRAZO_DO_PAR`]: bloquear o laço principal por isso pausaria a voz, o ping
/// e o resto desta sessão até o par responder ou o prazo vencer. A tarefa roda
/// solta (ver [`Motor::assistir_por_par`]) e devolve o que decidiu por aqui; só
/// quem tem o `&mut Client` — o laço de `rodar` — pode falar com o servidor.
#[derive(Debug)]
enum ResultadoDoPar {
    /// Um verbo do túnel que uma tarefa solta quer mandar (`TunelFalhou`,
    /// `SairDoTunel`): só o laço, dono do `&mut Client`, escreve no fluxo.
    ParaOServidor(ClientMessage),
    /// O par não veio, ou ligou e nunca abriu a transmissão: o servidor
    /// precisa saber, para poder ele mesmo assumir. Ver
    /// [`seele_proto::control::ClientMessage::ParFalhou`].
    ParFalhou {
        /// Qual transmissão.
        screen: ScreenId,
        /// O que aconteceu.
        motivo: MotivoDeFalhaDePar,
    },
}

/// Um arquivo para mandar, com a mensagem que vai junto.
///
/// ADR 0027. O corpo viaja com o arquivo e não num `Dizer` separado: a
/// mensagem só é publicada quando os bytes chegam inteiros, e duas metades da
/// mesma mensagem em dois caminhos teriam uma ordem para errar.
#[derive(Debug, Clone)]
pub struct Anexo {
    /// Em que Linha.
    pub linha: ChannelId,
    /// A chave de idempotência da mensagem. É por ela que a tela reconhece a
    /// própria subida, e é ela que torna uma retentativa segura.
    pub id: ClientMessageId,
    /// O que a pessoa escreveu ao lado do arquivo. Pode ser vazio.
    pub corpo: String,
    /// A mensagem a que esta responde, se responde a alguma. O servidor
    /// confere que ela é do mesmo canal, como no envio sem arquivo.
    pub responde_a: Option<MessageId>,
    /// Onde o arquivo está nesta máquina. Nunca sai daqui.
    pub caminho: std::path::PathBuf,
    /// Que nome dar a ele do outro lado.
    pub nome: String,
    /// Que tipo alegar que ele é. Alegação, e tratada como tal.
    pub tipo: String,
}

/// O que voltou de um pedido de prévia. ADR 0027.
///
/// **Nada disto encosta no disco.** É a linha entre prever e salvar: salvar é
/// um ato de quem recebeu, num lugar que a pessoa escolheu, e uma miniatura que
/// deixasse uma cópia num diretório de cache teria feito esse ato acontecer sem
/// ninguém pedir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Previa {
    /// Os bytes, inteiros e conferidos contra o hash, só na memória.
    Bytes(Vec<u8>),
    /// Maior do que esta janela desenha. Nenhum byte foi lido.
    ///
    /// Enumerado e não erro: nada deu errado, e quem está olhando merece uma
    /// frase diferente da que uma transferência quebrada recebe.
    GrandeDemais {
        /// O que o servidor teria mandado.
        tamanho: u64,
    },
    /// Não veio: expirou, não existe, ou não chegou inteiro. A razão, quando é
    /// do servidor, chega pelo controle como `ServerMessage::AttachmentUnavailable`.
    NaoVeio,
}

/// Onde uma transferência está.
///
/// Enumerado, e é o que a tela precisa para não mentir: enquanto sobe, uma
/// barra com o total — que é sempre conhecido, porque quem escolheu o arquivo
/// sabe o tamanho dele. **Se cair, [`Transferencia::Caiu`]**, e a frase dessa
/// variante tem de dizer que recomeçar recomeça do zero: o ADR 0027 não tem
/// retomada, e isso precisa ser dito a quem está esperando em vez de
/// descoberto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transferencia {
    /// Subindo, com quantos bytes já foram de quantos.
    Subindo {
        /// Qual mensagem.
        id: ClientMessageId,
        /// Bytes que já saíram.
        feito: u64,
        /// Bytes ao todo. Sempre conhecido.
        total: u64,
    },
    /// Todos os bytes saíram. A mensagem aparece na Linha em seguida.
    Subiu {
        /// Qual mensagem.
        id: ClientMessageId,
    },
    /// O servidor cortou o fluxo: recusou. A razão vem pelo controle, como
    /// `ServerMessage::AttachmentRefused`.
    Recusada {
        /// Qual mensagem.
        id: ClientMessageId,
    },
    /// O enlace caiu no meio. **Recomeçar recomeça do zero.**
    Caiu {
        /// Qual mensagem.
        id: ClientMessageId,
    },
    /// Baixando, com quantos bytes já vieram de quantos.
    Baixando {
        /// Qual anexo.
        anexo: AttachmentId,
        /// Bytes que já chegaram.
        feito: u64,
        /// Bytes ao todo.
        total: u64,
    },
    /// O arquivo está no disco de quem recebeu.
    Salvo {
        /// Qual anexo.
        anexo: AttachmentId,
        /// Onde ficou de verdade: «foto (2).png», se o nome estava tomado e foi
        /// ao lado que ele ficou.
        caminho: std::path::PathBuf,
    },
    /// Não deu para salvar. Se o motivo for do servidor, ele vem pelo controle
    /// como `ServerMessage::AttachmentUnavailable`.
    NaoSalvou {
        /// Qual anexo.
        anexo: AttachmentId,
    },
}

/// Um candidato que o laço está prestes a tentar, para quem observa de fora.
///
/// Existe porque o laço é o único lugar que sabe as duas coisas ao mesmo tempo:
/// **qual** endereço está sendo tentado agora e **se um `LEVE` saiu por ele**. A
/// segunda só é verdade depois do envio, e quem publica etapas fica uma camada
/// acima — [`crate::chegada::Chegada`], que antes disto publicava a primeira
/// tentativa antes de o laço começar e não via nenhuma das outras.
///
/// Três campos e não o [`Destino`] inteiro: o que atravessa aqui já estava no
/// convite de quem vai ler, e mandar o segredo de entrada junto seria pôr uma
/// credencial numa trilha que vira log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tentativa {
    /// Qual da lista, contando do zero.
    pub candidato: u8,
    /// O endereço que está sendo tentado.
    pub onde: SocketAddr,
    /// Um `LEVE` saiu pelo ponto de encontro por causa deste candidato.
    ///
    /// O que `avisar_pelo_candidato` respondeu: verdadeiro só quando o
    /// datagrama saiu de verdade. Sem bilhete, num candidato que não precisa de
    /// furo, ou com o envio recusado, é falso — e é falso pelo mesmo critério
    /// que faz o log daquela função só dizer que avisou quando avisou.
    pub avisou: bool,
}

/// O que a corrida conta a quem acompanha a chegada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Andamento {
    /// Um candidato começou.
    Tentando(Tentativa),
    /// Um candidato ouviu `Tocando` de uma sala pessoal (tarefa 16 da F7): ele
    /// venceu a corrida, e quem recebe está sendo chamado.
    Tocando {
        /// Qual da lista, contando do zero: o mesmo índice da [`Tentativa`].
        candidato: u8,
        /// O prazo do toque que a sala mandou, em segundos.
        prazo_s: u16,
    },
}

/// A resposta que [`Enlace::pedir_entrada_no_tunel`] ainda espera do servidor.
///
/// Dona da caixa e de mais nada: pode ir para outra tarefa enquanto o enlace
/// segue servindo quem o usa.
#[derive(Debug)]
pub struct EsperaDeTunel {
    caixa: tokio::sync::oneshot::Receiver<Result<crate::tunel::PortaLocal, RecusaDeTunel>>,
}

impl EsperaDeTunel {
    /// Espera até `PRAZO_DO_PAR`. Sem resposta (sessão caída, prazo vencido) a
    /// recusa é [`RecusaDeTunel::SemResposta`].
    ///
    /// # Errors
    ///
    /// A recusa do servidor, ou as locais de [`Enlace::entrar_no_tunel`].
    pub async fn aguardar(self) -> Result<crate::tunel::PortaLocal, RecusaDeTunel> {
        match tokio::time::timeout(PRAZO_DO_PAR, self.caixa).await {
            Ok(Ok(resultado)) => resultado,
            Ok(Err(_)) | Err(_) => Err(RecusaDeTunel::SemResposta),
        }
    }
}

/// Quanto se espera a resposta de um pedido da máquina: os 15 s com que a
/// janela de um MOD espera a dela. A metade de servidor é a mesma, e um prazo
/// diferente só porque quem pergunta é a máquina daria duas paciências ao
/// mesmo MOD.
pub const PRAZO_DO_PEDIDO_DA_MAQUINA: Duration = Duration::from_secs(15);

/// Por que um pedido da máquina não teve resposta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalhaDoPedidoDaMaquina {
    /// O servidor disse que esta conexão não é a máquina aceita daquele
    /// servidor de jogo (`MaquinaDispensada { SemAceite }`).
    SemAceite,
    /// Nada voltou: prazo vencido, sessão caída, ou o pedido nem saiu (o
    /// porquê vai ao log).
    SemResposta,
}

impl FalhaDoPedidoDaMaquina {
    /// O nome em kebab-case, o que a casca recebe.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::SemAceite => "sem-aceite",
            Self::SemResposta => "sem-resposta",
        }
    }
}

impl std::fmt::Display for FalhaDoPedidoDaMaquina {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.nome())
    }
}

impl std::error::Error for FalhaDoPedidoDaMaquina {}

/// A resposta que [`Enlace::pedir_da_maquina`] ainda espera.
///
/// Pela razão de [`EsperaDeTunel`]: o laço da FFI não pode parar 15 s por um
/// pedido, e esta espera vai para outra tarefa sem segurar o enlace.
#[derive(Debug)]
pub struct EsperaDoPedidoDaMaquina {
    caixa: tokio::sync::oneshot::Receiver<Result<String, FalhaDoPedidoDaMaquina>>,
}

impl EsperaDoPedidoDaMaquina {
    /// Espera até [`PRAZO_DO_PEDIDO_DA_MAQUINA`].
    ///
    /// # Errors
    ///
    /// As de [`FalhaDoPedidoDaMaquina`].
    pub async fn aguardar(self) -> Result<String, FalhaDoPedidoDaMaquina> {
        match tokio::time::timeout(PRAZO_DO_PEDIDO_DA_MAQUINA, self.caixa).await {
            Ok(Ok(resultado)) => resultado,
            Ok(Err(_)) | Err(_) => Err(FalhaDoPedidoDaMaquina::SemResposta),
        }
    }
}

/// A sessão com um servidor, viva através de quedas.
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
    /// O caminho de subida que a sonda mediu, em bits por segundo. Zero é
    /// «ninguém mediu ainda».
    ///
    /// Um átomo pela mesma razão que o `rtt`: quem o lê é quem grava a lista de
    /// conhecidos, e transformar cada janela de medição num aviso encheria a
    /// fila de coisas que ninguém precisa ver acontecer.
    caminho_medido: Arc<std::sync::atomic::AtomicU32>,
    /// O protocolo que a conexão de agora negociou (`Client::versao_negociada`),
    /// escrito pelo motor a cada (re)conexão. Um átomo pela razão do `rtt`: é
    /// lido de fora a cada pedido de perfil, e não vale um aviso.
    protocolo: Arc<std::sync::atomic::AtomicU8>,
    /// O que a máquina que serve túneis de jogo já contou, para quem investiga.
    maquina: Arc<crate::tunel::EstadoDaMaquina>,
    /// Se este enlace tem tela para mostrar um pedido de máquina. Dividido
    /// com o motor, que é quem decide na chegada do pedido.
    com_tela: Arc<std::sync::atomic::AtomicBool>,
    /// A tarefa do motor. `None` depois de [`Enlace::sair`], que a espera
    /// terminar — ou a aborta, vencido o teto.
    tarefa: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for Enlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enlace")
            .field("estado", &self.estado)
            .field("server", &self.sessao.server)
            .finish()
    }
}

/// Fila de comandos. Controle é raro; isto é folga, não capacidade.
const COMANDOS: usize = 32;

/// Quanto tempo cada endereço do convite ganha antes de o próximo ser tentado.
///
/// Quatro segundos. O número sai de duas contas em sentidos opostos. Para
/// baixo: um aperto de mão QUIC numa rede doméstica com perda cabe folgado em
/// quatro segundos, e cortar antes disso descartaria um endereço que ia dar
/// certo. Para cima: um endereço que não volta — o público visto de dentro da
/// própria casa, o de uma VPN que não aceita entrada — gasta o prazo inteiro,
/// e ele é multiplicado pelo número de candidatos antes de a sala abrir.
///
/// É por isso que a ordem do convite importa mais que este número: com a rede
/// de casa em primeiro, o caso comum nunca chega a esperar nada disto.
const PRAZO_POR_CANDIDATO: Duration = Duration::from_secs(4);

/// Quanto se espera antes de disparar o próximo candidato da corrida.
///
/// # O número que esta constante existe para apagar
///
/// Medido em campo, com o cliente rodando de um 5G contra um servidor de
/// verdade: quatro candidatos, os três primeiros sem chance, **9,6 segundos**
/// queimados — e o quarto respondeu em **358 ms**. Em série, o quarto quase
/// nunca era alcançado, e a tela dizia «tempo esgotado» sobre um servidor que
/// estava no ar.
///
/// Aqui o quarto **começa** em 750 ms, e a conversa abre em ~1,1 s.
///
/// # A previsão que estava escrita aqui, e a resposta
///
/// O que esta constante substituiu — `PRAZO_DA_PRIMEIRA_VOLTA`, a paciência
/// curta da primeira de duas voltas em série — carregava esta frase, e ela
/// merece sobreviver ao código que a hospedava:
///
/// > O que isto **não** faz é tentar em paralelo, que seria melhor ainda. Vários
/// > apertos de mão ao mesmo tempo escrevem pins ao mesmo tempo, e o pin é a
/// > propriedade do ADR 0003: um vencedor e três órfãos exigem um desenho
/// > próprio para decidir qual fica. Fica registrado como o próximo passo, e não
/// > como esquecimento.
///
/// Este é o próximo passo, e o desenho próprio existe: a limpeza de pin dos
/// perdedores roda depois do vencedor e pula a chave dele. Ver o ADR 0037, e o
/// bloco que faz isso em [`Enlace::tentar_entre`].
///
/// # Por que 250 ms
///
/// É o número do RFC 8305. A medição da pendência nº 26 o justifica:
/// com quatro candidatos o último começa em 750 ms, e o bom respondeu em 358 ms
/// depois disso — contra os 9,6 s que a série cobrava. Encurtar para 150 ms
/// ganharia ~300 ms e poria mais apertos de mão simultâneos numa rede lenta,
/// onde vários teriam fechado sozinhos. Ver o ADR 0037.
const DEFASAGEM_ENTRE_CANDIDATOS: Duration = Duration::from_millis(250);

/// O que uma corrida produziu.
///
/// As falhas vêm junto com o vencedor de propósito: quem chama precisa delas
/// para a limpeza de pin órfão dos perdedores, e precisa saber **quem** venceu
/// para pular a chave dele. Ver o ADR 0037.
#[derive(Debug)]
struct Corrida<P, T> {
    /// Quem fechou primeiro, e a posição dele em [`Corrida::iniciados`].
    vencedor: Option<(usize, T)>,
    /// Quem não fechou, na ordem em que desistiram.
    falhas: Vec<(usize, ConnectError)>,
    /// Quem começou, **na ordem em que começou**: a posição de um corredor é o
    /// índice dele aqui. Quem ficou na fila sem começar não está, porque não
    /// tentou nada e não tem o que limpar.
    iniciados: Vec<P>,
}

/// Corre os candidatos da `fila`, disparando um a cada `defasagem`, e fica com
/// o primeiro que fechar.
///
/// É o RFC 8305 — «Happy Eyeballs» — e a razão de existir está medida na
/// pendência nº 26: em série, três candidatos sem chance custam 9,6 s antes de o
/// quarto sequer ser tentado.
///
/// # Os que chegam depois
///
/// `tardios` é quem só fica pronto com a corrida já andando: o que o quarto
/// do ponto de encontro dá a quem volta a um servidor guardado (a pendência
/// 55). Antes, a corrida acabava quando a lista dela acabava, e quem consultava
/// o quarto tinha de esperar a resposta antes de começar — até 1,5 s, pagos
/// também por quem estava na mesma casa.
///
/// - **O lote que chega entra na frente** de quem ainda espera a vez, e o
///   primeiro dele começa na hora. A defasagem recomeça dele.
/// - **Um lote vazio não mexe em nada**: ninguém começa fora da vez.
/// - **A corrida espera o lote** quando ninguém mais corre e a fila acabou. Ela
///   não acaba antes de ele chegar, e por isso quem passa `tardios` responde
///   por ele chegar: o quarto tem prazo.
/// - **Quem já ganhava não espera**: o primeiro a fechar vence, chegue o lote
///   quando chegar.
/// - **A posição é a ordem em que cada um começou**, e não a da lista. A trilha
///   numera por ela, e a regra de que só o último candidato desiste
///   (`Etapa::transicao_legal`) continua valendo com quem chegou depois.
///
/// Sem `tardios`, a corrida é a de sempre, nos mesmos instantes: os testes de
/// `a_corrida` escrevem esses instantes em números.
///
/// # Por que genérica
///
/// Para que a defasagem e o «primeiro vence» tenham teste sem socket. Um teste
/// que provasse isto com sockets de verdade dependeria de rede, e o que precisa
/// ser provado aqui não é a rede: é que o segundo candidato **começa** sem
/// esperar o primeiro terminar, e que o rápido vence mesmo estando por último na
/// lista. Os testes moram em `a_corrida`.
///
/// # Por que `JoinSet` e não `futures::FuturesUnordered`
///
/// Para não acrescentar dependência. O `tokio` já entra com `features =
/// ["full"]`, e este repositório documenta em `Cargo.toml` o que cada
/// dependência arrasta — uma a mais para isto seria custo sem necessidade.
///
/// # Quem toca vence
///
/// `tocados` traz a posição de quem ouviu `Tocando` de uma sala pessoal
/// (tarefa 16 da F7). Esse candidato venceu, mesmo sem ter terminado: quem
/// recebe já está sendo chamado, e qualquer outro que chegasse à sala seria
/// **um segundo toque** — a mesma pessoa por outro caminho, que a sala anuncia
/// como outro `Tocou` e que fecha calado o toque de antes. Por isso, no primeiro
/// aviso:
///
/// - ninguém mais começa: a fila e os tardios acabam ali;
/// - quem já corria é derrubado, e o que ele entregar depois não conta;
/// - a corrida espera quem toca terminar, e o desfecho é o dele: a sessão, se
///   atenderam, ou a razão da sala, se não.
///
/// Os derrubados não entram em `falhas`, como os que o `JoinSet` derruba no fim:
/// numa ligação pelo código o pino só é escrito depois da sessão, e eles não
/// chegaram lá.
///
/// # Cancelamento
///
/// As tentativas perdedoras são derrubadas com o `JoinSet` ao fim desta função.
/// Elas não continuam escrevendo em lugar nenhum, e o que uma delas possa ter
/// escrito em disco — um pin de TLS — é assunto de quem chama, que sabe qual
/// chave o vencedor usou.
async fn correr<P, T, F, Fut, L>(
    fila: Vec<P>,
    defasagem: Duration,
    tardios: Option<L>,
    tocados: Option<mpsc::UnboundedReceiver<usize>>,
    tentar: F,
) -> Corrida<P, T>
where
    P: Clone,
    T: Send + 'static,
    F: Fn(usize, P) -> Fut,
    Fut: std::future::Future<Output = Result<T, ConnectError>> + Send + 'static,
    L: std::future::Future<Output = Vec<P>>,
{
    let mut fila: std::collections::VecDeque<P> = fila.into();
    let mut corredores = tokio::task::JoinSet::new();
    // Quem corre, pela posição: é por ela que os outros caem quando um toca.
    let mut no_ar: Vec<(usize, tokio::task::AbortHandle)> = Vec::new();
    // Quem ouviu `Tocando`: daí em diante a corrida é dele.
    let mut tocando: Option<usize> = None;
    let mut tocados = tocados;
    let mut falhas: Vec<(usize, ConnectError)> = Vec::new();
    let mut iniciados: Vec<P> = Vec::new();
    // Sem tardios, eles já chegaram: um lote vazio, que nunca é esperado.
    let mut tardios_chegaram = tardios.is_none();
    let tardios = async move {
        match tardios {
            Some(tardios) => tardios.await,
            None => Vec::new(),
        }
    };
    tokio::pin!(tardios);
    // O primeiro sai sem espera nenhuma.
    let mut disparar = true;
    // A vez do próximo, contada de quem começou por último. É um instante, e
    // não um `sleep` refeito a cada volta do laço: um lote vazio que chega no
    // meio da defasagem não pode empurrar a vez de ninguém.
    let mut vez = tokio::time::Instant::now() + defasagem;

    loop {
        // Dispara o próximo, se for a vez e ainda houver.
        if std::mem::take(&mut disparar) {
            if let Some(proximo) = fila.pop_front() {
                let posicao = iniciados.len();
                iniciados.push(proximo.clone());
                let futuro = tentar(posicao, proximo);
                let alca = corredores.spawn(async move { (posicao, futuro.await) });
                no_ar.push((posicao, alca));
                vez = tokio::time::Instant::now() + defasagem;
            }
        }

        if corredores.is_empty() && fila.is_empty() {
            if tardios_chegaram {
                break;
            }
            // Ninguém correndo e ninguém na vez: só o lote pode trazer alguém,
            // e a corrida o espera em vez de acabar sem ele.
            let lote = (&mut tardios).await;
            tardios_chegaram = true;
            disparar = receber_o_lote(&mut fila, lote);
            continue;
        }

        // Enquanto os que já estão no ar correm, o relógio da defasagem anda. Se
        // alguém fechar antes dela, a corrida acaba ali — que é o caso comum, e
        // é o motivo de isto existir.
        // `biased`, com o toque primeiro: um toque e uma sessão de outro
        // candidato prontos no mesmo instante são o toque, e a sessão cai no
        // braço de baixo que larga quem não tocou.
        let terminou = tokio::select! {
            biased;
            Some(quem) = ouvir_um_toque(&mut tocados), if tocando.is_none() => {
                tocando = Some(quem);
                fila.clear();
                tardios_chegaram = true;
                for (posicao, alca) in &no_ar {
                    if *posicao != quem {
                        alca.abort();
                    }
                }
                continue;
            }
            terminou = corredores.join_next(), if !corredores.is_empty() => terminou,
            () = tokio::time::sleep_until(vez), if !fila.is_empty() => {
                disparar = true;
                continue;
            }
            lote = &mut tardios, if !tardios_chegaram => {
                tardios_chegaram = true;
                disparar = receber_o_lote(&mut fila, lote);
                continue;
            }
        };
        // Quem terminou sem vencer abre a vez do próximo, sem esperar a
        // defasagem: é o comportamento de sempre.
        disparar = true;

        match terminou {
            // Depois de um toque, só o desfecho de quem toca conta: uma sessão
            // que outro candidato tenha aberto no mesmo instante cai aqui.
            Some(Ok((posicao, _))) if tocando.is_some_and(|quem| quem != posicao) => {}
            Some(Ok((posicao, Ok(pronto)))) => {
                // O `JoinSet` derruba o resto ao ser recolhido no fim desta
                // função.
                return Corrida {
                    vencedor: Some((posicao, pronto)),
                    falhas,
                    iniciados,
                };
            }
            Some(Ok((posicao, Err(erro)))) => falhas.push((posicao, erro)),
            // Uma tentativa que entrou em pânico ou foi cancelada. Não é
            // vencedora e não tem erro próprio a contar.
            Some(Err(_)) | None => {}
        }
    }

    Corrida {
        vencedor: None,
        falhas,
        iniciados,
    }
}

/// O próximo aviso de toque da corrida, ou nunca, quando ela não os escuta.
///
/// `None` quando o canal fechou: o ramo do `select!` desarma naquela volta, e
/// os outros seguem.
async fn ouvir_um_toque(tocados: &mut Option<mpsc::UnboundedReceiver<usize>>) -> Option<usize> {
    match tocados {
        Some(tocados) => tocados.recv().await,
        None => std::future::pending().await,
    }
}

/// O prazo de um candidato da corrida, que deixa de valer quando ele toca
/// (tarefa 16 da F7).
///
/// Sem `prazo`, a tentativa vai até o fim, e só o toque é escutado (o caminho
/// de candidato único). `None` é o prazo vencido sem toque. Com o toque, o aperto de mão inteiro
/// passa a ser medido pelo prazo do toque, dentro de `Client::connect_por`, e
/// o do candidato, que existe para soltar quem nunca respondeu, cortaria uma
/// ligação que está chamando. `ao_tocar` roda uma vez, com o `prazo_s`.
///
/// Um `select!` sobre a tentativa **inteira**, e não sobre uma leitura: quem
/// perde é largado de uma vez, com a conexão dele.
async fn ate_o_prazo_ou_o_toque<T, M>(
    prazo: Option<Duration>,
    tentativa: impl std::future::Future<Output = T>,
    ouviu: &mut mpsc::UnboundedReceiver<M>,
    ao_tocar: impl FnOnce(M),
) -> Option<T> {
    tokio::pin!(tentativa);
    let limite = async {
        match prazo {
            Some(prazo) => tokio::time::sleep(prazo).await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(limite);
    let mut ao_tocar = Some(ao_tocar);
    loop {
        tokio::select! {
            biased;
            resultado = &mut tentativa => return Some(resultado),
            Some(prazo_s) = ouviu.recv(), if ao_tocar.is_some() => {
                if let Some(ao_tocar) = ao_tocar.take() {
                    ao_tocar(prazo_s);
                }
            }
            () = &mut limite, if ao_tocar.is_some() => return None,
        }
    }
}

/// O que o quarto do ponto de encontro deu a quem volta a um servidor guardado,
/// para a corrida que já está andando (a pendência 55).
///
/// Quem monta é [`crate::chegada::Chegada`], que pergunta ao quarto: ela sabe
/// quem já corre, e só manda o endereço de hoje quando ele é novo.
#[derive(Debug, Clone, Default)]
pub(crate) struct DoQuarto {
    /// O endereço de hoje como candidato, quando ele não é nenhum dos que já
    /// correm.
    pub(crate) candidato: Option<Destino>,
}

/// Um candidato da corrida: o destino e o pin da chave dele **antes** da
/// corrida, para a limpeza dos perdedores.
#[derive(Clone)]
struct Candidato {
    destino: Destino,
    antes: Option<String>,
}

/// O que corre na corrida desde a 0.17: **um agente ICE**, e não um endereço.
///
/// Até a 0.16 cada endereço do convite era um aperto de mão QUIC próprio, e a
/// corrida do ADR 0037 disparava um a cada 250 ms. Com WebRTC, o ICE já tenta
/// vários candidatos de uma vez e fica com o primeiro que responde: os endereços
/// diretos de um anfitrião vão juntos numa discagem só (D2). O que sobra como
/// corrida é entre **caminhos**: o direto, o do degrau 4 (o `CHAMA` pelo ponto)
/// e o que o quarto devolve depois.
#[derive(Clone)]
enum Lote {
    /// Os endereços que se discam direto, todos num agente ICE.
    Direto(Vec<Candidato>),
    /// O degrau 4 (D7): chamar o anfitrião pelo ponto de encontro e discar o
    /// endereço que ele devolver, com o `U` do `CHAMA`.
    ///
    /// `modelo` é o destino cujas regras o endereço devolvido herda — a
    /// impressão esperada, a chave de pino, o apelido, o segredo —, **sem** a
    /// prova pelo pino: é um candidato que ninguém escolheu
    /// ([`Destino::o_pino_prova_o_servidor`]).
    ///
    /// `do_convite_v6` são os candidatos IPv6 do anfitrião que o convite já
    /// trazia ([`e_ipv6`]). Servem à **segunda chamada**
    /// (`crate::encontro::Chamada::chamar_pelo_ipv6`), a que sai quando o
    /// anfitrião atendeu e o IPv4 não abriu: é até eles que o sistema escolhe a
    /// origem que vira candidato, e é a eles que a discagem seguinte vai, com
    /// o que o segundo `AQUI` der. O `AQUI` não diz IPv6 nenhum do anfitrião:
    /// o `ATENDE` só leva o endereço que o ponto viu e as dicas de rede local.
    PelaChamada {
        chamada: crate::encontro::Chamada,
        modelo: Box<Candidato>,
        do_convite_v6: Vec<Candidato>,
    },
}

impl Lote {
    /// Os candidatos de antes da corrida, para a limpeza de pin órfão.
    fn candidatos(&self) -> Vec<&Candidato> {
        match self {
            Self::Direto(candidatos) => candidatos.iter().collect(),
            Self::PelaChamada {
                modelo,
                do_convite_v6,
                ..
            } => std::iter::once(modelo.as_ref())
                .chain(do_convite_v6)
                .collect(),
        }
    }

    /// Um endereço para o log de quem falhou.
    fn onde(&self) -> Option<SocketAddr> {
        match self {
            Self::Direto(candidatos) => candidatos.first().map(|c| c.destino.servidor),
            Self::PelaChamada { chamada, .. } => Some(chamada.ponto()),
        }
    }
}

/// Se este candidato depende de alguém ter furado o caminho até ele: há
/// chamada possível, e o endereço é público.
///
/// **A pergunta é uma só**, a que decide quem espera o quarto: quem não
/// precisa de furo não tem por que esperar a escuta de hoje.
fn pede_furo(chamada: Option<&crate::encontro::Chamada>, onde: SocketAddr) -> bool {
    chamada.is_some() && e_publico(onde.ip())
}

/// O lote que o quarto solta na corrida: quem esperava, e o endereço de hoje
/// logo depois do alvo (se o alvo esperava) ou na frente (se não).
fn lote_do_quarto(
    do_quarto: DoQuarto,
    esperam: Vec<Candidato>,
    o_alvo_espera: bool,
    pins: &dyn PinStore,
) -> Vec<Lote> {
    let mut candidatos = esperam;
    if let Some(destino) = do_quarto.candidato {
        let de_hoje = Candidato {
            antes: pins.pinned(&destino.chave_do_pin),
            destino,
        };
        candidatos.insert(usize::from(o_alvo_espera).min(candidatos.len()), de_hoje);
    }
    if candidatos.is_empty() {
        Vec::new()
    } else {
        vec![Lote::Direto(candidatos)]
    }
}

/// Põe o lote que chegou depois **na frente** de quem espera a vez, na ordem
/// dele, e diz se alguém tem de começar agora: o primeiro do lote começa na
/// hora, e um lote vazio não dispara ninguém fora da vez.
fn receber_o_lote<P>(fila: &mut std::collections::VecDeque<P>, lote: Vec<P>) -> bool {
    let alguem = !lote.is_empty();
    for tardio in lote.into_iter().rev() {
        fila.push_front(tardio);
    }
    alguem
}

/// O que uma chamada que não deu em endereço vira para quem liga.
fn falha_da_chamada(falha: crate::encontro::FalhaDaChamada) -> ConnectError {
    use crate::encontro::{FalhaAoLocalizar, FalhaDaChamada};
    ConnectError::NaoLocalizou(match falha {
        FalhaDaChamada::Ninguem => FalhaAoLocalizar::NinguemMora,
        FalhaDaChamada::Calado => FalhaAoLocalizar::AnfitriaoCalado,
        FalhaDaChamada::Recusa => FalhaAoLocalizar::PontoRecusou,
        FalhaDaChamada::Mudo => FalhaAoLocalizar::PontoMudo,
    })
}

/// Os destinos que um `AQUI` do degrau 4 dá: o endereço público e as dicas de
/// rede local, cada um herdando as regras do `modelo` sem a prova pelo pino.
fn destinos_da_chamada(modelo: &Candidato, atendida: &crate::encontro::Atendida) -> Vec<Candidato> {
    std::iter::once(atendida.endereco)
        .chain(atendida.dicas.iter().copied())
        .map(|servidor| Candidato {
            destino: Destino {
                servidor,
                o_pino_prova_o_servidor: false,
                ..modelo.destino.clone()
            },
            antes: modelo.antes.clone(),
        })
        .collect()
}

/// Os destinos da discagem que segue a **segunda chamada** do degrau 4, a que
/// levou o IPv6 desta máquina: os do segundo `AQUI`
/// ([`destinos_da_chamada`]), e os IPv6 do anfitrião que o convite já trazia,
/// cada um com as regras que tinha.
///
/// **Os do convite vão junto porque o `AQUI` não os diz.** O anfitrião furou
/// para o IPv6 de quem chama, e o pacote que entra por esse furo tem de ir ao
/// IPv6 dele. Sem repetir o que o `AQUI` já deu (um anfitrião registrado no
/// ponto por IPv6 diz o próprio).
fn destinos_da_segunda_chamada(
    modelo: &Candidato,
    atendida: &crate::encontro::Atendida,
    do_convite_v6: &[Candidato],
) -> Vec<Candidato> {
    let mut destinos = destinos_da_chamada(modelo, atendida);
    for candidato in do_convite_v6 {
        let repetido = destinos
            .iter()
            .any(|ja| mesmo_endereco(ja.destino.servidor, candidato.destino.servidor));
        if !repetido {
            destinos.push(candidato.clone());
        }
    }
    destinos
}

/// Se este endereço é IPv6 de verdade: o `::ffff:a.b.c.d` é IPv4 escrito de
/// outro jeito, e sai pelo caminho IPv4.
fn e_ipv6(servidor: SocketAddr) -> bool {
    servidor.ip().to_canonical().is_ipv6()
}

/// Os alvos da discagem que segue a segunda chamada na volta da bateria
/// interna: o endereço que o ponto deu, as dicas, e o próprio endereço da
/// sessão quando ele é IPv6 ([`e_ipv6`]).
///
/// A discagem direta a ele deu em silêncio, e o que mudou desde então é o
/// furo: o anfitrião mandou para o IPv6 desta máquina, e o firewall dele
/// passou a deixar entrar. Sem voltar a ele, o furo IPv6 abria e ninguém
/// passava.
fn alvos_da_segunda_volta(
    servidor: SocketAddr,
    atendida: &crate::encontro::Atendida,
) -> Vec<SocketAddr> {
    let mut alvos: Vec<SocketAddr> = std::iter::once(atendida.endereco)
        .chain(atendida.dicas.iter().copied())
        .collect();
    if e_ipv6(servidor) && !alvos.iter().any(|ja| mesmo_endereco(*ja, servidor)) {
        alvos.push(servidor);
    }
    alvos
}

/// Quanto um lote da corrida pode durar, quando corre com outros.
///
/// O degrau 4 soma o prazo da chamada ao da discagem. E o lote da chamada
/// pode fazê-lo duas vezes: a segunda chamada, com o IPv6, só sai depois de a
/// discagem da primeira dar em silêncio, e sem este dobro ela nasceria com o
/// prazo do lote já gasto. Quem não tem IPv6 global não a faz, e o lote acaba
/// na hora de sempre: isto é um teto, e não uma espera.
fn prazo_do_lote(pela_chamada: bool) -> Duration {
    let uma_volta = crate::encontro::PRAZO_DA_CHAMADA + PRAZO_DA_DISCAGEM_NA_CORRIDA;
    if pela_chamada {
        uma_volta * 2
    } else {
        uma_volta
    }
}

/// Se dois endereços são o mesmo, com o IPv4 mapeado igual ao escrito.
fn mesmo_endereco(a: SocketAddr, b: SocketAddr) -> bool {
    a.ip().to_canonical() == b.ip().to_canonical() && a.port() == b.port()
}

/// O prazo da discagem de um lote que corre com outros.
///
/// Quatro segundos, o número do ADR 0037 para um candidato. Com o ICE ele vale
/// para o lote inteiro: um endereço morto não gasta o prazo de ninguém, porque
/// os vivos respondem ao mesmo agente.
const PRAZO_DA_DISCAGEM_NA_CORRIDA: Duration = PRAZO_POR_CANDIDATO;

impl Enlace {
    /// Conecta no primeiro endereço que atender.
    ///
    /// Um convite pode trazer vários endereços do mesmo servidor — ADR 0006 — e
    /// eles não são intercambiáveis: o da rede de casa não é alcançável de
    /// fora, e o público que o roteador abriu costuma não voltar para dentro.
    /// Desde a 0.17 eles vão **juntos** num agente ICE só ([`Lote`]).
    ///
    /// # Que erro sai quando nenhum entra
    ///
    /// O de quem **respondeu**, se algum respondeu ([`FalhaAMostrar`]).
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar`], escolhido como acima.
    pub async fn conectar_entre(
        destinos: Vec<Destino>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        Self::conectar_entre_com_bilhete(destinos, None, chave, pins).await
    }

    /// O mesmo, com o degrau 4 do ponto de encontro do convite.
    ///
    /// O bilhete vem do `enc` do `seele://` — ADR 0006. Com ele, além da
    /// discagem direta, corre a chamada do ENC/3 ([`crate::encontro::Chamada`]):
    /// o ponto repassa o `CHAMA` ao anfitrião, o anfitrião fura o NAT dele para
    /// a porta desta sessão e responde com o endereço público, e a discagem vai
    /// para lá com o mesmo `U`.
    ///
    /// # O que acontece quando não dá para chamar
    ///
    /// Conecta como sempre conectou. Um ponto fora do ar, um convite sem
    /// impressão digital ou uma máquina sem rota fazem o degrau 4 não acontecer
    /// — e nenhum dos endereços do convite depende dele.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar_entre`].
    pub async fn conectar_entre_com_bilhete(
        destinos: Vec<Destino>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        Self::entre(destinos, bilhete, chave, pins, None, None).await
    }

    /// O mesmo, contando cada candidato a quem observa.
    ///
    /// A porta por onde [`crate::chegada::Chegada`] entra. Uma [`Tentativa`] sai
    /// por candidato, no instante em que o lote dele começa.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar_entre`].
    pub async fn conectar_entre_observado(
        destinos: Vec<Destino>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: mpsc::UnboundedSender<Andamento>,
    ) -> Result<Self, ConnectError> {
        Self::entre(destinos, bilhete, chave, pins, Some(olhos), None).await
    }

    /// O mesmo, com o quarto do ponto de encontro correndo **junto** com a
    /// corrida, e não antes dela (a pendência 55).
    ///
    /// - **Quem não precisa de furo não espera**: um endereço da rede de casa
    ///   começa na hora.
    /// - **Quem precisa de furo espera a resposta**, e entra num lote com o
    ///   endereço de hoje, quando ele é novo.
    ///
    /// Quem passa `quarto` responde por ele chegar: a corrida não acaba sem ele.
    /// Um remetente largado vale como um quarto que não trouxe nada.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar_entre`].
    pub(crate) async fn conectar_entre_com_o_quarto(
        destinos: Vec<Destino>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: mpsc::UnboundedSender<Andamento>,
        quarto: Option<tokio::sync::oneshot::Receiver<DoQuarto>>,
    ) -> Result<Self, ConnectError> {
        Self::entre(destinos, bilhete, chave, pins, Some(olhos), quarto).await
    }

    /// A preparação que os três compartilham, e o laço.
    async fn entre(
        destinos: Vec<Destino>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: Option<mpsc::UnboundedSender<Andamento>>,
        quarto: Option<tokio::sync::oneshot::Receiver<DoQuarto>>,
    ) -> Result<Self, ConnectError> {
        // Só resolve o ponto. **Nenhum pacote sai daqui.**
        let chamada = match &bilhete {
            Some(bilhete) => {
                let marcas = destinos.first().and_then(Destino::marcas);
                crate::encontro::Chamada::preparar(bilhete, marcas).await
            }
            None => None,
        };
        Self::tentar_entre(
            destinos,
            chamada,
            bilhete,
            chave,
            pins,
            olhos.as_ref(),
            quarto,
        )
        .await
    }

    /// A corrida entre os caminhos: o direto, o do degrau 4, e o do quarto.
    async fn tentar_entre(
        destinos: Vec<Destino>,
        chamada: Option<crate::encontro::Chamada>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: Option<&mpsc::UnboundedSender<Andamento>>,
        quarto: Option<tokio::sync::oneshot::Receiver<DoQuarto>>,
    ) -> Result<Self, ConnectError> {
        // **Uma ponta por tentativa de entrar** (R5): o socket desta sessão, de
        // onde saem a discagem e o `CHAMA`, e que só ela usa.
        let ponta = crate::transporte::abrir_ponta(None)?;
        if destinos.is_empty() {
            // Ninguém chama assim, e devolver um erro é melhor que entrar num
            // laço que termina sem resposta nenhuma.
            return Err(ConnectError::Unreachable);
        }
        let todos: Vec<Candidato> = destinos
            .into_iter()
            .map(|destino| Candidato {
                antes: pins.pinned(&destino.chave_do_pin),
                destino,
            })
            .collect();
        let alvo = todos.first().cloned();
        // Os IPv6 do anfitrião que o convite trouxe: servem à segunda chamada
        // do degrau 4.
        let do_convite_v6: Vec<Candidato> = todos
            .iter()
            .filter(|candidato| e_ipv6(candidato.destino.servidor))
            .cloned()
            .collect();

        // Com o quarto a caminho, quem precisa de furo espera a resposta dele, e
        // quem não precisa começa na hora.
        let o_alvo_espera = quarto.is_some()
            && alvo
                .as_ref()
                .is_some_and(|alvo| pede_furo(chamada.as_ref(), alvo.destino.servidor));
        let (ja, esperam): (Vec<Candidato>, Vec<Candidato>) =
            todos.into_iter().partition(|candidato| {
                quarto.is_none() || !pede_furo(chamada.as_ref(), candidato.destino.servidor)
            });
        let mut fila: Vec<Lote> = Vec::new();
        if !ja.is_empty() {
            fila.push(Lote::Direto(ja));
        }
        // O degrau 4 entra **depois** do direto, na defasagem: quem está na
        // mesma casa entra antes de o ponto ou o anfitrião ouvirem qualquer
        // coisa, e o `CHAMA` só sai quando o direto não respondeu logo.
        if let (Some(chamada), Some(modelo)) = (chamada.clone(), alvo.clone()) {
            fila.push(Lote::PelaChamada {
                chamada,
                modelo: Box::new(modelo),
                do_convite_v6,
            });
        }
        let corrida_de_um = fila.len() == 1 && quarto.is_none();
        let tardios = quarto.map(|quarto| {
            let pins = Arc::clone(&pins);
            async move {
                // Um remetente largado é um quarto que não trouxe nada.
                let do_quarto = quarto.await.unwrap_or_default();
                lote_do_quarto(do_quarto, esperam, o_alvo_espera, pins.as_ref())
            }
        });

        // A numeração dos candidatos para a trilha, na ordem em que cada um
        // começou, através dos lotes.
        let numeracao = Arc::new(std::sync::atomic::AtomicU8::new(0));
        // Quem ouvir `Tocando` avisa a corrida por aqui (tarefa 16 da F7).
        let (para_a_corrida, tocados) = mpsc::unbounded_channel();
        let corrida = correr(
            fila,
            DEFASAGEM_ENTRE_CANDIDATOS,
            tardios,
            Some(tocados),
            |posicao, lote: Lote| {
                let pela_chamada = matches!(lote, Lote::PelaChamada { .. });
                let chave = chave.clone();
                let pins = Arc::clone(&pins);
                let ponta = ponta.clone();
                let olhos = olhos.cloned();
                let para_a_corrida = para_a_corrida.clone();
                let bilhete = bilhete.clone();
                let numeracao = Arc::clone(&numeracao);
                async move {
                    let (tocou, mut ouviu) = mpsc::unbounded_channel();
                    let tentativa = Self::tentar_o_lote(
                        ponta,
                        lote,
                        bilhete,
                        chave,
                        pins,
                        olhos.clone(),
                        numeracao,
                        tocou,
                        if corrida_de_um {
                            seele_proto::transport::HANDSHAKE_TIMEOUT
                        } else {
                            PRAZO_DA_DISCAGEM_NA_CORRIDA
                        },
                    );
                    // Um lote sozinho não tem prazo além do da discagem e do
                    // aperto: é o caminho de antes para um link de um endereço.
                    // O degrau 4 soma o prazo da chamada ao da discagem, e pode
                    // fazê-lo duas vezes ([`prazo_do_lote`]).
                    let prazo = (!corrida_de_um).then(|| prazo_do_lote(pela_chamada));
                    match ate_o_prazo_ou_o_toque(
                        prazo,
                        tentativa,
                        &mut ouviu,
                        |(candidato, prazo_s)| {
                            let _ = para_a_corrida.send(posicao);
                            contar(olhos.as_ref(), Andamento::Tocando { candidato, prazo_s });
                        },
                    )
                    .await
                    {
                        Some(resultado) => resultado,
                        // `SemResposta` e não `HandshakeTimeout`: este prazo é o
                        // do lote inteiro, e queimá-lo é não ter recebido nada.
                        None => Err(ConnectError::SemResposta),
                    }
                }
            },
        )
        .await;
        let Corrida {
            vencedor,
            falhas,
            iniciados,
        } = corrida;

        // A limpeza de pin órfão dos perdedores, **depois** do vencedor e
        // pulando a chave dele (ADR 0037).
        let chave_do_vencedor = vencedor
            .as_ref()
            .map(|(_, (_, chave_do_pin))| chave_do_pin.clone());
        for (posicao, falha) in &falhas {
            let Some(perdido) = iniciados.get(*posicao) else {
                continue;
            };
            // Um degrau 4 que nem achou o anfitrião não discou ninguém, e não
            // escreveu pin nenhum: limpar por ele apagaria o pin de um vizinho
            // com o motivo errado no log.
            if matches!(falha, ConnectError::NaoLocalizou(_)) {
                continue;
            }
            for candidato in perdido.candidatos() {
                let chave_perdida = &candidato.destino.chave_do_pin;
                if chave_do_vencedor.as_deref() == Some(chave_perdida.as_str()) {
                    continue;
                }
                desfazer_pin_orfao(
                    pins.as_ref(),
                    chave_perdida,
                    candidato.antes.as_deref(),
                    falha,
                );
            }
        }

        let onde_de = |posicao: usize| iniciados.get(posicao).and_then(Lote::onde);
        if let Some((posicao, (enlace, _))) = vencedor {
            let onde = onde_de(posicao);
            for (perdedor, falha) in &falhas {
                registrar_a_falha_do_candidato(onde_de(*perdedor), falha, onde);
            }
            tracing::info!(?onde, "este é o caminho que deu");
            // Só o vencedor declara, e só agora que se sabe quem venceu.
            let _ = enlace.declarar_identidade_de_par().await;
            return Ok(enlace);
        }

        let mut a_mostrar = FalhaAMostrar::default();
        for (posicao, falha) in falhas {
            registrar_a_falha_do_candidato(onde_de(posicao), &falha, None);
            a_mostrar.anotar(falha);
        }
        Err(a_mostrar.escolhida())
    }

    /// Um lote da corrida: a chamada pelo ponto, quando é o degrau 4, e a
    /// discagem dos endereços dele num agente ICE.
    ///
    /// Devolve o enlace e a chave de pino de quem venceu, que é o que a limpeza
    /// dos perdedores pula.
    #[expect(
        clippy::too_many_arguments,
        reason = "o que cada lote leva para dentro da tarefa dele: a ponta, o lote, o \
                  bilhete, a chave, os pins, quem observa, a numeração, o toque e o prazo"
    )]
    async fn tentar_o_lote(
        ponta: crate::transporte::PontaDaSessao,
        lote: Lote,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        olhos: Option<mpsc::UnboundedSender<Andamento>>,
        numeracao: Arc<std::sync::atomic::AtomicU8>,
        tocou: mpsc::UnboundedSender<(u8, u16)>,
        prazo_da_discagem: Duration,
    ) -> Result<(Self, String), ConnectError> {
        let (candidatos, pelo_chama, segunda) = match lote {
            Lote::Direto(candidatos) => (candidatos, None, None),
            Lote::PelaChamada {
                chamada,
                modelo,
                do_convite_v6,
            } => {
                let atendida = chamada
                    .chamar(ponta.ponta(), crate::encontro::PRAZO_DA_CHAMADA)
                    .await
                    .map_err(falha_da_chamada)?;
                (
                    destinos_da_chamada(&modelo, &atendida),
                    Some(PeloChama {
                        credencial: atendida.credencial,
                        espera_checagem_ipv6: false,
                    }),
                    Some((chamada, modelo, do_convite_v6)),
                )
            }
        };
        let pelo_ponto = pelo_chama.is_some();
        let numerar = |candidatos: Vec<Candidato>| -> Vec<(Candidato, u8)> {
            candidatos
                .into_iter()
                .map(|candidato| {
                    let numero = numeracao.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    contar(
                        olhos.as_ref(),
                        Andamento::Tentando(Tentativa {
                            candidato: numero,
                            onde: candidato.destino.servidor,
                            avisou: pelo_ponto,
                        }),
                    );
                    (candidato, numero)
                })
                .collect()
        };
        let primeira = Self::discar_o_lote(
            &ponta,
            numerar(candidatos),
            pelo_chama,
            bilhete.clone(),
            chave.clone(),
            Arc::clone(&pins),
            tocou.clone(),
            prazo_da_discagem,
        )
        .await;
        let Some((chamada, modelo, do_convite_v6)) = segunda else {
            return primeira;
        };
        match primeira {
            // **O ponto disse que o anfitrião atendeu, e o caminho não abriu.**
            // É a falha que o degrau 4 tem de nomear: dois NATs que não deixam
            // entrar o que não saiu, sem relay. Antes de nomeá-la, o que resta.
            Err(ConnectError::SemResposta) => {}
            outro => return outro,
        }
        // **A segunda chamada, com o IPv6 desta máquina.** O IPv4 não abriu, e
        // o anfitrião só furou o endereço que o ponto viu. Com o IPv6 de quem
        // chama no `CHAMA`, ele fura também o firewall IPv6 do roteador dele, e
        // a discagem volta com o que o segundo `AQUI` der e com os IPv6 dele
        // que o convite já trazia. Esta discagem aceita a checagem IPv6 do
        // anfitrião mesmo sem conhecer IPv6 nenhum dele (quem liga por
        // código): a primeira acabou, e não há outra com este `U`.
        let enderecos_v6: Vec<SocketAddr> = do_convite_v6
            .iter()
            .map(|candidato| candidato.destino.servidor)
            .collect();
        let Some(Ok(atendida)) = chamada
            .chamar_pelo_ipv6(
                ponta.ponta(),
                crate::encontro::PRAZO_DA_CHAMADA,
                &enderecos_v6,
            )
            .await
        else {
            // Sem IPv6 global para dizer, ou o ponto não atendeu desta vez: é
            // a falha de antes, com o nome de antes.
            return Err(ConnectError::NatNaoAbriu);
        };
        Self::discar_o_lote(
            &ponta,
            numerar(destinos_da_segunda_chamada(
                &modelo,
                &atendida,
                &do_convite_v6,
            )),
            Some(PeloChama {
                credencial: atendida.credencial,
                espera_checagem_ipv6: true,
            }),
            bilhete,
            chave,
            pins,
            tocou,
            prazo_da_discagem,
        )
        .await
        .map_err(|erro| match erro {
            ConnectError::SemResposta => ConnectError::NatNaoAbriu,
            outro => outro,
        })
    }

    /// Disca os endereços de um lote num agente ICE e faz o aperto do
    /// protocolo com quem respondeu.
    ///
    /// **Um impostor na lista não derruba o lote.** Se quem respondeu não é o
    /// servidor que a impressão promete (ou a chave fixada mudou), a conexão
    /// fecha antes do `Hello`, a recusa vai ao log, e o lote disca de novo com
    /// os endereços que sobram. A recusa só é devolvida se ninguém mais entrar.
    #[expect(
        clippy::too_many_arguments,
        reason = "os de `tentar_o_lote`, menos o que já foi resolvido lá"
    )]
    async fn discar_o_lote(
        ponta: &crate::transporte::PontaDaSessao,
        mut restantes: Vec<(Candidato, u8)>,
        pelo_chama: Option<PeloChama>,
        bilhete: Option<seele_proto::uri::Bilhete>,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        tocou: mpsc::UnboundedSender<(u8, u16)>,
        prazo: Duration,
    ) -> Result<(Self, String), ConnectError> {
        let mut recusa: Option<ConnectError> = None;
        while !restantes.is_empty() {
            let alvos: Vec<SocketAddr> = restantes
                .iter()
                .map(|(candidato, _)| candidato.destino.servidor)
                .collect();
            let conexao = match crate::client::discar(
                ponta.ponta(),
                alvos,
                pelo_chama.clone(),
                prazo,
            )
            .await
            {
                Ok(conexao) => conexao,
                // Uma recusa de antes diz mais que o silêncio de agora.
                Err(erro) => return Err(recusa.unwrap_or(erro)),
            };
            let remoto = conexao.remote_address();
            let indice = restantes
                .iter()
                .position(|(candidato, _)| mesmo_endereco(candidato.destino.servidor, remoto))
                .unwrap_or(0);
            let (candidato, numero) = restantes.remove(indice);
            // O endereço como o convite o escreveu, e não na forma que o socket
            // de pilha dupla devolve (`::ffff:a.b.c.d`): é o que quem lê o log
            // procura.
            let onde = candidato.destino.servidor;
            // O toque deste candidato vai à corrida com o número dele.
            let (tocou_aqui, mut ouviu_aqui) = mpsc::unbounded_channel();
            let repasse = {
                let tocou = tocou.clone();
                tokio::spawn(async move {
                    while let Some(prazo_s) = ouviu_aqui.recv().await {
                        let _ = tocou.send((numero, prazo_s));
                    }
                })
            };
            let chave_do_pin = candidato.destino.chave_do_pin.clone();
            let resultado = Self::conectar_sobre(
                ponta.clone(),
                conexao,
                bilhete.clone(),
                candidato.destino,
                chave.clone(),
                Arc::clone(&pins),
                Some(tocou_aqui),
            )
            .await;
            repasse.abort();
            match resultado {
                Ok(enlace) => return Ok((enlace, chave_do_pin)),
                Err(
                    erro @ (ConnectError::InviteMismatch { .. }
                    | ConnectError::PinChanged { .. }
                    | ConnectError::CodigoNaoConfere(_)),
                ) if !restantes.is_empty() => {
                    registrar_a_falha_do_candidato(Some(onde), &erro, None);
                    recusa = Some(erro);
                }
                Err(erro) => return Err(erro),
            }
        }
        Err(recusa.unwrap_or(ConnectError::Unreachable))
    }

    /// Conecta pela primeira vez, a um endereço só.
    ///
    /// A primeira conexão falha para fora: quem não conseguiu entrar não tem
    /// sessão para segurar.
    ///
    /// # Errors
    ///
    /// Devolve o motivo de não ter conseguido conectar, incluindo
    /// [`ConnectError::InviteMismatch`] quando o link prometia outra
    /// identidade.
    pub async fn conectar(
        destino: Destino,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Self, ConnectError> {
        let ponta = crate::transporte::abrir_ponta(None)?;
        let conexao = crate::client::discar(
            ponta.ponta(),
            vec![destino.servidor],
            None,
            seele_proto::transport::HANDSHAKE_TIMEOUT,
        )
        .await?;
        let enlace = Self::conectar_sobre(ponta, conexao, None, destino, chave, pins, None).await?;
        let _ = enlace.declarar_identidade_de_par().await;
        Ok(enlace)
    }

    /// O aperto do protocolo sobre uma conexão que o enlace já abriu, e o motor.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Enlace::conectar`].
    async fn conectar_sobre(
        ponta: crate::transporte::PontaDaSessao,
        conexao: crate::transporte::Conexao,
        bilhete: Option<seele_proto::uri::Bilhete>,
        destino: Destino,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
        tocou: Option<mpsc::UnboundedSender<u16>>,
    ) -> Result<Self, ConnectError> {
        // O verificador escreve o pin antes do `Hello`, e os vizinhos desta
        // tentativa escrevem na mesma chave quando são alternativos do mesmo
        // nome de convite. Por isso o que este aperto escreveu é **anotado**, e
        // não deduzido de olhar a loja antes e depois. Ver
        // [`desfazer_o_pin_deste_aperto`].
        let deste_aperto = Arc::new(PinsDesteAperto::nova(Arc::clone(&pins)));

        let resultado = Client::connect_sobre(
            ponta,
            conexao,
            &destino.chave_do_pin,
            // Conferida **antes do `Hello`**: um primeiro contato que não
            // confere nem chega a mandar o convite.
            destino.esperada.as_ref(),
            destino.o_pino_prova_o_servidor,
            &destino.apelido,
            &chave,
            Arc::clone(&deste_aperto) as Arc<dyn PinStore>,
            destino.segredo.as_deref(),
            destino.aceito.as_deref(),
            tocou,
        )
        .await;

        let mut cliente = match resultado {
            Ok(cliente) => cliente,
            Err(erro) => {
                // Só desfaz o que este aperto escreveu. A recusa pela impressão
                // (`InviteMismatch`), a chave trocada (`PinChanged`) e qualquer
                // falha que veio antes de um primeiro contato aceito não
                // escreveram nada, e um pin que exista agora é de antes deste
                // aperto (o de um candidato fixado cujo pino não prova o
                // servidor) ou de um vizinho.
                desfazer_o_pin_deste_aperto(
                    pins.as_ref(),
                    &destino.chave_do_pin,
                    deste_aperto.o_que_escreveu().as_deref(),
                    &erro,
                );
                return Err(erro);
            }
        };

        let pin = cliente.pin_decision().clone();
        let veredito = match conferir(&destino, &pin, pins.as_ref()) {
            Ok(veredito) => veredito,
            Err(erro) => {
                // **Segunda linha.** Este braço já não roda para a impressão
                // que o convite desmente: o verificador a recusa dentro do TLS,
                // antes do `Hello`, e a falha sobe como `InviteMismatch` pelo
                // braço de cima. Só chegaria aqui um servidor que o verificador
                // deixasse passar e o `verdict` recusasse, e os dois leem a
                // mesma comparação (`tofu::confere`). Fica porque uma defesa que
                // some quando a primeira funciona é uma defesa que ninguém
                // verificou que ainda existe.
                //
                // Derrubar, não só relatar. E explicitamente, não por `Drop`.
                //
                // Soltar o `Client` **acaba** fechando a conexão — medido em
                // ~85 ms contra um servidor de verdade, com e sem esta linha,
                // no tempo em que este caminho era o único —, mas pelo caminho
                // longo: `Client::connect` deixa uma tarefa de leitura dona do
                // `RecvStream`, e ela só descobre que ninguém escuta quando o
                // servidor manda o quadro seguinte. Contra um servidor que fala
                // (telemetria a cada segundo) isso é rápido; contra um que
                // emudeceu, é o tempo ocioso do QUIC inteiro, com uma sessão de
                // pé do lado de quem acabou de ser recusado.
                //
                // Fechar aqui não depende de o servidor dizer nada. Nenhum teste
                // alcança esta linha: com o verificador recusando no TLS, não há
                // como um servidor de verdade fazê-la rodar, e apagá-la não deixa
                // nada vermelho — o que valia dizer já valia antes, em
                // `crates/seele-conformance/tests/convite.rs`.
                //
                // E fecha **dizendo o que foi**: o motivo viaja no
                // `CONNECTION_CLOSE` e é o que fica no log do servidor. Fechar
                // como `ejected` faria uma recusa de convite parecer uma pessoa
                // saindo, que é o único jeito de esconder a recusa de quem tem
                // o log na mão.
                cliente.close(crate::client::INVITE_REFUSED);
                return Err(erro);
            }
        };

        let sessao = cliente.session().clone();
        let media = cliente.media();
        let rtt = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let caminho_medido = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let protocolo = Arc::new(std::sync::atomic::AtomicU8::new(cliente.versao_negociada()));

        let (comandos_tx, comandos_rx) = mpsc::channel(COMANDOS);
        let (avisos_tx, avisos_rx) = mpsc::unbounded_channel();
        let (resultados_do_par_tx, resultados_do_par) = mpsc::unbounded_channel();

        let bateria = bateria_de(&destino);
        let tuneis = crate::tunel::Tuneis::nova(avisos_tx.clone());
        let maquina = tuneis.estado_da_maquina();
        let lado_da_maquina = LadoDaMaquina::default();
        let com_tela = Arc::clone(&lado_da_maquina.com_tela);
        let motor = Motor {
            destino,
            // Guardado para a reconexão, e não só para a primeira entrada: uma
            // reconexão sai de um socket novo, com uma porta nova, e o caminho
            // que o anfitrião furou era para a porta velha. Sem bater de novo, a
            // bateria de cinco minutos contaria até o fim contra uma porta
            // fechada.
            bilhete,
            chave,
            pins,
            cliente: Some(cliente),
            bateria,
            inicio: Instant::now(),
            voice_room: None,
            entradas: crate::assento::EntradasPendentes::default(),
            queda: None,
            linha: None,
            muted: false,
            isolamento: false,
            avisos: avisos_tx,
            rtt: Arc::clone(&rtt),
            caminho_medido: Arc::clone(&caminho_medido),
            protocolo: Arc::clone(&protocolo),
            tela_pedida: None,
            tela_viva: None,
            faixa: FAIXA_INICIAL,
            caminho_de_quem_hospeda_bps: None,
            espectadores: 0,
            caminho: crate::caminho::Sonda::nova(),
            estimativa_da_midia: None,
            ultima_chave_pela_midia: None,
            chave_devida: false,
            ultima_mudanca_da_midia: None,
            atendendo_pares: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            consentimento: ConsentimentoDePar::de_ninguem(),
            repasse: Arc::new(RepasseDeTela::default()),
            resultados_do_par,
            resultados_do_par_tx,
            tarefas_de_par: TarefasDePar::default(),
            tuneis,
            lado_da_maquina,
        };
        // **Não declara identidade aqui.** Achado do fix round 2:
        // `conectar_por` é o funil que **cada candidato** de uma conexão com
        // múltiplos destinos atravessa (`tentar_entre`), não só o vencedor da
        // corrida. Declarar deste lado faria todo candidato que conectasse
        // mandar `EmprestarSubida`, e o candidato perdedor — cuja conexão é
        // fechada logo depois — apagaria ao sair a declaração que o vencedor
        // acabara de fazer: `Pares` é chaveado por pessoa, e os dois
        // candidatos são a mesma pessoa. Só o vencedor declara, e só depois de
        // decidido quem venceu — ver `Enlace::declarar_identidade_de_par` e
        // onde ela é chamada em `conectar`/`tentar_entre`.
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
            caminho_medido,
            protocolo,
            maquina,
            com_tela,
            tarefa: Some(tarefa),
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
            Aviso::Reconectado { media, sessao, .. } => {
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

    /// Semeia a sonda com o caminho medido da última vez com este servidor.
    ///
    /// Sem isto, toda transmissão parte de
    /// [`crate::tela::CAMINHO_DA_PROVA_BPS`] — 2 Mbps supostos — e gasta os
    /// primeiros segundos reaprendendo um cano que já mediu. Medido em campo
    /// numa LAN: doze segundos de 540p até a escada reencontrar 1080p.
    ///
    /// Zero é «não sei», e não faz nada: é o que a lista de conhecidos guarda
    /// para um servidor onde ninguém compartilhou tela ainda.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn lembrar_o_caminho(&self, bps: u32) -> Result<(), Fechado> {
        if bps == 0 {
            return Ok(());
        }
        self.mandar(Comando::LembrarCaminho(bps)).await
    }

    /// O caminho de subida que a sonda mediu, em bits por segundo.
    ///
    /// Zero enquanto ninguém compartilhou tela: a sonda mede **enquanto a tela
    /// transmite**, porque é a tela que enche o cano. Quem grava a lista de
    /// conhecidos deve tratar zero como «não mediu» e deixar o valor antigo.
    #[must_use]
    pub fn caminho_medido(&self) -> u32 {
        self.caminho_medido
            .load(std::sync::atomic::Ordering::Relaxed)
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

    /// Entra numa sala de voz. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn entrar_na_voice_room(
        &self,
        voice_room: VoiceRoomId,
        password: Option<String>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::EntrarNaVoiceRoom(voice_room, password))
            .await
    }

    /// Sai da sala de voz.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn sair_da_voice_room(&self) -> Result<(), Fechado> {
        self.mandar(Comando::SairDaVoiceRoom).await
    }

    /// Abre uma Linha. Restaurada depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn abrir_linha(&self, linha: ChannelId) -> Result<(), Fechado> {
        self.mandar(Comando::AbrirLinha(linha)).await
    }

    /// Encaminha uma transferência sem ocupar o laço de controle.
    pub async fn imagem_de_mod(
        &self,
        pedido: seele_proto::volume::PedidoDeImagem,
        bytes: Vec<u8>,
        resposta: tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::ImagemDeMod {
            pedido,
            bytes,
            resposta,
        })
        .await
    }

    /// Queues a MOD request; fails if the connection task is closed.
    pub async fn mod_request(
        &self,
        request: u32,
        id: String,
        channel: ChannelId,
        payload: String,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::ModRequest {
            request,
            id,
            channel,
            payload,
        })
        .await
    }

    /// Enfileira uma ação declarada de MOD (API 7). Quem autoriza é o servidor;
    /// a resposta volta no `ModReply` deste `request`.
    ///
    /// # Errors
    ///
    /// Falha se a tarefa da conexão já tiver fechado.
    pub async fn acao_de_mod(
        &self,
        request: u32,
        id: String,
        channel: ChannelId,
        acao: String,
        payload: String,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::AcaoDeMod {
            request,
            id,
            channel,
            acao,
            payload,
        })
        .await
    }

    /// Diz alguma coisa.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    ///
    /// `responde_a` é a mensagem a que esta responde, ou `None`.
    pub async fn dizer(
        &self,
        linha: ChannelId,
        corpo: String,
        responde_a: Option<MessageId>,
        id: ClientMessageId,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::Dizer {
            linha,
            corpo,
            responde_a,
            id,
        })
        .await
    }

    /// Pede histórico.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn historico(&self, linha: ChannelId, limite: u16) -> Result<(), Fechado> {
        self.mandar(Comando::Historico { linha, limite }).await
    }

    /// Pede uma página de histórico **atrás de `cursor`**, e logo atrás um
    /// `Ping` com `carimbo`, que serve de cerca: o `Pong` dele chega depois da
    /// página inteira (ver [`Client::send_ping_marcado`]).
    ///
    /// É o que deixa quem chama saber quando uma página acabou, e pedir a
    /// seguinte. O fio só pagina para trás, e as mensagens da página chegam
    /// como `MessageReceived`, sem marca nenhuma de começo ou fim. Lote P61: a
    /// FFI fecha assim o buraco que a queda deixou no canal aberto.
    ///
    /// Não é lembrado para a volta: a página é de uma conexão, e a busca
    /// recomeça na conexão seguinte a partir do que já chegou.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn historico_com_cerca(
        &self,
        linha: ChannelId,
        cursor: Option<MessageId>,
        limite: u16,
        carimbo: u64,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::HistoricoComCerca {
            linha,
            cursor,
            limite,
            carimbo,
        })
        .await
    }

    /// Liga ou desliga o mudo. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn muted(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Muted(ligado)).await
    }

    /// Liga ou desliga o isolamento total. Restaurado depois de uma reconexão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn isolamento(&self, ligado: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Isolamento(ligado)).await
    }

    /// Pede ao servidor que faça uma sala de voz.
    ///
    /// Pede, e só. Nada aqui confere se esta pessoa pode: a `specs/08-seguranca.md`
    /// põe a decisão no servidor, e um core que recusasse por conta própria
    /// seria uma segunda autoridade para manter de acordo com a primeira. A
    /// resposta chega como aviso — `VoiceRoomCreated` se aconteceu, `Alert` com
    /// `PermissionDenied` se não.
    ///
    /// **Não** é refeito ao reconectar, ao contrário da sala de voz e da Linha
    /// abertos. Aqueles são onde a pessoa estava, e voltar sem eles é voltar
    /// para outro lugar; este é uma coisa que se faz uma vez. Repetido depois de
    /// uma queda, ele criaria uma sala minutos mais tarde, do nada, e mais uma
    /// se a pessoa já tivesse pedido de novo à mão.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn criar_voice_room(
        &self,
        nome: String,
        limite: u16,
        linha: Option<ChannelId>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::CriarVoiceRoom {
            nome,
            limite,
            linha,
        })
        .await
    }

    /// Pede ao servidor que faça uma Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn criar_linha(&self, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::CriarLinha { nome }).await
    }

    /// Pede ao servidor que renomeie uma sala de voz.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_voice_room(
        &self,
        voice_room: VoiceRoomId,
        nome: String,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearVoiceRoom { voice_room, nome })
            .await
    }

    /// Pede ao servidor que renomeie uma Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_linha(&self, linha: ChannelId, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearLinha { linha, nome }).await
    }

    /// Pede ao servidor que troque o próprio nome.
    ///
    /// Pede, e só, como os verbos de sala e pelo mesmo motivo: quem decide é o
    /// servidor, que quer `AdministerServer` para isto e responde `Alert` com
    /// `PermissionDenied` quando nega. Quando aceita, o nome novo volta para
    /// **todo mundo** como `ServerRenamed`, inclusive para quem pediu — é o que
    /// impede a tela de quem renomeou de ser a única com o nome certo.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e de moderação:
    /// dar nome é coisa que se faz uma vez, e repetido depois de cinco minutos
    /// de bateria desfaria o nome que outra pessoa pôs no meio.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn renomear_server(&self, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::RenomearServer { nome }).await
    }

    /// Pede ao servidor que troque a própria imagem, ou que fique sem nenhuma.
    ///
    /// `None` tira a imagem, e é um verbo e não uma ausência: quem pôs tem que
    /// poder tirar.
    ///
    /// **Não** é refeito ao reconectar, pelo mesmo motivo do nome.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn definir_icone(&self, icone: Option<Vec<u8>>) -> Result<(), Fechado> {
        self.mandar(Comando::IconeDoServer { icone }).await
    }

    /// Põe ou tira **a sua** imagem de perfil.
    ///
    /// Diferente do ícone do servidor num ponto: aquele exige permissão, este
    /// não. O servidor grava na linha de quem pediu e em nenhuma outra, e uma
    /// permissão aqui seria alguém podendo escolher a cara dos outros.
    ///
    /// **Não** é refeita ao reconectar, pelo mesmo motivo do nome e do ícone:
    /// ela está gravada no servidor, e reenviá-la a cada volta seria escrever
    /// de novo o que já está lá.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn definir_minha_imagem(&self, icone: Option<Vec<u8>>) -> Result<(), Fechado> {
        self.mandar(Comando::MinhaImagem { icone }).await
    }

    /// Grava **o seu** perfil público (0.19, protocolo 12).
    ///
    /// Recebe a mensagem já montada e validada (`perfil::montar_gravar`), e
    /// **não** é refeita ao reconectar, pelo mesmo motivo do nome e da imagem:
    /// está gravada no servidor, e reenviá-la a cada volta escreveria de novo o
    /// que já está lá — e, com a revisão velha, seria uma recusa a cada volta.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn gravar_perfil(
        &self,
        mensagem: seele_proto::control::ClientMessage,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::GravarPerfil(mensagem)).await
    }

    /// Tira **a sua** foto ou **a sua** capa (0.19, protocolo 12).
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn tirar_imagem_do_perfil(
        &self,
        lado: seele_proto::perfil::LadoDoPerfil,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::TirarImagemDoPerfil { lado }).await
    }

    /// O número de protocolo que a conexão **de agora** negociou. Sobe e desce
    /// com a volta do enlace: um servidor que reiniciou numa versão menor muda
    /// o que pode ser dito.
    #[must_use]
    pub fn versao_negociada(&self) -> u8 {
        self.protocolo.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Troca **o seu** apelido.
    ///
    /// **Não** é refeito ao reconectar: o nome fica gravado no servidor, e a
    /// reconexão volta a apresentar quem já se é. Reenviá-lo seria escrever de
    /// novo o que já está lá — e, num servidor onde outra pessoa tenha tomado
    /// o nome nesse meio-tempo, seria uma recusa a cada volta.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn definir_meu_apelido(&self, nome: String) -> Result<(), Fechado> {
        self.mandar(Comando::MeuApelido { nome }).await
    }

    /// Pede ao servidor que acabe com a sessão de alguém.
    ///
    /// Pede, e só — como os verbos de sala, e pela mesma razão: a
    /// `specs/08-seguranca.md` põe a decisão no servidor, e um core que
    /// recusasse por conta própria seria uma segunda autoridade para manter de
    /// acordo com a primeira. Esconder o botão é conveniência; quem nega é o
    /// servidor, e ele responde com `Alert` de `PermissionDenied` quando nega.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e pelo mesmo
    /// motivo: expulsar é coisa que se faz uma vez, e repetida minutos depois
    /// derrubaria alguém que já tinha voltado.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn expulsar(&self, pessoa: PersonId) -> Result<(), Fechado> {
        self.mandar(Comando::Expulsar { pessoa }).await
    }

    /// Pede um verbo de cargo com a revisão que a tela viu, ou, com `None`, o
    /// quadro de cargos (ADR 0064). **Não volta na reconexão**, como moderar:
    /// refeito minutos depois, ele valeria contra uma revisão que já não é a
    /// vista — e o servidor o recusaria com `RevisaoVelha` de qualquer jeito.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn cargo(
        &self,
        pedido: Option<(u64, seele_proto::cargos::PedidoDeCargo)>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::Cargo(pedido)).await
    }

    /// Pede ao servidor que impeça alguém de voltar.
    ///
    /// `expira_em` em segundos desde a época; `None` é para sempre. O `motivo`
    /// é para o registro de quem hospeda e nunca chega a quem foi banido — a
    /// `specs/08-seguranca.md` quer falha uniforme, e a recusa que essa pessoa
    /// encontra na volta é a mesma qualquer que seja o texto.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn banir(
        &self,
        pessoa: PersonId,
        motivo: Option<String>,
        expira_em: Option<i64>,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::Banir {
            pessoa,
            motivo,
            expira_em,
        })
        .await
    }

    /// Pede ao servidor que tire uma mensagem da Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn remover_mensagem(&self, mensagem: MessageId) -> Result<(), Fechado> {
        self.mandar(Comando::RemoverMensagem { mensagem }).await
    }

    /// Pede ao servidor que reescreva o corpo de uma mensagem própria
    /// (protocolo 13).
    ///
    /// **O portão de versão é de quem chama** ([`seele_sessao::perfil::portao_do_cliente`],
    /// com o número negociado): a fila não devolve o motivo de um comando que
    /// não saiu. O motor confere de novo, e um servidor que negociou menos
    /// não recebe o verbo — só o registro fica sabendo.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn editar_mensagem(&self, mensagem: MessageId, corpo: String) -> Result<(), Fechado> {
        self.mandar(Comando::EditarMensagem { mensagem, corpo })
            .await
    }

    /// Pede ao servidor que tire todo banimento de uma conta (protocolo 13).
    /// O portão de versão é de quem chama, como em [`Self::editar_mensagem`].
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn desbanir(&self, pessoa: PersonId) -> Result<(), Fechado> {
        self.mandar(Comando::Desbanir { pessoa }).await
    }

    /// Pede a lista de banidos (protocolo 13). O portão de versão é de quem
    /// chama, como em [`Self::editar_mensagem`].
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn listar_banidos(&self) -> Result<(), Fechado> {
        self.mandar(Comando::ListarBanidos).await
    }

    /// Relata ao servidor o que esta ponta mede da voz que ouve (protocolo
    /// 13): a perda e o jitter que só existem no receptor, e que entram no
    /// sinal que os outros veem **desta** pessoa.
    ///
    /// Devolve se o relato foi para a fila. **`false` não é erro**: o servidor
    /// de agora negociou menos que o 13 ([`o_servidor_ouve_o_relato`]) e não
    /// conhece a mensagem — nada sai, e o sinal desta pessoa segue, lá, como
    /// sempre foi. O portão é conferido aqui, a cada relato, porque o número
    /// negociado muda com a volta do enlace; o motor confere de novo
    /// (`Client::relatar_recepcao`), e essa segunda cerca vai ao registro —
    /// uma vez por segundo, se a primeira não existisse.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn relatar_recepcao(&self, relato: Recepcao) -> Result<bool, Fechado> {
        if !o_servidor_ouve_o_relato(self.versao_negociada()) {
            return Ok(false);
        }
        self.mandar(Comando::RelatarRecepcao(relato)).await?;
        Ok(true)
    }

    /// Pede ao servidor que mova alguém para uma sala de voz.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn mover_pessoa(
        &self,
        pessoa: PersonId,
        voice_room: VoiceRoomId,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::MoverPersono { pessoa, voice_room })
            .await
    }

    /// Pede ao servidor que destrua uma sala de voz.
    ///
    /// Pede, e só, como todo verbo daqui. Quem recusa é o servidor: sem
    /// `administrar_server` volta `Alert` com `PermissionDenied`, e no único
    /// sala de voz que resta volta `Alert` com `LastVoiceRoom`, que é frase diferente.
    ///
    /// **Não** é refeito ao reconectar, como os verbos de sala e de moderação e
    /// pelo mesmo motivo, com uma ponta a mais: repetido minutos depois, este
    /// destruiria a sala que alguém fez no lugar da que sumiu.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn apagar_voice_room(&self, voice_room: VoiceRoomId) -> Result<(), Fechado> {
        self.mandar(Comando::ApagarVoiceRoom { voice_room }).await
    }

    /// Pede ao servidor que destrua uma Linha, e tudo que foi escrito nela.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn apagar_linha(&self, linha: ChannelId) -> Result<(), Fechado> {
        self.mandar(Comando::ApagarLinha { linha }).await
    }

    /// Pergunta quanto custaria destruir uma Linha. Não destrói nada.
    ///
    /// A resposta chega como `ChannelWeighed` no fluxo de avisos, como toda
    /// resposta deste enlace. É o que enche a caixa de confirmação com número
    /// contado no banco — uma casca segura uma página de histórico e chutaria
    /// para baixo por todo o passado da Linha.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn pesar_linha(&self, linha: ChannelId) -> Result<(), Fechado> {
        self.mandar(Comando::PesarLinha { linha }).await
    }

    /// Manda um arquivo, num fluxo só dele.
    ///
    /// Volta assim que a transferência foi enfileirada, e não quando ela
    /// terminou: o andamento chega por [`Aviso::Transferencia`], e a mensagem
    /// só aparece na Linha depois de os bytes chegarem inteiros. É por isso que
    /// enquanto sobe só quem enviou a vê.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn anexar(&self, anexo: Anexo) -> Result<(), Fechado> {
        self.mandar(Comando::Anexar(Box::new(anexo))).await
    }

    /// Pede um anexo e grava em `pasta`, com o nome que veio com ele.
    ///
    /// **A regra de onde e com que nome mora em [`crate::anexo_no_disco`]**, e
    /// não em quem chama: o motor abre o parcial com
    /// [`crate::anexo_no_disco::abrir_parcial`], que confere o nome de novo, e o
    /// arquivo só ganha o nome final depois de o hash conferir — e, se ele
    /// estiver tomado, ganha o de ao lado, «foto (2).png», em vez de passar por
    /// cima. O caminho real volta em [`Transferencia::Salvo`]. O ADR 0027 não dá
    /// a cliente nenhum do SEELE um botão que abre arquivo; salvar é um ato de
    /// quem recebeu.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn salvar_anexo(
        &self,
        anexo: AttachmentId,
        pasta: std::path::PathBuf,
        nome: String,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::SalvarAnexo { anexo, pasta, nome })
            .await
    }

    /// Pede os bytes de um anexo **para a memória**, para olhar o começo deles.
    ///
    /// Devolve a caixa em que a resposta vai cair, e devolve **na hora**: quem
    /// chama decide onde esperar. Esperar aqui dentro faria a fila de comandos
    /// desta sessão parar pelo tempo de um download — ninguém conseguiria dizer
    /// uma frase enquanto uma prévia baixa, que é exatamente o bloqueio de
    /// cabeça de fila que o fluxo próprio de cada anexo existe para evitar.
    ///
    /// Um anexo maior que [`seele_core::preview::PREVIEW_LIMIT`] volta como
    /// [`Previa::GrandeDemais`] sem que um byte do corpo seja lido.
    ///
    /// [`seele_core::preview::PREVIEW_LIMIT`]: crate::preview::PREVIEW_LIMIT
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn prever_anexo(
        &self,
        anexo: AttachmentId,
    ) -> Result<tokio::sync::oneshot::Receiver<Previa>, Fechado> {
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.mandar(Comando::PreverAnexo { anexo, resposta })
            .await?;
        Ok(caixa)
    }

    /// Começa a compartilhar a tela escolhida, com os tetos escolhidos.
    ///
    /// **Não volta com a transmissão pronta**, e não teria como: o que sai
    /// daqui é um `StartScreenShare`, e o nome da transmissão — o
    /// [`ScreenId`] — só chega depois, num `ScreenShareStarted` do fluxo de
    /// eventos. É por isso que este verbo não devolve nada além de «foi
    /// mandado»: entre o botão e o primeiro quadro há uma volta de rede, e
    /// prometer aqui seria prometer no lugar do servidor.
    ///
    /// **Não é refeito depois de uma queda**, ao contrário da sala de voz e da Linha.
    /// Ver o comentário de `Motor::lembrar`: uma transmissão que voltasse
    /// sozinha cinco minutos depois poria a tela de alguém no ar sem que
    /// ninguém tivesse apertado nada.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn compartilhar_tela(
        &self,
        fonte: PedidoDeTela,
        limites: LimitesDeTela,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::CompartilharTela {
            fonte: Box::new(fonte),
            limites,
        })
        .await
    }

    /// Troca os tetos de uma transmissão em curso. Ver
    /// [`Comando::AjustarLimitesDaTela`].
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn ajustar_limites_da_tela(&self, limites: LimitesDeTela) -> Result<(), Fechado> {
        self.mandar(Comando::AjustarLimitesDaTela { limites }).await
    }

    /// Para de compartilhar.
    ///
    /// Idempotente: parar sem estar compartilhando manda o verbo assim mesmo, e
    /// o servidor o ignora. A alternativa — conferir aqui — poria uma segunda
    /// autoridade sobre quem está transmitindo, e ela discordaria da primeira no
    /// primeiro atraso de rede.
    ///
    /// # Errors
    ///
    /// Falha se a sessão já tiver acabado.
    pub async fn parar_de_compartilhar(&self) -> Result<(), Fechado> {
        self.mandar(Comando::PararDeCompartilhar).await
    }

    /// Pede um quadro-chave a quem está compartilhando.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a conexão já foi embora.
    pub async fn pedir_quadro_chave(&self, tela: ScreenId) -> Result<(), Fechado> {
        self.mandar(Comando::PedirQuadroChave { tela }).await
    }

    /// Passa a receber a imagem desta transmissão, ou para de receber.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a conexão já foi embora.
    pub async fn assistir(&self, tela: ScreenId, quero: bool) -> Result<(), Fechado> {
        self.mandar(Comando::Assistir { tela, quero }).await
    }

    /// Encerra por vontade própria, e volta **depois** de o motor se despedir.
    ///
    /// Voltava assim que o pedido entrava na fila. Quem derrubasse o runtime em
    /// seguida — a thread da conexão da FFI faz isso na linha seguinte —
    /// derrubava junto o motor que ainda ia fechar a conexão, e o fechamento
    /// não saía: ver `Motor::despedir`. Agora ela espera a tarefa do motor
    /// terminar, com teto de um segundo (`ESPERA_DO_MOTOR_NA_SAIDA`); vencido o
    /// teto, a tarefa é abortada e a linha sai no registro.
    ///
    /// Pelo caminho normal, quando ela volta o `Encerrado` e tudo o que veio
    /// antes dele já estão na fila de [`Self::proximo`].
    pub async fn sair(&mut self) {
        let Some(mut motor) = self.tarefa.take() else {
            return;
        };
        let despedida = async {
            let _ = self.mandar(Comando::Sair).await;
            let _ = (&mut motor).await;
        };
        if tokio::time::timeout(ESPERA_DO_MOTOR_NA_SAIDA, despedida)
            .await
            .is_err()
        {
            tracing::warn!(
                teto = ?ESPERA_DO_MOTOR_NA_SAIDA,
                "o motor do enlace não se despediu dentro do teto da saída, e foi abortado: \
                 quem ficou pode notar a saída só pelo prazo de ociosidade, 20 s"
            );
            motor.abort();
        }
    }

    /// Declara ao servidor a identidade efêmera desta sessão para o caminho
    /// entre pares.
    ///
    /// **Chamada uma vez, sobre quem já venceu** — nunca de dentro do funil
    /// que conecta candidatos. Achado do fix round 2: `conectar_por` conecta
    /// **cada** candidato de uma conexão com múltiplos destinos, não só o
    /// vencedor da corrida (`Enlace::tentar_entre`), e declarar de lá dentro
    /// fazia todo candidato mandar `EmprestarSubida` pela própria conexão —
    /// o perdedor, cuja conexão fecha logo depois, apagava ao sair a
    /// declaração que o vencedor tinha acabado de fazer (`Pares` é chaveado
    /// por pessoa, e os dois candidatos são a mesma pessoa). Por isso esta
    /// função só é chamada nos pontos de retorno de `conectar` e
    /// `tentar_entre`, depois de já se saber quem ganhou.
    async fn declarar_identidade_de_par(&self) -> Result<(), Fechado> {
        self.mandar(Comando::DeclararIdentidadeDePar).await
    }

    /// Passa a valer este consentimento no caminho entre pares — inclusive
    /// quando a resposta nova é «nada».
    ///
    /// **É opt-in, e as duas metades são independentes** (§5 da spec de
    /// 05/09). Ver [`ConsentimentoDePar`]: privacidade, porque o endereço de
    /// quem consente é entregue à outra ponta; e custo, porque quem empresta
    /// passa a subir cópias para outras pessoas. Nada disto acontece sem esta
    /// chamada.
    ///
    /// **A retirada alcança o que já está no ar.** Desligar não é só parar de
    /// aceitar pedidos novos: o repasse em curso é cancelado e os caminhos de
    /// par abertos são derrubados, aqui, antes de a declaração nova sair —
    /// ver `Motor::passar_a_consentir`. Quem estava sendo servido por esta
    /// máquina volta a ser servido pelo servidor.
    ///
    /// # O teto desta versão
    ///
    /// [`PARES_QUE_ESTA_VERSAO_ATENDE`] é o máximo que este cliente consegue
    /// honrar, e valores acima dele são **baixados** antes de virar declaração:
    /// declarar um teto que esta máquina não atende faria o servidor apontá-la
    /// duas vezes e a segunda pessoa esperar o prazo do par vencer por uma
    /// promessa que nunca teve como ser cumprida.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn consentir_no_caminho_entre_pares(
        &self,
        consentimento: ConsentimentoDePar,
    ) -> Result<ConsentimentoDePar, Fechado> {
        // **Um `let` só, usado duas vezes.** O que vai no fio e o que volta a
        // quem pediu não podem diferir, e a maneira de garantir isso é não
        // haver dois valores — e não uma asserção dizendo que os dois são
        // iguais.
        let valeu = cabivel(consentimento);
        self.mandar(Comando::ConsentirNoCaminhoEntrePares(valeu))
            .await?;
        Ok(valeu)
    }

    /// Quem joga entra num túnel de jogo: o servidor confere a ação
    /// `entrar-no-jogo`, e a resposta é a porta em `127.0.0.1` onde o jogo
    /// deve ligar. As medidas chegam depois como [`Aviso::Tunel`].
    ///
    /// Um servidor que negociou menos de 12 recebe **nada**: a resposta é
    /// [`RecusaDeTunel::ServidorAntigo`] sem escrever no fio.
    ///
    /// Espera até `PRAZO_DO_PAR` pela resposta do servidor. Sem resposta
    /// (sessão caída, prazo vencido) a recusa é [`RecusaDeTunel::SemResposta`];
    /// sem porta local livre, [`RecusaDeTunel::PortaLocalOcupada`]. As duas só
    /// existem aqui: nunca atravessam o fio.
    ///
    /// # Errors
    ///
    /// A recusa do servidor, ou as locais descritas acima.
    pub async fn entrar_no_tunel(
        &self,
        mod_id: &str,
        nome: &str,
    ) -> Result<crate::tunel::PortaLocal, RecusaDeTunel> {
        self.pedir_entrada_no_tunel(mod_id, nome)
            .await?
            .aguardar()
            .await
    }

    /// A primeira metade de [`Self::entrar_no_tunel`]: manda o pedido e devolve
    /// a espera, **sem esperar**.
    ///
    /// Existe para quem tem só um `&Enlace` dentro de um laço que não pode
    /// parar (o da FFI, que também entrega os avisos e as teclas de quem usa):
    /// a espera de até `PRAZO_DO_PAR` vai para uma tarefa à parte, e o laço
    /// segue. A espera é dona do que precisa e não segura o enlace.
    ///
    /// # Errors
    ///
    /// As recusas que se sabem antes de mandar: nome malformado e sessão
    /// caída.
    pub async fn pedir_entrada_no_tunel(
        &self,
        mod_id: &str,
        nome: &str,
    ) -> Result<EsperaDeTunel, RecusaDeTunel> {
        // A régua do fio, aplicada antes: um nome que o servidor recusaria
        // como malformado não existe.
        seele_proto::tunel::check_tunel(mod_id, nome).map_err(|_| RecusaDeTunel::NaoExiste)?;
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.mandar(Comando::EntrarNoTunel {
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
            resposta,
        })
        .await
        .map_err(|_| RecusaDeTunel::SemResposta)?;
        Ok(EsperaDeTunel { caixa })
    }

    /// Quem joga deixa o túnel: a porta fecha e os fluxos acabam.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn sair_do_tunel(&self, mod_id: &str, nome: &str) -> Result<(), Fechado> {
        self.mandar(Comando::SairDoTunel {
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
        })
        .await
    }

    /// Quem joga deixa todos os túneis de um MOD (o MOD foi fechado).
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn sair_dos_tuneis_do_mod(&self, mod_id: &str) -> Result<(), Fechado> {
        self.mandar(Comando::SairDosTuneisDoMod {
            mod_id: mod_id.to_owned(),
        })
        .await
    }

    /// A máquina passa a servir `porta` (a do jogo, em `127.0.0.1` desta
    /// máquina) como o túnel `mod_id/nome`.
    ///
    /// Nesta parte da API 8 ninguém no produto chama isto: quem serve é a
    /// metade de máquina, da parte 3. Existe para a prova de conformidade.
    ///
    /// # Errors
    ///
    /// A recusa do servidor, [`RecusaDeTunel::ServidorAntigo`] sem escrever no
    /// fio, ou [`RecusaDeTunel::SemResposta`] se ninguém respondeu.
    pub async fn servir_tunel(
        &self,
        mod_id: &str,
        nome: &str,
        protocolo: ProtocoloDeTunel,
        porta: u16,
    ) -> Result<(), RecusaDeTunel> {
        seele_proto::tunel::check_tunel(mod_id, nome).map_err(|_| RecusaDeTunel::NaoExiste)?;
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.mandar(Comando::ServirTunel {
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
            protocolo,
            porta,
            resposta,
        })
        .await
        .map_err(|_| RecusaDeTunel::SemResposta)?;
        match tokio::time::timeout(PRAZO_DO_PAR, caixa).await {
            Ok(Ok(resultado)) => resultado,
            Ok(Err(_)) | Err(_) => Err(RecusaDeTunel::SemResposta),
        }
    }

    /// A máquina para de servir o túnel.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn parar_tunel(&self, mod_id: &str, nome: &str) -> Result<(), Fechado> {
        self.mandar(Comando::PararTunel {
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
        })
        .await
    }

    /// Por que as conexões de um túnel que **esta máquina serve** acabaram,
    /// contadas por motivo — é aqui que aparece `JogoRecusou`, que só a
    /// máquina sabe. Vazio para um túnel que ela não serve.
    #[must_use]
    pub fn quedas_do_tunel_servido(
        &self,
        mod_id: &str,
        nome: &str,
    ) -> Vec<(crate::tunel::Queda, u32)> {
        self.maquina.quedas(mod_id, nome)
    }

    /// Quantos canais de túnel a máquina cortou por nomearem outro túnel.
    #[must_use]
    pub fn canais_de_tunel_cortados(&self) -> u64 {
        self.maquina.canais_alheios()
    }

    /// Com quantas pessoas esta máquina mantém, agora, uma ligação direta que
    /// aceita canais de túnel. Cai a zero quando o servidor retira de todas
    /// elas o direito aos túneis (`TunelRetirado`).
    #[must_use]
    pub fn ligacoes_diretas_da_maquina(&self) -> usize {
        self.maquina.ligacoes_diretas()
    }

    /// Quantos datagramas de carga chegaram a um canal `j` da máquina antes da
    /// abertura dele: descartados e contados, sem cortar o canal.
    #[must_use]
    pub fn datagramas_de_tunel_antes_da_abertura(&self) -> u64 {
        self.maquina.dados_antes_da_abertura()
    }

    // ------------------------------------------------- a máquina de jogo

    /// Se este enlace mostra pedidos de máquina a alguém.
    ///
    /// **Começa em `false`, e é o guarda do Desvio 5.** Sem tela não há quem
    /// leia a oferta, e um pedido que ninguém lê não pode ficar parado: o
    /// motor responde `RecusaDeMaquina { SemTela }` na hora, e loga. Só quem
    /// tem tela — o app — liga isto, e aí o pedido vira
    /// [`Aviso::PedidoDeMaquina`].
    ///
    /// Começa em `false` **a menos que o processo tenha dito que tem tela**
    /// ([`este_processo_tem_tela`]): ligar só aqui, depois da conexão, perde os
    /// pedidos que o servidor manda na chegada.
    pub fn aceitar_pedidos_de_maquina(&self, sim: bool) {
        self.com_tela
            .store(sim, std::sync::atomic::Ordering::Relaxed);
    }

    /// A pessoa aceitou a oferta de `resumo` — o do [`Aviso::PedidoDeMaquina`],
    /// como veio.
    ///
    /// Um servidor que negociou menos de 12 não recebe nada: o verbo fica no
    /// log, e não no fio.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn aceitar_maquina(
        &self,
        mod_id: &str,
        servidor: &str,
        resumo: &str,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::VerboDaMaquina(ClientMessage::AceiteDeMaquina {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            resumo: resumo.to_owned(),
        }))
        .await
    }

    /// A pessoa (ou quem decide por ela) não vai rodar a máquina.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn recusar_maquina(
        &self,
        mod_id: &str,
        servidor: &str,
        motivo: seele_proto::maquina::MotivoDaRecusaDeMaquina,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::VerboDaMaquina(ClientMessage::RecusaDeMaquina {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            motivo,
        }))
        .await
    }

    /// A máquina parou, e por quê.
    ///
    /// # Errors
    ///
    /// [`Fechado`] quando a sessão já acabou.
    pub async fn parar_maquina(
        &self,
        mod_id: &str,
        servidor: &str,
        motivo: seele_proto::maquina::MotivoDaParadaDeMaquina,
    ) -> Result<(), Fechado> {
        self.mandar(Comando::VerboDaMaquina(ClientMessage::MaquinaParada {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            motivo,
        }))
        .await
    }

    /// Um pedido da máquina à metade de servidor do MOD, e a resposta dela.
    ///
    /// A resposta é o `ModReply` de sempre, no mesmo espaço de números de
    /// `request` do [`Self::mod_request`] — o motor escolhe o número, numa
    /// faixa que a casca não usa, e monta as partes. `MaquinaDispensada {
    /// SemAceite }` para este MOD e servidor de jogo é a recusa do servidor.
    ///
    /// # Errors
    ///
    /// [`FalhaDoPedidoDaMaquina::SemAceite`] quando o servidor diz que esta
    /// conexão não é a máquina; [`FalhaDoPedidoDaMaquina::SemResposta`] depois
    /// de [`PRAZO_DO_PEDIDO_DA_MAQUINA`], com a sessão caída, ou quando o
    /// pedido nem pôde sair.
    pub async fn pedido_da_maquina(
        &self,
        mod_id: &str,
        servidor: &str,
        payload: String,
    ) -> Result<String, FalhaDoPedidoDaMaquina> {
        self.pedir_da_maquina(mod_id, servidor, payload)
            .await?
            .aguardar()
            .await
    }

    /// A primeira metade de [`Self::pedido_da_maquina`]: manda, e devolve a
    /// espera sem esperar. Pela razão de [`Self::pedir_entrada_no_tunel`].
    ///
    /// # Errors
    ///
    /// [`FalhaDoPedidoDaMaquina::SemResposta`] quando a sessão já acabou.
    pub async fn pedir_da_maquina(
        &self,
        mod_id: &str,
        servidor: &str,
        payload: String,
    ) -> Result<EsperaDoPedidoDaMaquina, FalhaDoPedidoDaMaquina> {
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.mandar(Comando::PedidoDaMaquina {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            payload,
            resposta,
        })
        .await
        .map_err(|_| FalhaDoPedidoDaMaquina::SemResposta)?;
        Ok(EsperaDoPedidoDaMaquina { caixa })
    }

    /// A primeira metade de [`Self::servir_tunel`]: manda, e devolve a espera
    /// sem esperar. Pela razão de [`Self::pedir_entrada_no_tunel`]: o laço da
    /// FFI não para.
    ///
    /// # Errors
    ///
    /// As recusas que se sabem antes de mandar: nome malformado e sessão
    /// caída.
    pub async fn pedir_servico_de_tunel(
        &self,
        mod_id: &str,
        nome: &str,
        protocolo: ProtocoloDeTunel,
        porta: u16,
    ) -> Result<EsperaDeServico, RecusaDeTunel> {
        seele_proto::tunel::check_tunel(mod_id, nome).map_err(|_| RecusaDeTunel::NaoExiste)?;
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.mandar(Comando::ServirTunel {
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
            protocolo,
            porta,
            resposta,
        })
        .await
        .map_err(|_| RecusaDeTunel::SemResposta)?;
        Ok(EsperaDeServico { caixa })
    }

    async fn mandar(&self, comando: Comando) -> Result<(), Fechado> {
        self.comandos.send(comando).await.map_err(|_| Fechado)
    }
}

/// Se um servidor que negociou `negociado` conhece o relato de recepção
/// (`ClientMessage::RelatoDeRecepcao`).
///
/// A tabela é a do protocolo (`desde_do_cliente`), pela mesma porta dos outros
/// verbos: um servidor 12 — a v0.19.6 publicada — leria uma variante que não
/// conhece. Zero, o «ainda sem aperto de mão», também não ouve.
#[must_use]
pub fn o_servidor_ouve_o_relato(negociado: u8) -> bool {
    let relato = ClientMessage::RelatoDeRecepcao(Recepcao {
        perda: None,
        jitter_ms: None,
    });
    seele_sessao::perfil::portao_do_cliente(&relato, negociado).is_ok()
}

/// A resposta que [`Enlace::pedir_servico_de_tunel`] ainda espera.
#[derive(Debug)]
pub struct EsperaDeServico {
    caixa: tokio::sync::oneshot::Receiver<Result<(), RecusaDeTunel>>,
}

impl EsperaDeServico {
    /// Espera até `PRAZO_DO_PAR`, como [`Enlace::servir_tunel`].
    ///
    /// # Errors
    ///
    /// A recusa do servidor, ou [`RecusaDeTunel::SemResposta`].
    pub async fn aguardar(self) -> Result<(), RecusaDeTunel> {
        match tokio::time::timeout(PRAZO_DO_PAR, self.caixa).await {
            Ok(Ok(resultado)) => resultado,
            Ok(Err(_)) | Err(_) => Err(RecusaDeTunel::SemResposta),
        }
    }
}

/// O primeiro número de `request` dos pedidos da máquina.
///
/// O mesmo espaço do `ModRequest`, que a casca conta de 1 para cima: a faixa
/// de cima é deste motor, e o `ModReply` de um número que ele pôs no ar é dele
/// — não vai à casca, onde seria a resposta de um pedido que ela não fez.
const PRIMEIRO_PEDIDO_DA_MAQUINA: u32 = 0x8000_0000;

/// Quantos pedidos de máquina avisados sem resposta se guardam para não
/// repetir o aviso. Quem os gera é o servidor; um teto impede que um servidor
/// que varia a oferta encha a memória deste lado.
const MAX_PEDIDOS_AVISADOS: usize = 64;

/// Quantas partes um `ModReply` de pedido da máquina pode ter. As do servidor
/// são de 10 KiB; isto é folga, e o que passar disso é resposta torta.
const MAX_PARTES_DA_RESPOSTA: u32 = 1024;

/// Um pedido da máquina no ar.
#[derive(Debug)]
struct PedidoNoAr {
    mod_id: String,
    servidor: String,
    partes: Vec<Option<String>>,
    resposta: tokio::sync::oneshot::Sender<Result<String, FalhaDoPedidoDaMaquina>>,
}

/// O que o motor sabe da máquina de jogo desta pessoa.
#[derive(Debug)]
struct LadoDaMaquina {
    /// Ver [`Enlace::aceitar_pedidos_de_maquina`].
    com_tela: Arc<std::sync::atomic::AtomicBool>,
    /// O contador dos pedidos da máquina, somado a
    /// [`PRIMEIRO_PEDIDO_DA_MAQUINA`].
    contador: u32,
    no_ar: std::collections::HashMap<u32, PedidoNoAr>,
    /// `(mod_id, servidor, resumo)` dos pedidos já avisados à casca e ainda
    /// sem resposta dela.
    avisados: std::collections::HashSet<(String, String, String)>,
}

/// Se este processo tem tela: o padrão de todo enlace que nasce nele. Ver
/// [`este_processo_tem_tela`].
static TELA_DESTE_PROCESSO: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Diz que **todo enlace que nascer daqui em diante** neste processo tem tela
/// para pedidos de máquina — o padrão de
/// [`Enlace::aceitar_pedidos_de_maquina`], que cada enlace ainda pode mudar.
///
/// # Por que existe, ao lado do método do enlace
///
/// O servidor manda os pedidos pendentes **na chegada**, logo depois do
/// `Session` — e o motor decide na hora em que o pedido chega. Um
/// `aceitar_pedidos_de_maquina(true)` chamado depois de o `Enlace` voltar
/// perde essa corrida: o pedido já foi recusado com `SemTela`, e o servidor
/// não pergunta de novo a esta conexão. Quem tem tela de nascença (o app)
/// diz isso uma vez, antes de conectar, e a corrida não existe.
///
/// É do processo e não de uma conexão porque é isso que a frase do Desvio 5
/// diz: o app tem tela, o CLI não tem. **Só o `main` do app chama isto**; um
/// teste que o chamasse mudaria o padrão dos outros testes do mesmo binário.
pub fn este_processo_tem_tela(sim: bool) {
    TELA_DESTE_PROCESSO.store(sim, std::sync::atomic::Ordering::Relaxed);
}

impl Default for LadoDaMaquina {
    fn default() -> Self {
        Self {
            com_tela: Arc::new(std::sync::atomic::AtomicBool::new(
                TELA_DESTE_PROCESSO.load(std::sync::atomic::Ordering::Relaxed),
            )),
            contador: 0,
            no_ar: std::collections::HashMap::new(),
            avisados: std::collections::HashSet::new(),
        }
    }
}

impl LadoDaMaquina {
    fn com_tela(&self) -> bool {
        self.com_tela.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn proximo_numero(&mut self) -> u32 {
        self.contador = self.contador.wrapping_add(1) & !PRIMEIRO_PEDIDO_DA_MAQUINA;
        PRIMEIRO_PEDIDO_DA_MAQUINA | self.contador
    }

    /// `true` na primeira vez deste pedido: é a vez de a casca o ver.
    fn avisar(&mut self, mod_id: &str, servidor: &str, resumo: &str) -> bool {
        let chave = (mod_id.to_owned(), servidor.to_owned(), resumo.to_owned());
        if self.avisados.contains(&chave) {
            return false;
        }
        if self.avisados.len() >= MAX_PEDIDOS_AVISADOS {
            tracing::warn!(
                avisados = self.avisados.len(),
                "máquina: pedidos demais sem resposta da casca; a lista de repetidos recomeça"
            );
            self.avisados.clear();
        }
        self.avisados.insert(chave);
        true
    }

    /// A casca respondeu, ou o servidor dispensou: um pedido seguinte desta
    /// máquina é novo.
    fn esquecer(&mut self, mod_id: &str, servidor: &str) {
        self.avisados
            .retain(|(m, s, _)| !(m == mod_id && s == servidor));
    }

    /// Falha todo pedido no ar desta máquina.
    fn falhar(&mut self, mod_id: &str, servidor: &str, falha: FalhaDoPedidoDaMaquina) {
        let numeros: Vec<u32> = self
            .no_ar
            .iter()
            .filter(|(_, pedido)| pedido.mod_id == mod_id && pedido.servidor == servidor)
            .map(|(numero, _)| *numero)
            .collect();
        for numero in numeros {
            if let Some(pedido) = self.no_ar.remove(&numero) {
                tracing::info!(
                    mod_id,
                    servidor,
                    request = numero,
                    motivo = falha.nome(),
                    "máquina: o pedido à metade de servidor falhou"
                );
                let _ = pedido.resposta.send(Err(falha));
            }
        }
    }

    /// Uma parte de resposta de um pedido no ar.
    fn parte(&mut self, request: u32, part: u32, total: u32, payload: String) {
        let Some(pedido) = self.no_ar.get_mut(&request) else {
            return;
        };
        let torta = total == 0
            || total > MAX_PARTES_DA_RESPOSTA
            || part >= total
            || (!pedido.partes.is_empty() && pedido.partes.len() != total as usize);
        if torta {
            if let Some(pedido) = self.no_ar.remove(&request) {
                tracing::warn!(mod_id = %pedido.mod_id, servidor = %pedido.servidor, request, part, total, motivo = "resposta-torta", "máquina: a resposta do pedido não se monta");
                let _ = pedido
                    .resposta
                    .send(Err(FalhaDoPedidoDaMaquina::SemResposta));
            }
            return;
        }
        if pedido.partes.is_empty() {
            pedido.partes = vec![None; total as usize];
        }
        if let Some(lugar) = pedido.partes.get_mut(part as usize) {
            *lugar = Some(payload);
        }
        if pedido.partes.iter().all(Option::is_some) {
            if let Some(pedido) = self.no_ar.remove(&request) {
                let inteira: String = pedido.partes.into_iter().flatten().collect();
                let _ = pedido.resposta.send(Ok(inteira));
            }
        }
    }

    /// A conexão caiu: nada no ar terá resposta, e o servidor da conexão nova
    /// pede de novo o que ainda quiser.
    fn soltar(&mut self) {
        for (numero, pedido) in self.no_ar.drain() {
            tracing::info!(mod_id = %pedido.mod_id, servidor = %pedido.servidor, request = numero, motivo = "conexao-caiu", "máquina: o pedido à metade de servidor ficou sem resposta");
            let _ = pedido
                .resposta
                .send(Err(FalhaDoPedidoDaMaquina::SemResposta));
        }
        self.avisados.clear();
    }
}

impl Drop for Enlace {
    fn drop(&mut self) {
        // Só falta a alça depois de `sair`, que já a esperou ou abortou.
        if let Some(tarefa) = self.tarefa.take() {
            tarefa.abort();
        }
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
    /// O bilhete de encontro do convite, quando o link trouxe um.
    ///
    /// Degrau 4 do ADR 0022, e ele vale por reconexão: ver [`Motor::tentar`].
    bilhete: Option<seele_proto::uri::Bilhete>,
    chave: SigningKey,
    pins: Arc<dyn PinStore>,
    cliente: Option<Client>,
    bateria: Battery,
    inicio: Instant,
    /// O que restaurar ao reconectar.
    voice_room: Option<VoiceRoomId>,
    /// As entradas pedidas que o servidor ainda não decidiu, pela mesma regra
    /// do `Room` da casca. Ver [`crate::assento`].
    entradas: crate::assento::EntradasPendentes,
    /// O que a conexão que caiu deixou: o `ssrc` dela e o assento confirmado.
    ///
    /// É com isto que a volta sabe se o servidor resgatou o assento — o
    /// `ssrc` da sessão nova igual ao desta — e qual.
    queda: Option<Queda>,
    linha: Option<ChannelId>,
    muted: bool,
    isolamento: bool,
    avisos: mpsc::UnboundedSender<Aviso>,
    rtt: Arc<std::sync::atomic::AtomicU64>,
    caminho_medido: Arc<std::sync::atomic::AtomicU32>,
    /// O protocolo negociado, o mesmo átomo de [`Enlace`].
    protocolo: Arc<std::sync::atomic::AtomicU8>,
    /// A escolha da pessoa, esperando a transmissão ganhar nome.
    ///
    /// **A bomba não pode nascer no comando**, e é a forma da coisa e não uma
    /// preguiça: [`Client::start_screen_share`] manda um pedido, e o
    /// [`ScreenId`] — que é o que `escoar` escreve no cabeçalho de abertura —
    /// só chega depois, num `ScreenShareStarted` do laço de mensagens. Entre os
    /// dois instantes, isto é a transmissão inteira.
    tela_pedida: Option<(Box<PedidoDeTela>, LimitesDeTela)>,
    /// A bomba viva desta pessoa, quando há uma.
    tela_viva: Option<TelaViva>,
    /// A faixa do sinal da **própria** voz, lida do que o servidor devolve.
    ///
    /// Era uma constante `Nominal`, e a dívida estava escrita no lugar dela: o
    /// teto respondia ao `HostUplink` e ao número de espectadores e **não** ao
    /// sinal da voz piorando — numa sala onde a voz começava a doer, a tela não
    /// cedia sozinha. Faltava metade da regra de aceite do §3.2.
    ///
    /// O que fechava a dívida já vinha pelo fio e ninguém guardava: o servidor
    /// calcula a taxa de cada pessoa e a devolve em `PersonState`, uma vez por
    /// segundo. O `Signal` da casca é a mesma conta feita de novo para
    /// desenhar — não é fonte, é cópia —, e é por isso que isto não precisou de
    /// comando novo nem de a casca falar com o núcleo.
    faixa: SignalBand,
    /// A subida de quem hospeda, como o `HostUplink` a mediu. §5.1.
    ///
    /// `None` é «não medido», e é assim que o zero do protocolo chega aqui: a
    /// perna fica no cano das provas em vez de zerar o teto.
    caminho_de_quem_hospeda_bps: Option<u32>,
    /// Quantos estão assistindo, como o `ScreenViewers` contou. O N do §5.1.
    espectadores: u32,
    /// A subida **desta** máquina, medida enquanto a tela enche.
    ///
    /// Era a perna que faltava, e a falta estava escrita no lugar dela: o teto
    /// respondia ao `HostUplink`, ao número de espectadores e à faixa da voz, e
    /// para o caminho de quem compartilha usava a suposição de
    /// `crate::tela::CAMINHO_DA_PROVA_BPS` — 2 Mbps do cano em que os spikes
    /// rodaram. Quem tinha menos que isso descobria pela voz doendo; quem tinha
    /// mais nunca descobria, e olhava 720p numa casa onde cabia 1080p.
    ///
    /// Quem mede é a [`crate::caminho::Sonda`], e quem a alimenta é este motor,
    /// na tica que ele já tem — ver [`Motor::medir_o_caminho`]. Era a pergunta
    /// 2 do §8.
    caminho: crate::caminho::Sonda,
    /// O caminho de saída como a mídia RTP o estima (ADR 0067), já na
    /// medida que o [`crate::tela::TetoDeVideo`] espera. `None` sem
    /// transmissão por mídia: aí vale a sonda.
    estimativa_da_midia: Option<u32>,
    /// Quando o último quadro-chave foi pedido a quem codifica, numa
    /// transmissão por mídia, e se há um pedido esperando a vez
    /// ([`ENTRE_CHAVES_PELA_MIDIA`]).
    ultima_chave_pela_midia: Option<Instant>,
    chave_devida: bool,
    /// Quando a estimativa da mídia mudou o teto pela última vez
    /// ([`SUBIDA_DO_TETO_PELA_MIDIA`]).
    ultima_mudanca_da_midia: Option<Instant>,
    /// Se já há uma tentativa de servir um par em curso.
    ///
    /// No A1 quem empresta serve um par por vez (`crate::par::atender` só
    /// aceita uma ligação). Um `SirvaTelaPara` que chegasse no meio de outro
    /// não teria vaga — a escolha aqui é recusar em silêncio para o servidor
    /// (que não espera resposta nenhuma desta mensagem) e deixar só o rastro.
    ///
    /// `Arc<AtomicBool>` e não `bool` simples: a tarefa solta que
    /// [`Motor::servir_par`] cria precisa devolver a vaga quando termina, e
    /// ela não tem `&mut Motor` — só a cópia deste punho.
    atendendo_pares: Arc<std::sync::atomic::AtomicBool>,
    /// O que a pessoa consentiu no caminho entre pares.
    ///
    /// Guardado aqui e não só mandado uma vez porque a declaração **não
    /// sobrevive à sessão**: o servidor a apaga em `Pares::saiu` quando a
    /// conexão morre, e cada reconexão tem de dizer de novo quem esta máquina
    /// é e o que ela consente. Sem este campo, uma queda de rede tiraria a
    /// pessoa da malha caladamente e ela só descobriria por ninguém mais ser
    /// servido.
    ///
    /// Nasce em [`ConsentimentoDePar::de_ninguem`]: um opt-in que começasse
    /// ligado não seria um opt-in.
    consentimento: ConsentimentoDePar,
    /// Onde o repasse ao par vai buscar os bytes da tela que chega do
    /// servidor. Ver [`RepasseDeTela`].
    repasse: Arc<RepasseDeTela>,
    /// Por onde as tarefas soltas do caminho entre pares devolvem o que
    /// decidiram — ver [`ResultadoDoPar`].
    resultados_do_par: mpsc::UnboundedReceiver<ResultadoDoPar>,
    /// A metade de [`Self::resultados_do_par`] que as tarefas soltas recebem,
    /// para mandar de volta. Clonada a cada tarefa nova: um `Sender` barato
    /// de clonar, e cada tarefa é dona da própria cópia.
    resultados_do_par_tx: mpsc::UnboundedSender<ResultadoDoPar>,
    /// As tarefas soltas do caminho entre pares — a única coisa que alcança
    /// aquele caminho depois de ele ter começado.
    ///
    /// **Sem elas, `assistir(tela, false)` mentia.** Ele mandava
    /// `UnwatchScreen` ao servidor e nada mais: a tarefa de
    /// [`Motor::assistir_por_par`] seguia viva, lendo do par, e quem tinha
    /// acabado de fechar a janela continuava recebendo a tela por baixo —
    /// gastando a subida de quem empresta com imagem que ninguém olhava. Pior,
    /// quando aquele fluxo enfim terminasse, o `ParFalhou` de rotina sairia e
    /// pediria ao servidor a tela de volta.
    ///
    /// **E guardá-las num mapa cru não bastava**, pelo mesmo motivo por outra
    /// porta: um mapa largado desprende as tarefas em vez de cancelá-las, e
    /// elas sobreviviam à morte do motor inteiro. Ver [`TarefasDePar`].
    tarefas_de_par: TarefasDePar,
    /// Os túneis de jogo: o de quem joga, o da máquina e os pedidos no ar.
    tuneis: crate::tunel::Tuneis,
    /// A máquina de jogo do lado de quem a opera: se há tela, os pedidos já
    /// avisados e os pedidos à metade de servidor no ar.
    lado_da_maquina: LadoDaMaquina,
}

/// O que uma conexão que caiu deixa para a volta. Ver [`Motor::queda`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Queda {
    ssrc: seele_proto::ids::Ssrc,
    sessao: seele_proto::ids::SessionId,
    confirmada: Option<VoiceRoomId>,
}

/// Uma transmissão desta pessoa que está no ar.
///
/// Não guarda a tarefa que escoa, e a ausência é deliberada: ela acaba sozinha
/// quando a bomba manda o [`EventoDaBomba::Fim`](crate::EventoDaBomba::Fim) —
/// **e é assim que o fluxo fecha direito**. Abortá-la seria cortar no meio de um
/// quadro e deixar quem assiste esperando o resto dele.
#[derive(Debug)]
struct TelaViva {
    /// O nome que o servidor deu a esta transmissão.
    tela: ScreenId,
    /// Quem produz os quadros. Largá-lo para a transmissão.
    produtor: Produtor,
    /// O que a pessoa escolheu, guardado porque o teto é recalculado a cada
    /// `ScreenViewers` e a escolha é uma das pernas dele (§5).
    limites: LimitesDeTela,
}

/// Quem produz os quadros de uma transmissão desta pessoa.
///
/// A tela tem a bomba — a thread que captura e codifica aqui. A câmera tem a
/// alça de `crate::de_fora`: quem codifica é a janela, e deste lado há só uma
/// tarefa que entrega os quadros a quem escoa. As ordens são as mesmas, e
/// cada uma vira o que o produtor sabe fazer com ela.
#[derive(Debug)]
enum Produtor {
    /// A thread do codificador de tela.
    Bomba(crate::bomba::Bomba),
    /// A câmera, codificada fora deste processo.
    DeFora(crate::de_fora::AlcaDeFora),
}

impl Produtor {
    /// O teto andou.
    fn teto(&self, teto: crate::tela::TetoDeVideo, faixa: SignalBand) {
        match self {
            Self::Bomba(bomba) => {
                let _ = bomba.teto(teto, faixa);
            }
            // Quem está de fora não lê as pernas do teto: recebe o resultado,
            // em bits por segundo, ou a parada com o motivo.
            Self::DeFora(alca) => {
                let _ = alca.teto(teto.teto(faixa));
            }
        }
    }

    /// A pessoa mexeu na resolução ou na cadência.
    ///
    /// Só a bomba troca de degrau sozinha. As medidas da câmera são as do
    /// codificador da janela, e mudá-las é ela recomeçar a transmissão.
    fn escolha(&self, resolucao: crate::Resolucao, cadencia: crate::Cadencia) {
        if let Self::Bomba(bomba) = self {
            let _ = bomba.escolha(resolucao, cadencia);
        }
    }

    /// Alguém que assiste precisa de um quadro-chave.
    fn chave(&self) {
        match self {
            Self::Bomba(bomba) => {
                let _ = bomba.chave();
            }
            Self::DeFora(alca) => {
                let _ = alca.chave();
            }
        }
    }
}

/// As tarefas soltas do caminho entre pares, e quem é dono delas.
///
/// # Uma alça largada desprende a tarefa; não a cancela
///
/// Enquanto estas alças moravam num `HashMap` cru dentro do [`Motor`], os dois
/// caminhos que matam o motor largavam o mapa sem tocá-lo — [`Enlace::drop`]
/// aborta a tarefa de [`Motor::rodar`], e um `Comando::Sair` a faz voltar —, e
/// cada tarefa de par ficava **viva** depois disso: lendo do par, com a conexão
/// QUIC de pé, gastando a subida de quem empresta por uma sessão que já tinha
/// acabado. E não em silêncio: a tarefa escreve no mesmo canal de avisos que o
/// `Enlace` continua segurando depois de `sair()`, então a casca recebia quadro
/// de tela **depois** do `Encerrado` que ela mesma pediu.
///
/// A tarefa de servir um par ([`Motor::servir_par`]) era pior: a alça dela não
/// era guardada em lugar nenhum. Ela não tinha sequer o acidente que salvava as
/// outras — descobrir no primeiro `send` que ninguém escuta —, porque não fala
/// com a casca: repassa bytes ao par até a conexão morrer.
///
/// # Por que um dono com `Drop`, e não uma chamada de limpeza
///
/// Porque «alguém tem de se lembrar de cancelar» é a forma de defeito que este
/// arquivo já pagou duas vezes. Atrás de um dono, cancelar passa a ser o que
/// acontece quando o motor some — por `abort`, por `return`, ou por um caminho
/// que ainda não existe.
///
/// É a escolha oposta à de [`TelaViva`], e o contraste é de propósito: a bomba
/// tem um fim direito a esperar — o `Fim` que fecha o fluxo —, e abortá-la
/// cortaria um quadro no meio. Estas tarefas não têm fim nenhum a esperar:
/// passam a vida paradas num `read` do par, sem ponto onde conferir um pedido
/// de parada.
#[derive(Default)]
struct TarefasDePar {
    /// A alça da tarefa que busca cada tela num par.
    ///
    /// Por [`ScreenId`] porque é isso que o comando nomeia — ver
    /// [`Motor::assistir_por_par`].
    caminhos: std::collections::HashMap<ScreenId, tokio::task::JoinHandle<()>>,
    /// A alça da tarefa que serve um par, quando esta máquina está servindo.
    ///
    /// Uma só porque no A1 quem empresta serve um par por vez — ver
    /// [`Motor::atendendo_pares`] e [`VagaDeAtendimento`].
    servindo: Option<tokio::task::JoinHandle<()>>,
}

impl TarefasDePar {
    /// Passa a ser dona da tarefa que busca esta tela num par.
    ///
    /// Um `AssistaTelaPor` novo para a mesma tela substitui a alça e aborta a
    /// anterior: duas tarefas lendo a mesma tela seriam duas cópias chegando, e
    /// a segunda nomeação é a que o servidor tem de pé.
    fn assistir(&mut self, screen: ScreenId, tarefa: tokio::task::JoinHandle<()>) {
        // As que já acabaram sozinhas saem daqui: a alça de uma tarefa morta
        // não aborta nada, e sem esta linha o mapa cresceria por sessão a cada
        // transmissão assistida.
        self.caminhos.retain(|_, alca| !alca.is_finished());
        if let Some(anterior) = self.caminhos.insert(screen, tarefa) {
            anterior.abort();
        }
    }

    /// Derruba o caminho desta tela, se houver um.
    fn parar_de_assistir(&mut self, screen: ScreenId) {
        if let Some(caminho) = self.caminhos.remove(&screen) {
            caminho.abort();
        }
    }

    /// Passa a ser dona da tarefa que serve um par.
    ///
    /// A anterior é abortada por garantia e não por necessidade:
    /// [`VagaDeAtendimento`] já impede que uma segunda comece enquanto a
    /// primeira corre, e uma alça que sobre aqui é de tarefa morta, sobre a
    /// qual `abort` não faz nada.
    fn servir(&mut self, tarefa: tokio::task::JoinHandle<()>) {
        if let Some(anterior) = self.servindo.replace(tarefa) {
            anterior.abort();
        }
    }

    /// Derruba **todos** os caminhos de par abertos, e deixa o resto de pé.
    ///
    /// Quem chama é a retirada do consentimento de assistir por par
    /// ([`Motor::passar_a_consentir`]): os caminhos existem por causa do
    /// endereço que a pessoa acabou de tirar de circulação. A tarefa que
    /// **serve** não é tocada — é o outro consentimento, e retirar um não é
    /// retirar o outro.
    fn parar_de_assistir_a_tudo(&mut self) {
        for (_, caminho) in self.caminhos.drain() {
            caminho.abort();
        }
    }

    /// Cancela a tarefa que serve um par, se houver uma.
    ///
    /// Quem chama é a retirada do consentimento de emprestar a conexão. A vaga
    /// volta por tabela: ela viaja para dentro da tarefa, e o [`Drop`] de
    /// [`VagaDeAtendimento`] a devolve quando a tarefa é solta.
    fn parar_de_servir(&mut self) {
        if let Some(servindo) = self.servindo.take() {
            servindo.abort();
        }
    }

    /// Cancela tudo o que está de pé.
    ///
    /// Chamado por [`Motor::cair`], onde o motor **sobrevive** ao corte e por
    /// isso não há `Drop` nenhum para fazer isto sozinho.
    fn largar_tudo(&mut self) {
        self.parar_de_assistir_a_tudo();
        self.parar_de_servir();
    }

    /// Cancela tudo o que está de pé **e espera as tarefas devolverem**, até
    /// `teto`.
    ///
    /// Chamado por [`Motor::despedir`], que diz o fim à casca logo depois. Um
    /// `abort` é um pedido: ele não interrompe um `poll` que já corre noutra
    /// thread, e as baterias rodam em `multi_thread`. Sem a espera, uma tarefa
    /// abortada ainda escreveria um quadro entre o pedido e o `await` em que ela
    /// morre — atrás do `Encerrado`, na mesma fila.
    ///
    /// A alça de uma tarefa abortada volta quando a tarefa foi solta de fato. O
    /// teto é para a que não volta, e ela fica como ficava antes: abortada e
    /// largada, agora com uma linha no registro.
    ///
    /// **A espera é plausível, e não provada.** Medido no lote F: com
    /// [`Self::largar_tudo`] no lugar desta — o `abort` sem esperar —,
    /// `a_despedida_cala_as_tarefas_de_par_antes_do_encerrado` passou 5 de 5 e
    /// `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para` 3 de 3. A
    /// corrida que ela cobre é de microssegundos, e nenhum teste a provoca. O
    /// que os dois testes provam é o cancelamento antes do aviso.
    async fn largar_tudo_e_esperar(&mut self, teto: Duration) {
        let alcas: Vec<tokio::task::JoinHandle<()>> = self
            .caminhos
            .drain()
            .map(|(_, alca)| alca)
            .chain(self.servindo.take())
            .collect();
        if alcas.is_empty() {
            return;
        }
        for alca in &alcas {
            alca.abort();
        }
        let todas = async {
            for alca in alcas {
                let _ = alca.await;
            }
        };
        if tokio::time::timeout(teto, todas).await.is_err() {
            tracing::warn!(
                ?teto,
                "uma tarefa do caminho entre pares não devolveu dentro do teto da despedida"
            );
        }
    }
}

impl Drop for TarefasDePar {
    fn drop(&mut self) {
        self.largar_tudo();
    }
}

/// A vaga de «estou servindo um par», enquanto ela está tomada.
///
/// # Por que um punho com `Drop`, e não duas escritas
///
/// A vaga era tomada no laço do motor e devolvida na **última linha** do corpo
/// da tarefa que serve. Enquanto ninguém cancelava aquela tarefa, as duas
/// escritas bastavam. Cancelá-la — e é o que [`TarefasDePar`] passou a fazer —
/// faz a última linha nunca correr, e uma vaga que não volta é esta máquina
/// **fora da malha para sempre**, sem erro em lugar nenhum: o servidor continua
/// apontando este par, porque a vaga dele voltou, e o cliente recusa cada
/// pedido em silêncio pela vaga que ficou.
///
/// Como guarda, devolver a vaga passa a ser o que acontece de todo jeito — pelo
/// fim do corpo, pelas saídas antecipadas de [`Motor::servir_par`], e pelo
/// `abort`, que solta a tarefa e com ela tudo o que ela segurava.
struct VagaDeAtendimento(Arc<std::sync::atomic::AtomicBool>);

impl VagaDeAtendimento {
    /// Toma a vaga, se ela estiver livre.
    ///
    /// `compare_exchange` e não um `load` seguido de `store`: conferir e tomar
    /// viram um ato só, sem instante entre os dois por onde um segundo pedido
    /// passasse pela mesma porta.
    ///
    /// `AcqRel` e não `Relaxed` porque a vaga guarda mais do que si mesma: quem
    /// a toma arma a ponta com `par::passar_a_atender`, e quem a devolve a
    /// desarmou antes. As duas metades têm de ser vistas em ordem por quem
    /// tomar a vaga depois.
    fn tomar(punho: &Arc<std::sync::atomic::AtomicBool>) -> Option<Self> {
        punho
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .ok()?;
        Some(Self(Arc::clone(punho)))
    }
}

impl Drop for VagaDeAtendimento {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Em que faixa o sinal da voz começa, antes de o servidor dizer a primeira.
///
/// Otimista de propósito, e o motivo é qual erro custa mais: começar em
/// `Critical` pararia a tela de alguém cuja voz está ótima, por causa de um
/// dado que ainda não chegou. Começar em `Nominal` deixa a tela abrir e ceder
/// no primeiro `PersonState`, que vem uma vez por segundo.
const FAIXA_INICIAL: SignalBand = SignalBand::Nominal;

/// De quanto em quanto tempo a bateria é consultada.
///
/// Menor que o intervalo de ping e muito menor que o menor backoff, para que
/// nem o ping nem uma tentativa de reconexão fiquem esperando a tica seguinte.
const TICA: Duration = Duration::from_millis(200);

/// Quanto a despedida mantém o runtime e a ponta vivos depois de fechar a
/// conexão (o `wait_idle` de [`Motor::despedir`]).
///
/// **O que ela não compra: a saída do `CONNECTION_CLOSE`.** Ele sai na
/// primeira volta do driver da conexão depois de `Client::disconnect`, e essa
/// volta acontece antes de [`Enlace::sair`] voltar, com ou sem esta espera.
/// Medido no lote F: sem o `wait_idle`, os três testes de
/// `saida_com_despedida.rs` continuam verdes, com quem saiu sumindo dos
/// presentes em menos de um milissegundo.
///
/// **O que ela compra: o período de fechamento do QUIC.** O
/// `CONNECTION_CLOSE` não é retransmitido como dado; quem fecha fica nesse
/// período e o manda de novo quando chega pacote do outro lado. Sem a espera,
/// o runtime e a ponta podem cair logo depois do primeiro envio, e um
/// fechamento perdido deixa o outro lado esperando o prazo de ociosidade, 20 s.
/// **Nenhum teste prova isto**: o laço local não perde pacote, e a bateria não
/// tem um caminho com perda.
///
/// O teto corta o período (3×PTO) numa rede longe. O custo é de quem sai: até
/// 300 ms na thread da conexão — e, pela `encerrar_e_esperar` da casca, na
/// thread principal do `ExitRequested` —, medido em ~86 ms no laço local.
const ESPERA_DA_DESPEDIDA: Duration = Duration::from_millis(300);

/// Quanto a despedida espera as tarefas do caminho entre pares devolverem,
/// depois de abortadas. Ver [`TarefasDePar::largar_tudo_e_esperar`].
///
/// Uma tarefa abortada devolve no próximo toque do runtime nela: microssegundos.
/// O teto é para a que estiver no meio de um `poll` longo noutra thread.
const ESPERA_DAS_TAREFAS_DE_PAR: Duration = Duration::from_millis(100);

/// Quanto [`Enlace::sair`] espera o motor se despedir, antes de abortá-lo.
///
/// Um segundo: a despedida do motor gasta no máximo
/// [`ESPERA_DAS_TAREFAS_DE_PAR`] e [`ESPERA_DA_DESPEDIDA`], quatrocentos
/// milissegundos. O que passar disso é um motor preso, e esperá-lo mais seria
/// prender junto quem está saindo.
const ESPERA_DO_MOTOR_NA_SAIDA: Duration = Duration::from_secs(1);

/// A faixa nova, quando este `PersonState` for sobre esta pessoa e mudar de faixa.
///
/// Separada do `Motor` porque é a decisão inteira, e uma decisão sobre valores
/// não precisa de conexão QUIC para ser conferida — o mesmo argumento que
/// `alcance::Alcance::decidir` usa no servidor.
///
/// `None` em três casos, e os três importam: a mensagem é sobre outra pessoa; a
/// sessão ainda não existe, e então ela não é sobre ninguém que este `Motor`
/// conheça; ou a faixa não mudou, e refazer o teto a cada chegada acordaria a
/// thread do codificador uma vez por segundo para lhe dizer o que ela já sabe.
fn faixa_nova(
    atual: SignalBand,
    estado: &seele_proto::control::PersonState,
    eu: Option<PersonId>,
) -> Option<SignalBand> {
    if eu != Some(estado.person) {
        return None;
    }
    let nova = SignalBand::of(estado.signal);
    (nova != atual).then_some(nova)
}

/// **A faixa que corta o teto do vídeo**, que não é sempre a do sinal.
///
/// No canal confiável, a faixa é o único jeito de o vídeo ceder quando a rede
/// piora: Degraded corta o teto pela metade, Critical para. Na mídia RTP quem
/// regula a taxa é a estimativa do TWCC (ADR 0067), que vê o atraso e a perda
/// de cada pacote e já cede ao congestionamento — e que **não** lê perda
/// sorteada como falta de banda. Cortar de novo pela faixa era cobrar a mesma
/// perda duas vezes: medido em 11/11/2026 a 5% de perda, o vídeo caía de
/// 2,4 para 1,4 Mbps com o caminho livre, e com perda e atraso juntos a faixa
/// Critical o parava — a parada que a estimativa já não causa.
///
/// A faixa continua valendo para o que a tela **mostra** (o sinal) e para o
/// canal. Só o teto do vídeo pela mídia deixa de segui-la.
const fn faixa_para_o_teto(do_sinal: SignalBand, pela_midia: bool) -> SignalBand {
    if pela_midia {
        SignalBand::Nominal
    } else {
        do_sinal
    }
}

impl Motor {
    async fn rodar(mut self, mut comandos: mpsc::Receiver<Comando>) {
        let mut tica = tokio::time::interval(TICA);
        tica.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            // Tirados antes do `select!`, e é o que faz o braço da tela
            // compilar: o braço de cima já pega `self.cliente` emprestado
            // mutável, e um segundo empréstimo de `self` no mesmo `select!` —
            // mesmo imutável — é recusado. Estes dois punhos não emprestam
            // nada: um `Arc` e um remetente, os dois baratos de clonar.
            let fila_de_telas = self
                .cliente
                .as_ref()
                .map(crate::client::Client::fila_de_telas);
            let avisos_da_tela = self.avisos.clone();
            // Pelo mesmo motivo dos dois de cima, e é um `Arc`: quem lê a tela
            // do servidor precisa do ponto de encontro com quem serve um par
            // (ver `RepasseDeTela`), e o braço logo abaixo não pode pedir
            // `self` de novo.
            let repasse_da_tela = Arc::clone(&self.repasse);
            // Tirado pela mesma razão dos dois de cima: `resultados_do_par` é
            // um campo próprio, disjunto de `self.cliente`, e emprestá-lo aqui
            // é o que deixa o braço novo do `select!` chamar `cliente.par_falhou`
            // sem pedir `self` de novo.
            let resultados_do_par = &mut self.resultados_do_par;
            // E a fila do que a mídia RTP diz ao motor, pelo mesmo motivo da
            // fila de telas. O pedido de quadro-chave precisa do `self`
            // inteiro, então o braço só o anota, e ele é atendido depois do
            // `select!`.
            let fila_da_midia = self.cliente.as_ref().and_then(Client::fila_da_midia);
            let mut pediram_chave_pela_midia = false;

            // Só há o que ler quando há conexão. Sem ela, a espera é o relógio.
            let houve_evento = match self.cliente.as_mut() {
                Some(cliente) => tokio::select! {
                    evento = cliente.next_event() => Some(evento),
                    // Uma tela alheia chegando. O roteador de `Client::connect`
                    // já separou este fluxo dos anexos; aqui ele vira quadros.
                    Some(fluxo) = espera_da_fila(&fila_de_telas) => {
                        escoar_tela_alheia(
                            avisos_da_tela.clone(),
                            FonteDaTelaAlheia::Canal(fluxo),
                            DeOndeVeioATela::Servidor(Arc::clone(&repasse_da_tela)),
                        );
                        None
                    }
                    // A mídia RTP (ADR 0067): a abertura de uma geração da
                    // transmissão desta pessoa vai ao servidor pelo controle,
                    // que só este laço escreve.
                    Some(da_midia) = espera_da_midia(&fila_da_midia) => {
                        match da_midia {
                            crate::client::DaMidia::Abertura(abertura) => {
                                if let Err(erro) = cliente.tela_por_midia(abertura).await {
                                    tracing::warn!(
                                        %erro,
                                        "a abertura da transmissão por RTP não saiu; ninguém vai vê-la"
                                    );
                                }
                            }
                            crate::client::DaMidia::PediramChave => {
                                pediram_chave_pela_midia = true;
                            }
                        }
                        None
                    }
                    // O que uma tarefa solta do caminho entre pares (ver
                    // `Motor::assistir_por_par`) decidiu. Só chega aqui um
                    // `ParFalhou` — ver `ResultadoDoPar` — porque só o laço de
                    // `rodar`, dono do `&mut Client`, pode falar com o servidor.
                    Some(resultado) = resultados_do_par.recv() => {
                        match resultado {
                            ResultadoDoPar::ParaOServidor(mensagem) => {
                                if let Err(erro) = cliente.enviar_verbo_do_tunel(&mensagem).await {
                                    let (mod_id, tunel) = match &mensagem {
                                        ClientMessage::TunelFalhou { mod_id, nome, .. }
                                        | ClientMessage::SairDoTunel { mod_id, nome } => {
                                            (mod_id.as_str(), nome.as_str())
                                        }
                                        _ => ("", ""),
                                    };
                                    tracing::warn!(
                                        mod_id,
                                        tunel,
                                        ?erro,
                                        "o verbo do túnel não saiu; o túnel segue pela estrela"
                                    );
                                }
                            }
                            ResultadoDoPar::ParFalhou { screen, motivo } => {
                                if let Err(erro) = cliente.par_falhou(screen, motivo).await {
                                    tracing::warn!(
                                        %erro,
                                        ?screen,
                                        "não deu para avisar o servidor que o par falhou"
                                    );
                                }
                            }
                        }
                        None
                    }
                    comando = comandos.recv() => {
                        match comando {
                            Some(Comando::Sair) | None => return self.despedir().await,
                            Some(comando) => { self.executar(comando).await; None }
                        }
                    }
                    _ = tica.tick() => None,
                },
                None => tokio::select! {
                    comando = comandos.recv() => {
                        match comando {
                            Some(Comando::Sair) | None => return self.despedir().await,
                            // Guardado, não perdido: entrar numa sala de voz durante a
                            // queda é uma intenção que vale quando voltar.
                            Some(comando) => {
                                // Um verbo da máquina não volta (`lembrar` não o
                                // guarda): o servidor da conexão nova pede de
                                // novo. Mas não some calado.
                                if let Some((mod_id, servidor)) = comando.da_maquina() {
                                    tracing::warn!(mod_id, servidor, comando = comando.nome(), motivo = "sem-conexao", "máquina: o verbo não saiu: o enlace está na bateria");
                                }
                                self.lembrar(&comando);
                                None
                            }
                        }
                    }
                    _ = tica.tick() => None,
                },
            };

            if pediram_chave_pela_midia {
                // O mesmo que um `KeyFrameRequested` do controle faz.
                self.pedir_a_chave_da_tela();
            }

            if let Some(evento) = houve_evento {
                match evento {
                    Ok(mensagem) => {
                        // O que a reconexão vai refazer também muda quando
                        // **outra pessoa** decide. Sem isto, alguém movido por
                        // um operador voltaria, depois de uma queda, para a
                        // sala de voz de onde foi tirado: o motor refaz a última sala de voz
                        // que este cliente pediu, e ele não pediu este.
                        self.ouvir_o_assento(&mensagem);
                        if matches!(mensagem, ServerMessage::Pong { .. }) {
                            self.bateria.on_pong();
                            if let Some(medido) = self.cliente.as_ref().and_then(Client::rtt) {
                                let micros = u64::try_from(medido.as_micros()).unwrap_or(u64::MAX);
                                self.rtt
                                    .store(micros.max(1), std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        // A despedida que não é queda — ver `a_sessao_acabou_aqui`.
                        // Lida aqui e agida **depois** do aviso, logo abaixo:
                        // quem desenha a tela precisa da mensagem para saber
                        // dizer «você foi expulso», e um `return` antes dela
                        // trocaria a frase certa por um fim mudo.
                        let despedida = match &mensagem {
                            ServerMessage::Disconnecting { reason }
                                if a_sessao_acabou_aqui(*reason) =>
                            {
                                Some(*reason)
                            }
                            _ => None,
                        };
                        // Antes de o aviso sair, porque a tela é a única coisa
                        // desta casa que **age** sobre uma mensagem em vez de
                        // repassá-la: é aqui que a transmissão ganha nome e a
                        // bomba nasce.
                        self.a_tela_ouviu(&mensagem);
                        self.o_tunel_ouviu(&mensagem);
                        // A máquina **consome** o que é dela: o pedido vira
                        // aviso próprio (ou recusa, sem tela), e a resposta de
                        // um pedido da máquina não chega à casca como um
                        // `ModReply` que ela não pediu.
                        if let Some(mensagem) = self.a_maquina_ouviu(mensagem).await {
                            let _ = self.avisos.send(Aviso::Mensagem(Box::new(mensagem)));
                        }
                        if let Some(reason) = despedida {
                            tracing::info!(?reason, "o servidor acabou com esta sessão");
                            return self.encerrar(Motivo::Moderado(reason)).await;
                        }
                    }
                    // O fluxo caiu. Não é o fim da sessão: é o começo da
                    // bateria.
                    Err(erro) => {
                        // **`warn` e não `debug`.** Esta linha é a única que diz
                        // por que uma sessão acabou do lado de quem estava
                        // dentro, e ela estava abaixo do filtro: o log do
                        // cliente ficava **mudo** enquanto a conexão caía.
                        //
                        // Custou um diagnóstico inteiro. Um relato de campo —
                        // «minha sessão some para o host e o compartilhamento
                        // para sozinho, mas no Mac eu ainda estou na call» —
                        // teve de ser cruzado com o log do servidor para se
                        // saber sequer que a conexão havia caído, e o motivo
                        // continuou sem aparecer em lugar nenhum.
                        //
                        // Não é ruidosa: acontece uma vez, e o que vem depois é
                        // a bateria de reconexão.
                        tracing::warn!(%erro, "o enlace caiu");
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
        // Um pedido de quadro-chave que esperava a vez (`pedir_a_chave_da_tela`).
        if self.chave_devida {
            self.pedir_a_chave_da_tela();
        }
        // Antes da bateria, e não depois: o passo pode encerrar a sessão, e uma
        // leitura depois disso seria contra um cliente que já foi embora.
        self.medir_o_caminho();
        // A medida de dor da sonda desde a 0.17: o `Ping` dela, só enquanto a
        // tela transmite (ver `caminho::IdaEVolta`). O cliente espaça e não
        // põe dois no ar.
        if self.tela_viva.is_some() {
            if let Some(cliente) = self.cliente.as_mut() {
                if cliente.pingar_para_a_sonda().await.is_err() {
                    self.cair();
                }
            }
        }
        let agora = self.inicio.elapsed();
        let antes = self.bateria.state();
        let acao = self.bateria.poll(agora);
        // **A segunda porta da bateria, e por muito tempo a única sem porteiro.**
        //
        // [`Motor::cair`] trata a queda que o transporte *avisa*: o fluxo
        // devolve erro e o motor larga a conexão inteira, caminho entre pares
        // junto. Mas há outra porta, e é aqui: três `Ping` sem resposta
        // (`Battery::poll_online`) põem a bateria de pé **por dentro**, e
        // devolvem `Action::Wait` — nenhum erro, nenhuma chamada a `cair`.
        //
        // É a porta que uma queda de verdade usa. Um caminho que some sem
        // avisar — NAT que reescreve, rota que cai, a máquina que dorme — não
        // produz erro de fluxo nenhum: produz silêncio, e silêncio é o que os
        // pings contam. Medido em
        // `a_queda_de_uma_conexao_so_derruba_o_caminho_do_par`, com um relé
        // cortado: sem esta linha, a tarefa de par da conexão morta atravessa a
        // bateria **e a reconexão** inteiras, e 1336 quadros da mídia velha
        // chegam à casca depois de `Reconectado` — por um caminho que o servidor
        // novo não montou e não conhece.
        if matches!(antes, Link::Online)
            && matches!(self.bateria.state(), Link::InternalBattery { .. })
        {
            self.soltar_a_conexao();
        }
        match acao {
            Action::SendPing => {
                if let Some(cliente) = self.cliente.as_mut() {
                    if cliente.send_ping().await.is_err() {
                        self.cair();
                    }
                }
            }
            Action::Reconnect => self.tentar().await,
            Action::EndSession => {
                self.encerrar(Motivo::Descarregou).await;
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

    /// A conexão morreu, e o transporte avisou. Entra na bateria e conta para a
    /// casca.
    fn cair(&mut self) {
        self.soltar_a_conexao();
        let agora = self.inicio.elapsed();
        let antes = self.bateria.state();
        self.bateria.on_connection_lost(agora);
        if antes != self.bateria.state() {
            self.anunciar();
        }
    }

    /// Tudo o que uma conexão perdida leva junto, **sem tocar na bateria**.
    ///
    /// Separado de [`Self::cair`] porque há duas portas para a bateria e as
    /// duas têm de passar por aqui: o erro de fluxo, que chega a `cair`, e três
    /// `Ping` sem resposta, que põem a bateria de pé por dentro de
    /// `Battery::poll` sem erro nenhum — ver o comentário em [`Self::passo`].
    /// Naquela porta a bateria já mudou de estado quando isto corre, e chamar
    /// `on_connection_lost` de novo reiniciaria a contagem dos cinco minutos.
    ///
    /// Idempotente: chamada duas vezes, a segunda não tem o que largar.
    fn soltar_a_conexao(&mut self) {
        // O que a volta precisa saber desta conexão: o `ssrc` dela, para saber
        // se o servidor resgatou o assento, e qual era o confirmado. Os pedidos
        // que estavam no ar não terão resposta. Ver `crate::assento`.
        if let Some(cliente) = self.cliente.take() {
            self.queda = Some(Queda {
                ssrc: cliente.session().ssrc,
                sessao: cliente.session().id,
                confirmada: self.entradas.confirmada(self.voice_room),
            });
        }
        self.entradas.esquecer();
        // Os contadores do `quinn` morrem com a conexão, e a janela aberta
        // contra eles daria uma medida absurda na conexão seguinte. O que se
        // aprendeu sobre a casa desta pessoa fica: quem cai e volta em cinco
        // segundos volta para o mesmo cano.
        self.caminho.esquecer_a_conexao();
        // **Uma bomba que sobrevive ao enlace é uma thread codificando para uma
        // conexão morta**, e ela não pararia sozinha: a captura continua
        // entregando quadros e `escoar` só descobre a queda no próximo fluxo
        // que não abre. Além do custo, é o indicador de gravação do macOS aceso
        // sobre uma transmissão que ninguém está recebendo.
        //
        // O pedido morre junto, e é a mesma decisão de [`Self::lembrar`]: uma
        // transmissão que voltasse sozinha depois de cinco minutos de bateria
        // poria a tela de alguém no ar sem que ninguém tivesse apertado nada.
        self.tela_pedida = None;
        self.parar_a_tela();
        self.largar_o_caminho_entre_pares();
        // **Sem quem hospeda, quem joga pela malha continua jogando** (API 8,
        // «A reserva», último item). O túnel que anda pela estrela acaba com
        // `AnfitriaoSaiu` — quem passava por ele caiu, e a tela diz por quê; o
        // que anda pela ligação direta segue, e a máquina segue servindo quem
        // está ligado a ela pela malha. Nada novo abre sem ele: concessão e
        // apresentação são dele. Na volta, cada túnel que seguiu pede entrada
        // de novo (`reentrar_nos_tuneis`). Antes desta regra, todo túnel morria
        // aqui com `ServidorSaiu`, inclusive o direto.
        self.tuneis.sem_anfitriao();
        self.lado_da_maquina.soltar();
    }

    /// O caminho entre pares não sobrevive à conexão que o montou.
    ///
    /// # Por que a queda derruba isto, e não só a saída
    ///
    /// [`Motor::encerrar`] não precisa disto: `rodar` devolve logo depois, o
    /// `Motor` é solto, e [`TarefasDePar`] cancela tudo ao ser solto junto.
    /// **[`Motor::cair`] é o caso em que o motor sobrevive ao corte** — a
    /// sessão entra na bateria e volta noutra conexão —, e é o único em que
    /// alguém tem de pedir.
    ///
    /// E tem de pedir porque nada daquele caminho vale para a conexão nova. A
    /// nomeação que pôs estas tarefas de pé foi feita pela sessão que morreu, e
    /// o servidor já a apagou do lado dele (`Pares::saiu`); as tarefas discam
    /// pela ponta QUIC daquele cliente, que morreu com ele. O que sobra é
    /// imagem chegando por um caminho que ninguém mais reconhece.
    ///
    /// # A fila é trocada, e não esvaziada
    ///
    /// `resultados_do_par` não tem fundo, e o braço que o lê em [`Motor::rodar`]
    /// **só existe quando há cliente**: durante a bateria tudo o que as tarefas
    /// disseram fica parado ali. A primeira volta do laço depois da reconexão
    /// entregaria esses relatos à conexão nova — um `ParFalhou` de uma nomeação
    /// que morreu com a sessão anterior, mandando o servidor desfazer um
    /// caminho que ele acabou de montar para a substituta.
    ///
    /// Trocar o par de pontas, em vez de drenar a fila, é o que fecha a corrida
    /// junto: uma tarefa abortada ainda pode escrever entre o pedido de
    /// cancelamento e o `await` em que ela morre, e drenar antes disso deixaria
    /// esse relato passar. Com as pontas trocadas ela escreve para um recebedor
    /// que já não existe, e o `send` falha — que é o que se quer dela.
    ///
    /// O que vier **depois** da queda continua chegando: quem for criado da
    /// reconexão em diante clona a ponta nova.
    fn largar_o_caminho_entre_pares(&mut self) {
        self.tarefas_de_par.largar_tudo();
        // O repasse ao par morre com elas: o `Sender` largado fecha o canal, e
        // quem estivesse escrevendo termina o fluxo direito em vez de ficar
        // parado num canal que ninguém mais alimenta. Sem isto, o punho do
        // repasse atravessaria a queda apontando para um par de outra sessão.
        self.repasse.desligar();
        let (tx, rx) = mpsc::unbounded_channel();
        self.resultados_do_par_tx = tx;
        self.resultados_do_par = rx;
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
        // **Uma ponta nova por volta, desta sessão só** (R5): a reconexão de
        // uma sessão nunca fecha o socket de outra. A volta é ao endereço que
        // atendeu, e o degrau 4 vai junto quando ele falha: o caminho que o
        // anfitrião furou era para a porta anterior.
        let resultado = voltar(
            &self.destino,
            self.bilhete.as_ref(),
            &self.chave,
            &self.pins,
        )
        .await;
        let agora = self.inicio.elapsed();
        match resultado {
            Ok(mut cliente) => {
                // Restaurar antes de anunciar. Uma casca que recebesse
                // "reconectado" e perguntasse a sala de voz antes de ele existir veria
                // uma sala vazia e acharia que perdeu gente.
                //
                // As quatro chamadas aqui dentro escreviam com `let _ =`: se o
                // envio falhasse — a mensagem nem chegava a sair, porque o
                // aperto de mão desta tentativa acabou de terminar e a conexão
                // já morreu de novo —, o código adiante seguia como se tivesse
                // dado certo. `self.cliente` virava `Some`, `Aviso::Reconectado`
                // saía, e a casca se dava por dentro da sala de voz sem que o
                // servidor jamais tivesse recebido o pedido. É o mecanismo do
                // §2.2: o servidor confirma por silêncio, e aqui nem o silêncio
                // vinha do servidor — vinha do cliente jogando fora o próprio
                // erro. Falhar aqui agora conta como a mesma coisa que a
                // tentativa de `connect_por` ter falhado: a bateria continua
                // correndo, e a próxima tentativa tenta de novo.
                // O assento que o servidor devolveu, se devolveu: o mesmo
                // `ssrc` da conexão que caiu. Ver `crate::assento`.
                let resgatada = self.queda.and_then(|queda| {
                    crate::assento::assento_resgatado(
                        Some((queda.ssrc, queda.sessao)),
                        (cliente.session().ssrc, cliente.session().id),
                        queda.confirmada,
                    )
                });
                // O assento desta conexão, pela mesma conta que a casca faz: o
                // resgatado, e por cima dele o pedido que `restabelecer` refaz —
                // a entrada, no ar até o `Pong` marcado do número dela, ou a
                // saída. Montado antes, porque o número vai no `Ping`.
                let mut entradas = crate::assento::EntradasPendentes::default();
                let mut assento = resgatada;
                let numero = match self.voice_room {
                    Some(sala) => Some(entradas.pedir(&mut assento, sala)),
                    None => {
                        entradas.sair(&mut assento);
                        None
                    }
                };
                if let Err(erro) = restabelecer(
                    &mut cliente,
                    self.voice_room.zip(numero),
                    resgatada,
                    self.linha,
                    self.muted,
                    self.isolamento,
                )
                .await
                {
                    tracing::warn!(
                        ?erro,
                        "a reconexão completou o aperto de mão mas não restabeleceu \
                         o estado de antes da queda; contando como tentativa que não deu"
                    );
                    self.bateria.on_reconnect_failed(agora);
                    self.anunciar();
                    return;
                }

                self.entradas = entradas;
                self.voice_room = assento;
                self.queda = None;
                let sessao = cliente.session().clone();
                let media = cliente.media();
                self.protocolo.store(
                    cliente.versao_negociada(),
                    std::sync::atomic::Ordering::Relaxed,
                );
                self.cliente = Some(cliente);
                self.bateria.on_reconnected();
                self.reentrar_nos_tuneis().await;
                // A sessão anterior já foi apagada de `Pares` (`Pares::saiu`,
                // no servidor) quando ela caiu — sem redeclarar aqui, esta
                // pessoa reconecta sem identidade nenhuma registrada até a
                // próxima vez que este método for chamado por acaso. Ver o
                // doc de `Motor::declarar_identidade_de_par`.
                //
                // **Com a escolha de emprestar, e não com `false` fixo.** Quem
                // optou por emprestar a subida sairia da malha na primeira
                // queda de rede, sem nada na tela mudando — ver o doc de
                // `Motor::consentimento`.
                self.declarar_identidade_de_par(self.consentimento).await;

                // A sala vai junto porque foi refeita aqui, por `restabelecer`,
                // e só este motor sabe disso. Ver `Aviso::Reconectado::voice_room`.
                let _ = self.avisos.send(Aviso::Reconectado {
                    media: Box::new(media),
                    sessao: Box::new(sessao),
                    voice_room: self.voice_room,
                });
            }
            // Uma recusa não melhora com insistência, e insistir contra uma
            // credencial rejeitada é a diferença entre reconectar e martelar.
            //
            // **E acaba dizendo qual foi.** A tentativa que falha e deixa a
            // bateria correndo tem o `debug!` do braço de baixo; esta, que
            // acaba com a sessão, saía sem linha nenhuma no `seele.log`.
            Err(erro) if !vale_insistir(&erro) => {
                tracing::warn!(?erro, "a volta da bateria foi recusada, e a sessão acaba");
                self.encerrar(Motivo::Recusado(erro)).await;
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
        // Especial, e antes do empréstimo de `cliente` logo abaixo: declarar
        // identidade precisa do `&mut self` inteiro (identidade **e**
        // cliente — ver `Motor::declarar_identidade_de_par`), e o resto deste
        // método já empresta só `self.cliente`. Os dois empréstimos não
        // convivem no mesmo escopo.
        //
        // **A opção de emprestar é lembrada, e é ela que vai no fio nas duas.**
        // `DeclararIdentidadeDePar` é chamada na conexão e em cada reconexão,
        // e passava `false` fixo; com o comando de emprestar existindo, um
        // `false` fixo aqui desfaria em silêncio, na primeira queda de rede, a
        // escolha que a pessoa fez — ela continuaria na malha na tela dela e
        // fora dela no servidor.
        match comando {
            Comando::DeclararIdentidadeDePar => {
                self.declarar_identidade_de_par(self.consentimento).await;
                return;
            }
            Comando::ConsentirNoCaminhoEntrePares(novo) => {
                self.passar_a_consentir(novo).await;
                return;
            }
            Comando::EntrarNoTunel {
                mod_id,
                nome,
                resposta,
            } => {
                self.entrar_no_tunel(mod_id, nome, resposta).await;
                return;
            }
            Comando::VerboDaMaquina(verbo) => {
                self.verbo_da_maquina(verbo).await;
                return;
            }
            Comando::PedidoDaMaquina {
                mod_id,
                servidor,
                payload,
                resposta,
            } => {
                self.pedir_a_maquina(mod_id, servidor, payload, resposta)
                    .await;
                return;
            }
            Comando::SairDoTunel { mod_id, nome } => {
                self.sair_do_tunel(&mod_id, &nome).await;
                return;
            }
            Comando::SairDosTuneisDoMod { mod_id } => {
                for nome in self.tuneis.sair_do_mod(&mod_id) {
                    self.mandar_verbo_de_tunel(ClientMessage::SairDoTunel {
                        mod_id: mod_id.clone(),
                        nome,
                    })
                    .await
                    .ok();
                }
                return;
            }
            Comando::ServirTunel {
                mod_id,
                nome,
                protocolo,
                porta,
                resposta,
            } => {
                self.servir_tunel(mod_id, nome, protocolo, porta, resposta)
                    .await;
                return;
            }
            Comando::PararTunel { mod_id, nome } => {
                if self.tuneis.parar_servico(&mod_id, &nome) {
                    self.mandar_verbo_de_tunel(ClientMessage::PararTunel { mod_id, nome })
                        .await
                        .ok();
                }
                return;
            }
            _ => {}
        }
        // O número do pedido de entrada que `lembrar` acabou de pôr no ar, se
        // este comando é um: vai no carimbo do `Ping` marcado.
        let numero_da_entrada = self.entradas.ultimo_numero().unwrap_or_default();
        let Some(cliente) = self.cliente.as_mut() else {
            return;
        };
        // O nome, sem os valores, para a recusa local dizer qual foi; e, se é
        // uma entrada, o número dela, que a recusa tem de desfazer. Lidos antes
        // do `match`, que consome o comando.
        let nome = comando.nome();
        let entrada =
            matches!(comando, Comando::EntrarNaVoiceRoom(..)).then_some(numero_da_entrada);
        let resultado = match comando {
            // Nada a mandar ao servidor: é estado desta máquina.
            Comando::LembrarCaminho(_) => Ok(()),
            // O pedido, e logo atrás o `Ping` marcado: o `Pong` dele é o que
            // diz que o servidor decidiu. Ver `crate::assento`.
            Comando::EntrarNaVoiceRoom(voice_room, password) => {
                match cliente.enter_voice_room(voice_room, password).await {
                    Ok(()) => cliente.send_ping_de_entrada(numero_da_entrada).await,
                    Err(erro) => Err(erro),
                }
            }
            Comando::SairDaVoiceRoom => cliente.leave_voice_room().await,
            Comando::AbrirLinha(linha) => cliente.join_channel(linha).await,
            Comando::Dizer {
                linha,
                corpo,
                responde_a,
                id,
            } => cliente.send_message(linha, &corpo, responde_a, id).await,
            Comando::ImagemDeMod {
                pedido,
                bytes,
                resposta,
            } => {
                let transferencias = cliente.transfers();
                tokio::spawn(async move {
                    // O prazo e o texto moram no `seele-sessao`
                    // (`transferencia`): a página do navegador espera o mesmo.
                    let resultado = tokio::time::timeout(
                        std::time::Duration::from_secs(
                            seele_sessao::transferencia::PRAZO_DA_IMAGEM_DE_MOD_S,
                        ),
                        transferencias.imagem_de_mod(pedido, &bytes),
                    )
                    .await;
                    let resultado = match resultado {
                        Ok(r) => r.map_err(|e| e.to_string()),
                        Err(_) => Err(seele_sessao::transferencia::PRAZO_ESGOTADO.to_owned()),
                    };
                    let _ = resposta.send(resultado);
                });
                Ok(())
            }
            Comando::ModRequest {
                request,
                id,
                channel,
                payload,
            } => cliente.mod_request(request, id, channel, payload).await,
            Comando::AcaoDeMod {
                request,
                id,
                channel,
                acao,
                payload,
            } => {
                cliente
                    .acao_de_mod(request, id, channel, acao, payload)
                    .await
            }
            Comando::Historico { linha, limite } => {
                cliente.fetch_history(linha, None, limite).await
            }
            Comando::HistoricoComCerca {
                linha,
                cursor,
                limite,
                carimbo,
            } => match cliente.fetch_history(linha, cursor, limite).await {
                Ok(()) => cliente.send_ping_marcado(carimbo).await,
                Err(erro) => Err(erro),
            },
            Comando::Muted(ligado) => cliente.set_muted(ligado).await,
            Comando::Isolamento(ligado) => cliente.set_total_isolation(ligado).await,
            Comando::CriarVoiceRoom {
                nome,
                limite,
                linha,
            } => cliente.create_voice_room(&nome, limite, linha).await,
            Comando::CriarLinha { nome } => cliente.create_channel(&nome).await,
            Comando::RenomearVoiceRoom { voice_room, nome } => {
                cliente.rename_voice_room(voice_room, &nome).await
            }
            Comando::RenomearLinha { linha, nome } => cliente.rename_channel(linha, &nome).await,
            Comando::RenomearServer { nome } => cliente.rename_server(&nome).await,
            Comando::IconeDoServer { icone } => cliente.set_server_icon(icone).await,
            Comando::MinhaImagem { icone } => cliente.set_person_icon(icone).await,
            Comando::MeuApelido { nome } => cliente.set_nickname(nome).await,
            Comando::GravarPerfil(mensagem) => cliente.gravar_perfil(mensagem).await,
            Comando::TirarImagemDoPerfil { lado } => cliente.tirar_imagem_do_perfil(lado).await,
            Comando::Expulsar { pessoa } => cliente.kick_person(pessoa).await,
            Comando::Cargo(pedido) => cliente.cargo(pedido).await,
            Comando::Banir {
                pessoa,
                motivo,
                expira_em,
            } => {
                cliente
                    .ban_person(pessoa, motivo.as_deref(), expira_em)
                    .await
            }
            Comando::RemoverMensagem { mensagem } => cliente.remove_message(mensagem).await,
            Comando::EditarMensagem { mensagem, corpo } => {
                cliente.edit_message(mensagem, &corpo).await
            }
            Comando::Desbanir { pessoa } => cliente.unban_person(pessoa).await,
            Comando::ListarBanidos => cliente.list_bans().await,
            Comando::RelatarRecepcao(relato) => cliente.relatar_recepcao(relato).await,
            Comando::MoverPersono { pessoa, voice_room } => {
                cliente.move_person(pessoa, voice_room).await
            }
            Comando::ApagarVoiceRoom { voice_room } => cliente.delete_voice_room(voice_room).await,
            Comando::ApagarLinha { linha } => cliente.delete_channel(linha).await,
            Comando::PesarLinha { linha } => cliente.weigh_channel(linha).await,
            // Tratados acima, antes deste empréstimo de `cliente` — nunca
            // chegam aqui de verdade. `match` continua exaustivo porque o
            // `enum` inteiro é um só, e um braço a menos aqui quebraria a
            // primeira vez que `Comando` ganhasse mais uma variante.
            Comando::DeclararIdentidadeDePar
            | Comando::ConsentirNoCaminhoEntrePares(_)
            | Comando::EntrarNoTunel { .. }
            | Comando::SairDoTunel { .. }
            | Comando::SairDosTuneisDoMod { .. }
            | Comando::ServirTunel { .. }
            | Comando::PararTunel { .. }
            | Comando::VerboDaMaquina(_)
            | Comando::PedidoDaMaquina { .. } => Ok(()),

            // Numa tarefa própria, e não aqui dentro. Executar vinte megabytes
            // no laço de comandos devolveria, dentro do cliente, exatamente o
            // bloqueio de cabeça de fila que o fluxo próprio existe para
            // evitar: ninguém conseguiria dizer uma frase enquanto o arquivo
            // sobe. `Transfers` é clonável para isto.
            Comando::Anexar(anexo) => {
                let transferencias = cliente.transfers();
                let avisos = self.avisos.clone();
                let id = anexo.id;
                tokio::spawn(async move {
                    let andamento = |feito, total| {
                        let _ = avisos.send(Aviso::Transferencia(Transferencia::Subindo {
                            id,
                            feito,
                            total,
                        }));
                    };
                    let pedido = crate::client::AttachmentRequest {
                        channel: anexo.linha,
                        client_message_id: anexo.id,
                        body: &anexo.corpo,
                        replies_to: anexo.responde_a,
                        path: &anexo.caminho,
                        file_name: &anexo.nome,
                        declared_type: &anexo.tipo,
                    };
                    let fim = match transferencias.send_attachment(&pedido, andamento).await {
                        Ok(crate::client::Sent::Delivered { .. }) => Transferencia::Subiu { id },
                        Ok(crate::client::Sent::Stopped { .. }) => Transferencia::Recusada { id },
                        Ok(crate::client::Sent::Interrupted { .. }) | Err(_) => {
                            Transferencia::Caiu { id }
                        }
                    };
                    let _ = avisos.send(Aviso::Transferencia(fim));
                });
                Ok(())
            }

            Comando::SalvarAnexo { anexo, pasta, nome } => {
                // O parcial é criado **antes** de o pedido sair, por
                // `abrir_parcial`: o nome é conferido de novo, e um que não é
                // só um nome volta como falha aqui, sem byte nenhum pedido ao
                // servidor. O nome final — ou «foto (2).png», se ele estiver
                // tomado — só é dado depois de o hash conferir. Num
                // `spawn_blocking` porque abrir é uma chamada ao disco, e esta
                // fila carrega toda tecla da sessão.
                let criado = tokio::task::spawn_blocking(move || {
                    crate::anexo_no_disco::abrir_parcial(
                        &pasta,
                        &nome,
                        crate::anexo_no_disco::TENTATIVAS,
                    )
                })
                .await;
                let (arquivo, parcial) = match criado {
                    Ok(Ok(criado)) => criado,
                    Ok(Err(erro)) => {
                        tracing::warn!(%anexo, %erro, "não consegui criar o arquivo do anexo; nada foi gravado");
                        let _ = self
                            .avisos
                            .send(Aviso::Transferencia(Transferencia::NaoSalvou { anexo }));
                        return;
                    }
                    Err(erro) => {
                        tracing::warn!(%anexo, %erro, "a tarefa que cria o arquivo do anexo caiu; nada foi gravado");
                        let _ = self
                            .avisos
                            .send(Aviso::Transferencia(Transferencia::NaoSalvou { anexo }));
                        return;
                    }
                };
                let transferencias = cliente.transfers();
                let avisos = self.avisos.clone();
                let pedido = cliente.fetch_attachment(anexo).await;
                tokio::spawn(async move {
                    let andamento = |feito, total| {
                        let _ = avisos.send(Aviso::Transferencia(Transferencia::Baixando {
                            anexo,
                            feito,
                            total,
                        }));
                    };
                    let parcial_em = parcial.caminho().to_path_buf();
                    let recebido = transferencias
                        .receive_attachment(anexo, arquivo, parcial, ESPERA_DE_ANEXO, andamento)
                        .await;
                    let _ = avisos.send(Aviso::Transferencia(fim_do_salvar(
                        anexo,
                        &parcial_em,
                        recebido,
                    )));
                });
                pedido
            }

            // A mesma forma do `SalvarAnexo`, com duas diferenças que são a
            // decisão inteira: os bytes param na memória, e o teto que os
            // limita é o desta janela — não o do disco de quem hospeda.
            Comando::PreverAnexo { anexo, resposta } => {
                let transferencias = cliente.transfers();
                let pedido = cliente.fetch_attachment(anexo).await;
                tokio::spawn(async move {
                    let fim = match transferencias
                        .preview_attachment(anexo, crate::preview::PREVIEW_LIMIT, ESPERA_DE_ANEXO)
                        .await
                    {
                        Ok(crate::client::Previewed::Whole(bytes)) => Previa::Bytes(bytes),
                        Ok(crate::client::Previewed::TooBig { byte_size }) => {
                            Previa::GrandeDemais { tamanho: byte_size }
                        }
                        Err(_) => Previa::NaoVeio,
                    };
                    let _ = resposta.send(fim);
                });
                pedido
            }

            // O pedido é guardado **antes** de o verbo sair, e a ordem
            // importa: a resposta do servidor chega pelo mesmo laço que trouxe
            // este comando, e guardar depois abriria uma janela em que o
            // `ScreenShareStarted` chega e não encontra a escolha da pessoa.
            Comando::CompartilharTela { fonte, limites } => {
                // **A câmera só é pedida a um servidor que a conhece.** A
                // fonte 2 nasceu no protocolo 13, e um servidor do 12 recusa o
                // cabeçalho dela *depois* de ter anunciado a transmissão à
                // sala: ficaria desenhada uma tela que não tem fluxo. Quem
                // chama já conferiu e respondeu com o nome do erro; isto é a
                // parede que não depende de quem chama. O pedido é largado
                // aqui, e a entrada da janela passa a responder «fechada».
                let negociado = cliente.versao_negociada();
                if fonte.origem().desde() > negociado {
                    tracing::warn!(
                        negociado,
                        exige = fonte.origem().desde(),
                        "a câmera foi pedida a um servidor que não a conhece; nada saiu"
                    );
                    return;
                }
                // **No lugar da tela, e não ao lado dela.** Uma pessoa manda
                // um fluxo de imagem por vez, e quem troca é o servidor: o
                // pedido novo toma a vaga, o fim da transmissão anterior é
                // anunciado à sala (e a esta ponta, que mata a bomba dela em
                // `a_tela_ouviu`), e só então a nova ganha nome.
                if self.caminho.estimativa_medida().is_none()
                    && self.caminho.estimativa() == crate::tela::CAMINHO_DA_PROVA_BPS
                {
                    self.caminho =
                        crate::caminho::Sonda::partindo_de(limites.semente_da_sonda_bps());
                }
                self.tela_pedida = Some((fonte, limites));
                cliente.start_screen_share().await
            }

            // Os tetos da pessoa, na transmissão que já existe.
            //
            // Sem verbo para o servidor: os tetos são desta ponta. O que atravessa
            // a rede é o resultado deles — a resolução no cabeçalho do fluxo
            // novo —, e não a escolha em si. `TelaEmCurso::pedido` é lido do
            // que está guardado aqui, e é por isso que ele é escrito antes de a
            // bomba responder: a coluna «pedido» é a escolha, e ela vale desde
            // o aperto, mesmo que o degrau demore um quadro a acompanhar.
            Comando::AjustarLimitesDaTela { limites } => {
                let Some(viva) = self.tela_viva.as_mut() else {
                    // Ninguém está transmitindo. Não é erro: é alguém que
                    // apertou APLICAR na janela um instante depois de a
                    // transmissão cair sozinha.
                    return;
                };
                viva.limites = limites;
                let (resolucao, cadencia) = (limites.resolucao, limites.cadencia);
                viva.produtor.escolha(resolucao, cadencia);
                // E o teto de novo, porque a banda escolhida é uma das pernas
                // dele: sem esta linha, mexer só na banda não mudaria nada.
                self.reconferir_o_teto();
                return;
            }

            // A bomba morre aqui, e não quando o `ScreenShareStopped` voltar:
            // quem apertou parar não deve continuar capturando enquanto uma
            // volta de rede acontece, e o servidor pode nunca responder.
            Comando::PararDeCompartilhar => {
                self.tela_pedida = None;
                // Pelos campos e não por [`Self::parar_a_tela`]: `cliente` é um
                // empréstimo de `self.cliente` que ainda está vivo na linha
                // abaixo, e um método pegaria `self` inteiro.
                self.espectadores = 0;
                self.caminho.esquecer_a_conexao();
                matar(self.tela_viva.take());
                cliente.stop_screen_share().await
            }

            Comando::Assistir { tela, quero } => {
                if quero {
                    cliente.watch_screen(tela).await
                } else {
                    // **Quem para de assistir para de receber, e o caminho do
                    // par cai junto.** Avisar só o servidor deixava metade: a
                    // tarefa de [`Motor::assistir_por_par`] continuava lendo do
                    // par, e a imagem seguia chegando por baixo do pedido de
                    // parar — gastando a subida de quem empresta com uma janela
                    // fechada. E o `ParFalhou` de rotina que aquele fluxo
                    // acabaria mandando pedia a tela de volta ao servidor.
                    //
                    // `abort` e não um sinal cooperativo: a tarefa passa a vida
                    // parada num `read` do par, e ela não tem ponto onde
                    // conferir um pedido de parada. É por isso que ela é **uma
                    // tarefa só** — ver [`ler_a_tela_alheia`]: enquanto a
                    // leitura acontecia numa segunda tarefa, abortar esta
                    // deixava aquela lendo.
                    self.tarefas_de_par.parar_de_assistir(tela);
                    cliente.unwatch_screen(tela).await
                }
            }
            Comando::PedirQuadroChave { tela } => cliente.request_key_frame(tela).await,

            Comando::Sair => return,
        };
        // **Recusa, e não queda** (M2 da revisão do P61). Um campo maior que o
        // teto do fio — um apelido, um nome de canal, a senha de uma sala —
        // fazia o `encode` falhar, e este braço tratava como enlace caído: a
        // sessão, a voz e o resto iam junto, por um nome comprido.
        if let Err(erro) = resultado {
            // **Um verbo que o servidor negociado não conhece não é queda**: a
            // `Connection` já devolveu `ServidorAntigo` à casca antes de o
            // comando chegar; isto é a segunda cerca, para uma volta que
            // renegociou um número menor. Nada saiu, a sessão segue, e o
            // registro diz.
            if let Some(antigo) = erro.downcast_ref::<crate::client::ServidorAntigo>() {
                tracing::warn!(
                    comando = nome,
                    motivo = ?antigo.0,
                    "o comando não saiu: o servidor negociado não conhece o pedido; a sessão segue"
                );
                return;
            }
            match recusa_local(&erro) {
                Some(recusa) => self.recusar_aqui(nome, recusa, entrada),
                None => self.cair(),
            }
        }
    }

    /// Um comando que não saiu porque um campo dele não cabe no fio.
    ///
    /// Vai ao `seele.log` com o comando e o campo, **sem o valor** — pode ser a
    /// senha de uma sala. A janela ainda não a mostra: a frase é de outro lote,
    /// e o caminho mais barato é a casca conferir o tamanho antes de mandar,
    /// como o envio de mensagem já faz.
    ///
    /// A entrada numa sala com senha grande demais (`entrada`, o número dela)
    /// é desfeita aqui **pelo número**, pela porta de quem pediu
    /// ([`crate::assento::EntradasPendentes::desfazer`]): sem isso, a volta a
    /// refaria a cada queda. Não pela recusa do servidor, que desfaz o pedido
    /// mais antigo no ar — com outra entrada no ar, era essa que se desfazia
    /// (R1-M4 da revisão do P61).
    ///
    /// A casca não recebe nada daqui: a do app confere a senha antes de pedir
    /// (`seele_ffi::Connection::enter_voice_room`), e o `Room` dela nunca senta
    /// a pessoa numa entrada que não vai sair.
    fn recusar_aqui(
        &mut self,
        comando: &'static str,
        recusa: &seele_proto::control::ControlError,
        entrada: Option<u64>,
    ) {
        tracing::warn!(
            comando,
            %recusa,
            "o comando não saiu: um campo dele não cabe no fio; a recusa é local, e o enlace segue"
        );
        if let Some(numero) = entrada {
            self.entradas.desfazer(&mut self.voice_room, numero);
        }
    }

    // ------------------------------------------------------------------ túneis

    /// Um verbo do túnel para o servidor, conferido contra o número negociado.
    async fn mandar_verbo_de_tunel(
        &mut self,
        mensagem: ClientMessage,
    ) -> Result<(), RecusaDeTunel> {
        let Some(cliente) = self.cliente.as_mut() else {
            return Err(RecusaDeTunel::SemResposta);
        };
        match cliente.enviar_verbo_do_tunel(&mensagem).await {
            Ok(()) => Ok(()),
            Err(erro) => {
                if let crate::client::NaoSaiu::Escrita(porque) = &erro {
                    tracing::warn!(%porque, "o fluxo de controle não aceitou o verbo do túnel");
                    self.cair();
                }
                Err(recusa_do_verbo(&erro))
            }
        }
    }

    // ------------------------------------------------------------- a máquina

    /// Um verbo da máquina para o servidor: conferido contra o número
    /// negociado (um servidor 11 desalinharia o fluxo com uma variante que não
    /// conhece) e contra o teto dos campos, antes de escrever.
    ///
    /// # Errors
    ///
    /// O nome do porquê, para o log de quem chamou: `sem-conexao`,
    /// `servidor-antigo`, `campo-fora-do-teto` ou `escrita`. Só `escrita` é
    /// queda.
    async fn mandar_verbo_da_maquina(
        &mut self,
        mensagem: &ClientMessage,
    ) -> Result<(), &'static str> {
        use seele_proto::control::Validate as _;
        let Some(cliente) = self.cliente.as_mut() else {
            return Err("sem-conexao");
        };
        // Antes do fio: `enviar_verbo_do_tunel` trata toda falha de escrita
        // como queda, e um payload grande demais é recusa local, não rede.
        if mensagem.validate().is_err() {
            return Err("campo-fora-do-teto");
        }
        match cliente.enviar_verbo_do_tunel(mensagem).await {
            Ok(()) => Ok(()),
            Err(crate::client::NaoSaiu::ServidorAntigo) => Err("servidor-antigo"),
            Err(crate::client::NaoSaiu::Escrita(porque)) => {
                tracing::warn!(%porque, "o fluxo de controle não aceitou o verbo da máquina");
                self.cair();
                Err("escrita")
            }
        }
    }

    /// O aceite, a recusa ou a parada que a casca decidiu.
    async fn verbo_da_maquina(&mut self, verbo: ClientMessage) {
        let (mod_id, servidor, qual) = match &verbo {
            ClientMessage::AceiteDeMaquina {
                mod_id, servidor, ..
            } => (mod_id.clone(), servidor.clone(), "aceite"),
            ClientMessage::RecusaDeMaquina {
                mod_id,
                servidor,
                motivo,
            } => (
                mod_id.clone(),
                servidor.clone(),
                seele_proto::maquina::nome_da_recusa(motivo),
            ),
            ClientMessage::MaquinaParada {
                mod_id,
                servidor,
                motivo,
            } => (
                mod_id.clone(),
                servidor.clone(),
                seele_proto::maquina::nome_da_parada(motivo),
            ),
            _ => return,
        };
        // A casca respondeu: o mesmo pedido, se vier de novo, é outra
        // pergunta — e tem de chegar a ela.
        self.lado_da_maquina.esquecer(&mod_id, &servidor);
        match self.mandar_verbo_da_maquina(&verbo).await {
            Ok(()) => {
                tracing::info!(mod_id, servidor, verbo = qual, "máquina: verbo ao servidor");
            }
            Err(porque) => {
                tracing::warn!(
                    mod_id,
                    servidor,
                    verbo = qual,
                    motivo = porque,
                    "máquina: o verbo não saiu"
                );
            }
        }
    }

    /// Um pedido da máquina à metade de servidor: o número sai da faixa deste
    /// motor, e a resposta é montada em [`Self::a_maquina_ouviu`].
    async fn pedir_a_maquina(
        &mut self,
        mod_id: String,
        servidor: String,
        payload: String,
        resposta: tokio::sync::oneshot::Sender<Result<String, FalhaDoPedidoDaMaquina>>,
    ) {
        // Quem desistiu de esperar (o prazo venceu do lado do `Enlace`) não
        // precisa de lugar: sem isto, um servidor que nunca responde
        // acumularia pedidos aqui pela sessão inteira.
        self.lado_da_maquina
            .no_ar
            .retain(|_, pedido| !pedido.resposta.is_closed());
        let request = self.lado_da_maquina.proximo_numero();
        let mensagem = ClientMessage::PedidoDaMaquina {
            request,
            mod_id: mod_id.clone(),
            servidor: servidor.clone(),
            payload,
        };
        self.lado_da_maquina.no_ar.insert(
            request,
            PedidoNoAr {
                mod_id: mod_id.clone(),
                servidor: servidor.clone(),
                partes: Vec::new(),
                resposta,
            },
        );
        if let Err(porque) = self.mandar_verbo_da_maquina(&mensagem).await {
            tracing::warn!(
                mod_id,
                servidor,
                request,
                motivo = porque,
                "máquina: o pedido à metade de servidor não saiu"
            );
            if let Some(pedido) = self.lado_da_maquina.no_ar.remove(&request) {
                let _ = pedido
                    .resposta
                    .send(Err(FalhaDoPedidoDaMaquina::SemResposta));
            }
        }
    }

    /// O que uma mensagem do servidor faz com a máquina. Devolve a mensagem
    /// quando ela não é da máquina, para seguir à casca como sempre.
    async fn a_maquina_ouviu(&mut self, mensagem: ServerMessage) -> Option<ServerMessage> {
        match mensagem {
            ServerMessage::PedidoDeMaquina {
                mod_id,
                hash,
                servidor,
                pedido_por,
                oferta,
                papel,
            } => {
                if !self.lado_da_maquina.com_tela() {
                    // Desvio 5: sem tela não há quem leia a oferta. Recusar na
                    // hora é o que deixa a metade de servidor saber
                    // (`MaquinaRecusou`), em vez de esperar quem não vem.
                    let recusa = ClientMessage::RecusaDeMaquina {
                        mod_id: mod_id.clone(),
                        servidor: servidor.clone(),
                        motivo: seele_proto::maquina::MotivoDaRecusaDeMaquina::SemTela,
                    };
                    let saiu = self.mandar_verbo_da_maquina(&recusa).await;
                    tracing::info!(%mod_id, %servidor, %pedido_por, motivo = "sem-tela", saiu = ?saiu, "máquina: pedido recusado sozinho: este enlace não tem tela");
                    return None;
                }
                let resumo =
                    seele_proto::maquina::resumo_em_hex(&seele_proto::maquina::resumo_da_oferta(
                        &mod_id, &hash, &servidor, papel, &oferta,
                    ));
                if !self.lado_da_maquina.avisar(&mod_id, &servidor, &resumo) {
                    // O servidor pode mandar o mesmo pedido duas vezes na
                    // mesma conexão; a casca já o tem e ainda não respondeu.
                    tracing::debug!(%mod_id, %servidor, %resumo, "máquina: o mesmo pedido de novo; a casca já o tem");
                    return None;
                }
                tracing::info!(%mod_id, %servidor, %pedido_por, papel = papel.nome(), %resumo, "máquina: pedido à casca");
                let _ = self.avisos.send(Aviso::PedidoDeMaquina {
                    mod_id,
                    hash,
                    servidor,
                    pedido_por,
                    oferta,
                    papel,
                    resumo,
                });
                None
            }
            ServerMessage::AvisoParaMaquina {
                mod_id,
                servidor,
                carga,
            } => {
                let _ = self.avisos.send(Aviso::AvisoParaMaquina {
                    mod_id,
                    servidor,
                    carga,
                });
                None
            }
            ServerMessage::MaquinaDispensada {
                mod_id,
                servidor,
                motivo,
            } => {
                tracing::info!(%mod_id, %servidor, motivo = seele_proto::maquina::nome_da_dispensa(&motivo), "máquina: dispensada pelo servidor");
                self.lado_da_maquina.esquecer(&mod_id, &servidor);
                if motivo == seele_proto::maquina::MotivoDaDispensa::SemAceite {
                    // A resposta do servidor a um `PedidoDaMaquina` de quem
                    // não é a máquina: não vem `ModReply` nenhum depois dela.
                    self.lado_da_maquina.falhar(
                        &mod_id,
                        &servidor,
                        FalhaDoPedidoDaMaquina::SemAceite,
                    );
                }
                let _ = self.avisos.send(Aviso::MaquinaDispensada {
                    mod_id,
                    servidor,
                    motivo,
                });
                None
            }
            ServerMessage::ModReply {
                request,
                part,
                total,
                payload,
            } if self.lado_da_maquina.no_ar.contains_key(&request) => {
                self.lado_da_maquina.parte(request, part, total, payload);
                None
            }
            outra => Some(outra),
        }
    }

    /// **Quem hospeda voltou**: cada túnel que seguiu pela malha sem ele pede
    /// entrada de novo, para ele conceder (e conferir a permissão) outra vez.
    /// A concessão troca a estrela do túnel e mantém a porta; a recusa o acaba
    /// com o motivo (ver `Tuneis::reentrar`).
    async fn reentrar_nos_tuneis(&mut self) {
        for (mod_id, nome) in self.tuneis.reentrar() {
            tracing::info!(%mod_id, tunel = %nome, "túnel: quem hospeda voltou; o túnel que seguiu pela malha pede entrada de novo");
            let chave = (mod_id.clone(), nome.clone());
            if let Err(recusa) = self
                .mandar_verbo_de_tunel(ClientMessage::EntrarNoTunel { mod_id, nome })
                .await
            {
                self.tuneis.recusar_pedido(&chave, recusa);
            }
        }
    }

    /// `EntrarNoTunel`: o pedido sai, e a resposta é a porta local quando o
    /// `TunelConcedido` chegar (ver `o_tunel_ouviu`).
    async fn entrar_no_tunel(
        &mut self,
        mod_id: String,
        nome: String,
        resposta: tokio::sync::oneshot::Sender<Result<crate::tunel::PortaLocal, RecusaDeTunel>>,
    ) {
        let chave = (mod_id.clone(), nome.clone());
        // Um túnel já aberto (ou já pedido) responde sem verbo nenhum: ver
        // `Tuneis::pedir_entrada`.
        if !self.tuneis.pedir_entrada(chave.clone(), resposta) {
            return;
        }
        if let Err(recusa) = self
            .mandar_verbo_de_tunel(ClientMessage::EntrarNoTunel { mod_id, nome })
            .await
        {
            self.tuneis.recusar_pedido(&chave, recusa);
        }
    }

    async fn sair_do_tunel(&mut self, mod_id: &str, nome: &str) {
        if self.tuneis.sair(mod_id, nome) {
            self.mandar_verbo_de_tunel(ClientMessage::SairDoTunel {
                mod_id: mod_id.to_owned(),
                nome: nome.to_owned(),
            })
            .await
            .ok();
        }
    }

    async fn servir_tunel(
        &mut self,
        mod_id: String,
        nome: String,
        protocolo: ProtocoloDeTunel,
        porta: u16,
        resposta: tokio::sync::oneshot::Sender<Result<(), RecusaDeTunel>>,
    ) {
        let chave = (mod_id.clone(), nome.clone());
        self.tuneis
            .pedir_servico(chave.clone(), protocolo, porta, resposta);
        if let Err(recusa) = self
            .mandar_verbo_de_tunel(ClientMessage::ServirTunel {
                mod_id,
                nome,
                protocolo,
                porta,
            })
            .await
        {
            self.tuneis.recusar_pedido(&chave, recusa);
        }
    }

    /// O que uma mensagem do servidor faz com os túneis de jogo.
    fn o_tunel_ouviu(&mut self, mensagem: &ServerMessage) {
        if !matches!(
            mensagem,
            ServerMessage::TunelConcedido { .. }
                | ServerMessage::TunelRecusado { .. }
                | ServerMessage::TunelEncerrado { .. }
                | ServerMessage::AlcanceTunelPor { .. }
                | ServerMessage::SirvaTunelPara { .. }
                | ServerMessage::TunelRetirado { .. }
        ) {
            return;
        }
        let estrela = self.cliente.as_ref().map(Client::conexao);
        let ponta = self.ponta_de_pares();
        let resultados = self.resultados_do_par_tx.clone();
        let enviar: crate::tunel::Enviar = Arc::new(move |mensagem| {
            let _ = resultados.send(ResultadoDoPar::ParaOServidor(mensagem));
        });
        self.tuneis.tratar(
            mensagem,
            estrela.as_ref(),
            ponta.as_ref(),
            &enviar,
            PRAZO_DO_PAR,
        );
    }

    // ------------------------------------------------------------------ tela

    /// O que uma mensagem do servidor faz com a tela desta pessoa.
    fn a_tela_ouviu(&mut self, mensagem: &ServerMessage) {
        match *mensagem {
            // A faixa da **própria** voz, que é a perna que faltava no teto do
            // §3.2. O servidor calcula a taxa de cada pessoa e a devolve aqui uma
            // vez por segundo; o que faltava era guardar a sua.
            //
            // `SignalBand::of` e não um limiar escrito aqui: a conta de onde
            // começa cada faixa é do `seele-proto`, e duas cópias dela
            // divergiriam no dia em que uma mudasse.
            ServerMessage::PersonState(ref estado) => {
                let eu = self.cliente.as_ref().map(|c| c.session().person);
                if let Some(nova) = faixa_nova(self.faixa, estado, eu) {
                    {
                        self.faixa = nova;
                        // Só quando muda, e só se houver tela: a taxa chega uma
                        // vez por segundo e quase sempre na mesma faixa, e
                        // refazer o teto a cada chegada seria acordar a thread
                        // do codificador sessenta vezes por minuto para lhe
                        // dizer o que ela já sabe.
                        self.reconferir_o_teto();
                    }
                }
            }
            ServerMessage::ScreenShareStarted { person, screen, .. } => {
                self.talvez_ligar_a_bomba(person, screen);
            }
            ServerMessage::ScreenShareStopped { screen, .. } => {
                if self.e_a_minha_tela(screen) {
                    self.parar_a_tela();
                }
            }
            // §3.3: quadro-chave quando quem recebe pede, e nunca periódico.
            // Um de 1080p custa 65 KiB, quatro vezes um quadro comum.
            ServerMessage::KeyFrameRequested { screen, .. } => {
                if self.e_a_minha_tela(screen) {
                    self.pedir_a_chave_da_tela();
                }
            }
            // Uma transmissão a que esta sessão assiste desce pela mídia RTP
            // (ADR 0067): a abertura vem aqui, e os quadros, pela faixa.
            ServerMessage::TelaPorMidia { faixa, abertura }
                if faixa == crate::tela::FAIXA_DE_SAIDA =>
            {
                self.assistir_pela_midia(abertura);
            }
            ServerMessage::FimDaTelaPorMidia { faixa, limpo }
                if faixa == crate::tela::FAIXA_DE_SAIDA =>
            {
                if !limpo {
                    tracing::info!(
                        "a cópia de uma transmissão por RTP acabou antes da transmissão"
                    );
                }
                if let Some(cliente) = self.cliente.as_ref() {
                    cliente.fim_da_tela_por_midia();
                }
            }
            ServerMessage::ScreenViewers { tela, quantos } => {
                if self.e_a_minha_tela(tela) {
                    self.espectadores = quantos;
                    self.reconferir_o_teto();
                }
            }
            // Zero é **ausência de medida** e nunca zero bits por segundo — o
            // contrato está escrito no próprio quadro do protocolo.
            ServerMessage::HostUplink { bps } => {
                self.caminho_de_quem_hospeda_bps = (bps > 0).then_some(bps);
                self.reconferir_o_teto();
            }
            // O caminho entre pares: vá buscar esta tela naquele par, e caia
            // para o servidor sem drama se ele não vier. Ver
            // `Motor::assistir_por_par`.
            ServerMessage::AssistaTelaPor {
                screen,
                ref enderecos,
                ref impressao,
            } => {
                self.assistir_por_par(screen, enderecos.clone(), impressao.clone());
            }
            // O caminho entre pares, do outro lado: passe a atender e disque
            // para quem vai me buscar. Ver `Motor::servir_par`.
            ServerMessage::SirvaTelaPara {
                screen,
                ref enderecos,
                ref impressao,
            } => {
                self.servir_par(screen, enderecos.clone(), impressao.clone());
            }
            _ => {}
        }
    }

    // -------------------------------------------------------- caminho entre pares

    /// A ponta do caminho entre pares — a mesma que já fala com o servidor, e
    /// não uma nova.
    ///
    /// **Achado do fix round 2.** A primeira versão desta função abria um
    /// socket próprio (`crate::client::local_endpoint`), e o §3.1 da spec de
    /// 05/09 proíbe isso com todas as letras: *"Não é escuta nova, socket
    /// novo nem porta nova [...] aquela porta já tem mapeamento de NAT vivo,
    /// mantido pelo `keep_alive_interval` da conexão com o servidor"*. Uma
    /// porta nova tem endereço público **desconhecido do servidor** — ele só
    /// vê a origem da conexão de controle, que é a que `Client::endpoint`
    /// devolve — e o furo simultâneo mirava um endereço onde nada atendia.
    ///
    /// `None` só quando não há conexão viva agora. Não deveria acontecer
    /// quando isto é chamado: só se chega aqui processando uma
    /// `ServerMessage`, que só existe porque há conexão, ou declarando
    /// identidade logo depois de conectar.
    fn ponta_de_pares(&self) -> Option<crate::transporte::PontaDaSessao> {
        self.cliente.as_ref().map(Client::ponta)
    }

    /// A identidade efêmera desta sessão para o caminho entre pares: o
    /// certificado DTLS que a ponta da conexão viva apresenta como par.
    ///
    /// Nasce com a ponta, e uma reconexão abre ponta nova, com identidade
    /// nova: é por isso que a declaração se repete a cada volta.
    ///
    /// # Errors
    ///
    /// [`par::ErroDePar::Escuta`] sem conexão viva.
    fn identidade_de_par(&self) -> Result<par::Identidade, par::ErroDePar> {
        self.ponta_de_pares()
            .map(|ponta| ponta.identidade().clone())
            .ok_or_else(|| par::ErroDePar::Escuta("sem conexão viva".to_owned()))
    }

    /// Declara ao servidor a identidade efêmera deste par — **mesmo sem optar
    /// por emprestar a subida**.
    ///
    /// # Por que declarar sempre, e não só quando alguém empresta
    ///
    /// Achado do fix round 1 da Task 8: a parede simétrica da Task 5 exige
    /// certificado dos dois lados de toda ligação entre pares, e quem só
    /// assiste (`AssistaTelaPor`) também disca com a própria identidade
    /// quando `SirvaTelaPara` manda alguém procurá-la — ver
    /// [`Motor::assistir_por_par`]. Sem esta declaração, só quem emprestasse
    /// teria impressão registrada no servidor, e a discagem de todo mundo que
    /// só assiste seria recusada como `SemCertificado`. Um ruling meu de
    /// pré-voo misturava «quem eu sou» com «eu empresto»; `crate::par` e
    /// `seele-server/src/pares.rs` documentam a separação inteira.
    ///
    /// # Por que aqui, e não sob demanda na primeira mensagem do caminho
    ///
    /// Por então já seria tarde: o servidor só pode apontar esta pessoa a
    /// quem for procurá-la se a declaração já tiver chegado **antes** de
    /// alguém pedir. Por isso é chamada na conexão inicial e em cada
    /// reconexão — a sessão anterior já foi apagada de `Pares` na saída
    /// (`Pares::saiu`, no servidor), e a identidade efêmera não sobrevive a
    /// ela sem ser dita de novo.
    ///
    /// `locais` só leva os endereços de rede local desta máquina quando
    /// `emprestando` for `true` — **achado do fix round 3, e correção de um
    /// vazamento de privacidade que o round 2 introduziu.** O §5 da spec
    /// nomeia privacidade como a primeira das duas razões independentes do
    /// opt-in — um servidor não é necessariamente entre amigos (ADR 0021) —,
    /// e o round 2 fez esta função publicar a topologia de rede interna de
    /// **toda** máquina que apenas conecta, `emprestando: false` incluído.
    /// Quem só assiste declara a impressão e nada mais: o público que o
    /// servidor já vê na conexão de controle basta para quem empresta discar
    /// de volta, e o furo simultâneo cobre o resto.
    ///
    /// Quem chama passa a escolha da pessoa, guardada em
    /// [`Motor::emprestando`]: `Comando::EmprestarSubida` a muda, e a conexão
    /// inicial e cada reconexão a repetem. Enquanto ninguém optar por
    /// emprestar, `locais_de_pares` nem chega a rodar — ver
    /// [`locais_a_publicar`].
    async fn declarar_identidade_de_par(&mut self, consentimento: ConsentimentoDePar) {
        let identidade = match self.identidade_de_par() {
            Ok(identidade) => identidade,
            Err(erro) => {
                tracing::warn!(%erro, "não deu para gerar a identidade deste par");
                return;
            }
        };
        let Some(ponta) = self.ponta_de_pares() else {
            return;
        };
        let locais = locais_a_publicar(consentimento, || locais_de_pares(&ponta));
        let Some(cliente) = self.cliente.as_mut() else {
            return;
        };
        if let Err(erro) = cliente
            .emprestar_subida(consentimento, par::impressao(&identidade), locais)
            .await
        {
            tracing::warn!(%erro, "não deu para declarar a identidade deste par ao servidor");
        }
    }

    /// Passa a valer este consentimento — e desfaz agora o que ele revoga.
    ///
    /// # Por que a retirada alcança o que já está no ar
    ///
    /// Porque o contrário é a pessoa continuar pagando pela decisão que acabou
    /// de desfazer. Um consentimento que só valesse para o pedido seguinte
    /// deixaria a cópia em curso subindo depois de o interruptor ter sido
    /// desligado, e o único aviso seria a conta de internet dela.
    ///
    /// As duas metades caem em lugares diferentes, porque são coisas
    /// diferentes:
    ///
    /// - **deixar de emprestar a conexão** cancela a tarefa que serve um par,
    ///   e o [`Drop`] de [`VagaDeAtendimento`] devolve a vaga por tabela;
    /// - **deixar de assistir por par** derruba os caminhos abertos. Quem
    ///   reabre o cano do servidor para esta máquina é o **servidor**, ao
    ///   receber a declaração nova — ver `session::devolver_ao_servidor`. Ele
    ///   é quem tem o cano; este lado só desfaz o que é dele.
    ///
    /// **Só o que o consentimento novo revoga.** A declaração é repetida a
    /// cada reconexão, e uma retirada escrita larga demais derrubaria todo
    /// caminho vivo a cada volta da bateria interna.
    async fn passar_a_consentir(&mut self, novo: ConsentimentoDePar) {
        if !novo.empresta_conexao() {
            self.tarefas_de_par.parar_de_servir();
        }
        if !novo.assiste_por_par {
            self.tarefas_de_par.parar_de_assistir_a_tudo();
        }
        self.consentimento = novo;
        self.declarar_identidade_de_par(novo).await;
    }

    /// `ServerMessage::AssistaTelaPor`: vá buscar esta tela naquele par.
    ///
    /// Roda numa tarefa solta porque `par::cumprir_apresentacao` pode levar até
    /// [`PRAZO_DO_PAR`] — bloquear o laço de [`Motor::rodar`] por isso
    /// pausaria a voz, o ping e o resto desta sessão até o par responder ou o
    /// prazo vencer.
    ///
    /// Quem chama não tem decisão a tomar sobre a falha do par: o erro de
    /// `par::cumprir_apresentacao` vira o motivo enumerado (`par::motivo_de_falha`)
    /// e este método o manda ao servidor pelo canal de [`ResultadoDoPar`]: só o
    /// laço de `rodar`, dono do `&mut Client`, pode falar com ele.
    ///
    /// Disca com a **própria** identidade — a parede simétrica da Task 5 faz
    /// quem atende exigir certificado sempre, e sem uma identidade para
    /// apresentar todo `SirvaTelaPara` real seria recusado como
    /// `SemCertificado`. `Motor::declarar_identidade_de_par` garante que ela
    /// já existe (e já foi dita ao servidor) desde a conexão.
    fn assistir_por_par(
        &mut self,
        screen: ScreenId,
        enderecos: Vec<SocketAddr>,
        impressao: String,
    ) {
        // **O pedido que chegou depois da retirada.** O servidor escolhe e
        // difunde; a retirada desta máquina viaja no sentido contrário. As
        // duas se cruzam no fio, e atender o pedido velho seria discar — e ser
        // discado — com o endereço que esta máquina acabou de tirar de
        // circulação. Quem tem a palavra final é quem paga a conta.
        if !self.consentimento.assiste_por_par {
            tracing::info!(
                ?screen,
                "chegou um pedido para assistir por par depois de este consentimento ter sido \
                 retirado; esta tela continua vindo do servidor"
            );
            return;
        }
        let Some(ponta) = self.ponta_de_pares() else {
            tracing::warn!(
                ?screen,
                "sem conexão com o servidor: não há ponta para o caminho entre pares"
            );
            let _ = self.resultados_do_par_tx.send(ResultadoDoPar::ParFalhou {
                screen,
                motivo: MotivoDeFalhaDePar::NaoAlcancou,
            });
            return;
        };
        let avisos = self.avisos.clone();
        let resultados = self.resultados_do_par_tx.clone();
        let tarefa = tokio::spawn(async move {
            match par::cumprir_apresentacao(
                &ponta,
                par::Papel::Busca,
                &enderecos,
                impressao,
                PRAZO_DO_PAR,
            )
            .await
            {
                Ok(ligado) => {
                    // A conta de bytes é de quem empresta — `crate::par::repassar`
                    // escreve por pedaço do lado dele. Do lado de quem assiste
                    // muda uma coisa só, e ela está no doc de
                    // `DeOndeVeioATela`: um fluxo que acaba no meio é o par
                    // tendo caído, e é daqui que o servidor fica sabendo.
                    match tokio::time::timeout(PRAZO_DO_PAR, ligado.conexao.accept_uni()).await {
                        // [`ler_a_tela_alheia`] e não [`escoar_tela_alheia`],
                        // que é a diferença entre a alça valer e não valer:
                        // `escoar` abre uma tarefa nova, e abortar **esta**
                        // deixaria aquela lendo do par. Aqui a leitura acontece
                        // dentro da tarefa que o motor guarda, e é por isso que
                        // o corpo foi separado da tarefa.
                        Ok(Ok(fluxo)) => {
                            ler_a_tela_alheia(
                                avisos,
                                FonteDaTelaAlheia::Canal(fluxo),
                                DeOndeVeioATela::Par {
                                    screen,
                                    resultados: resultados.clone(),
                                },
                            )
                            .await;
                        }
                        Ok(Err(erro)) => {
                            tracing::warn!(%erro, ?screen, "o par ligou e a transmissão não abriu");
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen,
                                motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                            });
                        }
                        Err(_prazo) => {
                            tracing::warn!(
                                ?screen,
                                "o par ligou e não abriu a transmissão a tempo"
                            );
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen,
                                motivo: MotivoDeFalhaDePar::ParouDeMandar,
                            });
                        }
                    }
                }
                // O motivo é enumerado aqui — `ImpressaoNaoBate`
                // continua sendo o evento de segurança que é, agora também
                // para o servidor, e não só no `tracing` local de
                // `classificar` em `par.rs`.
                Err(erro) => {
                    let motivo = par::motivo_de_falha(&erro);
                    tracing::info!(%erro, ?motivo, ?screen, "o par não veio; a tela vem do servidor");
                    let _ = resultados.send(ResultadoDoPar::ParFalhou { screen, motivo });
                }
            }
        });
        self.tarefas_de_par.assistir(screen, tarefa);
    }

    /// `ServerMessage::SirvaTelaPara`: passe a atender, e disque para o outro
    /// lado — as duas tentativas simultâneas são o furo.
    ///
    /// Como [`Motor::assistir_por_par`], roda solta pela mesma razão de prazo.
    /// Ao contrário dela, quem empresta **nunca** manda `ParFalhou`:
    /// `seele_proto::control::ClientMessage::ParFalhou` é explícita que só
    /// quem recebe manda essa mensagem, porque só quem recebe sabe que a
    /// imagem parou — quem empresta pode ter caído sem chegar a saber de nada.
    fn servir_par(&mut self, screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String) {
        // O mesmo cruzamento, do lado de quem empresta — e aqui o que estava
        // em jogo é a subida desta máquina. Ver [`Motor::assistir_por_par`].
        if !self.consentimento.empresta_conexao() {
            tracing::info!(
                ?screen,
                "chegou um pedido para servir um par depois de este consentimento ter sido \
                 retirado; esta máquina não sobe esta cópia"
            );
            return;
        }
        // **Tomada aqui, e devolvida por quem a segura.** Ver
        // [`VagaDeAtendimento`]: enquanto isto eram duas escritas, a devolução
        // morava na última linha do corpo da tarefa — e um `abort` faz essa
        // linha nunca correr. Como guarda, cada saída antecipada abaixo
        // devolve a vaga ao voltar, sem precisar dizer.
        let Some(vaga) = VagaDeAtendimento::tomar(&self.atendendo_pares) else {
            // No A1 quem empresta serve um par por vez (`par::atender` só
            // aceita uma ligação). Sem vaga, e sem resposta esperada desta
            // mensagem — só o rastro.
            tracing::warn!(
                ?screen,
                "um pedido para servir chegou enquanto este par já servia outro"
            );
            return;
        };
        let Some(ponta) = self.ponta_de_pares() else {
            tracing::warn!(
                ?screen,
                "sem conexão com o servidor: não há ponta para o caminho entre pares"
            );
            return;
        };
        self.passar_a_servir(vaga, ponta, screen, enderecos, impressao);
    }

    /// Põe de pé a tarefa que serve este par, e passa a ser dono dela.
    ///
    /// # Por que separada de [`Motor::servir_par`]
    ///
    /// Porque é a metade que dá para provar. O que fica lá em cima precisa de
    /// um [`Client`] vivo — `ponta_de_pares` sai dele —, e um `Client` precisa
    /// de um servidor do outro lado: nada disso cabe num teste de unidade
    /// deste crate, e é por isso que o resto do caminho entre pares é provado
    /// em `seele-conformance`. **A posse da alça não estava em nenhum dos
    /// dois.** Medido: com `self.tarefas_de_par.servir(tarefa)` trocado por
    /// largar a alça, a suíte inteira continuava verde.
    ///
    /// Daqui para baixo não há `Client` nenhum — só a ponta da sessão e
    /// endereços —, e é o que
    /// `testes::cair_devolve_a_vaga_de_quem_estava_servindo_um_par` monta à
    /// mão para prender as duas coisas que este método faz: guardar a alça e
    /// entregar a vaga a quem a solta.
    fn passar_a_servir(
        &mut self,
        vaga: VagaDeAtendimento,
        ponta: crate::transporte::PontaDaSessao,
        screen: ScreenId,
        enderecos: Vec<SocketAddr>,
        impressao: String,
    ) {
        let repasse = Arc::clone(&self.repasse);
        let tarefa = tokio::spawn(async move {
            // **A vaga viaja para dentro da tarefa, e morre com ela.** É o que
            // faz o `abort` de [`TarefasDePar`] devolvê-la: soltar a tarefa
            // solta tudo o que ela segurava, e o guarda é uma dessas coisas.
            let _vaga = vaga;
            let ligado = servir_um_par(ponta, enderecos, impressao).await;
            match ligado {
                Some(ligado) => {
                    tracing::info!(
                        par = %ligado.conexao.remote_address(),
                        como = ?ligado.como,
                        ?screen,
                        "este par está sendo servido"
                    );
                    repassar_a_tela(&repasse, &ligado, screen).await;
                }
                // As duas tentativas — passar_a_atender+atender e ligar — não
                // deram em nada. Sem `ParFalhou` daqui: é quem assiste que vai
                // notar a falta de imagem e avisar o servidor.
                None => tracing::info!(
                    ?screen,
                    "nenhuma das duas tentativas de servir este par deu certo"
                ),
            }
            // **Devolvida depois do repasse, e não depois da ligação.** A vaga
            // é «estou servindo um par», e servir é o repasse — devolvê-la
            // assim que a conexão fecha deixaria um segundo `SirvaTelaPara`
            // entrar por cima de um repasse em curso, e `crate::par::atender`
            // só tem uma vaga. É onde `_vaga` morre, e é por isso que ele é
            // ligado no topo deste corpo e não perto de `servir_um_par`.
            drop(_vaga);
        });
        // E o motor passa a ser dono dela: sem esta linha a tarefa não tinha
        // alça nenhuma em lugar nenhum, e sobrevivia à sessão inteira
        // repassando a tela a um par por uma conexão que já tinha morrido.
        self.tarefas_de_par.servir(tarefa);
    }
}

/// Como terminou o salvar de um anexo, e o que o `seele.log` fica sabendo dele.
///
/// A tela recebe `NaoSalvou` sem motivo — para ela, é `Falhou` —, e o motor é
/// o único que sabe se foram os bytes que não fecharam com o hash, o prazo que
/// acabou ou o disco que recusou. Fora do `tokio::spawn` que a chama para que
/// um teste a exercite sem servidor.
///
/// `parcial` é onde os bytes estavam chegando — `.foto.png.seele-parcial` —, e
/// vai para a linha da falha porque diz a pasta e o nome que o anexo teria; o
/// caminho do salvo é o que [`crate::client::Transfers::receive_attachment`]
/// devolve, com o nome final que ele ganhou.
fn fim_do_salvar(
    anexo: AttachmentId,
    parcial: &std::path::Path,
    recebido: anyhow::Result<(u64, std::path::PathBuf)>,
) -> Transferencia {
    match recebido {
        Ok((_, caminho)) => Transferencia::Salvo { anexo, caminho },
        Err(erro) => {
            tracing::warn!(
                %anexo,
                caminho = %parcial.display(),
                %erro,
                "o anexo não foi salvo"
            );
            Transferencia::NaoSalvou { anexo }
        }
    }
}

/// Repassa a este par a tela que esta máquina está recebendo do servidor.
///
/// **É o último elo do subprojeto A.** As tarefas anteriores fazem dois
/// clientes se ligarem e fazem `crate::par::repassar` saber escrever; esta
/// função é o que liga a tela que chega à ligação que existe — sem ela, quem
/// empresta a subida abre a conexão com o par e não lhe manda byte nenhum, e
/// quem assiste vê exatamente o que veria se a malha não existisse.
///
/// Não devolve nada e não avisa o servidor de falha nenhuma, de propósito:
/// `seele_proto::control::ClientMessage::ParFalhou` é explícita que só quem
/// **recebe** manda essa mensagem. Um repasse que morre no meio é imagem que
/// para do lado de lá, e é de lá que o aviso sai.
async fn repassar_a_tela(repasse: &RepasseDeTela, ligado: &par::ParLigado, screen: ScreenId) {
    // Sem abertura não há o que repassar: ou nenhuma transmissão está chegando
    // agora, ou ela acabou entre o pedido do servidor e a ligação fechar. Um
    // par ligado num fluxo sem cabeçalho não decodifica nada, e mandar-lhe
    // pedaços soltos seria pior que não mandar nada.
    // `abertura_de` e não `abertura`: com duas transmissões no ar, a que está
    // sendo repassada pode não ser a que o servidor mandou servir. Repassar a
    // outra seria entregar ao par uma tela com o nome de outra — ver o doc de
    // `EstadoDoRepasse::qual`.
    let Some(abertura) = repasse.abertura_de(screen) else {
        tracing::warn!(
            ?screen,
            "o par ligou e esta máquina não está recebendo do servidor a transmissão que lhe \
             mandaram repassar; quem assiste continua sendo servido pelo servidor"
        );
        return;
    };
    let (pedacos_tx, pedacos_rx) = mpsc::channel(PEDACOS_A_ESPERA_DO_PAR);
    repasse.ligar(pedacos_tx);
    let resultado = par::repassar(ligado, &abertura, pedacos_rx).await;
    repasse.desligar();
    match resultado {
        Ok(()) => tracing::info!(?screen, "o repasse desta tela ao par terminou"),
        Err(erro) => tracing::warn!(%erro, ?screen, "o repasse desta tela ao par falhou"),
    }
}

/// Passa a atender, e fura o NAT desta ponta para o outro lado enquanto
/// espera: a metade de quem serve de [`par::cumprir_apresentacao`].
///
/// **Quem assiste disca; quem empresta atende.** O corpo mora em
/// `par::cumprir_apresentacao`, que o túnel de jogo usa com os mesmos prazos;
/// aqui sobra o que é da tela: o resultado sem motivo (quem empresta nunca
/// manda `ParFalhou`) e o `Option` que o resto do motor lê.
///
/// `None` se ninguém chegar a tempo. Livre e não método de `Motor` porque roda
/// dentro do `tokio::spawn` de [`Motor::servir_par`].
async fn servir_um_par(
    ponta: crate::transporte::PontaDaSessao,
    enderecos: Vec<SocketAddr>,
    impressao: String,
) -> Option<par::ParLigado> {
    par::cumprir_apresentacao(
        &ponta,
        par::Papel::Serve,
        &enderecos,
        impressao,
        PRAZO_DO_PAR,
    )
    .await
    .ok()
}

/// Os endereços de rede local desta máquina, na porta que `ponta` já usa.
///
/// **Achado do fix round 2.** Enumeração simples, sem a ordenação por
/// heurística de VPN que `seele-server::alcance::interfaces::descobrir` faz
/// para o convite. O ADR 0002 proíbe este crate de depender de `seele-server`,
/// então a pergunta é refeita aqui, com o mesmo crate (`if_addrs`).
///
/// Vazio se a enumeração falhar ou não achar nenhum endereço utilizável: o
/// público que o servidor já vê na conexão de controle continua sobrando.
///
/// # Sem caminho de produção hoje
///
/// **Nada em `apps/` nem no `seele-ffi` liga o empréstimo**, e esta função só
/// roda quando alguém o liga (ver [`locais_a_publicar`]). O atalho de LAN nunca
/// foi exercitado contra uma rede de verdade.
fn locais_de_pares(ponta: &crate::transporte::PontaDaSessao) -> Vec<SocketAddr> {
    let local = ponta.ponta().local_addr();
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => interfaces
            .into_iter()
            .map(|interface| interface.addr.ip())
            .filter(|ip| e_endereco_de_rede_local(*ip))
            .map(|ip| SocketAddr::new(ip, local.port()))
            .collect(),
        Err(erro) => {
            tracing::warn!(%erro, "não deu para enumerar os endereços locais desta máquina");
            Vec::new()
        }
    }
}

/// Quantos pares esta versão do cliente consegue atender ao mesmo tempo.
///
/// **Um, e o número é desta implementação e não do protocolo.**
/// `crate::par::atender` aceita uma ligação por vez e
/// [`VagaDeAtendimento`] é um punho só, então servir dois seria uma promessa
/// que este cliente não tem como cumprir — e quem pagaria por ela seria a
/// segunda pessoa, esperando o prazo do par vencer por uma cópia que nunca
/// chegaria. A casa chama isto de «existir não é funcionar», e é por isso que
/// [`Enlace::consentir_no_caminho_entre_pares`] **baixa** um teto maior em vez
/// de o mandar adiante.
///
/// O campo no fio é um `u8` e o servidor respeita o número que chegar
/// (`seele_server::pares::Pares::escolher`): quando o subprojeto B ensinar
/// este cliente a servir vários, é esta constante que sobe, e nada mais — sem
/// versão nova de protocolo.
pub const PARES_QUE_ESTA_VERSAO_ATENDE: u8 = 1;

/// Baixa um consentimento ao que esta versão do cliente consegue honrar.
///
/// Só o teto muda; `assiste_por_par` atravessa como veio, porque não há nada
/// nesta máquina que o limite. Ver [`PARES_QUE_ESTA_VERSAO_ATENDE`] para o
/// porquê de baixar em vez de mandar adiante.
fn cabivel(consentimento: ConsentimentoDePar) -> ConsentimentoDePar {
    ConsentimentoDePar {
        pares_que_atende: consentimento
            .pares_que_atende
            .min(PARES_QUE_ESTA_VERSAO_ATENDE),
        ..consentimento
    }
}

/// Quais endereços de rede local uma declaração publica.
///
/// **A regra do opt-in, isolada para poder ser presa por teste — achado do
/// fix round 3.** Só quem optou por emprestar publica os endereços da própria
/// máquina: o §5 da spec nomeia privacidade como a primeira das duas razões
/// independentes do opt-in, e lembra que um servidor não é necessariamente
/// entre amigos (ADR 0021). Quem só assiste declara a impressão e nada mais —
/// o público que o servidor já vê na conexão de controle basta para quem
/// empresta discar de volta, e o furo simultâneo cobre o resto.
///
/// # Consentir em assistir por par **não** destranca esta publicação
///
/// E é deliberado. As duas metades do §5 protegem coisas diferentes:
/// `assiste_por_par` decide se o endereço **público** desta máquina — o que o
/// servidor já vê, e o único de que quem empresta precisa — pode ser
/// *entregue* a quem a serve; `empresta_conexao` decide se a **topologia de
/// rede interna** desta máquina é publicada, o que só faz sentido para quem
/// vai ser procurado por vários. Destrancar a segunda com a primeira daria a
/// quem consentiu no menor o custo do maior.
///
/// `todos` é adiado (`FnOnce`) de propósito: enumerar as interfaces desta
/// máquina é trabalho que quem não empresta nem chega a fazer, e um argumento
/// já avaliado esconderia dentro do chamador justamente a decisão que este
/// guarda existe para prender.
///
/// # O ramo `true` não tem caminho de produção hoje
///
/// **Nada em `apps/` nem no `seele-ffi` chama `Enlace::emprestar_subida`**, e
/// sem isso `emprestando` é sempre `false` em produção: o ramo que publica
/// endereços só roda no teste de integração
/// `seele-conformance/tests/tela_por_um_par.rs`. O guarda do opt-in está preso
/// por teste; o que não foi exercitado é o **caminho de LAN** que ele
/// destranca. Ver `docs/teste-duas-maquinas.md`, que registra o mesmo.
fn locais_a_publicar<F: FnOnce() -> Vec<SocketAddr>>(
    consentimento: ConsentimentoDePar,
    todos: F,
) -> Vec<SocketAddr> {
    if consentimento.empresta_conexao() {
        // O servidor recusa mais que isto (`MAX_LOCAIS_DE_PAR`): uma máquina
        // com muitas interfaces manda as primeiras, e não uma declaração que
        // o fio rejeita.
        let mut locais = todos();
        locais.truncate(seele_proto::control::MAX_LOCAIS_DE_PAR);
        locais
    } else {
        Vec::new()
    }
}

/// Se este endereço vale como "local" para o caminho entre pares.
///
/// Loopback e não especificado não saem desta máquina; link-local
/// (`169.254.0.0/16`, `fe80::/10`) não sai do cabo — o mesmo motivo que
/// `seele-server::alcance::interfaces::descobrir` já documenta para o
/// convite. Tudo o mais entra, inclusive um endereço público diretamente
/// atribuído a uma interface: um duplicado do que o servidor já vê não faz
/// mal, `Pares::declarou` já lida com isso.
fn e_endereco_de_rede_local(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => !v4.is_link_local(),
        IpAddr::V6(v6) => !v6.segments().first().is_some_and(|s| s & 0xffc0 == 0xfe80),
    }
}

/// Quantos pedaços de tela ficam à espera de sair para o par.
///
/// **Um número, e a razão de ele existir é o que se faz quando ele estoura.**
/// Quem repassa está assistindo à mesma tela, e a leitura dele não pode
/// esperar a escrita para o par: um par que parou de ler prenderia a imagem de
/// quem empresta, que é o oposto de «a malha é alívio». Cheio, o repasse é
/// **desligado inteiro** — nunca é descartado um pedaço no meio, porque um
/// buraco no fluxo desloca o enquadramento de quem recebe para sempre.
///
/// # Por qual mecanismo quem assiste volta ao servidor
///
/// Desligado o destino, o `Sender` cai, [`crate::par::repassar`] termina o
/// fluxo do par **direito** — e um fluxo que termina direito não é um erro do
/// outro lado. Por isso o fim limpo de um fluxo de par é reportado como
/// `ClientMessage::ParFalhou { motivo: ParouDeMandar }` em
/// [`escoar_tela_alheia`], e não só o fim torto: é esse relato que faz o
/// servidor religar o cano de quem assiste. Sem ele, esta constante estourar
/// seria tela em branco permanente — quem assiste já saiu do cano do servidor
/// desde que o par foi apontado, e nada o recolocaria lá.
///
/// Trinta e dois pedaços são cerca de um segundo a trinta quadros por segundo,
/// que é muito mais do que uma escrita para um par saudável leva e pouco o
/// bastante para a memória não crescer sem limite atrás de um par doente.
const PEDACOS_A_ESPERA_DO_PAR: usize = 32;

/// A tela que chega do servidor, aberta para quem a repassa a um par.
///
/// # Por que um lugar partilhado, e não um argumento
///
/// As duas pontas desta ligação nascem em momentos diferentes e vivem em
/// tarefas diferentes. A tela alheia chega quando quem compartilha abre o
/// fluxo, e é lida por [`escoar_tela_alheia`], numa tarefa própria que dura o
/// que a transmissão durar. O pedido para servir um par chega depois — pode
/// chegar muito depois — como `ServerMessage::SirvaTelaPara`, e é atendido por
/// outra tarefa ([`Motor::servir_par`]). Nenhuma das duas pode ser argumento
/// da outra; o que as liga é este ponto de encontro, que o [`Motor`] cria uma
/// vez e empresta às duas.
///
/// # O que ele guarda
///
/// A **abertura** da transmissão que está chegando — os bytes crus do
/// cabeçalho, que [`crate::par::repassar`] escreve tal e qual —, e o
/// **destino** dos pedaços, quando há um par sendo servido. Sem abertura não
/// há o que repassar: um par ligado no meio de uma transmissão cujo cabeçalho
/// ele nunca viu não decodifica nada.
///
/// `std::sync::Mutex` e não o do `tokio`: nada aqui dentro espera por nada, e
/// os dois punhos são segurados por microssegundos. Um mutex assíncrono aqui
/// só acrescentaria pontos de suspensão a caminhos que não os têm.
#[derive(Debug, Default)]
struct RepasseDeTela {
    /// Tudo sob um punho só. Ver o doc de [`EstadoDoRepasse`].
    estado: std::sync::Mutex<EstadoDoRepasse>,
}

/// O que o repasse guarda, e por que num punho só.
///
/// **Porque as três coisas se decidem juntas.** «Estes bytes vão para o par?»
/// é uma pergunta sobre a tela em curso *e* sobre haver destino; separada em
/// dois punhos, ela é respondida em dois instantes, e entre os dois a
/// transmissão pode ter trocado. Um quadro copiado para o fluxo de outra tela
/// não dá erro em lugar nenhum — dá duas telas fundidas numa só na janela de
/// quem assiste, que é o defeito que este guarda existe para impedir.
#[derive(Debug, Default)]
struct EstadoDoRepasse {
    /// De qual transmissão é o repasse em curso.
    ///
    /// **É a identidade que faltava, e ela é uma só de propósito.** Havia um
    /// [`RepasseDeTela`] por [`Motor`] e nenhuma marca de tela: com duas
    /// transmissões no ar na mesma sala — o cenário do §0 do desenho —, a
    /// segunda `abriu()` sobrescrevia a primeira, os quadros das duas entravam
    /// intercalados no mesmo fluxo do par, e o primeiro `fechou()` matava o
    /// repasse da outra. Quem recebia rotulava tudo com o `screen` do fluxo e
    /// via as duas telas fundidas, sem um erro em lugar nenhum.
    ///
    /// **A versão que repassa as duas é do subprojeto B**, e não cabe aqui:
    /// ela exige uma marca de tela no fio entre pares — mudança de protocolo,
    /// que é a fundação daquele subprojeto. O que cabe hoje é a honestidade:
    /// uma tela por vez, dito em voz alta, com quem assiste a outra
    /// continuando a receber do servidor pelo caminho de sempre.
    ///
    /// A identidade é o [`ScreenId`] e **não** o dono da transmissão: este
    /// lado não sabe de quem é uma tela alheia. `ScreenHeader` não carrega
    /// pessoa e [`Aviso::TelaAbriu`] também não — e não precisa carregar: o
    /// `ScreenId` é atribuído pelo servidor e é único por transmissão, que é
    /// exatamente a pergunta que este campo responde.
    qual: Option<ScreenId>,
    /// Os bytes crus do cabeçalho da transmissão que chega agora do servidor.
    abertura: Option<Vec<u8>>,
    /// Para onde copiar cada quadro, enquanto há um par a servir.
    destino: Option<mpsc::Sender<Vec<u8>>>,
}

impl RepasseDeTela {
    /// Uma transmissão do servidor abriu com este cabeçalho.
    ///
    /// **A segunda tela não assume.** Se já há um repasse em curso de outra
    /// transmissão, esta é recusada e o `warn!` diz por quê — ver o doc de
    /// [`EstadoDoRepasse::qual`]. Quem assiste à tela recusada continua
    /// recebendo do servidor, que é o caminho de sempre: nenhuma imagem se
    /// perde, e nada se funde em silêncio.
    fn abriu(&self, tela: ScreenId, abertura: Vec<u8>) {
        let Ok(mut estado) = self.estado.lock() else {
            return;
        };
        if let Some(em_curso) = estado.qual {
            if em_curso != tela {
                tracing::warn!(
                    %em_curso,
                    nova = %tela,
                    "só uma tela por vez é repassada a um par nesta versão; esta segunda \
                     transmissão continua vindo do servidor para quem a assiste"
                );
                return;
            }
        }
        estado.qual = Some(tela);
        estado.abertura = Some(abertura);
    }

    /// A transmissão do servidor acabou: não há mais o que repassar, e o par
    /// que estava sendo servido vê o fluxo terminar direito.
    ///
    /// **Só se for a que está sendo repassada.** Sem esta conferência, o fim
    /// de uma segunda transmissão — que nunca chegou a ser repassada —
    /// derrubaria o repasse da primeira.
    fn fechou(&self, tela: ScreenId) {
        let Ok(mut estado) = self.estado.lock() else {
            return;
        };
        if estado.qual != Some(tela) {
            return;
        }
        estado.qual = None;
        estado.abertura = None;
        estado.destino = None;
    }

    /// O cabeçalho da transmissão que está chegando, se a que chega é esta.
    ///
    /// `None` quando o repasse em curso é de outra tela: servir o pedido do
    /// servidor com a abertura da tela errada seria entregar ao par uma
    /// transmissão com o nome de outra.
    fn abertura_de(&self, tela: ScreenId) -> Option<Vec<u8>> {
        let estado = self.estado.lock().ok()?;
        if estado.qual != Some(tela) {
            return None;
        }
        estado.abertura.clone()
    }

    /// Passa a copiar os pedaços para aqui.
    fn ligar(&self, destino: mpsc::Sender<Vec<u8>>) {
        if let Ok(mut estado) = self.estado.lock() {
            estado.destino = Some(destino);
        }
    }

    /// Para de copiar. O `Sender` largado fecha o canal, e
    /// [`crate::par::repassar`] termina o fluxo do par direito.
    fn desligar(&self) {
        if let Ok(mut estado) = self.estado.lock() {
            estado.destino = None;
        }
    }

    /// Copia mais um quadro para o par, se há um sendo servido.
    ///
    /// **Nunca espera, e nunca descarta um pedaço só.** Ver o doc de
    /// [`PEDACOS_A_ESPERA_DO_PAR`].
    ///
    /// **E só os da tela em curso.** Um quadro de outra transmissão entrando
    /// neste fluxo é o que fundia duas telas numa só.
    fn pedaco(&self, tela: ScreenId, bytes: Vec<u8>) {
        let Ok(mut estado) = self.estado.lock() else {
            return;
        };
        if estado.qual != Some(tela) {
            return;
        }
        let Some(destino) = estado.destino.as_ref() else {
            return;
        };
        match destino.try_send(bytes) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    quantos = PEDACOS_A_ESPERA_DO_PAR,
                    "o par parou de aceitar bytes; o repasse para ele é desligado"
                );
                estado.destino = None;
            }
            Err(mpsc::error::TrySendError::Closed(_)) => estado.destino = None,
        }
    }
}

/// De onde uma transmissão de tela alheia está chegando.
///
/// **As duas pontas leem o mesmo formato e fazem coisas diferentes com o fim
/// dele**, e é isso que este `enum` carrega. Do servidor, o fim é a
/// transmissão acabando, e os pedaços do meio podem interessar a um par que
/// esta máquina sirva. De um par, o fim no meio é o par tendo caído — e quem
/// assiste é a **única** pessoa que sabe disso (ver o doc de
/// `ClientMessage::ParFalhou`), então é daqui que sai o aviso ao servidor.
///
/// # O byte de tipo, que só um dos dois já leu
///
/// Do servidor, o fluxo chega pelo roteador de `Client::connect`, que lê o
/// byte de tipo para saber para qual fila mandar o fluxo — então este lado
/// continua de onde ele parou (`Recepcao::do_fluxo_ja_tipado`). De um par não
/// há roteador nenhum: `crate::par::repassar` escreve o byte de tipo como
/// primeiro byte do fluxo, e quem lê tem de lê-lo (`Recepcao::do_fluxo`).
/// Trocar os dois é ler o primeiro byte do cabeçalho como se fosse o tipo, e
/// o cabeçalho inteiro sai deslocado.
enum DeOndeVeioATela {
    /// Do servidor. Os pedaços são copiados para o par que esta máquina serve,
    /// se ela estiver servindo algum.
    Servidor(Arc<RepasseDeTela>),
    /// De um par. Uma queda no meio vira `ParFalhou`.
    Par {
        /// Qual transmissão.
        screen: ScreenId,
        /// Por onde avisar o laço de [`Motor::rodar`], que é quem fala com o
        /// servidor.
        resultados: mpsc::UnboundedSender<ResultadoDoPar>,
    },
}

/// A espera de uma fila que pode não existir.
///
/// `None` quando não há conexão, e aí este braço do `select!` nunca acorda —
/// que é o certo: sem conexão não chega tela nenhuma. Um braço que devolvesse
/// `None` de imediato giraria o laço a full CPU.
async fn espera_da_fila(
    fila: &Option<crate::client::FilaDeTelas>,
) -> Option<crate::transporte::Recebe> {
    match fila {
        Some(fila) => fila.lock().await.recv().await,
        None => std::future::pending().await,
    }
}

/// Lê uma transmissão alheia até o fim, numa tarefa própria.
///
/// Própria porque ler uma tela dura o que a transmissão durar, e fazer isso no
/// laço principal pararia a voz, a presença e as mensagens de todo mundo
/// enquanto alguém compartilha — que é o oposto do que a §3.2 da spec pede
/// quando diz que a voz nunca cede à tela.
///
/// Livre e não método por causa do `select!` que a chama: o outro braço já tem
/// o `Motor` emprestado mutável, e um método aqui seria um segundo empréstimo.
/// O que ela precisa do motor é o remetente de avisos, que vem por argumento.
///
/// Os quadros saem pelo mesmo canal de avisos que todo o resto, e por isso
/// chegam à casca na ordem em que foram lidos.
///
/// `de_onde` diz por onde a transmissão veio, e as duas coisas que dependem
/// disso — quem lê o byte de tipo, e o que significa o fluxo acabar no meio —
/// estão no doc de [`DeOndeVeioATela`].
fn escoar_tela_alheia(
    avisos: mpsc::UnboundedSender<Aviso>,
    fonte: FonteDaTelaAlheia,
    de_onde: DeOndeVeioATela,
) {
    tokio::spawn(ler_a_tela_alheia(avisos, fonte, de_onde));
}

/// Por onde os quadros de uma transmissão alheia chegam: pelo canal `u`, ou
/// pela mídia RTP da conexão (ADR 0067), com a abertura já lida do controle.
enum FonteDaTelaAlheia {
    Canal(crate::transporte::Recebe),
    Midia(crate::tela::RecepcaoPorMidia),
}

/// A espera da fila da mídia, que pode não existir (sem conexão, ou com um
/// servidor que não fala RTP). Como [`espera_da_fila`].
async fn espera_da_midia(
    fila: &Option<crate::client::FilaDaMidia>,
) -> Option<crate::client::DaMidia> {
    match fila {
        Some(fila) => fila.lock().await.recv().await,
        None => std::future::pending().await,
    }
}

/// O corpo de [`escoar_tela_alheia`], sem a tarefa.
///
/// **Separado para que o caminho do par caiba numa tarefa só.**
/// [`Motor::assistir_por_par`] já roda solta — discar pode levar
/// [`PRAZO_DO_PAR`] —, e a leitura acontece dentro dela. Enquanto a leitura
/// abria uma **segunda** tarefa, a alça que o motor guardava não alcançava a
/// parte que importa: abortar a de fora deixava a de dentro lendo do par, e
/// quem tinha pedido para parar de assistir continuava recebendo imagem por
/// baixo. Uma tarefa só é o que faz a alça valer.
async fn ler_a_tela_alheia(
    avisos: mpsc::UnboundedSender<Aviso>,
    fonte: FonteDaTelaAlheia,
    de_onde: DeOndeVeioATela,
) {
    {
        {
            let aberto = match fonte {
                FonteDaTelaAlheia::Midia(recepcao) => {
                    Ok(crate::tela::LeituraDeTela::Midia(recepcao))
                }
                FonteDaTelaAlheia::Canal(fluxo) => match &de_onde {
                    DeOndeVeioATela::Servidor(_) => {
                        crate::tela::Recepcao::do_fluxo_ja_tipado(fluxo).await
                    }
                    DeOndeVeioATela::Par { .. } => crate::tela::Recepcao::do_fluxo(fluxo).await,
                }
                .map(crate::tela::LeituraDeTela::Canal),
            };
            let mut recepcao = match aberto {
                Ok(recepcao) => recepcao,
                Err(erro) => {
                    // **`warn!` e não `debug!`, e a casca fica sabendo.**
                    //
                    // Alguém do outro lado abriu um fluxo de tela e este build
                    // não conseguiu ler o cabeçalho dele. Não é ruído: é uma
                    // transmissão que existe e que esta pessoa não vai ver, e a
                    // causa quase sempre é versão diferente dos dois lados.
                    //
                    // Voltar calado era o que produzia a tela preta: sem
                    // `TelaAbriu` não há o que desenhar, e sem `TelaFechou` não
                    // há o que apagar — a casca não sabia que tinha havido nada.
                    tracing::warn!(
                        %erro,
                        "chegou uma transmissão de tela que esta versão não sabe ler"
                    );
                    let _ = avisos.send(Aviso::TelaIlegivel {
                        motivo: erro.to_string(),
                    });
                    // Um par que abre um fluxo ilegível é um par que não está
                    // servindo nada. Sem este aviso, quem assiste esperaria a
                    // imagem de alguém que nunca a vai mandar, e o servidor
                    // nunca saberia que tem de assumir.
                    if let DeOndeVeioATela::Par { screen, resultados } = &de_onde {
                        let _ = resultados.send(ResultadoDoPar::ParFalhou {
                            screen: *screen,
                            motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                        });
                    }
                    return;
                }
            };
            let cabecalho = *recepcao.cabecalho();
            let tela = cabecalho.screen;
            if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
                repasse.abriu(tela, recepcao.abertura().to_vec());
            }
            let leitura = nova_leitura_de_tela();
            if avisos
                .send(Aviso::TelaAbriu {
                    tela,
                    largura: cabecalho.width,
                    altura: cabecalho.height,
                    origem: cabecalho.source,
                    leitura,
                })
                .is_err()
            {
                return;
            }
            // **O fim desta leitura sai quando ela é solta, por onde for**: o
            // fim do laço, um `return`, ou o `abort` da troca de par e da queda
            // (`TarefasDePar`), que solta a tarefa parada no `await` e nunca
            // chegava à linha do fim.
            let _fim = FimDaLeitura {
                avisos: avisos.clone(),
                tela,
                leitura,
            };

            loop {
                match recepcao.proximo_quadro().await {
                    Ok(Some(quadro)) => {
                        // **A cópia para o par sai daqui, antes de qualquer
                        // decisão sobre o que a casca desenha.** Os bytes
                        // repassados são os mesmos que chegaram, na mesma
                        // ordem, som incluído: quem recebe do par tem de ver a
                        // mesma transmissão que quem recebe do servidor, e
                        // filtrar aqui produziria duas telas diferentes com o
                        // mesmo nome.
                        if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
                            repasse.pedaco(tela, quadro.no_fio());
                        }
                        // **O som não atravessa a ponte.** Ele vai para a
                        // mistura, aqui em Rust, e nunca para a casca: a janela
                        // não tem o que fazer com um pacote Opus, e mandá-la
                        // decodificar seria dar a ela um trabalho que este lado
                        // já sabe fazer — e que precisa acontecer no mesmo lugar
                        // onde o isolamento total vale.
                        let aviso = if quadro.tipo == crate::tela::TipoDeQuadro::Som {
                            Aviso::TelaSom {
                                tela,
                                bytes: quadro.bytes,
                                leitura,
                            }
                        } else {
                            Aviso::TelaQuadro {
                                tela,
                                chave: quadro.chave(),
                                bytes: quadro.bytes,
                                leitura,
                            }
                        };
                        if avisos.send(aviso).is_err() {
                            return;
                        }
                    }
                    Ok(None) => {
                        // **O fim limpo também é um `ParFalhou`.** Quem
                        // assiste não tem como distinguir «a transmissão
                        // acabou» de «o par calou»: as duas chegam como um
                        // fluxo que termina sem erro. E há três caminhos que
                        // terminam limpo com a transmissão ainda no ar — a
                        // contrapressão de [`PEDACOS_A_ESPERA_DO_PAR`], quem
                        // empresta reconectando ao servidor, e quem empresta
                        // saindo da sala. Calar aqui é tela em branco
                        // permanente, porque o cano do servidor para esta
                        // pessoa foi desligado quando o par foi apontado.
                        //
                        // Então reporta sempre, e deixa o **servidor**
                        // adjudicar: ele é o único que sabe se a transmissão
                        // ainda existe, e o braço de `ParFalhou` dele já não
                        // faz nada quando ela acabou de verdade.
                        //
                        // `ParouDeMandar` e não `CaiuNoMeio`: nada caiu. O par
                        // fechou o fluxo direito e simplesmente não manda
                        // mais. Nenhum dos dois desacredita ninguém
                        // (`crate::pares::quem_desacreditar`, no servidor), e
                        // o motivo é o que fica no rastro.
                        if let DeOndeVeioATela::Par { screen, resultados } = &de_onde {
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen: *screen,
                                motivo: MotivoDeFalhaDePar::ParouDeMandar,
                            });
                        }
                        break;
                    }
                    Err(erro) => {
                        // Um quadro torto encerra esta transmissão e não a
                        // conexão: o fluxo já perdeu o sincronismo, e continuar
                        // lendo dele é ler lixo. Quem transmite recomeça com um
                        // fluxo novo se quiser.
                        // `warn!`: o fluxo perdeu o sincronismo no meio, e do
                        // lado de quem assiste isso é a imagem congelando. O
                        // `TelaFechou` logo abaixo ao menos apaga o palco, que é
                        // mais do que o caso do cabeçalho tinha.
                        tracing::warn!(%erro, %tela, "a transmissão alheia terminou torta");
                        // **Do par, isto é o par tendo caído** — e quem
                        // assiste é a única pessoa que sabe. É este aviso que
                        // faz o servidor assumir, e sem ele a promessa da spec
                        // («ninguém perde imagem por causa da máquina de outra
                        // pessoa») ficaria escrita e não cumprida: a tela
                        // congelaria e ninguém do outro lado ficaria sabendo.
                        if let DeOndeVeioATela::Par { screen, resultados } = &de_onde {
                            let _ = resultados.send(ResultadoDoPar::ParFalhou {
                                screen: *screen,
                                motivo: MotivoDeFalhaDePar::CaiuNoMeio,
                            });
                        }
                        break;
                    }
                }
            }
            if let DeOndeVeioATela::Servidor(repasse) = &de_onde {
                repasse.fechou(tela);
            }
            // O `TelaFechou` sai do `Drop` de `_fim`, aqui.
        }
    }
}

/// O número da próxima leitura de tela, crescente no processo: o `leitura` de
/// [`Aviso::TelaAbriu`] e de [`Aviso::TelaFechou`].
/// O menor intervalo entre duas subidas do teto pela estimativa da mídia.
/// Ver [`Motor::medir_o_caminho`].
const SUBIDA_DO_TETO_PELA_MIDIA: Duration = Duration::from_secs(3);

/// O menor intervalo entre dois quadros-chave pedidos numa transmissão por
/// mídia. Ver [`Motor::pedir_a_chave_da_tela`].
const ENTRE_CHAVES_PELA_MIDIA: Duration = Duration::from_secs(2);

/// A estimativa de banda do RTP, na medida que o [`crate::tela::TetoDeVideo`]
/// espera como «caminho».
///
/// O teto gasta 60% do caminho (a regra da sonda, que mede o cano inteiro). A
/// estimativa do RTP já é o que **este fluxo** pode mandar, e a medida do
/// protótipo usou 85% dela (`docs/medidas/2026-11-10-rtp-sob-perda.md`). A
/// conversão faz os 60% do teto darem 85% da estimativa.
///
/// **Nunca abaixo do que dá o piso do vídeo.** Pela sonda, um caminho que não
/// carrega o piso para o vídeo, e ele volta quando a sonda mede de novo. Pela
/// mídia isso é um beco sem saída: a estimativa vem do que é mandado, e com o
/// vídeo parado ela não tem como subir (medido em 11/11/2026: uma queda
/// passageira da estimativa a 1 Mbps parou a imagem por 18 s). O vídeo segue no
/// piso, e a estimativa volta com ele.
fn para_a_medida_do_teto(estimativa_bps: u64) -> u32 {
    let caminho = u32::try_from(estimativa_bps.saturating_mul(85) / 60).unwrap_or(u32::MAX);
    // O teto gasta 60% do caminho: este é o caminho cujos 60% dão o piso.
    let do_piso = crate::tela::PISO_DE_BANDA_BPS.saturating_mul(100) / 60 + 1;
    caminho.max(do_piso)
}

/// O contrário de [`para_a_medida_do_teto`]: de um caminho, a estimativa
/// que o RTP teria de ter para o teto dar o mesmo.
fn da_medida_do_teto(caminho_bps: u32) -> u32 {
    u32::try_from(u64::from(caminho_bps).saturating_mul(60) / 85).unwrap_or(u32::MAX)
}

fn nova_leitura_de_tela() -> u64 {
    static PROXIMA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    PROXIMA.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Manda o [`Aviso::TelaFechou`] de uma leitura quando ela é solta — também
/// quando a tarefa é abortada, que é o caminho em que o fim não saía.
struct FimDaLeitura {
    avisos: mpsc::UnboundedSender<Aviso>,
    tela: ScreenId,
    leitura: u64,
}

impl Drop for FimDaLeitura {
    fn drop(&mut self) {
        let _ = self.avisos.send(Aviso::TelaFechou {
            tela: self.tela,
            leitura: self.leitura,
        });
    }
}

impl Motor {
    fn e_a_minha_tela(&self, tela: ScreenId) -> bool {
        self.tela_viva
            .as_ref()
            .is_some_and(|viva| viva.tela == tela)
    }

    /// O pedido guardado vira bomba, se este `ScreenShareStarted` for o dele.
    ///
    /// A guarda que mora aqui é uma só, e é a que precisa do [`Client`]: **é
    /// desta pessoa?** O quadro sai do barramento do servidor para a sala de voz inteira,
    /// e sem ela quem apenas assiste ligaria a captura da própria tela ao ver
    /// outro começar. As outras duas — já há uma viva, e o pedido existe — moram
    /// em [`Self::nascer_a_tela`], porque valem também para quem o chama de um
    /// teste.
    ///
    /// **A `Bomba` não vai para uma bomba a mais quando o servidor reenvia.** Ele
    /// reenvia `ScreenShareStarted` a cada pessoa que entra numa sala de voz onde já há
    /// transmissão, e quem transmite recebe o reenvio junto; o pedido já foi
    /// consumido no primeiro, então o segundo não acha nada.
    fn talvez_ligar_a_bomba(&mut self, pessoa: PersonId, tela: ScreenId) {
        let escoadouro = match self.cliente.as_ref() {
            Some(cliente) if cliente.session().person == pessoa => cliente.escoadouro_de_tela(),
            _ => return,
        };

        let avisos = self.avisos.clone();
        // Uma leitura por transmissão própria, como a das alheias.
        let leitura = nova_leitura_de_tela();
        self.nascer_a_tela(tela, move |origem, mut eventos, pedido_de_chave| {
            // Numa tarefa própria pelo mesmo motivo de `Comando::Anexar`:
            // escoar dura o que a transmissão durar, e fazê-lo dentro do laço
            // pararia quem lê a conexão e atende comandos até a pessoa parar de
            // compartilhar.
            tokio::spawn(async move {
                // O espelho: os mesmos avisos que uma tela alheia produz, pelo
                // mesmo caminho, para a casca não ter dois modos de desenhar a
                // mesma coisa. Quem compartilha era a única pessoa da sala que
                // não via o que estava mostrando — o servidor não devolve a
                // transmissão a quem a produziu, e com razão.
                //
                // **Da câmera, só a abertura.** Quem a codifica é a janela, que
                // tem a imagem na mão antes de ela ser comprimida: devolver-lhe
                // os quadros seria mandá-la decodificar o que ela mesma acabou
                // de codificar. A abertura vai, porque é ela que diz à janela
                // que o fluxo existe e que os quadros já têm para onde ir.
                let espelho = |visto: crate::bomba::EspelhoDaTela<'_>| {
                    let aviso = match visto {
                        crate::bomba::EspelhoDaTela::Abriu { largura, altura } => {
                            Aviso::TelaAbriu {
                                tela,
                                largura,
                                altura,
                                origem,
                                leitura,
                            }
                        }
                        crate::bomba::EspelhoDaTela::Quadro { .. }
                            if origem == seele_proto::screen::ScreenSource::Camera =>
                        {
                            return;
                        }
                        crate::bomba::EspelhoDaTela::Quadro { chave, bytes } => Aviso::TelaQuadro {
                            tela,
                            chave,
                            bytes: bytes.to_vec(),
                            leitura,
                        },
                    };
                    let _ = avisos.send(aviso);
                };
                let fim = escoadouro
                    .escoar_espelhado(tela, origem, &mut eventos, espelho, Some(&pedido_de_chave))
                    .await;
                let _ = avisos.send(Aviso::TelaFechou { tela, leitura });
                // Pelo mesmo motivo da de cima: são as duas linhas que contam o
                // fim de uma transmissão, uma vez cada, e as duas estavam
                // apagadas. A contagem do caminho bom diz quantos quadros
                // saíram e quantos o teto descartou — é o que separa «parou» de
                // «nunca andou».
                match fim {
                    Ok(contagem) => tracing::info!(?contagem, "a transmissão de tela acabou"),
                    Err(erro) => tracing::warn!(%erro, "a transmissão de tela caiu"),
                }
            });
        });
    }

    /// O pedido guardado vira bomba, e `escoar` recebe o canal dela.
    ///
    /// Quem escoa entra por fora, e a costura é o que torna esta máquina de
    /// estados afirmável: escrever no fio é a única metade daqui que precisa de
    /// uma conexão QUIC viva, e sem separá-la «pedido guardado → nome chegando →
    /// bomba nascendo» só seria exercível contra um servidor de verdade — que é o
    /// mesmo que dizer que nunca seria exercido.
    fn nascer_a_tela(
        &mut self,
        tela: ScreenId,
        escoar: impl FnOnce(
            seele_proto::screen::ScreenSource,
            mpsc::Receiver<crate::EventoDaBomba>,
            crate::bomba::PedidoDeChave,
        ),
    ) {
        if self.tela_viva.is_some() {
            return;
        }
        let Some((pedido, limites)) = self.tela_pedida.take() else {
            return;
        };
        self.semear_a_midia(&limites);

        let (biblioteca, captura, origem) = match *pedido {
            PedidoDeTela::DoSistema {
                biblioteca,
                captura,
                origem,
            } => (biblioteca, captura, origem),
            PedidoDeTela::DeFora(quadros) => {
                self.nascer_a_camera(tela, quadros, limites, escoar);
                return;
            }
        };
        let arranjo = crate::bomba::Arranjo {
            teto: self.teto_de_video(Some(&limites)),
            faixa: self.faixa_do_teto(),
            escolha_de_resolucao: limites.resolucao,
            cadencia: limites.cadencia,
            prioridade: limites.prioridade,
            com_som: limites.com_som,
        };

        // `|| {}` é a resposta que este crate consegue dar ao §2, e a bomba diz
        // por quê no cabeçalho dela: baixar a prioridade da thread pede
        // `setpriority`/`SetThreadPriority`, e `unsafe_code` é `forbid` neste
        // workspace. A ausência fica visível aqui em vez de escondida lá.
        let (bomba, eventos) = match crate::bomba::ligar(biblioteca, captura, arranjo, || {}) {
            Ok(ligada) => ligada,
            Err(erro) => {
                tracing::warn!(%erro, "a thread do codificador de tela não nasceu");
                return;
            }
        };

        escoar(origem, eventos, bomba.pedido_de_chave());
        self.tela_viva = Some(TelaViva {
            tela,
            produtor: Produtor::Bomba(bomba),
            limites,
        });
    }

    /// A câmera pedida vira transmissão: o fluxo abre com o teto de agora, e
    /// os quadros que a janela empurrar passam a ir a quem escoa.
    ///
    /// **O portão de versão, pela segunda vez.** Quem pede a câmera já foi
    /// conferido antes de o `StartScreenShare` sair (`Comando::CompartilharTela`),
    /// mas entre o pedido e o nome a conexão pode ter caído e voltado num
    /// servidor do protocolo 12 — que recusa a fonte 2 e deixaria anunciada
    /// uma transmissão sem fluxo. Aqui o pedido é largado: a entrada da janela
    /// passa a responder «fechada», e ela diz por quê.
    fn nascer_a_camera(
        &mut self,
        tela: ScreenId,
        quadros: crate::de_fora::QuadrosDeFora,
        limites: LimitesDeTela,
        escoar: impl FnOnce(
            seele_proto::screen::ScreenSource,
            mpsc::Receiver<crate::EventoDaBomba>,
            crate::bomba::PedidoDeChave,
        ),
    ) {
        let negociado = self.protocolo.load(std::sync::atomic::Ordering::Relaxed);
        if negociado < seele_proto::screen::PROTOCOLO_DA_CAMERA {
            tracing::warn!(
                negociado,
                exige = seele_proto::screen::PROTOCOLO_DA_CAMERA,
                "o servidor deu nome a uma câmera numa conexão que não a conhece; nada foi aberto"
            );
            return;
        }
        let teto = self
            .teto_de_video(Some(&limites))
            .teto(self.faixa_do_teto());
        let avisos = self.avisos.clone();
        let (alca, eventos, pedido_de_chave) = crate::de_fora::ligar_de_fora(
            quadros,
            teto,
            crate::tela::intervalo_de_quadro(limites.cadencia.hz()),
            move |sinal| {
                let _ = avisos.send(Aviso::Camera { tela, sinal });
            },
        );
        escoar(
            seele_proto::screen::ScreenSource::Camera,
            eventos,
            pedido_de_chave,
        );
        self.tela_viva = Some(TelaViva {
            tela,
            produtor: Produtor::DeFora(alca),
            limites,
        });
    }

    /// O teto de agora, com as três pernas do §5.1.
    ///
    /// As três, enfim: a de quem hospeda chega pelo `HostUplink`, o N pelo
    /// `ScreenViewers`, e a de quem compartilha sai da [`crate::caminho::Sonda`]
    /// — que é a pergunta 2 do §8 respondida, e não mais o cano das provas
    /// assumido para sempre. Enquanto a sonda não mediu nada, ela devolve
    /// exatamente aquela suposição, então a primeira transmissão de uma sessão
    /// abre com o mesmo teto de antes.
    /// Pede um quadro-chave a quem codifica a transmissão desta pessoa.
    ///
    /// **Por mídia, no máximo um a cada [`ENTRE_CHAVES_PELA_MIDIA`], e o que
    /// chega antes disso espera a vez em vez de sumir.** Um quadro-chave leva
    /// de 300 ms a um segundo para atravessar o caminho; cada pedido que chegava
    /// nesse meio-tempo (de quem assiste, do servidor) fazia outro, que atrasava
    /// o primeiro. Num caminho de 1 Mbps isso virava um quadro-chave atrás do
    /// outro: 30 em 20 s, e um quarto do tempo com a imagem parada
    /// (`docs/medidas/2026-11-10-rtp-no-produto.md`). O pedido adiado não se
    /// perde: quem entrou no meio da transmissão só é ligado num quadro-chave,
    /// e o servidor pede uma vez só por ele.
    ///
    /// Pelo canal nada muda: lá quem junta os pedidos é quem escoa.
    fn pedir_a_chave_da_tela(&mut self) {
        let Some(viva) = self.tela_viva.as_ref() else {
            return;
        };
        if self.estimativa_da_midia.is_none() {
            viva.produtor.chave();
            return;
        }
        let agora = Instant::now();
        let cedo = self
            .ultima_chave_pela_midia
            .is_some_and(|antes| agora.saturating_duration_since(antes) < ENTRE_CHAVES_PELA_MIDIA);
        if cedo {
            self.chave_devida = true;
            return;
        }
        self.ultima_chave_pela_midia = Some(agora);
        self.chave_devida = false;
        viva.produtor.chave();
    }

    /// Começa a ler uma transmissão que desce pela mídia RTP.
    fn assistir_pela_midia(&mut self, abertura: [u8; seele_proto::screen::SCREEN_HEADER_LEN]) {
        let Some(cliente) = self.cliente.as_ref() else {
            return;
        };
        match cliente.receber_tela_por_midia(abertura) {
            Ok(Some(recepcao)) => escoar_tela_alheia(
                self.avisos.clone(),
                FonteDaTelaAlheia::Midia(recepcao),
                DeOndeVeioATela::Servidor(Arc::clone(&self.repasse)),
            ),
            Ok(None) => tracing::warn!(
                "o servidor mandou uma transmissão pela mídia a uma conexão que não a ligou"
            ),
            Err(erro) => {
                // Como a abertura ilegível de um canal: a casca fica sabendo.
                tracing::warn!(
                    %erro,
                    "chegou pela mídia uma transmissão de tela que esta versão não sabe ler"
                );
                let _ = self.avisos.send(Aviso::TelaIlegivel {
                    motivo: erro.to_string(),
                });
            }
        }
    }

    /// A transmissão que vai nascer sobe pela mídia RTP? Então a estimativa
    /// de banda dela parte de onde a sonda partiria, e não dos 300 kbps de
    /// quem ainda não mandou nada (que ficam abaixo do piso do vídeo).
    fn semear_a_midia(&mut self, limites: &crate::video::LimitesDeTela) {
        self.estimativa_da_midia = None;
        let Some(remetente) = self.cliente.as_ref().and_then(Client::remetente_de_midia) else {
            return;
        };
        let semente = limites.semente_da_sonda_bps();
        remetente.semear_estimativa(u64::from(da_medida_do_teto(semente)));
        // Até onde a estimativa procura: o dobro de onde parte, que cobre a
        // banda escolhida com folga para o som e a retransmissão.
        remetente.taxa_desejada(u64::from(semente).saturating_mul(2));
        self.estimativa_da_midia = Some(semente);
        self.ultima_mudanca_da_midia = Some(Instant::now());
    }

    fn teto_de_video(
        &self,
        limites: Option<&crate::video::LimitesDeTela>,
    ) -> crate::tela::TetoDeVideo {
        let caminho = self
            .estimativa_da_midia
            .unwrap_or_else(|| self.caminho.estimativa());
        let mut teto = crate::tela::TetoDeVideo::com_caminho(caminho);
        // **A escolha é piso enquanto ninguém mediu, e teto sempre.**
        //
        // `caminho_de_quem_hospeda_bps` é `Some` só quando o `HostUplink` veio
        // maior que zero — medido ou declarado, porque `caminho_no_fio` não põe
        // hipótese no fio. Havendo número real, ele manda, para mais **e** para
        // menos: repetir o palpite por cima de uma medida seria insistir no que
        // a máquina já desmentiu.
        //
        // Não havendo, o padrão deixa de ser `CAMINHO_DA_PROVA_BPS` e passa a
        // sair da resolução escolhida. Sem isto a primeira transmissão de uma
        // sessão abre em 540p qualquer que seja a escolha — o teto fica em
        // 1200 kbps porque **esta** perna o prende —, e a escolha só começa a
        // valer oito janelas depois, quando a escada alcança o degrau.
        //
        // Fora de uma transmissão não há escolha, e aí a suposição volta: não
        // há resolução pedida de que derivar coisa nenhuma.
        let hospeda = self
            .caminho_de_quem_hospeda_bps
            .or_else(|| limites.map(crate::video::LimitesDeTela::caminho_inicial_bps));
        if let Some(bps) = hospeda {
            teto = teto.com_caminho_de_quem_hospeda(bps);
        }
        teto.com_espectadores(self.espectadores)
            .com_escolha(limites.and_then(|limites| limites.banda_bps))
    }

    /// Uma leitura do transporte para a sonda, e a ordem para a bomba quando a
    /// estimativa andou.
    ///
    /// Chamado em toda volta do laço, que é cinco vezes por segundo — a janela
    /// de amostragem é da sonda, não daqui, e ler mais vezes que o necessário
    /// custa uma cópia de contadores.
    ///
    /// **Só enquanto esta pessoa está compartilhando**, e a condição é a medida
    /// inteira: sem transmissão não há quem encha o cano, e o que sairia pelo
    /// soquete seria a voz — que diz que está bom a 40 kbps e não diz quanto
    /// cabe. É a frase do §8 pergunta 2, e é por isso que a resposta é a tela.
    fn medir_o_caminho(&mut self) {
        let (Some(cliente), Some(viva)) = (self.cliente.as_ref(), self.tela_viva.as_ref()) else {
            return;
        };
        // **Com a transmissão na mídia RTP, quem mede o caminho é ela** (ADR
        // 0067): a estimativa do TWCC vê a perda e o atraso de cada pacote, e
        // não lê perda sorteada como congestionamento. A sonda por `Ping`
        // fica para os servidores antigos.
        if let (Some(antes), Some(remetente)) =
            (self.estimativa_da_midia, cliente.remetente_de_midia())
        {
            if let Some(bps) = remetente.estimativa_bps() {
                let agora = para_a_medida_do_teto(bps);
                self.caminho_medido
                    .store(agora, std::sync::atomic::Ordering::Relaxed);
                // **Mudar o teto custa um quadro-chave**: quem codifica é
                // reconfigurado, e o codificador recomeça de uma imagem
                // inteira. A estimativa do RTP muda a cada retorno; repassar
                // cada mudança fazia chover quadro-chave, e num caminho de
                // 1 Mbps a rajada deles derrubava a própria estimativa
                // (medido em 11/11/2026: de 3,3 Mbps a 260 kbps em 6 s, sem
                // perda). Então:
                // - **cair é na hora**, mas só de 15% para mais;
                // - **subir é devagar**: 25% para mais, e no máximo uma vez a
                //   cada [`SUBIDA_DO_TETO_PELA_MIDIA`].
                let caiu = agora < antes.saturating_sub(antes / 7);
                let pode_subir = self
                    .ultima_mudanca_da_midia
                    .is_none_or(|quando| quando.elapsed() >= SUBIDA_DO_TETO_PELA_MIDIA);
                let subiu = pode_subir && agora > antes.saturating_add(antes / 4);
                if caiu || subiu {
                    self.estimativa_da_midia = Some(agora);
                    self.ultima_mudanca_da_midia = Some(Instant::now());
                    self.reconferir_o_teto();
                }
            }
            return;
        }
        let amostra = crate::caminho::Amostra {
            transporte: cliente.amostra_do_transporte(),
            teto: self.teto_de_video(Some(&viva.limites)).teto(self.faixa),
            faixa: self.faixa,
        };
        let sem_medida_antes = self.caminho.janelas_sem_medida();
        if self.caminho.observar(Instant::now(), &amostra).is_some() {
            self.reconferir_o_teto();
        }
        // **O produto sabe e conta**: a primeira janela sem medida de dor, e
        // depois uma a cada minuto, vão ao log com o que faltou. Sem isto uma
        // tela presa no cano das provas seria um mistério.
        let sem_medida = self.caminho.janelas_sem_medida();
        if sem_medida > sem_medida_antes && (sem_medida == 1 || sem_medida.is_multiple_of(60)) {
            tracing::warn!(
                janelas = sem_medida,
                ida_e_volta = ?amostra.transporte.ida_e_volta,
                teto_bps = crate::caminho::TETO_SEM_MEDIDA_BPS,
                "a sonda de subida fechou janela sem medida de dor (sem perda, congestionamento \
                 nem ida e volta fresca): ela não sobe, e fica no cano das provas"
            );
        }
        // **Fora do `if`, e de propósito.** O `Some` acima diz que a estimativa
        // *mudou*; o que quem grava a lista de conhecidos quer é o valor de
        // agora, e uma sessão inteira sem mudança nenhuma continua tendo um
        // número que vale a pena lembrar.
        //
        // **Mas só o medido.** Uma sonda que só carrega a semente ou a
        // suposição publica zero, que quem grava lê como «não mediu» e não
        // regrava: sem isto a semente de ontem voltava ao disco com a data de
        // hoje, e a lembrança se renovava sem nunca ter sido confirmada.
        if let Some(medida) = self.caminho.estimativa_medida() {
            self.caminho_medido
                .store(medida, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// A faixa que o teto do vídeo segue agora: a do sinal no canal, e a
    /// nominal enquanto a transmissão vai pela mídia ([`faixa_para_o_teto`]).
    const fn faixa_do_teto(&self) -> SignalBand {
        faixa_para_o_teto(self.faixa, self.estimativa_da_midia.is_some())
    }

    /// Conta à bomba que o teto andou.
    ///
    /// Uma ordem só para as três coisas que o mexem, porque o N já mora dentro
    /// do [`crate::tela::TetoDeVideo`] pela perna de quem hospeda.
    fn reconferir_o_teto(&self) {
        let Some(viva) = self.tela_viva.as_ref() else {
            return;
        };
        let teto = self.teto_de_video(Some(&viva.limites));
        viva.produtor.teto(teto, self.faixa_do_teto());
    }

    /// Mata a bomba, se houver uma.
    fn parar_a_tela(&mut self) {
        self.estimativa_da_midia = None;
        self.ultima_mudanca_da_midia = None;
        self.ultima_chave_pela_midia = None;
        self.chave_devida = false;
        // A contagem morre junto: ela é de **uma** transmissão, e deixá-la de pé
        // faria a próxima nascer dividindo a perna de quem hospeda pelo público
        // da anterior — um teto apertado sem que ninguém estivesse assistindo.
        self.espectadores = 0;
        // E a janela da sonda também: entre esta transmissão e a próxima o cano
        // fica vazio, e uma janela que atravessasse esse buraco mediria o
        // silêncio. A estimativa fica — o cano é o mesmo.
        self.caminho.esquecer_a_conexao();
        matar(self.tela_viva.take());
    }

    /// O que o servidor diz que muda a sala de voz que a reconexão refaz.
    ///
    /// **Pela mesma porta que o `Room` da casca**, [`EntradasPendentes::ouvir`]
    /// (`crate::assento`): o operador que move, a sala apagada, a recusa de
    /// entrada e a confirmação dela. A casca senta a pessoa, na volta, onde
    /// `Aviso::Reconectado` diz que este motor a sentou, e abre o áudio por esse
    /// assento; as duas contas têm de dar o mesmo.
    ///
    /// A recusa desfaz só o pedido de entrada que está no ar. Esquecer a sala a
    /// cada recusa, como este braço fazia, deixava a volta sem sala — e a casca
    /// com o áudio fechado — para quem o servidor manteve sentado: a troca de
    /// sala recusada, a volta numa sala com senha, o apelido tomado. Sem esquecer
    /// nada, como antes disso, a volta refazia a entrada numa sala apagada ou
    /// que tinha acabado de recusar.
    ///
    /// [`EntradasPendentes::ouvir`]: crate::assento::EntradasPendentes::ouvir
    fn ouvir_o_assento(&mut self, mensagem: &ServerMessage) {
        self.entradas.ouvir(&mut self.voice_room, mensagem);
    }

    /// Guarda o que a reconexão vai ter que refazer.
    fn lembrar(&mut self, comando: &Comando) {
        match comando {
            Comando::LembrarCaminho(bps) => {
                // **Grampeada, e não publicada como medida.** Quem lembra pode
                // ter lido um número sem data nem rede (a casca não é obrigada
                // a passar por `CaminhoLembrado::semente`), e o teto da semente
                // vale aqui também. E `caminho_medido` fica como estava: uma
                // semente que ninguém confirmou não volta ao disco como medida.
                let semente = (*bps).min(crate::caminho::TETO_DA_SEMENTE_BPS);
                tracing::info!(
                    lembrado = bps,
                    semente,
                    "a sonda começa do caminho lembrado deste servidor"
                );
                self.caminho = crate::caminho::Sonda::partindo_de(semente);
            }
            // Pelas entradas pendentes, como o `Room` da casca: o pedido fica no
            // ar até o servidor decidir. Sem conexão, ele é só a intenção que a
            // volta refaz, e a volta esquece a fila.
            Comando::EntrarNaVoiceRoom(voice_room, _password) => {
                self.entradas.pedir(&mut self.voice_room, *voice_room);
            }
            Comando::SairDaVoiceRoom => self.entradas.sair(&mut self.voice_room),
            Comando::AbrirLinha(linha) => self.linha = Some(*linha),
            Comando::Muted(ligado) => self.muted = *ligado,
            Comando::Isolamento(ligado) => self.isolamento = *ligado,
            // Fazer uma sala e moderar alguém **não** entram aqui, e a ausência
            // é deliberada nos dois casos. O que se refaz ao reconectar é onde
            // a pessoa estava — a sala de voz, a Linha, os dois silêncios —, porque
            // voltar sem isso é voltar para outro lugar. Fazer uma sala é coisa
            // que se faz uma vez; repetida depois de uma queda, ela apareceria
            // minutos mais tarde do nada, e duplicada se a pessoa já tivesse
            // pedido de novo à mão. Expulsar é pior: refeito depois de cinco
            // minutos de bateria, derrubaria de novo alguém que já tinha
            // voltado, e ninguém entenderia por quê.
            //
            // Nomear o servidor e dar-lhe uma imagem também não entram, pelo
            // primeiro motivo: são coisas que se fazem uma vez. Refeito depois
            // da queda, um `RenomearServer` desfaria o nome que **outra pessoa**
            // pôs nos cinco minutos em que este cliente esteve fora — e o nome
            // do servidor é de todo mundo que está dentro, ao contrário da sala de voz em
            // que esta pessoa estava sentada.
            //
            // Apagar é o pior dos três, e por isso vale escrevê-lo: refeito
            // depois da queda, ele destruiria a sala que alguém fez no lugar da
            // que sumiu — e a confirmação que autorizou o primeiro pedido dizia
            // o tamanho de **outro** estrago. Pesar uma Linha também não volta:
            // é uma pergunta, e a resposta que interessava era a de quando a
            // caixa estava aberta.
            //
            // **Compartilhar a tela é o que menos pode voltar dos dois.** Refeito
            // depois de cinco minutos de bateria, ele poria o monitor de alguém
            // no ar sem que ninguém tivesse apertado nada, e minutos depois de a
            // pessoa ter desistido — a captura já morreu na queda, por
            // [`Self::cair`], e ressuscitá-la seria a única coisa neste enum que
            // liga uma câmera sozinha.
            _ => {}
        }
    }

    /// A saída pedida: cala o caminho entre pares, fecha a conexão e fica de
    /// pé no período de fechamento, antes de o motor voltar.
    ///
    /// # O que consertou a pendência 50, e o que é daqui
    ///
    /// `Client::disconnect` só põe o `CONNECTION_CLOSE` na fila do `quinn`: quem
    /// o escreve no soquete é o driver da conexão, uma tarefa deste mesmo
    /// runtime. [`Enlace::sair`] voltava sem esperar o motor, e quem o chamava
    /// derrubava o runtime — a thread da conexão da FFI faz isso na linha
    /// seguinte — com o fechamento ainda na fila: o servidor só via a saída
    /// pelo prazo de ociosidade, 20 s. **O conserto é `sair` esperar o motor.**
    /// Com isso, o fechamento sai na primeira volta do driver, mesmo que este
    /// motor voltasse logo depois de [`Self::encerrar`] (medido no lote F: os
    /// dois braços de volta a `encerrar`, e os três testes de
    /// `saida_com_despedida.rs` seguem verdes).
    ///
    /// # A ordem, e a prova de cada passo
    ///
    /// 1. A ponta é guardada antes de `encerrar` largar o cliente: é nela que
    ///    se espera.
    /// 2. O caminho entre pares cai, e as tarefas dele devolvem
    ///    ([`TarefasDePar::largar_tudo_e_esperar`]). O `Encerrado` é o último
    ///    aviso que a casca ouve desta sessão, e o motor fica de pé depois dele,
    ///    no passo 4: um par vivo nesse intervalo escreveria quadro atrás do
    ///    fim. **O cancelamento é provado**: sem ele,
    ///    `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para` recebe
    ///    quadro depois do `Encerrado` — é a janela do passo 4 que deixa o
    ///    quadro a caminho. A espera pelas alças não é (ver a função).
    /// 3. `encerrar` fecha a conexão e diz o fim à casca, como antes.
    /// 4. O `wait_idle`, até [`ESPERA_DA_DESPEDIDA`]: o período de fechamento
    ///    do QUIC, para reenviar o `CONNECTION_CLOSE` se o primeiro se perder.
    ///    **Sem prova**: o laço local não perde pacote — ver a constante.
    async fn despedir(&mut self) {
        let ponta = self.cliente.as_ref().map(Client::ponta);
        self.tarefas_de_par
            .largar_tudo_e_esperar(ESPERA_DAS_TAREFAS_DE_PAR)
            .await;
        self.encerrar(Motivo::Pedido).await;
        if let Some(ponta) = ponta {
            // O `close` do enlace pôs o corte no `ctl` e o fechamento na fila
            // da ponta; espera-se a ponta ficar sem conexão viva, e então ela
            // fecha o socket desta sessão — só o dela.
            let esvaziou = tokio::time::timeout(ESPERA_DA_DESPEDIDA, async {
                while ponta.ponta().conexoes_vivas() > 0 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await;
            ponta.ponta().fechar();
            if esvaziou.is_err() {
                // Não é falha: o fechamento saiu na primeira volta do driver, e
                // o que o teto cortou foi o escoamento, que só responde a
                // pacote atrasado.
                tracing::debug!(
                    "a despedida saiu e o escoamento da conexão passou do teto da espera"
                );
            }
        }
    }

    async fn encerrar(&mut self, motivo: Motivo) {
        // Antes de derrubar a conexão: a bomba fecha o fluxo dela ao morrer, e
        // o fim do fluxo é a segunda maneira de dizer «parei» (§3.6). Derrubar
        // primeiro trocaria isso por um fluxo cortado.
        self.tela_pedida = None;
        self.parar_a_tela();
        // Os túneis dizem por que acabaram **antes** do `Encerrado`: depois
        // dele a casca já não espera relatório nenhum.
        self.tuneis
            .derrubar_e_esperar(crate::tunel::Queda::ServidorSaiu, Duration::from_secs(1))
            .await;
        if let Some(mut cliente) = self.cliente.take() {
            cliente.disconnect();
        }
        let _ = self.avisos.send(Aviso::Encerrado(motivo));
    }
}

/// A recusa que `entrar_no_tunel` e `servir_tunel` devolvem quando o verbo não
/// saiu: o servidor que negociou 11 é `ServidorAntigo` (nada foi escrito); o
/// fluxo de controle fechado é `SemResposta`.
fn recusa_do_verbo(erro: &crate::client::NaoSaiu) -> RecusaDeTunel {
    match erro {
        crate::client::NaoSaiu::ServidorAntigo => RecusaDeTunel::ServidorAntigo,
        crate::client::NaoSaiu::Escrita(_) => RecusaDeTunel::SemResposta,
    }
}

/// Mata uma transmissão, se houver uma.
///
/// Livre e não método porque o caminho de parar corre com um empréstimo do
/// [`Client`] vivo na mão, e um `&mut self` o atropelaria.
fn matar(viva: Option<TelaViva>) {
    let Some(viva) = viva else {
        return;
    };
    // `Bomba::parar` junta a thread do codificador, e juntar **bloqueia**. A
    // casca gráfica roda este motor num runtime de thread única, e bloquear aqui
    // pararia junto a tarefa que escoa — que é justamente quem tem de esvaziar o
    // canal para a thread conseguir entregar o `Fim` dela. Na piscina de bloqueio
    // o mesmo fim não custa nada ao laço.
    let bomba = match viva.produtor {
        Produtor::Bomba(bomba) => bomba,
        // Sem thread a juntar: a tarefa ouve o fim e fecha o fluxo sozinha.
        Produtor::DeFora(alca) => {
            alca.parar();
            return;
        }
    };
    match tokio::runtime::Handle::try_current() {
        Ok(runtime) => {
            runtime.spawn_blocking(move || bomba.parar());
        }
        // Fora de um runtime — um teste que monta o motor à mão. Aqui não há
        // tarefa nenhuma para proteger do bloqueio.
        Err(_) => bomba.parar(),
    }
}

/// Confere o que o convite prometeu contra o que o servidor ofereceu.
///
/// Devolve o veredito quando a conexão pode seguir, e o erro quando ela tem que
/// cair. Uma função à parte de [`Enlace::conectar`] porque tudo aqui é decisão
/// sobre valores, e sem isso a fiação inteira ficava sem guarda.
///
/// Os cinco desfechos são exercidos por teste, sem servidor do outro lado — e os
/// dois de `PinDecision::Matches` importam tanto quanto os de primeiro contato:
/// é neles que mora a política de **não** derrubar. Um link velho contra um
/// servidor já conhecido, num alvo de escopo público, avisa e segue, porque o
/// TOFU já provou que é o mesmo servidor de ontem; recusar ali trancaria a
/// pessoa para fora de um servidor que ela usa. (Onde o pino não prova o
/// servidor, o verificador já recusou o mesmo caso dentro do TLS, e ele nem
/// chega aqui; ver [`Destino::o_pino_prova_o_servidor`].) Enquanto `Matches`
/// não tinha teste, alargar esta função para recusar também nesse caso
/// passava a suíte inteira.
fn conferir(
    destino: &Destino,
    pin: &PinDecision,
    pins: &dyn PinStore,
) -> Result<Verdict, ConnectError> {
    let veredito = match (
        &destino.esperada,
        verdict(pin, destino.impressao_esperada()),
    ) {
        // **O código conferiu dentro do TLS** (`ConfereCodigo`): a chave é a do
        // código, como a impressão de um link prova um certificado. Um primeiro
        // contato por ele é conferido, e não cego, e a casca não pergunta «confia
        // nesta chave?» a quem acabou de prová-la.
        (Some(Esperada::Codigo(_)), Verdict::FirstContact { fingerprint }) => {
            Verdict::FirstContactVerified { fingerprint }
        }
        (_, veredito) => veredito,
    };

    // O efeito vem **antes** de qualquer saída, e por isso está aqui e não
    // dentro do `if`: o verificador fixa a chave dentro do retorno de chamada
    // do TLS, então devolver o erro sem desfazer o pin deixaria a visita
    // seguinte — sem link para conferir — ver `Matches` e entrar sem hesitar no
    // servidor que acabou de ser rejeitado.
    aplicar_veredito(&veredito, pins, &destino.chave_do_pin);

    if let Verdict::InviteRefused { expected, offered } = &veredito {
        return Err(ConnectError::InviteMismatch {
            expected: expected.clone(),
            offered: offered.clone(),
        });
    }
    Ok(veredito)
}

/// Aplica o que o veredito manda fazer com o pin.
///
/// Separado da decisão porque a decisão é uma tabela pura e isto é um efeito.
/// Só a recusa tem efeito: ela desfaz o pin que o verificador escreveu antes
/// de alguém poder julgar.
///
/// # Por que apagar aqui é seguro
///
/// `InviteRefused` nasce de três decisões, e só a primeira chegaria aqui. De
/// `PinDecision::FirstContact` — nada estava fixado antes, então o `unpin`
/// remove exatamente o que este aperto de mão acabou de escrever. De
/// `PinDecision::InviteRefused` ela não chega: o verificador recusa essa chave
/// sem fixá-la, e ela sobe como erro, sem virar veredito. De
/// `PinDecision::Changed`, e **essa** apagaria um pin antigo e legítimo, que é
/// o oposto do ADR 0003; ela não chega porque o verificador reprova a chave
/// trocada no TLS e a falha sobe como [`ConnectError::PinChanged`], sem nunca
/// virar veredito. A recusa de um candidato fixado cujo pino não prova o
/// servidor (`PinDecision::InviteRefused` com pin) apagaria do mesmo jeito o
/// pin antigo daquele endereço. Se algum dia uma dessas duas passar a chegar
/// até aqui, esta função precisa distinguir de onde veio a recusa antes de
/// apagar nada.
///
/// # Segunda linha, desde que a impressão é conferida no TLS
///
/// O verificador recebe a impressão esperada e recusa dentro do aperto de mão,
/// sem fixar nada e antes do `Hello`, o primeiro contato que não confere
/// (`PinDecision::InviteRefused`, que sobe como [`ConnectError::InviteMismatch`]).
/// Pelo caminho de produção, então, um `FirstContact` que desmente o convite não
/// chega mais aqui. Este braço fica como segunda linha: se o verificador algum
/// dia deixar de recusar, a conexão ainda cai e o pin ainda é desfeito. Tarde,
/// depois do `Hello`, mas cai.
///
/// **E ele desfaz sem perguntar de quem é o pin.** Quando era o único caminho,
/// isso era exato. Numa corrida de candidatos com a mesma chave de pino, um pin
/// que estivesse ali poderia ser de um vizinho. A limpeza de primeira linha,
/// em `Enlace::conectar_por`, só desfaz o que o próprio aperto anotou
/// ([`desfazer_o_pin_deste_aperto`]); se este braço um dia voltar a ser o
/// caminho da recusa, ele precisa da mesma cautela.
fn aplicar_veredito(veredito: &Verdict, pins: &dyn PinStore, chave_do_pin: &str) {
    if matches!(veredito, Verdict::InviteRefused { .. }) {
        pins.unpin(chave_do_pin);
    }
}

/// A loja de pins como **um** aperto de mão a vê: repassa tudo à loja de
/// verdade e anota o que ele escreveu.
///
/// Existe porque «o que este aperto escreveu» não se deduz olhando a loja antes
/// e depois. Candidatos da mesma corrida (ADR 0037) compartilham `chave_do_pin`
/// quando são alternativos do mesmo nome de convite, e um pin que apareceu entre
/// a leitura de antes e a de depois pode ser de um deles. Quem desfaz um pin
/// tem de desfazer só o que sabe que foi seu.
#[derive(Debug)]
struct PinsDesteAperto {
    loja: Arc<dyn PinStore>,
    escrito: std::sync::Mutex<Option<String>>,
}

impl PinsDesteAperto {
    fn nova(loja: Arc<dyn PinStore>) -> Self {
        Self {
            loja,
            escrito: std::sync::Mutex::new(None),
        }
    }

    /// A impressão que este aperto fixou, se fixou alguma.
    fn o_que_escreveu(&self) -> Option<String> {
        self.escrito.lock().ok().and_then(|escrito| escrito.clone())
    }
}

impl PinStore for PinsDesteAperto {
    fn pinned(&self, host: &str) -> Option<String> {
        self.loja.pinned(host)
    }

    fn pin(&self, host: &str, fingerprint: String) {
        if let Ok(mut escrito) = self.escrito.lock() {
            *escrito = Some(fingerprint.clone());
        }
        self.loja.pin(host, fingerprint);
    }

    fn unpin(&self, host: &str) {
        self.loja.unpin(host);
    }
}

/// Desfaz o pin que **este** aperto de mão escreveu e que sobrou dele.
///
/// O `TofuVerifier` fixa a chave dentro do TLS, quando aceita o certificado, e
/// o aperto de mão continua depois disso: o resto do próprio TLS (a assinatura
/// do servidor sobre o aperto, que prova que quem atendeu tem a chave privada
/// do certificado, e o `Finished`) e, com o TLS terminado, abrir o fluxo de
/// controle, o prazo, a credencial, a resposta. Qualquer uma dessas saídas
/// devolve erro com o pin já escrito. Inclusive a de um impostor que repete o
/// certificado de um servidor (o certificado é público) sem ter a chave dele: o
/// verificador aceita o certificado e fixa, e o TLS cai na assinatura.
///
/// O que isso estragava: um aperto aceita a chave `A` num primeiro contato (sem
/// impressão esperada, por exemplo), fixa, e falha depois. `A` fica fixado sem
/// que conexão nenhuma tenha subido com ela. Na tentativa seguinte, com um link
/// que promete `B`, a decisão é `Matches { A }`, e num alvo de escopo público (o
/// único lugar em que o pino prova o servidor) o veredito vira
/// `InviteDisagrees` em vez da recusa — a conexão é **permitida**, sem desfazer
/// nada e sem erro. Uma falha de aperto de mão convertia a conferência de
/// *recusar* para *avisar*, para sempre, naquele endereço. Onde o pino não
/// prova o servidor, a esperada que não confere é recusada mesmo com ele, e o
/// estrago que sobra é o de qualquer endereço: uma volta sem impressão esperada
/// trata `A` como já conhecida.
///
/// # Só o que este aperto escreveu, e só se ainda estiver lá
///
/// `escrito` é o que [`PinsDesteAperto`] anotou. Sem nada anotado, não há o que
/// desfazer: a recusa pela impressão (`InviteMismatch`), a chave trocada
/// (`PinChanged`) e toda falha de um aperto que não chegou a aceitar um
/// primeiro contato não escrevem (o verificador só fixa num `FirstContact`), e
/// um pin que exista na hora é de outro. Com algo anotado, o pin só cai se a
/// loja ainda guardar aquele valor: um vizinho que tenha fixado outra coisa
/// depois fica com o que é dele.
///
/// **O que ela não distingue:** um vizinho da mesma chave de pino que só
/// confirmou o pin que este aperto escreveu. É o mesmo servidor por dois
/// endereços do convite com o mesmo nome, nenhum com pin antes da corrida: o
/// verificador deste aperto aceita a chave primeiro e a fixa; o do vizinho
/// aceita depois, vê o pin que este escreveu e o confirma (`Matches`), sem
/// escrever nada. A condição é os dois verificadores aceitarem, e não o TLS
/// terminar. Se este aperto falhar depois disso e o vizinho vencer, o pin ainda
/// é o que este anotou, e cai: o vencedor fica sem o pin do servidor em que
/// acabou de entrar. Fechar isso pede saber quem venceu, que é o que a limpeza
/// de depois da corrida, [`desfazer_pin_orfao`], sabe e esta não.
///
/// # Quando apaga, diz
///
/// Uma linha `info!` com a chave do pino, a impressão apagada e a falha do
/// aperto (`falha`). «Por que meu pino sumiu» não tinha resposta no
/// `seele.log`. Quando não apaga nada, não escreve nada.
///
/// A frase diz só o que vale em toda saída: o aperto falhou depois de o
/// verificador aceitar a chave, no resto do TLS ou depois dele. Em qual dos
/// dois, quem diz é `falha`, e quase todo erro aponta um só: `ChaveNaoProvada`,
/// `TlsRefused` e `SemResposta` saem do próprio TLS
/// (`client::classify_connection_error`); `HandshakeTimeout`, `Refused`,
/// `ProtocolViolation` e `ModsNaoAceitos`, do aperto do protocolo, depois dele.
/// `Unreachable` pode ser dos dois: a conexão que caiu no meio do TLS, ou o
/// fluxo de controle e um quadro do aperto do protocolo que não passaram depois
/// dele; ali, o `warn!` de `crate::client` que vem antes desta linha no
/// `seele.log` diz qual. Com `ChaveNaoProvada`, foi o próprio TLS que caiu, na
/// assinatura de quem atendeu, como no impostor de cima, e ali dizer que o TLS
/// terminou faria quem investiga achar que quem atendeu provou ter a chave. Com
/// `TlsRefused`, o TLS também caiu, por outro alerta.
fn desfazer_o_pin_deste_aperto(
    pins: &dyn PinStore,
    chave_do_pin: &str,
    escrito: Option<&str>,
    falha: &ConnectError,
) {
    let Some(escrito) = escrito else {
        return;
    };
    if pins.pinned(chave_do_pin).as_deref() == Some(escrito) {
        pins.unpin(chave_do_pin);
        tracing::info!(
            chave = %chave_do_pin,
            impressao = %escrito,
            erro = %falha,
            "desfiz o pin que este aperto de mão fixou: o aperto falhou depois de o \
             verificador aceitar a chave (no resto do TLS ou depois dele), e o pin deixado \
             faria a visita seguinte tratar esta chave como já conhecida"
        );
    }
}

/// Desfaz o pin que sobrou de um aperto de mão que não terminou, quando não se
/// sabe o que ele escreveu.
///
/// É a limpeza de **depois da corrida** (ver `Enlace::tentar_entre`), para o
/// candidato cujo prazo estourou: a tentativa foi derrubada no meio, levou
/// consigo a anotação de [`PinsDesteAperto`], e o que resta é comparar a loja com
/// a fotografia de antes da corrida. Um aperto que falhou por conta própria já
/// se limpou em `Enlace::conectar_por`, com [`desfazer_o_pin_deste_aperto`].
///
/// Só apaga o que **ninguém** tinha fixado antes: se já havia pin, ele fica.
/// Quando apaga, diz no log (`info!`) a chave, a impressão apagada e a falha do
/// candidato (`falha`), como [`desfazer_o_pin_deste_aperto`].
///
/// **O que ela não distingue:** qual candidato escreveu o pin, quando dois
/// perdedores dividem a chave de pino. A chave é `host:porta` do nome do
/// convite, e alternativos do mesmo nome a dividem; a limpeza só pula a chave
/// do vencedor. Ela roda por perdedor, na ordem em que as falhas chegaram, e o
/// primeiro daquela chave apaga o pin órfão com a falha **dele** na linha. Com
/// `A`, que escreveu o pin e teve o prazo estourado, e `B`, na mesma chave,
/// que falhou antes sem escrever nada (por exemplo, por ter achado o pin de
/// `A` e só o confirmado), a limpeza de `B` vem primeiro, apaga o pin de `A` e
/// põe na linha a falha de `B`: a linha do pin órfão leva a falha do vizinho.
/// O pin que cai é o certo; só a atribuição erra. A falha de cada candidato,
/// com o endereço, está na linha da corrida
/// ([`registrar_a_falha_do_candidato`]).
///
/// **Os dois resíduos falham para o lado seguro**: este e o de
/// [`desfazer_o_pin_deste_aperto`], o vencedor que fica sem o pin do servidor
/// em que acabou de entrar. Nos dois, o que pode sair errado é um pin a menos
/// ou uma linha que atribui mal, e nunca um pin que nenhuma conexão confirmou
/// ficando na loja. Um pin a menos faz a visita seguinte àquela chave ser um
/// primeiro contato, conferido pela impressão esperada quando houver uma; um
/// pin órfão que ficasse faria dela um `Matches`, e num alvo de escopo público
/// a recusa da impressão viraria aviso, que é o estrago que as duas limpezas
/// existem para desfazer.
fn desfazer_pin_orfao(
    pins: &dyn PinStore,
    chave_do_pin: &str,
    fixado_antes: Option<&str>,
    falha: &ConnectError,
) {
    if fixado_antes.is_some() {
        return;
    }
    if let Some(orfao) = pins.pinned(chave_do_pin) {
        pins.unpin(chave_do_pin);
        tracing::info!(
            chave = %chave_do_pin,
            impressao = %orfao,
            erro = %falha,
            "desfiz um pin órfão depois da corrida: ninguém o tinha fixado antes dela, e o \
             candidato que perdeu não terminou o aperto de mão"
        );
    }
}

/// Diz no log por que um candidato da corrida não entrou.
///
/// Com vencedor ou sem. As falhas dos perdedores eram descartadas sem uma linha
/// quando outro candidato entrava, e entre elas está o evento que a
/// conferência da impressão existe para barrar: um servidor com a chave errada
/// num endereço da corrida, como o que o quarto devolveu.
///
/// - A impressão que não confere (`InviteMismatch`) e a chave fixada que mudou
///   (`PinChanged`) vão em `warn!`, com o endereço e as duas impressões:
///   impressão não é segredo, e sem as duas ninguém confere nada.
/// - Quem mostrou o certificado sem provar a chave (`ChaveNaoProvada`) também
///   vai em `warn!`, com o endereço: é o mesmo evento por outro caminho, e em
///   `info!` ele se lia como um endereço que só não respondeu.
/// - As outras falhas vão em `info!`, que é o nível que o `seele.log` grava.
///
/// `entrou` é o endereço que venceu, quando algum venceu: a linha diz que a
/// recusa aconteceu e que a conversa subiu por outro lado.
///
/// O verificador também registra as duas recusas (`crate::tofu::TofuVerifier::decide`),
/// sob a chave de pino e com o porquê; esta linha é a da corrida, com o
/// endereço e o desfecho.
fn registrar_a_falha_do_candidato(
    onde: Option<SocketAddr>,
    falha: &ConnectError,
    entrou: Option<SocketAddr>,
) {
    match falha {
        ConnectError::InviteMismatch { expected, offered } => tracing::warn!(
            ?onde,
            esperada = %expected,
            ofertada = %offered,
            ?entrou,
            "este candidato não é o servidor que a impressão esperada promete: recusado \
             dentro do TLS, antes do `Hello`"
        ),
        ConnectError::PinChanged { pinned, offered } => tracing::warn!(
            ?onde,
            fixada = %pinned,
            ofertada = %offered,
            ?entrou,
            "a chave fixada deste candidato mudou: recusado dentro do TLS, antes do `Hello` \
             (ADR 0003)"
        ),
        // Sem impressões: a que ele mostrou é a do certificado que repetiu, e
        // a linha do pin desfeito (`desfazer_o_pin_deste_aperto`) já a traz
        // quando o verificador chegou a fixá-la.
        ConnectError::ChaveNaoProvada => tracing::warn!(
            ?onde,
            ?entrou,
            "este candidato mostrou um certificado sem provar ter a chave dele: recusado \
             dentro do TLS, antes do `Hello`"
        ),
        outra => tracing::info!(?onde, erro = %outra, ?entrou, "este endereço do convite não deu"),
    }
}

/// Conta um candidato a quem estiver observando, se alguém estiver.
///
/// Um `send` recusado é silêncio de propósito: o canal só fecha quando quem
/// observava desistiu, e a conexão não pode depender de uma tela estar viva —
/// é a mesma regra que faz o `watch` da chegada ignorar o `send` sem ouvinte.
fn contar(olhos: Option<&mpsc::UnboundedSender<Andamento>>, andamento: Andamento) {
    if let Some(olhos) = olhos {
        let _ = olhos.send(andamento);
    }
}

/// Uma volta da bateria interna: a ponta nova desta sessão, a discagem ao
/// endereço que atendeu, e o degrau 4 quando ela não responde.
///
/// A mesma conferência da entrada, e aqui ela é a única: esta volta não passa
/// por `conferir`. O pin normalmente já existe e decide sozinho; a esperada pesa
/// quando o pin sumiu entre a queda e a volta, e sem ela a reconexão fixaria às
/// cegas quem atendesse. A volta é ao endereço que venceu, e o pino dele prova
/// o servidor, ou não, como na entrada.
///
/// # Errors
///
/// O [`ConnectError`] da discagem ou do aperto, como na entrada.
async fn voltar(
    destino: &Destino,
    bilhete: Option<&seele_proto::uri::Bilhete>,
    chave: &SigningKey,
    pins: &Arc<dyn PinStore>,
) -> Result<Client, ConnectError> {
    let ponta = crate::transporte::abrir_ponta(None)?;
    let direto = crate::client::discar(
        ponta.ponta(),
        vec![destino.servidor],
        None,
        PRAZO_POR_CANDIDATO,
    )
    .await;
    let conexao = match direto {
        Ok(conexao) => conexao,
        // Só o silêncio pede o degrau 4: uma recusa é de quem respondeu.
        Err(ConnectError::SemResposta) if bilhete.is_some() && e_publico(destino.servidor.ip()) => {
            let chamada = match bilhete {
                Some(bilhete) => {
                    crate::encontro::Chamada::preparar(bilhete, destino.marcas()).await
                }
                None => None,
            };
            let Some(chamada) = chamada else {
                return Err(ConnectError::SemResposta);
            };
            let atendida = chamada
                .chamar(ponta.ponta(), crate::encontro::PRAZO_DA_CHAMADA)
                .await
                .map_err(falha_da_chamada)?;
            let alvos = std::iter::once(atendida.endereco)
                .chain(atendida.dicas.iter().copied())
                .collect();
            let primeira = crate::client::discar(
                ponta.ponta(),
                alvos,
                Some(PeloChama {
                    credencial: atendida.credencial,
                    espera_checagem_ipv6: false,
                }),
                PRAZO_POR_CANDIDATO,
            )
            .await;
            match primeira {
                Ok(conexao) => conexao,
                // O anfitrião atendeu e o IPv4 não abriu: a segunda chamada,
                // com o IPv6 desta máquina, como na entrada
                // ([`Enlace::tentar_o_lote`]).
                Err(ConnectError::SemResposta) => {
                    let Some(Ok(atendida)) = chamada
                        .chamar_pelo_ipv6(
                            ponta.ponta(),
                            crate::encontro::PRAZO_DA_CHAMADA,
                            &[destino.servidor],
                        )
                        .await
                    else {
                        return Err(ConnectError::NatNaoAbriu);
                    };
                    crate::client::discar(
                        ponta.ponta(),
                        alvos_da_segunda_volta(destino.servidor, &atendida),
                        Some(PeloChama {
                            credencial: atendida.credencial,
                            espera_checagem_ipv6: true,
                        }),
                        PRAZO_POR_CANDIDATO,
                    )
                    .await
                    .map_err(|erro| match erro {
                        ConnectError::SemResposta => ConnectError::NatNaoAbriu,
                        outro => outro,
                    })?
                }
                Err(outro) => return Err(outro),
            }
        }
        Err(erro) => return Err(erro),
    };
    Client::connect_sobre(
        ponta,
        conexao,
        &destino.chave_do_pin,
        destino.esperada.as_ref(),
        destino.o_pino_prova_o_servidor,
        &destino.apelido,
        chave,
        Arc::clone(pins),
        destino.segredo.as_deref(),
        destino.aceito.as_deref(),
        // A volta não corre candidatos: ela é ao endereço que venceu.
        None,
    )
    .await
}

/// Refaz, na conexão nova de uma reconexão, o que a pessoa tinha escolhido
/// antes da queda: a sala de voz, a Linha, o mudo, o isolamento total.
///
/// Para na primeira chamada que falhar, e não tenta as seguintes: a chamada
/// que falhou já prova que esta conexão nova não serve, e continuar mandando
/// coisa por ela só empilharia erro em cima de erro. Quem chama trata
/// qualquer `Err` daqui como o mesmo caso de um `Client::connect_por` que não
/// deu — a tentativa não conta como reconexão, e a bateria continua.
///
/// # Errors
///
/// Falha se qualquer uma das chamadas não conseguir sair — tipicamente porque
/// a conexão que acabou de completar o aperto de mão já morreu de novo.
async fn restabelecer(
    cliente: &mut Client,
    voice_room: Option<(VoiceRoomId, u64)>,
    resgatada: Option<VoiceRoomId>,
    linha: Option<ChannelId>,
    muted: bool,
    isolamento: bool,
) -> anyhow::Result<()> {
    if let Some((voice_room, numero)) = voice_room {
        // A senha não é guardada para a reconexão: uma sala com senha exigiria
        // pedi-la de novo aqui, e isso é o mesmo gap que existe para toda
        // credencial nesta struct — fora do escopo desta correção, que é só
        // não engolir a falha do restabelecimento. O que muda em relação ao
        // `let _ =` de antes é que a recusa por senha ausente agora conta como
        // tentativa que não deu, em vez de passar por reconexão bem-sucedida.
        //
        // **E a recusa não tira ninguém do assento resgatado.** O servidor
        // devolve o assento de quem cai e volta dentro da carência, e recusa
        // esta entrada numa sala com senha; ela é um pedido no ar como outro
        // qualquer, com o `Ping` marcado atrás, e a recusa devolve o assento
        // de antes dele — o resgatado. Ver `crate::assento`.
        cliente.enter_voice_room(voice_room, None).await?;
        cliente.send_ping_de_entrada(numero).await?;
    } else if resgatada.is_some() {
        // A pessoa saiu da sala durante a queda, e o servidor, que não soube,
        // a sentou de novo. A saída que ela pediu vale.
        cliente.leave_voice_room().await?;
    }
    if let Some(linha) = linha {
        cliente.join_channel(linha).await?;
    }
    if muted {
        cliente.set_muted(true).await?;
    }
    if isolamento {
        cliente.set_total_isolation(true).await?;
    }
    Ok(())
}

/// Se este endereço é de uma rede privada — a de casa **ou** a de outra casa.
///
/// Sem loopback de propósito: `127.0.0.1` não é uma rede de ninguém, e as duas
/// perguntas que se fazem com isto aqui (precisa de furo? merece prazo curto?)
/// têm resposta própria para ele.
///
/// Pública por causa de uma terceira pergunta, que a FFI faz sobre o endereço
/// resolvido do alvo ao montar o destino: o pino dele prova o servidor
/// ([`Destino::o_pino_prova_o_servidor`])? Num endereço privado, não: ele é o
/// mesmo de uma casa para outra, e quem atende nele depende da rede em que a
/// pessoa está. O loopback fica de fora ali também, e de propósito: ele é esta
/// máquina em qualquer rede, e o pino dele prova o servidor como o de um
/// endereço público.
///
/// # Por que `to_canonical` na entrada
///
/// Porque a forma mapeada **é** o caso comum, não a borda. Um ponto de encontro
/// atrás de um socket de pilha dupla enxerga a origem de quem bateu como
/// `::ffff:a.b.c.d`, e é essa origem que volta no `AQUI` e vira o candidato
/// refletido do convite. Este crate já sabe disso — `encontro::mapear` existe
/// exatamente por causa dela.
///
/// Sem canonizar, `::ffff:192.168.1.5` não casava com nenhum dos ramos: o de
/// IPv4 nem era consultado, e o de IPv6 comparava `0x0000` contra `fc00`/`fe80`.
/// O endereço da rede de casa de alguém passava por público, queimava três
/// furos da janela do anfitrião, vazava metadado que ninguém pediu e ainda
/// levava o prazo cheio de quatro segundos em vez de um.
#[must_use]
pub fn e_privado(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(quatro) => quatro.is_private() || quatro.is_link_local() || e_cgnat(quatro),
        IpAddr::V6(seis) => {
            let primeiro = seis.segments().first().copied().unwrap_or(0);
            // `fc00::/7`, os endereços locais únicos da RFC 4193, e `fe80::/10`,
            // o link-local — o par do `169.254.x.x` do outro lado.
            (primeiro & 0xfe00) == 0xfc00 || (primeiro & 0xffc0) == 0xfe80
        }
    }
}

/// Se um candidato depende de alguém ter furado o caminho até ele.
///
/// A negação de "privado, loopback, sem destino ou multicast". Endereços de
/// documentação (`203.0.113.x`, TEST-NET-3) contam como públicos, e é de
/// propósito: eles são globais em tudo que importa aqui — o sistema os roteia
/// para a porta de saída, e é isso que o furo cobre.
///
/// # As três últimas perguntas são feitas na forma escrita, e não na canônica
///
/// `e_privado` canoniza porque a forma mapeada de um endereço privado aparece no
/// campo — é assim que o ponto de encontro reflete a origem de quem está atrás
/// de pilha dupla. Loopback, "sem destino" e multicast **não** são canonizados,
/// e isso é uma escolha, com preço conhecido.
///
/// O que ela compra: um candidato que é público para esta função e que ainda
/// assim responde numa máquina só — `[::ffff:127.0.0.1]:porta`, um socket de
/// verdade no loopback. É o que torna possível medir que o `Initial` sai
/// **depois** do `LEVE`, e por quanto; sem ele isso exigiria duas casas e um NAT
/// entre elas.
///
/// O que ela custa, e não é hipótese: o loopback mapeado **pode** entrar num
/// convite. `alcance::anunciar_com_porta` empurra o endereço refletido para a
/// lista conferindo só a família contra a pilha da escuta, sem filtro de
/// loopback nenhum — então um servidor cujo ponto de encontro roda na mesma
/// máquina, atrás de socket de pilha dupla, observa `::ffff:127.0.0.1` como
/// origem e publica isso. Quando acontece, quem entra gasta
/// [`AVISOS_POR_CANDIDATO`] furos da janela do anfitrião (três, não um),
/// 3 × 96 bytes de metadado que ninguém pediu, e [`ESPERA_DO_FURO`] a mais antes
/// do aperto de mão; o servidor fura contra o próprio loopback, e a conexão sobe
/// assim mesmo, porque o candidato sempre foi alcançável sem furo nenhum.
///
/// O que ela **não** custa é segurança, e é por isso que o preço é aceitável: o
/// destino do furo é `bilhete.aviso()`, fixado em `Batida::preparar` e embutido
/// no datagrama. O candidato decide apenas **se** o `LEVE` sai, nunca **para
/// onde** o anfitrião fura. Um candidato mal classificado não redireciona pacote
/// contra terceiro nenhum.
pub(crate) fn e_publico(ip: IpAddr) -> bool {
    !e_privado(ip) && !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast()
}

/// `100.64.0.0/10`, que a RFC 6598 reservou para CGNAT.
fn e_cgnat(quatro: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = quatro.octets();
    a == 100 && (64..128).contains(&b)
}

/// Qual falha a pessoa vê quando nenhum candidato entra.
///
/// Um convite traz vários endereços do mesmo servidor e eles correm juntos
/// (ADR 0006). Quando todos falham sobra escolher **uma** para mostrar, e a
/// ordem de chegada é a pior régua possível: um endereço morto falha em
/// milissegundos, enquanto quem respondeu alguma coisa gastou o aperto de mão
/// inteiro para responder. Por ordem de chegada, o «não alcancei» ganha quase
/// sempre.
///
/// Era código solto dentro de `conectar_entre`, e a decisão que ele toma não
/// tinha teste nenhum — foi assim que [`ConnectError::ModsNaoAceitos`] entrou
/// na enumeração sem entrar na regra. Aqui ela tem nome e é chamável de um
/// teste.
#[derive(Debug, Default)]
struct FalhaAMostrar {
    /// A primeira falha de alguém que disse alguma coisa sobre o mundo.
    respondeu: Option<ConnectError>,
    /// A primeira de todas, por ordem de conclusão.
    primeira: Option<ConnectError>,
}

impl FalhaAMostrar {
    /// Registra mais um candidato que não deu.
    fn anotar(&mut self, falha: ConnectError) {
        if self.respondeu.is_none() && alguem_respondeu(&falha) {
            self.respondeu = Some(falha.clone());
        }
        if self.primeira.is_none() {
            self.primeira = Some(falha);
        }
    }

    /// A que sobra. Sem candidato nenhum, «não alcancei» é a verdade.
    fn escolhida(self) -> ConnectError {
        self.respondeu
            .or(self.primeira)
            .unwrap_or(ConnectError::Unreachable)
    }
}

/// A bateria de uma sessão: cinco minutos, e um numa ligação.
///
/// Uma ligação é o destino que espera um código (ADR 0060). A volta dela é ao
/// `IP:porta` que atendeu, e não ao código: o `destino` do motor é o do
/// candidato que venceu a corrida, e a corrida não se repete. A carga é a
/// tolerância da campainha ([`crate::battery::CARGA_DA_LIGACAO`]): dentro dela
/// quem volta entra sem tocar de novo, e passado ela a ligação caiu.
fn bateria_de(destino: &Destino) -> Battery {
    match destino.esperada {
        Some(Esperada::Codigo(_)) => Battery::da_ligacao(),
        Some(Esperada::Certificado(_)) | None => Battery::new(),
    }
}

/// Se este erro veio de alguém que **respondeu**.
///
/// A diferença decide qual erro sobra quando nenhum candidato entra. Um servidor
/// que recusou o convite, ou cuja chave mudou, disse alguma coisa sobre o
/// mundo; um "não alcancei" de um endereço que nunca ia voltar não disse nada,
/// e mostrá-lo no lugar do outro manda a pessoa procurar problema de rede
/// enquanto o servidor está ali, recusando.
///
/// # `match` exaustivo, e sem braço `_`
///
/// Os dois predicados deste par decidem **o que a pessoa vê** e **se a bateria
/// insiste**, e nenhum dos dois tem como cair num padrão por omissão sem
/// alguém perceber: foi exatamente o que aconteceu com
/// [`ConnectError::ModsNaoAceitos`], que entrou na enumeração e ficou de fora
/// das duas decisões. O irmão dela do lado das despedidas —
/// [`a_sessao_acabou_aqui`] — já era preso por um teste exaustivo; este lado
/// não era preso por nada. Agora uma variante nova **não compila** até alguém
/// escrever de que lado ela cai.
fn alguem_respondeu(erro: &ConnectError) -> bool {
    match erro {
        ConnectError::PinChanged { .. }
        | ConnectError::InviteMismatch { .. }
        | ConnectError::ChaveNaoProvada
        | ConnectError::Refused { .. }
        | ConnectError::TlsRefused
        | ConnectError::ProtocolViolation => true,
        // Respondeu, e disse o número do protocolo dele: é o que a pessoa
        // precisa ler para abrir a versão certa.
        ConnectError::OutraLinha { .. } => true,
        // **Respondeu, e respondeu a coisa mais informativa do conjunto:** a
        // lista inteira do que exige. Sem isto ela perde a corrida de
        // `conectar_entre` para o `Unreachable` de um endereço morto — que
        // falha rápido, enquanto o anúncio exige o aperto de mão inteiro — e a
        // casca recebe «não alcancei» sem ter o que mostrar, contra o contrato
        // da própria entrega: a tela mostra, a pessoa aceita, a casca guarda.
        ConnectError::ModsNaoAceitos { .. } => true,
        // Alguém atendeu com uma chave que não é a do código: a mesma família
        // da impressão que não confere, e o que a pessoa precisa ler.
        ConnectError::CodigoNaoConfere(_) => true,
        // Não é de candidato nenhum: sai antes da corrida, quando o quarto não
        // deu onde conectar, e nenhuma tentativa o produz.
        ConnectError::NaoLocalizou(_) => false,
        // Não é de candidato nenhum: é quem liga, pela `Chegada`, que larga a
        // corrida inteira (tarefa 16 da F7).
        ConnectError::Cancelada => false,
        // **O ponto respondeu e o anfitrião atendeu o `CHAMA`**: diz mais que o
        // silêncio de um endereço morto, e é o que a pessoa precisa ler.
        ConnectError::NatNaoAbriu => true,
        // A linha 1 respondeu, e disse que é a linha 1.
        ConnectError::LinhaLegada => true,
        ConnectError::LocalEndpoint
        | ConnectError::Unreachable
        | ConnectError::HandshakeTimeout
        | ConnectError::SemResposta => false,
    }
}

/// Se insistir pode dar em outra coisa.
///
/// Uma credencial rejeitada ou um banimento não mudam de resposta por
/// repetição; uma queda de rede muda.
///
/// Exaustivo pela razão que [`alguem_respondeu`] escreve.
fn vale_insistir(erro: &ConnectError) -> bool {
    match erro {
        ConnectError::PinChanged { .. }
        | ConnectError::Refused { .. }
        | ConnectError::InviteMismatch { .. } => false,
        // Quem atendeu não provou ter a chave do servidor que mostrou. Insistir
        // é perguntar de novo a quem já não provou, e é o comportamento que um
        // impostor espera.
        ConnectError::ChaveNaoProvada => false,
        // O servidor fala outro número, e continua falando na próxima batida:
        // quem resolve é abrir o app da versão dele, ou atualizar este.
        ConnectError::OutraLinha { .. } => false,
        // **Só uma pessoa desfaz isto**, e é a metade simétrica do que
        // `a_sessao_acabou_aqui` já dizia para `DisconnectReason::ModsMudaram`:
        // a reconexão encontra o mesmo anúncio e o mesmo não. Insistir manda
        // `RecusarMods` ao servidor a cada batida por cinco minutos, e deixa a
        // casca dizendo «reconectando» a quem precisa ler uma lista e
        // responder.
        ConnectError::ModsNaoAceitos { .. } => false,
        // Quem atendeu não é a sala do código: perguntar de novo dá a mesma
        // chave, como com a impressão que não confere.
        ConnectError::CodigoNaoConfere(_) => false,
        // Não chega aqui hoje: a volta da bateria é ao `IP:porta` que atendeu,
        // e não ao código. Se um dia chegar, o que passa com o tempo é a rede —
        // o ponto mudo, o nome que não resolveu, a recusa por excesso, a
        // resposta pela metade. A sala que não está registrada, o ponto
        // anterior ao ENC/2 e a falta de ponto não mudam por repetição.
        ConnectError::NaoLocalizou(falha) => matches!(
            falha,
            crate::encontro::FalhaAoLocalizar::PontoMudo
                | crate::encontro::FalhaAoLocalizar::PontoNaoResolve
                | crate::encontro::FalhaAoLocalizar::PontoRecusou
                | crate::encontro::FalhaAoLocalizar::SoAEscuta
        ),
        // Quem liga desistiu: insistir seria ligar de novo contra a vontade de
        // quem cancelou. Não chega à volta hoje, que ninguém cancela.
        ConnectError::Cancelada => false,
        // Uma 0.16 continua 0.16 na próxima batida.
        ConnectError::LinhaLegada => false,
        // O NAT pode reabrir (o anfitrião trocou de rede, o mapeamento expirou
        // e voltou): a bateria insiste, como insiste no silêncio.
        ConnectError::NatNaoAbriu => true,
        ConnectError::LocalEndpoint
        | ConnectError::Unreachable
        | ConnectError::TlsRefused
        | ConnectError::HandshakeTimeout
        | ConnectError::SemResposta
        | ConnectError::ProtocolViolation => true,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "num teste, o pânico é o relatório")]
/// A corrida do RFC 8305, provada sem socket.
///
/// Estes testes nasceram com ela (`e09d537a`) e saíram por engano no revert
/// `b4f5ef94`, que tirava outra coisa. De 31/08 a 09/10 a corrida não teve
/// prova nenhuma, e o doc dela dizia que tinha.
mod a_corrida;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod o_debug_do_destino;

/// A máquina de jogo do lado de quem a opera (API 8 parte 3), contra um
/// servidor de mentira que aperta a mão, manda o que o teste pede e devolve o
/// que ouve.
#[cfg(test)]
mod a_maquina;

#[cfg(test)]
mod a_medida_do_teto_pela_midia;
