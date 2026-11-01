//! **As imagens do perfil público pelo fluxo bidirecional** (SEELE 0.19,
//! protocolo 12, `docs/0.19/perfis-oficiais.md` §5.1): o pedido de enviar a
//! foto ou a capa, o de ler a de outra pessoa, a conferência antes de subir e
//! o ícone nativo que os clientes 0.18 enxergam.
//!
//! O fluxo é o de [`crate::volume`] — a página abre um fluxo bidirecional por
//! pedido, escreve o pedido enquadrado (e, num envio, os bytes logo atrás) e
//! lê a [`crate::volume::Resposta`] —, com dois pedidos a mais. **O navegador
//! também envia imagem**: quem escolhe o arquivo é a página, e o que vale a
//! regra (tamanho, tipo pelos bytes, HEIC nomeado) é daqui, a mesma do
//! desktop (`seele_sessao::perfil`).
//!
//! # O portão de versão
//!
//! Cada pedido recebe o número que a **conexão negociou** (`Ligacao::carimbo`):
//! num servidor 11 o pedido não é escrito, e volta
//! [`ErroDoPerfil::ServidorAntigo`], tipado. O carimbo do fluxo de imagem é o
//! desta build (ver [`crate::volume`]); o que decide se o servidor o entende
//! é o número negociado.
//!
//! # O cache
//!
//! O hash é o endereço e a imagem vale para sempre: a página guarda o que
//! baixou por hash (um `Map`), e [`conferir_resposta`] é o que impede um
//! servidor de colocar sob um hash bytes que não são dele.

use seele_proto::control::encode_em;
use seele_proto::ids::PersonId;
use seele_proto::perfil::{LadoDoPerfil, TipoDaImagem};
use seele_proto::quadro::enquadrar;
use seele_proto::version::PROTOCOL_VERSION;
use seele_proto::volume::PedidoDeImagem;
use seele_sessao::perfil::{
    conferir_imagem, erro_do_servidor, hash_da_imagem, portao_do_pedido_de_imagem, ErroDoPerfil,
};

fn enquadrado(pedido: &PedidoDeImagem, negociado: u8) -> Result<Vec<u8>, ErroDoPerfil> {
    portao_do_pedido_de_imagem(pedido, negociado)?;
    let quadro = encode_em(PROTOCOL_VERSION, pedido).map_err(|_| ErroDoPerfil::TipoRecusado)?;
    enquadrar(&quadro).map_err(|_| ErroDoPerfil::TipoRecusado)
}

/// **O pedido de enviar** `bytes` como a foto ou a capa de quem pede. Os bytes
/// seguem o pedido no mesmo fluxo.
///
/// Confere antes de montar: tamanho, tipo pelos bytes, HEIC nomeado. A página
/// passa os bytes inteiros porque precisa deles para a conferência; o que ela
/// escreve no fluxo é o pedido devolvido e depois os mesmos bytes.
///
/// # Errors
///
/// Os de [`conferir_imagem`] e [`ErroDoPerfil::ServidorAntigo`].
pub fn pedido_de_enviar(
    lado: LadoDoPerfil,
    bytes: &[u8],
    negociado: u8,
) -> Result<Vec<u8>, ErroDoPerfil> {
    conferir_imagem(bytes)?;
    enquadrado(
        &PedidoDeImagem::EnviarAoPerfil {
            lado,
            bytes: bytes.len() as u64,
        },
        negociado,
    )
}

/// **O pedido de ler** a imagem `lado` de `pessoa`, com o hash que o perfil
/// dela traz.
///
/// # Errors
///
/// [`ErroDoPerfil::ServidorAntigo`].
pub fn pedido_de_ler(
    pessoa: u64,
    lado: LadoDoPerfil,
    hash: [u8; 32],
    negociado: u8,
) -> Result<Vec<u8>, ErroDoPerfil> {
    enquadrado(
        &PedidoDeImagem::LerDoPerfil {
            pessoa: PersonId(pessoa),
            lado,
            hash,
        },
        negociado,
    )
}

/// **O que a resposta acabada quer dizer**: o nome da recusa do servidor vira
/// o erro tipado, e os bytes de uma leitura são conferidos contra o hash
/// pedido (`hash` vazio, num envio, não confere nada).
///
/// # Errors
///
/// O erro tipado do nome que o servidor mandou; [`ErroDoPerfil::Incompleto`]
/// quando os bytes não têm o hash pedido.
pub fn conferir_resposta(
    acabada: Result<Vec<u8>, String>,
    hash: Option<[u8; 32]>,
) -> Result<Vec<u8>, ErroDoPerfil> {
    let bytes = acabada.map_err(|nome| erro_do_servidor(&nome))?;
    match hash {
        Some(esperado) if hash_da_imagem(&bytes) != esperado => Err(ErroDoPerfil::Incompleto),
        _ => Ok(bytes),
    }
}

/// O tipo que os bytes dizem ser, para a página saber o que foi aceito.
///
/// # Errors
///
/// Os de [`conferir_imagem`].
pub fn tipo_dos_bytes(bytes: &[u8]) -> Result<TipoDaImagem, ErroDoPerfil> {
    conferir_imagem(bytes)
}

/// **O ícone nativo** que acompanha uma foto nova (§9 Q1): PNG de até 256 px e
/// 8 KiB, do primeiro quadro, para os clientes 0.18 verem o mesmo rosto pelo
/// `SetPersonIcon`. A mesma escada do desktop (`reducao::icone_do_perfil`).
/// `None` quando os bytes não são uma imagem que se decodifica.
#[must_use]
pub fn icone_nativo(bytes: &[u8]) -> Option<Vec<u8>> {
    seele_sessao::reducao::icone_do_perfil(bytes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use seele_proto::control::decode;
    use seele_proto::perfil::TETO_DA_IMAGEM;
    use seele_proto::quadro::{comprimento, PREFIXO_LEN};

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    fn pedido_lido(quadro: &[u8]) -> PedidoDeImagem {
        let tamanho = comprimento(quadro[..PREFIXO_LEN].try_into().unwrap()).unwrap();
        assert_eq!(quadro.len(), PREFIXO_LEN + tamanho);
        decode(&quadro[PREFIXO_LEN..]).unwrap()
    }

    /// O pedido sai enquadrado, com o lado e o tamanho dos bytes; e **nenhum
    /// sai para um servidor 11**. Voltar o portão de `enquadrado` reprova.
    #[test]
    fn os_dois_pedidos_saem_no_doze_e_nao_no_onze() {
        let enviar = pedido_de_enviar(LadoDoPerfil::Capa, PNG, 12).unwrap();
        assert!(
            matches!(
                pedido_lido(&enviar),
                PedidoDeImagem::EnviarAoPerfil { lado: LadoDoPerfil::Capa, bytes }
                    if bytes == PNG.len() as u64
            ),
            "{:?}",
            pedido_lido(&enviar)
        );
        let ler = pedido_de_ler(4, LadoDoPerfil::Foto, [7; 32], 12).unwrap();
        assert!(
            matches!(
                pedido_lido(&ler),
                PedidoDeImagem::LerDoPerfil { pessoa: PersonId(4), lado: LadoDoPerfil::Foto, hash }
                    if hash == [7; 32]
            ),
            "{:?}",
            pedido_lido(&ler)
        );
        let antigo = ErroDoPerfil::ServidorAntigo {
            negociado: 11,
            exige: 12,
        };
        assert_eq!(
            pedido_de_enviar(LadoDoPerfil::Capa, PNG, 11),
            Err(antigo.clone())
        );
        assert_eq!(
            pedido_de_ler(4, LadoDoPerfil::Foto, [7; 32], 11),
            Err(antigo)
        );
    }

    /// O navegador também recusa antes de subir: tamanho, tipo pelos bytes e
    /// HEIC. Nenhum pedido é montado.
    #[test]
    fn o_envio_do_navegador_confere_tamanho_e_tipo() {
        let mut grande = PNG.to_vec();
        grande.resize(TETO_DA_IMAGEM + 1, 0);
        assert!(matches!(
            pedido_de_enviar(LadoDoPerfil::Foto, &grande, 12),
            Err(ErroDoPerfil::ImagemGrandeDemais { .. })
        ));
        assert_eq!(
            pedido_de_enviar(LadoDoPerfil::Foto, b"%PDF-1.7 ....", 12),
            Err(ErroDoPerfil::TipoRecusado)
        );
        assert_eq!(
            pedido_de_enviar(LadoDoPerfil::Foto, &[], 12),
            Err(ErroDoPerfil::ImagemVazia)
        );
        let heic = include_bytes!("../../seele-proto/tests/vetores/heic-8x8.heic");
        assert_eq!(
            pedido_de_enviar(LadoDoPerfil::Foto, heic, 12),
            Err(ErroDoPerfil::Heic)
        );
    }

    #[test]
    fn a_resposta_e_conferida_contra_o_hash_pedido() {
        let hash = hash_da_imagem(PNG);
        assert_eq!(
            conferir_resposta(Ok(PNG.to_vec()), Some(hash)),
            Ok(PNG.to_vec())
        );
        assert_eq!(
            conferir_resposta(Ok(b"outra".to_vec()), Some(hash)),
            Err(ErroDoPerfil::Incompleto)
        );
        // Um envio confirmado (sem corpo) não tem hash a conferir.
        assert_eq!(conferir_resposta(Ok(Vec::new()), None), Ok(Vec::new()));
        assert_eq!(
            conferir_resposta(Err("mudou".into()), Some(hash)),
            Err(ErroDoPerfil::Mudou)
        );
        assert_eq!(
            conferir_resposta(Err("heic".into()), None),
            Err(ErroDoPerfil::Heic)
        );
    }

    /// Um PNG de verdade, 4 × 4, de uma cor: o que o decodificador do `.wasm`
    /// lê. A escada inteira (o pior caso, o primeiro quadro de um GIF) é
    /// provada em `seele_sessao::reducao`; aqui se prova que a fronteira do
    /// navegador chega nela e que o que sai o servidor aceita.
    const PNG_4X4: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x08, 0x06, 0x00, 0x00, 0x00, 0xa9,
        0xf1, 0x9e, 0x7e, 0x00, 0x00, 0x00, 0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x38,
        0x61, 0x24, 0xf2, 0x1f, 0x19, 0x33, 0x90, 0x2e, 0x00, 0x00, 0x57, 0xe8, 0x20, 0xd1, 0x26,
        0x21, 0x53, 0xc2, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn o_icone_nativo_cabe_no_que_o_0_18_aceita() {
        let icone = icone_nativo(PNG_4X4).unwrap();
        assert_eq!(
            seele_proto::control::check_server_icon(Some(&icone)),
            Ok(())
        );
        assert!(icone_nativo(b"isto nao e imagem").is_none());
    }
}
