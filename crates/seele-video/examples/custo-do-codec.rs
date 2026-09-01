//! Quanto custa codificar um quadro, por software e por hardware, lado a lado.
//!
//! **Mede tempo de CPU do processo, e não relógio de parede.** É a diferença
//! que decide a pergunta: o codificador do sistema trabalha num bloco dedicado,
//! e o que o nosso processo faz enquanto ele trabalha é **esperar**. Relógio de
//! parede contaria essa espera como custo e mostraria o hardware perdendo de um
//! software que está queimando núcleo — que é o avesso da verdade.
//!
//! Quem mede o tempo de CPU é quem chama, de fora:
//!
//! ```text
//! /usr/bin/time -l cargo run --release --example custo-do-codec -- software
//! /usr/bin/time -l cargo run --release --example custo-do-codec -- hardware
//! ```
//!
//! Um lado por execução, de propósito: dois num processo só somariam o custo de
//! carregar o módulo do Cisco na conta dos dois.

use std::path::PathBuf;

use seele_video::codec::{Cadencia, CodificaVideo, ConfigDoCodificador, QuadroI420, Resolucao};
use seele_video::BibliotecaDeVideo;

/// Quantos quadros. O bastante para o custo de armar diluir.
const QUADROS: usize = 300;

fn quadro(resolucao: Resolucao, passo: usize) -> Result<QuadroI420, String> {
    let (largura, altura) = (resolucao.largura(), resolucao.altura());
    let mut luma = Vec::with_capacity(largura * altura);
    for linha in 0..altura {
        for coluna in 0..largura {
            // Bordas duras, que é o conteúdo caro de uma tela de trabalho. Um
            // quadro chapado sairia com trinta bytes e mediria o nada.
            let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
            luma.push(if claro { 235 } else { 16 });
        }
    }
    let croma = vec![128_u8; largura.div_ceil(2) * altura.div_ceil(2)];
    QuadroI420::novo(largura, altura, luma, croma.clone(), croma)
        .map_err(|erro| format!("montar o quadro: {erro}"))
}

fn main() -> Result<(), String> {
    let lado = std::env::args().nth(1).unwrap_or_default();
    let resolucao = Resolucao::P1080;
    let config = ConfigDoCodificador {
        resolucao,
        cadencia: Cadencia::Q60,
        teto_bps: 4_000_000,
    };

    let mut codificador: Box<dyn CodificaVideo> = match lado.as_str() {
        "software" => {
            let caminho = std::env::var_os("SEELE_OPENH264").map_or_else(
                || {
                    PathBuf::from(std::env::var("HOME").unwrap_or_default())
                        .join(".config/seele/libopenh264.dylib")
                },
                PathBuf::from,
            );
            let biblioteca = BibliotecaDeVideo::carregar(&caminho)
                .map_err(|erro| format!("carregar o módulo: {erro}"))?;
            Box::new(
                seele_video::codec::Codificador::novo(&biblioteca, config)
                    .map_err(|erro| format!("armar o software: {erro}"))?,
            )
        }
        #[cfg(target_os = "macos")]
        "hardware" => Box::new(
            seele_video::codec::macos::Codificador::novo(&config)
                .map_err(|erro| format!("armar o hardware: {erro}"))?,
        ),
        // A linha de base: monta os quadros e não codifica nada.
        //
        // Existe porque gerar 300 quadros de 1080p com um laço por pixel custa
        // CPU de verdade, e esse custo está nas **duas** contas. Sem subtraí-lo,
        // a razão entre os dois lados sai achatada e o hardware parece pior do
        // que é.
        "nada" => {
            let quadros: Vec<QuadroI420> = (0..QUADROS)
                .map(|passo| quadro(resolucao, passo))
                .collect::<Result<_, _>>()?;
            println!(
                "nada: {} quadros montados e nenhum codificado",
                quadros.len()
            );
            return Ok(());
        }
        outro => {
            return Err(format!(
                "diga `software`, `hardware` ou `nada`, e não {outro:?}"
            ))
        }
    };

    // Os quadros são montados **antes** da medição: gerar o padrão custa mais
    // que codificá-lo, e somá-lo mediria o gerador.
    let quadros: Vec<QuadroI420> = (0..QUADROS)
        .map(|passo| quadro(resolucao, passo))
        .collect::<Result<_, _>>()?;

    let comeco = std::time::Instant::now();
    let mut bytes = 0_usize;
    let mut sairam = 0_usize;
    for (indice, q) in quadros.iter().enumerate() {
        if let Some(saiu) = codificador
            .codificar(q, indice == 0)
            .map_err(|erro| format!("codificar: {erro}"))?
        {
            bytes += saiu.bytes.len();
            sairam += 1;
        }
    }
    let parede = comeco.elapsed().as_secs_f64();

    println!(
        "{lado}: {QUADROS} quadros de 1080p, {sairam} sairam, {bytes} bytes, \
         {parede:.2}s de parede ({:.2} ms/quadro)",
        parede * 1000.0 / QUADROS as f64
    );
    Ok(())
}
