//! Quem publica um release tem de exigir o codec.
//!
//! Metade dos testes de tela pula quando o módulo do Cisco não está por perto, e
//! pular tem a mesma cor de passar num relatório verde — foi assim que o
//! `ida_e_volta` ficou anos verde enquanto **falhava**, e assim que o som da
//! tela do Windows atravessou seis versões quebrado.
//!
//! Quem garantia isso era o `.github/workflows/ci.yml`, que baixava o módulo e
//! ligava `SEELE_EXIGE_CODEC`. Ele saiu: em repositório privado os minutos do
//! Actions não fecham — macOS conta 10× —, e `empacotar/publicar.sh` já era «o
//! `release.yml` inteiro numa máquina só». A obrigação mudou de casa com ele, e
//! este guarda mudou junto.

#![allow(clippy::expect_used)]

use std::path::Path;

fn publicar() -> String {
    let caminho = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../empacotar/publicar.sh");
    std::fs::read_to_string(&caminho)
        .unwrap_or_else(|erro| panic!("não li {}: {erro}", caminho.display()))
        .replace("\r\n", "\n")
}

#[test]
fn quem_publica_exige_o_codec() {
    let script = publicar();
    assert!(
        script.contains("SEELE_EXIGE_CODEC"),
        "`empacotar/publicar.sh` deixou de exigir o codec.\n\
         Sem essa variável, metade dos testes de tela volta a pular quando o \
         módulo não está por perto — e pular tem a mesma cor de passar. Um \
         release sairia com a tela nunca tendo sido exercitada."
    );
    assert!(
        script.contains("SEELE_OPENH264"),
        "`empacotar/publicar.sh` deixou de apontar o módulo de vídeo.\n\
         Exigir o codec sem dizer onde ele está é reprovar todo release nesta \
         máquina."
    );
}

#[test]
fn quem_publica_roda_a_bateria_nos_dois_sistemas() {
    // Numa única sessão o Windows encontrou três defeitos que o macOS não
    // mostra: dois de fim de linha, em guardas que comparavam texto com `\n`
    // num repositório que faz checkout com CRLF, e um binário de teste que nem
    // carregava. Rodar só na máquina de quem publica troca cobertura por
    // conveniência de quem aperta o botão.
    let script = publicar();
    // No começo da linha, porque `etapa_da_bateria() {` também termina em
    // «bateria() {» — e uma âncora que casa com a função auxiliar apontaria para
    // o corpo errado, onde nada do que este teste procura mora. Foi o que
    // aconteceu quando ela nasceu.
    let bateria = script
        .split("\nbateria() {")
        .nth(1)
        .expect("a função da bateria sumiu do publicar.sh");
    assert!(
        bateria.contains("cargo test --workspace"),
        "a bateria deixou de rodar os testes nesta máquina"
    );
    assert!(
        bateria.contains("no_windows"),
        "a bateria deixou de rodar os testes no Windows.\n\
         É o único sistema onde três defeitos desta casa apareceram, e nenhum \
         deles se vê daqui."
    );
}
