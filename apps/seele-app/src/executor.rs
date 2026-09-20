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

/// Quantas mensagens do MOD cabem esperando a janela lê-las.
///
/// **Não é o mesmo teto da mensagem.** Uma mensagem tem 12 KiB, que é o do
/// quadro de controle; este é o do **acumulado**, e sem ele um MOD num laço
/// enche a memória de quem usa uma mensagem de cada vez, todas dentro do
/// limite individual.
pub(crate) const MENSAGENS_NA_FILA: usize = 64;

/// Quantos bytes cabem na mesma fila.
///
/// Existe **ao lado** do teto de quantidade porque as duas saturações são
/// diferentes: sessenta e quatro mensagens de 12 KiB são 768 KiB, e um milhão
/// de mensagens de dez bytes é uma inundação que a contagem pegaria e os bytes
/// não, e vice-versa. Quem enche primeiro fecha a porta.
pub(crate) const BYTES_NA_FILA: usize = 256 * 1024;

/// A fila de saída, medida.
///
/// **Nada aqui bloqueia.** Uma fila que faz quem escreve esperar seria uma fila
/// que prende a thread do motor — e prender a thread do motor é prender o
/// encerramento, que é justamente o que não pode acontecer. Cheia, ela recusa e
/// conta; o MOD recebe `false` e fica sabendo, e quem hospeda lê o contador.
/// Quantos avisos de diagnóstico cabem esperando, por instância.
///
/// **Cota própria, e pequena.** Um aviso nasce de uma falha, e uma falha não
/// pode consumir a cota do que está funcionando — nem ficar de fora de cota
/// nenhuma, que era o caso: `Falhou` e `Interrompido` saíam do motor sem
/// reservar nada, e um MOD que lança num laço enchia o canal sozinho.
pub(crate) const AVISOS_NA_FILA: usize = 16;

#[derive(Debug, Default)]
pub(crate) struct Fila {
    mensagens: AtomicUsize,
    bytes: AtomicUsize,
    recusadas: AtomicUsize,
    /// Os avisos esperando, na cota deles.
    avisos: AtomicUsize,
    /// Quantos avisos não couberam.
    avisos_recusados: AtomicUsize,
}

impl Fila {
    /// Tenta reservar lugar para uma mensagem. `false` quer dizer «não coube».
    pub(crate) fn cabe(&self, quantos: usize) -> bool {
        // Conferido **antes** de somar, e somado só se couber: somar primeiro e
        // devolver depois deixaria uma janela em que a fila se diz maior do que
        // é, e duas mensagens simultâneas se recusariam por causa uma da outra.
        if self.mensagens.load(Ordering::Acquire) >= MENSAGENS_NA_FILA
            || self.bytes.load(Ordering::Acquire).saturating_add(quantos) > BYTES_NA_FILA
        {
            self.recusadas.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.mensagens.fetch_add(1, Ordering::AcqRel);
        self.bytes.fetch_add(quantos, Ordering::AcqRel);
        true
    }

    /// Tenta reservar lugar para um aviso, na cota dele.
    ///
    /// **Simétrico com [`Self::cabe`], e separado.** Cada classe reserva na sua
    /// cota e é devolvida pela mesma regra — foi a assimetria que fazia colher
    /// um erro devolver crédito que ninguém tinha tomado.
    pub(crate) fn cabe_aviso(&self) -> bool {
        if self.avisos.load(Ordering::Acquire) >= AVISOS_NA_FILA {
            self.avisos_recusados.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.avisos.fetch_add(1, Ordering::AcqRel);
        true
    }

    /// Um aviso saiu da fila.
    pub(crate) fn tirar_aviso(&self) {
        let _ = self
            .avisos
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                Some(n.saturating_sub(1))
            });
    }

    /// Quantos avisos não couberam.
    #[must_use]
    pub(crate) fn avisos_recusados(&self) -> usize {
        self.avisos_recusados.load(Ordering::Relaxed)
    }

    /// Uma mensagem saiu da fila.
    pub(crate) fn tirar(&self, quantos: usize) {
        self.tirar_varios(1, quantos);
    }

    /// Várias saíram de uma vez — é o que a colheita faz.
    pub(crate) fn tirar_varios(&self, mensagens: usize, bytes: usize) {
        // Saturante nos dois: um descompasso de contabilidade não pode virar
        // um estouro que envolve o contador e faz a fila parecer vazia.
        let _ = self
            .mensagens
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                Some(n.saturating_sub(mensagens))
            });
        let _ = self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                Some(n.saturating_sub(bytes))
            });
    }

    /// Quantas mensagens o MOD tentou pôr e não couberam.
    ///
    /// **Contado, e não engolido.** Uma mensagem que o produto descarta em
    /// silêncio é o defeito que este repositório mais paga; aqui o MOD recebe
    /// `false` na hora e quem hospeda lê o total depois.
    #[must_use]
    pub(crate) fn recusadas(&self) -> usize {
        self.recusadas.load(Ordering::Relaxed)
    }

    /// Quanto está esperando ser lido, em mensagens e em bytes.
    #[must_use]
    pub(crate) fn ocupacao(&self) -> (usize, usize) {
        (
            self.mensagens.load(Ordering::Acquire),
            self.bytes.load(Ordering::Acquire),
        )
    }

    /// Quantos avisos estão de pé na cota — **em qualquer ponto do caminho**.
    ///
    /// O crédito de um aviso é reservado quando o motor o manda e só sai quando
    /// a janela o colhe ou o descarte o solta. Entre uma coisa e outra ele pode
    /// estar no canal, esperando a bomba, ou já na instância, esperando a
    /// janela: é a mesma reserva, e é por isso que há um número só.
    pub(crate) fn avisos_de_pe(&self) -> usize {
        self.avisos.load(Ordering::Acquire)
    }
}

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

/// Um temporizador que o MOD pediu, e que o anfitrião possui.
///
/// **Do anfitrião, e não do motor.** O QuickJS não tem laço de eventos: quem
/// tem relógio aqui é a thread que roda o motor. Isso não é uma limitação a
/// contornar — é o que faz um temporizador ser um **recurso com dono**, que o
/// §4.2 do contrato exige: encerrar a instância apaga a tabela, e nenhum
/// temporizador sobrevive porque o MOD esqueceu de cancelá-lo.
#[derive(Debug, Clone, Copy)]
struct Temporizador {
    /// Quando ele vence.
    quando: Instant,
    /// De quanto em quanto, se ele repete.
    repete: Option<Duration>,
}

/// Quantos temporizadores um MOD pode ter de pé.
///
/// Teto porque a tabela é memória do anfitrião: um MOD num laço criando
/// temporizadores enche a memória de quem usa sem nunca passar pelo teto de
/// heap do motor, que conta outra coisa.
pub(crate) const TEMPORIZADORES_DE_PE: usize = 256;

/// O menor intervalo que um temporizador pode pedir.
///
/// Quatro milissegundos é o piso que os navegadores usam, e ele existe pela
/// mesma razão: um temporizador de zero em laço vira espera ocupada, e espera
/// ocupada num produto de voz disputa com o áudio.
pub(crate) const INTERVALO_MINIMO: Duration = Duration::from_millis(4);

/// O que entra no executor.
#[derive(Debug)]
pub(crate) enum ParaODentro {
    /// O código do MOD, uma vez.
    /// O fonte do MOD e a API que o manifesto dele declarou.
    Codigo {
        /// O que o pacote traz.
        fonte: String,
        /// O número do manifesto — ver `capacidades_da_api`.
        api: u32,
    },
    /// Uma resposta a um pedido que o MOD fez.
    Resposta(String),
    /// Pare.
    Encerrar,
    /// Conte o que você tem de pé.
    ///
    /// Existe porque «a tabela ficou vazia» não se observa de fora: ela é do
    /// anfitrião, e o MOD não a enxerga. Sem isto, provar que um intervalo
    /// cancelado saiu da tabela seria provar pela ausência de batidas — o que
    /// também acontece quando o motor morre.
    Diagnostico,
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
    /// O que o anfitrião tem de pé por este MOD.
    Diagnostico {
        /// Quantos temporizadores estão na tabela.
        relogios: usize,
    },
}

/// Um MOD de pé, com heap e contexto próprios.
///
/// A afinidade é de uma thread só: o `Runtime` do QuickJS não atravessa
/// threads, e o binding é explícito sobre isso. O dono é quem a criou, e o
/// descarte acontece nela — a diretriz proíbe o contrário: «não tentar matar
/// uma thread Rust à força».
pub(crate) struct ExecutorQuickJs {
    para_dentro: Sender<ParaODentro>,
    /// A fila de **entrada**, medida do mesmo jeito que a de saída.
    ///
    /// Ela não tinha teto, e a diretriz nomeia o buraco: `entregar` copiava e
    /// enfileirava o JSON sem limite. Um MOD que pede mais rápido do que o
    /// motor atende — ou uma janela que responde em rajada — fazia a memória
    /// crescer do lado de cá, onde nenhum teto de heap do motor conta.
    entrada: Arc<Fila>,
    /// **Some quando alguém escuta.** Ver [`Self::escutar`]: um canal tem um
    /// dono, e dois leitores dividiriam as mensagens em vez de vê-las.
    para_fora: Option<Receiver<ParaOFora>>,
    interrupcao: Arc<Interrupcao>,
    fila: Arc<Fila>,
    /// A thread do motor, para o `join` de quem esperou a confirmação.
    ///
    /// Lida só por [`Self::encerrou`], que é caminho de bancada: na janela
    /// ninguém espera, e a thread sai sozinha ao ver o canal fechado.
    #[cfg_attr(not(test), allow(dead_code))]
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
        let fila = Arc::new(Fila::default());
        let dele = Arc::clone(&fila);
        let entrada = Arc::new(Fila::default());
        let dela_entrada = Arc::clone(&entrada);

        let thread = std::thread::Builder::new()
            .name("mod-quickjs".into())
            .spawn(move || rodar(&recebe, &manda, &dela, &dele, &dela_entrada, limites))?;

        Ok(Self {
            para_dentro,
            para_fora: Some(para_fora),
            interrupcao,
            fila,
            entrada,
            thread: Some(thread),
        })
    }

    /// Entrega o código do MOD.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn iniciar(&self, codigo: &str) -> Result<(), &'static str> {
        self.iniciar_com_api(codigo, seele_ffi::mods::MOD_API_VERSION)
    }

    /// Entrega o código do MOD, dizendo **que API ele declarou**.
    ///
    /// A versão não é decoração: o prelúdio monta `SeeleUI` a partir dela, e um
    /// pacote de API 3 recebe um `SeeleUI` sem `superficies` e sem
    /// `contribuicoes`. Aceitar a API 3 não é dar à API 3 o que a 4 tem — ver
    /// `seele_ffi::mods::capacidades_da_api`.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    pub(crate) fn iniciar_com_api(&self, codigo: &str, api: u32) -> Result<(), &'static str> {
        self.para_dentro
            .send(ParaODentro::Codigo {
                fonte: codigo.to_owned(),
                api,
            })
            .map_err(|_| "o executor não está de pé")
    }

    /// Entrega uma resposta a um pedido que o MOD fez.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn diagnostico(&self) -> Result<(), &'static str> {
        self.para_dentro
            .send(ParaODentro::Diagnostico)
            .map_err(|_| "o executor não está de pé")
    }

    /// Entrega uma resposta a um pedido que o MOD fez.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    pub(crate) fn entregar(&self, json: &str) -> Result<(), &'static str> {
        // **O mesmo teto da saída, na entrada.** Sem ele, quem responde rápido
        // demais enche a memória de quem usa, e o motor nem chega a ver as
        // mensagens que já estão na fila.
        //
        // Recusado e dito: quem chamou recebe o motivo e decide — segurar,
        // descartar, ou avisar. Enfileirar sem teto decide por ele.
        if !self.entrada.cabe(json.len()) {
            return Err("a fila de entrada do executor está cheia");
        }
        let quantos = json.len();
        self.para_dentro
            .send(ParaODentro::Resposta(json.to_owned()))
            .map_err(|_| {
                self.entrada.tirar(quantos);
                "o executor não está de pé"
            })
    }

    /// A fila de entrada, para a bancada olhar.
    #[must_use]
    pub(crate) fn entrada(&self) -> &Fila {
        &self.entrada
    }

    /// O que o MOD postou, se já postou alguma coisa.
    ///
    /// **Só a bancada usa.** Na janela quem lê é a bomba de [`Self::escutar`],
    /// que não dorme entre mensagens; este caminho existe para os testes, onde
    /// esperar com prazo é mais simples que montar uma thread.
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) fn receber(&self, prazo: Duration) -> Option<ParaOFora> {
        let saiu = self.para_fora.as_ref()?.recv_timeout(prazo).ok();
        // A contabilidade é baixada **ao tirar**, e não ao entregar: é isto que
        // faz a fila voltar a aceitar assim que alguém a lê. Só mensagem ocupa
        // lugar; as outras variantes são avisos de tamanho fixo.
        if let Some(ParaOFora::Mensagem(json)) = &saiu {
            self.fila.tirar(json.len());
        }
        saiu
    }

    /// A fila de saída, para a bancada olhar.
    #[must_use]
    pub(crate) fn fila(&self) -> &Fila {
        &self.fila
    }

    /// Entrega o canal de saída a quem vai escutá-lo, uma vez só.
    ///
    /// **Um canal tem um dono.** Depois disto, [`Self::receber`] devolve
    /// `None` e [`Self::encerrou`] não tem como confirmar — quem escuta é que
    /// vê o [`ParaOFora::Parou`], e é por lá que a confirmação chega.
    ///
    /// Existe para a integração na janela: a fala de um MOD precisa virar
    /// evento assim que sai, e um laço de `receber` com prazo ou perderia
    /// tempo dormindo ou gastaria CPU acordando.
    ///
    /// A contabilidade da fila passa a ser de quem escuta: sem devolver o
    /// lugar, ela enche uma vez e não aceita mais nada.
    pub(crate) fn escutar(&mut self) -> Option<(Receiver<ParaOFora>, Arc<Fila>)> {
        self.para_fora
            .take()
            .map(|canal| (canal, Arc::clone(&self.fila)))
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn encerrou(&mut self, prazo: Duration) -> Result<Duration, &'static str> {
        let relogio = Instant::now();
        self.pedir_encerramento();
        // Quem escuta é quem vê a confirmação. Dizer isso em vez de esperar
        // para sempre por uma mensagem que vai para outro lugar.
        if self.para_fora.is_none() {
            return Err("outro leitor tem o canal: a confirmação chega por lá");
        }
        // A confirmação vem pelo canal, e não pelo `join`: `join` bloqueia sem
        // prazo, e uma thread presa numa função nativa prenderia a saída junto.
        let Some(canal) = self.para_fora.as_ref() else {
            return Err("sem canal de saída");
        };
        let confirmou = loop {
            match canal.recv_timeout(prazo.saturating_sub(relogio.elapsed())) {
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
    fila: &Arc<Fila>,
    // Nomeada assim, e não `entrada`: o laço abaixo já chama de `entrada` a
    // mensagem que chegou, e dois nomes iguais para coisas diferentes na mesma
    // função é como um erro entra sem ninguém ver.
    fila_de_entrada: &Arc<Fila>,
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

    // **Uma promessa que rejeita sem ninguém pegando é dita.**
    //
    // Sem isto ela morre calada: o motor descarta a rejeição e o MOD para onde
    // estava, sem uma palavra para a janela, para o registro nem para quem o
    // escreveu. Medi no aplicativo nativo antes de escrever a linha — o MOD de
    // referência chamava `SeeleMods.snapshot()` numa função `async` sem
    // `catch`, e quando a sessão ainda não tinha subido ele sumia na segunda
    // linha. No registro apareceu como duas mensagens e silêncio, e levei três
    // execuções para separar isso de um travamento.
    //
    // `is_handled` é a segunda chamada, quando alguém prende um `catch`
    // depois: essa não é falha, e avisar nela seria ensinar a ignorar o aviso.
    //
    // Passa pela mesma cota dos outros avisos, e por isso um MOD que rejeita
    // num laço não enche o canal — a cota é o que separa «dito» de «gritado».
    let manda_rejeicao = manda.clone();
    let fila_de_rejeicao = Arc::clone(fila);
    runtime.set_host_promise_rejection_tracker(Some(Box::new(
        move |ctx, _promessa, motivo, ja_tratada| {
            if ja_tratada {
                return;
            }
            let texto = motivo
                .as_exception()
                .and_then(rquickjs::Exception::message)
                .or_else(|| {
                    ctx.json_stringify(motivo.clone())
                        .ok()
                        .flatten()
                        .and_then(|s| s.to_string().ok())
                })
                .unwrap_or_else(|| "sem motivo".to_owned());
            avisar(
                &manda_rejeicao,
                &fila_de_rejeicao,
                ParaOFora::Falhou(format!("promessa rejeitada sem tratamento: {texto}")),
            );
        },
    )));

    let Ok(contexto) = Context::full(&runtime) else {
        let _ = manda.send(ParaOFora::Falhou("o contexto não subiu".into()));
        let _ = manda.send(ParaOFora::Parou);
        return;
    };

    // **A única ponte para fora.** Uma função, e o que ela faz é pôr texto num
    // canal. Não há aqui disco, rede, IPC nem armazenamento — e a ausência não
    // é uma jaula construída com cuidado: é o que um contexto de QuickJS **é**
    // antes de alguém acrescentar coisas a ele.
    //
    // O prelúdio que traduz esta ponte na API que um MOD conhece — `SeeleMods`
    // e `SeeleUI` — é montado sobre ela em [`PRELUDIO`], em JavaScript, pela
    // mesma razão que o do Worker: é código que roda dentro do contexto do MOD,
    // e escrevê-lo em Rust não o tornaria mais nosso.
    let saida = manda.clone();
    let contagem = Arc::clone(fila);
    let montou = contexto.with(|ctx| -> rquickjs::Result<()> {
        let seele = rquickjs::Object::new(ctx.clone())?;
        seele.set(
            "postar",
            Function::new(ctx.clone(), move |json: String| {
                // **Dois tetos, e eles respondem coisas diferentes.**
                //
                // O primeiro é o do quadro de controle: uma mensagem que não
                // caberia no fio não pode encher a fila da janela no caminho
                // até descobrir isso.
                if json.len() > 12 * 1024 {
                    return false;
                }
                // O segundo é o do **acumulado**. Sem ele, um MOD num laço
                // enche a memória de quem usa uma mensagem de cada vez, todas
                // dentro do limite individual — e a fila cresceria até o
                // processo cair, sem nada no caminho para dizer o que houve.
                //
                // Recusar, e não esperar: uma fila que faz esta função
                // bloquear prende a thread do motor, e prender a thread do
                // motor é prender o encerramento.
                if !contagem.cabe(json.len()) {
                    return false;
                }
                let quantos = json.len();
                if saida.send(ParaOFora::Mensagem(json)).is_ok() {
                    true
                } else {
                    // O outro lado sumiu: devolve o lugar em vez de deixá-lo
                    // reservado para sempre.
                    contagem.tirar(quantos);
                    false
                }
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

    // A tabela de temporizadores, que é do anfitrião e morre com ele.
    let mut relogios: std::collections::BTreeMap<u32, Temporizador> =
        std::collections::BTreeMap::new();

    loop {
        // **Espera até o pedido seguinte ou até o temporizador mais próximo.**
        // É isto que faz esta thread não gastar CPU parada: sem temporizador
        // ela dorme no canal, e com um ela dorme até a hora dele.
        let entrada = match proximo_vencimento(&relogios) {
            Some(quando) => match recebe.recv_timeout(quando) {
                Ok(entrada) => Some(entrada),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match recebe.recv() {
                Ok(entrada) => Some(entrada),
                Err(_) => break,
            },
        };
        if interrupcao.revogada() {
            break;
        }
        let Some(entrada) = entrada else {
            // Venceu algum: dispara os que estão na hora, dentro de uma volta
            // com prazo, como qualquer outra execução.
            interrupcao.comecar();
            let vencidos = recolher_vencidos(&mut relogios);
            for id in vencidos {
                let resultado = contexto.with(|ctx| -> rquickjs::Result<()> {
                    let Ok(bate) = ctx.globals().get::<_, Function<'_>>("__seeleRelogio") else {
                        return Ok(());
                    };
                    bate.call::<_, ()>((id,))
                });
                relatar(manda, fila, interrupcao, resultado);
            }
            escoar_jobs(&runtime, interrupcao, manda, fila);
            // **Também aqui.** Sem esta linha, um callback que agenda outro
            // temporizador — ou que cancela o próprio intervalo — deixava o
            // pedido parado até chegar uma mensagem de fora. Num MOD que só
            // usa relógio, «uma mensagem de fora» pode nunca chegar: o timeout
            // encadeado nunca disparava e o intervalo cancelado continuava
            // batendo. O `continue` escondia os dois.
            recolher_pedidos_de_relogio(&contexto, &mut relogios);
            continue;
        };
        match entrada {
            ParaODentro::Encerrar => break,
            ParaODentro::Diagnostico => {
                let _ = manda.send(ParaOFora::Diagnostico {
                    relogios: relogios.len(),
                });
            }
            ParaODentro::Codigo { fonte, api } => {
                interrupcao.comecar();
                // O prelúdio primeiro, e o código do MOD depois, na mesma
                // volta: se o prelúdio não subir, o MOD não deve subir.
                //
                // **As capacidades entram antes do prelúdio**, como um vetor
                // que ele lê e apaga. Passá-las por interpolação de texto faria
                // o prelúdio deixar de ser uma constante — e uma constante é
                // exatamente o que dá para revisar uma vez e confiar sempre.
                let capacidades = seele_ffi::mods::capacidades_da_api(api);
                let resultado = contexto.with(|ctx| {
                    let lista = rquickjs::Array::new(ctx.clone())?;
                    for (onde, nome) in capacidades.iter().enumerate() {
                        lista.set(onde, *nome)?;
                    }
                    ctx.globals().set("__seeleCapacidades", lista)?;
                    ctx.eval::<(), _>(PRELUDIO.as_bytes())?;
                    ctx.eval::<(), _>(fonte.as_bytes())
                });
                relatar(manda, fila, interrupcao, resultado);
            }
            ParaODentro::Resposta(json) => {
                // O lugar volta **ao tirar da fila**, que é quando ela deixa de
                // ocupar memória — e é o que a faz voltar a aceitar.
                fila_de_entrada.tirar(json.len());
                interrupcao.comecar();
                let resultado = contexto.with(|ctx| -> rquickjs::Result<()> {
                    let Ok(ao_responder) = ctx.globals().get::<_, Function<'_>>("aoResponder")
                    else {
                        return Ok(());
                    };
                    ao_responder.call::<_, ()>((json,))
                });
                relatar(manda, fila, interrupcao, resultado);
            }
        }
        escoar_jobs(&runtime, interrupcao, manda, fila);
        // Os pedidos de temporizador que o MOD fez nesta volta.
        recolher_pedidos_de_relogio(&contexto, &mut relogios);
    }

    // O descarte acontece **aqui**, na dona do runtime, e não noutra thread.
    drop(contexto);
    drop(runtime);
    let _ = manda.send(ParaOFora::Parou);
}

/// O que roda **antes** do código do MOD, dentro do contexto dele.
///
/// A mesma API que o prelúdio do Worker oferece — `SeeleMods.request`,
/// `SeeleMods.snapshot`, `SeeleUI.regiao`, `SeeleUI.tema` —, montada sobre a
/// única ponte que existe aqui: `seele.postar`.
///
/// **Escrito em JavaScript, e não em Rust**, pela mesma razão do outro: é
/// código que roda dentro do contexto do MOD, e um MOD pode redefinir o que
/// quiser depois dele. O que garante a fronteira é o contexto não ter ambiente,
/// e não este texto.
///
/// O teto de oito pedidos em voo é o mesmo, e existe aqui pela mesma razão que
/// existe lá: o MOD recebe o erro onde ele o escreveu.
const PRELUDIO: &str = r#"
'use strict';
(() => {
  const pendentes = new Map();
  let proximo = 0;

  let ouvinte = null;

  globalThis.aoResponder = (texto) => {
    let m;
    try { m = JSON.parse(texto); } catch { return; }
    if (m.tipo === 'evento') {
      if (!ouvinte) return;
      // O erro do MOD fica com o MOD: um ouvinte que lança não pode impedir o
      // próximo evento de chegar. A volta inteira falharia, e o anfitrião a
      // relataria como `Falhou` — que é o MOD com defeito, não este.
      try { ouvinte(m); } catch (erro) { seele.postar(JSON.stringify({ tipo: 'erro-no-evento', erro: String(erro) })); }
      return;
    }
    const espera = pendentes.get(m.n);
    if (!espera) return;
    pendentes.delete(m.n);
    if (m.ok) espera.resolve(m.valor);
    else espera.reject(new Error(m.erro || 'recusado'));
  };

  const pedir = (tipo, carga) => new Promise((resolve, reject) => {
    if (pendentes.size >= 8) { reject(new Error('too-many-requests')); return; }
    const n = ++proximo;
    pendentes.set(n, { resolve, reject });
    if (!seele.postar(JSON.stringify({ tipo, n, ...carga }))) {
      pendentes.delete(n);
      reject(new Error('fila-cheia'));
    }
  });

  globalThis.SeeleMods = Object.freeze({
    request: (id, canal, valor) => pedir('pedido', { id, canal, valor }),
    snapshot: () => pedir('snapshot', {}),
  });

  // ---- as capacidades desta execução ----
  //
  // **O que o pacote declarou, e não o que este build oferece.** O anfitrião
  // põe a lista em `__seeleCapacidades` antes de avaliar este texto, a partir
  // de `capacidades_da_api(manifesto.api)`.
  //
  // O vetor é lido e **apagado**: um MOD que o encontrasse depois poderia
  // reescrevê-lo, e o próximo a lê-lo leria o que o MOD escreveu. Apagar aqui
  // não é a fronteira — a fronteira é o anfitrião conferir cada mensagem —, é
  // não deixar uma pergunta com duas respostas.
  const capacidades = new Set(Array.isArray(globalThis.__seeleCapacidades)
    ? globalThis.__seeleCapacidades
    : []);
  delete globalThis.__seeleCapacidades;

  /**
   * Monta um objeto só com o que esta API tem.
   *
   * **Ausente, e não recusado em tempo de execução.** Um `superficies` que
   * existisse e sempre falhasse faria um MOD de API 3 descobrir o problema
   * dentro de um `catch`, com uma mensagem, em produção. Um método que não
   * existe é um `TypeError` na primeira linha que o chama, com pilha, onde
   * quem escreveu o MOD consegue ver.
   */
  const comCapacidade = (nome, membros) => (capacidades.has(nome) ? membros : null);

  const superficies = comCapacidade('superficies', Object.freeze({
    // Devolve um punho local: os métodos serializam pedidos, e nada aqui é um
    // nó do documento. §3 do plano.
    criar: async (descricao) => {
      const id = String(descricao && descricao.id || '');
      await pedir('superficie-criar', { descricao });
      return Object.freeze({
        id,
        montar: (arvore) => pedir('superficie-montar', { superficie: id, arvore }),
        classes: (mapa) => pedir('superficie-classes', { superficie: id, classes: mapa }),
        mostrar: () => pedir('superficie-mostrar', { superficie: id }),
        ocultar: () => pedir('superficie-ocultar', { superficie: id }),
        suja: (valor) => pedir('superficie-suja', { superficie: id, suja: valor !== false }),
        titulo: (texto) => pedir('superficie-titulo', { superficie: id, titulo: texto }),
        fechar: (motivo) => pedir('superficie-fechar', { superficie: id, motivo }),
        descartar: () => pedir('superficie-descartar', { superficie: id }),
      });
    },
    // Um aviso é uma superfície de vida curta, e por isso tem atalho: um MOD
    // que precise dizer «gravado» não deve ter de montar uma janela.
    avisar: (texto, tom) => pedir('superficie-criar', {
      descricao: { id: 'aviso-' + (++proximo), tipo: 'aviso', titulo: String(texto ?? ''), tom },
    }),
  }));

  const contribuicoes = comCapacidade('contribuicoes', Object.freeze({
    registrar: (pedido) => pedir('contribuir', { pedido }),
    revogar: (handle) => pedir('revogar-contribuicao', { handle }),
  }));

  globalThis.SeeleUI = Object.freeze({
    regiao: (conteudo) => pedir('regiao', { conteudo }),
    tema: (valores) => pedir('tema', { valores }),
    ...(superficies ? { superficies } : {}),
    ...(contribuicoes ? { contribuicoes } : {}),
    // A lista, legível pelo próprio MOD: um pacote que queira degradar em vez
    // de falhar precisa poder perguntar, e perguntar a `SeeleUI.superficies`
    // já é a resposta — mas um nome explícito evita o truque.
    capacidades: () => [...capacidades],

    // **Um cartão por pessoa na lista do produto.** A declaração é a mesma da
    // região, montada pelo mesmo renderer, com uma gramática menor: nada que
    // receba foco ou clique, porque a linha do roster já tem um botão do
    // produto. É a única superfície fora da região, e ela sai com ela.
    cartoes: (cartoes) => pedir('cartoes', { cartoes }),
    // ---- os eventos ----
    //
    // **A janela fala com o MOD sem que ele tenha perguntado.** Um pedido tem
    // número e resposta; um evento não tem nem um nem outro, porque quem
    // digita não espera o MOD confirmar que recebeu a tecla.
    //
    // Um ouvinte só, e o último vence. Uma lista de ouvintes seria um MOD
    // registrando dentro de um laço e a janela mantendo a lista viva; e
    // «remover» exigiria devolver um cancelador que um MOD pode perder.
    aoEvento: (fn) => { ouvinte = typeof fn === 'function' ? fn : null; },

    // ---- o arquivo que alguém escolheu ----
    //
    // O MOD **não** abre o seletor: ele declara a forma `arquivo`, a pessoa
    // aperta, e o evento traz o identificador. Daqui ele lê os bytes em
    // pedaços e os manda para o servidor dele pelo protocolo que ele já tem.
    //
    // Sem esta função, um MOD teria o identificador e nada para fazer com ele.
    // Com acesso ao disco, ele teria o caminho errado — e nenhum cliente do
    // SEELE abre arquivo por conta de terceiro.
    pedaco: (arquivo, inicio) => pedir('pedaco', { arquivo, inicio: Number(inicio) || 0 }),
    soltar: (arquivo) => pedir('soltar-arquivo', { arquivo }),
  });

  // ---- tempo ----
  //
  // O QuickJS não tem laço de eventos, então `setTimeout` e `setInterval` não
  // existem aqui. Quem tem relógio é a thread do anfitrião, e estas quatro
  // funções são a fachada dela.
  //
  // **Os pedidos vão por um vetor, e não por chamada nativa.** Uma função
  // nativa chamada de dentro do motor teria de travar a tabela enquanto o
  // motor roda — e o motor pode estar a meio de uma interrupção. O anfitrião
  // recolhe este vetor quando a volta termina.
  const relogios = new Map();
  let pedidosDeRelogio = [];
  let proximoRelogio = 0;

  // **O mesmo teto do anfitrião, aqui também.** Sem isto, a fachada guardava o
  // callback de um temporizador que o anfitrião tinha ignorado: `setTimeout`
  // devolvia um número, o MOD acreditava que estava agendado, e ele nunca
  // disparava. Zero é o «não deu», e é o que um MOD confere.
  const TETO = 256;
  const marcar = (fn, ms, repete) => {
    if (typeof fn !== 'function') return 0;
    if (relogios.size >= TETO) return 0;
    const id = ++proximoRelogio;
    relogios.set(id, { fn, repete });
    pedidosDeRelogio.push({ id, ms: Number(ms) || 0, repete, cancelar: false });
    return id;
  };
  const desmarcar = (id) => {
    if (!relogios.delete(id)) return;
    pedidosDeRelogio.push({ id, ms: 0, repete: false, cancelar: true });
  };

  globalThis.setTimeout = (fn, ms) => marcar(fn, ms, false);
  globalThis.setInterval = (fn, ms) => marcar(fn, ms, true);
  globalThis.clearTimeout = desmarcar;
  globalThis.clearInterval = desmarcar;

  // Chamado pelo anfitrião quando um temporizador vence.
  globalThis.__seeleRelogio = (id) => {
    const marca = relogios.get(id);
    if (!marca) return;
    if (!marca.repete) relogios.delete(id);
    // **O erro do MOD fica com o MOD.** Um `setInterval` que lança não pode
    // parar o anfitrião, e a repetição continua: quem decide parar é quem
    // cancela, e não um erro de uma volta.
    try { marca.fn(); } catch (erro) { seele.postar(JSON.stringify({ tipo: 'erro-no-relogio', erro: String(erro) })); }
  };

  // E o anfitrião recolhe o que se acumulou, esvaziando.
  globalThis.__seelePedidosDeRelogio = () => {
    const meus = pedidosDeRelogio;
    pedidosDeRelogio = [];
    return JSON.stringify(meus);
  };
})();
"#;

/// O que o MOD pediu de temporizador, como o prelúdio o escreve.
#[derive(Debug, serde::Deserialize)]
struct PedidoDeRelogio {
    /// O número que o prelúdio deu a este temporizador.
    id: u32,
    /// De quanto em quanto, em milissegundos.
    #[serde(default)]
    ms: f64,
    /// Repete, ou dispara uma vez só.
    #[serde(default)]
    repete: bool,
    /// É um cancelamento, e não um pedido.
    #[serde(default)]
    cancelar: bool,
}

/// O maior atraso que um temporizador pode pedir.
///
/// Vinte e quatro horas. Acima disso o pedido é **aparado** para cá em vez de
/// recusado: quem escreve um número enorme quer «nunca», e um dia é o mais
/// perto de nunca que faz sentido guardar numa tabela que morre com a sessão.
pub(crate) const ATRASO_MAXIMO: Duration = Duration::from_secs(24 * 60 * 60);

/// O atraso que o MOD pediu, em milissegundos, virado num `Duration` seguro.
///
/// Devolve nada para o que não é número — `NaN` reprova as duas comparações — e
/// apara o resto entre [`INTERVALO_MINIMO`] e [`ATRASO_MAXIMO`].
fn intervalo_valido(ms: f64) -> Option<Duration> {
    if !ms.is_finite() {
        return None;
    }
    let segundos = (ms.max(0.0) / 1000.0).min(ATRASO_MAXIMO.as_secs_f64());
    Some(Duration::from_secs_f64(segundos).clamp(INTERVALO_MINIMO, ATRASO_MAXIMO))
}

/// Quanto falta para o temporizador mais próximo, ou nada se não houver.
fn proximo_vencimento(
    relogios: &std::collections::BTreeMap<u32, Temporizador>,
) -> Option<Duration> {
    let agora = Instant::now();
    relogios
        .values()
        .map(|t| t.quando.saturating_duration_since(agora))
        .min()
}

/// Tira da tabela os que venceram, reagendando os que repetem.
fn recolher_vencidos(relogios: &mut std::collections::BTreeMap<u32, Temporizador>) -> Vec<u32> {
    let agora = Instant::now();
    let vencidos: Vec<u32> = relogios
        .iter()
        .filter(|(_, t)| t.quando <= agora)
        .map(|(id, _)| *id)
        .collect();
    for id in &vencidos {
        let Some(t) = relogios.get_mut(id) else {
            continue;
        };
        match t.repete {
            // Reagendado a partir de **agora**, e não do vencimento: somando ao
            // vencimento, um MOD que demora mais que o intervalo acumularia
            // disparos atrasados e a thread nunca mais dormiria.
            Some(intervalo) => t.quando = agora + intervalo,
            None => {
                relogios.remove(id);
            }
        }
    }
    vencidos
}

/// Lê o que o MOD pediu de temporizador nesta volta, e atualiza a tabela.
///
/// **Pelo estado, e não por callback nativo.** Uma função nativa chamada de
/// dentro do motor teria de travar a tabela enquanto o motor roda, e o motor
/// pode estar a meio de uma interrupção. O prelúdio anota os pedidos num
/// vetor, e o anfitrião o recolhe quando a volta termina.
fn recolher_pedidos_de_relogio(
    contexto: &Context,
    relogios: &mut std::collections::BTreeMap<u32, Temporizador>,
) {
    // Atravessa como texto JSON, e não como tupla: o binding sabe converter
    // `String`, e escrever um `FromJs` para uma forma que só este arquivo usa
    // seria código de conversão para não usar o conversor que já existe.
    let pedidos = contexto.with(|ctx| {
        ctx.globals()
            .get::<_, Function<'_>>("__seelePedidosDeRelogio")
            .and_then(|f| f.call::<_, String>(()))
            .unwrap_or_default()
    });
    let pedidos: Vec<PedidoDeRelogio> = serde_json::from_str(&pedidos).unwrap_or_default();
    for PedidoDeRelogio {
        id,
        ms,
        repete,
        cancelar,
    } in pedidos
    {
        if cancelar {
            relogios.remove(&id);
            continue;
        }
        if relogios.len() >= TEMPORIZADORES_DE_PE {
            continue;
        }
        // **O número vem do MOD, e `Duration::from_secs_f64` entra em pânico
        // fora do intervalo representável.** `1e300` é finito, passa por
        // qualquer conferência de «é número», e derruba a thread do motor — que
        // é derrubar o MOD de quem está na sessão por causa de um argumento.
        //
        // Aparado, e não recusado: quem escreve `setTimeout(f, 1e300)` quer
        // «nunca», e o teto é o mais perto de nunca que dá para representar sem
        // mentir. Um `NaN` também cai aqui, pela comparação que ele reprova.
        let Some(intervalo) = intervalo_valido(ms) else {
            continue;
        };
        // E somar ao relógio também é verificado: `Instant + Duration` entra em
        // pânico no estouro, e o teto acima o torna improvável — não impossível.
        let Some(quando) = Instant::now().checked_add(intervalo) else {
            continue;
        };
        relogios.insert(
            id,
            Temporizador {
                quando,
                repete: repete.then_some(intervalo),
            },
        );
    }
}

/// Escoa as microtarefas da volta, e **diz** quando uma é interrompida.
///
/// A primeira versão saía do laço em silêncio: o MOD era parado no meio de uma
/// microtarefa e ninguém ficava sabendo — nem a janela, nem quem hospeda, nem
/// quem escreveu o MOD. É o defeito que o `CLAUDE.md` deste repositório nomeia
/// como o mais caro daqui, cometido pelo próprio mecanismo de contenção.
fn escoar_jobs(
    runtime: &Runtime,
    interrupcao: &Arc<Interrupcao>,
    manda: &Sender<ParaOFora>,
    fila: &Arc<Fila>,
) {
    while runtime.is_job_pending() && !interrupcao.revogada() {
        if runtime.execute_pending_job().is_err() {
            avisar(manda, fila, ParaOFora::Interrompido);
            return;
        }
    }
}

/// Manda um aviso, **se ele couber na cota dele**.
///
/// Um MOD que lança num laço manda o mesmo aviso sem parar. Sem cota, ele enche
/// o canal entre o motor e a bomba — que nenhum teto de fila alcançava, porque
/// os avisos saíam sem reservar nada.
fn avisar(manda: &Sender<ParaOFora>, fila: &Arc<Fila>, aviso: ParaOFora) {
    if !fila.cabe_aviso() {
        return;
    }
    if manda.send(aviso).is_err() {
        fila.tirar_aviso();
    }
}

/// Traduz o resultado de uma volta para o canal de saída.
fn relatar(
    manda: &Sender<ParaOFora>,
    fila: &Arc<Fila>,
    interrupcao: &Arc<Interrupcao>,
    resultado: rquickjs::Result<()>,
) {
    match resultado {
        Ok(()) => {}
        Err(_) if interrupcao.revogada() => {
            avisar(manda, fila, ParaOFora::Interrompido);
        }
        // Uma interrupção chega como erro do motor, como qualquer outra
        // exceção. Distinguir as duas importa: uma é o MOD com defeito, a
        // outra é o produto parando o MOD, e elas pedem frases diferentes.
        Err(rquickjs::Error::Exception) => {
            avisar(manda, fila, ParaOFora::Falhou("o MOD lançou".into()));
        }
        Err(erro) => {
            avisar(manda, fila, ParaOFora::Interrompido);
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
#[cfg(test)]
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

    /// **A fila tem teto, e o MOD fica sabendo quando bate nele.**
    ///
    /// O teto de 12 KiB é da mensagem; este é do acumulado. Sem ele, um MOD num
    /// laço enche a memória de quem usa uma mensagem de cada vez, todas dentro
    /// do limite individual — e a fila cresce até o processo cair.
    #[test]
    fn a_fila_de_saida_para_de_crescer_e_o_mod_recebe_a_recusa() {
        let executor = executor();
        // Dez mil mensagens de um kibibyte: cento e poucos vezes o teto de
        // bytes, e mais de cem vezes o de quantidade.
        executor
            .iniciar(
                "let coube = 0;                  for (let i = 0; i < 10000; i++) { if (seele.postar('x'.repeat(1024))) coube++; }                  globalThis.__coube = coube;",
            )
            .expect("código");

        // Sem ler nada da fila: é assim que a saturação acontece de verdade —
        // o MOD escreve mais rápido do que a janela lê.
        std::thread::sleep(Duration::from_millis(300));
        let (mensagens, bytes) = executor.fila().ocupacao();
        assert!(
            mensagens <= MENSAGENS_NA_FILA,
            "a fila passou do teto de quantidade: {mensagens}"
        );
        assert!(
            bytes <= BYTES_NA_FILA,
            "a fila passou do teto de bytes: {bytes}"
        );
        assert!(
            executor.fila().recusadas() > 0,
            "nada foi recusado, então o teto não valeu"
        );
    }

    /// **E a fila cheia não prende o encerramento.**
    ///
    /// É a propriedade que separa uma fila com teto de uma fila que empurra o
    /// problema para outro lugar. Se `postar` esperasse por espaço, a thread do
    /// motor ficaria parada dentro de uma função nativa — e o tratador de
    /// interrupção não alcança função nativa, que é justamente o que a diretriz
    /// avisa.
    #[test]
    fn uma_fila_cheia_nao_prende_o_encerramento() {
        let mut executor = executor();
        executor
            .iniciar("while (true) { seele.postar('x'.repeat(4096)); }")
            .expect("código");
        // A fila enche em milissegundos, e o laço continua tentando.
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            executor.fila().recusadas() > 0,
            "a fila não chegou a encher, então o teste não mediu a saturação"
        );

        let levou = executor
            .encerrou(Duration::from_secs(10))
            .expect("a saturação prendeu o encerramento");
        assert!(
            levou < Duration::from_secs(5),
            "a saída levou {levou:?} com a fila cheia"
        );
    }

    /// E ler a fila devolve o lugar: a saturação é um estado, e não uma marca.
    #[test]
    fn ler_a_fila_devolve_o_lugar() {
        let executor = executor();
        executor
            .iniciar("for (let i = 0; i < 200; i++) seele.postar('x'.repeat(1024));")
            .expect("código");
        std::thread::sleep(Duration::from_millis(200));
        let (antes, _) = executor.fila().ocupacao();
        assert!(antes > 0, "a fila ficou vazia, e o teste não mede nada");

        let _ = executor.receber(Duration::from_secs(2));
        let (depois, _) = executor.fila().ocupacao();
        assert!(
            depois < antes,
            "ler não devolveu o lugar: {antes} antes, {depois} depois"
        );
    }

    /// Quanta memória residente este processo está usando, em KiB.
    ///
    /// Lida do sistema operacional, e não estimada: `ps` responde o que o
    /// gerente de tarefas responderia, que é a medida de que alguém reclama.
    /// Zero quando não deu para ler — e zero aparece no relatório em vez de
    /// virar um número inventado.
    /// **`None` é medição inválida, e não zero.**
    ///
    /// Converter uma leitura que falhou em zero faria o relatório dizer «este
    /// processo não usa memória», que é a coisa mais errada que ele poderia
    /// dizer — e ninguém saberia que o `ps` não respondeu.
    fn residente_kib() -> Option<u64> {
        campo_do_ps("rss=").and_then(|texto| texto.trim().parse().ok())
    }

    /// Quanto de CPU este processo já gastou, em centésimos de segundo.
    ///
    /// `ps` devolve `MM:SS.cc`. Centésimos porque é a resolução que ele dá — e
    /// dizer «dois centésimos» é mais honesto que converter para microssegundos
    /// um número que não os tem.
    fn cpu_centesimos() -> Option<u64> {
        let bruto = campo_do_ps("cputime=")?;
        let bruto = bruto.trim().to_owned();
        let (minutos, resto) = bruto.split_once(':').unwrap_or(("0", &bruto));
        let (segundos, centesimos) = resto.split_once('.').unwrap_or((resto, "0"));
        let m: u64 = minutos.trim().parse().ok()?;
        let s: u64 = segundos.parse().ok()?;
        let c: u64 = centesimos.parse().ok()?;
        Some((m * 60 + s) * 100 + c)
    }

    /// Um campo do `ps` sobre este processo, cru.
    fn campo_do_ps(campo: &str) -> Option<String> {
        std::process::Command::new("ps")
            .args(["-o", campo, "-p"])
            .arg(std::process::id().to_string())
            .output()
            .ok()
            .and_then(|saida| String::from_utf8(saida.stdout).ok())
    }

    /// **O custo de zero, uma e três instâncias** — medida da bancada.
    ///
    /// Não é um teste de aceite: ele não reprova por número, porque um número
    /// medido numa máquina não é um orçamento. Ele **imprime**, e o que decide
    /// é quem lê — a diretriz proíbe inventar um teto em MB sem medir, e este é
    /// o passo que dá o que medir.
    ///
    /// Rode com `--nocapture` para ver o relatório.
    ///
    /// O que ele mede, e o que ele não mede: memória residente do processo
    /// inteiro, tempo de subida e tempo de encerramento. **Não** mede impacto
    /// na voz nem latência de interação — isso precisa do aplicativo de
    /// verdade, com renderer e mídia, e está registrado como pendente.
    #[test]
    fn o_custo_de_zero_uma_e_tres_instancias() {
        // Um MOD que faz o que um MOD faz: guarda estado e responde.
        const TRABALHO: &str = "globalThis.n = 0;              globalThis.aoResponder = () => { globalThis.n++; seele.postar('ok'); };              seele.postar('pronto');";

        // Aquecido: a primeira alocação do processo não é o custo de ninguém.
        {
            let e = executor();
            e.iniciar(TRABALHO).expect("código");
            let _ = uma_mensagem(&e);
        }
        std::thread::sleep(Duration::from_millis(200));
        let base = residente_kib();

        let mut relatorio = vec![match base {
            Some(kib) => format!("sem instância: {kib} KiB"),
            None => "sem instância: MEDIÇÃO INVÁLIDA (o `ps` não respondeu)".to_owned(),
        }];
        for quantas in [1_usize, 3] {
            let subida = Instant::now();
            let mut instancias = Vec::new();
            for _ in 0..quantas {
                let e = executor();
                e.iniciar(TRABALHO).expect("código");
                assert_eq!(uma_mensagem(&e), "pronto");
                instancias.push(e);
            }
            let subiu_em = subida.elapsed();

            // Uma volta de trabalho em cada, para nenhuma estar apenas parada.
            for e in &instancias {
                e.entregar("{}").expect("entregar");
                assert_eq!(uma_mensagem(e), "ok");
            }
            let com = residente_kib();

            // **CPU ociosa.** Instâncias paradas não podem custar: um motor que
            // rodasse um laço de espera gastaria bateria por não estar fazendo
            // nada, e num aplicativo de voz isso disputa com o áudio.
            let cpu_antes = cpu_centesimos();
            std::thread::sleep(Duration::from_secs(1));
            let ociosa = match (cpu_antes, cpu_centesimos()) {
                // **Zero aqui é «nada observável nesta resolução»**, e não
                // «consumo nulo». O `ps` dá centésimos de segundo; o que cabe
                // abaixo disso não aparece, e afirmar que não existe seria
                // afirmar mais do que a medida diz.
                (Some(antes), Some(depois)) => {
                    format!("{} centésimos em 1 s", depois.saturating_sub(antes))
                }
                _ => "MEDIÇÃO INVÁLIDA".to_owned(),
            };

            let saida = Instant::now();
            for e in &mut instancias {
                e.encerrou(Duration::from_secs(5))
                    .expect("a instância tem de confirmar a parada");
            }
            let saiu_em = saida.elapsed();
            drop(instancias);

            let memoria = match (base, com) {
                (Some(base), Some(com)) => {
                    let sobre = i64::try_from(com).unwrap_or(0) - i64::try_from(base).unwrap_or(0);
                    let cada = (com as f64 - base as f64) / quantas as f64;
                    format!("{com} KiB ({sobre:+} KiB sobre a base, {cada:.0} KiB cada)")
                }
                _ => "MEDIÇÃO INVÁLIDA".to_owned(),
            };
            relatorio.push(format!(
                "{quantas} instância(s): {memoria} · subida {subiu_em:?} · CPU ociosa {ociosa} · encerramento {saiu_em:?}"
            ));
        }

        println!("---- custo do executor QuickJS ----");
        for linha in &relatorio {
            println!("{linha}");
        }
        println!(
            "medido em {} · `ps -o rss` e `-o cputime`, processo inteiro, uma coleta",
            std::env::consts::OS
        );
        println!(
            "**alcance**: binário de teste, sem bomba, sem WebView e sem mídia. \
             Não é o custo do produto, e não substitui a coleta integrada."
        );
    }

    /// Quantos temporizadores o anfitrião tem de pé por este MOD.
    fn relogios_de_pe(executor: &ExecutorQuickJs) -> usize {
        executor.diagnostico().expect("pedir diagnóstico");
        loop {
            match executor.receber(Duration::from_secs(2)) {
                Some(ParaOFora::Diagnostico { relogios }) => return relogios,
                Some(_) => {}
                None => panic!("o executor não respondeu ao diagnóstico"),
            }
        }
    }

    /// **Um timeout que agenda outro timeout dispara** — sem tráfego de fora.
    ///
    /// O ramo dos vencidos tinha um `continue` antes de recolher os pedidos, e
    /// o pedido do segundo timeout ficava parado até chegar uma mensagem
    /// externa. Num MOD que só usa relógio, ela pode nunca chegar.
    ///
    /// Este teste não manda nada depois de iniciar: se a corrente andar, foi
    /// porque a volta dos vencidos recolheu o pedido seguinte.
    #[test]
    fn um_timeout_que_agenda_outro_anda_sozinho() {
        let executor = executor();
        executor
            .iniciar(
                "let n = 0;                  const passo = () => { n++; seele.postar(String(n)); if (n < 3) setTimeout(passo, 5); };                  setTimeout(passo, 5);",
            )
            .expect("código");
        for esperado in ["1", "2", "3"] {
            assert_eq!(
                uma_mensagem(&executor),
                esperado,
                "a corrente de temporizadores parou"
            );
        }
    }

    /// **Um intervalo que se cancela para, e some da tabela do anfitrião.**
    ///
    /// Pelo mesmo `continue`: o cancelamento era anotado dentro do callback e
    /// ficava sem coleta, então o intervalo continuava batendo depois de o MOD
    /// tê-lo cancelado.
    #[test]
    fn um_intervalo_que_se_cancela_para_de_bater() {
        let executor = executor();
        executor
            .iniciar(
                "let n = 0;                  const id = setInterval(() => {                    n++; seele.postar(String(n));                    if (n === 2) clearInterval(id);                  }, 5);",
            )
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "1");
        assert_eq!(uma_mensagem(&executor), "2");
        // Se o cancelamento não tivesse sido recolhido, viria «3» em 5 ms.
        assert!(
            executor.receber(Duration::from_millis(400)).is_none(),
            "o intervalo continuou batendo depois de se cancelar"
        );
        // **E a tabela do anfitrião ficou vazia.** Sem esta linha, o teste
        // passaria também se o motor tivesse morrido: as duas coisas param de
        // mandar mensagem.
        assert_eq!(
            relogios_de_pe(&executor),
            0,
            "o intervalo cancelado continuou na tabela do anfitrião"
        );
    }

    /// **Um atraso absurdo não derruba a thread do motor.**
    ///
    /// `1e300` é finito, passa por qualquer conferência de «é número», e
    /// `Duration::from_secs_f64` entra em pânico com ele. Derrubar a thread por
    /// causa de um argumento é derrubar o MOD de quem está na sessão.
    #[test]
    fn um_atraso_absurdo_nao_derruba_o_motor() {
        let executor = executor();
        executor
            .iniciar(
                "setTimeout(() => seele.postar('absurdo'), 1e300);                  setTimeout(() => seele.postar('nan'), NaN);                  setTimeout(() => seele.postar('negativo'), -1);                  setTimeout(() => seele.postar('vivo'), 5);",
            )
            .expect("código");
        // `NaN` e `-1` viram zero na fachada, como num navegador, e zero vira o
        // piso de 4 ms — então os três curtos disparam. O de `1e300` é aparado
        // para um dia, e não chega.
        let mut chegaram = Vec::new();
        while let Some(ParaOFora::Mensagem(json)) = executor.receber(Duration::from_millis(400)) {
            chegaram.push(json);
        }
        chegaram.sort();
        assert_eq!(chegaram, vec!["nan", "negativo", "vivo"]);
        assert_eq!(
            relogios_de_pe(&executor),
            1,
            "o temporizador de um dia devia continuar na tabela, e só ele"
        );
    }

    /// E os atrasos aparados continuam dentro do que dá para representar.
    #[test]
    fn os_atrasos_sao_aparados_em_vez_de_estourar() {
        for (pedido, esperado) in [
            (0.0, Some(INTERVALO_MINIMO)),
            (-5.0, Some(INTERVALO_MINIMO)),
            (1.0, Some(INTERVALO_MINIMO)),
            (100.0, Some(Duration::from_millis(100))),
            (1e300, Some(ATRASO_MAXIMO)),
            (f64::INFINITY, None),
            (f64::NAN, None),
        ] {
            assert_eq!(intervalo_valido(pedido), esperado, "pedido de {pedido}");
        }
    }

    /// **O teto de temporizadores é dito a quem chamou.**
    ///
    /// A fachada guardava o callback de um temporizador que o anfitrião tinha
    /// ignorado: `setTimeout` devolvia um número, o MOD acreditava que estava
    /// agendado, e ele nunca disparava.
    #[test]
    fn o_teto_de_temporizadores_chega_a_quem_pediu() {
        let executor = executor();
        executor
            .iniciar(
                "let ultimo = 1;                  for (let i = 0; i < 300; i++) ultimo = setTimeout(() => {}, 60000);                  seele.postar(JSON.stringify({ ultimo }));",
            )
            .expect("código");
        let resposta: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("JSON");
        assert_eq!(
            resposta.get("ultimo").and_then(serde_json::Value::as_u64),
            Some(0),
            "o tricentésimo `setTimeout` devolveu um número, e ele nunca vai disparar"
        );
    }

    /// **A fila de entrada também tem teto, e ela não tinha.**
    ///
    /// `entregar` copiava e enfileirava o JSON sem limite. Uma janela que
    /// responde em rajada — ou um MOD que pede mais rápido do que o motor
    /// atende — fazia a memória crescer deste lado, onde nenhum teto de heap do
    /// motor conta.
    #[test]
    fn a_fila_de_entrada_para_de_crescer_e_diz_a_quem_entrega() {
        let executor = executor();
        // O MOD nem precisa existir: o que se mede é a fila antes do motor.
        // Um laço grande o bastante para passar dos dois tetos.
        let mut recusou = false;
        for _ in 0..10_000 {
            let corpo = format!(r#"{{"n":1,"ok":true,"enchimento":"{}"}}"#, "x".repeat(1024));
            if executor.entregar(&corpo).is_err() {
                recusou = true;
                break;
            }
        }
        assert!(
            recusou,
            "a fila de entrada aceitou dez mil respostas de um kibibyte sem recusar nenhuma"
        );
        let (mensagens, bytes) = executor.entrada().ocupacao();
        assert!(
            mensagens <= MENSAGENS_NA_FILA && bytes <= BYTES_NA_FILA,
            "a fila de entrada passou do teto: {mensagens} mensagens, {bytes} bytes"
        );
    }

    /// **Saturar a entrada não impede revogar nem confirmar a parada.**
    ///
    /// É a mesma propriedade que já vale para a saída, e a diretriz pede as
    /// duas: «preservar revogação e confirmação de parada quando houver
    /// saturação».
    #[test]
    fn a_entrada_cheia_nao_prende_o_encerramento() {
        let mut executor = so_com(SEM_TETO_DE_TRABALHO, SEM_PRAZO);
        // O MOD entra num laço para o motor não drenar a fila de entrada.
        executor
            .iniciar("seele.postar('comecei'); while (true) {}")
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "comecei");
        while executor.entregar(r#"{"n":1,"ok":true}"#).is_ok() {}

        let levou = executor
            .encerrou(Duration::from_secs(10))
            .expect("a saturação da entrada prendeu o encerramento");
        assert!(
            levou < Duration::from_secs(5),
            "a saída levou {levou:?} com a entrada cheia e o MOD em laço"
        );
    }

    /// **Um MOD que chama `seele.postar` direto também esbarra no teto.**
    ///
    /// A diretriz avisa: «não confiar no limite de oito pedidos do prelúdio: o
    /// autor pode chamar `seele.postar` diretamente». O teto de oito é uma
    /// conveniência do prelúdio, que é código do MOD; o que contém de verdade é
    /// a fila, que é nossa.
    #[test]
    fn o_teto_da_fila_nao_depende_do_preludio() {
        let executor = executor();
        executor
            .iniciar(
                "let coube = 0;                  for (let i = 0; i < 5000; i++) { if (seele.postar('x'.repeat(2048))) coube++; }                  globalThis.__coube = coube;",
            )
            .expect("código");
        std::thread::sleep(Duration::from_millis(300));
        let (mensagens, bytes) = executor.fila().ocupacao();
        assert!(
            mensagens <= MENSAGENS_NA_FILA && bytes <= BYTES_NA_FILA,
            "o teto foi ultrapassado por quem não usa o prelúdio: {mensagens}, {bytes}"
        );
        assert!(executor.fila().recusadas() > 0);
    }

    /// **Uma promessa que rejeita sem ninguém pegando é dita.**
    ///
    /// Medido no aplicativo nativo antes de ser escrito aqui: o MOD de
    /// referência chamava `SeeleMods.snapshot()` numa função `async` sem
    /// `catch`, e quando a sessão ainda não tinha subido o `snapshot` rejeitava
    /// — o MOD morria ali, na segunda linha, **sem uma palavra**. No registro
    /// aparecia como duas falas e silêncio; na tela, como um MOD que não
    /// desenha. Levei três execuções para separar isso de um travamento.
    ///
    /// É o defeito que o `CLAUDE.md` deste repositório nomeia como o mais caro:
    /// o produto sabe e não conta. O motor **sabe** — QuickJS rastreia a
    /// rejeição sem tratamento —, e quem escreveu o MOD é quem precisa saber.
    #[test]
    fn uma_promessa_rejeitada_sem_tratamento_nao_morre_calada() {
        let executor = executor();
        executor
            .iniciar(
                "globalThis.aoResponder = () => {};\
                 (async () => { throw new Error('ninguém me pega'); })();",
            )
            .expect("código");

        let prazo = std::time::Instant::now();
        let mut dito = None;
        while prazo.elapsed() < Duration::from_secs(3) {
            match executor.receber(Duration::from_millis(100)) {
                Some(ParaOFora::Falhou(motivo)) => {
                    dito = Some(motivo);
                    break;
                }
                Some(_) => {}
                None => {}
            }
        }
        let motivo = dito.expect(
            "um MOD cuja promessa rejeitou sem tratamento sumiu sem dizer nada — \
             é exatamente assim que ele desaparece na máquina de quem o instalou",
        );
        assert!(
            motivo.contains("ninguém me pega"),
            "o aviso não leva o que o MOD disse: {motivo}"
        );
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
