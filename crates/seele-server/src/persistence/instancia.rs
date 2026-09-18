//! A identidade deste servidor, que atravessa reinício, endereço e chave.
//!
//! Plano de isolamento de 18/09, P1 «identidade do servidor e identidade da
//! sessão estão incompletas».
//!
//! # Por que endereço e impressão digital não bastam
//!
//! O endereço muda: a mesma casa responde por um IP de LAN, por um par NAT cuja
//! porta troca a cada sessão, e por um nome que alguém registrou ontem.
//!
//! E a impressão digital **pode repetir**. `uma_chave_por_maquina` faz bancos
//! diferentes herdarem a mesma chave TLS do legado — de propósito, para que
//! quem já hospedava não veja o alarme de chave trocada. Dois servidores desta
//! máquina podem apresentar a mesma.
//!
//! Juntos, os dois não distinguem dois servidores lógicos. E é sobre essa
//! distinção que o aceite de MODs, o estado e os arquivos precisam se apoiar.
//!
//! # O que ela é, e o que ela não é
//!
//! É do **banco**. Copiar o arquivo copia a identidade, e isso é o que se quer:
//! um backup restaurado é o mesmo servidor, não outro.
//!
//! **Não** é segredo e não prova nada sozinha: quem fala com um servidor recebe
//! esta identidade no aperto de mão, e quem quiser mentir pode repeti-la. O que
//! prova continua sendo a chave TLS, conferida pelo pin do ADR 0017. Esta
//! responde outra pergunta — «é o mesmo de antes?» — para quem já sabe que a
//! chave confere.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use anyhow::Result;
use rusqlite::params;

use super::Persistence;

/// A identidade deste servidor, criando-a no primeiro arranque.
///
/// Trinta e dois caracteres hexadecimais — dezesseis bytes do mesmo gerador que
/// a `admissao` já usa. Não é UUID formatado porque não há nada que ganhe com
/// os hífens: ela viaja em JSON e é comparada inteira.
///
/// # Errors
///
/// Quando o banco não responde.
pub fn identidade(persistence: &Persistence) -> Result<String> {
    if let Some(ja) = ler(persistence)? {
        return Ok(ja);
    }
    let bruto: [u8; 16] = rand::random();
    let mut nova = String::with_capacity(32);
    for byte in bruto {
        use std::fmt::Write as _;
        let _ = write!(nova, "{byte:02x}");
    }
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();

    // `OR IGNORE` e uma releitura depois: entre o `ler` acima e este `INSERT`
    // nada mais escreve hoje — uma conexão atrás de um mutex —, mas escrever
    // isto como se fosse exclusivo seria escrever uma suposição sobre o futuro.
    // Se alguém chegou antes, a identidade dele é a que vale, e não a minha.
    persistence.connection().execute(
        "INSERT OR IGNORE INTO instancia (id, identidade, nascida_em) VALUES (1, ?1, ?2)",
        params![nova, agora as i64],
    )?;
    ler(persistence)?.ok_or_else(|| anyhow::anyhow!("a identidade não ficou gravada"))
}

/// A identidade, se já existe.
fn ler(persistence: &Persistence) -> Result<Option<String>> {
    let mut consulta = persistence
        .connection()
        .prepare("SELECT identidade FROM instancia WHERE id = 1")?;
    let mut linhas = consulta.query([])?;
    Ok(match linhas.next()? {
        Some(linha) => Some(linha.get(0)?),
        None => None,
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::persistence::Location;

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).expect("banco de memória")
    }

    #[test]
    fn a_identidade_nasce_uma_vez_e_nao_muda() {
        let p = banco();
        let primeira = identidade(&p).expect("primeira");
        let segunda = identidade(&p).expect("segunda");
        assert_eq!(primeira, segunda, "o servidor trocou de identidade sozinho");
        assert_eq!(primeira.len(), 32, "{primeira}");
        assert!(
            primeira.chars().all(|c| c.is_ascii_hexdigit()),
            "{primeira}"
        );
    }

    /// **Dois bancos são dois servidores**, ainda que apresentem a mesma chave
    /// TLS — que é exatamente o que `uma_chave_por_maquina` faz com o legado.
    #[test]
    fn dois_bancos_nascem_com_identidades_diferentes() {
        let a = identidade(&banco()).expect("a");
        let b = identidade(&banco()).expect("b");
        assert_ne!(
            a, b,
            "dois servidores desta máquina não têm como ser distinguidos"
        );
    }

    /// E ela é do banco: quem abre o mesmo arquivo de novo é o mesmo servidor.
    /// Um backup restaurado não é outro servidor, e quem entrava nele continua
    /// tendo entrado nele.
    #[test]
    fn reabrir_o_mesmo_banco_devolve_a_mesma_identidade() {
        let dir = std::env::temp_dir().join(format!(
            "seele-instancia-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temporário");
        let arquivo = dir.join("seele.db");

        let antes = {
            let p = Persistence::open(&Location::File(arquivo.clone())).expect("abrir");
            identidade(&p).expect("primeira")
        };
        let depois = {
            let p = Persistence::open(&Location::File(arquivo)).expect("reabrir");
            identidade(&p).expect("segunda")
        };
        assert_eq!(antes, depois, "reiniciar trocou a identidade do servidor");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
