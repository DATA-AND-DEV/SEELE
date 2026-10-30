//! Os canais do túnel de jogo (API 8, parte 2): o `t` (ordenado e confiável, o
//! TCP e o controle do túnel) e o `j` (sem ordem e sem retransmissão, o UDP).
//! Em loopback de verdade: ICE, DTLS e SCTP inteiros.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::time::Duration;

use seele_enlace::teste::{configuracao_do_jogo, par_em_loopback};
use seele_enlace::{
    CanalDeTunel, Confiabilidade, Motivo, Recusa, FILA_DE_JOGO, MAX_CANAIS, MAX_DADOS,
};
use tokio::io::AsyncWriteExt;

const PRAZO: Duration = Duration::from_secs(10);

#[tokio::test]
async fn o_canal_t_leva_bytes_em_ordem_e_o_fim() {
    let par = par_em_loopback().await;
    // `_leitura` vive até o fim: largar o `Recebe` cortaria o canal.
    let CanalDeTunel::Ordenado(mut envio, _leitura) = par
        .cliente
        .abrir_tunel(Confiabilidade::Ordenada)
        .await
        .unwrap()
    else {
        panic!("a confiabilidade Ordenada abriu um canal sem ordem");
    };
    let dados: Vec<u8> = (0..1024 * 1024_u32).map(|i| (i % 251) as u8).collect();
    let escrita = {
        let dados = dados.clone();
        tokio::spawn(async move {
            // Em pedaços, como o repasse de um socket TCP escreve.
            for pedaco in dados.chunks(5000) {
                envio.write_all(pedaco).await.unwrap();
            }
            envio.shutdown().await.unwrap();
        })
    };
    let CanalDeTunel::Ordenado(_, mut recebe) =
        tokio::time::timeout(PRAZO, par.anfitriao.aceitar_tunel())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("o t chegou como canal sem ordem");
    };
    let lidos = tokio::time::timeout(PRAZO, recebe.read_to_end(2 * 1024 * 1024))
        .await
        .unwrap()
        .unwrap();
    escrita.await.unwrap();
    assert_eq!(lidos.len(), dados.len());
    assert!(
        lidos == dados,
        "os bytes chegaram fora de ordem ou trocados"
    );
}

#[tokio::test]
async fn o_canal_j_leva_datagramas_e_conta_o_que_nao_coube() {
    let par = par_em_loopback().await;
    let CanalDeTunel::SemOrdem(de_quem_joga) = par
        .cliente
        .abrir_tunel(Confiabilidade::SemOrdem)
        .await
        .unwrap()
    else {
        panic!("a confiabilidade SemOrdem abriu um canal ordenado");
    };
    let CanalDeTunel::SemOrdem(mut da_maquina) =
        tokio::time::timeout(PRAZO, par.anfitriao.aceitar_tunel())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("o j chegou como canal ordenado");
    };

    // O guarda do fio: o que o OUTRO lado leu da configuração do canal. Um j
    // que nascesse confiável ou ordenado passaria nos datagramas abaixo (em
    // loopback nada se perde), e viraria fila atrás de fila na rede de verdade.
    let lida = configuracao_do_jogo(&da_maquina);
    assert!(!lida.ordenado, "o j chegou ordenado");
    assert!(
        lida.sem_retransmissao,
        "o j chegou com retransmissão: um datagrama atrasado trava os de trás"
    );

    // 100 datagramas de 1200 B: a ordem não é exigida, o número que chega é.
    for n in 0..100_u8 {
        let mut d = vec![n; 1200];
        d[0] = n;
        match de_quem_joga.mandar(&d) {
            Ok(()) => {}
            Err(Recusa::Cheia) => {
                // A fila é pequena de propósito: dá a vez à tarefa do enlace.
                tokio::time::sleep(Duration::from_millis(5)).await;
                de_quem_joga.mandar(&d).unwrap();
            }
            Err(outra) => panic!("o datagrama {n} foi recusado: {outra}"),
        }
    }
    let mut vistos = [false; 100];
    for _ in 0..100 {
        let d = tokio::time::timeout(PRAZO, da_maquina.receber())
            .await
            .expect("faltou datagrama em loopback")
            .expect("o canal acabou");
        assert_eq!(d.len(), 1200);
        vistos[usize::from(d[0])] = true;
    }
    assert!(vistos.iter().all(|v| *v), "algum datagrama não chegou");
    assert_eq!(par.cliente.contadores().tunel_descartado, 0);
    assert_eq!(par.cliente.contadores().tunel_grande_demais, 0);

    // Um acima do teto: Grande, e contado em quem tentou mandar.
    assert_eq!(
        de_quem_joga.mandar(&vec![0_u8; MAX_DADOS + 1]),
        Err(Recusa::Grande)
    );
    assert_eq!(par.cliente.contadores().tunel_grande_demais, 1);
    // No teto exato cabe.
    de_quem_joga.mandar(&vec![9_u8; MAX_DADOS]).unwrap();
    let d = tokio::time::timeout(PRAZO, da_maquina.receber())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(d.len(), MAX_DADOS);
}

#[tokio::test]
async fn o_j_nunca_espera_e_conta_a_fila_cheia() {
    let par = par_em_loopback().await;
    let CanalDeTunel::SemOrdem(d) = par
        .cliente
        .abrir_tunel(Confiabilidade::SemOrdem)
        .await
        .unwrap()
    else {
        panic!("esperava um j");
    };
    // Sem ceder a vez nenhuma vez (runtime de uma thread): a tarefa do enlace
    // não esvazia a fila no meio do laço, então o número que cabe é exato.
    let tentativas = FILA_DE_JOGO + 50;
    let mut cheias = 0_u64;
    for _ in 0..tentativas {
        match d.mandar(&[1_u8; 100]) {
            Ok(()) => {}
            Err(Recusa::Cheia) => cheias += 1,
            Err(outra) => panic!("recusa inesperada: {outra}"),
        }
    }
    assert_eq!(cheias, 50, "a fila de saída do j não tem o tamanho dito");
    assert_eq!(par.cliente.contadores().tunel_descartado, 50);
}

#[tokio::test]
async fn um_rotulo_de_tunel_nao_vira_pedido_de_imagem() {
    let par = par_em_loopback().await;
    let _t = par
        .cliente
        .abrir_tunel(Confiabilidade::Ordenada)
        .await
        .unwrap();
    let _j = par
        .cliente
        .abrir_tunel(Confiabilidade::SemOrdem)
        .await
        .unwrap();
    // Os dois chegam como túnel...
    let primeiro = tokio::time::timeout(PRAZO, par.anfitriao.aceitar_tunel())
        .await
        .unwrap()
        .unwrap();
    let segundo = tokio::time::timeout(PRAZO, par.anfitriao.aceitar_tunel())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        (&primeiro, &segundo),
        (CanalDeTunel::Ordenado(..), CanalDeTunel::SemOrdem(_))
    ));
    // ...e nenhum cai nas filas das imagens (b) nem dos anexos (u).
    assert!(
        tokio::time::timeout(Duration::from_millis(300), par.anfitriao.accept_bi())
            .await
            .is_err(),
        "um canal de túnel apareceu como pedido de imagem"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), par.anfitriao.accept_uni())
            .await
            .is_err(),
        "um canal de túnel apareceu como fluxo unidirecional"
    );
    assert_eq!(par.anfitriao.contadores().canais_recusados, 0);
}

/// A medida pedida pelo Task 4: `MAX_CANAIS` é teto de canais vivos por
/// conexão, e os de túnel contam nele junto com `u`, `b`, o `ctl` e a `voz`.
#[tokio::test]
async fn os_canais_de_tunel_contam_no_teto_e_quem_abre_recebe_canais_esgotados() {
    let par = par_em_loopback().await;
    // O ctl e a voz ocupam duas vagas; dez u e o resto de túnel enchem as demais.
    let mut guardados = Vec::new();
    for _ in 0..10 {
        guardados.push(Box::new(par.cliente.open_uni().await.unwrap()) as Box<dyn std::any::Any>);
    }
    let mut abertos = 10;
    loop {
        let confiabilidade = if abertos % 2 == 0 {
            Confiabilidade::Ordenada
        } else {
            Confiabilidade::SemOrdem
        };
        match par.cliente.abrir_tunel(confiabilidade).await {
            Ok(canal) => {
                guardados.push(Box::new(canal) as Box<dyn std::any::Any>);
                abertos += 1;
            }
            Err(motivo) => {
                assert_eq!(motivo, Motivo::CanaisEsgotados);
                break;
            }
        }
        assert!(abertos <= MAX_CANAIS, "passou do teto sem ser recusado");
    }
    assert_eq!(abertos, MAX_CANAIS - 2, "ctl e voz ocupam duas vagas");
    // Ainda é só uma recusa: a conexão segue de pé.
    assert!(par.cliente.acabou().is_none());
}

/// Largar o `j` o fecha dos dois lados: um túnel que
/// acaba não pode deixar o canal ocupando uma das 62 vagas.
#[tokio::test]
async fn largar_o_j_fecha_o_canal_no_par() {
    let par = par_em_loopback().await;
    let CanalDeTunel::SemOrdem(meu) = par
        .cliente
        .abrir_tunel(Confiabilidade::SemOrdem)
        .await
        .unwrap()
    else {
        panic!("esperava um j");
    };
    let CanalDeTunel::SemOrdem(mut dele) = par.anfitriao.aceitar_tunel().await.unwrap() else {
        panic!("esperava um j");
    };
    drop(meu);
    let fim = tokio::time::timeout(PRAZO, dele.receber()).await.unwrap();
    assert!(fim.is_none(), "o par continuou vendo o j depois de largado");
    assert_eq!(dele.mandar(b"x"), Err(Recusa::Fechada));
    // Nenhum datagrama de envelope vazou como dado: nada foi contado.
    assert_eq!(par.anfitriao.contadores().violacoes, 0);
}
