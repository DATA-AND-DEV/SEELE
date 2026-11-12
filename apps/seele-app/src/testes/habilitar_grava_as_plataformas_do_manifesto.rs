use super::habilitado_de;
use seele_server::persistence::mods::{enable, enabled};
use seele_server::persistence::{Location, Persistence};

/// Um pacote de verdade numa pasta, lido pelo caminho da casca
/// (`seele_ffi::mods::ler_pasta`) — a casca não nomeia o protocolo.
fn instalado(resto: &str, conteudo: &str) -> seele_ffi::mods::ModInstalado {
    let pasta = tempfile::tempdir().expect("pasta");
    std::fs::create_dir_all(pasta.path().join("cliente")).expect("cliente");
    std::fs::write(pasta.path().join("cliente/main.js"), conteudo).expect("script");
    std::fs::write(
            pasta.path().join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"seele/bot","version":"1.0.0","api":{},"repo":"https://github.com/seele/bot","reach":[],"client":"cliente/main.js"{resto}}}"#,
                seele_ffi::mods::MOD_API_VERSION
            ),
        )
        .expect("manifesto");
    seele_ffi::mods::ler_pasta(&pasta.path().to_string_lossy()).expect("pacote válido")
}

/// **Habilitar grava as plataformas do manifesto no mesmo ato que o hash**,
/// e habilitar de novo, com outros bytes, as regrava. Com `None` aqui, um
/// MOD `["app"]` seria anunciado nas duas até a próxima subida.
#[test]
fn habilitar_grava_e_regrava_as_plataformas() {
    let banco = Persistence::open(&Location::Memory).expect("banco");
    let so_app = instalado(r#","plataformas":["app"]"#, "1");
    enable(&banco, &habilitado_de(so_app.clone())).expect("habilitar");
    let gravada = enabled(&banco)
        .expect("listar")
        .first()
        .expect("o habilitado está na lista")
        .plataformas
        .clone();
    assert_eq!(
        serde_json::to_value(gravada).expect("json"),
        serde_json::json!(["app"])
    );

    let sem_chave = instalado("", "2");
    assert_ne!(so_app.hash, sem_chave.hash);
    enable(&banco, &habilitado_de(sem_chave)).expect("habilitar");
    let gravada = enabled(&banco)
        .expect("listar")
        .first()
        .expect("o habilitado está na lista")
        .plataformas
        .clone();
    assert_eq!(
        serde_json::to_value(gravada).expect("json"),
        serde_json::json!(["app", "navegador"]),
        "sem a chave, as duas, regravadas com os bytes novos"
    );
}
