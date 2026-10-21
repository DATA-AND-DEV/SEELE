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
use seele_sessao::search::Search;
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

/// **O que é da página, e não de uma conexão**: as chaves das mensagens e a
/// busca. No desktop, a busca mora no `Session` do app e atravessa as
/// conexões (`busca_andar` e `busca_limpar` respondem sem sessão nenhuma); aqui
/// ela mora na página, pelo mesmo motivo.
#[derive(Debug)]
pub struct DaPagina {
    /// As chaves das mensagens.
    pub chaves: Chaves,
    /// A busca corrente, com o cursor.
    pub busca: Option<Search>,
}

impl DaPagina {
    /// A página que acabou de carregar.
    #[must_use]
    pub const fn nova(semente: u32) -> Self {
        Self {
            chaves: Chaves::com_semente(semente),
            busca: None,
        }
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
    "abertura_da_tela",
    "ajustar_limites_da_tela",
    "assistir",
    "banir_pessoa",
    "busca_andar",
    "busca_limpar",
    "buscar",
    "codec_da_tela",
    "codificacao_da_tela",
    "compartilhar_tela",
    // Os três dos anexos que a ponte chama por dentro: o cabeçalho do envio,
    // o destino (com as regras do nome) e o pedido de um anexo ao servidor.
    "cabecalho_do_anexo",
    "criar_linha",
    "criar_voice_room",
    "descartar_mensagem",
    // As duas regras da prévia de um link que a ponte pergunta sem sessão:
    // aonde a prévia não vai, e que mídia uma página de GIF declara.
    "destino_de_previa",
    "destino_do_anexo",
    "dispensar_aviso",
    "enter_voice_room",
    "escolher_apelido",
    "expulsar_pessoa",
    "icone_do_server",
    "imagem_da_pessoa",
    "leave_voice_room",
    "membros_fora_do_ar",
    "messages",
    "midia_da_pagina",
    "minha_imagem",
    "mod_request",
    "mover_pessoa",
    "open_channel",
    "parar_de_compartilhar",
    "pedir_anexo",
    "pedir_quadro_chave",
    "peso_da_linha",
    // Os dois da volta, que a ponte chama por dentro: o `Ping` de rotina da
    // bateria e a repetição das pendentes.
    "ping_de_rotina",
    "reenviar_mensagem",
    "repetir_pendentes",
    "remover_mensagem",
    "renomear_linha",
    "renomear_server",
    "renomear_voice_room",
    "send_message",
    // Uma consulta da ponte, e não um comando da casca: a sala que o servidor
    // já não pode recusar e o número da voz desta conexão, para a voz abrir
    // como a `PortaDaVoz` do desktop abre (só na sala confirmada).
    "sala_da_voz",
    "set_muted",
    "set_total_isolation",
    "set_volume",
    "tirar_icone_do_server",
    "volumes_locais",
];

/// O que um comando escreve e diz, montado antes de ir ao fio.
#[derive(Default)]
struct Saida {
    mensagens: Vec<ClientMessage>,
    eventos: Vec<Value>,
}

/// Atende um comando da casca, com a ligação desta página se houver uma.
/// `None` quando o comando não é desta camada.
///
/// `agora_s` é o relógio da página, em segundos desde 1970: a hora de uma
/// pendente, como o `agora_em_segundos` do desktop.
pub fn atender(
    ligacao: Option<&mut Ligacao>,
    comando: &str,
    args: &Value,
    pagina: &mut DaPagina,
    agora_s: i64,
) -> Option<Atendido> {
    if !COMANDOS_DA_SESSAO.contains(&comando) {
        return None;
    }
    // Os dois da busca que o desktop responde sem sessão.
    match comando {
        "busca_andar" => {
            let a = Args { comando, args };
            return Some(a.booleano("adiante").map(|adiante| {
                feito(pagina.busca.as_mut().map_or_else(estado_vazio, |busca| {
                    if adiante {
                        busca.next_match();
                    } else {
                        busca.previous_match();
                    }
                    estado_da_busca(busca)
                }))
            }));
        }
        "busca_limpar" => {
            pagina.busca = None;
            return Some(Ok(feito(Value::Null)));
        }
        // O codec que o navegador codifica, para a sonda de antes da sessão.
        "codec_da_tela" => return Some(Ok(feito(json!(crate::tela::CODEC_DO_NAVEGADOR)))),
        "destino_de_previa" => {
            let a = Args { comando, args };
            return Some(
                a.texto("host")
                    .map(|host| feito(json!(seele_sessao::preview::host_de_previa(&host)))),
            );
        }
        "midia_da_pagina" => {
            let a = Args { comando, args };
            return Some(a.texto("html").map(|html| {
                feito(json!(seele_sessao::preview::midia_declarada_pela_pagina(
                    &html
                )))
            }));
        }
        _ => {}
    }
    let Some(ligacao) = ligacao else {
        return Some(Err(json!("NotConnected")));
    };
    ligacao.atender(comando, args, pagina, agora_s)
}

const fn feito(resposta: Value) -> Feito {
    Feito {
        resposta,
        eventos: Vec::new(),
        nao_saiu: None,
    }
}

/// O `BuscaEstado` do desktop (`main.rs`): todos os casamentos, o do cursor,
/// qual ele é dentro da mensagem, e a posição de um em diante.
fn estado_da_busca(busca: &Search) -> Value {
    let (posicao, total) = busca.position();
    json!({
        "casamentos": busca.matches(),
        "atual": busca.current(),
        "ordinal": busca.ordinal_in_message().and_then(|ordinal| u32::try_from(ordinal).ok()),
        "posicao": u32::try_from(posicao).unwrap_or(0),
        "total": u32::try_from(total).unwrap_or(0),
    })
}

/// O `BuscaEstado::default()`: sem busca guardada.
fn estado_vazio() -> Value {
    json!({ "casamentos": [], "atual": null, "ordinal": null, "posicao": 0, "total": 0 })
}

impl Ligacao {
    /// Atende um comando da sessão desta ligação. `None` quando o comando não
    /// é desta camada.
    pub fn atender(
        &mut self,
        comando: &str,
        args: &Value,
        pagina: &mut DaPagina,
        agora_s: i64,
    ) -> Option<Atendido> {
        if !COMANDOS_DA_SESSAO.contains(&comando) {
            return None;
        }
        let mut saida = Saida::default();
        if self.sessao_mut().is_none() {
            // **Na bateria**, como o motor do desktop sem cliente: o comando
            // atende pela sessão guardada (a conversa segue legível, a pendente
            // fica para a repetição, a sala e o mudo ficam lembrados para a
            // volta), e o que iria ao fio é largado.
            let Some(sessao) = self.sessao_na_bateria() else {
                return Some(Err(json!("NotConnected")));
            };
            return Some(
                atender_na_sessao(sessao, comando, args, pagina, agora_s, &mut saida).map(
                    |resposta| Feito {
                        resposta,
                        eventos: saida.eventos,
                        nao_saiu: None,
                    },
                ),
            );
        }
        let sessao = self.sessao_mut()?;
        let resposta = match atender_na_sessao(sessao, comando, args, pagina, agora_s, &mut saida) {
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
    pagina: &mut DaPagina,
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
            let chave = pagina.chaves.tirar();
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
        // Sobre os corpos que o `messages` devolve, crus e com as pendentes,
        // como o `buscar` do desktop; o cursor atravessa a reconstrução
        // (`Search::resume_at`).
        "buscar" => {
            let termo = a.texto("termo")?;
            let conversa = conversa_de(&sessao.room);
            let corpos = conversa.iter().map(|mensagem| {
                mensagem
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            });
            let mut busca = Search::new(corpos, &termo);
            if let Some(anterior) = pagina.busca.as_ref().and_then(Search::current) {
                busca.resume_at(anterior);
            }
            let estado = estado_da_busca(&busca);
            pagina.busca = Some(busca);
            Ok(estado)
        }
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
        // Pede um anexo ao servidor, que o manda num fluxo próprio; diz o que o
        // histórico sabe dele (o nome, o tipo alegado e o tamanho), ou `null`.
        "pedir_anexo" => {
            let anexo = a.u64("anexo")?;
            saida.mensagens.push(ClientMessage::FetchAttachment {
                attachment: seele_proto::ids::AttachmentId(anexo),
            });
            Ok(descricao_do_anexo(&sessao.room, anexo))
        }
        // O `destino_de` do desktop, com a pasta que a página diz: o anexo do
        // histórico, o nome conferido pelas regras de lá, e o nome ao lado.
        "destino_do_anexo" => {
            let anexo = a.u64("anexo")?;
            let pasta = a.texto("pasta")?;
            let Some((nome, _, _)) = crate::previa::anexo_no_historico(&sessao.room, anexo) else {
                return Err(
                    json!({ "reason": "AnexoDesconhecido", "claimed": "", "folder": pasta }),
                );
            };
            let mostrado = seele_sessao::nome_do_anexo::para_mostrar(&nome);
            match seele_sessao::nome_do_anexo::nome_seguro(&nome) {
                Ok(seguro) => Ok(json!({
                    "folder": pasta,
                    "file_name": seguro,
                    "beside": seele_sessao::nome_do_anexo::nome_ao_lado(seguro, 2),
                })),
                Err(_) => {
                    Err(json!({ "reason": "NomeRecusado", "claimed": mostrado, "folder": pasta }))
                }
            }
        }
        // O que vai na frente dos bytes de um anexo que sobe: o tipo do fluxo e
        // o `AttachmentHeader` enquadrado, com a chave da página (o número que o
        // `enviar_anexo` do desktop devolve). O hash é o dos bytes, em hex.
        "cabecalho_do_anexo" => {
            let hash = a.texto("hash")?;
            let Some(content_hash) = crate::ligacao::bytes_do_hex(&hash) else {
                return Err(json!(format!(
                    "invalid args `hash` for command `{comando}`: esperava 32 bytes em hex"
                )));
            };
            let chave = pagina.chaves.tirar();
            let cabecalho = seele_proto::attachment::AttachmentHeader {
                channel: ChannelId(a.u32("channel")?),
                client_message_id: chave,
                body: a.texto("body")?,
                replies_to: None,
                file_name: a.texto("nome")?,
                declared_type: a.texto("tipo")?,
                declared_len: a.u64("tamanho")?,
                content_hash,
            };
            let mut fluxo = vec![seele_proto::stream::StreamType::Attachment.byte()];
            let enquadrado =
                seele_proto::control::encode_em(seele_proto::version::PROTOCOL_VERSION, &cabecalho)
                    .and_then(|quadro| seele_proto::quadro::enquadrar(&quadro));
            match enquadrado {
                Ok(bytes) => {
                    fluxo.extend(bytes);
                    Ok(json!({ "id": chave.get(), "cabecalho": fluxo }))
                }
                // O que o fio não leva cai como a transferência do desktop cai:
                // a página o diz com o número, e nada sobe.
                Err(erro) => Ok(json!({ "id": chave.get(), "recusa": format!("{erro:?}") })),
            }
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
            let channel = ChannelId(a.u32("channel")?);
            // Guardado **antes** de sair, como o `pending_weights` do desktop.
            sessao.pesos_pendentes.push_back(channel);
            saida
                .mensagens
                .push(ClientMessage::WeighChannel { channel });
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
        "sala_da_voz" => Ok(json!({
            "sala": sessao.room.sala_confirmada().map(|sala| sala.0),
            "voz": sessao.room.ssrc.map(|ssrc| ssrc.0),
        })),
        // A ordem dos erros é a do desktop: quem não está na sala, e depois a
        // página sem áudio (`comAudio`, que a ponte diz).
        "set_volume" => {
            let apelido = a.texto("nickname")?;
            let percent = a.u16("percent")?;
            if sessao.room.ssrc_of(&apelido).is_none() {
                return Err(json!("UnknownPerson"));
            }
            if !a.booleano("comAudio")? {
                return Err(json!("NoAudioDevice"));
            }
            sessao.guardar_volume(&apelido, percent);
            Ok(Value::Null)
        }
        "volumes_locais" => Ok(json!(sessao.volumes())),
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
        // **A tela que esta ponta compartilha** (3.10): o pedido fica guardado
        // (o `minha_transmissao.pedido` do retrato), e o servidor responde com o
        // `ScreenShareStarted` que abre o fluxo. Quem captura é a página.
        "compartilhar_tela" => {
            let limites = a.limites("limites")?;
            sessao.pedir(Some(limites));
            saida.mensagens.push(ClientMessage::StartScreenShare);
            Ok(Value::Null)
        }
        // Idempotente: o pedido sai, e o servidor fica sabendo.
        "parar_de_compartilhar" => {
            sessao.pedir(None);
            saida.mensagens.push(ClientMessage::StopScreenShare);
            Ok(Value::Null)
        }
        // Só com a transmissão no ar, e nada no fio: o teto é desta ponta.
        "ajustar_limites_da_tela" => {
            let limites = a.limites("limites")?;
            if seele_sessao::retrato::minha_tela_de(&sessao.room, None).is_some() {
                sessao.pedir(Some(limites));
            }
            Ok(Value::Null)
        }
        "codificacao_da_tela" => Ok(crate::tela::codificacao(
            &a.limites("limites")?,
            a.u32("largura")?,
            a.u32("altura")?,
        )),
        "abertura_da_tela" => {
            let lado = |nome: &str| -> Result<u16, Value> { a.u16(nome) };
            crate::tela::abertura(
                a.u32("tela")?,
                a.booleano("monitor")?,
                lado("largura")?,
                lado("altura")?,
            )
            .map(|bytes| json!(bytes))
            .map_err(|motivo| json!(format!("a abertura da tela: {motivo}")))
        }
        // O `Ping` de rotina: o `Pong` dele é o que a bateria conta.
        "ping_de_rotina" => {
            sessao.pings += 1;
            saida.mensagens.push(ClientMessage::Ping {
                timestamp: sessao.pings,
            });
            Ok(Value::Null)
        }
        // **A repetição da volta** (a `Repeticao` e o `repetir` da FFI): a
        // rajada na primeira vez, e uma por vez depois; cada pendente que ainda
        // espera sai de novo com a mesma chave, e a grande demais fica recusada.
        "repetir_pendentes" => {
            let quantas = sessao.rajada.max(1).min(sessao.a_repetir.len());
            sessao.rajada = 0;
            let chaves: Vec<ClientMessageId> = sessao.a_repetir.drain(..quantas).collect();
            for chave in chaves {
                let Some((canal, corpo)) = sessao
                    .room
                    .pendentes
                    .iter()
                    .find(|pendente| {
                        pendente.client_message_id == chave
                            && matches!(
                                pendente.estado,
                                seele_sessao::state::EstadoDoEnvio::SemResposta
                                    | seele_sessao::state::EstadoDoEnvio::Enviando
                            )
                    })
                    .map(|pendente| (pendente.channel, pendente.body.clone()))
                else {
                    continue;
                };
                if let Some(recusa) = grande_demais(&corpo) {
                    if sessao.room.recusar_envio(chave, recusa) {
                        sessao.a_conversa_mudou(&mut saida.eventos);
                    }
                    continue;
                }
                sessao
                    .room
                    .anotar_envio(canal, chave, corpo.clone(), agora_s);
                sessao.a_conversa_mudou(&mut saida.eventos);
                saida.mensagens.push(ClientMessage::SendMessage {
                    channel: canal,
                    body: corpo,
                    replies_to: None,
                    client_message_id: chave,
                });
            }
            // O passo até a próxima é o da FFI: a página só o espera.
            Ok(json!({
                "faltam": sessao.a_repetir.len(),
                "passo_ms": seele_sessao::volta::INTERVALO_DA_REPETICAO.as_millis(),
            }))
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

/// O que o histórico sabe de um anexo: `{nome, tipo, tamanho}`, ou `null`.
fn descricao_do_anexo(room: &seele_sessao::state::Room, anexo: u64) -> Value {
    crate::previa::anexo_no_historico(room, anexo).map_or(
        Value::Null,
        |(nome, tipo, tamanho)| json!({ "nome": nome, "tipo": tipo, "tamanho": tamanho }),
    )
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

    fn limites(&self, nome: &str) -> Result<seele_sessao::retrato::LimitesDeTela, Value> {
        self.campo(nome)
            .cloned()
            .and_then(|valor| serde_json::from_value(valor).ok())
            .ok_or_else(|| self.invalido(nome, "os limites da tela"))
    }

    fn bytes_opcional(&self, nome: &str) -> Result<Option<Vec<u8>>, Value> {
        self.campo(nome).map(|_| self.bytes(nome)).transpose()
    }
}

/// Base64 padrão, com o preenchimento: o codificador do produto
/// (`seele_sessao::preview`), o mesmo alfabeto do `base64_de` da FFI.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    let mut texto = String::with_capacity(bytes.len().div_ceil(3) * 4);
    seele_sessao::preview::encode_base64_publico(bytes, &mut texto);
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

    fn atender(ligacao: &mut Ligacao, chaves: &mut DaPagina, comando: &str, args: Value) -> Feito {
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
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
        let mut chaves = DaPagina::nova(SEMENTE);
        let erro = |ligacao: &mut Ligacao, chaves: &mut DaPagina, comando: &str, args: Value| {
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
    fn a_busca_e_a_do_desktop_e_o_cursor_atravessa_a_reconstrucao() {
        let mut ligacao = dentro();
        let mut chaves = DaPagina::nova(SEMENTE);
        // Sem busca, andar e limpar respondem sem sessão, como no desktop.
        assert_eq!(
            atender_sem_ligacao(&mut chaves, "busca_andar", json!({ "adiante": true })),
            Ok(estado_vazio())
        );
        assert_eq!(
            atender_sem_ligacao(&mut chaves, "buscar", json!({ "termo": "x" })),
            Err(json!("NotConnected"))
        );
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "open_channel",
            json!({ "channel": 1 }),
        );
        for corpo in ["sync caiu", "o Sync voltou", "e o SYNC nem caiu"] {
            let _ = atender(
                &mut ligacao,
                &mut chaves,
                "send_message",
                json!({ "channel": 1, "body": corpo }),
            );
        }
        let achou = atender(
            &mut ligacao,
            &mut chaves,
            "buscar",
            json!({ "termo": "sync" }),
        )
        .resposta;
        assert_eq!(achou["total"], 3);
        assert_eq!(achou["posicao"], 1);
        assert_eq!(
            achou["atual"],
            json!({ "message": 0, "start": 0, "end": 4 })
        );
        let andou =
            atender_sem_ligacao(&mut chaves, "busca_andar", json!({ "adiante": true })).unwrap();
        assert_eq!(andou["posicao"], 2);
        // Uma mensagem nova e a busca de novo: o cursor fica na segunda.
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "send_message",
            json!({ "channel": 1, "body": "sync" }),
        );
        let refeita = atender(
            &mut ligacao,
            &mut chaves,
            "buscar",
            json!({ "termo": "sync" }),
        )
        .resposta;
        assert_eq!(
            (refeita["posicao"].clone(), refeita["total"].clone()),
            (json!(2), json!(4))
        );
        // E dá a volta pela ponta de trás.
        let _ = atender_sem_ligacao(&mut chaves, "busca_limpar", json!({}));
        let _ = atender(
            &mut ligacao,
            &mut chaves,
            "buscar",
            json!({ "termo": "sync" }),
        );
        let volta =
            atender_sem_ligacao(&mut chaves, "busca_andar", json!({ "adiante": false })).unwrap();
        assert_eq!(volta["posicao"], 4);
    }

    #[test]
    fn o_volume_segue_o_apelido_e_a_ordem_dos_erros_e_a_do_desktop() {
        let mut ligacao = dentro();
        let mut pagina = DaPagina::nova(SEMENTE);
        let _ = ligacao.chegou(&do_servidor(&ServerMessage::PersonPresent {
            profile: seele_proto::control::PersonProfile {
                id: PersonId(8),
                nickname: "Ana".into(),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(800),
        }));
        let erro = |ligacao: &mut Ligacao, pagina: &mut DaPagina, args: Value| {
            ligacao
                .atender("set_volume", &args, pagina, 0)
                .unwrap()
                .err()
        };
        assert_eq!(
            erro(
                &mut ligacao,
                &mut pagina,
                json!({ "nickname": "bia", "percent": 50, "comAudio": false })
            ),
            Some(json!("UnknownPerson"))
        );
        assert_eq!(
            erro(
                &mut ligacao,
                &mut pagina,
                json!({ "nickname": "ana", "percent": 50, "comAudio": false })
            ),
            Some(json!("NoAudioDevice"))
        );
        assert_eq!(
            erro(
                &mut ligacao,
                &mut pagina,
                json!({ "nickname": "ana", "percent": 900, "comAudio": true })
            ),
            None
        );
        assert_eq!(ligacao.sessao().unwrap().ganhos(), vec![(800, 4.0)]);
        // Outra caixa, o mesmo nome: substitui.
        let _ = erro(
            &mut ligacao,
            &mut pagina,
            json!({ "nickname": "ANA", "percent": 50, "comAudio": true }),
        );
        assert_eq!(
            atender(&mut ligacao, &mut pagina, "volumes_locais", json!({})).resposta,
            json!({ "ANA": 50 })
        );
        // Trocou de nome: o volume vai junto. Voltou com outra SSRC: o ganho também.
        let _ = ligacao.chegou(&do_servidor(&ServerMessage::PersonRenamed {
            person: PersonId(8),
            nickname: "aninha".into(),
        }));
        assert_eq!(
            atender(&mut ligacao, &mut pagina, "volumes_locais", json!({})).resposta,
            json!({ "aninha": 50 })
        );
        let _ = ligacao.chegou(&do_servidor(&ServerMessage::PersonPresent {
            profile: seele_proto::control::PersonProfile {
                id: PersonId(8),
                nickname: "aninha".into(),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(801),
        }));
        assert_eq!(ligacao.sessao().unwrap().ganhos(), vec![(801, 0.5)]);
    }

    fn atender_sem_ligacao(
        pagina: &mut DaPagina,
        comando: &str,
        args: Value,
    ) -> Result<Value, Value> {
        super::atender(None, comando, &args, pagina, 0)
            .expect("um comando da sessão")
            .map(|feito| feito.resposta)
    }

    #[test]
    fn o_anexo_tem_destino_pelas_regras_do_nome_e_o_cabecalho_volta_lido() {
        use seele_proto::attachment::AttachmentHeader;
        use seele_proto::control::{AttachmentInfo, AttachmentState};
        use seele_proto::ids::{AttachmentId, MessageId};
        let mut ligacao = dentro();
        let mut pagina = DaPagina::nova(SEMENTE);
        for (id, nome) in [
            (1_u64, "foto.png"),
            (2, "../.zshrc"),
            (3, "foto\u{202E}gnp.exe"),
        ] {
            let _ = ligacao.chegou(&do_servidor(&ServerMessage::MessageReceived {
                channel: ChannelId(1),
                id: MessageId(100 + id),
                author: PersonId(8),
                at_seconds: 1,
                author_nickname: "ana".into(),
                body: String::new(),
                replies_to: None,
                client_message_id: None,
                attachment: Some(AttachmentInfo {
                    id: AttachmentId(id),
                    file_name: nome.into(),
                    declared_type: "image/png".into(),
                    byte_size: 10,
                    state: AttachmentState::Available,
                }),
            }));
        }
        let destino = |ligacao: &mut Ligacao, pagina: &mut DaPagina, anexo: u64| {
            ligacao
                .atender(
                    "destino_do_anexo",
                    &json!({ "anexo": anexo, "pasta": "Downloads" }),
                    pagina,
                    0,
                )
                .unwrap()
                .map(|feito| feito.resposta)
        };
        assert_eq!(
            destino(&mut ligacao, &mut pagina, 1),
            Ok(json!({ "folder": "Downloads", "file_name": "foto.png", "beside": "foto (2).png" }))
        );
        assert_eq!(
            destino(&mut ligacao, &mut pagina, 2).unwrap_err()["reason"],
            "NomeRecusado"
        );
        let invisivel = destino(&mut ligacao, &mut pagina, 3).unwrap_err();
        assert_eq!(invisivel["reason"], "NomeRecusado");
        assert!(
            !invisivel["claimed"].as_str().unwrap().contains('\u{202E}'),
            "{invisivel}"
        );
        assert_eq!(
            destino(&mut ligacao, &mut pagina, 9),
            Err(json!({ "reason": "AnexoDesconhecido", "claimed": "", "folder": "Downloads" }))
        );

        let feito = atender(
            &mut ligacao,
            &mut pagina,
            "cabecalho_do_anexo",
            json!({ "channel": 1, "body": " oi ", "nome": "a.txt", "tipo": "text/plain", "tamanho": 3, "hash": "ab".repeat(32) }),
        );
        let bytes: Vec<u8> = feito.resposta["cabecalho"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(bytes[0], seele_proto::stream::StreamType::Attachment.byte());
        let mut leitor = Desenquadrador::default();
        leitor.empurrar(&bytes[1..]);
        let lido: AttachmentHeader = decode(&leitor.proximo().unwrap().unwrap()).unwrap();
        assert_eq!(
            lido.client_message_id.get(),
            feito.resposta["id"].as_u64().unwrap()
        );
        assert_eq!(
            (lido.body.as_str(), lido.declared_len, lido.content_hash),
            (" oi ", 3, [0xab; 32])
        );
        // Um anexo vazio não sai: a recusa vem com o número, como a queda do desktop.
        let vazio = atender(
            &mut ligacao,
            &mut pagina,
            "cabecalho_do_anexo",
            json!({ "channel": 1, "body": "", "nome": "a.txt", "tipo": "text/plain", "tamanho": 0, "hash": "ab".repeat(32) }),
        );
        assert!(vazio.resposta["recusa"].is_string() && vazio.resposta["id"].is_u64());
    }

    #[test]
    fn as_regras_da_previa_de_link_respondem_sem_sessao() {
        let mut pagina = DaPagina::nova(SEMENTE);
        for (host, vale) in [
            ("tenor.com", true),
            ("127.0.0.1", false),
            ("[::ffff:192.168.0.1]", false),
            ("roteador.local", false),
            ("10.0.0.8", false),
        ] {
            assert_eq!(
                atender_sem_ligacao(&mut pagina, "destino_de_previa", json!({ "host": host })),
                Ok(json!(vale)),
                "{host}"
            );
        }
        assert_eq!(
            atender_sem_ligacao(
                &mut pagina,
                "midia_da_pagina",
                json!({ "html": r#"<META property="og:image" content="https://media.tenor.com/a.gif?x=1&amp;y=2">"# })
            ),
            Ok(json!("https://media.tenor.com/a.gif?x=1&y=2"))
        );
    }

    /// A volta pelo aperto inteiro: o `Hello` sem convite, o desafio assinado e
    /// a `Session` nova.
    fn voltar(
        ligacao: &mut Ligacao,
        escolhas: crate::sessao::Escolhas,
    ) -> Vec<crate::ligacao::Passo> {
        let pedido = crate::ligacao::Pedido {
            apelido: "marcela".into(),
            chave_publica: [5; 32],
            impressao: [7; 32],
            convite: None,
            aceito: None,
        };
        ligacao.retomar(pedido, escolhas).unwrap();
        assert!(matches!(
            &escritos(ligacao)[..],
            [ClientMessage::Hello {
                join_secret: None,
                ..
            }]
        ));
        let _ = ligacao.chegou(&do_servidor(&ServerMessage::Challenge {
            nonce: vec![4; 32],
        }));
        let _ = ligacao.assinado(&[9; 64]);
        let _ = escritos(ligacao);
        ligacao.chegou(&do_servidor(&ServerMessage::Session {
            id: SessionId(42),
            person: PersonId(7),
            ssrc: Ssrc(701),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: vec![ChannelInfo {
                id: ChannelId(1),
                name: "geral".into(),
            }],
            roles: Vec::new(),
            permissions: vec![Permission::Speak],
        }))
    }

    #[test]
    fn na_bateria_os_comandos_atendem_pela_sessao_guardada_e_nada_vai_ao_fio() {
        use crate::ligacao::Passo;
        use crate::sessao::Escolhas;
        let mut ligacao = dentro();
        let mut pagina = DaPagina::nova(SEMENTE);
        let _ = atender(
            &mut ligacao,
            &mut pagina,
            "open_channel",
            json!({ "channel": 1 }),
        );
        let _ = escritos(&mut ligacao);
        let _ = ligacao.largar_para_voltar();

        // A conversa segue legível, e a mensagem nova fica pendente, como no
        // motor do desktop sem cliente: nada sai, e nada é recusado.
        let chave = atender(
            &mut ligacao,
            &mut pagina,
            "send_message",
            json!({ "channel": 1, "body": "na bateria" }),
        );
        assert!(chave.nao_saiu.is_none());
        let conversa = atender(&mut ligacao, &mut pagina, "messages", json!({})).resposta;
        assert_eq!(conversa[0]["body"], "na bateria");
        assert_eq!(escritos(&mut ligacao), Vec::<ClientMessage>::new());

        // A volta a repete, com a chave que o comando devolveu.
        assert_eq!(
            voltar(&mut ligacao, Escolhas::default()),
            vec![Passo::Voltou]
        );
        let _ = escritos(&mut ligacao);
        let _ = atender(&mut ligacao, &mut pagina, "repetir_pendentes", json!({}));
        let [ClientMessage::SendMessage {
            client_message_id, ..
        }] = &escritos(&mut ligacao)[..]
        else {
            panic!("a mensagem da bateria não saiu na volta");
        };
        assert_eq!(
            client_message_id.get().to_string(),
            chave.resposta.as_str().unwrap()
        );
    }

    #[test]
    fn a_volta_readota_restabelece_e_repete_as_pendentes() {
        use crate::ligacao::Passo;
        use crate::sessao::{Escolhas, PAGINA_DO_BURACO};
        let mut ligacao = dentro();
        let mut pagina = DaPagina::nova(SEMENTE);
        let _ = atender(
            &mut ligacao,
            &mut pagina,
            "open_channel",
            json!({ "channel": 1 }),
        );
        let _ = atender(
            &mut ligacao,
            &mut pagina,
            "enter_voice_room",
            json!({ "voiceRoom": 2 }),
        );
        let chave = atender(
            &mut ligacao,
            &mut pagina,
            "send_message",
            json!({ "channel": 1, "body": "durante" }),
        )
        .resposta;
        let _ = escritos(&mut ligacao);

        // Caiu: o que saía fica sem resposta, e a sessão fica guardada.
        let eventos = ligacao.largar_para_voltar();
        assert_eq!(eventos, vec![json!("MessagesChanged")]);
        let conversa = conversa_de(&ligacao.sessao().unwrap().room);
        assert_eq!(conversa[0]["estado"], "SemResposta");

        assert_eq!(
            voltar(
                &mut ligacao,
                Escolhas {
                    mudo: true,
                    isolado: false
                }
            ),
            vec![Passo::Voltou]
        );
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
                ClientMessage::JoinChannel {
                    channel: ChannelId(1)
                },
                ClientMessage::FetchHistory {
                    channel: ChannelId(1),
                    cursor: None,
                    limit: PAGINA_DO_BURACO
                },
                ClientMessage::SetMuted(true),
            ]
        );
        // A conversa ficou, e a pendente sai de novo com a mesma chave.
        let repetida = atender(&mut ligacao, &mut pagina, "repetir_pendentes", json!({}));
        assert_eq!(repetida.resposta, json!({ "faltam": 0, "passo_ms": 125 }));
        let [ClientMessage::SendMessage {
            client_message_id,
            body,
            ..
        }] = &escritos(&mut ligacao)[..]
        else {
            panic!("a pendente não saiu de novo");
        };
        assert_eq!(
            (client_message_id.get().to_string(), body.as_str()),
            (chave.as_str().unwrap().to_owned(), "durante")
        );
        // E o ping de rotina recomeça de 1 na conexão nova.
        let _ = atender(&mut ligacao, &mut pagina, "ping_de_rotina", json!({}));
        assert_eq!(
            escritos(&mut ligacao),
            vec![ClientMessage::Ping { timestamp: 1 }]
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
