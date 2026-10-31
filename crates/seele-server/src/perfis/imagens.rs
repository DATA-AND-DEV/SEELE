//! As imagens do perfil: o envio, a leitura, a troca e a retirada (0.19,
//! `docs/0.19/perfis-oficiais.md`, §2 e §3).
//!
//! # Onde moram
//!
//! Em `perfis/<sha256 em hex>`, ao lado de `seele.db` (o mesmo padrão de
//! `diretorio_de_anexos`). A pasta sai do **caminho do próprio banco**, e não de
//! um campo à parte: o servidor em memória não tem arquivo, não tem pasta, e o
//! envio volta com [`RecusaDePerfil::SemPastaDePerfis`] em vez de uma pasta
//! brotar no diretório de onde o processo subiu.
//!
//! # O caminho de um envio
//!
//! 1. O tamanho **declarado** é conferido antes de qualquer byte: passou de
//!    [`TETO_DA_IMAGEM`], recusa (e o quadro do pedido já nem decodifica).
//! 2. Um envio por vez por pessoa, e o balde de bytes por pessoa
//!    ([`crate::taxa::Vazao`]) cobrado com o tamanho declarado, como nos anexos.
//! 3. Os bytes vão em blocos de 64 KiB para `.em-obras-*`, com o hash somado no
//!    caminho. Os doze primeiros decidem o tipo; HEIC volta com o nome dele.
//!    Passou do teto durante o fluxo, para ali. Terminou antes do declarado:
//!    [`RecusaDePerfil::Incompleto`].
//! 4. `rename` para o endereço do conteúdo, `trocar_imagem` no banco, e **só
//!    depois do commit** o arquivo antigo sai — se ninguém mais o aponta.
//!
//! O passo 4 roda atrás de uma guarda por pasta: publicar um arquivo e apagar
//! outro com o mesmo nome, ao mesmo tempo, deixaria uma linha apontando para o
//! nada. O fluxo em si (passo 3) fica fora da guarda.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use seele_proto::attachment::{hex, ContentDigest};
use seele_proto::ids::PersonId;
use seele_proto::imagem::{reconhecido_e_nao_aceito, sniff, SNIFF_LEN};
use seele_proto::perfil::{
    ImagemDoPerfil, LadoDoPerfil, PerfilPublico, RecusaDePerfil, TipoDaImagem, TETO_DA_IMAGEM,
};

use crate::persistence::perfis as banco;
use crate::persistence::perfis::PASTA_DAS_IMAGENS;
use crate::persistence::Persistence;
use crate::server::{Event, Server};
use crate::taxa::Vazao;

/// Tamanho do bloco de leitura e de gravação.
const BLOCO: usize = 64 * 1024;

/// O prefixo do arquivo de obras, o mesmo de `mods/volume.rs`.
const PREFIXO_DE_OBRAS: &str = ".em-obras-";

/// Por que um envio ou uma leitura não deu.
#[derive(Debug, PartialEq, Eq)]
pub enum Falha {
    /// Uma recusa do vocabulário do perfil: a pessoa recebe o
    /// `PerfilRecusado` correspondente.
    Recusa(RecusaDePerfil),
    /// Qualquer outra: vai só como texto na resposta do fluxo (o balde de
    /// bytes, o banco, o disco).
    Outra(String),
}

impl From<RecusaDePerfil> for Falha {
    fn from(recusa: RecusaDePerfil) -> Self {
        Self::Recusa(recusa)
    }
}

impl Falha {
    fn outra(erro: impl std::fmt::Display) -> Self {
        Self::Outra(erro.to_string())
    }

    /// O texto que o cliente lê em `RespostaDeImagem::erro`.
    #[must_use]
    pub fn texto(&self) -> String {
        match self {
            Self::Recusa(recusa) => nome_da_recusa(*recusa).to_owned(),
            Self::Outra(texto) => texto.clone(),
        }
    }
}

/// O nome de uma recusa na resposta do fluxo de imagem.
#[must_use]
pub const fn nome_da_recusa(recusa: RecusaDePerfil) -> &'static str {
    match recusa {
        RecusaDePerfil::RevisaoVelha => "revisao-velha",
        RecusaDePerfil::Campos => "campos",
        RecusaDePerfil::Heic => "heic",
        RecusaDePerfil::TipoRecusado => "tipo-recusado",
        RecusaDePerfil::GrandeDemais => "grande-demais",
        RecusaDePerfil::Incompleto => "incompleto",
        RecusaDePerfil::SemPastaDePerfis => "sem-pasta-de-perfis",
    }
}

// ---------------------------------------------------------------------------
// A pasta e o estado por pasta
// ---------------------------------------------------------------------------

/// A pasta das imagens deste servidor, ou `None` quando o banco é o de memória.
#[must_use]
pub fn pasta_das_imagens(persistence: &Persistence) -> Option<PathBuf> {
    let arquivo = Path::new(persistence.connection().path()?);
    if arquivo.as_os_str().is_empty() {
        return None;
    }
    Some(
        arquivo
            .parent()
            .filter(|pai| !pai.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join(PASTA_DAS_IMAGENS),
    )
}

/// O que se compartilha entre os envios de uma mesma pasta.
#[derive(Default)]
struct EstadoDaPasta {
    /// O balde de bytes por pessoa.
    vazao: Mutex<Vazao>,
    /// Publicar um arquivo e apagar outro, um de cada vez.
    guarda: tokio::sync::Mutex<()>,
    /// Um envio por vez por pessoa.
    enviando: Mutex<HashMap<PersonId, Arc<tokio::sync::Mutex<()>>>>,
    /// Distingue dois arquivos de obras.
    obras: AtomicU64,
}

/// Um estado por pasta, e a pasta é do banco: dois servidores no mesmo processo
/// (os testes) não dividem balde nem fila.
fn estado_de(pasta: &Path) -> Arc<EstadoDaPasta> {
    static ESTADOS: OnceLock<Mutex<HashMap<PathBuf, Arc<EstadoDaPasta>>>> = OnceLock::new();
    let mapa = ESTADOS.get_or_init(Mutex::default);
    let mut mapa = mapa
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Arc::clone(mapa.entry(pasta.to_path_buf()).or_default())
}

fn caminho_do_arquivo(pasta: &Path, hash: &[u8; 32]) -> PathBuf {
    pasta.join(hex(hash))
}

// ---------------------------------------------------------------------------
// Receber os bytes
// ---------------------------------------------------------------------------

/// De onde os bytes vêm: o fluxo em produção, e uma fonte de bytes nos testes.
pub trait Fonte {
    /// Lê até `buf.len()` bytes. `None` é o fim do fluxo; `Err` é um fluxo
    /// derrubado.
    fn ler(
        &mut self,
        buf: &mut [u8],
    ) -> impl std::future::Future<Output = Result<Option<usize>, ()>> + Send;
}

impl Fonte for crate::transporte::Recebe {
    async fn ler(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ()> {
        self.read(buf).await.map_err(|_| ())
    }
}

/// O arquivo de obras some quando isto cai — inclusive quando o prazo do fluxo
/// cancela a future no meio da escrita. Depois do `rename` não há mais o que
/// apagar, e a remoção que falha é ignorada.
struct Obras(PathBuf);
impl Drop for Obras {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// O que chegou, inteiro e conferido, ainda no arquivo de obras.
struct Chegou {
    imagem: ImagemDoPerfil,
    obras: Obras,
}

const fn tipo_do_formato(formato: seele_proto::imagem::ImageFormat) -> TipoDaImagem {
    match formato {
        seele_proto::imagem::ImageFormat::Png => TipoDaImagem::Png,
        seele_proto::imagem::ImageFormat::Jpeg => TipoDaImagem::Jpeg,
        seele_proto::imagem::ImageFormat::Gif => TipoDaImagem::Gif,
        seele_proto::imagem::ImageFormat::Webp => TipoDaImagem::Webp,
    }
}

/// O tipo pelos bytes do começo: um dos quatro, ou a recusa certa.
fn tipo_dos_bytes(comeco: &[u8]) -> Result<TipoDaImagem, RecusaDePerfil> {
    if let Some(formato) = sniff(comeco) {
        return Ok(tipo_do_formato(formato));
    }
    if reconhecido_e_nao_aceito(comeco).is_some() {
        return Err(RecusaDePerfil::Heic);
    }
    Err(RecusaDePerfil::TipoRecusado)
}

/// Escorre o fluxo para `obras`, somando o hash e conferindo teto e tipo
/// **durante** a leitura.
async fn escorrer(
    fonte: &mut impl Fonte,
    obras: &Path,
    declarado: u64,
) -> Result<(TipoDaImagem, [u8; 32], u64), Falha> {
    use tokio::io::AsyncWriteExt as _;

    // O teto vale pelo menor dos dois: o declarado já foi conferido contra o
    // teto, mas esta função não depende de quem a chamou ter conferido.
    let teto = declarado.min(TETO_DA_IMAGEM as u64);
    let mut arquivo = tokio::fs::File::create(obras).await.map_err(Falha::outra)?;
    let mut soma = ContentDigest::new();
    let mut bytes = 0_u64;
    let mut comeco: Vec<u8> = Vec::with_capacity(SNIFF_LEN);
    let mut tipo = None;
    let mut bloco = vec![0_u8; BLOCO];

    loop {
        let lido = fonte
            .ler(&mut bloco)
            .await
            .map_err(|()| RecusaDePerfil::Incompleto)?;
        let Some(quantos) = lido else { break };
        let pedaco = bloco.get(..quantos).unwrap_or_default();
        bytes = bytes.saturating_add(quantos as u64);
        if bytes > teto {
            return Err(RecusaDePerfil::GrandeDemais.into());
        }
        soma.feed(pedaco);
        let mut a_gravar = pedaco;
        if tipo.is_none() {
            let falta = SNIFF_LEN - comeco.len();
            let (cabe, resto) = pedaco.split_at(falta.min(pedaco.len()));
            comeco.extend_from_slice(cabe);
            if comeco.len() < SNIFF_LEN {
                continue;
            }
            tipo = Some(tipo_dos_bytes(&comeco)?);
            arquivo.write_all(&comeco).await.map_err(Falha::outra)?;
            a_gravar = resto;
        }
        arquivo.write_all(a_gravar).await.map_err(Falha::outra)?;
    }
    arquivo.flush().await.map_err(Falha::outra)?;
    drop(arquivo);

    if bytes != declarado {
        return Err(RecusaDePerfil::Incompleto.into());
    }
    // Um fluxo mais curto que o sniff nunca chegou a conferir: confere agora.
    let tipo = match tipo {
        Some(tipo) => tipo,
        None => tipo_dos_bytes(&comeco)?,
    };
    Ok((tipo, soma.finish(), bytes))
}

async fn receber(
    estado: &EstadoDaPasta,
    pasta: &Path,
    fonte: &mut impl Fonte,
    declarado: u64,
) -> Result<Chegou, Falha> {
    tokio::fs::create_dir_all(pasta)
        .await
        .map_err(|_| RecusaDePerfil::SemPastaDePerfis)?;
    let obras = Obras(pasta.join(format!(
        "{PREFIXO_DE_OBRAS}{}-{}",
        std::process::id(),
        estado.obras.fetch_add(1, Ordering::Relaxed)
    )));
    let (tipo, hash, bytes) = escorrer(fonte, &obras.0, declarado).await?;
    let bytes = u32::try_from(bytes).map_err(Falha::outra)?;
    Ok(Chegou {
        imagem: ImagemDoPerfil { hash, tipo, bytes },
        obras,
    })
}

// ---------------------------------------------------------------------------
// Publicar, trocar, tirar
// ---------------------------------------------------------------------------

/// Difunde o perfil como está agora no banco.
async fn difundir(server: &Server, pessoa: PersonId) -> Result<PerfilPublico> {
    let perfil = {
        let db = server.persistence.lock().await;
        banco::ler(&db, pessoa)?
    }
    .ok_or_else(|| anyhow!("o perfil sumiu entre o commit e a difusão"))?;
    let _ = server.events.send(Event::PerfilMudou(perfil.clone()));
    Ok(perfil)
}

/// Apaga o arquivo que a troca deixou para trás — **depois do commit**, e só se
/// nenhum perfil mais o aponta. Quem chama segura a guarda da pasta.
async fn apagar_se_solto(server: &Server, pasta: &Path, anterior: Option<[u8; 32]>) {
    let Some(hash) = anterior else { return };
    let em_uso = {
        let db = server.persistence.lock().await;
        banco::hash_em_uso(&db, &hash)
    };
    match em_uso {
        Ok(false) => {
            if let Err(erro) = tokio::fs::remove_file(caminho_do_arquivo(pasta, &hash)).await {
                if erro.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(%erro, "perfis: não deu para apagar a imagem antiga; a varredura da subida a leva");
                }
            }
        }
        Ok(true) => {}
        Err(erro) => {
            tracing::warn!(%erro, "perfis: não deu para conferir se a imagem antiga ainda é usada; ela fica");
        }
    }
}

/// Recebe a foto ou a capa de `pessoa`, grava, troca no banco e difunde.
///
/// # Errors
///
/// [`Falha`]: a recusa certa, ou o texto do que deu errado.
pub async fn enviar(
    server: &Server,
    pessoa: PersonId,
    lado: LadoDoPerfil,
    declarado: u64,
    fonte: &mut impl Fonte,
) -> Result<(), Falha> {
    let pasta = {
        let db = server.persistence.lock().await;
        pasta_das_imagens(&db)
    }
    .ok_or(RecusaDePerfil::SemPastaDePerfis)?;
    // Antes de qualquer byte, e antes de tocar o disco.
    if declarado > TETO_DA_IMAGEM as u64 {
        return Err(RecusaDePerfil::GrandeDemais.into());
    }
    if declarado == 0 {
        return Err(RecusaDePerfil::Incompleto.into());
    }
    let estado = estado_de(&pasta);

    // Um envio por vez por pessoa: o seguinte espera (o prazo do fluxo limita).
    let fila = {
        let mut enviando = estado
            .enviando
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(enviando.entry(pessoa).or_default())
    };
    let _vez = fila.lock().await;

    // O balde de bytes, com o tamanho declarado, como nos anexos.
    let agora = Instant::now();
    let passou = estado
        .vazao
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .permitir(pessoa, declarado, agora);
    if !passou {
        tracing::info!(%pessoa, bytes = declarado, "perfis: imagem recusada pelo balde de bytes");
        return Err(Falha::Outra("limite-de-bytes".to_owned()));
    }

    let chegou = match receber(&estado, &pasta, fonte, declarado).await {
        Ok(chegou) => chegou,
        Err(falha) => {
            // O que não foi gasto de verdade volta ao balde.
            estado
                .vazao
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .devolver(pessoa, declarado, agora);
            return Err(falha);
        }
    };
    let destino = caminho_do_arquivo(&pasta, &chegou.imagem.hash);

    let guarda = estado.guarda.lock().await;
    // O mesmo conteúdo já está lá (a mesma foto de duas pessoas, ou de novo):
    // o endereço é o conteúdo, e o arquivo de obras só sobra.
    if !tokio::fs::try_exists(&destino).await.unwrap_or(false) {
        tokio::fs::rename(&chegou.obras.0, &destino)
            .await
            .map_err(|erro| Falha::outra(format!("nao-gravei: {erro}")))?;
    }

    let troca = {
        let db = server.persistence.lock().await;
        let troca = banco::trocar_imagem(&db, pessoa, lado, &chegou.imagem);
        // A pessoa agiu sobre as imagens: o aviso da migração foi visto.
        if troca.is_ok() {
            let _ = banco::marcar_avisada(&db, pessoa);
        }
        troca
    };
    match troca {
        Ok(troca) => {
            // O commit já aconteceu: agora, e só agora, o antigo pode sair.
            if troca.anterior != Some(chegou.imagem.hash) {
                apagar_se_solto(server, &pasta, troca.anterior).await;
            }
            drop(guarda);
            difundir(server, pessoa).await.map_err(Falha::outra)?;
            tracing::info!(%pessoa, ?lado, bytes = chegou.imagem.bytes, "perfis: imagem trocada");
            Ok(())
        }
        Err(erro) => {
            // A linha não mudou: o arquivo novo, se ninguém o aponta, sai.
            apagar_se_solto(server, &pasta, Some(chegou.imagem.hash)).await;
            Err(Falha::outra(erro))
        }
    }
}

/// Tira a foto ou a capa de `pessoa` e difunde. Sem imagem naquele lado, não
/// há escrita nem difusão.
///
/// # Errors
///
/// Falha se o banco não responder.
pub async fn tirar(server: &Server, pessoa: PersonId, lado: LadoDoPerfil) -> Result<()> {
    let pasta = {
        let db = server.persistence.lock().await;
        pasta_das_imagens(&db)
    };
    let estado = pasta.as_deref().map(estado_de);
    let guarda = match &estado {
        Some(estado) => Some(estado.guarda.lock().await),
        None => None,
    };
    let troca = {
        let db = server.persistence.lock().await;
        let troca = banco::tirar_imagem(&db, pessoa, lado)?;
        let _ = banco::marcar_avisada(&db, pessoa);
        troca
    };
    let Some(anterior) = troca.anterior else {
        return Ok(());
    };
    if let Some(pasta) = &pasta {
        apagar_se_solto(server, pasta, Some(anterior)).await;
    }
    drop(guarda);
    difundir(server, pessoa).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Ler
// ---------------------------------------------------------------------------

/// Abre a imagem de `pessoa` no lado `lado`, se o hash ainda é o atual.
///
/// # Errors
///
/// `"mudou"` quando o hash já não é o do perfil (o cliente pede o perfil de
/// novo), ou o texto do que deu errado.
pub async fn abrir(
    server: &Server,
    pessoa: PersonId,
    lado: LadoDoPerfil,
    hash: [u8; 32],
) -> Result<(tokio::fs::File, u64), String> {
    let (pasta, perfil) = {
        let db = server.persistence.lock().await;
        (
            pasta_das_imagens(&db),
            banco::ler(&db, pessoa).map_err(|erro| erro.to_string())?,
        )
    };
    let atual = perfil.and_then(|perfil| match lado {
        LadoDoPerfil::Foto => perfil.foto,
        LadoDoPerfil::Capa => perfil.capa,
    });
    let Some(imagem) = atual.filter(|imagem| imagem.hash == hash) else {
        return Err("mudou".to_owned());
    };
    let pasta = pasta.ok_or_else(|| nome_da_recusa(RecusaDePerfil::SemPastaDePerfis).to_owned())?;
    let arquivo = tokio::fs::File::open(caminho_do_arquivo(&pasta, &hash))
        .await
        .map_err(|erro| format!("nao-encontrada: {erro}"))?;
    let tamanho = arquivo
        .metadata()
        .await
        .map_err(|erro| erro.to_string())?
        .len();
    if tamanho != u64::from(imagem.bytes) {
        return Err("arquivo-diferente-do-banco".to_owned());
    }
    Ok((arquivo, tamanho))
}

// ---------------------------------------------------------------------------
// A varredura da subida
// ---------------------------------------------------------------------------

/// Remove da pasta das imagens o que nenhum perfil aponta: os arquivos de obras
/// que um desligamento deixou, e as imagens soltas por um commit que não veio.
/// Nomes que não são `.em-obras-*` nem 64 dígitos hexadecimais ficam onde estão.
///
/// Chamada na subida, **depois** de a migração dos perfis do MOD acabar: antes
/// disso ela apagaria o que a migração acabou de copiar e ainda não gravou.
/// Devolve quantos arquivos saíram. Sem pasta (banco em memória ou pasta ainda
/// inexistente), não há o que varrer.
///
/// # Errors
///
/// Falha se o banco ou a pasta não responderem.
pub fn varrer_orfaos(persistence: &Persistence) -> Result<usize> {
    let Some(pasta) = pasta_das_imagens(persistence) else {
        return Ok(0);
    };
    let entradas = match std::fs::read_dir(&pasta) {
        Ok(entradas) => entradas,
        Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(erro) => return Err(erro).context("não deu para ler a pasta das imagens de perfil"),
    };
    let em_uso: std::collections::HashSet<String> =
        banco::hashes_em_uso(persistence)?.iter().map(hex).collect();
    let mut removidos = 0;
    for entrada in entradas {
        let entrada = entrada?;
        let nome = entrada.file_name();
        let Some(nome) = nome.to_str() else { continue };
        let solto = if nome.starts_with(PREFIXO_DE_OBRAS) {
            true
        } else if nome.len() == 64 && nome.bytes().all(|b| b.is_ascii_hexdigit()) {
            !em_uso.contains(nome)
        } else {
            false
        };
        if solto && std::fs::remove_file(entrada.path()).is_ok() {
            removidos += 1;
        }
    }
    if removidos > 0 {
        tracing::info!(removidos, "perfis: imagens órfãs varridas na subida");
    }
    Ok(removidos)
}

// ---------------------------------------------------------------------------
// O despacho do fluxo de imagem (chamado por `mods::volume::atender_imagem`)
// ---------------------------------------------------------------------------

/// `EnviarAoPerfil`: recebe, e em caso de recusa avisa a pessoa pelo controle
/// (`PerfilRecusado`) além de responder no fluxo.
///
/// # Errors
///
/// Com o texto que vai em `RespostaDeImagem::erro`.
pub async fn atender_envio(
    server: &Server,
    pessoa: PersonId,
    lado: LadoDoPerfil,
    declarado: u64,
    entrada: &mut crate::transporte::Recebe,
) -> Result<()> {
    match enviar(server, pessoa, lado, declarado, entrada).await {
        Ok(()) => Ok(()),
        Err(falha) => {
            if let Falha::Recusa(motivo) = &falha {
                let _ = server.events.send(Event::PerfilRecusado {
                    pessoa,
                    motivo: *motivo,
                });
            }
            Err(anyhow!("{}", falha.texto()))
        }
    }
}

/// `LerDoPerfil`: a imagem, se o hash ainda é o atual.
///
/// # Errors
///
/// Com o texto que vai em `RespostaDeImagem::erro` (`"mudou"` incluído).
pub async fn atender_leitura(
    server: &Server,
    pessoa: PersonId,
    lado: LadoDoPerfil,
    hash: [u8; 32],
) -> Result<tokio::fs::File> {
    abrir(server, pessoa, lado, hash)
        .await
        .map(|(arquivo, _)| arquivo)
        .map_err(|texto| anyhow!("{texto}"))
}

#[cfg(test)]
mod testes {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// Uma fonte que entrega `dados` em pedaços de `passo` bytes.
    struct Fatiada {
        dados: Vec<u8>,
        passo: usize,
        derruba_no_fim: bool,
    }

    impl Fonte for Fatiada {
        async fn ler(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ()> {
            if self.dados.is_empty() {
                return if self.derruba_no_fim {
                    Err(())
                } else {
                    Ok(None)
                };
            }
            let n = self.passo.min(buf.len()).min(self.dados.len());
            buf[..n].copy_from_slice(&self.dados[..n]);
            self.dados.drain(..n);
            Ok(Some(n))
        }
    }

    fn png(tamanho: usize) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.resize(tamanho, 7);
        v
    }

    #[tokio::test]
    async fn escorre_confere_o_tipo_e_soma_o_hash() {
        let pasta = tempfile::tempdir().unwrap();
        let obras = pasta.path().join("o");
        let dados = png(200_000);
        let mut fonte = Fatiada {
            dados: dados.clone(),
            passo: 4096,
            derruba_no_fim: false,
        };
        let (tipo, hash, bytes) = escorrer(&mut fonte, &obras, dados.len() as u64)
            .await
            .unwrap();
        assert_eq!(tipo, TipoDaImagem::Png);
        assert_eq!(bytes, 200_000);
        let mut soma = ContentDigest::new();
        soma.feed(&dados);
        assert_eq!(hash, soma.finish());
        assert_eq!(std::fs::read(&obras).unwrap(), dados);
    }

    #[tokio::test]
    async fn passar_do_declarado_para_a_leitura() {
        let pasta = tempfile::tempdir().unwrap();
        let mut fonte = Fatiada {
            dados: png(10_000),
            passo: 1000,
            derruba_no_fim: false,
        };
        let erro = escorrer(&mut fonte, &pasta.path().join("o"), 5_000)
            .await
            .unwrap_err();
        assert_eq!(erro, Falha::Recusa(RecusaDePerfil::GrandeDemais));
        // Parou no bloco que passou, e não leu o resto.
        assert!(fonte.dados.len() >= 4_000);
    }

    #[tokio::test]
    async fn curto_ou_derrubado_e_incompleto() {
        let pasta = tempfile::tempdir().unwrap();
        let mut curto = Fatiada {
            dados: png(100),
            passo: 100,
            derruba_no_fim: false,
        };
        assert_eq!(
            escorrer(&mut curto, &pasta.path().join("a"), 200)
                .await
                .unwrap_err(),
            Falha::Recusa(RecusaDePerfil::Incompleto)
        );
        let mut caiu = Fatiada {
            dados: png(100),
            passo: 100,
            derruba_no_fim: true,
        };
        assert_eq!(
            escorrer(&mut caiu, &pasta.path().join("b"), 200)
                .await
                .unwrap_err(),
            Falha::Recusa(RecusaDePerfil::Incompleto)
        );
    }

    #[tokio::test]
    async fn heic_e_bytes_quaisquer_tem_recusas_diferentes() {
        let pasta = tempfile::tempdir().unwrap();
        let mut heic = b"\0\0\0\x18ftypheic".to_vec();
        heic.resize(64, 0);
        let mut fonte = Fatiada {
            dados: heic,
            passo: 64,
            derruba_no_fim: false,
        };
        assert_eq!(
            escorrer(&mut fonte, &pasta.path().join("a"), 64)
                .await
                .unwrap_err(),
            Falha::Recusa(RecusaDePerfil::Heic)
        );
        let mut fonte = Fatiada {
            dados: vec![1; 64],
            passo: 64,
            derruba_no_fim: false,
        };
        assert_eq!(
            escorrer(&mut fonte, &pasta.path().join("b"), 64)
                .await
                .unwrap_err(),
            Falha::Recusa(RecusaDePerfil::TipoRecusado)
        );
    }
}
