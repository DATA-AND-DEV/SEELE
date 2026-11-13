//! **O PERFIS da API 5 que a 0.15 deixou ligado, no ANTES DE HOSPEDAR da
//! 0.19** (`docs/0.19/perfis-oficiais.md`, §4).
//!
//! O caso: quem hospedou na 0.15 com o `seele/perfis` 3.3.1 (API 5) ligado abre
//! a 0.19. Até a 0.18 a saída era ATUALIZAR para o 4.0.0 (API 6) — e na 0.19 o
//! 4.0.0 é um MOD que o servidor recusa: o PERFIS virou o perfil do produto
//! (`seele_server::mods::incorporados`). A saída agora é uma só, «Perfis agora
//! faz parte do SEELE (0.19). Os perfis deste servidor serão trazidos.», e
//! hospedar segue sem o MOD; a subida do servidor copia os perfis dele para as
//! tabelas do produto (`seele_server::perfis::migracao`). Aqui se prova, pelos
//! comandos de verdade (`resolver_mod_de_api_antiga`,
//! `desfazer_escolha_de_mod_em`, `hospedar_em`) e com as mesmas garantias da
//! travessia de antes:
//!
//! 1. o ANTES DE HOSPEDAR lê o PERFIS como incorporado, e hospedar sem a
//!    escolha recusa com `("seele/perfis", Some(5))` — nada muda;
//! 2. **o 4.0.0 não é oferecido**: ATUALIZAR (com o 4.0.0 no cache, que
//!    serviria) e DESLIGAR são recusados com `IncorporadoAoProduto`, e nada é
//!    estagiado;
//! 3. seguir sem ele (`Incorporado`) **não toca no banco** nem cria cópia;
//! 4. **cancelar** (o DESFAZER) esquece a escolha, o banco segue igual, e o
//!    hospedar volta a recusar;
//! 5. escolhido de novo, o CTA faz a cópia do estado de antes (esquema 13, o
//!    3.3.1 ligado, o quintal) e só então converte; o servidor sobe com os
//!    perfis do 3.3.1 nas tabelas do produto — o texto, a foto do volume e a
//!    capa dos fragmentos —, o relatório dizendo quantos vieram, o MOD
//!    desligado e o quintal e a pasta dele intactos.
//!
//! # O que NÃO é prova aqui, dito
//!
//! - **O pacote 3.3.1 é o do repositório do MOD** (`SEELE-MOD-PERFIS@0774774`:
//!   `mod.json`, `cliente/main.js`, `servidor/main.js`), não o baixado do
//!   catálogo; o hash pode não ser o publicado. A pasta (o banco, as contas, o
//!   MOD ligado, o quintal e a pasta de arquivos dele) foi escrita **pelo
//!   código da v0.15.1** rodando essa metade de servidor (ver o `LEIA.md` da
//!   fixture); só a chegada dos bytes da foto pelo fluxo de volume é simulada.
//! - O 4.0.0 é de mentira (`testes/pasta-da-0.15.1-perfis/perfis-4.0.0/`): só
//!   precisa ser um pacote do PERFIS que este build instalaria.
//! - A tela não é vista: os comandos são chamados por baixo da janela. O que
//!   ela diz é guardado em `tests/frontend.rs`.
//!
//! # Por que em `src/` e não em `tests/`
//!
//! O `seele-app` é só binário (`[[bin]]`, sem `[lib]`): um arquivo em
//! `apps/seele-app/tests/` não alcança `hospedar_em` nem `mods::`, e só
//! conseguiria ler texto. Os comandos de verdade só se chamam de dentro do
//! crate, como `varias_sessoes_testes.rs` faz.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório; e indexar um serde_json::Value devolve Null"
)]

use std::path::{Path, PathBuf};

use seele_proto::ids::PersonId;

use super::{desfazer_escolha_de_mod_em, hospedar_em, FalhaAoHospedar, Session};
use crate::mods::{FalhaAoResolverModAntigo, SaidaDoModAntigo};

/// A pasta de quem hospedou na 0.15.1 com o Perfis 3.3.1 (API 5).
const PASTA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testes/pasta-da-0.15.1-perfis/pasta"
);

/// O Perfis 4.0.0 de mentira, da API 6: o que o ATUALIZAR de antes traria.
const PERFIS_4: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testes/pasta-da-0.15.1-perfis/perfis-4.0.0"
);

const PERFIS: &str = "seele/perfis";

fn copiar(de: &Path, para: &Path) {
    std::fs::create_dir_all(para).unwrap();
    for entrada in std::fs::read_dir(de).unwrap() {
        let entrada = entrada.unwrap();
        let destino = para.join(entrada.file_name());
        if entrada.path().is_dir() {
            copiar(&entrada.path(), &destino);
        } else {
            std::fs::copy(entrada.path(), destino).unwrap();
        }
    }
}

/// A pasta da 0.15.1 como pasta de configuração, depois do `setup` desta
/// versão (a primeira abertura e a migração dos pacotes para o cache). O
/// quintal de arquivos do PERFIS vai para `mod-data-legado/`, que é onde as
/// duas versões o põem para o servidor de sempre (`raizes_dos_mods(_, None)`).
fn pasta_da_0_15_1_com_o_perfis() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    rusqlite::Connection::open(dir.path().join("seele.db"))
        .unwrap()
        .execute_batch(&std::fs::read_to_string(format!("{PASTA}/seele.sql")).unwrap())
        .unwrap();
    for pasta in ["mod-packages", "mod-data-legado"] {
        copiar(&Path::new(PASTA).join(pasta), &dir.path().join(pasta));
    }
    let config = dir.path().to_string_lossy().into_owned();
    crate::versoes::primeira_abertura(&config);
    crate::mods::levar_pacotes_antigos_para_o_cache(dir.path());
    (dir, config)
}

fn ler(banco: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
}

fn esquema(banco: &Path) -> i64 {
    ler(banco)
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |l| l.get(0),
        )
        .unwrap()
}

/// A linha do Perfis em `mods`: versão e ligado.
fn linha_do_perfis(banco: &Path) -> (String, i64) {
    ler(banco)
        .query_row(
            "SELECT version, enabled FROM mods WHERE id = ?1",
            [PERFIS],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .unwrap()
}

/// O `mod_data` do Perfis, linha por linha, como o disco o tem.
fn quintal_do_perfis(banco: &Path) -> Vec<(String, Vec<u8>)> {
    let conexao = ler(banco);
    let mut consulta = conexao
        .prepare("SELECT key, CAST(value AS BLOB) FROM mod_data WHERE mod_id = ?1 ORDER BY key")
        .unwrap();
    consulta
        .query_map([PERFIS], |l| Ok((l.get(0)?, l.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Os bytes do banco e dos arquivos ao lado dele: o que «nada mudou» quer dizer.
fn bytes_do_banco(banco: &Path) -> Vec<(String, Option<String>)> {
    use sha2::Digest as _;
    ["", "-wal", "-shm", "-journal"]
        .into_iter()
        .map(|sufixo| {
            let caminho = format!("{}{sufixo}", banco.display());
            let resumo = std::fs::read(&caminho).ok().map(|bytes| {
                sha2::Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            });
            (sufixo.to_owned(), resumo)
        })
        .collect()
}

fn arquivos_em(pasta: &Path) -> Vec<PathBuf> {
    fn achar(pasta: &Path, achadas: &mut Vec<PathBuf>) {
        for entrada in std::fs::read_dir(pasta).into_iter().flatten().flatten() {
            if entrada.path().is_dir() {
                achar(&entrada.path(), achadas);
            } else {
                achadas.push(entrada.path());
            }
        }
    }
    let mut achadas = Vec::new();
    achar(pasta, &mut achadas);
    achadas.sort();
    achadas
}

fn copias_no_disco(config: &str) -> Vec<PathBuf> {
    arquivos_em(&Path::new(config).join(crate::antes_de_hospedar::PASTA_DAS_COPIAS))
}

/// A pasta de arquivos do PERFIS (o volume e os fragmentos), pelo SHA de cada
/// arquivo: «copiar, nunca mover» é ela igual antes e depois.
fn pasta_do_perfis(config: &str) -> Vec<(PathBuf, [u8; 32])> {
    arquivos_em(&crate::servidores::raizes_dos_mods(config, None).dados_de(PERFIS))
        .into_iter()
        .map(|caminho| {
            let bytes = std::fs::read(&caminho).unwrap();
            (caminho, seele_proto::attachment::hash(&bytes))
        })
        .collect()
}

fn escolher(config: &str, saida: SaidaDoModAntigo) -> Result<(), FalhaAoResolverModAntigo> {
    crate::mods::resolver_mod_de_api_antiga(config, None, PERFIS, &saida, false)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Hospedar pelo corpo do comando, como o teste do item 8 (porta zero, modo
/// local: sem a porta 8383 e sem a escada).
async fn hospedar(session: &Session, config: &str) -> Result<super::Anfitriao, FalhaAoHospedar> {
    hospedar_em(
        session,
        config,
        None,
        super::PedidoDeHospedar::default(),
        0,
        true,
    )
    .await
}

/// A recusa pela API 5: o que a pessoa lê quando nada foi escolhido.
fn recusa_pela_api_5(resultado: Result<super::Anfitriao, FalhaAoHospedar>) -> bool {
    matches!(
        resultado,
        Err(FalhaAoHospedar::ModsDeApiAntiga { ref mods })
            if mods == &vec![(PERFIS.to_owned(), Some(5))]
    )
}

#[test]
fn o_perfis_da_api_5_e_incorporado_sem_o_4_0_0_com_copia_cancelar_e_os_perfis_trazidos() {
    let (_dir, config) = pasta_da_0_15_1_com_o_perfis();
    let banco = crate::servidores::banco_de_hospedar(&config, None);
    let bytes_antes = bytes_do_banco(&banco);
    let linha_antes = linha_do_perfis(&banco);
    let quintal_antes = quintal_do_perfis(&banco);
    let pasta_antes = pasta_do_perfis(&config);
    assert_eq!(esquema(&banco), 13, "a pasta não é a da 0.15.1");
    assert_eq!(
        linha_antes,
        ("3.3.1".to_owned(), 1),
        "o Perfis 3.3.1 não está ligado na pasta"
    );
    assert_eq!(
        quintal_antes
            .iter()
            .map(|(chave, _)| chave.as_str())
            .collect::<Vec<_>>(),
        ["profiles"],
        "a pasta não tem os perfis que o 3.3.1 gravou"
    );
    assert_eq!(
        pasta_antes.len(),
        4,
        "a pasta do 3.3.1 não tem a foto do volume e a capa em fragmentos: {pasta_antes:?}"
    );
    let session = Session::default();
    let rt = runtime();

    // 1. O ANTES DE HOSPEDAR lê o PERFIS como incorporado (o que a tela diz),
    // e hospedar sem escolha recusa pela API 5. Nada muda.
    assert_eq!(
        crate::mods::incorporados_ligados(&banco),
        [(PERFIS.to_owned(), "0.19".to_owned())],
        "o ANTES DE HOSPEDAR não sabe que o PERFIS é do produto"
    );
    assert!(
        recusa_pela_api_5(rt.block_on(hospedar(&session, &config))),
        "hospedar sem escolha não recusou o Perfis da API 5"
    );
    assert_eq!(
        bytes_do_banco(&banco),
        bytes_antes,
        "a recusa mexeu no banco"
    );
    assert!(
        copias_no_disco(&config).is_empty(),
        "a recusa fez uma cópia"
    );

    // 2. O 4.0.0 não é oferecido: nem com ele no cache, onde o ATUALIZAR de
    // antes o aceitava. DESLIGAR também não é a saída dele.
    let manifesto: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{PERFIS_4}/mod.json")).unwrap())
            .unwrap();
    assert_eq!(
        (manifesto["id"].as_str(), manifesto["version"].as_str()),
        (Some(PERFIS), Some("4.0.0")),
        "a fixture do 4.0.0 deixou de ser o Perfis 4.0.0"
    );
    let hash_4 = crate::mods::instalar_de(Path::new(&config), Path::new(PERFIS_4))
        .unwrap()
        .hash;
    for saida in [
        SaidaDoModAntigo::Atualizar { hash: hash_4 },
        SaidaDoModAntigo::Desligar,
    ] {
        assert_eq!(
            escolher(&config, saida.clone()),
            Err(FalhaAoResolverModAntigo::IncorporadoAoProduto),
            "{saida:?} foi aceito para o PERFIS, que agora é parte do produto"
        );
    }
    assert!(
        crate::escolhas_de_mods::pendentes(Path::new(&config), None).is_empty(),
        "uma saída recusada ficou estagiada"
    );

    // 3. Seguir sem ele: só estagia.
    assert_eq!(escolher(&config, SaidaDoModAntigo::Incorporado), Ok(()));
    assert_eq!(
        bytes_do_banco(&banco),
        bytes_antes,
        "escolher seguir sem o PERFIS mexeu no banco"
    );
    assert_eq!(esquema(&banco), 13, "escolher migrou o banco");
    assert!(
        copias_no_disco(&config).is_empty(),
        "uma cópia apareceu antes do CTA"
    );

    // 4. Cancelar (o DESFAZER): a escolha some, o banco segue igual, e o
    // hospedar volta a recusar.
    desfazer_escolha_de_mod_em(&config, None, PERFIS).expect("o DESFAZER não desfez");
    assert!(
        crate::escolhas_de_mods::pendentes(Path::new(&config), None).is_empty(),
        "depois de cancelar, a escolha continua pendente"
    );
    assert_eq!(
        bytes_do_banco(&banco),
        bytes_antes,
        "cancelar mexeu no banco"
    );
    assert!(
        recusa_pela_api_5(rt.block_on(hospedar(&session, &config))),
        "depois de cancelar, hospedar não voltou a recusar o Perfis da API 5"
    );
    assert_eq!(
        bytes_do_banco(&banco),
        bytes_antes,
        "a segunda recusa mexeu no banco"
    );
    assert_eq!(esquema(&banco), 13, "cancelar migrou o banco");
    assert!(
        copias_no_disco(&config).is_empty(),
        "cancelar deixou uma cópia"
    );

    // 5. Escolhido de novo, o CTA: a cópia do estado de antes, a conversão e a
    // subida, que traz os perfis.
    assert_eq!(escolher(&config, SaidaDoModAntigo::Incorporado), Ok(()));
    let anfitriao = rt
        .block_on(hospedar(&session, &config))
        .unwrap_or_else(|falha| panic!("o CTA sem o PERFIS não subiu: {falha:?}"));
    let copias = copias_no_disco(&config);
    assert_eq!(
        copias.len(),
        1,
        "a cópia de segurança não é uma só: {copias:?}"
    );
    let copia = copias.first().unwrap();
    assert_eq!(
        esquema(copia),
        13,
        "a cópia não é do estado de antes da conversão"
    );
    assert_eq!(
        linha_do_perfis(copia),
        linha_antes,
        "a cópia não tem o Perfis 3.3.1 ligado"
    );
    assert_eq!(
        quintal_do_perfis(copia),
        quintal_antes,
        "a cópia não tem os perfis de antes"
    );

    // O servidor de pé: os perfis do 3.3.1 nas tabelas do produto.
    let servidor = session
        .hospedagens
        .lock()
        .unwrap()
        .do_guardado()
        .expect("o guardado de pé")
        .servidor();
    let (dona, convidada, relatorio) = rt.block_on(async {
        let banco = servidor.persistence.lock().await;
        (
            seele_server::persistence::perfis::ler(&banco, PersonId(1)).unwrap(),
            seele_server::persistence::perfis::ler(&banco, PersonId(2)).unwrap(),
            seele_server::persistence::perfis::relatorio(&banco).unwrap(),
        )
    });
    let dona = dona.expect("o perfil de quem hospeda não chegou às tabelas do produto");
    assert_eq!(
        (dona.nome_exibido.as_str(), dona.bio.as_str(), dona.revisao),
        ("Dona da Casa", "hospedo desde a 0.15", 3),
        "o texto do 3.3.1 não veio como estava"
    );
    assert!(
        dona.foto.is_some() && dona.capa.is_some(),
        "a foto (volume) ou a capa (fragmentos) do 3.3.1 não veio: {dona:?}"
    );
    assert_eq!(
        convidada.map(|c| c.nome_exibido),
        Some("Convidada".to_owned()),
        "o perfil da convidada não chegou"
    );
    let relatorio = relatorio.expect("quem hospeda não tem o relatório da migração");
    assert_eq!(
        (
            relatorio.trazidos,
            relatorio.incompleta,
            relatorio.ocorrencias.len()
        ),
        (2, false, 0),
        "o relatório não diz os dois perfis trazidos: {relatorio:?}"
    );
    drop(anfitriao);
    let guardado = session.hospedagens.lock().unwrap().guardado.take();
    rt.block_on(async {
        if let Some(guardado) = guardado {
            guardado.hospedagem.encerrar().await;
        }
    });

    assert_ne!(esquema(&banco), 13, "o CTA não converteu o banco");
    assert_eq!(
        linha_do_perfis(&banco),
        ("3.3.1".to_owned(), 0),
        "o PERFIS não saiu do conjunto (ou foi trocado)"
    );
    assert_eq!(
        quintal_do_perfis(&banco),
        quintal_antes,
        "a migração mexeu no mod_data do Perfis"
    );
    assert_eq!(
        pasta_do_perfis(&config),
        pasta_antes,
        "a migração moveu ou apagou arquivo da pasta do Perfis"
    );
    assert!(
        crate::escolhas_de_mods::pendentes(Path::new(&config), None).is_empty(),
        "a escolha ficou pendente depois de aplicada"
    );
}
