//! Desenha as telas do `plug` num buffer e imprime como texto.
//!
//! ```text
//! cargo run --example telas -p seele-tui
//! ```
//!
//! Serve para o README. É um retrato de verdade — sai do mesmo `ui::render`
//! que desenha no terminal, com os mesmos widgets e as mesmas larguras — e não
//! um desenho feito à mão que envelhece calado. Quando a interface mudar, roda
//! de novo e cola.
//!
//! Sem cor, porque é texto. O que o texto **não** mostra é justamente o que a
//! paleta carrega: a Taxa de Sincronização muda de cor por faixa. Mas o número
//! e a marca de bloco continuam ali, e é essa a promessa de
//! `specs/05-cliente-tui.md` — nenhuma informação transmitida só por cor.

#![allow(clippy::print_stdout, reason = "é uma ferramenta de linha de comando")]
#![allow(clippy::expect_used, reason = "idem")]

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use seele_tui::app::{Alert, App, ChatLine, Key, Node, RosterEntry, Screen};
use seele_tui::theme::{Palette, Theme};
use seele_tui::ui;

const LARGURA: u16 = 80;
const ALTURA: u16 = 24;

fn main() {
    imprimir("Operação — PADRÃO: AZUL", &operando());
    imprimir("Busca no histórico, com `/`", &buscando());
    imprimir("Ajuda, com `?`", &com_ajuda());
    imprimir("Bateria interna — o enlace caiu", &na_bateria());
    imprimir("Terminal apertado — abaixo de 80×24", &operando());
}

fn imprimir(titulo: &str, app: &App) {
    let (largura, altura) = if titulo.contains("apertado") {
        (56, 14)
    } else {
        (LARGURA, ALTURA)
    };

    println!("### {titulo}\n");
    println!("```text");
    print!("{}", desenhar(app, largura, altura));
    println!("```\n");
}

fn desenhar(app: &App, largura: u16, altura: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(largura, altura)).expect("terminal");
    terminal
        .draw(|frame| ui::render(frame, app, Theme::with_palette(Palette::True)))
        .expect("desenhar");

    let buffer = terminal.backend().buffer();
    (0..altura)
        .map(|y| {
            // Um caractere largo ocupa uma célula e deixa a seguinte como
            // preenchimento. Ler todas contaria o kanji duas vezes.
            let mut linha = String::new();
            let mut x = 0_u16;
            while x < largura {
                let simbolo = buffer[(x, y)].symbol();
                linha.push_str(simbolo);
                x += u16::try_from(ui::width(simbolo).max(1)).unwrap_or(1);
            }
            format!("{}\n", linha.trim_end())
        })
        .collect()
}

/// Uma conversa em andamento, com gente falando e uma Linha aberta.
fn operando() -> App {
    let mut app = App::new();
    app.screen = Screen::PatternBlue;
    app.clock = "12:04:33".into();
    app.dogmas = vec!["Terceira Tóquio".into()];
    app.tree = vec![
        Node::Cage {
            name: "CAGE-01 CENTRAL".into(),
            open: true,
        },
        Node::Pilot(RosterEntry {
            nickname: "ayanami".into(),
            sync: 98,
            speaking: true,
            at_field: false,
            total_isolation: false,
        }),
        Node::Pilot(RosterEntry {
            nickname: "shinji".into(),
            sync: 71,
            speaking: false,
            at_field: false,
            total_isolation: false,
        }),
        Node::Pilot(RosterEntry {
            nickname: "asuka".into(),
            sync: 44,
            speaking: false,
            at_field: true,
            total_isolation: false,
        }),
        Node::Cage {
            name: "CAGE-02 TESTE".into(),
            open: false,
        },
        Node::Line {
            name: "#geral".into(),
        },
        Node::Line {
            name: "#logs".into(),
        },
    ];
    app.messages = vec![
        ChatLine {
            at: "12:01".into(),
            author: "ayanami".into(),
            body: "verificando harmônicos".into(),
            own: false,
        },
        ChatLine {
            at: "12:03".into(),
            author: "shinji".into(),
            body: "sync caiu aqui".into(),
            own: false,
        },
        ChatLine {
            at: "12:04".into(),
            author: "você".into(),
            body: "vendo — o jitter subiu junto".into(),
            own: true,
        },
    ];
    app.bar = seele_tui::app::Bar {
        sync: 94,
        rtt_ms: 38.0,
        jitter_ms: 12.0,
        loss: 0.002,
        bitrate: 32_000,
    };
    app
}

/// A mesma conversa, com uma busca em andamento.
///
/// As teclas são apertadas de verdade — `/` entra no modo Busca e cada letra
/// refaz a busca —, então o contador `[1/3]` sai da mesma contagem que o
/// terminal faz, e não de um número escrito aqui.
///
/// O que este retrato não mostra é a ocorrência acesa: o realce é cor, e isto é
/// texto. É por isso que o contador existe — `specs/05-cliente-tui.md` proíbe
/// informação que só a cor carrega, e ele é a metade que sobrevive ao
/// `NO_COLOR` e a esta página.
fn buscando() -> App {
    let mut app = operando();
    app.messages.push(ChatLine {
        at: "12:05".into(),
        author: "ayanami".into(),
        body: "o sync voltou a subir".into(),
        own: false,
    });
    app.messages.push(ChatLine {
        at: "12:06".into(),
        author: "asuka".into(),
        body: "aqui o sync nem caiu".into(),
        own: false,
    });
    for tecla in [
        Key::Char('/'),
        Key::Char('s'),
        Key::Char('y'),
        Key::Char('n'),
        Key::Char('c'),
    ] {
        drop(app.on_key(tecla));
    }
    app
}

fn com_ajuda() -> App {
    let mut app = operando();
    app.help = true;
    app
}

fn na_bateria() -> App {
    let mut app = operando();
    app.screen = Screen::InternalBattery {
        remaining: 287,
        attempts: 3,
    };
    app.alert = Some(Alert {
        text: "ENLACE PERDIDO".into(),
        blocking: false,
    });
    app
}
