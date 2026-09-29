//! Quem volta pela lista de servidores confere pela impressão guardada.
//!
//! O terceiro defeito do link (§2.1 da análise de 22/09) e o S3 (§3.1) são o
//! mesmo corte. A impressão esperada só vinha de um link colado nesta sessão,
//! e quem voltava pela lista de conhecidos não tinha link nenhum: ele foi lido
//! uma vez, num processo que já fechou. Sem ela:
//!
//! - o endereço novo que o quarto devolvia era fixado às cegas, com a faixa
//!   PRIMEIRO CONTATO, e um impostor que ocupasse a marca no quarto era aceito
//!   e gravado na lista;
//! - `Batida::preparar` não tinha de onde tirar a marca do `LEVE`, e quem estava
//!   atrás de NAT não era alcançado na volta.
//!
//! # O que estes testes chamam, e o que eles não alcançam
//!
//! O `connect` do app é um comando Tauri e não roda sem casca. A decisão que
//! ele toma mora em `seele_ffi::impressao_a_conferir`, e é ela que estes testes
//! chamam. A impressão vem de uma lista de conhecidos de verdade, em disco, e
//! a configuração vai para a mesma `Connection::connect` que o app chama. Que
//! o comando use a função, para a conferência e para a pergunta ao quarto, é
//! guardado por texto-fonte em `apps/seele-app/tests/frontend.rs`
//! (`a_volta_pela_lista_confere_pela_impressao_guardada`).
//!
//! O que a lista **guarda** depois de entrar segue a mesma divisão. A regra é
//! `seele_ffi::impressao_a_guardar`, e o último teste deste arquivo a exercita
//! com um servidor de verdade que fecha e volta noutra porta. O uso pelo
//! comando, com a impressão que a conexão conferiu, é guardado em
//! `a_lista_guarda_a_impressao_que_a_conexao_aceitou`.
//!
//! # Por que `[::ffff:127.0.0.1]`
//!
//! Pelo mesmo motivo de `estados.rs` e `furo.rs`. `enlace::e_publico` pergunta
//! por loopback na forma escrita, então a forma mapeada conta como pública e o
//! `LEVE` sai por ela, mas o pacote ainda chega a um socket desta máquina.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use seele_ffi::conhecidos::Conhecidos;
use seele_ffi::uri::Bilhete;
use seele_ffi::{ConnectConfig, Connection, ConnectionError, Trust};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// Sobe um servidor de verdade numa porta que o sistema escolhe.
///
/// Banco em memória: cada um nasce com uma identidade nova, e é disso que o
/// segundo teste precisa para ter um impostor.
async fn server_de_teste() -> Option<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.ok()?);
    let endereco = servidor.local_addr().ok()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Some((endereco, servidor))
}

/// Um ponto de encontro que só conta quantos avisos chegaram.
///
/// A mesma forma do de `estados.rs`. A batida não lê resposta do ponto de
/// encontro (o cabeçalho de `seele_core::encontro` diz o que este lado lê e o
/// que não lê), então um contador basta para provar que o `LEVE` saiu.
async fn ponto_que_conta() -> Option<(SocketAddr, Arc<AtomicUsize>)> {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok()?;
    let onde = socket.local_addr().ok()?;
    let quantos = Arc::new(AtomicUsize::new(0));
    let contador = Arc::clone(&quantos);
    tokio::spawn(async move {
        let mut balde = [0_u8; 96];
        while socket.recv_from(&mut balde).await.is_ok() {
            contador.fetch_add(1, Ordering::Relaxed);
        }
    });
    Some((onde, quantos))
}

/// A porta de ontem: aberta, lida e devolvida, para ninguém atender nela.
fn endereco_morto() -> SocketAddr {
    let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") else {
        panic!("esta máquina não deixou abrir um socket em 127.0.0.1");
    };
    let Ok(endereco) = socket.local_addr() else {
        panic!("um socket aberto sem endereço local");
    };
    drop(socket);
    endereco
}

/// O endereço na forma que conta como pública e ainda chega a esta máquina.
fn mapeado(porta: u16) -> String {
    format!("[::ffff:127.0.0.1]:{porta}")
}

/// Escreve a lista de conhecidos como a primeira visita pelo link a deixou, e
/// devolve a impressão **relida do disco**.
///
/// É o que o `connect` do app grava depois de entrar por um link, com
/// `registrar` e depois `anotar_caminhos`, pela mesma `Conhecidos`. A impressão
/// é relida porque o valor que a volta pela lista tem na mão é o que sobreviveu
/// ao arquivo, e não o que foi escrito.
fn lembrar_de_ontem(
    casa: &std::path::Path,
    ontem: &str,
    bilhete: Option<&Bilhete>,
    impressao: &str,
) -> Option<String> {
    let caminho = casa.join("conhecidos");
    let mut lista = Conhecidos::abrir(caminho.clone()).ok()?;
    lista.registrar(ontem, "pessoa", None).ok()?;
    let bilhete = bilhete.map(ToString::to_string);
    lista
        .anotar_caminhos(ontem, &[], bilhete.as_deref(), Some(impressao))
        .ok()?;
    let relida = Conhecidos::abrir(caminho).ok()?;
    relida.buscar(ontem)?.impressao.clone()
}

/// A configuração de uma entrada, com o que o teste decide e o resto fixo.
///
/// `esperada` é o que o app passaria a `expected_fingerprint`: o teste a obtém
/// de `seele_ffi::impressao_a_conferir`, a regra do app, e não de um valor
/// escrito à mão.
fn config(
    casa: &std::path::Path,
    alvo: String,
    alternativos: Vec<String>,
    esperada: Option<String>,
    bilhete: Option<Bilhete>,
) -> ConnectConfig {
    ConnectConfig {
        server: alvo,
        alternate_servers: alternativos,
        nickname: "pessoa".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: esperada,
        bilhete,
        // Não há placa de som numa máquina de integração contínua.
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

/// Conecta pela mesma porta que o app usa, numa thread que pode bloquear.
async fn conectar(config: ConnectConfig) -> Result<(Arc<Connection>, Trust), ConnectionError> {
    let Ok(resultado) = tokio::task::spawn_blocking(move || Connection::connect(config)).await
    else {
        panic!("a thread que conecta caiu");
    };
    resultado
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_volta_pela_lista_a_um_servidor_que_mudou_de_porta_confere_e_avisa() {
    let _vaga = vaga::minha();
    let Some((hoje, servidor)) = server_de_teste().await else {
        panic!("o servidor de teste não subiu");
    };
    let Some((ponto, avisos)) = ponto_que_conta().await else {
        panic!("o ponto de encontro de teste não subiu");
    };
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há lista nem identidade");
    };
    let Ok(bilhete) = Bilhete::novo(ponto.to_string(), "45.33.32.156:41234") else {
        panic!("o bilhete de teste não se monta");
    };
    let real = servidor.fingerprint().to_owned();

    // Ontem ele morava numa porta onde hoje não há ninguém: é o servidor que
    // fechou e abriu de novo atrás de um NAT que lhe deu outra porta.
    let ontem = mapeado(endereco_morto().port());
    let Some(guardada) = lembrar_de_ontem(casa.path(), &ontem, Some(&bilhete), &real) else {
        panic!("a lista de conhecidos não devolveu a impressão da primeira visita");
    };

    // O alvo é o de ontem, que é o que a pessoa clicou. O de hoje vai entre os
    // alternativos, que é onde o `connect` do app põe a resposta do quarto. Sem
    // link nesta sessão, só a guardada.
    let configuracao = config(
        casa.path(),
        ontem,
        vec![mapeado(hoje.port())],
        seele_ffi::impressao_a_conferir(None, Some(&guardada)),
        Some(bilhete),
    );

    let (connection, confianca) = match conectar(configuracao).await {
        Ok(entrada) => entrada,
        Err(erro) => panic!("a volta pela lista não entrou no servidor de verdade: {erro:?}"),
    };

    assert_eq!(
        confianca,
        Trust::FirstContactVerified { fingerprint: real },
        "quem voltou pela lista entrou num endereço novo sem conferir nada — o \
         PRIMEIRO CONTATO cego do S3, em que um impostor no quarto seria aceito"
    );
    assert!(
        avisos.load(Ordering::Relaxed) >= 1,
        "nenhum `LEVE` chegou ao ponto de encontro: sem impressão esperada, \
         `Batida::preparar` não tem de onde tirar a marca, e quem está atrás de \
         NAT não é alcançado na volta"
    );

    connection.disconnect();
    servidor.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn o_impostor_no_endereco_novo_e_recusado_pela_impressao_guardada() {
    let _vaga = vaga::minha();
    // O S3 numa máquina só. O servidor de verdade é o da lista, e quem atende
    // no endereço que o quarto devolveu é outro, com outra chave. Bastava o
    // anfitrião ficar fora do quarto por mais de 60 s para a marca ser dele.
    let Some((_, verdadeiro)) = server_de_teste().await else {
        panic!("o servidor de verdade não subiu");
    };
    let Some((impostor_em, impostor)) = server_de_teste().await else {
        panic!("o impostor não subiu");
    };
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há lista nem identidade");
    };
    let real = verdadeiro.fingerprint().to_owned();
    let ontem = mapeado(endereco_morto().port());
    let Some(guardada) = lembrar_de_ontem(casa.path(), &ontem, None, &real) else {
        panic!("a lista de conhecidos não devolveu a impressão da primeira visita");
    };

    // Só o impostor na corrida. O endereço de ontem não responde e somaria os
    // quatro segundos do prazo dele sem mudar o que se mede, que é o que
    // acontece com quem **responde** com a chave errada.
    let configuracao = config(
        casa.path(),
        mapeado(impostor_em.port()),
        Vec::new(),
        seele_ffi::impressao_a_conferir(None, Some(&guardada)),
        None,
    );

    let resultado = conectar(configuracao).await.map(|(connection, confianca)| {
        connection.disconnect();
        confianca
    });

    assert_eq!(
        resultado,
        Err(ConnectionError::InviteMismatch {
            expected: real,
            offered: impostor.fingerprint().to_owned(),
        }),
        "o impostor que o quarto apontou foi aceito, ou recusado com outro nome"
    );

    verdadeiro.shutdown();
    impostor.shutdown();
}

/// A impressão de um link que aponta para outro servidor: a forma certa, e
/// dona nenhuma.
const DE_OUTRO_SERVIDOR: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Sobe um servidor com banco em arquivo, numa porta que o sistema escolhe.
///
/// Em arquivo, e não em memória: o servidor que sobe de novo com o mesmo banco
/// é o **mesmo** servidor, com a mesma chave, noutra porta. É o anfitrião que
/// fechou e abriu atrás de um NAT que lhe deu outro mapeamento.
async fn server_com_banco(banco: &Path) -> Option<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco.to_path_buf()),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.ok()?);
    let endereco = servidor.local_addr().ok()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Some((endereco, servidor))
}

/// A configuração de uma visita por um link colado nesta sessão.
fn config_do_link(casa: &Path, alvo: String, do_link: &str) -> ConnectConfig {
    // A regra do app, com o link desta sessão e nada guardado.
    let esperada = seele_ffi::impressao_a_conferir(Some(do_link), None);
    config(casa, alvo, Vec::new(), esperada, None)
}

/// Grava a lista como o `connect` do app grava depois de entrar.
///
/// `registrar`, e depois `anotar_caminhos` com a impressão que
/// `seele_ffi::impressao_a_guardar` decide a partir do veredito e da impressão
/// que a visita conferiu. Nestas visitas ela é a do link colado nesta sessão,
/// porque não há guardada a conferir antes.
fn anotar_como_o_app(casa: &Path, alvo: &str, veredito: &Trust, conferida: Option<&str>) {
    let Ok(mut lista) = Conhecidos::abrir(casa.join("conhecidos")) else {
        panic!("a lista de conhecidos não abriu");
    };
    let aceita = seele_ffi::impressao_a_guardar(veredito, conferida);
    if lista.registrar(alvo, "pessoa", None).is_err() {
        panic!("a lista de conhecidos não registrou a visita");
    }
    if lista
        .anotar_caminhos(alvo, &[], None, aceita.as_deref())
        .is_err()
    {
        panic!("a lista de conhecidos não anotou a impressão");
    }
}

/// Espera o servidor terminar de fechar, para o banco e o socket ficarem livres
/// para o que sobe em seguida.
///
/// **Só depois de `shutdown`.** `wait_idle` volta quando o endpoint não tem
/// conexão nenhuma, e `Connection::disconnect` não a faz voltar logo: medido,
/// 20,0 s (o `IDLE_TIMEOUT`) depois de cada visita, duas vezes seguidas. O
/// `shutdown` fecha o que estiver de pé e o `wait_idle` volta em dezenas de
/// milissegundos, como em `bateria_interna.rs`.
async fn esperar_o_servidor_fechar(servidor: &Daemon) {
    if tokio::time::timeout(Duration::from_secs(10), servidor.wait_idle())
        .await
        .is_err()
    {
        panic!("o servidor continuou de pé dez segundos depois de `shutdown`");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn um_link_de_outro_servidor_nao_envenena_a_volta_pela_lista() {
    let _vaga = vaga::minha();
    // O defeito: o `connect` do app gravava na lista a impressão do link
    // qualquer que fosse o veredito. Um link de outro servidor, colado para um
    // endereço já fixado, entra e avisa (ADR 0003), e a lista ficava com a
    // chave do outro. A volta pela lista confere pela guardada, e o servidor
    // verdadeiro era recusado no primeiro endereço sem pin: a porta nova do
    // NAT, ou o endereço que o quarto devolve.
    let Ok(pasta) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há banco");
    };
    let banco = pasta.path().join("seele.db");
    let Some((ontem, servidor)) = server_com_banco(&banco).await else {
        panic!("o servidor de teste não subiu");
    };
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há lista nem identidade");
    };
    let real = servidor.fingerprint().to_owned();
    let alvo = ontem.to_string();

    // A primeira visita, pelo link certo: a chave fica fixada neste endereço.
    let (connection, veredito) =
        match conectar(config_do_link(casa.path(), alvo.clone(), &real)).await {
            Ok(entrada) => entrada,
            Err(erro) => panic!("a primeira visita, pelo link certo, não entrou: {erro:?}"),
        };
    assert_eq!(
        veredito,
        Trust::FirstContactVerified {
            fingerprint: real.clone()
        },
        "a primeira visita não conferiu a impressão, e o resto do teste perde o assunto"
    );
    anotar_como_o_app(casa.path(), &alvo, &veredito, Some(&real));
    connection.disconnect();

    // Um link de outro servidor, para o mesmo endereço. O pin confere e o link
    // discorda: a conexão fica de pé e avisa.
    let (connection, veredito) =
        match conectar(config_do_link(casa.path(), alvo.clone(), DE_OUTRO_SERVIDOR)).await {
            Ok(entrada) => entrada,
            Err(erro) => panic!(
                "o link que discorda do pin derrubou a conexão com um servidor já \
                 conhecido, contra o ADR 0003: {erro:?}"
            ),
        };
    assert_eq!(
        veredito,
        Trust::InviteDisagrees {
            expected: DE_OUTRO_SERVIDOR.to_owned(),
            offered: real.clone(),
        },
        "o link que discorda do pin não deu `InviteDisagrees`, e o teste não mede o \
         que diz medir"
    );
    anotar_como_o_app(casa.path(), &alvo, &veredito, Some(DE_OUTRO_SERVIDOR));
    connection.disconnect();

    // O servidor fecha e abre de novo com o mesmo banco: a mesma chave, noutra
    // porta.
    servidor.shutdown();
    esperar_o_servidor_fechar(&servidor).await;
    drop(servidor);
    let Some((hoje, de_novo)) = server_com_banco(&banco).await else {
        panic!("o servidor não subiu de novo com o mesmo banco");
    };
    assert_eq!(
        de_novo.fingerprint(),
        real,
        "o banco em arquivo não guardou a chave, e o servidor de hoje é outro"
    );
    assert_ne!(
        hoje.port(),
        ontem.port(),
        "o sistema devolveu a mesma porta, e ela tem pin: este teste precisa de um \
         endereço sem pin para dizer alguma coisa. Rode de novo"
    );

    // A volta pela lista: sem link nesta sessão, com a impressão relida do
    // disco, e o endereço de hoje na frente, onde o `connect` do app põe a
    // resposta do quarto.
    let Some(guardada) = Conhecidos::abrir(casa.path().join("conhecidos"))
        .ok()
        .and_then(|lista| {
            lista
                .buscar(&alvo)
                .and_then(|conhecido| conhecido.impressao.clone())
        })
    else {
        panic!("a lista de conhecidos perdeu a impressão das duas visitas");
    };
    let configuracao = config(
        casa.path(),
        alvo,
        vec![hoje.to_string()],
        seele_ffi::impressao_a_conferir(None, Some(&guardada)),
        None,
    );
    let (connection, confianca) = match conectar(configuracao).await {
        Ok(entrada) => entrada,
        Err(erro) => panic!(
            "a volta pela lista recusou o servidor verdadeiro no endereço novo ({erro:?}): \
             a lista guardou a impressão do link que discordava do pin, e não a que a \
             conexão aceitou"
        ),
    };
    assert_eq!(
        confianca,
        Trust::FirstContactVerified { fingerprint: real },
        "a volta pela lista entrou no endereço novo sem conferir pela impressão guardada"
    );

    connection.disconnect();
    de_novo.shutdown();
}
