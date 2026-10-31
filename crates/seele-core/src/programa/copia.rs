//! **A cópia do mundo de uma máquina a outra** (API 8 parte 7, a reserva):
//! `maquina.programa.oferecerCopia` na máquina que tem o mundo, e
//! `maquina.programa.receberCopia` na que o recebe.
//!
//! # Pelo caminho do túnel, e não por um fio novo
//!
//! A cópia anda **pelo túnel de jogo** que a máquina dona já serve: a que
//! oferece escuta numa porta TCP de um túnel da oferta aceita, em
//! `127.0.0.1`, e a que recebe entra nesse túnel como quem joga entra e liga
//! na porta local que ele abre. O túnel já sabe ir direto pela malha e, sem
//! ela, pela estrela; já conta as quedas; e o servidor só deixa servir quem
//! é a dona agora (spec, «A reserva»). Um fio próprio para arquivos teria de
//! reaprender tudo isso.
//!
//! # A fala, nos dois lados
//!
//! Quem recebe manda uma linha, `SEELE-COPIA/1 <codigo> <sha256> <desde>\n`;
//! quem oferece responde `OK <tamanho>\n` e os bytes de `desde` até o fim, ou
//! `RECUSA <motivo>\n`. O **código** é sorteado a cada oferta e só vai à
//! máquina que recebe pela metade de servidor: o túnel é de quem tem a ação
//! `entrar-no-jogo`, e o mundo não é de todos os que jogam. O **sha256** na
//! linha diz qual cópia se quer: uma retomada de uma cópia velha contra uma
//! nova é recusada (`outra-copia`), em vez de virar um arquivo misturado.
//!
//! # Retomar
//!
//! O que chega fica num parcial ao lado do destino, com o começo do sha256 no
//! nome. Uma ligação que cai no meio é tentada de novo dali, até
//! [`MAX_TENTATIVAS_DA_COPIA`]; e o parcial **fica** quando elas acabam, para
//! o próximo `receberCopia` da mesma cópia continuar. Só a cópia inteira é
//! conferida, e só a que bate vira o destino.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use sha2::Digest as _;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::{caminho, Interior, Programa, Recusa};

/// Quantas vezes `receberCopia` liga de novo depois de uma queda no meio.
pub const MAX_TENTATIVAS_DA_COPIA: u32 = 5;

/// Quanto se espera sem byte nenhum antes de dar a ligação por caída.
pub const PRAZO_SEM_BYTES_DA_COPIA: Duration = Duration::from_secs(30);

/// A primeira palavra da fala.
const FALA: &str = "SEELE-COPIA/1";

/// O maior pedido: a fala, o código, o sha256 e um número.
const MAX_LINHA: usize = 256;

/// Quantas ligações de cópia uma oferta atende ao mesmo tempo.
const MAX_LIGACOES: usize = 4;

/// O que vai de uma vez pelo túnel.
const PEDACO: usize = 64 * 1024;

/// A extensão do parcial de uma cópia que chega. Ela **conta** no disco
/// usado (`caminho::ocupado`), ao contrário da dos downloads: a cópia não
/// reserva o tamanho dela, e o que já chegou ocupa.
const EXTENSAO_RECEBENDO: &str = "seele-recebendo";

/// Uma cópia oferecida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiaOferecida {
    /// O código que quem recebe tem de mandar.
    pub codigo: String,
    /// O tamanho do pacote.
    pub tamanho: u64,
    /// O sha256 do pacote, em hex minúsculo.
    pub sha256: String,
    /// A porta em `127.0.0.1` onde ela é servida.
    pub porta: u16,
}

/// O que `receberCopia` pede.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoDeCopia {
    /// A porta em `127.0.0.1` que leva à máquina que oferece (a porta local
    /// do túnel em que se entrou).
    pub porta_local: u16,
    /// O código da oferta.
    pub codigo: String,
    /// O sha256 anunciado.
    pub sha256: String,
    /// O tamanho anunciado.
    pub tamanho: u64,
    /// Onde a cópia fica, relativo à raiz do servidor de jogo.
    pub destino: String,
}

/// Uma cópia que chegou e bateu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiaRecebida {
    /// Onde ela ficou.
    pub destino: PathBuf,
    /// O tamanho.
    pub tamanho: u64,
    /// O sha256 conferido.
    pub sha256: String,
    /// Quantas vezes a ligação caiu e foi retomada.
    pub retomadas: u32,
    /// Quantos bytes já estavam de uma vez anterior.
    pub ja_estavam: u64,
}

/// Para a oferta de agora, se há uma.
pub(crate) fn parar_de_oferecer(interior: &Interior) {
    if let Ok(mut copia) = interior.copia.lock() {
        if let Some(escuta) = copia.take() {
            escuta.abort();
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut texto = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(texto, "{byte:02x}");
    }
    texto
}

/// O sha256 de um arquivo, em blocos.
fn sha256_de(caminho: &Path) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut arquivo = std::fs::File::open(caminho)?;
    let mut sha = sha2::Sha256::new();
    let mut pedaco = vec![0_u8; 1024 * 1024];
    loop {
        let n = arquivo.read(&mut pedaco)?;
        if n == 0 {
            break;
        }
        sha.update(pedaco.get(..n).unwrap_or_default());
    }
    Ok(hex(&sha.finalize()))
}

/// Compara sem parar no primeiro byte diferente: o código é um segredo.
fn iguais(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0_u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// O que se lembra do pacote oferecido, para ver se ele mudou no disco.
#[derive(Clone)]
struct Oferecido {
    caminho: PathBuf,
    codigo: String,
    sha256: String,
    tamanho: u64,
    modificado: Option<SystemTime>,
}

impl Oferecido {
    fn ainda_e_o_mesmo(&self) -> bool {
        std::fs::metadata(&self.caminho)
            .is_ok_and(|m| m.len() == self.tamanho && m.modified().ok() == self.modificado)
    }
}

async fn atender(mut fluxo: TcpStream, oferecido: Arc<Oferecido>) {
    let (leitura, mut escrita) = fluxo.split();
    let mut leitor = BufReader::new(leitura).take(MAX_LINHA as u64);
    let mut linha = String::new();
    let lida = tokio::time::timeout(Duration::from_secs(10), leitor.read_line(&mut linha)).await;
    let recusar = |motivo: &'static str| format!("RECUSA {motivo}\n");
    let resposta: Result<u64, &'static str> = match lida {
        Ok(Ok(_)) if linha.ends_with('\n') => {
            let partes: Vec<&str> = linha.trim_end().split(' ').collect();
            match partes.as_slice() {
                [fala, codigo, sha, desde] if *fala == FALA => {
                    if !iguais(codigo, &oferecido.codigo) {
                        Err("codigo-errado")
                    } else if *sha != oferecido.sha256 {
                        Err("outra-copia")
                    } else if !oferecido.ainda_e_o_mesmo() {
                        Err("copia-mudou")
                    } else {
                        match desde.parse::<u64>() {
                            Ok(desde) if desde <= oferecido.tamanho => Ok(desde),
                            Ok(_) => Err("alem-do-fim"),
                            Err(_) => Err("fala-errada"),
                        }
                    }
                }
                _ => Err("fala-errada"),
            }
        }
        _ => Err("fala-errada"),
    };
    let desde = match resposta {
        Ok(desde) => desde,
        Err(motivo) => {
            tracing::warn!(motivo, "cópia do mundo: pedido recusado");
            let _ = escrita.write_all(recusar(motivo).as_bytes()).await;
            return;
        }
    };
    let mut arquivo = match tokio::fs::File::open(&oferecido.caminho).await {
        Ok(arquivo) => arquivo,
        Err(erro) => {
            tracing::warn!(%erro, "cópia do mundo: o pacote não abriu");
            let _ = escrita.write_all(recusar("copia-mudou").as_bytes()).await;
            return;
        }
    };
    if tokio::io::AsyncSeekExt::seek(&mut arquivo, std::io::SeekFrom::Start(desde))
        .await
        .is_err()
    {
        let _ = escrita.write_all(recusar("copia-mudou").as_bytes()).await;
        return;
    }
    if escrita
        .write_all(format!("OK {}\n", oferecido.tamanho).as_bytes())
        .await
        .is_err()
    {
        return;
    }
    let mut falta = oferecido.tamanho - desde;
    let mut pedaco = vec![0_u8; PEDACO];
    while falta > 0 {
        let quero = usize::try_from(falta.min(PEDACO as u64)).unwrap_or(PEDACO);
        let Some(buf) = pedaco.get_mut(..quero) else {
            return;
        };
        match arquivo.read(buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if escrita
                    .write_all(pedaco.get(..n).unwrap_or_default())
                    .await
                    .is_err()
                {
                    tracing::info!(desde, falta, "cópia do mundo: quem recebia caiu no meio");
                    return;
                }
                falta -= n as u64;
            }
        }
    }
    let _ = escrita.shutdown().await;
    tracing::info!(
        desde,
        tamanho = oferecido.tamanho,
        "cópia do mundo: servida inteira"
    );
}

impl Programa {
    /// **Oferece `arquivo` como a cópia do mundo**, em `127.0.0.1:porta`, até
    /// a próxima oferta, o [`Programa::encerrar_tudo`] ou o fim do
    /// [`Programa`]. Uma oferta nova troca a de antes.
    ///
    /// `porta` tem de ser a de um túnel TCP da oferta aceita
    /// ([`super::Politica::portas_de_copia`]): é por ele que a cópia sai.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], as do caminho, [`Recusa::PortaForaDoAceite`],
    /// [`Recusa::PortaOcupada`] e [`Recusa::Disco`]. Toda recusa é contada.
    pub async fn oferecer_copia(
        &self,
        arquivo: &str,
        porta: u16,
    ) -> Result<CopiaOferecida, Recusa> {
        self.oferecer_copia_sem_contar(arquivo, porta)
            .await
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    async fn oferecer_copia_sem_contar(
        &self,
        arquivo: &str,
        porta: u16,
    ) -> Result<CopiaOferecida, Recusa> {
        let interior = Arc::clone(&self.interior);
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        if !interior.politica.portas_de_copia.contains(&porta) {
            return Err(Recusa::PortaForaDoAceite {
                porta,
                aceitas: interior.politica.portas_de_copia.clone(),
            });
        }
        let caminho = caminho::dentro_da_raiz(&interior.raiz, arquivo, false)?;
        let disco = |erro: std::io::Error| Recusa::Disco {
            caminho: caminho.clone(),
            motivo: erro.to_string(),
        };
        let meta = std::fs::metadata(&caminho).map_err(disco)?;
        let para_o_hash = caminho.clone();
        let sha256 = tokio::task::spawn_blocking(move || sha256_de(&para_o_hash))
            .await
            .map_err(|_| Recusa::Encerrado)?
            .map_err(disco)?;
        // A escuta de antes sai primeiro: a porta é a mesma.
        parar_de_oferecer(&interior);
        let escuta = TcpListener::bind(("127.0.0.1", porta))
            .await
            .map_err(|erro| Recusa::PortaOcupada {
                porta,
                motivo: erro.to_string(),
            })?;
        let codigo = { hex(&rand::random::<[u8; 16]>()) };
        let oferecido = Arc::new(Oferecido {
            caminho: caminho.clone(),
            codigo: codigo.clone(),
            sha256: sha256.clone(),
            tamanho: meta.len(),
            modificado: meta.modified().ok(),
        });
        let vagas = Arc::new(tokio::sync::Semaphore::new(MAX_LIGACOES));
        let servindo = Arc::clone(&oferecido);
        let tarefa = tokio::spawn(async move {
            while let Ok((fluxo, _)) = escuta.accept().await {
                let Ok(vaga) = Arc::clone(&vagas).try_acquire_owned() else {
                    tracing::warn!(
                        teto = MAX_LIGACOES,
                        "cópia do mundo: ligações demais; esta foi cortada"
                    );
                    continue;
                };
                let oferecido = Arc::clone(&servindo);
                tokio::spawn(async move {
                    atender(fluxo, oferecido).await;
                    drop(vaga);
                });
            }
        });
        if let Ok(mut copia) = interior.copia.lock() {
            *copia = Some(tarefa.abort_handle());
        }
        tracing::info!(arquivo = %caminho.display(), porta, tamanho = meta.len(), sha256 = %sha256, "cópia do mundo: oferecida");
        Ok(CopiaOferecida {
            codigo,
            tamanho: meta.len(),
            sha256,
            porta,
        })
    }

    /// **Recebe a cópia oferecida** pela porta local de um túnel, e só a
    /// grava em `destino` se o sha256 do que chegou for o anunciado.
    ///
    /// `ao_progresso(recebidos, total)` é chamado a cada pedaço.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], as do caminho, [`Recusa::DiscoCheio`],
    /// [`Recusa::HashMalFormado`], [`Recusa::CopiaRecusada`],
    /// [`Recusa::CopiaCaiu`], [`Recusa::CopiaNaoBate`] e [`Recusa::Disco`].
    /// Toda recusa é contada.
    pub async fn receber_copia(
        &self,
        pedido: PedidoDeCopia,
        ao_progresso: impl Fn(u64, u64) + Send + Sync + 'static,
    ) -> Result<CopiaRecebida, Recusa> {
        let interior = Arc::clone(&self.interior);
        let numero = interior.proximo_numero();
        let tarefa = tokio::spawn(receber(Arc::clone(&interior), pedido, ao_progresso));
        if let Ok(mut downloads) = interior.downloads.lock() {
            downloads.insert(numero, tarefa.abort_handle());
        }
        let feito = tarefa.await;
        if let Ok(mut downloads) = interior.downloads.lock() {
            downloads.remove(&numero);
        }
        match feito {
            Ok(feito) => feito,
            Err(_) => Err(Recusa::Encerrado),
        }
        .map_err(|recusa| interior.recusar(recusa))
    }
}

/// O parcial de uma cópia: ao lado do destino, com o começo do sha256.
fn parcial_de(alvo: &Path, sha256: &str) -> PathBuf {
    let nome = alvo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let curto: String = sha256.chars().take(16).collect();
    alvo.with_file_name(format!(".{nome}.{curto}.{EXTENSAO_RECEBENDO}"))
}

/// Uma ligação: devolve se acabou inteira, ou o motivo de ter caído (que
/// vale tentar de novo), ou a recusa (que não vale).
async fn uma_ligacao(
    pedido: &PedidoDeCopia,
    parcial: &Path,
    ao_progresso: &(impl Fn(u64, u64) + Send + Sync),
) -> Result<Result<(), String>, Recusa> {
    let ja = std::fs::metadata(parcial).map_or(0, |m| m.len());
    let fluxo = match tokio::time::timeout(
        Duration::from_secs(10),
        TcpStream::connect(("127.0.0.1", pedido.porta_local)),
    )
    .await
    {
        Ok(Ok(fluxo)) => fluxo,
        Ok(Err(erro)) => return Ok(Err(format!("não ligou na porta local: {erro}"))),
        Err(_) => return Ok(Err("não ligou na porta local em 10 s".to_owned())),
    };
    let (leitura, mut escrita) = tokio::io::split(fluxo);
    let linha = format!("{FALA} {} {} {ja}\n", pedido.codigo, pedido.sha256);
    if let Err(erro) = escrita.write_all(linha.as_bytes()).await {
        return Ok(Err(format!("o pedido não saiu: {erro}")));
    }
    let mut leitor = BufReader::new(leitura);
    let mut resposta = String::new();
    match tokio::time::timeout(
        PRAZO_SEM_BYTES_DA_COPIA,
        (&mut leitor)
            .take(MAX_LINHA as u64)
            .read_line(&mut resposta),
    )
    .await
    {
        Ok(Ok(n)) if n > 0 && resposta.ends_with('\n') => {}
        Ok(Ok(_)) => return Ok(Err("a ligação fechou antes da resposta".to_owned())),
        Ok(Err(erro)) => return Ok(Err(format!("a resposta não chegou: {erro}"))),
        Err(_) => return Ok(Err("sem resposta no prazo".to_owned())),
    }
    let resposta = resposta.trim_end();
    if let Some(motivo) = resposta.strip_prefix("RECUSA ") {
        return Err(Recusa::CopiaRecusada {
            motivo: motivo.chars().take(64).collect(),
        });
    }
    match resposta
        .strip_prefix("OK ")
        .and_then(|n| n.parse::<u64>().ok())
    {
        Some(tamanho) if tamanho == pedido.tamanho => {}
        Some(_) => {
            return Err(Recusa::CopiaRecusada {
                motivo: "tamanho-diferente".to_owned(),
            })
        }
        None => {
            return Err(Recusa::CopiaRecusada {
                motivo: "fala-errada".to_owned(),
            })
        }
    }
    let disco = |erro: std::io::Error| Recusa::Disco {
        caminho: parcial.to_path_buf(),
        motivo: erro.to_string(),
    };
    let mut saida = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(parcial)
        .await
        .map_err(disco)?;
    let mut recebidos = ja;
    let mut pedaco = vec![0_u8; PEDACO];
    while recebidos < pedido.tamanho {
        let quero =
            usize::try_from((pedido.tamanho - recebidos).min(PEDACO as u64)).unwrap_or(PEDACO);
        let Some(buf) = pedaco.get_mut(..quero) else {
            break;
        };
        let n = match tokio::time::timeout(PRAZO_SEM_BYTES_DA_COPIA, leitor.read(buf)).await {
            Ok(Ok(0)) => {
                saida.flush().await.map_err(disco)?;
                return Ok(Err(format!(
                    "a ligação fechou em {recebidos} de {} bytes",
                    pedido.tamanho
                )));
            }
            Ok(Ok(n)) => n,
            Ok(Err(erro)) => {
                saida.flush().await.map_err(disco)?;
                return Ok(Err(format!("a ligação caiu em {recebidos} bytes: {erro}")));
            }
            Err(_) => {
                saida.flush().await.map_err(disco)?;
                return Ok(Err(format!(
                    "{} s sem byte nenhum, em {recebidos} bytes",
                    PRAZO_SEM_BYTES_DA_COPIA.as_secs()
                )));
            }
        };
        saida
            .write_all(pedaco.get(..n).unwrap_or_default())
            .await
            .map_err(disco)?;
        recebidos += n as u64;
        ao_progresso(recebidos, pedido.tamanho);
    }
    saida.flush().await.map_err(disco)?;
    saida.sync_all().await.map_err(disco)?;
    Ok(Ok(()))
}

async fn receber(
    interior: Arc<Interior>,
    pedido: PedidoDeCopia,
    ao_progresso: impl Fn(u64, u64) + Send + Sync + 'static,
) -> Result<CopiaRecebida, Recusa> {
    if interior.encerrado.load(Ordering::SeqCst) {
        return Err(Recusa::Encerrado);
    }
    let sha = pedido.sha256.to_ascii_lowercase();
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Recusa::HashMalFormado {
            algoritmo: super::AlgoritmoDeHash::Sha256,
            hex: pedido.sha256.clone(),
        });
    }
    let pedido = PedidoDeCopia {
        sha256: sha,
        ..pedido
    };
    let alvo = caminho::dentro_da_raiz(&interior.raiz, &pedido.destino, false)?;
    let pasta = alvo.parent().unwrap_or(&interior.raiz).to_path_buf();
    std::fs::create_dir_all(&pasta).map_err(|erro| Recusa::Disco {
        caminho: pasta.clone(),
        motivo: erro.to_string(),
    })?;
    caminho::dentro_da_raiz(&interior.raiz, &pedido.destino, false)?;
    let parcial = parcial_de(&alvo, &pedido.sha256);
    // Os parciais de outras cópias deste destino são de cópias que não vêm
    // mais: saem, e não ficam ocupando o disco.
    if let Ok(entradas) = std::fs::read_dir(&pasta) {
        let nome = alvo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for entrada in entradas.flatten() {
            let outro = entrada.path();
            let e_deste = outro.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with(&format!(".{nome}.")) && n.ends_with(EXTENSAO_RECEBENDO)
            });
            if e_deste && outro != parcial {
                let _ = std::fs::remove_file(&outro);
            }
        }
    }
    let mut ja_estavam = std::fs::metadata(&parcial).map_or(0, |m| m.len());
    if ja_estavam > pedido.tamanho {
        let _ = std::fs::remove_file(&parcial);
        ja_estavam = 0;
    }
    // O disco: o que falta chegar, contra o que sobra no teto, com o destino
    // que esta cópia substitui contado de volta.
    {
        let reservado = interior.reservado.lock().map_or(0, |r| *r);
        let usado =
            caminho::ocupado(&interior.raiz, &super::baixar::e_parcial).saturating_add(reservado);
        let substitui = std::fs::symlink_metadata(&alvo)
            .ok()
            .filter(std::fs::Metadata::is_file)
            .map_or(0, |m| m.len());
        let pedido_de_disco = (pedido.tamanho - ja_estavam).saturating_sub(substitui);
        let teto = interior.politica.teto_de_disco;
        if usado.saturating_add(pedido_de_disco) > teto {
            return Err(Recusa::DiscoCheio {
                usado,
                pedido: pedido_de_disco,
                teto,
            });
        }
    }
    let mut tentativas = 0_u32;
    let mut retomadas = 0_u32;
    loop {
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        tentativas += 1;
        match uma_ligacao(&pedido, &parcial, &ao_progresso).await? {
            Ok(()) => break,
            Err(motivo) => {
                let recebidos = std::fs::metadata(&parcial).map_or(0, |m| m.len());
                tracing::warn!(tentativa = tentativas, recebidos, tamanho = pedido.tamanho, motivo = %motivo, "cópia do mundo: a ligação caiu; tenta de novo de onde parou");
                if tentativas >= MAX_TENTATIVAS_DA_COPIA {
                    return Err(Recusa::CopiaCaiu {
                        recebidos,
                        tamanho: pedido.tamanho,
                        tentativas,
                        motivo,
                    });
                }
                retomadas += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    let conferir = parcial.clone();
    let obtido = tokio::task::spawn_blocking(move || sha256_de(&conferir))
        .await
        .map_err(|_| Recusa::Encerrado)?
        .map_err(|erro| Recusa::Disco {
            caminho: parcial.clone(),
            motivo: erro.to_string(),
        })?;
    if obtido != pedido.sha256 {
        let _ = std::fs::remove_file(&parcial);
        return Err(Recusa::CopiaNaoBate {
            esperado: pedido.sha256,
            obtido,
        });
    }
    std::fs::rename(&parcial, &alvo).map_err(|erro| Recusa::Disco {
        caminho: alvo.clone(),
        motivo: erro.to_string(),
    })?;
    tracing::info!(destino = %alvo.display(), tamanho = pedido.tamanho, retomadas, ja_estavam, "cópia do mundo: recebida e conferida");
    Ok(CopiaRecebida {
        destino: alvo,
        tamanho: pedido.tamanho,
        sha256: pedido.sha256,
        retomadas,
        ja_estavam,
    })
}
