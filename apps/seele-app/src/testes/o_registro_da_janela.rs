use crate::rastro_de_teste::capturar;
use crate::relogio_de_teste::origem;

#[test]
fn o_aviso_da_janela_sai_como_warn_com_o_id_do_mod_em_campo_proprio() {
    let ((), rastro) = capturar(|| {
        super::registrar_da_janela_em(
            &super::BaldesDoRegistro::default(),
            origem(),
            "aviso".to_owned(),
            "atender-mod".to_owned(),
            "seele/perfis: «contribuir» falhou — a API de MODs não conhece o ponto «x»".to_owned(),
            Some("seele/perfis".to_owned()),
        );
    });
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("«contribuir» falhou"))
        .unwrap_or_else(|| panic!("a recusa da janela não chegou ao registro: {rastro}"));
    for pedaco in ["WARN", "mod_id=seele/perfis", "onde=atender-mod"] {
        assert!(
            linha.contains(pedaco),
            "a linha da janela não traz `{pedaco}`, e `mod_id=` é o que junta numa busca \
                 o que a janela e o Rust disseram do mesmo MOD: {linha}"
        );
    }
    assert!(
        linha.contains(
            "frase=\"seele/perfis: «contribuir» falhou — a API de MODs não conhece o ponto \
                 «x»\""
        ),
        "a frase da janela não saiu inteira no campo `frase=\"…\"`: solta na mensagem, um \
             `mod_id=` escrito por um MOD dentro dela vem antes do verdadeiro na linha: {linha}"
    );
}

/// **O id do MOD vem antes da frase, e um `mod_id=` escrito nela não se
/// passa pelo verdadeiro.**
///
/// A frase carrega texto que um MOD escolheu. Escrita como a mensagem da
/// linha, ela vinha antes dos campos, e um MOD que pusesse
/// `mod_id=vitima/mod` nela tinha o primeiro `mod_id=` da linha: quem lê da
/// esquerda para a direita atribui a frase ao MOD errado. Num campo em `?`,
/// ela sai depois do id, entre aspas e escapada.
#[test]
fn um_mod_id_forjado_na_frase_nao_vem_antes_do_verdadeiro() {
    let ((), rastro) = capturar(|| {
        super::registrar_da_janela_em(
            &super::BaldesDoRegistro::default(),
            origem(),
            "aviso".to_owned(),
            "atender-mod".to_owned(),
            "x onde=forjado mod_id=vitima/mod".to_owned(),
            Some("a/b".to_owned()),
        );
    });
    let primeiro = |nome: &str| -> String {
        rastro
            .split(&format!("{nome}="))
            .nth(1)
            .and_then(|resto| resto.split_whitespace().next())
            .unwrap_or_else(|| panic!("a linha da janela não traz `{nome}=`: {rastro}"))
            .to_owned()
    };
    assert_eq!(
        primeiro("mod_id"),
        "a/b",
        "o primeiro `mod_id=` da linha da janela é o que um MOD escreveu na frase, e não o \
             do campo: a linha se lê como sendo de outro MOD: {rastro}"
    );
    assert_eq!(
        primeiro("onde"),
        "atender-mod",
        "o primeiro `onde=` da linha da janela é o que um MOD escreveu na frase, e não o do \
             campo: {rastro}"
    );
    assert!(
        rastro.contains(r#"frase="x onde=forjado mod_id=vitima/mod""#),
        "a frase forjada não saiu entre aspas no campo `frase`: {rastro}"
    );
}

/// A linha da janela no `nivel` dado, com o id `a/b`, e o rastro dela.
fn da_janela_no_nivel(nivel: &str) -> String {
    let ((), rastro) = capturar(|| {
        super::registrar_da_janela_em(
            &super::BaldesDoRegistro::default(),
            origem(),
            nivel.to_owned(),
            "atender-mod".to_owned(),
            "uma frase da janela".to_owned(),
            Some("a/b".to_owned()),
        );
    });
    rastro
}

/// Os pedaços da linha, separados por espaço: um campo é um pedaço inteiro,
/// e `mod_id=a/b` não casa com `mod_id=a/bc`.
fn tem_o_campo(linha: &str, campo: &str) -> bool {
    linha.split_whitespace().any(|pedaco| pedaco == campo)
}

/// **Os três braços levam o id, e não só o do aviso.** O teste de cima
/// prova o WARN; o ERROR e o DEBUG escrevem por chamadas próprias, e um
/// braço que esquecesse o `mod_id` deixava a linha fora da busca por
/// `mod_id=` sem que nenhum teste reclamasse (T3 M2). Pelos campos, e não
/// pela forma inteira da linha.
#[test]
fn o_erro_da_janela_sai_como_error_com_o_id_do_mod() {
    let rastro = da_janela_no_nivel("erro");
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("uma frase da janela"))
        .unwrap_or_else(|| panic!("o erro da janela não chegou ao registro: {rastro}"));
    assert!(
        tem_o_campo(linha, "ERROR"),
        "o erro da janela não saiu em ERROR, e o filtro do seele.log o trata como outra \
             coisa: {linha}"
    );
    assert!(
        tem_o_campo(linha, "mod_id=a/b"),
        "o erro da janela saiu sem `mod_id=`, e uma busca pelo MOD não junta o que a janela \
             disse dele com o que o Rust disse: {linha}"
    );
}

/// O mesmo para um nível que a janela não conhece, que sai em DEBUG.
#[test]
fn um_nivel_desconhecido_da_janela_sai_como_debug_com_o_id_do_mod() {
    let rastro = da_janela_no_nivel("outro");
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("uma frase da janela"))
        .unwrap_or_else(|| {
            panic!(
                "a linha da janela num nível desconhecido não chegou ao registro, que captura \
                     do TRACE para cima: {rastro}"
            )
        });
    assert!(
        tem_o_campo(linha, "DEBUG"),
        "a linha da janela num nível desconhecido não saiu em DEBUG: {linha}"
    );
    assert!(
        tem_o_campo(linha, "mod_id=a/b"),
        "a linha DEBUG da janela saiu sem `mod_id=`, e com `RUST_LOG=debug` uma busca pelo \
             MOD não a acha: {linha}"
    );
}

#[test]
fn sem_id_a_linha_da_janela_continua_saindo() {
    let ((), rastro) = capturar(|| {
        super::registrar_da_janela_em(
            &super::BaldesDoRegistro::default(),
            origem(),
            "aviso".to_owned(),
            "geral".to_owned(),
            "sem MOD".to_owned(),
            None,
        );
    });
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("sem MOD"))
        .unwrap_or_else(|| panic!("uma linha sem MOD deixou de sair: {rastro}"));
    assert!(
        linha.contains("WARN") && !linha.contains("mod_id"),
        "uma linha sem MOD ganhou um `mod_id` que não é de ninguém: {linha}"
    );
}

/// **O id vazio é o catálogo, e não um MOD.** `carregarMods` pede o
/// catálogo do servidor por `pedirAoServidor("", …)`, e as recusas de
/// `pedirAoServidor` passam o id adiante como está. Um `mod_id=` sem nome
/// é um campo que uma busca por `mod_id=` acha e que não é de MOD nenhum.
#[test]
fn o_id_vazio_do_catalogo_nao_vira_um_mod_id_sem_nome() {
    let ((), rastro) = capturar(|| {
        super::registrar_da_janela_em(
            &super::BaldesDoRegistro::default(),
            origem(),
            "aviso".to_owned(),
            "pedido-de-mod".to_owned(),
            ": oito pedidos em voo, este não cabe".to_owned(),
            Some(String::new()),
        );
    });
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("oito pedidos em voo"))
        .unwrap_or_else(|| panic!("a recusa do catálogo deixou de sair: {rastro}"));
    assert!(
        linha.contains("WARN") && !linha.contains("mod_id"),
        "o pedido do catálogo, que não é de MOD nenhum, saiu com um `mod_id` sem \
             nome — e uma busca por `mod_id=` o acha: {linha}"
    );
}

#[test]
fn uma_quebra_de_linha_da_janela_nao_forja_outra_linha_no_registro() {
    let ((), rastro) = capturar(|| {
        super::registrar_da_janela_em(
            &super::BaldesDoRegistro::default(),
            origem(),
            "aviso".to_owned(),
            "recusa-de-mod".to_owned(),
            "a/b: ponto «x\nWARN seele_app: forjada»".to_owned(),
            Some("a/b".to_owned()),
        );
    });
    assert_eq!(
        rastro.lines().count(),
        1,
        "texto de MOD atravessou a janela e escreveu uma segunda linha no seele.log: {rastro}"
    );
}

/// **Os dois campos também, e não só a frase.** `onde` é, hoje, um literal
/// do produto, e o id vem de um manifesto já conferido; mas os dois chegam
/// pela mesma ponte que a frase, e o registro não pode depender de quem
/// chama ser bem-comportado. Um caso por campo, para que tirar o filtro de
/// um deles reprove sozinho.
#[test]
fn uma_quebra_de_linha_em_onde_ou_no_id_nao_forja_outra_linha_no_registro() {
    for (campo, onde, mod_id) in [
        ("onde", "atender-mod\nWARN seele_app: forjada", "a/b"),
        ("mod_id", "atender-mod", "a/b\nWARN seele_app: forjada"),
    ] {
        let ((), rastro) = capturar(|| {
            super::registrar_da_janela_em(
                &super::BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                onde.to_owned(),
                "a/b: uma frase de verdade".to_owned(),
                Some(mod_id.to_owned()),
            );
        });
        assert_eq!(
            rastro.lines().count(),
            1,
            "uma quebra de linha em `{campo}` escreveu uma segunda linha no seele.log, \
                 com a cara de uma linha do produto: {rastro}"
        );
    }
}

/// **O que quebra ou inverte a linha num editor também não passa**, e não
/// só o caractere de controle.
///
/// As portas do Rust escrevem pelo `Debug` de um `str` e saem com
/// `\u{2028}` e `\u{202e}`; a da janela escrevia a frase sem escape, e
/// deixava passar crus os separadores de linha e de parágrafo — que um
/// editor quebra como um `\n` — e os controles bidirecionais: um U+202E na
/// frase de um MOD inverte, num editor que segue o bidi, o `onde=` e o
/// `mod_id=` que vêm depois dela. Um caso por campo, para que tirar o
/// filtro de um deles reprove sozinho.
#[test]
fn a_janela_nao_deixa_passar_o_que_quebra_ou_inverte_a_linha() {
    for c in [
        '\u{0085}', '\u{2028}', '\u{2029}', '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}',
        '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}',
        '\u{2069}',
    ] {
        let com = |texto: &str| format!("{texto}{c}forjado");
        for (campo, onde, o_que, mod_id) in [
            (
                "frase",
                "recusa-de-mod".to_owned(),
                com("a/b: x"),
                "a/b".to_owned(),
            ),
            (
                "id",
                "recusa-de-mod".to_owned(),
                "a/b: x".to_owned(),
                com("a/b"),
            ),
            (
                "onde",
                com("recusa-de-mod"),
                "a/b: x".to_owned(),
                "a/b".to_owned(),
            ),
        ] {
            let ((), rastro) = capturar(|| {
                super::registrar_da_janela_em(
                    &super::BaldesDoRegistro::default(),
                    origem(),
                    "aviso".to_owned(),
                    onde,
                    o_que,
                    Some(mod_id),
                );
            });
            assert!(
                rastro.contains("a/b: x") && !rastro.contains(c),
                "U+{:04X} no campo `{campo}` saiu cru no seele.log, e num editor ele quebra \
                     a linha ou inverte o `onde=` e o `mod_id=` que vêm depois: {rastro:?}",
                u32::from(c)
            );
        }
    }
}
