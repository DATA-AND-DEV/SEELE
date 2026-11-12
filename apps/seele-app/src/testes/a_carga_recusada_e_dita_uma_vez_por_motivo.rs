use super::{carga_aceita, carga_dita, motivo_de, recusa_de_carga_dita, FalhaNoMod};
use crate::rastro_de_teste::capturar;

/// Uma recusa de carga pelo `motivo`, na `geracao`, dita com a memória
/// `ditas`.
fn recusar(
    ditas: &std::sync::Mutex<std::collections::HashMap<String, (u64, String)>>,
    geracao: u64,
    id: &str,
    motivo: &str,
) -> FalhaNoMod {
    recusa_de_carga_dita(
        ditas,
        geracao,
        id,
        FalhaNoMod::Recusado {
            motivo: motivo.to_owned(),
        },
    )
}

/// As linhas de carga recusada no rastro.
fn ditas_no(rastro: &str) -> Vec<&str> {
    rastro
        .lines()
        .filter(|linha| linha.contains("MOD não carregou"))
        .collect()
}

#[test]
fn uma_carga_recusada_duas_vezes_pelo_mesmo_motivo_deixa_uma_linha_so() {
    let ditas = std::sync::Mutex::default();
    let (falha, rastro) = capturar(|| {
        recusar(&ditas, 1, "prova/carga", "conteudo-de-outro-mod");
        recusar(&ditas, 1, "prova/carga", "conteudo-de-outro-mod")
    });
    assert_eq!(
        motivo_de(&falha),
        "conteudo-de-outro-mod",
        "a recusa que volta à janela foi mexida, e a gestão já não diz o motivo"
    );
    let linhas = ditas_no(&rastro);
    assert_eq!(
        linhas.len(),
        1,
        "a mesma recusa de carga, de novo a cada quatro segundos, saiu a cada tentativa — \
             novecentas linhas iguais por hora: {rastro}"
    );
    for pedaco in ["WARN", "mod_id=prova/carga", "motivo=conteudo-de-outro-mod"] {
        assert!(
            linhas.iter().all(|linha| linha.contains(pedaco)),
            "a linha da carga recusada não traz `{pedaco}`, e é por ela que quem lê o \
                 registro acha o MOD e o motivo: {rastro}"
        );
    }
}

/// **O que muda é dito**: outro motivo, outro MOD, e o mesmo motivo de
/// novo depois de uma carga que deu certo.
#[test]
fn um_motivo_novo_outro_mod_ou_a_volta_depois_de_carregar_sao_ditos() {
    let ditas = std::sync::Mutex::default();
    let ((), rastro) = capturar(|| {
        recusar(&ditas, 1, "prova/carga", "cliente-nao-servido");
        recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
        recusar(&ditas, 1, "prova/outro", "executor-nao-subiu");
        carga_aceita(&ditas, "prova/carga");
        recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
    });
    assert_eq!(
        ditas_no(&rastro).len(),
        4,
        "uma recusa que mudou — de motivo, de MOD, ou depois de uma carga que deu certo — \
             não foi dita: {rastro}"
    );
}

/// **Uma sessão nova diz de novo a recusa que a anterior já tinha dito.**
///
/// A memória segura a recusa repetida dentro de uma sessão. Atravessando a
/// saída, ela calava a mesma falha na sessão seguinte: a pessoa sai, entra
/// de novo, o MOD não carrega pelo mesmo motivo, e o `seele.log` desta
/// entrada não diz nada — a linha que existe é a de uma sessão que já
/// acabou.
#[test]
fn a_mesma_recusa_numa_sessao_nova_e_dita_de_novo() {
    let sessao = super::Session::default();
    let ((), rastro) = capturar(|| {
        recusar(
            &sessao.recusas_de_carga,
            1,
            "prova/carga",
            "conteudo-de-outro-mod",
        );
        sessao.revogar();
        recusar(
            &sessao.recusas_de_carga,
            2,
            "prova/carga",
            "conteudo-de-outro-mod",
        );
    });
    assert_eq!(
        ditas_no(&rastro).len(),
        2,
        "a recusa de carga dita numa sessão calou a mesma recusa na sessão seguinte, e o \
             seele.log da entrada nova não diz por que o MOD não carregou: {rastro}"
    );
}

/// **A recusa atrasada da sessão que acabou não cala a da seguinte.**
///
/// A corrida que a limpeza de [`super::Session::revogar`] não fecha: uma
/// carga passou da conferência de geração antes da saída e falhou depois
/// dela. A recusa dela é dita — e guardada — depois da limpeza, e a mesma
/// recusa na sessão seguinte achava o motivo igual e ficava calada, com a
/// linha da sessão anterior segundos antes no arquivo. Guardada com a
/// geração, a da sessão seguinte é outra recusa.
#[test]
fn a_recusa_atrasada_da_sessao_que_acabou_nao_cala_a_da_seguinte() {
    let sessao = super::Session::default();
    // A sessão 1 de pé.
    assert_eq!(
        sessao.revogar(),
        1,
        "a primeira geração de uma sessão não é 1"
    );
    let ((), rastro) = capturar(|| {
        assert_eq!(sessao.revogar(), 2, "a saída não fechou a geração 1");
        recusar(
            &sessao.recusas_de_carga,
            1,
            "prova/carga",
            "conteudo-de-outro-mod",
        );
        recusar(
            &sessao.recusas_de_carga,
            2,
            "prova/carga",
            "conteudo-de-outro-mod",
        );
    });
    let avisos = ditas_no(&rastro)
        .into_iter()
        .filter(|linha| linha.contains("WARN"))
        .count();
    assert_eq!(
        avisos, 2,
        "a recusa atrasada da sessão que acabou calou a mesma recusa na sessão seguinte, e \
             o seele.log da entrada nova não diz por que o MOD não carregou: {rastro}"
    );
}

/// **A sessão que acabou não é recusa a dizer**: é o desfecho certo de
/// uma carga atrasada, contada em `comandos_de_geracao_morta`.
#[test]
fn a_sessao_que_acabou_nao_vira_aviso_de_carga() {
    let ditas = std::sync::Mutex::default();
    let (_, rastro) = capturar(|| recusar(&ditas, 1, "prova/carga", "sessao-encerrada"));
    assert!(
        !rastro.contains("WARN") && rastro.contains("DEBUG"),
        "uma carga que não subiu porque a pessoa saiu virou aviso — é o desfecho certo, e \
             não um defeito: {rastro}"
    );
}

/// **`carga_dita`, no braço que aceita**: o valor volta intacto, e o motivo
/// guardado é esquecido — a mesma recusa depois dela é dita de novo.
#[test]
fn a_carga_dita_que_deu_certo_esquece_o_motivo_e_devolve_o_valor() {
    let ditas = std::sync::Mutex::default();
    let (valor, rastro) = capturar(|| {
        recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
        let valor = carga_dita(&ditas, 1, "prova/carga", Ok::<u64, FalhaNoMod>(7));
        recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
        valor
    });
    assert!(
        matches!(valor, Ok(7)),
        "a carga que deu certo não voltou intacta, e a janela perde o número da instância: \
             {valor:?}"
    );
    assert_eq!(
        ditas_no(&rastro).len(),
        2,
        "a carga que deu certo não esqueceu o motivo, e a mesma recusa depois dela ficou \
             calada — um MOD que subiu e caiu de novo pelo mesmo motivo some do registro: \
             {rastro}"
    );
}

/// **`carga_dita`, no braço que recusa**: passa pela recusa dita — uma
/// linha por motivo, e não uma por tentativa — e a recusa volta intacta.
#[test]
fn a_carga_dita_recusada_e_dita_uma_vez_e_volta_intacta() {
    let ditas = std::sync::Mutex::default();
    let (cargas, rastro) = capturar(|| {
        (0..3)
            .map(|_| {
                carga_dita(
                    &ditas,
                    1,
                    "prova/carga",
                    Err::<u64, _>(FalhaNoMod::Recusado {
                        motivo: "conteudo-de-outro-mod".to_owned(),
                    }),
                )
            })
            .collect::<Vec<_>>()
    });
    for carga in &cargas {
        let falha = carga
            .as_ref()
            .expect_err("uma carga recusada voltou como aceita");
        assert_eq!(
            motivo_de(falha),
            "conteudo-de-outro-mod",
            "a recusa que volta à janela foi mexida, e a gestão já não diz o motivo"
        );
    }
    assert_eq!(
        ditas_no(&rastro).len(),
        1,
        "a recusa de carga não passou pela recusa dita uma vez por motivo: três tentativas \
             deixaram outra conta de linhas: {rastro}"
    );
}

/// O corpo de `comando` em `main.rs` — do `{` da assinatura ao `}` que a
/// fecha —, sem as linhas de comentário, sem espaço nenhum e sem a vírgula
/// que o `rustfmt` põe antes de um `)` quando quebra a chamada em linhas: o
/// que sobra é o código, e a formatação não muda a conta.
fn corpo_sem_espaco(fonte: &str, comando: &str) -> String {
    let funcao = fonte
        .split(comando)
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_else(|| panic!("`{comando}` sumiu de main.rs"));
    let corpo = funcao
        .split_once("> {\n")
        .map(|(_, corpo)| corpo)
        .unwrap_or_else(|| panic!("o recorte não achou o corpo de `{comando}`: {funcao}"));
    corpo
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace(",)", ")")
}

/// **Os dois comandos de carga passam pela recusa dita**, cada um na forma
/// exata que lhe cabe, e só a reserva que deu certo esquece o motivo. Os
/// comandos precisam de um `AppHandle` e não sobem num teste; um que
/// devolvesse a recusa por fora seria o MOD sem carregar e o registro
/// calado de novo, com os testes acima verdes.
///
/// **A reserva é a chamada única de [`carga_dita`]**, e não um `match` que
/// a imite. Conferir só que `carga_aceita(` e `recusa_de_carga_dita(`
/// aparecem no corpo deixava passar o `carga_aceita` no braço errado — a
/// recusa esquecida a cada tentativa, e de novo as novecentas linhas por
/// hora — sem nada reprovar. Os dois braços são provados nos testes de
/// `carga_dita`, acima; aqui se prova que a reserva é ela.
///
/// **E o código é só a recusa dita**: `ler_codigo_do_mod(..)` com o
/// `map_err` de [`recusa_de_carga_dita`], e nada mais. Não é `carga_dita`:
/// a janela pede o código e depois a reserva, de novo a cada quatro
/// segundos enquanto o MOD não sobe, e o código lido esqueceria o motivo
/// antes de a reserva falhar — a mesma recusa dita a cada tentativa
/// (medido: dez linhas em dez tentativas, contra uma). Conferir só que
/// `recusa_de_carga_dita(` aparece no corpo deixava passar esse
/// esquecimento, num `match` com `carga_aceita` no `Ok` ou num `.inspect`
/// depois do `map_err`, sem nada reprovar.
#[test]
fn os_dois_comandos_de_carga_passam_pela_recusa_dita() {
    let fonte = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("main.rs legível")
    .replace("\r\n", "\n");
    let codigo = corpo_sem_espaco(&fonte, "fn codigo_do_mod(");
    assert_eq!(
        codigo,
        "ler_codigo_do_mod(&app,&session,geracao,&id,&hash).map_err(|falha|\
             recusa_de_carga_dita(&session.recusas_de_carga,geracao,&id,falha))",
        "`fn codigo_do_mod(` deixou de ser `ler_codigo_do_mod(..)` com o `map_err` de \
             `recusa_de_carga_dita`, e nada mais: sem a recusa dita, a carga recusada volta à \
             janela e não chega ao registro; com um `carga_aceita` (ou `carga_dita`), o código \
             lido esquece o motivo antes de a reserva falhar, e a mesma recusa volta a sair a \
             cada quatro segundos"
    );
    let reserva = corpo_sem_espaco(&fonte, "fn mod_nativo_reservar(");
    assert_eq!(
        reserva,
        "carga_dita(&session.recusas_de_carga,geracao,&id,reservar_no_executor(&app,&session,\
             geracao,&id,hash))",
        "`fn mod_nativo_reservar(` deixou de ser a chamada única de `carga_dita` sobre \
             `reservar_no_executor`: um `match` escrito à mão pode pôr o `carga_aceita` no braço \
             errado, e a mesma recusa volta a sair a cada quatro segundos, ou a seguinte fica \
             calada"
    );
}
