use seele_server::mods::anuncio::conjunto_exigido;
use seele_server::persistence::mods::{definir_conjunto, enable, EnabledMod};
use seele_server::persistence::{Location, Persistence};

use super::{
    corpo_de, fonte_sem_comentarios, gravar_o_sim_de_quem_hospeda, gravar_o_sim_se_for_o_guardado,
    onde_estou_em, Session,
};

/// A chave do guardado, a da sala e a de outro servidor S.
const GUARDADO: &str = "aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11";
const SALA: &str = "bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22";
const DE_S: &str = "cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33";

fn versao(numero: &str, byte: char) -> EnabledMod {
    EnabledMod {
        id: "autor/rpg".to_owned(),
        version: numero.to_owned(),
        hash: byte.to_string().repeat(64),
        repo: "https://example.invalid/rpg".to_owned(),
        reach: vec!["dom".to_owned()],
        server_half: false,
        api: seele_ffi::mods::MOD_API_VERSION,
        tema: false,
        plataformas: None,
    }
}

/// Uma pasta de configuração só deste teste.
fn pasta(nome: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("seele-lote-x-{nome}-"))
        .tempdir()
        .unwrap()
}

/// A identidade que cada gravador grava, calculada como ele a calcula.
fn identidades_dos_gravadores() -> [(&'static str, String); 2] {
    let salvar = Persistence::open(&Location::Memory).unwrap();
    definir_conjunto(&salvar, &[versao("1.0.0", 'a')]).unwrap();
    let atualizar = Persistence::open(&Location::Memory).unwrap();
    enable(&atualizar, &versao("1.0.0", 'a')).unwrap();
    enable(&atualizar, &versao("2.0.0", 'b')).unwrap();
    [
        (
            "o SALVAR da gestão (`aplicar_conjunto_de_mods`)",
            conjunto_exigido(&salvar).unwrap().identidade,
        ),
        (
            "a atualização pela gestão (`aplicar_mod`, QA-01)",
            conjunto_exigido(&atualizar).unwrap().identidade,
        ),
    ]
}

/// Um caso do portão: por onde a sessão entrou, a chave que ela conferiu, e
/// as das hospedagens no ar.
struct Caso {
    nome: &'static str,
    endereco: &'static str,
    impressao: &'static str,
    guardado: Option<&'static str>,
    grava: bool,
}

/// O que o portão grava, para cada caso e cada gravador: a lista do que
/// saiu diferente do esperado.
fn o_portao_nos(casos: &[Caso]) -> Vec<String> {
    let mut errados = Vec::new();
    for (quem, daqui) in identidades_dos_gravadores() {
        for caso in casos {
            let config = pasta("portao");
            let config = config.path().to_str().unwrap();
            let antes = "5".repeat(64);
            seele_ffi::mods::aceitar(config, caso.endereco, &antes).unwrap();
            let onde = onde_estou_em(
                Some(caso.endereco),
                Some(caso.impressao),
                caso.guardado,
                Some(SALA),
            );
            gravar_o_sim_se_for_o_guardado(config, Some(caso.endereco), onde, &daqui).unwrap();
            let esperado = if caso.grava { &daqui } else { &antes };
            if seele_ffi::mods::aceite_de(config, caso.endereco).as_deref()
                != Some(esperado.as_str())
            {
                errados.push(format!("{quem}, {}", caso.nome));
            }
        }
    }
    errados
}

/// A medida: a sessão em S, a janela hospedando, e o sim de S fica como
/// estava. Pela chave, também com S no mesmo endereço do guardado.
#[test]
fn com_a_sessao_em_outro_servidor_o_sim_dele_fica_como_estava() {
    let errados = o_portao_nos(&[
        Caso {
            nome: "a sessão em S, noutro endereço",
            endereco: "198.51.100.7:8383",
            impressao: DE_S,
            guardado: Some(GUARDADO),
            grava: false,
        },
        Caso {
            nome: "a sessão em S, no mesmo endereço do guardado",
            endereco: "127.0.0.1:8383",
            impressao: DE_S,
            guardado: Some(GUARDADO),
            grava: false,
        },
    ]);
    assert!(
        errados.is_empty(),
        "com a sessão em outro servidor, o sim dele virou a identidade do servidor daqui: \
             {errados:?}. Ele pergunta de novo, ou, com os conjuntos iguais, não pergunta, e a \
             lista de aceites mostra um sim que a pessoa não deu (ADR 0045)"
    );
}

/// O A04 e o próprio link: com a chave do guardado, o sim de quem hospeda é
/// gravado, entre a sessão pelo `aqui` ou pelo endereço da rede local que o
/// convite anuncia.
#[test]
fn na_sessao_do_guardado_o_sim_e_gravado_entre_por_onde_entrar() {
    let errados = o_portao_nos(&[
        Caso {
            nome: "o `aqui`",
            endereco: "127.0.0.1:8383",
            impressao: GUARDADO,
            guardado: Some(GUARDADO),
            grava: true,
        },
        Caso {
            nome: "o próprio link, pela rede local",
            endereco: "192.168.1.20:8383",
            impressao: GUARDADO,
            guardado: Some(GUARDADO),
            grava: true,
        },
    ]);
    assert!(
        errados.is_empty(),
        "na sessão do servidor guardado, o sim de quem hospeda não foi gravado: \
             {errados:?}. Quem acabou de decidir o conjunto é perguntado de novo na volta (A04)"
    );
}

/// A sala pessoal não tem MODs, e sem guardado de pé não há servidor daqui:
/// nos dois, nada é gravado.
#[test]
fn na_sala_e_sem_guardado_nada_e_gravado() {
    let errados = o_portao_nos(&[
        Caso {
            nome: "a sala pessoal",
            endereco: "127.0.0.1:8385",
            impressao: SALA,
            guardado: Some(GUARDADO),
            grava: false,
        },
        Caso {
            nome: "sem guardado de pé, no `aqui`",
            endereco: "127.0.0.1:8383",
            impressao: GUARDADO,
            guardado: None,
            grava: false,
        },
    ]);
    assert!(
        errados.is_empty(),
        "o sim de quem hospeda foi gravado fora do servidor guardado: {errados:?} (ADR 0045)"
    );

    // E pela sessão de verdade: sem conexão nem hospedagem, nada.
    let sessao = Session::default();
    *sessao.endereco_da_sessao.lock().unwrap() = Some("127.0.0.1:8383".to_owned());
    let config = pasta("sem-guardado");
    let config = config.path().to_str().unwrap();
    gravar_o_sim_de_quem_hospeda(config, &sessao, &"6".repeat(64)).unwrap();
    assert_eq!(
        seele_ffi::mods::aceite_de(config, "127.0.0.1:8383"),
        None,
        "sem conexão nem hospedagem, o sim de quem hospeda foi gravado no endereço da sessão"
    );
}

/// Os dois gravadores gravam o sim só por [`gravar_o_sim_de_quem_hospeda`],
/// e ela só pela resposta de `onde_estou`.
///
/// As agulhas são montadas por `concat!`, para que este guarda não case com
/// o próprio texto, e a fonte é lida sem as linhas de comentário.
#[test]
fn os_dois_gravadores_passam_pelo_sim_de_quem_hospeda() {
    let fonte = fonte_sem_comentarios();
    let fora: Vec<&str> = [
        ("aplicar_mod", concat!("async fn aplicar_mod", "(")),
        (
            "aplicar_conjunto_de_mods",
            concat!("async fn aplicar_conjunto_de_mods", "("),
        ),
    ]
    .into_iter()
    .filter(|(_, cabecalho)| {
        let corpo = corpo_de(&fonte, cabecalho);
        !(corpo.contains(concat!("gravar_o_sim_de_quem_hospeda", "("))
            && !corpo.contains(concat!("seele_ffi::mods::aceitar", "("))
            && !corpo.contains(concat!("endereco_da", "_sessao")))
    })
    .map(|(quem, _)| quem)
    .collect();
    assert!(
        fora.is_empty(),
        "{fora:?} grava o sim sem passar por `gravar_o_sim_de_quem_hospeda`: com a sessão \
             em outro servidor, o sim dele vira a identidade do servidor daqui (ADR 0045)"
    );

    let grava = corpo_de(&fonte, concat!("fn gravar_o_sim_de_quem_hospeda", "("));
    let se_for = corpo_de(&fonte, concat!("fn gravar_o_sim_se_for_o_guardado", "("));
    let comando = corpo_de(&fonte, concat!("fn onde_estou", "(session"));
    assert!(
        grava.contains(concat!("onde_estou_na", "(session)"))
            && grava.contains(concat!("gravar_o_sim_se_for_o", "_guardado("))
            && se_for.contains(concat!("onde == OndeEstou::", "Guardado"))
            && comando.contains(concat!("onde_estou_na", "(&session)")),
        "o sim de quem hospeda deixou de passar pela resposta de `onde_estou`, ou o comando \
             deixou de dar a mesma: são duas respostas a «estou no meu servidor?», e o sim vai \
             a um servidor que não é o guardado (ADR 0045):\n{grava}\n{se_for}\n{comando}"
    );
}
