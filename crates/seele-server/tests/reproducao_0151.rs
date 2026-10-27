//! diag-0151 (H): o caso 0.15.1 → 0.16 → 0.17, do lado do servidor.
//!
//! O relato: um servidor criado na 0.15.1 não hospedava depois de atualizar o app, num laço de
//! «atualize os MODs». O diagnóstico (docs/evidencias/diagnostico-0151/RELATORIO.md) mediu na
//! 0.16.0 que o banco e a rede não eram o bloqueio: era o MOD (o `seele/estilo` 3.1.0, API 4), e a
//! decisão sobre ele vivia no app. A 0.17 exige o mesmo: **os dados da 0.15.1 migram**, e a rede
//! incompatível e o MOD de API antiga são decisões separadas, que não impedem a migração.
//!
//! O que estes testes seguram, com a pasta que a 0.15.1 gravou de verdade
//! (`apps/seele-app/testes/pasta-da-0.15.1`, código da 54f542f94):
//! 1. o banco do esquema 13 abre e migra para o esquema deste build, sem perder conta, papel,
//!    MOD ligado nem quintal;
//! 2. o MOD de API antiga continua ligado e legível (a API dele se lê depois de migrar): migrar
//!    não decide por quem hospeda, nem desliga em silêncio;
//! 3. com a pasta COMPLETA (canais, mensagens, segunda conta, identidade TLS e uma batida
//!    pendente, gerada por docs/evidencias/diagnostico-0151/gerar_complemento.rs com o código da
//!    0.15.1), a hospedagem sobe com a MESMA impressão TLS e a batida que esperava aparece na fila.
//!    A pasta completa leva uma chave privada de teste e não vai para o repositório: o teste a lê de
//!    `$SEELE_DIAG_FIXTURE` (a pasta de saída do gerador), e diz em voz alta quando não a acha.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::{Path, PathBuf};

use seele_server::persistence::{esquema_conhecido, Location, Persistence};

const PASTA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/seele-app/testes/pasta-da-0.15.1/pasta"
);
const HASH_DO_ESTILO_3_1_0: &str =
    "ca0b79565bbb41a79403ce69b500c62f908a080efbb9bcbd4e3486a302d21682";

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

fn contar(banco: &Path, tabela: &str) -> i64 {
    rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |l| l.get(0))
        .unwrap()
}

fn esquema(banco: &Path) -> i64 {
    rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |l| l.get(0),
        )
        .unwrap()
}

/// A pasta da 0.15.1 numa pasta de passagem: o banco do `.dump` e o cache dos pacotes.
fn pasta_da_0_15_1() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    rusqlite::Connection::open(dir.path().join("seele.db"))
        .unwrap()
        .execute_batch(&std::fs::read_to_string(format!("{PASTA}/seele.sql")).unwrap())
        .unwrap();
    copiar(
        &Path::new(PASTA).join("mod-packages"),
        &dir.path().join("mod-packages"),
    );
    dir
}

const TABELAS: [&str; 5] = ["people", "person_roles", "roles", "mods", "mod_data"];

#[test]
fn os_dados_da_0_15_1_migram_para_o_esquema_desta_versao() {
    let dir = pasta_da_0_15_1();
    let banco = dir.path().join("seele.db");
    assert_eq!(
        esquema(&banco),
        13,
        "a pasta da 0.15.1 não está no esquema 13"
    );
    let antes: Vec<i64> = TABELAS.iter().map(|t| contar(&banco, t)).collect();

    let aberto = Persistence::open(&Location::File(banco.clone()));
    assert!(
        aberto.is_ok(),
        "o banco da 0.15.1 não abriu nesta versão — os dados não migram: {:?}",
        aberto.err()
    );
    drop(aberto);

    assert_eq!(
        esquema(&banco),
        esquema_conhecido(),
        "o banco não chegou ao esquema deste build"
    );
    let depois: Vec<i64> = TABELAS.iter().map(|t| contar(&banco, t)).collect();
    // Nenhuma tabela perde linha. Ganhar pode: a migração dos cargos da 0.17 acrescenta papel
    // (medido em 24/10: roles 4 → 5), e isso não é perda dos dados da 0.15.1.
    for ((tabela, a), d) in TABELAS.iter().zip(&antes).zip(&depois) {
        assert!(d >= a, "a migração perdeu linhas em {tabela}: {a} → {d}");
    }
    println!("linhas por tabela {TABELAS:?}: 0.15.1 {antes:?} → esta versão {depois:?}");
    let papeis: Vec<String> = {
        let conexao = rusqlite::Connection::open(&banco).unwrap();
        let mut consulta = conexao.prepare("SELECT name FROM roles").unwrap();
        consulta
            .query_map([], |l| l.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    for papel in ["Commander", "Operator", "Person", "Observer"] {
        assert!(
            papeis.iter().any(|p| p == papel),
            "o papel {papel} da 0.15.1 sumiu: {papeis:?}"
        );
    }
    let (dono, papel_do_dono): (Vec<u8>, i64) = rusqlite::Connection::open(&banco)
        .unwrap()
        .query_row(
            "SELECT p.public_key, pr.role_id FROM people p JOIN person_roles pr ON pr.person_id = p.id WHERE p.id = 1",
            [],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (dono, papel_do_dono),
        (vec![0x5e; 32], 1),
        "a conta de quem hospeda perdeu a chave ou a Comandante"
    );
    let tema: String = rusqlite::Connection::open(&banco)
        .unwrap()
        .query_row(
            "SELECT CAST(value AS TEXT) FROM mod_data WHERE mod_id = 'seele/estilo' AND key = 'theme'",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert!(
        tema.contains("#2f80ed"),
        "o quintal do MOD não sobreviveu: {tema}"
    );
}

/// Migrar não decide sobre o MOD: o 3.1.0 continua ligado, com o mesmo hash, e a API dele (4) se
/// lê pela mesma regra do anúncio. É ela que a tela «antes de hospedar» mostra como decisão
/// separada (atualizar, desligar ou hospedar pela linha antiga), e não como bloqueio dos dados.
#[test]
fn o_mod_de_api_antiga_fica_ligado_e_legivel_depois_de_migrar() {
    let dir = pasta_da_0_15_1();
    let banco = dir.path().join("seele.db");
    drop(Persistence::open(&Location::File(banco.clone())).unwrap());
    let (versao, hash, ligado): (String, String, i64) = rusqlite::Connection::open(&banco)
        .unwrap()
        .query_row(
            "SELECT version, hash, enabled FROM mods WHERE id = 'seele/estilo'",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (versao.as_str(), hash.as_str(), ligado),
        ("3.1.0", HASH_DO_ESTILO_3_1_0, 1)
    );
    let raizes = seele_server::RaizesDosMods {
        pacotes: dir.path().to_path_buf(),
        dados: dir.path().join("mod-data"),
    };
    assert_eq!(seele_server::mods::api_do_pacote(&raizes, &hash), Ok(4));
}

fn fixture_completa() -> Option<PathBuf> {
    let pasta = PathBuf::from(std::env::var_os("SEELE_DIAG_FIXTURE")?);
    pasta.join("seele.db").exists().then_some(pasta)
}

/// A pasta completa da 0.15.1 hospedada por ESTA versão: mesma impressão TLS, dados inteiros, e a
/// batida que esperava na 0.15.1 continua na fila de quem hospeda.
#[tokio::test(flavor = "multi_thread")]
async fn a_pasta_completa_hospeda_com_a_mesma_impressao_e_a_batida_antiga_na_fila() {
    let Some(origem) = fixture_completa() else {
        println!(
            "NÃO MEDIDO: $SEELE_DIAG_FIXTURE não aponta a pasta completa. Gere-a com \
             docs/evidencias/diagnostico-0151/gerar_complemento.rs (código da 0.15.1)."
        );
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let banco = dir.path().join("seele.db");
    std::fs::copy(origem.join("seele.db"), &banco).unwrap();
    copiar(
        &Path::new(PASTA).join("mod-packages"),
        &dir.path().join("mod-packages"),
    );

    let cert: Vec<u8> = rusqlite::Connection::open(&banco)
        .unwrap()
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = 'tls_cert'",
            [],
            |l| l.get(0),
        )
        .expect("a pasta completa não tem a identidade TLS da 0.15.1");
    let impressao_da_0_15 = seele_proto::transport::certificate_fingerprint(&cert);
    let tabelas = ["people", "channels", "voice_rooms", "messages", "mod_data"];
    let antes: Vec<i64> = tabelas.iter().map(|t| contar(&banco, t)).collect();

    let dono = "0c".repeat(32);
    let hospedagem = seele_server::hospedagem::Hospedagem::iniciar(
        0,
        Location::File(banco.clone()),
        "Casa",
        Some(seele_server::RaizesDosMods {
            pacotes: dir.path().to_path_buf(),
            dados: dir.path().join("mod-data"),
        }),
        move |persistence| {
            seele_server::portaria::semear_ligada(persistence)?;
            seele_server::portaria::admitir_o_dono(persistence, &dono)
        },
    )
    .await
    .expect("esta versão não hospeda a pasta completa da 0.15.1");

    assert_eq!(
        hospedagem.impressao_digital(),
        impressao_da_0_15,
        "a impressão do servidor mudou ao subir da 0.15.1: todo convite e todo pino antigos acusariam troca de chave"
    );
    let (pendentes, apelidos) = {
        let persistence = hospedagem.persistence();
        let persistence = persistence.lock().await;
        let pedidos = seele_server::portaria::pedidos(&persistence).unwrap();
        (
            seele_server::portaria::pendentes(&persistence).unwrap(),
            pedidos.into_iter().map(|p| p.apelido).collect::<Vec<_>>(),
        )
    };
    hospedagem.encerrar().await;

    assert_eq!(
        pendentes, 1,
        "a batida que esperava na 0.15.1 sumiu da fila: {apelidos:?}"
    );
    assert!(
        apelidos.iter().any(|a| a == "batedora"),
        "a batida da 0.15.1 não está na fila: {apelidos:?}"
    );
    let depois: Vec<i64> = tabelas.iter().map(|t| contar(&banco, t)).collect();
    assert_eq!(depois, antes, "a hospedagem perdeu dados em {tabelas:?}");
}
