//! O que se fala com um ponto de encontro. Degrau 4 do ADR 0022.
//!
//! ```text
//! SEELE-ENC/1 ONDE <marca>
//! SEELE-ENC/1 LEVE <destino> <marca>
//! SEELE-ENC/1 AQUI <marca> <origem>
//! ```
//!
//! Três linhas, e é o protocolo inteiro. Duas entram no ponto de encontro e uma
//! sai dele, e a única coisa que ele sabe fazer é **dizer de onde um pacote
//! veio** — para quem o mandou (`ONDE`) ou para um terceiro endereço (`LEVE`).
//!
//! # Por que isto basta para furar um NAT
//!
//! Nenhuma máquina atrás de NAT sabe o próprio endereço público: o roteador
//! reescreve isso na saída e o interior nunca vê o resultado (ADR 0022). Alguém
//! de fora precisa contar — e é só isso que o ponto de encontro faz.
//!
//! O anfitrião pergunta `ONDE` e aprende o endereço público da escuta de avisos
//! dele; pergunta `LEVE <aquele endereço>` **pelo socket do servidor** e aprende o
//! endereço público do servidor, que é o que vai no convite. Quem recebe o convite
//! manda um `LEVE <endereço de avisos do anfitrião>` pelo socket com que vai
//! conectar; o ponto de encontro conta ao anfitrião de onde aquele pacote veio,
//! e o anfitrião manda alguns pacotes para lá. Os dois roteadores passam a ter
//! uma saída registrada para o outro lado, e daí em diante o tráfego é direto.
//!
//! # O que ele não faz, e por construção
//!
//! **Não guarda nada.** [`responder`] é uma função livre: não tem `self`, não
//! tem tabela, não recebe estado e não devolve estado. Um `LEVE` funciona sem
//! nenhum `ONDE` antes dele, e reiniciar o processo não muda resposta nenhuma —
//! não porque alguém teve o cuidado de limpar, mas porque não há onde guardar.
//!
//! **Não repassa conteúdo.** A resposta é **montada** aqui, campo a campo, e
//! nunca copiada do pedido: o que sai é o verbo, a marca (alfanumérica, curta) e
//! um endereço que o próprio ponto observou. Não há caminho por onde um byte
//! escolhido por quem manda chegue a quem recebe.
//!
//! **Não decide para onde ninguém conecta.** Quem recebe o convite nunca lê
//! resposta nenhuma daqui: os endereços que ele tenta saem do `seele://`, e a
//! impressão digital que ele confere também. Um ponto de encontro hostil
//! consegue não responder, e nada além disso.
//!
//! # Amplificação, e por que os pedidos são gordos
//!
//! `LEVE` manda um pacote para um endereço que quem pede escolheu. Isso é um
//! refletor, e um refletor que responde mais do que recebe é uma arma. Por isso
//! todo datagrama tem [`TAMANHO`] bytes — os pedidos vão com enchimento — e a
//! resposta nunca é maior que o pedido que a causou. O ganho de banda de quem
//! abusar é, no máximo, 1:1.
//!
//! O resto das defesas está em [`responder`]: destino global, porta alta, e
//! nada que não seja exatamente uma destas três linhas.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Porta padrão de um ponto de encontro.
///
/// 8384, ao lado do 8383 do servidor: quem for abrir uma no firewall vai abrir as
/// duas na mesma tarde.
pub const PORTA_PADRAO: u16 = 8383 + 1;

/// O que todo datagrama deste protocolo começa dizendo.
///
/// O `/1` é a versão. Um ponto de encontro que receba `/2` não responde, que é
/// o comportamento certo: ele não sabe o que fazer com aquilo, e adivinhar é
/// pior que calar.
pub const MAGIA: &str = "SEELE-ENC/1";

/// O tamanho de todo datagrama, pedido ou resposta.
///
/// Fixo de propósito. Ver a nota sobre amplificação no cabeçalho: um pedido
/// menor que a resposta que ele provoca transformaria o ponto de encontro num
/// amplificador, e quem paga por isso é uma vítima que nunca ouviu falar deste
/// projeto. Cabe o maior endereço possível — um IPv6 com colchetes e porta —
/// com folga.
pub const TAMANHO: usize = 96;

/// O caractere de enchimento. Escolhido por ser visível num `tcpdump`.
const ENCHIMENTO: u8 = b'.';

/// Quantos caracteres uma marca pode ter.
///
/// 32 cabe uma impressão digital pela metade, que é o uso previsto — ver
/// [`Marca`].
pub const LIMITE_DA_MARCA: usize = 32;

/// A etiqueta que volta na resposta, e o único campo que quem pede escolhe.
///
/// Alfanumérica e curta, e as duas coisas são defesa: ela é o único pedaço do
/// pedido que reaparece na resposta, então ela é também o único caminho por onde
/// alguém tentaria enfiar bytes escolhidos num datagrama que outra pessoa vai
/// receber.
///
/// # Para que ela serve de verdade
///
/// Para o anfitrião saber quem bateu. Ele conhece a marca que ele mesmo mandou,
/// e conhece a que quem tem o convite vai usar — os primeiros dígitos da
/// impressão digital dele, que está no `seele://` e em nenhum outro lugar. Uma
/// marca que não é nenhuma das duas é ruído da internet, e não vira furo de NAT
/// nenhum.
///
/// Não é autenticação e não tenta ser: quem tem o link, tem. É o que impede um
/// varredor de portas de fazer um servidor mandar pacotes para onde ele quiser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marca(String);

impl Marca {
    /// Uma marca, se este texto puder ser uma.
    ///
    /// `None` para vazio, longo demais, ou com qualquer coisa que não seja
    /// letra ou número.
    #[must_use]
    pub fn nova(texto: &str) -> Option<Self> {
        if texto.is_empty() || texto.len() > LIMITE_DA_MARCA {
            return None;
        }
        texto
            .chars()
            .all(|c| c.is_ascii_alphanumeric())
            .then(|| Self(texto.to_owned()))
    }

    /// O texto dela.
    #[must_use]
    pub fn texto(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Marca {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// O que se pede a um ponto de encontro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pedido {
    /// "De onde você vê que este pacote veio?" A resposta volta para a origem.
    Onde {
        /// A etiqueta que volta na resposta.
        marca: Marca,
    },
    /// "Conte a este endereço de onde você vê que este pacote veio."
    ///
    /// É a apresentação inteira: o pacote que o anfitrião recebe por causa
    /// disto é o aviso de que alguém quer entrar, **e** o endereço para onde
    /// furar.
    Leve {
        /// Quem recebe o aviso.
        destino: SocketAddr,
        /// A etiqueta que vai nele.
        marca: Marca,
    },
}

/// O que um ponto de encontro tem a dizer, e para quem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resposta {
    /// Para onde este datagrama vai.
    pub destino: SocketAddr,
    /// Os bytes, já com [`TAMANHO`].
    pub datagrama: Vec<u8>,
}

/// Monta um `ONDE`.
#[must_use]
pub fn onde(marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} ONDE {marca}"))
}

/// Monta um `LEVE`.
#[must_use]
pub fn leve(destino: SocketAddr, marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} LEVE {destino} {marca}"))
}

/// Monta um `AQUI`.
#[must_use]
pub fn aqui(marca: &Marca, origem: SocketAddr) -> Vec<u8> {
    encher(format!("{MAGIA} AQUI {marca} {origem}"))
}

/// Monta um `FURO`: o pacote que abre o caminho, mandado de uma ponta à outra.
///
/// **Não é para o ponto de encontro**, e ele não responde a isto — `analisar`
/// não conhece este verbo. É o pacote que o anfitrião manda ao endereço que o
/// aviso trouxe, para que o roteador dele passe a ter uma saída registrada para
/// lá. Quem recebe descarta: do outro lado quem está lendo o socket é o QUIC.
///
/// Existe como verbo nomeado por uma razão só: quem estiver olhando um
/// `tcpdump` para entender por que uma conexão não sobe merece ver o que é
/// aquele pacote de 96 bytes, em vez de bytes anônimos.
#[must_use]
pub fn furo(marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} FURO {marca}"))
}

/// Completa a linha até [`TAMANHO`], para que nada amplifique.
fn encher(mut linha: String) -> Vec<u8> {
    linha.push('\n');
    let mut bytes = linha.into_bytes();
    bytes.resize(TAMANHO, ENCHIMENTO);
    bytes
}

/// A decisão inteira de um ponto de encontro, sem estado e sem rede.
///
/// Recebe o datagrama e de onde ele veio; devolve o que mandar e para quem, ou
/// `None` para calar. É uma função livre porque o ponto de encontro **é** uma
/// função livre: o processo que a chama não guarda nada entre um datagrama e o
/// outro, e não é possível fazê-lo guardar sem mudar esta assinatura.
///
/// # Quando ela cala, e por quê
///
/// - Tamanho diferente de [`TAMANHO`] — inclusive **menor**, e essa é a defesa
///   contra amplificação: sem ela, um pedido curto provocaria uma resposta
///   longa contra uma origem forjada.
/// - Não começa com [`MAGIA`], ou traz um verbo desconhecido, ou uma marca com
///   caractere que marca não tem. Nada aqui tenta adivinhar.
/// - `AQUI`. É o que sai daqui, não o que entra: um ponto de encontro que
///   respondesse a respostas seria um laço entre dois pontos de encontro.
/// - `LEVE` para um endereço que não é da internet — privado, loopback,
///   multicast, CGNAT, documentação. Um refletor que alcança rede privada é uma
///   forma de bater em máquinas que não estão na internet a partir de uma que
///   está.
/// - `LEVE` para porta abaixo de 1024. É onde moram os serviços que respondem a
///   qualquer coisa (DNS, NTP), e mandar pacote para lá em nome de outra pessoa
///   é o começo de uma reflexão em cadeia.
#[must_use]
pub fn responder(datagrama: &[u8], origem: SocketAddr) -> Option<Resposta> {
    responder_em(datagrama, origem, Vizinhanca::Internet)
}

/// Até onde um `LEVE` pode apontar.
///
/// Existe porque a regra certa na internet — só endereço da internet —
/// impossibilita experimentar o mecanismo numa máquina só, e um mecanismo que
/// não dá para experimentar é um que ninguém confere antes de apontar o mundo
/// para ele.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Vizinhanca {
    /// Só endereços da internet. O que um ponto de encontro público usa.
    #[default]
    Internet,
    /// Também rede local e loopback.
    ///
    /// Para o teste automatizado e para quem sobe um ponto de encontro dentro
    /// da própria rede — numa VPN de rede, por exemplo — antes de subir um de
    /// verdade. Num ponto público isto é um refletor apontado para dentro da
    /// rede de quem hospeda, e é por isso que não é o padrão.
    TambemAqui,
}

/// [`responder`], dizendo até onde um `LEVE` alcança.
#[must_use]
pub fn responder_em(
    datagrama: &[u8],
    origem: SocketAddr,
    vizinhanca: Vizinhanca,
) -> Option<Resposta> {
    match analisar(datagrama)? {
        Pedido::Onde { marca } => Some(Resposta {
            destino: origem,
            datagrama: aqui(&marca, origem),
        }),
        Pedido::Leve { destino, marca } => {
            let permitido = match vizinhanca {
                Vizinhanca::Internet => alcancavel(destino),
                // A porta continua valendo: refletir para a porta 53 de um
                // resolvedor da rede local é o mesmo estrago, mais perto.
                Vizinhanca::TambemAqui => destino.port() >= 1024,
            };
            if !permitido {
                return None;
            }
            Some(Resposta {
                destino,
                datagrama: aqui(&marca, origem),
            })
        }
    }
}

/// Lê um pedido, sem julgar o destino. Ver [`responder`].
#[must_use]
pub fn analisar(datagrama: &[u8]) -> Option<Pedido> {
    let linha = linha_de(datagrama)?;
    let resto = linha.strip_prefix(MAGIA)?.strip_prefix(' ')?;
    let (verbo, resto) = resto.split_once(' ')?;
    match verbo {
        "ONDE" => Some(Pedido::Onde {
            marca: Marca::nova(resto)?,
        }),
        "LEVE" => {
            let (destino, marca) = resto.split_once(' ')?;
            Some(Pedido::Leve {
                destino: destino.parse().ok()?,
                marca: Marca::nova(marca)?,
            })
        }
        _ => None,
    }
}

/// Lê um `AQUI`: a marca que voltou e o endereço que o ponto de encontro viu.
///
/// É o que o anfitrião chama ao receber um aviso. Devolve `None` para qualquer
/// outra coisa, incluindo os pedidos — quem espera resposta não trata pedido.
#[must_use]
pub fn ler_aqui(datagrama: &[u8]) -> Option<(Marca, SocketAddr)> {
    let linha = linha_de(datagrama)?;
    let resto = linha.strip_prefix(MAGIA)?.strip_prefix(' ')?;
    let resto = resto.strip_prefix("AQUI ")?;
    let (marca, endereco) = resto.split_once(' ')?;
    Some((Marca::nova(marca)?, endereco.parse().ok()?))
}

/// A linha útil de um datagrama do tamanho certo, sem o enchimento.
fn linha_de(datagrama: &[u8]) -> Option<&str> {
    if datagrama.len() != TAMANHO {
        return None;
    }
    let texto = std::str::from_utf8(datagrama).ok()?;
    texto.split('\n').next()
}

/// Se um `LEVE` pode apontar para cá.
///
/// A pergunta não é "este endereço existe" e sim "mandar um pacote para cá em
/// nome de um desconhecido faz mal a alguém". Ver [`responder`].
#[must_use]
pub fn alcancavel(destino: SocketAddr) -> bool {
    // Serviços de sistema respondem a qualquer coisa; um pacote nosso batendo
    // lá em nome de outra pessoa é o primeiro passo de uma reflexão em cadeia.
    if destino.port() < 1024 {
        return false;
    }
    match destino.ip() {
        IpAddr::V4(quatro) => global_v4(quatro),
        IpAddr::V6(seis) => global_v6(seis),
    }
}

/// Se este IPv4 é da internet.
///
/// Escrito à mão porque `Ipv4Addr::is_global` ainda não é estável e o MSRV do
/// ADR 0011 não espera por ele. A faixa `100.64.0.0/10` está aqui pelo mesmo
/// motivo que está no degrau 3: a RFC 6598 a reservou para CGNAT, e ela não é
/// privada para a `std`.
fn global_v4(endereco: Ipv4Addr) -> bool {
    let [a, b, _, _] = endereco.octets();
    let cgnat = a == 100 && (64..128).contains(&b);
    !endereco.is_private()
        && !endereco.is_loopback()
        && !endereco.is_link_local()
        && !endereco.is_broadcast()
        && !endereco.is_documentation()
        && !endereco.is_unspecified()
        && !endereco.is_multicast()
        && !cgnat
        && a != 0
        && a < 240
}

/// Se este IPv6 é da internet.
fn global_v6(endereco: Ipv6Addr) -> bool {
    let primeiro = endereco.segments().first().copied().unwrap_or_default();
    !endereco.is_loopback()
        && !endereco.is_unspecified()
        && !endereco.is_multicast()
        // fe80::/10, link-local.
        && primeiro & 0xffc0 != 0xfe80
        // fc00::/7, unique-local.
        && primeiro & 0xfe00 != 0xfc00
        // 2001:db8::/32, documentação (RFC 3849).
        && !(primeiro == 0x2001 && endereco.segments().get(1) == Some(&0x0db8))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn marca(texto: &str) -> Marca {
        Marca::nova(texto).expect("marca de teste inválida")
    }

    fn de(texto: &str) -> SocketAddr {
        texto.parse().expect("endereço de teste inválido")
    }

    #[test]
    fn um_onde_volta_para_quem_perguntou_com_o_endereco_que_o_ponto_viu() {
        // O degrau 4 inteiro depende disto: ninguém atrás de NAT sabe o próprio
        // endereço público, e esta é a única forma de descobrir.
        let origem = de("45.33.32.156:41234");
        let resposta = responder(&onde(&marca("abc123")), origem).expect("calou num ONDE");
        assert_eq!(resposta.destino, origem, "respondeu para outro endereço");
        assert_eq!(
            ler_aqui(&resposta.datagrama),
            Some((marca("abc123"), origem))
        );
    }

    #[test]
    fn um_leve_apresenta_os_dois_sem_nunca_ter_ouvido_falar_de_nenhum() {
        // "Sem estado" não é uma promessa deste módulo: é a assinatura dele.
        // Este `LEVE` funciona sem nenhum `ONDE` antes, de nenhum dos dois
        // lados, porque não existe cadastro para consultar.
        let visitante = de("200.160.2.3:50000");
        let anfitriao = de("45.33.32.156:41234");
        let resposta =
            responder(&leve(anfitriao, &marca("3cbcfb0212da738f")), visitante).expect("calou");
        assert_eq!(resposta.destino, anfitriao);
        assert_eq!(
            ler_aqui(&resposta.datagrama),
            Some((marca("3cbcfb0212da738f"), visitante)),
            "o anfitrião não recebeu o endereço para onde furar"
        );
    }

    #[test]
    fn a_mesma_pergunta_tem_a_mesma_resposta_sempre() {
        // Um ponto de encontro que aprendesse alguma coisa teria uma segunda
        // resposta diferente da primeira. Este não tem como.
        let origem = de("45.33.32.156:41234");
        let pedido = leve(de("200.160.2.3:50000"), &marca("umamarca"));
        let primeira = responder(&pedido, origem);
        let segunda = responder(&pedido, origem);
        assert_eq!(primeira, segunda);
        assert!(primeira.is_some());
    }

    #[test]
    fn nenhuma_resposta_e_maior_que_o_pedido_que_a_causou() {
        // A defesa contra amplificação, e ela vale para todo pedido aceito: um
        // refletor que devolve mais do que recebe é uma arma apontada para
        // alguém que nunca ouviu falar deste projeto.
        let origens = [de("45.33.32.156:41234"), de("[2001:4860:4860::8888]:65535")];
        let pedidos = [
            onde(&marca("a")),
            onde(&marca(&"z".repeat(LIMITE_DA_MARCA))),
            leve(de("200.160.2.3:50000"), &marca("a")),
            leve(
                de("[2606:4700:4700::1111]:65535"),
                &marca(&"z".repeat(LIMITE_DA_MARCA)),
            ),
        ];
        for origem in origens {
            for pedido in &pedidos {
                let Some(resposta) = responder(pedido, origem) else {
                    panic!("recusou um pedido legítimo");
                };
                assert!(
                    resposta.datagrama.len() <= pedido.len(),
                    "amplificou: {} bytes de resposta para {} de pedido",
                    resposta.datagrama.len(),
                    pedido.len()
                );
            }
        }
    }

    #[test]
    fn um_pedido_curto_e_recusado_mesmo_estando_certo() {
        // É o mesmo `ONDE` do primeiro teste, sem o enchimento. Aceitá-lo seria
        // aceitar amplificação de graça, e o formato existe para isso.
        let curto = b"SEELE-ENC/1 ONDE abc123\n";
        assert_eq!(responder(curto, de("45.33.32.156:41234")), None);
    }

    #[test]
    fn um_leve_nao_alcanca_rede_que_nao_e_da_internet() {
        // Um refletor que entra em rede privada é uma forma de bater em
        // máquinas que não estão na internet a partir de uma que está — a
        // mesma família de estrago que um SSRF.
        for destino in [
            "192.168.0.1:8383",
            "10.0.0.1:8383",
            "172.16.0.1:8383",
            "127.0.0.1:8383",
            "169.254.1.1:8383",
            "100.64.0.1:8383",
            "[::1]:8383",
            "[fe80::1]:8383",
            "[fd12:3456::1]:8383",
            "[2001:db8::1]:8383",
            "[ff02::1]:8383",
            "0.0.0.0:8383",
            "224.0.0.1:8383",
        ] {
            let pedido = leve(de(destino), &marca("abc"));
            assert_eq!(
                responder(&pedido, de("45.33.32.156:41234")),
                None,
                "aceitou refletir para {destino}"
            );
        }
    }

    #[test]
    fn um_leve_nao_alcanca_porta_de_servico() {
        // DNS, NTP e companhia respondem a qualquer coisa. Um pacote nosso
        // batendo lá em nome de outra pessoa começa uma reflexão em cadeia.
        for porta in [53_u16, 123, 161, 1023] {
            let destino = SocketAddr::from(([200, 160, 2, 3], porta));
            assert_eq!(
                responder(&leve(destino, &marca("abc")), de("45.33.32.156:41234")),
                None,
                "aceitou refletir para a porta {porta}"
            );
        }
        // E a porta logo acima continua valendo: a regra é /1024, não "portas
        // que eu conheço".
        let alta = SocketAddr::from(([200, 160, 2, 3], 1024));
        assert!(responder(&leve(alta, &marca("abc")), de("45.33.32.156:41234")).is_some());
    }

    #[test]
    fn nada_do_pedido_atravessa_para_a_resposta_alem_da_marca() {
        // "Não consegue ler nem repassar" — a resposta é montada campo a campo
        // e nunca copiada. O único pedaço que reaparece é a marca, e ela é
        // alfanumérica e curta justamente por isso.
        let mut pedido = leve(de("200.160.2.3:50000"), &marca("abc"));
        // Bytes escolhidos por quem manda, no enchimento, onde caberia um
        // repasse desatento.
        for (indice, byte) in pedido.iter_mut().enumerate().skip(40) {
            *byte = if indice % 2 == 0 { b'X' } else { b'Y' };
        }
        let Some(resposta) = responder(&pedido, de("45.33.32.156:41234")) else {
            panic!("o enchimento sujo derrubou um pedido válido");
        };
        assert!(
            !resposta.datagrama.windows(2).any(|par| par == b"XY"),
            "bytes escolhidos por quem pediu chegaram a quem recebe"
        );
    }

    #[test]
    fn o_pacote_do_furo_nao_e_coisa_do_ponto_de_encontro() {
        // O furo vai de uma ponta à outra e nunca ao meio. Um ponto de encontro
        // que respondesse a ele estaria a um passo de repassar pacote entre as
        // duas pontas — que é retransmissão, e o ADR 0022 a deixou fora de
        // escopo por decisão.
        let pacote = furo(&marca("abc"));
        assert_eq!(responder(&pacote, de("45.33.32.156:41234")), None);
        assert_eq!(analisar(&pacote), None);
        assert_eq!(ler_aqui(&pacote), None);
    }

    #[test]
    fn uma_resposta_nao_e_um_pedido() {
        // Dois pontos de encontro apontados um para o outro não podem virar um
        // laço, e um `AQUI` que entra é ou ruído ou exatamente essa tentativa.
        let aqui = aqui(&marca("abc"), de("45.33.32.156:41234"));
        assert_eq!(responder(&aqui, de("200.160.2.3:50000")), None);
        assert_eq!(analisar(&aqui), None);
    }

    #[test]
    fn lixo_nao_vira_resposta() {
        let origem = de("45.33.32.156:41234");
        let mut casos: Vec<Vec<u8>> = vec![
            vec![0_u8; TAMANHO],
            vec![b'A'; TAMANHO],
            encher("SEELE-ENC/2 ONDE abc".to_owned()),
            encher("SEELE-ENC/1 ONDE".to_owned()),
            encher("SEELE-ENC/1 ONDE tem espaço".to_owned()),
            encher("SEELE-ENC/1 VAI 45.33.32.156:1 abc".to_owned()),
            encher(format!(
                "SEELE-ENC/1 ONDE {}",
                "z".repeat(LIMITE_DA_MARCA + 1)
            )),
            encher("SEELE-ENC/1 LEVE naoehendereco abc".to_owned()),
            encher("SEELE-ENC/1 LEVE 45.33.32.156 abc".to_owned()),
            Vec::new(),
        ];
        // Um datagrama com um byte que não é UTF-8 no meio da linha útil.
        let mut invalido = onde(&marca("abc"));
        if let Some(byte) = invalido.get_mut(13) {
            *byte = 0xff;
        }
        casos.push(invalido);

        for caso in casos {
            assert_eq!(
                responder(&caso, origem),
                None,
                "respondeu a lixo: {:?}",
                String::from_utf8_lossy(&caso)
            );
        }
    }

    #[test]
    fn uma_marca_e_curta_e_alfanumerica() {
        assert!(Marca::nova("").is_none());
        assert!(Marca::nova(&"z".repeat(LIMITE_DA_MARCA + 1)).is_none());
        assert!(Marca::nova("tem espaço").is_none());
        assert!(Marca::nova("tem\nlinha").is_none());
        assert!(Marca::nova("3cbcfb0212da738f").is_some());
        assert!(Marca::nova(&"z".repeat(LIMITE_DA_MARCA)).is_some());
    }

    #[test]
    fn a_vizinhanca_maior_abre_a_rede_local_e_nao_a_porta_de_servico() {
        // O que o teste automatizado usa, e é a única frouxidão que existe:
        // continua sendo um datagrama do tamanho certo, com verbo conhecido, e
        // continua sem alcançar porta de serviço.
        let local = de("127.0.0.1:40000");
        let pedido = leve(local, &marca("abc"));
        assert_eq!(responder(&pedido, de("45.33.32.156:41234")), None);
        assert!(responder_em(&pedido, de("45.33.32.156:41234"), Vizinhanca::TambemAqui).is_some());

        let servico = leve(de("127.0.0.1:53"), &marca("abc"));
        assert_eq!(
            responder_em(&servico, de("45.33.32.156:41234"), Vizinhanca::TambemAqui),
            None,
            "a vizinhança maior virou vale-tudo"
        );
    }

    #[test]
    fn a_porta_padrao_fica_ao_lado_da_do_server() {
        assert_eq!(PORTA_PADRAO, 8384);
    }
}
