use super::base64_de;

#[test]
fn os_vetores_do_rfc_4648() {
    // Escrever base64 à mão é barato; escrevê-lo **errado** é caro de um
    // jeito específico: o `atob` da janela não reclama de quase nada, então
    // um preenchimento errado no último grupo vira um quadro truncado que o
    // decodificador recusa em silêncio — e o sintoma é uma tela preta, que
    // é o mesmo sintoma de outras seis coisas.
    //
    // Os vetores são os do §10 do RFC 4648, que existem exatamente para
    // isto e cobrem os três restos possíveis.
    assert_eq!(base64_de(b""), "");
    assert_eq!(base64_de(b"f"), "Zg==");
    assert_eq!(base64_de(b"fo"), "Zm8=");
    assert_eq!(base64_de(b"foo"), "Zm9v");
    assert_eq!(base64_de(b"foob"), "Zm9vYg==");
    assert_eq!(base64_de(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64_de(b"foobar"), "Zm9vYmFy");
}

#[test]
fn os_bytes_altos_tambem() {
    // A tabela tem 64 entradas e o índice tem seis bits, então nenhum byte
    // pode sair dela — mas um `>>` com o deslocamento errado sai, e sai
    // justamente nos bytes altos, que texto ASCII nunca exercita. Um quadro
    // de vídeo é quase só byte alto.
    assert_eq!(base64_de(&[0xFF, 0xFF, 0xFF]), "////");
    assert_eq!(base64_de(&[0x00, 0x00, 0x00]), "AAAA");
    assert_eq!(base64_de(&[0xFB, 0xFF, 0xBE]), "+/++");
    // E o Annex-B começa sempre assim: `00 00 00 01`.
    assert_eq!(base64_de(&[0x00, 0x00, 0x00, 0x01]), "AAAAAQ==");
}
