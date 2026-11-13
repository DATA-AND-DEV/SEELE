use super::impressao_a_conferir;

#[test]
fn sem_link_nesta_sessao_confere_pela_guardada() {
    assert_eq!(
        impressao_a_conferir(None, Some("aaaa1111")),
        Some("aaaa1111".to_owned()),
        "quem volta pela lista entra num endereço novo sem conferir nada — o \
             PRIMEIRO CONTATO cego do S3 — e sem `LEVE`"
    );
}

#[test]
fn o_link_desta_sessao_vence_a_guardada() {
    assert_eq!(
        impressao_a_conferir(Some("bbbb2222"), Some("aaaa1111")),
        Some("bbbb2222".to_owned()),
        "um link colado agora é a intenção mais nova de quem conecta; conferir \
             pela guardada recusaria o servidor que a pessoa acabou de pedir"
    );
}

#[test]
fn so_o_link_e_o_caminho_de_sempre() {
    assert_eq!(
        impressao_a_conferir(Some("bbbb2222"), None),
        Some("bbbb2222".to_owned()),
        "sem nada guardado, o link desta sessão deixou de ser conferido: quem cola um \
             link num endereço novo entra às cegas"
    );
}

#[test]
fn sem_nenhuma_das_duas_nao_ha_o_que_conferir() {
    assert_eq!(
        impressao_a_conferir(None, None),
        None,
        "sem link e sem guardada apareceu uma impressão do nada: a conexão \
             recusaria o servidor por uma promessa que ninguém fez"
    );
}
