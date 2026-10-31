//! A pasta de dados de quem hospedou na 0.15.1 com o `seele/perfis` 3.3.1, da
//! API 5, e gravou perfis — escrita **pelo código da v0.15.1** (`54f542f94`),
//! com a **metade de servidor de verdade** do PERFIS 3.3.1
//! (`SEELE-MOD-PERFIS@0774774`).
//!
//! Derivado do `../pasta-da-0.15.1/gerar.rs` (o do ESTILO), com os pedidos do
//! PERFIS no lugar do tema; o resto é o mesmo, chamada por chamada. Não compila
//! neste repositório: o `gerar.sh` ao lado o copia para
//! `crates/seele-server/examples/` de uma cópia do commit da v0.15.1.
//!
//! ```text
//! cargo run -p seele-server --example gerar-pasta-perfis -- <saída> <pacote>
//! ```
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

/// A chave pública da segunda pessoa, que só preencheu o texto.
const CHAVE_DA_CONVIDADA: [u8; 32] = [0x7a; 32];

/// Um PNG de 1×1 de verdade (assinatura, IHDR, IDAT e IEND).
const PNG_1X1: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// Base64 padrão, à mão: o gerador não ganha dependência por isto.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            s.push(if i <= c.len() { A[((n >> (18 - 6 * i)) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

fn base64_de_volta(texto: &str) -> Vec<u8> {
    let valor = |c: u8| match c {
        b'A'..=b'Z' => u32::from(c - b'A'),
        b'a'..=b'z' => u32::from(c - b'a') + 26,
        b'0'..=b'9' => u32::from(c - b'0') + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut saida = Vec::new();
    for q in texto.as_bytes().chunks(4) {
        let validos = q.iter().filter(|&&c| c != b'=').count();
        let n = q.iter().fold(0_u32, |n, &c| (n << 6) | if c == b'=' { 0 } else { valor(c) });
        saida.extend_from_slice(&n.to_be_bytes()[1..validos]);
    }
    saida
}

async fn pedir(servidor: &Arc<Server>, quem: seele_proto::ids::PersonId, pedido: &str) -> serde_json::Value {
    let resposta = seele_server::mods::pedidos::executar(
        servidor,
        quem,
        seele_proto::ids::ChannelId(0),
        "seele/perfis",
        pedido,
    )
    .await;
    let lida: serde_json::Value = serde_json::from_str(&resposta).expect("resposta em JSON");
    assert_eq!(lida["ok"], true, "o PERFIS 3.3.1 recusou {pedido}: {resposta}");
    lida
}

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
    let dados_do_perfis = raizes.dados_de(&manifesto.id);

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
            plataformas: None,
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
    let convidada = Permissions::new(&*servidor.persistence.lock().await)
        .register_or_find(&CHAVE_DA_CONVIDADA, "convidada")
        .expect("a conta da convidada");

    // 6. O texto de quem hospeda, pelo `save` do 3.3.1 (revisão 0 → 1).
    pedir(
        &servidor,
        dono.id,
        r##"{"op":"save","revision":0,"profile":{"displayName":"Dona da Casa","pronouns":"ela/dela","bio":"hospedo desde a 0.15","status":"no ar","accent":"#3366CC","effect":"aurora"}}"##,
    )
    .await;

    // 7. A foto pelo volume (o transporte do 3.2 em diante, o que a 0.15.1
    // usa): o `upload-start` registra a espera e o `upload-finish` confere o
    // tamanho. **O único passo simulado**: os bytes são postos onde o fluxo
    // da 0.15.1 os deixaria (`<dados>/volume/images/<pessoa>-<token>`), em vez
    // de virem por um fluxo de rede, que este gerador não abre.
    let foto = {
        let mut bytes = base64_de_volta(PNG_1X1);
        bytes.extend_from_slice(b"foto-da-dona");
        bytes
    };
    let inicio = pedir(
        &servidor,
        dono.id,
        &format!(r#"{{"op":"upload-start","slot":"avatar","transporte":"volume","bytes":{}}}"#, foto.len()),
    )
    .await;
    let token = inicio["token"].as_str().expect("o token").to_owned();
    let destino = dados_do_perfis
        .join("volume/images")
        .join(format!("{}-{token}", dono.id.0));
    std::fs::create_dir_all(destino.parent().unwrap()).unwrap();
    std::fs::write(&destino, &foto).unwrap();
    pedir(&servidor, dono.id, &format!(r#"{{"op":"upload-finish","token":"{token}"}}"#)).await;

    // 8. A capa pelos fragmentos (`.parts`, o transporte do 1.2 ao 3.1, que
    // o 3.3.1 ainda aceita sem `transporte`): um data-URL de dois pedaços.
    let capa = {
        let mut bytes = base64_de_volta(PNG_1X1);
        bytes.extend(std::iter::repeat_n(0xA5_u8, 5000));
        bytes
    };
    let data_url = format!("data:image/png;base64,{}", base64(&capa));
    let inicio = pedir(
        &servidor,
        dono.id,
        &format!(r#"{{"op":"upload-start","slot":"banner","length":{}}}"#, data_url.len()),
    )
    .await;
    let token = inicio["token"].as_str().expect("o token").to_owned();
    let pedacos: Vec<&str> = data_url
        .as_bytes()
        .chunks(6000)
        .map(|p| std::str::from_utf8(p).unwrap())
        .collect();
    assert_eq!(pedacos.len(), 2, "a capa tem de vir em dois pedaços");
    for (indice, parte) in pedacos.iter().enumerate() {
        pedir(
            &servidor,
            dono.id,
            &serde_json::json!({"op":"upload-part","token":token,"index":indice,"part":parte}).to_string(),
        )
        .await;
    }

    // 9. A convidada, só com o texto.
    pedir(
        &servidor,
        convidada.id,
        r##"{"op":"save","revision":0,"profile":{"displayName":"Convidada","pronouns":"","bio":"","status":"de passagem","accent":"#f2521f","effect":"none"}}"##,
    )
    .await;

    let visto = pedir(&servidor, dono.id, &format!(r#"{{"op":"view","people":["{}","{}"]}}"#, dono.id.0, convidada.id.0)).await;
    println!("hash {hash}");
    println!("dona {} convidada {}", dono.id.0, convidada.id.0);
    println!("foto {} bytes, capa {} bytes", foto.len(), capa.len());
    println!("view {visto}");
}
