//! `seeled` — the SEELE daemon.
//!
//! `specs/04-servidor-seele.md`: one instance is a **Dogma Central**. This binary
//! is a thin wrapper; everything it does lives in the library beside it, so the
//! integration tests can drive a server in process (`specs/10-convencoes.md`).

use std::net::SocketAddr;

use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "seele_server=info,seeled=info".into()),
        )
        .init();

    // Subcomandos de operação. Rodam contra o banco e saem, sem subir servidor:
    // o operador está com o `seeled` parado quando faz isto.
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    if let Some(primeiro) = argumentos.first() {
        match primeiro.as_str() {
            "convite" => return criar_convite(&argumentos),
            "senha" => return definir_senha(&argumentos),
            "--ajuda" | "--help" | "-h" => {
                uso();
                return Ok(());
            }
            _ => {}
        }
    }

    let listen: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("0.0.0.0:{}", seele_proto::transport::DEFAULT_PORT))
        .parse()
        .context("could not parse the listen address")?;

    let dogma = seele_server::DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen,
        // Em arquivo, não em memória. O padrão da biblioteca é `Memory`, que é
        // o certo para teste e o errado para um daemon: com ele o `seeled`
        // perdia contas, histórico e identidade TLS a cada reinício, e o
        // critério de aceite de M3 — "servidor reiniciado preserva estado e
        // histórico" — só passava nos testes, que pedem arquivo explicitamente.
        database: banco(),
        ..seele_server::DogmaConfig::default()
    };

    let server = seele_server::Server::bind(dogma).await?;
    let bound = server.local_addr()?;
    println!("seeled listening on {bound}");

    // What to type on the other machine. A server that only reports
    // `0.0.0.0:8383` has told the operator nothing they can use, and the first
    // thing anybody does with a self-hosted voice server is try it from a
    // second computer.
    if bound.ip().is_unspecified() {
        if let Some(lan) = lan_address() {
            println!();
            println!("na outra máquina:");
            println!("  plug --server {lan}:{}", bound.port());
        }
    }
    println!("certificate fingerprint: {}", server.fingerprint());
    println!();
    println!("TOFU (ADR 0003): a client pins this on first contact and refuses");
    println!("to connect silently if it ever changes. Read it out over another");
    println!("channel if somebody asks whether a change was real.");

    // Um Dogma sem porteiro na rede é a configuração certa para testar entre
    // duas máquinas e a errada para deixar ligada. Dizer isso em voz alta é o
    // que impede de virar padrão por esquecimento.
    if politica_aberta(&server) && !bound.ip().is_loopback() {
        println!();
        println!("  ATENÇÃO: este Dogma aceita qualquer um que alcance a porta.");
        println!("  Para fechar:  seeled senha <a senha>");
        println!("  Ou por convite:  seeled convite <para quem>");
    }

    server.run().await
}

/// Se o Dogma está aceitando qualquer um.
fn politica_aberta(server: &seele_server::Server) -> bool {
    server
        .politica_de_admissao()
        .map(|politica| politica.aberto())
        .unwrap_or(false)
}

fn uso() {
    println!("seeled — o servidor SEELE (um Dogma)");
    println!();
    println!("  seeled [endereço]              sobe o Dogma (padrão 0.0.0.0:8383)");
    println!("  seeled senha <senha>           exige esta senha para entrar");
    println!("  seeled senha --remover         volta a aceitar qualquer um");
    println!("  seeled convite [para quem]     gera um convite de uso único");
    println!();
    println!("  O convite sai como link seele://, pronto para mandar. Ele já");
    println!("  carrega a impressão digital do certificado, então quem receber");
    println!("  não precisa conferi-la por fora.");
}

/// Onde o Dogma guarda tudo.
///
/// `$SEELE_DB`, ou `seele.db` na pasta de onde o `seeled` foi executado.
fn banco() -> seele_server::casper::Location {
    let caminho = std::env::var("SEELE_DB").unwrap_or_else(|_| "seele.db".to_owned());
    seele_server::casper::Location::File(std::path::PathBuf::from(caminho))
}

/// Abre o banco do Dogma sem subir o servidor.
fn abrir_banco() -> anyhow::Result<seele_server::casper::Casper> {
    seele_server::casper::Casper::open(&banco())
}

fn criar_convite(argumentos: &[String]) -> anyhow::Result<()> {
    let observacao = argumentos.get(1).cloned().unwrap_or_default();
    let mut casper = abrir_banco()?;
    let token = seele_server::admissao::criar_convite(&mut casper, &observacao)?;

    // Cria a identidade se o Dogma ainda não subiu nenhuma vez. Sem isto o
    // primeiro convite sairia sem impressão digital — justamente o convite que
    // mais precisa dela, porque é o primeiro contato de alguém.
    let _ = seele_server::tls::Identity::load_or_create(&casper, vec!["localhost".into()]);
    let impressao = seele_server::Server::fingerprint_do_banco(&casper).ok();
    let alvo = lan_address().map_or_else(
        || format!("SEU-ENDERECO:{}", seele_proto::transport::DEFAULT_PORT),
        |ip| format!("{ip}:{}", seele_proto::transport::DEFAULT_PORT),
    );

    let mut convite = seele_proto::uri::Convite::novo(alvo).com_token(&token);
    if let Some(impressao) = impressao {
        convite = convite.com_impressao_digital(impressao);
    }

    println!(
        "convite criado{}",
        if observacao.is_empty() {
            String::new()
        } else {
            format!(" para {observacao}")
        }
    );
    println!();
    println!("  {convite}");
    println!();
    println!("  Vale uma vez só e por sete dias.");
    Ok(())
}

fn definir_senha(argumentos: &[String]) -> anyhow::Result<()> {
    let mut casper = abrir_banco()?;
    match argumentos.get(1).map(String::as_str) {
        Some("--remover") => {
            seele_server::admissao::definir_senha(&mut casper, None)?;
            println!("senha removida. O Dogma volta a aceitar qualquer um.");
        }
        Some(senha) if !senha.is_empty() => {
            seele_server::admissao::definir_senha(&mut casper, Some(senha))?;
            println!("senha definida. Quem entrar precisa dela.");
            println!();
            println!("  Um convite é melhor para uma pessoa só: vale uma vez e");
            println!("  não precisa ser trocado quando alguém sai do grupo.");
        }
        _ => {
            uso();
            anyhow::bail!("`seeled senha` precisa da senha, ou de --remover");
        }
    }
    Ok(())
}

/// This machine's address on the network it would reach the world through.
///
/// No dependency and no interface enumeration: connecting a UDP socket picks a
/// route and binds a local address without sending a single packet, which is
/// exactly the question being asked — "which of my addresses would somebody
/// else see". The target is TEST-NET-3 (`203.0.113.0/24`, RFC 5737), reserved
/// for documentation, so nothing is implied about reaching a real host.
fn lan_address() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("203.0.113.1:80").ok()?;
    let local = socket.local_addr().ok()?.ip();
    (!local.is_loopback() && !local.is_unspecified()).then_some(local)
}
