//! A mídia RTP do enlace (ADR 0067): vídeo e som entre duas pontas de verdade,
//! sem SDP, pelo cano com perda.
//!
//! É a prova da etapa 1 do ADR: a API direta do `str0m` leva quadros inteiros
//! de uma `Ponta` a outra, com retransmissão, pedido de quadro-chave e
//! estimativa de banda, convivendo com os canais SCTP da mesma conexão.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    missing_docs
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_enlace::teste::{par_em_loopback, par_por_cano, ConfigDoCano, Sentido};
use seele_enlace::{EventoDeMidia, Midia, Trilha, RELOGIO_DO_VIDEO};

const QUADROS_POR_SEGUNDO: u64 = 30;

/// A faixa dos testes: não a primeira, para provar que a faixa viaja.
const FAIXA: u8 = 2;

/// Um quadro com forma de H.264 em Annex B: o prefixo, o cabeçalho da NAL
/// (IDR ou fatia comum), o número do quadro em ASCII e enchimento que nunca
/// forma um prefixo de início.
fn quadro(numero: u64, chave: bool, tamanho: usize) -> Arc<[u8]> {
    let mut bytes = vec![0, 0, 0, 1, if chave { 0x65 } else { 0x41 }];
    bytes.extend_from_slice(format!("{numero:016}").as_bytes());
    bytes.resize(tamanho.max(bytes.len()), 0x5A);
    bytes.into()
}

/// O número que [`quadro`] escreveu, lido do que chegou. `None` se o quadro
/// não é um dos nossos, inteiro.
fn numero_de(bytes: &[u8], tamanho: usize) -> Option<(u64, bool)> {
    let inicio = bytes.windows(3).position(|j| j == [0, 0, 1])? + 3;
    let chave = match bytes.get(inicio)? {
        0x65 => true,
        0x41 => false,
        _ => return None,
    };
    let texto = bytes.get(inicio + 1..inicio + 17)?;
    let numero = std::str::from_utf8(texto).ok()?.parse().ok()?;
    // O quadro tem de chegar do tamanho que saiu (o prefixo pode ter 3 ou 4 bytes).
    let corpo = bytes.len() - inicio;
    (corpo == tamanho - 4).then_some((numero, chave))
}

async fn proximo(midia: &mut Midia, prazo: Duration) -> Option<EventoDeMidia> {
    tokio::time::timeout(prazo, midia.receber())
        .await
        .ok()
        .flatten()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn o_video_atravessa_tres_por_cento_de_perda_sem_buraco() {
    let sentido = Sentido {
        perda: 0.03,
        atraso: Duration::from_millis(30),
        ..Sentido::limpo()
    };
    let (par, cano) = par_por_cano(ConfigDoCano::simetrico(sentido, 7)).await;
    let mut enc = par.ponta_do_anfitriao.mensagens_enc().unwrap();
    let emissor = par.cliente.ligar_midia().remetente();
    let mut receptor = par.anfitriao.ligar_midia();
    emissor.taxa_desejada(2_000_000);

    const QUADROS: u64 = 150;
    const CHAVE: usize = 40_000;
    const COMUM: usize = 5_000;
    let mandar = tokio::spawn(async move {
        let mut relogio = tokio::time::interval(Duration::from_millis(1000 / QUADROS_POR_SEGUNDO));
        for n in 0..QUADROS {
            relogio.tick().await;
            let chave = n == 0;
            let tempo = n * u64::from(RELOGIO_DO_VIDEO) / QUADROS_POR_SEGUNDO;
            let tamanho = if chave { CHAVE } else { COMUM };
            emissor
                .mandar(FAIXA, Trilha::Video, tempo, quadro(n, chave, tamanho))
                .unwrap();
        }
        // Segura o punho: a estimativa é lida depois.
        tokio::time::sleep(Duration::from_secs(2)).await;
        emissor
    });

    let mut vistos = Vec::new();
    let mut quebras = 0;
    let fim = Instant::now() + Duration::from_secs(20);
    while vistos.len() < QUADROS as usize && Instant::now() < fim {
        let Some(evento) = proximo(&mut receptor, Duration::from_secs(3)).await else {
            break;
        };
        if let EventoDeMidia::Quadro {
            faixa: FAIXA,
            trilha: Trilha::Video,
            contiguo,
            bytes,
            ..
        } = evento
        {
            let tamanho = if vistos.is_empty() { CHAVE } else { COMUM };
            let (numero, _) = numero_de(&bytes, tamanho).expect("um quadro inteiro, dos nossos");
            if !contiguo && !vistos.is_empty() {
                quebras += 1;
            }
            vistos.push(numero);
        }
    }
    let emissor = mandar.await.unwrap();

    // A perda foi de verdade, e a retransmissão a cobriu: todos os quadros, na
    // ordem, sem um buraco. No canal confiável isto também chega inteiro; o
    // que muda é o atraso, e esse é da medida, não deste teste.
    let perdidos = cano.ida().perdidos_no_sorteio;
    assert!(
        perdidos > 0,
        "o cano não perdeu nada: o teste não provou nada"
    );
    assert_eq!(
        vistos,
        (0..QUADROS).collect::<Vec<_>>(),
        "com {perdidos} datagramas perdidos"
    );
    assert_eq!(quebras, 0);

    // A estimativa de banda existe: o retorno de TWCC chegou a quem manda.
    assert!(
        emissor.estimativa_bps().is_some(),
        "nenhuma estimativa de banda depois de 5 s de vídeo"
    );
    // E nenhum pacote de mídia foi parar na fila do ponto de encontro.
    assert!(enc.try_recv().is_err());
    assert_eq!(par.ponta_do_anfitriao.contadores().midia_sem_dono, 0);
    assert_eq!(par.cliente.contadores().midia_nao_saiu, 0);
}

/// O que as duas pontas leram da perda enquanto um vídeo atravessava `perda`:
/// a maior fração de saída vista por quem manda e a maior de entrada vista por
/// quem recebe (`None`: nunca houve medida).
async fn perdas_lidas(perda: f64, semente: u64) -> (Option<f32>, Option<f32>) {
    let sentido = Sentido {
        perda,
        atraso: Duration::from_millis(10),
        ..Sentido::limpo()
    };
    let (par, _cano) = par_por_cano(ConfigDoCano::simetrico(sentido, semente)).await;
    let emissor = par.cliente.ligar_midia().remetente();
    let mut receptor = par.anfitriao.ligar_midia();
    emissor.taxa_desejada(2_000_000);
    let consumir = tokio::spawn(async move { while receptor.receber().await.is_some() {} });

    let maior = |atual: Option<f32>, lida: Option<f32>| match (atual, lida) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let (mut saida, mut entrada) = (None, None);
    let mut relogio = tokio::time::interval(Duration::from_millis(1000 / QUADROS_POR_SEGUNDO));
    for n in 0..(6 * QUADROS_POR_SEGUNDO) {
        relogio.tick().await;
        let tempo = n * u64::from(RELOGIO_DO_VIDEO) / QUADROS_POR_SEGUNDO;
        emissor
            .mandar(FAIXA, Trilha::Video, tempo, quadro(n, n == 0, 5_000))
            .unwrap();
        saida = maior(saida, par.cliente.leitura().fracao_de_perda_na_saida);
        entrada = maior(entrada, par.anfitriao.leitura().fracao_de_perda_na_entrada);
    }
    consumir.abort();
    (saida, entrada)
}

/// **A perda do caminho é um número nas duas pontas**, e não só em quem manda.
///
/// A de saída já era lida (o retorno de TWCC); a de entrada o str0m também
/// conta e ninguém a tirava dele. É ela que diz ao anfitrião quanto do vídeo
/// de quem **transmite** se perde a caminho — e quem transmite não recebe RTP
/// nenhum do anfitrião, então a de saída dele fica em `None` para sempre.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_perda_do_caminho_aparece_na_leitura_das_duas_pontas() {
    let (saida, entrada) = perdas_lidas(0.08, 21).await;
    println!("8% de perda: saída {saida:?}, entrada {entrada:?}");
    let entrada = entrada.expect("quem recebe RTP não mediu perda de entrada nenhuma");
    assert!(
        (0.03..0.25).contains(&entrada),
        "8% de perda no caminho e a entrada leu {entrada}"
    );
    let saida = saida.expect("quem manda RTP não mediu perda de saída nenhuma");
    assert!(
        (0.03..0.25).contains(&saida),
        "8% de perda no caminho e a saída leu {saida}"
    );

    let (saida, entrada) = perdas_lidas(0.0, 22).await;
    println!("sem perda: saída {saida:?}, entrada {entrada:?}");
    assert!(
        entrada.is_some_and(|f| f < 0.01),
        "caminho limpo e a entrada leu {entrada:?}"
    );
    assert!(
        saida.is_some_and(|f| f < 0.01),
        "caminho limpo e a saída leu {saida:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn o_pedido_de_quadro_chave_chega_a_quem_manda_e_o_som_passa() {
    let par = par_em_loopback().await;
    let mut de_ca = par.cliente.ligar_midia();
    let mut de_la = par.anfitriao.ligar_midia();
    let para_la = de_ca.remetente();
    let para_ca = de_la.remetente();

    // O anfitrião manda (o sentido contrário ao do outro teste): vídeo e som.
    para_ca
        .mandar(FAIXA, Trilha::Video, 0, quadro(0, true, 3_000))
        .unwrap();
    let som: Arc<[u8]> = vec![0xF8, 1, 2, 3, 4, 5, 6, 7].into();
    para_ca
        .mandar(FAIXA, Trilha::Som, 960, Arc::clone(&som))
        .unwrap();

    let mut video = false;
    let mut ouviu = false;
    while !(video && ouviu) {
        match proximo(&mut de_ca, Duration::from_secs(5))
            .await
            .expect("a mídia do anfitrião")
        {
            EventoDeMidia::Quadro {
                faixa: FAIXA,
                trilha: Trilha::Video,
                bytes,
                ..
            } => {
                assert_eq!(numero_de(&bytes, 3_000), Some((0, true)));
                video = true;
            }
            EventoDeMidia::Quadro {
                faixa: FAIXA,
                trilha: Trilha::Som,
                tempo,
                bytes,
                ..
            } => {
                assert_eq!(&*bytes, &*som);
                assert_eq!(tempo, 960);
                ouviu = true;
            }
            outro => panic!("inesperado: {outro:?}"),
        }
    }

    // Quem assiste pede um quadro-chave: quem manda fica sabendo.
    para_la.pedir_chave(FAIXA);
    loop {
        if proximo(&mut de_la, Duration::from_secs(5))
            .await
            .expect("o pedido de quadro-chave")
            == (EventoDeMidia::PediramChave { faixa: FAIXA })
        {
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quem_nao_ligou_a_midia_larga_o_pacote_e_conta() {
    let par = par_em_loopback().await;
    let mut enc = par.ponta_do_anfitriao.mensagens_enc().unwrap();
    // Só quem liga declarou a mídia: o anfitrião é um par que não fala RTP.
    let emissor = par.cliente.ligar_midia().remetente();
    for n in 0..5 {
        emissor
            .mandar(FAIXA, Trilha::Video, n * 3000, quadro(n, n == 0, 2_000))
            .unwrap();
    }
    let fim = Instant::now() + Duration::from_secs(5);
    while par.ponta_do_anfitriao.contadores().midia_sem_dono == 0 {
        assert!(Instant::now() < fim, "o pacote de mídia não foi contado");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Não foi ao ponto de encontro, e a conexão segue de pé.
    assert!(enc.try_recv().is_err());
    let (mut envio, _) = par.cliente.controle().unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut envio, b"ainda aqui")
        .await
        .unwrap();
}

/// **Um quadro que não é H.264 não derruba a conexão.** O vídeo que o
/// servidor repassa vem de um par: lixo num quadro tem de custar aquele
/// quadro, e nunca a voz e o controle de quem assiste.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn um_quadro_que_nao_e_h264_nao_derruba_a_conexao() {
    let par = par_em_loopback().await;
    let mut de_la = par.anfitriao.ligar_midia();
    let para_la = par.cliente.ligar_midia().remetente();

    // Sem prefixo de início, só zeros, um prefixo sem NAL, e um corpo vazio
    // depois do prefixo.
    let lixos: [&[u8]; 5] = [
        &[0xAB; 64],
        &[0; 64],
        &[0, 0, 1],
        &[0, 0, 0, 1],
        &[0, 0, 0, 1, 0x65, 0, 0, 1],
    ];
    for (n, lixo) in lixos.iter().enumerate() {
        let _ = para_la.mandar(FAIXA, Trilha::Video, n as u64 * 3000, (*lixo).into());
    }
    // Depois do lixo, um quadro de verdade ainda chega…
    para_la
        .mandar(FAIXA, Trilha::Video, 30_000, quadro(9, true, 2_000))
        .unwrap();
    loop {
        let evento = proximo(&mut de_la, Duration::from_secs(5))
            .await
            .expect("a mídia depois do lixo");
        if let EventoDeMidia::Quadro { bytes, .. } = evento {
            if numero_de(&bytes, 2_000) == Some((9, true)) {
                break;
            }
        }
    }
    // …e a conexão segue de pé nos dois lados.
    let (mut envio, _) = par.cliente.controle().unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut envio, b"ainda aqui")
        .await
        .unwrap();
    // Cada lixo foi recusado antes de chegar ao empacotador, e contado.
    assert_eq!(par.cliente.contadores().midia_nao_saiu, lixos.len() as u64);
}

/// A forma do Annex B, caso a caso: é ela que decide o que entra no
/// empacotador.
#[test]
fn a_forma_do_annex_b() {
    use seele_enlace::annex_b_valido;
    assert!(annex_b_valido(&[0, 0, 1, 0x65, 1, 2]));
    assert!(annex_b_valido(&[
        0, 0, 0, 1, 0x67, 9, 0, 0, 1, 0x68, 8, 0, 0, 0, 1, 0x65, 7
    ]));
    // Um zero no fim da última NAL é dela (não há prefixo depois).
    assert!(annex_b_valido(&[0, 0, 1, 0x41, 0]));
    assert!(!annex_b_valido(&[]));
    assert!(!annex_b_valido(&[0x65, 1, 2]));
    assert!(!annex_b_valido(&[0, 0, 1]));
    assert!(!annex_b_valido(&[0, 0, 0, 1]));
    assert!(!annex_b_valido(&[0, 0, 1, 0x65, 0, 0, 1]));
    assert!(!annex_b_valido(&[0, 0, 1, 0, 0, 1, 0x65]));
    assert!(!annex_b_valido(&[0; 64]));
}
