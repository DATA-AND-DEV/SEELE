//! Os endereços desta máquina, perguntados às **interfaces**.
//!
//! # Por que não dá para perguntar à rota padrão
//!
//! O jeito antigo — e ele ainda existe, em [`super::endereco_de_saida_v4`] —
//! era abrir um socket UDP, `connect` num endereço de documentação e ler o
//! `local_addr`. Isso responde a uma pergunta só: **qual endereço meu o sistema
//! usaria para sair daqui**. É o endereço da rota padrão.
//!
//! Uma VPN captura a rota padrão. Foi o que aconteceu num Windows hospedando
//! com o Cloudflare WARP ligado:
//!
//! ```text
//! Name : CloudflareWARP    IPv4Connectivity: Internet   IPv6Connectivity: Internet
//! Name : Rede (Ethernet)   IPv4Connectivity: Internet   IPv6Connectivity: NoTraffic
//! ```
//!
//! O truque do socket devolvia o endereço do túnel, e o convite saía com ele.
//! Um Mac na mesma sala não alcançava aquele endereço — ele não é de nenhuma
//! rede que o Mac veja — e o degrau 3 pedia ao roteador que encaminhasse a
//! porta para um endereço que não existe na LAN. O caso que sempre funcionou,
//! "estamos na mesma rede", tinha sido perdido pelo produto sem que ninguém
//! mexesse nele.
//!
//! Enumerar interfaces devolve **todos** os endereços, inclusive o da Ethernet
//! que a rota padrão deixou de fora, e é a única forma de achar o endereço da
//! rede local numa máquina com VPN.
//!
//! # O que custa, e por que se paga
//!
//! Um crate: `if-addrs`, cuja única dependência é a `libc` (o `windows-sys` que
//! ele usa no Windows já vem com o resto da árvore). É a mesma conta que o
//! `alcance::porta` fez ao recusar o `portmapper`, com o resultado invertido:
//! lá eram 31 crates para cobrir roteadores que falam PCP e não falam UPnP;
//! aqui é **um** crate para não mentir o endereço numa máquina com VPN, que é
//! um caso que já mordeu em campo. A std não enumera interfaces, e fazer isso à
//! mão seria `getifaddrs` e `GetAdaptersAddresses` — FFI, e portanto `unsafe`,
//! que o workspace proíbe.
//!
//! # A heurística de túnel, e o que ela pode e não pode fazer
//!
//! Não existe pergunta ao sistema que responda "este endereço aceita conexão de
//! fora". O que dá para saber é se a interface é ponto-a-ponto e como ela se
//! chama, e as duas coisas juntas acertam nos casos conhecidos: `utun*` no
//! macOS, `wg*` e `tun*` no Linux, `CloudflareWARP` e `Tailscale` no Windows.
//!
//! Por isso a heurística **nunca descarta** um endereço: ela só decide a ordem
//! em que os candidatos são oferecidos e permite que a frase mostrada a quem
//! hospeda diga "isto veio de uma VPN". Um falso positivo custa alguns
//! segundos de espera num candidato mal colocado; descartar por engano custaria
//! a única forma de entrar.
use std::net::IpAddr;

/// De que tipo de interface um endereço veio.
///
/// A ordem da declaração **é** a ordem em que os candidatos são oferecidos, e
/// é por isso que este enum é `Ord`. Ver [`Achado::classe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origem {
    /// Uma placa de rede de verdade: Ethernet, wifi.
    Fisica,
    /// Um túnel: VPN, WireGuard, WARP, Tailscale.
    ///
    /// Vale como candidato — dois pares na mesma Tailscale se alcançam por
    /// aqui e por mais lugar nenhum —, e vale por último.
    Tunel,
    /// Uma ponte de máquina virtual ou de contêiner: `docker0`, `vboxnet`,
    /// `vEthernet (WSL)`.
    ///
    /// O endereço existe e não leva a lugar nenhum fora desta máquina. Fica em
    /// último em vez de ser descartado, pelo mesmo motivo do túnel: a lista de
    /// nomes é heurística, e uma heurística que **apaga** endereço é uma
    /// heurística que um dia apaga o único que funcionava.
    Virtual,
}

/// Um endereço desta máquina, e o que a enumeração soube sobre ele.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Achado {
    /// O endereço.
    pub ip: IpAddr,
    /// A máscara da sub-rede, quando a enumeração a trouxe.
    ///
    /// É o que responde à pergunta do degrau 3: **qual dos meus endereços está
    /// na mesma rede do roteador**. Sem ela, um servidor com VPN pedia ao roteador
    /// que encaminhasse a porta para o endereço do túnel.
    pub mascara: Option<IpAddr>,
    /// De onde ele veio.
    pub origem: Origem,
}

impl Achado {
    /// Em que classe de candidato este endereço entra.
    ///
    /// Rede local antes de qualquer coisa: é o caso comum — os dois na mesma
    /// casa — e o único em que a resposta é imediata. Ver `super::Alcance`.
    #[must_use]
    pub fn classe(&self) -> Origem {
        self.origem
    }

    /// Se este endereço é da rede de casa: `192.168.x.x` e as outras faixas
    /// privadas, ou o `fd00::/8` do IPv6.
    #[must_use]
    pub fn e_da_rede_local(&self) -> bool {
        match self.ip {
            IpAddr::V4(quatro) => quatro.is_private(),
            // fc00::/7, unique-local: o equivalente v6 do 192.168.
            IpAddr::V6(seis) => {
                seis.segments().first().copied().unwrap_or_default() & 0xfe00 == 0xfc00
            }
        }
    }

    /// Se `outro` está na mesma sub-rede que este endereço.
    ///
    /// `false` quando a enumeração não trouxe máscara, e é a resposta certa:
    /// sem máscara não há como saber, e chutar aqui põe uma regra no roteador
    /// apontando para a máquina errada.
    #[must_use]
    pub fn na_mesma_rede(&self, outro: IpAddr) -> bool {
        match (self.ip, self.mascara, outro) {
            (IpAddr::V4(meu), Some(IpAddr::V4(mascara)), IpAddr::V4(dele)) => {
                let meu = u32::from(meu) & u32::from(mascara);
                let dele = u32::from(dele) & u32::from(mascara);
                meu == dele
            }
            _ => false,
        }
    }
}

/// Todos os endereços desta máquina que servem para alguém bater neles.
///
/// Loopback e link-local ficam de fora: o primeiro não sai da máquina e o
/// segundo não sai do cabo — um `fe80::` num `seele://` é um link que parece
/// certo, é aceito pelo analisador e não conecta de lugar nenhum, que é o
/// silêncio que o ADR 0022 manda evitar.
///
/// A ordem é a das interfaces, e quem decide a ordem dos candidatos é
/// `super::Alcance::decidir`.
#[must_use]
pub fn descobrir() -> Vec<Achado> {
    let interfaces = match if_addrs::get_if_addrs() {
        Ok(interfaces) => interfaces,
        Err(erro) => {
            // Não é fatal: sem enumeração sobra o truque da rota padrão, que
            // ainda acerta em toda máquina sem VPN.
            tracing::warn!(%erro, "não deu para enumerar as interfaces desta máquina");
            return Vec::new();
        }
    };

    interfaces
        .into_iter()
        .filter(|interface| {
            !matches!(
                interface.oper_status,
                if_addrs::IfOperStatus::Down
                    | if_addrs::IfOperStatus::NotPresent
                    | if_addrs::IfOperStatus::LowerLayerDown
            )
        })
        .filter_map(|interface| {
            let mascara = match &interface.addr {
                if_addrs::IfAddr::V4(quatro) => Some(IpAddr::V4(quatro.netmask)),
                if_addrs::IfAddr::V6(seis) => Some(IpAddr::V6(seis.netmask)),
            };
            classificar(
                &interface.name,
                interface.is_p2p,
                interface.addr.ip(),
                mascara,
            )
        })
        .collect()
}

/// A parte de [`descobrir`] que não fala com o sistema.
///
/// Separada para poder ser testada com a máquina do relato encenada em quatro
/// linhas — que é o único jeito honesto de exercitar este caminho sem ter uma
/// VPN ligada na máquina que roda o teste.
///
/// `None` quer dizer "este endereço não serve para convidar ninguém".
#[must_use]
pub fn classificar(
    nome: &str,
    ponto_a_ponto: bool,
    ip: IpAddr,
    mascara: Option<IpAddr>,
) -> Option<Achado> {
    if ip.is_loopback() || ip.is_unspecified() {
        return None;
    }
    if e_link_local(ip) {
        return None;
    }

    let origem = if parece_tunel(nome, ponto_a_ponto) {
        Origem::Tunel
    } else if parece_virtual(nome) {
        Origem::Virtual
    } else {
        Origem::Fisica
    };
    Some(Achado {
        ip,
        mascara,
        origem,
    })
}

/// `169.254.0.0/16` e `fe80::/10`: endereços que não saem do cabo.
fn e_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(quatro) => quatro.is_link_local(),
        IpAddr::V6(seis) => seis.segments().first().copied().unwrap_or_default() & 0xffc0 == 0xfe80,
    }
}

/// Se esta interface tem cara de túnel.
///
/// Dois sinais, porque nenhum dos dois basta sozinho. O `is_p2p` vem do
/// `IFF_POINTOPOINT` no Unix e pega `utun*`, `wg*` e `ppp*`; no Windows um
/// adaptador WinTun — que é o que WARP e WireGuard usam — costuma se declarar
/// virtual e não ponto-a-ponto, e aí sobra o nome.
#[must_use]
pub fn parece_tunel(nome: &str, ponto_a_ponto: bool) -> bool {
    if ponto_a_ponto {
        return true;
    }
    let nome = nome.to_ascii_lowercase();
    [
        "utun",
        "tun",
        "tap",
        "wg",
        "ppp",
        "ipsec",
        "vpn",
        "warp",
        "tailscale",
        "zerotier",
        "wireguard",
        "nordlynx",
        "proton",
        "mullvad",
        "anyconnect",
        "openvpn",
    ]
    .iter()
    .any(|marca| nome.contains(marca))
}

/// Se esta interface é uma ponte de contêiner ou de máquina virtual.
///
/// O endereço de um `docker0` é real, responde, e não leva a lugar nenhum fora
/// desta máquina. Sem esta lista, numa máquina de desenvolvimento o primeiro
/// candidato do convite seria o `172.17.0.1` do Docker.
#[must_use]
pub fn parece_virtual(nome: &str) -> bool {
    let nome = nome.to_ascii_lowercase();
    [
        "docker",
        "br-",
        "veth",
        "vboxnet",
        "vmnet",
        "virbr",
        "vethernet",
        "hyper-v",
        "vnic",
    ]
    .iter()
    .any(|marca| nome.contains(marca))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A máquina do relato, encenada: um Windows com Cloudflare WARP ligado, o
    /// túnel com a rota padrão e a Ethernet com a rede de casa.
    ///
    /// É assim que este caminho é exercitado sem uma VPN na máquina que roda o
    /// teste — a alternativa seria não testar, e este é justamente o caminho
    /// que só existe por causa de uma VPN.
    fn a_maquina_do_relato() -> Vec<Achado> {
        [
            // O túnel do WARP: IPv4 privado e IPv6 global, os dois dele.
            ("CloudflareWARP", false, "172.16.0.2", "255.255.255.255"),
            (
                "CloudflareWARP",
                false,
                "2606:4700:110:8a3f:c0de:cafe:1:2",
                "ffff:ffff:ffff:ffff::",
            ),
            // A placa de verdade, e o endereço que o Mac na mesma sala alcança.
            ("Rede (Ethernet)", false, "192.168.0.30", "255.255.255.0"),
        ]
        .into_iter()
        .filter_map(|(nome, p2p, ip, mascara)| {
            classificar(
                nome,
                p2p,
                ip.parse().ok()?,
                mascara.parse().ok().map(|m: IpAddr| m),
            )
        })
        .collect()
    }

    #[test]
    fn o_endereco_da_rede_local_aparece_mesmo_com_a_vpn_com_a_rota_padrao() {
        // O defeito de campo em uma asserção: com WARP ligado, o truque da rota
        // padrão devolvia `172.16.0.2` e o `192.168.0.30` simplesmente não
        // existia para o produto. A enumeração acha os dois.
        let achados = a_maquina_do_relato();
        let ethernet = achados
            .iter()
            .find(|achado| achado.ip == IpAddr::from([192, 168, 0, 30]))
            .expect("o endereço da Ethernet sumiu da enumeração");
        assert_eq!(ethernet.classe(), Origem::Fisica);
        assert!(ethernet.e_da_rede_local());
    }

    #[test]
    fn o_endereco_do_tunel_nao_passa_por_endereco_da_rede_local() {
        // O `172.16.0.2` do WARP é um endereço privado como o da LAN, então a
        // faixa não distingue os dois: o que distingue é a interface. Sem isto
        // o convite continuaria oferecendo o túnel como se fosse a rede de
        // casa, e o degrau 3 pediria ao roteador que encaminhasse para lá.
        let achados = a_maquina_do_relato();
        let tunel = achados
            .iter()
            .find(|achado| achado.ip == IpAddr::from([172, 16, 0, 2]))
            .expect("o endereço do túnel sumiu da enumeração");
        assert_eq!(tunel.classe(), Origem::Tunel);
        assert!(
            tunel.e_da_rede_local(),
            "a faixa dele é privada — é justamente por isso que a faixa não basta"
        );
        assert!(
            achados
                .iter()
                .filter(|achado| achado.classe() == Origem::Fisica)
                .all(|achado| achado.ip != IpAddr::from([172, 16, 0, 2])),
            "o túnel foi oferecido como placa de rede"
        );
    }

    #[test]
    fn o_ipv6_do_tunel_e_global_e_ainda_assim_vem_do_tunel() {
        // O outro lado do mesmo defeito: o IPv6 do WARP passa em `global_v6` —
        // e não tem como não passar, é um unicast global de verdade. Quem sabe
        // que ele é de túnel é a interface, não a faixa.
        let achados = a_maquina_do_relato();
        let seis = achados
            .iter()
            .find(|achado| achado.ip.is_ipv6())
            .expect("o IPv6 do túnel sumiu da enumeração");
        assert_eq!(seis.classe(), Origem::Tunel);
        let IpAddr::V6(endereco) = seis.ip else {
            panic!("o IPv6 do túnel deixou de ser IPv6");
        };
        assert!(
            super::super::global_v6(endereco),
            "este endereço tem de passar por global: é aí que mora a armadilha"
        );
    }

    #[test]
    fn a_mesma_sub_rede_e_o_que_o_degrau_3_pergunta() {
        // O roteador responde de `192.168.0.1`. Qual dos endereços desta
        // máquina é o que ele consegue alcançar? Só o que estiver na sub-rede
        // dele — e é essa a conta que impede o UPnP de mandar o roteador
        // encaminhar a porta para o endereço do túnel.
        let achados = a_maquina_do_relato();
        let roteador = IpAddr::from([192, 168, 0, 1]);
        let na_rede: Vec<IpAddr> = achados
            .iter()
            .filter(|achado| achado.na_mesma_rede(roteador))
            .map(|achado| achado.ip)
            .collect();
        assert_eq!(na_rede, vec![IpAddr::from([192, 168, 0, 30])]);
    }

    #[test]
    fn sem_mascara_ninguem_esta_na_mesma_rede() {
        // Sem máscara não há como saber, e chutar aqui abriria uma porta no
        // roteador apontando para a máquina errada.
        let sem = Achado {
            ip: IpAddr::from([192, 168, 0, 30]),
            mascara: None,
            origem: Origem::Fisica,
        };
        assert!(!sem.na_mesma_rede(IpAddr::from([192, 168, 0, 1])));
    }

    #[test]
    fn loopback_e_link_local_nao_viram_candidato() {
        // Um `127.0.0.1` num convite convida para a máquina de quem recebeu; um
        // `fe80::` é aceito pelo analisador do `seele://` e não conecta de lugar
        // nenhum. Os dois são links que parecem certos.
        for texto in ["127.0.0.1", "::1", "169.254.10.1", "fe80::1", "0.0.0.0"] {
            let ip: IpAddr = texto.parse().expect("endereço de teste inválido");
            assert!(
                classificar("en0", false, ip, None).is_none(),
                "{texto} virou candidato"
            );
        }
    }

    #[test]
    fn uma_ponte_de_conteiner_nao_e_a_rede_de_casa() {
        // Numa máquina de desenvolvimento o `docker0` é um `172.17.0.1`
        // privado, com cara de rede local e que não leva a lugar nenhum fora
        // desta máquina.
        let docker = classificar("docker0", false, IpAddr::from([172, 17, 0, 1]), None)
            .expect("o docker0 sumiu");
        assert_eq!(docker.classe(), Origem::Virtual);
    }

    #[test]
    fn a_ordem_das_classes_poe_a_placa_de_rede_na_frente() {
        // A ordem da declaração do enum é a ordem dos candidatos, e é o que faz
        // quem está na sala ao lado conectar de primeira em vez de esperar o
        // prazo de um endereço que não volta.
        assert!(Origem::Fisica < Origem::Tunel);
        assert!(Origem::Tunel < Origem::Virtual);
    }

    #[test]
    fn os_nomes_de_tunel_conhecidos_sao_reconhecidos() {
        for nome in [
            "utun4",
            "wg0",
            "CloudflareWARP",
            "Tailscale",
            "ProtonVPN",
            "NordLynx",
        ] {
            assert!(parece_tunel(nome, false), "{nome} passou por placa de rede");
        }
        // E o `IFF_POINTOPOINT` sozinho basta, para o túnel que ninguém nomeou.
        assert!(parece_tunel("qualquer0", true));

        for nome in ["en0", "eth0", "Wi-Fi", "Rede (Ethernet)", "Ethernet 2"] {
            assert!(
                !parece_tunel(nome, false),
                "{nome} foi tratado como túnel, e é a placa de rede da máquina"
            );
        }
    }

    #[test]
    fn a_enumeracao_de_verdade_acha_o_endereco_da_rota_padrao() {
        // O único teste daqui que toca no sistema, e ele prova o que importa
        // sobre a máquina que estiver rodando: a enumeração é **superconjunto**
        // do truque antigo. Numa máquina sem VPN os dois coincidem; numa
        // máquina com VPN a enumeração acha também o endereço que o truque
        // perdia — e é por isso que a asserção é "contém", e não "é igual".
        let achados = descobrir();
        if achados.is_empty() {
            eprintln!("pulado: esta máquina não tem interface com endereço utilizável");
            return;
        }
        let Some(rota_padrao) = super::super::endereco_de_saida_v4() else {
            eprintln!("pulado: esta máquina não tem rota IPv4 para lugar nenhum");
            return;
        };
        eprintln!("a rota padrão sai por {rota_padrao}; a enumeração achou {achados:?}");
        assert!(
            achados.iter().any(|achado| achado.ip == rota_padrao),
            "a enumeração perdeu o endereço que a rota padrão usa: {rota_padrao}"
        );
    }
}
