use super::{entrar_no_tunel_do_mod, EstadoNativo, FalhaNoMod, InstanciaNativa, Session};
use crate::executor::{ExecutorQuickJs, Limites};

const SEM_SESSAO: crate::sessoes::CarimboDaJanela = crate::sessoes::CarimboDaJanela {
    sessao: 0,
    geracao: 0,
};

fn motivo(falha: &FalhaNoMod) -> &str {
    match falha {
        FalhaNoMod::Recusado { motivo } => motivo,
        outra => panic!("esperava `Recusado`, veio {outra:?}"),
    }
}

/// Uma sessão viva (geração 1) com um MOD de pé que pode chamar `capacidades`.
fn sessao_com(id: &str, capacidades: Vec<&'static str>) -> (Session, u64) {
    let sessao = Session::default();
    let geracao = sessao.revogar();
    let instancia = InstanciaNativa {
        id: id.to_owned(),
        geracao,
        hash: "a".repeat(64),
        capacidades,
        estado: EstadoNativo::Ativa,
        executor: ExecutorQuickJs::novo(Limites::default()).expect("motor"),
        pendentes: std::collections::VecDeque::new(),
        bytes_pendentes: 0,
        avisos: std::collections::VecDeque::new(),
        avisos_perdidos: 0,
        parou: false,
        codigo: None,
        avisado: false,
    };
    sessao
        .mods_nativos
        .lock()
        .expect("tabela")
        .guardar(instancia);
    (sessao, geracao)
}

fn rodar<T>(futuro: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(futuro)
}

#[test]
fn a_sessao_que_acabou_e_recusada_antes_de_olhar_o_mod() {
    let sessao = Session::default();
    // Geração 0 nunca vale; e uma morta (1, depois revogada) também não.
    let viva = sessao.revogar();
    sessao.revogar();
    for geracao in [0, viva] {
        let falha = rodar(entrar_no_tunel_do_mod(
            &sessao,
            SEM_SESSAO,
            geracao,
            "x/jogo",
            "n".into(),
        ))
        .expect_err("uma geração morta não entra");
        assert_eq!(motivo(&falha), "sessao-encerrada", "geração {geracao}");
    }
}

#[test]
fn um_mod_sem_o_poder_porta_e_recusado_com_o_nome() {
    // De pé, na geração viva, mas com as capacidades da API e sem `tunel`.
    let (sessao, geracao) = sessao_com(
        "x/sem-porta",
        seele_ffi::mods::capacidades_da_api(seele_ffi::mods::MOD_API_VERSION).to_vec(),
    );
    let falha = rodar(entrar_no_tunel_do_mod(
        &sessao,
        SEM_SESSAO,
        geracao,
        "x/sem-porta",
        "n".into(),
    ))
    .expect_err("sem o poder porta não entra");
    assert_eq!(motivo(&falha), "mod-sem-porta");
    // Um MOD que não está de pé tampouco.
    let falha = rodar(entrar_no_tunel_do_mod(
        &sessao,
        SEM_SESSAO,
        geracao,
        "x/ninguem",
        "n".into(),
    ))
    .expect_err("quem não está de pé não entra");
    assert_eq!(motivo(&falha), "mod-sem-porta");
}

#[test]
fn o_poder_porta_passa_o_portao_e_so_para_na_falta_de_sessao() {
    // `tunel` nas capacidades: passa pelo portão do Rust, e a recusa que
    // sobra é a da conexão — que esta sessão de teste não tem. A diferença
    // para `mod-sem-porta` é o que prova que o portão deixou passar.
    let (sessao, geracao) = sessao_com("x/jogo", vec!["tunel"]);
    let falha = rodar(entrar_no_tunel_do_mod(
        &sessao,
        SEM_SESSAO,
        geracao,
        "x/jogo",
        "n".into(),
    ))
    .expect_err("sem conexão não há porta");
    assert_eq!(motivo(&falha), "sessao-encerrada");
}

#[test]
fn o_poder_de_outra_geracao_ou_de_outro_mod_nao_vale() {
    let (sessao, geracao) = sessao_com("x/jogo", vec!["tunel"]);
    let tabela = sessao.mods_nativos.lock().expect("tabela");
    assert!(tabela.pode(geracao, "x/jogo", "tunel"));
    assert!(
        !tabela.pode(geracao + 1, "x/jogo", "tunel"),
        "outra geração"
    );
    assert!(!tabela.pode(geracao, "x/outro", "tunel"), "outro MOD");
    assert!(
        !tabela.pode(geracao, "x/jogo", "retrato"),
        "outra capacidade"
    );
}
