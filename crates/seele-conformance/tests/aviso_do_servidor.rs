//! O aviso da metade de servidor às janelas, com QUIC, QuickJS e banco de
//! verdade (rascunho da API 6, §7.3 e §5; tarefa 8 do plano da F3).
//!
//! O que se prova aqui, e que os testes de unidade não provam sozinhos:
//!
//! 1. **Só quem pode ler o canal recebe**, e a janela de quem pediu também. A
//!    ausência é provada pela ordem do barramento: um aviso do servidor inteiro
//!    vem depois, e é o primeiro que chega a quem não lê o canal.
//! 2. **O aviso cai com o quintal**: um pedido que avisa e não consegue gravar
//!    não manda aviso nenhum.
//! 3. **`contexto.sala` é do servidor**: `null` fora de sala, o número da sala
//!    dentro dela, e um pedido que mande outra sala não muda nada.
//!
//! A taxa com o excedente junto por canal é provada no teste de unidade do
//! `Avisador`, com o relógio parado: aqui ela dependeria de quanto tempo cada
//! pedido leva.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore};
use seele_proto::{
    control::ServerMessage,
    ids::{ChannelId, VoiceRoomId},
};
use seele_server::{
    permissions::{Permissions, PERSON_ROLE},
    persistence::{
        mods::{enable, EnabledMod},
        Location, Persistence,
    },
    Daemon, ServerConfig,
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

mod vaga;

/// O canal que todo servidor novo tem.
const GERAL: ChannelId = ChannelId(1);

/// Quanto um ajudante espera pelo que procura, **no total** — e não por leitura,
/// pela razão que `tema_do_servidor.rs` escreve: o servidor manda quadros o
/// tempo todo, e um prazo por leitura nunca vence.
const PRAZO: Duration = Duration::from_secs(15);

/// O MOD: `avisar` avisa o que o pedido mandar; `quintal-cheio` avisa e grava
/// um quintal grande demais; `contexto` devolve o contexto que recebeu.
const MOD_QUE_AVISA: &str = r#"
globalThis.aoPedir = (contextoJSON, pedidoJSON) => {
  const pedido = JSON.parse(pedidoJSON);
  if (pedido.op === 'avisar') return JSON.stringify(mundo.avisar(pedido.aviso));
  if (pedido.op === 'quintal-cheio') {
    mundo.avisar({ carga: 'nunca' });
    dados.grande = 'a'.repeat(300 * 1024);
    return '{}';
  }
  return contextoJSON;
};
"#;

fn temporario(nome: &str) -> Result<PathBuf> {
    let raiz = std::env::temp_dir().join(format!(
        "seele-aviso-{nome}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir_all(&raiz)?;
    Ok(raiz)
}

fn instalar(raiz: &Path) -> Result<String> {
    let obras = raiz.join("em-obras");
    std::fs::create_dir_all(obras.join("servidor"))?;
    let manifesto = serde_json::json!({
        "schema": 1,
        "id": "seele/mesa",
        "version": "4.0.0",
        "api": seele_proto::mods::MOD_API_VERSION,
        "repo": "https://example.invalid/aviso",
        "server": "servidor/main.js",
    });
    let mut arquivos = vec![
        ("mod.json".to_owned(), manifesto.to_string().into_bytes()),
        (
            "servidor/main.js".to_owned(),
            MOD_QUE_AVISA.as_bytes().to_vec(),
        ),
    ];
    for (caminho, bytes) in &arquivos {
        std::fs::write(obras.join(caminho), bytes)?;
    }
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let destino = raiz.join(seele_core::mods::PACOTES).join(&hash);
    std::fs::create_dir_all(destino.parent().expect("pai"))?;
    std::fs::rename(&obras, &destino)?;
    Ok(hash)
}

async fn subir(raiz: &Path, hash: &str) -> Result<(Arc<Daemon>, SocketAddr, String)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Aviso".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::File(raiz.join("seele.db")),
            mods_dir: Some(seele_server::RaizesDosMods {
                pacotes: raiz.to_path_buf(),
                dados: raiz.join("mod-data"),
            }),
            ..ServerConfig::default()
        })
        .await?,
    );
    let conjunto = {
        let banco = daemon.server().persistence.lock().await;
        enable(
            &banco,
            &EnabledMod {
                id: "seele/mesa".into(),
                version: "4.0.0".into(),
                hash: hash.to_owned(),
                repo: "https://example.invalid/aviso".into(),
                reach: Vec::new(),
                server_half: true,
                api: seele_proto::mods::MOD_API_VERSION,
                tema: false,
            },
        )?;
        seele_server::mods::anuncio::conjunto_exigido(&banco)?.identidade
    };
    let endereco = daemon.local_addr()?;
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    Ok((daemon, endereco, conjunto))
}

/// Uma pessoa conectada, e cada `ModAviso` que ela já leu, na ordem.
struct Pessoa {
    cliente: Client,
    avisos: Vec<(Option<ChannelId>, String)>,
    pedidos: u32,
}

impl Pessoa {
    async fn entrar(endereco: SocketAddr, semente: u8, conjunto: &str) -> Result<Self> {
        let cliente = Client::connect(
            endereco,
            "localhost",
            &endereco.to_string(),
            &format!("pessoa{semente:03}"),
            &SigningKey::from_bytes(&[semente; 32]),
            Arc::new(MemoryPinStore::new()),
            None,
            Some(conjunto),
        )
        .await?;
        Ok(Self {
            cliente,
            avisos: Vec::new(),
            pedidos: 0,
        })
    }

    async fn ler(&mut self, fim: tokio::time::Instant, o_que: &str) -> Result<ServerMessage> {
        let quadro = tokio::time::timeout_at(fim, self.cliente.next_event())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "esperou {o_que} por {} s e não veio; os avisos vistos até aqui: {:?}",
                    PRAZO.as_secs(),
                    self.avisos
                )
            })??;
        if let ServerMessage::ModAviso {
            mod_id,
            canal,
            carga,
        } = &quadro
        {
            assert_eq!(mod_id, "seele/mesa", "um aviso de outro MOD");
            self.avisos.push((*canal, carga.clone()));
        }
        Ok(quadro)
    }

    /// Faz um pedido ao MOD, no escopo do servidor, e devolve a resposta.
    async fn pedir(&mut self, corpo: serde_json::Value) -> Result<serde_json::Value> {
        self.pedidos += 1;
        let pedido = self.pedidos;
        self.cliente
            .mod_request(pedido, "seele/mesa".into(), ChannelId(0), corpo.to_string())
            .await?;
        let fim = tokio::time::Instant::now() + PRAZO;
        loop {
            if let ServerMessage::ModReply {
                request, payload, ..
            } = self.ler(fim, "a resposta do pedido").await?
            {
                if request == pedido {
                    return Ok(serde_json::from_str(&payload)?);
                }
            }
        }
    }

    async fn esperar_avisos(&mut self, quantos: usize) -> Result<&[(Option<ChannelId>, String)]> {
        let fim = tokio::time::Instant::now() + PRAZO;
        let o_que = format!("{quantos} avisos");
        while self.avisos.len() < quantos {
            self.ler(fim, &o_que).await?;
        }
        Ok(&self.avisos)
    }
}

fn avisar(canal: Option<ChannelId>, carga: &str) -> serde_json::Value {
    serde_json::json!({
        "op": "avisar",
        "aviso": { "canal": canal.map(|c| c.0), "carga": carga },
    })
}

/// Tira o papel `Pessoa` de quem tem esta chave: sem ele, não lê canal nenhum.
fn tirar_o_papel(banco: &Path, semente: u8) -> Result<()> {
    let banco = Persistence::open(&Location::File(banco.to_path_buf()))?;
    let permissions = Permissions::new(&banco);
    let chave = SigningKey::from_bytes(&[semente; 32]);
    let pessoa = permissions.register_or_find(
        chave.verifying_key().as_bytes(),
        &format!("pessoa{semente:03}"),
    )?;
    permissions.revoke_role(pessoa.id, PERSON_ROLE)?;
    Ok(())
}

/// **Só quem pode ler o canal, e a janela de quem pediu também.**
#[tokio::test(flavor = "multi_thread")]
async fn o_aviso_de_um_canal_chega_so_a_quem_o_le() -> Result<()> {
    let _vaga = vaga::minha();
    let raiz = temporario("canal")?;
    let hash = instalar(&raiz)?;
    let (daemon, endereco, conjunto) = subir(&raiz, &hash).await?;

    let mut quem_pede = Pessoa::entrar(endereco, 241, &conjunto).await?;
    let mut quem_nao_le = Pessoa::entrar(endereco, 242, &conjunto).await?;
    tirar_o_papel(&raiz.join("seele.db"), 242)?;

    assert_eq!(
        quem_pede.pedir(avisar(Some(GERAL), "do-canal")).await?,
        serde_json::json!({ "ok": true })
    );
    assert_eq!(
        quem_pede.pedir(avisar(None, "do-servidor")).await?,
        serde_json::json!({ "ok": true })
    );
    assert_eq!(
        quem_pede.esperar_avisos(2).await?,
        &[
            (Some(GERAL), r#""do-canal""#.to_owned()),
            (None, r#""do-servidor""#.to_owned())
        ],
        "a janela de quem pediu não recebeu os dois avisos"
    );
    // Pela ordem do barramento: o do servidor saiu depois do do canal, e é o
    // primeiro a chegar a quem não lê o canal.
    assert_eq!(
        quem_nao_le.esperar_avisos(1).await?,
        &[(None, r#""do-servidor""#.to_owned())],
        "o aviso de um canal chegou a quem não pode lê-lo"
    );

    daemon.shutdown();
    // A limpeza não reprova o teste: no Windows, o servidor que acabou de
    // parar ainda segura o banco por um instante, e apagar um arquivo aberto
    // dá «os error 32». Achado pela bateria do Windows no publicar.sh da
    // 0.16 (24/10); a pasta é temporária, e o que o teste prova já passou.
    let _ = std::fs::remove_dir_all(raiz);
    Ok(())
}

/// **O aviso cai com o quintal** (pedido da MESA, 09/10).
#[tokio::test(flavor = "multi_thread")]
async fn um_aviso_de_um_pedido_que_nao_gravou_nao_sai() -> Result<()> {
    let _vaga = vaga::minha();
    let raiz = temporario("quintal")?;
    let hash = instalar(&raiz)?;
    let (daemon, endereco, conjunto) = subir(&raiz, &hash).await?;

    let mut pessoa = Pessoa::entrar(endereco, 243, &conjunto).await?;
    let recusado = pessoa
        .pedir(serde_json::json!({ "op": "quintal-cheio" }))
        .await?;
    assert_eq!(
        recusado["ok"], false,
        "o quintal grande demais foi gravado: {recusado}"
    );
    pessoa.pedir(avisar(None, "depois")).await?;
    assert_eq!(
        pessoa.esperar_avisos(1).await?,
        &[(None, r#""depois""#.to_owned())],
        "o aviso de um pedido cujo quintal foi recusado saiu"
    );

    daemon.shutdown();
    let _ = std::fs::remove_dir_all(raiz);
    Ok(())
}

/// **`contexto.sala` é do servidor** (rascunho da API 6, §5).
#[tokio::test(flavor = "multi_thread")]
async fn a_sala_do_contexto_e_a_da_presenca() -> Result<()> {
    let _vaga = vaga::minha();
    let raiz = temporario("sala")?;
    let hash = instalar(&raiz)?;
    let (daemon, endereco, conjunto) = subir(&raiz, &hash).await?;

    let mut pessoa = Pessoa::entrar(endereco, 244, &conjunto).await?;
    let fora = pessoa
        .pedir(serde_json::json!({ "op": "contexto", "sala": 99 }))
        .await?;
    assert_eq!(
        fora["sala"],
        serde_json::Value::Null,
        "fora de sala, o contexto disse {fora}"
    );

    pessoa
        .cliente
        .enter_voice_room(VoiceRoomId(1), None)
        .await?;
    // A presença assenta logo depois; o pedido é repetido até ela valer, com o
    // mesmo prazo total das esperas.
    let fim = tokio::time::Instant::now() + PRAZO;
    loop {
        let dentro = pessoa
            .pedir(serde_json::json!({ "op": "contexto", "sala": 99 }))
            .await?;
        if dentro["sala"] == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < fim,
            "dentro da sala 1, o contexto continuou dizendo {dentro}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    daemon.shutdown();
    let _ = std::fs::remove_dir_all(raiz);
    Ok(())
}
