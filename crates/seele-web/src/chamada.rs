//! Fronteira pura da chamada pessoal: canonização e Ed25519 estrito.
//! WebCrypto assina com chave não exportável; nunca decide sozinho se uma
//! assinatura recebida é válida (aceita chaves pequenas em alguns motores).

use ed25519_dalek::{Signature, VerifyingKey};
use seele_proto::chamada::{self, Convite, Operacao, Pedido};
use serde_json::{json, Value};

/// A mesma política de core/encontro: chave forte e `verify_strict`.
#[must_use]
pub fn verificar(chave: &[u8; 32], mensagem: &[u8], assinatura: &chamada::Assinatura) -> bool {
    let Ok(chave) = VerifyingKey::from_bytes(chave) else {
        return false;
    };
    !chave.is_weak()
        && chave
            .verify_strict(mensagem, &Signature::from_bytes(&assinatura.bytes()))
            .is_ok()
}

fn convite_verificado(convite: &Convite, agora: u64) -> bool {
    chamada::validar_convite(convite, agora).is_ok()
        && verificar(
            &convite.dono,
            &chamada::mensagem_do_convite(convite),
            &convite.assinatura,
        )
}

fn pedido_verificado(pedido: &Pedido, agora: u64) -> bool {
    if chamada::validar_pedido(pedido, agora).is_err() {
        return false;
    }
    if let Operacao::Registrar { convite } | Operacao::Chamar { convite, .. } = &pedido.operacao {
        if !convite_verificado(convite, agora) {
            return false;
        }
    }
    chamada::mensagem_do_pedido(pedido)
        .is_ok_and(|bytes| verificar(&pedido.remetente, &bytes, &pedido.assinatura))
}

/// Configuração pública derivada exclusivamente das constantes do protocolo.
struct Configuracao {
    origem_dos_links: String,
    url_do_encontro: String,
    ice_servers: [ServidorIce; 1],
}

struct ServidorIce {
    urls: String,
}

impl Configuracao {
    fn em_json(self) -> Value {
        let servidores: Vec<Value> = self
            .ice_servers
            .into_iter()
            .map(|servidor| json!({"urls": servidor.urls}))
            .collect();
        json!({
            "origem_dos_links": self.origem_dos_links,
            "url_do_encontro": self.url_do_encontro,
            "ice_servers": servidores,
        })
    }

    fn do_produto() -> Self {
        Self {
            origem_dos_links: format!("https://{}", seele_proto::uri::ORIGEM_DO_NAVEGADOR),
            url_do_encontro: format!("https://{}/chamada/1", seele_proto::encontro::PONTO_PADRAO),
            ice_servers: [ServidorIce {
                urls: format!(
                    "stun:{}:{}",
                    seele_proto::encontro::PONTO_PADRAO,
                    seele_proto::encontro::PORTA_PADRAO
                ),
            }],
        }
    }
}

/// Executa uma intenção da casca sem E/S nem material secreto.
#[must_use]
pub fn operar(texto: &str) -> Value {
    if texto.len() > chamada::MAX_RESPOSTA * 2 {
        return json!({"erro":"Grande"});
    }
    let Ok(lido) = serde_json::from_str::<Value>(texto) else {
        return json!({"erro":"Invalido"});
    };
    let campo = |nome: &str| lido.get(nome).unwrap_or(&Value::Null);
    let agora = lido.get("agora").and_then(Value::as_u64).unwrap_or(0);
    match lido.get("acao").and_then(Value::as_str) {
        Some("configuracao") => Configuracao::do_produto().em_json(),
        Some("codigo_da_chave") => {
            let Ok(chave) = serde_json::from_value::<[u8; 32]>(campo("chave").clone()) else {
                return json!({"erro":"Invalido"});
            };
            json!({"codigo":seele_proto::codigo::Codigo::da_chave(&chave).to_string()})
        }
        Some("codigo_ler") => {
            match seele_proto::codigo::Codigo::ler(campo("codigo").as_str().unwrap_or("")) {
                Ok(codigo) => json!({"codigo":codigo.to_string()}),
                Err(seele_proto::codigo::CodigoInvalido::Verificador) => {
                    json!({"erro":"CodigoComErro"})
                }
                Err(_) => json!({"erro":"CodigoInvalido"}),
            }
        }
        Some("convite_mensagem" | "convite_verificar") => {
            let Ok(convite) = serde_json::from_value::<Convite>(campo("convite").clone()) else {
                return json!({"erro":"Invalido"});
            };
            if campo("acao") == "convite_mensagem" {
                if chamada::validar_convite(&convite, agora).is_err() {
                    return json!({"erro":"Convite"});
                }
                json!({"bytes":chamada::mensagem_do_convite(&convite)})
            } else if convite_verificado(&convite, agora) {
                json!({"convite":convite})
            } else {
                json!({"erro":"AssinaturaOuValidade"})
            }
        }
        Some("pedido_mensagem" | "pedido_verificar" | "pedido_escrever") => {
            let Ok(pedido) = serde_json::from_value::<Pedido>(campo("pedido").clone()) else {
                return json!({"erro":"Invalido"});
            };
            if campo("acao") == "pedido_verificar" {
                if pedido_verificado(&pedido, agora) {
                    json!({"pedido":pedido})
                } else {
                    json!({"erro":"AssinaturaOuValidade"})
                }
            } else if campo("acao") == "pedido_escrever" {
                match chamada::escrever_pedido(&pedido) {
                    Ok(texto) => json!({"texto":texto}),
                    Err(_) => json!({"erro":"Invalido"}),
                }
            } else {
                match chamada::mensagem_do_pedido(&pedido) {
                    Ok(bytes) => json!({"bytes":bytes}),
                    Err(_) => json!({"erro":"Invalido"}),
                }
            }
        }
        Some("resposta_ler") => {
            match chamada::ler_resposta(lido.get("texto").and_then(Value::as_str).unwrap_or("")) {
                Ok(resposta) => json!({"resposta":resposta}),
                Err(_) => json!({"erro":"Invalido"}),
            }
        }
        Some("convite_escrever" | "convite_ler") => {
            let convite = if campo("acao") == "convite_ler" {
                campo("fragmento")
                    .as_str()
                    .ok_or(chamada::Erro::Invalido)
                    .and_then(chamada::ler_convite)
            } else {
                serde_json::from_value::<Convite>(campo("convite").clone())
                    .map_err(|_| chamada::Erro::Invalido)
            };
            match convite {
                Ok(convite) if convite_verificado(&convite, agora) => {
                    match chamada::escrever_convite(&convite) {
                        Ok(fragmento) => json!({"convite":convite,"fragmento":fragmento}),
                        Err(_) => json!({"erro":"Invalido"}),
                    }
                }
                _ => json!({"erro":"AssinaturaOuValidade"}),
            }
        }
        Some("controle_escrever") => {
            match serde_json::from_value::<chamada::Controle>(campo("controle").clone())
                .ok()
                .and_then(|v| chamada::escrever_controle(&v).ok())
            {
                Some(bytes) => json!({"bytes":bytes}),
                None => json!({"erro":"Invalido"}),
            }
        }
        Some("controle_ler") => {
            match serde_json::from_value::<Vec<u8>>(campo("bytes").clone())
                .ok()
                .and_then(|v| chamada::ler_controle(&v).ok())
            {
                Some(controle) => json!({"controle":controle}),
                None => json!({"erro":"Invalido"}),
            }
        }
        Some("tela_empacotar") => {
            match serde_json::from_value::<chamada::TelaPessoal>(campo("mensagem").clone())
                .ok()
                .and_then(|v| chamada::escrever_tela(&v).ok())
            {
                Some(bytes) => json!({"bytes":bytes}),
                None => json!({"erro":"Invalido"}),
            }
        }
        Some("tela_desempacotar") => {
            match serde_json::from_value::<Vec<u8>>(campo("bytes").clone())
                .ok()
                .and_then(|v| chamada::ler_tela(&v).ok())
            {
                Some(resposta) => json!({"resposta":resposta}),
                None => json!({"erro":"Invalido"}),
            }
        }
        Some("codificacao_da_tela") => {
            let Ok(limites) = serde_json::from_value::<seele_sessao::retrato::LimitesDeTela>(
                campo("limites").clone(),
            ) else {
                return json!({"erro":"Invalido"});
            };
            let medidas = |nome: &str| {
                lido.get(nome)
                    .and_then(Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
            };
            match (medidas("largura"), medidas("altura")) {
                (Some(largura), Some(altura)) => {
                    json!({"resposta":crate::tela::codificacao(&limites, largura, altura)})
                }
                _ => json!({"erro":"Invalido"}),
            }
        }
        Some("abertura_da_tela") => {
            let medidas = |nome: &str| {
                lido.get(nome)
                    .and_then(Value::as_u64)
                    .and_then(|v| u16::try_from(v).ok())
            };
            match (medidas("largura"), medidas("altura")) {
                (Some(largura), Some(altura)) => {
                    match crate::tela::abertura(1, false, largura, altura) {
                        Ok(bytes) => json!({"bytes":bytes}),
                        Err(_) => json!({"erro":"Invalido"}),
                    }
                }
                _ => json!({"erro":"Invalido"}),
            }
        }
        _ => json!({"erro":"Operacao"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn configuracao_publica_usa_as_constantes_do_protocolo() {
        assert_eq!(
            operar(r#"{"acao":"configuracao"}"#),
            json!({
                "origem_dos_links": format!("https://{}", seele_proto::uri::ORIGEM_DO_NAVEGADOR),
                "url_do_encontro": format!("https://{}/chamada/1", seele_proto::encontro::PONTO_PADRAO),
                "ice_servers": [{"urls": format!("stun:{}:{}", seele_proto::encontro::PONTO_PADRAO, seele_proto::encontro::PORTA_PADRAO)}],
            })
        );
        assert_eq!(
            operar(r#"{"acao":"configuracao","url_do_encontro":"https://falso.exemplo"}"#),
            operar(r#"{"acao":"configuracao"}"#)
        );
    }

    #[test]
    fn codigo_da_chave_e_verificador_sao_os_mesmos_do_nativo_com_espacos() {
        assert_eq!(
            operar(
                r#"{"acao":"codigo_da_chave","chave":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}"#
            ).get("codigo").and_then(Value::as_str),
            Some("CSM7-NBFR-CAYQ-EV4Y")
        );
        assert_eq!(
            operar(r#"{"acao":"codigo_ler","codigo":"c s m 7 n b f r c a y q e v 4 y"}"#)
                .get("codigo")
                .and_then(Value::as_str),
            Some("CSM7-NBFR-CAYQ-EV4Y")
        );
        assert_eq!(
            operar(r#"{"acao":"codigo_ler","codigo":"c s m 7 n b f r c a y q e v 4 z"}"#)
                .get("erro")
                .and_then(Value::as_str),
            Some("CodigoComErro")
        );
        assert_eq!(
            operar(r#"{"acao":"codigo_ler","codigo":"host.exemplo"}"#)
                .get("erro")
                .and_then(Value::as_str),
            Some("CodigoInvalido")
        );
    }

    #[test]
    fn a_chave_fraca_aceita_pelo_webcrypto_nao_entra() {
        let mut identidade = [0; 32];
        identidade[0] = 1;
        let assinatura = chamada::Assinatura {
            primeira: identidade,
            segunda: [0; 32],
        };
        assert!(!verificar(&identidade, b"qualquer mensagem", &assinatura));
    }

    #[test]
    fn assinatura_valida_e_alterada_passam_pelo_mesmo_verificador() {
        let chave = SigningKey::from_bytes(&[7; 32]);
        let mensagem = b"mensagem local canonica";
        let assinatura = chamada::Assinatura::de_bytes(chave.sign(mensagem).to_bytes());
        assert!(verificar(
            &chave.verifying_key().to_bytes(),
            mensagem,
            &assinatura
        ));
        assert!(!verificar(
            &chave.verifying_key().to_bytes(),
            b"outra",
            &assinatura
        ));
    }
}
