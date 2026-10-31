//! **Um Minecraft Vanilla de verdade, pelo túnel de verdade** (API 8 parte 4,
//! «Como provar que funciona»), numa máquina só: o `programa` do núcleo baixa
//! o `server.jar` (sha1) e o Java da Temurin (sha256) da oferta do MOD de
//! multiplayer, abre o pacote, escreve o `eula.txt` e o `server.properties`,
//! roda o Java e lê o «Done (»; o servidor SEELE de verdade, com a máquina e
//! quem joga como `Enlace` de verdade e o aceite pelo caminho do produto, serve
//! o túnel; e o status do jogo (handshake + status request, o que a lista de
//! servidores do cliente faz) vai **pela porta local de quem joga**, pelo
//! caminho direto e pela estrela, e volta com o motd.
//!
//! O que isto **não** mede: duas casas de verdade (aqui as duas pontas estão
//! em `127.0.0.1`, e o «direto» é um par WebRTC na mesma máquina), um cliente
//! do Minecraft entrando no mundo (o login pede uma conta da Microsoft), e a
//! metade de máquina do MOD (essa é a prova do app,
//! `minecraft_de_verdade_pela_metade_de_maquina`).
//!
//! Rede de verdade e ~105 MB, então fora da suíte. Para rodar:
//!
//! ```text
//! SEELE_MINECRAFT_DE_VERDADE=<plano.json do MOD> \
//!   cargo test -p seele-conformance --test minecraft_pelo_tunel -- --ignored --nocapture
//! ```
//!
//! O `plano.json` é a oferta e o plano que a metade de servidor do MOD monta:
//! `node ferramentas/plano-de-verdade.cjs plano.json`, no repositório do MOD.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::programa::{
    EventoDoProcesso, HashDeclarado, PedidoDeDesempacotar, PedidoDeDownload, PedidoDeExecucao,
    Programa,
};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::tunel::ProtocoloDeTunel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod maquina_de_jogo;
use maquina_de_jogo::{aceitar_como_maquina, servidor, MOD, SERVIDOR_DE_JOGO};

const TUNEL: &str = "minecraft-1";

async fn enlace(
    endereco: SocketAddr,
    conjunto: &str,
    semente: u8,
    apelido: &str,
) -> Result<Enlace> {
    Ok(Enlace::conectar_entre(
        vec![Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: apelido.to_owned(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: Some(conjunto.to_owned()),
        }],
        SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::default()) as Arc<dyn PinStore>,
    )
    .await?)
}

fn varint(mut v: u32, saida: &mut Vec<u8>) {
    loop {
        let b = u8::try_from(v & 0x7f).unwrap();
        v >>= 7;
        if v == 0 {
            saida.push(b);
            return;
        }
        saida.push(b | 0x80);
    }
}

async fn ler_varint(r: &mut tokio::net::TcpStream) -> Result<u32> {
    let mut v = 0_u32;
    for i in 0..5 {
        let b = r.read_u8().await?;
        v |= u32::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    anyhow::bail!("varint longo demais")
}

/// O status do Minecraft Java em `127.0.0.1:porta`, e quanto ele levou.
async fn status(porta: u16) -> Result<(String, Duration)> {
    let relogio = Instant::now();
    let mut fluxo = tokio::net::TcpStream::connect(("127.0.0.1", porta)).await?;
    let mut corpo = vec![0x00];
    varint(769, &mut corpo); // 1.21.4
    varint(9, &mut corpo);
    corpo.extend_from_slice(b"127.0.0.1");
    corpo.extend_from_slice(&porta.to_be_bytes());
    varint(1, &mut corpo);
    let mut pacote = Vec::new();
    varint(u32::try_from(corpo.len())?, &mut pacote);
    pacote.extend_from_slice(&corpo);
    pacote.extend_from_slice(&[0x01, 0x00]);
    fluxo.write_all(&pacote).await?;
    let lido = tokio::time::timeout(Duration::from_secs(20), async {
        let _tamanho = ler_varint(&mut fluxo).await?;
        let _id = ler_varint(&mut fluxo).await?;
        let n = ler_varint(&mut fluxo).await? as usize;
        let mut json = vec![0_u8; n];
        fluxo.read_exact(&mut json).await?;
        anyhow::Ok(String::from_utf8_lossy(&json).into_owned())
    })
    .await??;
    Ok((lido, relogio.elapsed()))
}

/// O jogo de pé pelo `programa`: baixa, abre, escreve, roda e espera o «Done».
async fn o_jogo_de_pe(
    plano: &serde_json::Value,
    porta: u16,
    pasta: &std::path::Path,
) -> Result<(Programa, seele_core::programa::Processo, String)> {
    let oferta: seele_ffi::OfertaDeMaquinaFfi = serde_json::from_value(plano["oferta"].clone())?;
    let oferta = oferta
        .para_o_nucleo()
        .map_err(|e| anyhow::anyhow!("a oferta do MOD não passa: {e}"))?;
    let plataforma =
        seele_core::programa::plataforma_desta_maquina().expect("um sistema de receita");
    let raiz = seele_core::aceites_de_maquina::pasta_do_servidor_de_jogo(
        pasta,
        "casa:8383",
        "seele/multiplayer",
        TUNEL,
    );
    let politica = seele_core::programa::politica_do_aceite(&oferta, raiz, plataforma);
    let origem: Arc<dyn seele_core::programa::Origem> =
        Arc::new(seele_core::programa::OrigemHttps::nova().map_err(anyhow::Error::msg)?);
    let programa = Programa::novo(politica, origem)?;
    eprintln!("pasta: {}", programa.raiz().display());

    let artefatos = plano["plano"]["artefatos"].as_array().unwrap();
    let java = plano["plano"]["porPlataforma"][plataforma]
        .as_str()
        .unwrap();
    let mut relatorio = Vec::new();
    for a in artefatos
        .iter()
        .filter(|a| a["id"] != "java" || a["nome"] == java)
    {
        let relogio = Instant::now();
        let algoritmo = match a["hash"]["algoritmo"].as_str().unwrap() {
            "sha1" => seele_core::programa::AlgoritmoDeHash::Sha1,
            "sha256" => seele_core::programa::AlgoritmoDeHash::Sha256,
            _ => seele_core::programa::AlgoritmoDeHash::Sha512,
        };
        let pacote = a.get("desempacotar").filter(|d| !d.is_null());
        let destino = if pacote.is_some() {
            format!("pacotes/{}", a["nome"].as_str().unwrap())
        } else {
            a["destino"].as_str().unwrap().to_owned()
        };
        let feito = programa
            .baixar(PedidoDeDownload {
                url: a["url"].as_str().unwrap().to_owned(),
                hashes: vec![HashDeclarado {
                    algoritmo,
                    hex: a["hash"]["hex"].as_str().unwrap().to_owned(),
                }],
                tamanho: a["tamanho"].as_u64().unwrap(),
                destino: destino.clone(),
            })?
            .terminar()
            .await?;
        relatorio.push(format!(
            "{} {} bytes ({}) em {:?}",
            a["nome"],
            feito.tamanho,
            feito.algoritmo,
            relogio.elapsed()
        ));
        if let Some(d) = pacote {
            let aberto = programa.desempacotar(&PedidoDeDesempacotar {
                arquivo: destino,
                destino: a["destino"].as_str().unwrap().to_owned(),
                raiz: d["raiz"].as_str().unwrap().to_owned(),
            })?;
            relatorio.push(format!(
                "aberto: {} arquivos, {} bytes, {} links pulados",
                aberto.arquivos, aberto.bytes, aberto.links_pulados
            ));
        }
    }
    programa.escrever("eula.txt", "eula=true\n")?;
    programa.escrever(
        "server.properties",
        &format!("server-ip=127.0.0.1\nserver-port={porta}\nmotd=\\u00a7bCasa \\u00a76pelo t\\u00fanel\nonline-mode=true\nmax-players=4\n"),
    )?;
    let rodar = &plano["oferta"]["rodar"];
    let mut processo = programa.rodar(&PedidoDeExecucao {
        comando: rodar["comando"].as_str().unwrap().to_owned(),
        argumentos: rodar["argumentos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_owned())
            .collect(),
        pasta: String::new(),
    })?;
    let relogio = Instant::now();
    loop {
        let evento = tokio::time::timeout(Duration::from_secs(300), processo.proximo())
            .await?
            .ok_or_else(|| anyhow::anyhow!("o jogo saiu antes do Done"))?;
        match evento {
            EventoDoProcesso::Saida(linha) | EventoDoProcesso::Erro(linha) => {
                if linha.contains("]: Done (") {
                    relatorio.push(format!("Done em {:?}: {linha}", relogio.elapsed()));
                    break;
                }
            }
            EventoDoProcesso::Terminou(fim) => {
                anyhow::bail!("o jogo terminou antes do Done: {fim:?}")
            }
        }
    }
    Ok((programa, processo, relatorio.join("\n")))
}

/// Um caminho do túnel: o servidor SEELE, o aceite, servir, entrar, e o status
/// pela porta de quem joga.
async fn pelo_tunel(malha: bool, jogo: u16) -> Result<String> {
    let (endereco, daemon, conjunto, _temp) = servidor(malha, None).await?;
    let mut maquina = enlace(endereco, &conjunto, 1, "maquina").await?;
    let mut joga = enlace(endereco, &conjunto, 2, "joga").await?;
    aceitar_como_maquina(
        &daemon,
        &mut maquina,
        MOD,
        SERVIDOR_DE_JOGO,
        &[(TUNEL, ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
    maquina
        .servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("servir: {r:?}"))?;
    let porta = joga
        .entrar_no_tunel(MOD, TUNEL)
        .await
        .map_err(|r| anyhow::anyhow!("entrar: {r:?}"))?
        .porta;
    // Pela malha, o par direto leva um instante para abrir: espera o
    // relatório dizer o caminho antes de medir.
    let mut caminho = None;
    let fim = Instant::now() + Duration::from_secs(20);
    while Instant::now() < fim
        && (caminho.is_none() || (malha && !format!("{caminho:?}").contains("Direto")))
    {
        if let Ok(Aviso::Tunel(r)) =
            tokio::time::timeout(Duration::from_millis(500), joga.proximo()).await
        {
            if r.nome == TUNEL {
                caminho = Some(r.caminho);
            }
        }
    }
    let mut medidas = Vec::new();
    for _ in 0..3 {
        medidas.push(status(porta).await?);
    }
    let (json, _) = &medidas[0];
    anyhow::ensure!(
        json.contains("pelo t"),
        "o motd não veio pelo túnel: {json}"
    );
    let linha = format!(
        "malha={malha}: caminho {caminho:?}, porta local {porta}, status em {:?}: {json}",
        medidas.iter().map(|(_, t)| *t).collect::<Vec<_>>()
    );
    daemon.shutdown();
    Ok(linha)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "rede de verdade e ~105 MB: SEELE_MINECRAFT_DE_VERDADE=<plano.json>"]
async fn o_status_do_minecraft_volta_pelo_tunel() -> Result<()> {
    let Ok(caminho) = std::env::var("SEELE_MINECRAFT_DE_VERDADE") else {
        eprintln!("pulado: sem SEELE_MINECRAFT_DE_VERDADE");
        return Ok(());
    };
    let plano: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(caminho)?)?;
    let jogo = {
        let livre = std::net::TcpListener::bind("127.0.0.1:0")?;
        livre.local_addr()?.port()
    };
    let pasta = tempfile::tempdir()?;
    let (programa, _processo, relatorio) = o_jogo_de_pe(&plano, jogo, pasta.path()).await?;
    eprintln!("{relatorio}");
    let direto = pelo_tunel(true, jogo).await;
    let estrela = pelo_tunel(false, jogo).await;
    let relogio = Instant::now();
    let mortos = programa.encerrar_tudo().await;
    eprintln!(
        "encerrar_tudo: {mortos} processo(s) em {:?}",
        relogio.elapsed()
    );
    eprintln!(
        "{}",
        direto
            .as_ref()
            .map_or_else(|e| format!("malha=true FALHOU: {e:#}"), String::clone)
    );
    eprintln!(
        "{}",
        estrela
            .as_ref()
            .map_or_else(|e| format!("malha=false FALHOU: {e:#}"), String::clone)
    );
    direto?;
    estrela?;
    anyhow::ensure!(
        tokio::net::TcpStream::connect(("127.0.0.1", jogo))
            .await
            .is_err(),
        "o jogo seguiu escutando depois de encerrar_tudo"
    );
    Ok(())
}
