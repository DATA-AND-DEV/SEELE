//! `programa.escrever` e `programa.desempacotar` (API 8 parte 4), com pacotes
//! de verdade montados aqui: um `.tar.gz` pelo `tar` e pelo `flate2`, e um
//! `.zip` escrito à mão (stored e deflate), no formato que a Adoptium publica
//! para o Windows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use seele_core::programa::{
    Origem, OrigemHttps, PedidoDeDesempacotar, Politica, Programa, Recusa, MAX_BYTES_ESCRITOS,
};

struct Pasta(PathBuf);

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn programa(nome: &str, teto: u64) -> (Pasta, Programa) {
    static N: AtomicU64 = AtomicU64::new(0);
    let caminho = std::env::temp_dir().join(format!(
        "seele-programa-arq-{nome}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&caminho);
    let politica = Politica {
        dominios: vec![],
        artefatos: vec![],
        de_outros_sistemas: vec![],
        origens: vec![],
        comando: None,
        teto_de_disco: teto,
        raiz: caminho.join("jogo"),
    };
    let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::nova().unwrap());
    let programa = Programa::novo(politica, origem).unwrap();
    (Pasta(caminho), programa)
}

/// Nenhuma pasta ou arquivo parcial sobrou na raiz.
fn sem_parcial(raiz: &Path) -> bool {
    let mut pendentes = vec![raiz.to_path_buf()];
    while let Some(p) = pendentes.pop() {
        for e in std::fs::read_dir(&p).unwrap().flatten() {
            let c = e.path();
            if c.to_string_lossy().contains("seele-parcial") {
                return false;
            }
            if c.is_dir() {
                pendentes.push(c);
            }
        }
    }
    true
}

// ------------------------------------------------------------ escrever

#[test]
fn escrever_grava_troca_e_fica_na_pasta() {
    let (_p, programa) = programa("escrever", 1 << 20);
    let alvo = programa.escrever("eula.txt", "eula=true\n").unwrap();
    assert_eq!(std::fs::read_to_string(&alvo).unwrap(), "eula=true\n");
    programa.escrever("eula.txt", "eula=false\n").unwrap();
    assert_eq!(std::fs::read_to_string(&alvo).unwrap(), "eula=false\n");
    programa
        .escrever("config/server.properties", "motd=oi\n")
        .unwrap();
    assert!(programa.raiz().join("config/server.properties").is_file());

    for (caminho, nome) in [
        ("../fora.txt", "caminho-com-pai"),
        ("/etc/x", "caminho-absoluto"),
        ("", "caminho-vazio"),
    ] {
        let recusa = programa.escrever(caminho, "x").unwrap_err();
        assert_eq!(recusa.nome(), nome, "{caminho}");
    }
    let grande = "a".repeat(usize::try_from(MAX_BYTES_ESCRITOS).unwrap() + 1);
    assert!(matches!(
        programa.escrever("grande.txt", &grande),
        Err(Recusa::TextoGrandeDemais { .. })
    ));
    let recusas = programa.recusas();
    assert_eq!(recusas.get("caminho-com-pai"), Some(&1));
    assert_eq!(recusas.get("texto-grande-demais"), Some(&1));
    assert!(sem_parcial(programa.raiz()));
}

#[test]
fn escrever_respeita_o_teto_de_disco() {
    let (_p, programa) = programa("escrever-teto", 10);
    programa.escrever("a.txt", "12345").unwrap();
    match programa.escrever("b.txt", "123456") {
        Err(Recusa::DiscoCheio {
            usado,
            pedido,
            teto,
        }) => {
            assert_eq!((usado, pedido, teto), (5, 6, 10));
        }
        outro => panic!("passou do teto: {outro:?}"),
    }
    // Trocar um arquivo conta só a diferença.
    programa.escrever("a.txt", "1234567890").unwrap();
}

// ------------------------------------------------------------ os pacotes

/// Um `.tar.gz` com o formato do JRE da Adoptium no Mac: tudo sob
/// `jdk-21-jre/Contents/Home`, com o `bin/java` executável e um link.
fn tar_gz(entradas: &[(&str, &[u8], u32)], links: &[(&str, &str)]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    for (nome, bytes, modo) in entradas {
        let mut cabecalho = tar::Header::new_gnu();
        cabecalho.set_size(bytes.len() as u64);
        cabecalho.set_mode(*modo);
        cabecalho.set_entry_type(tar::EntryType::Regular);
        cabecalho.set_cksum();
        tar.append_data(&mut cabecalho, nome, *bytes).unwrap();
    }
    for (nome, alvo) in links {
        let mut cabecalho = tar::Header::new_gnu();
        cabecalho.set_size(0);
        cabecalho.set_entry_type(tar::EntryType::Symlink);
        cabecalho.set_cksum();
        tar.append_link(&mut cabecalho, nome, alvo).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

/// Um `.tar.gz` com uma entrada de nome cru (o `tar` recusa `..` ao montar,
/// então o nome vai direto no cabeçalho).
fn tar_gz_cru(nome: &str, bytes: &[u8]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    let mut cabecalho = tar::Header::new_old();
    cabecalho.as_old_mut().name[..nome.len()].copy_from_slice(nome.as_bytes());
    cabecalho.set_size(bytes.len() as u64);
    cabecalho.set_mode(0o644);
    cabecalho.set_entry_type(tar::EntryType::Regular);
    cabecalho.set_cksum();
    tar.append(&cabecalho, bytes).unwrap();
    tar.into_inner().unwrap().finish().unwrap()
}

/// Um `.zip` com entradas `(nome, bytes, deflate)`, como o da Adoptium.
fn zip(entradas: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut saida = Vec::new();
    let mut central = Vec::new();
    for (nome, bytes, deflate) in entradas {
        let mut crc = flate2::Crc::new();
        crc.update(bytes);
        let dados = if *deflate {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(bytes).unwrap();
            e.finish().unwrap()
        } else {
            bytes.to_vec()
        };
        let metodo: u16 = if *deflate { 8 } else { 0 };
        let local = u32::try_from(saida.len()).unwrap();
        let campos = |s: &mut Vec<u8>| {
            s.extend_from_slice(&20_u16.to_le_bytes());
            s.extend_from_slice(&0_u16.to_le_bytes());
            s.extend_from_slice(&metodo.to_le_bytes());
            s.extend_from_slice(&[0, 0, 0, 0]);
            s.extend_from_slice(&crc.sum().to_le_bytes());
            s.extend_from_slice(&u32::try_from(dados.len()).unwrap().to_le_bytes());
            s.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
            s.extend_from_slice(&u16::try_from(nome.len()).unwrap().to_le_bytes());
            s.extend_from_slice(&0_u16.to_le_bytes());
        };
        saida.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        campos(&mut saida);
        saida.extend_from_slice(nome.as_bytes());
        saida.extend_from_slice(&dados);
        central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        central.extend_from_slice(&20_u16.to_le_bytes());
        campos(&mut central);
        central.extend_from_slice(&[0; 10]);
        central.extend_from_slice(&local.to_le_bytes());
        central.extend_from_slice(nome.as_bytes());
    }
    let inicio = u32::try_from(saida.len()).unwrap();
    let n = u16::try_from(entradas.len()).unwrap();
    saida.extend_from_slice(&central);
    saida.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    saida.extend_from_slice(&n.to_le_bytes());
    saida.extend_from_slice(&n.to_le_bytes());
    saida.extend_from_slice(&u32::try_from(central.len()).unwrap().to_le_bytes());
    saida.extend_from_slice(&inicio.to_le_bytes());
    saida.extend_from_slice(&0_u16.to_le_bytes());
    saida
}

fn pedido(arquivo: &str, destino: &str, raiz: &str) -> PedidoDeDesempacotar {
    PedidoDeDesempacotar {
        arquivo: arquivo.to_owned(),
        destino: destino.to_owned(),
        raiz: raiz.to_owned(),
    }
}

/// **O Java da Adoptium no Mac**: só o que está sob `Contents/Home` vai para
/// `java/`, o `bin/java` sai executável, e o link não é criado — é contado.
#[test]
fn o_tar_gz_do_mac_vira_a_pasta_do_java() {
    let (_p, programa) = programa("tar", 1 << 20);
    let bytes = tar_gz(
        &[
            ("jdk-21-jre/Contents/Home/bin/java", b"#!java", 0o755),
            ("jdk-21-jre/Contents/Home/lib/modules", b"modulos", 0o644),
            ("jdk-21-jre/Contents/Info.plist", b"plist", 0o644),
        ],
        &[("jdk-21-jre/Contents/Home/legal/x", "../../../../etc/passwd")],
    );
    std::fs::write(programa.raiz().join("java.tar.gz"), &bytes).unwrap();
    // O destino de antes é trocado inteiro.
    std::fs::create_dir_all(programa.raiz().join("java/velho")).unwrap();

    let feito = programa
        .desempacotar(&pedido("java.tar.gz", "java", "jdk-21-jre/Contents/Home"))
        .unwrap();
    assert_eq!(
        (feito.arquivos, feito.links_pulados, feito.fora_da_raiz),
        (2, 1, 1)
    );
    let java = programa.raiz().join("java/bin/java");
    assert_eq!(std::fs::read(&java).unwrap(), b"#!java");
    assert!(
        !programa.raiz().join("java/velho").exists(),
        "o destino velho ficou"
    );
    assert!(
        !programa.raiz().join("java/legal/x").exists(),
        "o link foi criado"
    );
    assert!(!programa.raiz().join("java/Info.plist").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let modo = std::fs::metadata(&java).unwrap().permissions().mode();
        assert_eq!(modo & 0o111, 0o111, "o java não ficou executável: {modo:o}");
    }
    assert!(sem_parcial(programa.raiz()));
}

/// **O `.zip` do Windows**, stored e deflate, com o CRC conferido.
#[test]
fn o_zip_do_windows_vira_a_pasta_do_java() {
    let (_p, programa) = programa("zip", 1 << 20);
    let grande = "um modulo comprimido ".repeat(500);
    let bytes = zip(&[
        ("jdk-21-jre/", b"", false),
        ("jdk-21-jre/bin/java.exe", b"MZ java", false),
        ("jdk-21-jre/lib/modules", grande.as_bytes(), true),
        ("LEIA.txt", b"fora", false),
    ]);
    std::fs::write(programa.raiz().join("java.zip"), &bytes).unwrap();
    let feito = programa
        .desempacotar(&pedido("java.zip", "java", "jdk-21-jre"))
        .unwrap();
    assert_eq!((feito.arquivos, feito.fora_da_raiz), (2, 1));
    assert_eq!(
        std::fs::read(programa.raiz().join("java/bin/java.exe")).unwrap(),
        b"MZ java"
    );
    assert_eq!(
        std::fs::read_to_string(programa.raiz().join("java/lib/modules")).unwrap(),
        grande
    );

    // Um byte trocado no meio dos dados guardados é o CRC que não bate.
    let mut adulterado = bytes.clone();
    let i = adulterado.windows(7).position(|w| w == b"MZ java").unwrap();
    adulterado[i] = b'X';
    std::fs::write(programa.raiz().join("ruim.zip"), &adulterado).unwrap();
    let recusa = programa
        .desempacotar(&pedido("ruim.zip", "java2", "jdk-21-jre"))
        .unwrap_err();
    assert!(
        matches!(&recusa, Recusa::PacoteInvalido { motivo, .. } if motivo.contains("CRC")),
        "{recusa:?}"
    );
    assert!(!programa.raiz().join("java2").exists());
    assert!(sem_parcial(programa.raiz()));
}

/// **Uma entrada que sairia da pasta recusa o pacote inteiro**, e nada fica.
#[test]
fn uma_entrada_com_pai_recusa_o_pacote_e_nada_fica() {
    let (p, programa) = programa("pai", 1 << 20);
    std::fs::write(
        programa.raiz().join("mau.tar.gz"),
        tar_gz_cru("../../fora.txt", b"escapei"),
    )
    .unwrap();
    let recusa = programa
        .desempacotar(&pedido("mau.tar.gz", "java", ""))
        .unwrap_err();
    assert_eq!(recusa.nome(), "pacote-invalido", "{recusa:?}");
    assert!(!p.0.join("fora.txt").exists());
    assert!(!programa.raiz().join("java").exists());
    assert!(sem_parcial(programa.raiz()));
    assert_eq!(programa.recusas().get("pacote-invalido"), Some(&1));
}

#[test]
fn o_que_nao_e_pacote_ou_nao_tem_a_raiz_e_dito() {
    let (_p, programa) = programa("formatos", 1 << 20);
    std::fs::write(programa.raiz().join("texto.txt"), b"nao sou pacote").unwrap();
    assert!(matches!(
        programa.desempacotar(&pedido("texto.txt", "x", "")),
        Err(Recusa::FormatoDesconhecido { .. })
    ));
    std::fs::write(
        programa.raiz().join("j.tar.gz"),
        tar_gz(&[("outra/bin/java", b"j", 0o755)], &[]),
    )
    .unwrap();
    assert!(matches!(
        programa.desempacotar(&pedido("j.tar.gz", "java", "jdk-21-jre")),
        Err(Recusa::NadaSobARaiz { .. })
    ));
    for (arquivo, destino, nome) in [
        ("../j.tar.gz", "java", "caminho-com-pai"),
        ("j.tar.gz", "/tmp/java", "caminho-absoluto"),
        ("j.tar.gz", "", "caminho-vazio"),
    ] {
        let recusa = programa
            .desempacotar(&pedido(arquivo, destino, ""))
            .unwrap_err();
        assert_eq!(recusa.nome(), nome, "{arquivo} -> {destino}");
    }
    assert!(sem_parcial(programa.raiz()));
}

/// **O que se abre conta no teto**, e passar dele para no meio, sem sobrar nada.
#[test]
fn desempacotar_para_no_teto_de_disco() {
    let bytes = tar_gz(
        &[("j/a", &[7_u8; 4000], 0o644), ("j/b", &[8_u8; 4000], 0o644)],
        &[],
    );
    let teto = bytes.len() as u64 + 5000;
    let (_p, programa) = programa("teto", teto);
    std::fs::write(programa.raiz().join("j.tar.gz"), &bytes).unwrap();
    match programa.desempacotar(&pedido("j.tar.gz", "java", "j")) {
        Err(Recusa::DiscoCheio { .. }) => {}
        outro => panic!("passou do teto: {outro:?}"),
    }
    assert!(!programa.raiz().join("java").exists());
    assert!(sem_parcial(programa.raiz()));
}
