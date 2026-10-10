//! M3 da 0.16 — confere uma variante do MOD de carga contra o prelúdio da
//! cópia, sem abrir o app.
//!
//! Não é código de produto: a preparação (`preparar-api6.sh`) copia este
//! arquivo para `crates/seele-motor/examples/` da cópia da medição, e só lá ele
//! compila. Roda a metade de cliente no `Motor` de verdade, com as capacidades
//! da API 6, responde cada pedido como a janela responderia (com um retrato de
//! 27 pessoas, sete delas na sala), deixa os temporizadores correrem por
//! 1,2 s, e diz quantos pedidos de cada tipo o MOD fez. Sai com 1 se o MOD
//! tropeçou numa chamada (as linhas `m3/carga:` do `console`, que são o
//! `tentar` dele) ou se o motor recusou alguma coisa.
//!
//! Foi esta conferência que achou, na primeira versão do MOD de carga, o
//! `SeeleMods.snapshot()`, que a API 6 não tem (o retrato é `SeeleUI.retrato()`):
//! na campanha, o MOD teria parado na primeira linha, e a medida seria a de um
//! MOD que não faz nada. E que o palco com 200 figuras completas passava dos
//! 12 KiB de uma mensagem (`mensagem-grande`).
//!
//! ```text
//! cargo run --release -p seele-motor --example conferir-carga -- <cliente/main.js>
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used)]

use seele_motor::{Fila, Interrupcao, Limites, Motor, ParaOFora};
use std::collections::BTreeMap;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// `CAPACIDADES_POR_API[6]`, a lista da casca (`base.js`) e do
/// `seele_proto::mods::capacidades_da_api`.
const API_6: &[&str] = &[
    "superficies", "contribuicoes", "estilos", "classes", "volume",
    "arquivo", "tema", "retrato", "eventos-da-tela", "aviso-do-servidor",
];

/// O retrato da API 6 (`projetarORetrato`): quem usa é a pessoa 1, a sala 1
/// tem sete pessoas, e o servidor tem 27.
fn retrato() -> serde_json::Value {
    let pessoas: Vec<serde_json::Value> = (1..=27)
        .map(|id| serde_json::json!({ "id": id, "apelido": format!("m3-{id}"), "falando": false, "mudo": false }))
        .collect();
    serde_json::json!({
        "eu": 1,
        "servidor": { "nome": "M3" },
        "pessoas": pessoas,
        "salas": [{ "id": 1, "nome": "Sala principal", "limite": 8, "pessoas": [1, 2, 3, 4, 5, 6, 7], "minha": true }],
        "canais": [{ "id": 1, "nome": "geral", "aberto": true }],
        "onde": "computador",
        "regioes": ["redes", "servidor.navegacao", "operador.barra", "canal.cabecalho",
                    "conversa.mensagem", "compositor", "pessoas.lista", "status.barra",
                    "chamada.palco", "pagina", "dialogo", "folha", "aviso"],
    })
}

fn main() {
    let caminho = std::env::args().nth(1).expect("uso: conferir-carga <cliente/main.js>");
    let fonte = std::fs::read_to_string(&caminho).expect("ler a metade de cliente");
    let (manda, recebe) = mpsc::channel();
    let limites = Limites::default();
    let interrupcao = Arc::new(Interrupcao::nova(limites));
    let fila = Arc::new(Fila::default());
    let mut motor = Motor::subir(&manda, &interrupcao, &fila, limites).expect("o motor subiu");
    motor.codigo(&fonte, API_6);

    let mut pedidos: BTreeMap<String, usize> = BTreeMap::new();
    let mut maior = 0_usize;
    let mut tropecos: Vec<String> = Vec::new();
    let fim = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < fim {
        while let Ok(saida) = recebe.try_recv() {
            match saida {
                ParaOFora::Mensagem(json) => {
                    // Entregue: o lugar dela na fila volta, como na janela
                    // (`executor.rs`, `self.fila.tirar(json.len())`).
                    fila.tirar(json.len());
                    maior = maior.max(json.len());
                    let pedido: serde_json::Value = serde_json::from_str(&json).expect("o pedido é JSON");
                    let tipo = pedido["tipo"].as_str().unwrap_or("?").to_owned();
                    *pedidos.entry(tipo.clone()).or_default() += 1;
                    if let Some(n) = pedido["n"].as_u64() {
                        let valor = match tipo.as_str() {
                            "retrato" => retrato(),
                            "superficie-criar" => serde_json::json!({
                                "superficie": pedido["descricao"]["id"], "reaproveitada": false }),
                            "contribuir" => serde_json::json!({ "handle": format!("h{n}") }),
                            "pedido" => serde_json::json!({ "ok": true }),
                            _ => serde_json::Value::Null,
                        };
                        motor.resposta(&serde_json::json!({ "tipo": "resposta", "n": n, "ok": true, "valor": valor }).to_string());
                    }
                }
                ParaOFora::Console { texto, .. } => {
                    if texto.contains("m3/carga") || texto.contains("Error") {
                        tropecos.push(texto);
                    }
                }
                ParaOFora::Falhou(motivo) => tropecos.push(format!("falhou: {motivo}")),
                ParaOFora::Interrompido => tropecos.push("interrompido".to_owned()),
                ParaOFora::Parou | ParaOFora::Diagnostico { .. } => {}
            }
        }
        match motor.proximo_prazo() {
            Some(prazo) if prazo.is_zero() => motor.vencer(),
            Some(prazo) => std::thread::sleep(prazo.min(Duration::from_millis(5))),
            None => std::thread::sleep(Duration::from_millis(5)),
        }
        if motor.proximo_prazo().is_some_and(|prazo| prazo.is_zero()) {
            motor.vencer();
        }
    }
    println!(
        "{}",
        serde_json::json!({ "pedidos": pedidos, "maior_mensagem": maior, "tropecos": tropecos })
    );
    if !tropecos.is_empty() {
        std::process::exit(1);
    }
}
