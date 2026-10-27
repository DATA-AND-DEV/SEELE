//! Um anfitrião e quem liga, de verdade, em loopback: dois sockets UDP, dois
//! str0m, ICE, DTLS e SCTP inteiros. Nada aqui é simulado.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_enlace::{
    CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, FalhaAoDiscar, FalhaDoCanal, Motivo,
    Pino, Ponta,
};

fn rastro() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

async fn anfitriao_com(config: ConfigDoAnfitriao) -> Ponta {
    Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), Some(config))
        .await
        .unwrap()
}

async fn anfitriao() -> (Ponta, CertificadoDtls) {
    let certificado = CertificadoDtls::efemero().unwrap();
    let ponta = anfitriao_com(ConfigDoAnfitriao::novo(certificado.clone())).await;
    (ponta, certificado)
}

async fn cliente() -> Ponta {
    Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap()
}

/// Disca e aceita ao mesmo tempo: devolve (lado de quem liga, lado do anfitrião).
async fn ligar(anfitriao: &Ponta, impressao: [u8; 32], cliente: &Ponta) -> (Conexao, Conexao) {
    let discagem = Discagem::nova(vec![anfitriao.local_addr()], Pino::Impressao(impressao));
    let (discou, chegou) = tokio::join!(cliente.discar(discagem), async {
        tokio::time::timeout(Duration::from_secs(10), anfitriao.aceitar())
            .await
            .expect("o anfitrião não recebeu ninguém em 10 s")
            .expect("a ponta do anfitrião fechou")
    });
    (discou.expect("a discagem falhou"), chegou)
}

#[tokio::test]
async fn ctl_e_voz_atravessam_nos_dois_sentidos() {
    rastro();
    let (host, certificado) = anfitriao().await;
    let cli = cliente().await;
    let (a, b) = ligar(&host, certificado.impressao(), &cli).await;

    assert_eq!(a.impressao_dtls_do_par(), certificado.impressao());
    assert_eq!(b.impressao_dtls_do_par(), a.impressao_dtls_local());
    assert_eq!(a.apresentacao(), b"");
    assert_eq!(a.credencial(), b.credencial());

    let (mut a_envio, mut a_recebe) = a.controle().unwrap();
    let (mut b_envio, mut b_recebe) = b.controle().unwrap();
    assert!(a.controle().is_none(), "o ctl sai uma vez só");

    a_envio.write_all(b"ola do cliente").await.unwrap();
    let mut buf = [0_u8; 14];
    b_recebe.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ola do cliente");

    b_envio.write_all(b"ola do host").await.unwrap();
    let mut buf = [0_u8; 11];
    a_recebe.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ola do host");

    // Voz: sem ordem e sem retransmissão, mas em loopback chega.
    for n in 1..=20_u8 {
        a.send_datagram(vec![n; 120]).unwrap();
    }
    let mut vistos = Vec::new();
    for _ in 0..20 {
        let d = tokio::time::timeout(Duration::from_secs(5), b.read_datagram())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(d.len(), 120);
        vistos.push(d[0]);
    }
    vistos.sort_unstable();
    assert_eq!(vistos, (1..=20).collect::<Vec<u8>>());
    assert!(a.send_datagram(vec![1; seele_enlace::MAX_VOZ + 1]).is_err());
}

#[tokio::test]
async fn fluxos_u_e_b_com_fim_e_corte() {
    rastro();
    let (host, certificado) = anfitriao().await;
    let cli = cliente().await;
    let (a, b) = ligar(&host, certificado.impressao(), &cli).await;

    // "u" com 1 MiB e fim.
    let dados: Vec<u8> = (0..1024 * 1024).map(|i| (i % 251) as u8).collect();
    let mut envio = a.open_uni().await.unwrap();
    let escrita = {
        let dados = dados.clone();
        tokio::spawn(async move {
            envio.write_all(&dados).await.unwrap();
            envio.finish().unwrap();
        })
    };
    let mut recebe = b.accept_uni().await.unwrap();
    let lido = recebe.read_to_end(2 * 1024 * 1024).await.unwrap();
    escrita.await.unwrap();
    assert_eq!(lido, dados);

    // "u" cortado por quem escreve: quem lê vê o código.
    let mut envio = a.open_uni().await.unwrap();
    envio.write_all(b"metade").await.unwrap();
    envio.reset(77).unwrap();
    let mut recebe = b.accept_uni().await.unwrap();
    let mut buf = vec![0_u8; 64];
    let mut erro = None;
    for _ in 0..4 {
        match recebe.read(&mut buf).await {
            Ok(Some(_)) => {}
            Ok(None) => panic!("o corte não pode virar fim"),
            Err(e) => {
                erro = Some(e);
                break;
            }
        }
    }
    assert_eq!(erro, Some(FalhaDoCanal::Cortado(77)));

    // "u" parado por quem lê: quem escreve fica sabendo.
    let mut envio = b.open_uni().await.unwrap();
    envio.write_all(b"x").await.unwrap();
    let mut recebe = a.accept_uni().await.unwrap();
    recebe.stop(9).unwrap();
    let inicio = Instant::now();
    loop {
        match envio.write_all(b"mais").await {
            Err(FalhaDoCanal::Cortado(9)) => break,
            Err(outro) => panic!("esperava o corte 9, veio {outro:?}"),
            Ok(()) => {
                assert!(
                    inicio.elapsed() < Duration::from_secs(5),
                    "o corte não chegou a quem escreve"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

    // "b": pergunta e resposta, cada lado com o seu fim.
    let (mut pergunta_envio, mut pergunta_recebe) = a.open_bi().await.unwrap();
    pergunta_envio.write_all(b"qual?").await.unwrap();
    pergunta_envio.finish().unwrap();
    let (mut resposta_envio, mut resposta_recebe) = b.accept_bi().await.unwrap();
    assert_eq!(resposta_recebe.read_to_end(64).await.unwrap(), b"qual?");
    resposta_envio.write_all(b"esta").await.unwrap();
    resposta_envio.finish().unwrap();
    assert_eq!(pergunta_recebe.read_to_end(64).await.unwrap(), b"esta");
}

/// Contrapressão de verdade (o crédito `0x04`): quem lê devagar segura quem
/// escreve, e nada é cortado. Nos dois sentidos, com um anexo de 25 MiB, como o
/// do `anexos` da conformidade, que parava em ~2,2 MiB quando a fila cortava.
async fn quem_le_devagar_segura_quem_escreve(de_cliente: bool) {
    rastro();
    let (host, certificado) = anfitriao().await;
    let cli = cliente().await;
    let (a, b) = ligar(&host, certificado.impressao(), &cli).await;
    let (escreve, le) = if de_cliente { (a, b) } else { (b, a) };
    let total = 25 * 1024 * 1024;
    let mut envio = escreve.open_uni().await.unwrap();
    let mandados = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let escrita = {
        let mandados = Arc::clone(&mandados);
        tokio::spawn(async move {
            let pedaco = vec![9_u8; 64 * 1024];
            for _ in 0..(total / pedaco.len()) {
                envio.write_all(&pedaco).await.unwrap();
                mandados.fetch_add(pedaco.len(), std::sync::atomic::Ordering::Relaxed);
            }
            envio.finish().unwrap();
            envio.entregue().await.unwrap();
        })
    };
    let mut recebe = le.accept_uni().await.unwrap();
    // Quem lê fica parado: quem escreve tem de parar perto da janela.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let parado_em = mandados.load(std::sync::atomic::Ordering::Relaxed);
    let teto = seele_enlace::MAX_FILA_DE_ENTRADA
        + seele_enlace::CREDITO_DE_SAIDA
        + 2 * seele_enlace::MAX_MENSAGEM;
    assert!(
        parado_em <= teto,
        "quem escreve não parou: {parado_em} B aceitos com quem lê parado (teto {teto} B)"
    );
    // E agora lê, devagar, até o fim.
    let mut buf = vec![0_u8; 64 * 1024];
    let mut lidos = 0;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), recebe.read(&mut buf)).await {
            Ok(Ok(Some(n))) => lidos += n,
            Ok(Ok(None)) => break,
            Ok(Err(erro)) => panic!("o canal caiu depois de {lidos} B: {erro}"),
            Err(_) => panic!("a leitura parou em {lidos} B"),
        }
        if lidos % (1024 * 1024) < 64 * 1024 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    assert_eq!(lidos, total);
    tokio::time::timeout(Duration::from_secs(10), escrita)
        .await
        .expect("quem escreve não viu a entrega")
        .unwrap();
    assert_eq!(
        le.contadores().cortes_por_fila,
        0,
        "a fila cortou em vez de segurar"
    );
    assert!(escreve.acabou().is_none() && le.acabou().is_none());
}

#[tokio::test]
async fn quem_le_devagar_segura_quem_escreve_do_cliente_ao_anfitriao() {
    quem_le_devagar_segura_quem_escreve(true).await;
}

#[tokio::test]
async fn quem_le_devagar_segura_quem_escreve_do_anfitriao_ao_cliente() {
    quem_le_devagar_segura_quem_escreve(false).await;
}

#[tokio::test]
async fn a_impressao_errada_e_recusada_antes_de_qualquer_canal() {
    rastro();
    let (host, _certificado) = anfitriao().await;
    let cli = cliente().await;
    let outra = CertificadoDtls::efemero().unwrap().impressao();
    let discagem = Discagem::nova(vec![host.local_addr()], Pino::Impressao(outra));
    let resultado = cli.discar(discagem).await;
    match resultado {
        Err(FalhaAoDiscar::ImpressaoDiferente { .. }) => {}
        outro => panic!("esperava ImpressaoDiferente, veio {outro:?}"),
    }
    // E nada chegou ao anfitrião como conexão.
    let chegou = tokio::time::timeout(Duration::from_millis(500), host.aceitar()).await;
    assert!(
        chegou.is_err(),
        "uma impressão errada não vira conexão do outro lado"
    );
}

#[tokio::test]
async fn sem_ninguem_no_endereco_a_discagem_diz_sem_resposta() {
    rastro();
    let mudo = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let cli = cliente().await;
    let mut discagem = Discagem::nova(vec![mudo.local_addr().unwrap()], Pino::Qualquer);
    discagem.prazo = Duration::from_secs(2);
    match cli.discar(discagem).await {
        Err(FalhaAoDiscar::SemResposta) => {}
        outro => panic!("esperava SemResposta, veio {outro:?}"),
    }
}

#[tokio::test]
async fn o_primeiro_contato_sem_pino_mostra_a_impressao_vista() {
    rastro();
    let (host, certificado) = anfitriao().await;
    let cli = cliente().await;
    let discagem = Discagem::nova(vec![host.local_addr()], Pino::Qualquer);
    let (discou, _chegou) = tokio::join!(cli.discar(discagem), host.aceitar());
    assert_eq!(
        discou.unwrap().impressao_dtls_do_par(),
        certificado.impressao()
    );
}

#[tokio::test]
async fn a_apresentacao_do_anfitriao_chega_antes_do_ctl() {
    rastro();
    let certificado = CertificadoDtls::efemero().unwrap();
    let mut config = ConfigDoAnfitriao::novo(certificado.clone());
    config.apresentacao = vec![0xAB; 96];
    let host = anfitriao_com(config).await;
    let cli = cliente().await;
    let (a, b) = ligar(&host, certificado.impressao(), &cli).await;
    assert_eq!(a.apresentacao(), &[0xAB; 96][..]);
    // E ela não aparece no fluxo de controle.
    let (_a_envio, mut a_recebe) = a.controle().unwrap();
    let (mut b_envio, _b_recebe) = b.controle().unwrap();
    b_envio.write_all(b"z").await.unwrap();
    let mut buf = [0_u8; 1];
    a_recebe.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"z");
}

#[tokio::test]
async fn fechar_com_codigo_chega_ao_par() {
    rastro();
    let (host, certificado) = anfitriao().await;
    let cli = cliente().await;
    let (a, b) = ligar(&host, certificado.impressao(), &cli).await;
    b.close(42, b"tchau");
    let motivo = tokio::time::timeout(Duration::from_secs(5), a.closed())
        .await
        .unwrap();
    assert_eq!(motivo, Motivo::Fechada { codigo: 42 });
    assert!(a.read_datagram().await.is_err());
}

#[tokio::test]
async fn o_balde_do_anfitriao_e_consultado_antes_de_criar_o_par() {
    rastro();
    let certificado = CertificadoDtls::efemero().unwrap();
    let mut config = ConfigDoAnfitriao::novo(certificado.clone());
    let consultas = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let contador = Arc::clone(&consultas);
    config.admitir = Arc::new(move |_| {
        contador.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        false
    });
    let host = anfitriao_com(config).await;
    let cli = cliente().await;
    let mut discagem = Discagem::nova(
        vec![host.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    discagem.prazo = Duration::from_secs(2);
    assert!(cli.discar(discagem).await.is_err());
    assert!(consultas.load(std::sync::atomic::Ordering::Relaxed) >= 1);
    assert_eq!(
        host.conexoes_vivas(),
        0,
        "recusado pelo balde, nenhum Rtc foi criado"
    );
    assert!(host.contadores().bindings_recusados >= 1);
}

#[tokio::test]
async fn tres_discagens_simultaneas_e_fechar_uma_nao_mexe_nas_outras() {
    rastro();
    // Três anfitriões diferentes, como três servidores, e um cliente com uma
    // ponta só para os três (a forma mais exigente do E3).
    let mut hosts = Vec::new();
    for _ in 0..3 {
        hosts.push(anfitriao().await);
    }
    let cli = cliente().await;
    let mut conexoes = Vec::new();
    let discagens = hosts
        .iter()
        .map(|(host, certificado)| ligar(host, certificado.impressao(), &cli));
    for par in futures_join_all(discagens).await {
        conexoes.push(par);
    }
    assert_eq!(cli.conexoes_vivas(), 3);

    let mut controles: Vec<_> = conexoes
        .iter()
        .map(|(a, b)| (a.controle().unwrap(), b.controle().unwrap()))
        .collect();

    // Fecha a do meio.
    conexoes[1].0.close(5, b"so esta");
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), conexoes[1].1.closed())
            .await
            .unwrap(),
        Motivo::Fechada { codigo: 5 }
    );

    // As outras duas continuam conversando nos dois sentidos.
    for i in [0, 2] {
        let ((a_envio, a_recebe), (b_envio, b_recebe)) = &mut controles[i];
        a_envio.write_all(&[i as u8; 3]).await.unwrap();
        let mut buf = [0_u8; 3];
        b_recebe.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, [i as u8; 3]);
        b_envio.write_all(b"ok").await.unwrap();
        let mut buf = [0_u8; 2];
        a_recebe.read_exact(&mut buf).await.unwrap();
        conexoes[i].0.send_datagram(vec![9; 50]).unwrap();
        let voz = tokio::time::timeout(Duration::from_secs(5), conexoes[i].1.read_datagram())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(voz, vec![9; 50]);
        assert!(conexoes[i].0.acabou().is_none());
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(cli.conexoes_vivas(), 2);
}

async fn futures_join_all<F: std::future::Future>(
    futuros: impl Iterator<Item = F>,
) -> Vec<F::Output> {
    let tarefas: Vec<_> = futuros.collect();
    let mut saida = Vec::new();
    // Todos ao mesmo tempo: cada um num `select` cooperativo seria serial, então
    // eles andam juntos por um `join` em cadeia.
    let mut pinos: Vec<std::pin::Pin<Box<F>>> = tarefas.into_iter().map(Box::pin).collect();
    let mut prontos: Vec<Option<F::Output>> = (0..pinos.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut todos = true;
        for (i, futuro) in pinos.iter_mut().enumerate() {
            if prontos[i].is_none() {
                match futuro.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => prontos[i] = Some(v),
                    std::task::Poll::Pending => todos = false,
                }
            }
        }
        if todos {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    for v in prontos {
        saida.push(v.unwrap());
    }
    saida
}
