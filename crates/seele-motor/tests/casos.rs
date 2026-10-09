//! A tabela da volta (`tests/casos.json`), no nativo: o [`Motor`] embrulhado
//! como o desktop o embrulha, com o canal e a fila.
//!
//! O `cargo xtask web` roda a mesma tabela dentro do `.wasm`
//! (`xtask/web/motor.mjs`), com o `Motor` embrulhado como o celular o
//! embrulha. A tabela é a mesma nas duas pontas, e é ela que diz o que a volta
//! faz: um caso que passa só numa delas é uma volta que divergiu.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_motor::{Fila, Interrupcao, Limites, Motor, ParaOFora, SONDA};
use serde_json::Value;

/// Lê o que o motor disse, devolvendo o crédito como a janela devolve.
fn colher(recebe: &Receiver<ParaOFora>, fila: &Fila, sem_ler: bool, falas: &mut Vec<Value>) {
    for fala in recebe.try_iter() {
        if !sem_ler {
            match &fala {
                ParaOFora::Mensagem(json) => fila.tirar(json.len()),
                ParaOFora::Falhou(_) | ParaOFora::Interrompido => fila.tirar_aviso(),
                _ => {}
            }
        }
        falas.push(fala.em_json());
    }
}

/// Espera `ms` atendendo os prazos do motor, como a thread do desktop.
fn esperar(
    motor: &mut Motor,
    recebe: &Receiver<ParaOFora>,
    fila: &Fila,
    sem_ler: bool,
    falas: &mut Vec<Value>,
    ms: u64,
) {
    let fim = Instant::now() + Duration::from_millis(ms);
    loop {
        let agora = Instant::now();
        let resta = fim.saturating_duration_since(agora);
        match motor.proximo_prazo() {
            Some(prazo) if prazo <= resta => {
                std::thread::sleep(prazo);
                motor.vencer();
                colher(recebe, fila, sem_ler, falas);
            }
            _ => {
                std::thread::sleep(resta);
                return;
            }
        }
    }
}

fn rodar(tabela: &Value, caso: &Value) -> Vec<Value> {
    let (manda, recebe) = channel();
    let limites = Limites::default();
    let interrupcao = Arc::new(Interrupcao::nova(limites));
    let fila = Arc::new(Fila::default());
    let sem_ler = caso["sem_ler"].as_bool().unwrap_or(false);
    let capacidades: Vec<&str> = caso
        .get("capacidades")
        .unwrap_or(&tabela["capacidades_padrao"])
        .as_array()
        .expect("as capacidades")
        .iter()
        .map(|c| c.as_str().expect("uma capacidade"))
        .collect();
    let fonte = if caso["sonda"].as_bool() == Some(true) {
        SONDA
    } else {
        caso["fonte"].as_str().expect("a fonte")
    };
    let mut falas = Vec::new();
    let Some(mut motor) = Motor::subir(&manda, &interrupcao, &fila, limites) else {
        colher(&recebe, &fila, sem_ler, &mut falas);
        return falas;
    };
    motor.codigo(fonte, &capacidades);
    colher(&recebe, &fila, sem_ler, &mut falas);
    for passo in caso["passos"].as_array().map_or(&[][..], Vec::as_slice) {
        if let Some(json) = passo["responder"].as_str() {
            motor.resposta(json);
        } else if let Some(ms) = passo["esperar_ms"].as_u64() {
            esperar(&mut motor, &recebe, &fila, sem_ler, &mut falas, ms);
        } else if passo["diagnostico"].as_bool() == Some(true) {
            motor.diagnostico();
        } else {
            panic!("passo desconhecido: {passo}");
        }
        colher(&recebe, &fila, sem_ler, &mut falas);
    }
    motor.parar();
    colher(&recebe, &fila, sem_ler, &mut falas);
    // A diferença declarada da sonda: no nativo, cada global tem de existir, e
    // sai do corpo antes de comparar (o `.wasm` cobra que ela não exista).
    for so_daqui in caso["globais_so_do_nativo"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
    {
        let marca = format!(",{},", so_daqui.as_str().expect("um nome"));
        let corpo = falas[0]["corpo"]
            .as_str()
            .expect("a sonda fala primeiro")
            .to_owned();
        assert!(
            corpo.contains(&marca),
            "o nativo deixou de ter {marca}: a diferença declarada mudou"
        );
        falas[0]["corpo"] = Value::String(corpo.replacen(&marca, ",", 1));
    }
    falas
}

/// O que difere entre o esperado e o que veio, ou nada.
fn diferenca(esperado: &[Value], veio: &[Value]) -> Option<String> {
    let mut lista = Vec::new();
    for item in esperado {
        for _ in 0..item["vezes"].as_u64().unwrap_or(1) {
            lista.push(item);
        }
    }
    if lista.len() != veio.len() {
        return Some(format!(
            "{} falas esperadas, vieram {}",
            lista.len(),
            veio.len()
        ));
    }
    for (n, (e, v)) in lista.iter().zip(veio).enumerate() {
        let texto = |campo: &str| v[campo].as_str().unwrap_or("");
        let ok = e["tipo"] == v["tipo"]
            && e.get("corpo").is_none_or(|c| c == &v["corpo"])
            && e.get("corpo_comeca")
                .is_none_or(|c| texto("corpo").starts_with(c.as_str().unwrap_or("")))
            && e.get("texto").is_none_or(|t| t == &v["texto"])
            && e.get("texto_contem")
                .is_none_or(|t| texto("texto").contains(t.as_str().unwrap_or("")))
            && e.get("nivel").is_none_or(|t| t == &v["nivel"])
            && e.get("suprimidas").is_none_or(|t| t == &v["suprimidas"])
            && e.get("relogios").is_none_or(|t| t == &v["relogios"]);
        if !ok {
            return Some(format!("a fala {n}: esperava {e}, veio {v}"));
        }
    }
    None
}

#[test]
fn a_tabela_da_volta_passa_no_nativo() {
    let tabela: Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/casos.json"))
            .expect("a tabela"),
    )
    .expect("a tabela é JSON");
    let casos = tabela["casos"].as_array().expect("os casos");
    assert!(
        casos.len() >= 13,
        "a tabela encolheu para {} casos",
        casos.len()
    );
    let mut falhas = Vec::new();
    for caso in casos {
        let veio = rodar(&tabela, caso);
        if let Some(d) = diferenca(caso["esperado"].as_array().expect("o esperado"), &veio) {
            falhas.push(format!(
                "{}: {d}\n    veio: {}",
                caso["de"],
                Value::Array(veio)
            ));
        }
    }
    assert!(
        falhas.is_empty(),
        "{} caso(s):\n  {}",
        falhas.len(),
        falhas.join("\n  ")
    );
}
