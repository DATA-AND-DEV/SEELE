//! Tira a prévia de cada fonte desta máquina, como a caixa de compartilhar
//! faz, e diz o que veio. Com um caminho, grava a primeira em PPM.
//!
//! ```sh
//! cargo run -p seele-core --example previas_de_tela -- /tmp/previa.ppm
//! ```

#![allow(clippy::print_stdout, missing_docs)]

use std::io::Write as _;
use std::time::Instant;

fn main() {
    let fontes = match seele_core::fontes_de_tela() {
        Ok(fontes) => fontes,
        Err(erro) => {
            println!("não deu para listar: {erro}");
            return;
        }
    };
    println!("{} fontes", fontes.len());
    let mut gravar = std::env::args().nth(1).filter(|a| a != "--juntas");
    if std::env::args().any(|a| a == "--juntas") {
        // Como o produto faz: abre todas, e só então espera cada uma.
        let comeco = Instant::now();
        let abertas: Vec<_> = fontes
            .iter()
            .take(12)
            .map(|fonte| (fonte.rotulo().to_owned(), fonte.comecar_previa()))
            .collect();
        println!(
            "{:>4} ms para abrir {}",
            comeco.elapsed().as_millis(),
            abertas.len()
        );
        let abertas = abertas
            .into_iter()
            .filter_map(|(rotulo, aberta)| aberta.ok().map(|aberta| (rotulo, aberta)))
            .collect();
        seele_core::colher_previas(
            abertas,
            seele_core::PRAZO_DA_PREVIA,
            seele_core::PreviaEmCurso::olhar,
            |rotulo, _| println!("{:>4} ms · veio · {rotulo}", comeco.elapsed().as_millis()),
        );
        println!("{:>4} ms · fim", comeco.elapsed().as_millis());
        return;
    }
    for fonte in fontes.iter().take(12) {
        let comeco = Instant::now();
        let previa = fonte
            .comecar_previa()
            .map(|em_curso| em_curso.esperar(seele_core::PRAZO_DA_PREVIA));
        match previa {
            Ok(Some(previa)) => {
                let claros = previa.rgb.iter().filter(|b| **b > 24).count();
                println!(
                    "{:>4} ms · {}×{} · {}% de bytes acima do preto · {}",
                    comeco.elapsed().as_millis(),
                    previa.largura,
                    previa.altura,
                    claros * 100 / previa.rgb.len().max(1),
                    fonte.rotulo()
                );
                if let Some(caminho) = gravar.take() {
                    if let Ok(mut arquivo) = std::fs::File::create(&caminho) {
                        let _ = write!(arquivo, "P6\n{} {}\n255\n", previa.largura, previa.altura);
                        let _ = arquivo.write_all(&previa.rgb);
                    }
                }
            }
            Ok(None) => println!(
                "{:>4} ms · sem quadro no prazo · {}",
                comeco.elapsed().as_millis(),
                fonte.rotulo()
            ),
            Err(erro) => println!("não abriu ({erro}) · {}", fonte.rotulo()),
        }
    }
}
