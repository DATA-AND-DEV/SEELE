//! O cano de `seele_enlace::teste`: o emulador de rede dos testes do transporte.
//!
//! Duas camadas. A de baixo usa sockets UDP crus, sem o enlace: ali a ordem dos
//! datagramas é a do teste, e a perda, o atraso e a banda se conferem com conta
//! exata. A de cima passa um par de verdade (ICE, DTLS, SCTP) pelo cano: prova
//! que o ICE fecha por ele e que o que o enlace vê bate com o que foi pedido.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use seele_enlace::teste::{par_por_cano, Cano, ConfigDoCano, Sentido};
use seele_enlace::{CanalDeTunel, Confiabilidade, Datagramas};
use tokio::net::UdpSocket;

/// Largo de propósito: com perda, o aperto depende de retransmissões do DTLS e
/// do INIT do SCTP (RTO inicial de 3 s, dobrando).
const PRAZO: Duration = Duration::from_secs(60);

/// Um socket das pontas do teste, com o mesmo buffer de recepção que o cano
/// pede para os dele.
///
/// O destino é quem recebe a rajada que o cano repassa, e com o buffer padrão
/// (64 KiB no Windows) ele perderia depois do cano o que o cano deixou de
/// perder antes: o teste contaria como perda do caminho um transbordo da
/// própria ponta.
async fn socket() -> UdpSocket {
    let socket = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    // O sistema pode dar menos (o teto é dele), e o teste roda assim mesmo.
    let _ = socket2::SockRef::from(&socket).set_recv_buffer_size(seele_enlace::BUFFER_DE_RECEPCAO);
    socket
}

/// Manda `quantos` datagramas numerados de `tamanho` bytes pela ida de um cano
/// cru e devolve (o índice, quanto demorou) de cada um que chegou, na ordem de
/// chegada, mais o cano.
async fn pela_ida(
    config: ConfigDoCano,
    quantos: u32,
    tamanho: usize,
    intervalo: Duration,
) -> (Vec<(u32, Duration)>, Cano) {
    let (demoras, cano, _emissao) =
        pela_ida_cronometrada(config, quantos, tamanho, intervalo).await;
    (demoras, cano)
}

/// [`pela_ida`], e mais **quanto o emissor levou** para mandar tudo: é o que
/// diz a um teste o que foi de fato oferecido ao cano, antes de ele julgar o
/// que o cano fez com isso.
///
/// # O emissor anda por prazo, e não por espera
///
/// O n-ésimo datagrama sai em `comeco + intervalo × n`, e não «`intervalo`
/// depois do anterior». Com `sleep(intervalo)` o atraso de cada despertar se
/// **soma**: um relógio que acorda 10–20 ms depois do pedido (o macOS do runner)
/// ou só a cada 15,6 ms (o tique do Windows) fazia um emissor de «1000 bytes a
/// cada 4 ms» oferecer um quarto dos 2 Mbps que o teste supunha, e o gargalo de
/// 1 Mbps nunca enchia — o teste da banda reprovava o cano por um emissor que
/// não emitia (CI de 12/11/2026: 53 descartes onde se esperavam mais de 100 no
/// macOS, 622 kbps «passando» por 1 Mbps no Windows). Por prazo, quem acorda
/// tarde manda de uma vez o que já venceu, e a média oferecida é a pedida.
async fn pela_ida_cronometrada(
    config: ConfigDoCano,
    quantos: u32,
    tamanho: usize,
    intervalo: Duration,
) -> (Vec<(u32, Duration)>, Cano, Duration) {
    let destino = socket().await;
    let origem = socket().await;
    let cano = Cano::abrir(destino.local_addr().unwrap(), config)
        .await
        .unwrap();
    let alvo = cano.endereco();
    let comeco = Instant::now();
    let emissor = tokio::spawn(async move {
        let mut saidas = Vec::new();
        for n in 0..quantos {
            let mut d = vec![0_u8; tamanho];
            d[..4].copy_from_slice(&n.to_be_bytes());
            saidas.push(comeco.elapsed());
            origem.send_to(&d, alvo).await.unwrap();
            if intervalo.is_zero() {
                // Em rajada, mas sem estourar o buffer do socket do cano.
                if n % 32 == 31 {
                    tokio::task::yield_now().await;
                }
            } else {
                // O prazo do próximo, contado do começo: ver o comentário da função.
                let proximo = tokio::time::Instant::from_std(comeco) + intervalo * (n + 1);
                tokio::time::sleep_until(proximo).await;
            }
        }
        (saidas, origem, comeco.elapsed())
    });
    let mut chegadas = Vec::new();
    let mut buffer = vec![0_u8; 2048];
    // Até 500 ms de silêncio depois de o emissor acabar.
    loop {
        let fim = emissor.is_finished();
        match tokio::time::timeout(Duration::from_millis(500), destino.recv_from(&mut buffer)).await
        {
            Ok(Ok((n, _))) => {
                assert_eq!(n, tamanho, "o cano mudou o tamanho do datagrama");
                let indice = u32::from_be_bytes(buffer[..4].try_into().unwrap());
                chegadas.push((indice, comeco.elapsed()));
            }
            Ok(Err(erro)) => panic!("o destino falhou: {erro}"),
            Err(_) if fim => break,
            Err(_) => {}
        }
    }
    let (saidas, _origem, emissao) = emissor.await.unwrap();
    let demoras = chegadas
        .into_iter()
        .map(|(n, chegou)| (n, chegou.saturating_sub(saidas[n as usize])))
        .collect();
    (demoras, cano, emissao)
}

fn com_perda(perda: f64) -> Sentido {
    Sentido {
        perda,
        ..Sentido::limpo()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cru_sem_perda_nada_se_perde_e_a_ordem_fica() {
    let (chegou, cano) = pela_ida(
        ConfigDoCano::simetrico(Sentido::limpo(), 7),
        2000,
        200,
        Duration::ZERO,
    )
    .await;
    let indices: Vec<u32> = chegou.iter().map(|(n, _)| *n).collect();
    assert_eq!(indices, (0..2000).collect::<Vec<_>>());
    let ida = cano.ida();
    assert_eq!(ida.repassados, 2000);
    assert_eq!(ida.bytes_repassados, 2000 * 200);
    assert_eq!(ida.descartados(), 0);
    assert_eq!(cano.volta().repassados, 0, "nada voltou, nada se conta");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cru_a_perda_de_20_por_cento_fica_na_faixa_e_a_semente_decide_quais() {
    let rodada = |semente| async move {
        let (chegou, cano) = pela_ida(
            ConfigDoCano::simetrico(com_perda(0.20), semente),
            2000,
            200,
            Duration::ZERO,
        )
        .await;
        let indices: Vec<u32> = chegou.iter().map(|(n, _)| *n).collect();
        (indices, cano.ida(), cano.destinos_da_ida())
    };
    let (primeira, contagem, destinos) = rodada(42).await;
    let (segunda, _, destinos_de_novo) = rodada(42).await;
    let (outra, _, _) = rodada(43).await;

    // A conta fecha: o que não chegou foi o sorteio que tirou, e só ele.
    assert_eq!(contagem.repassados as usize, primeira.len());
    assert_eq!(contagem.repassados + contagem.perdidos_no_sorteio, 2000);
    assert_eq!(contagem.descartados_na_fila, 0);
    // 20% de 2000 é 400, desvio padrão de 18: a faixa é de mais de quatro desvios.
    let perdidos = 2000 - primeira.len();
    assert!(
        (320..=480).contains(&perdidos),
        "perdeu {perdidos} de 2000 com 20% pedidos"
    );
    // A mesma semente perde exatamente os mesmos; outra semente, outros.
    assert_eq!(
        primeira, segunda,
        "a mesma semente perdeu datagramas diferentes"
    );
    assert_eq!(destinos, destinos_de_novo);
    assert_eq!(destinos.len(), 2000);
    assert_ne!(
        primeira, outra,
        "outra semente perdeu os mesmos: a semente não manda"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cru_o_atraso_e_o_jitter_seguram_cada_datagrama_sem_trocar_a_ordem() {
    let fixo = Sentido {
        atraso: Duration::from_millis(50),
        ..Sentido::limpo()
    };
    let (chegou, _cano) = pela_ida(
        ConfigDoCano::simetrico(fixo, 1),
        100,
        200,
        Duration::from_millis(2),
    )
    .await;
    assert_eq!(chegou.len(), 100);
    for (n, demora) in &chegou {
        assert!(
            *demora >= Duration::from_millis(50) && *demora < Duration::from_millis(75),
            "o datagrama {n} demorou {demora:?} com 50 ms pedidos"
        );
    }

    let com_jitter = Sentido {
        atraso: Duration::from_millis(50),
        jitter: Duration::from_millis(40),
        ..Sentido::limpo()
    };
    let (chegou, _cano) = pela_ida(
        ConfigDoCano::simetrico(com_jitter, 1),
        200,
        200,
        Duration::from_millis(2),
    )
    .await;
    let indices: Vec<u32> = chegou.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        indices,
        (0..200).collect::<Vec<_>>(),
        "o jitter trocou a ordem"
    );
    let demoras: Vec<Duration> = chegou.iter().map(|(_, d)| *d).collect();
    let menor = demoras.iter().min().unwrap();
    let maior = demoras.iter().max().unwrap();
    assert!(*menor >= Duration::from_millis(50), "menor {menor:?}");
    // 50 + 40 de jitter, mais a folga de não trocar a ordem e do temporizador.
    assert!(*maior < Duration::from_millis(115), "maior {maior:?}");
    assert!(
        *maior - *menor >= Duration::from_millis(15),
        "o jitter de 40 ms não espalhou as demoras: {menor:?} a {maior:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cru_a_fila_do_gargalo_descarta_pela_cauda() {
    // 1 Mbps com fila de 10 000 bytes, e uma rajada de 100 datagramas de 1000:
    // cabem os 10 primeiros, e os 90 de trás caem.
    let estreito = Sentido {
        banda_bps: Some(1_000_000),
        fila_max: 10_000,
        ..Sentido::limpo()
    };
    let (chegou, cano) = pela_ida(
        ConfigDoCano::simetrico(estreito, 1),
        100,
        1000,
        Duration::ZERO,
    )
    .await;
    let indices: Vec<u32> = chegou.iter().map(|(n, _)| *n).collect();
    // A rajada leva alguns microssegundos para sair, e nesse tempo o gargalo
    // escoa um pouco: cabe um datagrama a mais, no máximo.
    assert!(
        indices.len() == 10 || indices.len() == 11,
        "passaram {} de uma rajada de 100 numa fila de 10",
        indices.len()
    );
    assert_eq!(
        &indices[..10],
        &(0..10).collect::<Vec<_>>()[..],
        "quem caiu não foi a cauda"
    );
    let ida = cano.ida();
    assert_eq!(ida.descartados_na_fila as usize, 100 - indices.len());
    assert_eq!(ida.perdidos_no_sorteio, 0);
    // 1000 bytes a 1 Mbps são 8 ms: o décimo sai aos 80 ms.
    let decimo = chegou[9].1;
    assert!(
        decimo >= Duration::from_millis(78) && decimo < Duration::from_millis(110),
        "o décimo datagrama saiu aos {decimo:?}, e a conta dá 80 ms"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cru_a_banda_segura_a_vazao() {
    // 2 Mbps oferecidos (1000 bytes a cada 4 ms, por 2 s) num gargalo de 1 Mbps.
    let estreito = Sentido {
        banda_bps: Some(1_000_000),
        fila_max: 20_000,
        ..Sentido::limpo()
    };
    let comeco = Instant::now();
    let (chegou, cano, emissao) = pela_ida_cronometrada(
        ConfigDoCano::simetrico(estreito, 1),
        500,
        1000,
        Duration::from_millis(4),
    )
    .await;
    // **Primeiro o arranjo, e só depois o cano.** Tudo o que vem abaixo supõe
    // um gargalo cheio, e um gargalo só enche se o emissor oferecer mais do que
    // ele escoa. O emissor anda por prazo e leva 2 s nos 500 datagramas; se a
    // máquina o segurou a ponto de a oferta média cair para perto da banda, o
    // que reprova aqui é o arranjo, com esse nome — e não «o cano deixou passar
    // de menos», que foi a frase errada que o CI leu em 12/11/2026.
    let oferecido_bps = 500.0 * 1000.0 * 8.0 / emissao.as_secs_f64();
    assert!(
        oferecido_bps >= 1_500_000.0,
        "o arranjo do teste não encheu o gargalo: o emissor levou {emissao:?} para mandar o que \
         devia sair em 2 s, e ofereceu {oferecido_bps:.0} bps a um gargalo de 1 Mbps (o pedido \
         eram 2 Mbps). Com isso o que vem abaixo não mede o cano: {:?}",
        cano.ida()
    );
    // A vazão se mede contra o tempo que de fato passou: o do emissor, mais o
    // que a fila levou para escoar depois dele (os 500 ms de silêncio com que
    // `pela_ida` encerra saem da conta).
    let ultima = chegou.iter().map(|(_, d)| *d).max().unwrap();
    let durou = comeco.elapsed().saturating_sub(Duration::from_millis(500));
    let bps = cano.ida().bytes_repassados as f64 * 8.0 / durou.as_secs_f64();
    assert!(
        (850_000.0..=1_100_000.0).contains(&bps),
        "passaram {bps:.0} bps por um gargalo de 1 Mbps (oferecidos {oferecido_bps:.0} bps em \
         {emissao:?}; a conta foi sobre {durou:?}): {:?}",
        cano.ida()
    );
    assert!(
        cano.ida().descartados_na_fila > 100,
        "com {oferecido_bps:.0} bps oferecidos a 1 Mbps, a fila descartou pouco: {:?}",
        cano.ida()
    );
    // Fila de 20 000 bytes a 1 Mbps: ninguém espera mais que 160 ms, mais a
    // folga do temporizador do próprio cano, que solta o datagrama no despertar
    // seguinte ao prazo. Com um relógio que só acorda a cada 15,6 ms (simulado
    // aqui, no emissor e no cano, em 12/11/2026) a maior demora foi de 174 ms, e
    // os 200 ms de antes deixavam 26 ms para um runner que acorda 10–20 ms
    // tarde. O teto continua pegando o que importa: uma fila que não respeita o
    // `fila_max` — o dobro dela já são 320 ms.
    assert!(
        ultima < Duration::from_millis(250),
        "um datagrama ficou {ultima:?} numa fila de 160 ms"
    );
}

/// O cano pede aos seus sockets o buffer de recepção da ponta. Sem ele, uma
/// rajada transbordava o socket de entrada e o cano perdia o que ninguém
/// mandou perder, fora de todo contador (ver `socket_do_cano`, em
/// `seele_enlace::teste`). O teto é do sistema, então o teste compara com o que
/// o mesmo sistema dá a um socket cru que pede o mesmo número — a mesma régua
/// de `pilha_dupla::a_ponta_pede_o_buffer_de_recepcao_grande`.
#[tokio::test]
async fn o_cano_pede_o_buffer_de_recepcao_grande() {
    let cru = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
    let padrao = cru.recv_buffer_size().unwrap();
    cru.set_recv_buffer_size(seele_enlace::BUFFER_DE_RECEPCAO)
        .unwrap();
    let possivel = cru.recv_buffer_size().unwrap();
    let destino = socket().await;
    let cano = Cano::abrir(
        destino.local_addr().unwrap(),
        ConfigDoCano::simetrico(Sentido::limpo(), 1),
    )
    .await
    .unwrap();
    let dado = cano.buffer_de_recepcao().unwrap();
    assert_eq!(
        dado, possivel,
        "o cano não pediu o buffer de recepção (padrão do sistema: {padrao})"
    );
    eprintln!("buffer de recepção: padrão {padrao}, do cano {dado}");
}

// ---- com o enlace de verdade por cima --------------------------------------

async fn abrir_j(par: &seele_enlace::teste::ParDeTeste) -> (Datagramas, Datagramas) {
    let CanalDeTunel::SemOrdem(emissor) =
        tokio::time::timeout(PRAZO, par.cliente.abrir_tunel(Confiabilidade::SemOrdem))
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("o j abriu ordenado");
    };
    let CanalDeTunel::SemOrdem(receptor) =
        tokio::time::timeout(PRAZO, par.anfitriao.aceitar_tunel())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("o j chegou ordenado");
    };
    (emissor, receptor)
}

/// 2000 datagramas `j` de 200 bytes, um por milissegundo. Devolve quantos
/// saíram para o fio (os que o emissor não descartou) e quantos chegaram.
async fn dois_mil_pelo_j(config: ConfigDoCano) -> (u64, u64, Cano) {
    let (par, cano) = tokio::time::timeout(PRAZO, par_por_cano(config))
        .await
        .unwrap();
    let (emissor, mut receptor) = abrir_j(&par).await;
    // O leitor só para quando o teste manda: com perda o SCTP fica mais de um
    // segundo sem entregar nada (o T3), e um leitor que desistisse no silêncio
    // largaria o `Datagramas` e fecharia o canal no meio da rajada.
    let (parar, mut parou) = tokio::sync::oneshot::channel::<()>();
    let leitor = tokio::spawn(async move {
        let mut chegaram = 0_u64;
        loop {
            tokio::select! {
                d = receptor.receber() => match d {
                    Some(_) => chegaram += 1,
                    None => break,
                },
                _ = &mut parou => break,
            }
        }
        chegaram
    });
    let mut recusados = 0_u64;
    let mut quando = tokio::time::interval(Duration::from_millis(1));
    for n in 0..2000_u32 {
        quando.tick().await;
        let mut d = vec![0_u8; 200];
        d[..4].copy_from_slice(&n.to_be_bytes());
        if emissor.mandar(&d).is_err() {
            recusados += 1;
        }
    }
    // O que ainda está a caminho: o atraso do cano e um estouro do T3.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let _ = parar.send(());
    let chegaram = tokio::time::timeout(PRAZO, leitor).await.unwrap().unwrap();
    // O que o `j` descartou antes do fio (o limiar de volume) não é perda do cano.
    let descartados = par.cliente.contadores().tunel_descartado;
    assert_eq!(recusados, 0, "a fila do j encheu a um datagrama por ms");
    (2000 - descartados, chegaram, cano)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_ice_fecha_pelo_cano_e_sem_perda_nada_se_perde() {
    let config = ConfigDoCano::simetrico(Sentido::limpo(), 11);
    let (par, cano) = tokio::time::timeout(PRAZO, par_por_cano(config))
        .await
        .unwrap();
    // Quem liga fala com o cano, e o anfitrião também: nenhum dos dois conhece
    // o endereço do outro. Um par que fechasse por fora do cano não mediria nada.
    assert_eq!(par.cliente.remote_address(), cano.endereco());
    assert_ne!(
        par.anfitriao.remote_address(),
        par.ponta_do_cliente.local_addr(),
        "o anfitrião fala direto com quem liga: o cano ficou de fora"
    );
    assert_ne!(par.anfitriao.remote_address(), cano.endereco());
    drop(par);
    drop(cano);

    let (sairam, chegaram, cano) = dois_mil_pelo_j(config).await;
    assert_eq!(sairam, 2000, "o emissor descartou sem haver perda");
    assert_eq!(chegaram, 2000);
    assert_eq!(cano.ida().descartados(), 0);
    assert_eq!(cano.volta().descartados(), 0);
    assert!(cano.ida().repassados >= 2000, "{:?}", cano.ida());
    assert!(cano.volta().repassados > 0, "{:?}", cano.volta());
}

/// O que este teste **não** afirma, porque foi medido e não é verdade: que o
/// `j` perde 10% com 10% de perda no caminho. O SCTP do `sctp-proto` 0.10.7
/// para de mandar a cada estouro do T3 (`RTO_MIN` de 1 s), o `j` descarta no
/// emissor o que passa de `LIMIAR_DE_VOLUME`, e do que chega ao fio se perde
/// mais que a fração do caminho. A fração exata é a do teste cru, acima (com
/// 20%; aqui são 10%); aqui
/// se confere que a perda do cano chega ao enlace, nos dois sentidos, e que o
/// sorteio se repete.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn com_10_por_cento_o_j_perde_e_a_semente_repete_o_sorteio() {
    let config = ConfigDoCano::simetrico(com_perda(0.10), 2026);
    let (sairam_a, chegaram_a, cano_a) = dois_mil_pelo_j(config).await;
    let (sairam_b, chegaram_b, cano_b) = dois_mil_pelo_j(config).await;
    for (sairam, chegaram, cano) in [
        (sairam_a, chegaram_a, &cano_a),
        (sairam_b, chegaram_b, &cano_b),
    ] {
        assert!(sairam >= 100, "quase nada saiu para o fio: {sairam}");
        let fracao = 1.0 - chegaram as f64 / sairam as f64;
        assert!(
            (0.04..1.0).contains(&fracao),
            "o j perdeu {:.1}% ({chegaram} de {sairam}) com 10% no caminho",
            fracao * 100.0
        );
        // Os dois sentidos somados, para a conta ter datagramas bastantes: com
        // 200 ou mais, o desvio padrão é de 2,1 pontos ou menos, e a faixa é de
        // quase quatro desvios.
        let (ida, volta) = (cano.ida(), cano.volta());
        let perdidos = ida.perdidos_no_sorteio + volta.perdidos_no_sorteio;
        let todos = perdidos + ida.repassados + volta.repassados;
        assert!(todos >= 200, "só {todos} datagramas passaram pelo cano");
        let no_cano = perdidos as f64 / todos as f64;
        assert!(
            (0.04..=0.18).contains(&no_cano),
            "o cano perdeu {:.1}% de {todos}",
            no_cano * 100.0
        );
        assert!(ida.perdidos_no_sorteio > 0, "a ida não perdeu nada");
        assert!(volta.perdidos_no_sorteio > 0, "a volta não perdeu nada");
    }
    // O sorteio é o mesmo nas duas rodadas, datagrama a datagrama. (Qual
    // datagrama do enlace cai em cada posição depende do relógio — um SACK a
    // mais muda a fila —, então o que o `j` perde é parecido, e não idêntico.)
    let (a, b) = (cano_a.destinos_da_ida(), cano_b.destinos_da_ida());
    let comum = a.len().min(b.len());
    assert!(comum >= 100, "só {comum} destinos guardados");
    assert_eq!(a[..comum], b[..comum], "a semente não repetiu o sorteio");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn com_50_ms_de_atraso_o_ida_e_volta_sobe_para_100() {
    async fn medir(atraso_ms: u64) -> (Duration, Duration) {
        let sentido = Sentido {
            atraso: Duration::from_millis(atraso_ms),
            ..Sentido::limpo()
        };
        let (par, _cano) =
            tokio::time::timeout(PRAZO, par_por_cano(ConfigDoCano::simetrico(sentido, 5)))
                .await
                .unwrap();
        // O ida e volta que a aplicação mede: uma voz vai e o anfitrião devolve.
        let eco = {
            let anfitriao = par.anfitriao.clone();
            tokio::spawn(async move {
                while let Ok(d) = anfitriao.read_datagram().await {
                    let _ = anfitriao.send_datagram(d);
                }
            })
        };
        let mut melhor = Duration::MAX;
        for n in 0..10_u8 {
            let saiu = Instant::now();
            par.cliente
                .send_datagram(vec![1, n, 0, 0, 0, 0, 0, 0])
                .unwrap();
            loop {
                let d = tokio::time::timeout(PRAZO, par.cliente.read_datagram())
                    .await
                    .unwrap()
                    .unwrap();
                if d.len() == 8 && d[1] == n {
                    break;
                }
            }
            melhor = melhor.min(saiu.elapsed());
        }
        eco.abort();
        // E o que o enlace mede (ICE): a estatística sai a cada segundo.
        let prazo = Instant::now() + Duration::from_secs(8);
        let mut do_enlace = par.cliente.rtt();
        while do_enlace.is_zero() && Instant::now() < prazo {
            tokio::time::sleep(Duration::from_millis(100)).await;
            do_enlace = par.cliente.rtt();
        }
        (melhor, do_enlace)
    }

    let (eco_limpo, enlace_limpo) = medir(0).await;
    let (eco_lento, enlace_lento) = medir(50).await;
    assert!(
        eco_limpo < Duration::from_millis(20),
        "sem atraso, o eco levou {eco_limpo:?}"
    );
    assert!(
        eco_lento >= Duration::from_millis(100) && eco_lento < Duration::from_millis(140),
        "com 50 ms por sentido, o eco levou {eco_lento:?}"
    );
    assert!(
        enlace_limpo < Duration::from_millis(20),
        "sem atraso, o enlace mediu {enlace_limpo:?}"
    );
    assert!(
        enlace_lento >= Duration::from_millis(100) && enlace_lento < Duration::from_millis(160),
        "com 50 ms por sentido, o enlace mediu {enlace_lento:?}"
    );
}
