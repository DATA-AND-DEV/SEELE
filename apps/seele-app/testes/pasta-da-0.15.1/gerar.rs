//! A pasta de dados de quem hospedou na 0.15.1 com o `seele/estilo` 3.1.0 e
//! publicou um tema — escrita **pelo código da v0.15.1** (`54f542f94`).
//!
//! Não compila neste repositório: o `gerar.sh` copia este arquivo para
//! `crates/seele-server/examples/` de uma cópia do commit da v0.15.1, e só lá
//! ele roda. Cada passo é uma chamada pública daquela versão, e o quintal é
//! escrito pelo caminho de pedido dela (`mods::pedidos::executar`), com o
//! `aoPedir` do MOD rodando no QuickJS dela — não por um `INSERT` que imita.
//!
//! ```text
//! cargo run -p seele-server --example gerar-pasta-da-0-15 -- <saída> <pacote>
//! ```
//!
//! 1. o banco em `<saída>/seele.db`, o que o HOSPEDAR da 0.15.1 abre numa
//!    máquina sem servidor guardado (`persistence::banco_do_cliente`);
//! 2. a conta de quem hospeda (a primeira, que vira a Comandante);
//! 3. o pacote no cache por conteúdo, `<saída>/mod-packages/<hash>`;
//! 4. o MOD habilitado com aquele hash;
//! 5. o pedido `save` de quem hospeda, que grava o tema no quintal, e um `view`
//!    que o lê de volta.
//!
//! Sem subir servidor: nenhuma porta, e nenhuma identidade TLS no banco — a
//! pasta vai para o repositório.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;

use seele_server::permissions::Permissions;
use seele_server::persistence::mods::{self, EnabledMod};
use seele_server::persistence::{Location, Persistence};
use seele_server::server::{spawn_writer, Server, Telas};
use tokio::sync::{broadcast, Mutex};

/// A chave pública de quem hospeda. Só identifica a conta; nada assina com ela.
const CHAVE_DE_QUEM_HOSPEDA: [u8; 32] = [0x5e; 32];

/// O tema que quem hospeda publicou na 0.15.1.
const TEMA: &str = r##"{"accent":"#2f80ed","background":"#0b0d10","panel":"#12161b","text":"#e8edf2","muted":"#9aa6b2","border":"#2a323c","radius":4,"density":"compact","font":"sans","glow":false}"##;

/// Os arquivos do pacote, como `mods::pedidos::package` da v0.15.1 os lê
/// (privada lá): sem `dados/` na raiz, sem atalho.
fn arquivos(raiz: &Path, dir: &Path, saida: &mut Vec<(String, Vec<u8>)>) {
    for entrada in std::fs::read_dir(dir).unwrap() {
        let entrada = entrada.unwrap();
        if dir == raiz && entrada.file_name() == "dados" {
            continue;
        }
        assert!(!entrada.file_type().unwrap().is_symlink(), "atalho no pacote");
        let caminho = entrada.path();
        if caminho.is_dir() {
            arquivos(raiz, &caminho, saida);
        } else {
            saida.push((
                caminho.strip_prefix(raiz).unwrap().to_string_lossy().replace('\\', "/"),
                std::fs::read(&caminho).unwrap(),
            ));
        }
    }
}

fn copiar(de: &Path, para: &Path) {
    std::fs::create_dir_all(para).unwrap();
    for entrada in std::fs::read_dir(de).unwrap() {
        let entrada = entrada.unwrap();
        let destino = para.join(entrada.file_name());
        if entrada.path().is_dir() {
            copiar(&entrada.path(), &destino);
        } else {
            std::fs::copy(entrada.path(), destino).unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let mut argumentos = std::env::args().skip(1);
    let saida = std::path::PathBuf::from(argumentos.next().expect("uso: <saída> <pacote>"));
    let pacote = std::path::PathBuf::from(argumentos.next().expect("uso: <saída> <pacote>"));
    assert!(!saida.exists(), "a saída já existe: {}", saida.display());
    std::fs::create_dir_all(&saida).unwrap();

    // 1.
    let banco = seele_server::persistence::banco_do_cliente(&saida);
    let persistence = Persistence::open(&Location::File(banco)).expect("o banco abriu");

    // 2.
    let dono = Permissions::new(&persistence)
        .register_or_find(&CHAVE_DE_QUEM_HOSPEDA, "dono")
        .expect("a conta de quem hospeda");

    // 3. O hash é conferido de novo pelo `executar`, abaixo: um hash errado
    // aqui para o gerador com «package changed».
    let mut lidos = Vec::new();
    arquivos(&pacote, &pacote, &mut lidos);
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut lidos));
    let manifesto = seele_proto::mods::read_manifest(
        &std::fs::read_to_string(pacote.join("mod.json")).unwrap(),
    )
    .expect("o manifesto");
    let raizes = seele_server::RaizesDosMods {
        // `servidores::raizes_dos_mods(config, None)` do app da v0.15.1.
        pacotes: saida.clone(),
        dados: saida.join("mod-data-legado"),
    };
    copiar(&pacote, &raizes.pacote_de(&hash));

    // 4.
    mods::enable(
        &persistence,
        &EnabledMod {
            id: manifesto.id.clone(),
            version: manifesto.version.clone(),
            hash: hash.clone(),
            repo: manifesto.repo.clone(),
            reach: manifesto.reach.clone(),
            server_half: manifesto.server.is_some(),
        },
    )
    .expect("habilitar");

    // 5. O servidor como `mods::pedidos::tests::servidor_que_recusa` da
    // v0.15.1 o monta, com o banco de verdade e a raiz dos MODs.
    let persistence = Arc::new(Mutex::new(persistence));
    let (events, _) = broadcast::channel(64);
    let writes = spawn_writer(Arc::clone(&persistence), events.clone());
    let servidor = Arc::new(Server {
        esperas: Arc::new(std::sync::Mutex::new(Default::default())),
        mods_em_curso: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        pacotes_conferidos: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        persistence,
        events,
        writes,
        slots: Arc::new(Mutex::new(seele_server::server::Slots::default())),
        occupancy: Arc::new(Mutex::new(seele_server::server::Occupancy::default())),
        presentes: Arc::new(Mutex::new(seele_server::server::Presentes::default())),
        subida: Arc::new(Mutex::new(seele_server::tela::Subida::nova())),
        portaria: Arc::new(Mutex::new(seele_server::taxa::Portaria::nova())),
        atrasos: Arc::new(seele_server::server::Atrasos::default()),
        desassentamentos: Arc::new(seele_server::server::Desassentamentos::default()),
        pares: Arc::new(Mutex::new(seele_server::pares::Pares::default())),
        versao_do_anuncio: seele_proto::mods::VERSAO_DO_ANUNCIO,
        telas: Arc::new(Mutex::new(Telas::default())),
        anexos: None,
        caminho_bps: None,
        mods_dir: Some(raizes),
    });
    let salvar = format!(r#"{{"op":"save","theme":{TEMA}}}"#);
    let salvo = seele_server::mods::pedidos::executar(
        &servidor,
        dono.id,
        seele_proto::ids::ChannelId(0),
        &manifesto.id,
        &salvar,
    )
    .await;
    let visto = seele_server::mods::pedidos::executar(
        &servidor,
        dono.id,
        seele_proto::ids::ChannelId(0),
        &manifesto.id,
        r#"{"op":"view"}"#,
    )
    .await;
    assert!(salvo.contains(r#""ok":true"#), "o save foi recusado: {salvo}");
    assert!(visto.contains(r#""revision":1"#), "o view não leu o save: {visto}");
    println!("hash {hash}");
    println!("pessoa {}", dono.id.0);
    println!("save {salvo}");
    println!("view {visto}");
}
