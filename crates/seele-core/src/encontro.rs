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
//! pelo qual o outro lado precisou de um espelho do socket do servidor.
//!
//! # O que este lado lê do ponto de encontro, e o que não lê
//!
//! **A batida não lê resposta nenhuma.** O `LEVE` é de mão única, e quem entra
//! não espera resposta a ele.
//!
//! **A consulta ao quarto lê**, e é o único caminho por onde um ponto de
//! encontro põe um endereço na lista de candidatos ([`onde_mora_hoje`]). Um
//! ponto hostil, ou quem ocupou a marca no quarto, consegue mandar quem chega
//! para o endereço errado. O que impede isso de virar conexão com um impostor é
//! a impressão digital, conferida dentro do TLS antes de qualquer `Hello`, e ela
//! só protege quando existe: a esperada vem do link desta sessão ou, na volta
//! pela lista, da impressão que a lista guardou (análise de 22/09/2026, §3.1,
//! S3). Vale também num endereço que esta máquina já fixou com a chave de outro
//! servidor: o endereço do quarto, quando entra na corrida, é um candidato que
//! a pessoa não escolheu, e ali o pino que confere não passa por cima da
//! esperada (o adendo de 2026-09-29 ao ADR 0003, `crate::tofu::TofuVerifier`).
//!
//! A mesma resposta traz a escuta de avisos, que vira o destino do `LEVE`, e
//! essa não tem TLS atrás: o `LEVE` sai antes do aperto de mão, com o endereço
//! de quem chega. Ela só vale no IP do servidor que a mesma resposta deu
//! ([`OndeMora::escuta_do_anfitriao`], [`bilhete_desta_volta`]).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use seele_proto::encontro::{self};
use seele_proto::uri::Bilhete;

// `Marca` e `Marcas` reexportadas: quem chama `onde_mora_hoje` de fora precisa
// das marcas prontas, e fazê-las vir daqui poupa a camada de cima de nomear
// `seele-proto`, que o ADR 0002 não deixa a casca alcançar de qualquer jeito.
pub use seele_proto::encontro::{Marca, Marcas};

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
        // A marca do aviso sai da mesma função com que o anfitrião a confere
        // ([`Marcas::do_servidor`]): duas derivações da mesma regra são duas
        // regras para discordarem no dia em que uma mudar.
        let marca = impressao_digital
            .and_then(Marcas::do_servidor)?
            .aviso()
            .clone();
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
    let aviso = bilhete.aviso().ok()?;
    let achados: Vec<SocketAddr> = tokio::net::lookup_host((alvo.maquina, alvo.porta))
        .await
        .ok()?
        .collect();
    escolher_ponto(&achados, aviso)
}

/// Qual endereço do ponto de encontro usar, entre os que o DNS devolveu.
///
/// **O da família do anfitrião**, e não o primeiro. O pedido que sai daqui é um
/// `LEVE`: ele manda o ponto de encontro avisar o anfitrião do endereço em que
/// esta máquina foi vista. Esse endereço é o que o anfitrião vai furar, e o furo
/// só serve se ele couber no mesmo caminho por onde o QUIC vai tentar passar.
///
/// Com `.next()` — que era o que estava aqui — quem chega por uma rede com IPv6
/// falava com o ponto de encontro por IPv6 para pedir um aviso a um anfitrião
/// IPv4. Duas coisas quebravam de uma vez: o ponto de encontro não tem por onde
/// repassar um aviso que cruza famílias, e mesmo se tivesse, o endereço
/// anunciado seria um IPv6 que o anfitrião IPv4 não alcança — um furo aberto no
/// lugar errado, enquanto o QUIC tenta o outro.
///
/// Sem nenhum da família certa, vale o primeiro: é o que havia antes, e falhar
/// ali é melhor que não tentar.
fn escolher_ponto(achados: &[SocketAddr], aviso: SocketAddr) -> Option<SocketAddr> {
    let mesma_familia = |ponto: &&SocketAddr| ponto.is_ipv4() == aviso.is_ipv4();
    achados
        .iter()
        .find(mesma_familia)
        .or_else(|| achados.first())
        .copied()
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

/// Onde o ponto de encontro atende, a partir do texto que o link carrega.
///
/// **Com a porta padrão quando ela falta**, pela mesma regra de
/// [`Bilhete::ponto`] ([`seele_proto::uri::separar_ponto`]). O texto ia cru
/// para `lookup_host`, que recusa nome sem porta, e como o link sai sem porta
/// (`enc=encontro.seele.app.br/…`) a pergunta ao quarto nunca saiu.
///
/// **Todos os endereços, IPv4 primeiro** ([`ordenar_os_achados`]). A consulta
/// pergunta ao primeiro, e ao seguinte só quando nada saiu ([`consultar`]).
///
/// **Diz por que não achou.** O defeito passou sem ser visto porque o `.ok()?`
/// engolia o erro: o produto sabia por que a pergunta não saía e não contava.
/// Cada lista vazia daqui deixa a causa no `seele.log` (que só grava `info`), e
/// quem lê o log distingue um texto que não é endereço de um nome que não
/// resolve.
async fn resolver_ponto(ponto: &str) -> Vec<SocketAddr> {
    let alvo = match seele_proto::uri::separar_ponto(ponto) {
        Ok(alvo) => alvo,
        Err(erro) => {
            tracing::info!(
                ponto,
                %erro,
                "o ponto de encontro do link não é um endereço: a pergunta ao quarto não saiu"
            );
            return Vec::new();
        }
    };
    match tokio::net::lookup_host((alvo.maquina, alvo.porta)).await {
        Ok(achados) => ordenar_os_achados(ponto, achados.collect()),
        Err(erro) => {
            tracing::info!(
                ponto,
                %erro,
                "o ponto de encontro do link não resolveu: a pergunta ao quarto não saiu"
            );
            Vec::new()
        }
    }
}

/// Os endereços que o nome do ponto deu, na ordem em que a consulta os tenta.
///
/// **IPv4 primeiro**, e a ordem do DNS não decide isto: numa máquina sem rota
/// IPv6, o AAAA na frente engole a pergunta em silêncio quando o envio dá certo
/// e o pacote some. O anfitrião aprendeu isso em campo (`resolver`, no
/// `seele-server`).
///
/// **Diz quando não sobrou nenhum.** Um nome que resolve para lista vazia não é
/// erro do DNS, e sem esta linha a pergunta ao quarto deixava de sair sem nada
/// no `seele.log`.
fn ordenar_os_achados(ponto: &str, mut achados: Vec<SocketAddr>) -> Vec<SocketAddr> {
    achados.sort_by_key(|achado| u8::from(achado.is_ipv6()));
    if achados.is_empty() {
        tracing::info!(
            ponto,
            "o nome do ponto de encontro resolveu para nenhum endereço: a pergunta ao quarto \
             não saiu"
        );
    }
    achados
}

/// Quanto se espera o quarto dizer onde um anfitrião mora.
///
/// Um segundo e meio, e é o teto, não o custo comum. As `ENVIOS_AO_QUARTO`
/// voltas cabem nele. Quanto cada consulta paga de fato está em
/// [`onde_mora_hoje`], caso a caso. O prazo inteiro é pago quando o ponto cala
/// nas três voltas, ou quando o nome dele não resolve a tempo, e aí o que sobra
/// são os endereços guardados.
pub const PRAZO_DO_QUARTO: Duration = Duration::from_millis(1500);

/// Quantas voltas de pergunta saem enquanto o ponto não responde.
///
/// Três, e não uma. Era uma, e um datagrama perdido custava a consulta
/// inteira em silêncio.
const ENVIOS_AO_QUARTO: u32 = 3;

/// O intervalo entre uma volta e a seguinte.
const INTERVALO_DOS_ENVIOS: Duration = Duration::from_millis(500);

/// O piso da espera pela marca que falta, depois da primeira resposta.
///
/// Num caminho curto o dobro da ida e volta são microssegundos, e isso não daria
/// ao ponto tempo nem de responder à segunda pergunta de uma rajada que saiu
/// junto com a primeira.
const FOLGA_MINIMA: Duration = Duration::from_millis(100);

/// Até quando se espera pela marca que falta, depois de o ponto responder.
///
/// A primeira resposta, mais o dobro da ida e volta que o `ONDE` da volta
/// mediu, nunca menos que [`FOLGA_MINIMA`]. As três perguntas saem juntas, e o
/// ponto responde a cada uma assim que ela chega: uma resposta que ainda não
/// veio depois de duas idas e voltas é uma pergunta que ele calou. Quem chama
/// não espera além da volta seguinte de qualquer jeito.
fn teto_depois_da_resposta(
    primeira: tokio::time::Instant,
    ida_e_volta: Duration,
) -> tokio::time::Instant {
    primeira + ida_e_volta.saturating_mul(2).max(FOLGA_MINIMA)
}

/// A marca do `ONDE` que corre junto das duas `QUEM`.
///
/// Oito caracteres, com letras fora do hexadecimal. Tem menos de dezesseis, e
/// por isso nenhuma marca de servidor (`fp16`, `fp16e`, `fp16s`) nem de código
/// (`p` e quinze símbolos, mais o sufixo) é igual a ela: a resposta dela nunca
/// se confunde com a de um morador.
const MARCA_DA_CONSULTA: &str = "consulta";

/// O que o quarto disse sobre onde um anfitrião mora hoje.
///
/// É um tipo, e não `(Option, Option)`, porque «não achei» tem causas que
/// apontam para lugares diferentes. O ponto estar fora do ar é problema de
/// rede, ou de quem opera o ponto. O ponto estar no ar e ninguém morar lá é o
/// anfitrião desligado, ou uma versão que registra outras marcas. Enquanto as
/// duas eram o mesmo `None`, quem investigava não tinha por onde começar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OndeMora {
    /// O ponto respondeu por pelo menos uma das marcas.
    ///
    /// Uma das duas pode faltar: um anfitrião 0.15.0 registra o servidor, e
    /// não registra a escuta com a marca que se pergunta.
    Achado {
        /// Onde o socket do servidor mora: para onde se conecta.
        servidor: Option<SocketAddr>,
        /// Onde o quarto diz que a escuta de avisos mora, **crua**: quem
        /// ocupou a marca da escuta também aparece aqui. Ela só vira o aviso do
        /// `LEVE` pela regra ([`OndeMora::escuta_do_anfitriao`], e o bilhete
        /// que a usa, [`bilhete_desta_volta`]).
        escuta: Option<SocketAddr>,
    },
    /// O ponto respondeu e calou para as duas marcas.
    ///
    /// Ele está no ar, e ninguém mora lá. Ou o anfitrião está fora do ar há
    /// mais que o prazo do quarto, ou o ponto é anterior ao quarto e não
    /// conhece `QUEM`.
    NinguemMora,
    /// O ponto não respondeu a nada dentro do prazo, **ou a pergunta nem saiu**.
    ///
    /// Os dois casos voltam iguais, e o segundo não é culpa do ponto: uma falha
    /// local (sem rota, um socket que o sistema não abriu ou recusou) também
    /// acaba aqui, e a consulta volta na hora em vez de esperar o prazo. Quem lê
    /// isto como «o ponto caiu» erra o diagnóstico. A causa de cada um está no
    /// log (`info`), e é lá que se distingue.
    PontoMudo,
    /// O texto do ponto não é um endereço, ou o nome não resolveu no prazo.
    PontoNaoResolve,
    /// Quem chamou não tinha como formar as marcas, e nenhuma pergunta saiu.
    ///
    /// [`onde_mora_hoje`] nunca devolve isto, porque recebe as marcas prontas.
    /// Quem devolve é quem as monta a partir de uma impressão digital que não
    /// forma marca.
    SemMarca,
}

impl OndeMora {
    /// O endereço do servidor, se o quarto o deu.
    #[must_use]
    pub fn servidor(&self) -> Option<SocketAddr> {
        match self {
            Self::Achado { servidor, .. } => *servidor,
            Self::NinguemMora | Self::PontoMudo | Self::PontoNaoResolve | Self::SemMarca => None,
        }
    }

    /// O endereço da escuta de avisos, se o quarto o deu, **cru**.
    ///
    /// É o que o quarto disse, e não serve de aviso do `LEVE`: quem ocupou a
    /// marca da escuta também é dito aqui. Para o aviso, a regra é
    /// [`OndeMora::escuta_do_anfitriao`], e o bilhete que a usa é
    /// [`bilhete_desta_volta`].
    #[must_use]
    pub fn escuta(&self) -> Option<SocketAddr> {
        match self {
            Self::Achado { escuta, .. } => *escuta,
            Self::NinguemMora | Self::PontoMudo | Self::PontoNaoResolve | Self::SemMarca => None,
        }
    }

    /// A escuta que o quarto deu, julgada pelo servidor que ele deu **na mesma
    /// resposta**.
    ///
    /// # Por que o servidor decide
    ///
    /// A escuta vira o aviso do `LEVE`, e o `LEVE` sai antes de qualquer aperto
    /// de mão: o ponto repassa a esse aviso o endereço público de quem chega, e
    /// o instante. A marca dela (os 16 primeiros caracteres da impressão
    /// digital, e um `e`) está em todo link, e no quarto fica quem escreveu
    /// primeiro. Um anfitrião 0.15.0 nunca a registra. A de um anfitrião de
    /// hoje fica livre quando ele passa mais de 60 s fora do ar; quando ele não
    /// se registra naquele ponto (com IPv4 global ele não abre o degrau 4, e
    /// `$SEELE_ENCONTRO` pode desligá-lo ou trocar o ponto, e o bilhete que a
    /// lista guardou continua apontando o ponto de antes); e logo depois de o
    /// ponto reiniciar, até o próximo registro dele, que sai a cada 15 s. As
    /// mesmas três janelas valem para a marca do servidor. Quem a tomou não
    /// fica sabendo de conteúdo nenhum, mas ficava sabendo de onde e quando
    /// cada um tentava chegar (o I1 da revisão final do Plano 1).
    ///
    /// A escuta e o servidor de um anfitrião moram na mesma máquina, e saem
    /// pelo mesmo IP público para o mesmo ponto: os dois sockets se registram
    /// juntos, no mesmo tique de `atender` (`registrar`, no `seele-server`).
    /// Com a escuta noutro IP que o servidor da mesma resposta, uma das duas
    /// marcas foi ocupada por outra pessoa (a da escuta ou a do servidor), ou o
    /// anfitrião sai por dois IPs públicos. A regra não sabe qual das três, e
    /// não usa a escuta: com a marca do servidor tomada, a escuta deixada de
    /// lado é a do anfitrião de verdade. Uma escuta sem servidor não tem quem
    /// a confirme: ou a marca do servidor está vazia, ou a resposta sobre ele
    /// não voltou a tempo.
    ///
    /// # O que sobra
    ///
    /// Quem toma **as duas** marcas passa por aqui: o servidor dele mora no IP
    /// dele. O TLS recusa o servidor errado logo depois, e o convite, a senha e
    /// o apelido não saem. O que sai antes é o `LEVE`, com o IP e a porta
    /// públicos de quem chega, até o SEELE-ENC/2 (Plano 4).
    ///
    /// **Compara o IP, e não a porta**: os dois sockets têm portas diferentes,
    /// e é a porta que o NAT troca. O IPv4 escrito como IPv6 mapeado
    /// (`::ffff:a.b.c.d`) é o mesmo IP.
    #[must_use]
    pub fn escuta_do_anfitriao(&self) -> EscutaDoQuarto {
        let Some(escuta) = self.escuta() else {
            return EscutaDoQuarto::Nenhuma;
        };
        match self.servidor() {
            Some(servidor) if servidor.ip().to_canonical() == escuta.ip().to_canonical() => {
                EscutaDoQuarto::DoAnfitriao(escuta)
            }
            servidor => EscutaDoQuarto::NaoConfirmada { escuta, servidor },
        }
    }
}

/// O que fazer com a escuta que o quarto deu: o veredito de
/// [`OndeMora::escuta_do_anfitriao`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscutaDoQuarto {
    /// O quarto não deu escuta. O aviso do bilhete guardado fica como estava.
    Nenhuma,
    /// A escuta mora no IP do servidor que a mesma resposta deu. Vira o aviso
    /// do `LEVE` desta volta.
    DoAnfitriao(SocketAddr),
    /// O quarto deu uma escuta que o servidor da mesma resposta não confirma:
    /// ou não veio servidor, ou ele mora noutro IP. Ela não vira aviso de nada.
    NaoConfirmada {
        /// A escuta que o quarto deu.
        escuta: SocketAddr,
        /// O servidor da mesma resposta, se veio.
        servidor: Option<SocketAddr>,
    },
}

/// O bilhete desta volta: o guardado, com o aviso trocado pela escuta que o
/// quarto deu **só** quando o servidor da mesma resposta a confirma.
///
/// O guardado é o do link desta sessão, ou o da lista de conhecidos. A troca
/// vale só para esta volta: quem chama não grava o bilhete devolvido na lista
/// (o `connect` do app grava o guardado). A regra é a de
/// [`OndeMora::escuta_do_anfitriao`].
///
/// Uma escuta deixada de lado vai para o log em `info`, o nível que o
/// `seele.log` grava, com ela, o servidor e o ponto: quem investiga um
/// anfitrião que não foi avisado precisa saber que o quarto deu outra escuta, e
/// por que ela não foi usada.
#[must_use]
pub fn bilhete_desta_volta(guardado: &Bilhete, no_quarto: &OndeMora) -> Bilhete {
    match no_quarto.escuta_do_anfitriao() {
        EscutaDoQuarto::Nenhuma => guardado.clone(),
        EscutaDoQuarto::DoAnfitriao(escuta) => {
            match Bilhete::novo(guardado.ponto.clone(), escuta.to_string()) {
                Ok(desta_volta) => desta_volta,
                // Um `SocketAddr` escrito é sempre um endereço que o bilhete
                // aceita. Se um dia não for, a volta segue com o guardado, e
                // diz por quê.
                Err(erro) => {
                    tracing::info!(
                        ?erro,
                        %escuta,
                        ponto = %guardado.ponto,
                        "quarto: a escuta que ele deu não coube no bilhete; o LEVE vai ao aviso \
                         guardado (do link ou da lista)"
                    );
                    guardado.clone()
                }
            }
        }
        EscutaDoQuarto::NaoConfirmada {
            escuta,
            servidor: None,
        } => {
            tracing::info!(
                %escuta,
                ponto = %guardado.ponto,
                "quarto: deu a escuta e não deu o servidor, que é quem a confirma; ela não vira o \
                 aviso do LEVE, que vai ao aviso guardado (do link ou da lista). Ou o anfitrião \
                 está fora do ar e outra pessoa ocupou a marca da escuta, ou o registro do \
                 servidor dele não chegou ao ponto, ou a resposta sobre o servidor não voltou a \
                 tempo"
            );
            guardado.clone()
        }
        EscutaDoQuarto::NaoConfirmada {
            escuta,
            servidor: Some(servidor),
        } => {
            tracing::info!(
                %escuta,
                %servidor,
                ponto = %guardado.ponto,
                "quarto: a escuta que ele deu mora noutro IP que o servidor da mesma resposta; \
                 ela não vira o aviso do LEVE, que vai ao aviso guardado (do link ou da lista). \
                 A escuta e o servidor de um anfitrião saem pelo mesmo IP público: uma das duas \
                 marcas foi ocupada por outra pessoa (a da escuta ou a do servidor), ou o \
                 anfitrião sai por dois IPs públicos"
            );
            guardado.clone()
        }
    }
}

/// Onde um anfitrião mora **agora**: o socket do servidor e a escuta de avisos.
///
/// # Por que ela existe
///
/// Todo endereço de um anfitrião atrás de NAT é perecível: o mapeamento nasce
/// quando um pacote sai, e o roteador dá outro na abertura seguinte. O que não
/// envelhece é a identidade, e é dela que saem as [`Marcas`]. O anfitrião
/// registra o endereço de hoje sob elas, na subida e a cada quinze segundos,
/// e esta função pergunta.
///
/// # Como pergunta
///
/// O nome do ponto vira os endereços dele, IPv4 primeiro. A consulta pergunta
/// ao primeiro, e passa ao seguinte só quando **nada saiu** por ele: o socket
/// da família dele não abriu, ou o sistema recusou as três perguntas da
/// primeira volta. Uma linha `info` diz que ela passou. Um endereço que recebeu
/// as perguntas e calou não leva a consulta ao seguinte: o ponto está no ar ou
/// fora do ar pelos dois (`consultar`).
///
/// Três perguntas por volta, num socket só: `QUEM` pelo servidor, `QUEM` pela
/// escuta, e um `ONDE` cuja única função é o ponto provar que está no ar. É
/// ele que separa «o ponto não respondeu» de «o ponto respondeu e ninguém mora
/// lá». Enquanto nada responder, saem até três voltas, uma a cada meio
/// segundo.
///
/// # Quanto ela custa
///
/// Cada espera é paga num caso só, e nenhuma passa do `prazo`:
///
/// - **as duas marcas respondem**: volta na hora em que a segunda chega, uma
///   ida e volta até o ponto;
/// - **o ponto responde e uma marca cala** (um anfitrião 0.15.0, que não
///   registra a escuta com esta marca; um anfitrião fora do ar, que não
///   registra nada e volta como [`OndeMora::NinguemMora`]): volta na primeira
///   resposta mais o dobro da ida e volta medida pelo `ONDE`, nunca menos de
///   100 ms, e nunca depois da hora em que a volta seguinte sairia (meio
///   segundo depois do envio). Se o `ONDE` daquela volta se perdeu, não há ida
///   e volta medida, e a espera vai até essa hora. A pergunta não se repete a
///   um ponto que está no ar;
/// - **o ponto só responde numa volta repetida**: cada volta perdida custa meio
///   segundo, e a que respondeu custa o de cima;
/// - **nada sai pela primeira volta de um endereço** (o socket não abriu; sem
///   rota, o sistema recusa as três): a consulta passa na hora ao endereço
///   seguinte, que paga o próprio caso desta lista. Sem endereço seguinte,
///   volta na hora, como [`OndeMora::PontoMudo`];
/// - **o sistema recusa uma volta depois da primeira**: a recusada conta como
///   volta. As perguntas da anterior saíram, e a consulta espera a resposta a
///   elas até a hora da volta seguinte, como se a recusada tivesse saído; a
///   ida e volta do `ONDE` se mede da última volta que saiu. Custa o mesmo que
///   a volta perdida;
/// - **a leitura do socket falha**: a consulta acaba ali, com o que chegou
///   até então: como [`OndeMora::PontoMudo`] se nada respondeu antes; se o
///   ponto já respondeu, como [`OndeMora::NinguemMora`], ou
///   [`OndeMora::Achado`] com o que chegou. Não passa ao endereço seguinte: as
///   perguntas saíram;
/// - **o ponto cala nas três voltas, ou o nome não resolve a tempo**: o prazo
///   inteiro. Resolver o nome conta dentro dele.
///
/// # O que se faz com a resposta
///
/// Ela entra na lista de candidatos, na frente dos guardados. Não substitui a
/// conferência da identidade de quem atender naquele endereço.
///
/// O resultado vai para o log, com o ponto, o que ele disse e quanto a
/// consulta levou (`levou=`, com a resolução do nome dentro). É o dado que
/// faltava quando a pergunta nunca saía e nada dizia isso, e o tempo é o que a
/// medida de campo do G1 só achava numa captura de pacotes. Cada caminho que
/// acaba sem resposta deixa também a sua causa, em `info` (o único nível que o
/// `seele.log` grava): o prazo que venceu, o envio que o sistema recusou, a
/// leitura que falhou, o endereço que a consulta deixou pelo seguinte.
pub async fn onde_mora_hoje(ponto: &str, marcas: &Marcas, prazo: Duration) -> OndeMora {
    consultar_o_ponto(
        ponto,
        || PeloQuem::nova(marcas),
        prazo,
        "quarto: onde o anfitrião mora hoje",
    )
    .await
}

/// Onde a sala pessoal de quem tem estas marcas mora **agora**, pelo ENC/2
/// (ADR 0062): o socket do servidor, a escuta de avisos e as dicas de rede
/// local.
///
/// As marcas são as da sala (`Marcas::da_sala`), e as perguntas são o `QUEM2`
/// de cada uma, com o `ONDE` da versão 1 como sonda. É a sonda que separa os
/// quatro casos que, para quem liga, apontam para lugares diferentes
/// ([`Localizacao`]): o ponto fala o ENC/2 e a sala está lá; fala, e ela não
/// está (`NINGUEM`); responde ao `ONDE` e cala o `QUEM2`, porque é anterior ao
/// ENC/2; ou não responde a nada.
///
/// Custa o que [`onde_mora_hoje`] custa, caso a caso, e passa pelo mesmo laço:
/// três perguntas por volta, até três voltas, o teto depois da primeira
/// resposta, e o endereço seguinte do ponto só quando nada saiu pelo primeiro.
/// O resultado vai para o log, como lá.
pub async fn localizar(ponto: &str, marcas: &Marcas, prazo: Duration) -> Localizacao {
    consultar_o_ponto(
        ponto,
        || PeloQuem2::nova(marcas),
        prazo,
        "quarto: onde a sala mora hoje (ENC/2)",
    )
    .await
}

/// O que [`localizar`] achou, para quem liga pelo código dizer o que houve
/// (spec de 23/09/2026, «Erros e frases»).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Localizacao {
    /// O ponto deu pelo menos um endereço da sala.
    ///
    /// Um dos dois pode faltar, quando a resposta sobre ele não voltou a
    /// tempo. As duas marcas só se registram com a chave da pessoa, então a
    /// escuta não precisa da regra do IP do servidor que a do ENC/1 precisa
    /// ([`OndeMora::escuta_do_anfitriao`]).
    Achada {
        /// Para onde se conecta, visto de fora.
        servidor: Option<SocketAddr>,
        /// Para onde vai o `LEVE`, com a marca do aviso da sala.
        escuta: Option<SocketAddr>,
        /// As dicas de rede local do servidor, já conferidas por
        /// `seele_proto::encontro::serve_de_dica`: é por elas que duas pessoas
        /// na mesma casa se ligam sem *hairpin* no roteador.
        dicas: Vec<SocketAddr>,
    },
    /// O ponto fala o ENC/2 e disse `NINGUEM`: a sala não está registrada. O
    /// SEELE do outro lado está fechado, ou não está recebendo ligações.
    NinguemMora,
    /// O ponto respondeu ao `ONDE` e calou para o `QUEM2`: ele é anterior ao
    /// ENC/2, e a culpa não é de quem recebe. Ninguém acha ninguém pelo código
    /// num ponto assim.
    PontoSemEnc2,
    /// O ponto recusou por excesso de pedidos desta rede (o limite é por
    /// endereço público, e a rede pode dividi-lo com outras pessoas).
    PontoRecusou,
    /// O ponto não respondeu a nada no prazo, ou a pergunta nem saiu. Como em
    /// [`OndeMora::PontoMudo`].
    PontoMudo,
    /// O texto do ponto não é um endereço, ou o nome não resolveu no prazo.
    PontoNaoResolve,
}

/// O que as duas consultas ao quarto têm em comum: resolver o ponto dentro do
/// prazo, perguntar endereço por endereço, e dizer no log o que voltou.
async fn consultar_o_ponto<C: Consulta>(
    ponto: &str,
    nova: impl Fn() -> C,
    prazo: Duration,
    o_que: &'static str,
) -> C::Resposta {
    // **O prazo cobre a consulta inteira, e resolver o nome é parte dela.**
    // Numa rede sem internet (duas máquinas na mesma casa, o caso que menos
    // precisa de ponto de encontro), resolver o nome não falha rápido: espera
    // um servidor de DNS que não vai responder. Isto roda antes de qualquer
    // tentativa de conexão, e uma pergunta acessória que atrasa a principal é
    // pior que não perguntar.
    let comecou = tokio::time::Instant::now();
    let ate = comecou + prazo;
    let resposta = match tokio::time::timeout_at(ate, resolver_ponto(ponto)).await {
        // `resolver_ponto` já deixou no log por que não achou.
        Ok(destinos) if destinos.is_empty() => C::nao_resolve(),
        Ok(destinos) => consultar_com_todos(&destinos, nova, ate).await,
        Err(_) => {
            tracing::info!(
                ponto,
                ?prazo,
                "quarto: o nome do ponto de encontro não resolveu dentro do prazo"
            );
            C::nao_resolve()
        }
    };
    tracing::info!(
        %ponto,
        ?resposta,
        levou = ?comecou.elapsed(),
        "{o_que}"
    );
    resposta
}

/// A consulta do ENC/1 pelos endereços do ponto: [`consultar_com_todos`] com
/// [`PeloQuem`], o mesmo caminho de [`onde_mora_hoje`]. Os testes do laço a
/// chamam pelo nome de antes do ENC/2.
#[cfg(test)]
async fn consultar(
    destinos: &[SocketAddr],
    marcas: &Marcas,
    ate: tokio::time::Instant,
) -> OndeMora {
    consultar_com_todos(destinos, || PeloQuem::nova(marcas), ate).await
}

/// A consulta pelos endereços do ponto, um de cada vez, **dentro do mesmo
/// `ate`**.
///
/// Pergunta ao primeiro, e passa ao seguinte só quando nenhuma pergunta saiu
/// por ele: o socket da família dele não abriu, ou o sistema recusou as três
/// perguntas da primeira volta. É o caso de um nome com A e AAAA numa máquina
/// sem rota para a família do primeiro: até aqui a consulta parava ali, como
/// [`OndeMora::PontoMudo`], com o outro endereço do mesmo ponto intocado. Um
/// ponto que recebeu as perguntas e calou não é tentado de novo por outro
/// endereço: ele está no ar ou fora do ar pelos dois. A escolha é
/// [`escolher`].
///
/// Sem endereço que sirva, o «ponto mudo» de cada consulta, como antes: para
/// fora, [`OndeMora`] não muda.
async fn consultar_com_todos<C: Consulta>(
    destinos: &[SocketAddr],
    nova: impl Fn() -> C,
    ate: tokio::time::Instant,
) -> C::Resposta {
    for (indice, destino) in destinos.iter().enumerate() {
        match escolher(perguntar_a(*destino, nova(), ate).await) {
            Escolha::Devolver(resposta) => return resposta,
            Escolha::TentarOSeguinte => {
                // A causa deste endereço já está no log, numa linha de quem
                // falhou. Esta diz que a consulta não parou nele.
                if let Some(seguinte) = destinos.get(indice + 1) {
                    tracing::info!(
                        %destino,
                        %seguinte,
                        "quarto: nenhuma pergunta saiu por este endereço do ponto; a consulta \
                         tenta o seguinte"
                    );
                }
            }
        }
    }
    C::mudo()
}

/// O que a consulta por um endereço do ponto deu, **por dentro**.
///
/// Para fora, os dois primeiros e um ponto que calou até o prazo são o mesmo
/// «ponto mudo». Por dentro eles se separam, porque só os dois primeiros dizem
/// que vale tentar o endereço seguinte: neles nenhuma pergunta chegou a sair, e
/// o ponto não teve chance de responder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Desfecho<R> {
    /// O socket da família deste endereço não abriu.
    SemSocket,
    /// Nenhuma pergunta da primeira volta saiu: o sistema recusou as três.
    NadaSaiu,
    /// As perguntas saíram, e isto é o que o ponto disse, ou calou, até a
    /// consulta acabar.
    Perguntou(R),
}

/// O [`Desfecho`] da consulta do ENC/1, pelo nome que os testes do laço usam.
#[cfg(test)]
type PorUmEndereco = Desfecho<OndeMora>;

/// O que [`consultar_com_todos`] faz com o que um endereço deu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escolha<R> {
    /// A consulta acaba com esta resposta.
    Devolver(R),
    /// Nenhuma pergunta saiu por este endereço: vale o seguinte.
    TentarOSeguinte,
}

/// A escolha de [`consultar_com_todos`] depois de um endereço do ponto.
fn escolher<R>(desfecho: Desfecho<R>) -> Escolha<R> {
    match desfecho {
        Desfecho::SemSocket | Desfecho::NadaSaiu => Escolha::TentarOSeguinte,
        Desfecho::Perguntou(resposta) => Escolha::Devolver(resposta),
    }
}

/// Abre um socket da família de `destino` e pergunta por ele
/// ([`consultar_com`]).
async fn perguntar_a<C: Consulta>(
    destino: SocketAddr,
    consulta: C,
    ate: tokio::time::Instant,
) -> Desfecho<C::Resposta> {
    let local = if destino.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0))
    };
    let socket = match tokio::net::UdpSocket::bind(local).await {
        Ok(socket) => socket,
        Err(erro) => {
            tracing::info!(%erro, %destino, "quarto: não abriu socket para perguntar");
            return Desfecho::SemSocket;
        }
    };
    consultar_com(&socket, destino, consulta, ate).await
}

/// O que uma consulta ao quarto pergunta, como lê as respostas, e o que
/// devolve. São duas: o `QUEM` do ENC/1 ([`PeloQuem`]) e o `QUEM2` do ENC/2
/// ([`PeloQuem2`]). O laço que as pergunta é um só, [`consultar_com`], e é nele
/// que mora tudo o que custou a acertar: as voltas, o teto depois da primeira
/// resposta, a volta recusada que conta.
trait Consulta: std::fmt::Debug {
    /// O que a consulta devolve.
    type Resposta: std::fmt::Debug;
    /// As três perguntas de cada volta: as duas marcas e a sonda (`ONDE`).
    fn pedidos(&self, sonda: &Marca) -> [Vec<u8>; 3];
    /// Lê um datagrama do ponto. `None` quando ele não é deste protocolo, e
    /// então não conta como resposta.
    fn ler(&mut self, datagrama: &[u8], sonda: &Marca) -> Option<Lido>;
    /// Se as duas marcas já responderam.
    fn completa(&self) -> bool;
    /// O que a consulta devolve com o que chegou, sabendo se o ponto
    /// respondeu a alguma coisa.
    fn concluir(self, no_ar: bool) -> Self::Resposta;
    /// Nenhum endereço do ponto serviu.
    fn mudo() -> Self::Resposta;
    /// O ponto não resolveu.
    fn nao_resolve() -> Self::Resposta;
}

/// O que um datagrama lido do ponto era.
enum Lido {
    /// A resposta à sonda: o ponto está no ar, e dela sai a ida e volta.
    Sonda,
    /// Outra resposta do ponto: uma das marcas, ou uma que não é de ninguém.
    /// O ponto está no ar.
    Outra,
}

/// A consulta do ENC/1: `QUEM` pelas duas marcas, e o `ONDE` como sonda.
#[derive(Debug)]
struct PeloQuem<'m> {
    marcas: &'m Marcas,
    servidor: Option<SocketAddr>,
    escuta: Option<SocketAddr>,
}

impl<'m> PeloQuem<'m> {
    fn nova(marcas: &'m Marcas) -> Self {
        Self {
            marcas,
            servidor: None,
            escuta: None,
        }
    }
}

impl Consulta for PeloQuem<'_> {
    type Resposta = OndeMora;

    fn pedidos(&self, sonda: &Marca) -> [Vec<u8>; 3] {
        [
            encontro::quem(self.marcas.servidor()),
            encontro::quem(self.marcas.escuta()),
            encontro::onde(sonda),
        ]
    }

    fn ler(&mut self, datagrama: &[u8], sonda: &Marca) -> Option<Lido> {
        let (marca, endereco) = encontro::ler_aqui(datagrama)?;
        if marca == *sonda {
            return Some(Lido::Sonda);
        }
        if marca == *self.marcas.servidor() {
            self.servidor = Some(endereco);
        } else if marca == *self.marcas.escuta() {
            self.escuta = Some(endereco);
        }
        Some(Lido::Outra)
    }

    fn completa(&self) -> bool {
        self.servidor.is_some() && self.escuta.is_some()
    }

    fn concluir(self, no_ar: bool) -> OndeMora {
        match (self.servidor, self.escuta) {
            (None, None) if no_ar => OndeMora::NinguemMora,
            (None, None) => OndeMora::PontoMudo,
            (servidor, escuta) => OndeMora::Achado { servidor, escuta },
        }
    }

    fn mudo() -> OndeMora {
        OndeMora::PontoMudo
    }

    fn nao_resolve() -> OndeMora {
        OndeMora::PontoNaoResolve
    }
}

/// A consulta do ENC/2: `QUEM2` pelas duas marcas da sala, e o `ONDE` da
/// versão 1 como sonda. É a sonda que separa «o ponto não fala o ENC/2» (ela
/// responde, e os `QUEM2` calam) de «ninguém mora» (os `QUEM2` dizem
/// `NINGUEM`).
///
/// Um `AQUI` da versão 1 com a marca da sala não é endereço da sala: só o
/// `AQUI2` é. A marca da sala não se registra pela versão 1 num ponto com o
/// ENC/2, e um ponto sem ele não a guarda assinada.
#[derive(Debug)]
struct PeloQuem2<'m> {
    marcas: &'m Marcas,
    servidor: Option<encontro::Resposta2>,
    escuta: Option<encontro::Resposta2>,
}

impl<'m> PeloQuem2<'m> {
    fn nova(marcas: &'m Marcas) -> Self {
        Self {
            marcas,
            servidor: None,
            escuta: None,
        }
    }
}

impl Consulta for PeloQuem2<'_> {
    type Resposta = Localizacao;

    fn pedidos(&self, sonda: &Marca) -> [Vec<u8>; 3] {
        [
            encontro::quem2(self.marcas.servidor()),
            encontro::quem2(self.marcas.escuta()),
            encontro::onde(sonda),
        ]
    }

    fn ler(&mut self, datagrama: &[u8], sonda: &Marca) -> Option<Lido> {
        if let Some((marca, _)) = encontro::ler_aqui(datagrama) {
            return Some(if marca == *sonda {
                Lido::Sonda
            } else {
                Lido::Outra
            });
        }
        let resposta = encontro::ler_resposta2(datagrama)?;
        if resposta.marca() == self.marcas.servidor() {
            self.servidor = Some(resposta);
        } else if resposta.marca() == self.marcas.escuta() {
            self.escuta = Some(resposta);
        }
        Some(Lido::Outra)
    }

    fn completa(&self) -> bool {
        self.servidor.is_some() && self.escuta.is_some()
    }

    fn concluir(self, no_ar: bool) -> Localizacao {
        let endereco = |resposta: &Option<encontro::Resposta2>| match resposta {
            Some(encontro::Resposta2::Aqui2 { endereco, .. }) => Some(*endereco),
            _ => None,
        };
        let (servidor, escuta) = (endereco(&self.servidor), endereco(&self.escuta));
        if servidor.is_some() || escuta.is_some() {
            let dicas = match self.servidor {
                Some(encontro::Resposta2::Aqui2 { dicas, .. }) => dicas,
                _ => Vec::new(),
            };
            return Localizacao::Achada {
                servidor,
                escuta,
                dicas,
            };
        }
        let respostas = [self.servidor, self.escuta];
        if respostas
            .iter()
            .flatten()
            .any(|resposta| matches!(resposta, encontro::Resposta2::Recusa { .. }))
        {
            Localizacao::PontoRecusou
        } else if respostas.iter().flatten().next().is_some() {
            Localizacao::NinguemMora
        } else if no_ar {
            Localizacao::PontoSemEnc2
        } else {
            Localizacao::PontoMudo
        }
    }

    fn mudo() -> Localizacao {
        Localizacao::PontoMudo
    }

    fn nao_resolve() -> Localizacao {
        Localizacao::PontoNaoResolve
    }
}

/// A consulta do ENC/1 por um endereço, num socket já aberto: [`consultar_com`]
/// com [`PeloQuem`], o mesmo caminho de [`onde_mora_hoje`]. Os testes do laço a
/// chamam pelo nome de antes do ENC/2.
#[cfg(test)]
async fn consultar_por(
    canal: &impl Canal,
    destino: SocketAddr,
    marcas: &Marcas,
    ate: tokio::time::Instant,
) -> PorUmEndereco {
    consultar_com(canal, destino, PeloQuem::nova(marcas), ate).await
}

/// As três perguntas a um endereço do ponto, num socket já aberto, repetidas
/// até alguém responder ou o prazo vencer.
///
/// Um socket só para as três: as respostas se separam pela marca, e é o `ONDE`
/// saindo pelo mesmo caminho que as perguntas das marcas que prova que aquele
/// caminho funciona.
///
/// Sai do laço, e com isso decide quanto a conexão espera (a lista caso a caso
/// está em [`onde_mora_hoje`]):
///
/// - quando as duas marcas responderam;
/// - quando o ponto respondeu e venceu o [`teto_depois_da_resposta`], medido
///   pela ida e volta do `ONDE`;
/// - na hora da volta seguinte, com o ponto no ar e o teto ainda por vencer
///   (o `ONDE` se perdeu, ou a ida e volta é longa);
/// - na hora, quando nenhuma pergunta da primeira volta saiu ou a leitura
///   falhou;
/// - no prazo, quando nada respondeu.
///
/// Recebe o socket, e não o abre, só para o teste poder dar o socket. É assim
/// que ele manda a esta consulta um `AQUI` de outro IP, e prova a chamada de
/// [`aceita_origem`] aqui dentro: o predicado sozinho tem teste, e a chamada
/// trocada por um no-op passava em todos (o m1 da revisão final do Plano 1).
/// Ver `um_aqui_de_outro_ip_nao_entra_na_resposta_da_consulta`. E é por isso
/// que ele recebe um [`Canal`], e não o socket: um teste precisa de um envio
/// que o sistema recusa numa volta e não na outra.
async fn consultar_com<C: Consulta>(
    canal: &impl Canal,
    destino: SocketAddr,
    mut consulta: C,
    ate: tokio::time::Instant,
) -> Desfecho<C::Resposta> {
    let Some(sonda) = Marca::nova(MARCA_DA_CONSULTA) else {
        // Inalcançável: a constante é uma marca válida, e
        // `a_marca_da_consulta_e_valida_e_nao_colide_com_marca_de_morador` o prova.
        return Desfecho::NadaSaiu;
    };
    let pedidos = consulta.pedidos(&sonda);

    let mut no_ar = false;
    let mut estourou = false;
    let mut enviados = 0_u32;
    let mut proxima_volta = tokio::time::Instant::now();
    // Quando a volta corrente saiu, quando chegou a primeira resposta, e quanto
    // o `ONDE` levou para voltar: é deles que sai o teto da espera pela marca
    // que falta ([`teto_depois_da_resposta`]).
    let mut saiu_em = proxima_volta;
    let mut primeira_resposta = None;
    let mut ida_e_volta = None;
    // Do tamanho do maior datagrama das duas versões, e um byte a mais: um
    // datagrama maior chega com o byte a mais e não se lê como resposta.
    let mut balde = [0_u8; encontro::TAMANHO2 + 1];

    while !consulta.completa() {
        let agora = tokio::time::Instant::now();
        if agora >= ate {
            estourou = true;
            break;
        }
        let teto = primeira_resposta
            .zip(ida_e_volta)
            .map(|(primeira, rtt)| teto_depois_da_resposta(primeira, rtt));
        // **O ponto respondeu e o teto venceu.** Quem ainda não respondeu não
        // está no quarto: um anfitrião 0.15.0 nunca registra a escuta com esta
        // marca, e um fora do ar não registra nada. Esperar a volta seguinte
        // era meio segundo cobrado de toda conexão a eles.
        if teto.is_some_and(|teto| agora >= teto) {
            break;
        }
        if agora >= proxima_volta && enviados < ENVIOS_AO_QUARTO {
            // O ponto já respondeu, e o teto não venceu antes desta hora: ou o
            // `ONDE` daquela volta se perdeu e não há ida e volta medida, ou ela
            // é tão longa que o teto passa da volta seguinte. Quem não
            // respondeu teve um intervalo inteiro para isso, e repetir a
            // pergunta a um ponto que está no ar só atrasa a conexão.
            if no_ar {
                break;
            }
            // Uma linha por volta, e não por pergunta: um envio que o sistema
            // recusa costuma recusar as três, e a causa é a mesma.
            let mut saidas = 0_u32;
            let mut recusa = None;
            for pedido in &pedidos {
                match canal.mandar(pedido, destino).await {
                    Ok(_) => saidas += 1,
                    Err(erro) => {
                        recusa.get_or_insert(erro);
                    }
                }
            }
            if let Some(erro) = recusa {
                tracing::info!(
                    %erro,
                    %destino,
                    volta = enviados + 1,
                    saidas,
                    "quarto: a pergunta não saiu"
                );
            }
            if depois_da_volta(saidas, enviados) == DepoisDaVolta::NadaSaiu {
                return Desfecho::NadaSaiu;
            }
            // A ida e volta do `ONDE` se mede da última volta que saiu: numa
            // volta recusada, as perguntas no ar são as da anterior.
            if saidas > 0 {
                saiu_em = agora;
            }
            enviados += 1;
            proxima_volta += INTERVALO_DOS_ENVIOS;
        }
        let acordar = if enviados < ENVIOS_AO_QUARTO {
            proxima_volta.min(ate)
        } else {
            ate
        };
        let acordar = teto.map_or(acordar, |teto| teto.min(acordar));
        let (lidos, origem) =
            match tokio::time::timeout_at(acordar, canal.receber(&mut balde)).await {
                Ok(Ok(recebido)) => recebido,
                // Hora da próxima volta, ou do fim: o topo do laço decide.
                Err(_) => continue,
                // O Windows entrega aqui o «porta fechada» de um envio anterior.
                Ok(Err(erro)) if erro.kind() == std::io::ErrorKind::ConnectionReset => continue,
                Ok(Err(erro)) => {
                    tracing::info!(%erro, %destino, "quarto: a leitura falhou");
                    break;
                }
            };
        // Só do ponto a que se perguntou. Uma resposta que chega de outro
        // lugar é ruído da internet, ou alguém tentando escolher para onde esta
        // máquina vai conectar.
        if !aceita_origem(origem, destino) {
            continue;
        }
        let Some(lido) = balde
            .get(..lidos)
            .and_then(|datagrama| consulta.ler(datagrama, &sonda))
        else {
            continue;
        };
        no_ar = true;
        let chegou = tokio::time::Instant::now();
        primeira_resposta.get_or_insert(chegou);
        if let Lido::Sonda = lido {
            ida_e_volta.get_or_insert(chegou.saturating_duration_since(saiu_em));
        }
    }

    if estourou {
        tracing::info!(
            %destino,
            enviados,
            no_ar,
            ?consulta,
            "quarto: o prazo da consulta venceu antes de as duas marcas responderem"
        );
    }
    Desfecho::Perguntou(consulta.concluir(no_ar))
}

/// Por onde [`consultar_por`] fala com o ponto: em produção, o socket UDP que
/// [`perguntar_a`] abre.
///
/// Um traço, e não o `tokio::net::UdpSocket` direto, por um teste só:
/// `uma_volta_recusada_depois_da_primeira_ainda_ouve_a_resposta_tardia`. Ele
/// precisa de um envio que dá certo na volta 1 e é recusado na 2, e nenhum
/// socket de verdade faz isso do mesmo jeito no macOS, no Windows e no Linux
/// (as recusas que cada sistema faz estão medidas em `destino_recusado`, nos
/// testes deste módulo).
trait Canal {
    /// Manda um datagrama a `destino`, como `send_to`.
    async fn mandar(&self, datagrama: &[u8], destino: SocketAddr) -> std::io::Result<usize>;
    /// Espera o próximo datagrama, como `recv_from`.
    async fn receber(&self, balde: &mut [u8]) -> std::io::Result<(usize, SocketAddr)>;
}

impl Canal for tokio::net::UdpSocket {
    async fn mandar(&self, datagrama: &[u8], destino: SocketAddr) -> std::io::Result<usize> {
        self.send_to(datagrama, destino).await
    }

    async fn receber(&self, balde: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        self.recv_from(balde).await
    }
}

/// O que [`consultar_por`] faz depois de mandar uma volta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepoisDaVolta {
    /// Nenhuma pergunta saiu, e nenhuma volta antes desta: não há resposta que
    /// possa vir, e a consulta por este endereço acaba na hora.
    NadaSaiu,
    /// A volta conta, tenha saído ou não, e a consulta espera a resposta até a
    /// volta seguinte.
    Contar,
}

/// A decisão de [`consultar_por`] depois de mandar a volta `enviados + 1`, da
/// qual saíram `saidas` perguntas.
///
/// **Só a primeira volta recusada inteira acaba a consulta.** Sem rota
/// (`ENETUNREACH`, `EHOSTUNREACH`) o sistema recusa as três na hora, e repetir a
/// volta e dormir até o prazo só faria a conexão pagar 1,5 s por uma resposta
/// que não tem como vir. A `perguntar` antiga também saía ali, na hora.
///
/// Numa volta 2 ou 3 não é assim: as perguntas da volta anterior saíram, e a
/// resposta a elas ainda pode chegar. Parar ali perdia essa resposta, e a
/// consulta voltava como `PontoMudo` com o anfitrião no quarto (o T6f da
/// pendência 51, item 17). A volta recusada **conta como volta**: a seguinte sai
/// meio segundo depois, e a recusa não vira laço quente.
fn depois_da_volta(saidas: u32, enviados: u32) -> DepoisDaVolta {
    if saidas == 0 && enviados == 0 {
        DepoisDaVolta::NadaSaiu
    } else {
        DepoisDaVolta::Contar
    }
}

/// Se uma resposta à consulta veio do ponto a que se perguntou.
///
/// É a única barreira entre um `AQUI` de terceiro e a resposta da consulta. O
/// servidor que o quarto responde entra na frente da escada, e a escuta vira o
/// aviso do `LEVE` ([`bilhete_desta_volta`]). Um `AQUI` forjado que passasse
/// daqui escolheria para onde esta máquina conecta, e, com as duas marcas
/// forjadas no mesmo IP, para onde vai o `LEVE`, que sai antes do TLS com o
/// endereço de quem chega. Quem protege a conexão depois disso é a impressão
/// digital conferida no aperto de mão; o `LEVE` não tem outra barreira.
///
/// A chamada dentro de [`consultar_por`] tem prova própria, com um `AQUI` de
/// outro IP mandado ao socket da consulta
/// (`um_aqui_de_outro_ip_nao_entra_na_resposta_da_consulta`).
///
/// **Compara o IP, não a porta**, pelo motivo de `aviso_e_do_ponto`, no
/// anfitrião: quem consegue forjar um endereço de origem forja a porta junto, e
/// recusar outra porta só quebraria um ponto atrás de um balanceador.
fn aceita_origem(origem: SocketAddr, destino: SocketAddr) -> bool {
    origem.ip() == destino.ip()
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O ponto de encontro é escolhido pela família do anfitrião, não pela ordem
    /// do DNS.
    ///
    /// O caso de campo: Mac numa rede 5G com IPv6, anfitrião Windows com
    /// endereço IPv4. O DNS devolve o AAAA primeiro, e com `.next()` o pedido
    /// saía por IPv6 — o ponto de encontro registrou `Network is unreachable`
    /// ao tentar repassar o aviso ao anfitrião IPv4, três vezes seguidas.
    #[test]
    fn o_ponto_de_encontro_e_o_da_familia_do_anfitriao() {
        let seis = "[2001:db8::1]:8384".parse::<SocketAddr>();
        let quatro = "216.128.168.216:8384".parse::<SocketAddr>();
        let anfitriao_v4 = "187.255.97.152:9621".parse::<SocketAddr>();
        let anfitriao_v6 = "[2804:388::1]:9621".parse::<SocketAddr>();
        let (Ok(seis), Ok(quatro), Ok(anfitriao_v4), Ok(anfitriao_v6)) =
            (seis, quatro, anfitriao_v4, anfitriao_v6)
        else {
            panic!("os endereços deste teste têm de ser válidos");
        };

        // O DNS devolve o IPv6 primeiro, como no 5G.
        let achados = [seis, quatro];
        assert_eq!(
            escolher_ponto(&achados, anfitriao_v4),
            Some(quatro),
            "um anfitrião IPv4 só é avisado por um ponto de encontro IPv4"
        );
        assert_eq!(escolher_ponto(&achados, anfitriao_v6), Some(seis));

        // Sem a família certa, o primeiro — falhar ali é melhor que não tentar.
        assert_eq!(escolher_ponto(&[seis], anfitriao_v4), Some(seis));
        assert_eq!(escolher_ponto(&[], anfitriao_v4), None);
    }

    #[tokio::test]
    async fn o_ponto_escrito_sem_porta_e_procurado_na_porta_do_ponto_de_encontro() {
        // O defeito de campo: o link carrega `enc=encontro.seele.app.br/…`, sem
        // porta, e este texto ia cru para `lookup_host`, que recusa nome sem
        // porta. A pergunta ao quarto nunca saiu, e nada dizia isso.
        assert_eq!(
            resolver_ponto("127.0.0.1").await,
            [SocketAddr::from(([127, 0, 0, 1], encontro::PORTA_PADRAO))],
            "um ponto sem porta não foi procurado na porta do ponto de encontro"
        );
        assert_eq!(
            resolver_ponto("[::1]").await,
            [SocketAddr::from((
                std::net::Ipv6Addr::LOCALHOST,
                encontro::PORTA_PADRAO
            ))],
            "um IPv6 entre colchetes sem porta não foi procurado na porta do ponto de encontro"
        );
        assert_eq!(
            resolver_ponto("127.0.0.1:9000").await,
            [SocketAddr::from(([127, 0, 0, 1], 9000))],
            "a porta escrita deixou de mandar"
        );
        assert!(
            resolver_ponto("tem espaço").await.is_empty(),
            "um texto que não é endereço virou pergunta"
        );
    }

    #[test]
    fn os_achados_vao_com_o_ipv4_na_frente_e_a_lista_vazia_diz_no_rastro() {
        // T5a e T5c da pendência 51, item 17. O DNS devolve o AAAA primeiro
        // numa máquina com IPv6, e a consulta pergunta ao primeiro da lista:
        // sem rota IPv6, o envio dá certo, o pacote some, e a consulta paga o
        // prazo inteiro calada. E um nome que resolve para nada não é erro do
        // DNS: sem a linha, a pergunta não saía e o `seele.log` não dizia.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let seis = SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, encontro::PORTA_PADRAO));
        let quatro = SocketAddr::from(([127, 0, 0, 1], encontro::PORTA_PADRAO));

        assert_eq!(
            ordenar_os_achados("encontro.exemplo", vec![seis, quatro]),
            [quatro, seis],
            "o AAAA que o DNS deu primeiro ficou na frente: numa máquina sem rota IPv6, a \
             pergunta ao quarto some em silêncio e a consulta paga o prazo inteiro"
        );
        assert!(
            captura.linhas().is_empty(),
            "uma lista com endereços deixou linha no rastro: {:?}",
            captura.linhas()
        );

        assert!(
            ordenar_os_achados("encontro.exemplo", Vec::new()).is_empty(),
            "a lista vazia ganhou um endereço"
        );
        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("resolveu para nenhum endereço")
                && linha.contains("encontro.exemplo")),
            "o nome que resolveu para nenhum endereço não disse no rastro (`info`, o único nível \
             que o seele.log grava) qual era: a pergunta ao quarto não sai e ninguém fica sabendo \
             por quê. Rastro: {linhas:?}"
        );
    }

    #[tokio::test]
    async fn um_ponto_que_nao_se_procura_diz_no_rastro_por_que_nao() {
        // «O produto sabe e não conta»: o `.ok()?` calado que havia aqui deixou a
        // pergunta ao quarto sem sair, e o `seele.log` sem uma linha sobre isso.
        // `#[tokio::test]` de thread única, de propósito: `set_default` fixa o
        // `Subscriber` só na thread corrente, e a resolução acontece nela.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());

        assert!(
            resolver_ponto("tem espaço").await.is_empty(),
            "um texto que não é endereço virou pergunta: este teste não mede nada"
        );
        assert!(
            resolver_ponto("nao-existe-mesmo.invalid").await.is_empty(),
            "um nome que não existe resolveu: este teste não mede nada"
        );

        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("não é um endereço")
                && linha.contains("tem espaço")
                && linha.contains("EnderecoInvalido")),
            "um ponto que não é endereço deixou de dizer no rastro (`info`, o único nível que o \
             seele.log grava) qual era o texto e qual foi a causa: a pergunta ao quarto não sai \
             e ninguém fica sabendo por quê. Rastro: {linhas:?}"
        );
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("não resolveu")
                && linha.contains("nao-existe-mesmo.invalid")),
            "um ponto que não resolve deixou de dizer no rastro (`info`) qual era o nome: a \
             pergunta ao quarto não sai e ninguém fica sabendo por quê. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn a_consulta_so_aceita_resposta_do_ip_a_que_perguntou() {
        // A única barreira entre um `AQUI` de terceiro e a lista de candidatos:
        // o que o quarto responde entra na frente da escada, e um `AQUI` forjado
        // que passasse daqui escolheria para onde esta máquina conecta.
        let ponto = SocketAddr::from(([216, 128, 168, 216], encontro::PORTA_PADRAO));
        let terceiro = SocketAddr::from(([203, 0, 113, 9], encontro::PORTA_PADRAO));
        assert!(
            !aceita_origem(terceiro, ponto),
            "um AQUI que veio de outro IP que não o do ponto foi aceito: qualquer um na internet \
             escolhe para onde esta máquina conecta"
        );
        assert!(
            aceita_origem(ponto, ponto),
            "a resposta do próprio ponto foi recusada: a consulta nunca acharia ninguém"
        );
        // Compara o IP e não a porta, como o anfitrião faz em `aviso_e_do_ponto`:
        // quem forja a origem forja a porta junto, e recusar outra porta só
        // quebraria um ponto atrás de um balanceador.
        let outra_porta = SocketAddr::from(([216, 128, 168, 216], 50_000));
        assert!(
            aceita_origem(outra_porta, ponto),
            "a resposta do IP do ponto por outra porta foi recusada: um ponto atrás de um \
             balanceador deixaria de ser ouvido"
        );
    }

    #[tokio::test]
    async fn um_aqui_de_outro_ip_nao_entra_na_resposta_da_consulta() {
        // O m1 da revisão final do Plano 1. O predicado tem teste (logo acima),
        // e a **chamada** dele dentro da consulta não tinha nenhum: trocada por
        // um no-op, os testes de `encontro` e do quarto passavam todos, e o
        // único sinal era um aviso de função sem uso.
        //
        // Aqui um terceiro, noutro IP, manda à consulta um `AQUI` com a marca
        // do servidor antes de o ponto responder. O ponto está em `::1`, e o
        // terceiro em `127.0.0.1`: é o outro IP desta máquina (o macOS recusa o
        // `127.0.0.2`). A consulta corre num socket de pilha dupla, o mesmo que
        // `abrir_socket_local` abre para a batida, e é por ele que o pacote do
        // IPv4 chega.
        let Some(dupla) = abrir_socket_local() else {
            panic!("não abriu o socket local da consulta");
        };
        let consulta = tokio::net::UdpSocket::from_std(dupla).unwrap();
        assert!(
            consulta.local_addr().unwrap().is_ipv6(),
            "o socket local caiu para IPv4: sem pilha dupla, o `AQUI` do terceiro não chega à \
             consulta e este teste não mede nada"
        );
        let porta_da_consulta = consulta.local_addr().unwrap().port();
        let ponto = tokio::net::UdpSocket::bind("[::1]:0").await.unwrap();
        let onde_ponto = ponto.local_addr().unwrap();
        let terceiro = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let para_a_consulta = SocketAddr::from(([127, 0, 0, 1], porta_da_consulta));
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let forjado = SocketAddr::from(([203, 0, 113, 66], 4_444));

        // O ponto: na primeira pergunta, o terceiro manda o `AQUI` forjado, e
        // só depois o ponto responde ao `ONDE`. O `QUEM` ele cala: ninguém mora
        // no quarto.
        let atender = async {
            let mut balde = [0_u8; encontro::TAMANHO];
            let mut forjou = false;
            loop {
                let (lidos, de) = ponto.recv_from(&mut balde).await.unwrap();
                if !forjou {
                    forjou = true;
                    terceiro
                        .send_to(&encontro::aqui(marcas.servidor(), forjado), para_a_consulta)
                        .await
                        .unwrap();
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                if let Some(encontro::Pedido::Onde { marca }) =
                    balde.get(..lidos).and_then(encontro::analisar)
                {
                    ponto
                        .send_to(&encontro::aqui(&marca, de), de)
                        .await
                        .unwrap();
                }
            }
        };
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = tokio::select! {
            achado = consultar_por(&consulta, onde_ponto, &marcas, ate) => achado,
            () = atender => unreachable!("o ponto deste teste não para"),
        };

        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::NinguemMora),
            "a consulta aceitou um `AQUI` que veio de outro IP que o do ponto ({achado:?}): \
             qualquer um que acerte a porta efêmera escolhe para onde esta máquina conecta e, \
             com a marca da escuta, para onde vai o `LEVE`, antes do TLS"
        );

        // E o caminho do terceiro existe: sem isto, um `NinguemMora` também
        // sairia de um pacote que nunca chegou, e o teste passaria sem medir.
        terceiro
            .send_to(&encontro::aqui(marcas.servidor(), forjado), para_a_consulta)
            .await
            .unwrap();
        let mut balde = [0_u8; encontro::TAMANHO];
        let chegou =
            tokio::time::timeout(Duration::from_secs(1), consulta.recv_from(&mut balde)).await;
        assert!(
            matches!(chegou, Ok(Ok((_, de))) if de.ip() != onde_ponto.ip()),
            "o `AQUI` do terceiro não chega ao socket da consulta ({chegou:?}): este teste não \
             mede a barreira, e o `NinguemMora` de cima não prova nada"
        );
    }

    /// O servidor de um anfitrião, a escuta dele, e quem ocupou a marca da
    /// escuta de outro lugar. Endereços de documentação (RFC 5737).
    const SERVIDOR: ([u8; 4], u16) = ([203, 0, 113, 7], 9_621);
    const ESCUTA: ([u8; 4], u16) = ([203, 0, 113, 7], 51_000);
    const OCUPANTE: ([u8; 4], u16) = ([198, 51, 100, 9], 51_000);

    #[test]
    fn a_escuta_do_quarto_so_vale_no_ip_do_servidor_da_mesma_resposta() {
        // O I1 da revisão final do Plano 1. A escuta que o quarto dá vira o
        // aviso do `LEVE`, que sai antes do TLS e leva o endereço de quem chega.
        // A marca dela (fp16 + `e`) está em todo link, e no quarto fica quem
        // escreveu primeiro: um anfitrião 0.15.0 nunca a registra, e qualquer um
        // com o link a toma. A escuta e o servidor de um anfitrião moram na
        // mesma máquina e saem pelo mesmo IP público, e o servidor é conferido
        // no TLS logo depois. Com a escuta noutro IP, uma das duas marcas é de
        // outra pessoa (a da escuta ou a do servidor), ou o anfitrião sai por
        // dois IPs: a regra não usa a escuta em nenhum dos três casos. O
        // `OCUPANTE` daqui é o caso barato, quem tomou só a da escuta.
        let servidor = SocketAddr::from(SERVIDOR);
        let escuta = SocketAddr::from(ESCUTA);
        let ocupante = SocketAddr::from(OCUPANTE);

        assert_eq!(
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::DoAnfitriao(escuta),
            "a escuta no IP do servidor da mesma resposta foi recusada: o anfitrião que mudou de \
             porta deixa de ser avisado pela escuta de hoje"
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(ocupante),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::NaoConfirmada {
                escuta: ocupante,
                servidor: Some(servidor),
            },
            "uma escuta noutro IP que o do servidor da mesma resposta virou aviso: quem ocupou a \
             marca da escuta recebe, antes do TLS, o endereço de quem chega"
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: None,
                escuta: Some(ocupante),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::NaoConfirmada {
                escuta: ocupante,
                servidor: None,
            },
            "uma escuta sem servidor na mesma resposta virou aviso: com o anfitrião fora do ar, \
             quem ocupou a marca da escuta recebe o endereço de quem chega"
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: None,
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::Nenhuma,
            "o quarto não deu escuta (o anfitrião 0.15.0) e a regra inventou uma"
        );
        for sem_resposta in [
            OndeMora::NinguemMora,
            OndeMora::PontoMudo,
            OndeMora::PontoNaoResolve,
            OndeMora::SemMarca,
        ] {
            assert_eq!(
                sem_resposta.escuta_do_anfitriao(),
                EscutaDoQuarto::Nenhuma,
                "{sem_resposta:?} não traz escuta, e a regra inventou uma"
            );
        }

        // O mesmo IP escrito nas duas formas: um ponto de pilha dupla escreve o
        // IPv4 como `::ffff:a.b.c.d`. A forma não muda de quem é o endereço.
        let mapeado = SocketAddr::new(
            std::net::Ipv4Addr::from(SERVIDOR.0).to_ipv6_mapped().into(),
            SERVIDOR.1,
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: Some(mapeado),
                escuta: Some(escuta),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::DoAnfitriao(escuta),
            "o mesmo IP, escrito como IPv4 mapeado num lado e cru no outro, foi tratado como \
             dois: um ponto de pilha dupla deixaria de avisar o anfitrião pela escuta de hoje"
        );
    }

    #[test]
    fn o_bilhete_desta_volta_so_troca_o_aviso_pela_escuta_que_o_servidor_confirma() {
        // `bilhete_desta_volta` é o que a casca usa (pela FFI) para montar o
        // `LEVE` da volta. O guardado é o do link desta sessão ou o da lista.
        // E o que ele deixa de lado vai para o `seele.log` (`info`), com a
        // escuta e o servidor: «o produto sabe e não conta» é o defeito que
        // mais custou neste repositório.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let guardado =
            Bilhete::novo("192.0.2.1:8384", "203.0.113.7:40000").expect("bilhete de teste");
        let servidor = SocketAddr::from(SERVIDOR);
        let escuta = SocketAddr::from(ESCUTA);
        let ocupante = SocketAddr::from(OCUPANTE);

        let confirmada = bilhete_desta_volta(
            &guardado,
            &OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            },
        );
        assert_eq!(
            confirmada,
            Bilhete::novo("192.0.2.1:8384", escuta.to_string()).expect("bilhete de teste"),
            "a escuta confirmada pelo servidor não trocou o aviso: o anfitrião que mudou de porta \
             é avisado no endereço velho"
        );

        for (no_quarto, o_que_quebra) in [
            (
                OndeMora::Achado {
                    servidor: Some(servidor),
                    escuta: Some(ocupante),
                },
                "uma escuta noutro IP que o do servidor",
            ),
            (
                OndeMora::Achado {
                    servidor: None,
                    escuta: Some(ocupante),
                },
                "uma escuta sem servidor na mesma resposta",
            ),
        ] {
            assert_eq!(
                bilhete_desta_volta(&guardado, &no_quarto),
                guardado,
                "{o_que_quebra} trocou o aviso do bilhete: o `LEVE` vai a quem ocupou a marca \
                 da escuta, com o endereço de quem chega"
            );
        }
        assert_eq!(
            bilhete_desta_volta(&guardado, &OndeMora::PontoMudo),
            guardado,
            "sem resposta do quarto, o bilhete guardado mudou"
        );

        let linhas = captura.linhas();
        // Cada linha diz também as causas possíveis, e não só a escuta: com o
        // servidor noutro IP, quem ocupou uma marca pode ter ocupado a do
        // servidor, e aí a escuta deixada de lado é a do anfitrião de verdade
        // (F-Seg-m1 da revisão final do Plano 1). Sem servidor, a resposta
        // dele pode só não ter voltado a tempo.
        for (caso, dito, causas) in [
            (
                "noutro IP que o servidor",
                servidor.to_string(),
                &[
                    "uma das duas marcas foi ocupada por outra pessoa (a da escuta ou a do \
                     servidor)",
                    "ou o anfitrião sai por dois IPs públicos",
                ][..],
            ),
            (
                "sem servidor",
                "não deu o servidor".to_owned(),
                &["ou a resposta sobre o servidor não voltou a tempo"][..],
            ),
        ] {
            let Some(linha) = linhas.iter().find(|linha| {
                linha.starts_with("INFO")
                    && linha.contains("não vira o aviso")
                    && linha.contains(&ocupante.to_string())
                    && linha.contains(&dito)
            }) else {
                panic!(
                    "a escuta deixada de lado {caso} não foi dita no rastro (`info`, o único \
                     nível que o seele.log grava) com a escuta e «{dito}»: quem investiga um \
                     anfitrião que não foi avisado não sabe que o quarto deu outra escuta, nem \
                     por que ela ficou de lado. Rastro: {linhas:?}"
                );
            };
            for causa in causas {
                assert!(
                    linha.contains(causa),
                    "a linha da escuta deixada de lado {caso} não diz «{causa}»: quem lê o \
                     seele.log culpa quem ocupou a escuta, e pode ser o anfitrião de verdade com \
                     a marca do servidor tomada, ou a resposta que se perdeu. Linha: {linha}"
                );
            }
        }
        assert!(
            !linhas
                .iter()
                .any(|linha| linha.contains("não vira o aviso")
                    && linha.contains(&escuta.to_string())),
            "a escuta confirmada foi dita como deixada de lado: o log acusa o anfitrião de \
             verdade. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn a_marca_da_consulta_e_valida_e_nao_colide_com_marca_de_morador() {
        // O `ONDE` da consulta só prova que o ponto está no ar se a resposta
        // dele não puder ser confundida com a de um morador. Uma marca de
        // servidor tem 16 ou 17 caracteres, e os 16 primeiros são
        // hexadecimais. Uma de código começa com `p` e tem 16 ou 17.
        assert!(
            Marca::nova(MARCA_DA_CONSULTA).is_some(),
            "a marca do ONDE não é uma marca: a consulta nunca saberia que o ponto está no ar"
        );
        assert!(
            MARCA_DA_CONSULTA.len() < 16,
            "a marca do ONDE tem o tamanho de uma marca de morador"
        );
        assert!(
            MARCA_DA_CONSULTA.chars().any(|c| !c.is_ascii_hexdigit()),
            "a marca do ONDE é hexadecimal pura e pode colidir com o começo de uma impressão digital"
        );
    }

    #[tokio::test]
    async fn a_consulta_que_estoura_o_prazo_diz_no_rastro_o_que_perguntou_e_o_que_ouviu() {
        // «O produto sabe e não conta»: um ponto que não responde acabava em
        // `PontoMudo` sem uma linha sobre quanto se esperou, e o `seele.log`
        // (que só grava `info`) ficava mudo junto com ele. Este teste fixa a
        // `Subscriber` só na thread corrente, e a consulta corre nela.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde_fica = mudo.local_addr().unwrap().to_string();
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };

        let achado = onde_mora_hoje(&onde_fica, &marcas, Duration::from_millis(150)).await;

        assert_eq!(
            achado,
            OndeMora::PontoMudo,
            "um ponto que não respondeu virou outra coisa: este teste não mede nada"
        );
        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("prazo")
                && linha.contains(&onde_fica)),
            "o prazo da consulta venceu sem dizer no rastro (`info`, o único nível que o \
             seele.log grava) a que ponto se perguntou: quem lê o log não distingue um ponto \
             fora do ar de uma pergunta que nunca saiu. Rastro: {linhas:?}"
        );
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("onde o anfitrião mora hoje")
                && linha.contains("PontoMudo")
                && linha.contains(&onde_fica)),
            "o resultado da consulta deixou de ir para o rastro (`info`): «o ponto calou» e «o \
             anfitrião não está» voltam a não deixar dado nenhum. Rastro: {linhas:?}"
        );
    }

    #[tokio::test]
    async fn um_nome_que_nao_resolve_no_prazo_diz_no_rastro_que_foi_o_prazo() {
        // O outro estouro: o do nome. Sem internet, resolver não falha rápido, e
        // o prazo vence antes de a pergunta existir. Sem isto o rastro trazia só
        // `PontoNaoResolve`, o mesmo de um texto que não é endereço, e quem lê
        // não distinguia um DNS mudo de um link malformado.
        //
        // Prazo zero, e um nome que só a resolução de verdade responde: a
        // primeira sondagem do `lookup_host` fica pendente, e o prazo, já
        // vencido, ganha. Um endereço em números resolveria sem esperar.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };

        let achado = onde_mora_hoje("nao-existe-mesmo.invalid:8384", &marcas, Duration::ZERO).await;

        assert_eq!(
            achado,
            OndeMora::PontoNaoResolve,
            "um nome que não resolveu no prazo virou outra coisa: este teste não mede nada"
        );
        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("dentro do prazo")
                && linha.contains("nao-existe-mesmo.invalid")),
            "o prazo que venceu na resolução do nome deixou de dizer no rastro (`info`, o único \
             nível que o seele.log grava) que foi o prazo: um DNS mudo e um link malformado \
             voltam a parecer a mesma coisa. Rastro: {linhas:?}"
        );
        // E quanto a consulta levou, na linha do resultado: a medida de campo
        // do G1 lia esse tempo numa captura de pacotes, porque o `seele.log`
        // não o trazia.
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("onde o anfitrião mora hoje")
                && linha.contains("levou=")),
            "a linha do resultado da consulta ao quarto não diz quanto ela levou (`levou=`): \
             quem mede uma conexão lenta volta a precisar de uma captura de pacotes para saber \
             quanto dela foi o quarto. Rastro: {linhas:?}"
        );
    }

    #[tokio::test]
    async fn uma_pergunta_que_nao_sai_volta_na_hora_e_diz_no_rastro_por_que_nao_saiu() {
        // O envio recusado pelo kernel era engolido num `debug!`: a consulta
        // esperava o prazo inteiro por uma resposta a uma pergunta que nunca
        // saiu, e o log não tinha uma palavra sobre isso.
        //
        // **E esperava mesmo.** Sem rota (`ENETUNREACH`, `EHOSTUNREACH`) nenhuma
        // das três perguntas sai, e não há resposta a aguardar: repetir a volta
        // e dormir até o prazo custava 1,5 s a toda conexão, antes de qualquer
        // tentativa. Uma pergunta acessória que atrasa a principal é pior que
        // não perguntar, então a consulta volta na hora.
        //
        // O destino é **descoberto**, como em `destino_recusado`: `0.0.0.0:0` é
        // recusado pelo macOS e pelo Windows (medidos) e, pela porta zero, pelo
        // Linux (não medido). Se esta máquina o aceitar, quem falha é o teste,
        // dizendo isso, e não a consulta acusada de engolir um erro que nunca
        // houve.
        let destino = SocketAddr::from(([0, 0, 0, 0], 0));
        let sonda = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        assert!(
            sonda.send_to(&[0_u8; encontro::TAMANHO], destino).is_err(),
            "esta máquina aceitou um envio a {destino}: sem uma recusa, este teste não tem um \
             envio falho para observar. Não é a consulta que está errada, é o mecanismo daqui \
             que não vale neste sistema"
        );
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };

        // O prazo de produção, e não um curto: é ele que a consulta pagaria.
        let comecou = std::time::Instant::now();
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar(&[destino], &marcas, ate).await;
        let levou = comecou.elapsed();

        assert_eq!(
            achado,
            OndeMora::PontoMudo,
            "uma pergunta que nunca saiu virou outra coisa: este teste não mede nada"
        );
        assert!(
            levou < Duration::from_millis(500),
            "nenhuma pergunta saiu e a consulta ainda esperou {levou:?}: sem rota, quem paga é a \
             conexão que vem depois, com {PRAZO_DO_QUARTO:?} a mais antes de qualquer tentativa"
        );
        let linhas = captura.linhas();
        assert!(
            linhas
                .iter()
                .any(|linha| linha.starts_with("INFO") && linha.contains("não saiu")),
            "o envio recusado ao quarto deixou de dizer no rastro (`info`, o único nível que o \
             seele.log grava) por que a pergunta não saiu: a consulta espera o prazo inteiro e \
             ninguém fica sabendo o motivo. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn a_consulta_passa_ao_endereco_seguinte_so_quando_nada_saiu_pelo_primeiro() {
        // O M2 do item 19 da pendência 51. Um nome com A e AAAA dá dois
        // endereços, e qual deles tem rota é propriedade desta máquina. Sem
        // rota para a família do primeiro, o sistema recusa as três perguntas
        // (ou o socket nem abre), e a consulta parava ali, como `PontoMudo`,
        // com o outro endereço do mesmo ponto intocado.
        for nada_saiu in [PorUmEndereco::SemSocket, PorUmEndereco::NadaSaiu] {
            assert_eq!(
                escolher(nada_saiu),
                Escolha::TentarOSeguinte,
                "{nada_saiu:?} acabou a consulta: nenhuma pergunta saiu por este endereço, e o \
                 endereço seguinte do mesmo ponto, que pode ter rota, nunca é tentado"
            );
        }
        // E um ponto que recebeu as perguntas não é perguntado de novo por outro
        // endereço: ele está no ar ou fora do ar pelos dois, e cada endereço a
        // mais custaria o prazo inteiro de novo.
        for perguntou in [
            OndeMora::PontoMudo,
            OndeMora::NinguemMora,
            OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: None,
            },
        ] {
            assert_eq!(
                escolher(PorUmEndereco::Perguntou(perguntou)),
                Escolha::Devolver(perguntou),
                "um endereço que recebeu as perguntas não acabou a consulta com o que deu \
                 ({perguntou:?})"
            );
        }
    }

    /// O que o ponto falso anotou: cada pedido que chegou, na ordem.
    type Anotados = Arc<std::sync::Mutex<Vec<encontro::Pedido>>>;

    /// Um ponto de encontro falso, no laço local. A cada pedido que chega,
    /// `responder` diz o que devolver a quem perguntou, cada datagrama com a
    /// demora dele, e o pedido fica anotado.
    async fn ponto_falso(
        responder: impl Fn(&encontro::Pedido, SocketAddr) -> Vec<(Duration, Vec<u8>)> + Send + 'static,
    ) -> (SocketAddr, Anotados) {
        let socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let onde = socket.local_addr().unwrap();
        let anotados: Anotados = Arc::default();
        let anotador = Arc::clone(&anotados);
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let Some(pedido) = balde.get(..lidos).and_then(encontro::analisar) else {
                    continue;
                };
                for (demora, datagrama) in responder(&pedido, de) {
                    let socket = Arc::clone(&socket);
                    tokio::spawn(async move {
                        tokio::time::sleep(demora).await;
                        let _ = socket.send_to(&datagrama, de).await;
                    });
                }
                anotador.lock().unwrap().push(pedido);
            }
        });
        (onde, anotados)
    }

    #[tokio::test]
    async fn um_endereco_sem_rota_nao_esconde_o_seguinte_do_mesmo_ponto() {
        // O par da escolha, com a consulta de verdade: a função pura correta
        // que `consultar` não chamasse deixaria aquele teste verde e a
        // consulta parada no primeiro endereço. O primeiro é um destino que
        // esta máquina recusa (o de
        // `uma_pergunta_que_nao_sai_volta_na_hora_e_diz_no_rastro_por_que_nao_saiu`);
        // o segundo, um ponto falso que responde às duas marcas.
        let recusado = SocketAddr::from(([0, 0, 0, 0], 0));
        let sonda = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        assert!(
            sonda.send_to(&[0_u8; encontro::TAMANHO], recusado).is_err(),
            "esta máquina aceitou um envio a {recusado}: sem uma recusa, o primeiro endereço \
             deste teste não é um sem rota, e o teste não mede nada"
        );
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (servidor, escuta) = (SocketAddr::from(SERVIDOR), SocketAddr::from(ESCUTA));
        let quem = marcas.clone();
        let (ponto, _) = ponto_falso(move |pedido, de| match pedido {
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::ZERO, encontro::aqui(marca, servidor))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.escuta() => {
                vec![(Duration::ZERO, encontro::aqui(marca, escuta))]
            }
            encontro::Pedido::Onde { marca } => vec![(Duration::ZERO, encontro::aqui(marca, de))],
            _ => Vec::new(),
        })
        .await;

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar(&[recusado, ponto], &marcas, ate).await;

        assert_eq!(
            achado,
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            },
            "nenhuma pergunta saiu pelo primeiro endereço do ponto, e a consulta não chegou ao \
             segundo, que responde: um nome com A e AAAA numa máquina sem rota para a família do \
             primeiro perde o quarto inteiro"
        );
        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("tenta o seguinte")
                && linha.contains(&recusado.to_string())
                && linha.contains(&ponto.to_string())),
            "a consulta passou ao endereço seguinte do ponto sem dizer no rastro (`info`, o \
             único nível que o seele.log grava) de qual para qual: quem lê o «não saiu» do \
             primeiro acha que a consulta parou ali. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn uma_volta_recusada_depois_da_primeira_conta_como_volta() {
        // O T6f da pendência 51, item 17. Nenhuma pergunta sair na primeira
        // volta é não haver resposta que possa vir, e a consulta volta na hora
        // (`uma_pergunta_que_nao_sai_volta_na_hora_e_diz_no_rastro_por_que_nao_saiu`).
        // Numa volta 2 ou 3 não é: as perguntas da volta anterior saíram, e a
        // resposta a elas ainda pode chegar.
        assert_eq!(
            depois_da_volta(0, 0),
            DepoisDaVolta::NadaSaiu,
            "nenhuma pergunta saiu na primeira volta e a consulta não desistiu dela: sem rota, \
             a conexão paga o prazo inteiro por uma resposta que não tem como vir"
        );
        for enviados in 1..ENVIOS_AO_QUARTO {
            assert_eq!(
                depois_da_volta(0, enviados),
                DepoisDaVolta::Contar,
                "a volta {} não saiu, depois de {enviados} que saíram, e a consulta parou: a \
                 resposta tardia às perguntas que já saíram se perde, e a consulta volta como \
                 `PontoMudo`",
                enviados + 1
            );
        }
        for saidas in [1, 3] {
            for enviados in 0..ENVIOS_AO_QUARTO {
                assert_eq!(
                    depois_da_volta(saidas, enviados),
                    DepoisDaVolta::Contar,
                    "a volta {} saiu ({saidas} perguntas) e a consulta não a contou",
                    enviados + 1
                );
            }
        }
    }

    /// Um canal que deixa sair as primeiras perguntas e recusa as seguintes,
    /// como uma máquina que perde a rota no meio da consulta.
    struct RecusaDepois {
        socket: tokio::net::UdpSocket,
        /// Quantas perguntas ainda saem.
        deixa: std::sync::atomic::AtomicU32,
    }

    impl Canal for RecusaDepois {
        async fn mandar(&self, datagrama: &[u8], destino: SocketAddr) -> std::io::Result<usize> {
            let saiu = self.deixa.fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |restam| restam.checked_sub(1),
            );
            if saiu.is_err() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NetworkUnreachable,
                    "a rota caiu no meio da consulta",
                ));
            }
            self.socket.send_to(datagrama, destino).await
        }

        async fn receber(&self, balde: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
            self.socket.recv_from(balde).await
        }
    }

    #[tokio::test]
    async fn uma_volta_recusada_depois_da_primeira_ainda_ouve_a_resposta_tardia() {
        // O par de `uma_volta_recusada_depois_da_primeira_conta_como_volta`,
        // com a consulta de verdade. A volta 1 sai; a 2, meio segundo depois,
        // o sistema recusa inteira; e a resposta à volta 1 chega aos 700 ms.
        // Parar na volta recusada perdia essa resposta, e a consulta voltava
        // como `PontoMudo` com o servidor no quarto.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let servidor = SocketAddr::from(SERVIDOR);
        let quem = marcas.clone();
        let (ponto, anotados) = ponto_falso(move |pedido, _| match pedido {
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::from_millis(700), encontro::aqui(marca, servidor))]
            }
            _ => Vec::new(),
        })
        .await;
        let canal = RecusaDepois {
            socket: tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            deixa: std::sync::atomic::AtomicU32::new(3),
        };

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&canal, ponto, &marcas, ate).await;

        assert_eq!(
            anotados.lock().unwrap().len(),
            3,
            "o ponto não recebeu exatamente as três perguntas da volta 1: o canal deste teste não \
             recusou a volta 2, e o teste não mede o que diz medir"
        );
        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(servidor),
                escuta: None,
            }),
            "a volta 2 não saiu e a consulta perdeu a resposta tardia à volta 1: o servidor estava \
             no quarto e quem chega fica sem o endereço de hoje"
        );
    }

    #[tokio::test]
    async fn a_ida_e_volta_se_mede_da_ultima_volta_que_saiu() {
        // O outro lado de contar a volta recusada: as perguntas no ar são as
        // da volta 1, e a ida e volta do `ONDE` se mede dela. Medida da volta
        // 2, que não saiu, a resposta aos 550 ms viraria uma ida e volta de
        // 50 ms, o teto cairia no piso, aos 650 ms, e a escuta que chega aos
        // 825 ms se perderia antes da hora da volta seguinte (1000 ms). As
        // duas margens são de ~175 ms: até o teto errado, e até a volta
        // seguinte.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (servidor, escuta) = (SocketAddr::from(SERVIDOR), SocketAddr::from(ESCUTA));
        let quem = marcas.clone();
        let (ponto, _) = ponto_falso(move |pedido, de| match pedido {
            encontro::Pedido::Onde { marca } => {
                vec![(Duration::from_millis(550), encontro::aqui(marca, de))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::from_millis(550), encontro::aqui(marca, servidor))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.escuta() => {
                vec![(Duration::from_millis(825), encontro::aqui(marca, escuta))]
            }
            _ => Vec::new(),
        })
        .await;
        let canal = RecusaDepois {
            socket: tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            deixa: std::sync::atomic::AtomicU32::new(3),
        };

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&canal, ponto, &marcas, ate).await;

        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            }),
            "a escuta respondeu antes da hora da volta seguinte e se perdeu: a ida e volta foi \
             medida da volta recusada, e não da que saiu, e o teto venceu cedo demais"
        );
    }

    #[test]
    fn o_teto_e_o_dobro_da_ida_e_volta_e_nunca_menos_que_a_folga() {
        // O C1 da pendência 51, item 17: os dois ramos do teto. Num caminho
        // curto, o dobro da ida e volta são microssegundos, e o piso dá ao
        // ponto tempo de responder à segunda pergunta da rajada. Num caminho
        // longo, o dobro manda.
        let primeira = tokio::time::Instant::now();
        assert_eq!(
            teto_depois_da_resposta(primeira, Duration::from_millis(1)),
            primeira + Duration::from_millis(100),
            "com 1 ms de ida e volta o teto não ficou no piso de 100 ms: num caminho curto a \
             consulta para antes de o ponto responder à segunda marca da rajada"
        );
        assert_eq!(
            teto_depois_da_resposta(primeira, Duration::from_millis(200)),
            primeira + Duration::from_millis(400),
            "com 200 ms de ida e volta o teto não ficou no dobro: num caminho longo a consulta \
             para antes de a segunda marca voltar, ou espera mais que duas idas e voltas por uma \
             pergunta que o ponto calou"
        );
    }

    /// O ponto falso que responde ao `ONDE` na hora, ao `QUEM` do servidor na
    /// hora e ao `QUEM` da escuta depois de `escuta_em`, se vier. Com
    /// `onde_volta` falso, o `ONDE` se perde.
    async fn ponto_que_demora_a_escuta(
        marcas: &Marcas,
        onde_volta: bool,
        escuta_em: Option<Duration>,
    ) -> (SocketAddr, Anotados) {
        let (servidor, escuta) = (SocketAddr::from(SERVIDOR), SocketAddr::from(ESCUTA));
        let quem = marcas.clone();
        ponto_falso(move |pedido, de| match pedido {
            encontro::Pedido::Onde { marca } if onde_volta => {
                vec![(Duration::ZERO, encontro::aqui(marca, de))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::ZERO, encontro::aqui(marca, servidor))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.escuta() => escuta_em
                .map(|demora| (demora, encontro::aqui(marca, escuta)))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        })
        .await
    }

    #[tokio::test]
    async fn a_segunda_marca_que_chega_dentro_do_teto_entra_na_resposta() {
        // O C1: o ponto responde ao servidor e ao `ONDE` na hora, e à escuta 15
        // ms depois. No laço local a ida e volta são microssegundos, e o teto é
        // o piso de 100 ms: a escuta chega dentro dele.
        //
        // 15 ms, e não perto do piso: o teste roda numa thread só, e uma pausa
        // dela que atravessasse o instante da escuta e o do teto faria os dois
        // timers vencerem no mesmo tique, com a consulta saindo sem a escuta.
        // Sobram ~85 ms para isso, e sem o piso o teto cai em microssegundos,
        // antes dos 15 ms.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (ponto, _) =
            ponto_que_demora_a_escuta(&marcas, true, Some(Duration::from_millis(15))).await;
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&socket, ponto, &marcas, ate).await;

        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: Some(SocketAddr::from(ESCUTA)),
            }),
            "a escuta respondeu 15 ms depois do servidor, dentro do teto de 100 ms, e ficou de \
             fora: a consulta parou na primeira marca, e quem chega manda o `LEVE` ao aviso \
             guardado em vez da escuta de hoje"
        );
    }

    #[tokio::test]
    async fn com_o_onde_perdido_a_espera_vai_ate_a_volta_seguinte_e_nao_repete() {
        // O C1, o ramo do `ONDE` perdido: sem ida e volta medida não há teto, e
        // a espera pela marca que falta vai até a hora da volta seguinte (meio
        // segundo depois do envio). A escuta que chega aos 300 ms entra; uma
        // que nunca chega custa essa meia volta, e a pergunta não se repete a
        // um ponto que está no ar.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (ponto, _) =
            ponto_que_demora_a_escuta(&marcas, false, Some(Duration::from_millis(300))).await;
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        assert_eq!(
            consultar_por(&socket, ponto, &marcas, ate).await,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: Some(SocketAddr::from(ESCUTA)),
            }),
            "o `ONDE` se perdeu, e a escuta que respondeu aos 300 ms, antes da hora da volta \
             seguinte, ficou de fora: sem ida e volta medida, a consulta inventou um teto"
        );

        let (ponto, anotados) = ponto_que_demora_a_escuta(&marcas, false, None).await;
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let comecou = std::time::Instant::now();
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&socket, ponto, &marcas, ate).await;
        let levou = comecou.elapsed();
        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: None,
            }),
            "o ponto deu o servidor e calou a escuta, e a consulta não voltou com o que achou"
        );
        assert!(
            levou >= INTERVALO_DOS_ENVIOS - Duration::from_millis(50)
                && levou < INTERVALO_DOS_ENVIOS * 2,
            "sem o `ONDE`, a espera pela escuta devia ir até a hora da volta seguinte \
             ({INTERVALO_DOS_ENVIOS:?}) e parar ali, e levou {levou:?}"
        );
        assert_eq!(
            anotados.lock().unwrap().len(),
            3,
            "a pergunta se repetiu a um ponto que já tinha respondido: quem não respondeu teve um \
             intervalo inteiro, e a volta a mais só atrasa a conexão"
        );
    }

    fn bilhete(ponto: &str) -> Bilhete {
        Bilhete::novo(ponto, "45.33.32.156:41234").expect("bilhete de teste")
    }

    /// Um `Bilhete` cujo ponto de encontro é `onde`, e cujo aviso é qualquer
    /// endereço global — `preparar` nunca lê `aviso`, só `ponto`.
    fn bilhete_de_teste(onde: SocketAddr) -> Bilhete {
        bilhete(&onde.to_string())
    }

    /// Um destino que **esta** máquina recusa, descoberto e não suposto.
    ///
    /// # Por que descoberto
    ///
    /// Porque três hipóteses sobre «o que todo sistema recusa» já foram escritas
    /// aqui, e as três estavam erradas. Sem broadcast o envio devolveria permissão
    /// negada em todo sistema — no Windows passa. A porta zero seria recusada em
    /// todo sistema — no Windows passa. Um socket desligado recusaria em todo
    /// sistema — no macOS passa. Todas afirmavam «todo sistema» tendo consultado
    /// um só.
    ///
    /// Medido, com o socket de pilha dupla que o produto abre e o mapeamento que
    /// ele aplica:
    ///
    /// | destino              | macOS  | Windows |
    /// |----------------------|--------|---------|
    /// | `255.255.255.255:0`  | recusa | passa   |
    /// | `127.0.0.1:0`        | recusa | passa   |
    /// | `[::1]:0`            | recusa | passa   |
    /// | socket desligado     | passa  | recusa  |
    /// | `0.0.0.0:0`          | recusa | recusa  |
    /// | `[::]:0`             | recusa | recusa  |
    ///
    /// O Linux não foi medido — o Docker desta máquina estava parado — e é
    /// exatamente esse buraco que as três hipóteses anteriores preencheram com
    /// palpite. Então em vez de escolher, o teste **experimenta**: manda um
    /// datagrama de sonda para cada candidato num socket igual ao do produto, e
    /// devolve o primeiro que a máquina recusar de fato. Onde nenhum for
    /// recusado, quem falha é o teste, dizendo isso — e não `avisar`, acusado de
    /// engolir um erro que nunca houve.
    ///
    /// Os dois candidatos são endereços **não especificados**: nenhum pacote sai
    /// para a rede se a máquina os aceitar, ao contrário do broadcast.
    ///
    /// Devolve o endereço **cru**: quem mapeia é `avisar`.
    fn destino_recusado() -> Option<SocketAddr> {
        let socket = abrir_socket_local()?;
        let local_e_seis = socket.local_addr().ok()?.is_ipv6();
        for cru in ["0.0.0.0:0", "[::]:0"] {
            let Ok(destino) = cru.parse::<SocketAddr>() else {
                continue;
            };
            let alvo = match (local_e_seis, destino.ip()) {
                (true, IpAddr::V4(quatro)) => {
                    SocketAddr::new(quatro.to_ipv6_mapped().into(), destino.port())
                }
                _ => destino,
            };
            if socket.send_to(&[0_u8; 96], alvo).is_err() {
                return Some(destino);
            }
        }
        None
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
        // # Duas tentativas de provocar a recusa por endereço, e as duas erradas
        //
        // Primeiro foi `255.255.255.255:9`, com o raciocínio de que sem
        // `SO_BROADCAST` o envio volta com permissão negada em todo sistema.
        // Depois foi `255.255.255.255:0`, com o raciocínio de que a porta zero é
        // recusada em todo sistema. Ambos afirmavam «em todo sistema» sem que
        // nenhum sistema além deste tivesse sido consultado, e o Windows
        // desmentiu os dois: lá, no socket de pilha dupla que o produto abre, o
        // envio **passa**.
        //
        // O que o teste quer não tem nada a ver com endereço: ele quer um envio
        // que falhe, para ver se o erro chega a quem chama. Então o destino é
        // **descoberto** — ver `destino_recusado`, que também guarda o que já foi
        // medido e o que ainda não foi.
        let Some(ponto) = destino_recusado() else {
            panic!(
                "nenhum destino candidato é recusado nesta máquina, e sem uma \
                 recusa este teste não tem um envio falho para observar. Não é \
                 `avisar` que está errado: é o mecanismo daqui que não vale neste \
                 sistema — acrescente um candidato em `destino_recusado`"
            );
        };
        let bilhete = bilhete(&ponto.to_string());
        let batida = Batida::preparar(&bilhete, Some(IMPRESSAO_DE_TESTE)).await;
        let Some(batida) = batida else {
            panic!(
                "preparar tem de dar certo mesmo com um ponto de encontro que só \
                 recusa no envio — é `avisar` que não vai conseguir"
            );
        };

        let erro = batida.avisar().await;
        assert!(
            erro.is_err(),
            "avisar engoliu o erro de um envio que o socket recusou: {erro:?}"
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
        let Some(ponto) = destino_recusado() else {
            panic!("nenhum destino candidato é recusado nesta máquina");
        };
        assert!(
            Batida::preparar(&bilhete(&ponto.to_string()), Some(FP))
                .await
                .is_some(),
            "preparar tem de dar certo para este bilhete — quem falha lá em cima \
             é o envio"
        );
    }

    /// Um ponto de teste da versão 2: responde ao `ONDE` (se `fala_v1`) e a
    /// cada `QUEM2` com o que `responder` disser, ou cala.
    async fn ponto_v2(
        fala_v1: bool,
        responder: impl Fn(&Marca) -> Option<encontro::Resposta2> + Send + 'static,
    ) -> SocketAddr {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO2 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let dados = balde.get(..lidos).unwrap_or_default();
                let resposta = if let Some(encontro::Pedido::Onde { marca }) =
                    encontro::analisar(dados)
                {
                    fala_v1.then(|| encontro::aqui(&marca, de))
                } else if let Some(encontro::Pedido2::Quem2 { marca }) = encontro::analisar2(dados)
                {
                    responder(&marca).map(|resposta| encontro::resposta2(&resposta))
                } else {
                    None
                };
                if let Some(resposta) = resposta {
                    let _ = socket.send_to(&resposta, de).await;
                }
            }
        });
        onde
    }

    fn marcas_da_sala() -> Marcas {
        Marcas::da_sala(&seele_proto::codigo::Codigo::da_chave(&[9; 32]))
    }

    #[tokio::test]
    async fn a_sala_achada_traz_o_servidor_a_escuta_e_as_dicas() {
        let marcas = marcas_da_sala();
        let (servidor, escuta) = (marcas.servidor().clone(), marcas.escuta().clone());
        let dica: SocketAddr = "192.168.0.10:8385".parse().unwrap();
        let ponto = ponto_v2(true, move |marca| {
            if *marca == servidor {
                Some(encontro::Resposta2::Aqui2 {
                    marca: marca.clone(),
                    endereco: "203.0.113.7:4001".parse().unwrap(),
                    dicas: vec![dica],
                })
            } else if *marca == escuta {
                Some(encontro::Resposta2::Aqui2 {
                    marca: marca.clone(),
                    endereco: "203.0.113.7:4002".parse().unwrap(),
                    dicas: Vec::new(),
                })
            } else {
                None
            }
        })
        .await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
            Localizacao::Achada {
                servidor: Some("203.0.113.7:4001".parse().unwrap()),
                escuta: Some("203.0.113.7:4002".parse().unwrap()),
                dicas: vec![dica],
            }
        );
    }

    #[tokio::test]
    async fn duas_marcas_vazias_sao_ninguem_mora() {
        let ponto = ponto_v2(true, |marca| {
            Some(encontro::Resposta2::Ninguem {
                marca: marca.clone(),
            })
        })
        .await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::NinguemMora
        );
    }

    #[tokio::test]
    async fn um_ponto_que_so_fala_o_enc1_e_dito_como_tal_e_nao_como_ninguem_mora() {
        // O ADR 0047, um degrau depois: com o ponto de produção sem o ENC/2,
        // «ALICE NÃO ESTÁ DISPONÍVEL» seria mentira. O ONDE responde, e os
        // QUEM2 calam.
        let ponto = ponto_v2(true, |_| None).await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::PontoSemEnc2
        );
    }

    #[tokio::test]
    async fn o_ponto_que_recusa_por_taxa_e_dito() {
        let ponto = ponto_v2(true, |marca| {
            Some(encontro::Resposta2::Recusa {
                marca: marca.clone(),
                motivo: encontro::Motivo::Taxa,
                agora: 1,
            })
        })
        .await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::PontoRecusou
        );
    }

    #[tokio::test]
    async fn um_ponto_mudo_para_as_duas_versoes_e_ponto_mudo() {
        let ponto = ponto_v2(false, |_| None).await;
        assert_eq!(
            localizar(
                &ponto.to_string(),
                &marcas_da_sala(),
                Duration::from_millis(300)
            )
            .await,
            Localizacao::PontoMudo
        );
    }

    #[tokio::test]
    async fn um_ponto_que_nao_e_endereco_nao_resolve() {
        assert_eq!(
            localizar("tem espaço", &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::PontoNaoResolve
        );
    }

    #[tokio::test]
    async fn uma_resposta_de_versao_1_nao_vira_achado_da_sala() {
        // Um AQUI da versão 1 com a marca da sala, de um ponto que não fala o
        // ENC/2, não é um endereço da sala: o QUEM2 é o único caminho.
        let marcas = marcas_da_sala();
        let servidor = marcas.servidor().clone();
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ponto = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO2 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let dados = balde.get(..lidos).unwrap_or_default();
                if let Some(encontro::Pedido::Onde { marca }) = encontro::analisar(dados) {
                    let _ = socket.send_to(&encontro::aqui(&marca, de), de).await;
                    let _ = socket
                        .send_to(
                            &encontro::aqui(&servidor, "203.0.113.66:1".parse().unwrap()),
                            de,
                        )
                        .await;
                }
            }
        });
        assert_eq!(
            localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
            Localizacao::PontoSemEnc2
        );
    }
}
