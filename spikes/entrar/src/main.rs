//! Entrar num link e dizer **onde parou**.
//!
//! # Por que isto existe
//!
//! O app entra por uma janela gráfica e o `plug` por um TUI, e os dois precisam
//! de terminal ou de tela. Numa investigação de campo o que se quer é a
//! travessia sem nada em volta: qual candidato foi tentado, nessa ordem, e com
//! que erro ela terminou.
//!
//! Sem isso a única evidência disponível era a frase que a pessoa lê — «tempo
//! esgotado na sincronização inicial» — que diz o desfecho e não o caminho.
//!
//! Usa a mesma `Plug::connect_watching` que o app, e as mesmas etapas: o que se
//! mede aqui é o que acontece lá.

use std::sync::Arc;

use seele_ffi::{ConnectConfig, Event, EventListener, Plug};

/// Imprime cada etapa da travessia com o instante em que ela chegou.
struct Narrador(std::time::Instant);

impl EventListener for Narrador {
    fn on_event(&self, evento: Event) {
        if let Event::ConnectStageChanged { stage } = evento {
            println!("[{:>6} ms] {stage:?}", self.0.elapsed().as_millis());
        }
    }
}

fn main() {
    let Some(link) = std::env::args().nth(1) else {
        eprintln!("uso: spike-entrar <seele://…>");
        std::process::exit(2);
    };

    let convite = match seele_ffi::uri::analisar(&link) {
        Ok(convite) => convite,
        Err(erro) => {
            eprintln!("o link não foi lido: {erro:?}");
            std::process::exit(2);
        }
    };

    println!("alvo       : {}", convite.alvo);
    println!("alternativos: {:?}", convite.alternativos);
    println!("bilhete    : {:?}", convite.bilhete);
    println!("impressão  : {:?}", convite.impressao_digital);
    println!("---");

    let comeco = std::time::Instant::now();
    let config = ConnectConfig {
        server: convite.alvo.clone(),
        alternate_servers: convite.alternativos.clone(),
        nickname: std::env::args().nth(2).unwrap_or_else(|| "sonda".to_owned()),
        home: std::env::var("SEELE_HOME").unwrap_or_else(|_| "/tmp/seele-sonda".to_owned()),
        join_secret: convite.token.clone(),
        expected_fingerprint: convite.impressao_digital.clone(),
        bilhete: convite.bilhete.clone(),
        // Sem áudio: o que se mede é a travessia, e uma placa de som ausente
        // não pode ser o motivo de a medição não acontecer.
        audio: false,
        capture_device: None,
        playback_device: None,
    };

    match Plug::connect_watching(config, Arc::new(Narrador(comeco))) {
        Ok((plug, veredito)) => {
            let snapshot = plug.snapshot();
            println!("---");
            println!("ENTROU em {} ms", comeco.elapsed().as_millis());
            println!("dogma    : {}", snapshot.dogma);
            println!("caminho  : {:?}", snapshot.caminho);
            println!("veredito : {veredito:?}");
        }
        Err(erro) => {
            println!("---");
            println!("NÃO ENTROU em {} ms: {erro:?}", comeco.elapsed().as_millis());
        }
    }
}
