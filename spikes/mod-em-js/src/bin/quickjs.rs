// Spike do ADR 0045: quanto custa um interpretador JS dentro de um SFU de
// 1 vCPU / 512 MB. Mede o que decide, e nada além.
//
// O programa é o mesmo do `boa.rs` de propósito: comparar dois motores com dois
// programas diferentes é comparar os programas.
use std::time::Instant;

fn main() {
    let contextos: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1);
    let chamadas: usize = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(100_000);

    let inicio = Instant::now();
    let rt = rquickjs::Runtime::new().expect("runtime");
    let mut ctxs = Vec::new();
    for _ in 0..contextos {
        let ctx = rquickjs::Context::full(&rt).expect("contexto");
        ctx.with(|ctx| {
            ctx.eval::<(), _>("globalThis.aoEntrar = (n) => n * 2 + 1;")
                .expect("eval");
        });
        ctxs.push(ctx);
    }
    let subiu = inicio.elapsed();

    let inicio = Instant::now();
    let mut soma: i64 = 0;
    ctxs[0].with(|ctx| {
        let f: rquickjs::Function = ctx.globals().get("aoEntrar").expect("função");
        for i in 0..chamadas {
            soma += f.call::<_, i64>((i as i64,)).expect("chamada");
        }
    });
    let chamou = inicio.elapsed();

    println!(
        "motor=quickjs contextos={contextos} subida_ms={:.1} chamadas={chamadas} total_ms={:.1} por_chamada_ns={:.0} soma={soma}",
        subiu.as_secs_f64() * 1000.0,
        chamou.as_secs_f64() * 1000.0,
        chamou.as_nanos() as f64 / chamadas as f64
    );
}
