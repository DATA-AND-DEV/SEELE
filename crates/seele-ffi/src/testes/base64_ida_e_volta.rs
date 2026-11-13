use super::{base64_de, de_base64};

#[test]
fn a_volta_desfaz_a_ida_em_todo_tamanho() {
    // Os três restos, porque é neles que base64 erra: múltiplo de três,
    // sobra de um byte (dois `=`) e sobra de dois (um `=`). Um decodificador
    // que ignorasse o resto devolveria bytes a mais no último grupo.
    for tamanho in 0..=64usize {
        let bytes: Vec<u8> = (0..tamanho).map(|i| (i * 7 % 256) as u8).collect();
        let texto = base64_de(&bytes);
        assert_eq!(
            de_base64(&texto).as_deref(),
            Some(bytes.as_slice()),
            "não fechou em {tamanho} bytes: {texto}"
        );
    }
}

#[test]
fn o_que_o_navegador_escreve_e_lido() {
    // Um vetor de fora, para o par não provar só que concorda consigo
    // mesmo: `btoa("Olá, SEELE")` numa janela dá exatamente isto.
    assert_eq!(
        de_base64("T2zDoSwgU0VFTEU=").as_deref(),
        Some("Olá, SEELE".as_bytes())
    );
}

#[test]
fn espaco_em_branco_nao_atrapalha() {
    // Base64 de e-mail vem quebrado em linhas, e recusar por isso seria
    // recusar o caso comum de quem cola de fora.
    assert_eq!(
        de_base64("T2zDoSwg\n  U0VFTEU =").as_deref(),
        Some("Olá, SEELE".as_bytes())
    );
}

#[test]
fn texto_que_nao_e_base64_nao_vira_bytes_truncados() {
    // O caso que importa: devolver o que deu para ler produziria um arquivo
    // corrompido com cara de arquivo bom, e o defeito só apareceria na
    // máquina de quem recebeu.
    assert_eq!(de_base64("não é base64"), None);
    // Um caractere sozinho são seis bits e não fecha byte nenhum.
    assert_eq!(de_base64("QUJD RA=="), Some(b"ABCD".to_vec()));
    assert_eq!(de_base64("QUJDR"), None);
}
