//! **As imagens de um MOD pelo volume** (Tarefa 2.6 da F4): o fluxo
//! bidirecional próprio de cada pedido, como o `imagem_de_mod` do
//! `seele-core` — o pedido enquadrado (`PedidoDeImagem`), os bytes da imagem
//! quando ela sobe, e a resposta (`RespostaDeImagem`) com os bytes dela quando
//! ela desce. A página abre o fluxo e move os bytes; o que vai nele e o que a
//! resposta diz são daqui.
//!
//! O fluxo não tem negociação: o carimbo é o desta build
//! (`PROTOCOL_VERSION`), como no `frame::write` do desktop.

use seele_proto::control::{decode, encode_em};
use seele_proto::ids::ChannelId;
use seele_proto::midia_de_mod::TETO_DE_ARQUIVO;
use seele_proto::quadro::{comprimento, enquadrar, PREFIXO_LEN};
use seele_proto::version::PROTOCOL_VERSION;
use seele_proto::volume::{PedidoDeImagem, RespostaDeImagem, VolumeHeader};

/// O começo do fluxo: o pedido, enquadrado e carimbado.
///
/// # Errors
///
/// O texto da recusa do `seele-proto` (a validação do pedido), como o desktop
/// o diz ao MOD.
fn enquadrado(pedido: &PedidoDeImagem) -> Result<Vec<u8>, String> {
    let quadro = encode_em(PROTOCOL_VERSION, pedido).map_err(|erro| erro.to_string())?;
    enquadrar(&quadro).map_err(|erro| erro.to_string())
}

/// O pedido de **ler** uma imagem que o MOD autoriza (`ler_imagem_mod`).
///
/// # Errors
///
/// A recusa do `seele-proto`, em texto.
pub fn pedido_de_ler(mod_id: &str, canal: u32, payload: &str) -> Result<Vec<u8>, String> {
    enquadrado(&PedidoDeImagem::Ler {
        mod_id: mod_id.to_owned(),
        channel: ChannelId(canal),
        payload: payload.to_owned(),
    })
}

/// O pedido de **mandar** `bytes` bytes ao volume do MOD (`enviar_imagem_mod`),
/// com a autorização de uso único que ele deu. Os bytes seguem o pedido no
/// mesmo fluxo.
///
/// # Errors
///
/// A recusa do `seele-proto`, em texto.
pub fn pedido_de_enviar(mod_id: &str, token: &str, bytes: u64) -> Result<Vec<u8>, String> {
    enquadrado(&PedidoDeImagem::Enviar {
        cabecalho: VolumeHeader {
            mod_id: mod_id.to_owned(),
            token: token.to_owned(),
        },
        bytes,
    })
}

/// **A resposta que chega**, aos pedaços: o quadro da `RespostaDeImagem`, e os
/// bytes que ela anuncia logo atrás.
#[derive(Debug, Default)]
pub struct Resposta {
    pendente: Vec<u8>,
    lida: Option<RespostaDeImagem>,
    falha: Option<String>,
}

impl Resposta {
    /// Uma resposta nova, antes do primeiro byte.
    #[must_use]
    pub fn nova() -> Self {
        Self::default()
    }

    /// Mais um pedaço do fluxo.
    pub fn chegou(&mut self, pedaco: &[u8]) {
        if self.falha.is_some() {
            return;
        }
        self.pendente.extend_from_slice(pedaco);
        if self.lida.is_none() {
            self.ler_o_cabecalho();
        }
        // O teto do arquivo, como o `read_to_end` do desktop: o que passa dele
        // não é guardado.
        if self.lida.is_some() && self.pendente.len() > TETO_DE_ARQUIVO {
            self.falha = Some("arquivo-grande-demais".to_owned());
            self.pendente.clear();
        }
    }

    fn ler_o_cabecalho(&mut self) {
        let Some(prefixo) = self.pendente.get(..PREFIXO_LEN) else {
            return;
        };
        let mut lido = [0_u8; PREFIXO_LEN];
        lido.copy_from_slice(prefixo);
        let fim = match comprimento(lido) {
            Ok(tamanho) => PREFIXO_LEN + tamanho,
            Err(erro) => {
                self.falha = Some(erro.to_string());
                return;
            }
        };
        let Some(quadro) = self.pendente.get(PREFIXO_LEN..fim) else {
            return;
        };
        match decode::<RespostaDeImagem>(quadro) {
            Ok(resposta) => self.lida = Some(resposta),
            Err(erro) => self.falha = Some(erro.to_string()),
        }
        self.pendente.drain(..fim);
    }

    /// **O fluxo acabou**: os bytes da imagem (vazios na confirmação de um
    /// envio), ou o motivo, como o desktop o diz.
    ///
    /// # Errors
    ///
    /// A recusa do servidor (o `erro` da resposta), a do `seele-proto`, e
    /// `incompleto` quando os bytes não são os que a resposta anunciou.
    pub fn acabou(self) -> Result<Vec<u8>, String> {
        if let Some(falha) = self.falha {
            return Err(falha);
        }
        let resposta = self.lida.ok_or_else(|| "incompleto".to_owned())?;
        if let Some(erro) = resposta.erro {
            return Err(erro);
        }
        // O nome da lista `recusas.volume`, e não uma frase: ele chega ao MOD
        // em `e.erro` (a auditoria do `v6.json`, B2).
        if u64::try_from(self.pendente.len()).ok() != Some(resposta.bytes) {
            return Err("incompleto".to_owned());
        }
        Ok(self.pendente)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    fn resposta(erro: Option<&str>, bytes: u64) -> Vec<u8> {
        let quadro = encode_em(
            PROTOCOL_VERSION,
            &RespostaDeImagem {
                erro: erro.map(str::to_owned),
                bytes,
            },
        )
        .unwrap();
        enquadrar(&quadro).unwrap()
    }

    #[test]
    fn a_resposta_aos_pedacos_da_os_bytes_anunciados_e_o_resto_e_recusado() {
        let mut fluxo = resposta(None, 5);
        fluxo.extend_from_slice(b"\x89PNG!");
        let mut lida = Resposta::nova();
        for pedaco in fluxo.chunks(3) {
            lida.chegou(pedaco);
        }
        assert_eq!(lida.acabou().unwrap(), b"\x89PNG!");

        let mut curta = Resposta::nova();
        let mut fluxo = resposta(None, 5);
        fluxo.extend_from_slice(b"ab");
        curta.chegou(&fluxo);
        assert_eq!(curta.acabou(), Err("incompleto".to_owned()));

        let mut recusada = Resposta::nova();
        recusada.chegou(&resposta(Some("leitura recusada pelo MOD"), 0));
        assert_eq!(
            recusada.acabou(),
            Err("leitura recusada pelo MOD".to_owned())
        );

        assert_eq!(Resposta::nova().acabou(), Err("incompleto".to_owned()));
    }

    #[test]
    fn o_pedido_sai_enquadrado_e_o_que_o_protocolo_recusa_vem_em_texto() {
        let pedido = pedido_de_ler("ana/mesa", 0, "{}").unwrap();
        let tamanho = comprimento(pedido[..4].try_into().unwrap()).unwrap();
        assert_eq!(pedido.len(), 4 + tamanho);
        assert!(pedido_de_enviar("ana/mesa", "t", 0)
            .unwrap_err()
            .contains("volume_bytes"));
        assert!(pedido_de_ler("", 0, "{}")
            .unwrap_err()
            .contains("volume_request"));
    }
}
