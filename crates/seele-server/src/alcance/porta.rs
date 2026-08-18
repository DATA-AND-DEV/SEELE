//! Degrau 3 do ADR 0022: pedir a porta ao **próprio** roteador.
//!
//! Nenhum terceiro em lugar nenhum. O anfitrião fala com o roteador da casa
//! dele, o roteador abre a porta, e daí em diante o tráfego é direto. É o
//! degrau com a melhor relação entre esforço e casas resolvidas, e é o que o
//! ADR 0022 sugeriu fazer primeiro.
//!
//! # UPnP falha em silêncio, e falhar em silêncio é o defeito
//!
//! O ADR nomeia três casos em que isto não funciona — roteador com UPnP
//! desligado, operadora com CGNAT, condomínio — e é por isso que **todo
//! caminho daqui termina numa [`FalhaAoAbrir`] que diz qual foi**. Um pedido de
//! porta que não deu certo tem de dizer que não deu; quem hospeda precisa poder
//! contar isso a quem está na frente da tela, em vez de a pessoa ficar achando
//! que abriu.
//!
//! # O que a implementação ensinou, e o ADR não previa
//!
//! O ADR trata CGNAT como um caso em que **UPnP não funciona**. Não é bem
//! isso, e a diferença importa: nesses casos o roteador da casa quase sempre
//! atende o pedido e **abriria a porta com sucesso** — só que na WAN dele, que
//! é um endereço que não sai para a internet. O `AddPortMapping` devolveria
//! `Ok`, o mapeamento existiria, e não serviria absolutamente para nada.
//!
//! É a forma mais cruel do silêncio que este módulo existe para não ter: não é
//! um erro que se possa mostrar, é um sucesso mentiroso. Por isso o endereço
//! externo é perguntado **antes** de mapear e conferido contra as faixas que
//! não roteiam — inclusive a `100.64.0.0/10`, que a RFC 6598 reservou
//! exatamente para CGNAT. Ver [`FalhaAoAbrir::SemSaidaParaInternet`].
//!
//! Não é teoria: na primeira rede real em que isto rodou, o roteador que
//! respondeu à busca informou WAN `192.168.0.30` — ele mesmo pendurado em outro
//! roteador. A conferência pegou de primeira, e sem ela o Dogma teria montado
//! um `seele://` para um endereço aonde ninguém chega.
//!
//! # Por que `igd-next`, e o que ele arrasta junto
//!
//! Medido, não escolhido por fama. Contra a árvore que este workspace já tem:
//!
//! | Candidato | Crates novos | O que resolve |
//! |---|---|---|
//! | `igd-next` | 6 | UPnP IGD |
//! | `crab_nat` | 1 | NAT-PMP e PCP, **sem** descobrir o gateway |
//! | `portmapper` | 31 | os três, e traz `netdev`, `iroh-metrics`, `objc2-*` |
//!
//! `portmapper` foi recusado pelo tamanho: 31 crates novos, incluindo
//! enumeração de interfaces e detecção de wifi, num daemon que por
//! `specs/04-servidor-seele.md` tem de caber em 1 vCPU e 512 MB.
//!
//! `igd-next` não é limpo e vale dizer: `attohttpc` é dependência **não
//! opcional** dele, então um segundo cliente HTTP entra mesmo usando só o
//! caminho assíncrono; e o `hyper` vem com a feature `http2` fixada, então o
//! `h2` é ligado para falar um protocolo que é HTTP/1.1 e SOAP. É desperdício
//! conhecido, e entrou assim mesmo porque UPnP IGD é o que a maioria dos
//! roteadores domésticos fala — e sem ele o degrau 3 não cumpre o que o ADR
//! promete, que é "resolver boa parte das casas".
//!
//! # Por que NAT-PMP e PCP ficaram de fora
//!
//! O ADR nomeia o degrau 3 como "UPnP / NAT-PMP", e só o UPnP chegou.
//! `crab_nat` custaria **um** crate e é bom, mas o próprio README dele diz que
//! não descobre o endereço do gateway e recomenda `netdev` para isso — e é o
//! `netdev` que custa caro (é de onde vinham os `objc2-core-wlan`,
//! `objc2-security` e afins na conta do `portmapper` acima). Pagar uma pilha de
//! enumeração de interfaces para cobrir a fatia de roteadores que fala PCP e
//! **não** fala UPnP não se paga hoje.
//!
//! Não é discordância do ADR: o degrau 3 é "o anfitrião pede a porta ao
//! roteador dele", e é o que acontece aqui. Fica registrado como o próximo
//! passo barato se aparecer descoberta de gateway sem esse custo.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use igd_next::aio::tokio::Tokio;
use igd_next::aio::Gateway;
use igd_next::{PortMappingProtocol, SearchOptions};

/// Quanto tempo de validade o mapeamento pede ao roteador.
///
/// Uma hora, e não "para sempre". Um mapeamento permanente sobrevive ao
/// processo que o pediu: se o Dogma morrer de mau jeito — queda de energia,
/// `kill -9`, pânico — a porta fica aberta no roteador apontando para uma
/// máquina que não atende mais, e ninguém nunca mais a fecha. Com prazo, o pior
/// caso se conserta sozinho em uma hora.
const VALIDADE: Duration = Duration::from_secs(3600);

/// De quanto em quanto tempo o mapeamento é renovado.
///
/// Bem antes da metade da [`VALIDADE`], de propósito: a renovação é um pedido
/// pela rede e pode falhar. Com esta margem cabem várias tentativas perdidas
/// antes de a porta realmente expirar debaixo de uma conversa em andamento.
const RENOVACAO: Duration = Duration::from_secs(1200);

/// Quanto tempo esperar um roteador aparecer.
///
/// O padrão do `igd-next` é 10 s, e 10 s é tempo demais: isto roda entre
/// apertar **HOSPEDAR AQUI** e a sala abrir. Numa rede sem UPnP a busca sempre
/// esgota o prazo, então este número é literalmente quanto a pessoa espera à
/// toa no pior caso.
const PROCURA: Duration = Duration::from_secs(3);

/// O nome que aparece na lista de encaminhamentos do roteador.
///
/// Quem for olhar aquela tela merece ver de onde a regra veio.
const DESCRICAO: &str = "SEELE Dogma";

/// Por que não deu para abrir a porta.
///
/// Cada variante é uma frase diferente para quem hospeda, e é por isso que são
/// variantes e não uma string: as coisas que a pessoa pode fazer a respeito são
/// diferentes. Ligar UPnP no roteador é uma; trocar de operadora não é.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaAoAbrir {
    /// Nenhum roteador respondeu à busca.
    ///
    /// UPnP desligado, ou um roteador que não fala UPnP, ou uma rede em que o
    /// multicast não chega ao roteador — condomínio, rede de empresa, hotel.
    /// Não dá para distinguir os três daqui, e prometer que dá seria pior.
    NenhumRoteador,
    /// O roteador respondeu e recusou o mapeamento.
    RoteadorRecusou(String),
    /// O roteador respondeu, e o endereço externo dele não sai para a internet.
    ///
    /// Dois casos caem aqui, e o ADR 0022 nomeia os dois. **CGNAT**: a
    /// operadora tem outro NAT em cima, e a WAN do roteador da casa é um
    /// endereço da faixa `100.64.0.0/10`. **NAT duplo**, o "condomínio": o
    /// roteador que respondeu está ele mesmo pendurado noutro roteador, e a WAN
    /// dele é um `192.168.x.x` — foi o que apareceu na primeira rede real em
    /// que isto rodou.
    ///
    /// Nos dois o mapeamento **daria certo** se fosse pedido, e não serviria
    /// para nada: a porta abriria num endereço aonde ninguém de fora chega. Por
    /// isso a pergunta vem antes do pedido. Antes do degrau 4 não há saída
    /// daqui, e o ADR 0022 manda dizer isso em voz alta em vez de deixar a
    /// pessoa descobrindo sozinha.
    SemSaidaParaInternet {
        /// O endereço que o roteador diz ser o dele.
        externo: IpAddr,
    },
    /// O roteador respondeu e não disse qual é o endereço externo dele.
    SemEnderecoExterno(String),
    /// O roteador respondeu, e nenhum endereço desta máquina está na rede dele.
    ///
    /// É o que uma VPN produz quando ela é a única coisa com endereço IPv4
    /// utilizável aqui: o roteador responde de `192.168.0.1` e o único endereço
    /// da máquina é o `172.16.0.2` do túnel. Pedir o mapeamento assim mesmo
    /// abriria a porta apontando para um endereço que não existe naquela rede —
    /// e o roteador diria `Ok`.
    SemEnderecoNaRedeDoRoteador {
        /// De onde o roteador respondeu.
        roteador: IpAddr,
    },
}

impl std::fmt::Display for FalhaAoAbrir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NenhumRoteador => write!(
                f,
                "nenhum roteador respondeu ao pedido de porta (UPnP desligado, \
                 ou uma rede que não deixa o pedido chegar)"
            ),
            Self::RoteadorRecusou(motivo) => {
                write!(f, "o roteador recusou abrir a porta: {motivo}")
            }
            Self::SemSaidaParaInternet { externo } => write!(
                f,
                "o roteador respondeu, e o endereço dele ({externo}) não sai para \
                 a internet — abrir a porta ali daria certo e não adiantaria \
                 nada. É CGNAT da operadora, ou um segundo roteador acima deste"
            ),
            Self::SemEnderecoExterno(motivo) => write!(
                f,
                "o roteador não disse qual é o endereço externo dele: {motivo}"
            ),
            Self::SemEnderecoNaRedeDoRoteador { roteador } => write!(
                f,
                "o roteador respondeu de {roteador} e nenhum endereço desta \
                 máquina está na rede dele — costuma ser uma VPN ligada, que \
                 fica com todo o tráfego e deixa a placa de rede sem uso"
            ),
        }
    }
}

impl std::error::Error for FalhaAoAbrir {}

/// Uma porta aberta no roteador, e a tarefa que a mantém aberta.
///
/// Descartar isto **não** fecha a porta no roteador: `Drop` não pode esperar, e
/// devolver a porta é um pedido pela rede. O que o `Drop` faz é parar de
/// renovar, e aí o prazo da [`VALIDADE`] fecha sozinho. Para devolver na hora,
/// [`PortaAberta::fechar`] — o mesmo desenho de `Hospedagem::encerrar`, e pelo
/// mesmo motivo.
pub struct PortaAberta {
    roteador: Gateway<Tokio>,
    externo: SocketAddr,
    renovacao: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for PortaAberta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortaAberta")
            .field("externo", &self.externo)
            .finish()
    }
}

impl PortaAberta {
    /// O endereço público em que quem está de fora bate.
    ///
    /// É o que vai no `seele://`. Pode ter porta diferente da interna: se a
    /// porta pedida já estava tomada no roteador, outra foi aceita — e o link
    /// carrega a porta, então isso não atrapalha ninguém.
    #[must_use]
    pub fn externo(&self) -> SocketAddr {
        self.externo
    }

    /// Devolve a porta ao roteador.
    ///
    /// Consome, e espera. Deixar a regra para trás no roteador é sujeira que
    /// sobrevive ao programa: aponta para uma máquina que não atende mais e só
    /// some quando o prazo vence.
    pub async fn fechar(self) {
        self.renovacao.abort();
        if let Err(erro) = self
            .roteador
            .remove_port(PortMappingProtocol::UDP, self.externo.port())
            .await
        {
            // Não é fatal: o prazo da VALIDADE fecha sozinho depois.
            tracing::warn!(%erro, porta = self.externo.port(), "o roteador não devolveu a porta");
        } else {
            tracing::info!(porta = self.externo.port(), "porta devolvida ao roteador");
        }
    }
}

impl Drop for PortaAberta {
    fn drop(&mut self) {
        self.renovacao.abort();
    }
}

/// Pede ao roteador que abra `porta` e a encaminhe para esta máquina.
///
/// # Qual endereço vai no pedido, e por que não é o da rota padrão
///
/// O roteador precisa de um endereço **na rede dele** para encaminhar. Antes
/// isto recebia um `SocketAddr` pronto, montado com o endereço da rota padrão —
/// e com uma VPN ligada esse endereço é o do túnel. O roteador aceitava, abria
/// a porta, e encaminhava para um endereço que não existe na LAN: mais um
/// sucesso mentiroso, da mesma família do CGNAT que este módulo já vigia.
///
/// Agora entram todos os endereços da máquina ([`super::interfaces`]) e a
/// escolha é feita **depois** de o roteador responder, contra a sub-rede dele —
/// que é a pergunta certa e tem resposta exata, sem heurística nenhuma.
///
/// # Errors
///
/// [`FalhaAoAbrir`], sempre dizendo qual dos casos foi. Ver o cabeçalho do
/// módulo: falhar em silêncio é o defeito que isto existe para não ter.
pub async fn abrir(
    candidatos: &[super::interfaces::Achado],
    porta: u16,
) -> Result<PortaAberta, FalhaAoAbrir> {
    let opcoes = SearchOptions {
        timeout: Some(PROCURA),
        single_search_timeout: Some(PROCURA),
        ..SearchOptions::default()
    };
    let roteador = igd_next::aio::tokio::search_gateway(opcoes)
        .await
        .map_err(|erro| {
            tracing::info!(%erro, "nenhum roteador respondeu ao pedido de porta");
            FalhaAoAbrir::NenhumRoteador
        })?;

    // Antes de mapear, e de propósito. Atrás de CGNAT o mapeamento **dá certo**
    // e não serve para nada; perguntar primeiro é o que transforma um sucesso
    // mentiroso numa frase que a pessoa pode ler.
    let externo = roteador
        .get_external_ip()
        .await
        .map_err(|erro| FalhaAoAbrir::SemEnderecoExterno(erro.to_string()))?;
    if !global(externo) {
        return Err(FalhaAoAbrir::SemSaidaParaInternet { externo });
    }

    let endereco_do_roteador = roteador.addr.ip();
    let interno = escolher_interno(candidatos, endereco_do_roteador).ok_or({
        FalhaAoAbrir::SemEnderecoNaRedeDoRoteador {
            roteador: endereco_do_roteador,
        }
    })?;
    let interno = SocketAddr::new(interno, porta);

    let porta = mapear(&roteador, interno).await?;
    let externo = SocketAddr::new(externo, porta);
    tracing::info!(%externo, %interno, "o roteador abriu a porta");

    let renovacao = tokio::spawn(renovar(roteador.clone(), interno, porta));

    Ok(PortaAberta {
        roteador,
        externo,
        renovacao,
    })
}

/// Pede o mapeamento, contornando as duas recusas que são só uma negociação.
///
/// Roteadores domésticos discordam entre si sobre o que aceitam, e duas dessas
/// discordâncias não são falhas: um roteador que só faz mapeamento permanente e
/// um roteador cuja porta pedida já está tomada. Tratar as duas como erro
/// devolveria "não deu" para casas em que dá.
async fn mapear(roteador: &Gateway<Tokio>, interno: SocketAddr) -> Result<u16, FalhaAoAbrir> {
    let desejada = interno.port();
    let validade = u32::try_from(VALIDADE.as_secs()).unwrap_or(u32::MAX);

    match roteador
        .add_port(
            PortMappingProtocol::UDP,
            desejada,
            interno,
            validade,
            DESCRICAO,
        )
        .await
    {
        Ok(()) => Ok(desejada),

        // Roteador que só faz mapeamento permanente. Aceitamos, e o preço é a
        // regra sobreviver a uma queda feia — ver VALIDADE.
        Err(igd_next::AddPortError::OnlyPermanentLeasesSupported) => {
            tracing::info!("o roteador só faz mapeamento permanente; pedindo sem prazo");
            roteador
                .add_port(PortMappingProtocol::UDP, desejada, interno, 0, DESCRICAO)
                .await
                .map(|()| desejada)
                .map_err(|erro| FalhaAoAbrir::RoteadorRecusou(erro.to_string()))
        }

        // A porta pedida já está tomada, ou o roteador exige que a externa seja
        // igual à interna e não pode. Qualquer porta serve: o `seele://` carrega
        // a porta, então quem receber o link não precisa saber disto.
        Err(
            erro @ (igd_next::AddPortError::PortInUse
            | igd_next::AddPortError::SamePortValuesRequired),
        ) => {
            tracing::info!(%erro, "a porta pedida não deu; pedindo qualquer uma");
            roteador
                .add_any_port(PortMappingProtocol::UDP, interno, validade, DESCRICAO)
                .await
                .map_err(|erro| FalhaAoAbrir::RoteadorRecusou(erro.to_string()))
        }

        Err(erro) => Err(FalhaAoAbrir::RoteadorRecusou(erro.to_string())),
    }
}

/// Mantém o mapeamento vivo enquanto o Dogma estiver de pé.
///
/// Sem isto a porta fecha no meio de uma conversa, uma hora depois de abrir, e
/// o sintoma é a pior coisa possível: funcionou, e parou de funcionar sem
/// ninguém mexer em nada.
///
/// Uma renovação perdida não é fatal e não interrompe o laço — a [`RENOVACAO`]
/// deixa margem para várias antes de a [`VALIDADE`] acabar.
async fn renovar(roteador: Gateway<Tokio>, interno: SocketAddr, porta: u16) {
    let validade = u32::try_from(VALIDADE.as_secs()).unwrap_or(u32::MAX);
    let mut relogio = tokio::time::interval(RENOVACAO);
    // O primeiro tique sai na hora; este é o que renova, não o que abriu.
    relogio.tick().await;
    loop {
        relogio.tick().await;
        match roteador
            .add_port(
                PortMappingProtocol::UDP,
                porta,
                interno,
                validade,
                DESCRICAO,
            )
            .await
        {
            Ok(()) => tracing::debug!(porta, "mapeamento renovado"),
            Err(erro) => tracing::warn!(%erro, porta, "não deu para renovar o mapeamento"),
        }
    }
}

/// Qual endereço desta máquina o roteador consegue alcançar.
///
/// A conta é exata quando a enumeração trouxe a máscara: o roteador encaminha
/// para dentro da rede dele, e só um endereço na sub-rede dele serve. Quando
/// não há máscara — o recuo para o truque da rota padrão —, sobra o primeiro
/// endereço de placa de rede na faixa privada, que é o que aquele truque teria
/// devolvido de qualquer forma.
///
/// Nunca um endereço de túnel: é exatamente o que fazia o roteador encaminhar a
/// porta para lugar nenhum.
fn escolher_interno(candidatos: &[super::interfaces::Achado], roteador: IpAddr) -> Option<IpAddr> {
    use super::interfaces::Origem;

    candidatos
        .iter()
        .find(|achado| achado.na_mesma_rede(roteador))
        .or_else(|| {
            candidatos.iter().find(|achado| {
                achado.ip.is_ipv4() && achado.classe() == Origem::Fisica && achado.e_da_rede_local()
            })
        })
        .map(|achado| achado.ip)
}

/// Se este endereço é um em que alguém de fora conseguiria bater.
fn global(endereco: IpAddr) -> bool {
    match endereco {
        IpAddr::V4(quatro) => global_v4(quatro),
        IpAddr::V6(seis) => super::global_v6(seis),
    }
}

/// Se este IPv4 sai para a internet.
///
/// `Ipv4Addr::is_global` ainda não é estável, e o MSRV do ADR 0011 não espera
/// por ele. A faixa que importa aqui e que não tem método na std é a
/// `100.64.0.0/10`: a RFC 6598 a reservou para CGNAT, e é exatamente o caso que
/// o ADR 0022 diz não ter saída antes do degrau 4.
pub(super) fn global_v4(endereco: Ipv4Addr) -> bool {
    let [a, b, _, _] = endereco.octets();
    // 100.64.0.0/10 — CGNAT.
    let cgnat = a == 100 && (64..128).contains(&b);
    !endereco.is_private()
        && !endereco.is_loopback()
        && !endereco.is_link_local()
        && !endereco.is_broadcast()
        && !endereco.is_documentation()
        && !endereco.is_unspecified()
        && !cgnat
        // 0.0.0.0/8 e 240.0.0.0/4 (reservado) não são endereços de host.
        && a != 0
        && a < 240
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_faixa_de_cgnat_nao_conta_como_saida() {
        // A descoberta que este módulo existe para não repetir: atrás de CGNAT
        // o roteador **abre a porta** e devolve sucesso. Se `100.64.0.0/10`
        // passar por endereço global aqui, o Dogma monta um `seele://` com um
        // endereço da operadora, manda para o amigo, e ninguém nunca descobre
        // por que "não conecta".
        for texto in [
            "100.64.0.1",
            "100.100.50.3",
            "100.127.255.254",
            "100.64.0.0",
            "100.127.255.255",
        ] {
            let endereco: Ipv4Addr = texto.parse().expect("endereço de teste inválido");
            assert!(!global_v4(endereco), "CGNAT passou por global: {texto}");
        }
        // E as bordas de fora da faixa continuam globais — /10 e não /8.
        for texto in ["100.63.255.255", "100.128.0.0", "100.0.0.1"] {
            let endereco: Ipv4Addr = texto.parse().expect("endereço de teste inválido");
            assert!(global_v4(endereco), "a faixa comeu demais: {texto}");
        }
    }

    #[test]
    fn as_faixas_privadas_tambem_nao_contam() {
        for texto in [
            "192.168.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "0.0.0.0",
            "255.255.255.255",
        ] {
            let endereco: Ipv4Addr = texto.parse().expect("endereço de teste inválido");
            assert!(!global_v4(endereco), "passou por global: {texto}");
        }
    }

    #[test]
    fn um_endereco_publico_de_verdade_conta() {
        for texto in ["8.8.8.8", "1.1.1.1", "200.160.2.3"] {
            let endereco: Ipv4Addr = texto.parse().expect("endereço de teste inválido");
            assert!(global_v4(endereco), "recusou um endereço público: {texto}");
        }
    }

    #[test]
    fn a_renovacao_cabe_com_folga_dentro_da_validade() {
        // Renovar depois do prazo vencer é não renovar. A folga é para caber
        // várias tentativas perdidas: renovação é pedido pela rede, e a rede
        // aqui é justamente a que está com problema quando isto importa.
        const { assert!(RENOVACAO.as_secs() * 2 < VALIDADE.as_secs()) };
        // E o prazo não é "para sempre": um mapeamento permanente sobrevive a
        // um `kill -9` e fica apontando para ninguém.
        const { assert!(VALIDADE.as_secs() > 0) };
    }

    #[test]
    fn a_procura_nao_faz_ninguem_esperar_muito() {
        // Isto roda entre apertar HOSPEDAR AQUI e a sala abrir, e numa rede sem
        // UPnP a busca sempre esgota o prazo inteiro. O padrão do igd-next é
        // 10 s, que é tempo demais para uma pergunta cuja resposta mais comum
        // em rede ruim é "não".
        const { assert!(PROCURA.as_secs() <= 5) };
    }

    #[test]
    fn toda_falha_diz_o_que_aconteceu_sem_repetir_a_anterior() {
        // As variantes existem porque as frases são diferentes; se duas
        // dissessem a mesma coisa, seriam uma. E nenhuma pode sair vazia: uma
        // falha sem frase é o silêncio de volta, com mais passos.
        let falhas = [
            FalhaAoAbrir::NenhumRoteador,
            FalhaAoAbrir::RoteadorRecusou("ActionNotAuthorized".into()),
            FalhaAoAbrir::SemSaidaParaInternet {
                externo: IpAddr::from([100, 64, 0, 1]),
            },
            FalhaAoAbrir::SemEnderecoExterno("tempo esgotado".into()),
        ];
        let frases: Vec<String> = falhas.iter().map(ToString::to_string).collect();
        for frase in &frases {
            assert!(frase.len() > 20, "falha sem frase de verdade: {frase}");
        }
        for (indice, frase) in frases.iter().enumerate() {
            for outra in frases.iter().skip(indice + 1) {
                assert_ne!(frase, outra, "duas falhas dizem a mesma coisa");
            }
        }
        // A do CGNAT tem de carregar o endereço: é a informação que permite a
        // quem hospeda entender que o problema não está na máquina dele.
        assert!(
            frases
                .get(2)
                .is_some_and(|frase| frase.contains("100.64.0.1")),
            "a falha de CGNAT não diz qual era o endereço"
        );
    }

    /// A máquina do relato: WARP com a rota padrão, Ethernet com a rede de
    /// casa. Os dois endereços IPv4 são privados — é o que torna a faixa
    /// inútil aqui e a sub-rede do roteador a única pergunta que decide.
    fn a_maquina_do_relato() -> Vec<super::super::interfaces::Achado> {
        use super::super::interfaces::{Achado, Origem};
        vec![
            Achado {
                ip: IpAddr::from([172, 16, 0, 2]),
                mascara: Some(IpAddr::from([255, 255, 255, 255])),
                origem: Origem::Tunel,
            },
            Achado {
                ip: IpAddr::from([192, 168, 0, 30]),
                mascara: Some(IpAddr::from([255, 255, 255, 0])),
                origem: Origem::Fisica,
            },
        ]
    }

    #[test]
    fn o_roteador_recebe_o_endereco_da_rede_dele_e_nao_o_do_tunel() {
        // O defeito de campo no degrau 3: com a VPN ligada, o endereço pedido
        // ao roteador era o do túnel. O roteador dizia `Ok` e encaminhava a
        // porta para um endereço que não existe na rede dele.
        let escolhido = escolher_interno(&a_maquina_do_relato(), IpAddr::from([192, 168, 0, 1]));
        assert_eq!(escolhido, Some(IpAddr::from([192, 168, 0, 30])));
    }

    #[test]
    fn sem_nenhum_endereco_na_rede_do_roteador_o_pedido_nao_e_feito() {
        // Só o túnel. Não há para onde encaminhar, e inventar um endereço aqui
        // seria abrir uma porta que não leva a lugar nenhum — com sucesso.
        let so_tunel: Vec<_> = a_maquina_do_relato()
            .into_iter()
            .filter(|achado| achado.classe() == super::super::interfaces::Origem::Tunel)
            .collect();
        assert_eq!(
            escolher_interno(&so_tunel, IpAddr::from([192, 168, 0, 1])),
            None
        );
    }

    #[test]
    fn sem_mascara_sobra_a_placa_de_rede_na_faixa_privada() {
        // O recuo, para quando a enumeração de interfaces não existe: é o
        // mesmo endereço que o truque da rota padrão devolveria, e nunca o do
        // túnel.
        use super::super::interfaces::{Achado, Origem};
        let candidatos = [
            Achado {
                ip: IpAddr::from([172, 16, 0, 2]),
                mascara: None,
                origem: Origem::Tunel,
            },
            Achado {
                ip: IpAddr::from([192, 168, 0, 30]),
                mascara: None,
                origem: Origem::Fisica,
            },
        ];
        assert_eq!(
            escolher_interno(&candidatos, IpAddr::from([192, 168, 0, 1])),
            Some(IpAddr::from([192, 168, 0, 30]))
        );
    }

    #[tokio::test]
    async fn pedir_porta_numa_rede_sem_roteador_falha_dizendo_isso() {
        // O teste que quase não pode existir: precisa de rede de verdade. Então
        // ele afirma só o que vale nas duas situações — que a resposta chega
        // dentro do prazo e que, se for falha, é uma falha **nomeada**. Nunca
        // "passou calado".
        let candidatos = [super::super::interfaces::Achado {
            ip: IpAddr::from([192, 0, 2, 10]),
            mascara: Some(IpAddr::from([255, 255, 255, 0])),
            origem: super::super::interfaces::Origem::Fisica,
        }];
        let prazo = PROCURA + Duration::from_secs(5);

        let Ok(resultado) = tokio::time::timeout(prazo, abrir(&candidatos, 8383)).await else {
            panic!("o pedido de porta não voltou em {prazo:?}, e travaria quem apertou HOSPEDAR");
        };

        match resultado {
            Err(falha) => {
                eprintln!("pulado (parcialmente): esta rede respondeu «{falha}»");
                assert!(
                    !falha.to_string().is_empty(),
                    "a falha não diz o que aconteceu"
                );
            }
            Ok(porta) => {
                // Há um roteador com UPnP nesta rede, e ele aceitou. Nesse caso
                // o mínimo que se pode exigir é que o endereço devolvido seja
                // utilizável — se não for, mapeamos para nada.
                assert!(
                    global(porta.externo().ip()),
                    "abriu a porta num endereço que não sai daqui: {}",
                    porta.externo()
                );
                assert_ne!(porta.externo().port(), 0, "abriu a porta zero");
                porta.fechar().await;
            }
        }
    }
}
