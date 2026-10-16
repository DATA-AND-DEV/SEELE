//! O tema do servidor, com QUIC, QuickJS e banco de verdade.
//!
//! Rascunho da API 6, §9, e as tarefas 17a a 17c do plano da F3. O tema é
//! **dado do servidor**: a metade de servidor de um MOD com a porta
//! (`"tema": true`) o declara com `mundo.tema`, o servidor o guarda no banco e
//! o manda — a quem entra, logo depois do `Session` e do `Instancia`, e a todos
//! a cada troca.
//!
//! O que se prova aqui, e que nenhum teste de unidade prova sozinho:
//!
//! 1. **Todos recebem, e o tema sobrevive ao reinício.** Um tema que vivesse
//!    só na memória do `seeled` sumiria no reinício, e a metade de servidor não
//!    tem gancho de carga para declará-lo de novo.
//! 2. **Desligar tira, religar devolve.** Desligar preserva os dados (ADR
//!    0045), e o tema é um dado.
//! 3. **Um tema por servidor, e a porta é o manifesto.** O segundo MOD com a
//!    porta e o MOD sem ela recebem a recusa pelo nome, e nenhum `TemaMudou`
//!    sai por causa deles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore};
use seele_proto::{
    control::{DisconnectReason, ServerMessage, TemaDoServidor},
    ids::ChannelId,
};
use seele_server::{
    persistence::{
        mods::{disable, enable, EnabledMod},
        Location,
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

/// Declara o tema que o pedido trouxe e devolve o que `mundo.tema` respondeu.
/// Com `encher`, grava depois um quintal grande demais na mesma chamada: o
/// pedido inteiro falha ao gravar.
const MOD_DE_TEMA: &str = "globalThis.aoPedir = (contexto, pedido) => { \
     const p = JSON.parse(pedido); \
     const r = JSON.stringify(mundo.tema(p.tema)); \
     if (p.encher) dados.grande = 'a'.repeat(300 * 1024); \
     return r; };";

const CLARO: &str = r##"{"acento":"#c86b1f"}"##;
const ESCURO: &str = r##"{"acento":"#331704"}"##;

fn tema(mod_id: &str, valores: &str) -> Option<TemaDoServidor> {
    Some(TemaDoServidor {
        mod_id: mod_id.to_owned(),
        valores: valores.to_owned(),
    })
}

fn temporario(nome: &str) -> Result<PathBuf> {
    let raiz = std::env::temp_dir().join(format!(
        "seele-tema-{nome}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir_all(&raiz)?;
    Ok(raiz)
}

/// Escreve o pacote em `mod-packages/<hash>/`, como o instalador, e devolve o
/// hash. `porta` decide se o manifesto declara `"tema": true`.
fn instalar(raiz: &Path, id: &str, porta: bool) -> Result<String> {
    let obras = raiz.join(format!("em-obras-{}", id.replace('/', "-")));
    std::fs::create_dir_all(obras.join("servidor"))?;
    let mut manifesto = serde_json::json!({
        "schema": 1,
        "id": id,
        "version": "4.0.0",
        // Da constante, pela razão que `ponte_de_pedidos.rs` escreve.
        "api": seele_proto::mods::MOD_API_VERSION,
        "repo": "https://example.invalid/tema",
        "server": "servidor/main.js",
    });
    if porta {
        manifesto["tema"] = serde_json::Value::Bool(true);
    }
    let mut arquivos = vec![
        ("mod.json".to_owned(), manifesto.to_string().into_bytes()),
        (
            "servidor/main.js".to_owned(),
            MOD_DE_TEMA.as_bytes().to_vec(),
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

/// Um servidor sobre o banco **em arquivo** de `raiz`: é o que deixa outro
/// `Daemon` abrir o mesmo banco, que é o reinício.
async fn subir(raiz: &Path) -> Result<(Arc<Daemon>, SocketAddr, tokio::task::JoinHandle<()>)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Tema".into(),
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
    let endereco = daemon.local_addr()?;
    let servindo = Arc::clone(&daemon);
    let tarefa = tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    Ok((daemon, endereco, tarefa))
}

async fn derrubar(daemon: Arc<Daemon>, tarefa: tokio::task::JoinHandle<()>) {
    daemon.shutdown();
    tarefa.abort();
    let _ = tarefa.await;
}

fn ligado(id: &str, hash: &str) -> EnabledMod {
    EnabledMod {
        id: id.to_owned(),
        version: "4.0.0".to_owned(),
        hash: hash.to_owned(),
        repo: "https://example.invalid/tema".to_owned(),
        reach: Vec::new(),
        server_half: true,
        api: seele_proto::mods::MOD_API_VERSION,
        tema: false,
    }
}

/// Liga os MODs e devolve a identidade do conjunto que o servidor passa a
/// exigir, que é o aceite com que se entra.
async fn habilitar(daemon: &Daemon, mods: &[(&str, &str)]) -> Result<String> {
    let banco = daemon.server().persistence.lock().await;
    for (id, hash) in mods {
        enable(&banco, &ligado(id, hash))?;
    }
    Ok(seele_server::mods::anuncio::conjunto_exigido(&banco)?.identidade)
}

/// Quanto um ajudante espera pelo que procura, **no total**.
///
/// No total, e não por leitura. O servidor manda quadros o tempo todo (o estado
/// de cada pessoa, a telemetria), e um prazo por leitura nunca vence enquanto
/// eles chegam: a primeira versão tinha só esse, e a reversão «sem traduzir o
/// evento» **pendurou** o teste em vez de reprová-lo (achado da orquestradora,
/// 09/10). Um teste que trava em vez de reprovar é ele mesmo um defeito: quem
/// o roda não sabe se esperou o bastante, e uma bateria inteira fica presa
/// atrás dele.
const PRAZO: Duration = Duration::from_secs(15);

/// Uma pessoa conectada, e cada `TemaMudou` que ela já leu, na ordem.
///
/// Os temas são guardados por **toda** leitura, e não só pela que os procura:
/// a resposta de um pedido e o `TemaMudou` que ele causou chegam por caminhos
/// diferentes (a resposta pela conexão que pediu, o tema pelo barramento), e a
/// ordem entre os dois não é contrato. Um ajudante que descartasse o que não
/// procura perderia o tema que chegou antes da resposta.
struct Pessoa {
    cliente: Client,
    temas: Vec<Option<TemaDoServidor>>,
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
            temas: Vec::new(),
            pedidos: 0,
        })
    }

    /// O tema com que se chega: o quadro **logo depois** do `Session` e do
    /// `Instancia`, antes de qualquer outra mensagem da sessão — é o que deixa
    /// a casca aplicá-lo antes de desenhar a sessão.
    async fn tema_da_chegada(&mut self) -> Result<Option<TemaDoServidor>> {
        let fim = tokio::time::Instant::now() + PRAZO;
        let primeiro = self.ler(fim, "o Instancia da chegada").await?;
        assert!(
            matches!(primeiro, ServerMessage::Instancia { .. }),
            "o primeiro quadro depois do Session não é o Instancia: {primeiro:?}"
        );
        match self.ler(fim, "o TemaMudou da chegada").await? {
            ServerMessage::TemaMudou { tema } => Ok(tema),
            outro => panic!(
                "o quadro logo depois do Instancia tem de ser o TemaMudou, inclusive sem \
                 tema; chegou {outro:?}"
            ),
        }
    }

    /// O próximo quadro, se ele chegar antes de `fim`; senão, a falha diz o que
    /// se esperava e os temas vistos até ali.
    async fn ler(&mut self, fim: tokio::time::Instant, o_que: &str) -> Result<ServerMessage> {
        let quadro = tokio::time::timeout_at(fim, self.cliente.next_event())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "esperou {o_que} por {} s e não veio; os TemaMudou vistos até aqui: {:?}",
                    PRAZO.as_secs(),
                    self.temas
                )
            })??;
        if let ServerMessage::TemaMudou { tema } = &quadro {
            self.temas.push(tema.clone());
        }
        Ok(quadro)
    }

    /// Pede ao MOD `id` que declare `tema`, no escopo do servidor (canal zero),
    /// e devolve o que `mundo.tema` respondeu.
    async fn declarar(&mut self, id: &str, tema: serde_json::Value) -> Result<serde_json::Value> {
        self.pedir(id, serde_json::json!({ "tema": tema })).await
    }

    /// O pedido inteiro, como a janela o mandaria, e a resposta do MOD.
    async fn pedir(&mut self, id: &str, corpo: serde_json::Value) -> Result<serde_json::Value> {
        self.pedidos += 1;
        let pedido = self.pedidos;
        self.cliente
            .mod_request(pedido, id.to_owned(), ChannelId(0), corpo.to_string())
            .await?;
        let fim = tokio::time::Instant::now() + PRAZO;
        loop {
            if let ServerMessage::ModReply {
                request,
                part,
                total,
                payload,
            } = self.ler(fim, "a resposta do pedido").await?
            {
                if request == pedido {
                    assert_eq!((part, total), (0, 1), "a resposta do MOD de tema é pequena");
                    return Ok(serde_json::from_str(&payload)?);
                }
            }
        }
    }

    /// Lê até ter visto `quantos` `TemaMudou` desde a chegada.
    async fn esperar_temas(&mut self, quantos: usize) -> Result<&[Option<TemaDoServidor>]> {
        let fim = tokio::time::Instant::now() + PRAZO;
        let o_que = format!("{quantos} TemaMudou desde a chegada");
        while self.temas.len() < quantos {
            self.ler(fim, &o_que).await?;
        }
        Ok(&self.temas)
    }

    /// Lê até o servidor encerrar a sessão, e devolve o motivo.
    async fn esperar_a_despedida(&mut self) -> Result<DisconnectReason> {
        let fim = tokio::time::Instant::now() + PRAZO;
        loop {
            if let ServerMessage::Disconnecting { reason } = self.ler(fim, "a despedida").await? {
                return Ok(reason);
            }
        }
    }
}

/// **17b.** O tema declarado chega a todos, e quem entra depois de um reinício
/// recebe o guardado logo depois do `Session`.
#[tokio::test(flavor = "multi_thread")]
async fn o_tema_declarado_chega_a_todos_e_sobrevive_ao_reinicio() -> Result<()> {
    let _vaga = vaga::minha();
    let raiz = temporario("reinicio")?;
    let hash = instalar(&raiz, "seele/estilo", true)?;
    let (daemon, endereco, tarefa) = subir(&raiz).await?;
    let conjunto = habilitar(&daemon, &[("seele/estilo", &hash)]).await?;

    let mut quem_publica = Pessoa::entrar(endereco, 211, &conjunto).await?;
    let mut quem_ve = Pessoa::entrar(endereco, 212, &conjunto).await?;
    // Sem tema, o quadro sai assim mesmo: é ele que diz à casca que o estado do
    // tema é conhecido.
    assert_eq!(quem_publica.tema_da_chegada().await?, None);
    assert_eq!(quem_ve.tema_da_chegada().await?, None);

    let resposta = quem_publica
        .declarar("seele/estilo", serde_json::from_str(CLARO)?)
        .await?;
    assert_eq!(resposta, serde_json::json!({ "ok": true }));
    // A todos, inclusive a quem publicou: a janela dele também desenha a sessão.
    assert_eq!(
        quem_publica.esperar_temas(2).await?,
        &[None, tema("seele/estilo", CLARO)]
    );
    assert_eq!(
        quem_ve.esperar_temas(2).await?,
        &[None, tema("seele/estilo", CLARO)]
    );

    // O reinício: outro `Daemon` sobre o mesmo banco.
    drop((quem_publica, quem_ve));
    derrubar(daemon, tarefa).await;
    let (daemon, endereco, tarefa) = subir(&raiz).await?;
    let mut quem_chega = Pessoa::entrar(endereco, 213, &conjunto).await?;
    assert_eq!(
        quem_chega.tema_da_chegada().await?,
        tema("seele/estilo", CLARO),
        "o tema declarado não sobreviveu ao reinício do servidor"
    );

    derrubar(daemon, tarefa).await;
    std::fs::remove_dir_all(raiz)?;
    Ok(())
}

/// **17c.** Desligar o MOD tira o tema de todos; religá-lo devolve o guardado.
///
/// Desligar muda o conjunto exigido, e a sessão de quem está dentro acaba
/// (`ModsMudaram`, ADR 0045): o `None` chega na reconexão, logo depois do
/// `Session`, como chega a quem entra.
#[tokio::test(flavor = "multi_thread")]
async fn desligar_o_mod_tira_o_tema_e_religar_devolve_o_guardado() -> Result<()> {
    let _vaga = vaga::minha();
    let raiz = temporario("desligar")?;
    let hash = instalar(&raiz, "seele/estilo", true)?;
    let (daemon, endereco, tarefa) = subir(&raiz).await?;
    let com_o_mod = habilitar(&daemon, &[("seele/estilo", &hash)]).await?;

    let mut pessoa = Pessoa::entrar(endereco, 221, &com_o_mod).await?;
    assert_eq!(pessoa.tema_da_chegada().await?, None);
    pessoa
        .declarar("seele/estilo", serde_json::from_str(CLARO)?)
        .await?;
    pessoa.esperar_temas(2).await?;

    let sem_o_mod = {
        let banco = daemon.server().persistence.lock().await;
        disable(&banco, "seele/estilo")?;
        seele_server::mods::anuncio::conjunto_exigido(&banco)?.identidade
    };
    assert_eq!(
        pessoa.esperar_a_despedida().await?,
        DisconnectReason::ModsMudaram
    );
    let mut pessoa = Pessoa::entrar(endereco, 221, &sem_o_mod).await?;
    assert_eq!(
        pessoa.tema_da_chegada().await?,
        None,
        "o MOD foi desligado e o tema dele continuou chegando"
    );

    let de_novo = habilitar(&daemon, &[("seele/estilo", &hash)]).await?;
    assert_eq!(de_novo, com_o_mod, "religar o mesmo MOD é o mesmo conjunto");
    assert_eq!(
        pessoa.esperar_a_despedida().await?,
        DisconnectReason::ModsMudaram
    );
    let mut pessoa = Pessoa::entrar(endereco, 221, &de_novo).await?;
    assert_eq!(
        pessoa.tema_da_chegada().await?,
        tema("seele/estilo", CLARO),
        "religar o MOD não devolveu o tema guardado"
    );

    derrubar(daemon, tarefa).await;
    std::fs::remove_dir_all(raiz)?;
    Ok(())
}

/// **17a e 17c.** A porta é o manifesto, e o servidor tem um tema só.
///
/// O MOD sem `"tema": true` e o segundo MOD com a porta recebem a recusa pelo
/// nome. A ausência de `TemaMudou` por causa deles é provada pela ordem do
/// barramento: o escolhido declara de novo depois das duas recusas, e o
/// segundo `TemaMudou` que chega é o dele.
#[tokio::test(flavor = "multi_thread")]
async fn so_o_escolhido_pinta_e_quem_nao_tem_a_porta_e_recusado() -> Result<()> {
    let _vaga = vaga::minha();
    let raiz = temporario("um-so")?;
    let estilo = instalar(&raiz, "seele/estilo", true)?;
    let outro = instalar(&raiz, "outro/tema", true)?;
    let perfis = instalar(&raiz, "seele/perfis", false)?;
    let (daemon, endereco, tarefa) = subir(&raiz).await?;
    let conjunto = habilitar(
        &daemon,
        &[
            ("seele/estilo", &estilo),
            ("outro/tema", &outro),
            ("seele/perfis", &perfis),
        ],
    )
    .await?;

    let mut pessoa = Pessoa::entrar(endereco, 231, &conjunto).await?;
    assert_eq!(pessoa.tema_da_chegada().await?, None);

    assert_eq!(
        pessoa
            .declarar("seele/perfis", serde_json::from_str(CLARO)?)
            .await?,
        serde_json::json!({ "ok": false, "erro": "sem-porta-de-tema" })
    );
    assert_eq!(
        pessoa
            .declarar("seele/estilo", serde_json::from_str(CLARO)?)
            .await?,
        serde_json::json!({ "ok": true })
    );
    assert_eq!(
        pessoa
            .declarar("outro/tema", serde_json::from_str(ESCURO)?)
            .await?,
        serde_json::json!({ "ok": false, "erro": "tema-de-outro-mod" })
    );
    assert_eq!(
        pessoa
            .declarar("seele/estilo", serde_json::from_str(ESCURO)?)
            .await?,
        serde_json::json!({ "ok": true })
    );
    assert_eq!(
        pessoa.esperar_temas(3).await?,
        &[
            None,
            tema("seele/estilo", CLARO),
            tema("seele/estilo", ESCURO)
        ],
        "um TemaMudou saiu por causa de uma declaração recusada"
    );

    derrubar(daemon, tarefa).await;
    std::fs::remove_dir_all(raiz)?;
    Ok(())
}

/// **O `TemaMudou` só sai do pedido que gravou, e qualquer pessoa pode pedir**
/// (`sentido.mundo.tema`: «só vale se a chamada terminar», «o `TemaMudou` sai
/// depois do quintal e do tema gravados», «o servidor não confere quem
/// pediu»; a auditoria do `v6.json`, tabela 2). O par do aviso existia
/// (`um_aviso_de_um_pedido_que_nao_gravou_nao_sai`), e o do tema não: nenhum
/// teste combinava `mundo.tema` com um quintal que não grava.
///
/// Uma Pessoa comum — a segunda a entrar; a primeira tem o comando — declara o
/// tema claro num pedido cujo quintal passa do teto: o pedido falha, e o tema
/// não sai. O seguinte, escuro, grava: o segundo `TemaMudou` que chega a quem
/// tem o comando é o dele, e não o claro.
#[tokio::test(flavor = "multi_thread")]
async fn o_tema_de_um_pedido_que_nao_gravou_nao_sai_e_qualquer_pessoa_pede() -> Result<()> {
    let _vaga = vaga::minha();
    let raiz = temporario("quintal")?;
    let hash = instalar(&raiz, "seele/estilo", true)?;
    let (daemon, endereco, tarefa) = subir(&raiz).await?;
    let conjunto = habilitar(&daemon, &[("seele/estilo", &hash)]).await?;

    let mut comando = Pessoa::entrar(endereco, 251, &conjunto).await?;
    let mut comum = Pessoa::entrar(endereco, 252, &conjunto).await?;
    assert_eq!(comando.tema_da_chegada().await?, None);
    assert_eq!(comum.tema_da_chegada().await?, None);

    let recusado = comum
        .pedir(
            "seele/estilo",
            serde_json::json!({ "tema": serde_json::from_str::<serde_json::Value>(CLARO)?, "encher": true }),
        )
        .await?;
    assert_eq!(
        recusado["ok"], false,
        "o quintal grande demais foi gravado: {recusado}"
    );
    assert_eq!(
        comum
            .declarar("seele/estilo", serde_json::from_str(ESCURO)?)
            .await?,
        serde_json::json!({ "ok": true }),
        "uma Pessoa comum não pôde declarar o tema, e o servidor não confere quem pediu"
    );
    assert_eq!(
        comando.esperar_temas(2).await?,
        &[None, tema("seele/estilo", ESCURO)],
        "o tema de um pedido cujo quintal foi recusado saiu"
    );

    derrubar(daemon, tarefa).await;
    std::fs::remove_dir_all(raiz)?;
    Ok(())
}
