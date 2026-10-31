//! **A máquina de jogo aceita, para as provas que servem túneis** (API 8,
//! parte 3): o servidor com o MOD `x/jogo` e a metade de servidor dele, e o
//! caminho inteiro do aceite pelo `Enlace` de verdade.
//!
//! # Por que existe
//!
//! Até a parte 3 servir um túnel dependia de um interruptor do servidor, e as
//! provas do túnel o ligavam. O interruptor saiu: só serve a máquina que a
//! pessoa aceitou, e só o que estava na oferta. Toda prova que serve um túnel
//! passa agora pelo aceite, e passa por ele **como o produto passa** — a
//! metade de servidor do MOD pede (`mundo.maquinas.pedir`), o núcleo mostra o
//! pedido (`Aviso::PedidoDeMaquina`) e a resposta leva o resumo que ele
//! calculou. Um atalho pelo livro do servidor provaria o túnel e deixaria o
//! pedido sem prova de ponta a ponta.
//!
//! Três arquivos o usam (`aceite_da_maquina.rs`, `tunel_de_jogo.rs`,
//! `tunel_pela_ffi.rs`). Não é um binário de teste: quem usa faz
//! `mod maquina_de_jogo;`.

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "um módulo de teste dividido por arquivos que usam partes diferentes dele"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_core::enlace::{Aviso, Enlace};
use seele_proto::control::ServerMessage;
use seele_proto::ids::ChannelId;
use seele_proto::tunel::{ProtocoloDeTunel, ACAO_ENTRAR_NO_JOGO};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

/// O MOD de teste: poder `porta`, metade de máquina e a de servidor
/// [`METADE`].
pub(crate) const MOD: &str = "x/jogo";

/// O servidor de jogo cuja máquina as provas aceitam.
pub(crate) const SERVIDOR_DE_JOGO: &str = "eco-1";

/// Larga pelo mesmo motivo da `PACIENCIA` dos arquivos que usam isto: uma
/// máquina carregada é lenta, e um teste que falha por relógio curto ensina a
/// ignorá-lo.
const PACIENCIA: Duration = Duration::from_secs(30);

/// A metade de servidor do MOD de teste.
///
/// `aoPedir` com `pedir`: chama **quem pediu** (`contexto.person`) a rodar a
/// máquina de `servidor`, com uma oferta que lista os túneis do pedido — a
/// porta do eco é sorteada pelo sistema, e por isso vem de quem testa, e não
/// do MOD. Com `ler`: devolve os momentos gravados. Um pedido da própria
/// máquina (`contexto.maquina`) só é respondido.
///
/// `aoAcontecer`: cada momento da máquina vira uma linha em `dados.momentos`,
/// como `MaquinaRecusou {"servidor":…,"papel":…,"pessoa":…,"motivo":…}`. É assim que a
/// prova vê o que **o MOD** soube, e não o que o servidor achou que disse.
pub(crate) const METADE: &str = r#"
globalThis.aoPedir = (contexto, pedido) => {
  const c = JSON.parse(contexto);
  const p = JSON.parse(pedido);
  if (c.maquina) return JSON.stringify({ ok: true });
  if (p.ler) return JSON.stringify({ ok: true, momentos: dados.momentos || "" });
  if (p.pedir) {
    mundo.maquinas.pedir({ quem: c.person, servidor: p.servidor, papel: p.papel, oferta: {
      titulo: "Eco", receita: "eco", versao_da_receita: "1.0.0", versao_do_jogo: "1",
      tuneis: p.tuneis } });
  }
  return JSON.stringify({ ok: true });
};
globalThis.aoAcontecer = (momento, carga) => {
  if (momento.indexOf("Maquina") === 0) dados.momentos = (dados.momentos || "") + momento + " " + carga + "\n";
};
"#;

/// O servidor de teste: onde ele escuta, o daemon, a identidade do conjunto de
/// MODs que quem entra tem de ter aceitado, e o diretório que o sustenta.
pub(crate) type Servidor = (SocketAddr, Arc<Daemon>, String, tempfile::TempDir);

/// Um servidor com o MOD `x/jogo` ligado — poder `porta`, plataforma `app`, a
/// ação `entrar-no-jogo` dada a todos e a metade de servidor [`METADE`] — e,
/// com `proprietario`, o dono declarado por impressão.
///
/// **O MOD é ligado antes de o servidor subir**, num banco em arquivo: o
/// despachante dos momentos nasce no `Daemon::bind` só se há MOD ligado, e sem
/// ele `MaquinaAceitou`/`MaquinaRecusou`/`MaquinaParou` não chegam a MOD
/// nenhum — o pedido funcionaria, e a prova do que o MOD sabe, não.
pub(crate) async fn servidor(
    malha_para_tuneis: bool,
    proprietario: Option<String>,
) -> Result<Servidor> {
    servidor_com(
        malha_para_tuneis,
        proprietario,
        METADE,
        seele_server::PRAZO_DA_MAQUINA_SUMIDA,
    )
    .await
}

/// [`servidor`] com outra metade de servidor e outro prazo da máquina que
/// sumiu (as provas da reserva, API 8 parte 7).
pub(crate) async fn servidor_com(
    malha_para_tuneis: bool,
    proprietario: Option<String>,
    metade: &str,
    prazo_da_maquina_sumida: Duration,
) -> Result<Servidor> {
    let temp = tempfile::tempdir()?;
    let raizes = seele_server::RaizesDosMods {
        pacotes: temp.path().join("cache"),
        dados: temp.path().join("dados"),
    };
    let manifesto = serde_json::json!({
        "schema": 1, "id": MOD, "version": "1.0.0",
        "api": seele_proto::mods::MOD_API_VERSION,
        "repo": "https://github.com/x/jogo", "reach": [],
        "client": "cliente.js",
        "plataformas": ["app"], "poderes": ["porta"],
        // A porta pede a metade de máquina, e ela a de servidor (API 8, parte 3).
        "maquina": "maquina.js", "server": "servidor.js",
        "permissoes": [{"acao": ACAO_ENTRAR_NO_JOGO, "nome": "Entrar no jogo", "todos": true}],
    })
    .to_string()
    .into_bytes();
    let mut arquivos = vec![
        ("mod.json".to_owned(), manifesto),
        ("cliente.js".to_owned(), b"void 0;".to_vec()),
        ("maquina.js".to_owned(), b"void 0;".to_vec()),
        ("servidor.js".to_owned(), metade.as_bytes().to_vec()),
    ];
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let pacote = raizes.pacote_de(&hash);
    std::fs::create_dir_all(&pacote)?;
    for (nome, bytes) in arquivos {
        std::fs::write(pacote.join(nome), bytes)?;
    }
    let banco = Location::File(temp.path().join("seele.db"));
    {
        let db = seele_server::persistence::Persistence::open(&banco)?;
        seele_server::persistence::mods::enable(
            &db,
            &seele_server::persistence::mods::EnabledMod {
                id: MOD.into(),
                version: "1.0.0".into(),
                hash,
                repo: "https://github.com/x/jogo".into(),
                reach: vec![],
                server_half: true,
                api: seele_proto::mods::MOD_API_VERSION,
                tema: false,
                plataformas: Some(vec![seele_proto::mods::Plataforma::App]),
            },
        )?;
        let queixas = seele_server::mods::gravar_permissoes_declaradas(&db, &raizes);
        assert!(queixas.is_empty(), "{queixas:?}");
    }
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: banco,
        mods_dir: Some(raizes),
        malha_para_tuneis,
        proprietario,
        prazo_da_maquina_sumida,
        ..ServerConfig::default()
    };
    // Porta efêmera: não há volta na mesma porta para disputar.
    let daemon = Arc::new(Daemon::bind(config).await?);
    let conjunto = {
        let db = daemon.server().persistence.lock().await;
        seele_server::mods::anuncio::conjunto_exigido(&db)?.identidade
    };
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon, conjunto, temp))
}

/// Os números de `request` destes pedidos ao MOD. Altos, para não cruzar com
/// os que um teste escolha à mão; um contador do binário, porque as provas
/// rodam em paralelo e cada uma tem o seu `Enlace`.
static PROXIMO_PEDIDO: AtomicU32 = AtomicU32::new(10_000);

/// O JSON de um túnel na oferta, como um MOD o escreve.
fn tunel_na_oferta((nome, protocolo, porta): (&str, ProtocoloDeTunel, u16)) -> serde_json::Value {
    let protocolo = match protocolo {
        ProtocoloDeTunel::Tcp => "tcp",
        ProtocoloDeTunel::Udp => "udp",
    };
    serde_json::json!({ "nome": nome, "protocolo": protocolo, "porta": porta })
}

/// Junta uma parte de `ModReply` a `partes`; devolve a resposta inteira
/// quando a última chega.
fn juntar(partes: &mut Vec<(u32, String)>, part: u32, total: u32, payload: &str) -> Option<String> {
    partes.push((part, payload.to_owned()));
    if partes.len() < total as usize {
        return None;
    }
    partes.sort_by_key(|(n, _)| *n);
    Some(partes.iter().map(|(_, p)| p.as_str()).collect())
}

/// Um `ModRequest` de `enlace` ao [`MOD`], e a resposta inteira. Os outros
/// avisos que chegam enquanto isso são descartados: quem chama não está lendo
/// este `Enlace` por outra coisa.
pub(crate) async fn pedir_ao_mod(
    enlace: &mut Enlace,
    payload: &serde_json::Value,
) -> Result<String> {
    let request = PROXIMO_PEDIDO.fetch_add(1, Ordering::Relaxed);
    enlace
        .mod_request(request, MOD.into(), ChannelId(0), payload.to_string())
        .await
        .map_err(|_| anyhow::anyhow!("o enlace já tinha acabado"))?;
    let mut partes = Vec::new();
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        let Ok(aviso) = tokio::time::timeout(Duration::from_millis(500), enlace.proximo()).await
        else {
            continue;
        };
        if let Aviso::Mensagem(m) = aviso {
            if let ServerMessage::ModReply {
                request: r,
                part,
                total,
                payload,
            } = *m
            {
                if r == request {
                    if let Some(inteira) = juntar(&mut partes, part, total, &payload) {
                        return Ok(inteira);
                    }
                }
            }
        }
    }
    anyhow::bail!("sem ModReply para o pedido {request}")
}

/// **O aceite de ponta a ponta, num papel** (API 8 parte 7): como
/// [`aceitar_como_maquina`], com `papel` no pedido (`{ pedir, servidor,
/// papel, tuneis }`, que a metade de servidor de quem usa entende), e a espera
/// pelo livro naquele papel.
pub(crate) async fn aceitar_no_papel(
    server: &Daemon,
    maquina: &mut Enlace,
    servidor: &str,
    papel: seele_server::maquinas::Papel,
    tuneis: &[(&str, ProtocoloDeTunel, u16)],
) -> Result<String> {
    maquina.aceitar_pedidos_de_maquina(true);
    let request = PROXIMO_PEDIDO.fetch_add(1, Ordering::Relaxed);
    let tuneis: Vec<_> = tuneis.iter().copied().map(tunel_na_oferta).collect();
    maquina
        .mod_request(
            request,
            MOD.into(),
            ChannelId(0),
            serde_json::json!({ "pedir": true, "servidor": servidor, "papel": papel.nome(), "tuneis": tuneis })
                .to_string(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("o enlace da máquina já tinha acabado"))?;
    let resumo = esperar_o_pedido(maquina, servidor).await?;
    maquina
        .aceitar_maquina(MOD, servidor, &resumo)
        .await
        .map_err(|_| anyhow::anyhow!("o enlace da máquina já tinha acabado"))?;
    let pessoa = maquina.sessao().person;
    let fim = Instant::now() + PACIENCIA;
    while server
        .server()
        .maquinas()
        .quem_opera_como(MOD, servidor, papel)
        != Some(pessoa)
    {
        anyhow::ensure!(
            Instant::now() < fim,
            "o aceite não entrou no livro ({papel:?})"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(resumo)
}

/// Lê avisos de `maquina` até um `PedidoDeMaquina` de `servidor`, e devolve o
/// resumo. O que mais chegar é descartado.
pub(crate) async fn esperar_o_pedido(maquina: &mut Enlace, servidor: &str) -> Result<String> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        let Ok(aviso) = tokio::time::timeout(Duration::from_millis(500), maquina.proximo()).await
        else {
            continue;
        };
        if let Aviso::PedidoDeMaquina {
            mod_id,
            servidor: s,
            resumo,
            ..
        } = aviso
        {
            if mod_id == MOD && s == servidor {
                return Ok(resumo);
            }
        }
    }
    anyhow::bail!("nenhum PedidoDeMaquina de {servidor} chegou")
}

/// O que a metade de servidor gravou dos momentos da máquina, lido por um
/// pedido ao MOD.
pub(crate) async fn momentos(enlace: &mut Enlace) -> Result<String> {
    let resposta = pedir_ao_mod(enlace, &serde_json::json!({ "ler": true })).await?;
    let valor: serde_json::Value = serde_json::from_str(&resposta)?;
    Ok(valor["momentos"].as_str().unwrap_or_default().to_owned())
}

/// Espera o MOD ter gravado um momento que contém `trecho`, lendo por
/// `enlace`. Devolve tudo o que ele gravou.
pub(crate) async fn esperar_o_momento(enlace: &mut Enlace, trecho: &str) -> Result<String> {
    let fim = Instant::now() + PACIENCIA;
    loop {
        let vistos = momentos(enlace).await?;
        if vistos.contains(trecho) {
            return Ok(vistos);
        }
        anyhow::ensure!(
            Instant::now() < fim,
            "o MOD não gravou `{trecho}`; gravou: {vistos:?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Pede ao MOD, por `maquina`, que chame a própria `maquina` a rodar a máquina
/// de `servidor` com os `tuneis` na oferta — o pedido sai da metade de
/// servidor, por `mundo.maquinas.pedir`. Não espera o `PedidoDeMaquina`.
pub(crate) async fn pedir_a_maquina(
    maquina: &mut Enlace,
    servidor: &str,
    tuneis: &[(&str, ProtocoloDeTunel, u16)],
) -> Result<()> {
    let tuneis: Vec<_> = tuneis.iter().copied().map(tunel_na_oferta).collect();
    let resposta = pedir_ao_mod(
        maquina,
        &serde_json::json!({ "pedir": true, "servidor": servidor, "tuneis": tuneis }),
    )
    .await?;
    anyhow::ensure!(
        resposta == r#"{"ok":true}"#,
        "a metade de servidor respondeu {resposta}"
    );
    Ok(())
}

/// **O aceite de ponta a ponta**: liga a tela deste `Enlace`
/// (`aceitar_pedidos_de_maquina(true)`), leva a metade de servidor a pedir
/// (um `mod_request`), espera o `Aviso::PedidoDeMaquina` e o aceita com o
/// resumo **que veio nele** — o que a tela de aceite do app faz. Volta quando o
/// servidor já tem o aceite no livro, e devolve o resumo.
///
/// A tela é ligada **antes** do pedido: o servidor só reenvia um pendente na
/// chegada da sessão, e um pedido que chegasse com a tela ainda desligada seria
/// recusado `SemTela` pelo núcleo. Nunca por `este_processo_tem_tela`, que
/// mudaria o padrão do binário de teste inteiro.
///
/// `tuneis` é o que a oferta traz (nome, protocolo, porta do jogo): o servidor
/// só deixa servir o que está nela, com a mesma porta.
pub(crate) async fn aceitar_como_maquina(
    server: &Daemon,
    maquina: &mut Enlace,
    mod_id: &str,
    servidor: &str,
    tuneis: &[(&str, ProtocoloDeTunel, u16)],
) -> Result<String> {
    anyhow::ensure!(mod_id == MOD, "a metade de servidor de teste é a de {MOD}");
    maquina.aceitar_pedidos_de_maquina(true);
    // O pedido e a resposta dele podem chegar em qualquer ordem: os dois são
    // lidos no mesmo laço, para nenhum se perder enquanto se espera o outro.
    let request = PROXIMO_PEDIDO.fetch_add(1, Ordering::Relaxed);
    let tuneis: Vec<_> = tuneis.iter().copied().map(tunel_na_oferta).collect();
    maquina
        .mod_request(
            request,
            mod_id.into(),
            ChannelId(0),
            serde_json::json!({ "pedir": true, "servidor": servidor, "tuneis": tuneis })
                .to_string(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("o enlace da máquina já tinha acabado"))?;
    let mut partes = Vec::new();
    let mut respondido = None;
    let mut resumo = None;
    let fim = Instant::now() + PACIENCIA;
    while respondido.is_none() || resumo.is_none() {
        anyhow::ensure!(
            Instant::now() < fim,
            "o aceite não fechou: resposta do MOD {respondido:?}, resumo {resumo:?}"
        );
        let Ok(aviso) = tokio::time::timeout(Duration::from_millis(500), maquina.proximo()).await
        else {
            continue;
        };
        match aviso {
            Aviso::Mensagem(m) => {
                if let ServerMessage::ModReply {
                    request: r,
                    part,
                    total,
                    payload,
                } = *m
                {
                    if r == request {
                        respondido = juntar(&mut partes, part, total, &payload);
                    }
                }
            }
            Aviso::PedidoDeMaquina {
                mod_id: m,
                servidor: s,
                resumo: r,
                ..
            } if m == mod_id && s == servidor => resumo = Some(r),
            _ => {}
        }
    }
    let respondido = respondido.unwrap();
    anyhow::ensure!(
        respondido == r#"{"ok":true}"#,
        "a metade de servidor respondeu {respondido}"
    );
    let resumo = resumo.unwrap();
    maquina
        .aceitar_maquina(mod_id, servidor, &resumo)
        .await
        .map_err(|_| anyhow::anyhow!("o enlace da máquina já tinha acabado"))?;
    // O aceite não tem resposta no fio: espera o livro, para que um `servir`
    // logo depois não dispute com ele.
    let pessoa = maquina.sessao().person;
    while server.server().maquinas().quem_opera(mod_id, servidor) != Some(pessoa) {
        anyhow::ensure!(
            Instant::now() < fim,
            "o aceite não entrou no livro do servidor"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(resumo)
}
