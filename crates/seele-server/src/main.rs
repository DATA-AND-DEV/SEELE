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
        // `[::]` e não `0.0.0.0`: o segundo atende só IPv4, e um Dogma que não
        // atende em IPv6 não tem o degrau 2 do ADR 0022 nem quando as duas
        // pontas têm IPv6. Ver `seele_server::alcance`.
        .unwrap_or_else(|| format!("[::]:{}", seele_proto::transport::DEFAULT_PORT))
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
        let lan = lan_address();
        // O IPv6 global vem junto e vem primeiro, porque é o único dos dois que
        // funciona **de fora** sem encaminhar porta nenhuma — degrau 2 do
        // ADR 0022. O da LAN continua ali para quem está na mesma rede.
        let global = seele_server::alcance::endereco_de_saida_v6();
        if lan.is_some() || global.is_some() {
            println!();
            println!("na outra máquina:");
        }
        if let Some(seis) = global {
            println!(
                "  plug --server [{seis}]:{}   (pela internet, se o",
                bound.port()
            );
            println!("                             firewall do roteador deixar entrar)");
        }
        if let Some(lan) = lan {
            println!("  plug --server {lan}:{}   (na mesma rede)", bound.port());
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
    println!("  seeled [endereço]              sobe o Dogma (padrão [::]:8383)");
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
        // `SocketAddr` e não `format!("{ip}:{porta}")`: o `Display` do
        // `SocketAddr` põe os colchetes quando o endereço é IPv6, e a
        // interpolação à mão escreveria `2001:db8::1:8383` — exatamente a forma
        // que o cliente agora recusa. Gerar torto e recusar educadamente seria
        // pôr uma frase bonita em cima de um defeito nosso.
        |ip| SocketAddr::new(ip, seele_proto::transport::DEFAULT_PORT).to_string(),
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

/// This machine's address on the network somebody in the next room can reach.
///
/// Asked of the interfaces, not of the default route. A VPN captures the
/// default route, and the address it hands back is the tunnel's — reachable by
/// nobody on the local network. That was a real field failure, and it is why
/// this is one call into `seele_server::alcance` instead of the four lines of
/// UDP trickery that used to live here.
fn lan_address() -> Option<std::net::IpAddr> {
    seele_server::alcance::endereco_de_rede_local()
}
