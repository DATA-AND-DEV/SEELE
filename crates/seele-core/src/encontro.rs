//! Degrau 4 do ADR 0022, do lado de quem entra.
//!
//! O trabalho pesado é de quem hospeda — ver `alcance::encontro`, no
//! `seele-server`. Deste lado o degrau 4 é **um datagrama**: "ponto de encontro,
//! diga ao anfitrião de onde este pacote veio". O anfitrião recebe aquele
//! endereço, manda alguns pacotes para cá, e o roteador de lá passa a deixar
//! entrar o aperto de mão que vem em seguida.
//!
//! # Por que o socket importa mais que o datagrama
//!
//! O NAT mapeia por porta interna. Se este aviso sair de um socket e o QUIC sair
//! de outro, o anfitrião fura o caminho para a porta errada e o aperto de mão
//! continua batendo numa porta fechada — em quase todo roteador doméstico, que
//! filtra por endereço **e** porta.
//!
//! É por isso que `Batida` guarda o socket por onde bateu, em vez de só mandar
//! o pacote: quem conecta em seguida tem de conectar por ele. É o mesmo motivo
//! pelo qual o outro lado precisou de um espelho do socket do Dogma.
//!
//! # O que este lado nunca faz
//!
//! **Não lê resposta nenhuma do ponto de encontro.** Nem precisa: os endereços
//! que serão tentados vieram todos do `seele://`, e a impressão digital contra a
//! qual o Dogma é conferido também. Um ponto de encontro hostil consegue não
//! avisar o anfitrião — e é só. Ele não tem por onde mandar ninguém para outro
//! lugar, porque ninguém deste lado escuta o que ele diz.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use seele_proto::encontro::{self, Marca};
use seele_proto::uri::Bilhete;

/// Quanto tempo se gasta batendo antes de desistir e conectar assim mesmo.
///
/// Curto porque **não há o que esperar**: o aviso é de mão única, não há
/// resposta a aguardar, e a única coisa que este prazo cobre é a resolução do
/// nome do ponto de encontro. Um ponto de encontro fora do ar não pode atrasar
/// uma conexão que talvez nem precisasse dele — o primeiro endereço do convite é
/// o da rede de casa, e quem está na sala ao lado entra por ele.
const PRAZO: Duration = Duration::from_millis(600);

/// O aviso ao ponto de encontro, e o socket por onde ele saiu.
///
/// Antes disto havia uma função só, `bater`, que abria o socket, resolvia o nome
/// e mandava dois avisos, tudo antes do laço de candidatos. O furo abria por
/// 600 ms e o aperto de mão chegava até doze segundos depois — o defeito que
/// este ciclo existe para consertar.
///
/// A separação é o conserto: **preparar uma vez, avisar por candidato**. O
/// socket tem de ser um só porque o NAT mapeia por porta interna, e um aviso que
/// saísse de outra porta faria o anfitrião furar o caminho errado.
///
/// O socket vive num `Arc` porque a Tarefa 7 clona a `Batida` inteira para
/// repetir o aviso numa tarefa de fundo enquanto o laço de candidatos corre em
/// primeiro plano — as duas metades precisam do mesmo socket, não de uma cópia
/// dele.
#[derive(Clone)]
pub(crate) struct Batida {
    socket: Arc<tokio::net::UdpSocket>,
    /// O ponto de encontro, **como `resolver` devolveu** — sem passar por
    /// `mapear`. É a forma que um operador reconhece num log; a forma mapeada
    /// (`::ffff:a.b.c.d` num socket de pilha dupla) só faz sentido para o
    /// kernel, na hora de mandar. `avisar` mapeia de novo, a cada chamada,
    /// bem em cima do envio — ver o comentário lá.
    ponto: SocketAddr,
    /// Para onde o anfitrião deve furar o caminho, só para diagnóstico: é o
    /// mesmo valor que já foi para dentro do `LEVE`, em `datagrama`, mas
    /// reencontrá-lo ali exigiria decodificar o próprio pacote.
    aviso: SocketAddr,
    datagrama: Vec<u8>,
}

impl Batida {
    /// Abre o socket e resolve o ponto de encontro. **Não manda nada.**
    ///
    /// `None` quando não dá para bater — nome que não resolve, convite sem
    /// impressão digital, máquina sem rota. Nesse caso quem chama conecta como
    /// sempre conectou: nenhum endereço do convite depende disto, e o degrau 4 é
    /// o único que se perde.
    ///
    /// # Por que a impressão digital é obrigatória aqui
    ///
    /// A marca do aviso são os primeiros dígitos dela, e é o que diz ao
    /// anfitrião que quem bateu tem o link dele. Sem impressão digital o aviso
    /// chegaria com uma marca que o anfitrião não reconhece, e ele o ignoraria —
    /// mandar o pacote assim mesmo seria gastar rede para produzir silêncio.
    pub(crate) async fn preparar(
        bilhete: &Bilhete,
        impressao_digital: Option<&str>,
    ) -> Option<Self> {
        let marca = impressao_digital
            .and_then(|impressao| impressao.get(..16))
            .and_then(Marca::nova)?;
        let aviso = bilhete.aviso().ok()?;
        let ponto = tokio::time::timeout(PRAZO, resolver(bilhete))
            .await
            .ok()??;

        let socket = abrir_socket_local()?;
        let socket = tokio::net::UdpSocket::from_std(socket).ok()?;

        let datagrama = encontro::leve(aviso, &marca);

        Some(Self {
            socket: Arc::new(socket),
            ponto,
            aviso,
            datagrama,
        })
    }

    /// Manda **um** datagrama de 96 bytes ao ponto de encontro.
    ///
    /// `async`, mas não por causa de ida e volta de rede: não há resposta
    /// nenhuma a aguardar, e quem chama continua com um aperto de mão para
    /// começar assim que `avisar` retorna. O `.await` aqui é só o tempo de o
    /// socket aceitar escrita — o `send_to` do tokio cuida disso por dentro.
    ///
    /// A versão anterior usava `try_send_to`, não-bloqueante, para evitar
    /// exatamente essa espera. Era a coisa errada a evitar: no caminho de
    /// produção, quando o primeiro candidato do convite já é o refletido (uma
    /// casa atrás de CGNAT, sem IPv6 e sem UPnP — o caso que o degrau 4 existe
    /// para servir), `avisar` é chamado logo depois de `preparar`, sem nenhum
    /// `.await` entre os dois. O socket, recém-registrado no reator, ainda não
    /// tinha passado por um ciclo dele, e `try_send_to` podia devolver
    /// `WouldBlock` à toa — não por a rede estar ocupada, mas por o reator
    /// ainda não ter marcado o socket como pronto. O aviso não saía, o
    /// anfitrião nunca furava, e o candidato queimava o prazo inteiro
    /// esperando uma resposta que nunca viria por um caminho que nunca abriu:
    /// um sucesso mentiroso, silencioso, com o erro final apontando para outro
    /// endereço. Esperar os poucos microssegundos de `.await` é sempre mais
    /// barato que essa mentira.
    ///
    /// # Quem decide o que fazer com o erro não é esta função
    ///
    /// `avisar` devolve o `Result` em vez de engolir o erro — ela mesma já
    /// engoliu um `WouldBlock` de mais numa rodada anterior deste ciclo, e a
    /// correção foi trocar o `try_send_to` pelo `send_to` que espera, não
    /// decidir por quem chama que qualquer falha é inofensiva. Um
    /// `ENETUNREACH`, um destino de família errada, um socket fechado — nada
    /// disso é `WouldBlock`, e fingir que é viraria o mesmo sucesso mentiroso
    /// de antes, um nível acima.
    ///
    /// O peso do erro muda com quem chama. Aqui no invólucro `bater`, uma
    /// falha cancela a conexão inteira — é o que o código de antes desta
    /// separação fazia, e não é desta tarefa mudar isso. Na Tarefa 7,
    /// chamada uma vez por candidato dentro de um laço, a mesma falha não
    /// pode ter esse peso: perder o aviso de **um** candidato não pode
    /// derrubar os outros. Por isso a decisão fica de fora — `avisar` só
    /// manda e informa.
    pub(crate) async fn avisar(&self) -> std::io::Result<()> {
        // Mapeado aqui, a cada envio, e não uma vez só em `preparar`: é isto
        // que deixa `self.ponto` guardado na forma crua (ver o campo `ponto`,
        // na struct) — a única forma que um log consegue mostrar de um jeito
        // que um operador reconhece.
        let destino = mapear(self.ponto, &self.socket);
        self.socket.send_to(&self.datagrama, destino).await?;
        Ok(())
    }

    /// O socket por onde o aviso saiu.
    ///
    /// Quem conecta em seguida tem de conectar por ele: o anfitrião abriu
    /// caminho para **esta** porta, e um aperto de mão saindo de outra
    /// continuaria batendo numa porta fechada.
    ///
    /// Devolve o `Arc`, não uma referência ao socket por dentro dele: é assim
    /// que a tarefa de fundo da Tarefa 7 fica dona do mesmo socket sem precisar
    /// de um `try_clone` que o `tokio::net::UdpSocket` nem oferece.
    ///
    /// Quem consome isto é o laço de candidatos, na Tarefa 7 — `bater`, o
    /// invólucro temporário desta tarefa, não precisa mais dele depois que
    /// `avisar` passou a esperar a própria prontidão do socket.
    // Só sob `cfg(test)`, os próprios testes chamam `socket()` — fora dele, até
    // a Tarefa 7 migrar o laço de candidatos para usá-lo, ele fica sem
    // chamador nenhum. Por isso o `expect` também só se aplica fora de teste:
    // dentro, a expectativa nunca se cumpriria (o lint não dispara porque o
    // método está em uso), e um `expect` que nunca se cumpre é ele mesmo um
    // aviso.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "consumido pelo laço de candidatos da Tarefa 7; até lá, só os testes o chamam"
        )
    )]
    pub(crate) fn socket(&self) -> &Arc<tokio::net::UdpSocket> {
        &self.socket
    }
}

/// Bate no ponto de encontro do convite, e devolve o socket por onde bateu.
///
/// invólucro temporário; some na tarefa do aviso por candidato — hoje só
/// prepara e manda um aviso, para os chamadores em `enlace.rs` continuarem
/// compilando enquanto não migram para `Batida::preparar` + `Batida::avisar`
/// por tentativa.
pub async fn bater(
    bilhete: &Bilhete,
    impressao_digital: Option<&str>,
) -> Option<std::net::UdpSocket> {
    let batida = Batida::preparar(bilhete, impressao_digital).await?;
    if let Err(erro) = batida.avisar().await {
        tracing::info!(%erro, ponto = %batida.ponto, "não deu para avisar o ponto de encontro");
        return None;
    }
    tracing::info!(ponto = %batida.ponto, aviso = %batida.aviso, "degrau 4: avisamos o ponto de encontro de que estamos chegando");

    let Batida { socket, .. } = batida;
    // De volta a um socket da `std`, porque é isso que o quinn adota. Só dá
    // para desembrulhar o `Arc` porque nada mais o segura por aqui — e é
    // sempre assim hoje, com `bater` como único dono. Se um dia isto deixar de
    // valer (por exemplo, a Tarefa 7 migrar mal e manter `bater` vivo com um
    // clone rodando numa tarefa de fundo), o sintoma vira um `None` mudo sem
    // este log — daí o `else`, em vez de só `.ok()?`.
    let Ok(socket) = Arc::try_unwrap(socket) else {
        tracing::debug!("o socket do degrau 4 ainda tinha outro dono ao desembrulhar");
        return None;
    };
    socket.into_std().ok()
}

/// Onde o ponto de encontro atende.
async fn resolver(bilhete: &Bilhete) -> Option<SocketAddr> {
    let alvo = bilhete.ponto().ok()?;
    tokio::net::lookup_host((alvo.maquina, alvo.porta))
        .await
        .ok()?
        .next()
}

/// Um socket local que alcança as duas famílias, como o do QUIC.
///
/// Mesmo raciocínio de `local_endpoint` em [`crate::client`], e pelo mesmo
/// motivo: um socket IPv4 não manda para destino IPv6 de jeito nenhum, e um
/// socket IPv6 só manda para IPv4 com o `IPV6_V6ONLY` desligado — cujo padrão
/// muda de sistema para sistema (o degrau 2 do ADR 0022 mediu isso e apanhou).
///
/// A diferença é que aqui a opção é escrita à mão em vez de herdada do quinn,
/// porque é este código que abre o socket. Uma máquina sem IPv6 cai para IPv4, e
/// continua exatamente como estava.
fn abrir_socket_local() -> Option<std::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let seis = || -> Option<std::net::UdpSocket> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP)).ok()?;
        socket.set_only_v6(false).ok()?;
        socket
            .bind(&SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0)).into())
            .ok()?;
        Some(socket.into())
    };
    let quatro = || std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).ok();

    let socket = seis().or_else(quatro)?;
    socket.set_nonblocking(true).ok()?;
    Some(socket)
}

/// Um destino IPv4 escrito como o socket local o entende.
///
/// Num socket IPv6 de pilha dupla, um endereço IPv4 precisa ir na forma mapeada
/// (`::ffff:a.b.c.d`) — é o que o quinn faz por dentro, e aqui é à mão porque o
/// socket é nosso.
fn mapear(destino: SocketAddr, socket: &tokio::net::UdpSocket) -> SocketAddr {
    let local_e_seis = socket.local_addr().is_ok_and(|local| local.is_ipv6());
    match (local_e_seis, destino.ip()) {
        (true, IpAddr::V4(quatro)) => {
            SocketAddr::new(quatro.to_ipv6_mapped().into(), destino.port())
        }
        _ => destino,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn bilhete(ponto: &str) -> Bilhete {
        Bilhete::novo(ponto, "45.33.32.156:41234").expect("bilhete de teste")
    }

    /// Um `Bilhete` cujo ponto de encontro é `onde`, e cujo aviso é qualquer
    /// endereço global — `preparar` nunca lê `aviso`, só `ponto`.
    fn bilhete_de_teste(onde: SocketAddr) -> Bilhete {
        bilhete(&onde.to_string())
    }

    /// Uma impressão digital de teste, com pelo menos 16 caracteres — é dela
    /// que `preparar` tira a marca do aviso.
    const IMPRESSAO_DE_TESTE: &str =
        "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";
    const FP: &str = IMPRESSAO_DE_TESTE;

    #[tokio::test]
    async fn sem_impressao_digital_nao_se_bate_em_ponto_nenhum() {
        // A marca sai da impressão digital, e sem ela o aviso chegaria com uma
        // etiqueta que o anfitrião não reconhece — rede gasta para produzir
        // silêncio. E é a asserção que garante que nenhum pacote sai daqui por
        // um link que não prometeu identidade nenhuma.
        assert!(bater(&bilhete("192.0.2.1:8384"), None).await.is_none());
        assert!(bater(&bilhete("192.0.2.1:8384"), Some("curto"))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn um_ponto_de_encontro_que_nao_resolve_nao_segura_a_conexao() {
        // O requisito do ADR 0022 deste lado: o degrau 4 não pode virar ponto
        // único de falha. Um nome que não existe custa o que a resolução custa,
        // e nunca mais que o prazo.
        let comeco = std::time::Instant::now();
        let batido = bater(&bilhete("nao-existe-mesmo.invalid:8384"), Some(FP)).await;
        assert!(batido.is_none(), "um nome inexistente devolveu um socket");
        assert!(
            comeco.elapsed() < PRAZO * 2,
            "a conexão ficou presa {:?} num ponto de encontro que não existe",
            comeco.elapsed()
        );
    }

    #[tokio::test]
    async fn bater_devolve_o_socket_por_onde_bateu() {
        // A propriedade que faz o furo funcionar: quem conecta em seguida tem de
        // conectar **por este socket**, ou o anfitrião fura o caminho para a
        // porta errada. O ponto de encontro aqui é um socket qualquer no
        // loopback: nada precisa responder, porque este lado não lê resposta.
        let fingido = std::net::UdpSocket::bind("127.0.0.1:0").expect("loopback");
        let onde = fingido.local_addr().expect("endereço");

        let socket = bater(&bilhete(&onde.to_string()), Some(FP))
            .await
            .expect("bater num ponto de encontro que existe");
        let local = socket.local_addr().expect("o socket não diz onde ligou");
        assert_ne!(
            local.port(),
            0,
            "o socket não ficou ligado em porta nenhuma"
        );

        // E o que chegou lá é um `LEVE` com a marca do convite, apontando para o
        // endereço de avisos do anfitrião.
        fingido
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("prazo");
        let mut balde = [0_u8; encontro::TAMANHO];
        let (lidos, de) = fingido.recv_from(&mut balde).expect("nada chegou ao ponto");
        assert_eq!(
            de.port(),
            local.port(),
            "o aviso saiu de outro socket que não o que vai conectar"
        );
        let Some(seele_proto::encontro::Pedido::Leve { destino, marca }) =
            balde.get(..lidos).and_then(seele_proto::encontro::analisar)
        else {
            panic!("chegou outra coisa ao ponto de encontro");
        };
        assert_eq!(destino.to_string(), "45.33.32.156:41234");
        assert_eq!(marca.texto(), &FP[..16]);
    }

    #[tokio::test]
    async fn preparar_abre_o_socket_e_nao_manda_pacote_nenhum() {
        // A separação existe para o aviso poder sair colado em cada candidato. Se
        // `preparar` mandasse um aviso, o primeiro candidato — que é o da rede de
        // casa e nunca precisou de furo — pagaria metadado e um furo da janela do
        // anfitrião por nada.
        //
        // O ponto de encontro deste teste é um socket nosso que nunca lê: o que se
        // afirma é que nada chegou nele.
        let ponto = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok();
        let Some(ponto) = ponto else { return };
        let Ok(onde) = ponto.local_addr() else { return };

        let bilhete = bilhete_de_teste(onde);
        let batida = Batida::preparar(&bilhete, Some(IMPRESSAO_DE_TESTE)).await;
        let Some(batida) = batida else {
            panic!("preparar tem de dar certo com um ponto de encontro que existe");
        };

        let mut balde = [0_u8; seele_proto::encontro::TAMANHO];
        let nada = tokio::time::timeout(
            std::time::Duration::from_millis(120),
            ponto.recv_from(&mut balde),
        )
        .await;
        assert!(nada.is_err(), "preparar não manda pacote nenhum");

        // E `avisar` manda exatamente um, do tamanho fixo do protocolo, e
        // devolve sucesso.
        let enviado = batida.avisar().await;
        assert!(enviado.is_ok(), "avisar não devia falhar aqui: {enviado:?}");
        let chegou = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            ponto.recv_from(&mut balde),
        )
        .await;
        let Ok(Ok((lidos, _))) = chegou else {
            panic!("avisar tem de mandar um datagrama");
        };
        assert_eq!(lidos, seele_proto::encontro::TAMANHO);
    }

    #[tokio::test]
    async fn a_batida_clonada_avisa_pelo_mesmo_socket() {
        // A Tarefa 7 clona a `Batida` para repetir o aviso numa tarefa de fundo
        // enquanto o laço de candidatos corre em primeiro plano. As duas cópias
        // têm de compartilhar o mesmo socket — é o `Arc` que garante isso, não
        // uma cópia independente que o NAT não reconheceria.
        let ponto = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok();
        let Some(ponto) = ponto else { return };
        let Ok(onde) = ponto.local_addr() else { return };

        let bilhete = bilhete_de_teste(onde);
        let batida = Batida::preparar(&bilhete, Some(IMPRESSAO_DE_TESTE)).await;
        let Some(batida) = batida else {
            panic!("preparar tem de dar certo com um ponto de encontro que existe");
        };
        let copia = batida.clone();

        assert!(
            Arc::ptr_eq(batida.socket(), copia.socket()),
            "a cópia da batida tem de apontar para o mesmo socket"
        );
    }

    #[tokio::test]
    async fn um_aviso_logo_depois_de_preparar_nao_se_perde() {
        // O caminho que mordeu de verdade: um convite cujo primeiro candidato já
        // é o refletido — casa atrás de CGNAT, sem IPv6, sem UPnP — chama
        // `avisar` logo depois de `preparar`, sem nenhum `.await` no meio. É
        // exatamente como a Tarefa 7 chama isto:
        //
        // ```
        // if let Some(batida) = batida { let _ = batida.avisar().await; }
        // tokio::time::sleep(ESPERA_DO_FURO).await;
        // ```
        //
        // Um `try_send_to` não-bloqueante logo ali devolvia `WouldBlock` à toa —
        // o socket, recém-registrado, ainda não tinha passado por um ciclo do
        // reator — e o aviso simplesmente não saía, sem que nada no chamador
        // percebesse. Este teste reprova sozinho se `avisar` voltar a ser
        // não-bloqueante: nesta plataforma (macOS/kqueue), sem o `.await`
        // interno de `send_to`, a asserção de baixo falhou nas 5 rodadas em
        // que foi medida — determinístico aqui, não uma flakiness ocasional.
        let ponto = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok();
        let Some(ponto) = ponto else { return };
        let Ok(onde) = ponto.local_addr() else { return };

        let bilhete = bilhete_de_teste(onde);
        let batida = Batida::preparar(&bilhete, Some(IMPRESSAO_DE_TESTE)).await;
        let Some(batida) = batida else {
            panic!("preparar tem de dar certo com um ponto de encontro que existe");
        };

        // Nenhum `.await` entre `preparar` e `avisar` além do que `avisar` já
        // faz por dentro — é essa ausência que reproduz a corrida.
        let mut balde = [0_u8; seele_proto::encontro::TAMANHO];
        let enviado = batida.avisar().await;
        assert!(enviado.is_ok(), "avisar não devia falhar aqui: {enviado:?}");
        let chegou = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            ponto.recv_from(&mut balde),
        )
        .await;
        let Ok(Ok((lidos, _))) = chegou else {
            panic!("o aviso mandado logo depois de preparar se perdeu");
        };
        assert_eq!(lidos, seele_proto::encontro::TAMANHO);
    }

    #[tokio::test]
    async fn avisar_devolve_o_erro_em_vez_de_engoli_lo() {
        // Regressão da rodada 2: `avisar` chegou a engolir **todo** erro num
        // `tracing::debug!`, não só o `WouldBlock` que a rodada 1 tinha em
        // mira — e quem chama nunca saberia que o aviso não saiu.
        //
        // Um destino de broadcast é rejeitado pelo próprio kernel, na hora,
        // sem round-trip de rede nenhum: este socket nunca liga
        // `SO_BROADCAST`, e mandar para `255.255.255.255` sem essa opção
        // devolve permissão negada — comportamento de BSD sockets (macOS,
        // Linux) e do WinSock equivalente no Windows, não algo que dependa de
        // rede de verdade estar presente.
        //
        // O caminho que dispara o erro passa por `mapear`: num socket de
        // pilha dupla (o caso comum de `abrir_socket_local`), `avisar` manda
        // para `[::ffff:255.255.255.255]:9`, não para o v4 puro — é essa
        // forma mapeada que o kernel avalia contra `SO_BROADCAST` e recusa.
        // Sem esse mapeamento (por exemplo, numa máquina sem IPv6, caindo
        // para o socket v4 de `abrir_socket_local`), o destino vai cru e o
        // erro medido foi outro: `InvalidInput`, não `PermissionDenied` — os
        // dois são erros, então a asserção de baixo (`is_err`) não distingue
        // e continua correta nos dois ramos; só a causa exata muda. Medido
        // nesta plataforma (macOS) nos dois ramos de `abrir_socket_local`.
        let bilhete = bilhete("255.255.255.255:9");
        let batida = Batida::preparar(&bilhete, Some(IMPRESSAO_DE_TESTE)).await;
        let Some(batida) = batida else {
            panic!(
                "preparar tem de dar certo mesmo quando o ponto de encontro é \
                 um endereço de broadcast — é `avisar`, mandando para lá, que \
                 não vai conseguir; `aviso` (para onde o anfitrião deve furar) \
                 é um endereço comum, e não tem nada a ver com isto"
            );
        };

        let erro = batida.avisar().await;
        assert!(
            erro.is_err(),
            "avisar engoliu o erro de um envio que o kernel recusou"
        );
    }

    #[tokio::test]
    async fn bater_nao_devolve_socket_quando_o_aviso_falha() {
        // A consequência observável, no invólucro, do conserto acima: era o
        // comportamento do código de antes desta separação (`tracing::info!`
        // e `return None` no primeiro envio que falhasse), perdido quando
        // `avisar` passou a engolir todo erro e `bater` seguia adiante
        // afirmando sucesso mesmo sem ele.
        //
        // `bater` devolve `None` por seis motivos diferentes, e «o aviso
        // falhou» é só um deles. Sem confirmar que `preparar` chega a dar
        // certo para este mesmo bilhete, um `None` por qualquer um dos outros
        // cinco (o `lookup_host` do endereço de broadcast recusando o
        // literal, o socket não abrindo, ...) passaria por aqui como se
        // tivesse testado a propriedade certa — vazio, mas verde.
        assert!(
            Batida::preparar(&bilhete("255.255.255.255:9"), Some(FP))
                .await
                .is_some(),
            "preparar tem de dar certo para este bilhete — senão o `None` de \
             `bater` abaixo pode vir de `preparar`, não de `avisar`, e este \
             teste para de testar o que diz testar"
        );

        let batido = bater(&bilhete("255.255.255.255:9"), Some(FP)).await;
        assert!(
            batido.is_none(),
            "bater devolveu um socket mesmo com o envio do aviso recusado pelo kernel"
        );
    }
}
