//! O cliente do celular, a parte que roda em `wasm` (ADR 0057).
//!
//! **Nenhuma linha do protocolo é escrita em JavaScript**
//! (`specs/06-clientes-gui.md:19`). A casca do celular é a mesma `ui/` do
//! desktop, e ela fala JSON com este crate. O `postcard`, o carimbo de versão e
//! a regra da mensagem desconhecida ficam todos aqui, no mesmo `seele-proto` do
//! desktop compilado para `wasm32` (a medição 2 do ADR, em
//! `spikes/proto-em-wasm/`).
//!
//! # Onde mora o `unsafe`
//!
//! Exportar uma função de um `.wasm` pede `unsafe`, e o workspace o proíbe.
//! Este crate tem a exceção do `seele-video` e do `seele-instalador` (emenda de
//! 08/10/2026 ao ADR 0057): `unsafe_code = "deny"` na folha, e o `allow` só em
//! [`fronteira`]. O resto do crate é o núcleo seguro, provado contra os
//! bytes-ouro da F1 (`tests/pelo_fio_1_0.rs`), e a fronteira é provada dentro do
//! `.wasm` pelo `cargo xtask web`.

pub mod fronteira;
pub mod voz;

use seele_proto::control::{
    decode_da_lista, encode_em, ClientMessage, ControlError, Lido, ServerMessage,
};

/// O que um quadro do servidor trouxe, para a casca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoServidor {
    /// Uma mensagem que esta build conhece, em JSON.
    Mensagem(String),
    /// Uma variante de um vocabulário mais novo. A casca pula, conta e registra
    /// a primeira, como o desktop (`seele_proto::control::Desconhecidas`).
    Desconhecida {
        /// O ordinal que veio.
        ordinal: u32,
    },
}

/// Por que a ponte não deu conta de um pedido.
#[derive(Debug)]
pub enum ErroDaPonte {
    /// O JSON da casca não é uma mensagem, ou a mensagem não virou JSON.
    Json(String),
    /// O fio recusou: carimbo de outra versão, quadro torto, campo longo demais.
    Fio(ControlError),
}

/// Uma mensagem do cliente, em JSON, para o quadro que vai ao fio.
///
/// O `carimbo` é a versão negociada no `Hello`, e não a desta build: depois
/// dele, os dois lados carimbam com o número que combinaram (adendo de
/// 08/10/2026, §4).
///
/// # Errors
///
/// [`ErroDaPonte::Json`] se o texto não for uma `ClientMessage`, e
/// [`ErroDaPonte::Fio`] se ela não passar na validação ou no teto do quadro.
pub fn codificar_do_cliente(carimbo: u8, json: &str) -> Result<Vec<u8>, ErroDaPonte> {
    let mensagem: ClientMessage =
        serde_json::from_str(json).map_err(|e| ErroDaPonte::Json(e.to_string()))?;
    encode_em(carimbo, &mensagem).map_err(ErroDaPonte::Fio)
}

/// Um quadro do servidor, para o que a casca lê.
///
/// Uma variante desconhecida chega como [`DoServidor::Desconhecida`], e não
/// como erro: o quadro tem comprimento próprio, e pulá-lo não desalinha o
/// próximo.
///
/// # Errors
///
/// [`ErroDaPonte::Fio`] para o carimbo de outra versão
/// (`ControlError::UnsupportedVersion`, com o `found` que a página usa para
/// dizer «Servidor 0.15»), o quadro torto e o grande demais.
pub fn decodificar_do_servidor(quadro: &[u8]) -> Result<DoServidor, ErroDaPonte> {
    match decode_da_lista::<ServerMessage>(quadro).map_err(ErroDaPonte::Fio)? {
        Lido::Mensagem(mensagem) => serde_json::to_string(&mensagem)
            .map(DoServidor::Mensagem)
            .map_err(|e| ErroDaPonte::Json(e.to_string())),
        Lido::Desconhecida { ordinal } => Ok(DoServidor::Desconhecida { ordinal }),
    }
}

/// Um link do navegador, lido com as regras da página (`analisar_no_navegador`
/// da F1), para a casca: `{"link": …}` ou `{"erro": "<variante>"}`.
///
/// O erro sai pelo nome da variante de `ErroDoNavegador`, que é o vocabulário
/// da página. `"SemHashesDoNavegador"` é o link de um Servidor 0.15, ou de um
/// desktop, e é por ele que a página diz isso sem discar. Um corpo de convite
/// torto sai como `"Convite"`, com o `ErroDeUri` em `detalhe`.
#[must_use]
pub fn link_do_navegador(texto: &str, origem: &str) -> serde_json::Value {
    use seele_proto::uri::{analisar_no_navegador, ErroDoNavegador};
    match analisar_no_navegador(texto, origem) {
        Ok(lido) => serde_json::json!({
            "link": {
                "alvo": lido.convite.alvo,
                "alternativos": lido.convite.alternativos,
                "impressao": lido.convite.impressao_digital,
                "convite": lido.convite.token,
                "sala": lido.convite.voice_room,
                "versao": lido.convite.versao,
                "hashes": lido.hashes,
                "ate": lido.ate,
            }
        }),
        Err(ErroDoNavegador::Convite(detalhe)) => {
            serde_json::json!({ "erro": "Convite", "detalhe": format!("{detalhe:?}") })
        }
        Err(outro) => serde_json::json!({ "erro": format!("{outro:?}") }),
    }
}
