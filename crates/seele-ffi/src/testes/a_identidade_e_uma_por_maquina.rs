use std::path::{Path, PathBuf};

use super::{codigo_com, impressao_com, pasta_da_identidade, semente_com};

fn pasta(nome: &str) -> PathBuf {
    let caminho = std::env::temp_dir().join(format!(
        "seele-ffi-identidade-{nome}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&caminho).unwrap();
    caminho
}

/// O contrato do ADR 0061: a pasta da variável, quando ela não está vazia;
/// senão, a de hoje.
#[test]
fn a_pasta_da_identidade_e_a_da_variavel_quando_ela_nao_esta_vazia() {
    let home = Path::new("/um/home");
    assert_eq!(
        pasta_da_identidade(home, Some("/a/maquina")),
        PathBuf::from("/a/maquina")
    );
    assert_eq!(
        pasta_da_identidade(home, Some("")),
        home,
        "vazia vale como ausente"
    );
    assert_eq!(pasta_da_identidade(home, None), home);
}

/// **A pessoa é a mesma nas duas linhas.** Com a variável, a impressão sai
/// da chave da pasta da máquina, e nenhuma chave nova nasce na pasta de
/// dados da linha — que é o que faria a pessoa ser outra nos servidores
/// 0.15.
#[test]
fn com_a_variavel_a_impressao_e_a_da_chave_da_maquina() {
    let maquina = pasta("maquina");
    let dados_da_linha = pasta("dados-da-linha");
    std::fs::write(maquina.join("identity.key"), [5_u8; 32]).unwrap();
    let esperada = seele_core::key_fingerprint(
        super::identity::load_or_create(&maquina.join("identity.key"))
            .unwrap()
            .verifying_key()
            .as_bytes(),
    );

    let lida = impressao_com(
        dados_da_linha.to_str().unwrap(),
        Some(maquina.to_str().unwrap()),
    )
    .unwrap();

    assert_eq!(lida, esperada);
    assert!(
        !dados_da_linha.join("identity.key").exists(),
        "uma chave nova nasceu na pasta de dados: a pessoa seria outra nesta linha"
    );
    let _ = std::fs::remove_dir_all(&maquina);
    let _ = std::fs::remove_dir_all(&dados_da_linha);
}

/// Sem a variável, tudo como antes: a chave mora em `home`.
#[test]
fn sem_a_variavel_a_chave_e_a_de_home() {
    let home = pasta("home");
    std::fs::write(home.join("identity.key"), [6_u8; 32]).unwrap();
    let esperada = seele_core::key_fingerprint(
        super::identity::load_or_create(&home.join("identity.key"))
            .unwrap()
            .verifying_key()
            .as_bytes(),
    );
    assert_eq!(
        impressao_com(home.to_str().unwrap(), None).unwrap(),
        esperada
    );
    assert_eq!(
        impressao_com(home.to_str().unwrap(), Some("")).unwrap(),
        esperada
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// **SEU CÓDIGO é o da chave da máquina** (tarefa 22 da F7, o terceiro
/// teste): com `SEELE_HOME=a` e `SEELE_IDENTIDADE=b`, o código é o da
/// chave de `b`. Um código lido de `a` seria um número que não toca aqui,
/// porque a sala sobe com a chave de `b`.
#[test]
fn com_a_variavel_o_codigo_e_o_da_chave_da_maquina() {
    let maquina = pasta("maquina-do-codigo");
    let dados_da_linha = pasta("dados-do-codigo");
    std::fs::write(maquina.join("identity.key"), [5_u8; 32]).unwrap();
    let chave = super::identity::load_or_create(&maquina.join("identity.key")).unwrap();
    let esperado = seele_core::Codigo::da_chave(chave.verifying_key().as_bytes());

    let lido = codigo_com(
        dados_da_linha.to_str().unwrap(),
        Some(maquina.to_str().unwrap()),
    )
    .unwrap();

    assert_eq!(lido, esperado);
    assert!(
        !dados_da_linha.join("identity.key").exists(),
        "uma chave nova nasceu na pasta de dados: SEU CÓDIGO seria o de outra pessoa"
    );
    let _ = std::fs::remove_dir_all(&maquina);
    let _ = std::fs::remove_dir_all(&dados_da_linha);
}

/// **A sala sobe com a chave cujo código a pessoa mostra.** A semente é a
/// da pasta da identidade, e a chave que ela dá tem o código desta máquina:
/// uma sala de outra semente atenderia por outro número.
#[test]
fn a_semente_da_sala_e_a_da_chave_da_maquina_e_da_o_mesmo_codigo() {
    let maquina = pasta("maquina-da-semente");
    let dados_da_linha = pasta("dados-da-semente");
    std::fs::write(maquina.join("identity.key"), [9_u8; 32]).unwrap();
    let (home, variavel) = (
        dados_da_linha.to_str().unwrap(),
        Some(maquina.to_str().unwrap()),
    );

    let semente = semente_com(home, variavel).unwrap();

    assert_eq!(
        semente.bytes(),
        super::identity::load_or_create(&maquina.join("identity.key"))
            .unwrap()
            .as_bytes(),
        "a semente da sala não é a chave da pasta da identidade"
    );
    let da_sala = seele_core::SigningKey::from_bytes(semente.bytes());
    assert_eq!(
        seele_core::Codigo::da_chave(da_sala.verifying_key().as_bytes()),
        codigo_com(home, variavel).unwrap(),
        "a sala subiria com uma chave cujo código não é o que SEU CÓDIGO mostra"
    );
    let _ = std::fs::remove_dir_all(&maquina);
    let _ = std::fs::remove_dir_all(&dados_da_linha);
}

/// **A semente não sai pelo `Debug`.** Ela é a chave privada da pessoa, e
/// um `tracing::info!(?semente)` a levaria ao `seele.log`.
#[test]
fn a_semente_nao_sai_pelo_debug() {
    let maquina = pasta("maquina-do-debug");
    std::fs::write(maquina.join("identity.key"), [0xA7_u8; 32]).unwrap();
    let semente = semente_com(maquina.to_str().unwrap(), None).unwrap();

    for impresso in [format!("{semente:?}"), format!("{semente:#?}")] {
        for vazamento in ["167", "a7", "A7"] {
            assert!(
                !impresso.contains(vazamento),
                "o `Debug` da semente imprime os bytes da chave:\n{impresso}"
            );
        }
        assert!(
            impresso.contains("oculta"),
            "o `Debug` da semente não diz que ela está ali:\n{impresso}"
        );
    }
    let _ = std::fs::remove_dir_all(&maquina);
}

/// O código de produção de `lib.rs`: o arquivo sem os módulos
/// `#[cfg(test)]`.
///
/// Aqui, e não na `linha/0.15`, os módulos de teste ficam **espalhados**
/// entre funções de produção (`onde_mora_hoje`, `impressao_a_conferir`,
/// `pares_que_esta_versao_atende` vêm depois deles). Cortar no primeiro
/// `#[cfg(test)]`, como o guarda da 0.15 faz, deixaria de fora metade do
/// arquivo. Cada módulo de teste é de primeiro nível e fecha numa linha
/// `}` sozinha, que é onde a produção recomeça.
///
/// **Um módulo de teste sem corpo não leva a produção junto.** Os que moram
/// em `testes/` deixam no `lib.rs` o `#[cfg(test)]`, os outros atributos e
/// `mod x;`: não há `}` dele para achar, e o corte ia até a `}` da próxima
/// função de produção, que saía da leitura sem ninguém ver. Só as linhas da
/// declaração saem.
fn so_producao(fonte: &str) -> String {
    let marca = format!("\n#[cfg({})]\n", "test");
    let mut pedacos = fonte.split(marca.as_str());
    let mut producao = pedacos.next().unwrap().to_owned();
    for pedaco in pedacos {
        // A primeira linha que não é de atributo é a do item.
        let mut depois_do_item = pedaco;
        let item = loop {
            let (linha, resto) = depois_do_item
                .split_once('\n')
                .unwrap_or((depois_do_item, ""));
            depois_do_item = resto;
            let de_atributo =
                linha.starts_with("#[") || linha.starts_with(' ') || linha.starts_with(')');
            if !de_atributo || resto.is_empty() {
                break linha;
            }
        };
        if item.starts_with("mod ") && item.trim_end().ends_with(';') {
            producao.push('\n');
            producao.push_str(depois_do_item);
            continue;
        }
        let fim = pedaco
            .find("\n}\n")
            .expect("um módulo de teste sem a linha `}` que o fecha");
        producao.push_str(&pedaco[fim + 3..]);
    }
    producao
}

/// **Os dois leitores de produção passam pela mesma resolução.** O
/// `impressao_desta_maquina` e o `connect` liam `identity.key` sob o `home`
/// cada um; um que continuasse lendo direto deixaria a pessoa com duas
/// chaves. As agulhas são montadas em tempo de execução para este teste não
/// ser contado.
#[test]
fn so_um_lugar_monta_o_caminho_da_chave() {
    let fonte = so_producao(include_str!("../lib.rs"));
    // O guarda do guarda: a produção que vem **depois** dos módulos de
    // teste está no texto lido, e nenhum teste está.
    for depois_dos_testes in [
        "pub async fn onde_mora_hoje",
        "pub const fn pares_que_esta_versao_atende",
    ] {
        assert!(
            fonte.contains(depois_dos_testes),
            "a leitura da produção perdeu `{depois_dos_testes}`, que vem depois dos testes"
        );
    }
    assert!(
        !fonte.contains("fn a_pasta_da_identidade_e_a_da_variavel_quando_ela_nao_esta_vazia"),
        "a leitura da produção levou um teste junto"
    );

    let monta = format!("{}(\"{}\")", ".join", "identity.key");
    assert_eq!(
        fonte.matches(&monta).count(),
        1,
        "a chave é procurada fora de `arquivo_da_identidade`"
    );
    // E cada um dos dois leitores usa o montador: a impressão desta
    // máquina, e o `connect`.
    for uso in [
        format!("{}(Path::new(home), variavel)", "arquivo_da_identidade_com"),
        format!("load_or_create(&{}(&home))", "arquivo_da_identidade"),
    ] {
        assert!(
            fonte.contains(&uso),
            "um dos dois leitores não passa pelo montador: falta `{uso}`"
        );
    }
}
