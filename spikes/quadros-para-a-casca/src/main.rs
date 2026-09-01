//! Gera quadros H.264 de verdade e os escreve em JSON, para a casca poder ser
//! testada sem uma tela nem um servidor.
//!
//! **Por que isto existe.** O compartilhamento de tela parou de funcionar nos
//! dois sistemas ao mesmo tempo, sem erro na tela. A captura foi provada aqui —
//! `captura_um_quadro_da_tela_de_verdade` passa neste Mac — e o codec também,
//! por `tests/ida_e_volta.rs`. O que sobrava sem prova era a metade de quem
//! recebe: `palco-imagem.js`, o `VideoDecoder` da janela e o canvas.
//!
//! Aquela metade só roda num navegador, e um navegador não tem como pedir
//! quadros a um servidor durante um teste. Então os quadros vêm daqui: os mesmos
//! bytes que o `Codificador` produz em produção, em base64, prontos para serem
//! entregues ao `quadroDaTela` da casca por `tools/carga-da-casca.py`.
//!
//! Descartável no sentido do `spikes/`: se a prova virar guarda, ela vira um
//! arquivo de quadros gravado, e este binário sai.

use seele_video::codec::{Cadencia, Codificador, ConfigDoCodificador, QuadroI420, Resolucao};
use seele_video::modulo::BibliotecaDeVideo;

/// Um xadrez que se move, para um quadro delta ter o que descrever.
fn quadro(resolucao: Resolucao, passo: usize) -> QuadroI420 {
    let (largura, altura) = (resolucao.largura() as usize, resolucao.altura() as usize);
    let mut luma = vec![16u8; largura * altura];
    for y in 0..altura {
        for x in 0..largura {
            let quadrado = ((x + passo * 8) / 64 + y / 64) % 2 == 0;
            luma[y * largura + x] = if quadrado { 235 } else { 16 };
        }
    }
    let croma = vec![128u8; (largura / 2) * (altura / 2)];
    QuadroI420::novo(
        resolucao.largura(),
        resolucao.altura(),
        luma,
        croma.clone(),
        croma,
    )
    .expect("um quadro do tamanho certo")
}

fn main() {
    let caminho = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("uso: quadros-para-a-casca <pasta com o módulo de vídeo>");
        std::process::exit(2)
    });
    let biblioteca = BibliotecaDeVideo::procurar_e_carregar(&[std::path::PathBuf::from(&caminho)])
        .expect("o módulo de vídeo desta máquina");

    let resolucao = Resolucao::P540;
    let mut codificador = Codificador::novo(
        &biblioteca,
        ConfigDoCodificador {
            resolucao,
            cadencia: Cadencia::Q30,
            teto_bps: 0,
        },
    )
    .expect("armar o codificador");

    let mut saida = String::from("{\n");
    saida.push_str(&format!(
        "  \"largura\": {},\n  \"altura\": {},\n  \"quadros\": [\n",
        resolucao.largura(),
        resolucao.altura()
    ));
    let mut primeiro = true;
    for passo in 0..6 {
        let Some(pronto) = codificador
            .codificar(&quadro(resolucao, passo), false)
            .expect("codificar")
        else {
            continue;
        };
        if !primeiro {
            saida.push_str(",\n");
        }
        primeiro = false;
        saida.push_str(&format!(
            "    {{ \"chave\": {}, \"bytes\": \"{}\" }}",
            pronto.chave,
            base64(&pronto.bytes)
        ));
    }
    saida.push_str("\n  ]\n}\n");
    println!("{saida}");
}

/// Base64 sem dependência: são poucos quadros e o alfabeto cabe aqui.
fn base64(bytes: &[u8]) -> String {
    const ALFABETO: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut saida = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for pedaco in bytes.chunks(3) {
        let mut buffer = [0u8; 3];
        buffer[..pedaco.len()].copy_from_slice(pedaco);
        let n = u32::from_be_bytes([0, buffer[0], buffer[1], buffer[2]]);
        for i in 0..4 {
            if i <= pedaco.len() {
                let indice = ((n >> (18 - 6 * i)) & 0x3F) as usize;
                saida.push(ALFABETO[indice] as char);
            } else {
                saida.push('=');
            }
        }
    }
    saida
}
