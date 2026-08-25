//! Degrau 2 do ADR 0022: pedir ao roteador que **deixe entrar** no IPv6.
//!
//! Este módulo é irmão de [`super::porta`] e faz a mesma coisa numa família
//! diferente. Lá o roteador é convencido a **encaminhar** uma porta IPv4 que o
//! NAT esconde; aqui não há NAT nenhum a desfazer — o endereço já é roteável — e
//! o que existe é **entrada bloqueada**. O verbo que falta é "abre o firewall
//! para esta porta", e quem o tem é o PCP, a RFC 6887.
//!
//! # O sintoma, medido em 2026-08-24, e a prova com controle
//!
//! Uma casa com IPv6 global publica os endereços IPv6 dela no `seele://` e
//! ninguém de fora entra por eles. Três pacotes UDP saíram de uma VPS para o
//! IPv6 do PC na porta 8384, com o firewall do Windows já liberando aquele
//! executável: **nenhum chegou**. Os mesmos três, de um Mac na mesma casa, para
//! o mesmo endereço e a mesma porta: **chegaram**. A escuta funciona, o firewall
//! da máquina deixa passar, o endereço está certo. Quem bloqueia é o roteador, e
//! ele vem assim de fábrica.
//!
//! O anfitrião não tinha como saber disso: está escrito no próprio
//! [`super::Degrau::Ipv6Direto`] — «se o firewall do roteador deixar entrar, e
//! isso não dá para saber daqui». Este módulo é a tentativa de trocar aquela
//! frase por um pedido.
//!
//! # O `Ok` do PCP **não é prova**, e isto não pode ser esquecido
//!
//! É a armadilha que o degrau 3 já pisou numa forma parecida: lá um roteador
//! pendurado noutro roteador respondia `Ok` e abria a porta na WAN dele, que não
//! sai para a internet — é por isso que existe
//! [`super::porta::FalhaAoAbrir::SemSaidaParaInternet`]. O equivalente em IPv6 é
//! **um roteador que aceita o pedido e não é quem filtra**: o PCP do roteador de
//! casa responde `SUCCESS` e quem barra a entrada é a caixa da operadora acima
//! dele. Não há como distinguir isso daqui.
//!
//! Então o que este módulo sabe, depois de um `Ok`, é exatamente isto: **alguém
//! que fala PCP disse que criou a regra**. A única prova de que o firewall
//! abriu é um pacote entrando de fora, e isso exige uma sonda externa — a VPS do
//! ponto de encontro serve, pelo método com controle descrito acima — que este
//! ciclo **não** constrói.
//!
//! Por isso o `Ok` é usado só para **promover** o candidato IPv6 global na
//! ordem da lista, e não para nomear degrau nenhum. É uma aposta melhor
//! fundamentada que a de hoje e barata de errar, porque a espera por candidato
//! já foi encurtada. Ver [`super::Tipo::GlobalLiberado`], onde está escrito por
//! que ela fica **abaixo** do endereço refletido: aquele é um terceiro tendo
//! **observado** um pacote chegar, e observação continua vencendo afirmação.
//!
//! # A peça que o PCP não tem: descobrir o roteador
//!
//! O UPnP acha o roteador sozinho, por multicast SSDP. O PCP **não tem
//! descoberta**: a RFC 6887 manda o cliente falar com o gateway padrão, e ler a
//! tabela de rotas é código por sistema. Isso mora inteiro em
//! [`roteador_padrao`], num ponto só, para que o dia em que alguém quiser trocar
//! o `netdev` por outra coisa seja um dia fácil.
//!
//! # O custo em crates, medido e não lembrado
//!
//! Contra a árvore que este workspace já tinha, com
//! `netdev = { default-features = false, features = ["gateway"] }`:
//!
//! | Alvo | Crates novos no workspace | Crates novos na árvore do daemon |
//! |---|---|---|
//! | `aarch64-apple-darwin` | 8 | 20 |
//! | `x86_64-unknown-linux-gnu` | 9 | 16 |
//! | `x86_64-pc-windows-msvc` | 7 | 12 |
//!
//! As duas colunas são a mesma pergunta feita em dois lugares e as duas
//! importam. A primeira é a conta que [`super::porta`] usou para recusar o
//! `portmapper` ("contra a árvore que este workspace já tem"), e ela é pequena
//! no macOS porque o Tauri já carregava `objc2-core-foundation`, `plist` e
//! `ipnet`. A segunda é a que `specs/04-servidor-seele.md` cobra, porque o
//! daemon **não** linka o Tauri e tem de caber em 1 vCPU e 512 MB: ali os mesmos
//! crates aparecem como novos.
//!
//! Os `objc2-core-wlan` e `objc2-security` que fizeram o `portmapper` custar 31
//! crates **não** entram: as features `apple-wifi-extra` e
//! `apple-system-configuration-extra` deixaram de ser padrão na 0.46, e é o
//! `default-features = false` que as mantém desligadas. Medido com `cargo tree`
//! comparado contra o `Cargo.lock` anterior, em 2026-08-24, num macOS.
//!
//! `crab_nat` sai quase de graça: `bytes`, `displaydoc`, `rand`, `thiserror`,
//! `tokio` e `tracing` já estavam aqui. O que ele acrescenta de próprio é o
//! `num_enum` e a cadeia de proc-macro dele — `num_enum_derive`,
//! `proc-macro-crate`, `toml_edit` e companhia —, que é código de **tempo de
//! compilação** e não entra no binário que roda em 512 MB.
//!
//! # Por que `pcp::port_mapping` e não `PortMapping::new`
//!
//! `PortMapping::new` tenta PCP e **recua para NAT-PMP** quando o roteador
//! responde que não conhece a versão 2. Aquele recuo é veneno aqui: a RFC 6886 é
//! IPv4 e só IPv4, e não tem verbo nenhum para firewall IPv6. Um `Ok` vindo dele
//! seria um mapeamento IPv4 — o trabalho que o degrau 3 já faz — devolvido como
//! se fosse a abertura do firewall que se pediu. Sucesso mentiroso, que é o
//! defeito que o módulo vizinho inteiro existe para não ter.
//!
//! Então o PCP é chamado direto, e "o roteador só fala NAT-PMP" vira uma falha
//! com nome: [`FalhaAoLiberar::SoFalaNatPmp`].

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::Duration;

use crab_nat::{
    pcp, GatewayAddress, InternetProtocol, PortMapping, PortMappingOptions, PortMappingType,
    TimeoutConfig, VersionCode,
};

/// Quanto tempo de validade o buraco pede ao roteador.
///
/// Uma hora, e não "para sempre", pelo mesmo motivo que o `VALIDADE` de
/// [`super::porta`] tem prazo: um buraco permanente sobrevive ao processo que o pediu. Se o Dogma
/// morrer de mau jeito — queda de energia, `kill -9`, pânico — o firewall fica
/// aberto apontando para uma máquina que não atende mais, e ninguém nunca mais o
/// fecha. Com prazo, o pior caso se conserta sozinho em uma hora.
///
/// A RFC 6887 §15 não fixa um número e manda o cliente pedir "o menor que sirva";
/// uma hora é o que o vizinho já usa, e usar dois números diferentes para a mesma
/// decisão seria inventar diferença onde não há.
const VALIDADE: Duration = Duration::from_secs(3600);

/// De quanto em quanto tempo o buraco é renovado.
///
/// Bem antes da metade da [`VALIDADE`], de propósito e pelo mesmo motivo do
/// degrau 3: renovar é um pedido pela rede e pode falhar. Com esta margem cabem
/// várias tentativas perdidas antes de o firewall realmente voltar a fechar
/// debaixo de uma conversa em andamento.
const RENOVACAO: Duration = Duration::from_secs(1200);

/// Quanto tempo o pedido inteiro pode levar.
///
/// Três segundos, o mesmo teto do `PROCURA` de [`super::porta`], e pelo mesmo
/// motivo:
/// isto roda entre apertar **HOSPEDAR AQUI** e a sala abrir. Numa rede cujo
/// roteador não fala PCP o pedido sempre esgota o prazo inteiro, então este
/// número é literalmente quanto a pessoa espera à toa no pior caso.
///
/// O padrão do `crab_nat` não serve: ele começa em 3 s e dobra por três
/// tentativas, o que dá até 45 s. A RFC 6887 §8.1.1 pede esses 3 s pensando em
/// rede larga; aqui o destino está no mesmo cabo, e um roteador doméstico que
/// não respondeu em um segundo não vai responder.
const PRAZO: Duration = Duration::from_secs(3);

/// Quanto se espera pela primeira resposta antes de repetir o pedido.
const PRIMEIRA_ESPERA: Duration = Duration::from_secs(1);

/// Quantas repetições depois da primeira tentativa.
///
/// Uma. O `crab_nat` dobra a espera a cada repetição, então uma repetição custa
/// `PRIMEIRA_ESPERA * 2` e o total cabe no [`PRAZO`] — que é o que o teste
/// `testes::a_espera_toda_cabe_no_prazo` confere, sem rede nenhuma.
const REPETICOES: usize = 1;

/// Por que não deu para pedir que o firewall abrisse.
///
/// Cada variante é uma frase diferente para quem hospeda, e é por isso que são
/// variantes e não uma string — o mesmo critério de
/// [`super::porta::FalhaAoAbrir`]: as coisas que a pessoa pode fazer a respeito
/// são diferentes. "Esta máquina não tem IPv6" e "o roteador recusou" mandam a
/// pessoa para lugares opostos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaAoLiberar {
    /// Esta máquina não tem endereço IPv6 global numa placa de rede.
    ///
    /// Não há o que abrir: o degrau 2 nunca chegou a existir aqui. É o caso de
    /// uma casa cuja operadora ainda não entrega IPv6, e a resposta não está no
    /// roteador — está no plano.
    ///
    /// Endereço de túnel não conta, pelo mesmo motivo do degrau 3: um IPv6 de
    /// VPN passa por unicast global e não aceita entrada nenhuma. Foi o relato
    /// que produziu [`super::Degrau::RedeLocalOuVpn`].
    SemIpv6Global,
    /// Não deu para descobrir o roteador desta rede.
    ///
    /// O PCP não tem descoberta — ver o cabeçalho do módulo —, então sem o
    /// gateway padrão não há a quem perguntar. Acontece quando a máquina não tem
    /// rota IPv6 padrão, ou quando o sistema não deixa lê-la.
    RoteadorNaoDescoberto,
    /// Não deu nem para mandar o pedido.
    ///
    /// Diferente de "não respondeu": aqui o socket falhou antes disso — sem rota
    /// até o gateway, ou o sistema recusou o envio. Vale a pena separar porque o
    /// problema está no caminho até o roteador e não no que ele pensa do pedido.
    NaoDeuParaFalarComORoteador {
        /// A quem se tentou falar.
        roteador: IpAddr,
        /// O que o sistema disse.
        erro: String,
    },
    /// O roteador não respondeu nada dentro do [`PRAZO`].
    ///
    /// É o caso comum e o mais provável: o roteador não fala PCP, ou fala e está
    /// desligado nas configurações. Nada distingue os dois daqui, e prometer que
    /// distingue seria pior.
    RoteadorNaoFalaPcp {
        /// A quem se perguntou.
        roteador: IpAddr,
    },
    /// O roteador respondeu que só conhece o NAT-PMP, a versão anterior.
    ///
    /// Variante própria porque a resposta é diferente das outras: **este
    /// roteador tem o mecanismo**, só numa versão que é IPv4 e só IPv4 (RFC
    /// 6886) e portanto não tem verbo nenhum para firewall IPv6. Atualizar o
    /// firmware, ou ligar o PCP se ele estiver na tela, resolveria — e nas
    /// outras variantes não resolveria nada.
    SoFalaNatPmp {
        /// Quem respondeu.
        roteador: IpAddr,
    },
    /// O roteador respondeu recusando, e disse por quê.
    ///
    /// O código é o da RFC 6887 §7.4, carregado inteiro: quem hospeda pode
    /// procurá-lo, e quem lê um relatório de campo consegue distinguir
    /// «`NOT_AUTHORIZED`, o PCP está ligado mas fechado para este cliente» de
    /// «`NO_RESOURCES`, a tabela do roteador encheu».
    RoteadorRecusou {
        /// Quem recusou.
        roteador: IpAddr,
        /// O número do código, tal como a RFC 6887 §7.4 o define.
        codigo: u8,
        /// O nome do código na RFC.
        nome: &'static str,
    },
    /// O roteador exige que o pedido **venha** do endereço que se quer abrir.
    ///
    /// É o `ADDRESS_MISMATCH`, código 12, e a definição dele na RFC 6887 §7.4 é
    /// literalmente esta: «o endereço de origem do pacote de pedido não bate com
    /// o conteúdo do campo do endereço IP do cliente PCP». Abrir o firewall para
    /// um endereço a partir de outro só é possível com a opção `THIRD_PARTY`,
    /// que o `crab_nat` não implementa.
    ///
    /// Isto acontece quando o gateway só é conhecido pelo endereço link-local
    /// (`fe80::`), que é o caso normal numa casa: o sistema escolhe uma origem
    /// link-local para um destino link-local (RFC 6724), e aí a origem não é o
    /// IPv6 global que se está pedindo para abrir.
    ///
    /// **Variante própria porque o limite está aqui e não no roteador.** A frase
    /// não pode mandar a pessoa mexer em nada: o roteador está certo, e quem
    /// teria de mudar é este código — amarrando o socket ao endereço global
    /// antes de enviar, que é coisa que o `crab_nat` hoje não deixa fazer.
    EnderecoNaoBate {
        /// Quem recusou.
        roteador: IpAddr,
        /// O endereço que se pediu para abrir.
        pedido: Ipv6Addr,
    },
    /// O roteador disse `Ok` e devolveu outro endereço ou outra porta.
    ///
    /// Num firewall IPv6 sem NAT o par externo devolvido tem de ser **o mesmo**
    /// que o interno: não há tradução a fazer, só uma regra a criar. Quando ele
    /// vem diferente, o roteador fez outra coisa — NAT64, NPTv6, um mapeamento
    /// IPv4 —, e o endereço que este Dogma anuncia no `seele://` não é o que foi
    /// aberto.
    ///
    /// Tratar isso como sucesso seria promover na lista um candidato que o
    /// próprio roteador acabou de dizer que não é o aberto. É a mesma família do
    /// CGNAT do degrau 3: um `Ok` que não serve para nada.
    NaoFoiBuracoNoFirewall {
        /// O que se pediu para abrir.
        pedido: SocketAddr,
        /// O que o roteador diz ter aberto.
        devolvido: SocketAddr,
    },
}

impl std::fmt::Display for FalhaAoLiberar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SemIpv6Global => write!(
                f,
                "esta máquina não tem endereço IPv6 próprio, então não há \
                 firewall IPv6 a abrir — quem entrega IPv6 é a operadora"
            ),
            Self::RoteadorNaoDescoberto => write!(
                f,
                "não deu para descobrir o roteador desta rede, e o PCP só sabe \
                 falar com o gateway padrão"
            ),
            Self::NaoDeuParaFalarComORoteador { roteador, erro } => write!(
                f,
                "não deu nem para mandar o pedido ao roteador ({roteador}): {erro}"
            ),
            Self::RoteadorNaoFalaPcp { roteador } => write!(
                f,
                "o roteador ({roteador}) não respondeu ao pedido de abrir o \
                 firewall IPv6 — ele não fala PCP, ou o PCP está desligado nele"
            ),
            Self::SoFalaNatPmp { roteador } => write!(
                f,
                "o roteador ({roteador}) só fala o NAT-PMP antigo, que é de IPv4 \
                 e não sabe abrir firewall IPv6 — um firmware mais novo saberia"
            ),
            Self::RoteadorRecusou {
                roteador,
                codigo,
                nome,
            } => write!(
                f,
                "o roteador ({roteador}) recusou abrir o firewall IPv6: {nome} \
                 (código {codigo} da RFC 6887)"
            ),
            Self::EnderecoNaoBate { roteador, pedido } => write!(
                f,
                "o roteador ({roteador}) exige que o pedido venha do próprio \
                 {pedido}, e o SEELE ainda não sabe mandar de lá — não há nada a \
                 mexer no roteador"
            ),
            Self::NaoFoiBuracoNoFirewall { pedido, devolvido } => write!(
                f,
                "pedimos {pedido} e o roteador devolveu {devolvido}: isso é \
                 tradução de endereço, e não abertura de firewall"
            ),
        }
    }
}

impl std::error::Error for FalhaAoLiberar {}

/// Um buraco no firewall do roteador, e a tarefa que o mantém aberto.
///
/// Descartar isto **não** fecha o buraco no roteador: `Drop` não pode esperar, e
/// devolver o buraco é um pedido pela rede. O que o `Drop` faz é parar de
/// renovar, e aí o prazo da [`VALIDADE`] fecha sozinho. Para devolver na hora,
/// [`BuracoAberto::fechar`] — o mesmo desenho de [`super::porta::PortaAberta`], e
/// pelo mesmo motivo.
pub struct BuracoAberto {
    liberado: SocketAddr,
    /// Compartilhado com a tarefa de renovação porque `PortMapping::renew` pede
    /// `&mut`: quem renova precisa escrever no mapeamento, e quem fecha precisa
    /// lê-lo depois. É o único estado deste módulo com dois donos.
    mapeamento: Arc<tokio::sync::Mutex<PortMapping>>,
    renovacao: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for BuracoAberto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuracoAberto")
            .field("liberado", &self.liberado)
            .finish()
    }
}

impl BuracoAberto {
    /// O endereço que o roteador disse ter liberado.
    ///
    /// **Não é promessa de que alguém de fora chega nele.** Ver o cabeçalho do
    /// módulo: um `Ok` do PCP diz que alguém que fala PCP criou a regra, e não
    /// que é ele quem filtra. Quem usa isto usa para **ordenar** candidatos, e
    /// nunca para afirmar alcance.
    #[must_use]
    pub fn liberado(&self) -> SocketAddr {
        self.liberado
    }

    /// Devolve o buraco ao roteador.
    ///
    /// Consome, e espera. Deixar a regra para trás no roteador é sujeira que
    /// sobrevive ao programa: um buraco no firewall apontando para uma máquina
    /// que não atende mais, e que só some quando o prazo vence.
    pub async fn fechar(self) {
        self.renovacao.abort();
        // Clonado, e não movido: o mapeamento tem dois donos enquanto a
        // renovação existe. Tudo o que o `try_drop` precisa — gateway, cliente,
        // nonce, porta interna — é estável entre renovações, então a cópia
        // fecha o mesmo buraco que a original abriu.
        let mapeamento = self.mapeamento.lock().await.clone();
        match mapeamento.try_drop().await {
            Ok(()) => {
                tracing::info!(liberado = %self.liberado, "buraco de firewall devolvido ao roteador");
            }
            // Não é fatal: o prazo da VALIDADE fecha sozinho depois.
            Err((erro, _)) => {
                tracing::warn!(%erro, liberado = %self.liberado, "o roteador não fechou o buraco");
            }
        }
    }
}

impl Drop for BuracoAberto {
    fn drop(&mut self) {
        self.renovacao.abort();
    }
}

/// Pede ao roteador que deixe entrar em `porta`, no IPv6 global desta máquina.
///
/// # Errors
///
/// [`FalhaAoLiberar`], sempre dizendo qual dos casos foi. Ver o cabeçalho do
/// módulo: falhar em silêncio é o defeito que o degrau 3 existe para não ter, e
/// este não é diferente.
pub async fn liberar(
    candidatos: &[super::interfaces::Achado],
    porta: NonZeroU16,
) -> Result<BuracoAberto, FalhaAoLiberar> {
    let nosso = escolher_ipv6_global(candidatos).ok_or(FalhaAoLiberar::SemIpv6Global)?;
    let roteador = roteador_padrao(nosso).ok_or(FalhaAoLiberar::RoteadorNaoDescoberto)?;
    let endereco_do_roteador = IpAddr::from(roteador);

    let opcoes = PortMappingOptions {
        // A mesma porta de fora e de dentro. Num firewall sem NAT não há outra
        // resposta certa: pedir outra seria pedir tradução, que é justamente o
        // que `NaoFoiBuracoNoFirewall` recusa quando o roteador a devolve.
        external_port: Some(porta),
        lifetime_seconds: Some(validade_em_segundos()),
        timeout_config: Some(TimeoutConfig {
            initial_timeout: PRIMEIRA_ESPERA,
            max_retries: REPETICOES,
            max_retry_timeout: Some(PRAZO),
        }),
    };

    let pedido =
        pcp::BaseMapRequest::new(roteador, IpAddr::V6(nosso), InternetProtocol::Udp, porta);

    // Teto duro por cima do recuo do `crab_nat`. As duas contas deviam bater —
    // e o teste `a_espera_toda_cabe_no_prazo` confere que batem —, mas a que
    // conta para quem está olhando a tela é esta.
    let resposta = tokio::time::timeout(PRAZO, pcp::port_mapping(pedido, None, None, opcoes))
        .await
        .map_err(|_| FalhaAoLiberar::RoteadorNaoFalaPcp {
            roteador: endereco_do_roteador,
        })?;

    let mapeamento = resposta.map_err(|falha| traduzir(falha, endereco_do_roteador, nosso))?;

    // O que se pediu e o que ele diz ter aberto. Conferido **antes** de guardar,
    // pelo mesmo motivo que o degrau 3 pergunta o endereço externo antes de
    // mapear: um `Ok` sobre outro endereço não é o `Ok` que se pediu.
    let liberado = SocketAddr::new(IpAddr::V6(nosso), porta.get());
    let devolvido = SocketAddr::new(
        match mapeamento.mapping_type() {
            PortMappingType::Pcp { external_ip, .. } => external_ip,
            // Inalcançável: `pcp::port_mapping` só constrói mapeamentos PCP, e o
            // recuo para NAT-PMP mora em `PortMapping::new`, que este módulo não
            // chama de propósito. Se isto mudar, cai na conferência abaixo em
            // vez de passar calado.
            PortMappingType::NatPmp => IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        },
        mapeamento.external_port().get(),
    );
    if devolvido != liberado {
        // Já existe no roteador: desfazer é o certo, e não largar.
        if let Err((erro, _)) = mapeamento.try_drop().await {
            tracing::warn!(%erro, "não deu para desfazer um mapeamento que não servia");
        }
        return Err(FalhaAoLiberar::NaoFoiBuracoNoFirewall {
            pedido: liberado,
            devolvido,
        });
    }

    tracing::info!(
        %liberado,
        roteador = %endereco_do_roteador,
        "o roteador disse ter aberto o firewall IPv6 — é o que ele disse, e não uma prova"
    );

    let mapeamento = Arc::new(tokio::sync::Mutex::new(mapeamento));
    let renovacao = tokio::spawn(renovar(Arc::clone(&mapeamento), liberado));

    Ok(BuracoAberto {
        liberado,
        mapeamento,
        renovacao,
    })
}

/// Mantém o buraco vivo enquanto o Dogma estiver de pé.
///
/// Sem isto o firewall fecha no meio de uma conversa, uma hora depois de abrir, e
/// o sintoma é a pior coisa possível: funcionou, e parou de funcionar sem
/// ninguém mexer em nada. Uma renovação perdida não é fatal e não interrompe o
/// laço — a [`RENOVACAO`] deixa margem para várias antes de a [`VALIDADE`]
/// acabar.
async fn renovar(mapeamento: Arc<tokio::sync::Mutex<PortMapping>>, liberado: SocketAddr) {
    let mut relogio = tokio::time::interval(RENOVACAO);
    // O primeiro tique sai na hora; este é o que renova, não o que abriu.
    relogio.tick().await;
    loop {
        relogio.tick().await;
        let resultado = mapeamento.lock().await.renew().await;
        match resultado {
            Ok(()) => tracing::debug!(%liberado, "buraco de firewall renovado"),
            Err(erro) => {
                tracing::warn!(%erro, %liberado, "não deu para renovar o buraco de firewall");
            }
        }
    }
}

/// O IPv6 global desta máquina que vale a pena mandar abrir.
///
/// Só endereço de placa de rede, nunca de túnel, e é a mesma lição que o degrau
/// 3 aprendeu em campo: um IPv6 de VPN passa por unicast global e não aceita
/// entrada nenhuma — foi o que fez a escada declarar o degrau 2 num Windows com
/// WARP e anunciar "alcança de qualquer lugar" para um endereço que não aceita
/// nada. Ver [`super::Degrau::RedeLocalOuVpn`].
///
/// O primeiro serve: quando a máquina tem vários IPv6 globais na mesma placa —
/// o permanente e os temporários da RFC 8981 —, a ordem é a que o próprio
/// sistema deu, que é a ordem em que ele os prefere para sair.
fn escolher_ipv6_global(candidatos: &[super::interfaces::Achado]) -> Option<Ipv6Addr> {
    candidatos.iter().find_map(|achado| match achado.ip {
        IpAddr::V6(seis)
            if super::global_v6(seis) && achado.classe() == super::interfaces::Origem::Fisica =>
        {
            Some(seis)
        }
        _ => None,
    })
}

/// O roteador com quem falar PCP, e **o único ponto deste módulo que fala com o
/// `netdev`**.
///
/// Isolado de propósito: o PCP não descobre o gateway — a RFC 6887 manda o
/// cliente falar com o gateway padrão e para por aí —, a leitura da tabela de
/// rotas é código por sistema, e é a parte mais provável de alguém querer trocar
/// um dia: por um `ioctl` próprio, por um crate menor, ou por um endereço que
/// quem hospeda digitou. Trocar isto tem de ser mexer numa função.
///
/// # Qual roteador, e por que não é simplesmente "o padrão"
///
/// É o gateway **da interface que tem o nosso IPv6 global**, e não o da rota
/// padrão. A diferença é a mesma que [`super::interfaces`] documenta: com uma
/// VPN ligada a rota padrão é a do túnel, e o roteador que interessa é o da
/// placa de rede. Aqui a pergunta tem resposta exata — de que interface veio
/// este endereço — e não precisa de heurística nenhuma.
///
/// # Escopo, e por que ele é obrigatório
///
/// O gateway IPv6 de uma casa é quase sempre um `fe80::`, e um link-local sem
/// número de interface não é endereçável: há um `fe80::1` por cabo. O escopo é o
/// índice da interface, que é o que o sistema espera em `sin6_scope_id`.
///
/// Um gateway **não** link-local é preferido quando existe, e não é preferência
/// estética: o servidor PCP recusa com `ADDRESS_MISMATCH` (RFC 6887 §7.4) o
/// pedido cujo endereço de origem não seja o que se está abrindo, e o sistema só
/// escolhe o IPv6 global como origem se o destino também for global (RFC 6724).
/// Ver [`FalhaAoLiberar::EnderecoNaoBate`], que é o que sobra quando só há
/// link-local — e que é o caso comum numa casa, porque o gateway anunciado por
/// RA é um `fe80::`.
fn roteador_padrao(nosso: Ipv6Addr) -> Option<GatewayAddress> {
    let interfaces = netdev::get_interfaces();
    let interface = interfaces
        .into_iter()
        .find(|interface| interface.ipv6.iter().any(|rede| rede.addr() == nosso))?;
    let gateway = interface.gateway?;

    // Global antes de link-local, pelo motivo escrito acima.
    if let Some(global) = gateway
        .ipv6
        .iter()
        .copied()
        .find(|ip| super::global_v6(*ip))
    {
        return Some(GatewayAddress::IpV6(global, None));
    }
    let primeiro = gateway.ipv6.first().copied()?;
    Some(GatewayAddress::IpV6(primeiro, Some(interface.index)))
}

/// A [`VALIDADE`] no tipo que o protocolo usa: segundos, `u32` (RFC 6887 §7.1).
fn validade_em_segundos() -> u32 {
    u32::try_from(VALIDADE.as_secs()).unwrap_or(u32::MAX)
}

/// Transforma a falha do `crab_nat` numa frase que quem hospeda pode ler.
///
/// O mapeamento dos códigos é o da RFC 6887 §7.4, na ordem em que ela os define
/// — que é a mesma ordem do `pcp::ResultCode`. Carregar o número junto do nome é
/// de propósito: um relatório de campo com «código 2» é procurável, e «não deu»
/// não é.
fn traduzir(falha: pcp::Failure, roteador: IpAddr, nosso: Ipv6Addr) -> FalhaAoLiberar {
    let recusa = |codigo: u8, nome: &'static str| FalhaAoLiberar::RoteadorRecusou {
        roteador,
        codigo,
        nome,
    };
    match falha {
        pcp::Failure::Timeout => FalhaAoLiberar::RoteadorNaoFalaPcp { roteador },
        pcp::Failure::Socket(erro) => FalhaAoLiberar::NaoDeuParaFalarComORoteador {
            roteador,
            erro: erro.to_string(),
        },
        // O roteador conhece a família e não a versão 2. Só o NAT-PMP é recuo
        // possível, e ele não serve para IPv6 — ver o cabeçalho do módulo.
        pcp::Failure::UnsupportedVersion(VersionCode::NatPmp) => {
            FalhaAoLiberar::SoFalaNatPmp { roteador }
        }
        pcp::Failure::UnsupportedVersion(VersionCode::Pcp) => recusa(1, "UNSUPP_VERSION"),
        pcp::Failure::NotAuthorized(_) => recusa(2, "NOT_AUTHORIZED"),
        pcp::Failure::MalformedRequest => recusa(3, "MALFORMED_REQUEST"),
        pcp::Failure::UnsupportedOpcode => recusa(4, "UNSUPP_OPCODE"),
        pcp::Failure::UnsupportedOption => recusa(5, "UNSUPP_OPTION"),
        pcp::Failure::MalformedOption => recusa(6, "MALFORMED_OPTION"),
        pcp::Failure::NetworkFailure(_) => recusa(7, "NETWORK_FAILURE"),
        pcp::Failure::NoResources(_) => recusa(8, "NO_RESOURCES"),
        pcp::Failure::UnsupportedProtocol => recusa(9, "UNSUPP_PROTOCOL"),
        pcp::Failure::UserExceededQuota(_) => recusa(10, "USER_EX_QUOTA"),
        pcp::Failure::CannotProvideExternal(_) => recusa(11, "CANNOT_PROVIDE_EXTERNAL"),
        // Código 12, e o único que ganha variante própria: o limite está deste
        // lado do fio. Ver `FalhaAoLiberar::EnderecoNaoBate`.
        pcp::Failure::AddressMismatch => FalhaAoLiberar::EnderecoNaoBate {
            roteador,
            pedido: nosso,
        },
        pcp::Failure::ExcessiveRemotePeers => recusa(13, "EXCESSIVE_REMOTE_PEERS"),
        // Uma resposta que não é PCP válido, ou um nonce de outra conversa. Não
        // é recusa e não é silêncio: é alguém falando errado na porta 5351.
        pcp::Failure::Nonce => FalhaAoLiberar::NaoDeuParaFalarComORoteador {
            roteador,
            erro: "a resposta veio com o identificador de outra sessão".to_owned(),
        },
        pcp::Failure::InvalidResponse(erro) => FalhaAoLiberar::NaoDeuParaFalarComORoteador {
            roteador,
            erro: erro.to_string(),
        },
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::alcance::interfaces::{Achado, Origem};

    /// Um `Achado` de teste, com máscara ausente porque nenhuma conta deste
    /// módulo depende dela — o degrau 2 não pergunta sub-rede a ninguém.
    fn achado(texto: &str, origem: Origem) -> Achado {
        Achado {
            ip: texto.parse().expect("endereço de teste inválido"),
            mascara: None,
            origem,
        }
    }

    fn porta() -> NonZeroU16 {
        NonZeroU16::new(8383).expect("8383 não é zero")
    }

    #[test]
    fn a_espera_toda_cabe_no_prazo() {
        // A aritmética que dá para conferir sem rede, e que é a única guarda
        // contra o padrão do `crab_nat` voltar por engano: ele começa em 3 s e
        // dobra por três tentativas, o que dá 45 s de gente parada olhando a
        // tela depois de apertar HOSPEDAR AQUI.
        //
        // A soma de uma progressão que dobra `n+1` vezes a partir de `p` é
        // `p * (2^(n+1) - 1)`.
        const TOTAL: u64 = PRIMEIRA_ESPERA.as_secs() * ((1u64 << (REPETICOES as u64 + 1)) - 1);
        const { assert!(TOTAL <= PRAZO.as_secs()) };
        // E o prazo continua sendo curto o bastante para não travar quem
        // apertou o botão, pelo mesmo teto do degrau 3.
        const { assert!(PRAZO.as_secs() <= 5) };
    }

    #[test]
    fn a_renovacao_cabe_com_folga_dentro_da_validade() {
        // Mesma conta do degrau 3, e pelo mesmo motivo: renovar é pedido pela
        // rede e pode falhar, então a folga tem de caber várias tentativas
        // perdidas antes de o firewall voltar a fechar debaixo de uma conversa.
        const { assert!(RENOVACAO.as_secs() * 2 < VALIDADE.as_secs()) };
        // E não é "para sempre": um buraco permanente sobrevive a um `kill -9`
        // e fica aberto apontando para ninguém.
        const { assert!(VALIDADE.as_secs() > 0) };
        // O protocolo fala em segundos num `u32` (RFC 6887 §7.1), e a conversão
        // não pode saturar sem ninguém perceber.
        assert_eq!(validade_em_segundos(), 3600);
    }

    #[test]
    fn toda_falha_diz_o_que_aconteceu_sem_repetir_a_anterior() {
        // As variantes existem porque as frases são diferentes; se duas
        // dissessem a mesma coisa, seriam uma. E nenhuma pode sair vazia: uma
        // falha sem frase é o silêncio de volta, com mais passos.
        let roteador = IpAddr::from([0xfe80, 0, 0, 0, 0, 0, 0, 1]);
        let nosso: Ipv6Addr = "2001:db8::10".parse().expect("endereço de teste inválido");
        let falhas = todas_as_falhas(roteador, nosso);
        let frases: Vec<String> = falhas.iter().map(ToString::to_string).collect();
        for frase in &frases {
            assert!(frase.len() > 20, "falha sem frase de verdade: {frase}");
        }
        for (indice, frase) in frases.iter().enumerate() {
            for outra in frases.iter().skip(indice + 1) {
                assert_ne!(frase, outra, "duas falhas dizem a mesma coisa");
            }
        }
    }

    /// Uma de cada variante, para os testes que afirmam algo sobre todas.
    ///
    /// Escrita à mão de propósito: se alguém acrescentar uma variante, este
    /// arquivo não compila até que ela ganhe frase e entre aqui — que é a única
    /// forma de o teste de "nenhuma diz o mesmo que outra" continuar valendo.
    fn todas_as_falhas(roteador: IpAddr, nosso: Ipv6Addr) -> Vec<FalhaAoLiberar> {
        let uma = |falha: FalhaAoLiberar| match falha {
            FalhaAoLiberar::SemIpv6Global
            | FalhaAoLiberar::RoteadorNaoDescoberto
            | FalhaAoLiberar::NaoDeuParaFalarComORoteador { .. }
            | FalhaAoLiberar::RoteadorNaoFalaPcp { .. }
            | FalhaAoLiberar::SoFalaNatPmp { .. }
            | FalhaAoLiberar::RoteadorRecusou { .. }
            | FalhaAoLiberar::EnderecoNaoBate { .. }
            | FalhaAoLiberar::NaoFoiBuracoNoFirewall { .. } => falha,
        };
        vec![
            uma(FalhaAoLiberar::SemIpv6Global),
            uma(FalhaAoLiberar::RoteadorNaoDescoberto),
            uma(FalhaAoLiberar::NaoDeuParaFalarComORoteador {
                roteador,
                erro: "no route to host".to_owned(),
            }),
            uma(FalhaAoLiberar::RoteadorNaoFalaPcp { roteador }),
            uma(FalhaAoLiberar::SoFalaNatPmp { roteador }),
            uma(FalhaAoLiberar::RoteadorRecusou {
                roteador,
                codigo: 2,
                nome: "NOT_AUTHORIZED",
            }),
            uma(FalhaAoLiberar::EnderecoNaoBate {
                roteador,
                pedido: nosso,
            }),
            uma(FalhaAoLiberar::NaoFoiBuracoNoFirewall {
                pedido: SocketAddr::new(IpAddr::V6(nosso), 8383),
                devolvido: SocketAddr::from(([200, 160, 2, 3], 40000)),
            }),
        ]
    }

    #[test]
    fn a_recusa_carrega_o_codigo_da_rfc_para_quem_for_procurar() {
        // Um relatório de campo com «código 2» é procurável na RFC 6887 §7.4;
        // «não deu» não é. É a mesma exigência que o degrau 3 faz da falha de
        // CGNAT, que tem de carregar o endereço.
        let roteador = IpAddr::from([0xfe80, 0, 0, 0, 0, 0, 0, 1]);
        let frase = FalhaAoLiberar::RoteadorRecusou {
            roteador,
            codigo: 8,
            nome: "NO_RESOURCES",
        }
        .to_string();
        assert!(
            frase.contains("NO_RESOURCES"),
            "sem o nome do código: {frase}"
        );
        assert!(frase.contains('8'), "sem o número do código: {frase}");
        assert!(
            frase.contains("6887"),
            "sem a RFC de onde o número veio: {frase}"
        );
    }

    #[test]
    fn cada_codigo_de_recusa_da_rfc_vira_uma_falha_com_o_numero_certo() {
        // A tradução é uma tabela, e tabela escrita à mão erra. Isto confere os
        // números contra a RFC 6887 §7.4, que os define nesta ordem.
        let roteador = IpAddr::from([0xfe80, 0, 0, 0, 0, 0, 0, 1]);
        let nosso: Ipv6Addr = "2001:db8::10".parse().expect("endereço de teste inválido");
        let casos = [
            (pcp::Failure::NotAuthorized(0), 2u8, "NOT_AUTHORIZED"),
            (pcp::Failure::MalformedRequest, 3, "MALFORMED_REQUEST"),
            (pcp::Failure::UnsupportedOpcode, 4, "UNSUPP_OPCODE"),
            (pcp::Failure::UnsupportedOption, 5, "UNSUPP_OPTION"),
            (pcp::Failure::MalformedOption, 6, "MALFORMED_OPTION"),
            (pcp::Failure::NetworkFailure(0), 7, "NETWORK_FAILURE"),
            (pcp::Failure::NoResources(0), 8, "NO_RESOURCES"),
            (pcp::Failure::UnsupportedProtocol, 9, "UNSUPP_PROTOCOL"),
            (pcp::Failure::UserExceededQuota(0), 10, "USER_EX_QUOTA"),
            (
                pcp::Failure::CannotProvideExternal(0),
                11,
                "CANNOT_PROVIDE_EXTERNAL",
            ),
            (
                pcp::Failure::ExcessiveRemotePeers,
                13,
                "EXCESSIVE_REMOTE_PEERS",
            ),
        ];
        for (falha, codigo, nome) in casos {
            assert_eq!(
                traduzir(falha, roteador, nosso),
                FalhaAoLiberar::RoteadorRecusou {
                    roteador,
                    codigo,
                    nome,
                }
            );
        }
    }

    #[test]
    fn o_codigo_12_nao_e_recusa_do_roteador_e_sim_limite_daqui() {
        // `ADDRESS_MISMATCH` seria uma linha a mais na tabela de recusas, e é a
        // única que não pode ser: a RFC 6887 §7.4 define o código 12 como «o
        // endereço de origem não bate com o campo do cliente», o roteador está
        // certo em recusar, e quem teria de mudar é este código. A frase não pode
        // mandar a pessoa mexer no roteador.
        let roteador = IpAddr::from([0xfe80, 0, 0, 0, 0, 0, 0, 1]);
        let nosso: Ipv6Addr = "2001:db8::10".parse().expect("endereço de teste inválido");
        assert_eq!(
            traduzir(pcp::Failure::AddressMismatch, roteador, nosso),
            FalhaAoLiberar::EnderecoNaoBate {
                roteador,
                pedido: nosso,
            }
        );
    }

    #[test]
    fn o_recuo_para_nat_pmp_e_uma_falha_e_nunca_um_sucesso() {
        // O degrau 3 já faz mapeamento IPv4, e o NAT-PMP não tem verbo nenhum
        // para firewall IPv6 (a RFC 6886 é IPv4 e só). Se isto virasse `Ok`, o
        // Dogma promoveria o candidato IPv6 na lista por causa de um mapeamento
        // IPv4 — sucesso mentiroso, que é o defeito que o módulo vizinho existe
        // para não ter.
        let roteador = IpAddr::from([0xfe80, 0, 0, 0, 0, 0, 0, 1]);
        let nosso: Ipv6Addr = "2001:db8::10".parse().expect("endereço de teste inválido");
        assert_eq!(
            traduzir(
                pcp::Failure::UnsupportedVersion(VersionCode::NatPmp),
                roteador,
                nosso
            ),
            FalhaAoLiberar::SoFalaNatPmp { roteador }
        );
    }

    #[test]
    fn o_endereco_a_abrir_e_o_da_placa_e_nunca_o_do_tunel() {
        // A lição do degrau 3, na outra família: um IPv6 de túnel passa por
        // unicast global e não aceita entrada nenhuma. Pedir para abrir o
        // firewall para ele seria pedir uma regra que não serve para nada — e o
        // roteador de casa nem conhece aquele endereço.
        let candidatos = [
            achado("2001:db8:dead::1", Origem::Tunel),
            achado("2804:14c:bf27::30", Origem::Fisica),
        ];
        assert_eq!(
            escolher_ipv6_global(&candidatos),
            Some(
                "2804:14c:bf27::30"
                    .parse()
                    .expect("endereço de teste inválido")
            )
        );
    }

    #[test]
    fn sem_ipv6_global_de_placa_nao_ha_o_que_pedir() {
        // Só túnel, só link-local, só IPv4: nos três não existe degrau 2, e
        // inventar um pedido aqui seria falar com o roteador sobre um endereço
        // que ninguém alcança.
        let candidatos = [
            achado("2001:db8:dead::1", Origem::Tunel),
            achado("fd00::1", Origem::Fisica),
            achado("192.168.0.30", Origem::Fisica),
        ];
        assert_eq!(escolher_ipv6_global(&candidatos), None);
    }

    #[test]
    fn uma_ponte_de_conteiner_nao_vira_pedido_de_firewall() {
        // `docker0` e afins entram na enumeração porque a lista de nomes é
        // heurística e apagar endereço é pior. Mas um endereço que não sai desta
        // máquina não tem firewall de roteador a abrir.
        let candidatos = [achado("2001:db8:c0::1", Origem::Virtual)];
        assert_eq!(escolher_ipv6_global(&candidatos), None);
    }

    #[tokio::test]
    async fn pedir_para_abrir_o_firewall_sem_roteador_falha_dizendo_isso() {
        // O teste que quase não pode existir: precisa de rede de verdade. Então
        // ele afirma só o que vale nas duas situações — que a resposta chega
        // dentro do prazo e que, se for falha, é uma falha **nomeada**. Nunca
        // "passou calado". É o padrão de
        // `porta::testes::pedir_porta_numa_rede_sem_roteador_falha_dizendo_isso`.
        let candidatos = [achado("2001:db8::10", Origem::Fisica)];
        let prazo = PRAZO + Duration::from_secs(5);

        let Ok(resultado) = tokio::time::timeout(prazo, liberar(&candidatos, porta())).await else {
            panic!(
                "o pedido de firewall não voltou em {prazo:?}, e travaria quem apertou HOSPEDAR"
            );
        };

        match resultado {
            Err(falha) => {
                eprintln!("pulado (parcialmente): esta rede respondeu «{falha}»");
                assert!(
                    !falha.to_string().is_empty(),
                    "a falha não diz o que aconteceu"
                );
            }
            Ok(buraco) => {
                // Um roteador que aceitou abrir para `2001:db8::10`, que é a
                // faixa de documentação da RFC 3849 e não é endereço desta
                // máquina. Se isso acontecer, o mínimo exigível é que ele tenha
                // dito ter aberto **o que se pediu** — a conferência que
                // `NaoFoiBuracoNoFirewall` faz.
                assert_eq!(buraco.liberado().port(), porta().get());
                buraco.fechar().await;
            }
        }
    }

    #[tokio::test]
    async fn a_maquina_que_roda_o_teste_diz_qual_e_o_roteador_dela() {
        // Não afirma nada: registra. Descobrir o gateway é a peça que o PCP não
        // tem, é a que muda de sistema para sistema, e é a que não dá para
        // encenar sem rede. Deixar o valor na saída do teste é o que permite a
        // quem for depurar um relato de campo saber o que a máquina viu.
        let achados = super::super::interfaces::descobrir();
        match escolher_ipv6_global(&achados) {
            Some(nosso) => match roteador_padrao(nosso) {
                Some(roteador) => eprintln!("IPv6 global {nosso}, roteador {roteador:?}"),
                None => eprintln!("IPv6 global {nosso}, e nenhum roteador IPv6 descoberto"),
            },
            None => eprintln!("esta máquina não tem IPv6 global de placa"),
        }
    }
}
