//! O que o SPS **declara**, contra o que o `palco-imagem.js` diz ao decodificador.
//!
//! # Por que este exemplo existe
//!
//! O `apps/seele-app/ui/palco-imagem.js` configura o `VideoDecoder` com
//! `avc1.42e0XX`. O `42` é `profile_idc = 66`: **Baseline**. O `e0` são as
//! restrições que o tornam Constrained Baseline.
//!
//! CABAC — adotado no commit `8c6661e` — **não existe em Baseline**. Se o
//! codificador subiu de perfil para poder usá-lo, a string mente, e quem recebe
//! foi informado de um fluxo que não é o que chega.
//!
//! Este exemplo lê o SPS que sai do codificador de verdade nos dois modos de
//! entropia e imprime a string que ele **deveria** ter recebido.
//!
//! ```sh
//! cargo run --release -p seele-video --example perfil
//! ```

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "sonda fora do produto; o pânico é o relatório"
)]

use std::path::PathBuf;

use seele_video::codec::{
    bytes_de_croma, bytes_de_luma, Cadencia, Codificador, ConfigDoCodificador, QuadroI420,
    Resolucao,
};
use seele_video::modulo::BibliotecaDeVideo;
use shiguredo_openh264::EntropyCodingMode;

fn quadro(largura: usize, altura: usize) -> QuadroI420 {
    let mut luma = vec![16u8; bytes_de_luma(largura, altura)];
    for (i, p) in luma.iter_mut().enumerate() {
        *p = 16 + ((i % 200) as u8);
    }
    QuadroI420::novo(
        largura,
        altura,
        luma,
        vec![128; bytes_de_croma(largura, altura)],
        vec![128; bytes_de_croma(largura, altura)],
    )
    .expect("planos consistentes")
}

/// O primeiro NAL de tipo 7 (SPS) de um fluxo Annex-B.
fn sps(bytes: &[u8]) -> Option<&[u8]> {
    let mut i = 0;
    while i + 4 < bytes.len() {
        if bytes[i..i + 3] == [0, 0, 1] && bytes[i + 3] & 0x1F == 7 {
            return Some(&bytes[i + 3..]);
        }
        if i + 5 < bytes.len() && bytes[i..i + 4] == [0, 0, 0, 1] && bytes[i + 4] & 0x1F == 7 {
            return Some(&bytes[i + 4..]);
        }
        i += 1;
    }
    None
}

fn main() -> Result<(), String> {
    let caminho = PathBuf::from(std::env::var("HOME").map_err(|_| "sem HOME")?)
        .join(".config/seele/libopenh264.dylib");
    if !caminho.exists() {
        return Err(format!("não achei o módulo em {}", caminho.display()));
    }
    let biblioteca = BibliotecaDeVideo::carregar(&caminho).map_err(|e| format!("carregar: {e}"))?;

    println!("\n  o que o palco-imagem.js manda hoje: avc1.42e0XX  (Constrained Baseline)\n");
    for (nome, entropia) in [
        ("CAVLC (antes do 8c6661e)", EntropyCodingMode::Cavlc),
        ("CABAC (hoje)", EntropyCodingMode::Cabac),
    ] {
        for resolucao in [Resolucao::P540, Resolucao::P720, Resolucao::P1080] {
            let config = ConfigDoCodificador {
                resolucao,
                cadencia: Cadencia::Q30,
                teto_bps: 4_000_000,
            };
            let mut cod = Codificador::novo_com_entropia(&biblioteca, config, entropia)
                .map_err(|e| format!("o codificador recusou: {e}"))?;
            let q = quadro(resolucao.largura(), resolucao.altura());
            let saida = cod
                .codificar(&q, true)
                .map_err(|e| format!("codificar: {e}"))?
                .ok_or("o quadro-chave não saiu")?;
            let s = sps(&saida.bytes).ok_or("não achei o SPS")?;
            let (perfil, restricoes, nivel) = (s[1], s[2], s[3]);
            let familia = match perfil {
                66 => "Baseline",
                77 => "Main",
                100 => "High",
                _ => "?",
            };
            println!(
                "  {nome:<26} {resolucao:?}: profile_idc {perfil} ({familia})  \
                 ==> deveria ser avc1.{perfil:02x}{restricoes:02x}{nivel:02x}"
            );
        }
        println!();
    }
    Ok(())
}
