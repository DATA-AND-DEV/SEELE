use super::{corpo_de, fonte_sem_comentarios};

#[test]
fn a_copia_vem_antes_de_toda_abertura_do_banco_e_as_escolhas_antes_da_subida() {
    let fonte = fonte_sem_comentarios();
    let corpo = corpo_de(&fonte, concat!("async fn hospedar_em", "("));
    let onde = |trecho: &str| {
        corpo
            .find(trecho)
            .unwrap_or_else(|| panic!("o `hospedar` não chama `{trecho}`"))
    };
    let ordem = [
        onde("mods::recusas_de_hospedar("),
        onde("antes_de_hospedar::garantir_a_copia("),
        onde("servidores::plantar_a_chave_depois_do_portao("),
        onde("escolhas_de_mods::aplicar_depois_do_portao("),
        onde("subir_o_guardado("),
    ];
    assert!(
        ordem
            .windows(2)
            .all(|par| matches!(par, [antes, depois] if antes < depois)),
        "a ordem do CTA no `hospedar` mudou: {ordem:?}"
    );
    assert!(
        !corpo.contains("mods::mods_de_api_antiga("),
        "o `hospedar` voltou a recusar sem olhar as escolhas estagiadas"
    );
    // O comando é só a casca do `hospedar_em`, com a porta e o modo do
    // produto: é o mesmo corpo que os testes de dono sobem.
    let comando = corpo_de(&fonte, concat!("async fn hospedar", "(\n"));
    assert!(
        comando.contains("hospedar_em(")
            && comando.contains("PORTA_PADRAO,")
            && comando.contains("modo_local(),"),
        "o comando `hospedar` não sobe pelo `hospedar_em` com a porta e o modo do produto"
    );
}

#[test]
fn nenhum_caminho_de_mod_com_o_servidor_parado_abre_o_banco_para_escrever() {
    let fonte = fonte_sem_comentarios();
    // Os cabeçalhos partidos: inteiros, este próprio teste seria a primeira
    // ocorrência que o `corpo_de` acha.
    for cabecalho in [
        concat!("async fn resolver_mod_de_api_antiga", "("),
        concat!("async fn apagar_pacote_do_cache", "("),
        concat!("fn antes_de_hospedar", "("),
    ] {
        let corpo = corpo_de(&fonte, cabecalho);
        assert!(
            !corpo.contains("Persistence::open"),
            "`{cabecalho}` abre o banco com `Persistence::open`, que migra"
        );
    }
    let mods = include_str!("../mods.rs");
    let resolver = corpo_de(mods, "pub(crate) fn resolver_mod_de_api_antiga(");
    assert!(
        !resolver.contains("Persistence::open") && !resolver.contains("banco_de_hospedar"),
        "o resolvedor das saídas voltou a abrir o banco (ou a pedi-lo com a chave plantada)"
    );
}
