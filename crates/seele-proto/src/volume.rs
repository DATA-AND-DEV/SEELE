//! O cabeçalho do fluxo de volume de um MOD — ADR 0048.
//!
//! # Por que um fluxo, e não o controle
//!
//! O caminho antigo empurrava uma imagem pelo canal de controle, em fragmentos
//! de base64, a vinte quadros por segundo. O ADR mediu o que isso custa: uma
//! imagem de 10 MiB vira 2 331 pedidos, e cada pedido atravessa o QuickJS.
//!
//! O ADR 0027 já tinha resolvido o mesmo problema para anexos, e a decisão foi
//! **estender aquele caminho** em vez de construir um segundo: um fluxo
//! unidirecional por transferência, cabeçalho e depois os bytes crus, blocos de
//! 64 KiB entre a rede e o disco, e a resposta pelo controle.
//!
//! # O que este cabeçalho **não** tem, e por quê
//!
//! **Não tem caminho.** O arquivo é nomeado pelo MOD, dentro do `aoPedir` que
//! autorizou, e guardado na espera. Um caminho vindo daqui seria um caminho
//! escolhido por quem envia — e a avaliação de PERFIS 1.0.0 registrou
//! justamente «o caminho é gerado pelo servidor, nunca vem do pedido» como a
//! propriedade que fecha a travessia por construção.
//!
//! **Não tem tamanho declarado.** O anexo declara o seu porque há um teto a
//! conferir antes do primeiro byte; o volume de um MOD **não tem teto** — foi a
//! decisão registrada no ADR 0048 —, e um número que ninguém confere é um
//! número que mente sem custo.
//!
//! **Não tem tipo declarado.** Quem confere os bytes é o servidor, com a mesma
//! tabela de quatro formatos da prévia de anexo, contra os tipos que o MOD
//! declarou na espera. Uma declaração aqui seria uma segunda fonte para a mesma
//! resposta, e a errada é sempre a que quem envia escolhe.
//!
//! O que sobra são dois nomes: de quem é a pasta, e qual autorização isto usa.

use serde::{Deserialize, Serialize};

/// Teto do identificador do MOD neste cabeçalho.
///
/// O mesmo que `ClientMessage::ModRequest` usa para o mesmo campo. Duas medidas
/// para o mesmo nome seriam um jeito de o fio discordar de si mesmo.
pub const MAX_MOD_ID_LEN: usize = 128;

/// Teto do token.
///
/// O token nasce dentro do MOD e o servidor não o interpreta — ele só o casa
/// com uma espera registrada. O teto existe porque tudo o que atravessa este
/// fio tem um, e não porque 128 seja um número significativo.
pub const MAX_TOKEN_LEN: usize = 128;

/// O que vai na frente dos bytes de um fluxo de volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeHeader {
    /// `autor/nome` do MOD em cuja pasta os bytes vão cair.
    ///
    /// Conferido contra a espera: uma espera é de um MOD, e um token certo com
    /// o nome de outro MOD não vale.
    pub mod_id: String,
    /// A autorização que o MOD registrou com `volume.esperar`.
    ///
    /// **Vale uma vez.** Consumida no primeiro fluxo que a case — sem isso, um
    /// token vazado viraria um lugar de escrita permanente na pasta do MOD,
    /// para quem o tivesse.
    pub token: String,
}

/// Por que um fluxo de volume não foi aceito.
///
/// Enumerado, por `specs/02-protocolo.md`, e cada variante carrega só o que uma
/// casca precisa para escrever a própria frase — ADR 0012.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolumeRefusal {
    /// Não há espera com este token — para este MOD, para esta pessoa, e
    /// dentro do prazo.
    ///
    /// **Uma recusa só para os quatro casos**, e é de propósito: distinguir
    /// «token errado» de «prazo vencido» de «espera de outra pessoa» contaria a
    /// quem tenta qual metade do palpite acertou.
    SemEspera,
    /// Os bytes não são de um tipo que o MOD declarou aceitar.
    TipoRecusado,
    /// O disco de quem hospeda recusou a escrita.
    NaoGravei,
    /// O fluxo terminou antes do que ele dizia carregar.
    Incompleto,
}

impl crate::control::Validate for VolumeHeader {
    /// Se este cabeçalho cabe nas medidas do fio.
    ///
    /// Pelo mesmo `Validate` do `AttachmentHeader`, e não por um método
    /// próprio: é o que `crate::frame::read` cobra de todo cabeçalho antes de
    /// devolvê-lo, e um cabeçalho com conferência própria seria um cabeçalho
    /// que alguém esquece de conferir.
    ///
    /// **Vazio é recusado nos dois.** Um `mod_id` vazio não nomeia pasta
    /// nenhuma, e um token vazio casaria com uma espera que ninguém registrou
    /// se algum dia o mapa aceitasse a chave vazia.
    ///
    /// # Errors
    ///
    /// [`crate::control::ControlError`] para o primeiro campo fora de medida.
    fn validate(&self) -> Result<(), crate::control::ControlError> {
        for (campo, valor, teto) in [
            ("mod_id", &self.mod_id, MAX_MOD_ID_LEN),
            ("token", &self.token, MAX_TOKEN_LEN),
        ] {
            if valor.is_empty() || valor.len() > teto {
                return Err(crate::control::ControlError::FieldOutOfRange { field: campo });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::control::{ControlError, Validate};

    #[test]
    fn um_cabecalho_sem_nome_ou_sem_token_nao_vale() {
        for (mod_id, token, culpado) in [
            ("", "t", "mod_id"),
            ("seele/perfis", "", "token"),
            (&*"a".repeat(MAX_MOD_ID_LEN + 1), "t", "mod_id"),
            ("seele/perfis", &*"t".repeat(MAX_TOKEN_LEN + 1), "token"),
        ] {
            let cabecalho = VolumeHeader {
                mod_id: mod_id.to_owned(),
                token: token.to_owned(),
            };
            assert_eq!(
                cabecalho.validate(),
                Err(ControlError::FieldOutOfRange { field: culpado }),
                "{mod_id:?} {token:?}"
            );
        }
    }

    #[test]
    fn um_cabecalho_no_tamanho_vale() {
        assert_eq!(
            VolumeHeader {
                mod_id: "seele/perfis".to_owned(),
                token: "t".repeat(MAX_TOKEN_LEN),
            }
            .validate(),
            Ok(())
        );
    }

    /// **O cabeçalho não carrega caminho, tamanho nem tipo.**
    ///
    /// Os três já foram propostos e os três foram recusados por escrito no
    /// cabeçalho deste módulo. O guarda existe porque a razão de cada um é
    /// fácil de esquecer e o custo de acrescentá-los é invisível em revisão:
    /// um caminho aqui é travessia, um tamanho aqui é um número que ninguém
    /// confere, e um tipo aqui é a segunda fonte para uma resposta que o
    /// servidor já tem nos bytes.
    #[test]
    fn o_cabecalho_tem_dois_campos_e_nao_tres() {
        let cabecalho = VolumeHeader {
            mod_id: "seele/perfis".to_owned(),
            token: "t".to_owned(),
        };
        let json = serde_json::to_value(&cabecalho).expect("serializa");
        let objeto = json.as_object().expect("é objeto");
        let mut campos: Vec<&str> = objeto.keys().map(String::as_str).collect();
        campos.sort_unstable();
        assert_eq!(
            campos,
            vec!["mod_id", "token"],
            "o cabeçalho do volume ganhou campo; leia o topo deste módulo antes"
        );
    }
}

/// Requisição de imagem em um fluxo bidirecional autenticado. Não usa o controle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PedidoDeImagem {
    /// A espera foi autorizada pelo MOD; o tamanho limita e detecta truncamento.
    Enviar {
        /// Autorização de uso único emitida pelo MOD.
        cabecalho: VolumeHeader,
        /// Tamanho conferido durante a escrita e no EOF.
        bytes: u64,
    },
    /// O MOD autoriza a leitura pelo seu pedido habitual, antes de servir bytes.
    Ler {
        /// MOD habilitado que decide o acesso.
        mod_id: String,
        /// Canal do pedido; zero para escopo de servidor.
        channel: crate::ids::ChannelId,
        /// Pedido pequeno de autorização, sem bytes de mídia.
        payload: String,
    },
}

impl crate::control::Validate for PedidoDeImagem {
    fn validate(&self) -> Result<(), crate::control::ControlError> {
        match self {
            Self::Enviar { cabecalho, bytes } => {
                cabecalho.validate()?;
                if *bytes == 0 || *bytes > crate::midia_de_mod::TETO_DE_ARQUIVO as u64 {
                    return Err(crate::control::ControlError::FieldOutOfRange {
                        field: "volume_bytes",
                    });
                }
            }
            Self::Ler {
                mod_id, payload, ..
            } => {
                if mod_id.is_empty() || mod_id.len() > MAX_MOD_ID_LEN || payload.len() > 12 * 1024 {
                    return Err(crate::control::ControlError::FieldOutOfRange {
                        field: "volume_request",
                    });
                }
            }
        }
        Ok(())
    }
}

/// O recebimento só é confirmado depois de gravar; a leitura anuncia o tamanho.
#[derive(Debug, Serialize, Deserialize)]
pub struct RespostaDeImagem {
    /// Motivo da recusa, quando houver.
    pub erro: Option<String>,
    /// Bytes que seguem o cabeçalho; zero na confirmação de envio.
    pub bytes: u64,
}
impl crate::control::Validate for RespostaDeImagem {
    fn validate(&self) -> Result<(), crate::control::ControlError> {
        if self.bytes > crate::midia_de_mod::TETO_DE_ARQUIVO as u64
            || self.erro.as_ref().is_some_and(|e| e.len() > 1024)
        {
            return Err(crate::control::ControlError::FieldOutOfRange {
                field: "volume_reply",
            });
        }
        Ok(())
    }
}
