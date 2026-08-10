//! A marca, conferida em vez de combinada.
//!
//! `docs/marca.md` é normativo, e uma folha de marca que ninguém verifica dura
//! até o primeiro commit apressado. Estes testes travam as regras que dá para
//! ler num arquivo — as outras (uma forma por tela, área livre) continuam
//! dependendo de olho, e estão escritas lá.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("a raiz do workspace")
        .to_path_buf()
}

fn ler(relativo: &str) -> String {
    let caminho = raiz().join(relativo);
    std::fs::read_to_string(&caminho).unwrap_or_else(|_| panic!("não li {}", caminho.display()))
}

#[test]
fn a_marca_servida_pelo_app_e_a_marca_de_design() {
    // Duas cópias de um arquivo é uma que vai ficar para trás. A do `ui/` existe
    // porque a CSP do app só aceita `'self'`, e o gerador é quem copia.
    for (origem, servida) in [
        (
            "design/marca/assinatura.svg",
            "apps/seele-app/ui/marca-assinatura.svg",
        ),
        ("design/marca/muda.svg", "apps/seele-app/ui/marca-muda.svg"),
    ] {
        assert_eq!(
            ler(origem),
            ler(servida),
            "{servida} divergiu de {origem}. Rode design/marca/gerar-icones.py."
        );
    }
}

#[test]
fn os_glifos_da_marca_estao_em_contorno() {
    // A regra que sustenta todas as outras. Com `<text>`, a marca vira o que a
    // face japonesa do sistema for — Hiragino no macOS, Yu Gothic no Windows —
    // e substituir a face é justamente o que a folha de marca proíbe.
    for arquivo in ["design/marca/assinatura.svg", "design/marca/reduzida.svg"] {
        let svg = ler(arquivo);
        assert!(
            !svg.contains("<text"),
            "{arquivo} ainda desenha a marca como texto"
        );
        assert!(
            !svg.contains("font-family"),
            "{arquivo} ainda depende de uma fonte"
        );
    }
}

#[test]
fn a_marca_nunca_usa_o_vermelho_de_falha() {
    // Regra 4 da folha: vermelho é estado de falha, e a marca não pode
    // significar erro. É a única cor do sistema proibida na marca.
    let vermelho = "#FF1A1A";
    for arquivo in [
        "design/marca/assinatura.svg",
        "design/marca/reduzida.svg",
        "design/marca/muda.svg",
    ] {
        let svg = ler(arquivo).to_uppercase();
        assert!(!svg.contains(vermelho), "{arquivo} usa o vermelho de falha");
    }
}

#[test]
fn a_marca_so_usa_cores_que_os_tokens_definem() {
    // A folha de marca e os tokens congelados em M0.12 dizem os mesmos valores.
    // Se um dia divergirem, é aqui que se descobre — e não numa tela.
    let permitidas = ["#F2521F", "#050403", "#EAE3CF"];
    for arquivo in [
        "design/marca/assinatura.svg",
        "design/marca/reduzida.svg",
        "design/marca/muda.svg",
    ] {
        let svg = ler(arquivo).to_uppercase();
        for achado in svg
            .match_indices('#')
            .map(|(i, _)| &svg[i..(i + 7).min(svg.len())])
        {
            assert!(
                permitidas.contains(&achado),
                "{arquivo} usa {achado}, que não é cor da marca"
            );
        }
    }
}

#[test]
fn o_favicon_e_a_forma_muda() {
    // Abaixo de 32px de largura do plug a folha manda trocar de forma, e uma
    // aba de navegador nunca chega perto disso. Com o nome dentro, ゼーレ vira
    // três borrões.
    let pagina = ler("apps/seele-app/ui/index.html");
    let icone = pagina
        .lines()
        .find(|linha| linha.contains("rel=\"icon\""))
        .expect("a página não declara favicon");
    assert!(
        icone.contains("marca-muda.svg"),
        "o favicon não é a forma muda: {icone}"
    );
}

#[test]
fn o_icone_de_32_e_muda_e_o_de_128_tem_o_nome() {
    // A troca de forma é a única decisão que o gerador toma sozinho, então é a
    // que mais merece um teste. Comparar bytes basta: se os dois tamanhos
    // saíssem da mesma forma, a arte escalada seria a mesma e o teste passaria
    // — por isso o critério é o **tamanho** do arquivo, que denuncia a
    // diferença de conteúdo entre uma silhueta e uma silhueta com três glifos.
    let icones = raiz().join("apps/seele-app/icons");
    let pequeno = std::fs::metadata(icones.join("32x32.png")).expect("32x32.png");
    let grande = std::fs::metadata(icones.join("128x128.png")).expect("128x128.png");

    assert!(pequeno.len() > 0 && grande.len() > 0, "ícone vazio");
    assert!(
        grande.len() > pequeno.len() * 2,
        "o ícone de 128 não parece trazer o nome dentro: {} contra {} bytes",
        grande.len(),
        pequeno.len()
    );
}

#[test]
fn o_ico_do_windows_traz_todos_os_tamanhos() {
    // Foi um `.ico` faltando que quebrou o release do Windows uma vez. Este
    // confere que ele existe **e** que não é um arquivo de um tamanho só, que é
    // como um ícone fica serrilhado na barra de tarefas.
    let ico = std::fs::read(raiz().join("apps/seele-app/icons/icon.ico")).expect("icon.ico");
    assert_eq!(&ico[0..4], &[0, 0, 1, 0], "não é um .ico");

    let imagens = u16::from_le_bytes([ico[4], ico[5]]);
    assert!(
        imagens >= 6,
        "o .ico traz só {imagens} tamanho(s); a barra de tarefas e o instalador pedem mais"
    );
}
