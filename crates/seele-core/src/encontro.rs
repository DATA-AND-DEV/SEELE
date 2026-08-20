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
    /// O peso do erro muda com quem chama, e é por isso que a decisão fica de
    /// fora. O invólucro `bater` que existia aqui até a Tarefa 7 cancelava a
    /// conexão inteira ao primeiro envio recusado — ele mandava um aviso só,
    /// antes do laço, e sem ele não havia degrau 4 nenhum. Quem chama hoje é o
    /// laço de candidatos de [`crate::enlace`], uma vez por candidato que
    /// precisa de furo, e ali a mesma falha não pode ter esse peso: um aviso
    /// que não sai para o candidato 2 não tem nada a ver com o candidato 3, e
    /// derrubar a conexão por causa dele trocaria um defeito por outro. Lá o
    /// erro é registrado e o laço segue. `avisar` só manda e informa.
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
    /// que a tarefa de fundo que repete o aviso fica dona do mesmo socket sem
    /// precisar de um `try_clone` que o `tokio::net::UdpSocket` nem oferece.
    ///
    /// Quem conecta não usa isto e sim [`Batida::emprestar_socket`]: o quinn
    /// adota um socket da `std`, não um do tokio.
    ///
    /// Por isso o método **só existe sob `cfg(test)`**. O laço de candidatos
    /// empresta o descritor e nunca precisa do `Arc` em si; fora de teste isto
    /// é código morto, e um `expect(dead_code)` renovado a cada rodada seria
    /// só uma forma educada de manter código morto no lugar.
    #[cfg(test)]
    pub(crate) fn socket(&self) -> &Arc<tokio::net::UdpSocket> {
        &self.socket
    }

    /// Um segundo descritor para a **mesma** porta, na forma que o quinn adota.
    ///
    /// É o `try_clone` de sempre, e o motivo dele também: um `quinn::Endpoint`
    /// fecha o socket que adotou quando é recolhido, e cada candidato do
    /// convite monta um `Endpoint` novo. Sem uma cópia por tentativa, a porta
    /// que o anfitrião acabou de furar voltaria para o sistema no meio do laço
    /// — e a tentativa seguinte sairia de outra porta, para a qual ninguém
    /// furou nada.
    ///
    /// A cópia é feita do descritor emprestado (`SockRef`), e não do
    /// `tokio::net::UdpSocket`, porque o tokio não oferece `try_clone` e
    /// desembrulhar o `Arc` mataria o original — que precisa continuar vivo
    /// até o fim do laço, ou o sistema devolveria a porta furada assim que a
    /// última tentativa terminasse.
    ///
    /// `None` quando o sistema recusa a cópia. Aí a tentativa sai de um socket
    /// novo, como quem não bateu em ponto de encontro nenhum: pior, mas não
    /// pior do que não tentar.
    pub(crate) fn emprestar_socket(&self) -> Option<std::net::UdpSocket> {
        let copia = socket2::SockRef::from(self.socket.as_ref())
            .try_clone()
            .ok()?;
        Some(copia.into())
    }

    /// Onde o aviso foi mandado, para o log de quem chama.
    pub(crate) fn ponto(&self) -> SocketAddr {
        self.ponto
    }

    /// Para onde o anfitrião foi convidado a furar, para o log de quem chama.
    pub(crate) fn aviso(&self) -> SocketAddr {
        self.aviso
    }
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
        assert!(Batida::preparar(&bilhete("192.0.2.1:8384"), None)
            .await
            .is_none());
        assert!(Batida::preparar(&bilhete("192.0.2.1:8384"), Some("curto"))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn um_ponto_de_encontro_que_nao_resolve_nao_segura_a_conexao() {
        // O requisito do ADR 0022 deste lado: o degrau 4 não pode virar ponto
        // único de falha. Um nome que não existe custa o que a resolução custa,
        // e nunca mais que o prazo.
        let comeco = std::time::Instant::now();
        let batida = Batida::preparar(&bilhete("nao-existe-mesmo.invalid:8384"), Some(FP)).await;
        assert!(batida.is_none(), "um nome inexistente virou uma batida");
        assert!(
            comeco.elapsed() < PRAZO * 2,
            "a conexão ficou presa {:?} num ponto de encontro que não existe",
            comeco.elapsed()
        );
    }

    #[tokio::test]
    async fn o_socket_emprestado_e_a_mesma_porta_de_onde_o_aviso_saiu() {
        // A propriedade que faz o furo funcionar: quem conecta em seguida tem de
        // conectar **por esta porta**, ou o anfitrião fura o caminho para a
        // porta errada. O ponto de encontro aqui é um socket qualquer no
        // loopback: nada precisa responder, porque este lado não lê resposta.
        //
        // Isto é a metade de baixo da propriedade — que o descritor emprestado
        // é o mesmo socket. A metade de cima, que o laço de candidatos
        // realmente conecta por ele, é
        // `o_aperto_de_mao_sai_da_mesma_porta_que_bateu_no_ponto_de_encontro`,
        // em `enlace.rs`: um `emprestar_socket` correto que ninguém chamasse
        // deixaria este teste verde e o furo quebrado do mesmo jeito.
        let Ok(fingido) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("o loopback não abriu");
        };
        let Ok(onde) = fingido.local_addr() else {
            panic!("o ponto de encontro de teste não tem endereço local");
        };

        let batida = Batida::preparar(&bilhete(&onde.to_string()), Some(FP)).await;
        let Some(batida) = batida else {
            panic!("preparar tem de dar certo com um ponto de encontro que existe");
        };
        let saiu = batida.avisar().await;
        assert!(saiu.is_ok(), "o aviso não saiu: {saiu:?}");
        let Some(socket) = batida.emprestar_socket() else {
            panic!("o sistema recusou emprestar um segundo descritor da porta");
        };
        let Ok(local) = socket.local_addr() else {
            panic!("o socket emprestado não diz onde ligou");
        };
        assert_ne!(
            local.port(),
            0,
            "o socket não ficou ligado em porta nenhuma"
        );

        // E o que chegou lá é um `LEVE` com a marca do convite, apontando para o
        // endereço de avisos do anfitrião.
        let prazo = fingido.set_read_timeout(Some(Duration::from_secs(2)));
        assert!(prazo.is_ok(), "o prazo de leitura não pegou");
        let mut balde = [0_u8; encontro::TAMANHO];
        let Ok((lidos, de)) = fingido.recv_from(&mut balde) else {
            panic!("nada chegou ao ponto de encontro");
        };
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
        // Os **dois** ramos de `abrir_socket_local` dão `PermissionDenied`:
        // o de pilha dupla mandando para `[::ffff:255.255.255.255]:9` e o v4
        // puro (a máquina sem IPv6) mandando para `255.255.255.255:9` — os
        // dois medidos nesta plataforma (macOS), os dois `EACCES`. O
        // `InvalidInput` que este comentário já afirmou não sai de ramo
        // nenhum daqui: ele só apareceria numa terceira configuração, que é o
        // socket de pilha dupla com o destino v4 **não** mapeado — isto é, o
        // que aconteceria se `mapear` saísse de `avisar`. A asserção de baixo
        // (`is_err`) vale para os três, e é de propósito: o que se afirma é
        // que o erro chega a quem chama, não qual erro é.
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
    async fn preparar_da_certo_no_mesmo_bilhete_em_que_avisar_falha() {
        // O guarda do teste de cima. `avisar_devolve_o_erro_em_vez_de_engoli_lo`
        // afirma que um envio recusado pelo kernel chega a quem chama; se
        // `preparar` deixasse de dar certo para este bilhete, aquele teste
        // pararia de rodar `avisar` de verdade e passaria por outro caminho.
        //
        // Quem consome esse erro é o laço de candidatos de `enlace.rs`, e o que
        // ele faz com ele — registrar e ir ao candidato seguinte, sem derrubar
        // a conexão — está em `um_aviso_recusado_pelo_kernel_nao_derruba_o_laco`,
        // em `crates/seele-conformance/tests/furo.rs`.
        assert!(
            Batida::preparar(&bilhete("255.255.255.255:9"), Some(FP))
                .await
                .is_some(),
            "preparar tem de dar certo para este bilhete — é `avisar`, mandando \
             para o broadcast, que não pode dar"
        );
    }
}
