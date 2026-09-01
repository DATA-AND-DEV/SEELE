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

use seele_video::codec::{
    Cadencia, CodificaVideo, ConfigDoCodificador, Decodificador, QuadroI420, Resolucao,
};
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

/// Quadros da tela de verdade, para medir sobre o que as pessoas transmitem.
#[cfg(target_os = "macos")]
fn capturar(resolucao: Resolucao, cadencia: Cadencia) -> Result<Vec<QuadroI420>, String> {
    use seele_video::captura::macos::{fontes, CapturaDaTela, Fonte};

    let lista = fontes().map_err(|erro| format!("listar as fontes: {erro}"))?;
    let monitor = lista
        .iter()
        .find(|f| matches!(f, Fonte::Monitor { .. }))
        .ok_or_else(|| "esta máquina não tem monitor".to_owned())?;
    let captura = CapturaDaTela::iniciar(monitor, resolucao, cadencia)
        .map_err(|erro| format!("iniciar a captura: {erro}"))?;

    let mut colhidos = Vec::with_capacity(QUADROS);
    let limite = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while colhidos.len() < QUADROS && std::time::Instant::now() < limite {
        match captura.tomar() {
            Some(da_tela) => colhidos.push(da_tela.quadro),
            None => std::thread::sleep(std::time::Duration::from_millis(5)),
        }
    }
    captura
        .parar()
        .map_err(|erro| format!("parar a captura: {erro}"))?;
    if colhidos.len() < QUADROS {
        return Err(format!(
            "a tela só entregou {} quadros em trinta segundos — mexa uma janela \
             enquanto isto roda, porque tela parada não produz quadro",
            colhidos.len()
        ));
    }
    Ok(colhidos)
}

#[cfg(not(target_os = "macos"))]
fn capturar(_: Resolucao, _: Cadencia) -> Result<Vec<QuadroI420>, String> {
    Err("capturar a tela de verdade só está escrito para o macOS".to_owned())
}

/// O decodificador que serve de espelho, quando o módulo está por perto.
fn espelho() -> Result<Option<Decodificador>, String> {
    let caminho = std::env::var_os("SEELE_OPENH264").map_or_else(
        || {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".config/seele/libopenh264.dylib")
        },
        PathBuf::from,
    );
    if !caminho.exists() {
        return Ok(None);
    }
    let biblioteca =
        BibliotecaDeVideo::carregar(&caminho).map_err(|erro| format!("carregar: {erro}"))?;
    Ok(Some(
        Decodificador::novo(&biblioteca).map_err(|erro| format!("armar o espelho: {erro}"))?,
    ))
}

/// PSNR do plano de luma, em decibéis. `None` quando os tamanhos não batem.
///
/// Acima de 40 dB a diferença é invisível; abaixo de 30 dB ela é o que alguém
/// chama de «pixelado». A escala é logarítmica: 3 dB é o dobro de erro.
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
    //
    // **`SEELE_TELA_REAL` troca o padrão pela tela desta máquina**, e a primeira
    // medida foi inútil sem isso: o xadrez de 235 e 16 sai do codificador de
    // hardware com 83 dB de PSNR — praticamente sem perda — porque é fácil
    // demais. Conteúdo de tela de verdade tem texto, degradê e foto, e é onde a
    // diferença que alguém chama de «pixelado» aparece.
    let quadros: Vec<QuadroI420> = if std::env::var_os("SEELE_TELA_REAL").is_some() {
        capturar(resolucao, config.cadencia)?
    } else {
        (0..QUADROS)
            .map(|passo| quadro(resolucao, passo))
            .collect::<Result<_, _>>()?
    };

    // O decodificador é só do medidor, e por isso é opcional: sem o módulo do
    // Cisco a medida de custo continua valendo e a de qualidade some.
    //
    // **A qualidade é a pergunta que o tamanho não responde.** Um codificador
    // que gasta um terço dos bits pode estar comprimindo melhor ou entregando
    // pior, e só comparar a imagem que volta com a que entrou separa os dois.
    // PSNR sobre o plano de luma: é a métrica grosseira e universal, e a
    // diferença que interessa aqui — «está mais pixelado» — é grande o bastante
    // para ela enxergar.
    let mut espelho = espelho()?;
    let mut soma_psnr = 0.0_f64;
    let mut medidos = 0_usize;
    let mut pendentes: std::collections::VecDeque<&QuadroI420> = std::collections::VecDeque::new();

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
            pendentes.push_back(q);
            if let Some(decodificador) = espelho.as_mut() {
                // Sem reordenação, a ordem de saída é a de entrada — e é o que
                // permite parear o que voltou com o que entrou.
                if let Ok(Some(voltou)) = decodificador.decodificar(&saiu.bytes) {
                    if let Some(origem) = pendentes.pop_front() {
                        if let Some(psnr) = psnr(origem.luma(), voltou.luma()) {
                            soma_psnr += psnr;
                            medidos += 1;
                        }
                    }
                }
            }
        }
    }
    let parede = comeco.elapsed().as_secs_f64();

    // **Quanto do orçamento foi gasto.** É a pergunta que o tamanho sozinho não
    // responde: um codificador que gasta um terço do teto pode estar comprimindo
    // melhor ou entregando pior, e é a comparação entre os dois lados no mesmo
    // conteúdo que separa. Relato de campo que trouxe isto para cá: «está mais
    // pixelado que antes», logo depois de o codec do sistema entrar.
    if medidos > 0 {
        println!(
            "{lado}: PSNR médio {:.2} dB em {medidos} quadros comparados",
            soma_psnr / medidos as f64
        );
    } else {
        println!("{lado}: sem medida de qualidade (o módulo do Cisco não estava aqui)");
    }
    let segundos = QUADROS as f64 / f64::from(config.cadencia.hz());
    let alcancado = bytes as f64 * 8.0 / segundos;
    println!(
        "{lado}: {QUADROS} quadros de 1080p, {sairam} sairam, {bytes} bytes, \
         {parede:.2}s de parede ({:.2} ms/quadro)\n\
         {lado}: {:.2} Mbps alcançados de {:.2} Mbps de teto ({:.0}% do orçamento)",
        parede * 1000.0 / QUADROS as f64,
        alcancado / 1e6,
        f64::from(config.teto_bps) / 1e6,
        alcancado * 100.0 / f64::from(config.teto_bps)
    );
    Ok(())
}
