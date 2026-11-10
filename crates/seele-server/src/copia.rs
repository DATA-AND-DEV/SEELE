//! Guardar e restaurar tudo o que um servidor tem em disco.
//!
//! Um servidor que alguém hospeda em casa tem um disco só, e até a 1.0 não
//! havia como tirar dele uma cópia que servisse: copiar o `seele.db` com o
//! servidor de pé leva um arquivo sem o que ainda está no `-wal`, e o que faz
//! a cópia inteira (os anexos, as imagens de perfil) mora em pastas ao lado.
//!
//! # O que é uma cópia
//!
//! Uma **pasta**, com:
//!
//! - `seele.db`: o banco inteiro num arquivo só, tirado com `VACUUM INTO`. É
//!   um instantâneo consistente mesmo com o servidor de pé, e sai sem `-wal`.
//! - `anexos/` e `perfis/`: as pastas que moram ao lado do banco, quando
//!   existem.
//!
//! A pasta nasce `0700` e o banco dentro dela `0600`: ela guarda a chave
//! privada do certificado e os convites, como o original.
//!
//! # O que restaurar faz, e o que nunca faz
//!
//! Confere a cópia **antes** de tocar em qualquer coisa: o banco tem de ser um
//! SQLite íntegro, de um esquema que este build sabe abrir. Depois **põe de
//! lado** o que está no lugar (banco, `-wal`, `-shm`, `anexos/`, `perfis/`),
//! com um sufixo datado, e só então traz a cópia. Nada é apagado: quem
//! restaurou a cópia errada tem o estado anterior ao lado, inteiro.
//!
//! Restaurar é com o servidor **parado**, como os outros subcomandos de
//! operação: um `seeled` de pé continuaria escrevendo no arquivo que saiu de
//! baixo dele.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};

use crate::persistence::{esquema_conhecido, esquema_gravado_em, EsquemaGravado};

/// O nome do banco dentro de uma cópia, seja qual for o nome do original.
pub const BANCO_DA_COPIA: &str = "seele.db";

/// As pastas que moram ao lado do banco e vão junto.
pub const PASTAS_AO_LADO: [&str; 2] = ["anexos", "perfis"];

/// O que uma cópia, ou uma restauração, levou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relatorio {
    /// O esquema do banco copiado.
    pub esquema: i64,
    /// O tamanho do banco copiado, em bytes.
    pub bytes_do_banco: u64,
    /// Quantos arquivos foram em cada pasta ao lado; ausente quando a pasta
    /// não existia na origem.
    pub arquivos: Vec<(&'static str, u64)>,
    /// Só na restauração: para onde foi o que estava no lugar.
    pub posto_de_lado: Vec<PathBuf>,
}

fn pasta_do_banco(banco: &Path) -> &Path {
    banco
        .parent()
        .filter(|pasta| !pasta.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(unix)]
fn so_do_dono(caminho: &Path, modo: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(modo))
}

/// No Windows vale a ACL do perfil de quem roda.
#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "a mesma assinatura da versão Unix, que falha de verdade"
)]
fn so_do_dono(_caminho: &Path, _modo: u32) -> std::io::Result<()> {
    Ok(())
}

/// Copia uma pasta inteira, e diz quantos arquivos foram.
fn copiar_pasta(de: &Path, para: &Path) -> Result<u64> {
    std::fs::create_dir_all(para).with_context(|| format!("criar {}", para.display()))?;
    so_do_dono(para, 0o700).with_context(|| format!("fechar {}", para.display()))?;
    let mut arquivos = 0;
    for entrada in std::fs::read_dir(de).with_context(|| format!("ler {}", de.display()))? {
        let entrada = entrada?;
        let destino = para.join(entrada.file_name());
        let tipo = entrada.file_type()?;
        if tipo.is_dir() {
            arquivos += copiar_pasta(&entrada.path(), &destino)?;
        } else if tipo.is_file() {
            std::fs::copy(entrada.path(), &destino)
                .with_context(|| format!("copiar {}", entrada.path().display()))?;
            arquivos += 1;
        }
        // Um link simbólico não vai: a cópia não segue caminho para fora da
        // pasta que foi pedida.
    }
    Ok(arquivos)
}

/// Confere que `banco` é um SQLite íntegro de um esquema que este build abre,
/// e devolve o esquema. Só lê.
fn conferir(banco: &Path) -> Result<i64> {
    let esquema = match esquema_gravado_em(banco) {
        EsquemaGravado::Gravado(esquema) => esquema,
        EsquemaGravado::Ausente => bail!("{} não existe", banco.display()),
        EsquemaGravado::NaoESqlite => bail!("{} não é um banco do SEELE", banco.display()),
        EsquemaGravado::Ilegivel(motivo) => {
            bail!("{} não pôde ser lido: {motivo}", banco.display())
        }
    };
    if esquema == 0 {
        bail!(
            "{} é um SQLite sem o esquema do SEELE: não é a cópia de um servidor",
            banco.display()
        );
    }
    let conhecido = esquema_conhecido();
    if esquema > conhecido {
        bail!(
            "{} está no esquema {esquema}, e este seeled só conhece até o {conhecido}: \
             ele foi gravado por uma versão mais nova. Atualize o seeled antes de restaurar",
            banco.display()
        );
    }
    let connection = Connection::open_with_flags(
        banco,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("abrir {}", banco.display()))?;
    let veredito: String = connection
        .query_row("PRAGMA integrity_check", [], |linha| linha.get(0))
        .with_context(|| format!("conferir a integridade de {}", banco.display()))?;
    if veredito != "ok" {
        bail!(
            "{} está corrompido (o SQLite respondeu «{veredito}»)",
            banco.display()
        );
    }
    Ok(esquema)
}

/// Guarda o banco e as pastas ao lado dele em `destino`, uma pasta nova.
///
/// Funciona com o servidor de pé: o `VACUUM INTO` lê um instantâneo. Os anexos
/// são copiados depois do banco; um que expire nesse intervalo fica na cópia
/// como o servidor já o trata, «o arquivo expirou», com o texto inteiro.
///
/// # Errors
///
/// Falha, sem deixar meia cópia para trás, se `destino` já existe e não está
/// vazio, se o banco não existe ou não pode ser lido, ou se o disco recusa.
pub fn guardar(banco: &Path, destino: &Path) -> Result<Relatorio> {
    if !banco.is_file() {
        bail!(
            "não há banco em {}: nada a guardar. O caminho vem de $SEELE_DB, ou é \
             `seele.db` na pasta de onde o seeled roda",
            banco.display()
        );
    }
    if destino.exists()
        && std::fs::read_dir(destino)
            .with_context(|| format!("ler {}", destino.display()))?
            .next()
            .is_some()
    {
        bail!(
            "{} já existe e não está vazia: uma cópia nova pede uma pasta nova, \
             para nunca misturar duas",
            destino.display()
        );
    }
    std::fs::create_dir_all(destino).with_context(|| format!("criar {}", destino.display()))?;
    match guardar_em(banco, destino) {
        Ok(relatorio) => Ok(relatorio),
        Err(erro) => {
            // Meia cópia é pior que nenhuma: ela tem cara de cópia.
            let _ = std::fs::remove_dir_all(destino);
            Err(erro)
        }
    }
}

fn guardar_em(banco: &Path, destino: &Path) -> Result<Relatorio> {
    so_do_dono(destino, 0o700).with_context(|| format!("fechar {}", destino.display()))?;
    let copia = destino.join(BANCO_DA_COPIA);
    {
        // Leitura e escrita, e não só leitura: um banco em WAL só se abre para
        // ler se o `-shm` puder ser criado. Nenhuma escrita é feita nele.
        let connection = Connection::open_with_flags(
            banco,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("abrir {}", banco.display()))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(10))
            .context("configurar a espera pelo banco")?;
        let alvo = copia
            .to_str()
            .with_context(|| format!("{} não é um caminho em UTF-8", copia.display()))?;
        connection
            .execute("VACUUM INTO ?1", [alvo])
            .with_context(|| format!("copiar o banco para {}", copia.display()))?;
    }
    so_do_dono(&copia, 0o600).with_context(|| format!("fechar {}", copia.display()))?;
    let esquema = conferir(&copia).context("a cópia que acabou de sair não confere")?;
    let bytes_do_banco = std::fs::metadata(&copia)?.len();

    let mut arquivos = Vec::new();
    for nome in PASTAS_AO_LADO {
        let origem = pasta_do_banco(banco).join(nome);
        if origem.is_dir() {
            arquivos.push((nome, copiar_pasta(&origem, &destino.join(nome))?));
        }
    }
    Ok(Relatorio {
        esquema,
        bytes_do_banco,
        arquivos,
        posto_de_lado: Vec::new(),
    })
}

/// Traz a cópia de `origem` para o lugar de `banco`, pondo de lado o que
/// estava lá. `agora` entra no nome do que é posto de lado.
///
/// # Errors
///
/// Falha **antes de tocar em qualquer coisa** se a cópia não confere: não
/// existe, não é um banco do SEELE, está corrompida, ou é de um esquema mais
/// novo que este build. Falha depois disso só se o disco recusar.
pub fn restaurar(origem: &Path, banco: &Path, agora: u64) -> Result<Relatorio> {
    let copia = origem.join(BANCO_DA_COPIA);
    if !copia.is_file() {
        bail!(
            "{} não é uma cópia do SEELE: falta o `{BANCO_DA_COPIA}` dentro dela",
            origem.display()
        );
    }
    let esquema = conferir(&copia)?;
    let pasta = pasta_do_banco(banco);
    std::fs::create_dir_all(pasta).with_context(|| format!("criar {}", pasta.display()))?;

    // Pôr de lado, e nunca apagar.
    let mut posto_de_lado = Vec::new();
    let mut por_de_lado = |caminho: PathBuf| -> Result<()> {
        if !caminho.exists() {
            return Ok(());
        }
        let mut nome = caminho.as_os_str().to_owned();
        nome.push(format!(".antes-de-restaurar-{agora}"));
        let destino = PathBuf::from(nome);
        std::fs::rename(&caminho, &destino)
            .with_context(|| format!("pôr {} de lado", caminho.display()))?;
        posto_de_lado.push(destino);
        Ok(())
    };
    for sufixo in ["", "-wal", "-shm"] {
        let mut nome = banco.as_os_str().to_owned();
        nome.push(sufixo);
        por_de_lado(PathBuf::from(nome))?;
    }
    for nome in PASTAS_AO_LADO {
        por_de_lado(pasta.join(nome))?;
    }

    std::fs::copy(&copia, banco).with_context(|| format!("trazer {}", banco.display()))?;
    so_do_dono(banco, 0o600).with_context(|| format!("fechar {}", banco.display()))?;
    let bytes_do_banco = std::fs::metadata(banco)?.len();
    let mut arquivos = Vec::new();
    for nome in PASTAS_AO_LADO {
        let de = origem.join(nome);
        if de.is_dir() {
            arquivos.push((nome, copiar_pasta(&de, &pasta.join(nome))?));
        }
    }
    Ok(Relatorio {
        esquema,
        bytes_do_banco,
        arquivos,
        posto_de_lado,
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::persistence::{Location, Persistence};

    /// Um jeito de estragar uma cópia, e a frase que a recusa tem de dizer.
    type Estrago<'a> = (&'a str, &'a dyn Fn(&Path), &'a str);

    /// Um servidor em disco com um convite, um anexo e uma imagem de perfil.
    fn servidor(pasta: &Path) -> (PathBuf, String) {
        let banco = pasta.join("seele.db");
        let mut aberto = Persistence::open(&Location::File(banco.clone())).expect("abrir");
        let convite = crate::admissao::criar_convite(&mut aberto, "para a Lia").expect("convite");
        drop(aberto);
        std::fs::create_dir_all(pasta.join("anexos/ab")).expect("anexos");
        std::fs::write(pasta.join("anexos/ab/foto"), b"uma foto").expect("anexo");
        std::fs::create_dir_all(pasta.join("perfis")).expect("perfis");
        std::fs::write(pasta.join("perfis/retrato"), b"um retrato").expect("perfil");
        (banco, convite)
    }

    /// Quantos convites o banco em `caminho` tem.
    fn convites(caminho: &Path) -> i64 {
        Connection::open(caminho)
            .expect("abrir")
            .query_row("SELECT count(*) FROM convites", [], |linha| linha.get(0))
            .expect("contar")
    }

    #[test]
    fn a_copia_leva_o_banco_inteiro_e_as_pastas_ao_lado_com_o_servidor_de_pe() {
        let mundo = tempfile::tempdir().expect("tempdir");
        let (banco, _) = servidor(mundo.path());
        // O servidor de pé: o banco aberto em WAL, com uma escrita que ainda
        // não foi para o arquivo principal. Copiar o arquivo perderia ela.
        let mut de_pe = Persistence::open(&Location::File(banco.clone())).expect("de pé");
        crate::admissao::criar_convite(&mut de_pe, "para o Rafa").expect("convite");

        let destino = mundo.path().join("copia");
        let relatorio = guardar(&banco, &destino).expect("guardar");

        assert_eq!(relatorio.esquema, esquema_conhecido());
        assert_eq!(relatorio.arquivos, vec![("anexos", 1), ("perfis", 1)]);
        assert_eq!(convites(&destino.join(BANCO_DA_COPIA)), 2, "o do WAL ficou");
        assert_eq!(
            std::fs::read(destino.join("anexos/ab/foto")).expect("anexo"),
            b"uma foto"
        );
        assert!(
            !destino.join("seele.db-wal").exists(),
            "a cópia é um arquivo só"
        );
        drop(de_pe);
    }

    #[cfg(unix)]
    #[test]
    fn a_copia_nasce_fechada_para_o_dono() {
        use std::os::unix::fs::PermissionsExt as _;
        let mundo = tempfile::tempdir().expect("tempdir");
        let (banco, _) = servidor(mundo.path());
        let destino = mundo.path().join("copia");
        guardar(&banco, &destino).expect("guardar");
        let modo = |caminho: &Path| {
            std::fs::metadata(caminho)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(modo(&destino), 0o700);
        assert_eq!(modo(&destino.join(BANCO_DA_COPIA)), 0o600);
    }

    #[test]
    fn guardar_recusa_uma_pasta_que_ja_tem_coisa_e_nao_toca_nela() {
        let mundo = tempfile::tempdir().expect("tempdir");
        let (banco, _) = servidor(mundo.path());
        let destino = mundo.path().join("copia");
        std::fs::create_dir_all(&destino).expect("pasta");
        std::fs::write(destino.join("outra-coisa"), b"minha").expect("arquivo");
        let erro = guardar(&banco, &destino).expect_err("tinha de recusar");
        assert!(erro.to_string().contains("não está vazia"), "{erro}");
        assert_eq!(
            std::fs::read(destino.join("outra-coisa")).expect("ficou"),
            b"minha"
        );
        assert!(!destino.join(BANCO_DA_COPIA).exists());
    }

    #[test]
    fn restaurar_traz_a_copia_e_poe_de_lado_o_que_estava_sem_apagar() {
        let mundo = tempfile::tempdir().expect("tempdir");
        let (banco, _) = servidor(mundo.path());
        let destino = mundo.path().join("copia");
        guardar(&banco, &destino).expect("guardar");

        // Depois da cópia o servidor mudou: mais um convite, outro anexo.
        let mut aberto = Persistence::open(&Location::File(banco.clone())).expect("abrir");
        crate::admissao::criar_convite(&mut aberto, "depois").expect("convite");
        drop(aberto);
        std::fs::write(mundo.path().join("anexos/ab/foto"), b"outra foto").expect("anexo");

        let relatorio = restaurar(&destino, &banco, 1_762_000_000).expect("restaurar");

        assert_eq!(convites(&banco), 1, "o banco voltou ao da cópia");
        assert_eq!(
            std::fs::read(mundo.path().join("anexos/ab/foto")).expect("anexo"),
            b"uma foto"
        );
        // O que estava no lugar continua em disco, inteiro.
        let de_lado = mundo.path().join("seele.db.antes-de-restaurar-1762000000");
        assert!(relatorio.posto_de_lado.contains(&de_lado));
        assert_eq!(convites(&de_lado), 2, "o estado anterior não foi apagado");
        assert_eq!(
            std::fs::read(
                mundo
                    .path()
                    .join("anexos.antes-de-restaurar-1762000000/ab/foto")
            )
            .expect("o anexo anterior"),
            b"outra foto"
        );
        // E o servidor sobe em cima do que foi restaurado.
        Persistence::open(&Location::File(banco)).expect("o restaurado abre");
    }

    #[test]
    fn restaurar_recusa_o_que_nao_confere_antes_de_tocar_no_banco() {
        let mundo = tempfile::tempdir().expect("tempdir");
        let (banco, _) = servidor(mundo.path());
        let antes = std::fs::read(&banco).expect("ler");

        let casos: [Estrago<'_>; 4] = [
            ("vazia", &|_| {}, "não é uma cópia do SEELE"),
            (
                "lixo",
                &|pasta| std::fs::write(pasta.join(BANCO_DA_COPIA), b"nada disto").expect("lixo"),
                "não é um banco do SEELE",
            ),
            (
                "outro-sqlite",
                &|pasta| {
                    Connection::open(pasta.join(BANCO_DA_COPIA))
                        .expect("abrir")
                        .execute_batch("CREATE TABLE coisa (x)")
                        .expect("criar");
                },
                "sem o esquema do SEELE",
            ),
            (
                "do-futuro",
                &|pasta| {
                    let copia = pasta.join(BANCO_DA_COPIA);
                    drop(Persistence::open(&Location::File(copia.clone())).expect("abrir"));
                    Connection::open(copia)
                        .expect("abrir")
                        .execute(
                            "UPDATE schema_version SET version = version + 1000 \
                             WHERE version = (SELECT MAX(version) FROM schema_version)",
                            [],
                        )
                        .expect("avançar");
                },
                "versão mais nova",
            ),
        ];
        for (nome, estragar, frase) in casos {
            let copia = mundo.path().join(nome);
            std::fs::create_dir_all(&copia).expect("pasta");
            estragar(&copia);
            let erro = restaurar(&copia, &banco, 7).expect_err(nome);
            assert!(erro.to_string().contains(frase), "{nome}: {erro}");
            assert_eq!(
                std::fs::read(&banco).expect("ler"),
                antes,
                "{nome}: o banco foi tocado por uma restauração recusada"
            );
        }
    }
}
