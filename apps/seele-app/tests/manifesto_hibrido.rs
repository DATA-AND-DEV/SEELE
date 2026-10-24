//! O `latest.json` que o `empacotar/manifesto.py` escreve, lido pelos dois
//! leitores que importam: o atualizador da 0.15.1 (o `RemoteRelease` do
//! próprio plugin) e o launcher (`seele_lancador::Manifesto`).
//!
//! Spec 0.16, §1, «O manifesto». Um teste que lesse o arquivo com um analisador
//! nosso provaria só que concordamos conosco. O `RemoteRelease` é o código que
//! roda na máquina de quem ainda está na 0.15.1.
//!
//! **Fora do Windows, de propósito.** Todo teste daqui roda o
//! `empacotar/manifesto.py`, e quem o roda é o `publicar.sh`, no Mac de quem
//! publica: o PC Windows da bateria não tem Python (o `python3` de lá é o atalho
//! da Microsoft Store), e reprovava os onze sem provar nada do produto. Nenhum
//! deles mede algo que o Windows faça: o arquivo é escrito no Mac, e os dois
//! leitores (o `RemoteRelease` e o `Manifesto`) são o mesmo código nos três
//! sistemas. Rodam no Mac e no Linux.
#![cfg(not(windows))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório; e indexar um serde_json::Value devolve Null em vez de entrar em pânico"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn raiz_do_repositorio() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Uma pasta temporária só deste caso, apagada no fim do teste.
fn pasta(nome: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("seele-manifesto-{nome}-"))
        .tempdir()
        .unwrap()
}

/// Uma árvore de código mínima, com as constantes que o `manifesto.py` lê.
fn arvore(protocolo: u8, api: u32, apis: &str, servidor: Option<&str>) -> tempfile::TempDir {
    let raiz = pasta("arvore");
    let proto = raiz.path().join("crates/seele-proto/src");
    std::fs::create_dir_all(&proto).unwrap();
    let mut version = format!("pub const PROTOCOL_VERSION: u8 = {protocolo};\n");
    if let Some(s) = servidor {
        version.push_str(&format!("pub const VERSAO_DO_SERVIDOR: &str = \"{s}\";\n"));
    }
    std::fs::write(proto.join("version.rs"), version).unwrap();
    std::fs::write(
        proto.join("mods.rs"),
        format!(
            "pub const MOD_API_VERSION: u32 = {api};\npub const APIS_ACEITAS: &[u32] = &[{apis}];\n"
        ),
    )
    .unwrap();
    raiz
}

/// A árvore da linha 1, como a 0.16 a compila.
fn arvore_da_linha_1() -> tempfile::TempDir {
    arvore(9, 6, "6", Some("1.0"))
}

/// A árvore da linha legada, como o `54f542f` a tem: sem `VERSAO_DO_SERVIDOR`.
fn arvore_da_linha_legada() -> tempfile::TempDir {
    arvore(8, 5, "5, 4, 3", None)
}

/// Uma entrega com o que cada sistema produz, cada arquivo com o `.sig` dele.
fn entrega(versao: &str) -> tempfile::TempDir {
    let pasta = pasta(&format!("entrega-{versao}"));
    for nome in [
        format!("SEELE_{versao}_aarch64.app.tar.gz"),
        format!("SEELE_{versao}_x64-instalador.exe"),
        format!("SEELE_{versao}_x64-portatil.tar.gz"),
        format!("seele_{versao}_amd64.deb"),
    ] {
        std::fs::write(pasta.path().join(&nome), format!("conteudo de {nome}")).unwrap();
        std::fs::write(
            pasta.path().join(format!("{nome}.sig")),
            format!("ASSINATURA-DE {nome}"),
        )
        .unwrap();
    }
    pasta
}

struct Saida {
    sucesso: bool,
    texto: String,
    bytes: Vec<u8>,
}

impl Saida {
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.bytes)
            .unwrap_or_else(|erro| panic!("o latest.json não é JSON ({erro}):\n{}", self.texto))
    }
}

fn manifesto_py() -> Command {
    let mut comando = Command::new("python3");
    comando.arg(raiz_do_repositorio().join("empacotar/manifesto.py"));
    comando
}

fn rodar(entrega: &Path, tag: &str, raiz: &Path, anteriores: &[&Path]) -> Saida {
    let mut comando = manifesto_py();
    comando
        .arg(entrega)
        .arg(tag)
        .args(["--repo", "DATA-AND-DEV/SEELE-RELEASES"])
        .arg("--raiz")
        .arg(raiz);
    for anterior in anteriores {
        comando.arg("--anterior").arg(anterior);
    }
    let saida = comando
        .output()
        .expect("este teste precisa de python3, o mesmo que o publicar.sh usa");
    Saida {
        sucesso: saida.status.success(),
        texto: format!(
            "{}{}",
            String::from_utf8_lossy(&saida.stdout),
            String::from_utf8_lossy(&saida.stderr)
        ),
        bytes: std::fs::read(entrega.join("latest.json")).unwrap_or_default(),
    }
}

fn chave_do_projeto() -> seele_lancador::Chave {
    let conf: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(raiz_do_repositorio().join("apps/seele-app/tauri.conf.json"))
            .unwrap(),
    )
    .unwrap();
    seele_lancador::Chave::do_formato_do_atualizador(
        conf["plugins"]["updater"]["pubkey"].as_str().unwrap(),
    )
    .unwrap()
}

fn versoes(json: &serde_json::Value) -> Vec<String> {
    json["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["version"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn o_topo_e_lido_pelo_atualizador_da_0_15_1_e_aponta_para_o_instalador() {
    let raiz = arvore_da_linha_1();
    let e = entrega("0.16.0");
    let s = rodar(e.path(), "v0.16.0", raiz.path(), &[]);
    assert!(s.sucesso, "{}", s.texto);

    let release: tauri_plugin_updater::RemoteRelease = serde_json::from_slice(&s.bytes)
        .expect("o atualizador da 0.15.1 tem de continuar lendo o topo");
    assert_eq!(release.version.to_string(), "0.16.0");
    let windows = release.download_url("windows-x86_64").unwrap().to_string();
    assert!(
        windows.ends_with("-instalador.exe"),
        "o topo do Windows tem de ser o instalador, nunca o portátil: {windows}"
    );
    assert!(release
        .download_url("darwin-aarch64")
        .unwrap()
        .as_str()
        .ends_with(".app.tar.gz"));
    assert!(release
        .download_url("linux-x86_64")
        .unwrap()
        .as_str()
        .ends_with(".deb"));
    assert_eq!(
        release.signature("windows-x86_64").unwrap(),
        "ASSINATURA-DE SEELE_0.16.0_x64-instalador.exe"
    );
}

/// O portátil **nunca** vai para o topo, nem quando falta o instalador. O
/// atualizador da 0.15.1 baixaria um tar.gz e o mandaria executar como
/// instalador; sem instalador, o certo é o Windows ficar fora do topo.
///
/// O teste de cima não prende isso sozinho: com o instalador presente, ele
/// vence pela ordem e esconde um portátil que tivesse entrado ali.
#[test]
fn sem_instalador_o_topo_nao_oferece_o_portatil() {
    let raiz = arvore_da_linha_1();
    let e = entrega("0.16.0");
    for nome in [
        "SEELE_0.16.0_x64-instalador.exe",
        "SEELE_0.16.0_x64-instalador.exe.sig",
    ] {
        std::fs::remove_file(e.path().join(nome)).unwrap();
    }
    let s = rodar(e.path(), "v0.16.0", raiz.path(), &[]);
    assert!(s.sucesso, "{}", s.texto);
    let release: tauri_plugin_updater::RemoteRelease = serde_json::from_slice(&s.bytes).unwrap();
    assert!(
        release.download_url("windows-x86_64").is_err(),
        "o portátil foi oferecido ao atualizador como se fosse o instalador"
    );
    // E o launcher continua tendo o portátil.
    assert_eq!(
        s.json()["versions"][0]["platforms"]["windows-x86_64"]["executable"],
        "SEELE.exe"
    );
}

#[test]
fn as_versoes_sao_lidas_pelo_launcher_com_soma_executavel_e_linha() {
    let raiz = arvore_da_linha_1();
    let e = entrega("0.16.0");
    let s = rodar(e.path(), "v0.16.0", raiz.path(), &[]);
    assert!(s.sucesso, "{}", s.texto);

    let m = seele_lancador::Manifesto::ler(&s.bytes, &chave_do_projeto()).unwrap();
    let nova = m.mais_nova();
    assert_eq!(nova.version.como_texto(), "0.16.0");
    assert_eq!(
        nova.linha(),
        Some(seele_lancador::Linha::nova("1").unwrap())
    );
    assert_eq!(nova.unit.as_ref().unwrap().mod_apis, Some(vec![6]));

    let windows = &nova.platforms["windows-x86_64"];
    assert!(windows.url().unwrap().ends_with("-portatil.tar.gz"));
    assert_eq!(windows.executavel().unwrap().como_texto(), "SEELE.exe");
    let mac = &nova.platforms["darwin-aarch64"];
    assert_eq!(
        mac.executavel().unwrap().como_texto(),
        "SEELE.app/Contents/MacOS/seele-app"
    );
    use sha2::Digest as _;
    let esperado: String = sha2::Sha256::digest(
        std::fs::read(e.path().join("SEELE_0.16.0_x64-portatil.tar.gz")).unwrap(),
    )
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
    assert_eq!(windows.sha256.as_deref(), Some(esperado.as_str()));
    assert!(
        !nova.platforms.contains_key("linux-x86_64"),
        "o .deb instala no sistema e não serve ao launcher"
    );
}

#[test]
fn o_manifesto_acumula_a_linha_legada_e_ordena_por_linha() {
    let legada = arvore_da_linha_legada();
    let um = arvore_da_linha_1();

    let e15 = entrega("0.15.2");
    let s15 = rodar(e15.path(), "v0.15.2", legada.path(), &[]);
    assert!(s15.sucesso, "{}", s15.texto);
    let j15 = s15.json();
    assert_eq!(j15["versions"][0]["unit"]["server"], "0.15");
    assert_eq!(j15["versions"][0]["unit"]["protocol"], 8);
    assert_eq!(
        j15["versions"][0]["unit"]["mod_apis"],
        serde_json::json!([3, 4, 5])
    );

    let e16 = entrega("0.16.0");
    let s16 = rodar(
        e16.path(),
        "v0.16.0",
        um.path(),
        &[&e15.path().join("latest.json")],
    );
    assert!(s16.sucesso, "{}", s16.texto);
    assert_eq!(versoes(&s16.json()), vec!["0.16.0", "0.15.2"]);

    let e161 = entrega("0.16.1");
    let s161 = rodar(
        e161.path(),
        "v0.16.1",
        um.path(),
        &[&e16.path().join("latest.json")],
    );
    assert_eq!(versoes(&s161.json()), vec!["0.16.1", "0.16.0", "0.15.2"]);
    assert_eq!(s161.json()["version"], "0.16.1");
}

#[test]
fn um_conserto_da_linha_legada_nao_toma_o_topo() {
    let legada = arvore_da_linha_legada();
    let um = arvore_da_linha_1();
    let e15 = entrega("0.15.2");
    assert!(rodar(e15.path(), "v0.15.2", legada.path(), &[]).sucesso);
    let e16 = entrega("0.16.0");
    assert!(
        rodar(
            e16.path(),
            "v0.16.0",
            um.path(),
            &[&e15.path().join("latest.json")]
        )
        .sucesso
    );

    let e153 = entrega("0.15.3");
    let s = rodar(
        e153.path(),
        "v0.15.3",
        legada.path(),
        &[&e16.path().join("latest.json")],
    );
    assert!(s.sucesso, "{}", s.texto);
    assert_eq!(versoes(&s.json()), vec!["0.16.0", "0.15.3", "0.15.2"]);
    let release: tauri_plugin_updater::RemoteRelease = serde_json::from_slice(&s.bytes).unwrap();
    assert_eq!(
        release.version.to_string(),
        "0.16.0",
        "um conserto da 0.15 publicado depois não pode virar a atualização de quem está na 1.0"
    );
}

#[test]
fn o_anterior_de_formato_velho_nao_entra_e_isso_e_dito() {
    let um = arvore_da_linha_1();
    let anterior = pasta("anterior-velho");
    let velho = anterior.path().join("latest.json");
    std::fs::write(
        &velho,
        r#"{"version":"0.15.1","pub_date":"2026-10-01T00:00:00Z","platforms":{"darwin-aarch64":{"url":"https://exemplo.invalido/a","signature":"x"}}}"#,
    )
    .unwrap();
    let e = entrega("0.16.0");
    let s = rodar(e.path(), "v0.16.0", um.path(), &[&velho]);
    assert!(s.sucesso, "{}", s.texto);
    assert_eq!(versoes(&s.json()), vec!["0.16.0"]);
    assert!(
        s.texto.contains("não é um manifesto acumulado"),
        "{}",
        s.texto
    );
}

#[test]
fn as_linhas_0_11_a_0_14_nao_entram() {
    let um = arvore_da_linha_1();
    let anterior = pasta("anterior-0-14");
    let caminho = anterior.path().join("latest.json");
    std::fs::write(
        &caminho,
        r#"{"version":"0.14.2","platforms":{},"schema":2,"versions":[
           {"version":"0.14.2","unit":{"protocol":7,"mod_api":5,"server":"0.14"},"platforms":{}}]}"#,
    )
    .unwrap();
    let e = entrega("0.16.0");
    let s = rodar(e.path(), "v0.16.0", um.path(), &[&caminho]);
    assert!(s.sucesso, "{}", s.texto);
    assert_eq!(versoes(&s.json()), vec!["0.16.0"]);
}

/// A lista de revogação do manifesto anterior atravessa: um manifesto novo que
/// a esquecesse seria o «manifesto servido sem lista» de que o launcher se
/// defende, só que vindo de nós.
#[test]
fn a_lista_de_revogacao_do_anterior_atravessa() {
    let um = arvore_da_linha_1();
    let anterior = pasta("anterior-revogacoes");
    let caminho = anterior.path().join("latest.json");
    std::fs::write(
        &caminho,
        r#"{"version":"0.16.0","platforms":{},"schema":2,"versions":[
           {"version":"0.16.0","unit":{"protocol":9,"mod_api":6,"server":"1.0"},"platforms":{}}],
           "revocations":{"document":"ZG9j","signature":"c2ln"}}"#,
    )
    .unwrap();
    let e = entrega("0.16.1");
    let s = rodar(e.path(), "v0.16.1", um.path(), &[&caminho]);
    assert!(s.sucesso, "{}", s.texto);
    assert_eq!(
        s.json()["revocations"],
        serde_json::json!({"document": "ZG9j", "signature": "c2ln"})
    );
}

/// O `schema` nunca sobe, e a unidade continua trazendo os dois campos que o
/// `Unidade` da 0.15.1 exige.
#[test]
fn o_schema_e_o_que_a_0_15_1_le() {
    let um = arvore_da_linha_1();
    let e = entrega("0.16.0");
    let j = rodar(e.path(), "v0.16.0", um.path(), &[]).json();
    assert_eq!(j["schema"], 2);
    assert_eq!(seele_lancador::manifesto::SCHEMA_CONHECIDO, 2);
    for v in j["versions"].as_array().unwrap() {
        assert!(
            v["unit"]["protocol"].is_u64() && v["unit"]["mod_api"].is_u64(),
            "{v}"
        );
    }
}

#[test]
fn sem_a_versao_do_servidor_uma_0_16_nao_sai() {
    let sem = arvore(9, 6, "6", None);
    let e = entrega("0.16.0");
    let s = rodar(e.path(), "v0.16.0", sem.path(), &[]);
    assert!(
        !s.sucesso,
        "uma 0.16 sem linha declarada não pode sair calada"
    );
    assert!(s.texto.contains("VERSAO_DO_SERVIDOR"), "{}", s.texto);
    assert!(
        !e.path().join("latest.json").exists(),
        "e não deixa um latest.json para trás"
    );
}

/// A unidade que o script lê da árvore **de verdade** confere com as
/// constantes que o produto compila. É o guarda contra a expressão regular que
/// para de casar em silêncio.
#[test]
fn a_unidade_lida_do_codigo_e_a_que_o_produto_compila() {
    let saida = manifesto_py()
        .args(["--so-unidade", "--raiz"])
        .arg(raiz_do_repositorio())
        .arg(".")
        .arg("v0.16.0")
        .output()
        .unwrap();
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let unidade: serde_json::Value = serde_json::from_slice(&saida.stdout).unwrap();
    let mut aceitas: Vec<u32> = seele_ffi::mods::APIS_ACEITAS.to_vec();
    aceitas.sort_unstable();
    assert_eq!(unidade["mod_apis"], serde_json::json!(aceitas));
    assert_eq!(unidade["mod_api"], seele_ffi::mods::MOD_API_VERSION);
    assert_eq!(unidade["protocol"], seele_ffi::PROTOCOL_VERSION);
    assert_eq!(unidade["server"], seele_ffi::VERSAO_DO_SERVIDOR);
}
