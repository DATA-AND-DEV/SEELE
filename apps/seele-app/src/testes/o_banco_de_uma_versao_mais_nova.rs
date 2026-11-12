use super::{classificar, FalhaAoHospedar};

/// **Hospedar um banco de uma versão mais nova diz isso pelo nome** (spec da
/// 0.16, §8). O erro é o de verdade: o `Persistence::open` sobre um banco
/// gravado num esquema acima do deste build, embrulhado como a subida o
/// embrulha. Sem o nome, a tela dizia NÃO CONSEGUI SUBIR O SERVIDOR AQUI, e
/// a pessoa não teria como saber que atualizar resolve.
#[test]
fn o_banco_do_futuro_chega_a_tela_pelo_nome() {
    use seele_server::persistence::{esquema_conhecido, Location, Persistence};
    let pasta = std::env::temp_dir().join(format!("seele-futuro-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).expect("pasta");
    let arquivo = pasta.join("seele.db");
    let conexao = rusqlite::Connection::open(&arquivo).expect("criar");
    conexao
            .execute_batch(
                "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL) STRICT;",
            )
            .expect("a tabela");
    conexao
        .execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?1, 0)",
            [esquema_conhecido() + 1],
        )
        .expect("a versão");
    drop(conexao);

    let Err(erro) = Persistence::open(&Location::File(arquivo)) else {
        panic!("o servidor abriu um banco de uma versão mais nova");
    };
    let embrulhado = erro.context("subindo o servidor desta janela");
    assert!(
        matches!(
            classificar(&embrulhado),
            FalhaAoHospedar::BancoDoFuturo { gravado, conhecido }
                if gravado == esquema_conhecido() + 1 && conhecido == esquema_conhecido()
        ),
        "o banco de uma versão mais nova chegou à tela como outra falha: {:?}",
        classificar(&embrulhado)
    );
    let _ = std::fs::remove_dir_all(&pasta);
}
