// Mesmo spike, outro motor. O programa é o mesmo de propósito: comparar dois
// motores com dois programas diferentes é comparar os programas.
use boa_engine::{js_string, Context, JsValue, Source};
use std::time::Instant;

fn main() {
    let contextos: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let chamadas: usize = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(100_000);

    let inicio = Instant::now();
    let mut ctxs = Vec::new();
    for _ in 0..contextos {
        let mut ctx = Context::default();
        ctx.eval(Source::from_bytes("globalThis.aoEntrar = (n) => n * 2 + 1;")).expect("eval");
        ctxs.push(ctx);
    }
    let subiu = inicio.elapsed();

    let inicio = Instant::now();
    let mut soma: i64 = 0;
    {
        let ctx = &mut ctxs[0];
        let global = ctx.global_object();
        let f = global.get(js_string!("aoEntrar"), ctx).expect("função");
        let f = f.as_callable().expect("callable").clone();
        for i in 0..chamadas {
            let r = f.call(&JsValue::undefined(), &[JsValue::from(i as i32)], ctx).expect("chamada");
            soma += r.as_number().unwrap_or(0.0) as i64;
        }
    }
    let chamou = inicio.elapsed();

    println!("motor=boa contextos={contextos} subida_ms={:.1} chamadas={chamadas} total_ms={:.1} por_chamada_ns={:.0} soma={soma}",
        subiu.as_secs_f64() * 1000.0,
        chamou.as_secs_f64() * 1000.0,
        chamou.as_nanos() as f64 / chamadas as f64);
}
