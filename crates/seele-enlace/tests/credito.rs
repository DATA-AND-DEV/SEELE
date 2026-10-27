//! O crédito `0x04` obrigatório da linha 2 (Δ8 do contrato): nenhum byte de
//! dados sai antes do primeiro anúncio; fim e corte nunca esperam; vários canais
//! ao mesmo tempo andam sem corte nem perda.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use seele_enlace::teste::{credito_em_aberto, par_em_loopback, reter_credito};
use seele_enlace::FalhaDoCanal;

/// Lê o que chegar em `janela`, sem esperar o fim.
async fn ler_por(recebe: &mut seele_enlace::Recebe, janela: Duration) -> usize {
    let mut buf = vec![0_u8; 64 * 1024];
    let mut lidos = 0;
    let prazo = tokio::time::Instant::now() + janela;
    while let Ok(lido) = tokio::time::timeout_at(prazo, recebe.read(&mut buf)).await {
        match lido.unwrap() {
            Some(n) => lidos += n,
            None => break,
        }
    }
    lidos
}

/// O primeiro crédito chega atrasado: quem escreve espera, nada de dados chega
/// antes dele, e depois chega tudo, sem corte.
#[tokio::test]
async fn o_primeiro_credito_atrasado_segura_quem_escreve_sem_perda() {
    let par = par_em_loopback().await;
    reter_credito(&par.anfitriao, true);
    let total = 3 * 1024 * 1024;
    let dados: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
    let mut envio = par.cliente.open_uni().await.unwrap();
    let escrita = {
        let dados = dados.clone();
        tokio::spawn(async move {
            envio.write_all(&dados).await.unwrap();
            envio.finish().unwrap();
            envio.entregue().await.unwrap();
        })
    };
    let mut recebe = par.anfitriao.accept_uni().await.unwrap();
    assert_eq!(
        ler_por(&mut recebe, Duration::from_millis(600)).await,
        0,
        "dados saíram antes do primeiro crédito"
    );
    assert!(!escrita.is_finished());
    reter_credito(&par.anfitriao, false);
    let recebido = tokio::time::timeout(Duration::from_secs(10), recebe.read_to_end(total))
        .await
        .expect("o crédito solto não destravou quem escreve")
        .unwrap();
    assert!(recebido == dados, "chegou diferente");
    tokio::time::timeout(Duration::from_secs(5), escrita)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(par.anfitriao.contadores().cortes_por_fila, 0);
}

/// Crédito nenhum: nenhum byte de dados sai, mas o fim e o corte saem.
#[tokio::test]
async fn sem_credito_nada_de_dados_sai_mas_fim_e_corte_saem() {
    let par = par_em_loopback().await;
    reter_credito(&par.anfitriao, true);

    // O fim de um canal sem dados chega.
    let mut vazio = par.cliente.open_uni().await.unwrap();
    vazio.finish().unwrap();
    let mut recebe_vazio = par.anfitriao.accept_uni().await.unwrap();
    let fim = tokio::time::timeout(Duration::from_secs(5), recebe_vazio.read_to_end(16))
        .await
        .expect("o fim esperou crédito")
        .unwrap();
    assert!(fim.is_empty());

    // Dados não; o corte de quem escreve, sim.
    let mut envio = par.cliente.open_uni().await.unwrap();
    let mut recebe = par.anfitriao.accept_uni().await.unwrap();
    let escrita = tokio::spawn(async move {
        let resultado = envio.write_all(&[7_u8; 200_000]).await;
        (envio, resultado)
    });
    assert_eq!(ler_por(&mut recebe, Duration::from_millis(600)).await, 0);
    assert!(!escrita.is_finished(), "quem escreve não esperou o crédito");
    escrita.abort();
    let mut outro = par.cliente.open_uni().await.unwrap();
    let mut recebe_outro = par.anfitriao.accept_uni().await.unwrap();
    outro.write_all(&[1_u8; 1000]).await.unwrap();
    outro.reset(41).unwrap();
    let cortado = tokio::time::timeout(Duration::from_secs(5), recebe_outro.read_to_end(4096))
        .await
        .expect("o corte esperou crédito");
    assert_eq!(cortado, Err(FalhaDoCanal::Cortado(41)));
    assert_eq!(par.anfitriao.contadores().cortes_por_fila, 0);
    assert!(par.anfitriao.acabou().is_none() && par.cliente.acabou().is_none());
}

/// Oito canais de 4 MiB ao mesmo tempo, nos dois sentidos: tudo chega, sem
/// corte; e com quem lê parado, nenhum canal passa da janela de 1 MiB.
#[tokio::test]
async fn varios_canais_ao_mesmo_tempo_sem_estouro_nem_perda() {
    let par = par_em_loopback().await;
    let por_canal = 4 * 1024 * 1024;
    for (escreve, le) in [
        (&par.cliente, &par.anfitriao),
        (&par.anfitriao, &par.cliente),
    ] {
        let mut escritas = Vec::new();
        for n in 0..8_u8 {
            let mut envio = escreve.open_uni().await.unwrap();
            escritas.push(tokio::spawn(async move {
                let dados: Vec<u8> = (0..por_canal).map(|i| (i as u8) ^ n).collect();
                envio.write_all(&dados).await.unwrap();
                envio.finish().unwrap();
                envio.entregue().await.unwrap();
            }));
        }
        let mut recebes = Vec::new();
        for _ in 0..8 {
            recebes.push(le.accept_uni().await.unwrap());
        }
        // Ninguém lê por um tempo: a soma em espera não passa de 8 janelas.
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(escritas.iter().all(|e| !e.is_finished()));
        let mut leituras = Vec::new();
        for mut recebe in recebes {
            leituras.push(tokio::spawn(async move {
                recebe.read_to_end(por_canal + 1).await.unwrap()
            }));
        }
        let mut vistos = Vec::new();
        for leitura in leituras {
            let recebido = tokio::time::timeout(Duration::from_secs(20), leitura)
                .await
                .expect("um canal parou")
                .unwrap();
            assert_eq!(recebido.len(), por_canal);
            let n = recebido[0];
            assert!(
                recebido
                    .iter()
                    .enumerate()
                    .all(|(i, b)| *b == (i as u8) ^ n),
                "o canal {n} chegou diferente"
            );
            vistos.push(n);
        }
        vistos.sort_unstable();
        assert_eq!(vistos, (0..8).collect::<Vec<u8>>());
        for escrita in escritas {
            tokio::time::timeout(Duration::from_secs(5), escrita)
                .await
                .unwrap()
                .unwrap();
        }
        assert_eq!(le.contadores().cortes_por_fila, 0);
    }
    assert!(par.anfitriao.acabou().is_none() && par.cliente.acabou().is_none());
}

/// Revisão do Codex (o mesmo defeito medido no JS): canais que o par abre,
/// enche e termina, que a aplicação aceita e não lê. O canal sai do SCTP quando o
/// fim chega, mas a fila dele fica guardada; o teto de canais tem de contar
/// essas filas, ou quem abre e termina sem parar guarda memória sem fim.
#[tokio::test]
async fn canais_terminados_sem_leitor_nao_passam_do_teto_de_filas() {
    let par = par_em_loopback().await;
    // Pequeno de propósito: o que se mede aqui é o número de filas; a memória
    // somada é do teste do agregado, logo abaixo.
    let pedaco = 1024;
    let tentativas = seele_enlace::MAX_CANAIS + 16;
    let mut entregues = 0_usize;
    let mut recusados = 0_usize;
    let mut guardados = Vec::new();
    for n in 0..tentativas {
        let Ok(mut envio) = par.cliente.open_uni().await else {
            recusados += 1;
            continue;
        };
        let feito = async {
            envio.write_all(&vec![n as u8; pedaco]).await?;
            envio.finish()?;
            envio.entregue().await
        };
        // O crédito só vem quando a aplicação aceita: aceita junto, e guarda
        // o `Recebe` sem ler. Um canal recusado ao chegar nunca aparece aqui.
        let aceite = tokio::time::timeout(Duration::from_millis(500), par.anfitriao.accept_uni());
        let (feito, aceite) =
            tokio::join!(tokio::time::timeout(Duration::from_secs(5), feito), aceite);
        match (feito, aceite) {
            (Ok(Ok(())), Ok(Ok(recebe))) => {
                entregues += 1;
                guardados.push(recebe);
            }
            (Ok(Err(_)), _) => recusados += 1,
            (Ok(Ok(())), _) => panic!("o canal {n} foi entregue sem ter sido aceito"),
            (Err(_), _) => panic!("o canal {n} ficou pendurado em vez de recusado"),
        }
    }
    // O `ctl` ocupa uma vaga: sobram MAX_CANAIS - 1 filas de entrada.
    assert!(
        entregues < seele_enlace::MAX_CANAIS,
        "{entregues} filas guardadas sem leitor (teto {})",
        seele_enlace::MAX_CANAIS
    );
    assert_eq!(entregues + recusados, tentativas);
    assert!(par.anfitriao.contadores().canais_recusados >= 16);
    // O que foi guardado chega inteiro a quem enfim lê, e esvaziar devolve as vagas.
    for mut recebe in guardados {
        let dados = recebe.read_to_end(pedaco + 1).await.unwrap();
        assert_eq!(dados.len(), pedaco);
    }
    let mut de_novo = par.cliente.open_uni().await.unwrap();
    de_novo.write_all(b"cabe de novo").await.unwrap();
    de_novo.finish().unwrap();
    let mut recebe = tokio::time::timeout(Duration::from_secs(5), par.anfitriao.accept_uni())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recebe.read_to_end(64).await.unwrap(), b"cabe de novo");
    assert!(par.anfitriao.acabou().is_none());
}

/// Decisão do coordenador: um teto agregado de crédito por conexão, igual ao
/// do navegador. Dezesseis canais de 4 MiB, aceitos e com o leitor parado: a
/// soma do que a conexão segura não passa de 8 MiB, quem escreve espera, e
/// depois tudo chega, sem corte.
#[tokio::test]
async fn muitos_canais_com_o_leitor_parado_seguram_no_maximo_o_agregado() {
    let par = par_em_loopback().await;
    let canais = 16;
    let por_canal = 4 * 1024 * 1024;
    let escritos = Arc::new(AtomicUsize::new(0));
    let mut escritas = Vec::new();
    for n in 0..canais {
        let mut envio = par.cliente.open_uni().await.unwrap();
        let escritos = Arc::clone(&escritos);
        escritas.push(tokio::spawn(async move {
            let pedaco = vec![n as u8; 16 * 1024];
            for _ in 0..(por_canal / pedaco.len()) {
                envio.write_all(&pedaco).await.unwrap();
                escritos.fetch_add(pedaco.len(), Ordering::Relaxed);
            }
            envio.finish().unwrap();
            envio.entregue().await.unwrap();
        }));
    }
    let mut recebes = Vec::new();
    for _ in 0..canais {
        recebes.push(par.anfitriao.accept_uni().await.unwrap());
    }
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let teto = seele_enlace::MAX_CREDITO_DA_CONEXAO as u64;
    let segurado = credito_em_aberto(&par.anfitriao);
    assert!(
        segurado <= teto,
        "a conexão segura {segurado} B (teto {teto} B)"
    );
    let folga_local = canais * (seele_enlace::CREDITO_DE_SAIDA + 2 * seele_enlace::MAX_MENSAGEM);
    let aceitos = escritos.load(Ordering::Relaxed);
    assert!(
        aceitos <= seele_enlace::MAX_CREDITO_DA_CONEXAO + folga_local,
        "quem escreve passou {aceitos} B com o leitor parado (agregado {teto} B + {folga_local} B locais)"
    );
    let mut leituras = Vec::new();
    for mut recebe in recebes {
        leituras.push(tokio::spawn(async move {
            recebe.read_to_end(por_canal + 1).await.unwrap()
        }));
    }
    for leitura in leituras {
        let dados = tokio::time::timeout(Duration::from_secs(30), leitura)
            .await
            .expect("um canal ficou sem crédito para sempre")
            .unwrap();
        assert_eq!(dados.len(), por_canal);
        let n = dados[0];
        assert!(dados.iter().all(|b| *b == n));
    }
    for escrita in escritas {
        tokio::time::timeout(Duration::from_secs(5), escrita)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(par.anfitriao.contadores().cortes_por_fila, 0);
    assert!(par.anfitriao.acabou().is_none());
}

/// Item 1 da decisão: um canal que o par abre e a aplicação não aceitou (o
/// servidor só aceita depois da prova D5, dentro da sessão) não recebe crédito
/// de dados nenhum. Quando a aplicação aceita, tudo anda.
#[tokio::test]
async fn um_canal_nao_aceito_nao_recebe_credito_de_dados() {
    let par = par_em_loopback().await;
    let total = 2 * 1024 * 1024;
    let mut envio = par.cliente.open_uni().await.unwrap();
    let escritos = Arc::new(AtomicUsize::new(0));
    let escrita = {
        let escritos = Arc::clone(&escritos);
        tokio::spawn(async move {
            let pedaco = vec![3_u8; 16 * 1024];
            for _ in 0..(total / pedaco.len()) {
                envio.write_all(&pedaco).await.unwrap();
                escritos.fetch_add(pedaco.len(), Ordering::Relaxed);
            }
            envio.finish().unwrap();
            envio.entregue().await.unwrap();
        })
    };
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(credito_em_aberto(&par.anfitriao), 0);
    let aceitos = escritos.load(Ordering::Relaxed);
    assert!(
        aceitos <= seele_enlace::CREDITO_DE_SAIDA,
        "{aceitos} B saíram de quem escreve sem o canal ter sido aceito"
    );
    let mut recebe = par.anfitriao.accept_uni().await.unwrap();
    let dados = tokio::time::timeout(Duration::from_secs(10), recebe.read_to_end(total + 1))
        .await
        .expect("aceitar não destravou o canal")
        .unwrap();
    assert_eq!(dados.len(), total);
    tokio::time::timeout(Duration::from_secs(5), escrita)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(par.anfitriao.contadores().cortes_por_fila, 0);
}

/// Como uma das oito reservas de 1 MiB é solta, nos casos da revisão do
/// crédito agregado.
#[derive(Clone, Copy, Debug)]
enum Soltura {
    /// Quem escreve termina sem dados (FIN vazio), e ninguém lê.
    FimVazio,
    /// Quem escreve corta o canal.
    Corte,
    /// Quem lê larga o `Recebe` com 1 MiB na fila.
    LargarComDados,
    /// Quem lê para (`stop`) com 1 MiB na fila.
    Parar,
    /// Os oito encheram 1 MiB e terminaram (filas órfãs); quem lê esvazia uma.
    EsvaziarFilaOrfa,
}

/// A revisão do crédito agregado (P1): o FIN, o corte e o fechamento soltam a
/// reserva de um canal, e o nono canal, que esperava vaga no agregado, tem de
/// andar. Sem a revisão nessas transições ele esperava para sempre.
async fn o_nono_canal_anda_quando_uma_reserva_e_solta(soltura: Soltura) {
    let par = par_em_loopback().await;
    let mib = 1024 * 1024;
    let cheio = matches!(
        soltura,
        Soltura::LargarComDados | Soltura::Parar | Soltura::EsvaziarFilaOrfa
    );
    let mut envios = Vec::new();
    let mut recebes = Vec::new();
    for _ in 0..8 {
        let mut envio = par.cliente.open_uni().await.unwrap();
        // Um byte só, para o canal chegar ao anfitrião; o resto do MiB, se
        // for o caso, depois do aceite.
        envio.write_all(&[1]).await.unwrap();
        let recebe = tokio::time::timeout(Duration::from_secs(5), par.anfitriao.accept_uni())
            .await
            .unwrap()
            .unwrap();
        envios.push(envio);
        recebes.push(recebe);
    }
    if cheio {
        for envio in &mut envios {
            tokio::time::timeout(
                Duration::from_secs(5),
                envio.write_all(&vec![2_u8; mib - 1]),
            )
            .await
            .expect("a reserva de 1 MiB não foi concedida")
            .unwrap();
        }
        if matches!(soltura, Soltura::EsvaziarFilaOrfa) {
            for envio in &mut envios {
                envio.finish().unwrap();
                tokio::time::timeout(Duration::from_secs(5), envio.entregue())
                    .await
                    .unwrap()
                    .unwrap();
            }
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let teto = seele_enlace::MAX_CREDITO_DA_CONEXAO as u64;
    assert_eq!(
        credito_em_aberto(&par.anfitriao),
        teto,
        "as oito reservas não encheram o agregado"
    );

    // O nono: aceito, mas sem vaga.
    let total = 2 * mib;
    let mut nono = par.cliente.open_uni().await.unwrap();
    let escritos = Arc::new(AtomicUsize::new(0));
    let escrita = {
        let escritos = Arc::clone(&escritos);
        tokio::spawn(async move {
            let pedaco = vec![9_u8; 16 * 1024];
            for _ in 0..(total / pedaco.len()) {
                nono.write_all(&pedaco).await.unwrap();
                escritos.fetch_add(pedaco.len(), Ordering::Relaxed);
            }
            nono.finish().unwrap();
            nono.entregue().await.unwrap();
        })
    };
    let mut recebe_nono = tokio::time::timeout(Duration::from_secs(5), par.anfitriao.accept_uni())
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        escritos.load(Ordering::Relaxed) <= seele_enlace::CREDITO_DE_SAIDA,
        "o nono escreveu sem vaga no agregado"
    );

    // Solta uma reserva, sem ler nem aceitar mais nada.
    let mut envio = envios.remove(0);
    let mut recebe = recebes.remove(0);
    match soltura {
        Soltura::FimVazio => envio.finish().unwrap(),
        Soltura::Corte => envio.reset(7).unwrap(),
        Soltura::LargarComDados => drop(recebe),
        Soltura::Parar => recebe.stop(5).unwrap(),
        Soltura::EsvaziarFilaOrfa => {
            let dados = recebe.read_to_end(mib + 1).await.unwrap();
            assert_eq!(dados.len(), mib);
        }
    }
    let recebido =
        tokio::time::timeout(Duration::from_secs(10), recebe_nono.read_to_end(total + 1))
            .await
            .unwrap_or_else(|_| panic!("{soltura:?}: o nono canal esperou para sempre"))
            .unwrap();
    assert_eq!(recebido.len(), total);
    tokio::time::timeout(Duration::from_secs(5), escrita)
        .await
        .unwrap()
        .unwrap();
    assert!(credito_em_aberto(&par.anfitriao) <= teto);
    assert_eq!(par.anfitriao.contadores().cortes_por_fila, 0);
    drop((envios, recebes, envio));
}

#[tokio::test]
async fn o_fim_vazio_solta_a_reserva_para_o_nono_canal() {
    o_nono_canal_anda_quando_uma_reserva_e_solta(Soltura::FimVazio).await;
}

#[tokio::test]
async fn o_corte_solta_a_reserva_para_o_nono_canal() {
    o_nono_canal_anda_quando_uma_reserva_e_solta(Soltura::Corte).await;
}

#[tokio::test]
async fn largar_o_recebe_cheio_solta_a_reserva_para_o_nono_canal() {
    o_nono_canal_anda_quando_uma_reserva_e_solta(Soltura::LargarComDados).await;
}

#[tokio::test]
async fn parar_a_leitura_cheia_solta_a_reserva_para_o_nono_canal() {
    o_nono_canal_anda_quando_uma_reserva_e_solta(Soltura::Parar).await;
}

#[tokio::test]
async fn esvaziar_uma_fila_orfa_solta_a_reserva_para_o_nono_canal() {
    o_nono_canal_anda_quando_uma_reserva_e_solta(Soltura::EsvaziarFilaOrfa).await;
}
