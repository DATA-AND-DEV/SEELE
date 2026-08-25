//! A marca, conferida em vez de combinada.
//!
//! `docs/marca.md` é normativo, e uma folha de marca que ninguém verifica dura
//! até o primeiro commit apressado. Estes testes travam as regras que dá para
//! ler num arquivo — as outras (uma forma por tela, área livre) continuam
//! dependendo de olho, e estão escritas lá.
//!
//! O **ADR 0034** trocou a marca inteira: saíram o katakana `ゼーレ` e a
//! silhueta do connection de entrada, entrou o símbolo de dois nós e uma ligação.
//! Vários testes daqui guardavam decisões daquela marca. Onde a decisão
//! sobreviveu à troca, o teste ficou e o comentário conta a razão nova; onde a
//! decisão mudou, o teste foi reescrito para cobrar a propriedade nova, com o
//! que ele cobrava antes dito por extenso. Nenhum foi apagado.

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

/// Todo SVG de `design/marca/`, achado em disco em vez de listado aqui.
///
/// Uma lista escrita à mão é uma lista que esquece o arquivo novo, e foi assim
/// que a marca velha andou meses com uma variação fora de conferência.
fn svgs_da_marca() -> Vec<String> {
    let pasta = raiz().join("design/marca");
    let mut achados: Vec<String> = std::fs::read_dir(&pasta)
        .unwrap_or_else(|_| panic!("não li {}", pasta.display()))
        .filter_map(|entrada| entrada.ok())
        .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
        .filter(|nome| nome.ends_with(".svg"))
        .map(|nome| format!("design/marca/{nome}"))
        .collect();
    achados.sort();
    assert!(
        achados.len() >= 6,
        "design/marca/ só tem {} SVG; a folha lista símbolo, assinatura, \
         assinatura com tagline, empilhada, mono, muda e os de ícone",
        achados.len()
    );
    achados
}

/// O valor de um atributo numérico de um elemento já recortado.
fn atributo(elemento: &str, nome: &str, onde: &str) -> f64 {
    let marcador = format!("{nome}=\"");
    let inicio = elemento
        .find(&marcador)
        .unwrap_or_else(|| panic!("{onde} não tem `{nome}`:\n{elemento}"))
        + marcador.len();
    let resto = &elemento[inicio..];
    let fim = resto
        .find('"')
        .unwrap_or_else(|| panic!("{onde} tem `{nome}` sem fechar"));
    resto[..fim]
        .parse()
        .unwrap_or_else(|_| panic!("{onde} tem `{nome}` que não é número: {}", &resto[..fim]))
}

/// O primeiro elemento `<tag ...>` que satisfaz `filtro`.
fn elemento<'a>(svg: &'a str, tag: &str, filtro: impl Fn(&str) -> bool, onde: &str) -> &'a str {
    let abertura = format!("<{tag}");
    svg.match_indices(&abertura)
        .map(|(i, _)| {
            let resto = &svg[i..];
            let fim = resto.find('>').unwrap_or(resto.len());
            &resto[..fim]
        })
        .find(|trecho| filtro(trecho))
        .unwrap_or_else(|| panic!("{onde} não tem um `<{tag}` que sirva"))
}

#[test]
fn a_marca_servida_pelo_app_e_a_marca_de_design() {
    // Duas cópias de um arquivo é uma que vai ficar para trás. As do `ui/`
    // existem porque a CSP do app só aceita `'self'`, e o gerador é quem copia.
    //
    // O símbolo entrou nesta lista com o ADR 0034: ele é a forma que a tela de
    // inicialização carrega, e ficou fora do gerador enquanto era desenhado à
    // mão. Uma cópia que ninguém confere é a próxima a divergir.
    for (origem, servida) in [
        (
            "design/marca/simbolo.svg",
            "apps/seele-app/ui/marca-simbolo.svg",
        ),
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
    // A regra sobreviveu à troca de marca; a razão dela é outra.
    //
    // A razão velha era o katakana: com `<text>`, `ゼーレ` virava a face japonesa
    // que o sistema tivesse — Hiragino no macOS, Yu Gothic no Windows — e
    // substituir a face era o que a folha proibia. Não há mais japonês.
    //
    // A razão nova vale para o alfabeto latino do mesmo jeito. Os SVGs da marca
    // nunca são lidos dentro da página: `marca-muda.svg` é favicon,
    // `marca-simbolo.svg` entra por `<img>`, e os de ícone são rasterizados
    // pelo `qlmanage`. Nos três casos o SVG é documento isolado, e o
    // `@font-face` de `ui/fontes.css` não alcança lá dentro — um
    // `font-family: "Saira Condensed"` cairia em Arial Narrow, que é a falha
    // silenciosa que aquele arquivo descreve. Trocar a fonte por `data:` custa
    // 20 KB por arquivo, num favicon.
    for arquivo in svgs_da_marca() {
        let svg = ler(&arquivo);
        assert!(
            !svg.contains("<text"),
            "{arquivo} desenha a marca como texto"
        );
        assert!(
            !svg.contains("font-family"),
            "{arquivo} depende de uma fonte instalada"
        );
    }
}

#[test]
fn a_marca_nunca_usa_o_vermelho_de_falha() {
    // Regra 4 da folha, e o ADR 0034 a repete de propósito: vermelho é estado
    // de falha, e a marca não pode significar erro. É a única cor do sistema
    // proibida na marca, inclusive na queda — ali a diagonal some e os dois nós
    // ficam como estão, sem trocar de cor.
    let vermelho = "#FF1A1A";
    for arquivo in svgs_da_marca() {
        let svg = ler(&arquivo).to_uppercase();
        assert!(!svg.contains(vermelho), "{arquivo} usa o vermelho de falha");
    }
}

#[test]
fn a_marca_so_usa_cores_que_os_tokens_definem() {
    // A folha de marca e os tokens congelados em M0.12 dizem os mesmos valores.
    // Se um dia divergirem, é aqui que se descobre — e não numa tela.
    //
    // A lista encolheu de dez para quatro com o ADR 0034. As seis placas de
    // profundidade eram do connection: cor plana deslocada por trás de um contorno de
    // octógono, para dar volume sem sombra. O símbolo de dois nós não tem
    // contorno para deslocar, e as seis saíram da marca junto com ele. Os
    // tokens continuam em `tokens.css` porque quem os tira de lá é quem cuidar
    // da folha de tokens, não a marca.
    let permitidas = ["#F2521F", "#050403", "#EAE3CF", "#7A7061"];
    for arquivo in svgs_da_marca() {
        let svg = ler(&arquivo).to_uppercase();
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
fn toda_variacao_carrega_a_geometria_do_simbolo() {
    // `simbolo.svg` é a fonte, e uma marca com duas geometrias é duas marcas.
    // As três formas dele — o enlace, o nó cheio e o nó vazio — entram nas
    // variações letra por letra; o que cada suporte troca é a cor.
    //
    // Ficam de fora, e é decisão: `muda.svg` e `icone-app-16.svg` engrossam o
    // traço para a faixa de 16 px. Quem confere aqueles dois é
    // `cada_faixa_de_icone_aguenta_os_pixeis_dela`.
    let simbolo = ler("design/marca/simbolo.svg");
    let formas = [
        "d=\"M34 34L62 62\"",
        "x=\"12\" y=\"12\" width=\"24\" height=\"24\"",
        "x=\"62\" y=\"62\" width=\"20\" height=\"20\"",
    ];
    for forma in formas {
        assert!(
            simbolo.contains(forma),
            "simbolo.svg mudou de geometria: não achei `{forma}`. \
             Se a mudança é de propósito, este teste é que tem de mudar junto."
        );
    }

    for arquivo in [
        "design/marca/assinatura.svg",
        "design/marca/assinatura-tagline.svg",
        "design/marca/empilhada.svg",
        "design/marca/mono.svg",
        "design/marca/cartela.svg",
        "design/marca/icone-app-128.svg",
    ] {
        let svg = ler(arquivo);
        for forma in formas {
            assert!(
                svg.contains(forma),
                "{arquivo} não traz `{forma}` do símbolo. \
                 Rode design/marca/gerar-wordmark.py."
            );
        }
    }
}

#[test]
fn o_favicon_e_a_forma_muda() {
    // A aba do navegador desenha 16 px e nada mais, e a folha manda trocar de
    // forma em vez de reduzir. Antes o motivo era o nome: `ゼーレ` dentro da
    // cinta virava três borrões. O nome saiu, e o motivo continua de pé por
    // aritmética de traço — ver o teste das faixas logo abaixo.
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
fn cada_faixa_de_icone_aguenta_os_pixeis_dela() {
    // Aqui morava `o_icone_de_app_tem_a_cinta_vazia_e_um_desenho_por_faixa`,
    // que cobrava quatro arquivos de ícone, nenhum `<path>` dentro deles, e um
    // traço estritamente mais grosso a cada faixa menor. Duas dessas três
    // asserções morreram com o ADR 0034, e não por afrouxamento:
    //
    // - **`<path>` era o proxy de "glifo"**, porque na marca velha só o
    //   katakana era desenhado com caminho. Agora o enlace é um `<path>`, e o
    //   proxy passou a acusar o próprio símbolo. O que se cobra no lugar é o
    //   que se queria dizer: um caminho só, e de um traço só — um contorno de
    //   letra tem dezenas de comandos.
    // - **Quatro faixas eram do connection**, que tinha contorno de octógono, cinta e
    //   placas, e a razão entre eles quebrava em quatro pontos. O símbolo tem
    //   um valor de traço só, então há um limiar só, em 48 px. Guardar quatro
    //   arquivos seria guardar três cópias do mesmo desenho para divergirem.
    //
    // O que passou a valer é a conta que obrigava os redesenhos desde sempre,
    // agora medida em vez de deduzida: em cada faixa, e no menor tamanho que
    // ela serve, o traço tem de valer 1 px inteiro e o furo do nó vazio 2 px.
    // Reduzir o desenho de 128 até 16 devolve traço de 0,67 px — cinza no dock,
    // e é essa a redução que este teste reprova.
    //
    // Continua valendo, e é o resto da marca velha que sobreviveu: no ícone o
    // nome não entra em tamanho nenhum.
    for (faixa, menor_servido) in [(128u32, 48u32), (16, 16)] {
        let arquivo = format!("design/marca/icone-app-{faixa}.svg");
        let svg = ler(&arquivo);

        assert!(
            !svg.contains("<text"),
            "{arquivo} desenha a marca como texto"
        );
        let caminhos = svg.matches("<path").count();
        assert_eq!(
            caminhos, 1,
            "{arquivo} tem {caminhos} caminhos; o ícone só desenha o enlace, e \
             mais de um é o nome voltando para dentro do ícone"
        );
        let enlace = elemento(&svg, "path", |_| true, &arquivo);
        let comandos = enlace.matches('L').count() + enlace.matches('C').count();
        assert_eq!(
            comandos, 1,
            "{arquivo} tem um caminho de {comandos} traços; o enlace é uma reta \
             só, e um contorno de letra tem dezenas"
        );

        // A grade do desenho, para converter unidade em pixel de tela.
        let interno = elemento(
            &svg,
            "svg",
            |trecho| trecho.contains("viewBox=\"0 0 96 96\""),
            &arquivo,
        );
        let lado_do_bloco = atributo(interno, "width", &arquivo);
        // Em pixel de tela, no menor tamanho que a faixa serve. A multiplicação
        // vem antes da divisão de propósito: `6 * 16 / 96` dá 1 exato, e
        // `6 * (16 / 96)` dá 0,9999999999999999 — a asserção de "1 px inteiro"
        // reprovaria o desenho certo por causa do arredondamento.
        let px = |unidades: f64| unidades * f64::from(menor_servido) / 96.0;
        assert!(
            (lado_do_bloco - f64::from(faixa)).abs() < f64::EPSILON,
            "{arquivo}: o bloco tem {lado_do_bloco} e a faixa é {faixa}"
        );

        let vazio = elemento(
            &svg,
            "rect",
            |trecho| trecho.contains("fill=\"none\""),
            &arquivo,
        );
        let traco = atributo(vazio, "stroke-width", &arquivo);
        let lado = atributo(vazio, "width", &arquivo);
        let furo = lado - traco;

        assert!(
            px(traco) >= 1.0,
            "{arquivo}: a {menor_servido} px o traço vale {:.2} px. \
             Abaixo de um pixel inteiro ele volta cinza do rasterizador.",
            px(traco)
        );
        assert!(
            px(furo) >= 2.0,
            "{arquivo}: a {menor_servido} px o furo do nó vazio vale {:.2} px. \
             Abaixo de dois o furo fecha e os dois nós viram a mesma forma — \
             que é exatamente o que a marca não pode dizer.",
            px(furo)
        );
        let traco_do_enlace = atributo(enlace, "stroke-width", &arquivo);
        assert!(
            px(traco_do_enlace) >= 1.0,
            "{arquivo}: a {menor_servido} px o enlace vale {:.2} px, e sem ele \
             sobram dois nós sem ligação nenhuma",
            px(traco_do_enlace)
        );

        // Massas iguais: o nó vazio se estende tanto quanto o cheio, com o
        // traço contado por fora. Engrossar o traço fecha o furo, nunca cresce
        // o nó — é o furo que paga a faixa miúda.
        let cheio = elemento(
            &svg,
            "rect",
            |trecho| trecho.contains("fill=\"#050403\""),
            &arquivo,
        );
        let extensao_do_cheio = atributo(cheio, "width", &arquivo);
        assert!(
            ((lado + traco) - extensao_do_cheio).abs() < f64::EPSILON,
            "{arquivo}: o nó vazio se estende por {} e o cheio por {}. \
             A folha pede massas iguais.",
            lado + traco,
            extensao_do_cheio
        );
    }
}

#[test]
fn os_icones_de_windows_e_linux_sao_png_de_32_bits() {
    // Windows e Linux recebem a **arte de placa**, de canto reto e opaca,
    // sangrada até a borda: a mesma que o macOS recebe recortada na superelipse.
    // O que dá para conferir daqui sem um decodificador é o formato — assinatura
    // de PNG e tipo de cor 6 (RGBA) no byte 25, que é o que o gerador escreve
    // para as duas famílias e o que o `.ico` embute. A garantia de pixel — canto
    // na cor da placa, marca desenhada por cima dela — é do `conferir()` de
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
