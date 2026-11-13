use super::{assentar_fala, Assentada, ModsNativos};
use crate::executor::{Fila, NivelDoConsole, ParaOFora};
use crate::rastro_de_teste::capturar;

/// Assenta uma linha de console como a bomba assentaria, e devolve o que
/// sobrou, o rastro, e a fila para conferir que nada foi reservado.
fn assentar(nivel: NivelDoConsole, texto: &str) -> (Assentada, String, std::sync::Arc<Fila>) {
    assentar_contando(nivel, texto, 0)
}

/// O mesmo, com quantas linhas o balde segurou antes desta.
fn assentar_contando(
    nivel: NivelDoConsole,
    texto: &str,
    suprimidas: u32,
) -> (Assentada, String, std::sync::Arc<Fila>) {
    let mods = std::sync::Mutex::new(ModsNativos::default());
    let fila = std::sync::Arc::new(Fila::default());
    let (assentada, rastro) = capturar(|| {
        assentar_fala(
            &mods,
            &|| true,
            &fila,
            1,
            ParaOFora::Console {
                nivel,
                texto: texto.to_owned(),
                suprimidas,
            },
            "seele/perfis",
        )
    });
    (assentada, rastro, fila)
}

/// **A parada de um MOD nativo sai no `seele.log` pelo filtro padrão**, com o
/// id e a instância.
///
/// Era DEBUG, e o filtro padrão não a escreve: numa saída do app, o registro
/// dizia que os MODs foram mandados parar e nunca que algum parou. A
/// instância, e não a geração: o número não se repete nesta janela, e a
/// linha «MOD reservado no executor nativo» o liga à geração.
#[test]
fn a_parada_do_mod_nativo_sai_em_info_com_o_id_e_a_instancia() {
    let mods = std::sync::Mutex::new(ModsNativos::default());
    let fila = std::sync::Arc::new(Fila::default());
    let (assentada, rastro) =
        capturar(|| assentar_fala(&mods, &|| true, &fila, 7, ParaOFora::Parou, "seele/perfis"));
    assert_eq!(assentada, Assentada::Fim { avisar: true });
    assert!(
        rastro.lines().any(|linha| linha.contains("INFO")
            && linha.contains("MOD nativo parou")
            && linha.contains("mod_id=seele/perfis")
            && linha.contains("instancia=7")),
        "a parada do MOD nativo não saiu em INFO com o id e a instância: o filtro padrão \
             não a escreve, e o seele.log de uma saída do app nunca diz que algum MOD parou: \
             {rastro}"
    );
}

/// Nos quatro níveis: um MOD em laço escreve com `log` tanto quanto com
/// `error`, e um braço que esquecesse a conta a perderia só no nível dele.
#[test]
fn a_linha_depois_do_aperto_diz_quantas_ficaram_para_tras() {
    for (nivel, marca) in [
        (NivelDoConsole::Depuracao, "DEBUG"),
        (NivelDoConsole::Informacao, "INFO"),
        (NivelDoConsole::Aviso, "WARN"),
        (NivelDoConsole::Erro, "ERROR"),
    ] {
        let (_, rastro, _) = assentar_contando(nivel, "de novo", 7);
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("console do MOD"))
            .unwrap_or_else(|| panic!("a linha {nivel:?} não chegou ao registro: {rastro}"));
        assert!(
            linha.contains(marca) && linha.contains("suprimidas=7"),
            "o registro não diz que houve mais do que ele mostra, e quem lê toma o pouco por \
                 tudo ({nivel:?}): {linha}"
        );
    }
    let (_, sozinha, _) = assentar(NivelDoConsole::Aviso, "sozinha");
    assert!(
        !sozinha.contains("suprimidas"),
        "uma linha sem aperto ganhou um `suprimidas=0` que só faz barulho: {sozinha}"
    );
}

#[test]
fn um_console_warn_sai_como_warn_com_o_id_do_mod() {
    let (assentada, rastro, fila) = assentar(NivelDoConsole::Aviso, "avatar recusado: 12 bytes");
    assert_eq!(
        assentada,
        Assentada::Nada,
        "uma linha de console virou fala para a janela, e ocuparia a cota dela"
    );
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("console do MOD"))
        .unwrap_or_else(|| {
            panic!(
                "o console do MOD não chegou ao registro — é o `console.warn` do caminho \
                     de erro sumindo em silêncio: {rastro}"
            )
        });
    for pedaco in [
        "WARN",
        "mod_id=seele/perfis",
        "texto=\"avatar recusado: 12 bytes\"",
    ] {
        assert!(
            linha.contains(pedaco),
            "a linha do console não traz `{pedaco}`, e é por ele que quem lê o registro \
                 acha o MOD e o que ele disse: {linha}"
        );
    }
    assert_eq!(
        (fila.ocupacao(), fila.avisos_de_pe()),
        ((0, 0), 0),
        "a linha de console tomou crédito da fila da janela"
    );
}

#[test]
fn cada_nivel_do_console_vira_o_mesmo_nivel_no_registro() {
    for (nivel, marca) in [
        (NivelDoConsole::Depuracao, "DEBUG"),
        (NivelDoConsole::Informacao, "INFO"),
        (NivelDoConsole::Aviso, "WARN"),
        (NivelDoConsole::Erro, "ERROR"),
    ] {
        let (_, rastro, _) = assentar(nivel, "linha");
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("console do MOD"))
            .unwrap_or_else(|| panic!("{nivel:?} não chegou ao registro: {rastro}"));
        assert!(
            linha.contains(marca),
            "`{nivel:?}` saiu com outro nível, e o filtro do seele.log o trataria como \
                 outra coisa: {linha}"
        );
    }
}

/// Nos quatro braços de `registrar_console_do_mod`, que são escritos à mão
/// um a um: só o do aviso tinha guarda, e o mais usado, `console.log`, cai
/// no da informação. Um braço que escrevesse o texto cru forjaria linha só
/// no nível dele, e um que esquecesse o id deixaria a linha sem dono — o
/// texto não o traz.
#[test]
fn uma_quebra_de_linha_do_mod_nao_forja_outra_linha_no_registro() {
    for nivel in [
        NivelDoConsole::Depuracao,
        NivelDoConsole::Informacao,
        NivelDoConsole::Aviso,
        NivelDoConsole::Erro,
    ] {
        let (_, rastro, _) = assentar(
            nivel,
            "primeira\n WARN seele_app: linha que o produto não escreveu",
        );
        assert_eq!(
            rastro.lines().count(),
            1,
            "um MOD escreveu uma segunda linha no seele.log ({nivel:?}), com a cara de uma \
                 linha do produto: {rastro}"
        );
        assert!(
            rastro.contains("mod_id=seele/perfis"),
            "a linha {nivel:?} do console saiu sem o id do MOD, e uma busca por `mod_id=` \
                 não a acha: {rastro}"
        );
    }
}
