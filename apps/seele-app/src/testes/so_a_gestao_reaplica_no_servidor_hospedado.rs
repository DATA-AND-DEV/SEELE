use seele_server::mods::anuncio::conjunto_exigido;
use seele_server::persistence::mods::{enable, enabled, EnabledMod};
use seele_server::persistence::{Location, Persistence};

use super::{corpo_de, fonte_sem_comentarios, reaplica_no_hospedado, UsoDoCatalogo};

const ID: &str = "autor/rpg";

/// Uma versão do MOD, como o banco do servidor a guarda.
fn versao(numero: &str, byte: char) -> EnabledMod {
    EnabledMod {
        id: ID.to_owned(),
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

/// O servidor que esta janela hospeda, exigindo o MOD na 1.0.0.
fn hospedado() -> Persistence {
    let banco = Persistence::open(&Location::Memory).unwrap();
    enable(&banco, &versao("1.0.0", 'a')).unwrap();
    banco
}

/// A versão que o servidor hospedado exige, e a identidade que ele anuncia.
fn exigido(banco: &Persistence) -> (String, String) {
    let ligado = enabled(banco)
        .unwrap()
        .into_iter()
        .find(|ligado| ligado.id == ID)
        .unwrap();
    (ligado.version, conjunto_exigido(banco).unwrap().identidade)
}

/// O que `instalar_mod_do_catalogo` faz no servidor hospedado depois de
/// baixar `chegou`: a decisão do comando, sobre o que `mod_exigido_aqui`
/// lê, e, se ela mandar, o que `aplicar_mod` grava.
fn depois_de_baixar(banco: &Persistence, uso: UsoDoCatalogo, chegou: &EnabledMod) -> bool {
    let ja_exigido = enabled(banco).unwrap().iter().any(|ligado| ligado.id == ID);
    let reaplicar = reaplica_no_hospedado(uso, ja_exigido);
    if reaplicar {
        enable(banco, chegou).unwrap();
    }
    reaplicar
}

/// A medida do lote: outro servidor exige o mesmo MOD na 2.0.0, e a janela
/// o obtém. É a mesma chamada na pergunta dos MODs, depois de a pessoa
/// desistir dela (a obtenção segue até o fim) e na sessão que já aceitou.
#[test]
fn obter_o_que_outro_servidor_exige_nao_troca_a_versao_do_servidor_hospedado() {
    let banco = hospedado();
    let antes = exigido(&banco);

    let reaplicou = depois_de_baixar(&banco, UsoDoCatalogo::Obter, &versao("2.0.0", 'b'));

    let depois = exigido(&banco);
    assert_eq!(
        depois, antes,
        "obter o MOD que outro servidor exige trocou a versão exigida pelo servidor que \
             esta janela hospeda, de {} para {}: quem estava dentro dele cai, e ele passa a \
             exigir uma versão que quem hospeda não escolheu (ADR 0045)",
        antes.0, depois.0
    );
    assert!(
        !reaplicou,
        "a obtenção para outro servidor respondeu que reaplicou no servidor hospedado aqui"
    );
}

/// O QA-01 continua: atualizar pela gestão um MOD que o servidor daqui
/// exige passa a exigência aos bytes novos, e um que ele não exige fica
/// desligado.
#[test]
fn a_atualizacao_pela_gestao_continua_reaplicando() {
    let banco = hospedado();
    let antes = exigido(&banco);

    let reaplicou = depois_de_baixar(&banco, UsoDoCatalogo::Atualizar, &versao("2.0.0", 'b'));

    let depois = exigido(&banco);
    assert!(
        reaplicou && depois.0 == "2.0.0" && depois.1 != antes.1,
        "atualizar pela gestão um MOD exigido aqui deixou de reaplicar: o disco tem a \
             versão nova e o servidor continua exigindo a antiga (QA-01) — {antes:?} → {depois:?}"
    );

    let vazio = Persistence::open(&Location::Memory).unwrap();
    assert!(
        !depois_de_baixar(&vazio, UsoDoCatalogo::Atualizar, &versao("2.0.0", 'b'))
            && enabled(&vazio).unwrap().is_empty(),
        "atualizar um MOD que o servidor daqui não exige o ligou: atualizar não é ligar"
    );
}

/// A janela diz o uso com estas palavras (`camada-mods.js`, `base.js`), e
/// nenhuma outra passa.
#[test]
fn a_janela_diz_o_uso_com_as_palavras_que_o_comando_entende() {
    let ler = |texto: &str| serde_json::from_str::<UsoDoCatalogo>(texto).ok();
    assert_eq!(
        (ler("\"obter\""), ler("\"atualizar\"")),
        (Some(UsoDoCatalogo::Obter), Some(UsoDoCatalogo::Atualizar)),
        "o comando deixou de entender a palavra que a janela manda: toda obtenção e \
             toda atualização pelo catálogo são recusadas antes de baixar"
    );
    assert_eq!(
        (ler("null"), ler("\"Atualizar\"")),
        (None, None),
        "o comando aceita um uso que a janela não escreve: uma chamada sem uso passaria \
             a ter um padrão calado"
    );
}

/// O comando reaplica pela decisão que lê o uso, e por nenhum outro
/// caminho. Sem isto, a decisão acima podia estar certa e o comando voltar
/// a reaplicar por `mod_exigido_aqui` sozinho.
///
/// A agulha do cabeçalho é montada por `concat!` (rodada 1): escrita
/// inteira, ela casava com este mesmo teste quando o comando mudava de
/// nome ou de cabeçalho, e o guarda conferia a si mesmo.
#[test]
fn o_comando_reaplica_pela_decisao_que_le_o_uso() {
    let fonte = fonte_sem_comentarios();
    let corpo = corpo_de(&fonte, concat!("async fn instalar_mod_do_catalogo", "("));
    let decide = corpo.find("reaplica_no_hospedado(uso,");
    let aplica = corpo.find("if reaplicar {");
    assert!(
        corpo.contains("uso: UsoDoCatalogo,")
            && matches!((decide, aplica), (Some(decide), Some(aplica)) if decide < aplica)
            && corpo.matches("aplicar_mod(").count() == 1
            && corpo.contains("Ok(reaplicar)"),
        "`instalar_mod_do_catalogo` deixou de reaplicar só pela decisão que lê o uso: a \
             obtenção para outro servidor volta a trocar o MOD do servidor hospedado aqui \
             (ADR 0045):\n{corpo}"
    );
}
