//! **Os comandos da sessão do navegador** (Tarefa 2.6 da F4): o que a casca
//! pede pela ponte e vira mensagem ao servidor, ou mexe no `Room`.
//!
//! Cada um faz o que o método da `seele_ffi::Connection` com o mesmo papel faz,
//! na mesma ordem: o que se apara, o que é recusado antes de sair (e com que
//! erro), o que sai no fio e o que muda no `Room` — e os eventos que a casca
//! ouve, com os contadores subindo antes. Os nomes e os argumentos são os do
//! `invoke` do desktop (o `#[tauri::command]` de `apps/seele-app/src/main.rs`,
//! com os argumentos em camelCase), e as respostas, o JSON que ele devolve.
//!
//! O que não é da sessão (a voz, as escolhas guardadas na origem, o seletor de
//! arquivo, a espera do `peso_da_linha`) fica na ponte, em JavaScript, e chama
//! daqui só o que vai ao fio.
//!
//! # O que não cabe no fio
//!
//! Um campo acima do teto (um nome de canal comprido, um motivo de banimento
//! longo) é recusado pelo `Validate` do `seele-proto` na hora de enquadrar. No
//! desktop, o motor diz isso no registro e segue (`recusar_aqui`): nada sai, a
//! sessão continua, e o comando já tinha respondido. Aqui é igual: a resposta é
//! a de sempre, e o motivo volta em [`Feito::nao_saiu`] para a página dizer no
//! console.

use seele_proto::control::{
    check_server_icon, AlertReason, AlertSeverity, ClientMessage, ControlError, MessageRefusal,
    MAX_BODY_LEN, MAX_NICKNAME_LEN,
};
use seele_proto::ids::{ChannelId, ClientMessageId, MessageId, PersonId, ScreenId, VoiceRoomId};
use seele_sessao::assento::carimbo_da_entrada;
use seele_sessao::retrato::aviso_de;
use seele_sessao::state::Notice;
use serde_json::{json, Value};

use crate::ligacao::Ligacao;
use crate::sessao::{conversa_de, fora_do_ar_de, EscolhaDeTela, Sessao};

/// O tamanho da página de histórico que o `open_channel` pede: o
/// `HISTORY_PAGE` da FFI.
pub const PAGINA_DO_HISTORICO: u16 = 50;

/// As chaves das mensagens que esta página escreve: o
/// `next_client_message_id` da FFI.
///
/// **Uma por página, e não por conexão.** O servidor deduplica por autor e
/// chave, e uma pendente que a volta repete leva a mesma chave: se a contagem
/// recomeçasse a cada conexão, a mensagem nova de depois da volta teria a
/// chave de uma de antes, e o servidor a jogaria fora como repetida.
#[derive(Debug, Clone)]
pub struct Chaves {
    proxima: u64,
}

impl Chaves {
    /// A contagem desta página: os 32 bits de cima sorteados por quem a cria,
    /// os de baixo contando de 1, como no desktop.
    #[must_use]
    pub const fn com_semente(semente: u32) -> Self {
        Self {
            proxima: ((semente as u64) << 32) | 1,
        }
    }

    fn tirar(&mut self) -> ClientMessageId {
        let chave = self.proxima;
        self.proxima = self.proxima.wrapping_add(1);
        ClientMessageId(chave)
    }
}

/// Um comando atendido: a resposta do `invoke`, os eventos, e o que não saiu.
#[derive(Debug, Clone, PartialEq)]
pub struct Feito {
    /// O que o `invoke` devolve.
    pub resposta: Value,
    /// Os eventos que a casca ouve, no JSON do `seele_ffi::Event`.
    pub eventos: Vec<Value>,
    /// O motivo de uma mensagem que o fio não levou (ver o doc do módulo).
    pub nao_saiu: Option<String>,
}

impl Feito {
    /// No JSON que a página lê: `{"resposta", "eventos", "nao_saiu"}`.
    #[must_use]
    pub fn em_json(&self) -> Value {
        json!({ "resposta": self.resposta, "eventos": self.eventos, "nao_saiu": self.nao_saiu })
    }
}

/// O que um comando pode dar: feito, ou o erro do `invoke` (o JSON do
/// `ConnectionError`, ou o texto de argumento inválido do Tauri).
pub type Atendido = Result<Feito, Value>;

/// Os comandos que esta camada atende. A ponte confere contra esta lista
/// (`bancada/ponte-web.cjs`): um comando da lista que a ponte não manda para cá
/// é um comando que ninguém atende.
pub const COMANDOS_DA_SESSAO: &[&str] = &[
    "apagar_linha",
    "apagar_voice_room",
    "aplicar_icone_do_server",
    "assistir",
    "banir_pessoa",
    "criar_linha",
    "criar_voice_room",
    "descartar_mensagem",
    "dispensar_aviso",
    "enter_voice_room",
    "escolher_apelido",
    "expulsar_pessoa",
    "icone_do_server",
    "imagem_da_pessoa",
    "leave_voice_room",
    "membros_fora_do_ar",
    "messages",
    "minha_imagem",
    "mod_request",
    "mover_pessoa",
    "open_channel",
    "pedir_quadro_chave",
    "peso_da_linha",
    "reenviar_mensagem",
    "remover_mensagem",
    "renomear_linha",
    "renomear_server",
    "renomear_voice_room",
    "send_message",
    "set_muted",
    "set_total_isolation",
    "tirar_icone_do_server",
];

/// O que um comando escreve e diz, montado antes de ir ao fio.
#[derive(Default)]
struct Saida {
    mensagens: Vec<ClientMessage>,
    eventos: Vec<Value>,
}

impl Ligacao {
    /// Atende um comando da casca. `None` quando o comando não é desta camada.
    ///
    /// `agora_s` é o relógio da página, em segundos desde 1970: a hora de uma
    /// pendente, como o `agora_em_segundos` do desktop.
    pub fn atender(
        &mut self,
        comando: &str,
        args: &Value,
        chaves: &mut Chaves,
        agora_s: i64,
    ) -> Option<Atendido> {
        if !COMANDOS_DA_SESSAO.contains(&comando) {
            return None;
        }
        let Some(sessao) = self.sessao_mut() else {
            return Some(Err(json!("NotConnected")));
        };
        let mut saida = Saida::default();
        let resposta = match atender_na_sessao(sessao, comando, args, chaves, agora_s, &mut saida) {
            Ok(resposta) => resposta,
            Err(erro) => return Some(Err(erro)),
        };
        let mut nao_saiu = None;
        for mensagem in &saida.mensagens {
            if let Err(erro) = self.escrever(mensagem) {
                nao_saiu = Some(recusado_aqui(mensagem, &erro));
            }
        }
        Some(Ok(Feito {
            resposta,
            eventos: saida.eventos,
            nao_saiu,
        }))
    }
}

/// A frase do registro para uma mensagem que o fio não levou.
fn recusado_aqui(mensagem: &ClientMessage, erro: &ControlError) -> String {
    let nome = serde_json::to_value(mensagem)
        .ok()
        .and_then(|valor| match valor {
            Value::String(nome) => Some(nome),
            Value::Object(campos) => campos.keys().next().cloned(),
            _ => None,
        })
        .unwrap_or_default();
    format!("o fio recusou o {nome}: {erro:?}; nada saiu, e a sessão segue")
}

#[allow(clippy::too_many_lines)]
fn atender_na_sessao(
    sessao: &mut Sessao,
    comando: &str,
    args: &Value,
    chaves: &mut Chaves,
    agora_s: i64,
    saida: &mut Saida,
) -> Result<Value, Value> {
    let a = Args { comando, args };
    match comando {
        // ---------------------------------------------------- a conversa
        "open_channel" => {
            let channel = ChannelId(a.u32("channel")?);
            saida.mensagens.push(ClientMessage::JoinChannel { channel });
            sessao.room.open_channel(channel);
            saida.mensagens.push(ClientMessage::FetchHistory {
                channel,
                cursor: None,
                limit: PAGINA_DO_HISTORICO,
            });
            sessao.a_conversa_mudou(&mut saida.eventos);
            Ok(Value::Null)
        }
        "send_message" => {
            let channel = ChannelId(a.u32("channel")?);
            let corpo = a.texto("body")?.trim().to_owned();
            if corpo.is_empty() {
                return Ok(json!(""));
            }
            let chave = chaves.tirar();
            sessao
                .room
                .anotar_envio(channel, chave, corpo.clone(), agora_s);
            sessao.a_conversa_mudou(&mut saida.eventos);
            if let Some(recusa) = grande_demais(&corpo) {
                if sessao.room.recusar_envio(chave, recusa) {
                    sessao.a_conversa_mudou(&mut saida.eventos);
                }
                return Ok(json!(chave.get().to_string()));
            }
            saida.mensagens.push(ClientMessage::SendMessage {
                channel,
                body: corpo,
                replies_to: None,
                client_message_id: chave,
            });
            Ok(json!(chave.get().to_string()))
        }
        "reenviar_mensagem" => {
            let Ok(chave) = a.texto("clientMessageId")?.parse::<u64>() else {
                return Ok(json!(false));
            };
            let chave = ClientMessageId(chave);
            let Some((channel, corpo)) = sessao
                .room
                .pendentes
                .iter()
                .find(|pendente| pendente.client_message_id == chave)
                .map(|pendente| (pendente.channel, pendente.body.clone()))
            else {
                return Ok(json!(false));
            };
            if let Some(recusa) = grande_demais(&corpo) {
                if sessao.room.recusar_envio(chave, recusa) {
                    sessao.a_conversa_mudou(&mut saida.eventos);
                }
                return Ok(json!(true));
            }
            sessao
                .room
                .anotar_envio(channel, chave, corpo.clone(), agora_s);
            sessao.a_conversa_mudou(&mut saida.eventos);
            saida.mensagens.push(ClientMessage::SendMessage {
                channel,
                body: corpo,
                replies_to: None,
                client_message_id: chave,
            });
            Ok(json!(true))
        }
        "descartar_mensagem" => {
            let Ok(chave) = a.texto("clientMessageId")?.parse::<u64>() else {
                return Ok(Value::Null);
            };
            let corpo = sessao.room.descartar_envio(ClientMessageId(chave));
            if corpo.is_some() {
                sessao.a_conversa_mudou(&mut saida.eventos);
            }
            Ok(json!(corpo))
        }
        "remover_mensagem" => {
            let message = a.u64("message")?;
            // Uma pendente não tem id de servidor: não há o que remover lá.
            if message == 0 {
                return Err(json!("UnknownMessage"));
            }
            saida.mensagens.push(ClientMessage::RemoveMessage {
                message: MessageId(message),
            });
            Ok(Value::Null)
        }
        "messages" => Ok(Value::Array(conversa_de(&sessao.room))),
        "membros_fora_do_ar" => Ok(Value::Array(fora_do_ar_de(&sessao.room))),
        "dispensar_aviso" => {
            if sessao.room.dispensar_aviso() {
                saida.eventos.push(json!("TelemetryChanged"));
            }
            Ok(Value::Null)
        }
        "escolher_apelido" => {
            let name = a.texto("apelido")?.trim().to_owned();
            if !name.is_empty() {
                saida.mensagens.push(ClientMessage::SetNickname { name });
            }
            Ok(Value::Null)
        }
        "icone_do_server" => Ok(json!(sessao.room.icon)),
        "imagem_da_pessoa" => {
            let pessoa = PersonId(a.u64("person")?);
            Ok(json!(sessao.room.person_icons.get(&pessoa).map(
                |png| format!("data:image/png;base64,{}", base64(png))
            )))
        }
        "mod_request" => {
            saida.mensagens.push(ClientMessage::ModRequest {
                request: a.u32("request")?,
                id: a.texto("id")?,
                channel: ChannelId(a.u32("channel")?),
                payload: a.texto("payload")?,
            });
            Ok(Value::Null)
        }

        // ---------------------------------------------- o que se modera
        "criar_voice_room" => {
            let name = a.texto("name")?;
            let limit = a.u16("limit")?;
            let channel = a.u32_opcional("channel")?.map(ChannelId);
            if !name.trim().is_empty() {
                saida.mensagens.push(ClientMessage::CreateVoiceRoom {
                    name,
                    limit,
                    channel,
                });
            }
            Ok(Value::Null)
        }
        "criar_linha" => {
            let name = a.texto("name")?;
            if !name.trim().is_empty() {
                saida.mensagens.push(ClientMessage::CreateChannel { name });
            }
            Ok(Value::Null)
        }
        "renomear_voice_room" => {
            let voice_room = VoiceRoomId(a.u32("voiceRoom")?);
            let name = a.texto("name")?;
            if !name.trim().is_empty() {
                saida
                    .mensagens
                    .push(ClientMessage::RenameVoiceRoom { voice_room, name });
            }
            Ok(Value::Null)
        }
        "renomear_linha" => {
            let channel = ChannelId(a.u32("channel")?);
            let name = a.texto("name")?;
            if !name.trim().is_empty() {
                saida
                    .mensagens
                    .push(ClientMessage::RenameChannel { channel, name });
            }
            Ok(Value::Null)
        }
        "renomear_server" => {
            let name = a.texto("name")?;
            if !name.trim().is_empty() {
                saida.mensagens.push(ClientMessage::RenameServer { name });
            }
            Ok(Value::Null)
        }
        "expulsar_pessoa" => {
            saida.mensagens.push(ClientMessage::KickPerson {
                person: PersonId(a.u64("person")?),
            });
            Ok(Value::Null)
        }
        "banir_pessoa" => {
            saida.mensagens.push(ClientMessage::BanPerson {
                person: PersonId(a.u64("person")?),
                reason: a.texto_opcional("reason")?,
                expires_at: a.i64_opcional("expiresAt")?,
            });
            Ok(Value::Null)
        }
        "mover_pessoa" => {
            saida.mensagens.push(ClientMessage::MovePerson {
                person: PersonId(a.u64("person")?),
                voice_room: VoiceRoomId(a.u32("voiceRoom")?),
            });
            Ok(Value::Null)
        }
        "apagar_voice_room" => {
            saida.mensagens.push(ClientMessage::DeleteVoiceRoom {
                voice_room: VoiceRoomId(a.u32("voiceRoom")?),
            });
            Ok(Value::Null)
        }
        "apagar_linha" => {
            saida.mensagens.push(ClientMessage::DeleteChannel {
                channel: ChannelId(a.u32("channel")?),
            });
            Ok(Value::Null)
        }
        // A resposta é o `ChannelWeighed`, e quem espera por ela é a ponte.
        "peso_da_linha" => {
            saida.mensagens.push(ClientMessage::WeighChannel {
                channel: ChannelId(a.u32("channel")?),
            });
            Ok(Value::Null)
        }
        "aplicar_icone_do_server" => {
            let icon = Some(a.bytes("icone")?);
            conferir_o_icone(icon.as_deref())?;
            saida.mensagens.push(ClientMessage::SetServerIcon { icon });
            Ok(Value::Null)
        }
        "tirar_icone_do_server" => {
            saida
                .mensagens
                .push(ClientMessage::SetServerIcon { icon: None });
            Ok(Value::Null)
        }
        // O `set_person_icon` da FFI, que a ponte chama depois de encolher a
        // imagem escolhida (`escolher_minha_imagem`), ao tirar
        // (`tirar_minha_imagem`) e ao entrar com o retrato guardado.
        "minha_imagem" => {
            let icon = a.bytes_opcional("icone")?;
            conferir_o_icone(icon.as_deref())?;
            saida.mensagens.push(ClientMessage::SetPersonIcon { icon });
            Ok(Value::Null)
        }

        // ------------------------------------------- a sala e as telas
        "enter_voice_room" => {
            let voice_room = VoiceRoomId(a.u32("voiceRoom")?);
            let senha = a.texto_opcional("senha")?;
            // A senha que o fio não leva é recusada aqui, com o aviso da recusa
            // de entrada: a mesma frase que o servidor daria.
            if senha
                .as_ref()
                .is_some_and(|senha| senha.len() > MAX_NICKNAME_LEN)
            {
                let recusa = Notice {
                    severity: AlertSeverity::Warning,
                    reason: AlertReason::VoiceRoomEntryRefused,
                    operator_text: None,
                };
                let notice = aviso_de(&recusa);
                sessao.room.notice = Some(recusa);
                saida
                    .eventos
                    .push(json!({ "NoticeRaised": { "notice": notice } }));
                return Ok(Value::Null);
            }
            saida.mensagens.push(ClientMessage::EnterVoiceRoom {
                voice_room,
                password: senha,
            });
            // O `Ping` marcado logo atrás: o `Pong` dele diz que a entrada não
            // foi recusada (`seele_sessao::assento`).
            sessao.entradas_pedidas += 1;
            saida.mensagens.push(ClientMessage::Ping {
                timestamp: carimbo_da_entrada(sessao.entradas_pedidas),
            });
            sessao.room.enter_voice_room(voice_room);
            saida.eventos.push(json!("RosterChanged"));
            Ok(Value::Null)
        }
        "leave_voice_room" => {
            saida.mensagens.push(ClientMessage::LeaveVoiceRoom);
            sessao.room.leave_voice_room();
            saida.eventos.push(json!("RosterChanged"));
            Ok(Value::Null)
        }
        "set_muted" => {
            saida
                .mensagens
                .push(ClientMessage::SetMuted(a.booleano("on")?));
            Ok(Value::Null)
        }
        "set_total_isolation" => {
            saida
                .mensagens
                .push(ClientMessage::SetTotalIsolation(a.booleano("on")?));
            Ok(Value::Null)
        }
        "assistir" => {
            let tela = ScreenId(a.u32("tela")?);
            let quero = a.booleano("quero")?;
            let escolha = if quero {
                EscolhaDeTela::Esta(tela)
            } else if sessao.escolha.tela() == Some(tela)
                || sessao.escolha == EscolhaDeTela::NinguemEscolheu
            {
                EscolhaDeTela::Nenhuma
            } else {
                sessao.escolha
            };
            if escolha != sessao.escolha {
                sessao.escolha = escolha;
                saida.eventos.push(json!("ScreenChanged"));
            }
            saida.mensagens.push(if quero {
                ClientMessage::WatchScreen { screen: tela }
            } else {
                ClientMessage::UnwatchScreen { screen: tela }
            });
            Ok(Value::Null)
        }
        "pedir_quadro_chave" => {
            saida.mensagens.push(ClientMessage::RequestKeyFrame {
                screen: ScreenId(a.u32("tela")?),
            });
            Ok(Value::Null)
        }
        _ => Err(json!({ "ComandoDesconhecido": { "comando": comando } })),
    }
}

/// O corpo acima do teto do fio: o `volta::grande_demais` da FFI.
fn grande_demais(corpo: &str) -> Option<MessageRefusal> {
    (corpo.len() > MAX_BODY_LEN).then(|| MessageRefusal::TooLong {
        limit: u32::try_from(MAX_BODY_LEN).unwrap_or(u32::MAX),
    })
}

/// O `check_server_icon` com os erros da FFI: o grande demais com o teto, e
/// todo o resto como «não é uma imagem».
fn conferir_o_icone(icone: Option<&[u8]>) -> Result<(), Value> {
    match check_server_icon(icone) {
        Ok(()) => Ok(()),
        Err(ControlError::FieldTooLong { limit, .. }) => {
            Err(json!({ "IconTooBig": { "limit_bytes": limit } }))
        }
        Err(_) => Err(json!("IconNotAPicture")),
    }
}

/// Os argumentos de um `invoke`, lidos como o Tauri os lê: o que falta ou vem
/// do tipo errado é recusado com a frase dele, e um `Option` ausente é `None`.
struct Args<'a> {
    comando: &'a str,
    args: &'a Value,
}

impl Args<'_> {
    fn campo(&self, nome: &str) -> Option<&Value> {
        self.args.get(nome).filter(|valor| !valor.is_null())
    }

    fn invalido(&self, nome: &str, esperado: &str) -> Value {
        json!(format!(
            "invalid args `{nome}` for command `{}`: esperava {esperado}",
            self.comando
        ))
    }

    fn u64(&self, nome: &str) -> Result<u64, Value> {
        self.campo(nome)
            .and_then(Value::as_u64)
            .ok_or_else(|| self.invalido(nome, "um inteiro sem sinal"))
    }

    fn u32(&self, nome: &str) -> Result<u32, Value> {
        u32::try_from(self.u64(nome)?).map_err(|_| self.invalido(nome, "um u32"))
    }

    fn u16(&self, nome: &str) -> Result<u16, Value> {
        u16::try_from(self.u64(nome)?).map_err(|_| self.invalido(nome, "um u16"))
    }

    fn u32_opcional(&self, nome: &str) -> Result<Option<u32>, Value> {
        self.campo(nome).map(|_| self.u32(nome)).transpose()
    }

    fn i64_opcional(&self, nome: &str) -> Result<Option<i64>, Value> {
        self.campo(nome)
            .map(|valor| valor.as_i64().ok_or_else(|| self.invalido(nome, "um i64")))
            .transpose()
    }

    fn booleano(&self, nome: &str) -> Result<bool, Value> {
        self.campo(nome)
            .and_then(Value::as_bool)
            .ok_or_else(|| self.invalido(nome, "um booleano"))
    }

    fn texto(&self, nome: &str) -> Result<String, Value> {
        self.campo(nome)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| self.invalido(nome, "um texto"))
    }

    fn texto_opcional(&self, nome: &str) -> Result<Option<String>, Value> {
        self.campo(nome).map(|_| self.texto(nome)).transpose()
    }

    fn bytes(&self, nome: &str) -> Result<Vec<u8>, Value> {
        let lista = self
            .campo(nome)
            .and_then(Value::as_array)
            .ok_or_else(|| self.invalido(nome, "uma lista de bytes"))?;
        lista
            .iter()
            .map(|byte| {
                byte.as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or_else(|| self.invalido(nome, "uma lista de bytes"))
            })
            .collect()
    }

    fn bytes_opcional(&self, nome: &str) -> Result<Option<Vec<u8>>, Value> {
        self.campo(nome).map(|_| self.bytes(nome)).transpose()
    }
}

/// Base64 padrão, com o preenchimento: o `base64_de` da FFI.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    const ALFABETO: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let letra = |indice: u32| {
        ALFABETO
            .get((indice & 63) as usize)
            .copied()
            .map_or('=', char::from)
    };
    let mut texto = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for bloco in bytes.chunks(3) {
        let b0 = bloco.first().copied().map_or(0, u32::from);
        let b1 = bloco.get(1).copied().map_or(0, u32::from);
        let b2 = bloco.get(2).copied().map_or(0, u32::from);
        let junto = (b0 << 16) | (b1 << 8) | b2;
        texto.push(letra(junto >> 18));
        texto.push(letra(junto >> 12));
        texto.push(if bloco.len() > 1 {
            letra(junto >> 6)
        } else {
            '='
        });
        texto.push(if bloco.len() > 2 { letra(junto) } else { '=' });
    }
    texto
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use crate::ligacao::{Passo, Pedido};
    use seele_proto::control::{decode, encode_em, ChannelInfo, Permission, ServerMessage};
    use seele_proto::ids::{SessionId, Ssrc};
    use seele_proto::quadro::{enquadrar, Desenquadrador};
    use seele_proto::version::PROTOCOL_VERSION;

    const SEMENTE: u32 = 0xABCD;

    fn do_servidor(mensagem: &ServerMessage) -> Vec<u8> {
        enquadrar(&encode_em(PROTOCOL_VERSION, mensagem).unwrap()).unwrap()
    }

    /// Uma ligação já dentro da sessão, com o fio limpo.
    fn dentro() -> Ligacao {
        let mut ligacao = Ligacao::abrir(Pedido {
            apelido: "marcela".into(),
            chave_publica: [5; 32],
            impressao: [7; 32],
            convite: None,
            aceito: None,
        })
        .unwrap();
        let _ = ligacao.chegou(&do_servidor(&ServerMessage::Challenge {
            nonce: vec![3; 32],
        }));
        let _ = ligacao.assinado(&[9; 64]);
        let passos = ligacao.chegou(&do_servidor(&ServerMessage::Session {
            id: SessionId(41),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: vec![ChannelInfo {
                id: ChannelId(1),
                name: "geral".into(),
            }],
            roles: Vec::new(),
            permissions: vec![Permission::Speak],
        }));
        assert_eq!(passos, vec![Passo::Entrou]);
        let _ = ligacao.para_o_fio();
        ligacao
    }

    fn escritos(ligacao: &mut Ligacao) -> Vec<ClientMessage> {
        let mut leitor = Desenquadrador::default();
        leitor.empurrar(&ligacao.para_o_fio());
        let mut quadros = Vec::new();
        while let Some(quadro) = leitor.proximo().unwrap() {
            quadros.push(decode::<ClientMessage>(&quadro).unwrap());
        }
        quadros
    }

    fn atender(ligacao: &mut Ligacao, chaves: &mut Chaves, comando: &str, args: Value) -> Feito {
        ligacao
            .atender(comando, &args, chaves, 1_790_000_000)
            .expect("um comando da sessão")
            .expect("atendido")
    }

    #[test]
    fn sem_sessao_e_not_connected_e_o_que_nao_e_da_sessao_nao_e_atendido() {
        let mut ligacao = Ligacao::abrir(Pedido {
            apelido: "a".into(),
            chave_publica: [5; 32],
            impressao: [7; 32],
            convite: None,
            aceito: None,
        })
        .unwrap();
        let mut chaves = Chaves::com_semente(SEMENTE);
        assert_eq!(
            ligacao.atender("messages", &json!({}), &mut chaves, 0),
            Some(Err(json!("NotConnected")))
        );
        assert_eq!(
            ligacao.atender("snapshot", &json!({}), &mut chaves, 0),
            None
        );
    }

    #[test]
    fn abrir_o_canal_pede_a_entrada_e_o_historico_e_avisa_a_conversa() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        let feito = atender(
            &mut ligacao,
            &mut chaves,
            "open_channel",
            json!({ "channel": 1 }),
        );
        assert_eq!(feito.resposta, Value::Null);
        assert_eq!(feito.eventos, vec![json!("MessagesChanged")]);
        assert_eq!(
            escritos(&mut ligacao),
            vec![
                ClientMessage::JoinChannel {
                    channel: ChannelId(1)
                },
                ClientMessage::FetchHistory {
                    channel: ChannelId(1),
                    cursor: None,
                    limit: 50
                },
            ]
        );
        let sessao = ligacao.sessao().unwrap();
        assert_eq!(sessao.room().current_channel, Some(ChannelId(1)));
        assert_eq!(sessao.contados().messages_revision, 1);
    }

    #[test]
    fn a_mensagem_sai_aparada_com_a_chave_da_pagina_e_fica_pendente() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "open_channel",
            json!({ "channel": 1 }),
        );
        let _ = escritos(&mut ligacao);

        let vazia = atender(
            &mut ligacao,
            &mut chaves,
            "send_message",
            json!({ "channel": 1, "body": "  \n " }),
        );
        assert_eq!(vazia.resposta, json!(""));
        assert!(vazia.eventos.is_empty() && escritos(&mut ligacao).is_empty());

        let feito = atender(
            &mut ligacao,
            &mut chaves,
            "send_message",
            json!({ "channel": 1, "body": "  oi  " }),
        );
        let chave = (u64::from(SEMENTE) << 32) | 1;
        assert_eq!(feito.resposta, json!(chave.to_string()));
        assert_eq!(feito.eventos, vec![json!("MessagesChanged")]);
        assert_eq!(
            escritos(&mut ligacao),
            vec![ClientMessage::SendMessage {
                channel: ChannelId(1),
                body: "oi".into(),
                replies_to: None,
                client_message_id: ClientMessageId(chave),
            }]
        );
        let conversa = atender(&mut ligacao, &mut chaves, "messages", json!({})).resposta;
        assert_eq!(conversa[0]["estado"], "Enviando");
        assert_eq!(conversa[0]["client_message_id"], chave.to_string());
        assert_eq!(conversa[0]["author_nickname"], "marcela");

        // A segunda leva a chave seguinte.
        let segunda = atender(
            &mut ligacao,
            &mut chaves,
            "send_message",
            json!({ "channel": 1, "body": "e aí" }),
        );
        assert_eq!(segunda.resposta, json!((chave + 1).to_string()));
    }

    #[test]
    fn a_mensagem_grande_demais_fica_recusada_e_nao_sai() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "open_channel",
            json!({ "channel": 1 }),
        );
        let _ = escritos(&mut ligacao);
        let corpo = "x".repeat(MAX_BODY_LEN + 1);
        let feito = atender(
            &mut ligacao,
            &mut chaves,
            "send_message",
            json!({ "channel": 1, "body": corpo }),
        );
        assert_eq!(
            feito.eventos,
            vec![json!("MessagesChanged"), json!("MessagesChanged")]
        );
        assert!(escritos(&mut ligacao).is_empty());
        let conversa = atender(&mut ligacao, &mut chaves, "messages", json!({})).resposta;
        assert_eq!(
            conversa[0]["estado"],
            json!({ "Recusada": { "TooLong": { "limit": 4096 } } })
        );

        // Reenviar a grande de novo a recusa, e não a manda.
        let chave = feito.resposta.as_str().unwrap().to_owned();
        let de_novo = atender(
            &mut ligacao,
            &mut chaves,
            "reenviar_mensagem",
            json!({ "clientMessageId": chave }),
        );
        assert_eq!(de_novo.resposta, json!(true));
        assert!(escritos(&mut ligacao).is_empty());

        // Descartar devolve o corpo, e só avisa quando havia o que descartar.
        let descartada = atender(
            &mut ligacao,
            &mut chaves,
            "descartar_mensagem",
            json!({ "clientMessageId": chave }),
        );
        assert_eq!(descartada.resposta, json!(corpo));
        assert_eq!(descartada.eventos, vec![json!("MessagesChanged")]);
        let outra_vez = atender(
            &mut ligacao,
            &mut chaves,
            "descartar_mensagem",
            json!({ "clientMessageId": chave }),
        );
        assert_eq!(outra_vez.resposta, Value::Null);
        assert!(outra_vez.eventos.is_empty());
        let torta = atender(
            &mut ligacao,
            &mut chaves,
            "reenviar_mensagem",
            json!({ "clientMessageId": "x" }),
        );
        assert_eq!(torta.resposta, json!(false));
    }

    #[test]
    fn reenviar_manda_de_novo_com_a_mesma_chave() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "open_channel",
            json!({ "channel": 1 }),
        );
        let chave = atender(
            &mut ligacao,
            &mut chaves,
            "send_message",
            json!({ "channel": 1, "body": "oi" }),
        )
        .resposta;
        let _ = escritos(&mut ligacao);
        let feito = atender(
            &mut ligacao,
            &mut chaves,
            "reenviar_mensagem",
            json!({ "clientMessageId": chave }),
        );
        assert_eq!(feito.resposta, json!(true));
        let [ClientMessage::SendMessage {
            client_message_id, ..
        }] = escritos(&mut ligacao)[..]
        else {
            panic!("não saiu um SendMessage");
        };
        assert_eq!(client_message_id.get().to_string(), chave.as_str().unwrap());
    }

    #[test]
    fn a_entrada_na_sala_leva_o_ping_marcado_e_a_senha_longa_e_recusada_aqui() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        let feito = atender(
            &mut ligacao,
            &mut chaves,
            "enter_voice_room",
            json!({ "voiceRoom": 2, "senha": null }),
        );
        assert_eq!(feito.eventos, vec![json!("RosterChanged")]);
        assert_eq!(
            escritos(&mut ligacao),
            vec![
                ClientMessage::EnterVoiceRoom {
                    voice_room: VoiceRoomId(2),
                    password: None
                },
                ClientMessage::Ping {
                    timestamp: carimbo_da_entrada(1)
                },
            ]
        );
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "enter_voice_room",
            json!({ "voiceRoom": 3 }),
        );
        assert_eq!(
            escritos(&mut ligacao)[1],
            ClientMessage::Ping {
                timestamp: carimbo_da_entrada(2)
            }
        );

        let longa = atender(
            &mut ligacao,
            &mut chaves,
            "enter_voice_room",
            json!({ "voiceRoom": 4, "senha": "x".repeat(MAX_NICKNAME_LEN + 1) }),
        );
        assert!(escritos(&mut ligacao).is_empty());
        assert_eq!(
            longa.eventos,
            vec![json!({ "NoticeRaised": { "notice": {
                "severity": "Warning", "reason": "VoiceRoomEntryRefused", "operator_text": null
            } } })]
        );
    }

    #[test]
    fn assistir_segue_os_tres_estados_da_ffi() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        // Ninguém escolheu: não querer a 3 vira «nenhuma», e avisa.
        let feito = atender(
            &mut ligacao,
            &mut chaves,
            "assistir",
            json!({ "tela": 3, "quero": false }),
        );
        assert_eq!(feito.eventos, vec![json!("ScreenChanged")]);
        assert_eq!(
            escritos(&mut ligacao),
            vec![ClientMessage::UnwatchScreen {
                screen: ScreenId(3)
            }]
        );
        // Querer a 3, e depois não querer a 4: a escolha fica na 3, sem aviso.
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "assistir",
            json!({ "tela": 3, "quero": true }),
        );
        let outra = atender(
            &mut ligacao,
            &mut chaves,
            "assistir",
            json!({ "tela": 4, "quero": false }),
        );
        assert!(outra.eventos.is_empty());
        assert_eq!(
            ligacao.sessao().unwrap().escolha,
            EscolhaDeTela::Esta(ScreenId(3))
        );
    }

    #[test]
    fn nomes_em_branco_nao_saem_e_os_compridos_saem_ditos() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        for (comando, args) in [
            ("criar_linha", json!({ "name": "  " })),
            ("renomear_linha", json!({ "channel": 1, "name": "" })),
            ("renomear_server", json!({ "name": " " })),
            (
                "criar_voice_room",
                json!({ "name": "\t", "limit": 4, "channel": null }),
            ),
            ("renomear_voice_room", json!({ "voiceRoom": 2, "name": "" })),
            ("escolher_apelido", json!({ "apelido": "   " })),
        ] {
            let feito = atender(&mut ligacao, &mut chaves, comando, args);
            assert_eq!(feito.resposta, Value::Null, "{comando}");
            assert!(feito.nao_saiu.is_none(), "{comando}");
            assert!(
                escritos(&mut ligacao).is_empty(),
                "{comando} saiu em branco"
            );
        }
        let comprido = atender(
            &mut ligacao,
            &mut chaves,
            "criar_linha",
            json!({ "name": "x".repeat(49) }),
        );
        assert_eq!(comprido.resposta, Value::Null);
        assert!(comprido
            .nao_saiu
            .is_some_and(|motivo| motivo.contains("CreateChannel")));
        assert!(escritos(&mut ligacao).is_empty());
        let bom = atender(
            &mut ligacao,
            &mut chaves,
            "criar_linha",
            json!({ "name": "x".repeat(48) }),
        );
        assert!(bom.nao_saiu.is_none());
        assert_eq!(escritos(&mut ligacao).len(), 1);
    }

    #[test]
    fn os_erros_sao_os_do_desktop() {
        let mut ligacao = dentro();
        let mut chaves = Chaves::com_semente(SEMENTE);
        let erro = |ligacao: &mut Ligacao, chaves: &mut Chaves, comando: &str, args: Value| {
            ligacao
                .atender(comando, &args, chaves, 0)
                .unwrap()
                .unwrap_err()
        };
        assert_eq!(
            erro(
                &mut ligacao,
                &mut chaves,
                "remover_mensagem",
                json!({ "message": 0 })
            ),
            json!("UnknownMessage")
        );
        assert_eq!(
            erro(
                &mut ligacao,
                &mut chaves,
                "aplicar_icone_do_server",
                json!({ "icone": [1, 2, 3] })
            ),
            json!("IconNotAPicture")
        );
        assert_eq!(
            erro(
                &mut ligacao,
                &mut chaves,
                "aplicar_icone_do_server",
                json!({ "icone": vec![0; 8193] })
            ),
            json!({ "IconTooBig": { "limit_bytes": 8192 } })
        );
        assert!(erro(&mut ligacao, &mut chaves, "open_channel", json!({}))
            .as_str()
            .is_some_and(|frase| frase.contains("invalid args `channel`")));
        assert!(escritos(&mut ligacao).is_empty());
        // Tirar o ícone sempre passa.
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "tirar_icone_do_server",
            json!({}),
        );
        assert_eq!(
            escritos(&mut ligacao),
            vec![ClientMessage::SetServerIcon { icon: None }]
        );
    }

    #[test]
    fn o_base64_e_o_padrao() {
        for (bytes, esperado) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (&[0xff, 0xfe, 0x00][..], "//4A"),
        ] {
            assert_eq!(base64(bytes), esperado);
        }
    }
}
