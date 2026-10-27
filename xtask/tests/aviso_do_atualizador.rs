//! **O aviso que um app já instalado lê antes de instalar** (E4 e E5 da 0.17).
//!
//! O `notes` do topo do `latest.json` é o que o atualizador de toda versão
//! publicada mostra no bloco «O QUE MUDA», antes de instalar. Desde a 0.17, o
//! `empacotar/manifesto.py` o monta com o bloco `AVISO-DO-ATUALIZADOR` das
//! notas da versão, em texto puro e com teto, e mantém a frase de sempre
//! quando o bloco não existe. É o único aviso que chega a quem ainda está na
//! 0.16: ela só o mostra, e não pede confirmação.
//!
//! Estes testes rodam o `manifesto.py` de verdade, contra uma árvore mínima,
//! como o `apps/seele-app/tests/manifesto_hibrido.rs`. Fora do Windows pela
//! mesma razão dele: o PC Windows da bateria não tem Python, e quem roda o
//! `manifesto.py` é o `publicar.sh`, no Mac.
#![cfg(not(windows))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn raiz_do_repositorio() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Uma pasta só deste caso, recriada vazia.
fn pasta(nome: &str) -> PathBuf {
    static CONTADOR: AtomicUsize = AtomicUsize::new(0);
    let caminho = std::env::temp_dir().join(format!(
        "seele-aviso-{nome}-{}-{}",
        std::process::id(),
        CONTADOR.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&caminho);
    std::fs::create_dir_all(&caminho).unwrap();
    caminho
}

/// Uma árvore da linha 2, com as notas `notas` para a versão `versao`.
fn arvore(versao: &str, notas: Option<&str>) -> PathBuf {
    let raiz = pasta("arvore");
    let proto = raiz.join("crates/seele-proto/src");
    std::fs::create_dir_all(&proto).unwrap();
    std::fs::write(
        proto.join("version.rs"),
        "pub const PROTOCOL_VERSION: u8 = 10;\npub const VERSAO_DO_SERVIDOR: &str = \"2.0\";\n",
    )
    .unwrap();
    std::fs::write(
        proto.join("mods.rs"),
        "pub const MOD_API_VERSION: u32 = 6;\npub const APIS_ACEITAS: &[u32] = &[6];\n",
    )
    .unwrap();
    if let Some(texto) = notas {
        let pasta_das_notas = raiz.join("empacotar/notas");
        std::fs::create_dir_all(&pasta_das_notas).unwrap();
        std::fs::write(pasta_das_notas.join(format!("{versao}.md")), texto).unwrap();
    }
    raiz
}

fn entrega(versao: &str) -> PathBuf {
    let pasta = pasta("entrega");
    let nome = format!("SEELE_{versao}_aarch64.app.tar.gz");
    std::fs::write(pasta.join(&nome), "conteudo").unwrap();
    std::fs::write(pasta.join(format!("{nome}.sig")), "ASSINATURA").unwrap();
    pasta
}

struct Saida {
    sucesso: bool,
    texto: String,
    manifesto: Option<serde_json::Value>,
}

fn rodar(versao: &str, notas: Option<&str>) -> Saida {
    let raiz = arvore(versao, notas);
    let entrega = entrega(versao);
    let saida = Command::new("python3")
        .arg(raiz_do_repositorio().join("empacotar/manifesto.py"))
        .arg(&entrega)
        .arg(format!("v{versao}"))
        .args(["--repo", "DATA-AND-DEV/SEELE-RELEASES"])
        .arg("--raiz")
        .arg(&raiz)
        .output()
        .expect("este teste precisa de python3, o mesmo que o publicar.sh usa");
    let manifesto = std::fs::read(entrega.join("latest.json"))
        .ok()
        .map(|bytes| serde_json::from_slice(&bytes).unwrap());
    let _ = std::fs::remove_dir_all(&raiz);
    let _ = std::fs::remove_dir_all(&entrega);
    Saida {
        sucesso: saida.status.success(),
        texto: format!(
            "{}{}",
            String::from_utf8_lossy(&saida.stdout),
            String::from_utf8_lossy(&saida.stderr)
        ),
        manifesto,
    }
}

const FRASE: &str =
    "SEELE 0.17.0. As notas estão em https://github.com/DATA-AND-DEV/SEELE-RELEASES/releases/tag/v0.17.0";

fn notes(saida: &Saida) -> String {
    assert!(saida.sucesso, "o manifesto.py reprovou:\n{}", saida.texto);
    let manifesto = saida.manifesto.as_ref().expect("o latest.json foi escrito");
    let topo = manifesto["notes"].as_str().unwrap().to_owned();
    assert_eq!(
        manifesto["versions"][0]["notes"].as_str(),
        Some(topo.as_str()),
        "a entrada da versão e o topo dizem notas diferentes"
    );
    topo
}

/// **Sem marcador, a frase de sempre** — e sem notas também.
#[test]
fn sem_o_bloco_o_notes_e_a_frase_de_sempre() {
    assert_eq!(notes(&rodar("0.17.0", None)), FRASE);
    assert_eq!(
        notes(&rodar(
            "0.17.0",
            Some("> ### Novidades\n>\n> Nada de aviso aqui.\n")
        )),
        FRASE
    );
}

/// **Com o bloco, o aviso em texto puro vem antes da frase**: sem a citação,
/// sem os títulos, sem a ênfase, sem o código em linha, com o link escrito, e
/// sem os comentários (nem o de condição).
#[test]
fn com_o_bloco_o_aviso_vem_em_texto_puro_antes_da_frase() {
    let notas = "\
> ### Antes\n\
>\n\
<!-- AVISO-DO-ATUALIZADOR -->\n\
> ### A 0.17 não conversa com a 0.16\n\
>\n\
> A 0.17 fala o **Servidor 2.0**, por `WebRTC`, e *não* entra nos servidores\n\
> da 0.16. Veja [as notas](https://exemplo.invalido/n).\n\
>\n\
> <!-- CONDIÇÃO: um comentário que não vai -->\n\
> A 0.16 continua guardada ao lado.\n\
<!-- /AVISO-DO-ATUALIZADOR -->\n\
> O resto das notas, que não vai.\n";
    let texto = notes(&rodar("0.17.0", Some(notas)));
    assert_eq!(
        texto,
        format!(
            "A 0.17 não conversa com a 0.16\n\n\
             A 0.17 fala o Servidor 2.0, por WebRTC, e não entra nos servidores da 0.16. \
             Veja as notas (https://exemplo.invalido/n).\n\n\
             A 0.16 continua guardada ao lado.\n\n{FRASE}"
        )
    );
    assert!(!texto.contains("O resto das notas"));
    assert!(!texto.contains("CONDIÇÃO"));
}

/// **O teto reprova, e não corta.** Um aviso cortado no meio da frase seria o
/// produto dizendo menos do que sabe.
#[test]
fn um_aviso_acima_do_teto_reprova_o_manifesto() {
    let longo = format!(
        "<!-- AVISO-DO-ATUALIZADOR -->\n{}\n<!-- /AVISO-DO-ATUALIZADOR -->\n",
        "palavra ".repeat(200)
    );
    let saida = rodar("0.17.0", Some(&longo));
    assert!(
        !saida.sucesso,
        "um aviso de 1600 caracteres passou:\n{}",
        saida.texto
    );
    assert!(
        saida.texto.contains("acima do teto de 1200"),
        "{}",
        saida.texto
    );
    assert!(
        saida.manifesto.is_none(),
        "o latest.json foi escrito assim mesmo"
    );
}

/// Um marcador sem o par, dois blocos, ou um bloco vazio reprovam com a frase.
#[test]
fn um_bloco_torto_reprova_com_a_frase() {
    for (nome, notas) in [
        ("sem o fim", "<!-- AVISO-DO-ATUALIZADOR -->\naviso\n"),
        ("sem o começo", "aviso\n<!-- /AVISO-DO-ATUALIZADOR -->\n"),
        (
            "ao contrário",
            "<!-- /AVISO-DO-ATUALIZADOR -->\naviso\n<!-- AVISO-DO-ATUALIZADOR -->\n",
        ),
        (
            "dois blocos",
            "<!-- AVISO-DO-ATUALIZADOR -->a<!-- /AVISO-DO-ATUALIZADOR -->\n\
             <!-- AVISO-DO-ATUALIZADOR -->b<!-- /AVISO-DO-ATUALIZADOR -->\n",
        ),
        (
            "vazio",
            "<!-- AVISO-DO-ATUALIZADOR -->\n> <!-- só comentário -->\n<!-- /AVISO-DO-ATUALIZADOR -->\n",
        ),
    ] {
        let saida = rodar("0.17.0", Some(notas));
        assert!(!saida.sucesso, "o bloco {nome} passou:\n{}", saida.texto);
        assert!(saida.texto.contains("aviso do atualizador"), "{nome}: {}", saida.texto);
    }
}

/// **As notas de verdade da 0.17 cabem.** O rascunho de
/// `empacotar/notas/0.17.0.md` tem o bloco, e ele passa pelo teto.
#[test]
fn o_aviso_das_notas_da_0_17_cabe_no_teto() {
    let notas =
        std::fs::read_to_string(raiz_do_repositorio().join("empacotar/notas/0.17.0.md")).unwrap();
    let texto = notes(&rodar("0.17.0", Some(&notas)));
    assert!(texto.ends_with(FRASE));
    assert!(
        texto.len() > FRASE.len() + 100,
        "as notas da 0.17 não levam o aviso: {texto}"
    );
    for trecho in ["0.16", "WebRTC"] {
        assert!(
            texto.contains(trecho),
            "o aviso da 0.17 não diz «{trecho}»: {texto}"
        );
    }
    assert!(!texto.contains("**") && !texto.contains('`') && !texto.contains("<!--"));
}
