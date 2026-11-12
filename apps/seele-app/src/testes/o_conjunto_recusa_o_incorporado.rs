use super::*;

fn escolhido(id: &str) -> ModEscolhido {
    serde_json::from_value(serde_json::json!({ "id": id, "hash": "0".repeat(64) }))
        .expect("um ModEscolhido")
}

/// Aplicar um conjunto com o PERFIS devolve `ModIncorporado`, antes de
/// qualquer leitura de disco. Tirar a chamada de `conferir_incorporados` do
/// comando (ou o corpo dela) reprova aqui.
#[test]
fn o_perfis_nao_se_liga_mais() {
    let Err(FalhaNoMod::ModIncorporado { id, desde }) =
        conferir_incorporados(&[escolhido("ana/outro"), escolhido("seele/perfis")])
    else {
        panic!("o PERFIS passou");
    };
    assert_eq!((id.as_str(), desde.as_str()), ("seele/perfis", "0.19"));
    assert!(conferir_incorporados(&[escolhido("ana/outro")]).is_ok());
    let fonte = fonte_sem_comentarios();
    let corpo = corpo_de(&fonte, concat!("async fn aplicar_conjunto_de_mods", "("));
    let chama = corpo.find(concat!("conferir_incorporados", "(&ligados)"));
    let le = corpo.find(concat!("ler_por_", "hash("));
    assert!(
        chama.is_some() && chama < le,
        "o comando não confere os incorporados antes de ler os pacotes"
    );
}
