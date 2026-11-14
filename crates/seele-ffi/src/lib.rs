//! **SEELE**, as one object a graphical shell can hold.
//!
//! `specs/06-clientes-gui.md` specifies this surface and states the rule it
//! exists to enforce: "Nenhuma funcionalidade nasce aqui: se algo é útil, é
//! implementado em `seele-core` e aparece nas duas interfaces." Nothing in this
//! crate decides anything. It owns a thread, a connection and a
//! [`seele_core::Room`], and it turns method calls into commands and server
//! events into notifications.
//!
//! # The shape, and why it is this shape
//!
//! ADR 0018: this is what `uniffi` will annotate in M6 without a rewrite.
//! One opaque object behind an `Arc`, no generics, no borrowed references
//! crossing over, errors as a closed enum, events through a `Send + Sync`
//! trait, and value types built from `u64`/`String`/`bool`/`Vec` and enums —
//! never a `seele-proto` newtype.
//!
//! # Threading
//!
//! [`Connection::connect`] **blocks**. It opens a QUIC connection and completes a
//! handshake, and a shell must call it off whatever thread draws. Everything
//! afterwards returns immediately: commands are queued to the driver thread,
//! and [`Connection::snapshot`] reads a copy.
//!
//! Events arrive on the driver thread, not the shell's. A listener that touches
//! a UI must marshal — `specs/06-clientes-gui.md`: "a casca marshala para sua
//! thread de UI."

// A test that has to handle the impossible case stops reading as a statement
// about the code. The same allowance the other crates take.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

/// O foco de voz: no máximo uma conexão desta máquina tem o microfone (E3 da
/// 0.17, multi-sessão).
pub mod cargos;
pub mod foco;
pub mod mods;
pub mod perfis;
pub mod programa;
/// Uma conexão que não existe, com o `fold` e o `snapshot` de uma que existe:
/// para os testes daqui e para a conformidade dos dois produtores de retrato.
#[cfg(any(test, feature = "conformidade"))]
pub mod sem_rede;
pub mod types;
mod volta;

/// Quantas mensagens cada página da busca do buraco pede, na volta de uma queda.
///
/// Pública só para a conformidade a prender ao `MAX_PAGE` do servidor
/// (`queda_recuperavel::a_pagina_da_busca_e_a_pagina_do_servidor`): a busca
/// mede cada página por este número, e um servidor que mandasse menos a faria
/// parar cedo. O cliente não pode ver o servidor (ADR 0002); a conformidade vê
/// os dois.
pub use volta::PAGINA_DO_BURACO;

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seele_core::enlace::Enlace;
use seele_core::retrato::{aviso_de, foto_da_sala, minha_tela_de, transmissoes_de};
use seele_core::{
    identity, ChannelId, ClientMessageId, FilePinStore, MediaChannel, MessageId, PersonId, Room,
    Signal, SignalBand, Ssrc, SyncInputs, Voice, VoiceRoomId,
};

pub use types::{
    AberturaDoAudio, ArtefatoOferecidoFfi, Attachment, AttachmentRefusal, CaminhoDeTunel,
    CaptureDevice, Carimbo, Channel, ChannelWeight, ComandoOferecidoFfi, ComandosDoSistemaFfi,
    ConnectionError, ControlesDaVoz, EndReason, EstadoDoAparelho, EstadoDoEnvio,
    EstadoDoTesteDeMicrofone, Event, ExclusaoDoSom, FalhaAoLocalizar, FonteDeTela,
    HashDeArtefatoFfi, LimitesDeTela, LimitesDosCampos, LinkState, LinkTrust, MembroForaDoAr,
    Message, MotivoDaRecusa, NotSavedReason, Notice, NoticeReason, OfertaDeMaquinaFfi,
    PermissaoDeMicrofone, PermissaoDeTela, Person, PlaybackDevice, PortaDeTunel, Preview,
    PreviewRefusal, PreviewRules, QuedaDeTunel, RelatorioDeTunel, SaveDestination, SaveRefused,
    ScreenKind, Severity, SignalBand as Band, Snapshot, SomDaTela, TelaEmCurso, Telemetry,
    TemaDaSessao, TermoOferecidoFfi, Transfer, TransmissaoNaSala, Trust, TunelOferecidoFfi,
    VoiceMode, VoiceRoom, VoiceRoomSync, VolumeRefusal,
};

/// Diz que este processo tem tela para pedidos de máquina de jogo: toda
/// conexão aberta depois disto já nasce aceitando-os. Ver
/// [`seele_core::enlace::este_processo_tem_tela`] — é o que fecha a corrida com
/// os pedidos que o servidor manda na chegada. Só o `main` do app chama.
pub use seele_core::enlace::este_processo_tem_tela;
pub use seele_core::PreviaDeTela;

/// Quantas fontes ganham prévia numa abertura da caixa de compartilhar.
pub const MAX_PREVIAS_DE_TELA: usize = 24;

/// Quantas capturas de prévia ficam abertas ao mesmo tempo. Abertas juntas,
/// uma fonte que não entrega quadro não segura as outras; em lotes, a caixa
/// não abre vinte e quatro capturas de uma vez.
const LOTE_DE_PREVIAS: usize = 8;

/// **A regra da menção**, pura: se `corpo` chama a pessoa de apelido `base` e
/// marca `tag`. É a função que a sessão usa para marcar
/// [`Message::mentions_me`]; a casca a usa no autocompletar e no realce, para
/// as duas concordarem. A regra inteira está no doc de
/// [`seele_core::mencao`].
pub use seele_core::mencao::menciona;
/// Um banimento que vale agora, como [`Connection::banidos`] o entrega.
pub use seele_core::retrato::Banido;

/// O que a casca gráfica precisa do core além de um [`Connection`] vivo.
///
/// ADR 0002 deixa `seele-app` ver `seele-ffi` e mais nada, e as telas do app
/// precisam dos mesmos módulos que o `connection` usa direto: a lista de servidores
/// visitados, a busca no histórico, a leitura de um `seele://` e os ajustes
/// que ficam nesta máquina. Nenhum deles é lógica de casca — se fossem escritos
/// aqui, seriam escritos de novo no terminal e mais uma vez no cliente móvel.
/// `preferences` é o caso mais claro: um microfone escolhido no app e ignorado
/// pelo terminal é um microfone que parece ter sido esquecido.
///
/// Re-exportados um a um, e não com um `pub use seele_core::*`, pela mesma
/// doutrina que `seele-core` já aplica sobre `seele-proto`: acrescentar a esta
/// lista é a hora de perguntar se a casca precisa do valor ou da decisão por
/// trás dele.
///
/// `preview` entrou por essa pergunta, e a resposta é **a decisão**: a casca
/// desenha imagem vinda de link de conversa, e a única forma de fazê-lo sem
/// abrir a CSP para `https:` é buscar pelo Rust e conferir os bytes. A regra
/// de «o tipo alegado tem de bater com o que os bytes dizem» é a mesma do
/// anexo, e uma segunda cópia dela seria duas regras para discordarem no dia
/// em que uma mudasse.
pub use seele_core::{
    catalogo, conhecidos, escolhidos, midia_de_mod, preferences, preview, reducao, search, som, uri,
};

/// O aparelho de mentira do áudio, só com o recurso (build de teste do CUA).
#[cfg(feature = "aparelho-de-mentira")]
pub use seele_core::aparelho_de_mentira;

/// A agenda, a leitura de um texto colado e o código de uma pessoa, para a sala
/// pessoal (a F7).
///
/// A agenda é a lista de conhecidos dos códigos, e vale o mesmo raciocínio da
/// lista de servidores: o app precisa do valor, e a regra de onde cada linha
/// mora e de como se lê é do núcleo. `entrada` é a «função só, no núcleo» da
/// spec de 23/09: a casca manda todo texto colado a ela.
///
/// O [`Codigo`] atravessa como o [`uri::Bilhete`] atravessa: tipado, e lido uma
/// vez só, na borda. Um código em `String` seria lido de novo em cada lugar que
/// o usa, e cada leitura é um lugar para errar o verificador.
pub use seele_core::{agenda, arquivo_local, entrada, Codigo};

/// O número do fio, a versão de servidor e a linha de cada número, para quem
/// confere o manifesto do launcher contra esta build. Pelo `seele-core`, e não
/// direto do `seele-proto`: o ADR 0002 deixa este crate ver só o núcleo.
pub use seele_core::{linha_do_protocolo, PROTOCOL_VERSION, VERSAO_DO_SERVIDOR};

/// A origem do web app (a D-web-2), para a casca montar o endereço de uma
/// página publicada nela — o teste de rede que a tela de «não conectou»
/// oferece (`abrir_o_teste_de_rede`, no app). Pelo `seele-core`, pela mesma
/// razão das de cima, e daqui e não copiada: o nome mora num lugar só
/// (`a_origem_do_web_app_e_escrita_num_lugar_so`).
pub use seele_core::uri::ORIGEM_DO_NAVEGADOR;

/// Every microphone this machine is offering, right now.
///
/// A free function and not a method on [`Connection`]: picking a microphone is a thing
/// a person does *before* connecting at least as often as during, and hanging
/// the list off a live session would put the control behind the door it exists
/// to open. It is also what makes the list answerable from the entry screen.
///
/// An empty list means the machine would not enumerate — **not** that there is
/// no microphone. The default device still opens when enumeration fails, which
/// is why [`ConnectConfig::capture_device`] of `None` never consults this. A
/// shell that draws "no audio" off an empty list is drawing the wrong sentence;
/// [`Snapshot::audio_available`] is the one that means it.
#[must_use]
pub fn capture_devices() -> Vec<CaptureDevice> {
    seele_core::capture_devices()
        .into_iter()
        .map(|found| CaptureDevice {
            id: found.id,
            name: found.name,
            default: found.default,
        })
        .collect()
}

/// A sensibilidade padrão da ativação por voz **com** redução de ruído, em dBFS.
///
/// F01, e o número vem do núcleo: uma cópia escrita aqui seria uma segunda
/// decisão sobre a mesma régua, esperando para discordar.
pub const ABERTURA_COM_SUPRESSAO_DBFS: f32 = seele_core::ABERTURA_COM_SUPRESSAO_DBFS;

/// A sensibilidade padrão da ativação por voz **sem** redução de ruído, em dBFS.
///
/// Ver [`ABERTURA_COM_SUPRESSAO_DBFS`]: os dois padrões existem porque o alvo de
/// −60 dBFS de F01 é do caminho com filtro.
pub const ABERTURA_SEM_SUPRESSAO_DBFS: f32 = seele_core::ABERTURA_SEM_SUPRESSAO_DBFS;

/// O extremo sensível da faixa de sensibilidade, em dBFS.
pub const ABERTURA_MINIMA_DBFS: f32 = *seele_core::FAIXA_DE_ABERTURA_DBFS.start();

/// O extremo surdo da faixa de sensibilidade, em dBFS.
pub const ABERTURA_MAXIMA_DBFS: f32 = *seele_core::FAIXA_DE_ABERTURA_DBFS.end();

/// Um teste de microfone em curso, sem sessão nenhuma. F01.
///
/// # Por que ele é livre, como a lista de aparelhos
///
/// Pela mesma razão que [`capture_devices`]: testar o microfone é algo que se faz
/// **antes** de conectar pelo menos tanto quanto durante. Pendurá-lo numa sessão
/// viva poria o controle atrás da porta que ele existe para abrir — a pessoa teria
/// de entrar em algum lugar para descobrir que o microfone estava mudo.
///
/// Largar esta alça encerra o teste.
#[derive(Debug)]
pub struct TesteDeMicrofone {
    dentro: seele_core::teste_de_microfone::TesteDeMicrofone,
}

impl TesteDeMicrofone {
    /// Abre o microfone e começa a medir.
    ///
    /// `captura` e `saida` são ids de [`capture_devices`] e [`playback_devices`];
    /// `None` é o aparelho padrão da máquina. `supressao` e `abertura_dbfs` são os
    /// mesmos controles da conversa — o teste tem de medir a régua que vale lá.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NoAudioDevice`] quando não há microfone ou o aparelho
    /// escolhido sumiu.
    pub fn abrir(
        captura: Option<String>,
        saida: Option<String>,
        supressao: f32,
        abertura_dbfs: Option<f32>,
    ) -> Result<Self, ConnectionError> {
        let escolha = seele_core::DeviceChoice {
            capture: captura,
            playback: saida,
        };
        seele_core::teste_de_microfone::TesteDeMicrofone::abrir(&escolha, supressao, abertura_dbfs)
            .map(|dentro| Self { dentro })
            .map_err(|erro| {
                tracing::warn!(%erro, "o teste de microfone não abriu");
                ConnectionError::NoAudioDevice
            })
    }

    /// O que o teste tem a contar agora.
    #[must_use]
    pub fn estado(&self) -> EstadoDoTesteDeMicrofone {
        let dentro = self.dentro.estado();
        EstadoDoTesteDeMicrofone {
            nivel_dbfs: dentro.nivel_dbfs,
            aberto: dentro.aberto,
            aberturas: dentro.aberturas,
            quadros_retidos: dentro.quadros_retidos,
            corte_dbfs: dentro.corte_dbfs,
            monitorando: self.dentro.monitorando(),
            falha: self.dentro.falha(),
        }
    }

    /// Liga ou desliga ouvir o próprio microfone. Começa desligado.
    pub fn set_monitorar(&self, ligado: bool) {
        self.dentro.set_monitorar(ligado);
    }

    /// Troca a força da supressão sem reabrir o aparelho. F02.
    pub fn set_supressao(&self, forca: f32) {
        self.dentro.set_supressao(forca);
    }

    /// Troca a sensibilidade sem reabrir o aparelho. F01.
    pub fn set_abertura_dbfs(&self, dbfs: Option<f32>) {
        self.dentro.set_abertura_dbfs(dbfs);
    }
}

/// O volume padrão do toque, de 0 a 1: audível, e longe do teto da escala.
pub const VOLUME_DO_TOQUE: f32 = seele_core::VOLUME_DO_TOQUE;

/// O toque da sala pessoal tocando (tarefas 24 e 25 da F7). Largar a alça
/// para o som.
///
/// Livre, como o [`TesteDeMicrofone`]: toca sem sessão nenhuma, porque quem
/// recebe uma ligação pode não estar em servidor algum. **Abre só a saída**, e
/// o microfone nunca é tocado (`seele_audio::device::Tocador`).
#[derive(Debug)]
pub struct Toque {
    tocador: seele_core::Tocador,
}

impl Toque {
    /// Começa a tocar em laço, na saída de id `saida` ([`playback_devices`]),
    /// ou na padrão.
    ///
    /// **Uma saída escolhida que sumiu toca na padrão**, como a voz faz
    /// (`abrir_a_voz`): um fone desligado não pode custar uma ligação. A troca
    /// vai ao `seele.log`, e [`Toque::saida`] diz onde de fato tocou.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NoAudioDevice`] quando nem a padrão abre; o motivo vai
    /// ao `seele.log`.
    pub fn tocar(saida: Option<String>, volume: f32) -> Result<Self, ConnectionError> {
        tocar_com(saida, |saida| seele_core::Tocador::tocar(saida, volume))
            .map(|tocador| Self { tocador })
    }

    /// A saída que de fato abriu, quando ela se descreve.
    #[must_use]
    pub fn saida(&self) -> Option<PlaybackDevice> {
        self.tocador.saida().map(|aparelho| PlaybackDevice {
            id: aparelho.id.clone(),
            name: aparelho.name.clone(),
            default: aparelho.default,
        })
    }
}

/// A decisão de [`Toque::tocar`], com quem abre a saída dito por quem chama:
/// a escolhida, e a padrão se a escolhida não abrir.
fn tocar_com<T>(
    saida: Option<String>,
    mut abrir: impl FnMut(Option<String>) -> Result<T, seele_core::DeviceError>,
) -> Result<T, ConnectionError> {
    let escolhida = saida.is_some();
    match abrir(saida) {
        Ok(tocando) => return Ok(tocando),
        Err(erro) if escolhida => {
            tracing::warn!(%erro, "o toque não abriu na saída escolhida; toca na padrão");
        }
        Err(erro) => {
            tracing::warn!(%erro, "o toque não abriu: a ligação toca sem som");
            return Err(ConnectionError::NoAudioDevice);
        }
    }
    abrir(None).map_err(|erro| {
        tracing::warn!(%erro, "o toque não abriu nem na saída padrão: a ligação toca sem som");
        ConnectionError::NoAudioDevice
    })
}

/// Every place this machine will play sound, right now.
///
/// The twin of [`capture_devices`], a free function for the same reason, and
/// with the same warning about an empty list: the machine would not enumerate,
/// **not** that there is nowhere to play. [`Snapshot::audio_available`] is still
/// the one that means "no audio".
#[must_use]
pub fn playback_devices() -> Vec<PlaybackDevice> {
    seele_core::playback_devices()
        .into_iter()
        .map(|found| PlaybackDevice {
            id: found.id,
            name: found.name,
            default: found.default,
        })
        .collect()
}

/// O que o sistema deixa este processo fazer com o microfone.
///
/// **Livre e não método**, ao contrário da permissão de tela: não precisa de
/// sessão nenhuma. Quem descobre que está mudo quer a resposta antes de
/// entrar, e a tela de entrada não tem `Connection` para perguntar.
///
/// Só olha, e não pede nada — no Windows não há o que pedir para um app de
/// área de trabalho. Ver `seele_audio::device::consentimento_do_microfone`.
#[must_use]
pub fn permissao_de_microfone() -> PermissaoDeMicrofone {
    seele_core::consentimento_do_microfone().into()
}

/// A impressão digital da identidade desta máquina.
///
/// A mesma chave com que este computador entra em qualquer servidor, e a mesma
/// impressão que o outro lado vê — `identity.key` na pasta da identidade
/// ([`pasta_da_identidade`], ADR 0061), criada na primeira vez se ainda não
/// existir.
///
/// Existe porque quem hospeda precisa se reconhecer no próprio servidor **antes**
/// de bater na própria porta, e o app não pode calcular isso sozinho: a regra de
/// dependência do ADR 0002 o deixa ver `seele-ffi` e `seele-server`, e nunca
/// `seele-core` nem `seele-proto`. A ponte é aqui.
///
/// # Errors
///
/// Falha se a identidade não puder ser lida nem criada.
pub fn impressao_desta_maquina(home: &str) -> Result<String, ConnectionError> {
    impressao_com(home, std::env::var(VARIAVEL_DA_IDENTIDADE).ok().as_deref())
}

/// O mesmo, com o valor de `SEELE_IDENTIDADE` dito por quem chama. Separado
/// para o teste não mexer no ambiente do processo, que os outros testes leem.
fn impressao_com(home: &str, variavel: Option<&str>) -> Result<String, ConnectionError> {
    let chave = chave_com(home, variavel)?;
    Ok(seele_core::key_fingerprint(
        chave.verifying_key().as_bytes(),
    ))
}

/// A chave desta máquina, da pasta da identidade, criada se ainda não existe.
/// A impressão, o código e a semente saem todos daqui: três leituras de uma
/// chave só, e nenhuma delas pode achar outra.
fn chave_com(
    home: &str,
    variavel: Option<&str>,
) -> Result<seele_core::SigningKey, ConnectionError> {
    let arquivo = arquivo_da_identidade_com(Path::new(home), variavel);
    identity::load_or_create(&arquivo).map_err(|error| {
        tracing::warn!(%error, "identity unavailable");
        ConnectionError::IdentityUnavailable
    })
}

/// O código desta máquina: o «número de telefone» da sala pessoal (ADR 0060),
/// o que SEU CÓDIGO mostra e o que o ATENDER confere
/// ([`ConnectConfig::codigo_esperado`]).
///
/// Da mesma chave que [`impressao_desta_maquina`], pela mesma pasta da
/// identidade (ADR 0061). A sala sobe com essa chave ([`semente_da_sala`]), e
/// um código lido de outra seria um número que não toca aqui.
///
/// # Errors
///
/// Falha se a identidade não puder ser lida nem criada.
pub fn codigo_desta_maquina(home: &str) -> Result<Codigo, ConnectionError> {
    codigo_com(home, std::env::var(VARIAVEL_DA_IDENTIDADE).ok().as_deref())
}

/// **O alvo canônico de quem liga para um código**: `@CÓDIGO` (tarefa 30 da F7,
/// a 6g; spec de 23/09, «Do lado de quem liga», 2).
///
/// Ele se basta sozinho: é o que a casca põe no `server` do [`ConnectConfig`],
/// e a ponte pergunta ao ponto de encontro onde a sala mora. Sobrevive ao
/// `ejetar`, que zera o convite da sessão, porque não depende dele. É o mesmo
/// texto da chave do pino do código (`seele_core::confere_codigo::chave_do_pino`).
#[must_use]
pub fn alvo_do_codigo(codigo: &Codigo) -> String {
    seele_core::confere_codigo::chave_do_pino(codigo)
}

/// O código de um alvo `@CÓDIGO`, ou nada quando o alvo é um endereço. A volta
/// de [`alvo_do_codigo`], e a única leitura dela.
fn codigo_do_alvo(alvo: &str) -> Option<Codigo> {
    Codigo::ler(alvo.trim().strip_prefix('@')?).ok()
}

/// O mesmo, com o valor de `SEELE_IDENTIDADE` dito por quem chama.
fn codigo_com(home: &str, variavel: Option<&str>) -> Result<Codigo, ConnectionError> {
    let chave = chave_com(home, variavel)?;
    Ok(Codigo::da_chave(chave.verifying_key().as_bytes()))
}

/// O ponto de encontro desta máquina: o que `$SEELE_ENCONTRO` diz, e o do
/// projeto sem ela; `None` quando a variável o desliga.
///
/// **O mesmo dos dois lados** da sala pessoal: a sala se registra nele (tarefa
/// 23 da F7), e quem liga pelo código pergunta nele. É a leitura única do
/// ambiente, a de `seele_core::encontro::ponto_de_quem_liga`: com
/// `$SEELE_ENCONTRO`, uma leitura diferente aqui deixaria a sala registrada
/// num ponto onde ninguém pergunta.
///
/// **Só a produção chama isto**, como o `iniciar_com_o_ponto_padrao` de quem
/// hospeda: os testes sobem a sala com o ponto deles, ou sem ponto nenhum.
#[must_use]
pub fn ponto_de_encontro() -> Option<String> {
    seele_core::encontro::ponto_de_quem_liga()
}

/// O ponto em que uma ligação desta casa pergunta pelo código.
///
/// No produto é sempre [`ponto_de_encontro`]. Com a feature `costura-de-teste`,
/// um ponto dito para **esta casa** ([`costura_de_teste::usar_o_ponto_nesta_casa`])
/// vem antes: é por onde um teste do app liga pelo código, como o produto, sem
/// pôr `$SEELE_ENCONTRO` no processo inteiro.
fn ponto_de_quem_liga_desta_casa(home: &str) -> Option<String> {
    #[cfg(feature = "costura-de-teste")]
    if let Some(ponto) = costura_de_teste::ponto_da_casa(home) {
        return Some(ponto);
    }
    let _ = home;
    ponto_de_encontro()
}

/// O ponto de encontro de quem liga, por casa, só nos testes do app.
#[cfg(feature = "costura-de-teste")]
pub mod costura_de_teste;

/// A semente da sala pessoal: a chave desta máquina, de onde saem o
/// certificado da sala e a assinatura do registro dela no ponto de encontro
/// (`seele_server::hospedagem::Hospedagem::iniciar_sala`, tarefa 23 da F7).
///
/// **Nunca atravessa a webview.** É a chave privada da pessoa: quem a tem
/// entra em qualquer servidor como ela e atende a sala dela. Por isso este tipo
/// não é `Serialize` nem `Clone`, o `Debug` não a imprime, e os bytes só saem
/// emprestados, por [`SementeDaSala::bytes`], para quem sobe a sala.
pub struct SementeDaSala(seele_core::SigningKey);

impl SementeDaSala {
    /// Os 32 bytes da semente, emprestados para `iniciar_sala`.
    #[must_use]
    pub fn bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl std::fmt::Debug for SementeDaSala {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SementeDaSala(<oculta>)")
    }
}

/// A semente da sala pessoal desta máquina, da pasta da identidade.
///
/// # Errors
///
/// Falha se a identidade não puder ser lida nem criada.
pub fn semente_da_sala(home: &str) -> Result<SementeDaSala, ConnectionError> {
    semente_com(home, std::env::var(VARIAVEL_DA_IDENTIDADE).ok().as_deref())
}

/// O mesmo, com o valor de `SEELE_IDENTIDADE` dito por quem chama.
fn semente_com(home: &str, variavel: Option<&str>) -> Result<SementeDaSala, ConnectionError> {
    chave_com(home, variavel).map(SementeDaSala)
}

/// A variável que diz onde mora a identidade desta máquina. ADR 0061.
///
/// O valor é o caminho absoluto de uma **pasta**; a chave é
/// `<pasta>/identity.key`, no formato do ADR 0017. Quem a põe é o launcher,
/// apontando a pasta da identidade do próprio processo: assim uma versão aberta
/// por outra é a mesma pessoa, e não uma chave nova na pasta de dados da linha
/// (`dados/servidor-<linha>`). O mesmo nome, com o mesmo contrato, na
/// `linha/0.15` e em `seele_lancador::inicializacao`.
pub const VARIAVEL_DA_IDENTIDADE: &str = "SEELE_IDENTIDADE";

/// A pasta da identidade: a de `SEELE_IDENTIDADE`, quando ela não está vazia;
/// senão, `home`, que é a de sempre.
///
/// **Só a chave sai daí.** O banco, os pins, os conhecidos e as preferências
/// continuam em `home` (o `SEELE_HOME`), que é o que separa as pastas de dados
/// de cada linha. ADR 0061, «O contrato da variável».
#[must_use]
pub fn pasta_da_identidade(home: &Path, variavel: Option<&str>) -> PathBuf {
    match variavel {
        Some(pasta) if !pasta.is_empty() => PathBuf::from(pasta),
        _ => home.to_path_buf(),
    }
}

/// O arquivo da chave, `<pasta da identidade>/identity.key` (ADR 0017). **É o
/// único lugar que monta o caminho**, e os dois leitores passam por ele; o
/// guarda `so_um_lugar_monta_o_caminho_da_chave` prende isso.
fn arquivo_da_identidade_com(home: &Path, variavel: Option<&str>) -> PathBuf {
    pasta_da_identidade(home, variavel).join("identity.key")
}

/// O arquivo da chave desta máquina, com a variável do processo.
fn arquivo_da_identidade(home: &Path) -> PathBuf {
    arquivo_da_identidade_com(home, std::env::var(VARIAVEL_DA_IDENTIDADE).ok().as_deref())
}

/// Assenta o padrão do microfone desta instalação, e devolve o modo que ficou
/// valendo para quem não escolheu. `None` se o disco recusou a escrita.
///
/// # Quem é instalação nova
///
/// O padrão de quem chega passou a ser a voz (pedido do dono, 13/11/2026), e
/// quem já usava e nunca escolheu fica na tecla — ver
/// `seele_core::preferences::Preferences::assentar_o_padrao_do_microfone`, que
/// é quem grava. Aqui é decidido só o «já usava», pelas duas coisas que uma
/// instalação usada deixa e uma nova não tem:
///
/// - **a chave da identidade**, na pasta da identidade (ADR 0061). Ela nasce
///   na primeira vez que o app mostra o código da pessoa ou conecta, e é uma
///   por máquina: uma linha aberta pelo launcher, com a pasta de dados dela, a
///   acha no mesmo lugar;
/// - **o arquivo de preferências**, que só existe depois de a pessoa (ou o
///   app) gravar alguma coisa nele.
///
/// # Quando chamar
///
/// **No arranque, antes de existir janela.** É a janela que pede a impressão e
/// o `connect`, e os dois criam a chave: chamada depois, toda instalação nova
/// pareceria usada. A casca chama isto no `setup`.
pub fn assentar_o_padrao_do_microfone(
    home: &str,
    ajustes: &mut preferences::Preferences,
) -> Option<VoiceMode> {
    assentar_o_padrao_do_microfone_com(
        home,
        std::env::var(VARIAVEL_DA_IDENTIDADE).ok().as_deref(),
        ajustes,
    )
}

/// O mesmo, com o valor de `SEELE_IDENTIDADE` dito por quem chama, pela razão
/// de [`impressao_com`].
fn assentar_o_padrao_do_microfone_com(
    home: &str,
    variavel: Option<&str>,
    ajustes: &mut preferences::Preferences,
) -> Option<VoiceMode> {
    let ja_usava =
        ajustes.havia_arquivo() || arquivo_da_identidade_com(Path::new(home), variavel).exists();
    match ajustes.assentar_o_padrao_do_microfone(ja_usava) {
        Ok(padrao) => Some(padrao.into()),
        Err(erro) => {
            tracing::warn!(%erro, "não consegui gravar o padrão do microfone desta instalação");
            None
        }
    }
}

/// How often measurements are refreshed.
const TICK: Duration = Duration::from_millis(250);

/// How many messages of history to ask for when a channel opens.
const HISTORY_PAGE: u16 = 50;

/// What a shell needs to connect.
///
/// O `Debug` é escrito à mão, logo abaixo, para não imprimir o `join_secret`.
#[derive(Clone)]
pub struct ConnectConfig {
    /// `host` or `host:port`.
    pub server: String,
    /// The invite's other addresses for the same server, in try order.
    ///
    /// Empty for an address typed by hand, and for every invite written before
    /// ADR 0006 grew the field. One address per entry, same syntax as
    /// [`ConnectConfig::server`]; one that does not resolve is dropped rather
    /// than refused, because losing a path is not a reason to lose the link.
    pub alternate_servers: Vec<String>,
    /// How to appear in the roster.
    pub nickname: String,
    /// Where the identity and the pins live. ADR 0017.
    ///
    /// A path rather than a key, because the shell knows where its platform
    /// keeps configuration and the core knows how to persist an identity.
    pub home: String,
    /// Convite de uso único ou senha, quando o servidor exige um.
    ///
    /// `None` num servidor aberto, que é o padrão.
    pub join_secret: Option<String>,
    /// A impressão digital que esta conexão confere, dentro do TLS e antes do
    /// `Hello`.
    ///
    /// É a do link, quando a pessoa colou um nesta sessão. Na volta pela lista
    /// de servidores, sem link, é a que a lista guardou: a impressão que uma
    /// conexão anterior **aceitou** ([`impressao_a_guardar`]), que quase
    /// sempre é a do link daquela vez, mas não é a do link quando ele
    /// discordava do pin. Quem decide qual das duas vale é
    /// [`impressao_a_conferir`], e o `connect` do app a usa.
    pub expected_fingerprint: Option<String>,
    /// O código da sala pessoal que esta conexão confere (ADR 0060), no lugar
    /// da impressão: quem atende tem de ser a sala dessa pessoa, e quem confere
    /// é o `ConfereCodigo` do núcleo, dentro do TLS.
    ///
    /// É o ATENDER (tarefa 27 da F7): a casca entra na própria sala conferindo
    /// o próprio código ([`codigo_desta_maquina`]), e não um certificado que
    /// lista nenhuma guardou. `None` em toda outra conexão. Com os dois, vale o
    /// código, e o `seele.log` diz que a impressão ficou de fora
    /// ([`esperada_de`]).
    pub codigo_esperado: Option<Codigo>,
    /// O bilhete de encontro do link, quando ele trouxe um.
    ///
    /// Degrau 4 do ADR 0022: com ele, quem entra bate no ponto de encontro antes
    /// de tentar os endereços, e o anfitrião fura o NAT para cá. `None` é o
    /// caminho de sempre — e é o que um link sem `enc` produz.
    pub bilhete: Option<seele_core::uri::Bilhete>,
    /// Whether to open the microphone and speakers at all.
    ///
    /// False on a machine with no sound card, which is most servers and every
    /// CI box. The text half of the product needs none.
    ///
    /// Verdadeiro não abre nada ao conectar: pela pendência 52 o áudio abre
    /// quando a pessoa entra numa sala de voz, e fecha quando ela sai.
    pub audio: bool,
    /// Which microphone to open, as a [`CaptureDevice::id`].
    ///
    /// `None` is the machine's default, and is what every session took before
    /// there was a screen to choose on. A device that is gone by the time the
    /// voice opens does **not** refuse the room — see `abrir_a_voz`, which
    /// falls back to the default rather than turning a stale preference into a
    /// room nobody can speak in.
    pub capture_device: Option<String>,
    /// Where the sound comes out, as a [`PlaybackDevice::id`].
    ///
    /// `None` is the machine's default. Falls back the same way its twin does,
    /// and independently of it: a headset left in another room must not cost
    /// somebody the microphone they chose.
    pub playback_device: Option<String>,
}

/// **O `join_secret` não sai pelo `Debug`** (lote R da F5, rodada 1).
///
/// Ele é o convite de uso único ou a senha do servidor, e um
/// `tracing::info!(?config)` no `connect` do app o levaria ao `seele.log`. Aqui
/// ele sai como `Some(<oculto>)` ou `None`: quem investiga uma recusa sabe se a
/// entrada levou um, e não qual. O resto sai como o `derive` o escrevia.
impl std::fmt::Debug for ConnectConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// O lugar do segredo, sem ele.
        struct Oculto;
        impl std::fmt::Debug for Oculto {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("<oculto>")
            }
        }
        f.debug_struct("ConnectConfig")
            .field("server", &self.server)
            .field("alternate_servers", &self.alternate_servers)
            .field("nickname", &self.nickname)
            .field("home", &self.home)
            .field("join_secret", &self.join_secret.as_ref().map(|_| Oculto))
            .field("expected_fingerprint", &self.expected_fingerprint)
            .field("codigo_esperado", &self.codigo_esperado)
            .field("bilhete", &self.bilhete)
            .field("audio", &self.audio)
            .field("capture_device", &self.capture_device)
            .field("playback_device", &self.playback_device)
            .finish()
    }
}

/// Uma etapa de uma chegada, do jeito que uma casca a recebe.
///
/// Espelho de `seele_core::chegada::Etapa`, com o **mesmo nome** em cada
/// variante e em cada campo. O que esta cópia acrescenta é `Serialize` e o
/// [`ConnectionError`] no lugar do erro do núcleo; nada mais. A regra que ela segue é
/// a das outras travessias deste arquivo: o nome do tipo é do crate e nunca
/// atravessa, e o nome que atravessa — variante e campo — é o que o núcleo
/// escolheu, porque é ele que a casca usa como chave de frase. Traduzir aqui
/// daria dois vocabulários para a mesma coisa, e a casca teria de aprender os
/// dois.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ConnectStage {
    /// Parada, com o convite lido e nada tentado.
    Parada {
        /// Quantos endereços o convite trouxe.
        candidatos: u8,
        /// O link trouxe `enc=` **e** o primeiro candidato tem impressão
        /// digital.
        ///
        /// Não é «o degrau 4 do ADR 0022 vai ser tentado»: ver a variante do
        /// núcleo, que escreve o que a bandeira sabe e o que ela não sabe.
        com_bilhete_e_impressao: bool,
    },
    /// Perguntando ao ponto de encontro onde a sala de um código mora.
    ///
    /// Só numa ligação. Ver a variante do núcleo: ao contrário do aviso, esta
    /// etapa pode reprovar a chegada, com [`ConnectionError::NaoLocalizou`].
    Localizando {
        /// O ponto de encontro a quem se pergunta.
        ponto: String,
    },
    /// Avisando o ponto de encontro de que estamos chegando.
    Avisando {
        /// O ponto de encontro, como o convite o escreveu.
        ponto: String,
    },
    /// Um aperto de mão correndo contra um endereço do convite.
    Tentando {
        /// Qual da lista, contando do zero.
        candidato: u8,
        /// De quantos.
        de: u8,
        /// O endereço, como texto: um `SocketAddr` não atravessa (ADR 0018).
        onde: String,
        /// Um `LEVE` saiu pelo ponto de encontro por causa deste candidato.
        ///
        /// Ver a variante do núcleo. É a metade da informação de que
        /// [`Snapshot::caminho`] é feito, e a única que não se lê do endereço.
        avisou: bool,
    },
    /// Um furo com a marca certa chegou, e o caminho até aqui abriu.
    ///
    /// **Marca não é autenticação.** Ver a variante do núcleo: esta etapa não
    /// decide para onde conectar nem dispensa conferência nenhuma — ela
    /// antecipa o instante da tentativa, e mais nada.
    CaminhoAberto {
        /// De onde o furo veio.
        onde: String,
    },
    /// Quem recebe está sendo chamado: uma sala pessoal mandou `Tocando`.
    ///
    /// Ver a variante do núcleo: quem tocou venceu a corrida, e daqui só se sai
    /// para `Dentro` ou `Desistiu`.
    Tocando {
        /// Qual da lista tocou, contando do zero.
        candidato: u8,
        /// O prazo do toque, em segundos.
        prazo_s: u16,
    },
    /// Dentro: o aperto de mão terminou e há sessão.
    Dentro,
    /// Nenhum candidato entrou, e este é o motivo.
    Desistiu {
        /// O mesmo erro que a casca já sabe escrever.
        motivo: ConnectionError,
    },
    /// Quem liga desistiu antes de entrar. Ver a variante do núcleo: não é
    /// `Desistiu`, e daqui não se sai.
    Cancelada,
}

/// Uma etapa e quando ela aconteceu.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConnectStep {
    /// Onde a chegada estava.
    pub etapa: ConnectStage,
    /// Quantos milissegundos depois do começo da chegada.
    ///
    /// Milissegundos e não uma `Duration`, pela mesma regra do resto desta
    /// fronteira: um número que toda linguagem tem.
    pub em_ms: u64,
}

/// Uma conexão que não aconteceu, e por onde ela passou tentando.
///
/// «Tentei quatro candidatos, o primeiro deu prazo esgotado em 4 s, o quarto
/// recusou» é o dado que faltou quando o teste de campo das duas casas falhou e
/// ninguém soube dizer por quê. Custa zero em privacidade: todo endereço da
/// trilha já estava com quem a lê, no que digitou, no convite, na lista de
/// conhecidos ou na resposta do quarto sobre o mesmo servidor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConnectFailure {
    /// Por que não deu, do jeito de sempre.
    pub error: ConnectionError,
    /// Por onde a chegada passou, em ordem. Vazia quando nada chegou a ser
    /// tentado — uma identidade que não abre, uma thread que não sobe.
    pub trail: Vec<ConnectStep>,
}

impl From<ConnectionError> for ConnectFailure {
    /// Uma falha de antes de haver chegada: o erro sozinho, sem trilha.
    fn from(error: ConnectionError) -> Self {
        Self {
            error,
            trail: Vec::new(),
        }
    }
}

impl std::fmt::Display for ConnectFailure {
    /// Para log e para `Error`, nunca para uma pessoa.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:?} depois de {} passos",
            self.error,
            self.trail.len()
        )
    }
}

impl std::error::Error for ConnectFailure {}

impl ConnectStage {
    /// O nome estável que a casca usa como chave de frase.
    ///
    /// O mesmo de `seele_core::chegada::Etapa::nome`, e é obrigação desta
    /// travessia que continue sendo: a casca escreve uma frase por nome, e um
    /// nome que se perde entre os dois lados é uma tela muda.
    #[must_use]
    pub fn nome(&self) -> &'static str {
        match self {
            Self::Parada { .. } => "Parada",
            Self::Localizando { .. } => "Localizando",
            Self::Avisando { .. } => "Avisando",
            Self::Tentando { .. } => "Tentando",
            Self::CaminhoAberto { .. } => "CaminhoAberto",
            Self::Tocando { .. } => "Tocando",
            Self::Dentro => "Dentro",
            Self::Desistiu { .. } => "Desistiu",
            Self::Cancelada => "Cancelada",
        }
    }

    /// Um exemplar de cada etapa que pode atravessar esta fronteira.
    ///
    /// **Derivada, e não escrita à mão.** Ela é `seele_core::chegada::Etapa::TODAS`
    /// passada por [`From`], que é a única porta por onde uma etapa atravessa —
    /// então uma variante nova do núcleo aparece aqui sozinha, e a casca, que
    /// pelo ADR 0002 não pode ver o núcleo, ganha a lista sem repeti-la.
    ///
    /// É para isso que ela existe: o guarda de cobertura de `frases.js` mora em
    /// `apps/seele-app/tests/frontend.rs` e lia uma terceira cópia da lista,
    /// escrita à mão. Uma etapa nova atravessava e caía no «falha que esta tela
    /// não sabe nomear» no meio de uma conexão que ia bem, sem que teste nenhum
    /// acendesse.
    ///
    /// Os valores são de exemplo e não significam nada — o que se lê deles é a
    /// variante.
    #[must_use]
    pub fn todas() -> Vec<Self> {
        seele_core::chegada::Etapa::TODAS
            .iter()
            .map(Self::from)
            .collect()
    }
}

impl From<&seele_core::chegada::Etapa> for ConnectStage {
    /// Uma etapa do núcleo, do jeito que a casca a recebe.
    fn from(etapa: &seele_core::chegada::Etapa) -> Self {
        use seele_core::chegada::Etapa;
        match etapa {
            Etapa::Parada {
                candidatos,
                com_bilhete_e_impressao,
            } => Self::Parada {
                candidatos: *candidatos,
                com_bilhete_e_impressao: *com_bilhete_e_impressao,
            },
            Etapa::Localizando { ponto } => Self::Localizando {
                ponto: ponto.clone(),
            },
            Etapa::Avisando { ponto } => Self::Avisando {
                ponto: ponto.clone(),
            },
            Etapa::Tentando {
                candidato,
                de,
                onde,
                avisou,
            } => Self::Tentando {
                candidato: *candidato,
                de: *de,
                onde: onde.to_string(),
                avisou: *avisou,
            },
            Etapa::CaminhoAberto { onde } => Self::CaminhoAberto {
                onde: onde.to_string(),
            },
            Etapa::Tocando { candidato, prazo_s } => Self::Tocando {
                candidato: *candidato,
                prazo_s: *prazo_s,
            },
            Etapa::Dentro => Self::Dentro,
            Etapa::Desistiu(erro) => Self::Desistiu {
                motivo: classify_connect_failure(erro),
            },
            Etapa::Cancelada => Self::Cancelada,
        }
    }
}

/// Os três campos que uma etapa pode carregar sobre **onde** ela aconteceu.
///
/// `Option` e não um valor de enfeite: `Dentro` não tem candidato, e escrever
/// `candidato = 0` para ela seria inventar o zero. Para `Avisando` e
/// `Localizando` o «onde» é o ponto de encontro, que é para onde aquele passo
/// mandou alguma coisa; para
/// `Parada` o `de` é quantos endereços o convite trouxe, que é a mesma
/// grandeza que o `de` de uma tentativa — «de quantos».
fn campos_do_passo(etapa: &seele_core::chegada::Etapa) -> (Option<String>, Option<u8>, Option<u8>) {
    use seele_core::chegada::Etapa;
    match etapa {
        Etapa::Localizando { ponto } | Etapa::Avisando { ponto } => {
            (Some(ponto.clone()), None, None)
        }
        Etapa::Tentando {
            candidato,
            de,
            onde,
            ..
        } => (Some(onde.to_string()), Some(*candidato), Some(*de)),
        Etapa::CaminhoAberto { onde } => (Some(onde.to_string()), None, None),
        // O endereço é o da `Tentando` do mesmo candidato, logo acima na trilha.
        Etapa::Tocando { candidato, .. } => (None, Some(*candidato), None),
        Etapa::Parada { candidatos, .. } => (None, None, Some(*candidatos)),
        Etapa::Dentro | Etapa::Desistiu(_) | Etapa::Cancelada => (None, None, None),
    }
}

/// A trilha de uma chegada que falhou, no log do processo.
///
/// **É a superfície inteira que responde a pergunta desta tarefa.** Enquanto
/// `apps/seele-app` entrar por [`Connection::connect`], que joga a trilha fora, este
/// log é o único lugar em que ela aparece — e um log que escrevesse só a etapa
/// e o relógio responderia «Parada, Tentando, Desistiu, aos 8003 ms», que não é
/// a pergunta. A pergunta é **qual dos quatro deu o quê**, e ela precisa do
/// endereço e do índice.
fn registrar_trilha(trilha: &[seele_core::chegada::Passo]) {
    for passo in trilha {
        let (onde, candidato, de) = campos_do_passo(&passo.etapa);
        tracing::info!(
            etapa = passo.etapa.nome(),
            candidato = ?candidato,
            de = ?de,
            onde = ?onde,
            avisou = ?avisou_do_passo(&passo.etapa),
            em_ms = %passo.em.as_millis(),
            "trilha da chegada"
        );
    }
}

/// Se um `LEVE` saiu por este passo, nos passos em que a pergunta faz sentido.
///
/// Vai para o log junto do endereço porque é a outra metade da resposta: uma
/// tentativa que falhou **tendo avisado** e uma que falhou sem aviso nenhum são
/// duas investigações diferentes, e sem esta linha as duas ficam iguais no
/// registro.
fn avisou_do_passo(etapa: &seele_core::chegada::Etapa) -> Option<bool> {
    use seele_core::chegada::Etapa;
    match etapa {
        Etapa::Tentando { avisou, .. } => Some(*avisou),
        Etapa::Parada { .. }
        | Etapa::Localizando { .. }
        | Etapa::Avisando { .. }
        | Etapa::CaminhoAberto { .. }
        | Etapa::Tocando { .. }
        | Etapa::Dentro
        | Etapa::Desistiu(_)
        | Etapa::Cancelada => None,
    }
}

/// Os nomes de caminho que podem chegar a uma casca.
///
/// **Derivada, e não escrita à mão**, pela mesma regra de
/// [`ConnectStage::todas`]: é `seele_core::chegada::Caminho::TODOS` passada pelo
/// `nome()` do núcleo. O guarda de cobertura de `frases.js` lê esta lista, e o
/// ADR 0002 não deixa `apps/seele-app` ver o núcleo — sem esta função a casca
/// teria de repetir os quatro nomes, que é exatamente o defeito que este ciclo
/// já pagou uma vez.
#[must_use]
pub fn caminhos() -> Vec<&'static str> {
    seele_core::chegada::Caminho::TODOS
        .iter()
        .map(seele_core::chegada::Caminho::nome)
        .collect()
}

/// Um passo da trilha, do jeito que a casca o recebe.
fn step_of(passo: &seele_core::chegada::Passo) -> ConnectStep {
    ConnectStep {
        etapa: ConnectStage::from(&passo.etapa),
        // Saturado, e não truncado: uma chegada que levasse mais de 584 milhões
        // de anos merece um número errado no fim da escala, e não um pequeno.
        em_ms: u64::try_from(passo.em.as_millis()).unwrap_or(u64::MAX),
    }
}

/// What a shell implements to be told things changed.
///
/// Called on the driver thread. `specs/06-clientes-gui.md` puts the marshalling
/// on the shell, because only the shell knows what its UI thread is.
pub trait EventListener: Send + Sync {
    /// Something happened.
    fn on_event(&self, event: Event);

    /// O mesmo, dizendo de qual sessão e de qual volta do enlace (E3 da 0.17).
    ///
    /// É por aqui que este crate entrega **todo** evento. O corpo padrão chama
    /// [`Self::on_event`], então quem só implementa aquele continua ouvindo
    /// tudo, sem o carimbo; quem precisa dele (a casca com várias sessões)
    /// implementa este.
    fn on_event_carimbado(&self, carimbo: Carimbo, event: Event) {
        let _ = carimbo;
        self.on_event(event);
    }
}

/// O `SessaoId` (E3 da 0.17): o número que identifica uma sessão cliente no
/// processo, da tentativa de conexão até o fim, através das reconexões.
///
/// **Um contador do processo, e não estado de conexão**, de propósito: o número
/// só serve se duas sessões nunca o dividem, nem uma que já acabou com uma que
/// começa agora. Um contador por conexão daria 1 a todas; um aleatório poderia
/// repetir. Ele não guarda nada de nenhuma sessão: só garante que o próximo
/// número é novo. Por isso pode ser estático sem contrariar o «`Connection` não
/// tem estado global».
pub struct SessaoId;

/// O próximo `SessaoId`. Começa em 1: o 0 nunca sai, e fica livre para quem
/// precisar dizer «nenhuma».
static PROXIMA_SESSAO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl SessaoId {
    /// Um `SessaoId` que nenhuma sessão deste processo usou: maior que todos os
    /// anteriores, e nunca 0.
    #[must_use]
    pub fn nova() -> u64 {
        // `fetch_add` é atômico: duas linhas pedindo juntas recebem números
        // diferentes. O estouro de um u64 não acontece (um por nanossegundo
        // levaria 584 anos), mas, se acontecesse, o 0 seria pulado.
        let numero = PROXIMA_SESSAO.fetch_add(1, Ordering::Relaxed);
        if numero == 0 {
            PROXIMA_SESSAO.fetch_add(1, Ordering::Relaxed)
        } else {
            numero
        }
    }
}

/// O que a casca decidiu sobre uma máquina, já com o motivo do núcleo.
enum VerboDaMaquina {
    Aceitar {
        mod_id: String,
        servidor: String,
        resumo: String,
    },
    Recusar {
        mod_id: String,
        servidor: String,
        motivo: seele_core::maquina::MotivoDaRecusaDeMaquina,
    },
    Parar {
        mod_id: String,
        servidor: String,
        motivo: seele_core::maquina::MotivoDaParadaDeMaquina,
    },
}

/// A command on its way to the driver thread.
/// Onde a resposta de uma pesagem chega: o peso, ou a causa com que o servidor
/// recusou a pergunta (`AlertReason::PedidoRecusado`).
type RespostaDoPeso = tokio::sync::oneshot::Sender<Result<ChannelWeight, types::CausaDaRecusa>>;

enum Command {
    /// Se esta conexão mostra pedidos de máquina (API 8 parte 3).
    AceitarPedidosDeMaquina(bool),
    /// O aceite, a recusa ou a parada de uma máquina.
    VerboDaMaquina(VerboDaMaquina),
    /// Um pedido da máquina à metade de servidor; a espera vai para uma
    /// tarefa, como a do túnel.
    PedidoDaMaquina {
        mod_id: String,
        servidor: String,
        payload: String,
        resposta: tokio::sync::oneshot::Sender<Result<String, &'static str>>,
    },
    /// A máquina passa a servir um túnel.
    ServirTunel {
        mod_id: String,
        nome: String,
        protocolo: seele_core::tunel::ProtocoloDeTunel,
        porta: u16,
        resposta: tokio::sync::oneshot::Sender<Result<(), &'static str>>,
    },
    /// A máquina para de servir um túnel.
    PararTunel {
        mod_id: String,
        nome: String,
    },
    /// As quedas de um túnel que esta máquina serve.
    QuedasDoTunelServido {
        mod_id: String,
        nome: String,
        resposta: tokio::sync::oneshot::Sender<Vec<QuedaDeTunel>>,
    },
    /// Quem joga entra num túnel de jogo. A resposta vai por `resposta`, e a
    /// espera por ela não pode parar este laço (ver `run_command`).
    EntrarNoTunel {
        mod_id: String,
        nome: String,
        resposta: tokio::sync::oneshot::Sender<Result<PortaDeTunel, &'static str>>,
    },
    /// Quem joga deixa um túnel.
    SairDoTunel {
        mod_id: String,
        nome: String,
    },
    /// Quem joga deixa todos os túneis de um MOD (ele foi descarregado).
    SairDosTuneisDoMod {
        mod_id: String,
    },
    ImagemDeMod {
        pedido: seele_core::PedidoDeImagem,
        bytes: Vec<u8>,
        resposta: tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>,
    },

    ModRequest {
        request: u32,
        id: String,
        channel: ChannelId,
        payload: String,
    },
    /// Uma ação declarada de MOD (API 7, `ClientMessage::AcaoDeMod`).
    AcaoDeMod {
        request: u32,
        id: String,
        channel: ChannelId,
        acao: String,
        payload: String,
    },
    /// Devolve à sonda o caminho medido da última visita a este servidor.
    LembrarCaminho(u32),
    EnterVoiceRoom(VoiceRoomId, Option<String>),
    LeaveVoiceRoom,
    OpenChannel(ChannelId),
    Send {
        channel: ChannelId,
        body: String,
        /// A mensagem a que esta responde, se responde a alguma.
        replies_to: Option<MessageId>,
        /// A chave escolhida por quem pediu o envio.
        ///
        /// Escolhida **antes** do comando e carregada nele, e não sorteada no
        /// laço: é ela que liga a pendente desenhada na tela à confirmação que
        /// volta, e um número sorteado aqui dentro seria um número que a casca
        /// não tem como esperar. Ver [`Connection::send_message`].
        id: ClientMessageId,
    },
    SetMuted(bool),
    SetTotalIsolation(bool),
    CreateVoiceRoom {
        name: String,
        limit: u16,
        channel: Option<ChannelId>,
    },
    CreateChannel {
        name: String,
    },
    RenameVoiceRoom {
        voice_room: VoiceRoomId,
        name: String,
    },
    RenameChannel {
        channel: ChannelId,
        name: String,
    },
    RenameServer {
        name: String,
    },
    /// Põe ou tira a imagem de perfil de quem está usando este cliente.
    SetPersonIcon {
        /// A figura, ou `None` para não ter.
        icon: Option<Vec<u8>>,
    },
    /// Troca o apelido de quem está usando este cliente.
    SetNickname {
        /// O nome novo.
        name: String,
    },
    /// O `GravarPerfil` já montado e validado (0.19, protocolo 12).
    GravarPerfil(seele_core::ClientMessage),
    /// Tira a foto ou a capa do perfil (0.19, protocolo 12).
    TirarImagemDoPerfil {
        /// Qual das duas.
        lado: perfis::LadoDoPerfil,
    },
    SetServerIcon {
        icon: Option<Vec<u8>>,
    },
    KickPerson {
        person: PersonId,
    },
    /// Um verbo de cargo com a revisão vista, ou `None` para pedir o quadro
    /// (ADR 0064; `cargos.rs` é de F).
    Cargo(Option<(u64, cargos::PedidoDeCargo)>),
    BanPerson {
        person: PersonId,
        reason: Option<String>,
        expires_at: Option<i64>,
    },
    RemoveMessage {
        message: MessageId,
    },
    /// Os três verbos do protocolo 13: o portão de versão já passou em
    /// `Connection`, antes de o comando entrar na fila.
    EditMessage {
        message: MessageId,
        body: String,
    },
    UnbanPerson {
        person: PersonId,
    },
    ListBans,
    MovePerson {
        person: PersonId,
        voice_room: VoiceRoomId,
    },
    DeleteVoiceRoom {
        voice_room: VoiceRoomId,
    },
    DeleteChannel {
        channel: ChannelId,
    },
    /// The one command that carries somewhere to answer.
    ///
    /// Everything else on this queue is a thing to do, confirmed — when it is
    /// confirmed at all — by the server announcing it to everybody. This is a
    /// question, and its answer is only useful to the caller who asked, while
    /// the box it fills is still open.
    WeighChannel {
        channel: ChannelId,
        answer: RespostaDoPeso,
    },
    /// A file, on a stream of its own. ADR 0027.
    ///
    /// Boxed because it is much larger than every other variant here, and an
    /// enum is as big as its largest arm — a path, three strings and two ids on
    /// every queued keystroke would be paid by all of them.
    Attach(Box<seele_core::enlace::Anexo>),
    /// Salvar um anexo nesta pasta. O nome não viaja aqui: é lido do histórico
    /// por quem executa, como o tipo alegado da prévia.
    SaveAttachment {
        attachment: seele_core::AttachmentId,
        folder: String,
    },
    /// A picture to look at, and where to put the verdict. ADR 0027.
    ///
    /// The second command here that carries somewhere to answer, and the same
    /// argument as [`Command::WeighChannel`]: the answer is only useful to the
    /// caller who asked, while the box it fills is still open.
    PreviewAttachment {
        attachment: seele_core::AttachmentId,
        answer: tokio::sync::oneshot::Sender<Preview>,
    },
    /// A tela escolhida, com o módulo do Cisco já carregado. §3.6.
    ///
    /// Boxeado pela mesma razão de [`Command::Attach`]: um enum é do tamanho do
    /// maior braço dele, e sem a caixa toda tecla digitada pagaria pelo peso
    /// desta.
    ShareScreen {
        pedido: Box<seele_core::PedidoDeTela>,
        limites: seele_core::LimitesDeTela,
    },
    /// Trocar os tetos da transmissão que já está de pé.
    ///
    /// Sem caixa: não carrega módulo nem captura, só três números.
    AdjustScreenLimits {
        limites: seele_core::LimitesDeTela,
    },
    StopScreenShare,
    /// Pede um quadro-chave a quem está compartilhando.
    ///
    /// **Quem chega no meio de uma transmissão precisa dele para ver alguma
    /// coisa.** Um fluxo H.264 é um quadro-chave e uma corrente de diferenças
    /// que só fazem sentido a partir dele; o codificador manda um no começo e
    /// depois **só quando alguém pede**. Entrar numa sala em que já se
    /// compartilha era receber deltas sobre um passado que nunca se viu, e ficar
    /// olhando para o nada — «quando alguém entra numa call que alguém tá
    /// compartilhando tela, a pessoa não consegue ver a transmissão».
    ///
    /// O pedido existia dos dois lados desde sempre: `Client::request_key_frame`
    /// aqui e `ClientMessage::RequestKeyFrame` no servidor, que já o traduz num
    /// aviso a quem compartilha. **Ninguém chamava nenhum dos dois.**
    RequestKeyFrame {
        tela: seele_core::ScreenId,
    },
    /// Passar a receber a imagem de uma transmissão, ou parar.
    Assistir {
        tela: seele_core::ScreenId,
        quero: bool,
    },
    /// Dizer ao servidor o que esta máquina aceita no caminho entre pares.
    ConsentirNoCaminhoEntrePares {
        pares_que_atende: u8,
        assiste_por_par: bool,
    },
    /// **O foco de voz mudou: olhe a voz de novo** (E3 da 0.17). Não faz nada
    /// além de dar uma volta no laço, que termina no olhar da porta da voz;
    /// com a resposta, ela sai depois desse olhar. Ver [`foco::FocoDeVoz::focar`].
    OlharAVoz(Option<std::sync::mpsc::Sender<()>>),
    Shutdown,
}

/// Qual transmissão esta sessão escolheu assistir. ADR 0054.
///
/// # Três estados, e são três de propósito
///
/// A ausência de escolha e a escolha de não ver **não são a mesma coisa**, e
/// tratá-las como uma custou um defeito de campo: «cliquei pra não ver, ele fecha
/// e depois abre novamente sozinho».
///
/// - [`Self::NinguemEscolheu`] — o servidor decide, e ele liga todo mundo na
///   primeira transmissão da sala. É o certo para quem não disse nada: ninguém
///   quer clicar para ver a única coisa que está acontecendo.
/// - [`Self::Nenhuma`] — escolheu **não** ver. Vale contra o religar do servidor.
/// - [`Self::Esta`] — escolheu esta, e a imagem pode ainda não ter chegado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscolhaDeTela {
    /// Ninguém escolheu. O religar automático do servidor vale.
    NinguemEscolheu,
    /// Escolheu não assistir nenhuma.
    Nenhuma,
    /// Escolheu esta.
    Esta(seele_core::ScreenId),
}

impl EscolhaDeTela {
    /// O `ScreenId` escolhido, se houver um.
    #[must_use]
    pub const fn tela(self) -> Option<seele_core::ScreenId> {
        match self {
            Self::Esta(tela) => Some(tela),
            Self::NinguemEscolheu | Self::Nenhuma => None,
        }
    }
}

/// Os túneis de jogo de quem joga, como `Connection::tuneis_abertos` os lê.
#[derive(Default)]
struct TabelaDeTuneis {
    /// O último relatório de cada túnel que está de pé.
    abertos: std::collections::BTreeMap<(String, String), RelatorioDeTunel>,
    /// Quantos relatórios finais (`encerrado`) cada túnel já teve.
    fins: std::collections::BTreeMap<(String, String), u64>,
}

/// State the driver thread writes and the shell reads.
struct Shared {
    /// O caminho de subida que a sonda mediu, em bits por segundo.
    ///
    /// Zero é «ninguém mediu ainda»: a sonda mede **enquanto a tela transmite**,
    /// porque é a tela que enche o cano. Quem o lê é a casca, para guardá-lo na
    /// lista de conhecidos e devolvê-lo à sonda na próxima visita a este mesmo
    /// servidor — é o que evita os doze segundos de imagem ruim que a escada
    /// gasta reaprendendo um cano que já mediu.
    ///
    /// Átomo e não `Event`, pela razão que o `rtt` já dá: é valor de agora, e
    /// não coisa que alguém precise ver acontecer.
    caminho_medido: std::sync::atomic::AtomicU32,
    room: Mutex<Room>,
    listeners: Mutex<Vec<Arc<dyn EventListener>>>,
    /// O `SessaoId` desta conexão (E3 da 0.17): ver [`Carimbo`]. Fixo do
    /// começo ao fim; não muda com a reconexão.
    sessao: u64,
    /// A geração do [`Carimbo`]: 1 na entrada, +1 a cada `Aviso::Reconectado`
    /// do enlace, no braço dele em `drive`. Aqui, e não na `Connection`,
    /// porque quem a sobe é a thread do driver e quem a lê é a casca e o
    /// [`Shared::notify`], que carimba cada evento.
    geracao: std::sync::atomic::AtomicU64,
    /// The voice path, and what the person chose for it.
    ///
    /// **Pendência 52.** A voz só existe dentro de uma sala de voz, então fora
    /// dela não há `Voice` nenhuma para lembrar o microfone escolhido, o mudo ou
    /// o modo — e o que a pessoa escolhe fora de sala tem de valer quando a voz
    /// abrir. Por isso a escolha mora aqui, ao lado da voz, e não dentro dela.
    ///
    /// Continua havendo **uma** resposta para «qual aparelho»: com a voz viva,
    /// a troca passa por ela e a escolha só é gravada se a troca deu certo; sem
    /// voz, a escolha é só gravada. Ver [`VozDaSala`].
    voice: VozDaSala<Voice>,
    /// What it takes to open a voice path on the connection that is up now.
    ///
    /// Here, and not only on the driver thread, so that switching microphone can
    /// answer on the caller's thread with a real error. Sent through the command
    /// queue instead, a device that is gone would fail somewhere nobody is
    /// listening, and the screen would show a pick that never took.
    ///
    /// Rewritten on every reconnection: the ssrc and the channel are both new.
    media: Mutex<Option<(MediaChannel, Ssrc)>>,
    nickname: Mutex<String>,
    link_state: AtomicU8,
    /// Round trip in microseconds. Integer because atomics have no `f32`, and
    /// microseconds because milliseconds would round a fast local link to zero.
    rtt_micros: std::sync::atomic::AtomicU64,
    /// O jitter de chegada deste receptor, em microssegundos (RFC 3550).
    ///
    /// Guardado aqui porque quem o calcula é [`measure`], no laço de voz, e quem
    /// o mostra é o [`Connection::snapshot`], na casca — e antes disto ele era
    /// calculado, usado no Sync Ratio e jogado fora, enquanto a tela lia o zero
    /// que o servidor manda de propósito (`session.rs` diz em comentário que o
    /// servidor não tem como medir jitter, porque jitter se mede no receptor).
    ///
    /// Microssegundos inteiros, e não os milissegundos em `f32` que a tela
    /// mostra, pelo mesmo motivo de [`Self::rtt_micros`] logo acima: não há
    /// átomo de `f32`, e um cadeado por quadro de interface seria contenção por
    /// nada. Um jitter de rede honesto vive na casa das unidades de
    /// milissegundo, então um milissegundo inteiro arredondaria a diferença
    /// entre um enlace bom e um ótimo para o mesmo número.
    jitter_de_chegada_micros: std::sync::atomic::AtomicU64,
    signal: AtomicU8,
    running: AtomicBool,
    /// Onde o enlace está, para o `Snapshot` contar à casca.
    ///
    /// Átomos e não um `Mutex`: isto é lido a cada quadro de interface e
    /// escrito raramente, e um cadeado por leitura seria contenção por nada.
    /// Zero segundos restantes significa "no ar".
    link_battery: AtomicBool,
    link_seconds: std::sync::atomic::AtomicU64,
    link_attempts: std::sync::atomic::AtomicU32,
    /// Por qual caminho esta conversa saiu, quando a chegada soube dizer.
    ///
    /// Escrito uma vez, quando a sessão sobe, e lido a cada quadro de interface.
    /// Um `Mutex` e não um átomo porque é `&'static str` e porque a escrita
    /// acontece exatamente uma vez — a contenção que os átomos de telemetria
    /// evitam não existe aqui.
    ///
    /// `None` é uma resposta, e é a que a casca recebe quando a trilha não sabe
    /// dizer: sem informação a tela **não escreve nada**. Ver
    /// `seele_core::chegada::caminho`.
    ///
    /// Não é reescrito quando o enlace reconecta. A reconexão de
    /// `seele_core::enlace` volta ao mesmo endereço do mesmo destino, então a
    /// forma não muda; o que poderia mudar é o `avisou`, e trocar o nome na tela
    /// por causa de um `send_to` que falhou num soluço de rede seria movimento
    /// sem informação.
    caminho: Mutex<Option<&'static str>>,
    /// A impressão do certificado que o aperto de mão conferiu, gravada antes
    /// de a casca poder perguntar. Ver [`Connection::impressao_do_servidor`].
    ///
    /// Não é reescrita quando o enlace reconecta: a reconexão de
    /// `seele_core::enlace` volta ao mesmo destino, e uma chave diferente
    /// nele é recusada pelo pin (`PinDecision::Changed`).
    impressao_do_servidor: Mutex<Option<String>>,
    /// Quantas vezes o histórico mudou desde que esta sessão começou.
    ///
    /// Existe para o [`Snapshot`] poder dizer "mudou" sem carregar a conversa
    /// inteira. Antes ele carregava: cada quadro de interface clonava todo
    /// apelido e todo corpo já dito, serializava em JSON e atravessava a ponte
    /// — duas vezes por segundo, custando proporcionalmente ao tamanho da
    /// conversa. Numa sessão longa isso é atraso que só cresce, e foi assim que
    /// apareceu num teste entre duas máquinas.
    ///
    /// O número em si não significa nada; só a diferença significa. A casca
    /// guarda o último que desenhou e busca o histórico quando ele muda.
    messages_revision: std::sync::atomic::AtomicU64,
    /// Quantas vezes o FORA DO AR pode ter mudado. Pelo mesmo motivo do
    /// `messages_revision`, e por um a mais: a lista de membros fica **fora do
    /// `Snapshot`** (P6), que todo MOD da API 5 para baixo lê, e este número é a
    /// única coisa dela que viaja nele. A casca relê
    /// [`Connection::membros_fora_do_ar`] quando ele muda.
    fora_do_ar_revision: std::sync::atomic::AtomicU64,
    /// Quantas vezes a imagem do servidor mudou desde que esta sessão começou.
    ///
    /// Existe pelo mesmo motivo do `messages_revision`, num tamanho menor: o
    /// [`Snapshot`] é lido a cada quadro de interface, e carregar os bytes nele
    /// seria cloná-los, serializá-los em JSON e atravessar a ponte com eles
    /// duas vezes por segundo — por um valor que muda quando alguém aperta um
    /// botão e nunca mais.
    ///
    /// O número em si não significa nada; só a diferença significa. A casca
    /// guarda o último que desenhou e busca a imagem quando ele muda.
    icon_revision: std::sync::atomic::AtomicU64,
    /// O mesmo número, para as imagens **das pessoas**.
    ///
    /// Um só para todas, e não um por pessoa: quem desenha os retratos desenha
    /// todos no mesmo quadro, então uma casca que descobre que algo mudou
    /// rebusca o que está na tela e pronto. Um contador por pessoa custaria um
    /// campo em cada `Person` do `Snapshot`, duas vezes por segundo, para
    /// distinguir um caso que ninguém trata separadamente.
    person_icons_revision: std::sync::atomic::AtomicU64,
    /// O perfil público (0.19, protocolo 12): o protocolo que a conexão
    /// negociou e as imagens já baixadas. Ver [`perfis`].
    perfis: perfis::DoPerfil,
    /// Quem está esperando o peso de uma Linha, e por qual Linha.
    ///
    /// A única pergunta com resposta deste crate. Todo o resto que a casca pede
    /// é ordem — o servidor confirma anunciando a mudança a todo mundo —, e este é
    /// o número de uma frase que só serve a quem perguntou, enquanto a caixa
    /// que ela enche estiver aberta.
    ///
    /// Uma lista e não um mapa por Linha: duas caixas sobre a mesma Linha ao
    /// mesmo tempo não acontecem numa janela só, e se acontecessem um mapa
    /// atenderia uma e deixaria a outra esperando para sempre. Aqui as duas
    /// recebem a mesma resposta.
    pending_weights: Mutex<Vec<(ChannelId, RespostaDoPeso)>>,
    /// **O último relatório de cada túnel de jogo de quem joga**, por MOD e
    /// nome. A casca que acaba de abrir (ou a faixa que recarregou) lê o que
    /// está de pé por [`Connection::tuneis_abertos`] em vez de esperar o
    /// próximo relatório. Sai daqui quando o túnel acaba (`encerrado`), quando
    /// o MOD é fechado, e com a sessão.
    tuneis: Mutex<TabelaDeTuneis>,
    /// Os limites que esta sessão pediu para a transmissão de tela dela.
    ///
    /// **Mora aqui porque a janela não sobrevive à transmissão.** O §5 manda
    /// mostrar o que está saindo ao lado do que foi pedido, e o que foi pedido
    /// não atravessa o fio em lugar nenhum — o `ScreenHeader` carrega resolução
    /// e codec, nunca a escolha. A casca gráfica cobria o buraco guardando a
    /// última escolha numa variável de JavaScript, e uma janela recarregada
    /// perdia metade da comparação enquanto a tela continuava saindo.
    ///
    /// Um `Mutex` e não um átomo: são três números e um `Option`, e a escrita
    /// acontece quando alguém aperta um botão — a contenção que os átomos de
    /// telemetria evitam não existe aqui.
    ///
    /// Limpo quando a transmissão desta pessoa acaba, e é a mesma regra que a
    /// casca já escrevia do lado dela: guardá-lo faria o painel da próxima
    /// comparar o que está saindo agora com um teto de outra vez.
    limites_da_tela: Mutex<Option<LimitesDeTela>>,
    /// Se o teto pausou a transmissão **desta pessoa**, e por quê.
    ///
    /// Ao lado do pedido porque é a mesma espécie de coisa: algo que só esta
    /// ponta sabe sobre a própria transmissão, e que o `Room` — o que o
    /// servidor contou — nunca viu. Escrita pelo `Aviso::TelaParou`, apagada
    /// pelo `Aviso::TelaAbriu` da mesma tela (a volta) e pelo fim da
    /// transmissão. Ver [`seele_core::retrato::ParadaDaMinhaTela`].
    parada_da_tela: Mutex<seele_core::retrato::ParadaDaMinhaTela>,
    /// Qual codificador de vídeo abriu a transmissão desta pessoa, e de qual
    /// tela: o que o último `Aviso::TelaAbriu` com nome disse.
    ///
    /// Com a tela junto porque o nome só descreve **aquela** transmissão: o
    /// retrato o veste só na tela de mesmo número
    /// ([`Self::vestir_o_codificador`]). Limpo quando a transmissão desta
    /// pessoa acaba, como o pedido acima.
    codificador_da_tela: Mutex<Option<(seele_core::ScreenId, String)>>,
    /// Qual transmissão esta sessão escolheu assistir, e em que estado a escolha
    /// está. ADR 0054.
    ///
    /// # Por que ela mora aqui, e não na janela
    ///
    /// Porque a janela é descartável e a sessão não. A escolha vivia numa
    /// variável de JavaScript (`telaQuerida`), então uma recarga no meio de uma
    /// sessão a perdia — e com ela a marca do botão, o controle de som daquela
    /// tela e a resposta a «qual delas é esta». Deste lado ela sobrevive à
    /// janela, e é deste lado que o servidor pergunta.
    ///
    /// Os três estados são os mesmos que a janela já distinguia, e continuam
    /// sendo três de propósito — ver [`EscolhaDeTela`].
    tela_escolhida: Mutex<EscolhaDeTela>,
    /// A transmissão alheia que o **servidor** ligou sozinho, à parte da escolha.
    ///
    /// Ver [`seele_core::retrato::TelaLigada`]. É o que o retrato usa quando
    /// ninguém escolheu ([`EscolhaDeTela::NinguemEscolheu`]): sem ela, quem não
    /// clicou em VER recebia a imagem e o retrato dizia «ninguém está
    /// compartilhando».
    tela_ligada: Mutex<seele_core::retrato::TelaLigada>,
    /// A última lista de fontes que [`Connection::fontes_de_tela`] devolveu.
    ///
    /// **Existe porque o número que a casca devolve é o índice desta lista**, e
    /// não um identificador do sistema: um `Alvo` da WGC não publica `HWND`
    /// nenhum, e um número estável entre duas listagens não existe nos dois
    /// sistemas. Ver `seele_core::FonteDeTela::id`.
    ///
    /// E porque o alvo nativo não atravessa a travessia: a casca recebe nome e
    /// tamanho, e o `SCWindow` — que é o que a captura precisa — fica deste
    /// lado, esperando a escolha.
    fontes_de_tela: Mutex<Vec<seele_core::FonteDeTela>>,
    /// Por onde os quadros da câmera entram, enquanto há uma câmera pedida ou
    /// no ar.
    ///
    /// `None` é «esta sessão não está mandando câmera», e é o que
    /// [`Connection::quadro_da_camera`] responde com
    /// [`ConnectionError::CameraNotLive`]. Trocada a cada
    /// [`Connection::compartilhar_camera`] e esvaziada por
    /// [`Connection::parar_de_compartilhar`].
    camera: Mutex<Option<seele_core::de_fora::EntradaDeFora>>,
}

impl Shared {
    /// Says the conversation moved: bumps the revision, then notifies.
    ///
    /// One call and not two, because the two must never happen apart. They did:
    /// `Command::OpenChannel` clears the room's messages and emitted the event
    /// **without** bumping the revision, so a shell that refetches only when the
    /// number moves — which is the whole point of the number — swallowed it.
    /// Switching Channel kept the previous Channel's conversation on screen until
    /// somebody said something.
    ///
    /// That was a regression of the fix that stopped the snapshot carrying the
    /// whole history: before it, the shell re-read every message twice a second
    /// and a cleared list showed up on the next tick regardless. Making the
    /// shell smarter made this notification load-bearing, and it was not
    /// carrying its half.
    ///
    /// The bump comes first. A shell reacting to the event would otherwise read
    /// the old number and conclude there is nothing to fetch.
    fn messages_changed(&self) {
        self.messages_revision.fetch_add(1, Ordering::Relaxed);
        self.notify(&Event::MessagesChanged);
    }

    /// Guarda onde o enlace está, para o `Snapshot` contar à casca.
    fn gravar_enlace(&self, estado: seele_core::Link, restante: Option<std::time::Duration>) {
        let na_bateria = matches!(estado, seele_core::Link::InternalBattery { .. });
        self.link_battery.store(na_bateria, Ordering::Relaxed);
        self.link_seconds
            .store(restante.map_or(0, |q| q.as_secs()), Ordering::Relaxed);
        if let seele_core::Link::InternalBattery { attempts } = estado {
            self.link_attempts.store(attempts, Ordering::Relaxed);
        }
    }

    /// Fecha a porta por onde os quadros da câmera entravam.
    fn largar_a_camera(&self) {
        if let Ok(mut camera) = self.camera.lock() {
            *camera = None;
        }
    }

    /// Guarda o que esta pessoa pediu para a tela dela, ou esquece.
    ///
    /// `None` esquece, e é o que acontece quando a transmissão acaba: um teto
    /// guardado além dela faria o painel da próxima comparar o que está saindo
    /// agora com uma escolha de outra vez.
    fn gravar_pedido_da_tela(&self, limites: Option<LimitesDeTela>) {
        if let Ok(mut guardado) = self.limites_da_tela.lock() {
            *guardado = limites;
        }
    }

    /// O teto pausou a transmissão desta pessoa. Devolve se o retrato mudou.
    fn a_tela_parou(&self, tela: seele_core::ScreenId, motivo: seele_core::MotivoDeParada) -> bool {
        self.parada_da_tela
            .lock()
            .is_ok_and(|mut parada| parada.parou(tela, nome_da_parada(motivo)))
    }

    /// O fluxo de uma tela abriu: se era a pausada, a pausa acabou. Devolve se
    /// o retrato mudou.
    fn a_tela_voltou(&self, tela: seele_core::ScreenId) -> bool {
        self.parada_da_tela
            .lock()
            .is_ok_and(|mut parada| parada.voltou(tela))
    }

    /// A transmissão desta pessoa acabou, e a pausa dela com ela.
    fn esquecer_a_parada_da_tela(&self) {
        if let Ok(mut parada) = self.parada_da_tela.lock() {
            parada.esquecer();
        }
    }

    /// A pausa da transmissão desta pessoa, para o retrato.
    ///
    /// Um cadeado envenenado responde «sem pausa», que é o retrato de antes de
    /// este campo existir.
    fn parada_da_tela(&self) -> seele_core::retrato::ParadaDaMinhaTela {
        self.parada_da_tela
            .lock()
            .map(|parada| *parada)
            .unwrap_or_default()
    }

    /// Guarda qual codificador abriu a transmissão desta pessoa, ou esquece.
    fn gravar_codificador_da_tela(&self, de: Option<(seele_core::ScreenId, String)>) {
        if let Ok(mut guardado) = self.codificador_da_tela.lock() {
            *guardado = de;
        }
    }

    /// Põe na foto o codificador da transmissão desta pessoa.
    ///
    /// Só na tela **desta pessoa** e **de mesmo número** que o motor disse: o
    /// nome ao lado da tela de outra pessoa, ou de uma transmissão de antes,
    /// seria o rodapé afirmando o que ninguém mediu. Um cadeado envenenado
    /// deixa o `None`, que a tela lê como «não sei».
    fn vestir_o_codificador(&self, foto: &mut seele_core::retrato::FotoDaSala) {
        let Some((tela, nome)) = self
            .codificador_da_tela
            .lock()
            .ok()
            .and_then(|guardado| guardado.clone())
        else {
            return;
        };
        for em_curso in [&mut foto.minha_transmissao, &mut foto.tela]
            .into_iter()
            .flatten()
        {
            if em_curso.e_minha && em_curso.tela == tela.0 {
                em_curso.codificador = Some(nome.clone());
            }
        }
    }

    /// O que esta pessoa pediu para a tela dela, se ainda vale.
    ///
    /// Um cadeado envenenado responde `None`, que é a mesma resposta de «esta
    /// sessão não pediu nada»: a casca escreve travessão e não uma escolha que
    /// ninguém consegue mais afirmar.
    fn pedido_da_tela(&self) -> Option<LimitesDeTela> {
        self.limites_da_tela.lock().ok().and_then(|g| *g)
    }

    /// Qual transmissão esta sessão escolheu assistir. ADR 0054.
    ///
    /// Um cadeado envenenado responde [`EscolhaDeTela::NinguemEscolheu`], que é o
    /// comportamento de antes de haver escolha: o servidor decide.
    fn tela_escolhida(&self) -> EscolhaDeTela {
        self.tela_escolhida
            .lock()
            .map_or(EscolhaDeTela::NinguemEscolheu, |escolha| *escolha)
    }

    /// Guarda a escolha, e devolve se ela mudou.
    fn gravar_tela_escolhida(&self, escolha: EscolhaDeTela) -> bool {
        let Ok(mut guardada) = self.tela_escolhida.lock() else {
            return false;
        };
        if *guardada == escolha {
            return false;
        }
        *guardada = escolha;
        true
    }

    /// Esquece a escolha quando a transmissão escolhida saiu do ar. ADR 0054.
    ///
    /// **Volta a `NinguemEscolheu`, e não a `Nenhuma`.** A diferença é o defeito
    /// que a distinção entre os dois estados existe para evitar: `Nenhuma` é uma
    /// recusa que vale contra o religar do servidor, e herdá-la de uma
    /// transmissão que acabou faria a próxima a começar ser recusada por uma
    /// decisão que ninguém tomou sobre ela.
    fn esquecer_tela_que_saiu(&self) {
        // A que o servidor ligou também morre com a transmissão: uma leitura
        // de par abortada não manda `TelaFechou`, e a próxima com o mesmo
        // `ScreenId` não pode herdar a contagem desta. Cadeados na ordem de
        // `tela_abriu`: o do `Room`, depois o da tela ligada.
        if let Ok(room) = self.room.lock() {
            if let Ok(mut ligada) = self.tela_ligada.lock() {
                ligada.esquecer_se_saiu(&room);
            }
        }
        let Some(escolhida) = self.tela_escolhida().tela() else {
            return;
        };
        let ainda_existe = self
            .room
            .lock()
            .is_ok_and(|room| room.telas.contains_key(&escolhida));
        if !ainda_existe {
            let _ = self.gravar_tela_escolhida(EscolhaDeTela::NinguemEscolheu);
        }
    }

    /// A leitura `leitura` de uma tela abriu, na geração que está no ar. Ver
    /// [`seele_core::retrato::TelaLigada`].
    fn tela_abriu(&self, tela: seele_core::ScreenId, leitura: u64) {
        let geracao = self.carimbo().geracao;
        if let Ok(room) = self.room.lock() {
            if let Ok(mut ligada) = self.tela_ligada.lock() {
                ligada.abriu(&room, tela, geracao, leitura);
            }
        }
    }

    /// A leitura `leitura` de uma tela acabou. Devolve se ela era a vigente
    /// daquela tela; um cadeado envenenado responde que sim, que é o
    /// comportamento de antes de haver identidade de leitura.
    fn tela_fechou(&self, tela: seele_core::ScreenId, leitura: u64) -> bool {
        self.tela_ligada
            .lock()
            .map_or(true, |mut ligada| ligada.fechou(tela, leitura))
    }

    /// Um quadro da leitura `leitura` vai à casca **só se ela é a vigente da
    /// tela**. A casca conhece a tela e não a leitura: o quadro de uma leitura
    /// já substituída (o abort ainda não efetivou) cairia no decodificador da
    /// nova e o contaminaria. Um cadeado envenenado deixa passar, como antes.
    fn quadro_da_tela(&self, tela: seele_core::ScreenId, leitura: u64, chave: bool, bytes: &[u8]) {
        let vigente = self
            .tela_ligada
            .lock()
            .map_or(true, |ligada| ligada.e_vigente(tela, leitura));
        if !vigente {
            tracing::debug!(%tela, leitura, "quadro de uma leitura de tela substituída: descartado");
            return;
        }
        self.notify(&Event::ScreenFrame {
            screen: tela.0,
            key: chave,
            data: base64_de(bytes),
        });
    }

    /// Se o som da leitura `leitura` vai à fila da mistura: o mesmo filtro do
    /// quadro ([`Self::quadro_da_tela`]), pela leitura vigente da tela. Um
    /// cadeado envenenado deixa passar, como antes.
    fn som_da_leitura_vale(&self, tela: seele_core::ScreenId, leitura: u64) -> bool {
        self.tela_ligada
            .lock()
            .map_or(true, |ligada| ligada.e_vigente(tela, leitura))
    }

    /// Uma volta nova do enlace: a tela ligada da anterior não é desta.
    fn tela_ligada_nova_geracao(&self) {
        if let Ok(mut ligada) = self.tela_ligada.lock() {
            ligada.nova_geracao();
        }
    }

    /// Esquece a tela ligada, se for esta.
    fn esquecer_tela_ligada(&self, tela: seele_core::ScreenId) {
        if let Ok(mut ligada) = self.tela_ligada.lock() {
            ligada.esquecer(tela);
        }
    }

    /// A transmissão que esta sessão está assistindo, para o retrato: a
    /// escolha explícita, ou — sem escolha — a que o servidor ligou nesta
    /// geração. Ver [`seele_core::retrato::assistida_no_retrato`].
    fn tela_assistida(&self) -> Option<seele_core::ScreenId> {
        let escolhida = match self.tela_escolhida() {
            EscolhaDeTela::Esta(tela) => Some(Some(tela)),
            EscolhaDeTela::Nenhuma => Some(None),
            EscolhaDeTela::NinguemEscolheu => None,
        };
        let geracao = self.carimbo().geracao;
        let ligada = self
            .tela_ligada
            .lock()
            .ok()
            .and_then(|ligada| ligada.agora(geracao));
        seele_core::retrato::assistida_no_retrato(escolhida, ligada)
    }

    /// O estado do enlace como a casca o vê.
    fn enlace(&self) -> LinkState {
        if self.link_battery.load(Ordering::Relaxed) {
            LinkState::InternalBattery {
                remaining_seconds: self.link_seconds.load(Ordering::Relaxed),
                attempts: self.link_attempts.load(Ordering::Relaxed),
            }
        } else {
            LinkState::Online
        }
    }

    /// Guarda o jitter de chegada que o laço de voz acabou de medir.
    ///
    /// Só quem chama [`jitter_para_a_tela`] tem o que gravar aqui, e é de
    /// propósito: o número certo e o errado são ambos milissegundos em `f32`, e
    /// nada além daquela função separa um do outro.
    ///
    /// Devolve se o número que a casca lê mudou de verdade — o suficiente para
    /// valer um redesenho. A comparação é em microssegundos inteiros, que é como
    /// ele é guardado: comparar os `f32` de origem acenderia o evento a cada
    /// volta de telemetria, quatro vezes por segundo, porque um jitter alisado
    /// muda no último bit sempre.
    fn gravar_jitter_de_chegada(&self, ms: f32) -> bool {
        // Sem limitar a zero à mão: em Rust o `as` de ponto flutuante para
        // inteiro **satura**, e leva tanto o negativo quanto o `NaN` a zero
        // sozinho. Um `max` aqui seria uma guarda que nunca dispara, e uma
        // guarda que nunca dispara faz o teste que a exercita passar por si
        // mesmo em vez de guardar a propriedade.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "o `as` satura: negativo e NaN viram zero, e o teto de u64 está muito acima de qualquer jitter"
        )]
        let micros = (ms * 1000.0) as u64;
        let antes = self
            .jitter_de_chegada_micros
            .swap(micros, Ordering::Relaxed);
        // Meio milissegundo, que é a menor diferença que o rodapé arredonda para
        // outro número: `Math.round` na casca, `{:.0}` na TUI. Abaixo disso o
        // evento acordaria a tela para ela redesenhar o mesmo texto.
        antes.abs_diff(micros) >= 500
    }

    /// Guarda a impressão que o pin conferiu nesta conexão.
    ///
    /// `FirstContact` e `Matches` trazem a impressão; `Changed` e
    /// `InviteRefused` nunca chegam aqui, porque o aperto de mão já falhou.
    fn gravar_impressao(&self, decisao: &seele_core::PinDecision) {
        let impressao = match decisao {
            seele_core::PinDecision::FirstContact { fingerprint }
            | seele_core::PinDecision::Matches { fingerprint } => Some(fingerprint.clone()),
            seele_core::PinDecision::Changed { .. }
            | seele_core::PinDecision::InviteRefused { .. } => None,
        };
        if let Ok(mut slot) = self.impressao_do_servidor.lock() {
            *slot = impressao;
        }
    }

    /// Guarda por qual caminho a chegada saiu, se ela soube dizer.
    fn gravar_caminho(&self, caminho: Option<seele_core::chegada::Caminho>) {
        if let Ok(mut slot) = self.caminho.lock() {
            *slot = caminho.map(|caminho| caminho.nome());
        }
    }

    /// O caminho como a casca o vê: um nome estável, ou nada.
    fn caminho(&self) -> Option<&'static str> {
        self.caminho.lock().ok().and_then(|slot| *slot)
    }

    /// O jitter de chegada como a casca o vê, em milissegundos.
    fn jitter_de_chegada_ms(&self) -> f32 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "um jitter em microssegundos está muito abaixo da faixa exata do f32"
        )]
        let ms = self.jitter_de_chegada_micros.load(Ordering::Relaxed) as f32 / 1000.0;
        ms
    }

    /// Entrega o peso a quem perguntou por aquela Linha.
    ///
    /// Quem sobrar na lista é de outra Linha e continua esperando. Ninguém fica
    /// esperando para sempre: o remetente é largado quando a tarefa do enlace
    /// termina, e um `oneshot` largado acorda quem espera com erro — que é como
    /// esta ponte já diz "não há sessão".
    /// Takes the transfer reasons the room collected, and empties the queue.
    ///
    /// Drained rather than read: two transfers can be in the air at once, and
    /// the second one failing must not erase the reason the first one did. The
    /// `Room` keeps them in order and this hands them on in order.
    fn drain_transfers(&self) -> Vec<Transfer> {
        let Ok(mut room) = self.room.lock() else {
            return Vec::new();
        };
        room.transfers
            .drain(..)
            .map(|aviso| match aviso {
                seele_core::TransferNotice::Refused {
                    client_message_id,
                    reason,
                } => Transfer::RefusedBecause {
                    client_message_id: client_message_id.get(),
                    reason: refusal_of(reason),
                },
                seele_core::TransferNotice::Unavailable { attachment, reason } => {
                    Transfer::Unavailable {
                        attachment: attachment.get(),
                        reason: refusal_of(reason),
                    }
                }
                seele_core::TransferNotice::VolumeRecusado { token, motivo } => {
                    Transfer::VolumeRecusado {
                        token,
                        motivo: recusa_de_volume(motivo),
                    }
                }
            })
            .collect()
    }

    fn answer_weight(&self, weight: ChannelWeight) {
        let Ok(mut pending) = self.pending_weights.lock() else {
            return;
        };
        let mut esperando = Vec::new();
        for (channel, answer) in pending.drain(..) {
            if channel.get() == weight.channel {
                let _ = answer.send(Ok(weight));
            } else {
                esperando.push((channel, answer));
            }
        }
        *pending = esperando;
    }

    /// **Uma pesagem que o servidor recusou volta a quem perguntou**, com a
    /// causa. Devolve se havia alguém esperando.
    ///
    /// O servidor responde a um `WeighChannel` que não deu com o
    /// `PedidoRecusado` do `nao_deu`, e não com um `ChannelWeighed`. Sem isto,
    /// a espera ficava viva até a sessão acabar: o APAGAR de um canal ficava
    /// sem resposta, a caixa «Não consegui contar…» nunca abria, e cada clique
    /// a mais empilhava outra espera.
    ///
    /// **A mais antiga**, uma por recusa: o servidor responde aos pedidos de um
    /// mesmo ordinal na ordem em que chegaram (`AlertReason::PedidoRecusado`).
    fn recusar_peso(&self, causa: types::CausaDaRecusa) -> bool {
        let Ok(mut pending) = self.pending_weights.lock() else {
            return false;
        };
        if pending.is_empty() {
            return false;
        }
        let (_, answer) = pending.remove(0);
        let _ = answer.send(Err(causa));
        true
    }

    /// O carimbo de agora: a sessão e a geração que está no ar.
    fn carimbo(&self) -> Carimbo {
        Carimbo {
            sessao: self.sessao,
            geracao: self.geracao.load(Ordering::Acquire),
        }
    }

    /// Uma volta nova do enlace: a geração sobe, e o carimbo de antes passa a
    /// ser recusado por [`Connection::com`].
    fn nova_geracao(&self) -> u64 {
        self.geracao.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn notify(&self, event: &Event) {
        let listeners = match self.listeners.lock() {
            Ok(listeners) => listeners.clone(),
            Err(_) => return,
        };
        // Lido uma vez por evento: todos os ouvintes veem o mesmo carimbo.
        let carimbo = self.carimbo();
        for listener in listeners {
            listener.on_event_carimbado(carimbo, event.clone());
        }
    }
}

/// A live session.
///
/// Dropping it disconnects.
pub struct Connection {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    shared: Arc<Shared>,
    /// Por onde a thread da conexão diz que terminou, para
    /// [`Connection::encerrar_e_esperar`].
    ///
    /// O sinal sai quando `drive` volta — com a despedida do enlace já feita
    /// — e **antes** de o runtime cair. Num `Mutex` porque o `Receiver` não é
    /// `Sync` e a casca segura a `Connection` atrás de um `Arc`; `None` depois
    /// de alguém já ter visto o fim, e na `Connection` montada à mão dos testes.
    encerrada: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    /// O código por que esta conexão foi discada, quando foi: ver
    /// [`Connection::codigo_da_ligacao`].
    ligacao: Option<Codigo>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Connection")
            .field("running", &self.shared.running.load(Ordering::Relaxed))
            .finish()
    }
}

/// Qual metade do áudio uma troca mexeu.
///
/// Só para [`Connection::conferir_troca`]: a conferência é a mesma dos dois
/// lados, e duas cópias dela divergiriam no dia em que uma ganhasse um caso.
#[derive(Debug, Clone, Copy)]
enum Lado {
    Entrada,
    Saida,
}

/// **O CANCELAR de quem está ligando** (tarefa 16 da F7): o pedido de largar a
/// chegada em voo de [`Connection::connect_cancelavel`].
///
/// Entregue antes do bloqueio e usado de outra linha. Pedido uma vez, fica
/// pedido: cancelar antes de a chegada começar a esperar também vale.
pub struct Cancelamento {
    pedido: tokio::sync::watch::Sender<bool>,
}

impl Cancelamento {
    /// Um cancelamento ainda não pedido.
    #[must_use]
    pub fn novo() -> Arc<Self> {
        Arc::new(Self {
            pedido: tokio::sync::watch::Sender::new(false),
        })
    }

    /// Larga a ligação em voo: o QUIC de cada candidato fecha, a sala que tocava
    /// vê `ParouDeTocar(Cancelada)`, e o `connect` volta com
    /// [`ConnectionError::Cancelada`]. Depois de a sessão abrir, não faz nada:
    /// sair é o `disconnect`.
    pub fn cancelar_ligacao(&self) {
        self.pedido.send_replace(true);
    }

    /// Fica pronto quando o cancelamento é pedido.
    async fn pedido(&self) {
        let mut pedido = self.pedido.subscribe();
        // O `Sender` mora em `self`, e não fecha enquanto isto espera.
        let _ = pedido.wait_for(|pedido| *pedido).await;
    }
}

impl Connection {
    /// Connects, authenticates, and starts the driver thread.
    ///
    /// **Blocks** until the session reaches CONEXÃO SEGURA or fails.
    ///
    /// A porta antiga, e o que ela perde: o erro sem a trilha. Ela fica porque
    /// oito arquivos de teste entram por aqui, e trocá-los todos no commit que
    /// abre a porta nova transformaria um passo barato num caro. Quem desenha
    /// uma tela de falha quer [`Connection::connect_with_trail`].
    ///
    /// # Errors
    ///
    /// Every failure is a [`ConnectionError`] variant, never a string: a shell has to
    /// be able to write its own sentence for each one.
    pub fn connect(config: ConnectConfig) -> Result<(Arc<Self>, Trust), ConnectionError> {
        Self::connect_with_trail(config).map_err(|falha| falha.error)
    }

    /// O mesmo, respondendo também por onde a chegada passou.
    ///
    /// A trilha é o que faltava quando o teste de campo das duas casas falhou:
    /// a tela dizia «não consegui conectar» sobre quatro tentativas das quais
    /// nenhuma tinha nome. Ela sobrevive à falha e atravessa inteira — ver
    /// [`ConnectFailure`].
    ///
    /// # Errors
    ///
    /// [`ConnectFailure`], que é o [`ConnectionError`] de sempre mais a trilha. Ela
    /// vem vazia quando a falha é de antes de haver chegada: um endereço que
    /// não resolve, uma identidade que não abre, uma thread que não sobe.
    pub fn connect_with_trail(config: ConnectConfig) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        Self::abrir(config, None, None, None, None)
    }

    /// O mesmo, com alguém ouvindo a chegada **enquanto** ela acontece.
    ///
    /// A porta que faltava, e a falta era estrutural. `seele_core` publica uma
    /// etapa por instante da travessia desde a tarefa 8, e nada em produção as
    /// lia: [`Connection::connect`] bloqueia, quem se inscreve por [`Connection::subscribe`]
    /// só tem o `Arc<Connection>` depois que ela volta, e quando ela volta a travessia
    /// inteira já terminou. O ouvinte tinha de entrar **antes** do bloqueio, e é
    /// só isso que esta função faz de diferente.
    ///
    /// Chega como [`Event::ConnectStageChanged`], na thread do motor, como todo
    /// evento deste crate — quem marshala é a casca (`specs/06-clientes-gui.md`).
    /// A última etapa (`Dentro` ou `Desistiu`) sai antes desta função devolver:
    /// o motor espera o reencaminhamento terminar, e ele termina sozinho quando
    /// a `Chegada` morre.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Connection::connect_with_trail`].
    pub fn connect_watching(
        config: ConnectConfig,
        olhos: Arc<dyn EventListener>,
    ) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        Self::abrir(config, Some(olhos), None, None, None)
    }

    /// O mesmo, com um [`Cancelamento`]: quem liga pode desistir enquanto a
    /// chegada anda (tarefa 16 da F7). É a porta do CANCELAR de quem está
    /// chamando alguém.
    ///
    /// O `Cancelamento` é entregue **antes** do bloqueio, como o ouvinte: é de
    /// outra linha que alguém o usa, enquanto esta espera.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Connection::connect_with_trail`], e
    /// [`ConnectionError::Cancelada`] quando o cancelamento chega antes da
    /// sessão, com a trilha acabando em [`ConnectStage::Cancelada`].
    pub fn connect_cancelavel(
        config: ConnectConfig,
        olhos: Option<Arc<dyn EventListener>>,
        cancelamento: Arc<Cancelamento>,
    ) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        Self::abrir(config, olhos, Some(cancelamento), None, None)
    }

    /// **A porta da multi-sessão** (E3 da 0.17): o mesmo de
    /// [`Connection::connect_cancelavel`], com a conexão inscrita no foco de voz
    /// da máquina.
    ///
    /// Com `foco`, a voz desta conexão só abre quando ela tem o foco
    /// ([`foco::FocoDeVoz`]); sentada numa sala sem ele, o retrato diz
    /// [`AberturaDoAudio::EmOutraSessao`]. A conexão é inscrita antes de o
    /// driver subir, e sai do foco quando acaba, por qualquer caminho. Sem
    /// `foco`, é a porta de sempre.
    ///
    /// `sessao` é o `SessaoId` da casca, que entra no [`Carimbo`] desta
    /// conexão; `None` sorteia um pelo contador ([`SessaoId::nova`]), como as
    /// outras portas. Um `Some(0)` também é trocado por um novo: o 0 não é
    /// sessão.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Connection::connect_cancelavel`].
    pub fn connect_com_foco(
        config: ConnectConfig,
        olhos: Option<Arc<dyn EventListener>>,
        cancelamento: Option<Arc<Cancelamento>>,
        foco: Option<Arc<foco::FocoDeVoz>>,
        sessao: Option<u64>,
    ) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        Self::abrir(config, olhos, cancelamento, foco, sessao)
    }

    /// O número desta conexão no foco de voz, quando ela entrou num.
    #[must_use]
    pub fn id_no_foco(&self) -> Option<u64> {
        self.shared.voice.id_no_foco()
    }

    /// Esta conexão tem a voz da máquina agora? Sem foco, sempre.
    #[must_use]
    pub fn tem_o_foco_de_voz(&self) -> bool {
        self.shared.voice.tem_foco()
    }

    /// De qual sessão esta conexão é, e em qual volta do enlace ela está agora.
    #[must_use]
    pub fn carimbo(&self) -> Carimbo {
        self.shared.carimbo()
    }

    /// **A porta dos comandos de sessão** (E3 da 0.17): devolve esta conexão
    /// só se o carimbo que o comando trouxe é o de agora.
    ///
    /// `conexao.com(carimbo)?.send_message(...)`: um clique feito sobre a tela
    /// de outra sessão, ou de antes de uma reconexão, é recusado aqui, antes de
    /// qualquer coisa sair, e nunca vai calado para a conexão errada.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::SessaoNaoConfere`], com os dois carimbos, quando a
    /// sessão ou a geração não batem.
    pub fn com(&self, carimbo: Carimbo) -> Result<&Self, ConnectionError> {
        let esperado = self.carimbo();
        if carimbo == esperado {
            Ok(self)
        } else {
            tracing::warn!(
                ?esperado,
                ?carimbo,
                "um comando chegou com o carimbo de outra sessão, ou de antes de uma reconexão: recusado, e nada saiu"
            );
            Err(ConnectionError::SessaoNaoConfere {
                esperado,
                veio: carimbo,
            })
        }
    }

    /// O corpo que as portas compartilham.
    fn abrir(
        config: ConnectConfig,
        olhos: Option<Arc<dyn EventListener>>,
        cancelamento: Option<Arc<Cancelamento>>,
        foco: Option<Arc<foco::FocoDeVoz>>,
        sessao: Option<u64>,
    ) -> Result<(Arc<Self>, Trust), ConnectFailure> {
        // O `SessaoId` nasce na tentativa, antes de tudo: as etapas da chegada
        // já saem carimbadas com ele. O 0 não é sessão.
        let sessao = sessao
            .filter(|&sessao| sessao != 0)
            .unwrap_or_else(SessaoId::nova);
        // **O apelido que o `Hello` não leva é recusado aqui, antes de tudo**
        // (pendência 58, item 55; lote C55 da F5). O teto é do fio, em bytes,
        // e o apelido está na mão antes de qualquer endereço: conferido aqui,
        // nada é resolvido, discado nem lido do disco. Sem isto, o QUIC e o TLS
        // abriam, o `encode` do `Hello` o recusava, e a falha chegava à janela
        // como `Unreachable`, com o servidor de pé.
        if config.nickname.len() > seele_core::MAX_NICKNAME_LEN {
            tracing::warn!(
                bytes = config.nickname.len(),
                teto = seele_core::MAX_NICKNAME_LEN,
                "o apelido passa do teto do fio, em bytes: o Hello não o leva, e a conexão não abre"
            );
            return Err(ConnectionError::ApelidoGrandeDemais {
                limite_bytes: seele_core::MAX_NICKNAME_LEN as u64,
            }
            .into());
        }
        // **Um código, e não um endereço** (tarefa 30 da F7): não há o que
        // resolver, e quem diz onde a sala mora é o ponto de encontro. Cada
        // candidato que ele der confere o código dentro do TLS: é a
        // `Chegada::para_o_codigo` quem monta os destinos, e não o
        // `build_destino`, então o `codigo_esperado` não entra aqui.
        let alvo = match codigo_do_alvo(&config.server) {
            Some(codigo) => Alvo::Codigo(codigo),
            None => {
                let (address, server_name, pin_key) = resolve(&config.server)?;
                // The invite's other addresses, resolved here so the driver
                // thread gets values. An alternative that does not resolve is
                // dropped: the first address is the one the person is most
                // likely on the same network as, and refusing the whole link
                // over a spare would trade one path fewer for no path at all.
                let alternates: Vec<(SocketAddr, String, String)> = config
                    .alternate_servers
                    .iter()
                    .filter_map(|alternate| resolve(alternate).ok())
                    .collect();
                Alvo::Enderecos {
                    address,
                    server_name,
                    pin_key,
                    alternates,
                }
            }
        };
        // A origem, lida do mesmo `Alvo` que o `drive` disca: o código que
        // vai para a `Chegada::para_o_codigo` é o que a conexão vai dizer.
        let ligacao = match &alvo {
            Alvo::Codigo(codigo) => Some(*codigo),
            Alvo::Enderecos { .. } => None,
        };
        if config.codigo_esperado.is_some() && config.expected_fingerprint.is_some() {
            tracing::warn!(
                "a conexão trouxe código e impressão: vale o código, e a impressão não é conferida"
            );
        }
        let home = PathBuf::from(&config.home);
        // ADR 0061: a chave mora na pasta da identidade da máquina, quando o
        // launcher a diz; os pins e o resto continuam em `home`.
        let key = identity::load_or_create(&arquivo_da_identidade(&home)).map_err(|error| {
            tracing::warn!(%error, "identity unavailable");
            ConnectionError::IdentityUnavailable
        })?;
        let pins = Arc::new(FilePinStore::open(home.join("pins")).map_err(|error| {
            tracing::warn!(%error, "pin store unavailable");
            ConnectionError::IdentityUnavailable
        })?);

        let shared = Arc::new(Shared {
            caminho_medido: std::sync::atomic::AtomicU32::new(0),
            link_battery: AtomicBool::new(false),
            link_seconds: std::sync::atomic::AtomicU64::new(0),
            link_attempts: std::sync::atomic::AtomicU32::new(0),
            messages_revision: std::sync::atomic::AtomicU64::new(0),
            fora_do_ar_revision: std::sync::atomic::AtomicU64::new(0),
            icon_revision: std::sync::atomic::AtomicU64::new(0),
            person_icons_revision: std::sync::atomic::AtomicU64::new(0),
            perfis: perfis::DoPerfil::default(),
            room: Mutex::new(Room::new()),
            listeners: Mutex::new(Vec::new()),
            sessao,
            geracao: std::sync::atomic::AtomicU64::new(1),
            voice: VozDaSala::nova(config.audio, chosen_devices(&config)),
            media: Mutex::new(None),
            nickname: Mutex::new(config.nickname.clone()),
            link_state: AtomicU8::new(link_state_byte(LinkTrust::Offline)),
            rtt_micros: std::sync::atomic::AtomicU64::new(0),
            jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
            caminho: Mutex::new(None),
            impressao_do_servidor: Mutex::new(None),
            signal: AtomicU8::new(0),
            running: AtomicBool::new(false),
            pending_weights: Mutex::new(Vec::new()),
            tuneis: Mutex::new(TabelaDeTuneis::default()),
            limites_da_tela: Mutex::new(None),
            parada_da_tela: Mutex::new(Default::default()),
            codificador_da_tela: Mutex::new(None),
            tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
            tela_ligada: Mutex::new(seele_core::retrato::TelaLigada::default()),
            fontes_de_tela: Mutex::new(Vec::new()),
            camera: Mutex::new(None),
        });

        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        // **No foco de voz antes de o driver subir** (E3): o primeiro olhar da
        // porta da voz já o encontra. Uma conexão que não chega a abrir sai do
        // foco quando o `Shared` cai, pelo `Drop` do lugar.
        if let Some(foco) = foco {
            let id = foco.inscrever(Arc::new(foco::NaConexao {
                shared: Arc::downgrade(&shared),
                comandos: command_tx.clone(),
            }));
            shared.voice.entrar_no_foco(foco::NoFoco { foco, id });
        }
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (encerrada_tx, encerrada_rx) = std::sync::mpsc::channel();

        let thread_shared = Arc::clone(&shared);
        let thread_config = config.clone();
        subir_a_thread_da_conexao(Arc::clone(&shared), ready_tx, move |ready_tx| {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    tracing::error!(%error, "could not start the connection runtime");
                    let _ = ready_tx.send(Err(ConnectionError::Unreachable.into()));
                    return;
                }
            };
            runtime.block_on(drive(
                thread_config,
                alvo,
                key,
                pins,
                thread_shared,
                command_rx,
                ready_tx,
                olhos,
                cancelamento,
            ));
            // **Antes de o runtime cair, e não depois.** `drive` voltou com
            // a despedida do enlace feita (`Enlace::sair`), e é isso que
            // `encerrar_e_esperar` espera. Soltar o runtime espera a piscina
            // de bloqueio sem prazo — a bomba da tela é parada ali —, e um
            // sinal depois dele seguraria a saída do app por isso.
            let _ = encerrada_tx.send(());
            drop(runtime);
        })
        .map_err(|error| {
            tracing::error!(%error, "could not start the connection thread");
            ConnectionError::Unreachable
        })?;

        // The thread reports the outcome of the handshake, then keeps running.
        let trust = ready_rx
            .recv()
            .map_err(|_| ConnectionError::Unreachable)??;

        // **Só de pé ela pode ganhar a voz sozinha** (E3): a primeira conexão
        // da vida do foco, e a única. Uma que falhou antes daqui nunca foi dona.
        shared.voice.tomar_o_foco_se_primeira();

        let connection = Arc::new(Self {
            commands: command_tx,
            shared,
            encerrada: Mutex::new(Some(encerrada_rx)),
            ligacao,
        });
        Ok((connection, trust))
    }

    /// Puts the connection into a voice room.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn enter_voice_room(
        &self,
        voice_room: u32,
        password: Option<String>,
    ) -> Result<(), ConnectionError> {
        // **Uma senha maior que o teto do fio é recusada aqui, antes de pedir**
        // (R1-M4 da revisão do lote P61). O teto é em bytes, e o campo da
        // janela conta caracteres. Sem isto, o pedido ia ao núcleo, que o
        // recusa sem escrever o quadro; mas o `Room` já tinha sentado a pessoa
        // na sala pedida, e não tinha como saber qual das entradas no ar
        // desfazer. Aqui a pessoa não é sentada, e lê a mesma recusa que o
        // servidor daria a uma senha errada — que nenhuma senha deste tamanho
        // deixa de ser.
        if password
            .as_ref()
            .is_some_and(|senha| senha.len() > seele_core::MAX_NICKNAME_LEN)
        {
            tracing::warn!(
                sala = voice_room,
                "a senha passa do teto do fio, em bytes: a entrada não sai, e é recusada aqui"
            );
            let recusa = seele_core::Notice {
                severity: seele_core::AlertSeverity::Warning,
                reason: seele_core::AlertReason::VoiceRoomEntryRefused,
                operator_text: None,
            };
            let notice = aviso_de(&recusa);
            if let Ok(mut room) = self.shared.room.lock() {
                room.notice = Some(recusa);
            }
            self.shared.notify(&Event::NoticeRaised { notice });
            return Ok(());
        }
        self.command(Command::EnterVoiceRoom(VoiceRoomId(voice_room), password))
    }

    /// Takes the connection out.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn leave_voice_room(&self) -> Result<(), ConnectionError> {
        self.command(Command::LeaveVoiceRoom)
    }

    /// Opens a channel and asks for the page of history behind it.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn open_channel(&self, channel: u32) -> Result<(), ConnectionError> {
        self.command(Command::OpenChannel(ChannelId(channel)))
    }

    /// Sends a file, on a stream of its own. ADR 0027.
    ///
    /// Asks, and does not wait: the bar comes back as
    /// [`Event::TransferChanged`], and the message appears on the Channel only
    /// once the bytes have arrived whole. While it is going up, the only person
    /// who can see it is the sender — the cost ADR 0027 takes on purpose, so
    /// that "not arrived yet" and "expired" are never two similar absences on
    /// the same screen.
    ///
    /// `declared_type` is a **claim**, passed through as one.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn send_attachment(
        &self,
        channel: u32,
        body: String,
        path: String,
        file_name: String,
        declared_type: String,
    ) -> Result<u64, ConnectionError> {
        // The key is taken here rather than on the queue, unlike `Send`: the
        // shell needs it **now**, to hang a bar on. Nothing else about a
        // message has ever had to be known before it was sent.
        let id = next_client_message_id();
        self.command(Command::Attach(Box::new(seele_core::enlace::Anexo {
            linha: ChannelId(channel),
            id: ClientMessageId(id),
            corpo: body,
            // Um anexo em resposta fica para quando a casca precisar: o
            // núcleo já o leva (`Anexo::responde_a`).
            responde_a: None,
            caminho: std::path::PathBuf::from(path),
            nome: file_name,
            tipo: declared_type,
        })))?;
        Ok(id)
    }

    /// Salva um anexo em `folder`, com o nome que veio com ele — ou ao lado.
    ///
    /// **O nome não é argumento.** Ele é lido do histórico local desta sessão,
    /// como o tipo alegado da prévia, e nunca vem da janela: uma página que o
    /// devolvesse poderia devolver outro. Conferido em
    /// `seele_core::anexo_no_disco::nome_seguro`, um nome que não é só um nome
    /// é recusado; e a gravação nunca substitui o que já está na pasta — se o
    /// nome estiver tomado, o arquivo vai para «foto (2).png». A regra inteira
    /// mora em `seele_core::anexo_no_disco`.
    ///
    /// Pede, e não espera: o resultado volta como [`Event::TransferChanged`],
    /// com [`Transfer::Saved`] e o caminho real, ou [`Transfer::NotSaved`] e o
    /// motivo. O arquivo é marcado com a quarentena do próprio sistema ao ser
    /// gravado — o que não é antivírus, e este produto não tem um.
    ///
    /// `folder` é escolhida por quem chama, e tem de ser um caminho absoluto: a
    /// casca do desktop passa a pasta de downloads, que ela mesma calcula.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn save_attachment(&self, attachment: u64, folder: String) -> Result<(), ConnectionError> {
        self.command(Command::SaveAttachment {
            attachment: seele_core::AttachmentId(attachment),
            folder,
        })
    }

    /// Onde [`Self::save_attachment`] gravaria este anexo, ou por que não
    /// gravaria.
    ///
    /// Só para a frase de confirmação: a mesma regra, lida agora, para que a
    /// pessoa veja o nome e a pasta antes de apertar. Na hora de gravar, tudo é
    /// derivado de novo.
    ///
    /// # Errors
    ///
    /// [`SaveRefused`], com o motivo, o nome que veio e a pasta.
    pub fn save_destination(
        &self,
        attachment: u64,
        folder: String,
    ) -> Result<SaveDestination, SaveRefused> {
        let (_, nome) = destino_de(&self.shared, seele_core::AttachmentId(attachment), &folder)?;
        Ok(SaveDestination {
            beside: seele_core::anexo_no_disco::nome_ao_lado(&nome, 2),
            file_name: nome,
            folder,
        })
    }

    /// Fetches a small attachment and says whether a window may draw it.
    ///
    /// **On a press, never on a scroll**, and that is a decision this call
    /// leaves no room to get wrong: it downloads. The file lives on the server,
    /// so looking at it costs the host's uplink, and a channel that previewed
    /// everything as it scrolled would turn a 1 GiB disk ceiling into a 1 GiB
    /// transfer every time somebody opened it.
    ///
    /// Nothing is written to disk on the way. Saving is a separate act with a
    /// separate confirmation, and this is not it.
    ///
    /// The claim the sender made is read from this client's own history rather
    /// than taken as an argument. The fewer hands a string somebody else chose
    /// passes through before it is compared to the bytes, the better.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over, and equally if it
    /// ends while the fetch is in flight.
    pub async fn preview_attachment(&self, attachment: u64) -> Result<Preview, ConnectionError> {
        let (answer, caixa) = tokio::sync::oneshot::channel();
        self.command(Command::PreviewAttachment {
            attachment: seele_core::AttachmentId(attachment),
            answer,
        })?;
        caixa.await.map_err(|_| ConnectionError::NotConnected)
    }

    /// What a screen needs to decide whether to offer a preview at all.
    ///
    /// Convenience, exactly like [`SessionInfo::permissions`] is: a window that
    /// asks anyway gets [`PreviewRefusal::TooBig`] or
    /// [`PreviewRefusal::NotAPicture`], so this saves a round trip and enforces
    /// nothing.
    ///
    /// The four types come from here rather than being written out again in a
    /// screen, because a second copy of the list is a second copy that can
    /// disagree — and it would disagree by offering to draw something the fetch
    /// then refuses.
    ///
    /// [`SessionInfo::permissions`]: seele_core::SessionInfo::permissions
    #[must_use]
    pub fn preview_rules() -> PreviewRules {
        PreviewRules {
            limit: seele_core::PREVIEW_LIMIT,
            types: seele_core::ImageFormat::ALL
                .iter()
                .map(|format| format.media_type().to_owned())
                .collect(),
        }
    }

    /// O maior corpo que o servidor aceita, **em bytes**.
    ///
    /// # Por que a casca precisa deste número
    ///
    /// Porque ela tinha o próprio, e o dela contava outra coisa. O campo da
    /// janela aceitava 4.000 **unidades de texto** e o contrato limita 4.096
    /// **bytes**: 3.000 letras `á` caem dentro do primeiro e ocupam 6.000 do
    /// segundo. E a recusa não era uma recusa — o quadro falhava ao ser
    /// codificado, `dizer` devolvia erro, e o laço da sessão tratava isso como
    /// «o enlace morreu» e **encerrava a sessão**. Um acento a mais derrubava a
    /// conexão sem uma palavra na tela.
    #[must_use]
    pub const fn limite_da_mensagem() -> usize {
        seele_core::MAX_BODY_LEN
    }

    /// Os tetos dos campos que a casca escreve: o apelido, o nome de um canal
    /// ou de uma sala, os lugares de uma sala nova, o nome do servidor e o
    /// motivo de um banimento. Ver [`LimitesDosCampos`].
    ///
    /// O molde é [`Self::limite_da_mensagem`]: a casca lê daqui, e nenhum
    /// número do fio fica escrito à mão na janela.
    #[must_use]
    pub const fn limites_dos_campos() -> LimitesDosCampos {
        LimitesDosCampos {
            apelido: seele_core::MAX_NICKNAME_LEN,
            nome_de_canal: seele_core::MAX_CHANNEL_NAME_LEN,
            lugares_da_sala: seele_core::MAX_VOICE_ROOM_LIMIT,
            nome_do_servidor: seele_core::MAX_CLIENT_NAME_LEN,
            motivo_do_banimento: seele_core::MAX_ALERT_TEXT_LEN,
        }
    }

    /// Says something in o canal.
    ///
    /// # O que mudou, e é a metade que faltava
    ///
    /// A chave da mensagem é escolhida **aqui**, antes de qualquer coisa sair, e
    /// a mensagem é anotada como pendente antes de ser entregue ao enlace. Com
    /// isso: o texto existe em algum lugar que não é o campo da janela, a casca
    /// pode desenhá-lo como «enviando», a confirmação o encontra pela chave, e
    /// tentar de novo usa a **mesma** chave — que é o que faz o servidor
    /// reconhecer a repetição em vez de publicar duas vezes.
    ///
    /// Devolve a chave, para quem quiser acompanhar aquela mensagem.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn send_message(&self, channel: u32, body: String) -> Result<String, ConnectionError> {
        self.enviar(channel, body, None)
    }

    /// Diz alguma coisa **em resposta a uma mensagem** do mesmo canal.
    ///
    /// Tudo o que [`Self::send_message`] faz, mais o `replies_to`: a pendente
    /// já nasce com ele ([`Message::replies_to`]), e tentar de novo manda a
    /// mesma resposta. O servidor confere que a mensagem respondida existe e é
    /// deste canal; a que não é volta como a pendente `Recusada`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over, ou
    /// [`ConnectionError::UnknownMessage`] quando `replies_to` é zero: uma
    /// pendente não tem id de servidor, e não há como responder ao que o
    /// servidor ainda não tem.
    pub fn send_reply(
        &self,
        channel: u32,
        body: String,
        replies_to: u64,
    ) -> Result<String, ConnectionError> {
        if replies_to == 0 {
            return Err(ConnectionError::UnknownMessage);
        }
        self.enviar(channel, body, Some(MessageId(replies_to)))
    }

    /// O envio, com ou sem resposta: o corpo de [`Self::send_message`].
    fn enviar(
        &self,
        channel: u32,
        body: String,
        replies_to: Option<MessageId>,
    ) -> Result<String, ConnectionError> {
        let corpo = body.trim().to_owned();
        if corpo.is_empty() {
            return Ok(String::new());
        }
        let id = ClientMessageId(next_client_message_id());
        let channel = ChannelId(channel);
        self.anotar_pendente(channel, id, corpo.clone(), replies_to);

        // **O teto é conferido aqui**, e não descoberto quando o quadro se
        // recusa a ser codificado. Ver [`Self::limite_da_mensagem`]: o caminho
        // antigo encerrava a sessão.
        //
        // Pela mesma conferência que a repetição da volta usa
        // (`volta::grande_demais`): uma pendente grande demais que chegasse lá
        // derrubaria o enlace a cada volta, para sempre.
        if let Some(recusa) = volta::grande_demais(&corpo) {
            self.recusar_pendente(id, recusa);
            return Ok(id.get().to_string());
        }

        self.command(Command::Send {
            channel,
            body: corpo,
            replies_to,
            id,
        })?;
        Ok(id.get().to_string())
    }

    /// Manda de novo uma mensagem que não foi gravada, com a **mesma** chave.
    ///
    /// A mesma chave é o ponto: o servidor é idempotente por ela, então repetir
    /// uma mensagem que na verdade havia sido gravada devolve a original em vez
    /// de publicar uma segunda. É por isso que tentar de novo é seguro mesmo
    /// depois de uma queda — o estado [`EstadoDoEnvio::SemResposta`] existe para
    /// dizer exatamente esse caso.
    ///
    /// `false` quando não há pendente com essa chave: ela já foi confirmada ou
    /// descartada, e mandar de novo seria publicar de novo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn reenviar_mensagem(&self, client_message_id: &str) -> Result<bool, ConnectionError> {
        let Ok(chave) = client_message_id.parse::<u64>() else {
            // Uma chave que não é número não é de nenhuma pendente. `false` é a
            // mesma resposta de «essa já foi confirmada», e é a honesta.
            return Ok(false);
        };
        let id = ClientMessageId(chave);
        let Some((channel, corpo, replies_to)) = self.shared.room.lock().ok().and_then(|room| {
            room.pendentes
                .iter()
                .find(|pendente| pendente.client_message_id == id)
                .map(|pendente| (pendente.channel, pendente.body.clone(), pendente.replies_to))
        }) else {
            return Ok(false);
        };
        // **A mesma conferência do envio** (`volta::grande_demais`), e pelo
        // mesmo motivo: um corpo acima do teto não é recusado pelo servidor, o
        // quadro se recusa a ser codificado e o núcleo derruba o enlace.
        // Medido na fase A do lote P61: TENTAR DE NOVO numa `Recusada(TooLong)`
        // derrubava a conexão, e a pendente voltava como `SemResposta`, com a
        // tela oferecendo como seguro um reenvio que derruba de novo.
        if let Some(recusa) = volta::grande_demais(&corpo) {
            self.recusar_pendente(id, recusa);
            return Ok(true);
        }
        // A resposta anotada no primeiro envio vai junto: tentar de novo manda
        // a mesma mensagem, e não uma solta.
        self.anotar_pendente(channel, id, corpo.clone(), replies_to);
        self.command(Command::Send {
            channel,
            body: corpo,
            replies_to,
            id,
        })?;
        Ok(true)
    }

    /// Desiste de uma mensagem que não foi gravada, e devolve o texto dela.
    ///
    /// Devolve o corpo para que a casca possa pô-lo de volta no compositor: uma
    /// desistência que apagasse o texto seria o R05 de novo, por outra porta.
    #[must_use]
    pub fn descartar_mensagem(&self, client_message_id: &str) -> Option<String> {
        let chave = client_message_id.parse::<u64>().ok()?;
        let corpo = self
            .shared
            .room
            .lock()
            .ok()?
            .descartar_envio(ClientMessageId(chave));
        if corpo.is_some() {
            self.shared.messages_changed();
        }
        corpo
    }

    /// Anota uma pendente e acorda quem desenha.
    fn anotar_pendente(
        &self,
        channel: ChannelId,
        id: ClientMessageId,
        body: String,
        replies_to: Option<MessageId>,
    ) {
        if let Ok(mut room) = self.shared.room.lock() {
            room.anotar_resposta(channel, id, body, replies_to, agora_em_segundos());
        }
        self.shared.messages_changed();
    }

    /// Marca uma pendente como recusada e acorda quem desenha.
    fn recusar_pendente(&self, id: ClientMessageId, motivo: seele_core::MessageRefusal) {
        let mexeu = self
            .shared
            .room
            .lock()
            .map(|mut room| room.recusar_envio(id, motivo))
            .unwrap_or(false);
        if mexeu {
            self.shared.messages_changed();
        }
    }

    /// Requests an enabled MOD action. Responses arrive as `Event::ModReply`.
    /// Returns an error when disconnected.
    pub fn mod_request(
        &self,
        request: u32,
        id: String,
        channel: u32,
        payload: String,
    ) -> Result<(), ConnectionError> {
        self.command(Command::ModRequest {
            request,
            id,
            channel: ChannelId(channel),
            payload,
        })
    }

    /// Pede uma **ação declarada** de MOD (API 7): o [`Self::mod_request`] com
    /// o nome da ação que o `mod.json` declarou em `permissoes`.
    ///
    /// Este lado não decide nada: quem confere o MOD ligado, a ação declarada
    /// e a permissão da pessoa é o servidor, antes de chamar a metade de
    /// servidor. A resposta — inclusive a recusa `{"ok":false,"recusa":
    /// "acao-nao-declarada" | "sem-permissao"}` — chega como
    /// [`Event::ModReply`] deste `request`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError`] quando a sessão já acabou.
    pub fn acao_de_mod(
        &self,
        request: u32,
        id: String,
        channel: u32,
        acao: String,
        payload: String,
    ) -> Result<(), ConnectionError> {
        self.command(Command::AcaoDeMod {
            request,
            id,
            channel: ChannelId(channel),
            acao,
            payload,
        })
    }

    /// Quem joga entra num túnel de jogo e espera a porta em `127.0.0.1`.
    ///
    /// A casca só chama isto de um clique (o bilhete de gesto é dela). Quem
    /// confere o MOD ligado, o poder `porta` e a ação `entrar-no-jogo` é o
    /// servidor, a cada entrada.
    ///
    /// # Errors
    ///
    /// O nome em kebab-case da recusa (`RecusaDeTunel`: `sem-permissao`,
    /// `porta-local-ocupada`, `servidor-antigo`...), ou `sessao-encerrada`
    /// quando a sessão já acabou. Nunca texto livre: a frase é da casca.
    pub async fn entrar_no_tunel(
        &self,
        mod_id: String,
        nome: String,
    ) -> Result<PortaDeTunel, &'static str> {
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.command(Command::EntrarNoTunel {
            mod_id,
            nome,
            resposta,
        })
        .map_err(|_| "sessao-encerrada")?;
        // A sessão que acaba no meio larga o remetente: é o mesmo desfecho.
        caixa.await.map_err(|_| "sessao-encerrada")?
    }

    /// Quem joga deixa o túnel `nome` do MOD: a porta fecha e os fluxos acabam.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn sair_do_tunel(&self, mod_id: String, nome: String) -> Result<(), ConnectionError> {
        self.command(Command::SairDoTunel { mod_id, nome })
    }

    /// Quem joga deixa todos os túneis do MOD: ele foi desligado, descarregado
    /// ou a janela dele acabou.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn sair_dos_tuneis_do_mod(&self, mod_id: String) -> Result<(), ConnectionError> {
        // Tirados daqui já: a faixa que lê logo depois não vê o que está
        // fechando, e o relatório final que chegar volta a pô-los só com
        // `encerrado`.
        if let Ok(mut tuneis) = self.shared.tuneis.lock() {
            tuneis
                .abertos
                .retain(|(mod_do_tunel, _), _| *mod_do_tunel != mod_id);
        }
        self.command(Command::SairDosTuneisDoMod { mod_id })
    }

    /// Os túneis de jogo abertos agora, com o último relatório de cada um.
    #[must_use]
    pub fn tuneis_abertos(&self) -> Vec<RelatorioDeTunel> {
        self.shared
            .tuneis
            .lock()
            .map(|tuneis| {
                tuneis
                    .abertos
                    .values()
                    .filter(|relatorio| relatorio.encerrado.is_none())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    // --------------------------------------------- a máquina de jogo (API 8)

    /// Se esta conexão mostra pedidos de máquina ([`Event::MachineRequest`]).
    /// Sem isso — e sem [`este_processo_tem_tela`] —, todo pedido é recusado
    /// no núcleo com `sem-tela` e só o log fica sabendo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn aceitar_pedidos_de_maquina(&self, sim: bool) -> Result<(), ConnectionError> {
        self.command(Command::AceitarPedidosDeMaquina(sim))
    }

    /// A pessoa aceitou a oferta de `resumo`: o do [`Event::MachineRequest`],
    /// como veio.
    ///
    /// # Errors
    ///
    /// `sessao-encerrada` quando a sessão já acabou.
    pub fn aceitar_maquina(
        &self,
        mod_id: String,
        servidor: String,
        resumo: String,
    ) -> Result<(), &'static str> {
        self.command(Command::VerboDaMaquina(VerboDaMaquina::Aceitar {
            mod_id,
            servidor,
            resumo,
        }))
        .map_err(|_| "sessao-encerrada")
    }

    /// A máquina não vai rodar; `motivo` em kebab-case (`pelo-dono`,
    /// `pacote-diferente`, `oferta-invalida`, ...).
    ///
    /// # Errors
    ///
    /// `motivo-desconhecido` para um nome que não existe; `sessao-encerrada`.
    pub fn recusar_maquina(
        &self,
        mod_id: String,
        servidor: String,
        motivo: &str,
    ) -> Result<(), &'static str> {
        let motivo = types::recusa_de_maquina_de(motivo).ok_or("motivo-desconhecido")?;
        self.command(Command::VerboDaMaquina(VerboDaMaquina::Recusar {
            mod_id,
            servidor,
            motivo,
        }))
        .map_err(|_| "sessao-encerrada")
    }

    /// A máquina parou; `motivo` em kebab-case (`pelo-dono`, `revogada`,
    /// `app-fechou`, `metade-falhou`).
    ///
    /// # Errors
    ///
    /// `motivo-desconhecido` para um nome que não existe; `sessao-encerrada`.
    pub fn parar_maquina(
        &self,
        mod_id: String,
        servidor: String,
        motivo: &str,
    ) -> Result<(), &'static str> {
        let motivo = types::parada_de_maquina_de(motivo).ok_or("motivo-desconhecido")?;
        self.command(Command::VerboDaMaquina(VerboDaMaquina::Parar {
            mod_id,
            servidor,
            motivo,
        }))
        .map_err(|_| "sessao-encerrada")
    }

    /// Um pedido da máquina à metade de servidor do MOD, e a resposta dela.
    ///
    /// # Errors
    ///
    /// `sem-aceite` (esta conexão não é a máquina daquele servidor de jogo),
    /// `sem-resposta` (prazo de 15 s, queda, ou o pedido não saiu — o porquê
    /// vai ao log), `sessao-encerrada`.
    pub async fn pedido_da_maquina(
        &self,
        mod_id: String,
        servidor: String,
        payload: String,
    ) -> Result<String, &'static str> {
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.command(Command::PedidoDaMaquina {
            mod_id,
            servidor,
            payload,
            resposta,
        })
        .map_err(|_| "sessao-encerrada")?;
        caixa.await.map_err(|_| "sessao-encerrada")?
    }

    /// A máquina passa a servir `porta` (a do jogo, em `127.0.0.1`) como o
    /// túnel `mod_id/nome`; `protocolo` é `tcp` ou `udp`. Quem confere o
    /// aceite é o servidor (`fora-do-aceite`).
    ///
    /// # Errors
    ///
    /// O nome em kebab-case da recusa (`RecusaDeTunel`), `protocolo-desconhecido`,
    /// ou `sessao-encerrada`.
    pub async fn servir_tunel(
        &self,
        mod_id: String,
        nome: String,
        protocolo: &str,
        porta: u16,
    ) -> Result<(), &'static str> {
        let protocolo = types::protocolo_de(protocolo).ok_or("protocolo-desconhecido")?;
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.command(Command::ServirTunel {
            mod_id,
            nome,
            protocolo,
            porta,
            resposta,
        })
        .map_err(|_| "sessao-encerrada")?;
        caixa.await.map_err(|_| "sessao-encerrada")?
    }

    /// A máquina para de servir o túnel `nome` do MOD.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn parar_tunel(&self, mod_id: String, nome: String) -> Result<(), ConnectionError> {
        self.command(Command::PararTunel { mod_id, nome })
    }

    /// Por que as conexões de um túnel que esta máquina serve acabaram,
    /// contadas por motivo (é aqui que aparece `jogo-recusou`). Vazio para um
    /// túnel que ela não serve.
    ///
    /// # Errors
    ///
    /// `sessao-encerrada` quando a sessão já acabou.
    pub async fn quedas_do_tunel_servido(
        &self,
        mod_id: String,
        nome: String,
    ) -> Result<Vec<QuedaDeTunel>, &'static str> {
        let (resposta, caixa) = tokio::sync::oneshot::channel();
        self.command(Command::QuedasDoTunelServido {
            mod_id,
            nome,
            resposta,
        })
        .map_err(|_| "sessao-encerrada")?;
        caixa.await.map_err(|_| "sessao-encerrada")
    }

    /// Asks the server to make a voice room.
    ///
    /// Asks. It does not decide, and it does not report back whether it worked
    /// — because it cannot: the answer comes from the server, arrives on the
    /// event stream, and reaches the shell as [`Event::ChannelsChanged`] when
    /// the room exists or [`Event::NoticeRaised`] carrying
    /// [`NoticeReason::PermissionDenied`] when it does not. Returning a result
    /// here would mean this method waiting on a round trip, which is the one
    /// thing every other command on this object promises not to do.
    ///
    /// `channel` binds a text channel to the room; `None` leaves it a voice room
    /// only, which `specs/04-servidor-seele.md` allows.
    ///
    /// The empty-name case returns `Ok` without sending anything, the same way
    /// [`Connection::send_message`] swallows an empty body: a person who pressed the
    /// button with nothing typed has not asked for anything, and answering that
    /// with an error would put a red message on a screen where nothing went
    /// wrong.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn create_voice_room(
        &self,
        name: String,
        limit: u16,
        channel: Option<u32>,
    ) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::CreateVoiceRoom {
            name,
            limit,
            channel: channel.map(ChannelId),
        })
    }

    /// Asks the server to make o canal.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn create_channel(&self, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::CreateChannel { name })
    }

    /// Asks the server to rename a voice room.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn rename_voice_room(&self, voice_room: u32, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameVoiceRoom {
            voice_room: VoiceRoomId(voice_room),
            name,
        })
    }

    /// Asks the server to rename o canal.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn rename_channel(&self, channel: u32, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameChannel {
            channel: ChannelId(channel),
            name,
        })
    }

    /// Asks the server to rename itself.
    ///
    /// Asks, and reports nothing back, like [`Connection::rename_voice_room`]: the answer
    /// comes from the server, as [`Event::ServerChanged`] with the new name on
    /// the next [`Snapshot`], or as [`Event::NoticeRaised`] carrying
    /// [`NoticeReason::PermissionDenied`].
    ///
    /// A blank name is swallowed here rather than sent, exactly as
    /// [`Connection::rename_voice_room`] swallows one: a shell with an empty box and a
    /// button is not a shell reporting an error, and the server would refuse it
    /// anyway.
    ///
    /// A shell may read [`Snapshot::may_customise_server`] to decide whether to
    /// draw the control. **Convenience, never enforcement** — pressing it
    /// without the permission gets a refusal and nothing changes.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn rename_server(&self, name: String) -> Result<(), ConnectionError> {
        if name.trim().is_empty() {
            return Ok(());
        }
        self.command(Command::RenameServer { name })
    }

    /// Asks the server to change its picture, or `None` to have none.
    ///
    /// PNG bytes, and the server refuses anything else — a real PNG, at most
    /// `seele_proto::control::MAX_SERVER_ICON_SIDE` a side, at most
    /// `seele_proto::control::MAX_SERVER_ICON_LEN` bytes. A shell hands over
    /// what the person picked; the picture that comes back to everybody is the
    /// proof it was taken.
    ///
    /// Checked here **before** the command is queued, with the protocol's own
    /// function rather than a copy of its rules. Not for politeness: a picture
    /// the protocol refuses cannot be turned into a frame, a frame that cannot
    /// be built is a send that fails, and a send that fails is how a dropped
    /// connection looks from inside `seele_core::enlace`. Somebody choosing a
    /// PDF would watch the app start its five-minute internal battery.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::IconNotAPicture`] or [`ConnectionError::IconTooBig`] when the
    /// bytes will not do; [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_server_icon(&self, icon: Option<Vec<u8>>) -> Result<(), ConnectionError> {
        seele_core::check_server_icon(icon.as_deref()).map_err(|recusa| match recusa {
            seele_core::IconRefusal::NotAnIcon => ConnectionError::IconNotAPicture,
            seele_core::IconRefusal::TooBig { limit_bytes } => {
                ConnectionError::IconTooBig { limit_bytes }
            }
        })?;
        self.command(Command::SetServerIcon { icon })
    }

    /// Põe ou tira **a sua** imagem de perfil.
    ///
    /// Os mesmos limites do ícone do servidor, e conferidos aqui antes de o
    /// comando sair: só PNG, 8 KiB, 256 px de lado. Conferir deste lado é o que
    /// permite a casca dizer o motivo — a recusa do servidor derruba a conexão
    /// por violação de protocolo, e uma figura grande demais não é um ataque,
    /// é um arquivo que alguém escolheu.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::IconNotAPicture`] se não for um PNG,
    /// [`ConnectionError::IconTooBig`] se passar do teto, e
    /// [`ConnectionError::NotConnected`] quando não há sessão.
    pub fn set_person_icon(&self, icon: Option<Vec<u8>>) -> Result<(), ConnectionError> {
        seele_core::check_server_icon(icon.as_deref()).map_err(|recusa| match recusa {
            seele_core::IconRefusal::NotAnIcon => ConnectionError::IconNotAPicture,
            seele_core::IconRefusal::TooBig { limit_bytes } => {
                ConnectionError::IconTooBig { limit_bytes }
            }
        })?;
        self.command(Command::SetPersonIcon { icon })
    }

    /// Troca **o seu** apelido.
    ///
    /// O histórico não muda: cada mensagem guarda o apelido de quem a escreveu
    /// no instante em que foi escrita, e é decisão de produto que continue
    /// mostrando aquele. Ver `ClientMessage::SetNickname`.
    ///
    /// Um nome em branco é desistir da edição, e não um pedido: devolve `Ok`
    /// sem mandar nada, como `set_person_icon` faz com o seletor fechado.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] quando não há sessão. Um nome já
    /// tomado por outra conta volta como alerta do servidor, e não daqui: quem
    /// sabe quais nomes existem é ele.
    pub fn set_nickname(&self, name: String) -> Result<(), ConnectionError> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Ok(());
        }
        self.command(Command::SetNickname { name })
    }

    /// A imagem de perfil de alguém, ou `None` quando não tem.
    ///
    /// Fora do [`Snapshot`] de propósito, e pela razão que
    /// [`Connection::server_icon`] escreve: o snapshot atravessa a ponte duas
    /// vezes por segundo, e uma figura de 8 KiB **por pessoa** ali dentro seria
    /// megabytes por minuto de uma coisa que muda quase nunca. A casca pede a
    /// imagem quando a lista muda, e não a cada tique.
    #[must_use]
    pub fn person_icon(&self, person: u64) -> Option<Vec<u8>> {
        self.shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.person_icons.get(&PersonId(person)).cloned())
    }

    /// The server's picture, if it has one.
    ///
    /// Separate from [`Connection::snapshot`] for the reason [`Connection::messages`] is
    /// separate from it: the two change at completely different rates, and
    /// carrying the bytes on every frame of a redraw would mean cloning them
    /// and serialising them twice a second for a value that moves when
    /// somebody presses a button. Ask for this when
    /// [`Snapshot::icon_revision`] changes, and not otherwise.
    #[must_use]
    pub fn server_icon(&self) -> Option<Vec<u8>> {
        self.shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.icon.clone())
    }

    /// Esquece o aviso que está na tela. Ver `seele_core::Room::dispensar_aviso`.
    ///
    /// Síncrono e sem passar pelo servidor: o aviso é desta ponta, e nada do outro
    /// lado precisa saber que alguém o leu.
    pub fn dispensar_aviso(&self) {
        let dispensou = self
            .shared
            .room
            .lock()
            .is_ok_and(|mut room| room.dispensar_aviso());
        // Só quando havia: um evento por clique num botão que não fez nada é a
        // casca redesenhando por educação.
        if dispensou {
            self.shared.notify(&Event::TelemetryChanged);
        }
    }

    /// Asks the server to end a person's session — `expulsar`.
    ///
    /// Asks, and reports nothing back, for the same reason [`Connection::create_voice_room`]
    /// gives: the answer comes from the server. The roster losing them arrives as
    /// [`Event::RosterChanged`]; a refusal arrives as [`Event::NoticeRaised`]
    /// carrying [`NoticeReason::PermissionDenied`].
    ///
    /// A shell may read [`Snapshot::may_kick`] to decide whether to draw the
    /// control. That is **convenience, never enforcement** — pressing it
    /// without the permission removes nobody.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn kick_person(&self, person: u64) -> Result<(), ConnectionError> {
        self.command(Command::KickPerson {
            person: PersonId(person),
        })
    }

    // ---- cargos (ADR 0064; os tipos e a regra são de `cargos.rs`, de F) ----

    /// Pede ao servidor o quadro de cargos desta sessão. A resposta chega
    /// pelo `Room` (`cargos_na_tela`), como todo estado.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] depois do fim da sessão.
    pub fn pedir_cargos(&self) -> Result<(), ConnectionError> {
        self.command(Command::Cargo(None))
    }

    /// Pede um verbo de cargo com a revisão que a tela viu. **Quem decide é o
    /// servidor**: a recusa volta como `CargoRecusado`, nomeada.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] depois do fim da sessão.
    pub fn pedido_de_cargo(
        &self,
        revisao: u64,
        pedido: cargos::PedidoDeCargo,
    ) -> Result<(), ConnectionError> {
        self.command(Command::Cargo(Some((revisao, pedido))))
    }

    /// Os apelidos que esta sessão conhece: os membros do FORA DO AR e quem
    /// está no roster.
    fn apelidos_conhecidos(room: &Room) -> std::collections::BTreeMap<PersonId, String> {
        let mut apelidos = room.membros.clone();
        for (id, pessoa) in &room.people {
            apelidos.insert(*id, pessoa.nickname.clone());
        }
        apelidos
    }

    /// O que a tela de cargos lê desta sessão: o quadro, as suas permissões e
    /// os apelidos de quem aparece no quadro.
    #[must_use]
    pub fn cargos_na_tela(&self) -> cargos::CargosNaTela {
        match self.shared.room.lock() {
            Ok(room) => room.cargos.na_tela(&Self::apelidos_conhecidos(&room)),
            Err(_) => cargos::CargosDaSessao::default().na_tela(&std::collections::BTreeMap::new()),
        }
    }

    /// A prévia de um verbo desta pessoa com a revisão `revisao`: a recusa que
    /// o servidor daria, ou o ganho e a perda por pessoa. `None` antes do
    /// primeiro quadro, ou sem saber quem é esta conexão. Só informativa.
    #[must_use]
    pub fn prever_cargo(
        &self,
        revisao: u64,
        pedido: &cargos::PedidoDeCargo,
    ) -> Option<Result<cargos::Previa, cargos::RecusaDeCargo>> {
        let room = self.shared.room.lock().ok()?;
        let eu = room.me?;
        room.cargos
            .prever(&Self::apelidos_conhecidos(&room), eu, revisao, pedido)
    }

    /// Por que `pessoa` tem, ou não, cada permissão, pelo último quadro.
    #[must_use]
    pub fn explicar_permissoes(&self, pessoa: u64) -> Option<Vec<cargos::PermissaoExplicada>> {
        let room = self.shared.room.lock().ok()?;
        room.cargos
            .explicar_tudo(&Self::apelidos_conhecidos(&room), PersonId(pessoa))
    }

    /// Asks the server to bar a person from returning — `banir`.
    ///
    /// `expires_at` is seconds since the Unix epoch; `None` is permanent. The
    /// `reason` is for whoever hosts, in their own records, and never reaches
    /// the person barred.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn ban_person(
        &self,
        person: u64,
        reason: Option<String>,
        expires_at: Option<i64>,
    ) -> Result<(), ConnectionError> {
        self.command(Command::BanPerson {
            person: PersonId(person),
            reason,
            expires_at,
        })
    }

    /// Asks the server to take a message off its Channel — `remover_mensagem`.
    ///
    /// It goes away for everybody, this client included, when the server says so.
    /// An author removing their own needs no permission, which is why a shell
    /// drawing this control on one's own message may draw it for anybody.
    ///
    /// # Uma pendente não tem o que remover
    ///
    /// Zero não é um identificador de mensagem: é o que
    /// [`Message::id`] vale numa mensagem que esta máquina escreveu e o servidor
    /// ainda não confirmou. Ela não existe lá, então não há o que tirar de canal
    /// nenhum — e mandar o pedido faria o servidor responder «não há tal
    /// mensagem», que é verdade e é a frase errada para quem clicou num texto que
    /// está na tela.
    ///
    /// Quem quer largar uma pendente usa [`Self::descartar_mensagem`], que
    /// endereça pela chave desta máquina.
    ///
    /// **Este guarda nasceu de uma reprovação.** Sem ele,
    /// `conformance::moderacao` pegava `messages()[0].id` numa lista onde a
    /// primeira linha era uma pendente, pedia a remoção do zero, e recebia «não
    /// há tal mensagem» no lugar da recusa de permissão que o teste mede — o
    /// primeiro caller a pisar nisto foi um teste, e o próximo seria uma tela.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over, or
    /// [`ConnectionError::UnknownMessage`] for a message the server never saw.
    pub fn remove_message(&self, message: u64) -> Result<(), ConnectionError> {
        if message == 0 {
            return Err(ConnectionError::UnknownMessage);
        }
        self.command(Command::RemoveMessage {
            message: MessageId(message),
        })
    }

    /// **O portão de versão**: um verbo só entra na fila para um servidor que
    /// o entende. A tabela é a do protocolo (`desde_do_cliente`), e esta
    /// função não repete número nenhum.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] antes do aperto de mão (o número
    /// negociado ainda é zero), e [`ConnectionError::ServidorAntigo`] quando o
    /// servidor negociou menos do que o verbo exige.
    fn portao_de_versao(
        &self,
        mensagem: &seele_core::ClientMessage,
    ) -> Result<(), ConnectionError> {
        let negociado = self.protocolo_negociado();
        if negociado == 0 {
            return Err(ConnectionError::NotConnected);
        }
        match seele_core::perfil::portao_do_cliente(mensagem, negociado) {
            Err(seele_core::perfil::ErroDoPerfil::ServidorAntigo { negociado, exige }) => {
                Err(ConnectionError::ServidorAntigo { negociado, exige })
            }
            _ => Ok(()),
        }
    }

    /// **Edita uma mensagem sua** — protocolo 13.
    ///
    /// Só a autora edita; o servidor recusa a de outra pessoa com o aviso de
    /// permissão negada, e a que já não existe com `PedidoRecusado`. A
    /// confirmação é a mensagem mudar: o corpo novo chega por
    /// [`Event::MessagesChanged`], com [`Message::edited`] ligado, para esta
    /// máquina e para quem mais lê o canal. Não há pendente: se o servidor
    /// não responder, o texto antigo continua na tela, que é a verdade.
    ///
    /// O corpo passa pelo mesmo `trim` do envio.
    ///
    /// # Errors
    ///
    /// - [`ConnectionError::UnknownMessage`]: `message` é zero (uma pendente;
    ///   ver [`Self::remove_message`]);
    /// - [`ConnectionError::MensagemVazia`]: não sobrou texto;
    /// - [`ConnectionError::MensagemGrandeDemais`]: passa de
    ///   [`Self::limite_da_mensagem`] bytes;
    /// - [`ConnectionError::ServidorAntigo`]: o servidor negociou menos que o
    ///   13, e nada foi escrito;
    /// - [`ConnectionError::NotConnected`] once the session is over.
    pub fn edit_message(&self, message: u64, body: String) -> Result<(), ConnectionError> {
        if message == 0 {
            return Err(ConnectionError::UnknownMessage);
        }
        let corpo = body.trim().to_owned();
        if corpo.is_empty() {
            return Err(ConnectionError::MensagemVazia);
        }
        if volta::grande_demais(&corpo).is_some() {
            return Err(ConnectionError::MensagemGrandeDemais {
                limite_bytes: seele_core::MAX_BODY_LEN as u64,
            });
        }
        let message = MessageId(message);
        self.portao_de_versao(&seele_core::ClientMessage::EditMessage {
            id: message,
            body: corpo.clone(),
        })?;
        self.command(Command::EditMessage {
            message,
            body: corpo,
        })
    }

    /// **Tira o banimento de uma conta** — protocolo 13, a mesma permissão e
    /// a mesma hierarquia de [`Self::ban_person`].
    ///
    /// A resposta é a lista de banidos como ficou ([`Event::BanidosChanged`],
    /// [`Self::banidos`]), e a pessoa volta ao FORA DO AR de todo mundo. A
    /// recusa chega como aviso: permissão negada, ou `PedidoRecusado` quando
    /// a conta não estava banida.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ServidorAntigo`] num servidor que negociou menos
    /// que o 13; [`ConnectionError::NotConnected`] once the session is over.
    pub fn unban_person(&self, person: u64) -> Result<(), ConnectionError> {
        let person = PersonId(person);
        self.portao_de_versao(&seele_core::ClientMessage::UnbanPerson { person })?;
        self.command(Command::UnbanPerson { person })
    }

    /// **Pede a lista de banidos** — protocolo 13. Só responde a quem pode
    /// banir; para os outros, volta o aviso de permissão negada.
    ///
    /// Pede, e não espera: a lista chega por [`Event::BanidosChanged`] e fica
    /// em [`Self::banidos`]. Ela **não se atualiza sozinha** quando outra
    /// pessoa bane alguém: quem abre a tela de banidos pede de novo.
    ///
    /// # Errors
    ///
    /// Os de [`Self::unban_person`].
    pub fn list_bans(&self) -> Result<(), ConnectionError> {
        self.portao_de_versao(&seele_core::ClientMessage::ListBans)?;
        self.command(Command::ListBans)
    }

    /// Os banimentos que valem, como o servidor os listou da última vez que
    /// esta sessão pediu ([`Self::list_bans`], [`Self::unban_person`]). Vazia
    /// até pedir, e depois de uma reconexão.
    ///
    /// O `nickname` é o que o servidor guarda (o alias, `ana#K7Q2`); a base
    /// que a tela mostra sai de [`Snapshot::identidades_apresentadas`], pela
    /// conta, como para um membro do FORA DO AR.
    #[must_use]
    pub fn banidos(&self) -> Vec<Banido> {
        self.shared
            .room
            .lock()
            .map(|room| seele_core::retrato::banidos_de(&room))
            .unwrap_or_default()
    }

    /// Asks the server to move a person into a voice room — `mover_pessoa`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn move_person(&self, person: u64, voice_room: u32) -> Result<(), ConnectionError> {
        self.command(Command::MovePerson {
            person: PersonId(person),
            voice_room: VoiceRoomId(voice_room),
        })
    }

    /// Asks the server to destroy a voice room — `apagar_voice_room`.
    ///
    /// Everybody inside is turned out of it and told; the Channel bound to it, if
    /// there is one, is left alone. The server refuses the last voice room, and says so
    /// with [`NoticeReason::LastVoiceRoom`] rather than with the sentence it uses for
    /// a refused entry.
    ///
    /// Asks, and nothing more. Nothing is removed from this client's own idea of
    /// the server until the server says the room is gone — a room removed
    /// optimistically would vanish off the screen of the person who asked
    /// whether or not it worked, and the refusal is the case they most need to
    /// see did not happen.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn delete_voice_room(&self, voice_room: u32) -> Result<(), ConnectionError> {
        self.command(Command::DeleteVoiceRoom {
            voice_room: VoiceRoomId(voice_room),
        })
    }

    /// Asks the server to destroy the channel, and everything written in it —
    /// `apagar_linha`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn delete_channel(&self, channel: u32) -> Result<(), ConnectionError> {
        self.command(Command::DeleteChannel {
            channel: ChannelId(channel),
        })
    }

    /// Asks what destroying the channel would cost, and waits for the answer.
    ///
    /// The one call on this handle that waits, and the reason is the sentence it
    /// feeds: a confirmation promising to destroy 1.847 messages by 6 people
    /// written since a certain day has to have counted them, in the server's own
    /// database, at the moment of asking. This client holds one page of history
    /// and would guess low by whatever the Channel's whole past is — and a number
    /// that is nearly right in that box is worse than no number at all.
    ///
    /// So the caller waits, and a shell that cannot get an answer must not open
    /// the box: there is no honest version of it without these three numbers.
    ///
    /// Destroys nothing, and needs no permission — the server answers about a
    /// Channel the asker may already read.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over, and equally if it
    /// ends while the question is in flight: the driver drops what it was going
    /// to answer with, and this returns rather than waiting for a server that is
    /// no longer there.
    pub async fn weigh_channel(&self, channel: u32) -> Result<ChannelWeight, ConnectionError> {
        let (answer, resposta) = tokio::sync::oneshot::channel();
        self.command(Command::WeighChannel {
            channel: ChannelId(channel),
            answer,
        })?;
        resposta
            .await
            .map_err(|_| ConnectionError::NotConnected)?
            .map_err(|causa| ConnectionError::PedidoRecusado { causa })
    }

    /// Mutes or unmutes the microphone — mudo.
    ///
    /// Announced to the server as well as applied locally: the roster shows it,
    /// and a mute nobody else can see is half a feature.
    ///
    /// Fora de sala não há voz para mutar, e o mudo é guardado: a voz abre muda
    /// quando a pessoa entrar numa sala. Pendência 52.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_muted(&self, on: bool) -> Result<(), ConnectionError> {
        self.shared.voice.escolher(|escolhida| escolhida.mudo = on);
        self.command(Command::SetMuted(on))
    }

    /// Mutes or unmutes the speakers — Isolamento total.
    ///
    /// Guardado fora de sala, como o mudo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] once the session is over.
    pub fn set_total_isolation(&self, on: bool) -> Result<(), ConnectionError> {
        self.shared
            .voice
            .escolher(|escolhida| escolhida.isolamento = on);
        self.command(Command::SetTotalIsolation(on))
    }

    /// Reports the push-to-talk key going down or coming up.
    ///
    /// Not a command: it has to take effect on the next 20 ms frame, and a
    /// round trip through the driver thread would put a queue between a key and
    /// a microphone.
    ///
    /// Sem voz viva — a volta reabrindo, a sala abrindo —, a tecla é guardada e
    /// vale na voz que entrar no lugar. Item 1 do lote V.
    pub fn set_talking(&self, talking: bool) {
        self.shared.voice.segurar_a_tecla(talking);
    }

    /// Switches this session to another microphone.
    ///
    /// `device` is a [`CaptureDevice::id`] from [`capture_devices`]; `None` goes
    /// back to the machine's default. The choice is remembered, so a
    /// reconnection reopens the same microphone rather than quietly falling back
    /// to the default one. The chosen sound output is left where it is.
    ///
    /// Takes effect **now**, not on the next voice room. That is worth the extra
    /// mechanism: somebody opens this screen because the microphone they are
    /// speaking into is the wrong one, and telling them to leave the server and
    /// come back is telling them to solve it themselves.
    ///
    /// Synchronous rather than queued, because the answer is the point: a device
    /// that has been unplugged since the list was drawn has to come back as an
    /// error the screen can put next to the row that was clicked.
    ///
    /// **Fora de sala, só grava.** Pendência 52: sem sala não há voz aberta para
    /// trocar, e a escolha vale quando a pessoa entrar numa. Recusar ali seria a
    /// tela dizendo que o microfone sumiu a quem só ainda não entrou.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::CaptureDeviceGone`] when the machine is not offering that
    /// device any more, in which case **nothing changed** and the previous
    /// microphone is still live. [`ConnectionError::NoAudioDevice`] when this session
    /// has no audio at all — a session joined with the audio box unticked has no
    /// voice path to move.
    pub fn set_capture_device(&self, device: Option<String>) -> Result<(), ConnectionError> {
        // The new path opens before the old one is dropped, so a microphone that
        // turns out to be gone leaves the session speaking instead of silent.
        // `switch_capture` is what carries mudo and the rest across — in
        // the core, so the terminal client gets the same list of what survives,
        // and the chosen output with it.
        let pedido = device.clone();
        self.switch_device(Lado::Entrada, pedido.clone(), |running, media, ssrc| {
            running
                .switch_capture(device.as_deref(), media, ssrc)
                .map_err(|error| {
                    tracing::warn!(%error, "could not open the chosen microphone");
                    ConnectionError::CaptureDeviceGone
                })
        })?;
        self.conferir_troca(pedido.as_deref(), Lado::Entrada)
    }

    /// Pede um quadro-chave a quem está compartilhando a tela.
    ///
    /// Chamado por quem **recebe**, quando descobre que há uma transmissão em
    /// curso: sem isto, quem entra numa sala que já compartilha recebe só
    /// diferenças de um quadro que nunca viu. Ver [`Command::RequestKeyFrame`].
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem sessão.
    pub fn pedir_quadro_chave(&self, tela: u32) -> Result<(), ConnectionError> {
        self.command(Command::RequestKeyFrame {
            tela: seele_core::ScreenId(tela),
        })
    }

    /// Passa a receber a imagem de uma transmissão, ou para de receber.
    ///
    /// **Assistir é um pedido.** Enquanto havia uma transmissão por sala, ela era
    /// empurrada para todo mundo; com várias, empurrar todas para todos faria cada
    /// máquina baixar uma cópia de cada e abrir um decodificador para cada — e nem
    /// a descida nem a CPU de quem assiste entram na conta do teto.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem sessão.
    /// **A escolha é gravada antes de o pedido sair**, e é o que a torna a
    /// autoridade sobre o que aparece. ADR 0054.
    ///
    /// Antes ela morava numa variável da janela, e a janela não sobrevive a uma
    /// recarga. Deste lado ela sobrevive, e é deste lado que
    /// [`Snapshot::tela`] e [`TransmissaoNaSala::assistida`] a leem — então a
    /// autoria, os controles e os metadados param de poder apontar para outra
    /// tela.
    pub fn assistir(&self, tela: u32, quero: bool) -> Result<(), ConnectionError> {
        let tela = seele_core::ScreenId(tela);
        let escolha = if quero {
            EscolhaDeTela::Esta(tela)
        } else if self.shared.tela_escolhida().tela() == Some(tela)
            || self.shared.tela_escolhida() == EscolhaDeTela::NinguemEscolheu
        {
            // **Recusar a que se estava assistindo é escolher não assistir.**
            // Recusar **outra** não mexe na escolha: é o que acontece quando o
            // servidor empurra uma transmissão que esta pessoa não pediu, e
            // devolvê-la não pode desfazer a escolha dela.
            EscolhaDeTela::Nenhuma
        } else {
            self.shared.tela_escolhida()
        };
        if self.shared.gravar_tela_escolhida(escolha) {
            self.shared.notify(&Event::ScreenChanged);
        }
        // Devolver a que o servidor ligou é deixar de recebê-la: a leitura do
        // par é abortada sem `TelaFechou`, e a contagem dela não pode ficar.
        if !quero {
            self.shared.esquecer_tela_ligada(tela);
        }
        self.command(Command::Assistir { tela, quero })
    }

    /// Qual transmissão esta sessão escolheu assistir, e em que estado. ADR 0054.
    ///
    /// A casca a lê para desenhar a fileira de botões sem guardar a escolha
    /// dela própria — duas cópias da mesma escolha são duas cópias que
    /// discordam depois de uma recarga.
    #[must_use]
    pub fn tela_escolhida(&self) -> Option<u32> {
        self.shared.tela_escolhida().tela().map(|tela| tela.0)
    }

    /// O volume do som da transmissão. `1.0` é o de origem, `0.0` é calado.
    ///
    /// **Independente do volume das pessoas**, que é o pedido inteiro do R17:
    /// silenciar o conteúdo compartilhado tem de preservar a conversa. O
    /// isolamento total continua calando a mistura inteira, porque é isso que
    /// ele diz que faz.
    ///
    /// Guardado na conexão, e posto em cada voz que abrir: sair da sala e
    /// entrar de novo não o devolve a 100. Sem voz viva, só grava. M-2 do lote V.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] numa sessão sem áudio.
    pub fn ajustar_volume_da_tela(&self, volume: f32) -> Result<(), ConnectionError> {
        self.shared
            .voice
            .escolher_o_som_da_tela(|escolhida| escolhida.volume_da_tela = volume.clamp(0.0, 4.0))
    }

    /// Cala ou devolve o som da transmissão, sem mexer no volume dela.
    ///
    /// Guardado como o volume dela: ver [`Self::ajustar_volume_da_tela`].
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] numa sessão sem áudio.
    pub fn calar_a_tela(&self, calada: bool) -> Result<(), ConnectionError> {
        self.shared
            .voice
            .escolher_o_som_da_tela(|escolhida| escolhida.tela_calada = calada)
    }

    /// O que esta máquina faz com o **próprio** áudio ao capturar uma tela.
    ///
    /// R21 e ADR 0054. Três respostas, e uma delas é «a saída inteira entra» —
    /// que a tela tem de dizer, junto com as duas saídas que existem:
    /// compartilhar uma janela em vez do monitor, ou transmitir sem áudio.
    ///
    /// Estática e não por sessão: é uma propriedade desta máquina e deste
    /// sistema, e vale antes de haver sessão — que é quando a pessoa está
    /// escolhendo o que compartilhar.
    #[must_use]
    pub fn exclusao_do_som_da_captura() -> ExclusaoDoSom {
        seele_core::exclusao_medida_no_sistema().into()
    }

    /// O volume e o mudo do som da transmissão, como a casca os desenha.
    ///
    /// Com a voz viva, os dela. **Sem voz — fora de sala, ou com ela abrindo —,
    /// os guardados**, que são os que a voz vai vestir ao abrir. Era `None` ali,
    /// e SILENCIAR sem voz gravava o mudo sem o botão mudar (achado 6 da revisão
    /// do lote V). `None` só numa sessão que entrou sem áudio.
    #[must_use]
    pub fn som_da_tela(&self) -> Option<SomDaTela> {
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                return Some(SomDaTela {
                    volume: voice.volume_da_tela(),
                    calada: voice.tela_calada(),
                });
            }
        }
        self.shared.voice.som_da_tela_escolhido()
    }

    /// Diz o que esta máquina aceita no caminho entre pares — ADR 0045 do §5.
    ///
    /// **As duas perguntas são separadas de propósito**, e o `control.rs` é
    /// explícito em não juntá-las num interruptor só:
    ///
    /// - `pares_que_atende` é *quanto da minha internet eu aceito gastar
    ///   servindo os outros*. Número, e não sim/não: um interruptor responderia
    ///   pela pessoa, fixando o teto em um. `0` é «não empresto».
    /// - `assiste_por_par` é *aceito que o meu endereço seja entregue a quem
    ///   for me servir*. É a metade de privacidade, e ela é de quem recebe.
    ///
    /// Esta versão do cliente atende no máximo
    /// [`pares_que_esta_versao_atende`], e o núcleo **baixa** um pedido maior
    /// em vez de recusá-lo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] sem sessão.
    pub fn consentir_no_caminho_entre_pares(
        &self,
        pares_que_atende: u8,
        assiste_por_par: bool,
    ) -> Result<(), ConnectionError> {
        self.command(Command::ConsentirNoCaminhoEntrePares {
            pares_que_atende,
            assiste_por_par,
        })
    }

    /// Switches this session to another sound output.
    ///
    /// `device` is a [`PlaybackDevice::id`] from [`playback_devices`]; `None`
    /// goes back to the machine's default. The microphone stays where it is.
    ///
    /// Takes effect **now**, for the reason its twin does and one more: somebody
    /// changes output in the middle of a conversation they cannot hear, and
    /// "leave the server and come back" is not an instruction you can give
    /// somebody who is already unable to follow what is being said.
    ///
    /// Isolamento total survives the switch. That is decided in
    /// `Voice::switch_playback`, not here, so the terminal cannot decide it
    /// differently — and it matters because changing output is exactly what
    /// somebody does when they cannot hear anything, muted speakers included.
    ///
    /// Fora de sala, só grava — ver [`Connection::set_capture_device`].
    ///
    /// # Errors
    ///
    /// [`ConnectionError::PlaybackDeviceGone`] when the machine is not offering that
    /// device any more, in which case **nothing changed** and the sound is still
    /// coming out of the old one. [`ConnectionError::NoAudioDevice`] when this session
    /// has no audio at all.
    pub fn set_playback_device(&self, device: Option<String>) -> Result<(), ConnectionError> {
        let pedido = device.clone();
        self.switch_device(Lado::Saida, pedido.clone(), |running, media, ssrc| {
            running
                .switch_playback(device.as_deref(), media, ssrc)
                .map_err(|error| {
                    tracing::warn!(%error, "could not open the chosen sound output");
                    ConnectionError::PlaybackDeviceGone
                })
        })?;
        self.conferir_troca(pedido.as_deref(), Lado::Saida)
    }

    /// Confere que a troca **aconteceu**, e não só que ninguém devolveu erro.
    ///
    /// # Por que isto existe
    ///
    /// Trocar a saída de som no Windows era relatado assim: «ele mostra EM USO
    /// num e ESCOLHIDO no que eu escolhi, mas não muda». `EM USO` sai do
    /// dispositivo que o `Voice` **abriu**, e `ESCOLHIDO` da preferência —
    /// então os dois discordando é o produto dizendo, sem saber, que a ordem
    /// foi gravada e não cumprida.
    ///
    /// Nada nesse caminho devolvia erro: `resolve` acha o dispositivo, `open`
    /// abre, a troca volta `Ok`. Um sucesso que não é sucesso é a forma exata
    /// dos três defeitos que custaram versões inteiras nesta casca — o `{}` do
    /// compartilhamento, o quadro-chave descartado, o assento devolvido sem
    /// anúncio. Todos silenciosos, todos achados só quando alguém foi conferir
    /// se o que se pediu foi feito.
    ///
    /// Conferir é barato: uma leitura de campo depois de uma troca que só
    /// acontece quando alguém aperta um botão. E ela é o que transforma «não
    /// muda e não diz nada» em uma frase que aponta para o lugar certo.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::PlaybackDeviceGone`] ou
    /// [`ConnectionError::CaptureDeviceGone`] quando o dispositivo aberto não é
    /// o que foi pedido. Sem pedido — o padrão do sistema — não há o que
    /// conferir: qualquer dispositivo que abra é o certo.
    fn conferir_troca(&self, pedido: Option<&str>, lado: Lado) -> Result<(), ConnectionError> {
        let Some(pedido) = pedido else {
            return Ok(());
        };
        let Ok(voice) = self.shared.voice.lock() else {
            return Ok(());
        };
        let Some(running) = voice.as_ref() else {
            return Ok(());
        };
        let aberto = match lado {
            Lado::Entrada => running.capture().map(|aparelho| aparelho.id),
            Lado::Saida => running.playback().map(|aparelho| aparelho.id),
        };
        conferencia(Some(pedido), aberto.as_deref(), lado)
    }

    /// Replaces the live voice path with one the caller reopens.
    ///
    /// Shared by both switches so that the order stays one decision: open the
    /// new path, and only then let go of the old. Written twice, one copy would
    /// eventually drop first and leave somebody with no audio at all on the one
    /// path where the device turned out to be missing.
    ///
    /// **Sem voz viva, só grava** (pendência 52): fora de sala, ou com a voz
    /// ainda abrindo. A abertura em curso confere a escolha quando chega, e
    /// recomeça se ela mudou no meio — ver [`PortaDaVoz::chegou`].
    fn switch_device(
        &self,
        lado: Lado,
        pedido: Option<String>,
        reopen: impl FnOnce(&Voice, MediaChannel, Ssrc) -> Result<Voice, ConnectionError>,
    ) -> Result<(), ConnectionError> {
        if !self.shared.voice.quer_audio {
            return Err(ConnectionError::NoAudioDevice);
        }
        let Ok(mut voice) = self.shared.voice.lock() else {
            return Err(ConnectionError::NoAudioDevice);
        };
        if let Some(running) = voice.as_ref() {
            let Some((media, ssrc)) = self.shared.media.lock().ok().and_then(|slot| slot.clone())
            else {
                return Err(ConnectionError::NotConnected);
            };
            let fresh = reopen(running, media, ssrc)?;
            *voice = Some(fresh);
        }
        // Gravada só depois da troca dar certo: uma que falhou deixou o aparelho
        // antigo de pé, e é ele que continua sendo a escolha.
        self.shared.voice.guardar_aparelho(lado, pedido);
        Ok(())
    }

    /// Chooses how the microphone opens.
    ///
    /// Guardado fora de sala, e aplicado quando a voz abrir. A casca chama isto
    /// logo depois do `connect`, com o modo gravado em disco, e é por isso que
    /// ele tem de valer sem voz nenhuma de pé.
    pub fn set_voice_mode(&self, mode: VoiceMode) {
        self.shared
            .voice
            .escolher(|escolhida| escolhida.modo = mode.into());
    }

    /// Liga, desliga ou dosa a supressão de ruído do microfone. F02.
    ///
    /// `1.0` é ela inteira, `0.0` a desliga. A escolha vale para esta sessão e
    /// **não** é gravada aqui: gravar é da casca, que é quem tem onde. Ver
    /// `seele_core::preferences::Preferences::set_supressao_de_ruido`.
    ///
    /// Guardada fora de sala, e aplicada quando a voz abrir.
    pub fn set_supressao_de_ruido(&self, forca: f32) {
        self.shared
            .voice
            .escolher(|escolhida| escolhida.supressao = forca.clamp(0.0, 1.0));
    }

    /// A sensibilidade da ativação por voz, em dBFS. `None` volta ao padrão. F01.
    ///
    /// O padrão depende de a supressão estar ligada — o alvo de −60 dBFS é do
    /// caminho **com** supressão. Ver `seele_core::voice`.
    ///
    /// Guardada fora de sala, e aplicada quando a voz abrir.
    pub fn set_abertura_da_voz_dbfs(&self, dbfs: Option<f32>) {
        let dbfs = dbfs.map(abertura_na_faixa);
        self.shared
            .voice
            .escolher(|escolhida| escolhida.abertura_dbfs = dbfs);
    }

    /// Veste esta sessão com o que está gravado em disco: o modo, a supressão
    /// de ruído e a sensibilidade. Acréscimo A do lote I-b.
    ///
    /// A casca chama isto logo depois do `connect`. Antes ela aplicava só o
    /// modo, e a supressão e a sensibilidade gravadas ficavam de fora: a sessão
    /// começava nos padrões até a pessoa mexer, e a tela de configuração, que
    /// lê o disco fora de sessão, mostrava outra coisa. Fora de sala, só grava;
    /// a voz que abrir veste.
    pub fn vestir_as_escolhas_gravadas(&self, ajustes: &preferences::Preferences) {
        vestir_com_o_disco(&self.shared.voice, ajustes);
    }

    /// Os controles de voz desta sessão, como a casca os desenha. F01 e F02.
    ///
    /// Com a voz viva, os dela. **Sem voz — fora de sala, ou com ela abrindo —,
    /// os que a pessoa escolheu**, que são os que a voz vai vestir ao abrir
    /// (pendência 52). Era `None` aí, e a tela de configuração caía nos
    /// valores do disco, que podem não ser os desta sessão.
    ///
    /// **`None` numa sessão que entrou sem áudio**, e a casca lê o disco: ali
    /// não há voz que vá vestir controle nenhum, e os guardados seriam só os
    /// padrões, por cima da escolha gravada (M-e da rodada 1 do lote I-a).
    #[must_use]
    pub fn controles_da_voz(&self) -> Option<ControlesDaVoz> {
        if !self.shared.voice.quer_audio {
            return None;
        }
        if let Ok(voice) = self.shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                let (abertura_dbfs, escolhida) = voice.abertura_dbfs();
                return Some(ControlesDaVoz {
                    supressao: voice.supressao(),
                    abertura_dbfs,
                    abertura_escolhida: escolhida,
                    abertura_minima_dbfs: *seele_core::FAIXA_DE_ABERTURA_DBFS.start(),
                    abertura_maxima_dbfs: *seele_core::FAIXA_DE_ABERTURA_DBFS.end(),
                });
            }
        }
        let escolhida = self.shared.voice.escolhida();
        Some(ControlesDaVoz {
            supressao: escolhida.supressao,
            // O mesmo padrão da voz, e pela mesma regra: −60 dBFS é do caminho
            // com filtro.
            abertura_dbfs: escolhida
                .abertura_dbfs
                .unwrap_or(if escolhida.supressao > 0.0 {
                    ABERTURA_COM_SUPRESSAO_DBFS
                } else {
                    ABERTURA_SEM_SUPRESSAO_DBFS
                }),
            abertura_escolhida: escolhida.abertura_dbfs.is_some(),
            abertura_minima_dbfs: ABERTURA_MINIMA_DBFS,
            abertura_maxima_dbfs: ABERTURA_MAXIMA_DBFS,
        })
    }

    /// Sets one talker's volume, as a percentage. 100 is unchanged.
    ///
    /// **Grava e aplica** (F1-vazamento, passo 3 (2)). O volume é guardado por
    /// apelido nesta conexão, e vale pelo `ssrc` que o apelido tiver: agora,
    /// na voz viva; e depois, na voz que abrir e no número novo de quem voltar —
    /// ver [`VozDaSala::seguir_os_volumes`]. Antes ele ia direto para o ganho do
    /// número de agora e não ficava em lugar nenhum, então fora de sala era
    /// recusado e a voz seguinte abria com todo mundo a 100.
    ///
    /// Fora de sala, só grava, como o mudo e os aparelhos (pendência 52).
    ///
    /// # Errors
    ///
    /// [`ConnectionError::UnknownPerson`] if nobody here is called that,
    /// [`ConnectionError::NoAudioDevice`] if this session has no audio.
    pub fn set_volume(&self, nickname: String, percent: u16) -> Result<(), ConnectionError> {
        // Quem não está aqui primeiro, com ou sem áudio: é a ordem de antes, e
        // a conformidade conta com ela.
        let conhecida = self
            .shared
            .room
            .lock()
            .is_ok_and(|room| room.ssrc_of(&nickname).is_some());
        if !conhecida {
            return Err(ConnectionError::UnknownPerson);
        }
        if !self.shared.voice.quer_audio {
            return Err(ConnectionError::NoAudioDevice);
        }
        // Gravado antes de olhar a voz: ver a ordem dos cadeados em
        // `seguir_os_volumes`.
        self.shared.voice.guardar_volume(&nickname, percent);
        self.shared.voice.seguir_os_volumes(&self.shared.room);
        Ok(())
    }

    /// O volume que esta sessão guardou para cada pessoa, por apelido, em por
    /// cento. F1-vazamento.
    ///
    /// É por aqui, e **nunca** pelo [`Snapshot`], que o volume chega à janela:
    /// o retrato vai inteiro aos MODs, e quanto esta máquina quer ouvir cada um
    /// não é deles. Vazio numa conexão nova — os volumes não passam de um
    /// servidor para outro —, e vazio numa sessão sem áudio.
    #[must_use]
    pub fn volumes_locais(&self) -> HashMap<String, u16> {
        self.shared.voice.volumes()
    }

    // ---- compartilhamento de tela ----
    //
    // Seis métodos, e a forma deles é o contrato de 22/08, fixado antes desta
    // ponte e desta casca para que as duas pudessem ser escritas ao mesmo
    // tempo. O que está escrito abaixo de cada um é o que **desta** metade
    // existe hoje; o relatório da onda 2 diz o resto com todas as letras.

    /// As telas e janelas que esta máquina pode transmitir.
    ///
    /// Uma lista vazia significaria «o sistema recusou a permissão», e aí
    /// [`Connection::permissao_de_tela`] diria qual foi a recusa — uma lista vazia sem
    /// motivo é um beco. Por isso a falta de captura sai como erro e não como
    /// lista curta.
    ///
    /// O número em [`FonteDeTela::id`] é o índice **desta** listagem, e a casca
    /// tem de devolver um dos que acabou de receber: listar de novo renumera.
    /// Ver `seele_core::FonteDeTela::id`, que diz por que um identificador
    /// estável não existe nos dois sistemas.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ScreenShareUnavailable`] quando esta máquina não tem
    /// captura de tela, ou quando o sistema recusou listar. Uma sessão sem
    /// monitor e sem janela **não** é erro: a lista vem vazia, porque não ter o
    /// que compartilhar é estado.
    pub fn fontes_de_tela(&self) -> Result<Vec<FonteDeTela>, ConnectionError> {
        let fontes = match seele_core::fontes_de_tela() {
            Ok(fontes) => fontes,
            // **Vazio, e não erro**, e é o contrato escrito acima: uma recusa de
            // permissão é a lista vazia que [`Connection::permissao_de_tela`] explica.
            // Devolvê-la como erro faria a casca escrever «esta máquina não tem
            // como compartilhar» ao lado do bloco que oferece pedir a permissão
            // — duas frases sobre o mesmo estado, e a primeira mentindo.
            Err(seele_core::ErroDeFontes::SemPermissao) => Vec::new(),
            Err(erro) => {
                tracing::debug!(%erro, "não deu para listar o que esta máquina compartilha");
                return Err(ConnectionError::ScreenShareUnavailable);
            }
        };

        let lista = fontes
            .iter()
            .map(|fonte| FonteDeTela {
                id: fonte.id(),
                nome: fonte.rotulo().to_owned(),
                monitor: fonte.monitor(),
                largura: fonte.largura(),
                altura: fonte.altura(),
            })
            .collect();

        // A lista fica deste lado inteira: o alvo nativo é o que a captura
        // precisa, e ele não cabe numa travessia de JSON.
        if let Ok(mut guardadas) = self.shared.fontes_de_tela.lock() {
            *guardadas = fontes;
        }
        Ok(lista)
    }

    /// O que o sistema operacional respondeu sobre gravar a tela.
    ///
    /// Não pergunta nada: só olha. Quem pergunta é
    /// [`Connection::pedir_permissao_de_tela`], e a separação é do §4 — no macOS o
    /// alerta do TCC aparece **uma vez só** por instalação, então uma consulta
    /// que perguntasse gastaria a única chance que a pessoa tem.
    #[must_use]
    pub fn permissao_de_tela(&self) -> PermissaoDeTela {
        permissao_daqui(seele_core::permissao_de_tela())
    }

    /// Pede a permissão ao sistema.
    ///
    /// No Windows devolve `Concedida` sem perguntar nada, porque lá não há
    /// permissão de sistema — o único consentimento é o da nossa interface. No
    /// macOS abre o alerta do TCC, **uma vez**: da segunda em diante o sistema
    /// responde o mesmo que a consulta sem mostrar nada, e o caminho de volta
    /// são os Ajustes.
    #[must_use]
    pub fn pedir_permissao_de_tela(&self) -> PermissaoDeTela {
        permissao_daqui(seele_core::pedir_permissao_de_tela())
    }

    /// Começa a transmitir a fonte escolhida, com os limites escolhidos.
    ///
    /// Uma por sala de voz: se outra pessoa já estiver compartilhando, quem
    /// perde a corrida é avisado pelo servidor — [`NoticeReason::ScreenShareTaken`]
    /// num [`Event::NoticeRaised`]. Esta ponte **não** confere a corrida por
    /// conta própria: `specs/08-seguranca.md` põe a decisão no servidor, e uma
    /// casca que recusasse aqui seria uma segunda autoridade discordando da
    /// primeira no primeiro atraso de rede.
    ///
    /// **`Ok` quer dizer «foi pedido», e não «está saindo».** O `StartScreenShare`
    /// sai daqui; o nome da transmissão volta depois, num `ScreenShareStarted`,
    /// e é só nesse instante que o codificador nasce. A casca vê o resultado
    /// pelo [`Snapshot`], como vê todo o resto.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ScreenShareUnavailable`] quando esta máquina não tem como
    /// começar: a fonte escolhida não está na última lista, o módulo do Cisco
    /// não está em disco, ou este build não tem captura.
    /// [`ConnectionError::NotConnected`] quando a sessão já acabou.
    pub fn compartilhar_tela(
        &self,
        fonte: u64,
        limites: LimitesDeTela,
    ) -> Result<(), ConnectionError> {
        let comecou = self.comecar_a_transmitir(fonte, limites);
        // O pedido é guardado **quando há transmissão a que ele pertença**, e
        // apagado quando não há. Guardá-lo depois de uma recusa poria, na coluna
        // do que foi pedido, o teto de um botão que devolveu erro — e o §5 quer
        // ali a metade que explica a outra, não um número solto.
        self.shared
            .gravar_pedido_da_tela(comecou.is_ok().then_some(limites));
        comecou
    }

    /// A metade que abre a transmissão de verdade, separada para que
    /// [`Connection::compartilhar_tela`] fique sendo só a memória do que foi pedido.
    ///
    /// As três recusas que ela sabe dar acontecem **antes** de qualquer coisa
    /// sair pelo fio, e é para isso que ela é síncrona: uma fonte que sumiu ou
    /// um codec que não está em disco viram uma resposta na mão de quem apertou,
    /// e não um silêncio no laço de comandos.
    fn comecar_a_transmitir(
        &self,
        fonte: u64,
        limites: LimitesDeTela,
    ) -> Result<(), ConnectionError> {
        let escolhida = self.fonte_escolhida(fonte)?;
        let biblioteca = modulo_de_video()?;
        let pedido = seele_core::PedidoDeTela::DoSistema {
            biblioteca,
            origem: escolhida.origem(),
            captura: seele_core::CapturaEmCaixa::nova(escolhida.captura()),
        };
        // A tela entra no lugar da câmera, se havia uma: a porta dos quadros
        // dela fecha aqui, e a casca que ainda empurrar ouve
        // [`ConnectionError::CameraNotLive`].
        self.shared.largar_a_camera();
        self.command(Command::ShareScreen {
            pedido: Box::new(pedido),
            limites: limites_do_nucleo(limites),
        })
    }

    /// Começa a transmitir a **câmera**, no lugar da tela.
    ///
    /// # Quem captura e quem codifica
    ///
    /// A casca. Este workspace proíbe `unsafe`, e não há captura de câmera
    /// nativa sem ele; a janela tem o `getUserMedia` e o `VideoEncoder` do
    /// sistema. `largura` e `altura` são as medidas com que **ela** codifica —
    /// vão no cabeçalho de abertura, e quem assiste arma o decodificador por
    /// elas. Os quadros chegam depois, por [`Self::quadro_da_camera`].
    ///
    /// # Uma imagem por pessoa
    ///
    /// A câmera vai pelo mesmo caminho da tela — o mesmo `StartScreenShare`, a
    /// mesma permissão, o mesmo fluxo —, e **no lugar dela**: quem estava
    /// compartilhando a tela para de compartilhá-la quando liga a câmera. Quem
    /// avisa a pessoa disso é a casca, antes de chamar.
    ///
    /// **`Ok` quer dizer «foi pedido»**, como em [`Self::compartilhar_tela`]:
    /// o fluxo abre quando o servidor dá nome à transmissão, e a casca fica
    /// sabendo por um [`Event::ScreenOpened`] da própria transmissão, com
    /// `source: "camera"`. Só então há para onde mandar quadro.
    ///
    /// # Errors
    ///
    /// - [`ConnectionError::NotConnected`] sem sessão;
    /// - [`ConnectionError::ServidorAntigo`] quando o servidor negociou menos
    ///   que o protocolo da câmera (13). **Nada saiu**: um servidor do 12
    ///   anunciaria a transmissão e recusaria o fluxo dela, e a sala ficaria
    ///   olhando uma tela que não existe;
    /// - [`ConnectionError::CameraBadSize`] para medidas que o cabeçalho de
    ///   abertura recusaria.
    pub fn compartilhar_camera(
        &self,
        largura: u32,
        altura: u32,
        limites: LimitesDeTela,
    ) -> Result<(), ConnectionError> {
        let comecou = self.comecar_a_camera(largura, altura, limites);
        self.shared
            .gravar_pedido_da_tela(comecou.is_ok().then_some(limites));
        comecou
    }

    /// A metade de [`Self::compartilhar_camera`] que recusa antes do fio.
    fn comecar_a_camera(
        &self,
        largura: u32,
        altura: u32,
        limites: LimitesDeTela,
    ) -> Result<(), ConnectionError> {
        let negociado = self.protocolo_negociado();
        if negociado == 0 {
            return Err(ConnectionError::NotConnected);
        }
        let exige = seele_core::PROTOCOLO_DA_CAMERA;
        if negociado < exige {
            return Err(ConnectionError::ServidorAntigo { negociado, exige });
        }
        let (largura, altura) = medidas_da_camera(largura, altura)?;
        let (entrada, quadros) = seele_core::de_fora::canal_de_fora(largura, altura);
        // A porta é trocada **antes** de o pedido sair: a resposta do servidor
        // chega por outra thread, e um quadro que encontrasse a porta da
        // câmera anterior iria para uma transmissão que acabou.
        if let Ok(mut camera) = self.shared.camera.lock() {
            *camera = Some(entrada);
        }
        let pedido = seele_core::PedidoDeTela::DeFora(quadros);
        let pedido = self.command(Command::ShareScreen {
            pedido: Box::new(pedido),
            limites: limites_da_camera(limites),
        });
        if pedido.is_err() {
            self.shared.largar_a_camera();
        }
        pedido
    }

    /// **O que a casca manda o codificador dela fazer com esta câmera**: as
    /// medidas, a cadência, o teto de banda e o codec.
    ///
    /// `largura` e `altura` são as que a câmera entrega. As medidas que voltam
    /// são as do degrau que os limites pedem, na proporção dela, e nunca
    /// maiores que ela — o mesmo que o codificador de tela faz. Não fala com
    /// o servidor: é conta.
    #[must_use]
    pub fn codificacao_da_camera(
        &self,
        largura: u32,
        altura: u32,
        limites: LimitesDeTela,
    ) -> CodificacaoDaCamera {
        codificacao_da_camera(largura, altura, limites)
    }

    /// Entrega um quadro da câmera, já codificado em H.264 Annex-B.
    ///
    /// `chave` diz se dá para começar a decodificar por ele. O primeiro de um
    /// fluxo tem de ser, e a casca manda outro a cada
    /// [`Event::CameraKeyFrameRequested`].
    ///
    /// # Errors
    ///
    /// Nenhum quadro some calado:
    /// - [`ConnectionError::CameraNotLive`] quando não há câmera a transmitir;
    /// - [`ConnectionError::CameraQueueFull`] quando a fila encheu — o quadro
    ///   não foi, e o próximo tem de ser chave;
    /// - [`ConnectionError::CameraBadSize`] para um quadro vazio ou maior que
    ///   o fio carrega.
    ///
    /// Quando o quadro entra, volta **quantos estão na fila, contando este**
    /// (`seele_core::de_fora::EntradaDeFora::quadro`): a casca deixa de
    /// codificar com a fila pela metade, em vez de esperar a recusa — que
    /// custa um quadro-chave.
    pub fn quadro_da_camera(&self, chave: bool, bytes: Vec<u8>) -> Result<u32, ConnectionError> {
        let entrada = self
            .shared
            .camera
            .lock()
            .ok()
            .and_then(|camera| camera.clone())
            .ok_or(ConnectionError::CameraNotLive)?;
        let na_fila = entrada
            .quadro(chave, bytes)
            .map_err(|recusa| match recusa {
                seele_core::de_fora::RecusaDeFora::FilaCheia { recusados } => {
                    // Dito uma vez por potência de dois: a 30 quadros por segundo,
                    // uma linha por recusa seria o próprio registro a engasgar.
                    if recusados.is_power_of_two() {
                        tracing::warn!(
                            recusados,
                            "a fila de quadros da câmera encheu: a rede não leva no passo de quem \
                         codifica"
                        );
                    }
                    ConnectionError::CameraQueueFull { dropped: recusados }
                }
                seele_core::de_fora::RecusaDeFora::Fechada => {
                    self.shared.largar_a_camera();
                    ConnectionError::CameraNotLive
                }
                seele_core::de_fora::RecusaDeFora::Vazio
                | seele_core::de_fora::RecusaDeFora::GrandeDemais { .. } => {
                    ConnectionError::CameraBadSize
                }
            })?;
        Ok(u32::try_from(na_fila).unwrap_or(u32::MAX))
    }

    /// Tira da última listagem a fonte que a casca escolheu.
    ///
    /// Lista de novo quando não há listagem guardada, e **só** nesse caso: uma
    /// casca que desenhou o menu tem a lista, e relistar por baixo dela
    /// renumeraria os índices no meio da escolha.
    /// **As prévias da última lista de fontes** ([`Self::fontes_de_tela`]): a
    /// imagem de cada monitor e de cada janela, entregue a `cada` com o `id` e
    /// o nome que a lista deu, uma por vez e na ordem da lista.
    ///
    /// Capturar leva tempo (abrir a captura, esperar o primeiro quadro), e
    /// quem chama roda isto fora da linha principal. A trava da lista só é
    /// tomada para abrir cada captura: a espera pelo quadro é fora dela, e um
    /// `compartilhar_tela` no meio não fica preso. Uma fonte que não entrega
    /// quadro (minimizada, fechada entre a lista e agora) fica sem prévia.
    ///
    /// No máximo [`MAX_PREVIAS_DE_TELA`] fontes: além disso a caixa já é uma
    /// lista longa, e cada prévia custa uma captura.
    pub fn previas_de_tela(&self, mut cada: impl FnMut(u64, &str, seele_core::PreviaDeTela)) {
        let ids: Vec<u64> = self.shared.fontes_de_tela.lock().map_or_else(
            |_| Vec::new(),
            |guardadas| {
                guardadas
                    .iter()
                    .map(seele_core::FonteDeTela::id)
                    .take(MAX_PREVIAS_DE_TELA)
                    .collect()
            },
        );
        for lote in ids.chunks(LOTE_DE_PREVIAS) {
            let mut abertas = Vec::with_capacity(lote.len());
            let mut a_lista_mudou = false;
            for id in lote {
                let comecada = self
                    .shared
                    .fontes_de_tela
                    .lock()
                    .ok()
                    .and_then(|guardadas| {
                        guardadas
                            .iter()
                            .find(|fonte| fonte.id() == *id)
                            .map(|fonte| (fonte.rotulo().to_owned(), fonte.comecar_previa()))
                    });
                match comecada {
                    Some((nome, Ok(em_curso))) => abertas.push(((*id, nome), em_curso)),
                    Some((nome, Err(erro))) => {
                        tracing::debug!(%erro, fonte = %nome, "a prévia desta fonte não abriu");
                    }
                    None => {
                        // A lista mudou (uma relista, uma fonte escolhida): o
                        // resto é de outra lista, e a tela pede de novo.
                        a_lista_mudou = true;
                        break;
                    }
                }
            }
            seele_core::colher_previas(
                abertas,
                seele_core::PRAZO_DA_PREVIA,
                seele_core::PreviaEmCurso::olhar,
                |(id, nome), previa| cada(id, &nome, previa),
            );
            if a_lista_mudou {
                return;
            }
        }
    }

    fn fonte_escolhida(&self, fonte: u64) -> Result<seele_core::FonteDeTela, ConnectionError> {
        let mut guardadas = self
            .shared
            .fontes_de_tela
            .lock()
            .map_err(|_| ConnectionError::ScreenShareUnavailable)?;
        if guardadas.is_empty() {
            *guardadas = seele_core::fontes_de_tela().map_err(|erro| {
                tracing::debug!(%erro, "não deu para listar o que esta máquina compartilha");
                ConnectionError::ScreenShareUnavailable
            })?;
            // Aqui a recusa de permissão **é** erro, ao contrário de
            // [`Connection::fontes_de_tela`]: ali a lista vazia é uma resposta que a
            // tela sabe desenhar, e aqui alguém já escolheu uma fonte que não
            // existe mais.
        }
        let onde = guardadas
            .iter()
            .position(|candidata| candidata.id() == fonte)
            .ok_or(ConnectionError::ScreenShareUnavailable)?;
        Ok(guardadas.swap_remove(onde))
    }

    /// Para de transmitir.
    ///
    /// Idempotente: parar sem estar compartilhando não é erro, e é a resposta
    /// que esta máquina dá hoje, porque ela nunca chega a compartilhar.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] quando a sessão já acabou.
    pub fn parar_de_compartilhar(&self) -> Result<(), ConnectionError> {
        // O que foi pedido morre com a transmissão, e parar é uma das duas
        // maneiras de ela acabar — a outra é o `ScreenShareStopped` que `fold`
        // dobra. As duas limpam, porque só uma delas acontece de cada vez:
        // quando esta pessoa aperta PARAR, o quadro do servidor pode nunca chegar.
        self.shared.gravar_pedido_da_tela(None);
        self.shared.esquecer_a_parada_da_tela();
        // E a porta da câmera, se era uma câmera: um quadro que a casca ainda
        // empurrar depois de parar ouve [`ConnectionError::CameraNotLive`].
        self.shared.largar_a_camera();
        self.command(Command::StopScreenShare)
    }

    /// Muda os tetos no meio da transmissão.
    ///
    /// **Recomeça o fluxo quando a resolução ou a cadência mudam**, e não há
    /// como não recomeçar: o tamanho da imagem vai no cabeçalho de abertura, e
    /// um fluxo que abriu dizendo 1080p não tem onde dizer «daqui para frente é
    /// 720p». Quem assiste vê a imagem piscar uma vez. Mexer só na banda não
    /// pisca nada — ela é uma perna do teto, e o teto se ajusta dentro do fluxo.
    ///
    /// Uma escolha idêntica à que já vale não faz nada, e é por isso que apertar
    /// APLICAR duas vezes seguidas não pisca a tela de ninguém.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::NotConnected`] quando a sessão já acabou.
    pub fn ajustar_limites_da_tela(&self, limites: LimitesDeTela) -> Result<(), ConnectionError> {
        // Guardado aqui, e não quando a bomba responder: a coluna «pedido» do
        // painel é a escolha da pessoa, e ela vale desde o aperto. A coluna de
        // ao lado — o que está saindo — é que espera o degrau acompanhar, e as
        // duas juntas são o §5 inteiro: escolher 1080p e receber 720p não é
        // defeito, esconder que aconteceu é.
        self.shared.gravar_pedido_da_tela(Some(limites));
        self.command(Command::AdjustScreenLimits {
            limites: limites_do_nucleo(limites),
        })
    }

    /// Subscribes to changes.
    ///
    /// Called back on the driver thread. The shell marshals.
    pub fn subscribe(&self, listener: Arc<dyn EventListener>) {
        if let Ok(mut listeners) = self.shared.listeners.lock() {
            listeners.push(listener);
        }
    }

    /// The conversation in the open Channel, oldest first.
    ///
    /// Separate from [`Connection::snapshot`] because the two change at completely
    /// different rates. Telemetry moves on its own and wants reading twice a
    /// second; the history only moves when somebody says something. Carrying
    /// both in one value meant paying for the conversation on every frame —
    /// cloning each nickname and body, serialising the lot — so a session got
    /// slower the longer it went on. Ask for this when
    /// [`Snapshot::messages_revision`] changes, and not otherwise.
    #[must_use]
    pub fn messages(&self) -> Vec<Message> {
        self.shared
            .room
            .lock()
            .map(|room| messages_of(&room))
            .unwrap_or_default()
    }

    /// O FORA DO AR: os membros deste servidor que não estão conectados agora.
    ///
    /// **Separado do [`Connection::snapshot`], e é isso que protege a lista**:
    /// o `Snapshot` inteiro vai a todo MOD da API 5 para baixo, e a lista de
    /// membros do servidor não é dele (P6, decisão da orquestradora). Muda junto
    /// com o roster: a janela relê no `Event::RosterChanged`.
    pub fn membros_fora_do_ar(&self) -> Vec<MembroForaDoAr> {
        self.shared
            .room
            .lock()
            .map(|room| {
                room.fora_do_ar()
                    .into_iter()
                    .map(|(id, nickname)| MembroForaDoAr {
                        id: id.get(),
                        nickname,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Everything the interface needs, in one value.
    ///
    /// Cheap enough to call on every frame of a redraw, and deliberately a copy:
    /// a shell holding a borrow into live state is a shell that can see the
    /// roster change halfway through drawing it.
    ///
    /// "Cheap" is now true. It used to carry the whole conversation, which made
    /// the cost grow with the session — see [`Snapshot::messages_revision`].
    ///
    /// # E «barato» passou a ser verdade de verdade
    ///
    /// Tirar as mensagens do JSON não tirou a cópia: esta função fazia
    /// `room.clone()`, `Room` carrega a conversa e deriva `Clone`, então o custo
    /// continuava crescendo com o histórico a cada redesenho — quatro vezes por
    /// segundo. Era o R12 da revisão da v15.
    ///
    /// Agora os campos são lidos **sob o cadeado**, e o cadeado é soltado antes
    /// de qualquer outra coisa. O que a documentação acima promete continua de
    /// pé: o que sai daqui é uma cópia dos campos que a casca desenha, e nenhuma
    /// casca fica com um empréstimo do estado vivo.
    ///
    /// A ordem também é a que evita um travamento: nada que tome outro cadeado
    /// — `audio_state` toma o da voz — é chamado enquanto este está na mão.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        // As duas escolhas desta ponta, lidas **antes** do cadeado do `Room`:
        // cada uma tem o seu, e nenhuma precisa dele. A foto do `Room` é a do
        // `seele_core::retrato`, a mesma que o navegador monta (fase 2 da
        // Tarefa 2.2 da F4).
        // A assistida é a escolhida, ou — sem escolha — a que o servidor ligou
        // sozinho na primeira transmissão (ver `TelaLigada`).
        let escolhida = self.shared.tela_assistida();
        let pedido = self.shared.pedido_da_tela();
        let parada = self.shared.parada_da_tela();
        let mut foto = match self.shared.room.lock() {
            // A pausa entra por cima da foto, e não pelo `Room`: quem pausa é
            // o teto desta máquina, e o servidor não sabe dela.
            Ok(room) => foto_da_sala(&room, escolhida, pedido).com_a_parada_da_minha_tela(parada),
            // Um cadeado envenenado é uma sessão que já não tem estado. A foto
            // vazia é a resposta honesta, e é a mesma que o `Room::new()` de
            // antes dava.
            Err(_) => foto_da_sala(&Room::new(), None, None),
        };
        // Depois do cadeado do `Room`, com o próprio: o codificador é dado
        // desta máquina, e não do `Room`.
        self.shared.vestir_o_codificador(&mut foto);
        // A perda que o servidor contou vai na telemetria, e não na parte do
        // `Room`: ela é lida antes de a foto virar o retrato da sala. O apelido
        // e as permissões saem do retrato (`seele_core::retrato`), que é a
        // mesma conta do navegador.
        let loss_fraction = foto.loss_fraction;
        let sala = foto.retrato(|| {
            self.shared
                .nickname
                .lock()
                .map(|name| name.clone())
                .unwrap_or_default()
        });

        let audio = self.audio_state();

        let signal = self.shared.signal.load(Ordering::Relaxed);
        #[allow(
            clippy::cast_precision_loss,
            reason = "a round trip in microseconds is far below f32's exact range"
        )]
        let rtt_ms = match self.shared.rtt_micros.load(Ordering::Relaxed) {
            // Zero é «nenhum `Pong` voltou ainda», e não uma ida e volta de 0 ms.
            0 => None,
            micros => Some(micros as f32 / 1000.0),
        };

        Snapshot {
            caminho: self.shared.caminho(),
            link: self.shared.enlace(),
            link_state: link_state_from_byte(self.shared.link_state.load(Ordering::Relaxed)),
            server: sala.server,
            icon_revision: self.shared.icon_revision.load(Ordering::Relaxed),
            person_icons_revision: self.shared.person_icons_revision.load(Ordering::Relaxed),
            me: sala.me,
            nickname: sala.nickname,
            voice_rooms: sala.voice_rooms,
            presentes: sala.presentes,
            channels: sala.channels,
            messages_revision: self.shared.messages_revision.load(Ordering::Relaxed),
            fora_do_ar_revision: self.shared.fora_do_ar_revision.load(Ordering::Relaxed),
            // Da sala: sai do `Room` pela foto, na mesma tomada do cadeado.
            identidades_apresentadas: sala.identidades_apresentadas,
            cargos_revision: sala.cargos_revision,
            perfis_revision: sala.perfis_revision,
            pendencia_do_perfil: sala.pendencia_do_perfil,
            telemetry: Telemetry {
                rtt_ms,
                // O jitter que a pessoa quer saber é o de chegada, medido
                // aqui — e não o do relatório do servidor, que é sempre `0.0`
                // porque o servidor não tem como medir jitter. Ver
                // [`Shared::jitter_de_chegada_micros`].
                jitter_ms: self.shared.jitter_de_chegada_ms(),
                loss_fraction,
                bitrate_bps: audio.bitrate_bps,
                signal,
                sync_band: SignalBand::of(signal).into(),
                input_level: audio.input_level,
                local_fault: audio.local_fault,
                frames_refused: audio.frames_refused,
                microfone_entrega_silencio_digital: audio.silencio_digital,
            },
            notice: sala.notice,
            muted: audio.muted,
            total_isolation: audio.total_isolation,
            speaking: audio.speaking,
            voice_mode: audio.mode,
            audio_available: audio.available,
            abertura_do_audio: audio.abertura,
            capture: audio.capture,
            playback: audio.playback,
            aparelho: audio.aparelho,
            trocas_de_aparelho: audio.trocas_de_aparelho,
            may_manage_voice_rooms: sala.may_manage_voice_rooms,
            may_kick: sala.may_kick,
            may_ban: sala.may_ban,
            may_remove_message: sala.may_remove_message,
            may_move_person: sala.may_move_person,
            may_customise_server: sala.may_customise_server,
            may_delete_rooms: sala.may_delete_rooms,
            tela: sala.tela,
            minha_transmissao: sala.minha_transmissao,
            transmissoes: sala.transmissoes,
            ended: sala.ended,
            tema_do_servidor: sala.tema_do_servidor,
        }
    }

    /// Ends the session.
    /// Devolve à sonda o caminho medido da última visita a este servidor.
    ///
    /// Zero não faz nada: é o que a lista de conhecidos guarda para um servidor
    /// onde ninguém compartilhou tela ainda.
    pub fn lembrar_o_caminho(&self, bps: u32) {
        if bps != 0 {
            // Perder isto não é perder nada: uma sessão que já caiu não tem
            // sonda para semear, e a próxima vai ler o mesmo número do disco.
            let _ = self.command(Command::LembrarCaminho(bps));
        }
    }

    /// O caminho de subida que a sonda mediu nesta sessão, em bits por segundo.
    ///
    /// Zero enquanto ninguém compartilhou tela. Quem guarda isto deve tratar
    /// zero como «não mediu» e **deixar o valor antigo** — sobrescrever uma
    /// medida boa com um zero é jogar fora exatamente o que ela serve para
    /// evitar.
    #[must_use]
    pub fn caminho_medido(&self) -> u32 {
        self.shared
            .caminho_medido
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// A impressão do certificado do servidor que esta conexão conferiu.
    ///
    /// **Em toda conexão que subiu**, e não só nas que o veredito conta:
    /// [`Trust::Known`], o de toda volta a um servidor já fixado, não leva a
    /// impressão, e é justamente o de quem hospeda entrando no próprio
    /// servidor pela segunda vez. A casca compara esta com a das hospedagens
    /// dela para saber se a sessão é com uma delas: pela chave, e não pelo
    /// endereço (lote X da F5 da 0.16).
    ///
    /// `None` só numa `Connection` montada à mão, sem aperto de mão.
    #[must_use]
    pub fn impressao_do_servidor(&self) -> Option<String> {
        self.shared
            .impressao_do_servidor
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    /// Ends the session.
    pub fn disconnect(&self) {
        self.shared.running.store(false, Ordering::Relaxed);
        let _ = self.commands.send(Command::Shutdown);
    }

    /// **O código por que esta conexão foi discada**, quando ela foi uma
    /// ligação: a tentativa veio de um `@CÓDIGO` e a ponte perguntou ao ponto
    /// de encontro onde a pessoa mora (`Chegada::para_o_codigo`). `None` para
    /// tudo o que chegou por um endereço — inclusive uma sala que tocou.
    ///
    /// É por aqui que a casca decide o que anotar: uma ligação vai para a
    /// agenda, e não para a lista de visitados (decisão de 16/10 na F7). O
    /// critério é a origem da tentativa, e não o texto do alvo parecer um
    /// código.
    #[must_use]
    pub const fn codigo_da_ligacao(&self) -> Option<Codigo> {
        self.ligacao
    }

    /// Encerra a sessão e espera a thread da conexão terminar, até `teto`.
    ///
    /// Para quem vai embora em seguida: a casca no `ExitRequested`, com o
    /// processo morrendo depois. [`Self::disconnect`] só pede o fim, e o
    /// `CONNECTION_CLOSE` sai pela despedida do enlace, na thread da conexão;
    /// um processo que morre antes dela leva o aviso junto, e quem ficou só vê
    /// a saída pelo prazo de ociosidade do servidor, 20 s. Era a espera cega de
    /// 150 ms da casca, que não sabia se a despedida tinha acontecido.
    ///
    /// Devolve `true` quando a thread terminou: deu o sinal, ou acabou sem
    /// chegar a ele — um runtime que não subiu, um pânico — e não há mais o que
    /// esperar. Também quando outra chamada já viu o fim. `false` só quando o
    /// teto venceu com a thread ainda correndo, e aí quem chama sabe que a
    /// despedida pode não ter saído.
    ///
    /// **Bloqueia** quem chama. Sozinha, até `teto`. Duas chamadas ao mesmo tempo
    /// não dividem a espera: a segunda espera a primeira soltar o cadeado do
    /// recebedor, e depois o próprio teto — até o dobro, quando a thread não
    /// termina.
    #[must_use]
    pub fn encerrar_e_esperar(&self, teto: Duration) -> bool {
        self.disconnect();
        let mut encerrada = self
            .encerrada
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(fim) = encerrada.as_ref() else {
            return true;
        };
        match fim.recv_timeout(teto) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                *encerrada = None;
                true
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
        }
    }

    fn command(&self, command: Command) -> Result<(), ConnectionError> {
        self.commands
            .send(command)
            .map_err(|_| ConnectionError::NotConnected)
    }

    fn audio_state(&self) -> AudioState {
        // Lidos antes do cadeado da voz, e não dentro dele: a ordem dos dois
        // cadeados é a voz primeiro, e é a que `VozDaSala` segue.
        let escolhida = self.shared.voice.escolhida();
        let abertura = self.shared.voice.abertura();
        let Ok(voice) = self.shared.voice.lock() else {
            return AudioState::sem_voz(&escolhida, abertura);
        };
        let Some(voice) = voice.as_ref() else {
            return AudioState::sem_voz(&escolhida, abertura);
        };
        let telemetry = voice.telemetry();
        // Uma leitura só do painel: nome novo com estado velho seria a
        // interface dizendo que já trocou quando ainda está trocando.
        let aparelhos = voice.estado_do_audio();
        AudioState {
            available: true,
            // Com a voz de pé, aberta por definição: o estado guardado pode
            // estar um passo atrás, entre a voz entrar no lugar e ele mudar.
            abertura: AberturaDoAudio::Aberto,
            mode: voice.mode().into(),
            speaking: telemetry.local.speaking,
            muted: voice.muted(),
            total_isolation: voice.total_isolation(),
            input_level: telemetry.local.input_level,
            local_fault: voice.falha_local(),
            frames_refused: voice.quadros_recusados(),
            silencio_digital: telemetry.local.microfone_entrega_silencio_digital,
            bitrate_bps: telemetry.local.bitrate_bps,
            capture: aparelhos.capture.map(|device| CaptureDevice {
                id: device.id,
                name: device.name,
                default: device.default,
            }),
            playback: aparelhos.playback.map(|device| PlaybackDevice {
                id: device.id,
                name: device.name,
                default: device.default,
            }),
            aparelho: aparelhos.estado.into(),
            trocas_de_aparelho: aparelhos.reaberturas,
        }
    }
}

/// O veredito da conferência da troca, sem sessão nenhuma por perto.
///
/// Uma função sobre dois `Option<&str>` e não um método porque é assim que ela
/// vira **teste de comportamento**. O guarda que existia lia o texto-fonte deste
/// arquivo com `include_str!` e conferia se certas palavras apareciam no corpo
/// do método — o que prova que alguém escreveu as palavras, e nada sobre o que
/// o produto faz. «Existir não é funcionar»: um `contains` verde sobreviveria a
/// uma comparação invertida.
///
/// # Errors
///
/// [`ConnectionError::CaptureDeviceGone`] ou
/// [`ConnectionError::PlaybackDeviceGone`] quando o aparelho aberto não é o que
/// foi pedido. Sem pedido — o padrão do sistema — qualquer um que abra é o
/// certo.
fn conferencia(
    pedido: Option<&str>,
    aberto: Option<&str>,
    lado: Lado,
) -> Result<(), ConnectionError> {
    let Some(pedido) = pedido else {
        return Ok(());
    };
    if aberto == Some(pedido) {
        return Ok(());
    }
    tracing::warn!(
        ?pedido,
        ?aberto,
        ?lado,
        "a troca de dispositivo voltou sem erro e o aberto não é o pedido"
    );
    Err(match lado {
        Lado::Entrada => ConnectionError::CaptureDeviceGone,
        Lado::Saida => ConnectionError::PlaybackDeviceGone,
    })
}

/// What the voice path is doing, read once per [`Connection::snapshot`].
///
/// A struct and not the eight-tuple this was: the tuple had already reached the
/// point of needing `clippy::type_complexity` waved through, and its run of
/// consecutive `bool`s was three chances to swap two fields with nothing
/// anywhere to catch it.
struct AudioState {
    available: bool,
    abertura: AberturaDoAudio,
    mode: VoiceMode,
    speaking: bool,
    muted: bool,
    total_isolation: bool,
    input_level: f32,
    local_fault: bool,
    frames_refused: u64,
    /// Ver [`Telemetry::microfone_entrega_silencio_digital`].
    silencio_digital: bool,
    bitrate_bps: u32,
    capture: Option<CaptureDevice>,
    playback: Option<PlaybackDevice>,
    aparelho: EstadoDoAparelho,
    trocas_de_aparelho: u64,
}

impl AudioState {
    /// A session with no voice path — before audio opens, and after it stops.
    ///
    /// **O mudo, o isolamento e o modo são os escolhidos**, e não os de uma voz
    /// recém-aberta: pela pendência 52, fora de sala não há voz, e o botão de
    /// mudo que a pessoa apertou ali tem de continuar apertado na tela. A sessão
    /// que ninguém vestiu continua nascendo em push-to-talk
    /// ([`ControlesGuardados::de_partida`]) — o modo que nunca dispara sozinho,
    /// e um nascimento em `Open` desenharia um microfone aberto numa sessão que
    /// não tem nenhum. O padrão de quem chega (a voz) entra depois, pelo disco:
    /// [`vestir_com_o_disco`].
    fn sem_voz(escolhida: &ControlesGuardados, abertura: AberturaDoAudio) -> Self {
        Self {
            available: false,
            abertura,
            mode: escolhida.modo.into(),
            speaking: false,
            muted: escolhida.mudo,
            total_isolation: escolhida.isolamento,
            input_level: 0.0,
            local_fault: false,
            frames_refused: 0,
            // Sem voz não há microfone aberto, e nada a dizer sobre ele.
            silencio_digital: false,
            bitrate_bps: 0,
            capture: None,
            playback: None,
            // Sem voz não há aparelho a acompanhar. `Perdido` seria uma falha
            // que não houve: ninguém pediu áudio nesta sessão.
            aparelho: EstadoDoAparelho::Funcionando,
            trocas_de_aparelho: 0,
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// A conversa do **canal aberto**, confirmadas e pendentes, na ordem de leitura.
///
/// # A projeção explícita que o R02 pede
///
/// Antes disto a FFI devolvia a lista inteira do core, e a lista inteira do core
/// era uma só para todos os canais: quem desenhava recebia mensagens de dois
/// canais sem nada que as separasse, e as desenhava todas sob o título de um. A
/// fronteira é aqui — a casca pede a conversa que está aberta, e é isso que ela
/// recebe.
///
/// As pendentes vão **depois** das confirmadas. Elas são sempre as mais novas: o
/// servidor ordena por id, e uma mensagem sem id ainda não tem lugar nessa
/// ordem. Ver [`seele_core::Pendente`].
fn messages_of(room: &Room) -> Vec<Message> {
    let Some(aberto) = room.current_channel else {
        return Vec::new();
    };
    let mut lista: Vec<Message> = room
        .mensagens_do_canal(aberto)
        .iter()
        .map(|message| Message {
            id: message.id.0,
            channel: message.channel.0,
            author: message.author.0,
            author_nickname: message.author_nickname.clone(),
            at_seconds: message.at_seconds,
            body: message.body.clone(),
            replies_to: message.replies_to.map(|pai| pai.0),
            own: message.own,
            edited: message.edited,
            mentions_me: message.mentions_me,
            estado: EstadoDoEnvio::Confirmada,
            // O eco da própria, para a casca trocar a pendente pela gravada no
            // mesmo nó (ver `seele_sessao::Message::client_message_id`).
            client_message_id: message
                .client_message_id
                .map(|chave| chave.get().to_string()),
            attachment: message.attachment.as_ref().map(|anexo| Attachment {
                id: anexo.id.get(),
                file_name: anexo.file_name.clone(),
                declared_type: anexo.declared_type.clone(),
                byte_size: anexo.byte_size,
                // Flattened to a boolean, and it is the whole state: the wire
                // enum has two variants and a third would be a state the design
                // does not have. ADR 0027 argues for exactly two — "this file
                // has not arrived yet" was deliberately never made a state, so
                // that it could not be confused with this one.
                expired: anexo.state == seele_core::AttachmentState::Expired,
            }),
        })
        .collect();

    // As pendentes desta máquina, nas mesmas linhas da conversa. Nas mesmas
    // linhas de propósito: uma área separada para «o que ainda não foi» tiraria
    // a mensagem do lugar onde ela vai ficar, e quem escreveu perderia de vista
    // onde ela entra.
    //
    // `id` zero e autor próprio: elas não têm id de servidor porque ainda não
    // existem para ele, e quem as identifica é a `client_message_id`. A casca
    // desenha por `estado`, nunca por `id`.
    let eu = room.me.map_or(0, |pessoa| pessoa.0);
    let meu_nome = room
        .me
        .and_then(|pessoa| room.people.get(&pessoa))
        .map_or_else(String::new, |pessoa| pessoa.nickname.clone());
    lista.extend(room.pendentes_do_canal(aberto).map(|pendente| Message {
        id: 0,
        channel: pendente.channel.0,
        author: eu,
        author_nickname: meu_nome.clone(),
        at_seconds: pendente.at_seconds,
        body: pendente.body.clone(),
        replies_to: pendente.replies_to.map(|pai| pai.0),
        own: true,
        edited: false,
        mentions_me: false,
        estado: pendente.estado.into(),
        client_message_id: Some(pendente.client_message_id.get().to_string()),
        attachment: None,
    }));
    lista
}

/// A resposta do núcleo, no vocabulário que atravessa a ponte.
///
/// Um `match` exaustivo de propósito, como [`nome_da_parada`]: uma resposta nova
/// em `seele-core` reprova aqui em vez de virar silenciosamente outra.
const fn permissao_daqui(resposta: seele_core::PermissaoDeTela) -> PermissaoDeTela {
    match resposta {
        seele_core::PermissaoDeTela::Concedida => PermissaoDeTela::Concedida,
        seele_core::PermissaoDeTela::Negada => PermissaoDeTela::Negada,
        seele_core::PermissaoDeTela::NaoPerguntada => PermissaoDeTela::NaoPerguntada,
        seele_core::PermissaoDeTela::SemCaptura => PermissaoDeTela::NaoSeSabe,
    }
}

/// O que a pessoa escolheu, traduzido para os degraus fechados do §5.
///
/// **Para baixo, sempre**: a escolha é teto, então um número entre dois degraus
/// vira o degrau de baixo, e um número abaixo do menor vira o menor — que é o
/// piso da lista e não uma quarta opção. Arredondar para cima seria a ponte
/// entregando mais do que a pessoa pediu, que é o §5 ao contrário.
fn limites_do_nucleo(limites: LimitesDeTela) -> seele_core::LimitesDeTela {
    let resolucao = seele_core::Resolucao::TODAS
        .into_iter()
        .rev()
        .find(|degrau| u32::try_from(degrau.altura()).unwrap_or(u32::MAX) <= limites.altura_maxima)
        .unwrap_or(seele_core::Resolucao::P540);
    let cadencia = seele_core::Cadencia::TODAS
        .into_iter()
        .rev()
        .find(|degrau| degrau.hz() <= limites.quadros_maximos)
        .unwrap_or(seele_core::Cadencia::Q8);
    let prioridade = match limites.prioridade {
        crate::types::Prioridade::Movimento => seele_core::tela::Prioridade::Movimento,
        crate::types::Prioridade::Nitidez => seele_core::tela::Prioridade::Nitidez,
    };
    seele_core::LimitesDeTela {
        banda_bps: limites.banda_bps,
        resolucao,
        cadencia,
        prioridade,
        com_som: limites.com_som,
    }
}

/// **O H.264 que a casca codifica**, na forma que o `VideoEncoder` pede:
/// Baseline, nível 4.0 — o `ScreenCodec::H264Baseline` que a abertura anuncia.
///
/// Mora aqui, e não na casca: quem decide o perfil é quem escreve a abertura,
/// e a casca não escreve perfil nenhum à mão. É o mesmo texto do
/// `seele_web::tela::CODEC_DO_NAVEGADOR`, e a conformidade
/// (`camera.rs`) prende um ao outro.
pub const CODEC_DA_CAMERA: &str = "avc1.42E028";

/// O teto de banda de uma câmera quando ninguém escolheu um: o da prova, o
/// mesmo da tela (`seele_core::TETO_DA_PROVA_BPS`).
pub const TETO_PADRAO_DA_CAMERA_BPS: u32 = seele_core::TETO_DA_PROVA_BPS;

/// O que o codificador da casca recebe para a câmera.
///
/// A mesma forma do que o `.wasm` dá à página do navegador
/// (`seele_web::tela::codificacao`), porque a casca é a mesma nos dois.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CodificacaoDaCamera {
    /// O codec, como o `VideoEncoder` o pede. Ver [`CODEC_DA_CAMERA`].
    pub codec: &'static str,
    /// Largura a codificar, em pixels, par.
    pub largura: u32,
    /// Altura a codificar, em pixels, par.
    pub altura: u32,
    /// Quadros por segundo.
    pub quadros: u32,
    /// O teto de banda de partida, em bits por segundo. O que vale depois é o
    /// que chega por [`Event::CameraBudget`].
    pub banda_bps: u32,
}

/// A conta de [`Connection::codificacao_da_camera`], fora da conexão para ser
/// conferida sem uma.
#[must_use]
pub fn codificacao_da_camera(
    largura: u32,
    altura: u32,
    limites: LimitesDeTela,
) -> CodificacaoDaCamera {
    let do_nucleo = limites_da_camera(limites);
    let fonte = (
        usize::try_from(largura).unwrap_or(0),
        usize::try_from(altura).unwrap_or(0),
    );
    let (mut l, mut a) = seele_core::medidas_para(do_nucleo.resolucao, fonte.0, fonte.1);
    // Nunca ampliar: uma câmera de 480 linhas vai do tamanho dela, com os dois
    // lados pares.
    if fonte.0 > 0 && fonte.1 > 0 && fonte.1 < a {
        (l, a) = ((fonte.0 / 2 * 2).max(2), (fonte.1 / 2 * 2).max(2));
    }
    CodificacaoDaCamera {
        codec: CODEC_DA_CAMERA,
        largura: u32::try_from(l).unwrap_or(u32::MAX),
        altura: u32::try_from(a).unwrap_or(u32::MAX),
        quadros: do_nucleo.cadencia.hz(),
        banda_bps: limites.banda_bps.unwrap_or(TETO_PADRAO_DA_CAMERA_BPS),
    }
}

/// As medidas de uma câmera, como o cabeçalho de abertura as aceita.
///
/// A régua é a do fio (`ScreenHeader::check`), perguntada aqui para que a
/// recusa seja uma resposta na mão de quem chamou, e não um fluxo que não abre
/// depois de a sala já ter ouvido que há uma transmissão.
fn medidas_da_camera(largura: u32, altura: u32) -> Result<(u16, u16), ConnectionError> {
    let (Ok(largura), Ok(altura)) = (u16::try_from(largura), u16::try_from(altura)) else {
        return Err(ConnectionError::CameraBadSize);
    };
    seele_core::ScreenHeader {
        version: seele_core::SCREEN_HEADER_VERSION,
        screen: seele_core::ScreenId(0),
        source: seele_core::ScreenSource::Camera,
        codec: seele_core::ScreenCodec::H264Baseline,
        width: largura,
        height: altura,
    }
    .check()
    .map_err(|_| ConnectionError::CameraBadSize)?;
    Ok((largura, altura))
}

/// Os limites de uma câmera, como o núcleo os lê.
///
/// **Movimento, e sem som**, qualquer que seja o que a casca mandou nesses
/// dois campos: uma câmera é um rosto que se mexe, e o som de quem fala já vai
/// pela voz. A resolução e a cadência são tetos como os da tela, e nunca acima
/// de 720p a 30 — é o que a casca codifica.
fn limites_da_camera(limites: LimitesDeTela) -> seele_core::LimitesDeTela {
    let mut do_nucleo = limites_do_nucleo(LimitesDeTela {
        altura_maxima: limites.altura_maxima.min(720),
        quadros_maximos: limites.quadros_maximos.min(30),
        ..limites
    });
    do_nucleo.prioridade = seele_core::tela::Prioridade::Movimento;
    do_nucleo.com_som = false;
    // **A câmera tem teto de banda, peça a casca o que pedir** — inclusive
    // quando ela não pede nenhum, que é o que a casca faz. Sem escolha, o teto
    // do fluxo é só o que a sonda mede, e ela sobe enquanto não vê dor: medido
    // em 10/11/2026 (`docs/medidas/2026-11-10-tela-do-produto-sob-perda.md`),
    // num caminho sem perda de 60 ms o orçamento chegou a 12,48 Mbps em 5 s, o
    // codificador o encheu, e quem assistia viu 55% do tempo congelado. O
    // codificador da tela para de gastar quando a imagem para; o de uma câmera
    // gasta o que lhe derem.
    let teto = teto_de_banda_da_camera(do_nucleo.resolucao);
    do_nucleo.banda_bps = Some(do_nucleo.banda_bps.map_or(teto, |pedida| pedida.min(teto)));
    do_nucleo
}

/// O máximo que uma câmera nesta resolução gasta, em bits por segundo.
///
/// **Derivado, e não escrito à mão**: é o que a tabela do núcleo diz que a
/// resolução precisa a 30 quadros (`bits_por_quadro` ×
/// `CADENCIA_DE_REFERENCIA`, 2,76 Mbps em 720p) — o limiar em que a imagem
/// está paga. Acima dele a câmera só compraria fila.
#[must_use]
pub const fn teto_de_banda_da_camera(resolucao: seele_core::Resolucao) -> u32 {
    seele_core::tela::bits_por_quadro(resolucao) * seele_core::tela::CADENCIA_DE_REFERENCIA
}

/// O base64 de ida e de volta mora no `seele-sessao` (`base64`) desde a
/// Tarefa 2.6 da F4: o navegador desfaz a mídia de um MOD pelo mesmo código.
pub use seele_core::base64::{base64_de, de_base64};

/// Onde o módulo do Cisco é procurado, na ordem.
///
/// A lista vem daqui porque `seele_video::modulo::procurar_em` a pede de fora, e
/// o argumento dele é o desta casa: *onde os arquivos do produto moram é decisão
/// da casca*. Esta é a casca.
///
/// A ordem é uma decisão. Primeiro o que quem roda apontou, que é como se
/// desenvolve e como se depura; depois a pasta do executável, que é onde um
/// pacote o carrega; depois `../Frameworks`, que é onde um `.app` do macOS o
/// põe; depois a pasta de build, que é onde ele cai numa árvore de fonte.
fn pastas_do_modulo() -> Vec<std::path::PathBuf> {
    let mut pastas = Vec::new();
    if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
        let caminho = std::path::PathBuf::from(apontado);
        pastas.push(if caminho.is_dir() {
            caminho
        } else {
            caminho
                .parent()
                .map_or_else(|| caminho.clone(), std::path::Path::to_path_buf)
        });
    }
    if let Some(perto) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    {
        pastas.push(perto.join("../Frameworks"));
        // A pasta de build de uma árvore de fonte: o executável mora em
        // `target/debug`, e o módulo em `target`.
        pastas.push(perto.join(".."));
        pastas.push(perto);
    }
    pastas
}

/// O que a casca precisa dizer a quem vai decidir se baixa o módulo.
///
/// Uma pessoa não consente com «baixar um componente»; ela consente com um
/// tamanho, uma origem e um motivo. Os três campos são isso, e nenhum deles é
/// texto de interface: a frase é da casca, e estes são os números que ela põe
/// dentro dela.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuloAOferecer {
    /// De onde vem, inteiro. É do Cisco, e quem lê tem direito de ver isso.
    pub url: String,
    /// Quantos bytes viajam.
    pub bytes: u64,
    /// Como o arquivo se chama depois de instalado.
    pub nome: String,
    /// Onde ele vai ficar. A pasta de configuração, não o pacote — é o que faz
    /// isto acontecer uma vez, e não a cada atualização.
    pub pasta: String,
}

/// O módulo que falta, se é que falta e se é que existe para este sistema.
///
/// `None` nos dois casos em que não há botão a oferecer: o módulo já está em
/// disco, ou este sistema não tem módulo publicado (Linux, e Mac Intel, por
/// enquanto). A casca não precisa distinguir os dois — nos dois ela não mostra
/// a caixa.
#[must_use]
pub fn modulo_de_video_a_baixar(pasta: &str) -> Option<ModuloAOferecer> {
    let pastas = pastas_do_modulo();
    if seele_core::procurar_modulo_de_video(&pastas).is_ok() {
        return None;
    }
    let publicado = seele_core::modulo_de_video_publicado()?;
    Some(ModuloAOferecer {
        url: publicado.url(),
        bytes: publicado.bytes_comprimido,
        nome: publicado.nome_em_disco.to_owned(),
        pasta: pasta.to_owned(),
    })
}

/// Instala o módulo a partir dos bytes comprimidos que a casca buscou.
///
/// **A rede não passa por aqui.** Quem busca é a casca gráfica, que já tem
/// cliente HTTP na árvore por causa do atualizador; somar um segundo aqui seria
/// pagar duas vezes pela mesma coisa. O que é desta camada é o que vem depois
/// dos bytes: conferir o hash fixado, expandir, gravar atômico.
///
/// # Errors
///
/// [`ConnectionError::ScreenModuleRefused`] se os bytes não são os fixados, se o bz2
/// não abre ou se a pasta não aceita o arquivo. Os três dizem a mesma coisa a
/// quem clicou — «não deu, tente de novo» — e o que separa os três está no log.
pub fn instalar_modulo_de_video(pasta: &str, comprimido: &[u8]) -> Result<String, ConnectionError> {
    seele_core::instalar_modulo_de_video(std::path::Path::new(pasta), comprimido)
        .map(|caminho| caminho.display().to_string())
        .map_err(|erro| {
            tracing::warn!(%erro, pasta, bytes = comprimido.len(), "o módulo de vídeo não instalou");
            ConnectionError::ScreenModuleRefused
        })
}

/// O módulo do Cisco, achado e carregado.
///
/// **O produto não vem com codec, e é a licença que impõe isso** — o módulo do
/// Cisco não pode ser redistribuído com este binário. Não achá-lo é o estado
/// normal de quem nunca compartilhou tela, e não um defeito.
fn modulo_de_video() -> Result<seele_core::BibliotecaDeVideo, ConnectionError> {
    seele_core::BibliotecaDeVideo::procurar_e_carregar(&pastas_do_modulo()).map_err(|erro| {
        let onde = seele_core::modulo_de_video_publicado()
            .map_or_else(|| "—".to_owned(), |modulo| modulo.url());
        tracing::warn!(%erro, %onde, "o módulo de vídeo não está nesta máquina");
        // **Este erro, e não `ScreenShareUnavailable`.** A captura está neste
        // app; o que falta é o módulo, que não vem no pacote por licença. A
        // frase daquele diz que o recurso não existe, e quem a lê para de
        // tentar — aconteceu duas vezes em campo, no macOS e no Windows.
        ConnectionError::ScreenModuleMissing
    })
}

/// O que a abertura de um fluxo declarou, na forma que atravessa para a casca.
///
/// Exaustivo de propósito: uma fonte nova no fio reprova aqui, em vez de
/// chegar à casca com o rótulo de outra.
const fn tipo_da_tela(origem: seele_core::ScreenSource) -> ScreenKind {
    match origem {
        seele_core::ScreenSource::Monitor => ScreenKind::Monitor,
        seele_core::ScreenSource::Window => ScreenKind::Window,
        seele_core::ScreenSource::Camera => ScreenKind::Camera,
    }
}

/// O evento que leva à casca um pedido da transmissão da própria câmera.
fn evento_da_camera(
    tela: seele_core::ScreenId,
    sinal: seele_core::de_fora::SinalParaFora,
) -> Event {
    use seele_core::de_fora::SinalParaFora;
    match sinal {
        SinalParaFora::Chave => Event::CameraKeyFrameRequested { screen: tela.0 },
        SinalParaFora::Banda { bps } => Event::CameraBudget {
            screen: tela.0,
            bps,
        },
        SinalParaFora::Parou(motivo) => Event::CameraPaused {
            screen: tela.0,
            reason: nome_da_parada(motivo),
        },
    }
}

/// O nome estável de um motivo de parada, para a casca escrever a frase.
///
/// Um `match` exaustivo de propósito: um motivo novo em `seele-core` reprova
/// aqui, em vez de virar silenciosamente o nome de outro.
fn nome_da_parada(motivo: seele_core::MotivoDeParada) -> &'static str {
    match motivo {
        seele_core::MotivoDeParada::SinalCritico => "SinalCritico",
        seele_core::MotivoDeParada::AbaixoDoPiso => "AbaixoDoPiso",
    }
}

/// Os nomes de parada que podem chegar a uma casca em [`TelaEmCurso::parada`].
///
/// Existe pelo mesmo motivo de [`caminhos`]: o guarda de cobertura de frase da
/// casca lê esta lista, e sem ela os nomes teriam de ser repetidos do outro lado
/// do ADR 0002 — que é o defeito que este ciclo já pagou uma vez.
///
/// À mão, e é uma diferença de [`caminhos`] que vale registrar:
/// `seele_core::MotivoDeParada` não publica um `TODOS`, então a lista é escrita
/// aqui e presa por um teste ao `match` exaustivo que a alimenta.
#[must_use]
pub fn motivos_de_parada_da_tela() -> Vec<&'static str> {
    [
        seele_core::MotivoDeParada::SinalCritico,
        seele_core::MotivoDeParada::AbaixoDoPiso,
    ]
    .into_iter()
    .map(nome_da_parada)
    .collect()
}

/// O nome estável de um motivo de queda, para a casca escrever a frase.
///
/// Exaustivo de propósito, como [`nome_da_parada`]: um motivo novo em
/// `seele-core` reprova aqui, em vez de chegar à casca com o nome de outro.
const fn nome_da_queda(motivo: seele_core::MotivoDeQueda) -> &'static str {
    match motivo {
        seele_core::MotivoDeQueda::Codificador => "Codificador",
        seele_core::MotivoDeQueda::Captura => "Captura",
        seele_core::MotivoDeQueda::Fluxo => "Fluxo",
    }
}

/// Os nomes de queda que podem chegar a uma casca em
/// [`Event::ScreenShareFell`].
///
/// Pelo mesmo motivo de [`motivos_de_parada_da_tela`]: o guarda de cobertura
/// de frase da casca lê esta lista. Derivada de `MotivoDeQueda::TODOS`, que
/// mora ao lado do enum — uma variante nova que não entre lá reprova no teste
/// que prende os dois.
#[must_use]
pub fn motivos_de_queda_da_tela() -> Vec<&'static str> {
    seele_core::MotivoDeQueda::TODOS
        .into_iter()
        .map(nome_da_queda)
        .collect()
}

/// O evento que diz à casca que a transmissão desta pessoa caiu.
fn evento_da_queda(
    tela: seele_core::ScreenId,
    origem: seele_core::ScreenSource,
    motivo: seele_core::MotivoDeQueda,
) -> Event {
    Event::ScreenShareFell {
        screen: tela.0,
        source: tipo_da_tela(origem),
        reason: nome_da_queda(motivo),
    }
}

/// Se o painel da tela precisa ser redesenhado.
///
/// Duas causas e não uma, e a segunda é o §5.1: a contagem de espectadores é um
/// campo de [`TelaEmCurso`] e ela anda quando o **roster** anda, sem nenhuma
/// mensagem de tela ter chegado. Sem ela, uma quinta pessoa entrando muda o que
/// está saindo e a tela continua escrevendo o número de antes.
///
/// `ha_tela` é lido **depois** de a mensagem ser dobrada, e por isso uma
/// transmissão que acabou de acabar ainda acende o evento: `changed.telas` é a
/// primeira parcela e não depende dele.
const fn a_tela_mudou(changed: seele_core::Changed, ha_tela: bool) -> bool {
    changed.telas || (changed.roster && ha_tela)
}

/// Maps a wire refusal onto this crate's own.
fn refusal_of(reason: seele_core::AttachmentRefusal) -> AttachmentRefusal {
    match reason {
        seele_core::AttachmentRefusal::NotAllowed => AttachmentRefusal::NotAllowed,
        seele_core::AttachmentRefusal::TooLarge { limit } => AttachmentRefusal::TooLarge { limit },
        seele_core::AttachmentRefusal::NoRoom => AttachmentRefusal::NoRoom,
        seele_core::AttachmentRefusal::SizeMismatch => AttachmentRefusal::SizeMismatch,
        seele_core::AttachmentRefusal::HashDidNotMatch => AttachmentRefusal::HashDidNotMatch,
        seele_core::AttachmentRefusal::RateLimited => AttachmentRefusal::RateLimited,
        seele_core::AttachmentRefusal::Unavailable => AttachmentRefusal::Unavailable,
        seele_core::AttachmentRefusal::NotFound => AttachmentRefusal::NotFound,
        seele_core::AttachmentRefusal::Expired => AttachmentRefusal::Expired,
        seele_core::AttachmentRefusal::Malformed => AttachmentRefusal::Malformed,
    }
}

/// O mesmo espelho, para a recusa de volume de um MOD — ADR 0048.
///
/// Exaustivo de propósito, como o de cima: uma variante nova no fio quebra a
/// compilação aqui em vez de virar um `_ => Outra` que a tela desenha errado.
fn recusa_de_volume(motivo: seele_core::VolumeRefusal) -> VolumeRefusal {
    use seele_core::VolumeRefusal as Fio;
    match motivo {
        Fio::SemEspera => VolumeRefusal::SemEspera,
        Fio::TipoRecusado => VolumeRefusal::TipoRecusado,
        Fio::NaoGravei => VolumeRefusal::NaoGravei,
        Fio::Incompleto => VolumeRefusal::Incompleto,
    }
}

/// Turns one step of a transfer into what a shell draws.
fn transfer_of(estado: &seele_core::enlace::Transferencia) -> Transfer {
    use seele_core::enlace::Transferencia;
    match estado {
        Transferencia::Subindo { id, feito, total } => Transfer::Sending {
            client_message_id: id.get(),
            done: *feito,
            total: *total,
        },
        Transferencia::Subiu { id } => Transfer::Sent {
            client_message_id: id.get(),
        },
        Transferencia::Recusada { id } => Transfer::Refused {
            client_message_id: id.get(),
        },
        Transferencia::Caiu { id } => Transfer::Fell {
            client_message_id: id.get(),
        },
        Transferencia::Baixando {
            anexo,
            feito,
            total,
        } => Transfer::Receiving {
            attachment: anexo.get(),
            done: *feito,
            total: *total,
        },
        Transferencia::Salvo { anexo, caminho } => Transfer::Saved {
            attachment: anexo.get(),
            path: caminho.display().to_string(),
        },
        // O nome e a pasta já passaram por `destino_de` antes de o pedido sair,
        // então o que sobra para o motor recusar é a gravação.
        Transferencia::NaoSalvou { anexo } => Transfer::NotSaved {
            attachment: anexo.get(),
            reason: NotSavedReason::Falhou,
        },
    }
}

fn link_state_byte(link_state: LinkTrust) -> u8 {
    match link_state {
        LinkTrust::Offline => 0,
        LinkTrust::Unverified => 1,
        LinkTrust::Verified => 2,
    }
}

fn link_state_from_byte(byte: u8) -> LinkTrust {
    match byte {
        1 => LinkTrust::Unverified,
        2 => LinkTrust::Verified,
        _ => LinkTrust::Offline,
    }
}

/// Resolves `host` or `host:port` into an address, a TLS label, and a pin key.
///
/// Three values because they are three things — see the same function in
/// the shell, and `TofuVerifier::new` for why keying the pin by the TLS label
/// was wrong.
/// The split is `seele_core::uri::separar` and not `rsplit_once(':')`: the port
/// separator and an IPv6's own separator are the same character, and doing it by
/// hand here made `[2001:db8::1]:8383` resolve to nothing. ADR 0022, step 2.
fn resolve(target: &str) -> Result<(SocketAddr, String, String), ConnectionError> {
    let alvo = seele_core::uri::separar(target).map_err(|_| ConnectionError::UnresolvableHost)?;
    let address = (alvo.maquina, alvo.porta)
        .to_socket_addrs()
        .map_err(|_| ConnectionError::UnresolvableHost)?
        .next()
        .ok_or(ConnectionError::UnresolvableHost)?;

    // TLS gets the name the M2 certificate carries; the pin gets the address,
    // which is what actually tells one server from another.
    let server_name = if alvo.maquina.parse::<std::net::IpAddr>().is_ok() {
        "localhost".to_owned()
    } else {
        alvo.maquina.to_owned()
    };
    // The pin key is written back in the canonical form rather than as typed, so
    // `[::1]:8383` and `[::1]` file under one entry instead of two.
    //
    // **Uma função só, e por quê.** Esta chave não arquiva mais só o pin: desde
    // o anúncio de MODs ela arquiva também o aceite, e quem grava o aceite é a
    // casca, por um verbo que não tem socket nenhum. Duas cópias da regra
    // seriam duas chaves para o mesmo servidor no dia em que uma delas mudasse
    // — e o efeito de errar a chave é mudo: o aceite fica escrito onde ninguém
    // lê, e a pessoa que já disse sim é perguntada de novo para sempre.
    let pin_key = chave_do_servidor(target).ok_or(ConnectionError::UnresolvableHost)?;
    debug_assert_eq!(
        pin_key.contains('['),
        address.is_ipv6() && alvo.maquina.contains(':'),
        "a chave canônica discordou da resolução sobre ser um IPv6 literal"
    );
    Ok((address, server_name, pin_key))
}

/// A chave sob a qual tudo que é **deste servidor** é arquivado nesta máquina.
///
/// O pin do certificado (ADR 0022) e o aceite dos MODs (ADR 0045) moram os dois
/// sob ela, e é de propósito: são duas memórias sobre o mesmo servidor, e uma
/// pessoa que apaga um servidor da lista espera apagar as duas.
///
/// É `host:porta` na forma canônica — a porta preenchida quando quem digitou a
/// omitiu, e um IPv6 literal de volta entre colchetes, sem os quais a chave
/// seria ambígua com `host:porta`. **Não resolve nome nenhum:** é uma decisão
/// sobre o texto, e por isso vale igual dentro de um comando da janela, onde
/// não há socket.
///
/// Devolve `None` quando o alvo não é um endereço — o mesmo caso em que
/// `resolve` devolve [`ConnectionError::UnresolvableHost`].
#[must_use]
pub fn chave_do_servidor(alvo: &str) -> Option<String> {
    let alvo = seele_core::uri::separar(alvo).ok()?;
    // `maquina` só contém `:` quando veio de dentro de colchetes: `separar`
    // recusa um IPv6 escrito cru e corta a porta em qualquer outro caso. Então
    // este teste diz «é um IPv6 literal» sem precisar resolver o nome.
    if alvo.maquina.contains(':') {
        Some(format!("[{}]:{}", alvo.maquina, alvo.porta))
    } else {
        Some(format!("{}:{}", alvo.maquina, alvo.porta))
    }
}

/// Turns a [`ConnectConfig`] and what [`resolve`] made of it into a
/// [`seele_core::enlace::Destino`].
///
/// Pulled apart from `drive` for the same reason `seele_core::enlace` pulls
/// `conferir` apart from `Enlace::conectar`: everything here is a decision on
/// values, and a decision on values does not need a QUIC socket to test.
/// Without this split, whether `expected_fingerprint` actually reaches
/// `impressao_esperada` could only be checked by a live handshake — which is
/// exactly the gap `every_expected_fingerprint_reaches_the_destino_it_promised`
/// below closes.
/// O endereço de hoje que o quarto deu, como candidato desta chegada.
///
/// Montado como os outros, por [`resolve`] e [`build_destino`], e é isso que faz
/// a `Chegada` reconhecer um endereço que já corre: ela compara pela chave de
/// pino, e a chave de pino sai daqui pela mesma [`chave_do_servidor`] que deu a
/// do alvo e a dos alternativos. `192.168.50.30` e `192.168.50.30:8383` têm a
/// mesma.
///
/// A chave dele não é a do alvo (a chave do aceite), então o pino dele não
/// prova o servidor: é a regra para um candidato que ninguém escolheu.
fn candidato_do_quarto(
    config: &ConnectConfig,
    endereco: SocketAddr,
    chave_do_aceite: &str,
) -> seele_core::enlace::Destino {
    // Um endereço já resolvido sempre resolve; a reserva só existe para o tipo,
    // e escreve o endereço como a chave.
    let (address, server_name, pin_key) = resolve(&endereco.to_string())
        .unwrap_or_else(|_| (endereco, "localhost".to_owned(), endereco.to_string()));
    build_destino(config, address, &server_name, &pin_key, chave_do_aceite)
}

/// O que cada destino desta conexão confere de quem atende: o código, quando
/// há; senão a impressão; senão nada. **A decisão mora aqui**, e
/// [`build_destino`] e [`quarto_de`] a leem: o quarto é perguntado pela mesma
/// impressão que o TLS confere, e por nenhuma quando o que vale é o código.
fn esperada_de(config: &ConnectConfig) -> Option<seele_core::enlace::Esperada> {
    match (config.codigo_esperado, &config.expected_fingerprint) {
        (Some(codigo), _) => Some(seele_core::enlace::Esperada::Codigo(codigo)),
        (None, Some(impressao)) => {
            Some(seele_core::enlace::Esperada::Certificado(impressao.clone()))
        }
        (None, None) => None,
    }
}

/// O quarto desta volta, quando a conexão confere uma impressão.
///
/// O endereço de hoje é montado como os outros candidatos, por
/// [`candidato_do_quarto`]: a chave de pino dele não é a do alvo, então o pino
/// não prova o servidor, que é a regra para o endereço que o quarto devolveu.
fn quarto_de(config: &ConnectConfig, chave_do_aceite: &str) -> Option<seele_core::chegada::Quarto> {
    match esperada_de(config) {
        Some(seele_core::enlace::Esperada::Certificado(impressao)) => {
            let config_do_quarto = config.clone();
            let aceite = chave_do_aceite.to_owned();
            Some(seele_core::chegada::Quarto::novo(
                impressao,
                move |endereco| candidato_do_quarto(&config_do_quarto, endereco, &aceite),
            ))
        }
        Some(seele_core::enlace::Esperada::Codigo(_)) | None => None,
    }
}

fn build_destino(
    config: &ConnectConfig,
    address: SocketAddr,
    server_name: &str,
    pin_key: &str,
    chave_do_aceite: &str,
) -> seele_core::enlace::Destino {
    seele_core::enlace::Destino {
        servidor: address,
        nome_tls: server_name.to_owned(),
        chave_do_pin: pin_key.to_owned(),
        apelido: config.nickname.clone(),
        segredo: config.join_secret.clone(),
        esperada: esperada_de(config),
        // **O pino deste destino prova o servidor só no alvo, e só num alvo de
        // escopo público.** A regra mora aqui, e em nenhum outro lugar: ver
        // `seele_core::enlace::Destino::o_pino_prova_o_servidor`.
        //
        // O alvo é o candidato cuja chave de pino é a do destino. A chave do
        // aceite é a do endereço que a pessoa escolheu (o do link, o digitado,
        // o da entrada da lista), e `drive` a passa igual a todos os
        // candidatos; a chave de pino é a de cada um. Só o primeiro destino de
        // `drive` tem as duas iguais, e um alternativo que as tenha é o mesmo
        // `host:porta` escrito de novo.
        //
        // O escopo é o do endereço **resolvido**, e não o do texto: um nome como
        // `casa.local` decide pelo endereço a que chegou. Um endereço de escopo
        // local (privado, link-local, CGNAT, ULA) é o mesmo de uma casa para
        // outra, e o pin dele prova só quem atende ali na rede em que a pessoa
        // está. O loopback conta como público: é esta máquina em qualquer rede.
        o_pino_prova_o_servidor: pin_key == chave_do_aceite
            && !seele_core::enlace::e_privado(address.ip()),
        // Lido de disco aqui, e não pedido à casca: o `home` é o mesmo caminho
        // de onde saem a identidade e os pins (ADR 0017), e quem sabe persistir
        // é o núcleo. Uma casca que tivesse de carregar o aceite junto seria uma
        // casca que pode esquecer de carregá-lo — e esquecer, aqui, é perguntar
        // de novo a quem já respondeu.
        // **A chave do aceite é a do destino, e não a do candidato.**
        //
        // O pin é por endereço porque é isso que ele prova: *este* endereço
        // apresentou *esta* chave. O aceite responde outra pergunta — «eu
        // autorizei rodar estes MODs neste servidor» —, e o servidor é um só,
        // por quantos caminhos se chegue a ele.
        //
        // Aqui as duas usavam a mesma chave, e o resultado era um laço que
        // ninguém conseguia romper: a pessoa aceitava, o sim era gravado sob o
        // endereço que ela digitou, a conexão entrava pelo candidato de
        // retransmissão — cuja porta o NAT troca a cada sessão —, e o aceite
        // procurado sob *essa* porta nunca existia. Aceitar de novo gravava sob
        // o mesmo endereço digitado e falhava de novo, para sempre.
        //
        // Visto em produção, com os dados na mão: `aceites` tinha
        // `192.168.50.30:8383 → acff8e16…`, exatamente o conjunto exigido, e a
        // conexão que respondia era `187.255.97.152:9368`. O arquivo de pins
        // tinha nove portas diferentes do mesmo endereço público, uma por
        // sessão — o rastro do mesmo problema.
        //
        // Todos os candidatos de uma chegada são o mesmo servidor: eles vêm do
        // mesmo convite e são conferidos contra a mesma impressão digital. O
        // consentimento é daquele servidor, e acompanha a chegada inteira.
        aceito: seele_core::aceites::Aceites::em(std::path::Path::new(&config.home))
            .aceito_de(chave_do_aceite),
    }
}

/// Para onde uma conexão vai.
enum Alvo {
    /// Os endereços de um servidor, o primeiro e os alternativos, já resolvidos.
    Enderecos {
        /// O primeiro endereço.
        address: SocketAddr,
        /// O nome que o TLS confere.
        server_name: String,
        /// A chave do pin e do aceite, na forma canônica.
        pin_key: String,
        /// Os outros endereços do convite.
        alternates: Vec<(SocketAddr, String, String)>,
    },
    /// O código de uma pessoa (tarefa 30 da F7): quem diz onde a sala mora é o
    /// ponto de encontro.
    Codigo(Codigo),
}

/// The driver: connects, then pumps until told to stop.
#[allow(
    clippy::too_many_arguments,
    reason = "one private function, called once, with everything the thread owns"
)]
async fn drive(
    config: ConnectConfig,
    alvo: Alvo,
    key: seele_core::SigningKey,
    pins: Arc<FilePinStore>,
    shared: Arc<Shared>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    ready: &std::sync::mpsc::Sender<Result<Trust, ConnectFailure>>,
    olhos: Option<Arc<dyn EventListener>>,
    cancelamento: Option<Arc<Cancelamento>>,
) {
    shared
        .link_state
        .store(link_state_byte(LinkTrust::Unverified), Ordering::Relaxed);

    // `Enlace` e não `Client`: é a sessão que atravessa quedas, com a bateria
    // interna dentro. Antes disto, o app pulava de "conectado" para "encerrado"
    // no primeiro soluço de rede.
    let chegada = match alvo {
        // **Uma ligação** (tarefa 30 da F7): a `Chegada` pergunta ao ponto de
        // encontro desta máquina onde a sala do código mora, e os candidatos
        // saem da resposta, todos conferindo o código dentro do TLS. O ponto é o
        // de quem liga, a mesma leitura do ambiente com que a sala se registra.
        Alvo::Codigo(codigo) => seele_core::chegada::Chegada::para_o_codigo(
            codigo,
            ponto_de_quem_liga_desta_casa(&config.home),
            config.nickname.clone(),
        ),
        Alvo::Enderecos {
            address,
            server_name,
            pin_key,
            alternates,
        } => {
            // O alvo como a pessoa o digitou. É sob ele que a casca grava o
            // sim, e é ele que vale para todos os candidatos desta chegada.
            let chave_do_aceite = pin_key.clone();
            let destinos = std::iter::once((address, server_name.clone(), pin_key.clone()))
                .chain(alternates)
                .map(|(address, server_name, pin_key)| {
                    build_destino(&config, address, &server_name, &pin_key, &chave_do_aceite)
                })
                .collect();
            // Uma `Chegada` e não a chamada direta ao laço: ela é quem dá nome
            // a cada etapa desta travessia e guarda a trilha. O laço continua
            // exatamente onde estava — com o aviso por candidato e os prazos —,
            // e o que muda aqui é que a falha deixa de ser uma frase só. O
            // bilhete vai junto: com ele, quem entra avisa o ponto de encontro
            // por candidato que precisa de furo, que é o degrau 4 do ADR 0022.
            // Sem bilhete os dois caminhos são o mesmo.
            let chegada = seele_core::chegada::Chegada::nova(destinos, config.bilhete.clone());
            // **O quarto corre junto com a corrida** (a pendência 55, tarefa
            // 14b da F7). Era o `connect` do app que perguntava, antes de
            // chamar esta ponte, e quem estava na mesma casa pagava até 1,5 s
            // de prazo do quarto com o ponto ou o DNS mudos. Agora a `Chegada`
            // pergunta enquanto corre: quem não precisa de furo começa na hora,
            // e o endereço de hoje entra quando chega.
            //
            // Com a impressão conferida, e o bilhete guardado (a `Chegada` só
            // pergunta com bilhete): é a mesma condição em que o `connect`
            // perguntava. Ver `quarto_de`.
            match quarto_de(&config, &chave_do_aceite) {
                Some(quarto) => chegada.com_o_quarto(quarto),
                None => chegada,
            }
        }
    };

    // O ouvinte entra **antes** do bloqueio, que é a coisa inteira: o `watch` da
    // chegada existe desde a tarefa 8 e não tinha um só leitor em produção,
    // porque toda inscrição acontecia depois de a travessia ter terminado.
    //
    // Uma tarefa e não um `select!` no laço abaixo: o laço só existe depois de
    // haver sessão, e estas etapas todas acontecem antes disso.
    let vigia = olhos.map(|ouvinte| {
        let mut etapas = chegada.acompanhar();
        // Carimbadas também (E3): a geração é 1 durante toda a chegada, porque
        // nenhum enlace abriu ainda para poder voltar.
        let carimbo = shared.carimbo();
        tokio::spawn(async move {
            while etapas.changed().await.is_ok() {
                let etapa = ConnectStage::from(&*etapas.borrow_and_update());
                ouvinte.on_event_carimbado(carimbo, Event::ConnectStageChanged { stage: etapa });
            }
        })
    });

    let chegou = chegada
        .chegar_ate(key, pins, async move {
            match cancelamento {
                Some(cancelamento) => cancelamento.pedido().await,
                None => std::future::pending().await,
            }
        })
        .await;
    // Esperada, e não abortada. A `Chegada` já morreu quando esta linha roda —
    // `chegar` a consome —, então o emissor do `watch` já caiu e a tarefa acima
    // termina sozinha depois de entregar a última etapa. Abortar aqui perderia
    // exatamente essa última, que é a que diz como a travessia acabou.
    if let Some(vigia) = vigia {
        let _ = vigia.await;
    }

    let chegado = match chegou {
        Ok(chegado) => chegado,
        Err(falha) => {
            tracing::warn!(motivo = %falha, "could not reach the server");
            registrar_trilha(falha.trilha());
            let _ = ready.send(Err(ConnectFailure {
                error: classify_connect_failure(falha.motivo()),
                trail: falha.trilha().iter().map(step_of).collect(),
            }));
            return;
        }
    };

    // Por qual caminho esta conversa saiu, gravado antes de a casca poder
    // perguntar: quem chama `snapshot()` no instante seguinte ao `connect` já
    // encontra o nome, e não um `None` que vira nome no quadro seguinte.
    //
    // A trilha inteira vai para o log junto, e não só na falha: «venceu o
    // terceiro candidato, público, com aviso» é a linha que responde por que a
    // tela diz `FuroDeNat`, e sem ela o nome na tela não teria como ser
    // conferido contra nada.
    registrar_trilha(&chegado.trilha);
    shared.gravar_caminho(chegado.caminho());
    let mut client = chegado.enlace;
    shared.gravar_impressao(client.pin_decision());
    // O protocolo que o `Hello` negociou: é com ele que `Connection` decide se
    // um verbo do perfil (protocolo 12) pode sair.
    shared.perfis.guardar_o_protocolo(client.versao_negociada());

    // `Enlace::conectar` already returned `Err` above for anything that would
    // have made this a `PinDecision::Changed` or a refused invite — see
    // `seele_core::tofu::verdict`'s own note that `Changed` never reaches a
    // caller as a verdict. What is left here is a verdict the shell is allowed
    // to see, not one that had to be turned back into a connection error.
    let trust = Trust::from(client.veredito().clone());

    if let Ok(mut room) = shared.room.lock() {
        room.adopt(client.sessao(), &config.nickname);
    }
    shared
        .link_state
        .store(link_state_byte(LinkTrust::Verified), Ordering::Relaxed);

    remember_media(&shared, client.media(), client.sessao().ssrc);

    // **O áudio não abre aqui.** Pendência 52: ele abre quando a pessoa passa a
    // estar numa sala de voz, e fecha quando ela sai — quem fica no servidor só
    // pela conversa por escrito devolve o repouso à máquina. Este primeiro olhar
    // é o do aperto de mão: o servidor adotado, ninguém sentado, nada a abrir.
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&shared.voice, sala_de(&shared), |pedidos| {
        abrir_a_voz(&shared, pedidos)
    });

    shared.running.store(true, Ordering::Relaxed);
    let _ = ready.send(Ok(trust));

    let mut sync = Signal::new();
    let mut next_tick = Instant::now() + TICK;
    // O que esta ponta mede da voz que ouve, dito ao servidor uma vez por
    // segundo (protocolo 13): ver `relatar_a_recepcao`.
    let mut relator = seele_core::recepcao::Relator::novo(Instant::now());

    // **A volta do enlace** (lote P61): a busca do buraco do canal aberto, e o
    // que ficou sem resposta saindo de novo com a mesma chave. Ver `volta`.
    let mut buraco: Option<volta::Buraco> = None;
    let mut carimbos_do_buraco: u64 = 1;
    let mut repeticao = volta::Repeticao::default();

    while shared.running.load(Ordering::Relaxed) {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                if !run_command(&client, &shared, command).await {
                    break;
                }
            }

            aviso = client.proximo() => {
                match aviso {
                    seele_core::enlace::Aviso::Mensagem(message) => {
                        receber_a_mensagem_do_motor(&shared, &message);
                        // A razão de uma recusa, que o `Room` guardou ao dobrar
                        // a mensagem. Drenada em vez de lida: duas
                        // transferências podem estar no ar, e a segunda falhar
                        // não pode apagar o motivo da primeira.
                        for aviso in shared.drain_transfers() {
                            shared.notify(&Event::TransferChanged { transfer: aviso });
                        }
                        // A página do buraco que estava no ar acabou? Depois do
                        // `fold`: a mensagem já está no `Room` quando a busca
                        // decide. Ver `volta::Buraco`.
                        let passo = buraco
                            .as_mut()
                            .and_then(|aberto| aberto.ouvir(&message, Instant::now()));
                        if let Some(passo) = passo {
                            volta::seguir(&client, &mut buraco, &mut carimbos_do_buraco, passo)
                                .await;
                        }
                    }
                    seele_core::enlace::Aviso::Transferencia(estado) => {
                        shared.notify(&Event::TransferChanged {
                            transfer: transfer_of(&estado),
                        });
                    }
                    seele_core::enlace::Aviso::TelaAbriu {
                        tela,
                        largura,
                        altura,
                        origem,
                        leitura,
                        codificador,
                    } => {
                        // **Gravada antes do evento**: a casca que reage ao
                        // `ScreenOpened` relendo o retrato já acha a tela nele.
                        shared.tela_abriu(tela, leitura);
                        // E se era a própria, pausada pelo teto, a pausa
                        // acabou: a volta é esta abertura, da mesma tela.
                        shared.a_tela_voltou(tela);
                        // E o codificador junto, pelo mesmo motivo. Só a
                        // transmissão desta máquina traz nome; a de outra
                        // pessoa não mexe no que está guardado.
                        if let Some(nome) = codificador {
                            shared.gravar_codificador_da_tela(Some((tela, nome)));
                        }
                        shared.notify(&Event::ScreenOpened {
                            screen: tela.0,
                            width: largura,
                            height: altura,
                            source: tipo_da_tela(origem),
                        });
                    }
                    // O que a transmissão da própria câmera pede a quem a
                    // codifica — a casca. Um evento por pedido, com nome.
                    seele_core::enlace::Aviso::Camera { tela, sinal } => {
                        shared.notify(&evento_da_camera(tela, sinal));
                    }
                    seele_core::enlace::Aviso::TelaIlegivel { motivo } => {
                        shared.notify(&Event::ScreenUnreadable { reason: motivo });
                    }
                    // O teto pausou a transmissão desta pessoa. Vai ao retrato
                    // (`TelaEmCurso::parada`), que é de onde a casca já lia a
                    // frase — e que até aqui era sempre `None`. O evento é o
                    // grosso de sempre: «a tela mudou, releia o retrato».
                    //
                    // A câmera recebe também o `CameraPaused` dela, por
                    // `Aviso::Camera`: aquele manda a janela parar de
                    // codificar, e este escreve a frase no palco.
                    seele_core::enlace::Aviso::TelaParou { tela, motivo, .. } => {
                        if shared.a_tela_parou(tela, motivo) {
                            shared.notify(&Event::ScreenChanged);
                        }
                    }
                    // A transmissão desta pessoa caiu, e o núcleo já a
                    // encerrou. O retrato se acerta sozinho — o
                    // `ScreenShareStopped` que o servidor devolve tira a
                    // transmissão dele e esquece o pedido (`fold`) —, e o que
                    // falta à casca é o que ele não diz: que não foi a pessoa
                    // quem parou, e por quê.
                    seele_core::enlace::Aviso::TelaCaiu {
                        tela,
                        origem,
                        motivo,
                    } => {
                        // Caiu: não está mais pausada, está acabada.
                        shared.esquecer_a_parada_da_tela();
                        shared.notify(&evento_da_queda(tela, origem, motivo));
                    }
                    seele_core::enlace::Aviso::TelaQuadro {
                        tela,
                        chave,
                        bytes,
                        leitura,
                    } => {
                        shared.quadro_da_tela(tela, leitura, chave, &bytes);
                    }
                    // **O som não vira evento.** Ele vai direto para a mistura,
                    // que é onde o codec já mora e onde o isolamento total
                    // decide se alguma coisa toca. Mandá-lo à casca seria pedir
                    // a ela que decodificasse Opus para depois devolvê-lo.
                    seele_core::enlace::Aviso::TelaSom {
                        tela,
                        bytes,
                        leitura,
                    } => {
                        // **O identificador segue junto até a mistura.** Ele era
                        // descartado aqui — `{ bytes, .. }` —, e por isso a fila
                        // não sabia de quem ela era: qualquer `TelaFechou`
                        // limpava a fila, então alguém parando de transmitir
                        // calava o som de outra transmissão. R17 e ADR 0054.
                        //
                        // E só o da leitura vigente: o som de uma leitura já
                        // substituída da mesma tela cairia na fila da nova.
                        if !shared.som_da_leitura_vale(tela, leitura) {
                            tracing::debug!(
                                %tela,
                                leitura,
                                "som de uma leitura de tela substituída: descartado"
                            );
                        } else if let Ok(voice) = shared.voice.lock() {
                            if let Some(voice) = voice.as_ref() {
                                voice.som_da_tela(tela, bytes);
                            }
                        }
                    }
                    seele_core::enlace::Aviso::TelaFechou { tela, leitura } => {
                        // **Só o fim da leitura vigente fecha.** O de uma leitura
                        // que outra da mesma tela já substituiu (a troca de par
                        // aborta a anterior, e o fim dela sai do `Drop`) pode
                        // chegar depois da abertura nova, e apagaria o palco e a
                        // fila de som dela.
                        if shared.tela_fechou(tela, leitura) {
                            // A transmissão acabou: o que sobrou na fila é som de
                            // uma tela que já não está na frente de ninguém — **se a
                            // fila for daquela tela.** Ver
                            // `Voice::esquecer_o_som_da_tela`.
                            if let Ok(voice) = shared.voice.lock() {
                                if let Some(voice) = voice.as_ref() {
                                    voice.esquecer_o_som_da_tela(tela);
                                }
                            }
                            shared.notify(&Event::ScreenClosed { screen: tela.0 });
                        } else {
                            tracing::info!(
                                %tela,
                                leitura,
                                "fim de uma leitura de tela já substituída: nada a fechar"
                            );
                        }
                    }
                    // Cair não encerra: começa a bateria interna, e a casca
                    // esmaece em vez de fechar.
                    seele_core::enlace::Aviso::Estado { estado, restante } => {
                        shared.gravar_enlace(estado, restante);
                        shared.notify(&Event::TelemetryChanged);
                        // **O que esta máquina escreveu e ainda esperava passa a
                        // «sem resposta».** O enlace caiu no meio, então o
                        // servidor pode ter gravado e a confirmação ter se
                        // perdido — é por isso que o estado não é «falhou».
                        //
                        // Sem esta linha, uma mensagem escrita no instante da
                        // queda ficaria «enviando» para sempre, o que é o mesmo
                        // defeito do R05 com outra cara: a interface parada num
                        // estado que nunca sai dele.
                        if matches!(estado, seele_core::Link::InternalBattery { .. }) {
                            let mexeu = shared
                                .room
                                .lock()
                                .map(|mut room| room.envios_sem_resposta())
                                .unwrap_or(false);
                            if mexeu {
                                shared.messages_changed();
                            }
                            // A busca e a repetição eram da conexão que caiu: a
                            // próxima volta as refaz a partir do `Room`.
                            if let Some(largado) = buraco.take() {
                                carimbos_do_buraco = largado.sequencia();
                            }
                            repeticao.esquecer();
                        }
                    }
                    seele_core::enlace::Aviso::Reconectado { media, sessao, voice_room } => {
                        // **A geração sobe antes de qualquer evento desta volta**
                        // (E3): o `Online` abaixo já sai com o carimbo novo, e um
                        // comando com o de antes da queda passa a ser recusado
                        // por `Connection::com`.
                        let geracao = shared.nova_geracao();
                        tracing::info!(geracao, "o enlace voltou: geração nova do carimbo");
                        shared.tela_ligada_nova_geracao();
                        shared.gravar_enlace(seele_core::Link::Online, None);
                        // A volta pode ter caído num servidor de outra versão.
                        shared.perfis.guardar_o_protocolo(client.versao_negociada());
                        // A pessoa volta sentada onde o motor a sentou de novo:
                        // ver `readotar_na_volta`. É esse assento que a porta da
                        // voz olha no fim desta volta do laço.
                        if let Ok(mut room) = shared.room.lock() {
                            readotar_na_volta(&mut room, &sessao, &config.nickname, voice_room);
                        }
                        // Conexão nova, `ssrc` novo, canal de mídia novo: sem
                        // reabrir, a voz sairia por uma conexão morta.
                        remember_media(&shared, (*media).clone(), sessao.ssrc);
                        // `reopen` e não `start_on`, porque os controles
                        // têm que atravessar a reabertura. A lista mora
                        // em `Voice::switch_capture` — mudo,
                        // Isolamento total, o modo, a tecla segura, cada
                        // ganho por interlocutor — e está lá justamente
                        // para que nenhuma casca esqueça um item. Esta
                        // esquecia todos.
                        //
                        // O que torna isto pior que "volta desmutado" é
                        // que `Enlace::tentar` **restaura** o mudo
                        // no servidor: o roster continuava mostrando a
                        // pessoa muda enquanto o portão local voltava
                        // aberto, e o indicador que todo mundo lê
                        // passava a mentir.
                        //
                        // `reopen` também é quem sabe **o que** reabrir:
                        // a voz guarda os dois dispositivos que pediu, e
                        // volta pedindo os mesmos. Sem isso, voltar do ar
                        // trocaria o microfone e a saída de quem está no
                        // meio de uma conversa — e trocaria calado, que é
                        // a pior parte. O recuo por lado é o mesmo do
                        // caminho de conexão: uma interface que sumiu
                        // enquanto estávamos fora do ar não pode custar a
                        // voz do resto da sessão.
                        //
                        // **Na thread de `esperar_o_audio`, com o prazo, e sem
                        // o cadeado da voz preso** (pendência 52): a voz velha
                        // vai inteira para a reabertura, e a nova chega pelo
                        // braço da porta abaixo. Quem estava fora de sala não
                        // tem voz, e continua sem aparelho.
                        let (canal, ssrc) = ((*media).clone(), sessao.ssrc);
                        let vez = shared.voice.vez();
                        porta.na_volta(
                            &shared.voice,
                            move |atual: Voice| -> Futuro<Voice> {
                                Box::pin(async move {
                                    esperar_o_audio(
                                        &vez,
                                        move || atual.reopen(canal, ssrc).map_err(|erro| format!("{erro:#}")),
                                        PRAZO_DO_AUDIO,
                                    )
                                    .await
                                })
                            },
                            |pedidos| abrir_a_voz(&shared, pedidos),
                        );
                        // **O buraco e o que ficou sem resposta** (lote P61,
                        // pendência 61). O núcleo só reassina o canal; o que
                        // passou nele durante a queda, e o que esta máquina
                        // mandava no instante, não voltavam sozinhos. A busca
                        // pede o canal aberto de trás para a frente até a última
                        // mensagem conhecida, e a repetição manda de novo, com a
                        // **mesma chave**, cada pendente que ficou sem resposta,
                        // na ordem em que foi escrita. Ver `volta`.
                        if let Some(velho) = buraco.take() {
                            carimbos_do_buraco = velho.sequencia();
                        }
                        let pagina = match shared.room.lock() {
                            Ok(room) => {
                                repeticao.na_volta(&room, Instant::now());
                                volta::Buraco::abrir(&room, carimbos_do_buraco, Instant::now())
                            }
                            Err(_) => None,
                        }
                        .map(|(aberto, pagina)| {
                            buraco = Some(aberto);
                            pagina
                        });
                        if let Some(pagina) = pagina {
                            let _ = client
                                .historico_com_cerca(
                                    pagina.canal,
                                    pagina.cursor,
                                    pagina.limite,
                                    pagina.carimbo,
                                )
                                .await;
                        }
                        shared.notify(&Event::TelemetryChanged);
                    }
                    // **O relatório de um túnel de jogo vai à casca inteiro**,
                    // e o último de cada túnel leva o motivo de ele ter
                    // acabado: sem isto a faixa do produto não teria como saber
                    // que o fluxo de relatórios parou.
                    seele_core::enlace::Aviso::Tunel(relatorio) => {
                        let relatorio = relatorio_para_a_casca(&shared, &relatorio);
                        guardar_relatorio_de_tunel(&shared, &relatorio);
                        shared.notify(&Event::TunnelReport { report: relatorio });
                    }
                    // **A máquina de jogo** (API 8 parte 3). O núcleo já
                    // calculou o resumo e já tirou os repetidos; aqui só se
                    // achata para a casca.
                    seele_core::enlace::Aviso::PedidoDeMaquina {
                        mod_id,
                        hash,
                        servidor,
                        pedido_por,
                        oferta,
                        papel,
                        resumo,
                    } => {
                        shared.notify(&Event::MachineRequest {
                            mod_id,
                            hash,
                            servidor,
                            pedido_por: pedido_por.0,
                            oferta: Box::new(OfertaDeMaquinaFfi::from(&*oferta)),
                            papel: papel.nome().to_owned(),
                            resumo,
                        });
                    }
                    seele_core::enlace::Aviso::AvisoParaMaquina {
                        mod_id,
                        servidor,
                        carga,
                    } => {
                        shared.notify(&Event::MachineNotice {
                            mod_id,
                            servidor,
                            carga,
                        });
                    }
                    seele_core::enlace::Aviso::MaquinaDispensada {
                        mod_id,
                        servidor,
                        motivo,
                    } => {
                        shared.notify(&Event::MachineDismissed {
                            mod_id,
                            servidor,
                            motivo: seele_core::maquina::nome_da_dispensa(&motivo),
                        });
                    }
                    seele_core::enlace::Aviso::Encerrado(motivo) => {
                        shared.notify(&Event::Ended {
                            reason: fim_da_sessao(&motivo),
                        });
                        break;
                    }
                }
            }

            // **O relógio da busca do buraco** (I2 da revisão do P61): a página
            // vazia que espera para ser pedida de novo, ou o prazo da cerca da
            // página no ar. Sem ele, uma cerca descartada pelo limitador do
            // servidor deixava a busca aberta e calada até a próxima queda.
            () = volta::esperar_ate(buraco.as_ref().and_then(volta::Buraco::quando)),
                if buraco.as_ref().and_then(volta::Buraco::quando).is_some() => {
                let passo = buraco.as_mut().and_then(|aberto| aberto.vez(Instant::now()));
                if let Some(passo) = passo {
                    volta::seguir(&client, &mut buraco, &mut carimbos_do_buraco, passo).await;
                }
            }

            // **A vez da repetição da volta** (lote P61): o que ficou sem
            // resposta sai de novo, com a mesma chave, no compasso que o balde
            // do servidor deixa — acima dele, o servidor descarta calado.
            () = volta::esperar_ate(repeticao.quando()), if repeticao.quando().is_some() => {
                let mut mexeu = false;
                for chave in repeticao.vez(Instant::now()) {
                    let repetir = shared
                        .room
                        .lock()
                        .map(|mut room| volta::repetir(&mut room, chave, agora_em_segundos()))
                        .unwrap_or(volta::Repetir::Nada);
                    match repetir {
                        volta::Repetir::Mandar { canal, corpo, responde_a, chave } => {
                            mexeu = true;
                            if client.dizer(canal, corpo, responde_a, chave).await.is_err() {
                                break;
                            }
                        }
                        volta::Repetir::Recusada => mexeu = true,
                        volta::Repetir::Nada => {}
                    }
                }
                if mexeu {
                    shared.messages_changed();
                }
            }

            // A voz da sala chegou — ou o prazo venceu, ou o aparelho recusou.
            aberta = porta.chegada() => {
                porta.chegou(&shared.voice, aberta, |pedidos| abrir_a_voz(&shared, pedidos));
                shared.notify(&Event::TelemetryChanged);
            }

            () = tokio::time::sleep_until(next_tick.into()) => {
                next_tick = Instant::now() + TICK;
                if measure(&mut sync, &client, &shared) {
                    shared.notify(&Event::TelemetryChanged);
                }
                relatar_a_recepcao(&mut relator, &client, &shared).await;
            }
        }

        // **O gatilho da pendência 52**, depois de cada volta e não dentro de
        // cada braço: o assento muda por cinco caminhos — a entrada pedida, o
        // operador que move, a saída pedida, a recusa por `Alert` e a sala
        // apagada —, e todos terminam no assento do `Room`, pela porta única de
        // `seele_core::assento`. Olhar o antes e o depois aqui cobre os cinco, e
        // o sexto que vier. O assento olhado é o confirmado: ver `sala_de`.
        let antes = shared.voice.abertura();
        porta.olhar(&shared.voice, sala_de(&shared), |pedidos| {
            abrir_a_voz(&shared, pedidos)
        });
        if shared.voice.abertura() != antes {
            shared.notify(&Event::TelemetryChanged);
        }

        // **Os volumes de cada pessoa, pelo `ssrc` de agora**, também no fim de
        // cada volta e pela mesma razão: a voz que abriu chega por um braço do
        // `select!` e o número novo de quem voltou chega por outro, e os dois
        // terminam aqui. Ver `VozDaSala::seguir_os_volumes` (F1-vazamento e
        // acréscimo B do lote I-b).
        shared.voice.seguir_os_volumes(&shared.room);
    }

    shared.running.store(false, Ordering::Relaxed);
    shared
        .link_state
        .store(link_state_byte(LinkTrust::Offline), Ordering::Relaxed);
    // Dropping the voice stops the audio thread with the session. A abertura
    // em curso vai junto, e a voz que ela trouxer cai na thread que a abria.
    //
    // **A despedida não espera a voz** (M-4 do lote V). Este comentário dizia
    // que nada segurava a despedida abaixo, e o cadeado da voz segurava: uma
    // troca de aparelho presa no driver o prende, e o fim da sessão esperava
    // junto até o teto da casca. Agora são duas situações, e
    // `largar_sem_segurar` diz quais:
    //
    // - com o cadeado livre, que é o caso comum, a voz cai **antes** da
    //   despedida, e o microfone fecha antes de o servidor saber da saída;
    // - com o cadeado ocupado, a despedida sai primeiro, e a voz cai numa
    //   thread à parte quando ele soltar — até lá, o microfone segue aberto.
    drop(porta);
    largar_o_que_a_sessao_segurava(&shared);
    client.sair().await;
}

/// O que a sessão segurava nesta máquina, largado no fim dela.
///
/// Uma função, e não o rabo de `drive`, porque o fim tem duas portas: `drive`
/// que volta, e `drive` que entra em pânico ([`subir_a_thread_da_conexao`]). Na
/// segunda nada do rabo dele roda, e o microfone ficava aberto atrás de uma
/// sessão que já não existia.
fn largar_o_que_a_sessao_segurava(shared: &Arc<Shared>) {
    shared.voice.marcar(AberturaDoAudio::ForaDeSala);
    largar_sem_segurar(shared, |shared| &shared.voice);
    // E sai do foco de voz (E3): uma sessão que acabou não segura o microfone
    // da máquina, nem enquanto a casca ainda guarda a `Connection` dela.
    shared.voice.sair_do_foco();
    // E o canal com ela: um `set_capture_device` que chegasse depois abriria um
    // caminho de voz sobre uma conexão morta, e ele ficaria de pé sem sessão
    // nenhuma por trás.
    if let Ok(mut media) = shared.media.lock() {
        *media = None;
    }
    // Quem estava esperando o peso de uma Linha é acordado, e não deixado
    // pendurado. Largar o remetente é o que faz `weigh_channel` devolver
    // `NotConnected` em vez de esperar uma resposta que não vem mais — e o que
    // impede uma caixa de confirmação de ficar aberta e muda depois de a sessão
    // acabar debaixo dela.
    if let Ok(mut pending) = shared.pending_weights.lock() {
        pending.clear();
    }
    // Os túneis acabam com a sessão (o enlace os derruba ao sair); a faixa que
    // lê depois não os vê de pé.
    if let Ok(mut tuneis) = shared.tuneis.lock() {
        tuneis.abertos.clear();
    }
}

/// Sobe a thread da conexão, a que bombeia a sessão (`drive`), e ampara o
/// pânico dela.
///
/// # O pânico que ninguém via
///
/// `drive` roda numa thread só dele. Um pânico ali desenrolava a thread, e mais
/// nada: `running` continuava `true`, o estado do enlace continuava
/// `Verified`, nenhum [`Event::Ended`] saía, e a voz, que mora no `Shared`,
/// continuava aberta. A tela mostrava uma sessão de pé que não recebia nem
/// mandava nada, e a única pista era a linha do gancho de pânico no
/// `seele.log`.
///
/// Agora o fim anormal é um fim: a linha no log diz o que o pânico disse, o
/// que a sessão segurava é largado ([`largar_o_que_a_sessao_segurava`]), e a
/// casca recebe o [`Event::Ended`] de sempre, com [`EndReason::MotorCaiu`]. Não
/// é evento novo nem atravessa o fio: é o caminho da sessão encerrada, com um
/// motivo local a mais, como [`EndReason::ServidorTrocouNaVolta`].
///
/// Um pânico **antes** do aperto de mão responde a quem espera em `abrir`
/// (`Unreachable`); depois dele o envio não tem leitor, e é largado.
///
/// `corpo` recebe o `ready` emprestado porque quem ampara precisa dele depois
/// do pânico.
fn subir_a_thread_da_conexao(
    shared: Arc<Shared>,
    ready: std::sync::mpsc::Sender<Result<Trust, ConnectFailure>>,
    corpo: impl FnOnce(&std::sync::mpsc::Sender<Result<Trust, ConnectFailure>>) + Send + 'static,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("seele-connection".into())
        .spawn(move || {
            // `AssertUnwindSafe`: depois de um pânico nada de `corpo` é lido de
            // novo. O que sobra é o `Shared`, e cada cadeado dele já trata o
            // envenenamento onde é pego.
            let fim = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| corpo(&ready)));
            let Err(carga) = fim else {
                return;
            };
            let detalhe = carga
                .downcast_ref::<&'static str>()
                .map(|texto| (*texto).to_owned())
                .or_else(|| carga.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "pânico sem mensagem de texto".to_owned());
            tracing::error!(
                %detalhe,
                "a thread da conexão entrou em pânico: a sessão é dada como caída, a voz é \
                 largada e a tela recebe o fim com o motivo MotorCaiu"
            );
            let _ = ready.send(Err(ConnectionError::Unreachable.into()));
            shared.running.store(false, Ordering::Relaxed);
            shared
                .link_state
                .store(link_state_byte(LinkTrust::Offline), Ordering::Relaxed);
            largar_o_que_a_sessao_segurava(&shared);
            shared.notify(&Event::Ended {
                reason: EndReason::MotorCaiu,
            });
        })
        .map(|_| ())
}

/// Um relatório do núcleo como a casca o lê: motivos por nome, a latência em
/// milissegundos e, no caminho direto, o apelido da máquina que o roster
/// conhece.
fn relatorio_para_a_casca(
    shared: &Shared,
    relatorio: &seele_core::tunel::RelatorioDoTunel,
) -> RelatorioDeTunel {
    use seele_core::tunel::Caminho;
    let caminho = match relatorio.caminho {
        Caminho::Direto { par } => CaminhoDeTunel::Direto {
            par: par.0,
            apelido: shared
                .room
                .lock()
                .ok()
                .and_then(|room| room.apelido_de(par)),
        },
        Caminho::Estrela => CaminhoDeTunel::Estrela,
    };
    RelatorioDeTunel {
        mod_id: relatorio.mod_id.clone(),
        nome: relatorio.nome.clone(),
        porta: relatorio.porta_local,
        preferida: relatorio.preferida,
        desejada: relatorio.desejada,
        caminho,
        latencia_ms: relatorio
            .latencia
            .map(|latencia| u64::try_from(latencia.as_millis()).unwrap_or(u64::MAX)),
        bytes_por_segundo: relatorio
            .bytes_por_segundo_ida
            .saturating_add(relatorio.bytes_por_segundo_volta),
        bytes_por_segundo_ida: relatorio.bytes_por_segundo_ida,
        bytes_por_segundo_volta: relatorio.bytes_por_segundo_volta,
        conexoes: relatorio.conexoes,
        conexoes_diretas: relatorio.conexoes_diretas,
        conexoes_pela_estrela: relatorio.conexoes_pela_estrela,
        quedas: relatorio
            .quedas
            .iter()
            .map(|(queda, vezes)| QuedaDeTunel {
                motivo: types::nome_da_queda(*queda),
                vezes: *vezes,
            })
            .collect(),
        udp_descartados: relatorio.udp_descartados,
        encerrado: relatorio.encerrado.map(types::nome_da_queda),
    }
}

/// Guarda o último relatório de um túnel. **O que acabou sai da tabela** (a
/// faixa o recebe no evento, com o motivo, e `tuneis_abertos` não o devolve
/// mais).
fn guardar_relatorio_de_tunel(shared: &Shared, relatorio: &RelatorioDeTunel) {
    let Ok(mut tuneis) = shared.tuneis.lock() else {
        return;
    };
    let chave = (relatorio.mod_id.clone(), relatorio.nome.clone());
    if let Some(motivo) = relatorio.encerrado {
        tuneis.abertos.remove(&chave);
        // Quantos fins este túnel já teve: `guardar_tunel_aberto` compara com
        // o que viu antes de pedir a entrada.
        *tuneis.fins.entry(chave).or_insert(0) += 1;
        tracing::info!(
            mod_id = %relatorio.mod_id,
            tunel = %relatorio.nome,
            motivo,
            quedas = ?relatorio.quedas,
            "túnel de jogo acabou"
        );
    } else {
        tuneis.abertos.insert(chave, relatorio.clone());
    }
}

/// Quantos fins de relatório este túnel já teve. Lido **antes** de pedir a
/// entrada, para `guardar_tunel_aberto` saber se o túnel acabou no meio.
fn fins_do_tunel(shared: &Shared, mod_id: &str, nome: &str) -> u64 {
    shared.tuneis.lock().map_or(0, |tuneis| {
        tuneis
            .fins
            .get(&(mod_id.to_owned(), nome.to_owned()))
            .copied()
            .unwrap_or(0)
    })
}

/// A porta que o núcleo abriu, na tabela antes do primeiro relatório: quem lê
/// `tuneis_abertos` logo depois de entrar já a vê, e o que a casca diz é o que
/// o núcleo fez. Não sobrescreve um relatório que chegou antes.
///
/// **Nem ressuscita um túnel que já acabou.** O relatório final (`encerrado`)
/// pode chegar antes de a espera da entrada voltar aqui; sem a conferência,
/// este `or_insert` punha na tabela um túnel fantasma, que `tuneis_abertos`
/// devolvia para sempre. `fins_antes` é o que `fins_do_tunel` contava antes de
/// o pedido sair: se mudou, o túnel acabou no meio e nada é guardado.
fn guardar_tunel_aberto(
    shared: &Shared,
    mod_id: &str,
    nome: &str,
    aberta: PortaDeTunel,
    fins_antes: u64,
) {
    let Ok(mut tuneis) = shared.tuneis.lock() else {
        return;
    };
    let chave = (mod_id.to_owned(), nome.to_owned());
    if tuneis.fins.get(&chave).copied().unwrap_or(0) != fins_antes {
        tracing::info!(mod_id = %mod_id, tunel = %nome, "túnel de jogo: acabou antes de a entrada voltar; nada é guardado");
        return;
    }
    tuneis
        .abertos
        .entry(chave)
        .or_insert_with(|| RelatorioDeTunel {
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
            porta: aberta.porta,
            preferida: aberta.preferida,
            desejada: aberta.desejada,
            caminho: CaminhoDeTunel::Estrela,
            latencia_ms: None,
            bytes_por_segundo: 0,
            bytes_por_segundo_ida: 0,
            bytes_por_segundo_volta: 0,
            conexoes: 0,
            conexoes_diretas: 0,
            conexoes_pela_estrela: 0,
            quedas: Vec::new(),
            udp_descartados: 0,
            encerrado: None,
        });
}

/// Uma recusa a servir um túnel, dita: `fora-do-aceite` é a que a pessoa vai
/// perguntar, e só o log a guarda.
fn tunel_servido_recusado_dito(
    mod_id: &str,
    nome: &str,
    porta: u16,
    recusa: seele_core::tunel::RecusaDeTunel,
) {
    tracing::warn!(
        mod_id = %mod_id,
        tunel = %nome,
        porta,
        motivo = types::nome_da_recusa_do_tunel(recusa),
        "servir o túnel de jogo foi recusado"
    );
}

/// «O produto sabe e não conta»: toda recusa de entrada num túnel vai ao
/// registro com o MOD e o túnel, no nível que ela merece (uma recusa do
/// servidor é rotina; a de porta local é uma máquina com problema).
fn tunel_recusado_dito(mod_id: &str, nome: &str, recusa: seele_core::tunel::RecusaDeTunel) {
    tracing::warn!(
        mod_id = %mod_id,
        tunel = %nome,
        motivo = types::nome_da_recusa_do_tunel(recusa),
        "entrar no túnel de jogo foi recusado"
    );
}

/// A entrada que deu certo, dita com a porta que de fato abriu.
fn tunel_aberto_dito(mod_id: &str, nome: &str, aberta: PortaDeTunel) {
    tracing::info!(
        mod_id = %mod_id,
        tunel = %nome,
        porta = aberta.porta,
        preferida = aberta.preferida,
        desejada = aberta.desejada,
        "porta do túnel de jogo aberta"
    );
}

/// Larga a voz da sessão sem segurar quem chama. M-4 do lote V.
///
/// O fim de [`drive`] chama isto antes da despedida. O `drop` da [`Voice`] não
/// espera nada — só marca o `stop` da thread de áudio —, mas o **cadeado** dela
/// pode estar ocupado: [`Connection::switch_device`] o segura enquanto o driver
/// abre o aparelho novo, por segundos se o driver prender, e quem só lê a voz
/// o segura por um instante. Esperado ali, ele segurava a despedida até o teto
/// da casca, que dizia «a despedida não terminou» sem dizer por quê.
///
/// São duas situações:
/// - **com o cadeado livre**, larga na hora, e a voz cai antes da despedida:
///   o microfone fecha antes de o servidor saber da saída;
/// - **com o cadeado ocupado**, larga numa thread própria, que espera a vez e
///   larga a voz que estiver lá — a que uma troca acabou de pôr, inclusive. A
///   despedida sai primeiro, o microfone segue aberto até o cadeado soltar, e o
///   log diz isso.
///
/// Uma thread que não sobe larga aqui, como era: a despedida atrasada é melhor
/// que um microfone aberto sem sessão.
fn largar_sem_segurar<D, V>(dona: &Arc<D>, voz: fn(&D) -> &VozDaSala<V>)
where
    D: Send + Sync + 'static,
    V: VozViva + Send + 'static,
{
    match voz(dona).viva.try_lock() {
        Ok(mut viva) => *viva = None,
        Err(std::sync::TryLockError::Poisoned(envenenado)) => *envenenado.into_inner() = None,
        Err(std::sync::TryLockError::WouldBlock) => {
            // Neutro de propósito (rodada 1, achado 3): quem prende o cadeado
            // pode ser uma troca de aparelho no driver, por segundos, ou alguém
            // que só lê a voz, por um instante, e daqui não se sabe qual.
            tracing::warn!(
                "o cadeado da voz estava ocupado no fim da sessão: a voz cai numa thread à \
                 parte assim que ele soltar, e até lá o microfone segue aberto; a despedida \
                 sai sem esperar por ela"
            );
            let dela = Arc::clone(dona);
            let subiu = std::thread::Builder::new()
                .name("seele-larga-a-voz".into())
                .spawn(move || voz(&dela).largar());
            if let Err(erro) = subiu {
                tracing::error!(
                    %erro,
                    "a thread que larga a voz não subiu; a despedida espera o driver soltar"
                );
                voz(dona).largar();
            }
        }
    }
}

/// Quanto a abertura do áudio pode levar antes de a sala seguir sem voz.
///
/// Dez segundos cobrem o driver são: abrir o CoreAudio ou o WASAPI leva de
/// dezenas a poucas centenas de milissegundos. **Não** foram medidas aqui, e são
/// medidas do dono: a primeira abertura no macOS, quando o sistema pergunta à
/// pessoa se o SEELE pode usar o microfone e a abertura espera a resposta, e a
/// do WASAPI numa máquina Windows lenta.
const PRAZO_DO_AUDIO: Duration = Duration::from_secs(10);

/// A abertura do áudio que não entregou nada dentro do prazo.
///
/// Inclui a thread que acabou sem entregar — não subiu, ou entrou em pânico, e
/// aí o gancho de pânico do app já deixou a linha dele no log. Para quem está
/// na sala é o mesmo fato: a voz não veio, e a conversa por escrito segue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Atraso;

/// Abre o áudio numa thread própria e espera a voz, até `prazo`. Pendência 52.
///
/// # Por que uma thread própria
///
/// A entrada numa sala acontece dentro do laço de `drive`, que roda num runtime
/// de uma thread só. Abrir ali, de forma síncrona, seguraria a conexão inteira
/// pelo tempo que o driver quisesse — e o driver é o que menos se controla aqui.
///
/// **Thread com nome, e não `spawn_blocking`.** Uma tarefa bloqueante presa no
/// driver seguraria o `drop` do runtime, e com ele a saída do app: soltar o
/// runtime espera a piscina de bloqueio sem prazo. Uma thread solta não segura
/// nada. Ela continua viva até o driver a soltar, e é o único custo.
///
/// # A voz que chega tarde
///
/// Volta no `Err` do envio e cai **na própria thread**, e o `drop` fecha o
/// aparelho. Quem esperava já desistiu: o prazo venceu, ou a pessoa saiu da sala
/// antes de a voz chegar.
async fn esperar_o_audio<V: Send + 'static>(
    vez: &Arc<Mutex<()>>,
    abrir: impl FnOnce() -> V + Send + 'static,
    prazo: Duration,
) -> Result<V, Atraso> {
    let (entrega, chegada) = tokio::sync::oneshot::channel();
    let vez = Arc::clone(vez);
    let subiu = std::thread::Builder::new()
        .name("seele-abre-audio".into())
        .spawn(move || {
            // **Uma abertura por vez no driver**, e a espera pela vez conta no
            // prazo de quem pediu. Ver `VozDaSala::vez_do_driver`.
            let _vez = vez
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if entrega.is_closed() {
                // Quem pediu desistiu enquanto a outra abertura corria: o
                // prazo venceu, ou a pessoa saiu da sala. Não há para quem abrir.
                return;
            }
            let aberta = abrir();
            // `Err(aberta)` quando ninguém espera mais, e ela é largada aqui.
            drop(entrega.send(aberta));
        });
    if let Err(erro) = subiu {
        tracing::error!(%erro, "a thread que abre o áudio não subiu; a sala segue sem voz");
        return Err(Atraso);
    }
    match tokio::time::timeout(prazo, chegada).await {
        Ok(Ok(aberta)) => Ok(aberta),
        Ok(Err(_)) => {
            tracing::warn!(
                "a thread que abria o áudio acabou sem entregar nada; a sala segue sem voz"
            );
            Err(Atraso)
        }
        Err(_) => {
            tracing::warn!("o áudio não abriu em {prazo:?}; a sala segue sem voz");
            Err(Atraso)
        }
    }
}

/// Os dois dispositivos que a casca pediu, como o core os lê.
///
/// Uma função e não dois campos soltos no ponto de chamada: os dois são
/// `Option<String>`, e trocá-los compilaria — a pessoa acabaria com a voz
/// saindo pelo microfone.
fn chosen_devices(config: &ConnectConfig) -> seele_core::DeviceChoice {
    seele_core::DeviceChoice {
        capture: config.capture_device.clone(),
        playback: config.playback_device.clone(),
    }
}

/// O que a reconexão faz com a voz que estava viva.
///
/// Mora aqui fora, e não dentro do braço de `Aviso::Reconectado`, para que a
/// decisão tenha guarda de comportamento: ninguém constrói uma
/// [`seele_core::Voice`] sem placa de som, mas nada do que se decide aqui
/// depende de placa nenhuma. São três decisões, e as três já foram o defeito:
/// reabrir **a partir** da voz viva (é o que carrega os controles), não abrir
/// voz para quem estava em texto puro, e não guardar a voz que não reabriu.
///
/// **A voz sai do lugar e vai inteira para quem reabre**, e não emprestada:
/// pela pendência 52 a reabertura corre na thread de [`esperar_o_audio`], com
/// prazo, e o cadeado da voz não pode ficar preso enquanto o aparelho abre. A
/// voz velha é largada lá, depois de a nova abrir — que é a ordem que
/// `Voice::reopen` já seguia. A terceira decisão mora em
/// [`PortaDaVoz::chegou`]: uma reabertura que falha não devolve nada ao lugar.
///
/// `None` é «não havia voz»: texto puro, ou uma sala em que o aparelho já não
/// tinha aberto. Nada é reaberto.
fn reabrir_voz_na_reconexao<V, F>(slot: &mut Option<V>, reabrir: impl FnOnce(V) -> F) -> Option<F> {
    slot.take().map(reabrir)
}

/// O que a pessoa escolheu para a voz, guardado fora dela. Pendência 52.
///
/// Fora de sala não há [`Voice`] para segurar estes valores, e eles têm de
/// valer quando ela abrir: a voz nasce com os padrões e **veste** isto ao entrar
/// no lugar ([`VozDaSala::instalar`]). Com a voz de pé, cada escolha é gravada
/// aqui e aplicada nela, e as duas cópias andam juntas.
///
/// O volume de cada pessoa não está aqui: ele é por apelido, e mora em
/// [`VozDaSala::volumes`] (F1-vazamento).
#[derive(Debug, Clone, PartialEq)]
struct ControlesGuardados {
    mudo: bool,
    isolamento: bool,
    modo: seele_core::VoiceMode,
    /// De zero a um, como `Voice::set_supressao` a recebe.
    supressao: f32,
    /// Já dentro da faixa. `None` é o padrão, que depende da supressão.
    abertura_dbfs: Option<f32>,
    /// Os aparelhos pedidos — o pedido, e não o que abriu.
    aparelhos: seele_core::DeviceChoice,
    /// A tecla de falar, como a janela a mandou por último. Item 1 do lote V.
    ///
    /// **Não é escolha, e mora aqui assim mesmo.** A janela manda a pressão e a
    /// soltura uma vez cada (`segurarFala` deduplica). Sem voz viva — a volta a
    /// tira do lugar enquanto ela reabre, e a primeira abertura fica no
    /// «ABRINDO» —, `set_talking` não tinha onde pôr a soltura, e ela se perdia:
    /// a voz nova entrava com a tecla da velha (`Voice::reopen` a carrega) e
    /// transmitia até a próxima pressão, com o roster dizendo que a pessoa fala.
    tecla_segura: bool,
    /// O volume do som da transmissão, de zero a quatro. M-2 do lote V.
    volume_da_tela: f32,
    /// Se o som da transmissão está calado, sem mexer no volume. M-2 do lote V.
    tela_calada: bool,
}

impl ControlesGuardados {
    /// Os de uma voz que acabou de abrir, com os aparelhos que a casca pediu.
    ///
    /// Push-to-talk, supressão inteira e a sensibilidade no padrão: os mesmos de
    /// `Controls::novos`, no núcleo. Uma voz que vestisse outra coisa por padrão
    /// abriria diferente da que se abria antes da pendência 52.
    ///
    /// **Não é o padrão do produto para quem chega**, que é a voz: esse depende
    /// de a instalação ser nova, quem sabe é o disco, e ele entra por
    /// [`vestir_com_o_disco`] logo depois do `connect`. Aqui é o nascimento, e
    /// o nascimento é fechado.
    fn de_partida(aparelhos: seele_core::DeviceChoice) -> Self {
        Self {
            mudo: false,
            isolamento: false,
            modo: seele_core::VoiceMode::PushToTalk,
            supressao: 1.0,
            abertura_dbfs: None,
            aparelhos,
            tecla_segura: false,
            volume_da_tela: 1.0,
            tela_calada: false,
        }
    }
}

/// O que a pendência 52 pede de uma voz viva: vestir o que a pessoa escolheu.
///
/// Um traço com uma implementação de verdade e uma de teste. Abrir uma
/// [`Voice`] pede placa de som, e o que se decide sobre ela — quando abrir,
/// quando fechar, o que ela veste — não depende de placa nenhuma.
trait VozViva {
    /// Aplica tudo o que a pessoa escolheu.
    fn vestir(&self, escolhida: &ControlesGuardados);

    /// Segura ou solta a tecla de falar. Item 1 do lote V.
    ///
    /// À parte de [`Self::vestir`] porque a tecla não é escolha: é o que a
    /// pessoa está fazendo agora. Em que voz ela vale quem decide é a
    /// [`VozDaSala`] — na viva, a cada pressão, e na que entra no lugar.
    fn segurar(&self, segura: bool);

    /// Põe o ganho de quem fala por este `ssrc`. `1.0` é o de origem.
    ///
    /// Os volumes são por apelido (F1-vazamento), e a voz só conhece números:
    /// quem traduz um no outro é [`ganhos_a_aplicar`].
    fn ganhar(&self, ssrc: Ssrc, ganho: f32);
}

impl VozViva for Voice {
    fn vestir(&self, escolhida: &ControlesGuardados) {
        self.set_muted(escolhida.mudo);
        self.set_total_isolation(escolhida.isolamento);
        self.set_mode(escolhida.modo);
        self.set_supressao(escolhida.supressao);
        self.set_abertura_dbfs(escolhida.abertura_dbfs);
        self.set_volume_da_tela(escolhida.volume_da_tela);
        self.set_tela_calada(escolhida.tela_calada);
    }

    fn segurar(&self, segura: bool) {
        self.set_key_held(segura);
    }

    fn ganhar(&self, ssrc: Ssrc, ganho: f32) {
        self.set_gain(ssrc.get(), ganho);
    }
}

/// Veste uma voz com tudo o que está guardado: o que a pessoa escolheu, e a
/// tecla de falar como a janela a mandou por último.
///
/// Uma função para os dois lugares que vestem — a escolha nova na voz viva
/// ([`VozDaSala::escolher`]) e a voz que entra no lugar
/// ([`VozDaSala::instalar`]) —, para a tecla não ficar de fora de um deles. Na
/// que entra, a guardada vale **por cima** da que `Voice::reopen` carregou da
/// velha (item 1 do lote V).
fn vestir_inteira<V: VozViva>(voz: &V, guardados: &ControlesGuardados) {
    voz.vestir(guardados);
    voz.segurar(guardados.tecla_segura);
}

/// Veste a voz desta sessão com o que está gravado em disco: o modo, a
/// supressão de ruído e a sensibilidade. Acréscimo A do lote I-b.
///
/// A supressão e a sensibilidade, só se gravadas: a que a pessoa nunca escolheu
/// fica no padrão da sessão. **O modo, sempre**: sem escolha, o que vale é o
/// padrão desta instalação — a voz para quem chegou, a tecla para quem já
/// usava —, e é o disco que sabe qual
/// (`seele_core::preferences::Preferences::modo_do_microfone`). A sessão nasce
/// na tecla ([`ControlesGuardados::de_partida`]), e é aqui que quem chegou
/// passa a falar sem ela.
///
/// Gravado em [`VozDaSala`], e por isso vale fora de sala e na primeira
/// abertura do áudio, como tudo o que se escolhe lá (pendência 52).
fn vestir_com_o_disco<V: VozViva>(
    voz: &VozDaSala<V>,
    ajustes: &seele_core::preferences::Preferences,
) {
    voz.escolher(|escolhida| {
        escolhida.modo = ajustes.modo_do_microfone();
        if let Some(forca) = ajustes.supressao_de_ruido() {
            escolhida.supressao = forca.clamp(0.0, 1.0);
        }
        if let Some(dbfs) = ajustes.abertura_da_voz_dbfs() {
            escolhida.abertura_dbfs = Some(abertura_na_faixa(dbfs));
        }
    });
}

/// A sensibilidade da ativação por voz, dentro da faixa que a voz aceita.
fn abertura_na_faixa(dbfs: f32) -> f32 {
    dbfs.clamp(
        *seele_core::FAIXA_DE_ABERTURA_DBFS.start(),
        *seele_core::FAIXA_DE_ABERTURA_DBFS.end(),
    )
}

/// O teto de [`Connection::set_volume`], em por cento: quatro vezes o de origem.
const VOLUME_MAXIMO: u16 = 400;

/// O ganho de cada volume guardado, pelo `ssrc` que o apelido tem agora.
/// F1-vazamento, passo 3 (2).
///
/// # Por que a tradução é refeita
///
/// O volume é escolhido para uma **pessoa**, e a voz só conhece **números**: o
/// `ssrc` que o servidor deu a ela nesta conexão dela. O número muda quando ela
/// cai e volta sem resgate, quando troca de rede, e quando é esta máquina que
/// volta. Gravado só no número, como `set_volume` fazia, o volume ficava para
/// trás, e a janela continuava mostrando 150 para quem tocava a 100.
///
/// Quem não está no `Room` não ganha número, e fica guardado para quando
/// estiver. O apelido não depende de maiúscula, como em `Room::ssrc_of`.
/// Ordenado pelo número, para duas respostas iguais serem iguais.
fn ganhos_a_aplicar(volumes: &HashMap<String, u16>, room: &Room) -> Vec<(Ssrc, f32)> {
    let mut ganhos: Vec<(Ssrc, f32)> = volumes
        .iter()
        .filter_map(|(apelido, percent)| {
            room.ssrc_of(apelido)
                .map(|ssrc| (ssrc, f32::from((*percent).min(VOLUME_MAXIMO)) / 100.0))
        })
        .collect();
    ganhos.sort_by_key(|(ssrc, _)| ssrc.get());
    ganhos
}

/// A voz desta sessão, o que a pessoa escolheu para ela, e em que pé a abertura
/// está. Pendência 52.
///
/// # A ordem dos cadeados
///
/// A voz primeiro, a escolha depois, sempre. [`Self::escolher`] grava a escolha
/// **antes** de olhar a voz, e [`Self::instalar`] lê a escolha **com** a voz
/// presa: uma escolha feita enquanto a voz abre é vista por um dos dois, em
/// qualquer ordem que as threads andem.
struct VozDaSala<V> {
    viva: Mutex<Option<V>>,
    escolhida: Mutex<ControlesGuardados>,
    abertura: Mutex<AberturaDoAudio>,
    /// Se esta sessão pediu áudio ([`ConnectConfig::audio`]). Sem ele, nenhuma
    /// sala abre aparelho.
    quer_audio: bool,
    /// A vez do driver: no máximo uma abertura de aparelho em voo por sessão.
    ///
    /// M-c da rodada 1 do lote I-a. Entrar e sair de uma sala depressa larga a
    /// abertura em curso, mas a thread dela segue no driver até ele soltar; a
    /// entrada seguinte poria uma segunda abertura no CoreAudio ao mesmo tempo,
    /// e duas simultâneas nunca foram medidas. A thread de
    /// [`esperar_o_audio`] toma esta vez antes de abrir, e a de uma abertura
    /// de que ninguém espera mais desiste sem abrir nada quando a vez chega.
    vez_do_driver: Arc<Mutex<()>>,
    /// O volume que a pessoa escolheu para cada um, por apelido, em por cento.
    /// F1-vazamento, passo 3 (2).
    ///
    /// **Aqui, na conexão, e não na janela.** Ele morava num mapa de
    /// `tela-sessao.js`, e o mapa sobrevivia à sessão: entrar noutro servidor
    /// desenhava o 150 que se deu à Rafa de lá para a Rafa de cá, que tocava a
    /// 100. Esta conexão nasce sem volume nenhum. E a voz da pendência 52 nasce
    /// a cada entrada numa sala, com todo mundo a 100: guardado só nela, o volume
    /// morria ao sair da sala (acréscimo B do lote I-b).
    ///
    /// Por apelido, porque é a pessoa que se escolhe; a voz recebe o número
    /// dela por [`ganhos_a_aplicar`]. Chega à casca por
    /// [`Connection::volumes_locais`], e **nunca** pelo `Snapshot`, que os MODs
    /// leem: quanto alguém quer ouvir cada um é desta máquina.
    ///
    /// Na ordem dos cadeados, depois da voz: ver [`Self::seguir_os_volumes`].
    volumes: Mutex<HashMap<String, u16>>,
    /// Os ganhos que já estão na voz viva, do jeito que [`ganhos_a_aplicar`] os
    /// deu. `None` com uma voz que ainda não recebeu nenhum — é o que
    /// [`Self::instalar`] põe numa voz aberta do zero, e o que faz a voz nova
    /// recebê-los. A reaberta da volta traz os da velha, e os mantém.
    aplicados: Mutex<Option<Vec<(Ssrc, f32)>>>,
    /// **O lugar desta conexão no foco de voz da máquina** (E3 da 0.17). Vazio
    /// numa conexão aberta pelas portas sem foco: ela é sempre dona da própria
    /// voz, como antes de haver várias sessões. Ver [`foco`].
    ///
    /// Preenchido uma vez, logo depois de o `Shared` existir e antes de o driver
    /// subir: nenhum olhar da porta da voz o vê vazio numa conexão com foco.
    foco: std::sync::OnceLock<foco::NoFoco>,
    /// Quem pediu para saber que o driver olhou a voz de novo: a troca do foco
    /// espera esta resposta para saber que a abertura em curso foi largada.
    /// Respondidos no fim de [`PortaDaVoz::olhar`].
    olhadas: Mutex<Vec<std::sync::mpsc::Sender<()>>>,
}

impl<V> VozDaSala<V> {
    fn nova(quer_audio: bool, aparelhos: seele_core::DeviceChoice) -> Self {
        Self {
            viva: Mutex::new(None),
            escolhida: Mutex::new(ControlesGuardados::de_partida(aparelhos)),
            abertura: Mutex::new(AberturaDoAudio::ForaDeSala),
            quer_audio,
            vez_do_driver: Arc::new(Mutex::new(())),
            volumes: Mutex::new(HashMap::new()),
            aplicados: Mutex::new(None),
            foco: std::sync::OnceLock::new(),
            olhadas: Mutex::new(Vec::new()),
        }
    }

    /// Põe esta voz num foco de voz. Uma vez só; a segunda é ignorada e dita.
    fn entrar_no_foco(&self, lugar: foco::NoFoco) {
        if let Err(repetido) = self.foco.set(lugar) {
            tracing::warn!(
                id = repetido.id,
                "uma voz já num foco foi posta noutro: o segundo foi ignorado"
            );
        }
    }

    /// Esta conexão tem a voz da máquina? Sem foco nenhum, sempre: é a conexão
    /// sozinha de antes da 0.17.
    fn tem_foco(&self) -> bool {
        self.foco.get().is_none_or(foco::NoFoco::tem)
    }

    /// A primeira conexão da vida do foco, já de pé, toma a voz sozinha. Ver
    /// [`foco::FocoDeVoz::tomar_se_primeira`].
    fn tomar_o_foco_se_primeira(&self) {
        if let Some(lugar) = self.foco.get() {
            lugar.foco.tomar_se_primeira(lugar.id);
        }
    }

    /// Tira esta conexão do foco, se ela está num. Idempotente; o `Drop` do
    /// lugar faz o mesmo.
    fn sair_do_foco(&self) {
        if let Some(lugar) = self.foco.get() {
            lugar.foco.desinscrever(lugar.id);
        }
    }

    /// O número desta conexão no foco, quando ela está num.
    fn id_no_foco(&self) -> Option<u64> {
        self.foco.get().map(|lugar| lugar.id)
    }

    /// A vez de aparelho: a da máquina, num foco; a desta conexão, sem.
    fn vez(&self) -> Arc<Mutex<()>> {
        self.foco
            .get()
            .map_or_else(|| Arc::clone(&self.vez_do_driver), |lugar| lugar.foco.vez())
    }

    /// Uma pergunta «ainda tenho o foco?» que atravessa para a thread que abre
    /// o aparelho. A abertura a faz **sob a vez da máquina**, logo depois de
    /// abrir: sem o foco, larga o que abriu ali mesmo, antes de soltar a vez, e
    /// a sessão que ganhou o foco só abre depois.
    fn conferidor_do_foco(&self) -> impl Fn() -> bool + Send + 'static {
        let lugar = self
            .foco
            .get()
            .map(|lugar| (Arc::clone(&lugar.foco), lugar.id));
        move || {
            lugar
                .as_ref()
                .is_none_or(|(foco, id)| foco.dono() == Some(*id))
        }
    }

    /// Guarda quem quer saber do próximo olhar da porta da voz.
    fn esperar_o_olhar(&self, resposta: Option<std::sync::mpsc::Sender<()>>) {
        if let Some(resposta) = resposta {
            self.olhadas
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(resposta);
        }
    }

    /// Responde a todos que esperavam um olhar.
    fn responder_olhadas(&self) {
        let esperando = std::mem::take(
            &mut *self
                .olhadas
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for resposta in esperando {
            let _ = resposta.send(());
        }
    }

    /// Grava o volume de uma pessoa, por apelido.
    ///
    /// Sem distinguir maiúscula, como `Room::ssrc_of` a acha: «Rafa» e «rafa»
    /// são a mesma pessoa, e duas linhas para ela seriam dois volumes brigando
    /// pelo mesmo número.
    fn guardar_volume(&self, apelido: &str, percent: u16) {
        let mut volumes = self
            .volumes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        volumes.retain(|guardado, _| !guardado.eq_ignore_ascii_case(apelido));
        volumes.insert(apelido.to_owned(), percent.min(VOLUME_MAXIMO));
    }

    /// Leva o volume de quem trocou de nome para o nome novo. M-4 da rodada 1.
    ///
    /// O nome de antes deixa de guardar qualquer coisa — senão o volume valeria
    /// para quem pegasse aquele apelido depois. O que já estava guardado no
    /// nome novo sai **sempre**: é da pessoa que trocou que o servidor está
    /// falando, e o volume dela — ou nenhum — é o que vale sob esse nome. Era
    /// só trocado quando ela tinha volume; sem volume, o de outra pessoa com
    /// aquele nome, que já saiu, passava a valer para ela (rodada 2 do lote
    /// I-b).
    fn renomear_volume(&self, antes: &str, agora: &str) {
        let mut volumes = self
            .volumes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dela = volumes
            .keys()
            .find(|guardado| guardado.eq_ignore_ascii_case(antes))
            .cloned()
            .and_then(|chave| volumes.remove(&chave));
        volumes.retain(|guardado, _| !guardado.eq_ignore_ascii_case(agora));
        if let Some(percent) = dela {
            volumes.insert(agora.to_owned(), percent);
        }
    }

    /// Os volumes guardados, como a casca os lê.
    fn volumes(&self) -> HashMap<String, u16> {
        self.volumes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// O cadeado da voz viva.
    ///
    /// O mesmo nome do `Mutex` que este campo era antes da pendência 52, para
    /// que quem só lê a voz — a telemetria, o som da tela, a malha de bitrate —
    /// continue lendo do mesmo jeito.
    fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, Option<V>>> {
        self.viva.lock()
    }

    /// O que a pessoa escolheu, agora.
    fn escolhida(&self) -> ControlesGuardados {
        self.escolhida
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// O som da tela guardado, como a casca o desenha sem voz viva. Achado 6
    /// da revisão do lote V.
    ///
    /// É o que a voz que abrir vai vestir, e por isso o que o botão tem de
    /// mostrar. `None` numa sessão sem áudio, que nunca terá voz a vestir — a
    /// mesma regra de [`Connection::controles_da_voz`].
    fn som_da_tela_escolhido(&self) -> Option<SomDaTela> {
        if !self.quer_audio {
            return None;
        }
        let escolhida = self.escolhida();
        Some(SomDaTela {
            volume: escolhida.volume_da_tela,
            calada: escolhida.tela_calada,
        })
    }

    /// Em que pé a abertura está, guardado.
    fn abertura(&self) -> AberturaDoAudio {
        *self
            .abertura
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn marcar(&self, estado: AberturaDoAudio) {
        *self
            .abertura
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = estado;
    }

    /// Grava o aparelho escolhido de um lado, sem mexer no outro.
    fn guardar_aparelho(&self, lado: Lado, pedido: Option<String>) {
        let mut escolhida = self
            .escolhida
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match lado {
            Lado::Entrada => escolhida.aparelhos.capture = pedido,
            Lado::Saida => escolhida.aparelhos.playback = pedido,
        }
    }
}

impl<V: VozViva> VozDaSala<V> {
    /// A tecla de falar, segura ou solta. Item 1 do lote V.
    ///
    /// **Gravada sempre**, com voz viva ou sem, e posta na viva se há uma. Era
    /// posta só na viva: durante a reabertura da volta e no «ABRINDO» a tecla
    /// não tinha onde cair, e a janela não a manda de novo.
    fn segurar_a_tecla(&self, segura: bool) {
        self.escolher(|escolhida| escolhida.tecla_segura = segura);
    }

    /// A tecla guardada volta a solta: a troca do foco faz isto na sessão que
    /// vai ganhá-lo, antes de ela poder abrir. Ver [`foco::FocoDeVoz::focar`].
    fn soltar_a_tecla_guardada(&self) {
        self.segurar_a_tecla(false);
    }

    /// **A voz cai porque o foco saiu desta sessão**, da thread de quem troca.
    ///
    /// A tecla solta primeiro, e a voz sai do lugar depois: o mesmo acordo do
    /// fechar da porta. O estado diz por que não há voz, se a sessão estava
    /// abrindo ou com a voz de pé; o driver confirma no olhar seguinte.
    fn calar_sem_foco(&self) {
        self.segurar_a_tecla(false);
        if matches!(
            self.abertura(),
            AberturaDoAudio::Abrindo | AberturaDoAudio::Aberto
        ) {
            self.marcar(AberturaDoAudio::EmOutraSessao);
        }
        self.largar();
    }

    /// O volume e o mudo do som da tela: gravados, e postos na voz viva se há
    /// uma. M-2 do lote V.
    ///
    /// Eram postos só na voz viva, e recusados sem ela: com a pendência 52 a
    /// voz nasce a cada entrada numa sala, e o som da transmissão voltava a 100
    /// e audível a cada uma. `NotConnected` só numa sessão sem áudio, que nunca
    /// terá voz a vestir.
    fn escolher_o_som_da_tela(
        &self,
        mudar: impl FnOnce(&mut ControlesGuardados),
    ) -> Result<(), ConnectionError> {
        if !self.quer_audio {
            return Err(ConnectionError::NotConnected);
        }
        self.escolher(mudar);
        Ok(())
    }

    /// Grava uma escolha e a aplica na voz viva, se há uma.
    fn escolher(&self, mudar: impl FnOnce(&mut ControlesGuardados)) {
        mudar(
            &mut self
                .escolhida
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        if let Ok(viva) = self.viva.lock() {
            if let Some(viva) = viva.as_ref() {
                vestir_inteira(viva, &self.escolhida());
            }
        }
    }

    /// Põe no lugar uma voz recém-aberta, vestida com o que a pessoa escolheu
    /// enquanto ela abria.
    ///
    /// **É aqui que o mudo pedido fora de sala vale.** Sem o `vestir`, a voz
    /// abriria com os padrões e o microfone de quem se mutou antes de entrar
    /// estaria aberto — com o roster dizendo «mudo», porque o servidor recebeu o
    /// pedido.
    ///
    /// **E é aqui que se confere o aparelho, sob o cadeado da voz** (M-b da
    /// rodada 1 do lote I-a). `pedidos` são os aparelhos com que a voz abriu; se
    /// a pessoa escolheu outro enquanto ela abria, a voz volta no `Err` e quem
    /// chamou recomeça. Conferido fora do cadeado, como era em
    /// [`PortaDaVoz::chegou`], uma troca entre a conferência e este cadeado
    /// instalava uma voz aberta no aparelho de antes: `switch_device` grava a
    /// escolha segurando este mesmo cadeado, e só sob ele as duas coisas não se
    /// cruzam.
    fn instalar(
        &self,
        nova: V,
        pedidos: &seele_core::DeviceChoice,
        origem: Origem,
    ) -> Result<(), V> {
        match self.viva.lock() {
            Ok(mut viva) => {
                // **Sem o foco, a voz que chegou é largada aqui** (E3), sob o
                // mesmo cadeado com que a troca do foco cala esta sessão: ou a
                // troca já calou, e esta conferência a vê sem foco, ou ela ainda
                // vai calar, e cala esta voz junto.
                if !self.tem_foco() {
                    drop(nova);
                    self.marcar(AberturaDoAudio::EmOutraSessao);
                    return Ok(());
                }
                let escolhida = self.escolhida();
                if escolhida.aparelhos != *pedidos {
                    return Err(nova);
                }
                vestir_inteira(&nova, &escolhida);
                *viva = Some(nova);
                // Uma voz nova nasce com todo mundo a 1.0, e os volumes ainda
                // não estão nela: zerar os aplicados é o que faz o fim desta
                // volta do laço os pôr (`seguir_os_volumes`, acréscimo B).
                //
                // **A reaberta, não** (item 2 do lote V): ela traz os ganhos da
                // velha, e os aplicados continuam dizendo o que está nela.
                // Zerados, `seguir_os_volumes` não sabia que o número de quem
                // tinha volume estava com ele, e não o devolvia a 1.0 quando o
                // número passava a ser de outra pessoa.
                if origem == Origem::Nova {
                    *self
                        .aplicados
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                }
                self.marcar(AberturaDoAudio::Aberto);
            }
            Err(_) => self.marcar(AberturaDoAudio::SemAparelho),
        }
        Ok(())
    }

    /// Larga a voz viva, se há uma. O `drop` fecha o aparelho.
    ///
    /// **Também com o cadeado envenenado** (rodada 1 do lote V, achado 4):
    /// alguém entrou em pânico com ele na mão, e a voz que ficou lá dentro
    /// continua com o microfone aberto. Desistir dela era deixá-lo assim.
    fn largar(&self) {
        *self
            .viva
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    fn tem_voz(&self) -> bool {
        self.viva.lock().is_ok_and(|viva| viva.is_some())
    }

    /// Põe na voz viva os volumes guardados, pelo `ssrc` de agora. Devolve se
    /// mexeu em algum ganho. F1-vazamento e acréscimo B.
    ///
    /// `drive` chama isto no fim de cada volta do laço, e é o que cobre os dois
    /// momentos em que o ganho tem de ser refeito: **a voz que acabou de abrir**
    /// e **o número que mudou** (uma mensagem dobrada no `Room` muda a resposta
    /// de [`ganhos_a_aplicar`]). Na voz aberta do zero, [`Self::instalar`] zera
    /// os aplicados, e ela recebe todos; na reaberta da volta, que traz os
    /// ganhos da velha, os aplicados ficam, e ela recebe só o que mudou (item 2
    /// do lote V). É a mesma forma do gatilho da pendência 52: o antes contra o
    /// agora, e não a lista das mensagens que mudam um `ssrc`, que seria mais
    /// um lugar para esquecer uma. Sem mudança, não toca na voz.
    ///
    /// O número que deixou de ser de alguém com volume volta a 1.0: o volume é
    /// da pessoa, e o número pode passar a outra. O `seele-server` de hoje só
    /// conta os números para a frente num mesmo processo, e depois de um
    /// reinício não há resgate de assento, e a voz abre do zero. Mas o fio não
    /// promete que um número não volte, e uma volta com resgate em que o número
    /// de alguém muda de dono é o caso que a reaberta guarda.
    ///
    /// # A ordem dos cadeados
    ///
    /// A voz, depois os volumes, depois o `Room`. Os volumes são lidos **com** a
    /// voz presa, e [`Connection::set_volume`] grava antes de chamar isto: um
    /// volume escolhido enquanto a voz abre é visto por uma das duas chamadas,
    /// em qualquer ordem que as threads andem — o mesmo acordo de
    /// [`Self::escolher`] e [`Self::instalar`]. Ninguém segura o `Room` enquanto
    /// pede a voz.
    fn seguir_os_volumes(&self, room: &Mutex<Room>) -> bool {
        let Ok(viva) = self.viva.lock() else {
            return false;
        };
        let Some(viva) = viva.as_ref() else {
            return false;
        };
        let ganhos = {
            let volumes = self
                .volumes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Ok(room) = room.lock() else {
                return false;
            };
            ganhos_a_aplicar(&volumes, &room)
        };
        let mut aplicados = self
            .aplicados
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if aplicados.as_ref() == Some(&ganhos) {
            return false;
        }
        if let Some(antes) = aplicados.as_ref() {
            for (ssrc, _) in antes {
                if !ganhos.iter().any(|(agora, _)| agora == ssrc) {
                    viva.ganhar(*ssrc, 1.0);
                }
            }
        }
        for (ssrc, ganho) in &ganhos {
            viva.ganhar(*ssrc, *ganho);
        }
        *aplicados = Some(ganhos);
        true
    }
}

/// O que uma abertura entrega: a voz, ou por que ela não veio.
type Aberta<V> = Result<Result<V, String>, Atraso>;

/// Uma abertura que ainda vai entregar.
type Futuro<V> = std::pin::Pin<Box<dyn std::future::Future<Output = Aberta<V>>>>;

/// Uma abertura em curso, com os aparelhos que ela pediu.
///
/// Largá-la desiste dela: a voz que ela trouxer cai na thread que a abriu, e o
/// `drop` fecha o aparelho. Ver [`esperar_o_audio`].
struct EmCurso<V> {
    futuro: Futuro<V>,
    pedidos: seele_core::DeviceChoice,
    origem: Origem,
}

/// De onde vem a voz que entra no lugar, e o que isso diz dos ganhos dela.
/// Item 2 do lote V.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origem {
    /// Aberta do zero: todo mundo a 1.0, e os volumes ainda não estão nela.
    Nova,
    /// A reabertura da volta: `Voice::reopen` carrega os ganhos da velha.
    Reaberta,
}

/// O que o assento pede da voz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PassoDaVoz {
    Abrir,
    Fechar,
    /// Numa sala, numa sessão que entrou sem áudio.
    SemAudio,
    Nada,
}

/// A decisão da pendência 52, sozinha: o áudio abre quando a pessoa **passa a
/// estar** numa sala, e fecha quando deixa de estar.
///
/// Lida de antes e depois do assento, e não de qual mensagem chegou, porque os
/// caminhos para dentro e para fora de uma sala são cinco — a entrada pedida, o
/// `MovedToVoiceRoom` de um operador, a saída pedida, a recusa por `Alert` e a
/// sala apagada — e todos já terminam no mesmo `Room::current_voice_room`. Um
/// gatilho que casasse as mensagens seria um sexto lugar para esquecer uma.
///
/// Mudar de uma sala para outra com a voz de pé não reabre nada: a voz não é
/// de uma sala, e o servidor é quem decide para onde ela vai. Sem voz — o
/// aparelho não abriu na sala de antes —, a sala nova tenta de novo.
fn passo_da_voz(
    antes: Option<VoiceRoomId>,
    agora: Option<VoiceRoomId>,
    quer_audio: bool,
    ha_voz: bool,
) -> PassoDaVoz {
    match (antes, agora) {
        (Some(_), None) => PassoDaVoz::Fechar,
        (_, Some(sala)) if antes != Some(sala) && !ha_voz => {
            if quer_audio {
                PassoDaVoz::Abrir
            } else {
                PassoDaVoz::SemAudio
            }
        }
        _ => PassoDaVoz::Nada,
    }
}

/// Quando o áudio abre e fecha, do lado de `drive`. Pendência 52.
///
/// Mora no laço, e não em [`Shared`], porque a abertura em curso é um futuro que
/// só o laço espera. O que a casca lê — a voz, a escolha, o estado — mora em
/// [`VozDaSala`].
struct PortaDaVoz<V> {
    /// O assento da última vez que se olhou.
    sala_vista: Option<VoiceRoomId>,
    em_curso: Option<EmCurso<V>>,
}

impl<V: VozViva> PortaDaVoz<V> {
    const fn nova() -> Self {
        Self {
            sala_vista: None,
            em_curso: None,
        }
    }

    /// Olha o assento de agora e abre, fecha, ou não faz nada.
    ///
    /// `drive` chama isto logo depois do aperto de mão — com o servidor adotado
    /// e ninguém sentado, e é por isso que nada abre ao conectar — e depois de
    /// cada volta do laço.
    fn olhar(
        &mut self,
        voz: &VozDaSala<V>,
        agora: Option<VoiceRoomId>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        // **O portão do foco de voz** (E3 da 0.17): sem o foco, a sala conta como
        // nenhuma para o áudio. Perder o foco sentado é o mesmo fechar de sair da
        // sala; ganhá-lo sentado é o mesmo abrir de entrar. O estado diz que a
        // voz está noutra sessão, e não que a pessoa saiu da sala.
        let tem_foco = voz.tem_foco();
        let sentada = agora;
        let agora = if tem_foco { agora } else { None };
        let antes = std::mem::replace(&mut self.sala_vista, agora);
        let ha_voz = self.em_curso.is_some() || voz.tem_voz();
        self.decidir(voz, antes, agora, ha_voz, abrir);
        if !tem_foco && sentada.is_some() {
            voz.marcar(AberturaDoAudio::EmOutraSessao);
        } else if sentada.is_none() && voz.abertura() == AberturaDoAudio::EmOutraSessao {
            // Saiu da sala depois de perder o foco (a troca a faz sair): fora
            // de sala, e não «a voz está noutra sessão».
            voz.marcar(AberturaDoAudio::ForaDeSala);
        }
        // Quem trocou o foco espera isto: daqui em diante a abertura em curso
        // desta sessão, se havia uma, foi largada.
        voz.responder_olhadas();
    }

    /// O passo da pendência 52, do antes para o agora do assento.
    fn decidir(
        &mut self,
        voz: &VozDaSala<V>,
        antes: Option<VoiceRoomId>,
        agora: Option<VoiceRoomId>,
        ha_voz: bool,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        match passo_da_voz(antes, agora, voz.quer_audio, ha_voz) {
            PassoDaVoz::Abrir => self.comecar(voz, abrir),
            PassoDaVoz::Fechar => {
                // A abertura em curso é largada, e a voz que ela trouxer cai na
                // thread que a abriu. O estado muda antes da voz sair do lugar:
                // quem ler entre os dois vê a voz ainda de pé, e não um «fora
                // de sala» com aparelho aberto.
                self.em_curso = None;
                voz.marcar(AberturaDoAudio::ForaDeSala);
                // **A tecla guardada volta a solta** (rodada 1 do lote V). Uma
                // soltura que a janela perdeu nesta sala não pode abrir o
                // microfone transmitindo na próxima; uma pressão no «ABRINDO»
                // da próxima vem depois de entrar, e continua valendo.
                voz.segurar_a_tecla(false);
                voz.largar();
            }
            PassoDaVoz::SemAudio => voz.marcar(AberturaDoAudio::SemAparelho),
            PassoDaVoz::Nada => {}
        }
    }

    fn comecar(
        &mut self,
        voz: &VozDaSala<V>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        let pedidos = voz.escolhida().aparelhos;
        voz.marcar(AberturaDoAudio::Abrindo);
        self.em_curso = Some(EmCurso {
            futuro: abrir(&pedidos),
            pedidos,
            origem: Origem::Nova,
        });
    }

    /// A volta do enlace: a voz viva reabre na conexão nova.
    ///
    /// Quem estava fora de sala não tem voz e continua sem aparelho. Uma
    /// abertura que estava em curso abria sobre a conexão que caiu, e recomeça
    /// na nova.
    fn na_volta(
        &mut self,
        voz: &VozDaSala<V>,
        reabrir: impl FnOnce(V) -> Futuro<V>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        if self.em_curso.take().is_some() {
            self.comecar(voz, abrir);
            return;
        }
        if !voz.tem_voz() {
            return;
        }
        voz.marcar(AberturaDoAudio::Abrindo);
        let pedidos = voz.escolhida().aparelhos;
        let reaberta = voz.lock().ok().and_then(|mut viva| {
            // **A velha sai do lugar calada e com a tecla solta** (rodada 1 do
            // lote V). Ela segue viva até a reabertura voltar, mandando pelo
            // canal da conexão que caiu — que ninguém fecha, e que fica de pé
            // até o prazo de ociosidade —, e fora do lugar nenhuma escolha a
            // alcança: a soltura e o mudo pedidos agora vão só para a guardada.
            // A falha fica do lado fechado. A reaberta herda isso, e `instalar`
            // a veste com o que está guardado.
            if let Some(velha) = viva.as_ref() {
                let mut calada = voz.escolhida();
                calada.mudo = true;
                calada.tecla_segura = false;
                vestir_inteira(velha, &calada);
            }
            reabrir_voz_na_reconexao(&mut viva, reabrir)
        });
        match reaberta {
            Some(futuro) => {
                self.em_curso = Some(EmCurso {
                    futuro,
                    pedidos,
                    origem: Origem::Reaberta,
                });
            }
            None => voz.marcar(AberturaDoAudio::SemAparelho),
        }
    }

    /// Espera a abertura em curso. Sem abertura, nunca volta.
    ///
    /// É o braço do `select!` de `drive`: um futuro que nunca volta é um braço
    /// que nunca é escolhido, e o laço não precisa de um `if` para ele.
    async fn chegada(&mut self) -> Aberta<V> {
        match self.em_curso.as_mut() {
            Some(em_curso) => em_curso.futuro.as_mut().await,
            None => std::future::pending().await,
        }
    }

    /// O que fazer com o que a abertura entregou.
    fn chegou(
        &mut self,
        voz: &VozDaSala<V>,
        aberta: Aberta<V>,
        abrir: impl FnOnce(&seele_core::DeviceChoice) -> Futuro<V>,
    ) {
        let Some(em_curso) = self.em_curso.take() else {
            return;
        };
        match aberta {
            Ok(Ok(nova)) => {
                if let Err(nova) = voz.instalar(nova, &em_curso.pedidos, em_curso.origem) {
                    // A pessoa escolheu outro aparelho enquanto este abria. O
                    // que abriu não é mais o pedido, e guardá-lo seria a tela
                    // dizendo ESCOLHIDO num e EM USO noutro.
                    drop(nova);
                    self.comecar(voz, abrir);
                }
            }
            Ok(Err(erro)) => {
                // A metade de texto funciona, e `abertura_do_audio` diz qual
                // metade é esta.
                tracing::warn!(%erro, "o áudio não abriu nesta sala; só texto");
                voz.marcar(AberturaDoAudio::SemAparelho);
            }
            Err(Atraso) => voz.marcar(AberturaDoAudio::NaoAbriuNoPrazo),
        }
    }
}

/// Abre a voz da sala nos aparelhos pedidos, na thread de [`esperar_o_audio`].
///
/// `start_preferring` and not `start_on`: it falls back to the machine's own
/// device, per side, rather than refusing the room. A preference written down
/// last week names a device that may be in another room by now. The screen
/// shows what actually opened, so the fallback is visible — and per side, so a
/// headset left behind does not also throw away the microphone.
fn abrir_a_voz(shared: &Shared, pedidos: &seele_core::DeviceChoice) -> Futuro<Voice> {
    let canal = shared.media.lock().ok().and_then(|slot| slot.clone());
    let pedidos = pedidos.clone();
    // A vez da máquina, num foco de voz (E3): as aberturas de todas as sessões
    // passam uma de cada vez, e a troca do foco espera a que estiver no aparelho.
    let vez = shared.voice.vez();
    let tem_foco = shared.voice.conferidor_do_foco();
    Box::pin(async move {
        let Some((media, ssrc)) = canal else {
            return Ok(Err("sem conexão por onde a voz saia".to_owned()));
        };
        esperar_o_audio(
            &vez,
            move || {
                let aberta = Voice::start_preferring(&pedidos, media, ssrc)
                    .map_err(|erro| format!("{erro:#}"))?;
                // **Sem o foco, largada aqui, sob a vez** (E3): o foco saiu
                // enquanto o aparelho abria, e a sessão que o ganhou espera esta
                // vez para abrir a dela. Duas vozes vivas nunca se cruzam.
                if tem_foco() {
                    Ok(aberta)
                } else {
                    drop(aberta);
                    Err("o foco de voz saiu desta sessão enquanto o aparelho abria".to_owned())
                }
            },
            PRAZO_DO_AUDIO,
        )
        .await
    })
}

/// O assento pelo qual o áudio abre e fecha.
///
/// **O confirmado, e não o especulativo** (M-d da rodada 1 do lote I-a): o
/// assento muda no instante em que a entrada é pedida, e o servidor ainda pode
/// recusá-la. Abrir o áudio por ele abriria o microfone numa sala que vai dizer
/// não. O confirmado chega uma volta de rede depois: o `Pong` do `Ping` marcado
/// que o motor manda atrás do pedido. Ver `seele_core::assento`.
fn sala_do_audio(room: &Room) -> Option<VoiceRoomId> {
    room.sala_confirmada()
}

/// [`sala_do_audio`], lida do `Room` desta sessão.
fn sala_de(shared: &Shared) -> Option<VoiceRoomId> {
    shared
        .room
        .lock()
        .ok()
        .and_then(|room| sala_do_audio(&room))
}

/// O que a volta do enlace faz com o `Room`: adota a sessão nova e refaz o que
/// o motor refez nela.
///
/// `assento` é o de `Aviso::Reconectado`: a sala em que o motor pediu a
/// entrada de novo, ou nenhuma. A sessão nova chega sem assento, o servidor
/// confirma a entrada por silêncio, e a fotografia de ocupação pula a própria
/// pessoa. Medido em `a_volta_mostra_a_pessoa_na_sala_em_que_o_motor_a_sentou_de_novo`:
/// sem refazer, quem caía e voltava era desenhado FORA DE SALA com a voz dentro
/// da sala, e o áudio da pendência 52, que segue o assento, fecharia.
///
/// **A mesma conta do motor, pelo mesmo `seele_core::assento`.** `Room::adopt`
/// guarda o assento que o servidor resgatou (o mesmo `ssrc`), e por cima dele
/// vem o pedido que `restabelecer` mandou: a entrada, no ar até o servidor
/// decidir — e a recusa dela, numa sala com senha, devolve o resgatado —, ou a
/// saída de quem deixou a sala durante a queda.
fn readotar_na_volta(
    room: &mut Room,
    sessao: &seele_core::SessionInfo,
    apelido: &str,
    assento: Option<VoiceRoomId>,
) {
    room.adopt(sessao, apelido);
    match assento {
        Some(sala) => room.enter_voice_room(sala),
        None => room.leave_voice_room(),
    }
}

/// Guarda por onde a voz sai, para quem for reabri-la.
fn remember_media(shared: &Arc<Shared>, media: MediaChannel, ssrc: Ssrc) {
    if let Ok(mut slot) = shared.media.lock() {
        *slot = Some((media, ssrc));
    }
}

/// Dobra uma mensagem do servidor no `Room`, e leva junto o volume de quem
/// trocou de nome. `None` com o cadeado do `Room` envenenado.
///
/// **A troca de nome muda a chave do volume** (M-4 da rodada 1 do lote I-b). O
/// volume é por apelido, e é isso que o faz sobreviver à volta, quando a pessoa
/// ganha outra sessão e outro `ssrc`. `PersonRenamed` traz o `PersonId` e o nome
/// novo, e o nome de antes é o que o `Room` tem para o mesmo `PersonId` —
/// lido antes de o `Room` aprender o novo.
///
/// O volume muda de chave **depois** de soltar o `Room`: a ordem dos cadeados
/// é voz, volumes, `Room` (`VozDaSala::seguir_os_volumes`), e segurar o `Room`
/// enquanto se pega os volumes a inverteria.
fn dobrar_no_room<V>(
    room: &Mutex<Room>,
    voz: &VozDaSala<V>,
    message: &seele_core::ServerMessage,
) -> Option<seele_core::Changed> {
    let (mudou, renomeada) = {
        let mut room = room.lock().ok()?;
        let renomeada = renomeacao(&room, message);
        (room.apply(message), renomeada)
    };
    if let Some((antes, agora)) = renomeada {
        voz.renomear_volume(&antes, &agora);
    }
    Some(mudou)
}

/// O nome de antes e o de agora de quem o servidor anunciou que trocou de nome.
///
/// `None` para qualquer outra mensagem, para uma pessoa que o `Room` não
/// conhece, e para um nome que não mudou.
fn renomeacao(room: &Room, message: &seele_core::ServerMessage) -> Option<(String, String)> {
    let seele_core::ServerMessage::PersonRenamed { person, nickname } = message else {
        return None;
    };
    let antes = &room.people.get(person)?.nickname;
    (antes != nickname).then(|| (antes.clone(), nickname.clone()))
}

/// Folds a server message into the room and tells the shell what moved.
/// Uma mensagem do servidor, dobrada no `Room` — menos a resposta a uma
/// pergunta, que vai a quem perguntou.
///
/// - **O peso** (`ChannelWeighed`) vai à caixa que o pediu. Antes do `fold`,
///   porque é onde ele para: `Room::apply` não tem arm que o guarde, de
///   propósito.
/// - **A recusa de uma pesagem** (`PedidoRecusado` de um `WeighChannel`) vai à
///   mesma caixa, com a causa, e **não vira faixa**: a caixa está onde a pessoa
///   agiu, e as duas juntas diriam a mesma coisa duas vezes. Sem ninguém
///   esperando — uma recusa que chega depois de a caixa fechar —, ela dobra
///   como qualquer aviso, e a faixa diz.
fn receber(shared: &Arc<Shared>, message: &seele_core::ServerMessage) {
    match message {
        seele_core::ServerMessage::ChannelWeighed {
            channel,
            messages,
            authors,
            oldest_at_seconds,
        } => shared.answer_weight(ChannelWeight {
            channel: channel.get(),
            messages: *messages,
            authors: *authors,
            oldest_at_seconds: *oldest_at_seconds,
        }),
        seele_core::ServerMessage::Alert {
            reason: seele_core::AlertReason::PedidoRecusado { pedido, causa },
            ..
        } if seele_core::nome_do_pedido(*pedido) == Some("WeighChannel")
            && shared.recusar_peso((*causa).into()) =>
        {
            return;
        }
        _ => {}
    }
    fold(shared, message);
}

fn fold(shared: &Arc<Shared>, message: &seele_core::ServerMessage) {
    if let seele_core::ServerMessage::ModReply {
        request,
        part,
        total,
        payload,
    } = message
    {
        shared.notify(&Event::ModReply {
            request: *request,
            part: *part,
            total: *total,
            payload: payload.clone(),
        });
        return;
    }
    // O aviso da metade de servidor de um MOD, pelo mesmo caminho da resposta
    // de um pedido: a casca o entrega às janelas daquele MOD (rascunho da API
    // 6, §7.3). O `Room` não o guarda — ele não é estado da sala.
    if let seele_core::ServerMessage::ModAviso {
        mod_id,
        canal,
        carga,
    } = message
    {
        shared.notify(&Event::ModAviso {
            mod_id: mod_id.clone(),
            canal: canal.map(|canal| canal.0),
            carga: carga.clone(),
        });
        return;
    }
    let Some(changed) = dobrar_no_room(&shared.room, &shared.voice, message) else {
        return;
    };

    // A malha do ADR 0036, ligada aqui e não dentro do `Room`.
    //
    // O `Room` é o que se sabe do **servidor**, e encolher o próprio microfone é
    // decisão do lado do áudio; ele não conhece o `Voice` e não deveria. Este é
    // o único lugar que segura os dois.
    //
    // Um servidor v1 nunca manda este quadro, e então nada disto acontece: a
    // faixa fica onde nasceu, que é o teto, e o comportamento é o de antes.
    if let seele_core::ServerMessage::UplinkLoss { fraction } = message {
        if let Ok(voice) = shared.voice.lock() {
            if let Some(voice) = voice.as_ref() {
                if let Some(bps) = voice.observar_perda(*fraction, std::time::Instant::now()) {
                    // Registrado só quando **muda**, que é o que a malha
                    // devolve. Uma linha por medida seriam sessenta por minuto
                    // por sessão, dizendo quase sempre a mesma coisa.
                    tracing::info!(
                        bps,
                        perda = fraction,
                        "a faixa de bitrate mudou por perda de subida"
                    );
                }
            }
        }
    }

    // **Antes do `RosterChanged`**, pelo motivo da revisão dos retratos logo
    // abaixo: a casca relê o FORA DO AR pelo número quando o evento chega, e um
    // número que sobe depois do aviso é um número que ela pode ler velho.
    if changed.membros {
        shared
            .fora_do_ar_revision
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    if changed.roster {
        // **A revisão dos retratos sobe aqui, e não no ramo do servidor.**
        //
        // Ela nasceu ao lado da revisão do retrato **do servidor**, por
        // semelhança — as duas fazem a mesma coisa pelo mesmo motivo. Mas o
        // `ServerIconChanged` marca `changed.server` e o `PersonIconChanged`
        // marca `changed.roster`: são partes diferentes do estado, e o ramo em
        // que a subida morava nunca rodava para ela.
        //
        // O efeito era completo e silencioso. A revisão ficava parada, o cache
        // de retratos da casca nunca invalidava, e **nenhuma** superfície
        // mudava: nem o bloco do operador, nem a grade da chamada, nem a
        // mensagem, e nem para as outras pessoas — o cliente delas tinha o
        // mesmo ramo morto. Só a prévia do próprio diálogo acompanhava, porque
        // ela lê o arquivo desta máquina e não passa por aqui.
        //
        // Antes do aviso, como a do servidor e pelo mesmo motivo: uma casca que
        // reage ao evento leria o número velho e concluiria que não há imagem
        // nova para buscar.
        if matches!(message, seele_core::ServerMessage::PersonIconChanged { .. }) {
            shared
                .person_icons_revision
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        shared.notify(&Event::RosterChanged);
    }
    if changed.messages {
        shared.messages_changed();
    }
    if changed.channels {
        shared.notify(&Event::ChannelsChanged);
    }
    if changed.server {
        // A revisão sobe **antes** do aviso, como em `messages_changed` e pelo
        // mesmo motivo: uma casca que reage ao evento leria o número velho e
        // concluiria que não há imagem nova para buscar.
        //
        // Só quando foi a imagem que mudou. Um nome novo já viaja inteiro no
        // `Snapshot`, e mexer no número faria toda casca rebuscar 8 KiB por
        // causa de uma string que ela acabou de receber.
        if matches!(message, seele_core::ServerMessage::ServerIconChanged { .. }) {
            shared
                .icon_revision
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        // E o das pessoas, pela mesma razão e com a mesma ordem: antes do aviso.
        shared.notify(&Event::ServerChanged);
    }
    // **Os cargos desta sessão** (ADR 0064): o evento leva a marca nova, que o
    // `Room` já subiu ao dobrar a mensagem — é por ela, e não por um laço de
    // leituras, que a camada de cargos sabe quando reler `cargos_na_tela`.
    if changed.cargos {
        let marca = shared.room.lock().map_or(0, |room| room.cargos.marca());
        shared.notify(&Event::CargosChanged { marca });
    }
    // **Os perfis públicos** (0.19, protocolo 12): a marca já subiu no `Room`,
    // antes do aviso, e vai no evento — a casca relê `Connection::perfis` por
    // ela. As recusas saem uma a uma, da mais antiga à mais nova, e a fila fica
    // vazia (duas recusas no ar não se apagam).
    if changed.perfis {
        let marca = shared.room.lock().map_or(0, |room| room.perfis_marca);
        shared.notify(&Event::PerfisChanged { marca });
    }
    if changed.recusa_de_perfil {
        let recusas = shared
            .room
            .lock()
            .map(|mut room| std::mem::take(&mut room.recusas_de_perfil))
            .unwrap_or_default();
        for recusa in recusas {
            shared.notify(&Event::PerfilRecusado {
                motivo: recusa.motivo,
                campos: recusa.campos,
            });
        }
    }
    if changed.banidos {
        shared.notify(&Event::BanidosChanged);
    }
    if changed.telemetry {
        shared.notify(&Event::TelemetryChanged);
    }
    if changed.notice {
        let notice = shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.notice.as_ref().map(aviso_de));
        if let Some(notice) = notice {
            shared.notify(&Event::NoticeRaised { notice });
        }
    }
    // Lido do `Room` de novo, e depois da dobra: o que decide entre «acendeu» e
    // «não acendeu» quando só o roster andou é se há transmissão **agora**.
    if changed.telas || changed.roster {
        // **A própria**, por identidade, e não «a da sala». Ver o ADR 0054: a
        // pergunta desta linha é sobre a transmissão desta pessoa, e pegar a
        // primeira da sala respondia com a de outra.
        let minha = shared
            .room
            .lock()
            .ok()
            .and_then(|room| minha_tela_de(&room, None));
        let ha_transmissao = shared
            .room
            .lock()
            .ok()
            .is_some_and(|room| !transmissoes_de(&room, None).is_empty());
        // O que foi pedido morre com a transmissão. Sem esta linha, a próxima
        // vez que esta pessoa compartilhasse mostraria o que está saindo agora
        // ao lado de um teto que ela escolheu noutra ocasião — dois números
        // lado a lado, o da direita mentindo, e nada na tela dizendo qual.
        if minha.is_none() {
            shared.gravar_pedido_da_tela(None);
            shared.esquecer_a_parada_da_tela();
            // E o codificador com ele: o nome é o daquela transmissão.
            shared.gravar_codificador_da_tela(None);
        }
        // **E a escolha de assistir é esquecida quando a tela escolhida sai do
        // ar.** ADR 0054: sem isto, a escolha ficaria apontando para uma
        // transmissão que já não existe, e a próxima a começar com o mesmo
        // `ScreenId` seria recusada por uma decisão sobre outra tela.
        shared.esquecer_tela_que_saiu();
        if a_tela_mudou(changed, ha_transmissao) {
            shared.notify(&Event::ScreenChanged);
        }
    }
    if changed.ended {
        let reason = shared
            .room
            .lock()
            .ok()
            .and_then(|room| room.ended)
            .map_or(EndReason::LinkLost, |end| end.reason.into());
        shared.notify(&Event::Ended { reason });
    }
}

/// Qual das duas grandezas chamadas "jitter" vai para a tela.
///
/// São duas, com o mesmo nome, e mostrar a errada é pior que não mostrar nada:
///
/// - `chegada_ms` é o **jitter de chegada** da RFC 3550 — a variação do
///   intervalo entre pacotes, medida neste receptor por
///   `seele_audio::jitter`, que alisa por `J += (|D(i-1,i)| - J) / 16`. É ruído
///   da rede, e é o que a pessoa quer saber: quanto menor, melhor;
/// - `profundidade_do_anel_ms` é a **profundidade do anel de reprodução** — a
///   nossa própria reserva contra esse ruído, que o ADR 0028 acabou de dotar de
///   alvo. Quanto maior, mais folga o anel teve. Mostrá-la como "jitter" exibiria
///   a reserva como se fosse o problema, e uma conexão saudável apareceria na
///   tela como ruim.
///
/// A resposta é sempre a primeira, e a segunda é ignorada de propósito. Um
/// parâmetro ignorado parece bobo e não é: ele existe para o segundo número ter
/// de ser **escrito** por quem chama, e para quem trocar os dois de lugar
/// reprovar aqui, num teste desta casa, em vez de na tela de outra pessoa.
///
/// O que a tela lia antes deste conserto não era nenhum dos dois: era o jitter
/// do relatório do servidor, que é `0.0` fixo porque o servidor não tem como medir
/// uma grandeza que só existe no receptor.
fn jitter_para_a_tela(chegada_ms: f32, profundidade_do_anel_ms: f32) -> f32 {
    let _ = profundidade_do_anel_ms;
    chegada_ms
}

/// As duas grandezas chamadas "jitter" que uma volta de telemetria produz, já
/// separadas e com destino escrito no nome.
///
/// Existe como estrutura, e não como uma tupla de dois `f32`, porque uma tupla
/// de dois `f32` na mesma unidade é exatamente a forma que deixa trocar os dois
/// sem nada notar — que é o defeito 3.3 outra vez, com outra roupa.
struct JitterDaVolta {
    /// Vai para o Sync Ratio: quanta reserva o anel de reprodução teve.
    profundidade_do_anel_ms: f32,
    /// Vai para a tela: o jitter de chegada da RFC 3550, medido neste receptor.
    ///
    /// Vazio quando não há fonte nenhuma sendo recebida. `None` e não zero de
    /// propósito: o pior de uma lista vazia seria zero, e zero é o valor que
    /// este conserto existe para tirar da tela.
    chegada_ms: Option<f32>,
}

/// Separa as duas, a partir do que a camada de áudio mediu.
///
/// Apartada de [`measure`] para poder ser afirmada por um teste: `measure`
/// precisa de um [`Enlace`] vivo e de um dispositivo de áudio aberto, e sem esta
/// separação a única coisa que sobrava para testar era a função pura dos dois
/// lados da fiação — que passava verde com a fiação desfeita.
fn jitter_da_volta(telemetria: &seele_core::AudioTelemetry) -> JitterDaVolta {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "milissegundos; f32 é o que a ponte para a casca carrega"
    )]
    let volta = JitterDaVolta {
        profundidade_do_anel_ms: telemetria.worst_jitter_depth_ms() as f32,
        // O pior entre as fontes, como já vale para a perda e para a
        // profundidade: um único interlocutor ruim tem de ficar visível sem a
        // casca ter de varrer a lista.
        //
        // A escolha entre os dois `f64` mora na camada de áudio, com nome que
        // diz o destino, e não aqui: a barra da TUI fazia a mesma escolha por
        // conta própria e escolhia errado, e nada num `f64` avisa qual dos dois
        // ele é.
        chegada_ms: telemetria
            .worst_arrival_jitter_ms()
            .map(|chegada| chegada as f32),
    };
    volta
}

/// Refreshes what the shell reads without asking the server.
///
/// Returns whether anything moved enough to be worth telling a shell about.
///
/// Só a fiação: pega a volta de áudio do laço de voz e o ida-e-volta do enlace,
/// e entrega a [`medir_a_volta`], que é onde as decisões moram e onde elas são
/// afirmáveis — esta função pede um [`Enlace`] vivo e um dispositivo de áudio
/// aberto, e não há nenhum dos dois numa máquina de integração contínua.
fn measure(sync: &mut Signal, client: &Enlace, shared: &Arc<Shared>) -> bool {
    let telemetria = shared
        .voice
        .lock()
        .ok()
        .and_then(|voice| voice.as_ref().map(Voice::telemetry));
    let rtt = client.rtt().unwrap_or_default();
    let rtt_micros = u64::try_from(rtt.as_micros()).unwrap_or(u64::MAX);
    // Na mesma volta que o resto da telemetria, e pelo mesmo motivo: quem lê
    // isto lê quando quiser, e uma medição não precisa virar evento.
    shared.caminho_medido.store(
        client.caminho_medido(),
        std::sync::atomic::Ordering::Relaxed,
    );
    // Diagnóstico só da build de teste do CUA (`SEELE_AUDIO_DE_MENTIRA=1`):
    // sobre a cópia acima, com o lock da voz já solto. Ver `diagnostico_da_voz`.
    #[cfg(all(debug_assertions, feature = "aparelho-de-mentira"))]
    let _ = diagnostico_da_voz::relatar(shared.carimbo(), telemetria.as_ref(), Instant::now());
    medir_a_volta(sync, shared, telemetria.as_ref(), rtt_micros)
}

/// **A vez do relato de recepção** (protocolo 13): uma vez por segundo, o que
/// esta ponta mede da voz que ouve vai ao servidor, que o põe no sinal que os
/// outros veem desta pessoa.
///
/// Só a fiação, como [`measure`]: o que é relatado e quando é do
/// [`seele_core::recepcao::Relator`], e para quem o relato pode ir (um
/// servidor do 12 não o conhece) é do [`Enlace::relatar_recepcao`]. Sem voz
/// chegando não há medida, e nada sai.
///
/// A telemetria é lida de novo aqui, e não aproveitada de `measure`: o relator
/// só a pede na vez dele, uma vez por segundo, e `measure` roda a cada volta.
async fn relatar_a_recepcao(
    relator: &mut seele_core::recepcao::Relator,
    client: &Enlace,
    shared: &Arc<Shared>,
) {
    let relato = relator.vez(Instant::now(), || {
        shared
            .voice
            .lock()
            .ok()
            .and_then(|voice| voice.as_ref().map(Voice::telemetry))
    });
    if let Some(relato) = relato {
        // `Err` é a sessão que acabou, e quem diz isso à casca é o laço.
        // `Ok(false)` é um servidor que não ouve o relato: nada saiu.
        let _ = client.relatar_recepcao(relato).await;
    }
}

/// **Diagnóstico da entrada de voz, só na build de teste do CUA.**
///
/// Existe para medir o buraco de 25/10 — depois de reconectar a sessão A, cerca
/// de 100 s com uns 260 blocos com som a cada 500, e a volta sozinha —, sem
/// supor a causa: jitter e decodificador (silêncio, conforto, fome), anel de
/// reprodução vazio ou a máquina atrasando o laço. Uma linha por sessão a cada
/// 5 s, chave=valor, com o local e cada fonte.
///
/// Com o recurso `aparelho-de-mentira`, num build de debug, **e** com
/// `SEELE_AUDIO_DE_MENTIRA=1`, a mesma porta do `audio_de_mentira.rs` do app.
/// Fora disso este módulo não existe: o `seele-audio` recusa o recurso num
/// build otimizado (`compile_error!`), e sem o recurso o `cfg` tira o módulo e
/// a chamada do `measure`. O guarda `o_diagnostico_da_voz::fora_da_build_de_teste_nao_existe`
/// prende essas duas portas.
///
/// O que falta, e por quê: `recebidos`, `tocados`, `alvo_ms` e `silenciados`
/// estão no `JitterMetrics` de cada fonte, mas só a task do `pipeline` o vê;
/// a [`seele_core::AudioTelemetry`] não os carrega, e trazê-los pede campo
/// novo na `SourceTelemetry` (API pública do `seele-audio`).
#[cfg(all(debug_assertions, feature = "aparelho-de-mentira"))]
mod diagnostico_da_voz {
    use std::collections::HashMap;
    use std::fmt::Write as _;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// No máximo uma linha por sessão neste intervalo.
    pub(super) const A_CADA: Duration = Duration::from_secs(5);

    /// A última linha de cada sessão. O `SessaoId` é fixo do começo ao fim da
    /// conexão, e a geração muda dentro dela: o limite é por sessão.
    static ULTIMA: Mutex<Option<HashMap<u64, Instant>>> = Mutex::new(None);

    /// A mesma variável, e o mesmo valor, que o `audio_de_mentira.rs` do app lê.
    pub(super) fn ligado() -> bool {
        std::env::var("SEELE_AUDIO_DE_MENTIRA").as_deref() == Ok("1")
    }

    /// Se a sessão pode dizer agora, e marca que disse.
    fn na_vez(sessao: u64, agora: Instant) -> bool {
        let Ok(mut ultima) = ULTIMA.lock() else {
            return false;
        };
        let ultima = ultima.get_or_insert_with(HashMap::new);
        match ultima.get(&sessao) {
            Some(antes) if agora.saturating_duration_since(*antes) < A_CADA => false,
            _ => {
                ultima.insert(sessao, agora);
                true
            }
        }
    }

    /// A linha, chave=valor. Pura, para o teste afirmar os campos.
    pub(super) fn linha(
        carimbo: super::Carimbo,
        telemetria: Option<&seele_core::AudioTelemetry>,
    ) -> String {
        let mut linha = format!(
            "diagnóstico da voz: sessao={} geracao={}",
            carimbo.sessao, carimbo.geracao
        );
        let Some(telemetria) = telemetria else {
            linha.push_str(" voz=nenhuma");
            return linha;
        };
        let local = &telemetria.local;
        let _ = write!(
            linha,
            " underruns={} overruns={} erros={} eventos={} catchup={} resyncs={} \
             atraso_ms={:.1} pacing_ppm={:.1} pacing_prof_ms={:.1} pacing_alvo_ms={:.1} \
             pacing_grampos={} pacing_primes={} fontes={}",
            local.playback_underruns,
            local.capture_overruns,
            local.device_errors,
            local.device_events,
            local.playout_catchup_frames,
            local.playout_resyncs,
            local.playout_worst_lateness_ms,
            local.pacing_ppm,
            local.pacing_depth_ms,
            local.pacing_target_ms,
            local.pacing_clamps,
            local.pacing_primes,
            telemetria.sources.len(),
        );
        for fonte in &telemetria.sources {
            let _ = write!(
                linha,
                " | ssrc={} prof_ms={:.1} jitter_ms={:.2} perda={:.4} plc={} comfort={} late={} \
                 rajada={} buraco_pulado={}",
                fonte.ssrc,
                fonte.jitter_depth_ms,
                fonte.jitter_ms,
                fonte.loss_fraction,
                fonte.concealed_frames,
                fonte.comfort_frames,
                fonte.late_frames,
                // Os dois de fora da perda: quadros velhos cortados de uma rajada
                // e lugares de buraco pulados com a fila no teto.
                fonte.descartes_por_rajada,
                fonte.buraco_pulado,
            );
        }
        linha
    }

    /// Diz a linha, se a variável pede e se é a vez desta sessão. Devolve o que
    /// disse, para o teste.
    pub(super) fn relatar(
        carimbo: super::Carimbo,
        telemetria: Option<&seele_core::AudioTelemetry>,
        agora: Instant,
    ) -> Option<String> {
        if !ligado() || !na_vez(carimbo.sessao, agora) {
            return None;
        }
        let linha = linha(carimbo, telemetria);
        tracing::info!("{linha}");
        Some(linha)
    }
}

/// Uma volta de medição: grava o que a casca lê, e diz se vale avisá-la.
///
/// # Por que isto é uma função, e não o corpo de [`measure`]
///
/// Porque a propriedade que este ciclo consertou — o `TelemetryChanged`
/// disparar por variação **só** de jitter — não tinha como ser afirmada dentro
/// de `measure`, e o que sobrava era um guarda que lia o **texto** dela. Esse
/// guarda passa verde com a propriedade quebrada: um braço que grava o jitter e
/// devolve `false` satisfaz tanto «a última linha nomeia `mudou_o_jitter`»
/// quanto «o corpo tem `let mudou_o_jitter = match chegada_ms`».
///
/// Nada aqui precisa de sessão: a telemetria de áudio se monta à mão, o
/// ida-e-volta é um `u64`, e o `Shared` já tinha um construtor de teste. Ler o
/// texto da produção prende a forma da linha; isto prende o que ela faz.
///
/// # As duas grandezas, e os dois destinos
///
/// A profundidade do anel vai para o Sync Ratio, que é o que ela mede — quanta
/// reserva o anel teve. O jitter de chegada vai para a tela, que é o que a
/// pessoa quer saber. Ver [`jitter_para_a_tela`].
///
/// # Sem fonte nenhuma, nada é gravado
///
/// É contrapartida deliberada de não gravar zero: zero é o número que este
/// conserto tirou dali — o relatório do servidor manda `0.0` fixo porque um
/// servidor não tem como medir jitter —, e escrevê-lo quando a voz cai faria a
/// tela afirmar «rede perfeita» sobre uma sessão sem áudio nenhum.
///
/// O preço, dito por escrito porque não estava dito em lugar nenhum: numa sessão
/// viva em que a voz cai no meio, o campo **congela** no último valor em vez de
/// esvaziar. Corrigi-lo pede um terceiro estado na travessia — «medido», «sem
/// medida ainda» e «sem medida agora» —, e nenhuma das duas cascas tem hoje como
/// desenhar a diferença.
fn medir_a_volta(
    sync: &mut Signal,
    shared: &Arc<Shared>,
    telemetria: Option<&seele_core::AudioTelemetry>,
    rtt_micros: u64,
) -> bool {
    let mut jitter_ms = 0.0;
    let mut loss = 0.0;
    let mut chegada_ms = None;
    if let Some(telemetria) = telemetria {
        // Jitter and loss are only observable at the receiver, which is why
        // the server's own numbers are not the ones used here.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a fraction; f32 is what the protocol carries"
        )]
        {
            loss = telemetria.worst_loss_fraction() as f32;
        }
        let volta = jitter_da_volta(telemetria);
        jitter_ms = volta.profundidade_do_anel_ms;
        chegada_ms = volta.chegada_ms;
    }

    let mudou_o_jitter = match chegada_ms {
        Some(chegada) => shared.gravar_jitter_de_chegada(jitter_para_a_tela(chegada, jitter_ms)),
        None => false,
    };

    #[allow(
        clippy::cast_precision_loss,
        reason = "a round trip in microseconds is far below f32's exact range"
    )]
    let ratio = sync.update(SyncInputs {
        rtt_ms: rtt_micros as f32 / 1000.0,
        jitter_ms,
        loss_fraction: loss,
    });

    let previous_ratio = shared.signal.swap(ratio, Ordering::Relaxed);
    let previous_rtt = shared.rtt_micros.swap(rtt_micros, Ordering::Relaxed);

    // A shell redrawing because the round trip moved by a microsecond is a
    // shell redrawing thirty times a second for nothing.
    //
    // O jitter entra nesta conta desde que ele passou a ser um número de
    // verdade. Antes ele era o `0.0` do relatório do servidor e nunca se mexia, e o
    // evento nunca precisou falar dele; agora ele varia sozinho, e sem esta
    // parcela quem depende do evento — em vez de puxar `snapshot` em laço, como
    // o app faz — desenha o número velho até outra coisa mudar.
    previous_ratio != ratio || previous_rtt.abs_diff(rtt_micros) > 1_000 || mudou_o_jitter
}

/// Runs one command. Returns false when the driver should stop.
async fn run_command(client: &Enlace, shared: &Arc<Shared>, command: Command) -> bool {
    match command {
        Command::AceitarPedidosDeMaquina(sim) => {
            tracing::info!(
                com_tela = sim,
                "máquina: esta conexão mostra pedidos de máquina"
            );
            client.aceitar_pedidos_de_maquina(sim);
        }
        Command::VerboDaMaquina(verbo) => {
            let saiu = match verbo {
                VerboDaMaquina::Aceitar {
                    mod_id,
                    servidor,
                    resumo,
                } => client.aceitar_maquina(&mod_id, &servidor, &resumo).await,
                VerboDaMaquina::Recusar {
                    mod_id,
                    servidor,
                    motivo,
                } => client.recusar_maquina(&mod_id, &servidor, motivo).await,
                VerboDaMaquina::Parar {
                    mod_id,
                    servidor,
                    motivo,
                } => client.parar_maquina(&mod_id, &servidor, motivo).await,
            };
            if saiu.is_err() {
                return false;
            }
        }
        Command::PedidoDaMaquina {
            mod_id,
            servidor,
            payload,
            resposta,
        } => {
            // O pedido sai agora; a espera (até 15 s) vai para uma tarefa, pela
            // razão do túnel logo abaixo.
            match client.pedir_da_maquina(&mod_id, &servidor, payload).await {
                Err(falha) => {
                    tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo = falha.nome(), "máquina: o pedido à metade de servidor não saiu");
                    let _ = resposta.send(Err(falha.nome()));
                }
                Ok(espera) => {
                    tokio::spawn(async move {
                        let desfecho = espera.aguardar().await.map_err(|falha| {
                            tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo = falha.nome(), "máquina: o pedido à metade de servidor falhou");
                            falha.nome()
                        });
                        let _ = resposta.send(desfecho);
                    });
                }
            }
        }
        Command::ServirTunel {
            mod_id,
            nome,
            protocolo,
            porta,
            resposta,
        } => match client
            .pedir_servico_de_tunel(&mod_id, &nome, protocolo, porta)
            .await
        {
            Err(recusa) => {
                tunel_servido_recusado_dito(&mod_id, &nome, porta, recusa);
                let _ = resposta.send(Err(types::nome_da_recusa_do_tunel(recusa)));
            }
            Ok(espera) => {
                tokio::spawn(async move {
                    let desfecho = espera.aguardar().await.map_err(|recusa| {
                        tunel_servido_recusado_dito(&mod_id, &nome, porta, recusa);
                        types::nome_da_recusa_do_tunel(recusa)
                    });
                    if desfecho.is_ok() {
                        tracing::info!(mod_id = %mod_id, tunel = %nome, porta, "túnel: a máquina serve");
                    }
                    let _ = resposta.send(desfecho);
                });
            }
        },
        Command::PararTunel { mod_id, nome } => {
            if client.parar_tunel(&mod_id, &nome).await.is_err() {
                return false;
            }
        }
        Command::QuedasDoTunelServido {
            mod_id,
            nome,
            resposta,
        } => {
            let quedas = client
                .quedas_do_tunel_servido(&mod_id, &nome)
                .into_iter()
                .map(|(queda, vezes)| QuedaDeTunel {
                    motivo: types::nome_da_queda(queda),
                    vezes,
                })
                .collect();
            let _ = resposta.send(quedas);
        }
        // O olhar mora no fim da volta do laço de `drive`; aqui só se guarda
        // quem espera por ele.
        Command::EntrarNoTunel {
            mod_id,
            nome,
            resposta,
        } => {
            // O pedido sai agora; a espera (até o prazo do par) vai para uma
            // tarefa. Parada aqui, ela pararia as teclas e os avisos de quem
            // usa por até três segundos, a cada recusa.
            let fins_antes = fins_do_tunel(shared, &mod_id, &nome);
            match client.pedir_entrada_no_tunel(&mod_id, &nome).await {
                Err(recusa) => {
                    tunel_recusado_dito(&mod_id, &nome, recusa);
                    let _ = resposta.send(Err(types::nome_da_recusa_do_tunel(recusa)));
                }
                Ok(espera) => {
                    let shared = Arc::clone(shared);
                    tokio::spawn(async move {
                        let desfecho = match espera.aguardar().await {
                            Ok(porta) => {
                                let aberta = PortaDeTunel {
                                    porta: porta.porta,
                                    preferida: porta.preferida,
                                    desejada: porta.desejada,
                                };
                                tunel_aberto_dito(&mod_id, &nome, aberta);
                                guardar_tunel_aberto(&shared, &mod_id, &nome, aberta, fins_antes);
                                Ok(aberta)
                            }
                            Err(recusa) => {
                                tunel_recusado_dito(&mod_id, &nome, recusa);
                                Err(types::nome_da_recusa_do_tunel(recusa))
                            }
                        };
                        let _ = resposta.send(desfecho);
                    });
                }
            }
        }
        Command::SairDoTunel { mod_id, nome } => {
            if client.sair_do_tunel(&mod_id, &nome).await.is_err() {
                return false;
            }
        }
        Command::SairDosTuneisDoMod { mod_id } => {
            if client.sair_dos_tuneis_do_mod(&mod_id).await.is_err() {
                return false;
            }
        }
        Command::OlharAVoz(resposta) => shared.voice.esperar_o_olhar(resposta),
        Command::LembrarCaminho(bps) => {
            let _ = client.lembrar_o_caminho(bps).await;
        }
        Command::EnterVoiceRoom(voice_room, password) => {
            if client
                .entrar_na_voice_room(voice_room, password)
                .await
                .is_err()
            {
                return false;
            }
            if let Ok(mut room) = shared.room.lock() {
                room.enter_voice_room(voice_room);
            }
            shared.notify(&Event::RosterChanged);
        }
        Command::LeaveVoiceRoom => {
            if client.sair_da_voice_room().await.is_err() {
                return false;
            }
            // O espelho do `EnterVoiceRoom` acima, e ele faltava. O servidor não
            // devolve o `PersonLeft` a quem o causou — «essa pessoa já sabe» —,
            // então esta metade do roster é contabilidade desta casca. Sem ela
            // o assento se esvazia no servidor e em todos os outros clientes, e
            // a única tela que continua desenhando a pessoa na sala é a de
            // quem acabou de sair dela.
            if let Ok(mut room) = shared.room.lock() {
                room.leave_voice_room();
            }
            shared.notify(&Event::RosterChanged);
        }
        Command::OpenChannel(channel) => {
            if client.abrir_linha(channel).await.is_err() {
                return false;
            }
            // Abrir marca como lido, o canal que se deixa e o que se abre, e
            // anota que a página que vem agora é histórico: a cerca (o `Ping`
            // com este carimbo, logo atrás do pedido) é o que diz à sessão
            // onde a página acaba e a conversa ao vivo começa — sem ela, uma
            // menção de ontem levantaria o aviso de agora. Ver
            // `Room::esperar_historico`.
            let cerca = shared.room.lock().ok().map(|mut room| {
                room.open_channel(channel);
                room.esperar_historico(channel)
            });
            // The fetch is what makes "sem perda de histórico" true: a client
            // arriving late reads what was said instead of an empty room.
            let pedido = match cerca {
                Some(carimbo) => {
                    client
                        .historico_com_cerca(channel, None, HISTORY_PAGE, carimbo)
                        .await
                }
                None => client.historico(channel, HISTORY_PAGE).await,
            };
            if pedido.is_err() {
                return false;
            }
            // As não lidas do canal que abriu zeraram, e elas moram na lista
            // de canais.
            shared.notify(&Event::ChannelsChanged);
            // A Linha trocou, então a conversa trocou — mesmo que nenhuma
            // mensagem nova tenha chegado. `Room::open_channel` limpou a lista, e
            // sem isto a tela continuaria mostrando a conversa da Linha
            // anterior sob o nome da nova.
            shared.messages_changed();
        }
        Command::Send {
            channel,
            body,
            replies_to,
            id,
        } => {
            // specs/02-protocolo.md: idempotent by client_msg_id, so a resend
            // after a lost acknowledgement does not post twice.
            //
            // A chave vem no comando — ver [`Command::Send`]. E o corpo já veio
            // conferido contra o teto de bytes, que é o que impede este `dizer`
            // de falhar na codificação e derrubar a sessão inteira por causa de
            // um acento.
            if client
                .dizer(channel, body.trim().to_owned(), replies_to, id)
                .await
                .is_err()
            {
                return false;
            }
        }
        Command::ImagemDeMod {
            pedido,
            bytes,
            resposta,
        } => {
            if client.imagem_de_mod(pedido, bytes, resposta).await.is_err() {
                return false;
            }
        }
        Command::ModRequest {
            request,
            id,
            channel,
            payload,
        } => {
            if client
                .mod_request(request, id, channel, payload)
                .await
                .is_err()
            {
                return false;
            }
        }
        Command::AcaoDeMod {
            request,
            id,
            channel,
            acao,
            payload,
        } => {
            if client
                .acao_de_mod(request, id, channel, acao, payload)
                .await
                .is_err()
            {
                return false;
            }
        }
        Command::Attach(anexo) => {
            if client.anexar(*anexo).await.is_err() {
                return false;
            }
        }
        // O nome é lido do histórico aqui, e não da janela, pela mesma razão do
        // tipo alegado da prévia logo abaixo. Uma recusa não derruba nada: ela
        // vira o motivo do `NotSaved`, e nada é pedido ao servidor.
        Command::SaveAttachment { attachment, folder } => {
            match destino_de(shared, attachment, &folder) {
                Ok((pasta, nome)) => {
                    if client.salvar_anexo(attachment, pasta, nome).await.is_err() {
                        return false;
                    }
                }
                Err(recusa) => shared.notify(&Event::TransferChanged {
                    transfer: Transfer::NotSaved {
                        attachment: attachment.get(),
                        reason: recusa.reason,
                    },
                }),
            }
        }
        Command::SetMuted(on) => {
            if client.muted(on).await.is_err() {
                return false;
            }
        }
        Command::SetTotalIsolation(on) => {
            if client.isolamento(on).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` here, unlike entering a voice room
        // or opening a channel. Those two are facts about this client, which the
        // server confirms by silence; a room is a fact about the server, and the
        // only honest source for it is the server saying it exists. Writing it in
        // optimistically would draw a room for the person who asked even when
        // the server refused them.
        Command::CreateVoiceRoom {
            name,
            limit,
            channel,
        } => {
            if client.criar_voice_room(name, limit, channel).await.is_err() {
                return false;
            }
        }
        Command::CreateChannel { name } => {
            if client.criar_linha(name).await.is_err() {
                return false;
            }
        }
        Command::RenameVoiceRoom { voice_room, name } => {
            if client.renomear_voice_room(voice_room, name).await.is_err() {
                return false;
            }
        }
        Command::RenameChannel { channel, name } => {
            if client.renomear_linha(channel, name).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` here either, and for the
        // sharpest version of the reason above: the server's own name is what
        // every window in the session is drawing. Writing it in optimistically
        // would put the new name on the screen of the one person who is not
        // allowed to change it, and put it there identically whether the server
        // agreed or refused.
        Command::RenameServer { name } => {
            if client.renomear_server(name).await.is_err() {
                return false;
            }
        }
        Command::SetServerIcon { icon } => {
            if client.definir_icone(icon).await.is_err() {
                return false;
            }
        }
        Command::SetPersonIcon { icon } => {
            if client.definir_minha_imagem(icon).await.is_err() {
                return false;
            }
        }
        Command::SetNickname { name } => {
            if client.definir_meu_apelido(name).await.is_err() {
                return false;
            }
        }
        // Nada vai ao `Room` aqui: o perfil novo é o que o servidor devolve no
        // `PerfilMudou` (ou a recusa no `PerfilRecusado`).
        Command::GravarPerfil(mensagem) => {
            if client.gravar_perfil(mensagem).await.is_err() {
                return false;
            }
        }
        Command::TirarImagemDoPerfil { lado } => {
            if client.tirar_imagem_do_perfil(lado).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` for these either, and for a
        // sharper version of the reason above: what a moderation verb changes
        // is somebody **else's** session. The only honest source for "they are
        // gone" is the `PersonLeft` the server sends when it is true. Marking it
        // here would draw a roster the person who pressed the button is alone
        // in believing — and draw it identically whether the server did it or
        // refused, which is the exact difference the button exists to expose.
        Command::KickPerson { person } => {
            if client.expulsar(person).await.is_err() {
                return false;
            }
        }
        // Como moderar: a resposta honesta é a do servidor (`QuadroDeCargos` ou
        // `CargoRecusado`), que o `Room` dobra; nada se marca aqui.
        Command::Cargo(pedido) => {
            if client.cargo(pedido).await.is_err() {
                return false;
            }
        }
        Command::BanPerson {
            person,
            reason,
            expires_at,
        } => {
            if client.banir(person, reason, expires_at).await.is_err() {
                return false;
            }
        }
        Command::RemoveMessage { message } => {
            if client.remover_mensagem(message).await.is_err() {
                return false;
            }
        }
        Command::EditMessage { message, body } => {
            if client.editar_mensagem(message, body).await.is_err() {
                return false;
            }
        }
        Command::UnbanPerson { person } => {
            if client.desbanir(person).await.is_err() {
                return false;
            }
        }
        Command::ListBans => {
            if client.listar_banidos().await.is_err() {
                return false;
            }
        }
        Command::MovePerson { person, voice_room } => {
            if client.mover_pessoa(person, voice_room).await.is_err() {
                return false;
            }
        }
        // Nothing is written into the local `Room` for these either, and here
        // the reason is at its sharpest: a room removed optimistically would
        // vanish off the screen of the person who asked whether or not the
        // server agreed — and the one case where it refuses, the last voice room, is
        // exactly the case they most need to see did not happen.
        Command::DeleteVoiceRoom { voice_room } => {
            if client.apagar_voice_room(voice_room).await.is_err() {
                return false;
            }
        }
        Command::DeleteChannel { channel } => {
            if client.apagar_linha(channel).await.is_err() {
                return false;
            }
        }
        // The question, and where to put the answer. Registered **before** the
        // ask goes out: the server is on the other side of a socket that can be
        // faster than this thread's next channel, and a reply arriving before its
        // slot exists is a reply with nowhere to go.
        Command::WeighChannel { channel, answer } => {
            if let Ok(mut pending) = shared.pending_weights.lock() {
                pending.push((channel, answer));
            }
            if client.pesar_linha(channel).await.is_err() {
                return false;
            }
        }
        // The claim is read out of this client's own history here, before
        // anything is asked of the server, and it never travels through the
        // window: a page that handed it back could hand back a different one,
        // and the whole of ADR 0027's rule is that the claim does not get to
        // choose the decoder.
        Command::PreviewAttachment { attachment, answer } => {
            let claimed = declared_type_of(shared, attachment).unwrap_or_default();
            let Ok(caixa) = client.prever_anexo(attachment).await else {
                return false;
            };
            // In a task of its own, and not awaited here. This queue is what
            // carries every keystroke of this session, and a download parked on
            // it would stop anybody saying anything until the picture came
            // down — the head-of-channel block that ADR 0027 gave each transfer
            // its own stream to avoid.
            tokio::spawn(async move {
                let fetched = caixa.await.ok();
                // Off the async workers: a preview larger than where it is
                // drawn is decoded and reduced here, and that takes time.
                let previa = tokio::task::spawn_blocking(move || {
                    preview_of(attachment.get(), &claimed, fetched)
                })
                .await;
                if let Ok(previa) = previa {
                    let _ = answer.send(previa);
                }
            });
        }
        Command::ShareScreen { pedido, limites } => {
            if client.compartilhar_tela(*pedido, limites).await.is_err() {
                return false;
            }
        }
        Command::AdjustScreenLimits { limites } => {
            if client.ajustar_limites_da_tela(limites).await.is_err() {
                return false;
            }
        }
        Command::Assistir { tela, quero } => {
            // Falhar aqui não derruba a sessão, pela mesma razão do
            // quadro-chave: sem o pedido a imagem não aparece, e com a sessão
            // fechada não aparece nunca mais.
            if let Err(erro) = client.assistir(tela, quero).await {
                tracing::debug!(%erro, quero, "não consegui mudar o que assisto");
            }
        }
        Command::ConsentirNoCaminhoEntrePares {
            pares_que_atende,
            assiste_por_par,
        } => {
            // Falhar aqui não derruba a sessão: sem o consentimento a tela vem
            // do servidor, que é o caminho de sempre — «a malha é alívio, nunca
            // dependência».
            if let Err(erro) = client
                .consentir_no_caminho_entre_pares(seele_core::ConsentimentoDePar {
                    pares_que_atende,
                    assiste_por_par,
                })
                .await
            {
                tracing::debug!(%erro, "não consegui declarar o consentimento de par");
            }
        }
        Command::RequestKeyFrame { tela } => {
            // Falhar aqui não derruba a sessão: sem o quadro-chave a tela
            // alheia demora a aparecer, e com a sessão fechada não aparece
            // nunca mais.
            if let Err(erro) = client.pedir_quadro_chave(tela).await {
                tracing::debug!(%erro, "não consegui pedir um quadro-chave");
            }
        }
        Command::StopScreenShare => {
            if client.parar_de_compartilhar().await.is_err() {
                return false;
            }
        }
        Command::Shutdown => return false,
    }
    true
}

/// O nome que o remetente deu a um anexo, tirado do histórico local.
///
/// Em todos os canais, pela razão de [`declared_type_of`].
fn file_name_of(shared: &Shared, attachment: seele_core::AttachmentId) -> Option<String> {
    let room = shared.room.lock().ok()?;
    room.mensagens.values().flatten().find_map(|message| {
        message
            .attachment
            .as_ref()
            .filter(|anexo| anexo.id == attachment)
            .map(|anexo| anexo.file_name.clone())
    })
}

/// A pasta e o nome com que um anexo seria gravado, ou por que não seria.
///
/// Um lugar só para as duas perguntas — a da frase de confirmação e a da
/// gravação —, para que elas não possam discordar. O nome sai do histórico e
/// passa por [`seele_core::anexo_no_disco::nome_seguro`]; a regra que o pegou,
/// quando pega, vai para o log, e a tela recebe só o motivo.
fn destino_de(
    shared: &Shared,
    attachment: seele_core::AttachmentId,
    folder: &str,
) -> Result<(PathBuf, String), SaveRefused> {
    let Some(alegado) = file_name_of(shared, attachment) else {
        return Err(SaveRefused {
            reason: NotSavedReason::AnexoDesconhecido,
            claimed: String::new(),
            folder: folder.to_owned(),
        });
    };
    let claimed = seele_core::anexo_no_disco::para_mostrar(&alegado);
    // Absoluta, e não só presente: uma pasta relativa grava onde quer que o
    // processo tenha sido iniciado, num lugar que a frase não consegue nomear.
    let pasta = PathBuf::from(folder);
    if folder.is_empty() || !pasta.is_absolute() {
        return Err(SaveRefused {
            reason: NotSavedReason::SemPasta,
            claimed,
            folder: folder.to_owned(),
        });
    }
    if let Err(regra) = seele_core::anexo_no_disco::nome_seguro(&alegado) {
        tracing::warn!(
            %attachment,
            nome = %claimed,
            %regra,
            "o nome que veio com o anexo não é só um nome; nada foi gravado"
        );
        return Err(SaveRefused {
            reason: NotSavedReason::NomeRecusado,
            claimed,
            folder: folder.to_owned(),
        });
    }
    Ok((pasta, alegado))
}

/// The type the sender claimed for one attachment, out of the local history.
fn declared_type_of(shared: &Shared, attachment: seele_core::AttachmentId) -> Option<String> {
    let room = shared.room.lock().ok()?;
    // Em todos os canais, e não só no aberto: um arquivo pode ter sido pedido
    // num canal e o download terminar depois de a pessoa trocar de canal.
    room.mensagens.values().flatten().find_map(|message| {
        message
            .attachment
            .as_ref()
            .filter(|anexo| anexo.id == attachment)
            .map(|anexo| anexo.declared_type.clone())
    })
}

/// Turns fetched bytes into the verdict a window acts on.
///
/// Every branch that produces a picture goes through
/// [`seele_core::preview::judge`], and the media type in the URI comes out of
/// what that function found in the bytes. `claimed` is only ever quoted back
/// into a sentence.
fn preview_of(
    attachment: u64,
    claimed: &str,
    fetched: Option<seele_core::enlace::Previa>,
) -> Preview {
    use seele_core::enlace::Previa;
    use seele_core::preview::{judge, Verdict};
    use seele_core::reducao::{previa_na_janela, PreviaNaJanela, TETO_DE_PIXELS};

    let refused = |refusal: PreviewRefusal, found: Option<String>| Preview {
        attachment,
        image: None,
        claimed: claimed.to_owned(),
        found,
        refusal: Some(refusal),
    };

    match fetched {
        None | Some(Previa::NaoVeio) => refused(PreviewRefusal::DidNotArrive, None),
        Some(Previa::GrandeDemais { .. }) => refused(
            PreviewRefusal::TooBig {
                limit: seele_core::PREVIEW_LIMIT,
            },
            None,
        ),
        Some(Previa::Bytes(bytes)) => match judge(claimed, &bytes) {
            // **Reduced to where it is drawn before it crosses the bridge**,
            // and refused from its header when it declares too many pixels.
            // The byte limit bounds the download and nothing else: the picture
            // went to the window at the size it came, and the conversation,
            // rebuilt on every message, wrote the whole `data:` URI into a new
            // `<img>` each time (the sweep after the 4K cover, 11/11/2026).
            // `found` stays what the sender's bytes were; the URI carries the
            // type of the bytes that actually cross.
            Verdict::Draw(format) => {
                let found = Some(format.media_type().to_owned());
                match previa_na_janela(bytes) {
                    PreviaNaJanela::Pronta(uri) => Preview {
                        attachment,
                        image: Some(uri),
                        claimed: claimed.to_owned(),
                        found,
                        refusal: None,
                    },
                    PreviaNaJanela::PixelsDemais { largura, altura } => refused(
                        PreviewRefusal::TooManyPixels {
                            width: largura,
                            height: altura,
                            limit: TETO_DE_PIXELS,
                        },
                        found,
                    ),
                    PreviaNaJanela::Ilegivel => refused(PreviewRefusal::Unreadable, found),
                }
            }
            Verdict::Disagrees { found, .. } => refused(
                PreviewRefusal::Disagrees,
                found.map(|format| format.media_type().to_owned()),
            ),
            Verdict::NotAPicture => refused(PreviewRefusal::NotAPicture, None),
        },
    }
}

/// A monotonic identifier for outgoing messages.
///
/// Process-wide rather than per-`Connection`: two handles in one process sending the
/// same number would collide in the server's idempotency check, and the second
/// message would be silently dropped as a resend of the first.
/// O relógio local, em segundos desde a época.
///
/// Só para uma pendente: ela é a única coisa nesta ponte cujo instante não vem
/// do servidor, porque o servidor ainda não sabe que ela existe. Uma mensagem
/// confirmada carrega o relógio de quem a aceitou, e é o certo — dois clientes
/// com relógios diferentes concordariam sobre a ordem e discordariam sobre a
/// hora.
fn agora_em_segundos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |desde| {
            i64::try_from(desde.as_secs()).unwrap_or(i64::MAX)
        })
}

fn next_client_message_id() -> u64 {
    // Drawn at process start, not counted from one — and the difference is a
    // defect, not tidiness.
    //
    // `Messages::append_batch` deduplicates on `(author_id, client_message_id)`.
    // `author_id` comes from the Ed25519 key on disk (ADR 0004) and never
    // changes; this counter used to restart at 1 on every launch. So the first
    // message after reopening the app was read as a retry of the first message
    // of the previous launch and **was never written**, with neither end told
    // (pendency 19). Process-wide was never the scope that mattered: the reuse
    // happened *between* processes.
    //
    // The high half is random and the low half counts, which leaves four
    // billion messages before the two halves could meet.
    static NEXT: std::sync::LazyLock<std::sync::atomic::AtomicU64> =
        std::sync::LazyLock::new(|| {
            std::sync::atomic::AtomicU64::new((u64::from(rand::random::<u32>()) << 32) | 1)
        });
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Por que a sessão acabou, como a casca o lê, a partir do que o enlace disse.
///
/// Fora de `drive` para ter teste: é a única tradução entre o fim que o núcleo
/// enumera e o `EndReason` que chega à tela e ao retrato.
fn fim_da_sessao(motivo: &seele_core::enlace::Motivo) -> EndReason {
    match motivo {
        seele_core::enlace::Motivo::Descarregou | seele_core::enlace::Motivo::Pedido => {
            EndReason::LinkLost
        }
        seele_core::enlace::Motivo::Recusado(erro) => fim_da_recusa(erro),
        // O motivo enumerado que o servidor mandou, e não uma tradução dele:
        // quem foi expulso tem de ler «expulso». Cair aqui em
        // `CredentialRejected` mandaria a pessoa conferir uma senha que está
        // certa — a mesma armadilha que o doc de
        // `DisconnectReason::NicknameTaken` registra.
        seele_core::enlace::Motivo::Moderado(porque) => (*porque).into(),
    }
}

/// O fim de uma sessão cuja volta pela bateria foi recusada.
///
/// Era `CredentialRejected` para toda recusa, e a tela mandava conferir convite
/// e senha a quem voltou para um servidor que não é o do convite, cuja chave
/// mudou, que lotou ou cujos MODs mudaram. A recusa já vinha tipada do núcleo
/// ([`seele_core::enlace::Motivo::Recusado`]); só esta tradução a jogava fora.
///
/// # `match` exaustivo, e sem braço `_`
///
/// Pela regra que `enlace::alguem_respondeu` escreve: uma variante nova de
/// `ConnectError` não compila até alguém dizer o que ela vira na tela.
///
/// As de rede não chegam aqui hoje — `enlace::vale_insistir` as devolve à
/// bateria —, e cada uma está nomeada para que uma mudança lá não as faça
/// chegar como credencial.
fn fim_da_recusa(erro: &seele_core::ConnectError) -> EndReason {
    match erro {
        seele_core::ConnectError::Refused { reason } => (*reason).into(),
        // Sem as impressões: `EndReason` é `Copy` e chega ao retrato. Elas
        // ficam no `seele.log`, na linha da volta recusada.
        seele_core::ConnectError::InviteMismatch { .. }
        | seele_core::ConnectError::ChaveNaoProvada => EndReason::ServidorTrocouNaVolta,
        seele_core::ConnectError::PinChanged { .. } => EndReason::ChaveMudouNaVolta,
        // Na volta de uma ligação, quem atendeu não é mais a sala do código: o
        // mesmo que o servidor que trocou, ou, com o pino, a chave que mudou.
        seele_core::ConnectError::CodigoNaoConfere(falha) => match falha {
            seele_core::confere_codigo::FalhaDoCodigo::SePassouPor { .. } => {
                EndReason::ChaveMudouNaVolta
            }
            seele_core::confere_codigo::FalhaDoCodigo::NaoEhChaveDePessoa { .. }
            | seele_core::confere_codigo::FalhaDoCodigo::CodigoNaoBate { .. } => {
                EndReason::ServidorTrocouNaVolta
            }
        },
        // Na volta, o anúncio que esta máquina não aceitou é o servidor que
        // passou a exigir outro conjunto no meio da sessão.
        seele_core::ConnectError::ModsNaoAceitos { .. } => EndReason::ModsMudaram,
        // Na volta, o servidor passou a falar outro número: foi atualizado, ou
        // trocado por outra versão, no meio da sessão.
        seele_core::ConnectError::OutraLinha { .. } => EndReason::Incompatible,
        // Não chega à volta hoje: ela é ao `IP:porta` que atendeu, e não ao
        // código. Se chegar, é a conversa que caiu, como as de rede. E ninguém
        // cancela uma volta: o `Cancelamento` é da chegada.
        seele_core::ConnectError::NaoLocalizou(_)
        | seele_core::ConnectError::Cancelada
        | seele_core::ConnectError::LocalEndpoint
        | seele_core::ConnectError::Unreachable
        | seele_core::ConnectError::TlsRefused
        | seele_core::ConnectError::HandshakeTimeout
        | seele_core::ConnectError::SemResposta
        | seele_core::ConnectError::NatNaoAbriu
        | seele_core::ConnectError::ProtocolViolation => EndReason::LinkLost,
        // Na volta, o servidor virou uma 0.16: a linha 1 é a do launcher.
        seele_core::ConnectError::LinhaLegada => EndReason::Incompatible,
    }
}

/// O que o braço `Aviso::Mensagem` de `drive` faz com a mensagem: dobra no
/// `Room` ([`receber`]) e, se ela é uma despedida recuperável, avisa a casca
/// com [`Event::CaiuComMotivo`], carimbado pela geração que caiu.
fn receber_a_mensagem_do_motor(shared: &Arc<Shared>, message: &seele_core::ServerMessage) {
    receber(shared, message);
    if let Some(caiu) = caiu_com_motivo(message) {
        shared.notify(&caiu);
    }
}

/// O [`Event::CaiuComMotivo`] de uma mensagem, se ela é uma despedida
/// recuperável.
///
/// A regra do que acaba e do que volta é a única, `a_sessao_acabou_aqui`: uma
/// despedida que acaba vira `Ended` pelo `Room`, e esta não a repete.
fn caiu_com_motivo(mensagem: &seele_core::ServerMessage) -> Option<Event> {
    match mensagem {
        seele_core::ServerMessage::Disconnecting { reason }
            if !seele_core::despedida::a_sessao_acabou_aqui(*reason) =>
        {
            Some(Event::CaiuComMotivo {
                reason: (*reason).into(),
            })
        }
        _ => None,
    }
}

/// Relabels a core connection failure for the shell.
///
/// A relabel and nothing more: the core already decided *what* went wrong, and
/// it decided it from typed causes rather than from the text of somebody else's
/// error message.
fn classify_connect_failure(error: &seele_core::ConnectError) -> ConnectionError {
    match error {
        seele_core::ConnectError::LocalEndpoint | seele_core::ConnectError::Unreachable => {
            ConnectionError::Unreachable
        }
        seele_core::ConnectError::TlsRefused | seele_core::ConnectError::ProtocolViolation => {
            ConnectionError::Refused {
                reason: EndReason::Incompatible,
            }
        }
        seele_core::ConnectError::OutraLinha { numero } => {
            // **A relação, e não só o nome.** A versão só vai quando é uma que
            // o launcher abre, uma de antes desta build; acima dela, a tabela
            // diria «1» de um servidor que não é uma 1.x (ver o campo).
            let linha = seele_core::linha_de(*numero);
            ConnectionError::OutraLinha {
                numero: *numero,
                // Na 0.17 a linha 1 (o 9) também é anterior: é a que o
                // launcher abre ao lado (`Linha::e_anterior`, versao.md).
                servidor: if linha.is_some_and(|linha| linha.e_anterior()) {
                    seele_core::linha_do_protocolo(*numero).map(str::to_owned)
                } else {
                    None
                },
                mais_nova: matches!(linha, Some(seele_core::Linha::Posterior { .. })),
            }
        }
        seele_core::ConnectError::PinChanged { pinned, offered } => ConnectionError::PinChanged {
            pinned: pinned.clone(),
            offered: offered.clone(),
        },
        seele_core::ConnectError::InviteMismatch { expected, offered } => {
            ConnectionError::InviteMismatch {
                expected: expected.clone(),
                offered: offered.clone(),
            }
        }
        seele_core::ConnectError::CodigoNaoConfere(falha) => match falha {
            seele_core::confere_codigo::FalhaDoCodigo::NaoEhChaveDePessoa { esperado } => {
                ConnectionError::CodigoNaoBate {
                    esperado: esperado.to_string(),
                    veio: None,
                }
            }
            seele_core::confere_codigo::FalhaDoCodigo::CodigoNaoBate { esperado, veio } => {
                ConnectionError::CodigoNaoBate {
                    esperado: esperado.to_string(),
                    veio: Some(veio.to_string()),
                }
            }
            seele_core::confere_codigo::FalhaDoCodigo::SePassouPor { codigo } => {
                ConnectionError::SePassouPorCodigo {
                    codigo: codigo.to_string(),
                }
            }
        },
        seele_core::ConnectError::NaoLocalizou(falha) => ConnectionError::NaoLocalizou {
            motivo: (*falha).into(),
        },
        seele_core::ConnectError::Cancelada => ConnectionError::Cancelada,
        // Era `TlsRefused`, e caía no braço de cima: o impostor que repete o
        // certificado sem ter a chave aparecia como versão incompatível, e a
        // tela mandava atualizar o SEELE nas duas máquinas.
        seele_core::ConnectError::ChaveNaoProvada => ConnectionError::ChaveNaoProvada,
        // Não é uma falha traduzida: é a pergunta do ADR 0045 atravessando
        // inteira, com o que a tela precisa para fazê-la.
        seele_core::ConnectError::ModsNaoAceitos {
            mods,
            conjunto,
            plataformas,
        } => ConnectionError::ModsNaoAceitos {
            mods: mods
                .iter()
                .cloned()
                .map(|anunciado| {
                    // A marca vem do quadro que o servidor 12 manda antes do
                    // anúncio, juntada pelo hash; sem o quadro (servidor 11) ou
                    // sem o hash nele, o campo fica ausente.
                    let plataformas = plataformas
                        .as_deref()
                        .and_then(|lista| lista.iter().find(|p| p.hash == anunciado.hash));
                    crate::types::ModExigido::from(anunciado).com_plataformas(plataformas)
                })
                .collect(),
            conjunto: conjunto.clone(),
        },
        seele_core::ConnectError::HandshakeTimeout => ConnectionError::HandshakeTimeout,
        seele_core::ConnectError::SemResposta => ConnectionError::SemResposta,
        seele_core::ConnectError::NatNaoAbriu => ConnectionError::NatNaoAbriu,
        seele_core::ConnectError::LinhaLegada => ConnectionError::LinhaLegada,
        seele_core::ConnectError::Refused { reason } => ConnectionError::Refused {
            reason: (*reason).into(),
        },
    }
}

#[path = "testes/a_volta_recusada_diz_o_motivo.rs"]
#[cfg(test)]
mod a_volta_recusada_diz_o_motivo;

#[path = "testes/a_identidade_e_uma_por_maquina.rs"]
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod a_identidade_e_uma_por_maquina;

#[path = "testes/tests.rs"]
#[cfg(test)]
mod tests;

#[path = "testes/aviso_de_mensagens.rs"]
#[cfg(test)]
mod aviso_de_mensagens;

#[path = "testes/previa.rs"]
#[cfg(test)]
mod previa;

/// O log que responde «qual dos quatro deu o quê».
#[path = "testes/uma_pendente_nao_e_uma_mensagem_do_servidor.rs"]
#[cfg(test)]
mod uma_pendente_nao_e_uma_mensagem_do_servidor;

#[cfg(test)]
mod trilha_no_log {
    use super::*;

    /// Um assinante de `tracing` que guarda o que foi escrito.
    ///
    /// Escrito à mão, e não com `tracing-subscriber`: o que se quer aqui é o
    /// nome dos campos de um `event!`, e trazer um crate inteiro para lê-los
    /// custaria mais do que as vinte linhas abaixo.
    #[derive(Clone, Default)]
    pub(super) struct Espiao(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

    impl Espiao {
        pub(super) fn escrito(&self) -> Vec<String> {
            self.0
                .lock()
                .map(|linhas| linhas.clone())
                .unwrap_or_default()
        }
    }

    /// Cada campo do evento como `nome=valor`, na ordem em que foi escrito.
    struct Anotador(String);

    impl tracing::field::Visit for Anotador {
        fn record_debug(&mut self, campo: &tracing::field::Field, valor: &dyn std::fmt::Debug) {
            self.0.push_str(&format!("{}={valor:?} ", campo.name()));
        }
    }

    impl tracing::Subscriber for Espiao {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        fn event(&self, evento: &tracing::Event<'_>) {
            let mut anotador = Anotador(String::new());
            evento.record(&mut anotador);
            if let Ok(mut linhas) = self.0.lock() {
                linhas.push(anotador.0);
            }
        }

        fn enter(&self, _: &tracing::span::Id) {}

        fn exit(&self, _: &tracing::span::Id) {}
    }

    /// **Um segundo despachante, vivo o processo inteiro, que não escreve
    /// nada.** A cópia deste crate do auxiliar do app
    /// (`apps/seele-app/src/main.rs`, `rastro_de_teste`), chamada antes de o
    /// [`Espiao`] ouvir (pendência 56).
    ///
    /// O `tracing` decide uma vez só, na primeira passagem de qualquer thread,
    /// se uma linha que escreve (o *callsite*) tem quem a ouça, e guarda a
    /// resposta. Se, no último registro de despachante, só havia um vivo, a
    /// pergunta vai só ao despachante da **thread que passou** (`tracing-core`
    /// 0.1.36, `callsite.rs`: `Dispatchers::rebuilder` devolve
    /// `Rebuilder::JustOne`). Um teste sem captura que passasse primeiro por
    /// uma linha, com o espião como único despachante de pé, a deixava marcada
    /// como «nunca» — e a mesma linha não chegava ao espião. Com dois vivos, a
    /// pergunta vai a todos, e cada evento pergunta de novo à thread em que
    /// acontece.
    pub(super) fn segundo_despachante() {
        static SEMPRE: std::sync::OnceLock<tracing::Dispatch> = std::sync::OnceLock::new();
        SEMPRE.get_or_init(|| tracing::Dispatch::new(tracing::subscriber::NoSubscriber::new()));
    }

    /// Marca o processo filho da prova abaixo: com ela, a prova roda a
    /// [`ordem_forcada`] em vez de chamar a si mesma.
    const PROVA_NO_FILHO: &str = "PROVA56_FILHO";

    /// **Uma linha que outra thread tocou primeiro, fora de qualquer captura,
    /// ainda chega ao espião.** A prova de ordem forçada da pendência 56, a do
    /// app, para a cópia de [`segundo_despachante`] deste crate, rodada num
    /// processo filho.
    ///
    /// **Por que num processo filho.** A ordem forçada só decide quando o
    /// espião dela é o único despachante vivo: é aí que o `tracing` pergunta só
    /// à thread que passou. Rodada junto com o teste do log da trilha, cujo
    /// espião fica de pé ao mesmo tempo, ela passava mesmo sem o auxiliar (5 em
    /// 5 rodadas deste módulo, em 09/10). O filho roda só ela (`--exact`,
    /// `--test-threads=1`), e ali o espião é o único. O pai exige `1 passed`,
    /// porque um `--exact` que não acha o teste sai com sucesso e `0 passed`, e
    /// a prova passaria sem ter rodado.
    #[test]
    fn uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada() {
        if std::env::var_os(PROVA_NO_FILHO).is_some() {
            return ordem_forcada();
        }
        let saida =
            std::process::Command::new(std::env::current_exe().expect("o binário destes testes"))
                .args([
                    "--exact",
                    "trilha_no_log::uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada",
                    "--test-threads=1",
                ])
                .env(PROVA_NO_FILHO, "1")
                .output()
                .expect("o processo filho da prova não subiu");
        let fala = String::from_utf8_lossy(&saida.stdout);
        assert!(
            saida.status.success() && fala.contains("ok. 1 passed;"),
            "a ordem forçada reprovou no processo filho, onde o espião é o único despachante \
             vivo, ou o filtro não achou a prova e ela não rodou: {fala}{}",
            String::from_utf8_lossy(&saida.stderr)
        );
    }

    /// A outra thread toca a linha primeiro, com o espião já de pé, e a linha
    /// ainda chega ao espião. E a linha da outra thread continua fora: o
    /// espião segue sendo só da thread corrente.
    ///
    /// Só decide com o espião dela como único despachante vivo, e por isso
    /// roda no processo filho da prova acima.
    fn ordem_forcada() {
        fn diz() {
            tracing::warn!("linha tocada primeiro fora da captura");
        }
        segundo_despachante();
        let espiao = Espiao::default();
        tracing::subscriber::with_default(espiao.clone(), || {
            std::thread::spawn(diz)
                .join()
                .expect("a outra thread terminou");
            diz();
        });

        let escrito = espiao.escrito();
        assert!(
            escrito
                .iter()
                .any(|linha| linha.contains("linha tocada primeiro fora da captura")),
            "uma linha que outra thread tocou primeiro sumiu do espião, e o guarda do log da \
             trilha passa ou falha conforme a ordem das threads: {escrito:?}"
        );
        assert_eq!(
            escrito.len(),
            1,
            "a linha da outra thread chegou a este espião, e um teste passaria a ler o que \
             outro teste escreveu: {escrito:?}"
        );
    }

    #[test]
    fn a_escolha_da_pessoa_vira_degrau_arredondando_para_baixo() {
        // §5: o que se escolhe é o **máximo**. Arredondar para cima seria a
        // ponte entregando mais do que a pessoa pediu — 1080p a quem escolheu
        // 800 —, que é a regra ao contrário e o `spikes/tela-no-transporte`
        // de volta.
        let escolha = |altura, quadros| {
            limites_do_nucleo(LimitesDeTela {
                banda_bps: Some(1_200_000),
                altura_maxima: altura,
                quadros_maximos: quadros,
                prioridade: crate::types::Prioridade::Nitidez,
                com_som: true,
            })
        };

        // Os três da lista fechada caem em si mesmos.
        assert_eq!(escolha(1080, 30).resolucao, seele_core::Resolucao::P1080);
        assert_eq!(escolha(720, 15).resolucao, seele_core::Resolucao::P720);
        assert_eq!(escolha(540, 8).resolucao, seele_core::Resolucao::P540);
        assert_eq!(escolha(1080, 30).cadencia, seele_core::Cadencia::Q30);
        assert_eq!(escolha(1080, 15).cadencia, seele_core::Cadencia::Q15);
        assert_eq!(escolha(1080, 8).cadencia, seele_core::Cadencia::Q8);

        // Entre dois degraus, o de baixo.
        assert_eq!(escolha(900, 29).resolucao, seele_core::Resolucao::P720);
        assert_eq!(escolha(900, 29).cadencia, seele_core::Cadencia::Q15);

        // Abaixo do menor, o menor: 540p é o piso da lista e não há uma quarta
        // opção — abaixo dele o encoder deixa de conseguir gastar o orçamento.
        assert_eq!(escolha(240, 1).resolucao, seele_core::Resolucao::P540);
        assert_eq!(escolha(240, 1).cadencia, seele_core::Cadencia::Q8);

        // E o teto de banda atravessa como está: quem o converte em degrau é o
        // `TetoDeVideo`, e não esta travessia.
        assert_eq!(escolha(720, 30).banda_bps, Some(1_200_000));
    }

    #[test]
    fn o_modulo_de_video_e_procurado_onde_a_casca_o_poe() {
        // A lista vem daqui porque `procurar_em` a pede de fora — «onde os
        // arquivos do produto moram é decisão da casca» —, e uma lista vazia
        // faria o botão de compartilhar responder «este build não sabe» numa
        // máquina que tem o módulo ao lado do executável.
        //
        // A variável de ambiente não é mexida aqui de propósito: ela é global ao
        // processo, e uma bateria que roda em paralelo não tem como emprestá-la.
        let pastas = pastas_do_modulo();
        let perto = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
            .expect("o executável do teste");

        assert!(
            pastas.contains(&perto),
            "sem a pasta do executável, um binário empacotado não acharia o \
             módulo que veio com ele: {pastas:?}"
        );
        assert!(
            pastas.contains(&perto.join("..")),
            "sem a pasta acima, uma árvore de fonte não acharia o módulo em \
             `target/`: {pastas:?}"
        );
    }

    #[test]
    fn uma_ligacao_que_nao_achou_a_sala_atravessa_com_o_motivo() {
        // A tarefa 6c: cada motivo é uma frase diferente na tela (spec de
        // 23/09), e a casca a escolhe pelo nome que atravessa. Achatar o motivo
        // aqui, num `Unreachable`, mandaria quem liga procurar defeito de rede
        // quando a pessoa só está com o SEELE fechado.
        use seele_core::chegada::Etapa;
        use seele_core::encontro::FalhaAoLocalizar as Nucleo;

        for falha in [
            Nucleo::SemPonto,
            Nucleo::NinguemMora,
            Nucleo::Enc2Calado,
            Nucleo::PontoRecusou,
            Nucleo::PontoMudo,
            Nucleo::PontoNaoResolve,
            Nucleo::SoAEscuta,
        ] {
            let etapa = Etapa::Desistiu(seele_core::ConnectError::NaoLocalizou(falha));
            let Ok(json) = serde_json::to_string(&ConnectStage::from(&etapa)) else {
                panic!("a desistência de uma ligação não serializa");
            };
            assert_eq!(
                json,
                format!(
                    r#"{{"Desistiu":{{"motivo":{{"NaoLocalizou":{{"motivo":"{falha:?}"}}}}}}}}"#
                ),
                "o motivo de não achar a sala atravessou com outro nome"
            );
        }

        // E a etapa que pergunta ao quarto atravessa com o ponto, que é o
        // «onde» do passo no log.
        let localizando = Etapa::Localizando {
            ponto: "encontro.exemplo:8384".into(),
        };
        assert_eq!(
            ConnectStage::from(&localizando),
            ConnectStage::Localizando {
                ponto: "encontro.exemplo:8384".into()
            }
        );
        assert_eq!(
            campos_do_passo(&localizando),
            (Some("encontro.exemplo:8384".into()), None, None)
        );
    }

    #[test]
    fn o_toque_atravessa_com_o_candidato_e_o_prazo() {
        // Tarefa 16 da F7. A casca desenha o CHAMANDO a partir desta etapa, e
        // lê o prazo pelo nome do campo.
        use seele_core::chegada::Etapa;
        let tocando = Etapa::Tocando {
            candidato: 1,
            prazo_s: 45,
        };
        let atravessou = ConnectStage::from(&tocando);
        assert_eq!(
            atravessou,
            ConnectStage::Tocando {
                candidato: 1,
                prazo_s: 45
            }
        );
        assert_eq!(atravessou.nome(), "Tocando");
        assert_eq!(
            serde_json::to_value(&atravessou).ok(),
            Some(serde_json::json!({ "Tocando": { "candidato": 1, "prazo_s": 45 } }))
        );
        assert_eq!(campos_do_passo(&tocando), (None, Some(1), None));
    }

    #[test]
    fn o_cancelamento_atravessa_como_etapa_e_como_erro_proprios() {
        // Tarefa 16 da F7. A casca lê os dois pelo nome: a etapa no `ETAPAS`, e
        // o erro no `FRASES`, que o `fraseDeErro` procura pela palavra que o
        // serde escreve para uma variante unitária.
        use seele_core::chegada::Etapa;
        let cancelada = ConnectStage::from(&Etapa::Cancelada);
        assert_eq!(cancelada, ConnectStage::Cancelada);
        assert_eq!(cancelada.nome(), "Cancelada");
        assert_eq!(campos_do_passo(&Etapa::Cancelada), (None, None, None));
        let erro = classify_connect_failure(&seele_core::ConnectError::Cancelada);
        assert_eq!(erro, ConnectionError::Cancelada);
        assert_eq!(
            serde_json::to_value(&erro).ok(),
            Some(serde_json::json!("Cancelada"))
        );
    }

    #[test]
    fn o_log_da_trilha_diz_qual_candidato_deu_o_que() {
        // Enquanto o app entrar por `Connection::connect`, este log é a superfície
        // inteira em que a trilha aparece. Ele escrevia só a etapa e o
        // relógio, e com isso respondia «Parada, Tentando, Desistiu, aos
        // 8003 ms» — que é a etapa e não o candidato, ou seja, não é a
        // pergunta que esta tarefa existe para responder.
        use seele_core::chegada::{Etapa, Passo};
        use std::time::Duration;

        let onde = std::net::SocketAddr::from(([203, 0, 113, 7], 8383));
        let trilha = vec![
            Passo {
                etapa: Etapa::Parada {
                    candidatos: 4,
                    com_bilhete_e_impressao: true,
                },
                em: Duration::ZERO,
            },
            Passo {
                etapa: Etapa::Avisando {
                    ponto: "encontro.exemplo:8384".into(),
                },
                em: Duration::from_millis(3),
            },
            Passo {
                etapa: Etapa::Tentando {
                    candidato: 2,
                    de: 4,
                    onde,
                    avisou: true,
                },
                em: Duration::from_millis(8003),
            },
        ];

        // Sem ele, uma linha da trilha pode sumir quando a bateria roda em
        // paralelo (pendência 56): ver `segundo_despachante`.
        segundo_despachante();
        let espiao = Espiao::default();
        tracing::subscriber::with_default(espiao.clone(), || registrar_trilha(&trilha));

        let escrito = espiao.escrito();
        assert_eq!(
            escrito.len(),
            trilha.len(),
            "um passo por linha, e saíram {} linhas: {escrito:?}",
            escrito.len()
        );

        let Some(tentativa) = escrito.iter().find(|linha| linha.contains("Tentando")) else {
            panic!("a tentativa não foi registrada: {escrito:?}");
        };
        assert!(
            tentativa.contains("203.0.113.7:8383"),
            "o log não diz qual endereço foi tentado: {tentativa}"
        );
        assert!(
            tentativa.contains("candidato=Some(2)"),
            "o log não diz qual dos candidatos era: {tentativa}"
        );
        assert!(
            tentativa.contains("de=Some(4)"),
            "o log diz o candidato e esconde de quantos, que é metade da \
             resposta: {tentativa}"
        );

        let Some(aviso) = escrito.iter().find(|linha| linha.contains("Avisando")) else {
            panic!("o aviso não foi registrado: {escrito:?}");
        };
        assert!(
            aviso.contains("encontro.exemplo:8384"),
            "o «onde» de um aviso é o ponto de encontro, e ele não está no \
             log: {aviso}"
        );
    }
}

#[path = "testes/base64_escrito_a_mao.rs"]
#[cfg(test)]
mod base64_escrito_a_mao;

#[path = "testes/base64_ida_e_volta.rs"]
#[cfg(test)]
mod base64_ida_e_volta;

#[path = "testes/conferir_a_troca.rs"]
#[cfg(test)]
mod conferir_a_troca;

// O que o quarto disse sobre onde um servidor mora hoje, reexportado para quem
// lê a resposta de [`onde_mora_hoje`] sem nomear o `seele-core`, que o ADR 0002
// não deixa a casca alcançar. A regra que monta o bilhete desta volta
// (`bilhete_desta_volta`) não sai mais daqui: desde a tarefa 14b da F7 quem a
// aplica é a `Chegada`, junto com a corrida.
pub use seele_core::encontro::OndeMora;

/// Onde este servidor mora hoje: o socket dele, e a escuta de avisos dele.
///
/// # Por que a casca chama isto e não monta as marcas
///
/// A marca é vocabulário de fio (ADR 0002). A casca tem uma impressão digital
/// e um ponto de encontro, e o que ela quer são dois endereços. As marcas saem
/// de [`seele_core::encontro::Marcas::do_servidor`], a **mesma** função com que
/// o anfitrião se registra (`Convocacao::para_servidor`, no `seele-server`).
/// Quando cada lado derivava as suas, os dois divergiram, e a escuta nunca foi
/// achada.
///
/// # O que volta
///
/// Um [`OndeMora`], e não um par de `Option`: «o ponto não respondeu» e «o
/// ponto respondeu e ninguém mora lá» apontam para lugares diferentes, e a
/// consulta registra no log qual foi. [`OndeMora::escuta`] é a resposta crua,
/// e quem ocupou a marca da escuta também aparece nela.
///
/// # Não é o caminho da conexão
///
/// Desde a tarefa 14b da F7, quem volta a um servidor guardado pergunta ao
/// quarto pela `Chegada` (`Chegada::com_o_quarto`, que [`Connection::connect`]
/// liga quando há impressão e bilhete), junto com a corrida, e é lá que a regra
/// da escuta (`bilhete_desta_volta`) é aplicada. Isto fica para a pergunta
/// avulsa: os testes do link (`quarto_pelo_link.rs`) e a conferência de um
/// cenário (`ocupante_da_escuta.rs`).
///
/// # Pelo ENC/3, como a `Chegada`
///
/// Um anfitrião 0.17 só se registra pelo `MORA3`, e o `QUEM` do ENC/1 nunca o
/// acha: esta função perguntava pelo ENC/1 e respondia `NinguemMora` a um
/// anfitrião que estava no quarto. Agora pergunta pelo
/// [`seele_core::encontro::localizar3`], o mesmo da `Chegada`. As dicas de rede
/// local do ENC/3 não cabem no `OndeMora`, e ficam de fora: a conexão as usa
/// pela `Chegada`, e não por aqui. O `Enc2Calado` (o ponto que cala o `QUEM3`)
/// volta como `NinguemMora`, o caso que o `OndeMora` já descrevia como «ponto
/// anterior que não conhece a pergunta».
pub async fn onde_mora_hoje(ponto: &str, impressao: &str) -> OndeMora {
    let Some(marcas) = seele_core::encontro::Marcas::do_servidor(impressao) else {
        tracing::info!(
            "quarto: a impressão digital guardada não forma marca; nenhuma pergunta saiu"
        );
        return OndeMora::SemMarca;
    };
    onde_mora_pelo_enc3(
        seele_core::encontro::localizar3(ponto, &marcas, seele_core::encontro::PRAZO_DO_QUARTO)
            .await,
    )
}

/// A resposta do ENC/3 na forma que a casca lê.
fn onde_mora_pelo_enc3(localizacao: seele_core::encontro::Localizacao) -> OndeMora {
    use seele_core::encontro::Localizacao;
    match localizacao {
        Localizacao::Achada {
            servidor, escuta, ..
        } => OndeMora::Achado { servidor, escuta },
        Localizacao::NinguemMora | Localizacao::Enc2Calado => OndeMora::NinguemMora,
        Localizacao::PontoRecusou => OndeMora::PontoRecusou,
        Localizacao::PontoMudo => OndeMora::PontoMudo,
        Localizacao::PontoNaoResolve => OndeMora::PontoNaoResolve,
    }
}

#[path = "testes/a_consulta_ao_quarto.rs"]
#[cfg(test)]
mod a_consulta_ao_quarto;

/// A impressão contra a qual uma conexão confere o certificado do servidor.
///
/// # Por que não é só a do link
///
/// Era só a do link, e isso eram dois defeitos com uma causa (§2.1 e §3.1 da
/// análise de 22/09). Quem volta a um servidor pela lista de conhecidos não tem
/// link nesta sessão: ele foi lido uma vez, num processo que já fechou. A
/// pessoa entrava então sem impressão esperada nenhuma:
///
/// - o endereço novo que o quarto devolvia era fixado **às cegas**, com a faixa
///   PRIMEIRO CONTATO. A marca do quarto está em todo link, e no quarto fica
///   quem escreveu primeiro. Bastava o anfitrião ficar fora mais de 60 s para
///   um impostor ser fixado e gravado na lista (o S3);
/// - `Batida::preparar` tira a marca do `LEVE` da impressão e, sem ela,
///   devolvia `None`: quem estava atrás de NAT não era alcançado na volta, nem
///   na reconexão (o terceiro defeito do link).
///
/// A impressão guardada existia, e só servia para formar a marca da pergunta.
///
/// # A ordem
///
/// O link desta sessão vence a guardada. Ele é o que a pessoa acabou de colar
/// para este endereço, e a guardada é, quase sempre, a impressão que uma conexão
/// anterior **aceitou** ([`impressao_a_guardar`]). Não é assim numa lista gravada
/// pela 0.15.0: ela guardava a impressão do link daquela vez qualquer que
/// tivesse sido o veredito, inclusive quando o link discordava do pino e a
/// conexão ficou com a chave fixada. Um link colado de novo corrige essa entrada,
/// e a primeira volta pelo alvo já fixado também, quando ele é de escopo público
/// ([`impressao_a_guardar`]). Num alvo de LAN, essa volta é recusada dentro do
/// TLS, e o remédio é colar o link de novo.
/// Se o link e a guardada discordam, conferir pela velha recusaria o servidor
/// que a pessoa acabou de pedir. A mesma impressão forma a marca da pergunta ao
/// quarto (o `connect` do app a usa nas duas coisas), pela mesma razão:
/// perguntar onde mora uma chave e conferir outra faria o quarto apontar para o
/// servidor que a conferência recusaria logo em seguida.
#[must_use]
pub fn impressao_a_conferir(do_link: Option<&str>, guardada: Option<&str>) -> Option<String> {
    do_link.or(guardada).map(str::to_owned)
}

#[path = "testes/a_volta_pela_trilha_confere.rs"]
#[cfg(test)]
mod a_volta_pela_trilha_confere;

/// A impressão que a lista de servidores guarda depois de uma conexão.
///
/// Recebe as mesmas duas entradas de [`impressao_a_conferir`], o link desta
/// sessão e a guardada: num [`Trust::Known`], o que vai para a lista é a
/// impressão que se conferiu, e ela sai de [`impressao_a_conferir`] com as
/// mesmas duas, na mesma ordem.
///
/// # Por que não é a do link
///
/// Era a do link desta sessão, qualquer que fosse o veredito. Com um link que
/// discorda do pin ([`Trust::InviteDisagrees`]), a conexão fica de pé com a
/// chave fixada, que é a decisão do ADR 0003: o pin prova a continuidade, e
/// quem discorda dele é o link. A lista, porém, ficava com a chave do link,
/// que é a de **outro** servidor. A guardada é a impressão que a volta pela
/// lista confere ([`impressao_a_conferir`]), e com ela o servidor verdadeiro
/// passava a ser recusado em todo endereço sem pin: a porta nova do NAT, o
/// endereço que o quarto devolve.
///
/// # O que volta
///
/// A chave que a conexão aceitou, quando alguém a conferiu:
///
/// - [`Trust::FirstContactVerified`]: a conferida no aperto de mão, pelo link
///   desta sessão ou pela guardada;
/// - [`Trust::InviteDisagrees`]: a ofertada, que é a fixada num alvo de escopo
///   público, com ou sem link nesta sessão. Sem link, quem discorda do pin é a
///   guardada, e é assim que uma lista gravada pela 0.15.0 (com a impressão de
///   um link que discordava do pin) se cura na primeira volta por um alvo
///   público;
/// - [`Trust::Known`]: a impressão que se conferiu, venha do link desta sessão
///   ou da lista ([`impressao_a_conferir`]), quando há uma, porque é ela
///   concordar com o pin que faz o veredito ser `Known`.
///
/// `None` quando não há nada conferido a guardar, e `None` deixa a lista como
/// estava (`Conhecidos::anotar_caminhos` não apaga a impressão). É o caso de
/// [`Trust::Known`] sem impressão conferida; de [`Trust::FirstContact`], em que
/// a chave foi fixada às cegas e ninguém a prometeu; e de
/// [`Trust::InviteRefused`], que não atravessa.
///
/// # A invariante de que isso depende
///
/// **`InviteDisagrees` só vem do alvo, e só num alvo de escopo público — ver
/// `TofuVerifier`** (`seele_core::tofu::TofuVerifier::decide`). O pin é por
/// endereço de candidato e prova só aquele endereço. Num alvo de escopo
/// público, o endereço que a pessoa escolheu e que é o mesmo em qualquer rede,
/// ele prova a continuidade do servidor da entrada, e a ofertada é a chave
/// dele. Num candidato que ninguém escolheu (a resposta do quarto, um caminho
/// da lista) e num alvo de escopo local (um endereço de LAN, que é o mesmo de
/// uma casa para outra), o pin pode ser de outro servidor, e ali o verificador
/// recusa dentro do TLS a esperada que não confere: o caso chega como
/// [`ConnectionError::InviteMismatch`], e nunca aqui. Quem decide onde o pin
/// prova o servidor é `build_destino`, e só ele
/// (`seele_core::enlace::Destino::o_pino_prova_o_servidor`). Se
/// `InviteDisagrees` voltasse a nascer onde o pin não prova o servidor, gravar
/// a ofertada poria na entrada a chave de outro servidor, e a volta seguinte
/// conferiria por ela, calada.
#[must_use]
pub fn impressao_a_guardar(
    veredito: &Trust,
    do_link: Option<&str>,
    guardada: Option<&str>,
) -> Option<String> {
    match veredito {
        Trust::FirstContactVerified { fingerprint } => Some(fingerprint.clone()),
        // **A ofertada, e não a esperada, com ou sem link.** A conexão ficou de
        // pé com ela, e ela é a fixada num alvo de escopo público: ver «A
        // invariante de que isso depende».
        Trust::InviteDisagrees { offered, .. } => Some(offered.clone()),
        Trust::Known => impressao_a_conferir(do_link, guardada),
        // Fixada às cegas: guardá-la faria a volta por outro endereço conferir
        // contra uma chave que ninguém prometeu. Fica como na 0.15.0, em que
        // este caso nunca tinha link.
        Trust::FirstContact { .. } => None,
        // Não atravessa: o núcleo recusa este caso no TLS, e ele chega como
        // `ConnectionError::InviteMismatch`. O braço mantém o `match` exaustivo.
        Trust::InviteRefused { .. } => None,
    }
}

#[path = "testes/a_lista_guarda_a_impressao_aceita.rs"]
#[cfg(test)]
mod a_lista_guarda_a_impressao_aceita;

#[path = "testes/o_candidato_do_quarto.rs"]
#[cfg(test)]
mod o_candidato_do_quarto;

#[path = "testes/a_abertura_do_audio.rs"]
#[cfg(test)]
mod a_abertura_do_audio;

#[path = "testes/o_audio_segue_a_sala.rs"]
#[cfg(test)]
mod o_audio_segue_a_sala;

#[path = "testes/a_voz_quando_o_enlace_volta.rs"]
#[cfg(test)]
mod a_voz_quando_o_enlace_volta;

/// Quantos pares esta versão do cliente consegue atender ao mesmo tempo.
///
/// Reexportado do núcleo, e não copiado: a tela precisa do número para não
/// oferecer o que o cliente não honra, e duas cópias do mesmo teto divergem no
/// dia em que ele subir.
#[must_use]
pub const fn pares_que_esta_versao_atende() -> u8 {
    seele_core::enlace::PARES_QUE_ESTA_VERSAO_ATENDE
}

// Ponte nativa de imagens; bytes nunca passam pelo executor JavaScript do MOD.
impl Connection {
    async fn imagem_de_mod(
        &self,
        pedido: seele_core::PedidoDeImagem,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let (resposta, recebe) = tokio::sync::oneshot::channel();
        self.command(Command::ImagemDeMod {
            pedido,
            bytes,
            resposta,
        })
        .map_err(|_| "sessao-encerrada".to_owned())?;
        recebe.await.map_err(|_| "sessao-encerrada".to_owned())?
    }
}

impl Connection {
    /// Envia a imagem escolhida pela UI e aguarda a gravação no servidor.
    pub async fn enviar_imagem_mod(
        &self,
        id: String,
        token: String,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.imagem_de_mod(
            seele_core::PedidoDeImagem::Enviar {
                cabecalho: seele_core::VolumeHeader { mod_id: id, token },
                bytes: bytes.len() as u64,
            },
            bytes,
        )
        .await
        .map(|_| ())
    }
    /// Lê a imagem somente após o MOD autorizar o pedido nesta sessão.
    pub async fn ler_imagem_mod(
        &self,
        id: String,
        channel: u32,
        payload: String,
    ) -> Result<Vec<u8>, String> {
        self.imagem_de_mod(
            seele_core::PedidoDeImagem::Ler {
                mod_id: id,
                channel: ChannelId(channel),
                payload,
            },
            Vec::new(),
        )
        .await
    }
}

#[path = "testes/onde_um_anexo_grava.rs"]
#[cfg(test)]
mod onde_um_anexo_grava;

#[path = "testes/o_debug_da_configuracao.rs"]
#[cfg(test)]
mod o_debug_da_configuracao;

/// O alvo canônico de quem liga (tarefa 30 da F7): a ida e a volta são a mesma
/// regra, e só um código que fecha vira alvo.
#[path = "testes/o_alvo_do_codigo.rs"]
#[cfg(test)]
mod o_alvo_do_codigo;

#[path = "testes/o_codigo_esperado.rs"]
#[cfg(test)]
mod o_codigo_esperado;

#[path = "testes/o_toque.rs"]
#[cfg(test)]
mod o_toque;

/// O codificador de vídeo em uso chega ao retrato, na tela de quem transmite.
#[path = "testes/o_codificador_no_retrato.rs"]
#[cfg(test)]
mod o_codificador_no_retrato;

/// O pânico da thread da conexão vira fim de sessão, com motivo, e não silêncio.
#[path = "testes/o_panico_da_thread_da_conexao.rs"]
#[cfg(test)]
mod o_panico_da_thread_da_conexao;

/// O carimbo de sessão (E3 da 0.17): quem é a sessão, em que volta do enlace
/// ela está, e o que acontece com um comando ou um evento de outra.
#[path = "testes/o_carimbo_da_sessao.rs"]
#[cfg(test)]
mod o_carimbo_da_sessao;

/// **A tela que o servidor liga sozinho aparece no retrato.**
///
/// O defeito: o servidor liga todo mundo na primeira transmissão da sala, o
/// `TelaAbriu` só virava `ScreenOpened`, e o retrato lia a tela só da escolha
/// da pessoa. Quem não tinha clicado em VER recebia a imagem e lia «NINGUÉM
/// ESTÁ COMPARTILHANDO», com `assistida` falso. Ver
/// [`seele_core::retrato::TelaLigada`].
#[path = "testes/a_tela_ligada_pelo_servidor.rs"]
#[cfg(test)]
mod a_tela_ligada_pelo_servidor;

#[path = "testes/o_diagnostico_da_voz.rs"]
#[cfg(test)]
mod o_diagnostico_da_voz;

#[path = "testes/mesa_de_texto.rs"]
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod mesa_de_texto;

#[path = "testes/camera_tests.rs"]
#[cfg(test)]
mod camera_tests;
