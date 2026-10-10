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
use seele_sessao::retrato::{foto_da_sala, LimitesDeTela, RetratoDaSala};
use seele_sessao::state::Room;
use seele_sessao::SessionInfo;

use crate::{DoServidor, ErroDaPonte};

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
        }
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
