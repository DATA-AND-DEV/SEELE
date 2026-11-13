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

// O aviso antes de atualizar para outra linha de servidor (E4 da 0.17). É
// do compat-validation, como o bloco do atualizador abaixo (fronteiras.md, §7).
mod atualizador;
mod cargos;
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
// As saídas de quem teve a gravação de tela negada pelo macOS: os Ajustes e a
// redefinição da permissão deste app (o laço relatado na 1.0.0).
mod gravacao_de_tela;
mod icone;
mod ligar;
mod maquina;
mod mods;
mod notificacao;
mod perfil_padrao;
// A imagem escolhida que espera o recorte, e a prévia que a janela desenha.
mod recorte;
// A redução de imagem de MOD mora no `seele-sessao` desde a Tarefa 2.6 da F4.
use seele_ffi::reducao;
mod sala;
// A senha do servidor, como quem hospeda a escolhe: a regra, uma só, da tela
// de preparar e do painel A PORTA.
mod senha_do_servidor;
mod servidores;
// O ANTES DE HOSPEDAR e o portão da conversão: a cópia de segurança do estado
// atual antes de converter um banco (E2 da 0.17).
mod antes_de_hospedar;
mod audio_de_mentira;
mod escolhas_de_mods;
// As sessões cliente, todas iguais, e o foco de voz da máquina (E3 da 0.17).
mod sessoes;
mod tecla_global;
mod toque;
// O cenário da multi-sessão contra servidores de verdade (E3 da 0.17).
#[cfg(test)]
mod varias_sessoes_testes;
// O PERFIS da API 5 que a 0.15 deixou ligado, no ANTES DE HOSPEDAR da 0.19:
// incorporado ao produto, sem ATUALIZAR, e os perfis trazidos na subida.
#[cfg(test)]
mod perfis_incorporado_testes;
mod versoes;

use std::sync::{Arc, Mutex};

// As regras da prévia de link que não buscam nada moram no `seele-sessao`
// desde a Tarefa 2.6 da F4: o navegador as aplica igual (`seele_ffi::preview`).
use seele_ffi::preview::{e_endereco_reservado, host_de_previa, midia_declarada_pela_pagina};
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
    /// **As metades de máquina** (API 8 parte 3): as que o dono desta máquina
    /// aceitou e estão de pé, e os pedidos que a tela ainda não respondeu.
    /// Aqui e não na janela pelo mesmo motivo dos MODs nativos: o runtime é
    /// nativo, e quem o desmonta tem de ser quem o criou. Ver [`maquina`].
    maquinas: maquina::Maquinas,
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

    /// **Todas as sessões cliente desta janela** (E3 da 0.17), e o foco de
    /// voz da máquina. Os comandos de conversa acham a conexão aqui, pelo
    /// carimbo que trouxeram ([`sessoes::Registro::conexao`]).
    sessoes: sessoes::Registro,
    /// **A ordem dos pedidos de [`ver_sessao`]**: a instância da janela (que o
    /// backend emite na carga dela) e o maior número já aplicado nela. Um
    /// pedido menor, ou de uma instância velha, é recusado e não leva a vista
    /// para trás. Ver [`sessoes::DominioDaVista`].
    dominio_da_vista: sessoes::DominioDaVista,
    /// **A conexão da sessão na vista**: a mesma que está em [`Self::sessoes`],
    /// e só ela. É por aqui que falam os comandos que são da vista — os MODs,
    /// conferidos pela [`Self::geracao`] da vista —, e nunca um comando de
    /// conversa, que leva carimbo.
    ///
    /// Até a 0.16 esta era a única conexão da janela.
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
            let soltos = guardados.soltar_geracao(nova.saturating_sub(1)).len();
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

    /// O carimbo da sessão na vista, para um teste mandar um comando como a
    /// janela que a mostra mandaria. Sem vista, um carimbo que nenhuma sessão
    /// tem.
    #[cfg(test)]
    fn carimbo_da_vista(&self) -> sessoes::CarimboDaJanela {
        self.sessoes
            .vista()
            .and_then(|vista| self.sessoes.buscar(vista))
            .map_or(
                sessoes::CarimboDaJanela {
                    sessao: 0,
                    geracao: 0,
                },
                |sessao| sessoes::CarimboDaJanela::de(sessao.conexao.carimbo()),
            )
    }

    /// A conexão da sessão na vista, ou por que não há.
    ///
    /// **Só para o que é da vista** (os MODs, conferidos pela geração da
    /// vista). Um comando de conversa acha a conexão pelo carimbo:
    /// [`Self::conexao`].
    fn connection(&self) -> Result<Arc<Connection>, ConnectionError> {
        self.connection
            .lock()
            .map_err(|_| ConnectionError::NotConnected)?
            .clone()
            .ok_or(ConnectionError::NotConnected)
    }

    /// **A conexão de um comando de sessão**, pelo carimbo que ele trouxe, e
    /// nunca pela vista. Ver [`sessoes::Registro::conexao`].
    fn conexao(
        &self,
        carimbo: sessoes::CarimboDaJanela,
    ) -> Result<Arc<Connection>, ConnectionError> {
        self.sessoes.conexao(carimbo)
    }

    /// Põe a sessão `sessao` na vista: a conexão, o endereço e o alvo dela
    /// passam a ser os da vista. `None` esvazia a vista.
    ///
    /// Quem chama já revogou a geração da vista (os MODs da anterior param).
    fn por_na_vista(&self, sessao: Option<&sessoes::SessaoCliente>) {
        if let Ok(mut slot) = self.connection.lock() {
            self.por_na_vista_com(&mut slot, sessao);
        }
    }

    /// O mesmo, com o cadeado da conexão da vista já na mão (o `connect`
    /// confere a geração e grava sob ele, pelo guarda E54).
    fn por_na_vista_com(
        &self,
        slot: &mut Option<Arc<Connection>>,
        sessao: Option<&sessoes::SessaoCliente>,
    ) {
        *slot = sessao.map(|sessao| Arc::clone(&sessao.conexao));
        if let Ok(mut onde) = self.endereco_da_sessao.lock() {
            *onde = sessao.map(|sessao| sessao.endereco.clone());
        }
        if let Ok(mut alvo) = self.alvo.lock() {
            *alvo = sessao.and_then(sessoes::SessaoCliente::alvo);
        }
        // A busca é da conversa que estava na tela.
        if let Ok(mut busca) = self.busca.lock() {
            *busca = None;
        }
        self.sessoes
            .por_na_vista(sessao.map(sessoes::SessaoCliente::id));
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
    /// **O que o manifesto deste pacote deixa o MOD chamar**: as capacidades da
    /// API dele mais as que os poderes dão (`tunel` para quem declara
    /// `porta`). Guardadas com a instância e não relidas na ativação: quem
    /// decide o que este MOD pode chamar tem de ser a mesma leitura que provou
    /// o hash dele — e é por aqui que o
    /// prelúdio monta `SeeleUI`, e que `tunel_entrar` confere o poder antes de
    /// falar com o servidor.
    capacidades: Vec<&'static str>,
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

    /// Este MOD está de pé nesta geração, e o manifesto dele deixa chamar
    /// `capacidade`? É a segunda porta do que o prelúdio só omite (a primeira é
    /// `podePedir`, na janela): um comando que abre algo na máquina de quem usa
    /// confere o poder aqui, com o que o Rust leu do pacote, e não com o que a
    /// janela diz.
    fn pode(&self, geracao: u64, id: &str, capacidade: &str) -> bool {
        self.vivos.values().any(|instancia| {
            instancia.geracao == geracao
                && instancia.id == id
                && instancia.estado == EstadoNativo::Ativa
                && instancia.capacidades.contains(&capacidade)
        })
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
///
/// # Desde a 0.17: uma ponte por sessão, e todo evento carimbado
///
/// Com várias sessões de pé, «a geração de pé» deixou de dizer de quem é um
/// evento: a vista troca sem que nenhuma sessão acabe. Cada ponte é de **uma**
/// sessão ([`sessoes::Marcas`]) e manda **todo** evento à janela como
/// [`EventoDeSessao`]: o `SessaoId`, a geração do enlace e se a sessão estava
/// na vista. Não há evento cru. A janela aplica à tela só o da sessão que ela
/// mostra, e o resto vai para a placa daquela sessão na trilha. Uma sessão que
/// já saiu, ou foi substituída na entrada, tem as marcas apagadas: o que ainda
/// vier dela é descartado e contado, como antes.
struct Bridge<R: tauri::Runtime = tauri::Wry> {
    app: AppHandle<R>,
    /// As marcas da sessão desta ponte: o `SessaoId`, se ela ainda vale, e o
    /// que a trilha lê.
    marcas: Arc<sessoes::Marcas>,
    /// Qual sessão está na vista agora, compartilhado com o registro.
    vista: Arc<std::sync::atomic::AtomicU64>,
    /// A geração da vista, compartilhada com a `Session`: o fim da sessão que
    /// está na vista a revoga, e os MODs dela param.
    de_pe: Arc<std::sync::atomic::AtomicU64>,
    /// Onde contar o que foi descartado, compartilhado com a `Session`.
    descartados: Arc<std::sync::atomic::AtomicU64>,
    /// A tentativa da janela que pediu esta entrada, quando ela disse qual.
    /// Vai em cada etapa da chegada ([`carga_do_evento`]).
    tentativa: Option<u64>,
    /// **Por onde esta sessão entrou**: o servidor SEELE de um pedido de
    /// máquina. Na ponte, e não lido do registro, porque o servidor reenvia os
    /// pedidos pendentes durante o `connect` — antes de a sessão estar lá.
    alvo: String,
}

/// O canal do sinal «a lista de sessões mudou» ([`MudancaNaSessao`]).
const CANAL_DAS_SESSOES: &str = "seele://sessoes";

/// **Todo evento como ele vai à janela** (E3 da 0.17): de qual sessão, de qual
/// volta do enlace, e se ela estava na vista quando ele saiu.
#[derive(Debug, Clone, serde::Serialize)]
struct EventoDeSessao {
    /// O `SessaoId`.
    sessao: u64,
    /// A geração do enlace daquela sessão.
    geracao: u64,
    /// A sessão estava na vista quando o evento saiu. Informativo: quem decide
    /// onde ele vai é a janela, pelo `sessao`.
    vista: bool,
    /// **A época da vista** no instante em que o evento saiu (a mesma das
    /// respostas `NaSessao`): a janela que já trocou de vista sabe que o
    /// evento é de antes da troca. Pedido dos cargos (G/F) para o evento de
    /// cargos, e vale para todos.
    epoca: u64,
    /// O evento, como sempre foi ([`carga_do_evento`]).
    evento: serde_json::Value,
}

/// **O sinal da trilha**: alguma coisa desta sessão mudou, releia `sessoes()`.
/// Sem conteúdo nenhum da conversa.
#[derive(Debug, Clone, serde::Serialize)]
struct MudancaNaSessao {
    /// O `SessaoId`.
    sessao: u64,
    /// O que mudou: `entrou`, `mensagens`, `presenca`, `enlace`, `aviso`,
    /// `cargos`, `encerrada`, `foco`, `vista` ou `saiu`.
    motivo: &'static str,
}

/// **O motivo da trilha de um evento**, ou nenhum: o que faz a placa de uma
/// sessão (inclusive a do fundo) reler `sessoes()`.
fn motivo_da_trilha(event: &Event, reconectou: bool) -> Option<&'static str> {
    match event {
        Event::Ended { .. } => Some("encerrada"),
        Event::MessagesChanged => Some("mensagens"),
        Event::RosterChanged => Some("presenca"),
        Event::NoticeRaised { .. } => Some("aviso"),
        // **Os cargos daquela sessão mudaram** (F: o quadro, as suas
        // permissões ou uma recusa): a placa da sessão do fundo relê, e a
        // camada de cargos dela relê pela marca (`cargos_revision`), sem laço.
        Event::CargosChanged { .. } => Some("cargos"),
        Event::TelemetryChanged if reconectou => Some("enlace"),
        _ => None,
    }
}

/// Os eventos depois dos quais «estou numa sala de voz» pode ter mudado.
///
/// Os que não entram são os de alta frequência que não dizem nada de sala
/// (telemetria, quadros de tela, mensagens): um aviso por quadro acordaria a
/// conferida sessenta vezes por segundo. A rede de segurança
/// (`tecla_global::REDE_DE_SEGURANCA`) cobre o que esta lista esquecer.
fn muda_a_sala_da_tecla_global(event: &Event) -> bool {
    matches!(
        event,
        Event::RosterChanged
            | Event::ChannelsChanged
            | Event::Ended { .. }
            | Event::CaiuComMotivo { .. }
            | Event::ConnectStageChanged { .. }
    )
}

#[path = "testes/os_eventos_que_avisam_a_tecla_global.rs"]
#[cfg(test)]
mod os_eventos_que_avisam_a_tecla_global;

#[path = "testes/o_motivo_da_trilha.rs"]
#[cfg(test)]
mod o_motivo_da_trilha;

/// Emite o sinal da trilha. Uma janela fechada não é motivo de log.
fn avisar_a_trilha<R: tauri::Runtime>(app: &AppHandle<R>, sessao: u64, motivo: &'static str) {
    let _ = app.emit(CANAL_DAS_SESSOES, MudancaNaSessao { sessao, motivo });
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
    /// Sem carimbo não há como dizer de quem é: só uma FFI antiga chamaria
    /// isto, e o evento vai com o carimbo que a sessão tinha na entrada.
    fn on_event(&self, event: Event) {
        self.on_event_carimbado(
            seele_ffi::Carimbo {
                sessao: self.marcas.sessao,
                geracao: self
                    .marcas
                    .geracao_vista
                    .load(std::sync::atomic::Ordering::Acquire),
            },
            event,
        );
    }

    fn on_event_carimbado(&self, carimbo: seele_ffi::Carimbo, event: Event) {
        use std::sync::atomic::Ordering;
        // Um carimbo de outra sessão nesta ponte seria um erro de fiação: ele é
        // contado e não chega à janela com o número errado.
        if !self.marcas.viva.load(Ordering::Acquire) || carimbo.sessao != self.marcas.sessao {
            self.marcas.descartados.fetch_add(1, Ordering::Relaxed);
            self.descartados.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // **Os eventos da máquina não vão crus à janela** (API 8 parte 3): o
        // pedido vai como `maquina-pedido`, com o apelido no lugar do número, e
        // o aviso é da metade de máquina e de mais ninguém. O fim da sessão
        // passa por aqui também (para as metades dela) e segue para a janela.
        if maquina::ao_evento(
            &self.app,
            carimbo.sessao,
            carimbo.geracao,
            &self.alvo,
            &event,
        ) {
            return;
        }
        let na_vista = self.vista.load(Ordering::Acquire) == self.marcas.sessao;
        let reconectou = self.marcas.anotar_geracao(carimbo.geracao);
        let carga = EventoDeSessao {
            sessao: carimbo.sessao,
            geracao: carimbo.geracao,
            vista: na_vista,
            epoca: self.de_pe.load(Ordering::Acquire),
            evento: carga_do_evento(&event, self.tentativa),
        };
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
        // O fim fica anotado antes de sair: quem relê a trilha logo depois do
        // evento já a encontra encerrada.
        if let Event::Ended { reason } = &event {
            *self
                .marcas
                .encerrada
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(*reason);
        }
        // A failed emit means the window is gone, which is not worth a log channel
        // per event during shutdown.
        let _ = self.app.emit(EVENT_CHANNEL, carga);
        // Entrar e sair de sala, cair, ser movido: a tecla de fora da janela
        // só vale em sala. Só avisa; quem confere é outra thread.
        if muda_a_sala_da_tecla_global(&event) {
            tecla_global::avisar(&self.app);
        }
        // O sinal da trilha, para o que a placa de uma sessão mostra.
        if matches!(event, Event::MessagesChanged) && !na_vista {
            self.marcas.mudou.store(true, Ordering::Release);
        }
        if let Some(motivo) = motivo_da_trilha(&event, reconectou) {
            avisar_a_trilha(&self.app, self.marcas.sessao, motivo);
        }
        if reconectou {
            tracing::info!(
                sessao = carimbo.sessao,
                geracao = carimbo.geracao,
                na_vista,
                "uma sessão voltou de uma queda: a vista, o foco de voz e as outras sessões \
                 ficam como estavam"
            );
        }
        // **Só a sessão da vista revoga a vista.** O fim de uma sessão do fundo
        // não para os MODs da que está na tela.
        if acabou && na_vista {
            let morta = self.de_pe.fetch_add(1, Ordering::AcqRel);
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

#[path = "testes/a_etapa_leva_a_tentativa.rs"]
#[cfg(test)]
mod a_etapa_leva_a_tentativa;

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
    /// **De qual sessão é esta entrada** (E3 da 0.17): é com este carimbo que a
    /// janela manda todo comando de conversa a ela, e é por ele que a janela
    /// separa os eventos dela dos das outras sessões.
    carimbo: seele_ffi::Carimbo,
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
    //
    // **Desde a 0.17 (E3), entrar noutro servidor não recusa nem derruba.** A
    // sessão que estava na vista continua conectada no registro; esta é mais
    // uma, e vai para a vista quando entrar. O que esta linha revoga é a
    // **vista** (os MODs da que estava na tela param), e não sessão nenhuma.
    // O guarda E54 continua: uma tentativa que outra substituiu — outra
    // entrada começou, ou a pessoa trocou a vista enquanto esta andava — não
    // entra.
    let geracao = {
        let slot = session.connection.lock();
        let geracao = session.revogar();
        drop(slot);
        geracao
    };
    // **O `SessaoId` nasce aqui, na tentativa** (E3): as etapas da chegada já
    // saem carimbadas com ele.
    let sessao_id = seele_ffi::SessaoId::nova();
    let marcas = sessoes::Marcas::nova(sessao_id);

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
                        conhecido.bilhete_para_voltar().unwrap_or_else(|erro| {
                            tracing::warn!(%erro, "bilhete guardado inválido: não será usado; a impressão e os demais caminhos foram preservados");
                            None
                        }),
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
        capture_device: audio_de_mentira::aparelho()
            .or_else(|| preferencias(&app).and_then(|p| p.capture().map(str::to_owned))),
        // A saída de som, pela mesma política e lida do mesmo lugar. Os dois
        // ajustes são independentes: um fone que ficou na outra sala não pode
        // custar o microfone que a pessoa escolheu.
        playback_device: audio_de_mentira::aparelho()
            .or_else(|| preferencias(&app).and_then(|p| p.playback().map(str::to_owned))),
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
        marcas: Arc::clone(&marcas),
        vista: session.sessoes.vista_atomica(),
        de_pe: Arc::clone(&session.geracao),
        descartados: Arc::clone(&session.eventos_de_geracao_morta),
        tentativa,
        alvo: alvo.clone(),
    }) as Arc<dyn EventListener>;
    let atento = Arc::clone(&ponte);
    // **O CANCELAR desta tentativa** (tarefa 28 da F7): a vaga fica na sessão
    // enquanto a chegada anda, e o `cancelar_ligacao` a acha lá.
    let em_voo = ligar::LigacaoEmVoo::abrir(&session);
    let cancelamento = em_voo.cancelamento();
    // **No foco de voz da máquina** (E3): a voz desta sessão só abre quando
    // ela tiver o microfone. Sozinha, ela o tem; com outras, só por um ato da
    // pessoa (`focar_voz`, ou entrar numa sala de voz nela).
    let foco = session.sessoes.foco();
    let resultado = tauri::async_runtime::spawn_blocking(move || {
        Connection::connect_com_foco(
            config,
            Some(atento),
            Some(cancelamento),
            Some(foco),
            Some(sessao_id),
        )
    })
    .await
    .unwrap_or_else(|_| Err(ConnectFailure::from(ConnectionError::Unreachable)));
    drop(em_voo);
    let (connection, veredito) = resultado.map_err(|falha| {
        // Uma tentativa que não entrou não fala mais: o que ainda viesse dela
        // seria de uma sessão que não existe.
        marcas
            .viva
            .store(false, std::sync::atomic::Ordering::Release);
        FalhaDaEntrada {
            falha,
            de_onde: DeOndeVeio {
                origem_da_impressao,
                a_lista_guardou: false,
            },
            substituida: false,
        }
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
        marcas
            .viva
            .store(false, std::sync::atomic::Ordering::Release);
        return Err(largar_a_substituida(
            &connection,
            geracao,
            session.geracao(),
            &alvo,
            origem_da_impressao,
        ));
    }

    connection.subscribe(ponte);

    // **O mudo e o isolamento são da máquina** (E3, ADR 0031): a sessão nova
    // nasce como as outras estão. Sem isto, quem estava mudo num servidor
    // entraria noutro com o microfone aberto, e uma troca de foco para lá
    // transmitiria.
    if let Some(outra) = session.sessoes.todas().first() {
        let retrato = outra.conexao.snapshot();
        if retrato.muted {
            let _ = connection.set_muted(true);
        }
        if retrato.total_isolation {
            let _ = connection.set_total_isolation(true);
        }
    }

    // O modo de microfone, a supressão de ruído e a sensibilidade gravados,
    // aplicados assim que há o que aplicar.
    //
    // Aqui e não num campo de `connect`: a sessão nasce fechada — push-to-talk
    // — e só então abre para o que a pessoa escolheu ou, sem escolha, para o
    // padrão desta instalação (a voz para quem chegou, a tecla para quem já
    // usava: `assentar_o_padrao_do_microfone`, no `setup`). A ordem é a segura;
    // a inversa teria um instante em que um microfone poderia estar aberto
    // antes de alguém ter dito que podia.
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
    //
    // **Desde a 0.17 (E3), «gravar» é entrar no registro e ir para a vista.**
    // As outras sessões continuam como estavam: a que estava na vista vai para
    // o fundo, conectada.
    let substituida = match session.connection.lock() {
        Ok(mut slot) if session.confere_geracao(geracao).is_ok() => {
            let nova = session.sessoes.inserir(
                Arc::clone(&connection),
                alvo.clone(),
                None,
                Arc::clone(&marcas),
            );
            session.por_na_vista_com(&mut slot, Some(&nova));
            drop(slot);
            avisar_a_trilha(&app, sessao_id, "entrou");
            // Os pedidos de máquina que chegaram durante o `connect` agora têm
            // conexão e lista de pessoas.
            maquina::sessao_entrou(&app, sessao_id);
            None
        }
        Ok(_) => Some(connection),
        Err(_) => None,
    };
    if let Some(connection) = substituida {
        marcas
            .viva
            .store(false, std::sync::atomic::Ordering::Release);
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
    //
    // Só se esta sessão ainda é a vista (E3): o espelho da vista não pode
    // receber o endereço de uma sessão que já foi para o fundo.
    if session.sessoes.vista() == Some(sessao_id) {
        if let Ok(mut onde) = session.endereco_da_sessao.lock() {
            *onde = Some(alvo.clone());
        }
    }
    // Falso até a lista gravar, lá embaixo, a impressão que a conexão aceitou.
    let mut a_lista_guardou = false;
    // Uma ligação vai para a agenda, e não para cá: é `anotar_a_chegada` quem
    // decide, pela origem da tentativa, e quem guarda o alvo da sessão.
    let visita = ligar::anotar_a_chegada(&session, &casa, &alvo, ligou_para);
    // E «onde voltar» fica com a sessão no registro, pela mesma decisão: é o
    // que a vista dela volta a ter quando a pessoa trocar para ela de novo.
    if let Some(nova) = session.sessoes.buscar(sessao_id) {
        nova.guardar_alvo(visita.then(|| alvo.clone()));
    }
    if visita {
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
        carimbo: connection_carimbo(&session, sessao_id),
        snapshot,
        veredito,
        geracao,
        de_onde: DeOndeVeio {
            origem_da_impressao,
            a_lista_guardou,
        },
    })
}

/// O carimbo de agora de uma sessão do registro; o da entrada, se ela já saiu.
fn connection_carimbo(session: &Session, sessao: u64) -> seele_ffi::Carimbo {
    session
        .sessoes
        .buscar(sessao)
        .map_or(seele_ffi::Carimbo { sessao, geracao: 1 }, |sessao| {
            sessao.conexao.carimbo()
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
            // A sessão da vista sai do registro também (E3): solta é solta, e
            // não uma sessão a mais esquecida no fundo.
            if let Some(vista) = session.sessoes.vista() {
                session.sessoes.tirar(vista);
                maquina::sessao_saiu_sem_tela(&session.maquinas, vista);
            }
            let solta = slot.take();
            session.por_na_vista_com(&mut slot, None);
            solta
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
#[path = "testes/a_entrada_velha_que_chega_tarde.rs"]
#[cfg(test)]
mod a_entrada_velha_que_chega_tarde;

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
#[path = "testes/o_apelido_grande_demais_no_hello.rs"]
#[cfg(test)]
mod o_apelido_grande_demais_no_hello;

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
    /// **A geração desta hospedagem** (a portaria do E7): sobe a cada servidor
    /// que esta janela levanta, e nunca se repete no processo. Parar A e subir
    /// B (ou A de novo) dá outra geração, mesmo com a mesma chave e a mesma
    /// porta: é o que deixa a decisão aberta para A ser recusada em B.
    geracao: u64,
}

impl Hospedado {
    /// Uma hospedagem de pé, com uma geração nova.
    fn novo(hospedagem: seele_server::hospedagem::Hospedagem, servidor: Option<String>) -> Self {
        static GERACOES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            hospedagem,
            servidor,
            geracao: GERACOES.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Quem esta hospedagem é, para a tela e para os atos da portaria.
    fn identidade(&self) -> HospedagemDaPorta {
        HospedagemDaPorta {
            servidor: self.servidor.clone(),
            geracao: self.geracao,
        }
    }
}

/// **A hospedagem exata de que a porta fala** (E7, a portaria): o servidor
/// guardado e a geração da hospedagem. O `estado_da_porta` a devolve, e o
/// `decidir_pedido` e o `revogar_admissao` a exigem de volta: uma decisão
/// aberta para uma hospedagem nunca é aplicada noutra.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct HospedagemDaPorta {
    /// O servidor guardado, ou `None` para o banco de sempre.
    servidor: Option<String>,
    /// A geração da hospedagem (ver [`Hospedado::geracao`]).
    geracao: u64,
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

/// A lista de amigos (0.17): os contatos que a pessoa adicionou. Ligar para um
/// é o `connect` com o `alvo` dele, o mesmo caminho do código colado.
///
/// # Errors
///
/// [`sala::FalhaNaAgenda::NaoGravou`] quando a agenda não abre.
#[tauri::command]
fn amigos(app: AppHandle) -> Result<Vec<sala::ContatoNaTela>, sala::FalhaNaAgenda> {
    sala::amigos_em(&config_dir(&app))
}

/// Põe um código na lista de amigos (0.17), com o nome, quando vem um; um nome
/// vazio mantém o que a agenda já tinha.
///
/// # Errors
///
/// [`sala::FalhaNaAgenda`]: o código não fecha, é o desta máquina, ou a agenda
/// não gravou.
#[tauri::command]
fn adicionar_amigo(
    app: AppHandle,
    codigo: String,
    nome: Option<String>,
) -> Result<(), sala::FalhaNaAgenda> {
    let adicionado = sala::adicionar_amigo_em(&config_dir(&app), &codigo, nome.as_deref());
    tracing::info!(?adicionado, "amigo adicionado à agenda");
    adicionado
}

/// Tira um código da lista de amigos (0.17), e só: o nome, o histórico, o
/// bloqueio e o pin ficam.
///
/// # Errors
///
/// [`sala::FalhaNaAgenda`]: o código não fecha, ou a agenda não gravou.
#[tauri::command]
fn tirar_dos_amigos(app: AppHandle, codigo: String) -> Result<(), sala::FalhaNaAgenda> {
    let tirado = sala::tirar_dos_amigos_em(&config_dir(&app), &codigo);
    tracing::info!(?tirado, "amigo tirado da agenda");
    tirado
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
    /// **O banco deste servidor não pôde ser lido antes de hospedar** (o ANTES
    /// DE HOSPEDAR, `antes_de_hospedar`): não é SQLite, ou não se lê. Nunca
    /// «pronto»: nada sobe e nada é convertido até a pessoa resolver.
    DadosNaoVerificados {
        /// O motivo, para «Detalhes técnicos».
        motivo: String,
        /// O que a tela oferece: `tentar-de-novo`, `abrir-a-pasta`.
        acoes: Vec<&'static str>,
    },
    /// **A cópia de segurança do estado atual não ficou pronta**, e hospedar
    /// converteria os dados: nada sobe e nada é convertido, e o banco original
    /// fica como estava. A etapa diz onde parou (`Copiar`, `Verificar`).
    CopiaDeSeguranca {
        /// Onde parou e por quê.
        falha: antes_de_hospedar::FalhaNaCopia,
    },
    /// **Uma escolha de MOD do ANTES DE HOSPEDAR não se aplicou** (P1 da
    /// 0.17), depois da cópia: o pacote saiu do cache, ou o banco não gravou.
    /// Nada sobe, e a escolha continua pendente; a cópia feita fica.
    EscolhaDeModNaoAplicou {
        /// O MOD e o motivo, para «Detalhes técnicos».
        motivo: String,
    },
    /// A porta 8383 está ocupada — quase sempre outro SEELE aberto.
    PortaOcupada,
    /// Qualquer outro motivo para o servidor não subir.
    NaoSubiu,
    /// **Os dados deste servidor são de uma versão mais nova do SEELE** (spec
    /// da 0.16, §8): o banco está num esquema que esta versão não conhece, e o
    /// servidor o recusou sem tocar nele (`persistence::BancoDoFuturo`). Quem
    /// resolve é atualizar o app; os números vão ao registro e à tela, para
    /// quem relata.
    BancoDoFuturo {
        /// O esquema gravado no banco.
        gravado: i64,
        /// O maior que esta versão conhece.
        conhecido: i64,
    },
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
    /// **Um MOD ligado neste servidor que este app não roda** (o corte da API
    /// 6; tarefa 13 do plano da F3, Step 5b).
    ///
    /// Conferido antes de subir, como o nome e a revogação: subido, o servidor
    /// recusaria todo app 0.16 com `ModsDeOutraApi`, e quem hospeda só ganhava
    /// uma queixa no registro — «o produto sabe e não conta». Cada um com o
    /// identificador e a API que o pacote declara, ou `None` quando o pacote
    /// não está nesta máquina; a frase e as três saídas são da tela.
    ModsDeApiAntiga {
        /// `(id, api)`, na ordem do banco.
        mods: Vec<(String, Option<u32>)>,
    },
    /// **A senha escolhida na tela de preparar não serve**, e por quê.
    ///
    /// Recusada antes de o servidor subir, como o nome: subir e depois recusar
    /// deixaria no ar, no modo portão, um servidor que quem hospeda pediu com
    /// senha. O motivo, e nunca a senha (`senha_do_servidor::SenhaRecusada`).
    SenhaRecusada(senha_do_servidor::SenhaRecusada),
    /// **A senha de um servidor novo chegou a um servidor que já tinha a porta
    /// dele**: uma senha, ou a portaria já decidida. Nada subiu e nada mudou.
    /// A tela de preparar só pergunta ao criar; a senha de um servidor que já
    /// existe se troca em CONFIGURAÇÕES · A PORTA, com ele de pé.
    PortaJaEscolhida,
}

/// A senha de um servidor novo chegou a um servidor que já tinha a porta dele
/// (`FalhaAoHospedar::PortaJaEscolhida`): o erro do `antes_de_aceitar`, que o
/// [`classificar`] reconhece na cadeia.
#[derive(Debug)]
struct PortaJaEscolhida;

impl std::fmt::Display for PortaJaEscolhida {
    fn fmt(&self, formatador: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatador.write_str("este servidor já tem a porta escolhida")
    }
}

impl std::error::Error for PortaJaEscolhida {}

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
fn servidores_guardados(app: AppHandle) -> Result<Vec<servidores::Servidor>, String> {
    servidores::listar_conferido(&config_dir(&app))
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

/// **As saídas 1 e 3 de quem hospeda com um MOD de API antiga**: atualizar o
/// MOD ou desligá-lo, com o servidor parado (tarefa 13 do plano da F3, Step
/// 5b; os limites da orquestradora de 15/10).
///
/// A regra e os limites são de [`mods::resolver_mod_de_api_antiga`], provados
/// lá com a pasta da 0.15.1. Aqui, o «no ar» lido da janela, pelo mesmo campo
/// do [`apagar_servidor`]: com um servidor guardado de pé nesta janela, nada
/// é aberto.
///
/// # Errors
///
/// [`mods::FalhaAoResolverModAntigo`], uma variante por motivo.
#[tauri::command]
async fn resolver_mod_de_api_antiga(
    app: AppHandle,
    session: State<'_, Session>,
    servidor: Option<String>,
    id: String,
    saida: mods::SaidaDoModAntigo,
) -> Result<(), mods::FalhaAoResolverModAntigo> {
    let no_ar = match session.hospedagens.lock() {
        Ok(hospedagens) => hospedagens.guardado.is_some(),
        // Sem saber, recusar: mexer no banco de um servidor de pé é o que a
        // recusa existe para impedir.
        Err(_) => true,
    };
    let config = config_dir(&app);
    // Só lê e estagia (P1 da 0.17): nada de abrir o banco para escrever. Fora
    // da thread da janela porque lê o pacote e o banco do disco.
    tauri::async_runtime::spawn_blocking(move || {
        mods::resolver_mod_de_api_antiga(&config, servidor.as_deref(), &id, &saida, no_ar)
    })
    .await
    .map_err(|erro| mods::FalhaAoResolverModAntigo::NaoGuardou(erro.to_string()))?
}

/// **Desfaz a escolha estagiada para um MOD deste servidor** (P1 da 0.17): o
/// banco nunca foi tocado, então desfazer é só esquecer o que o `hospedar`
/// aplicaria. O «DESFAZER» do ANTES DE HOSPEDAR (pedido do ui-ux).
///
/// # Errors
///
/// [`mods::FalhaAoResolverModAntigo::NaoGuardou`], com o motivo do disco.
#[tauri::command]
fn desfazer_escolha_de_mod(
    app: AppHandle,
    servidor: Option<String>,
    id: String,
) -> Result<(), mods::FalhaAoResolverModAntigo> {
    desfazer_escolha_de_mod_em(&config_dir(&app), servidor.as_deref(), &id)
}

/// O corpo do [`desfazer_escolha_de_mod`], sem o `AppHandle`: o servidor tem de
/// ser um guardado desta máquina (a mesma regra do resolvedor; revisão do
/// Codex, path traversal), e uma retirada que não chega ao disco é erro.
fn desfazer_escolha_de_mod_em(
    config: &str,
    servidor: Option<&str>,
    id: &str,
) -> Result<(), mods::FalhaAoResolverModAntigo> {
    mods::servidor_guardado(config, servidor)?;
    escolhas_de_mods::retirar(std::path::Path::new(config), servidor, id)
        .map_err(mods::FalhaAoResolverModAntigo::NaoGuardou)
}

/// **O DESFAZER só desfaz no servidor guardado que existe** (revisão do Codex):
/// um id desconhecido é recusado antes de qualquer caminho.
#[path = "testes/o_desfazer_confere_o_servidor.rs"]
#[cfg(test)]
mod o_desfazer_confere_o_servidor;

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
fn criar_servidor(app: AppHandle, nome: String) -> Result<servidores::Servidor, String> {
    servidores::criar_conferido(
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
    // **O resultado vai ao `seele.log`, nos dois sentidos** (medida com tela no
    // Mac, 23/10): um ABRIR que não lançou nada não deixava rastro nenhum. Sem
    // o link, que pode levar um convite de uso único: só se havia um.
    let com_link = link.as_deref().is_some_and(|l| !l.trim().is_empty());
    let pedido = format!("{versao:?} / {linha:?}");
    let resultado =
        abrir_versao_por_dentro(&app, versao, linha, hospedar, nome_publico, link).await;
    match &resultado {
        Ok(()) => tracing::info!(%pedido, hospedar, com_link, "abri outra versão pelo launcher"),
        Err(falha) => {
            tracing::warn!(%pedido, hospedar, com_link, ?falha, "não abri outra versão pelo launcher");
        }
    }
    resultado
}

/// O corpo do `abrir_versao`, sem o registro.
async fn abrir_versao_por_dentro(
    app: &AppHandle,
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
        &config_dir(app),
        versoes::atualizador_do_app(app).as_ref(),
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

/// **ANTES DE HOSPEDAR**, só leitura (E2 da 0.17): o que hospedar vai fazer
/// com os dados deste servidor — o esquema gravado, se converte, as cópias que
/// existem e qual delas é do estado atual, se vai copiar antes — e os MODs de
/// API antiga, sem subir nada. Ver [`antes_de_hospedar::conferir`].
#[tauri::command]
fn antes_de_hospedar(app: AppHandle, servidor: Option<String>) -> AntesDeHospedarNaTela {
    let config = config_dir(&app);
    let banco = servidores::caminho_do_banco(&config, servidor.as_deref());
    let mods_de_api_antiga = if banco.exists() {
        mods::mods_de_api_antiga(
            &banco,
            &servidores::raizes_dos_mods(&config, servidor.as_deref()),
        )
        .into_iter()
        .map(|antigo| (antigo.id, antigo.api))
        .collect()
    } else {
        Vec::new()
    };
    AntesDeHospedarNaTela {
        dados: antes_de_hospedar::conferir(
            std::path::Path::new(&config),
            servidor.as_deref(),
            &banco,
        ),
        mods_de_api_antiga,
        mods_incorporados: if banco.exists() {
            mods::incorporados_ligados(&banco)
        } else {
            Vec::new()
        },
        escolhas_pendentes: escolhas_de_mods::pendentes(
            std::path::Path::new(&config),
            servidor.as_deref(),
        ),
    }
}

/// O ANTES DE HOSPEDAR como a tela o lê: os dados, e os MODs ao lado.
#[derive(Debug, serde::Serialize)]
struct AntesDeHospedarNaTela {
    #[serde(flatten)]
    dados: antes_de_hospedar::AntesDeHospedar,
    /// Os MODs ligados que esta versão não roda, **como estão no banco**: uma
    /// escolha estagiada não os tira daqui, porque o banco não mudou.
    mods_de_api_antiga: Vec<(String, Option<u32>)>,
    /// **Os MODs ligados que agora são parte do produto** (0.19), de qualquer
    /// API: `(id, desde)`. A tela não oferece ATUALIZAR para eles; diz que o
    /// produto os incorporou e que os dados deles serão trazidos na subida.
    mods_incorporados: Vec<(String, String)>,
    /// **As escolhas de MOD estagiadas para este servidor** (P1 da 0.17): o
    /// `hospedar` as aplica depois da cópia e da conversão. Desistir, voltar e
    /// reabrir mostram as mesmas; o banco não foi tocado.
    escolhas_pendentes: Vec<escolhas_de_mods::Escolha>,
}

/// **PREPARAR CÓPIA** (E2 da 0.17): cria e verifica uma cópia de segurança do
/// estado atual do banco deste servidor, sem hospedar nem converter nada. O
/// `hospedar` faz o mesmo sozinho quando precisa; esta é a ação para quem quer
/// a cópia antes.
///
/// # Errors
///
/// [`antes_de_hospedar::FalhaNaCopia`], com a etapa e o motivo.
#[tauri::command]
fn preparar_copia(
    app: AppHandle,
    servidor: Option<String>,
) -> Result<antes_de_hospedar::Copia, antes_de_hospedar::FalhaNaCopia> {
    let config = config_dir(&app);
    let banco = servidores::caminho_do_banco(&config, servidor.as_deref());
    antes_de_hospedar::preparar_copia(std::path::Path::new(&config), servidor.as_deref(), &banco)
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
    // A senha de um servidor novo. `None` é o portão de sempre. Com ela o
    // servidor nasce com senha e sem a portaria. Ver o `PedidoDeHospedar`.
    senha: Option<String>,
) -> Result<Anfitriao, FalhaAoHospedar> {
    let config = config_dir(&app);
    let atualizador = versoes::atualizador_do_app(&app);
    hospedar_em(
        &session,
        &config,
        atualizador.as_ref(),
        PedidoDeHospedar {
            nome_publico,
            servidor,
            senha,
        },
        PORTA_PADRAO,
        modo_local(),
    )
    .await
}

/// O que quem hospeda pediu ao `hospedar`: os três argumentos do comando.
///
/// **Sem `Debug`, de propósito**: `senha` é a senha crua, como a pessoa a
/// digitou, e só existe até a primeira linha do [`hospedar_em`], que a troca
/// por um `SenhaDoServidor`.
#[derive(Default)]
struct PedidoDeHospedar {
    /// O nome público do link. Vazio é o link numérico de sempre.
    nome_publico: Option<String>,
    /// Qual servidor guardado levantar. `None` é o banco de sempre.
    servidor: Option<String>,
    /// A senha de um servidor novo. `None` é o portão.
    senha: Option<String>,
}

/// **O corpo do `hospedar`**, sem o `AppHandle`: a pasta de dados, o
/// atualizador, a porta e o modo local vêm de quem chama. O comando passa os de
/// produto (`PORTA_PADRAO`, `modo_local()`); os testes passam porta zero e o
/// modo local, para subir o servidor de verdade por este mesmo caminho sem
/// tocar na porta 8383 nem na escada.
async fn hospedar_em(
    session: &Session,
    config: &str,
    atualizador: Option<&versoes::Atualizador>,
    pedido: PedidoDeHospedar,
    porta: u16,
    local: bool,
) -> Result<Anfitriao, FalhaAoHospedar> {
    let PedidoDeHospedar {
        nome_publico,
        servidor,
        senha,
    } = pedido;
    // **A senha de um servidor novo, conferida antes de qualquer coisa subir**,
    // como o nome logo abaixo: uma senha que não serve recusada depois deixaria
    // no ar, no modo portão, o servidor que quem hospeda pediu com senha. Daqui
    // em diante ela é um `SenhaDoServidor`, que não se imprime.
    let senha = senha
        .as_deref()
        .map(senha_do_servidor::conferir)
        .transpose()
        .map_err(FalhaAoHospedar::SenhaRecusada)?;
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
    versoes::conferir_esta_versao(config, atualizador, seele_lancador::Ato::Hospedar).await?;

    // **Qual banco, e por que isso passa pelo registro.** Até aqui era sempre
    // `<config>/seele.db`, um por máquina. `servidores::banco` devolve esse
    // mesmo caminho quando ninguém escolheu nada — e é o que faz hospedar
    // continuar funcionando numa máquina sem registro nenhum.
    let config = config.to_owned();
    servidores::listar_conferido(&config).map_err(|motivo| {
        FalhaAoHospedar::DadosNaoVerificados {
            motivo,
            acoes: vec!["tentar-de-novo", "abrir-a-pasta"],
        }
    })?;
    let id_do_servidor = servidor.as_deref();
    if let Some(id) = id_do_servidor {
        servidores::marcar_uso(&config, id);
    }
    // O mesmo banco que as saídas de quem hospeda com um MOD de API antiga
    // abrem (`resolver_mod_de_api_antiga`): um lugar só, em `servidores`.
    let banco = servidores::banco_de_hospedar(&config, id_do_servidor);
    // **Um MOD ligado que este app não roda: dito antes de subir** (o corte da
    // API 6; tarefa 13 do plano da F3, Step 5b). Com o mesmo banco e as mesmas
    // raízes que vão ao servidor, e antes de marcar o servidor como hospedado:
    // recusado aqui, nada sobe e nada fica dizendo que esta janela hospeda.
    // Menos os que já têm uma escolha estagiada (ATUALIZAR ou DESLIGAR, no
    // ANTES DE HOSPEDAR): essas são aplicadas depois da cópia e da conversão,
    // em `preparar_a_casa_de_quem_hospeda`. Ver `escolhas_de_mods`.
    let antigos = mods::recusas_de_hospedar(&config, id_do_servidor, &banco);
    if !antigos.is_empty() {
        return Err(FalhaAoHospedar::ModsDeApiAntiga {
            mods: antigos
                .into_iter()
                .map(|antigo| (antigo.id, antigo.api))
                .collect(),
        });
    }
    // **O portão da conversão** (E2 da 0.17): hospedar por esta versão
    // converte um banco de versão anterior, e só converte com uma cópia
    // confirmada do estado atual — criada e verificada aqui, se não há. Falha
    // fechado: um banco que não se lê, ou uma cópia que não ficou pronta, não
    // sobe nada. Ver `antes_de_hospedar`.
    match antes_de_hospedar::garantir_a_copia(std::path::Path::new(&config), id_do_servidor, &banco)
    {
        Ok(portao) => tracing::info!(?portao, "o portão da conversão abriu"),
        Err(antes_de_hospedar::PortaoFechado::NaoVerificado { motivo, acoes }) => {
            tracing::warn!(%motivo, "hospedar recusado: o banco não foi verificado");
            return Err(FalhaAoHospedar::DadosNaoVerificados { motivo, acoes });
        }
        Err(antes_de_hospedar::PortaoFechado::Copia(falha)) => {
            tracing::warn!(
                ?falha,
                "hospedar recusado: a cópia de segurança não ficou pronta"
            );
            return Err(FalhaAoHospedar::CopiaDeSeguranca { falha });
        }
    }
    // A chave desta máquina no banco de um servidor guardado, **depois** do
    // portão: plantá-la abre o banco, e abrir migra (P1 da 0.17).
    if id_do_servidor.is_some() {
        servidores::plantar_a_chave_depois_do_portao(&config, &banco);
    }
    // **As escolhas de MOD estagiadas, aplicadas só agora** (P1 da 0.17):
    // depois da cópia nova e verificada; abrir o banco aqui é a conversão. E o
    // sim do dono ao conjunto que resulta, sob o endereço por onde esta janela
    // entra no próprio servidor (b1 do E7). Ver `escolhas_de_mods`.
    let aqui = format!("127.0.0.1:{porta}");
    match escolhas_de_mods::aplicar_depois_do_portao(&config, id_do_servidor, &banco, &aqui) {
        Ok(aplicou) => {
            if aplicou {
                tracing::info!("as escolhas de MOD estagiadas foram aplicadas depois da cópia");
            }
        }
        Err(motivo) => {
            tracing::warn!(%motivo, "hospedar recusado: uma escolha de MOD estagiada não se aplicou");
            return Err(FalhaAoHospedar::EscolhaDeModNaoAplicou { motivo });
        }
    }
    // Guardado **antes** de subir: quem perguntar «qual servidor esta janela
    // hospeda?» entre o início e o fim recebe a resposta certa, e não «nenhum».
    if let Ok(mut qual) = session.servidor_hospedado.lock() {
        *qual = id_do_servidor.map(str::to_owned);
    }
    // Quem hospeda, pela chave desta máquina: é a impressão que a portaria vai
    // admitir antes de a porta abrir. Falhar aqui é grave o bastante para não
    // hospedar — um servidor no ar em que o dono não entra é uma janela travada
    // com uma frase pedindo que alguém decida.
    let minha =
        seele_ffi::impressao_desta_maquina(&config).map_err(|_| FalhaAoHospedar::NaoSubiu)?;
    // **Com o ponto de encontro do projeto**, pedido aqui e só aqui: sem
    // `$SEELE_ENCONTRO`, o `iniciar` curto não convoca ponto nenhum, que é o
    // padrão seguro dos testes. Quem hospeda pelo app é quem está atrás do
    // roteador de casa, e é para essa pessoa que o degrau 4 existe. O guarda é
    // `o_ponto_de_encontro_de_quem_hospeda`, logo abaixo desta função.
    // ADR 0045 para o pacote, e o plano de isolamento de 18/09 para os
    // arquivos: o código é compartilhado porque é imutável e conferido por
    // hash; o que o MOD escreve é **desta instância**, e não da máquina.
    let raizes = Some(servidores::raizes_dos_mods(&config, id_do_servidor));
    let local_do_banco = seele_server::persistence::Location::File(banco);
    // **Antes de o servidor aceitar a primeira conexão** — D-seg-1, a
    // janela do Comandante. Isto rodava depois de `iniciar` voltar, com o
    // laço já no ar: num banco novo a portaria nasce desligada, e quem
    // chegasse nesse intervalo virava a primeira conta, e a primeira conta
    // vira a Comandante. Ver `crates/seele-server/tests/janela_do_comandante.rs`.
    //
    // **E a senha de um servidor novo junto**: gravada aqui, com o laço de
    // aceitação ainda parado, e não por um `definir_senha_do_server` depois de
    // subir. Um servidor pedido com senha não passa nem um instante no ar sem
    // ela. O guarda é `a_senha_e_gravada_antes_de_o_servidor_aceitar`.
    let antes_de_aceitar = move |persistence: &mut seele_server::persistence::Persistence| {
        preparar_a_casa_de_quem_hospeda(persistence, &minha, senha.as_ref())
    };
    let server = subir_o_guardado(porta, local_do_banco, raizes, antes_de_aceitar, local)
        .await
        .map_err(|erro| classificar(&erro))?;

    let alcance = server.alcance();
    let firewall = firewall_deste_executavel();
    let anfitriao = Anfitriao {
        // A porta em que ele de fato atende: a padrão no produto, a que o
        // sistema escolheu num teste de porta zero.
        aqui: if porta == 0 {
            format!("127.0.0.1:{}", server.endereco().port())
        } else {
            aqui
        },
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
        .guardado = Some(Hospedado::novo(server, servidor));

    Ok(anfitriao)
}

/// Sobe o servidor guardado do `hospedar`: com o ponto de encontro do projeto,
/// ou, no **modo local** da build de teste, sem escada e só em loopback.
async fn subir_o_guardado(
    porta: u16,
    banco: seele_server::persistence::Location,
    raizes: Option<seele_server::RaizesDosMods>,
    antes_de_aceitar: impl FnOnce(&mut seele_server::persistence::Persistence) -> anyhow::Result<()>,
    local: bool,
) -> anyhow::Result<seele_server::hospedagem::Hospedagem> {
    // **O modo local (E9), só na build de teste**: sem escada (nenhum UPnP,
    // PCP nem ponto de encontro) e só em loopback. Ver [`modo_local`]: o
    // comando passa o `modo_local()`, e os testes passam `true`.
    if local {
        tracing::info!("modo local: o servidor guardado sobe sem escada, só em loopback");
        seele_server::hospedagem::Hospedagem::iniciar_local(
            porta,
            banco,
            "Casa",
            raizes,
            antes_de_aceitar,
        )
        .await
    } else {
        seele_server::hospedagem::Hospedagem::iniciar_com_o_ponto_padrao(
            porta,
            banco,
            "Casa",
            raizes,
            antes_de_aceitar,
        )
        .await
    }
}

/// **O que o `hospedar` faz no banco já convertido, antes da primeira
/// conexão** (o `antes_de_aceitar` dele; D-seg-1, a janela do Comandante): a
/// porta do servidor — a portaria semeada ligada (ADR 0030), ou a senha que
/// quem hospeda escolheu para um servidor novo — e o dono admitido na própria
/// casa. As escolhas de MOD estagiadas já foram aplicadas antes da subida,
/// depois do portão da cópia (`escolhas_de_mods::aplicar_depois_do_portao`).
///
/// # A senha de um servidor novo
///
/// Com `senha`, o servidor nasce **com senha e sem a portaria**: quem tem a
/// senha entra direto, e quem não tem é recusado antes de bater
/// (`admissao::Politica`). São as mesmas duas funções do painel A PORTA —
/// `admissao::definir_senha` e `portaria::ligar` —, chamadas aqui, com o laço
/// de aceitação parado. Qualquer falha delas é falha de hospedar: um servidor
/// pedido com senha não sobe sem ela.
///
/// **Só num servidor que ainda não tem porta escolhida**: sem senha gravada e
/// sem decisão sobre a portaria. Num que já tem, a escolha de servidor novo é
/// recusada ([`PortaJaEscolhida`]) e nada muda: ele já tem a configuração dele,
/// e trocá-la é do painel A PORTA.
fn preparar_a_casa_de_quem_hospeda(
    persistence: &mut seele_server::persistence::Persistence,
    minha: &str,
    senha: Option<&senha_do_servidor::SenhaDoServidor>,
) -> anyhow::Result<()> {
    if let Some(senha) = senha {
        let ja_tem_senha = seele_server::admissao::Politica::carregar(persistence)?.tem_senha();
        if ja_tem_senha || seele_server::portaria::decidida(persistence)? {
            return Err(anyhow::Error::new(PortaJaEscolhida));
        }
        seele_server::admissao::definir_senha(persistence, Some(senha.texto()))?;
        // Desligada de propósito, e gravada: o `semear_ligada` de toda subida
        // seguinte deste servidor não a religa por cima da escolha.
        seele_server::portaria::ligar(persistence, false)?;
    } else if let Err(erro) = seele_server::portaria::semear_ligada(persistence) {
        // A porta se fecha no mesmo gesto de hospedar, antes do primeiro
        // pacote — ADR 0030. O `seeled` continua subindo aberto, porque
        // quem digita `seeled` aceitou cerimônia; quem apertou um botão
        // não, e para ele o padrão é perguntar.
        //
        // Semear, e não ligar: isto roda toda vez que a janela sobe um
        // servidor, e um interruptor que se rearma sozinho é um
        // interruptor quebrado.
        //
        // Não impede de hospedar. Um servidor no ar com a portaria
        // desligada é o comportamento de antes deste ADR, e a tela diz
        // em que estado a porta está.
        tracing::warn!(%erro, "não consegui semear a portaria");
    }
    // **E quem hospeda é o dono** (ADR 0064, achado do compat): a chave desta
    // máquina é declarada proprietária antes da primeira conexão. Sem isto,
    // quem apertou HOSPEDAR entrava sem autoridade nenhuma — nem salas, nem
    // moderação, nem cargos. A mesma declaração a cada subida não muda nada
    // nem sobe a revisão dos cargos (`declarar_proprietario`). Falha fechado:
    // um servidor no ar em que o dono não manda é pior que um que não subiu.
    seele_server::cargos::declarar_proprietario(persistence, minha)?;
    // E quem hospeda entra na própria casa. Sem isto a portaria tranca
    // o dono para fora: o app conecta no servidor que acabou de subir,
    // o porteiro trata quem hospeda como desconhecido, e o pedido fica
    // esperando a decisão de alguém que não consegue entrar para
    // decidir — foi o que aconteceu numa máquina de verdade.
    seele_server::portaria::admitir_o_dono(persistence, minha)
}

/// **Quem aperta HOSPEDAR é o dono** (ADR 0064; achado do compat, item 8):
/// pelo corpo de verdade do comando (`hospedar_em`, em porta zero e no modo
/// local), quem hospeda entra e tem a autoridade de dono — cria salas, gere
/// cargos, administra —, e a impressão declarada é a desta máquina. Outra
/// conta no mesmo banco não vira dona.
#[path = "testes/quem_hospeda_e_o_dono.rs"]
#[cfg(test)]
mod quem_hospeda_e_o_dono;

/// **A porta de um servidor novo se escolhe antes de ele subir**: pelo corpo
/// de verdade do comando, com gente de outra máquina batendo pela FFI. Com
/// senha, o servidor a exige desde a primeira conexão e quem a tem entra
/// direto; sem senha, nasce no portão, como sempre. A senha que não serve é
/// recusada antes de subir, não cai sobre um servidor que já tinha a porta
/// dele, e não aparece no registro.
#[path = "testes/a_porta_de_um_servidor_novo.rs"]
#[cfg(test)]
mod a_porta_de_um_servidor_novo;

/// **A ordem do CTA, no `hospedar` de verdade** (P1 da 0.17). Os testes do
/// `mods.rs` provam cada passo com bancos reais; este prende que o comando os
/// chama **nesta ordem**: a recusa pelos MODs (menos os estagiados), o portão
/// da cópia, a chave, as escolhas aplicadas, e só então a subida. E que os
/// caminhos de MOD com o servidor parado não abrem o banco para escrever.
#[path = "testes/a_ordem_do_cta_no_hospedar.rs"]
#[cfg(test)]
mod a_ordem_do_cta_no_hospedar;

/// **O modo local (E9 da 0.17): só na build de teste, e só pedido.**
///
/// Com `SEELE_MODO_LOCAL=1` **num build de debug**, o servidor guardado
/// (`hospedar`) e a sala pessoal (`sala::subir`) sobem pelo `iniciar_local` e
/// pelo `iniciar_sala_local` do `seele-server`: só em `127.0.0.1`, e sem a
/// escada do ADR 0022 (nenhum UPnP, PCP nem ponto de encontro). É o que deixa a
/// navegação nativa do Codex (CUA) hospedar sem mexer no roteador da casa. A
/// navegação nesse modo **não valida NAT real**.
///
/// Num build otimizado a função é `false` constante: a variável nem é lida, e o
/// modo é inalcançável no produto. O guarda é
/// `o_modo_local_nao_existe_num_build_otimizado`.
#[cfg(debug_assertions)]
pub(crate) fn modo_local() -> bool {
    std::env::var("SEELE_MODO_LOCAL").as_deref() == Ok("1")
}

/// O modo local num build otimizado: nunca. Ver a versão de debug.
#[cfg(not(debug_assertions))]
pub(crate) const fn modo_local() -> bool {
    false
}

/// **O modo local, medido** (E9): com `SEELE_MODO_LOCAL=1`, o servidor guardado
/// do `hospedar` e a sala pessoal do `setup` sobem só em `127.0.0.1` e sem a
/// escada (o alcance é `SoEstaMaquina`, e o convite não leva bilhete). E o modo
/// não existe num build otimizado.
#[path = "testes/o_modo_local_da_build_de_teste.rs"]
#[cfg(test)]
mod o_modo_local_da_build_de_teste;

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
#[path = "testes/o_banco_de_uma_versao_mais_nova.rs"]
#[cfg(test)]
mod o_banco_de_uma_versao_mais_nova;

#[path = "testes/o_ponto_de_encontro_de_quem_hospeda.rs"]
#[cfg(test)]
mod o_ponto_de_encontro_de_quem_hospeda;

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

/// O arquivo do teste de rede, na origem do web app (`ui/diagnostico-rede.html`,
/// servido com a política própria dele em `web/_headers`).
const PAGINA_DO_TESTE_DE_REDE: &str = "diagnostico-rede.html";

/// Quantas letras o nome de uma falha pode ter para viajar no endereço. O mais
/// longo de hoje (`VerificacaoDaConexaoSegura`) tem 26.
const TETO_DO_NOME_DA_FALHA: usize = 40;

/// **O endereço do teste de rede publicado**, com o nome da falha que a pessoa
/// viu, quando ele é um nome.
///
/// A origem é a [`seele_ffi::ORIGEM_DO_NAVEGADOR`], e não um texto daqui: o
/// guarda `a_origem_do_web_app_e_escrita_num_lugar_so` reprova a cópia.
///
/// **Só o nome da falha vai junto**, e só se for letras e números: é um
/// identificador de enum (`NatNaoAbriu`, `SemResposta`), que a página põe no
/// texto copiado para quem relata não ter de redigitar o que leu. O que não
/// for isso — um endereço, um apelido, um convite que alguém passasse aqui por
/// engano — não entra no endereço: a consulta de uma URL chega ao registro de
/// acesso de quem serve a página, e ali só cabe o que não identifica ninguém.
fn endereco_do_teste_de_rede(falha: Option<&str>) -> String {
    let pagina = format!(
        "https://{}/{PAGINA_DO_TESTE_DE_REDE}",
        seele_ffi::ORIGEM_DO_NAVEGADOR
    );
    match falha.map(str::trim) {
        Some(nome)
            if !nome.is_empty()
                && nome.len() <= TETO_DO_NOME_DA_FALHA
                && nome.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            format!("{pagina}?erro={nome}")
        }
        _ => pagina,
    }
}

/// Por que o teste de rede não abriu.
///
/// Enum e não frase, como as vizinhas: a fronteira erro→texto é do frontend.
/// Carrega o endereço porque a tela não o conhece (a origem mora no Rust), e
/// «não abriu» sem ele é um botão que falha sem deixar caminho.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
enum TesteDeRedeNaoAbriu {
    /// O sistema não iniciou o navegador.
    NavegadorNaoAbriu {
        /// O endereço que se tentou abrir, para a tela dizê-lo.
        endereco: String,
    },
}

/// **Abre o teste de rede no navegador do sistema**, na página publicada.
///
/// É o caminho de quem leu que não conectou por causa de rede (a lacuna 8 da
/// análise para a 1.0: a página existia e nada levava a ela).
///
/// **No navegador, e não nesta janela**, por três razões medidas:
///
///   - navegar esta janela para a página levaria as sessões e a voz embora, e
///     a casca tem uma janela só (`capabilities/janela.json` não deixa criar
///     outra);
///   - a página trata um convite **colado**, e na origem publicada ela roda com
///     a política mais estreita do produto (`connect-src` só para a própria
///     origem e o ponto de encontro oficial), sem `ipc:` nenhum ao alcance;
///     dentro desta janela ela rodaria na origem que fala com o Rust;
///   - o que ela mede é o caminho de um navegador até o servidor, e o botão
///     «Entrar no servidor» dela, quando o caminho abre, entra pelo navegador:
///     é a saída de quem o app não deixou entrar.
///
/// `erro` é o nome da falha que a tela mostrou (ver
/// [`endereco_do_teste_de_rede`]).
#[tauri::command]
fn abrir_o_teste_de_rede(erro: Option<String>) -> Result<(), TesteDeRedeNaoAbriu> {
    let endereco = endereco_do_teste_de_rede(erro.as_deref());
    abrir_no_navegador(endereco.clone()).map_err(|motivo| {
        tracing::warn!(%motivo, "o teste de rede não abriu no navegador");
        TesteDeRedeNaoAbriu::NavegadorNaoAbriu { endereco }
    })
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
        // A senha de um servidor novo num servidor que já tinha a porta dele.
        if causa.downcast_ref::<PortaJaEscolhida>().is_some() {
            return FalhaAoHospedar::PortaJaEscolhida;
        }
        // O banco de uma versão mais nova: a outra falha com conserto que a
        // pessoa consegue fazer, que é atualizar.
        if let Some(futuro) = causa.downcast_ref::<seele_server::persistence::BancoDoFuturo>() {
            return FalhaAoHospedar::BancoDoFuturo {
                gravado: futuro.gravado,
                conhecido: futuro.conhecido,
            };
        }
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
///
/// # Desde a 0.17: o SAIR de **uma** sessão (E3)
///
/// O carimbo diz qual. Sair dela não toca nas outras sessões, nem no foco de
/// voz de outra, nem em hospedagem nenhuma. **Só o `sessao` do carimbo conta,
/// e não a geração**: sair nunca é recusado, nem de uma sessão que reconectou
/// depois de a janela desenhar o botão. Sair de uma sessão que já saiu, ou que
/// nunca existiu, não faz nada — é o apertar duas vezes.
#[tauri::command]
async fn disconnect<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ()> {
    desmontar_o_cliente(&app, &session, carimbo.sessao);
    Ok(())
}

/// SAIR de uma sessão, ou dispensar a placa de uma que o servidor encerrou
/// (E3 da 0.17). O mesmo corpo do [`disconnect`]: um verbo só para sair, com o
/// nome que a trilha usa.
///
/// # Errors
///
/// Nunca.
#[tauri::command]
async fn encerrar_sessao<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ()> {
    desmontar_o_cliente(&app, &session, carimbo.sessao);
    Ok(())
}

/// **As sessões cliente desta janela**, como a trilha as desenha (E3 da 0.17).
///
/// Todas, na ordem de entrada: qual está na vista, qual tem o foco de voz,
/// qual é um servidor que esta máquina hospeda (pela chave), em que sala de
/// voz cada uma está, se reconecta ou foi encerrada, e as não lidas.
#[tauri::command]
fn sessoes(session: State<'_, Session>) -> Vec<sessoes::SessaoNaTela> {
    sessoes_na_tela(&session)
}

/// [`sessoes`] sem o `State`, para os testes.
fn sessoes_na_tela(session: &Session) -> Vec<sessoes::SessaoNaTela> {
    let vista = session.sessoes.vista();
    let (guardado, pessoal) = session
        .hospedagens
        .lock()
        .map_or((None, None), |hospedagens| hospedagens.impressoes());
    session
        .sessoes
        .todas()
        .iter()
        .map(|sessao| {
            let onde = onde_estou_em(
                Some(&sessao.endereco),
                sessao.conexao.impressao_do_servidor().as_deref(),
                guardado.as_deref(),
                pessoal.as_deref(),
            );
            sessao.na_tela(vista, onde)
        })
        .collect()
}

/// **Troca a sessão que a janela mostra** (E3 da 0.17). Nenhuma conexão cai,
/// e o foco de voz não muda: ver uma sessão não é falar nela.
///
/// A geração da vista sobe, e os MODs da anterior param, como numa saída: os
/// MODs são da vista. A [`Entrada`] devolvida é a da sessão pedida, com o
/// carimbo dela e a geração nova da vista.
///
/// # Errors
///
/// [`ConnectionError::NotConnected`] quando não há sessão com esse número.
///
/// # A ordem dos pedidos (as corridas que o Codex reproduziu)
///
/// `pedido` é `{ instancia, n }`: a instância da janela, emitida pelo backend
/// na carga dela ([`instancia_da_janela`]), e um número que a janela sobe a
/// cada clique na trilha. Dois `ver_sessao` (para 2 e para 3) podem chegar fora
/// de ordem; **o de número menor que o último aplicado, ou de uma instância
/// que não é mais a da janela (a página de antes de um reload), é recusado**
/// ([`FalhaAoVer::PedidoVelho`]) e não mexe na vista. Nada depende de relógio.
/// Os pedidos são aplicados um por vez, sob o cadeado da conexão da vista, e
/// cada um devolve a **época** nova (`Entrada::geracao`), que só sobe: a janela
/// descarta qualquer resposta de uma época menor que a da vista dela — inclusive
/// a da primeira A numa ida A→B→A. Sem `pedido` (uma janela antiga), vale a
/// ordem de chegada.
#[tauri::command]
fn ver_sessao<R: tauri::Runtime>(
    app: AppHandle<R>,
    session: State<'_, Session>,
    sessao: u64,
    pedido: Option<sessoes::PedidoDeVista>,
) -> Result<Entrada, FalhaAoVer> {
    ver_sessao_em(&app, &session, sessao, pedido)
}

/// **A instância desta carga da janela**, para os pedidos de [`ver_sessao`]:
/// um número que só sobe no processo. A janela o pede uma vez, ao carregar, e
/// numera os pedidos dela dentro dele. Recarregar a página pede outro, e todo
/// pedido da carga anterior passa a ser velho.
#[tauri::command]
fn instancia_da_janela(session: State<'_, Session>) -> u64 {
    session.dominio_da_vista.abrir_instancia()
}

/// Por que a vista não mudou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
enum FalhaAoVer {
    /// Não há sessão com esse número (saiu, ou nunca existiu).
    SemSessao,
    /// Um pedido mais novo que este já foi aplicado, ou ele é de uma carga da
    /// janela que não está mais de pé: chegou atrasado e é recusado, para a
    /// vista não voltar atrás.
    PedidoVelho {
        /// Este pedido.
        pedido: sessoes::PedidoDeVista,
        /// O último aplicado.
        ultimo: sessoes::PedidoDeVista,
    },
}

/// [`ver_sessao`] sem o `State`.
fn ver_sessao_em<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    sessao: u64,
    pedido: Option<sessoes::PedidoDeVista>,
) -> Result<Entrada, FalhaAoVer> {
    let pedida = session
        .sessoes
        .buscar(sessao)
        .ok_or(FalhaAoVer::SemSessao)?;
    let geracao = {
        let mut slot = session
            .connection
            .lock()
            .map_err(|_| FalhaAoVer::SemSessao)?;
        // Conferido e anotado com o cadeado na mão: dois pedidos nunca se
        // cruzam entre a conferência e a troca.
        if let Some(pedido) = pedido {
            if let Err(ultimo) = session.dominio_da_vista.conferir(pedido) {
                tracing::info!(
                    sessao,
                    ?pedido,
                    ?ultimo,
                    "um pedido de vista chegou depois de um mais novo, ou de uma carga \
                     da janela que já não está de pé: recusado, e a vista ficou onde estava"
                );
                return Err(FalhaAoVer::PedidoVelho { pedido, ultimo });
            }
        }
        let geracao = session.revogar();
        session.por_na_vista_com(&mut slot, Some(&pedida));
        geracao
    };
    avisar_a_trilha(app, sessao, "vista");
    tracing::info!(
        sessao,
        geracao,
        "a vista mudou de sessão; nenhuma conexão caiu"
    );
    Ok(Entrada {
        carimbo: pedida.conexao.carimbo(),
        snapshot: pedida.conexao.snapshot(),
        veredito: seele_ffi::Trust::Known,
        geracao,
        de_onde: DeOndeVeio::default(),
    })
}

/// O que uma troca de foco de voz fez, como a janela o lê.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct FocoNaTela {
    /// O `SessaoId` que tinha o microfone.
    antes: Option<u64>,
    /// O que tem agora.
    agora: Option<u64>,
    /// A sala de voz que a anterior deixou, para a frase «a voz saiu de X».
    saiu_da_sala: Option<String>,
}

/// Qual sessão tem o microfone desta máquina agora.
#[tauri::command]
fn foco_de_voz(session: State<'_, Session>) -> Option<u64> {
    session.sessoes.com_o_foco().map(|sessao| sessao.id())
}

/// **Traz o microfone para uma sessão**, ou o tira de todas (E3 da 0.17).
///
/// A troca é a do [`seele_ffi::foco::FocoDeVoz::focar`], em ordem total: a
/// anterior para de capturar e de tocar **antes** de esta abrir. Depois, a
/// anterior sai da sala de voz em que estava — ninguém lá deve ver presente
/// quem não ouve nem fala. Com o carimbo de outra geração, recusado e nada
/// muda.
///
/// # Errors
///
/// - [`ConnectionError::NotConnected`]: a sessão não existe;
/// - [`ConnectionError::SessaoNaoConfere`]: o carimbo é de antes de uma
///   reconexão dela.
#[tauri::command]
async fn focar_voz<R: tauri::Runtime>(
    app: AppHandle<R>,
    session: State<'_, Session>,
    carimbo: Option<sessoes::CarimboDaJanela>,
) -> Result<FocoNaTela, ConnectionError> {
    focar_voz_em(&app, &session, carimbo).await
}

/// [`focar_voz`] sem o `State`.
async fn focar_voz_em<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    carimbo: Option<sessoes::CarimboDaJanela>,
) -> Result<FocoNaTela, ConnectionError> {
    let nova = match carimbo {
        Some(carimbo) => Some(session.conexao(carimbo)?),
        None => None,
    };
    let antiga = session.sessoes.com_o_foco();
    let antes = antiga.as_ref().map(|antiga| antiga.id());
    let foco = session.sessoes.foco();
    let id_da_nova = nova.as_ref().and_then(|nova| nova.id_no_foco());
    if nova.is_some() && id_da_nova.is_none() {
        // Uma sessão que não entrou pelo foco não tem como tê-lo.
        return Err(ConnectionError::NotConnected);
    }
    // A troca bloqueia até a anterior confirmar: fora do runtime.
    let troca = tauri::async_runtime::spawn_blocking(move || foco.focar(id_da_nova))
        .await
        .map_err(|_| ConnectionError::NotConnected)?
        .map_err(|_| ConnectionError::NotConnected)?;
    let mut saiu_da_sala = None;
    let pedida = carimbo.map(|carimbo| carimbo.sessao);
    if let Some(antiga) = antiga.filter(|antiga| Some(antiga.id()) != pedida) {
        // **A anterior sai da sala depois de calada** (ADR 0031): o microfone
        // dela já parou na troca; agora o servidor dela também fica sabendo.
        saiu_da_sala = antiga
            .conexao
            .snapshot()
            .voice_rooms
            .iter()
            .find(|sala| sala.occupied_by_us)
            .map(|sala| sala.name.clone());
        if saiu_da_sala.is_some() {
            if let Err(erro) = antiga.conexao.leave_voice_room() {
                tracing::warn!(
                    ?erro,
                    "a sessão que perdeu a voz não conseguiu sair da sala dela: o servidor a \
                     vê presente, sem voz"
                );
            }
        }
        avisar_a_trilha(app, antiga.id(), "foco");
    }
    let agora = session.sessoes.com_o_foco().map(|sessao| sessao.id());
    if let Some(agora) = agora {
        avisar_a_trilha(app, agora, "foco");
    }
    tracing::info!(
        antes = ?troca.antes,
        agora = ?agora,
        confirmada = troca.confirmada,
        saiu_da_sala = saiu_da_sala.is_some(),
        "foco de voz trocado"
    );
    Ok(FocoNaTela {
        antes,
        agora,
        saiu_da_sala,
    })
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
///
/// # Desde a 0.17: uma sessão, pelo `SessaoId` (E3)
///
/// Só a vista é revogada, e só se a sessão que sai é a da vista: sair de uma
/// sessão do fundo não para os MODs da que está na tela. A que sai deixa de
/// valer no registro **antes** de a conexão cair, e o que ainda vier dela é
/// descartado e contado.
fn desmontar_o_cliente<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &Session,
    sessao: u64,
) {
    let na_vista = session.sessoes.vista() == Some(sessao);
    if na_vista {
        let morta = session.revogar();
        tracing::debug!(
            geracao = morta,
            "sessão da vista revogada antes de desmontar"
        );
    }

    let saindo = session.sessoes.tirar(sessao);
    // As metades de máquina desta sessão param com `servidor-saiu`; o aceite
    // guardado fica.
    maquina::sessao_saiu(app, sessao);
    if na_vista {
        session.por_na_vista(None);
    }
    let connection = saindo.as_ref().map(|saindo| Arc::clone(&saindo.conexao));
    let alvo_da_saindo = saindo.as_ref().and_then(|saindo| saindo.alvo());

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
        if let (Some(alvo), true) = (alvo_da_saindo, medido != 0) {
            if let Ok(mut lista) =
                seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(app))
            {
                if let Err(erro) = lista.anotar_caminho_medido(&alvo, medido) {
                    tracing::debug!(%erro, "não guardei o caminho medido deste servidor");
                }
            }
        }
    }
    // A sessão acaba aqui, e não quando o último `Arc` cair: um comando em voo
    // ainda pode segurar um, e a despedida não espera por ele.
    if let Some(viva) = connection.as_ref() {
        viva.disconnect();
    }
    drop(connection);
    drop(saindo);

    if na_vista {
        // O convite morre com a sessão que ele abriu. Enquanto nada era
        // conferido isto era inerte; deixou de ser no momento em que
        // `expected_fingerprint` passou a sair daqui — quem sai, digita outro
        // endereço e entra de novo levaria a impressão prometida por um link
        // anterior para um servidor que nunca a prometeu, e a recusa apareceria
        // sem nada na tela que a explique.
        if let Ok(mut slot) = session.convite.lock() {
            *slot = None;
        }
        // Onde a sessão estava deixa de existir com ela: um sim gravado depois
        // disso iria para o endereço da conversa anterior.
        if let Ok(mut onde) = session.endereco_da_sessao.lock() {
            *onde = None;
        }
    }
    avisar_a_trilha(app, sessao, "saiu");
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
#[path = "testes/o_mod_do_guardado_continua_respondendo.rs"]
#[cfg(test)]
mod o_mod_do_guardado_continua_respondendo;

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
fn snapshot(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<sessoes::RetratoNaSessao, ConnectionError> {
    // **Dito na entrada e na saída.** Um comando que não volta é
    // indistinguível, no registro, de um que nunca foi chamado — e era essa
    // ambiguidade que sobrava depois de instrumentar o resto do caminho de um
    // pedido de MOD, na medição de 19/09.
    tracing::trace!("retrato pedido");
    let conexao = session.conexao(carimbo)?;
    let retrato = sessoes::RetratoNaSessao {
        carimbo: conexao.carimbo(),
        epoca: session.geracao(),
        protocolo: conexao.protocolo_negociado(),
        retrato: conexao.snapshot(),
    };
    tracing::trace!("retrato pronto");
    Ok(retrato)
}

/// **Os arquivos que uma pessoa escolheu para um MOD** moram no `seele-sessao`
/// (`escolhidos`) desde a Tarefa 2.6 da F4: as regras do seletor e o guarda,
/// que o navegador usa também. Aqui ficam os bytes, guardados nele, e os
/// invólucros que põem o motivo no [`FalhaNoMod`].
use seele_ffi::escolhidos::{Escolhido, Escolhidos};
#[cfg(test)]
use seele_ffi::escolhidos::{ARQUIVOS_DE_PE, PEDACO_DO_ESCOLHIDO, TETO_DO_ESCOLHIDO};

/// Os arquivos escolhidos desta janela, com os bytes.
type ArquivosEscolhidos = Escolhidos<Vec<u8>>;

/// A recusa de uma regra dos escolhidos, no [`FalhaNoMod`].
fn recusado(motivo: String) -> FalhaNoMod {
    FalhaNoMod::Recusado { motivo }
}

/// O teto deste pedido de escolha — ver
/// [`seele_ffi::escolhidos::teto_do_escolhido`].
#[cfg(test)]
fn teto_do_escolhido(limite_de_bytes: Option<usize>) -> usize {
    seele_ffi::escolhidos::teto_do_escolhido(limite_de_bytes)
}

/// O papel provado contra os pedidos — ver
/// [`seele_ffi::escolhidos::o_papel_serve`].
fn o_papel_serve(papeis: Option<&[String]>, papel: &str) -> Result<(), FalhaNoMod> {
    seele_ffi::escolhidos::o_papel_serve(papeis, papel).map_err(recusado)
}

/// Um pedaço de um arquivo escolhido, em base64, para quem o escolheu.
fn pedaco_em_base64(
    guardados: &ArquivosEscolhidos,
    geracao: u64,
    id: &str,
    arquivo: u64,
    inicio: usize,
) -> Result<String, FalhaNoMod> {
    let faixa = guardados
        .pedaco(geracao, id, arquivo, inicio)
        .map_err(recusado)?;
    let guardado = guardados.de(geracao, id, arquivo).map_err(recusado)?;
    Ok(seele_ffi::base64_de(
        guardado.conteudo.get(faixa).unwrap_or_default(),
    ))
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
fn messages(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<sessoes::NaSessao<Vec<seele_ffi::Message>>, ConnectionError> {
    let conexao = session.conexao(carimbo)?;
    Ok(sessoes::NaSessao::de(
        &conexao,
        session.geracao(),
        conexao.messages(),
    ))
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
    carimbo: sessoes::CarimboDaJanela,
) -> Result<sessoes::NaSessao<Vec<seele_ffi::MembroForaDoAr>>, ConnectionError> {
    let conexao = session.conexao(carimbo)?;
    Ok(sessoes::NaSessao::de(
        &conexao,
        session.geracao(),
        conexao.membros_fora_do_ar(),
    ))
}

/// Entra numa sala de voz **e traz a voz desta máquina para cá** (E3 da 0.17).
///
/// Entrar numa sala é o ato explícito que pede o microfone (ADR 0031: «um ato,
/// não dois»): se a voz estava noutra sessão, ela para lá — e a outra sai da
/// sala dela — **antes** de esta abrir. É o único caminho, além do
/// [`focar_voz`], por onde o foco muda; um operador que move esta pessoa para
/// uma sala, do outro lado, não toma o microfone.
#[tauri::command]
async fn enter_voice_room<R: tauri::Runtime>(
    app: AppHandle<R>,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    voice_room: u32,
    senha: Option<String>,
) -> Result<(), ConnectionError> {
    entrar_na_sala_em(&app, &session, carimbo, voice_room, senha).await
}

/// [`enter_voice_room`] sem o `State`.
async fn entrar_na_sala_em<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    carimbo: sessoes::CarimboDaJanela,
    voice_room: u32,
    senha: Option<String>,
) -> Result<(), ConnectionError> {
    let conexao = session.conexao(carimbo)?;
    if !conexao.tem_o_foco_de_voz() {
        focar_voz_em(app, session, Some(carimbo)).await?;
    }
    conexao.enter_voice_room(voice_room, senha)
}

#[tauri::command]
fn leave_voice_room(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.leave_voice_room()
}

#[tauri::command]
fn open_channel(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    channel: u32,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.open_channel(channel)
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
///
/// `replies_to` é a mensagem a que esta responde, quando a pessoa apertou
/// RESPONDER numa (a mesa de texto da 1.0): o `id` que o servidor deu a ela.
/// Opcional, e o mesmo comando, como no navegador (`send_message` com
/// `repliesTo`, em `seele-web`): responder é mandar uma mensagem, e dois
/// comandos seriam dois caminhos para a pendente, o reenvio e o rascunho.
#[tauri::command]
fn send_message(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    channel: u32,
    body: String,
    replies_to: Option<u64>,
) -> Result<String, ConnectionError> {
    let conexao = session.conexao(carimbo)?;
    match replies_to {
        Some(citada) => conexao.send_reply(channel, body, citada),
        None => conexao.send_message(channel, body),
    }
}

/// Reescreve o corpo de uma mensagem **desta pessoa** (protocolo 13).
///
/// Pede, e não espera: a confirmação é a mensagem mudar na conversa, com a
/// marca «editada», pelo `MessagesChanged` de sempre. O que este comando
/// recusa na hora volta com nome, e a janela tem frase para cada um:
/// `MensagemVazia`, `MensagemGrandeDemais`, `UnknownMessage` (uma pendente
/// ainda não tem `id`) e `ServidorAntigo`, quando o servidor negociou menos
/// que o 13 — a tela nem oferece o EDITAR ali (`RetratoNaSessao::protocolo`).
/// A de outra pessoa quem recusa é o servidor, com o aviso de permissão.
#[tauri::command]
fn editar_mensagem(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    message: u64,
    body: String,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.edit_message(message, body)
}

/// Manda de novo uma mensagem que não foi gravada, com a mesma chave.
#[tauri::command]
fn reenviar_mensagem(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    client_message_id: String,
) -> Result<bool, ConnectionError> {
    session
        .conexao(carimbo)?
        .reenviar_mensagem(&client_message_id)
}

/// Desiste de uma mensagem que não foi gravada, e devolve o texto dela.
///
/// O texto volta para que a janela possa pô-lo de novo no compositor: desistir
/// não pode querer dizer perder o que se escreveu.
#[tauri::command]
fn descartar_mensagem(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    client_message_id: String,
) -> Result<Option<String>, ConnectionError> {
    Ok(session
        .conexao(carimbo)?
        .descartar_mensagem(&client_message_id))
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
    // **A ação declarada** (API 7, capacidade `acoes`): com ela, o pedido vai
    // como `AcaoDeMod`, e quem confere a ação e a permissão é o servidor. Sem
    // ela, é o `ModRequest` de sempre. A janela não decide nada aqui.
    acao: Option<String>,
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
    let conexao = session.connection()?;
    let saiu = match acao {
        Some(acao) => conexao.acao_de_mod(request, id.clone(), channel, acao, payload),
        None => conexao.mod_request(request, id.clone(), channel, payload),
    };
    if let Err(erro) = &saiu {
        pedido_recusado_na_saida_dito(&id, request, erro);
    }
    saiu
}

/// **Entra num túnel de jogo** (API 8, `SeeleUI.tunel.entrar`): abre a porta em
/// `127.0.0.1` em que o jogo de quem joga deve ligar, e devolve o número dela.
///
/// **Só a janela chama, e só depois de gastar o bilhete de gesto** (um clique,
/// 2 s, uso único; `base.js`). O Rust não confere o clique, que é da janela, mas
/// confere o que é dele: a geração, e que este MOD está de pé nela com o poder
/// `porta` no manifesto que o pacote provou. Quem confere o MOD ligado no
/// servidor, a ação `entrar-no-jogo` e a máquina que serve é o servidor, a cada
/// entrada.
///
/// A recusa é sempre `Recusado { motivo }`, com o nome em kebab-case de
/// `RecusaDeTunel` (ou `mod-sem-porta` e `sessao-encerrada`, daqui): a frase é
/// da janela.
///
/// # Errors
///
/// [`FalhaNoMod::Recusado`], como descrito.
#[tauri::command]
async fn tunel_entrar(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    geracao: u64,
    id: String,
    nome: String,
) -> Result<seele_ffi::PortaDeTunel, FalhaNoMod> {
    entrar_no_tunel_do_mod(&session, carimbo, geracao, &id, nome).await
}

/// **O dono da máquina aceitou** o pedido de máquina pendente de `resumo`
/// (API 8 parte 3): o aceite é guardado, a metade de máquina é lida do pacote
/// agora e ligada, e o `AceiteDeMaquina` sai. Ver [`maquina::aceitar`].
///
/// # Errors
///
/// `sem-pedido`, `pedido-velho` (o resumo não é o do pedido pendente),
/// `sem-conexao`, ou o nome da recusa da leitura do pacote ou do aceite.
#[tauri::command]
async fn maquina_aceitar(
    app: AppHandle,
    alvo: String,
    mod_id: String,
    servidor: String,
    resumo: String,
) -> Result<seele_ffi::mods::AceiteDeMaquinaGuardado, String> {
    // Numa thread de bloqueio: subir a metade pode esperar até 2 s a anterior
    // da mesma chave parar.
    tauri::async_runtime::spawn_blocking(move || {
        maquina::aceitar(&app, &alvo, &mod_id, &servidor, &resumo)
    })
    .await
    .unwrap_or_else(|_| Err("falhou".to_owned()))
}

/// O dono recusou o pedido de máquina: `RecusaDeMaquina { PeloDono }`.
///
/// # Errors
///
/// `sem-pedido`, `sem-conexao`, `sessao-encerrada`.
#[tauri::command]
fn maquina_recusar(
    app: AppHandle,
    alvo: String,
    mod_id: String,
    servidor: String,
) -> Result<(), String> {
    maquina::recusar(&app, &alvo, &mod_id, &servidor)
}

/// **DESLIGAR**: `aoParar("pelo-dono")`, os túneis dela caem, o executor
/// para e o servidor recebe `MaquinaParada { PeloDono }`. O aceite fica.
///
/// # Errors
///
/// `nao-esta-ligada`.
#[tauri::command]
async fn maquina_desligar(
    app: AppHandle,
    alvo: String,
    mod_id: String,
    servidor: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        maquina::desligar(&app, &alvo, &mod_id, &servidor, maquina::Fim::PeloDono)
    })
    .await
    .unwrap_or_else(|_| Err("falhou".to_owned()))
}

/// **REVOGAR**: o mesmo que DESLIGAR com `revogada`, e o aceite guardado é
/// apagado — o próximo pedido pergunta de novo.
///
/// # Errors
///
/// `sem-aceite` (nada ligado nem guardado), ou o nome da recusa do arquivo.
#[tauri::command]
async fn maquina_revogar(
    app: AppHandle,
    alvo: String,
    mod_id: String,
    servidor: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        maquina::desligar(&app, &alvo, &mod_id, &servidor, maquina::Fim::Revogada)
    })
    .await
    .unwrap_or_else(|_| Err("falhou".to_owned()))
}

/// Os aceites de máquina guardados nesta máquina, com quais estão de pé.
///
/// # Errors
///
/// `NaoDeuParaLer`.
#[tauri::command]
fn maquinas_aceitas(app: AppHandle) -> Result<Vec<maquina::MaquinaAceita>, String> {
    maquina::aceitas(&app)
}

/// Os jogos de uma execução anterior do app que esta achou rodando ao abrir,
/// e o que fez com cada um (e2e de 31/10, passo 14).
#[tauri::command]
fn maquinas_sobras(app: AppHandle) -> Vec<maquina::SobraNaTela> {
    maquina::sobras(&app)
}

/// A recusa de um comando de túnel que não achou a sessão do carimbo: ela
/// acabou (`sessao-encerrada`), ou é outra a razão (`falhou`, dita no registro).
fn recusa_da_sessao_do_tunel(erro: &ConnectionError) -> FalhaNoMod {
    let motivo = if matches!(erro, ConnectionError::NotConnected) {
        "sessao-encerrada"
    } else {
        tracing::warn!(
            ?erro,
            "comando de túnel recusado pela conferência do carimbo"
        );
        "falhou"
    };
    FalhaNoMod::Recusado {
        motivo: motivo.to_owned(),
    }
}

/// O que a janela recebe quando quem joga salvou um arquivo do MOD.
#[derive(serde::Serialize)]
struct SalvoParaOMod {
    /// Quantos bytes foram gravados. **O caminho não volta**: ele é da pessoa.
    bytes: usize,
}

/// **O nome que o diálogo sugere**, do que o MOD propôs: só letras, números,
/// espaço, `.`, `-`, `_` e parênteses, até 80 caracteres, sem começar por `.`
/// (nada de arquivo escondido), e `arquivo` quando não sobra nada. O MOD
/// sugere; quem escolhe o nome e a pasta é a pessoa, no diálogo do sistema.
fn nome_sugerido_ao_salvar(nome: &str) -> String {
    let limpo: String = nome
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_' | '(' | ')'))
        .take(80)
        .collect();
    let limpo = limpo.trim().trim_start_matches('.').trim();
    if limpo.is_empty() {
        "arquivo".to_owned()
    } else {
        limpo.to_owned()
    }
}

/// Grava `bytes` em `destino` **de uma vez**: num parcial ao lado, renomeado
/// no fim. Um salvamento que cai no meio não deixa meio `.mrpack` com o nome
/// que a pessoa escolheu.
fn gravar_o_salvo(destino: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut parcial = destino.as_os_str().to_owned();
    parcial.push(".seele-parcial");
    let parcial = std::path::PathBuf::from(parcial);
    let gravado = std::fs::write(&parcial, bytes).and_then(|()| std::fs::rename(&parcial, destino));
    if gravado.is_err() {
        let _ = std::fs::remove_file(&parcial);
    }
    gravado
}

/// **`SeeleUI.salvar`** (API 8 parte 6; spec §6): quem joga recebe um arquivo
/// que a metade de servidor guardou (o `.mrpack`), pelo diálogo de salvar do
/// sistema. **O MOD nunca escolhe o caminho**: ele sugere um nome, e o caminho
/// fica com a pessoa e não volta a ele.
///
/// Só para o MOD de pé com o poder `porta` (a capacidade `salvar`, conferida
/// aqui com o que o Rust leu do pacote); o clique é conferido pela janela,
/// como o do túnel. Os bytes vêm pelo fluxo de volume de sempre: `payload` vai
/// ao `aoPedir` da metade de servidor, que autoriza respondendo o caminho.
///
/// # Errors
///
/// [`FalhaNoMod::Recusado`] com `mod-sem-porta`, `sessao-encerrada`, o texto
/// do servidor (`arquivo-grande-demais:<t>:<teto>`, a recusa do MOD), ou
/// `nao-gravei` quando o disco recusou o caminho escolhido.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn salvar_do_mod(
    app: AppHandle,
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    channel: u32,
    payload: String,
    nome: String,
) -> Result<Option<SalvoParaOMod>, FalhaNoMod> {
    use tauri_plugin_dialog::DialogExt as _;
    session.recusar_se_morta(geracao)?;
    let pode = session
        .mods_nativos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
        .pode(geracao, &id, "salvar");
    if !pode {
        tracing::warn!(mod_id = %id_no_registro(&id), geracao, "o MOD pediu para salvar um arquivo e não está de pé com o poder porta");
        return Err(FalhaNoMod::Recusado {
            motivo: "mod-sem-porta".to_owned(),
        });
    }
    let sugerido = nome_sugerido_ao_salvar(&nome);
    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title("Salvar o arquivo que este MOD oferece")
        .set_file_name(&sugerido)
        .save_file(move |escolha| {
            let _ = envia.try_send(escolha);
        });
    // **Cancelar é uma resposta**: `null`, e o MOD sabe.
    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(None);
    };
    let Ok(destino) = escolha.into_path() else {
        return Err(FalhaNoMod::Recusado {
            motivo: "caminho-invalido".to_owned(),
        });
    };
    session.recusar_se_morta(geracao)?;
    let conexao = session.connection().map_err(|_| FalhaNoMod::Recusado {
        motivo: "sessao-encerrada".to_owned(),
    })?;
    let bytes = conexao
        .ler_imagem_mod(id.clone(), channel, payload)
        .await
        .map_err(|motivo| {
            tracing::warn!(mod_id = %id_no_registro(&id), motivo = %motivo, "salvar: o servidor recusou o arquivo");
            FalhaNoMod::Recusado { motivo }
        })?;
    let quantos = bytes.len();
    let gravado = tauri::async_runtime::spawn_blocking(move || gravar_o_salvo(&destino, &bytes))
        .await
        .map_err(|_| FalhaNoMod::Recusado {
            motivo: "nao-gravei".to_owned(),
        })?;
    if let Err(erro) = gravado {
        tracing::warn!(mod_id = %id_no_registro(&id), %erro, "salvar: o disco recusou o caminho escolhido");
        return Err(FalhaNoMod::Recusado {
            motivo: "nao-gravei".to_owned(),
        });
    }
    tracing::info!(mod_id = %id_no_registro(&id), geracao, bytes = quantos, "arquivo de MOD salvo pela pessoa");
    Ok(Some(SalvoParaOMod { bytes: quantos }))
}

#[path = "testes/salvar_testes.rs"]
#[cfg(test)]
mod salvar_testes;

/// O corpo de [`tunel_entrar`], sem o `State`: é o que deixa um teste chamá-lo
/// com uma `Session` à mão.
async fn entrar_no_tunel_do_mod(
    session: &Session,
    carimbo: sessoes::CarimboDaJanela,
    geracao: u64,
    id: &str,
    nome: String,
) -> Result<seele_ffi::PortaDeTunel, FalhaNoMod> {
    // Antes do efeito, como todo comando de MOD.
    session.recusar_se_morta(geracao)?;
    let pode = session
        .mods_nativos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
        .pode(geracao, id, "tunel");
    if !pode {
        // Dito: um MOD que pede uma porta sem ter o poder é um defeito dele, e
        // a recusa chega a quem hospeda além de chegar a ele.
        tracing::warn!(
            mod_id = %id_no_registro(id),
            geracao,
            "o MOD pediu uma porta de jogo e não está de pé com o poder porta"
        );
        return Err(FalhaNoMod::Recusado {
            motivo: "mod-sem-porta".to_owned(),
        });
    }
    // **A sessão do MOD pelo carimbo que ele trouxe**, e não a da vista: um
    // comando de sessão nunca se endereça pela vista (`Session::conexao`).
    let conexao = session
        .conexao(carimbo)
        .map_err(|erro| recusa_da_sessao_do_tunel(&erro))?;
    conexao
        .entrar_no_tunel(id.to_owned(), nome)
        .await
        .map_err(|motivo| FalhaNoMod::Recusado {
            motivo: motivo.to_owned(),
        })
}

/// **Fecha os túneis de jogo de um MOD** que saiu (desligado, descarregado,
/// sessão nova, troca de vista): a porta fecha e os fluxos acabam. Fechar o que
/// não existe não é erro.
///
/// **Endereçado pelo carimbo da sessão a que o MOD pertencia**, e não pela
/// vista nem pela geração dos MODs: na troca de vista a geração velha já não
/// vale e a sessão de antes continua viva em segundo plano — era ali que a
/// porta ficava aberta, sem MOD e sem ninguém ver. Fechar é sempre permitido.
///
/// # Errors
///
/// [`FalhaNoMod::Recusado`] com `sessao-encerrada` só quando a sessão do carimbo
/// de fato acabou (o enlace já derrubou os túneis ao sair).
#[tauri::command]
fn tunel_sair_do_mod(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    id: String,
) -> Result<(), FalhaNoMod> {
    session
        .conexao(carimbo)
        .and_then(|conexao| conexao.sair_dos_tuneis_do_mod(id))
        .map_err(|erro| recusa_da_sessao_do_tunel(&erro))
}

/// Os túneis de jogo abertos agora **na sessão do carimbo**, com o último
/// relatório de cada um: o que a faixa do produto lê ao carregar e quando a
/// vista muda. Sem a sessão, lista vazia — e é a resposta certa: ela acabou, e
/// não há porta aberta.
#[tauri::command]
fn tuneis_abertos(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Vec<seele_ffi::RelatorioDeTunel> {
    session
        .conexao(carimbo)
        .map(|conexao| conexao.tuneis_abertos())
        .unwrap_or_default()
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
    carimbo: sessoes::CarimboDaJanela,
    channel: u32,
    body: String,
    caminho: String,
    nome: String,
    tipo: String,
) -> Result<u64, ConnectionError> {
    session
        .conexao(carimbo)?
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
    carimbo: sessoes::CarimboDaJanela,
    anexo: u64,
) -> Result<(), ConnectionError> {
    session
        .conexao(carimbo)?
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
    carimbo: sessoes::CarimboDaJanela,
    anexo: u64,
) -> Result<SaveDestination, SaveRefused> {
    let pasta = pasta_dos_anexos(&app);
    // Sem sessão não há histórico, e sem histórico não há de onde tirar o nome.
    let Ok(connection) = session.conexao(carimbo) else {
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
async fn prever_anexo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    anexo: u64,
) -> Result<Preview, ConnectionError> {
    let connection = session.conexao(carimbo)?;
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
    carimbo: sessoes::CarimboDaJanela,
    name: String,
    limit: u16,
    channel: Option<u32>,
) -> Result<(), ConnectionError> {
    session
        .conexao(carimbo)?
        .create_voice_room(name, limit, channel)
}

/// Pede ao servidor que faça uma Linha.
#[tauri::command]
fn criar_linha(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    name: String,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.create_channel(name)
}

/// Pede ao servidor que renomeie uma sala de voz.
#[tauri::command]
fn renomear_voice_room(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    voice_room: u32,
    name: String,
) -> Result<(), ConnectionError> {
    session
        .conexao(carimbo)?
        .rename_voice_room(voice_room, name)
}

/// Pede ao servidor que renomeie uma Linha.
#[tauri::command]
fn renomear_linha(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    channel: u32,
    name: String,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.rename_channel(channel, name)
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
    carimbo: sessoes::CarimboDaJanela,
    name: String,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.rename_server(name.clone())?;
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

/// A regra da senha do servidor, para a tela escrevê-la **antes** de alguém
/// digitar: o mínimo de caracteres e o teto em bytes. A mesma que o `hospedar`
/// confere (`senha_do_servidor::conferir`): um limite que só aparece na recusa
/// é um limite que a pessoa descobre errando.
#[tauri::command]
const fn regras_da_senha_do_server() -> senha_do_servidor::Regras {
    senha_do_servidor::regras()
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
    carimbo: sessoes::CarimboDaJanela,
) -> Result<bool, ConnectionError> {
    let Some(pronto) = escolher_e_encolher(&app, "Escolha a imagem deste servidor").await? else {
        return Ok(false);
    };
    session.conexao(carimbo)?.set_server_icon(Some(pronto))?;
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
    carimbo: sessoes::CarimboDaJanela,
    icone: Vec<u8>,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.set_server_icon(Some(icone))
}

/// Tira a imagem do servidor, deixando-o sem nenhuma.
///
/// Verbo próprio e não `escolher` com um argumento vazio: são duas coisas que
/// uma pessoa faz por motivos diferentes, e são dois botões na tela.
#[tauri::command]
fn tirar_icone_do_server(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.set_server_icon(None)
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
    /// **O MOD virou parte do produto** (0.19): o servidor não aceita ligá-lo, e a
    /// tela de quem hospeda diz «Perfis agora faz parte do SEELE (0.19)…». Nada
    /// foi gravado.
    ModIncorporado {
        /// O MOD (`seele/perfis`).
        id: String,
        /// A versão do produto que o incorporou (`0.19`).
        desde: String,
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
        // **Só para leitura** (P1 da 0.17): o `Persistence::open` migrava o
        // banco de cada servidor parado, sem cópia, só para perguntar se ele
        // usa este pacote. `id` e `hash` das linhas ligadas existem do esquema
        // 11 em diante, como lê `mods::mods_de_api_antiga`.
        let usa =
            mods::hash_ligado_no_banco(&banco, &hash).map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
        if usa {
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
    let CodigoDoMod {
        fonte: codigo,
        capacidades,
        ..
    } = ler_codigo_do_mod(app, session, geracao, id, &hash)?;

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
            capacidades,
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
    // As capacidades do manifesto, e não o número da API: o `tunel` é do poder.
    let capacidades = alvo.capacidades.clone();
    alvo.executor
        .iniciar_com_capacidades(&codigo, capacidades)
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

/// Liga um MOD no servidor que este processo hospeda.
async fn habilitar_mod(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    hash: String,
) -> Result<(), FalhaNoMod> {
    // Por hash: dois pacotes do mesmo MOD podem estar no disco, e quem liga
    // sabe quais bytes quer — ler por identificador pegaria «o que estiver
    // lá», que pode ser o que outro servidor desta máquina baixou depois. E os
    // campos todos de uma vez, pela mesma construção da saída 1 de quem hospeda
    // com um MOD de API antiga (`mods::linha_do_pacote`): `habilitado_de`.
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
/// a deste build: é ela que o anúncio diz. Ver `a_api_do_mod_habilitado`. **E o
/// `tema` também** (migração 16; D-api-8): a tela de aceite diz se o MOD pinta a
/// aparência do servidor. O conjunto o gravava falso, à mão, para todo MOD.
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
        tema: instalado.tema,
        // Gravadas no mesmo ato que o hash (migração 20): as do manifesto dos
        // bytes, já efetivas.
        plataformas: Some(instalado.plataformas),
    }
}

#[path = "testes/habilitar_grava_as_plataformas_do_manifesto.rs"]
#[cfg(test)]
mod habilitar_grava_as_plataformas_do_manifesto;

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
    // **Toda sessão com o guardado, e não só a da vista** (E3 da 0.17): quem
    // hospeda pode estar olhando outro servidor enquanto aplica os MODs do
    // dele, e a sessão dele no próprio servidor, no fundo, é a que precisa do
    // sim. Uma sessão com outro servidor nunca recebe este sim.
    let (guardado, pessoal) = session
        .hospedagens
        .lock()
        .map_or((None, None), |hospedagens| hospedagens.impressoes());
    for sessao in session.sessoes.todas() {
        let onde = onde_estou_em(
            Some(&sessao.endereco),
            sessao.conexao.impressao_do_servidor().as_deref(),
            guardado.as_deref(),
            pessoal.as_deref(),
        );
        gravar_o_sim_se_for_o_guardado(config, Some(&sessao.endereco), onde, identidade)?;
    }
    // E a vista, como sempre: é ela que existe sem registro (a janela que
    // hospeda e entrou por outro caminho).
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
#[path = "testes/o_sim_de_quem_hospeda_vai_so_ao_servidor_daqui.rs"]
#[cfg(test)]
mod o_sim_de_quem_hospeda_vai_so_ao_servidor_daqui;

#[path = "testes/o_conjunto_recusa_o_incorporado.rs"]
#[cfg(test)]
mod o_conjunto_recusa_o_incorporado;

#[path = "testes/o_sim_de_quem_hospeda_vem_antes_da_queda.rs"]
#[cfg(test)]
mod o_sim_de_quem_hospeda_vem_antes_da_queda;

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

/// O texto dos módulos de teste deste arquivo, que moram em `testes/`, um
/// arquivo por módulo.
///
/// Para o guarda que **proíbe** um texto no arquivo inteiro, ou conta quantas
/// vezes ele aparece: o [`FONTE_DESTE_ARQUIVO`] não traz mais o que um teste
/// escreve, e quem lesse só ele deixaria de ver o que via quando os testes
/// eram texto do `main.rs`. Quem procura o corpo de uma função de produção
/// não precisa disto.
///
/// Lido do disco, e a pasta inteira: um módulo novo entra sem ser listado.
#[cfg(test)]
fn fonte_dos_modulos_de_teste() -> String {
    let pasta = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/testes");
    let mut arquivos: Vec<std::path::PathBuf> = std::fs::read_dir(&pasta)
        .unwrap_or_else(|erro| panic!("{} não abre: {erro}", pasta.display()))
        .map(|entrada| entrada.expect("uma entrada de src/testes").path())
        .filter(|caminho| caminho.extension().is_some_and(|extensao| extensao == "rs"))
        .collect();
    arquivos.sort();
    assert!(
        !arquivos.is_empty(),
        "{} não tem mais os módulos de teste do `main.rs`: o guarda que os lê ficou cego",
        pasta.display()
    );
    arquivos
        .iter()
        .map(|arquivo| {
            std::fs::read_to_string(arquivo)
                .unwrap_or_else(|erro| panic!("{} não se lê: {erro}", arquivo.display()))
                .replace("\r\n", "\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    /// Respondeu uma imagem que cabe no teto de bytes e declara mais pixels do
    /// que uma prévia decodifica. Com os números: a janela a diz no lugar da
    /// imagem, e «grande demais» sem número não diz o que passou.
    PixelsDemais {
        /// A largura declarada.
        largura: u32,
        /// A altura declarada.
        altura: u32,
        /// O teto, em pixels (largura × altura).
        teto: u64,
    },
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
    if let seele_ffi::preview::Verdict::Draw(_) = seele_ffi::preview::judge(&alegado, &bytes) {
        return imagem_de_link_para_a_janela(bytes).await;
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
            seele_ffi::preview::Verdict::Draw(_) => imagem_de_link_para_a_janela(bytes).await,
            _ => Err(FalhaNaPrevia::NaoEImagem),
        };
    }

    Err(FalhaNaPrevia::NaoEImagem)
}

/// **A imagem de um link como ela vai à janela**: o `data:` com o tipo que os
/// bytes provaram ser, reduzida ao lugar em que a prévia aparece
/// (`reducao::previa_na_janela`, o mesmo caminho da prévia de um anexo).
///
/// O teto de bytes (`PREVIEW_LIMIT`) limita a busca, e não o que a imagem
/// ocupa aberta: quem controla o endereço escolhia as medidas, e um PNG de
/// poucos megabytes que declara 30 000 × 30 000 ia inteiro ao `<img>` da
/// conversa (a varredura do incidente da capa em 4K, 11/11/2026). Fora da
/// linha do runtime: decodificar e reduzir uma imagem grande leva tempo.
///
/// # Errors
///
/// [`FalhaNaPrevia::PixelsDemais`], com as medidas e o teto, e
/// [`FalhaNaPrevia::NaoEImagem`] para a que não abre.
async fn imagem_de_link_para_a_janela(bytes: Vec<u8>) -> Result<String, FalhaNaPrevia> {
    let previa = tauri::async_runtime::spawn_blocking(move || reducao::previa_na_janela(bytes))
        .await
        .map_err(|_| FalhaNaPrevia::NaoEImagem)?;
    imagem_de_link_pronta(previa)
}

/// O desfecho de [`reducao::previa_na_janela`], na resposta de
/// [`previa_de_link`]. Fora da função assíncrona para um teste a ler.
fn imagem_de_link_pronta(previa: reducao::PreviaNaJanela) -> Result<String, FalhaNaPrevia> {
    match previa {
        reducao::PreviaNaJanela::Pronta(uri) => Ok(uri),
        reducao::PreviaNaJanela::PixelsDemais { largura, altura } => {
            Err(FalhaNaPrevia::PixelsDemais {
                largura,
                altura,
                teto: reducao::TETO_DE_PIXELS,
            })
        }
        reducao::PreviaNaJanela::Ilegivel => Err(FalhaNaPrevia::NaoEImagem),
    }
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
) -> Result<CodigoDoMod, FalhaNoMod> {
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
) -> Result<CodigoDoMod, FalhaNoMod> {
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
    // **As capacidades saem da mesma leitura**: os poderes que o manifesto
    // declara decidem se o MOD tem `SeeleUI.tunel`, e uma segunda leitura do
    // pacote seria uma segunda chance de discordar da que provou o hash.
    let capacidades = pacote.capacidades();
    let api = pacote.api;
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
    let fonte = String::from_utf8(bytes).map_err(|_| FalhaNoMod::Recusado {
        motivo: "cliente-nao-e-utf8".to_owned(),
    })?;
    Ok(CodigoDoMod {
        fonte,
        api,
        capacidades,
    })
}

/// O código de um MOD lido do disco, com o que o manifesto dele deixa chamar.
///
/// Para a janela é o par `[fonte, api]` de sempre (a serialização é a de uma
/// tupla): as capacidades são do lado do Rust, e é ele quem monta o prelúdio.
#[derive(Debug)]
struct CodigoDoMod {
    /// O que o pacote traz.
    fonte: String,
    /// A API que o manifesto declara.
    api: u32,
    /// As capacidades da API mais as dos poderes.
    capacidades: Vec<&'static str>,
}

impl serde::Serialize for CodigoDoMod {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.fonte, self.api).serialize(serializer)
    }
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

impl MidiaDoMod {
    /// A mídia pronta do caminho comum, com o `data:`.
    fn da_pronta(pronta: seele_ffi::midia_de_mod::MidiaPronta) -> Self {
        Self {
            uri: Some(pronta.uri),
            papel: pronta.papel,
            bytes: pronta.bytes,
            decodificados: pronta.decodificados,
        }
    }
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
        FalhaNoMod::ModIncorporado { .. } => "mod-incorporado",
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
    teto: Option<&reducao::Teto>,
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
            // **Reduzida ao lugar em que aparece antes do `data:`**, como a
            // do servidor (`seele_ffi::midia_de_mod::reduzir_ao_uso`). A
            // imagem do pacote ia à janela do tamanho que o MOD a trouxe — até
            // 10 MiB, qualquer medida —, e era o irmão do incidente da capa em
            // 4K (10/11/2026) que a varredura daquela vez deixou sem conserto.
            // A que declara pixels demais é recusada aqui, pelo nome e com os
            // números, e a recusa vai ao registro logo abaixo.
            let (bytes, decodificados) = seele_ffi::midia_de_mod::reduzir_ao_uso(bytes, teto)
                .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
            let lida = seele_ffi::mods::ler_midia(&bytes).ok_or_else(desconhecido)?;
            Ok(MidiaDoMod {
                uri: Some(lida.uri),
                papel: lida.papel,
                bytes: lida.bytes,
                decodificados,
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
///
/// **`uso`** nomeia o lugar em que a imagem vai aparecer (`"fundo"`), como em
/// [`midia_em_bytes`]; sem ele, vale o teto geral da mídia de MOD. Fora da
/// thread da janela (`async`), pelo mesmo motivo de lá: reduzir uma imagem
/// grande leva tempo, e um comando síncrono do Tauri roda na thread principal.
#[tauri::command(async)]
fn midia_do_mod(
    app: AppHandle,
    session: State<'_, Session>,
    geracao: u64,
    id: String,
    hash: String,
    caminho: String,
    uso: Option<String>,
) -> Result<MidiaDoMod, FalhaNoMod> {
    session.recusar_se_morta(geracao)?;
    let midia = midia_declarada_do_mod(
        &session.baldes_do_registro,
        std::time::Instant::now(),
        &config_dir(&app),
        &id,
        &hash,
        &caminho,
        uso.as_deref().and_then(reducao::teto_do_uso).as_ref(),
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

/// **O piso do som decodificado mora no `seele-sessao`** (`som`) desde a
/// Tarefa 2.6 da F4: o navegador recusa pelo mesmo número.
///
/// **O som que, decodificado, não cabe no teto do som, recusado antes de a
/// janela o decodificar** — ver [`seele_ffi::som::som_cabe_decodificado`].
fn som_cabe_decodificado(bytes: &[u8]) -> Result<(), FalhaNoMod> {
    seele_ffi::som::som_cabe_decodificado(bytes).map_err(|motivo| FalhaNoMod::Recusado { motivo })
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

/// O texto do `midia_em_bytes`, desfeito e pronto pelo caminho comum
/// (`seele_ffi::midia_de_mod`, que o navegador usa também; Tarefa 2.6 da F4).
fn ler_midia_do_servidor(
    base64: &str,
    teto: Option<&reducao::Teto>,
) -> Result<MidiaDoMod, FalhaNoMod> {
    seele_ffi::midia_de_mod::do_texto(base64, teto)
        .map(MidiaDoMod::da_pronta)
        .map_err(|motivo| FalhaNoMod::Recusado { motivo })
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
        // Consumido antes do envio, dê o envio certo ou não.
        guardados
            .tirar(geracao, &id, arquivo)
            .map_err(recusado)?
            .conteudo
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
    // O caminho comum (`seele_ffi::midia_de_mod::pronta`): o piso do som, a
    // redução ao uso e o `data:`, como o navegador.
    lida.and_then(|bytes| seele_ffi::midia_de_mod::pronta(bytes, teto))
        .map(MidiaDoMod::da_pronta)
        .map_err(|motivo| {
            recusa_de_midia_dita(baldes, agora, id, "volume", FalhaNoMod::Recusado { motivo })
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
#[cfg(test)]
fn tipo_do_escolhido(bytes: &[u8]) -> Result<seele_ffi::mods::TipoLido, FalhaNoMod> {
    seele_ffi::escolhidos::tipo_do_escolhido(bytes).map_err(recusado)
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
    let titulo = seele_ffi::escolhidos::titulo_do_seletor(finalidade.as_deref());

    // **O filtro é orientação, e não a fronteira.** A prova do tipo continua
    // saindo dos bytes, mais abaixo; o que ele evita é oferecer quarenta JSONs
    // a quem foi buscar um retrato. Extensões que o produto não conhece são
    // descartadas: um MOD não escolhe o que o seletor da máquina de outra
    // pessoa aceita (`seele_ffi::escolhidos::extensoes_do_seletor`, a regra
    // que o navegador usa também).
    let aceitas = seele_ffi::escolhidos::extensoes_do_seletor(extensoes.as_deref());

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
    // O teto do produto vale sempre; o que o MOD declarou só o aperta. O do
    // `.mrpack` (64 MiB, API 8 parte 6) só para o MOD que pediu o papel
    // `pack`; a recusa diz o tamanho e o teto.
    let teto = seele_ffi::escolhidos::teto_do_pedido(limite_de_bytes, papeis.as_deref());
    if tamanho > teto {
        return Err(FalhaNoMod::Recusado {
            motivo: seele_ffi::escolhidos::grande_demais(tamanho, teto),
        });
    }
    let bytes = std::fs::read(&caminho).map_err(|_| FalhaNoMod::Recusado {
        motivo: "arquivo-ilegivel".to_owned(),
    })?;

    // **O tipo sai dos bytes**, como o da mídia do pacote. A extensão que a
    // pessoa tem no disco é texto, e texto não escolhe decodificador.
    let lida = seele_ffi::escolhidos::tipo_do_escolhido_para(&bytes, papeis.as_deref())
        .map_err(recusado)?;

    // **E o papel provado é conferido contra o que o MOD pediu.** Sem isto, um
    // MOD que precisa de retrato recebia um som e só descobria ao tentar usá-lo
    // — quando descobria. O erro tem nome próprio para a região poder dizê-lo.
    o_papel_serve(papeis.as_deref(), lida.papel)?;

    // Conferida **de novo** depois do `await` do seletor: a pessoa pode ter
    // demorado a escolher, e a sessão pode ter acabado no meio.
    session.recusar_se_morta(geracao)?;
    let mut guardados = session
        .arquivos_escolhidos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?;
    let numero = guardados
        .guardar(Escolhido {
            de_quem: id.clone(),
            geracao,
            tipo: lida.media_type,
            tamanho,
            conteudo: bytes,
        })
        .map_err(recusado)?;
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
    pedaco_em_base64(&guardados, geracao, &id, arquivo, inicio)
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
    session
        .arquivos_escolhidos
        .lock()
        .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
        .soltar(geracao, &id, arquivo);
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
///
/// `ModIncorporado` é a primeira conferência: um MOD que o produto incorporou
/// (o PERFIS, na 0.19) não se liga mais, antes de ler disco algum.
fn conferir_incorporados(ligados: &[ModEscolhido]) -> Result<(), FalhaNoMod> {
    seele_server::mods::incorporados::conferir_conjunto(ligados.iter().map(|e| e.id.as_str()))
        .map_err(|r| FalhaNoMod::ModIncorporado {
            id: r.id.to_owned(),
            desde: r.desde.to_owned(),
        })
}

#[tauri::command]
async fn aplicar_conjunto_de_mods(
    app: AppHandle,
    session: State<'_, Session>,
    ligados: Vec<ModEscolhido>,
    base: String,
) -> Result<String, FalhaNoMod> {
    conferir_incorporados(&ligados)?;
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
        let identidade = seele_server::mods::anuncio::conjunto_exigido(&persistence)
            .map_err(|_| FalhaNoMod::BancoNaoRespondeu)?
            .identidade;

        // **Um sim, para o conjunto resultante.** Salvar já é a decisão
        // informada do operador sobre o conjunto que vai valer; perguntar de
        // novo, logo depois, seria perguntar o que acabou de ser respondido.
        // Isto não autoriza mudança futura, e não dispensa o aceite de quem
        // entra. E só ao servidor daqui (lote X, rodada 1): ver
        // `gravar_o_sim_de_quem_hospeda`.
        //
        // **Ainda com o banco na mão** (e2e de 31/10, passo 1): a sessão de
        // quem hospeda cai com `ModsMudaram` como a de todo mundo, e só lê o
        // conjunto novo depois deste cadeado. Com o sim já no disco, a volta
        // dela (`voltarSozinhoSeOConjuntoEMeu`, em `tela-fim.js`) passa direto;
        // gravado depois, a queda podia chegar antes do sim, e quem acabou de
        // ouvir «você não será perguntado de novo» lia «entre de novo».
        gravar_o_sim_de_quem_hospeda(&config_dir(&app), &session, &identidade)
            .map_err(|motivo| FalhaNoMod::Recusado { motivo })?;
        drop(persistence);
        identidade
    };
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
#[path = "testes/so_a_gestao_reaplica_no_servidor_hospedado.rs"]
#[cfg(test)]
mod so_a_gestao_reaplica_no_servidor_hospedado;

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
    // é o caso da tela inicial, e a imagem sobe no próximo `connect`. O
    // retrato é desta máquina (E3): vai a todas as sessões.
    for connection in session.sessoes.conexoes() {
        connection.set_person_icon(Some(pronto.clone()))?;
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
fn escolher_apelido(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    apelido: String,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.set_nickname(apelido)
}

/// Tira a sua imagem.
///
/// # Errors
///
/// O que a sessão devolver.
#[tauri::command]
fn tirar_minha_imagem(app: AppHandle, session: State<'_, Session>) -> Result<(), ConnectionError> {
    gravar_retrato(&app, None)?;
    // Da máquina (E3): sai de todas as sessões.
    for connection in session.sessoes.conexoes() {
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
fn imagem_da_pessoa(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    person: u64,
) -> Option<String> {
    let bytes = session.conexao(carimbo).ok()?.person_icon(person)?;
    Some(format!(
        "data:image/png;base64,{}",
        seele_ffi::base64_de(&bytes)
    ))
}

// ---- o perfil público (SEELE 0.19, protocolo 12) ---------------------------
//
// Os mesmos comandos que o navegador atende (`seele-web::comandos`), com os
// mesmos nomes, argumentos e respostas. Todos de SESSÃO: a conexão pelo
// carimbo. A validação, o portão de versão (nada do protocolo 12 sai para um
// servidor 0.18) e a conferência da imagem moram na `Connection`
// (`seele_ffi::perfis`); aqui só há o seletor de arquivo e o que é desta
// máquina. Os erros voltam tipados, pelo nome da variante.

/// Os perfis públicos desta sessão, em ordem de pessoa. A tela os pede quando o
/// `perfis_revision` do snapshot (ou o `PerfisChanged`) anda; vazia num
/// servidor 0.18.
#[tauri::command]
fn perfis(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<sessoes::NaSessao<Vec<seele_ffi::perfis::PerfilNaTela>>, ConnectionError> {
    let conexao = session.conexao(carimbo)?;
    Ok(sessoes::NaSessao::de(
        &conexao,
        session.geracao(),
        conexao.perfis(),
    ))
}

/// Grava o perfil de quem está usando esta janela. `revisao` é a que o editor
/// viu (zero para quem ainda não tem perfil). Os campos são validados aqui
/// pela mesma regra do servidor, e o erro volta **por campo**; o apelido é o
/// `escolher_apelido`.
///
/// # Errors
///
/// `Campos`, `ServidorAntigo`, `NotConnected`.
#[tauri::command]
fn gravar_perfil(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u32,
    entrada: seele_ffi::perfis::EntradaDoPerfil,
) -> Result<(), seele_ffi::perfis::FalhaNoPerfil> {
    session
        .conexao(carimbo)?
        .gravar_perfil(revisao, &entrada)
        .map_err(Into::into)
}

/// Tira a foto ou a capa de quem está usando esta janela.
///
/// # Errors
///
/// `ServidorAntigo`, `NotConnected`.
#[tauri::command]
fn tirar_imagem_do_perfil(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    lado: seele_ffi::perfis::LadoDoPerfil,
) -> Result<(), seele_ffi::perfis::FalhaNoPerfil> {
    session
        .conexao(carimbo)?
        .tirar_imagem_do_perfil(lado)
        .map_err(Into::into)
}

/// **Abre o seletor e prepara a imagem escolhida para o passo de recorte**
/// (`recorte.rs`): a janela recebe a prévia e as medidas, e os bytes inteiros
/// esperam aqui pela alça, que `enviar_imagem_do_perfil` e
/// `guardar_imagem_do_perfil_padrao` recebem de volta com o pedaço escolhido.
///
/// **Confere antes de mostrar**: até 10 MiB e o tipo pelos bytes (PNG, JPEG,
/// WebP ou GIF; o HEIC volta nomeado). Sem filtro de extensão no seletor, de
/// propósito: quem escolhe uma foto do iPhone precisa ouvir «HEIC não é aceito»
/// e não achar que o arquivo sumiu.
///
/// Sem sessão: é o mesmo passo na tela inicial (o perfil padrão) e dentro de um
/// servidor.
///
/// # Errors
///
/// `ImagemGrandeDemais`, `Heic`, `TipoRecusado`, `ImagemVazia`,
/// `ArquivoIlegivel`.
#[tauri::command]
async fn escolher_imagem_para_recortar(
    app: AppHandle,
    lado: seele_ffi::perfis::LadoDoPerfil,
) -> Result<recorte::ParaRecortar, seele_ffi::perfis::ErroDoPerfil> {
    use seele_ffi::perfis::{ErroDoPerfil, LadoDoPerfil};
    use tauri_plugin_dialog::DialogExt as _;

    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title(match lado {
            LadoDoPerfil::Foto => "Escolha a sua foto",
            LadoDoPerfil::Capa => "Escolha a sua capa",
        })
        .pick_file(move |escolha| {
            let _ = envia.try_send(escolha);
        });
    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(recorte::ParaRecortar::nada());
    };
    // Só o Android devolve `content://`, e este binário não roda lá.
    let Ok(caminho) = escolha.into_path() else {
        return Err(ErroDoPerfil::ArquivoIlegivel);
    };
    // O tamanho é perguntado antes de ler, e a leitura e a prévia são fora da
    // linha principal: dez megabytes no laço de eventos congelam a janela, e
    // decodificar um papel de parede em 8K leva um tempo que se vê.
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = seele_ffi::perfis::ler_o_arquivo_da_imagem(&caminho)?;
        recorte::preparar(&app.state::<recorte::ImagensARecortar>(), bytes)
    })
    .await
    .map_err(|_| ErroDoPerfil::ArquivoIlegivel)?
}

/// **Solta a imagem que esperava o recorte**: a pessoa cancelou o diálogo, e
/// os bytes (até 10 MiB) não têm por que ficar na memória. A alça de outra
/// imagem não solta nada.
#[tauri::command]
fn esquecer_imagem_a_recortar(a_recortar: State<'_, recorte::ImagensARecortar>, alca: u64) {
    drop(a_recortar.tirar(alca));
}

/// O que `enviar_imagem_do_perfil` fez.
#[derive(serde::Serialize)]
struct ImagemDoPerfilEnviada {
    /// Na foto nova, o ícone nativo (PNG de até 256 px e 8 KiB, do primeiro
    /// quadro) também foi mandado pelo `SetPersonIcon`, para os clientes 0.18
    /// verem o mesmo rosto. `false` quando a imagem não decodifica aqui (o
    /// perfil já foi gravado; só o rosto dos 0.18 fica como estava) ou quando
    /// é a capa.
    icone_nativo: bool,
}

/// Manda a imagem escolhida (a da `alca`, de `escolher_imagem_para_recortar`),
/// recortada no `recorte`, como a foto ou a capa de quem está usando esta
/// janela.
///
/// **O que sobe é o pedaço escolhido, pelo caminho de sempre**
/// ([`imagem_de_perfil_para_subir`]): os mesmos tetos, o mesmo formato, a
/// mesma conferência da `Connection` antes de gastar a subida.
///
/// **Foto nova gera o ícone nativo** (§9 Q1 da especificação): o mesmo
/// `icone::encolher` do `escolher_minha_imagem`, e o `SetPersonIcon` vai só
/// para **esta** sessão — o perfil é do servidor, e o retrato desta máquina
/// (`retrato.png`) não é tocado.
///
/// # Errors
///
/// `Mudou` (a alça não é a da imagem que espera), `TipoRecusado`,
/// `ServidorAntigo`, a recusa do servidor, `NotConnected`.
#[tauri::command]
async fn enviar_imagem_do_perfil(
    session: State<'_, Session>,
    a_recortar: State<'_, recorte::ImagensARecortar>,
    carimbo: sessoes::CarimboDaJanela,
    lado: seele_ffi::perfis::LadoDoPerfil,
    alca: u64,
    recorte: reducao::Recorte,
) -> Result<ImagemDoPerfilEnviada, seele_ffi::perfis::FalhaNoPerfil> {
    use seele_ffi::perfis::{ErroDoPerfil, LadoDoPerfil};

    let conexao = session.conexao(carimbo)?;
    let bytes = a_recortar.tirar(alca).ok_or(ErroDoPerfil::Mudou)?;
    let bytes = imagem_de_perfil_para_subir(bytes, lado, recorte).await?;

    let para_o_icone = (lado == LadoDoPerfil::Foto).then(|| bytes.clone());
    conexao.enviar_imagem_do_perfil(lado, bytes).await?;

    let mut icone_nativo = false;
    if let Some(bytes) = para_o_icone {
        let pronto = tauri::async_runtime::spawn_blocking(move || icone::encolher(&bytes))
            .await
            .ok()
            .flatten();
        match pronto {
            Some(png) => match conexao.set_person_icon(Some(png)) {
                Ok(()) => icone_nativo = true,
                Err(erro) => tracing::warn!(
                    ?erro,
                    "a foto do perfil subiu, mas o ícone nativo para os clientes 0.18 não saiu"
                ),
            },
            None => tracing::warn!(
                "a foto do perfil subiu, mas não virou um ícone nativo: os clientes 0.18 \
                 seguem vendo o de antes"
            ),
        }
    }
    Ok(ImagemDoPerfilEnviada { icone_nativo })
}

/// **A imagem de perfil que a pessoa escolheu, recortada e do tamanho em que
/// aparece** (`recorte::recortada`: o pedaço escolhido, e depois o teto de
/// `reducao::imagem_do_perfil_para_a_janela` — a capa em até 1280 × 720, a
/// foto em até 512), fora da linha principal.
///
/// É o que sobe ao servidor e o que fica guardado no perfil padrão. Subia o
/// arquivo inteiro, e quem recebia uma capa em 4K pagava por ela: os clientes
/// de antes da redução ao mostrar (a 0.19.6) ficam lentos com ela.
///
/// **O recorte que é a imagem inteira sai como saía antes do recorte**, com os
/// mesmos bytes quando ela já cabia: quem confirma sem recortar uma imagem que
/// já está na proporção não perde nada.
///
/// O que não decodifica ou tem pixels demais é recusado; o recorte sem pixel
/// nenhum dentro da imagem é `Mudou`.
async fn imagem_de_perfil_para_subir(
    bytes: Vec<u8>,
    lado: seele_ffi::perfis::LadoDoPerfil,
    recorte: reducao::Recorte,
) -> Result<Vec<u8>, seele_ffi::perfis::ErroDoPerfil> {
    let capa = lado == seele_ffi::perfis::LadoDoPerfil::Capa;
    tauri::async_runtime::spawn_blocking(move || recorte::recortada(&bytes, capa, recorte))
        .await
        .map_err(|_| seele_ffi::perfis::ErroDoPerfil::ArquivoIlegivel)?
}

/// A imagem de um perfil, em `data:` para a janela desenhar. `hash` é o que o
/// `perfis` entrega (hexadecimal), e a imagem é cacheada por ele para sempre:
/// a segunda chamada não toca a rede. `Mudou` quer dizer que o perfil trocou a
/// imagem — a tela relê `perfis` e pede a nova.
///
/// # Errors
///
/// `Mudou`, `Incompleto`, `ServidorAntigo`, `NotConnected`.
#[tauri::command]
async fn imagem_do_perfil(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    pessoa: u64,
    lado: seele_ffi::perfis::LadoDoPerfil,
    hash: String,
) -> Result<String, seele_ffi::perfis::FalhaNoPerfil> {
    use seele_ffi::perfis::{conferir_imagem, hash_de_hex, tipo_de_midia, ErroDoPerfil};

    let conexao = session.conexao(carimbo)?;
    // Um hash que não tem forma de hash é um perfil que mudou debaixo da tela.
    let hash = hash_de_hex(&hash).ok_or(ErroDoPerfil::Mudou)?;
    let bytes = conexao.imagem_do_perfil(pessoa, lado, hash).await?;
    conferir_imagem(&bytes).map_err(|_| ErroDoPerfil::TipoRecusado)?;
    // **Reduzida ao lugar em que aparece, antes de atravessar a ponte.** A
    // imagem chega do tamanho que subiu (até 10 MiB, qualquer medida), e uma
    // capa em 4K ia à janela como um `data:` de megabytes: o aplicativo inteiro
    // ficava lento (relato do dono, 10/11/2026). Fora da linha principal:
    // decodificar e reduzir uma imagem grande leva tempo.
    let capa = lado == seele_ffi::perfis::LadoDoPerfil::Capa;
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        seele_ffi::reducao::imagem_do_perfil_para_a_janela(&bytes, capa)
    })
    .await
    .map_err(|_| ErroDoPerfil::TipoRecusado)?
    .map_err(|_| ErroDoPerfil::TipoRecusado)?;
    let tipo = conferir_imagem(&bytes).map_err(|_| ErroDoPerfil::TipoRecusado)?;
    Ok(format!(
        "data:{};base64,{}",
        tipo_de_midia(tipo),
        seele_ffi::base64_de(&bytes)
    ))
}

// ---- o perfil padrão desta máquina (0.19, «Padrão + ajuste por servidor») ---
//
// O «Seu perfil» da tela inicial edita o perfil inteiro, e o que se salva ali é
// o padrão desta máquina (`perfil_padrao.rs`). Quem decide **quando** aplicá-lo
// a um servidor é a tela (`ui/perfil-padrao.js`), a mesma no desktop e no
// navegador: ela vê a lista `Perfis` de cada servidor, e é nela que «não tenho
// perfil aqui» se distingue de «tenho um perfil vazio». Aqui mora o que é desta
// máquina — o disco, o seletor — e a subida dos bytes guardados.

/// O armazém do padrão, na pasta das preferências.
fn armazem_do_padrao<R: tauri::Runtime>(app: &AppHandle<R>) -> perfil_padrao::Armazem {
    perfil_padrao::Armazem::em(std::path::PathBuf::from(config_dir(app)).join("perfil-padrao"))
}

/// O perfil padrão desta máquina, com as imagens em `data:`. Revisão zero é
/// «nunca salvo». Sem sessão: é o que a tela inicial edita.
#[tauri::command]
fn perfil_padrao(app: AppHandle) -> perfil_padrao::PerfilPadraoNaTela {
    armazem_do_padrao(&app).na_tela()
}

/// Grava os textos do padrão, validados pela regra do servidor
/// (`seele_proto::perfil::validar`): o erro volta **por campo**, antes de
/// qualquer servidor. Devolve o padrão salvo.
///
/// # Errors
///
/// `Campos` ou `NaoGravei`.
#[tauri::command]
fn gravar_perfil_padrao(
    app: AppHandle,
    entrada: seele_ffi::perfis::EntradaDoPerfil,
) -> Result<perfil_padrao::PerfilPadraoNaTela, perfil_padrao::FalhaNoPadrao> {
    let armazem = armazem_do_padrao(&app);
    armazem.gravar_textos(&entrada)?;
    Ok(armazem.na_tela())
}

/// Guarda a imagem escolhida (a da `alca`, de `escolher_imagem_para_recortar`),
/// recortada no `recorte`, como a foto ou a capa do padrão.
///
/// **A foto do padrão também vira o retrato desta máquina** (o ícone nativo de
/// 8 KiB que sobe em todo `connect`): é o rosto que os servidores 0.18 veem, e
/// o da pílula da tela inicial. Uma foto que não decodifica aqui deixa o
/// retrato como estava, e o registro diz.
///
/// # Errors
///
/// `Mudou` (a alça não é a da imagem que espera), `TipoRecusado`, `NaoGravei`.
#[tauri::command]
async fn guardar_imagem_do_perfil_padrao(
    app: AppHandle,
    a_recortar: State<'_, recorte::ImagensARecortar>,
    lado: seele_ffi::perfis::LadoDoPerfil,
    alca: u64,
    recorte: reducao::Recorte,
) -> Result<(), perfil_padrao::FalhaNoPadrao> {
    use seele_ffi::perfis::{ErroDoPerfil, LadoDoPerfil};

    let bytes = a_recortar.tirar(alca).ok_or(ErroDoPerfil::Mudou)?;
    let armazem = armazem_do_padrao(&app);
    // **Guardada do tamanho em que aparece.** O que fica guardado é o que
    // sobe a cada servidor, e o hash dele é o que diz se o servidor já tem
    // esta imagem: reduzir só ao enviar faria os dois nunca baterem.
    let bytes = imagem_de_perfil_para_subir(bytes, lado, recorte).await?;
    let para_o_retrato = (lado == LadoDoPerfil::Foto).then(|| bytes.clone());
    armazem.guardar_imagem(lado, &bytes)?;
    if let Some(bytes) = para_o_retrato {
        let pronto = tauri::async_runtime::spawn_blocking(move || icone::encolher(&bytes))
            .await
            .ok()
            .flatten();
        match pronto {
            Some(png) => {
                if gravar_retrato(&app, Some(&png)).is_err() {
                    tracing::warn!("a foto do perfil padrão foi guardada, mas o retrato desta máquina não");
                }
            }
            None => tracing::warn!(
                "a foto do perfil padrão não virou um retrato: os servidores 0.18 seguem vendo o de antes"
            ),
        }
    }
    Ok(())
}

/// Tira a foto ou a capa do padrão. Tirar a foto tira também o retrato desta
/// máquina, que é feito dela.
///
/// # Errors
///
/// `NaoGravei`.
#[tauri::command]
fn tirar_imagem_do_perfil_padrao(
    app: AppHandle,
    lado: seele_ffi::perfis::LadoDoPerfil,
) -> Result<(), perfil_padrao::FalhaNoPadrao> {
    let armazem = armazem_do_padrao(&app);
    let tinha = armazem.ler().foto.is_some();
    armazem.tirar_imagem(lado)?;
    if lado == seele_ffi::perfis::LadoDoPerfil::Foto && tinha {
        gravar_retrato(&app, None)?;
    }
    Ok(())
}

/// O que esta máquina sabe do servidor da sessão e do padrão.
#[derive(serde::Serialize)]
struct SeguimentoNaTela {
    /// A impressão do servidor, que é a chave; `None` quando a conexão ainda
    /// não a sabe — e então nada é aplicado, porque não haveria onde anotar.
    servidor: Option<String>,
    #[serde(flatten)]
    seguimento: perfil_padrao::Seguimento,
}

/// Se o servidor da sessão segue o padrão, a revisão aplicada por último e a
/// assinatura do que foi aplicado. Sem registro, não segue.
///
/// # Errors
///
/// `NotConnected`, `SessaoNaoConfere`.
#[tauri::command]
fn seguimento_do_perfil_padrao(
    app: AppHandle,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<SeguimentoNaTela, ConnectionError> {
    let servidor = session.conexao(carimbo)?.impressao_do_servidor();
    let seguimento = servidor
        .as_deref()
        .map(|servidor| armazem_do_padrao(&app).seguimento(servidor))
        .unwrap_or_default();
    Ok(SeguimentoNaTela {
        servidor,
        seguimento,
    })
}

/// Anota, para o servidor da sessão, se ele segue o padrão.
///
/// # Errors
///
/// `NotConnected`, `SessaoNaoConfere`, `NaoGravei` (inclusive quando a
/// conexão não sabe a impressão do servidor).
#[tauri::command]
fn guardar_seguimento_do_perfil_padrao(
    app: AppHandle,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    segue: bool,
    revisao_aplicada: u64,
    assinatura: Option<String>,
) -> Result<(), perfil_padrao::FalhaNoPadrao> {
    let servidor = session
        .conexao(carimbo)?
        .impressao_do_servidor()
        .unwrap_or_default();
    armazem_do_padrao(&app).guardar_seguimento(
        &servidor,
        perfil_padrao::Seguimento {
            segue,
            revisao_aplicada,
            assinatura,
        },
    )
}

/// **Sobe a foto ou a capa do padrão** para o servidor da sessão, como se a
/// pessoa a tivesse escolhido ali. `hash` é o que o `perfil_padrao` deu: se o
/// padrão trocou a imagem no meio, volta `Mudou` e nada sobe.
///
/// # Errors
///
/// `Mudou`, os de `enviar_imagem_do_perfil` (a conferência, o portão de
/// versão, a recusa do servidor), `NotConnected`.
#[tauri::command]
async fn enviar_imagem_do_perfil_padrao(
    app: AppHandle,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    lado: seele_ffi::perfis::LadoDoPerfil,
    hash: String,
) -> Result<(), perfil_padrao::FalhaNoPadrao> {
    let conexao = session.conexao(carimbo)?;
    let armazem = armazem_do_padrao(&app);
    let bytes = tauri::async_runtime::spawn_blocking(move || armazem.bytes_da_imagem(lado, &hash))
        .await
        .map_err(|_| seele_ffi::perfis::ErroDoPerfil::ArquivoIlegivel)??;
    conexao.enviar_imagem_do_perfil(lado, bytes).await?;
    Ok(())
}

/// O relatório da migração dos perfis do MOD PERFIS, para quem hospeda, ou
/// nada quando não há (ou já foi dispensado). Fica no banco do servidor até ser
/// dispensado.
///
/// Só vale numa janela que hospeda; sem hospedagem é `NaoEstaHospedando`, como
/// os outros comandos do servidor desta janela.
#[tauri::command]
async fn ler_relatorio_da_migracao_de_perfis(
    session: State<'_, Session>,
) -> Result<Option<seele_server::persistence::perfis::RelatorioDaMigracaoDePerfis>, FalhaNoMod> {
    let persistence = persistence_do_mod(&session)?;
    let persistence = persistence.lock().await;
    seele_server::persistence::perfis::relatorio(&persistence).map_err(|erro| {
        tracing::warn!(%erro, "não li o relatório da migração dos perfis");
        FalhaNoMod::BancoNaoRespondeu
    })
}

/// Dispensa o relatório da migração dos perfis: quem hospeda o leu.
#[tauri::command]
async fn dispensar_relatorio(session: State<'_, Session>) -> Result<(), FalhaNoMod> {
    let persistence = persistence_do_mod(&session)?;
    let persistence = persistence.lock().await;
    seele_server::persistence::perfis::dispensar_relatorio(&persistence).map_err(|erro| {
        tracing::warn!(%erro, "não dispensei o relatório da migração dos perfis");
        FalhaNoMod::BancoNaoRespondeu
    })
}

/// Os bytes da imagem que está valendo, ou nada.
///
/// Fora do `Snapshot` de propósito, e a tela respeita o mesmo acordo: o
/// snapshot é lido duas vezes por segundo e atravessa a ponte em JSON, então
/// ele carrega `icon_revision` — um número — e a casca só vem buscar os bytes
/// quando o número anda. É o precedente de `messages_revision`.
#[tauri::command]
fn icone_do_server(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<Option<Vec<u8>>, ConnectionError> {
    Ok(session.conexao(carimbo)?.server_icon())
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
fn expulsar_pessoa(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    person: u64,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.kick_person(person)
}

// ---- cargos e permissões (ADR 0064; os corpos estão em `cargos.rs`, de F) ----
//
// Todos de SESSÃO: a conexão pelo carimbo, nunca pela vista (fronteiras.md
// §12). Quem decide é o servidor; as recusas dele chegam pelo evento da sessão,
// e a prévia antecipa a mesma regra só para a tela explicar.

/// O quadro de cargos desta sessão, os apelidos de quem aparece nele e o que
/// esta pessoa pode. Também pede o quadro ao servidor, que responde pelo evento.
#[tauri::command]
fn pedir_cargos(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<sessoes::NaSessao<seele_ffi::cargos::CargosNaTela>, ConnectionError> {
    let conexao = session.conexao(carimbo)?;
    let valor = cargos::pedir(&conexao)?;
    Ok(sessoes::NaSessao::de(&conexao, session.geracao(), valor))
}

/// Por que esta pessoa tem, ou não, cada permissão.
#[tauri::command]
fn explicar_permissoes(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    pessoa: u64,
) -> Result<sessoes::NaSessao<Vec<seele_ffi::cargos::PermissaoExplicada>>, cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    let valor = cargos::explicar(&conexao, pessoa)?;
    Ok(sessoes::NaSessao::de(&conexao, session.geracao(), valor))
}

/// Quem ganha e quem perde o quê com este verbo, ou a recusa que o servidor
/// daria. Só informativa: a confirmação revalida tudo no servidor.
#[tauri::command]
fn prever_cargo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u64,
    pedido: seele_ffi::cargos::PedidoDeCargo,
) -> Result<sessoes::NaSessao<seele_ffi::cargos::Previa>, cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    let valor = cargos::prever(&conexao, revisao, &pedido)?;
    Ok(sessoes::NaSessao::de(&conexao, session.geracao(), valor))
}

/// Cria um cargo, na posição 1.
#[tauri::command]
fn criar_cargo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u64,
    nome: String,
    concede: Vec<seele_ffi::cargos::Permissao>,
    restringe: Vec<seele_ffi::cargos::Permissao>,
) -> Result<(), cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    cargos::mandar(
        &conexao,
        revisao,
        cargos::pedido::criar(nome, concede, restringe),
    )
}

/// Troca o nome e as listas de um cargo.
#[tauri::command]
fn editar_cargo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u64,
    cargo: u32,
    nome: String,
    concede: Vec<seele_ffi::cargos::Permissao>,
    restringe: Vec<seele_ffi::cargos::Permissao>,
) -> Result<(), cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    cargos::mandar(
        &conexao,
        revisao,
        cargos::pedido::editar(cargo, nome, concede, restringe),
    )
}

/// Põe um cargo noutra posição.
#[tauri::command]
fn mover_cargo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u64,
    cargo: u32,
    posicao: u32,
) -> Result<(), cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    cargos::mandar(&conexao, revisao, cargos::pedido::mover(cargo, posicao))
}

/// Apaga um cargo e as atribuições dele.
#[tauri::command]
fn apagar_cargo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u64,
    cargo: u32,
) -> Result<(), cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    cargos::mandar(&conexao, revisao, cargos::pedido::apagar(cargo))
}

/// Dá um cargo a uma pessoa.
#[tauri::command]
fn atribuir_cargo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u64,
    pessoa: u64,
    cargo: u32,
) -> Result<(), cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    cargos::mandar(&conexao, revisao, cargos::pedido::atribuir(pessoa, cargo))
}

/// Tira um cargo de uma pessoa.
#[tauri::command]
fn revogar_cargo(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    revisao: u64,
    pessoa: u64,
    cargo: u32,
) -> Result<(), cargos::FalhaDeCargo> {
    let conexao = session.conexao(carimbo)?;
    cargos::mandar(&conexao, revisao, cargos::pedido::revogar(pessoa, cargo))
}

/// Pede ao servidor que impeça alguém de voltar — `banir`.
///
/// `expires_at` em segundos desde a época; `None` é para sempre. O `reason` é
/// para o registro de quem hospeda e nunca chega a quem foi banido.
#[tauri::command]
fn banir_pessoa(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    person: u64,
    reason: Option<String>,
    expires_at: Option<i64>,
) -> Result<(), ConnectionError> {
    session
        .conexao(carimbo)?
        .ban_person(person, reason, expires_at)
}

/// Pede ao servidor que tire um banimento — `desbanir` (protocolo 13).
///
/// Só quem pode banir, e a hierarquia é a de `banir_pessoa`: quem recusa é o
/// servidor. A resposta é a lista de banidos inteira de novo
/// (`Event::BanidosChanged`), e a pessoa volta ao FORA DO AR de todo mundo.
#[tauri::command]
fn desbanir_pessoa(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    person: u64,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.unban_person(person)
}

/// Pede ao servidor a lista dos banimentos que valem (protocolo 13).
///
/// Pede, e não espera: a lista chega pelo `BanidosChanged` e é lida em
/// [`banidos`]. Ela não se atualiza sozinha quando outra pessoa bane — a tela
/// pede de novo cada vez que abre a lista.
#[tauri::command]
fn listar_banidos(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.list_bans()
}

/// A lista de banidos como chegou da última vez que foi pedida.
///
/// Fora do `snapshot`, como o FORA DO AR e pela mesma razão: um MOD recebe o
/// retrato, e nenhum MOD ganhou a lista de quem foi banido e por quê.
#[tauri::command]
fn banidos(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<sessoes::NaSessao<Vec<seele_ffi::Banido>>, ConnectionError> {
    let conexao = session.conexao(carimbo)?;
    Ok(sessoes::NaSessao::de(
        &conexao,
        session.geracao(),
        conexao.banidos(),
    ))
}

/// Pede ao servidor que tire uma mensagem da Linha.
///
/// Sem permissão nenhuma quando a mensagem é de quem pede: a permissão do
/// `specs/04-servidor-seele.md` diz «de outra pessoa».
#[tauri::command]
fn remover_mensagem(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    message: u64,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.remove_message(message)
}

/// Pede ao servidor que mova alguém para uma sala de voz — `mover_pessoa`.
#[tauri::command]
fn mover_pessoa(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    person: u64,
    voice_room: u32,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.move_person(person, voice_room)
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
fn apagar_voice_room(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    voice_room: u32,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.delete_voice_room(voice_room)
}

/// Pede ao servidor que destrua uma Linha, e tudo que foi escrito nela —
/// `apagar_linha`.
#[tauri::command]
fn apagar_linha(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    channel: u32,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.delete_channel(channel)
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
    carimbo: sessoes::CarimboDaJanela,
    channel: u32,
) -> Result<ChannelWeight, ConnectionError> {
    // O `Arc` sai do cadeado antes do `await`, e é de propósito: `Session::connection`
    // devolve um clone justamente para que nada segure o `Mutex` atravessando um
    // ponto de espera.
    let connection = session.conexao(carimbo)?;
    connection.weigh_channel(channel).await
}

/// O mudo **da máquina** (E3 da 0.17, ADR 0031): vale em todas as sessões, e
/// o servidor de cada uma o mostra no roster. Mudo num servidor e falando
/// noutro seria o microfone a decidir pelo foco, e não a pessoa.
///
/// # Errors
///
/// [`ConnectionError::NotConnected`] sem sessão nenhuma; senão, o primeiro
/// erro de uma delas (as outras recebem assim mesmo).
#[tauri::command]
fn set_muted(session: State<'_, Session>, on: bool) -> Result<(), ConnectionError> {
    em_todas(&session, |conexao| conexao.set_muted(on))
}

/// O isolamento total **da máquina**, pelo mesmo argumento do [`set_muted`].
///
/// # Errors
///
/// O mesmo de [`set_muted`].
#[tauri::command]
fn set_total_isolation(session: State<'_, Session>, on: bool) -> Result<(), ConnectionError> {
    em_todas(&session, |conexao| conexao.set_total_isolation(on))
}

/// Faz `fazer` em todas as conexões, e devolve o primeiro erro.
fn em_todas(
    session: &Session,
    mut fazer: impl FnMut(&Connection) -> Result<(), ConnectionError>,
) -> Result<(), ConnectionError> {
    let conexoes = session.sessoes.conexoes();
    if conexoes.is_empty() {
        return Err(ConnectionError::NotConnected);
    }
    let mut primeiro = Ok(());
    for conexao in &conexoes {
        if let Err(erro) = fazer(conexao) {
            if primeiro.is_ok() {
                primeiro = Err(erro);
            }
        }
    }
    primeiro
}

/// Push-to-talk, reported as it happens.
///
/// Not fallible on purpose: a key coming *up* must never be refused. Returning
/// an error here would give the frontend a path where the microphone was opened
/// and the close was rejected.
///
/// **Desde a 0.17, vai à sessão que tem o foco de voz, e só a ela** (E3,
/// ADR 0031: «com voz exclusiva, espaço tem exatamente um significado»),
/// qualquer que seja a sessão na vista. Uma soltura vai a todas: nenhuma tecla
/// pode ficar segura numa sessão que perdeu o foco no meio da pressão.
#[tauri::command]
fn set_talking(session: State<'_, Session>, talking: bool) {
    segurar_a_fala(&session, talking);
}

/// [`set_talking`] sem o `State`: a tecla de falar da janela e a de fora dela
/// ([`tecla_global`]) abrem e fecham o mesmo microfone, pela mesma regra.
fn segurar_a_fala(session: &Session, talking: bool) {
    if talking {
        if let Some(com_o_foco) = session.sessoes.com_o_foco() {
            com_o_foco.conexao.set_talking(true);
        }
    } else {
        for conexao in session.sessoes.conexoes() {
            conexao.set_talking(false);
        }
    }
}

/// Escolhe como o microfone abre: grava no disco e, se houver sessão, aplica agora.
///
/// **As duas metades, nesta ordem**, pelo mesmo argumento que
/// [`escolher_microfone`] escreve por extenso: a escrita é o que faz a escolha
/// valer amanhã, a aplicação é o que faz valer agora.
///
/// Antes daqui só existia a segunda, e o preço tinha duas faces. Quem escolhia
/// voz achava push-to-talk de volta no dia seguinte — e um padrão é argumento
/// sobre quem não escolheu, e não sobre quem escolheu. E, por exigir sessão,
/// não dava para escolher o modo na tela de entrada; agora dá.
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
    // Da máquina (E3): em todas as sessões, para o foco que chegar já chegar
    // com o modo escolhido.
    for connection in session.sessoes.conexoes() {
        connection.set_voice_mode(mode);
    }
    // Saindo do modo tecla, a tecla de fora da janela volta ao sistema.
    tecla_global::avisar(&app);
    Ok(())
}

/// Como o microfone vai abrir na próxima sessão: o modo escolhido, ou, sem
/// escolha, o padrão desta instalação. `None` só se as preferências não abrem.
///
/// **O padrão também, e não só a escolha.** Antes daqui quem nunca escolheu
/// recebia `None`, e a configuração aberta pela entrada desenhava os três modos
/// sem nenhum marcado — uma tela que não dizia o que ia valer. Com a voz de
/// padrão para quem chega, é ali que a pessoa descobre que há uma tecla a
/// escolher, e ela precisa ver qual dos três está valendo. É também o que a
/// página responde por este nome (`modoDeVozGuardado`, em `ponte-web.js`).
///
/// Do disco e não do `Snapshot`, pela mesma razão que [`microfone_escolhido`]:
/// esta pergunta tem resposta sem sessão nenhuma, e é a que a tela de entrada
/// precisa fazer.
#[tauri::command]
fn modo_de_voz_escolhido(app: AppHandle) -> Option<VoiceMode> {
    preferencias(&app).map(|p| VoiceMode::from(p.modo_do_microfone()))
}

/// Assenta o padrão do microfone desta instalação: a voz para quem chega, a
/// tecla para quem já usava. Uma vez; depois disso, só relê.
///
/// Chamado no `setup`, **antes** de a janela existir — ver lá. Sem preferências
/// (o arquivo não abre) ou com o disco recusando, nada é assentado, e a sessão
/// fica na tecla, que é o lado seguro de não saber.
fn assentar_o_padrao_do_microfone<R: tauri::Runtime>(app: &AppHandle<R>) {
    let Some(mut ajustes) = preferencias(app) else {
        tracing::warn!("sem preferências: o padrão do microfone não foi assentado");
        return;
    };
    let ja_assentado = ajustes.padrao_do_microfone().is_some();
    let padrao = seele_ffi::assentar_o_padrao_do_microfone(&config_dir(app), &mut ajustes);
    if !ja_assentado {
        tracing::info!(?padrao, "padrão do microfone desta instalação assentado");
    }
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
    // Da máquina (E3): em todas as sessões.
    for conexao in session.sessoes.conexoes() {
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
    // Da máquina (E3): em todas as sessões.
    for conexao in session.sessoes.conexoes() {
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
    // Os controles são da máquina (E3) e valem iguais em todas as sessões: a
    // com o foco responde, e sem foco, qualquer uma.
    let alguma = session
        .sessoes
        .com_o_foco()
        .map(|sessao| Arc::clone(&sessao.conexao))
        .or_else(|| session.sessoes.conexoes().into_iter().next());
    if let Some(conexao) = alguma {
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
        })?;
    // A tecla de fora da janela é a mesma: trocada aqui, ela troca no sistema.
    if let Some(global) = app.try_state::<tecla_global::TeclaGlobal>() {
        global.anotar(ajustes.tecla_de_falar_global(), ajustes.push_to_talk_key());
    }
    tecla_global::avisar(&app);
    Ok(())
}

/// Como está a tecla de falar fora da janela: se a opção está ligada, e o que
/// a tela diz dela. Ver [`tecla_global`].
///
/// O último estado conferido, e não uma conferida nova: este comando roda na
/// thread principal, e conferir espera por ela.
#[tauri::command]
fn estado_da_tecla_global(estado: State<'_, tecla_global::TeclaGlobal>) -> tecla_global::Retrato {
    estado.retrato()
}

/// Liga ou desliga a tecla de falar fora da janela, e devolve o que a tela diz
/// agora — inclusive que a tecla escolhida não serve de atalho, que é o que
/// quem acabou de ligar precisa ler.
///
/// # Errors
///
/// [`FalhaAoEscolher::NaoGravei`] se o disco recusar. Nada muda no sistema.
#[tauri::command]
async fn ligar_tecla_global(
    app: AppHandle,
    ligada: bool,
) -> Result<tecla_global::Retrato, FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    ajustes.set_tecla_de_falar_global(ligada).map_err(|erro| {
        tracing::warn!(%erro, "não consegui gravar a tecla de falar fora da janela");
        FalhaAoEscolher::NaoGravei
    })?;
    if let Some(global) = app.try_state::<tecla_global::TeclaGlobal>() {
        global.anotar(ligada, ajustes.push_to_talk_key());
    }
    Ok(tecla_global::conferir_fora_da_thread(&app).await)
}

/// As notificações do sistema: a escolha desta máquina, e o que se sabe da
/// permissão do sistema — que no desktop é «não se sabe», dito assim. Ver
/// [`notificacao`].
///
/// Sem o arquivo de preferências, desligadas: é o padrão de quem nunca
/// escolheu, e é o que [`notificar_no_sistema`] vai conferir.
#[tauri::command]
fn estado_das_notificacoes(app: AppHandle) -> notificacao::Retrato {
    notificacao::Retrato::do_desktop(
        preferencias(&app)
            .as_ref()
            .is_some_and(seele_ffi::preferences::Preferences::notificacoes_do_sistema),
    )
}

/// Liga ou desliga as notificações do sistema, e devolve o que a tela diz
/// agora.
///
/// **Só grava.** Não sai notificação nenhuma daqui: no macOS o sistema não
/// mostra a faixa com o app na frente, que é onde a pessoa está ao ligar, e uma
/// «notificação de teste» que não aparece ensinaria que a opção não funciona.
/// A pergunta do sistema, onde ele a faz, vem com a primeira notificação de
/// verdade.
///
/// `confirmacao` é o texto da notificação que a **página de navegador** mostra
/// ao ligar, onde ela é a única prova de que o navegador notifica; a casca o
/// manda sempre, porque é a mesma nos dois lugares, e este lado o recebe e não
/// o usa.
///
/// # Errors
///
/// [`FalhaAoEscolher::NaoGravei`] se o disco recusar. Nada muda.
#[tauri::command]
fn ligar_notificacoes(
    app: AppHandle,
    ligadas: bool,
    confirmacao: Option<serde_json::Value>,
) -> Result<notificacao::Retrato, FalhaAoEscolher> {
    drop(confirmacao);
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    ajustes
        .set_notificacoes_do_sistema(ligadas)
        .map_err(|erro| {
            tracing::warn!(%erro, "não consegui gravar a escolha das notificações do sistema");
            FalhaAoEscolher::NaoGravei
        })?;
    Ok(notificacao::Retrato::do_desktop(ligadas))
}

/// Pede uma notificação ao sistema. A única porta do plugin: a página não o
/// alcança (`capabilities/janela.json`, `tests/permissoes.rs`).
///
/// A regra — o que notifica, a repetição, o texto — é da casca
/// (`ui/notificacao-do-sistema.js`). Aqui se confere o que só este lado sabe:
/// a preferência no disco e a janela na frente, lida do sistema. Ver
/// [`notificacao::pedir`].
///
/// O `motivo` é lista fechada e vai ao registro; o texto **não** vai, nem na
/// falha: ele tem o nome de um servidor ou de uma pessoa.
///
/// `lugar` é para onde o clique levaria (a sessão da menção, a portaria). O
/// plugin não devolve o clique no desktop — o sistema só traz o app à frente —,
/// então este lado o recebe e não tem o que fazer com ele; quem o usa é a
/// página de navegador, pela ponte.
///
/// # Errors
///
/// [`notificacao::RecusaDeNotificar`], pelo nome. `Ok` quer dizer que o pedido
/// foi entregue ao sistema, e não que a notificação apareceu: o plugin não
/// devolve isso.
#[tauri::command]
fn notificar_no_sistema(
    app: AppHandle,
    motivo: notificacao::Motivo,
    titulo: String,
    corpo: String,
    lugar: Option<serde_json::Value>,
) -> Result<(), notificacao::RecusaDeNotificar> {
    drop(lugar);
    let ligadas = preferencias(&app)
        .as_ref()
        .is_some_and(seele_ffi::preferences::Preferences::notificacoes_do_sistema);
    let pedido = notificacao::pedir(
        ligadas,
        notificacao::janela_na_frente(&app),
        &titulo,
        &corpo,
        |titulo, corpo| notificacao::emitir(&app, titulo, corpo),
    );
    match &pedido {
        Ok(()) => tracing::info!(?motivo, "notificação entregue ao sistema"),
        Err(recusa) => tracing::info!(?motivo, ?recusa, "notificação não saiu"),
    }
    pedido
}

/// O que a tecla de falar fora da janela precisa saber da voz: se a sessão com
/// a voz desta máquina está numa sala, e se o microfone abre por tecla.
///
/// A sessão com o foco de voz, e não a da vista: é a ela que [`segurar_a_fala`]
/// manda o aperto. Sem ela, ninguém está em sala.
fn voz_para_a_tecla_global<R: tauri::Runtime>(app: &AppHandle<R>) -> (bool, bool) {
    let Some(session) = app.try_state::<Session>() else {
        return (false, false);
    };
    let Some(com_o_foco) = session.sessoes.com_o_foco() else {
        return (false, false);
    };
    let retrato = com_o_foco.conexao.snapshot();
    (
        retrato.voice_rooms.iter().any(|sala| sala.occupied_by_us),
        retrato.voice_mode == VoiceMode::PushToTalk,
    )
}

#[tauri::command]
fn set_volume(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    nickname: String,
    percent: u16,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.set_volume(nickname, percent)
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
fn volumes_locais(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> std::collections::HashMap<String, u16> {
    session
        .conexao(carimbo)
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
    carimbo: sessoes::CarimboDaJanela,
) -> Result<Vec<seele_ffi::FonteDeTela>, ConnectionError> {
    session.conexao(carimbo)?.fontes_de_tela()
}

/// Uma prévia como ela vai à janela: de qual fonte (o `id` e o nome que a lista
/// deu) e a imagem, em `data:`.
#[derive(Clone, serde::Serialize)]
struct PreviaNaJanela {
    id: u64,
    nome: String,
    url: String,
}

/// **Tira a prévia de cada fonte da última lista** e as manda à janela uma a
/// uma (`seele://previa-de-tela`), para a caixa de compartilhar mostrar o que
/// cada monitor e cada janela tem, e não só o título.
///
/// Volta na hora: capturar leva tempo, e a lista não espera por isso. Uma
/// rodada por vez; um pedido no meio de outra não faz nada (a tela pede de
/// novo quando a lista muda).
#[tauri::command]
fn previas_de_tela(
    app: AppHandle,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ConnectionError> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static EM_CURSO: AtomicBool = AtomicBool::new(false);

    let conexao = session.conexao(carimbo)?;
    if EM_CURSO.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || {
        conexao.previas_de_tela(|id, nome, previa| {
            let Some(url) = previa_em_data(&previa) else {
                return;
            };
            let _ = app.emit(
                "seele://previa-de-tela",
                PreviaNaJanela {
                    id,
                    nome: nome.to_owned(),
                    url,
                },
            );
        });
        EM_CURSO.store(false, Ordering::Release);
    });
    Ok(())
}

/// A prévia em JPEG, como `data:`. Uma miniatura de 320 pontos fica em poucos
/// kilobytes; `None` se as medidas não batem com os bytes.
fn previa_em_data(previa: &seele_ffi::PreviaDeTela) -> Option<String> {
    let imagem = image::RgbImage::from_raw(previa.largura, previa.altura, previa.rgb.clone())?;
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 70)
        .encode_image(&imagem)
        .ok()?;
    Some(format!(
        "data:image/jpeg;base64,{}",
        seele_ffi::base64_de(&jpeg)
    ))
}

/// O que o sistema operacional respondeu sobre gravar a tela.
#[tauri::command]
fn permissao_de_tela(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<seele_ffi::PermissaoDeTela, ConnectionError> {
    Ok(session.conexao(carimbo)?.permissao_de_tela())
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
    carimbo: sessoes::CarimboDaJanela,
) -> Result<seele_ffi::PermissaoDeTela, ConnectionError> {
    Ok(session.conexao(carimbo)?.pedir_permissao_de_tela())
}

/// Abre a página dos Ajustes do Sistema em que o macOS lista quem pode gravar
/// a tela. É a primeira saída de quem teve o pedido negado, e diz quando não
/// abriu. Ver [`gravacao_de_tela`].
#[tauri::command]
async fn abrir_ajustes_da_gravacao_de_tela() -> Result<(), gravacao_de_tela::FalhaNaGravacaoDeTela>
{
    tauri::async_runtime::spawn_blocking(gravacao_de_tela::abrir_os_ajustes)
        .await
        .unwrap_or(Err(
            gravacao_de_tela::FalhaNaGravacaoDeTela::AjustesNaoAbriram,
        ))
}

/// Apaga a decisão que o macOS guardou sobre **este app** gravar a tela.
///
/// Só por aperto de botão, com a tela dizendo antes o que acontece: é o que o
/// «−» da página dos Ajustes faz, e serve a quem ficou com a permissão presa a
/// uma versão anterior. O identificador é o deste app, lido da configuração
/// dele, e nunca vem da página. Ver [`gravacao_de_tela::comando_de_redefinir`].
#[tauri::command]
async fn redefinir_permissao_de_tela(
    app: AppHandle,
) -> Result<(), gravacao_de_tela::FalhaNaGravacaoDeTela> {
    let identificador = app.config().identifier.clone();
    tauri::async_runtime::spawn_blocking(move || {
        gravacao_de_tela::redefinir_a_permissao(&identificador)
    })
    .await
    .unwrap_or(Err(
        gravacao_de_tela::FalhaNaGravacaoDeTela::PermissaoNaoRedefinida,
    ))
}

/// Fecha o app e abre de novo.
///
/// O macOS só refaz a resposta sobre gravar a tela quando o app reabre: quem
/// concedeu nos Ajustes com o SEELE aberto continua negado até lá.
///
/// `request_restart` e não `restart`: o primeiro passa pelo `RunEvent::Exit`,
/// e é nele que a sessão e as hospedagens se despedem
/// ([`despedir_no_fim_do_app`]); o segundo, chamado da linha principal, sai sem
/// avisar ninguém. A página pergunta antes quando há um servidor no ar.
#[tauri::command]
fn reabrir_o_app(app: AppHandle) {
    tracing::info!("reabrindo o app a pedido, pela caixa de compartilhar");
    app.request_restart();
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
    carimbo: sessoes::CarimboDaJanela,
    fonte: u64,
    limites: seele_ffi::LimitesDeTela,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.compartilhar_tela(fonte, limites)
}

/// O que o codificador desta janela faz com a câmera: as medidas, a cadência,
/// o teto de banda e o codec. É conta, e não fala com o servidor.
///
/// A janela pergunta antes de pedir a câmera ao servidor, com as medidas que o
/// sistema entregou: é com a resposta que ela põe o `VideoEncoder` à prova, e
/// uma janela que não codifica recusa antes de a sala ouvir que há uma câmera.
#[tauri::command]
fn codificacao_da_camera(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    largura: u32,
    altura: u32,
    limites: seele_ffi::LimitesDeTela,
) -> Result<seele_ffi::CodificacaoDaCamera, ConnectionError> {
    Ok(session
        .conexao(carimbo)?
        .codificacao_da_camera(largura, altura, limites))
}

/// Começa a transmitir a câmera, no lugar da tela.
///
/// Quem captura e codifica é a janela (`ui/camera.js`): este processo não tem
/// captura de câmera, porque o workspace proíbe `unsafe`. `largura` e `altura`
/// são as do codificador dela. `ConnectionError::ServidorAntigo` quando o
/// servidor é de antes do protocolo da câmera — nada sai.
#[tauri::command]
fn compartilhar_camera(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    largura: u32,
    altura: u32,
    limites: seele_ffi::LimitesDeTela,
) -> Result<(), ConnectionError> {
    session
        .conexao(carimbo)?
        .compartilhar_camera(largura, altura, limites)
}

/// O cabeçalho que diz se o quadro é um quadro-chave: `1` ou `0`.
const CABECALHO_DA_CHAVE: &str = "seele-chave";
/// O cabeçalho com o `SessaoId` da sessão a que o quadro pertence.
const CABECALHO_DA_SESSAO: &str = "seele-sessao";
/// O cabeçalho com a geração que a janela viu dessa sessão.
const CABECALHO_DA_GERACAO: &str = "seele-geracao";

/// Um quadro da câmera, codificado pela janela, a caminho da sala.
///
/// # O corpo é cru, e o resto vai nos cabeçalhos
///
/// Trinta quadros por segundo em base64 dentro de JSON seriam um terço de
/// bytes a mais e duas cópias por quadro. O corpo do pedido são os bytes do
/// quadro, em Annex-B, e o que um comando comum levaria em argumentos — a
/// sessão, a geração e se é quadro-chave — vai nos cabeçalhos.
///
/// O WebView que não entrega corpo cru (o `postMessage` que o Tauri usa quando
/// o protocolo `ipc:` não está disponível) manda os mesmos bytes como uma
/// lista de números: ela é lida, mais devagar, em vez de recusada.
///
/// # Errors
///
/// Nenhum quadro some calado: `CameraNotLive`, `CameraQueueFull` e
/// `CameraBadSize` voltam à janela, que diz o que houve; e os erros de carimbo
/// de todo comando de sessão.
#[tauri::command]
fn quadro_da_camera(
    session: State<'_, Session>,
    request: tauri::ipc::Request<'_>,
) -> Result<u32, ConnectionError> {
    let (carimbo, chave) = cabecalhos_do_quadro(request.headers())?;
    let bytes = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.clone(),
        tauri::ipc::InvokeBody::Json(valor) => serde_json::from_value::<Vec<u8>>(valor.clone())
            .map_err(|_| ConnectionError::CameraBadSize)?,
    };
    session.conexao(carimbo)?.quadro_da_camera(chave, bytes)
}

/// O carimbo e a marca de quadro-chave, lidos dos cabeçalhos do pedido.
///
/// Sem a sessão ou sem a geração o quadro não tem dono, e a resposta é a de
/// um comando sem sessão. A marca de chave ausente é «não é chave»: é o lado
/// seguro, porque quem assiste só começa a decodificar por um quadro-chave.
fn cabecalhos_do_quadro(
    cabecalhos: &tauri::http::HeaderMap,
) -> Result<(sessoes::CarimboDaJanela, bool), ConnectionError> {
    let numero =
        |nome: &str| -> Option<u64> { cabecalhos.get(nome)?.to_str().ok()?.trim().parse().ok() };
    let (Some(sessao), Some(geracao)) = (numero(CABECALHO_DA_SESSAO), numero(CABECALHO_DA_GERACAO))
    else {
        return Err(ConnectionError::NotConnected);
    };
    let chave = cabecalhos
        .get(CABECALHO_DA_CHAVE)
        .is_some_and(|valor| valor.as_bytes() == b"1");
    Ok((sessoes::CarimboDaJanela { sessao, geracao }, chave))
}

#[path = "testes/quadro_da_camera_testes.rs"]
#[cfg(test)]
mod quadro_da_camera_testes;

/// Para de transmitir. Idempotente do outro lado: parar sem estar
/// compartilhando não é erro.
#[tauri::command]
fn parar_de_compartilhar(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.parar_de_compartilhar()
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
fn pedir_quadro_chave(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    tela: u32,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.pedir_quadro_chave(tela)
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
fn assistir(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    tela: u32,
    quero: bool,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.assistir(tela, quero)
}

/// O volume do som da transmissão que se está assistindo.
///
/// R21, o controle **de quem assiste**: silenciar o conteúdo compartilhado sem
/// calar as pessoas. Independente do volume dos participantes e do isolamento
/// total — este último continua calando tudo, que é o que ele diz que faz.
#[tauri::command]
fn ajustar_volume_da_tela(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    volume: f32,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.ajustar_volume_da_tela(volume)
}

/// Cala ou devolve o som da transmissão, sem mexer no volume dela.
#[tauri::command]
fn calar_a_tela(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    calada: bool,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.calar_a_tela(calada)
}

/// O volume e o mudo do som da transmissão, como a tela os desenha.
#[tauri::command]
fn som_da_tela(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<Option<seele_ffi::SomDaTela>, ConnectionError> {
    Ok(session.conexao(carimbo)?.som_da_tela())
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
    carimbo: sessoes::CarimboDaJanela,
    limites: seele_ffi::LimitesDeTela,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.ajustar_limites_da_tela(limites)
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
fn tela_cheia(app: AppHandle, ligada: bool) -> Result<(), String> {
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
        return Ok(());
    };
    janela.set_fullscreen(ligada).map_err(|erro| {
        tracing::debug!(%erro, ligada, "a janela não trocou de modo");
        "Não foi possível alterar a tela cheia da janela.".to_owned()
    })
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
fn lembrar_aparencia_do_servidor(
    app: AppHandle,
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) {
    // A conexão e o «onde voltar» **da sessão do carimbo** (E3), e não da
    // vista: a aparência de um servidor não pode ir para a entrada de outro.
    let Ok(connection) = session.conexao(carimbo) else {
        return;
    };
    let Some(alvo) = session
        .sessoes
        .buscar(carimbo.sessao)
        .and_then(|sessao| sessao.alvo())
    else {
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
fn dispensar_aviso(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
) -> Result<(), ConnectionError> {
    session.conexao(carimbo)?.dispensar_aviso();
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
    // O consentimento é desta máquina (E3): declarado a todas as sessões.
    for connection in session.sessoes.conexoes() {
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

    // O microfone é da máquina (E3): a escolha vai a todas as sessões. Só a com
    // o foco tem voz de pé para trocar agora; as outras guardam para quando o
    // tiverem. Sem sessão, a escolha está gravada, e era tudo o que havia.
    let mut sumiu = false;
    for connection in session.sessoes.conexoes() {
        if let Err(erro) = connection.set_capture_device(dispositivo.clone()) {
            tracing::warn!(%erro, "não consegui trocar o microfone da sessão");
            sumiu = true;
        }
    }
    if sumiu {
        Err(FalhaAoEscolher::DispositivoSumiu)
    } else {
        Ok(())
    }
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

    // A saída é da máquina (E3): a escolha vai a todas as sessões, como o
    // microfone.
    let mut sumiu = false;
    for connection in session.sessoes.conexoes() {
        if let Err(erro) = connection.set_playback_device(dispositivo.clone()) {
            tracing::warn!(%erro, "não consegui trocar a saída de som da sessão");
            sumiu = true;
        }
    }
    if sumiu {
        Err(FalhaAoEscolher::DispositivoSumiu)
    } else {
        Ok(())
    }
}

/// Onde fica a lista de servidores visitados.
fn caminho_dos_conhecidos<R: tauri::Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    std::path::PathBuf::from(config_dir(app)).join("conhecidos")
}

/// Os servidores onde esta pessoa já esteve.
///
/// Corrupção tenta a cópia validada e, sem ela, retorna erro. A casca mantém
/// a entrada disponível, mas informa que a lista foi preservada para recuperação.
#[tauri::command]
fn conhecidos(app: AppHandle) -> Result<Vec<seele_ffi::conhecidos::Conhecido>, String> {
    seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
        .map(|lista| lista.listar())
        .map_err(|erro| erro.to_string())
}

/// Corrupção ou recuperação encontrada nesta execução, sem conteúdo privado.
#[tauri::command]
fn avisos_dos_dados_locais() -> Vec<String> {
    seele_ffi::arquivo_local::avisos()
}

/// Tira um servidor da lista.
///
/// Uma lista ilegível devolve erro: não se confirma a remoção de dados que
/// não puderam ser lidos, nem se sobrescreve o arquivo com uma lista vazia.
#[tauri::command]
fn esquecer(app: AppHandle, alvo: String) -> Result<(), String> {
    let mut lista = seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
        .map_err(|e| e.to_string())?;
    lista.esquecer(&alvo).map_err(|e| e.to_string())
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
        Falha::DominioDescontinuado => "DominioDescontinuado",
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
    /// **O ato foi aberto para outra hospedagem** (E7): o servidor parou e
    /// subiu outro (ou o mesmo, de novo) depois de a decisão abrir. Nada foi
    /// decidido; `agora` é a hospedagem de pé, para a tela redesenhar a fila.
    HospedagemNaoConfere {
        /// A hospedagem de pé agora.
        agora: HospedagemDaPorta,
    },
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
    /// **A hospedagem exata desta porta** (E7): o servidor e a geração. Os
    /// atos (`decidir_pedido`, `revogar_admissao`) a devolvem, e são recusados
    /// com `HospedagemNaoConfere` se ela não for mais a de pé. `None` sem
    /// hospedagem.
    hospedagem: Option<HospedagemDaPorta>,
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

/// O banco do servidor guardado **só se ele é a hospedagem pedida** (E7, a
/// portaria): o mesmo servidor e a mesma geração. A conferência e a cópia do
/// banco saem sob o mesmo cadeado das hospedagens, então o banco devolvido é o
/// da hospedagem conferida, mesmo que ela pare logo depois.
fn persistence_da_hospedagem(
    hospedagens: &Hospedagens,
    pedida: &HospedagemDaPorta,
) -> Result<seele_server::hospedagem::PersistenceCompartilhada, FalhaNaPortaria> {
    let guardado = hospedagens
        .guardado
        .as_ref()
        .ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
    let agora = guardado.identidade();
    if &agora != pedida {
        tracing::warn!(
            ?pedida,
            ?agora,
            "um ato da portaria aberto para outra hospedagem foi recusado"
        );
        return Err(FalhaNaPortaria::HospedagemNaoConfere { agora });
    }
    Ok(guardado.hospedagem.persistence())
}

/// O banco de um ato da portaria: o da hospedagem pedida, conferida
/// ([`persistence_da_hospedagem`]). **Sem hospedagem no pedido, recusado**
/// (rodada 3: a casca manda a do `estado_da_porta`): um ato que não diz de
/// qual hospedagem é não vai ao guardado de pé, e volta
/// `HospedagemNaoConfere` com a de agora, para a tela reler a fila.
fn persistence_do_ato(
    hospedagens: &Hospedagens,
    pedida: Option<&HospedagemDaPorta>,
) -> Result<seele_server::hospedagem::PersistenceCompartilhada, FalhaNaPortaria> {
    if let Some(pedida) = pedida {
        return persistence_da_hospedagem(hospedagens, pedida);
    }
    let guardado = hospedagens
        .guardado
        .as_ref()
        .ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
    tracing::warn!("um ato da portaria chegou sem a hospedagem e foi recusado");
    Err(FalhaNaPortaria::HospedagemNaoConfere {
        agora: guardado.identidade(),
    })
}

/// **Parar A e subir B nunca aplica a B uma decisão aberta para A** (E7, a
/// portaria), com hospedagens de verdade: a mesma chave e o mesmo servidor
/// guardado (`None`, o de sempre) não bastam, porque a geração muda.
#[path = "testes/a_decisao_e_da_hospedagem_em_que_abriu.rs"]
#[cfg(test)]
mod a_decisao_e_da_hospedagem_em_que_abriu;

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

#[path = "testes/o_estado_do_servidor.rs"]
#[cfg(test)]
mod o_estado_do_servidor;

/// O estado das três camadas da porta.
#[tauri::command]
async fn estado_da_porta(session: State<'_, Session>) -> Result<EstadoDaPorta, FalhaNaPortaria> {
    let (persistence, alcance, hospedagem) = {
        let hospedagens = session
            .hospedagens
            .lock()
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
        let hospedagem = hospedagens.guardado.as_ref().map(Hospedado::identidade);
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
                hospedagem: None,
            });
        };
        let alcance = server
            .alcance()
            .map_or("SoRedeLocal", |alcance| alcance.degrau().nome());
        (server.persistence(), alcance, hospedagem)
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
        hospedagem,
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

/// O link do navegador de um convite deste servidor, para a tela de quem
/// hospeda (D-web-6).
#[derive(Debug, serde::Serialize)]
struct LinkDoNavegadorNaTela {
    /// O link na origem do navegador (`seele_proto::uri::ORIGEM_DO_NAVEGADOR`).
    link: String,
    /// Quem de fora entra por ele. **Desde a 0.17 o link não vence** (o
    /// WebRTC confere o certificado longo pela impressão, sem certificado
    /// curto): não há «vale até» nem «renova». (`AlcanceDoNavegador`): `Alcanca`,
    /// `SoPeloEncontro`, `SoNaRede` ou `NaoSeSabe`. Nome, e não frase: a frase
    /// mora no `LINK_DO_NAVEGADOR` do JavaScript.
    alcance: &'static str,
    /// A porta UDP deste servidor: é a que o aviso manda abrir no roteador.
    porta: u16,
}

/// Por que não há link do navegador para mostrar.
///
/// Enum e não frase, como as vizinhas: a fronteira erro→texto é do frontend.
#[derive(Debug, serde::Serialize)]
enum FalhaNoLinkDoNavegador {
    /// Esta janela não está hospedando nada.
    NaoEstaHospedando,
    /// O texto não é um convite deste servidor.
    ConviteInvalido,
    /// O convite é de outro servidor.
    OutroServidor,
    /// O que está no ar é a sala pessoal: ela é achada pelo código, e não por
    /// um link do navegador.
    SalaPessoal,
}

/// **O link do navegador do convite que a tela mostra** (D-web-6): o mesmo
/// convite, para quem entra pelo web app, com os hashes dos certificados
/// curtos que este servidor apresenta de agora em diante.
///
/// A conta inteira é do `Hospedagem::link_do_navegador`, e a bancada da sessão
/// no navegador pega o link por ele também (`servidor_da_bancada`): o link que
/// ela prova abrir é o que esta tela mostra.
#[tauri::command]
fn link_do_navegador(
    session: State<'_, Session>,
    convite: String,
) -> Result<LinkDoNavegadorNaTela, FalhaNoLinkDoNavegador> {
    use seele_server::hospedagem::SemLinkDoNavegador;

    let hospedagens = session
        .hospedagens
        .lock()
        .map_err(|_| FalhaNoLinkDoNavegador::NaoEstaHospedando)?;
    let server = hospedagens
        .do_guardado()
        .ok_or(FalhaNoLinkDoNavegador::NaoEstaHospedando)?;
    let link = server
        .link_do_navegador(&convite)
        .map_err(|sem| match sem {
            SemLinkDoNavegador::ConviteInvalido => FalhaNoLinkDoNavegador::ConviteInvalido,
            SemLinkDoNavegador::OutroServidor => FalhaNoLinkDoNavegador::OutroServidor,
            SemLinkDoNavegador::SalaPessoal => FalhaNoLinkDoNavegador::SalaPessoal,
        })?;
    Ok(LinkDoNavegadorNaTela {
        link: link.link,
        alcance: link.alcance.nome(),
        porta: server.endereco().port(),
    })
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

/// Quem hospeda decide sobre um pedido, **da hospedagem em que a decisão
/// abriu** (`hospedagem`, a do `estado_da_porta`; E7): outra hospedagem de pé
/// recusa com `HospedagemNaoConfere`, e nada é decidido.
#[tauri::command]
async fn decidir_pedido(
    session: State<'_, Session>,
    impressao: String,
    admitir: bool,
    hospedagem: Option<HospedagemDaPorta>,
) -> Result<(), FalhaNaPortaria> {
    let persistence = {
        let hospedagens = session
            .hospedagens
            .lock()
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
        persistence_do_ato(&hospedagens, hospedagem.as_ref())?
    };
    let mut persistence = persistence.lock().await;
    seele_server::portaria::decidir(&mut persistence, &impressao, admitir)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// Desfaz uma decisão: a pessoa volta a ser desconhecida. Da hospedagem em
/// que o ato abriu, como o [`decidir_pedido`].
///
/// Não é banir e não derruba quem está dentro — ADR 0030. A frase que a tela
/// mostra antes de fazer isto é que precisa dizer as duas coisas.
#[tauri::command]
async fn revogar_admissao(
    session: State<'_, Session>,
    impressao: String,
    hospedagem: Option<HospedagemDaPorta>,
) -> Result<(), FalhaNaPortaria> {
    let persistence = {
        let hospedagens = session
            .hospedagens
            .lock()
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
        persistence_do_ato(&hospedagens, hospedagem.as_ref())?
    };
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
fn buscar(
    session: State<'_, Session>,
    carimbo: sessoes::CarimboDaJanela,
    termo: String,
) -> Result<BuscaEstado, ConnectionError> {
    // `messages()` e não `snapshot()`: a conversa saiu do snapshot, que agora
    // carrega só a revisão dela. Aqui a lista inteira é mesmo necessária — é o
    // que se busca —, e este comando roda quando alguém digita, não a cada
    // quadro.
    let mensagens = session.conexao(carimbo)?.messages();
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
    /// Se a versão oferecida fala outra linha de servidor (E4 da 0.17). Com
    /// `pede_aviso`, a tela mostra o aviso antes de instalar, e
    /// `instalar_atualizacao` só aceita com a confirmação desta `versao`.
    fronteira: atualizador::Fronteira,
    /// Atalho de `fronteira`, para a tela não reimplementar a regra.
    pede_aviso: bool,
    /// Esta versão já foi adiada uma vez nesta máquina.
    adiada: bool,
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
    /// A versão escolhida fala outra linha de servidor, e a tela não mandou a
    /// confirmação **desta** versão (E4 da 0.17, `atualizador::pode_instalar`).
    ///
    /// Recusado antes de baixar um byte, e a escolha volta ao lugar: a tela
    /// mostra o aviso e pede de novo, sem procurar outra vez.
    TravessiaNaoConfirmada,
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

    let fronteira = fronteira_da_oferta(&nova);
    let resposta = VersaoNova {
        versao: nova.version.clone(),
        instalada: nova.current_version.clone(),
        notas: nova.body.clone(),
        pede_aviso: fronteira.pede_aviso(),
        fronteira,
        adiada: atualizador::foi_adiada(std::path::Path::new(&config_dir(&app)), &nova.version),
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
///
/// # A travessia de linha (E4 da 0.17)
///
/// `confirmada` é a versão que a tela mostrou no aviso de travessia, e só
/// importa quando a oferta fala outra linha de servidor: aí, sem ela — ou com
/// a de outra oferta —, a resposta é [`FalhaAoAtualizar::TravessiaNaoConfirmada`]
/// e a escolha **fica** no lugar, para a tela pedir de novo. Na mesma linha,
/// `confirmada` é ignorada, e a tela de antes (sem argumento) continua valendo.
#[tauri::command]
async fn instalar_atualizacao(
    app: AppHandle,
    session: State<'_, Session>,
    confirmada: Option<String>,
) -> Result<(), FalhaAoAtualizar> {
    atualizacao_permitida(&session.abertura)?;
    // Tirado do lugar, e não emprestado: instalar é uma vez. Se falhar, quem
    // quiser tentar de novo procura de novo — e a consulta nova é justamente o
    // que confirma que a versão ainda é aquela. A exceção é a travessia não
    // confirmada, que devolve a escolha antes de baixar qualquer coisa.
    let nova = {
        let mut slot = session
            .atualizacao
            .lock()
            .map_err(|_| FalhaAoAtualizar::NadaEscolhido)?;
        let escolhida = slot.as_ref().ok_or(FalhaAoAtualizar::NadaEscolhido)?;
        atualizador::pode_instalar(
            &fronteira_da_oferta(escolhida),
            &escolhida.version,
            atualizador::Escolha::Prosseguir,
            confirmada.as_deref(),
        )
        .map_err(|_| FalhaAoAtualizar::TravessiaNaoConfirmada)?;
        slot.take().ok_or(FalhaAoAtualizar::NadaEscolhido)?
    };
    atualizador::esquecer_o_adiamento(std::path::Path::new(&config_dir(&app)));

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

/// A fronteira de uma oferta do atualizador: a linha deste app contra a que o
/// manifesto que veio com a oferta atribui a ela (E4 da 0.17).
fn fronteira_da_oferta(nova: &tauri_plugin_updater::Update) -> atualizador::Fronteira {
    atualizador::fronteira_da_oferta(
        versoes::linha_deste_app().as_ref(),
        &nova.raw_json,
        &nova.version,
    )
}

/// Cancelar ou adiar a oferta que a tela mostrou (E4 da 0.17).
///
/// As duas tiram a oferta do lugar: nada é instalado, e um «instalar» seguinte
/// responde `NadaEscolhido`. Adiar guarda a versão, e a próxima procura a
/// devolve com `adiada`; cancelar esquece. `Prosseguir` aqui é o mesmo que
/// cancelar: quem prossegue chama `instalar_atualizacao`.
#[tauri::command]
fn escolher_atualizacao(
    app: AppHandle,
    session: State<'_, Session>,
    escolha: atualizador::Escolha,
) -> Result<(), FalhaAoAtualizar> {
    let nova = session
        .atualizacao
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .ok_or(FalhaAoAtualizar::NadaEscolhido)?;
    let escolha = match escolha {
        atualizador::Escolha::Prosseguir => atualizador::Escolha::Cancelar,
        outra => outra,
    };
    if let Err(erro) = atualizador::registrar_a_escolha(
        std::path::Path::new(&config_dir(&app)),
        &nova.version,
        escolha,
    ) {
        tracing::warn!(%erro, versao = %nova.version, "não guardei o adiamento da atualização");
    }
    tracing::info!(versao = %nova.version, ?escolha, "a atualização ficou para depois");
    Ok(())
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
    // A validação pode usar o mesmo app final com WebView sem persistência.
    // A pasta nativa também precisa ser explícita e absoluta; recusar antes
    // de abrir o log evita tocar o perfil padrão numa invocação incompleta.
    let webview_descartavel = std::env::var("SEELE_WEBVIEW_DESCARTAVEL").as_deref() == Ok("1");
    if webview_descartavel
        && !std::env::var("SEELE_HOME")
            .is_ok_and(|pasta| std::path::Path::new(&pasta).is_absolute())
    {
        eprintln!("SEELE_WEBVIEW_DESCARTAVEL=1 exige SEELE_HOME explícito e absoluto.");
        std::process::exit(2);
    }

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
         seele_encontro=info,seele_lancador=info,seele_motor=info,seele_enlace=info,\
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

    // **Este processo tem tela** (API 8 parte 3): toda conexão dele mostra
    // pedidos de máquina, em vez de o núcleo os recusar sozinho com
    // `sem-tela`. Aqui, uma vez, antes de qualquer conexão — inclusive a
    // automática do arranque —, porque o servidor reenvia os pedidos pendentes
    // logo depois do `Session`, antes de um comando por conexão poder chegar.
    // **Nunca num ajudante que os testes usam**: o valor é do processo, e um
    // binário de teste que o ligasse mudaria o padrão de todos os testes dele.
    seele_ffi::este_processo_tem_tela(true);

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
        // A notificação do sistema. Registrado para que o comando desta casca
        // possa chamá-lo daqui de dentro; a página **não** o alcança — a única
        // permissão dele em `capabilities/janela.json` é
        // `allow-is-permission-granted`, e ela existe por causa do script que o
        // plugin injeta em toda página, que faz essa pergunta ao carregar sem
        // `catch`: recusada, virava uma «promise recusada sem catch» no
        // registro de toda abertura. `notify` continua fechado
        // (`tests/permissoes.rs`).
        .plugin(tauri_plugin_notification::init())
        // A tecla de falar fora da janela. O plugin dela **não** sobe aqui: ver
        // `tecla_global::iniciar`, chamado do `setup`.
        .manage(tecla_global::TeclaGlobal::default())
        // A imagem do perfil que espera o passo de recorte (`recorte.rs`).
        .manage(recorte::ImagensARecortar::default())
        // A janela vir à frente devolve a tecla ao sistema, e sair da frente a
        // pede; fechada, devolve. Só avisa: este fecho roda na thread principal.
        .on_window_event(|janela, evento| {
            if matches!(
                evento,
                tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Destroyed
            ) {
                tecla_global::avisar(janela.app_handle());
            }
        })
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
            // O relatório do áudio de mentira (só a build de teste do CUA, e
            // só com a variável); fora dela não faz nada.
            audio_de_mentira::relatar(app.handle().clone());
            let primeira = versoes::primeira_abertura(&config_dir(app.handle()));
            let houve_0_15 = primeira.houve_0_15();
            // **O padrão do microfone desta instalação, assentado antes de a
            // janela existir.** Quem chega fala sem tecla; quem já usava e
            // nunca escolheu continua nela (pedido do dono, 13/11/2026). O que
            // separa os dois é a chave e o arquivo de preferências, e é a
            // janela que cria a chave (o código da pessoa, o `connect`):
            // perguntado depois dela, toda instalação nova pareceria usada. Ver
            // `seele_ffi::assentar_o_padrao_do_microfone` e o guarda
            // `o_padrao_do_microfone_e_assentado_antes_de_a_janela_existir`.
            assentar_o_padrao_do_microfone(app.handle());
            app.manage(versoes::EstadoDaPrimeiraAbertura(std::sync::Mutex::new(
                Some(versoes::PrimeiraAbertura {
                    dados: primeira,
                    linha_legada: None,
                }),
            )));
            for janela in app.config().app.windows.clone() {
                let janela = tauri::WebviewWindowBuilder::from_config(app.handle(), &janela)?;
                let janela = if webview_descartavel {
                    janela.incognito(true)
                } else {
                    janela
                };
                janela.build()?;
            }

            // A tecla de falar fora da janela: a opção e a tecla que estão no
            // disco, o serviço de atalhos e a tarefa que confere. Depois da
            // janela, que é de quem a conferida lê o foco.
            let ajustes = preferencias(app.handle());
            tecla_global::iniciar(
                app.handle(),
                ajustes
                    .as_ref()
                    .is_some_and(seele_ffi::preferences::Preferences::tecla_de_falar_global),
                ajustes.as_ref().and_then(|p| p.push_to_talk_key()),
            );

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
            // **O jogo que um app anterior deixou rodando** (e2e de 31/10,
            // passo 14): achado pelo registro, parado, e dito na faixa.
            maquina::procurar_sobras(app.handle().clone());
            // **E um `SIGTERM` de fora passa pela mesma saída** que fechar a
            // janela: sem isto, o `java` do jogo ficava órfão.
            #[cfg(unix)]
            sair_despedindo_nos_sinais(app.handle().clone());
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
            abrir_o_teste_de_rede,
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
            salvar_do_mod,
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
            resolver_mod_de_api_antiga,
            desfazer_escolha_de_mod,
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
            amigos,
            adicionar_amigo,
            tirar_dos_amigos,
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
            antes_de_hospedar,
            preparar_copia,
            encerrar_sessao,
            sessoes,
            ver_sessao,
            instancia_da_janela,
            foco_de_voz,
            focar_voz,
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
            perfis,
            gravar_perfil,
            tirar_imagem_do_perfil,
            enviar_imagem_do_perfil,
            imagem_do_perfil,
            perfil_padrao,
            gravar_perfil_padrao,
            escolher_imagem_para_recortar,
            esquecer_imagem_a_recortar,
            guardar_imagem_do_perfil_padrao,
            tirar_imagem_do_perfil_padrao,
            seguimento_do_perfil_padrao,
            guardar_seguimento_do_perfil_padrao,
            enviar_imagem_do_perfil_padrao,
            ler_relatorio_da_migracao_de_perfis,
            dispensar_relatorio,
            escolher_apelido,
            apelido_local,
            escolher_apelido_local,
            tirar_minha_imagem,
            imagem_da_pessoa,
            meu_retrato,
            tirar_icone_do_server,
            mods_instalados,
            mod_request,
            tunel_entrar,
            maquina_aceitar,
            maquina_recusar,
            maquina_desligar,
            maquina_revogar,
            maquinas_aceitas,
            maquinas_sobras,
            tunel_sair_do_mod,
            tuneis_abertos,
            aceite_de_mods,
            aceitar_mods,
            esquecer_aceite_de_mods,
            icone_do_server,
            expulsar_pessoa,
            pedir_cargos,
            explicar_permissoes,
            prever_cargo,
            criar_cargo,
            editar_cargo,
            mover_cargo,
            apagar_cargo,
            atribuir_cargo,
            revogar_cargo,
            banir_pessoa,
            desbanir_pessoa,
            listar_banidos,
            banidos,
            editar_mensagem,
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
            avisos_dos_dados_locais,
            fontes_de_tela,
            previas_de_tela,
            permissao_de_tela,
            pedir_permissao_de_tela,
            abrir_ajustes_da_gravacao_de_tela,
            redefinir_permissao_de_tela,
            reabrir_o_app,
            compartilhar_tela,
            codificacao_da_camera,
            compartilhar_camera,
            quadro_da_camera,
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
            estado_da_tecla_global,
            ligar_tecla_global,
            estado_das_notificacoes,
            ligar_notificacoes,
            notificar_no_sistema,
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
            escolher_atualizacao,
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
            regras_da_senha_do_server,
            criar_convite_do_server,
            link_do_navegador,
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
                // O `RunEvent::Exit` que vem depois desta saída não despede de
                // novo (`despedir_no_fim`); e uma despedida que já corre (a de
                // um sinal) não roda outra vez (revisão da 0.19, M6).
                api.prevent_exit();
                ao_fechar_a_ultima_janela(&handle.state::<Session>().despedida_feita, || {
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
                    // **As portas de jogo fecham aqui**: `despedir_se` encerra cada
                    // sessão (`encerrar_e_esperar`), e o enlace derruba os túneis de
                    // quem joga ao sair — a porta em `127.0.0.1` não sobrevive ao
                    // app. Provado em `tunel_pela_ffi.rs` (conformidade),
                    // `encerrar_a_sessao_fecha_a_porta`: a mesma chamada, com a porta
                    // aberta de verdade.
                    despedir_se(handle);
                });
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

/// **A saída de quem recebeu um sinal** (`SIGTERM`, `SIGINT`, `SIGHUP`; e2e
/// de 31/10, passo 14): a mesma despedida de fechar a última janela — as
/// metades de máquina param o jogo (`aoParar("app-fechou")`, que salva), a
/// sessão e as hospedagens se despedem — e só então o app sai.
///
/// Antes o sinal matava o processo na hora, e o `java` do jogo, num grupo
/// próprio (decisão R8), seguia órfão com o mundo e a porta. Um segundo sinal
/// durante a despedida sai na hora: quem manda dois quer sair, e o registro do
/// jogo ([`maquina::procurar_sobras`]) cobre o que ficar.
#[cfg(unix)]
fn sair_despedindo_nos_sinais(handle: tauri::AppHandle) {
    use tokio::signal::unix::{signal, SignalKind};
    tauri::async_runtime::spawn(async move {
        let sinais = [
            (SignalKind::terminate(), "SIGTERM"),
            (SignalKind::interrupt(), "SIGINT"),
            (SignalKind::hangup(), "SIGHUP"),
        ];
        let (avisa, mut chegou) = tokio::sync::mpsc::unbounded_channel::<&'static str>();
        for (tipo, nome) in sinais {
            match signal(tipo) {
                Ok(mut ouvinte) => {
                    let avisa = avisa.clone();
                    tauri::async_runtime::spawn(async move {
                        while ouvinte.recv().await.is_some() {
                            if avisa.send(nome).is_err() {
                                break;
                            }
                        }
                    });
                }
                Err(erro) => {
                    tracing::warn!(sinal = nome, %erro, "não consegui ouvir o sinal: ele mata o app sem despedida, e o jogo fica para a próxima abertura parar");
                }
            }
        }
        drop(avisa);
        let Some(nome) = chegou.recv().await else {
            return;
        };
        use tauri::Manager as _;
        let estado = handle.state::<Session>();
        let da_sessao = handle.clone();
        despedir_no_sinal(
            &estado.despedida_feita,
            nome,
            async {
                let sessao = tauri::async_runtime::spawn_blocking(move || despedir_se(&da_sessao));
                let hospedagens =
                    despedir_das_hospedagens(&estado, TETO_DA_DESPEDIDA_DAS_HOSPEDAGENS);
                let _ = tokio::join!(sessao, hospedagens);
            },
            chegou.recv(),
            || handle.exit(0),
        )
        .await;
    });
}

/// **A última janela fechou** (`ExitRequested { code: None }`, com a saída já
/// segurada): a despedida só começa se nenhuma outra já corre (revisão da
/// 0.19, M6). Um sinal que chegou antes já despede e já chama o `exit`;
/// despedir de novo dobrava a despedida das hospedagens e a espera da saída.
fn ao_fechar_a_ultima_janela(feita: &std::sync::atomic::AtomicBool, comecar: impl FnOnce()) {
    if feita.swap(true, std::sync::atomic::Ordering::SeqCst) {
        tracing::info!("a última janela fechou com a despedida já correndo: o exit dela vem");
        return;
    }
    comecar();
}

/// **A saída por um sinal**, sem o runtime: se outra despedida já corre (a
/// janela fechou antes), **não sai** — o `exit` daquela vem quando ela acaba, e
/// sair aqui cortava a janela de 1 s das hospedagens (revisão da 0.19, M1).
/// Senão despede, e um segundo sinal no meio sai na hora.
#[cfg(unix)]
async fn despedir_no_sinal<D, S>(
    feita: &std::sync::atomic::AtomicBool,
    nome: &'static str,
    despedir: D,
    segundo_sinal: S,
    sair: impl FnOnce(),
) where
    D: std::future::Future<Output = ()>,
    S: std::future::Future<Output = Option<&'static str>>,
{
    if feita.swap(true, std::sync::atomic::Ordering::SeqCst) {
        tracing::info!(
            sinal = nome,
            "sinal durante a saída: a despedida já corria, e o exit dela vem"
        );
        return;
    }
    tracing::info!(
        sinal = nome,
        "sinal recebido: o app se despede como ao fechar a janela, e sai"
    );
    tokio::select! {
        () = despedir => {}
        segundo = segundo_sinal => {
            tracing::warn!(sinal = ?segundo, "segundo sinal durante a despedida: o app sai sem terminá-la");
        }
    }
    sair();
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

    // **As metades de máquina primeiro**, todas ao mesmo tempo, com
    // `aoParar("app-fechou")` e o `MaquinaParada { AppFechou }`: depois da linha
    // de baixo a conexão já se despediu, e o servidor saberia só pela queda.
    maquina::parar_todas_na_saida(&handle.state::<Session>().maquinas);
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

    // **Todas as sessões se despedem, e ao mesmo tempo** (E3 da 0.17): três
    // servidores, três despedidas, dentro dos mesmos 500 ms. Em fila, a
    // terceira passaria do teto da saída, e quem estava lá a veria pelo prazo
    // de ociosidade.
    let mut conexoes = sessao.sessoes.conexoes();
    if let Ok(da_vista) = sessao.connection() {
        if !conexoes
            .iter()
            .any(|conexao| Arc::ptr_eq(conexao, &da_vista))
        {
            conexoes.push(da_vista);
        }
    }
    let despedidas: Vec<_> = conexoes
        .into_iter()
        .map(|connection| {
            std::thread::Builder::new()
                .name("despedida de uma sessão".into())
                .spawn(move || connection.encerrar_e_esperar(std::time::Duration::from_millis(500)))
        })
        .collect();
    for despedida in despedidas {
        let terminou = match despedida {
            Ok(thread) => thread.join().unwrap_or(false),
            Err(erro) => {
                tracing::warn!(%erro, "a despedida de uma sessão não conseguiu thread");
                false
            }
        };
        if !terminou {
            tracing::warn!(
                "a despedida não terminou em 500 ms: quem ficou vai notar a saída pelo prazo de \
                 ociosidade, 20 s"
            );
        }
    }
}

/// **Uma despedida por saída** (revisão da 0.19, M1 e M6): o sinal que chega
/// com a despedida da janela correndo não sai por cima dela, e a janela que
/// fecha com a despedida de um sinal correndo não despede de novo.
#[path = "testes/uma_despedida_por_saida.rs"]
#[cfg(test)]
mod uma_despedida_por_saida;

/// A despedida de quem sai sem fechar a janela (tarefa 21 da F7): o Sair do
/// Dock, o logout, um `quit` de fora. O que se prova aqui é a forma, com
/// metades de mentira; se o runtime ainda a entrega no `RunEvent::Exit` é
/// medida no app de verdade.
#[path = "testes/a_despedida_do_fim.rs"]
#[cfg(test)]
mod a_despedida_do_fim;

#[path = "testes/a_tela_le_o_que_o_rust_manda.rs"]
#[cfg(test)]
mod a_tela_le_o_que_o_rust_manda;

#[path = "testes/a_tela_le_os_limites_que_o_rust_manda.rs"]
#[cfg(test)]
mod a_tela_le_os_limites_que_o_rust_manda;

/// O que sai desta janela por um link, e o que não sai.
///
/// Chamava-se `o_caminho_do_mod_chega_inteiro` enquanto guardava também os
/// testes de `por_cento_desfeito`. O ADR 0049 tirou o `mod://` do produto e
/// aqueles testes saíram com ele; o nome ficou descrevendo o que não está mais
/// aqui, e um módulo cujo nome mente é onde o próximo teste entra no lugar
/// errado.
#[path = "testes/o_que_sai_desta_janela_por_um_link.rs"]
#[cfg(test)]
mod o_que_sai_desta_janela_por_um_link;

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
#[path = "testes/a_supervisao_dos_mods_nativos.rs"]
#[cfg(test)]
mod a_supervisao_dos_mods_nativos;

#[path = "testes/a_janela_sabe_de_onde_veio_a_impressao.rs"]
#[cfg(test)]
mod a_janela_sabe_de_onde_veio_a_impressao;

/// **O relógio fixo dos testes do registro.**
///
/// O teto de vazão ([`linha_admitida`]) recebe o instante de quem chama, e um
/// teste que lesse o relógio da máquina a cada chamada mediria o balde com a
/// recarga que o tempo da própria execução trouxe. Aqui o relógio é lido uma
/// vez no processo: todo instante de um teste é esta origem mais uma duração
/// escrita nele.
#[path = "testes/relogio_de_teste.rs"]
#[cfg(test)]
mod relogio_de_teste;

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
#[path = "testes/gancho_de_panico.rs"]
#[cfg(test)]
mod gancho_de_panico;

/// **O `console` de um MOD chega ao `seele.log`, com o id e o nível.**
///
/// Pelo caminho real: a mesma `assentar_fala` que a bomba chama para cada fala
/// do executor, chamada na thread do teste para o rastro ser capturável.
#[path = "testes/o_console_do_mod_chega_ao_registro.rs"]
#[cfg(test)]
mod o_console_do_mod_chega_ao_registro;

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
#[path = "testes/a_falha_do_mod_chega_ao_registro.rs"]
#[cfg(test)]
mod a_falha_do_mod_chega_ao_registro;

/// **Um MOD que não carregou é dito no `seele.log`, uma vez por motivo.**
///
/// `codigo_do_mod` e `mod_nativo_reservar` recusavam sem uma linha: a janela
/// anotava «não carregou» sem o motivo, num mapa em memória, e tentava de novo
/// a cada quatro segundos. Dita a cada tentativa, a recusa seria novecentas
/// linhas iguais por hora; dita quando muda, é uma.
#[path = "testes/a_carga_recusada_e_dita_uma_vez_por_motivo.rs"]
#[cfg(test)]
mod a_carga_recusada_e_dita_uma_vez_por_motivo;

/// **O que a janela escreve no registro, com o id do MOD em campo próprio.**
///
/// O comando é chamado direto: é uma função como outra qualquer, e o que se
/// mede é a linha que ela deixa no `seele.log`.
#[path = "testes/o_registro_da_janela.rs"]
#[cfg(test)]
mod o_registro_da_janela;

/// **O teto de vazão de cada MOD no `seele.log`**, pelas portas que não são
/// o `console`.
///
/// Com o relógio fixo ([`relogio_de_teste`]): cada instante é a origem mais
/// uma duração, e o balde é medido sem dormir.
#[path = "testes/o_teto_de_vazao_do_registro.rs"]
#[cfg(test)]
mod o_teto_de_vazao_do_registro;

/// **O id de um MOD que chega da janela sai escapado em toda linha do
/// registro**, e não só nas recusas.
///
/// Um campo em `%` sai cru no `seele.log`: o formatador escapa só a mensagem,
/// e só ANSI. As duas linhas de `mod_request` e as de `midia_em_bytes` e de
/// `escolher_para_o_mod` escreviam o id que o comando recebe da janela como
/// veio — e um `\n` nele escrevia uma segunda linha com a cara do produto.
#[path = "testes/o_id_do_pedido_de_mod_no_registro.rs"]
#[cfg(test)]
mod o_id_do_pedido_de_mod_no_registro;

/// **Um pacote de MOD publicado numa pasta de teste**, como o instalador o
/// deixa.
///
/// Escrito numa pasta de obras, lido para saber o hash do conteúdo, e movido
/// para `PACOTES/<hash>` — que é onde `ler_por_hash` e `mods::serve` o
/// procuram. Os bytes de cada formato são os menores que o `sniff` reconhece.
#[path = "testes/pacote_de_teste.rs"]
#[cfg(test)]
mod pacote_de_teste;

/// **A mídia de MOD recusada pelo Rust é dita no `seele.log`.**
///
/// Em 23/09, descobrir que o avatar do PERFIS carregava levou uma hora de
/// medição e um reinício com `RUST_LOG=debug`: o Rust recusava e devolvia a
/// recusa à janela, e o registro não tinha uma palavra.
#[path = "testes/a_midia_recusada_e_dita_no_registro.rs"]
#[cfg(test)]
mod a_midia_recusada_e_dita_no_registro;

/// **O som de um MOD sai em bytes, conferido como a imagem é conferida.**
#[path = "testes/o_som_do_mod_sai_em_bytes.rs"]
#[cfg(test)]
mod o_som_do_mod_sai_em_bytes;

/// **O som grande demais decodificado é recusado antes de a janela o
/// decodificar** (P49-7a).
///
/// `decodeAudioData` devolve o som inteiro em float32, de uma vez: a janela só
/// o conta depois de ele existir, e o pico já aconteceu. O Rust lê um piso do
/// cabeçalho e recusa antes de servir. Um teste por braço — WAV, Ogg, MP3 —,
/// cada um com o caso grande que só ele recusa, com bytes montados aqui; e as
/// duas portas do servidor, que entregam o som pelo mesmo `data:`.
#[path = "testes/o_som_grande_demais_decodificado_e_recusado_antes.rs"]
#[cfg(test)]
mod o_som_grande_demais_decodificado_e_recusado_antes;

#[path = "testes/a_abertura_pelo_launcher.rs"]
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod a_abertura_pelo_launcher;

/// O tipo de um arquivo escolhido para um MOD, ou a recusa com nome.
#[path = "testes/o_tipo_do_arquivo_escolhido.rs"]
#[cfg(test)]
mod o_tipo_do_arquivo_escolhido;

#[path = "testes/parar_de_hospedar_e_so_o_guardado.rs"]
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod parar_de_hospedar_e_so_o_guardado;

#[path = "testes/a_saida_despede_as_hospedagens.rs"]
#[cfg(test)]
mod a_saida_despede_as_hospedagens;

#[path = "testes/a_resposta_a_estou_no_meu_servidor.rs"]
#[cfg(test)]
mod a_resposta_a_estou_no_meu_servidor;

/// **A P7b: a API que o manifesto declara chega à coluna do banco** por quem a
/// grava a partir da tela.
///
/// O código entrou com a P7a (`4112f96b`), e o teste da tela ficou esperando a
/// fila do `main.rs`. Ele vai do pacote em disco, pelo mesmo `ler_por_hash` que
/// os dois comandos usam, até a linha que o banco devolve; o guarda de fonte
/// prende os dois comandos na mesma construção, que é o que sobra entre a tela e
/// este teste.
#[path = "testes/a_api_do_mod_habilitado.rs"]
#[cfg(test)]
mod a_api_do_mod_habilitado;

/// **O comando `tunel_entrar` confere o que é do Rust** (API 8, parte 2): a
/// geração, e que o MOD está de pé nela com o poder `porta` no manifesto que o
/// pacote provou. O bilhete de gesto é da janela (`bancada/tunel-de-jogador.cjs`);
/// quem confere o servidor, a ação e a máquina é o servidor (conformidade).
///
/// A recusa é sempre um nome da lista fechada, nunca a variante de um erro de
/// conexão: é o que o MOD e a faixa leem.
#[path = "testes/o_tunel_de_jogo_pelo_app.rs"]
#[cfg(test)]
mod o_tunel_de_jogo_pelo_app;

#[path = "testes/imagem_de_perfil_para_subir_testes.rs"]
#[cfg(test)]
mod imagem_de_perfil_para_subir_testes;
