//! CAVLC contra CABAC, no mesmo conteúdo e no mesmo teto.
//!
//! # A pergunta
//!
//! `codec.rs` escolhe CAVLC com esta justificativa:
//!
//! > CAVLC é o que faz o OpenH264 escolher o perfil baseline; CABAC o levaria a
//! > High. O §2 pediu baseline, e é também o único perfil que ele sabe
//! > codificar.
//!
//! O binding diz outra coisa, no próprio fonte: «PRO_MAIN/PRO_HIGH são aceitos
//! por `InitializeExt`, mas a capacidade real fica em **Constrained Baseline +
//! CABAC**; a transformada 8×8 (exigida por High) não está implementada.»
//!
//! Se o binding estiver certo, CABAC não leva a High — e a escolha de CAVLC
//! está recusando compressão de graça por um motivo que não existe.
//!
//! Este exemplo mede em vez de decidir por leitura.
//!
//! # Como rodar
//!
//! ```sh
//! cargo run --release -p seele-video --example entropia
//! ```
//!
//! Ele precisa do módulo do Cisco, no lugar onde o produto o guarda.

#![allow(
    clippy::expect_used,
    reason = "medição fora do produto; um plano inconsistente aqui é defeito deste arquivo, e o pânico é o relatório"
)]

use std::path::PathBuf;

use seele_video::codec::Decodificador;
use seele_video::codec::{
    bytes_de_croma, bytes_de_luma, Cadencia, Codificador, ConfigDoCodificador, QuadroI420,
    Resolucao,
};
use seele_video::modulo::BibliotecaDeVideo;
use shiguredo_openh264::EntropyCodingMode;

/// Quadros de teste com movimento, que é o que jogo tem e texto não.
///
/// Um gradiente que desliza mais um bloco que salta. Não é um jogo, e não
/// pretende ser: o que importa é que **haja** movimento e detalhe, porque num
/// quadro parado os dois modos de entropia empatam e a medida não diria nada.
fn quadro_com_movimento(largura: usize, altura: usize, n: usize) -> QuadroI420 {
    let desloca = (n * 7) % largura;
    let mut y = vec![16_u8; bytes_de_luma(largura, altura)];
    for linha in 0..altura {
        for coluna in 0..largura {
            let valor = (((coluna + desloca) % 256) ^ ((linha * 3) % 256)) as u8;
            if let Some(pixel) = y.get_mut(linha * largura + coluna) {
                *pixel = valor.clamp(16, 235);
            }
        }
    }
    // Um bloco que salta, para haver movimento de verdade entre quadros.
    let bloco = 64;
    let topo = (n * 11) % altura.saturating_sub(bloco).max(1);
    let esquerda = (n * 13) % largura.saturating_sub(bloco).max(1);
    for linha in topo..(topo + bloco).min(altura) {
        for coluna in esquerda..(esquerda + bloco).min(largura) {
            if let Some(pixel) = y.get_mut(linha * largura + coluna) {
                *pixel = 235;
            }
        }
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

fn medir(
    biblioteca: &BibliotecaDeVideo,
    cabac: bool,
    resolucao: Resolucao,
    teto_bps: u32,
    quantos: usize,
) -> Result<(usize, usize), String> {
    let config = ConfigDoCodificador {
        resolucao,
        cadencia: Cadencia::Q30,
        teto_bps,
    };
    let entropia = if cabac {
        EntropyCodingMode::Cabac
    } else {
        EntropyCodingMode::Cavlc
    };
    let mut codificador = Codificador::novo_com_entropia(biblioteca, config, entropia)
        .map_err(|erro| format!("o codificador recusou: {erro}"))?;

    let (largura, altura) = (resolucao.largura(), resolucao.altura());
    let (mut bytes, mut saidos) = (0_usize, 0_usize);
    for n in 0..quantos {
        let quadro = quadro_com_movimento(largura, altura, n);
        match codificador.codificar(&quadro, n == 0) {
            Ok(Some(codificado)) => {
                bytes += codificado.bytes.len();
                saidos += 1;
            }
            Ok(None) => {}
            Err(erro) => return Err(format!("quadro {n}: {erro}")),
        }
    }
    Ok((bytes, saidos))
}

fn main() -> Result<(), String> {
    let caminho = PathBuf::from(std::env::var("HOME").map_err(|_| "sem HOME")?)
        .join(".config/seele/libopenh264.dylib");
    if !caminho.exists() {
        return Err(format!(
            "não achei o módulo em {}. Ele é baixado sob consentimento pelo app; \
             sem ele esta medição não roda.",
            caminho.display()
        ));
    }
    let biblioteca =
        BibliotecaDeVideo::carregar(&caminho).map_err(|erro| format!("carregar: {erro}"))?;

    // **Antes da tabela: o outro lado decodifica?**
    //
    // É a pergunta que decide se a economia pode ser adotada. A spec §2 escolheu
    // H.264 porque «é o codec que o outro lado fala», e uma economia que só o
    // nosso decodificador entende não é economia, é incompatibilidade.
    //
    // Pelo padrão, CABAC exige perfil Main; o binding afirma que a capacidade
    // fica em Constrained Baseline + CABAC. Quem resolve isso não é a leitura, é
    // um quadro atravessando.
    {
        let config = ConfigDoCodificador {
            resolucao: Resolucao::P720,
            cadencia: Cadencia::Q30,
            teto_bps: 6_000_000,
        };
        let mut codificador =
            Codificador::novo_com_entropia(&biblioteca, config, EntropyCodingMode::Cabac)
                .map_err(|erro| format!("codificador CABAC: {erro}"))?;
        let mut decodificador =
            Decodificador::novo(&biblioteca).map_err(|erro| format!("decodificador: {erro}"))?;
        let (largura, altura) = (1280_usize, 720_usize);
        let mut voltaram = 0_usize;
        for n in 0..10 {
            let quadro = quadro_com_movimento(largura, altura, n);
            if let Ok(Some(codificado)) = codificador.codificar(&quadro, n == 0) {
                match decodificador.decodificar(&codificado.bytes) {
                    Ok(Some(volta)) => {
                        if volta.largura() == largura && volta.altura() == altura {
                            voltaram += 1;
                        }
                    }
                    Ok(None) => {}
                    Err(erro) => return Err(format!("o decodificador recusou CABAC: {erro}")),
                }
            }
        }
        println!("ida e volta com CABAC: {voltaram} de 10 quadros voltaram inteiros\n");
    }

    println!("resolução  teto      CAVLC        CABAC        diferença");
    for (resolucao, nome) in [(Resolucao::P540, "540p"), (Resolucao::P720, "720p")] {
        for teto in [1_200_000_u32, 6_000_000] {
            let (cavlc, _) = medir(&biblioteca, false, resolucao, teto, 90)?;
            let (cabac, _) = medir(&biblioteca, true, resolucao, teto, 90)?;
            // Menos bytes no mesmo teto e no mesmo conteúdo é mais qualidade por
            // bit: o controle de taxa gastou menos para dizer a mesma coisa.
            let diferenca = 100.0 - (cabac as f64 / cavlc as f64) * 100.0;
            println!(
                "{nome:<10} {:<9} {cavlc:<12} {cabac:<12} {diferenca:+.1}%",
                format!("{} kbps", teto / 1000)
            );
        }
    }
    Ok(())
}
