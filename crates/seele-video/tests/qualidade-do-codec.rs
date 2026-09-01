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

/// Um quadro com as três coisas que uma tela de trabalho tem.
///
/// Degradê — que é onde o banding aparece —, bordas duras — que é o texto — e
/// um bloco de ruído determinístico, que é a foto ou o vídeo dentro da janela.
/// O ruído sai de um gerador congruencial escrito aqui: uma dependência a mais
/// para gerar número aleatório num teste é uma dependência a mais para sempre.
fn quadro_de_tela(resolucao: Resolucao, passo: usize) -> QuadroI420 {
    let (largura, altura) = (resolucao.largura(), resolucao.altura());
    let mut luma = Vec::with_capacity(largura * altura);
    let mut semente = 0x2545_F491_4F6C_DD1D_u64 ^ (passo as u64);
    for linha in 0..altura {
        for coluna in 0..largura {
            semente = semente
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
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
                // Ruído, que é o que nenhum codificador comprime de graça.
                u8::try_from((semente >> 33) % 256).unwrap_or(0)
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

    let segundos = quadros.len() as f64 / 30.0;
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
