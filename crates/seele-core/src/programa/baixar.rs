//! `programa.baixar`: um artefato de centenas de MB, em segundo plano, com
//! hash e tamanho declarados antes.
//!
//! # A ordem, e por que cada coisa está onde está
//!
//! 1. **Antes de qualquer pedido** ([`super::Programa::baixar`]): `https`, o
//!    host dentro dos domínios aceitos, o hash declarado e bem formado, o
//!    `destino` dentro da raiz. Uma URL fora do aceite não chega a tocar a rede.
//! 2. **Antes do primeiro byte**: o teto de disco, com o tamanho declarado
//!    reservado até o download acabar — dois downloads ao mesmo tempo não
//!    cabem os dois só porque cada um olhou o disco sozinho.
//! 3. **A cada salto**: um redirecionamento é conferido como a URL de origem
//!    (`https` e domínio aceito). As origens reais redirecionam para CDN (o
//!    GitHub manda para `objects.githubusercontent.com`), então seguir é
//!    preciso; seguir sem conferir faria do domínio aceito um trampolim.
//! 4. **Enquanto chega**: o corpo é cortado no primeiro pedaço que passa do
//!    tamanho declarado, e não depois de ele chegar inteiro. O `Content-Length`
//!    que a origem alega e que difere do declarado recusa antes de ler.
//! 5. **No fim**: o hash do algoritmo mais forte declarado. O arquivo é escrito
//!    num parcial ao lado do destino e só renomeado se tudo bateu; qualquer
//!    outro fim apaga o parcial, inclusive o download cancelado no meio.
//!
//! # Origem injetável
//!
//! A rede entra por [`Origem`]. O produto usa [`OrigemHttps`] (o `reqwest`, sem
//! seguir redirecionamento sozinho, só `https`); os testes de unidade usam uma
//! origem de mentira que conta os pedidos e os pedaços entregues, e os de
//! integração (`tests/programa_https.rs`) um servidor HTTPS em `127.0.0.1`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::io::AsyncWriteExt as _;
use tokio::sync::mpsc;

use super::hash::{o_mais_forte, Calculo, HashDeclarado};
use super::{caminho, host_aceito, AlgoritmoDeHash, Interior, Politica, Programa, Recusa};

/// Quantos redirecionamentos um download segue.
pub const MAX_REDIRECIONAMENTOS: usize = 5;

/// De quanto em quanto byte o progresso é avisado (e no começo e no fim).
const PASSO_DO_PROGRESSO: u64 = 1024 * 1024;

/// A extensão do arquivo parcial, que o teto de disco não conta duas vezes.
pub(crate) const EXTENSAO_PARCIAL: &str = "seele-parcial";

/// Um futuro que se pode guardar numa caixa e mandar entre threads.
pub type FuturoDe<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// De onde os bytes vêm. Um pedido por salto: quem segue redirecionamento é
/// este módulo, para conferir cada salto.
pub trait Origem: Send + Sync {
    /// Pede `url` uma vez, sem seguir redirecionamento.
    ///
    /// O erro é texto: ele vai inteiro para [`Recusa::OrigemFalhou`].
    fn pedir<'a>(&'a self, url: &'a reqwest::Url) -> FuturoDe<'a, Result<Resposta, String>>;
}

/// O corpo de uma resposta, pedaço a pedaço.
pub trait Corpo: Send {
    /// O próximo pedaço, ou `None` no fim.
    fn pedaco(&mut self) -> FuturoDe<'_, Result<Option<Vec<u8>>, String>>;
}

/// O que uma origem respondeu a um pedido.
pub enum Resposta {
    /// Um redirecionamento, com o `Location` como veio (pode ser relativo).
    Redireciona {
        /// O `Location`.
        para: String,
    },
    /// Um status que não é sucesso nem redirecionamento com destino.
    Status(u16),
    /// Sucesso: o corpo.
    Corpo {
        /// O `Content-Length`, se a origem disse.
        alegado: Option<u64>,
        /// Os bytes.
        corpo: Box<dyn Corpo>,
    },
}

impl std::fmt::Debug for Resposta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Redireciona { para } => write!(f, "Redireciona({para})"),
            Self::Status(s) => write!(f, "Status({s})"),
            Self::Corpo { alegado, .. } => write!(f, "Corpo(alegado: {alegado:?})"),
        }
    }
}

/// O que o MOD pede para baixar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoDeDownload {
    /// De onde; `https://` e dentro dos domínios aceitos.
    pub url: String,
    /// Os hashes que a origem publica. Vale o mais forte ([`o_mais_forte`]).
    pub hashes: Vec<HashDeclarado>,
    /// O tamanho, em bytes.
    pub tamanho: u64,
    /// Onde gravar, relativo à raiz do servidor de jogo.
    pub destino: String,
}

/// Um download que terminou e bateu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baixado {
    /// Onde ficou.
    pub destino: PathBuf,
    /// Quantos bytes.
    pub tamanho: u64,
    /// Com que foi conferido.
    pub algoritmo: AlgoritmoDeHash,
    /// O hash, em minúsculas.
    pub hash: String,
}

/// O que um download avisa enquanto corre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoDoDownload {
    /// Quanto chegou, de quanto foi declarado.
    Progresso {
        /// Bytes recebidos.
        recebidos: u64,
        /// O tamanho declarado.
        total: u64,
    },
    /// Terminou e bateu. Último evento.
    Pronto(Baixado),
    /// Foi recusado (e contado). Último evento.
    Recusado(Recusa),
}

/// Um download em segundo plano.
///
/// Soltar o `Download` **não** o cancela: ele é do [`Programa`], que o cancela
/// em [`Programa::encerrar_tudo`] ou ao ser solto. Para cancelar este,
/// [`Download::cancelar`].
#[derive(Debug)]
pub struct Download {
    numero: u64,
    eventos: mpsc::UnboundedReceiver<EventoDoDownload>,
    tarefa: tokio::task::AbortHandle,
}

impl Download {
    /// O número deste download no [`Programa`].
    #[must_use]
    pub const fn numero(&self) -> u64 {
        self.numero
    }

    /// O próximo evento; `None` depois do último, ou se foi cancelado.
    pub async fn proximo(&mut self) -> Option<EventoDoDownload> {
        self.eventos.recv().await
    }

    /// Cancela. O parcial é apagado; nenhum evento final vem.
    pub fn cancelar(&self) {
        self.tarefa.abort();
    }

    /// Espera o fim, descartando o progresso.
    ///
    /// # Errors
    ///
    /// A [`Recusa`] do download, ou [`Recusa::Encerrado`] se ele foi
    /// cancelado antes de terminar.
    pub async fn terminar(mut self) -> Result<Baixado, Recusa> {
        while let Some(evento) = self.eventos.recv().await {
            match evento {
                EventoDoDownload::Progresso { .. } => {}
                EventoDoDownload::Pronto(baixado) => return Ok(baixado),
                EventoDoDownload::Recusado(recusa) => return Err(recusa),
            }
        }
        Err(Recusa::Encerrado)
    }
}

/// Lê a URL e confere `https` e o domínio.
fn conferir_url(url: &str, dominios: &[String]) -> Result<reqwest::Url, Recusa> {
    let lida = reqwest::Url::parse(url).map_err(|erro| Recusa::UrlInvalida {
        url: url.to_owned(),
        motivo: erro.to_string(),
    })?;
    if lida.scheme() != "https" {
        return Err(Recusa::NaoEHttps {
            url: url.to_owned(),
        });
    }
    let Some(host) = lida.host_str() else {
        return Err(Recusa::UrlInvalida {
            url: url.to_owned(),
            motivo: "sem host".to_owned(),
        });
    };
    if !host_aceito(host, dominios) {
        return Err(Recusa::DominioForaDoAceito {
            host: host.to_owned(),
            aceitos: dominios.to_vec(),
        });
    }
    Ok(lida)
}

/// **O aceite prende o que foi mostrado** (spec §4; decisão Q1). Uma URL de
/// artefato aceito só baixa com o hash e o tamanho que a tela mostrou; a de
/// um artefato de outro sistema não baixa; qualquer outra só baixa de um host
/// de `origens`.
///
/// O hash comparado é o mais forte do pedido, e tem de ser do mesmo algoritmo
/// do aceito: um `sha1` certo ao lado do `sha256` aceito não vale como o
/// `sha256`.
fn conferir_contra_o_aceite(
    politica: &Politica,
    pedido: &PedidoDeDownload,
    hash: &HashDeclarado,
    url: &reqwest::Url,
) -> Result<(), Recusa> {
    if let Some(aceito) = politica.artefatos.iter().find(|a| a.url == pedido.url) {
        let pedido_hex = hash.hex.to_ascii_lowercase();
        if aceito.hash.algoritmo != hash.algoritmo
            || aceito.hash.hex.to_ascii_lowercase() != pedido_hex
        {
            return Err(Recusa::DiferenteDoAceito {
                url: pedido.url.clone(),
                campo: "hash",
                aceito: format!("{} {}", aceito.hash.algoritmo, aceito.hash.hex),
                pedido: format!("{} {pedido_hex}", hash.algoritmo),
            });
        }
        if aceito.tamanho != pedido.tamanho {
            return Err(Recusa::DiferenteDoAceito {
                url: pedido.url.clone(),
                campo: "tamanho",
                aceito: aceito.tamanho.to_string(),
                pedido: pedido.tamanho.to_string(),
            });
        }
        return Ok(());
    }
    if politica.de_outros_sistemas.contains(&pedido.url) {
        return Err(Recusa::ArtefatoDeOutroSistema {
            url: pedido.url.clone(),
        });
    }
    if !url
        .host_str()
        .is_some_and(|host| host_aceito(host, &politica.origens))
    {
        return Err(Recusa::ForaDosArtefatosAceitos {
            url: pedido.url.clone(),
            origens: politica.origens.clone(),
        });
    }
    Ok(())
}

impl Programa {
    /// Começa um download em segundo plano.
    ///
    /// O que se confere sem rede (URL, domínio, hash, caminho) é conferido
    /// aqui, e a recusa volta já; o resto chega como evento. Toda recusa é
    /// contada.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], [`Recusa::UrlInvalida`], [`Recusa::NaoEHttps`],
    /// [`Recusa::DominioForaDoAceito`], [`Recusa::SemHash`],
    /// [`Recusa::HashMalFormado`] e as do caminho.
    ///
    /// # Panics
    ///
    /// Fora de um runtime do Tokio.
    pub fn baixar(&self, pedido: PedidoDeDownload) -> Result<Download, Recusa> {
        let interior = &self.interior;
        let conferido = (|| {
            if interior.encerrado.load(Ordering::SeqCst) {
                return Err(Recusa::Encerrado);
            }
            let url = conferir_url(&pedido.url, &interior.politica.dominios)?;
            let hash = o_mais_forte(&pedido.hashes)
                .ok_or_else(|| Recusa::SemHash {
                    url: pedido.url.clone(),
                })?
                .clone();
            if !hash.e_valido() {
                return Err(Recusa::HashMalFormado {
                    algoritmo: hash.algoritmo,
                    hex: hash.hex,
                });
            }
            conferir_contra_o_aceite(&interior.politica, &pedido, &hash, &url)?;
            let destino = caminho::dentro_da_raiz(&interior.raiz, &pedido.destino, false)?;
            Ok((url, hash, destino))
        })();
        let (url, hash, destino) = conferido.map_err(|r| interior.recusar(r))?;

        let numero = interior.proximo_numero();
        let (tx, rx) = mpsc::unbounded_channel();
        let dono = Arc::clone(interior);
        let tarefa = tokio::spawn(async move {
            let fim = correr(&dono, numero, &pedido, url, &hash, &destino, &tx).await;
            if let Ok(mut downloads) = dono.downloads.lock() {
                downloads.remove(&numero);
            }
            let evento = match fim {
                Ok(baixado) => {
                    tracing::info!(
                        destino = %baixado.destino.display(),
                        tamanho = baixado.tamanho,
                        algoritmo = %baixado.algoritmo,
                        "programa baixou"
                    );
                    EventoDoDownload::Pronto(baixado)
                }
                Err(recusa) => EventoDoDownload::Recusado(dono.recusar(recusa)),
            };
            let _ = tx.send(evento);
        });
        let abortar = tarefa.abort_handle();
        if let Ok(mut downloads) = interior.downloads.lock() {
            downloads.insert(numero, abortar.clone());
        }
        // Encerrado entre a conferência e o registro: quem encerrou já esvaziou
        // a lista, e este escaparia dela.
        if interior.encerrado.load(Ordering::SeqCst) {
            abortar.abort();
        }
        Ok(Download {
            numero,
            eventos: rx,
            tarefa: abortar,
        })
    }
}

/// A reserva de disco de um download: solta quando ele acaba, de qualquer jeito.
struct Reserva {
    interior: Arc<Interior>,
    bytes: u64,
}

impl Drop for Reserva {
    fn drop(&mut self) {
        if let Ok(mut reservado) = self.interior.reservado.lock() {
            *reservado = reservado.saturating_sub(self.bytes);
        }
    }
}

/// O parcial: apagado ao ser solto, a menos que tenha virado o destino.
struct Parcial {
    caminho: PathBuf,
    manter: bool,
}

impl Drop for Parcial {
    fn drop(&mut self) {
        if !self.manter {
            let _ = std::fs::remove_file(&self.caminho);
        }
    }
}

pub(crate) fn e_parcial(caminho: &Path) -> bool {
    caminho.extension().is_some_and(|e| e == EXTENSAO_PARCIAL)
}

fn reservar(interior: &Arc<Interior>, destino: &Path, tamanho: u64) -> Result<Reserva, Recusa> {
    // O cadeado da reserva é tomado **antes** de medir o disco: dois downloads
    // que medissem juntos veriam o mesmo espaço livre e caberiam os dois.
    let mut reservado = interior.reservado.lock().map_err(|_| Recusa::Disco {
        caminho: interior.raiz.clone(),
        motivo: "o cadeado da reserva de disco foi envenenado".to_owned(),
    })?;
    let usado = caminho::ocupado(&interior.raiz, &e_parcial).saturating_add(*reservado);
    // Um arquivo que este download substitui sai quando ele entra.
    let substitui = std::fs::symlink_metadata(destino)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .map_or(0, |m| m.len());
    let pedido = tamanho.saturating_sub(substitui);
    let teto = interior.politica.teto_de_disco;
    if usado.saturating_add(pedido) > teto {
        return Err(Recusa::DiscoCheio {
            usado,
            pedido,
            teto,
        });
    }
    *reservado = reservado.saturating_add(tamanho);
    Ok(Reserva {
        interior: Arc::clone(interior),
        bytes: tamanho,
    })
}

/// Quantas vezes um pedido que falhou **antes de qualquer resposta** é
/// tentado (decisão Q10).
pub const MAX_TENTATIVAS: u32 = 3;

/// A espera entre duas tentativas.
const ESPERA_ENTRE_TENTATIVAS: std::time::Duration = std::time::Duration::from_secs(1);

/// **Pede, e tenta de novo quando a falha vem antes de qualquer resposta**
/// (conexão, TLS): até [`MAX_TENTATIVAS`]. Medido em 31/10/2026, neste Mac,
/// contra `piston-data.mojang.com` (Azure Front Door): o primeiro pedido de
/// cada processo leva de 3 a 5 s, e alguns morrem em 9 a 10 s com «peer closed
/// connection without sending TLS close_notify»; o segundo leva ~80 ms. Uma
/// resposta (inclusive um status de erro) não é tentada de novo, e uma falha
/// no meio do corpo também não: o parcial é recusado e apagado.
async fn pedir_com_tentativas(origem: &dyn Origem, url: &reqwest::Url) -> Result<Resposta, Recusa> {
    let mut motivos = Vec::new();
    for tentativa in 1..=MAX_TENTATIVAS {
        match origem.pedir(url).await {
            Ok(resposta) => {
                if tentativa > 1 {
                    tracing::info!(url = %url, tentativa, antes = ?motivos, "programa: o pedido passou depois de falhar");
                }
                return Ok(resposta);
            }
            Err(motivo) => {
                tracing::warn!(url = %url, tentativa, de = MAX_TENTATIVAS, motivo = %motivo, "programa: o pedido falhou antes de qualquer resposta");
                motivos.push(motivo);
                if tentativa < MAX_TENTATIVAS {
                    tokio::time::sleep(ESPERA_ENTRE_TENTATIVAS).await;
                }
            }
        }
    }
    Err(Recusa::OrigemFalhou {
        url: url.to_string(),
        motivo: format!("{} tentativas: {}", MAX_TENTATIVAS, motivos.join(" | ")),
    })
}

#[allow(clippy::too_many_lines)]
async fn correr(
    interior: &Arc<Interior>,
    numero: u64,
    pedido: &PedidoDeDownload,
    mut url: reqwest::Url,
    hash: &HashDeclarado,
    destino: &Path,
    tx: &mpsc::UnboundedSender<EventoDoDownload>,
) -> Result<Baixado, Recusa> {
    let tamanho = pedido.tamanho;
    let _reserva = {
        let dono = Arc::clone(interior);
        let destino = destino.to_path_buf();
        tokio::task::spawn_blocking(move || reservar(&dono, &destino, tamanho))
            .await
            .map_err(|erro| Recusa::Disco {
                caminho: interior.raiz.clone(),
                motivo: erro.to_string(),
            })??
    };

    let mut saltos = 0_usize;
    let (alegado, mut corpo) = loop {
        let resposta = pedir_com_tentativas(interior.origem.as_ref(), &url).await?;
        match resposta {
            Resposta::Corpo { alegado, corpo } => break (alegado, corpo),
            Resposta::Status(status) => {
                return Err(Recusa::OrigemRespondeu {
                    url: url.to_string(),
                    status,
                })
            }
            Resposta::Redireciona { para } => {
                saltos += 1;
                if saltos > MAX_REDIRECIONAMENTOS {
                    return Err(Recusa::RedirecionamentosDemais {
                        url: pedido.url.clone(),
                        saltos: MAX_REDIRECIONAMENTOS,
                    });
                }
                let fora = || Recusa::RedirecionouParaFora {
                    de: url.to_string(),
                    para: para.clone(),
                };
                let proxima = url.join(&para).map_err(|_| fora())?;
                if conferir_url(proxima.as_str(), &interior.politica.dominios).is_err() {
                    return Err(fora());
                }
                url = proxima;
            }
        }
    };
    if let Some(alegado) = alegado.filter(|a| *a != tamanho) {
        return Err(Recusa::TamanhoAlegadoDiferente {
            url: url.to_string(),
            declarado: tamanho,
            alegado,
        });
    }

    // A pasta do destino, criada e conferida de novo: criar pode ter passado
    // por um link que apareceu depois da primeira conferência.
    let disco = |caminho: &Path| {
        let caminho = caminho.to_path_buf();
        move |erro: std::io::Error| Recusa::Disco {
            caminho,
            motivo: erro.to_string(),
        }
    };
    let pasta = destino.parent().unwrap_or(&interior.raiz).to_path_buf();
    tokio::fs::create_dir_all(&pasta)
        .await
        .map_err(disco(&pasta))?;
    caminho::dentro_da_raiz(&interior.raiz, &pedido.destino, false)?;

    let nome = destino
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut parcial = Parcial {
        caminho: pasta.join(format!(".{nome}.{numero}.{EXTENSAO_PARCIAL}")),
        manter: false,
    };
    let mut arquivo = tokio::fs::File::create(&parcial.caminho)
        .await
        .map_err(disco(&parcial.caminho))?;

    let mut calculo = Calculo::novo(hash.algoritmo);
    let mut recebidos = 0_u64;
    let mut proximo_aviso = 0_u64;
    loop {
        if recebidos >= proximo_aviso {
            let _ = tx.send(EventoDoDownload::Progresso {
                recebidos,
                total: tamanho,
            });
            proximo_aviso = recebidos.saturating_add(PASSO_DO_PROGRESSO);
        }
        let Some(pedaco) = corpo
            .pedaco()
            .await
            .map_err(|motivo| Recusa::OrigemFalhou {
                url: url.to_string(),
                motivo,
            })?
        else {
            break;
        };
        let depois = recebidos.saturating_add(pedaco.len() as u64);
        // O corte, no pedaço que passa — e antes de escrevê-lo.
        if depois > tamanho {
            return Err(Recusa::MaiorQueODeclarado {
                url: url.to_string(),
                declarado: tamanho,
                recebido: depois,
            });
        }
        arquivo
            .write_all(&pedaco)
            .await
            .map_err(disco(&parcial.caminho))?;
        calculo.somar(&pedaco);
        recebidos = depois;
    }
    drop(corpo);
    if recebidos < tamanho {
        return Err(Recusa::MenorQueODeclarado {
            url: url.to_string(),
            declarado: tamanho,
            recebido: recebidos,
        });
    }
    let _ = tx.send(EventoDoDownload::Progresso {
        recebidos,
        total: tamanho,
    });
    arquivo.flush().await.map_err(disco(&parcial.caminho))?;
    arquivo.sync_all().await.map_err(disco(&parcial.caminho))?;
    drop(arquivo);

    let obtido = calculo.hex();
    let esperado = hash.hex.to_ascii_lowercase();
    if obtido != esperado {
        return Err(Recusa::HashNaoBate {
            url: url.to_string(),
            algoritmo: hash.algoritmo,
            esperado,
            obtido,
        });
    }
    tokio::fs::rename(&parcial.caminho, destino)
        .await
        .map_err(disco(destino))?;
    parcial.manter = true;
    Ok(Baixado {
        destino: destino.to_path_buf(),
        tamanho,
        algoritmo: hash.algoritmo,
        hash: obtido,
    })
}

/// A origem do produto: `reqwest`, só `https`, sem seguir redirecionamento.
#[derive(Debug, Clone)]
pub struct OrigemHttps {
    cliente: reqwest::Client,
}

impl OrigemHttps {
    /// A origem com as raízes de certificado do sistema.
    ///
    /// # Errors
    ///
    /// O texto do `reqwest` se o cliente não se constrói.
    pub fn nova() -> Result<Self, String> {
        Self::construir(None)
    }

    /// A origem que confia **só** neste certificado (DER). Para o teste com um
    /// servidor HTTPS local, e para mais nada.
    ///
    /// # Errors
    ///
    /// O texto do `reqwest` se o certificado ou o cliente não se constroem.
    pub fn confiando_so_em(der: &[u8]) -> Result<Self, String> {
        Self::construir(Some(der))
    }

    fn construir(der: Option<&[u8]>) -> Result<Self, String> {
        // O provedor de criptografia, pelo idioma do workspace: `let _` torna
        // a instalação idempotente, e sem ela o `rustls-no-provider` entra em
        // pânico ao construir o cliente.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut construtor = reqwest::Client::builder()
            .https_only(true)
            // Quem segue é `correr`, para conferir cada salto contra o aceite.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(15))
            // Sem teto total — são centenas de MB numa rede qualquer —, mas
            // com teto de silêncio: uma origem que para de mandar não prende o
            // download para sempre.
            .read_timeout(std::time::Duration::from_secs(60));
        if let Some(der) = der {
            let certificado = reqwest::Certificate::from_der(der).map_err(|e| erro_inteiro(&e))?;
            construtor = construtor.tls_certs_only([certificado]);
        }
        let cliente = construtor.build().map_err(|e| erro_inteiro(&e))?;
        Ok(Self { cliente })
    }
}

/// O erro com a cadeia de causas: o `Display` do `reqwest` sozinho diz
/// «error sending request», e a causa (DNS, certificado, recusa) é o que a
/// pessoa precisa.
fn erro_inteiro(erro: &dyn std::error::Error) -> String {
    let mut texto = erro.to_string();
    let mut causa = erro.source();
    while let Some(c) = causa {
        texto.push_str(": ");
        texto.push_str(&c.to_string());
        causa = c.source();
    }
    texto
}

struct CorpoHttps(reqwest::Response);

impl Corpo for CorpoHttps {
    fn pedaco(&mut self) -> FuturoDe<'_, Result<Option<Vec<u8>>, String>> {
        Box::pin(async move {
            self.0
                .chunk()
                .await
                .map(|p| p.map(Vec::from))
                .map_err(|e| erro_inteiro(&e))
        })
    }
}

impl Origem for OrigemHttps {
    fn pedir<'a>(&'a self, url: &'a reqwest::Url) -> FuturoDe<'a, Result<Resposta, String>> {
        Box::pin(async move {
            let resposta = self
                .cliente
                .get(url.clone())
                .send()
                .await
                .map_err(|e| erro_inteiro(&e))?;
            let status = resposta.status();
            if status.is_redirection() {
                if let Some(para) = resposta
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                {
                    return Ok(Resposta::Redireciona {
                        para: para.to_owned(),
                    });
                }
            }
            if !status.is_success() {
                return Ok(Resposta::Status(status.as_u16()));
            }
            Ok(Resposta::Corpo {
                alegado: resposta.content_length(),
                corpo: Box::new(CorpoHttps(resposta)),
            })
        })
    }
}

#[cfg(test)]
mod tests;
