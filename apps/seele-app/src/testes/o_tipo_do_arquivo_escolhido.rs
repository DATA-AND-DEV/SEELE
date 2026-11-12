use super::{tipo_do_escolhido, FalhaNoMod};

const HEIC: &[u8] = include_bytes!("../../../../crates/seele-proto/tests/vetores/heic-8x8.heic");

fn motivo(falha: FalhaNoMod) -> String {
    match falha {
        FalhaNoMod::Recusado { motivo } => motivo,
        outra => panic!("esperava uma recusa com nome, e veio {outra:?}"),
    }
}

/// **O HEIC das Fotos é recusado pelo nome** (rascunho da API 6, §7.1; o
/// bloqueio G do PERFIS): `tipo-recusado:heic`, que o MOD da 6 recebe como
/// `tipo-recusado` com `detectado: 'heic'`. Antes ele caía em
/// `formato-desconhecido`, que manda procurar defeito num arquivo que não
/// tem defeito nenhum.
#[test]
fn um_heic_e_recusado_com_o_nome_dele() {
    let falha = tipo_do_escolhido(HEIC).expect_err("o HEIC passou como imagem");
    assert_eq!(motivo(falha), "tipo-recusado:heic");
}

#[test]
fn o_que_o_produto_nao_reconhece_continua_formato_desconhecido() {
    let falha = tipo_do_escolhido(b"isto nao e imagem").expect_err("texto passou como imagem");
    assert_eq!(motivo(falha), "formato-desconhecido");
}

#[test]
fn uma_imagem_aceita_passa() {
    let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
    let lido = tipo_do_escolhido(&png).expect("o PNG foi recusado");
    assert_eq!(lido.media_type, "image/png");
}
