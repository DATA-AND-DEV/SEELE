//! `seeled` — the SEELE daemon.
//!
//! `specs/04-servidor-seele.md`: one instance is a **server Central**. This binary
//! is a thin wrapper; everything it does lives in the library beside it, so the
//! integration tests can drive a server in process (`specs/10-convencoes.md`).

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "num teste, o pânico é o relatório"
    )
)]

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
            "anexos" => return teto_de_anexos(&argumentos),
            "--ajuda" | "--help" | "-h" => {
                uso();
                return Ok(());
            }
            _ => {}
        }
    }

    let listen: SocketAddr = std::env::args()
        .nth(1)
        // `[::]` e não `0.0.0.0`: o segundo atende só IPv4, e um servidor que não
        // atende em IPv6 não tem o degrau 2 do ADR 0022 nem quando as duas
        // pontas têm IPv6. Ver `seele_server::alcance`.
        .unwrap_or_else(|| format!("[::]:{}", seele_proto::transport::DEFAULT_PORT))
        .parse()
        .context("could not parse the listen address")?;

    let server = seele_server::ServerConfig {
        name: "Terceira Tóquio".into(),
        listen,
        // Em arquivo, não em memória. O padrão da biblioteca é `Memory`, que é
        // o certo para teste e o errado para um daemon: com ele o `seeled`
        // perdia contas, histórico e identidade TLS a cada reinício, e o
        // critério de aceite de M3 — "servidor reiniciado preserva estado e
        // histórico" — só passava nos testes, que pedem arquivo explicitamente.
        database: banco(),
        ..seele_server::ServerConfig::default()
    };

    let server = seele_server::Daemon::bind(server).await?;
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
                "  connection --server [{seis}]:{}   (pela internet, se o",
                bound.port()
            );
            println!("                             firewall do roteador deixar entrar)");
        }
        if let Some(lan) = lan {
            println!(
                "  connection --server {lan}:{}   (na mesma rede)",
                bound.port()
            );
        }
    }
    println!("certificate fingerprint: {}", server.fingerprint());
    println!();
    println!("TOFU (ADR 0003): a client pins this on first contact and refuses");
    println!("to connect silently if it ever changes. Read it out over another");
    println!("channel if somebody asks whether a change was real.");

    // Um servidor sem porteiro na rede é a configuração certa para testar entre
    // duas máquinas e a errada para deixar ligada. Dizer isso em voz alta é o
    // que impede de virar padrão por esquecimento.
    if politica_aberta(&server) && !bound.ip().is_loopback() {
        println!();
        println!("  ATENÇÃO: este servidor aceita qualquer um que alcance a porta.");
        println!("  Para fechar:  seeled senha <a senha>");
        println!("  Ou por convite:  seeled convite <para quem>");
    }

    server.run().await
}

/// Se o servidor está aceitando qualquer um.
fn politica_aberta(server: &seele_server::Daemon) -> bool {
    server
        .politica_de_admissao()
        .map(|politica| politica.aberto())
        .unwrap_or(false)
}

fn uso() {
    println!("seeled — o servidor SEELE (um servidor)");
    println!();
    println!("  seeled [endereço]              sobe o servidor (padrão [::]:8383)");
    println!("  seeled senha <senha>           exige esta senha para entrar");
    println!("  seeled senha --remover         volta a aceitar qualquer um");
    println!("  seeled convite [para quem]     gera um convite de uso único");
    println!("  seeled anexos                  mostra o teto de disco dos anexos");
    println!("  seeled anexos <tamanho>        escolhe o teto (ex.: 2G, 500M)");
    println!();
    println!("  O convite sai como link seele://, pronto para mandar. Ele já");
    println!("  carrega a impressão digital do certificado, então quem receber");
    println!("  não precisa conferi-la por fora.");
}

/// Onde o servidor guarda tudo.
///
/// `$SEELE_DB`, ou `seele.db` na pasta de onde o `seeled` foi executado.
fn banco() -> seele_server::persistence::Location {
    let caminho = std::env::var("SEELE_DB").unwrap_or_else(|_| "seele.db".to_owned());
    seele_server::persistence::Location::File(std::path::PathBuf::from(caminho))
}

/// Abre o banco do servidor sem subir o servidor.
fn abrir_banco() -> anyhow::Result<seele_server::persistence::Persistence> {
    seele_server::persistence::Persistence::open(&banco())
}

fn criar_convite(argumentos: &[String]) -> anyhow::Result<()> {
    let observacao = argumentos.get(1).cloned().unwrap_or_default();
    let mut persistence = abrir_banco()?;
    let token = seele_server::admissao::criar_convite(&mut persistence, &observacao)?;

    // Cria a identidade se o servidor ainda não subiu nenhuma vez. Sem isto o
    // primeiro convite sairia sem impressão digital — justamente o convite que
    // mais precisa dela, porque é o primeiro contato de alguém.
    let _ = seele_server::tls::Identity::load_or_create(&persistence, vec!["localhost".into()]);
    let impressao = seele_server::Daemon::fingerprint_do_banco(&persistence).ok();
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
    let mut persistence = abrir_banco()?;
    match argumentos.get(1).map(String::as_str) {
        Some("--remover") => {
            seele_server::admissao::definir_senha(&mut persistence, None)?;
            println!("senha removida. O servidor volta a aceitar qualquer um.");
        }
        Some(senha) if !senha.is_empty() => {
            seele_server::admissao::definir_senha(&mut persistence, Some(senha))?;
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

/// Mostra ou escolhe o teto de disco dos anexos. ADR 0027.
///
/// Subcomando, e não arquivo: o critério é o que a própria migração 2 escreveu
/// ao criar a tabela `configuracao` — "configuração do servidor que não cabe num
/// arquivo, porque muda em tempo de execução e precisa sobreviver a reinício".
/// Mexer no teto com o servidor no ar é o caso normal, e o TOML que `specs/04`
/// descreve **não existe**: não vai nascer por causa de um número.
fn teto_de_anexos(argumentos: &[String]) -> anyhow::Result<()> {
    use seele_server::persistence::attachments;

    let persistence = abrir_banco()?;
    let Some(pedido) = argumentos.get(1) else {
        let teto = attachments::quota(&persistence)?;
        let escolhido = attachments::quota_is_chosen(&persistence)?;
        println!(
            "teto de anexos: {}{}",
            tamanho(teto),
            if escolhido { "" } else { "  (padrão)" }
        );
        println!(
            "  por arquivo:  {}",
            tamanho(attachments::per_file_limit(teto))
        );
        println!();
        println!("  O servidor nunca guarda mais que isso. Ao encher, o anexo mais");
        println!("  antigo sai, e a mensagem passa a dizer que o arquivo expirou —");
        println!("  o texto sobrevive ao arquivo.");
        println!();
        println!("  Para mudar:  seeled anexos 2G");
        return Ok(());
    };

    let bytes = ler_tamanho(pedido)
        .with_context(|| format!("não entendi o tamanho «{pedido}»; tente 2G, 500M ou 1048576"))?;
    let antes = attachments::quota(&persistence)?;
    attachments::set_quota(&persistence, bytes)?;
    println!("teto de anexos: {}", tamanho(bytes));
    println!(
        "  por arquivo:  {}",
        tamanho(attachments::per_file_limit(bytes))
    );

    // Dito antes de acontecer, e não descoberto depois. Baixar o teto abaixo do
    // que já está guardado é uma escolha legítima — quem hospeda disse que o
    // disco vale menos do que valia — e o despejo acontece na próxima subida.
    if bytes < antes {
        println!();
        println!("  Isto é menos do que antes. Os anexos mais antigos que não");
        println!("  couberem serão descartados na próxima vez que o servidor subir,");
        println!("  e as mensagens deles passarão a dizer que o arquivo expirou.");
    }
    Ok(())
}

/// Lê `2G`, `500M`, `1024K` ou um número de bytes.
///
/// Binário e não decimal: um gibibyte é o que o teto padrão é, e um produto que
/// dissesse «1G» querendo dizer 10^9 estaria mentindo sobre o número que a
/// pessoa escolheu — que é a única coisa que esta decisão promete.
fn ler_tamanho(texto: &str) -> anyhow::Result<u64> {
    let texto = texto.trim();
    let (numero, escala) = match texto.chars().last() {
        Some('G' | 'g') => (&texto[..texto.len() - 1], 1024 * 1024 * 1024),
        Some('M' | 'm') => (&texto[..texto.len() - 1], 1024 * 1024),
        Some('K' | 'k') => (&texto[..texto.len() - 1], 1024),
        _ => (texto, 1),
    };
    let valor: u64 = numero.trim().parse()?;
    valor
        .checked_mul(escala)
        .ok_or_else(|| anyhow::anyhow!("esse número não cabe"))
}

/// Escreve um número de bytes do jeito que quem hospeda o escreveria.
fn tamanho(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    const KIB: u64 = 1024;
    if bytes >= GIB && bytes.is_multiple_of(GIB) {
        format!("{} GiB", bytes / GIB)
    } else if bytes >= MIB && bytes.is_multiple_of(MIB) {
        format!("{} MiB", bytes / MIB)
    } else if bytes >= KIB && bytes.is_multiple_of(KIB) {
        format!("{} KiB", bytes / KIB)
    } else {
        format!("{bytes} bytes")
    }
}

/// This machine's address on the network somebody in the next room can reach.
///
/// Asked of the interfaces, not of the default route. A VPN captures the
/// default route, and the address it hands back is the tunnel's — reachable by
/// nobody on the local network. That was a real field failure, and it is why
/// this is one call into `seele_server::alcance` instead of the four channels of
/// UDP trickery that used to live here.
fn lan_address() -> Option<std::net::IpAddr> {
    seele_server::alcance::endereco_de_rede_local()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_tamanho_e_lido_em_binario_e_nao_em_decimal() {
        // Um produto que dissesse «1G» querendo dizer 10^9 estaria mentindo
        // sobre o número que a pessoa escolheu, e esse número é a única coisa
        // que o ADR 0027 promete.
        assert_eq!(ler_tamanho("1G").expect("1G"), 1024 * 1024 * 1024);
        assert_eq!(ler_tamanho("2g").expect("2g"), 2 * 1024 * 1024 * 1024);
        assert_eq!(ler_tamanho("500M").expect("500M"), 500 * 1024 * 1024);
        assert_eq!(ler_tamanho("64k").expect("64k"), 64 * 1024);
        assert_eq!(ler_tamanho(" 4096 ").expect("cru"), 4096);
    }

    #[test]
    fn um_tamanho_que_nao_e_tamanho_recusa_em_vez_de_virar_zero() {
        // Virar zero seria um servidor que aceita a transferência e não consegue
        // guardá-la, e ninguém teria digitado isso de propósito.
        for torto in ["", "muito", "-1", "1TB", "3,5G", "999999999999999999999G"] {
            assert!(ler_tamanho(torto).is_err(), "aceitou «{torto}»");
        }
    }

    #[test]
    fn o_tamanho_volta_escrito_como_alguem_o_escreveria() {
        assert_eq!(tamanho(1024 * 1024 * 1024), "1 GiB");
        assert_eq!(tamanho(64 * 1024 * 1024), "64 MiB");
        assert_eq!(tamanho(1500), "1500 bytes");
    }
}
