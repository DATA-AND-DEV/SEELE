//! Subir um servidor dentro de outro programa.
//!
//! O `seeled` existe para quem quer um servidor no ar o tempo todo, numa VPS, sob
//! um supervisor. Este módulo é para o outro caso, que é o mais comum entre
//! amigos: **alguém quer conversar agora e está disposto a ser o anfitrião
//! enquanto a conversa dura.**
//!
//! Sem isto, essa pessoa precisa saber o que é uma linha de comando — e num
//! produto cujo argumento inteiro é "hospede você mesmo", exigir isso de quem
//! hospeda exclui justamente quem mais ganharia. Os dois clientes chamam daqui:
//! `connection --hospedar` e o botão **Hospedar** do app.
//!
//! # O que isto **não** é
//!
//! Não substitui o `seeled`. Um servidor hospedado por um cliente morre quando o
//! cliente fecha, e isso é correto para "estou hospedando uma conversa" e
//! errado para "mantenho um servidor no ar". São dois produtos e continuam sendo
//! dois programas.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;

use crate::persistence::{Location, Persistence};
use crate::{Daemon, ServerConfig};

/// A porta da sala pessoal: a 8385, ao lado da 8383 do servidor guardado e da
/// 8384 do ponto de encontro.
pub const PORTA_DA_SALA: u16 = 8385;

/// A configuração de um servidor guardado, que é o que [`Hospedagem::iniciar`]
/// sobe.
fn config_guardada(
    porta: u16,
    banco: Location,
    nome: &str,
    pasta_dos_mods: Option<crate::RaizesDosMods>,
) -> ServerConfig {
    ServerConfig {
        name: nome.to_owned(),
        // ADR 0045. `None` desliga MODs; quem hospeda pelo app passa a pasta que
        // ele já conhece, e o `seeled` a computa por si.
        mods_dir: pasta_dos_mods,
        listen: SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, porta)),
        database: banco,
        ..ServerConfig::default()
    }
}

/// O PERSISTENCE de um servidor hospedado, compartilhado com quem o hospeda.
///
/// Apelido, e não o tipo escrito por extenso, porque quem chama é a casca do
/// desktop e ela **não depende de `tokio`**. Sem este nome, expor o banco
/// obrigaria o app a declarar a dependência só para escrever o tipo de uma
/// variável — uma dependência inteira paga em nome de uma anotação.
pub type PersistenceCompartilhada = Arc<tokio::sync::Mutex<crate::persistence::Persistence>>;

/// O que o servidor que esta janela hospeda tem a dizer de si, para a tela de
/// quem hospeda.
///
/// Hoje, as mensagens de controle de um vocabulário mais novo que o servidor
/// pulou (`server::MensagensDesconhecidas`): quem usa um SEELE mais novo está
/// mandando o que esta versão não entende, e quem hospeda resolve atualizando.
/// Sem este número, o servidor sabia e não contava — só o `seele.log` dizia.
/// A tela (o comando `estado_do_servidor` e a linha nas configurações do
/// servidor) entra quando a casca for liberada: a P12 do plano F1b.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstadoDoServidor {
    /// Quantas mensagens de um vocabulário mais novo foram puladas.
    pub mensagens_desconhecidas: u64,
    /// Em quantas sessões isso aconteceu.
    pub sessoes_com_desconhecidas: u64,
}

/// O link do navegador de um convite, e até quando ele abre
/// ([`Hospedagem::link_do_navegador`], D-web-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkDoNavegador {
    /// O link na origem do navegador (`ORIGEM_DO_NAVEGADOR`), com o `wh=` e o `ate=`.
    pub link: String,
    /// Até quando ele abre, em segundos Unix UTC: o `ate=` que vai nele.
    pub ate: u64,
    /// Quando um link feito de novo passa a durar mais, em segundos Unix UTC: é
    /// quando a casca pede outro.
    pub renova: u64,
    /// Quem de fora entra por ele ([`alcance_do_navegador`]).
    pub alcance: AlcanceDoNavegador,
}

/// **Quem de fora da rede de casa entra pelo link do navegador** (D-web-7,
/// saída C): a pergunta que o botão responde ao lado do link.
///
/// O navegador não abre socket UDP, e por isso perde o degrau 4 do ADR 0022 (o
/// furo pelo ponto de encontro; ADR 0057, «O que isto custa»). O link do app e
/// o do navegador são o mesmo convite, e alcançam a mesma gente em todo degrau
/// **menos** nesse. Sem este aviso, quem hospeda atrás de um roteador sem UPnP
/// mandava o link do navegador achando que mandou o do app, e o amigo
/// descobria do outro lado, como «não conecta».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlcanceDoNavegador {
    /// Endereço próprio, ou porta aberta no roteador: entra de qualquer rede,
    /// como pelo app.
    Alcanca,
    /// Só o ponto de encontro abriu o caminho: pelo app, quem tem o link entra
    /// de fora; pelo navegador, só quem está na mesma rede.
    SoPeloEncontro,
    /// Nem o app passa da rede de casa (ou da VPN): os dois links são locais.
    SoNaRede,
    /// Não se sabe: o IPv6 direto, que o firewall do roteador pode barrar sem
    /// ninguém daqui ver, ou a escada sem resposta.
    NaoSeSabe,
}

impl AlcanceDoNavegador {
    /// O nome estável que atravessa para a casca, onde mora a frase.
    #[must_use]
    pub fn nome(self) -> &'static str {
        match self {
            Self::Alcanca => "Alcanca",
            Self::SoPeloEncontro => "SoPeloEncontro",
            Self::SoNaRede => "SoNaRede",
            Self::NaoSeSabe => "NaoSeSabe",
        }
    }
}

/// O degrau em que a escada parou, dito para o navegador.
#[must_use]
pub fn alcance_do_navegador(degrau: Option<crate::alcance::Degrau>) -> AlcanceDoNavegador {
    use crate::alcance::Degrau;
    match degrau {
        Some(Degrau::EnderecoDireto | Degrau::PortaNoRoteador) => AlcanceDoNavegador::Alcanca,
        Some(Degrau::FuroDeNat) => AlcanceDoNavegador::SoPeloEncontro,
        Some(Degrau::RedeLocalOuVpn | Degrau::SoRedeLocal) => AlcanceDoNavegador::SoNaRede,
        Some(Degrau::Ipv6Direto) | None => AlcanceDoNavegador::NaoSeSabe,
    }
}

/// Por que um convite não deu link do navegador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemLinkDoNavegador {
    /// O texto não é um convite, ou não tem a forma que o navegador lê.
    ConviteInvalido,
    /// O convite leva a impressão de outro servidor.
    OutroServidor,
    /// O servidor não apresenta certificado curto agora: a sala pessoal, ou um
    /// servidor no ar há mais tempo que as janelas que ele gerou ao subir.
    SemJanelas,
}

/// Um servidor rodando dentro deste processo.
///
/// Descartar isto encerra o servidor e derruba quem estiver conectado. É o
/// comportamento certo: o anfitrião fechou.
pub struct Hospedagem {
    server: Arc<Daemon>,
    endereco: SocketAddr,
    /// A escada do ADR 0022, já subida, com a porta do roteador presa nela.
    escada: Option<crate::alcance::Escada>,
    /// A tarefa que aceita conexões.
    ///
    /// Guardada para poder ser esperada: ela segura uma referência ao servidor,
    /// e o socket UDP só é devolvido ao sistema quando a **última** referência
    /// some. Sem esperá-la, encerrar e reabrir na mesma porta falha.
    aceitando: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for Hospedagem {
    fn fmt(&self, formatador: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatador
            .debug_struct("Hospedagem")
            .field("endereco", &self.endereco)
            .finish()
    }
}

impl Hospedagem {
    /// Sobe um servidor e começa a aceitar conexões.
    ///
    /// `porta` zero deixa o sistema escolher — útil para teste, e ruim para uso
    /// real, onde o anfitrião precisa dizer aos amigos onde bater.
    ///
    /// Escuta em `[::]` de propósito: um servidor hospedado que só aceitasse
    /// `localhost` não serviria para nada além de falar consigo mesmo, e o
    /// ponto todo é receber gente. `[::]` e não `0.0.0.0` porque o segundo
    /// atende só IPv4 — degrau 2 do ADR 0022, ver [`crate::alcance`].
    ///
    /// # `antes_de_aceitar`, e por que ele é obrigatório
    ///
    /// Roda com o banco já aberto e migrado, e **antes** de o laço de aceitação
    /// existir. É onde quem hospeda semeia a portaria e se admite (ADR 0030), e
    /// onde a sala pessoal cria a conta do dono (tarefa 4f).
    ///
    /// Era feito por quem chamava, depois de esta função voltar, com o laço já
    /// no ar. Num banco novo a portaria nasce desligada, e a primeira conta
    /// criada num servidor vira a Comandante (`permissions::seat_the_arrival`):
    /// quem chegasse nesse intervalo entrava sem bater e ficava com o
    /// servidor. É a D-seg-1 do spec da 0.16, e a prova é
    /// `tests/janela_do_comandante.rs`.
    ///
    /// Obrigatório, e não um `Option`: um `iniciar` sem gancho deixaria de pé
    /// exatamente o caminho que abria a janela. Quem não tem nada a fazer antes
    /// passa `|_| Ok(())`, e escreve isso.
    ///
    /// # O ponto de encontro: só o que o ambiente pedir
    ///
    /// O degrau 4 só sobe com `$SEELE_ENCONTRO` apontando um ponto. **Sem a
    /// variável, nenhum** ([`crate::alcance::encontro::Convocacao::do_ambiente`]):
    /// é o padrão seguro, e é o de todo teste. A produção pede o ponto do
    /// projeto por [`Self::iniciar_com_o_ponto_padrao`].
    ///
    /// # Errors
    ///
    /// Falha se a porta já estiver em uso, se o banco não abrir, ou se
    /// `antes_de_aceitar` falhar. Nesse caso nenhuma conexão foi aceita.
    pub async fn iniciar(
        porta: u16,
        banco: Location,
        nome: &str,
        pasta_dos_mods: Option<crate::RaizesDosMods>,
        antes_de_aceitar: impl FnOnce(&mut Persistence) -> Result<()>,
    ) -> Result<Self> {
        Self::subir(
            config_guardada(porta, banco, nome, pasta_dos_mods),
            antes_de_aceitar,
            |socket, impressao, _| {
                crate::alcance::encontro::Convocacao::do_ambiente(socket, impressao)
            },
        )
        .await
    }

    /// [`Self::iniciar`], com o ponto de encontro do projeto quando
    /// `$SEELE_ENCONTRO` não existe.
    ///
    /// **Só a produção chama isto**: o botão HOSPEDAR do app. Quem hospeda
    /// pelo app é justamente quem está atrás de um roteador doméstico, e é para
    /// essa pessoa que o degrau 4 existe. A variável continua mandando: `nao`
    /// ou vazio desligam, um endereço troca.
    ///
    /// Duas funções, e não um argumento a mais em `iniciar`: o padrão seguro
    /// fica com o nome curto, e o ponto de produção só é pedido por quem
    /// escreve este nome. O `seeled` não chama nenhuma das duas — ele é o
    /// degrau 1 e não sobe a escada.
    ///
    /// # Errors
    ///
    /// Os mesmos de [`Self::iniciar`].
    pub async fn iniciar_com_o_ponto_padrao(
        porta: u16,
        banco: Location,
        nome: &str,
        pasta_dos_mods: Option<crate::RaizesDosMods>,
        antes_de_aceitar: impl FnOnce(&mut Persistence) -> Result<()>,
    ) -> Result<Self> {
        Self::subir(
            config_guardada(porta, banco, nome, pasta_dos_mods),
            antes_de_aceitar,
            |socket, impressao, _| {
                crate::alcance::encontro::Convocacao::com_o_ponto_padrao(socket, impressao)
            },
        )
        .await
    }

    /// A sala pessoal desta máquina: a segunda hospedagem do app (spec de 23/09,
    /// «Do lado de quem recebe»; ADRs 0059 e 0060).
    ///
    /// - **Em memória e sem MODs**: a ligação não guarda conversa, e na sala não
    ///   há MOD nenhum.
    /// - **Com a identidade da pessoa**: o certificado sai de `semente`, a
    ///   semente da `identity.key`, que nunca atravessa a webview
    ///   ([`crate::tls::Identity::da_semente`]).
    /// - **Com a campainha**: quem liga é segurado até quem recebe decidir, e o
    ///   teto é de [`crate::campainha::TETO_DA_LIGACAO`] pessoas. A recepção sai
    ///   por [`Self::recepcao`].
    /// - **A conta de quem recebe é criada antes de aceitar** (o gancho de
    ///   [`Self::iniciar`]). Na sala não há portaria, e quem recebe só conecta
    ///   ao atender. Sem isto, o primeiro atendido seria a primeira conta, e a
    ///   primeira conta vira a Comandante.
    /// - **Registrada sempre no ponto de encontro**, assinada (ENC/2), quando há
    ///   `ponto`: ela só é achável pelo código. `None` não registra, e é o caso
    ///   dos testes. A produção passa o ponto que `$SEELE_ENCONTRO` escolher, ou
    ///   o do projeto (`seele_proto::encontro::ponto_escolhido`).
    ///
    /// `porta` é [`PORTA_DA_SALA`] em produção, e zero nos testes.
    ///
    /// # Errors
    ///
    /// Falha se a porta já estiver em uso, ou se o certificado da sala não
    /// sair da semente.
    pub async fn iniciar_sala(
        porta: u16,
        semente: &[u8; 32],
        apelido: &str,
        ponto: Option<String>,
    ) -> Result<Self> {
        let pessoa = ed25519_dalek::SigningKey::from_bytes(semente);
        let dono = pessoa.verifying_key().to_bytes();
        let config = ServerConfig {
            name: apelido.to_owned(),
            mods_dir: None,
            listen: SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, porta)),
            database: Location::Memory,
            identidade: Some(crate::tls::Identity::da_semente(semente)?),
            voice_room_limit: u16::try_from(crate::campainha::TETO_DA_LIGACAO).unwrap_or(u16::MAX),
            sala: Some(crate::recepcao::Sala::de(dono)),
            ..ServerConfig::default()
        };
        let apelido = apelido.to_owned();
        Self::subir(
            config,
            move |persistence| {
                crate::permissions::Permissions::new(persistence)
                    .register_or_find(&dono, &apelido)
                    .map(|_| ())
            },
            move |socket, _, escuta| {
                ponto.map(|ponto| {
                    crate::alcance::encontro::Convocacao::para_sala(socket, escuta, pessoa, ponto)
                })
            },
        )
        .await
    }

    /// A recepção da sala pessoal, para o app assinar o toque e decidir. `None`
    /// num servidor guardado.
    #[must_use]
    pub fn recepcao(&self) -> Option<Arc<crate::recepcao::Recepcao>> {
        self.server.server().recepcao.clone()
    }

    /// O corpo das três: `convocar` é quem decide o ponto de encontro, com o
    /// socket do servidor, a impressão digital dele e a escuta.
    async fn subir(
        config: ServerConfig,
        antes_de_aceitar: impl FnOnce(&mut Persistence) -> Result<()>,
        convocar: impl FnOnce(
            Arc<std::net::UdpSocket>,
            &str,
            crate::alcance::Escuta,
        ) -> Option<crate::alcance::encontro::Convocacao>,
    ) -> Result<Self> {
        let server = Arc::new(Daemon::bind(config).await?);
        let endereco = server.local_addr()?;

        // **Antes de aceitar a primeira conexão.** O laço só nasce lá embaixo,
        // depois disto; até lá, quem bater espera. Ver o cabeçalho.
        {
            let banco = Arc::clone(&server.server().persistence);
            let mut persistence = banco.lock().await;
            antes_de_aceitar(&mut persistence)?;
        }

        // A escada do ADR 0022, aqui e não no `Daemon`: quem hospeda de dentro
        // do cliente é justamente quem está atrás de um roteador doméstico. Um
        // `seeled` numa VPS já é o degrau 1 e não tem o que pedir a ninguém.
        //
        // Custa até `alcance::porta::PROCURA` no pior caso, e o pior caso é o
        // comum: numa rede sem UPnP a busca esgota o prazo inteiro. Foi por
        // isso que aquele prazo é curto.
        //
        // A escuta **inteira**, e não só a porta: numa máquina em que a pilha
        // dupla falhou o servidor atende só em IPv4, e a escada não pode prometer
        // um degrau que este socket não serve. Ver `alcance::Escuta`.
        //
        // O degrau 4 vai junto, e ele é opcional em três sentidos: só é tentado
        // se os degraus de cima não resolveram, só existe se houver ponto (o do
        // ambiente, ou o do projeto em `iniciar_com_o_ponto_padrao`), e só
        // funciona com o socket do próprio servidor — sem ele não há furo
        // possível, e o degrau simplesmente não acontece.
        //
        // Custa até `alcance::encontro::PRAZO` a mais, e só no caminho que hoje
        // termina em "só funciona na sua rede".
        let escuta = crate::alcance::Escuta::nova(endereco.port(), server.pilha());
        let convocacao = server
            .espelho()
            .and_then(|socket| convocar(socket, server.fingerprint(), escuta));
        // A segunda parede, só nos testes: um teste que chega aqui com o ponto
        // de produção — o ambiente apontando a VPS, ou
        // `iniciar_com_o_ponto_padrao` sem ambiente — reprova antes da escada,
        // em qualquer máquina, e não só numa sem IPv4 global, que é onde o
        // degrau 4 é tentado. Fora deles, nada. Ver
        // `crate::alcance::encontro::recusar_a_producao`.
        if let Some(convocacao) = &convocacao {
            crate::alcance::encontro::recusar_a_producao(
                &convocacao.ponto,
                "Hospedagem::iniciar, antes da escada",
            );
        }
        let escada = crate::alcance::Escada::subir(escuta, convocacao).await;
        tracing::info!(alcance = ?escada.alcance(), "escada do ADR 0022 subida");

        // O laço de aceitação numa tarefa própria: quem chamou tem interface
        // para desenhar, e o `run` só volta quando o servidor acaba.
        let referencia = Arc::clone(&server);
        let aceitando = tokio::spawn(async move {
            if let Err(erro) = referencia.run().await {
                tracing::error!(%erro, "o servidor hospedado parou");
            }
        });

        Ok(Self {
            server,
            endereco,
            escada: Some(escada),
            aceitando: Some(aceitando),
        })
    }

    /// Como está o registro assinado da sala pessoal no ponto de encontro
    /// (ENC/2, ADR 0062), para a casca dizer «SEU CÓDIGO NÃO ESTÁ ACHÁVEL
    /// AGORA», e o motivo. `None` num servidor guardado, ou sem degrau 4.
    #[must_use]
    pub fn registro_no_quarto(
        &self,
    ) -> Option<tokio::sync::watch::Receiver<crate::alcance::encontro::EstadoDoRegistro>> {
        self.escada
            .as_ref()
            .and_then(crate::alcance::Escada::registro_no_quarto)
    }

    /// Até onde este servidor é alcançável, e por qual degrau do ADR 0022.
    ///
    /// É o que permite à casca dizer "só na sua rede, e foi por isto" em vez de
    /// deixar quem hospeda achando que abriu para o mundo.
    #[must_use]
    pub fn alcance(&self) -> Option<&crate::alcance::Alcance> {
        self.escada.as_ref().map(crate::alcance::Escada::alcance)
    }

    /// O que o servidor desta hospedagem tem a dizer de si à tela de quem
    /// hospeda. Ver [`EstadoDoServidor`].
    #[must_use]
    pub fn estado(&self) -> EstadoDoServidor {
        let desconhecidas = &self.server.server().desconhecidas;
        EstadoDoServidor {
            mensagens_desconhecidas: desconhecidas.mensagens(),
            sessoes_com_desconhecidas: desconhecidas.sessoes(),
        }
    }

    /// Onde o servidor está escutando.
    #[must_use]
    pub fn endereco(&self) -> SocketAddr {
        self.endereco
    }

    /// O endereço que se manda para os amigos.
    ///
    /// `0.0.0.0` é onde se escuta, não um lugar aonde alguém possa ir. Isto
    /// devolve o endereço desta máquina na rede — o mesmo raciocínio do
    /// `seeled` ao imprimir o que digitar na outra máquina.
    #[must_use]
    pub fn endereco_na_rede(&self) -> Option<SocketAddr> {
        endereco_de_rede().map(|ip| SocketAddr::new(ip, self.endereco.port()))
    }

    /// A impressão digital que os clientes vão fixar. ADR 0003.
    #[must_use]
    pub fn impressao_digital(&self) -> &str {
        self.server.fingerprint()
    }

    /// O link para mandar aos amigos.
    ///
    /// Mora aqui, e não em cada casca, porque quem sabe montá-lo é quem tem as
    /// duas partes: o endereço em que dá para chegar e a impressão digital
    /// desta instância. Os dois clientes montavam o mesmo link à mão, e duas
    /// cópias de uma construção é uma que vai ficar para trás.
    ///
    /// Sem rede, cai no endereço de escuta — que não serve para convidar
    /// ninguém, mas é a resposta honesta, e quem chamou pode dizer isso.
    ///
    /// # Quais endereços entram aqui
    ///
    /// **Todos** os que a escada do ADR 0022 achou, na ordem em que se tenta:
    /// rede de casa primeiro, endereço global depois, porta do roteador em
    /// seguida, túnel por último. Ver `alcance::Alcance`.
    ///
    /// Foi um endereço só até 0.5.0, e essa é a história do defeito: com um
    /// endereço só, alguma situação sempre perde. Escolher o degrau mais alto
    /// fazia o link deixar de funcionar para quem estava na mesma casa — a
    /// maioria dos roteadores domésticos não devolve para dentro o próprio
    /// endereço externo.
    ///
    /// `SocketAddr` e nunca `format!("{ip}:{porta}")`: o `Display` do
    /// `SocketAddr` põe os colchetes num IPv6, e agora que estes endereços
    /// podem **ser** IPv6 isso deixou de ser detalhe.
    #[must_use]
    pub fn convite(&self) -> String {
        let alvos: Vec<String> = self.alcance().map_or_else(
            || vec![self.endereco_na_rede().unwrap_or(self.endereco).to_string()],
            |alcance| alcance.alvos().iter().map(SocketAddr::to_string).collect(),
        );
        let (primeiro, resto) = alvos.split_first().map_or_else(
            || (self.endereco.to_string(), &[][..]),
            |(um, resto)| (um.clone(), resto),
        );
        let mut convite = seele_proto::uri::Convite::novo(primeiro)
            .com_alternativos(resto.to_vec())
            .com_impressao_digital(self.impressao_digital());
        // **A versão do servidor vai no link** — ADR 0046, com as duas
        // numerações da 0.16 (spec-mestre, §3, item 22). É o único canal que
        // chega a quem vai entrar **antes** de qualquer aperto de mão, e é
        // nessa janela que o launcher decide se entra aqui ou abre o app da
        // linha do servidor (`versoes::para_entrar`, que lê `M.N` como versão
        // de servidor).
        //
        // Até a 0.15 era o carimbo da build do app, e um build feito à mão não
        // punha nada. A versão do servidor é fato do protocolo, e todo build
        // a sabe.
        convite = convite.com_versao(seele_proto::version::VERSAO_DO_SERVIDOR);
        // O bilhete de encontro só entra quando o degrau 4 deu — e ele só é
        // tentado quando os de cima não deram. Um servidor alcançável de fora não
        // põe ponto de encontro nenhum no link de ninguém.
        match self
            .escada
            .as_ref()
            .and_then(crate::alcance::Escada::bilhete)
        {
            Some(bilhete) => convite.com_bilhete(bilhete).to_string(),
            None => convite.to_string(),
        }
    }

    /// O mesmo convite, com um **nome público** no lugar do endereço numérico.
    ///
    /// # Por que o nome vira o alvo e os números viram alternativos
    ///
    /// Porque um nome é uma promessa de que alguém tem de cumprir no DNS, e um
    /// número é um fato. Pondo o nome na frente, quem recebe o link tenta o nome
    /// primeiro — que é o ponto de ter um nome: ele continua valendo amanhã,
    /// quando o endereço da casa mudar. Os números ficam logo atrás, e é o que
    /// faz o link não piorar nada: se o nome não resolver na rede de quem
    /// recebe, o caminho de hoje ainda está lá.
    ///
    /// # O que isto custa a quem já entrou pelo número
    ///
    /// Uma pergunta de confiança a mais. A chave do pino do TOFU é **o texto do
    /// alvo** (`seele-core/src/tofu.rs`), então `casa.exemplo` e `203.0.113.9`
    /// são dois pinos diferentes para o mesmo servidor: quem já entrou pelo
    /// número e passar a entrar pelo nome é perguntado de novo sobre a
    /// impressão digital. Não é defeito e não se conserta daqui — é o que TOFU
    /// quer dizer —, e por isso a tela avisa antes em vez de a pessoa descobrir
    /// no susto.
    ///
    /// `None` devolve [`Self::convite`] sem tirar nem pôr nada: o campo vazio é
    /// a ausência de escolha, e ela tem de continuar dando no link de sempre.
    #[must_use]
    pub fn convite_com_nome(&self, nome: Option<&str>) -> String {
        let Some(nome) = nome else {
            return self.convite();
        };
        match seele_proto::uri::analisar(&self.convite()) {
            Ok(convite) => {
                let mut alternativos = convite.alternativos.clone();
                // O alvo de antes desce para a primeira alternativa, e não some:
                // ele é o caminho que funciona **agora**, e o nome é o que
                // promete funcionar amanhã.
                alternativos.insert(0, convite.alvo.clone());
                seele_proto::uri::Convite {
                    alvo: nome.to_owned(),
                    alternativos,
                    ..convite
                }
                .to_string()
            }
            // Impossível na prática — é o link que acabamos de montar. Devolver
            // o numérico é melhor que devolver nada a quem quer convidar.
            Err(_) => self.convite(),
        }
    }

    /// O PERSISTENCE deste servidor, para quem hospeda mexer na própria porta.
    ///
    /// ADR 0030. É por aqui que a janela fecha o servidor, gera convite e decide
    /// quem entra — direto no banco da máquina, e não pelo fio como toda a
    /// moderação faz.
    ///
    /// Três motivos, no ADR: fechar a porta não pode depender de estar dentro,
    /// senão a defesa depende do canal que ela defende; a porta se fecha no
    /// mesmo gesto de hospedar, antes do primeiro pacote; e nenhum verbo novo de
    /// protocolo significa nenhuma superfície nova exposta à internet para uma
    /// decisão que é, por definição, de quem está na máquina.
    ///
    /// Devolve o `Arc` em vez de travar aqui de propósito: quem chama está
    /// segurando um `Mutex` de estado do app, e travar deste lado o faria
    /// segurar os dois. Clonar é barato, e o `await` acontece depois de o outro
    /// já ter sido solto.
    #[must_use]
    pub fn persistence(&self) -> PersistenceCompartilhada {
        Arc::clone(&self.server.server().persistence)
    }

    /// O servidor montado, para o que precisa do banco **e** do barramento —
    /// a escolha do tema por quem administra ([`crate::mods::tema::escolher`])
    /// grava e avisa as sessões no mesmo ato.
    ///
    /// Um `Arc`, pela razão de [`Self::persistence`]: quem chama segura o
    /// `Mutex` das hospedagens, e o `await` vem depois de soltá-lo.
    #[must_use]
    pub fn servidor(&self) -> Arc<crate::server::Server> {
        Arc::clone(self.server.server())
    }

    /// O link para mandar a uma pessoa, com um convite de uso único dentro.
    ///
    /// Igual ao [`Self::convite`] em tudo — os mesmos endereços, na mesma ordem,
    /// com a mesma impressão digital — mais o token. Reusa aquele em vez de
    /// remontar porque duas construções do mesmo link é uma que vai ficar para
    /// trás, que é o que o comentário dele já dizia.
    #[must_use]
    pub fn convite_com_token(&self, token: &str) -> String {
        match seele_proto::uri::analisar(&self.convite()) {
            Ok(convite) => convite.com_token(token).to_string(),
            // O link que acabamos de montar não parsear é impossível na prática;
            // devolver o sem token é melhor que entregar nada a quem convida.
            Err(_) => self.convite(),
        }
    }

    /// **O link do navegador de um convite deste servidor** (D-web-6): o mesmo
    /// convite, na forma que o web app abre (`Convite::link_do_navegador`), com
    /// os hashes dos certificados curtos que este servidor apresenta de agora em
    /// diante ([`crate::tls::janelas_do_link`]).
    ///
    /// Recebe o convite que a casca mostra, e não monta outro: com um convite
    /// de uso único, o link do navegador leva **o mesmo** token, e com um nome
    /// público, o mesmo nome. Duas construções do mesmo convite é uma que vai
    /// ficar para trás, como o comentário de [`Self::convite`] já dizia.
    ///
    /// `agora` em segundos Unix UTC. É argumento, e não o relógio daqui, para a
    /// casca e a bancada pedirem o mesmo link pelo mesmo caminho.
    ///
    /// # Errors
    ///
    /// [`SemLinkDoNavegador`]: um convite que não se lê, um de outro servidor
    /// (os hashes daqui num convite de lá não abririam nada), e um servidor sem
    /// a janela de agora.
    pub fn link_do_navegador(
        &self,
        convite: &str,
        agora: u64,
    ) -> Result<LinkDoNavegador, SemLinkDoNavegador> {
        let convite =
            seele_proto::uri::analisar(convite).map_err(|_| SemLinkDoNavegador::ConviteInvalido)?;
        let deste = convite
            .impressao_digital
            .as_deref()
            .is_some_and(|impressao| impressao.eq_ignore_ascii_case(self.impressao_digital()));
        if !deste {
            return Err(SemLinkDoNavegador::OutroServidor);
        }
        let janelas = crate::tls::janelas_do_link(self.server.janelas_do_navegador(), agora)
            .ok_or(SemLinkDoNavegador::SemJanelas)?;
        let link = convite
            .link_do_navegador(&janelas.hashes, Some(janelas.ate))
            .map_err(|_| SemLinkDoNavegador::ConviteInvalido)?;
        Ok(LinkDoNavegador {
            link,
            ate: janelas.ate,
            renova: janelas.renova,
            alcance: alcance_do_navegador(self.alcance().map(crate::alcance::Alcance::degrau)),
        })
    }

    /// Numa sala, a linha de resumo da campainha no log: quantos tocaram e
    /// como acabaram (tarefa 31 da F7). Num servidor guardado não há campainha,
    /// e nada a dizer.
    fn resumir_a_sala(&self) {
        if let Some(recepcao) = self.recepcao() {
            recepcao.resumir();
        }
    }

    /// O motivo com que quem está dentro é avisado de que esta hospedagem
    /// fecha, em [`Self::encerrar`] e em [`Self::despedir`].
    ///
    /// Num servidor guardado, o servidor está encerrando. Numa sala pessoal, a
    /// **ligação** acabou: `ChamadaEncerrada`, a mesma razão de quando quem
    /// recebe desliga, e a frase que quem ligou lê é «ALICE ENCERROU A
    /// LIGAÇÃO», e não a de um servidor que saiu do ar (spec de 23/09, «Erros e
    /// frases»).
    fn motivo_da_despedida(&self) -> seele_proto::control::DisconnectReason {
        if self.recepcao().is_some() {
            seele_proto::control::DisconnectReason::ChamadaEncerrada
        } else {
            seele_proto::control::DisconnectReason::ServerShuttingDown
        }
    }

    /// Encerra o servidor **avisando quem está dentro**, e devolve a porta.
    ///
    /// É o SAIR de quem hospeda: a casca chama isto pelo comando `disconnect`,
    /// depois de desmontar o próprio cliente.
    ///
    /// Consome, e a espera está aqui de propósito. Fechar uma conversa e abrir
    /// outra é o caso normal, e sem esperar isso falha com "endereço já em
    /// uso" — um erro que não diz nada a quem só clicou em parar e começar de
    /// novo. O custo cai onde a pessoa já espera uma pausa, ao fechar, e não
    /// onde ela espera rapidez, ao abrir.
    ///
    /// São quatro esperas e cada uma tem um motivo: cada sessão ler que o
    /// servidor está encerrando (até `PRAZO_DO_AVISO`, meio segundo), as conexões
    /// terminarem, a tarefa de aceitação soltar a referência dela, e o driver
    /// do QUIC devolver o socket depois que a última referência some.
    ///
    /// # O aviso
    ///
    /// Era `shutdown()` cru, e quem estava dentro lia uma queda de transporte:
    /// a tela dizia «não foi possível alcançar o servidor», e a bateria batia
    /// cinco minutos numa porta fechada. Agora cada sessão lê
    /// `ServerShuttingDown` («O SERVIDOR ESTÁ ENCERRANDO»), ou
    /// `ChamadaEncerrada` numa sala ([`Self::motivo_da_despedida`]), e acaba.
    /// Ver [`Daemon::encerrar_avisando`].
    ///
    /// O roteador vem **antes** do aviso aqui, e depois em [`Self::despedir`]:
    /// no SAIR o processo continua vivo, e as duas esperas cabem; na saída do
    /// app só cabe o que vier antes de o runtime cair, e o aviso vale mais.
    pub async fn encerrar(mut self) {
        // Primeiro a porta do roteador. Uma regra deixada para trás aponta para
        // uma máquina que não vai mais atender, e só some quando o prazo vence.
        if let Some(escada) = self.escada.take() {
            escada.descer().await;
        }
        self.resumir_a_sala();
        self.server
            .encerrar_avisando(self.motivo_da_despedida(), PRAZO_DO_AVISO)
            .await;
        self.server.wait_idle().await;
        if let Some(aceitando) = self.aceitando.take() {
            let _ = aceitando.await;
        }
        drop(self);
        // Medido: sem isto, reabrir a mesma porta em seguida falha. O driver do
        // endpoint fecha o socket ao ser recolhido, e isso leva um instante que
        // não dá para observar de fora.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    /// A despedida da **saída do app**: avisa quem está dentro e devolve a
    /// porta do roteador, tudo dentro de `teto`.
    ///
    /// Para o Plano 2 chamar da casca, no `ExitRequested` segurado: depois
    /// disto o runtime cai, e o que não tiver saído até ali não sai mais. Por
    /// isso cada passo tem a sua fatia do teto, e nenhum espera sem prazo:
    ///
    /// 1. **o aviso**, metade do teto: cada sessão lê `ServerShuttingDown` e
    ///    fecha ([`Daemon::encerrar_avisando`]), e o endpoint fecha depois;
    /// 2. **o fechamento sair**, um quarto: `wait_idle` espera o endpoint
    ///    terminar de mandar o fechamento das conexões que o prazo do aviso
    ///    deixou para trás. Sem ele, o runtime cai com esse fechamento na fila,
    ///    e quem estava conectado só descobre pelos 20 s do tempo ocioso do
    ///    transporte. Ele pode voltar pelo próprio prazo, numa rede longe, com
    ///    conexões ainda drenando: o fechamento delas já saiu, e o dreno só
    ///    responde a pacotes atrasados;
    /// 3. **o roteador**, o último quarto: devolver a porta pedida pelo degrau
    ///    3. Uma regra que sobra só some quando o prazo dela vence.
    ///
    /// É a ordem inversa da de [`Self::encerrar`], e de propósito: aqui o aviso
    /// vale mais que o roteador. A regra esquecida no roteador expira sozinha; a
    /// pessoa que lê «não foi possível alcançar o servidor» procura defeito na
    /// casa dela.
    ///
    /// Não espera a porta UDP voltar ao sistema, ao contrário de
    /// [`Self::encerrar`]: o processo está saindo, e ninguém vai reabri-la.
    pub async fn despedir(mut self, teto: std::time::Duration) {
        self.resumir_a_sala();
        self.server
            .encerrar_avisando(self.motivo_da_despedida(), teto / 2)
            .await;
        let _ = tokio::time::timeout(teto / 4, self.server.wait_idle()).await;
        if let Some(escada) = self.escada.take() {
            if tokio::time::timeout(teto / 4, escada.descer())
                .await
                .is_err()
            {
                tracing::info!(
                    "a porta do roteador não foi devolvida dentro do teto da despedida; ela \
                     some quando o prazo dela vencer"
                );
            }
        }
    }
}

/// Quanto o SAIR de quem hospeda espera, no máximo, cada sessão ler que o
/// servidor está encerrando.
///
/// Meio segundo. A despedida de uma sessão é o `Disconnecting` e a
/// confirmação de entrega: uma ida e volta. O endpoint, porém, só fica ocioso
/// depois que cada conexão fechada cumpre o dreno do QUIC, três PTO — no laço
/// local, milissegundos; numa internet com RTT de umas poucas dezenas de
/// milissegundos, mais que este prazo. Ver [`Daemon::encerrar_avisando`]. Então
/// o teto é pago inteiro em dois casos: por quem não se despede (alguém no
/// meio do aperto de mão, um par que não confirma a entrega), e pelo dreno de
/// quem já se despediu numa rede longe. No segundo caso o aviso já chegou, e o
/// `shutdown` do fim não muda nada para aquela sessão.
///
/// O `session::despedir` espera até 1 s pela entrega de cada sessão; com o
/// prazo menor que isso, uma sessão lenta não segura o SAIR, e é fechada pelo
/// `shutdown` do fim.
const PRAZO_DO_AVISO: std::time::Duration = std::time::Duration::from_millis(500);

impl Drop for Hospedagem {
    /// Sinaliza o fechamento, sem esperar — `drop` não pode aguardar.
    ///
    /// Quem vai reabrir a porta em seguida deve chamar [`Hospedagem::encerrar`]
    /// antes de descartar.
    ///
    /// **E sem avisar ninguém**: quem estava dentro lê a conexão caindo. Quem
    /// pode esperar chama [`Hospedagem::encerrar`] (o SAIR) ou
    /// [`Hospedagem::despedir`] (a saída do app), que avisam antes.
    fn drop(&mut self) {
        self.server.shutdown();
    }
}

/// O endereço desta máquina na rede de casa.
///
/// Pergunta às interfaces, e não à rota padrão: com uma VPN ligada a rota
/// padrão é a do túnel, e o endereço que ela devolve não é alcançável por
/// ninguém na mesma sala. Ver `alcance::endereco_de_rede_local`.
#[must_use]
pub fn endereco_de_rede() -> Option<std::net::IpAddr> {
    crate::alcance::endereco_de_rede_local()
}

#[cfg(test)]
mod o_alcance_do_navegador {
    use super::{alcance_do_navegador, AlcanceDoNavegador};
    use crate::alcance::Degrau;

    /// **O degrau 4 é o que o navegador perde, e só ele** (D-web-7, saída C):
    /// pelo furo, o link do navegador alcança só a rede de casa, e a casca diz
    /// isso ao lado do botão; nos degraus 1 e 3, alcança como o do app; no 2,
    /// e sem escada, não se sabe.
    #[test]
    fn so_o_degrau_4_tira_o_navegador_de_fora() {
        for (degrau, esperado) in [
            (Some(Degrau::EnderecoDireto), AlcanceDoNavegador::Alcanca),
            (Some(Degrau::PortaNoRoteador), AlcanceDoNavegador::Alcanca),
            (Some(Degrau::FuroDeNat), AlcanceDoNavegador::SoPeloEncontro),
            (Some(Degrau::Ipv6Direto), AlcanceDoNavegador::NaoSeSabe),
            (Some(Degrau::RedeLocalOuVpn), AlcanceDoNavegador::SoNaRede),
            (Some(Degrau::SoRedeLocal), AlcanceDoNavegador::SoNaRede),
            (None, AlcanceDoNavegador::NaoSeSabe),
        ] {
            assert_eq!(alcance_do_navegador(degrau), esperado, "{degrau:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sala que fecha diz o resumo da campainha no log, pelas duas
    /// despedidas, e um servidor guardado não diz nada (tarefa 31 da F7).
    #[tokio::test]
    async fn a_sala_que_fecha_resume_a_campainha_e_o_guardado_nao() {
        use std::io::Write;

        #[derive(Clone, Default)]
        struct Buffer(Arc<std::sync::Mutex<Vec<u8>>>);
        impl Write for Buffer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("o rastro").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        let buffer = Buffer::default();
        let escreve = buffer.clone();
        let assinante = tracing_subscriber::fmt()
            .with_writer(move || escreve.clone())
            .without_time()
            .with_ansi(false)
            .finish();
        crate::rastro_de_teste::segundo_despachante();
        let _captura = tracing::subscriber::set_default(assinante);

        let despedida = Hospedagem::iniciar_sala(0, &[1; 32], "dona", None)
            .await
            .expect("a sala");
        despedida
            .despedir(std::time::Duration::from_millis(200))
            .await;
        let encerrada = Hospedagem::iniciar_sala(0, &[2; 32], "outra", None)
            .await
            .expect("a outra sala");
        encerrada.encerrar().await;
        let guardado = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("o guardado");
        guardado.encerrar().await;

        let log = String::from_utf8(buffer.0.lock().expect("o rastro").clone()).expect("UTF-8");
        assert_eq!(
            log.lines()
                .filter(|linha| linha.contains("campainha: a sala fechou"))
                .count(),
            2,
            "uma linha de resumo por sala que fecha, e nenhuma do guardado: {log}"
        );
    }

    #[tokio::test]
    async fn um_server_hospedado_aceita_conexao() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");

        assert_ne!(hospedagem.endereco().port(), 0, "não escolheu porta");
        assert_eq!(
            hospedagem.impressao_digital().len(),
            64,
            "a impressão digital não é um SHA-256"
        );
    }

    /// O link com nome público, exercido pelo que ele **produz**.
    ///
    /// Comportamento e não texto-fonte: o que se afirma é a URI que sai, lida de
    /// volta pelo analisador de `seele-proto`. Um guarda que procurasse a
    /// palavra «nome» neste arquivo ficaria verde com o nome sendo ignorado.
    #[tokio::test]
    async fn o_nome_publico_vira_o_alvo_do_link_e_o_numero_desce_para_alternativo() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");

        let sem_nome = hospedagem.convite_com_nome(None);
        assert_eq!(
            sem_nome,
            hospedagem.convite(),
            "o campo vazio mudou o link: a ausência de escolha tem de dar no              comportamento de hoje, inteiro e sem tirar nem pôr"
        );

        let numerico = seele_proto::uri::analisar(&sem_nome).expect("o link de sempre analisa");

        let com_nome = hospedagem.convite_com_nome(Some("casa.exemplo.br"));
        let lido = seele_proto::uri::analisar(&com_nome).expect("o link com nome analisa");
        assert_eq!(
            lido.alvo, "casa.exemplo.br",
            "o nome não virou o alvo do link"
        );
        assert!(
            lido.alternativos.contains(&numerico.alvo),
            "o endereço numérico sumiu do link. Ele é o caminho que funciona              **agora**; o nome é o que promete funcionar amanhã, e tirar o              primeiro para pôr o segundo é piorar hoje por uma promessa."
        );
        assert_eq!(
            lido.impressao_digital, numerico.impressao_digital,
            "a impressão digital não atravessou: um link sem ela é um link em              que a primeira visita não confere nada"
        );
    }

    /// **O estado que a janela de quem hospeda lê** (a P12): as mensagens de
    /// um vocabulário mais novo que o servidor pulou, e em quantas sessões.
    /// Zero num servidor que não pulou nada, e os números do contador do
    /// servidor, e não de outro, quando pulou.
    #[tokio::test]
    async fn o_estado_conta_as_mensagens_de_apps_mais_novos() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");
        assert_eq!(
            hospedagem.estado(),
            EstadoDoServidor {
                mensagens_desconhecidas: 0,
                sessoes_com_desconhecidas: 0,
            }
        );

        // Duas mensagens numa sessão, uma noutra: o que a leitora do laço de
        // controle registra (`session::run_session`).
        let contador = &hospedagem.server.server().desconhecidas;
        contador.registrar(true);
        contador.registrar(false);
        contador.registrar(true);
        assert_eq!(
            hospedagem.estado(),
            EstadoDoServidor {
                mensagens_desconhecidas: 3,
                sessoes_com_desconhecidas: 2,
            }
        );
    }

    /// **O link diz a versão do servidor**, e não a do app (spec-mestre da
    /// 0.16, §3, item 22): `v=1.0`, de `VERSAO_DO_SERVIDOR`.
    ///
    /// Até a 0.15 o `v=` era o carimbo da build do app, e um build sem carimbo
    /// não punha nada. Com as duas numerações, quem recebe o link precisa saber
    /// **que servidor** vai encontrar, e isso é fato do protocolo, que todo
    /// build sabe: um build feito à mão também anuncia o Servidor 1.0.
    #[tokio::test]
    async fn o_link_anuncia_a_versao_do_servidor() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");
        let link = hospedagem.convite();
        let lido = seele_proto::uri::analisar(&link).expect("o link analisa");

        assert_eq!(
            lido.versao.as_deref(),
            Some(seele_proto::version::VERSAO_DO_SERVIDOR),
            "o link não anuncia a versão do servidor: {link}"
        );
        assert!(link.contains("v=1.0"), "{link}");
        // E os links derivados levam o mesmo.
        for derivado in [
            hospedagem.convite_com_nome(Some("casa.exemplo")),
            hospedagem.convite_com_token("a".repeat(43).as_str()),
        ] {
            let lido = seele_proto::uri::analisar(&derivado).expect("o derivado analisa");
            assert_eq!(
                lido.versao.as_deref(),
                Some(seele_proto::version::VERSAO_DO_SERVIDOR),
                "{derivado}"
            );
        }
    }

    #[tokio::test]
    async fn escuta_em_todas_as_interfaces_e_nao_so_em_localhost() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        // Um servidor hospedado que só aceitasse localhost serviria para falar
        // sozinho, que é o oposto do motivo de existir.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");
        assert!(hospedagem.endereco().ip().is_unspecified());
    }

    #[tokio::test]
    async fn o_endereco_para_os_amigos_nao_e_o_de_escuta() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        // `0.0.0.0` é onde se escuta, não um lugar aonde alguém possa ir.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");

        if let Some(rede) = hospedagem.endereco_na_rede() {
            assert!(!rede.ip().is_unspecified());
            assert!(!rede.ip().is_loopback());
            assert_eq!(rede.port(), hospedagem.endereco().port());
        }
        // Numa máquina sem rede não há o que devolver, e `None` é a resposta
        // honesta — quem chamou mostra o endereço de escuta e avisa.
    }

    #[tokio::test]
    async fn o_convite_carrega_a_impressao_digital_desta_instancia() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        // Sem ela o primeiro contato volta a ser cego, e o convite deixa de ser
        // a coisa que torna o TOFU verificável.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");

        let convite = hospedagem.convite();
        let lido = seele_proto::uri::analisar(&convite).expect("o convite não se lê de volta");
        assert_eq!(
            lido.impressao_digital.as_deref(),
            Some(hospedagem.impressao_digital())
        );
        assert!(
            !convite.contains("0.0.0.0"),
            "convidou para o nada: {convite}"
        );
    }

    #[tokio::test]
    async fn o_convite_leva_o_endereco_da_rede_local_quando_ele_existe() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        // O defeito de campo, do lado de quem gera o link: em 0.5.0 o convite
        // levava só o endereço do degrau mais alto, e quem estava na mesma casa
        // deixou de conseguir entrar. Agora o endereço da rede local está
        // sempre lá — e é o primeiro, porque é o caso comum.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");
        let convite = hospedagem.convite();
        let lido = seele_proto::uri::analisar(&convite).expect("o convite não se lê de volta");

        let Some(rede) = hospedagem.endereco_na_rede() else {
            eprintln!("pulado: esta máquina não tem endereço de rede local");
            return;
        };
        let candidatos: Vec<&str> = lido.candidatos().collect();
        assert!(
            candidatos.contains(&rede.to_string().as_str()),
            "o convite não leva o endereço da rede local ({rede}): {candidatos:?}"
        );
    }

    /// Um endereço para onde não há rota em lugar nenhum: TEST-NET-1 (RFC 5737).
    ///
    /// É o mais perto de "o ponto de encontro está fora do ar" que cabe num
    /// teste sem rede — um pacote sai e nunca volta resposta.
    const BURACO_NEGRO: &str = "192.0.2.1:8384";

    #[tokio::test]
    async fn um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada() {
        // O requisito do ADR 0022 escrito como asserção, e ele é sobre o degrau
        // 4 **não** poder virar ponto único de falha: com o ponto de encontro
        // inalcançável, hospedar continua funcionando, o link continua saindo
        // com os mesmos endereços de sempre, e a espera a mais é no máximo o
        // prazo escolhido — não uma espera de rede sem fim.
        //
        // A variável volta pelo `Drop` do guarda, inclusive se um `expect`
        // abaixo entrar em pânico, e a trava dele impede o outro teste que a
        // troca de correr junto.
        let ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::guardar();

        // Sem degrau 4 nenhum: é o servidor de antes desta mudança, e é a régua.
        ambiente.pedir("nao");
        let relogio = std::time::Instant::now();
        let sem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir sem ponto de encontro");
        let sem_encontro = relogio.elapsed();
        let convite_de_antes = sem.convite();
        let degrau_de_antes = sem.alcance().map(crate::alcance::Alcance::degrau);
        sem.encerrar().await;

        // Com um ponto de encontro que não responde nunca.
        ambiente.pedir(BURACO_NEGRO);
        let relogio = std::time::Instant::now();
        let com = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("hospedar falhou porque o ponto de encontro está fora do ar");
        let com_encontro = relogio.elapsed();
        let convite_de_agora = com.convite();
        let degrau_de_agora = com.alcance().map(crate::alcance::Alcance::degrau);
        com.encerrar().await;
        // As duas subidas acabaram: a variável e a trava voltam já, e não
        // depois das asserções.
        drop(ambiente);

        // Nada do que funcionava mudou: mesmo degrau, mesmos endereços.
        assert_eq!(
            degrau_de_agora, degrau_de_antes,
            "o ponto de encontro fora do ar mudou o degrau da escada"
        );
        // As máquinas, e não as portas: cada `iniciar` pediu porta zero e o
        // sistema deu uma diferente. O que a comparação cobra é que nenhum
        // **endereço** tenha sumido do link.
        let candidatos = |texto: &str| {
            seele_proto::uri::analisar(texto)
                .expect("o convite não se lê de volta")
                .enderecos()
                .expect("um endereço do convite não se separa")
                .into_iter()
                .map(|alvo| alvo.maquina.to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            candidatos(&convite_de_agora),
            candidatos(&convite_de_antes),
            "o ponto de encontro fora do ar tirou endereços do convite"
        );
        assert!(
            !convite_de_agora.contains("enc="),
            "o convite levou um bilhete para um ponto de encontro que não respondeu: \
             {convite_de_agora}"
        );

        // E a espera a mais é a que foi escolhida, não uma espera de rede.
        let folga = com_encontro.saturating_sub(sem_encontro);
        assert!(
            folga <= crate::alcance::encontro::PRAZO + std::time::Duration::from_millis(750),
            "hospedar demorou {folga:?} a mais por causa de um ponto de encontro fora do ar"
        );
    }

    #[tokio::test]
    async fn encerrar_libera_a_porta_para_hospedar_de_novo() {
        // Sem degrau 4, e com a trava: ver `crate::ambiente_de_teste`.
        let _ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::sem_degrau_4();
        // Fechar uma conversa e abrir outra é o caso normal, e sem esperar o
        // endpoint terminar isso falha com "endereço já em uso".
        let primeira = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir");
        let porta = primeira.endereco().port();
        primeira.encerrar().await;

        let segunda = Hospedagem::iniciar(porta, Location::Memory, "Casa", None, |_| Ok(())).await;
        assert!(
            segunda.is_ok(),
            "a porta continuou presa: {:?}",
            segunda.err()
        );
    }

    /// Sem `$SEELE_ENCONTRO`, `iniciar` não convoca ponto de encontro nenhum.
    ///
    /// A decisão (d) do lote G-servidor da F5, pelo lado da hospedagem: o
    /// padrão seguro. Antes, este teste reprovava no guarda de `iniciar`, porque
    /// sem a variável a convocação era a do ponto de produção.
    #[tokio::test]
    async fn iniciar_sem_ambiente_nao_convoca_ponto_nenhum() {
        let ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::guardar();
        ambiente.tirar();
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("subir sem ambiente");
        let recusa = hospedagem
            .alcance()
            .and_then(crate::alcance::Alcance::encontro_recusado)
            .map(str::to_owned);
        let convite = hospedagem.convite();
        hospedagem.encerrar().await;
        drop(ambiente);
        assert!(
            recusa.is_none() && !convite.contains("enc="),
            "sem `$SEELE_ENCONTRO`, a hospedagem tentou um ponto de encontro (recusa: {recusa:?}; \
             convite: {convite}): o padrão deixou de ser «nenhum ponto»"
        );
    }

    /// `iniciar_com_o_ponto_padrao`, sem `$SEELE_ENCONTRO`, leva o ponto do
    /// projeto à escada.
    ///
    /// Pelo comportamento, e sem rede: o ponto de produção chega ao guarda de
    /// `iniciar`, que reprova antes da escada. Se a função deixasse de pedir o
    /// ponto padrão, a convocação seria nenhuma, não haveria pânico, e o teste
    /// reprovaria — quem hospeda pelo app perderia o degrau 4 em silêncio.
    #[tokio::test]
    #[should_panic(expected = "(«encontro.seele.app.br», pego em Hospedagem::iniciar")]
    async fn iniciar_com_o_ponto_padrao_leva_o_ponto_do_projeto_a_escada() {
        let ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::guardar();
        ambiente.tirar();
        let _ =
            Hospedagem::iniciar_com_o_ponto_padrao(0, Location::Memory, "Casa", None, |_| Ok(()))
                .await;
    }

    /// O guarda de `iniciar` dispara, e é **ele** que dispara.
    ///
    /// `$SEELE_ENCONTRO` aponta um IP da VPS de produção, e não o nome: nem
    /// com este guarda desfeito há consulta de DNS. Desfeito, o que pega é o
    /// guarda de `resolver`, antes do primeiro pacote, e a frase dele diz
    /// outro lugar — o `expected` não casa, e o teste reprova. Numa máquina
    /// com IPv4 global o degrau 4 nem é tentado, e o teste reprova por não
    /// haver pânico. Nos dois casos nenhum pacote sai para a VPS.
    #[tokio::test]
    #[should_panic(expected = "pego em Hospedagem::iniciar, antes da escada")]
    async fn o_guarda_de_iniciar_pega_o_ponto_de_producao_antes_da_escada() {
        let ambiente = crate::ambiente_de_teste::AmbienteDoEncontro::guardar();
        ambiente.pedir(crate::alcance::encontro::REDE_DO_PADRAO[1]);
        let _ = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(())).await;
    }
}
