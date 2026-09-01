//! As URLs do módulo no CI têm de ser as que o código publica.
//!
//! O workflow busca o módulo do Cisco por URL escrita à mão, porque o `xtask`
//! não conhece este crate e criar a dependência para uma linha mexeria na regra
//! que o próprio `check-deps` guarda. Cópia é o que apodrece: no dia em que a
//! versão do OpenH264 subir aqui, o CI continuaria baixando a velha — e a
//! diferença apareceria como um teste que passa por pular, que é exatamente o
//! defeito que esse download existe para fechar.

#![allow(clippy::expect_used)]

use std::path::Path;

use seele_video::modulo::{MACOS_ARM64, WINDOWS_X64};

fn workflow() -> String {
    let caminho = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml");
    std::fs::read_to_string(&caminho)
        .unwrap_or_else(|erro| panic!("não li {}: {erro}", caminho.display()))
}

#[test]
fn o_ci_baixa_exatamente_os_modulos_que_este_crate_publica() {
    let ci = workflow();
    for (modulo, sistema) in [(MACOS_ARM64, "macOS"), (WINDOWS_X64, "Windows")] {
        let url = modulo.url();
        assert!(
            ci.contains(&url),
            "o CI não baixa o módulo do {sistema} que este crate publica.\n\
             Esperado em `.github/workflows/ci.yml`: {url}\n\
             Sem isso o CI busca uma versão que não é a nossa, os testes de \
             codec voltam a pular, e pular tem a mesma cor de passar."
        );
    }
}

#[test]
fn o_ci_exige_o_codec_onde_ele_e_publicado() {
    assert!(
        workflow().contains("SEELE_EXIGE_CODEC"),
        "o CI parou de exigir o codec.\n\
         Sem essa variável um módulo que não baixou deixa os testes voltarem \
         cedo, e um teste que volta cedo conta como passado — foi assim que o \
         `ida_e_volta` ficou verde por anos enquanto falhava."
    );
}
