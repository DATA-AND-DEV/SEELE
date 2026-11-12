use super::*;

/// **O sim de quem hospeda é gravado com o banco ainda na mão** (e2e de
/// 31/10, passo 1). A sessão dele cai com `ModsMudaram` assim que o banco
/// é solto, e a volta sozinha (`voltarSozinhoSeOConjuntoEMeu`) só passa se
/// o sim já está no disco. Gravado depois do cadeado, a queda podia chegar
/// antes, e quem ouviu «você não será perguntado de novo» lia «entre de
/// novo». Mover a gravação para depois do `drop(persistence)` reprova.
#[test]
fn o_sim_e_gravado_antes_de_soltar_o_banco() {
    let fonte = fonte_sem_comentarios();
    let corpo = corpo_de(&fonte, concat!("async fn aplicar_conjunto_de_mods", "("));
    let define = corpo.find(concat!("definir_", "conjunto("));
    let grava = corpo.find(concat!("gravar_o_sim_de_quem_", "hospeda("));
    let solta = corpo.find(concat!("drop(", "persistence)"));
    assert!(
        define.is_some() && grava.is_some() && solta.is_some(),
        "o comando mudou de forma: {corpo}"
    );
    assert!(
        define < grava && grava < solta,
        "o sim de quem hospeda não é gravado entre a escrita do conjunto e a soltura do banco"
    );
}
