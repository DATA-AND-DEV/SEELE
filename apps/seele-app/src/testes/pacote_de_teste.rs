/// O menor WAV que o `sniff` aceita (`seele-proto/src/midia_de_mod.rs:73`).
pub(crate) const WAV: &[u8] = b"RIFF\0\0\0\0WAVEfmt ";
/// Um PNG de verdade, de 1 × 1. **Inteiro, e não só a assinatura**: a
/// imagem de um MOD vai à janela depois de as medidas dela serem lidas do
/// cabeçalho (o teto de pixels), e a assinatura sozinha não tem medidas.
pub(crate) const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0dIDAT\x78\xda\x63\xd0\x08\xa8\xf8\x0f\x00\x03\x84\x01\xf0\xb6\x34\x04\x4b\x00\x00\x00\x00IEND\xae\x42\x60\x82";
/// Texto, que nenhum decodificador deste produto aceita.
pub(crate) const TEXTO: &[u8] = b"so texto";
/// O id do MOD de todo pacote daqui.
pub(crate) const ID: &str = "prova/midia";

/// Publica um pacote que declara `arquivos`, e devolve a pasta e o hash.
///
/// A pasta é um `TempDir`: some quando o teste termina.
pub(crate) fn publicar(arquivos: &[(&str, &[u8])]) -> (tempfile::TempDir, String) {
    let raiz = tempfile::tempdir().expect("a pasta de teste");
    let obras = raiz.path().join("em-obras");
    std::fs::create_dir_all(obras.join("cliente")).expect("a pasta de obras");
    let declarados: Vec<String> = arquivos
        .iter()
        .map(|(caminho, _)| format!("\"{caminho}\""))
        .collect();
    let api = seele_ffi::mods::MOD_API_VERSION;
    std::fs::write(
            obras.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"{ID}","version":"1.0.0","api":{api},"repo":"https://example.invalid/x","reach":["dom"],"client":"cliente/main.js","arquivos":[{}]}}"#,
                declarados.join(",")
            ),
        )
        .expect("o manifesto");
    std::fs::write(obras.join("cliente/main.js"), "/* nada */").expect("o cliente");
    for (caminho, bytes) in arquivos {
        let destino = obras.join(caminho);
        std::fs::create_dir_all(destino.parent().expect("a pasta do arquivo"))
            .expect("a pasta do arquivo");
        std::fs::write(&destino, bytes).expect("o arquivo");
    }
    let hash = seele_ffi::mods::ler_pasta(&obras.to_string_lossy())
        .expect("o pacote de teste é válido")
        .hash;
    let publicado = raiz.path().join(seele_ffi::mods::PACOTES).join(&hash);
    std::fs::create_dir_all(publicado.parent().expect("a pasta dos pacotes"))
        .expect("a pasta dos pacotes");
    std::fs::rename(&obras, &publicado).expect("publicar o pacote");
    (raiz, hash)
}

/// A pasta de configuração, como `config_dir` a devolveria.
pub(crate) fn pasta(raiz: &tempfile::TempDir) -> String {
    raiz.path().to_string_lossy().into_owned()
}
