//! SEELE headless client.
//!
//! All session, protocol, audio and state logic lives here. This crate is
//! headless and testable without any interface (`specs/01-arquitetura.md`).
//!
//! # One core, three shells
//!
//! The TUI, the desktop app and the mobile app are presentation layers over the
//! same state machine. None of them contains business logic. This crate exposes
//! a state machine that consumes commands and emits events:
//!
//! ```text
//! Command  →  [ seele-core ]  →  Event
//! ```
//!
//! Each shell only translates events into pixels and input into commands.
//!
//! **This boundary is the most important contract in the project.** If a feature
//! has to be implemented twice in two different interfaces, it is in the wrong
//! place. If the boundary leaks, the project becomes three applications.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod aceites;
/// O aparelho de mentira do `seele-audio` (só de teste), para a casca o ver pela
/// cadeia app -> ffi -> core -> audio. Só com o recurso; o `seele-audio` recusa
/// compilá-lo num build otimizado.
#[cfg(feature = "aparelho-de-mentira")]
pub mod aparelho_de_mentira {
    pub use seele_audio::device::mentira::{contagem, id, Contagem};
}
pub mod agenda;
pub mod anexo_no_disco;
pub use seele_sessao::assento;
/// O base64 de ida e de volta, de onde o desktop e o navegador o leem.
pub use seele_sessao::base64;
/// A bateria interna mora no `seele-sessao` desde a Tarefa 2.6 da F4: o
/// navegador volta da queda pela mesma conta (o tempo entra como parâmetro).
pub use seele_sessao::battery;
/// Os cargos de cada sessão (ADR 0064), de onde o desktop e o navegador os
/// leem: o ffi não vê o `seele-proto` (ADR 0002). Linha de registro de F.
pub use seele_sessao::cargos;
/// O catálogo de MODs sem a rede: a assinatura, a leitura, o plano e o
/// conjunto, de onde o desktop e o navegador os leem (Tarefa 2.6 da F4).
pub use seele_sessao::catalogo;
/// Os arquivos que uma pessoa escolheu para um MOD: as regras do seletor e o
/// guarda, de onde o desktop e o navegador os leem.
pub use seele_sessao::escolhidos;
/// A mídia que um MOD recebe do servidor, pronta, com a feature `reducao`.
#[cfg(feature = "reducao")]
pub use seele_sessao::midia_de_mod;
/// A redução de uma imagem de MOD ao uso, com a feature `reducao`.
#[cfg(feature = "reducao")]
pub use seele_sessao::reducao;
/// O retrato da sala: os tipos de vista do `Snapshot` e a projeção que os
/// monta a partir do `Room`, para o desktop e o navegador (Tarefa 2.2 da F4).
pub use seele_sessao::retrato;
/// O piso do que um som de MOD ocupa decodificado, de onde o desktop e o
/// navegador o leem (Tarefa 2.6 da F4).
pub use seele_sessao::som;
/// Os compassos da volta (a página do buraco, a rajada e o passo da
/// repetição), de onde o desktop e o navegador os leem.
pub use seele_sessao::volta;
pub mod bomba;
pub mod caminho;
pub mod chegada;
pub mod client;
pub mod confere_codigo;
pub mod conhecidos;
pub use seele_sessao::despedida;
pub mod encontro;
pub mod enlace;
pub mod entrada;
pub mod frame;
pub mod identity;
pub mod mods;
pub mod par;
pub mod preferences;
/// O juiz das prévias mora no `seele-sessao` desde a Tarefa 2.6 da F4, como a
/// busca: é a mesma regra no desktop e no navegador.
pub use seele_sessao::preview;
#[cfg(test)]
mod rastro_de_teste;
/// A busca no que a casca mostra mora no `seele-sessao` desde a Tarefa 2.6 da
/// F4: é a mesma regra no desktop e no navegador, e o navegador não leva o
/// `seele-core`.
pub use seele_sessao::search;
pub mod som_que_segue;
pub use seele_sessao::state;
pub mod tela;
pub mod teste_de_microfone;
pub mod tofu;
pub mod transporte;
pub mod video;
pub mod voice;

pub use battery::{Battery, Link};
pub use bomba::{
    escoar, ligar, Arranjo, Bomba, Contagem, Escoadouro, EventoDaBomba, Ordem, QuadroParaOFio,
    NOME_DA_THREAD,
};
/// A medida do caminho de subida de quem compartilha.
///
/// Reexportada porque é a resposta à pergunta 2 do §8 da spec de tela, e quem
/// desenha o painel de uma transmissão precisa poder dizer de onde o teto veio
/// — um número medido e um número suposto merecem frases diferentes.
pub use caminho::{Amostra as AmostraDoCaminho, Sonda, Transporte as TransporteDoCaminho};
pub use client::{
    AttachmentRequest, Client, ConnectError, FlowControl, LinkTrust, MediaChannel, Previewed, Sent,
    SessionInfo, Transfers,
};
pub use ed25519_dalek::SigningKey;
pub use identity::FilePinStore;
pub use preview::{
    check_server_icon, IconRefusal, ImageFormat, Verdict as PreviewVerdict, PREVIEW_LIMIT,
};
pub use state::{
    Changed, ChavePedida, Ended, EstadoDoEnvio, Message, Notice, Pendente, Person, Room, Tela,
    TransferNotice, VoiceRoomSync,
};
pub use tela::{
    menor_resolucao, resolucao_estimada_para, Envio, ErroDeTela, MotivoDeDescarte, MotivoDeParada,
    PernaQueAperta, QuadroRecebido, Recepcao, Teto, TetoDeVideo, Transmissao,
};
pub use tofu::{MemoryPinStore, PinDecision, PinStore, Verdict};
/// A tela: o trait, quem o cumpre nesta máquina, e o que se escolhe.
///
/// **[`Captura`] estava exportado sem ninguém que o cumprisse**, e era o degrau
/// que faltava para o botão de compartilhar deixar de responder «este build não
/// sabe»: uma casca não pode implementar o trait — não vê `seele-video` — e não
/// tinha o que passar para a bomba.
pub use video::{
    fontes_de_tela, pedir_permissao_de_tela, permissao_de_tela, Ajuste, Captura, CapturaDoSistema,
    CapturaEmCaixa, CapturaRecusou, Compartilhamento, ErroDeCompartilhamento, ErroDeFontes,
    FonteDeQuadros, FonteDeTela, LimitesDeTela, Passo as PassoDeVideo, PedidoDeTela,
    PermissaoDeTela,
};

pub use seele_audio::device::{consentimento_do_microfone, ConsentimentoDoMicrofone};
/// O toque da sala pessoal: abre só a saída, numa linha própria, e para quando
/// cai. Reexportado porque a casca toca quando a campainha toca, e o ADR 0002
/// não a deixa ver `seele-audio`.
pub use seele_audio::device::{DeviceError, Tocador};
pub use seele_audio::gate::{
    ABERTURA_COM_SUPRESSAO_DBFS, ABERTURA_SEM_SUPRESSAO_DBFS, FAIXA_DE_ABERTURA_DBFS,
};
/// O volume padrão do toque: audível, e longe do teto da escala.
pub use seele_audio::toque::VOLUME as VOLUME_DO_TOQUE;
/// As medidas de um degrau na proporção da fonte e o teto de banda da prova:
/// o navegador compartilha a tela com cópias deles (`seele_web::tela`, Tarefa
/// 3.10 da F4), e a conformidade as confere por aqui, que é por onde a regra de
/// dependências deixa um teste ver o `seele-video`.
pub use seele_video::codec::{medidas_para, TETO_DA_PROVA_BPS};
/// O módulo do Cisco e os degraus que a pessoa escolhe.
///
/// Reexportados porque [`PedidoDeTela`] pede os três e o ADR 0002 não deixa uma
/// casca ver `seele-video`. [`BibliotecaDeVideo`] vem pronta de quem chama pelo
/// argumento que `seele_video::modulo::procurar_em` escreve: onde os arquivos do
/// produto moram é decisão da casca, não desta biblioteca.
pub use seele_video::codec::{Cadencia, Resolucao};
pub use seele_video::modulo::{
    instalar_em as instalar_modulo_de_video, procurar_em as procurar_modulo_de_video,
    publicado_para_este_sistema as modulo_de_video_publicado, ModuloPublicado,
};
/// O que esta máquina faz com o próprio áudio ao capturar uma tela — R21.
///
/// Reexportado porque a resposta é da plataforma e a pergunta é da tela: o ADR
/// 0002 não deixa a casca ver `seele-video`, e é a casca que tem de explicar a
/// limitação antes de alguém escolher compartilhar um monitor.
pub use seele_video::{exclusao_do_som_deste_processo, exclusao_medida_no_sistema, ExclusaoDoSom};
pub use seele_video::{BibliotecaDeVideo, ErroDeVideo};
pub use voice::{
    capture_devices, playback_devices, seguir_o_aparelho, Acompanhamento, AparelhosAbertos,
    CaptureDevice, DeviceChoice, DeviceRates, EstadoDoAparelho, EstadoDoAudio, PassoDoAparelho,
    PlaybackDevice, TrocaFeita, Voice, VoiceMode,
};

/// The surface a shell is allowed to see.
///
/// ADR 0002 keeps `seele-ffi` from depending on `seele-proto` or
/// `seele-audio` directly. That is not bureaucracy: a shell that can name an
/// `ssrc` has protocol knowledge in it, and the same knowledge would then have
/// to be written twice more for the desktop and mobile shells.
///
/// So everything a shell legitimately needs is re-exported here, deliberately
/// and one item at a time. Adding to this list is the moment to ask whether the
/// shell needs the value or the decision behind it — usually it is the decision,
/// and that belongs in the core.
pub use seele_audio::telemetry::{AudioTelemetry, LocalTelemetry, SourceTelemetry};
/// O código de uma pessoa: o «número de telefone» da sala pessoal (ADR 0060).
///
/// Reexportado porque a casca o mostra em SEU CÓDIGO, o confere no ATENDER, o
/// guarda na agenda e o passa à campainha da sala, e o ADR 0002 não a deixa ver
/// `seele-proto`. Tipado até a borda: lido uma vez só, por [`Codigo::ler`].
pub use seele_proto::codigo::{Codigo, CodigoInvalido};
pub use seele_proto::control::{
    // Reexportados porque a casca escreve a frase de um
    // `AlertReason::PedidoRecusado` pelo nome do pedido e pela causa, e o ADR
    // 0002 não a deixa ver `seele-proto`.
    nome_do_pedido,
    AlertReason,
    AlertSeverity,
    AttachmentInfo,
    AttachmentRefusal,
    AttachmentState,
    CausaDaRecusa,
    ChannelInfo,
    // Reexportado porque a casca declara o consentimento do caminho entre pares
    // e o ADR 0002 não a deixa ver `seele-proto`. Mesmo motivo do
    // `key_fingerprint` logo abaixo.
    ConsentimentoDePar,
    DisconnectReason,
    // Reexportado com a lista do FORA DO AR (`ServerMessage::Membros`), que a
    // ponte dobra e entrega fora do `Snapshot`.
    Membro,
    // Reexportado porque a casca desenha por que uma mensagem não foi gravada,
    // e é o vocabulário fechado do fio que diz o motivo. Mesmo caso do
    // `AttachmentRefusal` acima.
    MessageRefusal,
    Permission,
    PersonProfile,
    PersonState,
    Presence,
    ServerMessage,
    Subsystem,
    SubsystemHealth,
    Telemetry,
    // Reexportado porque o `seele-ffi` leva o tema do servidor ao retrato da
    // casca (tarefa 17d da F3), e o ADR 0002 não o deixa ver `seele-proto`.
    TemaDoServidor,
    VoiceRoomInfo,
};
/// A impressão digital de uma chave pública, no formato que o produto mostra.
///
/// Reexportada porque a casca precisa dela para se reconhecer no próprio servidor
/// — quem hospeda tem de estar admitido antes de bater na própria porta — e a
/// regra de dependência do ADR 0002 não deixa a casca ver `seele-proto`.
pub use seele_proto::transport::key_fingerprint;
/// A recusa de um fluxo de volume de MOD — ADR 0048.
///
/// Reexportada porque o `seele-ffi` a espelha para a casca e não enxerga
/// `seele-proto`, pelo mesmo ADR 0002 que já traz os enumerados de anexo daqui.
pub use seele_proto::volume::VolumeRefusal;

pub use seele_proto::control::MAX_BODY_LEN;
/// O teto do apelido, e também da senha de uma sala de voz, em **bytes**.
///
/// Reexportado para a FFI conferir a senha antes de pedir a entrada (R1-M4 da
/// revisão do lote P61): uma senha acima dele não cabe no fio, e o núcleo a
/// recusaria sem que o `Room` da casca soubesse qual entrada desfazer.
pub use seele_proto::control::MAX_NICKNAME_LEN;
/// Os outros tetos dos campos que a casca escreve: o nome de um canal ou de
/// uma sala, quantas pessoas uma sala nova pode ter, o nome do servidor e o
/// motivo de um banimento (os textos em **bytes**).
///
/// Reexportados para a FFI os entregar à janela (`Connection::limites_dos_campos`,
/// pendência 58, item 55; lote C55 da F5). A janela contava caracteres, e o fio
/// conta bytes: um nome acentuado cabia no campo e não no quadro, e o núcleo o
/// recusava sem a pessoa saber. Daqui, e não de uma cópia: a regra do ADR 0002
/// não deixa a casca ver o `seele-proto`, e um número copiado é o que envelhece
/// calado.
pub use seele_proto::control::{
    MAX_ALERT_TEXT_LEN, MAX_CHANNEL_NAME_LEN, MAX_CLIENT_NAME_LEN, MAX_VOICE_ROOM_LIMIT,
};
pub use seele_proto::ids::{
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, RoleId, ScreenId, SessionId,
    Ssrc, VoiceRoomId,
};
/// O cabeçalho de abertura de uma transmissão de tela, e o que ele carrega.
///
/// Reexportado porque a casca é quem sabe **o que** a pessoa escolheu
/// compartilhar — um monitor ou uma janela — e em que resolução, e a regra de
/// dependência do ADR 0002 não deixa a casca ver `seele-proto`. Note o que
/// **não** está aqui: quadros por segundo. §5, e é decisão e não esquecimento —
/// a tela não promete a escolha, e um número que a casca pudesse escrever no
/// cabeçalho seria a casca prometendo.
pub use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource};
pub use seele_proto::signal::{Signal, SignalBand, SyncInputs};
pub use seele_proto::transport::DEFAULT_PORT;
pub use seele_proto::uri;
// `linha_de` e `Linha` com o `ConnectError::OutraLinha`: a FFI diz à casca se
// quem respondeu é de uma versão que o launcher abre ou mais nova que esta.
pub use seele_proto::version::{linha_de, linha_do_protocolo, Linha, VERSAO_DO_SERVIDOR};
pub use seele_proto::PROTOCOL_VERSION;

pub use seele_proto::volume::{PedidoDeImagem, VolumeHeader};
