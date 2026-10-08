//! O motor do celular, rodado no nativo com um anfitrião de teste.
//!
//! O que se prova aqui é o que não depende do alvo: o prelúdio é o do desktop,
//! o MOD comum fala a API inteira, e um laço sem fim é cortado no prazo da
//! volta. **O teto de memória não se prova aqui**: no nativo o alocador padrão
//! já responde o tamanho dos blocos, e o teste passaria com ou sem o
//! `RustAllocator`. Quem o prova é o `cargo xtask web`, dentro do `.wasm`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use seele_motor_web::{Anfitriao, Motor, PRELUDIO};

/// As capacidades da API 5, a mais nova publicada (`seele-proto/src/mods.rs`).
const CAPS: [&str; 9] = [
    "regiao",
    "tema",
    "cartoes",
    "arquivo",
    "superficies",
    "contribuicoes",
    "estilos",
    "classes",
    "volume",
];

/// Um anfitrião que guarda o que o MOD disse.
struct Anotador {
    inicio: Instant,
    falas: RefCell<Vec<serde_json::Value>>,
    console: RefCell<Vec<String>>,
}

impl Anfitriao for Anotador {
    fn postar(&self, json: &str) -> bool {
        self.falas
            .borrow_mut()
            .push(serde_json::from_str(json).expect("o prelúdio posta JSON"));
        true
    }
    fn console(&self, metodo: &str, texto: &str) {
        self.console.borrow_mut().push(format!("{metodo}: {texto}"));
    }
    fn agora_ms(&self) -> f64 {
        self.inicio.elapsed().as_secs_f64() * 1000.0
    }
}

fn anotador() -> Rc<Anotador> {
    Rc::new(Anotador {
        inicio: Instant::now(),
        falas: RefCell::default(),
        console: RefCell::default(),
    })
}

fn caps() -> Vec<String> {
    CAPS.iter().map(|c| (*c).to_owned()).collect()
}

/// A volta da janela, num laço só: responde cada pedido como a página
/// responderia e dispara os relógios, até `pronto` chegar. Um `setTimeout`
/// dispara uma vez; um `setInterval` dispara a cada volta até ser cancelado.
fn ate_o_pronto(motor: &Motor, anfitriao: &Anotador) -> serde_json::Value {
    let mut respondidas = 0;
    let mut intervalos: Vec<u32> = Vec::new();
    for _ in 0..200 {
        let pendentes: Vec<serde_json::Value> = anfitriao.falas.borrow()[respondidas..].to_vec();
        respondidas += pendentes.len();
        for fala in pendentes {
            if fala["canal"] == "pronto" {
                return fala["valor"].clone();
            }
            let Some(n) = fala["n"].as_u64() else {
                continue;
            };
            let resposta = match fala["tipo"].as_str() {
                Some("snapshot") => {
                    serde_json::json!({ "n": n, "ok": true, "valor": { "servidor": "bancada" } })
                }
                Some("pedido") if fala["id"] == "eco" => {
                    serde_json::json!({ "n": n, "ok": true, "valor": fala["valor"] })
                }
                Some("regiao") => serde_json::json!({ "n": n, "ok": true, "valor": null }),
                _ => serde_json::json!({ "n": n, "ok": false, "erro": "desconhecido" }),
            };
            motor.entregar(&resposta.to_string()).expect("a volta");
        }
        let pedidos: Vec<serde_json::Value> = serde_json::from_str(&motor.pedidos_de_relogio())
            .expect("os pedidos de relógio são JSON");
        let mut agora: Vec<u32> = intervalos.clone();
        for p in pedidos {
            let id = u32::try_from(p["id"].as_u64().unwrap()).unwrap();
            if p["cancelar"] == true {
                intervalos.retain(|i| *i != id);
                agora.retain(|i| *i != id);
            } else if p["repete"] == true {
                intervalos.push(id);
                agora.push(id);
            } else {
                agora.push(id);
            }
        }
        for id in agora {
            motor.relogio(id).expect("o relógio");
        }
    }
    panic!(
        "o MOD comum não chegou ao pronto; disse {:?}",
        anfitriao.falas.borrow()
    );
}

#[test]
fn o_preludio_e_o_texto_do_executor_do_desktop() {
    let executor = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/seele-app/src/executor.rs"),
    )
    .expect("o executor do desktop");
    assert!(
        executor.contains(&format!("const PRELUDIO: &str = r#\"{PRELUDIO}\"#;")),
        "o prelúdio do celular não é o texto do desktop: os dois executores dariam APIs diferentes"
    );
}

#[test]
fn o_mod_comum_fala_a_api_do_desktop() {
    let fonte =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mod-comum.js"))
            .expect("o MOD comum");
    let anfitriao = anotador();
    let motor = Motor::subir(&caps(), &fonte, anfitriao.clone()).expect("o motor sobe");
    let pronto = ate_o_pronto(&motor, &anfitriao);

    assert_eq!(pronto["snap"]["servidor"], "bancada");
    assert_eq!(pronto["capacidades"], serde_json::json!(CAPS));
    assert_eq!(
        pronto["intervalos"], 3,
        "o setInterval do prelúdio bateu três vezes e foi cancelado"
    );
    assert_eq!(pronto["erro"], "rejeitou: desconhecido");
    // 7000 «é» são 14 000 bytes em UTF-8, mais o envelope: o prelúdio conta
    // bytes, e não unidades UTF-16, e recusa antes de postar.
    assert!(
        pronto["grande"]
            .as_str()
            .unwrap()
            .starts_with("mensagem-grande: pedido tem 14062 bytes"),
        "{}",
        pronto["grande"]
    );
    assert_eq!(pronto["superficies"], "object");
    assert_eq!(pronto["console"], "function");
    assert!(anfitriao
        .console
        .borrow()
        .iter()
        .any(|l| l == "log: mod comum de pé"));
}

#[test]
fn um_laco_sem_fim_e_cortado_no_prazo_da_volta() {
    // Numa thread própria, para que um motor sem interruptor reprove o teste em
    // vez de pendurá-lo. O motor não é `Send`, então nasce lá dentro.
    let (manda, recebe) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let anfitriao = anotador();
        let motor = Motor::subir(
            &caps(),
            "SeeleUI.aoEvento(() => { while (true) {} });",
            anfitriao,
        )
        .expect("o motor sobe");
        let inicio = Instant::now();
        let volta = motor.entregar(r#"{"tipo":"evento","nome":"x"}"#);
        let _ = manda.send((volta, inicio.elapsed()));
    });
    let (volta, levou) = recebe
        .recv_timeout(Duration::from_secs(5))
        .expect("o laço sem fim não foi cortado em 5 s");
    let erro = volta.expect_err("um laço sem fim tem de virar erro da volta");
    assert!(erro.contains("interrupted"), "{erro}");
    // O prazo é 500 ms; o relógio só é consultado a cada 1024 interrupções.
    assert!(levou < Duration::from_secs(3), "o corte levou {levou:?}");
}
