use super::{
    corpo_de, endereco_desta_maquina, fonte_sem_comentarios, onde_estou_em, porta_desta_maquina,
    Hospedado, Hospedagens, OndeEstou,
};
use seele_server::hospedagem::Hospedagem;

/// A chave do guardado, a da sala e a de um servidor S qualquer.
const GUARDADO: &str = "aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11";
const SALA: &str = "bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22";
const DE_S: &str = "cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33";

#[test]
fn onde_estou_pela_chave_e_nao_pelo_endereco() {
    assert_eq!(
        onde_estou_em(None, None, Some(GUARDADO), Some(SALA)),
        OndeEstou::Nenhum,
        "sem sessão, não há onde estar"
    );
    // Os cinco endereços por onde se entra na mesma hospedagem: a chave é
    // a mesma, e a resposta também. A impressão chega da conexão em
    // maiúsculas aqui de propósito: a comparação não as diferencia.
    let fora: Vec<&str> = [
        "127.0.0.1:8383",
        "localhost:8383",
        "[::1]:8383",
        "192.168.1.20:8383",
        "casa.exemplo:8383",
    ]
    .into_iter()
    .filter(|endereco| {
        onde_estou_em(
            Some(endereco),
            Some(&GUARDADO.to_ascii_uppercase()),
            Some(GUARDADO),
            Some(SALA),
        ) != OndeEstou::Guardado
    })
    .collect();
    assert!(
        fora.is_empty(),
        "a sessão com a chave do guardado, aberta por {fora:?}, não foi dita no guardado: \
             o próprio link e os outros nomes da mesma máquina deixam de ser o servidor daqui"
    );
    assert_eq!(
        onde_estou_em(
            Some("127.0.0.1:8385"),
            Some(SALA),
            Some(GUARDADO),
            Some(SALA)
        ),
        OndeEstou::Pessoal,
        "a sessão com a chave da sala não foi dita na sala"
    );
}

#[test]
fn o_mesmo_endereco_com_outra_chave_e_outro_lugar() {
    // Um servidor S no mesmo endereço do guardado, com outra chave: o
    // HOSPEDAR COM ESTA da F6 pondo outra versão em 127.0.0.1, ou um
    // endereço da rede local que hoje é de outra máquina.
    for endereco in ["127.0.0.1:8383", "192.168.1.20:8383", "127.0.0.1:8385"] {
        assert_eq!(
            onde_estou_em(Some(endereco), Some(DE_S), Some(GUARDADO), Some(SALA)),
            OndeEstou::Outro,
            "um servidor com outra chave em {endereco} passou por uma das hospedagens desta \
                 janela: o sim de quem hospeda iria a ele (ADR 0045)"
        );
    }
    // Sem a hospedagem no ar, a chave dela não é de ninguém aqui.
    assert_eq!(
        onde_estou_em(Some("127.0.0.1:8383"), Some(GUARDADO), None, Some(SALA)),
        OndeEstou::Outro,
        "sem guardado no ar, a sessão foi dita no guardado"
    );
    assert_eq!(
        onde_estou_em(Some("127.0.0.1:8385"), Some(SALA), Some(GUARDADO), None),
        OndeEstou::Outro,
        "sem sala no ar, a sessão foi dita na sala"
    );
    // Com sessão e sem impressão, falha fechado.
    assert_eq!(
        onde_estou_em(Some("127.0.0.1:8383"), None, Some(GUARDADO), Some(SALA)),
        OndeEstou::Outro,
        "uma sessão sem a chave conferida foi dita numa hospedagem desta janela"
    );
}

#[test]
fn um_endereco_desta_maquina_como_a_pessoa_o_escreve() {
    for (escrito, porta) in [
        ("127.0.0.1:8383", Some(8383)),
        ("127.0.0.1", Some(8383)),
        ("localhost:8385", Some(8385)),
        ("localhost", Some(8383)),
        ("[::1]:8385", Some(8385)),
        ("[::1]", Some(8383)),
        ("  127.0.0.1:8383 ", Some(8383)),
        ("192.168.0.7:8383", None),
        ("casa.exemplo", None),
        ("127.0.0.1.casa.exemplo:8383", None),
    ] {
        assert_eq!(porta_desta_maquina(escrito), porta, "{escrito:?}");
    }
}

#[test]
fn a_lista_de_visitados_continua_sem_os_enderecos_desta_maquina() {
    // A regra da lista é outra pergunta, e ela não mudou: um endereço desta
    // máquina não é lugar aonde se volta (o `hospedado_aqui` de antes).
    for desta in ["127.0.0.1:8383", "localhost", "[::1]:8385", "127.0.0.1"] {
        assert!(endereco_desta_maquina(desta), "{desta:?}");
    }
    for de_fora in ["192.168.0.7:8383", "casa.exemplo:8383", "203.0.113.7"] {
        assert!(!endereco_desta_maquina(de_fora), "{de_fora:?}");
    }
}

/// **As impressões das duas hospedagens, cada uma no seu lugar** (lote X da
/// F5, a impressão, commit D). `onde_estou_em` recebe a do guardado em
/// terceiro e a da sala em quarto; o par trocado diria `Guardado` na sala e
/// gravaria nela o sim de quem hospeda.
///
/// Duas salas, e não um servidor guardado e uma sala, pela razão que
/// `a_saida_despede_as_hospedagens` escreve: este arquivo só sobe
/// hospedagem pelo ponto padrão. Sementes diferentes, chaves diferentes.
#[tokio::test]
async fn as_impressoes_saem_na_ordem_do_guardado_e_da_sala() {
    let guardado = Hospedagem::iniciar_sala(0, &[3; 32], "casa", None)
        .await
        .expect("o guardado não subiu");
    let sala = Hospedagem::iniciar_sala(0, &[4; 32], "dona", None)
        .await
        .expect("a sala não subiu");
    let do_guardado = guardado.impressao_digital().to_owned();
    let da_sala = sala.impressao_digital().to_owned();
    assert_ne!(
        do_guardado, da_sala,
        "as duas hospedagens saíram com a mesma chave, e este teste não mede a ordem"
    );

    let mut hospedagens = Hospedagens {
        guardado: Some(Hospedado::novo(guardado, None)),
        pessoal: Some(Hospedado::novo(sala, None)),
    };
    assert_eq!(
        hospedagens.impressoes(),
        (Some(do_guardado), Some(da_sala)),
        "`Hospedagens::impressoes` não devolve a chave do guardado primeiro e a da sala \
             depois: a sessão na sala passa por guardado, e o sim de quem hospeda vai a ela \
             (ADR 0045)"
    );

    for hospedado in [hospedagens.guardado.take(), hospedagens.pessoal.take()]
        .into_iter()
        .flatten()
    {
        hospedado.hospedagem.encerrar().await;
    }
}

/// **A ligação de `onde_estou_na`** (commit D). Os testes acima são puros:
/// recebem as impressões prontas. Este prende o que a função da sessão
/// passa a eles: a chave que a **conexão** conferiu em segundo, e as das
/// hospedagens, na ordem de `impressoes()`, em terceiro e quarto. Trocar a
/// chave da sessão pela do guardado diria `Guardado` em qualquer sessão
/// com o guardado de pé, e o sim de quem hospeda iria a S.
///
/// Olha a chamada e as ligações que a alimentam, sem espaço e sem as
/// linhas de comentário, com as agulhas montadas por `concat!`.
#[test]
fn onde_estou_na_passa_a_chave_da_conexao_e_as_das_hospedagens() {
    let fonte = fonte_sem_comentarios();
    let corpo: String = corpo_de(&fonte, concat!("fn onde_estou_na", "(session"))
        .chars()
        .filter(|letra| !letra.is_whitespace())
        .collect::<String>()
        // A vírgula final que o rustfmt põe numa chamada partida em linhas.
        .replace(",)", ")");
    let ligacao = |nome: &str| {
        corpo
            .split_once(nome)
            .and_then(|(_, depois)| depois.split_once(';'))
            .map(|(ligacao, _)| ligacao.to_owned())
            .unwrap_or_default()
    };
    let da_sessao = ligacao(concat!("let", "impressao="));
    let das_hospedagens = ligacao(concat!("let(guardado,", "pessoal)="));
    assert!(
        da_sessao.starts_with(concat!("session.connection", "()"))
            && da_sessao.contains(concat!(".impressao_do_", "servidor()"))
            && das_hospedagens.contains(concat!(".impressoes", "()"))
            && corpo.contains(concat!(
                "onde_estou_em(endereco.as_deref(),",
                "impressao.as_deref(),guardado.as_deref(),pessoal.as_deref())"
            )),
        "`onde_estou_na` deixou de passar a `onde_estou_em` a chave que a conexão conferiu \
             em segundo e as das hospedagens, na ordem de `impressoes()`, em terceiro e \
             quarto: a sessão em S ou na sala passa por guardado, e o sim de quem hospeda vai \
             a ela (ADR 0045):\n{corpo}"
    );
}
