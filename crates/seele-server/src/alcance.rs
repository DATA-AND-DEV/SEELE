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
//! | 4 | Furo de NAT com ponto de encontro | fora deste módulo, e fora da decisão |
//! | 5 | Retransmissão | fora de escopo por decisão |
//!
//! # Por que os dois degraus daqui não custam nada ao modelo
//!
//! Nenhum dos dois põe terceiro no caminho. O degrau 2 é um endereço que já é
//! roteável e uma regra de firewall; o degrau 3 é um pedido ao **próprio**
//! roteador do anfitrião. Ninguém mais aprende que a conversa existe, que é a
//! diferença entre estes dois degraus e o quarto.

use std::net::{SocketAddr, UdpSocket};

use anyhow::{Context, Result};

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
    /// **Degrau 2.** Sem porta no roteador, mas a máquina tem IPv6 global.
    /// Alcança quem também tem IPv6 — se o firewall do roteador deixar entrar,
    /// e isso não dá para saber daqui.
    Ipv6Direto,
    /// **Degrau 1.** Só quem estiver na mesma rede. É o que sempre existiu, e
    /// continua sendo a resposta honesta quando os dois de cima não deram.
    SoRedeLocal,
}

impl Degrau {
    /// O nome estável que atravessa para a casca.
    #[must_use]
    pub fn nome(self) -> &'static str {
        match self {
            Self::PortaNoRoteador => "PortaNoRoteador",
            Self::Ipv6Direto => "Ipv6Direto",
            Self::SoRedeLocal => "SoRedeLocal",
        }
    }

    /// Se alguém de fora da rede local tem chance de chegar.
    ///
    /// "Chance", e não "certeza", e a diferença é honesta: no degrau 2 o
    /// firewall do roteador ainda pode recusar a entrada, e ninguém daqui sabe
    /// disso sem alguém do outro lado tentando.
    #[must_use]
    pub fn alcanca_de_fora(self) -> bool {
        matches!(self, Self::PortaNoRoteador | Self::Ipv6Direto)
    }
}

/// Até onde este Dogma é alcançável, e por quê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alcance {
    /// O endereço a pôr no `seele://`.
    pub alvo: SocketAddr,
    /// Qual degrau o produziu.
    pub degrau: Degrau,
    /// Por que o degrau 3 não deu — quando não deu.
    ///
    /// Guardado mesmo quando o degrau 2 salvou o dia: "funcionou por IPv6, e a
    /// porta não abriu porque o roteador está atrás de CGNAT" são duas
    /// informações, e a segunda é a que explica por que um amigo só-IPv4 não
    /// entra.
    pub porta_recusada: Option<String>,
}

/// A escada do ADR 0022, subida uma vez, com o que ela abriu preso junto.
///
/// Existe para que o mapeamento de porta tenha dono: uma porta pedida ao
/// roteador precisa ser devolvida quando o Dogma fecha, e quem a devolve é
/// quem a segura.
pub struct Escada {
    alcance: Alcance,
    porta: Option<porta::PortaAberta>,
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
    /// A ordem é a do ADR 0022, e a escolha de qual endereço vai no convite
    /// segue dela. O degrau 3 vem primeiro **quando dá**, porque um IPv4
    /// público alcança praticamente todo cliente; o degrau 2 vem depois, porque
    /// um IPv6 global alcança de qualquer lugar mas só quem também tem IPv6.
    ///
    /// `local` é a porta em que o Dogma está atendendo.
    pub async fn subir(local: u16) -> Self {
        let v4 = endereco_de_saida_v4();

        // Degrau 3. Só faz sentido pedir se sabemos para onde o roteador deve
        // encaminhar, e é o endereço IPv4 desta máquina na rede.
        let (mapeada, recusa) = match v4 {
            Some(ip) => match porta::abrir(SocketAddr::new(ip, local)).await {
                Ok(aberta) => (Some(aberta), None),
                Err(falha) => {
                    tracing::info!(%falha, "o degrau 3 não deu");
                    (None, Some(falha.to_string()))
                }
            },
            None => (
                None,
                Some("esta máquina não está numa rede IPv4".to_owned()),
            ),
        };

        if let Some(aberta) = mapeada {
            let alcance = Alcance {
                alvo: aberta.externo(),
                degrau: Degrau::PortaNoRoteador,
                porta_recusada: None,
            };
            return Self {
                alcance,
                porta: Some(aberta),
            };
        }

        // Degrau 2.
        if let Some(seis) = endereco_de_saida_v6() {
            return Self {
                alcance: Alcance {
                    alvo: SocketAddr::new(seis.into(), local),
                    degrau: Degrau::Ipv6Direto,
                    porta_recusada: recusa,
                },
                porta: None,
            };
        }

        // Degrau 1. A resposta honesta, e a que sempre funcionou.
        let alvo = v4.map_or_else(
            || SocketAddr::from(([127, 0, 0, 1], local)),
            |ip| SocketAddr::new(ip, local),
        );
        Self {
            alcance: Alcance {
                alvo,
                degrau: Degrau::SoRedeLocal,
                porta_recusada: recusa,
            },
            porta: None,
        }
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
        if let Some(porta) = self.porta {
            porta.fechar().await;
        }
    }
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
        let escada = Escada::subir(8383).await;
        let alcance = escada.alcance().clone();
        eprintln!("esta rede parou em {:?}", alcance.degrau);

        assert_eq!(alcance.alvo.port(), 8383, "a escada trocou a porta");
        assert!(
            !alcance.alvo.ip().is_unspecified(),
            "convidou para o curinga, que não é lugar nenhum: {}",
            alcance.alvo
        );

        match alcance.degrau {
            Degrau::PortaNoRoteador => {
                assert!(alcance.porta_recusada.is_none(), "abriu e reclamou junto");
                assert!(alcance.degrau.alcanca_de_fora());
            }
            // O ponto todo do ADR 0022: quando o degrau 3 não deu, a razão não
            // pode sumir. É ela que explica a quem hospeda por que um amigo não
            // entra, e sem ela sobra "não conecta".
            Degrau::Ipv6Direto | Degrau::SoRedeLocal => {
                let motivo = alcance
                    .porta_recusada
                    .as_deref()
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
        let escada = Escada::subir(8383).await;
        let alvo = escada.alcance().alvo;
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

    #[test]
    fn cada_degrau_tem_nome_proprio_e_estavel() {
        // O nome atravessa até o JavaScript, onde a frase está escrita. Dois
        // degraus com o mesmo nome seriam duas situações com uma frase só.
        let nomes = [
            Degrau::PortaNoRoteador.nome(),
            Degrau::Ipv6Direto.nome(),
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
