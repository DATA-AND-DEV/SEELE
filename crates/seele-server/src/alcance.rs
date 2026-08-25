//! A escada de alcançabilidade do ADR 0022.
//!
//! Um Dogma na mesma rede sempre funcionou. Pela internet, atrás de um roteador
//! doméstico, não — e o ADR 0022 trata isso como uma **escada**, tentada em
//! ordem, onde cada degrau vale por si e nenhum depende do seguinte:
//!
//! | Degrau | O que é | Onde mora |
//! |---|---|---|
//! | 1 | Endereço direto | já existia: VPS, porta encaminhada à mão |
//! | 2 | **IPv6** | [`abrir_escuta`] |
//! | 3 | **UPnP / NAT-PMP** | [`porta`] |
//! | 4 | **Furo de NAT com ponto de encontro** | [`encontro`] |
//! | 5 | Retransmissão | fora de escopo por decisão |
//!
//! # Por que os degraus 2 e 3 não custam nada ao modelo, e o 4 custa
//!
//! Nenhum dos dois primeiros põe terceiro no caminho. O degrau 2 é um endereço
//! que já é roteável e uma regra de firewall; o degrau 3 é um pedido ao
//! **próprio** roteador do anfitrião. Ninguém mais aprende que a conversa
//! existe.
//!
//! O degrau 4 aprende: um ponto de encontro vê que endereço falou com que
//! endereço, e quando. Nunca o conteúdo — o TOFU e o TLS 1.3 continuam ponta a
//! ponta —, e é por isso que ele é tentado **por último**, só quando os de cima
//! não deram. Um Dogma que já alcança de fora não ganha terceiro nenhum.

use std::net::{IpAddr, SocketAddr, UdpSocket};

use anyhow::{Context, Result};

pub mod encontro;
pub mod firewall;
pub mod interfaces;
pub mod pcp;
pub mod porta;

/// Que famílias de endereço a escuta de um Dogma alcança de fato.
///
/// Existe para poder ser **dita**. Um Dogma que perdeu o IPv6 porque a máquina
/// não tem, ou que perdeu o IPv4 porque o sistema recusou pilha dupla, é um
/// Dogma que metade de quem tentar não vai alcançar — e descobrir isso pelo
/// silêncio de "não conecta" é o defeito que o ADR 0022 nomeia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pilha {
    /// IPv6 e IPv4 no mesmo socket. O que se quer, e o que o padrão pede.
    Dupla,
    /// Só IPv4. A máquina não tem IPv6, ou o operador pediu `0.0.0.0`.
    SoIpv4,
    /// Só IPv6, porque o operador nomeou um endereço IPv6.
    SoIpv6,
}

impl Pilha {
    /// Se esta escuta atende quem chega por IPv6. Degrau 2 do ADR 0022.
    #[must_use]
    pub fn alcanca_ipv6(self) -> bool {
        matches!(self, Self::Dupla | Self::SoIpv6)
    }

    /// Se esta escuta atende quem chega por IPv4.
    #[must_use]
    pub fn alcanca_ipv4(self) -> bool {
        matches!(self, Self::Dupla | Self::SoIpv4)
    }
}

/// A escuta que o Dogma abriu **de verdade**, e a única fonte do que a escada
/// pode prometer.
///
/// # Por que a escada deixou de receber só a porta
///
/// Ela recebia `local: u16`, e por isso não tinha como saber em que famílias o
/// socket atende. Numa máquina em que a pilha dupla falha — Windows e os BSD,
/// pelo quadro de [`abrir_escuta`] — o Dogma recua para `0.0.0.0`, e a escada
/// continuava consultando o IPv6 global da máquina, achando um, e declarando
/// [`Degrau::Ipv6Direto`]. O convite anunciava um endereço IPv6 em que ninguém
/// estava escutando: nem um par com IPv6 nativo e firewall aberto entraria.
///
/// Foi medido em campo, num Windows hospedando: `Get-NetUDPEndpoint` mostrou
/// `0.0.0.0:8383`, e o convite saiu com o IPv6 da máquina.
///
/// O predicado que faltava já existia — [`Pilha::alcanca_ipv6`] — e não era
/// perguntado a ninguém. Por isso este tipo, e não um parâmetro a mais: todo
/// endereço que entra num [`Alcance`] passa por [`Escuta::anunciar`], que é
/// privado ao módulo e é o único caminho até um candidato. Não dá para afirmar
/// alcance sem perguntar ao socket, porque não há como montar o endereço sem
/// passar por aqui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Escuta {
    porta: u16,
    pilha: Pilha,
}

impl Escuta {
    /// A escuta de um Dogma: a porta em que ele atende e o que o socket serve.
    ///
    /// Os dois vêm do socket já aberto — ver [`crate::Server::local_addr`] e
    /// [`crate::Server::pilha`].
    #[must_use]
    pub fn nova(porta: u16, pilha: Pilha) -> Self {
        Self { porta, pilha }
    }

    /// A porta em que o Dogma atende.
    #[must_use]
    pub fn porta(self) -> u16 {
        self.porta
    }

    /// O que este socket serve.
    #[must_use]
    pub fn pilha(self) -> Pilha {
        self.pilha
    }

    /// Se quem chegar neste endereço vai encontrar alguém atendendo.
    #[must_use]
    pub fn serve(self, endereco: IpAddr) -> bool {
        match endereco {
            IpAddr::V4(_) => self.pilha.alcanca_ipv4(),
            IpAddr::V6(_) => self.pilha.alcanca_ipv6(),
        }
    }

    /// O endereço pronto para o convite, ou `None` se a escuta não o serve.
    ///
    /// Privado de propósito: é a guarda do módulo inteiro, e uma guarda que
    /// outro arquivo pode contornar não é guarda.
    fn anunciar(self, endereco: IpAddr) -> Option<SocketAddr> {
        self.serve(endereco)
            .then(|| SocketAddr::new(endereco, self.porta))
    }

    /// O mesmo, para um endereço que já traz a porta dele.
    ///
    /// É o caso do degrau 3: o roteador pode ter aberto uma porta externa
    /// diferente da interna, e é a dele que vai no convite.
    fn anunciar_com_porta(self, alvo: SocketAddr) -> Option<SocketAddr> {
        self.serve(alvo.ip()).then_some(alvo)
    }
}

/// Abre o socket UDP em que o Dogma vai atender.
///
/// # Por que isto não é `UdpSocket::bind`
///
/// Um socket IPv6 pode ou não atender também em IPv4, e **o padrão dessa opção
/// muda de sistema para sistema**. A documentação do próprio quinn diz isso em
/// `Endpoint::server` — *"Platform defaults for dual-stack sockets vary"* — e é
/// por isso que aquele construtor não serve aqui: ele faz um `UdpSocket::bind`
/// cru e herda o que o sistema quiser.
///
/// Medido, e não lembrado, porque a primeira versão deste comentário estava
/// errada:
///
/// | Sistema | Padrão do `IPV6_V6ONLY` | Como se controla |
/// |---|---|---|
/// | Linux | desligado — pilha dupla | `net.ipv6.bindv6only` |
/// | macOS | desligado — pilha dupla | `net.inet6.ip6.v6only` |
/// | Windows | **ligado** — só IPv6 | sem padrão de sistema |
/// | FreeBSD, OpenBSD | **ligado** — só IPv6 | `net.inet6.ip6.v6only` |
///
/// Duas coisas seguem daí. A primeira é que o Windows é o que quebra calado: o
/// mesmo código que atende as duas famílias no Linux e no macOS atende só IPv6
/// lá, e todo cliente IPv4 some. A segunda é que **nem no Linux e no macOS dá
/// para confiar no padrão** — os dois são `sysctl`, e um administrador que
/// tenha posto `1` produz a mesma falha nas outras duas plataformas.
///
/// Então `IPV6_V6ONLY` é escrito à mão, e **conferido de volta**: no OpenBSD o
/// `setsockopt` recusa desligar a opção, e há sistemas que devolvem `Ok` sem
/// que ela valha. Um Dogma que *acha* que atende em IPv4 e não atende é
/// exatamente o silêncio que este módulo existe para não produzir.
///
/// # O que cada endereço quer dizer
///
/// - `[::]` — tudo. Pilha dupla, e é o padrão de um Dogma.
/// - `0.0.0.0` — todas as interfaces **IPv4**, e só elas. Literal, porque é o
///   que o texto diz; quem escreve isso está pedindo IPv4.
/// - qualquer outro — aquele endereço e mais nenhum. O operador nomeou uma
///   interface e não há o que adivinhar.
///
/// # Quando a pilha dupla não sai
///
/// Cai para IPv4 e avisa. É a escolha consciente entre as duas perdas
/// possíveis: hoje quase todo cliente alcança IPv4 e uma minoria alcança IPv6,
/// então ficar sem IPv4 tira mais gente da sala do que ficar sem IPv6. A perda
/// vai para o log e para [`Pilha`], em vez de ficar só no log.
///
/// # Errors
///
/// Falha se nem IPv6 nem IPv4 aceitarem a porta — normalmente porque ela já
/// está em uso.
pub fn abrir_escuta(escuta: SocketAddr) -> Result<(UdpSocket, Pilha)> {
    // Um endereço nomeado é uma ordem, não uma sugestão.
    if !escuta.ip().is_unspecified() {
        let socket = ligar(escuta)?;
        let pilha = if escuta.is_ipv6() {
            Pilha::SoIpv6
        } else {
            Pilha::SoIpv4
        };
        return Ok((socket, pilha));
    }

    // `0.0.0.0` quer dizer IPv4, e quer dizer só isso.
    if escuta.is_ipv4() {
        return Ok((ligar(escuta)?, Pilha::SoIpv4));
    }

    match pilha_dupla(escuta) {
        Ok(socket) => Ok((socket, Pilha::Dupla)),
        Err(erro) => {
            tracing::warn!(
                %erro,
                "sem pilha dupla nesta máquina: o Dogma vai atender só em IPv4, \
                 e quem só tem IPv6 não vai alcançar"
            );
            let quatro = SocketAddr::from(([0, 0, 0, 0], escuta.port()));
            Ok((ligar(quatro)?, Pilha::SoIpv4))
        }
    }
}

/// Um socket IPv6 que atende IPv4 junto, ou um erro dizendo por que não.
fn pilha_dupla(escuta: SocketAddr) -> Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))
        .context("esta máquina não abre socket IPv6")?;
    socket
        .set_only_v6(false)
        .context("o sistema recusou desligar o IPV6_V6ONLY")?;
    // Conferido, e não presumido: é a opção cujo padrão muda de sistema para
    // sistema, e a única leitura que prova o que este socket vai atender.
    if socket
        .only_v6()
        .context("o sistema não diz se o socket é só IPv6")?
    {
        anyhow::bail!("o sistema manteve o socket em IPv6 puro apesar de aceitar o pedido");
    }
    socket
        .bind(&escuta.into())
        .with_context(|| format!("não deu para ligar em {escuta}"))?;
    // quinn põe o socket em não-bloqueante ao adotá-lo; fazer isso aqui é
    // barato e tira a dependência de esse detalhe continuar verdadeiro.
    socket
        .set_nonblocking(true)
        .context("não deu para pôr o socket em não-bloqueante")?;
    Ok(socket.into())
}

/// O endereço IPv6 **global** desta máquina, se ela tiver um.
///
/// É a outra metade do degrau 2: atender em IPv6 não serve de nada se o
/// anfitrião não tem como dizer ao amigo onde bater. Com um IPv6 global dos
/// dois lados não há NAT, não há porta a encaminhar e não há terceiro nenhum —
/// sobra a regra de firewall.
///
/// Mesmo truque de [`endereco_de_saida_v4`], na outra família: conectar
/// um socket UDP escolhe uma rota e associa um endereço local sem enviar pacote
/// nenhum. O alvo é `2001:db8::/32`, a faixa reservada para documentação
/// (RFC 3849), então nada é insinuado sobre alcançar um host real.
///
/// # O que é recusado, e por quê
///
/// Link-local (`fe80::/10`) e unique-local (`fc00::/7`) não atravessam a
/// internet. Devolvê-los seria pior que devolver nada: viram um link que parece
/// certo, é aceito pelo `seele://` e não conecta de lugar nenhum — que é o
/// silêncio que o ADR 0022 manda evitar.
///
/// # O que isto não resolve
///
/// Um IPv6 global **não** quer dizer alcançável: o firewall do roteador ainda
/// pode recusar a entrada, e não há daqui como saber. É por isso que o degrau 2
/// vale por si e não dispensa o degrau 3.
#[must_use]
pub fn endereco_de_saida_v6() -> Option<std::net::Ipv6Addr> {
    let socket = std::net::UdpSocket::bind("[::]:0").ok()?;
    socket.connect("[2001:db8::1]:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V6(endereco) if global_v6(endereco) => Some(endereco),
        _ => None,
    }
}

/// Se este IPv6 é um endereço que outra pessoa na internet alcançaria.
///
/// Escrito à mão porque `Ipv6Addr::is_unicast_global` ainda não é estável, e o
/// MSRV do ADR 0011 não espera por ele.
fn global_v6(endereco: std::net::Ipv6Addr) -> bool {
    let primeiro = endereco.segments().first().copied().unwrap_or_default();
    !endereco.is_loopback()
        && !endereco.is_unspecified()
        && !endereco.is_multicast()
        // fe80::/10, link-local: não sai da rede local.
        && primeiro & 0xffc0 != 0xfe80
        // fc00::/7, unique-local: o equivalente v6 do 192.168, e não roteia.
        && primeiro & 0xfe00 != 0xfc00
}

/// Os endereços desta máquina, com o truque antigo como rede de segurança.
///
/// A enumeração é a resposta certa e pode não existir — um sistema que recuse
/// `getifaddrs`, um contêiner apertado. Quando ela vem vazia, o endereço da
/// rota padrão ainda acerta em toda máquina **sem** VPN, que é a maioria; é
/// pior que enumerar e muito melhor que convidar para o loopback.
fn descobrir_enderecos() -> Vec<interfaces::Achado> {
    let achados = interfaces::descobrir();
    if !achados.is_empty() {
        return achados;
    }
    tracing::warn!("sem enumeração de interfaces; caindo no endereço da rota padrão");
    endereco_de_saida_v4()
        .map(|ip| interfaces::Achado {
            ip,
            // Nada se sabe da sub-rede nem da interface por este caminho, e
            // `None`/`Fisica` é o que menos promete: sem máscara o degrau 3 não
            // afirma que este endereço está na rede do roteador.
            mascara: None,
            origem: interfaces::Origem::Fisica,
        })
        .into_iter()
        .collect()
}

/// O endereço desta máquina **na rede de casa**, sem consultar a rota padrão.
///
/// É o que se manda para quem está na mesma rede, e é a pergunta que
/// [`endereco_de_saida_v4`] responde errado quando há VPN: aquela devolve o
/// endereço do túnel, que é o da rota padrão, e ninguém na sala ao lado alcança
/// aquilo. Aqui a placa de rede é procurada entre as interfaces, e o truque
/// antigo fica como recuo para quando não há enumeração nenhuma.
#[must_use]
pub fn endereco_de_rede_local() -> Option<IpAddr> {
    interfaces::descobrir()
        .into_iter()
        .find(|achado| {
            achado.ip.is_ipv4()
                && achado.classe() == interfaces::Origem::Fisica
                && achado.e_da_rede_local()
        })
        .map(|achado| achado.ip)
        .or_else(endereco_de_saida_v4)
}

/// O endereço desta máquina na rede local que ela usaria para sair.
///
/// Sem dependência e sem enumerar interfaces: conectar um socket UDP escolhe
/// uma rota e associa um endereço local sem enviar pacote nenhum, que é
/// exatamente a pergunta — "qual dos meus endereços outra pessoa veria". O alvo
/// é TEST-NET-3 (`203.0.113.0/24`, RFC 5737), reservado para documentação,
/// então nada é insinuado sobre alcançar um host real.
///
/// Numa casa isto devolve o `192.168.x.x` da máquina, que é o que o degrau 3
/// precisa saber para dizer ao roteador para onde encaminhar. Numa VPS devolve
/// o endereço público, e aí o degrau 1 já resolveu tudo.
#[must_use]
pub fn endereco_de_saida_v4() -> Option<std::net::IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("203.0.113.1:80").ok()?;
    let local = socket.local_addr().ok()?.ip();
    (!local.is_loopback() && !local.is_unspecified()).then_some(local)
}

/// Em que degrau da escada do ADR 0022 este Dogma parou.
///
/// Nomes estáveis e não frases: `specs` manda a frase que a pessoa lê morar na
/// casca, e este enum atravessa o `seele-ffi` até o JavaScript, onde a frase
/// está escrita.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Degrau {
    /// **Degrau 3.** O roteador abriu a porta. Endereço público, alcança todo
    /// mundo, e nenhum terceiro soube de nada.
    PortaNoRoteador,
    /// **Degrau 4.** Nenhum caminho direto, e um ponto de encontro apresentou
    /// esta máquina a quem tiver o link. Ver [`encontro`].
    ///
    /// Acima do degrau 2 na frase porque alcança mais gente: o IPv6 direto só
    /// serve a quem também tem IPv6, e o furo de NAT serve a quem tem IPv4, que
    /// é quase todo mundo. Abaixo do 3 na ordem de tentativa porque o 3 não põe
    /// terceiro nenhum no caminho — e este põe, mesmo que só na apresentação.
    ///
    /// **Não é garantia**, e a frase não pode prometer que é: com NAT simétrico
    /// dos dois lados o furo não abre, e a resposta a esse caso seria
    /// retransmissão, que o ADR 0022 deixou fora de escopo por decisão.
    FuroDeNat,
    /// **Degrau 2.** Sem porta no roteador, mas a máquina tem IPv6 global.
    /// Alcança quem também tem IPv6 — se o firewall do roteador deixar entrar,
    /// e isso não dá para saber daqui.
    Ipv6Direto,
    /// **Degrau 1, com endereço próprio.** A máquina tem IPv4 global na placa:
    /// uma VPS, uma máquina com IP fixo, ou uma porta já encaminhada à mão.
    ///
    /// O ADR 0022 chama isto de "o caminho de quem hospeda a sério", e é o
    /// único degrau em que nada foi pedido a ninguém — nem ao roteador, nem a um
    /// ponto de encontro. Por isso ele fica acima do 2 na frase: alcança quem
    /// tem IPv4, que é quase todo mundo.
    ///
    /// Variante própria, e não um `SoRedeLocal` de sorte, pelo critério que
    /// [`Degrau::RedeLocalOuVpn`] já usa: **o que a pessoa faz a respeito é
    /// diferente**. Aqui não há nada a fazer, e a frase que mandava encaminhar a
    /// porta num roteador inexistente era pior que silêncio.
    EnderecoDireto,
    /// **Degrau 1, com uma VPN no meio.** Não há endereço desta máquina que
    /// alcance de fora, e o único que sai daqui é o de um túnel.
    ///
    /// Variante própria, e não um `SoRedeLocal` com sorte, porque **o que a
    /// pessoa pode fazer a respeito é diferente**: aqui a resposta é desligar a
    /// VPN, ou pôr os dois lados na mesma. É o critério que `porta::FalhaAoAbrir`
    /// já usa para ter quatro variantes em vez de uma.
    ///
    /// Foi a situação do relato que originou este módulo: um Windows com
    /// Cloudflare WARP, cujo IPv6 de túnel passava por IPv6 global e fazia a
    /// escada declarar o degrau 2 — anunciando "alcança de qualquer lugar" para
    /// um endereço que não aceita entrada nenhuma.
    RedeLocalOuVpn,
    /// **Degrau 1.** Só quem estiver na mesma rede. É o que sempre existiu, e
    /// continua sendo a resposta honesta quando os dois de cima não deram.
    SoRedeLocal,
}

/// O que um candidato é, e o que ele precisa para funcionar.
///
/// Antes disto o tipo **era** a ordem: um `u8` solto de 0 a 5, que respondia
/// "onde ele vai na lista" e não respondia "o que ele precisa". A inversão é
/// literal — a ordem passa a ser derivada do tipo —, e o que ela destrava está
/// em [`Tipo::precisa_de_furo`]: sem essa pergunta, avisar o ponto de encontro
/// por candidato queimaria a janela de furos do anfitrião com endereços que
/// nunca dependeram de furo nenhum.
///
/// `Local` e `Global` são os *host candidates* da literatura de NAT traversal;
/// `Refletido` é o *server-reflexive*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// Um endereço desta máquina, na rede de casa. O caso comum, e o único com
    /// resposta imediata.
    Local,
    /// Um endereço desta máquina que sai para a internet: IPv6 nativo, ou o
    /// IPv4 de uma VPS. Alcança de fora e também de dentro.
    Global,
    /// O mesmo que [`Tipo::Global`], depois de o roteador ter **dito** que abriu
    /// o firewall para ele. Degrau 2, ver [`pcp`].
    ///
    /// # É uma aposta, e não uma prova, e a distinção é o ponto da variante
    ///
    /// O que se sabe é que alguém que fala PCP respondeu `SUCCESS` a um pedido de
    /// abrir a porta. Não se sabe que é ele quem filtra: a caixa da operadora
    /// acima dele pode ser quem barra, e daqui não há como distinguir — é a
    /// mesma armadilha que o degrau 3 pisou com o NAT duplo, e que
    /// [`porta::FalhaAoAbrir::SemSaidaParaInternet`] nomeia.
    ///
    /// A única prova seria um pacote entrando de fora, e isso exige uma sonda
    /// externa que não existe. Por isso a variante **não** nomeia degrau nenhum
    /// e faz uma coisa só: passar na frente do [`Tipo::Global`] cru na lista de
    /// candidatos. É uma aposta melhor fundamentada que a de hoje — hoje o
    /// endereço IPv6 é anunciado sem que ninguém tenha pedido nada a ninguém — e
    /// é barata de errar, porque a espera por candidato já foi encurtada.
    GlobalLiberado,
    /// A porta que o roteador abriu a pedido (degrau 3). Refletido por
    /// **configuração**, e não por observação: existe porque alguém pediu.
    /// Alcança de fora, e de dentro só com *hairpin*, que muitos roteadores não
    /// fazem.
    PortaNoRoteador,
    /// O endereço que o ponto de encontro observou (degrau 4). É o único que
    /// depende de o anfitrião ter furado o caminho.
    Refletido,
    /// Um túnel: Tailscale, WireGuard, WARP. Dois pares na mesma VPN se acham
    /// por aqui, e ninguém mais.
    Tunel,
    /// Uma ponte de contêiner. Não sai desta máquina, e está aqui só porque a
    /// lista de nomes que a reconhece é heurística.
    Ponte,
}

impl Tipo {
    /// Onde ele entra na lista de tentativa.
    ///
    /// # A ordem é por evidência, e não por qualidade do caminho
    ///
    /// Ela já foi por qualidade: `Global` vinha antes de `Refletido`, porque um
    /// endereço direto é melhor que um furado — não passa por terceiro nenhum.
    /// O argumento está certo sobre o caminho e errado sobre a escolha, e uma
    /// medição de campo mostrou o tamanho do erro.
    ///
    /// Quem entra tenta um candidato de cada vez e paga **quatro segundos** por
    /// palpite errado. Numa casa com IPv6 global e o firewall do roteador
    /// fechado para entrada — que é o padrão de fábrica de todo roteador
    /// doméstico —, os dois `Global` do convite consomem 8,4 s antes de o
    /// `Refletido` ser tentado. Ele respondeu em **358 ms**. Do 5G, com prazo
    /// apertado, o que se via era «tempo esgotado» num servidor que estava no
    /// ar e alcançável.
    ///
    /// O que separa os dois não é qualidade: é quem sabe. `Global` é o
    /// anfitrião afirmando a própria alcançabilidade, e ele **não tem como
    /// saber** — está escrito em [`Degrau::Ipv6Direto`]: «se o firewall do
    /// roteador deixar entrar, e isso não dá para saber daqui». `Refletido` é um
    /// terceiro tendo observado um pacote chegar, neste instante.
    ///
    /// Observação vence afirmação. `Local` continua na frente porque a casa é
    /// mais barata que tudo — o ADR 0006 registra a 0.5.0 tendo quebrado isso —
    /// e `PortaNoRoteador` continua acima do furo porque ali alguém abriu a
    /// porta de propósito, e um Dogma com porta aberta nem chega a furar.
    ///
    /// # E é por isso que o PCP entra **abaixo** do refletido
    ///
    /// [`Tipo::GlobalLiberado`] é o `Global` de quem pediu ao roteador que
    /// abrisse o firewall e ouviu `SUCCESS` (degrau 2, ver [`pcp`]). Isso é
    /// afirmação mais bem fundamentada — alguém do outro lado respondeu alguma
    /// coisa —, e continua sendo afirmação: o `Ok` não prova que quem respondeu é
    /// quem filtra. Pô-lo acima do `Refletido` desfaria a medição de campo que
    /// produziu esta ordem em troca de um palpite melhor, o que não é a mesma
    /// coisa que uma observação.
    ///
    /// Então ele ganha só o que é seguro ganhar: passa na frente do `Global`
    /// cru. Numa máquina com dois IPv6 globais isso decide qual dos dois
    /// sobrevive ao corte do [`LIMITE_DE_CANDIDATOS`], e numa casa sem ponto de
    /// encontro ele sobe para logo depois da porta do roteador.
    #[must_use]
    pub fn ordem(self) -> u8 {
        match self {
            Self::Local => 0,
            Self::PortaNoRoteador => 1,
            Self::Refletido => 2,
            Self::GlobalLiberado => 3,
            Self::Global => 4,
            Self::Tunel => 5,
            Self::Ponte => 6,
        }
    }

    /// Se alguém precisa furar um NAT para este endereço atender.
    ///
    /// Só o refletido. É o que separa "avisar o ponto de encontro antes deste
    /// candidato" de "não gastar metadado nem orçamento de furo com quem não
    /// precisa" — e o caso dos dois na mesma casa não perde um milissegundo.
    #[must_use]
    pub fn precisa_de_furo(self) -> bool {
        matches!(self, Self::Refletido)
    }

    /// Se ele não pode perder a vaga no convite para outro da mesma classe.
    ///
    /// Três, e cada um por um motivo diferente de perder a vaga custar caro:
    ///
    /// - o primeiro `Local`, porque sem ele os dois na mesma casa param de se
    ///   achar — foi o que a 0.5.0 quebrou, e o ADR 0006 registra;
    /// - a `PortaNoRoteador`, porque o endereço que sai do convite deixa de dar
    ///   nome ao degrau, e um degrau 3 que não é declarado faz [`Escada::subir`]
    ///   **devolver ao roteador** o mapeamento que ele tinha acabado de abrir.
    ///   Truncar aqui não encolhe um convite: desliga um caminho que já
    ///   funcionava;
    /// - o `Refletido`, porque ele é o único endereço que o degrau 4 produz.
    ///
    /// Custa menos vaga do que parece: [`Escada::subir`] só tenta o degrau 4
    /// quando o 3 não deu (`mapeada.is_none()`), então `PortaNoRoteador` e
    /// `Refletido` são mutuamente exclusivos na prática e a reserva gasta duas
    /// vagas das quatro, não três. `decidir` aceita os dois juntos porque é
    /// função pura e não tem como cobrar isso — e reservar os dois é o que faz
    /// o teste que os passa junto não medir outra coisa.
    ///
    /// Ver a reserva em [`Alcance::decidir`], que consulta este método e não
    /// repete a lista.
    #[must_use]
    pub fn insubstituivel(self) -> bool {
        matches!(self, Self::Local | Self::PortaNoRoteador | Self::Refletido)
    }
}

/// Quantos endereços cabem num convite.
///
/// Quatro. Cada um custa uma tentativa a quem recebe, e o prazo de cada
/// tentativa é tempo de espera antes de a sala abrir. Quatro cobre o caso
/// gordo — rede de casa, IPv6, porta do roteador, túnel — sem transformar o
/// link numa parede de texto para colar numa conversa.
const LIMITE_DE_CANDIDATOS: usize = 4;

impl Degrau {
    /// O nome estável que atravessa para a casca.
    #[must_use]
    pub fn nome(self) -> &'static str {
        match self {
            Self::PortaNoRoteador => "PortaNoRoteador",
            Self::FuroDeNat => "FuroDeNat",
            Self::Ipv6Direto => "Ipv6Direto",
            Self::EnderecoDireto => "EnderecoDireto",
            Self::RedeLocalOuVpn => "RedeLocalOuVpn",
            Self::SoRedeLocal => "SoRedeLocal",
        }
    }

    /// Se alguém de fora da rede local tem chance de chegar.
    ///
    /// "Chance", e não "certeza", e a diferença é honesta: no degrau 2 o
    /// firewall do roteador ainda pode recusar a entrada, e ninguém daqui sabe
    /// disso sem alguém do outro lado tentando.
    ///
    /// [`Degrau::RedeLocalOuVpn`] responde `false`: um endereço de VPN comum
    /// não aceita entrada, e prometer que aceita é a mentira que o relato de
    /// campo pegou.
    #[must_use]
    pub fn alcanca_de_fora(self) -> bool {
        matches!(
            self,
            Self::PortaNoRoteador | Self::FuroDeNat | Self::Ipv6Direto | Self::EnderecoDireto
        )
    }
}

/// Até onde este Dogma é alcançável, e por quê.
///
/// Campos privados de propósito: o único construtor é [`Alcance::decidir`], que
/// passa cada endereço pela [`Escuta`]. Um campo público deixaria escrever um
/// alvo que o socket não atende — que é exatamente o defeito que este módulo
/// acabou tendo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alcance {
    /// Os endereços a pôr no `seele://`, na ordem em que o cliente os tenta.
    ///
    /// Nunca vazio, e cada um servido pela escuta.
    ///
    /// # Por que mais de um
    ///
    /// Enquanto era um só, alguma situação sempre perdia: o endereço de fora
    /// não serve para quem está dentro — muitos roteadores domésticos não fazem
    /// *hairpin* —, e o de dentro não serve para quem está fora. A escada
    /// escolhia o degrau mais alto e jogava o resto fora, e foi assim que
    /// 0.5.0 perdeu o caso que sempre funcionou: os dois na mesma casa.
    ///
    /// A ordem é a que faz o caso comum ser o mais rápido: rede local primeiro,
    /// endereço global depois, porta do roteador em seguida, túnel por último.
    /// Tentar um endereço público antes faria quem está na sala ao lado esperar
    /// o prazo de um caminho que não volta.
    alvos: Vec<SocketAddr>,
    /// Qual degrau o produziu.
    degrau: Degrau,
    /// Por que o degrau 4 não deu — quando ele chegou a ser tentado.
    ///
    /// `None` também quando ninguém o pediu: um ponto de encontro desligado
    /// não é uma falha a explicar, é uma escolha de quem hospeda.
    encontro_recusado: Option<String>,
    /// Por que o degrau 3 não deu — quando não deu.
    ///
    /// Guardado mesmo quando o degrau 2 salvou o dia: "funcionou por IPv6, e a
    /// porta não abriu porque o roteador está atrás de CGNAT" são duas
    /// informações, e a segunda é a que explica por que um amigo só-IPv4 não
    /// entra.
    porta_recusada: Option<String>,
    /// Por que o pedido de abrir o firewall IPv6 não deu — quando não deu.
    ///
    /// Mesmo formato e mesma disciplina de [`Alcance::porta_recusada`], e a
    /// mesma regra: guardado em relação ao que ele explica. Some quando o
    /// roteador respondeu que abriu, e sobra quando não respondeu — que é o caso
    /// em que quem hospeda precisa da frase, porque é ela que diz por que um
    /// amigo com IPv6 nativo ainda pode não entrar pelo endereço IPv6 que está
    /// no convite.
    ///
    /// Também é preenchido quando não havia o que pedir — sem IPv6 global, ou
    /// com uma escuta que não atende IPv6 —, pelo mesmo critério que o degrau 3
    /// usa para a escuta só-IPv6: "não pedi, e por isto" é informação, e a
    /// ausência dela obrigaria quem lê a adivinhar entre "não pedi" e "pedi e
    /// deu certo".
    pcp_recusada: Option<String>,
}

impl Alcance {
    /// A escada inteira decidida sobre valores, sem tocar na rede.
    ///
    /// Separada de [`Escada::subir`] pelo mesmo motivo que o `seele-ffi` separa
    /// `build_destino` do `drive`: o que aqui se decide são valores, e uma
    /// decisão sobre valores não precisa de roteador para ser testada. A
    /// combinação que mordeu em campo — escuta só-IPv4 e IPv6 global na máquina
    /// — é encenável em duas linhas por causa desta separação.
    ///
    /// `mapeada` é o que o degrau 3 abriu, `liberada` é o endereço IPv6 que o
    /// roteador **disse** ter liberado no degrau 2, e `achados` são os endereços
    /// que as interfaces desta máquina têm. Cada um ainda pode ser recusado
    /// aqui, se a escuta não o servir.
    fn decidir(
        escuta: Escuta,
        mapeada: Option<SocketAddr>,
        liberada: Option<SocketAddr>,
        encontrado: Option<SocketAddr>,
        achados: &[interfaces::Achado],
        porta_recusada: Option<String>,
        pcp_recusada: Option<String>,
    ) -> Self {
        // A ordem de tentativa, e o motivo de cada posição:
        //
        // 0. a rede de casa — o caso comum, e o único com resposta imediata;
        // 1. um endereço global desta máquina — IPv6 nativo, ou o IPv4 de uma
        //    VPS. Alcança de fora e também de dentro;
        // 2. a porta que o roteador abriu — alcança de fora, e de dentro só se
        //    o roteador fizer *hairpin*, que muitos não fazem;
        // 3. um túnel — dois pares na mesma Tailscale se acham por aqui, e
        //    ninguém mais;
        // 4. uma ponte de contêiner — não sai desta máquina, e está aqui só
        //    porque a lista de nomes que a reconhece é heurística.
        let mut candidatos: Vec<(Tipo, SocketAddr)> = Vec::new();
        for achado in achados {
            let Some(alvo) = escuta.anunciar(achado.ip) else {
                continue;
            };
            let tipo = match (achado.classe(), achado.e_da_rede_local()) {
                (interfaces::Origem::Fisica, true) => Tipo::Local,
                // O degrau 2 não acrescenta endereço nenhum: ele muda o que se
                // sabe sobre um endereço que já estava aqui. Por isso a promoção
                // é uma **reclassificação** deste candidato, e não um candidato
                // novo — se fosse novo, o mesmo IPv6 apareceria duas vezes no
                // convite e gastaria duas das quatro vagas.
                (interfaces::Origem::Fisica, false)
                    if liberada.is_some_and(|liberada| liberada.ip() == achado.ip) =>
                {
                    Tipo::GlobalLiberado
                }
                (interfaces::Origem::Fisica, false) => Tipo::Global,
                (interfaces::Origem::Tunel, _) => Tipo::Tunel,
                (interfaces::Origem::Virtual, _) => Tipo::Ponte,
            };
            candidatos.push((tipo, alvo));
        }
        let externo = mapeada.and_then(|alvo| escuta.anunciar_com_porta(alvo));
        if let Some(externo) = externo {
            candidatos.push((Tipo::PortaNoRoteador, externo));
        }
        // O endereço que o ponto de encontro viu. Depois da porta do roteador —
        // aquele funciona sem ninguém bater em ponto nenhum — e antes do túnel,
        // que só serve a quem estiver na mesma VPN.
        let furado = encontrado.and_then(|alvo| escuta.anunciar_com_porta(alvo));
        if let Some(furado) = furado {
            candidatos.push((Tipo::Refletido, furado));
        }

        // Estável: dentro de uma classe vale a ordem em que a máquina listou as
        // interfaces, que é a ordem em que o próprio sistema as prefere.
        //
        // Reservar antes de cortar. Dois endereços são insubstituíveis e não
        // podem perder a vaga para um terceiro da mesma classe:
        //
        // - o **primeiro `Local`**, porque sem ele os dois na mesma casa param
        //   de se achar — foi o que a 0.5.0 quebrou, e o ADR 0006 registra;
        // - o **furado**, porque ele é o único endereço que o degrau 4 produz, e
        //   sem ele o degrau não alcança ninguém.
        //
        // O que dá a vaga é o excedente da mesma classe: uma segunda placa, um
        // segundo IPv6. Na prática isto vira "no máximo dois `Local`".
        candidatos.sort_by_key(|(tipo, _)| tipo.ordem());

        // O primeiro de cada tipo insubstituível, e a lista de quais são vem do
        // próprio [`Tipo::insubstituivel`]: repeti-la aqui era o que deixava o
        // método prometer uma reserva que a reserva não fazia.
        //
        // "O primeiro" importa só para o `Local`, que é o único que pode
        // aparecer mais de uma vez — duas placas, duas redes de casa. Os outros
        // dois vêm de `Option`, e são um ou nenhum.
        let mut reservados: Vec<SocketAddr> = Vec::new();
        let mut tipos_reservados: Vec<Tipo> = Vec::new();
        for (tipo, alvo) in &candidatos {
            if tipo.insubstituivel() && !tipos_reservados.contains(tipo) {
                tipos_reservados.push(*tipo);
                reservados.push(*alvo);
            }
        }

        let mut alvos: Vec<SocketAddr> = Vec::new();
        for alvo in &reservados {
            if !alvos.contains(alvo) {
                alvos.push(*alvo);
            }
        }
        for (_, alvo) in &candidatos {
            if alvos.len() >= LIMITE_DE_CANDIDATOS {
                break;
            }
            if !alvos.contains(alvo) {
                alvos.push(*alvo);
            }
        }
        // A ordem de tentativa é a das classes, e não a da reserva: reservar é
        // sobre quem sobrevive ao corte, nunca sobre quem vem primeiro.
        let posicao = |alvo: &SocketAddr| {
            candidatos
                .iter()
                .find(|(_, candidato)| candidato == alvo)
                .map_or(u8::MAX, |(tipo, _)| tipo.ordem())
        };
        alvos.sort_by_key(posicao);

        // Sem rede não há para onde convidar, e o loopback é o que sobra. Qual
        // dos dois depende da escuta pelo mesmo motivo de tudo acima: numa
        // escuta só-IPv6 o `127.0.0.1` não atende.
        if alvos.is_empty() {
            let recuo = escuta
                .anunciar(IpAddr::from([127, 0, 0, 1]))
                .or_else(|| escuta.anunciar(IpAddr::from(std::net::Ipv6Addr::LOCALHOST)))
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], escuta.porta())));
            alvos.push(recuo);
        }

        // O degrau é lido dos alvos que sobraram, e não das variáveis que os
        // produziram: assim não há como dizer que se alcança por um endereço que
        // não está no convite.
        //
        // **Estes dois `contains` são infalsificáveis hoje, e ficam.** Desde que
        // `externo` e `furado` entraram na reserva, `is_some()` e
        // `alvos.contains(...)` decidem igual em toda entrada — os dois são
        // candidatos assim que a escuta os serve, e um candidato reservado nunca
        // é truncado. Trocá-los de volta por `is_some()` não deixa teste nenhum
        // vermelho, e é justamente por isso que isto está escrito aqui: eles são
        // a rede embaixo da reserva, para o dia em que alguém mexer no
        // `LIMITE_DE_CANDIDATOS` ou nos tipos insubstituíveis. Quem cobra a
        // propriedade de fora é `um_degrau_nomeado_sempre_tem_o_endereco_dele_no_convite`,
        // que a afirma sobre a saída pública e não sobre este `if`.
        let degrau = if externo.is_some_and(|alvo| alvos.contains(&alvo)) {
            Degrau::PortaNoRoteador
        } else if furado.is_some_and(|alvo| alvos.contains(&alvo)) {
            Degrau::FuroDeNat
        } else if achados.iter().any(|achado| {
            matches!(achado.ip, IpAddr::V4(quatro) if porta::global_v4(quatro))
                && achado.classe() == interfaces::Origem::Fisica
                && escuta.serve(achado.ip)
        }) {
            // **Antes** do degrau 2, e é o mesmo argumento que o ADR 0022 usa
            // para pôr o degrau 4 acima do 2 na frase: um IPv4 público alcança
            // quem tem IPv4, que é quase todo mundo, e o IPv6 direto só alcança
            // quem também tem IPv6. Numa VPS de pilha dupla a ordem invertida
            // devolvia `Ipv6Direto`, cuja frase diz "só alcança quem também
            // tiver IPv6" — numa máquina com endereço IPv4 público na placa.
            //
            // A mesma conjunção do degrau 2, e pelo mesmo motivo: um degrau só
            // pode ser declarado se a escuta o servir. `tem_ipv4_global` faz
            // metade desta pergunta em `subir`, para decidir se vale bater no
            // ponto de encontro; aqui a outra metade é a escuta.
            Degrau::EnderecoDireto
        } else if achados.iter().any(|achado| {
            matches!(achado.ip, IpAddr::V6(seis) if global_v6(seis))
                && achado.classe() == interfaces::Origem::Fisica
                && escuta.serve(achado.ip)
        }) {
            Degrau::Ipv6Direto
        } else if achados
            .iter()
            .any(|achado| achado.classe() == interfaces::Origem::Tunel && escuta.serve(achado.ip))
        {
            Degrau::RedeLocalOuVpn
        } else {
            Degrau::SoRedeLocal
        };

        // Guardada em relação ao que ela explica, como a do degrau 3 — e o que
        // ela explica **não** é o degrau. O PCP não nomeia degrau nenhum, de
        // propósito: um `Ok` dele não é prova de alcance, e o degrau 2 continua
        // sendo `Ipv6Direto` com a frase honesta que ele sempre teve. O que o
        // `Ok` produz é o candidato promovido, então é contra ele que a recusa
        // some.
        let pcp_recusada = if liberada.is_some_and(|alvo| alvos.contains(&alvo)) {
            None
        } else {
            pcp_recusada
        };

        Self {
            alvos,
            degrau,
            encontro_recusado: None,
            porta_recusada: if degrau == Degrau::PortaNoRoteador {
                None
            } else {
                porta_recusada
            },
            pcp_recusada,
        }
    }

    /// Guarda por que o degrau 4 não deu.
    ///
    /// Separado de [`Alcance::decidir`] porque é informação sobre o caminho, e
    /// não sobre o resultado: a decisão de degrau se toma sobre os endereços que
    /// sobraram, e um motivo não é um endereço.
    fn com_recusa_do_encontro(mut self, motivo: Option<String>) -> Self {
        self.encontro_recusado = motivo;
        self
    }

    /// O primeiro endereço a tentar, e o que um cliente velho vai ler.
    #[must_use]
    pub fn alvo(&self) -> SocketAddr {
        self.alvos
            .first()
            .copied()
            // Inalcançável: `decidir` nunca deixa a lista vazia. O recuo é o
            // mesmo endereço que ele usaria, e não um `unwrap`.
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)))
    }

    /// Todos os endereços do convite, na ordem em que se tenta.
    #[must_use]
    pub fn alvos(&self) -> &[SocketAddr] {
        &self.alvos
    }

    /// Em que degrau da escada este Dogma parou.
    #[must_use]
    pub fn degrau(&self) -> Degrau {
        self.degrau
    }

    /// Por que o degrau 3 não deu, quando não deu.
    #[must_use]
    pub fn porta_recusada(&self) -> Option<&str> {
        self.porta_recusada.as_deref()
    }

    /// Por que o degrau 4 não deu, quando ele foi tentado e não deu.
    #[must_use]
    pub fn encontro_recusado(&self) -> Option<&str> {
        self.encontro_recusado.as_deref()
    }

    /// Por que o firewall IPv6 do roteador não foi aberto, quando não foi.
    ///
    /// `None` quer dizer que o roteador respondeu que abriu — e ver
    /// [`Tipo::GlobalLiberado`] para o que isso vale e o que não vale.
    #[must_use]
    pub fn pcp_recusada(&self) -> Option<&str> {
        self.pcp_recusada.as_deref()
    }
}

/// A escada do ADR 0022, subida uma vez, com o que ela abriu preso junto.
///
/// Existe para que o mapeamento de porta tenha dono: uma porta pedida ao
/// roteador precisa ser devolvida quando o Dogma fecha, e quem a devolve é
/// quem a segura.
pub struct Escada {
    alcance: Alcance,
    porta: Option<porta::PortaAberta>,
    firewall: Option<pcp::BuracoAberto>,
    encontro: Option<encontro::Encontro>,
}

impl std::fmt::Debug for Escada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Escada")
            .field("alcance", &self.alcance)
            .finish()
    }
}

impl Escada {
    /// Sobe a escada e para no degrau mais alto que funcionar.
    ///
    /// O degrau mais alto vira a **frase**; os endereços de todos os degraus
    /// viram os candidatos do convite, em ordem. Ver [`Alcance`].
    ///
    /// `escuta` é onde o Dogma está atendendo **de fato** — porta e famílias.
    /// Nenhum degrau é declarado sem que ela o sirva; ver [`Escuta`].
    pub async fn subir(escuta: Escuta, convocacao: Option<encontro::Convocacao>) -> Self {
        let achados = descobrir_enderecos();

        // Os degraus 2 e 3 são pedidos independentes, em protocolos diferentes,
        // sobre famílias diferentes: o UPnP fala SOAP por HTTP com o roteador
        // sobre uma porta IPv4, e o PCP fala UDP na 5351 sobre um firewall IPv6.
        // Feitos em fila eles somam os dois prazos de 3 s **antes de a sala
        // abrir**, e é a pessoa que apertou HOSPEDAR AQUI que paga. Juntos, o
        // relógio anda uma vez só. Não há ordem entre eles a preservar: nenhum
        // lê o resultado do outro.
        let (degrau_3, degrau_2) = tokio::join!(
            abrir_porta(&achados, escuta),
            abrir_firewall(&achados, escuta)
        );
        let (mapeada, recusa) = degrau_3;
        let (liberada, recusa_do_pcp) = degrau_2;

        // Degrau 4, e uma condição só. Ela é sobre não pagar metadado à toa:
        // com um endereço IPv4 global esta máquina já é alcançável sem
        // apresentação nenhuma — é o caso de uma VPS, que é o degrau 1.
        //
        // A porta do roteador **não** dispensa mais o ponto de encontro. Quem
        // abriu porta ainda perde quem não consegue atravessá-la: um roteador
        // sem *hairpin*, um firewall do outro lado. O bilhete é a segunda
        // chance, e recusá-lo porque o degrau 3 deu é a escada decidindo o que
        // **descartar** — que é o mesmo erro que a 0.5.0 cometeu com os
        // endereços, e que o ADR 0022 registra: ela escolhe a frase, nunca o
        // que sai do convite.
        let precisa = !tem_ipv4_global(&achados);
        let (encontro, recusa_do_encontro) = match (precisa, convocacao) {
            (true, Some(convocacao)) => match encontro::abrir(&convocacao).await {
                Ok(aberto) => (Some(aberto), None),
                Err(falha) => {
                    tracing::info!(%falha, "o degrau 4 não deu");
                    (None, Some(falha.to_string()))
                }
            },
            // Ninguém pediu, ou não era preciso. Nos dois casos nenhum pacote
            // saiu daqui para ponto de encontro nenhum.
            _ => (None, None),
        };

        let alcance = Alcance::decidir(
            escuta,
            mapeada.as_ref().map(porta::PortaAberta::externo),
            liberada.as_ref().map(pcp::BuracoAberto::liberado),
            encontro.as_ref().map(encontro::Encontro::publico),
            &achados,
            recusa,
            recusa_do_pcp,
        )
        .com_recusa_do_encontro(recusa_do_encontro);
        // A porta só é guardada se ela produziu o degrau. Um mapeamento que a
        // escuta não serve é devolvido na hora, e não largado: uma regra de
        // encaminhamento que sobra no roteador aponta para uma máquina que não
        // atende, e some só quando o prazo vence.
        let porta = match mapeada {
            Some(aberta) if alcance.degrau() == Degrau::PortaNoRoteador => Some(aberta),
            Some(aberta) => {
                tracing::warn!("o roteador abriu uma porta que esta escuta não atende; devolvendo");
                aberta.fechar().await;
                None
            }
            None => None,
        };
        // O mesmo cuidado, e o critério do candidato e não o do degrau — porque
        // o degrau 2 de propósito não muda de nome por causa do PCP. Um buraco
        // cujo endereço não sobreviveu ao corte do convite é uma regra no
        // firewall do roteador apontando para uma porta que ninguém vai tentar,
        // e renová-la seria falar com o roteador para sempre por nada.
        let firewall = match liberada {
            Some(aberto) if alcance.alvos().contains(&aberto.liberado()) => Some(aberto),
            Some(aberto) => {
                tracing::warn!("o buraco de firewall não virou candidato desta escuta; devolvendo");
                aberto.fechar().await;
                None
            }
            None => None,
        };
        // O mesmo cuidado do degrau 3 e **não** o mesmo critério. Aqui a
        // pergunta é se o endereço virou candidato, não se ele nomeou a frase:
        // um furo que está no convite precisa continuar sendo reavivado, mesmo
        // que a porta do roteador tenha ganhado o nome do degrau. Ver
        // [`o_furo_vale_a_pena`].
        let encontro = match encontro {
            Some(aberto) if o_furo_vale_a_pena(&alcance, escuta, aberto.publico()) => Some(aberto),
            Some(aberto) => {
                tracing::warn!("o furo de NAT não virou candidato desta escuta; largando");
                aberto.fechar();
                None
            }
            None => None,
        };

        Self {
            alcance,
            porta,
            firewall,
            encontro,
        }
    }

    /// O bilhete que vai no `seele://`, se o degrau 4 deu.
    ///
    /// `None` é o caso comum e o desejável: quem alcança de fora sem
    /// apresentação não põe ponto de encontro nenhum no link.
    #[must_use]
    pub fn bilhete(&self) -> Option<seele_proto::uri::Bilhete> {
        self.encontro.as_ref().map(encontro::Encontro::bilhete)
    }

    /// Até onde este Dogma chega.
    #[must_use]
    pub fn alcance(&self) -> &Alcance {
        &self.alcance
    }

    /// Devolve ao roteador o que foi pedido a ele.
    ///
    /// Espera, e por isso é `async` e consome: deixar a regra para trás no
    /// roteador é sujeira que sobrevive ao programa.
    pub async fn descer(self) {
        if let Some(encontro) = self.encontro {
            // Nada a devolver a ninguém: o degrau 4 não deixa regra em roteador
            // nenhum, e o mapeamento de NAT some sozinho quando isto para de
            // falar. O que se para é de reavivar e de atender avisos.
            encontro.fechar();
        }
        if let Some(porta) = self.porta {
            porta.fechar().await;
        }
        // Mesma sujeira, do outro lado do roteador: um buraco de firewall
        // deixado para trás aponta para uma máquina que não atende mais, e só
        // some quando o prazo da validade vence.
        if let Some(firewall) = self.firewall {
            firewall.fechar().await;
        }
    }
}

/// O degrau 3, com a condição que decide se ele chega a ser tentado.
///
/// Extraído de [`Escada::subir`] só para que ele e o degrau 2 possam ser
/// esperados juntos: `tokio::join!` precisa de dois futuros, e um `if` no meio
/// da função não é um futuro.
///
/// A condição é a de sempre, e a segunda metade dela é a que mordeu em campo:
/// além de saber para onde o roteador deve encaminhar, a escuta tem de atender
/// em IPv4 — um mapeamento para uma máquina que só atende IPv6 abriria uma porta
/// que não leva a lugar nenhum, com o mesmo sucesso mentiroso do CGNAT.
async fn abrir_porta(
    achados: &[interfaces::Achado],
    escuta: Escuta,
) -> (Option<porta::PortaAberta>, Option<String>) {
    if !escuta.pilha().alcanca_ipv4() {
        return (
            None,
            Some("esta escuta não atende em IPv4, então não há porta IPv4 a pedir".to_owned()),
        );
    }
    match porta::abrir(achados, escuta.porta()).await {
        Ok(aberta) => (Some(aberta), None),
        Err(falha) => {
            tracing::info!(%falha, "o degrau 3 não deu");
            (None, Some(falha.to_string()))
        }
    }
}

/// O degrau 2, com as condições que decidem se ele chega a ser tentado.
///
/// Duas, e as duas são a mesma pergunta que o degrau 3 faz na outra família: a
/// escuta tem de atender em IPv6, senão o buraco abriria para uma porta em que
/// ninguém está; e a porta tem de existir, porque o PCP a carrega num
/// `NonZeroU16` — na RFC 6887 a porta interna zero tem significado próprio,
/// "todas as portas", que é outro pedido.
///
/// Uma escuta com porta zero é impossível hoje — [`Escuta`] vem de um socket já
/// ligado —, e a frase existe assim mesmo pelo mesmo motivo que a do IPv4: "não
/// pedi, e por isto" é informação, e um `unwrap` aqui seria trocá-la por um
/// pânico.
async fn abrir_firewall(
    achados: &[interfaces::Achado],
    escuta: Escuta,
) -> (Option<pcp::BuracoAberto>, Option<String>) {
    if !escuta.pilha().alcanca_ipv6() {
        return (
            None,
            Some("esta escuta não atende em IPv6, então não há firewall IPv6 a abrir".to_owned()),
        );
    }
    let Some(porta) = std::num::NonZeroU16::new(escuta.porta()) else {
        return (
            None,
            Some("esta escuta não tem porta, então não há o que pedir ao roteador".to_owned()),
        );
    };
    match pcp::liberar(achados, porta).await {
        Ok(aberto) => (Some(aberto), None),
        Err(falha) => {
            tracing::info!(%falha, "o degrau 2 não ganhou o firewall aberto");
            (None, Some(falha.to_string()))
        }
    }
}

/// Se o furo que o degrau 4 abriu merece continuar de pé.
///
/// A pergunta é uma só: **o endereço dele está no convite?** Se está, alguém
/// vai bater nele, e parar de reavivá-lo deixaria o convite apontando para um
/// mapeamento de NAT que já morreu. Se não está, manter o degrau 4 vivo é falar
/// com um ponto de encontro por um caminho que ninguém vai usar.
///
/// # Por que não é `degrau() == FuroDeNat`
///
/// Porque eram duas perguntas escritas como uma. O degrau nomeia a frase que
/// quem hospeda lê, e o convite carrega todos os endereços — o ADR 0022 separa
/// as duas coisas desde que a 0.5.0 quebrou o caso dos dois na mesma casa
/// escolhendo um degrau e jogando o resto fora. Com a porta do roteador aberta
/// **e** o furo pronto, o degrau é `PortaNoRoteador` e o furo continua sendo o
/// endereço que salva quem não atravessa aquela porta: um roteador sem
/// *hairpin* de um lado, um firewall do outro.
fn o_furo_vale_a_pena(alcance: &Alcance, escuta: Escuta, publico: SocketAddr) -> bool {
    escuta
        .anunciar_com_porta(publico)
        .is_some_and(|alvo| alcance.alvos().contains(&alvo))
}

/// Se esta máquina já tem um endereço IPv4 que a internet alcança.
///
/// A pergunta que decide se o degrau 4 chega a ser tentado. Numa VPS a resposta
/// é sim, e o degrau 1 já resolveu tudo há muito tempo: pedir apresentação a um
/// terceiro seria pagar metadado por um caminho que já existe.
fn tem_ipv4_global(achados: &[interfaces::Achado]) -> bool {
    achados.iter().any(|achado| {
        achado.classe() == interfaces::Origem::Fisica
            && matches!(achado.ip, IpAddr::V4(quatro) if porta::global_v4(quatro))
    })
}

/// Um `bind` com a mensagem de erro que diz onde foi.
fn ligar(escuta: SocketAddr) -> Result<UdpSocket> {
    let socket =
        UdpSocket::bind(escuta).with_context(|| format!("não deu para ligar em {escuta}"))?;
    socket
        .set_nonblocking(true)
        .context("não deu para pôr o socket em não-bloqueante")?;
    Ok(socket)
}

#[cfg(test)]
impl Alcance {
    /// [`Alcance::decidir`] sem o degrau 4, para os testes que não são sobre ele.
    fn decidir_sem_encontro(
        escuta: Escuta,
        mapeada: Option<SocketAddr>,
        achados: &[interfaces::Achado],
        porta_recusada: Option<String>,
    ) -> Self {
        Self::decidir_sem_pcp(escuta, mapeada, None, achados, porta_recusada)
    }

    /// [`Alcance::decidir`] sem o pedido de firewall do degrau 2.
    ///
    /// A promoção que o PCP faz é sobre **um** candidato, e a maioria destes
    /// testes não fala dele; passar `None` em cada chamada só faria a linha
    /// crescer sem dizer nada. Quem é sobre o degrau 2 chama a `decidir`
    /// inteira, e é assim que se vê de longe qual teste é sobre o quê.
    fn decidir_sem_pcp(
        escuta: Escuta,
        mapeada: Option<SocketAddr>,
        encontrado: Option<SocketAddr>,
        achados: &[interfaces::Achado],
        porta_recusada: Option<String>,
    ) -> Self {
        Self::decidir(
            escuta,
            mapeada,
            None,
            encontrado,
            achados,
            porta_recusada,
            None,
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::net::Ipv6Addr;

    /// A porta zero deixa o sistema escolher, e dois testes em paralelo não
    /// disputam nada.
    fn qualquer_porta_v6() -> SocketAddr {
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))
    }

    #[test]
    fn a_escuta_padrao_atende_as_duas_familias() {
        // O degrau 2 do ADR 0022 inteiro está nesta asserção. Sem ela, um
        // Dogma com IPv6 dos dois lados continua não atendendo em IPv6 — que
        // era o estado antes desta mudança.
        let Ok((socket, pilha)) = abrir_escuta(qualquer_porta_v6()) else {
            eprintln!("pulado: esta máquina não liga em nenhuma porta UDP");
            return;
        };

        if pilha != Pilha::Dupla {
            // Numa máquina sem IPv6 a queda para IPv4 é o comportamento certo,
            // e o teste diz isso em voz alta em vez de passar calado.
            eprintln!("pulado: esta máquina não faz pilha dupla, e caiu para {pilha:?}");
            assert_eq!(pilha, Pilha::SoIpv4, "caiu para uma pilha que não é IPv4");
            return;
        }

        assert!(pilha.alcanca_ipv6(), "pilha dupla que não alcança IPv6");
        assert!(pilha.alcanca_ipv4(), "pilha dupla que não alcança IPv4");
        let local = socket.local_addr().expect("o socket não diz onde ligou");
        assert!(local.is_ipv6(), "a pilha dupla mora num socket IPv6");
        assert_ne!(local.port(), 0, "o sistema não escolheu porta");
    }

    #[test]
    fn zero_zero_zero_zero_continua_querendo_dizer_ipv4() {
        // `0.0.0.0` é um endereço IPv4 e quem o escreve está pedindo IPv4.
        // Promovê-lo a pilha dupla por conveniência faria o texto do operador
        // dizer uma coisa e o socket fazer outra.
        let Ok((socket, pilha)) = abrir_escuta(SocketAddr::from(([0, 0, 0, 0], 0))) else {
            eprintln!("pulado: esta máquina não liga em nenhuma porta UDP IPv4");
            return;
        };
        assert_eq!(pilha, Pilha::SoIpv4);
        assert!(!pilha.alcanca_ipv6());
        assert!(
            socket
                .local_addr()
                .expect("o socket não diz onde ligou")
                .is_ipv4(),
            "pediu IPv4 e recebeu outra coisa"
        );
    }

    #[test]
    fn um_endereco_nomeado_e_uma_ordem() {
        // O operador que escreve uma interface está excluindo as outras de
        // propósito, e "melhorar" isso para pilha dupla abriria o Dogma numa
        // rede em que ele foi mantido fechado.
        let Ok((socket, pilha)) = abrir_escuta(SocketAddr::from(([127, 0, 0, 1], 0))) else {
            eprintln!("pulado: esta máquina não liga em 127.0.0.1");
            return;
        };
        assert_eq!(pilha, Pilha::SoIpv4);
        let local = socket.local_addr().expect("o socket não diz onde ligou");
        assert_eq!(local.ip(), std::net::IpAddr::from([127, 0, 0, 1]));
    }

    #[tokio::test]
    async fn a_escada_para_num_degrau_e_diz_qual_e_por_que() {
        // Roda em qualquer rede, e é por isso que ela não afirma **qual**
        // degrau: isso depende do roteador de quem estiver rodando. O que ela
        // afirma é o que vale nas três — que há sempre um degrau, que o alvo é
        // um endereço aonde alguém poderia ir, e que quando a escada não chegou
        // ao topo ela diz **por quê**.
        let escada = Escada::subir(Escuta::nova(8383, Pilha::Dupla), None).await;
        let alcance = escada.alcance().clone();
        eprintln!("esta rede parou em {:?}", alcance.degrau());

        assert_eq!(alcance.alvo().port(), 8383, "a escada trocou a porta");
        assert!(
            !alcance.alvo().ip().is_unspecified(),
            "convidou para o curinga, que não é lugar nenhum: {}",
            alcance.alvo()
        );

        match alcance.degrau() {
            Degrau::PortaNoRoteador => {
                assert!(alcance.porta_recusada().is_none(), "abriu e reclamou junto");
                assert!(alcance.degrau().alcanca_de_fora());
            }
            // Degrau 1 com endereço próprio, e o único que não cobra razão do
            // degrau 3: numa VPS o roteador pode ter aberto uma porta cujo
            // endereço externo esta escuta não serve — pedido atendido, nada
            // recusado, `porta_recusada` vazio. Exigir a razão aqui seria um
            // `expect` que só panica na máquina de campo que tiver essa
            // combinação, que é o pior lugar para descobrir.
            Degrau::EnderecoDireto => {
                assert!(alcance.degrau().alcanca_de_fora());
            }
            // O ponto todo do ADR 0022: quando o degrau 3 não deu, a razão não
            // pode sumir. É ela que explica a quem hospeda por que um amigo não
            // entra, e sem ela sobra "não conecta".
            Degrau::FuroDeNat
            | Degrau::Ipv6Direto
            | Degrau::RedeLocalOuVpn
            | Degrau::SoRedeLocal => {
                let motivo = alcance
                    .porta_recusada()
                    .expect("o degrau 3 não deu e ninguém disse por quê");
                assert!(motivo.len() > 20, "o motivo não explica nada: {motivo}");
            }
        }

        escada.descer().await;
    }

    #[tokio::test]
    async fn o_alvo_da_escada_sempre_vira_um_convite_que_se_le_de_volta() {
        // O alvo pode ser IPv4 ou IPv6 dependendo do degrau, e um IPv6 sem
        // colchetes não atravessa o `seele://`. Como o degrau varia com a rede,
        // este é o teste que pega o caso IPv6 na máquina de quem o tiver.
        let escada = Escada::subir(Escuta::nova(8383, Pilha::Dupla), None).await;
        let alvo = escada.alcance().alvo();
        escada.descer().await;

        let convite = seele_proto::uri::Convite::novo(alvo.to_string()).to_string();
        let lido = seele_proto::uri::analisar(&convite)
            .unwrap_or_else(|erro| panic!("geramos um convite ilegível: {convite} ({erro:?})"));
        let separado = lido.endereco().expect("o alvo gerado não se separa");
        assert_eq!(separado.porta, alvo.port());
        assert_eq!(
            separado.maquina.parse::<std::net::IpAddr>().ok(),
            Some(alvo.ip()),
            "o endereço mudou ao atravessar o convite: {convite}"
        );
    }

    /// O IPv6 global de uma máquina qualquer, para as encenações abaixo.
    fn um_ipv6_global() -> Ipv6Addr {
        // Documentação (RFC 3849), e passa em `global_v6` — que é justamente o
        // ponto: `global_v6` não distingue este de um IPv6 de verdade, e não
        // tem como. Quem decide se ele serve é a escuta.
        "2001:db8::1".parse().unwrap_or(Ipv6Addr::UNSPECIFIED)
    }

    /// Um endereço achado numa placa de rede de verdade.
    fn na_placa(ip: &str) -> interfaces::Achado {
        interfaces::Achado {
            ip: ip.parse().unwrap_or(IpAddr::from([0, 0, 0, 0])),
            mascara: None,
            origem: interfaces::Origem::Fisica,
        }
    }

    /// O mesmo, numa interface de túnel — WARP, Tailscale, WireGuard.
    fn no_tunel(ip: &str) -> interfaces::Achado {
        interfaces::Achado {
            ip: ip.parse().unwrap_or(IpAddr::from([0, 0, 0, 0])),
            mascara: None,
            origem: interfaces::Origem::Tunel,
        }
    }

    #[test]
    fn uma_escuta_so_ipv4_nunca_declara_um_degrau_de_ipv6() {
        // O defeito de campo, encenado sem rede nenhuma: um Windows onde a
        // pilha dupla falhou (`Get-NetUDPEndpoint` mostrou `0.0.0.0:8383`) e
        // que **tem** IPv6 global. A escada declarava degrau 2 e o convite saía
        // com um endereço IPv6 em que ninguém estava escutando — nem um par com
        // IPv6 nativo e firewall aberto entraria.
        let alcance = Alcance::decidir_sem_encontro(
            Escuta::nova(8383, Pilha::SoIpv4),
            None,
            &[na_placa("192.168.0.30"), na_placa("2001:db8::1")],
            Some("o roteador não respondeu ao pedido de porta".to_owned()),
        );

        assert_ne!(
            alcance.degrau(),
            Degrau::Ipv6Direto,
            "prometeu IPv6 numa escuta que só atende IPv4: {:?}",
            alcance.alvos()
        );
        assert!(
            alcance.alvos().iter().all(SocketAddr::is_ipv4),
            "o convite levaria um endereço que este socket não atende: {:?}",
            alcance.alvos()
        );
        assert_eq!(
            alcance.alvo(),
            SocketAddr::from(([192, 168, 0, 30], 8383)),
            "sobrou o degrau 1, e ele tem de ser o endereço da máquina na rede"
        );
    }

    #[test]
    fn uma_escuta_so_ipv6_nunca_anuncia_o_ipv4_da_maquina() {
        // A mesma guarda na outra família, e ela não é simétrica de graça: o
        // degrau 1 sempre caía no IPv4 da máquina, e num socket IPv6 puro esse
        // endereço não atende. O loopback de recurso muda de família junto.
        let alcance = Alcance::decidir_sem_encontro(
            Escuta::nova(8383, Pilha::SoIpv6),
            None,
            &[na_placa("192.168.0.30")],
            None,
        );
        assert!(
            alcance.alvos().iter().all(SocketAddr::is_ipv6),
            "anunciou IPv4 numa escuta que só atende IPv6: {:?}",
            alcance.alvos()
        );
    }

    #[test]
    fn uma_porta_aberta_que_a_escuta_nao_atende_nao_vira_degrau_3() {
        // O mesmo raciocínio no degrau 3, que é onde é mais fácil esquecê-lo:
        // um roteador pode responder com endereço externo IPv6, e mapear para
        // uma escuta que não atende IPv6 é abrir uma porta que não leva a
        // lugar nenhum — o "sucesso mentiroso" que `porta` existe para evitar.
        let externo = SocketAddr::new(um_ipv6_global().into(), 9000);
        let alcance = Alcance::decidir_sem_encontro(
            Escuta::nova(8383, Pilha::SoIpv4),
            Some(externo),
            &[na_placa("192.168.0.30")],
            None,
        );
        assert_ne!(alcance.degrau(), Degrau::PortaNoRoteador);
        assert!(!alcance.alvos().contains(&externo));
    }

    #[test]
    fn uma_escuta_de_pilha_dupla_continua_subindo_a_escada_inteira() {
        // A outra metade da guarda: ela recusa o que o socket não serve e não
        // pode recusar mais nada. Sem esta, "nunca prometa nada" passaria nos
        // três testes acima.
        let escuta = Escuta::nova(8383, Pilha::Dupla);
        let externo = SocketAddr::from(([203, 0, 113, 7], 9000));

        let tres = Alcance::decidir_sem_encontro(
            escuta,
            Some(externo),
            &[na_placa("192.168.0.30"), na_placa("2001:db8::1")],
            None,
        );
        assert_eq!(tres.degrau(), Degrau::PortaNoRoteador);
        assert!(
            tres.alvos().contains(&externo),
            "o degrau 3 perdeu a porta do roteador: {:?}",
            tres.alvos()
        );

        let dois = Alcance::decidir_sem_encontro(
            escuta,
            None,
            &[na_placa("192.168.0.30"), na_placa("2001:db8::1")],
            Some("nenhum roteador respondeu".to_owned()),
        );
        assert_eq!(dois.degrau(), Degrau::Ipv6Direto);
        assert!(dois
            .alvos()
            .contains(&SocketAddr::new(um_ipv6_global().into(), 8383)));
        assert!(
            dois.porta_recusada().is_some(),
            "o degrau 2 salvou o dia e engoliu o motivo de o 3 não ter dado"
        );
    }

    #[test]
    fn a_rede_de_casa_e_sempre_o_primeiro_candidato() {
        // A ordem é o que faz o caso comum ser rápido. Um endereço público com
        // prazo longo na frente faria quem está na sala ao lado esperar por um
        // caminho que muitos roteadores domésticos não fazem voltar — eles não
        // fazem *hairpin*.
        let escuta = Escuta::nova(8383, Pilha::Dupla);
        let externo = SocketAddr::from(([203, 0, 113, 7], 9000));
        let alcance = Alcance::decidir_sem_encontro(
            escuta,
            Some(externo),
            &[
                na_placa("2001:db8::1"),
                na_placa("192.168.0.30"),
                no_tunel("172.16.0.2"),
            ],
            None,
        );

        assert_eq!(
            alcance.alvo(),
            SocketAddr::from(([192, 168, 0, 30], 8383)),
            "o primeiro candidato não é o da rede de casa: {:?}",
            alcance.alvos()
        );
        let posicao = |procurado: SocketAddr| {
            alcance
                .alvos()
                .iter()
                .position(|alvo| *alvo == procurado)
                .unwrap_or(usize::MAX)
        };
        // A porta do roteador vem **antes** do IPv6, e este teste já exigia o
        // contrário. Ver o doc de `Tipo::ordem`: uma porta que alguém abriu de
        // propósito é fato, e um IPv6 global é o anfitrião supondo que o
        // firewall dele deixa entrar — que por padrão não deixa.
        assert!(
            posicao(externo) < posicao(SocketAddr::new(um_ipv6_global().into(), 8383)),
            "o IPv6 veio antes da porta do roteador, e ele é um palpite: {:?}",
            alcance.alvos()
        );
        assert!(
            posicao(externo) < posicao(SocketAddr::from(([172, 16, 0, 2], 8383))),
            "o túnel não é o último: {:?}",
            alcance.alvos()
        );
    }

    #[test]
    fn o_convite_leva_a_rede_de_casa_mesmo_quando_o_degrau_e_mais_alto() {
        // O que 0.5.0 perdeu, e o motivo de haver mais de um candidato: com a
        // porta aberta no roteador, o convite passou a levar **só** o endereço
        // externo — e quem estava na mesma casa deixou de entrar, porque o
        // roteador não devolve o próprio endereço para dentro.
        let alcance = Alcance::decidir_sem_encontro(
            Escuta::nova(8383, Pilha::Dupla),
            Some(SocketAddr::from(([203, 0, 113, 7], 9000))),
            &[na_placa("192.168.0.30")],
            None,
        );
        assert_eq!(alcance.degrau(), Degrau::PortaNoRoteador);
        assert!(
            alcance
                .alvos()
                .contains(&SocketAddr::from(([192, 168, 0, 30], 8383))),
            "o degrau 3 jogou fora o endereço da rede local: {:?}",
            alcance.alvos()
        );
    }

    #[test]
    fn um_ipv6_de_tunel_nao_vira_promessa_de_alcance() {
        // O relato inteiro numa asserção: o IPv6 do WARP é um unicast global de
        // verdade, passa em `global_v6`, e a escada declarava degrau 2 —
        // escrevendo "alcança de qualquer lugar" embaixo de um link que não
        // aceita entrada nenhuma. O endereço continua no convite, por último,
        // porque dois pares na mesma VPN se acham por ele.
        let alcance = Alcance::decidir_sem_encontro(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            &[
                na_placa("192.168.0.30"),
                no_tunel("2606:4700:110:8a3f::2"),
                no_tunel("172.16.0.2"),
            ],
            Some("nenhum roteador respondeu ao pedido de porta".to_owned()),
        );

        assert_eq!(alcance.degrau(), Degrau::RedeLocalOuVpn);
        assert!(
            !alcance.degrau().alcanca_de_fora(),
            "prometeu alcance de fora por um endereço de VPN"
        );
        assert_eq!(alcance.alvo(), SocketAddr::from(([192, 168, 0, 30], 8383)));
        assert!(
            alcance
                .alvos()
                .iter()
                .any(|alvo| alvo.ip().to_string() == "2606:4700:110:8a3f::2"),
            "o endereço do túnel sumiu do convite: {:?}",
            alcance.alvos()
        );
    }

    #[test]
    fn o_convite_nao_cresce_sem_limite() {
        // Cada candidato custa uma tentativa a quem recebe, e o link é para
        // colar numa conversa.
        let muitos: Vec<interfaces::Achado> = (0..12)
            .map(|n| na_placa(&format!("192.168.0.{n}")))
            .collect();
        let alcance =
            Alcance::decidir_sem_encontro(Escuta::nova(8383, Pilha::Dupla), None, &muitos, None);
        assert!(
            alcance.alvos().len() <= LIMITE_DE_CANDIDATOS,
            "o convite levou {} endereços",
            alcance.alvos().len()
        );
    }

    #[test]
    fn sem_endereco_nenhum_o_alvo_ainda_e_um_lugar() {
        // Uma máquina sem rede. Não há convite útil a montar, e um `0.0.0.0`
        // ou uma lista vazia seriam pior que o loopback: o primeiro é um
        // endereço que não é lugar nenhum, e a segunda quebraria quem lê.
        let alcance =
            Alcance::decidir_sem_encontro(Escuta::nova(8383, Pilha::Dupla), None, &[], None);
        assert_eq!(alcance.alvos().len(), 1);
        assert!(alcance.alvo().ip().is_loopback());
        assert_eq!(alcance.degrau(), Degrau::SoRedeLocal);
    }

    #[test]
    fn o_furo_de_nat_vira_candidato_e_degrau_quando_o_roteador_nao_abriu() {
        // O degrau 4 inteiro, decidido sobre valores: sem porta no roteador, um
        // ponto de encontro devolveu o endereço público desta máquina, e ele
        // entra no convite **junto** com o da rede de casa — que continua sendo
        // o primeiro, porque quem está na mesma casa não precisa de furo nenhum.
        let publico = SocketAddr::from(([45, 33, 32, 156], 41234));
        let alcance = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            Some(publico),
            &[na_placa("192.168.0.30")],
            Some("o roteador respondeu, e o endereço dele não sai para a internet".to_owned()),
        );

        assert_eq!(alcance.degrau(), Degrau::FuroDeNat);
        assert!(alcance.degrau().alcanca_de_fora());
        assert_eq!(
            alcance.alvo(),
            SocketAddr::from(([192, 168, 0, 30], 8383)),
            "o furo de NAT tomou o lugar da rede de casa: {:?}",
            alcance.alvos()
        );
        assert!(
            alcance.alvos().contains(&publico),
            "o endereço do furo não entrou no convite: {:?}",
            alcance.alvos()
        );
        assert!(
            alcance.porta_recusada().is_some(),
            "o degrau 4 salvou o dia e engoliu o motivo de o 3 não ter dado"
        );
    }

    #[test]
    fn a_porta_no_roteador_continua_ganhando_do_furo_de_nat() {
        // O degrau 3 não põe terceiro nenhum no caminho, e o 4 põe. Com os dois
        // disponíveis, quem manda é o 3 — e é por isso que a escada nem chega a
        // perguntar ao ponto de encontro quando o roteador abriu.
        let externo = SocketAddr::from(([203, 0, 113, 7], 9000));
        let publico = SocketAddr::from(([45, 33, 32, 156], 41234));
        let alcance = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::Dupla),
            Some(externo),
            Some(publico),
            &[na_placa("192.168.0.30")],
            None,
        );
        assert_eq!(alcance.degrau(), Degrau::PortaNoRoteador);
    }

    #[test]
    fn um_endereco_de_encontro_que_a_escuta_nao_serve_nao_vira_degrau_4() {
        // A mesma guarda dos outros degraus, na porta do degrau novo: todo
        // endereço que entra num `Alcance` passa pela escuta. Um ponto de
        // encontro IPv6 e uma escuta só-IPv4 dariam um candidato aonde ninguém
        // atende — a promessa vazia que este módulo existe para não fazer.
        let publico = SocketAddr::new(um_ipv6_global().into(), 41234);
        let alcance = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::SoIpv4),
            None,
            Some(publico),
            &[na_placa("192.168.0.30")],
            None,
        );
        assert_ne!(alcance.degrau(), Degrau::FuroDeNat);
        assert!(!alcance.alvos().contains(&publico));
    }

    #[test]
    fn a_ordem_do_convite_sai_do_tipo_do_candidato() {
        // A ordem deixa de ser um número solto que alguém pode editar sem saber o
        // que ele significa. Cada posição tem nome, e o nome responde a pergunta
        // que o número não respondia: **o que este candidato precisa para
        // funcionar**.
        assert!(Tipo::Local.ordem() < Tipo::PortaNoRoteador.ordem());
        assert!(Tipo::PortaNoRoteador.ordem() < Tipo::Refletido.ordem());
        // **O refletido antes do global**, e este par já esteve ao contrário.
        // Ver o doc de `ordem`: o global é o anfitrião afirmando a própria
        // alcançabilidade sem ter como conferi-la, e o refletido é um terceiro
        // tendo visto um pacote chegar. Quem entra paga quatro segundos por
        // palpite errado, em série — medido em campo: 8,4 s gastos em dois IPv6
        // que o roteador da casa recusa, antes do refletido, que respondeu em
        // 358 ms.
        assert!(Tipo::Refletido.ordem() < Tipo::Global.ordem());
        assert!(Tipo::Global.ordem() < Tipo::Tunel.ordem());
        assert!(Tipo::Tunel.ordem() < Tipo::Ponte.ordem());

        // Só o refletido depende de alguém ter furado o caminho. É esta linha que
        // impede o aviso por candidato da Tarefa 7 de queimar a janela de furos do
        // anfitrião com quem não precisa dela.
        assert!(Tipo::Refletido.precisa_de_furo());
        for tipo in [
            Tipo::Local,
            Tipo::Global,
            Tipo::PortaNoRoteador,
            Tipo::Tunel,
            Tipo::Ponte,
        ] {
            assert!(!tipo.precisa_de_furo());
        }

        // A porta do roteador é refletida por configuração, e não por observação:
        // ela existe porque alguém pediu, não porque alguém contou de onde o pacote
        // veio. Por isso variante própria, e por isso não precisa de furo.

        // Os três que não podem perder a vaga, e o que os separa dos outros três
        // não é serem melhores: é o **preço de perder**. Um `Global` ou um
        // `Tunel` que sai do convite é uma tentativa a menos; um `Local` que sai
        // desfaz o caso dos dois na mesma casa, uma `PortaNoRoteador` que sai faz
        // `Escada::subir` devolver ao roteador um mapeamento que funcionava, e um
        // `Refletido` que sai deixa o degrau 4 sem endereço nenhum.
        assert!(Tipo::Local.insubstituivel());
        assert!(Tipo::PortaNoRoteador.insubstituivel());
        assert!(Tipo::Refletido.insubstituivel());
        for tipo in [Tipo::Global, Tipo::Tunel, Tipo::Ponte] {
            assert!(!tipo.insubstituivel());
        }
        // E a reserva cabe no convite com folga, que é o que a impede de virar
        // um corte com outro nome.
        assert!([Tipo::Local, Tipo::PortaNoRoteador, Tipo::Refletido].len() < LIMITE_DE_CANDIDATOS);
    }

    #[test]
    fn o_endereco_furado_nunca_e_truncado_para_fora_do_convite() {
        // Ethernet e wifi ligadas, numa rede com IPv6 nativo: dois endereços de
        // ordem 0 e dois de ordem 1. São quatro, que é o `LIMITE_DE_CANDIDATOS`
        // inteiro — e o endereço furado, que é de ordem 3, cai fora da lista.
        //
        // É exatamente a máquina de casa com CGNAT, que é o único caso que o
        // degrau 4 existe para servir.
        let furado = SocketAddr::from(([200, 100, 30, 40], 61234));
        let alcance = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            Some(furado),
            &[
                na_placa("192.168.0.10"),
                na_placa("192.168.0.11"),
                na_placa("2804:388::1"),
                na_placa("2804:388::2"),
            ],
            None,
        );

        assert!(
            alcance.alvos().contains(&furado),
            "o endereço furado é insubstituível: sem ele o degrau 4 não alcança ninguém"
        );
    }

    #[test]
    fn a_porta_aberta_no_roteador_nunca_e_truncada_para_fora_do_convite() {
        // O truncamento do degrau 3 não encolhe um convite: ele **desliga um
        // caminho que funcionava**. `Escada::subir` guarda o mapeamento só
        // quando o degrau declarado é o 3, e devolve ao roteador o que não virou
        // degrau — então um endereço externo cortado da lista vira uma porta
        // fechada por decisão nossa, numa máquina que a internet alcançava.
        let externo = SocketAddr::from(([203, 0, 113, 7], 9000));

        // Quatro placas na rede de casa gastam o `LIMITE_DE_CANDIDATOS` inteiro
        // antes de o degrau 3 entrar na fila. Sem a reserva o convite sai só com
        // as quatro, o degrau cai para `SoRedeLocal`, e a porta é devolvida.
        let so_a_rede_de_casa = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::Dupla),
            Some(externo),
            None,
            &[
                na_placa("192.168.0.10"),
                na_placa("192.168.0.11"),
                na_placa("192.168.0.12"),
                na_placa("192.168.0.13"),
            ],
            None,
        );
        assert!(
            so_a_rede_de_casa.alvos().contains(&externo),
            "a porta do roteador foi truncada para fora do convite: {:?}",
            so_a_rede_de_casa.alvos()
        );
        assert_eq!(so_a_rede_de_casa.degrau(), Degrau::PortaNoRoteador);

        // O segundo caso, e o mais caro: dois IPv6 globais na mesma placa é
        // *privacy extensions*, que é o padrão e não uma raridade. Sem a reserva
        // o degrau **descia de 3 para 4** — a porta era devolvida ao roteador e a
        // máquina passava a depender de um ponto de encontro, com um terceiro
        // sabendo quem falou com quem, tendo um caminho direto que funcionava.
        let furado = SocketAddr::from(([200, 100, 30, 40], 61234));
        let com_o_furo = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::Dupla),
            Some(externo),
            Some(furado),
            &[
                na_placa("192.168.0.10"),
                na_placa("2804:388::1"),
                na_placa("2804:388::2"),
            ],
            None,
        );
        assert!(
            com_o_furo.alvos().contains(&externo),
            "o degrau 4 empurrou a porta do roteador para fora do convite: {:?}",
            com_o_furo.alvos()
        );
        assert_eq!(
            com_o_furo.degrau(),
            Degrau::PortaNoRoteador,
            "a escada desceu do degrau 3 para o 4 e pôs um terceiro num caminho \
             que já era direto: {:?}",
            com_o_furo.alvos()
        );
    }

    #[test]
    fn uma_vps_de_pilha_dupla_nao_promete_so_ipv6_tendo_ipv4_publico() {
        // A ordem dos dois degraus de "endereço próprio", e ela não é estética.
        // Uma VPS comum tem IPv4 público **e** IPv6 nativo. Com o ramo do
        // `EnderecoDireto` depois do `Ipv6Direto` a escada dizia degrau 2, cuja
        // frase é "só alcança quem também tiver IPv6" — embaixo de um link que
        // qualquer um com IPv4 alcança, que é quase todo mundo.
        //
        // É o mesmo argumento que o ADR 0022 usa para pôr o degrau 4 acima do 2
        // na frase, e é o defeito 3.2 com outro rótulo: prometer de menos.
        let alcance = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            None,
            &[na_placa("45.33.32.156"), na_placa("2804:388::1")],
            None,
        );

        assert_eq!(
            alcance.degrau(),
            Degrau::EnderecoDireto,
            "com IPv4 público na placa a escada leu o degrau do IPv6: {:?}",
            alcance.alvos()
        );
        // E os dois endereços continuam no convite: quem só tem IPv6 entra pelo
        // segundo, e o degrau declarado não é sobre jogar candidato fora.
        assert!(alcance
            .alvos()
            .iter()
            .any(|alvo| alvo.ip().to_string() == "45.33.32.156"));
        assert!(alcance
            .alvos()
            .iter()
            .any(|alvo| alvo.ip().to_string() == "2804:388::1"));
    }

    /// O bilhete não some porque a porta do roteador ganhou o nome do degrau.
    ///
    /// Enquanto o critério era `degrau() == FuroDeNat`, quem abriu porta perdia
    /// o furo — e perdia junto a única segunda chance de quem não atravessa
    /// aquela porta, que é o roteador sem *hairpin* de um lado e o firewall do
    /// outro. O degrau nomeia a frase; o convite carrega os endereços. São duas
    /// perguntas e estavam escritas como uma.
    #[test]
    fn o_furo_fica_de_pe_mesmo_quando_a_porta_do_roteador_nomeia_o_degrau() {
        let furado = SocketAddr::from(([200, 100, 30, 40], 61234));
        let externo = SocketAddr::from(([203, 0, 113, 7], 9000));
        let escuta = Escuta::nova(8383, Pilha::Dupla);
        let alcance = Alcance::decidir_sem_pcp(escuta, Some(externo), Some(furado), &[], None);

        // A porta ganha a frase — isto continua valendo.
        assert_eq!(alcance.degrau(), Degrau::PortaNoRoteador);
        // E o furo continua no convite, que é o que decide se ele fica de pé.
        assert!(alcance.alvos().contains(&furado));
        assert!(
            o_furo_vale_a_pena(&alcance, escuta, furado),
            "com o endereço furado no convite, largar o degrau 4 deixa o convite \
             apontando para um mapeamento de NAT que ninguém reaviva"
        );
    }

    #[test]
    fn um_degrau_nomeado_sempre_tem_o_endereco_dele_no_convite() {
        // A propriedade que os dois consertos desta tarefa produzem **juntos**, e
        // que nenhum dos dois cobra sozinho: se a escada diz `FuroDeNat`, o
        // endereço furado está no convite; se ela diz `PortaNoRoteador`, o
        // endereço do roteador está no convite.
        //
        // Um teste por mecanismo não bastava, e o motivo é que os dois se cobrem
        // um ao outro: com a reserva no lugar, `furado.is_some()` e
        // `alvos.contains(&furado)` passam a decidir igual em toda entrada, e a
        // leitura pelos alvos vira inobservável de fora. Um conserto que teste
        // nenhum consegue cobrar é um conserto que a próxima refatoração remove
        // sem nada ficar vermelho — que é o padrão que este ciclo existe para
        // acabar. Aqui a implicação é afirmada sobre a saída pública, e ela
        // reprova se **qualquer uma** das duas metades cair: tire a reserva e o
        // caso das quatro interfaces trunca o furado; troque o `contains` de
        // volta pelo `is_some` e o caso da escuta que recusa volta a nomear um
        // degrau que endereço nenhum sustenta.
        let furado = SocketAddr::from(([200, 100, 30, 40], 61234));
        let externo = SocketAddr::from(([203, 0, 113, 7], 9000));
        let furado_ipv6 = SocketAddr::new(um_ipv6_global().into(), 61234);
        let externo_ipv6 = SocketAddr::new(um_ipv6_global().into(), 9000);

        let uma_placa = [na_placa("192.168.0.10")];
        // Ethernet e wifi numa rede com IPv6 nativo: quatro endereços, que é o
        // `LIMITE_DE_CANDIDATOS` inteiro antes de qualquer degrau entrar.
        let quatro_placas = [
            na_placa("192.168.0.10"),
            na_placa("192.168.0.11"),
            na_placa("2804:388::1"),
            na_placa("2804:388::2"),
        ];
        let sem_rede: [interfaces::Achado; 0] = [];

        // Uma estrutura e não uma tupla de cinco: `(&str, Escuta, Option<_>,
        // Option<_>, &[_])` é o tipo que o `clippy::type_complexity` reclama, e
        // com razão — as duas `Option<SocketAddr>` seguidas são trocáveis à vista
        // desarmada, e trocá-las encenaria outra coisa em silêncio.
        struct Encenacao<'a> {
            conta: &'a str,
            escuta: Escuta,
            mapeada: Option<SocketAddr>,
            encontrado: Option<SocketAddr>,
            achados: &'a [interfaces::Achado],
        }

        let casos = &[
            Encenacao {
                conta: "uma placa e mais nada",
                escuta: Escuta::nova(8383, Pilha::Dupla),
                mapeada: None,
                encontrado: None,
                achados: &uma_placa,
            },
            Encenacao {
                conta: "uma placa e o furo",
                escuta: Escuta::nova(8383, Pilha::Dupla),
                mapeada: None,
                encontrado: Some(furado),
                achados: &uma_placa,
            },
            Encenacao {
                conta: "quatro interfaces e o furo: o limite inteiro gasto antes do degrau 4",
                escuta: Escuta::nova(8383, Pilha::Dupla),
                mapeada: None,
                encontrado: Some(furado),
                achados: &quatro_placas,
            },
            Encenacao {
                conta: "quatro interfaces, a porta do roteador e o furo",
                escuta: Escuta::nova(8383, Pilha::Dupla),
                mapeada: Some(externo),
                encontrado: Some(furado),
                achados: &quatro_placas,
            },
            Encenacao {
                conta: "uma placa e a porta do roteador",
                escuta: Escuta::nova(8383, Pilha::Dupla),
                mapeada: Some(externo),
                encontrado: None,
                achados: &uma_placa,
            },
            Encenacao {
                conta: "quatro interfaces e a porta do roteador: o limite inteiro gasto antes do degrau 3",
                escuta: Escuta::nova(8383, Pilha::Dupla),
                mapeada: Some(externo),
                encontrado: None,
                achados: &quatro_placas,
            },
            Encenacao {
                conta: "uma escuta só-IPv4 e um furo IPv6, que ela não serve",
                escuta: Escuta::nova(8383, Pilha::SoIpv4),
                mapeada: None,
                encontrado: Some(furado_ipv6),
                achados: &uma_placa,
            },
            Encenacao {
                conta: "uma escuta só-IPv4 e uma porta externa IPv6, que ela não serve",
                escuta: Escuta::nova(8383, Pilha::SoIpv4),
                mapeada: Some(externo_ipv6),
                encontrado: None,
                achados: &uma_placa,
            },
            Encenacao {
                conta: "sem rede nenhuma, só o furo",
                escuta: Escuta::nova(8383, Pilha::Dupla),
                mapeada: None,
                encontrado: Some(furado),
                achados: &sem_rede,
            },
        ];

        let mut disse_furo = false;
        let mut disse_porta = false;
        for caso in casos {
            let Encenacao {
                conta: encenacao,
                escuta,
                mapeada,
                encontrado,
                achados,
            } = caso;
            let alcance = Alcance::decidir_sem_pcp(*escuta, *mapeada, *encontrado, achados, None);

            // A metade da reserva, afirmada na mesma travessia: um endereço que
            // veio de fora desta máquina e que a escuta serve **está** no
            // convite, sempre. Sem esta asserção a implicação abaixo é
            // verdadeira de graça no caso das quatro interfaces — o endereço sai
            // da lista, o degrau desce, e a premissa some junto com o problema.
            //
            // Os dois, e não só o furado: perder a vaga custa caro dos dois
            // lados. Sem o furado o degrau 4 não alcança ninguém; sem a porta do
            // roteador o degrau 3 deixa de ser declarado, e `Escada::subir`
            // devolve ao roteador um mapeamento que estava funcionando.
            for (nome, endereco) in [
                ("o endereço furado", *encontrado),
                ("a porta do roteador", *mapeada),
            ] {
                let Some(endereco) = endereco else {
                    continue;
                };
                if escuta.serve(endereco.ip()) {
                    assert!(
                        alcance.alvos().contains(&endereco),
                        "{encenacao}: a escuta serve {nome} e ele não está no convite, \
                         então o degrau que ele produz não alcança ninguém: {:?}",
                        alcance.alvos()
                    );
                }
            }

            match alcance.degrau() {
                Degrau::FuroDeNat => {
                    disse_furo = true;
                    let Some(furado) = *encontrado else {
                        panic!("disse degrau 4 sem endereço furado nenhum ({encenacao})");
                    };
                    assert!(
                        alcance.alvos().contains(&furado),
                        "{encenacao}: disse `FuroDeNat` e o endereço furado não está no \
                         convite, então o degrau nomeia um caminho que candidato nenhum \
                         tenta: {:?}",
                        alcance.alvos()
                    );
                }
                Degrau::PortaNoRoteador => {
                    disse_porta = true;
                    let Some(mapeada) = *mapeada else {
                        panic!("disse degrau 3 sem porta aberta nenhuma ({encenacao})");
                    };
                    assert!(
                        alcance.alvos().contains(&mapeada),
                        "{encenacao}: disse `PortaNoRoteador` e a porta do roteador não \
                         está no convite: {:?}",
                        alcance.alvos()
                    );
                }
                // Os outros degraus não são sobre um endereço que veio de fora
                // desta máquina, e a implicação não os alcança.
                Degrau::Ipv6Direto
                | Degrau::EnderecoDireto
                | Degrau::RedeLocalOuVpn
                | Degrau::SoRedeLocal => {}
            }
        }

        // Uma implicação nunca é falsa quando a premissa nunca acontece. Sem
        // estas duas linhas, um dia em que `decidir` parasse de declarar os dois
        // degraus deixaria este teste verde sem cobrar coisa nenhuma.
        assert!(disse_furo, "nenhuma encenação chegou ao degrau 4");
        assert!(disse_porta, "nenhuma encenação chegou ao degrau 3");
    }

    #[test]
    fn quem_ja_tem_endereco_publico_nao_pede_apresentacao_a_ninguem() {
        // O guarda que evita pagar metadado à toa: numa VPS o degrau 1 já
        // resolveu tudo, e um ponto de encontro só aprenderia quem está falando
        // com quem sem melhorar nada para ninguém.
        assert!(tem_ipv4_global(&[na_placa("45.33.32.156")]));
        assert!(!tem_ipv4_global(&[na_placa("192.168.0.30")]));
        // CGNAT é o caso que **precisa** do degrau 4, e não pode ser confundido
        // com endereço público — é literalmente a rede do relato de campo.
        assert!(!tem_ipv4_global(&[na_placa("100.64.0.7")]));
        // E um endereço de túnel não conta, mesmo se for público: uma VPN de
        // navegação não aceita entrada nenhuma.
        assert!(!tem_ipv4_global(&[no_tunel("45.33.32.156")]));
    }

    #[test]
    fn um_endereco_publico_nao_e_um_link_que_so_funciona_na_sua_rede() {
        // Uma VPS: IPv4 global na placa, sem UPnP (não há roteador a pedir), sem
        // IPv6, sem túnel. O degrau 4 não é tentado de propósito — `subir` só
        // pergunta ao ponto de encontro quando não há IPv4 público, e numa VPS
        // perguntar seria pagar metadado por um caminho que já existe (ADR 0022).
        //
        // Antes deste conserto a escada caía no `else` final e declarava
        // `SoRedeLocal`, cuja frase manda encaminhar a porta 8383 num roteador que
        // não existe. É o defeito do relato do Cloudflare WARP com o sinal
        // invertido: lá a frase prometia demais, aqui promete de menos.
        let alcance = Alcance::decidir_sem_pcp(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            None,
            &[na_placa("45.33.32.156")],
            None,
        );

        assert_eq!(alcance.degrau(), Degrau::EnderecoDireto);
        assert!(alcance.degrau().alcanca_de_fora());
        assert!(
            alcance
                .alvos()
                .iter()
                .any(|alvo| alvo.ip().to_string() == "45.33.32.156"),
            "o endereço que dá nome ao degrau tem de estar no convite"
        );
    }

    #[test]
    fn cada_degrau_tem_nome_proprio_e_estavel() {
        // O nome atravessa até o JavaScript, onde a frase está escrita. Dois
        // degraus com o mesmo nome seriam duas situações com uma frase só.
        let nomes = [
            Degrau::PortaNoRoteador.nome(),
            Degrau::FuroDeNat.nome(),
            Degrau::Ipv6Direto.nome(),
            Degrau::EnderecoDireto.nome(),
            Degrau::RedeLocalOuVpn.nome(),
            Degrau::SoRedeLocal.nome(),
        ];
        for (indice, nome) in nomes.iter().enumerate() {
            assert!(!nome.is_empty(), "degrau sem nome");
            for outro in nomes.iter().skip(indice + 1) {
                assert_ne!(nome, outro, "dois degraus com o mesmo nome");
            }
        }
        assert!(Degrau::PortaNoRoteador.alcanca_de_fora());
        assert!(Degrau::Ipv6Direto.alcanca_de_fora());
        assert!(Degrau::EnderecoDireto.alcanca_de_fora());
        assert!(
            !Degrau::SoRedeLocal.alcanca_de_fora(),
            "o degrau 1 prometeu alcançar de fora, e é justamente o que ele não faz"
        );
    }

    #[test]
    fn a_pilha_dupla_recebe_um_pacote_ipv4_de_verdade() {
        // Ler `IPV6_V6ONLY` de volta diz o que o sistema **respondeu**; só um
        // pacote diz o que ele **faz**. É exatamente nessa diferença que mora o
        // defeito que passa numa máquina e falha noutra, e é o defeito que este
        // módulo existe para não ter. Não precisa de rede: 127.0.0.1 basta.
        //
        // Onde este teste tem dente: no Windows e nos BSD, onde o padrão do
        // `IPV6_V6ONLY` é *ligado*. Tirar o `set_only_v6(false)` de
        // `pilha_dupla` faz este teste reprovar lá e **não** no Linux nem no
        // macOS, onde o padrão já é pilha dupla e a chamada não muda nada
        // observável. Foi medido: a mutação sobrevive numa máquina macOS. Vale
        // saber ao ler um verde daqui — ele prova que a pilha dupla funciona
        // nesta máquina, não que a linha que a garante nas outras continua lá.
        let Ok((socket, pilha)) = abrir_escuta(qualquer_porta_v6()) else {
            eprintln!("pulado: esta máquina não liga em nenhuma porta UDP");
            return;
        };
        if pilha != Pilha::Dupla {
            eprintln!("pulado: esta máquina não faz pilha dupla, e caiu para {pilha:?}");
            return;
        }
        let porta = socket
            .local_addr()
            .expect("o socket não diz onde ligou")
            .port();

        socket
            .set_nonblocking(false)
            .expect("não deu para voltar a bloquear");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .expect("não deu para pôr prazo de leitura");

        let remetente = UdpSocket::bind("127.0.0.1:0").expect("não deu para abrir um socket IPv4");
        remetente
            .send_to(b"padrao laranja", ("127.0.0.1", porta))
            .expect("não deu para enviar por IPv4");

        let mut balde = [0_u8; 32];
        let (lidos, de) = socket
            .recv_from(&mut balde)
            .expect("a escuta de pilha dupla não recebeu o pacote IPv4");
        assert_eq!(
            balde.get(..lidos),
            Some(&b"padrao laranja"[..]),
            "chegou outra coisa"
        );
        // O remetente aparece como IPv4 mapeado em IPv6 — é assim que um socket
        // de pilha dupla mostra quem veio pela outra família.
        assert!(de.is_ipv6(), "um socket IPv6 relatou uma origem não-IPv6");
    }

    #[test]
    fn um_endereco_de_saida_v6_e_global_ou_nao_existe() {
        // Meio-termo é o que não pode haver: um `fe80::` devolvido daqui vira
        // um `seele://` que parece certo, é aceito pelo analisador e não
        // conecta de lugar nenhum.
        let Some(endereco) = endereco_de_saida_v6() else {
            eprintln!("pulado: esta máquina não tem rota IPv6 global");
            return;
        };
        assert!(
            global_v6(endereco),
            "devolveu um endereço que não sai daqui: {endereco}"
        );
        assert!(!endereco.is_loopback(), "devolveu o loopback: {endereco}");
        // E tem de atravessar o `seele://` inteiro, com colchetes.
        let alvo = format!("[{endereco}]:8383");
        let convite = seele_proto::uri::Convite::novo(&alvo).to_string();
        let lido = seele_proto::uri::analisar(&convite).expect("o convite não se lê de volta");
        let separado = lido.endereco().expect("o alvo não se separa");
        assert_eq!(separado.maquina, endereco.to_string());
        assert_eq!(separado.porta, 8383);
    }

    #[test]
    fn os_enderecos_que_nao_saem_da_rede_sao_recusados() {
        // Não precisa de rede: são as faixas, e elas não mudam.
        use std::net::Ipv6Addr;
        for (texto, esperado) in [
            ("2001:db8::1", true),
            ("2606:4700:4700::1111", true),
            ("::1", false),
            ("::", false),
            ("fe80::1", false),
            ("fe80::dead:beef", false),
            ("fc00::1", false),
            ("fd12:3456::1", false),
            ("ff02::1", false),
        ] {
            let endereco: Ipv6Addr = texto.parse().expect("endereço de teste inválido");
            assert_eq!(global_v6(endereco), esperado, "errou sobre {texto}");
        }
    }

    #[test]
    fn uma_porta_ocupada_falha_dizendo_onde() {
        // A mensagem é o produto: "não deu para ligar" sem o endereço manda o
        // operador adivinhar qual das duas escutas colidiu.
        let Ok((primeiro, _)) = abrir_escuta(qualquer_porta_v6()) else {
            eprintln!("pulado: esta máquina não liga em nenhuma porta UDP");
            return;
        };
        let porta = primeiro
            .local_addr()
            .expect("o socket não diz onde ligou")
            .port();

        let erro = abrir_escuta(SocketAddr::from((Ipv6Addr::UNSPECIFIED, porta)))
            .err()
            .map(|erro| format!("{erro:#}"));
        let Some(erro) = erro else {
            eprintln!("pulado: esta máquina deixa duas escutas na mesma porta UDP {porta}");
            return;
        };
        assert!(
            erro.contains(&porta.to_string()),
            "o erro não diz qual porta colidiu: {erro}"
        );
    }

    /// O IPv6 global que o roteador **disse** ter liberado, no formato em que
    /// `decidir` o recebe: o endereço da placa mais a porta da escuta.
    fn liberado(ip: &str, porta: u16) -> SocketAddr {
        SocketAddr::new(ip.parse().unwrap_or(IpAddr::from([0, 0, 0, 0])), porta)
    }

    #[test]
    fn o_ok_do_pcp_nao_muda_o_degrau_nem_acrescenta_candidato() {
        // Duas propriedades, e a primeira é a que importa mais: **o `Ok` do PCP
        // não nomeia degrau**. Ele não é prova de que alguém de fora chega — quem
        // respondeu pode não ser quem filtra —, e um `Degrau::Ipv6Liberado`
        // acrescentado por engano faria a tela dizer "alcança" sobre uma
        // afirmação do roteador. É esta asserção que fica vermelha nesse dia.
        //
        // A segunda é que o degrau 2 não descobre endereço nenhum: ele muda o
        // que se sabe sobre um endereço que já estava na lista. Vale dizer que
        // ela é fraca hoje, no espírito do comentário que já existe sobre os
        // `contains` de `decidir`: a deduplicação de `alvos` a faria passar
        // mesmo se a promoção empurrasse um candidato a mais. Fica como rede
        // para o dia em que aquela deduplicação mudar.
        let achados = [na_placa("192.168.0.30"), na_placa("2001:db8::1")];
        let alcance = Alcance::decidir(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            Some(liberado("2001:db8::1", 8383)),
            None,
            &achados,
            None,
            None,
        );
        let seis = liberado("2001:db8::1", 8383);
        assert_eq!(
            alcance.alvos().iter().filter(|alvo| **alvo == seis).count(),
            1,
            "o IPv6 liberado saiu repetido: {:?}",
            alcance.alvos()
        );
        // E o degrau **não** muda de nome por causa do PCP: um `Ok` do roteador
        // não é prova de que alguém de fora chega. Ver `Tipo::GlobalLiberado`.
        assert_eq!(alcance.degrau(), Degrau::Ipv6Direto);
    }

    #[test]
    fn o_ok_do_pcp_nao_passa_na_frente_do_endereco_refletido() {
        // A regra que a medição de campo produziu — observação vence afirmação —
        // continua valendo. Um `SUCCESS` do PCP é afirmação melhor fundamentada
        // e não é observação: quem respondeu pode não ser quem filtra. Pôr o
        // IPv6 na frente do refletido desfaria os 9,6 s medidos em troca de um
        // palpite melhor.
        let achados = [na_placa("192.168.0.30"), na_placa("2001:db8::1")];
        let furado = SocketAddr::from(([200, 160, 2, 3], 41000));
        let alcance = Alcance::decidir(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            Some(liberado("2001:db8::1", 8383)),
            Some(furado),
            &achados,
            None,
            None,
        );
        let alvos = alcance.alvos();
        let onde = |alvo: SocketAddr| {
            alvos
                .iter()
                .position(|candidato| *candidato == alvo)
                .unwrap_or(usize::MAX)
        };
        assert!(
            onde(furado) < onde(liberado("2001:db8::1", 8383)),
            "o palpite do PCP passou na frente da observação: {alvos:?}"
        );
    }

    #[test]
    fn entre_dois_ipv6_globais_o_liberado_e_o_que_sobrevive_ao_corte() {
        // Onde a promoção paga de verdade. Uma máquina com IPv6 permanente e
        // temporários (RFC 8981) tem vários globais, o convite cabe quatro, e
        // sem o PCP quem decide é a ordem em que o sistema listou as interfaces.
        // Com ele, quem decide é qual deles o roteador disse ter aberto.
        let achados = [
            na_placa("192.168.0.30"),
            na_placa("192.168.0.31"),
            na_placa("2001:db8::1"),
            na_placa("2001:db8::2"),
        ];
        let alcance = Alcance::decidir(
            Escuta::nova(8383, Pilha::Dupla),
            None,
            Some(liberado("2001:db8::2", 8383)),
            None,
            &achados,
            None,
            None,
        );
        assert!(
            alcance.alvos().contains(&liberado("2001:db8::2", 8383)),
            "o IPv6 liberado ficou de fora do convite: {:?}",
            alcance.alvos()
        );
        // E ele vem antes do outro global, que é o efeito inteiro da promoção.
        let alvos = alcance.alvos();
        let onde = |alvo: SocketAddr| alvos.iter().position(|c| *c == alvo);
        if let (Some(cru), Some(promovido)) = (
            onde(liberado("2001:db8::1", 8383)),
            onde(liberado("2001:db8::2", 8383)),
        ) {
            assert!(promovido < cru, "a promoção não mudou nada: {alvos:?}");
        }
    }

    #[test]
    fn a_recusa_do_pcp_sobra_quando_o_firewall_nao_abriu_e_some_quando_abriu() {
        // Mesma disciplina de `porta_recusada`: a recusa é guardada em relação
        // ao que ela explica. Enquanto o firewall não abriu, a frase é o que diz
        // a quem hospeda por que um amigo com IPv6 nativo ainda pode não entrar
        // pelo endereço IPv6 que está no convite.
        let achados = [na_placa("192.168.0.30"), na_placa("2001:db8::1")];
        let escuta = Escuta::nova(8383, Pilha::Dupla);
        let motivo = "o roteador (fe80::1) não respondeu".to_owned();

        let recusado = Alcance::decidir(
            escuta,
            None,
            None,
            None,
            &achados,
            None,
            Some(motivo.clone()),
        );
        assert_eq!(recusado.pcp_recusada(), Some(motivo.as_str()));

        let aberto = Alcance::decidir(
            escuta,
            None,
            Some(liberado("2001:db8::1", 8383)),
            None,
            &achados,
            None,
            Some(motivo),
        );
        assert_eq!(aberto.pcp_recusada(), None);
    }

    #[test]
    fn um_firewall_aberto_que_a_escuta_nao_serve_nao_vira_promocao_nem_apaga_a_recusa() {
        // O caso simétrico ao da porta que a escuta não atende. Numa máquina em
        // que a pilha dupla falhou, o `Ok` do roteador é sobre um endereço em
        // que ninguém está escutando — e ele não pode nem entrar no convite nem
        // fazer a frase de recusa sumir, ou quem hospeda leria "abriu" sobre um
        // caminho que não existe.
        let achados = [na_placa("192.168.0.30"), na_placa("2001:db8::1")];
        let motivo = "esta escuta não atende em IPv6".to_owned();
        let alcance = Alcance::decidir(
            Escuta::nova(8383, Pilha::SoIpv4),
            None,
            Some(liberado("2001:db8::1", 8383)),
            None,
            &achados,
            None,
            Some(motivo.clone()),
        );
        assert!(
            !alcance
                .alvos()
                .iter()
                .any(|alvo| alvo.ip() == liberado("2001:db8::1", 8383).ip()),
            "um IPv6 que a escuta não serve entrou no convite: {:?}",
            alcance.alvos()
        );
        assert_eq!(alcance.pcp_recusada(), Some(motivo.as_str()));
    }

    #[test]
    fn a_ordem_dos_tipos_nao_tem_empate_nem_buraco() {
        // A ordem é a lista de tentativa, e um empate entre dois tipos faria o
        // `sort_by_key` decidir por acidente de ordem de inserção. `GlobalLiberado`
        // entrou no meio dela e empurrou o resto; isto é o que impede a próxima
        // inserção de esquecer um.
        let todos = [
            Tipo::Local,
            Tipo::PortaNoRoteador,
            Tipo::Refletido,
            Tipo::GlobalLiberado,
            Tipo::Global,
            Tipo::Tunel,
            Tipo::Ponte,
        ];
        let mut ordens: Vec<u8> = todos.iter().map(|tipo| tipo.ordem()).collect();
        ordens.sort_unstable();
        let esperado: Vec<u8> = (0..u8::try_from(todos.len()).unwrap_or(u8::MAX)).collect();
        assert_eq!(ordens, esperado, "a ordem dos tipos tem empate ou buraco");
        // E o liberado continua abaixo do refletido, que é a decisão que a
        // medição de campo produziu e que este ciclo não desfaz.
        assert!(Tipo::Refletido.ordem() < Tipo::GlobalLiberado.ordem());
        assert!(Tipo::GlobalLiberado.ordem() < Tipo::Global.ordem());
    }

    #[test]
    fn a_promocao_do_pcp_nao_pede_furo_a_ninguem() {
        // `precisa_de_furo` decide se o ponto de encontro é avisado antes de um
        // candidato. Um IPv6 que o roteador liberou não depende de furo nenhum,
        // e pedir um gastaria metadado e orçamento de furo por nada.
        assert!(!Tipo::GlobalLiberado.precisa_de_furo());
    }
}
