//! A sessão do navegador: o `Room` do `seele-sessao`, dobrado pelas mesmas
//! mensagens que o desktop dobra, e o retrato montado pela mesma projeção
//! (`seele_sessao::retrato`). Tarefa 2.2 da F4, fase 2.
//!
//! **A entrada é o `adopt`, e não o `apply(Session)`.** É o que o `connect` do
//! desktop faz com a sessão que o aperto de mão entregou, e foi o segundo
//! achado da medida da 2.2 (`spikes/snapshot-no-wasm/`): dobrada como uma
//! mensagem qualquer, a `Session` não leva o apelido digitado, e a própria
//! pessoa aparece como «pessoa N».
//!
//! O aperto de mão em si é da sessão pelo navegador (Tarefa 2.6). Esta parte
//! começa com a `SessionInfo` na mão, e a conformidade dos dois produtores
//! (`dois_produtores_de_snapshot.rs`) a confere contra o desktop, mensagem a
//! mensagem.

use std::collections::BTreeMap;

use seele_proto::control::{decode_da_lista, Lido, ServerMessage};
use seele_proto::ids::ScreenId;
use seele_sessao::retrato::{
    aviso_de, foto_da_sala, minha_tela_de, transmissoes_de, EndReason, LimitesDeTela, RetratoDaSala,
};
use seele_sessao::state::Room;
use seele_sessao::SessionInfo;
use serde_json::{json, Value};

use crate::{DoServidor, ErroDaPonte};

/// Os números do retrato que **quem dobra** conta (a fase 2 da 2.2 os chamou
/// assim): a casca os relê para saber se algo mudou desde a última vez. A FFI
/// conta os mesmos, nos mesmos instantes, e a conformidade dos dois produtores
/// de eventos (`dois_produtores_de_eventos.rs`) confere.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Contados {
    /// Sobe no `ServerIconChanged`.
    pub icon_revision: u64,
    /// Sobe no `PersonIconChanged`.
    pub person_icons_revision: u64,
    /// Sobe quando a conversa muda.
    pub messages_revision: u64,
    /// Sobe quando os membros mudam, antes do `RosterChanged`.
    pub fora_do_ar_revision: u64,
}

impl Contados {
    /// Os quatro, com os nomes de campo do `Snapshot`.
    #[must_use]
    pub fn em_json(&self) -> Value {
        json!({
            "icon_revision": self.icon_revision,
            "person_icons_revision": self.person_icons_revision,
            "messages_revision": self.messages_revision,
            "fora_do_ar_revision": self.fora_do_ar_revision,
        })
    }
}

/// A transmissão que esta pessoa escolheu, nos três estados do
/// `EscolhaDeTela` da FFI: o «ninguém escolheu» e o «escolheu nenhuma» dão o
/// mesmo retrato, e só se distinguem no que o `assistir(false)` faz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscolhaDeTela {
    /// Ninguém escolheu nada ainda, ou a escolhida acabou.
    NinguemEscolheu,
    /// A pessoa escolheu não assistir.
    Nenhuma,
    /// Esta.
    Esta(ScreenId),
}

impl EscolhaDeTela {
    /// A tela, quando há uma.
    #[must_use]
    pub const fn tela(self) -> Option<ScreenId> {
        match self {
            Self::Esta(tela) => Some(tela),
            Self::NinguemEscolheu | Self::Nenhuma => None,
        }
    }
}

/// O que o navegador sabe da sala, e as duas escolhas desta ponta que o
/// retrato precisa.
#[derive(Debug)]
pub struct Sessao {
    pub(crate) room: Room,
    /// O apelido digitado ao conectar, para antes de o roster conhecer esta
    /// pessoa.
    apelido: String,
    pub(crate) escolha: EscolhaDeTela,
    pedido: Option<LimitesDeTela>,
    pub(crate) contados: Contados,
    /// Quantas entradas em sala esta conexão pediu: o número do `Ping` marcado
    /// que vai atrás de cada uma (`seele_sessao::assento`), contado como o
    /// motor do desktop conta, de um em um a partir de 1.
    pub(crate) entradas_pedidas: u64,
    /// O volume que esta pessoa escolheu para cada outra, pelo apelido (o
    /// `set_volume` do desktop): sem distinguir maiúsculas, até 400%.
    pub(crate) volumes: BTreeMap<String, u16>,
    /// Os canais cujo peso foi pedido e ainda não veio, na ordem do pedido: o
    /// `pending_weights` da FFI. O `ChannelWeighed` responde todos de um canal;
    /// a recusa do pedido responde o primeiro.
    pub(crate) pesos_pendentes: std::collections::VecDeque<seele_proto::ids::ChannelId>,
}

/// O volume mais alto que se escolhe para alguém: o `VOLUME_MAXIMO` da FFI.
pub const VOLUME_MAXIMO: u16 = 400;

impl Sessao {
    /// A sessão que o aperto de mão entregou, adotada como o desktop a adota.
    #[must_use]
    pub fn adotar(sessao: &SessionInfo, apelido: &str) -> Self {
        let mut room = Room::new();
        room.adopt(sessao, apelido);
        Self {
            room,
            apelido: apelido.to_owned(),
            escolha: EscolhaDeTela::NinguemEscolheu,
            pedido: None,
            contados: Contados::default(),
            entradas_pedidas: 0,
            volumes: BTreeMap::new(),
            pesos_pendentes: std::collections::VecDeque::new(),
        }
    }

    /// **Uma mensagem do servidor, dobrada, e os eventos que a casca recebe**,
    /// no JSON do `seele_ffi::Event`: o mesmo `fold` da FFI, flag por flag, com
    /// os contadores subindo antes do aviso (a casca relê o número quando o
    /// evento chega).
    ///
    /// O `ModReply` e o `ModAviso` não são estado da sala: vão direto à casca,
    /// sem passar pelo `Room`, como na FFI.
    pub fn dobrar_com_eventos(&mut self, mensagem: &ServerMessage) -> Vec<Value> {
        match mensagem {
            ServerMessage::ModReply {
                request,
                part,
                total,
                payload,
            } => {
                let mut eventos = vec![json!({ "ModReply": {
                    "request": request, "part": part, "total": total, "payload": payload,
                } })];
                eventos.extend(self.transferencias());
                return eventos;
            }
            ServerMessage::ModAviso {
                mod_id,
                canal,
                carga,
            } => {
                let mut eventos = vec![json!({ "ModAviso": {
                    "mod_id": mod_id, "canal": canal.map(|canal| canal.0), "carga": carga,
                } })];
                eventos.extend(self.transferencias());
                return eventos;
            }
            _ => {}
        }
        // O volume escolhido segue quem trocou de nome, como no `dobrar_no_room`
        // da FFI: o nome de antes é lido antes de o `Room` o trocar.
        if let ServerMessage::PersonRenamed { person, nickname } = mensagem {
            if let Some(antes) = self
                .room
                .people
                .get(person)
                .map(|pessoa| pessoa.nickname.clone())
            {
                if antes != *nickname {
                    self.renomear_volume(&antes, nickname);
                }
            }
        }
        let mudou = self.room.apply(mensagem);
        let mut eventos = Vec::new();
        if mudou.membros {
            self.contados.fora_do_ar_revision += 1;
        }
        if mudou.roster {
            if matches!(mensagem, ServerMessage::PersonIconChanged { .. }) {
                self.contados.person_icons_revision += 1;
            }
            eventos.push(json!("RosterChanged"));
        }
        if mudou.messages {
            self.a_conversa_mudou(&mut eventos);
        }
        if mudou.channels {
            eventos.push(json!("ChannelsChanged"));
        }
        if mudou.server {
            if matches!(mensagem, ServerMessage::ServerIconChanged { .. }) {
                self.contados.icon_revision += 1;
            }
            eventos.push(json!("ServerChanged"));
        }
        if mudou.telemetry {
            eventos.push(json!("TelemetryChanged"));
        }
        if mudou.notice {
            if let Some(notice) = self.room.notice.as_ref() {
                eventos.push(json!({ "NoticeRaised": { "notice": aviso_de(notice) } }));
            }
        }
        if mudou.telas || mudou.roster {
            // As duas escolhas desta ponta seguem a sala, como as da FFI: o teto
            // pedido some com a transmissão própria, e a tela escolhida, com a
            // transmissão dela.
            if minha_tela_de(&self.room, None).is_none() {
                self.pedido = None;
            }
            if self
                .escolha
                .tela()
                .is_some_and(|escolhida| !self.room.telas.contains_key(&escolhida))
            {
                self.escolha = EscolhaDeTela::NinguemEscolheu;
            }
            let ha_transmissao = !transmissoes_de(&self.room, None).is_empty();
            if mudou.telas || (mudou.roster && ha_transmissao) {
                eventos.push(json!("ScreenChanged"));
            }
        }
        if mudou.ended {
            let motivo = self
                .room
                .ended
                .as_ref()
                .map_or(EndReason::LinkLost, |fim| fim.reason.into());
            eventos.push(json!({ "Ended": { "reason": motivo } }));
        }
        eventos.extend(self.transferencias());
        eventos
    }

    /// **As recusas de transferência que o servidor mandou**, drenadas do
    /// `Room` depois de cada mensagem, como o laço da conexão do desktop faz
    /// (`drain_transfers`): cada uma vira o `TransferChanged` que a casca ouve.
    fn transferencias(&mut self) -> Vec<Value> {
        use seele_sessao::state::TransferNotice;
        self.room
            .transfers
            .drain(..)
            .map(|aviso| {
                let transfer = match aviso {
                    TransferNotice::Refused {
                        client_message_id,
                        reason,
                    } => json!({
                        "kind": "RefusedBecause",
                        "client_message_id": client_message_id.get(),
                        "reason": reason,
                    }),
                    TransferNotice::Unavailable { attachment, reason } => json!({
                        "kind": "Unavailable",
                        "attachment": attachment.get(),
                        "reason": reason,
                    }),
                    TransferNotice::VolumeRecusado { token, motivo } => json!({
                        "kind": "VolumeRecusado",
                        "token": token,
                        "motivo": motivo,
                    }),
                };
                json!({ "TransferChanged": { "transfer": transfer } })
            })
            .collect()
    }

    /// Os números que quem dobra conta.
    #[must_use]
    pub const fn contados(&self) -> Contados {
        self.contados
    }

    /// Uma mensagem do servidor, dobrada no `Room`.
    pub fn dobrar(&mut self, mensagem: &ServerMessage) {
        let _ = self.room.apply(mensagem);
    }

    /// Um quadro do servidor, como ele chega pelo fio: lido pelo mesmo
    /// decodificador da página, dobrado no `Room`, e devolvido à casca em JSON.
    ///
    /// Uma variante desconhecida é pulada, e não dobrada, como no desktop.
    ///
    /// # Errors
    ///
    /// Os de [`crate::decodificar_do_servidor`]: o carimbo de outra versão, o
    /// quadro torto e o grande demais.
    pub fn dobrar_quadro(&mut self, quadro: &[u8]) -> Result<DoServidor, ErroDaPonte> {
        match decode_da_lista::<ServerMessage>(quadro).map_err(ErroDaPonte::Fio)? {
            Lido::Mensagem(mensagem) => {
                self.dobrar(&mensagem);
                serde_json::to_string(&mensagem)
                    .map(DoServidor::Mensagem)
                    .map_err(|e| ErroDaPonte::Json(e.to_string()))
            }
            Lido::Desconhecida { ordinal } => Ok(DoServidor::Desconhecida { ordinal }),
        }
    }

    /// A transmissão que esta pessoa escolheu assistir.
    pub fn escolher_tela(&mut self, escolha: EscolhaDeTela) {
        self.escolha = escolha;
    }

    /// O `Room`, para quem lê o que o retrato não leva (a conversa, o FORA DO
    /// AR, as imagens).
    #[must_use]
    pub const fn room(&self) -> &Room {
        &self.room
    }

    /// Guarda o volume de alguém pelo apelido, no lugar do que havia com o
    /// mesmo nome em outra caixa: o `guardar_volume` da FFI.
    pub(crate) fn guardar_volume(&mut self, apelido: &str, percent: u16) {
        self.volumes
            .retain(|guardado, _| !guardado.eq_ignore_ascii_case(apelido));
        self.volumes
            .insert(apelido.to_owned(), percent.min(VOLUME_MAXIMO));
    }

    fn renomear_volume(&mut self, antes: &str, agora: &str) {
        let Some(dela) = self
            .volumes
            .keys()
            .find(|guardado| guardado.eq_ignore_ascii_case(antes))
            .cloned()
        else {
            return;
        };
        if let Some(percent) = self.volumes.remove(&dela) {
            self.guardar_volume(agora, percent);
        }
    }

    /// Os volumes escolhidos, como o `volumes_locais` do desktop os devolve.
    #[must_use]
    pub const fn volumes(&self) -> &BTreeMap<String, u16> {
        &self.volumes
    }

    /// O ganho de cada voz que tem volume escolhido, pela SSRC de agora: o
    /// `ganhos_a_aplicar` da FFI. Quem voltou com outra SSRC volta com o
    /// mesmo ganho.
    #[must_use]
    pub fn ganhos(&self) -> Vec<(u32, f32)> {
        let mut ganhos: Vec<(u32, f32)> = self
            .volumes
            .iter()
            .filter_map(|(apelido, percent)| {
                self.room
                    .ssrc_of(apelido)
                    .map(|ssrc| (ssrc.0, f32::from((*percent).min(VOLUME_MAXIMO)) / 100.0))
            })
            .collect();
        ganhos.sort_by_key(|(ssrc, _)| *ssrc);
        ganhos
    }

    /// Sobe a revisão da conversa e diz à casca: o `messages_changed` da FFI.
    pub(crate) fn a_conversa_mudou(&mut self, eventos: &mut Vec<Value>) {
        self.contados.messages_revision += 1;
        eventos.push(json!("MessagesChanged"));
    }

    /// O teto que esta pessoa escolheu para a própria transmissão.
    pub fn pedir(&mut self, pedido: Option<LimitesDeTela>) {
        self.pedido = pedido;
    }

    /// O retrato da sala, com os nomes de campo do `Snapshot`.
    #[must_use]
    pub fn retrato(&self) -> RetratoDaSala {
        self.foto().1
    }

    /// O retrato e a perda que o servidor contou (`UplinkLoss`), que o
    /// `Snapshot` leva na telemetria e não no retrato da sala.
    #[must_use]
    pub fn foto(&self) -> (f32, RetratoDaSala) {
        let foto = foto_da_sala(&self.room, self.escolha.tela(), self.pedido);
        let perda = foto.loss_fraction;
        (perda, foto.retrato(|| self.apelido.clone()))
    }
}

/// **A conversa do canal aberto, como a casca a lê**: o `messages_of` da FFI,
/// no JSON do `seele_ffi::Message`. As confirmadas na ordem guardada, e as
/// pendentes desta ponta no fim, nas mesmas linhas, com `id` zero e o autor
/// próprio. A conformidade (`dois_produtores_da_conversa.rs`) confere contra a
/// FFI.
#[must_use]
pub fn conversa_de(room: &Room) -> Vec<Value> {
    use seele_proto::control::AttachmentState;
    let Some(aberto) = room.current_channel else {
        return Vec::new();
    };
    let mut lista: Vec<Value> = room
        .mensagens_do_canal(aberto)
        .iter()
        .map(|mensagem| {
            json!({
                "id": mensagem.id.0,
                "channel": mensagem.channel.0,
                "author": mensagem.author.0,
                "author_nickname": mensagem.author_nickname,
                "at_seconds": mensagem.at_seconds,
                "body": mensagem.body,
                "own": mensagem.own,
                "edited": mensagem.edited,
                "estado": "Confirmada",
                "client_message_id": mensagem.client_message_id.map(|chave| chave.get().to_string()),
                "attachment": mensagem.attachment.as_ref().map(|anexo| json!({
                    "id": anexo.id.get(),
                    "file_name": anexo.file_name,
                    "declared_type": anexo.declared_type,
                    "byte_size": anexo.byte_size,
                    "expired": anexo.state == AttachmentState::Expired,
                })),
            })
        })
        .collect();
    let eu = room.me.map_or(0, |pessoa| pessoa.0);
    let meu_nome = room
        .me
        .and_then(|pessoa| room.people.get(&pessoa))
        .map_or_else(String::new, |pessoa| pessoa.nickname.clone());
    lista.extend(room.pendentes_do_canal(aberto).map(|pendente| {
        json!({
            "id": 0,
            "channel": pendente.channel.0,
            "author": eu,
            "author_nickname": meu_nome,
            "at_seconds": pendente.at_seconds,
            "body": pendente.body,
            "own": true,
            "edited": false,
            "estado": estado_do_envio(&pendente.estado),
            "client_message_id": pendente.client_message_id.get().to_string(),
            "attachment": null,
        })
    }));
    lista
}

/// O `EstadoDoEnvio` da FFI: a recusa do servidor vira `Recusada`, com o
/// motivo no JSON do `MessageRefusal`, que tem os nomes do `MotivoDaRecusa`.
fn estado_do_envio(estado: &seele_sessao::state::EstadoDoEnvio) -> Value {
    use seele_sessao::state::EstadoDoEnvio;
    match estado {
        EstadoDoEnvio::Enviando => json!("Enviando"),
        EstadoDoEnvio::SemResposta => json!("SemResposta"),
        EstadoDoEnvio::Falhou(motivo) => json!({ "Recusada": motivo }),
    }
}

/// O FORA DO AR, como a casca o lê: `[{id, nickname}]`, na ordem do `Room`.
#[must_use]
pub fn fora_do_ar_de(room: &Room) -> Vec<Value> {
    room.fora_do_ar()
        .into_iter()
        .map(|(id, nickname)| json!({ "id": id.0, "nickname": nickname }))
        .collect()
}
