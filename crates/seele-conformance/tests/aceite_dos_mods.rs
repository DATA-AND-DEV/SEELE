//! «Quem não aceita, não entra» — servidor de verdade contra cliente de
//! verdade. ADR 0045.
//!
//! Este é o único crate onde as duas pontas cabem juntas (ADR 0002), e é aqui
//! que a **costura** entre elas é provada: o `seeled` monta o anúncio da tabela
//! e o escreve no fio; o `seele_core::Client` o lê, compara com o que esta
//! máquina guardou, e responde. Nenhum dos dois lados tem dublê.
//!
//! # As duas metades, e por que elas são testes diferentes
//!
//! **O portão de pé, e desde 14/09/2026 é o de todo servidor.** O anúncio viaja
//! em [`seele_proto::mods::VERSAO_DO_ANUNCIO`], e a integração conjunta com a
//! malha subiu `PROTOCOL_VERSION` para 5, que o alcança. Estes testes nasceram
//! baixando o limiar por `ServerConfig::versao_do_anuncio` porque era o único
//! jeito de o caminho verdadeiro correr; **agora eles usam a configuração
//! padrão, sem sobrescrever nada**, e o caminho corre inteiro assim mesmo:
//! anúncio, aceite, recusa, aceite velho e troca de MOD no meio da sessão. É
//! essa troca que prova que o anúncio saiu da dormência — não um limiar que o
//! teste escolheu.
//!
//! **O portão dormente continua coberto**, com um limiar acima da versão global
//! — a situação em que nenhum par pode aceitar e recusar seria cem por cento de
//! recusa em troca de nada. Os dois últimos testes provam que, nesse estado,
//! habilitar um MOD não tranca a porta nem derruba a sala: é a regressão que a
//! entrega do anúncio chegou a introduzir, e eles são o guarda dela.
//!
//! **E a janela de compatibilidade**, no fim: um par falando a versão anterior
//! continua entrando num servidor sem MOD, e é recusado por um que exige MOD.
//! O que estes testes **não** provam, de propósito, é que a última release
//! publicada continue entrando — não continua, e a medida disso está em
//! `seele_proto::control::a_release_publicada_recusa_o_carimbo_desta_build`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_core::frame;
use seele_core::{Client, ConnectError, MemoryPinStore};
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

async fn servidor_com_limiar(limiar: u8) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        versao_do_anuncio: limiar,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon))
}

/// Um servidor como quem hospeda o levanta: **nada sobrescrito**.
///
/// É de propósito que esta função não passa por `servidor_com_limiar`. Enquanto
/// o anúncio era dormente, todo teste da costura tinha de baixar o limiar à mão
/// para que o caminho verdadeiro corresse — e uma costura provada só sob um
/// limiar escolhido pelo teste não diz nada sobre o servidor que alguém liga em
/// casa. Com a integração conjunta cumprida, o padrão basta, e **é o padrão que
/// estes testes exercitam agora**. Se o anúncio voltar a ser dormente, os testes
/// da costura reprovam aqui em vez de continuarem verdes num mundo forjado.
async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon))
}

/// Um servidor com o portão **dormente**: limiar acima da versão global.
///
/// Nenhuma conexão pode negociar acima de `PROTOCOL_VERSION`, então com este
/// limiar não existe par capaz de ler o anúncio nem de responder a ele. Era o
/// estado de todo servidor antes de 14/09/2026; hoje só se chega nele por
/// configuração — ou pela próxima variante que nascer adiantada.
async fn servidor_com_o_portao_dormente() -> Result<(SocketAddr, Arc<Daemon>)> {
    servidor_com_limiar(seele_proto::version::PROTOCOL_VERSION.saturating_add(1)).await
}

fn um_mod(id: &str, hash_de: u8) -> EnabledMod {
    EnabledMod {
        id: id.to_owned(),
        version: "1.0.0".to_owned(),
        hash: format!("{hash_de:02x}").repeat(32),
        repo: "https://github.com/seele/exemplo".to_owned(),
        reach: vec!["ler".to_owned()],
        server_half: true,
    }
}

async fn habilitar(daemon: &Daemon, ligado: &EnabledMod) {
    let banco = daemon.server().persistence.lock().await;
    enable(&banco, ligado).expect("habilitar");
}

async fn entrar(endereco: SocketAddr, semente: u8) -> Result<Client, ConnectError> {
    entrar_tendo_aceito(endereco, semente, None).await
}

/// O mesmo, com um aceite já guardado por esta máquina.
///
/// `aceito` é o que `seele_core::aceites` devolveria: a identidade do conjunto
/// que a pessoa leu e aprovou numa visita anterior. É o argumento que a casca
/// passa, e é o caminho de produção do «aceitou uma vez, entra direto nas
/// próximas».
async fn entrar_tendo_aceito(
    endereco: SocketAddr,
    semente: u8,
    aceito: Option<&str>,
) -> Result<Client, ConnectError> {
    Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        &format!("pessoa{semente:03}"),
        &SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        aceito,
    )
    .await
}

/// A identidade do conjunto que este servidor exige agora.
///
/// Lida do mesmo lugar de onde o anúncio sai, e não recalculada no teste: o que
/// se quer provar é que o cliente aceita **o que o servidor anunciou**, e uma
/// segunda conta aqui provaria que duas contas concordam.
async fn conjunto_exigido(daemon: &Daemon) -> String {
    let banco = daemon.server().persistence.lock().await;
    seele_server::mods::anuncio::conjunto_exigido(&banco)
        .expect("montar o conjunto")
        .identidade
}

/// **O fluxo protegido não abre, e a pessoa recebe a pergunta.**
///
/// Este é o teste da costura: o servidor de produção monta o anúncio da tabela
/// e o escreve no fio; o cliente de produção o lê, não encontra aceite guardado
/// nenhum, manda `RecusarMods` e devolve à casca a lista inteira. Nenhum
/// `Session` sai.
///
/// O que ele confere na lista é o contrato da tela de aceite do ADR 0045:
/// identidade, versão, hash, repositório, o alcance declarado, e se o MOD roda
/// na máquina de quem hospeda. Sem esses campos a tela teria de baixar o MOD
/// para saber o que ele alcança — que é decidir antes de perguntar.
#[tokio::test(flavor = "multi_thread")]
async fn um_servidor_com_mod_devolve_a_lista_a_quem_nao_aceitou() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    habilitar(&daemon, &um_mod("seele/bot", 0xa1)).await;
    let exigido = conjunto_exigido(&daemon).await;

    let Err(erro) = entrar(endereco, 1).await else {
        panic!("entrou num servidor com MOD habilitado sem aceitar nada");
    };
    let ConnectError::ModsNaoAceitos { mods, conjunto } = erro else {
        panic!("a casca não recebeu a pergunta, recebeu {erro:?}");
    };

    assert_eq!(
        conjunto, exigido,
        "o conjunto anunciado não é o que a tabela exige"
    );
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].id, "seele/bot");
    assert_eq!(mods[0].version, "1.0.0");
    assert_eq!(mods[0].hash, "a1".repeat(32));
    assert_eq!(mods[0].repo, "https://github.com/seele/exemplo");
    assert_eq!(mods[0].reach, vec!["ler"]);
    assert!(
        mods[0].no_servidor,
        "a tela não teria como dizer que este MOD roda na máquina de quem hospeda"
    );

    daemon.shutdown();
    Ok(())
}

/// **E quem aceitou entra.** A outra metade da mesma costura, e o «aceitou uma
/// vez, entra direto nas próximas» do ADR 0045 pelo caminho de produção
/// inteiro: o aceite guardado sai da casca, o cliente responde `AceitarMods`
/// sozinho dentro do aperto de mão, e o `Session` vem.
///
/// # As duas entradas, no mesmo servidor, e por quê
///
/// Provar só que quem aceitou entra é uma prova degenerada: um portão
/// desligado passaria nela. O que separa as duas hipóteses é a **diferença**
/// entre as duas entradas contra o mesmo servidor, no mesmo instante — uma sem
/// aceite, que não entra, e uma com, que entra. É o critério inteiro numa
/// função: quem não aceita não acessa o fluxo protegido, e quem aceita acessa.
#[tokio::test(flavor = "multi_thread")]
async fn quem_ja_aceitou_o_conjunto_entra_sem_perguntar_de_novo() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    habilitar(&daemon, &um_mod("seele/bot", 0xa1)).await;
    let exigido = conjunto_exigido(&daemon).await;

    assert!(
        matches!(
            entrar(endereco, 11).await,
            Err(ConnectError::ModsNaoAceitos { .. })
        ),
        "sem aceite guardado o fluxo protegido abriu, e o portão não está de pé"
    );

    let cliente = entrar_tendo_aceito(endereco, 11, Some(&exigido))
        .await
        .expect("um aceite válido não abriu o fluxo protegido");
    // O `Session` é o fluxo protegido: é dele que saem as salas e os canais.
    assert!(
        !cliente.session().voice_rooms.is_empty(),
        "entrou sem receber o que o `Session` carrega"
    );

    daemon.shutdown();
    Ok(())
}

/// **Um sim dado a outra lista não é um sim a esta.**
///
/// O aceite de ontem, com o servidor tendo trocado de MOD desde então. O fluxo
/// protegido não abre, e o que volta é a pergunta com a **lista de hoje** — os
/// dois MODs, e não o um que a pessoa tinha lido.
///
/// # Quem barra, e por que isso importa
///
/// Quem barra aqui é o cliente: ele compara o conjunto anunciado com o que
/// guardou, vê que não bate, e recusa em vez de mandar o sim velho adiante. É a
/// ordem certa — a pessoa recebe o que mudou em vez de uma porta na cara — e é
/// o que faz «um servidor que troca de MOD pergunta de novo» (ADR 0045) custar
/// uma tela, e não um mistério.
///
/// O servidor tem o mesmo guarda do lado dele, para um par que mande o sim
/// velho assim mesmo: ele responde `ModsRecusados`. Esse não tem como ser
/// provado daqui, porque nenhum cliente de produção manda essa mensagem —
/// quem o prova é `um_aceite_de_outro_conjunto_nao_reabre_a_porta`, em
/// `seele_server::session`, com o quadro escrito à mão. Os dois juntos são a
/// mesma regra nas duas pontas.
#[tokio::test(flavor = "multi_thread")]
async fn um_aceite_de_ontem_nao_vale_para_o_conjunto_de_hoje() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    habilitar(&daemon, &um_mod("seele/bot", 0xa1)).await;
    let de_ontem = conjunto_exigido(&daemon).await;

    // O servidor troca de MOD. O aceite guardado continua sendo o de ontem.
    habilitar(&daemon, &um_mod("seele/cor", 0xb2)).await;
    let de_hoje = conjunto_exigido(&daemon).await;
    assert_ne!(
        de_ontem, de_hoje,
        "acrescentar um MOD tinha de mudar o conjunto"
    );

    let Err(erro) = entrar_tendo_aceito(endereco, 12, Some(&de_ontem)).await else {
        panic!("um aceite para outro conjunto abriu o fluxo protegido");
    };
    let ConnectError::ModsNaoAceitos { mods, conjunto } = erro else {
        panic!("o aceite velho não virou uma pergunta nova, virou {erro:?}");
    };
    assert_eq!(
        conjunto, de_hoje,
        "a pergunta veio com a identidade de ontem, e aceitá-la seria aceitar o que \
         ninguém leu"
    );
    assert_eq!(
        mods.len(),
        2,
        "a tela mostraria a lista velha a quem precisa ler a nova"
    );

    daemon.shutdown();
    Ok(())
}

/// **E a reconexão relê.** Depois da recusa acima, guardar o conjunto de hoje —
/// que é o que a casca faz quando a pessoa lê a tela e aprova — abre a porta na
/// tentativa seguinte, contra o mesmo servidor, sem ele ter mudado nada.
#[tokio::test(flavor = "multi_thread")]
async fn reconectar_com_o_conjunto_novo_aceito_abre_a_porta() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    habilitar(&daemon, &um_mod("seele/bot", 0xa1)).await;
    let de_ontem = conjunto_exigido(&daemon).await;
    habilitar(&daemon, &um_mod("seele/cor", 0xb2)).await;

    assert!(
        entrar_tendo_aceito(endereco, 13, Some(&de_ontem))
            .await
            .is_err(),
        "o aceite velho tinha de ser recusado antes desta segunda metade valer"
    );

    let de_hoje = conjunto_exigido(&daemon).await;
    entrar_tendo_aceito(endereco, 13, Some(&de_hoje))
        .await
        .expect("aceitar o conjunto novo não abriu a porta na reconexão");

    daemon.shutdown();
    Ok(())
}

/// E sem MOD habilitado nada muda. É o servidor de todo mundo hoje.
#[tokio::test(flavor = "multi_thread")]
async fn um_servidor_sem_mod_continua_deixando_entrar() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    entrar(endereco, 2)
        .await
        .expect("um servidor sem MOD recusou");
    daemon.shutdown();
    Ok(())
}

/// **Habilitar um MOD com gente dentro acaba com a sessão de quem está lá.**
///
/// Quem entrou aceitou o conjunto de então — o vazio —, e o servidor passou a
/// exigir outro. ADR 0045: «um servidor que troca de MOD pergunta de novo», e
/// perguntar acontece na entrada seguinte.
///
/// Sem isto, ligar um MOD só valeria para quem chegasse depois, e a sala inteira
/// continuaria conversando sem nunca ter lido o que passou a ser exigido.
#[tokio::test(flavor = "multi_thread")]
async fn habilitar_um_mod_acaba_com_a_sessao_de_quem_aceitou_outro_conjunto() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    // Um par cru, para ler a despedida no fio em vez de deduzi-la de um erro.
    let mut par = abrir(endereco, 3).await?;

    habilitar(&daemon, &um_mod("seele/cor", 0xb2)).await;

    assert_eq!(
        esperar_despedida(&mut par, Duration::from_secs(5)).await,
        Some(DisconnectReason::ModsMudaram),
        "a sessão continuou de pé depois de o servidor passar a exigir um MOD"
    );

    // **E a despedida foi entregue, não só escrita.** Ver
    // `motivo_do_fechamento`: sem a espera do servidor, a conexão é recolhida
    // por cima do quadro e quem lia via erro de transporte — «não foi possível
    // alcançar o servidor» — no lugar de «os MODs mudaram».
    assert_eq!(
        motivo_do_fechamento(&par, Duration::from_secs(5))
            .await
            .as_deref(),
        Some(b"mods changed".as_slice()),
        "a conexão caiu por cima da despedida em vez de ser fechada depois dela"
    );

    daemon.shutdown();
    Ok(())
}

/// Escrever no quintal de um MOD não é trocar de MOD, e não pode derrubar
/// ninguém. O contrário é o defeito que um aviso mal colocado produziria: toda
/// mensagem de sala com um bot ligado acabaria com a sessão de todo mundo.
#[tokio::test(flavor = "multi_thread")]
async fn uma_sessao_sobrevive_ao_que_nao_muda_o_conjunto() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let mut par = abrir(endereco, 4).await?;

    {
        let mut banco = daemon.server().persistence.lock().await;
        seele_server::persistence::mods::gravar_quintal(
            &mut banco,
            "seele/inexistente",
            &std::collections::BTreeMap::new(),
        )
        .expect("gravar");
    }

    assert_eq!(
        esperar_despedida(&mut par, Duration::from_millis(500)).await,
        None,
        "a sessão foi derrubada por uma escrita que não muda o que o servidor exige"
    );

    daemon.shutdown();
    Ok(())
}

// ------------------------------------------- o portão enquanto dorme
//
// Daqui para baixo o servidor tem o portão dormente: um limiar que nenhuma
// conexão pode negociar, e portanto nenhum par no mundo capaz de ler o anúncio.
// Os dois testes provam que, nesse estado, exigir um MOD não é a mesma coisa que
// fechar a casa.
//
// **Eles descreviam o servidor de todo mundo até 14/09/2026** e passaram a
// descrever um servidor configurado assim de propósito. Continuam aqui porque o
// ramo dormente continua no código: «existir não é funcionar», e um ramo que
// ninguém mais exercita apodrece em silêncio até a próxima variante nascer
// adiantada e precisar dele.

/// **Com o portão dormente, habilitar um MOD não tranca a porta de um servidor
/// que funcionava.**
///
/// O guarda de uma regressão de verdade: na primeira versão daquela entrega, uma
/// linha habilitada passou a recusar toda entrada com `Incompatible` — e sem
/// dar a ninguém a chance de aceitar, porque o anúncio não tinha como sair. Um
/// portão que ninguém pode atravessar não protege, só recusa.
///
/// O outro mundo — o portão de pé, que é o do servidor padrão — está em
/// `um_servidor_com_mod_devolve_a_lista_a_quem_nao_aceitou`.
#[tokio::test(flavor = "multi_thread")]
async fn com_o_portao_dormente_habilitar_um_mod_nao_fecha_a_casa() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor_com_o_portao_dormente().await?;
    habilitar(&daemon, &um_mod("seele/bot", 0xa1)).await;

    entrar(endereco, 21)
        .await
        .expect("um MOD habilitado trancou um servidor que ninguém consegue sequer perguntar");

    daemon.shutdown();
    Ok(())
}

/// **E não derruba quem está dentro.**
///
/// A metade simétrica, e ela precisa ser dita à parte: derrubar uma sessão que
/// nunca foi perguntada é tirar da sala alguém que entraria de volta no segundo
/// seguinte, sem nunca ter lido lista nenhuma. Reconectar por reconectar.
#[tokio::test(flavor = "multi_thread")]
async fn com_o_portao_dormente_habilitar_um_mod_nao_derruba_a_sala() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor_com_o_portao_dormente().await?;
    let mut par = abrir(endereco, 22).await?;

    habilitar(&daemon, &um_mod("seele/cor", 0xb2)).await;

    assert_eq!(
        esperar_despedida(&mut par, Duration::from_secs(2)).await,
        None,
        "a sala foi derrubada por uma exigência que ainda não vale no fio"
    );

    daemon.shutdown();
    Ok(())
}

// ------------------------------------------------------------- o par cru

#[derive(Debug)]
struct AceitaQualquer(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AceitaQualquer {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Uma conexão com o fluxo de controle na mão, sem tarefa leitora.
struct Par {
    /// Mantém a conexão viva, e responde **como** ela fechou. Ver
    /// [`motivo_do_fechamento`].
    _conexao: quinn::Connection,
    _envio: quinn::SendStream,
    recebe: quinn::RecvStream,
}

/// Espera uma despedida, deixando passar o que não é uma.
///
/// O servidor escreve outros quadros logo depois do `Session` — o `HostUplink`,
/// por exemplo —, e um teste que lesse só o primeiro estaria perguntando «qual
/// foi o próximo quadro» quando a pergunta é «esta sessão acabou?». Foi o que
/// este teste fez na primeira versão, e ele reprovou por isso.
async fn esperar_despedida(par: &mut Par, prazo: Duration) -> Option<DisconnectReason> {
    let ate = tokio::time::Instant::now() + prazo;
    loop {
        let resta = ate.saturating_duration_since(tokio::time::Instant::now());
        if resta.is_zero() {
            return None;
        }
        match tokio::time::timeout(resta, frame::read::<ServerMessage>(&mut par.recebe)).await {
            Ok(Ok(ServerMessage::Disconnecting { reason })) => return Some(reason),
            // Qualquer outro quadro é a sessão viva, e é o que este laço deixa
            // passar. Um fluxo que acaba sem despedida também não é uma.
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

/// Por que a conexão fechou, como o QUIC a fechou.
///
/// **A prova de que a despedida foi entregue, e não só escrita.** Quem encerra
/// uma sessão com motivo chama `despedir`, que espera o outro lado reconhecer o
/// fim do fluxo e só então fecha a conexão **com uma razão de aplicação**.
/// Quem apenas escreve e sai deixa a `Connection` ser recolhida, e aí o QUIC
/// fecha com razão vazia — levando junto o quadro que ainda não tinha saído.
///
/// Ler a razão é o que torna este guarda determinístico: o quadro em si escapa
/// na maioria das execuções mesmo sem `despedir`, e um teste que só olhasse
/// para ele reprovaria numa em cada doze.
async fn motivo_do_fechamento(par: &Par, prazo: Duration) -> Option<Vec<u8>> {
    match tokio::time::timeout(prazo, par._conexao.closed()).await {
        Ok(quinn::ConnectionError::ApplicationClosed(fim)) => Some(fim.reason.to_vec()),
        _ => None,
    }
}

/// Faz o aperto de mão à mão, para que a despedida possa ser lida no fio.
async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    abrir_falando(endereco, semente, seele_proto::PROTOCOL_VERSION).await
}

/// O quadro que veio no lugar de `Session`, guardado em vez de resumido.
///
/// # Por que isto não é um `bail!` com texto
///
/// Um teste que só afirme `is_err()` passa igual se o aperto de mão quebrar por
/// outro motivo — outra recusa, um erro de transporte, um tempo esgotado. Foi
/// esse o buraco apontado na revisão desta entrega. Carregando o quadro inteiro,
/// quem afirma pode cobrar **qual** recusa veio, e uma regressão que troque o
/// motivo reprova em vez de passar verde.
#[derive(Debug)]
struct NaoEntrou(ServerMessage);

impl std::fmt::Display for NaoEntrou {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "em vez de `Session` o servidor mandou {:?}", self.0)
    }
}

impl std::error::Error for NaoEntrou {}

/// O mesmo, declarando no `Hello` a versão que se quiser.
///
/// **É a única forma de um par mais velho aparecer neste crate**, e é uma
/// simulação parcial — dizê-lo é metade do valor deste comentário. As duas
/// pontas são a mesma build: o quadro sai carimbado com `PROTOCOL_VERSION` por
/// `control::encode`, e volta a ser lido por este mesmo codec. Só o campo
/// `Hello.version` fala a versão pedida.
///
/// Isto mede o lado do **servidor** — a janela de compatibilidade e o limiar do
/// anúncio, que decidem sobre o número declarado. Não mede o lado do cliente
/// velho, que recusaria o carimbo antes de ler o corpo: essa metade está em
/// `seele_proto::control::a_release_publicada_recusa_o_carimbo_desta_build`, e é
/// por isso que nenhum teste deste arquivo promete que a release publicada
/// conecta.
async fn abrir_falando(endereco: SocketAddr, semente: u8, versao: u8) -> Result<Par> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    )));
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    // A ponta é dona do socket; sem isto ela cai no fim desta função.
    std::mem::forget(ponta);

    let (mut envio, mut recebe) = conexao.open_bi().await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            version: versao,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
        },
    )
    .await?;

    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave.sign(&nonce).to_bytes().to_vec(),
        },
    )
    .await?;
    let ultimo = frame::read::<ServerMessage>(&mut recebe).await?;
    let ServerMessage::Session { .. } = ultimo else {
        return Err(NaoEntrou(ultimo).into());
    };

    Ok(Par {
        _conexao: conexao,
        _envio: envio,
        recebe,
    })
}

// ------------------------------------------------- o cliente diante do anúncio
//
// **Estes não são a prova da costura** — ela está lá em cima, com o `seeled` de
// verdade do outro lado. O que um servidor de mentira acrescenta é o que um de
// verdade não faz: anunciar coisas que o `seeled` nunca anunciaria, e observar
// **qual quadro** o cliente escreveu de volta em vez de deduzi-lo do resultado.
//
// Vale a redundância pelo que ela isola: quando um destes falha e os de cima
// passam, o defeito é do cliente; quando é ao contrário, é da costura.

/// O que o servidor de mentira fez com a resposta ao anúncio.
#[derive(Debug, PartialEq, Eq)]
enum Resposta {
    Aceitou(String),
    Recusou,
    Outra,
}

/// Um servidor que só sabe anunciar MODs.
///
/// Faz o aperto de mão de verdade — `Challenge`, `Response` —, anuncia o
/// conjunto, e conta o que voltou. Entrega `Session` só a quem aceitou, que é
/// exatamente o contrato que o servidor de verdade cumpre.
/// O que o servidor de mentira fez, para quando o cliente reprovar.
///
/// # Por que isto existe
///
/// Estes testes afirmam sobre o **cliente** e só depois recolhem a tarefa do
/// servidor. Quando o cliente reprova, portanto, a mensagem descreve o sintoma
/// — «veio `SemResposta`» — e o que o outro lado fez morre com a tarefa nunca
/// recolhida. Foi exatamente o que aconteceu numa falha intermitente observada
/// uma vez em cerca de cento e setenta execuções, e o que impediu de
/// diagnosticá-la: o teste sabia e não contava.
///
/// O prazo é curto porque a tarefa termina 200 ms depois de ler a resposta; se
/// ela ainda estiver presa, isso **é** a notícia — quer dizer que o servidor de
/// mentira parou antes de chegar onde o cliente esperava.
async fn o_que_o_servidor_fez(servidor: tokio::task::JoinHandle<Result<Resposta>>) -> String {
    match tokio::time::timeout(Duration::from_secs(2), servidor).await {
        Ok(Ok(Ok(resposta))) => format!("o servidor leu {resposta:?}"),
        Ok(Ok(Err(erro))) => format!("o servidor parou com erro: {erro}"),
        Ok(Err(erro)) => format!("a tarefa do servidor morreu: {erro}"),
        Err(_) => "o servidor ainda estava esperando; ele não chegou onde o teste supõe".to_owned(),
    }
}

async fn servidor_que_anuncia(
    mods: Vec<seele_proto::mods::ModAnunciado>,
) -> Result<(SocketAddr, tokio::task::JoinHandle<Result<Resposta>>)> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let identidade = seele_server::tls::Identity::self_signed(vec!["localhost".to_owned()])?;
    let ponta = quinn::Endpoint::server(
        seele_server::tls::server_config(identidade)?,
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )?;
    let endereco = ponta.local_addr()?;

    let tarefa = tokio::spawn(async move {
        let conexao = ponta
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("ninguém bateu"))?
            .await?;
        let (mut envia, mut recebe) = conexao.accept_bi().await?;

        let ClientMessage::Hello { .. } = frame::read::<ClientMessage>(&mut recebe).await? else {
            anyhow::bail!("o primeiro quadro não foi Hello");
        };
        let nonce = vec![7_u8; 32];
        frame::write(
            &mut envia,
            &ServerMessage::Challenge {
                nonce: nonce.clone(),
            },
        )
        .await?;
        let ClientMessage::Response { .. } = frame::read::<ClientMessage>(&mut recebe).await?
        else {
            anyhow::bail!("o segundo quadro não foi Response");
        };

        let mut mods = mods;
        let conjunto =
            seele_proto::mods::hex(&seele_proto::mods::identidade_do_conjunto(&mut mods));
        frame::write(
            &mut envia,
            &ServerMessage::ModsExigidos {
                mods,
                conjunto: conjunto.clone(),
            },
        )
        .await?;

        let resposta = match frame::read::<ClientMessage>(&mut recebe).await? {
            ClientMessage::AceitarMods { conjunto } => Resposta::Aceitou(conjunto),
            ClientMessage::RecusarMods => Resposta::Recusou,
            _ => Resposta::Outra,
        };

        if let Resposta::Aceitou(aceito) = &resposta {
            if aceito == &conjunto {
                frame::write(
                    &mut envia,
                    &ServerMessage::Session {
                        id: seele_proto::ids::SessionId(1),
                        person: seele_proto::ids::PersonId(1),
                        ssrc: seele_proto::ids::Ssrc(1),
                        server: "Casa".into(),
                        voice_rooms: Vec::new(),
                        channels: Vec::new(),
                        roles: Vec::new(),
                        permissions: Vec::new(),
                    },
                )
                .await?;
            }
        }
        // O fluxo fica aberto até o cliente terminar de ler.
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(resposta)
    });

    Ok((endereco, tarefa))
}

fn anunciado(id: &str, hash_de: u8) -> seele_proto::mods::ModAnunciado {
    seele_proto::mods::ModAnunciado {
        id: id.to_owned(),
        version: "1.0.0".to_owned(),
        hash: format!("{hash_de:02x}").repeat(32),
        repo: "https://github.com/seele/exemplo".to_owned(),
        reach: vec!["ler".to_owned()],
        no_servidor: true,
    }
}

/// **Sem aceite guardado, o cliente recusa — e devolve a pergunta.**
///
/// O erro não é uma queixa: é a tela de aceite com o que ela precisa mostrar.
/// Repositório, alcance declarado, e a linha que diz que este MOD roda na
/// máquina de quem hospeda.
#[tokio::test(flavor = "multi_thread")]
async fn sem_aceite_guardado_o_cliente_recusa_e_devolve_a_lista() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor_que_anuncia(vec![anunciado("seele/bot", 0xa1)]).await?;

    let resultado = entrar(endereco, 5).await;
    let Err(erro) = resultado else {
        panic!("o cliente entrou sem ter aceitado nada");
    };
    let ConnectError::ModsNaoAceitos { mods, conjunto } = erro else {
        let la = o_que_o_servidor_fez(servidor).await;
        panic!("o cliente não devolveu a pergunta: {erro:?}; {la}");
    };
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].id, "seele/bot");
    assert_eq!(mods[0].repo, "https://github.com/seele/exemplo");
    assert!(
        mods[0].no_servidor,
        "a tela de aceite não teria como dizer que este MOD roda na máquina de \
         quem hospeda"
    );
    assert!(!conjunto.is_empty());

    // E a recusa foi **dita**, e não deduzida de um fluxo que morreu.
    assert_eq!(servidor.await??, Resposta::Recusou);
    Ok(())
}

/// **Com o aceite guardado, entra direto.** ADR 0045: «aceitou uma vez, entra
/// direto nas próximas.»
#[tokio::test(flavor = "multi_thread")]
async fn com_o_aceite_guardado_o_cliente_entra_direto() -> Result<()> {
    let _vaga = vaga::minha();
    let mut mods = vec![anunciado("seele/bot", 0xa1)];
    let conjunto = seele_proto::mods::hex(&seele_proto::mods::identidade_do_conjunto(&mut mods));
    let (endereco, servidor) = servidor_que_anuncia(mods).await?;

    let entrada = Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "pessoa006",
        &SigningKey::from_bytes(&[6; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        Some(&conjunto),
    )
    .await;
    if let Err(erro) = entrada {
        let la = o_que_o_servidor_fez(servidor).await;
        panic!("com o aceite guardado a entrada tem de passar, e veio {erro:?}; {la}");
    }

    assert_eq!(servidor.await??, Resposta::Aceitou(conjunto));
    Ok(())
}

/// **O aceite de outro conjunto não é reaproveitado.** É o caso de quem aceitou
/// ontem e o servidor trocou de MOD: o cliente percebe sozinho, recusa, e
/// devolve a lista nova para a tela.
#[tokio::test(flavor = "multi_thread")]
async fn um_aceite_guardado_de_outro_conjunto_nao_e_reaproveitado() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor_que_anuncia(vec![anunciado("seele/bot", 0xa1)]).await?;

    let resultado = Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "pessoa007",
        &SigningKey::from_bytes(&[7; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        // O sim de ontem, para um conjunto que este servidor não exige mais.
        Some(&"fe".repeat(32)),
    )
    .await;
    let Err(erro) = resultado else {
        panic!("um aceite de outro conjunto abriu a porta");
    };

    if !matches!(erro, ConnectError::ModsNaoAceitos { .. }) {
        let la = o_que_o_servidor_fez(servidor).await;
        panic!("o cliente reaproveitou um aceite inválido: {erro:?}; {la}");
    }
    assert_eq!(servidor.await??, Resposta::Recusou);
    Ok(())
}

// ------------------------------------- a integração conjunta, no fio
//
// O contrato que `seele_proto::mods::VERSAO_DO_ANUNCIO` carregava desde que
// nasceu: quando a malha e o anúncio se juntassem, `PROTOCOL_VERSION` subiria
// para 5 **uma vez**, a constante do anúncio continuaria em 5, e o anúncio
// passaria a sair sozinho. Estes três testes são a cobrança dele contra bytes de
// verdade — a constante já está provada em `seele-proto`, e uma constante não é
// um comportamento.

/// **O anúncio sai de um servidor que ninguém configurou.**
///
/// A diferença para `um_servidor_com_mod_devolve_a_lista_a_quem_nao_aceitou`
/// é o que está sendo afirmado. Lá, o assunto é a lista: quais campos a tela de
/// aceite recebe. Aqui é a **dormência**: que o limiar padrão passou a ser
/// alcançável e que o portão liga sem ninguém mexer em nada.
///
/// Enquanto o anúncio dormia, este teste era impossível de escrever — a mesma
/// entrada, no mesmo servidor padrão, devolvia uma sessão. É por isso que ele
/// vale como prova do contrato cumprido, e não como repetição do de cima.
#[tokio::test(flavor = "multi_thread")]
async fn o_anuncio_sai_do_servidor_padrao_sem_ninguem_baixar_limiar() -> Result<()> {
    let _vaga = vaga::minha();
    assert!(
        ServerConfig::default().versao_do_anuncio <= seele_proto::PROTOCOL_VERSION,
        "o limiar padrão voltou a ficar acima da versão global, e o portão \
         voltou a ser dormente para todo servidor"
    );

    let (endereco, daemon) = servidor().await?;

    // Sem MOD nenhum, a porta é a de sempre. É o controle: sem ele, um servidor
    // que recusasse tudo passaria na metade de baixo deste teste.
    entrar(endereco, 51)
        .await
        .expect("um servidor sem MOD deixou de deixar entrar");

    habilitar(&daemon, &um_mod("seele/bot", 0xa1)).await;

    let Err(erro) = entrar(endereco, 52).await else {
        panic!(
            "o anúncio continua dormente: um MOD habilitado num servidor padrão \
             não perguntou nada a quem entrou"
        );
    };
    assert!(
        matches!(erro, ConnectError::ModsNaoAceitos { .. }),
        "o servidor padrão não devolveu a pergunta, devolveu {erro:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// **Um par da versão anterior continua entrando, e é só isso que a janela
/// promete.**
///
/// A janela de compatibilidade é N−1: a v5 desta build ouve a v4. Este teste
/// prende esse lado — o `Hello` sai com o número anterior de verdade e o aperto
/// de mão inteiro corre até `Session`, sem baixar constante nenhuma.
///
/// **O que ele deliberadamente não promete:** que a release publicada
/// `v0.10.5-1`, que fala protocolo 3, continue entrando. Ela não continua, e a
/// janela não tem como fazê-la continuar — o quadro do servidor sai carimbado
/// com a versão global e aquele build recusa o carimbo antes de ler o corpo.
/// Medido em `seele_proto::control::a_release_publicada_recusa_o_carimbo_desta_build`
/// e contado por inteiro na pendência #42.
#[tokio::test(flavor = "multi_thread")]
async fn um_par_da_versao_anterior_continua_entrando() -> Result<()> {
    let _vaga = vaga::minha();
    let anterior = seele_proto::version::oldest_supported_version();
    assert!(
        anterior < seele_proto::PROTOCOL_VERSION,
        "a janela fechou sobre a própria versão: não há vocabulário anterior vivo"
    );

    let (endereco, daemon) = servidor().await?;

    // `abrir` só devolve `Par` depois de ler o `Session`, então chegar aqui é a
    // prova: o aperto de mão inteiro correu com o número anterior.
    let _par = abrir_falando(endereco, 53, anterior)
        .await
        .map_err(|erro| anyhow::anyhow!("um par da versão anterior não entrou: {erro}"))?;

    daemon.shutdown();
    Ok(())
}

/// Na v6, a janela N−1 já alcança o anúncio v5. O servidor deve anunciar
/// antes de Session, sem dispensar o aceite do par anterior. A recusa por
/// estar abaixo do limiar continua coberta na unidade do servidor com v4.
#[tokio::test(flavor = "multi_thread")]
async fn um_par_dentro_da_janela_recebe_o_anuncio_antes_de_entrar() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    habilitar(&daemon, &um_mod("seele/bot", 0xa1)).await;

    let anterior = seele_proto::version::oldest_supported_version();
    assert!(
        anterior >= seele_proto::mods::VERSAO_DO_ANUNCIO,
        "a janela atual deve alcançar o anúncio original"
    );
    let erro = abrir_falando(endereco, 54, anterior)
        .await
        .err()
        .expect("o servidor entregou Session sem primeiro exigir o aceite dos MODs");

    // O helper esperava Session; o que veio precisa ser o anúncio, não uma
    // desconexão ou falha de transporte mascarada como sucesso do teste.
    let recusa = erro
        .downcast_ref::<NaoEntrou>()
        .unwrap_or_else(|| panic!("a conexão caiu sem o servidor dizer por quê: {erro:#}"));
    assert!(
        matches!(&recusa.0, ServerMessage::ModsExigidos { mods, .. } if mods.len() == 1 && mods[0].id == "seele/bot"),
        "a v5 recebe o anúncio e precisa aceitá-lo antes de Session"
    );

    daemon.shutdown();
    Ok(())
}
