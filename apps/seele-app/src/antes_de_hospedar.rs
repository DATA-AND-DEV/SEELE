//! **ANTES DE HOSPEDAR: o que hospedar vai fazer com os dados, e a cópia de
//! segurança que vem antes de qualquer conversão** (E2 da 0.17, o caso do
//! servidor antigo com MODs; revisão do Codex).
//!
//! # O defeito
//!
//! Hospedar por esta versão um servidor criado numa versão anterior **converte
//! o banco** (o `Persistence::open` migra), e convertia calado: a tela não dizia
//! que ia converter, e a cópia que existia era a da *linha legada* (a primeira
//! abertura guarda os dados da versão anterior no depósito do launcher), não a
//! do estado **atual** daquele banco. Um banco do Servidor 1.0 com só uma cópia
//! da 0.15 era convertido sem cópia nenhuma do estado de antes.
//!
//! # A regra, no backend
//!
//! - [`conferir`] é **só leitura**: lê o esquema gravado pelo SQLite em modo
//!   leitura, lista as cópias e diz o que hospedar faria. Uma falha de leitura
//!   vira «não verificado» ([`Esquema::NaoESqlite`], [`Esquema::Ilegivel`]),
//!   nunca «pronto»; só um arquivo **ausente** (NotFound) é «banco novo».
//! - [`garantir_a_copia`] é o portão da conversão, e `hospedar` passa por ele
//!   antes de subir o servidor: **toda conversão cria e verifica uma cópia nova**
//!   do estado atual, sem confiar em cópia anterior (revisão do Codex: nome,
//!   data e esquema não provam conteúdo; um banco WAL com escrita depois da
//!   cópia mantém o mtime). A cópia é feita e comparada sob uma trava exclusiva
//!   do banco: `integrity_check` e as mesmas tabelas com o mesmo número de
//!   linhas que a fonte. Falha fechado em qualquer etapa, dizendo qual e por quê
//!   ([`FalhaNaCopia`]), e o banco original não é tocado.
//! - Cada cópia é identificada pela linha, pelo banco de origem, pelo esquema e
//!   pela data. Só a que acabou de ser feita e comprovada diz
//!   [`Copia::do_estado_atual`]; as listadas do disco nunca.
//!
//! # O que fica fora
//!
//! - A cópia da linha legada continua sendo da primeira abertura
//!   (`versoes::primeira_abertura`, do compat): ela aparece aqui, listada, de
//!   outra linha.
//! - **A janela entre a trava e a conversão.** A trava sai logo antes de o
//!   `hospedar` abrir o banco para migrar. Dentro do app, o único escritor
//!   daquele banco é o servidor que o `hospedar` vai subir, e ele ainda não
//!   subiu (`JaHospedando` antes). Um processo de fora escrevendo naquele
//!   instante não é impedido; não medido.

use std::path::{Path, PathBuf};

/// A pasta das cópias de segurança, dentro da pasta de dados do SEELE.
pub(crate) const PASTA_DAS_COPIAS: &str = "copias-de-seguranca";

/// O esquema gravado num banco, lido sem migrar.
///
/// A mesma forma que o `seele_server::persistence::EsquemaGravado` do E (decisão
/// do coordenador). Lida aqui pelo SQLite em modo leitura, até o E publicar a
/// dele; aí esta função passa a chamá-la, sem mudar o que a tela recebe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum Esquema {
    /// Não há arquivo (NotFound): hospedar cria um banco novo. O único caso
    /// «novo».
    Ausente,
    /// O esquema gravado.
    Gravado(i64),
    /// Há um arquivo, e ele não é um banco SQLite.
    NaoESqlite,
    /// O arquivo não pôde ser lido, ou nem se sabe se existe (permissão,
    /// disco, trava).
    Ilegivel(String),
}

/// Lê o esquema gravado em `banco`, **só para leitura**: nada é criado, nenhum
/// pragma é escrito e nenhuma migração roda.
pub(crate) fn esquema_de(banco: &Path) -> Esquema {
    // **Só o NotFound é «ausente»** (revisão do Codex): um `exists()` dizia
    // «ausente» também para um «permission denied», e um banco que não se lê
    // virava banco novo. Qualquer outro erro é «não verificado».
    match std::fs::metadata(banco) {
        Ok(_) => {}
        Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => return Esquema::Ausente,
        Err(erro) => return Esquema::Ilegivel(erro.to_string()),
    }
    let aberto = rusqlite::Connection::open_with_flags(
        banco,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    );
    let conexao = match aberto {
        Ok(conexao) => conexao,
        Err(erro) => return classificar(&erro),
    };
    // Um arquivo que não é SQLite abre, e falha na primeira leitura.
    if let Err(erro) = conexao.query_row("SELECT count(*) FROM sqlite_master", [], |linha| {
        linha.get::<_, i64>(0)
    }) {
        return classificar(&erro);
    }
    let tem_a_tabela = conexao
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
            [],
            |linha| linha.get::<_, i64>(0),
        )
        .map(|n| n > 0);
    match tem_a_tabela {
        Ok(false) => Esquema::Gravado(0),
        Ok(true) => match conexao.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |linha| linha.get::<_, i64>(0),
        ) {
            Ok(esquema) => Esquema::Gravado(esquema),
            Err(erro) => classificar(&erro),
        },
        Err(erro) => classificar(&erro),
    }
}

/// Um erro do SQLite, como a tela o lê.
fn classificar(erro: &rusqlite::Error) -> Esquema {
    match erro.sqlite_error_code() {
        Some(rusqlite::ErrorCode::NotADatabase) => Esquema::NaoESqlite,
        _ => Esquema::Ilegivel(erro.to_string()),
    }
}

/// A linha de servidor que é dona de um esquema: a regra da primeira abertura
/// (compat, `versoes`): até 13 é a 0.15, de 14 a 17 é a 1, e daí em diante é a
/// desta versão.
pub(crate) fn linha_do_esquema(esquema: i64) -> &'static str {
    match esquema {
        ..=13 => "0.15",
        14..=17 => "1",
        _ => "2",
    }
}

/// Uma cópia de segurança, como a tela a mostra.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Copia {
    /// De qual tipo: `seguranca` (a do estado de um banco desta pasta) ou
    /// `linha-legada` (a que a primeira abertura guardou para outra linha).
    pub(crate) tipo: &'static str,
    /// A linha dos dados copiados: `"0.15"`, `"1"` ou `"2"`.
    pub(crate) linha: String,
    /// O banco de origem, relativo à pasta de dados do SEELE.
    pub(crate) origem: String,
    /// O esquema dos dados copiados, quando se sabe.
    pub(crate) esquema: Option<i64>,
    /// Quando foi feita, em segundos desde 1970 (a tela formata).
    pub(crate) data: u64,
    /// Onde está, relativo à pasta de dados do SEELE.
    pub(crate) caminho: String,
    /// **É uma cópia COMPROVADA do estado atual do banco**: só a que acabou de
    /// ser feita e verificada contra a fonte, sob a trava do banco
    /// ([`preparar_copia`]). Uma cópia listada do disco é sempre `false`: nome,
    /// data e esquema não provam conteúdo (revisão do Codex: um banco WAL com
    /// escrita depois da cópia mantém o mesmo mtime, e uma cópia corrompida
    /// mantém o nome). A tela só diz «cópia do estado atual» com `true`.
    pub(crate) do_estado_atual: bool,
}

/// O que hospedar vai fazer, lido sem mexer em nada.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AntesDeHospedar {
    /// O servidor guardado, ou `None` para o banco de sempre.
    pub(crate) servidor: Option<String>,
    /// O banco, relativo à pasta de dados do SEELE.
    pub(crate) banco: String,
    /// O esquema gravado nele.
    pub(crate) esquema: Esquema,
    /// O esquema desta versão.
    pub(crate) esquema_desta_versao: i64,
    /// A linha dona dos dados, quando o esquema foi lido.
    pub(crate) linha_dos_dados: Option<&'static str>,
    /// O esquema foi lido: `Ausente` ou `Gravado`. Falso = «não verificado», e
    /// hospedar recusa até a pessoa resolver.
    pub(crate) verificado: bool,
    /// Por que não foi verificado, em texto para «Detalhes técnicos».
    pub(crate) motivo: Option<String>,
    /// As ações que a tela oferece quando não foi verificado.
    pub(crate) acoes: Vec<&'static str>,
    /// Hospedar por esta versão converte os dados.
    pub(crate) converte: bool,
    /// O banco é de uma versão mais nova que esta: hospedar recusa.
    pub(crate) futuro: bool,
    /// As cópias que existem, da mais nova para a mais velha. Nenhuma é dada
    /// como do estado atual: ver [`Copia::do_estado_atual`].
    pub(crate) copias: Vec<Copia>,
    /// Hospedar vai criar e verificar uma cópia nova do estado atual antes de
    /// converter: **toda conversão faz a sua**, sem confiar em cópia anterior.
    pub(crate) vai_copiar: bool,
}

/// A pasta das cópias de um servidor.
fn pasta_das_copias(config: &Path, servidor: Option<&str>) -> PathBuf {
    config
        .join(PASTA_DAS_COPIAS)
        .join(servidor.unwrap_or("padrao"))
}

/// O caminho de `caminho` relativo à pasta de dados, para a tela.
fn relativo(config: &Path, caminho: &Path) -> String {
    caminho
        .strip_prefix(config)
        .unwrap_or(caminho)
        .display()
        .to_string()
}

fn segundos(quando: std::time::SystemTime) -> u64 {
    quando
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |desde| desde.as_secs())
}

/// As cópias de segurança deste banco que estão no disco, lidas do nome do
/// arquivo (`esquema-<n>-<ms>.db`). **Listadas, e nunca dadas como do estado
/// atual**: o nome e a data não provam o conteúdo. Ver [`Copia::do_estado_atual`].
fn copias_de_seguranca(config: &Path, servidor: Option<&str>, banco: &Path) -> Vec<Copia> {
    let pasta = pasta_das_copias(config, servidor);
    let Ok(entradas) = std::fs::read_dir(&pasta) else {
        return Vec::new();
    };
    let mut copias: Vec<(u128, Copia)> = entradas
        .flatten()
        .filter_map(|entrada| {
            let nome = entrada.file_name().to_string_lossy().into_owned();
            let resto = nome.strip_prefix("esquema-")?.strip_suffix(".db")?;
            let (esquema, ms) = resto.split_once('-')?;
            let esquema: i64 = esquema.parse().ok()?;
            let ms: u128 = ms.parse().ok()?;
            Some((
                ms,
                Copia {
                    tipo: "seguranca",
                    linha: linha_do_esquema(esquema).to_owned(),
                    origem: relativo(config, banco),
                    esquema: Some(esquema),
                    data: u64::try_from(ms / 1000).unwrap_or(u64::MAX),
                    caminho: relativo(config, &entrada.path()),
                    do_estado_atual: false,
                },
            ))
        })
        .collect();
    copias.sort_by_key(|(ms, _)| std::cmp::Reverse(*ms));
    copias.into_iter().map(|(_, copia)| copia).collect()
}

/// As cópias que a primeira abertura guardou para as linhas legadas, no
/// depósito do launcher. Nunca do estado atual: são de outra linha.
fn copias_das_linhas_legadas(config: &Path) -> Vec<Copia> {
    let deposito = seele_lancador::Deposito::em(config.join("lancador"));
    ["1", "0.15"]
        .into_iter()
        .filter_map(|texto| {
            let linha = seele_lancador::Linha::nova(texto).ok()?;
            if !deposito.tem_dados_da_linha(&linha) {
                return None;
            }
            let pasta = deposito.pasta_de_dados_da_linha(&linha);
            let data = std::fs::metadata(&pasta)
                .and_then(|metadados| metadados.modified())
                .map_or(0, segundos);
            Some(Copia {
                tipo: "linha-legada",
                linha: texto.to_owned(),
                origem: "seele.db".to_owned(),
                esquema: None,
                data,
                caminho: relativo(config, &pasta),
                do_estado_atual: false,
            })
        })
        .collect()
}

/// **O que hospedar vai fazer, só lendo.** `banco` é o caminho do banco deste
/// servidor, sem criar nada (`servidores::caminho_do_banco`).
pub(crate) fn conferir(config: &Path, servidor: Option<&str>, banco: &Path) -> AntesDeHospedar {
    let esquema = esquema_de(banco);
    let desta_versao = seele_server::persistence::esquema_conhecido();
    let (verificado, motivo, acoes) = match &esquema {
        Esquema::Ausente | Esquema::Gravado(_) => (true, None, Vec::new()),
        Esquema::NaoESqlite => (
            false,
            Some("o arquivo do banco não é um banco SQLite".to_owned()),
            vec!["abrir-a-pasta"],
        ),
        Esquema::Ilegivel(motivo) => (
            false,
            Some(motivo.clone()),
            vec!["tentar-de-novo", "abrir-a-pasta"],
        ),
    };
    let gravado = match esquema {
        Esquema::Gravado(n) => Some(n),
        _ => None,
    };
    let converte = gravado.is_some_and(|n| n < desta_versao);
    let futuro = gravado.is_some_and(|n| n > desta_versao);
    let mut copias = copias_de_seguranca(config, servidor, banco);
    copias.extend(copias_das_linhas_legadas(config));
    AntesDeHospedar {
        servidor: servidor.map(str::to_owned),
        banco: relativo(config, banco),
        linha_dos_dados: gravado.map(linha_do_esquema),
        esquema,
        esquema_desta_versao: desta_versao,
        verificado,
        motivo,
        acoes,
        converte,
        futuro,
        copias,
        vai_copiar: converte,
    }
}

/// Em que etapa a cópia de segurança parou.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum Etapa {
    /// Ler o banco antes de copiar.
    Conferir,
    /// Escrever a cópia (inclusive tomar a trava do banco).
    Copiar,
    /// Conferir a cópia escrita contra a fonte.
    Verificar,
}

/// Por que a cópia não ficou pronta. **Nada foi convertido.**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct FalhaNaCopia {
    /// A etapa.
    pub(crate) etapa: Etapa,
    /// O motivo, para «Detalhes técnicos».
    pub(crate) motivo: String,
}

fn falha(etapa: Etapa, motivo: impl std::fmt::Display) -> FalhaNaCopia {
    FalhaNaCopia {
        etapa,
        motivo: motivo.to_string(),
    }
}

/// Quantas linhas cada tabela tem, por nome, em ordem: a régua do conteúdo
/// com que a cópia é comparada à fonte.
fn contagem_por_tabela(banco: &rusqlite::Connection) -> Result<Vec<(String, i64)>, String> {
    let mut consulta = banco
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|erro| erro.to_string())?;
    let nomes: Vec<String> = consulta
        .query_map([], |linha| linha.get(0))
        .map_err(|erro| erro.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|erro| erro.to_string())?;
    nomes
        .into_iter()
        .map(|nome| {
            let citado = nome.replace('"', "\"\"");
            banco
                .query_row(&format!("SELECT count(*) FROM \"{citado}\""), [], |linha| {
                    linha.get::<_, i64>(0)
                })
                .map(|n| (nome, n))
                .map_err(|erro| erro.to_string())
        })
        .collect()
}

/// **Cria e verifica uma cópia do estado atual do banco.** Ação própria
/// («preparar cópia») e o passo do portão em toda conversão.
///
/// 1. Toma a **trava** do banco: uma transação exclusiva numa conexão própria.
///    Nenhum outro processo escreve até a verificação; as leituras continuam.
///    Um banco ocupado por mais de dois segundos falha fechado.
/// 2. Conta as linhas de cada tabela da fonte, sob a trava.
/// 3. Escreve a cópia num nome provisório (`VACUUM INTO`, de uma conexão só de
///    leitura, que vê o que está no WAL).
/// 4. Verifica: abre a cópia só para leitura, `PRAGMA integrity_check` igual a
///    `ok`, o mesmo esquema e as mesmas tabelas com as mesmas contagens da fonte.
/// 5. Só então dá o nome final, e solta a trava sem escrever nada.
///
/// Uma cópia que não passa é apagada, e a falha diz a etapa.
pub(crate) fn preparar_copia(
    config: &Path,
    servidor: Option<&str>,
    banco: &Path,
) -> Result<Copia, FalhaNaCopia> {
    preparar_copia_com(config, servidor, banco, |_| {})
}

/// [`preparar_copia`] com o que acontece entre escrever e verificar, para o
/// teste estragar a cópia.
fn preparar_copia_com(
    config: &Path,
    servidor: Option<&str>,
    banco: &Path,
    depois_de_escrever: impl FnOnce(&Path),
) -> Result<Copia, FalhaNaCopia> {
    let Esquema::Gravado(esquema) = esquema_de(banco) else {
        return Err(falha(
            Etapa::Conferir,
            "o banco não foi lido: não há o que copiar com segurança",
        ));
    };
    // 1. A trava.
    let trava = rusqlite::Connection::open_with_flags(
        banco,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|erro| falha(Etapa::Copiar, erro))?;
    trava
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|erro| falha(Etapa::Copiar, erro))?;
    // `IMMEDIATE`, e não `EXCLUSIVE`: os dois barram outro escritor, mas o
    // `EXCLUSIVE` também barra leitores no modo de diário tradicional, e aí o
    // `VACUUM INTO` (um leitor) não leria. Medido: «database is locked».
    trava
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|erro| falha(Etapa::Copiar, format!("o banco está em uso: {erro}")))?;
    // 2. A régua da fonte.
    let contagem_da_fonte =
        contagem_por_tabela(&trava).map_err(|erro| falha(Etapa::Copiar, erro))?;

    let pasta = pasta_das_copias(config, servidor);
    std::fs::create_dir_all(&pasta).map_err(|erro| falha(Etapa::Copiar, erro))?;
    let agora_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |desde| desde.as_millis());
    let final_ = pasta.join(format!("esquema-{esquema}-{agora_ms}.db"));
    let provisoria = pasta.join(format!(".escrevendo-{esquema}-{agora_ms}.db"));
    let _ = std::fs::remove_file(&provisoria);

    // 3. A cópia.
    let escrever = || -> Result<(), String> {
        let origem = rusqlite::Connection::open_with_flags(
            banco,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|erro| erro.to_string())?;
        origem
            .execute("VACUUM INTO ?1", [provisoria.display().to_string()])
            .map_err(|erro| erro.to_string())?;
        Ok(())
    };
    if let Err(motivo) = escrever() {
        let _ = std::fs::remove_file(&provisoria);
        return Err(falha(Etapa::Copiar, motivo));
    }
    depois_de_escrever(&provisoria);

    // 4. A verificação, contra a fonte.
    let verificar = || -> Result<(), String> {
        let copia = rusqlite::Connection::open_with_flags(
            &provisoria,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|erro| erro.to_string())?;
        let integridade: String = copia
            .query_row("PRAGMA integrity_check", [], |linha| linha.get(0))
            .map_err(|erro| erro.to_string())?;
        if integridade != "ok" {
            return Err(format!("integrity_check: {integridade}"));
        }
        let da_copia = contagem_por_tabela(&copia)?;
        if da_copia != contagem_da_fonte {
            return Err(format!(
                "a cópia não tem o conteúdo da fonte: fonte {contagem_da_fonte:?}, cópia {da_copia:?}"
            ));
        }
        drop(copia);
        match esquema_de(&provisoria) {
            Esquema::Gravado(n) if n == esquema => Ok(()),
            outro => Err(format!(
                "a cópia não tem o esquema da origem ({esquema}): {outro:?}"
            )),
        }
    };
    if let Err(motivo) = verificar() {
        let _ = std::fs::remove_file(&provisoria);
        return Err(falha(Etapa::Verificar, motivo));
    }
    // 5. O nome final, e a trava sai sem escrever nada.
    std::fs::rename(&provisoria, &final_).map_err(|erro| {
        let _ = std::fs::remove_file(&provisoria);
        falha(Etapa::Copiar, erro)
    })?;
    let _ = trava.execute_batch("ROLLBACK");
    drop(trava);
    tracing::info!(
        banco = %banco.display(),
        copia = %final_.display(),
        esquema,
        "cópia de segurança do banco feita e conferida contra a fonte antes de converter"
    );
    let data = u64::try_from(agora_ms / 1000).unwrap_or(u64::MAX);
    Ok(Copia {
        tipo: "seguranca",
        linha: linha_do_esquema(esquema).to_owned(),
        origem: relativo(config, banco),
        esquema: Some(esquema),
        data,
        caminho: relativo(config, &final_),
        do_estado_atual: true,
    })
}

/// O que o portão da conversão decidiu.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum Portao {
    /// Hospedar não converte nada (banco novo, ou já desta versão).
    NadaAConverter,
    /// A cópia foi criada e verificada agora. **Toda conversão passa por
    /// aqui**: nenhuma cópia anterior a dispensa.
    CopiaFeita(Copia),
}

/// Por que o portão não abriu. **Nada foi convertido.**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum PortaoFechado {
    /// O banco não foi verificado: «não verificado» nunca vira «pronto».
    NaoVerificado {
        /// O motivo.
        motivo: String,
        /// O que a tela oferece.
        acoes: Vec<&'static str>,
    },
    /// A cópia não ficou pronta.
    Copia(FalhaNaCopia),
}

/// **O portão da conversão**: `hospedar` passa por ele antes de subir. Se
/// hospedar vai converter, cria e verifica uma cópia nova do estado atual; sem
/// ela, não deixa converter.
pub(crate) fn garantir_a_copia(
    config: &Path,
    servidor: Option<&str>,
    banco: &Path,
) -> Result<Portao, PortaoFechado> {
    let antes = conferir(config, servidor, banco);
    if !antes.verificado {
        return Err(PortaoFechado::NaoVerificado {
            motivo: antes.motivo.unwrap_or_default(),
            acoes: antes.acoes,
        });
    }
    if !antes.converte {
        return Ok(Portao::NadaAConverter);
    }
    preparar_copia(config, servidor, banco)
        .map(Portao::CopiaFeita)
        .map_err(PortaoFechado::Copia)
}

#[cfg(test)]
mod testes {
    //! O portão, com bancos SQLite de verdade numa pasta temporária, e a
    //! conversão de verdade (`Persistence::open`, que migra) depois dele.

    use super::{
        conferir, garantir_a_copia, preparar_copia_com, Esquema, Etapa, Portao, PortaoFechado,
        PASTA_DAS_COPIAS,
    };
    use seele_server::persistence::{Location, Persistence};
    use std::path::Path;

    /// Um banco com o esquema `n` gravado, como uma versão antiga deixaria.
    fn banco_no_esquema(caminho: &Path, n: i64) {
        let banco = rusqlite::Connection::open(caminho).expect("o banco de teste");
        banco
            .execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 CREATE TABLE coisa (x INTEGER);
                 INSERT INTO coisa VALUES (42);",
            )
            .expect("as tabelas");
        banco
            .execute("INSERT INTO schema_version VALUES (?1)", [n])
            .expect("o esquema");
    }

    /// Um banco do Servidor 1.0: o esquema 17, que é da linha 1. Basta ao
    /// portão, que lê o número e o conteúdo.
    fn banco_do_servidor_1(caminho: &Path) {
        banco_no_esquema(caminho, 17);
    }

    fn bytes(caminho: &Path) -> Vec<u8> {
        std::fs::read(caminho).expect("os bytes do banco")
    }

    fn linhas_de_coisa(caminho: &Path) -> i64 {
        rusqlite::Connection::open(caminho)
            .expect("a cópia abre")
            .query_row("SELECT count(*) FROM coisa", [], |linha| linha.get(0))
            .expect("a contagem")
    }

    /// **Os quatro casos do esquema**: só o ausente é «novo»; o que não é
    /// SQLite e o ilegível são «não verificado», com ação, e nunca prontos.
    #[test]
    fn so_o_ausente_e_novo_e_o_ilegivel_nunca_e_pronto() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();

        let ausente = config.join("ausente.db");
        let novo = conferir(config, None, &ausente);
        assert_eq!(novo.esquema, Esquema::Ausente);
        assert!(novo.verificado && !novo.converte && !novo.vai_copiar);

        let gravado = config.join("gravado.db");
        banco_do_servidor_1(&gravado);
        let velho = conferir(config, None, &gravado);
        assert_eq!(velho.esquema, Esquema::Gravado(17));
        assert!(velho.verificado && velho.converte && velho.vai_copiar);
        assert_eq!(velho.linha_dos_dados, Some("1"));

        let texto = config.join("texto.db");
        std::fs::write(
            &texto,
            b"isto nao e um banco sqlite, e sim um texto qualquer......",
        )
        .expect("o texto");
        let nao_sqlite = conferir(config, None, &texto);
        assert_eq!(nao_sqlite.esquema, Esquema::NaoESqlite);
        assert!(
            !nao_sqlite.verificado && !nao_sqlite.converte && !nao_sqlite.acoes.is_empty(),
            "um arquivo que não é SQLite saiu como pronto: {nao_sqlite:?}"
        );

        // Uma pasta no lugar do arquivo: existe, e não se lê como banco.
        let pasta_no_lugar = config.join("pasta.db");
        std::fs::create_dir_all(&pasta_no_lugar).expect("a pasta no lugar");
        let ilegivel = conferir(config, None, &pasta_no_lugar);
        assert!(
            matches!(ilegivel.esquema, Esquema::Ilegivel(_) | Esquema::NaoESqlite),
            "um banco que não se lê saiu como {:?}",
            ilegivel.esquema
        );
        assert!(
            !ilegivel.verificado,
            "um banco que não se lê saiu como verificado"
        );
        assert!(matches!(
            garantir_a_copia(config, None, &pasta_no_lugar),
            Err(PortaoFechado::NaoVerificado { .. })
        ));
    }

    /// **Uma pasta sem permissão de leitura, de verdade** (chmod 000, com o
    /// usuário normal): o banco dentro dela não é «ausente». Era o `exists()`,
    /// que dizia «não existe» para um «permission denied», e o banco virava
    /// banco novo.
    #[cfg(unix)]
    #[test]
    fn um_banco_numa_pasta_sem_permissao_e_ilegivel_e_nunca_banco_novo() {
        use std::os::unix::fs::PermissionsExt;

        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let trancada = config.join("trancada");
        std::fs::create_dir_all(&trancada).expect("a pasta trancada");
        let banco = trancada.join("seele.db");
        banco_do_servidor_1(&banco);
        std::fs::set_permissions(&trancada, std::fs::Permissions::from_mode(0o000))
            .expect("trancar");
        let lido = conferir(config, None, &banco);
        let portao = garantir_a_copia(config, None, &banco);
        // Destranca antes de qualquer asserção: o tempdir precisa apagar.
        std::fs::set_permissions(&trancada, std::fs::Permissions::from_mode(0o755))
            .expect("destrancar");
        assert!(
            matches!(lido.esquema, Esquema::Ilegivel(_)),
            "um banco numa pasta sem permissão saiu como {:?}: o «permission denied» virou \
             «ausente»",
            lido.esquema
        );
        assert!(!lido.verificado && !lido.converte);
        assert!(matches!(portao, Err(PortaoFechado::NaoVerificado { .. })));
    }

    /// **O banco 1.0 com só uma cópia da 0.15 pede uma cópia nova antes de
    /// converter**, e o portão a cria e verifica contra a fonte. E a próxima
    /// conversão faz OUTRA: nenhuma cópia anterior a dispensa.
    #[test]
    fn toda_conversao_cria_e_verifica_uma_copia_nova() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        banco_do_servidor_1(&banco);
        // A cópia que a 0.16 deixou para a 0.15, no depósito do launcher.
        let deposito = seele_lancador::Deposito::em(config.join("lancador"));
        let linha = seele_lancador::Linha::nova("0.15").expect("a linha");
        let da_015 = deposito.pasta_de_dados_da_linha(&linha);
        std::fs::create_dir_all(&da_015).expect("a pasta da 0.15");
        std::fs::write(da_015.join("PRONTA"), b"").expect("o marcador");

        let antes = conferir(config, None, &banco);
        assert!(antes.converte && antes.vai_copiar);
        assert!(
            antes.copias.iter().all(|copia| !copia.do_estado_atual),
            "a cópia da linha legada saiu como do estado atual: {antes:?}"
        );
        let Ok(Portao::CopiaFeita(primeira)) = garantir_a_copia(config, None, &banco) else {
            panic!("o portão não criou a cópia do estado atual");
        };
        assert!(primeira.do_estado_atual && primeira.linha == "1" && primeira.esquema == Some(17));
        assert!(config.join(&primeira.caminho).is_file());
        let relida = conferir(config, None, &banco);
        assert!(
            relida.vai_copiar && relida.copias.iter().all(|copia| !copia.do_estado_atual),
            "uma cópia listada do disco saiu como do estado atual: {relida:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        let Ok(Portao::CopiaFeita(segunda)) = garantir_a_copia(config, None, &banco) else {
            panic!("a segunda conversão confiou na cópia anterior");
        };
        assert_ne!(segunda.caminho, primeira.caminho);
    }

    /// **WAL vivo com escrita depois da cópia** (a reprodução do Codex): o
    /// banco em WAL sem checkpoint automático; a cópia; uma escrita nova com a
    /// conexão mantida aberta. Nenhuma cópia listada diz «atual», e a conversão
    /// cria uma cópia nova que tem a linha escrita depois.
    #[test]
    fn um_wal_vivo_com_escrita_depois_da_copia_nao_vale_como_atual() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        banco_do_servidor_1(&banco);
        let viva = rusqlite::Connection::open(&banco).expect("a conexão viva");
        viva.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
            .expect("o WAL");
        let Ok(Portao::CopiaFeita(velha)) = garantir_a_copia(config, None, &banco) else {
            panic!("a primeira cópia não saiu");
        };
        assert_eq!(linhas_de_coisa(&config.join(&velha.caminho)), 1);
        viva.execute("INSERT INTO coisa VALUES (43)", [])
            .expect("a escrita depois da cópia");

        let relida = conferir(config, None, &banco);
        assert!(
            relida.copias.iter().all(|copia| !copia.do_estado_atual),
            "a cópia de antes da escrita saiu como do estado atual: {relida:?}"
        );
        let Ok(Portao::CopiaFeita(nova)) = garantir_a_copia(config, None, &banco) else {
            panic!("a conversão confiou na cópia de antes da escrita");
        };
        assert_eq!(
            linhas_de_coisa(&config.join(&nova.caminho)),
            2,
            "a cópia nova não tem a escrita que estava no WAL"
        );
        drop(viva);
    }

    /// **Uma cópia corrompida depois de criada** não vale como atual, e não
    /// dispensa a cópia nova.
    #[test]
    fn uma_copia_corrompida_depois_de_criada_nao_vale() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        banco_do_servidor_1(&banco);
        let Ok(Portao::CopiaFeita(feita)) = garantir_a_copia(config, None, &banco) else {
            panic!("a cópia não saiu");
        };
        std::fs::write(config.join(&feita.caminho), b"estragada depois").expect("estragar");
        let relida = conferir(config, None, &banco);
        assert!(relida.copias.iter().all(|copia| !copia.do_estado_atual));
        assert!(matches!(
            garantir_a_copia(config, None, &banco),
            Ok(Portao::CopiaFeita(_))
        ));
    }

    /// **Um arquivo de nome plausível que é cópia de OUTRO banco**, deixado na
    /// pasta das cópias, com um carimbo do futuro: não vale como atual.
    #[test]
    fn a_copia_de_outro_banco_com_nome_plausivel_nao_vale() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        banco_do_servidor_1(&banco);
        let pasta_das_copias = config.join(PASTA_DAS_COPIAS).join("padrao");
        std::fs::create_dir_all(&pasta_das_copias).expect("a pasta das cópias");
        let outro = pasta_das_copias.join("esquema-17-99999999999999.db");
        banco_no_esquema(&outro, 17);
        rusqlite::Connection::open(&outro)
            .expect("o outro")
            .execute("INSERT INTO coisa VALUES (7)", [])
            .expect("o conteúdo de outro banco");
        let relida = conferir(config, None, &banco);
        assert!(
            !relida.copias.is_empty() && relida.copias.iter().all(|copia| !copia.do_estado_atual),
            "a cópia de outro banco, de nome plausível, saiu como do estado atual: {relida:?}"
        );
        assert!(relida.vai_copiar);
    }

    /// **A cópia que falha bloqueia a conversão**, e o banco fica intacto.
    #[test]
    fn a_copia_que_falha_nao_deixa_converter_e_o_banco_fica_intacto() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        banco_do_servidor_1(&banco);
        let antes = bytes(&banco);
        // A pasta das cópias é um arquivo: escrever a cópia falha.
        std::fs::write(config.join(PASTA_DAS_COPIAS), b"").expect("o arquivo no lugar");
        match garantir_a_copia(config, None, &banco) {
            Err(PortaoFechado::Copia(falha)) => assert_eq!(falha.etapa, Etapa::Copiar),
            outro => panic!("a cópia que não pôde ser escrita deixou o portão {outro:?}"),
        }
        assert_eq!(
            bytes(&banco),
            antes,
            "a cópia que falhou mexeu no banco original"
        );
        assert_eq!(super::esquema_de(&banco), Esquema::Gravado(17));
    }

    /// **A verificação que falha bloqueia a conversão**, e a cópia estragada
    /// some.
    #[test]
    fn a_verificacao_que_falha_nao_deixa_converter() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        banco_do_servidor_1(&banco);
        let antes = bytes(&banco);
        let resultado = preparar_copia_com(config, None, &banco, |escrita| {
            // A cópia é estragada entre escrever e verificar.
            std::fs::write(escrita, b"estragada").expect("estragar");
        });
        match resultado {
            Err(falha) => assert_eq!(falha.etapa, Etapa::Verificar),
            Ok(copia) => panic!("a cópia estragada passou na verificação: {copia:?}"),
        }
        assert_eq!(bytes(&banco), antes);
        assert!(
            conferir(config, None, &banco).copias.is_empty(),
            "a cópia estragada ficou"
        );
    }

    /// **A cópia com conteúdo diferente da fonte não passa**: entre escrever e
    /// verificar, a cópia perde uma linha (a fonte, sob a trava, não muda).
    #[test]
    fn a_copia_com_menos_linhas_que_a_fonte_nao_passa() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        banco_do_servidor_1(&banco);
        let resultado = preparar_copia_com(config, None, &banco, |escrita| {
            rusqlite::Connection::open(escrita)
                .expect("a cópia")
                .execute("DELETE FROM coisa", [])
                .expect("tirar a linha");
        });
        match resultado {
            Err(falha) => assert_eq!(falha.etapa, Etapa::Verificar, "{falha:?}"),
            Ok(copia) => panic!("a cópia sem o conteúdo da fonte passou: {copia:?}"),
        }
    }

    /// A conversão de verdade depois do portão: num banco que a própria
    /// `Persistence` criou (o esquema desta versão), não há o que converter.
    #[test]
    fn um_banco_desta_versao_nao_converte_nem_copia() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let config = pasta.path();
        let banco = config.join("seele.db");
        drop(Persistence::open(&Location::File(banco.clone())).expect("o banco"));
        let Esquema::Gravado(atual) = super::esquema_de(&banco) else {
            panic!("o banco criado não tem esquema");
        };
        assert_eq!(atual, seele_server::persistence::esquema_conhecido());
        assert!(matches!(
            garantir_a_copia(config, None, &banco),
            Ok(Portao::NadaAConverter)
        ));
    }
}
