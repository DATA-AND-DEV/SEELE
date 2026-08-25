//! O ícone de um servidor que **já tinha um** chega a quem acabou de conectar?
//!
//! # Por que este arquivo existe
//!
//! Relato de campo: «o ícone do servidor não está sendo mostrado pro host
//! quando ele hospeda; se um ícone já foi escolhido, precisa resgatar e
//! mostrar».
//!
//! O protocolo já tinha prova disso — `seele-server/tests/personalizacao.rs`
//! mostra o ícone sobrevivendo a um reinício e chegando a quem entra depois. O
//! que **não** tinha prova é o trecho seguinte: a `Connection` dobra a mensagem, a
//! revisão anda, e a casca busca os bytes por causa dela. Três peças, e o
//! defeito relatado cabe em qualquer uma.
//!
//! Este teste cobre exatamente esse trecho, do lado de cá da ponte, para
//! separar «a ponte não entrega» de «a tela não desenha». Sem essa separação a
//! busca continua sendo leitura de código, e leitura de código foi o que me fez
//! errar duas vezes hoje.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use seele_ffi::{ConnectConfig, Connection, ConnectionError};
use seele_server::persistence::{Persistence, Location};
use seele_server::{ServerConfig, Daemon};

/// Um PNG quadrado e opaco, do tamanho que o protocolo aceita.
fn png(lado: u32) -> Vec<u8> {
    let mut quadro = image_de_teste(lado);
    quadro.push(0);
    quadro.pop();
    quadro
}

/// O menor PNG válido que dá para escrever à mão, no lado pedido.
///
/// À mão e não com um crate de imagem: o que este teste precisa é de bytes que
/// `check_server_icon` aceite, e trazer um decodificador para produzir oito
/// dezenas de bytes seria pagar caro por uma constante.
fn image_de_teste(lado: u32) -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&lado.to_be_bytes());
    ihdr.extend_from_slice(&lado.to_be_bytes());
    // 8 bits, cor 6 (RGBA), sem compressão exótica, sem entrelaçamento.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&u32::try_from(ihdr.len() - 4).unwrap_or(0).to_be_bytes());
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&crc(&ihdr).to_be_bytes());
    // Um IEND vazio fecha o arquivo. Nada aqui decodifica a imagem — o que se
    // confere é o cabeçalho, e é ele que este teste precisa ter honesto.
    png.extend_from_slice(&0_u32.to_be_bytes());
    png.extend_from_slice(b"IEND");
    png.extend_from_slice(&crc(b"IEND").to_be_bytes());
    png
}

fn crc(dados: &[u8]) -> u32 {
    let mut valor = 0xFFFF_FFFF_u32;
    for byte in dados {
        valor ^= u32::from(*byte);
        for _ in 0..8 {
            valor = if valor & 1 == 1 {
                (valor >> 1) ^ 0xEDB8_8320
            } else {
                valor >> 1
            };
        }
    }
    valor ^ 0xFFFF_FFFF
}

async fn server(arquivo: std::path::PathBuf) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(arquivo),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

fn conectar(endereco: SocketAddr, casa: &str) -> Result<Arc<Connection>, ConnectionError> {
    Connection::connect(ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "anfitria".to_owned(),
        home: casa.to_owned(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    })
    .map(|(connection, _confianca)| connection)
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_conecta_recebe_o_icone_que_o_server_ja_tinha() -> Result<()> {
    let pasta = std::env::temp_dir().join(format!("seele-icone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta)?;
    let banco = pasta.join("seele.db");
    let imagem = png(64);

    // O ícone é gravado **antes** de qualquer sessão existir, que é o caso do
    // relato: quem hospeda já tinha escolhido a imagem noutro dia.
    {
        let persistence = Persistence::open(&Location::File(banco.clone()))?;
        seele_server::persistence::aparencia::definir_icone(&persistence, Some(&imagem))?;
    }

    let (endereco, servidor) = server(banco).await?;
    let casa = pasta.join("casa");
    let connection = {
        let casa = casa.to_string_lossy().into_owned();
        let endereco = endereco;
        tokio::task::spawn_blocking(move || conectar(endereco, &casa)).await??
    };

    // A revisão é o que a casca observa para saber que há bytes a buscar. Ela
    // começa em zero, e um ícone que chega no aperto de mão tem de movê-la —
    // senão a tela nunca vem buscar, e o defeito é exatamente esse.
    let mut revisao = 0;
    for _ in 0..40 {
        revisao = connection.snapshot().icon_revision;
        if revisao > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        revisao > 0,
        "o ícone chegou e a revisão não andou; a casca não tem como saber que há o que buscar"
    );

    assert_eq!(
        connection.server_icon().as_deref(),
        Some(imagem.as_slice()),
        "a revisão andou e os bytes não vieram"
    );

    servidor.shutdown();
    let _ = std::fs::remove_dir_all(&pasta);
    Ok(())
}
