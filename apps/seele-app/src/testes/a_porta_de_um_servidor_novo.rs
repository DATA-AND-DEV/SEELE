//! **A porta de um servidor novo se escolhe antes de ele subir** (pedido do
//! dono, 13/11/2026: «quando você hospeda um servidor, já tem que mostrar se
//! ele quer senha ou não na hora de configurar»).
//!
//! Pelo corpo de verdade do comando (`hospedar_em`, em porta zero e no modo
//! local), com gente de outra máquina batendo pela FFI:
//!
//! - **com senha**, o servidor nasce exigindo a senha desde a primeira
//!   conexão, e quem a tem entra direto, sem esperar aprovação;
//! - **sem senha**, nasce no portão, como sempre: quem chega fica esperando
//!   quem hospeda;
//! - a senha que não serve é recusada antes de qualquer coisa subir;
//! - a escolha de servidor novo não cai sobre um servidor que já tinha a porta
//!   dele;
//! - e a senha não aparece no registro.

use super::a_entrada_velha_que_chega_tarde::{app, runtime, vez};
use super::{
    config_dir, corpo_de, fonte_sem_comentarios, hospedar_em, parar_de_hospedar_em,
    preparar_a_casa_de_quem_hospeda, Anfitriao, FalhaAoHospedar, PedidoDeHospedar, Session,
};
use crate::senha_do_servidor::{self, SenhaRecusada};
use seele_ffi::{ConnectConfig, Connection, ConnectionError, EndReason};
use seele_server::persistence::{Location, Persistence};
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;

/// A senha dos testes. Duas palavras que não aparecem em nenhuma linha de
/// registro por outro motivo, para a busca no rastro não dar falso positivo.
const SENHA: &str = "ramiel octaedro";

/// Um servidor novo no registro desta casa, com o banco só dele.
fn servidor_novo(config: &str, nome: &str) -> String {
    crate::servidores::criar(config, nome, "1.1.1").id
}

/// Hospeda `servidor` pelo corpo do comando, com a porta escolhida.
async fn hospedar(
    session: &Session,
    config: &str,
    servidor: &str,
    senha: Option<&str>,
) -> Result<Anfitriao, FalhaAoHospedar> {
    let pedido = PedidoDeHospedar {
        nome_publico: None,
        servidor: Some(servidor.to_owned()),
        senha: senha.map(str::to_owned),
    };
    hospedar_em(session, config, None, pedido, 0, true).await
}

/// Alguém de outra máquina (outra casa, outra chave) bate em `alvo`.
async fn bater(
    alvo: &str,
    apelido: &str,
    segredo: Option<&str>,
) -> Result<Arc<Connection>, ConnectionError> {
    let casa = tempfile::tempdir().expect("a casa da outra máquina");
    let config = ConnectConfig {
        server: alvo.to_owned(),
        alternate_servers: Vec::new(),
        nickname: apelido.into(),
        home: casa.path().display().to_string(),
        join_secret: segredo.map(str::to_owned),
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::spawn_blocking(move || {
            // A casa vive até a tentativa acabar: a chave dela é lida no aperto.
            let _casa = casa;
            Connection::connect(config).map(|(conexao, _)| conexao)
        }),
    )
    .await
    .expect("a batida de outra máquina não voltou no prazo")
    .expect("a thread da outra máquina caiu")
}

/// O motivo com que o servidor recusou, ou o que veio no lugar.
fn recusa(resultado: Result<Arc<Connection>, ConnectionError>) -> Option<EndReason> {
    match resultado {
        Err(ConnectionError::Refused { reason }) => Some(reason),
        Ok(conexao) => {
            conexao.disconnect();
            None
        }
        Err(outro) => panic!("a batida não foi atendida nem recusada pelo servidor: {outro:?}"),
    }
}

/// A porta do servidor guardado de pé: `(tem senha, portaria ligada)`.
async fn a_porta(session: &Session) -> (bool, bool) {
    let banco = session
        .hospedagens
        .lock()
        .expect("as hospedagens")
        .do_guardado()
        .expect("o guardado de pé")
        .persistence();
    let banco = banco.lock().await;
    (
        seele_server::admissao::Politica::carregar(&banco)
            .expect("a política")
            .tem_senha(),
        seele_server::portaria::ligada(&banco).expect("a portaria"),
    )
}

#[test]
fn hospedar_com_senha_exige_a_senha_desde_a_primeira_conexao_e_ela_nao_vai_ao_registro() {
    let _vez = vez();
    let ((), rastro) = super::rastro_de_teste::capturar(|| {
        runtime().block_on(async {
            let app = app();
            let session = app.state::<Session>();
            let config = config_dir(app);
            let servidor = servidor_novo(&config, "Com senha");
            let anfitriao = hospedar(&session, &config, &servidor, Some(SENHA))
                .await
                .unwrap_or_else(|falha| panic!("o hospedar com senha não subiu: {falha:?}"));

            // **A primeira conexão que este servidor vê é a de um estranho sem
            // senha**, antes de quem hospeda entrar: se a senha fosse posta
            // depois de subir, era aqui que ele entrava.
            assert_eq!(
                recusa(bater(&anfitriao.aqui, "estranha", None).await),
                Some(EndReason::CredentialRejected),
                "um servidor hospedado com senha atendeu a primeira conexão sem ela"
            );
            assert_eq!(
                recusa(bater(&anfitriao.aqui, "estranha", Some("ramiel tetraedro")).await),
                Some(EndReason::CredentialRejected),
                "a senha errada passou"
            );
            assert_eq!(
                a_porta(&session).await,
                (true, false),
                "hospedar com senha não deixou a porta como «com senha, sem portaria»"
            );

            // Quem tem a senha entra direto: nem espera, nem pedido na fila.
            let com_senha = bater(&anfitriao.aqui, "convidada", Some(SENHA))
                .await
                .unwrap_or_else(|erro| panic!("quem tem a senha não entrou direto: {erro:?}"));
            com_senha.disconnect();

            // E quem hospeda entra na própria casa sem digitar a senha: a chave
            // desta máquina já está admitida (`portaria::admitir_o_dono`).
            let dona = bater_daqui(&config, &anfitriao.aqui).await;
            dona.unwrap_or_else(|erro| {
                panic!("quem hospeda ficou de fora do servidor com senha: {erro:?}")
            })
            .disconnect();

            assert!(parar_de_hospedar_em(&session).await, "havia guardado no ar");
        });
    });
    // O rastro viu o servidor trabalhar (a recusa de quem veio sem senha é dita
    // ao operador), e por isso a ausência da senha nele quer dizer alguma coisa.
    assert!(
        rastro.contains("SegredoAusente") && rastro.contains("SegredoInvalido"),
        "o rastro capturado não viu as recusas do servidor, e não prova nada sobre a senha: {rastro}"
    );
    for pedaco in ["ramiel", "octaedro", "tetraedro"] {
        assert!(
            !rastro.contains(pedaco),
            "a senha do servidor (ou a tentativa de alguém) foi parar no registro: {rastro}"
        );
    }
}

/// Quem hospeda, com a chave desta máquina (a casa do app), pela FFI.
async fn bater_daqui(config: &str, alvo: &str) -> Result<Arc<Connection>, ConnectionError> {
    let config = ConnectConfig {
        server: alvo.to_owned(),
        alternate_servers: Vec::new(),
        nickname: "dona".into(),
        home: config.to_owned(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::spawn_blocking(move || Connection::connect(config).map(|(c, _)| c)),
    )
    .await
    .expect("a entrada de quem hospeda não voltou no prazo")
    .expect("a thread de quem hospeda caiu")
}

#[test]
fn hospedar_sem_senha_nasce_no_portao_como_antes() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let config = config_dir(app);
        let servidor = servidor_novo(&config, "No portão");
        let anfitriao = hospedar(&session, &config, &servidor, None)
            .await
            .unwrap_or_else(|falha| panic!("o hospedar não subiu: {falha:?}"));

        assert_eq!(
            a_porta(&session).await,
            (false, true),
            "hospedar sem senha não nasceu «sem senha, com a portaria ligada»"
        );
        assert_eq!(
            recusa(bater(&anfitriao.aqui, "estranha", None).await),
            Some(EndReason::AdmissionPending),
            "no portão, quem chega bate e espera quem hospeda"
        );
        bater_daqui(&config, &anfitriao.aqui)
            .await
            .unwrap_or_else(|erro| {
                panic!("quem hospeda ficou esperando no próprio portão: {erro:?}")
            })
            .disconnect();

        assert!(parar_de_hospedar_em(&session).await, "havia guardado no ar");
    });
}

#[test]
fn a_senha_que_nao_serve_e_recusada_antes_de_qualquer_coisa_subir() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let config = config_dir(app);
        let servidor = servidor_novo(&config, "Recusada");
        for (digitada, motivo) in [
            ("", SenhaRecusada::Vazia),
            ("   ", SenhaRecusada::Vazia),
            ("1234", SenhaRecusada::Curta),
            (" ramiel octaedro", SenhaRecusada::ComEspacoNaPonta),
        ] {
            let resultado = hospedar(&session, &config, &servidor, Some(digitada)).await;
            assert!(
                matches!(&resultado, Err(FalhaAoHospedar::SenhaRecusada(m)) if *m == motivo),
                "a senha de {} caracteres não foi recusada como {motivo:?}: {resultado:?}",
                digitada.chars().count()
            );
            assert!(
                session
                    .hospedagens
                    .lock()
                    .expect("as hospedagens")
                    .guardado
                    .is_none(),
                "uma senha recusada deixou um servidor no ar"
            );
        }
        // E o servidor continua sem porta escolhida: a senha boa ainda vale.
        hospedar(&session, &config, &servidor, Some(SENHA))
            .await
            .unwrap_or_else(|falha| panic!("depois das recusas, a senha boa não subiu: {falha:?}"));
        assert_eq!(a_porta(&session).await, (true, false));
        assert!(parar_de_hospedar_em(&session).await, "havia guardado no ar");
    });
}

#[test]
fn a_senha_de_um_servidor_novo_nao_cai_sobre_um_que_ja_tinha_a_porta_dele() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let config = config_dir(app);
        let servidor = servidor_novo(&config, "Já existia");
        // Hospedado uma vez no portão: a portaria está decidida.
        hospedar(&session, &config, &servidor, None)
            .await
            .unwrap_or_else(|falha| panic!("a primeira subida falhou: {falha:?}"));
        assert!(parar_de_hospedar_em(&session).await, "havia guardado no ar");

        let resultado = hospedar(&session, &config, &servidor, Some(SENHA)).await;
        assert!(
            matches!(resultado, Err(FalhaAoHospedar::PortaJaEscolhida)),
            "a senha de servidor novo caiu sobre um servidor que já tinha a porta dele: \
             {resultado:?}"
        );
        assert!(
            session
                .hospedagens
                .lock()
                .expect("as hospedagens")
                .guardado
                .is_none(),
            "a recusa deixou um servidor no ar"
        );

        // Nada mudou: ele sobe de novo como estava, no portão e sem senha.
        hospedar(&session, &config, &servidor, None)
            .await
            .unwrap_or_else(|falha| panic!("depois da recusa, o servidor não subiu: {falha:?}"));
        assert_eq!(
            a_porta(&session).await,
            (false, true),
            "a escolha recusada mexeu na porta do servidor"
        );
        assert!(parar_de_hospedar_em(&session).await, "havia guardado no ar");
    });
}

#[test]
fn o_servidor_com_senha_continua_com_senha_e_sem_portaria_na_subida_seguinte() {
    // A semente da portaria roda em toda subida. Num servidor que nasceu com
    // senha ela não pode religar a portaria por cima da escolha.
    let mut banco = Persistence::open(&Location::Memory).expect("o banco em memória");
    let minha = "a".repeat(64);
    let senha = senha_do_servidor::conferir(SENHA).expect("a senha serve");

    preparar_a_casa_de_quem_hospeda(&mut banco, &minha, Some(&senha))
        .expect("a casa de um servidor novo com senha");
    preparar_a_casa_de_quem_hospeda(&mut banco, &minha, None).expect("a subida seguinte");

    let politica = seele_server::admissao::Politica::carregar(&banco).expect("a política");
    assert!(politica.tem_senha(), "a subida seguinte tirou a senha");
    assert!(
        !seele_server::portaria::ligada(&banco).expect("a portaria"),
        "a subida seguinte religou a portaria de um servidor que nasceu com senha"
    );
    assert!(
        politica.senha_confere(Some(SENHA)),
        "a senha gravada não é a escolhida"
    );
    assert_eq!(
        politica.avaliar(&banco, None).expect("avaliar"),
        Err(seele_server::admissao::Recusa::SegredoAusente),
        "sem a senha, a camada do segredo deixou passar"
    );
}

#[test]
fn a_senha_e_gravada_antes_de_o_servidor_aceitar() {
    // A ordem, lida do `hospedar_em` de verdade: a senha é conferida na
    // primeira linha, vai para dentro do `antes_de_aceitar`, e é ele que a
    // grava — com o laço de aceitação parado
    // (`crates/seele-server/tests/janela_do_comandante.rs` prova o gancho).
    let fonte = fonte_sem_comentarios();
    let corpo = corpo_de(&fonte, concat!("async fn hospedar_em", "("));
    let onde = |trecho: &str| {
        corpo
            .find(trecho)
            .unwrap_or_else(|| panic!("o `hospedar` não tem mais `{trecho}`"))
    };
    let ordem = [
        onde("senha_do_servidor::conferir"),
        onde("conferir_nome_publico"),
        onde("let antes_de_aceitar = move |"),
        onde("preparar_a_casa_de_quem_hospeda(persistence, &minha, senha.as_ref())"),
        onde("subir_o_guardado("),
    ];
    assert!(
        ordem
            .windows(2)
            .all(|par| matches!(par, [antes, depois] if antes < depois)),
        "a senha de um servidor novo não é mais conferida primeiro e gravada no \
         `antes_de_aceitar`, antes da subida: {ordem:?}"
    );
    let depois_de_subir = corpo
        .split_once("subir_o_guardado(")
        .map_or("", |(_, depois)| depois);
    assert!(
        !depois_de_subir.contains("senha"),
        "o `hospedar` mexe na senha depois de o servidor subir: é a janela em que ele \
         fica no ar no modo errado"
    );

    // **A senha crua só existe até a primeira linha**, e o texto dela só sai
    // do `SenhaDoServidor` para o `definir_senha`, que calcula o hash.
    let casa = corpo_de(&fonte, concat!("fn preparar_a_casa_de_quem_hospeda", "("));
    assert!(
        casa.contains("seele_server::admissao::definir_senha(persistence, Some(senha.texto()))")
            && casa.contains("seele_server::portaria::ligar(persistence, false)"),
        "a casa de um servidor novo com senha não a grava pelas funções do painel A PORTA"
    );
    for (nome, texto) in [
        ("hospedar_em", corpo),
        ("preparar_a_casa_de_quem_hospeda", casa),
    ] {
        for linha in texto.lines().filter(|linha| linha.contains("senha")) {
            assert!(
                !linha.contains("tracing::")
                    && !linha.contains("format!")
                    && !linha.contains("{senha"),
                "`{nome}` escreve a senha num registro ou numa frase: {linha}"
            );
        }
    }
}
