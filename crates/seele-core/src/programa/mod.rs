//! O poder `programa` da API 8 no núcleo: baixar com hash e tamanho declarados,
//! e rodar um processo (spec `2026-10-29-api-8-e-mod-multiplayer-design.md`,
//! §5).
//!
//! # O que este módulo é, e o que fica para a cola
//!
//! Tudo aqui recebe uma [`Politica`] explícita: **o que foi aceito** na máquina
//! (os domínios de origem, o comando e os argumentos, o teto de disco, a raiz
//! do servidor de jogo). Este módulo não sabe de onde ela vem. Ligar a
//! política ao registro do aceite (parte 3), os objetos `maquina.programa` do
//! QuickJS, os comandos do Tauri e a casca ficam para depois, e a ideia é que
//! cada um seja fino: um [`Programa`] por servidor de jogo aceito, criado com a
//! política daquele aceite, e jogado fora quando o aceite cai.
//!
//! # O produto sabe e conta
//!
//! Toda recusa é uma [`Recusa`] com os dados que a pessoa precisa (o domínio
//! pedido e os aceitos, o hash esperado e o obtido, o disco usado e o teto),
//! vai ao log com o nome dela, e **é contada** por nome
//! ([`Programa::recusas`]): a spec pede a contagem na tela do MOD.
//!
//! # Nada fica órfão
//!
//! O [`Programa`] é o dono de todo download e de todo processo que começou.
//! [`Programa::encerrar_tudo`] mata e espera cada um; soltar o `Programa` sem
//! isso manda matar todos ([`Drop`]), e cada filho é criado com
//! `kill_on_drop`, então nem o runtime caindo deixa um para trás.

mod baixar;
mod caminho;
mod hash;
mod rodar;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub use baixar::{
    Baixado, Corpo, Download, EventoDoDownload, FuturoDe, Origem, OrigemHttps, PedidoDeDownload,
    Resposta, MAX_REDIRECIONAMENTOS,
};
pub use caminho::dentro_da_raiz;
pub use hash::{o_mais_forte, AlgoritmoDeHash, HashDeclarado};
pub use rodar::{
    EventoDoProcesso, FimDoProcesso, PedidoDeExecucao, Processo, MAX_BYTES_POR_LINHA,
    MAX_LINHAS_NA_FILA,
};

/// O comando aceito: o programa, relativo à raiz, e os argumentos, um a um.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComandoAceito {
    /// O programa, relativo à raiz do servidor de jogo (por exemplo,
    /// `java/bin/java`). Nunca procurado no `PATH`.
    pub comando: String,
    /// Os argumentos, exatamente como foram mostrados no aceite.
    pub argumentos: Vec<String>,
}

/// O que foi aceito para um servidor de jogo nesta máquina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Politica {
    /// Os domínios de origem. Vale o host exato ou um subdomínio dele
    /// (`mojang.com` aceita `piston-data.mojang.com`).
    pub dominios: Vec<String>,
    /// O comando que pode rodar. `None`: nenhum.
    pub comando: Option<ComandoAceito>,
    /// O teto de disco da raiz, em bytes.
    pub teto_de_disco: u64,
    /// A pasta deste servidor de jogo, dentro da área do MOD. Criada se não
    /// existir.
    pub raiz: PathBuf,
}

/// Por que o `programa` recusou, com o que a pessoa precisa para entender.
///
/// O nome de cada uma ([`Recusa::nome`]) é de lista fechada, e é o que a tela
/// do MOD conta; a frase é do `Display`, para o log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Recusa {
    /// A URL não é `https://`.
    #[error("{url} não é https: só https é baixado")]
    NaoEHttps {
        /// A URL pedida.
        url: String,
    },
    /// A URL não se lê.
    #[error("{url} não é uma URL: {motivo}")]
    UrlInvalida {
        /// O texto pedido.
        url: String,
        /// O que o leitor de URL disse.
        motivo: String,
    },
    /// O host não é um dos domínios aceitos nem subdomínio de um.
    #[error("{host} está fora dos domínios aceitos ({})", aceitos.join(", "))]
    DominioForaDoAceito {
        /// O host pedido.
        host: String,
        /// Os domínios do aceite.
        aceitos: Vec<String>,
    },
    /// Um redirecionamento levou para fora dos domínios aceitos, ou para fora
    /// do `https`.
    #[error("{de} redirecionou para {para}, fora do aceito")]
    RedirecionouParaFora {
        /// De onde.
        de: String,
        /// Para onde.
        para: String,
    },
    /// Redirecionamentos demais.
    #[error("{url} redirecionou mais de {saltos} vezes")]
    RedirecionamentosDemais {
        /// A URL pedida.
        url: String,
        /// O teto ([`MAX_REDIRECIONAMENTOS`]).
        saltos: usize,
    },
    /// O pedido não declarou hash nenhum.
    #[error("{url} sem hash declarado: todo download tem hash e tamanho antes")]
    SemHash {
        /// A URL pedida.
        url: String,
    },
    /// O hash declarado não tem o tamanho do algoritmo, ou não é hex.
    #[error("o {algoritmo} declarado não é um {algoritmo} ({hex})")]
    HashMalFormado {
        /// O algoritmo declarado.
        algoritmo: AlgoritmoDeHash,
        /// O texto declarado.
        hex: String,
    },
    /// Caminho absoluto.
    #[error("{caminho} é absoluto: tudo é relativo à pasta do servidor de jogo")]
    CaminhoAbsoluto {
        /// O caminho pedido.
        caminho: String,
    },
    /// Caminho com `..`.
    #[error("{caminho} tem «..»: tudo fica dentro da pasta do servidor de jogo")]
    CaminhoComPai {
        /// O caminho pedido.
        caminho: String,
    },
    /// Caminho vazio onde é preciso um nome.
    #[error("o destino está vazio")]
    CaminhoVazio,
    /// Um pedaço do caminho que não é nome (`.`, uma letra de unidade no meio).
    #[error("{caminho} tem um pedaço que não é nome")]
    CaminhoInvalido {
        /// O caminho pedido.
        caminho: String,
    },
    /// Um link no caminho aponta para fora da raiz.
    #[error("{caminho} passa por um link que leva a {}", aponta_para.display())]
    LinkSaiDaRaiz {
        /// O caminho pedido.
        caminho: String,
        /// Onde o link cai.
        aponta_para: PathBuf,
    },
    /// O download não cabe no teto de disco.
    #[error("não cabe: a pasta usa {usado} bytes, o download pede {pedido}, e o teto é {teto}")]
    DiscoCheio {
        /// O que a raiz ocupa, mais o que os downloads em andamento reservaram.
        usado: u64,
        /// O que este download acrescenta (o tamanho declarado, menos o
        /// arquivo que ele substitui).
        pedido: u64,
        /// O teto do aceite.
        teto: u64,
    },
    /// A origem alegou (`Content-Length`) um tamanho diferente do declarado.
    #[error("{url}: o MOD declarou {declarado} bytes e a origem diz {alegado}")]
    TamanhoAlegadoDiferente {
        /// A URL.
        url: String,
        /// O que o MOD declarou.
        declarado: u64,
        /// O que a origem alegou.
        alegado: u64,
    },
    /// O corpo passou do tamanho declarado, e foi cortado ali.
    #[error("{url}: o corpo passou dos {declarado} bytes declarados e foi cortado em {recebido}")]
    MaiorQueODeclarado {
        /// A URL.
        url: String,
        /// O que o MOD declarou.
        declarado: u64,
        /// Quanto tinha chegado quando o corte aconteceu.
        recebido: u64,
    },
    /// O corpo acabou antes do tamanho declarado.
    #[error("{url}: o MOD declarou {declarado} bytes e chegaram {recebido}")]
    MenorQueODeclarado {
        /// A URL.
        url: String,
        /// O que o MOD declarou.
        declarado: u64,
        /// O que chegou.
        recebido: u64,
    },
    /// O hash não bate. O arquivo foi apagado.
    #[error("{url}: o {algoritmo} esperado era {esperado} e o obtido foi {obtido}; o arquivo foi apagado")]
    HashNaoBate {
        /// A URL.
        url: String,
        /// O algoritmo conferido (o mais forte declarado).
        algoritmo: AlgoritmoDeHash,
        /// O hash declarado, em minúsculas.
        esperado: String,
        /// O hash calculado.
        obtido: String,
    },
    /// A origem respondeu com um status que não é sucesso.
    #[error("{url} respondeu {status}")]
    OrigemRespondeu {
        /// A URL (a do último salto).
        url: String,
        /// O status HTTP.
        status: u16,
    },
    /// A origem não respondeu, ou o corpo parou no meio.
    #[error("{url} falhou: {motivo}")]
    OrigemFalhou {
        /// A URL (a do último salto).
        url: String,
        /// O que a camada de rede disse.
        motivo: String,
    },
    /// O disco recusou (criar a pasta, escrever, renomear).
    #[error("o disco recusou {}: {motivo}", caminho.display())]
    Disco {
        /// Onde.
        caminho: PathBuf,
        /// O que o sistema disse.
        motivo: String,
    },
    /// O comando pedido não é o aceito.
    #[error("{pedido} não é o comando aceito ({})", aceito.as_deref().unwrap_or("nenhum"))]
    ComandoForaDoAceito {
        /// O pedido.
        pedido: String,
        /// O aceito, se algum.
        aceito: Option<String>,
    },
    /// Os argumentos pedidos não são os aceitos.
    #[error("os argumentos pedidos ({pedidos:?}) não são os aceitos ({aceitos:?})")]
    ArgumentosForaDoAceito {
        /// Os pedidos.
        pedidos: Vec<String>,
        /// Os aceitos.
        aceitos: Vec<String>,
    },
    /// O sistema não iniciou o processo.
    #[error("{} não iniciou: {motivo}", comando.display())]
    NaoIniciou {
        /// O caminho do programa.
        comando: PathBuf,
        /// O que o sistema disse.
        motivo: String,
    },
    /// A entrada do processo fechou (ele terminou, ou fechou o `stdin`).
    #[error("a entrada do processo {processo} fechou: {motivo}")]
    EntradaFechada {
        /// O número do processo neste [`Programa`].
        processo: u64,
        /// O que o sistema disse.
        motivo: String,
    },
    /// O [`Programa`] já foi encerrado; nada novo começa.
    #[error("o programa deste servidor de jogo foi encerrado")]
    Encerrado,
}

impl Recusa {
    /// O nome, de lista fechada, como a tela do MOD o conta.
    #[must_use]
    pub const fn nome(&self) -> &'static str {
        match self {
            Self::NaoEHttps { .. } => "nao-e-https",
            Self::UrlInvalida { .. } => "url-invalida",
            Self::DominioForaDoAceito { .. } => "dominio-fora-do-aceito",
            Self::RedirecionouParaFora { .. } => "redirecionou-para-fora",
            Self::RedirecionamentosDemais { .. } => "redirecionamentos-demais",
            Self::SemHash { .. } => "sem-hash",
            Self::HashMalFormado { .. } => "hash-mal-formado",
            Self::CaminhoAbsoluto { .. } => "caminho-absoluto",
            Self::CaminhoComPai { .. } => "caminho-com-pai",
            Self::CaminhoVazio => "caminho-vazio",
            Self::CaminhoInvalido { .. } => "caminho-invalido",
            Self::LinkSaiDaRaiz { .. } => "link-sai-da-raiz",
            Self::DiscoCheio { .. } => "disco-cheio",
            Self::TamanhoAlegadoDiferente { .. } => "tamanho-alegado-diferente",
            Self::MaiorQueODeclarado { .. } => "maior-que-o-declarado",
            Self::MenorQueODeclarado { .. } => "menor-que-o-declarado",
            Self::HashNaoBate { .. } => "hash-nao-bate",
            Self::OrigemRespondeu { .. } => "origem-respondeu",
            Self::OrigemFalhou { .. } => "origem-falhou",
            Self::Disco { .. } => "disco",
            Self::ComandoForaDoAceito { .. } => "comando-fora-do-aceito",
            Self::ArgumentosForaDoAceito { .. } => "argumentos-fora-do-aceito",
            Self::NaoIniciou { .. } => "nao-iniciou",
            Self::EntradaFechada { .. } => "entrada-fechada",
            Self::Encerrado => "encerrado",
        }
    }
}

/// Se `host` é um dos `dominios` ou subdomínio de um.
///
/// Sem diferença de caixa e sem o ponto final (`mojang.com.` é `mojang.com`).
/// `evilmojang.com` **não** é subdomínio de `mojang.com`: a comparação é por
/// rótulo, com o ponto, e não por sufixo de texto.
#[must_use]
pub fn host_aceito(host: &str, dominios: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    dominios.iter().any(|dominio| {
        let dominio = dominio.trim_end_matches('.').to_ascii_lowercase();
        !dominio.is_empty()
            && (host == dominio
                || host
                    .strip_suffix(dominio.as_str())
                    .is_some_and(|antes| antes.ends_with('.')))
    })
}

/// O estado de um [`Programa`], dividido com as tarefas que ele começou.
pub(crate) struct Interior {
    pub(crate) politica: Politica,
    /// A raiz, canônica: é contra ela que um link é comparado.
    pub(crate) raiz: PathBuf,
    pub(crate) origem: Arc<dyn Origem>,
    recusas: Mutex<BTreeMap<&'static str, u64>>,
    /// Bytes declarados dos downloads em andamento, ainda não no disco.
    pub(crate) reservado: Mutex<u64>,
    pub(crate) encerrado: AtomicBool,
    proximo: AtomicU64,
    /// Os downloads em andamento, para encerrar.
    pub(crate) downloads: Mutex<HashMap<u64, tokio::task::AbortHandle>>,
    /// Os processos vivos: o sinal de matar e a tarefa que os vigia.
    pub(crate) processos: Mutex<HashMap<u64, rodar::Vigia>>,
}

impl Interior {
    /// Conta uma recusa, a escreve no log e a devolve.
    pub(crate) fn recusar(&self, recusa: Recusa) -> Recusa {
        tracing::warn!(
            recusa = recusa.nome(),
            raiz = %self.raiz.display(),
            "programa recusou: {recusa}"
        );
        if let Ok(mut contagem) = self.recusas.lock() {
            *contagem.entry(recusa.nome()).or_default() += 1;
        }
        recusa
    }

    pub(crate) fn proximo_numero(&self) -> u64 {
        self.proximo.fetch_add(1, Ordering::Relaxed)
    }
}

/// O poder `programa` de um servidor de jogo aceito.
///
/// Um por aceite. Os métodos que começam algo em segundo plano
/// ([`Programa::baixar`], [`Programa::rodar`]) precisam ser chamados de dentro
/// de um runtime do Tokio.
pub struct Programa {
    pub(crate) interior: Arc<Interior>,
}

impl std::fmt::Debug for Programa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Programa")
            .field("raiz", &self.interior.raiz)
            .finish_non_exhaustive()
    }
}

impl Programa {
    /// Um `programa` com o que foi aceito, baixando por `origem`.
    ///
    /// Cria a raiz se ela não existir.
    ///
    /// # Errors
    ///
    /// [`Recusa::Disco`] se a raiz não pode ser criada nem lida.
    pub fn novo(politica: Politica, origem: Arc<dyn Origem>) -> Result<Self, Recusa> {
        let disco = |erro: std::io::Error| Recusa::Disco {
            caminho: politica.raiz.clone(),
            motivo: erro.to_string(),
        };
        std::fs::create_dir_all(&politica.raiz).map_err(disco)?;
        let raiz = std::fs::canonicalize(&politica.raiz).map_err(disco)?;
        Ok(Self {
            interior: Arc::new(Interior {
                politica,
                raiz,
                origem,
                recusas: Mutex::new(BTreeMap::new()),
                reservado: Mutex::new(0),
                encerrado: AtomicBool::new(false),
                proximo: AtomicU64::new(1),
                downloads: Mutex::new(HashMap::new()),
                processos: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// A raiz, canônica.
    #[must_use]
    pub fn raiz(&self) -> &Path {
        &self.interior.raiz
    }

    /// Quantas recusas de cada nome ([`Recusa::nome`]) houve.
    #[must_use]
    pub fn recusas(&self) -> BTreeMap<&'static str, u64> {
        self.interior
            .recusas
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Quantos processos estão vivos.
    #[must_use]
    pub fn processos_vivos(&self) -> usize {
        self.interior.processos.lock().map(|p| p.len()).unwrap_or(0)
    }

    /// Encerra tudo: cancela cada download (o parcial é apagado), mata cada
    /// processo e **espera** cada um terminar. Depois disso, nada novo começa
    /// ([`Recusa::Encerrado`]).
    ///
    /// Devolve quantos processos foram mortos.
    pub async fn encerrar_tudo(&self) -> usize {
        self.interior.encerrado.store(true, Ordering::SeqCst);
        let downloads: Vec<_> = self
            .interior
            .downloads
            .lock()
            .map(|mut d| d.drain().map(|(_, a)| a).collect())
            .unwrap_or_default();
        for download in downloads {
            download.abort();
        }
        let vigias: Vec<rodar::Vigia> = self
            .interior
            .processos
            .lock()
            .map(|mut p| p.drain().map(|(_, v)| v).collect())
            .unwrap_or_default();
        let mortos = vigias.len();
        for vigia in vigias {
            vigia.matar_e_esperar().await;
        }
        mortos
    }
}

impl Drop for Programa {
    /// Sem `encerrar_tudo`, ao menos manda: cada download é cancelado e cada
    /// processo recebe a ordem de morrer, sem esperar.
    fn drop(&mut self) {
        self.interior.encerrado.store(true, Ordering::SeqCst);
        if let Ok(mut downloads) = self.interior.downloads.lock() {
            for (_, download) in downloads.drain() {
                download.abort();
            }
        }
        if let Ok(processos) = self.interior.processos.lock() {
            for vigia in processos.values() {
                vigia.mandar_matar();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(lista: &[&str]) -> Vec<String> {
        lista.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn host_exato_ou_subdominio_por_rotulo() {
        let aceitos = d(&["mojang.com", "Adoptium.NET."]);
        assert!(host_aceito("mojang.com", &aceitos));
        assert!(host_aceito("piston-data.mojang.com", &aceitos));
        assert!(host_aceito("PISTON-DATA.MOJANG.COM.", &aceitos));
        assert!(host_aceito("api.adoptium.net", &aceitos));
        assert!(!host_aceito("evilmojang.com", &aceitos));
        assert!(!host_aceito("mojang.com.evil.net", &aceitos));
        assert!(!host_aceito("com", &aceitos));
        assert!(!host_aceito("", &aceitos));
        assert!(!host_aceito("mojang.com", &d(&[""])));
    }
}
