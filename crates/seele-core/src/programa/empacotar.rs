//! `maquina.programa.empacotar`: uma pasta do servidor de jogo num `.tar.gz`
//! dentro dela (API 8 parte 7, a reserva).
//!
//! **Por que o produto:** a reserva recebe o mundo da principal, e a metade
//! de máquina, num QuickJS sem disco, não lê uma pasta nem escreve um pacote.
//! É o inverso de [`super::Programa::desempacotar`], com as mesmas réguas:
//!
//! - a origem e o pacote ficam **dentro da raiz**, e o pacote **fora da
//!   origem** (ele se leria a si mesmo enquanto cresce);
//! - **links não entram** (um link no mundo apontaria, na reserva, para onde
//!   ele quisesse); são contados em [`Empacotado::links_pulados`];
//! - **um arquivo que o jogo trava fica de fora, contado e nomeado**
//!   ([`Empacotado::travados_pulados`], decisão W7): no Windows, o Minecraft
//!   segura o `world/session.lock` com uma trava de intervalo enquanto roda, e
//!   ler esse arquivo falha (`ERROR_LOCK_VIOLATION`, 33); no Mac e no Linux a
//!   trava é só um aviso, e o arquivo entra. Uma leitura que falha por outra
//!   razão para tudo, com o caminho **do arquivo lido**;
//! - o que se escreve conta no **teto de disco**, e passar dele para no meio;
//! - o pacote é escrito ao lado, num parcial, e só vira o nome pedido no fim.
//!
//! **O que não é daqui:** salvar o mundo antes. Um mundo que o jogo escreve
//! enquanto se empacota sai torto; a receita diz como parar a escrita
//! (`save-off`, `save-all flush`) e a metade de máquina a segue antes de
//! chamar isto (spec, «A reserva»).
//!
//! As entradas levam o nome da última parte da origem na frente
//! (`world/level.dat`): a reserva abre com `raiz: "world"`, e o que sai é o
//! mundo, sem prefixo.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use sha2::Digest as _;

use super::{caminho, Programa, Recusa};

/// Um pacote feito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Empacotado {
    /// O pacote, na raiz.
    pub arquivo: PathBuf,
    /// O tamanho do pacote, em bytes.
    pub tamanho: u64,
    /// O sha256 do pacote, em hex minúsculo: é o que a reserva confere.
    pub sha256: String,
    /// Quantos arquivos entraram.
    pub arquivos: u64,
    /// Quantos bytes eles somam, antes de comprimir.
    pub bytes: u64,
    /// Quantos links da origem ficaram de fora.
    pub links_pulados: u64,
    /// **Quantos arquivos travados pelo jogo ficaram de fora** (decisão W7): no
    /// Windows, o `session.lock` do mundo enquanto o Minecraft roda.
    pub travados_pulados: u64,
    /// Os primeiros [`MAX_TRAVADOS_NOMEADOS`] deles, como entrariam no pacote
    /// (`world/session.lock`).
    pub travados: Vec<String>,
}

/// Quantos arquivos travados [`Empacotado::travados`] nomeia.
pub const MAX_TRAVADOS_NOMEADOS: usize = 16;

/// Se `erro` é a trava de outro processo sobre o arquivo: no Windows,
/// `ERROR_LOCK_VIOLATION` (33) ou `ERROR_SHARING_VIOLATION` (32). No Unix, uma
/// trava não impede a leitura, e isto nunca é verdade.
fn e_trava(erro: &std::io::Error) -> bool {
    cfg!(windows) && matches!(erro.raw_os_error(), Some(32 | 33))
}

/// Um arquivo da origem, lido para o pacote: um erro de leitura sai marcado
/// ([`Leitura`]), para a recusa nomear o arquivo lido, e não o pacote.
struct Lido(std::fs::File);

/// O erro de ler um arquivo da origem.
#[derive(Debug)]
struct Leitura(std::io::Error);

impl std::fmt::Display for Leitura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Leitura {}

impl std::io::Read for Lido {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .read(buf)
            .map_err(|e| std::io::Error::new(e.kind(), Leitura(e)))
    }
}

/// O que se escreve: conta os bytes contra o livre do teto e soma o sha256.
struct Contado<'a> {
    arquivo: std::fs::File,
    escritos: u64,
    livre: u64,
    sha: sha2::Sha256,
    encerrado: &'a std::sync::atomic::AtomicBool,
}

impl Write for Contado<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.encerrado.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("encerrado"));
        }
        let depois = self.escritos.saturating_add(buf.len() as u64);
        if depois > self.livre {
            return Err(std::io::Error::other(Teto));
        }
        let n = self.arquivo.write(buf)?;
        self.sha.update(buf.get(..n).unwrap_or_default());
        self.escritos = self.escritos.saturating_add(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.arquivo.flush()
    }
}

/// A marca de que o teto de disco acabou no meio da escrita.
#[derive(Debug)]
struct Teto;

impl std::fmt::Display for Teto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("teto de disco")
    }
}

impl std::error::Error for Teto {}

/// O parcial: apagado ao ser solto, a menos que tenha virado o pacote.
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

impl Programa {
    /// Empacota a pasta `origem` em `arquivo` (`.tar.gz`), as duas relativas
    /// à raiz do servidor de jogo.
    ///
    /// **Bloqueia** enquanto lê e escreve (um mundo vai a alguns GiB): quem
    /// chama de um runtime o chama por `spawn_blocking`. Para entre dois
    /// arquivos quando o [`Programa`] é encerrado.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], as do caminho, [`Recusa::NaoEPasta`],
    /// [`Recusa::ArquivoDentroDaOrigem`], [`Recusa::DiscoCheio`] e
    /// [`Recusa::Disco`]. Toda recusa é contada.
    pub fn empacotar(&self, origem: &str, arquivo: &str) -> Result<Empacotado, Recusa> {
        self.empacotar_sem_contar(origem, arquivo)
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    fn empacotar_sem_contar(&self, origem: &str, arquivo: &str) -> Result<Empacotado, Recusa> {
        let interior = &self.interior;
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let pasta_de_origem = caminho::dentro_da_raiz(&interior.raiz, origem, false)?;
        let alvo = caminho::dentro_da_raiz(&interior.raiz, arquivo, false)?;
        interior.presos.conferir(
            &interior.raiz,
            &alvo,
            arquivo,
            super::reservado::Escrita::Troca,
        )?;
        if !std::fs::symlink_metadata(&pasta_de_origem).is_ok_and(|m| m.is_dir()) {
            return Err(Recusa::NaoEPasta {
                origem: origem.to_owned(),
            });
        }
        if alvo.starts_with(&pasta_de_origem) {
            return Err(Recusa::ArquivoDentroDaOrigem {
                arquivo: arquivo.to_owned(),
                origem: origem.to_owned(),
            });
        }
        let disco = |caminho: &Path| {
            let caminho = caminho.to_path_buf();
            move |erro: std::io::Error| Recusa::Disco {
                caminho,
                motivo: erro.to_string(),
            }
        };
        // O livre do teto, medido uma vez (a régua de `desempacotar`, Q8), com
        // o pacote velho de mesmo nome contado de volta: ele sai no fim.
        let livre = {
            let reservado = interior.reservado.lock().map_or(0, |r| *r);
            let usado = caminho::ocupado(&interior.raiz, &|c| interior.parcial_vivo(c))
                .saturating_add(reservado);
            let substitui = std::fs::symlink_metadata(&alvo)
                .ok()
                .filter(std::fs::Metadata::is_file)
                .map_or(0, |m| m.len());
            interior
                .politica
                .teto_de_disco
                .saturating_sub(usado.saturating_sub(substitui))
        };
        let pasta = alvo.parent().unwrap_or(&interior.raiz).to_path_buf();
        std::fs::create_dir_all(&pasta).map_err(disco(&pasta))?;
        caminho::dentro_da_raiz(&interior.raiz, arquivo, false)?;
        let nome = alvo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut parcial = Parcial {
            caminho: pasta.join(format!(
                ".{nome}.{}.{}",
                interior.proximo_numero(),
                super::baixar::EXTENSAO_PARCIAL
            )),
            manter: false,
        };
        let saida = Contado {
            arquivo: std::fs::File::create(&parcial.caminho).map_err(disco(&parcial.caminho))?,
            escritos: 0,
            livre,
            sha: sha2::Sha256::new(),
            encerrado: &interior.encerrado,
        };
        let prefixo = pasta_de_origem
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "pasta".to_owned());
        let mut feito = Empacotado {
            arquivo: alvo.clone(),
            tamanho: 0,
            sha256: String::new(),
            arquivos: 0,
            bytes: 0,
            links_pulados: 0,
            travados_pulados: 0,
            travados: Vec::new(),
        };
        let gz = flate2::write::GzEncoder::new(saida, flate2::Compression::fast());
        let mut pacote = tar::Builder::new(gz);
        pacote.follow_symlinks(false);
        let erro_de_escrita = |erro: std::io::Error, tamanho: u64| -> Recusa {
            if erro.get_ref().is_some_and(|e| e.is::<Teto>()) {
                Recusa::DiscoCheio {
                    usado: tamanho,
                    pedido: 0,
                    teto: livre,
                }
            } else if interior.encerrado.load(Ordering::SeqCst) {
                Recusa::Encerrado
            } else {
                Recusa::Disco {
                    caminho: alvo.clone(),
                    motivo: erro.to_string(),
                }
            }
        };
        // Em ordem, para dois pacotes do mesmo mundo saírem iguais.
        let mut pendentes: Vec<(PathBuf, String)> = vec![(pasta_de_origem.clone(), prefixo)];
        while let Some((dir, nome_no_pacote)) = pendentes.pop() {
            let mut entradas: Vec<_> = std::fs::read_dir(&dir)
                .map_err(disco(&dir))?
                .flatten()
                .collect();
            entradas.sort_by_key(std::fs::DirEntry::file_name);
            pacote
                .append_dir(&nome_no_pacote, &dir)
                .map_err(|e| erro_de_escrita(e, feito.bytes))?;
            for entrada in entradas.into_iter().rev() {
                if interior.encerrado.load(Ordering::SeqCst) {
                    return Err(Recusa::Encerrado);
                }
                let caminho = entrada.path();
                // O parcial deste pacote nunca está na origem (conferido
                // acima), mas um parcial de outro download pode estar.
                if super::baixar::e_parcial(&caminho) {
                    continue;
                }
                let Ok(meta) = std::fs::symlink_metadata(&caminho) else {
                    continue;
                };
                let dentro = format!("{nome_no_pacote}/{}", entrada.file_name().to_string_lossy());
                if meta.file_type().is_symlink() {
                    feito.links_pulados += 1;
                } else if meta.is_dir() {
                    pendentes.push((caminho, dentro));
                } else if meta.is_file() {
                    let aberto = match std::fs::File::open(&caminho) {
                        Ok(aberto) => aberto,
                        Err(erro) if e_trava(&erro) => {
                            pular_travado(&mut feito, &dentro, &erro);
                            continue;
                        }
                        Err(erro) => return Err(disco(&caminho)(erro)),
                    };
                    // **A trava se vê antes do cabeçalho** (W7): ler o
                    // primeiro byte; com o cabeçalho já escrito, um arquivo
                    // que não se lê deixaria o pacote torto.
                    if meta.len() > 0 {
                        let mut um = [0_u8; 1];
                        match std::io::Read::read(&mut &aberto, &mut um) {
                            Ok(_) => {}
                            Err(erro) if e_trava(&erro) => {
                                pular_travado(&mut feito, &dentro, &erro);
                                continue;
                            }
                            Err(erro) => return Err(disco(&caminho)(erro)),
                        }
                        std::io::Seek::rewind(&mut &aberto).map_err(disco(&caminho))?;
                    }
                    // O cabeçalho do `append_file`, com o leitor que marca
                    // o erro de leitura.
                    let mut cabecalho = tar::Header::new_gnu();
                    cabecalho.set_metadata(&meta);
                    pacote
                        .append_data(&mut cabecalho, &dentro, Lido(aberto))
                        .map_err(|e| {
                            if e.get_ref().is_some_and(|e| e.is::<Leitura>()) {
                                disco(&caminho)(e)
                            } else {
                                erro_de_escrita(e, feito.bytes)
                            }
                        })?;
                    feito.arquivos += 1;
                    feito.bytes = feito.bytes.saturating_add(meta.len());
                }
            }
        }
        let gz = pacote
            .into_inner()
            .map_err(|e| erro_de_escrita(e, feito.bytes))?;
        let saida = gz.finish().map_err(|e| erro_de_escrita(e, feito.bytes))?;
        saida.arquivo.sync_all().map_err(disco(&parcial.caminho))?;
        feito.tamanho = saida.escritos;
        let mut sha = String::with_capacity(64);
        for byte in saida.sha.finalize() {
            use std::fmt::Write as _;
            let _ = write!(sha, "{byte:02x}");
        }
        feito.sha256 = sha;
        drop(saida.arquivo);
        std::fs::rename(&parcial.caminho, &alvo).map_err(disco(&alvo))?;
        parcial.manter = true;
        tracing::info!(
            origem = %pasta_de_origem.display(),
            arquivo = %alvo.display(),
            tamanho = feito.tamanho,
            arquivos = feito.arquivos,
            bytes = feito.bytes,
            links_pulados = feito.links_pulados,
            travados_pulados = feito.travados_pulados,
            travados = ?feito.travados,
            "programa empacotou"
        );
        Ok(feito)
    }
}

/// Um arquivo travado pelo jogo fica de fora: contado, nomeado (até o teto) e
/// dito no log.
fn pular_travado(feito: &mut Empacotado, dentro: &str, erro: &std::io::Error) {
    feito.travados_pulados += 1;
    if feito.travados.len() < MAX_TRAVADOS_NOMEADOS {
        feito.travados.push(dentro.to_owned());
    }
    tracing::warn!(arquivo = dentro, %erro, "programa: um arquivo travado pelo jogo ficou fora do pacote");
}
