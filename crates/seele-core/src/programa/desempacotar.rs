//! `maquina.programa.desempacotar`: abrir um `.tar.gz` ou um `.zip` que já
//! está na pasta do servidor de jogo, dentro dela (API 8 parte 4).
//!
//! **Por que o produto:** o Java da Adoptium vem empacotado (`.tar.gz` no Mac
//! e no Linux, `.zip` no Windows), e a metade de máquina, num QuickJS sem
//! disco, não abre nem um nem outro. Sem isto, o comando aceito
//! (`java/bin/java`) nunca existe.
//!
//! # O que é conferido
//!
//! - o arquivo e o destino ficam **dentro da raiz** (a regra de
//!   [`super::dentro_da_raiz`]); cada entrada do pacote também, e uma entrada
//!   com `..` ou absoluta recusa o pacote inteiro, em vez de ser pulada calada;
//! - só se extrai o que está **sob `raiz`** dentro do pacote (o
//!   `jdk-21.0.5+11-jre/Contents/Home` da Adoptium no Mac), sem esse prefixo;
//! - **links não são criados** (um link dentro do pacote poderia apontar para
//!   fora da pasta); são contados em [`Desempacotado::links_pulados`], e não
//!   calados;
//! - o que se escreve conta no **teto de disco**, e passar dele para no meio;
//! - tudo vai para uma pasta parcial ao lado do destino, e só vira o destino
//!   se o pacote inteiro deu certo: qualquer falha apaga a parcial.
//!
//! # O `.zip`, lido à mão
//!
//! Sem o crate `zip` (e a árvore dele): o diretório central, cabeçalho local,
//! `stored` (0) e `deflate` (8), com o CRC-32 de cada entrada conferido.
//! Zip64, criptografia e os outros métodos são recusados pelo nome
//! ([`Recusa::PacoteInvalido`]).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::reservado::Escrita;
use super::{caminho, Programa, Recusa};

/// Quantas entradas um pacote pode ter: um JRE tem uns 300 arquivos; um
/// modpack com `overrides`, alguns milhares.
pub const MAX_ENTRADAS_DESEMPACOTADAS: u64 = 100_000;

/// O que o MOD pede para desempacotar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoDeDesempacotar {
    /// O pacote, relativo à raiz do servidor de jogo.
    pub arquivo: String,
    /// A pasta onde fica o que estava sob `raiz`, relativa à raiz do servidor
    /// de jogo. O que houver ali é trocado.
    pub destino: String,
    /// O prefixo, dentro do pacote, do que se extrai; `""` é tudo.
    pub raiz: String,
    /// **Por cima do que já está** (API 8 parte 6): o que sai do pacote entra
    /// no destino arquivo a arquivo, trocando os de mesmo nome, e o resto do
    /// destino fica. É como os `overrides` e os `server-overrides` de um
    /// modpack entram na pasta do jogo, onde já moram o mundo e os mods. Sem
    /// isto (`false`), o destino inteiro é trocado.
    pub mesclar: bool,
}

/// Um pacote aberto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desempacotado {
    /// A pasta que ficou.
    pub destino: PathBuf,
    /// Quantos arquivos foram escritos.
    pub arquivos: u64,
    /// Quantos bytes foram escritos.
    pub bytes: u64,
    /// Quantos links do pacote não foram criados.
    pub links_pulados: u64,
    /// Quantas entradas estavam fora de `raiz`, e ficaram de fora.
    pub fora_da_raiz: u64,
}

/// A pasta parcial: apagada ao ser solta, a menos que tenha virado o destino.
struct PastaParcial {
    caminho: PathBuf,
    manter: bool,
}

impl Drop for PastaParcial {
    fn drop(&mut self) {
        if !self.manter {
            let _ = std::fs::remove_dir_all(&self.caminho);
        }
    }
}

/// O que se lê de cada entrada, nos dois formatos.
enum Tipo {
    Pasta,
    Arquivo { modo: Option<u32> },
    Link,
}

/// O que vale durante uma extração.
struct Extracao<'a> {
    pacote: &'a str,
    prefixo: Vec<String>,
    base: PathBuf,
    livre: u64,
    feito: Desempacotado,
    entradas: u64,
    encerrado: &'a std::sync::atomic::AtomicBool,
}

impl Extracao<'_> {
    fn invalido(&self, motivo: impl Into<String>) -> Recusa {
        Recusa::PacoteInvalido {
            arquivo: self.pacote.to_owned(),
            motivo: motivo.into(),
        }
    }

    /// O caminho de uma entrada sem o prefixo, ou `None` se ela está fora dele.
    ///
    /// # Errors
    ///
    /// Uma entrada absoluta ou com `..`: o pacote inteiro é recusado.
    fn relativo(&self, nome: &str) -> Result<Option<Vec<String>>, Recusa> {
        if nome.starts_with('/') || nome.starts_with('\\') || nome.as_bytes().get(1) == Some(&b':')
        {
            return Err(self.invalido(format!("a entrada «{nome}» é absoluta")));
        }
        let partes: Vec<String> = nome
            .split(['/', '\\'])
            .filter(|p| !p.is_empty() && *p != ".")
            .map(str::to_owned)
            .collect();
        if partes.iter().any(|p| p == "..") {
            return Err(self.invalido(format!("a entrada «{nome}» tem «..»")));
        }
        Ok(partes
            .as_slice()
            .strip_prefix(self.prefixo.as_slice())
            .map(<[String]>::to_vec))
    }

    /// Grava uma entrada. `leitor` é o conteúdo de um arquivo.
    fn entrada(&mut self, nome: &str, tipo: Tipo, leitor: &mut dyn Read) -> Result<(), Recusa> {
        if self.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        self.entradas += 1;
        if self.entradas > MAX_ENTRADAS_DESEMPACOTADAS {
            return Err(self.invalido(format!("mais de {MAX_ENTRADAS_DESEMPACOTADAS} entradas")));
        }
        let Some(partes) = self.relativo(nome)? else {
            self.feito.fora_da_raiz += 1;
            return Ok(());
        };
        if partes.is_empty() {
            // O próprio prefixo.
            return Ok(());
        }
        let relativo =
            seele_proto::mods::inner_path(&partes.iter().map(String::as_str).collect::<Vec<_>>())
                .ok_or_else(|| self.invalido(format!("a entrada «{nome}» não é um caminho")))?;
        let alvo = self.base.join(relativo);
        let disco = |caminho: &Path| {
            let caminho = caminho.to_path_buf();
            move |erro: std::io::Error| Recusa::Disco {
                caminho,
                motivo: erro.to_string(),
            }
        };
        match tipo {
            Tipo::Link => {
                self.feito.links_pulados += 1;
            }
            Tipo::Pasta => std::fs::create_dir_all(&alvo).map_err(disco(&alvo))?,
            Tipo::Arquivo { modo } => {
                if let Some(pasta) = alvo.parent() {
                    std::fs::create_dir_all(pasta).map_err(disco(pasta))?;
                }
                let mut arquivo = File::create(&alvo).map_err(disco(&alvo))?;
                let mut pedaco = vec![0_u8; 64 * 1024];
                loop {
                    let lidos = leitor
                        .read(&mut pedaco)
                        .map_err(|e| self.invalido(e.to_string()))?;
                    let Some(lido) = pedaco.get(..lidos).filter(|l| !l.is_empty()) else {
                        break;
                    };
                    let depois = self.feito.bytes.saturating_add(lidos as u64);
                    if depois > self.livre {
                        return Err(Recusa::DiscoCheio {
                            usado: self.feito.bytes,
                            pedido: lidos as u64,
                            teto: self.livre,
                        });
                    }
                    arquivo.write_all(lido).map_err(disco(&alvo))?;
                    self.feito.bytes = depois;
                }
                drop(arquivo);
                #[cfg(unix)]
                if let Some(modo) = modo {
                    use std::os::unix::fs::PermissionsExt as _;
                    std::fs::set_permissions(&alvo, std::fs::Permissions::from_mode(modo & 0o755))
                        .map_err(disco(&alvo))?;
                }
                #[cfg(not(unix))]
                let _ = modo;
                self.feito.arquivos += 1;
            }
        }
        Ok(())
    }
}

/// Lê um `.tar.gz` inteiro, entrada a entrada.
fn tar_gz(arquivo: File, x: &mut Extracao<'_>) -> Result<(), Recusa> {
    let mut pacote = tar::Archive::new(flate2::read::GzDecoder::new(arquivo));
    let entradas = pacote.entries().map_err(|e| x.invalido(e.to_string()))?;
    for entrada in entradas {
        let mut entrada = entrada.map_err(|e| x.invalido(e.to_string()))?;
        let nome = String::from_utf8_lossy(&entrada.path_bytes()).into_owned();
        let cabecalho = entrada.header();
        let tipo = match cabecalho.entry_type() {
            tar::EntryType::Directory => Tipo::Pasta,
            tar::EntryType::Regular | tar::EntryType::Continuous => Tipo::Arquivo {
                modo: cabecalho.mode().ok(),
            },
            tar::EntryType::Symlink | tar::EntryType::Link => Tipo::Link,
            // Os cabeçalhos estendidos (PAX, nomes longos do GNU) o `tar` já
            // aplicou; o que sobra (aparelhos, FIFOs) não é de um servidor de jogo.
            tar::EntryType::XGlobalHeader
            | tar::EntryType::XHeader
            | tar::EntryType::GNULongName
            | tar::EntryType::GNULongLink => continue,
            outro => {
                return Err(x.invalido(format!("a entrada «{nome}» é do tipo {outro:?}")));
            }
        };
        x.entrada(&nome, tipo, &mut entrada)?;
    }
    Ok(())
}

/// Dois bytes little-endian em `i`, ou `None` se o pacote acaba antes: o
/// pacote é dado de fora, e um índice fora dele é um pacote quebrado, e não
/// um pânico.
fn u16_em(b: &[u8], i: usize) -> Option<u16> {
    b.get(i..i.checked_add(2)?)?
        .try_into()
        .ok()
        .map(u16::from_le_bytes)
}

/// Quatro bytes little-endian em `i`, ou `None`.
fn u32_em(b: &[u8], i: usize) -> Option<u32> {
    b.get(i..i.checked_add(4)?)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

/// Se `b` tem a assinatura `assinatura` em `i`.
fn assinatura_em(b: &[u8], i: usize, assinatura: [u8; 4]) -> bool {
    i.checked_add(4)
        .and_then(|fim| b.get(i..fim))
        .is_some_and(|achado| achado == assinatura)
}

/// Confere o CRC-32 do que passou, no fim.
struct ComCrc<R> {
    dentro: R,
    crc: flate2::Crc,
}

impl<R: Read> Read for ComCrc<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.dentro.read(buf)?;
        self.crc.update(buf.get(..n).unwrap_or_default());
        Ok(n)
    }
}

/// Lê um `.zip` pelo diretório central.
#[allow(clippy::too_many_lines)]
fn zip(mut arquivo: File, x: &mut Extracao<'_>) -> Result<(), Recusa> {
    let tamanho = arquivo
        .seek(SeekFrom::End(0))
        .map_err(|e| x.invalido(e.to_string()))?;
    // O fim do diretório central: 22 bytes, mais um comentário de até 64 KiB.
    let cauda = tamanho.min(22 + 65_535);
    let mut fim = vec![0_u8; usize::try_from(cauda).unwrap_or(0)];
    arquivo
        .seek(SeekFrom::Start(tamanho - cauda))
        .and_then(|_| arquivo.read_exact(&mut fim))
        .map_err(|e| x.invalido(e.to_string()))?;
    let Some(eocd) = (0..fim.len().saturating_sub(21))
        .rev()
        .find(|&i| assinatura_em(&fim, i, [0x50, 0x4b, 0x05, 0x06]))
    else {
        return Err(x.invalido("sem o fim do diretório central"));
    };
    let quebrado = || x.invalido("o fim do diretório central está quebrado");
    let quantas = u16_em(&fim, eocd + 10).ok_or_else(quebrado)?;
    let tamanho_do_central = u32_em(&fim, eocd + 12).ok_or_else(quebrado)?;
    let inicio_do_central = u32_em(&fim, eocd + 16).ok_or_else(quebrado)?;
    if quantas == 0xFFFF || inicio_do_central == 0xFFFF_FFFF {
        return Err(x.invalido("zip64 não é lido"));
    }
    let mut central = vec![0_u8; tamanho_do_central as usize];
    arquivo
        .seek(SeekFrom::Start(u64::from(inicio_do_central)))
        .and_then(|_| arquivo.read_exact(&mut central))
        .map_err(|e| x.invalido(e.to_string()))?;
    let mut i = 0_usize;
    for _ in 0..quantas {
        let quebrado = || x.invalido("o diretório central está quebrado");
        if !assinatura_em(&central, i, [0x50, 0x4b, 0x01, 0x02]) {
            return Err(quebrado());
        }
        let campo16 = |d: usize| u16_em(&central, i + d).ok_or_else(quebrado);
        let campo32 = |d: usize| u32_em(&central, i + d).ok_or_else(quebrado);
        let feito_por = campo16(4)? >> 8;
        let bandeiras = campo16(8)?;
        let metodo = campo16(10)?;
        let crc = campo32(16)?;
        let comprimido = campo32(20)?;
        let descomprimido = campo32(24)?;
        let n_nome = usize::from(campo16(28)?);
        let n_extra = usize::from(campo16(30)?);
        let n_comentario = usize::from(campo16(32)?);
        let externos = campo32(38)?;
        let local = campo32(42)?;
        let Some(nome) = central.get(i + 46..i + 46 + n_nome) else {
            return Err(x.invalido("o diretório central está quebrado"));
        };
        let nome = String::from_utf8_lossy(nome).into_owned();
        i += 46 + n_nome + n_extra + n_comentario;
        if comprimido == 0xFFFF_FFFF || descomprimido == 0xFFFF_FFFF || local == 0xFFFF_FFFF {
            return Err(x.invalido(format!("a entrada «{nome}» é zip64, que não é lido")));
        }
        if bandeiras & 1 != 0 {
            return Err(x.invalido(format!("a entrada «{nome}» é criptografada")));
        }
        // 3 = Unix: o modo vem nos 16 bits altos dos atributos externos.
        let modo_unix = (feito_por == 3).then_some(externos >> 16);
        let e_link = modo_unix.is_some_and(|m| m & 0o170_000 == 0o120_000);
        let tipo = if nome.ends_with('/') || nome.ends_with('\\') {
            Tipo::Pasta
        } else if e_link {
            Tipo::Link
        } else {
            Tipo::Arquivo {
                modo: modo_unix.filter(|m| *m != 0),
            }
        };
        if !matches!(tipo, Tipo::Arquivo { .. }) {
            x.entrada(&nome, tipo, &mut std::io::empty())?;
            continue;
        }
        let mut cabecalho = [0_u8; 30];
        arquivo
            .seek(SeekFrom::Start(u64::from(local)))
            .and_then(|_| arquivo.read_exact(&mut cabecalho))
            .map_err(|e| x.invalido(e.to_string()))?;
        let quebrado = || x.invalido(format!("o cabeçalho local de «{nome}» está quebrado"));
        if !assinatura_em(&cabecalho, 0, [0x50, 0x4b, 0x03, 0x04]) {
            return Err(quebrado());
        }
        let pular = u64::from(u16_em(&cabecalho, 26).ok_or_else(quebrado)?)
            + u64::from(u16_em(&cabecalho, 28).ok_or_else(quebrado)?);
        arquivo
            .seek(SeekFrom::Current(i64::try_from(pular).unwrap_or(0)))
            .map_err(|e| x.invalido(e.to_string()))?;
        let bruto = (&mut arquivo).take(u64::from(comprimido));
        let mut leitor: ComCrc<Box<dyn Read + '_>> = match metodo {
            0 => ComCrc {
                dentro: Box::new(bruto),
                crc: flate2::Crc::new(),
            },
            8 => ComCrc {
                dentro: Box::new(flate2::read::DeflateDecoder::new(bruto)),
                crc: flate2::Crc::new(),
            },
            outro => {
                return Err(x.invalido(format!("a entrada «{nome}» usa o método {outro}")));
            }
        };
        let antes = x.feito.bytes;
        x.entrada(&nome, tipo, &mut leitor)?;
        let escritos = x.feito.bytes - antes;
        // Uma entrada fora do prefixo não foi lida: nada a conferir.
        if escritos > 0 || descomprimido == 0 {
            if escritos != u64::from(descomprimido) {
                return Err(x.invalido(format!(
                    "«{nome}» diz {descomprimido} bytes e deu {escritos}"
                )));
            }
            if leitor.crc.sum() != crc {
                return Err(x.invalido(format!("o CRC-32 de «{nome}» não bate")));
            }
        }
    }
    Ok(())
}

impl Programa {
    /// Abre um `.tar.gz` ou um `.zip` da pasta do servidor de jogo em
    /// `destino`, com só o que está sob `raiz`.
    ///
    /// **Bloqueia** enquanto lê e escreve (um JRE tem uns 150 MB abertos):
    /// quem chama de um runtime o chama por `spawn_blocking`. Para entre duas
    /// entradas quando o [`Programa`] é encerrado.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], as do caminho, [`Recusa::FormatoDesconhecido`],
    /// [`Recusa::PacoteInvalido`], [`Recusa::NadaSobARaiz`],
    /// [`Recusa::DiscoCheio`] e [`Recusa::Disco`]. Toda recusa é contada.
    pub fn desempacotar(&self, pedido: &PedidoDeDesempacotar) -> Result<Desempacotado, Recusa> {
        self.desempacotar_sem_contar(pedido)
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    fn desempacotar_sem_contar(
        &self,
        pedido: &PedidoDeDesempacotar,
    ) -> Result<Desempacotado, Recusa> {
        let interior = &self.interior;
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let origem = caminho::dentro_da_raiz(&interior.raiz, &pedido.arquivo, false)?;
        // Por cima, `""` é a própria pasta do servidor de jogo: é onde os
        // `overrides` de um modpack entram (API 8 parte 6).
        let destino = caminho::dentro_da_raiz(&interior.raiz, &pedido.destino, pedido.mesclar)?;
        let disco = |caminho: &Path| {
            let caminho = caminho.to_path_buf();
            move |erro: std::io::Error| Recusa::Disco {
                caminho,
                motivo: erro.to_string(),
            }
        };
        // **O que roda é o que foi aceito** (I1): um pacote que não tem os
        // bytes de um artefato aceito não troca uma pasta que o aceite prende
        // (`java/`), nem uma que contém um caminho preso. Por cima, cada
        // entrada é conferida antes de qualquer uma entrar.
        let aceito = super::reservado::e_artefato_aceito(&interior.politica, &origem);
        if !aceito && !pedido.mesclar {
            interior
                .presos
                .conferir(&interior.raiz, &destino, &pedido.destino, Escrita::Troca)?;
        }
        let mut arquivo = File::open(&origem).map_err(disco(&origem))?;
        let mut magica = [0_u8; 4];
        let lidos = arquivo.read(&mut magica).map_err(disco(&origem))?;
        arquivo.seek(SeekFrom::Start(0)).map_err(disco(&origem))?;
        let e_gz = lidos >= 2 && magica[..2] == [0x1f, 0x8b];
        let e_zip = lidos == 4 && magica == [0x50, 0x4b, 0x03, 0x04];
        if !e_gz && !e_zip {
            return Err(Recusa::FormatoDesconhecido {
                arquivo: pedido.arquivo.clone(),
            });
        }

        // O disco: o que sobra no teto, contando de volta o destino que este
        // pacote substitui. Medido uma vez, no começo (decisão Q8).
        let livre = {
            let reservado = interior.reservado.lock().map_or(0, |r| *r);
            let usado = caminho::ocupado(&interior.raiz, &super::baixar::e_parcial)
                .saturating_add(reservado);
            // Por cima, nada sai: o que se troca conta duas vezes (o lado
            // seguro do teto).
            let substitui = if pedido.mesclar {
                0
            } else if destino.is_dir() {
                caminho::ocupado(&destino, &|_| false)
            } else {
                std::fs::metadata(&destino).map_or(0, |m| m.len())
            };
            interior
                .politica
                .teto_de_disco
                .saturating_sub(usado.saturating_sub(substitui))
        };

        // Por cima, a parcial fica **dentro da raiz**: o destino pode ser a
        // própria raiz, e a pasta de cima dela não é do servidor de jogo.
        let pasta = if pedido.mesclar {
            interior.raiz.clone()
        } else {
            destino.parent().unwrap_or(&interior.raiz).to_path_buf()
        };
        std::fs::create_dir_all(&pasta).map_err(disco(&pasta))?;
        caminho::dentro_da_raiz(&interior.raiz, &pedido.destino, pedido.mesclar)?;
        let nome = if pedido.mesclar {
            "mesclar".to_owned()
        } else {
            destino
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let mut parcial = PastaParcial {
            caminho: pasta.join(format!(
                ".{nome}.{}.{}",
                interior.proximo_numero(),
                super::baixar::EXTENSAO_PARCIAL
            )),
            manter: false,
        };
        std::fs::create_dir_all(&parcial.caminho).map_err(disco(&parcial.caminho))?;

        let mut x = Extracao {
            pacote: &pedido.arquivo,
            prefixo: pedido
                .raiz
                .split(['/', '\\'])
                .filter(|p| !p.is_empty() && *p != ".")
                .map(str::to_owned)
                .collect(),
            base: parcial.caminho.clone(),
            livre,
            feito: Desempacotado {
                destino: destino.clone(),
                arquivos: 0,
                bytes: 0,
                links_pulados: 0,
                fora_da_raiz: 0,
            },
            entradas: 0,
            encerrado: &interior.encerrado,
        };
        if x.prefixo.iter().any(|p| p == "..") {
            return Err(Recusa::CaminhoComPai {
                caminho: pedido.raiz.clone(),
            });
        }
        if e_gz {
            tar_gz(arquivo, &mut x)?;
        } else {
            zip(arquivo, &mut x)?;
        }
        let feito = x.feito;
        if feito.arquivos == 0 {
            return Err(Recusa::NadaSobARaiz {
                arquivo: pedido.arquivo.clone(),
                raiz: pedido.raiz.clone(),
            });
        }
        if !aceito && pedido.mesclar {
            if destino.is_dir() {
                conferir_por_cima(interior, &destino, &pedido.destino, &parcial.caminho)?;
            } else {
                interior.presos.conferir(
                    &interior.raiz,
                    &destino,
                    &pedido.destino,
                    Escrita::Troca,
                )?;
            }
        }
        if pedido.mesclar && destino.is_dir() {
            // Arquivo a arquivo, cada destino conferido de novo contra a
            // raiz: uma pasta do jogo pode ter virado link depois da
            // primeira conferência.
            mesclar(
                &interior.raiz,
                &pedido.destino,
                &parcial.caminho,
                &parcial.caminho,
            )?;
            tracing::info!(
                destino = %destino.display(),
                arquivos = feito.arquivos,
                bytes = feito.bytes,
                "programa desempacotou por cima"
            );
            return Ok(feito);
        }
        // O destino de antes sai só agora, com o novo inteiro ao lado.
        if let Ok(meta) = std::fs::symlink_metadata(&destino) {
            let saiu = if meta.is_dir() {
                std::fs::remove_dir_all(&destino)
            } else {
                std::fs::remove_file(&destino)
            };
            saiu.map_err(disco(&destino))?;
        }
        std::fs::rename(&parcial.caminho, &destino).map_err(disco(&destino))?;
        parcial.manter = true;
        tracing::info!(
            destino = %destino.display(),
            arquivos = feito.arquivos,
            bytes = feito.bytes,
            links_pulados = feito.links_pulados,
            fora_da_raiz = feito.fora_da_raiz,
            "programa desempacotou"
        );
        Ok(feito)
    }
}

/// **Cada entrada que vai entrar por cima**, conferida contra o que o aceite
/// prende (I1) **antes** de qualquer uma entrar: uma recusa no meio deixaria
/// metade do pacote aplicada.
fn conferir_por_cima(
    interior: &super::Interior,
    destino: &Path,
    texto: &str,
    pasta: &Path,
) -> Result<(), Recusa> {
    let disco = |erro: std::io::Error| Recusa::Disco {
        caminho: pasta.to_path_buf(),
        motivo: erro.to_string(),
    };
    for entrada in std::fs::read_dir(pasta).map_err(disco)? {
        let entrada = entrada.map_err(disco)?;
        let nome = entrada.file_name();
        let alvo = destino.join(&nome);
        let junto = if texto.is_empty() {
            nome.to_string_lossy().into_owned()
        } else {
            format!("{texto}/{}", nome.to_string_lossy())
        };
        let e_pasta = entrada.file_type().map_err(disco)?.is_dir();
        let escrita = if e_pasta {
            Escrita::Pasta
        } else {
            Escrita::Troca
        };
        interior
            .presos
            .conferir(&interior.raiz, &alvo, &junto, escrita)?;
        if e_pasta {
            conferir_por_cima(interior, &alvo, &junto, &entrada.path())?;
        }
    }
    Ok(())
}

/// Move cada arquivo de `pasta` (sob `base`, a parcial) para o mesmo lugar sob
/// `destino` (relativo a `raiz`), trocando o que houver com o mesmo nome.
fn mesclar(raiz: &Path, destino: &str, base: &Path, pasta: &Path) -> Result<(), Recusa> {
    let disco = |caminho: &Path| {
        let caminho = caminho.to_path_buf();
        move |erro: std::io::Error| Recusa::Disco {
            caminho,
            motivo: erro.to_string(),
        }
    };
    for entrada in std::fs::read_dir(pasta).map_err(disco(pasta))? {
        let entrada = entrada.map_err(disco(pasta))?;
        let de = entrada.path();
        let relativo = de
            .strip_prefix(base)
            .map_err(|_| Recusa::CaminhoInvalido {
                caminho: de.display().to_string(),
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let junto = if destino.is_empty() {
            relativo.clone()
        } else {
            format!("{destino}/{relativo}")
        };
        let alvo = caminho::dentro_da_raiz(raiz, &junto, false)?;
        let tipo = entrada.file_type().map_err(disco(&de))?;
        if tipo.is_dir() {
            if !alvo.is_dir() {
                if std::fs::symlink_metadata(&alvo).is_ok() {
                    std::fs::remove_file(&alvo).map_err(disco(&alvo))?;
                }
                std::fs::create_dir_all(&alvo).map_err(disco(&alvo))?;
            }
            mesclar(raiz, destino, base, &de)?;
        } else {
            if alvo.is_dir() {
                std::fs::remove_dir_all(&alvo).map_err(disco(&alvo))?;
            }
            std::fs::rename(&de, &alvo).map_err(disco(&alvo))?;
        }
    }
    Ok(())
}
