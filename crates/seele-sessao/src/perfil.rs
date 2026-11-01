//! **O perfil público, do lado de quem pede** (SEELE 0.19, protocolo 12,
//! `docs/0.19/perfis-oficiais.md` §5.1): o que o desktop e o navegador fazem
//! igual antes de escrever no fio.
//!
//! O fold das mensagens do servidor mora em [`crate::state::Room`]. Aqui mora o
//! resto, que é puro e é comum aos dois produtores:
//!
//! - o **erro tipado** ([`ErroDoPerfil`]) que a casca recebe em vez de um texto;
//! - a **validação dos campos**, que é a de [`seele_proto::perfil::validar`] e
//!   de nenhuma outra, devolvida por campo ([`montar_gravar`]);
//! - a **conferência da imagem** antes de enviar ([`conferir_imagem`]): tamanho
//!   e tipo pelos **bytes**, com o HEIC nomeado;
//! - o **portão de versão** ([`portao_do_cliente`] e
//!   [`portao_do_pedido_de_imagem`]): um verbo do protocolo 12 não sai para um
//!   servidor que negociou 11 — ele leria uma variante que não conhece e o fluxo
//!   de controle desalinharia. Volta [`ErroDoPerfil::ServidorAntigo`], tipado, e
//!   a casca diz o que aconteceu em vez de calar.
//!
//! # Os nomes que o servidor devolve no fluxo de imagem
//!
//! A resposta de um envio ou de uma leitura de imagem de perfil (`RespostaDeImagem`)
//! carrega o motivo como **nome**, e não como frase (a regra da lista
//! `recusas.volume`). [`nome_da_recusa`] é a tabela dos nomes, em kebab-case,
//! um por [`RecusaDePerfil`] mais o `mudou` da leitura; [`erro_do_servidor`] é a
//! volta. Quem escreve o lado do servidor (`seele-server/src/perfis/imagens.rs`)
//! usa a mesma tabela, e o teste `os_nomes_das_recusas_fazem_a_volta` prende as
//! duas direções.

use seele_proto::control::ClientMessage;
use seele_proto::perfil::{
    validar, Efeito, EntradaDePerfil, ErroDeCampo, RecusaDePerfil, TipoDaImagem, TETO_DA_IMAGEM,
};
use seele_proto::vocabulario::{desde_do_cliente, desde_o_pedido_de_imagem};
use seele_proto::volume::PedidoDeImagem;

/// Tudo o que pode dar errado num pedido de perfil, enumerado.
///
/// Como `ConnectionError`, sem texto livre: a casca escreve a frase de cada
/// variante. Serializa pelo nome da variante (`"Heic"`) ou por
/// `{"Campos": {"campos": […]}}`, que é o que a página lê.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ErroDoPerfil {
    /// Não há sessão, ou ela já acabou.
    NotConnected,
    /// **O servidor negociou menos que o 12**: ele não conhece o perfil
    /// público, e nada foi enviado. Os 0.18 seguem vendo o apelido e o ícone.
    ServidorAntigo {
        /// O número que a conexão negociou.
        negociado: u8,
        /// O primeiro número que entende o pedido.
        exige: u8,
    },
    /// Algum campo não passou em [`seele_proto::perfil::validar`]: um erro por
    /// campo, na ordem do formulário. Nada foi enviado.
    Campos {
        /// Qual campo, e por quê.
        campos: Vec<ErroDeCampo>,
    },
    /// A imagem não tem nenhum byte.
    ImagemVazia,
    /// A imagem passa do teto de [`TETO_DA_IMAGEM`]. Nada foi enviado.
    ImagemGrandeDemais {
        /// O teto, em bytes.
        limite_bytes: u64,
    },
    /// O arquivo é HEIC: reconhecido, e não aceito. É o que as Fotos do iPhone
    /// entregam, e «formato desconhecido» mandaria procurar defeito num arquivo
    /// que não tem nenhum.
    Heic,
    /// Os bytes não são PNG, JPEG, WebP nem GIF.
    TipoRecusado,
    /// O arquivo não abriu ou não foi lido por inteiro.
    ArquivoIlegivel,
    /// O hash pedido não é mais o atual: o perfil mudou, e a casca deve reler
    /// [`crate::state::Room::perfis_em_ordem`] antes de pedir de novo.
    Mudou,
    /// O servidor recusou, com este motivo.
    Recusada {
        /// O motivo.
        motivo: RecusaDePerfil,
    },
    /// O servidor respondeu com um nome que esta build não conhece (um
    /// vocabulário mais novo). O nome vai ao registro, não à tela.
    RecusaDesconhecida,
    /// O fluxo acabou antes do tamanho anunciado, ou os bytes recebidos não
    /// são os do hash pedido.
    Incompleto,
    /// Ninguém respondeu no prazo, ou a sessão acabou no meio.
    SemResposta,
}

/// Os campos do perfil como a casca os manda (o JSON do `invoke`).
///
/// `Serialize` também: o **perfil padrão** (o «Seu perfil» fora de qualquer
/// servidor) guarda nesta máquina a entrada já validada
/// ([`entrada_validada`]), e a devolve à casca no mesmo formato.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct EntradaDoPerfil {
    /// Nome exibido.
    pub nome_exibido: String,
    /// Pronomes.
    pub pronomes: String,
    /// Status.
    pub status: String,
    /// Bio.
    pub bio: String,
    /// `#rrggbb`.
    pub cor: String,
    /// A moldura.
    pub efeito: Efeito,
}

/// **A validação e a mensagem**: o `GravarPerfil` com os textos aparados, ou os
/// erros por campo. É a mesma [`validar`] que o servidor aplica ao receber.
///
/// # Errors
///
/// [`ErroDoPerfil::Campos`], com todos os campos inválidos de uma vez.
pub fn montar_gravar(
    revisao: u32,
    entrada: &EntradaDoPerfil,
) -> Result<ClientMessage, ErroDoPerfil> {
    let validado = validar(&EntradaDePerfil {
        nome_exibido: &entrada.nome_exibido,
        pronomes: &entrada.pronomes,
        status: &entrada.status,
        bio: &entrada.bio,
        cor: &entrada.cor,
    })
    .map_err(|campos| ErroDoPerfil::Campos { campos })?;
    Ok(ClientMessage::GravarPerfil {
        revisao,
        nome_exibido: validado.nome_exibido,
        pronomes: validado.pronomes,
        status: validado.status,
        bio: validado.bio,
        cor: validado.cor,
        efeito: entrada.efeito,
    })
}

/// **A entrada validada, sem fio nenhum**: os textos aparados e a cor em
/// `#rrggbb` minúsculo, ou os erros por campo — a mesma [`validar`] que o
/// servidor aplica, e que [`montar_gravar`] usa.
///
/// É o que o **perfil padrão** (o «Seu perfil» da tela inicial, guardado nesta
/// máquina e aplicado a cada servidor 12 em que a pessoa ainda não tem perfil)
/// grava: uma entrada que passou aqui é uma que o servidor aceita como está, e
/// o `GravarPerfil` que a leva não muda nada nela. O desktop
/// (`seele_ffi::perfis`) e o navegador (`seele-web`, `perfil_validar`) chamam
/// esta, e nenhuma outra.
///
/// # Errors
///
/// [`ErroDoPerfil::Campos`], com todos os campos inválidos de uma vez.
pub fn entrada_validada(entrada: &EntradaDoPerfil) -> Result<EntradaDoPerfil, ErroDoPerfil> {
    let validado = validar(&EntradaDePerfil {
        nome_exibido: &entrada.nome_exibido,
        pronomes: &entrada.pronomes,
        status: &entrada.status,
        bio: &entrada.bio,
        cor: &entrada.cor,
    })
    .map_err(|campos| ErroDoPerfil::Campos { campos })?;
    Ok(EntradaDoPerfil {
        nome_exibido: validado.nome_exibido,
        pronomes: validado.pronomes,
        status: validado.status,
        bio: validado.bio,
        cor: seele_proto::perfil::cor_em_texto(validado.cor),
        efeito: entrada.efeito,
    })
}

/// **O portão do cliente**: um verbo só sai para um servidor que o entende.
///
/// `desde_do_cliente` diz em que número cada verbo nasceu; `negociado` é o que
/// a conexão combinou no `Hello`. A tabela é a do `seele-proto`, e esta função
/// não repete número nenhum.
///
/// # Errors
///
/// [`ErroDoPerfil::ServidorAntigo`] quando o servidor negociou menos.
pub const fn portao_do_cliente(
    mensagem: &ClientMessage,
    negociado: u8,
) -> Result<(), ErroDoPerfil> {
    portao(desde_do_cliente(mensagem), negociado)
}

/// O `TirarImagemDoPerfil` de um lado: a mensagem, para o portão e para o fio.
#[must_use]
pub fn mensagem_de_tirar(lado: seele_proto::perfil::LadoDoPerfil) -> ClientMessage {
    ClientMessage::TirarImagemDoPerfil { lado }
}

/// O SHA-256 de uma imagem de perfil: o endereço dela, o mesmo que o servidor
/// grava em [`seele_proto::perfil::ImagemDoPerfil::hash`].
#[must_use]
pub fn hash_da_imagem(bytes: &[u8]) -> [u8; 32] {
    seele_proto::attachment::hash(bytes)
}

/// O mesmo portão, para os pedidos do fluxo de imagem (`EnviarAoPerfil` e
/// `LerDoPerfil` valem 12; `Enviar` e `Ler` do volume, não).
///
/// # Errors
///
/// [`ErroDoPerfil::ServidorAntigo`] quando o servidor negociou menos.
pub const fn portao_do_pedido_de_imagem(
    pedido: &PedidoDeImagem,
    negociado: u8,
) -> Result<(), ErroDoPerfil> {
    portao(desde_o_pedido_de_imagem(pedido), negociado)
}

const fn portao(exige: u8, negociado: u8) -> Result<(), ErroDoPerfil> {
    if exige > negociado {
        Err(ErroDoPerfil::ServidorAntigo { negociado, exige })
    } else {
        Ok(())
    }
}

/// **O tamanho, antes de ler o arquivo**: quem escolhe um arquivo pelo caminho
/// pergunta isto ao disco e só então o lê, para não puxar um vídeo de
/// gigabytes para a memória.
///
/// # Errors
///
/// [`ErroDoPerfil::ImagemVazia`] e [`ErroDoPerfil::ImagemGrandeDemais`].
pub const fn conferir_tamanho(bytes: u64) -> Result<(), ErroDoPerfil> {
    if bytes == 0 {
        Err(ErroDoPerfil::ImagemVazia)
    } else if bytes > TETO_DA_IMAGEM as u64 {
        Err(ErroDoPerfil::ImagemGrandeDemais {
            limite_bytes: TETO_DA_IMAGEM as u64,
        })
    } else {
        Ok(())
    }
}

/// **A conferência de uma imagem antes de enviar**: o tamanho e o tipo, este
/// pelos **bytes** (`seele_proto::imagem`) e nunca pela extensão nem pelo que
/// o arquivo diz ser.
///
/// O servidor confere de novo — quem decide é ele —, mas conferir aqui é o que
/// permite a casca dizer o motivo antes de gastar a subida de 10 MiB.
///
/// # Errors
///
/// [`ErroDoPerfil::ImagemVazia`], [`ErroDoPerfil::ImagemGrandeDemais`],
/// [`ErroDoPerfil::Heic`] ou [`ErroDoPerfil::TipoRecusado`].
pub fn conferir_imagem(bytes: &[u8]) -> Result<TipoDaImagem, ErroDoPerfil> {
    conferir_tamanho(bytes.len() as u64)?;
    if seele_proto::imagem::reconhecido_e_nao_aceito(bytes) == Some("heic") {
        return Err(ErroDoPerfil::Heic);
    }
    seele_proto::imagem::sniff(bytes)
        .map(TipoDaImagem::from)
        .ok_or(ErroDoPerfil::TipoRecusado)
}

/// Uma imagem de perfil como a casca a lê: o hash em hexadecimal (o endereço
/// que `imagem_do_perfil` pede de volta), o tipo de mídia e o tamanho.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImagemNaTela {
    /// SHA-256 dos bytes, em hexadecimal minúsculo.
    pub hash: String,
    /// `image/png`, `image/jpeg`, `image/gif` ou `image/webp`.
    pub tipo: &'static str,
    /// O tamanho em bytes.
    pub bytes: u32,
}

/// **Um perfil como a casca o desenha**, no mesmo JSON nos dois produtores: a
/// cor já em `#rrggbb`, o efeito pelo nome, os hashes em hexadecimal. O
/// `PerfilPublico` do fio traz a cor em três bytes e o hash em 32 números — o
/// que um servidor precisa, e não o que uma página quer ler.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PerfilNaTela {
    /// De quem é.
    pub pessoa: u64,
    /// Sobe a cada mudança; é o que `gravar_perfil` confere.
    pub revisao: u32,
    /// O nome exibido; vazio quando não há.
    pub nome_exibido: String,
    /// Os pronomes.
    pub pronomes: String,
    /// O status.
    pub status: String,
    /// A bio, com quebras de linha.
    pub bio: String,
    /// `#rrggbb` minúsculo.
    pub cor: String,
    /// `Nenhum`, `Aurora`, `Brilho` ou `Pulso`.
    pub efeito: Efeito,
    /// A foto, quando há.
    pub foto: Option<ImagemNaTela>,
    /// A capa, quando há.
    pub capa: Option<ImagemNaTela>,
}

/// O perfil do fio, na forma da casca.
#[must_use]
pub fn perfil_na_tela(perfil: &seele_proto::perfil::PerfilPublico) -> PerfilNaTela {
    let imagem = |imagem: Option<seele_proto::perfil::ImagemDoPerfil>| {
        imagem.map(|imagem| ImagemNaTela {
            hash: seele_proto::mods::hex(&imagem.hash),
            tipo: tipo_de_midia(imagem.tipo),
            bytes: imagem.bytes,
        })
    };
    PerfilNaTela {
        pessoa: perfil.pessoa.0,
        revisao: perfil.revisao,
        nome_exibido: perfil.nome_exibido.clone(),
        pronomes: perfil.pronomes.clone(),
        status: perfil.status.clone(),
        bio: perfil.bio.clone(),
        cor: seele_proto::perfil::cor_em_texto(perfil.cor),
        efeito: perfil.efeito,
        foto: imagem(perfil.foto),
        capa: imagem(perfil.capa),
    }
}

/// O tipo de mídia de uma imagem de perfil.
#[must_use]
pub const fn tipo_de_midia(tipo: TipoDaImagem) -> &'static str {
    match tipo {
        TipoDaImagem::Png => "image/png",
        TipoDaImagem::Jpeg => "image/jpeg",
        TipoDaImagem::Gif => "image/gif",
        TipoDaImagem::Webp => "image/webp",
    }
}

/// O hash que a casca devolve (64 caracteres hexadecimais), ou `None` se não
/// tem essa forma. A casca nunca escolhe um hash: devolve o que leu de
/// [`ImagemNaTela::hash`].
#[must_use]
pub fn hash_de_hex(texto: &str) -> Option<[u8; 32]> {
    if texto.len() != 64 || !texto.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut hash = [0_u8; 32];
    for (i, byte) in hash.iter_mut().enumerate() {
        *byte = u8::from_str_radix(texto.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(hash)
}

/// O nome com que o servidor responde, no fluxo de imagem, a cada recusa.
#[must_use]
pub const fn nome_da_recusa(motivo: RecusaDePerfil) -> &'static str {
    match motivo {
        RecusaDePerfil::RevisaoVelha => "revisao-velha",
        RecusaDePerfil::Campos => "campos",
        RecusaDePerfil::Heic => "heic",
        RecusaDePerfil::TipoRecusado => "tipo-recusado",
        RecusaDePerfil::GrandeDemais => "grande-demais",
        RecusaDePerfil::Incompleto => "incompleto",
        RecusaDePerfil::SemPastaDePerfis => "sem-pasta-de-perfis",
    }
}

/// O nome da leitura de uma imagem cujo hash já não é o atual.
pub const NOME_DO_QUE_MUDOU: &str = "mudou";

/// **O que o nome da resposta de imagem quer dizer**, do lado de quem pediu.
#[must_use]
pub fn erro_do_servidor(nome: &str) -> ErroDoPerfil {
    match nome {
        NOME_DO_QUE_MUDOU => ErroDoPerfil::Mudou,
        "heic" => ErroDoPerfil::Heic,
        "tipo-recusado" => ErroDoPerfil::TipoRecusado,
        "grande-demais" => ErroDoPerfil::ImagemGrandeDemais {
            limite_bytes: TETO_DA_IMAGEM as u64,
        },
        "incompleto" => ErroDoPerfil::Incompleto,
        "sessao-encerrada" | "prazo-esgotado" => ErroDoPerfil::SemResposta,
        "servidor-antigo" => ErroDoPerfil::ServidorAntigo {
            negociado: 0,
            exige: 12,
        },
        outro => TODAS
            .iter()
            .find(|motivo| nome_da_recusa(**motivo) == outro)
            .map_or(ErroDoPerfil::RecusaDesconhecida, |motivo| {
                ErroDoPerfil::Recusada { motivo: *motivo }
            }),
    }
}

/// Os sete motivos, para a tabela de nomes e para o teste dela.
const TODAS: [RecusaDePerfil; 7] = [
    RecusaDePerfil::RevisaoVelha,
    RecusaDePerfil::Campos,
    RecusaDePerfil::Heic,
    RecusaDePerfil::TipoRecusado,
    RecusaDePerfil::GrandeDemais,
    RecusaDePerfil::Incompleto,
    RecusaDePerfil::SemPastaDePerfis,
];

/// O nome com que a camada de transporte (`Transfers::imagem_de_mod`) recusa,
/// **antes** de abrir o fluxo, um pedido que o servidor negociado não conhece.
pub const NOME_DO_SERVIDOR_ANTIGO: &str = "servidor-antigo";

#[cfg(test)]
mod testes {
    use super::*;
    use seele_proto::ids::PersonId;
    use seele_proto::perfil::{CampoDoPerfil, LadoDoPerfil, MotivoDoCampo};

    fn entrada() -> EntradaDoPerfil {
        EntradaDoPerfil {
            nome_exibido: "  Maya ".to_owned(),
            pronomes: "ela/dela".to_owned(),
            status: "ao vivo".to_owned(),
            bio: "um\ndois".to_owned(),
            cor: "#F2521F".to_owned(),
            efeito: Efeito::Aurora,
        }
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    #[test]
    fn gravar_apara_os_textos_e_leva_a_cor_em_tres_bytes() {
        let Ok(ClientMessage::GravarPerfil {
            revisao,
            nome_exibido,
            cor,
            efeito,
            bio,
            ..
        }) = montar_gravar(7, &entrada())
        else {
            panic!("a entrada era válida");
        };
        assert_eq!(revisao, 7);
        assert_eq!(nome_exibido, "Maya");
        assert_eq!(cor, [0xf2, 0x52, 0x1f]);
        assert_eq!(efeito, Efeito::Aurora);
        assert_eq!(bio, "um\ndois", "a quebra de linha é da bio");
    }

    /// **A entrada validada é a que o `GravarPerfil` levaria**: aparada, a cor
    /// em minúsculas, e montar o pedido com ela não muda nada. Os erros são os
    /// mesmos, por campo.
    #[test]
    fn a_entrada_validada_e_a_que_o_servidor_gravaria() {
        let validada = entrada_validada(&entrada()).unwrap();
        assert_eq!(validada.nome_exibido, "Maya");
        assert_eq!(validada.cor, "#f2521f");
        assert_eq!(validada.bio, "um\ndois");
        assert_eq!(
            montar_gravar(3, &validada).unwrap(),
            montar_gravar(3, &entrada()).unwrap(),
            "a entrada validada mudou o pedido"
        );
        let mut ruim = entrada();
        ruim.pronomes = "p".repeat(31);
        assert!(matches!(
            entrada_validada(&ruim),
            Err(ErroDoPerfil::Campos { campos }) if campos.len() == 1
        ));
    }

    /// **Os erros voltam por campo, todos de uma vez**, e nada é montado: o
    /// editor marca cada campo no lugar dele sem ir ao servidor.
    #[test]
    fn gravar_devolve_um_erro_por_campo_e_nao_monta_nada() {
        let mut ruim = entrada();
        ruim.nome_exibido = "x".repeat(41);
        ruim.status = "a\nb".to_owned();
        ruim.cor = "laranja".to_owned();
        let Err(ErroDoPerfil::Campos { campos }) = montar_gravar(1, &ruim) else {
            panic!("tinha de recusar");
        };
        assert_eq!(
            campos,
            vec![
                ErroDeCampo {
                    campo: CampoDoPerfil::NomeExibido,
                    motivo: MotivoDoCampo::Longo
                },
                ErroDeCampo {
                    campo: CampoDoPerfil::Status,
                    motivo: MotivoDoCampo::Controle
                },
                ErroDeCampo {
                    campo: CampoDoPerfil::Cor,
                    motivo: MotivoDoCampo::Formato
                },
            ]
        );
    }

    /// **Nenhum verbo ou pedido novo sai para um servidor 11.** Reverter o `>`
    /// do portão (ou o `=> 12` da tabela do protocolo) reprova aqui.
    #[test]
    fn um_servidor_onze_nao_recebe_os_verbos_do_doze() {
        let gravar = montar_gravar(0, &entrada()).unwrap();
        let tirar = ClientMessage::TirarImagemDoPerfil {
            lado: LadoDoPerfil::Foto,
        };
        let enviar = PedidoDeImagem::EnviarAoPerfil {
            lado: LadoDoPerfil::Capa,
            bytes: 1,
        };
        let ler = PedidoDeImagem::LerDoPerfil {
            pessoa: PersonId(1),
            lado: LadoDoPerfil::Foto,
            hash: [0; 32],
        };
        for mensagem in [&gravar, &tirar] {
            assert_eq!(
                portao_do_cliente(mensagem, 11),
                Err(ErroDoPerfil::ServidorAntigo {
                    negociado: 11,
                    exige: 12
                }),
                "{mensagem:?}"
            );
            assert_eq!(portao_do_cliente(mensagem, 12), Ok(()));
        }
        for pedido in [&enviar, &ler] {
            assert!(
                matches!(
                    portao_do_pedido_de_imagem(pedido, 11),
                    Err(ErroDoPerfil::ServidorAntigo { .. })
                ),
                "{pedido:?}"
            );
            assert_eq!(portao_do_pedido_de_imagem(pedido, 12), Ok(()));
        }
        // O que já existia não é barrado: o portão não pode derrubar o 11.
        assert_eq!(portao_do_cliente(&ClientMessage::PedirCargos, 11), Ok(()));
        assert_eq!(
            portao_do_pedido_de_imagem(
                &PedidoDeImagem::Ler {
                    mod_id: "a/b".into(),
                    channel: seele_proto::ids::ChannelId(0),
                    payload: String::new()
                },
                10
            ),
            Ok(())
        );
    }

    #[test]
    fn a_imagem_e_conferida_pelos_bytes_e_pelo_teto() {
        assert_eq!(conferir_imagem(PNG), Ok(TipoDaImagem::Png));
        assert_eq!(conferir_imagem(b""), Err(ErroDoPerfil::ImagemVazia));
        assert_eq!(
            conferir_imagem(b"%PDF-1.7 ......"),
            Err(ErroDoPerfil::TipoRecusado)
        );
        // Um PNG de nome e um JPEG de bytes: vale o byte.
        assert_eq!(
            conferir_imagem(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Ok(TipoDaImagem::Jpeg)
        );
        // HEIC: nomeado, e não «desconhecido».
        let heic = include_bytes!("../../seele-proto/tests/vetores/heic-8x8.heic");
        assert_eq!(conferir_imagem(heic), Err(ErroDoPerfil::Heic));
        // O teto: 10 MiB passa; 10 MiB + 1 é recusado antes de qualquer envio.
        let mut grande = PNG.to_vec();
        grande.resize(TETO_DA_IMAGEM, 0);
        assert_eq!(conferir_imagem(&grande), Ok(TipoDaImagem::Png));
        grande.push(0);
        assert_eq!(
            conferir_imagem(&grande),
            Err(ErroDoPerfil::ImagemGrandeDemais {
                limite_bytes: TETO_DA_IMAGEM as u64
            })
        );
        assert_eq!(
            conferir_tamanho(TETO_DA_IMAGEM as u64 + 1),
            Err(ErroDoPerfil::ImagemGrandeDemais {
                limite_bytes: TETO_DA_IMAGEM as u64
            })
        );
        assert_eq!(conferir_tamanho(0), Err(ErroDoPerfil::ImagemVazia));
    }

    #[test]
    fn os_nomes_das_recusas_fazem_a_volta() {
        for motivo in TODAS {
            let volta = erro_do_servidor(nome_da_recusa(motivo));
            let esperado = match motivo {
                RecusaDePerfil::Heic => ErroDoPerfil::Heic,
                RecusaDePerfil::TipoRecusado => ErroDoPerfil::TipoRecusado,
                RecusaDePerfil::GrandeDemais => ErroDoPerfil::ImagemGrandeDemais {
                    limite_bytes: TETO_DA_IMAGEM as u64,
                },
                RecusaDePerfil::Incompleto => ErroDoPerfil::Incompleto,
                outro => ErroDoPerfil::Recusada { motivo: outro },
            };
            assert_eq!(volta, esperado, "{motivo:?}");
        }
        assert_eq!(erro_do_servidor("mudou"), ErroDoPerfil::Mudou);
        assert_eq!(
            erro_do_servidor("um-nome-do-futuro"),
            ErroDoPerfil::RecusaDesconhecida
        );
    }

    /// `TODAS` tem de cobrir o enum: um motivo novo sem nome reprova aqui, em
    /// vez de virar `RecusaDesconhecida` em silêncio.
    #[test]
    fn a_tabela_de_nomes_cobre_todos_os_motivos() {
        let cobre = |m: RecusaDePerfil| match m {
            RecusaDePerfil::RevisaoVelha
            | RecusaDePerfil::Campos
            | RecusaDePerfil::Heic
            | RecusaDePerfil::TipoRecusado
            | RecusaDePerfil::GrandeDemais
            | RecusaDePerfil::Incompleto
            | RecusaDePerfil::SemPastaDePerfis => TODAS.contains(&m),
        };
        for m in TODAS {
            assert!(cobre(m));
        }
        let nomes: std::collections::BTreeSet<_> =
            TODAS.iter().map(|m| nome_da_recusa(*m)).collect();
        assert_eq!(nomes.len(), TODAS.len(), "dois motivos com o mesmo nome");
    }

    #[test]
    fn o_perfil_chega_a_casca_com_a_cor_e_o_hash_legiveis() {
        let perfil = seele_proto::perfil::PerfilPublico {
            pessoa: PersonId(5),
            revisao: 3,
            nome_exibido: "Maya".into(),
            pronomes: String::new(),
            status: String::new(),
            bio: "a\nb".into(),
            cor: [0xf2, 0x52, 0x1f],
            efeito: Efeito::Brilho,
            foto: Some(seele_proto::perfil::ImagemDoPerfil {
                hash: [0xab; 32],
                tipo: TipoDaImagem::Webp,
                bytes: 9,
            }),
            capa: None,
        };
        let tela = perfil_na_tela(&perfil);
        assert_eq!(tela.cor, "#f2521f");
        let foto = tela.foto.as_ref().unwrap();
        assert_eq!(foto.hash, "ab".repeat(32));
        assert_eq!(foto.tipo, "image/webp");
        assert_eq!(hash_de_hex(&foto.hash), Some([0xab; 32]));
        let json = serde_json::to_value(&tela).unwrap();
        assert_eq!(json["efeito"], "Brilho");
        assert_eq!(json["pessoa"], 5);
        assert!(json["capa"].is_null());
        // Quem não é hash não vira hash.
        for torto in ["", "ab", &"zz".repeat(32), &"ab".repeat(33)] {
            assert_eq!(hash_de_hex(torto), None, "{torto}");
        }
    }

    #[test]
    fn o_erro_serializa_pelo_nome_da_variante() {
        assert_eq!(
            serde_json::to_string(&ErroDoPerfil::Heic).unwrap(),
            "\"Heic\""
        );
        assert_eq!(
            serde_json::to_string(&ErroDoPerfil::ServidorAntigo {
                negociado: 11,
                exige: 12
            })
            .unwrap(),
            r#"{"ServidorAntigo":{"negociado":11,"exige":12}}"#
        );
    }
}
