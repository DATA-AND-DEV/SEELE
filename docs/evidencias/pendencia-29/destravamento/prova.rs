mod vaga;
use std::time::Instant;
fn main() {
    let inicio = Instant::now();
    // quem trava: toma a vaga e nunca a devolve (vaza o guarda de proposito)
    std::thread::spawn(|| { std::mem::forget(vaga::minha()); std::thread::park(); });
    std::thread::sleep(std::time::Duration::from_millis(300));
    // vigia: se a fila nunca andar, o programa morre em 30s com saida 9
    std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_secs(30)); println!("TRAVOU: 30s e nem todos seguiram"); std::process::exit(9); });
    let seguintes: Vec<_> = (0..8).map(|n| std::thread::spawn(move || {
        let _v = vaga::minha();
        println!("teste {n} seguiu aos {:?}", inicio.elapsed());
    })).collect();
    for s in seguintes { s.join().unwrap(); }
    println!("TODOS SEGUIRAM em {:?}", inicio.elapsed());
}
