//! Quem sai pelo app some da lista de presentes dos outros em menos de dois
//! segundos.
//!
//! # O defeito
//!
//! A pendência 50. Sair era pedir e não esperar: `Enlace::sair` voltava assim
//! que o `Sair` entrava na fila de comandos, e a thread da conexão da FFI
//! derrubava o runtime na linha seguinte. O `CONNECTION_CLOSE` que
//! `Client::disconnect` põe na fila do `quinn` só sai pelo driver da conexão,
//! que é uma tarefa daquele runtime — e morria com ele, sem ter rodado. O
//! servidor só desistia da pessoa pelo prazo de ociosidade do transporte, 20 s,
//! e até lá ela continuava presente para todo mundo: «se eu fecho o app no Mac,
//! o usuário não sai da sala para o Windows».
//!
//! # O que estes testes medem
//!
//! **A lista de presentes de quem ficou**, e não a chegada do `PersonGone`: é
//! ela que a tela desenha. Quem fica é um `Enlace` que dobra o que ouve num
//! [`Room`] e lê [`Room::presentes`], que o `PersonGone` encolhe. A medida
//! continua valendo quando a F1 trouxer o grupo dos membros fora do ar: quem
//! sai deixa de estar entre os presentes, e é isso que se lê aqui.
//!
//! Cada teste é uma porta de saída que a casca usa:
//!
//! - a FFI, como o app a chama: `Connection::disconnect` e o `Arc` largado;
//! - o `Enlace` num runtime de uma thread que cai logo depois de `sair`, que é
//!   a forma da thread da conexão;
//! - e a casca que sai do processo: `Connection::encerrar_e_esperar` e o
//!   processo morrendo na linha seguinte, como o app morre depois do
//!   `ExitRequested`. Este roda num processo filho, porque morrer é a prova.
//!
//! **Este arquivo é a medida.** A dedução de que a despedida cabe em menos de
//! dois segundos veio da leitura do `quinn`; quem a confirma, ou não, é ele.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore, Room};
use seele_ffi::{ConnectConfig, Connection};
use seele_proto::ids::PersonId;
use seele_proto::transport::IDLE_TIMEOUT;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// O que se promete a quem ficou: quem saiu some dos presentes dentro disto.
///
/// Dois segundos. A despedida é um datagrama no laço local e uma volta do
/// servidor desmontando a sessão; o número é folga para uma máquina carregada,
/// e fica uma ordem de grandeza abaixo do que ele separa — os 20 s do prazo de
/// ociosidade, que é como o servidor descobria a saída antes.
const ATE_SUMIR: Duration = Duration::from_secs(2);

/// Quanto se olha a lista depois da saída, antes de desistir.
///
/// **Além do prazo de ociosidade, de propósito.** Uma reprovação tem de dizer
/// quanto levou: «sumiu em 20,0 s» é a assinatura do defeito, o servidor
/// desistindo por silêncio e não pela despedida. Desistir em [`ATE_SUMIR`]
/// diria só que não sumiu.
const QUANTO_SE_OLHA: Duration = IDLE_TIMEOUT.saturating_add(Duration::from_secs(5));

/// Quanto cada passo de rede antes da saída tem para terminar: subir, entrar,
/// ser visto.
const PRAZO_DE_REDE: Duration = Duration::from_secs(20);

/// Como quem sai aparece na lista de quem fica.
const QUEM_SAI: &str = "quem-sai";

/// Sobe um servidor numa porta que o sistema escolhe, com banco em memória.
///
/// Um `Daemon` em memória não fala com ponto de encontro nenhum: nada aqui sai
/// desta máquina.
async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let aceitando = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, daemon))
}

fn destino(endereco: SocketAddr, apelido: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        impressao_esperada: None,
        o_pino_prova_o_servidor: true,
        aceito: None,
    }
}

/// Quem fica: um `Enlace` e a sala que ele dobra, já semeada com a própria
/// sessão como a casca semeia.
async fn quem_fica(endereco: SocketAddr) -> Result<(Enlace, Room)> {
    let fica = tokio::time::timeout(
        PRAZO_DE_REDE,
        Enlace::conectar(
            destino(endereco, "quem-fica"),
            SigningKey::from_bytes(&[81; 32]),
            Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
        ),
    )
    .await
    .expect("quem fica não entrou dentro do prazo de rede")?;
    let mut sala = Room::new();
    sala.adopt(fica.sessao(), "quem-fica");
    Ok((fica, sala))
}

/// A configuração com que quem sai entra pela FFI, sem voz: não há placa de
/// som numa máquina de integração contínua.
fn config(casa: &std::path::Path, endereco: SocketAddr) -> ConnectConfig {
    ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: QUEM_SAI.into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

/// Conecta pela mesma porta que o app usa, numa thread que pode bloquear —
/// como o `conectar` de `volta_pela_trilha.rs`.
async fn conectar_pela_ffi(config: ConnectConfig) -> Arc<Connection> {
    let entrou = tokio::time::timeout(
        PRAZO_DE_REDE,
        tokio::task::spawn_blocking(move || Connection::connect(config)),
    )
    .await
    .expect("quem sai não entrou pela FFI dentro do prazo de rede")
    .expect("a thread que conecta caiu");
    match entrou {
        Ok((conexao, _)) => conexao,
        Err(erro) => panic!("quem sai não entrou pela FFI: {erro:?}"),
    }
}

/// Quem tem este apelido entre os presentes, se tem.
fn presente(sala: &Room, apelido: &str) -> Option<PersonId> {
    sala.presentes
        .iter()
        .find(|id| {
            sala.people
                .get(id)
                .is_some_and(|pessoa| pessoa.nickname == apelido)
        })
        .copied()
}

/// Dobra o que quem fica ouve até a condição valer, ou até o prazo.
async fn dobrar_ate<F>(fica: &mut Enlace, sala: &mut Room, prazo: Duration, pronto: F) -> bool
where
    F: Fn(&Room) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if pronto(sala) {
            return true;
        }
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(20), fica.proximo()).await
        {
            sala.apply(&mensagem);
        }
    }
    pronto(sala)
}

/// Espera quem sai aparecer entre os presentes de quem fica, e devolve quem é.
async fn ver_entrar(fica: &mut Enlace, sala: &mut Room) -> PersonId {
    let viu = dobrar_ate(fica, sala, PRAZO_DE_REDE, |sala| {
        presente(sala, QUEM_SAI).is_some()
    })
    .await;
    assert!(
        viu,
        "quem fica não viu `{QUEM_SAI}` entre os presentes em {PRAZO_DE_REDE:?}: sem a entrada, \
         a saída não mede nada"
    );
    presente(sala, QUEM_SAI).expect("conferido logo acima")
}

/// Quanto levou, desde `saiu_em`, para `quem` deixar os presentes de quem fica.
async fn ver_sumir(
    fica: &mut Enlace,
    sala: &mut Room,
    quem: PersonId,
    saiu_em: Instant,
) -> Duration {
    let sumiu = dobrar_ate(fica, sala, QUANTO_SE_OLHA, |sala| {
        !sala.presentes.contains(&quem)
    })
    .await;
    let levou = saiu_em.elapsed();
    assert!(
        sumiu,
        "`{QUEM_SAI}` continuou entre os presentes de quem ficou por {levou:?} depois de sair: \
         nem a despedida nem o prazo de ociosidade do servidor o tiraram"
    );
    levou
}

/// **Quem sai pela FFI, como o app sai, some dos presentes dos outros em
/// menos de dois segundos.**
///
/// É o caminho do app inteiro abaixo da casca: `Connection::disconnect` pede o
/// fim, o laço de `drive` volta e chama `Enlace::sair`, e a thread da conexão
/// derruba o runtime logo depois. O fechamento tem de ter saído antes disso.
#[tokio::test(flavor = "multi_thread")]
async fn quem_sai_pela_ffi_some_dos_presentes_dos_outros_em_menos_de_dois_segundos() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let (mut fica, mut sala) = quem_fica(endereco).await?;
    let casa = tempfile::tempdir()?;

    let sai = conectar_pela_ffi(config(casa.path(), endereco)).await;
    let quem = ver_entrar(&mut fica, &mut sala).await;

    let saiu_em = Instant::now();
    sai.disconnect();
    drop(sai);
    let levou = ver_sumir(&mut fica, &mut sala, quem, saiu_em).await;
    println!("quem saiu pela FFI sumiu dos presentes em {levou:?}");

    assert!(
        levou < ATE_SUMIR,
        "quem saiu pela FFI levou {levou:?} para sumir dos presentes de quem ficou, e o \
         prometido é {ATE_SUMIR:?}: o `CONNECTION_CLOSE` não saiu antes de a thread da conexão \
         derrubar o runtime, e o servidor só viu a saída pelo prazo de ociosidade"
    );

    servidor.shutdown();
    Ok(())
}

/// **O `Enlace` que sai num runtime que cai em seguida ainda avisa.**
///
/// A forma da thread da conexão sem a FFI em volta: um runtime de uma thread
/// numa `std::thread`, que termina na linha seguinte a `sair().await`. O que
/// não saiu até ali não sai mais — então `sair` só pode voltar depois de o
/// motor ter se despedido.
#[tokio::test(flavor = "multi_thread")]
async fn o_enlace_que_sai_num_runtime_que_cai_em_seguida_ainda_avisa() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let (mut fica, mut sala) = quem_fica(endereco).await?;

    let (sair_tx, sair_rx) = tokio::sync::oneshot::channel::<()>();
    let linha = std::thread::Builder::new()
        .name("quem-sai".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("o runtime de quem sai");
            runtime.block_on(async move {
                let mut sai = Enlace::conectar(
                    destino(endereco, QUEM_SAI),
                    SigningKey::from_bytes(&[82; 32]),
                    Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
                )
                .await
                .expect("quem sai não entrou");
                let _ = sair_rx.await;
                sai.sair().await;
            });
            // A linha seguinte: o runtime cai, e com ele toda tarefa de quem
            // saiu — o driver do `quinn` junto.
            drop(runtime);
        })?;

    let quem = ver_entrar(&mut fica, &mut sala).await;

    let saiu_em = Instant::now();
    sair_tx
        .send(())
        .expect("a linha de quem sai espera a ordem de sair");
    let levou = ver_sumir(&mut fica, &mut sala, quem, saiu_em).await;
    println!("o enlace que saiu num runtime que caiu em seguida sumiu dos presentes em {levou:?}");
    let terminou = tokio::task::spawn_blocking(move || linha.join()).await?;
    assert!(
        terminou.is_ok(),
        "a linha de quem sai caiu em pânico, e a medida acima não é a da saída"
    );

    assert!(
        levou < ATE_SUMIR,
        "o enlace que saiu levou {levou:?} para sumir dos presentes de quem ficou, e o \
         prometido é {ATE_SUMIR:?}: `sair` voltou antes de o motor se despedir, o runtime caiu \
         com o `CONNECTION_CLOSE` na fila, e o servidor só viu a saída pelo prazo de ociosidade"
    );

    servidor.shutdown();
    Ok(())
}

/// A variável que faz deste binário o filho de
/// [`encerrar_e_esperar_volta_com_o_aviso_ja_dado`]. O valor é o endereço do
/// servidor do pai — o molde é o `CANARY` de
/// `crates/seele-audio/tests/realtime_safety.rs`.
const FILHO: &str = "SEELE_SAIDA_COM_DESPEDIDA_FILHO";

/// Onde o filho guarda a identidade e os pins: uma pasta do pai, que a apaga ao
/// fim. O filho morre por `exit`, e nada dele roda depois.
const CASA_DO_FILHO: &str = "SEELE_SAIDA_COM_DESPEDIDA_CASA";

/// O filho: entra pela FFI no servidor do pai, espera a ordem de sair, e sai
/// como o app sai — `encerrar_e_esperar` e o processo morrendo na linha
/// seguinte.
///
/// Bloqueia uma thread do runtime do teste de propósito: neste processo não há
/// mais nada para rodar, e a FFI tem a thread dela.
fn ser_o_filho(endereco: &str) -> ! {
    let endereco: SocketAddr = endereco
        .parse()
        .expect("o pai passa ao filho o endereço do servidor");
    let casa = std::env::var_os(CASA_DO_FILHO).expect("o pai passa ao filho uma casa");
    let sai = match Connection::connect(config(std::path::Path::new(&casa), endereco)) {
        Ok((conexao, _)) => conexao,
        Err(erro) => panic!("o filho não entrou pela FFI: {erro:?}"),
    };
    // A ordem é uma linha na entrada — ou a entrada fechada, que é o pai caindo.
    let mut ordem = String::new();
    let _ = std::io::stdin().read_line(&mut ordem);
    assert!(
        sai.encerrar_e_esperar(Duration::from_millis(500)),
        "a thread da conexão do filho não terminou em 500 ms"
    );
    std::process::exit(0);
}

/// **A casca que espera a thread da conexão pode morrer na linha seguinte: o
/// aviso já foi dado.**
///
/// É a saída do app inteira. No `ExitRequested`, a casca chama
/// `encerrar_e_esperar` e o processo vai embora — tudo o que não saiu até ali
/// morre com ele. Por isso o filho faz `exit(0)` logo depois, sem nada para
/// dar tempo ao fechamento, e o pai lê a lista de presentes de quem ficou.
///
/// Uma `encerrar_e_esperar` que voltasse sem esperar o sinal da thread da
/// conexão deixaria o processo morrer com a despedida no meio, e quem ficou só
/// veria a saída pelo prazo de ociosidade.
#[tokio::test(flavor = "multi_thread")]
async fn encerrar_e_esperar_volta_com_o_aviso_ja_dado() -> Result<()> {
    use std::io::Write as _;

    let _vaga = vaga::minha();
    if let Ok(endereco) = std::env::var(FILHO) {
        ser_o_filho(&endereco);
    }

    let (endereco, servidor) = servidor().await?;
    let (mut fica, mut sala) = quem_fica(endereco).await?;
    let casa = tempfile::tempdir()?;
    let mut filho = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "encerrar_e_esperar_volta_com_o_aviso_ja_dado",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(FILHO, endereco.to_string())
        .env(CASA_DO_FILHO, casa.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let quem = ver_entrar(&mut fica, &mut sala).await;

    let mut ordem = filho
        .stdin
        .take()
        .expect("a entrada do filho, pedida acima");
    let saiu_em = Instant::now();
    ordem.write_all(b"sai\n")?;
    drop(ordem);
    let levou = ver_sumir(&mut fica, &mut sala, quem, saiu_em).await;
    println!("o filho que saiu e morreu na linha seguinte sumiu dos presentes em {levou:?}");

    let saida = tokio::task::spawn_blocking(move || filho.wait_with_output()).await??;
    assert!(
        saida.status.success(),
        "o filho não saiu limpo ({}), e a medida acima não é a da saída do app:\n{}",
        saida.status,
        String::from_utf8_lossy(&saida.stderr)
    );
    assert!(
        levou < ATE_SUMIR,
        "o filho que saiu por `encerrar_e_esperar` e morreu na linha seguinte levou {levou:?} \
         para sumir dos presentes de quem ficou, e o prometido é {ATE_SUMIR:?}: \
         `encerrar_e_esperar` voltou antes de a despedida sair, o processo morreu com ela no \
         meio, e o servidor só viu a saída pelo prazo de ociosidade"
    );

    servidor.shutdown();
    Ok(())
}
