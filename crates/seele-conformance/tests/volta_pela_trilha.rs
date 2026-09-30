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
//! `seele_ffi::impressao_a_guardar`, exercitada aqui com servidores de verdade:
//! um que fecha e volta noutra porta, e um alvo já fixado cuja primeira volta
//! pela lista cura a impressão que a 0.15.0 gravou errada. Esse alvo é
//! `127.0.0.1`: o loopback é esta máquina em qualquer rede, e ali o pino prova
//! o servidor como num alvo público (`seele_ffi` monta o destino assim). Num
//! alvo de LAN o pino não prova o servidor, e a mesma volta é recusada dentro
//! do TLS; isso se prova no destino que a FFI monta e no verificador, sem
//! servidor (nenhum teste tem um endereço privado garantido). O uso pelo
//! comando, com o link desta sessão e a guardada como entradas, é guardado em
//! `a_lista_guarda_a_impressao_que_a_conexao_aceitou`.
//!
//! Os dois últimos testes põem na corrida um candidato de **outro** servidor,
//! já fixado nesta máquina num dos caminhos da lista. Fora do alvo, a
//! impressão guardada vale mais que o pino, e ele é recusado dentro do TLS: o
//! que se observa é o lado dele, sem conexão, com o convite inteiro e sem
//! batida na portaria.
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

use seele_core::{FilePinStore, PinStore};
use seele_ffi::conhecidos::Conhecidos;
use seele_ffi::uri::Bilhete;
use seele_ffi::{ConnectConfig, Connection, ConnectionError, Trust};
use seele_server::persistence::{Location, Persistence};
use seele_server::{admissao, Daemon, ServerConfig};

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
    caminhos: &[String],
    bilhete: Option<&Bilhete>,
    impressao: &str,
) -> Option<String> {
    let caminho = casa.join("conhecidos");
    let mut lista = Conhecidos::abrir(caminho.clone()).ok()?;
    lista.registrar(ontem, "pessoa", None).ok()?;
    let bilhete = bilhete.map(ToString::to_string);
    lista
        .anotar_caminhos(ontem, caminhos, bilhete.as_deref(), Some(impressao))
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
    let Some(guardada) = lembrar_de_ontem(casa.path(), &ontem, &[], Some(&bilhete), &real) else {
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
    let Some(guardada) = lembrar_de_ontem(casa.path(), &ontem, &[], None, &real) else {
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
/// `seele_ffi::impressao_a_guardar` decide a partir do veredito, do link desta
/// sessão e da impressão guardada, as mesmas entradas que o app lhe dá.
fn anotar_como_o_app(
    casa: &Path,
    alvo: &str,
    veredito: &Trust,
    do_link: Option<&str>,
    guardada: Option<&str>,
) {
    let Ok(mut lista) = Conhecidos::abrir(casa.join("conhecidos")) else {
        panic!("a lista de conhecidos não abriu");
    };
    let aceita = seele_ffi::impressao_a_guardar(veredito, do_link, guardada);
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
    anotar_como_o_app(casa.path(), &alvo, &veredito, Some(&real), None);
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
    anotar_como_o_app(casa.path(), &alvo, &veredito, Some(DE_OUTRO_SERVIDOR), None);
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

#[tokio::test(flavor = "multi_thread")]
async fn a_primeira_volta_pelo_alvo_cura_a_lista_envenenada() {
    let _vaga = vaga::minha();
    // A lista que a 0.15.0 deixou: ela gravava a impressão do link qualquer
    // que fosse o veredito, e um link de outro servidor colado para um
    // endereço já fixado deixava na entrada a chave do outro. A volta pela
    // lista confere por ela, e o servidor verdadeiro era recusado em todo
    // endereço sem pin.
    //
    // No alvo o pino prova a chave (ADR 0003): a volta por ele entra com
    // `InviteDisagrees`, e a lista passa a guardar a ofertada. Isso só vale
    // porque `InviteDisagrees` só nasce onde o pino prova o servidor: fora do
    // alvo ele não passa por cima da impressão guardada (os dois últimos
    // testes deste arquivo), e num alvo de LAN também não. O alvo daqui,
    // `127.0.0.1`, é esta máquina em qualquer rede, e conta como público.
    let Some((de_x, x)) = server_de_teste().await else {
        panic!("o servidor X não subiu");
    };
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há lista nem identidade");
    };
    let real = x.fingerprint().to_owned();
    let alvo = de_x.to_string();

    // O alvo fica fixado com a chave verdadeira de X por uma visita de antes.
    let (visita, veredito) = match conectar(config_do_link(casa.path(), alvo.clone(), &real)).await
    {
        Ok(entrada) => entrada,
        Err(erro) => panic!("a visita que fixa X não entrou: {erro:?}"),
    };
    assert_eq!(
        veredito,
        Trust::FirstContactVerified {
            fingerprint: real.clone()
        },
        "a visita que fixa X não conferiu, e o resto do teste perde o assunto"
    );
    visita.disconnect();

    // E a lista com a impressão de outro servidor na entrada de X.
    let Some(guardada) = lembrar_de_ontem(casa.path(), &alvo, &[], None, DE_OUTRO_SERVIDOR) else {
        panic!("a lista de conhecidos não devolveu a impressão envenenada");
    };

    // A volta pela lista, sem link: só a guardada, contra o pino do alvo.
    let configuracao = config(
        casa.path(),
        alvo.clone(),
        Vec::new(),
        seele_ffi::impressao_a_conferir(None, Some(&guardada)),
        None,
    );
    let (connection, veredito) = match conectar(configuracao).await {
        Ok(entrada) => entrada,
        Err(erro) => panic!(
            "a volta pelo alvo já fixado foi recusada pela impressão envenenada ({erro:?}): \
             no alvo, o pino decide (ADR 0003)"
        ),
    };
    assert_eq!(
        veredito,
        Trust::InviteDisagrees {
            expected: DE_OUTRO_SERVIDOR.to_owned(),
            offered: real.clone(),
        },
        "a volta pelo alvo não deu `InviteDisagrees` contra a impressão envenenada, e o teste \
         não mede o que diz medir"
    );
    anotar_como_o_app(casa.path(), &alvo, &veredito, None, Some(&guardada));
    connection.disconnect();

    assert_eq!(
        impressao_na_lista(casa.path(), &alvo),
        Some(real),
        "a primeira volta pelo alvo, onde o pino prova a chave de X, deixou a lista com a \
         impressão de outro servidor: a lista envenenada pela 0.15.0 não se cura, e o \
         servidor verdadeiro segue recusado em todo endereço sem pin"
    );

    x.shutdown();
}

/// A impressão que a lista de conhecidos guarda para `alvo`, relida do disco.
fn impressao_na_lista(casa: &Path, alvo: &str) -> Option<String> {
    Conhecidos::abrir(casa.join("conhecidos"))
        .ok()
        .and_then(|lista| {
            lista
                .buscar(alvo)
                .and_then(|conhecido| conhecido.impressao.clone())
        })
}

/// Sobe Y com um convite de uso único, e com a portaria ligada se `portaria`.
///
/// Com um convite emitido o servidor fica fechado (ADR 0021): só entra quem
/// traz um, e quem entra o gasta. Com a portaria (ADR 0030), um `Hello` com
/// convite válido vira um pedido na fila de quem hospeda. As duas coisas ficam
/// no banco de Y, e é por elas que se vê, do lado dele, se o `Hello` chegou.
async fn y_com_convite(banco: &Path, portaria: bool) -> (SocketAddr, Arc<Daemon>, String) {
    let convite = {
        let Ok(mut persistence) = Persistence::open(&Location::File(banco.to_path_buf())) else {
            panic!("o banco de Y não abriu");
        };
        if portaria && seele_server::portaria::ligar(&mut persistence, true).is_err() {
            panic!("a portaria de Y não ligou");
        }
        let Ok(convite) = admissao::criar_convite(&mut persistence, "para quem volta") else {
            panic!("o convite de Y não foi criado");
        };
        convite
    };
    let Some((onde, y)) = server_com_banco(banco).await else {
        panic!("o servidor Y não subiu");
    };
    (onde, y, convite)
}

/// O que a volta pela lista de X deixou: onde mora a lista, qual é a entrada,
/// e o que a conexão devolveu.
struct VoltaPelaLista {
    casa: tempfile::TempDir,
    ontem: String,
    /// Viva até o fim do teste: uma conexão que tivesse entrado em Y fica de pé
    /// enquanto este valor existir, e é isso que `wait_idle` do lado de Y mede.
    resultado: Result<(Arc<Connection>, Trust), ConnectionError>,
}

/// A volta pela lista de X, com Y já fixado num dos caminhos da entrada e o
/// convite de Y na mão.
///
/// O pino de Y é escrito na mesma loja que `Connection::connect` abre, sob a
/// chave que ela usa, e não por uma visita a Y: o que se observa é se o
/// `Hello` desta volta chega a Y, e uma visita de antes teria mandado um.
async fn voltar_pela_lista_com_y_fixado(
    no_y: &str,
    do_y: &str,
    do_x: &str,
    convite: &str,
) -> VoltaPelaLista {
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há lista nem identidade");
    };
    let Some(chave_de_y) = seele_ffi::chave_do_servidor(no_y) else {
        panic!("o endereço de Y não deu chave de pino");
    };
    let Ok(pins) = FilePinStore::open(casa.path().join("pins")) else {
        panic!("a loja de pins desta máquina não abriu");
    };
    pins.pin(&chave_de_y, do_y.to_owned());
    if let Some(falha) = pins.falha_de_gravacao() {
        panic!("o pino de Y não foi gravado, e o teste não mede o que diz medir: {falha}");
    }
    drop(pins);

    // A entrada de X: o endereço de ontem morreu, e o de Y está entre os
    // caminhos que a lista guardou.
    let ontem = mapeado(endereco_morto().port());
    let Some(guardada) = lembrar_de_ontem(casa.path(), &ontem, &[no_y.to_owned()], None, do_x)
    else {
        panic!("a lista de conhecidos não devolveu a impressão da primeira visita");
    };

    // Sem link: só a guardada, com o endereço de Y na corrida como o `connect`
    // do app o põe (os caminhos da lista), e o convite que a pessoa tem.
    let mut configuracao = config(
        casa.path(),
        ontem.clone(),
        vec![no_y.to_owned()],
        seele_ffi::impressao_a_conferir(None, Some(&guardada)),
        None,
    );
    configuracao.join_secret = Some(convite.to_owned());
    let resultado = conectar(configuracao).await;

    // Como o `connect` do app: a lista só é anotada depois de entrar.
    if let Ok((_, veredito)) = &resultado {
        anotar_como_o_app(casa.path(), &ontem, veredito, None, Some(&guardada));
    }
    VoltaPelaLista {
        casa,
        ontem,
        resultado,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn um_candidato_que_a_pessoa_nao_escolheu_nao_toma_a_entrada_da_lista() {
    let _vaga = vaga::minha();
    // O pino é por endereço de candidato, e o candidato nem sempre é o
    // endereço que a pessoa escolheu: entram na corrida a resposta do quarto e
    // os caminhos da lista, e um alternativo de LAN (`192.168.x.y:8383`) é o
    // mesmo de uma casa para outra. Se ali houver um servidor Y **já fixado**
    // nesta máquina, o pino confere com a chave de Y. Deixá-lo passar mandava a
    // Y o `Hello` (o convite, o apelido e a assinatura) e dava
    // `InviteDisagrees`, com a chave de Y a um passo da entrada de X na lista.
    //
    // Fora do alvo, a impressão prometida vale mais que o pino: Y é recusado
    // dentro do TLS, antes do `Hello`. Isso se vê do lado de Y, como em
    // `convite.rs`: nenhuma conexão de pé e o convite ainda inteiro. A batida
    // na portaria fica com o teste de baixo.
    let Ok(pasta) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há banco");
    };
    let (de_y, y, convite) = y_com_convite(&pasta.path().join("seele.db"), false).await;
    let do_y = y.fingerprint().to_owned();
    let no_y = de_y.to_string();
    // A chave do servidor X, dono da entrada. Nenhum servidor a atende: o que
    // se mede é o que acontece com Y e com a lista.
    let do_x = "ab".repeat(32);

    let volta = voltar_pela_lista_com_y_fixado(&no_y, &do_y, &do_x, &convite).await;

    // Do lado de Y. Uma volta que tivesse entrado nele estaria de pé agora, viva
    // em `volta`, e `wait_idle` não voltaria.
    if tokio::time::timeout(Duration::from_secs(10), y.wait_idle())
        .await
        .is_err()
    {
        panic!(
            "Y ficou com uma conexão de pé depois da volta pela lista de X: o `Hello` saiu \
             para um candidato que a pessoa não escolheu, fixado com a chave de outro servidor"
        );
    }
    // E o convite de Y continua inteiro: outra pessoa, com outra chave e outro
    // apelido, ainda entra com ele. O ADR 0017 prende o apelido à chave, e com
    // outro nenhum dos dois explica uma recusa, só o convite.
    let Ok(outra_casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há outra identidade");
    };
    let mut com_o_convite = config_do_link(outra_casa.path(), no_y.clone(), &do_y);
    com_o_convite.nickname = "joana".into();
    com_o_convite.join_secret = Some(convite.clone());
    match conectar(com_o_convite).await {
        Ok((entrou, _)) => entrou.disconnect(),
        Err(erro) => panic!(
            "o convite de Y foi gasto ({erro:?}): o `Hello` da volta pela lista de X saiu \
             com ele para um candidato que a pessoa não escolheu"
        ),
    }

    // Do lado de quem voltou: a recusa com nome, com a guardada de X como a
    // prometida e a chave de Y como a ofertada.
    assert_eq!(
        volta.resultado.as_ref().err(),
        Some(&ConnectionError::InviteMismatch {
            expected: do_x.clone(),
            offered: do_y,
        }),
        "a volta pela lista não foi recusada pela impressão guardada no candidato de Y, ou \
         foi recusada com outro nome"
    );
    // E a lista continua com X: nada entrou, e nada foi anotado.
    assert_eq!(
        impressao_na_lista(volta.casa.path(), &volta.ontem),
        Some(do_x),
        "a volta que passou por um candidato de outro servidor gravou a chave dele na entrada \
         de X: a próxima volta confere por ela, e a tomada é permanente e calada"
    );

    y.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn um_candidato_que_a_pessoa_nao_escolheu_nao_bate_na_portaria_dele() {
    let _vaga = vaga::minha();
    // O mesmo candidato de outro servidor, agora com a portaria do ADR 0030
    // ligada em Y. Um `Hello` com convite válido vira um pedido na fila de quem
    // hospeda Y, com o apelido de quem voltava pela lista de X: quem hospeda Y
    // passaria a ver, e poderia aprovar, uma batida que ninguém quis dar.
    let Ok(pasta) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há banco");
    };
    let banco = pasta.path().join("seele.db");
    let (de_y, y, convite) = y_com_convite(&banco, true).await;
    let do_y = y.fingerprint().to_owned();
    let do_x = "ab".repeat(32);

    let volta = voltar_pela_lista_com_y_fixado(&de_y.to_string(), &do_y, &do_x, &convite).await;

    let Ok(persistence) = Persistence::open(&Location::File(banco.clone())) else {
        panic!("o banco de Y não abriu por fora");
    };
    let Ok(fila) = seele_server::portaria::pedidos(&persistence) else {
        panic!("a fila da portaria de Y não se lê");
    };
    assert!(
        fila.is_empty(),
        "a portaria de Y recebeu a batida de quem voltava pela lista de X: o `Hello` saiu \
         para um candidato que a pessoa não escolheu: {fila:?}"
    );
    assert_eq!(
        volta.resultado.as_ref().err(),
        Some(&ConnectionError::InviteMismatch {
            expected: do_x,
            offered: do_y,
        }),
        "a volta pela lista não foi recusada pela impressão guardada no candidato de Y, ou \
         foi recusada com outro nome"
    );

    y.shutdown();
}
