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
pub mod interfaces;
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
            Self::PortaNoRoteador | Self::FuroDeNat | Self::Ipv6Direto
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
    /// `mapeada` é o que o degrau 3 abriu e `achados` são os endereços que as
    /// interfaces desta máquina têm. Cada um ainda pode ser recusado aqui, se a
    /// escuta não o servir.
    fn decidir(
        escuta: Escuta,
        mapeada: Option<SocketAddr>,
        encontrado: Option<SocketAddr>,
        achados: &[interfaces::Achado],
        porta_recusada: Option<String>,
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
        let mut candidatos: Vec<(u8, SocketAddr)> = Vec::new();
        for achado in achados {
            let Some(alvo) = escuta.anunciar(achado.ip) else {
                continue;
            };
            let ordem = match (achado.classe(), achado.e_da_rede_local()) {
                (interfaces::Origem::Fisica, true) => 0,
                (interfaces::Origem::Fisica, false) => 1,
                (interfaces::Origem::Tunel, _) => 4,
                (interfaces::Origem::Virtual, _) => 5,
            };
            candidatos.push((ordem, alvo));
        }
        let externo = mapeada.and_then(|alvo| escuta.anunciar_com_porta(alvo));
        if let Some(externo) = externo {
            candidatos.push((2, externo));
        }
        // O endereço que o ponto de encontro viu. Depois da porta do roteador —
        // aquele funciona sem ninguém bater em ponto nenhum — e antes do túnel,
        // que só serve a quem estiver na mesma VPN.
        let furado = encontrado.and_then(|alvo| escuta.anunciar_com_porta(alvo));
        if let Some(furado) = furado {
            candidatos.push((3, furado));
        }

        // Estável: dentro de uma classe vale a ordem em que a máquina listou as
        // interfaces, que é a ordem em que o próprio sistema as prefere.
        candidatos.sort_by_key(|(ordem, _)| *ordem);
        let mut alvos: Vec<SocketAddr> = Vec::new();
        for (_, alvo) in candidatos {
            if !alvos.contains(&alvo) {
                alvos.push(alvo);
            }
        }
        alvos.truncate(LIMITE_DE_CANDIDATOS);

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

        // O degrau é lido dos candidatos que sobraram, e não do caminho que os
        // produziu: assim não há como dizer que se alcança por um endereço que
        // não está no convite.
        let degrau = if externo.is_some() {
            Degrau::PortaNoRoteador
        } else if furado.is_some() {
            Degrau::FuroDeNat
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

        Self {
            alvos,
            degrau,
            encontro_recusado: None,
            porta_recusada: if degrau == Degrau::PortaNoRoteador {
                None
            } else {
                porta_recusada
            },
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
}

/// A escada do ADR 0022, subida uma vez, com o que ela abriu preso junto.
///
/// Existe para que o mapeamento de porta tenha dono: uma porta pedida ao
/// roteador precisa ser devolvida quando o Dogma fecha, e quem a devolve é
/// quem a segura.
pub struct Escada {
    alcance: Alcance,
    porta: Option<porta::PortaAberta>,
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

        // Degrau 3. Duas condições, e a segunda é nova: além de saber para onde
        // o roteador deve encaminhar, a escuta tem de atender em IPv4 — um
        // mapeamento para uma máquina que só atende IPv6 abriria uma porta que
        // não leva a lugar nenhum, com o mesmo sucesso mentiroso do CGNAT.
        let (mapeada, recusa) = if escuta.pilha().alcanca_ipv4() {
            match porta::abrir(&achados, escuta.porta()).await {
                Ok(aberta) => (Some(aberta), None),
                Err(falha) => {
                    tracing::info!(%falha, "o degrau 3 não deu");
                    (None, Some(falha.to_string()))
                }
            }
        } else {
            (
                None,
                Some("esta escuta não atende em IPv4, então não há porta IPv4 a pedir".to_owned()),
            )
        };

        // Degrau 4, e só se os de cima não resolveram. Duas condições, e as
        // duas são sobre não pagar o custo à toa: com a porta aberta no roteador
        // não há o que um ponto de encontro acrescente, e com um endereço IPv4
        // global esta máquina já é alcançável sem apresentação nenhuma — é o
        // caso de uma VPS, que é o degrau 1.
        let precisa = mapeada.is_none() && !tem_ipv4_global(&achados);
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
            encontro.as_ref().map(encontro::Encontro::publico),
            &achados,
            recusa,
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
        // O mesmo cuidado do degrau 3, pelo mesmo motivo: um encontro que não
        // virou candidato é um encontro que fica reavivando um caminho que
        // ninguém vai usar.
        let encontro = match encontro {
            Some(aberto) if alcance.degrau() == Degrau::FuroDeNat => Some(aberto),
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
    }
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
        Self::decidir(escuta, mapeada, None, achados, porta_recusada)
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
        assert!(
            posicao(SocketAddr::new(um_ipv6_global().into(), 8383)) < posicao(externo),
            "a porta do roteador veio antes do IPv6, e ela só volta de fora: {:?}",
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
        let alcance = Alcance::decidir(
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
        let alcance = Alcance::decidir(
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
        let alcance = Alcance::decidir(
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
    fn cada_degrau_tem_nome_proprio_e_estavel() {
        // O nome atravessa até o JavaScript, onde a frase está escrita. Dois
        // degraus com o mesmo nome seriam duas situações com uma frase só.
        let nomes = [
            Degrau::PortaNoRoteador.nome(),
            Degrau::FuroDeNat.nome(),
            Degrau::Ipv6Direto.nome(),
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
}
