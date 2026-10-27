//! O degrau 4 de quem entra, na linha 2: o `CHAMA` do ENC/3.
//!
//! # O que mudou na 0.17, e por que este arquivo foi reescrito
//!
//! Até a 0.16 o degrau 4 era o `LEVE` do ENC/1: um aviso **de mão única**, um
//! por candidato público, repetido três vezes enquanto o aperto de mão do QUIC
//! corria, e o que este arquivo media era o relógio entre o aviso e o
//! `Initial`. A linha 2 não manda `LEVE` (ADR 0063, `enc3.md`): quem entra manda
//! **um** `CHAMA` por tentativa de entrar (`seele_core::encontro::Chamada`),
//! da ponta da sessão, com o `U` desta tentativa; o ponto o repassa ao
//! anfitrião, que fura e responde, e a discagem vai ao endereço que o ponto
//! devolveu no `AQUI`. Não há mais «aviso por candidato» para contar, e as
//! asserções antigas («nenhum aviso por candidato privado», «dois avisos para
//! dois candidatos») perguntavam por um mecanismo que não existe.
//!
//! O que se prende aqui é o que o mecanismo novo promete, de ponta a ponta,
//! contra sockets de verdade:
//!
//! - o `CHAMA` sai **depois** da discagem direta (quem está na mesma casa
//!   entra sem o ponto ouvir nada) e **da mesma porta** que disca — é o
//!   endereço dessa porta que o ponto vê e que o anfitrião fura;
//! - sem resposta, ele se repete no máximo três vezes, espaçado, e para;
//! - uma resposta do ponto (`NINGUEM`) encerra a chamada no primeiro;
//! - um `AQUI` que não veio do ponto não desvia a discagem para lugar nenhum;
//! - sem impressão no convite não há marca, e nenhum `CHAMA` sai.
//!
//! Nenhum destes testes precisa conectar, como antes: o que se mede é o que
//! sai e para onde. O sucesso pelo degrau 4 (`FuroDeNat`) é medido em
//! `estados.rs`, e o ponto de verdade em `enc3_ponta_a_ponta.rs`.
//!
//! # Por que os endereços são estes
//!
//! `10.255.255.x` e `192.168.255.1` são privados e não levam a lugar nenhum.
//! `203.0.113.x` é TEST-NET-3 (RFC 5737). E `[::ffff:127.0.0.1]:porta` é o
//! candidato público que **recebe**: a classificação de `enlace.rs` pergunta
//! por loopback na forma escrita, e o pacote ainda assim chega a um socket
//! desta máquina.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seele_core::enlace::{e_privado, Destino, Enlace};
use seele_core::{ConnectError, MemoryPinStore, PinStore};
use seele_proto::encontro::{analisar3, resposta3, Pedido3, Resposta3};

mod vaga;

/// A impressão digital que o convite promete.
///
/// Obrigatória: é dela que saem as marcas do `CHAMA`, e sem marca não se chama
/// ninguém — todos estes testes, menos o último, mediriam silêncio.
const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

/// Um datagrama que chegou: quando, de onde, e os bytes.
type Caderno = Arc<Mutex<Vec<(Instant, SocketAddr, Vec<u8>)>>>;

/// Como o ponto de teste responde a um `CHAMA`.
#[derive(Clone, Copy)]
enum Responde {
    /// Não responde nada.
    Calado,
    /// Responde `NINGUEM` para a marca do servidor.
    Ninguem,
}

/// Um socket que anota tudo o que chega e, se for o ponto, responde.
async fn anotador(responde: Responde) -> (SocketAddr, Caderno, Arc<tokio::net::UdpSocket>) {
    let socket = Arc::new(
        tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("o loopback abre"),
    );
    let onde = socket.local_addr().expect("o socket sabe onde ligou");
    let caderno: Caderno = Arc::new(Mutex::new(Vec::new()));
    let anota = Arc::clone(&caderno);
    let leitor = Arc::clone(&socket);
    tokio::spawn(async move {
        let mut balde = [0_u8; 1500];
        while let Ok((lidos, de)) = leitor.recv_from(&mut balde).await {
            let bytes = balde.get(..lidos).unwrap_or_default().to_vec();
            if let (Responde::Ninguem, Some(Pedido3::Chama(chama))) = (responde, analisar3(&bytes))
            {
                let _ = leitor
                    .send_to(
                        &resposta3(&Resposta3::Ninguem {
                            marca: chama.servidor,
                        }),
                        de,
                    )
                    .await;
            }
            if let Ok(mut lista) = anota.lock() {
                lista.push((Instant::now(), de, bytes));
            }
        }
    });
    (onde, caderno, socket)
}

/// Os `CHAMA` do caderno de um ponto: quando e de onde.
fn chamas(caderno: &Caderno) -> Vec<(Instant, SocketAddr, seele_proto::encontro::Chama)> {
    caderno
        .lock()
        .expect("o caderno não envenena")
        .iter()
        .filter_map(|(quando, de, bytes)| match analisar3(bytes) {
            Some(Pedido3::Chama(chama)) => Some((*quando, *de, chama)),
            _ => None,
        })
        .collect()
}

/// O primeiro datagrama que um candidato recebeu: quando e de onde.
fn primeira_chegada(caderno: &Caderno) -> Option<(Instant, SocketAddr)> {
    let lista = caderno.lock().ok()?;
    let (quando, de, _) = lista.first()?;
    Some((*quando, *de))
}

/// O endereço de um anotador como ele vai no convite: na forma mapeada, que a
/// classificação lê como pública.
fn mapeado(onde: SocketAddr) -> String {
    format!("[::ffff:127.0.0.1]:{}", onde.port())
}

/// Um candidato do convite, como a FFI o monta em produção.
fn destino(alvo: &str, e_o_alvo: bool, impressao: Option<&str>) -> Destino {
    let servidor = alvo
        .parse::<SocketAddr>()
        .unwrap_or_else(|_| panic!("o alvo de teste `{alvo}` não é um endereço"));
    Destino {
        servidor,
        nome_tls: "localhost".to_owned(),
        chave_do_pin: alvo.to_owned(),
        apelido: "pessoa".to_owned(),
        segredo: None,
        esperada: impressao
            .map(|impressao| seele_core::enlace::Esperada::Certificado(impressao.to_owned())),
        o_pino_prova_o_servidor: e_o_alvo && !e_privado(servidor.ip()),
        aceito: None,
    }
}

/// Entra por um convite com estes candidatos e este ponto. **Sempre falha**:
/// não há servidor em endereço nenhum destes, e o que se mede é o que saiu.
async fn tentar(
    ponto: SocketAddr,
    alvos: &[String],
    impressao: Option<&str>,
) -> Result<Enlace, ConnectError> {
    let bilhete = seele_proto::uri::Bilhete::novo(ponto.to_string(), "45.33.32.156:41234")
        .expect("o bilhete de teste se monta");
    let destinos: Vec<Destino> = alvos
        .iter()
        .enumerate()
        .map(|(posicao, alvo)| destino(alvo, posicao == 0, impressao))
        .collect();
    Enlace::conectar_entre_com_bilhete(
        destinos,
        Some(bilhete),
        ed25519_dalek::SigningKey::from_bytes(&[13; 32]),
        Arc::new(MemoryPinStore::default()) as Arc<dyn PinStore>,
    )
    .await
}

/// O mesmo, numa tarefa que se abandona depois de `quanto`.
async fn tentar_por(
    ponto: SocketAddr,
    alvos: Vec<String>,
    impressao: Option<&'static str>,
    quanto: Duration,
) {
    let tentativa = tokio::spawn(async move {
        let _ = tentar(ponto, &alvos, impressao).await;
    });
    tokio::time::sleep(quanto).await;
    tentativa.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn o_chama_sai_depois_da_discagem_direta_e_da_mesma_porta() {
    let _vaga = vaga::minha();
    // **Depois**: quem está na mesma casa entra pela discagem direta antes de o
    // ponto ou o anfitrião ouvirem qualquer coisa (`Enlace::tentar_entre`: o
    // lote do degrau 4 entra na defasagem de 250 ms). Um `CHAMA` no instante
    // zero gastaria a janela de chamados do anfitrião (R2) por quem não
    // precisava, e mandaria ao ponto o metadado de uma entrada que era local.
    //
    // **Da mesma porta**: o NAT de quem entra mapeia por porta interna. O
    // anfitrião fura para o endereço que o ponto viu no `CHAMA`; se o `CHAMA`
    // saísse de um socket e a discagem de outro, o furo abriria para a porta
    // errada e o degrau 4 nunca entraria — sem erro nenhum que dissesse isso.
    let (ponto, do_ponto, _) = anotador(Responde::Calado).await;
    let (candidato, do_candidato, _) = anotador(Responde::Calado).await;

    let comeco = Instant::now();
    tentar_por(
        ponto,
        vec![mapeado(candidato)],
        Some(IMPRESSAO),
        Duration::from_millis(1500),
    )
    .await;

    let chamados = chamas(&do_ponto);
    let Some((primeiro_chama, porta_do_chama, _)) = chamados.first() else {
        panic!("nenhum CHAMA chegou ao ponto: o degrau 4 não aconteceu, e nada abaixo mede nada");
    };
    let Some((discou, porta_da_discagem)) = primeira_chegada(&do_candidato) else {
        panic!("o candidato direto nunca recebeu nada: não há ordem para conferir");
    };
    assert!(
        discou < *primeiro_chama,
        "o CHAMA saiu antes da discagem direta: o anfitrião é chamado por quem \
         talvez estivesse na mesma casa"
    );
    assert!(
        primeiro_chama.duration_since(comeco) >= Duration::from_millis(200),
        "o CHAMA saiu em {:?}, antes da defasagem: ele é do laço e não do lote \
         que espera o direto",
        primeiro_chama.duration_since(comeco)
    );
    assert_eq!(
        porta_do_chama.port(),
        porta_da_discagem.port(),
        "o CHAMA saiu de uma porta e a discagem de outra: o furo do anfitrião \
         abriria para a porta que não disca"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_resposta_o_chama_sai_tres_vezes_espacado_e_para() {
    let _vaga = vaga::minha();
    // UDP perde pacote, e um `CHAMA` perdido custava a chamada inteira em
    // silêncio. Por isso ele se repete — mas cada repetição vira um `CHAMADO`
    // novo e um furo novo do anfitrião, que tem teto próprio (R2). Três,
    // espaçados (`INTERVALO_DA_CHAMADA`, 800 ms), e nenhum depois.
    let (ponto, do_ponto, _) = anotador(Responde::Calado).await;
    tentar_por(
        ponto,
        vec!["203.0.113.7:8383".to_owned()],
        Some(IMPRESSAO),
        Duration::from_millis(6000),
    )
    .await;

    let chamados = chamas(&do_ponto);
    assert_eq!(
        chamados.len(),
        3,
        "um ponto calado recebeu {} CHAMA(s): três é o que cabe no teto de \
         chamados do anfitrião, e menos que isso perde a chamada por um pacote",
        chamados.len()
    );
    let (Some((primeiro, ..)), Some((terceiro, ..))) = (chamados.first(), chamados.get(2)) else {
        panic!("a contagem disse três e a lista não tem");
    };
    assert!(
        terceiro.duration_since(*primeiro) > Duration::from_millis(1200),
        "os três CHAMA couberam em {:?}: saíram em rajada, e uma rajada se perde \
         junto",
        terceiro.duration_since(*primeiro)
    );
    // E é a mesma tentativa: o mesmo `U` nos três, que é por onde o anfitrião
    // reconhece quem furou.
    let credenciais: std::collections::BTreeSet<String> = chamados
        .iter()
        .map(|(_, _, chama)| chama.credencial.texto().to_owned())
        .collect();
    assert_eq!(
        credenciais.len(),
        1,
        "as repetições levaram credenciais diferentes: o anfitrião furaria por \
         três tentativas que não existem"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn um_ninguem_do_ponto_encerra_a_chamada_no_primeiro_chama() {
    let _vaga = vaga::minha();
    // A metade de cima com resposta. O ponto disse que ninguém mora ali, e
    // insistir gastaria pacote e taxa do ponto por uma resposta que já veio.
    // O controle é o teste de cima: o mesmo ponto, calado, recebe três.
    let (ponto, do_ponto, _) = anotador(Responde::Ninguem).await;
    tentar_por(
        ponto,
        vec!["203.0.113.7:8383".to_owned()],
        Some(IMPRESSAO),
        Duration::from_millis(4000),
    )
    .await;
    assert_eq!(
        chamas(&do_ponto).len(),
        1,
        "o ponto respondeu NINGUEM e a chamada continuou insistindo"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn um_aqui_que_nao_vem_do_ponto_nao_desvia_a_discagem() {
    let _vaga = vaga::minha();
    // A ponta da sessão recebe datagramas de qualquer um. Um `AQUI` forjado,
    // vindo de outro endereço que não o do ponto, mandaria a discagem — com o
    // `U` desta tentativa — para onde o forjador quisesse. A impressão do DTLS
    // impediria a sessão com um impostor, mas não o pacote: seria um jeito de
    // fazer quem entra disparar Binding Requests contra terceiros.
    //
    // A isca é um socket que anota. O forjador manda, do socket dele, um `AQUI`
    // perfeito (a marca certa, lida do `CHAMA` que o ponto recebeu) apontando
    // para a isca. **O controle** é o mesmo `AQUI`, mandado do socket do ponto:
    // ele tem de levar a discagem à isca, ou o forjado não ter levado não
    // provaria nada.
    for do_ponto_mesmo in [false, true] {
        let (ponto, caderno_do_ponto, socket_do_ponto) = anotador(Responde::Calado).await;
        let (isca, caderno_da_isca, _) = anotador(Responde::Calado).await;
        let forjador = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("o loopback abre");

        let tentativa = tokio::spawn(async move {
            let _ = tentar(ponto, &["203.0.113.7:8383".to_owned()], Some(IMPRESSAO)).await;
        });
        let ate = Instant::now() + Duration::from_secs(3);
        let (de_quem, chama) = loop {
            if let Some((_, de, chama)) = chamas(&caderno_do_ponto).into_iter().next() {
                break (de, chama);
            }
            assert!(Instant::now() < ate, "nenhum CHAMA chegou ao ponto");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let aqui = resposta3(&Resposta3::Aqui {
            marca: chama.servidor,
            endereco: mapeado(isca).parse().expect("o endereço mapeado se lê"),
            dicas: Vec::new(),
        });
        if do_ponto_mesmo {
            socket_do_ponto
                .send_to(&aqui, de_quem)
                .await
                .expect("o AQUI sai");
        } else {
            forjador
                .send_to(&aqui, de_quem)
                .await
                .expect("o AQUI forjado sai");
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
        tentativa.abort();

        let a_isca_ouviu = primeira_chegada(&caderno_da_isca).is_some();
        if do_ponto_mesmo {
            assert!(
                a_isca_ouviu,
                "o AQUI do próprio ponto não levou a discagem à isca: o forjado \
                 não ter levado não prova o guarda"
            );
        } else {
            assert!(
                !a_isca_ouviu,
                "um AQUI que não veio do ponto desviou a discagem para a isca"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_impressao_no_convite_nenhum_chama_sai() {
    let _vaga = vaga::minha();
    // As marcas do `CHAMA` saem da impressão que o convite promete. Sem ela não
    // há a quem chamar, e chamar por uma marca inventada seria pedir ao ponto
    // que acordasse qualquer anfitrião. O controle é o primeiro teste deste
    // arquivo: o mesmo convite com a impressão chama.
    let (ponto, do_ponto, _) = anotador(Responde::Calado).await;
    tentar_por(
        ponto,
        vec!["203.0.113.7:8383".to_owned()],
        None,
        Duration::from_millis(2000),
    )
    .await;
    assert!(
        chamas(&do_ponto).is_empty(),
        "um convite sem impressão chamou o ponto"
    );
}

#[tokio::test]
async fn um_convite_de_enderecos_mortos_termina_em_segundos_e_nao_em_dezenas() {
    let _vaga = vaga::minha();
    // Quatro endereços privados de outra casa, discados juntos num lote, mais a
    // chamada pelo ponto: a má notícia chega em poucos segundos, e não na soma
    // dos prazos de cada um. Inclui a forma mapeada, que um ponto atrás de
    // pilha dupla devolve e que a classificação tem de canonizar.
    let (ponto, _, _) = anotador(Responde::Calado).await;
    for alvos in [
        [
            "10.255.255.1:8383",
            "10.255.255.2:8383",
            "10.255.255.3:8383",
            "10.255.255.4:8383",
        ],
        [
            "[::ffff:10.255.255.1]:8383",
            "[::ffff:192.168.255.1]:8383",
            "[::ffff:10.255.255.2]:8383",
            "[::ffff:10.255.255.3]:8383",
        ],
    ] {
        let alvos: Vec<String> = alvos.iter().map(|alvo| (*alvo).to_owned()).collect();
        let comeco = Instant::now();
        let _ = tentar(ponto, &alvos, Some(IMPRESSAO)).await;
        let gasto = comeco.elapsed();
        assert!(
            gasto < Duration::from_secs(8),
            "quatro endereços mortos ({alvos:?}) levaram {gasto:?}"
        );
    }
}

#[tokio::test]
async fn um_chama_recusado_pelo_kernel_nao_derruba_a_discagem() {
    let _vaga = vaga::minha();
    // O ponto é `255.255.255.255:9`: resolver dá certo, e o envio é recusado
    // pelo kernel na hora (o socket nunca liga `SO_BROADCAST`). A chamada
    // desiste (`FalhaDaChamada::Mudo`) e a discagem direta continua.
    //
    // **O que este teste não prova mais**: até a 0.16 a premissa («houve um
    // aviso para ser recusado») se conferia pela porta de origem, porque sem
    // batida cada candidato saía de um socket novo. Desde a 0.17 há uma ponta
    // por sessão sempre (R5), e a porta não distingue mais os dois casos. A
    // recusa do kernel só aparece no registro (`o CHAMA não saiu`), que este
    // teste não lê. O que fica é a metade que importa a quem entra: o
    // candidato direto é tentado e a tentativa termina.
    let (candidato, do_candidato, _) = anotador(Responde::Calado).await;
    let ponto = SocketAddr::from(([255, 255, 255, 255], 9));
    let comeco = Instant::now();
    let resultado = tokio::time::timeout(
        Duration::from_secs(20),
        tentar(ponto, &[mapeado(candidato)], Some(IMPRESSAO)),
    )
    .await;
    assert!(
        resultado.is_ok(),
        "um CHAMA recusado pelo kernel prendeu a tentativa por {:?}",
        comeco.elapsed()
    );
    assert!(
        primeira_chegada(&do_candidato).is_some(),
        "um CHAMA recusado pelo kernel derrubou a discagem direta"
    );
}
