//! As ações declaradas de MOD de ponta a ponta (API 7, ADR 0064): servidor de
//! verdade, QuickJS de verdade, e a `Connection` da FFI — o mesmo objeto que o
//! comando Tauri `mod_request` segura quando a janela manda `acao`.
//!
//! O caminho inteiro do cliente nativo: `Connection::acao_de_mod` →
//! `Enlace::acao_de_mod` → `Client::acao_de_mod` → `ClientMessage::AcaoDeMod`
//! no fio → `mods::pedidos::executar_acao`, com o portão dos cargos antes da
//! metade de servidor. O cliente não decide nada: as respostas abaixo são do
//! servidor e do MOD.
//!
//! O MOD é a fixture da árvore `apps/seele-app/testes/mod-de-acoes/`, lida
//! como está: o padrão que o `api/README.md` (A API 7) manda seguir — a
//! operação protegida só roda quando o `contexto.acao` escrito pelo HOST é a
//! ação que a protege.
//!
//! - quem TEM a permissão (a dona declarada) passa, e a metade de servidor
//!   roda com `contexto.acao` igual à ação, venha o que vier no corpo;
//! - quem NÃO a tem recebe `sem-permissao`, e a metade de servidor não roda;
//! - uma ação que o MOD não declarou é `acao-nao-declarada`, até para a dona;
//! - o corpo de um `ModRequest` comum que imita a ação (`acao`, `contexto`,
//!   `admin`, `permissoes`) não forja nada: o `contexto.acao` chega `null`, a
//!   autoridade é a dos cargos, e o MOD recusa a operação protegida — até
//!   para a dona, que tem a permissão mas não pediu a ação;
//! - a leitura legada (`ModRequest` comum) continua funcionando para quem não
//!   tem a permissão.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_ffi::{ConnectConfig, Connection, Event, EventListener};
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};
use serde_json::{json, Value};

mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

/// A fixture portátil, na árvore.
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/seele-app/testes/mod-de-acoes"
);
const ARQUIVOS_DA_FIXTURE: [&str; 3] = ["mod.json", "servidor/main.js", "cliente/main.js"];

/// O pacote da fixture, endereçado pelo conteúdo como o instalador o deixa.
/// Devolve o id e o hash.
fn instalar(raiz: &Path) -> Result<(String, String)> {
    let mut arquivos = ARQUIVOS_DA_FIXTURE
        .iter()
        .map(|relativo| {
            let bytes = std::fs::read(Path::new(FIXTURE).join(relativo))
                .unwrap_or_else(|erro| panic!("a fixture `{relativo}` sumiu: {erro}"));
            ((*relativo).to_owned(), bytes)
        })
        .collect::<Vec<_>>();
    let manifesto = seele_proto::mods::read_manifest(std::str::from_utf8(&arquivos[0].1)?)
        .expect("o manifesto da fixture é aceito pelo produto");
    assert_eq!(manifesto.api, 7, "a fixture das ações é da API 7");
    assert!(
        manifesto.permissoes.iter().any(|p| p.acao == "fechar-mesa"),
        "a fixture deixou de declarar fechar-mesa"
    );
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let destino = raiz.join(seele_core::mods::PACOTES).join(&hash);
    for (relativo, bytes) in &arquivos {
        let caminho = destino.join(relativo);
        std::fs::create_dir_all(caminho.parent().expect("pai"))?;
        std::fs::write(caminho, bytes)?;
    }
    Ok((manifesto.id, hash))
}

/// As partes de uma resposta: `parte → (total, pedaço)`.
type Partes = BTreeMap<u32, (u32, String)>;

/// As respostas de MOD que a casca receberia, remontadas por pedido.
#[derive(Default)]
struct Respostas(Mutex<BTreeMap<u32, Partes>>);

impl EventListener for Respostas {
    fn on_event(&self, event: Event) {
        if let Event::ModReply {
            request,
            part,
            total,
            payload,
        } = event
        {
            if let Ok(mut todas) = self.0.lock() {
                todas
                    .entry(request)
                    .or_default()
                    .insert(part, (total, payload));
            }
        }
    }
}

impl Respostas {
    fn esperar(&self, request: u32) -> Value {
        let fim = Instant::now() + PRAZO;
        loop {
            {
                let todas = self.0.lock().unwrap();
                if let Some(partes) = todas.get(&request) {
                    let total = partes.values().next().map_or(0, |(total, _)| *total);
                    if total > 0 && partes.len() == total as usize {
                        let texto: String =
                            partes.values().map(|(_, pedaco)| pedaco.as_str()).collect();
                        return serde_json::from_str(&texto).expect("a resposta é JSON");
                    }
                }
            }
            assert!(
                Instant::now() < fim,
                "o pedido {request} não teve resposta em {PRAZO:?}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// Uma pessoa: a conexão e o que ela ouve.
struct Pessoa {
    conexao: Arc<Connection>,
    ouve: Arc<Respostas>,
}

impl Pessoa {
    fn acao(&self, request: u32, id: &str, acao: &str, corpo: &Value) -> Value {
        self.conexao
            .acao_de_mod(request, id.into(), 0, acao.into(), corpo.to_string())
            .expect("a ação saiu");
        self.ouve.esperar(request)
    }

    fn pedido(&self, request: u32, id: &str, corpo: &Value) -> Value {
        self.conexao
            .mod_request(request, id.into(), 0, corpo.to_string())
            .expect("o pedido saiu");
        self.ouve.esperar(request)
    }
}

fn conectar(endereco: SocketAddr, home: &Path, apelido: &str, conjunto: &str) -> Result<Pessoa> {
    std::fs::create_dir_all(home)?;
    // O sim aos MODs deste servidor, como a portaria o grava.
    let alvo = seele_ffi::chave_do_servidor(&endereco.to_string()).expect("o alvo");
    seele_core::aceites::Aceites::em(home).aceitar_agora(&alvo, conjunto)?;
    let (conexao, _confianca) = Connection::connect(ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.to_owned(),
        home: home.to_string_lossy().into_owned(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    })?;
    let ouve = Arc::new(Respostas::default());
    conexao.subscribe(Arc::clone(&ouve) as Arc<dyn EventListener>);
    Ok(Pessoa { conexao, ouve })
}

/// O corpo que imita a ação e a autoridade: tudo o que a ponte escreve no
/// contexto, escrito pelo cliente dentro do pedido.
fn forjado() -> Value {
    json!({
        "op": "fechar",
        "acao": "fechar-mesa",
        "admin": true,
        "write": true,
        "permissoes": ["fechar-mesa"],
        "cargo": "Administrador",
        "contexto": { "acao": "fechar-mesa", "admin": true, "permissoes": ["fechar-mesa"] },
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_acao_declarada_passa_com_o_cargo_e_e_recusada_sem_ele_ou_sem_declaracao() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let raiz: PathBuf = pasta.path().join("servidor");
    let (id, hash) = instalar(&raiz)?;
    let raizes = seele_server::RaizesDosMods {
        pacotes: raiz.clone(),
        dados: raiz.join("mod-data"),
    };
    let home_da_dona = pasta.path().join("dona");
    let home_da_visita = pasta.path().join("visita");
    let dona_impressao = seele_ffi::impressao_desta_maquina(&home_da_dona.to_string_lossy())?;
    let servidor = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Acoes".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::File(raiz.join("seele.db")),
            mods_dir: Some(raizes.clone()),
            proprietario: Some(dona_impressao),
            ..ServerConfig::default()
        })
        .await?,
    );
    let conjunto = {
        let db = servidor.server().persistence.lock().await;
        enable(
            &db,
            &EnabledMod {
                id: id.clone(),
                version: "1.0.0".into(),
                hash,
                repo: "https://github.com/DATA-AND-DEV/SEELE".into(),
                reach: vec!["pedido".into()],
                server_half: true,
                api: 7,
                tema: false,
            },
        )?;
        // O que o arranque faz com cada MOD ligado: as permissões do mod.json
        // gravadas para os cargos.
        let queixas = seele_server::mods::gravar_permissoes_declaradas(&db, &raizes);
        assert!(
            queixas.is_empty(),
            "as permissões não foram gravadas: {queixas:?}"
        );
        seele_server::mods::anuncio::conjunto_exigido(&db)?.identidade
    };
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });

    let resultado = tokio::task::spawn_blocking(move || -> Result<()> {
        let dona = conectar(endereco, &home_da_dona, "dona", &conjunto)?;
        let visita = conectar(endereco, &home_da_visita, "visita", &conjunto)?;

        // 1. O corpo de um pedido COMUM que imita a ação não forja nada, nem
        //    para a dona — que tem a permissão, mas não pediu a ação.
        for (n, quem) in [(1, &dona), (2, &visita)] {
            let resposta = quem.pedido(n, &id, &forjado());
            assert_eq!(
                resposta["contexto"]["acao"],
                Value::Null,
                "o corpo de um ModRequest comum forjou o contexto.acao: {resposta}"
            );
            assert_eq!(
                (resposta["ok"].clone(), resposta["erro"].clone()),
                (json!(false), json!("pede-a-acao-fechar-mesa")),
                "o ModRequest comum executou a operação protegida: {resposta}"
            );
        }
        // E a autoridade é a dos cargos, e não a do corpo.
        let da_visita = visita.pedido(3, &id, &forjado());
        assert_eq!(da_visita["contexto"]["admin"], false, "{da_visita}");
        assert_eq!(da_visita["contexto"]["permissoes"], json!([]), "{da_visita}");

        // 2. Quem não tem o cargo: recusado no servidor, pelo nome, mesmo com
        //    o corpo forjado, e a metade de servidor não roda.
        assert_eq!(
            visita.acao(4, &id, "fechar-mesa", &forjado()),
            json!({ "ok": false, "recusa": "sem-permissao" }),
            "quem não tem o cargo não foi recusado pelo nome"
        );
        // 3. Um nome que o MOD não declarou: nem a dona.
        assert_eq!(
            dona.acao(5, &id, "apagar-tudo", &json!({ "op": "fechar" })),
            json!({ "ok": false, "recusa": "acao-nao-declarada" }),
            "a ação não declarada não foi recusada pelo nome"
        );

        // 4. A leitura legada continua, e nada acima fechou a mesa.
        let antes = visita.pedido(6, &id, &json!({ "op": "ler" }));
        assert_eq!(
            (antes["ok"].clone(), antes["mesa"].clone(), antes["fechamentos"].clone()),
            (json!(true), json!("aberta"), json!(0)),
            "a leitura legada quebrou, ou uma recusa executou a operação: {antes}"
        );

        // 5. Quem tem o cargo passa, e o `contexto.acao` é o do HOST: o corpo
        //    que diz outra ação não o muda.
        let passou = dona.acao(
            7,
            &id,
            "fechar-mesa",
            &json!({ "op": "fechar", "acao": "apagar-tudo", "contexto": { "acao": "apagar-tudo" } }),
        );
        assert_eq!(passou["ok"], true, "a ação de quem tem o cargo não passou: {passou}");
        assert_eq!(
            passou["contexto"]["acao"], "fechar-mesa",
            "a metade de servidor não recebeu a ação que o host autorizou: {passou}"
        );
        assert_eq!(passou["fechamentos"], 1, "{passou}");

        // 6. A leitura legada vê o efeito, uma vez só.
        let depois = visita.pedido(8, &id, &json!({ "op": "ler" }));
        assert_eq!(
            (depois["mesa"].clone(), depois["fechamentos"].clone()),
            (json!("fechada"), json!(1)),
            "{depois}"
        );

        dona.conexao.disconnect();
        visita.conexao.disconnect();
        Ok(())
    })
    .await?;
    resultado
}
