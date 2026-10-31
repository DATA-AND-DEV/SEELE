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

/// **O par de comandos desta máquina** (decisão W1 do Windows): o da entrada
/// de `por_sistema` deste sistema, ou o de cima — o que a política aceita, pela
/// mesma `comandos_para`. `(rodar, instalar)`, como os pedidos do `programa`.
fn par_desta_maquina(
    plano: &serde_json::Value,
) -> (Option<PedidoDeExecucao>, Option<PedidoDeExecucao>) {
    let oferta: seele_ffi::OfertaDeMaquinaFfi =
        serde_json::from_value(plano["oferta"].clone()).expect("a oferta do plano");
    let oferta = oferta.para_o_nucleo().expect("a oferta do MOD passa");
    let plataforma =
        seele_core::programa::plataforma_desta_maquina().expect("um sistema de receita");
    let c = seele_core::maquina::comandos_para(&oferta, plataforma);
    let pedido = |r: &seele_core::maquina::ComandoOferecido| PedidoDeExecucao {
        comando: r.comando.clone(),
        argumentos: r.argumentos.clone(),
        pasta: String::new(),
    };
    (c.rodar.map(pedido), c.instalar.map(pedido))
}

/// O jogo de pé pelo `programa`: baixa, abre, escreve, roda e espera o «Done».
async fn o_jogo_de_pe(
    plano: &serde_json::Value,
    porta: u16,
    pasta: &std::path::Path,
) -> Result<(Programa, seele_core::programa::Processo, String)> {
    let (programa, mut relatorio) = o_jogo_baixado(plano, porta, pasta, None).await?;
    let processo = rodar_ate_o_done(&programa, plano, &mut relatorio).await?;
    Ok((programa, processo, relatorio.join("\n")))
}

/// O jogo baixado, aberto e configurado, sem rodar. `copia` é a porta em que
/// esta máquina pode oferecer a cópia do mundo (a reserva, parte 7).
async fn o_jogo_baixado(
    plano: &serde_json::Value,
    porta: u16,
    pasta: &std::path::Path,
    copia: Option<u16>,
) -> Result<(Programa, Vec<String>)> {
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
    let mut politica = seele_core::programa::politica_do_aceite(&oferta, raiz, plataforma);
    politica.portas_de_copia.extend(copia);
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
                mesclar: false,
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
    // **Com mods** (parte 6): o pack de prova (`ferramentas/pack-de-prova.cjs`
    // do MOD, Fabric 1.21.4), trazido como a máquina o traz (só com o hash do
    // aceite), aberto, o índice lido, cada arquivo do servidor baixado com o
    // sha512 (a segunda porta, pelas origens), os overrides e os
    // server-overrides por cima, e o instalador do Fabric pelo `instalar`.
    if let Some(pack) = plano["pack_local"].as_str() {
        let relogio = Instant::now();
        let trazido = programa.trazer("pack.mrpack", &std::fs::read(pack)?)?;
        programa.desempacotar(&PedidoDeDesempacotar {
            mesclar: false,
            arquivo: "pack.mrpack".into(),
            destino: "pack".into(),
            raiz: String::new(),
        })?;
        let indice: serde_json::Value =
            serde_json::from_str(&programa.ler("pack/modrinth.index.json")?)?;
        let mut baixados = Vec::new();
        for f in indice["files"].as_array().unwrap() {
            if f["env"]["server"] == "unsupported" {
                continue;
            }
            let feito = programa
                .baixar(PedidoDeDownload {
                    url: f["downloads"][0].as_str().unwrap().to_owned(),
                    hashes: vec![HashDeclarado {
                        algoritmo: seele_core::programa::AlgoritmoDeHash::Sha512,
                        hex: f["hashes"]["sha512"].as_str().unwrap().to_owned(),
                    }],
                    tamanho: f["fileSize"].as_u64().unwrap(),
                    destino: f["path"].as_str().unwrap().to_owned(),
                })?
                .terminar()
                .await?;
            baixados.push(format!("{} ({} bytes)", f["path"], feito.tamanho));
        }
        for raiz in ["overrides", "server-overrides"] {
            // Um pack sem a pasta não tem o que aplicar (como o MOD:
            // `nada-sob-a-raiz` não é falha) — o Create Live 5 e o Create
            // OneBlock não trazem server-overrides.
            match programa.desempacotar(&PedidoDeDesempacotar {
                mesclar: true,
                arquivo: "pack.mrpack".into(),
                destino: String::new(),
                raiz: raiz.into(),
            }) {
                Ok(_) | Err(seele_core::programa::Recusa::NadaSobARaiz { .. }) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let instalar = par_desta_maquina(plano)
            .1
            .ok_or_else(|| anyhow::anyhow!("a oferta com pack não tem instalar aqui"))?;
        relatorio.push(format!(
            "instalar: {} {}",
            instalar.comando,
            instalar.argumentos.join(" ")
        ));
        let mut instalador = programa.rodar(&instalar)?;
        let fim = loop {
            match tokio::time::timeout(Duration::from_secs(300), instalador.proximo()).await? {
                Some(EventoDoProcesso::Terminou(fim)) => break fim,
                Some(_) => {}
                None => anyhow::bail!("o instalador sumiu sem fim"),
            }
        };
        anyhow::ensure!(fim.codigo == Some(0), "o instalador saiu com {fim:?}");
        relatorio.push(format!(
            "pack: trazido {} bytes (sha512 {}…), {} arquivo(s) do servidor: {}; overrides e server-overrides; instalador {:?} em {:?}",
            trazido.tamanho,
            &trazido.hash[..16],
            baixados.len(),
            baixados.join(", "),
            fim.codigo,
            relogio.elapsed()
        ));
    }
    programa.escrever("eula.txt", "eula=true\n")?;
    let propriedades = format!("server-ip=127.0.0.1\nserver-port={porta}\nmotd=\\u00a7bCasa \\u00a76pelo t\\u00fanel\nonline-mode=true\nmax-players=4\n");
    programa.escrever("server.properties", &propriedades)?;
    // Como a metade de máquina do MOD (`configuracoesDoPack`): um pack com o
    // mod Default Server Properties (o Create Live 5, Forge) troca o
    // server.properties pelo `default-server.properties` dele na primeira
    // subida; as chaves desta prova vão nele também (a última vale no Java).
    if let Ok(deles) = programa.ler("default-server.properties") {
        programa.escrever(
            "default-server.properties",
            &format!("{deles}\n{propriedades}"),
        )?;
        relatorio.push("default-server.properties do pack: as chaves da prova por cima".into());
    }
    Ok((programa, relatorio))
}

/// Roda o jogo baixado e espera o «Done».
async fn rodar_ate_o_done(
    programa: &Programa,
    plano: &serde_json::Value,
    relatorio: &mut Vec<String>,
) -> Result<seele_core::programa::Processo> {
    let rodar = par_desta_maquina(plano)
        .0
        .ok_or_else(|| anyhow::anyhow!("a oferta não tem rodar aqui"))?;
    relatorio.push(format!(
        "rodar: {} {}",
        rodar.comando,
        rodar.argumentos.join(" ")
    ));
    let mut processo = programa.rodar(&rodar)?;
    let relogio = Instant::now();
    loop {
        let evento = tokio::time::timeout(Duration::from_secs(300), processo.proximo())
            .await?
            .ok_or_else(|| anyhow::anyhow!("o jogo saiu antes do Done"))?;
        match evento {
            EventoDoProcesso::Saida(linha) | EventoDoProcesso::Erro(linha) => {
                if linha.contains("Loading") && linha.contains("mods") {
                    relatorio.push(format!("o jogo: {linha}"));
                }
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
    Ok(processo)
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

/// Lê a saída do jogo até uma linha que contenha `trecho`, e a devolve.
async fn esperar_linha(
    processo: &mut seele_core::programa::Processo,
    trecho: &str,
    prazo: Duration,
) -> Result<String> {
    let fim = Instant::now() + prazo;
    loop {
        let falta = fim.saturating_duration_since(Instant::now());
        anyhow::ensure!(!falta.is_zero(), "o jogo não disse «{trecho}»");
        match tokio::time::timeout(falta, processo.proximo()).await? {
            Some(EventoDoProcesso::Saida(linha) | EventoDoProcesso::Erro(linha)) => {
                if linha.contains(trecho) {
                    return Ok(linha);
                }
            }
            Some(EventoDoProcesso::Terminou(fim)) => anyhow::bail!("o jogo saiu: {fim:?}"),
            None => anyhow::bail!("o jogo sumiu"),
        }
    }
}

/// **A reserva de verdade** (API 8 parte 7): um Minecraft 1.21.4 na
/// principal, um placar gravado no mundo, o salvamento seguro da receita
/// (`save-off`, `save-all flush` até «Saved the game», o pacote, `save-on`),
/// a cópia pelo túnel de verdade (pela malha) até a reserva — outra pasta,
/// outro `Programa` —, a posse passada, e a reserva ligando o Minecraft com o
/// mundo que chegou: o placar está lá, e o status volta pelo túnel dela.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "rede de verdade e ~210 MB: SEELE_MINECRAFT_DE_VERDADE=<plano.json>"]
async fn a_reserva_liga_com_o_mundo_da_principal() -> Result<()> {
    let Ok(caminho) = std::env::var("SEELE_MINECRAFT_DE_VERDADE") else {
        eprintln!("pulado: sem SEELE_MINECRAFT_DE_VERDADE");
        return Ok(());
    };
    let plano: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(caminho)?)?;
    let livre = || -> Result<u16> {
        let l = std::net::TcpListener::bind("127.0.0.1:0")?;
        Ok(l.local_addr()?.port())
    };
    let (jogo_a, jogo_b) = (livre()?, livre()? + 200);
    let (copia_a, copia_b) = (jogo_a + 400, jogo_b + 400);
    const COPIA: &str = "minecraft-1-copia";
    let (pasta_a, pasta_b) = (tempfile::tempdir()?, tempfile::tempdir()?);

    // A principal: o jogo de pé, e algo no mundo que só ela escreveu.
    let (principal, mut relatorio) =
        o_jogo_baixado(&plano, jogo_a, pasta_a.path(), Some(copia_a)).await?;
    let mut jogo = rodar_ate_o_done(&principal, &plano, &mut relatorio).await?;
    eprintln!("principal:\n{}", relatorio.join("\n"));
    jogo.escrever("scoreboard objectives add reserva dummy\n").await?;
    jogo.escrever("scoreboard players set prova reserva 42\n").await?;
    let marcou = esperar_linha(&mut jogo, "prova to 42", Duration::from_secs(20)).await?;
    eprintln!("na principal: {marcou}");

    // O servidor SEELE, as duas máquinas aceitas, e a principal servindo.
    let (endereco, daemon, conjunto, _temp) = servidor(true, None).await?;
    let mut ea = enlace(endereco, &conjunto, 1, "principal").await?;
    let mut eb = enlace(endereco, &conjunto, 2, "reserva").await?;
    let joga = enlace(endereco, &conjunto, 3, "joga").await?;
    use maquina_de_jogo::aceitar_no_papel;
    use seele_server::maquinas::Papel;
    aceitar_no_papel(&daemon, &mut ea, SERVIDOR_DE_JOGO, Papel::Principal,
        &[(TUNEL, ProtocoloDeTunel::Tcp, jogo_a), (COPIA, ProtocoloDeTunel::Tcp, copia_a)]).await?;
    aceitar_no_papel(&daemon, &mut eb, SERVIDOR_DE_JOGO, Papel::Reserva,
        &[(TUNEL, ProtocoloDeTunel::Tcp, jogo_b), (COPIA, ProtocoloDeTunel::Tcp, copia_b)]).await?;
    ea.servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo_a).await.map_err(|r| anyhow::anyhow!("{r:?}"))?;

    // **O salvamento seguro da receita, com o pacote no meio.**
    let relogio = Instant::now();
    jogo.escrever("save-off\n").await?;
    jogo.escrever("save-all flush\n").await?;
    let salvo = esperar_linha(&mut jogo, "]: Saved the game", Duration::from_secs(60)).await?;
    let salvou_em = relogio.elapsed();
    let principal = Arc::new(principal);
    let p = Arc::clone(&principal);
    let pacote = tokio::task::spawn_blocking(move || p.empacotar("world", "copias/mundo.tar.gz")).await??;
    let empacotou_em = relogio.elapsed() - salvou_em;
    jogo.escrever("save-on\n").await?;
    eprintln!("salvo em {salvou_em:?} («{salvo}»); pacote: {} arquivos, {} bytes → {} bytes, {} links pulados, {} travados {:?}, em {empacotou_em:?}",
        pacote.arquivos, pacote.bytes, pacote.tamanho, pacote.links_pulados, pacote.travados_pulados, pacote.travados);
    // **O que o jogo trava** (decisão W7): no Windows, só o `session.lock`
    // do mundo, que o Minecraft segura enquanto roda; no Unix, nada.
    if cfg!(windows) {
        assert_eq!(pacote.travados, vec!["world/session.lock".to_owned()]);
    } else {
        assert_eq!(pacote.travados_pulados, 0);
    }
    let oferta = principal.oferecer_copia("copias/mundo.tar.gz", copia_a).await?;
    ea.servir_tunel(MOD, COPIA, ProtocoloDeTunel::Tcp, copia_a).await.map_err(|r| anyhow::anyhow!("{r:?}"))?;

    // A reserva: outra pasta, o mesmo jogo baixado, e a cópia pelo túnel.
    let (reserva, mut relatorio_b) =
        o_jogo_baixado(&plano, jogo_b, pasta_b.path(), Some(copia_b)).await?;
    eprintln!("reserva (baixada):\n{}", relatorio_b.join("\n"));
    let local = eb.entrar_no_tunel(MOD, COPIA).await.map_err(|r| anyhow::anyhow!("{r:?}"))?;
    let mut caminho = None;
    let ate = Instant::now() + Duration::from_secs(10);
    while Instant::now() < ate && !format!("{caminho:?}").contains("Direto") {
        if let Ok(Aviso::Tunel(r)) = tokio::time::timeout(Duration::from_millis(200), eb.proximo()).await {
            if r.nome == COPIA {
                caminho = Some(r.caminho);
            }
        }
    }
    let relogio = Instant::now();
    let chegou = reserva
        .receber_copia(
            seele_core::programa::PedidoDeCopia {
                porta_local: local.porta,
                codigo: oferta.codigo.clone(),
                sha256: oferta.sha256.clone(),
                tamanho: oferta.tamanho,
                destino: "copias/recebida.tar.gz".into(),
            },
            |_, _| {},
        )
        .await?;
    eprintln!("cópia: {} bytes por {caminho:?} em {:?}, {} retomadas", chegou.tamanho, relogio.elapsed(), chegou.retomadas);
    let _ = eb.sair_do_tunel(MOD, COPIA).await;
    reserva.desempacotar(&PedidoDeDesempacotar {
        mesclar: false,
        arquivo: "copias/recebida.tar.gz".into(),
        destino: "world".into(),
        raiz: "world".into(),
    })?;

    // **A passagem**: a principal para (o stop salva), a posse passa, a
    // reserva liga com o mundo que chegou.
    jogo.escrever("stop\n").await?;
    let _ = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(e) = jogo.proximo().await {
            if matches!(e, EventoDoProcesso::Terminou(_)) {
                break;
            }
        }
    })
    .await;
    seele_server::maquinas::passar_a_posse(daemon.server(), MOD, SERVIDOR_DE_JOGO, Papel::Reserva);
    let relogio = Instant::now();
    let mut jogo_b_proc = rodar_ate_o_done(&reserva, &plano, &mut relatorio_b).await?;
    eprintln!("a reserva ligou com o mundo em {:?}", relogio.elapsed());
    jogo_b_proc.escrever("scoreboard players get prova reserva\n").await?;
    let placar = esperar_linha(&mut jogo_b_proc, "prova has", Duration::from_secs(20)).await?;
    eprintln!("o placar na reserva: {placar}");
    anyhow::ensure!(placar.contains("prova has 42"), "o mundo da reserva não é o da principal: {placar}");
    eb.servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo_b).await.map_err(|r| anyhow::anyhow!("a reserva, dona, não serviu: {r:?}"))?;
    let porta = joga.entrar_no_tunel(MOD, TUNEL).await.map_err(|r| anyhow::anyhow!("{r:?}"))?.porta;
    let (json, levou) = status(porta).await?;
    eprintln!("status pela reserva em {levou:?}: {json}");
    anyhow::ensure!(json.contains("pelo t"), "{json}");
    reserva.encerrar_tudo().await;
    principal.encerrar_tudo().await;
    daemon.shutdown();
    Ok(())
}
