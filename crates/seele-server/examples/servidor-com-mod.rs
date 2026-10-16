//! Um servidor local para desenvolver **um MOD qualquer**, sem publicar nada.
//!
//! # Por que ele não traz nenhum MOD dentro
//!
//! A primeira versão deste exemplo embutia a Mesa por `include_bytes!`, e com
//! isso o repositório do produto carregava um MOD específico só para conseguir
//! levantar um servidor de teste. É o que o [ADR
//! 0045](../../../docs/adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md)
//! nega — «o produto base tem regras, e um MOD não» —, e na prática impedia a
//! Mesa de mudar de casa sem quebrar a compilação do SEELE.
//!
//! Agora a pasta do MOD é argumento. O exemplo serve a Mesa, serve o MOD de
//! outra pessoa, e não conhece nenhum dos dois.
//!
//! # Uso
//!
//! ```text
//! cargo run -p seele-server --example servidor-com-mod -- \
//!     /caminho/absoluto/do/mod  /caminho/absoluto/do/mundo  [porta]
//! ```
//!
//! A primeira pasta é o pacote — a que tem `mod.json` dentro. A segunda é o
//! mundo: banco, pacote instalado e os dados do MOD. Ela **não pode existir
//! ainda**, ou o comando recusa: reabrir um mundo com código diferente do que o
//! gerou é como se perde uma campanha, e essa é uma perda que não se desfaz.
//!
//! A porta é a 8390 se nada for dito, e **nunca a 8384**, que é a da bateria
//! de testes do repositório: um exemplo de pé nela derruba a bateria de quem
//! roda as duas coisas na mesma máquina.
//!
//! # O que ele faz, e o que não faz
//!
//! Instala o pacote **por hash**, onde o servidor o procura
//! (`seele_server::mods::instalar`; a prova é `tests/exemplo_com_mod.rs`), liga
//! o MOD antes de subir — a subida carrega o que está ligado, e só ela entrega
//! os momentos do `aoAcontecer` —, e diz no terminal o que a subida não
//! conseguiu carregar. Não abre janela: para ver a metade de janela, o app
//! entra neste servidor pelo endereço e pela impressão que ele imprime.
use anyhow::{Context, Result};
use seele_server::{
    mods::instalar::{instalar_de_pasta, raizes_de_um_mundo},
    persistence::{
        mods::{enable, EnabledMod},
        Location, Persistence,
    },
    Daemon, ServerConfig,
};
use std::{net::SocketAddr, path::PathBuf};

/// A porta de quem não diz outra.
const PORTA_PADRAO: u16 = 8390;

/// A porta da bateria de testes do repositório, que este exemplo não usa.
const PORTA_DA_BATERIA: u16 = 8384;

#[tokio::main]
async fn main() -> Result<()> {
    let uso = "uso: cargo run -p seele-server --example servidor-com-mod -- \
               /caminho/do/mod /caminho/do/mundo [porta]";
    let origem = PathBuf::from(std::env::args().nth(1).context(uso)?);
    let mundo = PathBuf::from(std::env::args().nth(2).context(uso)?);
    let porta = match std::env::args().nth(3) {
        Some(texto) => texto.parse::<u16>().context(uso)?,
        None => PORTA_PADRAO,
    };
    anyhow::ensure!(
        porta != PORTA_DA_BATERIA,
        "a {PORTA_DA_BATERIA} é a porta da bateria de testes do repositório; use outra"
    );
    anyhow::ensure!(
        origem.is_absolute() && mundo.is_absolute(),
        "use caminhos absolutos"
    );
    anyhow::ensure!(
        origem.join("mod.json").is_file(),
        "não achei mod.json em {} — aponte para a pasta do pacote",
        origem.display()
    );
    anyhow::ensure!(
        !mundo.exists(),
        "{} já existe. Escolha uma pasta nova: reabrir um mundo com código \
         diferente do que o gerou perde a campanha que está lá dentro.",
        mundo.display()
    );

    let raizes = raizes_de_um_mundo(&mundo);
    let (manifesto, hash) = instalar_de_pasta(&origem, &raizes)?;
    let banco = Location::File(mundo.join("seele.db"));
    enable(
        &Persistence::open(&banco)?,
        &EnabledMod {
            id: manifesto.id.clone(),
            version: manifesto.version.clone(),
            hash: hash.clone(),
            repo: manifesto.repo.clone(),
            reach: manifesto.reach.clone(),
            server_half: manifesto.server.is_some(),
            api: manifesto.api,
            tema: false,
        },
    )?;
    // O que a subida diz só no registro, dito aqui também: sem um assinante do
    // `tracing`, um MOD que não carrega sumiria em silêncio.
    let (_, queixas) =
        seele_server::mods::carregar_do_disco(&raizes, &[(manifesto.id.clone(), hash.clone())]);
    for queixa in &queixas {
        eprintln!("o MOD não carregou: {queixa}");
    }

    let daemon = Daemon::bind(ServerConfig {
        name: format!("{} — desenvolvimento", manifesto.id),
        listen: SocketAddr::from(([127, 0, 0, 1], porta)),
        database: banco,
        mods_dir: Some(raizes.clone()),
        ..ServerConfig::default()
    })
    .await?;
    println!(
        "MOD: {} {}\nServidor: {}\nImpressão digital: {}\nPacote instalado: {}",
        manifesto.id,
        manifesto.version,
        daemon.local_addr()?,
        daemon.fingerprint(),
        raizes.pacote_de(&hash).display()
    );
    daemon.run().await
}
