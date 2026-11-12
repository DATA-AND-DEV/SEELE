use super::{classify_connect_failure, fim_da_sessao, ConnectionError, EndReason};
use seele_core::enlace::Motivo;
use seele_core::{ConnectError, DisconnectReason};

/// **O `OutraLinha` diz a relação**: a versão só quando o launcher a abre,
/// e se o servidor é mais novo que este app (decisão da orquestradora de
/// 15/10). Antes, o 30 vinha com «1», uma versão que ele não é.
#[test]
fn outra_linha_diz_a_versao_so_quando_o_launcher_a_abre_e_se_e_mais_nova() {
    let caso = |numero| classify_connect_failure(&ConnectError::OutraLinha { numero });
    assert_eq!(
        caso(8),
        ConnectionError::OutraLinha {
            numero: 8,
            servidor: Some("0.15".into()),
            mais_nova: false,
        },
        "o Servidor 0.15 publicado"
    );
    assert_eq!(
        caso(30),
        ConnectionError::OutraLinha {
            numero: 30,
            servidor: None,
            mais_nova: true,
        },
        "acima desta build: mais novo, e sem uma versão que ele não é"
    );
    assert_eq!(
        caso(7),
        ConnectionError::OutraLinha {
            numero: 7,
            servidor: None,
            mais_nova: false,
        },
        "abaixo da 0.15: antigo, e sem versão oferecida"
    );
}

#[test]
fn quem_nao_prova_a_chave_nao_chega_como_versao_incompativel() {
    // Era `TlsRefused`, que esta tradução manda para
    // `Refused { Incompatible }`: o impostor que repete o certificado do
    // servidor aparecia como versão incompatível, e a tela mandava
    // atualizar o SEELE nas duas máquinas.
    assert_eq!(
        classify_connect_failure(&ConnectError::ChaveNaoProvada),
        ConnectionError::ChaveNaoProvada,
        "quem não provou a chave do servidor chega à tela com outro nome, e a frase \
             manda atualizar o SEELE por causa de um impostor"
    );
}

#[test]
fn fim_da_recusa_diz_o_motivo_de_verdade_e_nao_credencial_recusada() {
    // A volta da bateria recusada acabava em `CredentialRejected` qualquer
    // que fosse o motivo, e a tela mandava conferir convite e senha: sobre
    // um servidor que não é o do convite, sobre a chave que mudou, sobre o
    // servidor lotado e sobre os MODs que mudaram. Passa pelo mesmo
    // `fim_da_sessao` que `drive` chama, para que a fiação conte junto.
    let casos = [
        (
            "InviteMismatch",
            ConnectError::InviteMismatch {
                expected: "bbbb2222".into(),
                offered: "aaaa1111".into(),
            },
            EndReason::ServidorTrocouNaVolta,
        ),
        (
            "Refused { ServerFull }",
            ConnectError::Refused {
                reason: DisconnectReason::ServerFull,
            },
            EndReason::ServerFull,
        ),
        (
            "PinChanged",
            ConnectError::PinChanged {
                pinned: "aaaa1111".into(),
                offered: "cccc3333".into(),
            },
            EndReason::ChaveMudouNaVolta,
        ),
        (
            "ModsNaoAceitos",
            ConnectError::ModsNaoAceitos {
                mods: Vec::new(),
                conjunto: "conjunto".into(),
                plataformas: None,
            },
            EndReason::ModsMudaram,
        ),
        (
            "ChaveNaoProvada",
            ConnectError::ChaveNaoProvada,
            EndReason::ServidorTrocouNaVolta,
        ),
    ];
    let erradas: Vec<String> = casos
        .into_iter()
        .filter_map(|(nome, erro, esperado)| {
            let obtido = fim_da_sessao(&Motivo::Recusado(erro));
            (obtido != esperado).then(|| format!("{nome} deu {obtido:?}, e não {esperado:?}"))
        })
        .collect();
    assert!(
        erradas.is_empty(),
        "a volta recusada chega à tela com um motivo que não é o dela, e a frase manda \
             a pessoa conferir o que não está errado: {erradas:#?}"
    );
}

#[test]
fn fim_da_recusa_do_que_nao_e_recusa_e_enlace_perdido() {
    // Estas não chegam a `Recusado` hoje (`vale_insistir` as manda de volta
    // à bateria). Se uma chegar, ela é rede, e não credencial.
    for erro in [
        ConnectError::LocalEndpoint,
        ConnectError::Unreachable,
        ConnectError::TlsRefused,
        ConnectError::HandshakeTimeout,
        ConnectError::SemResposta,
        ConnectError::ProtocolViolation,
    ] {
        let obtido = fim_da_sessao(&Motivo::Recusado(erro.clone()));
        assert_eq!(
            obtido,
            EndReason::LinkLost,
            "a volta que acabou em {erro:?} chega à tela como {obtido:?}, e não como \
                 enlace perdido: a frase manda procurar outra causa para uma falha de rede"
        );
    }
}

/// A ligação de `drive` a [`fim_da_sessao`], que nenhum teste de
/// comportamento alcança.
///
/// Os testes de cima provam `fim_da_sessao`. `drive` só a usa com uma
/// sessão viva cuja volta pela bateria o servidor recuse, e nenhum teste
/// deste crate abre uma. Sem este guarda, um `match` reescrito à mão dentro
/// de `drive` (o `Recusado(_) => CredentialRejected` de antes do lote H)
/// deixaria todos verdes, e a tela voltaria a dizer «credencial recusada»
/// para toda volta recusada.
///
/// Comentários fora, para que um parágrafo que cite a chamada não conte
/// como a chamada. As agulhas são montadas, e não escritas: este é o
/// arquivo que o teste lê.
#[test]
fn drive_traduz_o_fim_que_o_enlace_avisa_por_fim_da_sessao() {
    let fonte = include_str!("../lib.rs");
    let sem_comentario: String = fonte
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let inicio = sem_comentario
        .find(&format!("{} fn drive(", "async"))
        .expect("`drive` sumiu de `lib.rs`");
    let depois = sem_comentario.get(inicio..).expect("o começo de `drive`");
    let fim = depois.find("\n}\n").expect("`drive` nunca fecha");
    let corpo = depois.get(..fim).expect("o corpo de `drive`");
    let Some(braco) = corpo
        .split(&format!("enlace::Aviso::{}(motivo) =>", "Encerrado"))
        .nth(1)
    else {
        panic!(
            "`drive` não trata mais o fim que o enlace avisa pelo nome de sempre, e este \
                 guarda deixou de achar o braço que ele prende"
        );
    };
    let braco = braco
        .split(&format!("{};", "break"))
        .next()
        .unwrap_or(braco);
    assert!(
        braco.contains(&format!("{}(&motivo)", "fim_da_sessao")),
        "`drive` traduz o fim da sessão sem passar por `fim_da_sessao`: os testes de \
             `fim_da_recusa` ficam verdes e a tela pode voltar a dizer «credencial recusada» \
             para toda volta recusada:\n{braco}"
    );
}
