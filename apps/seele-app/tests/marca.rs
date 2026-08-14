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
    for arquivo in [
        "design/marca/assinatura.svg",
        "design/marca/reduzida.svg",
        "design/marca/cartela.svg",
    ] {
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
        "design/marca/cartela.svg",
        "design/marca/icone-app-16.svg",
        "design/marca/icone-app-32.svg",
        "design/marca/icone-app-64.svg",
        "design/marca/icone-app-128.svg",
    ] {
        let svg = ler(arquivo).to_uppercase();
        assert!(!svg.contains(vermelho), "{arquivo} usa o vermelho de falha");
    }
}

#[test]
fn a_marca_so_usa_cores_que_os_tokens_definem() {
    // A folha de marca e os tokens congelados em M0.12 dizem os mesmos valores.
    // Se um dia divergirem, é aqui que se descobre — e não numa tela.
    // Laranja, negro, osso e o apagado do descritor, mais as seis placas que o
    // ícone de app usa para dar profundidade em cor plana. Todas em tokens.css.
    let permitidas = [
        "#F2521F", "#050403", "#EAE3CF", "#7A7061", "#A83A10", "#7A2A0B", "#4A1806", "#FFA070",
        "#C4400F", "#8E2A08",
    ];
    for arquivo in [
        "design/marca/assinatura.svg",
        "design/marca/reduzida.svg",
        "design/marca/muda.svg",
        "design/marca/cartela.svg",
        "design/marca/icone-app-16.svg",
        "design/marca/icone-app-32.svg",
        "design/marca/icone-app-64.svg",
        "design/marca/icone-app-128.svg",
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
fn o_icone_de_app_tem_a_cinta_vazia_e_um_desenho_por_faixa() {
    // Aqui morava `o_icone_de_32_e_muda_e_o_de_128_tem_o_nome`, que comparava o
    // tamanho em bytes do PNG de 32 com o de 128 para provar que só o maior
    // trazia ゼーレ dentro da cinta. A arte de ícone não traz o nome em tamanho
    // nenhum: a 128 px cada katakana teria seis pixels de largura, então a
    // cinta vai vazia em toda faixa e o nome fica na assinatura e na cartela. A
    // asserção velha ficou falsa por decisão de desenho, não por regressão, e
    // um teste que se conserta afrouxando não vale o arquivo em que está.
    //
    // O que passou a valer, e é o que se confere: são quatro desenhos, um por
    // faixa, e o traço engrossa conforme o bloco encolhe. Glifo é a única coisa
    // que a marca desenha com `<path>` — um `<path>` num arquivo de ícone é o
    // nome voltando para dentro da cinta.
    let mut traco_maior = 0u32;
    for faixa in [128, 64, 32, 16] {
        let arquivo = format!("design/marca/icone-app-{faixa}.svg");
        let svg = ler(&arquivo);
        assert!(
            !svg.contains("<text"),
            "{arquivo} desenha a marca como texto"
        );
        assert!(
            !svg.contains("<path"),
            "{arquivo} traz um glifo dentro da cinta; no ícone ela vai vazia"
        );

        // O contorno do plug é o traço pintado de negro. As placas de
        // profundidade têm as suas próprias cores e não contam aqui.
        let marcador = "stroke=\"#050403\" stroke-width=\"";
        let inicio = svg
            .find(marcador)
            .unwrap_or_else(|| panic!("{arquivo} não tem o contorno do plug em negro"))
            + marcador.len();
        let resto = &svg[inicio..];
        let fim = resto
            .find('"')
            .unwrap_or_else(|| panic!("{arquivo} tem stroke-width sem fechar"));
        let traco: u32 = resto[..fim]
            .parse()
            .unwrap_or_else(|_| panic!("{arquivo} tem stroke-width que não é inteiro"));

        assert!(
            traco > traco_maior,
            "o traço de {faixa} é {traco} e o da faixa maior era {traco_maior}: \
             o desenho menor precisa do traço mais grosso, senão some ao reduzir"
        );
        traco_maior = traco;
    }
}

#[test]
fn os_icones_de_windows_e_linux_sao_png_de_32_bits() {
    // Windows e Linux recebem a **arte de placa**, de canto reto e opaca,
    // sangrada até a borda: a mesma que o macOS recebe recortada na superelipse.
    // O que dá para conferir daqui sem um decodificador é o formato — assinatura
    // de PNG e tipo de cor 6 (RGBA) no byte 25, que é o que o gerador escreve
    // para as duas famílias e o que o `.ico` embute. A garantia de pixel — canto
    // na cor da placa, cinta desenhada por cima dela — é do `conferir()` de
    // `design/marca/gerar-icones.py`, que reprova antes de qualquer arquivo ser
    // escrito. Foi uma falha do `qlmanage` que a pediu: tamanho certo, arte num
    // canto, branco no resto.
    for arquivo in ["32x32.png", "128x128.png", "128x128@2x.png", "icon.png"] {
        let png = std::fs::read(raiz().join("apps/seele-app/icons").join(arquivo))
            .unwrap_or_else(|_| panic!("não li o ícone {arquivo}"));
        assert_eq!(&png[0..4], b"\x89PNG", "{arquivo} não é um PNG");
        assert_eq!(
            png[25], 6,
            "{arquivo} não é RGBA: não é o que o gerador escreve"
        );
    }
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
