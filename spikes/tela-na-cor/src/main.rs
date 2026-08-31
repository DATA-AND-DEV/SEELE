//! A `dcv-color-primitives` vale a troca no caminho BGRA→I420 do Windows?
//!
//! # A pergunta
//!
//! O `crates/seele-video/src/captura/windows.rs` documenta, medido num Ryzen 7
//! 5800X3D capturando 2560×1440: **16,5 ms por quadro** para 1080p, contra
//! **0,105 de núcleo** que o OpenH264 gasta codificando. Converter custa cinco
//! vezes codificar, e o comentário diz que baixar isso «é mudança de desenho —
//! converter na GPU, ou converter só as regiões sujas —, não ajuste».
//!
//! Antes de mexer em GPU, a pergunta barata: uma biblioteca com SIMD resolve?
//!
//! # A descoberta que o benchmark precisa respeitar
//!
//! **O nosso `converter()` faz duas coisas num laço só:** redimensiona
//! 2560×1440 → 1920×1080 com **média de área** (cada pixel de destino é a média
//! da região de origem inteira que ele cobre — o comentário original explica
//! que vizinho-mais-próximo «some a barra do `t` e o `e` vira `c`») **e**
//! converte BGRA em I420 BT.709 de faixa de TV.
//!
//! A `convert_image` da `dcv` faz **só a segunda**. Ela não redimensiona.
//!
//! Então «trocar uma pela outra» não existe, e medir as duas lado a lado na
//! mesma resolução seria comparar trabalhos diferentes. Este spike mede os três
//! números que a decisão precisa:
//!
//! 1. **nosso**, como está: 2560×1440 BGRA → 1080p I420, escala + cor;
//! 2. **dcv no destino**: 1920×1080 BGRA → I420. O piso otimista — o que a cor
//!    sozinha custaria *se* a escala fosse de graça, que ela não é;
//! 3. **dcv na origem**: 2560×1440 BGRA → I420. O caminho honesto de «converter
//!    primeiro, escalar depois», e que paga 1,8× mais pixels.
//!
//! # O que esta máquina não pode responder
//!
//! Isto roda num **Apple M5 Pro (arm64)** e o alvo é **Windows x86_64**. A
//! aceleração da `dcv` é AVX2/SSE. Se ela não tiver NEON, o número 2 aqui é o
//! caminho escalar dela — um **piso**, não a resposta. Por isso a primeira
//! coisa que o programa imprime é o que a própria biblioteca diz estar usando.

use std::time::Instant;

use dcv_color_primitives::{convert_image, ColorSpace, ImageFormat, PixelFormat};
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

/// Reescala um plano com a `fast_image_resize`, filtro caixa.
///
/// Filtro **caixa** e não Lanczos de propósito: caixa é média de área, que é
/// exatamente o que o nosso laço faz e o que o comentário do `windows.rs`
/// defende para texto. Trocar o filtro junto com a biblioteca mediria duas
/// mudanças de uma vez.
fn reescalar_rapido(
    resizer: &mut Resizer,
    origem: &[u8],
    (lo, ao): (usize, usize),
    (ld, ad): (usize, usize),
    destino: &mut Image<'static>,
) {
    let src = ImageRef::new(lo as u32, ao as u32, origem, PixelType::U8).expect("plano válido");
    resizer
        .resize(
            &src,
            destino,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Box)),
        )
        .expect("reescala aceita");
    let _ = (ld, ad);
}

const ORIGEM: (usize, usize) = (2560, 1440);
const DESTINO: (usize, usize) = (1920, 1080);
const RODADAS: usize = 30;

// ---------------------------------------------------------------------------
// O nosso caminho, portado fiel do `captura/windows.rs`.
//
// Portado e não importado porque aquele módulo é `cfg(windows)` e arrasta o
// `windows-capture`, que não compila aqui. As funções abaixo são cópia literal:
// `faixas`, `media`, `luma_de`, `croma_de` e o laço de `converter`.
// ---------------------------------------------------------------------------

const PRETO_LUMA: u8 = 16;
const CENTRO_CROMA: u8 = 128;

fn faixas(origem: usize, destino: usize) -> Vec<(usize, usize)> {
    (0..destino)
        .map(|i| {
            let inicio = i * origem / destino;
            let fim = ((i + 1) * origem / destino).max(inicio + 1).min(origem);
            (inicio, fim)
        })
        .collect()
}

fn media(
    bytes: &[u8],
    passo: usize,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
) -> (u32, u32, u32) {
    let mut soma = (0u32, 0u32, 0u32);
    let mut quantos = 0u32;
    for y in y0..y1 {
        let Some(faixa) = bytes.get(y * passo + x0 * 4..y * passo + x1 * 4) else {
            continue;
        };
        for pixel in faixa.chunks_exact(4) {
            if let [azul, verde, vermelho, _alfa] = pixel {
                soma.0 += u32::from(*vermelho);
                soma.1 += u32::from(*verde);
                soma.2 += u32::from(*azul);
                quantos += 1;
            }
        }
    }
    if quantos == 0 {
        return (0, 0, 0);
    }
    (soma.0 / quantos, soma.1 / quantos, soma.2 / quantos)
}

fn luma_de((vermelho, verde, azul): (u32, u32, u32)) -> u8 {
    let soma = 47 * vermelho as i32 + 157 * verde as i32 + 16 * azul as i32;
    (16 + soma.div_euclid(256)).clamp(16, 235) as u8
}

fn croma_de((vermelho, verde, azul): (u32, u32, u32)) -> (u8, u8) {
    let (vermelho, verde, azul) = (vermelho as i32, verde as i32, azul as i32);
    let u = 128 + (-25 * vermelho - 87 * verde + 112 * azul).div_euclid(256);
    let v = 128 + (112 * vermelho - 102 * verde - 10 * azul).div_euclid(256);
    (u.clamp(16, 240) as u8, v.clamp(16, 240) as u8)
}

/// Escala por média de área **e** converte, como o `converter()` de verdade.
///
/// Origem e destino são ambos 16:9, então não há tarja: `ativa` é o destino
/// inteiro e o canto é (0, 0). É o caso da medição documentada.
fn nosso(bytes: &[u8], passo: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (largura, altura) = DESTINO;
    let largura_croma = largura.div_ceil(2);
    let colunas = faixas(ORIGEM.0, largura);
    let linhas = faixas(ORIGEM.1, altura);

    let mut luma = vec![PRETO_LUMA; largura * altura];
    let mut croma_u = vec![CENTRO_CROMA; largura_croma * altura.div_ceil(2)];
    let mut croma_v = vec![CENTRO_CROMA; largura_croma * altura.div_ceil(2)];

    let pares_de_linha = luma.chunks_exact_mut(largura * 2);
    let linhas_u = croma_u.chunks_exact_mut(largura_croma);
    let linhas_v = croma_v.chunks_exact_mut(largura_croma);

    for (bloco_y, ((par_de_linhas, linha_u), linha_v)) in
        pares_de_linha.zip(linhas_u).zip(linhas_v).enumerate()
    {
        let (Some(&(oy0_cima, oy1_cima)), Some(&(oy0_baixo, oy1_baixo))) =
            (linhas.get(bloco_y * 2), linhas.get(bloco_y * 2 + 1))
        else {
            continue;
        };
        let (linha_cima, linha_baixo) = par_de_linhas.split_at_mut(largura);

        for (bloco_x, (((alto, baixo), destino_u), destino_v)) in linha_cima
            .chunks_exact_mut(2)
            .zip(linha_baixo.chunks_exact_mut(2))
            .zip(linha_u.iter_mut())
            .zip(linha_v.iter_mut())
            .enumerate()
        {
            let (Some(&(ox0_esq, ox1_esq)), Some(&(ox0_dir, ox1_dir))) =
                (colunas.get(bloco_x * 2), colunas.get(bloco_x * 2 + 1))
            else {
                continue;
            };

            let cor_ce = media(bytes, passo, ox0_esq, ox1_esq, oy0_cima, oy1_cima);
            let cor_cd = media(bytes, passo, ox0_dir, ox1_dir, oy0_cima, oy1_cima);
            let cor_be = media(bytes, passo, ox0_esq, ox1_esq, oy0_baixo, oy1_baixo);
            let cor_bd = media(bytes, passo, ox0_dir, ox1_dir, oy0_baixo, oy1_baixo);

            if let [esquerda, direita] = alto {
                *esquerda = luma_de(cor_ce);
                *direita = luma_de(cor_cd);
            }
            if let [esquerda, direita] = baixo {
                *esquerda = luma_de(cor_be);
                *direita = luma_de(cor_bd);
            }

            let media_do_bloco = (
                (cor_ce.0 + cor_cd.0 + cor_be.0 + cor_bd.0) / 4,
                (cor_ce.1 + cor_cd.1 + cor_be.1 + cor_bd.1) / 4,
                (cor_ce.2 + cor_cd.2 + cor_be.2 + cor_bd.2) / 4,
            );
            let (u, v) = croma_de(media_do_bloco);
            *destino_u = u;
            *destino_v = v;
        }
    }
    (luma, croma_u, croma_v)
}


/// Reescala um plano de 8 bits por média de área, como o nosso laço faz.
///
/// É a metade do trabalho que a `dcv` **não** faz, e sem ela a comparação é
/// injusta a favor dela. Fica num plano só porque I420 são três planos
/// independentes: a luma inteira e os dois cromas pela metade.
fn reescalar_plano(
    origem: &[u8],
    (largura_origem, altura_origem): (usize, usize),
    (largura_destino, altura_destino): (usize, usize),
    destino: &mut [u8],
) {
    let colunas = faixas(largura_origem, largura_destino);
    let linhas = faixas(altura_origem, altura_destino);
    for (y, &(y0, y1)) in linhas.iter().enumerate() {
        for (x, &(x0, x1)) in colunas.iter().enumerate() {
            let mut soma = 0u32;
            let mut quantos = 0u32;
            for ly in y0..y1 {
                for lx in x0..x1 {
                    soma += u32::from(origem[ly * largura_origem + lx]);
                    quantos += 1;
                }
            }
            destino[y * largura_destino + x] = (soma / quantos.max(1)) as u8;
        }
    }
}

// ---------------------------------------------------------------------------
// O caminho da dcv.
// ---------------------------------------------------------------------------

/// BGRA empacotado → I420 em três planos, BT.709 faixa de TV.
///
/// `Bt709` com `PixelFormat::I420` é a faixa de TV na `dcv`; é o mesmo espaço
/// que o nosso `luma_de`/`croma_de` implementa (16–235 na luma, 16–240 no
/// croma), então a comparação é entre duas implementações da mesma conta.
fn pela_dcv(
    bytes: &[u8],
    largura: usize,
    altura: usize,
    y: &mut [u8],
    u: &mut [u8],
    v: &mut [u8],
) {
    let origem = ImageFormat {
        pixel_format: PixelFormat::Bgra,
        color_space: ColorSpace::Rgb,
        num_planes: 1,
    };
    let destino = ImageFormat {
        pixel_format: PixelFormat::I420,
        color_space: ColorSpace::Bt709,
        num_planes: 3,
    };
    convert_image(
        largura as u32,
        altura as u32,
        &origem,
        None,
        &[bytes],
        &destino,
        None,
        &mut [y, u, v],
    )
    .expect("a dcv aceita BGRA→I420");
}

// ---------------------------------------------------------------------------

/// Conteúdo de tela sintético e determinístico.
///
/// Não é cor chapada: cor chapada é injustamente amigável ao preditor de saltos
/// e ao cache, e mediria um caso que não existe. Isto tem gradiente, blocos e
/// ruído — a mistura que uma tela de trabalho tem.
fn tela(largura: usize, altura: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; largura * altura * 4];
    let mut semente = 0x2545_F491_4F6C_DD1Du64;
    for y in 0..altura {
        for x in 0..largura {
            semente ^= semente << 13;
            semente ^= semente >> 7;
            semente ^= semente << 17;
            let ruido = if std::env::var_os("SEM_RUIDO").is_some() {
                0
            } else {
                (semente & 0x1F) as u8
            };
            let i = (y * largura + x) * 4;
            bytes[i] = (x % 256) as u8 ^ ruido; // azul
            bytes[i + 1] = (y % 256) as u8; // verde
            bytes[i + 2] = ((x + y) % 256) as u8; // vermelho
            bytes[i + 3] = 255;
        }
    }
    bytes
}

fn cronometrar(nome: &str, pixels_destino: usize, mut corpo: impl FnMut()) -> f64 {
    for _ in 0..3 {
        corpo();
    }
    let mut tempos = Vec::with_capacity(RODADAS);
    for _ in 0..RODADAS {
        let inicio = Instant::now();
        corpo();
        tempos.push(inicio.elapsed().as_secs_f64() * 1000.0);
    }
    tempos.sort_by(|a, b| a.partial_cmp(b).expect("sem NaN"));
    let mediana = tempos[RODADAS / 2];
    let minimo = tempos[0];
    let nucleo = mediana / (1000.0 / 30.0);
    println!(
        "  {nome:<34} mediana {mediana:>7.2} ms   min {minimo:>7.2} ms   \
         {nucleo:>5.3} de núcleo a 30 fps   ({pixels_destino} px de destino)"
    );
    mediana
}

fn main() {
    println!("\n=== o que a dcv diz estar usando nesta máquina ===");
    println!("  {}", dcv_color_primitives::describe_acceleration());
    println!("  arquitetura de compilação: {}", std::env::consts::ARCH);

    let origem_bytes = tela(ORIGEM.0, ORIGEM.1);
    let passo = ORIGEM.0 * 4;
    let destino_bytes = tela(DESTINO.0, DESTINO.1);

    println!("\n=== correção: as duas concordam num quadro de tela? ===");
    conferir(&destino_bytes);

    println!("\n=== custo por quadro ===");
    let nosso_ms = cronometrar("nosso (escala 1440p→1080p + cor)", DESTINO.0 * DESTINO.1, || {
        std::hint::black_box(nosso(std::hint::black_box(&origem_bytes), passo));
    });

    let (mut y, mut u, mut v) = planos(DESTINO.0, DESTINO.1);
    let dcv_destino_ms = cronometrar("dcv, só cor, em 1080p", DESTINO.0 * DESTINO.1, || {
        pela_dcv(&destino_bytes, DESTINO.0, DESTINO.1, &mut y, &mut u, &mut v);
    });

    let (mut y2, mut u2, mut v2) = planos(ORIGEM.0, ORIGEM.1);
    let dcv_origem_ms = cronometrar("dcv, só cor, em 1440p", ORIGEM.0 * ORIGEM.1, || {
        pela_dcv(&origem_bytes, ORIGEM.0, ORIGEM.1, &mut y2, &mut u2, &mut v2);
    });

    // A via de substituição completa: converter na origem, reescalar os três
    // planos. Escolhida em vez de «reescalar BGRA e converter no destino»
    // porque I420 tem 1,5 byte por pixel contra 4 do BGRA — reescalar depois
    // toca menos de metade da memória.
    let (mut ye, mut ue, mut ve) = planos(DESTINO.0, DESTINO.1);
    let escala_ms = cronometrar("  ...mais reescalar os 3 planos", DESTINO.0 * DESTINO.1, || {
        reescalar_plano(&y2, ORIGEM, DESTINO, &mut ye);
        reescalar_plano(
            &u2,
            (ORIGEM.0.div_ceil(2), ORIGEM.1.div_ceil(2)),
            (DESTINO.0.div_ceil(2), DESTINO.1.div_ceil(2)),
            &mut ue,
        );
        reescalar_plano(
            &v2,
            (ORIGEM.0.div_ceil(2), ORIGEM.1.div_ceil(2)),
            (DESTINO.0.div_ceil(2), DESTINO.1.div_ceil(2)),
            &mut ve,
        );
    });

    // A mesma via completa, com a escala trocada por uma biblioteca com SIMD.
    let mut resizer = Resizer::new();
    let croma_origem = (ORIGEM.0.div_ceil(2), ORIGEM.1.div_ceil(2));
    let croma_destino = (DESTINO.0.div_ceil(2), DESTINO.1.div_ceil(2));
    let mut ry = Image::new(DESTINO.0 as u32, DESTINO.1 as u32, PixelType::U8);
    let mut ru = Image::new(croma_destino.0 as u32, croma_destino.1 as u32, PixelType::U8);
    let mut rv = Image::new(croma_destino.0 as u32, croma_destino.1 as u32, PixelType::U8);
    let rapida_ms = cronometrar("  ...ou reescalar com fast_image_resize", DESTINO.0 * DESTINO.1, || {
        reescalar_rapido(&mut resizer, &y2, ORIGEM, DESTINO, &mut ry);
        reescalar_rapido(&mut resizer, &u2, croma_origem, croma_destino, &mut ru);
        reescalar_rapido(&mut resizer, &v2, croma_origem, croma_destino, &mut rv);
    });

    // A reescala rápida produz os mesmos pixels que a nossa? Um ganho de 3×
    // numa função que devolve outra imagem não é ganho.
    println!("\n=== correção: a reescala da fast_image_resize bate com a nossa? ===");
    relatar("luma escalada", &ye, ry.buffer());
    relatar("croma U esc.", &ue, ru.buffer());
    relatar("croma V esc.", &ve, rv.buffer());

    println!("\n=== leitura ===");
    println!(
        "  dcv no destino é {:.1}× o nosso  (piso otimista: ignora a escala)",
        nosso_ms / dcv_destino_ms
    );
    println!(
        "  dcv na origem  é {:.1}× o nosso  (converter antes de escalar; falta a escala)",
        nosso_ms / dcv_origem_ms
    );
    let via_completa = dcv_origem_ms + escala_ms;
    println!(
        "  via completa (dcv em 1440p + escala) = {via_completa:.2} ms  →  {:.1}× o nosso",
        nosso_ms / via_completa
    );
    let via_rapida = dcv_origem_ms + rapida_ms;
    println!(
        "  via SIMD    (dcv em 1440p + fast_image_resize) = {via_rapida:.2} ms  →  {:.1}× o nosso",
        nosso_ms / via_rapida
    );
    println!(
        "\n  Lembre: 16,5 ms é o número do Ryzen. Aqui o nosso deu {nosso_ms:.2} ms.\n  \
         O que transfere para o Windows é a RAZÃO, e só se a aceleração for a mesma.\n"
    );
}

fn planos(largura: usize, altura: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let croma = largura.div_ceil(2) * altura.div_ceil(2);
    (
        vec![0u8; largura * altura],
        vec![0u8; croma],
        vec![0u8; croma],
    )
}

/// Sem isto o benchmark mede duas coisas que não fazem o mesmo trabalho.
fn conferir(bytes: &[u8]) {
    let (mut y, mut u, mut v) = planos(DESTINO.0, DESTINO.1);
    pela_dcv(bytes, DESTINO.0, DESTINO.1, &mut y, &mut u, &mut v);

    // O nosso, sem escala: origem e destino do mesmo tamanho.
    let mut nosso_y = vec![0u8; DESTINO.0 * DESTINO.1];
    let mut nosso_u = vec![0u8; DESTINO.0.div_ceil(2) * DESTINO.1.div_ceil(2)];
    let mut nosso_v = nosso_u.clone();
    for by in 0..DESTINO.1 / 2 {
        for bx in 0..DESTINO.0 / 2 {
            let ler = |x: usize, ey: usize| {
                let i = (ey * DESTINO.0 + x) * 4;
                (
                    u32::from(bytes[i + 2]),
                    u32::from(bytes[i + 1]),
                    u32::from(bytes[i]),
                )
            };
            let (ce, cd) = (ler(bx * 2, by * 2), ler(bx * 2 + 1, by * 2));
            let (be, bd) = (ler(bx * 2, by * 2 + 1), ler(bx * 2 + 1, by * 2 + 1));
            nosso_y[by * 2 * DESTINO.0 + bx * 2] = luma_de(ce);
            nosso_y[by * 2 * DESTINO.0 + bx * 2 + 1] = luma_de(cd);
            nosso_y[(by * 2 + 1) * DESTINO.0 + bx * 2] = luma_de(be);
            nosso_y[(by * 2 + 1) * DESTINO.0 + bx * 2 + 1] = luma_de(bd);
            let bloco = (
                (ce.0 + cd.0 + be.0 + bd.0) / 4,
                (ce.1 + cd.1 + be.1 + bd.1) / 4,
                (ce.2 + cd.2 + be.2 + bd.2) / 4,
            );
            let (cu, cv) = croma_de(bloco);
            nosso_u[by * DESTINO.0.div_ceil(2) + bx] = cu;
            nosso_v[by * DESTINO.0.div_ceil(2) + bx] = cv;
        }
    }

    relatar("luma", &nosso_y, &y);
    relatar("croma U", &nosso_u, &u);
    relatar("croma V", &nosso_v, &v);
}

fn relatar(nome: &str, nosso: &[u8], deles: &[u8]) {
    let mut maior = 0i32;
    let mut soma = 0i64;
    for (a, b) in nosso.iter().zip(deles) {
        let d = (i32::from(*a) - i32::from(*b)).abs();
        maior = maior.max(d);
        soma += i64::from(d);
    }
    let medio = soma as f64 / nosso.len() as f64;
    println!("  {nome:<10} diferença média {medio:>6.3}   pior pixel {maior:>3}");
}
