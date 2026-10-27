//! **A travessia de um MOD da API 5 para a API 6, pelo fluxo estagiado do P1**
//! (pedido de 25/10; `revisao-astra-travessia-api5-6.md`, «validação nova
//! mínima»).
//!
//! O caso: quem hospedou na 0.15 com o `seele/perfis` 3.3.1 (API 5) ligado abre
//! a 0.17. A API 5 não roda neste servidor (a linha 2 aceita `[7, 6]`) e não
//! passa a rodar: o HOSPEDAR recusa, e a saída é ATUALIZAR para o 4.0.0 (API 6)
//! no ANTES DE HOSPEDAR. Aqui se prova, pelos comandos de verdade
//! (`resolver_mod_de_api_antiga`, `desfazer_escolha_de_mod_em`, `hospedar_em`):
//!
//! 1. hospedar sem escolha recusa com `("seele/perfis", Some(5))`, e nada muda;
//! 2. escolher ATUALIZAR **não toca no banco** (bytes, esquema 13, linha do
//!    3.3.1, quintal) e não cria cópia;
//! 3. **cancelar** (o DESFAZER) esquece a escolha, o banco segue igual, e o
//!    hospedar volta a recusar pela API 5;
//! 4. escolhido de novo, o CTA (`hospedar_em`, porta zero, modo local) faz a
//!    cópia de segurança do estado de antes (esquema 13, 3.3.1, o quintal), e só
//!    então converte e aplica o 4.0.0;
//! 5. o `mod_data` do Perfis chega intacto, byte a byte, e a metade de servidor
//!    do 4.0.0 o lê pelo caminho de pedido do servidor de pé.
//!
//! # O que NÃO é prova aqui, dito
//!
//! - **Os pacotes não são os publicados.** O 3.3.1 e o 4.0.0 deste teste são
//!   de mentira (`testes/pasta-da-0.15.1-perfis/`), com o id, a versão e a API
//!   dos oficiais, mas não os bytes nem o hash deles: os pacotes oficiais não
//!   estão neste repositório, e nada se baixa nem se republica num teste. A
//!   pasta (o banco, a conta, o MOD ligado e o quintal) foi escrita **pelo
//!   código da v0.15.1**, que é a linha que roda a API 5.
//! - A tela não é vista: os comandos são chamados por baixo da janela.
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

use super::{desfazer_escolha_de_mod_em, hospedar_em, FalhaAoHospedar, Session};

/// A pasta de quem hospedou na 0.15.1 com o Perfis 3.3.1 (API 5).
const PASTA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testes/pasta-da-0.15.1-perfis/pasta"
);

/// O hash do Perfis 3.3.1 que a 0.15.1 gravou em `mods`.
const HASH_DO_3_3_1: &str = "d3f49d8addc6ee4ef438620af18e3981dc1ef5899b8b5b157564e8ead4902b2d";

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
/// versão (a primeira abertura e a migração dos pacotes para o cache).
fn pasta_da_0_15_1_com_o_perfis() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    rusqlite::Connection::open(dir.path().join("seele.db"))
        .unwrap()
        .execute_batch(&std::fs::read_to_string(format!("{PASTA}/seele.sql")).unwrap())
        .unwrap();
    copiar(
        &Path::new(PASTA).join("mod-packages"),
        &dir.path().join("mod-packages"),
    );
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

/// A linha do Perfis em `mods`: versão, hash e ligado.
fn linha_do_perfis(banco: &Path) -> (String, String, i64) {
    ler(banco)
        .query_row(
            "SELECT version, hash, enabled FROM mods WHERE id = ?1",
            [PERFIS],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
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

fn copias_no_disco(config: &str) -> Vec<PathBuf> {
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
    achar(
        &Path::new(config).join(crate::antes_de_hospedar::PASTA_DAS_COPIAS),
        &mut achadas,
    );
    achadas
}

/// O Perfis 4.0.0 de mentira, ao lado do 3.3.1 na pasta de fixtures.
const PERFIS_4: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testes/pasta-da-0.15.1-perfis/perfis-4.0.0"
);

/// O Perfis 4.0.0 de mentira, **na API 6** (a dos oficiais 4.0.0), instalado
/// no cache por hash. A metade de servidor lê o perfil que o 3.3.1 gravou.
///
/// O manifesto mora num arquivo, como o do 3.3.1, e não num literal aqui: a
/// API 6 é um fato histórico do 4.0.0 publicado, e não acompanha a
/// `MOD_API_VERSION` deste build — o guarda `nenhum_manifesto_de_teste_crava_a_versao_da_api`
/// só olha os `.rs`. A API 6 é conferida aqui, para a travessia continuar
/// sendo de 5 para 6.
fn perfis_4_no_cache(config: &str) -> String {
    let manifesto: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{PERFIS_4}/mod.json")).unwrap())
            .unwrap();
    assert_eq!(
        (
            manifesto["id"].as_str(),
            manifesto["version"].as_str(),
            manifesto["api"].as_u64()
        ),
        (Some(PERFIS), Some("4.0.0"), Some(6)),
        "a fixture do 4.0.0 deixou de ser o Perfis 4.0.0 da API 6"
    );
    crate::mods::instalar_de(Path::new(config), Path::new(PERFIS_4))
        .unwrap()
        .hash
}

fn atualizar_para(config: &str, hash: &str) {
    assert_eq!(
        crate::mods::resolver_mod_de_api_antiga(
            config,
            None,
            PERFIS,
            &crate::mods::SaidaDoModAntigo::Atualizar {
                hash: hash.to_owned()
            },
            false,
        ),
        Ok(()),
        "ATUALIZAR PARA 4.0.0 não foi aceito no ANTES DE HOSPEDAR"
    );
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
    hospedar_em(session, config, None, None, None, 0, true).await
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
fn o_perfis_da_api_5_atravessa_para_o_4_0_0_da_api_6_com_copia_cancelar_e_o_quintal() {
    let (_dir, config) = pasta_da_0_15_1_com_o_perfis();
    let banco = crate::servidores::banco_de_hospedar(&config, None);
    let bytes_antes = bytes_do_banco(&banco);
    let linha_antes = linha_do_perfis(&banco);
    let quintal_antes = quintal_do_perfis(&banco);
    assert_eq!(esquema(&banco), 13, "a pasta não é a da 0.15.1");
    assert_eq!(
        linha_antes,
        ("3.3.1".to_owned(), HASH_DO_3_3_1.to_owned(), 1),
        "o Perfis 3.3.1 não está ligado na pasta"
    );
    assert_eq!(
        quintal_antes.len(),
        1,
        "a pasta não tem o perfil que o 3.3.1 gravou: {quintal_antes:?}"
    );
    let session = Session::default();
    let rt = runtime();

    // 1. Hospedar sem escolha: recusa pela API 5, e nada muda.
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

    // 2. ATUALIZAR PARA 4.0.0: só estagia.
    let hash_4 = perfis_4_no_cache(&config);
    assert_ne!(hash_4, HASH_DO_3_3_1, "o 4.0.0 tem de ser outro pacote");
    atualizar_para(&config, &hash_4);
    assert_eq!(
        bytes_do_banco(&banco),
        bytes_antes,
        "escolher ATUALIZAR mexeu no banco"
    );
    assert_eq!(esquema(&banco), 13, "escolher ATUALIZAR migrou o banco");
    assert!(
        copias_no_disco(&config).is_empty(),
        "uma cópia apareceu antes do CTA"
    );

    // 3. Cancelar (o DESFAZER): a escolha some, o banco segue igual, e a API 5
    // volta a ser recusada. A API 5 não passa a rodar por ter sido cancelada.
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
    assert!(
        copias_no_disco(&config).is_empty(),
        "cancelar deixou uma cópia"
    );

    // 4. Escolhido de novo, o CTA: a cópia do estado de antes, e só então a
    // conversão com o 4.0.0 aplicado.
    atualizar_para(&config, &hash_4);
    let anfitriao = rt
        .block_on(hospedar(&session, &config))
        .unwrap_or_else(|falha| panic!("o CTA com o 4.0.0 escolhido não subiu: {falha:?}"));
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
        "a cópia não tem o Perfis 3.3.1"
    );
    assert_eq!(
        quintal_do_perfis(copia),
        quintal_antes,
        "a cópia não tem o perfil de antes"
    );

    // 5. O servidor de pé: o 4.0.0 ligado, o quintal intacto, e lido por ele.
    let servidor = session
        .hospedagens
        .lock()
        .unwrap()
        .do_guardado()
        .expect("o guardado de pé")
        .servidor();
    let resposta = rt.block_on(async {
        let dono = {
            let banco = servidor.persistence.lock().await;
            seele_server::permissions::Permissions::new(&banco)
                .register_or_find(&[0x5e; 32], "dono")
                .unwrap()
        };
        seele_server::mods::pedidos::executar(
            &servidor,
            dono.id,
            0_u32.into(),
            PERFIS,
            r#"{"op":"view"}"#,
        )
        .await
    });
    let lido: serde_json::Value = serde_json::from_str(&resposta).unwrap();
    assert_eq!(
        lido["versao"], "4.0.0",
        "o pedido não chegou ao 4.0.0: {resposta}"
    );
    let (_, gravado) = quintal_antes.first().unwrap();
    assert_eq!(
        lido["perfil"].as_str().map(str::as_bytes),
        Some(gravado.as_slice()),
        "o 4.0.0 não leu o perfil que o 3.3.1 gravou na 0.15.1: {resposta}"
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
        ("4.0.0".to_owned(), hash_4, 1),
        "o CTA não aplicou o 4.0.0"
    );
    assert_eq!(
        quintal_do_perfis(&banco),
        quintal_antes,
        "a travessia mexeu no mod_data do Perfis"
    );
    assert!(
        crate::escolhas_de_mods::pendentes(Path::new(&config), None).is_empty(),
        "a escolha ficou pendente depois de aplicada"
    );
}
