//! A migração dos perfis do MOD PERFIS para o produto, na subida (F3).
//!
//! `docs/0.19/perfis-oficiais.md`, §4. O MOD guardava os perfis no quintal
//! dele (`mod_data`, chave `profiles`, `mod_id = 'seele/perfis'`) e as imagens
//! na pasta mutável dele, em três formatos (§1):
//!
//! 1. **volume** — `volume:images/<pessoa>-<token>`, bytes crus em
//!    `<dados>/volume/images/…` (PERFIS 3.2 em diante);
//! 2. **fragmentos** — `uploads/<pessoa>-<token>.parts`, um índice
//!    `{format:1,parts,length}` e os pedaços `….parts.<n>` de 6000 caracteres
//!    que, juntos, formam um data-URL base64 (PERFIS 1.2 até a 3.1; ver
//!    `git show 3128927:servidor/main.js`, linhas 88-116, e a leitura em
//!    `servidor/main.js:39-51`);
//! 3. **data-URL num arquivo só** — `uploads/<pessoa>-<token>.txt` com o
//!    data-URL inteiro (PERFIS 1.0 e 1.1).
//!
//! # As regras, e por que cada uma
//!
//! - **Antes do portão `ligados_de_outra_api`.** Um PERFIS de uma API que esta
//!   linha não roda, ligado num banco antigo, impediria a subida; migrado e
//!   desligado antes, ele não impede nada.
//! - **Uma transação por pessoa** (`persistence::perfis::registrar_migracao`):
//!   o perfil e o livro juntos, ou nenhum dos dois.
//! - **Idempotente.** Quem já está no livro é pulado; a exceção é uma imagem
//!   `falhou:` cujo arquivo de origem apareceu depois.
//! - **Nunca sobrescreve um perfil mais novo** que a revisão do MOD.
//! - **Copiar, nunca mover.** O quintal e a pasta do MOD ficam intactos: é o
//!   que deixa voltar à 0.18 sem perder nada.
//! - **Pessoa desconhecida vai para o relatório**, e não é criada.
//!
//! # «O produto sabe e não conta» — aqui, conta
//!
//! Toda falha é dita em três lugares: no `tracing::error!` (a contagem e cada
//! pessoa), no [`RelatorioDaMigracaoDePerfis`] gravado em `config` (que quem
//! hospeda lê até dispensar) e, para a imagem que não veio, no livro
//! (`falhou:<motivo>`), de onde sai o `PendenciaDoPerfil` da própria pessoa.
//! Um erro do próprio banco não derruba a subida: o relatório diz «migração
//! incompleta», o MOD fica desligado e a próxima subida tenta de novo.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use seele_proto::ids::PersonId;
use seele_proto::perfil::{
    validar, CampoDoPerfil, Efeito, EntradaDePerfil, ImagemDoPerfil, LadoDoPerfil, PerfilValidado,
    TipoDaImagem, COR_PADRAO, TETO_DA_IMAGEM,
};

use crate::persistence::perfis::{
    self as banco, DesfechoDaMigracao, EstadoDaImagem, MigracaoDaPessoa, OcorrenciaDaMigracao,
    RelatorioDaMigracaoDePerfis, ResultadoDaImagem,
};
use crate::persistence::Persistence;

/// O identificador do MOD que esta migração lê.
pub const MOD_DOS_PERFIS: &str = "seele/perfis";

/// A chave do quintal onde o MOD guardava os perfis.
pub const CHAVE_DOS_PERFIS: &str = "profiles";

/// O tamanho de cada pedaço de um `.parts`, em caracteres (`CHUNK` do MOD).
pub const PEDACO: usize = 6000;

/// Onde mora a impressão das ocorrências que se repetiriam a cada subida
/// (pessoa desconhecida, entrada ilegível), na tabela `config`.
///
/// Sem ela, uma pessoa que não existe em `people` voltaria ao relatório a cada
/// subida, e um relatório dispensado reapareceria sem nada ter mudado. Com ela,
/// o relatório só é regravado quando há novidade.
const CHAVE_DA_IMPRESSAO: &str = "perfis.migracao.impressao_do_relatado";

/// Por que uma imagem do MOD não veio. O nome é o que o livro guarda depois
/// de `falhou:` e o que o editor da pessoa explica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoDaImagem {
    /// O arquivo (ou o índice) não está na pasta do MOD.
    NaoEncontrada,
    /// O índice `.parts` existe, e um pedaço falta ou veio curto.
    FragmentoFaltando,
    /// Os bytes não são PNG, JPEG, WebP nem GIF (inclui HEIC e base64 ruim).
    TipoRecusado,
    /// Mais que [`TETO_DA_IMAGEM`].
    GrandeDemais,
    /// O índice `.parts` não é o JSON `{format:1,parts,length}`, ou promete um
    /// número de pedaços que o tamanho não dá.
    IndiceIlegivel,
    /// Os bytes vieram, e a pasta `perfis/` não os aceitou (disco cheio,
    /// permissão) — ou não há pasta, num banco em memória.
    NaoGravada,
}

impl MotivoDaImagem {
    /// O nome no livro: `nao-encontrada`, `fragmento-faltando`...
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::NaoEncontrada => "nao-encontrada",
            Self::FragmentoFaltando => "fragmento-faltando",
            Self::TipoRecusado => "tipo-recusado",
            Self::GrandeDemais => "grande-demais",
            Self::IndiceIlegivel => "indice-ilegivel",
            Self::NaoGravada => "nao-gravada",
        }
    }
}

/// O que a subida fez com os perfis do MOD.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesfechoDaSubida {
    /// O relatório gravado nesta subida, quando houve novidade. `None` quando
    /// não havia o que migrar, ou quando tudo já estava migrado e relatado.
    pub relatorio: Option<RelatorioDaMigracaoDePerfis>,
    /// Os MODs incorporados que estavam ligados e foram desligados.
    pub desligados: Vec<crate::mods::incorporados::ModIncorporado>,
}

/// **O que `Server::start` chama**, antes do portão `ligados_de_outra_api`:
/// migra (idempotente) e desliga os MODs incorporados.
///
/// Nunca falha: um erro do banco vira o relatório «migração incompleta» e o
/// MOD desligado (§4, item 5), e a próxima subida tenta de novo. Uma subida
/// que não acontece por causa da migração seria pior do que a migração não
/// ter acontecido — e calada ela não fica, porque o relatório e o registro
/// dizem.
///
/// `dados_do_mod` é `RaizesDosMods::dados_de("seele/perfis")`, quando o
/// servidor tem pasta de MODs; `pasta_das_imagens` é `perfis/` ao lado de
/// `seele.db` ([`crate::diretorio_de_perfis`]), `None` num banco em memória.
pub fn na_subida(
    persistence: &Persistence,
    dados_do_mod: Option<&Path>,
    pasta_das_imagens: Option<&Path>,
) -> DesfechoDaSubida {
    let relatorio = migrar(
        persistence,
        dados_do_mod,
        pasta_das_imagens,
        crate::persistence::now_seconds(),
    );
    // **Depois da migração, sempre** — inclusive quando ela não terminou: o
    // MOD ligado continuaria gravando perfis no quintal que a migração já leu,
    // e o que ele gravasse dali em diante se perderia.
    let desligados = crate::mods::incorporados::desligar_os_ligados(persistence);
    DesfechoDaSubida {
        relatorio,
        desligados,
    }
}

/// A migração em si. Devolve o relatório gravado, quando houve novidade.
///
/// Pública para o teste a chamar com um relógio fixo; quem sobe o servidor
/// chama [`na_subida`].
pub fn migrar(
    persistence: &Persistence,
    dados_do_mod: Option<&Path>,
    pasta_das_imagens: Option<&Path>,
    agora: i64,
) -> Option<RelatorioDaMigracaoDePerfis> {
    let mut estado = Andamento::default();
    if let Err(erro) = percorrer(persistence, dados_do_mod, pasta_das_imagens, &mut estado) {
        tracing::error!(
            %erro,
            trazidos = estado.trazidos,
            "migração dos perfis do PERFIS incompleta: o banco falhou no meio. O MOD fica \
             desligado, e a próxima subida tenta de novo"
        );
        estado.havia_o_que_migrar = true;
        estado.incompleta = true;
        estado.recorrentes.push(OcorrenciaDaMigracao {
            pessoa: None,
            assunto: format!("migração incompleta: o banco falhou ({erro:#})"),
        });
    }
    if !estado.havia_o_que_migrar {
        return None;
    }

    let impressao = impressao_de(&estado.recorrentes);
    let ja_relatada = ler_impressao(persistence).as_deref() == Some(impressao.as_str());
    if !estado.novidade && !estado.incompleta && ja_relatada {
        return None;
    }

    let mut ocorrencias = estado.recorrentes;
    ocorrencias.extend(estado.da_pessoa);
    let relatorio = RelatorioDaMigracaoDePerfis {
        feita_em: agora,
        trazidos: estado.trazidos,
        incompleta: estado.incompleta,
        ocorrencias,
    };
    if relatorio.ocorrencias.is_empty() {
        tracing::info!(
            trazidos = relatorio.trazidos,
            "os perfis do PERFIS foram trazidos para o produto"
        );
    } else {
        tracing::error!(
            falhas = relatorio.ocorrencias.len(),
            trazidos = relatorio.trazidos,
            "a migração dos perfis do PERFIS não trouxe tudo; o relatório fica para quem hospeda"
        );
        for ocorrencia in &relatorio.ocorrencias {
            tracing::error!(
                pessoa = ?ocorrencia.pessoa,
                assunto = %ocorrencia.assunto,
                "perfil do PERFIS não trazido por inteiro"
            );
        }
    }
    if let Err(erro) = banco::gravar_relatorio(persistence, &relatorio) {
        tracing::error!(%erro, "o relatório da migração dos perfis não foi gravado");
    } else if let Err(erro) = gravar_impressao(persistence, &impressao) {
        tracing::error!(%erro, "a impressão do relatório da migração dos perfis não foi gravada");
    }
    Some(relatorio)
}

/// O que a passada vai juntando.
#[derive(Default)]
struct Andamento {
    /// A chave `profiles` existe (ou o banco falhou antes de saber).
    havia_o_que_migrar: bool,
    /// Alguém foi registrado, ou uma imagem completada, nesta subida.
    novidade: bool,
    /// O banco falhou no meio.
    incompleta: bool,
    /// Quantas pessoas tiveram o perfil trazido nesta subida.
    trazidos: u32,
    /// As ocorrências que voltariam a cada subida (pessoa desconhecida,
    /// entrada ilegível): é a impressão delas que decide se há novidade.
    recorrentes: Vec<OcorrenciaDaMigracao>,
    /// As ocorrências de quem está no livro (campos, imagens, perfil mais novo).
    da_pessoa: Vec<OcorrenciaDaMigracao>,
}

fn percorrer(
    persistence: &Persistence,
    dados_do_mod: Option<&Path>,
    pasta_das_imagens: Option<&Path>,
    estado: &mut Andamento,
) -> Result<()> {
    let quintal = crate::persistence::mods::ler_quintal(persistence, MOD_DOS_PERFIS)?;
    let Some(texto) = quintal.get(CHAVE_DOS_PERFIS) else {
        return Ok(());
    };
    estado.havia_o_que_migrar = true;
    let perfis: BTreeMap<String, serde_json::Value> = match serde_json::from_str(texto) {
        Ok(perfis) => perfis,
        Err(erro) => {
            estado.recorrentes.push(OcorrenciaDaMigracao {
                pessoa: None,
                assunto: format!(
                    "a chave «profiles» do PERFIS não é um mapa de perfis legível ({erro}); \
                     nenhum perfil foi trazido"
                ),
            });
            return Ok(());
        }
    };

    for (chave, perfil) in &perfis {
        let Some(pessoa) = chave.parse::<u64>().ok().filter(|n| *n != 0).map(PersonId) else {
            estado.recorrentes.push(OcorrenciaDaMigracao {
                pessoa: None,
                assunto: format!("entrada «{chave}» não é uma pessoa; ignorada"),
            });
            continue;
        };
        let Some(perfil) = perfil.as_object() else {
            estado.recorrentes.push(OcorrenciaDaMigracao {
                pessoa: Some(pessoa.get()),
                assunto: "o perfil não é um objeto; não foi trazido".to_owned(),
            });
            continue;
        };
        let fontes = Fontes {
            dados_do_mod,
            pasta_das_imagens,
        };
        if banco::ja_migrado(persistence, pessoa)? {
            completar_as_que_falharam(persistence, pessoa, perfil, &fontes, estado)?;
        } else {
            migrar_a_pessoa(persistence, pessoa, perfil, &fontes, estado)?;
        }
    }
    Ok(())
}

/// De onde as imagens vêm e para onde vão.
struct Fontes<'a> {
    dados_do_mod: Option<&'a Path>,
    pasta_das_imagens: Option<&'a Path>,
}

type Perfil = serde_json::Map<String, serde_json::Value>;

fn texto<'a>(perfil: &'a Perfil, campo: &str) -> &'a str {
    perfil
        .get(campo)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

const fn nome_do_campo(campo: CampoDoPerfil) -> &'static str {
    match campo {
        CampoDoPerfil::NomeExibido => "nome exibido",
        CampoDoPerfil::Pronomes => "pronomes",
        CampoDoPerfil::Status => "status",
        CampoDoPerfil::Bio => "bio",
        CampoDoPerfil::Cor => "cor",
    }
}

/// Os textos e a cor, revalidados pela regra única (`perfil::validar`): um
/// campo inválido vira `""`, uma cor inválida volta ao padrão, e cada um vira
/// uma ocorrência.
fn revalidar(perfil: &Perfil, avisos: &mut Vec<String>) -> PerfilValidado {
    let mut campos = [
        texto(perfil, "displayName").to_owned(),
        texto(perfil, "pronouns").to_owned(),
        texto(perfil, "status").to_owned(),
        texto(perfil, "bio").to_owned(),
        texto(perfil, "accent").to_ascii_lowercase(),
    ];
    let validados = |c: &[String; 5]| {
        validar(&EntradaDePerfil {
            nome_exibido: &c[0],
            pronomes: &c[1],
            status: &c[2],
            bio: &c[3],
            cor: &c[4],
        })
    };
    let erros = match validados(&campos) {
        Ok(validado) => return validado,
        Err(erros) => erros,
    };
    for erro in erros {
        let indice = match erro.campo {
            CampoDoPerfil::NomeExibido => 0,
            CampoDoPerfil::Pronomes => 1,
            CampoDoPerfil::Status => 2,
            CampoDoPerfil::Bio => 3,
            CampoDoPerfil::Cor => 4,
        };
        let Some(campo) = campos.get_mut(indice) else {
            continue;
        };
        if erro.campo == CampoDoPerfil::Cor {
            avisos.push(format!("cor «{campo}» inválida, voltou ao padrão"));
            *campo = seele_proto::perfil::cor_em_texto(COR_PADRAO);
        } else {
            avisos.push(format!(
                "campo {} inválido ({:?}), ficou vazio",
                nome_do_campo(erro.campo),
                erro.motivo
            ));
            campo.clear();
        }
    }
    // Com os inválidos vazios e a cor padrão, não sobra o que recusar.
    validados(&campos).unwrap_or_else(|_| PerfilValidado {
        nome_exibido: String::new(),
        pronomes: String::new(),
        status: String::new(),
        bio: String::new(),
        cor: COR_PADRAO,
    })
}

/// `none|aurora|sparkle|pulse` → nenhum/aurora/brilho/pulso.
fn efeito(perfil: &Perfil, avisos: &mut Vec<String>) -> Efeito {
    match perfil.get("effect").and_then(serde_json::Value::as_str) {
        None | Some("none") => Efeito::Nenhum,
        Some("aurora") => Efeito::Aurora,
        Some("sparkle") => Efeito::Brilho,
        Some("pulse") => Efeito::Pulso,
        Some(outro) => {
            avisos.push(format!("efeito «{outro}» desconhecido, ficou nenhum"));
            Efeito::Nenhum
        }
    }
}

const fn chave_do_lado(lado: LadoDoPerfil) -> &'static str {
    match lado {
        LadoDoPerfil::Foto => "avatar",
        LadoDoPerfil::Capa => "banner",
    }
}

const fn nome_do_lado(lado: LadoDoPerfil) -> &'static str {
    match lado {
        LadoDoPerfil::Foto => "foto",
        LadoDoPerfil::Capa => "capa",
    }
}

/// O caminho que o MOD guardava para um lado, quando há.
fn caminho_do_lado(perfil: &Perfil, lado: LadoDoPerfil) -> Option<&str> {
    perfil
        .get(chave_do_lado(lado))
        .and_then(serde_json::Value::as_str)
        .filter(|caminho| !caminho.is_empty())
}

/// Lê, confere e copia a imagem de um lado para `perfis/<sha>`.
fn trazer_imagem(fontes: &Fontes<'_>, caminho: &str) -> Result<ImagemDoPerfil, MotivoDaImagem> {
    let dados = fontes.dados_do_mod.ok_or(MotivoDaImagem::NaoEncontrada)?;
    let bytes = ler_imagem_do_mod(dados, caminho)?;
    let tipo = conferir(&bytes)?;
    let pasta = fontes.pasta_das_imagens.ok_or(MotivoDaImagem::NaoGravada)?;
    let tamanho = u32::try_from(bytes.len()).map_err(|_| MotivoDaImagem::GrandeDemais)?;
    let hash = gravar_bytes(pasta, &bytes).map_err(|erro| {
        tracing::error!(%erro, pasta = %pasta.display(), "a imagem migrada não foi gravada");
        MotivoDaImagem::NaoGravada
    })?;
    Ok(ImagemDoPerfil {
        hash,
        tipo,
        bytes: tamanho,
    })
}

fn resultado_do_lado(
    perfil: &Perfil,
    lado: LadoDoPerfil,
    fontes: &Fontes<'_>,
) -> ResultadoDaImagem {
    match caminho_do_lado(perfil, lado) {
        None => ResultadoDaImagem::Nenhuma,
        Some(caminho) => match trazer_imagem(fontes, caminho) {
            Ok(imagem) => ResultadoDaImagem::Trazida(imagem),
            Err(motivo) => ResultadoDaImagem::Falhou(motivo.nome().to_owned()),
        },
    }
}

fn migrar_a_pessoa(
    persistence: &Persistence,
    pessoa: PersonId,
    perfil: &Perfil,
    fontes: &Fontes<'_>,
    estado: &mut Andamento,
) -> Result<()> {
    let mut avisos = Vec::new();
    let validado = revalidar(perfil, &mut avisos);
    let efeito = efeito(perfil, &mut avisos);
    let revisao_no_mod = perfil
        .get("revision")
        .and_then(serde_json::Value::as_u64)
        .map_or(0, |revisao| u32::try_from(revisao).unwrap_or(u32::MAX));
    let foto = resultado_do_lado(perfil, LadoDoPerfil::Foto, fontes);
    let capa = resultado_do_lado(perfil, LadoDoPerfil::Capa, fontes);
    let trazidas: Vec<[u8; 32]> = [&foto, &capa]
        .into_iter()
        .filter_map(|resultado| match resultado {
            ResultadoDaImagem::Trazida(imagem) => Some(imagem.hash),
            _ => None,
        })
        .collect();

    let desfecho = banco::registrar_migracao(
        persistence,
        &MigracaoDaPessoa {
            pessoa,
            revisao_no_mod,
            perfil: &validado,
            efeito,
            foto: foto.clone(),
            capa: capa.clone(),
        },
    );
    let desfecho = match desfecho {
        Ok(desfecho) => desfecho,
        Err(erro) => {
            descartar_orfas(persistence, fontes.pasta_das_imagens, &trazidas);
            return Err(erro.context(format!("ao registrar a pessoa {}", pessoa.get())));
        }
    };
    let ocorrencia = |assunto: String| OcorrenciaDaMigracao {
        pessoa: Some(pessoa.get()),
        assunto,
    };
    match desfecho {
        DesfechoDaMigracao::Registrada => {
            estado.novidade = true;
            estado.trazidos += 1;
            estado.da_pessoa.extend(avisos.into_iter().map(ocorrencia));
            for (lado, resultado) in [(LadoDoPerfil::Foto, &foto), (LadoDoPerfil::Capa, &capa)] {
                if let ResultadoDaImagem::Falhou(motivo) = resultado {
                    estado
                        .da_pessoa
                        .push(ocorrencia(format!("{}: {motivo}", nome_do_lado(lado))));
                }
            }
        }
        DesfechoDaMigracao::PerfilMaisNovo => {
            estado.novidade = true;
            descartar_orfas(persistence, fontes.pasta_das_imagens, &trazidas);
            estado.da_pessoa.push(ocorrencia(
                "o perfil já tinha sido editado no SEELE; o do PERFIS não o sobrescreveu"
                    .to_owned(),
            ));
        }
        DesfechoDaMigracao::PessoaInexistente => {
            descartar_orfas(persistence, fontes.pasta_das_imagens, &trazidas);
            estado.recorrentes.push(ocorrencia(
                "pessoa inexistente neste servidor: o perfil não foi trazido".to_owned(),
            ));
        }
        DesfechoDaMigracao::JaMigrada => {
            descartar_orfas(persistence, fontes.pasta_das_imagens, &trazidas);
        }
    }
    Ok(())
}

/// A exceção da idempotência: uma imagem `falhou:` cujo arquivo de origem
/// apareceu depois entra agora. A que continua faltando continua no relatório,
/// se ele for regravado.
fn completar_as_que_falharam(
    persistence: &Persistence,
    pessoa: PersonId,
    perfil: &Perfil,
    fontes: &Fontes<'_>,
    estado: &mut Andamento,
) -> Result<()> {
    let Some(registro) = banco::registro_da_migracao(persistence, pessoa)? else {
        return Ok(());
    };
    for (lado, situacao) in [
        (LadoDoPerfil::Foto, &registro.foto),
        (LadoDoPerfil::Capa, &registro.capa),
    ] {
        if !matches!(situacao, EstadoDaImagem::Falhou(_)) {
            continue;
        }
        let Some(caminho) = caminho_do_lado(perfil, lado) else {
            continue;
        };
        match trazer_imagem(fontes, caminho) {
            Ok(imagem) => {
                let troca = banco::completar_imagem_migrada(persistence, pessoa, lado, &imagem)?;
                estado.novidade = true;
                if troca.is_some() {
                    tracing::info!(
                        pessoa = pessoa.get(),
                        lado = nome_do_lado(lado),
                        "a imagem do PERFIS que faltava apareceu e foi trazida"
                    );
                } else {
                    descartar_orfas(persistence, fontes.pasta_das_imagens, &[imagem.hash]);
                }
            }
            Err(motivo) => estado.da_pessoa.push(OcorrenciaDaMigracao {
                pessoa: Some(pessoa.get()),
                assunto: format!("{}: {}", nome_do_lado(lado), motivo.nome()),
            }),
        }
    }
    Ok(())
}

/// Apaga os arquivos que esta passada gravou e que nenhum perfil aponta.
/// Uma falha aqui só deixa um órfão, que a varredura da subida recolhe.
fn descartar_orfas(persistence: &Persistence, pasta: Option<&Path>, hashes: &[[u8; 32]]) {
    let Some(pasta) = pasta else { return };
    for hash in hashes {
        if matches!(banco::hash_em_uso(persistence, hash), Ok(false)) {
            let _ = std::fs::remove_file(pasta.join(seele_proto::attachment::hex(hash)));
        }
    }
}

// ---------------------------------------------------------------------------
// Os três formatos de imagem do MOD
// ---------------------------------------------------------------------------

/// O maior data-URL que cabe numa imagem de [`TETO_DA_IMAGEM`]: o base64 dela
/// e um cabeçalho folgado. A conta do MOD 1.2 (`encodedLimit`).
const TETO_DO_DATA_URL: usize = 4 * TETO_DA_IMAGEM.div_ceil(3) + 64;

/// Lê os bytes de uma imagem do MOD, em qualquer dos três formatos.
///
/// **Só lê.** Nada aqui apaga, renomeia ou reescreve um arquivo da pasta do
/// MOD: copiar, nunca mover, é o que deixa voltar à 0.18.
///
/// # Errors
///
/// O [`MotivoDaImagem`] que o livro guarda.
pub fn ler_imagem_do_mod(dados: &Path, caminho: &str) -> Result<Vec<u8>, MotivoDaImagem> {
    if let Some(nome) = caminho.strip_prefix("volume:") {
        let arquivo = crate::mods::arquivos::dentro(&dados.join(crate::mods::volume::PASTA), nome)
            .ok_or(MotivoDaImagem::NaoEncontrada)?;
        let tamanho = std::fs::metadata(&arquivo)
            .map_err(|_| MotivoDaImagem::NaoEncontrada)?
            .len();
        if tamanho > TETO_DA_IMAGEM as u64 {
            return Err(MotivoDaImagem::GrandeDemais);
        }
        return std::fs::read(&arquivo).map_err(|_| MotivoDaImagem::NaoEncontrada);
    }
    let arquivo =
        crate::mods::arquivos::dentro(dados, caminho).ok_or(MotivoDaImagem::NaoEncontrada)?;
    let tamanho = std::fs::metadata(&arquivo)
        .map_err(|_| MotivoDaImagem::NaoEncontrada)?
        .len();
    if tamanho > TETO_DO_DATA_URL as u64 {
        return Err(MotivoDaImagem::GrandeDemais);
    }
    let bruto = std::fs::read_to_string(&arquivo).map_err(|_| MotivoDaImagem::NaoEncontrada)?;
    // O `indexOf` do MOD: um texto que começa por `{` é o índice dos fragmentos.
    let data_url = if bruto.starts_with('{') {
        juntar_fragmentos(&arquivo, &bruto)?
    } else {
        bruto
    };
    decodificar_data_url(&data_url)
}

/// O índice `{format:1,parts,length}` e os pedaços `<índice>.<n>`.
fn juntar_fragmentos(indice: &Path, bruto: &str) -> Result<String, MotivoDaImagem> {
    #[derive(serde::Deserialize)]
    struct Indice {
        format: u32,
        parts: usize,
        length: usize,
    }
    let lido: Indice = serde_json::from_str(bruto).map_err(|_| MotivoDaImagem::IndiceIlegivel)?;
    if lido.format != 1 || lido.parts != lido.length.div_ceil(PEDACO) {
        return Err(MotivoDaImagem::IndiceIlegivel);
    }
    if lido.length > TETO_DO_DATA_URL {
        return Err(MotivoDaImagem::GrandeDemais);
    }
    let nome = indice
        .file_name()
        .and_then(|nome| nome.to_str())
        .ok_or(MotivoDaImagem::NaoEncontrada)?;
    let mut juntado = String::with_capacity(lido.length);
    for n in 0..lido.parts {
        let pedaco = indice.with_file_name(format!("{nome}.{n}"));
        let texto =
            std::fs::read_to_string(&pedaco).map_err(|_| MotivoDaImagem::FragmentoFaltando)?;
        if juntado.len() + texto.len() > lido.length {
            return Err(MotivoDaImagem::FragmentoFaltando);
        }
        juntado.push_str(&texto);
    }
    if juntado.len() != lido.length {
        return Err(MotivoDaImagem::FragmentoFaltando);
    }
    Ok(juntado)
}

/// `data:image/<png|jpeg|webp|gif>;base64,<...>` → bytes.
fn decodificar_data_url(texto: &str) -> Result<Vec<u8>, MotivoDaImagem> {
    let resto = texto
        .strip_prefix("data:image/")
        .ok_or(MotivoDaImagem::TipoRecusado)?;
    let (tipo, base64) = resto
        .split_once(";base64,")
        .ok_or(MotivoDaImagem::TipoRecusado)?;
    if !matches!(tipo, "png" | "jpeg" | "webp" | "gif") {
        return Err(MotivoDaImagem::TipoRecusado);
    }
    let bytes = base64_padrao(base64.trim_end()).ok_or(MotivoDaImagem::TipoRecusado)?;
    if bytes.len() > TETO_DA_IMAGEM {
        return Err(MotivoDaImagem::GrandeDemais);
    }
    Ok(bytes)
}

/// Base64 padrão (RFC 4648 §4, com `=`). À mão: este crate não depende de
/// `base64`, e uma dependência inteira por uma leitura de migração não se paga.
fn base64_padrao(texto: &str) -> Option<Vec<u8>> {
    const fn valor(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = texto.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut saida = Vec::with_capacity(bytes.len() / 4 * 3);
    let total = bytes.len() / 4;
    for (i, quarteto) in bytes.chunks_exact(4).enumerate() {
        let ultimo = i + 1 == total;
        let mut acumulado = 0_u32;
        let mut validos = 0_usize;
        for (j, &c) in quarteto.iter().enumerate() {
            if c == b'=' {
                // `=` só no último quarteto, só nas duas últimas posições, e
                // nada depois dele além de outro `=`.
                if !ultimo || j < 2 || quarteto.get(j..)?.iter().any(|&d| d != b'=') {
                    return None;
                }
                acumulado <<= 6;
            } else {
                acumulado = (acumulado << 6) | valor(c)?;
                validos += 1;
            }
        }
        let [_, a, b, c] = acumulado.to_be_bytes();
        saida.extend_from_slice([a, b, c].get(..validos.checked_sub(1)?)?);
    }
    Some(saida)
}

/// O tipo pelos bytes (`imagem::sniff`) e o teto.
fn conferir(bytes: &[u8]) -> Result<TipoDaImagem, MotivoDaImagem> {
    if bytes.len() > TETO_DA_IMAGEM {
        return Err(MotivoDaImagem::GrandeDemais);
    }
    seele_proto::imagem::sniff(bytes)
        .map(TipoDaImagem::from)
        .ok_or(MotivoDaImagem::TipoRecusado)
}

/// Grava `bytes` em `<pasta>/<sha256 hex>`, passando por `.em-obras-*` e
/// terminando num `rename`, e devolve o hash. Um arquivo já presente com o
/// mesmo hash e o mesmo tamanho é o mesmo conteúdo, e fica como está.
///
/// TODO: usar `perfis::imagens` (F2) quando a gravação de lá existir; esta é a
/// mesma convenção, escrita aqui para a migração não esperar por ela.
fn gravar_bytes(pasta: &Path, bytes: &[u8]) -> std::io::Result<[u8; 32]> {
    use std::io::Write as _;
    let hash = seele_proto::attachment::hash(bytes);
    let nome = seele_proto::attachment::hex(&hash);
    let destino = pasta.join(&nome);
    if std::fs::metadata(&destino).is_ok_and(|m| m.len() == bytes.len() as u64) {
        return Ok(hash);
    }
    std::fs::create_dir_all(pasta)?;
    let obras: PathBuf = pasta.join(format!(".em-obras-migracao-{nome}"));
    let gravado = (|| {
        let mut arquivo = std::fs::File::create(&obras)?;
        arquivo.write_all(bytes)?;
        arquivo.sync_all()?;
        std::fs::rename(&obras, &destino)
    })();
    if gravado.is_err() {
        let _ = std::fs::remove_file(&obras);
    }
    gravado.map(|()| hash)
}

// ---------------------------------------------------------------------------
// A impressão do que já foi relatado
// ---------------------------------------------------------------------------

fn impressao_de(recorrentes: &[OcorrenciaDaMigracao]) -> String {
    let texto = serde_json::to_string(recorrentes).unwrap_or_default();
    seele_proto::attachment::hex(&seele_proto::attachment::hash(texto.as_bytes()))
}

fn ler_impressao(persistence: &Persistence) -> Option<String> {
    use rusqlite::OptionalExtension as _;
    persistence
        .connection()
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            [CHAVE_DA_IMPRESSAO],
            |linha| linha.get(0),
        )
        .optional()
        .ok()
        .flatten()
}

fn gravar_impressao(persistence: &Persistence, impressao: &str) -> Result<()> {
    persistence.connection().execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [CHAVE_DA_IMPRESSAO, impressao],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    /// Codificador de referência, só para o teste.
    fn base64_de(bytes: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for c in bytes.chunks(3) {
            let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= c.len() {
                    s.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }

    #[test]
    fn o_base64_padrao_volta_os_bytes_e_recusa_o_torto() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0xff, 0x00, 0x89]] {
            let texto = base64_de(bytes);
            assert_eq!(base64_padrao(&texto).as_deref(), Some(bytes), "{texto}");
        }
        for torto in ["YQ=", "Y=Q=", "YQ==YQ==", "Y!==", "===="] {
            assert_eq!(base64_padrao(torto), None, "{torto}");
        }
    }

    #[test]
    fn um_indice_que_promete_mais_pedacos_que_o_tamanho_e_ilegivel() {
        let pasta = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(pasta.path().join("uploads")).unwrap();
        std::fs::write(
            pasta.path().join("uploads/1-x.parts"),
            r#"{"format":1,"parts":3,"length":10}"#,
        )
        .unwrap();
        assert_eq!(
            ler_imagem_do_mod(pasta.path(), "uploads/1-x.parts"),
            Err(MotivoDaImagem::IndiceIlegivel)
        );
    }

    #[test]
    fn um_caminho_que_sobe_nao_sai_da_pasta_do_mod() {
        let pasta = tempfile::tempdir().unwrap();
        assert_eq!(
            ler_imagem_do_mod(pasta.path(), "../seele.db"),
            Err(MotivoDaImagem::NaoEncontrada)
        );
        assert_eq!(
            ler_imagem_do_mod(pasta.path(), "volume:../../seele.db"),
            Err(MotivoDaImagem::NaoEncontrada)
        );
    }

    #[test]
    fn um_data_url_que_nao_e_imagem_e_tipo_recusado() {
        assert_eq!(
            decodificar_data_url("data:image/png;base64,aGVsbG8="),
            Ok(b"hello".to_vec())
        );
        let pasta = tempfile::tempdir().unwrap();
        std::fs::write(pasta.path().join("x.txt"), "data:image/png;base64,aGVsbG8=").unwrap();
        // Decodifica, mas os bytes não são PNG: quem decide é o `sniff`.
        assert_eq!(conferir(b"hello"), Err(MotivoDaImagem::TipoRecusado));
        assert_eq!(
            decodificar_data_url("data:text/html;base64,aGVsbG8="),
            Err(MotivoDaImagem::TipoRecusado)
        );
    }
}
