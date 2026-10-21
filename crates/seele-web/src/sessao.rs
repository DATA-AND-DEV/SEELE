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

/// O que o navegador sabe da sala, e as duas escolhas desta ponta que o
/// retrato precisa.
#[derive(Debug)]
pub struct Sessao {
    room: Room,
    /// O apelido digitado ao conectar, para antes de o roster conhecer esta
    /// pessoa.
    apelido: String,
    escolhida: Option<ScreenId>,
    pedido: Option<LimitesDeTela>,
    contados: Contados,
}

impl Sessao {
    /// A sessão que o aperto de mão entregou, adotada como o desktop a adota.
    #[must_use]
    pub fn adotar(sessao: &SessionInfo, apelido: &str) -> Self {
        let mut room = Room::new();
        room.adopt(sessao, apelido);
        Self {
            room,
            apelido: apelido.to_owned(),
            escolhida: None,
            pedido: None,
            contados: Contados::default(),
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
                return vec![json!({ "ModReply": {
                    "request": request, "part": part, "total": total, "payload": payload,
                } })];
            }
            ServerMessage::ModAviso {
                mod_id,
                canal,
                carga,
            } => {
                return vec![json!({ "ModAviso": {
                    "mod_id": mod_id, "canal": canal.map(|canal| canal.0), "carga": carga,
                } })];
            }
            _ => {}
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
            self.contados.messages_revision += 1;
            eventos.push(json!("MessagesChanged"));
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
                .escolhida
                .is_some_and(|escolhida| !self.room.telas.contains_key(&escolhida))
            {
                self.escolhida = None;
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
        eventos
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
    pub fn escolher_tela(&mut self, tela: Option<ScreenId>) {
        self.escolhida = tela;
    }

    /// O teto que esta pessoa escolheu para a própria transmissão.
    pub fn pedir(&mut self, pedido: Option<LimitesDeTela>) {
        self.pedido = pedido;
    }

    /// O retrato da sala, com os nomes de campo do `Snapshot`.
    #[must_use]
    pub fn retrato(&self) -> RetratoDaSala {
        foto_da_sala(&self.room, self.escolhida, self.pedido).retrato(|| self.apelido.clone())
    }
}
