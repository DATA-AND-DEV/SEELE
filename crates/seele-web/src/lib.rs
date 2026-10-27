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
//! bytes-ouro da F1 (`tests/pelo_fio_2_0.rs`), e a fronteira é provada dentro do
//! `.wasm` pelo `cargo xtask web`.

pub mod cargos;
pub mod comandos;
pub mod fluxos;
pub mod fronteira;
pub mod ligacao;
pub mod mods;
pub mod previa;
pub mod sessao;
pub mod tela;
pub mod volume;
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
/// da página. `"OutraLinha"` é o link de um Servidor de outra linha (a 0.16,
/// ou anterior), com o `versao` do `v=` quando o link o traz: é por ele que a
/// página diz «abra a 0.16» sem discar. Um corpo de convite torto sai como
/// `"Convite"`, com o `ErroDeUri` em `detalhe`.
///
/// O `bilhete` é o do `enc=`: o ponto de encontro (o STUN e o sinal da página,
/// contrato da 0.17, D7 e D8) e a escuta do anfitrião, ou `null`.
#[must_use]
pub fn link_do_navegador(texto: &str, origem: &str) -> serde_json::Value {
    use seele_proto::uri::{analisar_no_navegador, ErroDoNavegador};
    match analisar_no_navegador(texto, origem) {
        Ok(lido) => {
            let convite = lido.convite;
            let bilhete = convite.bilhete.as_ref().map(
                |bilhete| serde_json::json!({ "ponto": bilhete.ponto, "aviso": bilhete.aviso }),
            );
            serde_json::json!({
                "link": {
                    "alvo": convite.alvo,
                    "alternativos": convite.alternativos,
                    "impressao": convite.impressao_digital,
                    "convite": convite.token,
                    "sala": convite.voice_room,
                    "versao": convite.versao,
                    "bilhete": bilhete,
                }
            })
        }
        Err(ErroDoNavegador::Convite(detalhe)) => {
            serde_json::json!({ "erro": "Convite", "detalhe": format!("{detalhe:?}") })
        }
        Err(ErroDoNavegador::OutraLinha { versao }) => {
            serde_json::json!({ "erro": "OutraLinha", "versao": versao })
        }
        Err(outro) => serde_json::json!({ "erro": format!("{outro:?}") }),
    }
}

/// O corpo do `POST /sinal/1` que a página manda ao ponto do convite (contrato
/// da 0.17, D7): `SEELE-SINAL/1 CHAMA <servidor> <escuta> <U> <candidatos>`,
/// com as marcas tiradas da impressão (`Marcas::do_servidor`), como o
/// anfitrião as registra. **A página não escreve linha do protocolo**: ela
/// passa `{"impressao": hex64, "credencial": U, "candidatos": ["ip:porta", …]}`.
///
/// `{"corpo": …}`, ou `{"erro": "impressao" | "credencial" | "candidato" | "json"}`.
/// Um candidato que o fio não leva (porta baixa, «qualquer», multicast, escopo)
/// é recusado aqui, e não mandado.
#[must_use]
pub fn sinal_chama(pedido: &str) -> serde_json::Value {
    use seele_proto::encontro::{
        pedido_do_sinal, serve_de_candidato, Chama, Credencial, Marcas, LIMITE_DE_CANDIDATOS,
    };
    let erro = |qual: &str| serde_json::json!({ "erro": qual });
    let Ok(lido) = serde_json::from_str::<serde_json::Value>(pedido) else {
        return erro("json");
    };
    let Some(marcas) = lido
        .get("impressao")
        .and_then(serde_json::Value::as_str)
        .filter(|impressao| impressao.len() == 64)
        .and_then(Marcas::do_servidor)
    else {
        return erro("impressao");
    };
    let Some(credencial) = lido
        .get("credencial")
        .and_then(serde_json::Value::as_str)
        .and_then(Credencial::nova)
    else {
        return erro("credencial");
    };
    let mut candidatos = Vec::new();
    for um in lido
        .get("candidatos")
        .and_then(serde_json::Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let Some(endereco) = um
            .as_str()
            .and_then(|texto| texto.parse::<std::net::SocketAddr>().ok())
            .filter(|endereco| serve_de_candidato(*endereco))
        else {
            return erro("candidato");
        };
        if !candidatos.contains(&endereco) && candidatos.len() < LIMITE_DE_CANDIDATOS {
            candidatos.push(endereco);
        }
    }
    serde_json::json!({
        "corpo": pedido_do_sinal(&Chama {
            servidor: marcas.servidor().clone(),
            escuta: marcas.escuta().clone(),
            credencial,
            candidatos,
        })
    })
}

/// A resposta do ponto ao `POST /sinal/1`, para a página:
/// `{"aqui": {"endereco", "dicas"}}`, `{"ninguem": true}`,
/// `{"recusa": {"motivo", "agora"}}`, `{"calado": true}`, ou
/// `{"erro": "ilegivel"}` para o que não é uma das quatro linhas.
#[must_use]
pub fn sinal_resposta(corpo: &str) -> serde_json::Value {
    use seele_proto::encontro::{ler_resposta_do_sinal, RespostaDoSinal};
    match ler_resposta_do_sinal(corpo) {
        Some(RespostaDoSinal::Aqui { endereco, dicas }) => serde_json::json!({
            "aqui": {
                "endereco": endereco.to_string(),
                "dicas": dicas.iter().map(ToString::to_string).collect::<Vec<_>>(),
            }
        }),
        Some(RespostaDoSinal::Ninguem) => serde_json::json!({ "ninguem": true }),
        Some(RespostaDoSinal::Recusa { motivo, agora }) => {
            serde_json::json!({ "recusa": { "motivo": motivo.texto(), "agora": agora } })
        }
        Some(RespostaDoSinal::Calado) => serde_json::json!({ "calado": true }),
        None => serde_json::json!({ "erro": "ilegivel" }),
    }
}

#[cfg(test)]
mod o_sinal {
    #![allow(clippy::indexing_slicing, reason = "num teste, o pânico é o relatório")]
    use super::*;

    #[test]
    fn a_pagina_monta_o_chama_sem_escrever_linha_e_le_a_resposta() {
        let pedido = serde_json::json!({
            "impressao": "3cbcfb02".repeat(8),
            "credencial": "AbCdEfGhIjKlMnOpQrStUvWxYz012345",
            "candidatos": ["198.51.100.7:51000", "198.51.100.7:51000", "[2001:db8::1]:3478"],
        });
        let feito = sinal_chama(&pedido.to_string());
        assert_eq!(
            feito["corpo"],
            "SEELE-SINAL/1 CHAMA 3cbcfb023cbcfb02s 3cbcfb023cbcfb02e AbCdEfGhIjKlMnOpQrStUvWxYz012345 198.51.100.7:51000,[2001:db8::1]:3478"
        );
        for (campo, valor) in [
            ("impressao", serde_json::json!("curta")),
            ("credencial", serde_json::json!("curta")),
            ("candidatos", serde_json::json!(["198.51.100.7:80"])),
        ] {
            let mut torto = pedido.clone();
            torto[campo] = valor;
            assert!(
                sinal_chama(&torto.to_string()).get("erro").is_some(),
                "{campo}"
            );
        }
        assert_eq!(
            sinal_resposta("SEELE-SINAL/1 AQUI 203.0.113.7:8383 192.168.0.4:8383"),
            serde_json::json!({ "aqui": { "endereco": "203.0.113.7:8383", "dicas": ["192.168.0.4:8383"] } })
        );
        assert_eq!(
            sinal_resposta("SEELE-SINAL/1 CALADO"),
            serde_json::json!({ "calado": true })
        );
        assert_eq!(
            sinal_resposta("SEELE-SINAL/1 RECUSA taxa 9"),
            serde_json::json!({ "recusa": { "motivo": "taxa", "agora": 9 } })
        );
        assert_eq!(
            sinal_resposta("<html>"),
            serde_json::json!({ "erro": "ilegivel" })
        );
    }
}
