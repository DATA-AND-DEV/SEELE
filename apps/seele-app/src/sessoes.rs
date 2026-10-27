//! **As sessões cliente desta janela** (E3 da 0.17, multi-sessão).
//!
//! Até a 0.16 a janela tinha uma conexão só: `Session::connection` era uma
//! vaga, e `connect` recusava com `AlreadyConnected`. Hospedar já era
//! independente (`Session::hospedagens`), mas conversar em dois servidores ao
//! mesmo tempo não existia.
//!
//! Aqui moram todas as sessões, **iguais entre si**:
//!
//! - cada uma tem o seu [`seele_ffi::Carimbo`] (o `SessaoId` dela, que nasce na
//!   tentativa e nunca se repete, e a geração, que sobe a cada reconexão);
//! - todo comando de sessão chega com o carimbo, e é pelo carimbo que se acha a
//!   conexão ([`Registro::conexao`]), nunca por «a que está na tela»;
//! - todo evento sai carimbado (`Bridge` em `main.rs`);
//! - **a vista** ([`Registro::vista`]) é só qual delas a janela mostra. Ela não
//!   roteia comando de conversa nenhum; serve aos MODs (que são da vista) e às
//!   não lidas.
//!
//! O microfone é da máquina, e quem decide qual sessão o tem é o
//! [`seele_ffi::foco::FocoDeVoz`] daqui ([`Registro::foco`]).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use seele_ffi::{Carimbo, Connection, ConnectionError, EndReason};

/// O carimbo como a janela o manda: os dois números.
///
/// Um tipo próprio porque o [`seele_ffi::Carimbo`] só sai (é `Serialize`): o que
/// entra da janela é conferido contra o da conexão em [`Registro::conexao`], e
/// nunca vira um carimbo da FFI sem passar por ali.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct CarimboDaJanela {
    /// O `SessaoId`.
    pub(crate) sessao: u64,
    /// A geração que a janela viu por último.
    pub(crate) geracao: u64,
}

impl CarimboDaJanela {
    /// O carimbo que uma entrada devolveu, como a janela o manda de volta.
    #[cfg(test)]
    pub(crate) fn da(entrada: &crate::Entrada) -> Self {
        Self::de(entrada.carimbo)
    }

    /// O carimbo de uma conexão, como a janela o manda de volta.
    #[cfg(test)]
    pub(crate) fn de(carimbo: Carimbo) -> Self {
        Self {
            sessao: carimbo.sessao,
            geracao: carimbo.geracao,
        }
    }
}

impl From<CarimboDaJanela> for Carimbo {
    fn from(carimbo: CarimboDaJanela) -> Self {
        Self {
            sessao: carimbo.sessao,
            geracao: carimbo.geracao,
        }
    }
}

/// O que a ponte de eventos de uma sessão escreve, e o registro lê.
#[derive(Debug)]
pub(crate) struct Marcas {
    /// O `SessaoId`, fixo desde a tentativa.
    pub(crate) sessao: u64,
    /// A sessão ainda vale: está no registro, ou é a tentativa em voo. Falso
    /// depois de sair, ou de ser substituída: os eventos dela passam a ser
    /// descartados, e contados.
    pub(crate) viva: AtomicBool,
    /// Alguma coisa da conversa mudou enquanto ela não estava na vista.
    pub(crate) mudou: AtomicBool,
    /// O servidor encerrou a sessão, e por quê. Ela fica no registro assim até a
    /// pessoa a dispensar: nada some calado.
    pub(crate) encerrada: Mutex<Option<EndReason>>,
    /// A última geração que a ponte viu. Quando ela sobe, foi uma reconexão.
    pub(crate) geracao_vista: AtomicU64,
    /// Quantas reconexões a ponte viu.
    pub(crate) reconexoes: AtomicU64,
    /// Quantos eventos desta sessão chegaram depois de ela deixar de valer.
    pub(crate) descartados: AtomicU64,
}

impl Marcas {
    /// As marcas de uma tentativa que acabou de começar.
    pub(crate) fn nova(sessao: u64) -> Arc<Self> {
        Arc::new(Self {
            sessao,
            viva: AtomicBool::new(true),
            mudou: AtomicBool::new(false),
            encerrada: Mutex::new(None),
            geracao_vista: AtomicU64::new(1),
            reconexoes: AtomicU64::new(0),
            descartados: AtomicU64::new(0),
        })
    }

    /// O motivo do fim, se o servidor encerrou.
    pub(crate) fn encerrada(&self) -> Option<EndReason> {
        *self
            .encerrada
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Anota a geração de um evento; devolve se ela subiu (uma reconexão).
    pub(crate) fn anotar_geracao(&self, geracao: u64) -> bool {
        let antes = self.geracao_vista.fetch_max(geracao, Ordering::AcqRel);
        if geracao > antes {
            self.reconexoes.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

/// Uma sessão cliente conectada.
pub(crate) struct SessaoCliente {
    /// A conexão. O `Arc` é clonado para fora do cadeado antes de qualquer
    /// espera, como `Session::connection` sempre fez.
    pub(crate) conexao: Arc<Connection>,
    /// Por onde ela entrou, sempre (o «onde estou»).
    pub(crate) endereco: String,
    /// «Onde voltar»: `None` num servidor desta máquina, que não entra na
    /// lista de visitados. Decidido depois de entrar (`ligar::anotar_a_chegada`).
    alvo: Mutex<Option<String>>,
    /// O que a ponte de eventos dela escreve.
    pub(crate) marcas: Arc<Marcas>,
    /// A maior mensagem confirmada que a pessoa já viu nesta sessão: é a régua
    /// das não lidas.
    pub(crate) vista_ate: AtomicU64,
    /// A ordem de entrada, para a trilha desenhar sempre na mesma ordem.
    pub(crate) ordem: u64,
}

impl SessaoCliente {
    /// O `SessaoId`.
    pub(crate) fn id(&self) -> u64 {
        self.marcas.sessao
    }

    /// «Onde voltar», quando há.
    pub(crate) fn alvo(&self) -> Option<String> {
        self.alvo
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Grava «onde voltar».
    pub(crate) fn guardar_alvo(&self, alvo: Option<String>) {
        *self.alvo.lock().unwrap_or_else(PoisonError::into_inner) = alvo;
    }

    /// Marca como vistas as mensagens que a sessão tem agora.
    pub(crate) fn marcar_vista(&self) {
        let maior = self
            .conexao
            .messages()
            .iter()
            .filter(|mensagem| mensagem.id != 0)
            .map(|mensagem| mensagem.id)
            .max()
            .unwrap_or(0);
        self.vista_ate.fetch_max(maior, Ordering::AcqRel);
        self.marcas.mudou.store(false, Ordering::Release);
    }

    /// Quantas mensagens confirmadas, de outras pessoas, no canal aberto,
    /// chegaram depois da última vez que a pessoa olhou esta sessão.
    ///
    /// **Do canal aberto, e só dele**: a FFI guarda a conversa do canal que
    /// está aberto (`Connection::messages`). Uma mensagem num canal fechado de
    /// uma sessão do fundo não entra nesta conta; ela acende `mudou` se chegou
    /// pelo `MessagesChanged`, e a frase da placa diz «mudou», não um número.
    pub(crate) fn nao_lidas(&self) -> u32 {
        let desde = self.vista_ate.load(Ordering::Acquire);
        let contadas = self
            .conexao
            .messages()
            .iter()
            .filter(|mensagem| {
                mensagem.id > desde
                    && !mensagem.own
                    && mensagem.estado == seele_ffi::EstadoDoEnvio::Confirmada
            })
            .count();
        u32::try_from(contadas).unwrap_or(u32::MAX)
    }
}

/// As sessões, a vista e o foco de voz desta janela.
pub(crate) struct Registro {
    sessoes: Mutex<BTreeMap<u64, Arc<SessaoCliente>>>,
    /// Qual sessão a janela mostra; 0 é nenhuma. Atômico porque a ponte de
    /// eventos de cada sessão o lê na thread do driver dela.
    vista: Arc<AtomicU64>,
    /// A ordem de entrada da próxima sessão.
    proxima_ordem: AtomicU64,
    /// O microfone desta máquina.
    foco: Arc<seele_ffi::foco::FocoDeVoz>,
    /// Quantos comandos foram recusados por um carimbo que não era o de agora.
    pub(crate) comandos_recusados: AtomicU64,
}

impl Default for Registro {
    fn default() -> Self {
        Self {
            sessoes: Mutex::new(BTreeMap::new()),
            vista: Arc::new(AtomicU64::new(0)),
            proxima_ordem: AtomicU64::new(1),
            foco: seele_ffi::foco::FocoDeVoz::novo(),
            comandos_recusados: AtomicU64::new(0),
        }
    }
}

impl Registro {
    /// O foco de voz da máquina.
    pub(crate) fn foco(&self) -> Arc<seele_ffi::foco::FocoDeVoz> {
        Arc::clone(&self.foco)
    }

    /// A vista, como a ponte de eventos a lê.
    pub(crate) fn vista_atomica(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.vista)
    }

    /// Qual sessão a janela mostra.
    pub(crate) fn vista(&self) -> Option<u64> {
        match self.vista.load(Ordering::Acquire) {
            0 => None,
            sessao => Some(sessao),
        }
    }

    /// Muda a vista. A que sai tem as mensagens dela marcadas como vistas (a
    /// pessoa estava olhando), e a que entra também.
    pub(crate) fn por_na_vista(&self, sessao: Option<u64>) {
        let antes = self.vista.swap(sessao.unwrap_or(0), Ordering::AcqRel);
        for id in [Some(antes), sessao].into_iter().flatten() {
            if let Some(sessao) = self.buscar(id) {
                sessao.marcar_vista();
            }
        }
    }

    /// Guarda uma sessão que acabou de entrar.
    pub(crate) fn inserir(
        &self,
        conexao: Arc<Connection>,
        endereco: String,
        alvo: Option<String>,
        marcas: Arc<Marcas>,
    ) -> Arc<SessaoCliente> {
        let sessao = Arc::new(SessaoCliente {
            conexao,
            endereco,
            alvo: Mutex::new(alvo),
            marcas,
            vista_ate: AtomicU64::new(0),
            ordem: self.proxima_ordem.fetch_add(1, Ordering::Relaxed),
        });
        self.sessoes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(sessao.id(), Arc::clone(&sessao));
        sessao
    }

    /// Tira uma sessão do registro. Os eventos dela passam a ser descartados.
    /// Se ela era a vista, a vista fica vazia.
    pub(crate) fn tirar(&self, sessao: u64) -> Option<Arc<SessaoCliente>> {
        let tirada = self
            .sessoes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&sessao);
        if let Some(tirada) = &tirada {
            tirada.marcas.viva.store(false, Ordering::Release);
        }
        let _ = self
            .vista
            .compare_exchange(sessao, 0, Ordering::AcqRel, Ordering::Acquire);
        tirada
    }

    /// Uma sessão pelo `SessaoId`.
    pub(crate) fn buscar(&self, sessao: u64) -> Option<Arc<SessaoCliente>> {
        self.sessoes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&sessao)
            .cloned()
    }

    /// Todas, na ordem de entrada.
    pub(crate) fn todas(&self) -> Vec<Arc<SessaoCliente>> {
        let mut todas: Vec<_> = self
            .sessoes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        todas.sort_by_key(|sessao| sessao.ordem);
        todas
    }

    /// **A conexão de um comando**, pelo carimbo que ele trouxe.
    ///
    /// Nunca «a que está na tela»: um clique feito na tela de A, que chega
    /// depois de a vista ter ido para B, vai para A — ou é recusado, se A já
    /// saiu ou reconectou depois do clique. Nunca para B.
    ///
    /// # Errors
    ///
    /// - [`ConnectionError::NotConnected`]: não há sessão com esse número (saiu,
    ///   ou nunca existiu);
    /// - [`ConnectionError::SessaoNaoConfere`]: a sessão existe e o carimbo é
    ///   de antes de uma reconexão dela.
    ///
    /// As duas recusas são contadas em `comandos_recusados`.
    pub(crate) fn conexao(
        &self,
        carimbo: CarimboDaJanela,
    ) -> Result<Arc<Connection>, ConnectionError> {
        let Some(sessao) = self.buscar(carimbo.sessao) else {
            self.comandos_recusados.fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                sessao = carimbo.sessao,
                "um comando chegou para uma sessão que já não existe: recusado, e nada saiu"
            );
            return Err(ConnectionError::NotConnected);
        };
        match sessao.conexao.com(carimbo.into()) {
            Ok(_) => Ok(Arc::clone(&sessao.conexao)),
            Err(recusa) => {
                self.comandos_recusados.fetch_add(1, Ordering::Relaxed);
                Err(recusa)
            }
        }
    }

    /// A sessão que tem o foco de voz agora, se há.
    pub(crate) fn com_o_foco(&self) -> Option<Arc<SessaoCliente>> {
        let dono = self.foco.dono()?;
        self.todas()
            .into_iter()
            .find(|sessao| sessao.conexao.id_no_foco() == Some(dono))
    }

    /// As conexões vivas, para os controles que são da máquina (mudo,
    /// isolamento, modo de voz, supressão, sensibilidade, aparelhos).
    pub(crate) fn conexoes(&self) -> Vec<Arc<Connection>> {
        self.todas()
            .into_iter()
            .map(|sessao| Arc::clone(&sessao.conexao))
            .collect()
    }
}

/// **Uma resposta que diz de qual sessão é** (E3): a janela a aplica à sessão
/// do carimbo, e não à que estiver na tela quando a promessa resolver.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct NaSessao<T> {
    /// O carimbo da sessão que respondeu, no instante da resposta.
    pub(crate) carimbo: Carimbo,
    /// A época da vista no instante da resposta (a `geracao` da [`crate::Entrada`]
    /// de `ver_sessao`/`connect`). Sobe a cada troca de vista e nunca desce: a
    /// janela descarta, para a tela, a resposta de uma época que já passou.
    pub(crate) epoca: u64,
    /// A resposta.
    pub(crate) valor: T,
}

impl<T> NaSessao<T> {
    /// A resposta de `conexao`, carimbada com o carimbo dela e a época agora.
    pub(crate) fn de(conexao: &Connection, epoca: u64, valor: T) -> Self {
        Self {
            carimbo: conexao.carimbo(),
            epoca,
            valor,
        }
    }
}

/// **O retrato, com o carimbo junto** (E3). Achatado: os campos do `Snapshot`
/// continuam no topo, como as funções de desenho os leem, e o `carimbo` entra
/// ao lado deles. Uma janela que ainda não lê o carimbo desenha igual; a que lê
/// sabe de quem é o retrato que chegou.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct RetratoNaSessao {
    /// O carimbo da sessão do retrato.
    pub(crate) carimbo: Carimbo,
    /// A época da vista no instante do retrato. Ver [`NaSessao::epoca`].
    pub(crate) epoca: u64,
    /// O retrato.
    #[serde(flatten)]
    pub(crate) retrato: seele_ffi::Snapshot,
}

/// Onde a sessão está, em relação às hospedagens deste app: o mesmo nome que
/// `onde_estou` dá à janela, por sessão.
pub(crate) type Onde = crate::OndeEstou;

/// O estado de uma sessão, como a trilha o desenha.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct SessaoNaTela {
    /// O carimbo de agora: é com ele que a janela manda comandos a esta sessão.
    pub(crate) carimbo: Carimbo,
    /// Por onde entrou.
    pub(crate) endereco: String,
    /// O nome do servidor, quando ele disse um.
    pub(crate) nome: Option<String>,
    /// É a que a janela mostra.
    pub(crate) na_vista: bool,
    /// É a que tem o microfone desta máquina.
    pub(crate) foco_de_voz: bool,
    /// A sala de voz em que está, quando está.
    pub(crate) na_sala_de_voz: Option<String>,
    /// Pela chave: é um servidor que esta máquina hospeda? `Pessoal` é a sala
    /// pessoal desta máquina, a de quem recebe uma ligação.
    pub(crate) onde: Onde,
    /// **A ligação pessoal feita daqui**: o código que a ponte discou, quando
    /// esta sessão foi aberta pelo `@CÓDIGO` (`Connection::codigo_da_ligacao`).
    /// `None` para um servidor. Vem da conexão, e nunca do nome nem do
    /// endereço.
    pub(crate) ligacao: Option<String>,
    /// `online`, `reconectando` ou `encerrada`.
    pub(crate) enlace: &'static str,
    /// O motivo do fim, pelo nome, quando o servidor encerrou.
    pub(crate) encerrada: Option<EndReason>,
    /// Mensagens novas no canal aberto desde a última vista.
    pub(crate) nao_lidas: u32,
    /// Alguma coisa da conversa mudou desde a última vista.
    pub(crate) mudou: bool,
}

impl SessaoCliente {
    /// O estado desta sessão para a trilha. `onde` vem de quem conhece as
    /// hospedagens.
    pub(crate) fn na_tela(&self, vista: Option<u64>, onde: Onde) -> SessaoNaTela {
        let retrato = self.conexao.snapshot();
        let encerrada = self.marcas.encerrada();
        let na_vista = vista == Some(self.id());
        let enlace = if encerrada.is_some() {
            "encerrada"
        } else {
            match retrato.link {
                seele_ffi::LinkState::Online => "online",
                seele_ffi::LinkState::InternalBattery { .. } => "reconectando",
            }
        };
        SessaoNaTela {
            carimbo: self.conexao.carimbo(),
            endereco: self.endereco.clone(),
            nome: Some(retrato.server.clone()).filter(|nome| !nome.trim().is_empty()),
            na_vista,
            foco_de_voz: self.conexao.id_no_foco().is_some() && self.conexao.tem_o_foco_de_voz(),
            na_sala_de_voz: retrato
                .voice_rooms
                .iter()
                .find(|sala| sala.occupied_by_us)
                .map(|sala| sala.name.clone()),
            onde,
            ligacao: self
                .conexao
                .codigo_da_ligacao()
                .map(|codigo| codigo.to_string()),
            enlace,
            encerrada,
            nao_lidas: if na_vista { 0 } else { self.nao_lidas() },
            mudou: !na_vista && self.marcas.mudou.load(Ordering::Acquire),
        }
    }
}

/// **Um pedido de troca de vista** (`ver_sessao`), no domínio da janela: a
/// instância da janela que o backend emitiu na carga dela
/// ([`DominioDaVista::abrir_instancia`]) e o número do pedido dentro dela.
///
/// Substitui o `Date.now() + época` (o Codex mediu: depois de recarregar, ou
/// com o relógio recuando, um pedido novo saía **menor** que o último, e o
/// backend recusava pedidos válidos). Nada aqui depende de relógio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct PedidoDeVista {
    /// A instância da janela, emitida pelo backend; só sobe no processo.
    pub(crate) instancia: u64,
    /// O número do pedido nesta instância; a janela o sobe a cada clique.
    pub(crate) n: u64,
}

/// **A ordem dos pedidos de vista**: a instância mais recente da janela, e o
/// maior `n` já aplicado nela.
///
/// - Uma instância nova (a janela recarregou) recomeça a contar, e toda
///   instância anterior passa a ser velha: um pedido em voo da página de antes
///   do reload, chegando depois, é recusado.
/// - Dentro da instância, só um `n` maior que o último é aplicado: dois
///   cliques que chegam fora de ordem não levam a vista para trás.
#[derive(Debug, Default)]
pub(crate) struct DominioDaVista {
    /// `(a última instância emitida, a instância em vigor, o maior n nela)`.
    estado: Mutex<(u64, u64, u64)>,
}

impl DominioDaVista {
    /// Emite a instância de uma janela que acabou de carregar.
    pub(crate) fn abrir_instancia(&self) -> u64 {
        let mut estado = self.estado.lock().unwrap_or_else(PoisonError::into_inner);
        estado.0 = estado.0.saturating_add(1);
        *estado = (estado.0, estado.0, 0);
        estado.0
    }

    /// Confere e anota um pedido, sob o cadeado: aplicado (`Ok`) ou velho, com
    /// o último aplicado (`Err`).
    ///
    /// # Errors
    ///
    /// O pedido é de uma instância que não é mais a da janela, ou não é maior
    /// que o último aplicado nela.
    pub(crate) fn conferir(&self, pedido: PedidoDeVista) -> Result<(), PedidoDeVista> {
        let mut estado = self.estado.lock().unwrap_or_else(PoisonError::into_inner);
        let (_, em_vigor, ultimo) = *estado;
        let ultimo_aplicado = PedidoDeVista {
            instancia: em_vigor,
            n: ultimo,
        };
        if pedido.instancia != em_vigor || pedido.n <= ultimo {
            return Err(ultimo_aplicado);
        }
        estado.2 = pedido.n;
        Ok(())
    }
}

#[cfg(test)]
mod a_ordem_dos_pedidos_de_vista {
    use super::{DominioDaVista, PedidoDeVista};

    fn pedido(instancia: u64, n: u64) -> PedidoDeVista {
        PedidoDeVista { instancia, n }
    }

    /// Fora de ordem, recarregar, e o pedido em voo da página de antes.
    #[test]
    fn so_o_pedido_mais_novo_da_instancia_em_vigor_passa() {
        let dominio = DominioDaVista::default();
        let primeira = dominio.abrir_instancia();
        assert_eq!(dominio.conferir(pedido(primeira, 2)), Ok(()));
        assert_eq!(
            dominio.conferir(pedido(primeira, 1)),
            Err(pedido(primeira, 2)),
            "o pedido 1, chegando depois do 2, passou"
        );
        assert!(
            dominio.conferir(pedido(primeira, 2)).is_err(),
            "o mesmo pedido passou duas vezes"
        );
        // A janela recarrega: o contador dela volta a 1, e passa.
        let segunda = dominio.abrir_instancia();
        assert!(segunda > primeira);
        assert_eq!(
            dominio.conferir(pedido(segunda, 1)),
            Ok(()),
            "depois de recarregar, o primeiro pedido da página nova foi recusado"
        );
        // O pedido em voo da página de antes, chegando agora: recusado.
        assert!(
            dominio.conferir(pedido(primeira, 99)).is_err(),
            "um pedido da página de antes do reload passou depois dela"
        );
        // Uma instância que ninguém emitiu também não passa.
        assert!(dominio.conferir(pedido(segunda + 7, 1)).is_err());
        assert_eq!(dominio.conferir(pedido(segunda, 2)), Ok(()));
    }
}

#[cfg(test)]
mod testes {
    use super::Marcas;

    /// A geração que sobe é uma reconexão; a repetida e a menor não são.
    #[test]
    fn so_a_geracao_que_sobe_conta_como_reconexao() {
        let marcas = Marcas::nova(7);
        assert!(!marcas.anotar_geracao(1));
        assert!(marcas.anotar_geracao(2));
        assert!(!marcas.anotar_geracao(2));
        assert!(!marcas.anotar_geracao(1));
        assert_eq!(
            marcas.reconexoes.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }
}
