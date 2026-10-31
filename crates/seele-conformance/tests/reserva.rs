//! **A reserva, de ponta a ponta** (API 8 parte 7; spec «A reserva» e «Como
//! provar que funciona»): o servidor SEELE de verdade com uma metade de
//! servidor que faz o papel do MOD, e **três `Enlace`s** — a principal, a
//! reserva e quem joga (que também é quem opera). Cada máquina tem o
//! `Programa` do núcleo de verdade, com a pasta dela, e roda um processo de
//! verdade (`seele-jogo-de-mentira`: um eco TCP e um batimento em disco).
//!
//! O que cada prova mostra:
//!
//! - `a_passagem_avisada_nao_perde_nada`: duas máquinas aceitas ao mesmo
//!   tempo, só a dona serve; salvar, a cópia final pelo túnel, a reserva liga,
//!   a principal desliga — o mundo da reserva é o da principal, byte a byte, e
//!   um processo só fica de pé.
//! - `a_queda_de_surpresa_passa_a_posse_depois_do_prazo`: a principal some;
//!   antes do prazo nada muda de dona; depois, `MaquinaSumiu` acorda a metade
//!   de servidor, a reserva assume com a última cópia, e o tempo perdido é o
//!   que passou desde ela.
//! - `a_principal_que_volta_com_o_processo_vivo_o_mata`: a principal perdeu a
//!   conexão mas não o processo; volta (`MaquinaVoltou`), o servidor não a
//!   deixa servir (`nao-e-a-dona`, contado), ela pergunta, ouve «desligar» e
//!   mata o processo, e a metade de servidor conta — um processo só fica.
//! - `sem_quem_hospeda_nada_muda_de_dona`: o servidor cai; a dona segue
//!   rodando, e a reserva não liga.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::programa::{
    ComandoAceito, EventoDoProcesso, Origem, OrigemHttps, PedidoDeCopia, PedidoDeDesempacotar,
    PedidoDeExecucao, Politica, Processo, Programa,
};
use seele_core::tunel::{Caminho, Queda, RelatorioDoTunel};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::tunel::{ProtocoloDeTunel, RecusaDeTunel};
use seele_server::maquinas::Papel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod maquina_de_jogo;
use maquina_de_jogo::{aceitar_no_papel, pedir_ao_mod, servidor_com, MOD};

const PACIENCIA: Duration = Duration::from_secs(30);

/// O servidor de jogo das provas.
const SERVIDOR: &str = "mundo-1";

/// O túnel do jogo e o da cópia.
const JOGO: &str = "jogo";
const COPIA: &str = "copia";

/// O prazo da máquina que sumiu, curto para a prova.
const PRAZO: Duration = Duration::from_secs(2);

/// **A metade de servidor da prova**, no papel do MOD de multiplayer: chama
/// cada máquina no papel dela, guarda a última cópia, passa a posse (avisada,
/// ou quando a dona some), diz a quem volta quem é a dona, e conta. Tudo o
/// que ela soube fica no quintal, e `{ ler: true }` devolve.
const METADE: &str = r#"
const outro = p => p === "principal" ? "reserva" : "principal";
const anotar = (chave, linha) => { dados[chave] = (dados[chave] || "") + linha + "\n"; };
const dona = () => dados.dona || "principal";
const ok = (extra) => JSON.stringify(Object.assign({ ok: true }, extra || {}));
function pedirA(quem, servidor, papel, tuneis) {
  mundo.maquinas.pedir({ quem, servidor, papel, oferta: {
    titulo: "Eco (" + papel + ")", receita: "eco", versao_da_receita: "1.0.0",
    versao_do_jogo: "1", tuneis } });
}
function passar(servidor, papel, perdido) {
  dados.dona = papel;
  mundo.maquinas.dona({ servidor, papel });
  mundo.maquinas.avisar({ servidor, papel, carga: { op: "assumir", perdido } });
  mundo.maquinas.avisar({ servidor, papel: outro(papel), carga: { op: "desligar" } });
}
globalThis.aoPedir = (contexto, pedido) => {
  const c = JSON.parse(contexto);
  const p = JSON.parse(pedido);
  if (c.maquina) {
    const m = c.maquina;
    anotar("falas", m.papel + " " + (m.dona ? "dona" : "nao-dona") + " " + p.op);
    if (p.op === "copia-pronta") {
      if (!m.dona) return JSON.stringify({ ok: false, erro: "nao-e-a-dona", ordem: "desligar" });
      dados.copia = JSON.stringify({ sha256: p.sha256, quando: Date.now() });
      mundo.maquinas.avisar({ servidor: m.servidor, papel: outro(m.papel),
        carga: { op: "copia", codigo: p.codigo, sha256: p.sha256, tamanho: p.tamanho } });
      return ok();
    }
    if (p.op === "copia-recebida") { dados.recebida = p.sha256; return ok(); }
    if (p.op === "quem-e-a-dona") return ok({ dona: m.dona, ordem: m.dona ? null : "desligar" });
    if (p.op === "matei") { dados.voltouRodando = String(Number(dados.voltouRodando || "0") + 1); return ok(); }
    return ok();
  }
  if (p.ler) return ok({ momentos: dados.momentos || "", falas: dados.falas || "",
    perdido: dados.perdido === undefined ? null : JSON.parse(dados.perdido), dona: dona(),
    voltouRodando: Number(dados.voltouRodando || "0"), recebida: dados.recebida || null });
  if (p.pedir) {
    dados["tuneis:" + p.papel] = JSON.stringify(p.tuneis);
    pedirA(c.person, p.servidor, p.papel, p.tuneis);
  }
  if (p.passar) {
    // Avisada só com a reserva já tendo a última cópia: é o «nada se perde».
    const copia = JSON.parse(dados.copia || "null");
    if (!copia || dados.recebida !== copia.sha256) return JSON.stringify({ ok: false, erro: "reserva-sem-a-copia" });
    passar(p.servidor, p.passar, 0);
  }
  return ok();
};
globalThis.aoAcontecer = (momento, carga) => {
  if (momento.indexOf("Maquina") !== 0) return;
  anotar("momentos", momento + " " + carga);
  const k = JSON.parse(carga);
  if (momento === "MaquinaSumiu" && k.papel === dona()) {
    const copia = JSON.parse(dados.copia || "null");
    const saiuEm = Date.now() - k.segundos * 1000;
    const perdido = copia ? Math.max(0, Math.round((saiuEm - copia.quando) / 100) / 10) : null;
    dados.perdido = JSON.stringify(perdido);
    passar(k.servidor, outro(k.papel), perdido);
  }
  if (momento === "MaquinaVoltou") {
    pedirA(String(k.pessoa), k.servidor, k.papel, JSON.parse(dados["tuneis:" + k.papel] || "[]"));
  }
};
"#;

/// Uma porta livre **longe das outras desta prova**: quem entra num túnel
/// abre a porta local na do jogo ou numa das 64 seguintes, e as quatro pontas
/// moram nesta mesma máquina — uma porta de cópia logo depois da do jogo seria
/// tomada pela porta de quem joga.
fn porta_livre() -> u16 {
    static DADAS: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());
    let mut dadas = DADAS.lock().unwrap();
    loop {
        let porta = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if dadas.iter().all(|d| d.abs_diff(porta) > 70) && porta < 65_400 {
            dadas.push(porta);
            return porta;
        }
    }
}

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

/// Uma máquina de jogo: o enlace dela, o `Programa` com a pasta dela, as
/// portas da oferta (o jogo e a cópia) e o processo, se roda.
struct Maquina {
    semente: u8,
    enlace: Enlace,
    programa: Arc<Programa>,
    _pasta: tempfile::TempDir,
    porta_do_jogo: u16,
    porta_da_copia: u16,
    processo: Option<Processo>,
    /// Os relatórios de túnel que passaram enquanto se lia outra coisa.
    relatorios: Vec<RelatorioDoTunel>,
}

impl Maquina {
    async fn nova(
        endereco: SocketAddr,
        conjunto: &str,
        semente: u8,
        apelido: &str,
    ) -> Result<Self> {
        let pasta = tempfile::tempdir()?;
        let raiz = pasta.path().join("jogo");
        std::fs::create_dir_all(raiz.join("bin"))?;
        std::fs::copy(
            env!("CARGO_BIN_EXE_seele-jogo-de-mentira"),
            raiz.join("bin")
                .join(format!("jogo{}", std::env::consts::EXE_SUFFIX)),
        )?;
        let porta_do_jogo = porta_livre();
        let porta_da_copia = porta_livre();
        let politica = Politica {
            dominios: vec![],
            artefatos: vec![],
            de_outros_sistemas: vec![],
            origens: vec![],
            comando: Some(ComandoAceito {
                comando: "bin/jogo".into(),
                argumentos: vec![porta_do_jogo.to_string()],
            }),
            instalar: None,
            pack: None,
            teto_de_disco: 1 << 30,
            portas_de_copia: vec![porta_da_copia],
            raiz,
        };
        let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::nova().unwrap());
        Ok(Self {
            semente,
            enlace: enlace(endereco, conjunto, semente, apelido).await?,
            programa: Arc::new(Programa::novo(politica, origem)?),
            _pasta: pasta,
            porta_do_jogo,
            porta_da_copia,
            processo: None,
            relatorios: Vec::new(),
        })
    }

    fn tuneis(&self) -> [(&'static str, ProtocoloDeTunel, u16); 2] {
        [
            (JOGO, ProtocoloDeTunel::Tcp, self.porta_do_jogo),
            (COPIA, ProtocoloDeTunel::Tcp, self.porta_da_copia),
        ]
    }

    fn raiz(&self) -> &Path {
        self.programa.raiz()
    }

    /// Liga o jogo e espera ele dizer que está pronto.
    async fn rodar(&mut self) -> Result<()> {
        let mut processo = self.programa.rodar(&PedidoDeExecucao {
            comando: "bin/jogo".into(),
            argumentos: vec![self.porta_do_jogo.to_string()],
            pasta: String::new(),
        })?;
        let fim = Instant::now() + PACIENCIA;
        loop {
            anyhow::ensure!(Instant::now() < fim, "o jogo não ficou pronto");
            match tokio::time::timeout(Duration::from_secs(1), processo.proximo()).await {
                Ok(Some(EventoDoProcesso::Saida(linha))) if linha.trim() == "pronto" => break,
                Ok(Some(EventoDoProcesso::Terminou(fim))) => anyhow::bail!("o jogo saiu: {fim:?}"),
                Ok(None) => anyhow::bail!("o jogo sumiu"),
                _ => {}
            }
        }
        self.processo = Some(processo);
        Ok(())
    }

    /// Para o jogo como a receita para (`stop`), e espera ele sair.
    async fn desligar_o_jogo(&mut self) -> Result<()> {
        let Some(mut processo) = self.processo.take() else {
            return Ok(());
        };
        processo.escrever("stop\n").await?;
        let fim = Instant::now() + PACIENCIA;
        loop {
            anyhow::ensure!(Instant::now() < fim, "o jogo não saiu no stop");
            if let Ok(Some(EventoDoProcesso::Terminou(_)) | None) =
                tokio::time::timeout(Duration::from_secs(1), processo.proximo()).await
            {
                return Ok(());
            }
        }
    }

    /// O batimento do jogo anda?
    async fn jogo_vivo(&self) -> bool {
        let ler = || {
            std::fs::read_to_string(self.raiz().join("batimento"))
                .ok()
                .and_then(|t| t.trim().parse::<u64>().ok())
                .unwrap_or(0)
        };
        let antes = ler();
        tokio::time::sleep(Duration::from_millis(300)).await;
        ler() > antes
    }

    /// Um mundo com arquivos e pastas, e uma marca de quem o escreveu.
    fn escrever_o_mundo(&self, marca: &str) {
        let w = self.raiz().join("world");
        std::fs::create_dir_all(w.join("region")).unwrap();
        std::fs::write(w.join("level.dat"), marca).unwrap();
        let grande: Vec<u8> = (0..2_000_000_u32)
            .map(|i| u8::try_from((i * 13 + i / 7) % 251).unwrap())
            .collect();
        std::fs::write(w.join("region/r.0.0.mca"), grande).unwrap();
    }

    fn mundo(&self) -> Vec<(String, Vec<u8>)> {
        arvore(&self.raiz().join("world"))
    }

    /// Um pedido desta máquina à metade de servidor, lido.
    async fn falar(&self, pedido: serde_json::Value) -> Result<serde_json::Value> {
        let resposta = self
            .enlace
            .pedido_da_maquina(MOD, SERVIDOR, pedido.to_string())
            .await
            .map_err(|e| anyhow::anyhow!("o pedido da máquina falhou: {e:?}"))?;
        Ok(serde_json::from_str(&resposta)?)
    }

    /// **A cópia**: o mundo empacotado depois de salvo, oferecido na porta do
    /// túnel da cópia, o túnel servido, e a metade de servidor avisada.
    async fn fazer_a_copia(&self) -> Result<()> {
        let programa = Arc::clone(&self.programa);
        let feito =
            tokio::task::spawn_blocking(move || programa.empacotar("world", "copias/mundo.tar.gz"))
                .await??;
        let oferta = self
            .programa
            .oferecer_copia("copias/mundo.tar.gz", self.porta_da_copia)
            .await?;
        assert_eq!(oferta.sha256, feito.sha256);
        // Servir de novo o que já serve é recusado; a primeira vez basta.
        let _ = self
            .enlace
            .servir_tunel(MOD, COPIA, ProtocoloDeTunel::Tcp, self.porta_da_copia)
            .await;
        let r = self
            .falar(serde_json::json!({ "op": "copia-pronta", "codigo": oferta.codigo, "sha256": oferta.sha256, "tamanho": oferta.tamanho }))
            .await?;
        anyhow::ensure!(r["ok"] == true, "a metade de servidor recusou a cópia: {r}");
        Ok(())
    }

    /// Lê avisos até um `AvisoParaMaquina` com `op`, guardando os relatórios
    /// de túnel que passarem.
    async fn esperar_aviso(&mut self, op: &str) -> Result<serde_json::Value> {
        let fim = Instant::now() + PACIENCIA;
        while Instant::now() < fim {
            let Ok(aviso) =
                tokio::time::timeout(Duration::from_millis(300), self.enlace.proximo()).await
            else {
                continue;
            };
            match aviso {
                Aviso::AvisoParaMaquina { carga, .. } => {
                    let carga: serde_json::Value = serde_json::from_str(&carga)?;
                    if carga["op"] == op {
                        return Ok(carga);
                    }
                }
                Aviso::Tunel(r) => self.relatorios.push(r),
                _ => {}
            }
        }
        anyhow::bail!("nenhum aviso `{op}` chegou a esta máquina")
    }

    /// Nenhum aviso `op` chega em `quanto`.
    async fn nenhum_aviso(&mut self, op: &str, quanto: Duration) -> Result<()> {
        let fim = Instant::now() + quanto;
        while Instant::now() < fim {
            if let Ok(Aviso::AvisoParaMaquina { carga, .. }) =
                tokio::time::timeout(Duration::from_millis(100), self.enlace.proximo()).await
            {
                anyhow::ensure!(
                    !carga.contains(&format!("\"{op}\"")),
                    "chegou `{op}` cedo demais: {carga}"
                );
            }
        }
        Ok(())
    }

    /// **Recebe a cópia anunciada**: entra no túnel da cópia, recebe pela
    /// porta local, sai, confere, abre o mundo e diz à metade de servidor.
    async fn receber_a_copia(&mut self, carga: &serde_json::Value) -> Result<Caminho> {
        let porta = self
            .enlace
            .entrar_no_tunel(MOD, COPIA)
            .await
            .map_err(|r| anyhow::anyhow!("entrar no túnel da cópia: {r:?}"))?;
        // **A cópia espera a malha um pouco**: um túnel começa pela estrela e
        // só as ligações novas vão pelo direto; uma cópia de GiB que começa
        // na estrela fica nela. Até 5 s pelo relatório que diz `Direto`.
        let ate = Instant::now() + Duration::from_secs(5);
        while Instant::now() < ate
            && !self
                .relatorios
                .iter()
                .any(|r| r.nome == COPIA && matches!(r.caminho, Caminho::Direto { .. }))
        {
            if let Ok(Aviso::Tunel(r)) =
                tokio::time::timeout(Duration::from_millis(100), self.enlace.proximo()).await
            {
                self.relatorios.push(r);
            }
        }
        let comecou = Instant::now();
        let recebida = self
            .programa
            .receber_copia(
                PedidoDeCopia {
                    porta_local: porta.porta,
                    codigo: carga["codigo"].as_str().unwrap().to_owned(),
                    sha256: carga["sha256"].as_str().unwrap().to_owned(),
                    tamanho: carga["tamanho"].as_u64().unwrap(),
                    destino: "copias/recebida.tar.gz".into(),
                },
                |_, _| {},
            )
            .await?;
        println!(
            "cópia de {} bytes recebida em {:?}",
            recebida.tamanho,
            comecou.elapsed()
        );
        // O caminho que a cópia usou, pelo relatório do túnel dela.
        let mut caminho = None;
        let fim = Instant::now() + Duration::from_secs(5);
        while caminho.is_none() && Instant::now() < fim {
            if let Some(r) = self.relatorios.iter().rev().find(|r| r.nome == COPIA) {
                caminho = Some(r.caminho);
                break;
            }
            if let Ok(Aviso::Tunel(r)) =
                tokio::time::timeout(Duration::from_millis(200), self.enlace.proximo()).await
            {
                self.relatorios.push(r);
            }
        }
        let _ = self.enlace.sair_do_tunel(MOD, COPIA).await;
        let programa = Arc::clone(&self.programa);
        tokio::task::spawn_blocking(move || {
            programa.desempacotar(&PedidoDeDesempacotar {
                arquivo: "copias/recebida.tar.gz".into(),
                destino: "world".into(),
                raiz: "world".into(),
                mesclar: false,
            })
        })
        .await??;
        let r = self
            .falar(serde_json::json!({ "op": "copia-recebida", "sha256": recebida.sha256 }))
            .await?;
        anyhow::ensure!(r["ok"] == true, "{r}");
        Ok(caminho.unwrap_or(Caminho::Estrela))
    }
}

fn arvore(raiz: &Path) -> Vec<(String, Vec<u8>)> {
    let mut todos = Vec::new();
    let mut pendentes: Vec<PathBuf> = vec![raiz.to_path_buf()];
    while let Some(p) = pendentes.pop() {
        let Ok(entradas) = std::fs::read_dir(&p) else {
            continue;
        };
        for e in entradas.flatten() {
            let c = e.path();
            if c.is_dir() {
                pendentes.push(c);
            } else {
                let rel = c
                    .strip_prefix(raiz)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                todos.push((rel, std::fs::read(&c).unwrap()));
            }
        }
    }
    todos.sort();
    todos
}

/// O que a metade de servidor guardou.
async fn ler(enlace: &mut Enlace) -> Result<serde_json::Value> {
    let resposta = pedir_ao_mod(enlace, &serde_json::json!({ "ler": true })).await?;
    Ok(serde_json::from_str(&resposta)?)
}

/// Espera o quintal ter `trecho` nos momentos.
async fn esperar_momento(enlace: &mut Enlace, trecho: &str) -> Result<serde_json::Value> {
    let fim = Instant::now() + PACIENCIA;
    loop {
        let lido = ler(enlace).await?;
        if lido["momentos"]
            .as_str()
            .unwrap_or_default()
            .contains(trecho)
        {
            return Ok(lido);
        }
        anyhow::ensure!(Instant::now() < fim, "o MOD não ouviu `{trecho}`: {lido}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Um eco pela porta local de quem joga: o que vai volta igual.
async fn eco_passa(porta: u16) -> Result<()> {
    let mut fluxo = TcpStream::connect(("127.0.0.1", porta)).await?;
    let ida = b"ola, mundo";
    fluxo.write_all(ida).await?;
    let mut volta = vec![0_u8; ida.len()];
    tokio::time::timeout(PACIENCIA, fluxo.read_exact(&mut volta)).await??;
    anyhow::ensure!(volta == ida, "o eco voltou outro");
    Ok(())
}

/// Lê avisos de quem joga até o túnel do jogo acabar, com o motivo.
async fn esperar_o_fim_do_jogo(joga: &mut Enlace) -> Result<Queda> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        if let Ok(Aviso::Tunel(r)) =
            tokio::time::timeout(Duration::from_millis(300), joga.proximo()).await
        {
            if r.nome == JOGO {
                if let Some(q) = r.encerrado {
                    return Ok(q);
                }
            }
        }
    }
    anyhow::bail!("o túnel do jogo não acabou para quem joga")
}

/// Entra no túnel do jogo, tentando até a dona estar servindo.
async fn entrar_no_jogo(joga: &Enlace) -> Result<u16> {
    let fim = Instant::now() + PACIENCIA;
    loop {
        match joga.entrar_no_tunel(MOD, JOGO).await {
            Ok(porta) => return Ok(porta.porta),
            Err(_) if Instant::now() < fim => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(r) => anyhow::bail!("não entrou no jogo: {r:?}"),
        }
    }
}

/// O servidor, a principal e a reserva aceitas, e quem joga.
async fn montar(
    malha: bool,
) -> Result<(
    Arc<seele_server::Daemon>,
    tempfile::TempDir,
    Maquina,
    Maquina,
    Enlace,
    SocketAddr,
    String,
)> {
    let (endereco, daemon, conjunto, temp) = servidor_com(malha, None, METADE, PRAZO).await?;
    let mut principal = Maquina::nova(endereco, &conjunto, 1, "principal").await?;
    let mut reserva = Maquina::nova(endereco, &conjunto, 2, "reserva").await?;
    let joga = enlace(endereco, &conjunto, 3, "joga").await?;
    let tuneis = principal.tuneis();
    aceitar_no_papel(
        &daemon,
        &mut principal.enlace,
        SERVIDOR,
        Papel::Principal,
        &tuneis,
    )
    .await?;
    let tuneis = reserva.tuneis();
    aceitar_no_papel(
        &daemon,
        &mut reserva.enlace,
        SERVIDOR,
        Papel::Reserva,
        &tuneis,
    )
    .await?;
    Ok((daemon, temp, principal, reserva, joga, endereco, conjunto))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_passagem_avisada_nao_perde_nada() -> Result<()> {
    let (daemon, _temp, mut principal, mut reserva, mut joga, _, _) = montar(true).await?;
    let livro = || daemon.server().maquinas();

    // **As duas aceitas, e só a dona serve.** Pedir a reserva não dispensou a
    // principal.
    assert_eq!(
        livro().quem_opera_como(MOD, SERVIDOR, Papel::Principal),
        Some(principal.enlace.sessao().person)
    );
    assert_eq!(
        livro().quem_opera_como(MOD, SERVIDOR, Papel::Reserva),
        Some(reserva.enlace.sessao().person)
    );
    principal.rodar().await?;
    principal
        .enlace
        .servir_tunel(MOD, JOGO, ProtocoloDeTunel::Tcp, principal.porta_do_jogo)
        .await
        .map_err(|r| anyhow::anyhow!("a principal não serviu: {r:?}"))?;
    assert_eq!(
        reserva
            .enlace
            .servir_tunel(MOD, JOGO, ProtocoloDeTunel::Tcp, reserva.porta_do_jogo)
            .await,
        Err(RecusaDeTunel::ForaDoAceite),
        "a reserva serviu sem ser a dona"
    );
    assert_eq!(livro().tuneis_de_quem_nao_e_dona(MOD), 1);
    let porta = entrar_no_jogo(&joga).await?;
    eco_passa(porta).await?;

    // **Salvar, mandar a cópia final, a reserva liga, a principal desliga.**
    principal.escrever_o_mundo("o mundo da principal, salvo");
    principal.fazer_a_copia().await?;
    let carga = reserva.esperar_aviso("copia").await?;
    let caminho = reserva.receber_a_copia(&carga).await?;
    println!("a cópia andou por: {caminho:?}");
    // Com a malha, pela ligação direta entre as duas máquinas.
    assert!(matches!(caminho, Caminho::Direto { .. }), "{caminho:?}");
    let passou = pedir_ao_mod(
        &mut joga,
        &serde_json::json!({ "passar": "reserva", "servidor": SERVIDOR }),
    )
    .await?;
    assert_eq!(passou, r#"{"ok":true}"#, "{passou}");

    // O servidor parou o túnel da dona de antes **agora**, sem ela parar.
    assert_eq!(esperar_o_fim_do_jogo(&mut joga).await?, Queda::MaquinaParou);
    assert_eq!(livro().dona(MOD, SERVIDOR), Papel::Reserva);
    assert_eq!(
        livro().quem_opera_como(MOD, SERVIDOR, Papel::Principal),
        Some(principal.enlace.sessao().person),
        "a principal deixou de ser aceita: a posse não é um aceite"
    );
    reserva.esperar_aviso("assumir").await?;
    reserva.rodar().await?;
    reserva
        .enlace
        .servir_tunel(MOD, JOGO, ProtocoloDeTunel::Tcp, reserva.porta_do_jogo)
        .await
        .map_err(|r| anyhow::anyhow!("a reserva, dona, não serviu: {r:?}"))?;
    principal.esperar_aviso("desligar").await?;
    principal.desligar_o_jogo().await?;
    assert_eq!(
        principal
            .enlace
            .servir_tunel(MOD, JOGO, ProtocoloDeTunel::Tcp, principal.porta_do_jogo)
            .await,
        Err(RecusaDeTunel::ForaDoAceite),
        "a principal, sem a posse, serviu"
    );

    // Nada se perdeu: o mundo da reserva é o da principal, byte a byte.
    assert_eq!(
        reserva.mundo(),
        principal.mundo(),
        "o mundo da reserva não é o da principal"
    );
    assert!(!reserva.mundo().is_empty());
    // E quem joga entra de novo, pelo mesmo nome, agora na reserva.
    let porta = entrar_no_jogo(&joga).await?;
    eco_passa(porta).await?;
    // **Um processo só de pé.**
    assert!(reserva.jogo_vivo().await, "o jogo da reserva não roda");
    assert!(
        !principal.jogo_vivo().await,
        "o jogo da principal segue rodando"
    );
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queda_de_surpresa_passa_a_posse_depois_do_prazo() -> Result<()> {
    let (daemon, _temp, mut principal, mut reserva, mut joga, _, _) = montar(false).await?;
    principal.rodar().await?;
    principal
        .enlace
        .servir_tunel(MOD, JOGO, ProtocoloDeTunel::Tcp, principal.porta_do_jogo)
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    principal.escrever_o_mundo("a última cópia");
    principal.fazer_a_copia().await?;
    let copiou = Instant::now();
    let carga = reserva.esperar_aviso("copia").await?;
    // Sem a malha, pela estrela: o mesmo túnel, repassado por quem hospeda.
    assert_eq!(reserva.receber_a_copia(&carga).await?, Caminho::Estrela);
    let copiado = reserva.mundo();
    // Depois da cópia o jogo segue: isto é o que se perde.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    principal.escrever_o_mundo("o que veio depois da cópia");

    // **A principal some** — a máquina inteira: a conexão e o processo.
    let caiu = Instant::now();
    let pessoa = principal.enlace.sessao().person;
    drop(principal.processo.take());
    principal.programa.encerrar_tudo().await;
    drop(principal.enlace);

    // Antes do prazo, nada muda de dona, e a reserva não liga.
    esperar_momento(&mut joga, &format!(r#"MaquinaParou {{"servidor":"{SERVIDOR}","papel":"principal","pessoa":{},"motivo":"saiu"}}"#, pessoa.0)).await?;
    assert!(
        caiu.elapsed() < PRAZO,
        "o teste é lento demais para ver o antes do prazo"
    );
    assert_eq!(
        daemon.server().maquinas().dona(MOD, SERVIDOR),
        Papel::Principal,
        "a posse mudou antes do prazo"
    );
    reserva
        .nenhum_aviso(
            "assumir",
            PRAZO
                .saturating_sub(caiu.elapsed())
                .saturating_sub(Duration::from_millis(300)),
        )
        .await?;

    // Depois do prazo, `MaquinaSumiu` acorda a metade de servidor, e a
    // reserva assume com a última cópia.
    let carga = reserva.esperar_aviso("assumir").await?;
    let depois = caiu.elapsed();
    assert!(
        depois >= PRAZO,
        "assumiu em {depois:?}, antes do prazo de {PRAZO:?}"
    );
    let lido = esperar_momento(&mut joga, "MaquinaSumiu").await?;
    assert!(lido["momentos"].as_str().unwrap().contains(&format!(r#"MaquinaSumiu {{"servidor":"{SERVIDOR}","papel":"principal","pessoa":{},"motivo":"saiu","segundos":2}}"#, pessoa.0)), "{lido}");
    assert_eq!(
        daemon.server().maquinas().dona(MOD, SERVIDOR),
        Papel::Reserva
    );
    // **O tempo perdido** é o que passou entre a última cópia e a queda.
    let perdido = carga["perdido"].as_f64().expect("o tempo perdido não veio");
    println!("assumiu {depois:?} depois da queda; tempo de jogo perdido: {perdido} s");
    // Jogou-se 1,5 s depois da cópia; o servidor vê a queda no tempo dele, e
    // o que ele conta não passa do que correu da cópia até o «assumir», menos
    // o prazo.
    let teto = (copiou.elapsed() - PRAZO).as_secs_f64() + 0.5;
    assert!(
        (1.4..=teto).contains(&perdido),
        "perdido: {perdido}, teto {teto}"
    );
    assert_eq!(lido["perdido"].as_f64(), Some(perdido));
    reserva.rodar().await?;
    reserva
        .enlace
        .servir_tunel(MOD, JOGO, ProtocoloDeTunel::Tcp, reserva.porta_do_jogo)
        .await
        .map_err(|r| anyhow::anyhow!("a reserva, dona, não serviu: {r:?}"))?;
    assert_eq!(
        reserva.mundo(),
        copiado,
        "a reserva não ligou com a última cópia"
    );
    let porta = entrar_no_jogo(&joga).await?;
    eco_passa(porta).await?;
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_principal_que_volta_com_o_processo_vivo_o_mata() -> Result<()> {
    let (daemon, _temp, mut principal, mut reserva, mut joga, endereco, conjunto) =
        montar(false).await?;
    principal.rodar().await?;
    principal.escrever_o_mundo("antes da queda");
    principal.fazer_a_copia().await?;
    let carga = reserva.esperar_aviso("copia").await?;
    reserva.receber_a_copia(&carga).await?;

    // **A principal perde a conexão, e só ela**: o processo segue vivo.
    let semente = principal.semente;
    let pessoa = principal.enlace.sessao().person;
    let velho = std::mem::replace(
        &mut principal.enlace,
        enlace_de_mentira(endereco, &conjunto).await?,
    );
    drop(velho);
    let carga = reserva.esperar_aviso("assumir").await?;
    assert!(carga["perdido"].is_number(), "{carga}");
    reserva.rodar().await?;
    assert!(
        principal.jogo_vivo().await && reserva.jogo_vivo().await,
        "dois de pé é o caso que a prova quer"
    );

    // **Ela volta.** O servidor diz `MaquinaVoltou`, e a metade de servidor
    // a chama de novo como principal.
    principal.enlace = enlace(endereco, &conjunto, semente, "principal").await?;
    assert_eq!(principal.enlace.sessao().person, pessoa);
    let tuneis = principal.tuneis();
    aceitar_no_papel(
        &daemon,
        &mut principal.enlace,
        SERVIDOR,
        Papel::Principal,
        &tuneis,
    )
    .await?;
    esperar_momento(
        &mut joga,
        &format!(
            r#"MaquinaVoltou {{"servidor":"{SERVIDOR}","papel":"principal","pessoa":{}}}"#,
            pessoa.0
        ),
    )
    .await?;
    // O servidor não a deixa servir: não é a dona. Contado.
    let antes = daemon.server().maquinas().tuneis_de_quem_nao_e_dona(MOD);
    assert_eq!(
        principal
            .enlace
            .servir_tunel(MOD, JOGO, ProtocoloDeTunel::Tcp, principal.porta_do_jogo)
            .await,
        Err(RecusaDeTunel::ForaDoAceite)
    );
    assert_eq!(
        daemon.server().maquinas().tuneis_de_quem_nao_e_dona(MOD),
        antes + 1
    );
    // Ela pergunta, ouve «desligar», mata o processo, e diz.
    let r = principal
        .falar(serde_json::json!({ "op": "quem-e-a-dona" }))
        .await?;
    assert_eq!(r["dona"], false, "{r}");
    assert_eq!(r["ordem"], "desligar", "{r}");
    principal.desligar_o_jogo().await?;
    let r = principal
        .falar(serde_json::json!({ "op": "matei" }))
        .await?;
    assert_eq!(r["ok"], true);
    let lido = ler(&mut joga).await?;
    assert_eq!(
        lido["voltouRodando"], 1,
        "a volta com o processo vivo não foi contada: {lido}"
    );
    // **Um processo só de pé**, o da dona.
    assert!(
        !principal.jogo_vivo().await,
        "o processo da principal segue vivo"
    );
    assert!(reserva.jogo_vivo().await, "o da reserva caiu");
    assert_eq!(
        daemon.server().maquinas().dona(MOD, SERVIDOR),
        Papel::Reserva
    );
    daemon.shutdown();
    Ok(())
}

/// Um enlace de outra pessoa qualquer, para tomar o lugar do que cai sem a
/// `Maquina` ficar sem enlace.
async fn enlace_de_mentira(endereco: SocketAddr, conjunto: &str) -> Result<Enlace> {
    enlace(endereco, conjunto, 9, "ninguem").await
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_quem_hospeda_nada_muda_de_dona() -> Result<()> {
    let (daemon, _temp, mut principal, mut reserva, _joga, _, _) = montar(false).await?;
    principal.rodar().await?;
    principal.escrever_o_mundo("a dona");
    principal.fazer_a_copia().await?;
    let carga = reserva.esperar_aviso("copia").await?;
    reserva.receber_a_copia(&carga).await?;

    // **Quem hospeda cai.**
    daemon.shutdown();
    drop(daemon);
    // Mais que o prazo: ninguém decide, e nada muda.
    reserva.nenhum_aviso("assumir", PRAZO * 3).await?;
    assert!(principal.jogo_vivo().await, "a dona parou sem quem hospeda");
    assert!(
        reserva.processo.is_none() && reserva.programa.processos_vivos() == 0,
        "a reserva ligou sozinha"
    );
    assert!(!reserva.jogo_vivo().await);
    Ok(())
}
