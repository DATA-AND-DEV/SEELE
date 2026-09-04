//! Quanta qualidade cada codificador entrega pelo mesmo teto de banda.
//!
//! # Por que isto é teste e não exemplo
//!
//! Porque medição que só uma pessoa consegue rodar não é medição: é anedota. A
//! pergunta que este arquivo responde — «o codificador do sistema entrega pior
//! que o OpenH264?» — nasceu de um relato de campo, «está mais pixelado que
//! antes», e a resposta muda por máquina, por driver e por sistema. Como teste,
//! qualquer um a refaz na máquina dele, e o CI a refaz nos três sistemas.
//!
//! **Ele não reprova por qualidade.** Um limiar de PSNR fixo reprovaria hardware
//! honesto num laptop de outra geração, e um teste que acusa por causa da
//! máquina em que roda ensina a ser ignorado. O que ele exige é o que é sempre
//! verdade: os dois lados produzem fluxo que abre. O número vai para a saída,
//! para quem estiver perguntando.
//!
//! # O conteúdo, e por que não é um xadrez
//!
//! A primeira versão desta medida usava o xadrez de 235 e 16 que os outros
//! testes usam, e mediu nada: o codificador de hardware do macOS o devolveu com
//! 83 dB de PSNR — praticamente sem perda — porque um padrão perfeitamente
//! regular é fácil demais. Tela de verdade tem degradê, borda dura de texto e
//! ruído de foto, e é onde a diferença que alguém chama de «pixelado» aparece.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use seele_video::codec::{
    armar, Cadencia, CodificaVideo, Codificador, ConfigDoCodificador, Decodificador, QuadroI420,
    Resolucao,
};
use seele_video::{modulo, BibliotecaDeVideo};

/// Quantos quadros. O bastante para o quadro-chave diluir na média.
const QUADROS: usize = 120;

/// Ruído **de valor**: uma grade grossa sorteada e interpolada.
///
/// # Por que não é `rand()` por pixel, e por que isso importava
///
/// A primeira versão deste arquivo sorteava um byte por pixel. Isso não é «foto
/// ou vídeo dentro da janela» — é **ruído branco**, que é incompressível por
/// construção: não há correlação entre vizinhos, então nenhuma predição acerta
/// e nenhuma transformada concentra energia. O codificador não tem o que fazer
/// com ele em bitrate nenhum.
///
/// O efeito na medida era esconder tudo o que ela existia para mostrar: a
/// tabela de cadências media 16 dB em 1080p a **3,0 bits por pixel**, um regime
/// onde qualquer conteúdo de verdade estaria acima de 40. Comparações entre
/// linhas continuavam válidas — o conteúdo era o mesmo dos dois lados —, mas o
/// **nível** não dizia nada, e foi por pouco que ele não virou argumento para
/// mexer nos limiares de `seele_core::tela`.
///
/// Foto tem estrutura: regiões suaves, bordas, e detalhe correlacionado na
/// escala de alguns pixels. Ruído de valor é a maneira mais barata de ter isso
/// — uma grade a cada [`CELULA`] pixels, interpolada — e não custa dependência
/// nenhuma, que era a razão de o gerador congruencial estar aqui.
fn ruido_de_valor(x: usize, y: usize, passo: usize) -> f64 {
    /// De quantos em quantos pixels a grade é sorteada.
    ///
    /// Oito, que é o lado do bloco de transformada do H.264 em perfil High: é a
    /// escala em que um codificador de verdade decide, então é a escala em que
    /// o conteúdo tem de ter o que decidir.
    const CELULA: usize = 8;

    let sortear = |cx: usize, cy: usize| -> f64 {
        let semente = 0x2545_F491_4F6C_DD1D_u64
            ^ (cx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (cy as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ (passo as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
        let semente = semente
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((semente >> 33) % 256) as f64
    };

    let (cx, cy) = (x / CELULA, y / CELULA);
    let (fx, fy) = (
        (x % CELULA) as f64 / CELULA as f64,
        (y % CELULA) as f64 / CELULA as f64,
    );
    let cima = sortear(cx, cy) + (sortear(cx + 1, cy) - sortear(cx, cy)) * fx;
    let baixo = sortear(cx, cy + 1) + (sortear(cx + 1, cy + 1) - sortear(cx, cy + 1)) * fx;
    cima + (baixo - cima) * fy
}

/// Um quadro com as três coisas que uma tela de trabalho tem.
///
/// Degradê — que é onde o banding aparece —, bordas duras — que é o texto — e
/// um bloco de detalhe correlacionado, que é a foto ou o vídeo dentro da janela.
/// Ver [`ruido_de_valor`] para por que o terceiro não é ruído branco.
fn quadro_de_tela(resolucao: Resolucao, passo: usize) -> QuadroI420 {
    let (largura, altura) = (resolucao.largura(), resolucao.altura());
    let mut luma = Vec::with_capacity(largura * altura);
    for linha in 0..altura {
        for coluna in 0..largura {
            let valor = if linha < altura / 3 {
                // Degradê horizontal, com a fase andando a cada quadro.
                u8::try_from(((coluna + passo * 4) * 255 / largura.max(1)) % 256).unwrap_or(0)
            } else if linha < altura * 2 / 3 {
                // Bordas duras: barras finas, como linhas de texto.
                if ((coluna + passo) / 3).is_multiple_of(2) {
                    235
                } else {
                    16
                }
            } else {
                // Detalhe correlacionado, andando com o quadro para dar trabalho
                // à predição entre quadros — que é o que um vídeo tocando faz.
                u8::try_from(ruido_de_valor(coluna + passo, linha, passo / 8) as u32 % 256)
                    .unwrap_or(0)
            };
            luma.push(valor);
        }
    }
    let croma = vec![128_u8; largura.div_ceil(2) * altura.div_ceil(2)];
    QuadroI420::novo(largura, altura, luma, croma.clone(), croma).expect("os planos do quadro")
}

/// PSNR do plano de luma, em decibéis.
///
/// Acima de 40 dB a diferença é invisível; abaixo de 30 dB é o que alguém chama
/// de pixelado. A escala é logarítmica: 3 dB é o dobro de erro.
fn psnr(original: &[u8], voltou: &[u8]) -> Option<f64> {
    if original.len() != voltou.len() || original.is_empty() {
        return None;
    }
    let soma: f64 = original
        .iter()
        .zip(voltou)
        .map(|(a, b)| {
            let d = f64::from(*a) - f64::from(*b);
            d * d
        })
        .sum();
    let erro = soma / original.len() as f64;
    if erro == 0.0 {
        return Some(99.0);
    }
    Some(10.0 * (255.0_f64 * 255.0 / erro).log10())
}

fn pastas() -> Vec<PathBuf> {
    let mut pastas = Vec::new();
    if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
        let caminho = PathBuf::from(apontado);
        pastas.push(if caminho.is_dir() {
            caminho
        } else {
            caminho.parent().map_or(caminho.clone(), PathBuf::from)
        });
    }
    pastas
}

/// O resultado de uma passada: qualidade e quanto do teto foi gasto.
struct Medida {
    psnr: f64,
    mbps: f64,
    sairam: usize,
}

fn medir(
    codificador: &mut dyn CodificaVideo,
    quadros: &[QuadroI420],
    biblioteca: &BibliotecaDeVideo,
) -> Medida {
    let mut espelho = Decodificador::novo(biblioteca).expect("armar o espelho");
    let mut pendentes = std::collections::VecDeque::new();
    let (mut soma, mut medidos, mut bytes, mut sairam) = (0.0_f64, 0_usize, 0_usize, 0_usize);

    for (indice, quadro) in quadros.iter().enumerate() {
        let Ok(Some(saiu)) = codificador.codificar(quadro, indice == 0) else {
            continue;
        };
        bytes += saiu.bytes.len();
        sairam += 1;
        pendentes.push_back(quadro);
        // Sem reordenação nos dois caminhos, a ordem de saída é a de entrada.
        if let Ok(Some(voltou)) = espelho.decodificar(&saiu.bytes) {
            if let Some(origem) = pendentes.pop_front() {
                if let Some(psnr) = psnr(origem.luma(), voltou.luma()) {
                    soma += psnr;
                    medidos += 1;
                }
            }
        }
    }

    // **A cadência do codificador, e não um 30 escrito à mão.**
    //
    // Enquanto toda medida rodava a 30 quadros isto estava certo. Deixou de
    // estar quando `ajustar_quadros` passou a existir: medir 120 quadros a 8 por
    // segundo e dividir por 30 diz que saíram quatro vezes mais bits do que
    // saíram, e a tabela que compara cadências mentiria exatamente na coluna
    // que ela existe para comparar.
    let segundos = quadros.len() as f64 / f64::from(codificador.quadros_por_segundo().max(1));
    Medida {
        psnr: if medidos == 0 {
            0.0
        } else {
            soma / medidos as f64
        },
        mbps: bytes as f64 * 8.0 / segundos / 1e6,
        sairam,
    }
}

#[test]
fn o_codificador_do_sistema_ao_lado_do_de_software() {
    let Ok(biblioteca) = BibliotecaDeVideo::procurar_e_carregar(&pastas()) else {
        assert!(
            std::env::var_os("SEELE_EXIGE_CODEC").is_none()
                || modulo::publicado_para_este_sistema().is_none(),
            "SEELE_EXIGE_CODEC está ligado e o módulo não está aqui"
        );
        eprintln!("PULADO: sem o módulo do Cisco não há com o que comparar.");
        return;
    };

    let resolucao = Resolucao::P720;
    let teto_bps = 2_000_000;
    let config = ConfigDoCodificador {
        resolucao,
        cadencia: Cadencia::Q30,
        teto_bps,
    };
    let quadros: Vec<QuadroI420> = (0..QUADROS)
        .map(|passo| quadro_de_tela(resolucao, passo))
        .collect();

    let mut software = Codificador::novo(&biblioteca, config).expect("armar o software");
    let software = medir(&mut software, &quadros, &biblioteca);

    // `armar` é quem escolhe, e é o que o produto usa: medir outra coisa aqui
    // seria medir algo que ninguém executa.
    let mut escolhido = armar(&biblioteca, config).expect("armar o do sistema");
    let sistema = medir(escolhido.as_mut(), &quadros, &biblioteca);

    eprintln!(
        "MEDIDA 720p, teto {:.1} Mbps, {QUADROS} quadros:\n  \
         software  {:.2} dB  {:.2} Mbps ({:.0}% do teto)  {} quadros\n  \
         sistema   {:.2} dB  {:.2} Mbps ({:.0}% do teto)  {} quadros",
        f64::from(teto_bps) / 1e6,
        software.psnr,
        software.mbps,
        software.mbps * 1e6 * 100.0 / f64::from(teto_bps),
        software.sairam,
        sistema.psnr,
        sistema.mbps,
        sistema.mbps * 1e6 * 100.0 / f64::from(teto_bps),
        sistema.sairam,
    );

    // O que é sempre verdade, e a única coisa que este teste reprova: os dois
    // produzem fluxo que abre. Um limiar de qualidade fixo reprovaria hardware
    // honesto de outra geração.
    assert!(
        software.sairam > 0 && sistema.sairam > 0,
        "um dos dois não produziu quadro nenhum"
    );
    assert!(
        software.psnr > 0.0 && sistema.psnr > 0.0,
        "um dos dois produziu fluxo que o decodificador não abriu"
    );
}

/// Quanta qualidade cada cadência compra pelo mesmo teto.
///
/// # Por que esta tabela faltava
///
/// O §2 fixa a regra — «a resolução segura, o quadro cede» — e `cadencia_para`
/// a implementa. Mas a régua que ela usa, [`seele_core::tela::bits_por_quadro`],
/// é **0,10 bits por pixel**, e esse número foi escolhido por argumento e não
/// por medida: é onde se diz que a borda de uma fonte para de virar bloco.
///
/// Esta tabela é o que permite conferir. Se 0,10 bpp for generoso, a coluna do
/// PSNR mostra o ganho achatando antes dele; se for apertado, ela mostra o
/// PSNR ainda subindo depois. Como o resto deste arquivo, **não reprova por
/// qualidade** — imprime, e quem estiver perguntando lê.
#[test]
fn quanto_a_cadencia_compra_pelo_mesmo_teto() {
    let Ok(biblioteca) = BibliotecaDeVideo::procurar_e_carregar(&pastas()) else {
        eprintln!("PULADO: sem o módulo do Cisco não há com o que comparar.");
        return;
    };

    eprintln!("\n  resolução | teto  | quadros |   PSNR   |  Mbps  | saíram | bits/pixel");
    for resolucao in [Resolucao::P540, Resolucao::P720, Resolucao::P1080] {
        let quadros: Vec<QuadroI420> = (0..QUADROS)
            .map(|passo| quadro_de_tela(resolucao, passo))
            .collect();
        let pixels = (resolucao.largura() * resolucao.altura()) as f64;
        for teto_bps in [1_200_000_u32, 6_000_000, 12_480_000] {
            for pedido in [8_u32, 15, 30, 60] {
                let mut codificador = armar(
                    &biblioteca,
                    ConfigDoCodificador {
                        resolucao,
                        cadencia: Cadencia::Q60,
                        teto_bps,
                    },
                )
                .expect("armar o codificador");
                let valendo = codificador
                    .ajustar_quadros(pedido)
                    .expect("ajustar a cadência");
                let medida = medir(codificador.as_mut(), &quadros, &biblioteca);
                let bpp = f64::from(teto_bps) / f64::from(valendo) / pixels;
                eprintln!(
                    "  {:>9?} | {:>4} k | {valendo:>7} | {:>5.2} dB | {:>6.2} | {:>3}/{QUADROS} | {bpp:>10.3}",
                    resolucao,
                    teto_bps / 1000,
                    medida.psnr,
                    medida.mbps,
                    medida.sairam,
                );
            }
        }
    }
}
