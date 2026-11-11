//! **O laço de voz e o teste de microfone usam a supressão por rede neural.**
//!
//! A subtração espectral continua na árvore (`seele_audio::supressao`), com os
//! testes dela, e seria uma linha trocar de volta sem ninguém notar: a voz
//! seguiria funcionando, e a batida de teclado voltaria a passar inteira
//! (`docs/medidas/2026-11-11-teclado-na-supressao.md`). O que a rede faz com a
//! batida é provado em `seele-audio`; aqui se prova que é ela que está ligada.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

/// O texto procurado é montado em pedaços: escrito inteiro, este arquivo o
/// conteria e o guarda casaria com ele mesmo se um dia lesse a própria pasta.
fn construtor(modulo: &str, tipo: &str) -> String {
    format!("seele_audio::{modulo}::{tipo}")
}

#[test]
fn o_laco_de_voz_constroi_a_supressao_neural() {
    let voz = include_str!("../src/voice.rs");
    let neural = format!(
        "{}::nova(",
        construtor("supressao_neural", "SupressaoNeural")
    );
    let espectral = format!("{}::nova(", construtor("supressao", "Supressao"));
    assert!(
        voz.split_whitespace().collect::<String>().contains(&neural),
        "o laço de voz não constrói a supressão por rede neural"
    );
    assert!(
        !voz.contains(&espectral),
        "o laço de voz voltou a construir a subtração espectral"
    );
}

#[test]
fn o_teste_de_microfone_usa_a_mesma_supressao_da_voz() {
    let teste = include_str!("../src/teste_de_microfone.rs");
    let importa = format!(
        "use {} as Supressao;",
        construtor("supressao_neural", "SupressaoNeural")
    );
    assert!(
        teste.contains(&importa),
        "o teste de microfone não soa como a sala vai ouvir: ele usa outra supressão"
    );
}
