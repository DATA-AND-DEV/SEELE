// Segunda metade do spike: os dois tetos que o ADR 0044 promete existem?
// Sem eles, um MOD em laço infinito é a diferença entre a sala funcionar e não.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn main() {
    // 1. Teto de memória.
    let rt = rquickjs::Runtime::new().expect("runtime");
    rt.set_memory_limit(1024 * 1024); // 1 MiB
    let ctx = rquickjs::Context::full(&rt).expect("contexto");
    let estourou = ctx.with(|ctx| {
        ctx.eval::<(), _>("const a = []; for (let i = 0; i < 1e9; i++) a.push(i);")
            .is_err()
    });
    println!("teto_de_memoria_para_o_mod={estourou}");

    // A sala continua de pé depois do estouro?
    let vivo = ctx.with(|ctx| ctx.eval::<i64, _>("1 + 1").unwrap_or(-1));
    println!("o_contexto_sobrevive_ao_estouro={}", vivo == 2);

    // 2. Teto de tempo, por interrupção.
    let rt2 = rquickjs::Runtime::new().expect("runtime 2");
    let passos = Arc::new(AtomicUsize::new(0));
    let contador = Arc::clone(&passos);
    rt2.set_interrupt_handler(Some(Box::new(move || {
        contador.fetch_add(1, Ordering::Relaxed) > 10_000
    })));
    let ctx2 = rquickjs::Context::full(&rt2).expect("contexto 2");
    let inicio = std::time::Instant::now();
    let parou = ctx2.with(|ctx| ctx.eval::<(), _>("while (true) {}").is_err());
    println!(
        "laco_infinito_foi_interrompido={parou} em_ms={:.1}",
        inicio.elapsed().as_secs_f64() * 1000.0
    );

    let vivo2 = ctx2.with(|ctx| ctx.eval::<i64, _>("2 + 2").unwrap_or(-1));
    println!("o_contexto_sobrevive_a_interrupcao={}", vivo2 == 4);
}
