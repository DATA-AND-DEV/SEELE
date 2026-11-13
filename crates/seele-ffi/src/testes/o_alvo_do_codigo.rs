use super::{alvo_do_codigo, codigo_do_alvo, Codigo};

#[test]
fn o_alvo_e_arroba_codigo_e_volta_ao_mesmo_codigo() {
    let codigo = Codigo::da_chave(&[0; 32]);
    let alvo = alvo_do_codigo(&codigo);
    assert_eq!(alvo, format!("@{codigo}"));
    assert_eq!(codigo_do_alvo(&alvo), Some(codigo));
    assert_eq!(codigo_do_alvo(&format!("  {alvo}  ")), Some(codigo));
}

#[test]
fn um_endereco_ou_um_codigo_que_nao_fecha_nao_e_alvo_de_codigo() {
    for texto in [
        "casa.exemplo:8383",
        "10.0.0.1:8383",
        "[::1]:8383",
        "CSM7-NBFR-CAYQ-EV4Y",
        "@CSM7-NBFR-CAYQ-EV4Z",
        "@",
    ] {
        assert_eq!(
            codigo_do_alvo(texto),
            None,
            "`{texto}` virou alvo de código"
        );
    }
}
