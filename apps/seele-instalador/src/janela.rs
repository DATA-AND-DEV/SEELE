//! A janela, desenhada por nós.
//!
//! **Sem moldura do sistema.** `WS_POPUP` e nada mais: a barra de título é
//! pintada aqui, com a marca e a versão, que é o que o desenho pede e o que o
//! NSIS não sabia fazer. O preço é assumir o que a moldura dava de graça —
//! arrastar a janela, fechar, e o foco do teclado.
//!
//! **Tudo é desenhado, menos o campo de texto.** Um `EDIT` do Windows aceita
//! cor por `WM_CTLCOLOREDIT` — ao contrário do botão, que o tema do sistema
//! desenha e ignora quem manda — e traz cursor, seleção, teclado e IME prontos.
//! Reescrever isso à mão para uma caixa que recebe um caminho de pasta seria
//! trocar o certo pelo pior.
//!
//! **Desenho em buffer, e não direto na tela.** Cada `WM_PAINT` pinta num
//! bitmap de memória e copia uma vez. Sem isso a janela pisca a cada movimento
//! do mouse, porque cada retângulo aparece separado — e o desenho é quase todo
//! retângulo.
#![allow(unsafe_code)]

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_QUALITY, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FF_DONTCARE, FW_BOLD, FW_NORMAL, HBRUSH, HDC, HFONT,
    OUT_TT_PRECIS, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_ESCAPE, VK_RETURN, VK_SHIFT, VK_SPACE, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, LoadCursorW, LoadIconW, PostQuitMessage, RegisterClassW, SendMessageW,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    GWLP_USERDATA, HTCAPTION, HTCLIENT, IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
    WM_CTLCOLOREDIT, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_NCHITTEST, WM_PAINT, WNDCLASSW, WS_CHILD, WS_EX_APPWINDOW, WS_POPUP, WS_VISIBLE,
};

use crate::{carga, instalacao, pele, sistema};

/// O aviso que a linha da instalação manda à janela a cada arquivo.
///
/// `WM_APP` é a primeira mensagem que o Windows reserva para o programa: nada do
/// sistema a usa, e é assim que uma linha de trabalho acorda a linha da janela
/// sem tocar no estado dela por fora.
const WM_ANDOU: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// Em que passo do assistente a janela está.
///
/// Quatro, como o desenho: destino, opções, instalando, pronto.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Passo {
    Destino,
    Opcoes,
    Instalando,
    Pronto,
}

impl Passo {
    const TODOS: [Self; 4] = [Self::Destino, Self::Opcoes, Self::Instalando, Self::Pronto];

    const fn numero(self) -> &'static str {
        match self {
            Self::Destino => "01",
            Self::Opcoes => "02",
            Self::Instalando => "03",
            Self::Pronto => "04",
        }
    }

    const fn nome(self) -> &'static str {
        match self {
            Self::Destino => "DESTINO",
            Self::Opcoes => "OPÇÕES",
            Self::Instalando => "INSTALANDO",
            Self::Pronto => "PRONTO",
        }
    }

    const fn titulo(self) -> &'static str {
        match self {
            Self::Destino => "Instalar o SEELE nesta máquina",
            Self::Opcoes => "O que o instalador vai mexer",
            Self::Instalando => "Instalando",
            Self::Pronto => "O SEELE está pronto",
        }
    }

    /// O rótulo do botão que avança, que muda com o passo.
    const fn verbo(self) -> &'static str {
        match self {
            Self::Destino => "CONTINUAR",
            Self::Opcoes => "INSTALAR",
            Self::Instalando => "AGUARDE…",
            Self::Pronto => "ABRIR O SEELE",
        }
    }
}

/// Uma região que responde ao mouse.
///
/// Guardada como retângulo em pixels da janela, recalculada a cada desenho: a
/// janela não muda de tamanho, mas o dpi da tela em que ela está pode mudar
/// quando alguém a arrasta para outro monitor.
#[derive(Clone, Copy)]
struct Alvo {
    caixa: RECT,
    qual: Acao,
}

/// O que um clique faz.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Acao {
    Fechar,
    Voltar,
    Avancar,
    Alternar(usize),
    Escolher,
}

/// O que a janela sabe de si.
struct Estado {
    passo: Passo,
    /// As duas escolhas do passo 02, na ordem em que são desenhadas.
    opcoes: [bool; 2],
    alvos: Vec<Alvo>,
    sob_o_mouse: Option<Acao>,
    apertando: Option<Acao>,
    /// O campo da pasta: o único controle do Windows nesta janela.
    ///
    /// **Nativo de propósito.** Um `EDIT` traz cursor, seleção, teclado, IME e
    /// as teclas de edição que ninguém lembra de implementar — e, ao contrário
    /// do botão, ele **aceita cor** por `WM_CTLCOLOREDIT`. Desenhar um campo de
    /// texto à mão para receber um caminho de pasta seria trocar o certo pelo
    /// pior.
    campo: HWND,
    /// O pincel do fundo do campo, vivo enquanto a janela viver.
    ///
    /// `WM_CTLCOLOREDIT` exige devolver um `HBRUSH` que continue válido depois
    /// do retorno: o Windows o usa para pintar. Um pincel criado e destruído
    /// dentro do tratador é um pincel que o sistema usa depois de morto.
    fundo_do_campo: HBRUSH,
    /// O que a instalação já disse, uma linha por arquivo.
    ///
    /// Cresce até o fim e só as últimas aparecem: o log existe para uma
    /// instalação que demora **parecer** uma instalação que anda, e o que
    /// importa nele é a última linha.
    log: Vec<String>,
    /// O que a linha da instalação manda de volta, e de onde a janela lê.
    ///
    /// `Option` porque ele só existe depois de alguém apertar INSTALAR: um canal
    /// aberto desde o começo seria um canal que ninguém escreve.
    recado: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    /// O que impediu a instalação, quando impediu.
    erro: Option<String>,
    /// Onde o teclado está. `None` até alguém apertar Tab pela primeira vez.
    ///
    /// **Por ação e não por índice.** A lista de alvos é refeita a cada
    /// repintura e muda de tamanho com o passo; um índice guardado apontaria
    /// para outro botão depois de avançar, e o foco pularia sozinho.
    foco: Option<Acao>,
    dpi: i32,
    cartela: HFONT,
    rotulo: HFONT,
    corpo: HFONT,
}

impl Estado {
    /// Converte um valor do desenho (96 dpi) para o pixel desta tela.
    const fn px(&self, valor: i32) -> i32 {
        valor * self.dpi / 96
    }
}

/// As duas opções do passo 02 — as que o produto sabe cumprir.
///
/// O desenho traz quatro. As outras duas — tratar `seele://` e abrir junto com o
/// Windows — descrevem coisas que o app não faz: `seele://` só existe como nome
/// de canal interno de evento, e não há partida minimizada. Um interruptor que
/// não cumpre é pior que um interruptor ausente, então elas ficam de fora até
/// existirem.
const OPCOES: [(&str, &str); 2] = [
    (
        "Atalho na área de trabalho",
        "e uma entrada no menu Iniciar",
    ),
    (
        "Abrir a porta 8383 UDP no firewall do Windows",
        "só é necessário se você for hospedar um servidor",
    ),
];

/// Texto para o Win32: UTF-16 terminado em zero.
fn larga(texto: &str) -> Vec<u16> {
    texto.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Pinta um retângulo cheio.
fn bloco(hdc: HDC, caixa: RECT, cor: u32) {
    // SAFETY: `caixa` é um retângulo válido e o pincel é criado e destruído aqui.
    unsafe {
        let pincel: HBRUSH = CreateSolidBrush(COLORREF(cor));
        FillRect(hdc, &caixa, pincel);
        let _ = DeleteObject(pincel.into());
    }
}

/// O contorno de uma caixa: quatro riscos de 1px, sem preencher.
fn contorno(hdc: HDC, caixa: RECT, cor: u32) {
    let largura = caixa.right - caixa.left;
    let altura = caixa.bottom - caixa.top;
    risco(hdc, caixa.left, caixa.top, largura, 1, cor);
    risco(hdc, caixa.left, caixa.bottom - 1, largura, 1, cor);
    risco(hdc, caixa.left, caixa.top, 1, altura, cor);
    risco(hdc, caixa.right - 1, caixa.top, 1, altura, cor);
}

/// Uma linha de 1px — a única borda que este desenho conhece.
fn risco(hdc: HDC, x: i32, y: i32, largura: i32, altura: i32, cor: u32) {
    bloco(
        hdc,
        RECT {
            left: x,
            top: y,
            right: x + largura,
            bottom: y + altura,
        },
        cor,
    );
}

/// Escreve texto numa caixa.
fn escrever(hdc: HDC, caixa: RECT, texto: &str, fonte: HFONT, cor: u32, formato: u32) {
    let mut unidades = larga(texto);
    let mut alvo = caixa;
    // SAFETY: `unidades` vive até o fim da chamada e `alvo` é um retângulo válido.
    unsafe {
        let anterior = SelectObject(hdc, fonte.into());
        SetTextColor(hdc, COLORREF(cor));
        SetBkMode(hdc, TRANSPARENT);
        DrawTextW(
            hdc,
            &mut unidades,
            &mut alvo,
            windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(formato),
        );
        SelectObject(hdc, anterior);
    }
}

/// Cria uma face a partir do nome que a família registrou.
fn fonte(familia: &str, altura: i32, negrito: bool) -> HFONT {
    let nome = larga(familia);
    // SAFETY: `nome` é uma string terminada em zero e vive até o fim da chamada.
    unsafe {
        CreateFontW(
            -altura,
            0,
            0,
            0,
            if negrito {
                FW_BOLD.0 as i32
            } else {
                FW_NORMAL.0 as i32
            },
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            u32::from(FF_DONTCARE.0),
            PCWSTR(nome.as_ptr()),
        )
    }
}

/// Onde o campo da pasta mora, em pixels da janela.
///
/// **Uma função, dois consumidores.** Quem desenha a moldura em volta e quem
/// posiciona o controle nativo leem daqui. Duas contas separadas divergem no
/// primeiro ajuste, e o sintoma é uma moldura ao lado do campo em vez de em
/// volta dele.
fn caixa_do_campo(estado: &Estado, largura: i32, altura: i32) -> RECT {
    let px = |v: i32| estado.px(v);
    let topo = px(pele::BARRA) + 1 + px(30) + 1;
    let corpo_x = px(pele::LOMBADA) + px(28);
    let _ = altura;
    RECT {
        left: corpo_x,
        top: topo + px(124),
        right: largura - px(28) - px(96),
        bottom: topo + px(148),
    }
}

/// Desenha a janela inteira e devolve os alvos de clique que ela criou.
///
/// Os alvos saem daqui, e não de uma tabela à parte, porque quem sabe onde um
/// botão ficou é quem o desenhou. Duas listas — uma para pintar, outra para
/// clicar — divergem no primeiro ajuste de layout, e o sintoma é um botão que
/// responde ao lado de onde aparece.
fn desenhar(estado: &Estado, hdc: HDC, largura: i32, altura: i32) -> Vec<Alvo> {
    let mut alvos = Vec::new();
    let px = |v: i32| estado.px(v);

    // O fundo, e a moldura de 1px que substitui a do sistema.
    bloco(
        hdc,
        RECT {
            left: 0,
            top: 0,
            right: largura,
            bottom: altura,
        },
        pele::NEGRO,
    );
    risco(hdc, 0, 0, largura, 1, pele::LINHA_FORTE);
    risco(hdc, 0, altura - 1, largura, 1, pele::LINHA_FORTE);
    risco(hdc, 0, 0, 1, altura, pele::LINHA_FORTE);
    risco(hdc, largura - 1, 0, 1, altura, pele::LINHA_FORTE);

    // ---- a barra de título, que é nossa ----
    let barra = px(pele::BARRA);
    bloco(
        hdc,
        RECT {
            left: 1,
            top: 1,
            right: largura - 1,
            bottom: barra,
        },
        pele::PAINEL,
    );
    risco(hdc, 0, barra, largura, 1, pele::LINHA);
    escrever(
        hdc,
        RECT {
            left: px(14),
            top: 1,
            right: largura - px(90),
            bottom: barra,
        },
        "INSTALAR O SEELE",
        estado.cartela,
        pele::OSSO,
        DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0,
    );
    escrever(
        hdc,
        RECT {
            left: px(160),
            top: 1,
            right: largura - px(50),
            bottom: barra,
        },
        concat!(env!("SEELE_VERSAO"), " · WINDOWS 64 BITS"),
        estado.rotulo,
        pele::ROTULO,
        DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0,
    );

    let fechar = RECT {
        left: largura - px(40),
        top: 1,
        right: largura - 1,
        bottom: barra,
    };
    if estado.sob_o_mouse == Some(Acao::Fechar) {
        bloco(hdc, fechar, pele::LINHA);
    }
    escrever(
        hdc,
        fechar,
        "×",
        estado.corpo,
        pele::OSSO,
        DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0 | windows::Win32::Graphics::Gdi::DT_CENTER.0,
    );
    alvos.push(Alvo {
        caixa: fechar,
        qual: Acao::Fechar,
    });

    // ---- os quatro passos ----
    let fita = barra + 1;
    let altura_da_fita = px(30);
    let largura_do_passo = (largura - 2) / 4;
    for (i, passo) in Passo::TODOS.iter().enumerate() {
        let x = 1 + largura_do_passo * i32::try_from(i).unwrap_or(0);
        let caixa = RECT {
            left: x,
            top: fita,
            right: x + largura_do_passo,
            bottom: fita + altura_da_fita,
        };
        let atual = *passo == estado.passo;
        bloco(hdc, caixa, if atual { pele::LARANJA } else { pele::PAINEL });
        escrever(
            hdc,
            RECT {
                left: caixa.left + px(12),
                ..caixa
            },
            &format!("{}  {}", passo.numero(), passo.nome()),
            estado.rotulo,
            if atual { pele::NEGRO } else { pele::ROTULO },
            DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0,
        );
        risco(hdc, caixa.right - 1, fita, 1, altura_da_fita, pele::LINHA);
    }
    risco(hdc, 0, fita + altura_da_fita, largura, 1, pele::LINHA);

    // ---- a lombada ----
    let topo = fita + altura_da_fita + 1;
    let base = altura - px(pele::RODAPE);
    let lombada = px(pele::LOMBADA);
    bloco(
        hdc,
        RECT {
            left: 1,
            top: topo,
            right: lombada,
            bottom: base,
        },
        pele::PAINEL,
    );
    risco(hdc, lombada, topo, 1, base - topo, pele::LINHA);
    marca(hdc, px(20), topo + px(20), estado);
    escrever(
        hdc,
        RECT {
            left: px(20),
            top: topo + px(76),
            right: lombada,
            bottom: topo + px(110),
        },
        "SEELE",
        estado.cartela,
        pele::LARANJA,
        DT_SINGLELINE.0 | DT_NOPREFIX.0,
    );
    escrever(
        hdc,
        RECT {
            left: px(20),
            top: topo + px(112),
            right: lombada - px(16),
            bottom: base - px(60),
        },
        "VOZ, VÍDEO E TEXTO AUTO-HOSPEDADOS",
        estado.rotulo,
        pele::ROTULO,
        DT_WORDBREAK.0 | DT_NOPREFIX.0,
    );
    escrever(
        hdc,
        RECT {
            left: px(20),
            top: base - px(56),
            right: lombada - px(16),
            bottom: base - px(8),
        },
        "O mesmo executável é o aplicativo e o servidor.",
        estado.corpo,
        pele::ROTULO,
        DT_WORDBREAK.0 | DT_NOPREFIX.0,
    );

    // ---- o corpo ----
    let corpo_x = lombada + px(28);
    let corpo_direita = largura - px(28);
    escrever(
        hdc,
        RECT {
            left: corpo_x,
            top: topo + px(24),
            right: corpo_direita,
            bottom: topo + px(58),
        },
        estado.passo.titulo(),
        estado.cartela,
        pele::OSSO,
        DT_SINGLELINE.0 | DT_NOPREFIX.0,
    );

    if estado.passo == Passo::Destino {
        escrever(
            hdc,
            RECT {
                left: corpo_x,
                top: topo + px(64),
                right: corpo_direita,
                bottom: topo + px(96),
            },
            "Nada é enviado para fora durante a instalação. O SEELE não cria conta: sua identidade é uma chave gerada aqui, no primeiro uso.",
            estado.corpo,
            pele::ROTULO,
            DT_WORDBREAK.0 | DT_NOPREFIX.0,
        );
        escrever(
            hdc,
            RECT {
                left: corpo_x,
                top: topo + px(108),
                right: corpo_direita,
                bottom: topo + px(120),
            },
            "PASTA DE DESTINO",
            estado.rotulo,
            pele::ROTULO,
            DT_SINGLELINE.0 | DT_NOPREFIX.0,
        );

        // O campo é um filho de verdade, então quem o move é o Windows e não o
        // desenho. Aqui só se diz onde ele deve estar; a moldura é nossa, porque
        // um `EDIT` com borda do sistema traz o cinza do tema junto.
        let campo = caixa_do_campo(estado, largura, altura);
        contorno(hdc, campo, pele::LINHA_FORTE);

        let escolher = RECT {
            left: campo.right + px(8),
            top: campo.top,
            right: corpo_direita,
            bottom: campo.bottom,
        };
        contorno(hdc, escolher, pele::LINHA_FORTE);
        escrever(
            hdc,
            escolher,
            "ESCOLHER…",
            estado.rotulo,
            pele::OSSO,
            DT_SINGLELINE.0
                | DT_VCENTER.0
                | DT_NOPREFIX.0
                | windows::Win32::Graphics::Gdi::DT_CENTER.0,
        );
        alvos.push(Alvo {
            caixa: escolher,
            qual: Acao::Escolher,
        });

        escrever(
            hdc,
            RECT {
                left: corpo_x,
                top: topo + px(160),
                right: corpo_direita,
                bottom: topo + px(176),
            },
            &if carga::existe() {
                // O número honesto é o comprimido — o instalado só se saberia
                // descompactando, e descompactar duas vezes para escrever uma
                // linha é pagar a instalação inteira por um rótulo.
                format!(
                    "ESTE INSTALADOR CARREGA {} MiB COMPRIMIDOS",
                    carga::comprimida() / (1024 * 1024)
                )
            } else {
                "SEM CARGA: ESTE BUILD DESENHA A JANELA E NÃO INSTALA".to_owned()
            },
            estado.rotulo,
            if carga::existe() {
                pele::ROTULO
            } else {
                pele::LARANJA
            },
            DT_SINGLELINE.0 | DT_NOPREFIX.0,
        );
        escrever(
            hdc,
            RECT {
                left: corpo_x,
                top: topo + px(180),
                right: corpo_direita,
                bottom: topo + px(210),
            },
            "Ao continuar você aceita a licença do projeto, que acompanha o executável.",
            estado.corpo,
            pele::ROTULO,
            DT_WORDBREAK.0 | DT_NOPREFIX.0,
        );
    }

    if estado.passo == Passo::Instalando {
        let (frase, cor_da_frase) = estado.erro.as_ref().map_or_else(
            || {
                (
                    "Escrevendo os arquivos. Nada é baixado: tudo já está neste instalador."
                        .to_owned(),
                    pele::ROTULO,
                )
            },
            |motivo| (motivo.clone(), pele::LARANJA),
        );
        escrever(
            hdc,
            RECT {
                left: corpo_x,
                top: topo + px(64),
                right: corpo_direita,
                bottom: topo + px(112),
            },
            &frase,
            estado.corpo,
            cor_da_frase,
            DT_WORDBREAK.0 | DT_NOPREFIX.0,
        );

        // O log, e só o fim dele: quem olha uma instalação quer saber onde ela
        // está, não o que já passou.
        let cabem = 6_usize;
        let comeco = estado.log.len().saturating_sub(cabem);
        let mut y = topo + px(124);
        for linha in estado.log.iter().skip(comeco) {
            escrever(
                hdc,
                RECT {
                    left: corpo_x,
                    top: y,
                    right: corpo_direita,
                    bottom: y + px(16),
                },
                &format!("· {linha}"),
                estado.corpo,
                pele::ROTULO,
                DT_SINGLELINE.0 | DT_NOPREFIX.0 | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS.0,
            );
            y += px(16);
        }
    }

    if estado.passo == Passo::Pronto {
        escrever(
            hdc,
            RECT {
                left: corpo_x,
                top: topo + px(64),
                right: corpo_direita,
                bottom: topo + px(104),
            },
            "Na primeira abertura o app gera sua chave e pede um apelido. Depois disso você escolhe entre entrar num servidor ou hospedar um aqui.",
            estado.corpo,
            pele::ROTULO,
            DT_WORDBREAK.0 | DT_NOPREFIX.0,
        );
        for (i, (rotulo, valor)) in [
            ("VERSÃO", env!("SEELE_VERSAO").to_owned()),
            ("PASTA", pasta_escolhida(estado)),
            ("ARQUIVOS", format!("{} escritos", estado.log.len())),
        ]
        .iter()
        .enumerate()
        {
            let y = topo + px(116) + px(20) * i32::try_from(i).unwrap_or(0);
            escrever(
                hdc,
                RECT {
                    left: corpo_x,
                    top: y,
                    right: corpo_x + px(80),
                    bottom: y + px(16),
                },
                rotulo,
                estado.rotulo,
                pele::ROTULO,
                DT_SINGLELINE.0 | DT_NOPREFIX.0,
            );
            escrever(
                hdc,
                RECT {
                    left: corpo_x + px(88),
                    top: y,
                    right: corpo_direita,
                    bottom: y + px(16),
                },
                valor,
                estado.corpo,
                pele::OSSO,
                DT_SINGLELINE.0 | DT_NOPREFIX.0 | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS.0,
            );
        }
    }

    if estado.passo == Passo::Opcoes {
        let mut y = topo + px(72);
        for (i, (nome, nota)) in OPCOES.iter().enumerate() {
            let caixa = RECT {
                left: corpo_x,
                top: y,
                right: corpo_direita,
                bottom: y + px(44),
            };
            let marcada = estado.opcoes.get(i).copied().unwrap_or(false);
            let quadro = RECT {
                left: corpo_x,
                top: y + px(2),
                right: corpo_x + px(16),
                bottom: y + px(18),
            };
            bloco(
                hdc,
                quadro,
                if marcada { pele::LARANJA } else { pele::NEGRO },
            );
            if !marcada {
                contorno(hdc, quadro, pele::LINHA_FORTE);
            } else {
                escrever(
                    hdc,
                    quadro,
                    "×",
                    estado.corpo,
                    pele::NEGRO,
                    DT_SINGLELINE.0
                        | DT_VCENTER.0
                        | DT_NOPREFIX.0
                        | windows::Win32::Graphics::Gdi::DT_CENTER.0,
                );
            }
            escrever(
                hdc,
                RECT {
                    left: corpo_x + px(28),
                    top: y,
                    right: corpo_direita,
                    bottom: y + px(18),
                },
                nome,
                estado.corpo,
                pele::OSSO,
                DT_SINGLELINE.0 | DT_NOPREFIX.0,
            );
            escrever(
                hdc,
                RECT {
                    left: corpo_x + px(28),
                    top: y + px(18),
                    right: corpo_direita,
                    bottom: y + px(40),
                },
                nota,
                estado.corpo,
                pele::ROTULO,
                DT_WORDBREAK.0 | DT_NOPREFIX.0,
            );
            alvos.push(Alvo {
                caixa,
                qual: Acao::Alternar(i),
            });
            y += px(52);
        }
    }

    // ---- o rodapé ----
    bloco(
        hdc,
        RECT {
            left: 1,
            top: base,
            right: largura - 1,
            bottom: altura - 1,
        },
        pele::PAINEL,
    );
    risco(hdc, 0, base, largura, 1, pele::LINHA);
    escrever(
        hdc,
        RECT {
            left: px(28),
            top: base,
            right: largura - px(240),
            bottom: altura,
        },
        &format!(
            "PASSO {} DE 04 · {}",
            estado.passo.numero(),
            estado.passo.nome()
        ),
        estado.rotulo,
        pele::ROTULO,
        DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0,
    );

    let avancar = RECT {
        left: largura - px(150),
        top: base + px(14),
        right: largura - px(28),
        bottom: altura - px(14),
    };
    let aceso = estado.apertando == Some(Acao::Avancar);
    bloco(hdc, avancar, if aceso { pele::OSSO } else { pele::LARANJA });
    escrever(
        hdc,
        avancar,
        estado.passo.verbo(),
        estado.rotulo,
        pele::NEGRO,
        DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0 | windows::Win32::Graphics::Gdi::DT_CENTER.0,
    );
    alvos.push(Alvo {
        caixa: avancar,
        qual: Acao::Avancar,
    });

    let voltar = RECT {
        left: avancar.left - px(104),
        top: avancar.top,
        right: avancar.left - px(10),
        bottom: avancar.bottom,
    };
    let primeiro = estado.passo == Passo::Destino;
    let cor_da_borda = if primeiro {
        pele::LINHA
    } else {
        pele::LINHA_FORTE
    };
    risco(
        hdc,
        voltar.left,
        voltar.top,
        voltar.right - voltar.left,
        1,
        cor_da_borda,
    );
    risco(
        hdc,
        voltar.left,
        voltar.bottom - 1,
        voltar.right - voltar.left,
        1,
        cor_da_borda,
    );
    risco(
        hdc,
        voltar.left,
        voltar.top,
        1,
        voltar.bottom - voltar.top,
        cor_da_borda,
    );
    risco(
        hdc,
        voltar.right - 1,
        voltar.top,
        1,
        voltar.bottom - voltar.top,
        cor_da_borda,
    );
    escrever(
        hdc,
        voltar,
        "VOLTAR",
        estado.rotulo,
        if primeiro {
            pele::LINHA_FORTE
        } else {
            pele::ROTULO
        },
        DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0 | windows::Win32::Graphics::Gdi::DT_CENTER.0,
    );
    if !primeiro {
        alvos.push(Alvo {
            caixa: voltar,
            qual: Acao::Voltar,
        });
    }

    // **O anel de foco sai do mesmo retângulo que recebe o clique.**
    //
    // Desenhado por último, sobre tudo, e a partir da lista de alvos — não de
    // uma cópia das coordenadas. É o que garante que o anel não possa aparecer
    // num lugar e o clique responder noutro; e um Tab que move um foco invisível
    // é pior que não ter Tab, porque promete navegação e esconde onde se está.
    if let Some(onde) = estado.foco {
        if let Some(alvo) = alvos.iter().find(|alvo| alvo.qual == onde) {
            let folga = px(2);
            contorno(
                hdc,
                RECT {
                    left: alvo.caixa.left - folga,
                    top: alvo.caixa.top - folga,
                    right: alvo.caixa.right + folga,
                    bottom: alvo.caixa.bottom + folga,
                },
                pele::OSSO,
            );
        }
    }

    alvos
}

/// Põe o campo da pasta no lugar, e o esconde fora do passo 01.
///
/// Chamado a cada repintura, e não só na troca de passo: é uma chamada barata e
/// é o que mantém o campo colado à moldura quando a janela muda de monitor —
/// dpi novo, medidas novas, e o campo teria ficado onde estava.
fn arrumar_campo(estado: &Estado, largura: i32, altura: i32) {
    let caixa = caixa_do_campo(estado, largura, altura);
    let no_passo = estado.passo == Passo::Destino;
    // SAFETY: `campo` é o `EDIT` criado em `abrir`, filho desta janela.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::MoveWindow(
            estado.campo,
            caixa.left + 1,
            caixa.top + 1,
            caixa.right - caixa.left - 2,
            caixa.bottom - caixa.top - 2,
            true,
        );
        let _ = ShowWindow(
            estado.campo,
            if no_passo {
                SW_SHOW
            } else {
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE
            },
        );
    }
}

/// Qual alvo está sob um ponto, se algum.
fn alvo_em(alvos: &[Alvo], x: i32, y: i32) -> Option<Acao> {
    alvos
        .iter()
        .find(|alvo| {
            x >= alvo.caixa.left
                && x < alvo.caixa.right
                && y >= alvo.caixa.top
                && y < alvo.caixa.bottom
        })
        .map(|alvo| alvo.qual)
}

/// O estado da janela, pendurado nela.
///
/// `GWLP_USERDATA` é onde o Win32 deixa um ponteiro por janela. O estado é
/// criado antes da janela e vazado de propósito no fim do programa: um
/// instalador que fecha é um processo que termina, e devolver a memória ao
/// sistema um instante antes de o sistema tomá-la de volta não paga o risco de
/// uma mensagem tardia encontrar o ponteiro já solto.
fn estado_de(janela: HWND) -> Option<&'static mut Estado> {
    // SAFETY: o ponteiro guardado é sempre o do `Box` criado em `abrir`, e ele
    // vive enquanto o processo viver.
    unsafe {
        let bruto = GetWindowLongPtrW(janela, GWLP_USERDATA) as *mut Estado;
        if bruto.is_null() {
            None
        } else {
            Some(&mut *bruto)
        }
    }
}

/// Manda redesenhar a janela inteira.
fn repintar(janela: HWND) {
    // SAFETY: `janela` é válida enquanto esta função é chamada de dentro do
    // procedimento dela.
    unsafe {
        let _ = InvalidateRect(Some(janela), None, false);
    }
}

extern "system" fn procedimento(janela: HWND, mensagem: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match mensagem {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: par `BeginPaint`/`EndPaint` completo, e todo objeto de GDI
            // criado aqui é destruído antes do retorno.
            unsafe {
                let hdc = BeginPaint(janela, &mut ps);
                let mut caixa = RECT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(janela, &mut caixa);
                let (largura, altura) = (caixa.right, caixa.bottom);

                // O buffer: pintar direto na tela faz a janela piscar a cada
                // movimento do mouse, porque cada retângulo aparece sozinho.
                let memoria = CreateCompatibleDC(Some(hdc));
                let bitmap = CreateCompatibleBitmap(hdc, largura, altura);
                let anterior = SelectObject(memoria, bitmap.into());

                if let Some(estado) = estado_de(janela) {
                    estado.alvos = desenhar(estado, memoria, largura, altura);
                    arrumar_campo(estado, largura, altura);
                }

                let _ = BitBlt(hdc, 0, 0, largura, altura, Some(memoria), 0, 0, SRCCOPY);
                SelectObject(memoria, anterior);
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(memoria);
                let _ = EndPaint(janela, &ps);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = (posicao_x(l), posicao_y(l));
            if let Some(estado) = estado_de(janela) {
                let agora = alvo_em(&estado.alvos, x, y);
                if agora != estado.sob_o_mouse {
                    estado.sob_o_mouse = agora;
                    repintar(janela);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = (posicao_x(l), posicao_y(l));
            if let Some(estado) = estado_de(janela) {
                estado.apertando = alvo_em(&estado.alvos, x, y);
                if estado.apertando.is_some() {
                    // SAFETY: a captura é solta no `WM_LBUTTONUP` logo abaixo.
                    unsafe {
                        SetCapture(janela);
                    }
                    repintar(janela);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = (posicao_x(l), posicao_y(l));
            // SAFETY: solta a captura tomada no botão apertado.
            unsafe {
                let _ = ReleaseCapture();
            }
            if let Some(estado) = estado_de(janela) {
                let sobre = alvo_em(&estado.alvos, x, y);
                let apertado = estado.apertando.take();
                // Só age quando soltou **em cima do mesmo alvo** em que apertou:
                // apertar e arrastar para fora é como se desiste de um clique, e
                // um instalador que instala no arrependimento é um instalador
                // que ninguém confia.
                if sobre.is_some() && sobre == apertado {
                    agir(janela, estado, sobre);
                }
                repintar(janela);
            }
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // A barra de título é nossa, então arrastar a janela também é.
            // `HTCAPTION` faz o próprio Windows mover a janela — reimplementar o
            // arrasto à mão perderia o encaixe nas bordas e o atalho de teclado.
            // SAFETY: chamada padrão, sem ponteiro nenhum.
            let padrao = unsafe { DefWindowProcW(janela, mensagem, w, l) };
            if padrao.0 != HTCLIENT as isize {
                return padrao;
            }
            let mut ponto = POINT {
                x: posicao_x(l),
                y: posicao_y(l),
            };
            // SAFETY: `ponto` é válido e a janela existe.
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(janela, &mut ponto);
            }
            if let Some(estado) = estado_de(janela) {
                if alvo_em(&estado.alvos, ponto.x, ponto.y).is_none()
                    && ponto.y < estado.px(pele::BARRA)
                {
                    return LRESULT(HTCAPTION as isize);
                }
            }
            padrao
        }
        WM_ANDOU => {
            // Esvazia o canal de uma vez, e não uma linha por acordada: a linha
            // da instalação manda mais depressa do que a janela repinta, e ler
            // uma por vez faria o log ficar para trás do que já foi escrito em
            // disco — um progresso que mente para menos.
            if let Some(estado) = estado_de(janela) {
                let mut acabou = false;
                if let Some(canal) = estado.recado.as_ref() {
                    while let Ok(recado) = canal.try_recv() {
                        match recado {
                            Ok(arquivo) => estado.log.push(arquivo),
                            Err(motivo) => {
                                estado.erro = Some(motivo);
                                acabou = true;
                            }
                        }
                    }
                }
                // Sem erro e sem canal vivo, acabou bem. O `Sender` morre com a
                // linha de trabalho, e é isso que o `try_recv` desconectado diz.
                if !acabou
                    && estado.recado.as_ref().is_some_and(|canal| {
                        matches!(
                            canal.try_recv(),
                            Err(std::sync::mpsc::TryRecvError::Disconnected)
                        )
                    })
                {
                    estado.recado = None;
                    estado.passo = Passo::Pronto;
                }
                repintar(janela);
            }
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => {
            // O `EDIT` é o único controle do Windows nesta janela, e o único que
            // aceita cor. O pincel devolvido tem de continuar válido **depois**
            // do retorno — o sistema o usa para pintar —, por isso ele vive no
            // estado e não é criado aqui.
            if let Some(estado) = estado_de(janela) {
                // SAFETY: `w` é o HDC que o sistema mandou, válido nesta chamada.
                unsafe {
                    SetTextColor(HDC(w.0 as *mut core::ffi::c_void), COLORREF(pele::OSSO));
                    windows::Win32::Graphics::Gdi::SetBkColor(
                        HDC(w.0 as *mut core::ffi::c_void),
                        COLORREF(pele::NEGRO),
                    );
                }
                return LRESULT(estado.fundo_do_campo.0 as isize);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let tecla = u16::try_from(w.0).unwrap_or(0);
            if tecla == VK_ESCAPE.0 {
                // SAFETY: fecha a janela; o `WM_DESTROY` encerra o laço.
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(janela);
                }
                return LRESULT(0);
            }
            if let Some(estado) = estado_de(janela) {
                if tecla == VK_TAB.0 {
                    // SAFETY: leitura do estado de uma tecla, sem ponteiro.
                    let com_shift = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0;
                    estado.foco = vizinho(&estado.alvos, estado.foco, com_shift);
                    repintar(janela);
                } else if tecla == VK_RETURN.0 || tecla == VK_SPACE.0 {
                    let alvo = estado.foco;
                    if alvo.is_some() {
                        agir(janela, estado, alvo);
                        repintar(janela);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: encerra o laço de mensagens.
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        // SAFETY: o resto é do sistema.
        _ => unsafe { DefWindowProcW(janela, mensagem, w, l) },
    }
}

/// O alvo seguinte (ou anterior) na ordem em que a tela é lida.
///
/// **Ordenado por posição, e não pela ordem em que foi desenhado.** As duas
/// coincidem hoje por acaso; no dia em que alguém mover um botão no desenho sem
/// mover a linha que o pinta, o Tab passaria a pular na ordem do código — que
/// ninguém vê — em vez da ordem da tela, que é a única que quem usa conhece.
fn vizinho(alvos: &[Alvo], atual: Option<Acao>, para_tras: bool) -> Option<Acao> {
    let mut ordenados: Vec<&Alvo> = alvos.iter().collect();
    ordenados.sort_by_key(|alvo| (alvo.caixa.top, alvo.caixa.left));
    if ordenados.is_empty() {
        return None;
    }

    let posicao = atual.and_then(|acao| ordenados.iter().position(|alvo| alvo.qual == acao));
    let quantos = ordenados.len();
    let proxima = match (posicao, para_tras) {
        (None, false) => 0,
        (None, true) => quantos - 1,
        (Some(i), false) => (i + 1) % quantos,
        (Some(i), true) => (i + quantos - 1) % quantos,
    };
    ordenados.get(proxima).map(|alvo| alvo.qual)
}

/// O que um clique consumado faz.
fn agir(janela: HWND, estado: &mut Estado, acao: Option<Acao>) {
    match acao {
        Some(Acao::Fechar) => {
            // SAFETY: fecha a janela, o que leva ao `WM_DESTROY`.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(janela);
            }
        }
        Some(Acao::Voltar) => {
            // Do 03 e do 04 não se volta: o 03 está escrevendo em disco, e o 04
            // é depois de ter escrito. Um VOLTAR que reabre as opções depois de
            // a instalação ter acontecido é um botão que promete desfazer o que
            // não desfaz.
            if matches!(estado.passo, Passo::Opcoes) {
                estado.passo = Passo::Destino;
            }
        }
        Some(Acao::Avancar) => match estado.passo {
            Passo::Destino => estado.passo = Passo::Opcoes,
            Passo::Opcoes => {
                estado.passo = Passo::Instalando;
                comecar_a_instalar(janela, estado);
            }
            // Enquanto instala, avançar não faz nada: o passo 04 chega quando a
            // linha da instalação disser que acabou, e não quando alguém apertar.
            Passo::Instalando => {}
            Passo::Pronto => {
                // **Abrir antes de fechar, e nesta ordem.**
                //
                // O botão diz ABRIR O SEELE. A primeira versão disto só fechava
                // a janela, com um comentário afirmando que o produto abria no
                // lugar dela — a intenção escrita e não implementada. Quem
                // apertava via o instalador sumir e mais nada.
                //
                // Fechar primeiro também não serviria: o `DestroyWindow` leva ao
                // fim do laço de mensagens e à saída do processo, e o que vier
                // depois pode não chegar a rodar.
                instalacao::abrir_o_produto(&std::path::PathBuf::from(pasta_escolhida(estado)));
                // SAFETY: fecha a janela, agora com o produto já lançado.
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(janela);
                }
            }
        },
        Some(Acao::Escolher) => {
            if let Some(escolhida) = perguntar_a_pasta(janela, estado) {
                let texto = larga(&escolhida);
                // SAFETY: `texto` é uma string terminada em zero viva até o fim
                // da chamada, e `campo` é o `EDIT` criado em `abrir`.
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                        estado.campo,
                        PCWSTR(texto.as_ptr()),
                    );
                }
            }
        }
        Some(Acao::Alternar(qual)) => {
            if let Some(marca) = estado.opcoes.get_mut(qual) {
                *marca = !*marca;
            }
        }
        None => {}
    }
}

/// O x de um `LPARAM` de mouse. O Win32 empacota os dois numa palavra dupla.
const fn posicao_x(l: LPARAM) -> i32 {
    (l.0 & 0xFFFF) as i16 as i32
}

/// O y do mesmo `LPARAM`.
const fn posicao_y(l: LPARAM) -> i32 {
    ((l.0 >> 16) & 0xFFFF) as i16 as i32
}

/// Registra uma face embarcada e devolve `false` se o Windows a recusar.
///
/// **Da memória, nunca instalada.** `AddFontMemResourceEx` deixa a face
/// disponível só para este processo e some quando ele termina: quem roda o
/// instalador não fica com fonte nova no sistema, e quem desiste no primeiro
/// passo não deixa rastro.
fn registrar_face(bytes: &'static [u8]) -> bool {
    let mut quantas: u32 = 0;
    // SAFETY: `bytes` é `'static` — vem de `include_bytes!` — e continua válido
    // enquanto o processo viver, que é o que esta função exige.
    let alça = unsafe {
        windows::Win32::Graphics::Gdi::AddFontMemResourceEx(
            bytes.as_ptr().cast(),
            u32::try_from(bytes.len()).unwrap_or(0),
            None,
            &raw mut quantas,
        )
    };
    !alça.is_invalid() && quantas > 0
}

/// Abre a janela e roda até alguém fechá-la.
pub(crate) fn abrir() -> Result<(), String> {
    // SAFETY: chamadas de inicialização do processo, antes de qualquer janela.
    unsafe {
        // Por monitor, e na segunda versão: sem isto o Windows estica a janela
        // numa tela de 150% e o desenho sai borrado — que é justamente o que
        // este instalador existe para não ser.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // As faces antes da janela: uma face que não registrou faz o `CreateFontW`
    // cair na substituta do sistema **em silêncio**, e o instalador aparece com
    // a tipografia errada sem nada dizer.
    let faces = [
        ("Saira Condensed 700", pele::SAIRA_700),
        ("Saira Condensed 500", pele::SAIRA_500),
        ("IBM Plex Mono 400", pele::PLEX_400),
    ];
    for (nome, bytes) in faces {
        if !registrar_face(bytes) {
            return Err(format!(
                "o Windows recusou a face embarcada «{nome}». Sem ela o \
                 instalador apareceria na tipografia do sistema, que não é a do \
                 produto."
            ));
        }
    }

    // SAFETY: registro de classe e criação de janela, com todos os ponteiros
    // vivos até depois do uso.
    let janela = unsafe {
        let instancia: HINSTANCE = GetModuleHandleW(None)
            .map_err(|erro| format!("não achei o módulo deste processo: {erro}"))?
            .into();
        let classe = larga("SeeleInstalador");
        let cursor = LoadCursorW(None, IDC_ARROW)
            .map_err(|erro| format!("não carreguei o cursor: {erro}"))?;

        // O mesmo ícone que o Explorer mostra, agora na barra de tarefas e no
        // Alt-Tab. São dois caminhos diferentes para a mesma imagem: o do
        // Explorer é o recurso embutido pelo `build.rs`; este é o `HICON` que a
        // classe da janela carrega, e sem ele a janela aparece com o ícone
        // genérico mesmo com o `.exe` já ilustrado.
        //
        // `PCWSTR(1 as *const u16)` é o `MAKEINTRESOURCE(1)` do C: o `1` do
        // `icone.rc`, passado como número e não como nome.
        // `without_provenance(1)` é o `MAKEINTRESOURCE(1)` do C dito em Rust: um
        // ponteiro cujo **endereço** é o número do recurso, e que ninguém
        // desreferencia — o Windows testa se o valor cabe em 16 bits e o trata
        // como id. O clippy o via como ponteiro pendurado, e a sugestão dele
        // (`ptr::dangling`) daria um endereço qualquer, que é o único jeito de
        // isto ficar errado de verdade.
        let numero_do_icone: *const u16 = std::ptr::without_provenance(1);
        let icone = LoadIconW(Some(instancia), PCWSTR(numero_do_icone)).unwrap_or_default();

        let registro = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(procedimento),
            hInstance: instancia,
            lpszClassName: PCWSTR(classe.as_ptr()),
            hCursor: cursor,
            hIcon: icone,
            ..Default::default()
        };
        if RegisterClassW(&registro) == 0 {
            return Err("não registrei a classe da janela".to_owned());
        }

        // O dpi só se sabe depois de a janela existir, e o tamanho depende dele.
        // Então ela nasce no tamanho de 96 dpi e é ajustada logo abaixo.
        let janela = CreateWindowExW(
            WS_EX_APPWINDOW,
            PCWSTR(classe.as_ptr()),
            PCWSTR(larga("Instalar o SEELE").as_ptr()),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            pele::LARGURA,
            pele::ALTURA,
            None,
            None,
            Some(instancia),
            None,
        )
        .map_err(|erro| format!("não criei a janela: {erro}"))?;

        let dpi = i32::try_from(GetDpiForWindow(janela)).unwrap_or(96);
        let largura = pele::LARGURA * dpi / 96;
        let altura = pele::ALTURA * dpi / 96;
        let x = (GetSystemMetrics(SM_CXSCREEN) - largura) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - altura) / 2;
        let _ = windows::Win32::UI::WindowsAndMessaging::MoveWindow(
            janela, x, y, largura, altura, true,
        );

        // O campo da pasta: filho da janela, criado uma vez e mostrado só no
        // passo 01. `ES_AUTOHSCROLL` porque um caminho longo tem de rolar dentro
        // dele em vez de sumir.
        let classe_do_campo = larga("EDIT");
        let proposta = larga(&instalacao::pasta_padrao());
        let campo = CreateWindowExW(
            Default::default(),
            PCWSTR(classe_do_campo.as_ptr()),
            PCWSTR(proposta.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                    (windows::Win32::UI::WindowsAndMessaging::ES_AUTOHSCROLL
                        | windows::Win32::UI::WindowsAndMessaging::ES_LEFT)
                        as u32,
                ),
            0,
            0,
            10,
            10,
            Some(janela),
            None,
            Some(instancia),
            None,
        )
        .map_err(|erro| format!("não criei o campo da pasta: {erro}"))?;

        let estado = Box::new(Estado {
            passo: Passo::Destino,
            campo,
            fundo_do_campo: CreateSolidBrush(COLORREF(pele::NEGRO)),
            // O atalho nasce marcado e a porta nasce desmarcada, como o desenho
            // os mostra: um atalho é conveniência e uma porta aberta é decisão.
            opcoes: [true, false],
            alvos: Vec::new(),
            sob_o_mouse: None,
            apertando: None,
            log: Vec::new(),
            recado: None,
            erro: None,
            foco: None,
            dpi,
            cartela: fonte("Saira Condensed", 22 * dpi / 96, true),
            rotulo: fonte("Saira Condensed", 11 * dpi / 96, false),
            corpo: fonte("IBM Plex Mono", 12 * dpi / 96, false),
        });
        // A fonte do campo é a mesma do corpo. Sem isto ele nasce na fonte de
        // sistema, e o único texto que quem instala pode editar seria o único
        // que não é do produto.
        SendMessageW(
            campo,
            windows::Win32::UI::WindowsAndMessaging::WM_SETFONT,
            Some(WPARAM(estado.corpo.0 as usize)),
            Some(LPARAM(1)),
        );
        SetWindowLongPtrW(janela, GWLP_USERDATA, Box::into_raw(estado) as isize);

        let _ = ShowWindow(janela, SW_SHOW);
        janela
    };

    // O laço. `GetMessageW` devolve 0 no `WM_QUIT`, que é o que `PostQuitMessage`
    // enfileira quando a janela morre.
    let mut mensagem = MSG::default();
    // SAFETY: laço de mensagens padrão, com `mensagem` viva em todas as chamadas.
    unsafe {
        while GetMessageW(&mut mensagem, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&mensagem);
            DispatchMessageW(&mensagem);
        }
    }
    let _ = janela;
    Ok(())
}

/// Desenha a marca embarcada, direto do `.bmp`, sem criar objeto de GDI.
///
/// `SetDIBitsToDevice` pinta os bytes como estão — o cabeçalho do arquivo já diz
/// tamanho, profundidade e ordem das linhas. Criar um `HBITMAP` para depois
/// destruí-lo seria dois objetos a mais para vazar num caminho que roda a cada
/// repintura.
///
/// O fundo do arquivo já é `--seele-negro-painel`, que é a cor da lombada: por
/// isso ele encaixa sem transparência nenhuma. Ver
/// `empacotar/marca-do-instalador.py`.
fn marca(hdc: HDC, x: i32, y: i32, estado: &Estado) {
    // 14 bytes de cabeçalho de arquivo, e o `BITMAPINFOHEADER` logo depois. O
    // deslocamento dos pixels vem do próprio cabeçalho, e não de uma conta:
    // um `.bmp` com paleta põe os pixels mais adiante.
    let Some(cabecalho) = pele::MARCA.get(14..) else {
        return;
    };
    let Some(inicio) = pele::MARCA.get(10..14) else {
        return;
    };
    let deslocamento = u32::from_le_bytes([
        *inicio.first().unwrap_or(&0),
        *inicio.get(1).unwrap_or(&0),
        *inicio.get(2).unwrap_or(&0),
        *inicio.get(3).unwrap_or(&0),
    ]) as usize;
    let Some(pixels) = pele::MARCA.get(deslocamento..) else {
        return;
    };

    let lado = estado.px(52);
    // SAFETY: `cabecalho` aponta para o `BITMAPINFOHEADER` de um `.bmp` que este
    // binário embarca, e `pixels` para os dados que ele descreve. Os dois vivem
    // enquanto o processo viver — vêm de `include_bytes!`.
    unsafe {
        windows::Win32::Graphics::Gdi::StretchDIBits(
            hdc,
            x,
            y,
            lado,
            lado,
            0,
            0,
            52,
            52,
            Some(pixels.as_ptr().cast()),
            cabecalho.as_ptr().cast(),
            windows::Win32::Graphics::Gdi::DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

/// Abre a caixa do Windows para escolher a pasta.
///
/// `SHBrowseForFolderW`, e não o `IFileDialog` moderno: o diálogo moderno é COM
/// com quatro interfaces e um `CoInitialize` que precisa combinar com o resto do
/// processo, e o que se pede aqui é uma pasta. O antigo faz exatamente isso,
/// existe desde sempre e não tem o que dar errado.
fn perguntar_a_pasta(janela: HWND, estado: &Estado) -> Option<String> {
    use windows::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS,
        BROWSEINFOW,
    };

    let titulo = larga("Onde instalar o SEELE");
    let informacao = BROWSEINFOW {
        hwndOwner: janela,
        lpszTitle: PCWSTR(titulo.as_ptr()),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
        ..Default::default()
    };

    // SAFETY: `titulo` vive até o fim da função, e `caminho` tem o tamanho que a
    // API exige — `MAX_PATH` unidades.
    unsafe {
        let lista = SHBrowseForFolderW(&informacao);
        if lista.is_null() {
            // Cancelou, que não é erro nenhum: é a resposta «deixa como está».
            return None;
        }
        let mut caminho = [0_u16; 260];
        let deu = SHGetPathFromIDListW(lista, &mut caminho);
        windows::Win32::System::Com::CoTaskMemFree(Some(lista.cast()));
        if !deu.as_bool() {
            return None;
        }
        let fim = caminho
            .iter()
            .position(|u| *u == 0)
            .unwrap_or(caminho.len());
        let _ = estado;
        caminho.get(..fim).map(String::from_utf16_lossy)
    }
}

/// Põe a instalação numa linha própria e volta na hora.
///
/// **Numa linha própria porque a janela precisa continuar respondendo.** Abrir a
/// carga é escrever dezenas de arquivos em disco; feito aqui dentro, a janela
/// para de repintar, o Windows a marca como travada e escurece a tela dela — no
/// exato momento em que ela deveria estar mostrando que anda.
///
/// A linha de trabalho não toca no estado: ela manda cada arquivo por um canal e
/// acorda a janela com `WM_ANDOU`. Quem escreve no estado continua sendo só o
/// procedimento da janela, numa linha só, que é o que dispensa tranca.
fn comecar_a_instalar(janela: HWND, estado: &mut Estado) {
    estado.log.clear();
    estado.erro = None;

    // **Antes de escrever qualquer arquivo.** O Windows não deixa sobrescrever
    // um executável em uso; descobrir isso no meio deixaria parte dos arquivos
    // novos e parte velhos — o estado mais difícil de explicar e o mais fácil de
    // evitar.
    if sistema::produto_aberto() {
        estado.erro = Some(
            "o SEELE está aberto nesta máquina. Feche-o e aperte INSTALAR de \
             novo — enquanto ele estiver rodando, o Windows não deixa trocar os \
             arquivos dele."
                .to_owned(),
        );
        return;
    }

    let destino = std::path::PathBuf::from(pasta_escolhida(estado));
    let opcoes = estado.opcoes;
    let (manda, recebe) = std::sync::mpsc::channel();
    estado.recado = Some(recebe);

    // O `isize` atravessa a fronteira da linha porque `HWND` não é `Send` — e
    // não é `Send` por uma razão que não vale aqui: o que não se pode fazer de
    // outra linha é **mexer** na janela, e isto só a acorda. `PostMessageW` é
    // feito exatamente para isso.
    let aviso = janela.0 as isize;
    std::thread::spawn(move || {
        let acordar = || {
            // SAFETY: `PostMessageW` é a chamada que o Win32 documenta como
            // segura de outra linha; ela enfileira e volta na hora.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(HWND(aviso as *mut core::ffi::c_void)),
                    WM_ANDOU,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        };

        let [atalho, porta] = opcoes;
        let resultado =
            instalacao::executar(&destino, instalacao::Escolhas { atalho, porta }, &|passo| {
                let _ = manda.send(Ok(passo.to_owned()));
                acordar();
            });

        if let Err(motivo) = resultado {
            let _ = manda.send(Err(motivo));
        }
        acordar();
    });
}

/// O que está escrito no campo da pasta, ou a proposta se ele estiver vazio.
fn pasta_escolhida(estado: &Estado) -> String {
    let mut texto = [0_u16; 512];
    // SAFETY: `campo` é o `EDIT` desta janela e `texto` tem o tamanho que a
    // chamada recebe.
    let quantos = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(estado.campo, &mut texto)
    };
    let lido = texto
        .get(..usize::try_from(quantos).unwrap_or(0))
        .map(String::from_utf16_lossy)
        .unwrap_or_default();
    if lido.trim().is_empty() {
        instalacao::pasta_padrao()
    } else {
        lido
    }
}
