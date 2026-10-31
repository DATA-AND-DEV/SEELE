//! PERFIS — o perfil público de cada pessoa, na tabela `perfis` (migração 20).
//!
//! `docs/0.19/perfis-oficiais.md`, §2 e §4. Este módulo é só o **banco**: as
//! linhas, a revisão e o livro da migração do MOD. Os arquivos das imagens, a
//! sessão e a migração em si moram em `perfis/` (fases F2 e F3).
//!
//! # Duas regras que quem chama precisa cumprir
//!
//! 1. **Apagar o arquivo depois do commit.** [`trocar_imagem`] e
//!    [`tirar_imagem`] devolvem o hash que saiu; quem chama apaga
//!    `perfis/<hex>` **depois** de elas voltarem, e só se [`hash_em_uso`] disser
//!    que ninguém mais o aponta — a imagem é endereçada pelo conteúdo, então
//!    duas pessoas (ou a foto e a capa da mesma) podem dividir um arquivo.
//!    Apagar antes do commit deixaria uma linha apontando para o nada se o
//!    commit falhasse.
//! 2. **Nada de meio-termo.** Cada função aqui é uma transação: ou a linha
//!    inteira (e o livro) muda, ou nada muda.
//!
//! A revisão sobe a cada escrita, inclusive de imagem. Só [`gravar`] a
//! **confere**: trocar ou tirar uma imagem não pede a revisão esperada, mas
//! faz o editor aberto em outra janela tomar `RevisaoVelha` ao salvar, e é
//! isso que se quer.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashSet;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use seele_proto::ids::PersonId;
use seele_proto::perfil::{
    cor_de_texto, cor_em_texto, validar, EntradaDePerfil, Efeito, ImagemDoPerfil, LadoDoPerfil,
    PerfilPublico, PerfilValidado, TipoDaImagem, COR_PADRAO,
};
use serde::{Deserialize, Serialize};

use super::Persistence;

/// O nome da pasta das imagens, ao lado de `seele.db` (o mesmo padrão de
/// `diretorio_de_anexos`, em `lib.rs`). O arquivo é `perfis/<sha256 em hex>`.
pub const PASTA_DAS_IMAGENS: &str = "perfis";

/// Onde o relatório da migração mora, na tabela `config`.
pub const CHAVE_DO_RELATORIO: &str = "perfis.relatorio_da_migracao";

/// O prefixo do estado de uma imagem que não veio, no livro da migração.
const PREFIXO_FALHOU: &str = "falhou:";

/// A revisão esperada não é a atual: o perfil mudou em outra janela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisaoVelha {
    /// A revisão que o banco tem agora (0 quando a pessoa não tem linha).
    pub atual: u32,
}

/// O que uma troca ou retirada de imagem deixou para trás.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrocaDeImagem {
    /// O hash que estava ali antes, quando havia. **É o que o chamador apaga do
    /// disco, depois do commit**, se [`hash_em_uso`] disser que não é mais usado.
    pub anterior: Option<[u8; 32]>,
    /// A revisão depois da escrita. Em [`tirar_imagem`] sobre um lado já vazio
    /// não houve escrita, e é a revisão de antes.
    pub revisao: u32,
}

// ---------------------------------------------------------------------------
// Texto <-> tipos do protocolo
// ---------------------------------------------------------------------------

const fn efeito_em_texto(efeito: Efeito) -> &'static str {
    match efeito {
        Efeito::Nenhum => "nenhum",
        Efeito::Aurora => "aurora",
        Efeito::Brilho => "brilho",
        Efeito::Pulso => "pulso",
    }
}

fn efeito_de_texto(texto: &str) -> Result<Efeito> {
    Ok(match texto {
        "nenhum" => Efeito::Nenhum,
        "aurora" => Efeito::Aurora,
        "brilho" => Efeito::Brilho,
        "pulso" => Efeito::Pulso,
        outro => return Err(anyhow!("efeito desconhecido no banco: {outro:?}")),
    })
}

const fn tipo_em_texto(tipo: TipoDaImagem) -> &'static str {
    match tipo {
        TipoDaImagem::Png => "png",
        TipoDaImagem::Jpeg => "jpeg",
        TipoDaImagem::Gif => "gif",
        TipoDaImagem::Webp => "webp",
    }
}

fn tipo_de_texto(texto: &str) -> Result<TipoDaImagem> {
    Ok(match texto {
        "png" => TipoDaImagem::Png,
        "jpeg" => TipoDaImagem::Jpeg,
        "gif" => TipoDaImagem::Gif,
        "webp" => TipoDaImagem::Webp,
        outro => return Err(anyhow!("tipo de imagem desconhecido no banco: {outro:?}")),
    })
}

/// As três colunas de um lado. Nomes fixos no código, nunca vindos de fora: é o
/// que permite montar o SQL sem abrir a porta a injeção.
const fn colunas(lado: LadoDoPerfil) -> (&'static str, &'static str, &'static str) {
    match lado {
        LadoDoPerfil::Foto => ("foto_hash", "foto_tipo", "foto_bytes"),
        LadoDoPerfil::Capa => ("capa_hash", "capa_tipo", "capa_bytes"),
    }
}

fn imagem_da_linha(
    hash: Option<Vec<u8>>,
    tipo: Option<String>,
    bytes: Option<i64>,
) -> Result<Option<ImagemDoPerfil>> {
    let Some(hash) = hash else {
        return Ok(None);
    };
    let hash: [u8; 32] = hash
        .try_into()
        .map_err(|_| anyhow!("hash de imagem de perfil com tamanho errado no banco"))?;
    let tipo = tipo_de_texto(&tipo.ok_or_else(|| anyhow!("imagem de perfil sem tipo"))?)?;
    let bytes = bytes.ok_or_else(|| anyhow!("imagem de perfil sem tamanho"))?;
    Ok(Some(ImagemDoPerfil {
        hash,
        tipo,
        bytes: u32::try_from(bytes).context("tamanho de imagem de perfil fora da faixa")?,
    }))
}

const SELECIONAR: &str = "SELECT pessoa, revisao, nome_exibido, pronomes, status, bio, cor, efeito, \
     foto_hash, foto_tipo, foto_bytes, capa_hash, capa_tipo, capa_bytes FROM perfis";

fn perfil_da_linha(linha: &rusqlite::Row<'_>) -> Result<PerfilPublico> {
    let pessoa: i64 = linha.get(0)?;
    let revisao: i64 = linha.get(1)?;
    let cor: String = linha.get(6)?;
    let efeito: String = linha.get(7)?;
    Ok(PerfilPublico {
        pessoa: PersonId(pessoa as u64),
        revisao: u32::try_from(revisao).context("revisão fora da faixa")?,
        nome_exibido: linha.get(2)?,
        pronomes: linha.get(3)?,
        status: linha.get(4)?,
        bio: linha.get(5)?,
        // Uma cor ilegível no banco não derruba a lista inteira: volta ao padrão.
        cor: cor_de_texto(&cor).unwrap_or(COR_PADRAO),
        efeito: efeito_de_texto(&efeito)?,
        foto: imagem_da_linha(linha.get(8)?, linha.get(9)?, linha.get(10)?)?,
        capa: imagem_da_linha(linha.get(11)?, linha.get(12)?, linha.get(13)?)?,
    })
}

fn imediata(persistence: &Persistence) -> Result<Transaction<'_>> {
    // `IMMEDIATE`: pega o direito de escrever já no começo. Com uma conexão só
    // atrás de um mutex não há disputa hoje, mas ler a revisão e escrever
    // depois só é atômico se ninguém escreve no meio — e dizer isso no código é
    // melhor que depender do mutex de quem chama.
    Transaction::new_unchecked(persistence.connection(), TransactionBehavior::Immediate)
        .map_err(Into::into)
}

fn revisao_atual(conexao: &Connection, pessoa: PersonId) -> Result<u32> {
    let revisao: Option<i64> = conexao
        .query_row(
            "SELECT revisao FROM perfis WHERE pessoa = ?1",
            [pessoa.get() as i64],
            |linha| linha.get(0),
        )
        .optional()?;
    revisao.map_or(Ok(0), |r| u32::try_from(r).context("revisão fora da faixa"))
}

fn proxima(revisao: u32) -> Result<u32> {
    revisao
        .checked_add(1)
        .ok_or_else(|| anyhow!("a revisão do perfil estourou o u32"))
}

// ---------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------

/// O perfil de uma pessoa, ou `None` quando ela nunca gravou nem migrou um.
///
/// # Errors
///
/// Falha se o banco não responder ou guardar um valor impossível.
pub fn ler(persistence: &Persistence, pessoa: PersonId) -> Result<Option<PerfilPublico>> {
    let sql = format!("{SELECIONAR} WHERE pessoa = ?1");
    let mut consulta = persistence.connection().prepare(&sql)?;
    let mut linhas = consulta.query([pessoa.get() as i64])?;
    linhas.next()?.map(perfil_da_linha).transpose()
}

/// Todos os perfis, por pessoa. É o que o anúncio `Perfis` reparte em partes de
/// [`seele_proto::perfil::MAX_PERFIS_POR_PARTE`].
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn todos(persistence: &Persistence) -> Result<Vec<PerfilPublico>> {
    let sql = format!("{SELECIONAR} ORDER BY pessoa");
    let mut consulta = persistence.connection().prepare(&sql)?;
    let mut linhas = consulta.query([])?;
    let mut achados = Vec::new();
    while let Some(linha) = linhas.next()? {
        achados.push(perfil_da_linha(linha)?);
    }
    Ok(achados)
}

// ---------------------------------------------------------------------------
// Escrita
// ---------------------------------------------------------------------------

/// Reaplica [`validar`] ao que já veio validado. Defesa: a validação mora em
/// `seele_proto::perfil`, e quem chama deveria tê-la feito; se não fez, a
/// linha não entra no banco.
fn revalidar(perfil: &PerfilValidado) -> Result<()> {
    let cor = cor_em_texto(perfil.cor);
    let de_novo = validar(&EntradaDePerfil {
        nome_exibido: &perfil.nome_exibido,
        pronomes: &perfil.pronomes,
        status: &perfil.status,
        bio: &perfil.bio,
        cor: &cor,
    })
    .map_err(|erros| anyhow!("perfil não passa em `validar`: {erros:?}"))?;
    if &de_novo != perfil {
        return Err(anyhow!("perfil não está aparado: `validar` o mudaria"));
    }
    Ok(())
}

/// Grava os campos de texto, a cor e o efeito, se a revisão esperada bate.
///
/// `revisao_esperada` é a que a pessoa viu ao abrir o editor; **0 vale para quem
/// ainda não tem linha**. As imagens não são tocadas. Devolve a revisão nova.
///
/// # Errors
///
/// `Ok(Err(RevisaoVelha))` quando a revisão não bate — nada é escrito. `Err`
/// quando o banco falha, quando o perfil não passa em [`validar`] (bug de quem
/// chama) ou quando a pessoa não existe em `people`.
pub fn gravar(
    persistence: &Persistence,
    pessoa: PersonId,
    revisao_esperada: u32,
    perfil: &PerfilValidado,
    efeito: Efeito,
) -> Result<std::result::Result<u32, RevisaoVelha>> {
    revalidar(perfil)?;
    let transacao = imediata(persistence)?;
    let atual = revisao_atual(&transacao, pessoa)?;
    if atual != revisao_esperada {
        return Ok(Err(RevisaoVelha { atual }));
    }
    let nova = proxima(atual)?;
    transacao.execute(
        "INSERT INTO perfis (pessoa, revisao, nome_exibido, pronomes, status, bio, cor, efeito)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(pessoa) DO UPDATE SET
           revisao = excluded.revisao, nome_exibido = excluded.nome_exibido,
           pronomes = excluded.pronomes, status = excluded.status, bio = excluded.bio,
           cor = excluded.cor, efeito = excluded.efeito",
        params![
            pessoa.get() as i64,
            i64::from(nova),
            perfil.nome_exibido,
            perfil.pronomes,
            perfil.status,
            perfil.bio,
            cor_em_texto(perfil.cor),
            efeito_em_texto(efeito),
        ],
    )?;
    transacao.commit()?;
    Ok(Ok(nova))
}

fn hash_atual(conexao: &Connection, pessoa: PersonId, lado: LadoDoPerfil) -> Result<Option<[u8; 32]>> {
    let (hash, _, _) = colunas(lado);
    let guardado: Option<Option<Vec<u8>>> = conexao
        .query_row(
            &format!("SELECT {hash} FROM perfis WHERE pessoa = ?1"),
            [pessoa.get() as i64],
            |linha| linha.get(0),
        )
        .optional()?;
    guardado
        .flatten()
        .map(|bytes| {
            <[u8; 32]>::try_from(bytes)
                .map_err(|_| anyhow!("hash de imagem de perfil com tamanho errado no banco"))
        })
        .transpose()
}

/// Põe `imagem` no lado `lado`, criando a linha (com os textos vazios) se a
/// pessoa ainda não tinha. Sobe a revisão. Não confere a revisão esperada.
///
/// O arquivo da imagem **já tem de estar em disco** quando isto é chamado: a
/// linha só aponta para o que existe.
///
/// # Errors
///
/// Falha se o banco não responder ou se a pessoa não existe em `people`.
pub fn trocar_imagem(
    persistence: &Persistence,
    pessoa: PersonId,
    lado: LadoDoPerfil,
    imagem: &ImagemDoPerfil,
) -> Result<TrocaDeImagem> {
    let transacao = imediata(persistence)?;
    let anterior = hash_atual(&transacao, pessoa, lado)?;
    let nova = proxima(revisao_atual(&transacao, pessoa)?)?;
    let (hash, tipo, bytes) = colunas(lado);
    transacao.execute(
        &format!(
            "INSERT INTO perfis (pessoa, revisao, {hash}, {tipo}, {bytes}) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(pessoa) DO UPDATE SET revisao = excluded.revisao,
               {hash} = excluded.{hash}, {tipo} = excluded.{tipo}, {bytes} = excluded.{bytes}"
        ),
        params![
            pessoa.get() as i64,
            i64::from(nova),
            &imagem.hash[..],
            tipo_em_texto(imagem.tipo),
            i64::from(imagem.bytes),
        ],
    )?;
    transacao.commit()?;
    Ok(TrocaDeImagem {
        anterior,
        revisao: nova,
    })
}

/// Tira a imagem do lado `lado`. Sobe a revisão, a menos que o lado já
/// estivesse vazio — aí não há escrita nenhuma.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn tirar_imagem(
    persistence: &Persistence,
    pessoa: PersonId,
    lado: LadoDoPerfil,
) -> Result<TrocaDeImagem> {
    let transacao = imediata(persistence)?;
    let atual = revisao_atual(&transacao, pessoa)?;
    let Some(anterior) = hash_atual(&transacao, pessoa, lado)? else {
        return Ok(TrocaDeImagem {
            anterior: None,
            revisao: atual,
        });
    };
    let nova = proxima(atual)?;
    let (hash, tipo, bytes) = colunas(lado);
    transacao.execute(
        &format!(
            "UPDATE perfis SET revisao = ?2, {hash} = NULL, {tipo} = NULL, {bytes} = NULL WHERE pessoa = ?1"
        ),
        params![pessoa.get() as i64, i64::from(nova)],
    )?;
    transacao.commit()?;
    Ok(TrocaDeImagem {
        anterior: Some(anterior),
        revisao: nova,
    })
}

/// Se algum perfil ainda aponta para `hash` (na foto ou na capa de qualquer
/// pessoa). Quem apaga um arquivo depois de [`trocar_imagem`] pergunta isto
/// antes.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn hash_em_uso(persistence: &Persistence, hash: &[u8; 32]) -> Result<bool> {
    Ok(persistence.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM perfis WHERE foto_hash = ?1 OR capa_hash = ?1)",
        [&hash[..]],
        |linha| linha.get(0),
    )?)
}

/// Todos os hashes que algum perfil aponta: o que a varredura de órfãos da
/// subida confere contra a pasta `perfis/`.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn hashes_em_uso(persistence: &Persistence) -> Result<HashSet<[u8; 32]>> {
    let mut consulta = persistence.connection().prepare(
        "SELECT foto_hash FROM perfis WHERE foto_hash IS NOT NULL
         UNION SELECT capa_hash FROM perfis WHERE capa_hash IS NOT NULL",
    )?;
    let brutos = consulta
        .query_map([], |linha| linha.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    brutos
        .into_iter()
        .map(|bruto| {
            <[u8; 32]>::try_from(bruto)
                .map_err(|_| anyhow!("hash de imagem de perfil com tamanho errado no banco"))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Livro da migração do MOD PERFIS
// ---------------------------------------------------------------------------

/// O que aconteceu com uma das duas imagens na migração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoDaImagem {
    /// Os bytes foram copiados para `perfis/<sha>`; a linha aponta para eles.
    Trazida(ImagemDoPerfil),
    /// O MOD não tinha essa imagem.
    Nenhuma,
    /// Tinha, e não veio: `nao-encontrada`, `fragmento-faltando`,
    /// `tipo-recusado` ou `grande-demais`.
    Falhou(String),
}

/// O estado de uma imagem como o livro o guarda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoDaImagem {
    /// `trazida`.
    Trazida,
    /// `nenhuma`.
    Nenhuma,
    /// `falhou:<motivo>`.
    Falhou(String),
}

impl EstadoDaImagem {
    fn em_texto(&self) -> String {
        match self {
            Self::Trazida => "trazida".to_owned(),
            Self::Nenhuma => "nenhuma".to_owned(),
            Self::Falhou(motivo) => format!("{PREFIXO_FALHOU}{motivo}"),
        }
    }

    fn de_texto(texto: &str) -> Result<Self> {
        Ok(match texto {
            "trazida" => Self::Trazida,
            "nenhuma" => Self::Nenhuma,
            outro => match outro.strip_prefix(PREFIXO_FALHOU) {
                Some(motivo) => Self::Falhou(motivo.to_owned()),
                None => return Err(anyhow!("estado de migração desconhecido: {outro:?}")),
            },
        })
    }
}

impl From<&ResultadoDaImagem> for EstadoDaImagem {
    fn from(resultado: &ResultadoDaImagem) -> Self {
        match resultado {
            ResultadoDaImagem::Trazida(_) => Self::Trazida,
            ResultadoDaImagem::Nenhuma => Self::Nenhuma,
            ResultadoDaImagem::Falhou(motivo) => Self::Falhou(motivo.clone()),
        }
    }
}

/// Uma pessoa migrada, com tudo que `registrar_migracao` grava de uma vez.
#[derive(Debug, Clone)]
pub struct MigracaoDaPessoa<'a> {
    /// De quem é o perfil.
    pub pessoa: PersonId,
    /// A `revision` que o MOD tinha; vira a `revisao` do perfil oficial.
    pub revisao_no_mod: u32,
    /// Os textos e a cor, já revalidados (campo inválido virou `""`).
    pub perfil: &'a PerfilValidado,
    /// O efeito, já traduzido (`sparkle` → `Brilho`, `pulse` → `Pulso`).
    pub efeito: Efeito,
    /// O que foi da foto (`avatar`).
    pub foto: ResultadoDaImagem,
    /// O que foi da capa (`banner`).
    pub capa: ResultadoDaImagem,
}

/// O que `registrar_migracao` fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesfechoDaMigracao {
    /// Perfil e livro gravados juntos.
    Registrada,
    /// Já estava no livro: nada foi tocado.
    JaMigrada,
    /// A pessoa não existe em `people`: nada foi gravado (e ela não é criada).
    /// Quem chama a põe no relatório.
    PessoaInexistente,
    /// O perfil oficial já passou da revisão do MOD (a pessoa editou): o perfil
    /// **não** é sobrescrito. O livro recebe a linha, com as imagens como
    /// `nenhuma` e já avisada — não há nada a corrigir.
    PerfilMaisNovo,
}

/// Uma linha do livro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistroDaMigracao {
    /// A `revision` que o MOD tinha.
    pub revisao_no_mod: u32,
    /// O estado da foto.
    pub foto: EstadoDaImagem,
    /// O estado da capa.
    pub capa: EstadoDaImagem,
    /// Se a pessoa já foi avisada da falha.
    pub avisada: bool,
}

/// Uma imagem que o editor da pessoa precisa avisar que não veio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendenciaDeImagem {
    /// Qual das duas.
    pub lado: LadoDoPerfil,
    /// O motivo (`fragmento-faltando`, ...).
    pub motivo: String,
}

fn pessoa_existe(conexao: &Connection, pessoa: PersonId) -> Result<bool> {
    Ok(conexao.query_row(
        "SELECT EXISTS(SELECT 1 FROM people WHERE id = ?1)",
        [pessoa.get() as i64],
        |linha| linha.get(0),
    )?)
}

fn imagem_de(resultado: &ResultadoDaImagem) -> Option<&ImagemDoPerfil> {
    match resultado {
        ResultadoDaImagem::Trazida(imagem) => Some(imagem),
        _ => None,
    }
}

/// Registra a migração de uma pessoa: o perfil e a linha do livro, **na mesma
/// transação**. Quem já está no livro é pulado.
///
/// A revisão do perfil oficial é a que o MOD tinha. Se já existe um perfil
/// oficial com revisão maior, ele não é sobrescrito ([`DesfechoDaMigracao::PerfilMaisNovo`]).
///
/// # Errors
///
/// Falha se o banco não responder ou se o perfil não passa em [`validar`].
pub fn registrar_migracao(
    persistence: &Persistence,
    migracao: &MigracaoDaPessoa<'_>,
) -> Result<DesfechoDaMigracao> {
    revalidar(migracao.perfil)?;
    let transacao = imediata(persistence)?;
    let pessoa = migracao.pessoa;
    let ja: bool = transacao.query_row(
        "SELECT EXISTS(SELECT 1 FROM perfis_migracao WHERE pessoa = ?1)",
        [pessoa.get() as i64],
        |linha| linha.get(0),
    )?;
    if ja {
        return Ok(DesfechoDaMigracao::JaMigrada);
    }
    if !pessoa_existe(&transacao, pessoa)? {
        return Ok(DesfechoDaMigracao::PessoaInexistente);
    }
    let revisao_do_mod = i64::from(migracao.revisao_no_mod);
    let mais_novo = revisao_atual(&transacao, pessoa)? > migracao.revisao_no_mod;
    let (foto, capa, avisada) = if mais_novo {
        (EstadoDaImagem::Nenhuma, EstadoDaImagem::Nenhuma, true)
    } else {
        let (foto, capa) = (&migracao.foto, &migracao.capa);
        transacao.execute(
            "INSERT OR REPLACE INTO perfis
               (pessoa, revisao, nome_exibido, pronomes, status, bio, cor, efeito) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                pessoa.get() as i64,
                revisao_do_mod,
                migracao.perfil.nome_exibido,
                migracao.perfil.pronomes,
                migracao.perfil.status,
                migracao.perfil.bio,
                cor_em_texto(migracao.perfil.cor),
                efeito_em_texto(migracao.efeito),
            ],
        )?;
        for (lado, imagem) in [
            (LadoDoPerfil::Foto, imagem_de(foto)),
            (LadoDoPerfil::Capa, imagem_de(capa)),
        ] {
            if let Some(imagem) = imagem {
                let (hash, tipo, bytes) = colunas(lado);
                transacao.execute(
                    &format!("UPDATE perfis SET {hash} = ?2, {tipo} = ?3, {bytes} = ?4 WHERE pessoa = ?1"),
                    params![
                        pessoa.get() as i64,
                        &imagem.hash[..],
                        tipo_em_texto(imagem.tipo),
                        i64::from(imagem.bytes),
                    ],
                )?;
            }
        }
        let falhou = |r: &ResultadoDaImagem| matches!(r, ResultadoDaImagem::Falhou(_));
        (foto.into(), capa.into(), !(falhou(foto) || falhou(capa)))
    };
    transacao.execute(
        "INSERT INTO perfis_migracao (pessoa, revisao_no_mod, foto, capa, avisada) VALUES (?1,?2,?3,?4,?5)",
        params![
            pessoa.get() as i64,
            revisao_do_mod,
            foto.em_texto(),
            capa.em_texto(),
            i64::from(avisada),
        ],
    )?;
    transacao.commit()?;
    Ok(if mais_novo {
        DesfechoDaMigracao::PerfilMaisNovo
    } else {
        DesfechoDaMigracao::Registrada
    })
}

/// Se a pessoa já está no livro.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn ja_migrado(persistence: &Persistence, pessoa: PersonId) -> Result<bool> {
    Ok(persistence.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM perfis_migracao WHERE pessoa = ?1)",
        [pessoa.get() as i64],
        |linha| linha.get(0),
    )?)
}

/// A linha do livro de uma pessoa.
///
/// # Errors
///
/// Falha se o banco não responder ou guardar um estado desconhecido.
pub fn registro_da_migracao(
    persistence: &Persistence,
    pessoa: PersonId,
) -> Result<Option<RegistroDaMigracao>> {
    let bruto: Option<(i64, String, String, i64)> = persistence
        .connection()
        .query_row(
            "SELECT revisao_no_mod, foto, capa, avisada FROM perfis_migracao WHERE pessoa = ?1",
            [pessoa.get() as i64],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
        )
        .optional()?;
    bruto
        .map(|(revisao, foto, capa, avisada)| {
            Ok(RegistroDaMigracao {
                revisao_no_mod: u32::try_from(revisao).context("revisão do MOD fora da faixa")?,
                foto: EstadoDaImagem::de_texto(&foto)?,
                capa: EstadoDaImagem::de_texto(&capa)?,
                avisada: avisada != 0,
            })
        })
        .transpose()
}

/// As imagens que não vieram **e ainda não foram avisadas** à pessoa: o que
/// `PendenciaDoPerfil` carrega. Vazio quando não há falha, ou quando ela já foi
/// avisada.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn pendencias(persistence: &Persistence, pessoa: PersonId) -> Result<Vec<PendenciaDeImagem>> {
    let Some(registro) = registro_da_migracao(persistence, pessoa)? else {
        return Ok(Vec::new());
    };
    if registro.avisada {
        return Ok(Vec::new());
    }
    Ok([(LadoDoPerfil::Foto, registro.foto), (LadoDoPerfil::Capa, registro.capa)]
        .into_iter()
        .filter_map(|(lado, estado)| match estado {
            EstadoDaImagem::Falhou(motivo) => Some(PendenciaDeImagem { lado, motivo }),
            _ => None,
        })
        .collect())
}

/// Marca a pessoa como avisada: `pendencias` passa a voltar vazio.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn marcar_avisada(persistence: &Persistence, pessoa: PersonId) -> Result<()> {
    persistence.connection().execute(
        "UPDATE perfis_migracao SET avisada = 1 WHERE pessoa = ?1",
        [pessoa.get() as i64],
    )?;
    Ok(())
}

/// O caso em que o arquivo de origem de uma imagem `falhou:` apareceu depois: a
/// imagem entra, o estado vira `trazida`, a revisão sobe.
///
/// Só age quando o estado do lado é `falhou:` **e** o lado está vazio no perfil.
/// Se a pessoa já escolheu uma imagem sua nesse meio-tempo, a dela vale: o
/// estado vira `nenhuma` e nada mais muda. Devolve `None` quando não havia o que
/// completar.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn completar_imagem_migrada(
    persistence: &Persistence,
    pessoa: PersonId,
    lado: LadoDoPerfil,
    imagem: &ImagemDoPerfil,
) -> Result<Option<TrocaDeImagem>> {
    let Some(registro) = registro_da_migracao(persistence, pessoa)? else {
        return Ok(None);
    };
    let estado = match lado {
        LadoDoPerfil::Foto => &registro.foto,
        LadoDoPerfil::Capa => &registro.capa,
    };
    if !matches!(estado, EstadoDaImagem::Falhou(_)) {
        return Ok(None);
    }
    let coluna = match lado {
        LadoDoPerfil::Foto => "foto",
        LadoDoPerfil::Capa => "capa",
    };
    let ocupado = hash_atual(persistence.connection(), pessoa, lado)?.is_some();
    let novo_estado = if ocupado { "nenhuma" } else { "trazida" };
    let troca = if ocupado {
        None
    } else {
        Some(trocar_imagem(persistence, pessoa, lado, imagem)?)
    };
    persistence.connection().execute(
        &format!("UPDATE perfis_migracao SET {coluna} = ?2 WHERE pessoa = ?1"),
        params![pessoa.get() as i64, novo_estado],
    )?;
    Ok(troca)
}

// ---------------------------------------------------------------------------
// Relatório da migração, para quem hospeda
// ---------------------------------------------------------------------------

/// Uma coisa que a migração não conseguiu fazer por inteiro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcorrenciaDaMigracao {
    /// A pessoa, quando se sabe qual (`None` para falhas do conjunto).
    pub pessoa: Option<u64>,
    /// O que houve, em uma frase para quem hospeda: `capa: fragmento-faltando`,
    /// `pessoa inexistente`, `campo bio inválido, ficou vazio`...
    pub assunto: String,
}

/// O que `camada-hospedagem.js` mostra a quem hospeda, até ser dispensado.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RelatorioDaMigracaoDePerfis {
    /// Quando terminou, em segundos desde 1970.
    pub feita_em: i64,
    /// Quantas pessoas tiveram o perfil trazido.
    pub trazidos: u32,
    /// `true` quando o próprio banco falhou no meio («migração incompleta»): o
    /// MOD fica desligado e a próxima subida tenta de novo.
    pub incompleta: bool,
    /// Cada coisa que não veio inteira. Vazio quando tudo veio.
    pub ocorrencias: Vec<OcorrenciaDaMigracao>,
}

/// Grava o relatório (substituindo o anterior).
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn gravar_relatorio(
    persistence: &Persistence,
    relatorio: &RelatorioDaMigracaoDePerfis,
) -> Result<()> {
    persistence.connection().execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![CHAVE_DO_RELATORIO, serde_json::to_string(relatorio)?],
    )?;
    Ok(())
}

/// O relatório que ainda não foi dispensado.
///
/// # Errors
///
/// Falha se o banco não responder ou se o texto guardado não é um relatório.
pub fn relatorio(persistence: &Persistence) -> Result<Option<RelatorioDaMigracaoDePerfis>> {
    let texto: Option<String> = persistence
        .connection()
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            [CHAVE_DO_RELATORIO],
            |linha| linha.get(0),
        )
        .optional()?;
    texto
        .map(|texto| serde_json::from_str(&texto).context("relatório da migração ilegível"))
        .transpose()
}

/// Dispensa o relatório: quem hospeda o leu.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn dispensar_relatorio(persistence: &Persistence) -> Result<()> {
    persistence
        .connection()
        .execute("DELETE FROM config WHERE key = ?1", [CHAVE_DO_RELATORIO])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Location;

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).unwrap()
    }

    fn pessoa(p: &Persistence, n: u8) -> PersonId {
        p.connection()
            .execute(
                "INSERT INTO people (nickname, public_key, created_at, last_seen_at) VALUES (?1, ?2, 0, 0)",
                params![format!("p{n}"), &[n; 32][..]],
            )
            .unwrap();
        PersonId(p.connection().last_insert_rowid() as u64)
    }

    fn validado(nome: &str) -> PerfilValidado {
        validar(&EntradaDePerfil {
            nome_exibido: nome,
            pronomes: "ela/dela",
            status: "ao vivo",
            bio: "linha um\nlinha dois",
            cor: "#A1B2C3",
        })
        .unwrap()
    }

    fn imagem(n: u8) -> ImagemDoPerfil {
        ImagemDoPerfil {
            hash: [n; 32],
            tipo: TipoDaImagem::Png,
            bytes: 1000 + u32::from(n),
        }
    }

    #[test]
    fn o_esquema_cria_as_duas_tabelas_na_20() {
        let p = banco();
        let versao: i64 = p
            .connection()
            .query_row(
                "SELECT version FROM schema_version WHERE version = 20",
                [],
                |l| l.get(0),
            )
            .unwrap();
        assert_eq!(versao, 20);
        for tabela in ["perfis", "perfis_migracao"] {
            let n: i64 = p
                .connection()
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [tabela],
                    |l| l.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{tabela}");
        }
    }

    #[test]
    fn gravar_e_ler_dao_o_que_foi_gravado() {
        let p = banco();
        let ana = pessoa(&p, 1);
        assert_eq!(ler(&p, ana).unwrap(), None);
        let nova = gravar(&p, ana, 0, &validado("Ana"), Efeito::Brilho)
            .unwrap()
            .unwrap();
        assert_eq!(nova, 1);
        let lido = ler(&p, ana).unwrap().unwrap();
        assert_eq!(lido.revisao, 1);
        assert_eq!(lido.nome_exibido, "Ana");
        assert_eq!(lido.bio, "linha um\nlinha dois");
        assert_eq!(lido.cor, [0xa1, 0xb2, 0xc3]);
        assert_eq!(lido.efeito, Efeito::Brilho);
        assert_eq!(todos(&p).unwrap(), vec![lido]);
    }

    /// **A revisão velha é recusada e nada é escrito.** Reverter a conferência
    /// em `gravar` faz este teste falhar.
    #[test]
    fn revisao_velha_e_recusada() {
        let p = banco();
        let ana = pessoa(&p, 1);
        gravar(&p, ana, 0, &validado("Ana"), Efeito::Nenhum).unwrap().unwrap();
        // Outra janela salvou antes: a que ainda acha que a revisão é 0 perde.
        let recusa = gravar(&p, ana, 0, &validado("Outra"), Efeito::Nenhum).unwrap();
        assert_eq!(recusa, Err(RevisaoVelha { atual: 1 }));
        assert_eq!(ler(&p, ana).unwrap().unwrap().nome_exibido, "Ana");
        // Com a revisão certa, passa e sobe.
        assert_eq!(
            gravar(&p, ana, 1, &validado("Ana 2"), Efeito::Nenhum).unwrap(),
            Ok(2)
        );
    }

    #[test]
    fn a_imagem_sobe_a_revisao_e_derruba_o_editor_velho() {
        let p = banco();
        let ana = pessoa(&p, 1);
        gravar(&p, ana, 0, &validado("Ana"), Efeito::Nenhum).unwrap().unwrap();
        trocar_imagem(&p, ana, LadoDoPerfil::Foto, &imagem(1)).unwrap();
        assert_eq!(ler(&p, ana).unwrap().unwrap().revisao, 2);
        assert_eq!(
            gravar(&p, ana, 1, &validado("Ana"), Efeito::Nenhum).unwrap(),
            Err(RevisaoVelha { atual: 2 })
        );
    }

    /// **Trocar devolve o hash anterior** (o que o chamador apaga depois do
    /// commit), e `gravar` não mexe nas imagens. Reverter a leitura do hash
    /// anterior em `trocar_imagem` faz este teste falhar.
    #[test]
    fn trocar_a_imagem_devolve_o_hash_anterior() {
        let p = banco();
        let ana = pessoa(&p, 1);
        let primeira = trocar_imagem(&p, ana, LadoDoPerfil::Foto, &imagem(1)).unwrap();
        assert_eq!(primeira.anterior, None);
        assert_eq!(primeira.revisao, 1);
        let segunda = trocar_imagem(&p, ana, LadoDoPerfil::Foto, &imagem(2)).unwrap();
        assert_eq!(segunda.anterior, Some([1; 32]));
        assert_eq!(segunda.revisao, 2);
        // Texto gravado depois não leva a foto embora.
        gravar(&p, ana, 2, &validado("Ana"), Efeito::Nenhum).unwrap().unwrap();
        let lido = ler(&p, ana).unwrap().unwrap();
        assert_eq!(lido.foto, Some(imagem(2)));
        assert_eq!(lido.capa, None);

        assert!(!hash_em_uso(&p, &[1; 32]).unwrap());
        assert!(hash_em_uso(&p, &[2; 32]).unwrap());

        let tirada = tirar_imagem(&p, ana, LadoDoPerfil::Foto).unwrap();
        assert_eq!(tirada.anterior, Some([2; 32]));
        assert_eq!(tirada.revisao, 4);
        assert_eq!(ler(&p, ana).unwrap().unwrap().foto, None);
        // Tirar de um lado vazio não escreve.
        let nada = tirar_imagem(&p, ana, LadoDoPerfil::Foto).unwrap();
        assert_eq!((nada.anterior, nada.revisao), (None, 4));
    }

    #[test]
    fn o_arquivo_dividido_so_e_orfao_quando_ninguem_o_aponta() {
        let p = banco();
        let (ana, bia) = (pessoa(&p, 1), pessoa(&p, 2));
        trocar_imagem(&p, ana, LadoDoPerfil::Foto, &imagem(7)).unwrap();
        trocar_imagem(&p, bia, LadoDoPerfil::Capa, &imagem(7)).unwrap();
        let troca = trocar_imagem(&p, ana, LadoDoPerfil::Foto, &imagem(8)).unwrap();
        assert_eq!(troca.anterior, Some([7; 32]));
        assert!(hash_em_uso(&p, &[7; 32]).unwrap(), "a capa da Bia ainda o usa");
        assert_eq!(
            hashes_em_uso(&p).unwrap(),
            HashSet::from([[7; 32], [8; 32]])
        );
    }

    /// **A chave estrangeira com `people` vale.**
    #[test]
    fn so_existe_perfil_de_quem_existe() {
        let p = banco();
        let fantasma = PersonId(9999);
        assert!(gravar(&p, fantasma, 0, &validado("X"), Efeito::Nenhum).is_err());
        assert!(trocar_imagem(&p, fantasma, LadoDoPerfil::Capa, &imagem(1)).is_err());
        assert_eq!(todos(&p).unwrap(), vec![]);
    }

    #[test]
    fn um_perfil_que_nao_passa_em_validar_nao_entra() {
        let p = banco();
        let ana = pessoa(&p, 1);
        let mut ruim = validado("Ana");
        ruim.nome_exibido = "x\u{202e}y".to_owned();
        assert!(gravar(&p, ana, 0, &ruim, Efeito::Nenhum).is_err());
        let mut sem_aparar = validado("Ana");
        sem_aparar.nome_exibido = " Ana ".to_owned();
        assert!(gravar(&p, ana, 0, &sem_aparar, Efeito::Nenhum).is_err());
        assert_eq!(ler(&p, ana).unwrap(), None);
    }

    fn migracao<'a>(
        pessoa: PersonId,
        revisao: u32,
        perfil: &'a PerfilValidado,
        foto: ResultadoDaImagem,
        capa: ResultadoDaImagem,
    ) -> MigracaoDaPessoa<'a> {
        MigracaoDaPessoa {
            pessoa,
            revisao_no_mod: revisao,
            perfil,
            efeito: Efeito::Aurora,
            foto,
            capa,
        }
    }

    /// **O livro da migração faz a viagem de ida e volta.** Reverter o
    /// `INSERT` do livro (ou o `ja` de `registrar_migracao`) faz falhar.
    #[test]
    fn o_livro_da_migracao_faz_a_viagem_de_ida_e_volta() {
        let p = banco();
        let ana = pessoa(&p, 1);
        assert!(!ja_migrado(&p, ana).unwrap());
        let perfil = validado("Ana");
        let m = migracao(
            ana,
            5,
            &perfil,
            ResultadoDaImagem::Trazida(imagem(3)),
            ResultadoDaImagem::Falhou("fragmento-faltando".into()),
        );
        assert_eq!(registrar_migracao(&p, &m).unwrap(), DesfechoDaMigracao::Registrada);
        assert!(ja_migrado(&p, ana).unwrap());

        let lido = ler(&p, ana).unwrap().unwrap();
        assert_eq!((lido.revisao, lido.efeito), (5, Efeito::Aurora));
        assert_eq!(lido.foto, Some(imagem(3)));
        assert_eq!(lido.capa, None);

        let registro = registro_da_migracao(&p, ana).unwrap().unwrap();
        assert_eq!(registro.revisao_no_mod, 5);
        assert_eq!(registro.foto, EstadoDaImagem::Trazida);
        assert_eq!(registro.capa, EstadoDaImagem::Falhou("fragmento-faltando".into()));
        assert!(!registro.avisada);

        assert_eq!(
            pendencias(&p, ana).unwrap(),
            vec![PendenciaDeImagem {
                lado: LadoDoPerfil::Capa,
                motivo: "fragmento-faltando".into()
            }]
        );
        marcar_avisada(&p, ana).unwrap();
        assert_eq!(pendencias(&p, ana).unwrap(), vec![]);

        // Segunda vez: pulada, nada muda.
        assert_eq!(registrar_migracao(&p, &m).unwrap(), DesfechoDaMigracao::JaMigrada);
        assert_eq!(ler(&p, ana).unwrap().unwrap(), lido);
    }

    #[test]
    fn a_migracao_nao_cria_pessoa_nem_sobrescreve_perfil_mais_novo() {
        let p = banco();
        let perfil = validado("Antigo");
        let m = migracao(
            PersonId(4242),
            1,
            &perfil,
            ResultadoDaImagem::Nenhuma,
            ResultadoDaImagem::Nenhuma,
        );
        assert_eq!(
            registrar_migracao(&p, &m).unwrap(),
            DesfechoDaMigracao::PessoaInexistente
        );
        assert!(!ja_migrado(&p, PersonId(4242)).unwrap());

        let ana = pessoa(&p, 1);
        gravar(&p, ana, 0, &validado("Nova"), Efeito::Nenhum).unwrap().unwrap();
        gravar(&p, ana, 1, &validado("Nova"), Efeito::Nenhum).unwrap().unwrap();
        let m = migracao(
            ana,
            1,
            &perfil,
            ResultadoDaImagem::Falhou("nao-encontrada".into()),
            ResultadoDaImagem::Nenhuma,
        );
        assert_eq!(registrar_migracao(&p, &m).unwrap(), DesfechoDaMigracao::PerfilMaisNovo);
        assert_eq!(ler(&p, ana).unwrap().unwrap().nome_exibido, "Nova");
        assert_eq!(pendencias(&p, ana).unwrap(), vec![], "não há o que corrigir");
        assert!(ja_migrado(&p, ana).unwrap());
    }

    #[test]
    fn uma_imagem_que_apareceu_depois_completa_a_migracao() {
        let p = banco();
        let ana = pessoa(&p, 1);
        let perfil = validado("Ana");
        let m = migracao(
            ana,
            3,
            &perfil,
            ResultadoDaImagem::Nenhuma,
            ResultadoDaImagem::Falhou("nao-encontrada".into()),
        );
        registrar_migracao(&p, &m).unwrap();
        // Lado que não falhou: nada a completar.
        assert_eq!(
            completar_imagem_migrada(&p, ana, LadoDoPerfil::Foto, &imagem(1)).unwrap(),
            None
        );
        let troca = completar_imagem_migrada(&p, ana, LadoDoPerfil::Capa, &imagem(9))
            .unwrap()
            .unwrap();
        assert_eq!(troca.revisao, 4);
        assert_eq!(ler(&p, ana).unwrap().unwrap().capa, Some(imagem(9)));
        assert_eq!(
            registro_da_migracao(&p, ana).unwrap().unwrap().capa,
            EstadoDaImagem::Trazida
        );
    }

    #[test]
    fn o_relatorio_faz_a_viagem_de_ida_e_volta_e_se_dispensa() {
        let p = banco();
        assert_eq!(relatorio(&p).unwrap(), None);
        let r = RelatorioDaMigracaoDePerfis {
            feita_em: 1_793_000_000,
            trazidos: 3,
            incompleta: false,
            ocorrencias: vec![
                OcorrenciaDaMigracao {
                    pessoa: Some(2),
                    assunto: "capa: fragmento-faltando".into(),
                },
                OcorrenciaDaMigracao {
                    pessoa: None,
                    assunto: "pessoa inexistente".into(),
                },
            ],
        };
        gravar_relatorio(&p, &r).unwrap();
        assert_eq!(relatorio(&p).unwrap(), Some(r.clone()));
        // Gravar de novo substitui.
        let r2 = RelatorioDaMigracaoDePerfis { trazidos: 4, ..r };
        gravar_relatorio(&p, &r2).unwrap();
        assert_eq!(relatorio(&p).unwrap(), Some(r2));
        dispensar_relatorio(&p).unwrap();
        assert_eq!(relatorio(&p).unwrap(), None);
    }
}
