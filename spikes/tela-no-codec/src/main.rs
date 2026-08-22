//! Prova de desenho: quantos quadros por segundo o OpenH264 por software
//! entrega, e a que custo de CPU, com conteúdo de tela de verdade.
//!
//! **Descartável.** Fora do workspace, como os quatro spikes anteriores.
//! Existe para responder **uma** pergunta antes de a spec do compartilhamento
//! de tela fixar a lista de resoluções e de quadros que a interface oferece, e
//! morre com a resposta. Nada pode depender dele.
//!
//! # A pergunta
//!
//! A spec escreve, no §2, que o custo de CPU do encoder «não foi medido aqui, e
//! este documento não vai fingir que foi», e o §5 deixa a lista de opções da
//! interface deliberadamente em aberto até que ele seja. Sem esse número
//! qualquer teto é chute, e um teto chutado vira uma opção na tela que a
//! máquina não aguenta — pior que uma opção a menos.
//!
//! # Por que conteúdo de tela, e não vídeo nem ruído
//!
//! Um codec não tem um custo: tem um custo **por conteúdo**. Ruído aleatório é
//! o pior caso de todo encoder — nenhuma predição acerta, tudo vira resíduo — e
//! não é o caso de ninguém. Vídeo natural é quase o oposto: gradientes suaves,
//! movimento contínuo, bordas raras.
//!
//! Uma tela não é nem um nem outro. Ela é **quase parada**, com regiões de
//! texto de altíssimo contraste — a borda mais cara que existe para uma DCT —
//! e mudanças que chegam em rajada: uma rolagem, uma janela que ganha foco. É
//! esse o conteúdo deste produto, e é sobre ele que a medida tem de valer.
//!
//! Então a textura aqui é **capturada da tela de quem roda**, com
//! `screencapture(1)`, e o movimento é sintetizado em cima dela a 30 quadros
//! por segundo (`Roteiro`). Nenhuma imagem é escrita no repositório: a captura
//! vai para um arquivo temporário do sistema e é apagada assim que é lida.
//!
//! # O que ele mede, e nada além
//!
//! - **quadros por segundo sustentados**, encodando sem folga;
//! - **CPU**: quanto de um núcleo e quantos núcleos, lidos do próprio processo;
//! - **o que acontece quando não dá conta**: as duas políticas possíveis do
//!   lado de quem captura — enfileirar ou descartar — postas lado a lado.
//!
//! Não há rede aqui, nem captura de verdade em cadência, nem decodificação.
//!
//! # Uso
//!
//! ```text
//! cargo run --release                          # a matriz inteira
//! cargo run --release -- --modo 1080p-1t       # um cenário (casa com o nome)
//! cargo run --release -- --segundos 10
//! OPENH264_PATH=/caminho/libopenh264.dylib cargo run --release
//! ```

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use shiguredo_openh264::{
    EncodeOptions, Encoder, EncoderConfig, FrameType, Openh264Library, RateControlMode, SliceMode,
};

/// Cadência de captura que a spec assume no topo da faixa (§2: «entre 5 e 30
/// quadros por segundo»). Tudo aqui é medido contra ela.
const FPS_ALVO: usize = 30;

/// Teto de bitrate do vídeo, em bits por segundo.
///
/// Não é um número escolhido aqui: é o do `spikes/tela-no-transporte`, que
/// mediu 1200 kbps num caminho de 2000 — os 60% do §3.2 — como o único ponto em
/// que a voz volta para a linha de base. Medir com outro bitrate mediria um
/// encoder que o produto não vai configurar.
const BITRATE_BPS: usize = 1_200_000;

/// Quantos quadros da sequência ficam prontos na memória antes de o relógio
/// começar.
///
/// A geração de quadro custa CPU, e essa CPU não é do encoder. Gerar tudo antes
/// é o que separa uma medida do encoder de uma medida do encoder mais o gerador
/// de teste. 90 quadros são os 3 s de roteiro; a sequência dá a volta.
const QUADROS_NA_MEMORIA: usize = 90;

/// Quadros descartados antes de o relógio começar.
///
/// O primeiro quadro é IDR e as primeiras dezenas ainda estão com o controle de
/// taxa procurando o QP. Contá-los mediria a partida, não o regime.
const AQUECIMENTO: usize = 30;

// ============================================================================
// Imagem capturada
// ============================================================================

/// Uma imagem em BGRA, do jeito que o `screencapture` a entrega.
struct Imagem {
    largura: usize,
    altura: usize,
    bgra: Vec<u8>,
}

/// Captura a tela principal e devolve os pixels.
///
/// Sai em BMP e não em PNG de propósito: o BMP de 32 bits que o macOS escreve é
/// um cabeçalho e um bloco de pixels, e lê-se em quarenta linhas. Um PNG
/// custaria uma dependência de decodificação num spike cuja árvore inteira é
/// uma linha só.
///
/// O arquivo é temporário e é apagado aqui mesmo. Nada do que está na tela de
/// quem roda entra no repositório.
fn capturar_tela(pronta: Option<PathBuf>) -> Result<Imagem, String> {
    // Uma textura já capturada, quando a máquina não tem `screencapture`.
    //
    // Existe porque este spike foi levado a uma segunda máquina — um Ryzen com
    // Windows, alcançado por SSH — e lá aquele comando não existe. Medir numa
    // arquitetura só era a limitação que o próprio README declarava, e ela se
    // fechava por um `Command::new` que não é do assunto.
    //
    // O que se mede não muda: continua sendo tela de verdade, capturada de uma
    // tela de verdade. O que muda é **de qual** tela, e isso está no relatório.
    if let Some(arquivo) = pronta {
        let bytes = std::fs::read(&arquivo)
            .map_err(|e| format!("não consegui ler a textura de {arquivo:?}: {e}"))?;
        return ler_bmp(&bytes);
    }
    let destino =
        std::env::temp_dir().join(format!("seele-spike-codec-{}.bmp", std::process::id()));
    let saida = Command::new("screencapture")
        .arg("-x") // sem o som do obturador
        .arg("-m") // só o monitor principal
        .arg("-r") // sem metadado de dpi, que não usamos
        .arg("-t")
        .arg("bmp")
        .arg(&destino)
        .output()
        .map_err(|e| format!("não consegui executar screencapture: {e}"))?;
    if !saida.status.success() {
        let _ = std::fs::remove_file(&destino);
        return Err(format!(
            "screencapture falhou ({}): {}",
            saida.status,
            String::from_utf8_lossy(&saida.stderr).trim()
        ));
    }
    let bytes = std::fs::read(&destino).map_err(|e| {
        format!("screencapture não deixou arquivo legível em {destino:?}: {e} — é assim que a negativa do TCC aparece")
    })?;
    let _ = std::fs::remove_file(&destino);
    ler_bmp(&bytes)
}

/// Lê o BMP de 32 bits, linha de cima primeiro, que o macOS escreve.
///
/// Aceita só a forma que o `screencapture` produz. Um leitor geral de BMP seria
/// código para um caso que não existe aqui.
fn ler_bmp(bytes: &[u8]) -> Result<Imagem, String> {
    let u32_em = |p: usize| -> Result<u32, String> {
        bytes
            .get(p..p + 4)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| format!("BMP truncado no byte {p}"))
    };
    if bytes.first().copied() != Some(b'B') || bytes.get(1).copied() != Some(b'M') {
        return Err("não é um BMP".to_string());
    }
    let inicio_pixels = u32_em(10)? as usize;
    let largura = u32_em(18)? as i32;
    let altura_bruta = u32_em(22)? as i32;
    let bits = bytes
        .get(28..30)
        .and_then(|s| <[u8; 2]>::try_from(s).ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| "BMP sem contagem de bits".to_string())?;
    if bits != 32 {
        return Err(format!("BMP de {bits} bits; este leitor só trata 32"));
    }
    // Altura negativa quer dizer «primeira linha do arquivo é a de cima», que é
    // o que o `screencapture` do macOS escreve. Positiva é de baixo para cima,
    // que é a forma **padrão** do formato e a que o `System.Drawing` do Windows
    // produz — este leitor recusava justamente a comum, porque foi escrito
    // olhando uma máquina só. Encontrado ao levar o spike a um Ryzen.
    let de_baixo = altura_bruta >= 0;
    let largura = usize::try_from(largura).map_err(|_| "largura negativa".to_string())?;
    let altura = usize::try_from(altura_bruta.abs()).map_err(|_| "altura inválida".to_string())?;
    let esperado = largura
        .checked_mul(altura)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "dimensões absurdas".to_string())?;
    let cru = bytes
        .get(inicio_pixels..inicio_pixels + esperado)
        .ok_or_else(|| "BMP menor que o próprio cabeçalho promete".to_string())?;
    // Virado uma vez na leitura, e não a cada quadro: o resto deste spike
    // trabalha em cima da imagem milhares de vezes, e pagar a inversão ali
    // mediria o custo de virar linha, não o de encodar.
    let bgra = if de_baixo {
        let passo = largura * 4;
        let mut virada = Vec::with_capacity(esperado);
        for linha in (0..altura).rev() {
            let inicio = linha * passo;
            virada.extend_from_slice(
                cru.get(inicio..inicio + passo)
                    .ok_or_else(|| "linha fora do BMP".to_string())?,
            );
        }
        virada
    } else {
        cru.to_vec()
    };
    Ok(Imagem {
        largura,
        altura,
        bgra,
    })
}

// ============================================================================
// Tela de trabalho, em I420
// ============================================================================

/// A captura reduzida para a largura de destino, em I420 e mais alta que o
/// quadro.
///
/// Mais alta de propósito: a diferença entre a altura dela e a do quadro é a
/// margem por onde o roteiro rola. Sem margem não há rolagem, e sem rolagem
/// mede-se uma tela parada, que é metade do caso.
struct Lousa {
    largura: usize,
    altura: usize,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

/// Um quadro pronto para o encoder: três planos, empacotados sem borda.
struct Quadro {
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

/// Reduz a captura para `largura_alvo` com média de caixa e converte para I420.
///
/// Média de caixa, e não vizinho mais próximo, porque é o que um compositor faz
/// ao mandar uma tela Retina para um quadro de 1080p: o texto chega
/// antisserrilhado, com meios-tons na borda. Vizinho mais próximo devolveria
/// bordas duras demais e um custo de encoder que ninguém veria na prática.
fn reduzir(imagem: &Imagem, largura_alvo: usize) -> Result<Lousa, String> {
    if imagem.largura == 0 || imagem.altura == 0 {
        return Err("captura vazia".to_string());
    }
    let altura_alvo = (imagem.altura * largura_alvo / imagem.largura) & !1;
    if altura_alvo == 0 {
        return Err("captura baixa demais para o alvo".to_string());
    }
    let mut y = vec![0u8; largura_alvo * altura_alvo];
    // Guardamos o R, G e B já reduzidos para converter o croma por média de
    // quatro, que é o que o I420 pede.
    let mut rgb = vec![0u8; largura_alvo * altura_alvo * 3];
    for ly in 0..altura_alvo {
        let y0 = ly * imagem.altura / altura_alvo;
        let y1 = ((ly + 1) * imagem.altura / altura_alvo).max(y0 + 1);
        for lx in 0..largura_alvo {
            let x0 = lx * imagem.largura / largura_alvo;
            let x1 = ((lx + 1) * imagem.largura / largura_alvo).max(x0 + 1);
            let (mut sb, mut sg, mut sr, mut n) = (0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1 {
                let linha = sy * imagem.largura * 4;
                for sx in x0..x1 {
                    let p = linha + sx * 4;
                    let Some(px) = imagem.bgra.get(p..p + 3) else {
                        continue;
                    };
                    sb += u32::from(px[0]);
                    sg += u32::from(px[1]);
                    sr += u32::from(px[2]);
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let (b, g, r) = ((sb / n) as u8, (sg / n) as u8, (sr / n) as u8);
            let d = (ly * largura_alvo + lx) * 3;
            if let Some(alvo) = rgb.get_mut(d..d + 3) {
                alvo[0] = r;
                alvo[1] = g;
                alvo[2] = b;
            }
            if let Some(alvo) = y.get_mut(ly * largura_alvo + lx) {
                *alvo = luma(r, g, b);
            }
        }
    }
    let (largura_c, altura_c) = (largura_alvo / 2, altura_alvo / 2);
    let mut u = vec![128u8; largura_c * altura_c];
    let mut v = vec![128u8; largura_c * altura_c];
    for cy in 0..altura_c {
        for cx in 0..largura_c {
            let (mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32);
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let d = ((cy * 2 + dy) * largura_alvo + cx * 2 + dx) * 3;
                if let Some(px) = rgb.get(d..d + 3) {
                    sr += u32::from(px[0]);
                    sg += u32::from(px[1]);
                    sb += u32::from(px[2]);
                }
            }
            let (r, g, b) = ((sr / 4) as u8, (sg / 4) as u8, (sb / 4) as u8);
            if let Some(alvo) = u.get_mut(cy * largura_c + cx) {
                *alvo = croma_u(r, g, b);
            }
            if let Some(alvo) = v.get_mut(cy * largura_c + cx) {
                *alvo = croma_v(r, g, b);
            }
        }
    }
    Ok(Lousa {
        largura: largura_alvo,
        altura: altura_alvo,
        y,
        u,
        v,
    })
}

// BT.601 com faixa de estúdio, que é o que o H.264 de conversa usa por padrão.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
    (16.0 + (65.481 * r + 128.553 * g + 24.966 * b) / 255.0).clamp(0.0, 255.0) as u8
}
fn croma_u(r: u8, g: u8, b: u8) -> u8 {
    let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
    (128.0 + (-37.797 * r - 74.203 * g + 112.0 * b) / 255.0).clamp(0.0, 255.0) as u8
}
fn croma_v(r: u8, g: u8, b: u8) -> u8 {
    let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
    (128.0 + (112.0 * r - 93.786 * g - 18.214 * b) / 255.0).clamp(0.0, 255.0) as u8
}

// ============================================================================
// Roteiro: o movimento que uma tela de trabalho de fato tem
// ============================================================================

/// O que o quadro `n` mostra.
///
/// O roteiro dá a volta em 90 quadros — 3 s a 30 por segundo — e é
/// deliberadamente **desequilibrado**: uma tela de trabalho fica parada a maior
/// parte do tempo, e quando muda, muda inteira. Uma sequência de movimento
/// constante mediria um vídeo, não uma tela.
///
/// ```text
///  0..30   parada, cursor piscando a 2 Hz
/// 30..60   rolagem de texto, 3 px por quadro
/// 60       uma janela ganha foco: um retângulo inteiro troca de conteúdo
/// 60..75   parada de novo, com a janela por cima
/// 75..90   rolagem de volta
/// ```
struct Cena {
    /// Deslocamento vertical na lousa, sempre par (o croma é subamostrado).
    desloc: usize,
    /// A janela em foco por cima, e de que altura da lousa ela vem.
    janela: Option<usize>,
    /// O bloco do cursor está aceso.
    cursor: bool,
}

fn cena(n: usize, margem: usize) -> Cena {
    let n = n % 90;
    let passo = 3;
    let rolagem = |q: usize| ((q * passo) % (margem + 1)) & !1;
    let cursor = (n / 15).is_multiple_of(2);
    match n {
        0..=29 => Cena {
            desloc: 0,
            janela: None,
            cursor,
        },
        30..=59 => Cena {
            desloc: rolagem(n - 30),
            janela: None,
            cursor,
        },
        60..=74 => Cena {
            desloc: rolagem(29),
            janela: Some(margem),
            cursor,
        },
        _ => Cena {
            desloc: rolagem(89 - n),
            janela: Some(margem),
            cursor,
        },
    }
}

/// Monta a sequência inteira na memória, antes de o relógio começar.
fn montar_sequencia(lousa: &Lousa, largura: usize, altura: usize, quantos: usize) -> Vec<Quadro> {
    let margem = lousa.altura.saturating_sub(altura) & !1;
    (0..quantos)
        .map(|n| {
            let c = cena(n, margem);
            let mut q = recortar(lousa, largura, altura, c.desloc);
            if let Some(origem) = c.janela {
                // Uma janela que ganha foco não é um efeito: é um retângulo da
                // tela que passa a mostrar outra coisa, com borda dura. Aqui o
                // «outra coisa» vem de outra altura da mesma captura, então são
                // pixels de tela de verdade dos dois lados da borda.
                compor_janela(&mut q, lousa, largura, altura, origem);
            }
            desenhar_cursor(&mut q, largura, altura, c.cursor);
            q
        })
        .collect()
}

fn recortar(lousa: &Lousa, largura: usize, altura: usize, desloc: usize) -> Quadro {
    let (lc, ac) = (largura / 2, altura / 2);
    let mut y = vec![16u8; largura * altura];
    let mut u = vec![128u8; lc * ac];
    let mut v = vec![128u8; lc * ac];
    for linha in 0..altura {
        let origem = (desloc + linha) * lousa.largura;
        if let (Some(dst), Some(src)) = (
            y.get_mut(linha * largura..linha * largura + largura),
            lousa.y.get(origem..origem + largura),
        ) {
            dst.copy_from_slice(src);
        }
    }
    for linha in 0..ac {
        let origem = (desloc / 2 + linha) * (lousa.largura / 2);
        if let (Some(dst), Some(src)) = (
            u.get_mut(linha * lc..linha * lc + lc),
            lousa.u.get(origem..origem + lc),
        ) {
            dst.copy_from_slice(src);
        }
        if let (Some(dst), Some(src)) = (
            v.get_mut(linha * lc..linha * lc + lc),
            lousa.v.get(origem..origem + lc),
        ) {
            dst.copy_from_slice(src);
        }
    }
    Quadro { y, u, v }
}

fn compor_janela(q: &mut Quadro, lousa: &Lousa, largura: usize, altura: usize, origem: usize) {
    // Um retângulo com a proporção de uma janela de editor, encostado à direita.
    let jx = (largura / 8) & !1;
    let jy = (altura / 8) & !1;
    let jl = (largura * 3 / 4) & !1;
    let ja = (altura * 3 / 4) & !1;
    for linha in 0..ja {
        let src = (origem + linha) * lousa.largura + jx;
        let dst = (jy + linha) * largura + jx;
        if let (Some(d), Some(s)) = (
            q.y.get_mut(dst..dst + jl),
            lousa.y.get(src..src.saturating_add(jl)),
        ) {
            d.copy_from_slice(s);
        }
    }
    let (lc, jxc, jyc, jlc, jac) = (largura / 2, jx / 2, jy / 2, jl / 2, ja / 2);
    for linha in 0..jac {
        let src = (origem / 2 + linha) * (lousa.largura / 2) + jxc;
        let dst = (jyc + linha) * lc + jxc;
        if let (Some(d), Some(s)) = (
            q.u.get_mut(dst..dst + jlc),
            lousa.u.get(src..src.saturating_add(jlc)),
        ) {
            d.copy_from_slice(s);
        }
        if let (Some(d), Some(s)) = (
            q.v.get_mut(dst..dst + jlc),
            lousa.v.get(src..src.saturating_add(jlc)),
        ) {
            d.copy_from_slice(s);
        }
    }
}

/// O bloco do cursor: poucos pixels, contraste máximo, duas vezes por segundo.
///
/// É a menor mudança que uma tela parada tem, e é a que decide se «parada»
/// significa quadro de zero byte ou não.
fn desenhar_cursor(q: &mut Quadro, largura: usize, altura: usize, aceso: bool) {
    let (cx, cy) = (largura / 4, altura / 2);
    let (cl, ca) = (largura / 160, altura / 40);
    let tom = if aceso { 235 } else { 16 };
    for linha in 0..ca {
        let p = (cy + linha) * largura + cx;
        if let Some(d) = q.y.get_mut(p..p + cl) {
            d.fill(tom);
        }
    }
}

/// Escreve o plano de luma de um quadro como BMP, para conferir com o olho que
/// o conteúdo é o que este arquivo diz que é.
///
/// Existe porque a alternativa é acreditar: uma sequência com um defeito de
/// recorte encodaria depressa e devolveria um número bonito e falso. Sai só
/// quando `--amostra` pede, e sai para fora do repositório.
fn escrever_amostra(
    caminho: &Path,
    q: &Quadro,
    largura: usize,
    altura: usize,
) -> Result<(), String> {
    let mut bmp = Vec::with_capacity(54 + largura * altura * 4);
    let tamanho = 54 + largura * altura * 4;
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(tamanho as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(largura as u32).to_le_bytes());
    bmp.extend_from_slice(&(-(altura as i32)).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&32u16.to_le_bytes());
    bmp.extend_from_slice(&[0u8; 24]);
    for tom in q.y.iter().copied() {
        bmp.extend_from_slice(&[tom, tom, tom, 255]);
    }
    std::fs::write(caminho, bmp).map_err(|e| format!("não consegui escrever {caminho:?}: {e}"))
}

// ============================================================================
// A medida
// ============================================================================

/// Segundos de CPU já gastos por este processo, somando todas as threads.
///
/// Lido do `ps` e não de `getrusage` porque `getrusage` exige FFI, e FFI exige
/// `unsafe`, que esta casa proíbe. A resolução do `ps` é de 10 ms, o que sobre
/// uma corrida de segundos erra na terceira casa — bem abaixo do que qualquer
/// conclusão daqui depende.
fn cpu_segundos() -> Option<f64> {
    let saida = Command::new("ps")
        .args(["-o", "cputime=", "-p"])
        .arg(std::process::id().to_string())
        .output()
        .ok()?;
    let texto = String::from_utf8_lossy(&saida.stdout);
    let texto = texto.trim();
    let (minutos, resto) = texto.rsplit_once(':')?;
    let minutos: f64 = minutos.rsplit(':').next()?.parse().ok()?;
    let segundos: f64 = resto.parse().ok()?;
    Some(minutos * 60.0 + segundos)
}

struct Cenario {
    nome: &'static str,
    largura: usize,
    altura: usize,
    threads: usize,
    /// Em quantas fatias o quadro é cortado.
    ///
    /// Não é enfeite: o OpenH264 só paraleliza **entre fatias**. Pedir quatro
    /// threads e deixar o quadro numa fatia só devolve exatamente uma thread
    /// trabalhando, e foi assim que a primeira corrida deste spike saiu — 1,00
    /// núcleo nas cinco linhas. Cortar em fatias custa qualidade por bit
    /// (predição não atravessa fatia) e é a única maneira de o encoder usar
    /// mais de um núcleo.
    fatias: usize,
}

struct Resultado {
    nome: &'static str,
    fps: f64,
    nucleos: f64,
    ms_p50: f64,
    ms_p95: f64,
    ms_pior: f64,
    /// Quanto custa o quadro-chave, que é a rajada que o §3.3 manda espalhar.
    ms_idr: f64,
    kib_idr: f64,
    kbps: f64,
    /// Fração dos quadros que o próprio controle de taxa jogou fora para não
    /// estourar o teto.
    pulados_pct: f64,
}

impl Resultado {
    /// Quanto de um núcleo custaria sustentar 30 quadros por segundo.
    ///
    /// É o único número desta tabela que atravessa máquinas: `fps` e `nucleos`
    /// descrevem esta máquina, mas «0,07 núcleo por 30 quadros de 1080p» é uma
    /// razão, e numa máquina três vezes mais lenta ela vira 0,21. É por ele que
    /// a lista de opções do §5 se decide.
    fn nucleo_por_30(&self) -> f64 {
        self.nucleos * 30.0 / self.fps
    }
}

fn medir(
    lib: &Openh264Library,
    cenario: &Cenario,
    sequencia: &[Quadro],
    segundos: f64,
    chave_s: f64,
) -> Result<Resultado, String> {
    let mut config = EncoderConfig::new(cenario.largura, cenario.altura, BITRATE_BPS, FPS_ALVO, 1);
    // Modo bitrate, e não qualidade: o §3.2 diz que o vídeo tem teto, então o
    // encoder que interessa medir é o que respeita um teto. É também o único
    // modo em que o OpenH264 pula quadro por conta própria — e se ele pula,
    // isso é resposta para a pergunta «o que acontece quando não dá conta».
    config.rate_control_mode = Some(RateControlMode::Bitrate);
    config.thread_count = std::num::NonZeroUsize::new(cenario.threads);
    config.slice_mode = Some(if cenario.fatias > 1 {
        SliceMode::FixedCount(cenario.fatias)
    } else {
        SliceMode::Single
    });
    let mut encoder = Encoder::new(lib.clone(), config).map_err(|e| e.to_string())?;

    // O §3.3 decidiu quadro-chave **sob demanda**, não periódico, e o padrão
    // deste spike é medir os dois: com `--chave-s 2` a rajada aparece na
    // medida e o custo dela entra no orçamento de bits; com `--chave-s 0` só o
    // primeiro quadro é IDR, que é o desenho de verdade. A diferença entre as
    // duas tabelas é o preço do quadro-chave periódico, e ele não é pequeno.
    let entre_chaves = if chave_s > 0.0 {
        (FPS_ALVO as f64 * chave_s).round().max(1.0) as usize
    } else {
        usize::MAX
    };

    let mut n = 0usize;
    let tomar = |encoder: &mut Encoder, n: usize, forcar: bool| -> Result<(usize, bool), String> {
        let q = sequencia
            .get(n % sequencia.len())
            .ok_or_else(|| "sequência vazia".to_string())?;
        let opcoes = EncodeOptions { force_idr: forcar };
        match encoder
            .encode(&q.y, &q.u, &q.v, &opcoes)
            .map_err(|e| e.to_string())?
        {
            Some(f) => Ok((f.data.len(), f.frame_type == FrameType::Idr)),
            None => Ok((0, false)),
        }
    };

    for _ in 0..AQUECIMENTO {
        tomar(&mut encoder, n, n == 0)?;
        n += 1;
    }

    let cpu0 = cpu_segundos().ok_or_else(|| "não consegui ler a CPU do processo".to_string())?;
    let t0 = Instant::now();
    let mut duracoes: Vec<Duration> = Vec::new();
    let mut chaves: Vec<(Duration, usize)> = Vec::new();
    let mut bytes = 0usize;
    let mut pulados = 0usize;
    let mut quadros = 0usize;
    while t0.elapsed().as_secs_f64() < segundos {
        let forcar = n.is_multiple_of(entre_chaves) && chave_s > 0.0;
        let m0 = Instant::now();
        let (tam, foi_idr) = tomar(&mut encoder, n, forcar)?;
        let levou = m0.elapsed();
        quadros += 1;
        bytes += tam;
        if foi_idr {
            chaves.push((levou, tam));
        } else {
            duracoes.push(levou);
            if tam == 0 {
                pulados += 1;
            }
        }
        n += 1;
    }
    let parede = t0.elapsed().as_secs_f64();
    let cpu1 = cpu_segundos().ok_or_else(|| "não consegui ler a CPU do processo".to_string())?;

    duracoes.sort_unstable();
    let ms = |q: f64| -> f64 {
        let i = ((duracoes.len().max(1) as f64 - 1.0) * q).round() as usize;
        duracoes.get(i).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    };
    let media = |f: fn(&(Duration, usize)) -> f64| -> f64 {
        if chaves.is_empty() {
            0.0
        } else {
            chaves.iter().map(f).sum::<f64>() / chaves.len() as f64
        }
    };
    Ok(Resultado {
        nome: cenario.nome,
        fps: quadros as f64 / parede,
        nucleos: (cpu1 - cpu0) / parede,
        ms_p50: ms(0.50),
        ms_p95: ms(0.95),
        ms_pior: ms(1.0),
        ms_idr: media(|(d, _)| d.as_secs_f64() * 1000.0),
        kib_idr: media(|(_, t)| *t as f64 / 1024.0),
        // O bitrate se lê no tempo do conteúdo, não no tempo de parede: cada
        // quadro vale 1/30 s para o encoder, encodado depressa ou devagar.
        kbps: bytes as f64 * 8.0 * FPS_ALVO as f64 / quadros as f64 / 1000.0,
        pulados_pct: pulados as f64 * 100.0 / quadros.max(1) as f64,
    })
}

// ============================================================================
// O que acontece quando não dá conta
// ============================================================================

struct Atraso {
    politica: &'static str,
    entregues: usize,
    descartados: usize,
    fila_final: usize,
    idade_p50_ms: f64,
    idade_pior_ms: f64,
}

/// Alimenta o encoder a uma cadência que ele **não** sustenta, com as duas
/// políticas possíveis do lado de quem captura, e mede a idade do quadro que
/// sai — quanto tempo se passou entre ele ter sido capturado e ter sido
/// encodado.
///
/// O encoder em si não faz nem uma coisa nem outra: `encode()` é uma chamada
/// síncrona que volta quando termina. Quem enfileira ou descarta é o chamador,
/// e é por isso que a escolha é de desenho e não de biblioteca.
fn medir_atraso(
    lib: &Openh264Library,
    cenario: &Cenario,
    sequencia: &[Quadro],
    fps_captura: f64,
    segundos: f64,
    descartar: bool,
) -> Result<Atraso, String> {
    let mut config = EncoderConfig::new(cenario.largura, cenario.altura, BITRATE_BPS, FPS_ALVO, 1);
    config.rate_control_mode = Some(RateControlMode::Bitrate);
    config.thread_count = std::num::NonZeroUsize::new(cenario.threads);
    config.slice_mode = Some(if cenario.fatias > 1 {
        SliceMode::FixedCount(cenario.fatias)
    } else {
        SliceMode::Single
    });
    let mut encoder = Encoder::new(lib.clone(), config).map_err(|e| e.to_string())?;

    let intervalo = 1.0 / fps_captura;
    let t0 = Instant::now();
    // A fila guarda só o índice do quadro e o instante em que ele foi
    // «capturado»; os pixels já estão na memória e são os mesmos.
    let mut fila: std::collections::VecDeque<(usize, f64)> = std::collections::VecDeque::new();
    let mut proximo = 0usize;
    let mut idades: Vec<f64> = Vec::new();
    let mut entregues = 0usize;
    let mut descartados = 0usize;

    while t0.elapsed().as_secs_f64() < segundos {
        let agora = t0.elapsed().as_secs_f64();
        // Tudo que a captura produziu desde a última volta entra agora.
        while (proximo as f64) * intervalo <= agora {
            let nascimento = (proximo as f64) * intervalo;
            if descartar && !fila.is_empty() {
                // A regra do §1: o quadro novo substitui o que o encoder ainda
                // não pegou. Um quadro velho entregue tarde é pior que um
                // quadro perdido.
                descartados += fila.len();
                fila.clear();
            }
            fila.push_back((proximo, nascimento));
            proximo += 1;
        }
        let Some((indice, nascimento)) = fila.pop_front() else {
            continue;
        };
        let q = sequencia
            .get(indice % sequencia.len())
            .ok_or_else(|| "sequência vazia".to_string())?;
        encoder
            .encode(&q.y, &q.u, &q.v, &EncodeOptions::default())
            .map_err(|e| e.to_string())?;
        entregues += 1;
        idades.push((t0.elapsed().as_secs_f64() - nascimento) * 1000.0);
    }

    idades.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let em = |q: f64| -> f64 {
        let i = ((idades.len().max(1) as f64 - 1.0) * q).round() as usize;
        idades.get(i).copied().unwrap_or(0.0)
    };
    Ok(Atraso {
        politica: if descartar { "descarta" } else { "enfileira" },
        entregues,
        descartados,
        fila_final: fila.len(),
        idade_p50_ms: em(0.50),
        idade_pior_ms: em(1.0),
    })
}

// ============================================================================

fn caminho_da_biblioteca(argumento: Option<String>) -> Result<PathBuf, String> {
    if let Some(p) = argumento {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("OPENH264_PATH") {
        return Ok(PathBuf::from(p));
    }
    let vizinho = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("libopenh264.dylib");
    if vizinho.exists() {
        return Ok(vizinho);
    }
    Err(format!(
        "o módulo do Cisco não está nesta máquina.\n\
         O binário deste produto não vem com codec — é a decisão do §2 da spec, e é a licença\n\
         que a impõe. Busque-o e aponte para ele:\n\n  \
         macOS arm64:  libopenh264-2.6.0-mac-arm64.dylib.bz2\n  \
         Windows x64:  openh264-2.6.0-win64.dll.bz2  (sem o «lib» na frente)\n\n  \
         curl -L -o m.bz2 https://ciscobinary.openh264.org/<o de cima>\n  \
         bunzip2 m.bz2 && mv m {}\n\n\
         Ou aponte para onde ele já estiver, com $OPENH264_PATH ou --lib.\n",
        vizinho.display()
    ))
}

fn escrever_tabela(
    linhas: &mut String,
    resultados: &[(f64, Resultado)],
    chave_s: f64,
) -> Result<(), String> {
    writeln!(
        linhas,
        "{:<17} {:>7} {:>8} {:>9} {:>7} {:>7} {:>8} {:>7} {:>8} {:>7} {:>8}",
        "cenario",
        "fps",
        "nucleos",
        "nucleo/30",
        "p50 ms",
        "p95 ms",
        "pior ms",
        "idr ms",
        "idr KiB",
        "kbps",
        "pulados"
    )
    .map_err(|e| e.to_string())?;
    for (_, r) in resultados.iter().filter(|(c, _)| *c == chave_s) {
        writeln!(
            linhas,
            "{:<17} {:>7.0} {:>8.2} {:>9.3} {:>7.2} {:>7.2} {:>8.2} {:>7.2} {:>8.1} {:>7.0} {:>7.1}%",
            r.nome,
            r.fps,
            r.nucleos,
            r.nucleo_por_30(),
            r.ms_p50,
            r.ms_p95,
            r.ms_pior,
            r.ms_idr,
            r.kib_idr,
            r.kbps,
            r.pulados_pct
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mut argumentos = std::env::args().skip(1);
    let mut lib_arg = None;
    let mut modo: Option<String> = None;
    let mut amostra: Option<PathBuf> = None;
    let mut textura: Option<PathBuf> = None;
    let mut segundos = 8.0f64;
    let mut chaves_s = vec![2.0f64, 0.0];
    while let Some(a) = argumentos.next() {
        match a.as_str() {
            "--lib" => lib_arg = argumentos.next(),
            "--modo" => modo = argumentos.next(),
            "--amostra" => amostra = argumentos.next().map(PathBuf::from),
            "--textura" => textura = argumentos.next().map(PathBuf::from),
            "--chave-s" => {
                chaves_s = vec![argumentos
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| "--chave-s pede um número".to_string())?]
            }
            "--segundos" => {
                segundos = argumentos
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| "--segundos pede um número".to_string())?
            }
            outro => return Err(format!("argumento que não conheço: {outro}")),
        }
    }

    let caminho = caminho_da_biblioteca(lib_arg)?;
    let lib = Openh264Library::load(&caminho).map_err(|e| {
        format!("o dlopen de {} falhou: {e}\nÉ o achado que o §2 previu: sem o módulo do Cisco não há encoder.", caminho.display())
    })?;
    println!(
        "módulo: {} · OpenH264 {} · bindings de {}",
        caminho.display(),
        lib.runtime_version(),
        shiguredo_openh264::BUILD_VERSION,
    );

    let imagem = capturar_tela(textura)?;
    println!(
        "captura: {}x{} da tela principal, em memória e já apagada do disco",
        imagem.largura, imagem.altura
    );

    let cenarios = [
        Cenario {
            nome: "1080p, 1 fatia",
            largura: 1920,
            altura: 1080,
            threads: 1,
            fatias: 1,
        },
        Cenario {
            nome: "1080p, 4 fatias",
            largura: 1920,
            altura: 1080,
            threads: 4,
            fatias: 4,
        },
        Cenario {
            nome: "720p, 1 fatia",
            largura: 1280,
            altura: 720,
            threads: 1,
            fatias: 1,
        },
        Cenario {
            nome: "720p, 4 fatias",
            largura: 1280,
            altura: 720,
            threads: 4,
            fatias: 4,
        },
        Cenario {
            nome: "540p, 1 fatia",
            largura: 960,
            altura: 540,
            threads: 1,
            fatias: 1,
        },
        Cenario {
            nome: "360p, 1 fatia",
            largura: 640,
            altura: 360,
            threads: 1,
            fatias: 1,
        },
    ];

    let mut lousas: Vec<(usize, Lousa)> = Vec::new();
    let mut resultados: Vec<(f64, Resultado)> = Vec::new();
    for cenario in &cenarios {
        if modo.as_deref().is_some_and(|m| m != cenario.nome) {
            continue;
        }
        if !lousas.iter().any(|(l, _)| *l == cenario.largura) {
            lousas.push((cenario.largura, reduzir(&imagem, cenario.largura)?));
        }
        let lousa = lousas
            .iter()
            .find(|(l, _)| *l == cenario.largura)
            .map(|(_, l)| l)
            .ok_or_else(|| "lousa sumiu".to_string())?;
        let sequencia =
            montar_sequencia(lousa, cenario.largura, cenario.altura, QUADROS_NA_MEMORIA);
        if let Some(dir) = &amostra {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            for n in [0usize, 45, 62, 70] {
                let Some(q) = sequencia.get(n) else { continue };
                escrever_amostra(
                    &dir.join(format!("{}-{n:03}.bmp", cenario.largura)),
                    q,
                    cenario.largura,
                    cenario.altura,
                )?;
            }
        }
        for chave_s in &chaves_s {
            let r = medir(&lib, cenario, &sequencia, segundos, *chave_s)?;
            resultados.push((*chave_s, r));
        }
    }

    for chave_s in &chaves_s {
        let mut linhas = String::new();
        writeln!(
            linhas,
            "\n{}",
            if *chave_s > 0.0 {
                format!("quadro-chave forçado a cada {chave_s:.0} s (o pessimista):")
            } else {
                "quadro-chave só no início, que é o desenho do §3.3:".to_string()
            }
        )
        .map_err(|e| e.to_string())?;
        escrever_tabela(&mut linhas, &resultados, *chave_s)?;
        print!("{linhas}");
    }

    // Quando não dá conta. Nesta máquina o 1080p a 30 sobra, então a falta de
    // CPU é provocada pedindo mais quadros do que a medida diz que cabem — a
    // forma da resposta é a mesma que numa máquina fraca a 30, e o README diz
    // que é provocada.
    let Some((_, referencia)) = resultados.first() else {
        return Ok(());
    };
    let Some(cenario) = cenarios.iter().find(|c| c.nome == referencia.nome) else {
        return Ok(());
    };
    let fps_captura = referencia.fps * 1.6;
    let lousa = reduzir(&imagem, cenario.largura)?;
    let sequencia = montar_sequencia(&lousa, cenario.largura, cenario.altura, QUADROS_NA_MEMORIA);
    println!(
        "\nsem CPU para dar conta — «{}» pedindo {:.0} quadros/s, que é 1,6x o que ele sustenta:",
        cenario.nome, fps_captura
    );
    println!(
        "{:<12} {:>10} {:>12} {:>11} {:>12} {:>12} {:>10}",
        "politica", "entregues", "descartados", "fila final", "idade p50", "idade pior", "cresce"
    );
    for descartar in [false, true] {
        let a = medir_atraso(&lib, cenario, &sequencia, fps_captura, segundos, descartar)?;
        println!(
            "{:<12} {:>10} {:>12} {:>11} {:>9.0} ms {:>9.0} ms {:>9.1}%",
            a.politica,
            a.entregues,
            a.descartados,
            a.fila_final,
            a.idade_p50_ms,
            a.idade_pior_ms,
            // Quanto da corrida virou atraso. É o número que atravessa
            // máquinas: com 1,6x de déficit a idade cresce 37,5% do tempo
            // decorrido, e cresceria igual numa máquina dez vezes mais lenta
            // com o mesmo déficit — sem limite, porque nada nesse caminho para.
            a.idade_pior_ms / 10.0 / segundos
        );
    }

    Ok(())
}
