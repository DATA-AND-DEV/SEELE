//! A coordenação entre o aviso e o aperto de mão.
//!
//! Nenhum teste do projeto olhava para o **relógio entre as duas coisas**:
//! `candidatos.rs` prova que o próximo candidato é tentado, `apresentacao.rs`
//! prova que o aviso chega, e o intervalo entre eles não era de ninguém. Era
//! exatamente ali que o defeito morava — o furo abria por 600 ms e o aperto de
//! mão chegava até doze segundos depois.
//!
//! # O que se mede aqui, e o que nunca se mede
//!
//! Nenhum destes testes conecta, e nenhum deles quer. O que se mede é **quando**
//! o datagrama do degrau 4 sai, em relação a quando o candidato que depende dele
//! é tentado. Um teste que exigisse sucesso precisaria de um NAT de verdade
//! entre duas casas — que é justamente o teste de campo que este ciclo existe
//! para fazer passar, e não cabe numa máquina só.
//!
//! # Por que os endereços são estes
//!
//! `10.255.255.x` e `192.168.255.1` são privados e não levam a lugar nenhum: são
//! o candidato da rede de outra casa, que de fora queima o prazo inteiro sem
//! nunca devolver um ICMP. `203.0.113.x` é TEST-NET-3 (RFC 5737), reservado para
//! documentação e sem rota em lugar nenhum, mas **global** para toda
//! classificação que este código faz — é o candidato refletido, o único que
//! depende de alguém ter furado o caminho até ele.
//!
//! E `[::ffff:127.0.0.1]:porta` é o candidato público que **responde**: a
//! classificação de `enlace.rs` pergunta por loopback na forma escrita, então a
//! forma mapeada do loopback conta como pública e ainda assim entrega o pacote a
//! um socket desta máquina. É o que torna possível medir o intervalo entre o
//! `LEVE` e o `Initial` sem duas casas e um NAT entre elas.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seele_core::enlace::{Destino, Enlace};
use seele_core::{ConnectError, MemoryPinStore, PinStore};

/// A impressão digital que o convite promete.
///
/// Obrigatória: é dela que sai a marca do aviso, e sem marca não se prepara
/// batida nenhuma — todos estes testes mediriam silêncio.
const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

/// Um nome TLS que o quinn recusa antes de mandar pacote nenhum.
///
/// Serve para um candidato **acabar depressa** sem que nada precise responder:
/// `Endpoint::connect` devolve erro na hora, e o que sobra para medir é o que o
/// laço faz quando a tentativa termina.
const NOME_TLS_IMPOSSIVEL: &str = "nome inválido com espaço";

/// Um ponto de encontro que só anota quando cada aviso chegou.
async fn ponto_que_anota() -> Option<(SocketAddr, Arc<Mutex<Vec<Instant>>>)> {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok()?;
    let onde = socket.local_addr().ok()?;
    let quando: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    let anotador = Arc::clone(&quando);
    tokio::spawn(async move {
        let mut balde = [0_u8; 96];
        while socket.recv_from(&mut balde).await.is_ok() {
            if let Ok(mut lista) = anotador.lock() {
                lista.push(Instant::now());
            }
        }
    });
    Some((onde, quando))
}

/// Um candidato de verdade, que anota quando cada pacote chegou e de onde.
///
/// Devolve o endereço **como ele vai no convite** e o caderno. `mapeado` decide
/// entre a forma escrita (`127.0.0.1:porta`, que a classificação lê como
/// loopback e não avisa) e a mapeada (`[::ffff:127.0.0.1]:porta`, que ela lê
/// como pública e avisa) — as duas caem no mesmo socket.
async fn candidato_que_anota(
    mapeado: bool,
) -> Option<(String, Arc<Mutex<Vec<(Instant, SocketAddr)>>>)> {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok()?;
    let onde = socket.local_addr().ok()?;
    let escrito = if mapeado {
        format!("[::ffff:127.0.0.1]:{}", onde.port())
    } else {
        onde.to_string()
    };
    let chegadas: Arc<Mutex<Vec<(Instant, SocketAddr)>>> = Arc::new(Mutex::new(Vec::new()));
    let anotador = Arc::clone(&chegadas);
    tokio::spawn(async move {
        let mut balde = [0_u8; 1500];
        while let Ok((_, de)) = socket.recv_from(&mut balde).await {
            if let Ok(mut lista) = anotador.lock() {
                lista.push((Instant::now(), de));
            }
        }
    });
    Some((escrito, chegadas))
}

/// O primeiro instante anotado, ou o pânico com a frase de quem chamou.
fn primeira_chegada(caderno: &Arc<Mutex<Vec<(Instant, SocketAddr)>>>) -> Option<(Instant, u16)> {
    let lista = caderno.lock().ok()?;
    let (quando, de) = lista.first()?;
    Some((*quando, de.port()))
}

/// Um candidato do convite, com a impressão digital que o degrau 4 exige.
fn destino_com_nome(alvo: &str, nome_tls: &str) -> Destino {
    let Ok(servidor) = alvo.parse::<SocketAddr>() else {
        panic!("o alvo de teste `{alvo}` não é um endereço");
    };
    Destino {
        servidor,
        nome_tls: nome_tls.to_owned(),
        // Sob o endereço escrito, e não sob um nome fixo: dois candidatos são
        // Dogmas diferentes até que um deles responda.
        chave_do_pin: alvo.to_owned(),
        apelido: "piloto".to_owned(),
        segredo: None,
        impressao_esperada: Some(IMPRESSAO.to_owned()),
    }
}

/// Monta um convite com `alvos` de candidatos e `ponto` de ponto de encontro, e
/// chama o caminho público de conexão do `seele-core`.
///
/// **Sempre falha em conectar**, e é isso que se quer: não há Dogma nenhum em
/// endereço nenhum destes. O que se mede é o relógio, não o sucesso.
async fn tentar_convite_com_nome(
    ponto: SocketAddr,
    alvos: &[&str],
    nome_tls: &str,
) -> Result<Enlace, ConnectError> {
    // O `aviso` do bilhete é para onde o anfitrião seria mandado furar. Nada
    // deste lado o usa a não ser para escrever o `LEVE`, e ele nunca é tentado
    // como candidato — daí um endereço global qualquer.
    let Ok(bilhete) = seele_proto::uri::Bilhete::novo(ponto.to_string(), "45.33.32.156:41234")
    else {
        panic!("o bilhete de teste não se monta");
    };
    let destinos: Vec<Destino> = alvos
        .iter()
        .map(|alvo| destino_com_nome(alvo, nome_tls))
        .collect();

    Enlace::conectar_entre_com_bilhete(
        destinos,
        Some(bilhete),
        ed25519_dalek::SigningKey::from_bytes(&[13; 32]),
        Arc::new(MemoryPinStore::default()) as Arc<dyn PinStore>,
    )
    .await
}

/// O mesmo, com um nome TLS que serve.
async fn tentar_convite_de_teste(
    ponto: SocketAddr,
    alvos: &[&str],
) -> Result<Enlace, ConnectError> {
    tentar_convite_com_nome(ponto, alvos, "localhost").await
}

#[tokio::test]
async fn o_aviso_sai_imediatamente_antes_do_candidato_que_precisa_dele() {
    // O teste-carro-chefe do ciclo, e ele tem de prender o aviso **dos dois
    // lados**. Um limite inferior sozinho («saiu depois de 500 ms») não prende
    // nada: o conserto inteiro desfeito, com um aviso único antes do laço
    // atrasado por 700 ms, passa por ele — e um aviso que saísse *depois* da
    // tentativa também.
    //
    // Por isso os dois candidatos são sockets de verdade, e o que se mede é a
    // ordem entre três instantes:
    //
    //   1. o `Initial` chega ao candidato 1, que é loopback na forma escrita e
    //      **não** avisa;
    //   2. o `LEVE` chega ao ponto de encontro — tem de ser **depois** de (1),
    //      porque o aviso é do candidato 2 e não do laço;
    //   3. o `Initial` chega ao candidato 2, entre 100 e 600 ms depois de (2):
    //      ESPERA_DO_FURO é de 200 ms, e é ela que faz o `Initial` cair dentro
    //      do furo em vez de antes dele.
    let Some((ponto, quando)) = ponto_que_anota().await else {
        return;
    };
    let Some((primeiro_alvo, chegadas_no_primeiro)) = candidato_que_anota(false).await else {
        return;
    };
    let Some((segundo_alvo, chegadas_no_segundo)) = candidato_que_anota(true).await else {
        return;
    };

    let comeco = Instant::now();
    let _ = tentar_convite_de_teste(ponto, &[primeiro_alvo.as_str(), segundo_alvo.as_str()]).await;

    let Ok(avisos) = quando.lock() else {
        return;
    };
    let Some(primeiro_aviso) = avisos.first() else {
        panic!("nenhum aviso saiu; o degrau 4 não aconteceu");
    };
    let atraso = primeiro_aviso.duration_since(comeco);
    assert!(
        atraso > Duration::from_millis(500),
        "o aviso saiu no instante zero — é o defeito de origem: ele tem de sair \
         colado ao candidato refletido, e não antes do laço (saiu em {atraso:?})"
    );

    let Some((tentou_o_primeiro, _)) = primeira_chegada(&chegadas_no_primeiro) else {
        panic!("o candidato 1 nunca foi tentado; não há ordem para conferir");
    };
    assert!(
        *primeiro_aviso > tentou_o_primeiro,
        "o aviso saiu antes de o candidato 1 chegar a ser tentado: ele é do laço \
         e não do candidato que precisa dele"
    );

    let Some((tentou_o_segundo, _)) = primeira_chegada(&chegadas_no_segundo) else {
        panic!("o candidato 2 nunca foi tentado; o aviso não coordenou com nada");
    };
    assert!(
        tentou_o_segundo > *primeiro_aviso,
        "o aperto de mão do candidato 2 saiu antes do aviso dele: o anfitrião \
         ainda não teria furado nada quando o Initial chegou"
    );
    let entre = tentou_o_segundo.duration_since(*primeiro_aviso);
    assert!(
        entre > Duration::from_millis(100),
        "o Initial saiu {entre:?} depois do LEVE: sem a espera do furo ele chega \
         antes de o anfitrião ter furado, e o degrau 4 volta a depender de um \
         PTO do quinn"
    );
    assert!(
        entre < Duration::from_millis(600),
        "o Initial saiu {entre:?} depois do LEVE: o furo do outro lado já fechou, \
         que é exatamente o defeito de origem"
    );
}

#[tokio::test]
async fn um_candidato_da_rede_de_casa_nao_gasta_aviso_nenhum() {
    // Um convite só com endereços privados não precisa de furo nenhum. Avisar
    // ali gastaria metadado de quem não pediu e furos da janela do anfitrião:
    // com três avisos por candidato, quatro candidatos privados custariam doze
    // furos a uma pessoa que não precisava de um — e a janela é de sessenta por
    // dez segundos, que é o teto dimensionado para quem **precisa**.
    let Some((ponto, quando)) = ponto_que_anota().await else {
        return;
    };

    let _ = tentar_convite_de_teste(ponto, &["10.255.255.1:8383", "192.168.255.1:8383"]).await;

    {
        let Ok(avisos) = quando.lock() else {
            return;
        };
        assert!(
            avisos.is_empty(),
            "nenhum candidato privado precisa de furo, e nenhum aviso devia ter saído"
        );
    }

    // O controle, e ele não é decorativo. Esta é uma asserção de **ausência**,
    // e uma asserção de ausência fica verde também quando o mecanismo inteiro
    // está quebrado: um bilhete que não vira batida, um ponto de encontro que
    // não escuta, um `preparar` que devolve `None` — em qualquer um desses o
    // `is_empty` de cima passa sem ter testado nada.
    //
    // Então o mesmo ponto de encontro, montado do mesmo jeito, recebe um
    // convite que **tem** um candidato refletido. Se nada chegar aqui, o que o
    // teste de cima mediu foi o próprio silêncio dele.
    let Some((outro_ponto, outro_quando)) = ponto_que_anota().await else {
        return;
    };
    let _ = tentar_convite_de_teste(outro_ponto, &["203.0.113.7:8383", "10.255.255.1:8383"]).await;
    let Ok(avisos_do_controle) = outro_quando.lock() else {
        return;
    };
    assert!(
        !avisos_do_controle.is_empty(),
        "o controle não recebeu aviso nenhum por um candidato público: o teste \
         de ausência acima não estava medindo o silêncio dos candidatos \
         privados, e sim o de um degrau 4 que não acontece nunca"
    );
}

#[tokio::test]
async fn um_candidato_privado_na_forma_mapeada_nao_engana_a_guarda() {
    // A forma mapeada não é borda: um ponto de encontro atrás de socket de pilha
    // dupla reflete a origem de quem bateu como `::ffff:a.b.c.d`, e é essa
    // origem que volta no `AQUI` e entra no convite. Uma classificação que só
    // olhasse a forma escrita veria `::ffff:192.168.1.5` como público — o
    // endereço da rede de casa de alguém queimaria três furos, vazaria metadado
    // e ainda levaria o prazo cheio de quatro segundos.
    //
    // As duas metades da mesma guarda são medidas juntas de propósito: nenhum
    // aviso (é privado) **e** prazo curto (é de outra casa). São os dois
    // caminhos que a canonização alimenta, e nenhum deles pode ficar verde por
    // acaso: sem `to_canonical`, os quatro candidatos avisam e cada um custa
    // 4,2 s em vez de 1 s.
    let Some((ponto, quando)) = ponto_que_anota().await else {
        return;
    };

    let comeco = Instant::now();
    let _ = tentar_convite_de_teste(
        ponto,
        &[
            "[::ffff:10.255.255.1]:8383",
            "[::ffff:192.168.255.1]:8383",
            "[::ffff:10.255.255.2]:8383",
            "[::ffff:10.255.255.3]:8383",
        ],
    )
    .await;
    let gasto = comeco.elapsed();

    {
        let Ok(avisos) = quando.lock() else {
            return;
        };
        assert!(
            avisos.is_empty(),
            "{} aviso(s) saíram por candidatos privados escritos na forma mapeada: \
             a guarda do furo lê a forma escrita e não a canônica",
            avisos.len()
        );
        assert!(
            gasto < Duration::from_secs(8),
            "quatro privados mapeados levaram {gasto:?}: o prazo curto do \
             candidato distante não reconhece a forma mapeada"
        );
    }

    // O controle, e ele cobre as duas direções de uma vez.
    //
    // A primeira metade é a de sempre: a asserção de cima é de **ausência**, e
    // uma ausência fica verde também quando o degrau 4 inteiro está morto — um
    // `preparar` que devolvesse `None` deixaria zero avisos e zero prazo longo,
    // e o teste passaria sem ter medido nada.
    //
    // A segunda é o outro lado da mesma faca, e hoje não tem mais ninguém
    // olhando para ela: uma canonização **exagerada**, que fizesse um endereço
    // público mapeado parar de avisar, não acenderia teste nenhum deste arquivo.
    // `[::ffff:203.0.113.7]` é as duas coisas — mapeado e público —, e é
    // exatamente o que o ponto de encontro devolve para uma casa atrás de pilha
    // dupla: se ele deixar de avisar, o degrau 4 morre para quem mais precisa
    // dele, em silêncio.
    let Some((ponto_do_controle, quando_do_controle)) = ponto_que_anota().await else {
        return;
    };
    let _ = tentar_convite_de_teste(
        ponto_do_controle,
        &["[::ffff:203.0.113.7]:8383", "10.255.255.1:8383"],
    )
    .await;
    let Ok(avisos_do_controle) = quando_do_controle.lock() else {
        return;
    };
    assert!(
        !avisos_do_controle.is_empty(),
        "nenhum aviso saiu por um candidato público escrito na forma mapeada: ou \
         o degrau 4 não está acontecendo — e a ausência medida acima não provou \
         nada —, ou a canonização passou do ponto e apagou o furo de quem mais \
         depende dele"
    );
}

#[tokio::test]
async fn um_convite_de_enderecos_mortos_termina_em_segundos_e_nao_em_dezenas() {
    // Quatro endereços privados de outra casa: cada um custava
    // PRAZO_POR_CANDIDATO = 4 s, e a má notícia chegava em dezesseis segundos.
    // Com o prazo curto do candidato distante o pior caso cai para poucos.
    let Some((ponto, _)) = ponto_que_anota().await else {
        return;
    };

    let comeco = Instant::now();
    let _ = tentar_convite_de_teste(
        ponto,
        &[
            "10.255.255.1:8383",
            "10.255.255.2:8383",
            "10.255.255.3:8383",
            "10.255.255.4:8383",
        ],
    )
    .await;

    let gasto = comeco.elapsed();
    assert!(
        gasto < Duration::from_secs(8),
        "quatro endereços mortos levaram {gasto:?}; o prazo curto do candidato \
         distante não está sendo aplicado"
    );
}

#[tokio::test]
async fn o_aviso_se_repete_enquanto_o_aperto_de_mao_corre() {
    // A retentativa que não existia. Um `AQUI` perdido no caminho custava a
    // conexão inteira em silêncio: o anfitrião nunca furava, o candidato
    // queimava os quatro segundos, e o erro que saía era o de outro endereço.
    //
    // Três avisos por candidato, espaçados de 700 ms, **enquanto** o aperto de
    // mão corre — e não antes dele, que é o que os transformaria de novo em
    // dois pacotes gastos cedo demais.
    let Some((ponto, quando)) = ponto_que_anota().await else {
        return;
    };

    // Abandonada no fim: nenhum destes endereços responde, e o que interessa
    // acontece dentro do prazo do primeiro candidato.
    let tentativa = tokio::spawn(async move {
        let _ = tentar_convite_de_teste(ponto, &["203.0.113.7:8383", "203.0.113.8:8383"]).await;
    });
    tokio::time::sleep(Duration::from_millis(2500)).await;
    tentativa.abort();

    let Ok(avisos) = quando.lock() else {
        return;
    };
    assert!(
        avisos.len() >= 3,
        "só {} aviso(s) em 2,5 s do primeiro candidato: a repetição não está \
         correndo junto com o aperto de mão",
        avisos.len()
    );

    // E são espaçados, não uma rajada. Três pacotes juntos no instante zero
    // seriam os dois avisos de antes do laço com um irmão — cobertura temporal
    // nenhuma, e a mesma corrida de sempre.
    let (Some(primeiro), Some(terceiro)) = (avisos.first(), avisos.get(2)) else {
        panic!("a contagem disse que havia três avisos e a lista não tem");
    };
    let janela = terceiro.duration_since(*primeiro);
    assert!(
        janela > Duration::from_millis(1000),
        "os três avisos couberam em {janela:?}: saíram em rajada, e não \
         espaçados enquanto o aperto de mão corria"
    );
}

#[tokio::test]
async fn a_repeticao_para_quando_o_candidato_acaba() {
    // Avisar sobre um candidato que já terminou gasta furo da janela do
    // anfitrião por um caminho que ninguém vai tentar de novo — e a janela é o
    // que separa "quem tem o link entra" de "quem tem o link faz o Dogma jorrar
    // pacote".
    //
    // O que torna isto observável numa máquina só é o candidato **acabar
    // rápido**, e não precisar conectar: com um nome TLS que o quinn recusa, a
    // tentativa morre antes de qualquer pacote, muito antes de a repetição
    // chegar ao segundo aviso (que sairia 700 ms depois do primeiro). Se o
    // `abort` não acontecer, os dois avisos que faltam saem assim mesmo.
    let Some((ponto, quando)) = ponto_que_anota().await else {
        return;
    };
    let _ = tentar_convite_com_nome(ponto, &["203.0.113.7:8383"], NOME_TLS_IMPOSSIVEL).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    {
        let Ok(avisos) = quando.lock() else {
            return;
        };
        assert_eq!(
            avisos.len(),
            1,
            "o candidato único acabou na hora e mesmo assim saíram {} avisos: a \
             repetição não foi abortada",
            avisos.len()
        );
    }

    // E o mesmo pelo caminho do laço, que é o que roda no convite de verdade —
    // o de cima passa pelo atalho do convite de um endereço só, e os dois têm o
    // próprio `abort`.
    let Some((ponto_do_laco, quando_do_laco)) = ponto_que_anota().await else {
        return;
    };
    let _ = tentar_convite_com_nome(
        ponto_do_laco,
        &["203.0.113.7:8383", "203.0.113.8:8383"],
        NOME_TLS_IMPOSSIVEL,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let Ok(avisos) = quando_do_laco.lock() else {
        return;
    };
    assert_eq!(
        avisos.len(),
        2,
        "dois candidatos que acabaram na hora deixaram {} avisos: o laço não \
         aborta a repetição quando o candidato termina",
        avisos.len()
    );
}

#[tokio::test]
async fn um_aviso_recusado_pelo_kernel_nao_derruba_o_laco() {
    // A decisão que o laço toma com o erro de `Batida::avisar`, e a única que
    // não troca um defeito por outro: registrar e ir ao candidato seguinte.
    //
    // O ponto de encontro aqui é `255.255.255.255:9`. `preparar` dá certo — é
    // um endereço —, e o `send_to` é recusado pelo kernel na hora, sem rede
    // nenhuma no meio: este socket nunca liga `SO_BROADCAST`, e sem essa opção
    // o envio volta com permissão negada em BSD, Linux e Windows.
    //
    // # O guarda da premissa
    //
    // «O laço continuou» ficaria verde também se `preparar` tivesse devolvido
    // `None`: sem batida não há aviso para falhar, e o segundo candidato seria
    // tentado do mesmo jeito — o teste passaria sem nunca ter exercido o erro
    // que diz cobrir.
    //
    // A premissa é conferida pela **porta de origem**. Com batida, todos os
    // candidatos saem do socket dela, que é o do aviso; sem batida, cada
    // tentativa abre um `Endpoint` próprio e sai de uma porta nova. Duas portas
    // iguais nos dois candidatos é a prova de que a batida existia — e portanto
    // de que houve um `avisar` para ser recusado.
    let Some((primeiro_alvo, chegadas_no_primeiro)) = candidato_que_anota(true).await else {
        return;
    };
    let Some((segundo_alvo, chegadas_no_segundo)) = candidato_que_anota(true).await else {
        return;
    };

    let ponto = SocketAddr::from(([255, 255, 255, 255], 9));
    let alvos = [primeiro_alvo.clone(), segundo_alvo.clone()];
    let tentativa = tokio::spawn(async move {
        let _ = tentar_convite_de_teste(ponto, &[alvos[0].as_str(), alvos[1].as_str()]).await;
    });

    // Folgado: o segundo candidato só é tentado depois de o primeiro queimar os
    // quatro segundos dele.
    let ate = Instant::now() + Duration::from_secs(20);
    while Instant::now() < ate && primeira_chegada(&chegadas_no_segundo).is_none() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tentativa.abort();

    let Some((_, porta_do_segundo)) = primeira_chegada(&chegadas_no_segundo) else {
        panic!(
            "o segundo candidato nunca foi tentado: um aviso que o kernel recusou \
             derrubou o laço inteiro"
        );
    };
    let Some((_, porta_do_primeiro)) = primeira_chegada(&chegadas_no_primeiro) else {
        panic!("o primeiro candidato nunca foi tentado; não há premissa para conferir");
    };
    assert_eq!(
        porta_do_primeiro, porta_do_segundo,
        "os dois candidatos saíram de portas diferentes, então não havia batida \
         nenhuma: `preparar` falhou, não houve `avisar` para o kernel recusar, e \
         este teste não estava medindo o que diz medir"
    );
}
