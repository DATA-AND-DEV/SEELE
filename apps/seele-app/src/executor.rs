//! O executor de MODs do **cliente**: QuickJS, num runtime por instância.
//!
//! Protótipo da etapa E1, e a diretriz de 18/09 diz por que ele existe: o
//! Worker de `blob:` não satisfaz o contrato. A sonda de fronteira releu, num
//! aplicativo reiniciado, uma marca que um MOD tinha gravado em IndexedDB — e
//! armazenamento que sobrevive à sessão é o contrário da promessa deste
//! produto.
//!
//! # O que muda de lugar, e o que não muda
//!
//! **Não muda:** a WebView é a mesma, o renderer confiável é o mesmo, e a API é
//! a mesma. Não há uma WebView por MOD, e não há um segundo motor de navegador
//! no instalador.
//!
//! **Muda:** a lógica do autor deixa de rodar num contexto de navegador. Não há
//! `indexedDB` porque não há origem; não há `BroadcastChannel` porque não há
//! outros contextos da mesma origem; não há `fetch` porque ninguém o ligou. O
//! que existe é o que este arquivo põe lá dentro, uma função por vez.
//!
//! # O que este arquivo deliberadamente não faz
//!
//! **Não importa os poderes da metade de servidor.** O `seele-server` tem
//! bindings de disco, de rede e de persistência, e eles existem porque um MOD
//! de servidor roda na máquina de quem hospeda, com o consentimento que aquela
//! tela pede. Trazê-los para cá por conveniência seria dar à metade de janela
//! uma autoridade que ninguém autorizou. A diretriz é explícita: «reaproveitar
//! a experiência com limites, não os poderes da metade de servidor».
//!
//! # O que ele ainda não é
//!
//! Um protótipo. Ele responde à pergunta da E1 — **que autoridade o código de
//! um MOD alcança?** — e não é o executor aprovado: falta medir custo, latência
//! de saída e o impacto na voz, e falta repetir a matriz de corridas da E2 com
//! ele. Enquanto isso, `ui/mods-runtime.js` continua sendo quem decide qual
//! executor sobe, e o de hoje continua sendo o Worker.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rquickjs::{Context, Function, Runtime};

/// Quanta memória o motor pode alocar para um MOD.
///
/// **Não é o orçamento do MOD**, e a diretriz cobra a distinção: «o teto de
/// heap do QuickJS não limita a memória total do MOD». Buffers nativos, filas e
/// recursos visuais são contados noutros lugares e têm tetos próprios.
///
/// Oito mebibytes é o mesmo número que a metade de servidor usa. Ele está aqui
/// como ponto de partida a medir, e não como decisão: a carga de uma interface
/// não é a de um handler de pedido.
pub(crate) const TETO_DE_MEMORIA: usize = 8 * 1024 * 1024;

/// Quantas consultas do motor cabem numa volta de execução.
///
/// **Isto não é um prazo**, e é a armadilha que a diretriz nomeia: «só contar
/// consultas do motor não estabelece prazo real de saída». Uma função nativa
/// que bloqueia não consulta o motor, e este contador nunca a vê. Ele é o teto
/// de trabalho; o prazo é o campo ao lado dele em [`Interrupcao`].
pub(crate) const CONSULTAS_POR_VOLTA: usize = 200_000;

/// Os três tetos de uma volta de execução.
///
/// **Separados para poderem ser medidos separadamente**, e a diretriz cobra
/// isso: «limites distintos para heap do motor, buffers/filas nativos e
/// recursos visuais», e «só contar consultas do motor não estabelece prazo real
/// de saída».
///
/// A separação também é o que torna os testes honestos. Com um número só, um
/// laço infinito para — e não dá para saber **qual** mecanismo o parou. Foi o
/// que aconteceu na primeira versão destes testes: dois deles passavam com a
/// revogação arrancada, porque o teto de trabalho os salvava. Medindo um
/// mecanismo por vez, cada teste afrouxa os outros dois e só o seu pode
/// explicar a parada.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limites {
    /// Quanto o motor pode alocar. Não é o orçamento do MOD.
    pub(crate) memoria: usize,
    /// Quantas consultas do motor cabem numa volta.
    pub(crate) consultas_por_volta: usize,
    /// Quanto tempo uma volta pode levar.
    pub(crate) prazo_por_volta: Duration,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            memoria: TETO_DE_MEMORIA,
            consultas_por_volta: CONSULTAS_POR_VOLTA,
            prazo_por_volta: Duration::from_millis(500),
        }
    }
}

/// O que faz o motor parar no meio de uma execução.
///
/// Três razões, e elas são diferentes de propósito:
///
/// - **revogado**: a sessão acabou. É imediato e não espera prazo nenhum — é o
///   passo 1 do §5 do contrato, «nenhum novo efeito é admitido»;
/// - **prazo**: esta volta de execução passou do tempo. Vale contra um laço
///   infinito que não aloca nada e por isso nunca esbarra no teto de memória;
/// - **consultas**: esta volta passou do trabalho. Vale contra o laço que
///   corre rápido demais para o relógio ser consultado com frequência útil.
///
/// Um laço infinito escapa das duas primeiras se o motor nunca chamar o
/// tratador; é por isso que as três existem juntas.
#[derive(Debug)]
pub(crate) struct Interrupcao {
    revogado: AtomicBool,
    consultas: AtomicUsize,
    /// Quando esta volta começou. Guardado em milissegundos desde o início do
    /// executor porque `Instant` não cabe num atômico.
    comeco: std::sync::Mutex<Option<Instant>>,
    limites: Limites,
}

impl Interrupcao {
    /// Uma interrupção com os três tetos de uma volta.
    #[must_use]
    pub(crate) fn nova(limites: Limites) -> Self {
        Self {
            revogado: AtomicBool::new(false),
            consultas: AtomicUsize::new(0),
            comeco: std::sync::Mutex::new(None),
            limites,
        }
    }

    /// A sessão acabou. **Monotônico**: nada volta a ser admitido.
    pub(crate) fn revogar(&self) {
        self.revogado.store(true, Ordering::Release);
    }

    /// Foi revogada?
    #[must_use]
    pub(crate) fn revogada(&self) -> bool {
        self.revogado.load(Ordering::Acquire)
    }

    /// Começa uma volta: zera o trabalho e marca a hora.
    fn comecar(&self) {
        self.consultas.store(0, Ordering::Relaxed);
        if let Ok(mut comeco) = self.comeco.lock() {
            *comeco = Some(Instant::now());
        }
    }

    /// O que o motor pergunta a cada tanto.
    fn deve_parar(&self) -> bool {
        if self.revogada() {
            return true;
        }
        if self.consultas.fetch_add(1, Ordering::Relaxed) > self.limites.consultas_por_volta {
            return true;
        }
        // O relógio não é lido a cada consulta: `Instant::now` num laço apertado
        // custa mais que o trabalho que ele mede.
        if !self.consultas.load(Ordering::Relaxed).is_multiple_of(1024) {
            return false;
        }
        self.comeco
            .lock()
            .ok()
            .and_then(|c| *c)
            .is_some_and(|inicio| inicio.elapsed() > self.limites.prazo_por_volta)
    }
}

/// O que entra no executor.
#[derive(Debug)]
pub(crate) enum ParaODentro {
    /// O código do MOD, uma vez.
    Codigo(String),
    /// Uma resposta a um pedido que o MOD fez.
    Resposta(String),
    /// Pare.
    Encerrar,
}

/// O que sai do executor.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParaOFora {
    /// Uma mensagem que o MOD postou, em JSON.
    Mensagem(String),
    /// O código não compilou, ou a execução lançou.
    Falhou(String),
    /// A volta foi interrompida — por revogação, prazo ou trabalho.
    Interrompido,
    /// O executor parou. **É esta a confirmação** que `encerrou()` espera no
    /// contrato da interface: revogar é imediato e nosso, parar é dele.
    Parou,
}

/// Um MOD de pé, com heap e contexto próprios.
///
/// A afinidade é de uma thread só: o `Runtime` do QuickJS não atravessa
/// threads, e o binding é explícito sobre isso. O dono é quem a criou, e o
/// descarte acontece nela — a diretriz proíbe o contrário: «não tentar matar
/// uma thread Rust à força».
pub(crate) struct ExecutorQuickJs {
    para_dentro: Sender<ParaODentro>,
    para_fora: Receiver<ParaOFora>,
    interrupcao: Arc<Interrupcao>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ExecutorQuickJs {
    /// Sobe um runtime numa thread própria.
    ///
    /// # Errors
    ///
    /// Falha quando a thread não sobe.
    pub(crate) fn novo(limites: Limites) -> std::io::Result<Self> {
        let (para_dentro, recebe) = std::sync::mpsc::channel::<ParaODentro>();
        let (manda, para_fora) = std::sync::mpsc::channel::<ParaOFora>();
        let interrupcao = Arc::new(Interrupcao::nova(limites));
        let dela = Arc::clone(&interrupcao);

        let thread = std::thread::Builder::new()
            .name("mod-quickjs".into())
            .spawn(move || rodar(&recebe, &manda, &dela, limites))?;

        Ok(Self {
            para_dentro,
            para_fora,
            interrupcao,
            thread: Some(thread),
        })
    }

    /// Entrega o código do MOD.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    pub(crate) fn iniciar(&self, codigo: &str) -> Result<(), &'static str> {
        self.para_dentro
            .send(ParaODentro::Codigo(codigo.to_owned()))
            .map_err(|_| "o executor não está de pé")
    }

    /// Entrega uma resposta a um pedido que o MOD fez.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    pub(crate) fn entregar(&self, json: &str) -> Result<(), &'static str> {
        self.para_dentro
            .send(ParaODentro::Resposta(json.to_owned()))
            .map_err(|_| "o executor não está de pé")
    }

    /// O que o MOD postou, se já postou alguma coisa.
    #[must_use]
    pub(crate) fn receber(&self, prazo: Duration) -> Option<ParaOFora> {
        self.para_fora.recv_timeout(prazo).ok()
    }

    /// **Revoga agora e pede a parada.**
    ///
    /// A revogação é imediata e é a que importa para o §5 do contrato: a partir
    /// dela o motor para na primeira consulta, mesmo no meio de um laço que
    /// nunca terminaria. A parada física vem depois, e é [`Self::encerrou`] que
    /// a confirma.
    pub(crate) fn pedir_encerramento(&self) {
        self.interrupcao.revogar();
        let _ = self.para_dentro.send(ParaODentro::Encerrar);
    }

    /// Espera a thread confirmar que parou.
    ///
    /// **Diferente de revogar**, e a diretriz cobra a diferença: «até confirmar
    /// o descarte dos recursos, não anunciar que tudo foi limpo».
    ///
    /// # Errors
    ///
    /// Devolve o motivo quando a thread não confirmou dentro do prazo — o que é
    /// informação, e não um erro a esconder: um executor que não confirma é
    /// exatamente o que o contrato manda medir.
    pub(crate) fn encerrou(&mut self, prazo: Duration) -> Result<Duration, &'static str> {
        let relogio = Instant::now();
        self.pedir_encerramento();
        // A confirmação vem pelo canal, e não pelo `join`: `join` bloqueia sem
        // prazo, e uma thread presa numa função nativa prenderia a saída junto.
        let confirmou = loop {
            match self
                .para_fora
                .recv_timeout(prazo.saturating_sub(relogio.elapsed()))
            {
                Ok(ParaOFora::Parou) => break true,
                Ok(_) => {}
                Err(_) => break false,
            }
        };
        if !confirmou {
            return Err("o executor não confirmou a parada dentro do prazo");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Ok(relogio.elapsed())
    }
}

impl Drop for ExecutorQuickJs {
    fn drop(&mut self) {
        // O dono some sem ter pedido: revogar e avisar é tudo o que dá para
        // fazer sem bloquear um `drop`. A thread termina ao ver o canal fechado.
        self.interrupcao.revogar();
        let _ = self.para_dentro.send(ParaODentro::Encerrar);
    }
}

/// O laço da thread do motor.
fn rodar(
    recebe: &Receiver<ParaODentro>,
    manda: &Sender<ParaOFora>,
    interrupcao: &Arc<Interrupcao>,
    limites: Limites,
) {
    let Ok(runtime) = Runtime::new() else {
        let _ = manda.send(ParaOFora::Falhou("o motor não subiu".into()));
        let _ = manda.send(ParaOFora::Parou);
        return;
    };
    runtime.set_memory_limit(limites.memoria);
    let dela = Arc::clone(interrupcao);
    runtime.set_interrupt_handler(Some(Box::new(move || dela.deve_parar())));

    let Ok(contexto) = Context::full(&runtime) else {
        let _ = manda.send(ParaOFora::Falhou("o contexto não subiu".into()));
        let _ = manda.send(ParaOFora::Parou);
        return;
    };

    // **A única ponte para fora.** Uma função, e o que ela faz é pôr texto num
    // canal. Não há aqui disco, rede, IPC nem armazenamento — e a ausência não
    // é uma jaula construída com cuidado: é o que um contexto de QuickJS **é**
    // antes de alguém acrescentar coisas a ele.
    let saida = manda.clone();
    let montou = contexto.with(|ctx| -> rquickjs::Result<()> {
        let seele = rquickjs::Object::new(ctx.clone())?;
        seele.set(
            "postar",
            Function::new(ctx.clone(), move |json: String| {
                // O teto é o do quadro de controle, e ele existe aqui também:
                // uma mensagem que não caberia no fio não pode encher a fila da
                // janela no caminho até descobrir isso.
                if json.len() > 12 * 1024 {
                    return false;
                }
                saida.send(ParaOFora::Mensagem(json)).is_ok()
            })?,
        )?;
        ctx.globals().set("seele", seele)?;
        Ok(())
    });
    if montou.is_err() {
        let _ = manda.send(ParaOFora::Falhou("a ponte não montou".into()));
        let _ = manda.send(ParaOFora::Parou);
        return;
    }

    while let Ok(entrada) = recebe.recv() {
        if interrupcao.revogada() {
            break;
        }
        match entrada {
            ParaODentro::Encerrar => break,
            ParaODentro::Codigo(fonte) => {
                interrupcao.comecar();
                let resultado = contexto.with(|ctx| ctx.eval::<(), _>(fonte.as_bytes()));
                relatar(manda, interrupcao, resultado);
            }
            ParaODentro::Resposta(json) => {
                interrupcao.comecar();
                let resultado = contexto.with(|ctx| -> rquickjs::Result<()> {
                    let Ok(ao_responder) = ctx.globals().get::<_, Function<'_>>("aoResponder")
                    else {
                        return Ok(());
                    };
                    ao_responder.call::<_, ()>((json,))
                });
                relatar(manda, interrupcao, resultado);
            }
        }
        // Os jobs de Promise correm **dentro do mesmo prazo** da volta que os
        // criou: sem isto, um MOD moveria trabalho para uma microtarefa e
        // escaparia do teto pela porta de trás.
        //
        // **E a interrupção de um job é dita.** A primeira versão saía do laço
        // em silêncio: o MOD era parado no meio de uma microtarefa e ninguém
        // ficava sabendo — nem a janela, nem quem hospeda, nem quem escreveu o
        // MOD. É o defeito que o `CLAUDE.md` deste repositório nomeia como o
        // mais caro daqui, cometido pelo próprio mecanismo de contenção.
        while runtime.is_job_pending() && !interrupcao.revogada() {
            if runtime.execute_pending_job().is_err() {
                let _ = manda.send(ParaOFora::Interrompido);
                break;
            }
        }
    }

    // O descarte acontece **aqui**, na dona do runtime, e não noutra thread.
    drop(contexto);
    drop(runtime);
    let _ = manda.send(ParaOFora::Parou);
}

/// Traduz o resultado de uma volta para o canal de saída.
fn relatar(
    manda: &Sender<ParaOFora>,
    interrupcao: &Arc<Interrupcao>,
    resultado: rquickjs::Result<()>,
) {
    match resultado {
        Ok(()) => {}
        Err(_) if interrupcao.revogada() => {
            let _ = manda.send(ParaOFora::Interrompido);
        }
        // Uma interrupção chega como erro do motor, como qualquer outra
        // exceção. Distinguir as duas importa: uma é o MOD com defeito, a
        // outra é o produto parando o MOD, e elas pedem frases diferentes.
        Err(rquickjs::Error::Exception) => {
            let _ = manda.send(ParaOFora::Falhou("o MOD lançou".into()));
        }
        Err(erro) => {
            let _ = manda.send(ParaOFora::Interrompido);
            let _ = erro;
        }
    }
}

/// A sonda de fronteira do QuickJS — a irmã da que rodou no WKWebView.
///
/// Ela tenta, um por um, os caminhos que a sonda da janela **alcançou**, e
/// alguns que ela nem tinha como tentar. O resultado sai por `seele.postar`,
/// que é a única porta.
///
/// Escrita aqui e não num arquivo de MOD de propósito: ela é o instrumento
/// desta medição, e um instrumento que mora junto do que ele mede não sai de
/// sincronia com ele.
const SONDA: &str = r#"
const achados = [];
function tentar(nome, o_que) {
  try {
    const valor = o_que();
    achados.push({ nome, alcancou: true, detalhe: String(valor).slice(0, 80) });
  } catch (erro) {
    achados.push({ nome, alcancou: false, detalhe: String(erro.name || erro).slice(0, 80) });
  }
}

// O que a sonda da janela alcançou, e que aqui não deveria existir.
for (const nome of [
  'indexedDB', 'caches', 'localStorage', 'sessionStorage', 'BroadcastChannel',
  'fetch', 'XMLHttpRequest', 'WebSocket', 'Worker', 'SharedWorker',
  'importScripts', 'document', 'window', 'navigator', 'crypto',
  '__TAURI__', '__TAURI_INTERNALS__', 'require', 'process', 'std', 'os',
]) {
  tentar('ausente:' + nome, () => {
    const valor = globalThis[nome];
    if (valor === undefined) throw new Error('undefined');
    return typeof valor;
  });
}

// E o que **está** aqui: a lista inteira, sem corte. Ela não passa por
// `tentar` porque aquele corta em oitenta caracteres — e a lista cortada
// esconderia justamente o que alguém acrescentou no fim do alfabeto.
achados.push({
  nome: 'globais',
  alcancou: true,
  detalhe: Object.getOwnPropertyNames(globalThis).sort().join(','),
});

seele.postar(JSON.stringify(achados));
"#;

#[cfg(test)]
mod testes {
    use super::*;

    /// Um executor com os tetos do produto.
    fn executor() -> ExecutorQuickJs {
        ExecutorQuickJs::novo(Limites::default()).expect("a thread do motor")
    }

    /// Um executor em que **só um** mecanismo pode parar uma volta.
    ///
    /// Os outros dois ficam altos a ponto de não alcançarem, e é isso que faz
    /// cada teste abaixo medir o que ele diz medir.
    ///
    /// A primeira versão destes testes usava os tetos do produto, e dois deles
    /// **passavam com a revogação arrancada** — o teto de trabalho os salvava.
    /// Um guarda que passa sem o conserto não guarda nada, e a reversão foi o
    /// que mostrou isso.
    fn so_com(consultas: usize, prazo: Duration) -> ExecutorQuickJs {
        ExecutorQuickJs::novo(Limites {
            memoria: TETO_DE_MEMORIA,
            consultas_por_volta: consultas,
            prazo_por_volta: prazo,
        })
        .expect("a thread do motor")
    }

    /// Alto a ponto de não chegar dentro de um teste.
    const SEM_TETO_DE_TRABALHO: usize = usize::MAX;
    /// Longo a ponto de não chegar dentro de um teste.
    const SEM_PRAZO: Duration = Duration::from_secs(3600);

    fn uma_mensagem(executor: &ExecutorQuickJs) -> String {
        match executor.receber(Duration::from_secs(5)) {
            Some(ParaOFora::Mensagem(json)) => json,
            Some(outro) => panic!("veio {outro:?} em vez de mensagem"),
            None => panic!("o executor não respondeu"),
        }
    }

    /// **A fronteira do QuickJS, medida** — etapa E1.
    ///
    /// A sonda que rodou no WKWebView alcançou `indexedDB`, `caches`,
    /// `BroadcastChannel` e a rota do IPC do Tauri. Esta roda a mesma pergunta
    /// no motor que é candidato a substituí-lo.
    ///
    /// **A diferença que importa não é a lista, é de onde ela vem.** No
    /// navegador, o ambiente existe e a fronteira é tudo o que se consegue
    /// tirar dele. Aqui o contexto nasce sem ambiente nenhum, e a fronteira é
    /// tudo o que alguém escolheu pôr — uma função por vez, neste arquivo.
    ///
    /// Por isso este teste é automático e o da janela não era: ele não precisa
    /// de aplicativo, de servidor nem de clique. É a prova de E1 que dá para
    /// repetir a cada commit.
    #[test]
    fn um_mod_no_quickjs_nao_alcanca_ambiente_nenhum() {
        let executor = executor();
        executor.iniciar(SONDA).expect("entregar o código");
        let achados: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("a sonda devolve JSON");

        let mut alcancados = Vec::new();
        let mut globais = String::new();
        for achado in achados.as_array().expect("uma lista") {
            let nome = achado["nome"].as_str().unwrap_or_default();
            if nome == "globais" {
                globais = achado["detalhe"].as_str().unwrap_or_default().to_owned();
                continue;
            }
            if achado["alcancou"].as_bool().unwrap_or(false) {
                alcancados.push(nome.to_owned());
            }
        }

        assert!(
            alcancados.is_empty(),
            "o código de um MOD alcançou ambiente que ninguém lhe deu: {alcancados:?}"
        );

        // Impresso sempre: quem rodar com `--nocapture` lê a superfície inteira
        // do contexto, e é ela que precisa ser revisada quando alguém a mudar.
        println!("globais do contexto de um MOD: {globais}");
        assert!(
            globais.split(',').any(|nome| nome == "seele"),
            "a ponte não está no contexto: {globais}"
        );
        for proibido in ["indexedDB", "fetch", "Worker", "__TAURI"] {
            assert!(
                !globais.contains(proibido),
                "`{proibido}` apareceu na lista de globais: {globais}"
            );
        }
    }

    /// **Um laço infinito não prende a saída** — e quem o para é a revogação.
    ///
    /// O critério de aceite da E1 escreve isso, e é a propriedade que separa «o
    /// MOD está isolado» de «o MOD está isolado enquanto ele colaborar».
    ///
    /// **Sem teto de trabalho e sem prazo**: se o laço parar, foi porque a
    /// sessão foi revogada, e não porque o motor cansou.
    /// **E o laço tem de estar rodando quando a revogação chega.**
    ///
    /// A primeira versão deste teste passava em 0,00 s com a revogação
    /// arrancada, e passava porque o laço nunca começava: `pedir_encerramento`
    /// marca o revogado **antes** de mandar a mensagem, e a thread via a marca
    /// ao tirar o `Codigo` da fila e saía sem rodar nada. O que o teste media
    /// era a conferência de fila, não a interrupção.
    ///
    /// Por isso o MOD avisa que começou. Quando `comecei` chega, a thread está
    /// dentro do `eval`, e dali só o tratador de interrupção a tira.
    #[test]
    fn a_revogacao_sozinha_para_um_laco_infinito() {
        let mut executor = so_com(SEM_TETO_DE_TRABALHO, SEM_PRAZO);
        executor
            .iniciar("seele.postar('comecei'); while (true) {}")
            .expect("entregar o código");
        assert_eq!(
            uma_mensagem(&executor),
            "comecei",
            "o laço não chegou a rodar"
        );
        let levou = executor
            .encerrou(Duration::from_secs(10))
            .expect("o executor tem de confirmar a parada");
        assert!(
            levou < Duration::from_secs(5),
            "a saída levou {levou:?} com um MOD em laço infinito"
        );
    }

    /// **O prazo sozinho interrompe uma volta**, sem revogação e sem teto de
    /// trabalho.
    ///
    /// Contra o MOD que trava a si mesmo sem que ninguém tenha mandado sair.
    /// «Só contar consultas do motor não estabelece prazo real de saída» — a
    /// diretriz nomeia, e este teste é o que separa as duas coisas.
    #[test]
    fn o_prazo_sozinho_interrompe_uma_volta() {
        let executor = so_com(SEM_TETO_DE_TRABALHO, Duration::from_millis(50));
        executor.iniciar("while (true) {}").expect("código");
        match executor.receber(Duration::from_secs(5)) {
            Some(ParaOFora::Interrompido | ParaOFora::Falhou(_)) => {}
            outro => panic!("a volta não foi interrompida pelo prazo: {outro:?}"),
        }
        // Sem revogação nenhuma nesta linha: quem parou foi o relógio.
        assert!(!executor.interrupcao.revogada());
    }

    /// **O teto de trabalho sozinho interrompe uma volta**, sem prazo e sem
    /// revogação.
    ///
    /// Vale contra o laço que corre rápido demais para o relógio ser consultado
    /// com frequência útil — e por isso ele existe ao lado do prazo, e não no
    /// lugar dele.
    #[test]
    fn o_teto_de_trabalho_sozinho_interrompe_uma_volta() {
        let executor = so_com(1_000, SEM_PRAZO);
        executor.iniciar("while (true) {}").expect("código");
        match executor.receber(Duration::from_secs(5)) {
            Some(ParaOFora::Interrompido | ParaOFora::Falhou(_)) => {}
            outro => panic!("a volta não foi interrompida pelo trabalho: {outro:?}"),
        }
    }

    /// **Uma microtarefa não escapa do prazo da volta que a criou.**
    ///
    /// Sem isto, um MOD moveria o laço para dentro de uma `Promise` e correria
    /// fora do teto — pela porta de trás do motor.
    ///
    /// Medido com o prazo curto e **sem teto de trabalho**, para que a parada
    /// só possa ser explicada pelo relógio da volta.
    #[test]
    fn um_laco_dentro_de_uma_promessa_tambem_para() {
        let executor = so_com(SEM_TETO_DE_TRABALHO, Duration::from_millis(50));
        executor
            .iniciar("Promise.resolve().then(() => { while (true) {} });")
            .expect("código");
        match executor.receber(Duration::from_secs(10)) {
            Some(ParaOFora::Interrompido | ParaOFora::Falhou(_)) => {}
            outro => panic!("o laço dentro da promessa não foi interrompido: {outro:?}"),
        }
    }

    /// **A mensagem que não caberia no fio não entra na fila.**
    ///
    /// O mesmo teto de 12 KiB do quadro de controle, conferido onde a mensagem
    /// nasce. Descobrir o tamanho depois de ela já estar na fila da janela
    /// seria pagar a memória duas vezes para recusar no fim.
    #[test]
    fn uma_mensagem_grande_demais_e_recusada_na_porta() {
        let executor = executor();
        executor
            .iniciar("seele.postar('x'.repeat(13 * 1024)); seele.postar('coube');")
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "coube");
    }

    /// **A volta completa: o MOD pergunta, o produto responde, o MOD continua.**
    ///
    /// É o quarto verbo do contrato — `entregar` — e sem este teste ele
    /// existiria sem nunca ter sido exercitado. O que ele prende é que a
    /// resposta chega ao código do autor pelo nome que a API promete, e que o
    /// que ele fizer com ela ainda sai pela mesma porta.
    #[test]
    fn a_resposta_volta_para_dentro_e_o_mod_segue() {
        let executor = executor();
        executor
            .iniciar(
                "globalThis.aoResponder = (json) => {                    seele.postar(JSON.stringify({ recebi: JSON.parse(json).valor }));                  };                  seele.postar('pronto');",
            )
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "pronto");

        executor.entregar(r#"{"valor":42}"#).expect("entregar");
        let volta: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("JSON");
        assert_eq!(
            volta.get("recebi").and_then(serde_json::Value::as_u64),
            Some(42)
        );
    }

    /// E uma resposta que chega **depois da revogação** não entra.
    ///
    /// §5 do contrato, passo 1: «nenhum novo efeito daquela instância é
    /// admitido, mesmo que mensagens já estejam na fila». A fila é literal
    /// aqui — é um canal — e a conferência acontece ao tirar dela.
    #[test]
    fn uma_resposta_depois_da_revogacao_nao_entra() {
        let executor = executor();
        executor
            .iniciar(
                "globalThis.aoResponder = () => seele.postar('entrou');                  seele.postar('pronto');",
            )
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "pronto");

        executor.interrupcao.revogar();
        executor.entregar(r#"{"valor":1}"#).expect("entregar");
        match executor.receber(Duration::from_millis(500)) {
            Some(ParaOFora::Parou) | None => {}
            Some(outro) => panic!("a resposta entrou depois da revogação: {outro:?}"),
        }
    }

    /// E o descarte acontece na dona do runtime, sem matar thread à força.
    #[test]
    fn o_encerramento_confirma_e_a_thread_sai_sozinha() {
        let mut executor = executor();
        executor.iniciar("seele.postar('oi');").expect("código");
        assert_eq!(uma_mensagem(&executor), "oi");
        assert!(executor.encerrou(Duration::from_secs(5)).is_ok());
    }
}
