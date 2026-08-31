//! A descrição de cor atravessa: o SPS ganha VUI e o decodificador aceita.
//!
//! # O que se prova aqui, e por que não em `cargo test`
//!
//! `crate::vui` tem testes de unidade, e eles cobrem o que se observa sem
//! encoder: que o que não se entende não se mexe, que escapar e desescapar
//! fecham um no outro, que o leitor de Exp-Golomb lê o que a norma manda.
//!
//! O que eles **não** podem provar é que o SPS reescrito continua sendo um SPS
//! — para isso é preciso um encoder de verdade emitindo um, e um decodificador
//! de verdade aceitando o resultado. Montar um SPS à mão para o teste provaria
//! que o código aceita o que ele mesmo escreveu, o que é circular.
//!
//! O módulo do Cisco é baixado sob consentimento pelo app, então esta prova não
//! pode ser exigida de `cargo test` — ela mora aqui.
//!
//! ```sh
//! cargo run --release -p seele-video --example cor
//! ```

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "medição fora do produto; o pânico é o relatório"
)]

use std::path::PathBuf;

use seele_video::codec::{
    bytes_de_croma, bytes_de_luma, Cadencia, Codificador, ConfigDoCodificador, Decodificador,
    QuadroI420, Resolucao,
};
use seele_video::modulo::BibliotecaDeVideo;

fn quadro(largura: usize, altura: usize, n: usize) -> QuadroI420 {
    let mut y = vec![16_u8; bytes_de_luma(largura, altura)];
    for (i, pixel) in y.iter_mut().enumerate() {
        *pixel = (((i + n * 37) % 220) + 16) as u8;
    }
    QuadroI420::novo(
        largura,
        altura,
        y,
        vec![128; bytes_de_croma(largura, altura)],
        vec![128; bytes_de_croma(largura, altura)],
    )
    .expect("planos consistentes")
}

/// Acha o NAL de SPS (tipo 7) num fluxo Annex B.
fn sps(fluxo: &[u8]) -> Option<&[u8]> {
    let mut i = 0;
    while i + 4 < fluxo.len() {
        let tamanho = if fluxo[i..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if fluxo[i..].starts_with(&[0, 0, 1]) {
            3
        } else {
            i += 1;
            continue;
        };
        let comeco = i + tamanho;
        let mut fim = comeco;
        while fim < fluxo.len()
            && !fluxo[fim..].starts_with(&[0, 0, 1])
            && !fluxo[fim..].starts_with(&[0, 0, 0, 1])
        {
            fim += 1;
        }
        if fluxo.get(comeco).is_some_and(|c| c & 0x1F == 7) {
            return Some(&fluxo[comeco..fim]);
        }
        i = fim;
    }
    None
}

fn main() -> Result<(), String> {
    let caminho = PathBuf::from(std::env::var("HOME").map_err(|_| "sem HOME")?)
        .join(".config/seele/libopenh264.dylib");
    if !caminho.exists() {
        return Err(format!("não achei o módulo em {}", caminho.display()));
    }
    let biblioteca =
        BibliotecaDeVideo::carregar(&caminho).map_err(|erro| format!("carregar: {erro}"))?;

    // 540p **de propósito**: é a resolução em que o defeito aparece. Com 540
    // linhas ela fica abaixo das 576 do corte, e sem VUI o decodificador
    // adivinha BT.601 sobre um quadro que a captura fez em BT.709.
    for (resolucao, nome) in [(Resolucao::P540, "540p"), (Resolucao::P720, "720p")] {
        let (largura, altura) = (resolucao.largura(), resolucao.altura());
        let mut codificador = Codificador::novo(
            &biblioteca,
            ConfigDoCodificador {
                resolucao,
                cadencia: Cadencia::Q30,
                teto_bps: 6_000_000,
            },
        )
        .map_err(|erro| format!("codificador: {erro}"))?;
        let mut decodificador =
            Decodificador::novo(&biblioteca).map_err(|erro| format!("decodificador: {erro}"))?;

        let mut viu_sps = false;
        let mut voltaram = 0_usize;
        for n in 0..10 {
            let Ok(Some(codificado)) = codificador.codificar(&quadro(largura, altura, n), n == 0)
            else {
                continue;
            };
            if let Some(sps) = sps(&codificado.bytes) {
                viu_sps = true;
                // **Reescrever de novo não pode mexer.** É a prova de que a cor
                // já está lá que não depende de reimplementar o leitor de bits
                // neste arquivo: `com_descricao_de_cor` devolve o fluxo intacto
                // exatamente quando a cor já foi declarada.
                //
                // Procurar os três bytes de BT.709 crus não serviria: eles caem
                // em posição arbitrária de bit, e um `windows(3)` sobre bytes só
                // os acharia por acidente de alinhamento — foi assim que a
                // primeira redação deste exemplo deu falso negativo.
                let mut annexb = vec![0_u8, 0, 0, 1];
                annexb.extend_from_slice(sps);
                let tem_cor = seele_video::vui::com_descricao_de_cor(&annexb) == annexb;
                println!(
                    "{nome}: SPS com {} bytes, descrição de cor {}",
                    sps.len(),
                    if tem_cor { "presente" } else { "AUSENTE" }
                );
                if !tem_cor {
                    return Err(format!("{nome}: o SPS saiu sem a descrição de cor"));
                }
            }
            match decodificador.decodificar(&codificado.bytes) {
                Ok(Some(volta)) => {
                    if volta.largura() == largura && volta.altura() == altura {
                        voltaram += 1;
                    }
                }
                Ok(None) => {}
                Err(erro) => {
                    return Err(format!(
                        "{nome}: o decodificador recusou o fluxo com VUI: {erro}"
                    ))
                }
            }
        }
        if !viu_sps {
            return Err(format!("{nome}: nenhum SPS saiu do encoder"));
        }
        println!("{nome}: {voltaram} de 10 quadros voltaram inteiros\n");
    }
    Ok(())
}
