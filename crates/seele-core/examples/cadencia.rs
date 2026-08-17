//! Mede a volta do laço de voz nesta máquina, sem microfone e sem rede.
//!
//! # A pergunta que isto responde
//!
//! Duas máquinas na mesma sala, o mesmo binário, a mesma conversa: uma ouve
//! perfeitamente e a outra ouve picotado. O codec é o mesmo, o protocolo é o
//! mesmo, o caminho é o mesmo — e o texto atravessa inteiro nos dois sentidos o
//! tempo todo. Sobra a máquina.
//!
//! O laço de voz de `seele-core` empurra um quadro de 20 ms de áudio para o
//! dispositivo por volta. Enquanto a volta durar bem menos que 20 ms, isso é
//! bastante. Assim que ela durar **mais**, o laço entrega menos áudio do que o
//! relógio pede, para sempre, e o alto-falante pica. Nada no áudio recebido
//! explicaria o buraco, o que é exatamente por que essa falha se disfarça de
//! problema de rede.
//!
//! E a volta não dura o mesmo em toda parte: ela é feita de um
//! `timeout(1 ms)` e de um `sleep(2 ms)`, e quanto esses dois custam de verdade
//! depende da granularidade do temporizador do sistema operacional. Onde o
//! temporizador é fino, três milissegundos. Onde é grosso, dezenas.
//!
//! Este exemplo dá a volta do laço, com a mesma forma que o laço de verdade
//! tem, e imprime o número. **Rodar isto nas duas máquinas responde a pergunta**
//! — não há hipótese envolvida, é a leitura de um relógio.
//!
//! # Como ler o que sai
//!
//! - `volta p50` bem abaixo de 20 ms: o laço acompanha sozinho. Não é daqui.
//! - `volta p99` ou `pior` acima de 20 ms: esta máquina não entrega 50 quadros
//!   por segundo pela via normal, e a reprodução só fica em dia porque o
//!   `PlayoutClock` repõe. Antes de haver reposição, aqui é onde o áudio sumia.
//! - `reposição` acima de zero é a mesma coisa dita em quadros.
//! - `entregue sem repor` é o que a versão anterior deste código teria posto no
//!   anel: se der 64%, 36% do áudio desta máquina era silêncio inventado.
//!
//! Rodar com:
//! `cargo run --release --package seele-core --example cadencia`

use std::time::{Duration, Instant};

use seele_audio::playout::PlayoutClock;
use seele_audio::FRAME_MS;

/// Quanto tempo medir. Longo o bastante para pegar uma soneca do escalonador.
const DURACAO: Duration = Duration::from_secs(10);

/// O que o laço de voz espera por um datagrama antes de seguir em frente.
const ESPERA_DE_MIDIA: Duration = Duration::from_millis(1);

/// O que o laço de voz dorme no fim de cada volta.
const SONECA: Duration = Duration::from_millis(2);

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!(
        "medindo {} s da volta do laço de voz ({} ms de espera de mídia + {} ms de soneca)\n",
        DURACAO.as_secs(),
        ESPERA_DE_MIDIA.as_millis(),
        SONECA.as_millis()
    );

    let inicio = Instant::now();
    let mut relogio = PlayoutClock::new(inicio, FRAME_MS);
    let mut voltas: Vec<f64> = Vec::with_capacity(4096);
    let mut sem_repor = 0_u64;
    let mut anterior = inicio;

    while anterior.duration_since(inicio) < DURACAO {
        // A mesma forma do laço de verdade: uma espera curta por mídia que na
        // prática expira quase sempre, e uma soneca no fim. O canal aqui nunca
        // entrega nada, que é o caso de quem está calado — e é justamente
        // quando o áudio dos outros tem que sair sem falhar.
        let _ = tokio::time::timeout(ESPERA_DE_MIDIA, std::future::pending::<()>()).await;

        let agora = Instant::now();
        let vencidos = relogio.due(agora);
        if vencidos > 0 {
            // O que a versão anterior teria produzido: um, nunca mais.
            sem_repor += 1;
        }
        voltas.push((agora - anterior).as_secs_f64() * 1000.0);
        anterior = agora;

        tokio::time::sleep(SONECA).await;
    }

    let medido = relogio.metrics();
    voltas.sort_by(f64::total_cmp);

    println!("voltas             {}", voltas.len());
    println!("volta p50          {:.2} ms", percentil(&voltas, 0.50));
    println!("volta p99          {:.2} ms", percentil(&voltas, 0.99));
    println!("volta pior         {:.2} ms", percentil(&voltas, 1.0));
    println!("quadro             {FRAME_MS} ms");
    println!();
    println!("atraso pior        {:.2} ms", medido.worst_lateness_ms);
    println!("quadros devidos    {}", medido.frames_due);
    println!("reposição          {}", medido.catchup_frames);
    println!("reacertos          {}", medido.resyncs);

    if medido.frames_due > 0 {
        let fracao = sem_repor as f64 / medido.frames_due as f64 * 100.0;
        println!("entregue sem repor {fracao:.1}%");
        println!();
        if fracao > 99.0 {
            println!(
                "VEREDITO: esta máquina acompanha o relógio. A volta cabe dentro de um \
                 quadro, então o áudio picotado que ela ouvir não nasce aqui."
            );
        } else {
            println!(
                "VEREDITO: esta máquina NÃO acompanha o relógio pela via normal. Sem \
                 reposição ela entregaria {fracao:.1}% do áudio ao alto-falante, e os \
                 outros {:.1}% sairiam como silêncio — audivelmente picotado, e sem \
                 nada na rede que explicasse.",
                100.0 - fracao
            );
        }
    }
}

/// O valor no percentil pedido de uma amostra já ordenada.
fn percentil(ordenado: &[f64], fracao: f64) -> f64 {
    if ordenado.is_empty() {
        return 0.0;
    }
    let ultimo = ordenado.len() - 1;
    let posicao = (fracao * ultimo as f64).round().max(0.0) as usize;
    ordenado.get(posicao.min(ultimo)).copied().unwrap_or(0.0)
}
