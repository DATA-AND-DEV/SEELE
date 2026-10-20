//! The desktop client — a Tauri shell over [`seele_ffi`].
//!
//! `specs/06-clientes-gui.md` sets the shape of this file in one sentence:
//! "Nenhuma lógica de protocolo em JavaScript. Se o frontend precisa saber o
//! que é um `ssrc`, algo está errado." So the frontend gets a `Snapshot` and
//! sends back verbs — enter this voice room, say this, mute — and every one of them
//! is a call straight through to the FFI.
//!
//! Nothing here decides anything either. If a command in this file grows a
//! judgement, it belongs in `seele-core`, and the terminal client would have had
//! to grow the same one.
//!
//! # Threading
//!
//! [`seele_ffi::Connection::connect`] blocks, so it runs on a blocking thread. Events
//! arrive on the FFI's driver thread; [`Bridge`] is what marshals them onto the
//! webview, which is the "a casca marshala para sua thread de UI" the spec asks
//! for.

// A desktop shell with no window is not a desktop shell. The attribute keeps
// the console from opening behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// `specs/10-convencoes.md`: fora de teste não há `unwrap`/`expect`. Dentro, um
// `expect` com mensagem é mais legível que um `match` que só pode ir para um
// lado, e uma falha ali é falha do teste e não do produto. Mesma linha, mesma
// razão, que a de `seele-video`.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod catalogo;
// **O executor de MODs do cliente em QuickJS — experimento de bancada.**
//
// Etapa E1: o Worker de `blob:` não satisfaz o contrato — a sonda releu, num
// aplicativo reiniciado, uma marca que um MOD gravou em IndexedDB.
//
// Ele é compilado sempre e **ligado por `SEELE_EXECUTOR=quickjs`**, que é uma
// chave de bancada e não uma opção do produto. A restrição da diretriz é «não
// manter dois executores públicos por precaução», e esta não é pública: sem a
// variável, o produto sobe MODs pelo Worker como sempre, e nada nesta janela
// oferece a troca a quem usa.
//
// A chave existe porque medir interação, descarte e impacto na voz exige o
// aplicativo de verdade — e um protótipo que só roda em teste não mede nada
// disso. Ela sai no dia em que a decisão de E1 for tomada, junto com o
// executor que perder.
mod executor;
mod firewall;
mod icone;
mod ligar;
mod mods;
mod reducao;
mod sala;
mod servidores;
mod toque;
mod versoes;

use std::sync::{Arc, Mutex};

use seele_ffi::{
    ChannelWeight, ConnectConfig, ConnectFailure, Connection, ConnectionError, Event,
    EventListener, NotSavedReason, Preview, PreviewRules, SaveDestination, SaveRefused, Snapshot,
    VoiceMode,
};
use tauri::{AppHandle, Emitter, Manager, State};

/// The name the webview listens on.
///
/// One channel rather than one per variant: the payload already says which
/// [`Event`] it is, and a frontend subscribing to seven names would drift from
/// this list the first time one is added.
const EVENT_CHANNEL: &str = "seele://event";

/// O canal por onde o download de uma atualização diz por onde vai.
///
/// Separado do [`EVENT_CHANNEL`] de propósito: aquele carrega `Event`, que é o
/// que a FFI emite sobre a conversa, e um andamento de download não é um evento
/// de sessão. Quem estiver ouvindo a conversa não deve ter que peneirar bytes
/// baixados, e quem baixa não precisa estar em sessão nenhuma.
const CANAL_DE_ATUALIZACAO: &str = "seele://atualizacao";

/// Everything the commands share.
#[derive(Default)]
struct Session {
    /// Com que intenção este processo foi aberto, se com alguma.
    ///
    /// É por onde o launcher fala com a versão que ele abriu: `--hospedar`, e
    /// o `--nome-publico` que o acompanha. Lida uma vez no arranque e nunca
    /// mais — argumentos de linha de comando não mudam durante a execução, e um
    /// campo que os relesse esconderia isso de quem lê.
    ///
    /// **Uma versão que não conhece estes argumentos os ignora**, e é o que faz
    /// o launcher valer também para as publicadas antes dele. Ela abre; quem
    /// aperta o botão é a pessoa.
    abertura: Abertura,
    /// **Qual execução de sessão é esta.** Etapa E2 do contrato de API própria.
    ///
    /// Sobe a cada tentativa de conexão e a cada desmontagem, e nunca desce.
    /// Serve para uma coisa só, e ela é a espinha de todo o resto: dizer se um
    /// trabalho que está terminando agora pertence à sessão que está de pé
    /// agora.
    ///
    /// # Por que um número, e não «tem conexão?»
    ///
    /// Porque as duas perguntas têm respostas diferentes no momento que
    /// importa. Entrar de novo no **mesmo** servidor, depois de sair, deixa o
    /// slot de conexão cheio outra vez — e um pedido da visita anterior que
    /// chegasse atrasado encontraria uma conexão viva e seria atendido, contra
    /// a sessão errada. O contrato escreve isso: «entrar de novo no mesmo
    /// servidor cria uma geração nova».
    ///
    /// **Zero é «nenhuma sessão»**, e é onde ela começa. Zero também é o que
    /// uma casca manda quando não sabe de que sessão fala, e as duas coisas
    /// querem dizer a mesma: nada é admitido em nome dela. A primeira tentativa
    /// de conexão a leva a 1.
    geracao: Arc<std::sync::atomic::AtomicU64>,
    /// Quantos eventos foram descartados por pertencerem a uma geração morta.
    ///
    /// Contador de diagnóstico — o contrato pede «contadores por instância e
    /// geração, acessíveis na bancada». Ele existe para que «não sobrou nada»
    /// deixe de ser inspeção visual: se ele sobe depois de uma saída, alguma
    /// coisa da sessão anterior ainda estava falando.
    eventos_de_geracao_morta: Arc<std::sync::atomic::AtomicU64>,
    /// Quantos comandos foram recusados por virem de uma geração morta.
    comandos_de_geracao_morta: Arc<std::sync::atomic::AtomicU64>,
    /// Os MODs de pé no executor nativo — experimento de bancada da etapa E1.
    ///
    /// A chave é o número da instância, e a instância é dona do runtime dela.
    ///
    /// **Aqui e não na janela** porque o runtime é nativo: quem o desmonta tem
    /// de ser quem o criou, e a janela pode desaparecer antes de pedir.
    mods_nativos: Mutex<ModsNativos>,
    /// O último motivo por que cada MOD não carregou, como o `seele.log` o
    /// disse. É o que faz uma recusa repetida a cada quatro segundos sair uma
    /// vez: ver [`recusa_de_carga_dita`]. Esvaziada a cada sessão que fecha
    /// ([`Session::revogar`]).
    recusas_de_carga: RecusasDeCarga,
    /// **O teto de vazão de cada MOD no `seele.log`**: as linhas que o produto
    /// escreve em nome de um MOD — a frase da janela, a mídia recusada, a
    /// servida e o som servido — passam pelo balde dele. Ver
    /// [`linha_admitida`]. Esvaziado, com a conta dita, a cada sessão que fecha
    /// ([`Session::revogar`]).
    baldes_do_registro: BaldesDoRegistro,

    /// Os arquivos que **uma pessoa escolheu** para um MOD, por identificador.
    ///
    /// # Por que o MOD não recebe o arquivo
    ///
    /// Porque nenhum cliente do SEELE abre arquivo por conta de terceiro, e um
    /// MOD é terceiro. O que ele recebe é um número e o que o produto **provou**
    /// sobre os bytes — o tipo, saído deles, e o tamanho. Os bytes ficam aqui, e
    /// saem em pedaços, sob pedido, com teto.
    ///
    /// É o que a diretriz de 19/09 chama de mediada, com identificador e
    /// cancelamento: o caminho no disco nunca atravessa, e soltar é uma linha
    /// que quem escreveu o MOD chama — ou que a saída da sessão chama por ele.
    arquivos_escolhidos: Mutex<ArquivosEscolhidos>,

    connection: Mutex<Option<Arc<Connection>>>,
    /// As hospedagens deste app: o servidor guardado e a sala pessoal (Plano 2).
    ///
    /// Vivem aqui e não numa variável local porque têm que sobreviver ao
    /// comando que as criou: cada uma fica de pé enquanto a janela estiver
    /// aberta, e independente da sessão. Ver [`Hospedagens`].
    hospedagens: Mutex<Hospedagens>,
    /// Como está a sala pessoal: subindo, no ar (e se o código está achável),
    /// ou por que não subiu. Guardado aqui para a tela que abre depois do
    /// anúncio por [`sala::CANAL_DA_SALA`]; ver [`estado_da_sala`].
    sala: Mutex<sala::EstadoDaSala>,
    /// Quem está tocando na sala pessoal agora (tarefa 24 da F7): a faixa da
    /// tela que abre com o toque já tocando, e o toque de quem se atende.
    toques: Mutex<toque::Toques>,
    /// **A despedida da saída já rodou** (tarefa 21 da F7).
    ///
    /// A saída comum, a da última janela fechada, despede e só então sai; e o
    /// runtime ainda manda o `RunEvent::Exit` depois dela. Sem esta marca, a
    /// despedida do fim ([`despedir_no_fim`]) despediria de novo uma sessão já
    /// despedida, e o `seele.log` contaria duas saídas.
    despedida_feita: std::sync::atomic::AtomicBool,
    /// **Qual servidor guardado está de pé**, quando há um.
    ///
    /// Plano de isolamento de 18/09: «gestão indica servidor-alvo… nunca
    /// inferir o destino apenas de *estou hospedando*». A tela dizia «ligar e
    /// desligar MOD é do servidor que esta janela hospeda» sem dizer **qual** —
    /// e com a lista de servidores, quem tem dois não tem como saber em qual
    /// está mexendo.
    ///
    /// `None` com hospedagem de pé é o legado: a máquina que hospedava antes de
    /// haver lista, e cujo banco é o de sempre.
    servidor_hospedado: Mutex<Option<String>>,
    /// A busca corrente. O cursor é estado de sessão, e é o que impede a regra
    /// de dar-a-volta de ser reescrita em JavaScript.
    busca: Mutex<Option<seele_ffi::search::Search>>,
    /// O teste de microfone em curso, se há um. F01.
    ///
    /// **Aqui e não numa sessão de voz**, pela razão que
    /// `seele_ffi::TesteDeMicrofone` escreve: testar o microfone é algo que se faz
    /// antes de conectar pelo menos tanto quanto durante, e pendurá-lo numa sessão
    /// poria o controle atrás da porta que ele existe para abrir.
    ///
    /// `None` fecha o teste: o `Drop` da alça manda parar a thread e espera.
    teste_de_microfone: Mutex<Option<seele_ffi::TesteDeMicrofone>>,
    /// O endereço com que esta sessão entrou, como a lista de visitados o
    /// conhece.
    ///
    /// Guardado porque a aparência do servidor — nome e imagem — só chega **depois**
    /// do aperto de mão, e quem a anota precisa saber sob qual chave. A
    /// alternativa era a tela devolver o endereço, e aí a chave da lista
    /// passaria a depender de o JavaScript normalizar a string do mesmo jeito
    /// que o Rust normalizou.
    ///
    /// `None` quando não há sessão, e também quando o servidor é hospedado aqui:
    /// esse não entra na lista de para-onde-voltar, e anotar sobre ele seria
    /// escrever numa entrada que não existe.
    /// O endereço desta sessão, **sempre** — inclusive hospedando aqui.
    ///
    /// Irmão de `alvo`, e a diferença é o motivo de existirem dois. `alvo` é
    /// «onde voltar»: ele alimenta a lista de visitados, e por isso exclui de
    /// propósito um servidor hospedado nesta máquina — `127.0.0.1` não é lugar
    /// aonde se volta, é o botão HOSPEDAR.
    ///
    /// Este é «onde estou», e um sim precisa dele. O QA de 17/09 achou o
    /// defeito exato: `aplicar_mod` prendia o consentimento a `alvo`, que num
    /// servidor hospedado aqui está vazio — então o aceite não era gravado, e o
    /// diálogo que tinha acabado de prometer «você não vai ser perguntado outra
    /// vez» perguntava de novo na reconexão seguinte.
    ///
    /// Um campo servindo a duas perguntas responde mal a uma delas.
    endereco_da_sessao: Mutex<Option<String>>,
    alvo: Mutex<Option<String>>,
    /// O último `seele://` lido, **inteiro** — impressão digital inclusive.
    ///
    /// A impressão é o que `connect` entrega como
    /// [`seele_ffi::ConnectConfig::expected_fingerprint`]: é ela que transforma
    /// o primeiro contato de cego em verificado, e é a razão de o ADR 0006 ter
    /// fechado — `seele-proto/src/uri.rs` chama `fp` "o motivo principal de isto
    /// existir".
    ///
    /// # Uma decisão que se inverteu
    ///
    /// Este campo dizia que a impressão **não** atravessa a ponte. Era a
    /// resposta certa enquanto este app não sabia conferir nada: sem
    /// `expected_fingerprint` na FFI, mandar a string ao frontend não daria a
    /// ninguém nada para fazer com ela além de reescrever a comparação em
    /// JavaScript, que é exatamente o que `specs/06-clientes-gui.md:19` proíbe.
    ///
    /// O que mudou foi o outro lado: [`seele_ffi::Connection::connect`] devolve o
    /// veredito já decidido, em Rust, com a comparação feita. A string que
    /// atravessa agora não é uma entrada de decisão — é o que uma pessoa lê e
    /// confere por outro canal, do mesmo jeito que o `PinChanged` já mandava as
    /// duas impressões porque a coisa toda é um humano compará-las (ADR 0003).
    /// O frontend continua sem decidir nada.
    convite: Mutex<Option<seele_ffi::uri::Convite>>,
    /// A versão nova que a pessoa acabou de ver, esperando o «instalar».
    ///
    /// Guardada entre os dois comandos porque **quem decide é ela**: procurar
    /// não baixa nada, e instalar não procura de novo. Sem este campo, apertar
    /// «instalar» refaria a consulta, e o que seria instalado poderia não ser o
    /// que estava escrito na tela quando a pessoa leu e concordou.
    ///
    /// Some(_) é a única forma de [`instalar_atualizacao`] ter o que instalar, e
    /// é por isso que ele recusa com [`FalhaAoAtualizar::NadaEscolhido`] em vez
    /// de silenciosamente ir procurar.
    atualizacao: Mutex<Option<tauri_plugin_updater::Update>>,
    /// **A ligação em voo, para o CANCELAR** (tarefa 28 da F7): o
    /// [`seele_ffi::Cancelamento`] do `connect` que está andando, ou nada. Quem
    /// abre e fecha a vaga é o [`ligar::LigacaoEmVoo`].
    ligacao_em_voo: Mutex<Option<Arc<seele_ffi::Cancelamento>>>,
}

/// Com que intenção este processo foi aberto.
///
/// # Por que ela existe
///
/// Porque o launcher abre **outro** processo, e precisa dizer a ele o que a
/// pessoa pediu. Sem isto, escolher «hospedar com a 0.10.5» abriria a 0.10.5
/// parada na tela de entrada, e a pessoa teria de apertar HOSPEDAR AQUI de
/// novo — a tela prometeria uma coisa e faria outra.
///
/// Lida uma vez, no arranque. Nada aqui é segredo e nada aqui é confiável do
/// ponto de vista de segurança: são argumentos que qualquer um pode passar. O
/// que eles disparam é o mesmo botão que a pessoa apertaria com o mouse, e o
/// nome público volta a ser conferido pelo `hospedar` como se tivesse sido
/// digitado.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct Abertura {
    /// A pessoa pediu para hospedar assim que a janela subir.
    hospedar: bool,
    /// O nome público que ela escolheu, quando escolheu.
    nome_publico: Option<String>,
    /// O servidor em que entrar assim que a janela subir, como link inteiro.
    ///
    /// É o coração do ADR 0046: quem clica num `seele://` de um servidor que
    /// roda outra versão tem **aquela** versão aberta, já indo para lá. O link
    /// vai inteiro porque ele carrega a impressão digital e o convite de uso
    /// único — e um link remontado de pedaços é um segundo analisador para
    /// discordar do primeiro.
    entrar: Option<String>,
    /// **Quem abriu esta janela foi o launcher de outra linha.**
    ///
    /// Lido de `SEELE_LANCADOR`, que o `Lancamento` do `seele-lancador` põe
    /// com o valor `1`. Aberta assim, esta versão mora no depósito do
    /// launcher, e o atualizador dela trocaria o app de lá pela versão do topo
    /// do manifesto — outra linha. Então o botão some e o comando recusa (spec
    /// 0.16, D-linha-1). Na 0.16 isto vale para quando uma linha mais nova
    /// abrir a 1.x pelo mesmo caminho.
    pelo_lancador: bool,
}

impl Abertura {
    /// O que a linha de comando deste processo diz.
    ///
    /// **Tolerante de propósito.** Um argumento desconhecido não é erro: uma
    /// versão publicada antes do launcher recebe estes mesmos argumentos e os
    /// ignora, e é isso que faz versões lado a lado funcionarem com o que já
    /// está no mundo. Aqui, a mesma tolerância vale para o caminho oposto — uma
    /// versão futura passando um argumento que esta não conhece.
    fn da_linha_de_comando() -> Self {
        Self::de(
            std::env::args().skip(1),
            std::env::var(seele_lancador::VARIAVEL_DO_LANCADOR)
                .ok()
                .as_deref(),
        )
    }

    /// O mesmo, a partir dos argumentos e do valor de `SEELE_LANCADOR`.
    ///
    /// Separado da leitura do processo para o teste poder dizer o que entra,
    /// sem mexer no ambiente de quem roda a bateria.
    fn de(argumentos: impl IntoIterator<Item = String>, lancador: Option<&str>) -> Self {
        let mut aberta = Self {
            pelo_lancador: lancador == Some("1"),
            ..Self::default()
        };
        let mut argumentos = argumentos.into_iter();
        while let Some(argumento) = argumentos.next() {
            match argumento.as_str() {
                "--hospedar" => aberta.hospedar = true,
                // O valor vem no argumento seguinte, e não colado com `=`: é a
                // forma que o `Lancamento` monta, e aceitar as duas seria
                // aceitar uma que ninguém produz.
                "--nome-publico" => aberta.nome_publico = argumentos.next(),
                "--entrar" => aberta.entrar = argumentos.next(),
                _ => {}
            }
        }
        aberta
    }
}

/// Com que intenção esta janela foi aberta.
///
/// A casca pergunta uma vez, no arranque, e age: é o que faz «HOSPEDAR COM
/// 0.10.5» abrir a 0.10.5 **hospedando**, em vez de abri-la parada na tela de
/// entrada com a pessoa tendo de apertar o botão de novo.
#[tauri::command]
fn abertura(session: State<'_, Session>) -> Abertura {
    session.abertura.clone()
}

impl Session {
    /// Qual execução de sessão está de pé agora.
    fn geracao(&self) -> u64 {
        self.geracao.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Fecha a execução atual e abre a seguinte, devolvendo a nova.
    ///
    /// **Monotônica, e chamada antes dos efeitos.** O §5 do contrato: «a partir
    /// desse ponto, nenhum novo efeito daquela instância é admitido, mesmo que
    /// mensagens já estejam na fila». Por isso ela é a **primeira** linha de
    /// quem desmonta, e não a última: entre revogar e destruir a conexão há
    /// `await`s, e cada um deles é uma janela por onde um trabalho antigo
    /// entraria.
    fn revogar(&self) -> u64 {
        let nova = revogar_em(&self.geracao, &self.mods_nativos);
        // **E os arquivos que alguém escolheu saem junto.** Eles são dez
        // megabytes cada, escolhidos para um MOD que já não está de pé, e
        // guardá-los seria exatamente o que a sonda E1 mediu no Worker: uma
        // coisa de uma sessão sobrevivendo à sessão.
        if let Ok(mut guardados) = self.arquivos_escolhidos.lock() {
            let soltos = guardados.soltar_geracao(nova.saturating_sub(1));
            if soltos > 0 {
                tracing::info!(soltos, "arquivos escolhidos soltos com a sessão");
            }
        }
        // **E a memória das recusas de carga também.** Ela segura a mesma
        // recusa repetida a cada quatro segundos dentro de uma sessão; levada
        // para a seguinte, calava a mesma falha numa entrada nova, e o
        // `seele.log` dela não dizia por que o MOD não carregou.
        if let Ok(mut ditas) = self.recusas_de_carga.lock() {
            ditas.clear();
        }
        // **E os baldes do registro, com a conta dita.**
        self.dizer_as_contas_do_registro();
        nova
    }

    /// **Diz a conta de cada balde do registro e os esvazia.**
    ///
    /// Uma linha por balde que tinha linhas seguradas sem outra depois que as
    /// contasse ([`conta_do_balde_dita`]): sem ela, a conta caía com o balde,
    /// e o `seele.log` mostrava a rajada como se fosse tudo. [`Self::revogar`]
    /// a chama ao fechar a sessão, e a saída do app também passa por ele
    /// ([`despedir_da_sessao`]).
    ///
    /// **O mapa sai do cadeado antes de a primeira linha ser escrita.** Com a
    /// guarda na mão, uma linha lenta no disco seguraria toda porta do
    /// registro, de todo MOD, até terminar.
    fn dizer_as_contas_do_registro(&self) {
        let baldes = std::mem::take(
            &mut *self
                .baldes_do_registro
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for ((classe, chave), mut balde) in baldes {
            if let Some(suprimidas) = balde.pendentes() {
                conta_do_balde_dita(classe, &chave, suprimidas);
            }
        }
    }

    /// Esta geração ainda é a de pé?
    ///
    /// Zero é «a casca não sabe», e responde falso: uma casca que não sabe de
    /// que sessão ela fala não pode ter um efeito admitido em nome de nenhuma.
    fn geracao_vale(&self, geracao: u64) -> bool {
        geracao != 0 && geracao == self.geracao()
    }

    /// Recusa um comando que vem de uma sessão que já acabou, e conta.
    fn confere_geracao(&self, geracao: u64) -> Result<(), ConnectionError> {
        if self.geracao_vale(geracao) {
            return Ok(());
        }
        self.comandos_de_geracao_morta
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::debug!(
            geracao,
            atual = self.geracao(),
            "comando recusado: a sessão dele já acabou"
        );
        Err(ConnectionError::NotConnected)
    }

    /// **Recusa um comando de MOD que vem de uma sessão que já acabou, e
    /// conta**, com a recusa que a janela lê: `sessao-encerrada`.
    ///
    /// É [`Self::confere_geracao`] com o erro dos comandos de MOD: a mesma
    /// conta em `comandos_de_geracao_morta` e a mesma linha DEBUG. Antes dela,
    /// cada comando conferia com um `if` seu, e metade não contava — um zero no
    /// contador não queria dizer que nada da sessão anterior tinha falado.
    fn recusar_se_morta(&self, geracao: u64) -> Result<(), FalhaNoMod> {
        self.confere_geracao(geracao)
            .map_err(|_| FalhaNoMod::Recusado {
                motivo: "sessao-encerrada".to_owned(),
            })
    }

    /// The live handle, or the reason there is none.
    fn connection(&self) -> Result<Arc<Connection>, ConnectionError> {
        self.connection
            .lock()
            .map_err(|_| ConnectionError::NotConnected)?
            .clone()
            .ok_or(ConnectionError::NotConnected)
    }
}

/// Os MODs de pé no executor nativo, com identidade completa.
///
/// # Por que a chave não é o identificador do MOD
///
/// Ela era, e a revisão nomeia o buraco: «um pedido antigo pode atingir a nova
/// instância com o mesmo nome». Sair de um servidor e entrar noutro que exige o
/// **mesmo MOD** repõe o mesmo texto; um encerramento a caminho, disparado pela
/// saída anterior, chegava e matava a instância nova.
///
/// A chave é um número que nasce uma vez e nunca se repete nesta janela. O
/// identificador do MOD, a geração e o hash do pacote viajam **com** ele, e
/// juntos são a identidade que o §3 do contrato pede.
///
/// # Duas tabelas, e não uma
///
/// «Manter registro das instâncias em encerramento fora do mapa de instâncias
/// ativas até a resolução». Numa tabela só, «ativa» passava a querer dizer
/// «ativa ou parando», e quem lê o diagnóstico não tinha como separar as duas.
#[derive(Default)]
struct ModsNativos {
    /// O número da instância seguinte. Nunca volta.
    proxima: u64,
    /// As que ainda podem produzir efeito.
    vivos: std::collections::BTreeMap<u64, InstanciaNativa>,
    /// Quantas falas foram descartadas por chegarem depois da revogação.
    ///
    /// **Contadas, e não engolidas.** Uma fala descartada em silêncio é o
    /// defeito que este repositório mais paga; aqui ela não podia ser entregue,
    /// e quem lê o diagnóstico fica sabendo que houve.
    descartadas: u64,
    /// As que pediram para parar e não confirmaram.
    ///
    /// Saem daqui quando o executor confirma — e não antes. Uma que fica aqui
    /// para sempre é um defeito **visível**, que é o ponto: removê-la ao pedir
    /// tornaria a falha invisível.
    encerrando: std::collections::BTreeMap<u64, InstanciaNativa>,
}

/// Em que ponto do ciclo uma instância nativa está.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
enum EstadoNativo {
    /// Tem identidade e ainda não rodou código.
    ///
    /// **Existe para fechar a corrida de identidade.** A janela precisa do
    /// número **antes** de o MOD poder falar; sem esta etapa, ela ouvia pelo
    /// nome enquanto esperava, e uma instância anterior do mesmo MOD falava
    /// naquela janela.
    Reservada,
    /// Rodou o código e pode produzir efeito.
    Ativa,
    /// Pediu para parar e não confirmou.
    Encerrando,
}

/// Uma fala do MOD esperando a janela colher.
#[derive(Debug, Clone, serde::Serialize)]
struct FalaPendente {
    /// `mensagem`, `falhou` ou `interrompido`.
    tipo: String,
    /// O corpo, quando há.
    corpo: String,
    /// Quantas vezes esta fala se repetiu seguidas.
    ///
    /// **Só faz sentido para aviso.** Um MOD que lança num laço manda o mesmo
    /// texto mil vezes; guardar as mil é guardar novecentas e noventa e nove
    /// cópias de uma informação. Agregado, ele continua dizendo tudo — inclusive
    /// que foram mil.
    vezes: u32,
}

/// Quantos avisos de erro cabem por instância.
///
/// **Separado do teto de mensagens, e pequeno.** Um aviso não reserva crédito
/// na fila de dados — ele nasce de uma falha, e uma falha não pode consumir a
/// cota do que está funcionando. O que o contém é este número e a agregação.
const AVISOS_POR_INSTANCIA: usize = 16;

/// Um MOD de pé no executor nativo.
struct InstanciaNativa {
    /// `autor/nome`.
    id: String,
    /// Em que execução de sessão ela nasceu.
    geracao: u64,
    /// O conteúdo exato que subiu.
    hash: String,
    /// A API que o manifesto deste pacote declara.
    ///
    /// Guardada com a instância e não relida na ativação: quem decide o que
    /// este MOD pode chamar tem de ser a mesma leitura que provou o hash dele.
    api_declarada: u32,
    /// Em que ponto do ciclo ela está.
    estado: EstadoNativo,
    /// O executor.
    executor: executor::ExecutorQuickJs,
    /// As **mensagens** que a janela ainda não colheu.
    ///
    /// Só mensagem entra aqui, e só mensagem reserva crédito. Erros e
    /// interrupções vão para [`Self::avisos`] — a revisão nomeia o porquê:
    /// eles eram enfileirados junto sem nunca terem reservado nada, e colhê-los
    /// devolvia crédito que ninguém tinha tomado.
    ///
    /// **Aqui, e não emitido.** A revisão mede a diferença: «no Tauri 2.11.5
    /// presente no checkout, `webview::emit_js` chama `eval` […]; esse retorno
    /// não representa conclusão do handler JavaScript». Emitir e devolver o
    /// crédito era dar por consumido o que ainda não tinha sido lido.
    ///
    /// Os créditos da fila do executor continuam **retidos** enquanto a fala
    /// está aqui, e só voltam quando a janela a colhe. Com a janela parada, o
    /// MOD bate no teto sem nada crescer.
    pendentes: std::collections::VecDeque<FalaPendente>,
    /// Quantos bytes estão em [`Self::pendentes`].
    bytes_pendentes: usize,
    /// Erros e interrupções, com teto próprio e agregação.
    ///
    /// Eles não reservam crédito de dados e não o devolvem: a contabilidade é
    /// simétrica porque eles ficam **fora** dela. Uma falha do MOD não pode
    /// bloquear mensagens futuras por contabilidade inválida.
    avisos: std::collections::VecDeque<FalaPendente>,
    /// Quantos avisos foram descartados por não caberem.
    avisos_perdidos: u32,
    /// O executor confirmou que parou.
    parou: bool,
    /// O fonte do MOD, enquanto ela estiver só reservada.
    ///
    /// **Aqui, e não numa tabela ao lado.** Ele morava em
    /// `Session::codigos_reservados`, inserido **depois** de a instância ter
    /// sido registrada e o cadeado liberado — e a revogação que caísse no meio
    /// apagava um fonte que ainda não existia, e o comando o inseria depois do
    /// descarte. Sem ativação posterior, ele ficava retido para sempre.
    ///
    /// Guardado dentro da instância, ele entra no descarte pelo mesmo ato que
    /// a descarta. Atômico por construção, e não por cuidado.
    codigo: Option<String>,
    /// Há um aviso de «tem o que colher» pendente para a janela.
    ///
    /// **No máximo um por instância.** A bomba emitia um evento por fala,
    /// inclusive pelas que ela agregava ou descartava — e um evento sem corpo
    /// ainda ocupa memória enquanto a janela não o processa. Agregado, a
    /// transição vazio→não-vazio continua sendo avisada uma vez, e o resto é
    /// silêncio até a janela drenar.
    avisado: bool,
}

impl InstanciaNativa {
    /// Anota um erro ou interrupção, agregando o que se repete.
    ///
    /// **Fora da contabilidade de dados.** Um aviso nasce de uma falha, e uma
    /// falha não pode consumir a cota do que está funcionando — nem devolver
    /// crédito que ela nunca tomou, que é o que a revisão encontrou.
    fn anotar_aviso(&mut self, tipo: &str, corpo: String) {
        // Repetido seguidas vezes vira contagem. Um MOD que lança num laço
        // manda o mesmo texto mil vezes, e guardar as mil é guardar
        // novecentas e noventa e nove cópias da mesma informação.
        if let Some(ultimo) = self.avisos.back_mut() {
            if ultimo.tipo == tipo && ultimo.corpo == corpo {
                ultimo.vezes = ultimo.vezes.saturating_add(1);
                // Agregado: o lugar na cota volta, porque nenhuma entrada nova
                // foi ocupada.
                self.executor.fila().tirar_aviso();
                return;
            }
        }
        if self.avisos.len() >= AVISOS_POR_INSTANCIA {
            // **Perde o mais velho, e conta.** Os avisos recentes são os que
            // explicam o que está acontecendo agora; e o contador diz que
            // houve mais do que cabe, para ninguém ler a lista como completa.
            self.avisos.pop_front();
            self.executor.fila().tirar_aviso();
            self.avisos_perdidos = self.avisos_perdidos.saturating_add(1);
        }
        self.avisos.push_back(FalaPendente {
            tipo: tipo.to_owned(),
            corpo,
            vezes: 1,
        });
    }

    /// Guarda uma fala vinda do executor, no lugar certo.
    ///
    /// **Mensagem e aviso não são a mesma coisa.** A mensagem reservou crédito
    /// na fila do executor e por isso entra na contabilidade; o aviso nasce de
    /// uma falha, nunca reservou nada, e entra numa lista própria com teto e
    /// agregação. Misturá-los era o que fazia colher um erro devolver crédito
    /// que ninguém tomou.
    fn guardar_fala(&mut self, tipo: &str, corpo: String) {
        if tipo == "mensagem" {
            self.bytes_pendentes = self.bytes_pendentes.saturating_add(corpo.len());
            self.pendentes.push_back(FalaPendente {
                tipo: tipo.to_owned(),
                corpo,
                vezes: 1,
            });
        } else {
            self.anotar_aviso(tipo, corpo);
        }
    }

    /// Tira até `limite` falas, devolvendo **só** o crédito das mensagens.
    ///
    /// Extraída do comando para poder ser medida sem janela: a contabilidade é
    /// o que a revisão encontrou errado, e ela não pode depender de um
    /// `AppHandle` para ser provada.
    fn colher_falas(&mut self, limite: usize) -> Vec<FalaPendente> {
        let quantas = limite.clamp(1, 64).min(self.pendentes.len());
        let mut colhidas: Vec<FalaPendente> = self.pendentes.drain(..quantas).collect();
        let bytes: usize = colhidas.iter().map(|f| f.corpo.len()).sum();
        self.bytes_pendentes = self.bytes_pendentes.saturating_sub(bytes);
        // **Só as mensagens.** A versão anterior subtraía quantidade e bytes de
        // todas as falas colhidas, e erro nunca tinha reservado nada.
        self.executor.fila().tirar_varios(colhidas.len(), bytes);

        for aviso in self.avisos.drain(..) {
            self.executor.fila().tirar_aviso();
            colhidas.push(aviso);
        }
        if self.avisos_perdidos > 0 {
            colhidas.push(FalaPendente {
                tipo: "avisos-perdidos".to_owned(),
                corpo: self.avisos_perdidos.to_string(),
                vezes: 1,
            });
            self.avisos_perdidos = 0;
        }
        // **Rearma a sinalização quando esvaziou.** Com fala ainda pendente ela
        // fica armada: a janela continua drenando e não precisa de outro aviso.
        // Vazia, o próximo que chegar volta a avisar — é a transição
        // vazio→não-vazio, e perdê-la deixaria o MOD mudo.
        if self.pendentes.is_empty() && self.avisos.is_empty() {
            self.avisado = false;
        }
        colhidas
    }

    /// Precisa avisar a janela por esta fala?
    ///
    /// No máximo um aviso pendente por instância. Enquanto ele não for colhido,
    /// tudo o mais entra em silêncio — a janela já sabe que há o que buscar.
    fn precisa_avisar(&mut self) -> bool {
        if self.avisado {
            return false;
        }
        self.avisado = true;
        true
    }

    /// Solta tudo o que ela retém, devolvendo os créditos.
    ///
    /// Chamado na revogação e no descarte: o que não foi colhido **não é
    /// entregue depois**, é descartado — entregar efeito de uma sessão que
    /// acabou é exatamente o que o §5 do contrato proíbe.
    fn soltar(&mut self) {
        let bytes = self.bytes_pendentes;
        let quantas = self.pendentes.len();
        self.pendentes.clear();
        self.bytes_pendentes = 0;
        self.executor.fila().tirar_varios(quantas, bytes);
        // **Cada classe devolve na cota dela**, inclusive no descarte. Soltar
        // avisos pela conta das mensagens era a mesma assimetria de antes, do
        // outro lado.
        for _ in self.avisos.drain(..) {
            self.executor.fila().tirar_aviso();
        }
        self.avisos_perdidos = 0;
        // E o fonte sai junto: é o que torna o descarte da reserva atômico.
        self.codigo = None;
    }
}

impl ModsNativos {
    /// Guarda uma instância reservada e devolve o número dela.
    fn guardar(&mut self, instancia: InstanciaNativa) -> u64 {
        self.proxima = self.proxima.saturating_add(1);
        let numero = self.proxima;
        self.vivos.insert(numero, instancia);
        numero
    }

    /// Acha uma instância, viva ou encerrando.
    fn achar_mut(&mut self, numero: u64) -> Option<&mut InstanciaNativa> {
        self.vivos
            .get_mut(&numero)
            .or_else(|| self.encerrando.get_mut(&numero))
    }

    /// Move uma instância para a tabela de encerramento e pede a parada.
    fn encerrar(&mut self, numero: u64) {
        if let Some(mut instancia) = self.vivos.remove(&numero) {
            instancia.estado = EstadoNativo::Encerrando;
            instancia.executor.pedir_encerramento();
            self.encerrando.insert(numero, instancia);
        } else if let Some(instancia) = self.encerrando.get_mut(&numero) {
            // Já estava parando: pedir de novo é o desfecho normal de quem
            // apertou duas vezes, e não custa nada.
            instancia.executor.pedir_encerramento();
        }
    }

    /// Pede a parada de tudo o que pertence a uma geração.
    fn revogar_geracao(&mut self, geracao: u64) {
        let alvos: Vec<u64> = self
            .vivos
            .iter()
            .filter(|(_, i)| i.geracao == geracao)
            .map(|(n, _)| *n)
            .collect();
        for numero in alvos {
            self.encerrar(numero);
        }
        // **E solta o que ficou retido.** Sem isto, uma mensagem que não foi
        // colhida antes da saída prendia bytes e a instância para sempre: a
        // colheita recusa a geração que saiu, então não havia caminho.
        self.descartar_geracao(geracao);
    }

    /// O executor confirmou: a instância sai da supervisão.
    ///
    /// **Só aqui ela some.** É a diferença entre «encerrou» e «mandei
    /// encerrar», e é o que faz uma parada que nunca chega continuar aparecendo
    /// no diagnóstico.
    fn confirmar_parada(&mut self, numero: u64) {
        // **As duas tabelas, e a marca antes do critério.**
        //
        // A versão anterior escrevia `parou = true` só na de encerramento, e
        // para a de vivas conferia `i.parou` sem nunca tê-lo escrito. Uma
        // parada espontânea — o motor que cai sozinho — não removia nem marcava
        // a instância, e ela continuava descrita como ativa para sempre.
        let estava_viva = if let Some(instancia) = self.vivos.get_mut(&numero) {
            instancia.parou = true;
            instancia.estado = EstadoNativo::Encerrando;
            true
        } else {
            false
        };
        if estava_viva {
            if let Some(instancia) = self.vivos.remove(&numero) {
                self.encerrando.insert(numero, instancia);
            }
        }
        let Some(instancia) = self.encerrando.get_mut(&numero) else {
            return;
        };
        instancia.parou = true;
        // Sai da supervisão quando não há mais nada dela para colher: o que o
        // MOD disse antes de parar ainda precisa chegar a quem lê.
        if instancia.pendentes.is_empty() && instancia.avisos.is_empty() {
            self.encerrando.remove(&numero);
        }
    }

    /// Tira uma instância da supervisão, soltando o que ela retém.
    ///
    /// Para o desfazer de uma montagem que não completou: sem ela, uma reserva
    /// cuja bomba não subiu ficaria na tabela com o fonte dentro, esperando uma
    /// ativação que não vem.
    fn descartar(&mut self, numero: u64) {
        for tabela in [&mut self.vivos, &mut self.encerrando] {
            if let Some(mut instancia) = tabela.remove(&numero) {
                instancia.soltar();
            }
        }
    }

    /// Uma fala foi descartada por chegar depois da revogação.
    ///
    /// Se a instância já tinha parado e agora não retém mais nada, ela sai: sem
    /// isto, uma mensagem que chegou tarde a manteria na supervisão para
    /// sempre, esperando uma colheita que não pode mais acontecer.
    fn contar_descartada(&mut self, numero: u64) {
        self.descartadas = self.descartadas.saturating_add(1);
        if self
            .encerrando
            .get(&numero)
            .is_some_and(|i| i.parou && i.pendentes.is_empty() && i.avisos.is_empty())
        {
            self.encerrando.remove(&numero);
        }
    }

    /// **Solta tudo o que pertence a uma geração que saiu.**
    ///
    /// A revisão nomeia o buraco: «sair com uma mensagem ainda na fila deixa
    /// dados e a instância retidos mesmo depois de `Parou`, sem caminho de
    /// colheita válido» — porque `colher` recusa a geração anterior.
    ///
    /// Aqui os dados são descartados e os créditos acertados **sem a janela**.
    /// Ela pode ter fechado, e o que ela faria não é entregar: efeito de uma
    /// sessão encerrada não é entregue, é jogado fora.
    fn descartar_geracao(&mut self, geracao: u64) {
        for instancia in self
            .vivos
            .values_mut()
            .chain(self.encerrando.values_mut())
            .filter(|i| i.geracao == geracao)
        {
            instancia.soltar();
        }
        // E as que já confirmaram a parada saem agora: não há mais nada nelas.
        let acabadas: Vec<u64> = self
            .encerrando
            .iter()
            .filter(|(_, i)| i.geracao == geracao && i.parou)
            .map(|(n, _)| *n)
            .collect();
        for numero in acabadas {
            self.encerrando.remove(&numero);
        }
    }
}

/// Carries FFI events onto the webview — **da geração que a criou**.
///
/// # O que ela deixou de fazer
///
/// Ela emitia para a janela e pronto, sem saber de qual sessão o evento vinha.
/// Uma conexão que cai devagar continua entregando eventos enquanto as tarefas
/// dela terminam, e a janela já pode estar dentro de outro servidor — o evento
/// do anterior chegava como se fosse do atual. O roteiro da etapa E2 nomeia
/// exatamente isto: «vincular eventos à conexão/geração que criou o listener».
///
/// Agora cada ponte carrega o número da execução em que nasceu e compara com o
/// que está de pé. O que não bate é **contado** antes de ser descartado: um
/// contador que sobe depois de uma saída é a prova de que alguma coisa da
/// sessão anterior continuava falando, e sem ele isso seria invisível.
struct Bridge<R: tauri::Runtime = tauri::Wry> {
    app: AppHandle<R>,
    /// A execução em que esta ponte nasceu.
    geracao: u64,
    /// O número que está de pé agora, compartilhado com a `Session`.
    de_pe: Arc<std::sync::atomic::AtomicU64>,
    /// Onde contar o que foi descartado, compartilhado com a `Session`.
    descartados: Arc<std::sync::atomic::AtomicU64>,
    /// A tentativa da janela que pediu esta entrada, quando ela disse qual.
    /// Vai em cada etapa da chegada ([`carga_do_evento`]).
    tentativa: Option<u64>,
}

/// **O evento como ele vai à janela**, com a tentativa dela nas etapas da
/// chegada (item 52 da pendência 58).
///
/// A geração separa os eventos de duas entradas só a partir do `connect` da
/// mais nova. Antes dele, a janela já começou a nova (o apelido, a pergunta do
/// nome, o `conferir_esta_versao`), e as etapas da velha ainda passam pela
/// [`Bridge`] dela. Com a tentativa dentro, o ouvinte de etapas da janela
/// (`tela-boot.js`) mostra só as da mais nova. Os outros eventos vão como
/// sempre: a geração basta a eles, porque nenhum chega antes do `connect`.
fn carga_do_evento(event: &Event, tentativa: Option<u64>) -> serde_json::Value {
    let mut carga = serde_json::to_value(event).unwrap_or(serde_json::Value::Null);
    if let (Some(tentativa), Some(etapa)) = (
        tentativa,
        carga
            .get_mut("ConnectStageChanged")
            .and_then(serde_json::Value::as_object_mut),
    ) {
        etapa.insert("tentativa".to_owned(), tentativa.into());
    }
    carga
}

impl<R: tauri::Runtime> EventListener for Bridge<R> {
    fn on_event(&self, event: Event) {
        if self.geracao != self.de_pe.load(std::sync::atomic::Ordering::Acquire) {
            self.descartados
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        // **O fim revoga aqui, e não quando a janela pedir.**
        //
        // `Ended` chega quando a sessão acaba do outro lado: a sala terminou, o
        // enlace caiu, o conjunto de MODs mudou, alguém foi expulso. A
        // desmontagem nativa só acontecia depois, quando a pessoa apertasse um
        // botão na tela de fim — e entre uma coisa e outra a geração continuava
        // valendo do lado do Rust. Um comando que chegasse ali passava.
        //
        // O §5 do contrato manda o contrário: «saída local, expulsão,
        // encerramento remoto, troca de servidor, alteração do conjunto de MODs
        // e falha definitiva de conexão passam pelo mesmo encerramento».
        //
        // Emitido **antes** de revogar, de propósito: é este evento que faz a
        // tela de fim aparecer, e revogar primeiro o descartaria na linha de
        // cima — a sessão acabaria sem ninguém ficar sabendo.
        let acabou = matches!(event, Event::Ended { .. });
        // A failed emit means the window is gone, which is not worth a log channel
        // per event during shutdown.
        let _ = self
            .app
            .emit(EVENT_CHANNEL, carga_do_evento(&event, self.tentativa));
        if acabou {
            let morta = self.de_pe.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            // E os executores nativos daquela geração param junto, pela mesma
            // razão de `Session::revogar`: o dono do runtime é este lado, e a
            // janela pode não pedir — ela pode estar travada ou já ter fechado.
            if let Some(sessao) = self.app.try_state::<Session>() {
                if let Ok(mut vivos) = sessao.mods_nativos.lock() {
                    vivos.revogar_geracao(morta);
                }
            }
            tracing::debug!(geracao = morta, "sessão revogada pelo fim que chegou");
        }
    }
}

#[cfg(test)]
mod a_etapa_leva_a_tentativa {
    use super::carga_do_evento;
    use seele_ffi::{ConnectStage, Event};

    fn etapa() -> Event {
        Event::ConnectStageChanged {
            stage: ConnectStage::Parada {
                candidatos: 2,
                com_bilhete_e_impressao: false,
            },
        }
    }

    /// **A etapa leva a tentativa da janela, e o resto vai como sempre** (item
    /// 52 da pendência 58). A etapa continua com o `stage` de antes, e ganha a
    /// `tentativa` ao lado dele; sem tentativa, ela sai igual ao evento; e um
    /// evento que não é etapa não ganha nada, com tentativa ou sem.
    #[test]
    fn so_a_etapa_ganha_a_tentativa_e_sem_ela_nada_muda() {
        let com = carga_do_evento(&etapa(), Some(7));
        assert_eq!(
            com.pointer("/ConnectStageChanged/tentativa"),
            Some(&serde_json::Value::from(7)),
            "a etapa não levou a tentativa: {com}"
        );
        assert_eq!(
            com.pointer("/ConnectStageChanged/stage"),
            serde_json::to_value(etapa())
                .unwrap()
                .pointer("/ConnectStageChanged/stage"),
            "a etapa com a tentativa mudou o `stage`: {com}"
        );
        assert_eq!(
            carga_do_evento(&etapa(), None),
            serde_json::to_value(etapa()).unwrap()
        );
        for outro in [Event::RosterChanged, Event::MessagesChanged] {
            assert_eq!(
                carga_do_evento(&outro, Some(7)),
                serde_json::to_value(&outro).unwrap(),
                "um evento que não é etapa ganhou a tentativa"
            );
        }
    }
}

/// Where this client keeps its identity and its pins. ADR 0017.
///
/// The FFI takes a path because the shell knows where its platform keeps
/// configuration and the core knows how to persist an identity. `$SEELE_HOME`
/// comes first so the desktop app and `connection` can be told to be the same person —
/// which is what makes a session resumable between them.
fn config_dir<R: tauri::Runtime>(app: &AppHandle<R>) -> String {
    if let Ok(home) = std::env::var("SEELE_HOME") {
        return home;
    }
    // The same `~/.config/seele` the terminal client uses, deliberately: two
    // clients on one machine should be one person unless told otherwise.
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return format!("{xdg}/seele");
    }
    if let Ok(home) = std::env::var("HOME") {
        return format!("{home}/.config/seele");
    }
    app.path()
        .app_config_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".seele".to_owned())
}

/// O que uma conexão bem-sucedida entrega à tela.
///
/// O veredito vem **junto** do `Snapshot`, e não por evento: `Connection::connect` só
/// devolve o `Arc<Connection>` depois de a identidade estar decidida, então uma casca
/// que se inscrevesse para ouvi-lo chegaria sempre tarde demais.
#[derive(Debug, serde::Serialize)]
struct Entrada {
    /// A tela inteira, como sempre.
    snapshot: Snapshot,
    /// O que a chave deste servidor acabou de ser.
    ///
    /// `crates/seele-core/src/tofu.rs` é explícito: a casca tem que dizer o que
    /// acabou de confiar. Um pin que se estabelece invisível é um pin que
    /// ninguém sabe que devia conferir.
    veredito: seele_ffi::Trust,
    /// Qual execução de sessão esta entrada abriu — etapa E2.
    ///
    /// A janela guarda este número e o devolve em todo comando de MOD. É o que
    /// faz um trabalho que termina depois da saída ser recusado aqui, em vez de
    /// ser atendido contra a sessão seguinte.
    geracao: u64,
    /// De onde veio a impressão que esta conexão conferiu, e se a lista a
    /// guardou. Achatado: os dois campos chegam à janela no topo da entrada.
    #[serde(flatten)]
    de_onde: DeOndeVeio,
}

/// De onde veio a impressão que uma conexão conferiu.
///
/// A conferência é a mesma, e a frase não pode ser: uma impressão que veio do
/// link colado nesta sessão acusa o link quando não confere, e uma que veio da
/// lista de servidores conhecidos não tem link nenhum a acusar. O app sabia de
/// onde ela vinha e não dizia, e a tela falava de um link que a pessoa não
/// colou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
enum OrigemDaImpressao {
    /// Do link colado nesta sessão.
    Link,
    /// Da lista de servidores conhecidos: a impressão que uma conexão anterior
    /// aceitou, ou a que a 0.15.0 gravou do link daquela vez.
    Lista,
}

impl OrigemDaImpressao {
    /// De onde veio a impressão que `seele_ffi::impressao_a_conferir` escolhe
    /// com as mesmas duas entradas, e `None` quando ela não escolhe nenhuma.
    ///
    /// O link desta sessão vence a guardada, pela razão escrita lá.
    fn da_conferida(do_link: Option<&str>, guardada: Option<&str>) -> Option<Self> {
        match (do_link, guardada) {
            (Some(_), _) => Some(Self::Link),
            (None, Some(_)) => Some(Self::Lista),
            (None, None) => None,
        }
    }
}

/// O que a janela precisa saber, além do veredito, para dizer a frase certa.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
struct DeOndeVeio {
    /// De onde veio a impressão conferida; `None` quando não houve nenhuma.
    origem_da_impressao: Option<OrigemDaImpressao>,
    /// Se a lista de servidores conhecidos gravou, nesta entrada, a impressão
    /// que a conexão aceitou.
    ///
    /// A faixa de uma lista que discordava do pino diz que a lista passou a
    /// guardar a chave dele, e isso só é verdade se a gravação deu certo: a
    /// lista pode não abrir, e a gravação pode falhar. Falso numa falha, em que
    /// nada é gravado.
    a_lista_guardou: bool,
}

/// Uma entrada que não aconteceu, como chega à janela.
///
/// O erro e a trilha da FFI (`ConnectFailure`), achatados no topo como sempre
/// chegaram, e mais de onde veio a impressão: a recusa pela impressão diz uma
/// frase para o link e outra para a lista. É daqui, e não de um campo novo na
/// FFI, porque quem sabe de onde a impressão veio é este `connect`.
#[derive(Debug, serde::Serialize)]
struct FalhaDaEntrada {
    /// O erro de sempre e por onde a chegada passou.
    #[serde(flatten)]
    falha: ConnectFailure,
    /// De onde veio a impressão que esta tentativa conferia.
    #[serde(flatten)]
    de_onde: DeOndeVeio,
    /// **Esta tentativa entrou, e outra já a tinha substituído** (pendência
    /// 58, item 54; lote E54 da F5). A conexão dela foi fechada aqui, e a
    /// sessão de pé é a de quem a substituiu: a janela não diz nada na tela
    /// sobre ela. Ver [`largar_a_substituida`].
    ///
    /// Fora do JSON quando falso: toda outra recusa chega à janela como
    /// chegava.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    substituida: bool,
}

impl From<ConnectionError> for FalhaDaEntrada {
    /// Uma falha de antes de haver impressão a conferir.
    fn from(error: ConnectionError) -> Self {
        Self {
            falha: ConnectFailure::from(error),
            de_onde: DeOndeVeio::default(),
            substituida: false,
        }
    }
}

/// **Fecha a conexão de uma tentativa que outra substituiu, e diz.**
///
/// O `connect` revoga a geração ao começar, e uma tentativa nova que começa
/// enquanto outra anda tira a geração dela. A velha que termina depois não
/// pode entrar: ela gravava o slot por cima da nova, e a pessoa passava a
/// escrever num servidor que tinha deixado, com a tela mostrando o outro
/// (medido em `a_entrada_velha_que_chega_tarde`). Aqui a conexão dela é
/// fechada, uma linha vai ao `seele.log`, e a janela recebe a recusa marcada
/// como substituída.
fn largar_a_substituida(
    connection: &Connection,
    geracao: u64,
    de_pe: u64,
    alvo: &str,
    origem_da_impressao: Option<OrigemDaImpressao>,
) -> FalhaDaEntrada {
    connection.disconnect();
    tracing::warn!(
        geracao,
        de_pe,
        alvo,
        "a entrada a {alvo} foi substituída por outra antes de entrar: a conexão dela foi \
         fechada, e a sessão de pé é a de quem a substituiu"
    );
    FalhaDaEntrada {
        falha: ConnectFailure::from(ConnectionError::NotConnected),
        de_onde: DeOndeVeio {
            origem_da_impressao,
            a_lista_guardou: false,
        },
        substituida: true,
    }
}

#[tauri::command]
async fn connect<R: tauri::Runtime>(
    app: AppHandle<R>,
    session: State<'_, Session>,
    server: String,
    nickname: String,
    audio: bool,
    join_secret: Option<String>,
    tentativa: Option<u64>,
) -> Result<Entrada, FalhaDaEntrada> {
    // **Conferir que não há sessão e revogar, com o slot na mão** (M1 da
    // revisão do lote E54). Eram duas linhas soltas — `connection().is_ok()`
    // aqui e `revogar()` logo abaixo —, e uma tentativa velha podia gravar o
    // slot entre elas. Esta passava pelo `AlreadyConnected` com o slot vazio e
    // revogava depois de a velha já ter entrado; se falhasse, a velha ficava
    // no slot sem tela (a janela a larga, e o pedido de soltá-la chega com uma
    // geração que já não é a de pé), e toda entrada seguinte recebia
    // `AlreadyConnected` até o app fechar. Com o cadeado: ou a velha grava
    // antes e esta recusa, ou esta revoga antes e a velha é largada na
    // conferência dela. A ordem dos cadeados é a de `soltar_a_entrada_em`: o
    // do slot, e dentro dele os de `Session::revogar`.
    //
    // Sem o cadeado não há sessão a recusar, como antes
    // (`Session::connection` devolve `NotConnected`): revoga e segue.
    //
    // **A geração nasce com a tentativa, e não com o sucesso.** O roteiro da
    // etapa E2 é explícito: «capturar a geração desde a tentativa de conexão».
    //
    // A razão é a ponte logo abaixo: ela entra por `connect_watching`, **antes**
    // de haver conexão, porque as etapas da chegada acontecem durante aquela
    // linha e um ouvinte inscrito depois chegaria tarde. Se o número só
    // existisse no fim, essa ponte não teria contra o que comparar durante a
    // parte da conexão em que mais coisa acontece.
    //
    // E uma tentativa que falha também gasta um número, de propósito: ela pode
    // ter deixado tarefas pelo caminho, e as respostas delas não podem ser
    // admitidas pela tentativa seguinte.
    let geracao = {
        let slot = session.connection.lock();
        if slot.as_ref().is_ok_and(|slot| slot.is_some()) {
            return Err(ConnectionError::AlreadyConnected.into());
        }
        let geracao = session.revogar();
        drop(slot);
        geracao
    };

    // O convite guardado vale para o servidor dele e para nenhum outro. Quem cola
    // um link e depois troca o endereço no campo deixaria para trás uma
    // confirmação de identidade que não é deste servidor — e agora que a FFI
    // sabe conferi-la, uma sobra dessas seria uma recusa que ninguém consegue
    // explicar.
    //
    // O que sobrevive ao descarte é o que vai conferir esta conexão.
    //
    // Os outros endereços do convite saem daqui pelo mesmo caminho e pela mesma
    // razão: eles pertencem ao servidor daquele link, e quem trocou o endereço no
    // campo está indo a outro lugar — tentar os alternativos do link anterior
    // seria bater à porta de um servidor que ninguém pediu.
    let (esperada, alternativos, bilhete) = match session.convite.lock() {
        Ok(mut slot) => {
            if slot.as_ref().is_some_and(|convite| convite.alvo != server) {
                *slot = None;
            }
            (
                slot.as_ref()
                    .and_then(|convite| convite.impressao_digital.clone()),
                slot.as_ref()
                    .map(|convite| convite.alternativos.clone())
                    .unwrap_or_default(),
                // Degrau 4 do ADR 0022. Sai daqui pela mesma porta e pela mesma
                // razão que os alternativos: o bilhete é do servidor **daquele**
                // link, e bater no ponto de encontro de outro seria apresentar
                // esta máquina a um anfitrião que ninguém pediu.
                slot.as_ref().and_then(|convite| convite.bilhete.clone()),
            )
        }
        // Sem o cadeado não há convite a ler. Entrar sem a confirmação é o
        // comportamento de quem digitou o endereço à mão, e é o pior que pode
        // acontecer aqui: nunca uma conferência contra um valor duvidoso.
        Err(_) => (None, Vec::new(), None),
    };

    // **Sem convite, a escada vem da lista de conhecidos.**
    //
    // Um convite traz três coisas para chegar ao mesmo servidor: o endereço da
    // rede local primeiro, os de fora depois, e o bilhete de encontro para
    // furar o NAT. Quem volta a um servidor pela lista não tem convite nenhum —
    // ele foi lido uma vez, num processo que já fechou — e a lista guardava só
    // o primeiro dos três. O primeiro é o da **rede local**, porque é o que
    // funciona para quem está na mesma casa.
    //
    // Então quem recebeu o link pela internet conectava uma vez e nunca mais: a
    // lista de para-onde-voltar tinha guardado, dos três endereços, o único que
    // não serve a ela. «Ele salva apenas o endereço LAN, o que não faz sentido
    // algum para pessoas que se conversam pela internet.»
    let veio_de_link = !alternativos.is_empty() || bilhete.is_some();
    let (alternativos, bilhete, impressao_guardada) = if veio_de_link {
        (alternativos, bilhete, None)
    } else {
        match seele_ffi::conhecidos::Conhecidos::abrir(
            std::path::PathBuf::from(config_dir(&app)).join("conhecidos"),
        ) {
            Ok(lista) => lista
                .buscar(&server)
                .map(|conhecido| {
                    (
                        conhecido.caminhos.clone(),
                        conhecido
                            .bilhete
                            .as_deref()
                            .and_then(|texto| seele_ffi::uri::Bilhete::ler(texto).ok()),
                        conhecido.impressao.clone(),
                    )
                })
                .unwrap_or_default(),
            // **Dito, e não engolido.** Sem a lista, a volta segue como quem
            // digitou o endereço: sem os outros caminhos, sem o bilhete e sem a
            // impressão guardada, que é a que ela conferiria. Era um `.ok()`, e
            // ninguém ficava sabendo por que a volta entrou às cegas.
            Err(erro) => {
                tracing::warn!(
                    %erro,
                    "não abri a lista de servidores conhecidos: esta volta segue sem os \
                     caminhos, o bilhete e a impressão que ela guardava deste servidor"
                );
                Default::default()
            }
        }
    };

    // **A impressão que esta conexão confere, e a que o quarto pergunta.**
    //
    // Até a 0.15.0 a conexão só conferia a impressão de um link colado nesta
    // sessão. Quem voltava pela lista não tinha link nenhum e entrava no
    // endereço que o quarto devolvia sem conferir nada. Era PRIMEIRO CONTATO
    // cego: um impostor que ocupasse a marca era aceito e gravado na lista (o
    // S3 da análise de 22/09). E o `LEVE` não saía, porque a marca dele sai da
    // impressão (o terceiro defeito do link). A guardada existia e só servia
    // para a pergunta ao quarto.
    //
    // Uma impressão só para as duas coisas, e a regra é da FFI. Perguntar ao
    // quarto onde mora uma chave e conferir outra faria o quarto apontar para o
    // servidor que a conferência recusaria em seguida. A pergunta usava a
    // guardada primeiro; agora o link desta sessão vence, pelo motivo escrito
    // em `impressao_a_conferir`.
    let conferida =
        seele_ffi::impressao_a_conferir(esperada.as_deref(), impressao_guardada.as_deref());
    // E de onde ela veio, pela mesma ordem: a frase de uma impressão que não
    // confere é outra quando quem a prometeu foi a lista, e não um link.
    let origem_da_impressao =
        OrigemDaImpressao::da_conferida(esperada.as_deref(), impressao_guardada.as_deref());

    // **O quarto, onde este servidor mora hoje, é perguntado na ponte, e junto
    // com a corrida** (a pendência 55, tarefa 14b da F7).
    //
    // Tudo o que a lista guarda é endereço, e endereço atrás de NAT é perecível:
    // o mapeamento nasce quando um pacote sai, e o roteador dá outro na
    // abertura seguinte. A impressão digital não envelhece, e é dela que sai a
    // marca com que o quarto é perguntado. Esta função perguntava aqui, antes de
    // chamar a ponte, e quem estava na mesma casa pagava até 1,5 s de prazo do
    // quarto com o ponto ou o DNS mudos. Agora a `Chegada` da FFI pergunta
    // enquanto a corrida anda, com a impressão conferida e o bilhete guardado:
    // o endereço de hoje entra quando chega, e a escuta de hoje vira o aviso do
    // `LEVE` só pela regra de `bilhete_desta_volta`.
    //
    // **Nada do que o quarto dá volta para cá**, e é isso que mantém a lista de
    // conhecidos limpa. O bilhete que ela grava é o guardado (o do link desta
    // sessão, ou o que a lista já tinha), e não o desta volta: quem ocupasse a
    // marca da escuta ficaria gravado, e as voltas seguintes sem escuta fresca
    // mandariam a ele o `LEVE`, com o endereço de quem chega. E o endereço de
    // hoje também não entra na escada da lista (`caminhos_a_guardar`, abaixo):
    // ele é perguntado de novo a cada conexão.
    let bilhete_texto = bilhete.as_ref().map(ToString::to_string);

    let home = config_dir(&app);
    // **Sem nome escolhido, um nome desta máquina** — e não uma constante que
    // todo mundo compartilha.
    //
    // A casca manda vazio quando ninguém se nomeou. O recurso antigo era a
    // palavra `pessoa`, igual para todos, e por ADR 0017 o apelido pertence a
    // uma chave: a **segunda** pessoa sem nome de qualquer servidor colidia por
    // construção e era recusada na porta. Aconteceu em campo, e a frase que ela
    // lia mandava escolher outro apelido — um apelido que ela nunca tinha
    // escolhido.
    //
    // Quatro caracteres da impressão da chave desta máquina resolvem as duas
    // metades: não colidem, porque a impressão é da chave; e são **estáveis**,
    // então quem volta amanhã volta na mesma conta, com o mesmo histórico, em
    // vez de criar uma conta nova a cada abertura — que é o que um sufixo
    // aleatório faria.
    //
    // Continua sendo recurso e não escolha: quem se nomeia no perfil manda, e
    // o rodapé da entrada segue mostrando o travessão até alguém se nomear de
    // verdade.
    let nickname = if nickname.trim().is_empty() {
        seele_ffi::impressao_desta_maquina(&home).map_or_else(
            |_| "pessoa".to_owned(),
            |impressao| format!("pessoa-{}", &impressao[..4.min(impressao.len())]),
        )
    } else {
        nickname
    };
    // Guardados antes de a configuração levar os originais para a outra thread:
    // a lista de visitados só é escrita lá embaixo, depois de a conexão existir.
    let alvo = server.clone();
    let apelido = nickname.clone();
    let casa = home.clone();
    // A escada, para a mesma lista. O endereço de hoje não está nela: o quarto
    // é perguntado na ponte, e ele não volta para cá (ver o começo desta
    // função e `caminhos_a_guardar`).
    let caminhos = seele_ffi::conhecidos::caminhos_a_guardar(&alternativos, None);
    // **A sala desta máquina se confere pelo código** (o ATENDER, tarefa 27 da
    // F7): ela apresenta a chave da pessoa, e nenhuma lista guardou o
    // certificado dela. Com o código, vale ele
    // (`seele_ffi::ConnectConfig::codigo_esperado`).
    let codigo_da_sala = sala::codigo_se_for_a_sala(&session, &server, &home);
    let config = ConnectConfig {
        server,
        alternate_servers: alternativos,
        nickname,
        home,
        audio,
        join_secret: join_secret.filter(|s| !s.trim().is_empty()),
        // A decidida acima, e não só a do link: quem volta pela lista também
        // confere, dentro do TLS e antes do `Hello`. `esperada` e
        // `impressao_guardada` continuam vivas para a lista de conhecidos lá
        // embaixo, onde elas e o veredito decidem o que se guarda
        // (`impressao_a_guardar`).
        expected_fingerprint: conferida,
        codigo_esperado: codigo_da_sala,
        bilhete,
        // O microfone escolhido no Terminal servidor, lido do disco a cada
        // conexão em vez de guardado em memória: quem escolheu ontem não
        // escolhe de novo hoje, e quem nunca escolheu continua no padrão da
        // máquina. Um dispositivo que sumiu não impede de entrar — a FFI cai
        // para o padrão e a tela mostra o que abriu de verdade.
        capture_device: preferencias(&app).and_then(|p| p.capture().map(str::to_owned)),
        // A saída de som, pela mesma política e lida do mesmo lugar. Os dois
        // ajustes são independentes: um fone que ficou na outra sala não pode
        // custar o microfone que a pessoa escolheu.
        playback_device: preferencias(&app).and_then(|p| p.playback().map(str::to_owned)),
    };

    // `connect` blocks on a QUIC handshake. Running it on the async runtime's
    // worker would stall every other command until it finished or timed out.
    //
    // O veredito volta daqui junto do handle, e continua sendo o único lugar de
    // onde ele pode vir: ele é decidido dentro do aperto de mão.
    //
    // **As etapas, não.** Elas acontecem durante esta linha, e é por isso que a
    // ponte entra por `connect_watching` em vez de por `subscribe` depois: o
    // comentário que estava aqui dizia que quem se inscreve só tem o
    // `Arc<Connection>` depois que esta linha termina, e isso continua verdade — o
    // que mudou é que a FFI passou a aceitar o ouvinte **antes** de bloquear.
    // Sem isso o `watch` da chegada não tinha um só leitor em produção.
    //
    // `Bridge` não depende do connection para nada: ele carrega o `AppHandle` e
    // reemite. Por isso um segundo, criado aqui, não é duplicação de estado —
    // é o mesmo destino, ligado mais cedo.
    let ponte = Arc::new(Bridge {
        app: app.clone(),
        geracao,
        de_pe: Arc::clone(&session.geracao),
        descartados: Arc::clone(&session.eventos_de_geracao_morta),
        tentativa,
    }) as Arc<dyn EventListener>;
    let atento = Arc::clone(&ponte);
    // **O CANCELAR desta tentativa** (tarefa 28 da F7): a vaga fica na sessão
    // enquanto a chegada anda, e o `cancelar_ligacao` a acha lá.
    let em_voo = ligar::LigacaoEmVoo::abrir(&session);
    let cancelamento = em_voo.cancelamento();
    let resultado = tauri::async_runtime::spawn_blocking(move || {
        Connection::connect_cancelavel(config, Some(atento), cancelamento)
    })
    .await
    .unwrap_or_else(|_| Err(ConnectFailure::from(ConnectionError::Unreachable)));
    drop(em_voo);
    let (connection, veredito) = resultado.map_err(|falha| FalhaDaEntrada {
        falha,
        de_onde: DeOndeVeio {
            origem_da_impressao,
            a_lista_guardou: false,
        },
        substituida: false,
    })?;

    // **A tentativa que outra substituiu não entra** (pendência 58, item 54;
    // lote E54 da F5). Duas tentativas passam pelo `AlreadyConnected` lá em
    // cima antes de qualquer uma terminar, e a mais nova revogou a geração
    // desta. Conferido aqui, antes de qualquer efeito: as escolhas gravadas, o
    // retrato e o consentimento de par não vão ao servidor que a pessoa
    // deixou. E de novo ao gravar o slot, logo abaixo.
    //
    // Por `confere_geracao`, como todo comando de uma geração morta: ela conta
    // em `comandos_de_geracao_morta` e diz em DEBUG, e a velha que chega tarde
    // é uma coisa da sessão anterior falando depois da hora.
    if session.confere_geracao(geracao).is_err() {
        return Err(largar_a_substituida(
            &connection,
            geracao,
            session.geracao(),
            &alvo,
            origem_da_impressao,
        ));
    }

    connection.subscribe(ponte);

    // O modo de microfone, a supressão de ruído e a sensibilidade gravados,
    // aplicados assim que há o que aplicar.
    //
    // Aqui e não num campo de `connect`: a sessão nasce no padrão da spec —
    // push-to-talk, microfone fechado — e só então abre para o que a pessoa
    // escolheu. A ordem é a segura; a inversa teria um instante em que um
    // microfone poderia estar aberto antes de alguém ter dito que podia.
    //
    // **Os três, e não só o modo** (acréscimo A do lote I-b). A supressão e a
    // sensibilidade gravadas ficavam de fora, e a sessão começava nos padrões
    // até a pessoa mexer. Desde a pendência 52 os três moram na conexão e valem
    // ao abrir o áudio, então aplicá-los aqui é gravá-los lá.
    if let Some(ajustes) = preferencias(&app) {
        connection.vestir_as_escolhas_gravadas(&ajustes);
    }

    // **O retrato desta máquina sobe junto com a conexão.**
    //
    // É o mesmo acordo do apelido, que já viaja no `connect`: a imagem é sua e
    // não do servidor, então cada servidor em que você entra passa a te ver como
    // você se desenhou. Sem isto, escolher a imagem na tela inicial não teria
    // consequência nenhuma — que era a metade que faltava para os dois diálogos
    // de perfil serem a mesma coisa.
    //
    // Falhar aqui não derruba a conexão: uma sessão sem retrato é uma sessão com
    // iniciais, e uma sessão que não abre é uma pessoa que não entrou.
    if let Some(bytes) = retrato_guardado(&app) {
        if let Err(erro) = connection.set_person_icon(Some(bytes)) {
            tracing::warn!(?erro, "não mandei o retrato desta máquina ao entrar");
        }
    }

    // **A semente da sonda, e ela vale também para quem hospeda.**
    //
    // A perna que aperta no primeiro segundo não é o cano: é o que a sonda
    // mediu, e ela parte de `CAMINHO_DA_PROVA_BPS` — 2 Mbps — enquanto não
    // mediu nada. Medido em `tela.rs`: com loopback por baixo e cinquenta
    // megabits de subida do anfitrião, o primeiro segundo ainda é **540p**.
    //
    // A semente por servidor existe e é chamada, mas **dentro** do ramo
    // `if !endereco_desta_maquina(&alvo)` — o mesmo ramo que já tinha causado o
    // QA-02.
    // Quem hospeda nunca a recebia, e os vinte e cinco segundos de subida se
    // repetiam em toda transmissão, na mesma máquina, para sempre.
    //
    // A ordem é a da especificidade: a medida **deste servidor** manda, porque
    // o caminho até cada um é diferente; a da máquina é o ponto de partida de
    // quem ainda não tem uma — hospedando aqui, ou visitando pela primeira vez.
    {
        let do_servidor = seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
            .ok()
            .and_then(|lista| lista.buscar(&alvo).and_then(|c| c.caminho_bps));
        let semente =
            do_servidor.or_else(|| preferencias(&app).and_then(|p| p.caminho_da_maquina()));
        if let Some(bps) = semente {
            connection.lembrar_o_caminho(bps);
        }
    }

    // **O consentimento do caminho entre pares, declarado a cada conexão.**
    //
    // A declaração de identidade que o núcleo manda sozinho ao conectar leva
    // `ConsentimentoDePar::de_ninguem()` — «não empresto nada». Quem optou por
    // participar precisa dizer isso outra vez, aqui, ou a escolha guardada
    // valeria apenas até a janela fechar. Foi a peça que faltava: o caminho
    // entre pares estava inteiro no núcleo e inerte em produção porque ninguém
    // nesta camada o declarava.
    //
    // Falhar não derruba a entrada: sem consentimento a tela vem do servidor,
    // que é o caminho de sempre — «a malha é alívio, nunca dependência».
    {
        let (pares, assiste) = preferencias(&app).map_or((0, false), |p| p.caminho_entre_pares());
        if pares > 0 || assiste {
            if let Err(erro) = connection.consentir_no_caminho_entre_pares(pares, assiste) {
                tracing::warn!(?erro, "não declarei o consentimento de par ao entrar");
            }
        }
    }

    let snapshot = connection.snapshot();
    // Se a ponte discou um código, e qual: a ligação não é visita
    // (`ligar::anotar_a_chegada`). Lido aqui, antes de a conexão ir para o slot.
    let ligou_para = connection.codigo_da_ligacao();

    // **A mesma conferência, com o slot na mão.** Entre a de cima e esta há os
    // efeitos de entrada, e uma tentativa nova pode ter começado no meio. Com o
    // cadeado tomado não há espera entre conferir e gravar: ou a geração ainda
    // é desta e o slot é dela, ou a conexão dela é fechada e nada do que vem
    // abaixo — o endereço da sessão, o alvo, a lista de visitados — vale para
    // ela. Gravar sem conferir punha a conexão da velha no lugar da nova.
    let substituida = match session.connection.lock() {
        Ok(mut slot) if session.confere_geracao(geracao).is_ok() => {
            *slot = Some(connection);
            None
        }
        Ok(_) => Some(connection),
        Err(_) => None,
    };
    if let Some(connection) = substituida {
        return Err(largar_a_substituida(
            &connection,
            geracao,
            session.geracao(),
            &alvo,
            origem_da_impressao,
        ));
    }

    // A metade invisível da lista de visitados: sem isto a seção da tela de
    // entrada ficaria permanentemente vazia. A política é a mesma que o `connection`
    // já escreveu em `crates/seele-tui/src/main.rs`.
    //
    // Registrado só **depois** de dar certo — guardar antes encheria a lista de
    // endereços errados digitados uma vez, que é o oposto de uma lista de
    // atalhos. E um servidor hospedado aqui não entra: `127.0.0.1` não é lugar
    // aonde se volta, é o botão HOSPEDAR. O `connection` decide isso pela bandeira
    // `--hospedar`; aqui não há bandeira, e o endereço é o que sobrou para
    // dizer a mesma coisa.
    // **Fora do `if`, e é essa a correção.** Onde a sessão está não depende de
    // o endereço merecer entrar na lista de atalhos.
    if let Ok(mut onde) = session.endereco_da_sessao.lock() {
        *onde = Some(alvo.clone());
    }
    // Falso até a lista gravar, lá embaixo, a impressão que a conexão aceitou.
    let mut a_lista_guardou = false;
    // Uma ligação vai para a agenda, e não para cá: é `anotar_a_chegada` quem
    // decide, pela origem da tentativa, e quem guarda o alvo da sessão.
    if ligar::anotar_a_chegada(&session, &casa, &alvo, ligou_para) {
        let aberta = seele_ffi::conhecidos::Conhecidos::abrir(
            std::path::PathBuf::from(&casa).join("conhecidos"),
        );
        // A lista que não abre não anota a visita, os caminhos nem a impressão
        // aceita, e a cura de uma lista velha depende de ela gravar. Dito.
        if let Err(erro) = &aberta {
            tracing::warn!(
                %erro,
                "não abri a lista de servidores conhecidos: esta visita, os caminhos e a \
                 impressão aceita não foram guardados"
            );
        }
        if let Ok(mut lista) = aberta {
            // A sala de voz que já estava anotado, preservado. `registrar` reescreve a
            // entrada inteira, e este arquivo é compartilhado com o `connection`, que
            // grava em qual sala de voz a pessoa entrou e o lê de volta como padrão na
            // sua tela de seleção. Passar `None` daqui apagaria, a cada visita
            // pelo app, o que o terminal anotou.
            let voice_room = lista
                .buscar(&alvo)
                .and_then(|conhecido| conhecido.voice_room);
            // **A escada começa de onde parou da última vez.**
            //
            // Sem isto a sonda parte de `CAMINHO_DA_PROVA_BPS` — 2 Mbps
            // supostos — e gasta os primeiros segundos de toda transmissão
            // reaprendendo um cano que já mediu. Medido em campo numa LAN: doze
            // segundos de 540p até a escada reencontrar 1080p, entre duas
            // máquinas que tinham medido 12,48 Mbps entre si minutos antes.
            //
            // **Antes de `registrar`**, que reescreve a entrada: lê-lo depois
            // funcionaria, e depender da ordem de duas linhas para isso é a
            // espécie de coisa que quebra quando alguém as troca de lugar.
            let lembrado = lista
                .buscar(&alvo)
                .and_then(|conhecido| conhecido.caminho_bps)
                .unwrap_or(0);
            // Falhar em gravar um atalho não pode derrubar uma conversa que já
            // está de pé.
            if let Err(erro) = lista.registrar(&alvo, &apelido, voice_room) {
                tracing::warn!(%erro, "não guardei este servidor na lista de visitados");
            }
            // **E a escada, quando esta conexão a conhece.**
            //
            // `anotar_caminhos` não apaga o que já estava guardado quando não há
            // nada a anotar: uma volta pelo endereço salvo não traz convite, e
            // passar vazio aqui desfaria, na segunda visita, o que o link
            // ensinou na primeira.
            // Semeada pelo slot: a `connection` já foi movida para a sessão
            // algumas linhas acima, e mover a leitura da lista para antes dela
            // só trocaria uma ordem frágil por outra.
            if let Ok(slot) = session.connection.lock() {
                if let Some(viva) = slot.as_ref() {
                    viva.lembrar_o_caminho(lembrado);
                }
            }
            // **A impressão que a conexão aceitou, e não a do link.** Até a
            // 0.15.0 ia para cá a do link desta sessão, qualquer que tivesse
            // sido o veredito. Com um link que discorda do pin, a conexão fica
            // de pé com a chave fixada (ADR 0003), e a lista ficava com a de
            // outro servidor. A guardada é a que a volta pela lista confere, e
            // com ela o servidor verdadeiro seria recusado em todo endereço sem
            // pin. `None` deixa a guardada como estava.
            //
            // Com o link desta sessão **e** a guardada, na ordem de
            // `impressao_a_conferir`: num `Known`, o que se guarda é a que se
            // conferiu. Um `InviteDisagrees`, com ou sem link, grava a
            // ofertada: ele só nasce num alvo de escopo público, onde o pin
            // prova o servidor, e é assim que uma lista gravada pela 0.15.0 se
            // cura na primeira volta por ele. Num alvo de LAN, a esperada que
            // não confere já foi recusada dentro do TLS.
            let impressao_aceita = seele_ffi::impressao_a_guardar(
                &veredito,
                esperada.as_deref(),
                impressao_guardada.as_deref(),
            );
            match lista.anotar_caminhos(
                &alvo,
                &caminhos,
                bilhete_texto.as_deref(),
                impressao_aceita.as_deref(),
            ) {
                // **Só aqui a lista guardou a impressão aceita**, e só quando
                // havia uma a guardar. A faixa de uma lista que discordava do
                // pino diz que ela passou a guardar a chave dele: sai deste
                // valor, e não de supor que a gravação deu certo.
                Ok(()) => a_lista_guardou = impressao_aceita.is_some(),
                // `warn`, e não `debug`, que o `seele.log` não grava: a volta
                // pela lista confere pela impressão que ficou lá, e a cura de
                // uma lista gravada pela 0.15.0 depende desta gravação.
                Err(erro) => tracing::warn!(
                    %erro,
                    "não guardei na lista os caminhos, o bilhete e a impressão aceita deste \
                     servidor: a volta pela lista confere pela impressão que ficou lá"
                ),
            }
        }
    }

    Ok(Entrada {
        snapshot,
        veredito,
        geracao,
        de_onde: DeOndeVeio {
            origem_da_impressao,
            a_lista_guardou,
        },
    })
}

/// **Solta a entrada que a janela não adotou** (pendência 58, item 54; lote
/// E54 da F5).
///
/// O par de [`largar_a_substituida`], do lado da janela. O `connect` só sabe
/// que uma tentativa foi substituída quando a nova chega a ele e revoga a
/// geração. Entre a nova começar na janela (`++tentativas`, em `conectar`) e o
/// `connect` dela, a velha pode terminar e entrar com a geração ainda de pé; a
/// janela, que já não a quer, não a mostra, e pede aqui que ela seja solta.
/// Sem isto a conexão dela ficava no slot sem tela nenhuma: a nova recebia
/// `AlreadyConnected`, toda entrada seguinte também, e os eventos da velha
/// continuavam chegando à janela.
///
/// Devolve se soltou.
#[tauri::command]
fn soltar_a_entrada(session: State<'_, Session>, geracao: u64) -> bool {
    soltar_a_entrada_em(&session, geracao)
}

/// O corpo de [`soltar_a_entrada`], sem o `State`, para o teste.
///
/// **Só a geração dada, e só se ela ainda é a de pé**, conferida com o slot na
/// mão e revogada ali mesmo. Um `connect` novo que chega neste instante espera
/// o cadeado, acha o slot vazio e revoga depois: a geração dele é a seguinte, e
/// não esta. E uma entrada que outra já substituiu no Rust não derruba a de
/// quem a substituiu, porque a geração dela já não é a de pé.
///
/// **O convite fica.** Ele pode ser da tentativa nova, que ainda vai lê-lo no
/// `connect` dela; é a diferença deste para `desmontar_o_cliente`, que o
/// apaga com a sessão que ele abriu.
fn soltar_a_entrada_em(session: &Session, geracao: u64) -> bool {
    let solta = match session.connection.lock() {
        Ok(mut slot) if session.confere_geracao(geracao).is_ok() && slot.is_some() => {
            session.revogar();
            slot.take()
        }
        _ => None,
    };
    let Some(conexao) = solta else {
        tracing::info!(
            geracao,
            de_pe = session.geracao(),
            "a janela largou uma entrada que já não estava de pé: nada a soltar"
        );
        return false;
    };
    conexao.disconnect();
    drop(conexao);
    // Onde a sessão estava deixa de existir com ela, como na saída.
    if let Ok(mut onde) = session.endereco_da_sessao.lock() {
        *onde = None;
    }
    tracing::warn!(
        geracao,
        "a janela largou a entrada da geração {geracao}: outra tentativa a substituiu enquanto \
         ela entrava, e a conexão dela foi fechada"
    );
    true
}

/// **A entrada velha que chega tarde** (pendência 58, item 54; lote E54 da F5).
///
/// Duas tentativas passam pelo `AlreadyConnected` do começo de [`connect`] antes
/// de qualquer uma terminar: o HOSPEDAR AQUI fica aceso enquanto um `connect`
/// anda. A velha é lenta; a nova entra; a velha entra depois. Este módulo mede,
/// com dois servidores de verdade em porta zero e o `connect` de verdade (pelo
/// runtime falso do Tauri), qual conexão fica no slot da [`Session`] e qual
/// servidor recebe a mensagem que a pessoa manda em seguida.
///
/// A velha fica lenta pelo banco do servidor dela: o aperto de mão do
/// servidor pega o cadeado do banco antes de responder, e o teste o segura até
/// a nova ter entrado. A ordem é a do cenário, e não a de um relógio.
///
/// **Medido na base** (`b101e385`, com o `connect` genérico de `ecc3d9b5`): a
/// velha devolvia `Ok(Entrada { geracao: 1 })` com a geração de pé em 2, o
/// slot ficava com o servidor velho, a mensagem ia a ele, e a conexão da nova
/// era solta e desligada calada — nenhum `Ended` chegava à janela, e a pessoa
/// sumia da lista do servidor novo.
#[cfg(test)]
mod a_entrada_velha_que_chega_tarde {
    use super::{connect, send_message, Session};
    use seele_server::persistence::Location;
    use seele_server::{Daemon, ServerConfig};
    use std::net::SocketAddr;
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::{AppHandle, Manager};

    /// A casa do app neste teste: identidade e pinos numa pasta temporária,
    /// e não na de quem roda a bateria. Uma só para o processo inteiro, porque
    /// `SEELE_HOME` é do processo.
    fn casa() -> &'static std::path::Path {
        static CASA: OnceLock<tempfile::TempDir> = OnceLock::new();
        let pasta = CASA
            .get_or_init(|| tempfile::tempdir().expect("a pasta temporária não abriu"))
            .path();
        std::env::set_var("SEELE_HOME", pasta);
        pasta
    }

    /// Um teste deste módulo por vez: a casa é uma só, e a identidade e os
    /// pinos dela são arquivos que dois `connect` de testes diferentes
    /// escreveriam ao mesmo tempo. **E os do módulo vizinho que usa esta casa**
    /// (`o_apelido_grande_demais_no_hello`, lote C55 da F5): a `SEELE_HOME` é
    /// do processo, e uma segunda casa a trocaria por baixo destes testes.
    pub(super) fn vez() -> std::sync::MutexGuard<'static, ()> {
        static VEZ: std::sync::Mutex<()> = std::sync::Mutex::new(());
        VEZ.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// O runtime de um teste deste módulo: uma thread só, a do teste, para o
    /// `seele.log` capturado ver o que o `connect` escreve.
    pub(super) fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("o runtime do teste não subiu")
    }

    /// Um servidor de verdade, em porta zero e banco em memória.
    pub(super) async fn servidor(nome: &str) -> (SocketAddr, Arc<Daemon>) {
        let config = ServerConfig {
            name: nome.into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            ..ServerConfig::default()
        };
        let servidor = Arc::new(Daemon::bind(config).await.expect("o servidor não subiu"));
        let endereco = servidor
            .local_addr()
            .expect("o servidor não disse o endereço");
        let aceitando = Arc::clone(&servidor);
        tokio::spawn(async move {
            let _ = aceitando.run().await;
        });
        (endereco, servidor)
    }

    /// O app, com o runtime falso. Vazado: o `connect` corre numa tarefa
    /// própria, e o `State` que ele recebe empresta do app.
    ///
    /// **E sempre na casa temporária** (R1-I2 da re-revisão do E54). A casa
    /// era montada por cada teste, e o teste do M1 esqueceu: rodado sozinho,
    /// o `config_dir` dele caía em `$HOME/.config/seele`, e o `connect` lia a
    /// identidade e o retrato de quem roda a bateria e gravava pinos de
    /// `127.0.0.1` ao lado dos dela. Montada aqui, nenhum teste do módulo pode
    /// esquecer; e a casa que o app vai usar é conferida antes de qualquer
    /// `connect`, para um caminho novo do `config_dir` reprovar em vez de
    /// escrever na casa de alguém.
    pub(super) fn app() -> &'static AppHandle<MockRuntime> {
        let casa = casa();
        let app = mock_builder()
            .manage(Session::default())
            .build(mock_context(noop_assets()))
            .expect("o app falso não montou");
        let handle = app.handle().clone();
        let do_app = super::config_dir(&handle);
        assert_eq!(
            std::path::Path::new(&do_app),
            casa,
            "a casa do app falso não é a temporária deste módulo: um `connect` daqui escreveria \
             a identidade e os pinos em {do_app}"
        );
        Box::leak(Box::new(app));
        Box::leak(Box::new(handle))
    }

    /// Quem recebeu `corpo`, entre os servidores dados, esperando até `prazo`.
    async fn quem_recebeu(
        ouvidos: &mut [(
            &str,
            tokio::sync::broadcast::Receiver<seele_server::server::Event>,
        )],
        corpo: &str,
        prazo: Duration,
    ) -> Vec<String> {
        let fim = tokio::time::Instant::now() + prazo;
        let mut receberam = Vec::new();
        while tokio::time::Instant::now() < fim {
            for (nome, ouvido) in ouvidos.iter_mut() {
                while let Ok(evento) = ouvido.try_recv() {
                    if let seele_server::server::Event::MessagePosted(mensagem) = evento {
                        if mensagem.body == corpo && !receberam.contains(&(*nome).to_owned()) {
                            receberam.push((*nome).to_owned());
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        receberam
    }

    /// Quantos estão presentes no servidor, esperando até `prazo` pelo número
    /// que se espera.
    async fn presentes(servidor: &Daemon, espera: usize, prazo: Duration) -> usize {
        let fim = tokio::time::Instant::now() + prazo;
        loop {
            let agora = servidor.server().presentes.lock().await.todos().len();
            if agora == espera || tokio::time::Instant::now() >= fim {
                return agora;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Um retrato desta máquina, para o `connect` ter um efeito de entrada que
    /// o servidor vê: ele manda o retrato guardado logo depois de conectar.
    fn guardar_um_retrato() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("o retrato de teste não codificou");
        std::fs::write(casa().join("retrato.png"), png.into_inner())
            .expect("o retrato de teste não gravou");
    }

    /// Um `connect` numa thread só dele, com um runtime próprio.
    ///
    /// Para os testes que seguram um cadeado da sessão e precisam que o
    /// `connect` pare nele: no runtime de uma thread só em que os servidores
    /// andam, um `connect` parado num cadeado pararia os servidores junto.
    fn connect_noutra_thread(
        app: &'static AppHandle<MockRuntime>,
        servidor: String,
        apelido: &str,
    ) -> std::thread::JoinHandle<Result<super::Entrada, super::FalhaDaEntrada>> {
        let apelido = apelido.to_owned();
        std::thread::spawn(move || {
            runtime().block_on(connect(
                app.clone(),
                app.state::<Session>(),
                servidor,
                apelido,
                false,
                None,
                None,
            ))
        })
    }

    /// Espera uma thread terminar sem parar o runtime do teste, até `prazo`.
    async fn quando_terminar<T>(thread: std::thread::JoinHandle<T>, prazo: Duration) -> T {
        let fim = tokio::time::Instant::now() + prazo;
        while !thread.is_finished() {
            assert!(
                tokio::time::Instant::now() < fim,
                "a thread não terminou no prazo"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        thread.join().expect("a thread caiu")
    }

    /// Segura, numa thread própria, a guarda que `tomar` devolve — um cadeado
    /// da sessão —, até o `Sender` devolvido ser usado ou cair. Numa thread, e
    /// não no teste, para nenhuma guarda atravessar um `await`.
    fn segurar<G>(
        tomar: impl FnOnce() -> G + Send + 'static,
    ) -> (std::sync::mpsc::Sender<()>, std::thread::JoinHandle<()>) {
        let (tomado, foi_tomado) = std::sync::mpsc::channel();
        let (soltar, quando_soltar) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let guarda = tomar();
            let _ = tomado.send(());
            let _ = quando_soltar.recv();
            drop(guarda);
        });
        foi_tomado.recv().expect("o cadeado não foi tomado");
        (soltar, thread)
    }

    /// Espera o servidor anunciar um retrato novo, até `prazo`.
    async fn um_retrato_chegou(
        ouvido: &mut tokio::sync::broadcast::Receiver<seele_server::server::Event>,
        prazo: Duration,
    ) -> bool {
        tokio::time::timeout(prazo, async {
            loop {
                match ouvido.recv().await {
                    Ok(seele_server::server::Event::PersonIconChanged { .. }) => break true,
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break false,
                }
            }
        })
        .await
        .unwrap_or(false)
    }

    /// O cenário inteiro, do começo da velha à mensagem: o que ficou errado.
    async fn a_velha_termina_depois_da_nova() -> Vec<String> {
        guardar_um_retrato();
        let (velho, servidor_velho) = servidor("Velho").await;
        let (novo, servidor_novo) = servidor("Novo").await;
        let app = app();
        let sessao = app.state::<Session>();

        // A velha sai primeiro e fica presa no aperto de mão do servidor dela.
        let trava = servidor_velho.server().persistence.lock().await;
        let velha = tokio::spawn(connect(
            app.clone(),
            app.state::<Session>(),
            velho.to_string(),
            "velha".to_owned(),
            false,
            None,
            None,
        ));
        let comeco = tokio::time::Instant::now();
        while sessao.geracao() == 0 && comeco.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(sessao.geracao(), 1, "a tentativa velha não começou");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !velha.is_finished(),
            "a tentativa velha terminou antes da hora"
        );

        // A nova entra, inteira, com a velha ainda andando.
        let nova = tokio::time::timeout(
            Duration::from_secs(10),
            connect(
                app.clone(),
                app.state::<Session>(),
                novo.to_string(),
                "nova".to_owned(),
                false,
                None,
                None,
            ),
        )
        .await
        .expect("a tentativa nova não voltou")
        .map_err(|falha| format!("{falha:?}"))
        .expect("a tentativa nova não entrou");
        assert_eq!(nova.geracao, 2, "a tentativa nova não abriu a geração dois");
        let canal = nova
            .snapshot
            .channels
            .first()
            .map(|canal| canal.id)
            .expect("o servidor novo não tem canal nenhum");

        // Solta a velha, e ela termina depois da nova. O que o servidor velho
        // recebe dela daqui em diante é o que os efeitos de entrada fizeram.
        let mut do_velho = servidor_velho.server().events.subscribe();
        drop(trava);
        let velha = tokio::time::timeout(Duration::from_secs(10), velha)
            .await
            .expect("a tentativa velha não voltou")
            .expect("a tarefa da tentativa velha caiu");

        let mut vermelhos = Vec::new();
        match &velha {
            Ok(entrada) => vermelhos.push(format!(
                "a tentativa velha voltou como entrada (geração {}), depois de a nova ter entrado",
                entrada.geracao
            )),
            Err(falha) => {
                let dita = serde_json::to_value(falha).unwrap_or_default();
                if dita.get("substituida") != Some(&serde_json::Value::Bool(true)) {
                    vermelhos.push(format!(
                        "a recusa da tentativa velha não diz que ela foi substituída, e a janela \
                         não tem como calá-la: {dita}"
                    ));
                }
            }
        }
        // Pelo nome que cada servidor dá a si mesmo no aperto de mão, e não
        // pela impressão: a FFI da linha 0.15 não diz a impressão da conexão,
        // e o teste é o mesmo nas duas linhas.
        let qual = |nome: Option<&str>| match nome {
            Some("Novo") => "o servidor novo",
            Some("Velho") => "o servidor velho",
            Some(_) => "um servidor desconhecido",
            None => "nenhum",
        };
        let no_slot = sessao
            .connection()
            .ok()
            .map(|conexao| conexao.snapshot().server);
        if qual(no_slot.as_deref()) != "o servidor novo" {
            vermelhos.push(format!(
                "o slot da sessão ficou com {}, e não com a conexão da nova",
                qual(no_slot.as_deref())
            ));
        }
        if sessao.geracao() != nova.geracao {
            vermelhos.push(format!(
                "a geração de pé é {}, e não a da nova ({})",
                sessao.geracao(),
                nova.geracao
            ));
        }
        let onde = sessao
            .endereco_da_sessao
            .lock()
            .ok()
            .and_then(|onde| onde.clone());
        if onde.as_deref() != Some(novo.to_string().as_str()) {
            vermelhos.push(format!(
                "o endereço da sessão é {onde:?}, e não o do servidor novo ({novo})"
            ));
        }

        // A mensagem que a pessoa manda em seguida: qual servidor a recebe.
        let mut ouvidos = [
            (
                "o servidor velho",
                servidor_velho.server().events.subscribe(),
            ),
            ("o servidor novo", servidor_novo.server().events.subscribe()),
        ];
        let corpo = "para onde eu vou?";
        if let Err(erro) = send_message(app.state::<Session>(), canal, corpo.to_owned()) {
            vermelhos.push(format!("a mensagem não saiu: {erro:?}"));
        }
        let receberam = quem_recebeu(&mut ouvidos, corpo, Duration::from_secs(2)).await;
        if receberam != ["o servidor novo"] {
            vermelhos.push(format!(
                "a mensagem mandada em seguida chegou a {receberam:?}, e não só ao servidor novo"
            ));
        }

        // E a conexão da velha não fica pendurada no servidor que a pessoa
        // deixou, nem a da nova sai do servidor em que ela está.
        let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
        if no_velho != 0 {
            vermelhos.push(format!(
                "a conexão da velha continua presente no servidor velho ({no_velho})"
            ));
        }
        let no_novo = presentes(&servidor_novo, 1, Duration::from_secs(2)).await;
        if no_novo != 1 {
            vermelhos.push(format!(
                "a pessoa não está presente no servidor novo ({no_novo})"
            ));
        }

        // Nem os efeitos de entrada valem para ela: o retrato desta máquina
        // não vai ao servidor que a pessoa deixou.
        let mut retratos = 0;
        while let Ok(evento) = do_velho.try_recv() {
            if matches!(
                evento,
                seele_server::server::Event::PersonIconChanged { .. }
            ) {
                retratos += 1;
            }
        }
        if retratos != 0 {
            vermelhos.push(format!(
                "a tentativa velha mandou o retrato ao servidor que a pessoa deixou ({retratos} \
                 vez(es)): os efeitos de entrada valeram para ela"
            ));
        }
        vermelhos
    }

    /// O cenário, com o `seele.log` capturado: a linha da velha descartada
    /// sai na thread do teste, que é a do runtime em que o `connect` corre.
    #[test]
    fn a_velha_que_entra_depois_da_nova_nao_toma_a_conexao() {
        let _vez = vez();
        let runtime = runtime();
        let (mut vermelhos, rastro) =
            crate::rastro_de_teste::capturar(|| runtime.block_on(a_velha_termina_depois_da_nova()));
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("substituída") && linha.contains(" WARN "));
        if linha.is_none() {
            vermelhos.push(
                "o seele.log não tem uma linha WARN dizendo que a entrada substituída foi \
                 descartada"
                    .to_owned(),
            );
        }
        assert!(
            vermelhos.is_empty(),
            "a entrada velha que chegou tarde tomou a sessão nova:\n  {}",
            vermelhos.join("\n  ")
        );
    }

    /// **A outra ordem**: a velha entra no Rust antes de a nova chegar a ele.
    ///
    /// A nova já começou na janela (`++tentativas`), e ainda não chamou o
    /// `connect` — está no `conferir_esta_versao`, por exemplo. A geração da
    /// velha ainda é a de pé, e ela entra. A janela não a quer, e a solta pela
    /// geração dela. O que se cobra: a velha sai do slot e do servidor dela, a
    /// nova entra em vez de receber `AlreadyConnected`, a mensagem vai à nova,
    /// e um pedido com uma geração que já não é a de pé não derruba ninguém.
    async fn a_janela_larga_a_velha_antes_de_a_nova_chegar() -> Vec<String> {
        casa();
        let (velho, servidor_velho) = servidor("Velho").await;
        let (novo, servidor_novo) = servidor("Novo").await;
        let app = app();
        let sessao = app.state::<Session>();
        let mut vermelhos = Vec::new();

        let velha = tokio::time::timeout(
            Duration::from_secs(10),
            connect(
                app.clone(),
                app.state::<Session>(),
                velho.to_string(),
                "velha".to_owned(),
                false,
                None,
                None,
            ),
        )
        .await
        .expect("a tentativa velha não voltou")
        .map_err(|falha| format!("{falha:?}"))
        .expect("a tentativa velha não entrou");
        assert_eq!(
            presentes(&servidor_velho, 1, Duration::from_secs(2)).await,
            1
        );

        // A janela larga a velha pela geração dela.
        if !super::soltar_a_entrada_em(&sessao, velha.geracao) {
            vermelhos.push(format!(
                "pedir para soltar a entrada da geração {} — a de pé — não soltou nada",
                velha.geracao
            ));
        }
        if sessao.connection().is_ok() {
            vermelhos
                .push("a conexão da velha continua no slot depois de a janela largá-la".into());
        }
        if sessao.geracao_vale(velha.geracao) {
            vermelhos.push(format!(
                "a geração da velha ({}) continua de pé depois de a janela largá-la: um comando \
                 dela ainda seria atendido",
                velha.geracao
            ));
        }
        let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
        if no_velho != 0 {
            vermelhos.push(format!(
                "a conexão largada continua presente no servidor velho ({no_velho})"
            ));
        }
        let onde = sessao
            .endereco_da_sessao
            .lock()
            .ok()
            .and_then(|onde| onde.clone());
        if onde.is_some() {
            vermelhos.push(format!(
                "o endereço da sessão continua {onde:?} depois de a janela largar a entrada"
            ));
        }

        // A nova chega ao `connect` e entra.
        let nova = tokio::time::timeout(
            Duration::from_secs(10),
            connect(
                app.clone(),
                app.state::<Session>(),
                novo.to_string(),
                "nova".to_owned(),
                false,
                None,
                None,
            ),
        )
        .await
        .expect("a tentativa nova não voltou");
        let nova = match nova {
            Ok(entrada) => entrada,
            Err(falha) => {
                vermelhos.push(format!(
                    "a tentativa nova não entrou depois de a janela largar a velha: {}",
                    serde_json::to_string(&falha).unwrap_or_default()
                ));
                return vermelhos;
            }
        };
        let canal = nova
            .snapshot
            .channels
            .first()
            .map(|canal| canal.id)
            .expect("o servidor novo não tem canal nenhum");

        // Um pedido com a geração da velha, que já não é a de pé, não derruba
        // a nova.
        if super::soltar_a_entrada_em(&sessao, velha.geracao) {
            vermelhos.push(
                "um pedido com a geração da velha, que já não era a de pé, soltou a entrada nova"
                    .into(),
            );
        }
        if sessao.geracao() != nova.geracao {
            vermelhos.push("o pedido com a geração velha revogou a geração da nova".into());
        }
        let mut ouvidos = [
            (
                "o servidor velho",
                servidor_velho.server().events.subscribe(),
            ),
            ("o servidor novo", servidor_novo.server().events.subscribe()),
        ];
        let corpo = "e agora, para onde?";
        if let Err(erro) = send_message(app.state::<Session>(), canal, corpo.to_owned()) {
            vermelhos.push(format!("a mensagem não saiu: {erro:?}"));
        }
        let receberam = quem_recebeu(&mut ouvidos, corpo, Duration::from_secs(2)).await;
        if receberam != ["o servidor novo"] {
            vermelhos.push(format!(
                "a mensagem mandada depois chegou a {receberam:?}, e não só ao servidor novo"
            ));
        }
        let no_novo = presentes(&servidor_novo, 1, Duration::from_secs(2)).await;
        if no_novo != 1 {
            vermelhos.push(format!(
                "a pessoa não está presente no servidor novo ({no_novo})"
            ));
        }
        vermelhos
    }

    /// **A nova começa entre as duas conferências da velha** (I1 da revisão
    /// do E54).
    ///
    /// A velha passa a primeira conferência e aplica os efeitos de entrada; a
    /// nova começa e revoga a geração antes de a velha gravar o slot. O teste
    /// para a velha no cadeado do slot, que ele segura: solta o banco do
    /// servidor velho, espera o retrato da velha chegar a ele (ela passou da
    /// primeira conferência), revoga a geração como o `connect` da nova faz ao
    /// começar, e só então solta o slot. A velha tem de ser largada ali.
    async fn a_nova_comeca_entre_as_duas_conferencias() -> Vec<String> {
        guardar_um_retrato();
        let (velho, servidor_velho) = servidor("Velho").await;
        let app = app();
        let sessao = app.state::<Session>();
        let mut vermelhos = Vec::new();

        let trava = servidor_velho.server().persistence.lock().await;
        let velha = connect_noutra_thread(app, velho.to_string(), "velha");
        let comeco = tokio::time::Instant::now();
        while sessao.geracao() == 0 && comeco.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(sessao.geracao(), 1, "a tentativa velha não começou");

        // O slot, seguro: a velha para nele, na segunda conferência.
        let (soltar_o_slot, quem_segura) =
            segurar(move || app.state::<Session>().inner().connection.lock());
        let mut do_velho = servidor_velho.server().events.subscribe();
        drop(trava);
        assert!(
            um_retrato_chegou(&mut do_velho, Duration::from_secs(8)).await,
            "o retrato da velha não chegou ao servidor velho: ela não passou da primeira \
             conferência, e este teste não mede a segunda"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !velha.is_finished(),
            "a velha terminou sem esperar o slot, e este teste não mede a segunda conferência"
        );

        // A nova começa, e revoga a geração da velha.
        sessao.revogar();
        let _ = soltar_o_slot.send(());
        quem_segura.join().expect("quem segurava o slot caiu");

        match quando_terminar(velha, Duration::from_secs(10)).await {
            Ok(entrada) => vermelhos.push(format!(
                "a velha voltou como entrada (geração {}) depois de a nova revogar a geração dela \
                 no meio dos efeitos de entrada",
                entrada.geracao
            )),
            Err(falha) => {
                let dita = serde_json::to_value(&falha).unwrap_or_default();
                if dita.get("substituida") != Some(&serde_json::Value::Bool(true)) {
                    vermelhos.push(format!(
                        "a recusa da velha substituída entre as duas conferências não vem marcada: \
                         {dita}"
                    ));
                }
            }
        }
        if let Ok(conexao) = sessao.connection() {
            vermelhos.push(format!(
                "o slot ficou com a conexão da velha ({})",
                conexao.snapshot().server
            ));
        }
        let onde = sessao
            .endereco_da_sessao
            .lock()
            .ok()
            .and_then(|onde| onde.clone());
        if onde.is_some() {
            vermelhos.push(format!(
                "o endereço da sessão ficou {onde:?}, o do servidor que a pessoa deixou"
            ));
        }
        let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
        if no_velho != 0 {
            vermelhos.push(format!(
                "a velha continua presente no servidor velho ({no_velho})"
            ));
        }
        vermelhos
    }

    #[test]
    fn a_velha_que_a_nova_substitui_entre_as_duas_conferencias_nao_grava_o_slot() {
        let _vez = vez();
        let vermelhos = runtime().block_on(a_nova_comeca_entre_as_duas_conferencias());
        assert!(
            vermelhos.is_empty(),
            "a velha substituída entre as duas conferências do `connect` tomou a sessão:\n  {}",
            vermelhos.join("\n  ")
        );
    }

    /// **A nova que falha não deixa a velha órfã no slot** (M1 da revisão do
    /// E54, a sonda P53).
    ///
    /// A ordem forçada: a nova passa pelo `AlreadyConnected` com o slot vazio
    /// e para antes de revogar (o teste segura `mods_nativos`, que a revogação
    /// toma antes de somar); a velha passa as duas conferências com a geração
    /// dela ainda de pé; a nova revoga e falha (a porta 99999 não existe, e a
    /// recusa sai sem rede nenhuma). A janela larga a velha, porque a nova é a
    /// mais nova, e pede para soltá-la pela geração dela — que já não é a de
    /// pé. Sem conferir o slot e revogar com o cadeado dele na mão, a velha
    /// ficava no slot e presente no servidor deixado, e toda entrada seguinte
    /// recebia `AlreadyConnected` até o app fechar.
    async fn a_nova_falha_no_meio_da_velha() -> Vec<String> {
        // O retrato é a prova de que a velha passou da primeira conferência
        // (R1-I2 da re-revisão: sem ele, o teste dependia de outro do módulo
        // tê-lo gravado antes).
        guardar_um_retrato();
        let (velho, servidor_velho) = servidor("Velho").await;
        let (novo, _servidor_novo) = servidor("Novo").await;
        let app = app();
        let sessao = app.state::<Session>();
        let mut vermelhos = Vec::new();

        let trava = servidor_velho.server().persistence.lock().await;
        let velha = connect_noutra_thread(app, velho.to_string(), "velha");
        let comeco = tokio::time::Instant::now();
        while sessao.geracao() == 0 && comeco.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(sessao.geracao(), 1, "a tentativa velha não começou");
        tokio::time::sleep(Duration::from_millis(300)).await;

        // A nova começa e para antes de revogar.
        let (soltar_os_mods, quem_segura) =
            segurar(move || app.state::<Session>().inner().mods_nativos.lock());
        let nova = connect_noutra_thread(app, "127.0.0.1:99999".to_owned(), "nova");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            sessao.geracao(),
            1,
            "a nova revogou sem esperar os MODs, e a ordem não foi a forçada"
        );

        // A velha termina o aperto de mão e passa a primeira conferência.
        let mut do_velho = servidor_velho.server().events.subscribe();
        drop(trava);
        assert!(
            um_retrato_chegou(&mut do_velho, Duration::from_secs(8)).await,
            "o retrato da velha não chegou ao servidor velho: ela não passou da primeira \
             conferência, e a ordem não foi a forçada"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;

        // A nova revoga e falha.
        let _ = soltar_os_mods.send(());
        quem_segura.join().expect("quem segurava os MODs caiu");
        let da_nova = quando_terminar(nova, Duration::from_secs(10)).await;
        let da_velha = quando_terminar(velha, Duration::from_secs(10)).await;

        // **A ordem foi a forçada** (R1-M5 da re-revisão). A nova falha pela
        // porta que não existe, e não por `AlreadyConnected`: chegando ao slot
        // depois de a velha gravá-lo, a recusa seria essa, a janela soltaria a
        // velha pela geração ainda de pé, e o teste passaria sem medir nada.
        match &da_nova {
            Ok(_) => vermelhos.push("a nova a uma porta que não existe entrou".to_owned()),
            Err(falha) if falha.falha.error == super::ConnectionError::AlreadyConnected => {
                vermelhos.push(
                    "a nova recebeu `AlreadyConnected`: chegou ao slot depois de a velha gravá-lo, \
                     e a ordem não foi a forçada"
                        .to_owned(),
                );
            }
            Err(_) => {}
        }
        // A velha cai na segunda conferência dela, marcada como substituída.
        match &da_velha {
            Ok(entrada) => vermelhos.push(format!(
                "a velha voltou como entrada (geração {}) com a geração da nova de pé",
                entrada.geracao
            )),
            Err(falha) => {
                let dita = serde_json::to_value(falha).unwrap_or_default();
                if dita.get("substituida") != Some(&serde_json::Value::Bool(true)) {
                    vermelhos.push(format!(
                        "a recusa da velha não vem marcada como substituída: {dita}"
                    ));
                }
            }
        }
        // O slot fica vazio sem pedido nenhum da janela.
        if let Ok(conexao) = sessao.connection() {
            vermelhos.push(format!(
                "a velha ficou no slot ({}) sem a janela pedir nada",
                conexao.snapshot().server
            ));
        }
        // E o que a janela faz em seguida, para o vermelho dizer a
        // consequência: ela larga a velha que voltou entrando, pela geração
        // dela, que já não é a de pé.
        if let Ok(entrada) = &da_velha {
            let _ = super::soltar_a_entrada_em(&sessao, entrada.geracao);
            if let Ok(conexao) = sessao.connection() {
                vermelhos.push(format!(
                    "a velha ficou órfã no slot ({}): a janela a largou e o Rust não a soltou",
                    conexao.snapshot().server
                ));
            }
        }
        let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
        if no_velho != 0 {
            vermelhos.push(format!(
                "a velha continua presente no servidor que a pessoa deixou ({no_velho})"
            ));
        }
        let seguinte = quando_terminar(
            connect_noutra_thread(app, novo.to_string(), "seguinte"),
            Duration::from_secs(10),
        )
        .await;
        if let Err(falha) = seguinte {
            vermelhos.push(format!(
                "a entrada seguinte não entrou: {}",
                serde_json::to_string(&falha).unwrap_or_default()
            ));
        }
        vermelhos
    }

    #[test]
    fn a_nova_que_falha_no_meio_da_velha_nao_a_deixa_orfa_no_slot() {
        let _vez = vez();
        let vermelhos = runtime().block_on(a_nova_falha_no_meio_da_velha());
        assert!(
            vermelhos.is_empty(),
            "a nova falhou no meio da velha, e a velha ficou com a sessão sem tela:\n  {}",
            vermelhos.join("\n  ")
        );
    }

    /// **As duas conferências do `connect`, pela forma.**
    ///
    /// O comportamento das duas tem teste: a primeira em
    /// `a_velha_que_entra_depois_da_nova_nao_toma_a_conexao`, e a segunda em
    /// `a_velha_que_a_nova_substitui_entre_as_duas_conferencias_nao_grava_o_slot`,
    /// que para o `connect` no cadeado do slot. (Este doc dizia que nenhum teste
    /// conseguia parar o `connect` ali, e não era verdade: a sonda P2 da
    /// revisão o parou, e o teste dela entrou aqui.) Este guarda prende a forma
    /// que dá aquele comportamento, sem ler comentário:
    ///
    /// 1. entre o `connect_watching` voltar e o primeiro efeito de entrada há a
    ///    conferência da geração, e ela **volta** com a substituída largada;
    /// 2. o slot só é gravado com o cadeado dele tomado e a geração conferida
    ///    ali, sem espera no meio;
    /// 3. quem não gravou **volta** largada, antes de o endereço da sessão ser
    ///    escrito. Largar sem voltar (o mutante R3b da revisão) fechava a
    ///    conexão e seguia gravando o endereço e a lista da velha, e devolvia
    ///    `Ok`: o guarda só conferia a ordem do texto, e o deixava passar.
    #[test]
    fn o_connect_confere_a_geracao_antes_dos_efeitos_e_ao_gravar_o_slot() {
        let fonte = super::fonte_sem_comentarios();
        let corpo = super::corpo_de(&fonte, "async fn connect<");
        let Some(volta) = corpo.find("let (connection, veredito) = resultado") else {
            panic!("o `connect` não recebe mais a conexão do `connect_watching`: {corpo}");
        };
        let Some(primeiro_efeito) = [
            "connection.subscribe(",
            "vestir_as_escolhas_gravadas(",
            "set_person_icon(",
            "consentir_no_caminho_entre_pares(",
        ]
        .iter()
        .filter_map(|efeito| corpo[volta..].find(efeito).map(|n| volta + n))
        .min() else {
            panic!("o `connect` não tem mais efeitos de entrada que este guarda conheça: {corpo}");
        };
        let antes_dos_efeitos = &corpo[volta..primeiro_efeito];
        assert!(
            antes_dos_efeitos.contains("session.confere_geracao(geracao).is_err()")
                && antes_dos_efeitos.contains("return Err(largar_a_substituida("),
            "o `connect` aplica os efeitos de entrada sem conferir antes se a geração dele \
             ainda é a de pé: a tentativa que outra substituiu manda o retrato e o \
             consentimento ao servidor que a pessoa deixou.\n{antes_dos_efeitos}"
        );
        let Some(gravar) = corpo.find("*slot = Some(connection)") else {
            panic!("o `connect` não grava mais o slot: {corpo}");
        };
        let Some(cadeado) = corpo[..gravar].rfind("session.connection.lock()") else {
            panic!("o `connect` grava o slot sem tomar o cadeado dele: {corpo}");
        };
        let ao_gravar = &corpo[cadeado..gravar];
        assert!(
            ao_gravar.contains("session.confere_geracao(geracao).is_ok()")
                && !ao_gravar.contains(".await"),
            "o `connect` grava o slot sem conferir a geração com o cadeado na mão: a tentativa \
             que outra substituiu no meio dos efeitos de entrada grava a conexão dela por cima \
             da nova.\n{ao_gravar}"
        );
        let depois = &corpo[gravar..];
        let Some(ramo) = depois.find("if let Some(connection) = substituida {") else {
            panic!(
                "depois de gravar o slot, o `connect` não tem mais o ramo da tentativa que não \
                 gravou: {depois}"
            );
        };
        let Some(fim_do_ramo) = depois[ramo..].find("\n    }\n").map(|n| ramo + n) else {
            panic!("o ramo da tentativa que não gravou não fecha: {depois}");
        };
        let bloco = &depois[ramo..fim_do_ramo];
        let endereco = depois.find("endereco_da_sessao");
        assert!(
            bloco.contains("return Err(largar_a_substituida(")
                && endereco.is_some_and(|endereco| fim_do_ramo < endereco),
            "depois de gravar o slot, a tentativa que não gravou não volta largada antes de o \
             endereço da sessão ser escrito: largada sem voltar, ela fecha a conexão e segue \
             gravando o endereço, o alvo e a lista de visitados dela, e volta `Ok`.\n{bloco}"
        );
    }

    #[test]
    fn a_entrada_que_a_janela_larga_e_solta_pela_geracao_dela() {
        let _vez = vez();
        let runtime = runtime();
        let (mut vermelhos, rastro) = crate::rastro_de_teste::capturar(|| {
            runtime.block_on(a_janela_larga_a_velha_antes_de_a_nova_chegar())
        });
        let linhas = rastro
            .lines()
            .filter(|linha| linha.contains("a janela largou") && linha.contains(" WARN "))
            .count();
        if linhas != 1 {
            vermelhos.push(format!(
                "o seele.log tem {linhas} linha(s) WARN de entrada que a janela largou, e não uma: \
                 a que soltou diz, a que não soltou não diz como se tivesse soltado"
            ));
        }
        assert!(
            vermelhos.is_empty(),
            "a entrada que a janela largou não foi solta como devia:\n  {}",
            vermelhos.join("\n  ")
        );
    }
}

/// **O apelido grande demais é recusado com nome, antes de abrir a conexão**
/// (pendência 58, item 55; lote C55 da F5).
///
/// O apelido gravado nesta máquina (`escolher_apelido_local`) vai no `Hello` da
/// conexão seguinte, e o `seele-proto` o limita a `MAX_NICKNAME_LEN` **bytes**.
/// O campo do perfil contava caracteres até este lote, então um apelido de 20
/// letras `á` (40 bytes) pode estar gravado de antes. O teste conecta pelo
/// `connect` do app, contra um servidor de verdade nesta máquina, pela casa
/// temporária do módulo do E54 (`a_entrada_velha_que_chega_tarde`), que é a
/// única `SEELE_HOME` dos testes deste binário.
///
/// **Medido na base do lote** (`3b1f57c8`), com este teste ainda em
/// `#[ignore]` como instrumento:
/// - 16 `á` (32 B, no teto): `Ok(geracao 1)` em 16 ms, e o servidor a vê;
/// - 20 `á` (40 B): `{"error":"Unreachable", …}` em 5 ms, com a trilha
///   `Parada` → `Tentando` (127.0.0.1) → `Desistiu { Unreachable }` aos 3 ms,
///   e ninguém presente no servidor. O QUIC e o TLS abriam, o `encode` do
///   `Hello` recusava o apelido, e a escrita do `Hello`
///   (`seele-core/src/client.rs`, `handshake`) traduzia qualquer erro em
///   `ConnectError::Unreachable`. A janela lia «NADA RESPONDEU NESSE
///   ENDEREÇO» com o servidor de pé.
///
/// **Agora** a FFI confere o apelido antes de abrir a conexão
/// (`Connection::abrir`) e recusa com `ConnectionError::ApelidoGrandeDemais`,
/// que leva o teto. A trilha vazia é a prova de que nada foi tentado: a
/// chegada não começou, e nenhum candidato foi discado.
#[cfg(test)]
mod o_apelido_grande_demais_no_hello {
    use super::a_entrada_velha_que_chega_tarde::{app, runtime, servidor, vez};
    use super::{connect, Session};
    use std::time::Duration;
    use tauri::Manager;

    #[test]
    fn o_apelido_grande_demais_e_recusado_com_nome_antes_de_abrir_a_conexao() {
        let _vez = vez();
        runtime().block_on(async {
            let teto = seele_ffi::Connection::limites_dos_campos().apelido;
            let (endereco, servidor) = servidor("Apelido").await;
            let mut vermelhos = Vec::new();

            // Vinte letras `á` para o teto de hoje: 40 bytes, a medida da base.
            let grande = "á".repeat(teto / 2 + 4);
            let primeira = app();
            let resultado = tokio::time::timeout(
                Duration::from_secs(30),
                connect(
                    primeira.clone(),
                    primeira.state::<Session>(),
                    endereco.to_string(),
                    grande.clone(),
                    false,
                    None,
                    None,
                ),
            )
            .await
            .expect("o connect com o apelido grande demais não voltou");
            match &resultado {
                Ok(entrada) => vermelhos.push(format!(
                    "um apelido de {} bytes entrou (geração {}), e o fio leva {teto}",
                    grande.len(),
                    entrada.geracao
                )),
                Err(falha) => {
                    let dita = serde_json::to_value(falha).unwrap_or_default();
                    let esperada =
                        serde_json::json!({ "ApelidoGrandeDemais": { "limite_bytes": teto } });
                    if dita.get("error") != Some(&esperada) {
                        vermelhos.push(format!(
                            "um apelido de {} bytes não voltou recusado com nome e teto \
                             ({esperada}), e a janela não sabe o que dizer: {dita}",
                            grande.len()
                        ));
                    }
                    let tentou = !matches!(
                        dita.get("trail").and_then(serde_json::Value::as_array),
                        Some(trilha) if trilha.is_empty()
                    );
                    if tentou {
                        vermelhos.push(format!(
                            "a conexão foi aberta para um apelido que o `Hello` não leva: a \
                             trilha não está vazia: {}",
                            dita.get("trail").cloned().unwrap_or_default()
                        ));
                    }
                }
            }
            if let Ok(sessao) = primeira.state::<Session>().connection() {
                sessao.disconnect();
            }

            // E um apelido no teto ainda entra: sem isto, um `connect` que
            // recusasse tudo passaria.
            let certo = "á".repeat(teto / 2);
            let segunda = app();
            match tokio::time::timeout(
                Duration::from_secs(30),
                connect(
                    segunda.clone(),
                    segunda.state::<Session>(),
                    endereco.to_string(),
                    certo.clone(),
                    false,
                    None,
                    None,
                ),
            )
            .await
            .expect("o connect com o apelido no teto não voltou")
            {
                Ok(_) => {
                    let presentes = servidor.server().presentes.lock().await.todos().len();
                    if presentes != 1 {
                        vermelhos.push(format!(
                            "depois das duas tentativas, o servidor tem {presentes} presentes, \
                             e não só a do apelido no teto"
                        ));
                    }
                }
                Err(falha) => vermelhos.push(format!(
                    "um apelido de {} bytes, no teto, não entrou: {}",
                    certo.len(),
                    serde_json::to_value(&falha).unwrap_or_default()
                )),
            }
            if let Ok(sessao) = segunda.state::<Session>().connection() {
                sessao.disconnect();
            }

            assert!(
                vermelhos.is_empty(),
                "o apelido grande demais no `Hello`:\n  {}",
                vermelhos.join("\n  ")
            );
        });
    }
}

/// As hospedagens deste app (Plano 2, spec de 23/09, «Hospedagens»): o
/// servidor guardado, que a pessoa sobe com HOSPEDAR, e a sala pessoal.
///
/// **Duas, e independentes.** Entrar e sair de uma não mexe na outra. Onde a
/// sessão desta janela está em relação a elas é [`onde_estou`], e é a única
/// resposta a «estou no meu servidor?».
#[derive(Default)]
struct Hospedagens {
    /// O servidor guardado no ar, quando está.
    guardado: Option<Hospedado>,
    /// A sala pessoal no ar, quando está. Ela sobe sozinha, no `setup`
    /// ([`sala::subir`]), e o estado dela é [`Session::sala`].
    pessoal: Option<Hospedado>,
}

impl Hospedagens {
    /// O servidor guardado, que é o alvo da portaria, dos MODs e do convite.
    fn do_guardado(&self) -> Option<&seele_server::hospedagem::Hospedagem> {
        self.guardado.as_ref().map(|guardado| &guardado.hospedagem)
    }

    /// As impressões das duas, para [`onde_estou_em`]: é pela chave que uma
    /// sessão é de uma delas, e não pelo endereço por onde entrou.
    fn impressoes(&self) -> (Option<String>, Option<String>) {
        let impressao = |hospedado: &Option<Hospedado>| {
            hospedado
                .as_ref()
                .map(|hospedado| hospedado.hospedagem.impressao_digital().to_owned())
        };
        (impressao(&self.guardado), impressao(&self.pessoal))
    }
}

/// Uma hospedagem de pé, e qual servidor guardado ela é.
struct Hospedado {
    hospedagem: seele_server::hospedagem::Hospedagem,
    /// O identificador do servidor guardado. `None` no legado (a máquina que
    /// hospedava antes de haver lista, com o banco de sempre) e na sala.
    servidor: Option<String>,
}

/// Onde a sessão desta janela está, em relação às hospedagens deste app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
enum OndeEstou {
    /// Não há sessão.
    Nenhum,
    /// No servidor guardado que esta janela hospeda.
    Guardado,
    /// Na sala pessoal desta máquina.
    Pessoal,
    /// Em qualquer outro lugar, inclusive num servidor desta máquina que este
    /// app não hospeda (um `seeled` à parte, por exemplo).
    Outro,
}

/// **A única resposta a «estou no meu servidor?»** (Plano 2). Eram três, cada
/// uma com o seu critério: o `hospedado_aqui` deste arquivo, o `hospedando` do
/// `estado_da_porta` e o `nossoServidorEm` da tela de entrada. Com duas
/// hospedagens, nenhum dos três dizia em qual das duas a sessão está.
#[tauri::command]
fn onde_estou(session: State<'_, Session>) -> OndeEstou {
    onde_estou_na(&session)
}

/// [`onde_estou`] sobre a sessão, para quem não é comando: o sim de quem
/// hospeda ([`gravar_o_sim_de_quem_hospeda`]) e o ATENDER
/// ([`sala::atender_em`]) perguntam a mesma coisa, e têm de ouvir a mesma
/// resposta.
fn onde_estou_na(session: &Session) -> OndeEstou {
    let endereco = session
        .endereco_da_sessao
        .lock()
        .ok()
        .and_then(|onde| onde.clone());
    // A chave que o aperto de mão conferiu (`Connection::impressao_do_servidor`),
    // em toda conexão que subiu, e não só nas que o veredito conta.
    let impressao = session
        .connection()
        .ok()
        .and_then(|connection| connection.impressao_do_servidor());
    let (guardado, pessoal) = session
        .hospedagens
        .lock()
        .map_or((None, None), |hospedagens| hospedagens.impressoes());
    onde_estou_em(
        endereco.as_deref(),
        impressao.as_deref(),
        guardado.as_deref(),
        pessoal.as_deref(),
    )
}

/// Como está a sala pessoal desta máquina agora (tarefa 23 da F7).
///
/// O estado chega por [`sala::CANAL_DA_SALA`] a cada mudança; este comando é
/// para a tela que abre depois do anúncio, e não o ouviu.
#[tauri::command]
fn estado_da_sala(session: State<'_, Session>) -> sala::EstadoDaSala {
    session
        .sala
        .lock()
        .map_or(sala::EstadoDaSala::Subindo, |estado| estado.clone())
}

/// ATENDER (tarefa 27 da F7): decide na campainha, e diz à casca se a sessão já
/// está na sala ou por onde entrar nela.
///
/// O resto de «Atender, nesta ordem» (spec de 23/09) é da casca: encerrar os
/// MODs, desmontar só o cliente e entrar pelo `connect` no `alvo` do
/// [`sala::Atendido`], que o `connect` confere pelo código desta máquina. Se
/// entrar falhar duas vezes, a casca chama [`encerrar_a_ligacao`], e ninguém
/// fica sozinho na sala.
///
/// # Errors
///
/// [`sala::FalhaAoAtender`]: a sala não está no ar, ou o toque já parou.
#[tauri::command]
fn atender(
    session: State<'_, Session>,
    quem: String,
) -> Result<sala::Atendido, sala::FalhaAoAtender> {
    let atendido = sala::atender_em(&session, &quem);
    tracing::info!(?atendido, "ATENDER");
    atendido
}

/// SEU CÓDIGO (tarefa 29 da F7): o código desta máquina, o do dono da sala que
/// ela sobe. Ao lado de `impressao_desta_maquina`, e da mesma chave.
///
/// # Errors
///
/// Nunca com motivo: a identidade que não abre é dita no `seele.log`, e a tela
/// mostra o campo vazio.
#[tauri::command]
fn meu_codigo(app: AppHandle) -> Result<String, ()> {
    sala::meu_codigo_em(&config_dir(&app)).ok_or(())
}

/// O que um texto colado em CONECTAR é (tarefa 30 da F7, a 6g): a leitura do
/// núcleo, com «é o seu código?» respondido pela chave desta máquina. Ver
/// [`ligar::EntradaColada`].
#[tauri::command]
fn ler_entrada(app: AppHandle, texto: String) -> ligar::EntradaColada {
    let config = config_dir(&app);
    let meu = seele_ffi::codigo_desta_maquina(&config).ok();
    ligar::ler_entrada_em(&texto, meu.as_ref(), |codigo| {
        ligar::nome_na_agenda(&config, codigo)
    })
}

/// CANCELAR (tarefa 28 da F7): larga a ligação em voo, se houver uma. Devolve
/// se havia o que cancelar. Ver [`ligar::cancelar_em`].
#[tauri::command]
fn cancelar_ligacao(session: State<'_, Session>) -> bool {
    ligar::cancelar_em(&session)
}

/// A agenda (tarefa 29 da F7): os códigos com quem esta máquina já falou.
///
/// # Errors
///
/// [`sala::FalhaNaAgenda::NaoGravou`] quando a agenda não abre.
#[tauri::command]
fn contatos_da_agenda(app: AppHandle) -> Result<Vec<sala::ContatoNaTela>, sala::FalhaNaAgenda> {
    sala::contatos_em(&config_dir(&app))
}

/// Dá um nome a uma pessoa da agenda (tarefa 29 da F7). Vazio tira o nome.
///
/// # Errors
///
/// [`sala::FalhaNaAgenda`]: o código não fecha, ou a agenda não gravou.
#[tauri::command]
fn dar_nome_na_agenda(
    app: AppHandle,
    codigo: String,
    nome: String,
) -> Result<(), sala::FalhaNaAgenda> {
    sala::dar_nome_em(&config_dir(&app), &codigo, &nome)
}

/// Bloqueia ou desbloqueia um código (tarefa 29 da F7), **ao vivo**: quem
/// estava tocando e passou a estar bloqueado para de tocar como recusado.
///
/// # Errors
///
/// [`sala::FalhaNaAgenda`]: o código não fecha, ou a agenda não gravou.
#[tauri::command]
fn bloquear(
    app: AppHandle,
    session: State<'_, Session>,
    codigo: String,
    bloqueado: bool,
) -> Result<(), sala::FalhaNaAgenda> {
    // A campainha mora em `session.hospedagens`: `sala::bloquear_em` leva a ela
    // a lista nova, quando a sala está no ar.
    let bloqueio = sala::bloquear_em(&session, &config_dir(&app), &codigo, bloqueado);
    tracing::info!(?bloqueio, bloqueado, "bloqueio na agenda");
    bloqueio
}

/// RECUSAR (tarefa 27 da F7).
///
/// # Errors
///
/// [`sala::FalhaAoAtender`]: a sala não está no ar, ou o toque já parou.
#[tauri::command]
fn recusar(session: State<'_, Session>, quem: String) -> Result<(), sala::FalhaAoAtender> {
    let recusado = sala::recusar_em(&session, &quem);
    tracing::info!(?recusado, "RECUSAR");
    recusado
}

/// Encerra a ligação da sala pessoal: quem estava dentro ouve «chamada
/// encerrada» (tarefa 27 da F7). Devolve quantos estavam.
///
/// # Errors
///
/// [`sala::FalhaAoAtender::SemSala`] quando a sala não está no ar.
#[tauri::command]
fn encerrar_a_ligacao(session: State<'_, Session>) -> Result<usize, sala::FalhaAoAtender> {
    let encerrada = sala::encerrar_a_ligacao_em(&session);
    tracing::info!(?encerrada, "a ligação da sala pessoal foi encerrada");
    encerrada
}

/// Quem está tocando na sala pessoal agora (tarefa 24 da F7), para a tela que
/// abre com o toque já tocando. Cada toque chega por [`toque::CANAL_DO_TOQUE`]
/// quando começa e quando para.
#[tauri::command]
fn toques_agora(session: State<'_, Session>) -> Vec<toque::NaFaixa> {
    session
        .toques
        .lock()
        .map(|toques| toques.na_faixa())
        .unwrap_or_default()
}

/// «Não receber ligações», no PERFIL: desliga a sala inteira, ou a religa
/// (tarefa 23 da F7). A escolha atravessa o processo.
///
/// Desligar despede a sala: quem estiver nela, ou tocando, ouve «chamada
/// encerrada». Religar sobe a sala de novo, a menos que ela já esteja no ar ou
/// subindo. O que aconteceu chega pelo [`sala::CANAL_DA_SALA`], como na subida
/// do `setup`.
///
/// # Errors
///
/// Nunca: uma escolha que não foi guardada vale até o app fechar, e o
/// `seele.log` diz.
#[tauri::command]
async fn receber_ligacoes(
    app: AppHandle,
    session: State<'_, Session>,
    receber: bool,
) -> Result<(), ()> {
    match preferencias(&app).map(|mut preferencias| preferencias.set_receber_ligacoes(receber)) {
        Some(Ok(())) => {}
        Some(Err(erro)) => tracing::warn!(
            %erro,
            receber,
            "a escolha de receber ligações não foi guardada: vale até o app fechar"
        ),
        None => tracing::warn!(
            receber,
            "as preferências não abriram: a escolha de receber ligações vale até o app fechar"
        ),
    }
    if !receber {
        sala::desligar(&app).await;
        return Ok(());
    }
    let no_ar = session
        .hospedagens
        .lock()
        .is_ok_and(|hospedagens| hospedagens.pessoal.is_some());
    let subindo = session
        .sala
        .lock()
        .is_ok_and(|estado| *estado == sala::EstadoDaSala::Subindo);
    if !no_ar && !subindo {
        tauri::async_runtime::spawn(sala::subir_agora(
            app.clone(),
            sala::Subida::da_producao(&app),
        ));
    }
    Ok(())
}

/// [`onde_estou`] sem estado: o endereço da sessão, a impressão que ela
/// conferiu, e as impressões do guardado e da sala quando estão no ar.
///
/// **Pela chave, e não pelo endereço** (lote X da F5, decisão da orquestradora).
/// `127.0.0.1`, `localhost`, `[::1]`, o endereço da rede local, o público e o
/// nome são a mesma hospedagem, porque a chave é a mesma; um endereço velho
/// que hoje é de outra máquina tem outra chave, e é `Outro`. Comparar pela
/// porta de loopback errava os dois: o próprio link não era o guardado, e o
/// servidor de outra versão que o HOSPEDAR COM ESTA da F6 põe em `127.0.0.1`
/// passava por ele.
///
/// O endereço só diz se há sessão. Com sessão e sem impressão (uma conexão
/// sem aperto de mão), é `Outro`: falha fechado.
fn onde_estou_em(
    endereco: Option<&str>,
    impressao: Option<&str>,
    do_guardado: Option<&str>,
    da_sala: Option<&str>,
) -> OndeEstou {
    if endereco.is_none() {
        return OndeEstou::Nenhum;
    }
    let e_a_chave_de = |hospedada: Option<&str>| {
        impressao
            .zip(hospedada)
            .is_some_and(|(da_sessao, hospedada)| da_sessao.eq_ignore_ascii_case(hospedada))
    };
    if e_a_chave_de(do_guardado) {
        OndeEstou::Guardado
    } else if e_a_chave_de(da_sala) {
        OndeEstou::Pessoal
    } else {
        OndeEstou::Outro
    }
}

/// A porta de um endereço desta máquina, como a pessoa o escreveu:
/// `127.0.0.1`, `localhost` ou `[::1]`, com a porta ou sem ela (aí é a
/// [`PORTA_PADRAO`], como na conexão). `None` para qualquer outro endereço.
///
/// Só o texto, e não uma resolução de nome: a pergunta é sobre o que a pessoa
/// escolheu, não sobre para onde o pacote foi.
fn porta_desta_maquina(endereco: &str) -> Option<u16> {
    let endereco = endereco.trim();
    if let Ok(socket) = endereco.parse::<std::net::SocketAddr>() {
        return socket.ip().is_loopback().then_some(socket.port());
    }
    let (maquina, porta) = match endereco.rsplit_once(':') {
        Some((maquina, porta)) if !maquina.ends_with(':') => (maquina, porta.parse::<u16>().ok()?),
        _ => (endereco, PORTA_PADRAO),
    };
    matches!(maquina, "127.0.0.1" | "localhost" | "[::1]" | "::1").then_some(porta)
}

/// Este endereço é a própria máquina?
///
/// **A pergunta da lista de visitados, e não a de «estou no meu servidor?»**,
/// que é de [`onde_estou`]. `127.0.0.1:8383` é o que o botão HOSPEDAR escreve
/// no campo, e um endereço desta máquina não pertence a uma lista de lugares
/// aonde se volta: ele volta pelo VOLTAR AO MEU SERVIDOR.
fn endereco_desta_maquina(alvo: &str) -> bool {
    porta_desta_maquina(alvo).is_some()
}

/// O que o app precisa saber depois de virar anfitrião.
#[derive(Debug, serde::Serialize)]
struct Anfitriao {
    /// Onde este app se conecta — no próprio computador.
    aqui: String,
    /// O link para mandar aos amigos. ADR 0006.
    convite: String,
    /// Até onde esse link chega: em qual degrau da escada do ADR 0022 parou.
    ///
    /// Nome estável e não frase, igual a [`FalhaAoHospedar`] e pelo mesmo
    /// motivo: a frase mora no `FRASES` do JavaScript. Os nomes são
    /// `PortaNoRoteador`, `FuroDeNat`, `Ipv6Direto`, `RedeLocalOuVpn` e
    /// `SoRedeLocal`, e os cinco já têm frase escrita lá.
    ///
    /// Por que isto cruza a fronteira: um link que só funciona na rede de casa e
    /// um link que funciona pela internet **são o mesmo texto**. Sem este campo
    /// o anfitrião manda o primeiro achando que mandou o segundo, e a descoberta
    /// acontece do outro lado, como "não conecta". O ADR 0022 pede que seja dito
    /// em vez de deixar a pessoa descobrindo sozinha.
    alcance: &'static str,
    /// Por que a porta não abriu no roteador, quando não abriu.
    ///
    /// Esta **é** uma frase pronta, e é a exceção consciente à regra acima: o
    /// texto vem do roteador (`RoteadorRecusou` carrega o que ele respondeu) e
    /// nenhuma lista fechada de frases cobriria o que cada modelo inventa. Vai
    /// para a tela como detalhe secundário, embaixo da frase que o `alcance`
    /// escolheu.
    porta_recusada: Option<String>,
    /// Por que o ponto de encontro não deu, quando ele chegou a ser tentado.
    ///
    /// Degrau 4 do ADR 0022, e o mesmo raciocínio do campo acima: é detalhe
    /// secundário, e é o que explica a quem hospeda por que sobrou um link de
    /// rede local numa casa em que o roteador também não abriu a porta. `None`
    /// quando ninguém pediu ponto de encontro nenhum — desligá-lo é uma escolha,
    /// não uma falha a explicar.
    encontro_recusado: Option<String>,
    /// A regra de firewall desta máquina não cobre **este** executável.
    ///
    /// # Como isto some sem ninguém notar
    ///
    /// A regra é presa ao programa e não à porta — de propósito, porque uma
    /// regra por número abriria a 8383 para qualquer coisa que a escutasse
    /// depois. O preço é que ela vale para **um caminho**, e um caminho pode
    /// deixar de ser o certo sem que nada mude no Windows: quem roda uma cópia
    /// compilada à mão, quem move a pasta, quem apaga a instalação sem passar
    /// pelo desinstalador.
    ///
    /// Aí a regra continua lá, habilitada, verde no painel, **e não permite
    /// nada** — porque o programa que ela nomeia não é o que está escutando.
    ///
    /// Medido numa máquina de verdade em 04/09/2026: o registro dizia
    /// `C:\Program Files\SEELE`, a pasta não existia, e a regra apontava para o
    /// `SEELE.exe` que estaria lá. O anfitrião subia, a escada anunciava furo de
    /// NAT, e quem estava na mesma LAN batia três vezes sem entrar — o candidato
    /// da rede local era barrado pelo firewall, e sobrava um furo de NAT entre
    /// duas máquinas atrás do mesmo roteador, que depende de *hairpinning* e
    /// quase nunca funciona.
    ///
    /// Nada nesta tela dizia isso. O relato foi «teste em LAN não funciona».
    ///
    /// Com o motivo, a frase com o que fazer e se o botão que pede a regra
    /// resolve ([`firewall::NaTela`]): a tela mostra o botão só quando ele
    /// resolve (D-sala-1).
    firewall: Option<firewall::NaTela>,
    /// O roteador abriu a porta, só que não a canônica (8383) — ver "O
    /// quarto" no ADR 0022.
    ///
    /// `None` é o caso comum: a 8383 abriu, ou o degrau 3 nem venceu. `Some`
    /// é o aviso de que o link funciona agora e pode parar de servir depois
    /// do próximo reinício, porque um link guardado numa lista de servidores
    /// depende de a 8383 continuar sendo a porta que o roteador dá.
    porta_nao_canonica: Option<String>,
}

/// Por que não deu para hospedar.
///
/// Enum, e não frase: `specs` põe a fronteira erro→texto no frontend, e uma
/// mensagem escrita aqui seria uma frase que nenhum tradutor alcança. As três
/// pedem coisas diferentes de quem está na frente da tela, e é por isso que são
/// três e não uma.
#[derive(Debug, serde::Serialize)]
enum FalhaAoHospedar {
    /// Já se está hospedando nesta janela.
    JaHospedando,
    /// A porta 8383 está ocupada — quase sempre outro SEELE aberto.
    PortaOcupada,
    /// Qualquer outro motivo para o servidor não subir.
    NaoSubiu,
    /// O nome público digitado não pode virar link, e por quê.
    ///
    /// **Recusado antes de o servidor subir**, e essa ordem é a decisão: subir
    /// e depois recusar deixaria a pessoa hospedando com um link que ela não
    /// pediu, e teria de haver um segundo caminho para desfazer. Conferir
    /// primeiro custa nada e não tem esse segundo caminho.
    ///
    /// O nome da variante e não uma frase, como as três acima: a frase mora no
    /// `FRASES` do JavaScript, e é lá que ela é traduzida.
    NomeRecusado(seele_ffi::uri::NomeRecusado),
    /// Esta versão foi revogada, e a lista assinada diz o defeito e o conserto.
    ///
    /// ADR 0046: uma versão revogada não é hospedável. Conferido antes de
    /// subir, como o nome, e pela mesma razão: recusar depois deixaria um
    /// servidor no ar que não devia estar. Ver `versoes::conferir_esta_versao`.
    EstaVersaoRevogada {
        /// O defeito, como a lista o escreve.
        defeito: String,
        /// A versão que o corrige, quando a lista diz.
        corrigida_em: Option<String>,
    },
    /// A lista de revogação que chegou não abriu, ou é mais velha que a
    /// guardada: o ato é recusado junto, em vez de seguir calado.
    ListaDeRevogacaoRecusada,
}

impl From<versoes::RecusaDestaVersao> for FalhaAoHospedar {
    fn from(recusa: versoes::RecusaDestaVersao) -> Self {
        match recusa {
            versoes::RecusaDestaVersao::EstaVersaoRevogada {
                defeito,
                corrigida_em,
            } => Self::EstaVersaoRevogada {
                defeito,
                corrigida_em,
            },
            versoes::RecusaDestaVersao::ListaDeRevogacaoRecusada => Self::ListaDeRevogacaoRecusada,
        }
    }
}

/// Os servidores que esta máquina guarda, do mais recente ao mais antigo.
///
/// Listar é o que adota o servidor antigo: uma máquina que já hospedou antes
/// desta versão tem um `seele.db` e nenhum registro, e a primeira listagem o
/// põe na lista apontando para onde ele já está. Ver `servidores.rs`.
#[tauri::command]
fn servidores_guardados(app: AppHandle) -> Vec<servidores::Servidor> {
    servidores::listar(&config_dir(&app))
}

/// Apaga somente com a hospedagem parada nesta janela.
#[tauri::command]
fn apagar_servidor(app: AppHandle, session: State<'_, Session>, id: String) -> Result<(), String> {
    let hospedagens = session
        .hospedagens
        .lock()
        .map_err(|_| "Hospedagem indisponível.")?;
    if hospedagens.guardado.is_some() {
        return Err("Encerre a hospedagem antes de apagar um servidor.".into());
    }
    servidores::apagar(&config_dir(&app), &id)
}

/// Guarda um servidor novo e devolve o que foi guardado.
///
/// **Não o levanta.** Criar e hospedar são dois passos porque a tela de
/// preparar ainda pode ser abandonada depois de escrever o nome, e um registro
/// com um servidor que nunca subiu é mais fácil de explicar do que um servidor
/// no ar que ninguém pediu.
/// A versão sai de `versoes::em_uso` e não da tela: é a mesma fonte do `Hello`
/// do cliente e do `seeled --versao`, e uma casca que a escrevesse por conta
/// própria seria a segunda cópia de um número que já tem dono. Um build feito à
/// mão não a carimba, e aí a entrada fica sem versão — que é a verdade.
#[tauri::command]
fn criar_servidor(app: AppHandle, nome: String) -> servidores::Servidor {
    servidores::criar(
        &config_dir(&app),
        &nome,
        versoes::em_uso().unwrap_or_default(),
    )
}

/// Que versões do SEELE estão instaladas nesta máquina.
///
/// ADR 0046. A lista é a do **depósito**, e não a do manifesto: este caminho é
/// offline de propósito — ver `crate::versoes`. Ela nunca diz qual é a mais
/// nova, porque quem sabe isso é o manifesto e ele não passa por aqui.
#[tauri::command]
fn versoes_instaladas(app: AppHandle) -> Vec<versoes::VersaoInstalada> {
    versoes::instaladas(&config_dir(&app))
}

/// Abre uma versão instalada, com os dados dela, e opcionalmente já hospedando.
///
/// **Não fecha esta janela.** Quem lançou não segura o processo filho (ADR 0039
/// quer uma casca só, e o `Lancamento` não espera), e fechar daqui deixaria a
/// pessoa sem nada na tela caso o outro processo não suba. Quem fecha é ela.
///
/// # Errors
///
/// [`versoes::FalhaAoAbrirVersao`], uma variante por motivo.
#[tauri::command]
async fn abrir_versao(
    app: AppHandle,
    // Uma versão exata ou a mais nova de uma linha de servidor — uma das
    // duas ou nenhuma: ver `versoes::pedido_de`. Sem vírgula e sem parêntese
    // nestes comentários: o guarda de argumentos de `tests/frontend.rs` parte
    // a lista de parâmetros nos dois antes de tirar os comentários.
    versao: Option<String>,
    linha: Option<String>,
    hospedar: bool,
    nome_publico: Option<String>,
    link: Option<String>,
) -> Result<(), versoes::FalhaAoAbrirVersao> {
    // O nome público é conferido **aqui também**, e não só no `hospedar`: ele
    // vai virar argumento de linha de comando de outro processo, e um nome com
    // espaço ou com barra chegaria lá partido ou apontando para outro lugar.
    let nome = match nome_publico
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        Some(bruto) => match seele_ffi::uri::conferir_nome_publico(bruto) {
            Ok(nome) => Some(nome),
            // A recusa do nome não é a recusa da versão, e misturar as duas
            // daria à tela uma frase sobre launcher para um erro de digitação.
            // Abrir sem o nome seria pior: a pessoa hospedaria com o link que
            // ela não pediu. Então não abre, e o motivo é o do nome.
            Err(_) => return Err(versoes::FalhaAoAbrirVersao::IdentificadorInvalido),
        },
        None => None,
    };
    // O link é conferido antes de virar argumento de outro processo: um texto
    // que não é um `seele://` não deve ser repassado como se fosse.
    let link = match link.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        Some(bruto) => match seele_ffi::uri::analisar(bruto) {
            Ok(_) => Some(bruto.to_owned()),
            Err(_) => return Err(versoes::FalhaAoAbrirVersao::IdentificadorInvalido),
        },
        None => None,
    };
    // Hospedar e entrar são os atos do ADR 0046, e conferem a revogação;
    // abrir para ler não confere, e não fala com a rede.
    let pedido = versoes::pedido_de(versao.as_deref(), linha.as_deref())?;
    let ato = if hospedar {
        seele_lancador::Ato::Hospedar
    } else if link.is_some() {
        seele_lancador::Ato::Entrar
    } else {
        seele_lancador::Ato::Abrir
    };
    let intencao = versoes::Intencao {
        hospedar,
        nome_publico: nome,
        link,
    };
    versoes::abrir(
        &config_dir(&app),
        versoes::atualizador_do_app(&app).as_ref(),
        &pedido,
        ato,
        &intencao,
    )
    .await
}

/// Confere se **esta** versão pode entrar num servidor: a lista de revogação,
/// num ato. A tela chama logo antes do `connect`.
///
/// À parte, e não dentro do `connect`, porque o erro dele é o da FFI
/// (`ConnectFailure`) e não tem onde dizer «esta versão foi revogada». Há uma
/// chamada a `connect` só, em `tela-boot.js`, e o guarda
/// `entrar_confere_esta_versao_antes_de_conectar` prende a ordem.
///
/// # Errors
///
/// [`versoes::RecusaDestaVersao`].
#[tauri::command]
async fn conferir_esta_versao(app: AppHandle) -> Result<(), versoes::RecusaDestaVersao> {
    versoes::conferir_esta_versao(
        &config_dir(&app),
        versoes::atualizador_do_app(&app).as_ref(),
        seele_lancador::Ato::Entrar,
    )
    .await
}

/// Quanto o depósito de versões ocupa: cada app guardado e as conversas de
/// cada linha. Ver `versoes::ocupacao`.
#[tauri::command]
fn ocupacao_do_deposito(app: AppHandle) -> versoes::Ocupacao {
    versoes::ocupacao(&config_dir(&app))
}

/// Apaga um app guardado, sem tocar nas conversas da linha dele.
///
/// # Errors
///
/// [`versoes::FalhaAoRemoverVersao`].
#[tauri::command]
fn remover_versao(app: AppHandle, versao: String) -> Result<u64, versoes::FalhaAoRemoverVersao> {
    versoes::remover_versao(&config_dir(&app), &versao)
}

/// As linhas de servidor em que se pode hospedar, a desta janela primeiro.
///
/// Sem rede: lê o catálogo guardado. Ver `versoes::linhas_para_hospedar`.
#[tauri::command]
fn linhas_para_hospedar(app: AppHandle) -> Vec<versoes::LinhaParaHospedar> {
    versoes::linhas_para_hospedar(
        &config_dir(&app),
        versoes::atualizador_do_app(&app)
            .as_ref()
            .map(|atualizador| &atualizador.chave),
    )
}

/// O que a primeira abertura desta versão fez com os dados da 0.15 e com a
/// linha legada, para a tela dizer uma vez. Ver `versoes::primeira_abertura`.
#[tauri::command]
fn estado_da_primeira_abertura(
    estado: State<'_, versoes::EstadoDaPrimeiraAbertura>,
) -> Option<versoes::PrimeiraAbertura> {
    estado.0.lock().ok().and_then(|slot| slot.clone())
}

/// Sobe um servidor dentro do app e devolve o link do convite.
///
/// Este comando é o item de UX que faltava: sem ele, hospedar exige abrir um
/// terminal, e num produto cujo argumento é "hospede você mesmo" isso exclui
/// justamente quem só quer clicar. O mesmo caminho do `connection --hospedar`, o
/// mesmo módulo, o mesmo server.
///
/// Não conecta. Quem conecta é o `connect` de sempre, com o endereço que este
/// comando devolve — um caminho só para entrar num servidor, hospedado aqui ou do
/// outro lado do mundo.
#[tauri::command]
async fn hospedar(
    app: AppHandle,
    session: State<'_, Session>,
    // Vazio é a ausência de escolha e é o padrão: o link numérico de sempre.
    nome_publico: Option<String>,
    // Qual servidor guardado levantar. `None` é o banco de sempre — o que toda
    // máquina que já hospedou antes desta versão tem.
    servidor: Option<String>,
) -> Result<Anfitriao, FalhaAoHospedar> {
    // Conferido **antes** de subir o servidor. Ver `FalhaAoHospedar::NomeRecusado`.
    //
    // Um campo em branco não é um nome recusado: é alguém que não quis um nome.
    // Só o que tem algum caractere é conferido, e aí a recusa é dita na hora —
    // um link torto não falha aqui, falha dias depois na tela de outra pessoa,
    // como «não conecta», com quem hospedou já tendo ido dormir.
    let nome_publico = match nome_publico
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        Some(bruto) => Some(
            seele_ffi::uri::conferir_nome_publico(bruto).map_err(FalhaAoHospedar::NomeRecusado)?,
        ),
        None => None,
    };
    {
        let hospedagens = session
            .hospedagens
            .lock()
            .map_err(|_| FalhaAoHospedar::NaoSubiu)?;
        if hospedagens.guardado.is_some() {
            return Err(FalhaAoHospedar::JaHospedando);
        }
    }

    // ADR 0046: a lista de revogação é conferida nos atos, e esta versão
    // também é uma versão. Sem rede vale a guardada; sem nenhuma, passa.
    versoes::conferir_esta_versao(
        &config_dir(&app),
        versoes::atualizador_do_app(&app).as_ref(),
        seele_lancador::Ato::Hospedar,
    )
    .await?;

    // **Qual banco, e por que isso passa pelo registro.** Até aqui era sempre
    // `<config>/seele.db`, um por máquina. `servidores::banco` devolve esse
    // mesmo caminho quando ninguém escolheu nada — e é o que faz hospedar
    // continuar funcionando numa máquina sem registro nenhum.
    let config = config_dir(&app);
    let id_do_servidor = servidor.as_deref();
    let banco = match id_do_servidor {
        Some(id) => {
            servidores::marcar_uso(&config, id);
            servidores::banco(&config, Some(id))
        }
        // O legado, e ele não é um caso de canto: é toda máquina que já
        // hospedou antes desta versão existir.
        None => seele_server::persistence::banco_do_cliente(std::path::Path::new(&config)),
    };
    // Guardado **antes** de subir: quem perguntar «qual servidor esta janela
    // hospeda?» entre o início e o fim recebe a resposta certa, e não «nenhum».
    if let Ok(mut qual) = session.servidor_hospedado.lock() {
        *qual = id_do_servidor.map(str::to_owned);
    }
    // Quem hospeda, pela chave desta máquina: é a impressão que a portaria vai
    // admitir antes de a porta abrir. Falhar aqui é grave o bastante para não
    // hospedar — um servidor no ar em que o dono não entra é uma janela travada
    // com uma frase pedindo que alguém decida.
    let minha = seele_ffi::impressao_desta_maquina(&config_dir(&app))
        .map_err(|_| FalhaAoHospedar::NaoSubiu)?;
    // **Com o ponto de encontro do projeto**, pedido aqui e só aqui: sem
    // `$SEELE_ENCONTRO`, o `iniciar` curto não convoca ponto nenhum, que é o
    // padrão seguro dos testes. Quem hospeda pelo app é quem está atrás do
    // roteador de casa, e é para essa pessoa que o degrau 4 existe. O guarda é
    // `o_ponto_de_encontro_de_quem_hospeda`, logo abaixo desta função.
    let server = seele_server::hospedagem::Hospedagem::iniciar_com_o_ponto_padrao(
        PORTA_PADRAO,
        seele_server::persistence::Location::File(banco),
        "Casa",
        // ADR 0045 para o pacote, e o plano de isolamento de 18/09 para os
        // arquivos: o código é compartilhado porque é imutável e conferido por
        // hash; o que o MOD escreve é **desta instância**, e não da máquina.
        Some(servidores::raizes_dos_mods(&config, id_do_servidor)),
        // **Antes de o servidor aceitar a primeira conexão** — D-seg-1, a
        // janela do Comandante. Isto rodava depois de `iniciar` voltar, com o
        // laço já no ar: num banco novo a portaria nasce desligada, e quem
        // chegasse nesse intervalo virava a primeira conta, e a primeira conta
        // vira a Comandante. Ver `crates/seele-server/tests/janela_do_comandante.rs`.
        move |persistence| {
            // A porta se fecha no mesmo gesto de hospedar, antes do primeiro
            // pacote — ADR 0030. O `seeled` continua subindo aberto, porque
            // quem digita `seeled` aceitou cerimônia; quem apertou um botão
            // não, e para ele o padrão é perguntar.
            //
            // Semear, e não ligar: isto roda toda vez que a janela sobe um
            // servidor, e um interruptor que se rearma sozinho é um
            // interruptor quebrado.
            if let Err(erro) = seele_server::portaria::semear_ligada(persistence) {
                // Não impede de hospedar. Um servidor no ar com a portaria
                // desligada é o comportamento de antes deste ADR, e a tela diz
                // em que estado a porta está.
                tracing::warn!(%erro, "não consegui semear a portaria");
            }
            // E quem hospeda entra na própria casa. Sem isto a portaria tranca
            // o dono para fora: o app conecta no servidor que acabou de subir,
            // o porteiro trata quem hospeda como desconhecido, e o pedido fica
            // esperando a decisão de alguém que não consegue entrar para
            // decidir — foi o que aconteceu numa máquina de verdade.
            seele_server::portaria::admitir_o_dono(persistence, &minha)
        },
    )
    .await
    .map_err(|erro| classificar(&erro))?;

    let alcance = server.alcance();
    let firewall = firewall_deste_executavel();
    let anfitriao = Anfitriao {
        aqui: format!("127.0.0.1:{PORTA_PADRAO}"),
        convite: server.convite_com_nome(nome_publico.as_deref()),
        alcance: alcance.map_or("SoRedeLocal", |alcance| alcance.degrau().nome()),
        porta_recusada: alcance.and_then(|alcance| alcance.porta_recusada().map(str::to_owned)),
        encontro_recusado: alcance
            .and_then(|alcance| alcance.encontro_recusado().map(str::to_owned)),
        firewall,
        porta_nao_canonica: alcance
            .and_then(|alcance| alcance.porta_nao_canonica().map(str::to_owned)),
    };

    // Qual servidor guardado é, anotado junto do servidor de pé: uma janela que
    // não hospeda não pode dizer que hospeda um servidor guardado.
    session
        .hospedagens
        .lock()
        .map_err(|_| FalhaAoHospedar::NaoSubiu)?
        .guardado = Some(Hospedado {
        hospedagem: server,
        servidor,
    });

    Ok(anfitriao)
}

/// Toda hospedagem deste app sobe com o ponto de encontro do projeto.
///
/// Desde a decisão (d) do lote G-servidor da F5, `Hospedagem::iniciar` sem
/// `$SEELE_ENCONTRO` não convoca ponto nenhum, que é o padrão seguro para os
/// testes. A produção pede o ponto padrão de forma explícita, e estes são os
/// guardas de que ela pede: sem ele, quem hospeda atrás de CGNAT perderia o
/// degrau 4 em silêncio, e o link sairia sem o bilhete que faz «manda o link e
/// funciona» virar verdade.
///
/// O arquivo inteiro, e não só o `hospedar`: a sala pessoal (tarefa 4f da F7)
/// e o G-casca do Plano 2 vão abrir pontos de entrada novos aqui, e o que
/// esquecer o padrão reprova sem que alguém precise lembrar de pôr um guarda
/// nele. **Menos os módulos de teste**, que sobem pelo `iniciar` curto de
/// propósito (o padrão seguro, acima): o MOD do guardado
/// (`o_mod_do_guardado_continua_respondendo`, tarefa 20 da F7) precisa de uma
/// hospedagem com MODs, e com o ponto padrão ela convocaria o ponto do projeto.
#[cfg(test)]
mod o_ponto_de_encontro_de_quem_hospeda {
    /// As duas formas, partidas para não casarem com este texto.
    const COM_O_PADRAO: &str = concat!("Hospedagem::", "iniciar_com_o_ponto_padrao(");
    const SEM_O_PADRAO: &str = concat!("Hospedagem::", "iniciar(");

    /// Este arquivo sem comentários e sem os módulos `#[cfg(test)]`. Cada um é
    /// de primeiro nível e fecha numa linha `}` sozinha, onde a produção
    /// recomeça. Só os módulos: um `use` sob `#[cfg(test)]` não tem `}`, e
    /// cortar até a próxima levaria junto a produção que vem depois dele.
    fn producao() -> String {
        producao_de(&super::fonte_sem_comentarios())
    }

    /// [`producao`] sobre um texto qualquer, para o recorte ter prova própria.
    ///
    /// **E ele se confere sozinho** (sugestão da F5, ao aceitar o recorte): o
    /// que se corta é um módulo, e dentro de um módulo toda linha é recuada,
    /// menos a do nome e a do `}` final. Um `#[cfg(test)] mod x;`, sem corpo,
    /// levaria o corte até a `}` da próxima função de produção, e é essa linha
    /// na coluna zero que acusa.
    fn producao_de(fonte: &str) -> String {
        let mut pedacos = fonte.split(concat!("\n#[cfg(test)]\n", "mod "));
        let mut producao = pedacos.next().unwrap_or_default().to_owned();
        for pedaco in pedacos {
            if let Some(fim) = pedaco.find("\n}\n") {
                let soltas: Vec<&str> = pedaco[..fim]
                    .lines()
                    .skip(1)
                    .filter(|linha| linha.starts_with(|c: char| !c.is_whitespace()))
                    .collect();
                assert!(
                    soltas.is_empty(),
                    "o recorte de um módulo de teste levou linhas da coluna zero, que são de \
                     produção: {soltas:?}"
                );
                producao.push_str(&pedaco[fim + 2..]);
            }
        }
        producao
    }

    #[test]
    #[should_panic(expected = "linhas da coluna zero")]
    fn o_recorte_acusa_um_modulo_de_teste_sem_corpo() {
        let fonte = concat!(
            "fn antes() {}\n",
            "#[cfg(test)]\n",
            "mod fora;\n",
            "\n",
            "async fn hospedar_de_producao() {\n",
            "    let _ = 1;\n",
            "}\n",
            "fn depois() {}\n",
        );
        let _ = producao_de(fonte);
    }

    #[test]
    fn nenhuma_hospedagem_deste_arquivo_sobe_sem_o_ponto_padrao() {
        let fonte = producao();
        assert!(
            fonte.contains("async fn hospedar(") && !fonte.contains("mod o_ponto_de_encontro"),
            "o recorte da produção deste arquivo quebrou"
        );
        let sem: Vec<&str> = fonte
            .lines()
            .filter(|linha| linha.contains(SEM_O_PADRAO))
            .map(str::trim)
            .collect();
        assert!(
            sem.is_empty(),
            "este arquivo sobe hospedagem com `{SEM_O_PADRAO}`, que sem `$SEELE_ENCONTRO` não \
             convoca ponto de encontro nenhum: quem hospeda por esse caminho perde o degrau 4 em \
             silêncio. Use `{COM_O_PADRAO}`. Linhas: {sem:?}"
        );
        assert!(
            fonte.contains(COM_O_PADRAO),
            "nenhuma hospedagem deste arquivo usa `{COM_O_PADRAO}`: o app hospeda sem o ponto de \
             encontro do projeto"
        );
    }

    #[test]
    fn hospedar_sobe_com_o_ponto_de_encontro_padrao() {
        let fonte = super::fonte_sem_comentarios();
        let corpo = super::corpo_de(&fonte, "async fn hospedar(");
        assert!(
            corpo.contains(COM_O_PADRAO) && !corpo.contains(SEM_O_PADRAO),
            "o `hospedar` não sobe o servidor com `{COM_O_PADRAO}`: sem ele, e sem \
             `$SEELE_ENCONTRO`, a hospedagem não convoca ponto de encontro nenhum, e quem hospeda \
             atrás de CGNAT perde o degrau 4 em silêncio"
        );
    }
}

/// Desfaz `%XX` num caminho de URL. O resto passa inteiro.
///
/// Escrito aqui, e não trazido numa dependência: são quinze linhas, e o
/// `Cargo.toml` deste app é curto de propósito — cada caixa a mais é mais uma
/// coisa que um invasor alcança sem tocar em nada nosso.
///
/// Um `%` solto ou seguido de coisa que não é hexadecimal fica como está. É o
/// comportamento certo para o que vem depois: quem decide se o caminho serve é
/// a conferência de hash e o `serve`, e os dois recusam o que não conhecem.
/// O endereço que pode sair desta janela, ou o motivo de não poder.
///
/// **O texto vem de outra pessoa.** Uma mensagem de chat é escrita por quem
/// está do outro lado, e transformá-la em algo clicável é dar a essa pessoa um
/// botão na máquina de quem lê. O que este filtro recusa não é estética:
///
///   - `file://` abriria o Finder ou o Explorador num caminho escolhido por
///     terceiro;
///   - `javascript:` e `data:` executam na origem de quem clica;
///   - qualquer esquema que o sistema saiba abrir — `smb:`, `vscode:`, `ms-*:`
///     — é um programa que alguém de fora escolhe iniciar aqui.
///
/// Sobram `http` e `https`, que é o que se quer dizer com «abrir no navegador».
/// Lista do que **pode**, e não do que não pode: uma lista de proibidos erra
/// no dia em que o sistema aprende um esquema novo, e erra para o lado de
/// deixar passar.
///
/// Espaço e caractere de controle também saem. Eles não aparecem num endereço
/// legítimo, e são por onde um argumento vira dois.
fn endereco_que_pode_sair(bruto: &str) -> Result<&str, String> {
    let url = bruto.trim();
    if url.len() > 2048 {
        return Err("endereço longo demais".to_owned());
    }
    let minusculo = url.to_ascii_lowercase();
    if !minusculo.starts_with("http://") && !minusculo.starts_with("https://") {
        return Err(
            "só abro endereços `http` e `https`: qualquer outro esquema é um              programa que alguém de fora escolheu iniciar nesta máquina"
                .to_owned(),
        );
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("endereço com espaço ou caractere de controle".to_owned());
    }
    // Um `http://` sozinho não é endereço nenhum.
    if url.len() <= "https://".len() {
        return Err("endereço vazio".to_owned());
    }
    Ok(url)
}

/// Abre um endereço no navegador do sistema.
///
/// Fora da janela de propósito: um link de chat que navegasse **esta** janela
/// trocaria o SEELE por um site, sem botão de voltar — a casca não tem barra de
/// endereço, e a pessoa ficaria com o produto substituído por uma página que
/// outra pessoa escolheu.
///
/// Sem `tauri-plugin-opener` e sem `tauri-plugin-shell`: o `Cargo.toml` deste
/// app diz «nada além disso» sobre dependências, e isto cabe em três linhas por
/// sistema. O que não cabe em três linhas é a conferência acima, e ela é a
/// parte que importa.
#[tauri::command]
fn abrir_no_navegador(url: String) -> Result<(), String> {
    let alvo = endereco_que_pode_sair(&url)?;

    // Nenhum deles passa por shell: o argumento vai direto ao programa, então
    // `&`, `|` e aspas dentro do endereço são texto e não sintaxe.
    #[cfg(target_os = "macos")]
    let mut comando = {
        let mut c = std::process::Command::new("open");
        c.arg(alvo);
        c
    };
    // `rundll32` e não `cmd /c start`: o `cmd` interpreta a linha antes de
    // qualquer programa vê-la, e um `&` no endereço vira um segundo comando.
    #[cfg(target_os = "windows")]
    let mut comando = {
        let mut c = std::process::Command::new("rundll32.exe");
        c.args(["url.dll,FileProtocolHandler", alvo]);
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut comando = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(alvo);
        c
    };

    comando
        .spawn()
        .map(|_| ())
        .map_err(|erro| format!("não consegui abrir o navegador: {erro}"))
}

/// Se o firewall do Windows **não** deixa chegar conexão a este SEELE, e por
/// quê: a frase que a linha da sala e a tela de hospedar mostram.
///
/// Quem lê a regra é [`firewall`], num lugar só (D-sala-1, a conferência de
/// 15/10): a regra ligada, de entrada, UDP, do caminho inteiro deste programa,
/// no perfil da rede ativa, com o bloqueio vencendo a liberação. Era aqui, e
/// perguntava só se existia uma regra chamada `SEELE` — um bloqueio com esse
/// nome passava por coberto.
///
/// A resposta vai ao `seele.log` no Windows, inclusive quando é «não sei»:
/// `None` na tela, mas o motivo no registro. O botão que pede a regra só
/// aparece quando este é o SEELE instalado, com o instalador ao lado
/// ([`pasta_instalada`]), que é quem o botão chama.
fn firewall_deste_executavel() -> Option<firewall::NaTela> {
    let entrada = firewall::ler_deste_programa();
    #[cfg(windows)]
    tracing::info!(?entrada, "o firewall do Windows, para este SEELE");
    let eu = std::env::current_exe().ok()?;
    #[cfg(windows)]
    let instalado = pasta_instalada()
        .filter(|pasta| pasta.join("desinstalar.exe").is_file())
        .map(|pasta| pasta.join("SEELE.exe"));
    #[cfg(not(windows))]
    let instalado: Option<std::path::PathBuf> = None;
    firewall::na_tela(&entrada, &eu, instalado.as_deref())
}

/// A pasta da instalação, como o instalador a registrou (`InstallLocation`).
#[cfg(windows)]
fn pasta_instalada() -> Option<std::path::PathBuf> {
    use std::os::windows::process::CommandExt;

    std::process::Command::new("reg")
        .args(["query", CHAVE_DA_INSTALACAO, "/v", "InstallLocation"])
        .creation_flags(SEM_CONSOLE)
        .output()
        .ok()
        .filter(|saida| saida.status.success())
        .and_then(|saida| {
            String::from_utf8_lossy(&saida.stdout)
                .lines()
                .find_map(|linha| linha.split("REG_SZ").nth(1).map(|r| r.trim().to_owned()))
        })
        .map(std::path::PathBuf::from)
}

/// Onde o instalador grava o que se sabe sobre a instalação.
#[cfg(windows)]
const CHAVE_DA_INSTALACAO: &str = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\SEELE";

/// Cria a regra de firewall do SEELE, pedindo elevação ao Windows.
///
/// # Por que o app não faz isso sozinho
///
/// A regra exige administrador, e este processo roda como a pessoa — de
/// propósito: um app elevado gravaria identidade, pinos e preferências na pasta
/// do administrador, onde a abertura seguinte não os acha.
///
/// Quem exige administrador é o instalador, cujo manifesto pede elevação, e a
/// cópia que a instalação deixa na pasta é ele mesmo. Então o app o chama, o
/// Windows pergunta uma vez, e a regra nasce.
///
/// # Por que também aqui, e não só no instalador
///
/// O instalador cria a regra por padrão: a caixa «Liberar o SEELE no firewall
/// do Windows» nasce marcada, e a atualização silenciosa guarda a escolha. Este
/// caminho é para o que sobra dele — quem a desmarcou, a regra que alguém
/// apagou, a que uma política não deixou nascer. A linha da sala e a tela de
/// hospedar mostram o botão quando o leitor ([`firewall`]) diz que de fora não
/// se chega.
///
/// Ele libera o SEELE **instalado**, que é o caminho que o instalador conhece:
/// uma cópia portátil não ganha nada com ele, e um bloqueio criado pelo aviso
/// do Windows também não sai por aqui.
///
/// # Errors
///
/// Quando não há instalação registrada, quando o desinstalador não está lá, ou
/// quando o Windows recusa lançar. Uma recusa no UAC não é erro: a pessoa disse
/// não, e o app segue sem a regra — a tela continua avisando.
#[tauri::command]
fn abrir_a_porta_no_firewall() -> Result<(), String> {
    #[cfg(not(windows))]
    {
        Err("a regra de firewall do Windows só existe no Windows.".to_owned())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let instalado = pasta_instalada().ok_or_else(|| {
            "não achei a instalação do SEELE no registro desta máquina.".to_owned()
        })?;

        let ferramenta = std::path::Path::new(&instalado).join("desinstalar.exe");
        if !ferramenta.is_file() {
            return Err(format!(
                "não achei {} — reinstale o SEELE para recuperá-lo.",
                ferramenta.display()
            ));
        }

        // **`Start-Process -Verb RunAs`, e não `ShellExecuteW`.**
        //
        // O que faz o Windows perguntar é o verbo `runas`; sem ele, lançar um
        // executável que exige administrador a partir de um processo que não é
        // falha com «elevação necessária» e nada acontece.
        //
        // A forma direta seria o `ShellExecuteW`, e ela não cabe aqui: a folha de
        // lints deste workspace proíbe `unsafe`, e `seele-app` não tem folha
        // própria — abrir uma exceção para uma chamada seria pagar caro por
        // conveniência. O PowerShell existe em todo Windows desde o 7 e faz a
        // mesma coisa por um caminho que não precisa de exceção nenhuma.
        //
        // **E ele diz quando a pessoa recusa.** `Start-Process` lança um erro
        // quando o UAC é negado, e o PowerShell sai com código diferente de zero
        // — que é exatamente o retorno que o atualizador do Tauri descarta no
        // `ShellExecuteW` dele, e foi assim que uma atualização que não
        // acontecia virou um app que fechava sozinho. Aqui ele é lido.
        let saida = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "Start-Process -FilePath '{}' -ArgumentList '--abrir-a-porta' \
                     -Verb RunAs -Wait",
                    ferramenta.display()
                ),
            ])
            .creation_flags(SEM_CONSOLE)
            .output()
            .map_err(|erro| format!("não consegui pedir a permissão ao Windows: {erro}"))?;

        if !saida.status.success() {
            return Err("o Windows não criou a regra. Se você recusou a permissão, \
                        tente de novo e responda Sim."
                .to_owned());
        }
        Ok(())
    }
}

/// `CREATE_NO_WINDOW`: o processo filho não ganha console.
///
/// **Sem isto, uma janela preta pisca por cima do app.** Três chamadas ao
/// sistema saem daqui — `netsh` uma vez e `reg` duas —, e elas rodam **toda vez
/// que alguém aperta HOSPEDAR AQUI**, antes de qualquer decisão. Relatado
/// assim: «agora toda vez que vai abrir o host aparece o terminal, isso passa
/// insegurança pro usuário».
///
/// E passa mesmo: um terminal que aparece sozinho num programa de conversa é
/// indistinguível de um programa fazendo o que não devia. O instalador já tinha
/// esta constante pela mesma razão, e eu não a copiei ao trazer as chamadas para
/// cá.
#[cfg(windows)]
const SEM_CONSOLE: u32 = 0x0800_0000;

/// A porta em que um servidor escuta por padrão.
const PORTA_PADRAO: u16 = 8383;

/// Separa "já tem um SEELE aberto" de todo o resto.
///
/// A distinção não é cosmética: porta ocupada tem conserto óbvio — fechar a
/// outra janela — e todo o resto não tem. Dizer "não subiu" nos dois casos
/// esconde a única falha que a pessoa consegue resolver sozinha.
fn classificar(erro: &anyhow::Error) -> FalhaAoHospedar {
    for causa in erro.chain() {
        if let Some(io) = causa.downcast_ref::<std::io::Error>() {
            if io.kind() == std::io::ErrorKind::AddrInUse {
                return FalhaAoHospedar::PortaOcupada;
            }
        }
    }
    FalhaAoHospedar::NaoSubiu
}

/// Termina a sessão **desta janela**, e só ela (tarefa 19 da F7, Plano 2).
///
/// **As hospedagens ficam de pé.** O servidor guardado continua no ar com quem
/// estiver nele, e a sala pessoal continua recebendo ligações. Quem quer
/// derrubar o guardado diz isso com outro verbo, [`parar_de_hospedar`], e a sala
/// desce pelo «receber ligações» ([`receber_ligacoes`]).
///
/// **A03 da auditoria de 17/09, resolvido de vez.** Este verbo derrubava o
/// servidor hospedado junto, e o RECONECTAR, que passava por ele, tirava do ar o
/// endereço em que ia entrar em seguida. A cura de então foi um segundo verbo
/// (`desmontar_a_sessao`), que não derrubava. Com sair deixando de derrubar, os
/// dois eram o mesmo, e um saiu: dois verbos iguais são dois lugares para
/// divergirem.
///
/// Genérico no runtime, como o [`connect`]: o teste do MOD do guardado
/// (`o_mod_do_guardado_continua_respondendo`) sai por ele, pelo runtime falso.
///
/// # Errors
///
/// Nunca: o tipo existe para a ponte, e desmontar o que já está desmontado é o
/// desfecho normal de quem apertou duas vezes.
#[tauri::command]
async fn disconnect<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session: State<'_, Session>,
) -> Result<(), ()> {
    desmontar_o_cliente(&app, &session);
    Ok(())
}

/// PARAR DE HOSPEDAR (tarefa 19 da F7): derruba o servidor guardado. Quem estava
/// dentro lê «O SERVIDOR ESTÁ ENCERRANDO», e não uma queda de transporte
/// (`Hospedagem::encerrar`, que avisa).
///
/// **Só o guardado.** A sala pessoal desce pelo «receber ligações»
/// ([`receber_ligacoes`]), que grava a escolha: um segundo caminho para ela que
/// não gravasse nada a faria voltar na abertura seguinte.
///
/// Devolve se havia servidor no ar.
///
/// # Errors
///
/// Nunca: parar o que não está no ar é o desfecho de quem apertou duas vezes.
#[tauri::command]
async fn parar_de_hospedar(session: State<'_, Session>) -> Result<bool, ()> {
    let parou = parar_de_hospedar_em(&session).await;
    tracing::info!(parou, "PARAR DE HOSPEDAR");
    Ok(parou)
}

/// [`parar_de_hospedar`] sobre a sessão. `encerrar`, e não o `despedir` da
/// saída do app: o app continua aberto, e hospedar de novo em seguida tem de
/// achar a porta livre.
async fn parar_de_hospedar_em(session: &Session) -> bool {
    let guardado = session
        .hospedagens
        .lock()
        .ok()
        .and_then(|mut hospedagens| hospedagens.guardado.take());
    let Some(guardado) = guardado else {
        return false;
    };
    guardado.hospedagem.encerrar().await;
    true
}

/// O servidor guardado no ar, do jeito que o VOLTAR AO MEU SERVIDOR precisa
/// dele (tarefa 20 da F7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct MeuServidor {
    /// Onde entrar para voltar: esta máquina, na porta em que ele atende.
    alvo: String,
    /// O nome que a lista de servidores mostra, quando a pessoa deu um.
    ///
    /// O nome de verdade mora no banco do servidor; o da lista é o que o
    /// `renomear_server` mantém em dia, e é o que a pessoa escolheu de olho.
    nome: Option<String>,
}

/// O servidor guardado no ar, para o VOLTAR AO MEU SERVIDOR e o PARAR DE
/// HOSPEDAR da entrada (tarefa 20 da F7). `None` quando não há.
#[tauri::command]
fn meu_servidor(app: AppHandle, session: State<'_, Session>) -> Option<MeuServidor> {
    let hospedagens = session.hospedagens.lock().ok()?;
    meu_servidor_em(&hospedagens, &config_dir(&app))
}

/// [`meu_servidor`] sobre as hospedagens e a pasta de dados.
fn meu_servidor_em(hospedagens: &Hospedagens, config: &str) -> Option<MeuServidor> {
    let (porta, id) = {
        let guardado = hospedagens.guardado.as_ref()?;
        (
            guardado.hospedagem.endereco().port(),
            guardado.servidor.clone(),
        )
    };
    let nome = id.and_then(|id| {
        servidores::listar(config)
            .into_iter()
            .find(|servidor| servidor.id == id)
            .map(|servidor| servidor.nome)
            .filter(|nome| !nome.trim().is_empty())
    });
    Some(MeuServidor {
        alvo: format!("127.0.0.1:{porta}"),
        nome,
    })
}

/// O que a saída faz: desmontar a sessão de cliente.
///
/// **Revoga antes de desmontar** — §5 do contrato de API própria, passo 1: «a
/// partir desse ponto, nenhum novo efeito daquela instância é admitido, mesmo
/// que mensagens já estejam na fila».
///
/// A ordem não é estética. Entre tirar a conexão do slot e ela de fato morrer
/// há tarefas terminando, eventos em voo e comandos da janela a caminho; cada
/// um deles é uma janela por onde um trabalho da sessão que acabou entraria na
/// seguinte. Revogar primeiro fecha todas de uma vez, e o custo é uma linha.
///
/// Idempotente: desmontar o que já está desmontado gasta um número de geração e
/// não faz mais nada, que é o desfecho certo para quem apertou duas vezes.
fn desmontar_o_cliente<R: tauri::Runtime>(app: &tauri::AppHandle<R>, session: &State<'_, Session>) {
    let morta = session.revogar();
    tracing::debug!(geracao = morta, "sessão revogada antes de desmontar");

    let connection = session
        .connection
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());

    // **A medida da escada sobrevive à sessão**, e este é o único ponto que tem
    // as duas coisas na mão: a conexão que mediu e o endereço que a nomeia.
    //
    // Zero não apaga o que estava guardado — `anotar_caminho_medido` recusa —,
    // porque a sonda só mede enquanto a tela transmite: uma sessão em que
    // ninguém compartilhou nada devolve zero, e sobrescrever com ele desfaria
    // justamente o que a memória serve para evitar.
    if let Some(viva) = connection.as_ref() {
        let medido = viva.caminho_medido();
        // **E a máquina também aprende.** A linha abaixo guarda por servidor, e
        // um servidor hospedado aqui não entra naquela lista de propósito —
        // então sem isto a medida de quem hospeda se perdia inteira. A subida é
        // da máquina; o caminho até cada servidor é que difere.
        if let Some(mut p) = preferencias(app) {
            if let Err(erro) = p.anotar_caminho_da_maquina(medido) {
                tracing::debug!(%erro, "não guardei a subida desta máquina");
            }
        }
        let alvo = session.alvo.lock().ok().and_then(|a| a.clone());
        if let (Some(alvo), true) = (alvo, medido != 0) {
            if let Ok(mut lista) =
                seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(app))
            {
                if let Err(erro) = lista.anotar_caminho_medido(&alvo, medido) {
                    tracing::debug!(%erro, "não guardei o caminho medido deste servidor");
                }
            }
        }
    }
    // Dropping the handle is what ends the session; taking it out of the slot
    // is what makes the next `connect` allowed.
    drop(connection);

    // O convite morre com a sessão que ele abriu. Enquanto nada era conferido
    // isto era inerte; deixou de ser no momento em que `expected_fingerprint`
    // passou a sair daqui — quem sai, digita outro endereço e entra de novo
    // levaria a impressão prometida por um link anterior para um servidor que
    // nunca a prometeu, e a recusa apareceria sem nada na tela que a explique.
    if let Ok(mut slot) = session.convite.lock() {
        *slot = None;
    }
    // Onde a sessão estava deixa de existir com ela: um sim gravado depois
    // disso iria para o endereço da conversa anterior.
    if let Ok(mut onde) = session.endereco_da_sessao.lock() {
        *onde = None;
    }
}

/// **Um MOD do guardado continua respondendo enquanto o dono visita outro
/// servidor** (tarefa 20 da F7, Plano 2).
///
/// Sair desmonta só o cliente (tarefa 19), e o servidor guardado continua no ar
/// com quem estiver nele. Para quem ficou, isso inclui os MODs: o pedido dele
/// chega à metade de servidor e volta com resposta, com o dono noutro servidor.
/// E os comandos de MOD do app continuam achando o guardado
/// ([`Hospedagens::do_guardado`]), e não o servidor em que a sessão está.
///
/// Pelo caminho de verdade: o `connect` e o `disconnect` da casca, pelo runtime
/// falso do Tauri, e o MOD de referência, com as duas metades, ligado no banco
/// do guardado antes de ele subir (como `seele-server/tests/exemplo_com_mod.rs`).
/// O pedido entra por `pedidos::executar`, que é por onde o do fio entra
/// (`seele-server/src/session.rs`), com a conta de quem ficou.
#[cfg(test)]
mod o_mod_do_guardado_continua_respondendo {
    use super::a_entrada_velha_que_chega_tarde::{app, runtime, servidor, vez};
    use super::{
        connect, disconnect, onde_estou_na, parar_de_hospedar_em, servidor_do_mod, Hospedado,
        OndeEstou, Session,
    };
    use seele_server::hospedagem::Hospedagem;
    use seele_server::mods::instalar::{instalar_de_pasta, raizes_de_um_mundo};
    use seele_server::permissions::Permissions;
    use seele_server::persistence::mods::{enable, EnabledMod};
    use seele_server::persistence::{Location, Persistence};
    use std::time::Duration;
    use tauri::test::MockRuntime;
    use tauri::{AppHandle, Manager};

    /// O MOD de referência do produto, que tem as duas metades.
    const MOD_DE_REFERENCIA: &str =
        concat!(env!("CARGO_MANIFEST_DIR"), "/testes/mod-de-referencia");

    /// Entra em `alvo` pelo `connect` da casca, até 30 s cada tentativa. Um
    /// servidor com MOD pergunta antes (`ModsNaoAceitos`): a dona aceita como a
    /// casca aceita (o corpo de `aceitar_mods`) e entra de novo.
    async fn entrar(app: &AppHandle<MockRuntime>, alvo: &str) {
        for _ in 0..2 {
            let tentativa = tokio::time::timeout(
                Duration::from_secs(30),
                connect(
                    app.clone(),
                    app.state::<Session>(),
                    alvo.to_owned(),
                    "dona".to_owned(),
                    false,
                    None,
                    None,
                ),
            )
            .await
            .unwrap_or_else(|_| panic!("o `connect` a {alvo} não voltou"));
            let Err(falha) = tentativa else {
                return;
            };
            let dita = serde_json::to_value(&falha).unwrap_or_default();
            let Some(conjunto) = dita
                .pointer("/error/ModsNaoAceitos/conjunto")
                .and_then(serde_json::Value::as_str)
            else {
                panic!("o `connect` a {alvo} não entrou: {dita}");
            };
            seele_ffi::mods::aceitar(&super::config_dir(app), alvo, conjunto)
                .expect("o sim aos MODs não gravou");
        }
        panic!("o `connect` a {alvo} não entrou nem com o sim aos MODs");
    }

    /// O que o MOD `id` responde a `contar`, pedido por quem ficou no
    /// guardado, no servidor que os comandos de MOD do app acham. `quando` diz
    /// onde a dona está, para a falha dizer em que ponto o guardado sumiu.
    async fn contar(
        session: &tauri::State<'_, Session>,
        id: &str,
        quando: &str,
    ) -> serde_json::Value {
        let servidor = servidor_do_mod(session).unwrap_or_else(|falha| {
            panic!("{quando}, os comandos de MOD não acham o guardado: {falha:?}")
        });
        // Quem ficou: uma conta do guardado, criada depois da dona, que é a
        // primeira e por isso a Comandante.
        let quem_ficou = {
            let banco = servidor.persistence.lock().await;
            Permissions::new(&banco)
                .register_or_find(&[7; 32], "quem-ficou")
                .expect("a conta de quem ficou")
                .id
        };
        let resposta = seele_server::mods::pedidos::executar(
            &servidor,
            quem_ficou,
            // O canal zero, «nenhum canal»: por `into`, porque a casca não
            // nomeia o `seele-proto` (ADR 0002).
            0_u32.into(),
            id,
            r#"{"op":"contar"}"#,
        )
        .await;
        serde_json::from_str(&resposta)
            .unwrap_or_else(|_| serde_json::json!({ "resposta": resposta }))
    }

    #[test]
    fn um_mod_do_guardado_continua_respondendo_enquanto_o_dono_visita_outro_servidor() {
        let _vez = vez();
        runtime().block_on(async {
            let app = app();
            let session = app.state::<Session>();

            // O guardado, com o MOD de referência ligado antes de subir: a
            // subida carrega o que está ligado.
            let mundo = tempfile::tempdir().expect("a pasta do guardado não abriu");
            let raizes = raizes_de_um_mundo(mundo.path());
            let (manifesto, hash) =
                instalar_de_pasta(std::path::Path::new(MOD_DE_REFERENCIA), &raizes)
                    .expect("o MOD de referência não instalou");
            let banco = Location::File(mundo.path().join("seele.db"));
            enable(
                &Persistence::open(&banco).expect("o banco do guardado não abriu"),
                &EnabledMod {
                    id: manifesto.id.clone(),
                    version: manifesto.version.clone(),
                    hash,
                    repo: manifesto.repo.clone(),
                    reach: manifesto.reach.clone(),
                    server_half: manifesto.server.is_some(),
                    api: manifesto.api,
                },
            )
            .expect("o MOD de referência não ligou");
            let guardado = Hospedagem::iniciar(0, banco, "Casa", Some(raizes), |_| Ok(()))
                .await
                .expect("o guardado não subiu");
            let casa = format!("127.0.0.1:{}", guardado.endereco().port());
            session.hospedagens.lock().expect("as hospedagens").guardado = Some(Hospedado {
                hospedagem: guardado,
                servidor: None,
            });

            // A dona entra no próprio servidor, e o MOD responde.
            entrar(app, &casa).await;
            assert_eq!(
                onde_estou_na(&session),
                OndeEstou::Guardado,
                "a dona não entrou no próprio servidor, e o teste não mede a visita"
            );
            let dentro = contar(&session, &manifesto.id, "com a dona dentro").await;
            assert_eq!(
                dentro.get("vezes"),
                Some(&serde_json::json!(1)),
                "o MOD do guardado não responde nem com a dona dentro, e o teste não mede a \
                 visita: {dentro}"
            );

            // A dona sai e vai a outro servidor.
            disconnect(app.clone(), app.state::<Session>())
                .await
                .expect("o `disconnect` nunca falha");
            let (outro, servidor_outro) = servidor("Outro").await;
            entrar(app, &outro.to_string()).await;
            assert_eq!(
                onde_estou_na(&session),
                OndeEstou::Outro,
                "a dona não está noutro servidor, e o teste não mede a visita"
            );

            // E o MOD do guardado continua respondendo a quem ficou: a mesma
            // instância, com a conta de antes.
            let fora = contar(&session, &manifesto.id, "com a dona noutro servidor").await;
            assert_eq!(
                fora.get("vezes"),
                Some(&serde_json::json!(2)),
                "o MOD do guardado não continuou respondendo a quem ficou nele, com a dona \
                 noutro servidor: {fora}"
            );

            disconnect(app.clone(), app.state::<Session>())
                .await
                .expect("o `disconnect` nunca falha");
            assert!(
                parar_de_hospedar_em(&session).await,
                "o guardado não estava mais no ar no fim"
            );
            servidor_outro.shutdown();
        });
    }
}

// **O teto de um pedaço de texto de terceiro numa linha do `seele.log`** mora
// em `seele_motor::TETO_DA_FRASE_NO_REGISTRO` desde 09/10/2026, com a regra das
// quatro portas: o corte do `console` é da volta do motor, e as portas daqui
// cortam pela mesma regra ([`executor::ate_o_teto_do_registro`]). Os testes do
// executor o leem por `crate::`, como liam quando ele morava aqui.
#[cfg(test)]
use seele_motor::TETO_DA_FRASE_NO_REGISTRO;

/// **O maior id de MOD que uma linha do `seele.log` leva**, em caracteres.
///
/// Um id de verdade (`autor/nome`) é bem menor; o teto é para o que chega pela
/// ponte sem ter passado por um manifesto. Nomeado porque mais de uma porta
/// escreve `mod_id=`, e o número estava escrito à mão em duas: ver
/// [`id_no_registro`].
const TETO_DO_ID_NO_REGISTRO: usize = 128;

/// **O id de um MOD como uma linha do `seele.log` o leva**: cortado em
/// [`TETO_DO_ID_NO_REGISTRO`] e sem caractere que quebre ou inverta a linha
/// ([`quebra_ou_inverte_a_linha`]).
///
/// Uma função só para as portas que escrevem `mod_id=` com um id que chega da
/// janela — a própria janela ([`registrar_da_janela`]), as recusas de mídia
/// ([`recusa_de_midia_dita`]) e de carga ([`recusa_de_carga_dita`]) do Rust, o
/// pedido de MOD ([`mod_request`], pelos três braços de [`IdDoPedido`]), a
/// mídia vinda do servidor ([`midia_em_bytes`]) e o arquivo escolhido para um
/// MOD ([`escolher_para_o_mod`]). É por esse campo que uma busca só junta o
/// que elas disseram do mesmo MOD, e um tratamento repetido à mão em cada uma
/// podia divergir sem que nada reclamasse.
///
/// Sem aspas, porque `mod_id=autor/nome` é a grafia que se procura: é o filtro
/// que impede uma quebra de linha no id de escrever uma segunda linha com a
/// cara do produto.
fn id_no_registro(id: &str) -> String {
    id.chars()
        .take(TETO_DO_ID_NO_REGISTRO)
        .filter(|&c| !quebra_ou_inverte_a_linha(c))
        .collect()
}

/// **Um caractere que, escrito cru numa linha do `seele.log`, a quebra ou
/// reordena o que vem depois dele.**
///
/// Os de controle (`char::is_control`: C0, DEL e C1, com o U+0085 que outros
/// sistemas usam como quebra de linha); os separadores de linha e de parágrafo
/// (U+2028 e U+2029), que um editor quebra como um `\n`; e os de controle
/// bidirecional — a propriedade `Bidi_Control` do Unicode: U+061C, U+200E,
/// U+200F, U+202A a U+202E e U+2066 a U+2069. Um U+202E na frase de um MOD
/// inverte, num editor que segue o bidi, o `onde=` e o `mod_id=` que vêm depois
/// dela na mesma linha.
///
/// As portas que escrevem pelo `Debug` de um `str` escapam todos eles
/// (`\u{2028}`, `\u{202e}`); os campos que vão crus à linha — o `onde` e o id
/// da janela, e o `mod_id=` das recusas de mídia e de carga — passam por aqui.
/// A frase da janela, que vai escapada, também passa, com o caractere trocado
/// por espaço antes do escape: é a segunda defesa.
fn quebra_ou_inverte_a_linha(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}'
                | '\u{2029}'
                | '\u{061C}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
        )
}

/// **Os baldes de vazão do `seele.log`, um por MOD e por classe de linha**
/// ([`Classe`]), pela chave de [`chave_do_balde`]. Ver [`linha_admitida`].
type BaldesDoRegistro =
    Mutex<std::collections::HashMap<(Classe, String), executor::BaldeDoConsole>>;

/// **A classe de uma linha que o produto escreve em nome de um MOD**, e de
/// que balde ela gasta.
///
/// Um balde só por MOD deixava uma classe calar a outra. Segurada, uma linha
/// não atrasa: ela some, e só a conta segue, na próxima linha do MOD — que
/// pode ser de outra classe. Um MOD que monta mais de 32 mídias de uma vez
/// gastava a rajada em «mídia de MOD servida», e a recusa da mídia seguinte
/// sumia: o defeito de 23/09, de volta pelo teto. Com um balde por classe, a
/// INFO e a depuração não gastam a ficha de um aviso, nem inflam a conta dele.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Classe {
    /// O que deu errado: a janela em aviso e em erro
    /// ([`registrar_da_janela`]), e a mídia recusada
    /// ([`recusa_de_midia_dita`]).
    Aviso,
    /// O que saiu: a mídia ([`midia_servida_dita`]) e o som
    /// ([`som_servido_dito`]) servidos.
    Servida,
    /// A depuração, que o filtro padrão do `seele.log` não escreve: a janela em
    /// qualquer outro nível, a mídia não servida porque a sessão acabou, e a
    /// mídia vinda do servidor ([`midia_vinda_do_servidor_dita`]).
    Depuracao,
}

impl Classe {
    /// As três: cada uma tem o seu balde de transbordo.
    const TODAS: [Self; 3] = [Self::Aviso, Self::Servida, Self::Depuracao];
}

/// **Quantos baldes o mapa do registro guarda**, contando os de transbordo.
///
/// Sessenta e quatro: sessenta e um baldes próprios — um por MOD e por classe,
/// o do catálogo entre eles — e, para cada uma das três classes, um dividido
/// ([`BALDE_DE_TRANSBORDO`]) por todo id que chegar depois. Os ids vêm da
/// janela, e uma janela que mandasse um id novo a cada linha faria do mapa
/// memória sem teto — e de cada id novo um balde cheio, a vazão também sem
/// teto. O transbordo é por classe pela razão de [`Classe`]: dividido entre
/// todas, as linhas servidas de um id gastariam a ficha do aviso de outro.
const BALDES_NO_REGISTRO: usize = 64;

/// A chave do balde da linha sem id, que é a do catálogo do servidor
/// (`carregarMods` pede o catálogo com o id vazio). O caractere de controle na
/// frente faz dela uma chave que [`id_no_registro`] nunca devolve: nenhum MOD
/// cai neste balde.
const BALDE_DO_CATALOGO: &str = "\u{0}catalogo";

/// A chave do balde de um id que não é vazio, mas que o filtro de
/// [`id_no_registro`] esvazia (ou deixa só espaço): o terceiro braço de
/// [`IdDoPedido`]. Não é o catálogo — que é o pedido sem MOD —, e não é de MOD
/// nenhum que se leia. Pela razão de [`BALDE_DO_CATALOGO`], nenhum id a
/// escreve.
const BALDE_ILEGIVEL: &str = "\u{0}ilegivel";

/// A chave do balde dividido pelos ids que chegam com o mapa cheio
/// ([`BALDES_NO_REGISTRO`]). Pela razão de [`BALDE_DO_CATALOGO`], nenhum id a
/// escreve.
const BALDE_DE_TRANSBORDO: &str = "\u{0}transbordo";

/// A chave do balde de `mod_id`, pelos três braços de [`IdDoPedido`]: o id
/// vazio é o catálogo ([`BALDE_DO_CATALOGO`]), o que o filtro esvazia é
/// ilegível ([`BALDE_ILEGIVEL`]), e o resto é [`id_no_registro`] dele.
fn chave_do_balde(mod_id: &str) -> String {
    match IdDoPedido::de(mod_id) {
        IdDoPedido::Catalogo => BALDE_DO_CATALOGO.to_owned(),
        IdDoPedido::Ilegivel => BALDE_ILEGIVEL.to_owned(),
        IdDoPedido::Legivel(id) => id,
    }
}

/// **Uma linha que o balde do registro deixou passar**, com os dois campos que
/// ela leva por isso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Admitida {
    /// Quantas foram seguradas antes dela, no molde de
    /// [`registrar_console_do_mod`]. `None` quando nenhuma: o campo só sai
    /// quando diz alguma coisa.
    suprimidas: Option<u32>,
    /// `Some(true)` quando ela passou pelo balde de transbordo.
    ///
    /// A conta de lá é do balde dividido — de todos os ids que caíram nele
    /// depois de o mapa encher —, e sem o campo a linha a atribuía só ao id
    /// dela: `mod_id=x suprimidas=40` se lia como quarenta linhas de `x`, que
    /// podiam ser de outros quarenta MODs.
    transbordo: Option<bool>,
}

/// **Uma linha do registro em nome de `mod_id`, da `classe` dela, quer
/// passar.**
///
/// `Some` é «passa», com os campos que ela leva por isso ([`Admitida`]);
/// `None` é «segurada»: a linha não é escrita, e entra na conta da próxima.
///
/// **A conta só sai numa linha.** A de um MOD que se cala no meio da sessão
/// espera a próxima linha dele, da mesma classe, ou o fim da sessão
/// ([`Session::dizer_as_contas_do_registro`]): não há volta de motor aqui que
/// a diga no meio do caminho, como o executor faz com o `console`.
///
/// # Por que um teto por MOD
///
/// O `console` de um MOD já tinha o dele, no executor. As outras portas por
/// onde um MOD faz o produto escrever não tinham nenhum: a janela
/// ([`registrar_da_janela`]) diz o que um MOD faz dar errado, a cada vez, e um
/// MOD que pede num laço a mesma mídia recusada deixava uma linha por pedido
/// ([`recusa_de_midia_dita`]), num arquivo que só gira na abertura do app. O
/// balde é o do `console` ([`executor::BaldeDoConsole`]): a mesma rajada, o
/// mesmo ritmo e a mesma conta.
///
/// **Um balde por MOD e por classe, dividido pelas portas da classe**: a
/// janela em aviso e a mídia recusada gastam do balde de avisos do MOD; a
/// mídia e o som servidos ([`midia_servida_dita`], [`som_servido_dito`]), do
/// de servidas; a depuração, do dela. Um MOD não segura as linhas de outro, e
/// uma classe não segura as de outra ([`Classe`]). Com o mapa cheio
/// ([`BALDES_NO_REGISTRO`]), um id novo gasta do balde de transbordo da
/// classe — e a conta que uma linha de lá leva pode ser de mais de um id, que
/// é por que ela sai com `transbordo=true` ([`Admitida::transbordo`]).
///
/// O relógio vem de quem chama, como no balde do `console`: é o que deixa o
/// teste medir o teto sem dormir. Um cadeado envenenado não tira o teto: o
/// balde é lido como está.
fn linha_admitida(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    classe: Classe,
    mod_id: &str,
) -> Option<Admitida> {
    let mut baldes = baldes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let chave = (classe, chave_do_balde(mod_id));
    // Um lugar fica guardado para o transbordo de cada classe: o mapa nunca
    // passa de `BALDES_NO_REGISTRO`.
    let proprios = BALDES_NO_REGISTRO - Classe::TODAS.len();
    let proprio = baldes.contains_key(&chave) || baldes.len() < proprios;
    let chave = if proprio {
        chave
    } else {
        (classe, BALDE_DE_TRANSBORDO.to_owned())
    };
    let suprimidas = baldes
        .entry(chave)
        .or_insert_with(|| executor::BaldeDoConsole::novo(agora))
        .admitir(agora)?;
    Some(Admitida {
        suprimidas: (suprimidas > 0).then_some(suprimidas),
        transbordo: (!proprio).then_some(true),
    })
}

/// A frase da linha em que o produto diz a conta de um balde do registro.
const CONTA_DO_BALDE_DO_REGISTRO: &str =
    "linhas do registro seguradas pelo teto de vazão, sem outra linha depois que as contasse";

/// **Diz a conta que um balde do registro guardou e nenhuma linha levou**.
///
/// Quem a chama é [`Session::dizer_as_contas_do_registro`], quando os baldes
/// vão embora: ao fechar a sessão ([`Session::revogar`]), e é por `revogar`
/// que a saída do app passa também ([`despedir_da_sessao`]).
///
/// Com ficha ou sem, como a última linha de uma instância do `console`
/// ([`executor::BaldeDoConsole::pendentes`]): o balde vai embora, e guardar a
/// conta para uma ficha que não vai chegar era perdê-la.
///
/// No nível da classe que ela conta — WARN para os avisos, INFO para as
/// servidas, DEBUG para a depuração —, e com os campos da linha que ela
/// contaria: `mod_id=`, ou `catalogo=true` para a linha sem id, ou
/// `mod_id_ilegivel=true` para o id que o filtro esvazia, ou `transbordo=true`
/// para o balde dividido.
fn conta_do_balde_dita(classe: Classe, chave: &str, suprimidas: u32) {
    let catalogo = (chave == BALDE_DO_CATALOGO).then_some(true);
    let mod_id_ilegivel = (chave == BALDE_ILEGIVEL).then_some(true);
    let transbordo = (chave == BALDE_DE_TRANSBORDO).then_some(true);
    // A chave de um MOD já é `id_no_registro` do id: em `%` ela não quebra a
    // linha.
    let mod_id = (catalogo.is_none() && mod_id_ilegivel.is_none() && transbordo.is_none())
        .then(|| tracing::field::display(chave));
    match classe {
        Classe::Aviso => {
            tracing::warn!(
                mod_id,
                catalogo,
                mod_id_ilegivel,
                transbordo,
                suprimidas,
                "{CONTA_DO_BALDE_DO_REGISTRO}"
            );
        }
        Classe::Servida => {
            tracing::info!(
                mod_id,
                catalogo,
                mod_id_ilegivel,
                transbordo,
                suprimidas,
                "{CONTA_DO_BALDE_DO_REGISTRO}"
            );
        }
        Classe::Depuracao => {
            tracing::debug!(
                mod_id,
                catalogo,
                mod_id_ilegivel,
                transbordo,
                suprimidas,
                "{CONTA_DO_BALDE_DO_REGISTRO}"
            );
        }
    }
}

/// **A janela também escreve no registro.**
///
/// Ela não tinha como. Todo o caminho de um pedido de MOD é observável no Rust
/// — o executor, a bomba, a ponte, o servidor —, e o pedaço que roda na janela
/// era um vão silencioso no meio dele. A medição de 19/09 parou exatamente ali:
/// a fala sai do MOD, o `invoke` nunca acontece, e entre as duas coisas não
/// havia nada escrito.
///
/// É a forma mais cara do defeito que o `CLAUDE.md` nomeia — o produto sabe e
/// não conta — aplicada ao próprio diagnóstico.
///
/// `onde` é um identificador curto de uma lista que quem escreve a chamada
/// escolhe; `o_que` é a frase. Nada disto vai para a tela: é registro, e o
/// registro é de quem hospeda.
///
/// `mod_id` é o MOD de quem a frase fala, quando é de um. Vai num campo
/// próprio, e não só no meio da frase: é ele que faz `mod_id=autor/nome` achar
/// no registro, numa busca só, o que a janela e o Rust disseram sobre o mesmo
/// MOD.
///
/// A linha é `a janela disse onde=… mod_id=… frase="…"`: a frase num campo, em
/// `?`, depois do id — ver o comentário do corte, em
/// [`registrar_da_janela_em`].
///
/// **Com o teto de vazão do MOD** ([`linha_admitida`]): a janela diz o que um
/// MOD faz dar errado a cada vez que dá, e um MOD num laço deixava uma linha
/// por volta. A sessão dá os baldes; os argumentos do `invoke` não mudam.
#[tauri::command]
fn registrar_da_janela(
    session: State<'_, Session>,
    nivel: String,
    onde: String,
    o_que: String,
    mod_id: Option<String>,
) {
    registrar_da_janela_em(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        nivel,
        onde,
        o_que,
        mod_id,
    );
}

/// O corpo de [`registrar_da_janela`], com os baldes e o relógio de quem
/// chama: é o que deixa um teste medir o teto de vazão sem `AppHandle` e sem
/// dormir.
fn registrar_da_janela_em(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    nivel: String,
    onde: String,
    o_que: String,
    mod_id: Option<String>,
) {
    // **O balde primeiro**: a linha segurada não custa o corte nem o escape.
    // A chave é o id como a linha o levaria ([`chave_do_balde`]), e a linha sem
    // id gasta do balde do catálogo. Aviso e erro gastam do balde de avisos;
    // o resto sai em DEBUG e gasta do de depuração ([`Classe`]).
    let classe = match nivel.as_str() {
        "aviso" | "erro" => Classe::Aviso,
        _ => Classe::Depuracao,
    };
    // `suprimidas` só aparece quando não é zero, como no `console` do MOD: é
    // o registro dizendo que houve mais do que ele mostra. `transbordo`, só
    // quando a conta é do balde dividido ([`Admitida`]).
    let Some(Admitida {
        suprimidas,
        transbordo,
    }) = linha_admitida(baldes, agora, classe, mod_id.as_deref().unwrap_or_default())
    else {
        return;
    };
    // Cortado aqui, e não confiando em quem chama: uma frase sem teto vinda da
    // janela é uma linha de registro sem teto no disco de quem hospeda.
    //
    // **Sem caractere que quebre ou inverta a linha**
    // ([`quebra_ou_inverte_a_linha`]), nem aqui nem nos outros dois campos: os
    // três chegam pela mesma ponte, e uma quebra de linha em qualquer um
    // escreveria no `seele.log` uma segunda linha com a cara do produto.
    let onde: String = onde
        .chars()
        .take(64)
        .filter(|&c| !quebra_ou_inverte_a_linha(c))
        .collect();
    // A frase carrega texto que um MOD escolheu — o nome de um ponto, a
    // mensagem de um erro —, e por isso vai num campo em `?`, como o `texto`
    // do `console`: escapada, entre aspas, e **depois** do `onde` e do id. Na
    // mensagem da linha, ela vinha antes dos campos, e um `mod_id=` escrito
    // nela era o primeiro da linha.
    //
    // O caractere que quebra ou inverte a linha é trocado por espaço antes do
    // escape, como segunda defesa. E o corte é o do `console`
    // ([`executor::cortar_linha_do_console`]), pelo tamanho já escapado e com
    // o aviso: em `?`, um caractere chega a ocupar dez, e um `take` de 512
    // caracteres contado antes do escape deixava a frase com dez vezes o teto.
    let o_que: String = o_que
        .chars()
        .map(|c| if quebra_ou_inverte_a_linha(c) { ' ' } else { c })
        .collect();
    let frase = executor::cortar_linha_do_console(o_que);
    let frase = frase.texto();
    // Vazio é ninguém: o id vazio é o do catálogo do próprio servidor
    // (`carregarMods`), e um `mod_id=` sem nome seria achado por uma busca e não
    // seria de MOD nenhum.
    let mod_id: Option<String> = mod_id
        .map(|id| id_no_registro(&id))
        .filter(|id| !id.is_empty());
    // Em `display`, e não como texto cru: `mod_id=autor/nome`, sem aspas, é a
    // mesma grafia que as linhas do Rust escrevem com `%`. Ausente, o campo
    // não sai.
    let mod_id = mod_id.as_deref().map(tracing::field::display);
    match nivel.as_str() {
        "aviso" => {
            tracing::warn!(onde = %onde, mod_id, frase = ?frase, suprimidas, transbordo, "a janela disse");
        }
        "erro" => {
            tracing::error!(onde = %onde, mod_id, frase = ?frase, suprimidas, transbordo, "a janela disse");
        }
        _ => {
            tracing::debug!(onde = %onde, mod_id, frase = ?frase, suprimidas, transbordo, "a janela disse")
        }
    }
}

#[tauri::command]
fn snapshot(session: State<'_, Session>) -> Result<Snapshot, ConnectionError> {
    // **Dito na entrada e na saída.** Um comando que não volta é
    // indistinguível, no registro, de um que nunca foi chamado — e era essa
    // ambiguidade que sobrava depois de instrumentar o resto do caminho de um
    // pedido de MOD, na medição de 19/09.
    tracing::trace!("retrato pedido");
    let retrato = session.connection()?.snapshot();
    tracing::trace!("retrato pronto");
    Ok(retrato)
}

/// **Quantos arquivos escolhidos uma sessão segura ao mesmo tempo.**
///
/// Quatro. Um MOD que abre o seletor num laço não pode encher a memória de quem
/// está numa conversa, e quatro é mais do que qualquer fluxo real precisa: um
/// retrato, uma faixa, uma cena e uma folga.
const ARQUIVOS_DE_PE: usize = 4;

/// **O maior arquivo que uma pessoa pode escolher para um MOD.**
///
/// Dez megabytes, que é o teto que os MODs oficiais já praticavam do lado do
/// servidor. Ele não atravessa inteiro: sai em pedaços que o MOD pede.
const TETO_DO_ESCOLHIDO: usize = 10 * 1024 * 1024;

/// Quanto de um arquivo escolhido sai por pedido.
///
/// **Múltiplo de três**, e é o que importa aqui. O base64 codifica três bytes
/// em quatro caracteres; uma janela que não fecha um grupo de três sai com
/// `=` no meio, e dois pedaços consecutivos concatenados deixam de ser o
/// arquivo. `65536` não é múltiplo de três — `65535` é —, e a diferença entre
/// os dois é uma imagem que chega corrompida sem nada acusar.
///
/// Sessenta e quatro kibibytes viram oitenta e sete mil caracteres: bem abaixo
/// do teto de mensagem, e grande o bastante para dez megabytes saírem em cento
/// e sessenta idas em vez de milhares.
const PEDACO_DO_ESCOLHIDO: usize = 65535;

/// Um arquivo que uma pessoa escolheu, guardado pelo produto.
struct ArquivoEscolhidoDeMod {
    /// De quem é. Um MOD não lê o arquivo que outro pediu.
    de_quem: String,
    /// Em que sessão foi escolhido: a saída leva todos os dela.
    geracao: u64,
    /// O que os bytes provaram ser. Dito à janela na escolha, e guardado para
    /// o diagnóstico saber o que está de pé sem abrir os bytes.
    #[allow(dead_code, reason = "lido pelo diagnóstico da sessão")]
    tipo: &'static str,
    /// Os bytes. Saem em pedaços, e nunca de uma vez.
    bytes: Vec<u8>,
}

/// Os arquivos escolhidos desta janela, por identificador.
#[derive(Default)]
struct ArquivosEscolhidos {
    /// O próximo identificador. Monotônico: um número nunca se repete, e por
    /// isso um pedido atrasado nunca lê o arquivo que ocupou o lugar do dele.
    proximo: u64,
    guardados: std::collections::HashMap<u64, ArquivoEscolhidoDeMod>,
}

impl ArquivosEscolhidos {
    /// Guarda um arquivo e devolve o número dele, ou nada se não couber.
    fn guardar(&mut self, arquivo: ArquivoEscolhidoDeMod) -> Option<u64> {
        if self.guardados.len() >= ARQUIVOS_DE_PE {
            return None;
        }
        self.proximo = self.proximo.saturating_add(1);
        let numero = self.proximo;
        self.guardados.insert(numero, arquivo);
        Some(numero)
    }

    /// Solta tudo o que era de uma geração — é o que a saída chama.
    fn soltar_geracao(&mut self, geracao: u64) -> usize {
        let antes = self.guardados.len();
        self.guardados.retain(|_, a| a.geracao != geracao);
        antes - self.guardados.len()
    }
}

/// O que a bancada precisa saber sobre a sessão de agora — etapa E2.
#[derive(Debug, serde::Serialize)]
struct EstadoDaSessao {
    /// A execução de pé. Zero quer dizer «nenhuma».
    geracao: u64,
    /// Quantos eventos foram descartados por virem de uma geração morta.
    eventos_descartados: u64,
    /// Quantos comandos foram recusados pelo mesmo motivo.
    comandos_recusados: u64,
    /// Quantas falas foram descartadas por chegarem depois da revogação.
    ///
    /// Elas não tinham para onde ir — a colheita recusa a geração que saiu —,
    /// e entregá-las seria admitir efeito de uma sessão encerrada. Contadas
    /// para o descarte não ser silencioso.
    falas_descartadas: u64,
    /// Os MODs de pé no executor nativo, um por um. Vazio fora da bancada.
    ///
    /// **Um por um, e não só a contagem.** A identidade de uma instância é o
    /// que o §3 do contrato pede — nome, geração, pacote e número —, e um total
    /// não responde «qual delas não está saindo».
    mods_nativos: Vec<InstanciaNaTela>,
}

/// Uma instância nativa, como a bancada a lê.
#[derive(Debug, serde::Serialize)]
struct InstanciaNaTela {
    /// O número desta execução. Nunca se repete nesta janela.
    instancia: u64,
    /// `autor/nome`.
    id: String,
    /// Em que execução de sessão ela nasceu.
    geracao: u64,
    /// O conteúdo exato que subiu, cortado — é o que se compara a olho.
    hash: String,
    /// Em que ponto do ciclo ela está: reservada, ativa ou encerrando.
    ///
    /// **`Reservada` é o estado que fecha a corrida de identidade**: a janela
    /// tem o número e o MOD ainda não rodou, então não há fala antiga para
    /// confundir com a nova.
    estado: EstadoNativo,
    /// O executor já confirmou que parou.
    ///
    /// Uma instância que confirmou mas ainda tem fala a colher continua na
    /// supervisão: o que o MOD disse antes de parar ainda precisa chegar.
    parou: bool,
    /// Quantas falas esperam a janela colher.
    ///
    /// **É este número que mede a contenção de verdade.** O da fila do executor
    /// diz o que ainda não saiu do motor; este diz o que saiu e ninguém leu — e
    /// o crédito fica retido enquanto ele for maior que zero.
    a_colher: usize,
    /// Quantos bytes há nelas.
    bytes_a_colher: usize,
    /// Quantos avisos de erro esperam ser colhidos.
    ///
    /// **Fora da contabilidade de dados.** Eles não reservam crédito e não o
    /// devolvem: uma falha do MOD não pode consumir a cota do que funciona,
    /// nem bloquear mensagens futuras por contabilidade inválida.
    avisos: usize,
    /// Quantos avisos não couberam e foram perdidos, por instância.
    avisos_perdidos: u32,
    /// Quantos avisos estão de pé na cota, no canal ou já na instância.
    avisos_no_motor: usize,
    /// Quantos avisos o **executor** recusou antes de sair do motor.
    ///
    /// Outro ponto de retenção, e outro contador: a cota do canal entre o motor
    /// e a bomba é separada da lista da instância, e um MOD que lança num laço
    /// bate primeiro naquela.
    avisos_recusados_no_motor: usize,
    /// Quantas mensagens dela esperam a janela lê-las.
    saida_na_fila: usize,
    /// Quantas ela quis mandar e não couberam.
    ///
    /// **Contado e mostrado** porque uma mensagem descartada em silêncio é o
    /// defeito que este repositório mais paga. Se este número sobe, um MOD está
    /// falando mais rápido do que a janela lê.
    saida_recusadas: usize,
    /// Quantas respostas esperam o motor atendê-las.
    entrada_na_fila: usize,
    /// Quantas respostas não couberam na entrada.
    entrada_recusadas: usize,
}

/// Qual execução de sessão está de pé, e o que já foi recusado por não ser ela.
///
/// **Os contadores não são enfeite.** O contrato pede «contadores de
/// diagnóstico por instância e geração, acessíveis na bancada», e a razão está
/// escrita ao lado deles: sem um número, «não sobrou nada» é inspeção visual, e
/// inspeção visual não vê o evento que chegou dois segundos depois de a tela
/// ter trocado.
#[tauri::command]
fn estado_da_sessao(session: State<'_, Session>) -> EstadoDaSessao {
    use std::sync::atomic::Ordering::Relaxed;
    let nativos = session.mods_nativos.lock().map_or_else(
        |_| Vec::new(),
        |mods| {
            // **As duas tabelas.** Mostrar só as vivas esconderia exatamente o
            // caso que importa: uma que pediu para parar e não confirmou.
            mods.vivos
                .iter()
                .chain(mods.encerrando.iter())
                .map(|(numero, instancia)| {
                    let (saida, _) = instancia.executor.fila().ocupacao();
                    let (entrada, _) = instancia.executor.entrada().ocupacao();
                    InstanciaNaTela {
                        instancia: *numero,
                        id: instancia.id.clone(),
                        geracao: instancia.geracao,
                        hash: instancia.hash.chars().take(12).collect(),
                        estado: instancia.estado,
                        parou: instancia.parou,
                        a_colher: instancia.pendentes.len(),
                        bytes_a_colher: instancia.bytes_pendentes,
                        avisos: instancia.avisos.len(),
                        avisos_perdidos: instancia.avisos_perdidos,
                        avisos_no_motor: instancia.executor.fila().avisos_de_pe(),
                        avisos_recusados_no_motor: instancia.executor.fila().avisos_recusados(),
                        saida_na_fila: saida,
                        saida_recusadas: instancia.executor.fila().recusadas(),
                        entrada_na_fila: entrada,
                        entrada_recusadas: instancia.executor.entrada().recusadas(),
                    }
                })
                .collect()
        },
    );
    EstadoDaSessao {
        geracao: session.geracao(),
        eventos_descartados: session.eventos_de_geracao_morta.load(Relaxed),
        comandos_recusados: session.comandos_de_geracao_morta.load(Relaxed),
        falas_descartadas: session
            .mods_nativos
            .lock()
            .map_or(0, |vivos| vivos.descartadas),
        mods_nativos: nativos,
    }
}

/// A conversa da Linha aberta.
///
/// Fora do `snapshot` de propósito. Aquele é lido a cada quadro de interface, e
/// carregar a conversa junto fazia o custo crescer com a sessão — uma conversa
/// longa ficava lenta de escrever. A tela pede este quando o
/// `messages_revision` do snapshot muda, e só então.
#[tauri::command]
fn messages(session: State<'_, Session>) -> Result<Vec<seele_ffi::Message>, ConnectionError> {
    Ok(session.connection()?.messages())
}

/// O FORA DO AR: os membros deste servidor que não estão conectados agora (P6,
/// D-fio-4).
///
/// Fora do `snapshot` como a conversa, e por mais um motivo: um MOD da API 5
/// recebe o retrato inteiro, e nenhum MOD ganhou a lista de membros. A tela
/// pede este quando o `fora_do_ar_revision` do snapshot muda.
#[tauri::command]
fn membros_fora_do_ar(
    session: State<'_, Session>,
) -> Result<Vec<seele_ffi::MembroForaDoAr>, ConnectionError> {
    Ok(session.connection()?.membros_fora_do_ar())
}

#[tauri::command]
fn enter_voice_room(
    session: State<'_, Session>,
    voice_room: u32,
    senha: Option<String>,
) -> Result<(), ConnectionError> {
    session.connection()?.enter_voice_room(voice_room, senha)
}

#[tauri::command]
fn leave_voice_room(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.leave_voice_room()
}

#[tauri::command]
fn open_channel(session: State<'_, Session>, channel: u32) -> Result<(), ConnectionError> {
    session.connection()?.open_channel(channel)
}

/// Manda uma mensagem, e devolve a chave por onde acompanhá-la.
///
/// A chave é o que faz a janela poder desenhar «enviando», trocar para
/// «recusada» quando a recusa volta, e tentar de novo com a **mesma** chave —
/// que é o que impede um reenvio de publicar duas vezes. Vazia quer dizer «não
/// havia nada para mandar».
///
/// Texto e não número: a chave passa de 2^53 e o `Number` do JavaScript a
/// arredondaria — ver `seele_ffi::Message::client_message_id`.
#[tauri::command]
fn send_message(
    session: State<'_, Session>,
    channel: u32,
    body: String,
) -> Result<String, ConnectionError> {
    session.connection()?.send_message(channel, body)
}

/// Manda de novo uma mensagem que não foi gravada, com a mesma chave.
#[tauri::command]
fn reenviar_mensagem(
    session: State<'_, Session>,
    client_message_id: String,
) -> Result<bool, ConnectionError> {
    session.connection()?.reenviar_mensagem(&client_message_id)
}

/// Desiste de uma mensagem que não foi gravada, e devolve o texto dela.
///
/// O texto volta para que a janela possa pô-lo de novo no compositor: desistir
/// não pode querer dizer perder o que se escreveu.
#[tauri::command]
fn descartar_mensagem(
    session: State<'_, Session>,
    client_message_id: String,
) -> Result<Option<String>, ConnectionError> {
    Ok(session.connection()?.descartar_mensagem(&client_message_id))
}

/// O maior corpo que o servidor aceita, **em bytes**.
///
/// A janela contava caracteres e o contrato conta bytes; um acento a mais
/// derrubava a sessão. Ver `seele_ffi::Connection::limite_da_mensagem`.
#[tauri::command]
const fn limite_da_mensagem() -> usize {
    seele_ffi::Connection::limite_da_mensagem()
}

/// Os tetos dos outros campos que a janela escreve: o apelido, o nome de um
/// canal ou de uma sala, os lugares de uma sala nova, o nome do servidor e o
/// motivo de um banimento (os textos em **bytes**).
///
/// O molde de [`limite_da_mensagem`], e pela mesma razão: os campos contavam
/// caracteres, o fio conta bytes, e o que passava do teto o núcleo recusava
/// sem a janela saber (pendência 58, item 55; lote C55 da F5). Ver
/// `seele_ffi::LimitesDosCampos`.
#[tauri::command]
const fn limites_dos_campos() -> seele_ffi::LimitesDosCampos {
    seele_ffi::Connection::limites_dos_campos()
}

#[tauri::command]
fn mod_request(
    session: State<'_, Session>,
    geracao: u64,
    request: u32,
    id: String,
    channel: u32,
    payload: String,
) -> Result<(), ConnectionError> {
    // **Antes do efeito, e não depois.** Um pedido que a janela despachou antes
    // de sair pode chegar aqui depois; atendê-lo mandaria tráfego de um MOD da
    // sessão anterior para o servidor da seguinte, com o número de correlação
    // dela — e a resposta voltaria para um pedido que outro MOD agora ocupa.
    session.confere_geracao(geracao)?;
    // **Dito na entrada e no desfecho.** Um pedido de MOD que some entre a
    // janela e o servidor é indistinguível, no registro, de um MOD que nunca
    // pediu — e foi exatamente essa ambiguidade que me custou três execuções
    // na medição de 19/09. Com estas duas linhas a pergunta «ele chegou?» tem
    // resposta sem ninguém instrumentar nada.
    pedido_a_caminho_dito(&id, geracao, request, channel);
    let saiu = session
        .connection()?
        .mod_request(request, id.clone(), channel, payload);
    if let Err(erro) = &saiu {
        pedido_recusado_na_saida_dito(&id, request, erro);
    }
    saiu
}

/// **O id de um pedido de MOD como as linhas de [`mod_request`] o levam**:
/// os três braços da metade servidor (`executar`, em
/// `crates/seele-server/src/mods/pedidos.rs`), para que as duas metades
/// escrevam o mesmo pedido do mesmo jeito.
///
/// O id chega da janela, e em `%` sai cru: o formatador escapa só a mensagem.
/// Por isso ele passa por [`id_no_registro`], e o que o filtro deixa decide o
/// braço.
///
/// Os mesmos três braços escolhem o balde de vazão de uma linha
/// ([`chave_do_balde`]): o catálogo, o ilegível e cada MOD têm o seu.
enum IdDoPedido {
    /// O id vazio: o pedido do catálogo do servidor (`carregarMods`), que não
    /// é de MOD nenhum. Sai como `catalogo=true`, sem `mod_id=` em branco —
    /// que casaria com qualquer busca por `mod_id=`.
    Catalogo,
    /// Um id que não é vazio, mas é feito só do que o filtro tira (e de
    /// espaço): escrito, ele seria o mesmo `mod_id=` em branco. Sai como
    /// `mod_id_ilegivel=true`.
    Ilegivel,
    /// O id como a linha o leva, já filtrado.
    Legivel(String),
}

impl IdDoPedido {
    /// Qual dos três braços é `id`.
    fn de(id: &str) -> Self {
        let legivel = id_no_registro(id);
        if id.is_empty() {
            Self::Catalogo
        } else if legivel.trim().is_empty() {
            Self::Ilegivel
        } else {
            Self::Legivel(legivel)
        }
    }
}

/// **Diz no registro que um pedido de MOD saiu para o servidor**, com o id
/// por um dos três braços de [`IdDoPedido`].
fn pedido_a_caminho_dito(id: &str, geracao: u64, request: u32, channel: u32) {
    match IdDoPedido::de(id) {
        IdDoPedido::Catalogo => tracing::debug!(
            catalogo = true,
            geracao,
            request,
            channel,
            "pedido de MOD a caminho do servidor"
        ),
        IdDoPedido::Ilegivel => tracing::debug!(
            mod_id_ilegivel = true,
            geracao,
            request,
            channel,
            "pedido de MOD a caminho do servidor"
        ),
        IdDoPedido::Legivel(legivel) => tracing::debug!(
            mod_id = %legivel,
            geracao,
            request,
            channel,
            "pedido de MOD a caminho do servidor"
        ),
    }
}

/// **Diz no registro que um pedido de MOD foi recusado antes de sair**, com o
/// id pelos mesmos três braços ([`IdDoPedido`]).
fn pedido_recusado_na_saida_dito(id: &str, request: u32, erro: &ConnectionError) {
    match IdDoPedido::de(id) {
        IdDoPedido::Catalogo => {
            tracing::warn!(catalogo = true, request, %erro, "pedido de MOD recusado na saída");
        }
        IdDoPedido::Ilegivel => tracing::warn!(
            mod_id_ilegivel = true,
            request,
            %erro,
            "pedido de MOD recusado na saída"
        ),
        IdDoPedido::Legivel(legivel) => {
            tracing::warn!(mod_id = %legivel, request, %erro, "pedido de MOD recusado na saída");
        }
    }
}

// ---------------------------------------------------------------- anexos
//
// ADR 0027. Quatro comandos, e o que **não** existe entre eles é a decisão: não
// há `abrir_anexo`. Nenhum cliente do SEELE abre arquivo, e este é o único
// ponto do desenho em que dá para ser estrito — então ele é estrito. Salvar é
// um ato de quem recebeu, num lugar que a pessoa escolheu, e o que acontece
// depois é com ela e com o sistema operacional dela.
//
// O quarto é o seletor, e ele entrou depois: o ADR 0027 tinha decidido que
// escolher um arquivo era arrastá-lo, e o primeiro dono a usar isto clicou no
// botão ARQUIVO esperando um seletor. A emenda do ADR conta o resto.

/// O que se sabe de um arquivo antes de mandá-lo.
#[derive(serde::Serialize)]
struct ArquivoEscolhido {
    /// O caminho nesta máquina. Nunca sai daqui: o servidor guarda o blob sob o
    /// hash do conteúdo, então nada deste caminho atravessa a rede.
    caminho: String,
    /// O nome que vai junto. **Não renomeado e sem extensão cortada** — o ADR
    /// 0027 é explícito: renomear `.exe` para parecer inofensivo faz o arquivo
    /// mentir, e mentir é a última coisa que ajuda aqui.
    nome: String,
    /// Quantos bytes.
    tamanho: u64,
    /// O tipo alegado, deduzido da extensão. **Alegação**, e é assim que ela
    /// atravessa: ninguém decide o que decodificar só por causa disto.
    tipo: String,
}

/// Lê nome, tamanho e tipo alegado de um arquivo que alguém escolheu.
///
/// Antes de mandar, para a tela poder mostrar o que vai e a pessoa poder
/// desistir. O tamanho é o que faz a barra ser barra: ele é sempre conhecido —
/// quem escolheu o arquivo sabe o tamanho dele —, então esta janela nunca
/// mostra um travessão no lugar de um andamento.
#[tauri::command]
fn descrever_arquivo(caminho: String) -> Result<ArquivoEscolhido, ConnectionError> {
    let alvo = std::path::PathBuf::from(&caminho);
    let meta = std::fs::metadata(&alvo).map_err(|_| ConnectionError::NotConnected)?;
    if !meta.is_file() {
        return Err(ConnectionError::NotConnected);
    }
    let nome = alvo
        .file_name()
        .map(|nome| nome.to_string_lossy().into_owned())
        .unwrap_or_else(|| "arquivo".to_owned());
    Ok(ArquivoEscolhido {
        tipo: tipo_alegado(&nome),
        caminho,
        nome,
        tamanho: meta.len(),
    })
}

/// O maior que uma imagem colada pode ter, em bytes.
///
/// **Não é o limite do servidor** — quem decide se um anexo cabe é ele, e a
/// recusa dele já tem nome (`AttachmentRefusal::TooLarge`). Este é o limite
/// desta ponte: os bytes colados atravessam para cá **em base64 dentro de uma
/// chamada**, e base64 engorda um terço. Vinte e cinco megabytes viram um texto
/// de trinta e três, montado de uma vez na memória da janela e de novo aqui.
///
/// Acima disso a resposta honesta é «mande como arquivo»: o caminho do seletor
/// não paga essa travessia, porque manda o caminho e o Rust lê do disco.
const MAIOR_COLAGEM_BYTES: usize = 25 * 1024 * 1024;

/// Guarda em disco uma imagem que veio da área de transferência.
///
/// # Por que existe, e por que não dá para reaproveitar o seletor
///
/// Todo anexo deste produto viaja por **caminho de arquivo**: `enviar_anexo`
/// recebe um caminho e lê do disco. Uma imagem colada não tem caminho — o que a
/// janela tem são bytes de um `Blob` que nunca esteve num arquivo.
///
/// Então o caminho é inventado aqui: os bytes viram um arquivo temporário, e o
/// que volta é o mesmo [`ArquivoEscolhido`] que o seletor devolve. Deste ponto
/// em diante **não há caminho novo nenhum** — a barra de progresso, a recusa do
/// servidor, o nome que vai junto, tudo é o fluxo que já existia.
///
/// # O nome
///
/// A área de transferência não traz um. `colado-<carimbo>.<extensão>` é
/// legível, ordena por tempo, e a extensão sai do tipo que a janela relatou —
/// que é uma **alegação**, como toda extensão neste produto (ADR 0027). Nada
/// aqui decodifica a imagem para conferir.
///
/// # Onde o arquivo fica
///
/// Na pasta temporária do sistema, e não na de configuração: é um arquivo
/// descartável cujo dono é o sistema operacional. Ele **não** é apagado aqui —
/// entre esta chamada e o envio existe a tela, onde a pessoa pode desistir,
/// trocar de arquivo ou fechar a janela, e um apagamento cedo demais tiraria o
/// arquivo debaixo do envio.
///
/// # Errors
///
/// [`FalhaAoColar::GrandeDemais`] acima de [`MAIOR_COLAGEM_BYTES`],
/// [`FalhaAoColar::NaoEImagem`] quando o texto não é base64, e
/// [`FalhaAoColar::NaoGravei`] quando o disco recusa.
#[tauri::command]
fn guardar_colado(tipo: String, base64: String) -> Result<ArquivoEscolhido, FalhaAoColar> {
    // Conferido **antes** de decodificar: decodificar para depois recusar seria
    // montar na memória exatamente o que se quer recusar.
    if base64.len() > MAIOR_COLAGEM_BYTES / 3 * 4 {
        return Err(FalhaAoColar::GrandeDemais);
    }
    let bytes = seele_ffi::de_base64(&base64).ok_or(FalhaAoColar::NaoEImagem)?;
    if bytes.len() > MAIOR_COLAGEM_BYTES {
        return Err(FalhaAoColar::GrandeDemais);
    }
    if bytes.is_empty() {
        return Err(FalhaAoColar::NaoEImagem);
    }

    let extensao = match tipo.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        // Um tipo que esta versão não conhece **não** vira `.png`: a extensão é
        // o que o outro lado usa para decidir como abrir, e chutar seria fazer
        // o arquivo mentir. Sem extensão ele chega como o que é — desconhecido.
        _ => "",
    };
    let carimbo = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|desde| desde.as_millis())
        .unwrap_or(0);
    let nome = if extensao.is_empty() {
        format!("colado-{carimbo}")
    } else {
        format!("colado-{carimbo}.{extensao}")
    };

    let alvo = std::env::temp_dir().join(&nome);
    let tamanho = bytes.len() as u64;
    std::fs::write(&alvo, &bytes).map_err(|erro| {
        tracing::warn!(%erro, "não consegui gravar a imagem colada");
        FalhaAoColar::NaoGravei
    })?;

    Ok(ArquivoEscolhido {
        tipo: tipo_alegado(&nome),
        caminho: alvo.to_string_lossy().into_owned(),
        nome,
        tamanho,
    })
}

/// Por que não deu para guardar o que foi colado.
///
/// Enum e não frase, pela razão de sempre nesta fronteira: quem escreve a frase
/// é a casca, que sabe em que idioma a pessoa está lendo.
#[derive(Debug, Clone, Copy, serde::Serialize)]
enum FalhaAoColar {
    /// Acima do que esta ponte carrega.
    GrandeDemais,
    /// O que veio não era base64, ou não veio nada.
    NaoEImagem,
    /// O disco recusou o arquivo temporário.
    NaoGravei,
}

/// Abre o seletor de arquivos do sistema e descreve o que a pessoa escolheu.
///
/// `Ok(None)` é desistir, e é o caso mais comum de todos: quem abre um seletor
/// fecha um seletor. Não é falha e não vira frase de erro em lugar nenhum.
///
/// **Fora da linha principal, e não por gosto.** O diálogo é modal e roda no
/// laço de eventos da janela; a versão bloqueante deste seletor trava esse laço
/// se for chamada de dentro dele, e é o que aconteceria num comando síncrono.
/// Então este é `async`, o seletor é o não-bloqueante, e a resposta volta por um
/// canal — a janela continua desenhando enquanto o diálogo está aberto.
///
/// Nenhum filtro de extensão. É a mesma decisão do `tipo_alegado` logo abaixo,
/// pelo mesmo motivo do ADR 0027: uma lista de extensões aqui esconderia
/// justamente o arquivo que alguém quer mandar, e um `rename` a contorna.
#[tauri::command]
async fn escolher_arquivo(app: AppHandle) -> Result<Option<ArquivoEscolhido>, ConnectionError> {
    use tauri_plugin_dialog::DialogExt as _;

    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title("Escolha um arquivo para anexar")
        .pick_file(move |escolha| {
            // `try_send` e não `blocking_send`: isto roda no laço de eventos da
            // janela, e o canal tem espaço para este único envio. Bloquear aqui
            // seria trocar um travamento por outro.
            let _ = envia.try_send(escolha);
        });

    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(None);
    };
    let Ok(caminho) = escolha.into_path() else {
        // Só o Android devolve `content://`, e este binário não roda lá. Um
        // caminho que não é caminho é recusado em vez de virar texto.
        return Err(ConnectionError::NotConnected);
    };
    descrever_arquivo(caminho.display().to_string()).map(Some)
}

/// O tipo que a extensão sugere.
///
/// Uma **alegação**, e o único uso dela é registro: o servidor a guarda como
/// alegação e nenhuma tela deste app decide o que desenhar por causa dela. Não
/// há lista de extensões proibidas, e não vai haver: o ADR 0027 explica por
/// que uma lista dessas é pior que lista nenhuma — contorna-se com um
/// `rename`, quebra mandar um build deste próprio projeto a um amigo, e faz o
/// que passou parecer conferido.
fn tipo_alegado(nome: &str) -> String {
    let extensao = nome
        .rsplit_once('.')
        .map(|(_, fim)| fim.to_ascii_lowercase());
    match extensao.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        Some("txt" | "md") => "text/plain",
        Some("opus") => "audio/opus",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
    .to_owned()
}

/// Manda um arquivo, num fluxo só dele.
///
/// Devolve a chave de idempotência da mensagem, na hora, porque a tela precisa
/// dela **agora**: é por ela que a barra encontra a própria subida entre os
/// eventos que chegam, e é ela que torna uma retentativa segura em vez de uma
/// segunda mensagem.
///
/// A mensagem só aparece na Linha quando os bytes chegam inteiros. Enquanto
/// sobe, quem a vê é só quem enviou — o custo que o ADR 0027 aceita para que
/// «ainda não chegou» e «expirou» nunca sejam duas ausências parecidas na
/// mesma tela.
#[tauri::command]
fn enviar_anexo(
    session: State<'_, Session>,
    channel: u32,
    body: String,
    caminho: String,
    nome: String,
    tipo: String,
) -> Result<u64, ConnectionError> {
    session
        .connection()?
        .send_attachment(channel, body, caminho, nome, tipo)
}

/// A pasta onde os anexos salvos vão: a de downloads, ou a pessoal.
///
/// **Calculada aqui, em Rust, e em nenhum outro lugar.** A janela não manda
/// caminho: ela mandava, e `salvar_anexo` aceitava qualquer caminho absoluto, de
/// modo que qualquer script que rodasse nela gravava onde quisesse. Vazia
/// quando a máquina não diz nenhuma das duas, ou diz num caminho que não é
/// texto — e a ponte recusa uma pasta vazia como `SemPasta`, em vez de gravar
/// num caminho relativo que ninguém consegue nomear.
fn pasta_dos_anexos(app: &AppHandle) -> String {
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .ok()
        .and_then(|pasta| pasta.into_os_string().into_string().ok())
        .unwrap_or_default()
}

/// Salva um anexo na pasta de downloads.
///
/// **Da janela vem só o número do anexo.** A pasta é [`pasta_dos_anexos`], e o
/// nome a ponte lê do histórico local e confere em
/// `seele_core::anexo_no_disco`, que é onde mora a regra: um nome que não é só
/// um nome é recusado, e um nome tomado grava ao lado, como «foto (2).png», sem
/// substituir nada. O resultado volta como evento, com o caminho real ou o
/// motivo.
///
/// O arquivo é marcado com a quarentena do próprio sistema ao ser gravado —
/// `com.apple.quarantine` no macOS, o fluxo `Zone.Identifier` no Windows —, que
/// é o que faz o Gatekeeper e o SmartScreen pararem o arquivo na frente de quem
/// for abri-lo. Não é antivírus: **este produto não varre vírus e não vai
/// varrer.** É a guarda que o sistema já tem, e que só funciona se quem grava a
/// acionar.
#[tauri::command]
fn salvar_anexo(
    app: AppHandle,
    session: State<'_, Session>,
    anexo: u64,
) -> Result<(), ConnectionError> {
    session
        .connection()?
        .save_attachment(anexo, pasta_dos_anexos(&app))
}

/// Onde [`salvar_anexo`] gravaria este anexo, ou por que não gravaria.
///
/// **Só para a frase de confirmação.** A janela escreve a pasta e o nome que
/// voltam daqui, e mais nada: na hora de gravar, `salvar_anexo` não recebe nada
/// disto de volta e deriva tudo de novo.
#[tauri::command]
fn destino_do_anexo(
    app: AppHandle,
    session: State<'_, Session>,
    anexo: u64,
) -> Result<SaveDestination, SaveRefused> {
    let pasta = pasta_dos_anexos(&app);
    // Sem sessão não há histórico, e sem histórico não há de onde tirar o nome.
    let Ok(connection) = session.connection() else {
        return Err(SaveRefused {
            reason: NotSavedReason::AnexoDesconhecido,
            claimed: String::new(),
            folder: pasta,
        });
    };
    connection.save_destination(anexo, pasta)
}

/// Baixa um anexo pequeno e diz se esta janela pode desenhá-lo.
///
/// A outra metade da regra do ADR 0027, e a que faltava: só uma lista curta de
/// tipos de imagem é desenhada embutida, e **só quando os bytes concordam com a
/// alegação**. O nome do arquivo e o tipo declarado são texto que a outra pessoa
/// escolheu; os primeiros bytes é que dizem o que a coisa é, e é deles que sai o
/// tipo de mídia que o `<img>` recebe.
///
/// **Prever não é abrir, e não é salvar.** Nada é gravado em disco em ponto
/// nenhum deste caminho — não há arquivo, não há caminho, não há marca de
/// quarentena a pôr, e nenhuma tela deste produto ganhou um botão que abre
/// arquivo. Salvar continua sendo o único verbo com destino, e continua tendo a
/// confirmação que diz em voz alta o que este produto não promete.
///
/// **Acontece ao apertar, nunca ao rolar.** O anexo está no servidor: ver é baixar.
/// Uma Linha que buscasse toda imagem enquanto a conversa rola transformaria o
/// teto de disco de quem hospeda em banda de todo mundo, uma vez por vez que
/// alguém abrisse a Linha.
#[tauri::command]
async fn prever_anexo(session: State<'_, Session>, anexo: u64) -> Result<Preview, ConnectionError> {
    let connection = session.connection()?;
    connection.preview_attachment(anexo).await
}

/// O que a tela precisa para decidir se oferece uma prévia.
///
/// Conveniência e não o limite: uma janela que pedisse assim mesmo receberia
/// `TooBig` ou `NotAPicture`, e a decisão continua sendo tomada no lado que
/// baixa e olha os bytes.
///
/// O número é do cliente e não do servidor de propósito. O teto por arquivo é uma
/// fração do teto de disco de quem hospeda e protege o disco **dele**; este
/// protege a memória de quem está lendo, que é outra máquina. E a lista dos
/// tipos vem daqui em vez de ser escrita de novo na página: duas cópias da
/// mesma lista discordam um dia, e a discordância seria a tela oferecer
/// desenhar o que a busca depois recusa.
#[tauri::command]
fn regras_de_previa() -> PreviewRules {
    Connection::preview_rules()
}

/// Onde os arquivos salvos vão parar.
///
/// A pasta de downloads do sistema, ou a pessoal — [`pasta_dos_anexos`], a
/// mesma que [`salvar_anexo`] usa, escolhida em Rust. Escrita na dica do botão
/// de salvar, e por extenso na confirmação, antes de qualquer arquivo ser
/// gravado: o lugar tem de estar **visível** em vez de suposto. Não há diálogo
/// de salvar, e não é por falta de crate — o `tauri-plugin-dialog` já está na
/// árvore, e é ele que abre o seletor de [`escolher_arquivo`]. É que o destino
/// é decisão do Rust, e um diálogo seria outra decisão de produto.
#[tauri::command]
fn pasta_de_downloads(app: AppHandle) -> String {
    pasta_dos_anexos(&app)
}

/// Pede ao servidor que faça uma sala de voz.
///
/// Devolve assim que o pedido entra na fila, e **não** quando a sala existe.
/// Quem responde isso é o servidor, e a resposta chega pela mesma porta que todo o
/// resto: `ChannelsChanged` se ele fez, `NoticeRaised` com `PermissionDenied`
/// se recusou. A tela redesenha a lista pelo evento, como já faz quando alguém
/// entra numa sala de voz — não há caminho novo a aprender.
///
/// A tela pode consultar `Snapshot::may_manage_voice_rooms` para decidir se mostra o
/// botão. Isso é conveniência: mandar o pedido sem ter a permissão não cria
/// nada, e a `specs/08-seguranca.md` põe a segurança nessa recusa e não no
/// botão escondido.
#[tauri::command]
fn criar_voice_room(
    session: State<'_, Session>,
    name: String,
    limit: u16,
    channel: Option<u32>,
) -> Result<(), ConnectionError> {
    session
        .connection()?
        .create_voice_room(name, limit, channel)
}

/// Pede ao servidor que faça uma Linha.
#[tauri::command]
fn criar_linha(session: State<'_, Session>, name: String) -> Result<(), ConnectionError> {
    session.connection()?.create_channel(name)
}

/// Pede ao servidor que renomeie uma sala de voz.
#[tauri::command]
fn renomear_voice_room(
    session: State<'_, Session>,
    voice_room: u32,
    name: String,
) -> Result<(), ConnectionError> {
    session.connection()?.rename_voice_room(voice_room, name)
}

/// Pede ao servidor que renomeie uma Linha.
#[tauri::command]
fn renomear_linha(
    session: State<'_, Session>,
    channel: u32,
    name: String,
) -> Result<(), ConnectionError> {
    session.connection()?.rename_channel(channel, name)
}

// ------------------------------------------------- a cara e o nome do servidor
//
// O que quem hospeda personaliza: o nome que todo mundo lê no cabeçalho e a
// imagem ao lado dele. Cinco comandos, e nenhum deles decide nada — a
// permissão é conferida pelo PERMISSIONS no instante do verbo, e o que é uma
// imagem aceitável é conferido pelo próprio protocolo, dentro de
// `Connection::set_server_icon`.

/// Pede ao servidor que troque o próprio nome.
///
/// A tela pode consultar `Snapshot::may_customise_server` para decidir se
/// desenha o campo. Conveniência, como em `criar_voice_room`: quem pede sem a
/// permissão recebe `Alert`/`PermissionDenied` do servidor, e é lá que a
/// `specs/08-seguranca.md` põe a segurança — nunca no controle escondido.
#[tauri::command]
fn renomear_server(
    app: AppHandle,
    session: State<'_, Session>,
    name: String,
) -> Result<(), ConnectionError> {
    session.connection()?.rename_server(name.clone())?;
    // **E a lista para de mentir.** O nome de verdade mora no banco do
    // servidor; o do registro é só o que a lista mostra para se escolher entre
    // um e outro sem abrir os dois. Sem esta linha ele ficaria congelado no dia
    // da criação, e escolher pelo nome errado é pior que escolher por um
    // identificador cru.
    if let Ok(hospedagens) = session.hospedagens.lock() {
        if let Some(id) = hospedagens
            .guardado
            .as_ref()
            .and_then(|guardado| guardado.servidor.as_deref())
        {
            servidores::renomear(&config_dir(&app), id, &name);
        }
    }
    Ok(())
}

/// O que a tela pode dizer sobre a imagem **antes** de alguém escolher uma.
///
/// Os dois números que o protocolo cobra, num lugar só, para que a frase da
/// tela não os traga escritos à mão. É a forma de `regras_de_previa`, pelo
/// mesmo motivo dela: duas cópias da mesma regra discordam um dia, e a
/// discordância aqui seria a tela prometendo aceitar o que o servidor recusa.
///
/// **Isto ainda é uma cópia**, e a nota é a dívida: os números de verdade são
/// `seele_proto::control::MAX_SERVER_ICON_LEN` e `MAX_SERVER_ICON_SIDE`, e o
/// ADR 0002 impede este binário de enxergá-los — ele vê `seele-ffi` e nada
/// além. `Connection::preview_rules()` existe justamente para não fazer isto com o
/// teto de prévia; falta o irmão dela, `Connection::server_icon_rules()`, e enquanto
/// ele não existe a cópia mora aqui, em Rust, onde uma linha a substitui.
///
/// O que a cópia **não** faz é julgar. Nenhum comando abaixo recusa uma imagem
/// por causa destes números: quem recusa é `Connection::set_server_icon`, com a
/// função do protocolo, e o número que a tela escreve no erro é o que o
/// `ConnectionError::IconTooBig` carrega. Se esta cópia envelhecer, a tela mostra
/// dois números diferentes — que é ruim, e ainda assim é melhor que uma casca
/// recusando em nome de uma regra que deixou de ser a regra.
#[tauri::command]
fn regras_do_icone_do_server() -> RegrasDoIcone {
    RegrasDoIcone {
        limite_bytes: TETO_DO_ICONE,
        lado: LADO_DO_ICONE,
    }
}

/// O que oferecer a quem clicou em compartilhar e não tem o módulo de vídeo.
///
/// `None` quando não há o que oferecer: o módulo já está lá, ou este sistema
/// não tem um publicado. A casca não mostra a caixa nos dois casos.
#[tauri::command]
fn modulo_de_video_a_baixar(app: AppHandle) -> Option<seele_ffi::ModuloAOferecer> {
    seele_ffi::modulo_de_video_a_baixar(&config_dir(&app))
}

/// Busca o módulo do Cisco e o instala, depois de a pessoa ter dito que sim.
///
/// **Só é chamado por um clique.** Nada aqui roda sozinho no arranque: baixar
/// um megabyte da rede é uma coisa que se pede, e o ADR 0026 já fixou essa
/// postura para o atualizador — que é, aliás, quem põe o `reqwest` nesta árvore,
/// então este comando não custa dependência nenhuma nova.
///
/// A conferência do hash não está aqui, e é de propósito: ela é do
/// `seele-video`, que é quem fixou o número, e assim ela é testável sem rede.
/// Esta função faz a única parte que não dá para testar sem rede — pedir os
/// bytes — e entrega o resto para quem sabe recusá-los.
#[tauri::command]
async fn baixar_modulo_de_video(app: AppHandle) -> Result<String, seele_ffi::ConnectionError> {
    let pasta = config_dir(&app);
    let Some(oferta) = seele_ffi::modulo_de_video_a_baixar(&pasta) else {
        // Já está instalado, ou não existe para este sistema. Nos dois casos não
        // há nada a buscar, e devolver erro seria pior: quem clicou duas vezes
        // veria uma falha por ter conseguido.
        return Ok(pasta);
    };

    let resposta = reqwest::get(&oferta.url).await.map_err(|erro| {
        tracing::warn!(%erro, url = %oferta.url, "não consegui pedir o módulo de vídeo");
        seele_ffi::ConnectionError::ScreenModuleRefused
    })?;
    if !resposta.status().is_success() {
        tracing::warn!(status = %resposta.status(), url = %oferta.url, "a origem do módulo recusou");
        return Err(seele_ffi::ConnectionError::ScreenModuleRefused);
    }
    let bytes = resposta.bytes().await.map_err(|erro| {
        tracing::warn!(%erro, "o download do módulo de vídeo não completou");
        seele_ffi::ConnectionError::ScreenModuleRefused
    })?;

    let caminho = seele_ffi::instalar_modulo_de_video(&pasta, &bytes)?;
    tracing::info!(caminho, "o módulo de vídeo está instalado");
    Ok(caminho)
}

/// Quanto pesa e quão grande é a imagem que o servidor aceita.
#[derive(Debug, Clone, Copy, serde::Serialize)]
struct RegrasDoIcone {
    /// O máximo de bytes.
    limite_bytes: u64,
    /// O máximo de pixels de cada lado.
    lado: u32,
}

/// Cópia do teto do protocolo. Veja `regras_do_icone_do_server`.
const TETO_DO_ICONE: u64 = 8 * 1024;

/// Cópia do lado máximo do protocolo. Veja `regras_do_icone_do_server`.
const LADO_DO_ICONE: u32 = 256;

/// Abre o seletor, lê o que a pessoa escolheu e o põe como imagem do servidor.
///
/// Um comando e não dois — escolher e aplicar — porque esta tela não tem
/// SALVAR: a escolha vale na hora, como a do microfone ao lado. `Ok(false)` é
/// desistir do seletor, que é o desfecho mais comum e não é falha nenhuma.
///
/// **Sem filtro de extensão**, pela razão do ADR 0027 que `escolher_arquivo` já
/// segue: um filtro esconde justamente o arquivo que a pessoa quer, e um
/// `rename` o contorna. O que separa uma imagem aceitável de outra é o
/// conteúdo, e quem o lê é o protocolo.
///
/// **Fora da linha principal**, como `escolher_arquivo` e pelo mesmo motivo: o
/// diálogo é modal e roda no laço de eventos da janela, e a versão bloqueante
/// trava esse laço se for chamada de dentro dele.
///
/// **A imagem escolhida é encolhida aqui**, e é o que faz este botão servir para
/// alguma coisa. O teto do protocolo é 8 KiB, e nenhuma imagem que uma pessoa
/// tem no computador tem 8 KiB: a versão anterior lia o arquivo, ouvia «não
/// cabe» do protocolo e devolvia isso à tela, o que na prática queria dizer
/// «arrume um PNG minúsculo por conta própria». Ver `icone::encolher`.
///
/// Lê no máximo `icone::TETO_DA_ORIGEM`, que é o teto do **arquivo escolhido** e
/// não o do ícone: sem um corte, escolher um vídeo de dois gigabytes seria
/// lê-lo inteiro para a memória antes de descobrir que não é imagem.
#[tauri::command]
async fn escolher_icone_do_server(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<bool, ConnectionError> {
    let Some(pronto) = escolher_e_encolher(&app, "Escolha a imagem deste servidor").await? else {
        return Ok(false);
    };
    session.connection()?.set_server_icon(Some(pronto))?;
    Ok(true)
}

/// Abre o seletor, lê e encolhe — e devolve os bytes, sem aplicar nada.
///
/// A metade comum dos três lugares que escolhem uma imagem. Era duas cópias com
/// um comentário dizendo que eram cópias de propósito; a terceira é que não
/// cabia, porque a tela de preparar o servidor precisa da imagem **antes** de
/// existir servidor para aplicá-la.
///
/// `Ok(None)` é ter fechado o seletor sem escolher, e é o desfecho mais comum.
///
/// # Errors
///
/// [`ConnectionError::IconNotAPicture`] quando o arquivo não vira um PNG dentro
/// do teto.
async fn escolher_e_encolher(
    app: &AppHandle,
    titulo: &str,
) -> Result<Option<Vec<u8>>, ConnectionError> {
    use std::io::Read as _;
    use tauri_plugin_dialog::DialogExt as _;

    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title(titulo)
        .pick_file(move |escolha| {
            let _ = envia.try_send(escolha);
        });

    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(None);
    };
    let Ok(caminho) = escolha.into_path() else {
        // Só o Android devolve `content://`, e este binário não roda lá.
        return Err(ConnectionError::IconNotAPicture);
    };

    let Ok(arquivo) = std::fs::File::open(&caminho) else {
        // Um arquivo que não abre não é uma imagem que este servidor possa usar, e
        // é a única frase honesta que esta casca tem: ela não sabe se o disco
        // sumiu ou se a permissão é de outra pessoa.
        return Err(ConnectionError::IconNotAPicture);
    };
    let mut bytes = Vec::new();
    if arquivo
        .take(icone::TETO_DA_ORIGEM.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Err(ConnectionError::IconNotAPicture);
    }

    // Fora da linha principal: uma foto de doze megapixels leva um tempo visível
    // para ser decodificada e reduzida, e fazer isso no laço de eventos congela
    // a janela inteira no meio de um gesto que parecia instantâneo.
    let Ok(pronto) = tauri::async_runtime::spawn_blocking(move || icone::encolher(&bytes)).await
    else {
        return Err(ConnectionError::IconNotAPicture);
    };
    // Não é imagem, ou é uma que nem o último degrau fez caber. As duas dizem a
    // mesma coisa a quem escolheu: este arquivo não vira ícone.
    pronto.map(Some).ok_or(ConnectionError::IconNotAPicture)
}

/// Escolhe a imagem do servidor **antes** de existir servidor.
///
/// A tela de preparar hospedagem precisa mostrar a imagem escolhida enquanto
/// ainda não há nada para aplicá-la: `renomear_server` e `set_server_icon` são
/// verbos de protocolo e só valem depois de conectar. Então esta devolve os
/// bytes já encolhidos, a página os segura, e `aplicar_icone_do_server` os
/// manda quando o servidor estiver de pé.
///
/// `Ok(None)` é ter fechado o seletor sem escolher.
///
/// # Errors
///
/// [`ConnectionError::IconNotAPicture`] quando o arquivo não vira um PNG dentro
/// do teto.
#[tauri::command]
async fn preparar_icone_do_server(app: AppHandle) -> Result<Option<Vec<u8>>, ConnectionError> {
    escolher_e_encolher(&app, "Escolha a imagem deste servidor").await
}

/// Põe no servidor uma imagem que já veio pronta.
///
/// Gêmea de [`escolher_icone_do_server`] sem o seletor: os bytes vêm da tela de
/// preparar, que os escolheu antes de hospedar. **Não** encolhe de novo — eles
/// já passaram por `icone::encolher`, e encolher duas vezes só perderia
/// qualidade para chegar ao mesmo lugar.
///
/// # Errors
///
/// O que a sessão devolver, incluindo a recusa do servidor.
#[tauri::command]
fn aplicar_icone_do_server(
    session: State<'_, Session>,
    icone: Vec<u8>,
) -> Result<(), ConnectionError> {
    session.connection()?.set_server_icon(Some(icone))
}

/// Tira a imagem do servidor, deixando-o sem nenhuma.
///
/// Verbo próprio e não `escolher` com um argumento vazio: são duas coisas que
/// uma pessoa faz por motivos diferentes, e são dois botões na tela.
#[tauri::command]
fn tirar_icone_do_server(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.set_server_icon(None)
}

// ------------------------------------------------------------------------ MODs
//
// ADR 0045. Um MOD é código de terceiro, e as regras do produto base não o
// alcançam — a defesa é o repositório público e a revisão de código de cada
// versão, e ela não mora aqui.
//
// Nesta etapa habilitar é **local**: fala com o banco do servidor que esta
// janela hospeda, pelo mesmo acessor da portaria (ADR 0030). Os verbos de
// protocolo com `AdministerServer` são de um plano posterior, e é aí que um
// servidor rodando por `seeled` numa VPS passa a ser administrável.

/// Por que um comando de MOD não deu.
///
/// Enum e não frase, como as vizinhas: a fronteira erro→texto é do frontend.
#[derive(Debug, serde::Serialize)]
enum FalhaNoMod {
    /// Esta janela não está hospedando, então não há servidor a que habilitar.
    NaoEstaHospedando,
    /// O banco do servidor não respondeu.
    BancoNaoRespondeu,
    /// O MOD existe e não passou na validação. O nome é da variante de
    /// recusa que o `seele-ffi` devolve, e quem escreve a frase é o `frases.js`.
    Recusado {
        /// O nome da recusa, de uma lista fechada. A recusa que leva números
        /// os traz depois do nome, no mesmo texto
        /// (`som-grande-demais-decodificado: …`, [`som_cabe_decodificado`]).
        motivo: String,
    },
    /// **O conjunto mudou desde que a tela abriu.**
    ///
    /// Outro operador, ou outro processo desta mesma máquina, aplicou um
    /// conjunto enquanto esta tela estava aberta com um rascunho em cima do
    /// conjunto antigo. Gravar assim mesmo apagaria a decisão do outro sem que
    /// ninguém soubesse — e as duas pessoas sairiam achando que a sua valeu.
    ConjuntoMudou {
        /// A identidade do conjunto que está de pé agora, para a tela recarregar
        /// e o rascunho ser refeito sobre ele.
        atual: String,
    },
}

/// Um MOD como a janela o desenha.
///
/// O `seele-ffi` entrega o que está em disco; o que esta camada acrescenta é a
/// única coisa que ele não pode saber — se o servidor **desta janela** o exige
/// agora. `flatten` para que a página leia um objeto só.
#[derive(Debug, serde::Serialize)]
struct ModNaTela {
    /// O que o FFI leu do disco.
    #[serde(flatten)]
    instalado: seele_ffi::mods::ModInstalado,
    /// Se o servidor desta janela o exige agora.
    enabled: bool,
    /// **Se é este MOD que pinta o servidor desta janela** (D-api-5; tarefa
    /// 17f.0 da F3): ligado, com a porta do tema, e o escolhido. Com dois MODs
    /// com a porta ligados, a gestão pergunta qual, e este é o que está
    /// marcado; a escolha vai por [`escolher_o_tema_do_servidor`].
    pinta: bool,
    /// Se, com essa exigência, quem não aceita já é barrado pela rede.
    ///
    /// **A metade que `enabled` sozinho não conta.** `enabled` responde só o
    /// que está gravado no banco desta janela; esta responde o que esse
    /// registro *faz* hoje na rede — e as duas podem discordar: um MOD pode
    /// estar `enabled: true` e ninguém ser barrado por ele, se o anúncio que o
    /// exige estiver dormente. Sem este campo, quem hospeda liga o interruptor
    /// e lê "exigido" numa tela que não tranca ninguém — o "produto sabe e não
    /// conta" que o `CLAUDE.md` deste repositório nomeia como o defeito mais
    /// caro daqui, cometido contra a própria pessoa que hospeda.
    ///
    /// **Com a versão 5 do protocolo as duas concordam**: o anúncio saiu da
    /// dormência em 14/09/2026 e este campo devolve verdadeiro para todo
    /// servidor que esta janela levanta. Ele não foi removido porque a pergunta
    /// é do produto e não do dia: a resposta volta a ser falsa na próxima
    /// variante que nascer adiantada, e a tela precisa continuar sabendo.
    exigencia_vale_na_rede: bool,
}

/// Todo MOD em `mods/`, com o que o servidor desta máquina tem ligado.
#[tauri::command]
async fn mods_instalados(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<Vec<ModNaTela>, FalhaNoMod> {
    let ligados = mods_ligados(&session).await?;
    let quem_pinta = quem_pinta_o_servidor(&session).await?;
    let pasta = config_dir(&app);

    let exigencia_vale_na_rede = seele_server::mods::anuncio::exigencia_vale_na_rede();
    Ok(seele_ffi::mods::listar_por_conteudo(&pasta)
        .into_iter()
        .map(|instalado| {
            // Ligado é **este pacote**, e não este MOD: dois conteúdos do mesmo
            // MOD podem estar no disco, e só um é o que o servidor exige.
            let enabled = ligados
                .iter()
                .any(|(id, hash)| *id == instalado.id && *hash == instalado.hash);
            ModNaTela {
                pinta: pinta_o_servidor(enabled, quem_pinta.as_deref(), &instalado.id),
                enabled,
                exigencia_vale_na_rede,
                instalado,
            }
        })
        .collect())
}

/// **Se este pacote é o que pinta o servidor desta janela** — o
/// [`ModNaTela::pinta`] (D-api-5; tarefa 17f.0 da F3).
///
/// O escolhido que o servidor guarda é um identificador, e a marca vai só no
/// pacote **ligado** dele: com dois conteúdos do mesmo MOD no disco, o outro
/// não pinta nada. Uma função pura para o teste de unidade ver o lado do
/// Rust: a bancada da gestão responde o `mods_instalados` por conta própria.
fn pinta_o_servidor(ligado: bool, quem_pinta: Option<&str>, id: &str) -> bool {
    ligado && quem_pinta == Some(id)
}

/// Um pacote guardado nesta máquina, como a manutenção local o desenha.
#[derive(Debug, serde::Serialize)]
struct PacoteNoCache {
    /// `autor/nome`, ou o nome da pasta quando o pacote foi recusado.
    id: String,
    /// A versão que o autor declara, ou vazio.
    version: String,
    /// O hash do conteúdo. É o nome da pasta e é o que a tela mostra cortado.
    hash: String,
    /// Quantos bytes ele ocupa. É o número que faz a manutenção ser uma
    /// decisão em vez de um palpite.
    bytes: u64,
    /// Se o servidor **desta janela** exige este pacote agora.
    ///
    /// Quando é verdadeiro, apagar é recusado. Ver [`apagar_pacote_do_cache`].
    exigido_aqui: bool,
    /// A API que este pacote pede, ou zero quando o manifesto não abriu.
    ///
    /// **É o número que diz o caminho de saída** — U10. Um pacote que este
    /// build não roda é uma de duas coisas: velho demais, e então é o pacote
    /// que precisa de uma versão nova; ou novo demais, e então é o aplicativo.
    /// Sem ele, a tela dizia `api-too-old` e parava aí.
    api: u32,
    /// Por que este pacote não serve, quando não serve.
    recusado: Option<String>,
}

/// Todo pacote no cache local, com tamanho e com quem o exige.
///
/// # Por que existe, e por que não é «configuração de MOD»
///
/// Plano de isolamento de 18/09: «gerenciar espaço do cache sem chamar isso de
/// configuração de MOD». O cache é endereçado por conteúdo, então cada versão
/// que passou por aqui deixou uma pasta — e nada nunca a tirava. Isso não é um
/// defeito do endereçamento: é o preço dele, e o preço precisava de uma tela.
///
/// A lista é do **disco**, e não do servidor: ela mostra o que ocupa lugar,
/// inclusive o que nenhum servidor exige e inclusive o que veio de um servidor
/// em que esta pessoa entrou como convidada.
///
/// # Errors
///
/// [`FalhaNoMod`] quando o banco do servidor hospedado não responde.
#[tauri::command]
async fn pacotes_no_cache(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<Vec<PacoteNoCache>, FalhaNoMod> {
    let ligados = mods_ligados(&session).await?;
    let pasta = config_dir(&app);
    Ok(seele_ffi::mods::listar_por_conteudo(&pasta)
        .into_iter()
        .map(|instalado| pacote_no_cache_de(&pasta, instalado, &ligados))
        .collect())
}

/// Um pacote do cache como a tela de MODs o desenha.
///
/// **A API é a que o manifesto do pacote declara**, a mesma que
/// [`habilitado_de`] grava quando ele é exigido: a tela e o banco não podem
/// discordar sobre qual API um pacote pede. Ver `a_api_do_mod_habilitado`.
fn pacote_no_cache_de(
    pasta: &str,
    instalado: seele_ffi::mods::ModInstalado,
    ligados: &[(String, String)],
) -> PacoteNoCache {
    PacoteNoCache {
        bytes: seele_ffi::mods::bytes_do_pacote(pasta, &instalado.hash),
        exigido_aqui: ligados
            .iter()
            .any(|(id, hash)| *id == instalado.id && *hash == instalado.hash),
        api: instalado.api,
        recusado: instalado.refused,
        id: instalado.id,
        version: instalado.version,
        hash: instalado.hash,
    }
}

/// Tira um pacote do cache local.
///
/// # O que ele recusa, e por quê
///
/// **Um pacote que o servidor desta janela exige não sai.** Plano de
/// isolamento de 18/09: «remoção de cache referenciado por servidor deve ser
/// impedida ou exigir desinstalação explícita naquele servidor». Apagá-lo
/// deixaria o servidor exigindo bytes que saíram do disco — e quem entrasse
/// leria «o servidor exige este MOD e ele não está instalado aqui» sem que
/// ninguém tivesse desligado nada.
///
/// O caminho para tirá-lo é o que já existe: desligar o MOD naquele servidor,
/// salvar, e então apagar. São dois atos porque são duas decisões.
///
/// **E ele não toca no que um MOD guardou.** Ver [`mods::apagar_do_cache`]:
/// bytes que se baixa de novo não podem levar junto o que ninguém tem como
/// recuperar.
///
/// # Errors
///
/// [`FalhaNoMod::Recusado`] com `exigido-por-este-servidor` quando o pacote
/// está ligado aqui, ou com o nome da falha de disco.
#[tauri::command]
async fn apagar_pacote_do_cache(
    app: AppHandle,
    session: State<'_, Session>,
    hash: String,
) -> Result<(), FalhaNoMod> {
    // A pergunta é feita **agora**, e não com o que a tela sabia: entre
    // desenhar a lista e apertar o botão, outro operador pode ter ligado este
    // pacote.
    let ligados = mods_ligados(&session).await?;
    if ligados.iter().any(|(_, ligado)| *ligado == hash) {
        return Err(FalhaNoMod::Recusado {
            motivo: "exigido-por-este-servidor".to_owned(),
        });
    }
    let config = config_dir(&app);
    for servidor in servidores::listar(&config) {
        let banco = servidor.caminho.as_ref().map_or_else(
            || std::path::Path::new(&config).join("seele.db"),
            |p| std::path::Path::new(&config).join(p),
        );
        if !banco.exists() {
            continue;
        }
        let db = seele_server::persistence::Persistence::open(
            &seele_server::persistence::Location::File(banco),
        )
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
        let usados = seele_server::persistence::mods::enabled(&db)
            .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
        if usados.iter().any(|m| m.hash == hash) {
            return Err(FalhaNoMod::Recusado {
                motivo: format!(
                    "Desligue este MOD no servidor {} antes de apagar.",
                    servidor.nome
                ),
            });
        }
    }
    mods::apagar_do_cache(std::path::Path::new(&config), &hash).map_err(|falha| {
        FalhaNoMod::Recusado {
            motivo: match falha {
                mods::FalhaAoApagarPacote::HashTorto => "hash-torto".to_owned(),
                mods::FalhaAoApagarPacote::NaoEstaAqui => "nao-esta-aqui".to_owned(),
                mods::FalhaAoApagarPacote::NaoApaguei(_) => "nao-apaguei".to_owned(),
            },
        }
    })
}

// --------------------------------------------- o executor nativo, de bancada
//
// Etapa E1. Três comandos e um evento, e eles existem para medir o que só o
// aplicativo de verdade mede: interação, descarte e impacto na voz.
//
// **São o caminho normal.** Eles nasceram atrás de `SEELE_EXECUTOR=quickjs`,
// enquanto o Worker de `blob:` ainda era o executor do produto e este era a
// bancada que o mediria. A medição terminou: o Worker herda a origem de quem o
// cria, e o que um MOD gravou nela sobreviveu ao encerramento do aplicativo.
//
// Um executor reprovado guardado como reserva é a reserva errada — se o nativo
// falhar, cair no outro é cair justamente no que não isola. A variável saiu, o
// Worker saiu, e o que sobrou é um caminho só.

/// O canal por onde as mensagens de um MOD nativo chegam à janela.
const EVENTO_DO_EXECUTOR: &str = "seele://mod-nativo";

/// O que a janela recebe quando um MOD nativo fala.
#[derive(Debug, Clone, serde::Serialize)]
struct AvisoDoMod {
    /// **Qual execução deste MOD.** Nunca se repete nesta janela.
    ///
    /// Sem ela, duas montagens do mesmo MOD dentro da mesma geração —
    /// recarregar um MOD é isso — falariam com a mesma voz, e a janela
    /// entregaria a resposta de uma à outra.
    instancia: u64,
    /// Em que sessão — a janela descarta o que não for a dela.
    geracao: u64,
    /// O executor confirmou que parou.
    ///
    /// **Sem carga, e é o ponto.** Este aviso diz «há o que colher» e nada
    /// mais: um evento que carregasse a mensagem acumularia bytes na janela
    /// antes de qualquer `callback` rodar, que é o que a revisão mediu.
    parou: bool,
}

/// O último motivo de carga recusada dito por MOD, com a geração em que foi
/// dito, como o registro o leva: a chave por [`id_no_registro`] e o motivo por
/// [`motivo_no_registro`], os dois com teto — ver [`recusa_de_carga_dita`].
type RecusasDeCarga = Mutex<std::collections::HashMap<String, (u64, String)>>;

/// **Diz no registro por que um MOD não carregou** — quando o motivo ou a
/// sessão muda —, e devolve a recusa intacta.
///
/// `codigo_do_mod` e `mod_nativo_reservar` recusavam sem uma linha. A janela
/// anotava «não carregou» sem o motivo num mapa em memória, e o `seele.log` não
/// tinha nada: o pacote de outro MOD, o cliente que não está no pacote, o
/// executor que não subiu.
///
/// **Uma vez por motivo, e não uma por tentativa.** A janela tenta de novo a
/// cada quatro segundos um MOD que não subiu (`carregarMods`), e a recusa é
/// quase sempre a mesma: dita a cada vez, seriam novecentas linhas iguais por
/// hora. Guardado o último motivo por id, com a geração, sai a primeira
/// recusa, a primeira de cada sessão e cada mudança; a carga que dá certo o
/// esquece ([`carga_aceita`]), e a recusa que vier depois dela é dita de novo.
/// Sem a memória — um cadeado envenenado —, a recusa é dita: repetir é melhor
/// que calar.
///
/// WARN com o id e o motivo, menos `sessao-encerrada`, que vai em DEBUG e não
/// mexe na memória: uma carga que não subiu porque a pessoa saiu é o desfecho
/// certo, e quem a conta é `comandos_de_geracao_morta`.
fn recusa_de_carga_dita(
    ditas: &RecusasDeCarga,
    geracao: u64,
    mod_id: &str,
    falha: FalhaNoMod,
) -> FalhaNoMod {
    let motivo = motivo_de(&falha);
    let id = id_no_registro(mod_id);
    let no_registro = motivo_no_registro(motivo);
    if motivo == "sessao-encerrada" {
        tracing::debug!(mod_id = %id, motivo = %no_registro, "MOD não carregou: a sessão acabou");
        return falha;
    }
    // Dita quando não havia nada guardado, quando o guardado é de outra
    // geração, ou quando o motivo mudou.
    let dita = (geracao, no_registro);
    let mudou = ditas.lock().map_or(true, |mut ditas| {
        ditas.insert(id.clone(), dita.clone()).as_ref() != Some(&dita)
    });
    let (_, no_registro) = dita;
    if mudou {
        tracing::warn!(mod_id = %id, motivo = %no_registro, "MOD não carregou");
    }
    falha
}

/// **Uma carga deu certo**: o motivo guardado deste MOD é esquecido, e a
/// próxima recusa dele é dita mesmo que seja a de antes.
fn carga_aceita(ditas: &RecusasDeCarga, mod_id: &str) {
    if let Ok(mut ditas) = ditas.lock() {
        ditas.remove(&id_no_registro(mod_id));
    }
}

/// **O desfecho de uma carga, dito**: aceita, esquece o motivo guardado
/// ([`carga_aceita`]); recusada, passa por [`recusa_de_carga_dita`]. Devolve o
/// desfecho intacto.
///
/// Uma função, e não um `match` em cada comando, para que os dois braços não
/// possam ser trocados num lugar só: o `carga_aceita` no braço da recusa
/// esqueceria o motivo a cada tentativa, e a mesma recusa voltaria a sair a
/// cada quatro segundos.
fn carga_dita<T>(
    ditas: &RecusasDeCarga,
    geracao: u64,
    mod_id: &str,
    carga: Result<T, FalhaNoMod>,
) -> Result<T, FalhaNoMod> {
    match carga {
        Ok(valor) => {
            carga_aceita(ditas, mod_id);
            Ok(valor)
        }
        Err(falha) => Err(recusa_de_carga_dita(ditas, geracao, mod_id, falha)),
    }
}

/// **Reserva a identidade de um MOD nativo, sem rodar nada** — etapa 1 de 2.
///
/// # Por que duas etapas
///
/// A revisão de `fd1de3a`: «enquanto `numero` é `null`, o listener aceita
/// eventos pelo nome do MOD e pela geração. Uma instância anterior do mesmo MOD
/// pode emitir nessa janela». A reprodução mostrou o pior caso — a mensagem de
/// uma instância antiga aceita, a resposta perdida, e o `parou` dela
/// confirmando o encerramento da nova.
///
/// Consertar ignorando os eventos iniciais perderia as primeiras mensagens
/// legítimas. Consertar **dando o número antes de o MOD poder falar** não
/// perde nenhuma: entre a reserva e a ativação não existe fala para ouvir.
///
/// # Errors
///
/// [`FalhaNoMod`] quando o executor não está ligado, quando a geração já
/// acabou, quando o pacote não é deste MOD, ou quando a thread não sobe.
#[tauri::command]
fn mod_nativo_reservar(
    app: AppHandle,
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    hash: String,
) -> Result<u64, FalhaNoMod> {
    // **Dita no registro quando não reserva, e esquecida quando reserva**: a
    // janela tenta de novo a cada quatro segundos, e a recusa sai uma vez por
    // motivo — ver [`carga_dita`].
    carga_dita(
        &session.recusas_de_carga,
        geracao,
        &id,
        reservar_no_executor(&app, &session, geracao, &id, hash),
    )
}

/// O corpo de [`mod_nativo_reservar`], com a recusa devolvida sem ser dita:
/// quem a diz, uma vez por motivo, é o comando.
fn reservar_no_executor(
    app: &AppHandle,
    session: &Session,
    geracao: u64,
    id: &str,
    hash: String,
) -> Result<u64, FalhaNoMod> {
    // Recusada **e contada** em `comandos_de_geracao_morta`, pela mesma regra
    // de todo comando de MOD, como a de `codigo_do_mod`: as duas são a carga
    // atrasada, e o doc de `recusa_de_carga_dita` diz que é esse contador que
    // a conta.
    session.recusar_se_morta(geracao)?;
    // O mesmo caminho de sempre para chegar ao código: a conferência de hash e
    // de identidade mora lá, e um segundo caminho seria uma segunda regra. Lido
    // agora para que a ativação não possa falhar por causa do disco.
    let (codigo, api_declarada) = ler_codigo_do_mod(app, session, geracao, id, &hash)?;

    let mut executor =
        executor::ExecutorQuickJs::novo(executor::Limites::default()).map_err(|_| {
            FalhaNoMod::Recusado {
                motivo: "executor-nao-subiu".to_owned(),
            }
        })?;
    let Some((recebedor, fila)) = executor.escutar() else {
        return Err(FalhaNoMod::Recusado {
            motivo: "executor-ja-escutado".to_owned(),
        });
    };

    // **Registrar dentro do mesmo cadeado que confere a revogação.** Fora dele,
    // entre conferir e inserir cabe uma desmontagem inteira — e o executor
    // ficaria órfão na tabela de uma sessão que já acabou.
    // **A instância nasce com o fonte dentro.** Ele morava numa tabela ao
    // lado, inserido depois de este cadeado ser liberado — e a revogação que
    // caísse no meio apagava um fonte que ainda não existia, deixando o comando
    // inseri-lo depois do descarte. Guardado aqui, ele é descartado pelo mesmo
    // ato que descarta a instância.
    let numero = registrar_reserva(
        &session.mods_nativos,
        &|| session.geracao_vale(geracao),
        &session.comandos_de_geracao_morta,
        InstanciaNativa {
            id: id.to_owned(),
            geracao,
            hash,
            estado: EstadoNativo::Reservada,
            executor,
            pendentes: std::collections::VecDeque::new(),
            bytes_pendentes: 0,
            avisos: std::collections::VecDeque::new(),
            avisos_perdidos: 0,
            parou: false,
            codigo: Some(codigo),
            api_declarada,
            avisado: false,
        },
    )?;

    // A bomba **não emite carga**: ela guarda a fala na instância e avisa que
    // há o que colher. Ver `FalaPendente` para o porquê.
    let janela = app.clone();
    let quem = id.to_owned();
    if std::thread::Builder::new()
        .name("mod-nativo-bomba".into())
        .spawn(move || bombear(&janela, &recebedor, &fila, numero, geracao, &quem))
        .is_err()
    {
        // **Rollback completo.** Sem isto, a instância ficava na tabela com o
        // fonte dentro e sem ninguém para escutá-la — e só uma ativação futura,
        // que pode nunca vir, a tiraria de lá.
        if let Ok(mut vivos) = session.mods_nativos.lock() {
            vivos.encerrar(numero);
            vivos.descartar(numero);
        }
        return Err(FalhaNoMod::Recusado {
            motivo: "a-bomba-nao-subiu".to_owned(),
        });
    }

    tracing::info!(mod_id = %id, geracao, instancia = numero, "MOD reservado no executor nativo");
    Ok(numero)
}

/// **Libera a execução de um MOD já reservado** — etapa 2 de 2.
///
/// A revogação é conferida **aqui também**: entre a reserva e a ativação a
/// sessão pode ter acabado, e ativar depois disso seria admitir efeito de uma
/// geração morta.
///
/// # Errors
///
/// [`FalhaNoMod`] quando a geração já acabou, quando a instância não está
/// reservada, ou quando o motor não aceita o código.
#[tauri::command]
fn mod_nativo_ativar(
    session: State<'_, Session>,
    geracao: u64,
    instancia: u64,
) -> Result<(), FalhaNoMod> {
    liberar_reserva(
        &session.mods_nativos,
        &|| session.geracao_vale(geracao),
        &session.comandos_de_geracao_morta,
        geracao,
        instancia,
    )?;
    tracing::info!(instancia, geracao, "MOD ativado no executor nativo");
    Ok(())
}

/// **Fecha a geração e varre o que era dela.**
///
/// **Fechar a geração e varrer o que era dela é um ato só**, sob o cadeado das
/// instâncias. Não é o arranjo original: a geração subia **fora** do cadeado,
/// antes de tomá-lo. Aquela ordem era correta — quem registrasse antes era
/// varrido, quem registrasse depois via a geração morta —, mas correta de um
/// jeito que **não dá para provar**: inverter as duas linhas abre uma janela de
/// poucas instruções entre soltar o cadeado e incrementar, e nenhuma disputa
/// por thread cai nela, porque acordar de um mutex custa mais do que ela dura.
/// Medi: com a ordem invertida de propósito, trinta rodadas com quatro
/// registradores disputando o cadeado passaram todas.
///
/// Debaixo do cadeado não há ordem a inverter. Registrar e revogar passam a se
/// excluir inteiros, e o que sobra é verificável: enquanto alguém registra, a
/// geração **não muda** — e é isso que o guarda mede.
fn revogar_em(geracao: &std::sync::atomic::AtomicU64, mods: &std::sync::Mutex<ModsNativos>) -> u64 {
    let Ok(mut vivos) = mods.lock() else {
        // Sem cadeado ainda se fecha a geração: uma tabela envenenada não pode
        // virar uma sessão que continua admitindo efeito.
        return geracao
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_add(1);
    };
    let morta = geracao.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    // O fonte de uma reserva sai junto: ele mora **dentro** da instância, e
    // `soltar` o larga pelo mesmo ato que larga o resto. Antes ele morava numa
    // tabela ao lado, e uma revogação entre as duas inserções deixava um fonte
    // órfão.
    vivos.revogar_geracao(morta);
    morta.saturating_add(1)
}

/// **Registra a reserva sob o mesmo cadeado que confere a revogação.**
///
/// Extraída do comando para que a corrida entre registrar e revogar possa ser
/// provada com threads de verdade, e não descrita em prosa. A ordem que a torna
/// segura mora em dois lugares e só funciona nos dois: [`Session::revogar`]
/// incrementa a geração **antes** de tomar este cadeado, e aqui a geração é
/// conferida **depois** de tomá-lo. Quem chega primeiro insere e é varrido pela
/// revogação; quem chega depois já vê a geração morta e não insere.
///
/// A instância chega inteira — **com o fonte dentro** —, e é isso que faz a
/// revogação não ter onde cair no meio: não há um segundo `insert`.
///
/// A recusa por geração morta é contada em `mortos` — o
/// `comandos_de_geracao_morta` da sessão —, como a do começo de
/// [`reservar_no_executor`] e a de [`liberar_reserva`], logo abaixo.
fn registrar_reserva(
    mods: &std::sync::Mutex<ModsNativos>,
    geracao_vale: &dyn Fn() -> bool,
    mortos: &std::sync::atomic::AtomicU64,
    instancia: InstanciaNativa,
) -> Result<u64, FalhaNoMod> {
    let mut vivos = mods.lock().map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    if !geracao_vale() {
        mortos.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        instancia.executor.pedir_encerramento();
        return Err(FalhaNoMod::Recusado {
            motivo: "sessao-encerrada".to_owned(),
        });
    }
    Ok(vivos.guardar(instancia))
}

/// **Libera a execução de uma reserva**, sob o cadeado, conferindo a revogação.
///
/// Extraída pela mesma razão que [`registrar_reserva`]: a ativação também é uma
/// corrida contra a saída da sessão, e uma prova dela não pode precisar de um
/// `AppHandle`.
///
/// A recusa por geração morta é contada em `mortos`, como a da reserva, e
/// dita com a linha DEBUG de [`Session::confere_geracao`], como a dos outros
/// comandos de MOD — com a instância no lugar da geração de pé, que esta
/// função não conhece.
fn liberar_reserva(
    mods: &std::sync::Mutex<ModsNativos>,
    geracao_vale: &dyn Fn() -> bool,
    mortos: &std::sync::atomic::AtomicU64,
    geracao: u64,
    instancia: u64,
) -> Result<(), FalhaNoMod> {
    let mut vivos = mods.lock().map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    if !geracao_vale() {
        mortos.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        vivos.encerrar(instancia);
        // A linha sai com o cadeado solto: com ele na mão, uma escrita lenta
        // no disco seguraria toda a supervisão dos MODs.
        drop(vivos);
        tracing::debug!(
            geracao,
            instancia,
            "comando recusado: a sessão dele já acabou"
        );
        return Err(FalhaNoMod::Recusado {
            motivo: "sessao-encerrada".to_owned(),
        });
    }
    let alvo = vivos
        .vivos
        .get_mut(&instancia)
        .ok_or(FalhaNoMod::Recusado {
            motivo: "instancia-nao-esta-reservada".to_owned(),
        })?;
    if alvo.geracao != geracao || alvo.estado != EstadoNativo::Reservada {
        return Err(FalhaNoMod::Recusado {
            motivo: "instancia-nao-esta-reservada".to_owned(),
        });
    }
    // O fonte sai da instância aqui: depois de rodar, ele não serve mais, e
    // guardá-lo seria guardar uma cópia do MOD por sessão.
    let codigo = alvo.codigo.take().ok_or(FalhaNoMod::Recusado {
        motivo: "nada-reservado".to_owned(),
    })?;
    let api = alvo.api_declarada;
    alvo.executor
        .iniciar_com_api(&codigo, api)
        .map_err(|_| FalhaNoMod::Recusado {
            motivo: "executor-nao-aceitou-o-codigo".to_owned(),
        })?;
    alvo.estado = EstadoNativo::Ativa;
    Ok(())
}

/// O que sobra de uma fala depois de assentada.
#[derive(Debug, PartialEq, Eq)]
enum Assentada {
    /// Não era fala: diagnóstico, ou coisa que não chega à janela.
    Nada,
    /// Guardada na instância; `avisar` diz se a janela ainda não foi chamada.
    Guardada { avisar: bool },
    /// A bomba desce: o MOD parou, ou a supervisão sumiu debaixo dela.
    Fim { avisar: bool },
}

/// **O que a bomba faz com uma fala**, sem tocar na janela.
///
/// Extraída do laço para que a saturação possa ser medida pelo caminho real:
/// produzir erros de verdade, não assentá-los, e contar cada ponto de retenção.
/// Testar isto reimplementando o laço provaria a reimplementação.
fn assentar_fala(
    mods: &std::sync::Mutex<ModsNativos>,
    geracao_vale: &dyn Fn() -> bool,
    fila: &std::sync::Arc<executor::Fila>,
    numero: u64,
    fala: executor::ParaOFora,
    quem: &str,
) -> Assentada {
    let (tipo, corpo, parou) = match fala {
        executor::ParaOFora::Mensagem(json) => ("mensagem", json, false),
        executor::ParaOFora::Falhou(motivo) => ("falhou", motivo, false),
        executor::ParaOFora::Interrompido => ("interrompido", String::new(), false),
        executor::ParaOFora::Parou => {
            // **INFO, e não DEBUG**: é a outra metade da linha «MODs nativos
            // mandados parar» da saída do app, e o filtro padrão não escreve
            // DEBUG. A instância, e não a geração: o número não se repete nesta
            // janela, e a linha da reserva o liga à geração.
            tracing::info!(mod_id = %quem, instancia = numero, "MOD nativo parou");
            ("parou", String::new(), true)
        }
        executor::ParaOFora::Diagnostico { relogios } => {
            tracing::debug!(mod_id = %quem, relogios, "diagnóstico de MOD nativo");
            return Assentada::Nada;
        }
        executor::ParaOFora::Console {
            nivel,
            texto,
            suprimidas,
        } => {
            // **Registro, e não fala.** Não entra em fila da janela, não
            // reserva crédito e não toma o cadeado da supervisão: a linha vai
            // para o `seele.log` e acabou.
            registrar_console_do_mod(quem, nivel, &texto, suprimidas);
            return Assentada::Nada;
        }
    };
    tracing::debug!(mod_id = %quem, tipo, "fala de MOD nativo");

    let Ok(mut vivos) = mods.lock() else {
        return Assentada::Fim { avisar: false };
    };
    if parou {
        // **O caminho de parada é independente do crédito de dados.** Ele não
        // entra em fila nenhuma, não ocupa byte, e não espera a janela colher:
        // resolve a supervisão na hora. E avisa sempre, fora de cota.
        vivos.confirmar_parada(numero);
        return Assentada::Fim { avisar: true };
    }
    if !geracao_vale() {
        // **Depois da revogação, a fala é descartada aqui.**
        //
        // Ela não tem para onde ir: `colher` recusa a geração que saiu, e
        // guardá-la reteria bytes e a instância para sempre esperando uma
        // janela que não vai mais pedir.
        // **Contada e dita.** A linha acima diz que a fala chegou; sem esta,
        // «chegou» e «foi guardada» eram indistinguíveis no registro — e uma
        // fala descartada aqui deixa o MOD esperando para sempre a resposta
        // que ela carregava. Custou-me seis execuções descobrir isso.
        tracing::warn!(
            mod_id = %quem,
            numero,
            tipo,
            "fala de MOD descartada: a geração dela não é mais a de pé"
        );
        devolver_credito(fila, tipo, corpo.len());
        vivos.contar_descartada(numero);
        return Assentada::Guardada { avisar: false };
    }
    let Some(instancia) = vivos.achar_mut(numero) else {
        // Sem instância não há onde guardar — e sem esta linha, sem rastro.
        tracing::warn!(
            mod_id = %quem,
            numero,
            tipo,
            "fala de MOD descartada: a instância não está mais na supervisão"
        );
        devolver_credito(fila, tipo, corpo.len());
        return Assentada::Guardada { avisar: false };
    };
    instancia.guardar_fala(tipo, corpo);
    // **No máximo um aviso pendente por instância.** A versão anterior emitia
    // um evento por fala, inclusive pelas que ela agregava ou descartava — e um
    // evento sem corpo ainda ocupa memória enquanto a janela não o processa.
    Assentada::Guardada {
        avisar: instancia.precisa_avisar(),
    }
}

/// **O que um MOD escreveu com `console`, no `seele.log`.**
///
/// Com o id do MOD num campo próprio e o nível que ele escolheu: `warn` é
/// WARN, `error` é ERROR, `log` e `info` são INFO, e `debug` é DEBUG — que o
/// filtro padrão não escreve, como não escreve o DEBUG do próprio produto.
///
/// Em 23/09, descobrir que o avatar do PERFIS carregava levou uma hora de
/// medição e um reinício com `RUST_LOG=debug`.
///
/// É também por aqui, em ERROR, que chega o erro que o MOD não pegou: o
/// executor o põe no canal como uma linha de `console` (`o MOD lançou: …`,
/// `erro num temporizador: …`, `erro num ouvinte de evento: …`, `promessa
/// rejeitada sem tratamento: …`, e, depois das rejeições de uma volta que
/// passaram do teto da espera, `e mais N rejeições sem tratamento além da
/// espera`), pelo mesmo balde. E a volta que o produto
/// parou por um teto, com a frase do teto (`esta volta do MOD passou do prazo
/// de 500 ms, e o produto a interrompeu (o MOD continua de pé): …`), e não
/// como «o MOD lançou».
///
/// **O texto vai em `?`, e não solto na frase.** Ele é de um terceiro, e uma
/// quebra de linha nele escreveria no registro uma segunda linha com a cara de
/// uma linha do produto. Em `?` ela sai escapada, entre aspas.
///
/// `suprimidas` é quantas linhas o balde do executor segurou antes desta. Só
/// aparece quando não é zero: é o registro dizendo que houve mais do que ele
/// mostra.
fn registrar_console_do_mod(
    quem: &str,
    nivel: executor::NivelDoConsole,
    texto: &str,
    suprimidas: u32,
) {
    use executor::NivelDoConsole as Nivel;
    let suprimidas = (suprimidas > 0).then_some(suprimidas);
    match nivel {
        Nivel::Depuracao => {
            tracing::debug!(mod_id = %quem, texto = ?texto, suprimidas, "console do MOD");
        }
        Nivel::Informacao => {
            tracing::info!(mod_id = %quem, texto = ?texto, suprimidas, "console do MOD");
        }
        Nivel::Aviso => {
            tracing::warn!(mod_id = %quem, texto = ?texto, suprimidas, "console do MOD");
        }
        Nivel::Erro => {
            tracing::error!(mod_id = %quem, texto = ?texto, suprimidas, "console do MOD");
        }
    }
}

/// A bomba de uma instância: guarda o que o MOD fala e avisa a janela.
///
/// **Ela não emite carga.** `emit` volta sem que o handler da janela tenha
/// rodado — a revisão mediu isso no Tauri deste checkout —, então devolver o
/// crédito ali seria dar por consumido o que ninguém leu. Aqui a fala fica na
/// instância, o crédito fica retido, e o aviso que sai é só «há o que colher».
fn bombear(
    janela: &AppHandle,
    recebedor: &std::sync::mpsc::Receiver<executor::ParaOFora>,
    fila: &std::sync::Arc<executor::Fila>,
    numero: u64,
    geracao: u64,
    quem: &str,
) {
    while let Ok(fala) = recebedor.recv() {
        let Some(sessao) = janela.try_state::<Session>() else {
            return;
        };
        let assentada = assentar_fala(
            &sessao.mods_nativos,
            &|| sessao.geracao_vale(geracao),
            fila,
            numero,
            fala,
            quem,
        );
        let (avisar, parou) = match assentada {
            Assentada::Nada => continue,
            Assentada::Guardada { avisar } => (avisar, false),
            Assentada::Fim { avisar } => (avisar, true),
        };

        // O aviso é **sem carga**: ele não pode acumular bytes na janela, e por
        // isso um MOD que fala muito não enche memória de ninguém antes de
        // alguém ler. E é **agregado**: um por instância até a janela drenar.
        if avisar {
            let _ = janela.emit(
                EVENTO_DO_EXECUTOR,
                AvisoDoMod {
                    instancia: numero,
                    geracao,
                    parou,
                },
            );
        }
        if parou {
            return;
        }
    }
}

/// Devolve o crédito **na cota de quem o tomou**.
///
/// Mensagem e aviso reservam em pools diferentes, e por isso devolvem em pools
/// diferentes. Os ramos de descarte da bomba devolviam tudo pela conta das
/// mensagens — inclusive erros que nunca tinham reservado nada —, e com
/// mensagens ainda pendentes isso aparecia como crédito a menos.
fn devolver_credito(fila: &std::sync::Arc<executor::Fila>, tipo: &str, bytes: usize) {
    if tipo == "mensagem" {
        fila.tirar(bytes);
    } else {
        fila.tirar_aviso();
    }
}

/// **Colhe o que um MOD falou, e é a colheita que devolve o crédito.**
///
/// A janela chama isto ao ser avisada. Enquanto ela não chamar, a fala fica na
/// instância e o crédito fica retido — que é o que faz o teto valer até o
/// consumo, e não até o `emit`.
///
/// # Errors
///
/// [`FalhaNoMod`] quando a geração já acabou ou a instância não existe.
#[tauri::command]
fn mod_nativo_colher(
    session: State<'_, Session>,
    geracao: u64,
    instancia: u64,
    limite: usize,
) -> Result<Vec<FalaPendente>, FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    let mut vivos = session
        .mods_nativos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    let alvo = vivos.achar_mut(instancia).ok_or(FalhaNoMod::Recusado {
        motivo: "instancia-nao-esta-de-pe".to_owned(),
    })?;
    if alvo.geracao != geracao {
        return Err(FalhaNoMod::Recusado {
            motivo: "instancia-de-outra-geracao".to_owned(),
        });
    }
    let colhidas = alvo.colher_falas(limite);
    // **A colheita é dita, com quanto sobrou.** A bomba já dizia cada fala que
    // guardava; sem o outro lado, «guardada» e «entregue» eram a mesma linha no
    // registro — e é entre as duas que uma fala pode ficar parada esperando um
    // aviso que não vem.
    tracing::debug!(
        instancia,
        colhidas = colhidas.len(),
        restam = alvo.pendentes.len(),
        avisos = alvo.avisos.len(),
        "falas colhidas pela janela"
    );

    // Uma que já tinha parado e agora esvaziou sai da supervisão.
    if alvo.parou && alvo.pendentes.is_empty() && alvo.avisos.is_empty() {
        vivos.confirmar_parada(instancia);
    }
    Ok(colhidas)
}

/// Entrega a um MOD nativo a resposta de um pedido que ele fez.
///
/// # Errors
///
/// [`FalhaNoMod`] quando a geração já acabou ou a instância não está ativa.
#[tauri::command]
fn mod_nativo_entregar(
    session: State<'_, Session>,
    geracao: u64,
    instancia: u64,
    json: String,
) -> Result<(), FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    let vivos = session
        .mods_nativos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    // **Pelo número da instância, e só entre as vivas.** Uma que está
    // encerrando não recebe: ela está em `Encerrando`, e admitir efeito ali é o
    // que o §5 do contrato proíbe.
    let alvo = vivos.vivos.get(&instancia).ok_or(FalhaNoMod::Recusado {
        motivo: "instancia-nao-esta-viva".to_owned(),
    })?;
    if alvo.geracao != geracao || alvo.estado != EstadoNativo::Ativa {
        return Err(FalhaNoMod::Recusado {
            motivo: "instancia-nao-esta-ativa".to_owned(),
        });
    }
    // **Toda recusa é dita, com o tamanho junto.** Uma entrega recusada deixa
    // quem perguntou esperando para sempre — `SeeleMods.request` é uma promessa
    // e ela não rejeita sozinha —, e sem o número não dá para saber se foi a
    // fila cheia ou a mensagem grande.
    alvo.executor.entregar(&json).map_err(|motivo| {
        tracing::warn!(
            instancia,
            geracao,
            bytes = json.len(),
            motivo,
            "entrega a um MOD nativo recusada"
        );
        FalhaNoMod::Recusado {
            motivo: "executor-nao-aceitou".to_owned(),
        }
    })
}

/// Pede a parada de um MOD nativo, ou de todos os desta sessão.
///
/// **Pede, e não remove.** Quem sai da supervisão é quem confirmou. Removendo
/// aqui, o diagnóstico diria zero com o motor ainda parando — que é a diferença
/// entre «encerrou» e «mandei encerrar».
///
/// # Errors
///
/// Nunca: pedir a parada do que já está parando é o desfecho normal de quem
/// apertou duas vezes.
#[tauri::command]
fn mod_nativo_encerrar(
    session: State<'_, Session>,
    instancia: Option<u64>,
) -> Result<(), FalhaNoMod> {
    let Ok(mut vivos) = session.mods_nativos.lock() else {
        return Ok(());
    };
    match instancia {
        Some(numero) => vivos.encerrar(numero),
        None => {
            let todos: Vec<u64> = vivos.vivos.keys().copied().collect();
            for numero in todos {
                vivos.encerrar(numero);
            }
        }
    }
    Ok(())
}

/// Liga um MOD no servidor que este processo hospeda./// Liga um MOD no servidor que este processo hospeda.
async fn habilitar_mod(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    hash: String,
) -> Result<(), FalhaNoMod> {
    // Por hash: dois pacotes do mesmo MOD podem estar no disco, e quem liga
    // sabe quais bytes quer — ler por identificador pegaria «o que estiver
    // lá», que pode ser o que outro servidor desta máquina baixou depois.
    let instalado = seele_ffi::mods::ler_por_hash(&config_dir(&app), &hash)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
    if instalado.id != id {
        return Err(FalhaNoMod::Recusado {
            motivo: "conteudo-de-outro-mod".to_owned(),
        });
    }

    let persistence = persistence_do_mod(&session)?;
    let persistence = persistence.lock().await;
    seele_server::persistence::mods::enable(&persistence, &habilitado_de(instalado))
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)
}

/// A linha do banco em que um pacote instalado vira ao ser exigido aqui.
///
/// **Uma construção só, para os dois caminhos que gravam**: `habilitar_mod` e
/// `aplicar_conjunto_de_mods`. Os campos de uma vez, e não só os três de
/// identidade: o que a tela de aceite de quem entra vai ler — repositório,
/// alcance declarado, e se o MOD roda na máquina de quem hospeda — é gravado
/// **no mesmo ato** que grava o hash, e por isso descreve exatamente os bytes que
/// o hash cobre (migração 12).
///
/// **A API é a que o manifesto do pacote declara** (migração 14; D-fio-7), e não
/// a deste build: é ela que o anúncio diz. Ver `a_api_do_mod_habilitado`.
fn habilitado_de(
    instalado: seele_ffi::mods::ModInstalado,
) -> seele_server::persistence::mods::EnabledMod {
    seele_server::persistence::mods::EnabledMod {
        id: instalado.id,
        version: instalado.version,
        hash: instalado.hash,
        repo: instalado.repo,
        reach: instalado.reach,
        server_half: instalado.server,
        api: instalado.api,
    }
}

/// Desliga um MOD. Os dados dele ficam — ADR 0045.
async fn desabilitar_mod(session: State<'_, Session>, id: String) -> Result<(), FalhaNoMod> {
    let persistence = persistence_do_mod(&session)?;
    let persistence = persistence.lock().await;
    seele_server::persistence::mods::disable(&persistence, &id)
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)
}

/// Muda o conjunto deste servidor **e** registra o sim desta máquina a ele.
///
/// **A04 da auditoria de 17/09.** `habilitar_mod` gravava a exigência no banco
/// do servidor e não registrava consentimento nenhum no cliente local. O
/// servidor então encerra as conexões cujo conjunto aceito mudou — sem
/// distinguir quem fez a mudança —, e na entrada seguinte quem acabara de
/// ligar o MOD via a tela genérica de aceite, para decidir de novo o que tinha
/// acabado de decidir.
///
/// Instalar, exigir num servidor e consentir com execução nesta máquina são
/// três decisões tecnicamente diferentes. Duas delas são da **mesma pessoa no
/// mesmo instante**, e expor a separação interna como duas perguntas iguais é
/// cobrar dela o preço de um detalhe de implementação.
///
/// # O que isto **não** é
///
/// Não é isenção por papel. Não há atalho por «é localhost», «é administrador»
/// ou «é o dono» — nada aqui olha para quem pediu. O que se registra é a
/// decisão que a pessoa acabou de tomar, **para o conjunto exato que resulta
/// dela** e para o endereço desta sessão. Um servidor alterado depois, por
/// outro processo ou outro operador, produz outro conjunto, e outro conjunto
/// volta a perguntar — que é a proteção que o ADR 0045 existe para dar.
///
/// Sem sessão não há endereço a que prender um sim, e aí só a exigência é
/// gravada. É o caso de uma janela que hospeda sem ter entrado, e ele não é
/// erro.
///
/// # Errors
///
/// [`FalhaNoMod`] quando o MOD não serve, quando esta janela não hospeda, ou
/// quando o banco não responde.
/// **Não é mais comando da janela.** Ele existiu para o botão de cada linha,
/// que o rascunho e o SALVAR substituíram: a tela não muda mais o conjunto um
/// MOD por vez, porque cada mudança acordava o anúncio e derrubava a sessão.
///
/// Fica como função porque `instalar_mod_do_catalogo` a usa para reaplicar um
/// MOD que **já era exigido** depois de atualizá-lo — ali a exigência não
/// muda de conjunto por escolha de ninguém, ela passa a descrever os bytes que
/// acabaram de chegar.
async fn aplicar_mod(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    hash: String,
    ligar: bool,
) -> Result<String, FalhaNoMod> {
    if ligar {
        habilitar_mod(app.clone(), session.clone(), id, hash).await?;
    } else {
        desabilitar_mod(session.clone(), id).await?;
    }

    // A identidade do conjunto **resultante**, lida do banco depois da mudança
    // e pela mesma função que o anúncio usa. Calculá-la aqui de outro jeito
    // seria uma segunda definição do que é «o mesmo conjunto», e as duas
    // discordariam no dia em que um campo entrasse no anúncio.
    let persistence = persistence_do_mod(&session)?;
    let identidade = {
        let persistence = persistence.lock().await;
        seele_server::mods::anuncio::conjunto_exigido(&persistence)
            .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
            .identidade
    };

    // `endereco_da_sessao` e não `alvo`: o segundo é «onde voltar» e exclui um
    // servidor hospedado aqui, que é justamente o caso em que quem liga o MOD é
    // quem hospeda. Ver o campo. E só quando a sessão é com o servidor daqui
    // (lote X, rodada 1): ver `gravar_o_sim_de_quem_hospeda`.
    gravar_o_sim_de_quem_hospeda(&config_dir(&app), &session, &identidade)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
    Ok(identidade)
}

/// Grava o sim de quem hospeda ao conjunto que acabou de aplicar, **só** quando
/// a sessão é com o servidor guardado que esta janela hospeda.
///
/// Há dois gravadores: [`aplicar_mod`] (a atualização pela gestão, QA-01) e
/// [`aplicar_conjunto_de_mods`] (o SALVAR). Os dois gravavam
/// `aceitar(endereco_da_sessao, identidade do servidor daqui)`, fosse a
/// sessão com quem fosse. `Aceites::guardar` substitui o sim por alvo, então,
/// com a sessão em outro servidor S, o sim que a pessoa deu a S era trocado
/// por uma identidade que não descreve S:
/// - S perguntava de novo;
/// - quando o conjunto de S era igual ao daqui, S **não** perguntava, contra o
///   ADR 0045;
/// - a lista de aceites mostrava um sim a S que ninguém deu.
///
/// **O portão é [`onde_estou_na`]**, a única resposta a «estou no meu
/// servidor?», e ela é pela chave: o sim vai ao endereço da sessão quando a
/// impressão que ela conferiu é a do guardado, entre por onde ela entrar.
/// Fora dele nada é gravado, nem na sala, que não tem MODs, nem sem guardado
/// de pé.
fn gravar_o_sim_de_quem_hospeda(
    config: &str,
    session: &Session,
    identidade: &str,
) -> Result<(), String> {
    let alvo = session
        .endereco_da_sessao
        .lock()
        .ok()
        .and_then(|a| a.clone());
    gravar_o_sim_se_for_o_guardado(config, alvo.as_deref(), onde_estou_na(session), identidade)
}

/// [`gravar_o_sim_de_quem_hospeda`] sem a sessão: o endereço dela e a
/// resposta de [`onde_estou_na`]. Separada para os testes, que não sobem
/// hospedagem nem conexão.
fn gravar_o_sim_se_for_o_guardado(
    config: &str,
    endereco_da_sessao: Option<&str>,
    onde: OndeEstou,
    identidade: &str,
) -> Result<(), String> {
    match endereco_da_sessao {
        Some(alvo) if onde == OndeEstou::Guardado => {
            seele_ffi::mods::aceitar(config, alvo, identidade)
        }
        _ => Ok(()),
    }
}

/// **O sim de quem hospeda vai só ao servidor guardado** — lote X da F5,
/// ADR 0045. Ver [`gravar_o_sim_de_quem_hospeda`].
///
/// Os dois gravadores têm `AppHandle` e pedem uma hospedagem de pé, que num
/// teste abriria UPnP e PCP no roteador, e a impressão da sessão pede uma
/// conexão. Por isso o que roda aqui é:
/// - a resposta de [`onde_estou_em`], a função pura sobre impressões que o
///   portão usa;
/// - a identidade que cada gravador calcula, sobre o banco de verdade do
///   servidor hospedado, em memória;
/// - a gravação, sobre o arquivo de aceites de verdade, numa pasta de passagem.
///
/// O guarda de texto-fonte prende os dois gravadores ao `onde_estou`.
#[cfg(test)]
mod o_sim_de_quem_hospeda_vai_so_ao_servidor_daqui {
    use seele_server::mods::anuncio::conjunto_exigido;
    use seele_server::persistence::mods::{definir_conjunto, enable, EnabledMod};
    use seele_server::persistence::{Location, Persistence};

    use super::{
        corpo_de, fonte_sem_comentarios, gravar_o_sim_de_quem_hospeda,
        gravar_o_sim_se_for_o_guardado, onde_estou_em, Session,
    };

    /// A chave do guardado, a da sala e a de outro servidor S.
    const GUARDADO: &str = "aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11";
    const SALA: &str = "bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22";
    const DE_S: &str = "cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33";

    fn versao(numero: &str, byte: char) -> EnabledMod {
        EnabledMod {
            id: "autor/rpg".to_owned(),
            version: numero.to_owned(),
            hash: byte.to_string().repeat(64),
            repo: "https://example.invalid/rpg".to_owned(),
            reach: vec!["dom".to_owned()],
            server_half: false,
            api: seele_ffi::mods::MOD_API_VERSION,
        }
    }

    /// Uma pasta de configuração só deste teste.
    fn pasta(nome: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("seele-lote-x-{nome}-"))
            .tempdir()
            .unwrap()
    }

    /// A identidade que cada gravador grava, calculada como ele a calcula.
    fn identidades_dos_gravadores() -> [(&'static str, String); 2] {
        let salvar = Persistence::open(&Location::Memory).unwrap();
        definir_conjunto(&salvar, &[versao("1.0.0", 'a')]).unwrap();
        let atualizar = Persistence::open(&Location::Memory).unwrap();
        enable(&atualizar, &versao("1.0.0", 'a')).unwrap();
        enable(&atualizar, &versao("2.0.0", 'b')).unwrap();
        [
            (
                "o SALVAR da gestão (`aplicar_conjunto_de_mods`)",
                conjunto_exigido(&salvar).unwrap().identidade,
            ),
            (
                "a atualização pela gestão (`aplicar_mod`, QA-01)",
                conjunto_exigido(&atualizar).unwrap().identidade,
            ),
        ]
    }

    /// Um caso do portão: por onde a sessão entrou, a chave que ela conferiu, e
    /// as das hospedagens no ar.
    struct Caso {
        nome: &'static str,
        endereco: &'static str,
        impressao: &'static str,
        guardado: Option<&'static str>,
        grava: bool,
    }

    /// O que o portão grava, para cada caso e cada gravador: a lista do que
    /// saiu diferente do esperado.
    fn o_portao_nos(casos: &[Caso]) -> Vec<String> {
        let mut errados = Vec::new();
        for (quem, daqui) in identidades_dos_gravadores() {
            for caso in casos {
                let config = pasta("portao");
                let config = config.path().to_str().unwrap();
                let antes = "5".repeat(64);
                seele_ffi::mods::aceitar(config, caso.endereco, &antes).unwrap();
                let onde = onde_estou_em(
                    Some(caso.endereco),
                    Some(caso.impressao),
                    caso.guardado,
                    Some(SALA),
                );
                gravar_o_sim_se_for_o_guardado(config, Some(caso.endereco), onde, &daqui).unwrap();
                let esperado = if caso.grava { &daqui } else { &antes };
                if seele_ffi::mods::aceite_de(config, caso.endereco).as_deref()
                    != Some(esperado.as_str())
                {
                    errados.push(format!("{quem}, {}", caso.nome));
                }
            }
        }
        errados
    }

    /// A medida: a sessão em S, a janela hospedando, e o sim de S fica como
    /// estava. Pela chave, também com S no mesmo endereço do guardado.
    #[test]
    fn com_a_sessao_em_outro_servidor_o_sim_dele_fica_como_estava() {
        let errados = o_portao_nos(&[
            Caso {
                nome: "a sessão em S, noutro endereço",
                endereco: "198.51.100.7:8383",
                impressao: DE_S,
                guardado: Some(GUARDADO),
                grava: false,
            },
            Caso {
                nome: "a sessão em S, no mesmo endereço do guardado",
                endereco: "127.0.0.1:8383",
                impressao: DE_S,
                guardado: Some(GUARDADO),
                grava: false,
            },
        ]);
        assert!(
            errados.is_empty(),
            "com a sessão em outro servidor, o sim dele virou a identidade do servidor daqui: \
             {errados:?}. Ele pergunta de novo, ou, com os conjuntos iguais, não pergunta, e a \
             lista de aceites mostra um sim que a pessoa não deu (ADR 0045)"
        );
    }

    /// O A04 e o próprio link: com a chave do guardado, o sim de quem hospeda é
    /// gravado, entre a sessão pelo `aqui` ou pelo endereço da rede local que o
    /// convite anuncia.
    #[test]
    fn na_sessao_do_guardado_o_sim_e_gravado_entre_por_onde_entrar() {
        let errados = o_portao_nos(&[
            Caso {
                nome: "o `aqui`",
                endereco: "127.0.0.1:8383",
                impressao: GUARDADO,
                guardado: Some(GUARDADO),
                grava: true,
            },
            Caso {
                nome: "o próprio link, pela rede local",
                endereco: "192.168.1.20:8383",
                impressao: GUARDADO,
                guardado: Some(GUARDADO),
                grava: true,
            },
        ]);
        assert!(
            errados.is_empty(),
            "na sessão do servidor guardado, o sim de quem hospeda não foi gravado: \
             {errados:?}. Quem acabou de decidir o conjunto é perguntado de novo na volta (A04)"
        );
    }

    /// A sala pessoal não tem MODs, e sem guardado de pé não há servidor daqui:
    /// nos dois, nada é gravado.
    #[test]
    fn na_sala_e_sem_guardado_nada_e_gravado() {
        let errados = o_portao_nos(&[
            Caso {
                nome: "a sala pessoal",
                endereco: "127.0.0.1:8385",
                impressao: SALA,
                guardado: Some(GUARDADO),
                grava: false,
            },
            Caso {
                nome: "sem guardado de pé, no `aqui`",
                endereco: "127.0.0.1:8383",
                impressao: GUARDADO,
                guardado: None,
                grava: false,
            },
        ]);
        assert!(
            errados.is_empty(),
            "o sim de quem hospeda foi gravado fora do servidor guardado: {errados:?} (ADR 0045)"
        );

        // E pela sessão de verdade: sem conexão nem hospedagem, nada.
        let sessao = Session::default();
        *sessao.endereco_da_sessao.lock().unwrap() = Some("127.0.0.1:8383".to_owned());
        let config = pasta("sem-guardado");
        let config = config.path().to_str().unwrap();
        gravar_o_sim_de_quem_hospeda(config, &sessao, &"6".repeat(64)).unwrap();
        assert_eq!(
            seele_ffi::mods::aceite_de(config, "127.0.0.1:8383"),
            None,
            "sem conexão nem hospedagem, o sim de quem hospeda foi gravado no endereço da sessão"
        );
    }

    /// Os dois gravadores gravam o sim só por [`gravar_o_sim_de_quem_hospeda`],
    /// e ela só pela resposta de `onde_estou`.
    ///
    /// As agulhas são montadas por `concat!`, para que este guarda não case com
    /// o próprio texto, e a fonte é lida sem as linhas de comentário.
    #[test]
    fn os_dois_gravadores_passam_pelo_sim_de_quem_hospeda() {
        let fonte = fonte_sem_comentarios();
        let fora: Vec<&str> = [
            ("aplicar_mod", concat!("async fn aplicar_mod", "(")),
            (
                "aplicar_conjunto_de_mods",
                concat!("async fn aplicar_conjunto_de_mods", "("),
            ),
        ]
        .into_iter()
        .filter(|(_, cabecalho)| {
            let corpo = corpo_de(&fonte, cabecalho);
            !(corpo.contains(concat!("gravar_o_sim_de_quem_hospeda", "("))
                && !corpo.contains(concat!("seele_ffi::mods::aceitar", "("))
                && !corpo.contains(concat!("endereco_da", "_sessao")))
        })
        .map(|(quem, _)| quem)
        .collect();
        assert!(
            fora.is_empty(),
            "{fora:?} grava o sim sem passar por `gravar_o_sim_de_quem_hospeda`: com a sessão \
             em outro servidor, o sim dele vira a identidade do servidor daqui (ADR 0045)"
        );

        let grava = corpo_de(&fonte, concat!("fn gravar_o_sim_de_quem_hospeda", "("));
        let se_for = corpo_de(&fonte, concat!("fn gravar_o_sim_se_for_o_guardado", "("));
        let comando = corpo_de(&fonte, concat!("fn onde_estou", "(session"));
        assert!(
            grava.contains(concat!("onde_estou_na", "(session)"))
                && grava.contains(concat!("gravar_o_sim_se_for_o", "_guardado("))
                && se_for.contains(concat!("onde == OndeEstou::", "Guardado"))
                && comando.contains(concat!("onde_estou_na", "(&session)")),
            "o sim de quem hospeda deixou de passar pela resposta de `onde_estou`, ou o comando \
             deixou de dar a mesma: são duas respostas a «estou no meu servidor?», e o sim vai \
             a um servidor que não é o guardado (ADR 0045):\n{grava}\n{se_for}\n{comando}"
        );
    }
}

/// O texto deste arquivo, para os guardas que afirmam sobre ele.
#[cfg(test)]
const FONTE_DESTE_ARQUIVO: &str = include_str!("main.rs");

/// Este arquivo sem as linhas de comentário, para os guardas de texto-fonte.
///
/// Sem isto, um guarda passa porque a coisa que ele procura está **escrita num
/// comentário** — que é o defeito mais constrangedor que um guarda de
/// texto-fonte pode ter: ele confere a si mesmo.
#[cfg(test)]
fn fonte_sem_comentarios() -> String {
    FONTE_DESTE_ARQUIVO
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// O corpo de uma função, do cabeçalho até a chave que a fecha na coluna zero.
#[cfg(test)]
fn corpo_de<'a>(fonte: &'a str, cabecalho: &str) -> &'a str {
    let depois = fonte
        .split_once(cabecalho)
        .unwrap_or_else(|| panic!("`{cabecalho}` não está mais neste arquivo"))
        .1;
    depois
        .split_once("\n}\n")
        .map_or(depois, |(corpo, _)| corpo)
}

/// Por que um link de imagem não virou imagem.
///
/// Enum e não frase, como as vizinhas: a fronteira erro→texto é do frontend.
#[derive(Debug, serde::Serialize)]
enum FalhaNaPrevia {
    /// O esquema não sai desta janela. Ver [`endereco_que_pode_sair`].
    NaoEAberto(String),
    /// Não respondeu, ou respondeu erro.
    NaoRespondeu(String),
    /// Respondeu mais do que uma prévia pode custar.
    GrandeDemais,
    /// Respondeu, e o que veio não é imagem — ou não é a imagem que disse ser.
    NaoEImagem,
    /// O destino é desta máquina, desta rede, ou de um endereço reservado.
    ///
    /// ADR 0053. Separada de [`Self::NaoEAberto`] porque não é o esquema que
    /// está errado: `http://127.0.0.1:9999/private.png` é um endereço `http`
    /// perfeitamente válido, e é exatamente o que esta janela não pode ser
    /// mandada buscar por alguém que escreveu uma mensagem.
    DestinoPrivado(String),
    /// A pessoa ainda não disse que este domínio pode ser buscado.
    ///
    /// ADR 0053. Não é falha: é a política de prévia automática esperando uma
    /// decisão. A janela desenha o link com um botão em vez da imagem.
    SemConsentimento {
        /// O domínio sobre o qual a pergunta é feita.
        dominio: String,
    },
}

/// Busca um link de imagem e devolve um `data:` que a janela pode desenhar.
///
/// # Por que passa pelo Rust, e não pela janela
///
/// A CSP deste app é `default-src 'self'` com `img-src 'self' data: mod:`, e
/// ela **não se mexe** — `preview.rs` já diz isso sobre o `data:`. Alargá-la
/// para `https:` deixaria qualquer imagem remota carregar, de qualquer
/// origem, inclusive de dentro de um MOD: seria abrir um canal de rastreio
/// para toda a janela por causa de um GIF.
///
/// Aqui a busca é nossa, e com ela vêm as travas que a janela não teria como
/// aplicar sozinha.
///
/// # O que isto entrega a terceiros, dito em voz alta
///
/// Desenhar um link é buscá-lo, e buscar é aparecer: quem controla o endereço
/// aprende o IP de quem está lendo a conversa, e a hora. Não há como desenhar
/// sem isso. O que **não** acontece: nenhum cookie vai junto, nenhum
/// `Referer` diz de onde veio, e o conteúdo é conferido contra os bytes antes
/// de chegar à tela.
///
/// # Errors
///
/// [`FalhaNaPrevia`], uma variante por motivo.
#[tauri::command]
async fn previa_de_link(app: AppHandle, url: String) -> Result<String, FalhaNaPrevia> {
    // **A política de prévia automática vem antes de qualquer byte sair.**
    //
    // ADR 0053. Antes disto, ler uma mensagem já era uma requisição: bastava o
    // caminho terminar em `.png` e esta janela buscava o endereço de quem
    // escreveu — entregando a ele o endereço de origem de quem lê e a hora. Um
    // link posto na conversa só para colher endereços funcionava.
    //
    // Agora o domínio tem de estar consentido. Enquanto não estiver, a janela
    // desenha o link com um botão de «mostrar imagem» — e é o clique que
    // consente, uma vez, para aquele domínio.
    let destino = endereco_de_previa(&url)?;
    let dominio = dominio_de(destino)
        .ok_or_else(|| FalhaNaPrevia::NaoEAberto("endereço sem host".to_owned()))?;
    if !previa_consentida(&app, &dominio) {
        return Err(FalhaNaPrevia::SemConsentimento { dominio });
    }

    let (alegado, bytes) = buscar_com_teto(&url).await?;

    // A mesma conferência do anexo, e não uma segunda cópia dela: o tipo
    // alegado tem de bater com o que os bytes dizem ser. Um endereço que
    // promete `image/gif` e entrega HTML é recusado aqui.
    if let seele_ffi::preview::Verdict::Draw(formato) = seele_ffi::preview::judge(&alegado, &bytes)
    {
        return Ok(seele_ffi::preview::data_uri(formato, &bytes));
    }

    // **A página de um site de GIF, resolvida até a mídia que ela declara.**
    //
    // Só aqui, e só depois de os bytes já terem dito que não são imagem: o
    // endereço nunca decide isto sozinho. Quem manda um GIF manda
    // `tenor.com/view/…`, que é uma página HTML; sem este passo o produto
    // deixava o link cru na conversa e a pessoa lia isso como defeito.
    if e_de_um_dominio_de_gif(&url) && alegado.starts_with("text/html") {
        let html = String::from_utf8_lossy(&bytes);
        let midia = midia_declarada_pela_pagina(&html).ok_or(FalhaNaPrevia::NaoEImagem)?;
        // **A mídia tem de morar no mesmo punhado de domínios.** Sem isto, uma
        // página desta lista — ou um redirecionamento para ela — escolheria
        // qualquer endereço do mundo para esta janela buscar, e a lista
        // fechada não teria fechado nada.
        if !e_de_um_dominio_de_gif(&midia) {
            return Err(FalhaNaPrevia::NaoEImagem);
        }
        let (alegado, bytes) = buscar_com_teto(&midia).await?;
        return match seele_ffi::preview::judge(&alegado, &bytes) {
            seele_ffi::preview::Verdict::Draw(formato) => {
                Ok(seele_ffi::preview::data_uri(formato, &bytes))
            }
            _ => Err(FalhaNaPrevia::NaoEImagem),
        };
    }

    Err(FalhaNaPrevia::NaoEImagem)
}

/// Os domínios cuja **página** é resolvida até a mídia que ela declara.
///
/// # Por que uma lista fechada, e não «resolver qualquer página»
///
/// Desenhar um link é buscá-lo, e buscar é aparecer. O guarda de
/// `pareceImagem` existe para que o IP de quem lê a conversa não vá para todo
/// domínio que outra pessoa cole ali — inclusive um posto na conversa só para
/// colher endereços de quem a lê. Resolver qualquer página desfaria esse
/// guarda inteiro, e não é o que este produto está disposto a trocar por um
/// GIF.
///
/// O que a lista troca é menor e está dito: quem cola um link destes três
/// entrega a esses três — que já sabem quem pediu o GIF, porque é deles que ele
/// vem. Nenhum domínio novo passa a ser buscado.
///
/// **Domínio registrável, e a comparação é por sufixo de ponto.** `tenor.com`
/// alcança `www.tenor.com` e `media.tenor.com`; e o ponto é o que impede
/// `naotenor.com` de se passar por um deles — um `ends_with("tenor.com")` sem
/// o ponto aceita qualquer domínio que termine nessas letras, e aí quem cola
/// o link escolhe o endereço que esta janela busca.
const DOMINIOS_DE_GIF: &[&str] = &["tenor.com", "giphy.com", "imgur.com"];

/// Se este endereço mora num dos [`DOMINIOS_DE_GIF`].
fn e_de_um_dominio_de_gif(url: &str) -> bool {
    let Ok(endereco) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = endereco.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    DOMINIOS_DE_GIF
        .iter()
        .any(|dominio| host == *dominio || host.ends_with(&format!(".{dominio}")))
}

/// Quantos saltos uma busca de prévia segue antes de desistir.
const SALTOS_DA_PREVIA: usize = 3;

/// O host de um endereço, em minúsculas, para a pergunta do consentimento.
fn dominio_de(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

/// Se a prévia automática deste domínio já foi liberada por quem usa. ADR 0053.
///
/// Preferências que não abrem respondem «não», e é o lado certo: sem poder ler a
/// escolha, buscar seria decidir por alguém que não foi consultado. Um domínio de
/// GIF **também** passa por aqui — a lista fechada de
/// [`DOMINIOS_DE_GIF`] diz quais páginas são resolvidas até a mídia, e não que
/// buscá-las dispensa consentimento.
fn previa_consentida(app: &AppHandle, dominio: &str) -> bool {
    preferencias(app).is_some_and(|ajustes| ajustes.previa_consentida(dominio))
}

/// Se este endereço IP é de um lugar que uma prévia **não** pode alcançar.
///
/// ADR 0053. A lista é do que é reservado, e não do que é «interno»: loopback,
/// privado, link-local, `0.0.0.0/8`, compartilhado entre operadoras, multicast e
/// o resto do que não é internet pública. Em IPv6 o mesmo, mais os endereços
/// mapeados de IPv4 — sem eles, `http://[::ffff:127.0.0.1]/` atravessaria.
///
/// # Por que ela existe
///
/// Porque desenhar um link é buscá-lo, e quem escreve a mensagem escolhia o
/// destino. `http://127.0.0.1:9999/private.png` numa conversa fazia esta janela
/// bater na porta de um serviço da própria máquina de quem lê — e
/// `http://192.168.0.1/` na do roteador da casa dele. Não demonstrei extração de
/// conteúdo; a requisição sozinha já é o que não pode acontecer.
fn e_endereco_reservado(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;

    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || v4.is_unspecified()
                // `0.0.0.0/8`, que em várias pilhas é «esta máquina».
                || a == 0
                // `100.64.0.0/10`, compartilhado entre operadoras.
                || (a == 100 && (64..128).contains(&b))
                // `192.0.0.0/24`, atribuições de protocolo do IETF.
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0)
                // `198.18.0.0/15`, bancada de testes.
                || (a == 198 && (18..20).contains(&b))
                // `240.0.0.0/4` em diante, reservado.
                || a >= 240
        }
        IpAddr::V6(v6) => {
            // Um IPv4 mapeado é um IPv4, e tem de passar pela régua dele: sem
            // isto, `::ffff:127.0.0.1` é um IPv6 «qualquer» e atravessa.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return e_endereco_reservado(IpAddr::V4(v4));
            }
            let primeiro = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // `fc00::/7`, endereços locais únicos.
                || (primeiro & 0xfe00) == 0xfc00
                // `fe80::/10`, link-local.
                || (primeiro & 0xffc0) == 0xfe80
                // `2001:db8::/32`, documentação.
                || (primeiro == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

/// O endereço que esta janela pode **buscar sozinha**, ou o motivo de não poder.
///
/// # Por que ela não é [`endereco_que_pode_sair`]
///
/// ADR 0053: são duas políticas para duas coisas diferentes, e tratá-las como
/// uma foi o defeito. Abrir um link é uma pessoa **clicando** e o navegador do
/// sistema indo — ali `http://192.168.0.1` é legítimo, é o roteador da casa
/// dela, e recusá-lo seria o produto decidindo onde ela pode navegar. Buscar uma
/// prévia é esta janela indo **sozinha**, por causa de um texto que outra pessoa
/// escreveu, sem ninguém clicar em nada.
///
/// O que esta acrescenta à outra: o destino não pode ser um endereço reservado,
/// e não pode ser um nome que só existe nesta rede.
fn endereco_de_previa(bruto: &str) -> Result<&str, FalhaNaPrevia> {
    let url = endereco_que_pode_sair(bruto).map_err(FalhaNaPrevia::NaoEAberto)?;
    let endereco =
        reqwest::Url::parse(url).map_err(|erro| FalhaNaPrevia::NaoEAberto(erro.to_string()))?;
    let Some(host) = endereco.host_str() else {
        return Err(FalhaNaPrevia::NaoEAberto("endereço sem host".to_owned()));
    };
    if !host_de_previa(host) {
        return Err(FalhaNaPrevia::DestinoPrivado(host.to_ascii_lowercase()));
    }
    Ok(url)
}

/// Se este host, **como está escrito**, pode ser buscado por uma prévia.
///
/// Duas perguntas numa função porque as duas são sobre o texto do host e não
/// sobre a resolução dele:
///
///   - um endereço literal não pode ser reservado — `http://127.0.0.1/x.png` e
///     `http://[::ffff:127.0.0.1]/x.png` são os dois casos, e o segundo é o que
///     uma régua só de IPv4 deixa passar;
///   - um nome não pode ser dos que por definição não são da internet pública.
///     Conferido aqui **além** da conferência sobre os endereços resolvidos,
///     porque um resolvedor local pode devolver um endereço público para
///     `algo.local` e a conferência de baixo o aceitaria.
fn host_de_previa(host: &str) -> bool {
    // Um IPv6 numa URL vem entre colchetes, e `parse` não os quer.
    let literal = host
        .strip_prefix('[')
        .and_then(|resto| resto.strip_suffix(']'));
    if let Ok(ip) = literal.unwrap_or(host).parse::<std::net::IpAddr>() {
        return !e_endereco_reservado(ip);
    }
    let nome = host.to_ascii_lowercase();
    // O ponto final de um nome absoluto não muda o nome: `localhost.` é
    // `localhost`, e sem isto ele atravessaria.
    let nome = nome.strip_suffix('.').unwrap_or(&nome);
    !(nome == "localhost"
        || nome.ends_with(".localhost")
        || nome.ends_with(".local")
        || nome.ends_with(".internal")
        || nome.ends_with(".home.arpa"))
}

/// Os endereços deste nome, se **todos** eles são da internet pública.
///
/// # Por que todos, e não «algum»
///
/// Porque a escolha não é nossa. Um nome que resolve para um endereço público e
/// um privado deixaria a pilha do sistema escolher qual usar, e metade das
/// tentativas bateria na rede de quem lê. «Todos» é a única resposta que não
/// depende de sorte.
///
/// O resultado é usado para **fixar** o endereço no cliente — ver
/// [`cliente_da_previa`]. É isso que fecha a troca de resolução: sem fixar, o
/// nome é resolvido uma vez para conferir e outra vez para conectar, e nada
/// obriga as duas respostas a serem a mesma.
async fn enderecos_publicos_de(
    host: &str,
    porta: u16,
) -> Result<Vec<std::net::SocketAddr>, FalhaNaPrevia> {
    // Um endereço literal não precisa de resolvedor, e passar por ele seria
    // dar-lhe a chance de responder outra coisa.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if e_endereco_reservado(ip) {
            return Err(FalhaNaPrevia::DestinoPrivado(ip.to_string()));
        }
        return Ok(vec![std::net::SocketAddr::new(ip, porta)]);
    }

    let resolvidos: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, porta))
        .await
        .map_err(|erro| FalhaNaPrevia::NaoRespondeu(erro.to_string()))?
        .collect();
    if resolvidos.is_empty() {
        return Err(FalhaNaPrevia::NaoRespondeu(format!(
            "o nome {host} não resolveu para endereço nenhum"
        )));
    }
    if let Some(reservado) = resolvidos
        .iter()
        .find(|alvo| e_endereco_reservado(alvo.ip()))
    {
        return Err(FalhaNaPrevia::DestinoPrivado(format!(
            "{host} resolve para {}",
            reservado.ip()
        )));
    }
    Ok(resolvidos)
}

/// O cliente de **um** salto da busca de prévia, com o endereço fixado.
///
/// # Por que um cliente por salto, e por que fixado
///
/// A regra de salto de antes conferia cada redirecionamento pela política de
/// abertura de link — que aceita `http://127.0.0.1`. Um `302` para a rede local
/// atravessava, e a validação não era refeita sobre a resolução do nome.
///
/// Agora cada salto é uma ida separada: o endereço é validado, o nome é
/// resolvido, os endereços são conferidos, e o cliente é construído fixado
/// **naqueles** endereços. Um resolvedor que responda outra coisa entre a
/// conferência e a conexão não tem onde entrar, porque não há segunda resolução.
///
/// `Policy::none()` porque o laço em [`buscar_com_teto`] é quem segue os saltos:
/// uma política interna seguiria com este cliente, fixado no host errado.
fn cliente_da_previa(
    host: &str,
    enderecos: &[std::net::SocketAddr],
) -> Result<reqwest::Client, FalhaNaPrevia> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, enderecos)
        .build()
        .map_err(|erro| FalhaNaPrevia::NaoRespondeu(erro.to_string()))
}

/// Busca um endereço e devolve o tipo alegado e os bytes, sob o teto.
///
/// # Errors
///
/// [`FalhaNaPrevia`], uma variante por motivo.
async fn buscar_com_teto(url: &str) -> Result<(String, Vec<u8>), FalhaNaPrevia> {
    // **Cada salto é uma ida completa: valida, resolve, confere, fixa, busca.**
    //
    // O laço substitui a política de redirecionamento do cliente, e a troca é o
    // conserto: aquela política conferia o salto pela régua de *abrir um link*,
    // que aceita a rede local, e não refazia nada sobre a resolução do nome. Um
    // `302` para `http://192.168.0.1/` atravessava.
    let mut alvo = reqwest::Url::parse(url.trim())
        .map_err(|erro| FalhaNaPrevia::NaoEAberto(erro.to_string()))?;
    let mut saltos = 0usize;
    let resposta = loop {
        let texto = alvo.to_string();
        endereco_de_previa(&texto)?;
        let host = alvo
            .host_str()
            .ok_or_else(|| FalhaNaPrevia::NaoEAberto("endereço sem host".to_owned()))?
            .to_owned();
        let porta = alvo.port_or_known_default().unwrap_or(443);
        let enderecos = enderecos_publicos_de(host.trim_matches(['[', ']']), porta).await?;
        let cliente = cliente_da_previa(&host, &enderecos)?;

        let resposta = cliente
            .get(alvo.clone())
            .send()
            .await
            .map_err(|erro| FalhaNaPrevia::NaoRespondeu(erro.to_string()))?;

        if !resposta.status().is_redirection() {
            break resposta
                .error_for_status()
                .map_err(|erro| FalhaNaPrevia::NaoRespondeu(erro.to_string()))?;
        }
        saltos += 1;
        if saltos > SALTOS_DA_PREVIA {
            return Err(FalhaNaPrevia::NaoRespondeu(
                "o endereço redireciona sem parar".to_owned(),
            ));
        }
        let destino = resposta
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|valor| valor.to_str().ok())
            .ok_or_else(|| {
                FalhaNaPrevia::NaoRespondeu("redirecionamento sem destino".to_owned())
            })?;
        // Resolvido contra o endereço atual, porque um `Location` pode ser
        // relativo. A volta do laço o valida de novo, inteiro.
        alvo = alvo
            .join(destino)
            .map_err(|erro| FalhaNaPrevia::NaoEAberto(erro.to_string()))?;
    };

    // O tipo que o outro lado alega. Ele **não** decide nada sozinho: quem
    // decide são os bytes, em `judge`.
    let alegado = resposta
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    // **O teto é aplicado enquanto chega, e não depois.** Confiar no
    // `Content-Length` seria confiar em quem está do outro lado; juntar tudo
    // para medir seria deixá-lo escolher quanta memória esta janela gasta.
    //
    // O mesmo teto vale para o HTML de uma página de GIF: ela também é bytes
    // que outra pessoa serve, e um teto separado para ela seria um segundo
    // número para a mesma pergunta.
    let mut bytes: Vec<u8> = Vec::new();
    let mut corpo = resposta;
    while let Some(pedaco) = corpo
        .chunk()
        .await
        .map_err(|erro| FalhaNaPrevia::NaoRespondeu(erro.to_string()))?
    {
        if bytes.len() + pedaco.len() > seele_ffi::preview::PREVIEW_LIMIT as usize {
            return Err(FalhaNaPrevia::GrandeDemais);
        }
        bytes.extend_from_slice(&pedaco);
    }
    Ok((alegado, bytes))
}

/// O endereço da mídia que uma página declara para quem a compartilha.
///
/// `og:image` é o que Tenor, Giphy e Imgur escrevem para que a prévia apareça
/// em qualquer lugar onde o link seja colado. Ler isso é usar o que a página
/// publicou para este fim exato, e não raspar o HTML atrás de um `<img>`:
/// aquele é o endereço que o site escolheu mostrar, e ele não muda quando o
/// desenho da página muda.
///
/// `twitter:image` entra como recuo porque parte das páginas antigas do Imgur
/// só trazem esse.
///
/// **Não resolve endereço relativo.** Um `og:image` relativo teria de ser
/// juntado à base da página, e os três escrevem absoluto; devolver o relativo
/// faria a conferência de domínio recusá-lo, que é o desfecho certo para o que
/// este código não entende.
fn midia_declarada_pela_pagina(html: &str) -> Option<String> {
    for etiqueta in etiquetas_meta(html) {
        let atributos = atributos_de(etiqueta);
        let acha = |nome: &str| {
            atributos
                .iter()
                .find(|(chave, _)| chave == nome)
                .map(|(_, valor)| valor.clone())
        };
        let Some(qual) = acha("property").or_else(|| acha("name")) else {
            continue;
        };
        if !matches!(
            qual.trim().to_ascii_lowercase().as_str(),
            "og:image" | "og:image:secure_url" | "twitter:image"
        ) {
            continue;
        }
        let Some(conteudo) = acha("content") else {
            continue;
        };
        // `&amp;` e mais nada: um endereço de mídia carrega parâmetros de
        // consulta, e é essa a entidade que um gerador de HTML escreve neles.
        let conteudo = conteudo.replace("&amp;", "&");
        let conteudo = conteudo.trim();
        if !conteudo.is_empty() {
            return Some(conteudo.to_owned());
        }
    }
    None
}

/// Os trechos entre `<meta` e o `>` que o fecha.
///
/// A procura é na cópia em minúsculas e o corte é no original: `<META` e
/// `<meta` são a mesma etiqueta, e `to_ascii_lowercase` não muda o tamanho em
/// bytes de nada — então um deslocamento achado numa vale na outra.
fn etiquetas_meta(html: &str) -> Vec<&str> {
    let minusculo = html.to_ascii_lowercase();
    minusculo
        .match_indices("<meta")
        .filter_map(|(inicio, _)| {
            let resto = html.get(inicio + "<meta".len()..)?;
            resto.get(..resto.find('>')?)
        })
        .collect()
}

/// Os pares `nome="valor"` de uma etiqueta, com o nome em minúsculas.
///
/// Escrito à mão e não com um analisador de HTML: a árvore deste app diz «nada
/// além disto» sobre dependências, e o que se lê aqui são atributos de uma
/// etiqueta que não aninha nada. O que ele **não** faz — entidades além de
/// `&amp;`, atributo sem valor com significado, HTML malformado — só o faz
/// devolver menos, e devolver menos é a página não ter prévia.
fn atributos_de(etiqueta: &str) -> Vec<(String, String)> {
    /// Onde o varredor está dentro da etiqueta.
    enum Onde {
        /// Juntando letras de um nome.
        Nome,
        /// O nome acabou num espaço, e o `=` ainda pode vir.
        AntesDoIgual,
        /// O `=` veio; o valor começa no próximo que não for espaço.
        AntesDoValor,
        /// Dentro do valor. `Some` são as aspas que o fecham; `None`, o espaço.
        Valor(Option<char>),
    }

    let mut saida: Vec<(String, String)> = Vec::new();
    let mut nome = String::new();
    let mut valor = String::new();
    let mut onde = Onde::Nome;

    // Por caractere e não por índice: um `&str` não se fatia em qualquer byte,
    // e um valor de atributo com acento faria o corte cair no meio de um.
    for c in etiqueta.chars() {
        match onde {
            Onde::Nome => {
                if c == '=' {
                    onde = Onde::AntesDoValor;
                } else if c.is_whitespace() {
                    if !nome.is_empty() {
                        onde = Onde::AntesDoIgual;
                    }
                } else if c == '/' {
                    nome.clear();
                } else {
                    nome.push(c);
                }
            }
            Onde::AntesDoIgual => {
                if c == '=' {
                    onde = Onde::AntesDoValor;
                } else if !c.is_whitespace() {
                    // Atributo sem valor. Nenhum dos que interessam é assim, e
                    // o que começou aqui é o nome do próximo.
                    nome.clear();
                    nome.push(c);
                    onde = Onde::Nome;
                }
            }
            Onde::AntesDoValor => {
                if c == '"' || c == '\'' {
                    onde = Onde::Valor(Some(c));
                } else if !c.is_whitespace() {
                    valor.push(c);
                    onde = Onde::Valor(None);
                }
            }
            Onde::Valor(fecha) => {
                let acabou = fecha.map_or_else(|| c.is_whitespace(), |aspa| c == aspa);
                if acabou {
                    saida.push((nome.to_ascii_lowercase(), std::mem::take(&mut valor)));
                    nome.clear();
                    onde = Onde::Nome;
                } else {
                    valor.push(c);
                }
            }
        }
    }
    // Um valor sem aspas encostado no fim da etiqueta ainda é um valor. Um
    // valor com aspas que nunca fecham é etiqueta quebrada, e cai fora.
    if matches!(onde, Onde::Valor(None)) {
        saida.push((nome.to_ascii_lowercase(), valor));
    }
    saida
}

/// As versões da API de MOD que este build aceita executar.
///
/// **Para a tela de MODs escolher qual versão de um MOD oferecer.** O catálogo
/// lista todas as versões publicadas, e a última delas pode pedir uma API que
/// este build não tem — é o que acontece no dia seguinte a toda subida de API.
/// Oferecer a última da lista faz quem ainda não atualizou apertar instalar e
/// receber uma recusa, **sem caminho de volta** para a versão que funciona para
/// ele, publicada logo acima na mesma lista.
///
/// Lida daqui e não escrita na janela pelo mesmo motivo de [`regras_de_previa`]
/// e de [`dominios_de_gif`]: uma segunda cópia discorda no dia em que
/// `APIS_ACEITAS` mudar, e discordaria oferecendo o que o produto recusa.
#[tauri::command]
fn apis_de_mod_aceitas() -> Vec<u32> {
    seele_ffi::mods::APIS_ACEITAS.to_vec()
}

/// Os domínios cuja página esta janela resolve, para o lado de lá decidir a
/// quem perguntar.
///
/// Lido daqui e não escrito na janela pelo mesmo motivo de
/// [`regras_de_previa`]: duas cópias de uma lista discordam algum dia, e esta
/// discordaria oferecendo buscar um domínio que o Rust depois recusa — ou,
/// pior, deixando de buscar um que ele aceitaria.
#[tauri::command]
fn dominios_de_gif() -> Vec<String> {
    DOMINIOS_DE_GIF
        .iter()
        .map(|dominio| (*dominio).to_owned())
        .collect()
}

/// O código do cliente de um MOD, conferido, para a janela pôr num `Worker`.
///
/// # Por que passa por aqui, e não pelo `mod://`
///
/// ADR 0049. Um `Worker` não aceita URL de outra origem, e `mod://localhost` é
/// outra origem que `tauri://localhost`. As saídas eram alargar `connect-src`
/// para a janela buscar o texto por `fetch`, ou pedi-lo pela ponte que já
/// existe.
///
/// A segunda não custa nada e é melhor: a conferência de hash **já mora** deste
/// lado, e o texto vira um `Blob` de mesma origem na janela. A CSP ganha
/// `worker-src blob:` e mais nada.
///
/// E o `mod://` saiu junto. Ele existia para a janela carregar o código de um
/// MOD como `<script>`, e isso deixou de acontecer: um esquema registrado sem
/// ninguém do outro lado é alcance de disco que um terceiro escolhe, oferecido
/// à página de graça. A conferência de caminho e de manifesto que ele carregava
/// continua, em `mods::serve`, que é quem este comando chama.
///
/// # Errors
///
/// [`FalhaNoMod`] quando o pacote não está no disco, quando os bytes não são os
/// que o nome da pasta promete, ou quando o manifesto não declara metade de
/// cliente.
#[tauri::command]
fn codigo_do_mod(
    app: AppHandle,
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    hash: String,
) -> Result<(String, u32), FalhaNoMod> {
    // **Dita no registro, uma vez por motivo** ([`recusa_de_carga_dita`]). A
    // carga só é dada por aceita quando a reserva sobe, em
    // [`mod_nativo_reservar`]: o código lido aqui ainda não é um MOD de pé.
    ler_codigo_do_mod(&app, &session, geracao, &id, &hash)
        .map_err(|falha| recusa_de_carga_dita(&session.recusas_de_carga, geracao, &id, falha))
}

/// O corpo de [`codigo_do_mod`], com a recusa devolvida sem ser dita — é o
/// caminho que [`mod_nativo_reservar`] também usa, e quem diz é o comando que
/// a janela chamou.
fn ler_codigo_do_mod(
    app: &AppHandle,
    session: &Session,
    geracao: u64,
    id: &str,
    hash: &str,
) -> Result<(String, u32), FalhaNoMod> {
    // O código de um MOD é o começo de uma montagem, e uma montagem pertence a
    // uma sessão. Entregá-lo para uma geração morta seria pôr um worker de pé
    // depois de a sessão dele ter acabado — a corrida que o roteiro da E2 chama
    // de «montagem atrasada».
    session.recusar_se_morta(geracao)?;
    let pacote = seele_ffi::mods::ler_por_hash(&config_dir(app), hash)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
    // **O hash tem de ser deste MOD.** Sem isto, uma janela que pedisse o
    // código de um MOD sob o nome de outro receberia o primeiro — e o nome é o
    // que a janela usa para decidir de quem é a região.
    if pacote.id != id {
        return Err(FalhaNoMod::Recusado {
            motivo: "conteudo-de-outro-mod".to_owned(),
        });
    }
    let Some(cliente) = pacote.client else {
        return Err(FalhaNoMod::Recusado {
            motivo: "sem-metade-de-cliente".to_owned(),
        });
    };
    let caminho = format!("/{id}/{cliente}");
    let bytes = mods::serve(std::path::Path::new(&config_dir(app)), &caminho, hash).ok_or(
        FalhaNoMod::Recusado {
            motivo: "cliente-nao-servido".to_owned(),
        },
    )?;
    // **A API declarada sai daqui junto com o fonte**, e não de uma segunda
    // leitura do manifesto: duas leituras são duas oportunidades de discordar,
    // e a que decide o que o MOD pode chamar tem de ser a mesma que provou o
    // hash dele.
    let api = pacote.api;
    let fonte = String::from_utf8(bytes).map_err(|_| FalhaNoMod::Recusado {
        motivo: "cliente-nao-e-utf8".to_owned(),
    })?;
    Ok((fonte, api))
}

/// Um arquivo de mídia de um MOD, pronto para a janela montar.
#[derive(serde::Serialize)]
struct MidiaDoMod {
    /// O `data:` inteiro, **com o tipo que os bytes provaram ser**.
    ///
    /// Composto aqui, e não na janela. É a regra do ADR 0027 para anexos, pela
    /// mesma razão: uma página que junta um tipo a bytes é uma página que pode
    /// juntar a **alegação de quem mandou** a eles, e a alegação é texto que um
    /// terceiro escolheu. O manifesto do MOD é exatamente esse texto.
    ///
    /// **Nenhum só no som do pacote** (P49-7b), e então fora da resposta. A
    /// janela lê os bytes dele crus, por `som_do_mod`, e um `data:` aqui seria
    /// o mesmo arquivo atravessando a ponte uma segunda vez, em base64, sem
    /// ninguém que o lesse. A imagem do pacote e toda mídia do servidor o
    /// trazem: o `<img>` mostra o `data:`, e o som do servidor não tem outro
    /// caminho até a janela (`bufferDaUri`, em `mods-regiao.js`).
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<String>,
    /// `som` ou `imagem`, de uma lista fechada.
    ///
    /// A janela precisa saber qual elemento montar, e recebe a palavra em vez
    /// de derivá-la do tipo: derivá-la seria uma segunda cópia da lista, e uma
    /// segunda cópia é uma que pode discordar.
    papel: &'static str,
    /// Quantos bytes o arquivo tem, para a contabilidade de recursos da janela.
    bytes: usize,
    /// **Quanto a imagem pesa decodificada** — largura × altura × 4 × quadros —,
    /// quando ela foi reduzida a um uso (`reducao`; rascunho da API 6, §4). É o
    /// que o bolso da capa conta: o custo na janela é o quadro, e não o arquivo.
    #[serde(skip_serializing_if = "Option::is_none")]
    decodificados: Option<u64>,
}

/// O nome de uma recusa de MOD, como o registro o escreve.
///
/// A mesma palavra que a janela recebe em `Recusado { motivo }`: quem lê o
/// `seele.log` e quem lê a frase na tela procuram pela mesma coisa.
fn motivo_de(falha: &FalhaNoMod) -> &str {
    match falha {
        FalhaNoMod::Recusado { motivo } => motivo.as_str(),
        FalhaNoMod::NaoEstaHospedando => "nao-esta-hospedando",
        FalhaNoMod::BancoNaoRespondeu => "banco-nao-respondeu",
        FalhaNoMod::ConjuntoMudou { .. } => "conjunto-mudou",
    }
}

/// **Diz no registro por que a mídia de um MOD foi recusada**, e devolve a
/// recusa intacta.
///
/// Em 23/09, descobrir que o avatar do PERFIS carregava levou uma hora de
/// medição e um reinício com `RUST_LOG=debug`: o Rust recusava, devolvia à
/// janela, e o registro não tinha uma palavra.
///
/// WARN com o id do MOD, a origem e o motivo — menos `sessao-encerrada`, que
/// vai em DEBUG: uma mídia que não montou porque a pessoa saiu é o desfecho
/// certo, e avisá-la ensinaria a ignorar os avisos.
///
/// **Texto de terceiro não entra solto numa linha do registro**, e são três
/// campos com ele. `origem` é o caminho que a janela pede em `midia_do_mod` e
/// em `som_do_mod`, e passa por [`caminho_no_registro`]: as aspas impedem a
/// linha forjada, e o teto, a linha sem fim. O `mod_id` chega da janela
/// (`midia_em_bytes`, `ler_imagem_mod`) e passa por [`id_no_registro`], a
/// mesma função de [`registrar_da_janela`]. O `motivo` passa por
/// [`motivo_no_registro`]: em `ler_imagem_mod` ele é o texto que o servidor
/// mandou. A recusa que volta à janela sai intacta — quem se protege é a
/// linha, e não a resposta.
///
/// **Com o teto de vazão do MOD** ([`linha_admitida`]): um MOD que pede num
/// laço a mesma mídia recusada deixava uma linha por pedido. A recusa volta à
/// janela do mesmo jeito; só a linha é segurada, e contada. A recusa gasta do
/// balde de avisos do MOD, e a sessão que acabou, do de depuração
/// ([`Classe`]): nenhuma mídia servida segura uma recusa.
fn recusa_de_midia_dita(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    mod_id: &str,
    origem: &str,
    falha: FalhaNoMod,
) -> FalhaNoMod {
    let motivo = motivo_de(&falha);
    let classe = if motivo == "sessao-encerrada" {
        Classe::Depuracao
    } else {
        Classe::Aviso
    };
    let Some(Admitida {
        suprimidas,
        transbordo,
    }) = linha_admitida(baldes, agora, classe, mod_id)
    else {
        return falha;
    };
    let mod_id = id_no_registro(mod_id);
    let origem = caminho_no_registro(origem);
    let no_registro = motivo_no_registro(motivo);
    if motivo == "sessao-encerrada" {
        tracing::debug!(
            mod_id = %mod_id,
            origem = %origem,
            motivo = %no_registro,
            suprimidas,
            transbordo,
            "mídia de MOD não servida: a sessão acabou"
        );
    } else {
        tracing::warn!(
            mod_id = %mod_id,
            origem = %origem,
            motivo = %no_registro,
            suprimidas,
            transbordo,
            "mídia de MOD recusada"
        );
    }
    falha
}

/// **O motivo de uma recusa como uma linha do `seele.log` o leva.**
///
/// Um nome de lista fechada (`arquivo-nao-declarado`) sai como está: é
/// `motivo=arquivo-nao-declarado` que quem lê o registro procura, e é a mesma
/// palavra que a janela recebe. Qualquer outra coisa é texto de terceiro — o
/// `erro` que o servidor põe na resposta de imagem, a razão com que a conexão
/// fechou, que `ler_imagem_mod` repassa como veio — e sai entre aspas, com
/// escape.
///
/// **Cortado em [`TETO_DA_FRASE_NO_REGISTRO`] pelo tamanho já escapado**, com
/// a mesma regra do `console` ([`executor::ate_o_teto_do_registro`]): o `{:?}`
/// faz um caractere que não se imprime ocupar até dez (`\u{100000}`), e um
/// corte contado antes do escape deixava quinhentos e doze deles virarem 5122
/// caracteres na linha, com as aspas.
///
/// O escape é o que importa: o formatador escapa só a mensagem, e só ANSI; um
/// campo em `%` sai cru, e quem protege a linha são as aspas e o escape desta
/// função. Cru, um `\n` no meio do motivo escreveria no `seele.log` uma
/// segunda linha com a cara do produto; e sem aspas
/// `motivo=Prazo da transferência esgotado.` não se lê como chave=valor.
fn motivo_no_registro(motivo: &str) -> String {
    let cortado = executor::ate_o_teto_do_registro(motivo);
    let nome = !cortado.is_empty()
        && cortado
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if nome {
        cortado.to_owned()
    } else {
        format!("{cortado:?}")
    }
}

/// **O caminho de uma mídia de MOD como uma linha do `seele.log` o leva**:
/// cortado em [`TETO_DA_FRASE_NO_REGISTRO`] pelo tamanho já escapado, e entre
/// aspas, com escape.
///
/// O caminho é texto de terceiro nas duas pontas. Na recusa, é o que a janela
/// pede ([`recusa_de_midia_dita`], campo `origem`). Na mídia e no som servidos,
/// é o nome que o MOD escolheu no manifesto ([`midia_servida_dita`] e
/// [`som_servido_dito`], campo `caminho`), e o manifesto só confere que cada
/// pedaço é um nome (`inner_path`): uma quebra de linha passa. O formatador do
/// registro não escapa quebra de linha em `%`, e um `\n` no caminho escreveria
/// no `seele.log` uma segunda linha com a cara do produto.
///
/// Uma função só para as três linhas, para que o teto e o escape não divirjam:
/// as aspas impedem a linha forjada, e o teto, a linha sem fim. O corte é o
/// mesmo de [`motivo_no_registro`] e do `console`
/// ([`executor::ate_o_teto_do_registro`]): contado depois do escape, que pode
/// fazer um caractere ocupar até dez.
fn caminho_no_registro(caminho: &str) -> String {
    format!("{:?}", executor::ate_o_teto_do_registro(caminho))
}

/// **Os bytes de um arquivo que o manifesto deste MOD declara**, conferidos.
///
/// A conferência inteira de [`midia_do_mod`], num lugar só para que o som
/// (`som_do_mod`) passe pela mesma: o pacote achado pelo hash, o hash sendo
/// deste MOD, o arquivo declarado e dentro da pasta, e o teto por arquivo.
fn bytes_declarados_do_mod(
    pasta: &str,
    id: &str,
    hash: &str,
    caminho: &str,
) -> Result<Vec<u8>, FalhaNoMod> {
    let pacote = seele_ffi::mods::ler_por_hash(pasta, hash)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
    // **O hash tem de ser deste MOD.** Sem isto, uma janela que pedisse a
    // mídia de um MOD sob o nome de outro receberia a do primeiro.
    if pacote.id != id {
        return Err(FalhaNoMod::Recusado {
            motivo: "conteudo-de-outro-mod".to_owned(),
        });
    }
    let bytes = mods::serve(
        std::path::Path::new(pasta),
        &format!("/{id}/{caminho}"),
        hash,
    )
    .ok_or(FalhaNoMod::Recusado {
        motivo: "arquivo-nao-declarado".to_owned(),
    })?;
    // **O tamanho é separado do formato**: são recusas diferentes para quem
    // está escrevendo o MOD, e «não toca» sem dizer qual das duas é a falha
    // que este repositório mais paga.
    if bytes.len() > seele_ffi::mods::TETO_DE_MIDIA {
        return Err(FalhaNoMod::Recusado {
            motivo: "arquivo-grande-demais".to_owned(),
        });
    }
    Ok(bytes)
}

/// A mídia de um MOD pronta para a janela, **ou a recusa dita no registro**,
/// com o teto de vazão de `baldes` ([`recusa_de_midia_dita`]).
///
/// **O tipo sai dos bytes antes do `data:`** (`ler_tipo`, a pergunta de oito
/// bytes): o som do pacote volta só com o papel e o tamanho, sem `uri`, e
/// só a imagem passa por `ler_midia`, que codifica o arquivo inteiro. As
/// conferências de pacote, de arquivo declarado e de teto são as mesmas para
/// os dois, em [`bytes_declarados_do_mod`].
fn midia_declarada_do_mod(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    pasta: &str,
    id: &str,
    hash: &str,
    caminho: &str,
) -> Result<MidiaDoMod, FalhaNoMod> {
    bytes_declarados_do_mod(pasta, id, hash, caminho)
        .and_then(|bytes| {
            let desconhecido = || FalhaNoMod::Recusado {
                motivo: "formato-desconhecido".to_owned(),
            };
            let tipo = seele_ffi::mods::ler_tipo(&bytes).ok_or_else(desconhecido)?;
            if tipo.papel == "som" {
                return Ok(MidiaDoMod {
                    uri: None,
                    papel: tipo.papel,
                    bytes: bytes.len(),
                    decodificados: None,
                });
            }
            let lida = seele_ffi::mods::ler_midia(&bytes).ok_or_else(desconhecido)?;
            Ok(MidiaDoMod {
                uri: Some(lida.uri),
                papel: lida.papel,
                bytes: lida.bytes,
                decodificados: None,
            })
        })
        .map_err(|falha| recusa_de_midia_dita(baldes, agora, id, caminho, falha))
}

/// **Um arquivo que o MOD declarou, para a janela tocar ou mostrar.**
///
/// Quatro recusas, todas pelo nome:
///
/// - a sessão acabou — montar mídia de uma geração morta é admitir efeito dela;
/// - o pacote é de outro MOD, achado pelo mesmo hash que a janela mandou;
/// - o arquivo não está declarado no manifesto, ou tenta sair da pasta — as
///   duas respondidas por `mods::serve` com a mesma resposta, de propósito;
/// - os bytes não são de um formato que este produto decodifica, ou passam do
///   teto.
///
/// O teto é por arquivo. O total é da janela, que é quem sabe quantos já
/// montou — por lugar, em `LIMITES_DA_REGIAO.bytesDeMidia` e no bolso dos
/// cartões (`mods-regiao.js`).
///
/// **O som sai sem `uri`**, só com o papel e o tamanho: a janela o pede cru a
/// [`som_do_mod`], e o `data:` dele seria o arquivo atravessando a ponte duas
/// vezes ([`MidiaDoMod`]).
///
/// Toda recusa, exceto a de geração morta (contada em
/// `comandos_de_geracao_morta`), vai ao `seele.log` por
/// [`recusa_de_midia_dita`], com o id do MOD e o motivo.
#[tauri::command]
fn midia_do_mod(
    app: AppHandle,
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    hash: String,
    caminho: String,
) -> Result<MidiaDoMod, FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    let midia = midia_declarada_do_mod(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &config_dir(&app),
        &id,
        &hash,
        &caminho,
    )?;
    // **Dito, como o código do MOD é dito.** Um arquivo que sai do pacote de um
    // terceiro e vira som na máquina de quem está numa conversa é um evento que
    // quem hospeda tem direito de ler no registro — e é o que permite medir o
    // caminho da mídia sem a janela contar nada.
    midia_servida_dita(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &id,
        geracao,
        &caminho,
        midia.papel,
        midia.bytes,
    );
    Ok(midia)
}

/// **Diz no registro que a mídia de um MOD saiu**, e qual.
///
/// Fora do comando pelo mesmo motivo de [`som_servido_dito`]: um teste lê a
/// linha sem `AppHandle`. O caminho é o nome que o MOD escolheu no manifesto,
/// e passa por [`caminho_no_registro`]: escrito cru, um nome com quebra de
/// linha escreveria, a cada mídia servida, uma linha com a cara do produto. O
/// `mod_id` vai em `%` como está, pela razão dita em [`som_servido_dito`].
///
/// Com o teto de vazão do MOD ([`linha_admitida`]): a janela pede a mesma
/// mídia a cada vez que a monta de novo. No balde das servidas
/// ([`Classe::Servida`]), e não no dos avisos: um MOD com muitas mídias não
/// cala a recusa da seguinte.
fn midia_servida_dita(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    mod_id: &str,
    geracao: u64,
    caminho: &str,
    papel: &str,
    bytes: usize,
) {
    let Some(Admitida {
        suprimidas,
        transbordo,
    }) = linha_admitida(baldes, agora, Classe::Servida, mod_id)
    else {
        return;
    };
    tracing::info!(
        mod_id = %mod_id,
        geracao,
        caminho = %caminho_no_registro(caminho),
        papel,
        bytes,
        suprimidas,
        transbordo,
        "mídia de MOD servida"
    );
}

/// **Os bytes de um som que o MOD declarou**, crus, para a janela tocar por
/// WebAudio.
///
/// # Por que um comando além de [`midia_do_mod`]
///
/// A CSP desta janela não tem `media-src` e cai em `default-src 'self'`: um
/// `<audio src="data:…">` cai junto — medido no Chromium, o motor do WebView2;
/// a medida no WKWebView é a Task 1 do Plano 1D —, e o `uri` que
/// [`midia_do_mod`] devolve é um `data:`. A saída que não afrouxa nada é a
/// casca decodificar os bytes com `decodeAudioData` — um `ArrayBuffer` não é
/// uma busca, e não passa por `media-src`.
///
/// Os bytes já chegavam dentro do `data:`, em base64: um terço maior, em JSON,
/// e decodificado à mão na thread da janela. Aqui eles vão crus, por
/// `tauri::ipc::Response`, e chegam como `ArrayBuffer` pelo protocolo `ipc:`.
/// Quando o Tauri cai no `postMessage`, chegam como lista de números — a casca
/// aceita os dois.
///
/// A conferência é a mesma da imagem, e é a mesma **função**
/// ([`bytes_declarados_do_mod`]): o pacote pelo hash, o hash deste MOD, o
/// arquivo declarado, o teto. E o tipo sai dos bytes: o que não é som é
/// recusado pelo nome, `nao-e-som`, em vez de virar um `decodeAudioData` que
/// falha sem dizer por quê.
///
/// Toda recusa, exceto a de geração morta (contada em
/// `comandos_de_geracao_morta`), vai ao `seele.log` por
/// [`recusa_de_midia_dita`].
///
/// # Errors
///
/// [`FalhaNoMod::Recusado`] quando a geração já acabou, quando o pacote é de
/// outro MOD, quando o arquivo não está declarado, quando passa do teto, quando
/// os bytes não são de um formato conhecido, quando são de imagem, ou quando o
/// som, decodificado, passaria do teto do som ([`som_cabe_decodificado`]).
#[tauri::command]
fn som_do_mod(
    app: AppHandle,
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    hash: String,
    caminho: String,
) -> Result<tauri::ipc::Response, FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    let bytes = som_declarado_do_mod(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &config_dir(&app),
        &id,
        &hash,
        &caminho,
    )?;
    som_servido_dito(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &id,
        geracao,
        &caminho,
        bytes.len(),
    );
    Ok(tauri::ipc::Response::new(bytes))
}

/// **Diz no registro que o som de um MOD saiu**, e qual.
///
/// Fora do comando para que um teste leia a linha sem `AppHandle`. O caminho é
/// o nome que o MOD escolheu no manifesto e passa por [`caminho_no_registro`]:
/// escrito cru, um nome com quebra de linha escreveria, a cada som servido, uma
/// linha com a cara do produto. O `mod_id` vai em `%` como está: aqui ele já é
/// o do manifesto ([`bytes_declarados_do_mod`] recusa o hash de outro MOD), e o
/// manifesto só passa com `autor/nome` em minúsculas, dígitos e hífen.
///
/// Com o teto de vazão do MOD ([`linha_admitida`]): o som que um MOD toca a
/// cada evento é pedido a cada vez. No balde das servidas
/// ([`Classe::Servida`]), como a mídia servida.
fn som_servido_dito(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    mod_id: &str,
    geracao: u64,
    caminho: &str,
    bytes: usize,
) {
    let Some(Admitida {
        suprimidas,
        transbordo,
    }) = linha_admitida(baldes, agora, Classe::Servida, mod_id)
    else {
        return;
    };
    tracing::info!(
        mod_id = %mod_id,
        geracao,
        caminho = %caminho_no_registro(caminho),
        bytes,
        suprimidas,
        transbordo,
        "som de MOD servido em bytes"
    );
}

/// Os bytes de um som declarado, **ou a recusa dita no registro**, com o teto
/// de vazão de `baldes` ([`recusa_de_midia_dita`]).
///
/// O som que, decodificado, passa do teto do som é recusado aqui, antes de
/// sair ([`som_cabe_decodificado`]).
fn som_declarado_do_mod(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    pasta: &str,
    id: &str,
    hash: &str,
    caminho: &str,
) -> Result<Vec<u8>, FalhaNoMod> {
    bytes_declarados_do_mod(pasta, id, hash, caminho)
        .and_then(|bytes| match seele_ffi::mods::ler_tipo(&bytes) {
            Some(tipo) if tipo.papel == "som" => som_cabe_decodificado(&bytes).map(|()| bytes),
            Some(_) => Err(FalhaNoMod::Recusado {
                motivo: "nao-e-som".to_owned(),
            }),
            None => Err(FalhaNoMod::Recusado {
                motivo: "formato-desconhecido".to_owned(),
            }),
        })
        .map_err(|falha| recusa_de_midia_dita(baldes, agora, id, caminho, falha))
}

/// **O teto do som decodificado, em bytes** — o mesmo da janela.
///
/// A janela soma, por lugar, o que o WebAudio segura, e recusa o som que passa
/// dele (`LIMITES_DA_REGIAO.bytesDeSomDecodificado` e o do cartão, em
/// `mods-regiao.js`). Aqui ele vale para um som sozinho: um som cujo piso
/// decodificado passa deste número não cabe em lugar nenhum, e a janela o
/// decodificaria inteiro para largá-lo em seguida. O guarda
/// `os_numeros_do_som_decodificado_sao_os_da_janela` confere que os dois
/// lados dizem o mesmo número.
const TETO_DO_SOM_DECODIFICADO: u64 = 64 * 1024 * 1024;

/// **A taxa em que a janela decodifica o som de MOD**, em quadros por
/// segundo — `TAXA_DA_DECODIFICACAO`, em `mods-regiao.js`, conferida pelo
/// mesmo guarda do teto.
const TAXA_DA_DECODIFICACAO: u64 = 48_000;

/// O nome da recusa do som que, decodificado, passa do teto.
///
/// É o começo do `motivo`, e os números vêm depois dele: a janela reconhece a
/// recusa por este nome e a diz como o teto do som, e não como falha de carga
/// (`recusaDoSomGrandeDemais`, em `mods-regiao.js`).
const SOM_GRANDE_DEMAIS_DECODIFICADO: &str = "som-grande-demais-decodificado";

/// **O som que, decodificado, não cabe no teto do som, recusado antes de a
/// janela o decodificar** (P49-7a).
///
/// `decodeAudioData` devolve o som inteiro em float32, de uma vez: a janela só
/// pode contá-lo depois de ele existir, e o pico — dez MiB de MP3 a 128 kbps
/// viram uns 250 MB — já aconteceu. Quando o cabeçalho permite, o piso
/// ([`bytes_decodificados_minimos`]) sai antes de os bytes saírem daqui, e o
/// som acima do teto é recusado com o nome
/// [`SOM_GRANDE_DEMAIS_DECODIFICADO`] e os números.
///
/// **Só o piso recusa.** Um som cujo cabeçalho não diz o bastante segue como
/// antes: a janela decodifica e confere o teto dela.
fn som_cabe_decodificado(bytes: &[u8]) -> Result<(), FalhaNoMod> {
    match bytes_decodificados_minimos(bytes) {
        Some(piso) if piso > TETO_DO_SOM_DECODIFICADO => Err(FalhaNoMod::Recusado {
            motivo: format!(
                "{SOM_GRANDE_DEMAIS_DECODIFICADO}: decodificado a {TAXA_DA_DECODIFICACAO} Hz, o \
                 som ocupa ao menos {piso} bytes, acima do teto de {TETO_DO_SOM_DECODIFICADO} \
                 bytes de som"
            ),
        }),
        _ => Ok(()),
    }
}

/// **Um piso do que um som ocupa decodificado pela janela**, em bytes: os
/// quadros a [`TAXA_DA_DECODIFICACAO`], vezes os canais, vezes quatro — o
/// float32 que o `AudioBuffer` segura.
///
/// **Um piso, e nunca uma estimativa que possa passar do real** num arquivo
/// bem formado: só ele recusa ([`som_cabe_decodificado`]). Um MP3 com mais
/// quadros corrompidos do que o corte absorve é o caso em que ele passa — ver
/// [`cabecalho_de_som::mp3`]. O cabeçalho é lido à mão, sem crate de áudio
/// ([`cabecalho_de_som`]):
///
/// - **WAV**, exato, pelo `fmt ` e pelo `data`, com o `data` limitado aos
///   bytes que estão no arquivo e ao tamanho que o RIFF declara;
/// - **Ogg**, pelas páginas do fluxo Vorbis ou Opus: a granule da última,
///   menos a da primeira de dados — e, no Opus, que já conta a 48 kHz, menos o
///   pre-skip;
/// - **MP3**, pelos quadros que estão no arquivo, encadeados desde o primeiro,
///   e não mais do que o quadro Xing/Info conta quando há um.
///
/// Um quadro sai da conta por arredondamento: a conversão de taxa do
/// navegador conta em ponto flutuante, e pode ficar um quadro abaixo do
/// quociente exato.
///
/// `None` quando o cabeçalho não diz o bastante: um WAV comprimido, um Ogg de
/// outro codec, um MP3 de formato livre, um arquivo cortado antes do que a
/// leitura precisa, uma taxa ou um número de canais fora do que o WebAudio
/// decodifica. O som segue como antes, para a janela decodificar e conferir.
fn bytes_decodificados_minimos(bytes: &[u8]) -> Option<u64> {
    let piso = match seele_ffi::mods::ler_tipo(bytes)?.media_type {
        "audio/wav" => cabecalho_de_som::wav(bytes)?,
        "audio/ogg" => cabecalho_de_som::ogg(bytes)?,
        "audio/mpeg" => cabecalho_de_som::mp3(bytes)?,
        _ => return None,
    };
    // A faixa que o WebAudio aceita num `AudioBuffer`: fora dela, a janela
    // não decodifica, e o piso não teria do que ser piso.
    if !(3_000..=768_000).contains(&piso.taxa) || !(1..=32).contains(&piso.canais) {
        return None;
    }
    let quadros =
        u128::from(piso.amostras) * u128::from(TAXA_DA_DECODIFICACAO) / u128::from(piso.taxa);
    let bytes = quadros.saturating_sub(1) * u128::from(piso.canais) * 4;
    Some(u64::try_from(bytes).unwrap_or(u64::MAX))
}

/// **A leitura à mão dos cabeçalhos de som**, para
/// [`bytes_decodificados_minimos`].
///
/// Sem crate de áudio, e sem índice que entre em pânico: cada leitura passa
/// por `get`, e o que não está nos bytes é `None`. Cada leitura devolve um
/// piso dos quadros que o decodificador entrega: o que ela não sabe ao certo
/// conta para menos.
mod cabecalho_de_som {
    /// Um som como o cabeçalho o diz.
    pub(super) struct Piso {
        /// Um piso dos quadros, na taxa do arquivo.
        pub(super) amostras: u64,
        /// Quadros por segundo, no arquivo.
        pub(super) taxa: u32,
        /// Canais que o decodificador entrega.
        pub(super) canais: u32,
    }

    fn u16_le(bytes: &[u8], em: usize) -> Option<u16> {
        Some(u16::from_le_bytes(
            bytes.get(em..em.checked_add(2)?)?.try_into().ok()?,
        ))
    }

    fn u32_le(bytes: &[u8], em: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(em..em.checked_add(4)?)?.try_into().ok()?,
        ))
    }

    fn u32_be(bytes: &[u8], em: usize) -> Option<u32> {
        Some(u32::from_be_bytes(
            bytes.get(em..em.checked_add(4)?)?.try_into().ok()?,
        ))
    }

    fn u64_le(bytes: &[u8], em: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(em..em.checked_add(8)?)?.try_into().ok()?,
        ))
    }

    /// **WAV, exato**: os pedaços do RIFF em ordem, o `fmt ` antes do `data`.
    ///
    /// O tamanho que o `data` declara é limitado aos bytes que estão no
    /// arquivo: um WAV gravado aos poucos leva ali `0xFFFFFFFF`, e um cortado,
    /// mais do que tem. **E ao tamanho que o RIFF declara**, salvo quando o
    /// RIFF também foi gravado aos poucos (`0xFFFFFFFF`): o WebKit para onde o
    /// RIFF diz que o arquivo acaba, e o que vem depois não é som para ele.
    ///
    /// **Um segundo `fmt ` dá `None`.** O Chromium decodifica pelo primeiro, e
    /// qual deles vale é pergunta de motor: um piso não escolhe.
    pub(super) fn wav(bytes: &[u8]) -> Option<Piso> {
        let riff = u32_le(bytes, 4)?;
        let bytes = if riff == u32::MAX {
            bytes
        } else {
            let fim = usize::try_from(riff).ok()?.checked_add(8)?;
            bytes.get(..fim.min(bytes.len()))?
        };
        let mut resto = bytes.get(12..)?;
        let mut formato = None;
        loop {
            let id = resto.get(..4)?;
            let tamanho = usize::try_from(u32_le(resto, 4)?).ok()?;
            let corpo = resto.get(8..)?;
            if id == b"data" {
                let (canais, taxa, por_quadro) = formato?;
                let quadros = tamanho.min(corpo.len()) / por_quadro;
                return Some(Piso {
                    amostras: u64::try_from(quadros).ok()?,
                    taxa,
                    canais,
                });
            }
            if id == b"fmt " {
                if formato.is_some() {
                    return None;
                }
                formato = Some(fmt(corpo.get(..tamanho)?)?);
            }
            // Um pedaço de tamanho ímpar leva um byte de alinhamento.
            resto = corpo.get(tamanho.checked_add(tamanho % 2)?..)?;
        }
    }

    /// Canais, taxa e bytes por quadro de um `fmt `, só dos formatos em que
    /// um quadro é um bloco: PCM, float, A-law e µ-law, direto ou dentro do
    /// `WAVE_FORMAT_EXTENSIBLE`. Os comprimidos ficam de fora.
    ///
    /// O quadro conta pelo maior dos dois tamanhos que o cabeçalho permite
    /// ler — o bloco declarado e os canais vezes os bytes da amostra —, para
    /// que um cabeçalho que discorde de si mesmo dê menos quadros, e não mais.
    fn fmt(fmt: &[u8]) -> Option<(u32, u32, usize)> {
        let codigo = match u16_le(fmt, 0)? {
            0xFFFE => u16_le(fmt, 24)?,
            codigo => codigo,
        };
        if !matches!(codigo, 1 | 3 | 6 | 7) {
            return None;
        }
        let canais = u16_le(fmt, 2)?;
        let taxa = u32_le(fmt, 4)?;
        let bloco = usize::from(u16_le(fmt, 12)?);
        let amostra = usize::from(u16_le(fmt, 14)?).div_ceil(8);
        let por_quadro = bloco.max(usize::from(canais).checked_mul(amostra)?);
        (canais > 0 && por_quadro > 0).then_some((u32::from(canais), taxa, por_quadro))
    }

    /// Uma página Ogg, lida pelo cabeçalho de 27 bytes e pela tabela de
    /// segmentos.
    struct PaginaOgg<'a> {
        primeira: bool,
        ultima: bool,
        /// A posição ao fim da página, ou nada quando nenhum pacote termina
        /// nela (`-1`).
        granule: Option<u64>,
        serial: u32,
        corpo: &'a [u8],
        depois: &'a [u8],
    }

    fn pagina_ogg(bytes: &[u8]) -> Option<PaginaOgg<'_>> {
        if bytes.get(..4)? != b"OggS" || *bytes.get(4)? != 0 {
            return None;
        }
        let tipo = *bytes.get(5)?;
        let granule = u64_le(bytes, 6)?;
        let serial = u32_le(bytes, 14)?;
        let segmentos = usize::from(*bytes.get(26)?);
        let inicio = 27 + segmentos;
        let tamanho: usize = bytes.get(27..inicio)?.iter().map(|&s| usize::from(s)).sum();
        let fim = inicio + tamanho;
        Some(PaginaOgg {
            primeira: tipo & 0x02 != 0,
            ultima: tipo & 0x04 != 0,
            granule: (granule != u64::MAX).then_some(granule),
            serial,
            corpo: bytes.get(inicio..fim)?,
            depois: bytes.get(fim..)?,
        })
    }

    /// **Ogg, pelas páginas**, seguidas pelo tamanho de cada uma desde a
    /// primeira — e não por uma busca de `OggS`, que acharia a marca dentro dos
    /// dados.
    ///
    /// A primeira página é a da identificação, Vorbis ou Opus. O piso é a
    /// granule da última página do fluxo menos a da primeira de dados: um
    /// fluxo que não começa do zero — um pedaço de transmissão — decodifica
    /// menos do que a última granule diz. No Opus sai também o pre-skip, e a
    /// taxa é sempre 48 kHz: a do cabeçalho é a da entrada, e só informa.
    pub(super) fn ogg(bytes: &[u8]) -> Option<Piso> {
        let identificacao = pagina_ogg(bytes)?;
        if !identificacao.primeira {
            return None;
        }
        let pacote = identificacao.corpo;
        let (taxa, canais, pre_skip) = if pacote.starts_with(b"\x01vorbis") {
            if u32_le(pacote, 7)? != 0 {
                return None;
            }
            (u32_le(pacote, 12)?, u32::from(*pacote.get(11)?), 0)
        } else if pacote.starts_with(b"OpusHead") {
            // A versão de cima diferente de zero é um Opus incompatível.
            if *pacote.get(8)? >= 16 {
                return None;
            }
            (
                48_000,
                u32::from(*pacote.get(9)?),
                u64::from(u16_le(pacote, 10)?),
            )
        } else {
            return None;
        };
        let (mut primeira, mut ultima) = (None, None);
        let mut resto = identificacao.depois;
        while let Some(pagina) = pagina_ogg(resto) {
            resto = pagina.depois;
            if pagina.serial != identificacao.serial {
                continue;
            }
            if let Some(granule) = pagina.granule.filter(|&granule| granule > 0) {
                primeira.get_or_insert(granule);
                ultima = Some(granule);
            }
            if pagina.ultima {
                break;
            }
        }
        let comeco = primeira?.max(pre_skip);
        Some(Piso {
            amostras: ultima?.saturating_sub(comeco),
            taxa,
            canais,
        })
    }

    /// O que o cabeçalho de quatro bytes de um quadro MPEG áudio diz.
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct QuadroMpeg {
        mpeg1: bool,
        camada: u8,
        taxa: u32,
        canais: u32,
        amostras: u64,
        tamanho: usize,
    }

    /// As taxas de bits, em kbps, pelo índice do cabeçalho. O zero é o
    /// formato livre, que não diz o tamanho do quadro.
    #[rustfmt::skip]
    const KBPS: [[u16; 15]; 5] = [
        // MPEG-1, camadas I, II e III.
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
        // MPEG-2 e 2.5, camada I; e camadas II e III.
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    ];

    fn quadro_mpeg(bytes: &[u8]) -> Option<QuadroMpeg> {
        let [b0, b1, b2, b3]: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        if b0 != 0xFF || b1 & 0xE0 != 0xE0 {
            return None;
        }
        // Versão: `11` é MPEG-1, `10` MPEG-2, `00` MPEG-2.5, `01` reservada.
        let versao = (b1 >> 3) & 0b11;
        let camada = match (b1 >> 1) & 0b11 {
            0b01 => 3,
            0b10 => 2,
            0b11 => 1,
            _ => return None,
        };
        let mpeg1 = versao == 0b11;
        let tabela = match (mpeg1, camada) {
            (true, camada) => usize::from(camada - 1),
            (false, 1) => 3,
            (false, _) => 4,
        };
        let kbps = *KBPS.get(tabela)?.get(usize::from(b2 >> 4))?;
        if kbps == 0 {
            return None;
        }
        let taxa = match (versao, (b2 >> 2) & 0b11) {
            (0b11, 0) => 44_100,
            (0b11, 1) => 48_000,
            (0b11, 2) => 32_000,
            (0b10, 0) => 22_050,
            (0b10, 1) => 24_000,
            (0b10, 2) => 16_000,
            (0b00, 0) => 11_025,
            (0b00, 1) => 12_000,
            (0b00, 2) => 8_000,
            _ => return None,
        };
        let enchimento = u64::from((b2 >> 1) & 1);
        let amostras: u64 = match (camada, mpeg1) {
            (1, _) => 384,
            (2, _) | (3, true) => 1152,
            _ => 576,
        };
        let bits = u64::from(kbps) * 1000;
        let tamanho = if camada == 1 {
            (12 * bits / u64::from(taxa) + enchimento) * 4
        } else {
            amostras / 8 * bits / u64::from(taxa) + enchimento
        };
        Some(QuadroMpeg {
            mpeg1,
            camada,
            taxa,
            canais: if b3 >> 6 == 0b11 { 1 } else { 2 },
            amostras,
            tamanho: usize::try_from(tamanho)
                .ok()
                .filter(|&tamanho| tamanho > 4)?,
        })
    }

    /// Quantos quadros o quadro Xing/Info diz que o arquivo tem, quando o
    /// primeiro quadro é um e traz a contagem.
    fn quadros_do_xing(quadro: &[u8], cabecalho: QuadroMpeg) -> Option<u64> {
        if cabecalho.camada != 3 {
            return None;
        }
        // A marca vem depois da informação lateral, que depende da versão e
        // dos canais.
        let em = 4 + match (cabecalho.mpeg1, cabecalho.canais) {
            (true, 1) | (false, 2) => 17,
            (true, _) => 32,
            (false, _) => 9,
        };
        let marca = quadro.get(em..em + 4)?;
        if marca != b"Xing" && marca != b"Info" {
            return None;
        }
        if u32_be(quadro, em + 4)? & 1 == 0 {
            return None;
        }
        Some(u64::from(u32_be(quadro, em + 8)?))
    }

    /// **MP3, pelos quadros que estão no arquivo.**
    ///
    /// Depois das etiquetas ID3v2 do começo, o primeiro quadro tem de estar
    /// ali; dele em diante, cada quadro inteiro com a mesma versão, camada,
    /// taxa e canais conta, e a cadeia para no primeiro que não for um — uma
    /// etiqueta no fim, um arquivo cortado. Contar quadros, e não bytes pela
    /// taxa de bits mais alta, deixa de fora o que não é som: uma etiqueta
    /// APE com a capa no fim do arquivo não vira minutos de som.
    ///
    /// **O quadro Xing/Info limita a conta**: é o que o codificador escreveu,
    /// e o piso fica no menor dos dois. Um download interrompido tem um Xing
    /// maior que o arquivo, e a cadeia o segura; uma cadeia que seguisse além
    /// do que o codificador contou, o Xing a segura.
    ///
    /// **E o que o decodificador pode cortar sai**: o próprio quadro
    /// Xing/Info, o atraso e o enchimento que a etiqueta do LAME declara (até
    /// 4095 amostras cada), e o atraso do decodificador (529).
    ///
    /// **O «nunca passa do real» vale para um fluxo bem formado.** Um quadro
    /// com o cabeçalho certo e o conteúdo corrompido entra na cadeia, e o
    /// decodificador não o entrega. O Chromium o descarta. O WebKit, conforme
    /// o defeito, recusa o arquivo inteiro — medido com dez quadros
    /// corrompidos seguidos — ou descarta os quadros como o Chromium — com
    /// dez espalhados. O corte acima absorve uns oito quadros de 1152 amostras
    /// (9871 amostras): com um corrompido, o piso fica abaixo do real nos dois
    /// motores. Com mais que isso, e o arquivo decodificado, o piso pode
    /// passar do real **nos dois** — a revisão mediu, com dez espalhados no
    /// `City_Night_Crowd`, 18 952 bytes a mais no WebKit e 14 352 no
    /// Chromium —, e o som pode ser recusado pelo teto que, decodificado, ele
    /// talvez não passasse.
    pub(super) fn mp3(bytes: &[u8]) -> Option<Piso> {
        let mut inicio = 0usize;
        while bytes.get(inicio..)?.starts_with(b"ID3") {
            let etiqueta = bytes.get(inicio..inicio.checked_add(10)?)?;
            // O tamanho em quatro bytes de sete bits, sem o cabeçalho, e o
            // rodapé quando a bandeira o diz.
            let tamanho = etiqueta.get(6..10)?.iter().try_fold(0usize, |soma, &b| {
                (b < 0x80).then_some((soma << 7) | usize::from(b))
            })?;
            let rodape = if etiqueta.get(5)? & 0x10 != 0 { 10 } else { 0 };
            inicio = inicio.checked_add(10 + tamanho + rodape)?;
        }
        let primeiro = quadro_mpeg(bytes.get(inicio..)?)?;
        let mut quadros = 0u64;
        let mut em = inicio;
        while let Some(quadro) = bytes.get(em..).and_then(quadro_mpeg) {
            let mesmo = (quadro.mpeg1, quadro.camada, quadro.taxa, quadro.canais)
                == (
                    primeiro.mpeg1,
                    primeiro.camada,
                    primeiro.taxa,
                    primeiro.canais,
                );
            let fim = em.checked_add(quadro.tamanho)?;
            if !mesmo || fim > bytes.len() {
                break;
            }
            quadros += 1;
            em = fim;
        }
        if let Some(contados) = quadros_do_xing(bytes.get(inicio..)?, primeiro) {
            quadros = quadros.min(contados);
        }
        let corte = primeiro.amostras + 2 * 4095 + 529;
        Some(Piso {
            amostras: quadros
                .saturating_mul(primeiro.amostras)
                .saturating_sub(corte),
            taxa: primeiro.taxa,
            canais: primeiro.canais,
        })
    }
}

/// **Mídia que veio da metade de servidor de um MOD.**
///
/// A do pacote é [`midia_do_mod`]; esta é a que o MOD guarda no servidor — a
/// cena de um tabuleiro, o retrato de um perfil. O caminho é diferente e a
/// regra é a mesma: **o tipo vem dos bytes**. Aqui isso importa ainda mais,
/// porque estes bytes vieram pela ponte de pedidos, e quem os pôs lá foi um
/// terceiro — pode ter sido a pessoa que usa, pode ter sido outra.
///
/// Os bytes **não passam pela fila do MOD**. Quem pede é a janela, com o `id`
/// deste MOD, e a resposta vai direto do servidor para cá: uma imagem não cabe
/// numa declaração de região, e fazê-la caber seria alargar o teto de mensagem
/// para todo mundo por causa de um caso.
///
/// Toda recusa, exceto a de geração morta (contada em
/// `comandos_de_geracao_morta`), vai ao `seele.log` por
/// [`recusa_de_midia_dita`], com o id do MOD que a janela manda.
///
/// # Errors
///
/// [`FalhaNoMod`] quando a geração já acabou, quando o texto não é base64,
/// quando passa do teto, ou quando os bytes não são de um formato conhecido.
///
/// **`uso`** nomeia o lugar em que a imagem vai aparecer (`"capa"`), e ela
/// chega reduzida a ele (`reducao`; tarefa 6, Step 5). Fora da thread da
/// janela (`async`): reduzir uma animação de 60 quadros leva tempo, e um
/// comando síncrono do Tauri roda na thread principal.
#[tauri::command(async)]
fn midia_em_bytes(
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    base64: String,
    uso: Option<String>,
) -> Result<MidiaDoMod, FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    let midia = midia_do_servidor_dita(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &id,
        &base64,
        uso.as_deref().and_then(reducao::teto_do_uso).as_ref(),
    )?;
    midia_vinda_do_servidor_dita(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &id,
        geracao,
        midia.papel,
        midia.bytes,
    );
    Ok(midia)
}

/// **Diz no registro que a mídia que um MOD guardou no servidor chegou**, em
/// DEBUG.
///
/// Fora do comando para que um teste leia a linha sem sessão. O id chega da
/// janela, e passa por [`id_no_registro`].
///
/// **Com o teto de vazão do MOD** ([`linha_admitida`]), no balde da
/// depuração ([`Classe::Depuracao`]). É o par bem-sucedido da recusa de
/// [`midia_do_servidor_dita`]: com `RUST_LOG=debug`, um MOD que pede a mesma
/// mídia num laço escrevia uma linha por pedido. Na depuração, e não nas
/// servidas, porque sai em DEBUG: o filtro padrão do `seele.log` não a
/// escreve, e ela não pode gastar a ficha de uma linha que ele escreve.
fn midia_vinda_do_servidor_dita(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    mod_id: &str,
    geracao: u64,
    papel: &str,
    bytes: usize,
) {
    let Some(Admitida {
        suprimidas,
        transbordo,
    }) = linha_admitida(baldes, agora, Classe::Depuracao, mod_id)
    else {
        return;
    };
    tracing::debug!(
        mod_id = %id_no_registro(mod_id),
        geracao,
        papel,
        bytes,
        suprimidas,
        transbordo,
        "mídia de MOD vinda do servidor"
    );
}

/// A mídia vinda do servidor de um MOD, **ou a recusa dita no registro** com o
/// id dele.
///
/// O `warn` que estava no comando não dizia de quem era a mídia: a janela não
/// mandava o id, e «mídia de MOD recusada» com três MODs ligados não aponta
/// para nenhum.
///
/// Com o teto de vazão de `baldes` ([`recusa_de_midia_dita`]).
fn midia_do_servidor_dita(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    id: &str,
    base64: &str,
    teto: Option<&reducao::Teto>,
) -> Result<MidiaDoMod, FalhaNoMod> {
    ler_midia_do_servidor(base64, teto)
        .map_err(|falha| recusa_de_midia_dita(baldes, agora, id, "servidor", falha))
}

fn ler_midia_do_servidor(
    base64: &str,
    teto: Option<&reducao::Teto>,
) -> Result<MidiaDoMod, FalhaNoMod> {
    // Conferido **antes** de decodificar: o texto cresce um terço sobre os
    // bytes, então o teto do texto é o teto dos bytes com folga, e recusar aqui
    // é recusar sem alocar.
    if base64.len() > seele_ffi::mods::TETO_DE_MIDIA * 2 {
        return Err(FalhaNoMod::Recusado {
            motivo: "arquivo-grande-demais".to_owned(),
        });
    }
    // O cabeçalho delimita o conteúdo; o tipo é provado pelos bytes abaixo.
    let conteudo = if let Some(uri) = base64.strip_prefix("data:") {
        let (cabecalho, conteudo) = uri.split_once(',').ok_or(FalhaNoMod::Recusado {
            motivo: "nao-e-base64".to_owned(),
        })?;
        if cabecalho.len() > 128 || !cabecalho.ends_with(";base64") {
            return Err(FalhaNoMod::Recusado {
                motivo: "nao-e-base64".to_owned(),
            });
        }
        conteudo
    } else {
        base64
    };
    let bytes = seele_ffi::de_base64(conteudo).ok_or(FalhaNoMod::Recusado {
        motivo: "nao-e-base64".to_owned(),
    })?;
    if bytes.len() > seele_ffi::mods::TETO_DE_MIDIA {
        return Err(FalhaNoMod::Recusado {
            motivo: "arquivo-grande-demais".to_owned(),
        });
    }
    // **O som do servidor tem o mesmo pico do som do pacote**: a janela o lê
    // do `data:` (`bufferDaUri`) e o decodifica inteiro. Recusado antes do
    // `data:`, que seria o arquivo em base64 para ninguém.
    if seele_ffi::mods::ler_tipo(&bytes).is_some_and(|tipo| tipo.papel == "som") {
        som_cabe_decodificado(&bytes)?;
    }
    let (bytes, decodificados) = reduzir_ao_uso(bytes, teto)?;
    let lida = seele_ffi::mods::ler_midia(&bytes).ok_or(FalhaNoMod::Recusado {
        motivo: "formato-desconhecido".to_owned(),
    })?;
    Ok(MidiaDoMod {
        uri: Some(lida.uri),
        papel: lida.papel,
        bytes: lida.bytes,
        decodificados,
    })
}

/// **Uma imagem reduzida ao uso que a janela nomeou** (`reducao`; rascunho da
/// API 6, §4; tarefa 6, Step 5), e quanto ela pesa decodificada. Sem uso, ou
/// para um som, os bytes seguem como vieram.
///
/// # Errors
///
/// `formato-desconhecido`, quando a imagem não decodifica.
fn reduzir_ao_uso(
    bytes: Vec<u8>,
    teto: Option<&reducao::Teto>,
) -> Result<(Vec<u8>, Option<u64>), FalhaNoMod> {
    let Some(teto) = teto else {
        return Ok((bytes, None));
    };
    if !seele_ffi::mods::ler_tipo(&bytes).is_some_and(|tipo| tipo.papel == "imagem") {
        return Ok((bytes, None));
    }
    let reduzida =
        reducao::reduzir(&bytes, teto).map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
    Ok((reduzida.bytes, Some(reduzida.decodificados)))
}

/// Os bytes escolhidos seguem direto da memória nativa para um fluxo autenticado.
#[tauri::command]
async fn enviar_imagem_mod(
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    arquivo: u64,
    token: String,
) -> Result<(), FalhaNoMod> {
    let recusa = |motivo: &str| FalhaNoMod::Recusado {
        motivo: motivo.to_owned(),
    };
    session.recusar_se_morta(geracao)?;
    let bytes = {
        let mut guardados = session
            .arquivos_escolhidos
            .lock()
            .map_err(|_| recusa("banco-indisponivel"))?;
        let escolhido = guardados
            .guardados
            .get(&arquivo)
            .ok_or_else(|| recusa("arquivo-nao-esta-de-pe"))?;
        if escolhido.de_quem != id || escolhido.geracao != geracao {
            return Err(recusa("arquivo-de-outro-mod"));
        }
        guardados
            .guardados
            .remove(&arquivo)
            .ok_or_else(|| recusa("arquivo-nao-esta-de-pe"))?
            .bytes
    };
    let conexao = session
        .connection()
        .map_err(|_| recusa("sessao-encerrada"))?;
    conexao
        .enviar_imagem_mod(id, token, bytes)
        .await
        .map_err(|m| recusa(&m))?;
    session.recusar_se_morta(geracao)?;
    Ok(())
}

/// **Lê por volume a imagem que um MOD guardou no servidor.**
///
/// Toda recusa é dita no `seele.log`: a geração morta, na entrada e — quando
/// os bytes chegaram — depois do `await`, por [`Session::recusar_se_morta`],
/// que a conta; a conexão que não está de pé, por [`recusa_de_midia_dita`], em
/// DEBUG; e as duas de depois do `await` — a do servidor e
/// `formato-desconhecido` — por [`imagem_lida_dita`], com o id do MOD.
///
/// # Errors
///
/// [`FalhaNoMod::Recusado`] quando a geração já acabou, quando não há conexão,
/// com o texto que o servidor mandou, ou quando os bytes não são de um formato
/// conhecido.
#[tauri::command]
async fn ler_imagem_mod(
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    channel: u32,
    payload: String,
    uso: Option<String>,
) -> Result<MidiaDoMod, FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    // **Dita, e não contada.** A geração foi conferida logo acima: uma conexão
    // ausente com a geração de pé não é comando de uma sessão encerrada, e
    // contá-la faria o contador dizer que a sessão anterior falou sem ela ter
    // falado.
    let conexao = session.connection().map_err(|_| {
        recusa_de_midia_dita(
            &session.baldes_do_registro,
            std::time::Instant::now(),
            &id,
            "volume",
            FalhaNoMod::Recusado {
                motivo: "sessao-encerrada".to_owned(),
            },
        )
    })?;
    let lida = conexao.ler_imagem_mod(id.clone(), channel, payload).await;
    // **A ordem de sempre.** O `Err` do servidor é dito como veio, mesmo que a
    // geração tenha morrido durante o `await`: ele é o dado da recusa, e a
    // janela continua recebendo o que o servidor disse. Só os bytes que
    // chegaram esperam a conferência, porque decodificá-los já seria trabalho
    // para uma sessão que acabou.
    if lida.is_ok() {
        session.recusar_se_morta(geracao)?;
    }
    // O relógio é lido quando a linha é escrita, e não antes do `await`: a
    // leitura por volume pode levar segundos, e o balde contaria a recarga
    // desde um instante que já passou.
    // Reduzida ao `uso`, quando a janela o nomeia (`reducao`; tarefa 6, Step 5).
    imagem_lida_dita(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &id,
        lida,
        uso.as_deref().and_then(reducao::teto_do_uso).as_ref(),
    )
}

/// **A imagem lida por volume, ou a recusa dita no registro** com o id do MOD:
/// a parte de [`ler_imagem_mod`] depois do `await`.
///
/// Três recusas, todas por [`recusa_de_midia_dita`] com a origem `volume`:
///
/// - a do servidor, com o texto que ele mandou — o `erro` da resposta, a razão
///   com que a conexão fechou, o prazo esgotado;
/// - a dos bytes que chegaram e não são de um formato que este produto
///   decodifica, `formato-desconhecido`;
/// - a do som que, decodificado, passa do teto do som
///   ([`som_cabe_decodificado`]): a leitura por volume também chega à janela
///   pelo caminho do som do servidor, o `data:` lido por `bufferDaUri`, e tem o
///   mesmo pico.
///
/// Extraída porque o comando fala com a rede e não sobe num teste: o guarda de
/// texto que procurava `recusa_de_midia_dita` no comando passava com uma só das
/// duas, e a outra saía calada. Aqui cada uma tem o seu caso.
///
/// Com o teto de vazão de `baldes`.
fn imagem_lida_dita(
    baldes: &BaldesDoRegistro,
    agora: std::time::Instant,
    id: &str,
    lida: Result<Vec<u8>, String>,
    teto: Option<&reducao::Teto>,
) -> Result<MidiaDoMod, FalhaNoMod> {
    let bytes = lida.map_err(|motivo| {
        recusa_de_midia_dita(baldes, agora, id, "volume", FalhaNoMod::Recusado { motivo })
    })?;
    if seele_ffi::mods::ler_tipo(&bytes).is_some_and(|tipo| tipo.papel == "som") {
        som_cabe_decodificado(&bytes)
            .map_err(|falha| recusa_de_midia_dita(baldes, agora, id, "volume", falha))?;
    }
    let (bytes, decodificados) = reduzir_ao_uso(bytes, teto)
        .map_err(|falha| recusa_de_midia_dita(baldes, agora, id, "volume", falha))?;
    let midia = seele_ffi::mods::ler_midia(&bytes).ok_or_else(|| {
        recusa_de_midia_dita(
            baldes,
            agora,
            id,
            "volume",
            FalhaNoMod::Recusado {
                motivo: "formato-desconhecido".to_owned(),
            },
        )
    })?;
    Ok(MidiaDoMod {
        uri: Some(midia.uri),
        papel: midia.papel,
        bytes: midia.bytes,
        decodificados,
    })
}

/// **O tipo de um arquivo que uma pessoa escolheu para um MOD**, ou a recusa
/// com nome.
///
/// Do que os bytes são, e nunca da extensão. Um formato que o produto
/// reconhece e não aceita — o HEIC das Fotos — é recusado pelo nome dele,
/// `tipo-recusado:heic` (rascunho da API 6, §7.1), no molde do
/// `papel-nao-serve:<papel>`; o resto que não é um dos formatos é
/// `formato-desconhecido`.
fn tipo_do_escolhido(bytes: &[u8]) -> Result<seele_ffi::mods::TipoLido, FalhaNoMod> {
    seele_ffi::mods::ler_tipo(bytes).ok_or_else(|| FalhaNoMod::Recusado {
        motivo: seele_ffi::mods::reconhecido_e_nao_aceito(bytes).map_or_else(
            || "formato-desconhecido".to_owned(),
            |formato| format!("tipo-recusado:{formato}"),
        ),
    })
}

/// O que a janela recebe quando uma pessoa escolhe um arquivo para um MOD.
#[derive(serde::Serialize)]
struct EscolhidoParaOMod {
    /// O número pelo qual o MOD pede os pedaços. Nunca se repete nesta janela.
    id: u64,
    /// O que os bytes provaram ser — `image/png`, `audio/wav`, e assim por
    /// diante. **Nunca a extensão**: ela é texto que quem escolheu digitou.
    tipo: &'static str,
    /// `som` ou `imagem`.
    papel: &'static str,
    /// Quantos bytes tem. O MOD usa para saber quantos pedaços pedir.
    bytes: usize,
}

/// **Uma pessoa escolhe um arquivo para um MOD, e o MOD recebe um número.**
///
/// O seletor é o do sistema, aberto por este processo. O caminho no disco
/// **não atravessa**: o que volta é um identificador, o tipo que os bytes
/// provaram ser e o tamanho. Os bytes ficam na sessão e saem em pedaços por
/// [`pedaco_do_escolhido`].
///
/// É a diferença entre mediar e entregar o disco. Um MOD não lista pasta, não
/// abre caminho e não recebe nome de arquivo — ele recebe o que a pessoa
/// escolheu, depois de o produto ter olhado.
///
/// # Errors
///
/// [`FalhaNoMod`] quando a geração já acabou, quando já há arquivos demais de
/// pé, quando o arquivo passa do teto, ou quando os bytes não são de um
/// formato que este produto reconhece.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn escolher_para_o_mod(
    app: AppHandle,
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    finalidade: Option<String>,
    papeis: Option<Vec<String>>,
    extensoes: Option<Vec<String>>,
    limite_de_bytes: Option<usize>,
) -> Result<Option<EscolhidoParaOMod>, FalhaNoMod> {
    use tauri_plugin_dialog::DialogExt as _;

    session.recusar_se_morta(geracao)?;

    // **O título diz para quê.** Antes dizia «Escolha um arquivo para este
    // MOD», que não responde a pergunta de quem está olhando o Finder: *este
    // arquivo vai virar o quê?* A auditoria de 20/09/2026 abriu o seletor de
    // avatar do PERFIS e não teve como saber que era o avatar.
    //
    // O texto vem do MOD, então ele é cortado e limpo de quebras de linha: um
    // título de diálogo do sistema não é onde se escreve um parágrafo.
    let proposito: String = finalidade
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    let titulo = if proposito.trim().is_empty() {
        "Escolha um arquivo para este MOD".to_owned()
    } else {
        proposito.trim().to_owned()
    };

    // **O filtro é orientação, e não a fronteira.** A prova do tipo continua
    // saindo dos bytes, mais abaixo; o que ele evita é oferecer quarenta JSONs
    // a quem foi buscar um retrato. Extensões que o produto não conhece são
    // descartadas: um MOD não escolhe o que o seletor da máquina de outra
    // pessoa aceita.
    const EXTENSOES_CONHECIDAS: &[&str] = &[
        "png", "jpg", "jpeg", "webp", "gif", "wav", "mp3", "ogg", "flac", "m4a",
    ];
    let aceitas: Vec<&str> = extensoes
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            EXTENSOES_CONHECIDAS
                .iter()
                .find(|c| c.eq_ignore_ascii_case(e))
                .copied()
        })
        .collect();

    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    let mut dialogo = app.dialog().file().set_title(&titulo);
    if !aceitas.is_empty() {
        dialogo = dialogo.add_filter("Aceitos por este MOD", &aceitas);
    }
    dialogo.pick_file(move |escolha| {
        let _ = envia.try_send(escolha);
    });
    // **Cancelar é uma resposta.** Quem fecha o seletor sem escolher recebe
    // `None`, e o MOD sabe que foi cancelado em vez de esperar para sempre.
    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(None);
    };
    let Ok(caminho) = escolha.into_path() else {
        return Err(FalhaNoMod::Recusado {
            motivo: "caminho-invalido".to_owned(),
        });
    };

    // **Conferido antes de ler.** Um arquivo de gigabytes não vira erro depois
    // de estar na memória: o tamanho é lido do sistema de arquivos primeiro.
    let tamanho = std::fs::metadata(&caminho)
        .map(|m| usize::try_from(m.len()).unwrap_or(usize::MAX))
        .map_err(|_| FalhaNoMod::Recusado {
            motivo: "arquivo-ilegivel".to_owned(),
        })?;
    // O teto do produto vale sempre; o que o MOD declarou só o aperta. Um MOD
    // não amplia o que esta máquina aceita ler.
    let teto = limite_de_bytes
        .filter(|n| *n > 0)
        .map_or(TETO_DO_ESCOLHIDO, |n| n.min(TETO_DO_ESCOLHIDO));
    if tamanho > teto {
        return Err(FalhaNoMod::Recusado {
            motivo: "arquivo-grande-demais".to_owned(),
        });
    }
    let bytes = std::fs::read(&caminho).map_err(|_| FalhaNoMod::Recusado {
        motivo: "arquivo-ilegivel".to_owned(),
    })?;

    // **O tipo sai dos bytes**, como o da mídia do pacote. A extensão que a
    // pessoa tem no disco é texto, e texto não escolhe decodificador.
    let lida = tipo_do_escolhido(&bytes)?;

    // **E o papel provado é conferido contra o que o MOD pediu.** Sem isto, um
    // MOD que precisa de retrato recebia um som e só descobria ao tentar usá-lo
    // — quando descobria. O erro tem nome próprio para a região poder dizê-lo.
    if let Some(pedidos) = papeis.as_ref().filter(|p| !p.is_empty()) {
        if !pedidos.iter().any(|p| p == lida.papel) {
            return Err(FalhaNoMod::Recusado {
                motivo: format!("papel-nao-serve:{}", lida.papel),
            });
        }
    }

    // Conferida **de novo** depois do `await` do seletor: a pessoa pode ter
    // demorado a escolher, e a sessão pode ter acabado no meio.
    session.recusar_se_morta(geracao)?;
    let mut guardados = session
        .arquivos_escolhidos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    let numero = guardados
        .guardar(ArquivoEscolhidoDeMod {
            de_quem: id.clone(),
            geracao,
            tipo: lida.media_type,
            bytes,
        })
        .ok_or(FalhaNoMod::Recusado {
            motivo: "arquivos-demais-de-pe".to_owned(),
        })?;
    tracing::info!(mod_id = %id_no_registro(&id), geracao, arquivo = numero, tipo = lida.media_type, bytes = tamanho, "arquivo escolhido para um MOD");
    Ok(Some(EscolhidoParaOMod {
        id: numero,
        tipo: lida.media_type,
        papel: lida.papel,
        bytes: tamanho,
    }))
}

/// **Um pedaço de um arquivo escolhido, em base64.**
///
/// Em pedaços porque o arquivo pode ter dez megabytes e a fila de mensagens de
/// um MOD tem teto — e porque um MOD que só precisa do começo não deve pagar
/// pelo resto.
///
/// # Errors
///
/// [`FalhaNoMod`] quando a geração já acabou, quando o número não é de um
/// arquivo de pé, ou quando ele é de **outro** MOD.
#[tauri::command]
fn pedaco_do_escolhido(
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    arquivo: u64,
    inicio: usize,
) -> Result<String, FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    let guardados = session
        .arquivos_escolhidos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    let guardado = guardados
        .guardados
        .get(&arquivo)
        .ok_or(FalhaNoMod::Recusado {
            motivo: "arquivo-nao-esta-de-pe".to_owned(),
        })?;
    // **De quem pediu, e não de quem souber o número.** Sem esta linha, um MOD
    // que adivinhasse um número leria o arquivo que outro pediu — e o número é
    // pequeno e sequencial.
    if guardado.de_quem != id || guardado.geracao != geracao {
        return Err(FalhaNoMod::Recusado {
            motivo: "arquivo-de-outro-mod".to_owned(),
        });
    }
    let fim = inicio
        .saturating_add(PEDACO_DO_ESCOLHIDO)
        .min(guardado.bytes.len());
    let fatia = guardado.bytes.get(inicio..fim).unwrap_or_default();
    Ok(seele_ffi::base64_de(fatia))
}

/// **Solta um arquivo escolhido**, agora, a pedido de quem o pediu.
///
/// O cancelamento explícito. A saída da sessão solta os dela de qualquer jeito,
/// mas um MOD que já mandou o arquivo para o servidor dele não precisa esperar
/// a saída para devolver dez megabytes de memória.
///
/// # Errors
///
/// [`FalhaNoMod`] quando a geração já acabou.
#[tauri::command]
fn soltar_escolhido(
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    arquivo: u64,
) -> Result<(), FalhaNoMod> {
    let mut guardados = session
        .arquivos_escolhidos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    // Soltar o que já não é de ninguém não é erro: a saída pode ter passado na
    // frente, e quem chama não tem como saber disso sem uma corrida.
    if guardados
        .guardados
        .get(&arquivo)
        .is_some_and(|a| a.de_quem == id && a.geracao == geracao)
    {
        guardados.guardados.remove(&arquivo);
    }
    Ok(())
}

/// A identidade do conjunto que este servidor exige agora.
///
/// A tela de gestão a lê ao abrir e a guarda como **base** do rascunho. É ela
/// que o SALVAR devolve para conferência: se o que está de pé na hora de gravar
/// for outro, alguém aplicou no meio, e gravar por cima apagaria a decisão
/// dessa pessoa em silêncio.
///
/// Vazia quando esta janela não hospeda — não há conjunto de que falar.
///
/// # Errors
///
/// [`FalhaNoMod`] quando o banco não responde.
#[tauri::command]
async fn conjunto_exigido_agora(session: State<'_, Session>) -> Result<String, FalhaNoMod> {
    let Ok(persistence) = persistence_do_mod(&session) else {
        return Ok(String::new());
    };
    let persistence = persistence.lock().await;
    Ok(seele_server::mods::anuncio::conjunto_exigido(&persistence)
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
        .identidade)
}

/// Um MOD escolhido pela tela: qual, e **quais bytes**.
///
/// O par, e não só o identificador — plano de isolamento de 18/09. Com o cache
/// por conteúdo, dois pacotes do mesmo MOD podem estar no disco, e escolher um
/// deles é a decisão que a tela toma.
#[derive(Debug, serde::Deserialize)]
struct ModEscolhido {
    /// `autor/nome`.
    id: String,
    /// O hash do conteúdo daquele pacote.
    hash: String,
}

/// Aplica um conjunto inteiro de MODs num ato — o SALVAR da tela de gestão.
///
/// # Por que não é um laço sobre [`aplicar_mod`]
///
/// Porque cada chamada daquele acorda o anúncio, e acordar o anúncio derruba
/// quem está dentro. Ligar três MODs custava três quedas ao operador e a todo
/// mundo que estava com ele. O documento de ajustes de 18/09 pede «no máximo
/// um ciclo de reconexão por aplicação, não um por switch», e diz explicitamente
/// para não construir isto como laço. [`definir_conjunto`] escreve tudo numa
/// transação e acorda uma vez.
///
/// # A conferência de conflito
///
/// `base` é a identidade do conjunto que a tela leu quando abriu. Se o que está
/// de pé agora for outro, alguém aplicou no meio, e gravar por cima apagaria a
/// decisão dessa pessoa em silêncio. A recusa devolve a identidade atual, para
/// a tela recarregar e a pessoa refazer o rascunho sabendo o que mudou.
///
/// Vazia dispensa a conferência: é o caminho de quem abriu a tela sem conjunto
/// nenhum de pé.
///
/// # Errors
///
/// [`FalhaNoMod`] quando um dos MODs não serve, quando esta janela não hospeda,
/// quando o conjunto mudou por baixo, ou quando o banco não responde. **Nada é
/// gravado em nenhum desses casos**: a validação inteira acontece antes da
/// primeira escrita, e a escrita é uma transação.
#[tauri::command]
async fn aplicar_conjunto_de_mods(
    app: AppHandle,
    session: State<'_, Session>,
    ligados: Vec<ModEscolhido>,
    base: String,
) -> Result<String, FalhaNoMod> {
    let persistence = persistence_do_mod(&session)?;

    // **Validar tudo antes de escrever qualquer coisa.** Um MOD que não serve
    // no meio da lista não pode deixar metade do conjunto aplicada: o servidor
    // ficaria exigindo uma mistura que ninguém decidiu.
    let mut linhas = Vec::with_capacity(ligados.len());
    for escolhido in &ligados {
        // **Por hash.** A tela manda quais bytes escolheu, e não só qual MOD:
        // dois pacotes do mesmo MOD podem estar no disco, e «qual deles» é
        // justamente a decisão que esta chamada aplica.
        let instalado = seele_ffi::mods::ler_por_hash(&config_dir(&app), &escolhido.hash)
            .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
        if instalado.id != escolhido.id {
            return Err(FalhaNoMod::Recusado {
                motivo: "conteudo-de-outro-mod".to_owned(),
            });
        }
        linhas.push(habilitado_de(instalado));
    }

    let identidade = {
        let persistence = persistence.lock().await;

        let atual = seele_server::mods::anuncio::conjunto_exigido(&persistence)
            .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
            .identidade;
        if !base.is_empty() && base != atual {
            return Err(FalhaNoMod::ConjuntoMudou { atual });
        }

        seele_server::persistence::mods::definir_conjunto(&persistence, &linhas)
            .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;

        // Lida depois da escrita, e pela mesma função que o anúncio usa — a
        // mesma razão de `aplicar_mod`: uma segunda definição de «o mesmo
        // conjunto» discordaria da primeira no dia em que um campo entrasse.
        seele_server::mods::anuncio::conjunto_exigido(&persistence)
            .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
            .identidade
    };

    // **Um sim, para o conjunto resultante.** Salvar já é a decisão informada
    // do operador sobre o conjunto que vai valer; perguntar de novo, logo
    // depois, seria perguntar o que acabou de ser respondido. Isto não autoriza
    // mudança futura, e não dispensa o aceite de quem entra. E só ao servidor
    // daqui (lote X, rodada 1): ver `gravar_o_sim_de_quem_hospeda`.
    gravar_o_sim_de_quem_hospeda(&config_dir(&app), &session, &identidade)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
    Ok(identidade)
}

/// **A escolha de quem administra** (rascunho da API 6, §9; D-api-5; tarefa
/// 17f.0 da F3): qual MOD pinta o servidor desta janela.
///
/// Com dois MODs com a porta do tema ligados, um só pinta, e quem hospeda
/// decide qual — como já decide quem desenha cada parte da tela. O tema que
/// cada um declarou fica guardado, e é o do escolhido que passa a valer, ou
/// nenhum se ele ainda não declarou. Quem está dentro recebe o `TemaMudou`
/// na hora, sem reconexão: o conjunto exigido não muda.
///
/// # Errors
///
/// [`FalhaNoMod::NaoEstaHospedando`] sem servidor nesta janela;
/// [`FalhaNoMod::Recusado`] com `mod-desligado` ou `sem-porta-de-tema`, e nada
/// muda; [`FalhaNoMod::BancoNaoRespondeu`].
#[tauri::command]
async fn escolher_o_tema_do_servidor(
    session: State<'_, Session>,
    id: String,
) -> Result<(), FalhaNoMod> {
    let servidor = servidor_do_mod(&session)?;
    match seele_server::mods::tema::escolher(&servidor, &id)
        .await
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
    {
        seele_server::mods::tema::Escolha::Feita => Ok(()),
        seele_server::mods::tema::Escolha::Recusada(motivo) => Err(FalhaNoMod::Recusado {
            motivo: motivo.to_owned(),
        }),
    }
}

// --------------------------------------------- o aceite de quem entra
//
// Os três acima são de quem **hospeda** — decidem o que a sala exige. Os três
// abaixo são de quem **entra** — registram o que esta máquina respondeu, e não
// tocam em servidor nenhum: valem numa janela que nunca hospedou nada.
//
// Existem porque sem eles o contrato do ADR 0045 não fecha deste lado. A lista
// de MODs chega à tela pelo `ConnectionError::ModsNaoAceitos`, e sem um verbo
// de gravar o sim a pergunta não teria resposta possível — a pessoa leria a
// lista e não teria o que fazer com ela.
//
// **A tela que mostra a lista e tem o botão é `ui/camada-mods.js`**, desde
// 17/09/2026 (`65ad0653`). Ela abre a pergunta com o que chegou no
// `ModsNaoAceitos`, pergunta a `aceite_de_mods` se a lista mudou desde o
// último sim, e chama `aceitar_mods` quando a pessoa aceita. O
// `esquecer_aceite_de_mods` é chamado por ela, para desfazer o sim de quem
// desistiu enquanto ele era gravado, e pela seção MODS de CONFIGURAÇÕES, no
// mesmo arquivo.

/// O conjunto de MODs que esta máquina já aceitou para este servidor, se algum.
///
/// `alvo` é o endereço como a pessoa o digitou; a forma canônica sob a qual ele
/// é arquivado é decidida pelo `seele-ffi`, e não aqui — é a mesma chave que a
/// conexão procura, e reproduzi-la nesta camada seria convidar as duas a
/// discordarem.
#[tauri::command]
fn aceite_de_mods(app: AppHandle, alvo: String) -> Option<String> {
    seele_ffi::mods::aceite_de(&config_dir(&app), &alvo)
}

/// Guarda o sim que a pessoa deu à lista de MODs deste servidor.
///
/// `conjunto` é a identidade que veio no `ConnectionError::ModsNaoAceitos` —
/// inteira e como chegou. A entrada seguinte neste servidor passa direto, e só
/// neste: um aceite é de um servidor, e não desta máquina.
#[tauri::command]
fn aceitar_mods(app: AppHandle, alvo: String, conjunto: String) -> Result<(), FalhaNoMod> {
    seele_ffi::mods::aceitar(&config_dir(&app), &alvo, &conjunto)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })
}

/// Desfaz o sim dado a este servidor: a entrada seguinte volta a perguntar.
///
/// Um consentimento que não se retira não é consentimento — ADR 0045.
#[tauri::command]
fn esquecer_aceite_de_mods(app: AppHandle, alvo: String) -> Result<(), FalhaNoMod> {
    seele_ffi::mods::esquecer_aceite(&config_dir(&app), &alvo)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })
}

/// Escolhe uma pasta e instala o MOD que houver nela.
///
/// A pasta vem do diálogo do sistema, e não de texto digitado: um caminho
/// escrito à mão é um caminho que erra, e o engano mais provável — apontar para
/// a pasta que **contém** o MOD em vez da pasta do MOD — tem recusa própria.
///
/// `None` é a pessoa tendo fechado o diálogo, e não é falha: desistir de
/// escolher é uma resposta.
///
/// Devolve o identificador que o manifesto declara, para a tela dizer **o que**
/// foi instalado em vez de só dizer que deu certo.
///
/// # Errors
///
/// [`mods::FalhaAoInstalarMod`], uma variante por motivo.
#[tauri::command]
async fn instalar_mod(app: AppHandle) -> Result<Option<String>, mods::FalhaAoInstalarMod> {
    use tauri_plugin_dialog::DialogExt as _;

    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title("Escolha a pasta do MOD")
        .pick_folder(move |escolha| {
            let _ = envia.try_send(escolha);
        });

    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(None);
    };
    let Ok(pasta) = escolha.into_path() else {
        return Err(mods::FalhaAoInstalarMod::SemManifesto);
    };
    // O identificador é o que a tela mostra; o hash é o que o servidor exige,
    // e quem o escolhe é outra decisão — ver `aplicar_conjunto_de_mods`.
    mods::instalar_de(std::path::Path::new(&config_dir(&app)), &pasta).map(|p| Some(p.id))
}

/// Baixa do catálogo o app mais novo de uma linha de servidor (ou o mais novo
/// de todos) e o instala ao lado dos outros.
///
/// A chave e os endereços saem do bloco do atualizador do `tauri.conf.json`:
/// ver `versoes::atualizador_do_app`.
///
/// # Errors
///
/// [`versoes::FalhaAoBaixarVersao`], uma variante por motivo.
#[tauri::command]
async fn baixar_versao(
    app: AppHandle,
    // A linha de servidor cujo app mais novo baixar; sem ela o mais novo de
    // todos.
    linha: Option<String>,
) -> Result<String, versoes::FalhaAoBaixarVersao> {
    let Some(atualizador) = versoes::atualizador_do_app(&app) else {
        // Um build sem o bloco do atualizador não tem catálogo a consultar, e
        // dizer isso é melhor que tentar e falhar na rede: o motivo é outro e
        // o conserto é outro.
        return Err(versoes::FalhaAoBaixarVersao::CatalogoNaoServe(
            "este build não declara catálogo nem chave".to_owned(),
        ));
    };
    let pedido = versoes::pedido_de(None, linha.as_deref()).map_err(|_| {
        versoes::FalhaAoBaixarVersao::CatalogoNaoServe(format!(
            "«{}» não é uma linha de servidor",
            linha.as_deref().unwrap_or_default()
        ))
    })?;
    versoes::baixar(&config_dir(&app), &atualizador, &pedido).await
}

/// Se **esta janela** está hospedando um servidor agora.
///
/// # Por que a tela precisa perguntar isto separado
///
/// Porque `mods_instalados` devolve `enabled: false` para todos quando não há
/// servidor hospedado — e isso é indistinguível de «há servidor e nenhum MOD
/// está ligado» olhando só a lista. A tela desenharia botões de LIGAR que não
/// têm onde ligar, e a pessoa apertaria um e leria uma recusa que não explica
/// nada sobre o que ela fez de errado: ela não fez nada de errado.
#[tauri::command]
fn estou_hospedando(session: State<'_, Session>) -> bool {
    session
        .hospedagens
        .lock()
        .is_ok_and(|hospedagens| hospedagens.guardado.is_some())
}

/// **Qual** servidor guardado esta janela hospeda, quando hospeda um.
///
/// Plano de isolamento de 18/09: «gestão indica servidor-alvo… nunca inferir o
/// destino apenas de *estou hospedando*». A tela dizia «ligar e desligar MOD é
/// do servidor que esta janela hospeda» sem dizer qual — e quem tem dois
/// servidores guardados não tinha como saber em qual estava mexendo.
///
/// `None` quando não hospeda, e **também** quando hospeda o legado: a máquina
/// que já hospedava antes de haver lista, cujo banco é o de sempre. Quem lê
/// distingue os dois casos pelo `estou_hospedando`, que continua respondendo a
/// outra pergunta.
#[tauri::command]
fn servidor_hospedado(session: State<'_, Session>) -> Option<String> {
    session
        .servidor_hospedado
        .lock()
        .ok()
        .and_then(|qual| qual.clone())
}

/// O catálogo de MODs, baixado e conferido contra a chave embutida.
///
/// **Só quando alguém pede.** Não há consulta ao abrir o app — ADR 0026, e é a
/// mesma regra do botão de atualizar: um produto que fala com a rede por ter
/// sido aberto é um produto que conta a alguém que ele foi aberto.
///
/// # Errors
///
/// [`catalogo::FalhaNoCatalogo`], uma variante por motivo.
#[tauri::command]
async fn catalogo_de_mods() -> Result<catalogo::Catalogo, catalogo::FalhaNoCatalogo> {
    catalogo::buscar_catalogo().await
}

/// Para que a janela chama [`instalar_mod_do_catalogo`]. São dois usos, e só
/// um deles mexe no servidor que esta janela hospeda.
///
/// **Lote X da F5, ADR 0045.** Instalar, exigir num servidor e consentir são
/// três decisões. O comando servia aos dois usos sem saber qual era cada um, e
/// reaplicava no servidor hospedado aqui todo MOD que ele já exigia. Na
/// obtenção para outro servidor, isso trocava a versão que o servidor daqui
/// exige pela que o outro exige, e quem estava dentro dele caía. Acontecia
/// também depois de a pessoa desistir da pergunta, porque a obtenção segue até
/// o fim.
///
/// **Obrigatório, sem padrão.** Uma chamada que não diz o uso é recusada pela
/// ponte do Tauri antes de baixar qualquer coisa, e a recusa chega à tela. Um
/// padrão seria um defeito calado nos dois sentidos: reaplicar por omissão é o
/// achado deste lote, e não reaplicar por omissão traz o QA-01 de volta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum UsoDoCatalogo {
    /// A gestão de MODs atualiza um MOD pelo catálogo (`instalarDoCatalogo`,
    /// em `camada-mods.js`). Se o servidor hospedado aqui já o exigia, a
    /// exigência passa aos bytes novos: é o QA-01.
    Atualizar,
    /// A janela obtém o que um servidor exige para entrar nele
    /// (`obterOConjunto`, em `camada-mods.js`, e
    /// `buscarOQueFaltaSeJaFoiAceito`, em `base.js`). Só instala: o servidor
    /// hospedado aqui continua exigindo o que exigia.
    Obter,
}

/// Se este uso reaplica o MOD no servidor que esta janela hospeda: só a
/// atualização pela gestão, e só de um MOD que ele já exigia. Ver
/// [`UsoDoCatalogo`].
fn reaplica_no_hospedado(uso: UsoDoCatalogo, ja_exigido: bool) -> bool {
    ja_exigido && uso == UsoDoCatalogo::Atualizar
}

/// Baixa do catálogo uma versão de um MOD e a instala nesta máquina.
///
/// Confere a assinatura do catálogo, a lista de revogações, e o **hash do
/// conjunto** dos arquivos baixados — nesta ordem, e nada vai para o disco
/// antes da última. Instala desligado, como toda instalação de MOD: ligar é
/// outra decisão.
///
/// `uso` diz quem chama ([`UsoDoCatalogo`]), e a resposta diz se o MOD foi
/// reaplicado no servidor que esta janela hospeda.
///
/// # Errors
///
/// [`catalogo::FalhaNoCatalogo`], uma variante por motivo.
#[tauri::command]
async fn instalar_mod_do_catalogo(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    versao: String,
    uso: UsoDoCatalogo,
) -> Result<bool, catalogo::FalhaNoCatalogo> {
    let reaplicar = reaplica_no_hospedado(uso, mod_exigido_aqui(&session, &id).await);
    let hash = catalogo::instalar_do_catalogo(&config_dir(&app), &id, &versao).await?;

    // **QA-01: baixar não é aplicar.** Trocar os arquivos no disco não mexe na
    // linha que o servidor guarda, e é o **hash** dela que vai no anúncio.
    // Depois de atualizar um MOD que estava ligado, o disco tinha a versão nova
    // e o servidor continuava exigindo a antiga — e a tela dizia que o conteúdo
    // instalado não é o que o servidor exige, sem dizer o que fazer. Sair dali
    // pedia descobrir sozinho uma sequência de desligar, reconectar, ligar e
    // reconectar.
    //
    // Reaplicar aqui é o que fecha a operação: a exigência passa a descrever os
    // bytes que acabaram de chegar, e o sim desta máquina é regravado para o
    // conjunto resultante — que é outro conjunto, e por isso precisa de outro
    // sim, mesmo sendo a mesma decisão de segundos antes.
    //
    // Só para um MOD que **já estava exigido**: atualizar não é ligar, e um que
    // estava desligado continua desligado. E só quando quem chama é a gestão:
    // obter o que um servidor exige não decide nada sobre o servidor daqui.
    if reaplicar {
        aplicar_mod(app, session, id, hash, true)
            .await
            .map_err(|erro| catalogo::FalhaNoCatalogo::NaoAplicou(format!("{erro:?}")))?;
    }
    Ok(reaplicar)
}

/// Se este MOD está exigido pelo servidor que **esta janela** hospeda.
///
/// Não hospedar não é falha: é a resposta «não», e é o caso de quem instala do
/// catálogo sem ter servidor de pé.
async fn mod_exigido_aqui(session: &State<'_, Session>, id: &str) -> bool {
    let Ok(persistence) = persistence_do_mod(session) else {
        return false;
    };
    let persistence = persistence.lock().await;
    seele_server::persistence::mods::enabled(&persistence)
        .is_ok_and(|ligados| ligados.iter().any(|ligado| ligado.id == id))
}

/// **Só a atualização pela gestão reaplica no servidor hospedado** — lote X da
/// F5, ADR 0045. Ver [`UsoDoCatalogo`].
///
/// O comando tem `AppHandle` e baixa da rede, e não roda num teste. O que roda
/// é o banco de verdade do servidor hospedado (`Persistence`), em memória, com
/// a decisão do comando ([`reaplica_no_hospedado`]) e o efeito de
/// [`aplicar_mod`]: `habilitar_mod` grava a linha **por identificador**, então
/// reaplicar troca a versão exigida e a identidade do conjunto — e, com a
/// identidade nova, o servidor derruba quem estava dentro.
#[cfg(test)]
mod so_a_gestao_reaplica_no_servidor_hospedado {
    use seele_server::mods::anuncio::conjunto_exigido;
    use seele_server::persistence::mods::{enable, enabled, EnabledMod};
    use seele_server::persistence::{Location, Persistence};

    use super::{corpo_de, fonte_sem_comentarios, reaplica_no_hospedado, UsoDoCatalogo};

    const ID: &str = "autor/rpg";

    /// Uma versão do MOD, como o banco do servidor a guarda.
    fn versao(numero: &str, byte: char) -> EnabledMod {
        EnabledMod {
            id: ID.to_owned(),
            version: numero.to_owned(),
            hash: byte.to_string().repeat(64),
            repo: "https://example.invalid/rpg".to_owned(),
            reach: vec!["dom".to_owned()],
            server_half: false,
            api: seele_ffi::mods::MOD_API_VERSION,
        }
    }

    /// O servidor que esta janela hospeda, exigindo o MOD na 1.0.0.
    fn hospedado() -> Persistence {
        let banco = Persistence::open(&Location::Memory).unwrap();
        enable(&banco, &versao("1.0.0", 'a')).unwrap();
        banco
    }

    /// A versão que o servidor hospedado exige, e a identidade que ele anuncia.
    fn exigido(banco: &Persistence) -> (String, String) {
        let ligado = enabled(banco)
            .unwrap()
            .into_iter()
            .find(|ligado| ligado.id == ID)
            .unwrap();
        (ligado.version, conjunto_exigido(banco).unwrap().identidade)
    }

    /// O que `instalar_mod_do_catalogo` faz no servidor hospedado depois de
    /// baixar `chegou`: a decisão do comando, sobre o que `mod_exigido_aqui`
    /// lê, e, se ela mandar, o que `aplicar_mod` grava.
    fn depois_de_baixar(banco: &Persistence, uso: UsoDoCatalogo, chegou: &EnabledMod) -> bool {
        let ja_exigido = enabled(banco).unwrap().iter().any(|ligado| ligado.id == ID);
        let reaplicar = reaplica_no_hospedado(uso, ja_exigido);
        if reaplicar {
            enable(banco, chegou).unwrap();
        }
        reaplicar
    }

    /// A medida do lote: outro servidor exige o mesmo MOD na 2.0.0, e a janela
    /// o obtém. É a mesma chamada na pergunta dos MODs, depois de a pessoa
    /// desistir dela (a obtenção segue até o fim) e na sessão que já aceitou.
    #[test]
    fn obter_o_que_outro_servidor_exige_nao_troca_a_versao_do_servidor_hospedado() {
        let banco = hospedado();
        let antes = exigido(&banco);

        let reaplicou = depois_de_baixar(&banco, UsoDoCatalogo::Obter, &versao("2.0.0", 'b'));

        let depois = exigido(&banco);
        assert_eq!(
            depois, antes,
            "obter o MOD que outro servidor exige trocou a versão exigida pelo servidor que \
             esta janela hospeda, de {} para {}: quem estava dentro dele cai, e ele passa a \
             exigir uma versão que quem hospeda não escolheu (ADR 0045)",
            antes.0, depois.0
        );
        assert!(
            !reaplicou,
            "a obtenção para outro servidor respondeu que reaplicou no servidor hospedado aqui"
        );
    }

    /// O QA-01 continua: atualizar pela gestão um MOD que o servidor daqui
    /// exige passa a exigência aos bytes novos, e um que ele não exige fica
    /// desligado.
    #[test]
    fn a_atualizacao_pela_gestao_continua_reaplicando() {
        let banco = hospedado();
        let antes = exigido(&banco);

        let reaplicou = depois_de_baixar(&banco, UsoDoCatalogo::Atualizar, &versao("2.0.0", 'b'));

        let depois = exigido(&banco);
        assert!(
            reaplicou && depois.0 == "2.0.0" && depois.1 != antes.1,
            "atualizar pela gestão um MOD exigido aqui deixou de reaplicar: o disco tem a \
             versão nova e o servidor continua exigindo a antiga (QA-01) — {antes:?} → {depois:?}"
        );

        let vazio = Persistence::open(&Location::Memory).unwrap();
        assert!(
            !depois_de_baixar(&vazio, UsoDoCatalogo::Atualizar, &versao("2.0.0", 'b'))
                && enabled(&vazio).unwrap().is_empty(),
            "atualizar um MOD que o servidor daqui não exige o ligou: atualizar não é ligar"
        );
    }

    /// A janela diz o uso com estas palavras (`camada-mods.js`, `base.js`), e
    /// nenhuma outra passa.
    #[test]
    fn a_janela_diz_o_uso_com_as_palavras_que_o_comando_entende() {
        let ler = |texto: &str| serde_json::from_str::<UsoDoCatalogo>(texto).ok();
        assert_eq!(
            (ler("\"obter\""), ler("\"atualizar\"")),
            (Some(UsoDoCatalogo::Obter), Some(UsoDoCatalogo::Atualizar)),
            "o comando deixou de entender a palavra que a janela manda: toda obtenção e \
             toda atualização pelo catálogo são recusadas antes de baixar"
        );
        assert_eq!(
            (ler("null"), ler("\"Atualizar\"")),
            (None, None),
            "o comando aceita um uso que a janela não escreve: uma chamada sem uso passaria \
             a ter um padrão calado"
        );
    }

    /// O comando reaplica pela decisão que lê o uso, e por nenhum outro
    /// caminho. Sem isto, a decisão acima podia estar certa e o comando voltar
    /// a reaplicar por `mod_exigido_aqui` sozinho.
    ///
    /// A agulha do cabeçalho é montada por `concat!` (rodada 1): escrita
    /// inteira, ela casava com este mesmo teste quando o comando mudava de
    /// nome ou de cabeçalho, e o guarda conferia a si mesmo.
    #[test]
    fn o_comando_reaplica_pela_decisao_que_le_o_uso() {
        let fonte = fonte_sem_comentarios();
        let corpo = corpo_de(&fonte, concat!("async fn instalar_mod_do_catalogo", "("));
        let decide = corpo.find("reaplica_no_hospedado(uso,");
        let aplica = corpo.find("if reaplicar {");
        assert!(
            corpo.contains("uso: UsoDoCatalogo,")
                && matches!((decide, aplica), (Some(decide), Some(aplica)) if decide < aplica)
                && corpo.matches("aplicar_mod(").count() == 1
                && corpo.contains("Ok(reaplicar)"),
            "`instalar_mod_do_catalogo` deixou de reaplicar só pela decisão que lê o uso: a \
             obtenção para outro servidor volta a trocar o MOD do servidor hospedado aqui \
             (ADR 0045):\n{corpo}"
        );
    }
}

/// Todo sim que esta máquina já deu a um servidor.
///
/// ADR 0045: «um consentimento que não se retira não é consentimento». Sem esta
/// lista, `esquecer_aceite_de_mods` só podia ser chamada por quem já soubesse o
/// endereço de cor — o que é o mesmo que não poder ser chamada.
#[tauri::command]
fn aceites_de_mods(app: AppHandle) -> Vec<seele_ffi::mods::AceiteGuardado> {
    seele_ffi::mods::aceites_guardados(&config_dir(&app))
}

/// Os identificadores ligados, ou vazio quando esta janela não hospeda.
///
/// Não hospedar não é falha aqui: a lista de MODs instalados é útil de qualquer
/// jeito, e desenhar um erro para quem só abriu a tela seria pior que desenhar
/// a lista com tudo desligado.
async fn mods_ligados(session: &State<'_, Session>) -> Result<Vec<(String, String)>, FalhaNoMod> {
    let Ok(persistence) = persistence_do_mod(session) else {
        return Ok(Vec::new());
    };
    let persistence = persistence.lock().await;
    // **O par, e não só o identificador.** Com o cache por conteúdo pode haver
    // dois pacotes do mesmo MOD nesta máquina, e «este está ligado» deixa de
    // ser uma pergunta sobre o identificador: é sobre **quais bytes** este
    // servidor exige. Marcar os dois como ligados diria que o servidor exige
    // duas coisas incompatíveis ao mesmo tempo.
    seele_server::persistence::mods::enabled(&persistence)
        .map(|ligados| {
            ligados
                .into_iter()
                .map(|ligado| (ligado.id, ligado.hash))
                .collect()
        })
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)
}

/// O `Arc` do PERSISTENCE hospedado, ou a recusa.
///
/// Clonado para fora do `Mutex` do app **antes** de qualquer `await`, pela
/// mesma razão que `persistence_hospedada` escreve: segurar um
/// `std::sync::MutexGuard` atravessando um ponto de espera trava os dois
/// cadeados de uma vez e nem compila do lado do Tauri.
fn persistence_do_mod(
    session: &State<'_, Session>,
) -> Result<seele_server::hospedagem::PersistenceCompartilhada, FalhaNoMod> {
    let hospedagens = session
        .hospedagens
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    let server = hospedagens
        .do_guardado()
        .ok_or(FalhaNoMod::NaoEstaHospedando)?;
    Ok(server.persistence())
}

/// O servidor hospedado inteiro, para o que grava **e** avisa as sessões, ou a
/// recusa. Clonado para fora do `Mutex` pela razão de [`persistence_do_mod`].
fn servidor_do_mod(
    session: &State<'_, Session>,
) -> Result<Arc<seele_server::server::Server>, FalhaNoMod> {
    let hospedagens = session
        .hospedagens
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    let server = hospedagens
        .do_guardado()
        .ok_or(FalhaNoMod::NaoEstaHospedando)?;
    Ok(server.servidor())
}

/// O MOD que pinta o servidor desta janela, ou nenhum — e nenhum quando esta
/// janela não hospeda, pela razão de [`mods_ligados`].
async fn quem_pinta_o_servidor(session: &State<'_, Session>) -> Result<Option<String>, FalhaNoMod> {
    let Ok(persistence) = persistence_do_mod(session) else {
        return Ok(None);
    };
    let persistence = persistence.lock().await;
    seele_server::persistence::tema::escolhido_ligado(&persistence)
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)
}

/// Abre o seletor e põe a imagem escolhida como **a sua**.
///
/// Gêmeo de [`escolher_icone_do_server`], e o corpo é o mesmo por escolha e não
/// por descuido: a imagem é do mesmo formato, do mesmo tamanho, e passa pelo
/// mesmo `icone::encolher` — que é o que permite escolher uma foto de câmera e
/// não um arquivo já preparado.
///
/// A diferença está uma camada abaixo: aquele exige `AdministerServer`, este
/// não exige nada. O servidor grava na linha de quem pediu.
///
/// `Ok(false)` é ter fechado o seletor sem escolher, e é o desfecho mais comum
/// de todos.
///
/// # Errors
///
/// [`ConnectionError::IconNotAPicture`] quando o arquivo não vira um PNG dentro
/// do teto, e o que a sessão devolver.
#[tauri::command]
async fn escolher_minha_imagem(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<bool, ConnectionError> {
    let Some(pronto) = escolher_e_encolher(&app, "Escolha a sua imagem").await? else {
        return Ok(false);
    };
    gravar_retrato(&app, Some(&pronto))?;
    // E, se há sessão, o servidor sabe agora. Sem sessão não é erro nenhum —
    // é o caso da tela inicial, e a imagem sobe no próximo `connect`.
    if let Ok(connection) = session.connection() {
        connection.set_person_icon(Some(pronto))?;
    }
    Ok(true)
}

/// Onde o retrato desta pessoa mora nesta máquina.
///
/// Um arquivo ao lado das preferências, e não uma linha dentro delas: são até
/// 8 KiB de PNG, e binário num arquivo de `nome <TAB> valor` faria uma edição à
/// mão virar conversa de suporte. É a mesma decisão que `conhecidos` já tomou
/// para a imagem de cada servidor.
fn caminho_do_retrato<R: tauri::Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    std::path::PathBuf::from(config_dir(app)).join("retrato.png")
}

/// Grava o retrato, ou o apaga. Falhar em gravar é falha de verdade: sem isto o
/// diálogo diria que guardou uma imagem que não existe.
fn gravar_retrato(app: &AppHandle, bytes: Option<&[u8]>) -> Result<(), ConnectionError> {
    let caminho = caminho_do_retrato(app);
    match bytes {
        Some(bytes) => std::fs::write(&caminho, bytes).map_err(|erro| {
            tracing::warn!(%erro, "não gravei o retrato nesta máquina");
            ConnectionError::IconNotAPicture
        }),
        None => match std::fs::remove_file(&caminho) {
            Ok(()) => Ok(()),
            // Apagar o que não existe é o estado que se pediu, e não uma falha.
            Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(erro) => {
                tracing::warn!(%erro, "não apaguei o retrato desta máquina");
                Err(ConnectionError::IconNotAPicture)
            }
        },
    }
}

/// Os bytes do retrato desta máquina, se há um.
fn retrato_guardado<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<Vec<u8>> {
    std::fs::read(caminho_do_retrato(app)).ok()
}

/// O retrato desta máquina, pronto para uma `<img>`.
///
/// **Sem sessão também**, que é o ponto: é ele que faz o diálogo de perfil da
/// tela inicial ter o mesmo conteúdo do de dentro do servidor.
#[tauri::command]
fn meu_retrato(app: AppHandle) -> Option<String> {
    let bytes = retrato_guardado(&app)?;
    Some(format!(
        "data:image/png;base64,{}",
        seele_ffi::base64_de(&bytes)
    ))
}

/// O apelido com que se entra, guardado nesta máquina.
///
/// **Antes de haver servidor.** A tela de entrada da 0.9.0 não tem campo de
/// apelido: ele mora no perfil, que se abre sem sessão nenhuma, e um nome
/// escolhido ali não tem onde ser gravado a não ser em disco.
///
/// O apelido **do servidor** é outra coisa: cada um guarda o seu, único, e
/// trocá-lo lá é [`escolher_apelido`]. Este é o que se leva ao entrar.
#[tauri::command]
fn apelido_local(app: AppHandle) -> Option<String> {
    preferencias(&app).and_then(|p| p.nickname().map(str::to_owned))
}

/// Escreve o apelido de entrada nesta máquina.
///
/// # Errors
///
/// [`FalhaAoEscolher::NaoGravei`] se o disco recusar.
#[tauri::command]
fn escolher_apelido_local(app: AppHandle, apelido: String) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    ajustes
        .set_nickname(Some(apelido.trim()).filter(|nome| !nome.is_empty()))
        .map_err(|erro| {
            tracing::warn!(%erro, "não consegui gravar o apelido");
            FalhaAoEscolher::NaoGravei
        })
}

/// Troca o seu apelido.
///
/// O histórico não muda: cada mensagem guarda o nome de quem a escreveu no
/// instante em que foi escrita. É a decisão de quem desenha o produto, e é a
/// que preserva o registro — um histórico reescrito é um em que uma conversa
/// passa a citar um nome que ninguém usava quando ela aconteceu.
///
/// # Errors
///
/// O que a sessão devolver. Um nome já tomado volta como alerta do servidor, e
/// não daqui: quem sabe quais nomes existem naquele servidor é ele.
#[tauri::command]
fn escolher_apelido(session: State<'_, Session>, apelido: String) -> Result<(), ConnectionError> {
    session.connection()?.set_nickname(apelido)
}

/// Tira a sua imagem.
///
/// # Errors
///
/// O que a sessão devolver.
#[tauri::command]
fn tirar_minha_imagem(app: AppHandle, session: State<'_, Session>) -> Result<(), ConnectionError> {
    gravar_retrato(&app, None)?;
    if let Ok(connection) = session.connection() {
        connection.set_person_icon(None)?;
    }
    Ok(())
}

/// A imagem de perfil de alguém, em `data:` para a janela desenhar.
///
/// Fora do `Snapshot` de propósito, pela razão que a FFI escreve: o snapshot
/// atravessa a ponte duas vezes por segundo, e figuras de 8 KiB **por pessoa**
/// ali dentro seriam megabytes por minuto de uma coisa que muda quase nunca.
///
/// # Errors
///
/// Não falha: quem não tem imagem, e uma sessão que não existe, devolvem `None`.
#[tauri::command]
fn imagem_da_pessoa(session: State<'_, Session>, person: u64) -> Option<String> {
    let bytes = session.connection().ok()?.person_icon(person)?;
    Some(format!(
        "data:image/png;base64,{}",
        seele_ffi::base64_de(&bytes)
    ))
}

/// Os bytes da imagem que está valendo, ou nada.
///
/// Fora do `Snapshot` de propósito, e a tela respeita o mesmo acordo: o
/// snapshot é lido duas vezes por segundo e atravessa a ponte em JSON, então
/// ele carrega `icon_revision` — um número — e a casca só vem buscar os bytes
/// quando o número anda. É o precedente de `messages_revision`.
#[tauri::command]
fn icone_do_server(session: State<'_, Session>) -> Result<Option<Vec<u8>>, ConnectionError> {
    Ok(session.connection()?.server_icon())
}

/// Pede ao servidor que acabe com a sessão de alguém — `expulsar`.
///
/// Devolve quando o pedido entra na fila, e não quando a pessoa saiu. Quem
/// responde isso é o servidor: `RosterChanged` quando ela sai de fato,
/// `NoticeRaised` com `PermissionDenied` quando ele recusa. É o mesmo caminho
/// dos verbos de sala, e não há porta nova a aprender.
///
/// A tela pode consultar `Snapshot::may_kick` para decidir se desenha o botão —
/// é o que faltava para o `EJETAR PLUG DO OPERADOR`, desenhado e desabilitado
/// desde o v2 porque não havia o que chamar. Isso é conveniência: mandar o
/// pedido sem a permissão não expulsa ninguém, e a `specs/08-seguranca.md` põe
/// a segurança nessa recusa e não no botão escondido.
#[tauri::command]
fn expulsar_pessoa(session: State<'_, Session>, person: u64) -> Result<(), ConnectionError> {
    session.connection()?.kick_person(person)
}

/// Pede ao servidor que impeça alguém de voltar — `banir`.
///
/// `expires_at` em segundos desde a época; `None` é para sempre. O `reason` é
/// para o registro de quem hospeda e nunca chega a quem foi banido.
#[tauri::command]
fn banir_pessoa(
    session: State<'_, Session>,
    person: u64,
    reason: Option<String>,
    expires_at: Option<i64>,
) -> Result<(), ConnectionError> {
    session.connection()?.ban_person(person, reason, expires_at)
}

/// Pede ao servidor que tire uma mensagem da Linha.
///
/// Sem permissão nenhuma quando a mensagem é de quem pede: a permissão do
/// `specs/04-servidor-seele.md` diz «de outra pessoa».
#[tauri::command]
fn remover_mensagem(session: State<'_, Session>, message: u64) -> Result<(), ConnectionError> {
    session.connection()?.remove_message(message)
}

/// Pede ao servidor que mova alguém para uma sala de voz — `mover_pessoa`.
#[tauri::command]
fn mover_pessoa(
    session: State<'_, Session>,
    person: u64,
    voice_room: u32,
) -> Result<(), ConnectionError> {
    session.connection()?.move_person(person, voice_room)
}

/// Pede ao servidor que destrua uma sala de voz — `apagar_voice_room`.
///
/// Quem estiver dentro é posto para fora e avisado; a Linha presa a ele, se
/// houver, fica onde está. O servidor recusa a última sala de voz e diz isso com
/// `LastVoiceRoom`, que é frase diferente da de entrada recusada.
///
/// A tela pode consultar `Snapshot::may_delete_rooms` para decidir se desenha o
/// controle — e é campo próprio, não `may_manage_voice_rooms`: fazer sala e destruir
/// sala são permissões diferentes na `specs/04-servidor-seele.md`, e é preciso
/// poder oferecer uma sem a outra. Isso é conveniência; quem nega é o servidor.
#[tauri::command]
fn apagar_voice_room(session: State<'_, Session>, voice_room: u32) -> Result<(), ConnectionError> {
    session.connection()?.delete_voice_room(voice_room)
}

/// Pede ao servidor que destrua uma Linha, e tudo que foi escrito nela —
/// `apagar_linha`.
#[tauri::command]
fn apagar_linha(session: State<'_, Session>, channel: u32) -> Result<(), ConnectionError> {
    session.connection()?.delete_channel(channel)
}

/// Pergunta quanto custaria destruir uma Linha. Não destrói nada.
///
/// O único comando desta janela que **espera** o servidor responder, e a razão é a
/// frase que ele alimenta: a caixa de confirmação promete um número exato de
/// mensagens, de gente e uma data, e os três são contados no banco no instante
/// de perguntar. A janela segura uma página de histórico e chutaria para baixo
/// por todo o passado da Linha — e um número quase certo numa caixa que promete
/// destruição é pior que nenhum.
///
/// Por isso `async`: um comando síncrono do Tauri roda na thread principal, e
/// esperar ali travaria a janela inteira enquanto a pergunta atravessa a rede.
///
/// Quem não conseguir resposta **não abre a caixa**. Não há versão honesta dela
/// sem os três números.
#[tauri::command]
async fn peso_da_linha(
    session: State<'_, Session>,
    channel: u32,
) -> Result<ChannelWeight, ConnectionError> {
    // O `Arc` sai do cadeado antes do `await`, e é de propósito: `Session::connection`
    // devolve um clone justamente para que nada segure o `Mutex` atravessando um
    // ponto de espera.
    let connection = session.connection()?;
    connection.weigh_channel(channel).await
}

#[tauri::command]
fn set_muted(session: State<'_, Session>, on: bool) -> Result<(), ConnectionError> {
    session.connection()?.set_muted(on)
}

#[tauri::command]
fn set_total_isolation(session: State<'_, Session>, on: bool) -> Result<(), ConnectionError> {
    session.connection()?.set_total_isolation(on)
}

/// Push-to-talk, reported as it happens.
///
/// Not fallible on purpose: a key coming *up* must never be refused. Returning
/// an error here would give the frontend a path where the microphone was opened
/// and the close was rejected.
#[tauri::command]
fn set_talking(session: State<'_, Session>, talking: bool) {
    if let Ok(connection) = session.connection() {
        connection.set_talking(talking);
    }
}

/// Escolhe como o microfone abre: grava no disco e, se houver sessão, aplica agora.
///
/// **As duas metades, nesta ordem**, pelo mesmo argumento que
/// [`escolher_microfone`] escreve por extenso: a escrita é o que faz a escolha
/// valer amanhã, a aplicação é o que faz valer agora.
///
/// Antes daqui só existia a segunda, e o preço tinha duas faces. Quem escolhia
/// voz achava push-to-talk de volta no dia seguinte — e o padrão é
/// push-to-talk *porque nunca dispara sozinho*, argumento que vale para quem
/// não escolheu e não para quem escolheu. E, por exigir sessão, não dava para
/// escolher o modo na tela de entrada; agora dá.
///
/// # Errors
///
/// [`FalhaAoEscolher::NaoGravei`] se o disco recusar. Sem sessão **não** é
/// falha: a escolha ficou gravada, que era tudo o que havia para fazer.
#[tauri::command]
fn set_voice_mode(
    app: AppHandle,
    session: State<'_, Session>,
    mode: VoiceMode,
) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    if let Err(erro) = ajustes.set_voice_mode(Some(mode.into())) {
        tracing::warn!(%erro, "não consegui gravar o modo de voz escolhido");
        return Err(FalhaAoEscolher::NaoGravei);
    }
    if let Ok(connection) = session.connection() {
        connection.set_voice_mode(mode);
    }
    Ok(())
}

/// Qual modo de microfone está escolhido, ou `None` para o padrão da spec.
///
/// Do disco e não do `Snapshot`, pela mesma razão que [`microfone_escolhido`]:
/// esta pergunta tem resposta sem sessão nenhuma, e é a que a tela de entrada
/// precisa fazer.
#[tauri::command]
fn modo_de_voz_escolhido(app: AppHandle) -> Option<VoiceMode> {
    preferencias(&app)
        .and_then(|p| p.voice_mode())
        .map(VoiceMode::from)
}

// ------------------------------------------------- qualidade de voz (F01, F02)
//
// Três superfícies, e elas são três porque respondem a perguntas diferentes:
//
//   - **os controles**, que valem para a sessão e ficam gravados;
//   - **o teste**, que roda sem sessão nenhuma e mede o mesmo caminho;
//   - **a leitura**, que a tela pede enquanto o controle é arrastado.

/// Liga, desliga ou dosa a supressão de ruído do microfone. F02.
///
/// Vale já e fica gravada: um interruptor que volta sozinho a cada abertura não é
/// um controle. `None` grava «o padrão do produto» em vez de um número.
///
/// # Errors
///
/// Falha se as preferências não puderem ser gravadas.
#[tauri::command]
fn ajustar_reducao_de_ruido(
    app: AppHandle,
    session: State<'_, Session>,
    forca: Option<f32>,
) -> Result<(), String> {
    // A sessão primeiro, para o efeito ser imediato; o disco depois. A ordem
    // importa quando o disco falha: a pessoa ouve a mudança e lê que ela não
    // ficou gravada, que é melhor que não ouvir nada e ler o mesmo.
    let aplicada = forca.unwrap_or(1.0);
    if let Ok(conexao) = session.connection() {
        conexao.set_supressao_de_ruido(aplicada);
    }
    // E o teste em curso, se há um: a pessoa pode estar calibrando agora.
    if let Ok(teste) = session.teste_de_microfone.lock() {
        if let Some(teste) = teste.as_ref() {
            teste.set_supressao(aplicada);
        }
    }
    let Some(mut ajustes) = preferencias(&app) else {
        return Err("não consegui abrir as preferências".to_owned());
    };
    ajustes
        .set_supressao_de_ruido(forca)
        .map_err(|erro| format!("não consegui gravar a escolha: {erro}"))
}

/// A sensibilidade da ativação por voz, em dBFS. `None` volta ao padrão. F01.
///
/// O padrão depende de a redução de ruído estar ligada: o alvo de −60 dBFS é do
/// caminho **com** filtro, e sem ele o portão volta a ser a única defesa contra o
/// ventilador.
///
/// # Errors
///
/// Falha se as preferências não puderem ser gravadas.
#[tauri::command]
fn ajustar_sensibilidade_da_voz(
    app: AppHandle,
    session: State<'_, Session>,
    dbfs: Option<f32>,
) -> Result<(), String> {
    if let Ok(conexao) = session.connection() {
        conexao.set_abertura_da_voz_dbfs(dbfs);
    }
    if let Ok(teste) = session.teste_de_microfone.lock() {
        if let Some(teste) = teste.as_ref() {
            teste.set_abertura_dbfs(dbfs);
        }
    }
    let Some(mut ajustes) = preferencias(&app) else {
        return Err("não consegui abrir as preferências".to_owned());
    };
    ajustes
        .set_abertura_da_voz_dbfs(dbfs)
        .map_err(|erro| format!("não consegui gravar a escolha: {erro}"))
}

/// Os controles de voz que estão valendo. F01 e F02.
///
/// Fora de sessão a resposta vem do disco: a tela de configuração abre sem
/// servidor nenhum, e mostrar os controles vazios ali faria a pessoa achar que
/// não há o que ajustar.
#[tauri::command]
fn controles_da_voz(app: AppHandle, session: State<'_, Session>) -> seele_ffi::ControlesDaVoz {
    if let Ok(conexao) = session.connection() {
        if let Some(controles) = conexao.controles_da_voz() {
            return controles;
        }
    }
    let ajustes = preferencias(&app);
    let supressao = ajustes
        .as_ref()
        .and_then(seele_ffi::preferences::Preferences::supressao_de_ruido)
        .unwrap_or(1.0);
    let escolhida = ajustes
        .as_ref()
        .and_then(seele_ffi::preferences::Preferences::abertura_da_voz_dbfs);
    seele_ffi::ControlesDaVoz {
        supressao,
        // O mesmo padrão da sessão, e pela mesma regra: −60 dBFS é do caminho
        // com filtro. Duas cópias desta decisão discordariam no dia em que uma
        // mudasse, então ela vem dos nomes do núcleo.
        abertura_dbfs: escolhida.unwrap_or(if supressao > 0.0 {
            seele_ffi::ABERTURA_COM_SUPRESSAO_DBFS
        } else {
            seele_ffi::ABERTURA_SEM_SUPRESSAO_DBFS
        }),
        abertura_escolhida: escolhida.is_some(),
        abertura_minima_dbfs: seele_ffi::ABERTURA_MINIMA_DBFS,
        abertura_maxima_dbfs: seele_ffi::ABERTURA_MAXIMA_DBFS,
    }
}

/// Abre o teste de microfone. F01.
///
/// Idempotente: pedir de novo com um teste aberto fecha o anterior e abre outro,
/// que é o que trocar de aparelho no meio do teste precisa fazer.
///
/// # Errors
///
/// Falha quando não há microfone ou o aparelho escolhido sumiu.
#[tauri::command]
fn abrir_teste_de_microfone(app: AppHandle, session: State<'_, Session>) -> Result<(), String> {
    let ajustes = preferencias(&app);
    let supressao = ajustes
        .as_ref()
        .and_then(seele_ffi::preferences::Preferences::supressao_de_ruido)
        .unwrap_or(1.0);
    let abertura = ajustes
        .as_ref()
        .and_then(seele_ffi::preferences::Preferences::abertura_da_voz_dbfs);
    let captura = ajustes
        .as_ref()
        .and_then(|p| p.capture().map(str::to_owned));
    let saida = ajustes
        .as_ref()
        .and_then(|p| p.playback().map(str::to_owned));

    let aberto = seele_ffi::TesteDeMicrofone::abrir(captura, saida, supressao, abertura)
        // O motivo vai por extenso: o enum do núcleo é o vocabulário da ponte, e
        // a frase de tela é escrita em `frases.js` a partir dele.
        .map_err(|erro| format!("{erro:?}"))?;
    let Ok(mut slot) = session.teste_de_microfone.lock() else {
        return Err("o teste de microfone não pôde ser guardado".to_owned());
    };
    // O anterior é largado **antes** de o novo entrar: o `Drop` dele para a
    // thread, e dois testes ao mesmo tempo seriam dois microfones abertos.
    *slot = Some(aberto);
    Ok(())
}

/// Fecha o teste de microfone. Idempotente.
#[tauri::command]
fn fechar_teste_de_microfone(session: State<'_, Session>) {
    if let Ok(mut slot) = session.teste_de_microfone.lock() {
        *slot = None;
    }
}

/// O que o teste de microfone tem a contar agora. F01.
///
/// `None` quando não há teste aberto — a tela desenha o estado parado em vez de
/// números de uma medição que não está acontecendo.
#[tauri::command]
fn estado_do_teste_de_microfone(
    session: State<'_, Session>,
) -> Option<seele_ffi::EstadoDoTesteDeMicrofone> {
    session
        .teste_de_microfone
        .lock()
        .ok()?
        .as_ref()
        .map(seele_ffi::TesteDeMicrofone::estado)
}

/// Liga ou desliga ouvir o próprio microfone durante o teste. F01.
///
/// Começa desligado: tocar o microfone no alto-falante realimenta, e o nível
/// responde a maior parte da pergunta sem som nenhum.
#[tauri::command]
fn monitorar_o_microfone(session: State<'_, Session>, ligado: bool) {
    if let Ok(teste) = session.teste_de_microfone.lock() {
        if let Some(teste) = teste.as_ref() {
            teste.set_monitorar(ligado);
        }
    }
}

/// Libera a prévia automática de um domínio. ADR 0053.
///
/// Chamada pelo botão «mostrar imagem» que a janela desenha no lugar da prévia.
/// **É o clique que consente**, e ele consente para o domínio, uma vez — não
/// para um endereço, e não para todos.
///
/// # Errors
///
/// Falha se as preferências não puderem ser gravadas.
#[tauri::command]
fn consentir_previa(app: AppHandle, dominio: String) -> Result<(), String> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err("não consegui abrir as preferências".to_owned());
    };
    ajustes
        .consentir_previa(&dominio)
        .map_err(|erro| format!("não consegui gravar a escolha: {erro}"))
}

/// Retira a liberação de um domínio. ADR 0053.
///
/// # Errors
///
/// Falha se as preferências não puderem ser gravadas.
#[tauri::command]
fn esquecer_previa(app: AppHandle, dominio: String) -> Result<(), String> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err("não consegui abrir as preferências".to_owned());
    };
    ajustes
        .esquecer_previa(&dominio)
        .map_err(|erro| format!("não consegui gravar a escolha: {erro}"))
}

/// Os domínios cuja prévia automática está liberada. ADR 0053.
///
/// A tela de configuração os lista para que uma escolha feita num clique possa
/// ser desfeita num clique: um consentimento que não se enxerga é um
/// consentimento que ninguém revoga.
#[tauri::command]
fn previas_consentidas(app: AppHandle) -> Vec<String> {
    preferencias(&app).map_or_else(Vec::new, |ajustes| ajustes.previas_consentidas().to_vec())
}

/// Qual tecla abre o microfone em push-to-talk, ou `None` para a barra de espaço.
///
/// O valor é um `KeyboardEvent.code` e **atravessa opaco**: este lado nunca
/// decide o que uma tecla significa, só lembra qual foi escolhida. Quem lê
/// teclado é a casca, e é o único lugar que pode nomeá-las.
#[tauri::command]
fn tecla_de_falar(app: AppHandle) -> Option<String> {
    preferencias(&app).and_then(|p| p.push_to_talk_key().map(str::to_owned))
}

/// Escolhe a tecla que abre o microfone. `None` volta para a barra de espaço.
///
/// Só grava: não há nada a aplicar numa sessão viva, porque quem lê a tecla é a
/// casca e ela relê esta preferência quando muda.
///
/// # Errors
///
/// [`FalhaAoEscolher::NaoGravei`] se o disco recusar.
#[tauri::command]
fn escolher_tecla_de_falar(app: AppHandle, tecla: Option<String>) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    ajustes
        .set_push_to_talk_key(tecla.as_deref())
        .map_err(|erro| {
            tracing::warn!(%erro, "não consegui gravar a tecla de falar");
            FalhaAoEscolher::NaoGravei
        })
}

#[tauri::command]
fn set_volume(
    session: State<'_, Session>,
    nickname: String,
    percent: u16,
) -> Result<(), ConnectionError> {
    session.connection()?.set_volume(nickname, percent)
}

/// O volume que esta sessão guardou para cada pessoa, por apelido. F1-vazamento.
///
/// Um comando próprio, e não um campo do `snapshot`: o retrato vai inteiro aos
/// MODs, e quanto esta máquina quer ouvir cada um não é deles. A janela relê
/// isto a cada quadro em vez de lembrar o que mandou — a lembrança dela
/// sobrevivia à sessão e passava o volume de uma Rafa para a de outro servidor.
///
/// Sem sessão, nenhum: não há de quem.
#[tauri::command]
fn volumes_locais(session: State<'_, Session>) -> std::collections::HashMap<String, u16> {
    session
        .connection()
        .map(|conexao| conexao.volumes_locais())
        .unwrap_or_default()
}

// ------------------------------------------------------- compartilhar a tela
//
// Seis comandos e nenhuma decisão, como todo o resto deste arquivo. O teto de
// verdade é do core e tem de continuar lá: a decisão de 22/08 do
// `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` (§5.1)
// o escreve como
//
//     min(caminho de quem HOSPEDA × 60% ÷ N espectadores,
//         caminho de quem COMPARTILHA × 60%,
//         a escolha da pessoa)
//
// e as duas primeiras linhas são medidas que esta casca não tem e não deve
// tentar refazer. O que atravessa daqui é só a terceira — a escolha —, e ela é
// **teto**: `Snapshot::tela` volta dizendo o que de fato está saindo, e é isso
// que a tela desenha ao lado do que foi pedido.

/// As telas e janelas que esta máquina pode transmitir.
///
/// Uma lista vazia não é falha: é a resposta quando o sistema recusou, e
/// [`permissao_de_tela`] é quem diz qual foi a recusa. Por isso os dois são
/// comandos separados e a janela chama os dois — uma lista vazia sem motivo é
/// um beco.
#[tauri::command]
fn fontes_de_tela(
    session: State<'_, Session>,
) -> Result<Vec<seele_ffi::FonteDeTela>, ConnectionError> {
    session.connection()?.fontes_de_tela()
}

/// O que o sistema operacional respondeu sobre gravar a tela.
#[tauri::command]
fn permissao_de_tela(
    session: State<'_, Session>,
) -> Result<seele_ffi::PermissaoDeTela, ConnectionError> {
    Ok(session.connection()?.permissao_de_tela())
}

/// Pede a permissão ao sistema.
///
/// Só por aperto de botão, e nunca ao abrir a caixa: no macOS isto abre o
/// alerta do TCC, e um alerta de sistema que aparece sem ninguém ter pedido é
/// o que ensina a pessoa a recusar por reflexo — e o TCC não pergunta duas
/// vezes.
#[tauri::command]
fn pedir_permissao_de_tela(
    session: State<'_, Session>,
) -> Result<seele_ffi::PermissaoDeTela, ConnectionError> {
    Ok(session.connection()?.pedir_permissao_de_tela())
}

/// Começa a transmitir a fonte escolhida, com os limites escolhidos.
///
/// `ConnectionError::ScreenShareTaken` quando alguém já está compartilhando nesta
/// sala. Não é permissão que falta — a pessoa pode compartilhar assim que o
/// outro parar —, e é por isso que a frase dela em `ui/frases.js` manda esperar
/// em vez de mandar procurar um papel que ela já tem.
#[tauri::command]
fn compartilhar_tela(
    session: State<'_, Session>,
    fonte: u64,
    limites: seele_ffi::LimitesDeTela,
) -> Result<(), ConnectionError> {
    session.connection()?.compartilhar_tela(fonte, limites)
}

/// Para de transmitir. Idempotente do outro lado: parar sem estar
/// compartilhando não é erro.
#[tauri::command]
fn parar_de_compartilhar(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.parar_de_compartilhar()
}

/// Pede um quadro-chave a quem está compartilhando.
///
/// Chamado por quem **recebe**, no instante em que descobre uma transmissão em
/// curso. Sem ele, entrar numa sala em que alguém já compartilha é receber
/// diferenças de um quadro que nunca se viu — e o decodificador as descarta,
/// então a tela fica vazia até a transmissão recomeçar.
///
/// # Errors
///
/// [`ConnectionError::NotConnected`] sem sessão.
#[tauri::command]
fn pedir_quadro_chave(session: State<'_, Session>, tela: u32) -> Result<(), ConnectionError> {
    session.connection()?.pedir_quadro_chave(tela)
}

/// Passa a receber a imagem de uma transmissão, ou para de receber.
///
/// **Assistir é um pedido**, e é a tela quem decide o que está aberto. Enquanto
/// havia uma transmissão por sala, ela chegava sozinha — o certo, porque ninguém
/// quer clicar para ver a única coisa que está acontecendo. Com várias, cada
/// imagem aberta custa uma cópia na descida desta máquina e um decodificador na
/// CPU dela, e nenhuma das duas entra na conta do teto, que só mede a subida de
/// quem hospeda.
#[tauri::command]
fn assistir(session: State<'_, Session>, tela: u32, quero: bool) -> Result<(), ConnectionError> {
    session.connection()?.assistir(tela, quero)
}

/// O volume do som da transmissão que se está assistindo.
///
/// R21, o controle **de quem assiste**: silenciar o conteúdo compartilhado sem
/// calar as pessoas. Independente do volume dos participantes e do isolamento
/// total — este último continua calando tudo, que é o que ele diz que faz.
#[tauri::command]
fn ajustar_volume_da_tela(session: State<'_, Session>, volume: f32) -> Result<(), ConnectionError> {
    session.connection()?.ajustar_volume_da_tela(volume)
}

/// Cala ou devolve o som da transmissão, sem mexer no volume dela.
#[tauri::command]
fn calar_a_tela(session: State<'_, Session>, calada: bool) -> Result<(), ConnectionError> {
    session.connection()?.calar_a_tela(calada)
}

/// O volume e o mudo do som da transmissão, como a tela os desenha.
#[tauri::command]
fn som_da_tela(
    session: State<'_, Session>,
) -> Result<Option<seele_ffi::SomDaTela>, ConnectionError> {
    Ok(session.connection()?.som_da_tela())
}

/// O que esta máquina faz com o **próprio** áudio ao capturar uma tela.
///
/// R21. Não depende de sessão: é propriedade desta máquina e deste sistema, e a
/// pergunta é feita quando alguém está escolhendo o que compartilhar.
#[tauri::command]
fn exclusao_do_som_da_captura() -> seele_ffi::ExclusaoDoSom {
    seele_ffi::Connection::exclusao_do_som_da_captura()
}

/// Altera a resolução sem interromper a captura.
#[tauri::command]
fn ajustar_limites_da_tela(
    session: State<'_, Session>,
    limites: seele_ffi::LimitesDeTela,
) -> Result<(), ConnectionError> {
    session.connection()?.ajustar_limites_da_tela(limites)
}

/// Põe a janela em tela cheia, ou a tira de lá.
///
/// A janela do sistema **e** o corte da interface, e os dois são precisos: sem
/// a janela sobra a barra de título e a moldura do sistema em volta da imagem;
/// sem o corte, a interface inteira cresce junto e a tela compartilhada continua
/// dividindo espaço com o roster e os botões. Quem corta a interface é o CSS,
/// que responde ao `data-cinema`; esta metade é a janela.
///
/// Ignora a falta da janela em vez de reclamar: chamar isto durante o
/// fechamento não é erro de ninguém.
#[tauri::command]
fn tela_cheia(app: AppHandle, ligada: bool) {
    // Pelo rótulo, e por qualquer uma se o rótulo não achar. O `main` é o que o
    // Tauri dá quando o `tauri.conf.json` não nomeia a janela — e ele não a
    // nomeia. Depender de um padrão que não está escrito em lugar nenhum é o
    // tipo de coisa que some numa atualização do Tauri sem ninguém notar, e o
    // sintoma seria este botão parando de funcionar em silêncio.
    let janela = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    let Some(janela) = janela else {
        tracing::debug!(ligada, "não há janela para pôr em tela cheia");
        return;
    };
    if let Err(erro) = janela.set_fullscreen(ligada) {
        tracing::debug!(%erro, ligada, "a janela não trocou de modo");
    }
}

/// Anota o nome e a imagem do servidor na lista de visitados.
///
/// # Por que a tela chama isto, e não o Rust sozinho
///
/// Porque o momento é da tela. A aparência chega num quadro que o servidor manda
/// **depois** do aperto de mão, e `Connection::connect` já voltou quando ele chega —
/// é a mesma janela cega que faz o cabeçalho precisar sincronizar o ícone à mão
/// ao entrar. A tela é quem sabe que já sincronizou; daqui não dá para saber
/// sem inventar um prazo.
///
/// Sem argumentos de propósito: o endereço vem do que a sessão guardou, e o
/// nome e a imagem vêm da `Connection`. Uma tela que passasse os três poderia passar
/// os três de outro servidor.
#[tauri::command]
fn lembrar_aparencia_do_servidor(app: AppHandle, session: State<'_, Session>) {
    let Ok(Some(alvo)) = session.alvo.lock().map(|guardado| guardado.clone()) else {
        return;
    };
    let Ok(connection) = session.connection() else {
        return;
    };
    let nome = connection.snapshot().server;
    let icone = connection.server_icon();

    let Ok(mut lista) = seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
    else {
        return;
    };
    if let Err(erro) = lista.anotar_aparencia(&alvo, Some(&nome), icone.as_deref()) {
        // Um distintivo que não gravou é uma lista sem enfeite, e não uma falha
        // que valha interromper quem está entrando numa conversa.
        tracing::debug!(%erro, "não anotei a aparência deste servidor");
    }
}

/// O que o sistema deixa este app fazer com o microfone.
///
/// Sem sessão, de propósito: quem desconfia que está mudo quer a resposta antes
/// de entrar, e a tela de entrada não tem `Connection` nenhum para perguntar.
#[tauri::command]
fn permissao_de_microfone() -> seele_ffi::PermissaoDeMicrofone {
    seele_ffi::permissao_de_microfone()
}

/// Abre a página de privacidade do microfone do sistema.
///
/// # Por que abrir os Ajustes é o máximo que dá para fazer
///
/// Porque no Windows **não existe pedido a fazer**. Um app empacotado tem
/// prompt de consentimento e API; um app de área de trabalho — que é o que este
/// é, e o que o Discord também é — não tem nem um nem outro. O interruptor é da
/// pessoa, e o que este produto pode fazer é levá-la até ele em vez de deixá-la
/// procurar numa página longa.
///
/// `ms-settings:` é o esquema do próprio Windows, e não um caminho de arquivo:
/// ele abre a seção certa dos Ajustes.
///
/// Sem barulho quando falha: um botão de conveniência que não abriu não é
/// motivo para uma segunda mensagem de erro em cima da primeira.
#[tauri::command]
fn abrir_ajustes_do_microfone(app: AppHandle) {
    let _ = app;
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:privacy-microphone"])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn();
    }
}

/// Abre a página de acessibilidade do sistema, onde o contraste mora.
///
/// # Por que abrir os Ajustes é o máximo que dá para fazer, aqui também
///
/// U07 da auditoria de 20/09/2026: «Aparência explica uma correção histórica de
/// contraste e `prefers-contrast`, mas não oferece um controle direto
/// equivalente.» A seção dizia onde o contraste mora — o sistema operacional —
/// e parava ali.
///
/// A alternativa de ter um interruptor próprio foi recusada quando a seção
/// nasceu, e a razão continua boa: uma chave no app competindo com a do sistema
/// é a receita para as duas discordarem, e `acessibilidade.css` já segue
/// `prefers-contrast`. O que faltava era **levar até lá**, que é exatamente o
/// que `abrir_ajustes_do_microfone` faz para a permissão do microfone.
///
/// Sem barulho quando falha, pela mesma razão: um botão de conveniência que não
/// abriu não é motivo para uma segunda mensagem em cima da primeira. No Linux
/// não há um destino único — cada ambiente põe a acessibilidade num lugar —, e
/// por isso lá não há botão: a janela o esconde, em vez de desenhá-lo morto.
#[tauri::command]
fn abrir_ajustes_de_acessibilidade(app: AppHandle) {
    let _ = app;
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:easeofaccess-highcontrast"])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.universalaccess?Seeing_Display")
            .spawn();
    }
}

/// Se esta plataforma tem uma página de acessibilidade para abrir.
///
/// A janela esconde o botão quando não tem: «omitir o que não se tem em vez de
/// desenhá-lo morto» é a regra desta tela, escrita no guarda
/// `every_section_of_the_settings_screen_carries_the_panel_and_the_heading_it_opens`.
#[tauri::command]
const fn ha_ajustes_de_acessibilidade() -> bool {
    cfg!(any(windows, target_os = "macos"))
}

/// Fecha o aviso que está na tela, de verdade.
///
/// A janela escondia a caixa por conta própria, e o redesenho — que roda duas
/// vezes por segundo a partir do `Snapshot` — a trazia de volta. Apagar uma sala
/// com alguém dentro deixava a janela coberta por um alerta que não fechava.
#[tauri::command]
fn dispensar_aviso(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.dispensar_aviso();
    Ok(())
}

/// Os ajustes desta máquina, ou nada quando o disco não deixa lê-los.
///
/// `Option` e não `Result` porque nenhum chamador tem o que fazer com o motivo:
/// sem o arquivo, todo ajuste é o padrão, e é exatamente onde o app já estava
/// antes de o Terminal servidor existir. A mesma política da lista de visitados.
/// O que esta máquina aceita hoje no caminho entre pares, e o teto da versão.
///
/// **§5 da spec de tela, e o opt-in que não existia.** O núcleo, o protocolo e o
/// servidor traziam a malha inteira desde a v0.11.0, e nada em `apps/` ou no
/// `seele-ffi` chamava o consentimento — o próprio `enlace.rs` registrava isso
/// por escrito, e o roteiro de duas máquinas marcava o passo como bloqueado.
/// Sem um lugar para clicar, `emprestando` era sempre falso em produção e a
/// tela vinha sempre do servidor.
#[derive(Debug, serde::Serialize)]
struct CaminhoEntrePares {
    /// Quantos pares esta máquina aceita atender. `0` é «não empresto».
    pares_que_atende: u8,
    /// Se aceita que o próprio endereço seja entregue a quem for servi-la.
    assiste_por_par: bool,
    /// O máximo que **esta versão do cliente** consegue honrar.
    ///
    /// Vem do núcleo e não é escrito na tela: duas cópias do mesmo teto
    /// divergem no dia em que ele subir, e a tela passaria a oferecer o que o
    /// cliente não cumpre.
    teto_desta_versao: u8,
}

#[tauri::command]
fn caminho_entre_pares(app: AppHandle) -> CaminhoEntrePares {
    let (pares_que_atende, assiste_por_par) =
        preferencias(&app).map_or((0, false), |p| p.caminho_entre_pares());
    CaminhoEntrePares {
        pares_que_atende,
        assiste_por_par,
        teto_desta_versao: seele_ffi::pares_que_esta_versao_atende(),
    }
}

/// Guarda a escolha e a declara na sessão, se houver uma.
///
/// **As duas coisas, e nesta ordem.** Guardar sem declarar deixaria a escolha
/// valendo só na conexão seguinte; declarar sem guardar a perderia ao fechar a
/// janela. A retirada alcança o que já está no ar: o núcleo derruba os caminhos
/// de par abertos antes de a declaração nova sair.
///
/// # Errors
///
/// Nunca: sem sessão, a escolha fica guardada e vale na próxima conexão, que é
/// o desfecho certo para quem escolheu antes de entrar em algum lugar.
#[tauri::command]
fn consentir_no_caminho_entre_pares(
    app: AppHandle,
    session: State<'_, Session>,
    pares_que_atende: u8,
    assiste_por_par: bool,
) -> Result<(), ()> {
    if let Some(mut p) = preferencias(&app) {
        if let Err(erro) = p.set_caminho_entre_pares(pares_que_atende, assiste_por_par) {
            tracing::warn!(%erro, "não deu para guardar o consentimento de par");
        }
    }
    if let Ok(connection) = session.connection() {
        let _ = connection.consentir_no_caminho_entre_pares(pares_que_atende, assiste_por_par);
    }
    Ok(())
}

fn preferencias<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Option<seele_ffi::preferences::Preferences> {
    seele_ffi::preferences::Preferences::open(
        std::path::PathBuf::from(config_dir(app)).join("preferences"),
    )
    .ok()
}

/// Os microfones que esta máquina está oferecendo agora.
///
/// Respondível sem sessão: escolher microfone é coisa que se faz antes de
/// conectar tanto quanto durante, e pendurar a lista num `Connection` vivo poria o
/// controle atrás da porta que ele existe para abrir.
///
/// Lista vazia significa que a máquina não quis enumerar — **não** que não há
/// microfone. Quem desenha "sem áudio" a partir disto está escrevendo a frase
/// errada; `snapshot.audio_available` é a que quer dizer isso.
#[tauri::command]
fn microfones() -> Vec<seele_ffi::CaptureDevice> {
    seele_ffi::capture_devices()
}

/// Qual sistema está desenhando esta janela.
///
/// # Por que a casca precisa saber
///
/// Ela quase nunca precisa — e é regra desta janela não saber de plataforma,
/// porque o que muda entre elas é assunto do Rust. A barra de janela da 0.9.0 é
/// a exceção, e a exceção é de convenção e não de capacidade:
///
/// - **no macOS** os três semáforos ficam à esquerda e são do sistema. Eles
///   aparecem sobre a janela sem decoração quando se pede `Overlay`, e desenhar
///   os nossos ali seria duplicá-los ou tirá-los. A marca vai para a direita,
///   porque aquele canto é deles.
/// - **no Windows** não há semáforo nenhum sem decoração, então os três botões
///   são nossos, à direita, e a marca fica à esquerda como a comp a desenha.
///
/// Uma string e não um booleano: um `é_mac` responderia a esta pergunta e a
/// nenhuma outra, e a próxima diferença de convenção nasceria com um segundo
/// booleano ao lado.
#[tauri::command]
const fn plataforma() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "outro"
    }
}

/// Qual microfone está escolhido, ou `None` para o padrão da máquina.
///
/// Vem do disco e não do `Snapshot`: são duas perguntas diferentes. Esta é "o
/// que foi escolhido", que tem resposta sem sessão nenhuma; `snapshot.capture`
/// é "o que abriu de verdade", que só existe com áudio de pé. As duas divergem
/// justamente quando importa — um dispositivo escolhido e desconectado.
#[tauri::command]
fn microfone_escolhido(app: AppHandle) -> Option<String> {
    preferencias(&app).and_then(|p| p.capture().map(str::to_owned))
}

/// Por que não deu para escolher esse microfone.
///
/// Enum, e não frase, pela mesma razão que [`FalhaAoHospedar`]: a fronteira
/// erro→texto do produto está no frontend, e uma mensagem escrita aqui seria uma
/// frase que nenhum tradutor alcança.
///
/// Duas e não uma porque pedem coisas diferentes de quem está na frente da tela.
/// Uma não tem conserto ali — o disco recusou —, e a outra tem: a lista está
/// logo acima, e o que sumiu entre desenhá-la e clicar nela pode ser trocado por
/// outro. Nenhuma delas é `ConnectionError::IdentityUnavailable`, que era o que este
/// comando devolvia: a frase daquela fala de identidade em disco, e acusar a
/// chave da pessoa por causa de um arquivo de ajustes manda quem lê procurar no
/// lugar errado.
#[derive(Debug, serde::Serialize)]
enum FalhaAoEscolher {
    /// O ajuste não pôde ser gravado nesta máquina.
    NaoGravei,
    /// O microfone não está mais aqui.
    DispositivoSumiu,
}

/// Escolhe o microfone: grava no disco e, se houver sessão, troca agora.
///
/// As duas metades, e nesta ordem, porque falham de jeitos diferentes. A
/// escrita é o que faz a escolha valer amanhã; a troca é o que faz valer agora.
/// Se a troca falhar — o dispositivo saiu do lugar entre desenhar a lista e
/// clicar nela — a escolha continua gravada, e é a certa: quem religar a
/// interface volta ao microfone que queria sem escolher de novo.
///
/// Sem sessão não é falha. Escolher microfone na tela de entrada é o caminho
/// normal, e é para ela que a próxima conexão vai olhar.
#[tauri::command]
fn escolher_microfone(
    app: AppHandle,
    session: State<'_, Session>,
    dispositivo: Option<String>,
) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    if let Err(erro) = ajustes.set_capture(dispositivo.as_deref()) {
        tracing::warn!(%erro, "não consegui gravar o microfone escolhido");
        return Err(FalhaAoEscolher::NaoGravei);
    }

    let Ok(connection) = session.connection() else {
        // Sem sessão a escolha está gravada, e era tudo o que havia para fazer.
        return Ok(());
    };
    connection.set_capture_device(dispositivo).map_err(|erro| {
        tracing::warn!(%erro, "não consegui trocar o microfone da sessão");
        FalhaAoEscolher::DispositivoSumiu
    })
}

/// Por onde esta máquina está oferecendo tocar som agora.
///
/// O irmão de [`microfones`], respondível sem sessão pelo mesmo motivo. E a
/// mesma advertência sobre lista vazia: significa que a máquina não quis
/// enumerar, **não** que não há onde tocar.
#[tauri::command]
fn saidas() -> Vec<seele_ffi::PlaybackDevice> {
    seele_ffi::playback_devices()
}

/// Qual saída está escolhida, ou `None` para o padrão da máquina.
///
/// Do disco e não do `Snapshot`, pela mesma razão que [`microfone_escolhido`], e
/// aqui a diferença entre as duas perguntas é a única coisa visível: cair para o
/// alto-falante da máquina não faz barulho nenhum próprio. Quem escolheu um fone
/// e não ouve nada tem `snapshot.playback` — o que abriu — para comparar com
/// isto, o que foi pedido.
#[tauri::command]
fn saida_escolhida(app: AppHandle) -> Option<String> {
    preferencias(&app).and_then(|p| p.playback().map(str::to_owned))
}

/// Escolhe a saída de som: grava no disco e, se houver sessão, troca agora.
///
/// As duas metades e nesta ordem, pelas mesmas razões que [`escolher_microfone`]
/// dá. Uma diferença de peso está do outro lado, no core: trocar de saída não
/// desliga o Isolamento total. Quem mexe neste controle costuma ser exatamente
/// quem não está ouvindo nada, e às vezes o motivo de não ouvir é que se mutou —
/// uma troca que desmutasse calada poria um servidor dentro de uma sala que estava
/// em silêncio.
#[tauri::command]
fn escolher_saida(
    app: AppHandle,
    session: State<'_, Session>,
    dispositivo: Option<String>,
) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    if let Err(erro) = ajustes.set_playback(dispositivo.as_deref()) {
        tracing::warn!(%erro, "não consegui gravar a saída de som escolhida");
        return Err(FalhaAoEscolher::NaoGravei);
    }

    let Ok(connection) = session.connection() else {
        // Sem sessão a escolha está gravada, e era tudo o que havia para fazer.
        return Ok(());
    };
    connection.set_playback_device(dispositivo).map_err(|erro| {
        tracing::warn!(%erro, "não consegui trocar a saída de som da sessão");
        FalhaAoEscolher::DispositivoSumiu
    })
}

/// Onde fica a lista de servidores visitados.
fn caminho_dos_conhecidos<R: tauri::Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    std::path::PathBuf::from(config_dir(app)).join("conhecidos")
}

/// Os servidores onde esta pessoa já esteve.
///
/// Uma lista de atalhos corrompida não pode fechar a porta: `specs/05` diz que
/// este arquivo é conveniência e pode ser apagado sem consequência. Por isso
/// falha vira lista vazia, e nunca erro — a tela esconde a seção e o formulário
/// de sempre continua ali.
#[tauri::command]
fn conhecidos(app: AppHandle) -> Vec<seele_ffi::conhecidos::Conhecido> {
    seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
        .map(|lista| lista.listar())
        .unwrap_or_default()
}

/// Tira um servidor da lista.
///
/// Um `Ok` quando a lista nem abriu é a resposta certa e não uma mentira: o que
/// se pediu foi que aquele endereço não estivesse mais lá, e ele não está.
#[tauri::command]
fn esquecer(app: AppHandle, alvo: String) -> Result<(), ()> {
    let Ok(mut lista) = seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
    else {
        return Ok(());
    };
    lista.esquecer(&alvo).map_err(|_| ())
}

/// O que um `seele://` colado diz à tela de entrada.
///
/// Deliberadamente **não** é o `Convite` inteiro do core: o que a tela de
/// entrada tem o que fazer com é o endereço a preencher e o token a devolver no
/// `connect`. A impressão digital do link fica em [`Session::convite`], que é de
/// onde `connect` a tira para conferir — mandá-la para cá seria mandar ao
/// frontend a **entrada** de uma comparação, e é isso que
/// `specs/06-clientes-gui.md:19` proíbe. O que ele recebe sobre identidade é o
/// veredito de depois, em [`Entrada::veredito`].
///
/// Não há mais campo dizendo que a conferência ficou pendente porque não fica:
/// o link que traz `fp` é conferido no `connect` seguinte, e o resultado tem
/// frase própria na tela.
#[derive(Debug, serde::Serialize)]
struct ConviteLido {
    /// O endereço do servidor, para o campo SERVER.
    alvo: String,
    /// O convite de uso único, quando o link trouxe um.
    token: Option<String>,
    /// Que versão do SEELE hospeda este servidor, quando o link a traz.
    ///
    /// ADR 0046. `None` num link de antes deste campo, ou de um servidor que
    /// não sabe a própria versão.
    versao: Option<String>,
    /// O que fazer com este link antes de discar: entrar aqui, abrir o app da
    /// linha do servidor, baixá-lo antes, ou dizer que a linha saiu.
    ///
    /// Respondido no Rust e não na casca: a resposta depende da linha deste
    /// app, do catálogo guardado e do depósito, que é disco. A casca recebe o
    /// veredito. Ver `versoes::para_entrar`.
    para_entrar: versoes::ParaEntrar,
}

/// Lê um `seele://` colado.
///
/// Mora aqui e não no JavaScript porque `specs/06:19` é inegociável, e porque um
/// segundo analisador de URI seria um segundo conjunto de casos de borda para
/// discordar do primeiro.
#[tauri::command]
fn analisar_convite(
    app: AppHandle,
    session: State<'_, Session>,
    link: String,
) -> Result<ConviteLido, String> {
    let convite =
        seele_ffi::uri::analisar(&link).map_err(|erro| nome_da_falha(&erro).to_owned())?;

    // A versão que o link anuncia, e se dá para abri-la aqui. Ver ADR 0046: é
    // **antes** de conectar que esta pergunta serve, porque um servidor de uma
    // versão anterior pode falar um protocolo que este cliente já não alcança.
    let versao = convite.versao.clone();
    let para_entrar = versoes::para_entrar(
        &config_dir(&app),
        versoes::atualizador_do_app(&app)
            .as_ref()
            .map(|atualizador| &atualizador.chave),
        versao.as_deref(),
    );

    let lido = ConviteLido {
        alvo: convite.alvo.clone(),
        token: convite.token.clone(),
        versao,
        para_entrar,
    };

    // Guardado inteiro deste lado da ponte. Ver o campo em `Session`.
    if let Ok(mut slot) = session.convite.lock() {
        *slot = Some(convite);
    }

    Ok(lido)
}

/// O caminho pela recusa (tarefa 13 da F6, passo 5; P8 da F1): o servidor
/// recusou o número deste app dizendo, no byte 0 da resposta, que fala o
/// Servidor `linha`. Responde o mesmo `versoes::ParaEntrar` do
/// `analisar_convite`, pelo mesmo núcleo, para a casca seguir pelo mesmo
/// `entrarPelaLinha`.
///
/// `linha` é o `ConnectionError::OutraLinha::servidor`, que a FFI só manda
/// para uma versão que o launcher abre («0.15»).
#[tauri::command]
fn para_entrar_pela_recusa(app: AppHandle, linha: String) -> versoes::ParaEntrar {
    versoes::para_entrar_pela_recusa(
        &config_dir(&app),
        versoes::atualizador_do_app(&app)
            .as_ref()
            .map(|atualizador| &atualizador.chave),
        &linha,
    )
}

/// O nome estável de uma falha ao ler um convite.
///
/// Um `match` e não o `Display` do core. O `Display` de `ErroDeUri` hoje escreve
/// o nome da variante, mas isso é escolha do core, e a frase que a pessoa lê
/// está no `FRASES` do JavaScript amarrada a este nome — igual a todo o resto
/// dos erros, porque nenhuma mensagem para gente é escrita em Rust. Escrito
/// aqui, a próxima variante para a compilação em vez de virar uma tela que diz
/// o nome de uma variante em inglês.
fn nome_da_falha(erro: &seele_ffi::uri::ErroDeUri) -> &'static str {
    use seele_ffi::uri::ErroDeUri as Falha;
    match erro {
        Falha::EsquemaDesconhecido => "EsquemaDesconhecido",
        Falha::SemEndereco => "SemEndereco",
        Falha::EnderecoInvalido => "EnderecoInvalido",
        // Falta a frase em `ui/frases.js`, e de propósito: outro agente está
        // naquele diretório agora. Até ela chegar, `desconhecida()` mostra o
        // nome em vez de um beco sem saída — que é o que aquele fallback existe
        // para fazer. A frase a escrever é sobre pôr o IPv6 entre colchetes.
        Falha::EnderecoIpv6SemColchetes => "EnderecoIpv6SemColchetes",
        // Degrau 4 do ADR 0022: o `enc` do convite veio pela metade ou com um
        // endereço que não é um. Nome próprio porque a frase é própria — meio
        // bilhete não leva a lugar nenhum, e o resto do link continua bom.
        Falha::BilheteInvalido => "BilheteInvalido",
        Falha::ImpressaoDigitalInvalida => "ImpressaoDigitalInvalida",
        Falha::TokenInvalido => "TokenInvalido",
        Falha::VoiceRoomInvalido => "VoiceRoomInvalido",
    }
}

// ---------------------------------------------------------------------------
// A portaria — ADR 0030. A porta do servidor que esta janela hospeda.
// ---------------------------------------------------------------------------
//
// Estes comandos falam **direto com o PERSISTENCE do servidor embutido**, e não pelo fio
// como toda a moderação faz. Não é atalho; é o ADR 0030:
//
// - fechar a porta não pode depender de estar dentro, senão a defesa depende do
//   canal que ela defende, e só fecha quem já tinha entrado enquanto estava
//   aberto;
// - a porta se fecha antes do primeiro pacote, no mesmo gesto de hospedar;
// - e nenhum verbo novo de protocolo é nenhuma superfície nova exposta à
//   internet para uma decisão que é, por definição, de quem está na máquina.
//
// O custo, que é real e está no ADR: isto não administra o servidor de outra
// pessoa. Quem está conectado ao servidor de um amigo não vê nenhuma destas telas,
// e é por isso que todas começam por `NaoEstaHospedando`.

/// Por que um comando da portaria não deu.
///
/// Enum e não frase, como as vizinhas: a fronteira erro→texto é do frontend.
#[derive(Debug, serde::Serialize)]
enum FalhaNaPortaria {
    /// Esta janela não está hospedando nada, então não há porta para mexer.
    NaoEstaHospedando,
    /// O banco do servidor não respondeu.
    BancoNaoRespondeu,
}

/// Em que estado está a porta do servidor que esta janela hospeda.
///
/// Uma leitura só, porque as quatro coisas são lidas juntas e mostradas juntas:
/// quem hospeda precisa ver **as três camadas ao mesmo tempo** para entender o
/// que está valendo. Ver `portaria` em `seele-server` e o ADR 0030.
#[derive(Debug, serde::Serialize)]
struct EstadoDaPorta {
    /// Se há um servidor no ar nesta janela.
    hospedando: bool,
    /// Sem senha e sem convites: qualquer um que alcance a porta passa a
    /// primeira camada. ADR 0021.
    aberto: bool,
    /// Se há senha do servidor configurada.
    tem_senha: bool,
    /// Se há pelo menos um convite emitido.
    aceita_convites: bool,
    /// Se a portaria pergunta antes de deixar entrar quem nunca entrou.
    portaria_ligada: bool,
    /// Quantos pedidos esperam decisão. O número que o cartão mostra sem
    /// desenhar a fila inteira.
    pendentes: i64,
    /// Até onde este servidor é alcançável, pelo degrau do ADR 0022.
    ///
    /// É o que transforma «está aberto» em «está aberto **para a internet**»,
    /// que são duas frases com urgências diferentes.
    alcance: &'static str,
}

/// Um pedido, como o cartão o desenha.
///
/// A ordem dos campos aqui não é a ordem da tela, e a da tela é que importa: a
/// impressão digital em primeiro lugar, o apelido abaixo e entre aspas. Ver o
/// ADR 0030 — título é do que a pessoa é, e quem bateu ainda não é nada.
#[derive(Debug, serde::Serialize)]
struct PedidoNaTela {
    /// SHA-256 da chave pública. **A identidade.**
    impressao: String,
    /// O apelido pedido. Texto que a pessoa digitou, e nada além disso.
    apelido: String,
    /// `aberto` | `senha` | `convite`.
    segredo: String,
    /// A observação que quem hospeda escreveu ao gerar o convite.
    observacao: String,
    /// Quando bateu pela primeira vez, em segundos.
    bateu_em: i64,
    /// Quantas vezes bateu.
    batidas: i64,
    /// `null` enquanto ninguém decidiu.
    decidido_em: Option<i64>,
    /// Se a decisão foi admitir.
    admitido: bool,
}

/// O `Arc` do PERSISTENCE do servidor hospedado, ou a recusa.
///
/// Clonado para fora do `Mutex` do app **antes** de qualquer `await`: segurar um
/// `std::sync::MutexGuard` atravessando um ponto de espera trava os dois
/// cadeados de uma vez e nem compila do lado do Tauri.
fn persistence_hospedada(
    session: &State<'_, Session>,
) -> Result<seele_server::hospedagem::PersistenceCompartilhada, FalhaNaPortaria> {
    let hospedagens = session
        .hospedagens
        .lock()
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let server = hospedagens
        .do_guardado()
        .ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
    Ok(server.persistence())
}

/// O que o servidor que esta janela hospeda tem a dizer de si, para a tela de
/// quem hospeda (a P12 do plano F1b).
///
/// Hoje, as mensagens de controle de um vocabulário mais novo que o servidor
/// pulou: quem usa um SEELE mais novo está mandando o que esta versão não
/// entende, e atualizar o SEELE daqui resolve. Sem isto, o servidor sabia e só
/// dizia no `seele.log`. **Uma janela que não hospeda responde
/// `hospedando: false`**, e a tela não desenha a linha: não há servidor de que
/// falar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
struct EstadoDoServidor {
    /// Se há um servidor no ar nesta janela.
    hospedando: bool,
    /// Quantas mensagens de um vocabulário mais novo o servidor pulou.
    mensagens_desconhecidas: u64,
    /// Em quantas sessões isso aconteceu.
    sessoes_com_desconhecidas: u64,
}

/// O estado de uma hospedagem, ou o de quem não hospeda.
fn estado_do_servidor_de(
    estado: Option<seele_server::hospedagem::EstadoDoServidor>,
) -> EstadoDoServidor {
    estado.map_or_else(EstadoDoServidor::default, |estado| EstadoDoServidor {
        hospedando: true,
        mensagens_desconhecidas: estado.mensagens_desconhecidas,
        sessoes_com_desconhecidas: estado.sessoes_com_desconhecidas,
    })
}

/// O estado do servidor que esta janela hospeda. Ver [`EstadoDoServidor`].
///
/// Lido do servidor embutido, em memória: barato o bastante para a tela de
/// configurações perguntar a cada volta do laço dela.
#[tauri::command]
fn estado_do_servidor(session: State<'_, Session>) -> Result<EstadoDoServidor, &'static str> {
    // O servidor guardado, que é o da seção ESTE SERVIDOR; a sala pessoal não
    // tem configurações e não entra aqui.
    let hospedagens = session
        .hospedagens
        .lock()
        .map_err(|_| "HospedagemIndisponivel")?;
    Ok(estado_do_servidor_de(
        hospedagens
            .do_guardado()
            .map(seele_server::hospedagem::Hospedagem::estado),
    ))
}

#[cfg(test)]
mod o_estado_do_servidor {
    use super::*;

    #[test]
    fn quem_nao_hospeda_diz_que_nao_hospeda_e_nada_mais() {
        assert_eq!(
            estado_do_servidor_de(None),
            EstadoDoServidor {
                hospedando: false,
                mensagens_desconhecidas: 0,
                sessoes_com_desconhecidas: 0,
            }
        );
    }

    #[test]
    fn quem_hospeda_diz_os_numeros_do_servidor_e_o_zero_tambem() {
        let numeros = |mensagens, sessoes| {
            estado_do_servidor_de(Some(seele_server::hospedagem::EstadoDoServidor {
                mensagens_desconhecidas: mensagens,
                sessoes_com_desconhecidas: sessoes,
            }))
        };
        assert_eq!(
            numeros(0, 0),
            EstadoDoServidor {
                hospedando: true,
                mensagens_desconhecidas: 0,
                sessoes_com_desconhecidas: 0,
            }
        );
        assert_eq!(
            numeros(3, 1),
            EstadoDoServidor {
                hospedando: true,
                mensagens_desconhecidas: 3,
                sessoes_com_desconhecidas: 1,
            }
        );
    }
}

/// O estado das três camadas da porta.
#[tauri::command]
async fn estado_da_porta(session: State<'_, Session>) -> Result<EstadoDaPorta, FalhaNaPortaria> {
    let (persistence, alcance) = {
        let hospedagens = session
            .hospedagens
            .lock()
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
        let Some(server) = hospedagens.do_guardado() else {
            // Não hospedar não é falha: é a resposta, e a tela desenha a partir
            // dela em vez de mostrar um erro a quem não pediu nada.
            return Ok(EstadoDaPorta {
                hospedando: false,
                aberto: false,
                tem_senha: false,
                aceita_convites: false,
                portaria_ligada: false,
                pendentes: 0,
                alcance: "SoRedeLocal",
            });
        };
        let alcance = server
            .alcance()
            .map_or("SoRedeLocal", |alcance| alcance.degrau().nome());
        (server.persistence(), alcance)
    };

    let persistence = persistence.lock().await;
    let politica = seele_server::admissao::Politica::carregar(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let portaria_ligada = seele_server::portaria::ligada(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let pendentes = seele_server::portaria::pendentes(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;

    Ok(EstadoDaPorta {
        hospedando: true,
        aberto: politica.aberto(),
        tem_senha: politica.tem_senha(),
        aceita_convites: politica.aceita_convites(),
        portaria_ligada,
        pendentes,
        alcance,
    })
}

/// Põe ou tira a senha do servidor. `None` tira. ADR 0021.
#[tauri::command]
async fn definir_senha_do_server(
    session: State<'_, Session>,
    senha: Option<String>,
) -> Result<(), FalhaNaPortaria> {
    let persistence = persistence_hospedada(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::admissao::definir_senha(&mut persistence, senha.as_deref())
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// Gera um convite de uso único e devolve o link inteiro para mandar.
///
/// O link, e não o token cru: o token sozinho obriga quem recebe a saber o
/// endereço por outro caminho, e é o link que a outra ponta já sabe colar.
#[tauri::command]
async fn criar_convite_do_server(
    session: State<'_, Session>,
    observacao: String,
) -> Result<String, FalhaNaPortaria> {
    let (persistence, ..) = {
        let hospedagens = session
            .hospedagens
            .lock()
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
        let server = hospedagens
            .do_guardado()
            .ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
        (server.persistence(), ())
    };

    let token = {
        let mut persistence = persistence.lock().await;
        seele_server::admissao::criar_convite(&mut persistence, &observacao)
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?
    };

    let hospedagens = session
        .hospedagens
        .lock()
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let server = hospedagens
        .do_guardado()
        .ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
    Ok(server.convite_com_token(&token))
}

/// Liga ou desliga a portaria. ADR 0030.
#[tauri::command]
async fn ligar_portaria(session: State<'_, Session>, ligada: bool) -> Result<(), FalhaNaPortaria> {
    let persistence = persistence_hospedada(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::portaria::ligar(&mut persistence, ligada)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// A fila e o histórico da portaria.
#[tauri::command]
async fn pedidos_da_portaria(
    session: State<'_, Session>,
) -> Result<Vec<PedidoNaTela>, FalhaNaPortaria> {
    let persistence = persistence_hospedada(&session)?;
    let persistence = persistence.lock().await;
    let fila = seele_server::portaria::pedidos(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;

    Ok(fila
        .into_iter()
        .map(|pedido| PedidoNaTela {
            impressao: pedido.impressao,
            apelido: pedido.apelido,
            segredo: pedido.segredo,
            observacao: pedido.observacao,
            bateu_em: pedido.bateu_em,
            batidas: pedido.batidas,
            decidido_em: pedido.decidido_em,
            admitido: pedido.admitido,
        })
        .collect())
}

/// Quem hospeda decide sobre um pedido.
#[tauri::command]
async fn decidir_pedido(
    session: State<'_, Session>,
    impressao: String,
    admitir: bool,
) -> Result<(), FalhaNaPortaria> {
    let persistence = persistence_hospedada(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::portaria::decidir(&mut persistence, &impressao, admitir)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// Desfaz uma decisão: a pessoa volta a ser desconhecida.
///
/// Não é banir e não derruba quem está dentro — ADR 0030. A frase que a tela
/// mostra antes de fazer isto é que precisa dizer as duas coisas.
#[tauri::command]
async fn revogar_admissao(
    session: State<'_, Session>,
    impressao: String,
) -> Result<(), FalhaNaPortaria> {
    let persistence = persistence_hospedada(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::portaria::revogar(&mut persistence, &impressao)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// O que o frontend precisa saber sobre a busca corrente.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct BuscaEstado {
    /// Onde o termo casou, na ordem em que a tela desenha.
    casamentos: Vec<seele_ffi::search::Match>,
    /// A ocorrência em que o cursor está.
    atual: Option<seele_ffi::search::Match>,
    /// Qual ocorrência **dentro da própria mensagem** é a do cursor, de zero.
    ///
    /// Sem isto o app desenhava todo casamento igual, e o cursor sumia dentro de
    /// uma mensagem: em "sync caiu, o sync voltou, e o sync nem caiu" com o
    /// contador em `[2/3]`, apertar a próxima duas vezes mudava o algarismo e
    /// mais nada na tela. `atual` sozinho só alcança **qual mensagem**, que é
    /// por onde a tela rola; é este número que alcança **qual trecho**.
    ///
    /// Vem do core, de `Search::ordinal_in_message`, que existe para isto: a
    /// casca que pinta o histórico inteiro de uma vez conta as ocorrências de
    /// cada mensagem do mesmo jeito, e precisa deste índice para distinguir a
    /// do cursor das vizinhas. Contar do outro lado da ponte seria reescrever
    /// em JavaScript uma contagem que o Rust já faz — e que ele tem de fazer,
    /// porque é ela que decide o `[n/m]`.
    ordinal: Option<u32>,
    /// Posição a partir de 1, para desenhar "[1/3]". Zero quando não casou nada.
    posicao: u32,
    /// Quantas ao todo.
    total: u32,
}

impl BuscaEstado {
    fn de(busca: &seele_ffi::search::Search) -> Self {
        let (posicao, total) = busca.position();
        Self {
            // Todos, e não só o corrente: o app pinta o histórico inteiro de uma
            // vez, e acender só a ocorrência do cursor esconderia as outras que
            // estão na mesma tela.
            casamentos: busca.matches().to_vec(),
            atual: busca.current(),
            ordinal: busca
                .ordinal_in_message()
                .and_then(|ordinal| u32::try_from(ordinal).ok()),
            posicao: u32::try_from(posicao).unwrap_or(0),
            total: u32::try_from(total).unwrap_or(0),
        }
    }
}

/// Roda o termo sobre o histórico desta sessão.
///
/// Os corpos vão **crus**, e é isso que amarra os deslocamentos ao texto que a
/// tela desenha: `.mensagens .corpo` é `white-space: pre-wrap`, então esta
/// janela mostra quebra de linha e espaço duplo como eles chegaram. Normalizar
/// aqui erraria o alvo em toda mensagem com espaço colapsado, e achatar o
/// desenho para consertar isso seria trocar o produto pela implementação.
///
/// O `connection` faz o contrário — e está certo: `ui::wrap` já colapsa antes de
/// desenhar, então lá o texto na tela é o normalizado. Cada casca busca o que
/// desenha.
///
/// O custo, dito: com `' '` e `'\n'` sendo caracteres diferentes, um termo com
/// espaço não casa por cima de uma quebra de linha.
///
/// # O cursor atravessa a reconstrução
///
/// Este comando é chamado de novo a cada mensagem que chega, porque os índices
/// andam e um realce guardado passaria a acender o trecho errado. Uma busca
/// recém-construída começa na ocorrência um — e era isso que, numa Linha
/// movimentada, jogava quem estava na 7 de 12 de volta para a 1 e puxava o
/// painel junto, toda vez que alguém falava. Numa conversa viva não dava para
/// segurar a posição, que é exatamente onde buscar serve para alguma coisa.
///
/// `Search::resume_at` é a mesma regra que o `connection` já usava em
/// `App::refazer_busca`, e mora no core justamente para não haver duas. O
/// cursor continua deste lado da ponte, em `Session::busca`: o JavaScript não
/// decide nada sobre busca (`specs/06-clientes-gui.md:19`).
#[tauri::command]
fn buscar(session: State<'_, Session>, termo: String) -> Result<BuscaEstado, ConnectionError> {
    // `messages()` e não `snapshot()`: a conversa saiu do snapshot, que agora
    // carrega só a revisão dela. Aqui a lista inteira é mesmo necessária — é o
    // que se busca —, e este comando roda quando alguém digita, não a cada
    // quadro.
    let mensagens = session.connection()?.messages();
    let mut busca =
        seele_ffi::search::Search::new(mensagens.iter().map(|mensagem| &mensagem.body), &termo);

    let Ok(mut slot) = session.busca.lock() else {
        // Sem o cadeado não há cursor anterior a carregar nem onde guardar o
        // novo. A busca ainda vale; o que se perde é a continuidade.
        return Ok(BuscaEstado::de(&busca));
    };
    if let Some(anterior) = slot.as_ref().and_then(seele_ffi::search::Search::current) {
        busca.resume_at(anterior);
    }
    let estado = BuscaEstado::de(&busca);
    *slot = Some(busca);
    Ok(estado)
}

/// Anda uma ocorrência, dando a volta nas pontas.
#[tauri::command]
fn busca_andar(session: State<'_, Session>, adiante: bool) -> BuscaEstado {
    let Ok(mut slot) = session.busca.lock() else {
        return BuscaEstado::default();
    };
    let Some(busca) = slot.as_mut() else {
        return BuscaEstado::default();
    };
    if adiante {
        busca.next_match();
    } else {
        busca.previous_match();
    }
    BuscaEstado::de(busca)
}

/// Apaga a busca. Não falha: esvaziar o campo nunca pode dar erro.
#[tauri::command]
fn busca_limpar(session: State<'_, Session>) {
    if let Ok(mut slot) = session.busca.lock() {
        *slot = None;
    }
}

// ------------------------------------------------------------- atualização
//
// Duas metades, e a divisão é a decisão: `procurar_atualizacao` só olha, e
// `instalar_atualizacao` só instala o que já foi olhado. Nada aqui roda sozinho.
//
// **Não há consulta automática ao abrir.** Num produto cujo argumento é que o
// servidor é seu, um app que fala com o github.com a cada arranque sem ninguém
// pedir contradiz o argumento — e o que o parceiro pediu foi um *botão*, não uma
// vigilância. Quem quiser saber aperta; quem não apertar nunca sai daqui.
//
// ADR 0026 registra as duas assinaturas em jogo e por que são duas.

/// O que uma versão nova diz de si, antes de baixar um byte.
#[derive(Debug, serde::Serialize)]
struct VersaoNova {
    /// A versão que está sendo oferecida.
    versao: String,
    /// A que está rodando agora, para a tela poder escrever «de X para Y».
    instalada: String,
    /// As notas do release, quando o manifesto trouxe alguma.
    notas: Option<String>,
}

/// Quanto do pacote já veio.
///
/// `total` é `Option` porque o servidor pode não mandar `Content-Length`. Uma
/// tela que assumir cem por cento conhecido desenha uma barra que trava; sem o
/// total, o desenho certo é o indeterminado.
#[derive(Debug, Clone, serde::Serialize)]
struct Andamento {
    /// Bytes recebidos até agora.
    baixados: u64,
    /// Bytes ao todo, quando o servidor disse quantos são.
    total: Option<u64>,
}

/// Por que não deu para atualizar.
///
/// Enum e não frase, pela mesma razão de [`FalhaAoHospedar`]: a fronteira
/// erro→texto está no frontend. Seis e não uma porque pedem seis coisas
/// diferentes de quem está na frente da tela — de «não há nada a fazer, este app
/// não foi empacotado com atualizador» a «tente de novo daqui a pouco».
#[derive(Debug, serde::Serialize)]
enum FalhaAoAtualizar {
    /// Este app saiu sem chave de atualização, e por isso não atualiza.
    ///
    /// Não é defeito nem erro de rede: é um build feito antes de a chave do
    /// projeto existir, ou um build de quem compilou do código-fonte. A tela
    /// certa aqui é a que manda baixar da página de releases — o caminho que
    /// sempre existiu — e **não** a que manda tentar de novo.
    NaoConfigurado,
    /// Não deu para alcançar o manifesto, ou ele não era o que se esperava.
    NaoAlcancei,
    /// A página de releases respondeu, e não há release com manifesto nela.
    ///
    /// Separado de [`Self::NaoAlcancei`] porque a diferença é tudo para quem
    /// lê. Ali a rede falhou e tentar de novo faz sentido; aqui a rede
    /// funcionou perfeitamente e a resposta foi «não há nada publicado» —
    /// tentar de novo não muda nada, e mandar a pessoa conferir a conexão a
    /// manda procurar defeito onde não há.
    ///
    /// É o estado normal de um projeto que ainda não publicou release nenhum
    /// com `latest.json` ao lado, e foi assim que ele apareceu: o botão
    /// respondeu «a página de releases não respondeu» sobre uma página que
    /// tinha respondido.
    NadaPublicado,
    /// Há versão nova, mas não para este sistema ou esta arquitetura.
    SemPacoteParaEsteSistema,
    /// O pacote baixado **não** foi assinado com a chave deste projeto.
    ///
    /// A falha mais séria da lista, e a única que não é para tentar de novo. O
    /// canal de atualização é onde comprometer um produto é mais barato, e este
    /// é o ponto em que a tentativa é recusada. O pacote é descartado sem
    /// tocar em nada instalado.
    AssinaturaRecusada,
    /// O pacote chegou inteiro e conferido, e a troca dos arquivos falhou.
    NaoInstalei,
    /// Pediram para instalar sem ter procurado antes.
    NadaEscolhido,
    /// Esta versão foi aberta pelo launcher de outra linha, e quem a atualiza
    /// é ele.
    ///
    /// Spec 0.16, D-linha-1. Aberta pelo launcher, esta versão mora no depósito
    /// dele, e o atualizador do Tauri trocaria o app de lá pela versão do topo
    /// do manifesto — outra linha. A tela esconde o botão; esta recusa é o que
    /// vale quando alguém chega ao comando por outro caminho.
    AbertaPeloLancador,
}

/// Se esta janela pode procurar e instalar atualização.
///
/// Uma função e não um `if` em cada comando, para os dois comandos não
/// poderem discordar: procurar sem poder instalar seria oferecer o que vai ser
/// recusado.
fn atualizacao_permitida(abertura: &Abertura) -> Result<(), FalhaAoAtualizar> {
    if abertura.pelo_lancador {
        return Err(FalhaAoAtualizar::AbertaPeloLancador);
    }
    Ok(())
}

/// Traduz a falha do plugin para o que a tela tem que dizer.
///
/// Um `match` e não o `Display` do plugin: aquelas mensagens são em inglês,
/// escritas para quem desenvolve, e algumas dizem «Updater does not have any
/// endpoints set», que não é frase para ninguém ler. Escrito aqui, a próxima
/// variante do plugin cai no ramo final em vez de virar texto cru na tela.
fn classificar_atualizacao(erro: &tauri_plugin_updater::Error) -> FalhaAoAtualizar {
    use tauri_plugin_updater::Error as Falha;
    match erro {
        // Sem endpoint não há de onde buscar, e isso é configuração de
        // empacotamento, igual à chave ausente. Mesma tela.
        Falha::EmptyEndpoints => FalhaAoAtualizar::NaoConfigurado,
        // `ReleaseNotFound` saiu daqui: ele é a resposta bem-sucedida «não há
        // release publicado», e não uma falha de rede. Ver `NadaPublicado`.
        Falha::ReleaseNotFound => FalhaAoAtualizar::NadaPublicado,
        Falha::Reqwest(_) | Falha::Network(_) | Falha::Serialization(_) => {
            FalhaAoAtualizar::NaoAlcancei
        }
        Falha::TargetNotFound(_)
        | Falha::TargetsNotFound(_)
        | Falha::UnsupportedOs
        | Falha::UnsupportedArch => FalhaAoAtualizar::SemPacoteParaEsteSistema,
        Falha::Minisign(_) | Falha::Base64(_) | Falha::SignatureUtf8(_) => {
            FalhaAoAtualizar::AssinaturaRecusada
        }
        // Tudo o mais é a troca dos arquivos dando errado: descompactar, mover,
        // permissão negada, o instalador do sistema recusando. O que sobrou de
        // comum a esses casos é que o app continua sendo o de antes.
        _ => FalhaAoAtualizar::NaoInstalei,
    }
}

/// Este app foi empacotado com uma chave de atualização?
///
/// A pergunta é sobre a `pubkey` do `plugins.updater`, e ela existe porque o
/// plugin **não** a confere na hora de procurar: uma chave vazia atravessa o
/// `check()` inteira e só falha lá na frente, ao conferir o pacote já baixado.
/// Sem esta conferência, um app sem chave ofereceria a atualização, gastaria o
/// download e recusaria no fim — três minutos para chegar à resposta que se
/// tinha antes de começar.
fn tem_chave_de_atualizacao(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(|chave| chave.as_str())
        .is_some_and(|chave| !chave.trim().is_empty())
}

/// Pergunta ao release se há versão mais nova. **Não baixa nada.**
///
/// `Ok(None)` é a resposta boa e comum: já se está na última. A tela distingue
/// isso de falha porque são coisas diferentes — uma pede um «você está em dia», a
/// outra pede um motivo.
///
/// O que atravessa a rede aqui é um GET de um JSON pequeno. Enquanto ele
/// acontece o app segue inteiro: a consulta roda no runtime assíncrono, sem
/// tocar na sessão nem na janela, e um erro volta como valor.
#[tauri::command]
async fn procurar_atualizacao(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<Option<VersaoNova>, FalhaAoAtualizar> {
    use tauri_plugin_updater::UpdaterExt;

    atualizacao_permitida(&session.abertura)?;
    if !tem_chave_de_atualizacao(&app) {
        return Err(FalhaAoAtualizar::NaoConfigurado);
    }

    let atualizador = app
        .updater()
        .map_err(|erro| classificar_atualizacao(&erro))?;
    let achado = atualizador
        .check()
        .await
        .map_err(|erro| classificar_atualizacao(&erro))?;

    let Some(nova) = achado else {
        // Em dia. O que estava guardado de uma consulta anterior morre aqui: se
        // ainda estivesse lá, um «instalar» seguinte instalaria uma versão que
        // esta consulta acabou de dizer que não é a de agora.
        if let Ok(mut slot) = session.atualizacao.lock() {
            *slot = None;
        }
        return Ok(None);
    };

    let resposta = VersaoNova {
        versao: nova.version.clone(),
        instalada: nova.current_version.clone(),
        notas: nova.body.clone(),
    };

    if let Ok(mut slot) = session.atualizacao.lock() {
        *slot = Some(nova);
    }

    Ok(Some(resposta))
}

/// Baixa, confere a assinatura, instala e reabre o app.
///
/// # O que acontece se falhar no meio
///
/// A ordem é a resposta, e ela é do plugin: o pacote inteiro é baixado **para a
/// memória** e a assinatura é conferida **antes** de qualquer arquivo instalado
/// ser tocado. Então uma queda de rede, um download truncado ou um pacote
/// adulterado terminam com o app exatamente como estava — não há meia
/// instalação possível nesses caminhos. A troca em disco, que é a parte
/// destrutiva, só começa depois de o pacote estar completo e conferido, e o
/// próprio plugin guarda o app anterior num diretório temporário para devolvê-lo
/// se a troca falhar no meio.
///
/// O que a tela precisa saber: qualquer variante de [`FalhaAoAtualizar`] que
/// volte daqui deixa uma janela viva e um SEELE utilizável. Não há estado
/// «quebrado pela metade» a explicar.
///
/// # Isto fecha e reabre o app
///
/// No Windows não há escolha: o instalador do NSIS não roda com o programa
/// aberto, então o plugin o dispara e encerra este processo — o `/R` do modo
/// passivo é o que reabre. Nos outros dois o processo continua vivo depois da
/// troca, mas rodando o código antigo que já está na memória, e reabrir é a
/// única forma de a atualização valer.
///
/// Uniformizado de propósito: uma ação que às vezes fecha a janela e às vezes
/// não é uma ação que ninguém consegue avisar direito. **A tela tem que dizer
/// antes que o SEELE vai fechar e abrir de novo** — e, se houver um servidor
/// hospedado nesta janela, que quem estiver dentro dele cai junto.
#[tauri::command]
async fn instalar_atualizacao(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<(), FalhaAoAtualizar> {
    atualizacao_permitida(&session.abertura)?;
    // Tirado do lugar, e não emprestado: instalar é uma vez. Se falhar, quem
    // quiser tentar de novo procura de novo — e a consulta nova é justamente o
    // que confirma que a versão ainda é aquela.
    let nova = session
        .atualizacao
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .ok_or(FalhaAoAtualizar::NadaEscolhido)?;

    let relator = app.clone();
    let mut baixados: u64 = 0;
    let pacote = nova
        .download(
            move |pedaco, total| {
                baixados = baixados.saturating_add(pedaco as u64);
                // Uma emissão perdida é uma barra que para de andar, e não uma
                // instalação perdida. Não vale derrubar o download por causa
                // dela.
                let _ = relator.emit(CANAL_DE_ATUALIZACAO, Andamento { baixados, total });
            },
            || {},
        )
        .await
        .map_err(|erro| classificar_atualizacao(&erro))?;

    // **O bilhete que sobrevive a este processo.**
    //
    // No Windows o `install` abaixo não volta: o atualizador do Tauri entrega o
    // instalador ao shell e sai com `exit(0)` — e sai **sem olhar** se o shell
    // conseguiu abri-lo. Está no código dele:
    //
    // ```rust
    // unsafe { ShellExecuteW(null, w!("open"), file, parameters, null, SW_SHOW) };
    // std::process::exit(0);
    // ```
    //
    // O retorno é descartado. UAC negado, arquivo que não abre, elevação que
    // falha — o app fecha igual, e quem reabre está na versão de antes sem uma
    // palavra sobre por quê. Relatado assim: «depois da versão 10.1-1 toda vez
    // que atualizo e fecho o app, ao abrir de novo volta pra essa versão».
    //
    // Este arquivo é o que permite dizer a frase. Ele é escrito **antes** de o
    // processo poder morrer, guarda para que versão se estava indo, e é lido na
    // próxima abertura: se a versão de agora não é aquela, a atualização não
    // aconteceu, e a tela passa a saber disso.
    //
    // Escrever falhando não impede de instalar. O que se perde é a frase da
    // próxima vez, e trocar uma atualização por uma frase seria a troca errada.
    anotar_a_tentativa(&app, nova.version.clone());

    // Daqui para baixo é a parte que mexe em disco.
    nova.install(pacote)
        .map_err(|erro| classificar_atualizacao(&erro))?;

    // Só se chega aqui fora do Windows: lá o `install` acima já encerrou este
    // processo. `restart` não volta, e é por isso que não há `Ok(())` depois.
    app.restart()
}

/// Onde fica o bilhete da atualização em curso.
///
/// Ao lado das preferências, e não em `Program Files`: quem escreve é o app
/// rodando como a pessoa, e quem lê é a abertura seguinte da mesma pessoa.
fn bilhete_da_atualizacao(app: &AppHandle) -> std::path::PathBuf {
    std::path::PathBuf::from(config_dir(app)).join("atualizacao-em-curso")
}

/// Guarda para que versão esta atualização estava indo.
///
/// Ver o comentário em `instalar_atualizacao`: no Windows o processo pode morrer
/// na linha seguinte, e morre sem saber se o instalador chegou a abrir.
fn anotar_a_tentativa(app: &AppHandle, versao: String) {
    let caminho = bilhete_da_atualizacao(app);
    if let Some(pasta) = caminho.parent() {
        let _ = std::fs::create_dir_all(pasta);
    }
    if let Err(erro) = std::fs::write(&caminho, &versao) {
        tracing::warn!(%erro, %versao, "não consegui anotar a atualização em curso");
    }
}

/// O que a atualização anterior prometeu e não cumpriu, se houve uma.
///
/// `Some(versão)` quando a última tentativa mirava uma versão que **não** é a que
/// está rodando agora. Nos dois outros casos o bilhete é apagado e a resposta é
/// `None`: ou a atualização deu certo, ou não houve nenhuma.
///
/// Apagado ao ser lido, e é o que faz a frase aparecer **uma vez**. Uma queixa
/// que se repete a cada abertura vira ruído que se aprende a ignorar, e a
/// próxima tentativa escreve o bilhete de novo se falhar de novo.
///
/// # Errors
///
/// Nenhum. Um bilhete que não abre é indistinguível de bilhete nenhum, e nos
/// dois casos não há o que contar.
#[tauri::command]
fn atualizacao_que_nao_pegou(app: AppHandle) -> Option<String> {
    let caminho = bilhete_da_atualizacao(&app);
    let anotada = std::fs::read_to_string(&caminho).ok()?;
    let _ = std::fs::remove_file(&caminho);

    let anotada = anotada.trim().to_owned();
    if anotada.is_empty() || anotada == app.package_info().version.to_string() {
        return None;
    }
    tracing::warn!(
        alvo = %anotada,
        rodando = %app.package_info().version,
        "a atualização anterior não foi aplicada"
    );
    Some(anotada)
}

/// Abre o arquivo de log desta máquina, em modo de acréscimo.
///
/// A mesma pasta do banco e das preferências, pela mesma ordem de
/// [`config_dir`] — `SEELE_HOME`, `XDG_CONFIG_HOME`, `HOME` — menos o último
/// degrau: aquele pede o `AppHandle`, que ainda não existe quando o log é
/// armado. Sem nenhuma das três, a pasta corrente, que é onde um binário rodado
/// à mão escreve.
///
/// `None` quando a pasta não abre e o arquivo não cria. Aí o log volta para a
/// saída padrão: melhor que log nenhum, e muito melhor que programa nenhum.
fn arquivo_de_log() -> Option<std::fs::File> {
    let pasta = if let Ok(home) = std::env::var("SEELE_HOME") {
        std::path::PathBuf::from(home)
    } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        std::path::PathBuf::from(xdg).join("seele")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home).join(".config").join("seele")
    } else if let Ok(appdata) = std::env::var("APPDATA") {
        // **O Windows não define nenhuma das três de cima.** Sem esta linha o
        // rastro caía em `"."` — o diretório de onde o processo foi iniciado,
        // que num app instalado é `Program Files`, onde ninguém escreve. O
        // arquivo não nascia, o `tracing` voltava para a saída padrão, e
        // `windows_subsystem = "windows"` mata o console em release: o log não
        // existia em lugar nenhum exatamente no sistema onde ele era mais
        // preciso. Perguntado por quem usa: «onde fica o seele.log no Windows?»
        //
        // `APPDATA` mais o identificador do pacote é onde `config_dir` também
        // vai parar pelo `app_config_dir` do Tauri, e é o ponto: o rastro tem de
        // ficar ao lado das preferências, que é onde este arquivo promete que
        // ele fica. Aqui não dá para chamar o Tauri — isto roda antes de haver
        // `AppHandle` —, então o caminho é montado à mão.
        std::path::PathBuf::from(appdata).join("tech.datadev.seele")
    } else {
        std::path::PathBuf::from(".")
    };
    std::fs::create_dir_all(&pasta).ok()?;
    let arquivo = pasta.join("seele.log");
    girar_o_log(&arquivo);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(arquivo)
        .ok()
}

/// O maior tamanho que o rastro alcança antes de virar `seele.log.1`.
///
/// Oito mebibytes: cabem dias de sessão com `RUST_LOG=debug` e é pequeno o
/// bastante para alguém anexar o arquivo numa conversa de suporte sem pensar
/// duas vezes.
const TETO_DO_LOG: u64 = 8 * 1024 * 1024;

/// Um pânico, em qualquer thread, deixa uma linha no `seele.log`. F1-audio-panico.
///
/// # Por que existe
///
/// O gancho padrão escreve o pânico na saída de erro, e só. No Windows, o build
/// de release não tem console, e a saída de erro não vai a lugar nenhum; no
/// Mac, o app aberto pelo Finder também não tem quem a leia. Um pânico na thread
/// que abre o áudio, ou na da voz, matava a thread e deixava a sala muda — e o
/// `seele.log`, que é o que a pessoa manda quando relata, sem uma linha sobre
/// isso. «O produto sabe e não conta», no lugar onde mais custa.
///
/// # O que ele faz
///
/// Guarda o gancho anterior, escreve a linha pelo `tracing` — com o nome da
/// thread, o lugar e a mensagem — e chama o anterior, que continua escrevendo
/// na saída de erro como sempre. Uma vez só (`Once`): instalar duas vezes
/// encadearia o gancho nele mesmo, e cada pânico sairia duas vezes no log.
fn instalar_gancho_de_panico() {
    static UMA_VEZ: std::sync::Once = std::sync::Once::new();
    UMA_VEZ.call_once(|| {
        let anterior = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let nome = thread.name().unwrap_or("sem nome");
            let local = info
                .location()
                .map_or_else(|| "lugar desconhecido".to_owned(), ToString::to_string);
            let mensagem = info.payload_as_str().unwrap_or("sem mensagem");
            tracing::error!(
                target: "seele_app",
                thread = nome,
                local = %local,
                "pânico: {mensagem}"
            );
            anterior(info);
        }));
    });
}

/// Troca o rastro por um novo quando o atual passa do teto. R15.
///
/// # Por que ele precisa girar
///
/// Porque ele abria em `append` e mais nada: o arquivo crescia enquanto o
/// aplicativo fosse usado, para sempre. Num servidor hospedado dentro do app,
/// que fica de pé por semanas, isso é o disco de quem hospeda enchendo sem
/// ninguém pedir — R15 da revisão da v15 nomeia esta linha.
///
/// # Duas gerações, e não dez
///
/// `seele.log` e `seele.log.1`, e a mais velha é substituída. Duas porque a
/// pergunta que o rastro responde é «o que aconteceu agora» e, quando a sessão
/// acabou de reiniciar, «o que aconteceu antes de reiniciar» — a segunda é o
/// motivo de haver uma anterior. Dez gerações seriam dez vezes o teto em disco
/// para responder uma pergunta que ninguém faz.
///
/// # Na abertura, e não por relógio
///
/// Girar no meio de um processo pediria trocar o destino do `tracing` com ele
/// rodando, que é maquinaria para um ganho pequeno: o rastro passa do teto
/// depois de dias, e o aplicativo é reaberto muito mais que isso. O custo desta
/// escolha, dito: uma sessão que fique aberta tempo bastante passa do teto e só
/// gira na próxima abertura.
///
/// Toda falha é engolida de propósito — `let _` e `ok()`. Não conseguir girar não
/// pode impedir o log de abrir: log grande é um problema, log nenhum é pior, e
/// era o que uma interrogação aqui causaria.
fn girar_o_log(arquivo: &std::path::Path) {
    let Ok(dados) = std::fs::metadata(arquivo) else {
        // Não existe ainda, que é o caso da primeira abertura.
        return;
    };
    if dados.len() < TETO_DO_LOG {
        return;
    }
    let anterior = arquivo.with_extension("log.1");
    // O `rename` substitui a geração anterior no Unix e falha no Windows quando
    // o destino existe, então ele sai antes. Ver `identity::gravar_privado`, que
    // tem a mesma nota pela mesma razão.
    let _ = std::fs::remove_file(&anterior);
    let _ = std::fs::rename(arquivo, &anterior);
}

fn main() {
    // Marca de arranque. `specs/06-clientes-gui.md` aceita M5 com inicialização
    // abaixo de 2 s, e um critério que ninguém mede é um critério que passa a
    // valer o que a lembrança de alguém sobre "pareceu rápido" valer.
    let arranque = std::time::Instant::now();

    // **O log num arquivo, e não só na saída padrão.**
    //
    // No Windows, um build de release carrega `windows_subsystem = "windows"`:
    // a janela não tem console, e o que se escreve na saída padrão não vai a
    // lugar nenhum. Quem instalou o app e viu alguma coisa dar errado não tem o
    // que copiar, e quem conserta não tem o que ler.
    //
    // Isso já custou: «sem áudio» chegou três vezes de campo, e as três sem um
    // lugar onde olhar — eu cheguei a pedir um arquivo de log que não existia.
    //
    // Ao lado do banco e das preferências, que é onde o resto do estado desta
    // máquina mora. Um arquivo só, aberto em modo de acréscimo: rotação é uma
    // decisão que este log ainda não precisa, e um arquivo que some é pior que
    // um arquivo grande.
    // **Todos os crates, e não três.**
    //
    // Faltavam `seele_video`, `seele_audio` e `seele_server`, e a falta era
    // invisível: o `tracing` não reclama de um alvo que ninguém declarou, ele
    // apenas não mostra. O custo apareceu em campo — a linha que diz qual
    // codificador pegou, `a tela vai pelo codificador do sistema`, mora em
    // `seele_video` e nunca chegou ao arquivo. Pedi ao dono um log para
    // responder uma pergunta que o log estava proibido de responder.
    //
    // `tests/empacotamento.rs` prende a lista: um crate novo amanhã entra mudo
    // se ninguém o acrescentar aqui, e mudo é como este ficou.
    let filtro = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "seele_app=info,seele_ffi=info,seele_core=info,\
         seele_video=info,seele_audio=info,seele_server=info,seele_proto=info,\
         seele_encontro=info,seele_lancador=info,seele_motor=info,\
         seele_sessao=info,seele_voz=info"
            .into()
    });
    match arquivo_de_log() {
        Some(arquivo) => tracing_subscriber::fmt()
            .with_env_filter(filtro)
            .with_writer(std::sync::Arc::new(arquivo))
            // Sem cor num arquivo: as sequências de escape viram lixo entre as
            // palavras para quem abre o arquivo num editor.
            .with_ansi(false)
            .init(),
        None => tracing_subscriber::fmt().with_env_filter(filtro).init(),
    }
    // Logo depois do assinante, e não antes: o gancho escreve pelo `tracing`,
    // e um pânico antes disto não teria onde cair além da saída de erro.
    instalar_gancho_de_panico();

    // A window that cannot open is not a case with a graceful path: there is
    // nowhere left to show the reason. It goes to the log and to the exit code.
    // Qual criptografia o `reqwest` usa, dito de fora.
    //
    // O `rustls` desta árvore é compilado sem provedor embutido de propósito
    // (ver `Cargo.toml`): a alternativa punha uma segunda pilha, em C, ao lado
    // do `ring` que já está aqui. O preço é esta linha, e ela precisa vir antes
    // do primeiro cliente HTTP existir.
    //
    // `Err` significa que alguém já instalou um — o atualizador, provavelmente —
    // e nesse caso está feito, que era o objetivo.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let started = tauri::Builder::default()
        // O atualizador. Registrado sempre, inclusive num build sem chave: o
        // plugin não fala com a rede por conta própria — quem o aciona são os
        // dois comandos lá em cima, e os dois recusam antes de sair da máquina
        // se a `pubkey` do `tauri.conf.json` estiver vazia.
        //
        // A chave e os endereços vêm do arquivo de configuração e não daqui.
        // Fixá-los em código faria um build de quem clonou o repositório
        // apontar para o nosso release sem que nada no repositório dissesse
        // isso — e é justamente o que o `tauri.conf.json` diz por escrito.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // O seletor de arquivos. Registrado para que `escolher_arquivo` possa
        // chamá-lo daqui de dentro; a página **não** o alcança — não há
        // permissão de `dialog` em `capabilities/janela.json`, e é a decisão:
        // quem abre um diálogo neste app é um comando desta casca, com o título
        // escrito aqui, e não uma linha de JavaScript.
        .plugin(tauri_plugin_dialog::init())
        .manage(Session {
            // Lida aqui e não dentro do `Default`: um `Default` que lê o
            // ambiente é um `Default` que se comporta diferente em teste e em
            // produção sem nada no tipo dizendo isso.
            abertura: Abertura::da_linha_de_comando(),
            ..Session::default()
        })
        .setup(move |app| {
            // **Spec 0.16, §1: os dados da 0.15 antes de qualquer outra coisa.**
            //
            // A cópia para a linha legada vem antes da migração dos MODs logo
            // abaixo (a 0.15.2 conhece o layout antigo) e antes de a janela
            // existir. A janela é `create: false` no tauri.conf.json porque o
            // Tauri 2.11.6 cria as janelas do arquivo **antes** deste `setup`
            // (`app.rs:2521-2531`): sem isso, a página e o IPC subiriam com a
            // cópia em andamento. Ver `versoes::primeira_abertura` e o guarda
            // `a_janela_so_nasce_depois_da_primeira_abertura`.
            let primeira = versoes::primeira_abertura(&config_dir(app.handle()));
            let houve_0_15 = primeira.houve_0_15();
            app.manage(versoes::EstadoDaPrimeiraAbertura(std::sync::Mutex::new(
                Some(versoes::PrimeiraAbertura {
                    dados: primeira,
                    linha_legada: None,
                }),
            )));
            for janela in app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(app.handle(), &janela)?.build()?;
            }

            // **Os pacotes do layout antigo, levados uma vez.**
            //
            // Quem já tem MODs instalados os tem em `mods/<autor>/<nome>/`, e a
            // leitura passou a ser por conteúdo. Sem isto eles sumiriam da
            // lista, e quem usa leria como o produto ter perdido o que estava
            // instalado. Aqui e não no primeiro uso da tela: a tela pode ser
            // aberta depois de o servidor já ter subido procurando um pacote.
            mods::levar_pacotes_antigos_para_o_cache(std::path::Path::new(&config_dir(
                app.handle(),
            )));

            // **A decoração do Windows sai; a do macOS fica.**
            //
            // A comp da 0.9.0 desenha a barra da janela — marca, servidor,
            // relógio e os três controles. No Windows isso pede a janela sem
            // decoração, e os controles passam a ser nossos.
            //
            // No macOS **não**, e a decisão é de quem desenha: o
            // `titleBarStyle: Overlay` do `tauri.conf.json` já deixa a barra
            // ser nossa mantendo os três semáforos do sistema no canto
            // esquerdo, que é onde a mão vai num Mac. Tirá-los para desenhar os
            // nossos no lugar seria cobrar do produto o preço de uma escolha
            // estética. A casca sabe disso pelo comando `plataforma`, e move a
            // marca para a direita para não empurrá-los.
            //
            // O que se perde no Windows, e é o custo aceito: o encaixe pelas
            // bordas do sistema. O redimensionar continua, porque
            // `decorations(false)` não o tira — só tira a moldura desenhada.
            #[cfg(target_os = "windows")]
            {
                use tauri::Manager;
                if let Some(janela) = app.get_webview_window("main") {
                    if let Err(erro) = janela.set_decorations(false) {
                        // Não é motivo para não abrir: uma janela com a barra
                        // do sistema por cima da nossa é feia e funciona, e uma
                        // que não abre não funciona.
                        tracing::warn!(%erro, "não consegui tirar a decoração da janela");
                    }
                }
            }

            // **E no macOS o título nativo é apagado.**
            //
            // O `titleBarStyle: Overlay` deixa a barra do sistema transparente e
            // a nossa aparece por baixo dela — mas o **título** continua sendo
            // desenhado pelo sistema, em cinza, logo depois dos semáforos. É
            // exatamente onde a nossa barra escreve o nome do servidor, e as
            // duas frases saíam uma por cima da outra: numa foto de campo lia-se
            // `SCasaE`, que é `SEELE` e `Casa` no mesmo lugar.
            //
            // Vazio e não outra coisa: o nome do produto já está escrito na
            // própria barra, à direita, e o menu do sistema continua vindo do
            // pacote e não daqui. Uma janela sem título nativo é o que toda
            // aplicação de barra própria faz.
            #[cfg(target_os = "macos")]
            {
                use tauri::Manager;
                if let Some(janela) = app.get_webview_window("main") {
                    if let Err(erro) = janela.set_title("") {
                        // Pelo mesmo motivo do Windows: feio e funcionando é
                        // melhor que não abrir.
                        tracing::warn!(%erro, "não consegui apagar o título nativo da janela");
                    }
                }
            }
            // **E o Cmd+Q passa pela janela** (tarefa 21 da F7): o Sair do menu
            // padrão saía sem perguntar e sem despedir ninguém. Ver
            // `trocar_o_sair_do_mac`.
            #[cfg(target_os = "macos")]
            trocar_o_sair_do_mac(app.handle());
            // O módulo de vídeo passa a morar ao lado do banco, e não dentro do
            // pacote.
            //
            // Dentro do pacote ele não sobrevive a uma instalação: cada versão
            // nova traz um pacote novo e vazio, e quem já tinha o módulo volta a
            // ver «falta o módulo de vídeo». Foi o que aconteceu duas vezes num
            // dia, no macOS e no Windows, e não é um acidente de empacotamento —
            // é onde o arquivo estava guardado. Ao lado do banco ele é do
            // computador, e não da versão instalada.
            //
            // Só quando ninguém apontou: `SEELE_OPENH264` continua sendo a
            // palavra final de quem quer testar outro módulo.
            if std::env::var_os("SEELE_OPENH264").is_none() {
                let pasta = config_dir(app.handle());
                // SAFETY-ish: estamos antes de qualquer tarefa do app tocar o
                // ambiente — o `setup` roda uma vez, na thread principal, e a
                // busca da linha legada logo abaixo só começa depois desta
                // linha. (A janela já existe: ela é criada no começo do
                // `setup`, depois da cópia dos dados da 0.15.)
                std::env::set_var("SEELE_OPENH264", &pasta);
                tracing::info!(%pasta, "onde o módulo de vídeo é procurado");
            }
            // Passo 2 da primeira abertura: a linha legada para o depósito, em
            // segundo plano e uma vez só, e só onde houve 0.15. **Depois** do
            // `set_var` de cima: a busca fala com a rede, e o `reqwest` lê o
            // ambiente (as variáveis de proxy).
            if houve_0_15 {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let resultado = versoes::baixar_linha_legada_se_preciso(&handle).await;
                    if let Some(estado) = handle.try_state::<versoes::EstadoDaPrimeiraAbertura>() {
                        if let Ok(mut slot) = estado.0.lock() {
                            if let Some(primeira) = slot.as_mut() {
                                primeira.linha_legada = resultado;
                            }
                        }
                    }
                });
            }
            // **A sala pessoal sobe sozinha, em segundo plano** (tarefa 23 da
            // F7): a escada custa até ~4 s, e a janela não espera por ela. O
            // estado chega à casca por `sala::CANAL_DA_SALA`.
            tauri::async_runtime::spawn(sala::subir(
                app.handle().clone(),
                sala::Subida::da_producao(app.handle()),
            ));
            tracing::info!(millis = arranque.elapsed().as_millis(), "janela pronta");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            abrir_no_navegador,
            previa_de_link,
            dominios_de_gif,
            apis_de_mod_aceitas,
            aplicar_conjunto_de_mods,
            escolher_o_tema_do_servidor,
            codigo_do_mod,
            midia_do_mod,
            midia_em_bytes,
            som_do_mod,
            enviar_imagem_mod,
            ler_imagem_mod,
            escolher_para_o_mod,
            pedaco_do_escolhido,
            soltar_escolhido,
            registrar_da_janela,
            estado_da_sessao,
            mod_nativo_reservar,
            mod_nativo_ativar,
            mod_nativo_colher,
            mod_nativo_entregar,
            mod_nativo_encerrar,
            pacotes_no_cache,
            apagar_pacote_do_cache,
            conjunto_exigido_agora,
            connect,
            soltar_a_entrada,
            hospedar,
            caminho_entre_pares,
            consentir_no_caminho_entre_pares,
            versoes_instaladas,
            servidores_guardados,
            criar_servidor,
            apagar_servidor,
            abrir_versao,
            abertura,
            instalar_mod,
            aceites_de_mods,
            estou_hospedando,
            servidor_hospedado,
            onde_estou,
            estado_da_sala,
            receber_ligacoes,
            toques_agora,
            atender,
            recusar,
            encerrar_a_ligacao,
            meu_codigo,
            ler_entrada,
            cancelar_ligacao,
            contatos_da_agenda,
            dar_nome_na_agenda,
            bloquear,
            baixar_versao,
            conferir_esta_versao,
            estado_da_primeira_abertura,
            linhas_para_hospedar,
            ocupacao_do_deposito,
            remover_versao,
            catalogo_de_mods,
            instalar_mod_do_catalogo,
            disconnect,
            parar_de_hospedar,
            meu_servidor,
            snapshot,
            messages,
            membros_fora_do_ar,
            enter_voice_room,
            leave_voice_room,
            open_channel,
            send_message,
            ajustar_reducao_de_ruido,
            ajustar_sensibilidade_da_voz,
            controles_da_voz,
            abrir_teste_de_microfone,
            fechar_teste_de_microfone,
            estado_do_teste_de_microfone,
            monitorar_o_microfone,
            consentir_previa,
            esquecer_previa,
            previas_consentidas,
            reenviar_mensagem,
            descartar_mensagem,
            limite_da_mensagem,
            limites_dos_campos,
            criar_voice_room,
            criar_linha,
            renomear_voice_room,
            renomear_linha,
            renomear_server,
            regras_do_icone_do_server,
            modulo_de_video_a_baixar,
            baixar_modulo_de_video,
            escolher_icone_do_server,
            preparar_icone_do_server,
            aplicar_icone_do_server,
            escolher_minha_imagem,
            escolher_apelido,
            apelido_local,
            escolher_apelido_local,
            tirar_minha_imagem,
            imagem_da_pessoa,
            meu_retrato,
            tirar_icone_do_server,
            mods_instalados,
            mod_request,
            aceite_de_mods,
            aceitar_mods,
            esquecer_aceite_de_mods,
            icone_do_server,
            expulsar_pessoa,
            banir_pessoa,
            remover_mensagem,
            mover_pessoa,
            apagar_voice_room,
            apagar_linha,
            peso_da_linha,
            set_muted,
            set_total_isolation,
            set_talking,
            set_voice_mode,
            set_volume,
            volumes_locais,
            fontes_de_tela,
            permissao_de_tela,
            pedir_permissao_de_tela,
            compartilhar_tela,
            ajustar_limites_da_tela,
            parar_de_compartilhar,
            pedir_quadro_chave,
            assistir,
            ajustar_volume_da_tela,
            calar_a_tela,
            som_da_tela,
            exclusao_do_som_da_captura,
            dispensar_aviso,
            permissao_de_microfone,
            abrir_ajustes_do_microfone,
            abrir_ajustes_de_acessibilidade,
            ha_ajustes_de_acessibilidade,
            lembrar_aparencia_do_servidor,
            tela_cheia,
            microfones,
            plataforma,
            microfone_escolhido,
            modo_de_voz_escolhido,
            tecla_de_falar,
            escolher_tecla_de_falar,
            escolher_microfone,
            saidas,
            saida_escolhida,
            escolher_saida,
            conhecidos,
            esquecer,
            analisar_convite,
            para_entrar_pela_recusa,
            buscar,
            busca_andar,
            busca_limpar,
            procurar_atualizacao,
            instalar_atualizacao,
            atualizacao_que_nao_pegou,
            abrir_a_porta_no_firewall,
            descrever_arquivo,
            guardar_colado,
            escolher_arquivo,
            enviar_anexo,
            salvar_anexo,
            destino_do_anexo,
            prever_anexo,
            regras_de_previa,
            pasta_de_downloads,
            estado_da_porta,
            estado_do_servidor,
            definir_senha_do_server,
            criar_convite_do_server,
            ligar_portaria,
            pedidos_da_portaria,
            decidir_pedido,
            revogar_admissao,
        ])
        .build(tauri::generate_context!());

    match started {
        Ok(app) => app.run(|handle, evento| {
            // **Despedir-se antes de morrer.**
            //
            // Fechar a janela matava o processo em silêncio. Um socket UDP que
            // some não avisa ninguém — QUIC precisa de um `CONNECTION_CLOSE`
            // dito por extenso —, então quem hospedava só descobria a ausência
            // no tempo limite de ociosidade, e até lá a pessoa continuava
            // sentada na sala para todo mundo. Relato de campo: «se eu fecho o
            // app no Mac, o usuário não sai da sala para o Windows».
            //
            // `ExitRequested` e não `Exit`: o primeiro chega quando a última
            // janela fecha e ainda há tempo de o quadro sair pelo fio; o segundo
            // chega com o processo já indo embora.
            //
            // **E as hospedagens, que também têm quem esteja dentro** (tarefa 21
            // da F7). A saída é segurada, as duas se despedem dentro de
            // `TETO_DA_DESPEDIDA_DAS_HOSPEDAGENS`, ao mesmo tempo que a sessão,
            // e só então o app sai.
            //
            // **Só com `code: None`**, que é a pessoa fechando a última janela.
            // O runtime o manda do `Destroyed` dela, com a janela já fora da
            // lista (`tauri-runtime-wry` 2.11.4): a página, e a credencial que
            // só mora na memória dela, já foram. Segurar aqui não segura janela
            // nenhuma. O `exit(0)` do fim vem com `Some(0)` e passa; o
            // `restart` do atualizador não é segurado pelo Tauri.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = &evento
            {
                api.prevent_exit();
                // O `RunEvent::Exit` que vem depois desta saída não despede de
                // novo (`despedir_no_fim`).
                handle
                    .state::<Session>()
                    .despedida_feita
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                let saida = handle.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri::Manager as _;
                    despedir_das_hospedagens(
                        &saida.state::<Session>(),
                        TETO_DA_DESPEDIDA_DAS_HOSPEDAGENS,
                    )
                    .await;
                    saida.exit(0);
                });
                despedir_se(handle);
            }
            // **E quem sai sem fechar a janela** (tarefa 21 da F7): o Sair do
            // Dock, o logout, um `quit` mandado de fora. O `NSApplication` manda
            // o `terminate:`, o `tao` não implementa `applicationShouldTerminate:`,
            // e o `applicationWillTerminate:` vem direto para cá, sem janela
            // fechada e sem `ExitRequested` (`tao` 0.35.3, `app_state.rs`,
            // `AppState::exit`; `tauri-runtime-wry` 2.11.4, `Event::LoopDestroyed`).
            // Quando este fecho volta, o AppKit chama `exit()`: a despedida daqui
            // é de melhor esforço, com teto, e diz no `seele.log` o que conseguiu.
            if matches!(evento, tauri::RunEvent::Exit) {
                despedir_no_fim_do_app(handle);
            }
        }),
        Err(error) => {
            tracing::error!(%error, "the desktop shell could not start");
            std::process::exit(1);
        }
    }
}

/// Avisa os MODs e o servidor de que esta máquina está saindo, antes de o
/// processo morrer: [`despedir_da_sessao`], com o teto dos MODs.
///
/// Segura a thread principal, que é a do `ExitRequested`, por até ~700 ms: até
/// [`ESPERA_DOS_MODS_NA_SAIDA`] pelos MODs nativos e até 500 ms pela thread da
/// conexão. No caso comum, numa LAN, são milissegundos. Era `disconnect` e uma
/// espera cega de 150 ms, que não sabia se o `CONNECTION_CLOSE` tinha saído — e
/// não saía: a thread da conexão derrubava o runtime com ele na fila, e quem
/// ficou via a pessoa sair pelo prazo de ociosidade do servidor, 20 s
/// (pendência 50). Se nem o teto der — a máquina desligando, a rede já morta —,
/// o servidor cai nesse mesmo prazo, e o `seele.log` diz.
fn despedir_se(handle: &tauri::AppHandle) {
    use tauri::Manager as _;

    despedir_da_sessao(&handle.state::<Session>(), ESPERA_DOS_MODS_NA_SAIDA);
}

/// Quanto cada hospedagem tem para se despedir na saída do app.
///
/// Um segundo, repartido por [`seele_server::hospedagem::Hospedagem::despedir`]:
/// metade para o aviso, que é o mesmo meio segundo do SAIR
/// (`PRAZO_DO_AVISO`), um quarto para o fechamento sair, um quarto para a porta
/// do roteador. As duas hospedagens se despedem ao mesmo tempo, e junto com a
/// sessão ([`despedir_se`]), então a saída não soma os tetos.
const TETO_DA_DESPEDIDA_DAS_HOSPEDAGENS: std::time::Duration = std::time::Duration::from_secs(1);

/// A saída do app, para as hospedagens: o servidor guardado e a sala se
/// despedem de quem está dentro, ao mesmo tempo, cada uma dentro de `teto`.
///
/// Quem estava num servidor guardado lê `ServerShuttingDown`, e quem estava
/// numa ligação lê `ChamadaEncerrada`: quem escolhe é a hospedagem
/// (`Hospedagem::despedir`). Antes, fechar a janela só despedia a sessão, e
/// quem estava dentro de um servidor desta máquina descobria pelo tempo de
/// ociosidade, 20 s depois.
///
/// As duas saem do `Mutex` **antes** de qualquer espera, e a sessão fica sem
/// hospedagem nenhuma: o que vem depois desta função é o processo saindo.
async fn despedir_das_hospedagens(sessao: &Session, teto: std::time::Duration) {
    let (guardado, pessoal) = sessao
        .hospedagens
        .lock()
        .map(|mut hospedagens| (hospedagens.guardado.take(), hospedagens.pessoal.take()))
        .unwrap_or((None, None));
    let despedir = |hospedado: Option<Hospedado>| async move {
        if let Some(hospedado) = hospedado {
            hospedado.hospedagem.despedir(teto).await;
        }
    };
    tokio::join!(despedir(guardado), despedir(pessoal));
}

/// O teto da despedida de quem sai sem fechar a janela: **um segundo, e é
/// inegociável** (decisão da orquestradora, 10/10). A saída nunca trava; o que
/// não coube nele sai pelo prazo de ociosidade do servidor, 20 s.
const TETO_DA_DESPEDIDA_NO_FIM: std::time::Duration = std::time::Duration::from_secs(1);

/// O que cada hospedagem tem dentro do [`TETO_DA_DESPEDIDA_NO_FIM`]. Menos que
/// ele, para a espera de fora não vencer por um fio a hospedagem que ainda
/// estava dentro do próprio teto. A sessão tem os ~700 ms dela
/// ([`despedir_se`]), e as três metades correm juntas.
const TETO_DAS_HOSPEDAGENS_NO_FIM: std::time::Duration = std::time::Duration::from_millis(800);

/// O que a despedida do fim conseguiu ([`despedir_no_fim`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DespedidaNoFim {
    /// A saída comum já tinha despedido todo mundo: nada a fazer.
    JaFeita,
    /// A sessão e as hospedagens acabaram dentro do teto.
    Rodou,
    /// O teto venceu, com alguma metade ainda andando: o processo sai assim
    /// mesmo, e quem ficou pode notar a saída pelo prazo de ociosidade.
    TetoVenceu,
    /// Uma metade caiu (um pânico) antes de dizer que acabou.
    Caiu,
}

/// **A despedida de quem sai sem fechar a janela**, de melhor esforço e com
/// teto (tarefa 21 da F7).
///
/// O `RunEvent::Exit` do `terminate:` chega na thread principal, e o AppKit
/// chama `exit()` assim que ela volta: uma espera sem teto aqui é um app que
/// não fecha. As duas metades correm fora dela — a `sessao` numa thread, as
/// `hospedagens` no runtime — e ela espera as duas até `teto`, contado do
/// começo. O que passar disso é cortado pelo `exit()`.
///
/// Roda uma vez por processo: `feita` é a marca que a saída comum também põe
/// ([`Session::despedida_feita`]).
fn despedir_no_fim<S, H>(
    feita: &std::sync::atomic::AtomicBool,
    teto: std::time::Duration,
    sessao: S,
    hospedagens: H,
) -> (DespedidaNoFim, std::time::Duration)
where
    S: FnOnce() + Send + 'static,
    H: std::future::Future<Output = ()> + Send + 'static,
{
    if feita.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return (DespedidaNoFim::JaFeita, std::time::Duration::ZERO);
    }
    let comeco = std::time::Instant::now();
    let (acabou, acabadas) = std::sync::mpsc::channel::<()>();
    let acabou_a_sessao = acabou.clone();
    if let Err(erro) = std::thread::Builder::new()
        .name("despedida do fim".into())
        .spawn(move || {
            sessao();
            let _ = acabou_a_sessao.send(());
        })
    {
        // A metade da sessão não começou; a das hospedagens ainda vale.
        tracing::warn!(%erro, "saída do app: a despedida da sessão não conseguiu thread");
    }
    tauri::async_runtime::spawn(async move {
        hospedagens.await;
        let _ = acabou.send(());
    });
    let prazo = comeco + teto;
    for _ in 0..2 {
        let resta = prazo.saturating_duration_since(std::time::Instant::now());
        match acabadas.recv_timeout(resta) {
            Ok(()) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                return (DespedidaNoFim::TetoVenceu, comeco.elapsed());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return (DespedidaNoFim::Caiu, comeco.elapsed());
            }
        }
    }
    (DespedidaNoFim::Rodou, comeco.elapsed())
}

/// A linha do `seele.log` da despedida do fim: se rodou e quanto levou, ou que
/// o teto venceu. Quem lê o log de quem «saiu e continuou aparecendo na sala»
/// precisa achar esta linha, e não um silêncio.
fn dizer_a_despedida_do_fim(desfecho: DespedidaNoFim, levou: std::time::Duration) {
    let levou_ms = u64::try_from(levou.as_millis()).unwrap_or(u64::MAX);
    let teto_ms = u64::try_from(TETO_DA_DESPEDIDA_NO_FIM.as_millis()).unwrap_or(u64::MAX);
    match desfecho {
        DespedidaNoFim::JaFeita => {
            tracing::info!("saída do app: a despedida já tinha rodado ao fechar a última janela")
        }
        DespedidaNoFim::Rodou => tracing::info!(
            levou_ms,
            "saída do app sem fechar a janela (Sair do Dock, logout, um quit de fora): \
             a sessão e as hospedagens se despediram"
        ),
        DespedidaNoFim::TetoVenceu => tracing::warn!(
            levou_ms,
            teto_ms,
            "saída do app sem fechar a janela: o teto da despedida venceu, e quem ficou \
             pode notar a saída pelo prazo de ociosidade, 20 s"
        ),
        DespedidaNoFim::Caiu => tracing::warn!(
            levou_ms,
            "saída do app sem fechar a janela: uma metade da despedida caiu antes de \
             acabar, e quem ficou pode notar a saída pelo prazo de ociosidade, 20 s"
        ),
    }
}

/// A despedida do fim com as metades de verdade: [`despedir_se`] e
/// [`despedir_das_hospedagens`].
fn despedir_no_fim_do_app(handle: &tauri::AppHandle) {
    let (da_sessao, das_hospedagens) = (handle.clone(), handle.clone());
    let (desfecho, levou) = despedir_no_fim(
        &handle.state::<Session>().despedida_feita,
        TETO_DA_DESPEDIDA_NO_FIM,
        move || despedir_se(&da_sessao),
        async move {
            despedir_das_hospedagens(
                &das_hospedagens.state::<Session>(),
                TETO_DAS_HOSPEDAGENS_NO_FIM,
            )
            .await;
        },
    );
    dizer_a_despedida_do_fim(desfecho, levou);
}

/// O id do Sair nosso no menu do macOS ([`trocar_o_sair_do_mac`]).
#[cfg(target_os = "macos")]
const ID_DO_SAIR: &str = "seele-sair";

/// **O Cmd+Q do macOS passa pela janela** (tarefa 21 da F7).
///
/// O Sair do menu padrão do Tauri (`PredefinedMenuItem::quit`) manda o
/// `terminate:` ao `NSApplication`, e o Cmd+Q ia direto ao `RunEvent::Exit`
/// (ver o fecho do `app.run`): sem perguntar, com o servidor guardado no ar, e
/// sem despedir nem a sessão nem as hospedagens. O nosso fecha a janela
/// principal ([`sair_pelo_menu`]), e o Cmd+Q passa a ser o botão vermelho: a
/// página pergunta, se houver o que perguntar, e a saída segurada do
/// `ExitRequested` despede todo mundo.
///
/// **O resto do menu é o padrão, item por item**, inclusive o Editar: sem ele,
/// o Cmd+C, o Cmd+V e o Cmd+Z param nos campos da página. Por isso o menu é o
/// que o Tauri já instalou (no `build`, antes do `setup`), e só o último item
/// do submenu do app é trocado, com o mesmo texto e o mesmo atalho — e só se
/// ele for mesmo o Sair. Se outra versão do Tauri mudar o menu, o padrão fica,
/// e o `seele.log` diz. O menu antes e depois sai no `seele.log` em `debug`.
#[cfg(target_os = "macos")]
fn trocar_o_sair_do_mac(app: &AppHandle) {
    let Some(menu) = app.menu() else {
        tracing::warn!(
            "o macOS ficou sem o menu padrão, e o Sair nosso não entrou: o Cmd+Q sai sem \
             perguntar e sem se despedir"
        );
        return;
    };
    tracing::debug!(menu = %listar_o_menu(&menu), "o menu do macOS, antes do Sair nosso");
    match trocar_o_sair(app, &menu) {
        Ok(true) => {
            tracing::debug!(menu = %listar_o_menu(&menu), "o menu do macOS, com o Sair nosso");
        }
        Ok(false) => tracing::warn!(
            menu = %listar_o_menu(&menu),
            "o último item do submenu do app não é o Sair padrão, e o menu ficou como \
             estava: o Cmd+Q sai sem perguntar e sem se despedir"
        ),
        Err(erro) => tracing::warn!(
            %erro,
            "não consegui trocar o Sair do menu do macOS: o Cmd+Q sai sem perguntar e sem \
             se despedir"
        ),
    }
    app.on_menu_event(|app, evento| {
        if evento.id().as_ref() == ID_DO_SAIR {
            sair_pelo_menu(app);
        }
    });
}

/// Troca o último item do submenu do app pelo Sair nosso, com o mesmo texto e o
/// mesmo atalho, se ele for o Sair padrão; `false` quando não é.
#[cfg(target_os = "macos")]
fn trocar_o_sair(app: &AppHandle, menu: &tauri::menu::Menu<tauri::Wry>) -> tauri::Result<bool> {
    let Some(tauri::menu::MenuItemKind::Submenu(do_app)) = menu.items()?.into_iter().next() else {
        return Ok(false);
    };
    let itens = do_app.items()?;
    let Some(padrao) = itens.last().and_then(|item| item.as_predefined_menuitem()) else {
        return Ok(false);
    };
    // O texto do Sair padrão é «Quit <nome do app>» (`muda` 0.19.3,
    // `platform_impl/macos`, `new_predefined`); o nosso leva o mesmo.
    let texto = padrao.text()?;
    if !texto.starts_with("Quit") {
        return Ok(false);
    }
    let nosso = tauri::menu::MenuItem::with_id(app, ID_DO_SAIR, &texto, true, Some("CmdOrCtrl+Q"))?;
    do_app.remove_at(itens.len().saturating_sub(1))?;
    do_app.append(&nosso)?;
    Ok(true)
}

/// O menu numa linha, item por item: `Título[item | item | —]`. O id só
/// aparece nos itens que não são do sistema, que é como o Sair nosso se
/// distingue do padrão.
#[cfg(target_os = "macos")]
fn listar_o_menu(menu: &tauri::menu::Menu<tauri::Wry>) -> String {
    fn item(kind: &tauri::menu::MenuItemKind<tauri::Wry>) -> String {
        use tauri::menu::MenuItemKind;
        match kind {
            MenuItemKind::Submenu(sub) => format!(
                "{}[{}]",
                sub.text().unwrap_or_default(),
                sub.items()
                    .unwrap_or_default()
                    .iter()
                    .map(item)
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            MenuItemKind::Predefined(sistema) => {
                let texto = sistema.text().unwrap_or_default();
                if texto.is_empty() {
                    "—".to_owned()
                } else {
                    texto
                }
            }
            MenuItemKind::MenuItem(nosso) => {
                format!(
                    "{} #{}",
                    nosso.text().unwrap_or_default(),
                    nosso.id().as_ref()
                )
            }
            MenuItemKind::Check(marca) => marca.text().unwrap_or_default(),
            MenuItemKind::Icon(icone) => icone.text().unwrap_or_default(),
        }
    }
    menu.items()
        .unwrap_or_default()
        .iter()
        .map(item)
        .collect::<Vec<_>>()
        .join("  ")
}

/// **O Sair nosso: o caminho do botão vermelho.** Pede o fechar da janela
/// principal, e a página decide: com o servidor guardado no ar ela pergunta,
/// sem nada no ar ela deixa fechar. Fechada a janela, a saída segurada do
/// `ExitRequested` despede todo mundo.
///
/// Com o guardado no ar, a pergunta vai aparecer: a janela vem para a frente
/// antes, porque um Cmd+Q com ela minimizada perguntaria a ninguém, e o app
/// pareceria não querer fechar.
#[cfg(target_os = "macos")]
fn sair_pelo_menu(app: &AppHandle) {
    let Some(janela) = app.get_webview_window("main") else {
        // Sem a janela não há página para perguntar: a saída vai pelo
        // `RunEvent::Exit`, com a despedida do fim.
        tracing::info!("Sair do menu sem a janela principal: o app sai pela despedida do fim");
        app.exit(0);
        return;
    };
    let guardado_no_ar = app
        .state::<Session>()
        .hospedagens
        .lock()
        .map(|hospedagens| hospedagens.guardado.is_some())
        .unwrap_or(false);
    if guardado_no_ar {
        for (passo, feito) in [
            ("unminimize", janela.unminimize()),
            ("show", janela.show()),
            ("set_focus", janela.set_focus()),
        ] {
            if let Err(erro) = feito {
                tracing::warn!(passo, %erro, "o Sair do menu não trouxe a janela para a frente");
            }
        }
    }
    tracing::info!(
        guardado_no_ar,
        "Sair do menu: pede o fechar da janela principal"
    );
    if let Err(erro) = janela.close() {
        tracing::warn!(
            %erro,
            "o Sair do menu não conseguiu pedir o fechar da janela: o app sai pela \
             despedida do fim, sem perguntar"
        );
        app.exit(0);
    }
}

/// Quanto a saída do app espera os MODs nativos confirmarem a parada.
///
/// Duzentos milissegundos: parar é o executor sair do laço dele e dizer
/// `Parou`, e um MOD que não para nisso está preso numa volta que o teto do
/// executor ainda vai interromper. O processo não espera por ele.
const ESPERA_DOS_MODS_NA_SAIDA: std::time::Duration = std::time::Duration::from_millis(200);

/// **A saída do app, sem a janela**: os MODs param primeiro, na mesma
/// despedida, e a conexão sai por último, esperada.
///
/// 1. [`Session::revogar`], como [`desmontar_o_cliente`] faz: fecha a geração,
///    manda os MODs nativos dela parar e diz as contas dos baldes do registro,
///    que a saída do app perdia. A linha INFO diz quantos MODs **desta
///    geração** ficaram parando.
/// 2. Espera, até `teto`, que eles saiam de `encerrando` — a bomba de cada MOD
///    tira a instância dali quando o executor confirma (`assentar_fala`). Só os
///    desta geração: uma instância de uma sessão anterior que nunca confirmou
///    fica em `encerrando` para sempre, de propósito, e contá-la faria toda
///    saída esperar o teto inteiro. O cadeado da supervisão é tomado a cada
///    conferência e solto antes da pausa: é o mesmo que a bomba precisa para
///    confirmar.
/// 3. [`Connection::encerrar_e_esperar`], com 500 ms: a thread da conexão só dá
///    o sinal depois de a despedida do enlace ter saído.
///
/// Sem `AppHandle` para a prova não precisar de janela.
fn despedir_da_sessao(sessao: &Session, teto: std::time::Duration) {
    // `revogar` devolve a geração nova; a que ele fechou é a anterior.
    let fechada = sessao.revogar().saturating_sub(1);
    let parando = |sessao: &Session| {
        sessao
            .mods_nativos
            .lock()
            .map(|mods| {
                mods.encerrando
                    .values()
                    .filter(|instancia| instancia.geracao == fechada)
                    .count()
            })
            .unwrap_or(0)
    };
    tracing::info!(
        mods = parando(sessao),
        "saída do app: MODs nativos mandados parar"
    );
    let fim = std::time::Instant::now() + teto;
    let mut restantes = parando(sessao);
    while restantes > 0 && std::time::Instant::now() < fim {
        std::thread::sleep(std::time::Duration::from_millis(10));
        restantes = parando(sessao);
    }
    if restantes > 0 {
        tracing::info!(
            restantes,
            ?teto,
            "saída do app: MODs nativos sem confirmar a parada dentro do teto"
        );
    }

    let Ok(connection) = sessao.connection() else {
        return;
    };
    if !connection.encerrar_e_esperar(std::time::Duration::from_millis(500)) {
        tracing::warn!(
            "a despedida não terminou em 500 ms: quem ficou vai notar a saída pelo prazo de \
             ociosidade, 20 s"
        );
    }
}

/// A despedida de quem sai sem fechar a janela (tarefa 21 da F7): o Sair do
/// Dock, o logout, um `quit` de fora. O que se prova aqui é a forma, com
/// metades de mentira; se o runtime ainda a entrega no `RunEvent::Exit` é
/// medida no app de verdade.
#[cfg(test)]
mod a_despedida_do_fim {
    use super::{despedir_no_fim, dizer_a_despedida_do_fim, DespedidaNoFim};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// **A saída nunca trava.** Com as duas metades presas, a despedida volta
    /// no teto, e diz que ele venceu.
    #[test]
    fn a_despedida_do_fim_nunca_passa_do_teto() {
        let feita = AtomicBool::new(false);
        let teto = Duration::from_millis(300);
        let comeco = Instant::now();
        let (desfecho, levou) = despedir_no_fim(
            &feita,
            teto,
            || std::thread::sleep(Duration::from_secs(3)),
            async { tokio::time::sleep(Duration::from_secs(3)).await },
        );
        let medido = comeco.elapsed();
        assert_eq!(desfecho, DespedidaNoFim::TetoVenceu);
        assert!(
            medido < teto + Duration::from_millis(200),
            "a despedida do fim segurou a saída {medido:?}, com teto de {teto:?}: o \
             AppKit espera, e o app não fecha"
        );
        assert!(levou >= teto, "o teto venceu e a conta diz {levou:?}");
    }

    /// **As duas metades, qualquer que acabe primeiro.** A despedida só diz
    /// que rodou depois de a sessão **e** as hospedagens acabarem.
    #[test]
    fn a_despedida_do_fim_espera_as_duas_metades() {
        for (da_sessao, das_hospedagens) in [(100, 250), (250, 100)] {
            let feita = AtomicBool::new(false);
            let sessao = Arc::new(AtomicBool::new(false));
            let hospedagens = Arc::new(AtomicBool::new(false));
            let (marca_a_sessao, marca_as_hospedagens) =
                (Arc::clone(&sessao), Arc::clone(&hospedagens));
            let (desfecho, _) = despedir_no_fim(
                &feita,
                Duration::from_secs(2),
                move || {
                    std::thread::sleep(Duration::from_millis(da_sessao));
                    marca_a_sessao.store(true, Ordering::SeqCst);
                },
                async move {
                    tokio::time::sleep(Duration::from_millis(das_hospedagens)).await;
                    marca_as_hospedagens.store(true, Ordering::SeqCst);
                },
            );
            assert_eq!(desfecho, DespedidaNoFim::Rodou);
            assert!(
                sessao.load(Ordering::SeqCst) && hospedagens.load(Ordering::SeqCst),
                "a despedida do fim voltou antes de uma metade acabar (sessão em \
                 {da_sessao} ms, hospedagens em {das_hospedagens} ms)"
            );
            assert!(
                feita.load(Ordering::SeqCst),
                "a despedida rodou e não ficou marcada"
            );
        }
    }

    /// **Uma vez por processo.** Depois da saída comum, que já despediu e
    /// marcou, o `RunEvent::Exit` não despede de novo.
    #[test]
    fn a_despedida_do_fim_nao_repete_a_da_janela() {
        let feita = AtomicBool::new(true);
        let chamou = Arc::new(AtomicBool::new(false));
        let (pela_sessao, pelas_hospedagens) = (Arc::clone(&chamou), Arc::clone(&chamou));
        let (desfecho, _) = despedir_no_fim(
            &feita,
            Duration::from_secs(1),
            move || pela_sessao.store(true, Ordering::SeqCst),
            async move { pelas_hospedagens.store(true, Ordering::SeqCst) },
        );
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(desfecho, DespedidaNoFim::JaFeita);
        assert!(
            !chamou.load(Ordering::SeqCst),
            "a despedida do fim despediu de novo uma saída que já tinha se despedido"
        );
    }

    /// **Uma metade que cai não vira «rodou».**
    #[test]
    fn a_despedida_do_fim_diz_quando_uma_metade_cai() {
        let feita = AtomicBool::new(false);
        let (desfecho, _) = despedir_no_fim(
            &feita,
            Duration::from_secs(1),
            || panic!("a metade da sessão caiu (de propósito, no teste)"),
            async {},
        );
        assert_eq!(desfecho, DespedidaNoFim::Caiu);
    }

    /// **O `seele.log` diz o que aconteceu**: se rodou e quanto levou, ou que
    /// o teto venceu. Um silêncio aqui é a pergunta de quem usa, dias depois,
    /// sem dado nenhum.
    #[test]
    fn a_despedida_do_fim_diz_no_log_o_que_conseguiu() {
        for (desfecho, agulhas) in [
            (DespedidaNoFim::JaFeita, &["já tinha rodado"][..]),
            (
                DespedidaNoFim::Rodou,
                &["se despediram", "levou_ms=420"][..],
            ),
            (
                DespedidaNoFim::TetoVenceu,
                &[
                    "WARN",
                    "o teto da despedida venceu",
                    "levou_ms=420",
                    "teto_ms=1000",
                ][..],
            ),
            (DespedidaNoFim::Caiu, &["WARN", "caiu antes de acabar"][..]),
        ] {
            let ((), rastro) = crate::rastro_de_teste::capturar(|| {
                dizer_a_despedida_do_fim(desfecho, Duration::from_millis(420));
            });
            for agulha in agulhas {
                assert!(
                    rastro.contains(agulha),
                    "a linha de {desfecho:?} não diz `{agulha}`: {rastro}"
                );
            }
        }
    }
}

#[cfg(test)]
mod a_tela_le_o_que_o_rust_manda {
    use super::{pinta_o_servidor, ModNaTela};
    use seele_ffi::mods::ModInstalado;
    use std::collections::BTreeSet;

    /// As chaves que um `ModNaTela` de verdade produz, e não as que parecem.
    fn chaves_serializadas() -> BTreeSet<String> {
        let amostra = ModNaTela {
            instalado: ModInstalado {
                id: "autor/nome".to_owned(),
                version: "1.0.0".to_owned(),
                api: seele_ffi::mods::MOD_API_VERSION,
                hash: "0".repeat(64),
                client: Some("cliente/main.js".to_owned()),
                arquivos: vec!["som/toque.wav".to_owned()],
                repo: "https://exemplo".to_owned(),
                reach: vec!["dom".to_owned()],
                server: true,
                tema: true,
                refused: None,
            },
            enabled: false,
            pinta: false,
            exigencia_vale_na_rede: true,
        };
        let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(amostra) else {
            panic!("um `ModNaTela` tem de serializar para objeto");
        };
        mapa.keys().cloned().collect()
    }

    /// **O defeito que este guarda existe para impedir, e que já aconteceu.**
    ///
    /// `ModNaTela` embrulha um `ModInstalado` num campo `instalado`, e o campo
    /// tem `#[serde(flatten)]` — então o invólucro **não existe** no JSON: os
    /// campos saem no topo. A página lia `mod.instalado.id` em sete lugares.
    /// `mod.instalado` é `undefined`, e `undefined.id` lança.
    ///
    /// O que isso fazia na tela é a metade que dói: a contagem do catálogo é
    /// escrita **antes** da lista, e o erro subia até um `console.warn`. Quem
    /// usava lia «1 no catálogo» ao lado de lista nenhuma, sem uma palavra
    /// sobre o motivo — o «o produto sabe e não conta» do `CLAUDE.md`, e ainda
    /// por cima quebrando a gestão de MODs inteira junto.
    ///
    /// Nenhum teste viu porque nenhum tinha um MOD instalado: com a lista
    /// vazia o `some()` nunca chama o predicado, e o caminho quebrado não é
    /// percorrido.
    ///
    /// A regra é sobre a **forma** do defeito, e não sobre o nome `instalado`:
    /// ler `x.<meio>.<folha>` em que `<meio>` não é chave e `<folha>` é, é
    /// exatamente atravessar um invólucro que o `flatten` apagou.
    /// **O `pinta` nos dois valores** (tarefa 17f.0 da F3): o pacote ligado do
    /// escolhido pinta; o outro MOD ligado, o pacote desligado do mesmo MOD e
    /// um servidor sem escolhido, não.
    #[test]
    fn so_o_pacote_ligado_do_escolhido_pinta_o_servidor() {
        assert!(
            pinta_o_servidor(true, Some("seele/estilo"), "seele/estilo"),
            "o pacote ligado do escolhido tem de pintar"
        );
        assert!(
            !pinta_o_servidor(true, Some("seele/estilo"), "seele/noite"),
            "outro MOD ligado não pinta"
        );
        assert!(
            !pinta_o_servidor(false, Some("seele/estilo"), "seele/estilo"),
            "o outro conteúdo do mesmo MOD, desligado, não pinta"
        );
        assert!(
            !pinta_o_servidor(true, None, "seele/estilo"),
            "sem escolhido, ninguém pinta"
        );
    }

    #[test]
    fn a_pagina_nao_atravessa_um_involucro_que_o_flatten_apagou() {
        let chaves = chaves_serializadas();
        assert!(
            !chaves.contains("instalado"),
            "o `flatten` saiu do `ModNaTela`; este guarda foi escrito para quando ele estava lá"
        );

        let script = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/camada-mods.js"),
        )
        .unwrap_or_else(|erro| panic!("a camada de MODs tem de ser legível: {erro}"));

        let mut culpados = Vec::new();
        for palavra in script.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')) {
            let partes: Vec<&str> = palavra.split('.').collect();
            for janela in partes.windows(3) {
                let [inicio, meio, folha] = janela else {
                    continue;
                };
                if !chaves.contains(*meio) && chaves.contains(*folha) {
                    culpados.push(format!("{inicio}.{meio}.{folha}"));
                }
            }
        }

        assert!(
            culpados.is_empty(),
            "a página lê {culpados:?}, e o meio dessas cadeias não é campo nenhum do que o \
             Rust serializa — quase sempre um invólucro que o `flatten` apagou. As chaves \
             que existem são {chaves:?}"
        );
    }
}

#[cfg(test)]
mod a_tela_le_os_limites_que_o_rust_manda {
    use super::RegrasDoIcone;

    /// **A02 da auditoria de 17/09.** A tela de preparar mostrava, literalmente,
    /// `Até NaN KiB, undefined px de lado`.
    ///
    /// Eu li `regras.teto_em_bytes` e `regras.lado_maximo`; o Rust serializa
    /// `limite_bytes` e `lado`. A tela de CONFIGURAÇÕES já usava os certos —
    /// dois leitores do mesmo comando, e só um deles conferiu o nome.
    ///
    /// O guarda deriva as chaves de um `RegrasDoIcone` **serializado de
    /// verdade**, e não de uma lista escrita aqui: assim ele não pode divergir
    /// do Rust no dia em que o campo mudar de nome.
    ///
    /// A forma cobrada é a mesma do guarda dos MODs: ler de um objeto do Rust
    /// um nome que ele não produz não lança, não aparece no console e não
    /// reprova teste nenhum. Chega na tela como `undefined`.
    #[test]
    fn nenhuma_tela_le_um_limite_de_imagem_que_nao_existe() {
        let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(RegrasDoIcone {
            limite_bytes: 8 * 1024,
            lado: 256,
        }) else {
            panic!("`RegrasDoIcone` tem de serializar para objeto");
        };
        let chaves: std::collections::BTreeSet<&str> = mapa.keys().map(String::as_str).collect();

        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut culpados = Vec::new();
        for arquivo in ["tela-boot.js", "tela-server.js"] {
            let script = std::fs::read_to_string(raiz.join(arquivo))
                .unwrap_or_else(|erro| panic!("{arquivo} tem de ser legível: {erro}"));
            // Toda leitura de `<algo com "regras" no nome>.<campo>`.
            for (at, _) in script.match_indices("regras") {
                let Some(resto) = script.get(at..) else {
                    continue;
                };
                let Some(ponto) = resto.find('.') else {
                    continue;
                };
                // Só o acesso imediato: `regras.x`, `regrasDoIcone.x`.
                if resto[..ponto].contains(['(', ')', ' ', '\n', ';']) {
                    continue;
                }
                let campo: String = resto[ponto + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !campo.is_empty() && !chaves.contains(campo.as_str()) {
                    culpados.push(format!("{arquivo}: regras.{campo}"));
                }
            }
        }

        assert!(
            culpados.is_empty(),
            "estas telas leem um limite de imagem que o Rust não serializa, e o que \
             chega é `undefined`: {culpados:?}. As chaves que existem são {chaves:?}"
        );
    }

    /// Os tetos que cada tela lê de `limites_dos_campos`, por arquivo: os nomes
    /// passados a `passaDoTeto("…"` e os lidos direto de `tetosDosCampos())?.…`.
    fn tetos_lidos_pelas_telas() -> Vec<(String, String)> {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut lidos = Vec::new();
        let mut arquivos: Vec<_> = std::fs::read_dir(&raiz)
            .expect("ui/ tem de ser legível")
            .filter_map(Result::ok)
            .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
            .filter(|nome| nome.ends_with(".js"))
            .collect();
        arquivos.sort();
        for arquivo in arquivos {
            let script = std::fs::read_to_string(raiz.join(&arquivo))
                .unwrap_or_else(|erro| panic!("{arquivo} tem de ser legível: {erro}"));
            for pedaco in script.split("passaDoTeto(\"").skip(1) {
                if let Some(campo) = pedaco.split('"').next() {
                    lidos.push((arquivo.clone(), campo.to_owned()));
                }
            }
            for pedaco in script.split("tetosDosCampos())?.").skip(1) {
                // `?.[campo]` é o acesso por nome variável do próprio
                // `passaDoTeto`, cujos nomes são os de quem o chama, lidos acima.
                if pedaco.starts_with('[') {
                    continue;
                }
                let campo: String = pedaco
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                lidos.push((arquivo.clone(), campo));
            }
        }
        lidos
    }

    /// **E os tetos dos campos, pela mesma razão** (pendência 58, item 55; lote
    /// C55 da F5).
    ///
    /// A janela pede cada teto pelo nome, e um nome que o Rust não serializa
    /// não lança: chega `undefined`, `passaDoTeto` não confere nada, e o campo
    /// volta a mandar calado o que o núcleo recusa. As chaves vêm de um
    /// `LimitesDosCampos` serializado de verdade. E, na outra mão, todo teto
    /// que o Rust serializa tem uma tela que o lê.
    #[test]
    fn nenhuma_tela_le_um_teto_de_campo_que_nao_existe() {
        let Ok(serde_json::Value::Object(mapa)) =
            serde_json::to_value(seele_ffi::Connection::limites_dos_campos())
        else {
            panic!("`LimitesDosCampos` tem de serializar para objeto");
        };
        let chaves: std::collections::BTreeSet<&str> = mapa.keys().map(String::as_str).collect();
        let lidos = tetos_lidos_pelas_telas();
        assert!(
            !lidos.is_empty(),
            "nenhuma tela lê um teto de `limites_dos_campos`, ou a leitura deste guarda \
             quebrou: ele passaria sem olhar nada"
        );
        let culpados: Vec<_> = lidos
            .iter()
            .filter(|(_, campo)| !chaves.contains(campo.as_str()))
            .collect();
        assert!(
            culpados.is_empty(),
            "estas telas leem um teto de campo que o Rust não serializa, e o que chega é \
             `undefined`, que não confere nada: {culpados:?}. As chaves que existem são \
             {chaves:?}"
        );
        // E todo teto que o Rust manda tem quem o confira: um teto sem leitor é
        // um campo que voltou a mandar sem conferir, ou um teto que sobrou.
        let sem_leitor: Vec<_> = chaves
            .iter()
            .filter(|chave| !lidos.iter().any(|(_, campo)| campo == *chave))
            .collect();
        assert!(
            sem_leitor.is_empty(),
            "nenhuma tela confere estes tetos de `limites_dos_campos`: {sem_leitor:?}. Os \
             lidos são {lidos:?}"
        );
    }
}

/// O que sai desta janela por um link, e o que não sai.
///
/// Chamava-se `o_caminho_do_mod_chega_inteiro` enquanto guardava também os
/// testes de `por_cento_desfeito`. O ADR 0049 tirou o `mod://` do produto e
/// aqueles testes saíram com ele; o nome ficou descrevendo o que não está mais
/// aqui, e um módulo cujo nome mente é onde o próximo teste entra no lugar
/// errado.
#[cfg(test)]
mod o_que_sai_desta_janela_por_um_link {
    use super::{
        e_de_um_dominio_de_gif, endereco_de_previa, endereco_que_pode_sair,
        midia_declarada_pela_pagina, FalhaNaPrevia, DOMINIOS_DE_GIF,
    };

    /// **A lista de domínios é fechada, e o sufixo é conferido com o ponto.**
    ///
    /// O defeito que este teste existe para impedir é de uma linha: um
    /// `ends_with("tenor.com")` sem ponto deixa `tenor.com.exemplo.br` passar,
    /// e aí quem cola um link escolhe o domínio que esta janela busca — que é
    /// exatamente o que a lista fechada existe para não permitir.
    #[test]
    fn so_os_dominios_da_lista_tem_a_pagina_resolvida() {
        for bom in [
            "https://tenor.com/view/gato-dancando-gif-12345",
            "https://www.tenor.com/view/x-gif-1",
            "https://media.tenor.com/abc/dancando.gif",
            "https://GIPHY.com/gifs/algo",
            "https://i.imgur.com/abc.gif",
        ] {
            assert!(e_de_um_dominio_de_gif(bom), "recusou `{bom}`");
        }
        for hostil in [
            // O sufixo sem o ponto: o caso que a comparação ingênua deixa passar.
            "https://tenor.com.exemplo.br/colher-ip",
            "https://naotenor.com/x",
            "https://exemplo.br/tenor.com/x",
            "https://exemplo.br/?a=tenor.com",
            "https://192.168.0.1/roteador",
            "nao e um endereco",
        ] {
            assert!(!e_de_um_dominio_de_gif(hostil), "aceitou `{hostil}`");
        }
    }

    /// **O que a página declara é lido de `og:image`, em qualquer ordem.**
    ///
    /// A ordem dos atributos é de quem escreve o HTML, e um leitor que exigisse
    /// `property` antes de `content` funcionaria num dos três sites e não nos
    /// outros — a forma de defeito que só aparece em produção, num site só.
    #[test]
    fn a_midia_declarada_e_lida_venha_em_que_ordem_vier() {
        let casos = [
            r#"<meta property="og:image" content="https://media.tenor.com/a.gif">"#,
            r#"<meta content="https://media.tenor.com/a.gif" property="og:image"/>"#,
            r#"<meta   property = 'og:image'   content = 'https://media.tenor.com/a.gif' >"#,
            r#"<META PROPERTY="OG:IMAGE" CONTENT="https://media.tenor.com/a.gif">"#,
            r#"<meta name="twitter:image" content="https://media.tenor.com/a.gif">"#,
            r#"<meta property="og:image:secure_url" content="https://media.tenor.com/a.gif">"#,
        ];
        for html in casos {
            assert_eq!(
                midia_declarada_pela_pagina(html).as_deref(),
                Some("https://media.tenor.com/a.gif"),
                "não leu: {html}"
            );
        }
    }

    /// `&amp;` volta a ser `&`: um endereço de mídia carrega consulta.
    #[test]
    fn a_entidade_do_e_comercial_volta_ao_endereco() {
        let html =
            r#"<meta property="og:image" content="https://media.tenor.com/a.gif?w=1&amp;h=2">"#;
        assert_eq!(
            midia_declarada_pela_pagina(html).as_deref(),
            Some("https://media.tenor.com/a.gif?w=1&h=2")
        );
    }

    /// **Uma página sem declaração não vira imagem nenhuma.**
    ///
    /// E o que ela traz de parecido não conta: um `<img>` no corpo é desenho da
    /// página, não o que ela publicou para quem compartilha o link.
    #[test]
    fn uma_pagina_sem_declaracao_nao_da_midia() {
        for html in [
            "<html><body><img src=\"https://media.tenor.com/a.gif\"></body></html>",
            r#"<meta name="description" content="um gato dançando">"#,
            r#"<meta property="og:title" content="og:image">"#,
            r#"<meta property="og:image" content="">"#,
            r#"<meta property="og:image">"#,
            "",
        ] {
            assert_eq!(midia_declarada_pela_pagina(html), None, "leu de: {html}");
        }
    }

    /// A lista existe e é pequena: ela é uma troca de privacidade, e uma troca
    /// que cresce sem ninguém olhar deixa de ser uma troca.
    #[test]
    fn a_lista_de_dominios_e_curta_e_nomeada() {
        assert!(
            DOMINIOS_DE_GIF.len() <= 5,
            "a lista cresceu sem quem a defendesse: {DOMINIOS_DE_GIF:?}"
        );
        for dominio in DOMINIOS_DE_GIF {
            assert!(
                dominio.contains('.') && !dominio.starts_with('.'),
                "`{dominio}` não é um domínio registrável"
            );
        }
    }

    /// **A busca de uma imagem de link tem três travas, e elas têm de estar lá.**
    ///
    /// Esta busca é a única do produto que vai a um endereço que outra pessoa
    /// escolheu. O que a segura:
    ///
    ///   - **o esquema**, pela mesma lista do `abrir_no_navegador`. Sem ela,
    ///     um `file://` colado na conversa faria esta janela ler o disco;
    ///   - **o teto, enquanto chega.** Confiar no `Content-Length` é confiar
    ///     em quem está do outro lado, e juntar tudo para medir depois é
    ///     deixá-lo escolher quanta memória esta janela gasta;
    ///   - **os bytes contra o tipo alegado**, pela mesma regra do anexo. Um
    ///     endereço que promete `image/gif` e entrega HTML é recusado.
    ///
    /// Guarda de texto-fonte porque as três só se exercitam contra um servidor
    /// de verdade, e um teste que levantasse um seria um teste que sai à rede.
    ///
    /// **As travas moram em `buscar_com_teto`**, e não no comando, desde que a
    /// prévia passou a fazer duas buscas — a página e a mídia que ela declara.
    /// Isso é o ponto: uma segunda busca escrita à mão, ao lado, é uma segunda
    /// busca sem as três travas, e este teste é o que obriga as duas a
    /// passarem pela mesma porta.
    #[test]
    fn a_busca_de_imagem_de_link_confere_esquema_teto_e_bytes() {
        let sem_comentario = super::fonte_sem_comentarios();
        let busca = super::corpo_de(&sem_comentario, "async fn buscar_com_teto(");

        assert!(
            busca.contains("endereco_de_previa(&texto)?"),
            "a busca deixou de conferir o destino: um `file://` colado na \
             conversa faria esta janela ler o disco de quem lê, e um \
             `http://127.0.0.1` a faria bater num serviço da máquina dela: \
             {busca}"
        );
        assert!(
            busca.contains("PREVIEW_LIMIT") && busca.contains(".chunk()"),
            "o teto deixou de ser aplicado enquanto os bytes chegam: quem \
             responde escolhe quanta memória esta janela gasta: {busca}"
        );

        let comando = super::corpo_de(&sem_comentario, "async fn previa_de_link(");
        assert!(
            comando.matches("judge(&alegado, &bytes)").count() == 2,
            "uma das duas buscas deixou de conferir os bytes contra o tipo \
             alegado: um endereço que promete imagem e entrega outra coisa \
             passa: {comando}"
        );
        // **A segunda busca é a que a página escolhe**, e é por isso que ela
        // tem uma trava a mais: sem a conferência de domínio, uma página desta
        // lista apontaria esta janela para qualquer endereço do mundo, e a
        // lista fechada não teria fechado nada.
        assert!(
            comando.contains("if !e_de_um_dominio_de_gif(&midia)"),
            "a mídia declarada por uma página deixou de ser conferida contra a \
             lista de domínios: {comando}"
        );
        assert!(
            comando.contains("alegado.starts_with(\"text/html\")"),
            "a resolução de página deixou de exigir que a resposta fosse HTML: \
             {comando}"
        );

        // **Nenhum redirecionamento é seguido por baixo.** Antes do ADR 0053 o
        // cliente os seguia com uma política própria, e essa política conferia
        // o salto pela régua de *abrir um link* — que aceita a rede local. Agora
        // quem os segue é o laço de `buscar_com_teto`, que revalida e resolve
        // cada um; a metade que fica aqui é a proibição de o cliente fazer isso
        // sozinho, porque ele está fixado no endereço do salto anterior.
        let cliente = super::corpo_de(&sem_comentario, "fn cliente_da_previa(");
        assert!(
            cliente.contains("redirect(reqwest::redirect::Policy::none())"),
            "o cliente voltou a seguir redirecionamento por conta própria: um \
             `302` para a rede local atravessa sem passar pela validação do \
             laço, e com o endereço fixado no host errado: {cliente}"
        );
    }

    /// **O que um link de chat não pode abrir.**
    ///
    /// Cada um destes é um programa que alguém do outro lado da conversa
    /// escolheria iniciar na máquina de quem lê. A lista é de casos vistos em
    /// produtos que aprenderam isso pelo caminho difícil.
    #[test]
    fn so_http_e_https_saem_desta_janela() {
        for hostil in [
            "file:///etc/passwd",
            "file://C:/Windows/System32",
            "javascript:alert(1)",
            "data:text/html,<script>x</script>",
            "smb://servidor/compartilhado",
            "vscode://file/etc/hosts",
            "ms-msdt:/id",
            "seele://casa.exemplo",
            "//exemplo.br",
            "exemplo.br",
            "",
            "   ",
            "http://",
            "https://",
        ] {
            assert!(
                endereco_que_pode_sair(hostil).is_err(),
                "deixou sair `{hostil}`"
            );
        }
    }

    /// E o que **tem** de sair, senão o ajuste não serve para nada.
    #[test]
    fn um_endereco_comum_atravessa_inteiro() {
        for bom in [
            "https://exemplo.br",
            "http://exemplo.br/pagina?a=1&b=2#parte",
            "HTTPS://EXEMPLO.BR",
            "https://exemplo.br/caminho%20escapado",
        ] {
            assert_eq!(endereco_que_pode_sair(bom), Ok(bom), "recusou `{bom}`");
        }
        // O espaço em volta é de quem colou, e não do endereço.
        assert_eq!(
            endereco_que_pode_sair("  https://exemplo.br  "),
            Ok("https://exemplo.br")
        );
    }

    /// **O que separa um argumento de dois.** Um endereço com nova linha
    /// dentro é a forma clássica de fazer um programa ler duas coisas onde
    /// quem conferiu viu uma.
    #[test]
    fn nada_com_espaco_ou_controle_no_meio_atravessa() {
        for torto in [
            "https://exemplo.br/a b",
            "https://exemplo.br\n/etc/passwd",
            "https://exemplo.br\u{0}",
            "https://exemplo.br\ta",
        ] {
            assert!(
                endereco_que_pode_sair(torto).is_err(),
                "deixou sair `{torto:?}`"
            );
        }
    }

    /// **Uma prévia não alcança esta máquina, esta rede, nem nada reservado.**
    ///
    /// ADR 0053 e R06. A revisão da v15 verificou que a janela classificava
    /// `http://127.0.0.1:9999/private.png` como prévia automática, e que a
    /// cadeia nativa faria a busca. Conferir os bytes depois da resposta não
    /// evita a requisição — e é a requisição que é o problema.
    #[test]
    fn uma_previa_nao_alcanca_a_maquina_nem_a_rede_de_quem_le() {
        for privado in [
            // A reprodução literal do review.
            "http://127.0.0.1:9999/private.png",
            "http://localhost/x.png",
            "http://LOCALHOST./x.png",
            "http://algo.local/x.png",
            "http://nas.internal/x.png",
            // O roteador da casa, e as três faixas privadas.
            "http://192.168.0.1/x.png",
            "http://10.0.0.5/x.png",
            "http://172.16.3.9/x.png",
            // `0.0.0.0/8`, que em várias pilhas é «esta máquina».
            "http://0.0.0.0/x.png",
            // Metadados de nuvem, que é link-local.
            "http://169.254.169.254/latest/meta-data",
            // Compartilhado entre operadoras.
            "http://100.100.1.1/x.png",
            // IPv6: loopback, local único, link-local…
            "http://[::1]/x.png",
            "http://[fd00::1]/x.png",
            "http://[fe80::1]/x.png",
            // …e o mapeado, que é o que uma régua só de IPv4 deixa passar.
            "http://[::ffff:127.0.0.1]/x.png",
        ] {
            assert!(
                matches!(
                    endereco_de_previa(privado),
                    Err(FalhaNaPrevia::DestinoPrivado(_))
                ),
                "a prévia alcançaria `{privado}`"
            );
            // **E a política de abrir um link continua aceitando.** As duas são
            // políticas diferentes de propósito: `http://192.168.0.1` clicado
            // por uma pessoa é o roteador da casa dela, e recusá-lo seria o
            // produto decidindo onde ela pode navegar.
            assert!(
                endereco_que_pode_sair(privado).is_ok(),
                "abrir um link parou de aceitar `{privado}`: as duas políticas \
                 voltaram a ser uma, que é o defeito que o ADR 0053 separou"
            );
        }
    }

    /// E a internet pública continua passando, senão o guarda acima é uma porta
    /// fechada para todo mundo — que passaria no teste de cima sem fazer nada.
    #[test]
    fn uma_previa_alcanca_a_internet_publica() {
        for publico in [
            "https://exemplo.br/foto.png",
            "https://media.tenor.com/algo.gif",
            "http://8.8.8.8/x.png",
            "http://[2606:4700::1]/x.png",
        ] {
            assert!(
                endereco_de_previa(publico).is_ok(),
                "recusou `{publico}`, que é internet pública"
            );
        }
    }

    /// **Cada salto é revalidado, e o endereço resolvido é fixado.**
    ///
    /// Guarda de texto-fonte pelo mesmo motivo dos vizinhos: exercitar isto
    /// pediria um servidor que redireciona, e um teste que levantasse um seria
    /// um teste que sai à rede.
    ///
    /// As três metades que o R06 pede — «restrição de destinos privados/loopback/
    /// link-local e nova validação em cada redirecionamento e resolução»:
    #[test]
    fn cada_salto_da_previa_e_validado_resolvido_e_fixado() {
        let sem_comentario = super::fonte_sem_comentarios();
        let busca = super::corpo_de(&sem_comentario, "async fn buscar_com_teto(");

        assert!(
            busca.contains("endereco_de_previa(&texto)?")
                && busca.contains("enderecos_publicos_de("),
            "um salto deixou de ser validado e resolvido de novo: um `302` para \
             a rede local atravessa: {busca}"
        );
        assert!(
            busca.contains("reqwest::header::LOCATION"),
            "o laço deixou de seguir os saltos à mão, e quem os seguir por \
             baixo os seguirá sem passar por esta validação: {busca}"
        );

        let cliente = super::corpo_de(&sem_comentario, "fn cliente_da_previa(");
        assert!(
            cliente.contains("resolve_to_addrs(host, enderecos)"),
            "o endereço deixou de ser fixado: o nome é resolvido uma vez para \
             conferir e outra para conectar, e nada obriga as duas respostas a \
             serem a mesma: {cliente}"
        );
        assert!(
            cliente.contains("Policy::none()"),
            "o cliente voltou a seguir redirecionamento por conta própria, com \
             o endereço fixado no host errado: {cliente}"
        );
    }

    /// **Nenhuma prévia sai antes do consentimento do domínio.**
    ///
    /// A ordem importa: a pergunta vem **antes** de `buscar_com_teto`. Um
    /// consentimento conferido depois seria um consentimento pedido depois de a
    /// requisição já ter aparecido no servidor de outra pessoa.
    #[test]
    fn a_previa_pergunta_antes_de_buscar() {
        let sem_comentario = super::fonte_sem_comentarios();
        let comando = super::corpo_de(&sem_comentario, "async fn previa_de_link(");

        let pergunta = comando
            .find("previa_consentida(")
            .expect("o comando deixou de conferir o consentimento");
        let busca = comando
            .find("buscar_com_teto(")
            .expect("o comando deixou de buscar");
        assert!(
            pergunta < busca,
            "o consentimento é conferido depois da busca: a requisição já \
             apareceu no servidor de quem escreveu a mensagem quando alguém \
             for perguntado: {comando}"
        );
    }
}

/// **A supervisão de MODs nativos, sem janela nenhuma** — revisão de `8adebb8`.
///
/// O que estes testes medem é o que a diretriz pede por escrito: «produzir
/// mensagens, impedir a colheita, sair do servidor e verificar descarte
/// completo após a parada, sem participação da janela».
///
/// Sem janela é o ponto. A janela pode ter fechado, travado, ou simplesmente
/// não pedir — e nada disso pode deixar bytes retidos na memória de quem usa.
/// Por isso aqui não há `AppHandle`, não há evento e não há colheita: só a
/// tabela, os créditos e o que sobra depois.
#[cfg(test)]
mod a_supervisao_dos_mods_nativos {
    use super::{
        liberar_reserva, registrar_reserva, revogar_em, EstadoNativo, InstanciaNativa, ModsNativos,
    };
    use crate::executor::{ExecutorQuickJs, Limites};

    /// Uma instância com um executor de verdade, na geração dada.
    fn instancia(geracao: u64) -> InstanciaNativa {
        InstanciaNativa {
            id: "prova/mod".to_owned(),
            geracao,
            hash: "a".repeat(64),
            api_declarada: seele_ffi::mods::MOD_API_VERSION,
            estado: EstadoNativo::Ativa,
            executor: ExecutorQuickJs::novo(Limites::default()).expect("motor"),
            pendentes: std::collections::VecDeque::new(),
            bytes_pendentes: 0,
            avisos: std::collections::VecDeque::new(),
            avisos_perdidos: 0,
            parou: false,
            codigo: Some("/* o fonte do MOD */".to_owned()),
            avisado: false,
        }
    }

    /// Enche a fila de mensagens como a bomba encheria, reservando crédito.
    fn produzir(instancia: &mut InstanciaNativa, quantas: usize, bytes: usize) {
        for _ in 0..quantas {
            let corpo = "x".repeat(bytes);
            assert!(
                instancia.executor.fila().cabe(corpo.len()),
                "a fila recusou antes do esperado"
            );
            instancia.guardar_fala("mensagem", corpo);
        }
    }

    /// **Sair com mensagem na fila solta tudo, e sem a janela.**
    ///
    /// A revisão: «sair com uma mensagem ainda na fila deixa dados e a
    /// instância retidos mesmo depois de `Parou`, sem caminho de colheita
    /// válido» — porque a colheita recusa a geração que saiu.
    ///
    /// Provar que a fila parou de crescer não basta: **ela precisa ser
    /// liberada**.
    #[test]
    fn revogar_solta_o_que_a_janela_nao_colheu() {
        let mut mods = ModsNativos::default();
        let mut viva = instancia(7);
        produzir(&mut viva, 20, 1024);
        let (antes, bytes_antes) = viva.executor.fila().ocupacao();
        assert_eq!(antes, 20, "a produção não reservou crédito");
        assert!(bytes_antes > 0);

        let numero = mods.guardar(viva);
        // Ninguém colhe. A sessão acaba.
        mods.revogar_geracao(7);

        // A instância foi para a tabela de encerramento — ela ainda não
        // confirmou — mas **os dados já saíram**.
        let retida = mods
            .encerrando
            .get(&numero)
            .expect("a instância sumiu antes de confirmar");
        assert!(
            retida.pendentes.is_empty() && retida.bytes_pendentes == 0,
            "a revogação não soltou as mensagens não colhidas"
        );
        let (depois, bytes_depois) = retida.executor.fila().ocupacao();
        assert_eq!(
            (depois, bytes_depois),
            (0, 0),
            "os créditos ficaram reservados depois da saída"
        );

        // E quando a parada chega, ela sai da supervisão — sem a janela.
        mods.confirmar_parada(numero);
        assert!(
            mods.vivos.is_empty() && mods.encerrando.is_empty(),
            "a instância continuou supervisionada depois de parar"
        );
    }

    /// **A saída do app manda os MODs nativos parar, e diz quantos.**
    ///
    /// Fechar a janela não passava por [`super::Session::revogar`]: a
    /// despedida era `disconnect` e um `sleep`, e um MOD nativo de pé seguia
    /// rodando até o processo morrer, sem nenhuma linha no `seele.log` dizendo
    /// que houve MOD algum. Aqui ninguém confirma a parada — não há bomba
    /// ouvindo o executor —, então a instância fica onde a revogação a pôs:
    /// em `encerrando`, que é o retrato de «mandei parar».
    #[test]
    fn despedir_da_sessao_manda_os_mods_nativos_parar_e_diz_quantos() {
        let sessao = super::Session::default();
        let mut reservada = instancia(sessao.geracao());
        reservada.estado = EstadoNativo::Reservada;
        let numero = sessao
            .mods_nativos
            .lock()
            .expect("a supervisão")
            .guardar(reservada);

        let ((), rastro) = crate::rastro_de_teste::capturar(|| {
            super::despedir_da_sessao(&sessao, std::time::Duration::from_millis(20));
        });

        let mods = sessao.mods_nativos.lock().expect("a supervisão");
        assert!(
            mods.encerrando.contains_key(&numero) && !mods.vivos.contains_key(&numero),
            "a saída do app não mandou parar o MOD nativo de pé: ele continua entre os vivos, e \
             roda até o processo morrer"
        );
        assert!(
            rastro.lines().any(|linha| linha.contains("INFO")
                && linha.contains("saída do app: MODs nativos mandados parar")
                && linha.contains("mods=1")),
            "a saída do app não disse no seele.log quantos MODs nativos mandou parar: {rastro}"
        );
    }

    /// Uma instância da geração `geracao` entre as vivas, e o número dela.
    fn de_pe(sessao: &super::Session, geracao: u64) -> u64 {
        sessao
            .mods_nativos
            .lock()
            .expect("a supervisão")
            .guardar(instancia(geracao))
    }

    /// Confirma a parada de `numero` depois de `depois`, tomando o cadeado da
    /// supervisão como a bomba de um MOD toma.
    fn confirmar_depois(sessao: &super::Session, numero: u64, depois: std::time::Duration) {
        std::thread::sleep(depois);
        sessao
            .mods_nativos
            .lock()
            .expect("a supervisão")
            .confirmar_parada(numero);
    }

    /// Roda `despedir_da_sessao` com um MOD que confirma a parada 30 ms depois,
    /// noutra thread, e devolve quanto ela levou e o rastro.
    fn despedir_com_um_mod_que_confirma(
        sessao: &super::Session,
        numero: u64,
        teto: std::time::Duration,
    ) -> (std::time::Duration, String) {
        std::thread::scope(|escopo| {
            escopo.spawn(|| confirmar_depois(sessao, numero, std::time::Duration::from_millis(30)));
            let comeco = std::time::Instant::now();
            let ((), rastro) =
                crate::rastro_de_teste::capturar(|| super::despedir_da_sessao(sessao, teto));
            (comeco.elapsed(), rastro)
        })
    }

    /// **A saída do app espera os MODs sem segurar o cadeado da supervisão.**
    ///
    /// A bomba de cada MOD confirma a parada tomando esse cadeado
    /// (`assentar_fala`). Uma espera que o segurasse de uma conferência a outra
    /// deixaria a confirmação parada atrás dela até o teto, e toda saída do app
    /// pagaria o teto inteiro dizendo que os MODs não confirmaram.
    #[test]
    fn despedir_da_sessao_espera_os_mods_sem_segurar_o_cadeado() {
        let sessao = super::Session::default();
        let numero = de_pe(&sessao, sessao.geracao());
        let teto = std::time::Duration::from_secs(2);

        let (levou, rastro) = despedir_com_um_mod_que_confirma(&sessao, numero, teto);

        assert!(
            levou < teto / 2,
            "a saída do app levou {levou:?} com um MOD que confirmou a parada em 30 ms: a espera \
             segurou o cadeado da supervisão, e a confirmação ficou parada atrás dela até o teto"
        );
        assert!(
            !rastro.contains("sem confirmar a parada"),
            "a saída do app disse que um MOD não confirmou a parada, e ele confirmou em 30 ms: \
             {rastro}"
        );
    }

    /// **A saída do app conta e espera só os MODs da sessão que ela fechou.**
    ///
    /// Uma instância de uma sessão anterior que nunca confirmou a parada fica em
    /// `encerrando` para sempre, de propósito (ver [`ModsNativos::encerrando`]).
    /// Contada junto, a linha dizia MODs a mais, e a espera corria sempre até o
    /// teto por uma instância que nenhuma saída vai ver parar.
    #[test]
    fn despedir_da_sessao_conta_e_espera_so_a_geracao_que_revogou() {
        let sessao = super::Session::default();
        let antiga = {
            let mut mods = sessao.mods_nativos.lock().expect("a supervisão");
            let antiga = mods.guardar(instancia(sessao.geracao()));
            mods.encerrar(antiga);
            antiga
        };
        // A sessão da antiga acabou, e ela não confirmou.
        let _ = sessao.revogar();
        let atual = de_pe(&sessao, sessao.geracao());
        let teto = std::time::Duration::from_secs(2);

        let (levou, rastro) = despedir_com_um_mod_que_confirma(&sessao, atual, teto);

        assert!(
            sessao
                .mods_nativos
                .lock()
                .expect("a supervisão")
                .encerrando
                .contains_key(&antiga),
            "a instância antiga, que nunca confirmou, saiu da supervisão: o teste não mede mais \
             o que diz medir"
        );
        assert!(
            rastro.lines().any(
                |linha| linha.contains("saída do app: MODs nativos mandados parar")
                    && linha.split_whitespace().any(|campo| campo == "mods=1")
            ),
            "a saída do app não disse `mods=1`, o único MOD que ela mandou parar: contou também \
             a instância de uma sessão anterior que nunca confirmou: {rastro}"
        );
        assert!(
            levou < teto / 2,
            "a saída do app levou {levou:?}, com o MOD da sessão dela confirmando em 30 ms: a \
             espera correu até o teto por outra coisa — a instância de uma sessão anterior, que \
             nunca vai confirmar, ou o cadeado da supervisão preso durante a espera (o teste \
             irmão, `despedir_da_sessao_espera_os_mods_sem_segurar_o_cadeado`, separa os dois)"
        );
    }

    /// **Uma parada espontânea muda o estado nativo.**
    ///
    /// A revisão: «`confirmar_parada` escreve `parou = true` somente na tabela
    /// `encerrando`. Para a tabela `vivos`, ele verifica `i.parou` sem tê-lo
    /// alterado». Uma instância cujo motor cai sozinha continuava descrita como
    /// ativa para sempre.
    #[test]
    fn uma_parada_espontanea_tira_a_instancia_de_viva() {
        let mut mods = ModsNativos::default();
        let numero = mods.guardar(instancia(7));
        assert_eq!(mods.vivos.len(), 1);

        // O motor caiu sozinho: ninguém pediu para encerrar.
        mods.confirmar_parada(numero);

        assert!(
            mods.vivos.is_empty(),
            "a instância parada continuou entre as vivas"
        );
        assert!(
            mods.encerrando.is_empty(),
            "sem nada a colher, ela devia ter saído da supervisão"
        );
    }

    /// E se ela tinha fala a colher, continua supervisionada até esvaziar.
    #[test]
    fn uma_parada_espontanea_com_fala_continua_visivel() {
        let mut mods = ModsNativos::default();
        let mut viva = instancia(7);
        produzir(&mut viva, 3, 64);
        let numero = mods.guardar(viva);

        mods.confirmar_parada(numero);
        let retida = mods
            .encerrando
            .get(&numero)
            .expect("sumiu com fala por colher");
        assert!(retida.parou, "parou sem ser marcada");
        assert_eq!(retida.pendentes.len(), 3);
        assert_eq!(retida.estado, EstadoNativo::Encerrando);
    }

    /// **Um erro não reserva crédito, e colhê-lo não devolve crédito nenhum.**
    ///
    /// A revisão: «colher um erro sem crédito reservado pode fazer o contador
    /// atômico dar a volta; repetir erros também contorna o teto da fila de
    /// mensagens».
    #[test]
    fn um_erro_fica_fora_da_contabilidade_de_dados() {
        let mut viva = instancia(7);
        // Só erros, nenhuma mensagem — pelo mesmo caminho da bomba.
        for _ in 0..5 {
            viva.guardar_fala("falhou", "o MOD lançou".to_owned());
        }
        let (mensagens, bytes) = viva.executor.fila().ocupacao();
        assert_eq!(
            (mensagens, bytes),
            (0, 0),
            "um erro reservou crédito de dados"
        );
        // E os cinco iguais viraram um, com a contagem.
        assert_eq!(viva.avisos.len(), 1, "os avisos repetidos não agregaram");
        assert_eq!(viva.avisos.front().map(|a| a.vezes), Some(5));
    }

    /// **Erros repetidos não empurram mensagens para fora do teto.**
    ///
    /// Com a janela parada, um MOD que só falha não pode consumir a cota do que
    /// está funcionando — e não pode deixar a contabilidade num estado em que
    /// mensagens futuras sejam recusadas para sempre.
    #[test]
    fn uma_chuva_de_erros_nao_bloqueia_mensagens_futuras() {
        let mut viva = instancia(7);
        for n in 0..10_000 {
            // Alternados para a agregação não os esconder todos.
            viva.guardar_fala("falhou", format!("erro {}", n % 3));
        }
        assert!(
            viva.avisos.len() <= super::AVISOS_POR_INSTANCIA,
            "a lista de avisos passou do teto: {}",
            viva.avisos.len()
        );
        assert!(viva.avisos_perdidos > 0, "nada foi contado como perdido");

        // E a fila de dados continua intacta: uma mensagem ainda cabe.
        assert!(
            viva.executor.fila().cabe(1024),
            "uma chuva de erros bloqueou as mensagens futuras"
        );
        let (mensagens, bytes) = viva.executor.fila().ocupacao();
        assert_eq!((mensagens, bytes), (1, 1024));
    }

    /// **Uma reserva que nunca foi ativada não guarda o fonte do MOD.**
    ///
    /// A revisão: «`codigos_reservados` […] só perde uma entrada na ativação.
    /// Reservar e sair sem ativar pode conservar o fonte».
    ///
    /// Medido sobre a mesma lista que a revogação usa: se ela não nomear a
    /// reserva, o código dela fica na memória desta janela para sempre.
    #[test]
    fn uma_reserva_sem_ativacao_nao_deixa_fonte_orfao() {
        let mut mods = ModsNativos::default();
        let mut reservada = instancia(7);
        reservada.estado = EstadoNativo::Reservada;
        let numero = mods.guardar(reservada);
        assert!(
            mods.vivos
                .get(&numero)
                .and_then(|i| i.codigo.as_ref())
                .is_some(),
            "a reserva devia nascer com o fonte dentro"
        );

        // Ninguém ativa. A sessão acaba.
        mods.revogar_geracao(7);

        // **Não há «entre».** O fonte mora dentro da instância, e a revogação o
        // solta pelo mesmo ato que solta o resto. Na versão anterior ele morava
        // numa tabela ao lado, inserida **depois** de o cadeado da instância ser
        // liberado — e uma revogação no meio apagava um fonte que ainda não
        // existia, deixando o comando inseri-lo depois do descarte.
        let orfao = mods
            .vivos
            .values()
            .chain(mods.encerrando.values())
            .any(|i| i.codigo.is_some());
        assert!(!orfao, "sobrou fonte de uma reserva que nunca foi ativada");

        // E ela continua supervisionada até confirmar — encerrar não é parar.
        assert!(
            mods.encerrando.contains_key(&numero),
            "a reserva revogada saiu da supervisão sem ter confirmado"
        );
        mods.confirmar_parada(numero);
        assert!(mods.encerrando.is_empty() && mods.vivos.is_empty());
    }

    /// **A reserva que chega numa geração morta é contada** em
    /// `comandos_de_geracao_morta`, como a leitura de `codigo_do_mod` que
    /// chega atrasada.
    ///
    /// Ela era recusada sem conta: `comandos_de_geracao_morta`, o contador que
    /// diz «alguma coisa da sessão anterior ainda estava falando», não subia
    /// para a reserva atrasada — e o doc da recusa de carga dizia que subia.
    /// A que vale não é contada.
    #[test]
    fn a_reserva_recusada_por_geracao_morta_e_contada() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let mods = std::sync::Mutex::new(ModsNativos::default());
        let mortos = AtomicU64::new(0);
        let mut atrasada = instancia(7);
        atrasada.estado = EstadoNativo::Reservada;
        let recusa = registrar_reserva(&mods, &|| false, &mortos, atrasada)
            .expect_err("a reserva de uma geração morta foi aceita");
        assert_eq!(
            super::motivo_de(&recusa),
            "sessao-encerrada",
            "a reserva de uma geração morta foi recusada por outro motivo"
        );
        assert_eq!(
            mortos.load(Ordering::Relaxed),
            1,
            "a reserva recusada por geração morta não foi contada em \
             `comandos_de_geracao_morta`, e o contador diz que nada da sessão anterior falou"
        );

        let mut viva = instancia(7);
        viva.estado = EstadoNativo::Reservada;
        registrar_reserva(&mods, &|| true, &mortos, viva).expect("a reserva viva foi recusada");
        assert_eq!(
            mortos.load(Ordering::Relaxed),
            1,
            "a reserva de uma geração viva foi contada como de geração morta"
        );
    }

    /// **A ativação que chega numa geração morta também é contada e dita**,
    /// como a reserva logo acima e como os comandos que passam por
    /// [`super::Session::recusar_se_morta`].
    ///
    /// Ela era a ressalva do doc de `registrar_reserva`: recusada sem conta, e
    /// um zero em `comandos_de_geracao_morta` não queria dizer que nada da
    /// sessão anterior tinha falado. Depois de contada, ainda não dizia: os
    /// outros comandos de MOD escreviam a linha DEBUG de `confere_geracao`, e
    /// a ativação, não. A que vale não é contada nem dita, nem quando é
    /// recusada por outro motivo.
    #[test]
    fn a_ativacao_recusada_por_geracao_morta_e_contada_e_dita() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let mods = std::sync::Mutex::new(ModsNativos::default());
        let nenhum = AtomicU64::new(0);
        let mut reservada = instancia(7);
        reservada.estado = EstadoNativo::Reservada;
        let numero = registrar_reserva(&mods, &|| true, &nenhum, reservada)
            .expect("a reserva viva foi recusada");

        let mortos = AtomicU64::new(0);
        let (recusa, rastro) = crate::rastro_de_teste::capturar(|| {
            liberar_reserva(&mods, &|| false, &mortos, 7, numero)
        });
        let recusa = recusa.expect_err("a ativação de uma geração morta foi aceita");
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("comando recusado: a sessão dele já acabou"))
            .unwrap_or_else(|| {
                panic!(
                    "a ativação recusada por geração morta foi contada e não foi dita: os \
                     outros comandos de MOD deixam a linha DEBUG de `confere_geracao`, e esta \
                     some do registro — {rastro}"
                )
            });
        for pedaco in ["DEBUG", "geracao=7", &format!("instancia={numero}")] {
            assert!(
                linha.contains(pedaco),
                "a linha da ativação recusada por geração morta não traz `{pedaco}`, e quem lê \
                 o registro não acha de que sessão e de que instância ela era: {linha}"
            );
        }
        assert_eq!(
            super::motivo_de(&recusa),
            "sessao-encerrada",
            "a ativação de uma geração morta foi recusada por outro motivo"
        );
        assert_eq!(
            mortos.load(Ordering::Relaxed),
            1,
            "a ativação recusada por geração morta não foi contada em \
             `comandos_de_geracao_morta`, e o contador diz que nada da sessão anterior falou"
        );
        assert!(
            mods.lock()
                .expect("cadeado")
                .encerrando
                .contains_key(&numero),
            "a reserva de uma ativação recusada por geração morta não foi encerrada"
        );

        let (recusa, rastro) = crate::rastro_de_teste::capturar(|| {
            liberar_reserva(&mods, &|| true, &mortos, 7, numero)
        });
        let recusa = recusa.expect_err("a ativação de uma reserva já encerrada foi aceita");
        assert_eq!(
            super::motivo_de(&recusa),
            "instancia-nao-esta-reservada",
            "a ativação viva de uma reserva encerrada foi recusada por outro motivo"
        );
        assert_eq!(
            mortos.load(Ordering::Relaxed),
            1,
            "a ativação de uma geração viva foi contada como de geração morta"
        );
        assert!(
            !rastro.contains("comando recusado: a sessão dele já acabou"),
            "a ativação de uma geração viva foi dita como de uma sessão que acabou: {rastro}"
        );
    }

    /// **Um comando de MOD de uma geração morta é recusado, contado e dito**,
    /// pela mesma função em todos eles ([`super::Session::recusar_se_morta`]).
    ///
    /// Eram treze conferências com um `if` cada uma, em dez funções, e sete não
    /// contavam: `comandos_de_geracao_morta` dizia zero com a sessão anterior
    /// ainda falando. Nenhum teste daqui monta um `State`, e por isso a prova é
    /// da função; o guarda de fonte logo abaixo prende que os comandos a usam.
    #[test]
    fn o_comando_de_uma_geracao_morta_e_recusado_contado_e_dito() {
        use std::sync::atomic::Ordering;

        let sessao = super::Session::default();
        sessao.geracao.store(3, Ordering::Release);
        let (resultado, rastro) = crate::rastro_de_teste::capturar(|| sessao.recusar_se_morta(2));
        let falha = resultado.expect_err("um comando de uma geração morta foi admitido");
        assert_eq!(
            super::motivo_de(&falha),
            "sessao-encerrada",
            "um comando de uma geração morta foi recusado por outro motivo, e a janela não o \
             lê como a sessão que acabou"
        );
        assert_eq!(
            sessao.comandos_de_geracao_morta.load(Ordering::Relaxed),
            1,
            "um comando recusado por geração morta não foi contado em \
             `comandos_de_geracao_morta`, e o contador diz que nada da sessão anterior falou"
        );
        assert!(
            rastro.contains("DEBUG")
                && rastro.contains("comando recusado: a sessão dele já acabou"),
            "um comando recusado por geração morta não deixou a linha DEBUG de \
             `confere_geracao`: {rastro}"
        );

        sessao
            .recusar_se_morta(0)
            .expect_err("o zero, que é «a casca não sabe», foi admitido");
        assert_eq!(
            sessao.comandos_de_geracao_morta.load(Ordering::Relaxed),
            2,
            "o comando com geração zero foi recusado sem ser contado"
        );

        sessao
            .recusar_se_morta(3)
            .expect("um comando da geração de pé foi recusado");
        assert_eq!(
            sessao.comandos_de_geracao_morta.load(Ordering::Relaxed),
            2,
            "um comando da geração de pé foi contado como de geração morta"
        );
    }

    /// **Nenhuma conferência de geração recusa por fora de
    /// [`super::Session::recusar_se_morta`].**
    ///
    /// O arquivo inteiro, menos `impl Session`, e não só os corpos de
    /// `#[tauri::command]`: a conferência de `ler_codigo_do_mod` mora num
    /// auxiliar, e um guarda que olhasse só os comandos não a veria. Os fechos
    /// `&|| session.geracao_vale(geracao)` não são negação, e ficam: quem os
    /// recebe (`registrar_reserva`, `liberar_reserva`) conta em `mortos`.
    ///
    /// E cada função que conferia continua conferindo: tirar a conferência, em
    /// vez de trocá-la, também deixaria este guarda sem negação nenhuma. A
    /// lista é a conta inteira — as chamadas de `recusar_se_morta` fora de
    /// `impl Session` e dos testes têm de ser as dela, uma por uma —, e uma
    /// conferência nova só passa depois de entrar nela.
    #[test]
    fn nenhuma_geracao_morta_e_recusada_por_fora_de_recusar_se_morta() {
        let fonte = super::fonte_sem_comentarios();
        let (antes, resto) = fonte
            .split_once("\nimpl Session {\n")
            .expect("`impl Session` não está mais neste arquivo");
        let (_, depois) = resto.split_once("\n}\n").expect(
            "`impl Session` não fecha mais na coluna zero: sem o fim dela, o guarda leria só o \
             que vem antes, e uma conferência crua depois dela passaria calada",
        );
        let fora_da_sessao = [antes, depois].concat();
        let sem_espaco =
            |texto: &str| -> String { texto.chars().filter(|c| !c.is_whitespace()).collect() };
        let fora = sem_espaco(&fora_da_sessao);
        // Montado, e não escrito: escrito, o padrão acharia este teste.
        let chamada = format!(".{}(", "geracao_vale");
        let cruas: Vec<String> = fora
            .match_indices(&chamada)
            .filter_map(|(posicao, _)| {
                let receptor = fora
                    .get(..posicao)?
                    .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
                receptor.ends_with('!').then(|| {
                    let inicio = receptor.len().saturating_sub(48);
                    fora.get(inicio..posicao + chamada.len())
                        .unwrap_or_default()
                        .to_owned()
                })
            })
            .collect();
        assert!(
            cruas.is_empty(),
            "{} conferência(s) de geração recusam com um `if` próprio, e não por \
             `recusar_se_morta`: o comando de uma geração morta volta à janela sem ser contado \
             em `comandos_de_geracao_morta`, e o zero dele mente — {cruas:#?}",
            cruas.len()
        );

        // Montado também.
        let regra = format!(".{}(", "recusar_se_morta");
        let mut na_lista = 0;
        for (funcao, minimas) in [
            ("fn mod_nativo_colher(", 1),
            ("fn mod_nativo_entregar(", 1),
            ("fn ler_codigo_do_mod(", 1),
            ("fn reservar_no_executor(", 1),
            ("fn midia_do_mod(", 1),
            ("fn som_do_mod(", 1),
            ("fn midia_em_bytes(", 1),
            ("async fn enviar_imagem_mod(", 2),
            ("async fn ler_imagem_mod(", 2),
            ("async fn escolher_para_o_mod(", 2),
            ("fn pedaco_do_escolhido(", 1),
        ] {
            let feitas = sem_espaco(super::corpo_de(&fonte, funcao))
                .matches(&regra)
                .count();
            na_lista += feitas;
            assert!(
                feitas >= minimas,
                "`{funcao}` confere a geração {feitas} vez(es) por `recusar_se_morta`, e \
                 conferia {minimas}: um comando de uma geração morta passa a ser admitido"
            );
        }
        let na_producao = sem_espaco(&sem_itens_de_teste(&fora_da_sessao))
            .matches(&regra)
            .count();
        assert_eq!(
            na_producao, na_lista,
            "há {na_producao} chamada(s) de `recusar_se_morta` fora de `impl Session` e dos \
             testes, e as funções da lista deste guarda somam {na_lista}: uma conferência ficou \
             fora da lista, e apagá-la não reprovaria nada"
        );
    }

    /// O fonte sem os itens `#[cfg(test)]` do topo do arquivo, para que a
    /// conta de [`nenhuma_geracao_morta_e_recusada_por_fora_de_recusar_se_morta`]
    /// seja a do produto, e não a dos testes que chamam `recusar_se_morta`.
    ///
    /// Um item vai até a chave que fecha na coluna zero, ou até a linha na
    /// coluna zero que termina em `;` (a `const` do fonte). Errar o fim só
    /// pode deixar teste na conta, e a igualdade reprova: o corte falha
    /// fechado.
    fn sem_itens_de_teste(fonte: &str) -> String {
        let mut producao = Vec::new();
        let mut linhas = fonte.lines();
        while let Some(linha) = linhas.next() {
            if linha != "#[cfg(test)]" {
                producao.push(linha);
                continue;
            }
            for linha in linhas.by_ref() {
                let de_uma_linha =
                    linha.ends_with(';') && !linha.starts_with(' ') && !linha.starts_with('#');
                if de_uma_linha || linha == "}" {
                    break;
                }
            }
        }
        producao.join("\n")
    }

    /// **A corrida entre registrar uma reserva e revogar a geração.**
    ///
    /// A revisão pedia «uma prova com barreira entre registrar a instância e
    /// guardar o fonte». **Essa barreira não tem onde ser posta**: o fonte mora
    /// dentro da instância, e não há instante entre as duas inserções porque
    /// não são duas. O que resta para provar é a corrida que sobrou — a que
    /// existe de verdade — e ela se prova com threads, não com prosa.
    ///
    /// A ordem que a torna segura mora em dois lugares e só funciona nos dois:
    /// [`revogar_em`] incrementa a geração **antes** de tomar o cadeado;
    /// [`registrar_reserva`] confere a geração **depois** de tomá-lo. Os dois
    /// desfechos possíveis são bons: quem registra primeiro é varrido pela
    /// revogação, quem registra depois é recusado.
    ///
    /// **A prova precisa de disputa pelo cadeado**, e não de duas threads
    /// soltas. A janela que a inversão abre fica entre soltar o cadeado e
    /// incrementar a geração — nanossegundos — e duzentas rodadas sem disputa
    /// não caem nela nenhuma vez: foi o que a primeira versão deste guarda fez,
    /// e ela passava com a ordem invertida. Com a tabela povoada, a varredura
    /// demora, os registradores se empilham no cadeado e acordam exatamente ali
    /// dentro.
    #[test]
    fn registrar_e_revogar_ao_mesmo_tempo_nao_deixa_fonte_orfao() {
        use std::sync::atomic::{AtomicU64, Ordering};

        const POVOAR: usize = 24;
        const REGISTRADORES: usize = 4;

        for rodada in 0..30u64 {
            let mods = std::sync::Arc::new(std::sync::Mutex::new(ModsNativos::default()));
            let geracao = std::sync::Arc::new(AtomicU64::new(7));
            let mortos = std::sync::Arc::new(AtomicU64::new(0));
            {
                let mut vivos = mods.lock().expect("cadeado");
                for _ in 0..POVOAR {
                    let mut habitante = instancia(7);
                    produzir(&mut habitante, 2, 256);
                    vivos.guardar(habitante);
                }
            }
            // As instâncias nascem antes da largada: criar um motor QuickJS
            // custa, e pagar esse custo dentro da corrida a desfaria.
            let prontas: Vec<InstanciaNativa> = (0..REGISTRADORES)
                .map(|_| {
                    let mut nova = instancia(7);
                    nova.estado = EstadoNativo::Reservada;
                    nova
                })
                .collect();

            let largada = std::sync::Arc::new(std::sync::Barrier::new(REGISTRADORES + 1));
            let registradores: Vec<_> = prontas
                .into_iter()
                .map(|nova| {
                    let mods = std::sync::Arc::clone(&mods);
                    let geracao = std::sync::Arc::clone(&geracao);
                    let mortos = std::sync::Arc::clone(&mortos);
                    let largada = std::sync::Arc::clone(&largada);
                    std::thread::spawn(move || {
                        largada.wait();
                        // Um respiro para o revogador tomar o cadeado antes:
                        // é dentro da varredura dele que se quer chegar.
                        for _ in 0..64 {
                            std::hint::spin_loop();
                        }
                        registrar_reserva(
                            &mods,
                            &|| geracao.load(Ordering::Acquire) == 7,
                            &mortos,
                            nova,
                        )
                    })
                })
                .collect();
            let revogador = {
                let mods = std::sync::Arc::clone(&mods);
                let geracao = std::sync::Arc::clone(&geracao);
                let largada = std::sync::Arc::clone(&largada);
                // **A revogação de verdade**, e não uma cópia da ordem dela:
                // uma cópia provaria a cópia.
                std::thread::spawn(move || {
                    largada.wait();
                    revogar_em(&geracao, &mods)
                })
            };
            let registradas: Vec<_> = registradores
                .into_iter()
                .map(|t| t.join().expect("registrador"))
                .collect();
            revogador.join().expect("revogador");
            // Cada reserva que a revogação recusou é contada, uma vez.
            let recusadas = registradas.iter().filter(|r| r.is_err()).count() as u64;
            assert_eq!(
                mortos.load(Ordering::Relaxed),
                recusadas,
                "rodada {rodada}: as reservas recusadas pela revogação e as contadas em \
                 `comandos_de_geracao_morta` não batem"
            );

            let vivos = mods.lock().expect("cadeado");
            let orfao = vivos
                .vivos
                .values()
                .chain(vivos.encerrando.values())
                .any(|i| i.codigo.is_some());
            assert!(
                !orfao,
                "rodada {rodada}: sobrou fonte de uma geração revogada"
            );
            // Registrou antes: a revogação tem de tê-la varrido para o
            // encerramento, e não pode tê-la deixado viva. Recusou: nada
            // entrou, nem para ser varrido.
            for numero in registradas.into_iter().flatten() {
                assert!(
                    !vivos.vivos.contains_key(&numero),
                    "rodada {rodada}: a reserva {numero} ficou viva numa \
                     geração que já tinha sido revogada"
                );
            }
            assert!(
                vivos.vivos.is_empty(),
                "rodada {rodada}: sobrou instância viva de uma geração revogada"
            );
        }
    }

    /// **Enquanto alguém registra, a geração não muda.**
    ///
    /// Este é o guarda que a corrida por thread não conseguia ser. A janela que
    /// a ordem antiga deixava durava poucas instruções, e nenhuma disputa caía
    /// nela: trinta rodadas com quatro registradores, com a ordem invertida de
    /// propósito, passaram todas. Um guarda que não falha quando o defeito está
    /// lá não é um guarda.
    ///
    /// Com o incremento **dentro** do cadeado, a propriedade vira observável:
    /// segurando o cadeado — que é exatamente o que [`registrar_reserva`] faz
    /// enquanto confere e insere —, a revogação não anda. Nem a geração sobe,
    /// nem a varredura roda. Tirar o incremento de dentro do cadeado faz esta
    /// asserção falhar na hora.
    #[test]
    fn registrar_e_revogar_se_excluem_inteiros() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let mods = std::sync::Arc::new(std::sync::Mutex::new(ModsNativos::default()));
        let geracao = std::sync::Arc::new(AtomicU64::new(7));
        let (avisou, chegou) = std::sync::mpsc::channel::<()>();

        let segurando = mods.lock().expect("cadeado");

        let revogador = {
            let mods = std::sync::Arc::clone(&mods);
            let geracao = std::sync::Arc::clone(&geracao);
            std::thread::spawn(move || {
                let _ = avisou.send(());
                revogar_em(&geracao, &mods)
            })
        };
        chegou.recv().expect("o revogador não largou");

        // Espera limitada, e sem prazo fixo: a asserção não é «ainda não
        // mudou», é «não pode mudar». Sob o arranjo certo isto roda até o fim
        // sem nunca ver 8; sob o errado, a primeira volta já vê.
        for volta in 0..2_000 {
            assert_eq!(
                geracao.load(Ordering::Acquire),
                7,
                "volta {volta}: a geração morreu enquanto um registro estava \
                 em curso — a revogação voltou a andar fora do cadeado"
            );
            std::thread::yield_now();
        }

        // E o que este registro inseriu é varrido assim que o cadeado sai.
        let mut reservada = instancia(7);
        reservada.estado = EstadoNativo::Reservada;
        let numero = {
            let mut vivos = segurando;
            vivos.guardar(reservada)
        };
        assert_eq!(revogador.join().expect("revogador"), 8);

        let vivos = mods.lock().expect("cadeado");
        assert!(
            !vivos.vivos.contains_key(&numero),
            "a reserva inserida sob o cadeado escapou da varredura"
        );
        assert!(
            vivos
                .vivos
                .values()
                .chain(vivos.encerrando.values())
                .all(|i| i.codigo.is_none()),
            "sobrou fonte de uma geração revogada"
        );
    }

    /// Uma instância montada em volta de um executor já escutado.
    ///
    /// A da corrida nasce com o executor dentro e o canal ainda fechado; aqui o
    /// canal precisa ficar na mão de quem testa, porque é ele que faz o papel
    /// da bomba lenta.
    fn com_canal(
        geracao: u64,
    ) -> (
        InstanciaNativa,
        std::sync::mpsc::Receiver<crate::executor::ParaOFora>,
        std::sync::Arc<crate::executor::Fila>,
    ) {
        let mut nova = instancia(geracao);
        let (recebedor, fila) = nova.executor.escutar().expect("canal");
        (nova, recebedor, fila)
    }

    /// Espera uma condição, com teto, **afirmando a condição e não o tempo**.
    fn ate(prazo: std::time::Duration, o_que: &str, mut condicao: impl FnMut() -> bool) {
        let limite = std::time::Instant::now() + prazo;
        while std::time::Instant::now() < limite {
            if condicao() {
                return;
            }
            std::thread::yield_now();
        }
        panic!("passou o prazo sem {o_que}");
    }

    /// **Um produtor de erros de verdade, com a bomba e a janela paradas.**
    ///
    /// A revisão: «testar produtor de erros real com bomba/consumidor lentos e
    /// entrega à janela suspensa, contando todos os pontos de retenção e
    /// notificações». Aqui o MOD lança a cada resposta entregue — não é um
    /// `ParaOFora::Falhou` fabricado pelo teste, é o motor traduzindo uma
    /// exceção —, ninguém escoa o canal, e ninguém colhe.
    ///
    /// São **três** pontos de retenção, e a cota é uma só atravessando os três:
    /// o que o motor recusou antes de mandar, o que está no canal esperando a
    /// bomba, e o que está na instância esperando a janela.
    #[test]
    fn um_produtor_de_erros_real_para_nas_tres_cotas() {
        use crate::executor::AVISOS_NA_FILA;

        let (viva, recebedor, fila) = com_canal(7);
        viva.executor
            .iniciar(
                "globalThis.aoResponder = () => { throw new Error('sempre'); };\
                 seele.postar('pronto');",
            )
            .expect("código");

        // O MOD lança em toda resposta. Entregar bem mais que a cota é o que
        // faz a recusa acontecer no motor, antes de ocupar o canal.
        ate(
            std::time::Duration::from_secs(5),
            "o MOD ficar de pé",
            || viva.executor.entregar("{\"v\":1}").is_ok(),
        );
        for _ in 0..(AVISOS_NA_FILA * 8) {
            // Recusa de entrada não é o que se mede aqui: o teto de entrada é
            // outro, e ele se solta sozinho quando o motor consome.
            let _ = viva.executor.entregar("{\"v\":1}");
            std::thread::yield_now();
        }

        // **Primeiro ponto: o motor recusa.** Sem cota lá, um MOD que lança num
        // laço enche o canal entre o motor e a bomba, que nenhum teto alcança.
        ate(
            std::time::Duration::from_secs(10),
            "o motor recusar um aviso",
            || fila.avisos_recusados() > 0,
        );
        assert_eq!(
            fila.avisos_de_pe(),
            AVISOS_NA_FILA,
            "a cota de avisos parou num número que não é o teto"
        );

        // **Segundo ponto: o canal.** A bomba é lenta — é este laço — e cada
        // fala que ela assenta carrega a mesma reserva para a instância.
        let mods = std::sync::Mutex::new(ModsNativos::default());
        let numero = mods.lock().expect("cadeado").guardar(viva);
        let mut notificacoes = 0usize;
        while let Ok(fala) = recebedor.recv_timeout(std::time::Duration::from_millis(50)) {
            if let super::Assentada::Guardada { avisar: true } | super::Assentada::Fim { .. } =
                super::assentar_fala(&mods, &|| true, &fila, numero, fala, "prova/mod")
            {
                notificacoes += 1;
            }
        }

        // **A notificação é agregada.** A janela suspensa recebe **um** aviso
        // sem carga, e não um por fala: a versão anterior emitia um evento por
        // fala, inclusive pelas que ela mesma agregava ou descartava.
        assert_eq!(
            notificacoes, 1,
            "a janela suspensa recebeu mais de um aviso para a mesma colheita"
        );

        // **Terceiro ponto: a instância.** Um MOD que lança num laço manda
        // sempre o mesmo texto, e é por isso que a agregação é o que contém
        // este caso: as repetições viram contagem, e cada uma **devolve** o
        // lugar na cota, porque nenhuma entrada nova foi ocupada. A perda do
        // mais velho é o outro caminho — avisos diferentes entre si —, e quem
        // o cobre é `uma_chuva_de_erros_nao_bloqueia_mensagens_futuras`.
        let mut vivos = mods.lock().expect("cadeado");
        let dentro = vivos.achar_mut(numero).expect("instância");
        assert_eq!(
            dentro.avisos.len(),
            1,
            "o mesmo erro repetido ocupou mais de uma entrada"
        );
        let repetido = dentro.avisos.front().expect("o aviso agregado");
        assert!(
            repetido.vezes > 1,
            "as repetições não foram agregadas: vezes = {}",
            repetido.vezes
        );
        assert!(
            dentro.avisos.len() <= super::AVISOS_POR_INSTANCIA,
            "a lista de avisos da instância passou do teto dela"
        );
        assert_eq!(
            fila.avisos_de_pe(),
            1,
            "a cota de avisos não acompanhou a agregação: o que está retido é \
             uma entrada, e é uma que tem de estar reservada"
        );

        // E é a colheita que solta o que sobrou, e que rearma o aviso.
        let colhidas = dentro.colher_falas(64);
        assert!(!colhidas.is_empty());
        assert_eq!(
            fila.avisos_de_pe(),
            0,
            "a colheita não devolveu a cota de avisos"
        );
        assert!(
            dentro.precisa_avisar(),
            "a janela drenou e o aviso não rearmou"
        );
    }

    /// **Um erro tardio no meio de mensagens ainda contabilizadas.**
    ///
    /// A revisão: «provar um erro tardio intercalado com mensagens ainda
    /// contabilizadas, em vez de testar apenas contadores já zerados». Um teste
    /// que drena tudo antes de conferir não distingue a contabilidade certa da
    /// que devolve crédito da classe errada: com os dois contadores em zero,
    /// tirar da cota de mensagens ou da de avisos dá no mesmo.
    ///
    /// Aqui o MOD fala, e **só então** lança. As mensagens continuam retidas
    /// enquanto o erro chega, e cada classe tem de voltar pela cota dela.
    #[test]
    fn um_erro_tardio_nao_come_o_credito_das_mensagens() {
        let (viva, recebedor, fila) = com_canal(7);
        viva.executor
            .iniciar(
                "globalThis.aoResponder = (j) => {\
                   if (JSON.parse(j).falhar) { throw new Error('tarde'); }\
                   seele.postar('x'.repeat(512));\
                 };\
                 seele.postar('pronto');",
            )
            .expect("código");
        ate(
            std::time::Duration::from_secs(5),
            "o MOD ficar de pé",
            || viva.executor.entregar("{\"falhar\":false}").is_ok(),
        );
        for _ in 0..4 {
            viva.executor
                .entregar("{\"falhar\":false}")
                .expect("entrega");
        }
        // O erro vem **depois**, com as mensagens ainda na fila e ninguém
        // colhendo.
        ate(
            std::time::Duration::from_secs(5),
            "as mensagens saírem do motor",
            || fila.ocupacao().0 >= 6,
        );
        viva.executor
            .entregar("{\"falhar\":true}")
            .expect("entrega do erro");
        ate(
            std::time::Duration::from_secs(5),
            "o erro sair do motor",
            || fila.avisos_de_pe() == 1,
        );

        let (mensagens_antes, bytes_antes) = fila.ocupacao();
        assert!(
            mensagens_antes >= 6,
            "as mensagens foram consumidas antes do erro chegar, e o teste \
             voltaria a medir contadores zerados"
        );

        let mods = std::sync::Mutex::new(ModsNativos::default());
        let numero = mods.lock().expect("cadeado").guardar(viva);
        while let Ok(fala) = recebedor.recv_timeout(std::time::Duration::from_millis(50)) {
            super::assentar_fala(&mods, &|| true, &fila, numero, fala, "prova/mod");
        }

        let mut vivos = mods.lock().expect("cadeado");
        let dentro = vivos.achar_mut(numero).expect("instância");
        assert_eq!(dentro.avisos.len(), 1, "o erro tardio não chegou inteiro");
        assert!(
            dentro.pendentes.len() >= 6,
            "as mensagens sumiram na passagem do erro"
        );
        // **Nada foi solto na travessia.** Esta é a asserção que os contadores
        // zerados não conseguiam fazer.
        assert_eq!(
            (fila.ocupacao(), fila.avisos_de_pe()),
            ((mensagens_antes, bytes_antes), 1),
            "assentar as falas mexeu na cota antes de a janela colher"
        );

        dentro.colher_falas(64);
        assert_eq!(
            (fila.ocupacao(), fila.avisos_de_pe()),
            ((0, 0), 0),
            "a colheita não devolveu cada classe pela cota dela"
        );
    }

    /// **A imagem com um uso chega reduzida a ele, e diz quanto pesa
    /// decodificada** (`reducao`; tarefa 6 da F3, Step 5), pelos dois caminhos
    /// da mídia do servidor: o texto em base64 e a leitura por volume. Sem uso,
    /// ela segue como veio.
    #[test]
    fn a_imagem_com_uso_chega_reduzida_e_diz_quanto_pesa_decodificada() {
        use image::ImageEncoder as _;
        let foto = image::RgbaImage::from_pixel(2000, 700, image::Rgba([40, 200, 120, 255]));
        let mut grande = Vec::new();
        image::codecs::png::PngEncoder::new(&mut grande)
            .write_image(&foto, 2000, 700, image::ExtendedColorType::Rgba8)
            .expect("gravar a foto de teste");
        let base64 = seele_ffi::base64_de(&grande);
        let teto = crate::reducao::TETO_DA_CAPA;
        // 2000 × 700 cabe em 960 × 320 como 914 × 320.
        let esperado = Some(914 * 320 * 4);

        let sem_uso = super::ler_midia_do_servidor(&base64, None).expect("imagem aceita");
        assert_eq!((sem_uso.bytes, sem_uso.decodificados), (grande.len(), None));

        let com_uso = super::ler_midia_do_servidor(&base64, Some(&teto)).expect("imagem reduzida");
        assert!(
            com_uso.bytes < grande.len(),
            "a imagem não encolheu: {} bytes",
            com_uso.bytes
        );
        assert_eq!(com_uso.decodificados, esperado);

        let lida = super::imagem_lida_dita(
            &super::BaldesDoRegistro::default(),
            std::time::Instant::now(),
            "seele/perfis",
            Ok(grande.clone()),
            Some(&teto),
        )
        .expect("imagem reduzida pelo volume");
        assert_eq!(lida.decodificados, esperado);
    }

    #[test]
    fn imagem_do_perfis_em_data_uri_chega_ao_decodificador() {
        for (mime, bytes) in [
            ("image/png", b"\x89PNG\r\n\x1a\n".as_slice()),
            ("image/jpeg", b"\xff\xd8\xff\xe0".as_slice()),
            ("image/webp", b"RIFF\0\0\0\0WEBP".as_slice()),
        ] {
            let base64 = seele_ffi::base64_de(bytes);
            for entrada in [base64.clone(), format!("data:{mime};base64,{base64}")] {
                let midia = super::ler_midia_do_servidor(&entrada, None).expect("imagem aceita");
                assert_eq!(midia.bytes, bytes.len());
                assert_eq!(
                    midia.uri,
                    Some(format!("data:{mime};base64,{base64}")),
                    "a mídia do servidor saiu sem o `data:` que os bytes provaram, e a janela \
                     fica sem o que mostrar"
                );
            }
        }
    }

    #[test]
    fn midia_do_servidor_confere_bytes_e_limite_mesmo_com_cabecalho() {
        let png = b"\x89PNG\r\n\x1a\n";
        let alegada = format!("data:image/jpeg;base64,{}", seele_ffi::base64_de(png));
        assert!(
            super::ler_midia_do_servidor(&alegada, None)
                .unwrap()
                .uri
                .is_some_and(|uri| uri.starts_with("data:image/png;")),
            "o tipo da mídia do servidor saiu do cabeçalho que o terceiro escreveu, e não dos \
             bytes"
        );
        for invalida in [
            "data:image/png,abc",
            "data:image/png;base64",
            "data:image/png;base64,%%%",
            "https://example.com/a.png",
        ] {
            assert!(super::ler_midia_do_servidor(invalida, None).is_err());
        }
        let mut bytes = vec![0; seele_ffi::mods::TETO_DE_MIDIA];
        // `get_mut` e não um fatiamento: `indexing_slicing` é `warn` no
        // workspace e o CI roda `-D warnings`. O `expect` mantém o teste
        // honesto — se o teto de mídia um dia não couber uma assinatura de
        // PNG, isto tem de gritar, não passar de lado.
        bytes
            .get_mut(..png.len())
            .expect("o teto de mídia cabe uma assinatura de PNG")
            .copy_from_slice(png);
        let uri = format!("data:image/png;base64,{}", seele_ffi::base64_de(&bytes));
        assert_eq!(
            super::ler_midia_do_servidor(&uri, None).unwrap().bytes,
            bytes.len()
        );
        bytes.push(0);
        let uri = format!("data:image/png;base64,{}", seele_ffi::base64_de(&bytes));
        assert!(super::ler_midia_do_servidor(&uri, None).is_err());
    }

    /// **Um arquivo escolhido não sobrevive à sessão que o escolheu.**
    ///
    /// É a mesma promessa que a sonda E1 mediu e encontrou falsa no Worker de
    /// `blob:` — o que um MOD guardou lá reapareceu depois de o aplicativo ser
    /// reiniciado. Aqui os bytes são do produto, e a saída os solta.
    ///
    /// Dez megabytes por arquivo, quatro arquivos: uma sessão que sai sem
    /// soltar deixa quarenta megabytes presos por um MOD que já não roda.
    #[test]
    fn a_saida_solta_os_arquivos_escolhidos_daquela_geracao() {
        let mut guardados = super::ArquivosEscolhidos::default();
        let de = |geracao: u64| super::ArquivoEscolhidoDeMod {
            de_quem: "prova/mod".to_owned(),
            geracao,
            tipo: "image/png",
            bytes: vec![0; 16],
        };
        assert!(guardados.guardar(de(7)).is_some());
        assert!(guardados.guardar(de(7)).is_some());
        assert!(guardados.guardar(de(8)).is_some());

        assert_eq!(guardados.soltar_geracao(7), 2);
        // E o da geração seguinte fica: soltar o que não é dela seria derrubar
        // a sessão que acabou de começar.
        assert_eq!(guardados.guardados.len(), 1);
        assert!(guardados.guardados.values().all(|a| a.geracao == 8));
    }

    /// **Dois pedaços seguidos, concatenados, são o arquivo.**
    ///
    /// O base64 codifica três bytes em quatro caracteres. Uma janela que não
    /// fecha um grupo de três sai com `=` no meio, e o que o MOD junta deixa de
    /// ser o arquivo — uma imagem que chega corrompida sem nada acusar, porque
    /// cada pedaço, sozinho, é base64 válido.
    ///
    /// `65536` não é múltiplo de três. `65535` é.
    #[test]
    fn os_pedacos_de_um_arquivo_escolhido_se_juntam_de_volta() {
        assert_eq!(
            super::PEDACO_DO_ESCOLHIDO % 3,
            0,
            "o pedaço deixou de fechar grupos de três, e dois seguidos não se juntam"
        );
        // Dois pedaços e meio, para o último ser o único com enchimento.
        let bytes: Vec<u8> = (0..super::PEDACO_DO_ESCOLHIDO * 2 + 7)
            .map(|i| u8::try_from(i % 251).unwrap_or(0))
            .collect();
        let inteiro = seele_ffi::base64_de(&bytes);
        let mut juntado = String::new();
        let mut inicio = 0;
        while inicio < bytes.len() {
            let fim = inicio
                .saturating_add(super::PEDACO_DO_ESCOLHIDO)
                .min(bytes.len());
            juntado.push_str(&seele_ffi::base64_de(
                bytes.get(inicio..fim).expect("a fatia"),
            ));
            inicio = fim;
        }
        assert_eq!(juntado, inteiro, "os pedaços não se juntam no arquivo");
        // E só o último tem enchimento: um `=` no meio é o defeito com nome.
        assert!(
            !juntado[..juntado.len() - 4].contains('='),
            "sobrou enchimento no meio do que foi juntado"
        );
    }

    /// **E há teto: um MOD que abre o seletor num laço não enche a memória.**
    #[test]
    fn os_arquivos_de_pe_tem_teto_e_o_numero_nunca_se_repete() {
        let mut guardados = super::ArquivosEscolhidos::default();
        let de = || super::ArquivoEscolhidoDeMod {
            de_quem: "prova/mod".to_owned(),
            geracao: 1,
            tipo: "image/png",
            bytes: Vec::new(),
        };
        let numeros: Vec<u64> = (0..super::ARQUIVOS_DE_PE)
            .map(|_| guardados.guardar(de()).expect("cabe"))
            .collect();
        assert!(
            guardados.guardar(de()).is_none(),
            "o teto de arquivos de pé não segurou"
        );

        // **Monotônico.** Soltar um e guardar outro não reaproveita o número:
        // um pedido atrasado com o número velho leria o arquivo que ocupou o
        // lugar dele, e seria o arquivo errado sem nada dizer.
        guardados
            .guardados
            .remove(numeros.first().expect("o primeiro guardado"));
        let novo = guardados.guardar(de()).expect("cabe depois de soltar");
        assert!(
            !numeros.contains(&novo),
            "um número de arquivo foi reaproveitado: {novo} já tinha sido usado"
        );
    }

    /// **Uma montagem que não completa desfaz tudo.**
    ///
    /// Sem o desfazer, a instância ficava na tabela com o fonte dentro e sem
    /// ninguém para escutá-la — e só uma ativação futura, que pode nunca vir, a
    /// tiraria de lá.
    #[test]
    fn uma_montagem_que_falha_no_meio_desfaz_tudo() {
        let mut mods = ModsNativos::default();
        let mut viva = instancia(7);
        produzir(&mut viva, 4, 128);
        let numero = mods.guardar(viva);

        // É o que o comando faz quando a bomba não sobe.
        mods.encerrar(numero);
        mods.descartar(numero);

        assert!(
            mods.vivos.is_empty() && mods.encerrando.is_empty(),
            "a montagem desfeita continuou na supervisão"
        );
    }

    /// **Colher um erro não devolve crédito que ninguém tomou.**
    ///
    /// A revisão: «`mod_nativo_colher` subtrai quantidade e bytes de **todas**
    /// as falas colhidas» — e erro nunca reservou nada. Colhendo mensagens e
    /// erros juntos, a contabilidade devolvia mais do que tinha sido tomado, e
    /// a fila passava a aceitar acima do teto.
    #[test]
    fn colher_devolve_o_credito_das_mensagens_e_so_delas() {
        let mut viva = instancia(7);
        produzir(&mut viva, 4, 100);
        // Intercalados, como acontece de verdade: o MOD fala e falha.
        viva.guardar_fala("falhou", "um".to_owned());
        viva.guardar_fala("interrompido", String::new());
        produzir(&mut viva, 2, 100);

        let (antes, bytes_antes) = viva.executor.fila().ocupacao();
        assert_eq!(
            (antes, bytes_antes),
            (6, 600),
            "só as mensagens deviam ter reservado crédito"
        );

        // **Colhe uma mensagem só, e os avisos vêm junto.** É o estado
        // intermediário que denuncia a conta errada: com mensagens ainda
        // pendentes, devolver crédito por um aviso aparece como crédito a
        // menos. Drenando tudo de uma vez o contador satura em zero e o erro
        // fica invisível — foi assim que a primeira versão deste teste passou
        // com o defeito de volta.
        let parcial = viva.colher_falas(1);
        assert_eq!(parcial.len(), 3, "uma mensagem e os dois avisos");
        let (meio, bytes_meio) = viva.executor.fila().ocupacao();
        assert_eq!(
            (meio, bytes_meio),
            (5, 500),
            "a devolução não fechou: sobraram cinco mensagens de cem bytes"
        );

        let colhidas = viva.colher_falas(64);
        assert_eq!(colhidas.len(), 5, "a colheita não trouxe o resto");
        let (depois, bytes_depois) = viva.executor.fila().ocupacao();
        assert_eq!(
            (depois, bytes_depois),
            (0, 0),
            "a devolução não fechou com o que foi tomado"
        );

        // **E a fila volta a aceitar exatamente o teto**, nem mais nem menos:
        // um crédito devolvido a mais faria caber acima dele.
        let mut couberam = 0;
        while viva.executor.fila().cabe(1024) {
            couberam += 1;
            if couberam > crate::executor::MENSAGENS_NA_FILA + 1 {
                break;
            }
        }
        assert_eq!(
            couberam,
            crate::executor::MENSAGENS_NA_FILA,
            "a fila aceitou fora do teto depois de colher erros"
        );
    }

    /// E o descarte não encosta em quem é de outra geração.
    #[test]
    fn o_descarte_de_uma_geracao_nao_toca_na_outra() {
        let mut mods = ModsNativos::default();
        let mut da_sete = instancia(7);
        produzir(&mut da_sete, 5, 128);
        mods.guardar(da_sete);

        let mut da_oito = instancia(8);
        produzir(&mut da_oito, 5, 128);
        let outro = mods.guardar(da_oito);

        mods.revogar_geracao(7);

        let sobrevivente = mods.vivos.get(&outro).expect("a de outra geração caiu");
        assert_eq!(
            sobrevivente.pendentes.len(),
            5,
            "o descarte de uma geração levou a fala de outra"
        );
        let (mensagens, _) = sobrevivente.executor.fila().ocupacao();
        assert_eq!(mensagens, 5, "os créditos de outra geração foram soltos");
    }
}

#[cfg(test)]
mod a_janela_sabe_de_onde_veio_a_impressao {
    use super::{DeOndeVeio, FalhaDaEntrada, OrigemDaImpressao};
    use seele_ffi::{ConnectFailure, ConnectionError};

    #[test]
    fn a_origem_e_a_da_impressao_que_se_confere() {
        // As duas regras leem as mesmas entradas, e esta prende uma à outra: a
        // origem dita à janela tem de ser a da impressão que o TLS conferiu. Se
        // a ordem de `impressao_a_conferir` mudasse e esta não, a tela acusaria
        // o link de uma impressão que veio da lista, ou o contrário.
        let do_link = Some("bbbb2222");
        let guardada = Some("aaaa1111");
        for (link, lista) in [
            (do_link, guardada),
            (do_link, None),
            (None, guardada),
            (None, None),
        ] {
            let conferida = seele_ffi::impressao_a_conferir(link, lista);
            let origem = OrigemDaImpressao::da_conferida(link, lista);
            let de_onde_veio = match origem {
                Some(OrigemDaImpressao::Link) => link,
                Some(OrigemDaImpressao::Lista) => lista,
                None => None,
            };
            assert_eq!(
                de_onde_veio.map(str::to_owned),
                conferida,
                "com o link {link:?} e a guardada {lista:?}, a origem dita à janela \
                 ({origem:?}) não é a da impressão conferida ({conferida:?}): a frase acusaria \
                 quem não prometeu nada"
            );
        }
    }

    #[test]
    fn a_falha_leva_o_erro_a_trilha_e_a_origem_no_topo() {
        // `tela-boot.js` lê `falha.error` e `falha.trail` desde que a trilha
        // existe, e agora também de onde veio a impressão. O `flatten` põe os
        // quatro no topo; sem ele, o erro ficaria dentro de um invólucro que a
        // janela não atravessa, e toda falha viraria «falha sem nome».
        let falha = FalhaDaEntrada {
            falha: ConnectFailure {
                error: ConnectionError::InviteMismatch {
                    expected: "bbbb2222".into(),
                    offered: "aaaa1111".into(),
                },
                trail: Vec::new(),
            },
            de_onde: DeOndeVeio {
                origem_da_impressao: Some(OrigemDaImpressao::Lista),
                a_lista_guardou: false,
            },
            substituida: false,
        };
        let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(&falha) else {
            panic!("a falha da entrada tem de serializar para objeto");
        };
        // A marca da tentativa substituída (lote E54) só aparece quando é
        // verdade: toda outra recusa chega à janela com a forma de antes.
        assert!(
            !mapa.contains_key("substituida"),
            "uma recusa que não é de tentativa substituída chegou à janela com a marca: {mapa:?}"
        );
        for chave in ["error", "trail", "origem_da_impressao", "a_lista_guardou"] {
            assert!(
                mapa.contains_key(chave),
                "a falha da entrada não traz `{chave}` no topo, e a janela o lê de lá: {mapa:?}"
            );
        }
        assert_eq!(
            mapa.get("origem_da_impressao"),
            Some(&serde_json::Value::String("Lista".into())),
            "a origem chegou à janela com outra forma que não o nome da variante"
        );
    }

    #[test]
    fn a_janela_le_o_que_o_rust_manda() {
        // Os nomes saem da serialização de verdade: renomear um campo ou uma
        // variante de um lado só faria a janela ler `undefined`, sem erro nenhum,
        // e dizer a frase do link para uma impressão que veio da lista.
        let ler = |nome: &str| {
            std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("ui")
                    .join(nome),
            )
            .unwrap_or_else(|erro| panic!("ui/{nome} tem de ser legível: {erro}"))
        };
        let entrada = ler("tela-boot.js");
        let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(DeOndeVeio::default())
        else {
            panic!("`DeOndeVeio` tem de serializar para objeto");
        };
        for chave in mapa.keys() {
            assert!(
                entrada.contains(chave.as_str()),
                "`tela-boot.js` não lê `{chave}`, que o `connect` manda"
            );
        }
        let Ok(serde_json::Value::String(lista)) = serde_json::to_value(OrigemDaImpressao::Lista)
        else {
            panic!("a origem tem de serializar como texto");
        };
        let comparacao = format!("=== \"{lista}\"");
        for nome in ["frases.js", "tela-sessao.js", "tela-auth.js"] {
            assert!(
                ler(nome).contains(&comparacao),
                "`{nome}` não compara a origem com `\"{lista}\"`, que é como o Rust a manda: \
                 a frase da lista nunca aparece ali"
            );
        }
    }
}

/// **O relógio fixo dos testes do registro.**
///
/// O teto de vazão ([`linha_admitida`]) recebe o instante de quem chama, e um
/// teste que lesse o relógio da máquina a cada chamada mediria o balde com a
/// recarga que o tempo da própria execução trouxe. Aqui o relógio é lido uma
/// vez no processo: todo instante de um teste é esta origem mais uma duração
/// escrita nele.
#[cfg(test)]
mod relogio_de_teste {
    /// A origem do relógio fixo, a mesma para todo teste do processo.
    pub(crate) fn origem() -> std::time::Instant {
        static ORIGEM: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        *ORIGEM.get_or_init(std::time::Instant::now)
    }
}

/// **O rastro de um teste, capturado.**
///
/// Os guardas de diagnóstico desta casca perguntam «o `seele.log` diz isto?»,
/// e a resposta honesta é ler o que o `tracing` escreveu — com o formatador
/// que `main` arma, sem hora e sem cor.
///
/// **Vale para a thread corrente, e só para ela**: `with_default` é por thread.
/// O que roda noutra — a thread do motor, a bomba — não aparece aqui, e por
/// isso os testes chamam na própria thread a função que escreve (hoje,
/// `assentar_fala`; as tarefas seguintes acrescentam as delas).
#[cfg(test)]
mod rastro_de_teste {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// Onde o formatador escreve, dividido com quem lê depois.
    #[derive(Clone, Default)]
    struct Captura(Arc<Mutex<Vec<u8>>>);

    impl Write for Captura {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("o rastro capturado")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// **Um segundo despachante, vivo o processo inteiro, que não escreve
    /// nada.** É ele que tira da captura a dependência da ordem das threads.
    ///
    /// O `tracing` decide uma vez só, na primeira passagem de qualquer thread,
    /// se uma linha que escreve (o *callsite*) tem quem a ouça, e guarda a
    /// resposta. Se, no último registro de despachante, só havia um vivo, a
    /// pergunta vai só ao despachante da **thread que passou**
    /// (`tracing-core` 0.1.36, `callsite.rs`: `Dispatchers::rebuilder` devolve
    /// `Rebuilder::JustOne`, e ele consulta `dispatcher::get_default`). Um
    /// teste sem captura que passasse primeiro por uma linha, com a captura de
    /// outro teste como único despachante de pé, a deixava marcada como «nunca»
    /// — e a mesma linha, no teste que a captura, não escrevia nada.
    ///
    /// Com dois vivos, a pergunta vai a todos, e uma linha que a captura ouve
    /// fica em «às vezes»: cada evento pergunta de novo, à thread em que
    /// acontece. O que uma captura vê não muda — só o que a thread dela
    /// escreveu. Chamar `rebuild_interest_cache` ao abrir a captura não basta:
    /// a outra thread pode passar pela linha depois, com a captura já aberta,
    /// e foi medido (7 falhas em 20 rodadas do módulo do som).
    pub(crate) fn segundo_despachante() {
        static SEMPRE: std::sync::OnceLock<tracing::Dispatch> = std::sync::OnceLock::new();
        SEMPRE.get_or_init(|| tracing::Dispatch::new(tracing::subscriber::NoSubscriber::new()));
    }

    /// Roda `o_que` com um rastro só dele, e devolve o resultado e o texto.
    ///
    /// Todos os níveis, do TRACE para cima: um guarda que confere que algo
    /// **não** saiu como WARN precisa ver que saiu como DEBUG.
    ///
    /// O resultado não depende de outro teste ter passado antes pela mesma
    /// linha sem captura: ver [`segundo_despachante`].
    pub(crate) fn capturar<T>(o_que: impl FnOnce() -> T) -> (T, String) {
        segundo_despachante();
        let captura = Captura::default();
        let escreve = captura.clone();
        let assinante = tracing_subscriber::fmt()
            .with_writer(move || escreve.clone())
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_ansi(false)
            .finish();
        let resultado = tracing::subscriber::with_default(assinante, o_que);
        let bytes = captura.0.lock().expect("o rastro capturado").clone();
        (
            resultado,
            String::from_utf8(bytes).expect("o rastro é UTF-8"),
        )
    }

    /// **Uma linha que outra thread tocou primeiro, fora de qualquer captura,
    /// ainda é capturada.**
    ///
    /// É a ordem que fazia um teste do som falhar por sorte: um teste sem
    /// captura passava pela linha de recusa antes do teste que a captura, e a
    /// linha sumia do rastro deste. Aqui a ordem é forçada — a outra thread
    /// toca a linha primeiro, com a captura já de pé —, e o resultado deixa de
    /// depender de quem chega antes. E a linha da outra thread continua fora:
    /// a captura segue sendo só da thread corrente.
    #[test]
    fn uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada() {
        fn diz() {
            tracing::warn!("linha tocada primeiro fora da captura");
        }
        let ((), rastro) = capturar(|| {
            std::thread::spawn(diz)
                .join()
                .expect("a outra thread terminou");
            diz();
        });
        assert!(
            rastro.contains("linha tocada primeiro fora da captura"),
            "uma linha que outra thread tocou primeiro sumiu do rastro, e um guarda que a \
             capture passa ou falha conforme a ordem das threads: {rastro}"
        );
        assert_eq!(
            rastro.lines().count(),
            1,
            "a linha da outra thread entrou no rastro desta captura, e um teste passaria a \
             ler o que outro teste escreveu: {rastro}"
        );
    }
}

/// O gancho de pânico, pelo que ele deixa no log. F1-audio-panico.
#[cfg(test)]
mod gancho_de_panico {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// Onde o formatador escreve, dividido com quem lê depois.
    #[derive(Clone, Default)]
    struct Captura(Arc<Mutex<Vec<u8>>>);

    impl Write for Captura {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("o rastro capturado")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// (e) Um pânico numa thread com nome deixa a linha, com o nome e a
    /// mensagem.
    ///
    /// O assinante é por thread (`with_default`), e por isso ele é posto
    /// **dentro** da thread que entra em pânico: o gancho roda nela, antes de a
    /// pilha desenrolar, e escreve no assinante dela. É o caso da thread que
    /// abre o áudio e da thread da voz — threads soltas, cujo pânico antes não
    /// deixava rastro nenhum no `seele.log`.
    ///
    /// O segundo despachante vem antes, como em `rastro_de_teste::capturar`:
    /// sem ele, a linha pode sumir quando a bateria roda em paralelo
    /// (pendência 56).
    #[test]
    fn um_panico_numa_thread_deixa_linha_no_log() {
        crate::rastro_de_teste::segundo_despachante();
        super::instalar_gancho_de_panico();

        let captura = Captura::default();
        let escreve = captura.clone();
        let caiu = std::thread::Builder::new()
            .name("seele-teste-do-panico".into())
            .spawn(move || {
                let assinante = tracing_subscriber::fmt()
                    .with_writer(move || escreve.clone())
                    .with_max_level(tracing::Level::TRACE)
                    .without_time()
                    .with_ansi(false)
                    .finish();
                tracing::subscriber::with_default(assinante, || {
                    panic!("o driver caiu no meio da abertura");
                });
            })
            .expect("a thread do pânico proposital subiu")
            .join();
        assert!(caiu.is_err(), "o pânico proposital não aconteceu");

        let rastro = String::from_utf8(captura.0.lock().expect("o rastro").clone()).expect("UTF-8");
        assert!(
            rastro.contains("pânico:"),
            "um pânico numa thread não deixou linha no log: quem relata «o áudio \
             não abre» não tem o que mostrar. Rastro: {rastro:?}"
        );
        assert!(
            rastro.contains("o driver caiu no meio da abertura"),
            "a linha do pânico não diz a mensagem dele: {rastro:?}"
        );
        assert!(
            rastro.contains("seele-teste-do-panico"),
            "a linha do pânico não diz em que thread foi: {rastro:?}"
        );
    }
}

/// **O `console` de um MOD chega ao `seele.log`, com o id e o nível.**
///
/// Pelo caminho real: a mesma `assentar_fala` que a bomba chama para cada fala
/// do executor, chamada na thread do teste para o rastro ser capturável.
#[cfg(test)]
mod o_console_do_mod_chega_ao_registro {
    use super::{assentar_fala, Assentada, ModsNativos};
    use crate::executor::{Fila, NivelDoConsole, ParaOFora};
    use crate::rastro_de_teste::capturar;

    /// Assenta uma linha de console como a bomba assentaria, e devolve o que
    /// sobrou, o rastro, e a fila para conferir que nada foi reservado.
    fn assentar(nivel: NivelDoConsole, texto: &str) -> (Assentada, String, std::sync::Arc<Fila>) {
        assentar_contando(nivel, texto, 0)
    }

    /// O mesmo, com quantas linhas o balde segurou antes desta.
    fn assentar_contando(
        nivel: NivelDoConsole,
        texto: &str,
        suprimidas: u32,
    ) -> (Assentada, String, std::sync::Arc<Fila>) {
        let mods = std::sync::Mutex::new(ModsNativos::default());
        let fila = std::sync::Arc::new(Fila::default());
        let (assentada, rastro) = capturar(|| {
            assentar_fala(
                &mods,
                &|| true,
                &fila,
                1,
                ParaOFora::Console {
                    nivel,
                    texto: texto.to_owned(),
                    suprimidas,
                },
                "seele/perfis",
            )
        });
        (assentada, rastro, fila)
    }

    /// **A parada de um MOD nativo sai no `seele.log` pelo filtro padrão**, com o
    /// id e a instância.
    ///
    /// Era DEBUG, e o filtro padrão não a escreve: numa saída do app, o registro
    /// dizia que os MODs foram mandados parar e nunca que algum parou. A
    /// instância, e não a geração: o número não se repete nesta janela, e a
    /// linha «MOD reservado no executor nativo» o liga à geração.
    #[test]
    fn a_parada_do_mod_nativo_sai_em_info_com_o_id_e_a_instancia() {
        let mods = std::sync::Mutex::new(ModsNativos::default());
        let fila = std::sync::Arc::new(Fila::default());
        let (assentada, rastro) =
            capturar(|| assentar_fala(&mods, &|| true, &fila, 7, ParaOFora::Parou, "seele/perfis"));
        assert_eq!(assentada, Assentada::Fim { avisar: true });
        assert!(
            rastro.lines().any(|linha| linha.contains("INFO")
                && linha.contains("MOD nativo parou")
                && linha.contains("mod_id=seele/perfis")
                && linha.contains("instancia=7")),
            "a parada do MOD nativo não saiu em INFO com o id e a instância: o filtro padrão \
             não a escreve, e o seele.log de uma saída do app nunca diz que algum MOD parou: \
             {rastro}"
        );
    }

    /// Nos quatro níveis: um MOD em laço escreve com `log` tanto quanto com
    /// `error`, e um braço que esquecesse a conta a perderia só no nível dele.
    #[test]
    fn a_linha_depois_do_aperto_diz_quantas_ficaram_para_tras() {
        for (nivel, marca) in [
            (NivelDoConsole::Depuracao, "DEBUG"),
            (NivelDoConsole::Informacao, "INFO"),
            (NivelDoConsole::Aviso, "WARN"),
            (NivelDoConsole::Erro, "ERROR"),
        ] {
            let (_, rastro, _) = assentar_contando(nivel, "de novo", 7);
            let linha = rastro
                .lines()
                .find(|linha| linha.contains("console do MOD"))
                .unwrap_or_else(|| panic!("a linha {nivel:?} não chegou ao registro: {rastro}"));
            assert!(
                linha.contains(marca) && linha.contains("suprimidas=7"),
                "o registro não diz que houve mais do que ele mostra, e quem lê toma o pouco por \
                 tudo ({nivel:?}): {linha}"
            );
        }
        let (_, sozinha, _) = assentar(NivelDoConsole::Aviso, "sozinha");
        assert!(
            !sozinha.contains("suprimidas"),
            "uma linha sem aperto ganhou um `suprimidas=0` que só faz barulho: {sozinha}"
        );
    }

    #[test]
    fn um_console_warn_sai_como_warn_com_o_id_do_mod() {
        let (assentada, rastro, fila) =
            assentar(NivelDoConsole::Aviso, "avatar recusado: 12 bytes");
        assert_eq!(
            assentada,
            Assentada::Nada,
            "uma linha de console virou fala para a janela, e ocuparia a cota dela"
        );
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("console do MOD"))
            .unwrap_or_else(|| {
                panic!(
                    "o console do MOD não chegou ao registro — é o `console.warn` do caminho \
                     de erro sumindo em silêncio: {rastro}"
                )
            });
        for pedaco in [
            "WARN",
            "mod_id=seele/perfis",
            "texto=\"avatar recusado: 12 bytes\"",
        ] {
            assert!(
                linha.contains(pedaco),
                "a linha do console não traz `{pedaco}`, e é por ele que quem lê o registro \
                 acha o MOD e o que ele disse: {linha}"
            );
        }
        assert_eq!(
            (fila.ocupacao(), fila.avisos_de_pe()),
            ((0, 0), 0),
            "a linha de console tomou crédito da fila da janela"
        );
    }

    #[test]
    fn cada_nivel_do_console_vira_o_mesmo_nivel_no_registro() {
        for (nivel, marca) in [
            (NivelDoConsole::Depuracao, "DEBUG"),
            (NivelDoConsole::Informacao, "INFO"),
            (NivelDoConsole::Aviso, "WARN"),
            (NivelDoConsole::Erro, "ERROR"),
        ] {
            let (_, rastro, _) = assentar(nivel, "linha");
            let linha = rastro
                .lines()
                .find(|linha| linha.contains("console do MOD"))
                .unwrap_or_else(|| panic!("{nivel:?} não chegou ao registro: {rastro}"));
            assert!(
                linha.contains(marca),
                "`{nivel:?}` saiu com outro nível, e o filtro do seele.log o trataria como \
                 outra coisa: {linha}"
            );
        }
    }

    /// Nos quatro braços de `registrar_console_do_mod`, que são escritos à mão
    /// um a um: só o do aviso tinha guarda, e o mais usado, `console.log`, cai
    /// no da informação. Um braço que escrevesse o texto cru forjaria linha só
    /// no nível dele, e um que esquecesse o id deixaria a linha sem dono — o
    /// texto não o traz.
    #[test]
    fn uma_quebra_de_linha_do_mod_nao_forja_outra_linha_no_registro() {
        for nivel in [
            NivelDoConsole::Depuracao,
            NivelDoConsole::Informacao,
            NivelDoConsole::Aviso,
            NivelDoConsole::Erro,
        ] {
            let (_, rastro, _) = assentar(
                nivel,
                "primeira\n WARN seele_app: linha que o produto não escreveu",
            );
            assert_eq!(
                rastro.lines().count(),
                1,
                "um MOD escreveu uma segunda linha no seele.log ({nivel:?}), com a cara de uma \
                 linha do produto: {rastro}"
            );
            assert!(
                rastro.contains("mod_id=seele/perfis"),
                "a linha {nivel:?} do console saiu sem o id do MOD, e uma busca por `mod_id=` \
                 não a acha: {rastro}"
            );
        }
    }
}

/// **A falha do próprio MOD chega ao `seele.log`, com o id dele.**
///
/// O erro mais comum de quem escreve MOD — ler de `null`, chamar o que a API
/// não tem — não deixava linha nenhuma em INFO. A exceção no topo virava
/// `Falhou("o MOD lançou")`, sem o texto; a de um temporizador ou de um ouvinte
/// ia à janela sem número de pedido, e a janela a descartava na primeira linha
/// de `atenderOMod`; e a de dentro de uma promessa só chegava à gestão. O
/// produto sabia, e não contava.
///
/// Pelo caminho real: um executor de verdade, e a mesma `assentar_fala` da
/// bomba, chamada na thread do teste para o rastro ser capturável.
#[cfg(test)]
mod a_falha_do_mod_chega_ao_registro {
    use super::{assentar_fala, ModsNativos};
    use crate::executor::{
        ExecutorQuickJs, Fila, Limites, ParaOFora, LINHAS_DE_CONSOLE_POR_SEGUNDO,
        RAJADA_DO_CONSOLE, REJEICOES_GUARDADAS,
    };
    use crate::rastro_de_teste::capturar;
    use std::time::{Duration, Instant};

    /// O id do MOD destes testes.
    const ID: &str = "prova/falha";

    /// O que um MOD de verdade deixou.
    struct Deixou {
        /// As falas que não são de console, em ordem.
        falas: Vec<ParaOFora>,
        /// O que a bomba escreveu no registro.
        rastro: String,
        /// Quantas linhas de console saíram do motor.
        linhas: u32,
        /// Quanto tempo passou desde antes de o motor subir.
        levou: Duration,
    }

    /// Sobe `codigo` num executor de verdade, chama `depois` com ele de pé, e
    /// assenta cada linha de console como a bomba assentaria — numa captura,
    /// na thread do teste — até `basta` dizer que chega ou o `prazo` passar.
    fn deixou(
        codigo: &str,
        depois: impl FnOnce(&ExecutorQuickJs),
        basta: impl Fn(&[ParaOFora], u32) -> bool,
        prazo: Duration,
    ) -> Deixou {
        let inicio = Instant::now();
        let ((falas, linhas), rastro) = capturar(|| {
            let executor = ExecutorQuickJs::novo(Limites::default()).expect("o motor");
            executor.iniciar(codigo).expect("o código");
            depois(&executor);
            let mods = std::sync::Mutex::new(ModsNativos::default());
            let fila = std::sync::Arc::new(Fila::default());
            let mut falas = Vec::new();
            let mut linhas = 0_u32;
            while inicio.elapsed() < prazo && !basta(&falas, linhas) {
                match executor.receber(Duration::from_millis(20)) {
                    Some(fala @ ParaOFora::Console { .. }) => {
                        linhas += 1;
                        assentar_fala(&mods, &|| true, &fila, 1, fala, ID);
                    }
                    Some(fala) => falas.push(fala),
                    None => {}
                }
            }
            (falas, linhas)
        });
        Deixou {
            falas,
            rastro,
            linhas,
            levou: inicio.elapsed(),
        }
    }

    /// O máximo de linhas que o balde do console deixa sair em `levou`: a
    /// rajada, a recarga do tempo que passou e uma de folga pelo
    /// arredondamento.
    fn teto_do_balde(levou: Duration) -> u32 {
        let recarga = (levou.as_secs_f64() * f64::from(LINHAS_DE_CONSOLE_POR_SEGUNDO)).ceil();
        RAJADA_DO_CONSOLE + recarga as u32 + 1
    }

    /// A linha de erro do console deste MOD que traz todos os `pedacos`.
    fn linha_de_erro<'a>(rastro: &'a str, pedacos: &[&str]) -> Option<&'a str> {
        rastro.lines().find(|linha| {
            linha.contains("ERROR")
                && linha.contains("console do MOD")
                && linha.contains(&format!("mod_id={ID}"))
                && pedacos.iter().all(|pedaco| linha.contains(pedaco))
        })
    }

    /// Uma das falas é uma mensagem para a janela que contém `trecho`?
    fn tem_mensagem(falas: &[ParaOFora], trecho: &str) -> bool {
        falas
            .iter()
            .any(|fala| matches!(fala, ParaOFora::Mensagem(json) if json.contains(trecho)))
    }

    #[test]
    fn uma_excecao_no_topo_do_mod_chega_ao_registro_com_o_texto() {
        let lancou = |falas: &[ParaOFora]| {
            falas
                .iter()
                .any(|fala| matches!(fala, ParaOFora::Falhou(texto) if texto == "o MOD lançou"))
        };
        let deixou = deixou(
            "const avatar = null; avatar.largura;",
            |_| {},
            |falas, linhas| lancou(falas) && linhas > 0,
            Duration::from_secs(5),
        );
        assert!(
            lancou(&deixou.falas),
            "a janela deixou de receber o `Falhou` da volta que lançou, e a gestão já não diz que \
             o MOD falhou: {:?}",
            deixou.falas
        );
        let linha =
            linha_de_erro(&deixou.rastro, &["o MOD lançou", "TypeError"]).unwrap_or_else(|| {
                panic!(
                    "a exceção no topo do MOD não chegou ao seele.log com o id e o texto — quem \
                     escreveu o MOD fica com «o MOD lançou» e nada mais: {}",
                    deixou.rastro
                )
            });
        assert!(
            linha.contains(r"\n    at "),
            "a linha da exceção perdeu a primeira linha da pilha, que diz onde o MOD lançou: \
             {linha}"
        );
    }

    /// **A falha dentro de uma promessa também chega ao registro.**
    ///
    /// Todo pedido de MOD é uma promessa, e o erro mais comum de quem escreve
    /// MOD — ler de `null` — dentro de uma função `async` sem `catch` só
    /// chegava à gestão: o `Falhou` saía, e o `seele.log` ficava com uma linha
    /// DEBUG que o filtro de produção nem grava (I2 da revisão final do Plano 1).
    #[test]
    fn uma_promessa_rejeitada_sem_tratamento_chega_ao_registro_com_o_texto() {
        let rejeitou = |falas: &[ParaOFora]| {
            falas.iter().any(|fala| {
                matches!(fala, ParaOFora::Falhou(texto)
                    if texto.starts_with("promessa rejeitada sem tratamento:"))
            })
        };
        let deixou = deixou(
            "(async () => { await 0; null.x; })();",
            |_| {},
            |falas, linhas| rejeitou(falas) && linhas > 0,
            Duration::from_secs(5),
        );
        assert!(
            rejeitou(&deixou.falas),
            "a janela deixou de receber o `Falhou` da promessa rejeitada, e a gestão já não diz \
             que o MOD falhou: {:?}",
            deixou.falas
        );
        let linha = linha_de_erro(
            &deixou.rastro,
            &["promessa rejeitada sem tratamento", "TypeError", "of null"],
        )
        .unwrap_or_else(|| {
            panic!(
                "a promessa rejeitada sem tratamento não chegou ao seele.log em ERROR, com o id e \
                 o texto — quem escreveu o MOD só a vê na gestão: {}",
                deixou.rastro
            )
        });
        assert!(
            linha.contains(r"\n    at "),
            "a linha da promessa rejeitada perdeu a primeira linha da pilha, que diz onde o MOD \
             lançou: {linha}"
        );
    }

    /// **A rejeição que o MOD pega logo depois não vira linha de erro, nem
    /// `Falhou` na janela.**
    ///
    /// `try { await f(); } catch {}`, com `f` uma função `async` que lança
    /// antes do primeiro `await`: a promessa rejeita antes de o `await` prender
    /// o tratamento, e o motor avisa a rejeição nessa hora. A linha do registro
    /// e o `Falhou` da janela esperam o fim da volta, e saem só se ninguém a
    /// pegou: dizer «sem tratamento» sobre uma rejeição tratada mandaria quem
    /// escreveu o MOD procurar um defeito que não existe — no `seele.log`, em
    /// ERROR, e na gestão, como «falhou». O `Falhou` saía na hora, de dentro
    /// do rastreador (P51-01).
    ///
    /// O `fim` vem de um temporizador, numa volta seguinte: quando ele chega,
    /// as linhas e os avisos da primeira volta já passaram pelo canal.
    #[test]
    fn uma_rejeicao_pega_logo_depois_nao_vira_linha_de_erro() {
        let deixou = deixou(
            "(async () => { try { await (async () => { throw new Error('pega logo depois'); })(); } \
               catch (e) {} })();\
             (async () => { await 0; null.x; })();\
             setTimeout(() => seele.postar('fim'), 0);",
            |_| {},
            |falas, _| tem_mensagem(falas, "fim"),
            Duration::from_secs(5),
        );
        assert!(
            tem_mensagem(&deixou.falas, "fim"),
            "o MOD não chegou ao fim, e o teste não mediu nada: {:?}",
            deixou.falas
        );
        assert!(
            linha_de_erro(
                &deixou.rastro,
                &["promessa rejeitada sem tratamento", "of null"]
            )
            .is_some(),
            "a rejeição que ninguém pegou, na mesma volta, não chegou ao seele.log: {}",
            deixou.rastro
        );
        assert!(
            !deixou.rastro.contains("pega logo depois"),
            "uma rejeição que o MOD pegou com `try`/`await` saiu no seele.log como «sem \
             tratamento»: quem escreveu o MOD vai procurar um defeito que não existe: {}",
            deixou.rastro
        );
        assert!(
            !deixou.falas.iter().any(
                |fala| matches!(fala, ParaOFora::Falhou(texto) if texto.contains("pega logo depois"))
            ),
            "uma rejeição que o MOD pegou com `try`/`await` chegou à janela como `Falhou`, e a \
             gestão diz que o MOD falhou por uma promessa que ele tratou: {:?}",
            deixou.falas
        );
    }

    /// **A volta que um teto parou, sem microtarefa na fila, também termina
    /// para as rejeições.**
    ///
    /// Nada mais vai rodar nela que possa pegá-las, e a linha sai logo depois
    /// da do teto. Esperar a próxima volta deixaria a rejeição de um MOD sem
    /// temporizador nem resposta calada até a instância parar.
    #[test]
    fn a_rejeicao_de_uma_volta_parada_sem_microtarefa_na_fila_e_dita_sem_esperar_a_parada() {
        let deixou = deixou(
            "Promise.reject(new Error('antes do laço'));\
             (async () => { await 0; while (true) {} })();",
            |_| {},
            |_, linhas| linhas >= 2,
            Duration::from_secs(5),
        );
        assert!(
            linha_de_erro(&deixou.rastro, &["passou do"]).is_some(),
            "a volta do laço não foi parada por um teto, e o teste não mediu nada: {}",
            deixou.rastro
        );
        assert!(
            linha_de_erro(
                &deixou.rastro,
                &["promessa rejeitada sem tratamento", "antes do laço"]
            )
            .is_some(),
            "a volta parou por um teto sem microtarefa na fila, e a rejeição dela ficou \
             esperando uma volta seguinte que um MOD sem temporizador nem resposta não tem: {}",
            deixou.rastro
        );
    }

    /// **A rejeição que ainda esperava quando a instância parou é dita.**
    ///
    /// Uma volta que um teto parou com microtarefas na fila deixa as rejeições
    /// dela esperando a próxima volta, porque essas microtarefas ainda podiam
    /// pegá-las. Se a instância para antes, ninguém mais as pega: a linha sai
    /// na parada, antes do `Parou`. E a espera fica vazia antes de o runtime
    /// ser desfeito, porque cada rejeição segura a promessa dela.
    #[test]
    fn a_rejeicao_que_esperava_quando_a_instancia_parou_e_dita() {
        let (falas, rastro) = capturar(|| {
            let executor = ExecutorQuickJs::novo(Limites::default()).expect("o motor");
            executor
                .iniciar(
                    "Promise.reject(new Error('esperava quando parou'));\
                     (async () => { await 0; while (true) {} })();\
                     (async () => { await 0; })();",
                )
                .expect("o código");
            let mods = std::sync::Mutex::new(ModsNativos::default());
            let fila = std::sync::Arc::new(Fila::default());
            let mut falas = Vec::new();
            let mut pediu = false;
            let inicio = Instant::now();
            while inicio.elapsed() < Duration::from_secs(10) {
                match executor.receber(Duration::from_millis(20)) {
                    Some(fala @ ParaOFora::Console { .. }) => {
                        assentar_fala(&mods, &|| true, &fila, 1, fala, ID);
                    }
                    Some(ParaOFora::Parou) => {
                        falas.push(ParaOFora::Parou);
                        break;
                    }
                    Some(fala) => falas.push(fala),
                    None => {}
                }
                if !pediu
                    && falas
                        .iter()
                        .any(|fala| matches!(fala, ParaOFora::Interrompido))
                {
                    executor.pedir_encerramento();
                    pediu = true;
                }
            }
            falas
        });
        assert!(
            falas
                .iter()
                .any(|fala| matches!(fala, ParaOFora::Interrompido)),
            "a volta do laço não foi parada por um teto, e o teste não mediu a espera: {falas:?}"
        );
        assert!(
            falas.iter().any(|fala| matches!(fala, ParaOFora::Parou)),
            "a instância não confirmou a parada: {falas:?}"
        );
        assert!(
            linha_de_erro(
                &rastro,
                &["promessa rejeitada sem tratamento", "esperava quando parou"]
            )
            .is_some(),
            "a rejeição que esperava o fim de uma volta parada por um teto sumiu quando a \
             instância parou, sem chegar ao seele.log: {rastro}"
        );
    }

    /// **A rejeição além do teto da espera não se perde: ela é contada, e a
    /// conta sai no fim da volta.**
    ///
    /// Até `REJEICOES_GUARDADAS` rejeições esperam o fim da volta, cada uma com
    /// a linha dela; as de depois só são contadas, e a conta sai numa linha
    /// depois das guardadas: «e mais N rejeições sem tratamento além da
    /// espera». A primeira linha, que costuma ser a causa, continua saindo.
    /// Quarenta numa volta só: as guardadas e a conta cabem juntas na rajada
    /// do balde, e a conta não é segurada atrás delas.
    #[test]
    fn a_rejeicao_alem_do_teto_da_espera_nao_se_perde() {
        let deixou = deixou(
            "for (let i = 0; i < 40; i++) Promise.reject(new Error('r' + i));\
             setTimeout(() => seele.postar('fim'), 0);",
            |_| {},
            |falas, _| tem_mensagem(falas, "fim"),
            Duration::from_secs(5),
        );
        assert!(
            tem_mensagem(&deixou.falas, "fim"),
            "o MOD não chegou ao fim, e o teste não mediu nada: {:?}",
            deixou.falas
        );
        let ultima_guardada = format!("Error: r{}", REJEICOES_GUARDADAS - 1);
        for primeira_e_ultima in ["Error: r0", ultima_guardada.as_str()] {
            assert!(
                linha_de_erro(
                    &deixou.rastro,
                    &["promessa rejeitada sem tratamento", primeira_e_ultima]
                )
                .is_some(),
                "a rejeição «{primeira_e_ultima}», dentro do teto da espera, não chegou ao \
                 seele.log com a linha dela: {}",
                deixou.rastro
            );
        }
        let alem = 40 - REJEICOES_GUARDADAS;
        assert!(
            linha_de_erro(
                &deixou.rastro,
                &[&format!(
                    "e mais {alem} rejeições sem tratamento além da espera"
                )]
            )
            .is_some(),
            "as {alem} rejeições além do teto da espera não foram contadas no fim da volta — \
             caladas, ou a conta segurada pelo balde atrás das guardadas: {}",
            deixou.rastro
        );
        assert!(
            !deixou.rastro.contains("Error: r39"),
            "a quadragésima rejeição, além do teto da espera, saiu com a linha dela na hora, e \
             não só na conta: uma que o MOD pegasse logo depois sairia em ERROR como «sem \
             tratamento»: {}",
            deixou.rastro
        );
        assert!(
            deixou.linhas <= teto_do_balde(deixou.levou),
            "{} linhas de console em {:?}: as rejeições passaram por fora do balde",
            deixou.linhas,
            deixou.levou
        );
    }

    /// **Um `Promise.allSettled` de mais promessas que o teto da espera, que
    /// lançam antes do primeiro `await`, não deixa linha de erro.**
    ///
    /// Cada função `async` rejeita a promessa dela antes de o `allSettled`
    /// prender o tratamento, e o motor avisa cada rejeição nessa hora. As que
    /// cabem na espera saem dela quando o tratamento chega; as de depois, que
    /// só foram contadas, saem da conta. Ditas na hora, como antes, elas
    /// saíam em ERROR como «sem tratamento» — oito linhas sobre rejeições que o
    /// MOD tratou (F-Mods-m1 da revisão final do Plano 1).
    #[test]
    fn um_allsettled_alem_do_teto_da_espera_nao_vira_linha_de_erro() {
        let deixou = deixou(
            "const lanca = async (i) => { throw new Error('tratada ' + i); };\
             Promise.allSettled(Array.from({ length: 40 }, (_, i) => lanca(i)))\
               .then((r) => seele.postar('assentou ' + \
                 r.filter((x) => x.status === 'rejected').length));\
             setTimeout(() => seele.postar('fim'), 0);",
            |_| {},
            |falas, _| tem_mensagem(falas, "fim"),
            Duration::from_secs(5),
        );
        assert!(
            tem_mensagem(&deixou.falas, "assentou 40") && tem_mensagem(&deixou.falas, "fim"),
            "o `allSettled` não assentou as quarenta rejeições, ou o MOD não chegou ao fim, e o \
             teste não mediu nada: {:?}",
            deixou.falas
        );
        assert!(
            !deixou.rastro.contains("tratada "),
            "uma rejeição que o `allSettled` tratou saiu no seele.log como «sem tratamento»: \
             quem escreveu o MOD vai procurar um defeito que não existe: {}",
            deixou.rastro
        );
        assert!(
            !deixou.rastro.contains("além da espera"),
            "as rejeições além do teto da espera foram tratadas pelo `allSettled`, e a conta \
             delas saiu mesmo assim: {}",
            deixou.rastro
        );
    }

    /// **Pelo mesmo balde do `console`.** Um MOD que lança em toda resposta
    /// lança tanto quanto a janela responde; sem o balde, cada volta seria uma
    /// linha no `seele.log`, num arquivo que só gira na abertura do app.
    #[test]
    fn uma_excecao_em_cada_resposta_nao_passa_do_balde_do_console() {
        let deixou = deixou(
            "globalThis.aoResponder = () => { const a = null; a.x; };",
            |executor| {
                let fim = Instant::now() + Duration::from_millis(600);
                while Instant::now() < fim {
                    let _ = executor.entregar("{}");
                    std::thread::yield_now();
                }
            },
            |_, _| false,
            Duration::from_millis(900),
        );
        assert!(
            linha_de_erro(&deixou.rastro, &["o MOD lançou", "TypeError"]).is_some(),
            "a exceção de uma resposta não chegou ao seele.log com o id e o texto: {}",
            deixou.rastro
        );
        let teto = teto_do_balde(deixou.levou);
        assert!(
            deixou.linhas <= teto,
            "{} linhas de console em {:?}, e o balde deixa {teto}: as exceções passaram por fora \
             dele",
            deixou.linhas,
            deixou.levou
        );
        assert!(
            deixou.rastro.contains("suprimidas="),
            "o balde não segurou nada, e o teste não mediu o teto: {}",
            deixou.rastro
        );
    }

    /// **Um temporizador que lança deixa linhas com o texto, no teto do
    /// balde.** O mesmo laço de 4 ms mandava 174 mensagens por segundo à
    /// janela, que as descartava.
    ///
    /// **E com a pilha**, que diz onde no MOD: o `emTexto` do prelúdio a põe
    /// numa linha abaixo de `nome: mensagem`, e no `seele.log` a quebra sai
    /// escapada (`\n    at …`).
    #[test]
    fn um_temporizador_que_lanca_deixa_linhas_limitadas_pelo_balde() {
        let deixou = deixou(
            "setInterval(() => { const a = null; a.b; }, 4);",
            |_| {},
            |_, _| false,
            Duration::from_secs(1),
        );
        let linha = linha_de_erro(&deixou.rastro, &["erro num temporizador", "TypeError"])
            .unwrap_or_else(|| {
                panic!(
                    "a exceção de um temporizador não chegou ao seele.log com o id e o texto: {}",
                    deixou.rastro
                )
            });
        assert!(
            linha.contains(r"\n    at "),
            "a linha do erro de um temporizador perdeu a pilha, e quem escreveu o MOD não sabe \
             em que ponto dele o temporizador lançou: {linha}"
        );
        assert!(
            tem_mensagem(&deixou.falas, "erro-no-relogio"),
            "a mensagem `erro-no-relogio` deixou de ir à janela, e ela é do outro lado: {:?}",
            deixou.falas.first()
        );
        let teto = teto_do_balde(deixou.levou);
        assert!(
            deixou.linhas <= teto,
            "{} linhas de console em {:?}, e o balde deixa {teto}: o laço do temporizador passou \
             por fora dele",
            deixou.linhas,
            deixou.levou
        );
        assert!(
            deixou.rastro.contains("suprimidas="),
            "o balde não segurou nada, e o teste não mediu o teto: {}",
            deixou.rastro
        );
    }

    /// **Um ouvinte de evento que lança deixa a linha, com a pilha** — como o
    /// temporizador, e pelo mesmo `emTexto`.
    #[test]
    fn um_ouvinte_de_evento_que_lanca_deixa_a_linha() {
        let deixou = deixou(
            "SeeleUI.aoEvento(() => { const a = null; a.b; });",
            |executor| {
                executor
                    .entregar(r#"{"tipo":"evento","nome":"tecla"}"#)
                    .expect("o evento");
            },
            |falas, linhas| tem_mensagem(falas, "erro-no-evento") && linhas > 0,
            Duration::from_secs(5),
        );
        let linha = linha_de_erro(&deixou.rastro, &["erro num ouvinte de evento", "TypeError"])
            .unwrap_or_else(|| {
                panic!(
                    "a exceção de um ouvinte de `SeeleUI.aoEvento` não chegou ao seele.log com o \
                     id e o texto: {}",
                    deixou.rastro
                )
            });
        assert!(
            linha.contains(r"\n    at "),
            "a linha do erro de um ouvinte de evento perdeu a pilha, e quem escreveu o MOD não \
             sabe em que ponto dele o ouvinte lançou: {linha}"
        );
        assert!(
            tem_mensagem(&deixou.falas, "erro-no-evento"),
            "a mensagem `erro-no-evento` deixou de ir à janela, e ela é do outro lado: {:?}",
            deixou.falas
        );
    }

    /// **A volta que o produto parou não vira «o MOD lançou» no registro.**
    ///
    /// O laço infinito é dos erros mais comuns de quem escreve MOD, e o
    /// `seele.log` dizia `o MOD lançou: InternalError: interrupted` — quem
    /// parou foi o produto, pelo prazo ou pelo teto de trabalho. Com os tetos
    /// do produto, o que para primeiro depende da máquina, e por isso a frase
    /// exigida é a que as duas paradas dizem.
    #[test]
    fn uma_volta_que_o_produto_parou_nao_vira_o_mod_lancou_no_registro() {
        let deixou = deixou(
            "while (true) {}",
            |_| {},
            |falas, linhas| !falas.is_empty() && linhas > 0,
            Duration::from_secs(5),
        );
        assert!(
            linha_de_erro(
                &deixou.rastro,
                &["o produto a interrompeu (o MOD continua de pé)"]
            )
            .is_some(),
            "a volta que o produto parou não chegou ao seele.log com o id e dizendo que só ela \
             foi interrompida — sem isso, quem lê entende que o MOD acabou: {}",
            deixou.rastro
        );
        assert!(
            !deixou.rastro.contains("lançou"),
            "a volta que o produto parou foi dita no seele.log como se o MOD tivesse lançado: {}",
            deixou.rastro
        );
    }
}

/// **Um MOD que não carregou é dito no `seele.log`, uma vez por motivo.**
///
/// `codigo_do_mod` e `mod_nativo_reservar` recusavam sem uma linha: a janela
/// anotava «não carregou» sem o motivo, num mapa em memória, e tentava de novo
/// a cada quatro segundos. Dita a cada tentativa, a recusa seria novecentas
/// linhas iguais por hora; dita quando muda, é uma.
#[cfg(test)]
mod a_carga_recusada_e_dita_uma_vez_por_motivo {
    use super::{carga_aceita, carga_dita, motivo_de, recusa_de_carga_dita, FalhaNoMod};
    use crate::rastro_de_teste::capturar;

    /// Uma recusa de carga pelo `motivo`, na `geracao`, dita com a memória
    /// `ditas`.
    fn recusar(
        ditas: &std::sync::Mutex<std::collections::HashMap<String, (u64, String)>>,
        geracao: u64,
        id: &str,
        motivo: &str,
    ) -> FalhaNoMod {
        recusa_de_carga_dita(
            ditas,
            geracao,
            id,
            FalhaNoMod::Recusado {
                motivo: motivo.to_owned(),
            },
        )
    }

    /// As linhas de carga recusada no rastro.
    fn ditas_no(rastro: &str) -> Vec<&str> {
        rastro
            .lines()
            .filter(|linha| linha.contains("MOD não carregou"))
            .collect()
    }

    #[test]
    fn uma_carga_recusada_duas_vezes_pelo_mesmo_motivo_deixa_uma_linha_so() {
        let ditas = std::sync::Mutex::default();
        let (falha, rastro) = capturar(|| {
            recusar(&ditas, 1, "prova/carga", "conteudo-de-outro-mod");
            recusar(&ditas, 1, "prova/carga", "conteudo-de-outro-mod")
        });
        assert_eq!(
            motivo_de(&falha),
            "conteudo-de-outro-mod",
            "a recusa que volta à janela foi mexida, e a gestão já não diz o motivo"
        );
        let linhas = ditas_no(&rastro);
        assert_eq!(
            linhas.len(),
            1,
            "a mesma recusa de carga, de novo a cada quatro segundos, saiu a cada tentativa — \
             novecentas linhas iguais por hora: {rastro}"
        );
        for pedaco in ["WARN", "mod_id=prova/carga", "motivo=conteudo-de-outro-mod"] {
            assert!(
                linhas.iter().all(|linha| linha.contains(pedaco)),
                "a linha da carga recusada não traz `{pedaco}`, e é por ela que quem lê o \
                 registro acha o MOD e o motivo: {rastro}"
            );
        }
    }

    /// **O que muda é dito**: outro motivo, outro MOD, e o mesmo motivo de
    /// novo depois de uma carga que deu certo.
    #[test]
    fn um_motivo_novo_outro_mod_ou_a_volta_depois_de_carregar_sao_ditos() {
        let ditas = std::sync::Mutex::default();
        let ((), rastro) = capturar(|| {
            recusar(&ditas, 1, "prova/carga", "cliente-nao-servido");
            recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
            recusar(&ditas, 1, "prova/outro", "executor-nao-subiu");
            carga_aceita(&ditas, "prova/carga");
            recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
        });
        assert_eq!(
            ditas_no(&rastro).len(),
            4,
            "uma recusa que mudou — de motivo, de MOD, ou depois de uma carga que deu certo — \
             não foi dita: {rastro}"
        );
    }

    /// **Uma sessão nova diz de novo a recusa que a anterior já tinha dito.**
    ///
    /// A memória segura a recusa repetida dentro de uma sessão. Atravessando a
    /// saída, ela calava a mesma falha na sessão seguinte: a pessoa sai, entra
    /// de novo, o MOD não carrega pelo mesmo motivo, e o `seele.log` desta
    /// entrada não diz nada — a linha que existe é a de uma sessão que já
    /// acabou.
    #[test]
    fn a_mesma_recusa_numa_sessao_nova_e_dita_de_novo() {
        let sessao = super::Session::default();
        let ((), rastro) = capturar(|| {
            recusar(
                &sessao.recusas_de_carga,
                1,
                "prova/carga",
                "conteudo-de-outro-mod",
            );
            sessao.revogar();
            recusar(
                &sessao.recusas_de_carga,
                2,
                "prova/carga",
                "conteudo-de-outro-mod",
            );
        });
        assert_eq!(
            ditas_no(&rastro).len(),
            2,
            "a recusa de carga dita numa sessão calou a mesma recusa na sessão seguinte, e o \
             seele.log da entrada nova não diz por que o MOD não carregou: {rastro}"
        );
    }

    /// **A recusa atrasada da sessão que acabou não cala a da seguinte.**
    ///
    /// A corrida que a limpeza de [`super::Session::revogar`] não fecha: uma
    /// carga passou da conferência de geração antes da saída e falhou depois
    /// dela. A recusa dela é dita — e guardada — depois da limpeza, e a mesma
    /// recusa na sessão seguinte achava o motivo igual e ficava calada, com a
    /// linha da sessão anterior segundos antes no arquivo. Guardada com a
    /// geração, a da sessão seguinte é outra recusa.
    #[test]
    fn a_recusa_atrasada_da_sessao_que_acabou_nao_cala_a_da_seguinte() {
        let sessao = super::Session::default();
        // A sessão 1 de pé.
        assert_eq!(
            sessao.revogar(),
            1,
            "a primeira geração de uma sessão não é 1"
        );
        let ((), rastro) = capturar(|| {
            assert_eq!(sessao.revogar(), 2, "a saída não fechou a geração 1");
            recusar(
                &sessao.recusas_de_carga,
                1,
                "prova/carga",
                "conteudo-de-outro-mod",
            );
            recusar(
                &sessao.recusas_de_carga,
                2,
                "prova/carga",
                "conteudo-de-outro-mod",
            );
        });
        let avisos = ditas_no(&rastro)
            .into_iter()
            .filter(|linha| linha.contains("WARN"))
            .count();
        assert_eq!(
            avisos, 2,
            "a recusa atrasada da sessão que acabou calou a mesma recusa na sessão seguinte, e \
             o seele.log da entrada nova não diz por que o MOD não carregou: {rastro}"
        );
    }

    /// **A sessão que acabou não é recusa a dizer**: é o desfecho certo de
    /// uma carga atrasada, contada em `comandos_de_geracao_morta`.
    #[test]
    fn a_sessao_que_acabou_nao_vira_aviso_de_carga() {
        let ditas = std::sync::Mutex::default();
        let (_, rastro) = capturar(|| recusar(&ditas, 1, "prova/carga", "sessao-encerrada"));
        assert!(
            !rastro.contains("WARN") && rastro.contains("DEBUG"),
            "uma carga que não subiu porque a pessoa saiu virou aviso — é o desfecho certo, e \
             não um defeito: {rastro}"
        );
    }

    /// **`carga_dita`, no braço que aceita**: o valor volta intacto, e o motivo
    /// guardado é esquecido — a mesma recusa depois dela é dita de novo.
    #[test]
    fn a_carga_dita_que_deu_certo_esquece_o_motivo_e_devolve_o_valor() {
        let ditas = std::sync::Mutex::default();
        let (valor, rastro) = capturar(|| {
            recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
            let valor = carga_dita(&ditas, 1, "prova/carga", Ok::<u64, FalhaNoMod>(7));
            recusar(&ditas, 1, "prova/carga", "executor-nao-subiu");
            valor
        });
        assert!(
            matches!(valor, Ok(7)),
            "a carga que deu certo não voltou intacta, e a janela perde o número da instância: \
             {valor:?}"
        );
        assert_eq!(
            ditas_no(&rastro).len(),
            2,
            "a carga que deu certo não esqueceu o motivo, e a mesma recusa depois dela ficou \
             calada — um MOD que subiu e caiu de novo pelo mesmo motivo some do registro: \
             {rastro}"
        );
    }

    /// **`carga_dita`, no braço que recusa**: passa pela recusa dita — uma
    /// linha por motivo, e não uma por tentativa — e a recusa volta intacta.
    #[test]
    fn a_carga_dita_recusada_e_dita_uma_vez_e_volta_intacta() {
        let ditas = std::sync::Mutex::default();
        let (cargas, rastro) = capturar(|| {
            (0..3)
                .map(|_| {
                    carga_dita(
                        &ditas,
                        1,
                        "prova/carga",
                        Err::<u64, _>(FalhaNoMod::Recusado {
                            motivo: "conteudo-de-outro-mod".to_owned(),
                        }),
                    )
                })
                .collect::<Vec<_>>()
        });
        for carga in &cargas {
            let falha = carga
                .as_ref()
                .expect_err("uma carga recusada voltou como aceita");
            assert_eq!(
                motivo_de(falha),
                "conteudo-de-outro-mod",
                "a recusa que volta à janela foi mexida, e a gestão já não diz o motivo"
            );
        }
        assert_eq!(
            ditas_no(&rastro).len(),
            1,
            "a recusa de carga não passou pela recusa dita uma vez por motivo: três tentativas \
             deixaram outra conta de linhas: {rastro}"
        );
    }

    /// O corpo de `comando` em `main.rs` — do `{` da assinatura ao `}` que a
    /// fecha —, sem as linhas de comentário, sem espaço nenhum e sem a vírgula
    /// que o `rustfmt` põe antes de um `)` quando quebra a chamada em linhas: o
    /// que sobra é o código, e a formatação não muda a conta.
    fn corpo_sem_espaco(fonte: &str, comando: &str) -> String {
        let funcao = fonte
            .split(comando)
            .nth(1)
            .and_then(|resto| resto.split("\n}\n").next())
            .unwrap_or_else(|| panic!("`{comando}` sumiu de main.rs"));
        let corpo = funcao
            .split_once("> {\n")
            .map(|(_, corpo)| corpo)
            .unwrap_or_else(|| panic!("o recorte não achou o corpo de `{comando}`: {funcao}"));
        corpo
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .flat_map(str::chars)
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .replace(",)", ")")
    }

    /// **Os dois comandos de carga passam pela recusa dita**, cada um na forma
    /// exata que lhe cabe, e só a reserva que deu certo esquece o motivo. Os
    /// comandos precisam de um `AppHandle` e não sobem num teste; um que
    /// devolvesse a recusa por fora seria o MOD sem carregar e o registro
    /// calado de novo, com os testes acima verdes.
    ///
    /// **A reserva é a chamada única de [`carga_dita`]**, e não um `match` que
    /// a imite. Conferir só que `carga_aceita(` e `recusa_de_carga_dita(`
    /// aparecem no corpo deixava passar o `carga_aceita` no braço errado — a
    /// recusa esquecida a cada tentativa, e de novo as novecentas linhas por
    /// hora — sem nada reprovar. Os dois braços são provados nos testes de
    /// `carga_dita`, acima; aqui se prova que a reserva é ela.
    ///
    /// **E o código é só a recusa dita**: `ler_codigo_do_mod(..)` com o
    /// `map_err` de [`recusa_de_carga_dita`], e nada mais. Não é `carga_dita`:
    /// a janela pede o código e depois a reserva, de novo a cada quatro
    /// segundos enquanto o MOD não sobe, e o código lido esqueceria o motivo
    /// antes de a reserva falhar — a mesma recusa dita a cada tentativa
    /// (medido: dez linhas em dez tentativas, contra uma). Conferir só que
    /// `recusa_de_carga_dita(` aparece no corpo deixava passar esse
    /// esquecimento, num `match` com `carga_aceita` no `Ok` ou num `.inspect`
    /// depois do `map_err`, sem nada reprovar.
    #[test]
    fn os_dois_comandos_de_carga_passam_pela_recusa_dita() {
        let fonte = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("main.rs legível")
        .replace("\r\n", "\n");
        let codigo = corpo_sem_espaco(&fonte, "fn codigo_do_mod(");
        assert_eq!(
            codigo,
            "ler_codigo_do_mod(&app,&session,geracao,&id,&hash).map_err(|falha|\
             recusa_de_carga_dita(&session.recusas_de_carga,geracao,&id,falha))",
            "`fn codigo_do_mod(` deixou de ser `ler_codigo_do_mod(..)` com o `map_err` de \
             `recusa_de_carga_dita`, e nada mais: sem a recusa dita, a carga recusada volta à \
             janela e não chega ao registro; com um `carga_aceita` (ou `carga_dita`), o código \
             lido esquece o motivo antes de a reserva falhar, e a mesma recusa volta a sair a \
             cada quatro segundos"
        );
        let reserva = corpo_sem_espaco(&fonte, "fn mod_nativo_reservar(");
        assert_eq!(
            reserva,
            "carga_dita(&session.recusas_de_carga,geracao,&id,reservar_no_executor(&app,&session,\
             geracao,&id,hash))",
            "`fn mod_nativo_reservar(` deixou de ser a chamada única de `carga_dita` sobre \
             `reservar_no_executor`: um `match` escrito à mão pode pôr o `carga_aceita` no braço \
             errado, e a mesma recusa volta a sair a cada quatro segundos, ou a seguinte fica \
             calada"
        );
    }
}

/// **O que a janela escreve no registro, com o id do MOD em campo próprio.**
///
/// O comando é chamado direto: é uma função como outra qualquer, e o que se
/// mede é a linha que ela deixa no `seele.log`.
#[cfg(test)]
mod o_registro_da_janela {
    use crate::rastro_de_teste::capturar;
    use crate::relogio_de_teste::origem;

    #[test]
    fn o_aviso_da_janela_sai_como_warn_com_o_id_do_mod_em_campo_proprio() {
        let ((), rastro) = capturar(|| {
            super::registrar_da_janela_em(
                &super::BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                "atender-mod".to_owned(),
                "seele/perfis: «contribuir» falhou — a API de MODs não conhece o ponto «x»"
                    .to_owned(),
                Some("seele/perfis".to_owned()),
            );
        });
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("«contribuir» falhou"))
            .unwrap_or_else(|| panic!("a recusa da janela não chegou ao registro: {rastro}"));
        for pedaco in ["WARN", "mod_id=seele/perfis", "onde=atender-mod"] {
            assert!(
                linha.contains(pedaco),
                "a linha da janela não traz `{pedaco}`, e `mod_id=` é o que junta numa busca \
                 o que a janela e o Rust disseram do mesmo MOD: {linha}"
            );
        }
        assert!(
            linha.contains(
                "frase=\"seele/perfis: «contribuir» falhou — a API de MODs não conhece o ponto \
                 «x»\""
            ),
            "a frase da janela não saiu inteira no campo `frase=\"…\"`: solta na mensagem, um \
             `mod_id=` escrito por um MOD dentro dela vem antes do verdadeiro na linha: {linha}"
        );
    }

    /// **O id do MOD vem antes da frase, e um `mod_id=` escrito nela não se
    /// passa pelo verdadeiro.**
    ///
    /// A frase carrega texto que um MOD escolheu. Escrita como a mensagem da
    /// linha, ela vinha antes dos campos, e um MOD que pusesse
    /// `mod_id=vitima/mod` nela tinha o primeiro `mod_id=` da linha: quem lê da
    /// esquerda para a direita atribui a frase ao MOD errado. Num campo em `?`,
    /// ela sai depois do id, entre aspas e escapada.
    #[test]
    fn um_mod_id_forjado_na_frase_nao_vem_antes_do_verdadeiro() {
        let ((), rastro) = capturar(|| {
            super::registrar_da_janela_em(
                &super::BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                "atender-mod".to_owned(),
                "x onde=forjado mod_id=vitima/mod".to_owned(),
                Some("a/b".to_owned()),
            );
        });
        let primeiro = |nome: &str| -> String {
            rastro
                .split(&format!("{nome}="))
                .nth(1)
                .and_then(|resto| resto.split_whitespace().next())
                .unwrap_or_else(|| panic!("a linha da janela não traz `{nome}=`: {rastro}"))
                .to_owned()
        };
        assert_eq!(
            primeiro("mod_id"),
            "a/b",
            "o primeiro `mod_id=` da linha da janela é o que um MOD escreveu na frase, e não o \
             do campo: a linha se lê como sendo de outro MOD: {rastro}"
        );
        assert_eq!(
            primeiro("onde"),
            "atender-mod",
            "o primeiro `onde=` da linha da janela é o que um MOD escreveu na frase, e não o do \
             campo: {rastro}"
        );
        assert!(
            rastro.contains(r#"frase="x onde=forjado mod_id=vitima/mod""#),
            "a frase forjada não saiu entre aspas no campo `frase`: {rastro}"
        );
    }

    /// A linha da janela no `nivel` dado, com o id `a/b`, e o rastro dela.
    fn da_janela_no_nivel(nivel: &str) -> String {
        let ((), rastro) = capturar(|| {
            super::registrar_da_janela_em(
                &super::BaldesDoRegistro::default(),
                origem(),
                nivel.to_owned(),
                "atender-mod".to_owned(),
                "uma frase da janela".to_owned(),
                Some("a/b".to_owned()),
            );
        });
        rastro
    }

    /// Os pedaços da linha, separados por espaço: um campo é um pedaço inteiro,
    /// e `mod_id=a/b` não casa com `mod_id=a/bc`.
    fn tem_o_campo(linha: &str, campo: &str) -> bool {
        linha.split_whitespace().any(|pedaco| pedaco == campo)
    }

    /// **Os três braços levam o id, e não só o do aviso.** O teste de cima
    /// prova o WARN; o ERROR e o DEBUG escrevem por chamadas próprias, e um
    /// braço que esquecesse o `mod_id` deixava a linha fora da busca por
    /// `mod_id=` sem que nenhum teste reclamasse (T3 M2). Pelos campos, e não
    /// pela forma inteira da linha.
    #[test]
    fn o_erro_da_janela_sai_como_error_com_o_id_do_mod() {
        let rastro = da_janela_no_nivel("erro");
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("uma frase da janela"))
            .unwrap_or_else(|| panic!("o erro da janela não chegou ao registro: {rastro}"));
        assert!(
            tem_o_campo(linha, "ERROR"),
            "o erro da janela não saiu em ERROR, e o filtro do seele.log o trata como outra \
             coisa: {linha}"
        );
        assert!(
            tem_o_campo(linha, "mod_id=a/b"),
            "o erro da janela saiu sem `mod_id=`, e uma busca pelo MOD não junta o que a janela \
             disse dele com o que o Rust disse: {linha}"
        );
    }

    /// O mesmo para um nível que a janela não conhece, que sai em DEBUG.
    #[test]
    fn um_nivel_desconhecido_da_janela_sai_como_debug_com_o_id_do_mod() {
        let rastro = da_janela_no_nivel("outro");
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("uma frase da janela"))
            .unwrap_or_else(|| {
                panic!(
                    "a linha da janela num nível desconhecido não chegou ao registro, que captura \
                     do TRACE para cima: {rastro}"
                )
            });
        assert!(
            tem_o_campo(linha, "DEBUG"),
            "a linha da janela num nível desconhecido não saiu em DEBUG: {linha}"
        );
        assert!(
            tem_o_campo(linha, "mod_id=a/b"),
            "a linha DEBUG da janela saiu sem `mod_id=`, e com `RUST_LOG=debug` uma busca pelo \
             MOD não a acha: {linha}"
        );
    }

    #[test]
    fn sem_id_a_linha_da_janela_continua_saindo() {
        let ((), rastro) = capturar(|| {
            super::registrar_da_janela_em(
                &super::BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                "geral".to_owned(),
                "sem MOD".to_owned(),
                None,
            );
        });
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("sem MOD"))
            .unwrap_or_else(|| panic!("uma linha sem MOD deixou de sair: {rastro}"));
        assert!(
            linha.contains("WARN") && !linha.contains("mod_id"),
            "uma linha sem MOD ganhou um `mod_id` que não é de ninguém: {linha}"
        );
    }

    /// **O id vazio é o catálogo, e não um MOD.** `carregarMods` pede o
    /// catálogo do servidor por `pedirAoServidor("", …)`, e as recusas de
    /// `pedirAoServidor` passam o id adiante como está. Um `mod_id=` sem nome
    /// é um campo que uma busca por `mod_id=` acha e que não é de MOD nenhum.
    #[test]
    fn o_id_vazio_do_catalogo_nao_vira_um_mod_id_sem_nome() {
        let ((), rastro) = capturar(|| {
            super::registrar_da_janela_em(
                &super::BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                "pedido-de-mod".to_owned(),
                ": oito pedidos em voo, este não cabe".to_owned(),
                Some(String::new()),
            );
        });
        let linha = rastro
            .lines()
            .find(|linha| linha.contains("oito pedidos em voo"))
            .unwrap_or_else(|| panic!("a recusa do catálogo deixou de sair: {rastro}"));
        assert!(
            linha.contains("WARN") && !linha.contains("mod_id"),
            "o pedido do catálogo, que não é de MOD nenhum, saiu com um `mod_id` sem \
             nome — e uma busca por `mod_id=` o acha: {linha}"
        );
    }

    #[test]
    fn uma_quebra_de_linha_da_janela_nao_forja_outra_linha_no_registro() {
        let ((), rastro) = capturar(|| {
            super::registrar_da_janela_em(
                &super::BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                "recusa-de-mod".to_owned(),
                "a/b: ponto «x\nWARN seele_app: forjada»".to_owned(),
                Some("a/b".to_owned()),
            );
        });
        assert_eq!(
            rastro.lines().count(),
            1,
            "texto de MOD atravessou a janela e escreveu uma segunda linha no seele.log: {rastro}"
        );
    }

    /// **Os dois campos também, e não só a frase.** `onde` é, hoje, um literal
    /// do produto, e o id vem de um manifesto já conferido; mas os dois chegam
    /// pela mesma ponte que a frase, e o registro não pode depender de quem
    /// chama ser bem-comportado. Um caso por campo, para que tirar o filtro de
    /// um deles reprove sozinho.
    #[test]
    fn uma_quebra_de_linha_em_onde_ou_no_id_nao_forja_outra_linha_no_registro() {
        for (campo, onde, mod_id) in [
            ("onde", "atender-mod\nWARN seele_app: forjada", "a/b"),
            ("mod_id", "atender-mod", "a/b\nWARN seele_app: forjada"),
        ] {
            let ((), rastro) = capturar(|| {
                super::registrar_da_janela_em(
                    &super::BaldesDoRegistro::default(),
                    origem(),
                    "aviso".to_owned(),
                    onde.to_owned(),
                    "a/b: uma frase de verdade".to_owned(),
                    Some(mod_id.to_owned()),
                );
            });
            assert_eq!(
                rastro.lines().count(),
                1,
                "uma quebra de linha em `{campo}` escreveu uma segunda linha no seele.log, \
                 com a cara de uma linha do produto: {rastro}"
            );
        }
    }

    /// **O que quebra ou inverte a linha num editor também não passa**, e não
    /// só o caractere de controle.
    ///
    /// As portas do Rust escrevem pelo `Debug` de um `str` e saem com
    /// `\u{2028}` e `\u{202e}`; a da janela escrevia a frase sem escape, e
    /// deixava passar crus os separadores de linha e de parágrafo — que um
    /// editor quebra como um `\n` — e os controles bidirecionais: um U+202E na
    /// frase de um MOD inverte, num editor que segue o bidi, o `onde=` e o
    /// `mod_id=` que vêm depois dela. Um caso por campo, para que tirar o
    /// filtro de um deles reprove sozinho.
    #[test]
    fn a_janela_nao_deixa_passar_o_que_quebra_ou_inverte_a_linha() {
        for c in [
            '\u{0085}', '\u{2028}', '\u{2029}', '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}',
            '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}',
            '\u{2069}',
        ] {
            let com = |texto: &str| format!("{texto}{c}forjado");
            for (campo, onde, o_que, mod_id) in [
                (
                    "frase",
                    "recusa-de-mod".to_owned(),
                    com("a/b: x"),
                    "a/b".to_owned(),
                ),
                (
                    "id",
                    "recusa-de-mod".to_owned(),
                    "a/b: x".to_owned(),
                    com("a/b"),
                ),
                (
                    "onde",
                    com("recusa-de-mod"),
                    "a/b: x".to_owned(),
                    "a/b".to_owned(),
                ),
            ] {
                let ((), rastro) = capturar(|| {
                    super::registrar_da_janela_em(
                        &super::BaldesDoRegistro::default(),
                        origem(),
                        "aviso".to_owned(),
                        onde,
                        o_que,
                        Some(mod_id),
                    );
                });
                assert!(
                    rastro.contains("a/b: x") && !rastro.contains(c),
                    "U+{:04X} no campo `{campo}` saiu cru no seele.log, e num editor ele quebra \
                     a linha ou inverte o `onde=` e o `mod_id=` que vêm depois: {rastro:?}",
                    u32::from(c)
                );
            }
        }
    }
}

/// **O teto de vazão de cada MOD no `seele.log`**, pelas portas que não são
/// o `console`.
///
/// Com o relógio fixo ([`relogio_de_teste`]): cada instante é a origem mais
/// uma duração, e o balde é medido sem dormir.
#[cfg(test)]
mod o_teto_de_vazao_do_registro {
    use std::time::{Duration, Instant};

    use super::{
        midia_servida_dita, midia_vinda_do_servidor_dita, recusa_de_midia_dita,
        registrar_da_janela_em, som_servido_dito, BaldesDoRegistro, Classe, FalhaNoMod, Session,
        BALDES_NO_REGISTRO,
    };
    use crate::executor::RAJADA_DO_CONSOLE;
    use crate::rastro_de_teste::capturar;
    use crate::relogio_de_teste::origem;

    /// Um aviso da janela em nome de `id`, no instante `agora`.
    fn da_janela(baldes: &BaldesDoRegistro, agora: Instant, id: &str) {
        da_janela_no(baldes, agora, "aviso", id);
    }

    /// Uma linha da janela no `nivel` que a janela manda.
    fn da_janela_no(baldes: &BaldesDoRegistro, agora: Instant, nivel: &str, id: &str) {
        registrar_da_janela_em(
            baldes,
            agora,
            nivel.to_owned(),
            "atender-mod".to_owned(),
            "uma frase".to_owned(),
            Some(id.to_owned()),
        );
    }

    /// Uma mídia de `id` não servida porque a sessão acabou: a linha DEBUG de
    /// [`recusa_de_midia_dita`].
    fn nao_servida(baldes: &BaldesDoRegistro, agora: Instant, id: &str) {
        recusa_de_midia_dita(
            baldes,
            agora,
            id,
            "img/rosto.png",
            FalhaNoMod::Recusado {
                motivo: "sessao-encerrada".to_owned(),
            },
        );
    }

    /// Uma recusa de mídia de `id`, no instante `agora`.
    fn recusa(baldes: &BaldesDoRegistro, agora: Instant, id: &str) {
        recusa_de_midia_dita(
            baldes,
            agora,
            id,
            "img/rosto.png",
            FalhaNoMod::Recusado {
                motivo: "arquivo-nao-declarado".to_owned(),
            },
        );
    }

    /// As linhas do rastro com `mod_id=<id>` num campo.
    fn de<'a>(rastro: &'a str, id: &str) -> Vec<&'a str> {
        let campo = format!("mod_id={id}");
        rastro
            .lines()
            .filter(|linha| linha.split_whitespace().any(|pedaco| pedaco == campo))
            .collect()
    }

    /// Um quarto de segundo: a recarga de uma ficha do balde.
    const UMA_FICHA: Duration = Duration::from_millis(250);

    #[test]
    fn duzentas_linhas_de_um_mod_no_mesmo_instante_deixam_so_a_rajada() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..200 {
                da_janela(&baldes, agora, "a/b");
            }
        });
        assert_eq!(
            de(&rastro, "a/b").len(),
            RAJADA_DO_CONSOLE as usize,
            "duzentas linhas da janela no mesmo instante passaram de outro jeito que a rajada: \
             um MOD num laço escreve sem teto no seele.log de quem hospeda"
        );
    }

    /// **O que o balde segura é contado**, e a primeira linha que passa depois
    /// leva a conta: o registro diz que houve mais do que ele mostra.
    #[test]
    fn a_linha_depois_do_aperto_diz_quantas_ficaram_para_tras() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..200 {
                da_janela(&baldes, agora, "a/b");
            }
            da_janela(&baldes, agora + UMA_FICHA, "a/b");
        });
        let linhas = de(&rastro, "a/b");
        let seguradas = 200 - RAJADA_DO_CONSOLE as usize;
        assert_eq!(
            linhas.len(),
            RAJADA_DO_CONSOLE as usize + 1,
            "um quarto de segundo depois da rajada, a linha seguinte não passou (ou passou mais \
             de uma): {rastro}"
        );
        let ultima = linhas.last().copied().unwrap_or_default();
        assert!(
            ultima.contains(&format!("suprimidas={seguradas}")),
            "a linha que passou depois do aperto não diz as {seguradas} que o balde segurou, e o \
             registro mostra a rajada como se fosse tudo: {ultima}"
        );
        assert!(
            linhas
                .iter()
                .take(RAJADA_DO_CONSOLE as usize)
                .all(|linha| !linha.contains("suprimidas")),
            "uma linha da rajada trouxe `suprimidas` sem nada ter sido segurado: {rastro}"
        );
    }

    #[test]
    fn um_mod_nao_segura_as_linhas_de_outro() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..200 {
                da_janela(&baldes, agora, "a/b");
            }
            da_janela(&baldes, agora, "c/d");
        });
        assert_eq!(
            de(&rastro, "c/d").len(),
            1,
            "a linha de `c/d` foi segurada pelo balde de `a/b`: um MOD num laço cala o \
             diagnóstico de todos os outros"
        );
    }

    /// **Com o mapa cheio, um id novo gasta do balde de transbordo da classe
    /// dele**, e nunca passa sem teto: o mapa não cresce, e o balde é um só
    /// para todos os ids da classe que chegaram depois. E é um por classe: as
    /// linhas servidas que transbordam não gastam a ficha de um aviso que
    /// transborda.
    #[test]
    fn com_o_mapa_cheio_um_id_novo_passa_pelo_transbordo() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            // Um aviso de um id novo por balde próprio que o mapa aceita — e
            // não além: o que passasse disso gastaria do transbordo antes do
            // id medido.
            for n in 0..BALDES_NO_REGISTRO - Classe::TODAS.len() {
                da_janela(&baldes, agora, &format!("cheio/{n}"));
            }
            for _ in 0..200 {
                midia_servida_dita(&baldes, agora, "servida/x", 7, "img/a.png", "imagem", 8);
            }
            for _ in 0..200 {
                da_janela(&baldes, agora, "novo/x");
            }
            da_janela(&baldes, agora, "novo/y");
            for n in 0..BALDES_NO_REGISTRO {
                da_janela(&baldes, agora, &format!("mais/{n}"));
            }
            da_janela_no(&baldes, agora, "depuracao", "depuracao/x");
        });
        for id in ["servida/x", "novo/x"] {
            let do_novo = de(&rastro, id).len();
            assert!(
                (1..=RAJADA_DO_CONSOLE as usize).contains(&do_novo),
                "com o mapa cheio, `{id}` deixou {do_novo} linhas em duzentas: zero cala um MOD \
                 que nunca escreveu — ou outra classe secou o transbordo desta —, e mais que a \
                 rajada é um id novo passando sem teto"
            );
        }
        let depois: usize = std::iter::once("novo/y".to_owned())
            .chain((0..BALDES_NO_REGISTRO).map(|n| format!("mais/{n}")))
            .map(|id| de(&rastro, &id).len())
            .sum();
        assert_eq!(
            depois, 0,
            "os ids novos seguintes ganharam balde cheio, e não o transbordo que o primeiro \
             secou: uma janela que muda de id a cada linha escreve sem teto: {rastro}"
        );
        let guardados = baldes.lock().expect("os baldes").len();
        assert_eq!(
            guardados, BALDES_NO_REGISTRO,
            "o mapa dos baldes não ficou em {BALDES_NO_REGISTRO} entradas com as três classes \
             transbordando: passar delas faz de uma janela que manda um id novo por linha \
             memória sem teto"
        );
    }

    /// **A sessão que fecha diz a conta que nenhuma linha levou**, uma linha
    /// por balde — o do catálogo também —, e esvazia o mapa.
    #[test]
    fn revogar_diz_a_conta_que_nenhuma_linha_levou() {
        let sessao = Session::default();
        let agora = origem();
        let a_mais = 8;
        let ((), rastro) = capturar(|| {
            for _ in 0..RAJADA_DO_CONSOLE as usize + a_mais {
                da_janela(&sessao.baldes_do_registro, agora, "a/b");
                registrar_da_janela_em(
                    &sessao.baldes_do_registro,
                    agora,
                    "aviso".to_owned(),
                    "pedido-de-mod".to_owned(),
                    "o catálogo".to_owned(),
                    None,
                );
            }
            sessao.revogar();
        });
        let contas: Vec<&str> = rastro
            .lines()
            .filter(|linha| linha.contains("seguradas pelo teto de vazão"))
            .collect();
        assert_eq!(
            contas.len(),
            2,
            "a sessão fechou com linhas seguradas em dois baldes e não disse uma conta por \
             balde: {rastro}"
        );
        for pedaco in ["mod_id=a/b", "catalogo=true"] {
            assert!(
                contas.iter().any(|conta| conta.contains(pedaco)
                    && conta.contains(&format!("suprimidas={a_mais}"))),
                "a sessão fechou sem dizer as {a_mais} linhas seguradas de `{pedaco}`: a conta \
                 caiu com o balde, e o seele.log mostra a rajada como se fosse tudo: {rastro}"
            );
        }
        assert!(
            sessao
                .baldes_do_registro
                .lock()
                .expect("os baldes")
                .is_empty(),
            "os baldes atravessaram a sessão: a seguinte começa com o teto gasto pela anterior"
        );
    }

    /// **A mídia vinda do servidor tem teto, na classe da depuração.** É o par
    /// bem-sucedido da mídia recusada de `midia_em_bytes`, em DEBUG: com
    /// `RUST_LOG=debug`, um MOD que pede a mesma mídia num laço escrevia uma
    /// linha por pedido. Na depuração, e não nas servidas: o filtro padrão não
    /// a escreve, e ela não pode gastar a ficha de uma linha que ele escreve.
    #[test]
    fn a_midia_vinda_do_servidor_tem_teto_na_classe_da_depuracao() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..200 {
                midia_vinda_do_servidor_dita(&baldes, agora, "a/b", 7, "imagem", 8);
            }
            recusa(&baldes, agora, "a/b");
            midia_servida_dita(&baldes, agora, "a/b", 7, "img/a.png", "imagem", 8);
            midia_vinda_do_servidor_dita(&baldes, agora + UMA_FICHA, "a/b", 7, "imagem", 8);
        });
        let vindas: Vec<&str> = rastro
            .lines()
            .filter(|linha| linha.contains("mídia de MOD vinda do servidor"))
            .collect();
        assert_eq!(
            vindas.len(),
            RAJADA_DO_CONSOLE as usize + 1,
            "duzentas mídias vindas do servidor no mesmo instante passaram de outro jeito que a \
             rajada, e a seguinte um quarto de segundo depois: com RUST_LOG=debug, um MOD num \
             laço escreve sem teto"
        );
        assert!(
            vindas
                .iter()
                .all(|linha| linha.split_whitespace().next() == Some("DEBUG")),
            "a mídia vinda do servidor deixou de sair em DEBUG: {rastro}"
        );
        let recusada = rastro
            .lines()
            .find(|linha| linha.contains("mídia de MOD recusada"));
        assert!(
            recusada
                .is_some_and(|linha| linha.contains("mod_id=a/b") && !linha.contains("suprimidas")),
            "as mídias vindas do servidor gastaram a ficha dos avisos do mesmo MOD, e a recusa \
             sumiu — o defeito de 23/09 na direção oposta: {rastro}"
        );
        assert!(
            rastro.contains("mídia de MOD servida"),
            "as mídias vindas do servidor gastaram a ficha das servidas do mesmo MOD: {rastro}"
        );
        let seguradas = 200 - RAJADA_DO_CONSOLE as usize;
        assert!(
            vindas
                .last()
                .is_some_and(|linha| linha.contains(&format!("suprimidas={seguradas}"))),
            "a mídia vinda do servidor que passou depois do aperto não diz as {seguradas} que o \
             balde segurou: {rastro}"
        );
    }

    /// **Os comandos passam às portas os baldes da sessão.**
    ///
    /// Os testes acima medem as portas com baldes que eles mesmos criam; os
    /// comandos precisam de `AppHandle`, de conexão ou de sessão e não sobem
    /// num teste. Um `&BaldesDoRegistro::default()` dentro de um comando
    /// compila, deixa os testes acima verdes e tira o teto: cada chamada
    /// ganharia um balde cheio. Aqui, toda chamada de porta nos cinco comandos
    /// tem de levar `&session.baldes_do_registro` no primeiro argumento.
    ///
    /// Em `ler_imagem_mod` as duas são a conexão ausente
    /// (`recusa_de_midia_dita`) e `imagem_lida_dita`, que leva as duas recusas
    /// de depois do `await`.
    #[test]
    fn os_comandos_passam_os_baldes_da_sessao() {
        const PORTAS: [&str; 9] = [
            "registrar_da_janela_em(",
            "midia_declarada_do_mod(",
            "midia_servida_dita(",
            "som_declarado_do_mod(",
            "som_servido_dito(",
            "midia_do_servidor_dita(",
            "midia_vinda_do_servidor_dita(",
            "recusa_de_midia_dita(",
            "imagem_lida_dita(",
        ];
        let fonte = super::fonte_sem_comentarios();
        for (comando, chamadas_minimas) in [
            ("fn registrar_da_janela(", 1),
            ("fn midia_do_mod(", 2),
            ("fn som_do_mod(", 2),
            ("fn midia_em_bytes(", 2),
            ("async fn ler_imagem_mod(", 2),
        ] {
            let codigo: String = super::corpo_de(&fonte, comando)
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            let mut chamadas = 0;
            for porta in PORTAS {
                for depois in codigo.split(porta).skip(1) {
                    chamadas += 1;
                    assert!(
                        depois.starts_with("&session.baldes_do_registro,"),
                        "`{comando}` chama `{porta}` sem `&session.baldes_do_registro`: com \
                         outros baldes, a porta perde o teto de vazão da sessão"
                    );
                }
            }
            assert!(
                chamadas >= chamadas_minimas,
                "`{comando}` chama {chamadas} das portas medidas, e chamava \
                 {chamadas_minimas}: a linha dele saiu por fora delas, e do teto"
            );
        }
    }

    /// **A linha que passa pelo transbordo diz que passou.** A conta que ela
    /// leva é do balde dividido — de todos os ids que caíram nele —, e sem o
    /// campo ela se lia como sendo só do id da linha.
    #[test]
    fn a_linha_que_passa_pelo_transbordo_diz_que_passou() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for n in 0..BALDES_NO_REGISTRO - Classe::TODAS.len() {
                da_janela(&baldes, agora, &format!("cheio/{n}"));
            }
            da_janela(&baldes, agora, "novo/x");
            da_janela(&baldes, agora, "cheio/0");
        });
        assert!(
            de(&rastro, "novo/x")
                .iter()
                .all(|linha| linha.contains("transbordo=true"))
                && !de(&rastro, "novo/x").is_empty(),
            "a linha que passou pelo balde dividido não diz `transbordo=true`, e a conta dela se \
             lê como sendo só do id dela: {rastro}"
        );
        assert!(
            de(&rastro, "cheio/0")
                .iter()
                .all(|linha| !linha.contains("transbordo")),
            "uma linha do balde próprio do MOD saiu como transbordo: {rastro}"
        );
    }

    /// **Um id que o filtro esvazia é ilegível, e não o catálogo**: o balde e
    /// o rótulo dele são os de `mod_id_ilegivel`, como o terceiro braço do
    /// pedido de MOD. No balde do catálogo, ele gastava as fichas do catálogo
    /// e a conta dele saía como `catalogo=true`.
    #[test]
    fn um_id_que_o_filtro_esvazia_conta_como_ilegivel_e_nao_como_catalogo() {
        let sessao = Session::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..RAJADA_DO_CONSOLE as usize + 2 {
                da_janela(&sessao.baldes_do_registro, agora, "\u{202E}\n");
            }
            registrar_da_janela_em(
                &sessao.baldes_do_registro,
                agora,
                "aviso".to_owned(),
                "pedido-de-mod".to_owned(),
                "o catálogo".to_owned(),
                None,
            );
            sessao.revogar();
        });
        assert!(
            rastro.contains("o catálogo"),
            "a linha do catálogo foi segurada pelas linhas de um id ilegível: {rastro}"
        );
        let contas: Vec<&str> = rastro
            .lines()
            .filter(|linha| linha.contains(super::CONTA_DO_BALDE_DO_REGISTRO))
            .collect();
        assert!(
            contas.len() == 1
                && contas
                    .iter()
                    .all(|conta| conta.contains("mod_id_ilegivel=true")
                        && conta.contains("suprimidas=2")
                        && !conta.contains("catalogo")),
            "a conta de um id que o filtro esvazia não saiu como `mod_id_ilegivel=true`: o \
             rótulo `catalogo=true` é de um pedido que não é de MOD nenhum: {rastro}"
        );
    }

    /// **A saída do app diz a conta que nenhuma linha levou.**
    ///
    /// Fechar a janela não passava por `Session::revogar`, e a conta pendente de
    /// um balde caía com o processo: o `seele.log` da última sessão mostrava a
    /// rajada como se fosse tudo. `despedir_da_sessao` revoga primeiro, e
    /// revogar diz as contas.
    #[test]
    fn despedir_da_sessao_diz_a_conta_que_nenhuma_linha_levou() {
        let sessao = Session::default();
        let agora = origem();
        let a_mais = 5;
        let ((), rastro) = capturar(|| {
            for _ in 0..RAJADA_DO_CONSOLE as usize + a_mais {
                da_janela(&sessao.baldes_do_registro, agora, "a/b");
            }
            super::despedir_da_sessao(&sessao, Duration::from_millis(20));
        });
        assert!(
            rastro
                .lines()
                .any(|linha| linha.contains(super::CONTA_DO_BALDE_DO_REGISTRO)
                    && linha.contains("mod_id=a/b")
                    && linha.contains(&format!("suprimidas={a_mais}"))),
            "a saída do app não disse as {a_mais} linhas seguradas do balde de `a/b`: a conta \
             caiu com o processo, e o seele.log mostra a rajada como se fosse tudo: {rastro}"
        );
    }

    /// **A conta também é dita sem fechar a sessão**, por
    /// `dizer_as_contas_do_registro`. Ela diz uma vez e esvazia os baldes: chamada de novo, não diz nada, e a
    /// geração não anda.
    #[test]
    fn dizer_as_contas_do_registro_diz_uma_vez_e_esvazia() {
        let sessao = Session::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..RAJADA_DO_CONSOLE as usize + 3 {
                da_janela(&sessao.baldes_do_registro, agora, "a/b");
            }
            sessao.dizer_as_contas_do_registro();
            sessao.dizer_as_contas_do_registro();
        });
        let contas: Vec<&str> = rastro
            .lines()
            .filter(|linha| linha.contains(super::CONTA_DO_BALDE_DO_REGISTRO))
            .collect();
        assert!(
            contas.len() == 1 && contas.iter().all(|conta| conta.contains("suprimidas=3")),
            "as contas não foram ditas uma vez, com as três linhas seguradas: a saída do app \
             perde a conta, ou a diz duas vezes e o registro as soma: {rastro}"
        );
        assert!(
            sessao
                .baldes_do_registro
                .lock()
                .expect("os baldes")
                .is_empty(),
            "dizer as contas não esvaziou os baldes"
        );
        assert_eq!(
            sessao.geracao(),
            0,
            "dizer as contas do registro fechou a sessão: ela só diz as contas, e quem revoga \
             é `revogar`"
        );
    }

    /// **A conta é dita com o cadeado dos baldes solto.**
    ///
    /// Escrita com ele na mão, uma linha que demora no disco segura toda porta
    /// do registro de todo MOD, em toda thread, até terminar — e um assinante
    /// que escrevesse pelo registro dentro do evento travaria no mesmo
    /// cadeado. A camada abaixo tenta o cadeado a cada evento, na thread que
    /// escreve: com a guarda na mão, o `try_lock` volta `WouldBlock`.
    #[test]
    fn a_conta_e_dita_com_o_cadeado_dos_baldes_solto() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tracing_subscriber::layer::SubscriberExt as _;

        /// Conta os eventos que acharam o cadeado livre e os que o acharam
        /// preso.
        struct TentaOCadeado {
            baldes: &'static BaldesDoRegistro,
            livre: Arc<AtomicUsize>,
            preso: Arc<AtomicUsize>,
        }

        /// A mensagem do evento é a da conta dos baldes?
        struct EAConta(bool);

        impl tracing::field::Visit for EAConta {
            fn record_debug(&mut self, campo: &tracing::field::Field, valor: &dyn std::fmt::Debug) {
                if campo.name() == "message"
                    && format!("{valor:?}") == super::CONTA_DO_BALDE_DO_REGISTRO
                {
                    self.0 = true;
                }
            }
        }

        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for TentaOCadeado {
            fn on_event(
                &self,
                evento: &tracing::Event<'_>,
                _contexto: tracing_subscriber::layer::Context<'_, S>,
            ) {
                // Só a linha da conta: qualquer outra que o `revogar` passe a
                // escrever não pode fazer as vezes dela.
                let mut e_a_conta = EAConta(false);
                evento.record(&mut e_a_conta);
                if !e_a_conta.0 {
                    return;
                }
                let contador = if self.baldes.try_lock().is_ok() {
                    &self.livre
                } else {
                    &self.preso
                };
                contador.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Como em `capturar`: com um segundo despachante vivo, o interesse da
        // linha da conta não fica preso ao de outra thread que passou por ela
        // antes, sem assinante.
        crate::rastro_de_teste::segundo_despachante();
        // `'static`, porque o assinante é: a sessão de um teste só.
        let sessao: &'static Session = Box::leak(Box::default());
        let agora = origem();
        for _ in 0..RAJADA_DO_CONSOLE as usize + 1 {
            da_janela(&sessao.baldes_do_registro, agora, "a/b");
        }
        let livre = Arc::new(AtomicUsize::new(0));
        let preso = Arc::new(AtomicUsize::new(0));
        let assinante = tracing_subscriber::registry().with(TentaOCadeado {
            baldes: &sessao.baldes_do_registro,
            livre: Arc::clone(&livre),
            preso: Arc::clone(&preso),
        });
        tracing::subscriber::with_default(assinante, || {
            sessao.revogar();
        });
        assert_eq!(
            preso.load(Ordering::Relaxed),
            0,
            "a conta dos baldes foi escrita com o cadeado deles na mão: uma linha lenta no disco \
             segura o registro de todo MOD até terminar"
        );
        assert_eq!(
            livre.load(Ordering::Relaxed),
            1,
            "a sessão fechou com um balde de linhas seguradas e não disse a conta dele uma vez"
        );
    }

    /// **A recusa de mídia tem o mesmo teto**, e a mesma conta.
    #[test]
    fn a_recusa_de_midia_tem_o_mesmo_teto_e_a_mesma_conta() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..200 {
                recusa(&baldes, agora, "a/b");
            }
            recusa(&baldes, agora + UMA_FICHA, "a/b");
        });
        let linhas = de(&rastro, "a/b");
        assert_eq!(
            linhas.len(),
            RAJADA_DO_CONSOLE as usize + 1,
            "duzentas recusas de mídia do mesmo MOD no mesmo instante passaram de outro jeito \
             que a rajada, e a seguinte um quarto de segundo depois: {rastro}"
        );
        let seguradas = 200 - RAJADA_DO_CONSOLE as usize;
        assert!(
            linhas
                .last()
                .is_some_and(|linha| linha.contains(&format!("suprimidas={seguradas}"))),
            "a recusa que passou depois do aperto não diz as {seguradas} que o balde segurou: \
             {rastro}"
        );
    }

    /// **Dentro de cada classe, as portas do mesmo MOD dividem o balde dele**:
    /// é um teto por MOD e por classe, e não um por porta — senão um MOD teria
    /// uma rajada por porta. Os avisos são a janela em aviso e a mídia
    /// recusada; as servidas, a mídia e o som; a depuração, a janela em DEBUG e
    /// a mídia não servida porque a sessão acabou.
    #[test]
    fn as_portas_do_mesmo_mod_dividem_o_balde() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let por_porta = RAJADA_DO_CONSOLE as usize / 2;
        let ((), rastro) = capturar(|| {
            for _ in 0..=por_porta {
                da_janela(&baldes, agora, "a/b");
                recusa(&baldes, agora, "a/b");
                midia_servida_dita(&baldes, agora, "a/b", 7, "img/rosto.png", "imagem", 8);
                som_servido_dito(&baldes, agora, "a/b", 7, "som/toque.wav", 16);
                da_janela_no(&baldes, agora, "depuracao", "a/b");
                nao_servida(&baldes, agora, "a/b");
            }
        });
        let linhas = de(&rastro, "a/b");
        for (classe, nivel) in [
            ("aviso", "WARN"),
            ("servida", "INFO"),
            ("depuração", "DEBUG"),
        ] {
            let da_classe = linhas
                .iter()
                .filter(|linha| linha.split_whitespace().next() == Some(nivel))
                .count();
            assert_eq!(
                da_classe, RAJADA_DO_CONSOLE as usize,
                "as duas portas da classe {classe} do mesmo MOD não deixaram uma rajada só: ou \
                 cada porta tem a dela, e o teto de um MOD é o dobro do escrito, ou outra classe \
                 gastou as fichas desta: {rastro}"
            );
        }
    }

    /// **Um aviso não é segurado pelas linhas servidas do mesmo MOD.**
    ///
    /// Segurada, uma linha não atrasa: ela some, e só a conta segue, na
    /// próxima linha do MOD — que pode ser outra «mídia de MOD servida». Um MOD
    /// que monta mais de 32 mídias de uma vez gastava a rajada em INFO, e a
    /// recusa da seguinte sumia: o defeito de 23/09, que o módulo da mídia
    /// recusada conta, de volta pelo teto.
    #[test]
    fn uma_recusa_depois_de_quarenta_servidas_do_mesmo_mod_sai() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for n in 0..40 {
                midia_servida_dita(
                    &baldes,
                    agora,
                    "a/b",
                    7,
                    &format!("img/{n}.png"),
                    "imagem",
                    8,
                );
            }
            recusa(&baldes, agora, "a/b");
        });
        let recusada = rastro
            .lines()
            .find(|linha| linha.contains("mídia de MOD recusada"));
        assert!(
            recusada.is_some_and(|linha| linha.contains("mod_id=a/b")),
            "quarenta mídias servidas do mesmo MOD gastaram a ficha da recusa, e ela sumiu do \
             seele.log — a mídia recusada em silêncio de novo: {rastro}"
        );
        assert!(
            recusada.is_some_and(|linha| !linha.contains("suprimidas")),
            "a recusa levou na conta as mídias servidas que o balde delas segurou, e quem lê \
             acha que avisos sumiram: {rastro}"
        );
    }

    /// **A depuração não gasta a ficha de um aviso, nem infla a conta dele.**
    /// O DEBUG não está no `seele.log` com o filtro padrão: segurar um aviso
    /// por causa dele era calar o que se lê pelo que não se lê.
    #[test]
    fn a_depuracao_nao_gasta_a_ficha_nem_infla_a_conta_de_um_aviso() {
        let baldes = BaldesDoRegistro::default();
        let agora = origem();
        let ((), rastro) = capturar(|| {
            for _ in 0..40 {
                da_janela_no(&baldes, agora, "depuracao", "a/b");
            }
            da_janela(&baldes, agora, "a/b");
        });
        let aviso = rastro
            .lines()
            .find(|linha| linha.split_whitespace().next() == Some("WARN"));
        assert!(
            aviso
                .is_some_and(|linha| linha.contains("mod_id=a/b") && !linha.contains("suprimidas")),
            "quarenta linhas DEBUG da janela seguraram o aviso do mesmo MOD, ou entraram na \
             conta dele: {rastro}"
        );
    }
}

/// **O id de um MOD que chega da janela sai escapado em toda linha do
/// registro**, e não só nas recusas.
///
/// Um campo em `%` sai cru no `seele.log`: o formatador escapa só a mensagem,
/// e só ANSI. As duas linhas de `mod_request` e as de `midia_em_bytes` e de
/// `escolher_para_o_mod` escreviam o id que o comando recebe da janela como
/// veio — e um `\n` nele escrevia uma segunda linha com a cara do produto.
#[cfg(test)]
mod o_id_do_pedido_de_mod_no_registro {
    use super::{pedido_a_caminho_dito, pedido_recusado_na_saida_dito, ConnectionError};
    use crate::rastro_de_teste::capturar;

    /// As duas linhas de `mod_request`, cada uma com o rastro dela.
    fn as_duas_linhas(id: &str) -> [(&'static str, String); 2] {
        let ((), a_caminho) = capturar(|| pedido_a_caminho_dito(id, 7, 3, 1));
        let ((), recusado) = capturar(|| {
            pedido_recusado_na_saida_dito(id, 3, &ConnectionError::NotConnected);
        });
        [
            ("pedido_a_caminho_dito", a_caminho),
            ("pedido_recusado_na_saida_dito", recusado),
        ]
    }

    #[test]
    fn uma_quebra_de_linha_no_id_nao_forja_outra_linha() {
        for (linha, rastro) in as_duas_linhas("a/b\nWARN seele_app: forjada") {
            assert_eq!(
                rastro.lines().count(),
                1,
                "o id que a janela manda escreveu, por `{linha}`, uma segunda linha no seele.log \
                 com a cara do produto: {rastro}"
            );
            assert!(
                rastro.contains("mod_id=a/bWARN seele_app: forjada"),
                "`{linha}` perdeu o id, ou o escreveu de outro jeito que as recusas \
                 (`id_no_registro`): {rastro}"
            );
        }
    }

    /// **O id vazio é o catálogo**, como na metade servidor (`executar`, em
    /// `seele-server/src/mods/pedidos.rs`): `catalogo=true`, e nenhum
    /// `mod_id=` em branco que uma busca por `mod_id=` ache.
    #[test]
    fn o_id_vazio_e_o_catalogo_e_nao_um_mod_id_em_branco() {
        for (linha, rastro) in as_duas_linhas("") {
            assert!(
                rastro.contains("catalogo=true") && !rastro.contains("mod_id"),
                "`{linha}` escreveu o pedido do catálogo, que não é de MOD nenhum, sem \
                 `catalogo=true` ou com um `mod_id=` em branco: {rastro}"
            );
        }
    }

    /// **Um id que o filtro esvazia é ilegível**, e não um `mod_id=` em
    /// branco: o terceiro braço da metade servidor.
    #[test]
    fn um_id_que_o_filtro_esvazia_e_ilegivel() {
        for (linha, rastro) in as_duas_linhas("\n\u{202E} ") {
            assert!(
                rastro.contains("mod_id_ilegivel=true") && !rastro.contains("mod_id="),
                "`{linha}` escreveu um id que o filtro esvazia como um `mod_id=` em branco, e \
                 não como ilegível: {rastro}"
            );
        }
    }

    /// **A mídia vinda do servidor também**: a linha saiu de `midia_em_bytes`
    /// para uma função medida, e o id que a janela manda continua passando por
    /// `id_no_registro`.
    #[test]
    fn a_midia_vinda_do_servidor_nao_forja_linha() {
        let ((), rastro) = capturar(|| {
            super::midia_vinda_do_servidor_dita(
                &super::BaldesDoRegistro::default(),
                crate::relogio_de_teste::origem(),
                "a/b\nWARN seele_app: forjada",
                7,
                "imagem",
                8,
            );
        });
        assert!(
            rastro.lines().count() == 1 && rastro.contains("mod_id=a/bWARN seele_app: forjada"),
            "o id que a janela manda escreveu, pela mídia vinda do servidor, uma segunda linha \
             no seele.log, ou saiu de outro jeito que as recusas: {rastro}"
        );
    }

    #[test]
    fn um_id_de_verdade_sai_como_mod_id() {
        for (linha, rastro) in as_duas_linhas("seele/perfis") {
            assert!(
                rastro
                    .split_whitespace()
                    .any(|pedaco| pedaco == "mod_id=seele/perfis"),
                "`{linha}` deixou de escrever `mod_id=seele/perfis`, que é o que o guia manda \
                 procurar: {rastro}"
            );
        }
    }

    /// O corpo de `funcao` neste arquivo, sem as linhas de comentário
    /// ([`super::fonte_sem_comentarios`]) e sem espaço nenhum.
    fn corpo_sem_espaco(fonte: &str, funcao: &str) -> String {
        super::corpo_de(fonte, funcao)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }

    /// **Nenhum `mod_id = %id` usa o id cru do comando**, nas três portas que
    /// o escreviam assim e nas duas funções de `mod_request`. Os comandos não
    /// sobem num teste — precisam de conexão, de `AppHandle` ou do seletor do
    /// sistema —, e uma linha escrita à mão dentro de um deles deixaria os
    /// testes acima verdes com o id cru no registro de novo.
    #[test]
    fn nenhum_mod_id_em_display_usa_o_id_cru_do_comando() {
        let fonte = super::fonte_sem_comentarios();
        // A função e o nome que o id cru tem nela.
        for (funcao, cru) in [
            ("fn mod_request(", "id"),
            ("fn pedido_a_caminho_dito(", "id"),
            ("fn pedido_recusado_na_saida_dito(", "id"),
            ("fn midia_em_bytes(", "id"),
            ("fn midia_vinda_do_servidor_dita(", "mod_id"),
            ("async fn escolher_para_o_mod(", "id"),
        ] {
            let codigo = corpo_sem_espaco(&fonte, funcao);
            for depois in codigo.split("mod_id=%").skip(1) {
                let valor: String = depois
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                assert_ne!(
                    valor, cru,
                    "`{funcao}` escreve `mod_id = %{cru}` com o id cru que a janela manda: em \
                     `%` ele sai sem escape, e uma quebra de linha nele escreve uma segunda \
                     linha no seele.log com a cara do produto"
                );
            }
        }
        let pedido = corpo_sem_espaco(&fonte, "fn mod_request(");
        for funcao in ["pedido_a_caminho_dito(", "pedido_recusado_na_saida_dito("] {
            assert!(
                pedido.contains(funcao),
                "`fn mod_request(` deixou de passar por `{funcao}`, e a linha escrita à mão no \
                 comando escapa dos testes acima"
            );
        }
    }
}

/// **Um pacote de MOD publicado numa pasta de teste**, como o instalador o
/// deixa.
///
/// Escrito numa pasta de obras, lido para saber o hash do conteúdo, e movido
/// para `PACOTES/<hash>` — que é onde `ler_por_hash` e `mods::serve` o
/// procuram. Os bytes de cada formato são os menores que o `sniff` reconhece.
#[cfg(test)]
mod pacote_de_teste {
    /// O menor WAV que o `sniff` aceita (`seele-proto/src/midia_de_mod.rs:73`).
    pub(crate) const WAV: &[u8] = b"RIFF\0\0\0\0WAVEfmt ";
    /// A assinatura de um PNG.
    pub(crate) const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    /// Texto, que nenhum decodificador deste produto aceita.
    pub(crate) const TEXTO: &[u8] = b"so texto";
    /// O id do MOD de todo pacote daqui.
    pub(crate) const ID: &str = "prova/midia";

    /// Publica um pacote que declara `arquivos`, e devolve a pasta e o hash.
    ///
    /// A pasta é um `TempDir`: some quando o teste termina.
    pub(crate) fn publicar(arquivos: &[(&str, &[u8])]) -> (tempfile::TempDir, String) {
        let raiz = tempfile::tempdir().expect("a pasta de teste");
        let obras = raiz.path().join("em-obras");
        std::fs::create_dir_all(obras.join("cliente")).expect("a pasta de obras");
        let declarados: Vec<String> = arquivos
            .iter()
            .map(|(caminho, _)| format!("\"{caminho}\""))
            .collect();
        let api = seele_ffi::mods::MOD_API_VERSION;
        std::fs::write(
            obras.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"{ID}","version":"1.0.0","api":{api},"repo":"https://example.invalid/x","reach":["dom"],"client":"cliente/main.js","arquivos":[{}]}}"#,
                declarados.join(",")
            ),
        )
        .expect("o manifesto");
        std::fs::write(obras.join("cliente/main.js"), "/* nada */").expect("o cliente");
        for (caminho, bytes) in arquivos {
            let destino = obras.join(caminho);
            std::fs::create_dir_all(destino.parent().expect("a pasta do arquivo"))
                .expect("a pasta do arquivo");
            std::fs::write(&destino, bytes).expect("o arquivo");
        }
        let hash = seele_ffi::mods::ler_pasta(&obras.to_string_lossy())
            .expect("o pacote de teste é válido")
            .hash;
        let publicado = raiz.path().join(seele_ffi::mods::PACOTES).join(&hash);
        std::fs::create_dir_all(publicado.parent().expect("a pasta dos pacotes"))
            .expect("a pasta dos pacotes");
        std::fs::rename(&obras, &publicado).expect("publicar o pacote");
        (raiz, hash)
    }

    /// A pasta de configuração, como `config_dir` a devolveria.
    pub(crate) fn pasta(raiz: &tempfile::TempDir) -> String {
        raiz.path().to_string_lossy().into_owned()
    }
}

/// **A mídia de MOD recusada pelo Rust é dita no `seele.log`.**
///
/// Em 23/09, descobrir que o avatar do PERFIS carregava levou uma hora de
/// medição e um reinício com `RUST_LOG=debug`: o Rust recusava e devolvia a
/// recusa à janela, e o registro não tinha uma palavra.
#[cfg(test)]
mod a_midia_recusada_e_dita_no_registro {
    use super::{
        imagem_lida_dita, midia_declarada_do_mod, midia_do_servidor_dita, midia_servida_dita,
        motivo_de, recusa_de_midia_dita, registrar_da_janela_em, BaldesDoRegistro, FalhaNoMod,
        TETO_DA_FRASE_NO_REGISTRO, TETO_DO_ID_NO_REGISTRO,
    };
    use crate::pacote_de_teste::{pasta, publicar, ID, PNG, TEXTO};
    use crate::rastro_de_teste::capturar;
    use crate::relogio_de_teste::origem;

    /// A linha de recusa de mídia no rastro, se houver.
    fn recusa(rastro: &str) -> Option<&str> {
        rastro
            .lines()
            .find(|linha| linha.contains("mídia de MOD recusada"))
    }

    /// Confere os pedaços que fazem uma linha de recusa servir a quem lê.
    fn traz(linha: &str, pedacos: &[&str]) {
        for pedaco in pedacos {
            assert!(
                linha.contains(pedaco),
                "a linha de recusa não traz `{pedaco}`, e é por ela que quem lê o registro \
                 acha o MOD e o motivo: {linha}"
            );
        }
    }

    #[test]
    fn um_arquivo_que_o_manifesto_nao_declara_sai_como_warn_com_o_id_e_o_motivo() {
        let (raiz, hash) = publicar(&[("img/rosto.png", PNG)]);
        let (resultado, rastro) = capturar(|| {
            midia_declarada_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                "img/escondido.png",
            )
        });
        let falha = resultado
            .err()
            .expect("um arquivo que o manifesto não declara foi servido");
        assert_eq!(
            motivo_de(&falha),
            "arquivo-nao-declarado",
            "a recusa mudou de nome, e a frase da janela deixa de achá-la"
        );
        let linha = recusa(&rastro).unwrap_or_else(|| {
            panic!(
                "a recusa não chegou ao registro: o MOD fica sem imagem e o seele.log sem \
                 uma palavra — {rastro}"
            )
        });
        traz(
            linha,
            &[
                "WARN",
                "mod_id=prova/midia",
                "motivo=arquivo-nao-declarado",
                "origem=\"img/escondido.png\"",
            ],
        );
    }

    #[test]
    fn um_arquivo_que_nao_e_midia_sai_como_warn_com_o_motivo() {
        let (raiz, hash) = publicar(&[("doc/leia.txt", TEXTO)]);
        let (resultado, rastro) = capturar(|| {
            midia_declarada_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                "doc/leia.txt",
            )
        });
        let falha = resultado.err().expect("texto foi servido como mídia");
        assert_eq!(
            motivo_de(&falha),
            "formato-desconhecido",
            "texto foi recusado por outro motivo, e o autor procuraria no lugar errado"
        );
        let linha =
            recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
        traz(
            linha,
            &["WARN", "mod_id=prova/midia", "motivo=formato-desconhecido"],
        );
    }

    #[test]
    fn uma_imagem_aceita_nao_escreve_recusa() {
        let (raiz, hash) = publicar(&[("img/rosto.png", PNG)]);
        let (resultado, rastro) = capturar(|| {
            midia_declarada_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                "img/rosto.png",
            )
        });
        let midia = resultado.expect("uma imagem declarada foi recusada");
        assert_eq!(midia.papel, "imagem", "um PNG saiu com outro papel");
        assert!(
            recusa(&rastro).is_none(),
            "uma mídia aceita escreveu uma recusa, e o registro ensinaria a ignorá-las: {rastro}"
        );
    }

    /// **O nome da mídia servida não escreve uma segunda linha no registro.**
    ///
    /// O mesmo vetor da linha do som servido: o manifesto só confere que cada
    /// pedaço do caminho é um nome (`inner_path`), e uma quebra de linha passa.
    /// O caminho vem em texto, e não num pacote no disco, porque um nome de
    /// arquivo com `\n` não existe no Windows.
    #[test]
    fn um_caminho_com_quebra_de_linha_nao_forja_linha_na_midia_servida() {
        let ((), rastro) = capturar(|| {
            midia_servida_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                7,
                "img/a\nWARN seele_app: forjada.png",
                "imagem",
                8,
            );
        });
        assert_eq!(
            rastro.lines().count(),
            1,
            "o nome que o MOD declarou escreveu uma segunda linha no seele.log, com a cara do \
             produto: {rastro}"
        );
        for pedaco in [
            "INFO",
            "mídia de MOD servida",
            "mod_id=prova/midia",
            r#"caminho="img/a\nWARN seele_app: forjada.png""#,
            "papel=\"imagem\"",
        ] {
            assert!(
                rastro.contains(pedaco),
                "a linha da mídia servida não traz `{pedaco}`, e quem lê o registro já não acha \
                 o que o Rust serviu: {rastro}"
            );
        }
    }

    /// **O nome da mídia servida tem o teto das outras portas de texto de
    /// terceiro.** As aspas impedem a linha forjada, e não a linha sem fim.
    #[test]
    fn um_caminho_enorme_sai_cortado_no_teto_de_frase_na_midia_servida() {
        let ((), rastro) = capturar(|| {
            midia_servida_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                7,
                &"i".repeat(TETO_DA_FRASE_NO_REGISTRO * 4),
                "imagem",
                8,
            );
        });
        assert!(
            rastro.contains(&format!(
                "caminho=\"{}\"",
                "i".repeat(TETO_DA_FRASE_NO_REGISTRO)
            )),
            "o nome que o MOD declarou entrou no registro sem o teto das outras portas de \
             texto de terceiro: {} caracteres numa linha",
            rastro.chars().count()
        );
    }

    #[test]
    fn a_sessao_que_acabou_nao_vira_aviso() {
        let (falha, rastro) = capturar(|| {
            recusa_de_midia_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                "img/rosto.png",
                FalhaNoMod::Recusado {
                    motivo: "sessao-encerrada".to_owned(),
                },
            )
        });
        assert_eq!(
            motivo_de(&falha),
            "sessao-encerrada",
            "a recusa mudou no caminho"
        );
        assert!(
            !rastro.contains("WARN") && rastro.contains("DEBUG"),
            "uma mídia que não montou porque a pessoa saiu virou aviso — é o desfecho \
             certo, e não um defeito: {rastro}"
        );
    }

    #[test]
    fn a_midia_do_servidor_recusada_leva_o_id_do_mod() {
        let (resultado, rastro) = capturar(|| {
            midia_do_servidor_dita(
                &BaldesDoRegistro::default(),
                origem(),
                "seele/perfis",
                "data:image/png;base64,%%%",
                None,
            )
        });
        let falha = resultado.err().expect("base64 inválido virou mídia");
        assert_eq!(
            motivo_de(&falha),
            "nao-e-base64",
            "o base64 inválido foi recusado por outro motivo"
        );
        let linha =
            recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
        traz(
            linha,
            &[
                "WARN",
                "mod_id=seele/perfis",
                "motivo=nao-e-base64",
                "origem=\"servidor\"",
            ],
        );
    }

    /// Uma recusa de imagem por volume, com o motivo que o servidor mandou.
    ///
    /// É o caminho de `ler_imagem_mod`, pela mesma função que o comando chama
    /// ([`imagem_lida_dita`]): o `Err` da conexão é o texto do par remoto (o
    /// `erro` da `RespostaDeImagem`, a razão com que a conexão fechou, o prazo
    /// esgotado), e vai à recusa como veio. Chamar `recusa_de_midia_dita`
    /// direto aqui mediria uma cópia do caminho, e os testes que passam por
    /// este ajudante ficariam verdes com o comando calado.
    fn recusa_do_servidor(motivo: &str) -> (FalhaNoMod, String) {
        let (lida, rastro) = capturar(|| {
            imagem_lida_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                Err(motivo.to_owned()),
                None,
            )
        });
        (
            lida.err()
                .expect("a recusa do servidor virou imagem, e não há recusa a medir"),
            rastro,
        )
    }

    #[test]
    fn o_texto_do_servidor_no_motivo_nao_forja_linha() {
        let forjado = "x\n2026-09-30T00:00:00.000000Z  WARN seele_app: linha forjada pelo servidor";
        let (falha, rastro) = recusa_do_servidor(forjado);
        assert_eq!(
            motivo_de(&falha),
            forjado,
            "a recusa que volta à janela foi mexida: quem se protege é a linha do registro, \
             e a janela continua recebendo o que o servidor disse"
        );
        assert_eq!(
            rastro.lines().count(),
            1,
            "texto do servidor no motivo escreveu no seele.log mais de uma linha, e a \
             segunda tem a cara do produto: {rastro}"
        );
        assert!(
            rastro.contains("linha forjada pelo servidor"),
            "o que o servidor disse sumiu do registro, e era o dado da recusa: {rastro}"
        );
    }

    #[test]
    fn o_id_que_a_janela_manda_nao_forja_linha() {
        let (_, rastro) = capturar(|| {
            recusa_de_midia_dita(
                &BaldesDoRegistro::default(),
                origem(),
                "prova/midia\n2026-09-30T00:00:00.000000Z  WARN seele_app: linha forjada pela janela",
                "servidor",
                FalhaNoMod::Recusado {
                    motivo: "nao-e-base64".to_owned(),
                },
            )
        });
        assert_eq!(
            rastro.lines().count(),
            1,
            "o id do MOD que a janela manda escreveu no seele.log mais de uma linha, e a \
             segunda tem a cara do produto: {rastro}"
        );
    }

    #[test]
    fn o_motivo_em_texto_livre_sai_entre_aspas() {
        let prazo = "Prazo da transferência esgotado. Confira a conexão e a versão do servidor.";
        let (_, rastro) = recusa_do_servidor(prazo);
        let linha =
            recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
        traz(
            linha,
            &["WARN", "mod_id=prova/midia", &format!("motivo=\"{prazo}\"")],
        );
    }

    /// **A imagem que o servidor recusou é dita**, com o id e o texto dele.
    ///
    /// É a primeira das duas recusas de `ler_imagem_mod` depois do `await`, e
    /// a que vem com o que o servidor mandou — o prazo esgotado, o `erro` da
    /// resposta, a razão com que a conexão fechou.
    #[test]
    fn a_imagem_recusada_pelo_servidor_sai_como_warn_com_o_id_e_o_motivo() {
        let (resultado, rastro) = capturar(|| {
            imagem_lida_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                Err("Prazo esgotado".to_owned()),
                None,
            )
        });
        let falha = resultado.err().expect("a recusa do servidor virou imagem");
        assert_eq!(
            motivo_de(&falha),
            "Prazo esgotado",
            "a recusa do servidor voltou à janela mexida, e a janela deixa de dizer o que ele \
             disse"
        );
        let linha = recusa(&rastro).unwrap_or_else(|| {
            panic!(
                "a imagem que o servidor recusou não chegou ao registro: o MOD fica sem imagem \
                 e o seele.log sem uma palavra — {rastro}"
            )
        });
        traz(
            linha,
            &[
                "WARN",
                "mod_id=prova/midia",
                "motivo=\"Prazo esgotado\"",
                "origem=\"volume\"",
            ],
        );
    }

    /// **A imagem lida que não é mídia é dita**, pelo nome da recusa.
    ///
    /// A segunda recusa de `ler_imagem_mod` depois do `await`: o servidor
    /// entregou, e os bytes não são de um formato que este produto decodifica.
    #[test]
    fn a_imagem_lida_que_nao_e_midia_sai_como_warn_com_o_motivo() {
        let (resultado, rastro) = capturar(|| {
            imagem_lida_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                Ok(TEXTO.to_vec()),
                None,
            )
        });
        let falha = resultado.err().expect("texto lido por volume virou imagem");
        assert_eq!(
            motivo_de(&falha),
            "formato-desconhecido",
            "texto lido por volume foi recusado por outro motivo, e o autor procuraria no lugar \
             errado"
        );
        let linha = recusa(&rastro).unwrap_or_else(|| {
            panic!(
                "a imagem lida que não é mídia não chegou ao registro: o MOD fica sem imagem e \
                 o seele.log sem uma palavra — {rastro}"
            )
        });
        traz(
            linha,
            &[
                "WARN",
                "mod_id=prova/midia",
                "motivo=formato-desconhecido",
                "origem=\"volume\"",
            ],
        );
    }

    /// **A imagem lida que é mídia passa sem linha**: uma recusa só é dita
    /// quando há recusa.
    #[test]
    fn a_imagem_lida_aceita_nao_escreve_linha() {
        let (resultado, rastro) = capturar(|| {
            imagem_lida_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                Ok(PNG.to_vec()),
                None,
            )
        });
        let midia = resultado.expect("um PNG lido por volume foi recusado");
        assert_eq!(
            midia.papel, "imagem",
            "um PNG lido por volume saiu com outro papel"
        );
        assert!(
            rastro.is_empty(),
            "uma imagem lida e aceita escreveu no registro, e ele ensinaria a ignorar as \
             recusas: {rastro}"
        );
    }

    #[test]
    fn o_motivo_do_servidor_tem_teto() {
        let (_, rastro) = recusa_do_servidor(&"a".repeat(TETO_DA_FRASE_NO_REGISTRO * 4));
        let linha =
            recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
        assert!(
            linha.contains(&"a".repeat(TETO_DA_FRASE_NO_REGISTRO))
                && !linha.contains(&"a".repeat(TETO_DA_FRASE_NO_REGISTRO + 1)),
            "o motivo que o servidor manda entrou no registro sem o teto das outras portas \
             de texto de terceiro: {} caracteres numa linha",
            linha.chars().count()
        );
    }

    /// **A origem também tem teto.** É o caminho que a janela pede em
    /// `midia_do_mod` e em `som_do_mod`: texto que chega pela ponte, e que ia
    /// inteiro para a linha. As aspas impedem a linha forjada, e não a linha
    /// sem fim.
    #[test]
    fn uma_origem_enorme_sai_cortada_no_teto_de_frase() {
        let (_, rastro) = capturar(|| {
            recusa_de_midia_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                &"c".repeat(TETO_DA_FRASE_NO_REGISTRO * 4),
                FalhaNoMod::Recusado {
                    motivo: "arquivo-nao-declarado".to_owned(),
                },
            )
        });
        let linha =
            recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
        assert!(
            linha.contains(&format!(
                "origem=\"{}\"",
                "c".repeat(TETO_DA_FRASE_NO_REGISTRO)
            )),
            "o caminho que a janela pede entrou no registro sem o teto das outras portas de \
             texto de terceiro: {} caracteres numa linha",
            linha.chars().count()
        );
    }

    /// **O id do MOD sai igual pelas duas portas do registro.** A janela
    /// (`registrar_da_janela`) e a recusa de mídia do Rust escrevem o mesmo
    /// `mod_id=`, e é por ele que uma busca só junta o que as duas disseram do
    /// mesmo MOD. Com o tratamento escrito à mão em cada porta, um teto que
    /// mudasse numa e não na outra separaria as duas linhas sem que nada
    /// reclamasse.
    #[test]
    fn o_id_do_mod_sai_igual_pelas_duas_portas_do_registro() {
        let enorme = format!("prova/{}", "m".repeat(TETO_DO_ID_NO_REGISTRO * 4));
        let esperado: String = enorme.chars().take(TETO_DO_ID_NO_REGISTRO).collect();
        let ((), da_janela) = capturar(|| {
            registrar_da_janela_em(
                &BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                "recusa-de-mod".to_owned(),
                "uma frase da janela".to_owned(),
                Some(enorme.clone()),
            );
        });
        let (_, do_rust) = capturar(|| {
            recusa_de_midia_dita(
                &BaldesDoRegistro::default(),
                origem(),
                &enorme,
                "volume",
                FalhaNoMod::Recusado {
                    motivo: "nao-e-base64".to_owned(),
                },
            )
        });
        for (porta, rastro) in [
            ("registrar_da_janela", &da_janela),
            ("recusa_de_midia_dita", &do_rust),
        ] {
            let id = rastro
                .split("mod_id=")
                .nth(1)
                .and_then(|resto| resto.split_whitespace().next())
                .unwrap_or_else(|| panic!("`{porta}` não escreveu `mod_id=`: {rastro}"));
            assert_eq!(
                id, esperado,
                "`{porta}` escreveu o id do MOD com outro teto, e uma busca por `mod_id=` já \
                 não junta o que a janela e o Rust disseram do mesmo MOD"
            );
        }
    }

    /// **O pior caso sai com o mesmo teto pelas três portas que escapam.**
    ///
    /// `\u{100000}` é o caractere que mais cresce no escape: um escrito, dez
    /// no registro. O `console` do executor corta pelo tamanho escapado; o
    /// motivo e o caminho de uma mídia de MOD cortavam em 512 caracteres
    /// **antes** do escape, e quinhentos e doze deles viravam 5120 na linha —
    /// dez vezes o teto, nas portas que não têm teto de vazão. Com uma regra só,
    /// os campos saem do mesmo tamanho, e nenhum passa do teto.
    ///
    /// O `console` vai pelo caminho real: o executor corta, e a mesma
    /// `assentar_fala` da bomba escreve.
    ///
    /// **E a frase da janela**, desde que ela vai em `?`: cortada antes do
    /// escape, como era, quinhentos e doze `\u{100000}` eram 5120 caracteres
    /// na linha. Ela é cortada pela mesma regra do `console`, com o mesmo aviso.
    #[test]
    fn o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas() {
        use crate::executor::{ExecutorQuickJs, Fila, Limites, ParaOFora};
        let pior = "\u{100000}".repeat(TETO_DA_FRASE_NO_REGISTRO * 4);
        // O que vai entre as aspas de `nome="…"`, como a linha o leva.
        let campo = |rastro: &str, nome: &str| -> String {
            rastro
                .split(&format!("{nome}=\""))
                .nth(1)
                .and_then(|resto| resto.split('"').next())
                .unwrap_or_else(|| panic!("a linha não traz `{nome}=\"…\"`: {rastro}"))
                .to_owned()
        };

        let executor = ExecutorQuickJs::novo(Limites::default()).expect("o motor");
        executor
            .iniciar(r"console.log('\u{100000}'.repeat(2048));")
            .expect("o código");
        let fala = loop {
            match executor.receber(std::time::Duration::from_secs(5)) {
                Some(fala @ ParaOFora::Console { .. }) => break fala,
                Some(_) => {}
                None => panic!("a linha de console não saiu do motor"),
            }
        };
        let mods = std::sync::Mutex::new(super::ModsNativos::default());
        let fila = std::sync::Arc::new(Fila::default());
        let (_, do_console) =
            capturar(|| super::assentar_fala(&mods, &|| true, &fila, 1, fala, ID));
        let do_console = campo(&do_console, "texto");
        let do_console = do_console
            .strip_suffix(" […cortado: 2048 caracteres]")
            .unwrap_or_else(|| panic!("a linha de console não diz que foi cortada: {do_console}"));

        let (_, do_motivo) = recusa_do_servidor(&pior);
        let (_, da_origem) = capturar(|| {
            recusa_de_midia_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                &pior,
                FalhaNoMod::Recusado {
                    motivo: "arquivo-nao-declarado".to_owned(),
                },
            )
        });
        let ((), do_caminho) = capturar(|| {
            midia_servida_dita(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                7,
                &pior,
                "imagem",
                8,
            )
        });
        // A frase da janela é cortada pela mesma regra do `console`, com o
        // mesmo aviso: os dois são texto de terceiro que chega inteiro, e o
        // aviso diz o tamanho do que foi escrito.
        let ((), da_janela) = capturar(|| {
            registrar_da_janela_em(
                &BaldesDoRegistro::default(),
                origem(),
                "aviso".to_owned(),
                "recusa-de-mod".to_owned(),
                pior.clone(),
                Some(ID.to_owned()),
            );
        });
        let da_janela = campo(&da_janela, "frase");
        let da_janela = da_janela
            .strip_suffix(" […cortado: 2048 caracteres]")
            .unwrap_or_else(|| {
                panic!(
                    "a frase da janela não diz que foi cortada, e quem lê o registro toma o \
                     pedaço pelo todo: {da_janela}"
                )
            });

        let um_a_mais = r"\u{100000}".chars().count();
        for (porta, escrito) in [
            ("console", do_console.to_owned()),
            ("janela", da_janela.to_owned()),
            ("motivo", campo(&do_motivo, "motivo")),
            ("origem", campo(&da_origem, "origem")),
            ("caminho", campo(&do_caminho, "caminho")),
        ] {
            let tamanho = escrito.chars().count();
            assert!(
                tamanho <= TETO_DA_FRASE_NO_REGISTRO,
                "`{porta}` levou {tamanho} caracteres ao registro, e o teto de uma frase de \
                 terceiro é {TETO_DA_FRASE_NO_REGISTRO}: o corte foi contado antes do escape"
            );
            assert!(
                tamanho + um_a_mais > TETO_DA_FRASE_NO_REGISTRO,
                "`{porta}` cortou mais do que precisava ({tamanho} de \
                 {TETO_DA_FRASE_NO_REGISTRO}): cabia mais um, e o registro perde o que o \
                 terceiro escreveu sem razão"
            );
            assert_eq!(
                escrito, do_console,
                "`{porta}` cortou o pior caso noutro tamanho que o `console`, e as portas do \
                 mesmo `seele.log` voltam a ter tetos diferentes"
            );
        }
    }

    /// **Os quatro comandos de mídia passam pela recusa dita.**
    ///
    /// Os testes acima (e os do som) medem as funções; este prende que os
    /// comandos as chamam. `ler_imagem_mod` fala com a rede e não sobe num
    /// teste sem servidor — e um comando que devolvesse a recusa por fora seria
    /// a mídia recusada em silêncio de novo, com os testes verdes.
    ///
    /// Em `ler_imagem_mod` a chamada é `imagem_lida_dita`, que leva as duas
    /// recusas de depois do `await`. Procurar `recusa_de_midia_dita` ali
    /// passava com uma só delas: a outra saía calada, e o guarda verde. E a
    /// conexão ausente de `ler_imagem_mod` vai ao registro sem ser contada.
    ///
    /// O fonte é normalizado para `\n`: num checkout com CRLF o corte em
    /// `"\n}\n"` não acharia o fim da função, e o guarda passaria a ler o
    /// arquivo inteiro — inclusive este teste, que cita as quatro chamadas.
    #[test]
    fn os_quatro_comandos_de_midia_passam_pela_recusa_dita() {
        let fonte = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("main.rs legível")
        .replace("\r\n", "\n");
        for (comando, chamada) in [
            ("fn midia_do_mod(", "midia_declarada_do_mod("),
            ("fn midia_em_bytes(", "midia_do_servidor_dita("),
            ("async fn ler_imagem_mod(", "imagem_lida_dita("),
            ("fn som_do_mod(", "som_declarado_do_mod("),
        ] {
            let corpo = fonte
                .split(comando)
                .nth(1)
                .and_then(|resto| resto.split("\n}\n").next())
                .unwrap_or_else(|| panic!("`{comando}` sumiu de main.rs"));
            let codigo: Vec<&str> = corpo
                .lines()
                .filter(|linha| !linha.trim_start().starts_with("//"))
                .collect();
            assert!(
                codigo.join("\n").contains(chamada),
                "`{comando}` não passa por `{chamada}`: a recusa dele volta à janela e não \
                 chega ao registro"
            );
        }

        // **A conexão ausente de `ler_imagem_mod` é dita, e não contada.** A
        // geração já foi conferida na entrada: com ela de pé, a conexão que
        // falta não é comando de uma sessão encerrada. Os outros guardas não
        // veem a diferença — um `recusar_se_morta` a mais ali sobe a conta da
        // função e a do arquivo juntas.
        let ler: String = fonte
            .split("async fn ler_imagem_mod(")
            .nth(1)
            .and_then(|resto| resto.split("\n}\n").next())
            .expect("`async fn ler_imagem_mod(` sumiu de main.rs")
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .flat_map(str::chars)
            .filter(|c| !c.is_whitespace())
            .collect();
        let conexao = ler
            .split_once("session.connection()")
            .and_then(|(_, depois)| depois.split_once(")?;"))
            .map(|(conexao, _)| conexao)
            .expect(
                "a conexão de `ler_imagem_mod` não sai mais por `session.connection()…?;`, e \
                 este guarda já não acha a recusa dela",
            );
        for contar in [
            "comandos_de_geracao_morta",
            "confere_geracao(",
            "recusar_se_morta(",
        ] {
            assert!(
                !conexao.contains(contar),
                "a conexão ausente de `ler_imagem_mod` passou por `{contar}`: com a geração de \
                 pé, ela não é comando de uma sessão encerrada, e o contador diria que a sessão \
                 anterior falou sem ela ter falado"
            );
        }
        assert!(
            conexao.contains("recusa_de_midia_dita("),
            "a conexão ausente de `ler_imagem_mod` voltou à janela sem passar por \
             `recusa_de_midia_dita`, e não chega ao registro"
        );
        assert!(
            !ler.contains("comandos_de_geracao_morta"),
            "`ler_imagem_mod` mexe em `comandos_de_geracao_morta` por fora de \
             `recusar_se_morta`: uma conta à mão volta a conviver com a regra"
        );
    }
}

/// **O som de um MOD sai em bytes, conferido como a imagem é conferida.**
#[cfg(test)]
mod o_som_do_mod_sai_em_bytes {
    use super::{
        midia_declarada_do_mod, motivo_de, som_declarado_do_mod, som_servido_dito,
        BaldesDoRegistro, TETO_DA_FRASE_NO_REGISTRO,
    };
    use crate::pacote_de_teste::{pasta, publicar, ID, PNG, TEXTO, WAV};
    use crate::rastro_de_teste::capturar;
    use crate::relogio_de_teste::origem;

    /// **O nome do som servido não escreve uma segunda linha no registro.**
    ///
    /// Quando o som sai, o caminho já passou pelo manifesto, e o manifesto só
    /// confere que cada pedaço dele é um nome (`inner_path`): uma quebra de
    /// linha passa. Escrito cru, um MOD que declarasse
    /// `som/a\nWARN seele_app: forjada.wav` escreveria, a cada som servido, uma
    /// linha com a cara do produto. O caminho vem aqui em texto, e não num
    /// pacote no disco, porque um nome de arquivo com `\n` não existe no
    /// Windows.
    #[test]
    fn um_caminho_com_quebra_de_linha_nao_forja_linha_no_som_servido() {
        let ((), rastro) = capturar(|| {
            som_servido_dito(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                7,
                "som/a\nWARN seele_app: forjada.wav",
                16,
            )
        });
        assert_eq!(
            rastro.lines().count(),
            1,
            "o nome que o MOD declarou escreveu uma segunda linha no seele.log, com a cara do \
             produto: {rastro}"
        );
        assert!(
            rastro.contains("INFO")
                && rastro.contains("som de MOD servido em bytes")
                && rastro.contains("mod_id=prova/midia")
                && rastro.contains(r#"caminho="som/a\nWARN seele_app: forjada.wav""#),
            "a linha do som servido perdeu o id, a frase ou o caminho entre aspas, e quem lê o \
             registro já não acha o que o Rust serviu: {rastro}"
        );
    }

    /// **O nome do som servido tem o teto das outras portas de texto de
    /// terceiro.** As aspas impedem a linha forjada, e não a linha sem fim.
    #[test]
    fn um_caminho_enorme_sai_cortado_no_teto_de_frase_no_som_servido() {
        let ((), rastro) = capturar(|| {
            som_servido_dito(
                &BaldesDoRegistro::default(),
                origem(),
                ID,
                7,
                &"s".repeat(TETO_DA_FRASE_NO_REGISTRO * 4),
                16,
            );
        });
        assert!(
            rastro.contains(&format!(
                "caminho=\"{}\"",
                "s".repeat(TETO_DA_FRASE_NO_REGISTRO)
            )),
            "o nome que o MOD declarou entrou no registro sem o teto das outras portas de \
             texto de terceiro: {} caracteres numa linha",
            rastro.chars().count()
        );
    }

    /// **Os dois comandos que servem o pacote dizem o que serviram pela função
    /// que os testes medem**: `som_do_mod` por `som_servido_dito` (acima), e
    /// `midia_do_mod` por `midia_servida_dita` (no módulo da mídia recusada).
    /// Uma linha escrita à mão dentro de um deles deixaria esses testes verdes
    /// e o caminho cru no registro de novo.
    #[test]
    fn os_comandos_de_midia_dizem_o_que_serviram_pela_funcao_medida() {
        let fonte = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("main.rs legível")
        .replace("\r\n", "\n");
        for (comando, funcao) in [
            ("fn som_do_mod(", "som_servido_dito("),
            ("fn midia_do_mod(", "midia_servida_dita("),
        ] {
            let corpo = fonte
                .split(comando)
                .nth(1)
                .and_then(|resto| resto.split("\n}\n").next())
                .unwrap_or_else(|| panic!("`{comando}` sumiu de main.rs"));
            let codigo: Vec<&str> = corpo
                .lines()
                .filter(|linha| !linha.trim_start().starts_with("//"))
                .collect();
            let codigo = codigo.join("\n");
            assert!(
                codigo.contains(funcao) && !codigo.contains("tracing::"),
                "`{comando}` escreve o que serviu por fora de `{funcao}`, e o caminho que o MOD \
                 declarou pode voltar cru ao registro"
            );
        }
    }

    #[test]
    fn um_som_declarado_sai_com_os_bytes_exatos() {
        let (raiz, hash) = publicar(&[("som/toque.wav", WAV)]);
        let bytes = som_declarado_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "som/toque.wav",
        )
        .expect("um som declarado foi recusado");
        assert_eq!(
            bytes, WAV,
            "os bytes do som mudaram no caminho, e a casca decodificaria outra coisa"
        );
    }

    /// **O som do pacote atravessa a ponte uma vez só** (P49-7b).
    ///
    /// `montarMidia` pede `midia_do_mod` para saber o papel e os bytes a
    /// contar, e o som em si vem cru por `som_do_mod`. O `data:` que
    /// `midia_do_mod` montava para um som não tinha quem o lesse: o arquivo
    /// era lido duas vezes, e uma delas atravessava a ponte em base64, um terço
    /// maior, em JSON. A imagem continua com o `data:`, que é o que o `<img>`
    /// mostra. O que se mede é o que atravessa a ponte: a resposta serializada.
    #[test]
    fn o_som_do_pacote_sai_sem_uri_e_a_imagem_com() {
        let (raiz, hash) = publicar(&[("som/toque.wav", WAV), ("img/rosto.png", PNG)]);
        let pela_ponte = |caminho: &str| {
            let (resultado, _) = capturar(|| {
                midia_declarada_do_mod(
                    &BaldesDoRegistro::default(),
                    origem(),
                    &pasta(&raiz),
                    ID,
                    &hash,
                    caminho,
                )
            });
            let midia = resultado
                .unwrap_or_else(|falha| panic!("`{caminho}` foi recusado: {}", motivo_de(&falha)));
            match serde_json::to_value(&midia) {
                Ok(serde_json::Value::Object(mapa)) => mapa,
                outro => panic!("a mídia não atravessa a ponte como objeto: {outro:?}"),
            }
        };
        let som = pela_ponte("som/toque.wav");
        assert!(
            !som.contains_key("uri"),
            "o som do pacote voltou a atravessar a ponte em base64, no `uri` de `midia_do_mod`, \
             além de cru por `som_do_mod`: o arquivo é lido duas vezes, e ninguém lê esse \
             `data:` — {som:?}"
        );
        assert_eq!(
            (som.get("papel"), som.get("bytes")),
            (
                Some(&serde_json::json!("som")),
                Some(&serde_json::json!(WAV.len()))
            ),
            "o som do pacote saiu sem o papel ou sem os bytes, e `montarMidia` não sabe que \
             elemento montar nem quanto contar no teto: {som:?}"
        );
        let imagem = pela_ponte("img/rosto.png");
        assert!(
            imagem
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|uri| uri.starts_with("data:image/png;base64,")),
            "a imagem do pacote saiu sem o `data:` que os bytes provaram, e o `<img>` da \
             região fica sem o que mostrar: {imagem:?}"
        );
    }

    #[test]
    fn uma_imagem_pedida_como_som_e_recusada_pelo_nome_e_dita() {
        let (raiz, hash) = publicar(&[("img/rosto.png", PNG)]);
        let (resultado, rastro) = capturar(|| {
            som_declarado_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                "img/rosto.png",
            )
        });
        let falha = resultado.expect_err("uma imagem saiu pelo caminho do som");
        assert_eq!(
            motivo_de(&falha),
            "nao-e-som",
            "a recusa não diz que o arquivo não é som, e o autor procuraria no lugar errado"
        );
        assert!(
            rastro.lines().any(|linha| linha.contains("WARN")
                && linha.contains("mod_id=prova/midia")
                && linha.contains("motivo=nao-e-som")),
            "a recusa do som não chegou ao registro: {rastro}"
        );
    }

    /// Dentro de `capturar`, como toda chamada de teste que passa por uma
    /// linha que outro teste captura.
    #[test]
    fn um_som_que_o_manifesto_nao_declara_nao_sai() {
        let (raiz, hash) = publicar(&[("som/toque.wav", WAV)]);
        let (resultado, rastro) = capturar(|| {
            som_declarado_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                "som/escondido.wav",
            )
        });
        let falha = resultado.expect_err("um som que o manifesto não declara saiu");
        assert_eq!(
            motivo_de(&falha),
            "arquivo-nao-declarado",
            "o som fora do manifesto foi recusado por outro motivo"
        );
        assert!(
            rastro.lines().any(|linha| linha.contains("WARN")
                && linha.contains("mod_id=prova/midia")
                && linha.contains("motivo=arquivo-nao-declarado")),
            "a recusa do som fora do manifesto não chegou ao registro: {rastro}"
        );
    }

    /// **Bytes que não são de formato nenhum, pedidos como som, são recusados
    /// pelo nome e ditos.** É o braço `None` de `som_declarado_do_mod`: sem
    /// ele, o texto sairia como som e viraria um `decodeAudioData` que falha
    /// sem dizer por quê.
    #[test]
    fn um_texto_pedido_como_som_e_recusado_como_formato_desconhecido_e_dito() {
        let (raiz, hash) = publicar(&[("som/x.wav", TEXTO)]);
        let (resultado, rastro) = capturar(|| {
            som_declarado_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                "som/x.wav",
            )
        });
        let falha = resultado.expect_err("texto saiu pelo caminho do som");
        assert_eq!(
            motivo_de(&falha),
            "formato-desconhecido",
            "texto pedido como som foi recusado por outro motivo, e o autor procuraria no \
             lugar errado"
        );
        assert!(
            rastro.lines().any(|linha| linha.contains("WARN")
                && linha.contains("mod_id=prova/midia")
                && linha.contains("motivo=formato-desconhecido")),
            "a recusa do som de formato desconhecido não chegou ao registro: {rastro}"
        );
    }
}

/// **O som grande demais decodificado é recusado antes de a janela o
/// decodificar** (P49-7a).
///
/// `decodeAudioData` devolve o som inteiro em float32, de uma vez: a janela só
/// o conta depois de ele existir, e o pico já aconteceu. O Rust lê um piso do
/// cabeçalho e recusa antes de servir. Um teste por braço — WAV, Ogg, MP3 —,
/// cada um com o caso grande que só ele recusa, com bytes montados aqui; e as
/// duas portas do servidor, que entregam o som pelo mesmo `data:`.
#[cfg(test)]
mod o_som_grande_demais_decodificado_e_recusado_antes {
    use super::{
        bytes_decodificados_minimos, imagem_lida_dita, ler_midia_do_servidor, motivo_de,
        som_declarado_do_mod, BaldesDoRegistro, FalhaNoMod, TAXA_DA_DECODIFICACAO,
        TETO_DO_SOM_DECODIFICADO,
    };
    use crate::pacote_de_teste::{pasta, publicar, ID, WAV};
    use crate::rastro_de_teste::capturar;
    use crate::relogio_de_teste::origem;
    use seele_ffi::mods::TETO_DE_MIDIA;

    /// Um WAV PCM com `dados` bytes de silêncio.
    fn wav(canais: u16, taxa: u32, bits: u16, dados: usize) -> Vec<u8> {
        let por_quadro = canais * bits.div_ceil(8);
        let tamanho = u32::try_from(dados).expect("o WAV de teste cabe em 32 bits");
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(36 + tamanho).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&canais.to_le_bytes());
        wav.extend_from_slice(&taxa.to_le_bytes());
        wav.extend_from_slice(&(taxa * u32::from(por_quadro)).to_le_bytes());
        wav.extend_from_slice(&por_quadro.to_le_bytes());
        wav.extend_from_slice(&bits.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&tamanho.to_le_bytes());
        wav.resize(44 + dados, 0);
        wav
    }

    /// Uma página Ogg com um pacote só, de menos de 255 bytes. O CRC fica em
    /// zero: a leitura segue as páginas pelo tamanho, e não o confere.
    fn pagina_ogg(tipo: u8, granule: u64, sequencia: u32, pacote: &[u8]) -> Vec<u8> {
        let mut pagina = b"OggS\0".to_vec();
        pagina.push(tipo);
        pagina.extend_from_slice(&granule.to_le_bytes());
        pagina.extend_from_slice(&0x5EE1_E000_u32.to_le_bytes());
        pagina.extend_from_slice(&sequencia.to_le_bytes());
        pagina.extend_from_slice(&[0; 4]);
        pagina.push(1);
        pagina.push(u8::try_from(pacote.len()).expect("o pacote de teste cabe num segmento"));
        pagina.extend_from_slice(pacote);
        pagina
    }

    /// Um Ogg de três páginas: a da identificação, uma de dados logo no
    /// começo, e a última, que diz até onde o som vai.
    fn ogg(identificacao: &[u8], granule_final: u64) -> Vec<u8> {
        [
            pagina_ogg(0x02, 0, 0, identificacao),
            pagina_ogg(0x00, 1024, 1, &[0; 16]),
            pagina_ogg(0x04, granule_final, 2, &[0; 16]),
        ]
        .concat()
    }

    /// O cabeçalho de identificação do Vorbis.
    fn vorbis(canais: u8, taxa: u32) -> Vec<u8> {
        let mut id = b"\x01vorbis".to_vec();
        id.extend_from_slice(&0u32.to_le_bytes());
        id.push(canais);
        id.extend_from_slice(&taxa.to_le_bytes());
        id.extend_from_slice(&[0; 12]);
        id.extend_from_slice(&[0xB8, 1]);
        id
    }

    /// O cabeçalho de identificação do Opus.
    fn opus(canais: u8, pre_skip: u16) -> Vec<u8> {
        let mut id = b"OpusHead\x01".to_vec();
        id.push(canais);
        id.extend_from_slice(&pre_skip.to_le_bytes());
        id.extend_from_slice(&44_100u32.to_le_bytes());
        id.extend_from_slice(&[0; 3]);
        id
    }

    /// Um quadro MPEG-1 camada III a 48 kHz e 320 kbps — 960 bytes e 1152
    /// amostras —, com o quadro Xing quando `xing` diz quantos são.
    fn quadro_mp3(canais: u8, xing: Option<u32>) -> Vec<u8> {
        let modo = if canais == 1 { 0xC0 } else { 0x00 };
        let mut quadro = vec![0xFF, 0xFB, 0xE4, modo];
        if let Some(quadros) = xing {
            quadro.resize(if canais == 1 { 4 + 17 } else { 4 + 32 }, 0);
            quadro.extend_from_slice(b"Xing");
            quadro.extend_from_slice(&1u32.to_be_bytes());
            quadro.extend_from_slice(&quadros.to_be_bytes());
        }
        quadro.resize(960, 0);
        quadro
    }

    /// Um MP3 de `tamanho` bytes, quadro depois de quadro; o último fica
    /// cortado onde o tamanho acaba.
    fn mp3(canais: u8, xing: Option<u32>, tamanho: usize) -> Vec<u8> {
        let mut mp3 = quadro_mp3(canais, xing);
        let quadro = quadro_mp3(canais, None);
        while mp3.len() < tamanho {
            mp3.extend_from_slice(&quadro);
        }
        mp3.truncate(tamanho);
        mp3
    }

    /// Publica `bytes` em `caminho` e confere que o som é recusado pelo nome,
    /// com os números, e dito no registro.
    fn recusado(caminho: &str, bytes: &[u8]) {
        let (raiz, hash) = publicar(&[(caminho, bytes)]);
        let (resultado, rastro) = capturar(|| {
            som_declarado_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                caminho,
            )
        });
        let falha = resultado.err().unwrap_or_else(|| {
            panic!(
                "`{caminho}`, que decodificado passa do teto do som, foi servido: a janela o \
                 decodifica inteiro antes de poder recusá-lo, e o pico que o teto existe para \
                 evitar acontece — o piso foi {:?}",
                bytes_decodificados_minimos(bytes)
            )
        });
        recusado_pelo_teto(caminho, &falha);
        assert!(
            rastro.lines().any(|linha| linha.contains("WARN")
                && linha.contains("mod_id=prova/midia")
                && linha.contains("som-grande-demais-decodificado")),
            "a recusa de `{caminho}` não chegou ao registro, e quem hospeda não sabe por que o \
             som não tocou: {rastro}"
        );
    }

    /// A recusa é a do som grande demais decodificado, com os números.
    fn recusado_pelo_teto(caminho: &str, falha: &FalhaNoMod) {
        let motivo = motivo_de(falha);
        assert!(
            motivo.starts_with("som-grande-demais-decodificado")
                && motivo.contains(&TETO_DO_SOM_DECODIFICADO.to_string()),
            "`{caminho}` foi recusado por outro motivo, ou sem o teto: a janela não o diz como o \
             teto do som, e o MOD fica sem os números — {motivo}"
        );
    }

    /// Publica `bytes` em `caminho` e confere que o som sai, intacto.
    fn servido(caminho: &str, bytes: &[u8]) {
        let (raiz, hash) = publicar(&[(caminho, bytes)]);
        let (resultado, _) = capturar(|| {
            som_declarado_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                caminho,
            )
        });
        let servido = resultado.unwrap_or_else(|falha| {
            panic!(
                "`{caminho}`, que cabe decodificado, foi recusado — {}",
                motivo_de(&falha)
            )
        });
        assert!(
            servido == bytes,
            "os bytes de `{caminho}` mudaram no caminho"
        );
    }

    /// O piso em bytes de `segundos` de som a 48 kHz, com `canais`: o que a
    /// janela segura, menos o quadro que a conversão de taxa pode arredondar.
    fn segundos(segundos: u64, canais: u64) -> u64 {
        (segundos * TAXA_DA_DECODIFICACAO - 1) * canais * 4
    }

    /// **WAV: exato, pelo `fmt ` e pelo `data`.** Oito bits, mono, 8 kHz, do
    /// tamanho do teto de arquivo: cada byte vira seis quadros de quatro bytes,
    /// e o arquivo de dez MiB, uns 250 MB decodificado.
    #[test]
    fn um_wav_que_passa_do_teto_decodificado_e_recusado_antes() {
        let grande = wav(1, 8_000, 8, TETO_DE_MIDIA - 44);
        assert_eq!(
            grande.len(),
            TETO_DE_MIDIA,
            "o WAV grande não é do tamanho do teto"
        );
        recusado("som/grande.wav", &grande);
        assert_eq!(
            bytes_decodificados_minimos(&grande),
            Some(((TETO_DE_MIDIA as u64 - 44) * 6 - 1) * 4),
            "o piso do WAV não é o exato que o `fmt ` e o `data` dizem"
        );

        // Um segundo passa, com os bytes exatos.
        let curto = wav(1, 8_000, 8, 8_000);
        assert_eq!(
            bytes_decodificados_minimos(&curto),
            Some(segundos(1, 1)),
            "o piso de um segundo de WAV não é um segundo a 48 kHz"
        );
        servido("som/curto.wav", &curto);

        // **O `data` diz mais do que o arquivo tem**, como num WAV gravado
        // aos poucos: o piso conta os bytes que estão lá.
        let mut aos_poucos = wav(1, 8_000, 8, 8_000);
        if let Some(tamanho) = aos_poucos.get_mut(40..44) {
            tamanho.copy_from_slice(&u32::MAX.to_le_bytes());
        }
        assert_eq!(
            bytes_decodificados_minimos(&aos_poucos),
            Some(segundos(1, 1)),
            "o piso do WAV contou o tamanho que o `data` alega, e não os bytes que estão no \
             arquivo: um WAV gravado aos poucos seria recusado pelo que não tem"
        );

        // O WAV mínimo do pacote de teste não tem `fmt ` nem `data`
        // completos: o piso não existe, e o som segue para a janela.
        assert_eq!(
            bytes_decodificados_minimos(WAV),
            None,
            "um WAV sem `fmt ` nem `data` ganhou um piso, que só pode ter sido inventado"
        );
    }

    /// **Ogg: pela granule da última página.** Três minutos de Vorbis estéreo a
    /// 44,1 kHz são uns 69 MB a 48 kHz; no Opus, a granule já está a 48 kHz,
    /// menos o pre-skip.
    #[test]
    fn um_ogg_que_passa_do_teto_decodificado_e_recusado_antes() {
        recusado("som/vorbis.ogg", &ogg(&vorbis(2, 44_100), 180 * 44_100));
        recusado("som/opus.ogg", &ogg(&opus(2, 312), 180 * 48_000 + 312));

        // Um minuto passa: pouco menos que um minuto a 48 kHz, estéreo.
        let piso = bytes_decodificados_minimos(&ogg(&vorbis(2, 44_100), 60 * 44_100));
        assert!(
            piso.is_some_and(|piso| piso <= segundos(60, 2) && piso > segundos(59, 2)),
            "o piso de um minuto de Vorbis não é um minuto: {piso:?}"
        );
        // O Opus mono conta um canal, e o pre-skip sai.
        let piso = bytes_decodificados_minimos(&ogg(&opus(1, 312), 60 * 48_000 + 312));
        assert!(
            piso.is_some_and(|piso| piso <= segundos(60, 1) && piso > segundos(59, 1)),
            "o piso de um minuto de Opus mono não é um minuto: {piso:?}"
        );
    }

    /// **MP3: pelos quadros.** Um Xing de 7600 quadros estéreo são uns 70 MB
    /// decodificado; sem Xing, a cadeia de quadros diz o mesmo. Um mono sem
    /// Xing do tamanho do teto — a 320 kbps, a taxa mais alta do MPEG-1 camada
    /// III, o mais curto que dez MiB podem ser — fica em uns 50 MB, e passa.
    #[test]
    fn um_mp3_que_passa_do_teto_decodificado_e_recusado_antes() {
        recusado("som/xing.mp3", &mp3(2, Some(7_600), 7_600 * 960));
        recusado("som/estereo.mp3", &mp3(2, None, TETO_DE_MIDIA));

        let mono = bytes_decodificados_minimos(&mp3(1, None, TETO_DE_MIDIA));
        assert!(
            mono.is_some_and(|piso| piso > 45_000_000 && piso <= TETO_DO_SOM_DECODIFICADO),
            "o MP3 mono do tamanho do teto não ficou abaixo do teto do som, e seria recusado \
             por um piso que passa do real: {mono:?}"
        );

        // **O Xing de um arquivo cortado conta o que não está nele.** O piso
        // fica nos quadros que estão: cem, e não 7600.
        let cortado = bytes_decodificados_minimos(&mp3(2, Some(7_600), 100 * 960));
        assert!(
            cortado.is_some_and(|piso| piso < segundos(3, 2)),
            "o piso do MP3 cortado contou os quadros que o Xing alega, e não os que estão no \
             arquivo: um download interrompido seria recusado pelo que não tem — {cortado:?}"
        );
    }

    /// **As margens que fazem do valor um piso**, uma por teste. Sem elas os
    /// casos acima continuam verdes, e o piso passa do real: medido pela
    /// revisão contra o `decodeAudioData` do Chromium e do WebKit — o motor do
    /// produto no Mac —, um WAV com o bloco declarado menor dava quatro vezes
    /// o real, um Opus com pre-skip de 3840 passava 23 032 bytes, e um MP3
    /// comum, de 16 a 25 mil.
    ///
    /// O WAV: o bloco declarado (1) é menor que os canais vezes os bytes da
    /// amostra (2 × 2). O quadro conta pelo maior: um segundo estéreo de
    /// 16 bits a 8 kHz, e não quatro.
    #[test]
    fn o_bloco_menor_que_a_amostra_nao_multiplica_o_piso_do_wav() {
        let mut wav = wav(2, 8_000, 16, 8_000 * 4);
        if let Some(bloco) = wav.get_mut(32..34) {
            bloco.copy_from_slice(&1u16.to_le_bytes());
        }
        assert_eq!(
            bytes_decodificados_minimos(&wav),
            Some(segundos(1, 2)),
            "o piso do WAV contou os quadros pelo bloco que o cabeçalho declara, menor que a \
             amostra: quatro vezes o real, e um som que cabe seria recusado"
        );
    }

    /// **Um segundo `fmt ` deixa o WAV sem piso.** O Chromium decodifica pelo
    /// primeiro, e o piso que lesse o último recusaria pelo formato que o
    /// motor não usa: a revisão mediu 127 MB de piso contra 11,5 MB reais. Qual
    /// dos dois vale é pergunta de motor, e um piso não escolhe.
    #[test]
    fn um_segundo_fmt_deixa_o_wav_sem_piso() {
        let primeiro = wav(2, 48_000, 16, 48_000 * 4);
        let segundo = wav(1, 8_000, 8, 0);
        let mut dois = [
            primeiro.get(..36).expect("o cabeçalho e o primeiro `fmt `"),
            segundo.get(12..36).expect("o segundo `fmt `"),
            primeiro.get(36..).expect("o `data`"),
        ]
        .concat();
        let riff = u32::try_from(dois.len() - 8).expect("o WAV de teste cabe em 32 bits");
        if let Some(tamanho) = dois.get_mut(4..8) {
            tamanho.copy_from_slice(&riff.to_le_bytes());
        }
        assert_eq!(
            bytes_decodificados_minimos(&dois),
            None,
            "um WAV com dois `fmt ` ganhou um piso por um deles, e o motor pode decodificar pelo \
             outro: a revisão mediu a recusa de um som que cabia"
        );
    }

    /// **O `data` não passa do tamanho do RIFF.** Um `data` gravado aos poucos
    /// (`0xFFFFFFFF`) seguido de outro pedaço: o WebKit para onde o RIFF diz
    /// que o arquivo acaba, e o piso que contasse até o fim do arquivo passaria
    /// do real — a revisão mediu 2,28 MB a mais.
    #[test]
    fn o_data_do_wav_nao_passa_do_tamanho_do_riff() {
        let mut wav = wav(1, 8_000, 8, 8_000);
        if let Some(tamanho) = wav.get_mut(40..44) {
            tamanho.copy_from_slice(&u32::MAX.to_le_bytes());
        }
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&4_000u32.to_le_bytes());
        wav.resize(wav.len() + 4_000, 0);
        assert_eq!(
            bytes_decodificados_minimos(&wav),
            Some(segundos(1, 1)),
            "o piso do WAV contou como som o pedaço que vem depois do fim que o RIFF declara, e \
             o WebKit para ali: o piso passa do real"
        );
    }

    /// O Opus: o pre-skip (3840) é maior que a granule da primeira página de
    /// dados (1024, um pacote só). O que sai do começo é o maior dos dois: um
    /// segundo depois do pre-skip, e não 2816 amostras a mais.
    #[test]
    fn o_pre_skip_maior_que_a_primeira_pagina_sai_do_piso_do_opus() {
        let opus = ogg(&opus(2, 3_840), 48_000 + 3_840);
        assert_eq!(
            bytes_decodificados_minimos(&opus),
            Some(segundos(1, 2)),
            "o piso do Opus tirou do começo só a granule da primeira página, menor que o \
             pre-skip: o decodificador corta o pre-skip inteiro, e o piso passa do real"
        );
    }

    /// O MP3: cem quadros estéreo de 1152 amostras são 115 200; o corte do
    /// decodificador — o quadro Xing/Info, o atraso e o enchimento do LAME, e o
    /// atraso do decodificador — tira 1152 + 2 × 4095 + 529 = 9871, a
    /// conversão de taxa tira um quadro, e o resto, vezes dois canais e quatro
    /// bytes, é 842 624.
    #[test]
    fn o_corte_do_decodificador_sai_do_piso_do_mp3() {
        assert_eq!(
            bytes_decodificados_minimos(&mp3(2, None, 100 * 960)),
            Some(842_624),
            "o piso do MP3 contou as amostras que o decodificador corta — o quadro do Xing, o \
             atraso e o enchimento —, e passa do real em todo MP3 comum"
        );
    }

    /// **As duas portas do servidor recusam o mesmo som**: a resposta de
    /// `midia_em_bytes` e a leitura por volume chegam à janela pelo `data:`, e
    /// a janela o decodifica inteiro, como o do pacote.
    #[test]
    fn o_som_do_servidor_que_passa_do_teto_decodificado_e_recusado_antes_de_servir() {
        let grande = wav(1, 8_000, 8, TETO_DE_MIDIA - 44);
        let falha = ler_midia_do_servidor(&seele_ffi::base64_de(&grande), None)
            .err()
            .expect("o som do servidor que passa do teto decodificado foi servido no `data:`");
        recusado_pelo_teto("o som do servidor", &falha);

        let (resultado, rastro) = capturar(|| {
            imagem_lida_dita(&BaldesDoRegistro::default(), origem(), ID, Ok(grande), None)
        });
        let falha = resultado
            .err()
            .expect("o som lido por volume que passa do teto decodificado foi servido");
        recusado_pelo_teto("o som lido por volume", &falha);
        assert!(
            rastro.lines().any(|linha| linha.contains("WARN")
                && linha.contains("origem=\"volume\"")
                && linha.contains("som-grande-demais-decodificado")),
            "a recusa do som lido por volume não chegou ao registro: {rastro}"
        );

        let curto = ler_midia_do_servidor(&seele_ffi::base64_de(&wav(1, 8_000, 8, 8_000)), None);
        assert!(
            curto.is_ok_and(|midia| midia.papel == "som" && midia.uri.is_some()),
            "o som curto do servidor deixou de sair com o `data:`, que é o único caminho dele \
             até a janela"
        );
    }

    /// **Os números do som decodificado são os da janela.** O Rust repete o
    /// teto e a taxa de `mods-regiao.js`: com um maior que o da janela, o Rust
    /// serviria o som que a janela vai recusar depois do pico; com um menor,
    /// recusaria o que cabe.
    #[test]
    fn os_numeros_do_som_decodificado_sao_os_da_janela() {
        let regiao = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/mods-regiao.js"),
        )
        .expect("mods-regiao.js legível")
        .replace("\r\n", "\n");
        let taxa = regiao
            .lines()
            .find_map(|linha| linha.strip_prefix("const TAXA_DA_DECODIFICACAO = "))
            .and_then(|valor| numero_em_js(valor.trim_end_matches(';')));
        assert_eq!(
            taxa,
            Some(TAXA_DA_DECODIFICACAO),
            "`TAXA_DA_DECODIFICACAO` de `mods-regiao.js` não é a do Rust: o piso conta os \
             quadros numa taxa e a janela decodifica noutra"
        );
        for tabela in ["LIMITES_DA_REGIAO", "LIMITES_DO_CARTAO"] {
            let teto = regiao
                .split(&format!("const {tabela} = Object.freeze({{"))
                .nth(1)
                .and_then(|resto| resto.split("\n});").next())
                .and_then(|corpo| {
                    corpo
                        .lines()
                        .find_map(|linha| linha.trim().strip_prefix("bytesDeSomDecodificado:"))
                })
                .and_then(|valor| numero_em_js(valor.trim().trim_end_matches(',')));
            assert_eq!(
                teto,
                Some(TETO_DO_SOM_DECODIFICADO),
                "`{tabela}.bytesDeSomDecodificado` não é o teto do Rust: o Rust serve o som que a \
                 janela recusa depois do pico, ou recusa o que cabe"
            );
        }
    }

    /// Um número de JavaScript escrito como produto de inteiros — `48000`,
    /// `64 * 1024 * 1024` —, ou nada.
    fn numero_em_js(texto: &str) -> Option<u64> {
        texto.split('*').try_fold(1u64, |produto, fator| {
            produto.checked_mul(fator.trim().replace('_', "").parse().ok()?)
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod a_abertura_pelo_launcher {
    use super::{atualizacao_permitida, Abertura, FalhaAoAtualizar};

    #[test]
    fn quem_foi_aberta_pelo_launcher_sabe_disso() {
        assert!(Abertura::de(Vec::<String>::new(), Some("1")).pelo_lancador);
        assert!(!Abertura::de(Vec::<String>::new(), None).pelo_lancador);
        // Só o valor que o launcher põe. Uma variável esquecida no ambiente
        // com outro valor não esconde o botão de ninguém.
        assert!(!Abertura::de(Vec::<String>::new(), Some("0")).pelo_lancador);
    }

    /// Os argumentos continuam sendo lidos como antes, com a variável junto.
    #[test]
    fn os_argumentos_do_launcher_continuam_chegando() {
        let aberta = Abertura::de(
            ["--hospedar", "--nome-publico", "casa.exemplo.br"].map(str::to_owned),
            Some("1"),
        );
        assert!(aberta.hospedar && aberta.pelo_lancador);
        assert_eq!(aberta.nome_publico.as_deref(), Some("casa.exemplo.br"));
    }

    /// Spec 0.16, D-linha-1, valendo também para a 0.16: aberta pelo launcher
    /// de uma linha mais nova (uma 2.0 abrindo a 1.x), o atualizador dela
    /// trocaria o app do depósito pela versão do topo do manifesto — outra
    /// linha. Esconder o botão não basta: o comando recusa.
    #[test]
    fn aberta_pelo_launcher_o_atualizador_recusa() {
        assert!(matches!(
            atualizacao_permitida(&Abertura::de(Vec::<String>::new(), Some("1"))),
            Err(FalhaAoAtualizar::AbertaPeloLancador)
        ));
        assert!(atualizacao_permitida(&Abertura::de(Vec::<String>::new(), None)).is_ok());
    }
}

/// O tipo de um arquivo escolhido para um MOD, ou a recusa com nome.
#[cfg(test)]
mod o_tipo_do_arquivo_escolhido {
    use super::{tipo_do_escolhido, FalhaNoMod};

    const HEIC: &[u8] = include_bytes!("../../../crates/seele-proto/tests/vetores/heic-8x8.heic");

    fn motivo(falha: FalhaNoMod) -> String {
        match falha {
            FalhaNoMod::Recusado { motivo } => motivo,
            outra => panic!("esperava uma recusa com nome, e veio {outra:?}"),
        }
    }

    /// **O HEIC das Fotos é recusado pelo nome** (rascunho da API 6, §7.1; o
    /// bloqueio G do PERFIS): `tipo-recusado:heic`, que o MOD da 6 recebe como
    /// `tipo-recusado` com `detectado: 'heic'`. Antes ele caía em
    /// `formato-desconhecido`, que manda procurar defeito num arquivo que não
    /// tem defeito nenhum.
    #[test]
    fn um_heic_e_recusado_com_o_nome_dele() {
        let falha = tipo_do_escolhido(HEIC).expect_err("o HEIC passou como imagem");
        assert_eq!(motivo(falha), "tipo-recusado:heic");
    }

    #[test]
    fn o_que_o_produto_nao_reconhece_continua_formato_desconhecido() {
        let falha = tipo_do_escolhido(b"isto nao e imagem").expect_err("texto passou como imagem");
        assert_eq!(motivo(falha), "formato-desconhecido");
    }

    #[test]
    fn uma_imagem_aceita_passa() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        let lido = tipo_do_escolhido(&png).expect("o PNG foi recusado");
        assert_eq!(lido.media_type, "image/png");
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod parar_de_hospedar_e_so_o_guardado {
    //! A 19 e a 20 da F7: sair não derruba nada, e o verbo que derruba é o
    //! `parar_de_hospedar`, só para o guardado. O `meu_servidor` diz à entrada
    //! para onde o VOLTAR AO MEU SERVIDOR leva.

    use super::{meu_servidor_em, parar_de_hospedar_em, Hospedado, MeuServidor, Session};
    use seele_server::hospedagem::Hospedagem;

    /// Uma sala no lugar do guardado, com a porta dela: este arquivo só sobe
    /// hospedagem pelo ponto padrão (o guarda
    /// `o_ponto_de_encontro_de_quem_hospeda`), e um teste não pergunta à VPS de
    /// produção. O que se prova aqui é o lugar do guardado na sessão.
    async fn guardado_de_teste(sessao: &Session, servidor: Option<String>) -> u16 {
        let hospedagem = Hospedagem::iniciar_sala(0, &[2; 32], "casa", None)
            .await
            .expect("o guardado de teste não subiu");
        let porta = hospedagem.endereco().port();
        sessao.hospedagens.lock().unwrap().guardado = Some(Hospedado {
            hospedagem,
            servidor,
        });
        porta
    }

    #[tokio::test]
    async fn parar_de_hospedar_derruba_o_guardado_devolve_a_porta_e_deixa_a_sala() {
        let sessao = Session::default();
        let porta = guardado_de_teste(&sessao, None).await;
        let sala = Hospedagem::iniciar_sala(0, &[1; 32], "dona", None)
            .await
            .expect("a sala não subiu");
        sessao.hospedagens.lock().unwrap().pessoal = Some(Hospedado {
            hospedagem: sala,
            servidor: None,
        });

        assert!(parar_de_hospedar_em(&sessao).await, "havia guardado no ar");

        {
            let hospedagens = sessao.hospedagens.lock().unwrap();
            assert!(hospedagens.guardado.is_none(), "o guardado ficou no ar");
            assert!(
                hospedagens.pessoal.is_some(),
                "PARAR DE HOSPEDAR derrubou a sala pessoal, que desce pelo «receber ligações»"
            );
        }
        assert!(
            std::net::UdpSocket::bind(("::", porta)).is_ok(),
            "o guardado parou e a porta {porta} ficou presa: hospedar de novo não subiria"
        );
        assert!(
            !parar_de_hospedar_em(&sessao).await,
            "parar duas vezes disse que havia o que parar"
        );

        let sala = sessao.hospedagens.lock().unwrap().pessoal.take();
        if let Some(sala) = sala {
            sala.hospedagem.encerrar().await;
        }
    }

    #[tokio::test]
    async fn meu_servidor_diz_onde_voltar_e_o_nome_que_a_lista_mostra() {
        let casa = tempfile::tempdir().unwrap();
        let config = casa.path().to_str().unwrap();
        let registrado = crate::servidores::criar(config, "Casa da Rafa", "0.16.0");
        let sessao = Session::default();
        let meu = |sessao: &Session| meu_servidor_em(&sessao.hospedagens.lock().unwrap(), config);
        assert_eq!(
            meu(&sessao),
            None,
            "sem guardado no ar, a entrada ofereceu VOLTAR a lugar nenhum"
        );

        let porta = guardado_de_teste(&sessao, Some(registrado.id.clone())).await;
        assert_eq!(
            meu(&sessao),
            Some(MeuServidor {
                alvo: format!("127.0.0.1:{porta}"),
                nome: Some("Casa da Rafa".into()),
            })
        );

        // O legado (o banco de sempre, sem registro) volta sem nome, e a frase
        // diz «o seu servidor».
        sessao
            .hospedagens
            .lock()
            .unwrap()
            .guardado
            .as_mut()
            .unwrap()
            .servidor = None;
        assert_eq!(meu(&sessao).and_then(|meu| meu.nome), None);
        parar_de_hospedar_em(&sessao).await;
    }
}

#[cfg(test)]
mod a_saida_despede_as_hospedagens {
    use super::{despedir_das_hospedagens, Hospedado, Session};
    use seele_server::hospedagem::Hospedagem;

    #[tokio::test]
    async fn as_duas_saem_da_sessao_e_se_despedem() {
        // O comportamento de cada despedida é da hospedagem (`quem_hospeda_fecha.rs`
        // e `sala_pessoal.rs`, no `seele-conformance`). O que é deste arquivo:
        // a saída leva as duas, e não só a que existia antes do Plano 2.
        let sessao = Session::default();
        // Duas salas, e não um servidor guardado e uma sala: este arquivo só
        // sobe hospedagem pelo ponto padrão (o guarda
        // `o_ponto_de_encontro_de_quem_hospeda`), e um teste não pergunta à VPS
        // de produção. O que se prova aqui é o lugar de cada uma na sessão.
        let guardado = Hospedagem::iniciar_sala(0, &[2; 32], "casa", None)
            .await
            .expect("a primeira não subiu");
        let sala = Hospedagem::iniciar_sala(0, &[1; 32], "dona", None)
            .await
            .expect("a sala não subiu");
        {
            let mut hospedagens = sessao.hospedagens.lock().expect("o cadeado");
            hospedagens.guardado = Some(Hospedado {
                hospedagem: guardado,
                servidor: None,
            });
            hospedagens.pessoal = Some(Hospedado {
                hospedagem: sala,
                servidor: None,
            });
        }

        despedir_das_hospedagens(&sessao, std::time::Duration::from_millis(400)).await;

        let hospedagens = sessao.hospedagens.lock().expect("o cadeado");
        assert!(hospedagens.guardado.is_none(), "o guardado ficou para trás");
        assert!(hospedagens.pessoal.is_none(), "a sala ficou para trás");
    }
}

#[cfg(test)]
mod a_resposta_a_estou_no_meu_servidor {
    use super::{
        corpo_de, endereco_desta_maquina, fonte_sem_comentarios, onde_estou_em,
        porta_desta_maquina, Hospedado, Hospedagens, OndeEstou,
    };
    use seele_server::hospedagem::Hospedagem;

    /// A chave do guardado, a da sala e a de um servidor S qualquer.
    const GUARDADO: &str = "aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11";
    const SALA: &str = "bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22";
    const DE_S: &str = "cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33";

    #[test]
    fn onde_estou_pela_chave_e_nao_pelo_endereco() {
        assert_eq!(
            onde_estou_em(None, None, Some(GUARDADO), Some(SALA)),
            OndeEstou::Nenhum,
            "sem sessão, não há onde estar"
        );
        // Os cinco endereços por onde se entra na mesma hospedagem: a chave é
        // a mesma, e a resposta também. A impressão chega da conexão em
        // maiúsculas aqui de propósito: a comparação não as diferencia.
        let fora: Vec<&str> = [
            "127.0.0.1:8383",
            "localhost:8383",
            "[::1]:8383",
            "192.168.1.20:8383",
            "casa.exemplo:8383",
        ]
        .into_iter()
        .filter(|endereco| {
            onde_estou_em(
                Some(endereco),
                Some(&GUARDADO.to_ascii_uppercase()),
                Some(GUARDADO),
                Some(SALA),
            ) != OndeEstou::Guardado
        })
        .collect();
        assert!(
            fora.is_empty(),
            "a sessão com a chave do guardado, aberta por {fora:?}, não foi dita no guardado: \
             o próprio link e os outros nomes da mesma máquina deixam de ser o servidor daqui"
        );
        assert_eq!(
            onde_estou_em(
                Some("127.0.0.1:8385"),
                Some(SALA),
                Some(GUARDADO),
                Some(SALA)
            ),
            OndeEstou::Pessoal,
            "a sessão com a chave da sala não foi dita na sala"
        );
    }

    #[test]
    fn o_mesmo_endereco_com_outra_chave_e_outro_lugar() {
        // Um servidor S no mesmo endereço do guardado, com outra chave: o
        // HOSPEDAR COM ESTA da F6 pondo outra versão em 127.0.0.1, ou um
        // endereço da rede local que hoje é de outra máquina.
        for endereco in ["127.0.0.1:8383", "192.168.1.20:8383", "127.0.0.1:8385"] {
            assert_eq!(
                onde_estou_em(Some(endereco), Some(DE_S), Some(GUARDADO), Some(SALA)),
                OndeEstou::Outro,
                "um servidor com outra chave em {endereco} passou por uma das hospedagens desta \
                 janela: o sim de quem hospeda iria a ele (ADR 0045)"
            );
        }
        // Sem a hospedagem no ar, a chave dela não é de ninguém aqui.
        assert_eq!(
            onde_estou_em(Some("127.0.0.1:8383"), Some(GUARDADO), None, Some(SALA)),
            OndeEstou::Outro,
            "sem guardado no ar, a sessão foi dita no guardado"
        );
        assert_eq!(
            onde_estou_em(Some("127.0.0.1:8385"), Some(SALA), Some(GUARDADO), None),
            OndeEstou::Outro,
            "sem sala no ar, a sessão foi dita na sala"
        );
        // Com sessão e sem impressão, falha fechado.
        assert_eq!(
            onde_estou_em(Some("127.0.0.1:8383"), None, Some(GUARDADO), Some(SALA)),
            OndeEstou::Outro,
            "uma sessão sem a chave conferida foi dita numa hospedagem desta janela"
        );
    }

    #[test]
    fn um_endereco_desta_maquina_como_a_pessoa_o_escreve() {
        for (escrito, porta) in [
            ("127.0.0.1:8383", Some(8383)),
            ("127.0.0.1", Some(8383)),
            ("localhost:8385", Some(8385)),
            ("localhost", Some(8383)),
            ("[::1]:8385", Some(8385)),
            ("[::1]", Some(8383)),
            ("  127.0.0.1:8383 ", Some(8383)),
            ("192.168.0.7:8383", None),
            ("casa.exemplo", None),
            ("127.0.0.1.casa.exemplo:8383", None),
        ] {
            assert_eq!(porta_desta_maquina(escrito), porta, "{escrito:?}");
        }
    }

    #[test]
    fn a_lista_de_visitados_continua_sem_os_enderecos_desta_maquina() {
        // A regra da lista é outra pergunta, e ela não mudou: um endereço desta
        // máquina não é lugar aonde se volta (o `hospedado_aqui` de antes).
        for desta in ["127.0.0.1:8383", "localhost", "[::1]:8385", "127.0.0.1"] {
            assert!(endereco_desta_maquina(desta), "{desta:?}");
        }
        for de_fora in ["192.168.0.7:8383", "casa.exemplo:8383", "203.0.113.7"] {
            assert!(!endereco_desta_maquina(de_fora), "{de_fora:?}");
        }
    }

    /// **As impressões das duas hospedagens, cada uma no seu lugar** (lote X da
    /// F5, a impressão, commit D). `onde_estou_em` recebe a do guardado em
    /// terceiro e a da sala em quarto; o par trocado diria `Guardado` na sala e
    /// gravaria nela o sim de quem hospeda.
    ///
    /// Duas salas, e não um servidor guardado e uma sala, pela razão que
    /// `a_saida_despede_as_hospedagens` escreve: este arquivo só sobe
    /// hospedagem pelo ponto padrão. Sementes diferentes, chaves diferentes.
    #[tokio::test]
    async fn as_impressoes_saem_na_ordem_do_guardado_e_da_sala() {
        let guardado = Hospedagem::iniciar_sala(0, &[3; 32], "casa", None)
            .await
            .expect("o guardado não subiu");
        let sala = Hospedagem::iniciar_sala(0, &[4; 32], "dona", None)
            .await
            .expect("a sala não subiu");
        let do_guardado = guardado.impressao_digital().to_owned();
        let da_sala = sala.impressao_digital().to_owned();
        assert_ne!(
            do_guardado, da_sala,
            "as duas hospedagens saíram com a mesma chave, e este teste não mede a ordem"
        );

        let mut hospedagens = Hospedagens {
            guardado: Some(Hospedado {
                hospedagem: guardado,
                servidor: None,
            }),
            pessoal: Some(Hospedado {
                hospedagem: sala,
                servidor: None,
            }),
        };
        assert_eq!(
            hospedagens.impressoes(),
            (Some(do_guardado), Some(da_sala)),
            "`Hospedagens::impressoes` não devolve a chave do guardado primeiro e a da sala \
             depois: a sessão na sala passa por guardado, e o sim de quem hospeda vai a ela \
             (ADR 0045)"
        );

        for hospedado in [hospedagens.guardado.take(), hospedagens.pessoal.take()]
            .into_iter()
            .flatten()
        {
            hospedado.hospedagem.encerrar().await;
        }
    }

    /// **A ligação de `onde_estou_na`** (commit D). Os testes acima são puros:
    /// recebem as impressões prontas. Este prende o que a função da sessão
    /// passa a eles: a chave que a **conexão** conferiu em segundo, e as das
    /// hospedagens, na ordem de `impressoes()`, em terceiro e quarto. Trocar a
    /// chave da sessão pela do guardado diria `Guardado` em qualquer sessão
    /// com o guardado de pé, e o sim de quem hospeda iria a S.
    ///
    /// Olha a chamada e as ligações que a alimentam, sem espaço e sem as
    /// linhas de comentário, com as agulhas montadas por `concat!`.
    #[test]
    fn onde_estou_na_passa_a_chave_da_conexao_e_as_das_hospedagens() {
        let fonte = fonte_sem_comentarios();
        let corpo: String = corpo_de(&fonte, concat!("fn onde_estou_na", "(session"))
            .chars()
            .filter(|letra| !letra.is_whitespace())
            .collect::<String>()
            // A vírgula final que o rustfmt põe numa chamada partida em linhas.
            .replace(",)", ")");
        let ligacao = |nome: &str| {
            corpo
                .split_once(nome)
                .and_then(|(_, depois)| depois.split_once(';'))
                .map(|(ligacao, _)| ligacao.to_owned())
                .unwrap_or_default()
        };
        let da_sessao = ligacao(concat!("let", "impressao="));
        let das_hospedagens = ligacao(concat!("let(guardado,", "pessoal)="));
        assert!(
            da_sessao.starts_with(concat!("session.connection", "()"))
                && da_sessao.contains(concat!(".impressao_do_", "servidor()"))
                && das_hospedagens.contains(concat!(".impressoes", "()"))
                && corpo.contains(concat!(
                    "onde_estou_em(endereco.as_deref(),",
                    "impressao.as_deref(),guardado.as_deref(),pessoal.as_deref())"
                )),
            "`onde_estou_na` deixou de passar a `onde_estou_em` a chave que a conexão conferiu \
             em segundo e as das hospedagens, na ordem de `impressoes()`, em terceiro e \
             quarto: a sessão em S ou na sala passa por guardado, e o sim de quem hospeda vai \
             a ela (ADR 0045):\n{corpo}"
        );
    }
}

/// **A P7b: a API que o manifesto declara chega à coluna do banco** por quem a
/// grava a partir da tela.
///
/// O código entrou com a P7a (`4112f96b`), e o teste da tela ficou esperando a
/// fila do `main.rs`. Ele vai do pacote em disco, pelo mesmo `ler_por_hash` que
/// os dois comandos usam, até a linha que o banco devolve; o guarda de fonte
/// prende os dois comandos na mesma construção, que é o que sobra entre a tela e
/// este teste.
#[cfg(test)]
mod a_api_do_mod_habilitado {
    use super::*;

    /// Um pacote no cache de uma casa temporária, com a API que o manifesto
    /// declara. Devolve a casa e o hash do pacote.
    fn pacote_no_cache(api: u32) -> (std::path::PathBuf, String) {
        // Uma pasta por chamada: os testes deste módulo rodam em paralelo no mesmo
        // processo, e uma pasta só deixaria um apagar o pacote do outro.
        static QUAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = QUAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let raiz = std::env::temp_dir().join(format!("seele-app-p7b-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        let fonte = raiz.join("fonte");
        std::fs::create_dir_all(fonte.join("servidor")).expect("temporário");
        std::fs::write(
            fonte.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"seele/p7b","version":"1.0.0","api":{api},"repo":"https://github.com/seele/p7b","reach":[],"server":"servidor/main.js"}}"#
            ),
        )
        .expect("manifesto");
        std::fs::write(
            fonte.join("servidor/main.js"),
            "globalThis.aoPedir = () => '';",
        )
        .expect("metade de servidor");
        let lido =
            seele_ffi::mods::ler_pasta(&fonte.to_string_lossy()).expect("pacote válido recusado");
        let casa = raiz.join("casa");
        let guardado = casa.join(seele_ffi::mods::PACOTES).join(&lido.hash);
        std::fs::create_dir_all(guardado.join("servidor")).expect("cache");
        for arquivo in ["mod.json", "servidor/main.js"] {
            std::fs::copy(fonte.join(arquivo), guardado.join(arquivo)).expect("copiar");
        }
        (casa, lido.hash)
    }

    #[test]
    fn exigir_um_mod_grava_no_banco_a_api_que_o_manifesto_declara() {
        // A 4: aceita por este build, e diferente do zero de «não preenchida» e
        // da API deste build. Gravar uma das duas no lugar da do pacote reprova.
        let declarada = 4;
        assert!(
            declarada != 0 && declarada != seele_ffi::mods::MOD_API_VERSION,
            "o teste precisa de uma API que não é nem o zero nem a deste build"
        );
        let (casa, hash) = pacote_no_cache(declarada);
        let instalado = seele_ffi::mods::ler_por_hash(&casa.to_string_lossy(), &hash)
            .expect("o pacote não foi achado por hash");
        let banco = seele_server::persistence::Persistence::open(
            &seele_server::persistence::Location::Memory,
        )
        .expect("banco em memória");
        seele_server::persistence::mods::enable(&banco, &habilitado_de(instalado))
            .expect("exigir o MOD");
        let gravada = seele_server::persistence::mods::enabled(&banco)
            .expect("ler os exigidos")
            .into_iter()
            .find(|exigido| exigido.id == "seele/p7b")
            .map(|exigido| exigido.api);
        let _ = std::fs::remove_dir_all(casa.parent().unwrap_or(&casa));
        assert_eq!(
            gravada,
            Some(declarada),
            "a coluna `api` não recebeu a API que o manifesto declara: o anúncio diria outra"
        );
    }

    /// A terceira escrita da API, a da lista do cache que a tela de MODs
    /// desenha: o mesmo pacote, pelo mesmo `listar_por_conteudo` do comando.
    #[test]
    fn a_lista_do_cache_mostra_a_api_que_o_manifesto_declara() {
        let declarada = 4;
        let (casa, hash) = pacote_no_cache(declarada);
        let pasta = casa.to_string_lossy().into_owned();
        let instalado = seele_ffi::mods::listar_por_conteudo(&pasta)
            .into_iter()
            .find(|instalado| instalado.hash == hash)
            .expect("o pacote não está na lista do cache");
        let na_tela = pacote_no_cache_de(&pasta, instalado, &[]);
        let _ = std::fs::remove_dir_all(casa.parent().unwrap_or(&casa));
        assert_eq!(
            na_tela.api, declarada,
            "a lista do cache mostra outra API que a do manifesto, e a tela e o banco discordam"
        );
    }

    #[test]
    fn os_dois_comandos_que_gravam_passam_pela_mesma_construcao() {
        let fonte = fonte_sem_comentarios();
        // Montada, e não escrita: escrita aqui, a própria agulha seria achada
        // no texto deste teste.
        let a_mao = concat!("EnabledMod", " {");
        for comando in [
            "async fn habilitar_mod(",
            "async fn aplicar_conjunto_de_mods(",
        ] {
            let corpo = corpo_de(&fonte, comando);
            assert!(
                corpo.contains("habilitado_de(") && !corpo.contains(a_mao),
                "`{comando}` deixou de gravar pela construção que leva a API do manifesto"
            );
        }
        let lista = corpo_de(&fonte, "async fn pacotes_no_cache(");
        assert!(
            lista.contains("pacote_no_cache_de(") && !lista.contains(concat!("PacoteNoCache", " {")),
            "`pacotes_no_cache` deixou de montar a lista pela construção que leva a API do manifesto"
        );
    }
}
