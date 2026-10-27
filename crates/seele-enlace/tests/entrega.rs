//! A confirmação de entrega (o `stopped()` do QUIC), os traços de E/S do Tokio,
//! o modo anfitrião armado depois, e as sementes do fuzz do envelope.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::net::SocketAddr;
use std::time::Duration;

use seele_enlace::teste::par_em_loopback;
use seele_enlace::{CertificadoDtls, ConfigDoAnfitriao, Discagem, FalhaDoCanal, Pino, Ponta};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn entregue_espera_quem_le_receber_o_fim() {
    let par = par_em_loopback().await;
    let mut envio = par.cliente.open_uni().await.unwrap();
    envio.write_all(&[3_u8; 40_000]).await.unwrap();
    envio.finish().unwrap();
    let mut recebe = par.anfitriao.accept_uni().await.unwrap();
    assert_eq!(
        recebe.read_to_end(1 << 20).await.unwrap(),
        vec![3_u8; 40_000]
    );
    tokio::time::timeout(Duration::from_secs(5), envio.entregue())
        .await
        .unwrap()
        .unwrap();
}

/// A recusa que chega depois do último bloco não vira «entregue».
#[tokio::test]
async fn um_corte_depois_do_ultimo_bloco_nao_e_entrega() {
    let par = par_em_loopback().await;
    let mut envio = par.cliente.open_uni().await.unwrap();
    envio.write_all(b"um anexo inteiro").await.unwrap();
    let mut recebe = par.anfitriao.accept_uni().await.unwrap();
    let mut buf = [0_u8; 16];
    recebe.read_exact(&mut buf).await.unwrap();
    // Quem lê recusa antes de o fim chegar.
    recebe.stop(31).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = envio.finish();
    let resultado = tokio::time::timeout(Duration::from_secs(5), envio.entregue())
        .await
        .unwrap();
    assert_eq!(resultado, Err(FalhaDoCanal::Cortado(31)));
}

#[tokio::test]
async fn os_tracos_de_e_s_do_tokio_funcionam() {
    let par = par_em_loopback().await;
    let mut envio = par.anfitriao.open_uni().await.unwrap();
    let dados: Vec<u8> = (0..300_000).map(|i| (i % 7) as u8).collect();
    let escrita = {
        let dados = dados.clone();
        tokio::spawn(async move {
            AsyncWriteExt::write_all(&mut envio, &dados).await.unwrap();
            envio.shutdown().await.unwrap();
        })
    };
    let mut recebe = par.cliente.accept_uni().await.unwrap();
    let mut lidos = Vec::new();
    AsyncReadExt::read_to_end(&mut recebe, &mut lidos)
        .await
        .unwrap();
    escrita.await.unwrap();
    assert_eq!(lidos, dados);
}

#[tokio::test]
async fn uma_ponta_de_cliente_passa_a_atender_e_para() {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 0));
    let par_de_tela = Ponta::abrir(loopback, None).await.unwrap();
    let quem_assiste = Ponta::abrir(loopback, None).await.unwrap();
    let certificado = CertificadoDtls::efemero().unwrap();
    par_de_tela.passar_a_atender(Some(ConfigDoAnfitriao::novo(certificado.clone())));
    let discagem = Discagem::nova(
        vec![par_de_tela.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    let (a, b) = tokio::join!(quem_assiste.discar(discagem), par_de_tela.aceitar());
    assert!(a.is_ok() && b.is_some());

    par_de_tela.passar_a_atender(None);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut discagem = Discagem::nova(
        vec![par_de_tela.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    discagem.prazo = Duration::from_secs(2);
    assert!(
        quem_assiste.discar(discagem).await.is_err(),
        "desarmada, a ponta não atende"
    );
    assert!(
        a.unwrap().acabou().is_none(),
        "desarmar não derruba quem já entrou"
    );
}

/// Pedido do compat: as sementes do fuzz do envelope são as do montador.
#[test]
fn as_sementes_do_fuzz_sao_as_do_montador() {
    use seele_enlace::envelope;
    let pasta =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/sementes/envelope");
    let ler = |nome: &str| std::fs::read(pasta.join(nome)).unwrap();
    for entrada in std::fs::read_dir(&pasta).unwrap() {
        let bytes = std::fs::read(entrada.unwrap().path()).unwrap();
        assert!(envelope::ler(&bytes).is_ok(), "semente recusada: {bytes:?}");
    }
    assert_eq!(ler("dados"), envelope::dados(&[0x0a, 0x0b, 0, 0]));
    assert_eq!(ler("fim"), envelope::fim());
    assert_eq!(ler("corte"), envelope::corte(2));
    assert_eq!(ler("apresentacao-vazia"), envelope::apresentacao(&[]));
    let mut sala = vec![0x11_u8; 32];
    sala.extend_from_slice(&[0x22; 64]);
    assert_eq!(ler("apresentacao-da-sala"), envelope::apresentacao(&sala));
}

/// Revisão do Codex: o que o enlace não mede sai como `None`, nunca como zero.
/// Os dois lados medem o ida e volta pelos checks: quem liga (controlling) e, desde
/// 26/10/2026, o anfitrião, que é ICE completo e controlado (até ali ICE-lite, que não
/// checava e devolvia `None`).
#[tokio::test]
async fn a_leitura_diz_nao_sei_em_vez_de_zero() {
    let par = par_em_loopback().await;
    par.cliente.send_datagram(vec![1; 50]).unwrap();
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let quem_liga = par.cliente.leitura();
    let anfitriao = par.anfitriao.leitura();
    for leitura in [quem_liga, anfitriao] {
        assert_eq!(leitura.pacotes_perdidos, None);
        assert_eq!(leitura.eventos_de_congestionamento, None);
        assert_eq!(leitura.fracao_de_perda_na_saida, None);
        assert!(leitura.bytes_enviados > 0 && leitura.pacotes_enviados > 0);
    }
    assert!(
        anfitriao.ida_e_volta.is_some(),
        "o anfitrião ICE completo mede ida e volta pelos checks dele"
    );
    assert!(
        quem_liga.ida_e_volta.is_some(),
        "o ICE controlling mede ida e volta pelos checks"
    );
}

/// Pedido do E: `fechar()` solta a porta mesmo com cópias da `Ponta` e
/// conexões vivas por aí, e `fechada()` diz quando ela voltou ao sistema.
#[tokio::test]
async fn fechar_solta_a_porta_mesmo_com_copias_vivas() {
    let par = par_em_loopback().await;
    let copia = par.ponta_do_anfitriao.clone();
    let porta = par.ponta_do_anfitriao.local_addr();
    par.ponta_do_anfitriao.fechar();
    tokio::time::timeout(Duration::from_secs(3), copia.fechada())
        .await
        .expect("a tarefa da ponta não saiu");
    let _ = tokio::time::timeout(Duration::from_secs(3), par.cliente.closed()).await;
    let de_novo = std::net::UdpSocket::bind(porta);
    assert!(
        de_novo.is_ok(),
        "a porta {porta} não voltou ao sistema: {de_novo:?}"
    );
    assert!(
        copia.enviar_bruto(porta, b"x").is_err(),
        "a ponta fechada ainda manda"
    );
    drop(par.anfitriao);
}

/// Defeito 1 do conformance (`quem_hospeda_fecha`): o anfitrião termina o `ctl`
/// (`0x01`) e depois fecha com um código. O corte ainda sai no `ctl`, e o par
/// vê `Fechada { codigo }`, e não uma queda sem código.
#[tokio::test]
async fn o_codigo_do_close_chega_mesmo_depois_do_fim_do_ctl() {
    let par = par_em_loopback().await;
    let (mut envio, _recebe) = par.anfitriao.controle().unwrap();
    envio.write_all(b"tchau").await.unwrap();
    envio.finish().unwrap();
    let (_, mut do_anfitriao) = par.cliente.controle().unwrap();
    assert_eq!(do_anfitriao.read_to_end(1024).await.unwrap(), b"tchau");
    par.anfitriao.close(0x5EE1, b"fim da sala");
    let motivo = tokio::time::timeout(Duration::from_secs(5), par.cliente.closed())
        .await
        .expect("quem liga não viu o fechamento");
    assert_eq!(motivo, seele_enlace::Motivo::Fechada { codigo: 0x5EE1 });
}

/// O mesmo, na pressa do servidor que se despede: a despedida, o fim e o close
/// no mesmo instante, antes de quem liga ler qualquer coisa.
#[tokio::test]
async fn a_despedida_o_fim_e_o_close_juntos_chegam_como_fechada() {
    let par = par_em_loopback().await;
    let (mut envio, _recebe) = par.anfitriao.controle().unwrap();
    envio.write_all(b"Disconnecting").await.unwrap();
    envio.finish().unwrap();
    par.anfitriao.close(0x5EE2, b"encerrando");
    let motivo = tokio::time::timeout(Duration::from_secs(5), par.cliente.closed())
        .await
        .expect("quem liga não viu o fechamento");
    assert_eq!(motivo, seele_enlace::Motivo::Fechada { codigo: 0x5EE2 });
}

/// `quem_hospeda_fecha::a_despedida_avisa…`: quem liga lê o `Disconnecting` e
/// larga a leitura do `ctl`, o que no enlace é fechar (manda `0x02` no `ctl`;
/// quem escreve no `ctl` do outro lado fica sabendo na hora, como o
/// STOP_SENDING do QUIC). O anfitrião devolve a despedida antes de derrubar o
/// DTLS, e os dois lados leem «fechada», nunca «caiu».
#[tokio::test]
async fn largar_a_leitura_do_ctl_fecha_e_os_dois_lados_leem_fechada() {
    let par = par_em_loopback().await;
    let (_envio_do_cliente, recebe_do_cliente) = par.cliente.controle().unwrap();
    drop(recebe_do_cliente);
    let (cliente, anfitriao) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(par.cliente.closed(), par.anfitriao.closed())
    })
    .await
    .expect("largar o ctl não fechou");
    assert_eq!(cliente, seele_enlace::Motivo::Fechada { codigo: 0 });
    assert_eq!(anfitriao, seele_enlace::Motivo::Fechada { codigo: 0 });
}

/// Defeito 2 do conformance: `Ponta::fechar()` (desligar o servidor) despede
/// cada conexão com um código no fio, e o par vê `Fechada { codigo: 0 }`.
#[tokio::test]
async fn desligar_a_ponta_fecha_cada_conexao_com_codigo() {
    let par = par_em_loopback().await;
    par.ponta_do_anfitriao.fechar();
    let motivo = tokio::time::timeout(Duration::from_secs(5), par.cliente.closed())
        .await
        .expect("quem liga não viu o desligar");
    assert_eq!(motivo, seele_enlace::Motivo::Fechada { codigo: 0 });
}

/// O que o eco faz com o navegador, sem o navegador: um "u" grande sobe, o
/// anfitrião o lê inteiro e devolve por um "u" novo, e quem ligou o lê inteiro.
/// Três rodadas seguidas na mesma conexão, para os ids de canal fechados serem
/// reaproveitados (o `Drop ChannelClose event` que o str0m avisa).
#[tokio::test]
async fn um_u_grande_vai_e_volta_varias_vezes_na_mesma_conexao() {
    let par = par_em_loopback().await;
    let tamanho = 8 * 1024 * 1024;
    for rodada in 0..3_u8 {
        let dados: Vec<u8> = (0..tamanho).map(|i| (i as u8) ^ rodada).collect();
        let mut subida = par.cliente.open_uni().await.unwrap();
        let escrita = {
            let dados = dados.clone();
            tokio::spawn(async move {
                subida.write_all(&dados).await.unwrap();
                subida.finish().unwrap();
                subida.entregue().await.unwrap();
            })
        };
        let mut chegou = par.anfitriao.accept_uni().await.unwrap();
        let lido = tokio::time::timeout(Duration::from_secs(20), chegou.read_to_end(64 << 20))
            .await
            .expect("a subida não terminou")
            .unwrap();
        assert_eq!(lido.len(), tamanho, "rodada {rodada}: subida");
        escrita.await.unwrap();

        let mut descida = par.anfitriao.open_uni().await.unwrap();
        let volta = tokio::spawn(async move {
            descida.write_all(&lido).await.unwrap();
            descida.finish().unwrap();
            descida.entregue().await.unwrap();
        });
        let mut de_volta = par.cliente.accept_uni().await.unwrap();
        let voltou = tokio::time::timeout(Duration::from_secs(20), de_volta.read_to_end(64 << 20))
            .await
            .expect("a descida não terminou")
            .unwrap();
        assert_eq!(voltou, dados, "rodada {rodada}: os bytes de volta");
        volta.await.unwrap();
    }
    assert!(par.cliente.acabou().is_none() && par.anfitriao.acabou().is_none());
}
