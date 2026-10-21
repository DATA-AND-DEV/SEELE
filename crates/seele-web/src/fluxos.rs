//! **Os fluxos que o servidor abre para o navegador** (Tarefas 2.6 e 3.9 da
//! F4): a tela de quem transmite, e o anexo pedido.
//!
//! Cada fluxo unidirecional começa pelo byte do tipo (`seele_proto::stream`).
//! Daí em diante:
//! - **a tela** é a abertura de 11 bytes e os quadros, lidos pelo
//!   `LeitorDeTela` do `seele-proto` — o mesmo leitor que confere o tamanho de
//!   cada quadro antes de reservar o corpo. Cada quadro de imagem vira o
//!   `ScreenFrame` que a casca já desenha (`palco-imagem.js`), e cada quadro de
//!   som vai à mistura, pela página;
//! - **o anexo** é a entrega enquadrada (`AttachmentDelivery`: qual anexo,
//!   quantos bytes, o SHA-256) e os bytes até o fim, conferidos aqui pelo hash,
//!   como o `seele-core` confere. Os bytes não atravessam a fronteira de volta:
//!   o passo diz o pedaço da entrada que é do arquivo, e a página o guarda.
//!
//! A página só carrega os pedaços que chegam, na ordem, e segue os passos.

use seele_proto::attachment::{AttachmentDelivery, ContentDigest, CONTENT_HASH_LEN};
use seele_proto::control::decode;
use seele_proto::ids::ScreenId;
use seele_proto::quadro::{comprimento, PREFIXO_LEN};
use seele_proto::screen::{LeitorDeTela, LidoDaTela, TipoDeQuadro};
use seele_proto::stream::StreamType;
use serde_json::{json, Value};

use crate::comandos::base64;

/// O que a página faz depois de um pedaço de um fluxo.
#[derive(Debug, Clone, PartialEq)]
pub enum Passo {
    /// Um evento para a casca, no JSON do `seele_ffi::Event` (`ScreenOpened`,
    /// `ScreenFrame`, `ScreenUnreadable`, `ScreenClosed`).
    Evento(Value),
    /// Um quadro de som de uma transmissão: o Opus, para a mistura.
    SomDaTela {
        /// De que transmissão.
        tela: u32,
        /// O pacote Opus.
        opus: Vec<u8>,
    },
    /// Um anexo começou a descer.
    AnexoAbriu {
        /// Qual.
        anexo: u64,
        /// Quantos bytes vêm.
        tamanho: u64,
    },
    /// Este pedaço da entrada (`de..ate`) é do arquivo.
    BytesDoAnexo {
        /// Onde começa, no pedaço que entrou.
        de: usize,
        /// Onde acaba, sem incluir.
        ate: usize,
    },
    /// O anexo terminou: inteiro, se o hash bateu e o tamanho fechou.
    AnexoChegou {
        /// Qual.
        anexo: u64,
        /// Se chegou inteiro.
        inteiro: bool,
    },
    /// O fluxo não é lido aqui: o motivo, para o registro.
    Ignorado(String),
}

impl Passo {
    /// No JSON que a página lê.
    #[must_use]
    pub fn em_json(&self) -> Value {
        match self {
            Self::Evento(evento) => json!({ "evento": evento }),
            Self::SomDaTela { tela, opus } => json!({ "som": { "tela": tela, "opus": opus } }),
            Self::AnexoAbriu { anexo, tamanho } => {
                json!({ "anexo_abriu": { "anexo": anexo, "tamanho": tamanho } })
            }
            Self::BytesDoAnexo { de, ate } => json!({ "bytes": [de, ate] }),
            Self::AnexoChegou { anexo, inteiro } => {
                json!({ "anexo_chegou": { "anexo": anexo, "inteiro": inteiro } })
            }
            Self::Ignorado(motivo) => json!({ "ignorado": motivo }),
        }
    }
}

/// Um anexo descendo, depois da entrega.
#[derive(Debug)]
struct Descendo {
    anexo: u64,
    tamanho: u64,
    hash: [u8; CONTENT_HASH_LEN],
    digest: ContentDigest,
    recebidos: u64,
}

#[derive(Debug)]
enum Estado {
    /// Antes do byte do tipo.
    Tipo,
    /// Uma tela: o leitor, e a tela depois da abertura.
    Tela {
        leitor: LeitorDeTela,
        tela: Option<ScreenId>,
    },
    /// A entrega do anexo, enquadrada, ainda chegando.
    Entrega(Vec<u8>),
    /// Os bytes do anexo.
    Anexo(Descendo),
    /// Nada mais se lê deste fluxo.
    Fim,
}

/// Um fluxo que o servidor abriu.
#[derive(Debug)]
pub struct FluxoQueChega {
    estado: Estado,
}

impl Default for FluxoQueChega {
    fn default() -> Self {
        Self::novo()
    }
}

impl FluxoQueChega {
    /// Um fluxo antes do primeiro byte.
    #[must_use]
    pub const fn novo() -> Self {
        Self {
            estado: Estado::Tipo,
        }
    }

    /// Um pedaço do fluxo, como chegou.
    pub fn chegou(&mut self, pedaco: &[u8]) -> Vec<Passo> {
        let mut passos = Vec::new();
        let mut i = 0;
        while i < pedaco.len() {
            let resto = pedaco.get(i..).unwrap_or_default();
            match &mut self.estado {
                Estado::Fim => break,
                Estado::Tipo => {
                    let Some(&byte) = resto.first() else { break };
                    i += 1;
                    self.estado = match StreamType::decode(byte) {
                        Ok(StreamType::Screen) => Estado::Tela {
                            leitor: LeitorDeTela::novo(),
                            tela: None,
                        },
                        Ok(StreamType::Attachment) => Estado::Entrega(Vec::new()),
                        Ok(outro) => {
                            passos.push(Passo::Ignorado(format!(
                                "um fluxo {outro:?} que o navegador não lê"
                            )));
                            Estado::Fim
                        }
                        Err(erro) => {
                            passos.push(Passo::Ignorado(erro.to_string()));
                            Estado::Fim
                        }
                    };
                }
                Estado::Tela { leitor, tela } => {
                    let empurrado = leitor.empurrar(resto);
                    while let Some(lido) = leitor.proximo() {
                        passos.extend(quadro_da_tela(tela, lido));
                    }
                    if let Err(erro) = empurrado {
                        if tela.is_none() {
                            passos.push(Passo::Evento(
                                json!({ "ScreenUnreadable": { "reason": erro.to_string() } }),
                            ));
                        } else {
                            passos.push(Passo::Ignorado(format!(
                                "a transmissão terminou torta: {erro}"
                            )));
                        }
                        passos.extend(self.fechar_a_tela());
                    }
                    break;
                }
                Estado::Entrega(guardado) => {
                    // O prefixo e o quadro da entrega, que podem chegar em pedaços.
                    let falta = falta_para_a_entrega(guardado);
                    let pega = falta.min(resto.len());
                    guardado.extend_from_slice(resto.get(..pega).unwrap_or_default());
                    i += pega;
                    match entrega_inteira(guardado) {
                        Ok(None) => {}
                        Ok(Some(entrega)) => {
                            passos.push(Passo::AnexoAbriu {
                                anexo: entrega.attachment.get(),
                                tamanho: entrega.byte_size,
                            });
                            self.estado = Estado::Anexo(Descendo {
                                anexo: entrega.attachment.get(),
                                tamanho: entrega.byte_size,
                                hash: entrega.content_hash,
                                digest: ContentDigest::new(),
                                recebidos: 0,
                            });
                        }
                        Err(motivo) => {
                            passos.push(Passo::Ignorado(motivo));
                            self.estado = Estado::Fim;
                        }
                    }
                }
                Estado::Anexo(descendo) => {
                    let falta = descendo.tamanho - descendo.recebidos;
                    let pega = usize::try_from(falta)
                        .unwrap_or(usize::MAX)
                        .min(resto.len());
                    if pega < resto.len() {
                        // Mais bytes que a entrega disse: o fluxo mentiu.
                        let anexo = descendo.anexo;
                        passos.push(Passo::AnexoChegou {
                            anexo,
                            inteiro: false,
                        });
                        self.estado = Estado::Fim;
                        break;
                    }
                    descendo.digest.feed(resto.get(..pega).unwrap_or_default());
                    descendo.recebidos += pega as u64;
                    passos.push(Passo::BytesDoAnexo {
                        de: i,
                        ate: i + pega,
                    });
                    i += pega;
                }
            }
        }
        passos
    }

    /// O fluxo acabou: a tela fecha, e o anexo que não chegou inteiro é dito.
    pub fn acabou(&mut self) -> Vec<Passo> {
        match std::mem::replace(&mut self.estado, Estado::Fim) {
            Estado::Tela { tela, .. } => tela
                .map(|tela| {
                    vec![Passo::Evento(
                        json!({ "ScreenClosed": { "screen": tela.0 } }),
                    )]
                })
                .unwrap_or_default(),
            Estado::Anexo(descendo) => {
                let inteiro = descendo.recebidos == descendo.tamanho
                    && descendo.digest.finish() == descendo.hash;
                vec![Passo::AnexoChegou {
                    anexo: descendo.anexo,
                    inteiro,
                }]
            }
            Estado::Tipo | Estado::Entrega(_) | Estado::Fim => Vec::new(),
        }
    }

    fn fechar_a_tela(&mut self) -> Vec<Passo> {
        let passos = self.acabou();
        self.estado = Estado::Fim;
        passos
    }
}

/// Um item do leitor de tela, nos passos da página: a abertura vira o
/// `ScreenOpened`, a imagem o `ScreenFrame`, e o som vai à mistura.
fn quadro_da_tela(tela: &mut Option<ScreenId>, lido: LidoDaTela) -> Vec<Passo> {
    match lido {
        LidoDaTela::Abertura(cabecalho) => {
            *tela = Some(cabecalho.screen);
            vec![Passo::Evento(json!({ "ScreenOpened": {
                "screen": cabecalho.screen.0,
                "width": cabecalho.width,
                "height": cabecalho.height,
            } }))]
        }
        LidoDaTela::Quadro { tipo, bytes } => {
            let Some(de_quem) = *tela else {
                return Vec::new();
            };
            if tipo == TipoDeQuadro::Som {
                vec![Passo::SomDaTela {
                    tela: de_quem.0,
                    opus: bytes,
                }]
            } else {
                vec![Passo::Evento(json!({ "ScreenFrame": {
                    "screen": de_quem.0,
                    "key": tipo == TipoDeQuadro::Chave,
                    "data": base64(&bytes),
                } }))]
            }
        }
    }
}

/// Quantos bytes faltam para a entrega enquadrada estar inteira: o prefixo, e
/// depois o quadro que ele anuncia.
fn falta_para_a_entrega(guardado: &[u8]) -> usize {
    let Some(prefixo) = guardado.get(..PREFIXO_LEN) else {
        return PREFIXO_LEN - guardado.len();
    };
    let mut lido = [0_u8; PREFIXO_LEN];
    lido.copy_from_slice(prefixo);
    // Um prefixo que não serve é recusado por `entrega_inteira`, e nada mais é
    // pedido.
    comprimento(lido).map_or(0, |tamanho| PREFIXO_LEN + tamanho - guardado.len())
}

/// A entrega, quando o quadro dela já chegou inteiro.
fn entrega_inteira(guardado: &[u8]) -> Result<Option<AttachmentDelivery>, String> {
    let Some(prefixo) = guardado.get(..PREFIXO_LEN) else {
        return Ok(None);
    };
    let mut lido = [0_u8; PREFIXO_LEN];
    lido.copy_from_slice(prefixo);
    let tamanho = comprimento(lido).map_err(|erro| format!("a entrega do anexo: {erro:?}"))?;
    let Some(quadro) = guardado.get(PREFIXO_LEN..PREFIXO_LEN + tamanho) else {
        return Ok(None);
    };
    decode::<AttachmentDelivery>(quadro)
        .map(Some)
        .map_err(|erro| format!("a entrega do anexo: {erro:?}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use seele_proto::attachment::hash;
    use seele_proto::control::encode_em;
    use seele_proto::ids::AttachmentId;
    use seele_proto::quadro::enquadrar;
    use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource};
    use seele_proto::version::PROTOCOL_VERSION;

    fn tela(quadros: &[(TipoDeQuadro, &[u8])]) -> Vec<u8> {
        let mut fluxo = vec![StreamType::Screen.byte()];
        let mut abertura = [0_u8; seele_proto::screen::SCREEN_HEADER_LEN];
        ScreenHeader {
            version: 1,
            screen: ScreenId(3),
            source: ScreenSource::Monitor,
            codec: ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        }
        .encode(&mut abertura)
        .unwrap();
        fluxo.extend(abertura);
        for (tipo, corpo) in quadros {
            fluxo.push(*tipo as u8);
            fluxo.extend(u32::try_from(corpo.len()).unwrap().to_be_bytes());
            fluxo.extend(*corpo);
        }
        fluxo
    }

    /// Em pedaços de `tamanho`, como a rede entrega.
    fn em_pedacos(
        fluxo: &mut FluxoQueChega,
        bytes: &[u8],
        tamanho: usize,
    ) -> (Vec<Passo>, Vec<u8>) {
        let mut passos = Vec::new();
        let mut arquivo = Vec::new();
        for pedaco in bytes.chunks(tamanho) {
            for passo in fluxo.chegou(pedaco) {
                if let Passo::BytesDoAnexo { de, ate } = passo {
                    arquivo.extend(&pedaco[de..ate]);
                } else {
                    passos.push(passo);
                }
            }
        }
        passos.extend(fluxo.acabou());
        (passos, arquivo)
    }

    #[test]
    fn a_tela_abre_desenha_toca_e_fecha_em_qualquer_pedaco() {
        let bytes = tela(&[
            (TipoDeQuadro::Chave, b"chave"),
            (TipoDeQuadro::Som, b"opus"),
            (TipoDeQuadro::Comum, b"comum"),
        ]);
        for tamanho in [1, 2, 7, 1000] {
            let (passos, _) = em_pedacos(&mut FluxoQueChega::novo(), &bytes, tamanho);
            assert_eq!(
                passos,
                vec![
                    Passo::Evento(
                        json!({ "ScreenOpened": { "screen": 3, "width": 1280, "height": 720 } })
                    ),
                    Passo::Evento(
                        json!({ "ScreenFrame": { "screen": 3, "key": true, "data": base64(b"chave") } })
                    ),
                    Passo::SomDaTela {
                        tela: 3,
                        opus: b"opus".to_vec()
                    },
                    Passo::Evento(
                        json!({ "ScreenFrame": { "screen": 3, "key": false, "data": base64(b"comum") } })
                    ),
                    Passo::Evento(json!({ "ScreenClosed": { "screen": 3 } })),
                ],
                "em pedaços de {tamanho}"
            );
        }
    }

    #[test]
    fn uma_abertura_torta_e_ilegivel_e_um_quadro_torto_fecha_a_tela() {
        let mut fluxo = FluxoQueChega::novo();
        let mut torto = vec![StreamType::Screen.byte()];
        torto.extend([9_u8; 11]);
        let passos = fluxo.chegou(&torto);
        assert!(
            matches!(&passos[..], [Passo::Evento(evento)] if evento.get("ScreenUnreadable").is_some()),
            "{passos:?}"
        );
        assert!(fluxo.acabou().is_empty());

        let mut bytes = tela(&[(TipoDeQuadro::Chave, b"x")]);
        bytes.extend([7, 0, 0, 0, 1, 0]);
        let (passos, _) = em_pedacos(&mut FluxoQueChega::novo(), &bytes, 1000);
        assert!(
            matches!(passos.last(), Some(Passo::Evento(evento)) if evento.get("ScreenClosed").is_some())
        );
        assert!(passos
            .iter()
            .any(|passo| matches!(passo, Passo::Ignorado(_))));
    }

    fn anexo(corpo: &[u8], hash_dito: [u8; 32]) -> Vec<u8> {
        let mut fluxo = vec![StreamType::Attachment.byte()];
        let entrega = AttachmentDelivery {
            attachment: AttachmentId(41),
            byte_size: corpo.len() as u64,
            content_hash: hash_dito,
        };
        fluxo.extend(enquadrar(&encode_em(PROTOCOL_VERSION, &entrega).unwrap()).unwrap());
        fluxo.extend(corpo);
        fluxo
    }

    #[test]
    fn o_anexo_desce_conferido_pelo_hash_em_qualquer_pedaco() {
        let corpo: Vec<u8> = (0..5000_u32).map(|n| (n % 251) as u8).collect();
        for tamanho in [1, 3, 64, 100_000] {
            let (passos, arquivo) = em_pedacos(
                &mut FluxoQueChega::novo(),
                &anexo(&corpo, hash(&corpo)),
                tamanho,
            );
            assert_eq!(arquivo, corpo, "em pedaços de {tamanho}");
            assert_eq!(
                passos,
                vec![
                    Passo::AnexoAbriu {
                        anexo: 41,
                        tamanho: 5000
                    },
                    Passo::AnexoChegou {
                        anexo: 41,
                        inteiro: true
                    },
                ]
            );
        }
    }

    #[test]
    fn o_anexo_com_hash_errado_curto_ou_longo_nao_chega_inteiro() {
        let corpo = b"um arquivo".to_vec();
        let (passos, _) = em_pedacos(&mut FluxoQueChega::novo(), &anexo(&corpo, [0; 32]), 4);
        assert_eq!(
            passos.last(),
            Some(&Passo::AnexoChegou {
                anexo: 41,
                inteiro: false
            })
        );

        let mut curto = anexo(&corpo, hash(&corpo));
        curto.truncate(curto.len() - 1);
        let (passos, _) = em_pedacos(&mut FluxoQueChega::novo(), &curto, 4);
        assert_eq!(
            passos.last(),
            Some(&Passo::AnexoChegou {
                anexo: 41,
                inteiro: false
            })
        );

        let mut longo = anexo(&corpo, hash(&corpo));
        longo.push(0);
        let (passos, _) = em_pedacos(&mut FluxoQueChega::novo(), &longo, 1000);
        assert_eq!(
            passos.last(),
            Some(&Passo::AnexoChegou {
                anexo: 41,
                inteiro: false
            })
        );
    }

    #[test]
    fn um_tipo_que_o_navegador_nao_le_e_dito_e_ignorado() {
        let mut fluxo = FluxoQueChega::novo();
        let passos = fluxo.chegou(&[StreamType::ModVolume.byte(), 1, 2, 3]);
        assert!(matches!(&passos[..], [Passo::Ignorado(_)]));
        assert!(fluxo.chegou(&[1, 2]).is_empty());
        let passos = FluxoQueChega::novo().chegou(&[200]);
        assert!(matches!(&passos[..], [Passo::Ignorado(_)]));
    }
}
