//! A volta de um MOD da janela: o motor QuickJS, sem thread e sem E/S.
//!
//! Até 09/10/2026 isto era o miolo de `apps/seele-app/src/executor.rs`, e o
//! celular tinha uma cópia menor dele (`crates/seele-motor-web`), que não sabia
//! das rejeições, do balde do `console`, dos tetos de temporizador nem da frase
//! do teto que parou a volta. **Uma volta só, para os dois.** O que muda entre
//! eles é quem espera:
//! - o desktop embrulha o [`Motor`] numa thread, com o `mpsc` e o
//!   `recv_timeout` de sempre (`executor.rs`, `ExecutorQuickJs`);
//! - o celular o embrulha num Worker, dentro de um `iframe` opaco, com o
//!   relógio do navegador (`seele-motor-web`, ADR 0057).
//!
//! # O que este crate não tem
//!
//! **Thread, E/S e `unsafe`.** A saída é um `Sender<ParaOFora>`, que quem
//! embrulha lê; o tempo é o `Instant`, que no `wasm32-wasip1` vem do relógio da
//! WASI. E ele não conhece o manifesto nem o protocolo: as capacidades de uma
//! API entram como lista, calculadas por quem embrulha.
//!
//! # O que o código de um MOD alcança
//!
//! Duas funções, `seele.postar` e `seele.console`, e o que o [`PRELUDIO`]
//! monta sobre elas. Não há disco, rede, IPC nem armazenamento — e a ausência
//! não é uma jaula construída com cuidado: é o que um contexto de QuickJS
//! **é** antes de alguém acrescentar coisas a ele.

#![forbid(unsafe_code)]
// A doc dos itens públicos conta o miolo — o rastreador, o balde, o `relatar` —,
// que é privado de propósito: quem embrulha o motor não o chama. Os links
// apontam para lá, e `cargo doc --document-private-items` os mostra.
#![allow(rustdoc::private_intra_doc_links)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rquickjs::{Context, Function, Runtime};

mod preludio;
pub use preludio::PRELUDIO;

/// **O teto de um pedaço de texto de terceiro numa linha do `seele.log`**,
/// contado em caracteres **como a linha os leva**.
///
/// Quinhentos e doze caracteres, o número que `registrar_da_janela` (no
/// `main.rs` do desktop) já usava escrito à mão. Nomeado quando o `console` dos
/// MODs passou a escrever no mesmo arquivo: as portas põem lá texto que um
/// terceiro escolheu, e o teto de uma não pode divergir do da outra por
/// esquecimento.
///
/// **O mesmo em todas as portas, e é o tamanho escrito.** As quatro que
/// escrevem o texto escapado pelo `Debug` de um `str` — a frase da janela
/// (`registrar_da_janela`), o `console` do executor, o motivo
/// (`motivo_no_registro`) e o caminho (`caminho_no_registro`) — cortam pelo
/// tamanho já escapado, numa regra só ([`ate_o_teto_do_registro`]): o escape
/// pode fazer um caractere ocupar até dez (`\u{100000}`), e um corte contado
/// antes dele deixava quinhentos e doze desses virarem 5122 caracteres na
/// linha, com as aspas. A frase da janela e o `console` levam também o aviso
/// do corte ([`cortar_linha_do_console`]).
///
/// Mora aqui desde que a volta saiu do `executor.rs` (09/10/2026), porque o
/// corte do `console` é dela.
pub const TETO_DA_FRASE_NO_REGISTRO: usize = 512;

/// Quanta memória o motor pode alocar para um MOD.
///
/// **Não é o orçamento do MOD**, e a diretriz cobra a distinção: «o teto de
/// heap do QuickJS não limita a memória total do MOD». Buffers nativos, filas e
/// recursos visuais são contados noutros lugares e têm tetos próprios.
///
/// Oito mebibytes é o mesmo número que a metade de servidor usa. Ele está aqui
/// como ponto de partida a medir, e não como decisão: a carga de uma interface
/// não é a de um handler de pedido.
pub const TETO_DE_MEMORIA: usize = 8 * 1024 * 1024;

/// Quanto cabe numa mensagem que o MOD manda para a janela.
///
/// O mesmo número do quadro de controle. Ele é **nomeado** desde a validação
/// nativa de 20/09/2026, e a razão está no defeito que ela encontrou: o ESTILO
/// mandava 14.164 bytes num `superficie-montar`, o `postar` devolvia `false`, e
/// o prelúdio traduzia qualquer `false` para `fila-cheia`. A página nascia
/// vazia e a frase na tela falava de saturação — que não era o que estava
/// acontecendo, e mandava procurar no lugar errado.
///
/// Agora o prelúdio o recebe e mede antes de postar, para poder dizer o que de
/// fato aconteceu. A conferência daqui continua: o prelúdio roda **dentro** do
/// contexto do MOD, e um MOD pode redefinir o que quiser depois dele.
pub const TETO_DA_MENSAGEM: usize = 12 * 1024;

/// Quantas consultas do motor cabem numa volta de execução.
///
/// **Isto não é um prazo**, e é a armadilha que a diretriz nomeia: «só contar
/// consultas do motor não estabelece prazo real de saída». Uma função nativa
/// que bloqueia não consulta o motor, e este contador nunca a vê. Ele é o teto
/// de trabalho; o prazo é o campo ao lado dele em [`Interrupcao`].
pub const CONSULTAS_POR_VOLTA: usize = 200_000;

/// Quantas mensagens do MOD cabem esperando a janela lê-las.
///
/// **Não é o mesmo teto da mensagem.** Uma mensagem tem 12 KiB, que é o do
/// quadro de controle; este é o do **acumulado**, e sem ele um MOD num laço
/// enche a memória de quem usa uma mensagem de cada vez, todas dentro do
/// limite individual.
pub const MENSAGENS_NA_FILA: usize = 64;

/// Quantos bytes cabem na mesma fila.
///
/// Existe **ao lado** do teto de quantidade porque as duas saturações são
/// diferentes: sessenta e quatro mensagens de 12 KiB são 768 KiB, e um milhão
/// de mensagens de dez bytes é uma inundação que a contagem pegaria e os bytes
/// não, e vice-versa. Quem enche primeiro fecha a porta.
pub const BYTES_NA_FILA: usize = 256 * 1024;

/// Quantos avisos de diagnóstico cabem esperando, por instância.
///
/// **Cota própria, e pequena.** Um aviso nasce de uma falha, e uma falha não
/// pode consumir a cota do que está funcionando — nem ficar de fora de cota
/// nenhuma, que era o caso: `Falhou` e `Interrompido` saíam do motor sem
/// reservar nada, e um MOD que lança num laço enchia o canal sozinho.
pub const AVISOS_NA_FILA: usize = 16;

/// A fila de saída, medida.
///
/// **Nada aqui bloqueia.** Uma fila que faz quem escreve esperar seria uma fila
/// que prende a thread do motor — e prender a thread do motor é prender o
/// encerramento, que é justamente o que não pode acontecer. Cheia, ela recusa e
/// conta; o MOD recebe `false` e fica sabendo, e quem hospeda lê o contador.
#[derive(Debug, Default)]
pub struct Fila {
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
    pub fn cabe(&self, quantos: usize) -> bool {
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
    pub fn cabe_aviso(&self) -> bool {
        if self.avisos.load(Ordering::Acquire) >= AVISOS_NA_FILA {
            self.avisos_recusados.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.avisos.fetch_add(1, Ordering::AcqRel);
        true
    }

    /// Um aviso saiu da fila.
    pub fn tirar_aviso(&self) {
        let _ = self
            .avisos
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                Some(n.saturating_sub(1))
            });
    }

    /// Quantos avisos não couberam.
    #[must_use]
    pub fn avisos_recusados(&self) -> usize {
        self.avisos_recusados.load(Ordering::Relaxed)
    }

    /// Uma mensagem saiu da fila.
    pub fn tirar(&self, quantos: usize) {
        self.tirar_varios(1, quantos);
    }

    /// Várias saíram de uma vez — é o que a colheita faz.
    pub fn tirar_varios(&self, mensagens: usize, bytes: usize) {
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
    pub fn recusadas(&self) -> usize {
        self.recusadas.load(Ordering::Relaxed)
    }

    /// Quanto está esperando ser lido, em mensagens e em bytes.
    #[must_use]
    pub fn ocupacao(&self) -> (usize, usize) {
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
    pub fn avisos_de_pe(&self) -> usize {
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
pub struct Limites {
    /// Quanto o motor pode alocar. Não é o orçamento do MOD.
    pub memoria: usize,
    /// Quantas consultas do motor cabem numa volta.
    pub consultas_por_volta: usize,
    /// Quanto tempo uma volta pode levar.
    pub prazo_por_volta: Duration,
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
///
/// **E ela guarda qual teto disse sim** ([`Teto`]). O QuickJS faz da
/// interrupção uma exceção que o MOD não consegue pegar — `InternalError:
/// interrupted`, marcada como não capturável, que atravessa todo `catch` dele
/// — e que chega ao anfitrião como qualquer outra. Sem isto, a volta que o
/// produto parou chegava ao `seele.log` como «o MOD lançou» — o produto sabia
/// que tinha sido ele, e por qual teto, e não contava.
#[derive(Debug)]
pub struct Interrupcao {
    revogado: AtomicBool,
    consultas: AtomicUsize,
    /// Quando esta volta começou. Guardado em milissegundos desde o início do
    /// executor porque `Instant` não cabe num atômico.
    comeco: std::sync::Mutex<Option<Instant>>,
    /// Qual teto parou a volta: [`SEM_TETO`], [`PARADA_PELO_PRAZO`] ou
    /// [`PARADA_PELO_TRABALHO`]. A revogação não entra aqui: ela tem o seu
    /// próprio sinal, e não é teto.
    parou_por: AtomicU8,
    limites: Limites,
}

/// Nenhum teto parou a volta.
const SEM_TETO: u8 = 0;
/// O prazo parou a volta.
const PARADA_PELO_PRAZO: u8 = 1;
/// O teto de trabalho parou a volta.
const PARADA_PELO_TRABALHO: u8 = 2;

/// **O teto que parou uma volta** — e não a revogação, que é a sessão
/// acabando, e não o MOD passando de nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Teto {
    /// A volta passou do prazo ([`Limites::prazo_por_volta`]).
    Prazo,
    /// A volta passou do trabalho ([`Limites::consultas_por_volta`]).
    Trabalho,
}

impl Interrupcao {
    /// Uma interrupção com os três tetos de uma volta.
    #[must_use]
    pub fn nova(limites: Limites) -> Self {
        Self {
            revogado: AtomicBool::new(false),
            consultas: AtomicUsize::new(0),
            comeco: std::sync::Mutex::new(None),
            parou_por: AtomicU8::new(SEM_TETO),
            limites,
        }
    }

    /// A sessão acabou. **Monotônico**: nada volta a ser admitido.
    pub fn revogar(&self) {
        self.revogado.store(true, Ordering::Release);
    }

    /// Foi revogada?
    #[must_use]
    pub fn revogada(&self) -> bool {
        self.revogado.load(Ordering::Acquire)
    }

    /// Começa uma volta: zera o trabalho, esquece o teto da volta anterior e
    /// marca a hora.
    pub fn comecar(&self) {
        self.consultas.store(0, Ordering::Relaxed);
        self.parou_por.store(SEM_TETO, Ordering::Relaxed);
        if let Ok(mut comeco) = self.comeco.lock() {
            *comeco = Some(Instant::now());
        }
    }

    /// O que o motor pergunta a cada tanto. Quando um teto diz sim, ele fica
    /// guardado para [`Self::tirar_teto`].
    pub fn deve_parar(&self) -> bool {
        if self.revogada() {
            return true;
        }
        if self.consultas.fetch_add(1, Ordering::Relaxed) > self.limites.consultas_por_volta {
            self.parou_por
                .store(PARADA_PELO_TRABALHO, Ordering::Relaxed);
            return true;
        }
        // O relógio não é lido a cada consulta: `Instant::now` num laço apertado
        // custa mais que o trabalho que ele mede.
        if !self.consultas.load(Ordering::Relaxed).is_multiple_of(1024) {
            return false;
        }
        let passou = self
            .comeco
            .lock()
            .ok()
            .and_then(|c| *c)
            .is_some_and(|inicio| inicio.elapsed() > self.limites.prazo_por_volta);
        if passou {
            self.parou_por.store(PARADA_PELO_PRAZO, Ordering::Relaxed);
        }
        passou
    }

    /// **O teto que parou a volta, tirado**: quem o lê o consome.
    ///
    /// Tirado, e não só lido, porque um teto lido não pode atribuir a parada a
    /// uma exceção seguinte da mesma volta. [`Motor::vencer`] dispara os
    /// temporizadores vencidos numa volta só, com um [`Self::comecar`] para
    /// todos, e [`relatar`] lê o teto depois de cada um. O `TypeError` comum de
    /// um temporizador não chega lá — o `catch` do prelúdio o pega e o diz como
    /// «erro num temporizador» —, mas uma exceção de verdade ainda chega: a de
    /// um valor lançado cujo `toString` lança dentro daquele `catch`, ou a de
    /// um `__seeleRelogio` que o MOD trocou pelo dele. Lido sem tirar, o teto
    /// de um temporizador anterior, parado pelo prazo, diria a parada dessa
    /// exceção, e o texto dela sumiria.
    pub fn tirar_teto(&self) -> Option<Teto> {
        match self.parou_por.swap(SEM_TETO, Ordering::Relaxed) {
            PARADA_PELO_PRAZO => Some(Teto::Prazo),
            PARADA_PELO_TRABALHO => Some(Teto::Trabalho),
            _ => None,
        }
    }

    /// **A frase que diz a parada no `seele.log`**, para quem escreveu o MOD:
    /// qual teto — o prazo com os milissegundos que ele tem nesta instância; o
    /// de trabalho sem número, porque uma contagem de consultas do motor não
    /// diz nada a quem escreve JavaScript — e o que costuma levar até ele. Sem
    /// «lançou»: quem parou foi o produto.
    ///
    /// **E o que parou foi a volta, e não o MOD.** A janela recebe `Falhou` ou
    /// `Interrompido`, que não encerram a instância: o MOD segue de pé,
    /// recebendo eventos e temporizadores. Uma frase que dissesse «parou o
    /// MOD» mandaria quem lê procurar por que ele morreu.
    fn frase_do_teto(&self, teto: Teto) -> String {
        let qual = match teto {
            Teto::Prazo => format!(
                "do prazo de {} ms",
                self.limites.prazo_por_volta.as_millis()
            ),
            Teto::Trabalho => "do teto de trabalho".to_owned(),
        };
        format!(
            "esta volta do MOD passou {qual}, e o produto a interrompeu (o MOD continua de pé): \
             um laço que não sai, ou trabalho demais para uma volta só"
        )
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
pub const TEMPORIZADORES_DE_PE: usize = 256;

/// O menor intervalo que um temporizador pode pedir.
///
/// Quatro milissegundos é o piso que os navegadores usam, e ele existe pela
/// mesma razão: um temporizador de zero em laço vira espera ocupada, e espera
/// ocupada num produto de voz disputa com o áudio.
pub const INTERVALO_MINIMO: Duration = Duration::from_millis(4);

/// Quantas linhas de `console` um MOD escreve de uma vez, antes de o balde secar.
///
/// Trinta e duas: o que um MOD escreve ao subir, contando o que deu errado,
/// cabe inteiro. O que passa disso numa volta é um laço, e um laço no
/// `console` é uma linha no `seele.log` por volta — num arquivo que só gira na
/// abertura do app.
pub const RAJADA_DO_CONSOLE: u32 = 32;

/// Quantas linhas por segundo o balde devolve depois de secar.
///
/// Quatro: um MOD que reclama a cada evento continua sendo ouvido, e um que
/// reclama a cada quadro escreve no pior caso quatro linhas por segundo, cada
/// uma cortada em 512 caracteres **do jeito que ficam no registro** — o corte
/// pelo tamanho escapado ([`cortar_linha_do_console`]) mantém esse teto
/// qualquer que seja o texto.
///
/// Em bytes por hora, o número é **calculado, e não medido** no aplicativo; o
/// tamanho de uma linha foi medido com o formatador do `seele.log`, num teste.
/// Uma linha de ASCII cortada tem perto de 620 bytes, e quatro por segundo são
/// perto de nove megabytes por hora. Um caractere que se imprime passa sem
/// escape e pode ter até quatro bytes: uma linha só deles tem perto de 2,2 KB,
/// e a conta chega a cerca de trinta megabytes por hora — o pior caso, e um
/// teto, onde antes não havia nenhum.
pub const LINHAS_DE_CONSOLE_POR_SEGUNDO: u32 = 4;

/// Uma ficha do balde, em milionésimos.
///
/// A recarga é microssegundos vezes linhas por segundo, e nesta unidade ela é
/// uma multiplicação de inteiros: nenhuma fração se perde entre duas chamadas
/// próximas.
const FICHA: u64 = 1_000_000;

// **A vazão divide a ficha.** É o que faz as fichas andarem em múltiplos dela,
// e o prazo de [`BaldeDoConsole::ate_sobrar_uma_ficha`] cair exato no
// microssegundo em que elas se completam: uma vazão que não dividisse a ficha
// deixaria o prazo depender do arredondamento.
const _: () = assert!(FICHA.is_multiple_of(LINHAS_DE_CONSOLE_POR_SEGUNDO as u64));

/// **O balde de fichas do `console` de uma instância.**
///
/// Cheio, ele deixa passar [`RAJADA_DO_CONSOLE`] linhas de uma vez; seco, uma a
/// cada `1 / LINHAS_DE_CONSOLE_POR_SEGUNDO` segundo. O que ele segura **é
/// contado**, e a primeira linha que passa depois leva a conta: o `seele.log`
/// diz que houve mais do que ele mostra, em vez de mostrar pouco como se fosse
/// tudo. Quando nenhuma passa, o produto diz a conta ele mesmo, e só com uma
/// ficha de sobra além da que fica com o MOD: ao fim de uma volta do motor
/// ([`Self::pendentes_se_couber`]), ou quando a espera de quem embrulha o [`Motor`]
/// acorda a thread para isso, se nenhuma volta vier antes
/// ([`Self::ate_sobrar_uma_ficha`]). Ou, sem regra de ficha, quando a
/// instância para (ver [`dizer_as_seguradas`]).
///
/// Mora na thread do motor, dividido entre a função nativa, o rastreador de
/// rejeições e a volta do [`Motor`], e o relógio é passado por quem chama — é
/// o que deixa o teste medi-lo sem dormir. Ele também é o que limita o canal
/// entre o motor e a bomba para esta variante: a linha de console não reserva
/// cota de fila, e a vazão dela é o teto.
#[derive(Debug)]
pub struct BaldeDoConsole {
    /// As fichas, em milionésimos de ficha.
    fichas: u64,
    /// Até onde a recarga já foi contada.
    contado_ate: Instant,
    /// Quantas linhas foram seguradas desde a última que passou.
    suprimidas: u32,
}

impl BaldeDoConsole {
    /// Um balde cheio.
    #[must_use]
    pub fn novo(agora: Instant) -> Self {
        Self {
            fichas: u64::from(RAJADA_DO_CONSOLE) * FICHA,
            contado_ate: agora,
            suprimidas: 0,
        }
    }

    /// Uma linha quer passar.
    ///
    /// `Some(n)` é «passa», com quantas foram seguradas antes dela; `None` é
    /// «segurada», e ela entra na conta da próxima.
    pub fn admitir(&mut self, agora: Instant) -> Option<u32> {
        if !self.gastar_ficha(agora) {
            self.suprimidas = self.suprimidas.saturating_add(1);
            return None;
        }
        Some(std::mem::take(&mut self.suprimidas))
    }

    /// **A conta que nenhuma linha levou, ao fim de uma volta do motor — só
    /// com uma ficha de sobra além da que fica com o MOD.**
    ///
    /// A linha que diz a conta é uma linha no `seele.log` como qualquer outra,
    /// e por isso gasta ficha: sem ela, um MOD que escreve um pouco a cada
    /// volta ganharia uma linha do produto por volta, e o teto de vazão
    /// deixaria de valer. Sem duas fichas, ou sem nada segurado, devolve
    /// `None` e não gasta nada — a conta espera a próxima volta, que a espera
    /// do laço garante ([`Self::ate_sobrar_uma_ficha`]), ou [`Self::pendentes`].
    ///
    /// **Duas fichas em toda volta, a regra da espera.** A conta gasta uma, e
    /// a outra é da próxima linha do MOD. Com uma só, a conta tomava a vez
    /// dele por dois caminhos, os dois medidos nas re-revisões do lote D:
    /// - numa volta em que o MOD não escreve (um evento, uma resposta), ela
    ///   dizia a conta com a ficha que achasse — eventos a cada 30 ms e o MOD
    ///   escrevendo a cada 100 ms davam 1 linha dele e 14 da conta em quatro
    ///   segundos, desde antes do lote;
    /// - numa volta em que o MOD escreveu, foi segurado e seguiu trabalhando,
    ///   a ficha que se completava durante o trabalho era tomada pela conta —
    ///   um `setInterval` de 100 ms que escreve e trabalha 120 ms dava 0 linhas
    ///   dele e 15 da conta.
    pub fn pendentes_se_couber(&mut self, agora: Instant) -> Option<u32> {
        if self.suprimidas == 0 {
            return None;
        }
        self.recarregar(agora);
        if self.fichas < 2 * FICHA {
            return None;
        }
        self.fichas -= FICHA;
        Some(std::mem::take(&mut self.suprimidas))
    }

    /// **A conta que nenhuma linha levou, quando a instância para — com ficha
    /// ou sem.**
    ///
    /// É a última linha da instância, uma vez só: não há vazão a proteger, e
    /// guardá-la para uma ficha que não vai chegar era perdê-la.
    pub fn pendentes(&mut self) -> Option<u32> {
        (self.suprimidas > 0).then(|| std::mem::take(&mut self.suprimidas))
    }

    /// **Quanto falta para sobrar uma ficha — a da conta, além da que fica com
    /// o MOD —, enquanto há linha segurada.**
    ///
    /// É um dos prazos de [`Motor::proximo_prazo`]: sem temporizador e sem
    /// mensagem, a thread do motor dormia no canal, e a conta ficava no balde
    /// até o MOD parar. `None` sem nada segurado: a thread não acorda à toa.
    ///
    /// **Duas fichas, e não uma.** Acordar quando a primeira se completava
    /// fazia a linha da conta gastá-la, e a linha seguinte do MOD achava o
    /// balde seco: segurada, ela pedia outra conta na ficha seguinte, e o ciclo
    /// se repetia. Medido na revisão do lote D: depois de uma rajada, um
    /// `setInterval` de 100 ms deixava em quatro segundos 1 linha do MOD e 14 da
    /// conta. Com duas, a primeira ficha fica com o MOD por um período inteiro
    /// (`1 / LINHAS_DE_CONSOLE_POR_SEGUNDO` segundo): quem escreve nesse ritmo
    /// ou mais a gasta antes de juntar a segunda, e leva a conta na própria
    /// linha — o produto não fala. Quem se calou tem a conta em até dois
    /// períodos, meio segundo, e ainda acha uma ficha quando voltar a escrever.
    ///
    /// O fim de toda volta pede as mesmas duas, pela mesma razão
    /// ([`Self::pendentes_se_couber`]). Por isso a regra é uma só, e diz a
    /// verdade em qualquer caminho: quem tenta escrever ao menos uma linha a
    /// cada período nunca deixa juntar duas fichas, e o produto não fala.
    ///
    /// Arredondado para cima, ao microssegundo, e exato: as fichas andam em
    /// múltiplos da vazão, o que o `assert` junto de [`FICHA`] prende na
    /// compilação. Quem acorda no prazo acha as duas fichas inteiras, e não uma
    /// fração que o mandaria dormir de novo.
    ///
    /// Só lê: a recarga é contada por quem gasta a ficha ([`Self::recarregar`]).
    pub fn ate_sobrar_uma_ficha(&self, agora: Instant) -> Option<Duration> {
        if self.suprimidas == 0 {
            return None;
        }
        let passou = agora.saturating_duration_since(self.contado_ate);
        let micros = u64::try_from(passou.as_micros()).unwrap_or(u64::MAX);
        let por_micro = u64::from(LINHAS_DE_CONSOLE_POR_SEGUNDO);
        let fichas = self.fichas.saturating_add(micros.saturating_mul(por_micro));
        let falta = (2 * FICHA).saturating_sub(fichas);
        Some(Duration::from_micros(falta.div_ceil(por_micro)))
    }

    /// Conta a recarga até `agora` e tira uma ficha, se houver.
    fn gastar_ficha(&mut self, agora: Instant) -> bool {
        self.recarregar(agora);
        if self.fichas < FICHA {
            return false;
        }
        self.fichas -= FICHA;
        true
    }

    /// Conta a recarga até `agora`, sem passar do balde cheio.
    fn recarregar(&mut self, agora: Instant) {
        let passou = agora.saturating_duration_since(self.contado_ate);
        let micros = u64::try_from(passou.as_micros()).unwrap_or(u64::MAX);
        // O relógio anda só o que foi contado: o resto abaixo de um
        // microssegundo fica para a próxima chamada, e um laço apertado não
        // perde a recarga inteira por arredondamento.
        self.contado_ate = self
            .contado_ate
            .checked_add(Duration::from_micros(micros))
            .unwrap_or(agora);
        let cheio = u64::from(RAJADA_DO_CONSOLE) * FICHA;
        let recarga = micros.saturating_mul(u64::from(LINHAS_DE_CONSOLE_POR_SEGUNDO));
        self.fichas = self.fichas.saturating_add(recarga).min(cheio);
    }
}

/// O `texto` da linha em que o produto diz a conta do balde, quando nenhuma
/// linha do MOD passou depois para levá-la.
///
/// Entre colchetes, como o aviso do corte e o recuo de `emTexto`: é o produto
/// falando dentro do `texto` de uma linha de console, e não o MOD. A conta vai
/// no campo `suprimidas`, como em qualquer outra linha.
pub const CONTA_DO_BALDE: &str =
    "[linhas de console seguradas pelo teto, sem outra linha depois que as contasse]";

/// **Diz a conta que o balde guardou e nenhuma linha levou.**
///
/// `tirar` é a regra da hora: [`BaldeDoConsole::pendentes_se_couber`] ao fim de
/// cada volta do motor, e [`BaldeDoConsole::pendentes`] quando a instância
/// para. Sem isto, um MOD que escreve uma rajada e se cala — ou é parado —
/// deixava a conta no balde, e ela caía com o contexto: o `seele.log` mostrava
/// as linhas da rajada como se fossem tudo.
///
/// Um MOD que não tem volta nenhuma depois da rajada — nem temporizador, nem
/// resposta — tem a conta dita quando sobra uma ficha além da que fica com
/// ele, em até meio segundo, sem esperar parar: a espera de quem embrulha
/// acorda para isso ([`BaldeDoConsole::ate_sobrar_uma_ficha`]). Ele tinha a
/// conta dita só quando parava, e numa sessão longa isso era o `seele.log`
/// mostrando a rajada como se fosse tudo por horas.
fn dizer_as_seguradas(
    balde: &std::cell::RefCell<BaldeDoConsole>,
    manda: &Sender<ParaOFora>,
    tirar: impl FnOnce(&mut BaldeDoConsole) -> Option<u32>,
) {
    // Entre duas voltas nenhum JavaScript roda, e a ponte não tem o balde
    // emprestado; um empréstimo que falhasse deixa a conta onde está, para a
    // próxima chamada.
    let Ok(mut balde) = balde.try_borrow_mut() else {
        return;
    };
    let Some(suprimidas) = tirar(&mut balde) else {
        return;
    };
    let _ = manda.send(ParaOFora::Console {
        nivel: NivelDoConsole::Aviso,
        texto: CONTA_DO_BALDE.to_owned(),
        suprimidas,
    });
}

/// **Uma linha de `console` sai do motor**: pelo balde, cortada, e para o
/// canal que a bomba escreve no `seele.log` com o id do MOD.
///
/// O caminho de toda linha que o MOD escreve com `seele.console` e da que o
/// produto diz por ele quando uma volta lança ou passa de um teto
/// ([`relatar`] e [`escoar_jobs`]), ou quando uma promessa rejeita sem que
/// ninguém a pegue ([`dizer_as_rejeitadas`], e o rastreador de [`Motor::subir`] na
/// reentrada em que a espera não pode ser emprestada), e é por ser um só que o
/// teto vale para todas elas. Menos a conta do balde, que
/// [`dizer_as_seguradas`] põe no canal por fora daqui: ao fim de uma volta ela
/// gasta a ficha dela no próprio balde, e na parada é a última linha da
/// instância.
fn linha_de_console(
    balde: &std::cell::RefCell<BaldeDoConsole>,
    manda: &Sender<ParaOFora>,
    nivel: NivelDoConsole,
    texto: String,
) {
    linha_ja_cortada(balde, manda, nivel, cortar_linha_do_console(texto));
}

/// [`linha_de_console`] para a linha que já foi cortada: a que esperou o fim
/// da volta em [`RejeicoesDaVolta`], cortada quando entrou na espera. Pelo
/// mesmo balde, sem cortar de novo.
fn linha_ja_cortada(
    balde: &std::cell::RefCell<BaldeDoConsole>,
    manda: &Sender<ParaOFora>,
    nivel: NivelDoConsole,
    linha: LinhaCortada,
) {
    // Um `RefCell` e não um cadeado: o balde só é usado na thread do motor, e
    // um `try_borrow` que falhasse seria uma reentrada que não existe — calar
    // é a resposta segura se um dia existir.
    let Ok(mut balde) = balde.try_borrow_mut() else {
        return;
    };
    let Some(suprimidas) = balde.admitir(Instant::now()) else {
        return;
    };
    let _ = manda.send(ParaOFora::Console {
        nivel,
        texto: linha.0,
        suprimidas,
    });
}

/// **Uma linha de `console` já no teto do registro**, como ela vai ao canal.
///
/// Só [`cortar_linha_do_console`] a faz. Um tipo, e não uma `String`, para a
/// linha que espera o fim da volta ([`RejeicoesDaVolta`]) ser cortada uma vez
/// só, quando entra na espera: cortada de novo na saída, ela perderia o aviso
/// do primeiro corte, que diz o tamanho do que o MOD lançou, e ganharia outro
/// com o tamanho já cortado.
///
/// Visível no crate porque a frase da janela (`registrar_da_janela`, em
/// `main.rs`) é cortada por aqui também; o campo continua privado, e quem está
/// fora do executor só a lê ([`LinhaCortada::texto`]).
#[derive(Debug)]
pub struct LinhaCortada(String);

impl LinhaCortada {
    /// O texto já cortado, com o aviso do corte quando houve um.
    pub fn texto(&self) -> &str {
        &self.0
    }
}

/// O nível de uma linha que o MOD escreveu com `console`.
///
/// Cinco métodos, quatro níveis: `log` e `info` são a mesma coisa para quem lê
/// o registro, e separá-los aqui inventaria uma distinção que o autor do MOD
/// não fez de propósito. Os outros métodos do navegador que escrevem (`dir`,
/// `table`, `trace` e o `assert` que falha) o prelúdio monta sobre estes cinco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NivelDoConsole {
    /// `console.debug` (e `trace`) — o filtro padrão do `seele.log` não o
    /// escreve, como não escreve o DEBUG do próprio produto.
    Depuracao,
    /// `console.log` e `console.info` (e `dir` e `table`).
    Informacao,
    /// `console.warn`.
    Aviso,
    /// `console.error` (e o `assert` cuja condição é falsa) — e o erro que o
    /// MOD não pegou, ou a volta que passou de um teto, que o produto diz por
    /// ele.
    Erro,
}

impl NivelDoConsole {
    /// O nível pelo nome do método que o prelúdio passa.
    ///
    /// Um nome desconhecido é informação: um MOD que chama `seele.console`
    /// direto, com um texto qualquer no primeiro argumento, continua sendo
    /// ouvido — e não é promovido a erro por isso. Um primeiro argumento que
    /// não é texto nem chega aqui: a conversão do `rquickjs` lança no MOD antes
    /// (ver a ponte em [`Motor::subir`]).
    fn do_metodo(metodo: &str) -> Self {
        match metodo {
            "debug" => Self::Depuracao,
            "warn" => Self::Aviso,
            "error" => Self::Erro,
            _ => Self::Informacao,
        }
    }
}

/// Corta uma linha de `console` no teto de uma frase do registro, e diz que
/// cortou.
///
/// **O teto é o da janela**: [`crate::TETO_DA_FRASE_NO_REGISTRO`], os 512
/// caracteres que `registrar_da_janela` já aplicava ao que a janela escreve.
/// As duas portas põem no mesmo `seele.log` texto que um terceiro escolheu, e
/// uma não pode ser mais larga que a outra. **E o corte é este mesmo nas
/// duas**: a janela também escreve a frase em `?` (`frase = ?…`), e a corta
/// aqui, com o aviso — e não com um `take` de 512 caracteres contado antes do
/// escape, que deixava a frase ocupar até dez vezes o teto.
///
/// **Não é o [`TETO_DA_MENSAGEM`]**, que foi o primeiro candidato: doze
/// kibibytes por linha, num MOD que escreve em laço, são megabytes por minuto
/// num arquivo que só gira na abertura do app (`TETO_DO_LOG`, em `main.rs`).
/// Uma pilha de erro cortada em 512 caracteres ainda diz o nome, a mensagem e
/// os primeiros quadros.
///
/// **Pelo tamanho que a linha tem no registro**, e não pelo número de
/// caracteres. A bomba a escreve em `?` (`texto = ?texto`), e o `Debug` de um
/// `str` escapa todo caractere que não se imprime: um `\u{200b}` são oito
/// caracteres no arquivo. Cortada por caracteres, uma linha deles ocupava oito
/// vezes o teto — e o teto de vazão do `console`
/// ([`LINHAS_DE_CONSOLE_POR_SEGUNDO`]) é contado sobre este. Para ASCII e para
/// `é`, que não se escapam, o corte continua nos 512 caracteres.
///
/// O corte é o de [`ate_o_teto_do_registro`], a regra que as outras portas que
/// escapam texto de terceiro também usam. O aviso do corte conta os caracteres
/// do texto que chegou aqui: o que o MOD escreveu, quando ele passa texto, ou o
/// que o `console` do [`PRELUDIO`] montou de um objeto. **Só até 1024
/// objetos**: acima disso, o orçamento de cada passada de `emTexto` já pôs
/// `[…]` no lugar do resto, e o número é o do texto montado, menor do que o
/// objeto inteiro daria.
pub fn cortar_linha_do_console(texto: String) -> LinhaCortada {
    let cabe = ate_o_teto_do_registro(&texto);
    if cabe.len() == texto.len() {
        return LinhaCortada(texto);
    }
    let mut cortado = cabe.to_owned();
    let total = texto.chars().count();
    let _ = write!(cortado, " […cortado: {total} caracteres]");
    LinhaCortada(cortado)
}

/// **O começo de `texto` que cabe no teto de uma frase do registro**, contado
/// pelo tamanho que ele tem lá, já escapado ([`tamanho_no_registro`]).
///
/// Uma regra só para as quatro portas que escrevem texto de terceiro escapado
/// pelo `Debug` de um `str`: o `console` do MOD e a frase da janela
/// (`registrar_da_janela`, em `main.rs`), as duas por
/// [`cortar_linha_do_console`], com o aviso do corte; e o motivo e o caminho
/// de uma mídia de MOD (`motivo_no_registro` e `caminho_no_registro`, em
/// `main.rs`). Cortado **antes** do escape, um pedaço de 512 caracteres que
/// não se imprimem chegava a 5120 no registro; cortado por aqui, nenhuma das
/// quatro passa do teto, e o mesmo texto sai do mesmo tamanho por qualquer uma
/// delas.
///
/// Caractere a caractere, e não por bytes: parar antes do caractere que não
/// cabe respeita a fronteira dele por construção, e cortar por bytes pediria
/// achá-la à mão. E para no primeiro que não cabe, sem medir o resto.
pub fn ate_o_teto_do_registro(texto: &str) -> &str {
    let mut ocupado = 0_usize;
    for (onde, c) in texto.char_indices() {
        ocupado = ocupado.saturating_add(tamanho_no_registro(c));
        if ocupado > crate::TETO_DA_FRASE_NO_REGISTRO {
            return texto.get(..onde).unwrap_or_default();
        }
    }
    texto
}

/// Quantos caracteres `c` ocupa no `Debug` de um `str`, que é como a linha de
/// `console` vai ao registro.
///
/// É o `escape_debug` do caractere, com uma exceção — a única, conferida
/// caractere a caractere em todo o Unicode contra o `Debug` de um `str` de um
/// caractere só: o `Debug` de um `char` escapa o apóstrofo, e o de um `str`
/// não. Contá-lo como dois cortaria uma mensagem de erro de JavaScript — que
/// cita nomes entre apóstrofos — antes do teto, sem razão.
fn tamanho_no_registro(c: char) -> usize {
    if c == '\'' {
        1
    } else {
        c.escape_debug().count()
    }
}

/// O que sai do executor.
#[derive(Debug, PartialEq, Eq)]
pub enum ParaOFora {
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
    /// Uma linha que o MOD escreveu com `console` — ou uma que o produto diz
    /// por ele: a conta que o balde guardou e nenhuma linha levou
    /// ([`CONTA_DO_BALDE`], em aviso), e o erro que o MOD não pegou (em erro):
    /// o de um temporizador ou de um ouvinte, dito pelo prelúdio, e o que
    /// encerrou uma volta, dito por [`relatar`] — ou, quando quem a encerrou
    /// foi um teto, a frase do teto ([`relatar`] e [`escoar_jobs`]).
    ///
    /// **Não é fala para a janela**: é registro. Quem a escreve no `seele.log`
    /// é a bomba, que sabe de quem é o MOD; o executor não sabe, e não precisa
    /// saber para pôr a linha no canal.
    Console {
        /// O nível, do método que o MOD chamou, ou o que o produto escolheu
        /// para a linha que ele diz.
        nivel: NivelDoConsole,
        /// O texto, já cortado por [`cortar_linha_do_console`], ou
        /// [`CONTA_DO_BALDE`].
        texto: String,
        /// Quantas linhas o balde segurou antes desta. Zero é o normal.
        suprimidas: u32,
    },
}

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
pub const ATRASO_MAXIMO: Duration = Duration::from_secs(24 * 60 * 60);

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
///
/// **E o registro também fica sabendo**, quando quem parou foi um teto: a
/// mesma linha que [`relatar`] escreve para a volta parada, pelo mesmo balde.
/// Sem ela, o laço que o MOD pôs numa promessa parava sem uma palavra no
/// `seele.log`, e o mesmo laço no topo deixava uma.
///
/// **E é aqui que a volta termina para as rejeições.** Sem microtarefa
/// pendente, a promessa rejeitada que ainda espera em `rejeicoes` não foi pega
/// por ninguém, e vira a linha do registro e o `Falhou` da janela, e as
/// contadas além do teto da espera viram a conta ([`dizer_as_rejeitadas`]). Numa
/// volta que parou com microtarefas na fila — por um teto ou pela revogação —,
/// elas ainda podem pegá-las: as rejeições esperam a próxima volta que
/// termine, ou a parada da instância.
fn escoar_jobs(
    runtime: &Runtime,
    interrupcao: &Arc<Interrupcao>,
    manda: &Sender<ParaOFora>,
    fila: &Arc<Fila>,
    balde: &std::cell::RefCell<BaldeDoConsole>,
    rejeicoes: &std::cell::RefCell<RejeicoesDaVolta>,
) {
    while runtime.is_job_pending() && !interrupcao.revogada() {
        if let Err(excecao) = runtime.execute_pending_job() {
            avisar(manda, fila, ParaOFora::Interrompido);
            if let Some(teto) = interrupcao.tirar_teto() {
                // A exceção da parada é lida do contexto da microtarefa, pelo
                // onde: a primeira linha da pilha diz em que ponto do MOD o
                // laço estava.
                let lancado = excecao.0.with(|ctx| o_que_o_mod_lancou(&ctx));
                linha_de_console(
                    balde,
                    manda,
                    NivelDoConsole::Erro,
                    lancado.dito_como_parada(&interrupcao.frase_do_teto(teto)),
                );
            }
            break;
        }
    }
    if !runtime.is_job_pending() {
        dizer_as_rejeitadas(rejeicoes, balde, manda, fila);
    }
}

/// O começo da frase de uma promessa rejeitada sem tratamento — no `Falhou`
/// que vai à janela e na linha que vai ao registro, que dizem a mesma coisa.
pub const REJEITADA_SEM_TRATAMENTO: &str = "promessa rejeitada sem tratamento";

/// Quantas rejeições sem tratamento esperam, no máximo, o fim de uma volta.
///
/// **A rajada do balde menos uma**, a da conta: acima do teto a rejeição só é
/// contada, e no fim da volta saem as guardadas e depois uma linha «e mais N
/// rejeições sem tratamento além da espera» ([`dizer_as_rejeitadas`]). Com o
/// balde cheio, as guardadas e a conta saem de uma vez; com a rajada inteira de
/// guardadas, a conta seria a primeira segurada, e o `seele.log` diria só que
/// uma linha foi segurada, e não quantas rejeições ficaram de fora. A
/// primeira linha, que costuma ser a causa, continua saindo.
///
/// Só contada, e não dita na hora, porque nessa hora quem a vai pegar pode
/// ainda não ter tido a vez: um `Promise.allSettled` de mais promessas que o
/// teto, de funções `async` que lançam antes do primeiro `await`, deixava em
/// ERROR, como «sem tratamento», as que passavam do teto (F-Mods-m1 da revisão
/// final do Plano 1).
///
/// **A memória do lado de fora do motor**, onde o teto de memória do MOD não
/// alcança: no máximo este tanto de rejeições, cada uma com a linha dela e o
/// `Falhou` dela, os dois já cortados no teto do registro quando entram na
/// espera (até 512 caracteres no registro, mais o aviso do corte;
/// [`cortar_linha_do_console`]); um número para as de fora; e uma referência a
/// cada promessa guardada, que continua morando no motor. Na espera, o texto
/// inteiro do que o MOD lançou não fica: ele existe só enquanto a linha e o
/// `Falhou` são montados e cortados, uma rejeição de cada vez. Guardada sem
/// corte, como antes, a espera segurava até uma rajada inteira de mensagens do
/// tamanho que o MOD quisesse até o fim da volta (F-Mods-m2).
pub const REJEICOES_GUARDADAS: usize = RAJADA_DO_CONSOLE as usize - 1;

/// **As rejeições sem tratamento de uma volta, à espera do fim dela.**
///
/// O QuickJS chama o rastreador na hora da rejeição, e nessa hora quem a vai
/// pegar pode ainda não ter tido a vez: `try { await f(); } catch {}`, com `f`
/// uma função `async` que lança antes do primeiro `await`, rejeita a promessa
/// antes de o `await` prender o tratamento — medido: o rastreador é chamado, e
/// o MOD pegou. Dita na hora, a linha do registro diria «sem tratamento» em
/// ERROR, e o `Falhou` faria a gestão dizer «falhou», sobre uma rejeição que o
/// MOD tratou — o `Falhou` saía assim até a P51-01. O rastreador é chamado de
/// novo quando o tratamento chega (`ja_tratada`), e é essa segunda chamada que
/// a tira daqui ([`Self::tratada`]); o que sobra quando as microtarefas da
/// volta acabam ([`escoar_jobs`]) é o que ninguém pegou. É a regra do
/// `unhandledrejection` do navegador.
///
/// Guarda a promessa, e não um número: segurá-la viva até o fim da volta é o
/// que garante que a segunda chamada ache a mesma, e não outra que nasceu no
/// endereço de uma que já foi coletada. Por isso a lista é esvaziada antes de
/// o runtime ser desfeito ([`Motor::parar`]).
#[derive(Default)]
struct RejeicoesDaVolta {
    /// Cada promessa rejeitada sem tratamento, com o que ela vira.
    guardadas: Vec<Rejeitada>,
    /// Quantas rejeitaram sem tratamento com a espera cheia, e ainda não o
    /// ganharam: só contadas ([`REJEICOES_GUARDADAS`]).
    alem: u32,
}

/// Uma rejeição sem tratamento à espera do fim da volta, em
/// [`RejeicoesDaVolta`].
struct Rejeitada {
    /// A promessa, viva até o fim da volta.
    promessa: rquickjs::Persistent<rquickjs::Value<'static>>,
    /// A linha do registro, já cortada no teto dele.
    linha: LinhaCortada,
    /// O texto do `Falhou` que vai à janela, cortado pela mesma regra.
    falhou: LinhaCortada,
}

impl RejeicoesDaVolta {
    /// Guarda uma rejeição até o fim da volta, com a `linha` do registro e o
    /// texto do `Falhou` da janela, os dois cortados no teto do registro; com a
    /// espera cheia ([`REJEICOES_GUARDADAS`]), só a conta.
    fn guardar<'js>(
        &mut self,
        ctx: &rquickjs::Ctx<'js>,
        promessa: rquickjs::Value<'js>,
        linha: String,
        falhou: String,
    ) {
        if self.guardadas.len() >= REJEICOES_GUARDADAS {
            self.alem = self.alem.saturating_add(1);
            return;
        }
        self.guardadas.push(Rejeitada {
            promessa: rquickjs::Persistent::save(ctx, promessa),
            linha: cortar_linha_do_console(linha),
            falhou: cortar_linha_do_console(falhou),
        });
    }

    /// A rejeição de `promessa` ganhou tratamento: ela sai da espera, sem linha.
    ///
    /// **Ou sai da conta.** A segunda chamada do rastreador só vem para uma
    /// promessa que a primeira deu como sem tratamento, e uma que não está na
    /// espera é, nesta volta, uma das contadas com ela cheia. O que isso não
    /// separa: a promessa de uma volta que já terminou, pega agora, também tira
    /// uma da conta desta volta, e a conta pode sair menor do que foi — nunca
    /// maior, que é o erro que esta espera existe para não cometer.
    fn tratada<'js>(&mut self, ctx: &rquickjs::Ctx<'js>, promessa: &rquickjs::Value<'js>) {
        let antes = self.guardadas.len();
        self.guardadas.retain(|guardada| {
            guardada
                .promessa
                .clone()
                .restore(ctx)
                .map_or(true, |ela| ela != *promessa)
        });
        if self.guardadas.len() == antes {
            self.alem = self.alem.saturating_sub(1);
        }
    }

    /// A linha e o `Falhou` de cada uma que ninguém pegou, nesta ordem e na
    /// ordem em que rejeitaram, e a conta das de fora; a espera fica vazia, e
    /// as promessas, soltas.
    fn sem_tratamento(&mut self) -> (Vec<(LinhaCortada, LinhaCortada)>, u32) {
        let ditas = std::mem::take(&mut self.guardadas)
            .into_iter()
            .map(|Rejeitada { linha, falhou, .. }| (linha, falhou))
            .collect();
        (ditas, std::mem::take(&mut self.alem))
    }
}

/// A linha que diz quantas rejeições sem tratamento passaram do teto da
/// espera, depois das linhas das guardadas. O `Falhou` que diz o mesmo à
/// janela é outro texto ([`falhou_alem_da_espera`]).
fn alem_da_espera(quantas: u32) -> String {
    if quantas == 1 {
        "e mais 1 rejeição sem tratamento além da espera".to_owned()
    } else {
        format!("e mais {quantas} rejeições sem tratamento além da espera")
    }
}

/// O `Falhou` da conta das rejeições além da espera: a frase das outras
/// rejeições na frente, porque ele chega à gestão sem as linhas que o cercam
/// no registro, e «e mais N além da espera» sozinho não diz de quê.
fn falhou_alem_da_espera(quantas: u32) -> String {
    format!("{REJEITADA_SEM_TRATAMENTO}: e mais {quantas} além da espera")
}

/// **Diz as rejeições que ninguém pegou**, cada uma como uma linha de
/// `console` em erro, pelo balde, sem cortar de novo ([`linha_ja_cortada`]),
/// e depois como o `Falhou` dela, pela cota dos avisos ([`avisar`]); e por
/// fim, se alguma passou do teto da espera, quantas — numa linha e num
/// `Falhou` só.
///
/// **A linha antes do `Falhou`**: a bomba lê o canal em ordem, e quem vê a
/// gestão dizer «falhou» e abre o `seele.log` já acha lá a linha, com a pilha.
/// O `Falhou` vai mesmo quando o balde segura a linha: a cota dele é outra, e
/// a janela não pode deixar de saber porque o registro estava cheio.
fn dizer_as_rejeitadas(
    rejeicoes: &std::cell::RefCell<RejeicoesDaVolta>,
    balde: &std::cell::RefCell<BaldeDoConsole>,
    manda: &Sender<ParaOFora>,
    fila: &Arc<Fila>,
) {
    // Entre duas voltas nenhum JavaScript roda, e o rastreador não tem a
    // espera emprestada.
    let (ditas, alem) = match rejeicoes.try_borrow_mut() {
        Ok(mut rejeicoes) => rejeicoes.sem_tratamento(),
        Err(_) => return,
    };
    for (linha, falhou) in ditas {
        linha_ja_cortada(balde, manda, NivelDoConsole::Erro, linha);
        avisar(manda, fila, ParaOFora::Falhou(falhou.0));
    }
    if alem > 0 {
        linha_de_console(balde, manda, NivelDoConsole::Erro, alem_da_espera(alem));
        avisar(manda, fila, ParaOFora::Falhou(falhou_alem_da_espera(alem)));
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

/// O resultado de uma volta do motor e, quando o MOD lançou, o que ele lançou
/// ([`o_que_o_mod_lancou`]).
type Volta = (rquickjs::Result<()>, Option<Lancado>);

/// **O que saiu de uma volta como exceção**, em duas partes: o que é e onde.
///
/// Separadas porque a volta que o produto parou por um teto também chega como
/// exceção (`InternalError: interrupted`), e nela o «o quê» é a frase do teto,
/// e não o que o QuickJS diz — mas o onde continua sendo o do MOD.
#[derive(Debug)]
struct Lancado {
    /// `nome: mensagem` quando é um `Error`; o valor como o JavaScript o diria,
    /// quando não é.
    texto: String,
    /// A primeira linha da pilha, quando há: `at <eval> (eval_script:1:20)`.
    onde: Option<String>,
}

impl Lancado {
    /// `nome: mensagem` e a primeira linha da pilha de um `Error`.
    ///
    /// O que não vira texto não derruba o resto: um nome que não vira texto
    /// sai como `Error`, e uma mensagem ou uma pilha, vazia. A exceção que a
    /// leitura deixar pendente fica para quem chamou tirar.
    fn da_excecao(excecao: &rquickjs::Exception<'_>) -> Self {
        use rquickjs::convert::Coerced;
        let nome = excecao
            .get::<_, Coerced<String>>("name")
            .map_or_else(|_| "Error".to_owned(), |nome| nome.0);
        let mensagem = excecao.message().unwrap_or_default();
        let pilha = excecao.stack().unwrap_or_default();
        Self {
            texto: format!("{nome}: {mensagem}"),
            onde: pilha
                .lines()
                .find(|linha| !linha.trim().is_empty())
                .map(str::to_owned),
        }
    }

    /// A linha do registro quando o MOD lançou: `o MOD lançou: <texto>` e o
    /// onde.
    fn dito_como_excecao(&self) -> String {
        self.com_o_onde(&format!("o MOD lançou: {}", self.texto))
    }

    /// A linha do registro de uma promessa rejeitada que ninguém pegou:
    /// `promessa rejeitada sem tratamento: <texto>` e o onde.
    fn dito_como_rejeicao(&self) -> String {
        self.com_o_onde(&format!("{REJEITADA_SEM_TRATAMENTO}: {}", self.texto))
    }

    /// A linha do registro quando quem parou foi o produto: a `frase` do teto,
    /// sem o que o QuickJS diz da interrupção, e o onde do MOD.
    fn dito_como_parada(&self, frase: &str) -> String {
        self.com_o_onde(frase)
    }

    /// `cabeca` e, numa linha abaixo, o onde, quando há.
    fn com_o_onde(&self, cabeca: &str) -> String {
        match &self.onde {
            Some(onde) => format!("{cabeca}\n{onde}"),
            None => cabeca.to_owned(),
        }
    }
}

/// Junta ao resultado de uma volta o que o MOD lançou, **lido ainda dentro do
/// contexto**: a exceção pendente é dele, e fora de `contexto.with` ela já não
/// tem de onde ser lida.
fn com_o_que_lancou(ctx: &rquickjs::Ctx<'_>, resultado: rquickjs::Result<()>) -> Volta {
    let lancado =
        matches!(resultado, Err(rquickjs::Error::Exception)).then(|| o_que_o_mod_lancou(ctx));
    (resultado, lancado)
}

/// **O que o MOD lançou, em texto**: `nome: mensagem` e a primeira linha da
/// pilha, quando é um `Error`; o valor como o JavaScript o diria, quando não é.
///
/// A pilha inteira não cabe no teto de uma linha, e a primeira linha é a que
/// diz onde: `at <eval> (eval_script:1:20)`. O que não vira texto não derruba
/// o resto: um nome que não vira texto sai como `Error`, uma mensagem ou uma
/// pilha sai vazia, e um valor lançado que não vira texto (um `Symbol`) sai
/// como um recuo que diz isso. A exceção que essa leitura deixar pendente é
/// tirada — a próxima volta não pode herdar uma exceção que não é dela.
fn o_que_o_mod_lancou(ctx: &rquickjs::Ctx<'_>) -> Lancado {
    use rquickjs::convert::Coerced;
    let lancado = ctx.catch();
    let lido = match lancado.as_exception() {
        Some(excecao) => Lancado::da_excecao(excecao),
        None => Lancado {
            texto: lancado.get::<Coerced<String>>().map_or_else(
                |_| "[valor que não virou texto]".to_owned(),
                |texto| texto.0,
            ),
            onde: None,
        },
    };
    if ctx.has_exception() {
        let _ = ctx.catch();
    }
    lido
}

/// Traduz o resultado de uma volta para o canal de saída.
///
/// **Quando o MOD lançou, a janela recebe o `Falhou` e o registro recebe o
/// texto.** O `Falhou` era tudo, e dizia «o MOD lançou» sem dizer o quê: o erro
/// mais comum de quem escreve MOD, ler de `null`, não deixava uma linha no
/// `seele.log`. O texto vai como uma linha de `console` em erro, pelo mesmo
/// balde ([`linha_de_console`]): com o id do MOD, o escape e o corte de
/// qualquer outra, e um MOD que lança em toda volta não passa do teto de vazão.
///
/// **Quando quem parou a volta foi um teto, a linha diz o teto**, e não «o MOD
/// lançou»: o QuickJS faz da interrupção uma exceção, e o texto dela seria
/// `InternalError: interrupted` — que manda procurar um `throw` que não
/// existe. O `Falhou` da janela fica como era.
fn relatar(
    manda: &Sender<ParaOFora>,
    fila: &Arc<Fila>,
    interrupcao: &Arc<Interrupcao>,
    balde: &std::cell::RefCell<BaldeDoConsole>,
    (resultado, lancado): Volta,
) {
    // Tirado em toda volta, e não só na que lançou: um teto que disse sim
    // numa volta que terminou bem não pode sobrar para a seguinte.
    let teto = interrupcao.tirar_teto();
    match resultado {
        Ok(()) => {}
        Err(_) if interrupcao.revogada() => {
            avisar(manda, fila, ParaOFora::Interrompido);
        }
        // Uma interrupção chega como erro do motor, como qualquer outra
        // exceção. Distinguir as duas importa: uma é o MOD com defeito, a
        // outra é o produto parando o MOD, e elas pedem frases diferentes —
        // a revogação acima, e os tetos aqui, pelo que a `Interrupcao`
        // guardou.
        Err(rquickjs::Error::Exception) => {
            avisar(manda, fila, ParaOFora::Falhou("o MOD lançou".into()));
            let linha = match (teto, lancado) {
                (Some(teto), Some(lancado)) => {
                    lancado.dito_como_parada(&interrupcao.frase_do_teto(teto))
                }
                (Some(teto), None) => interrupcao.frase_do_teto(teto),
                (None, Some(lancado)) => lancado.dito_como_excecao(),
                (None, None) => return,
            };
            linha_de_console(balde, manda, NivelDoConsole::Erro, linha);
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
/// que é uma das duas portas.
///
/// Escrita aqui e não num arquivo de MOD de propósito: ela é o instrumento
/// desta medição, e um instrumento que mora junto do que ele mede não sai de
/// sincronia com ele.
///
/// **Pública e escondida, e não só de teste**, desde que a volta saiu do
/// `executor.rs`: os testes do desktop a rodam pelo `ExecutorQuickJs`, e o
/// `cargo xtask web` a roda dentro do `.wasm`. As duas listas de globais têm
/// de ser a mesma.
#[doc(hidden)]
pub const SONDA: &str = r#"
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

/// Um MOD de pé: o runtime, o contexto, os temporizadores e o balde dele.
///
/// **É a volta do executor sem a thread.** Até 09/10/2026 isto era o corpo de
/// `rodar`, no `executor.rs` do desktop, e o celular tinha uma cópia menor.
/// Agora os dois embrulham o mesmo `Motor`:
/// - o desktop numa thread, com o `mpsc` e a espera de sempre;
/// - o celular num Worker, com o relógio do navegador (`seele-motor-web`).
///
/// Quem embrulha decide **quando** chamar cada verbo. O que acontece numa volta
/// é daqui, e só daqui:
/// - [`Motor::subir`] sobe o runtime, o rastreador de rejeições e a ponte;
/// - [`Motor::codigo`], [`Motor::resposta`] e [`Motor::diagnostico`] são as
///   entradas;
/// - [`Motor::vencer`] dispara os temporizadores na hora, e [`Motor::proximo_prazo`]
///   diz quando chamá-lo;
/// - [`Motor::parar`] diz o que ficou e confirma a parada.
///
/// A afinidade é de uma thread só: o `Runtime` do QuickJS não atravessa
/// threads, e o binding é explícito sobre isso. O descarte acontece em
/// [`Motor::parar`], na dona do runtime.
pub struct Motor {
    contexto: Context,
    runtime: Runtime,
    manda: Sender<ParaOFora>,
    interrupcao: Arc<Interrupcao>,
    fila: Arc<Fila>,
    balde_do_console: std::rc::Rc<std::cell::RefCell<BaldeDoConsole>>,
    rejeicoes: std::rc::Rc<std::cell::RefCell<RejeicoesDaVolta>>,
    /// A tabela de temporizadores, que é do anfitrião e morre com ele.
    relogios: std::collections::BTreeMap<u32, Temporizador>,
}

/// O runtime do motor, com o alocador de cada alvo.
///
/// **No `wasm32-wasip1`, o `RustAllocator`.** Com o alocador padrão do
/// quickjs-ng, o `js__malloc_usable_size` devolve 0 nesse alvo
/// (`rquickjs-sys-0.12.2/quickjs/cutils.h:601-612`), e o teto de memória nunca é
/// atingido: um MOD alocou 128 MiB com o teto de 8 MiB. O guarda roda dentro do
/// `.wasm` (`xtask/web/motor.mjs`, pelo `cargo xtask web`), porque no nativo o
/// alocador padrão já responde o tamanho dos blocos.
fn runtime_do_alvo() -> rquickjs::Result<Runtime> {
    #[cfg(target_family = "wasm")]
    {
        Runtime::new_with_alloc(rquickjs::allocator::RustAllocator)
    }
    #[cfg(not(target_family = "wasm"))]
    {
        Runtime::new()
    }
}

impl Motor {
    /// Sobe o runtime, o rastreador de rejeições e a ponte para fora.
    ///
    /// Devolve nada quando uma das três não sobe, **depois de dizer qual** e de
    /// confirmar a parada pelo canal: quem embrulha não precisa saber o motivo
    /// para encerrar, e a janela fica sabendo pelo mesmo caminho de sempre.
    #[must_use]
    pub fn subir(
        manda: &Sender<ParaOFora>,
        interrupcao: &Arc<Interrupcao>,
        fila: &Arc<Fila>,
        limites: Limites,
    ) -> Option<Self> {
        let Ok(runtime) = runtime_do_alvo() else {
            let _ = manda.send(ParaOFora::Falhou("o motor não subiu".into()));
            let _ = manda.send(ParaOFora::Parou);
            return None;
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
        // **Nem a janela nem o registro ficam sabendo na primeira**: o `Falhou` e a
        // linha esperam juntos o fim da volta, e é esta segunda chamada que os
        // tira da espera (ver [`RejeicoesDaVolta`]). O `Falhou` saía daqui, na
        // hora, e a gestão dizia «falhou» sobre a rejeição que o MOD pegava logo
        // depois (P51-01).
        //
        // O `Falhou` passa pela mesma cota dos outros avisos, e por isso um MOD
        // que rejeita num laço não enche o canal — a cota é o que separa «dito» de
        // «gritado». A linha do registro passa pelo balde do `console`, pela mesma
        // razão; e os dois saem cortados no teto do registro.
        //
        // **Três donos do balde, uma thread.** A ponte gasta ficha a cada linha; o
        // rastreador, só na reentrada em que a espera das rejeições não pode ser
        // emprestada; e a volta, ao fim de cada entrada e na parada, gasta com
        // as rejeições que ninguém pegou e tira dele a conta que nenhuma linha
        // levou (ver [`dizer_as_rejeitadas`] e [`dizer_as_seguradas`]). Um `Rc` e
        // não um `Arc`: os três moram na thread do motor, e este `rquickjs` (sem
        // `parallel`) não pede `Send` ao rastreador.
        let balde_do_console = std::rc::Rc::new(std::cell::RefCell::new(BaldeDoConsole::novo(
            Instant::now(),
        )));
        let rejeicoes = std::rc::Rc::new(std::cell::RefCell::new(RejeicoesDaVolta::default()));
        let manda_rejeicao = manda.clone();
        let fila_de_rejeicao = Arc::clone(fila);
        let balde_da_rejeicao = std::rc::Rc::clone(&balde_do_console);
        let rejeicoes_do_rastreador = std::rc::Rc::clone(&rejeicoes);
        runtime.set_host_promise_rejection_tracker(Some(Box::new(
            move |ctx, promessa, motivo, ja_tratada| {
                if ja_tratada {
                    if let Ok(mut rejeicoes) = rejeicoes_do_rastreador.try_borrow_mut() {
                        rejeicoes.tratada(&ctx, &promessa);
                    }
                    return;
                }
                // A leitura do motivo pode rodar código do MOD (um `name` com
                // getter, um `toJSON`) e deixar uma exceção pendente. Ela é nossa,
                // e é tirada; a de antes, se houvesse, não é.
                let havia_excecao = ctx.has_exception();
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
                let linha = match motivo.as_exception() {
                    Some(excecao) => Lancado::da_excecao(excecao),
                    None => Lancado {
                        texto: texto.clone(),
                        onde: None,
                    },
                }
                .dito_como_rejeicao();
                if !havia_excecao && ctx.has_exception() {
                    let _ = ctx.catch();
                }
                let falhou = format!("{REJEITADA_SEM_TRATAMENTO}: {texto}");
                // A espera guarda a linha e o `Falhou`, ou só a conta quando está
                // cheia. Só uma reentrada, que não existe (a espera fica emprestada
                // em código nativo que não roda JavaScript), deixaria de achá-la:
                // aí os dois saem na hora, a linha pelo balde e o `Falhou` pela
                // cota, em vez de sumir — sem isso, a janela deixaria de saber.
                match rejeicoes_do_rastreador.try_borrow_mut() {
                    Ok(mut rejeicoes) => rejeicoes.guardar(&ctx, promessa, linha, falhou),
                    Err(_) => {
                        linha_de_console(
                            &balde_da_rejeicao,
                            &manda_rejeicao,
                            NivelDoConsole::Erro,
                            linha,
                        );
                        avisar(
                            &manda_rejeicao,
                            &fila_de_rejeicao,
                            ParaOFora::Falhou(cortar_linha_do_console(falhou).0),
                        );
                    }
                }
            },
        )));

        let Ok(contexto) = Context::full(&runtime) else {
            let _ = manda.send(ParaOFora::Falhou("o contexto não subiu".into()));
            let _ = manda.send(ParaOFora::Parou);
            return None;
        };

        // **A única ponte para fora.** Duas funções, e o que as duas fazem é pôr
        // texto num canal: `postar` fala com a janela, `console` fala com o
        // registro. Não há aqui disco, rede, IPC nem armazenamento — e a ausência não
        // é uma jaula construída com cuidado: é o que um contexto de QuickJS **é**
        // antes de alguém acrescentar coisas a ele.
        //
        // O prelúdio que traduz esta ponte na API que um MOD conhece — `SeeleMods`
        // e `SeeleUI` — é montado sobre ela em [`PRELUDIO`], em JavaScript, pela
        // mesma razão que o do Worker: é código que roda dentro do contexto do MOD,
        // e escrevê-lo em Rust não o tornaria mais nosso.
        let saida = manda.clone();
        let contagem = Arc::clone(fila);
        let linhas = manda.clone();
        // O balde é o de cima, dividido com o rastreador de rejeições.
        let balde_da_ponte = std::rc::Rc::clone(&balde_do_console);
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
                    if json.len() > TETO_DA_MENSAGEM {
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
            // **O `console` do MOD, que não existia.** O prelúdio monta
            // `console.log/info/warn/error/debug` sobre esta função (e os outros
            // métodos do navegador sobre esses); ela passa a linha pelo balde, a
            // corta e a põe no canal, e a bomba a escreve no `seele.log` com o id
            // do MOD. Não devolve nada e, **com duas strings, não lança**. Com
            // outra coisa, a conversão do `rquickjs` lança no MOD antes de chegar
            // aqui — quem garante que o `console` do MOD nunca lança é o do
            // prelúdio, que sempre passa duas strings e chama esta dentro de um
            // `try`: um registro que derruba o caminho que ele observa é pior que
            // registro nenhum.
            seele.set(
                "console",
                Function::new(ctx.clone(), move |metodo: String, texto: String| {
                    linha_de_console(
                        &balde_da_ponte,
                        &linhas,
                        NivelDoConsole::do_metodo(&metodo),
                        texto,
                    );
                })?,
            )?;
            ctx.globals().set("seele", seele)?;
            Ok(())
        });
        if montou.is_err() {
            let _ = manda.send(ParaOFora::Falhou("a ponte não montou".into()));
            let _ = manda.send(ParaOFora::Parou);
            return None;
        }

        Some(Self {
            contexto,
            runtime,
            manda: manda.clone(),
            interrupcao: Arc::clone(interrupcao),
            fila: Arc::clone(fila),
            balde_do_console,
            rejeicoes,
            relogios: std::collections::BTreeMap::new(),
        })
    }

    /// **Até quando quem embrulha pode esperar** a próxima entrada: até o
    /// temporizador mais próximo ou até sobrar uma ficha no balde do
    /// `console`, o que vier primeiro. Nada é esperar sem prazo.
    ///
    /// É isto que faz a thread do desktop não gastar CPU parada: sem
    /// temporizador e sem linha segurada ela dorme no canal, e com um dos dois
    /// ela dorme até a hora do mais próximo. No celular, é o `setTimeout` do
    /// Worker.
    ///
    /// **A ficha que sobra acorda a volta para dizer a conta** que nenhuma
    /// linha levou: um MOD que escreve uma rajada e se cala, sem
    /// temporizador nem resposta, tinha a conta dita só quando parava, e o
    /// `seele.log` mostrava a rajada como se fosse tudo enquanto ele estava
    /// de pé. «Sobrar» é a segunda ficha, e não a primeira, que fica com o
    /// MOD por um período inteiro: acordar na primeira fazia a conta tomar a
    /// vez de um MOD que continuava escrevendo ([`BaldeDoConsole::
    /// ate_sobrar_uma_ficha`]). O custo é acordar uma vez por conta, e só
    /// enquanto o balde segura alguma linha: o [`Motor::vencer`] diz a
    /// conta e zera as seguradas, e a espera seguinte volta a ser sem prazo.
    #[must_use]
    pub fn proximo_prazo(&self) -> Option<Duration> {
        let ficha = self
            .balde_do_console
            .try_borrow()
            .ok()
            .and_then(|balde| balde.ate_sobrar_uma_ficha(Instant::now()));
        [proximo_vencimento(&self.relogios), ficha]
            .into_iter()
            .flatten()
            .min()
    }

    /// **Venceu um prazo** — de um temporizador, ou a ficha que sobrou no
    /// balde: dispara os temporizadores que estão na hora (quando quem
    /// acordou foi a ficha, pode não haver nenhum), dentro de uma volta com
    /// prazo, como qualquer outra execução. Os temporizadores vêm antes da
    /// conta: se os dois vencem juntos, a linha do MOD leva a conta, e o
    /// produto não precisa falar.
    pub fn vencer(&mut self) {
        self.interrupcao.comecar();
        let vencidos = recolher_vencidos(&mut self.relogios);
        for id in vencidos {
            let volta = self.contexto.with(|ctx| {
                let resultado = match ctx.globals().get::<_, Function<'_>>("__seeleRelogio") {
                    Ok(bate) => bate.call::<_, ()>((id,)),
                    Err(_) => Ok(()),
                };
                com_o_que_lancou(&ctx, resultado)
            });
            relatar(
                &self.manda,
                &self.fila,
                &self.interrupcao,
                &self.balde_do_console,
                volta,
            );
        }
        // **O fim da volta também aqui.** Sem ele, um callback que agenda
        // outro temporizador — ou que cancela o próprio intervalo — deixava o
        // pedido parado até chegar uma mensagem de fora. Num MOD que só usa
        // relógio, «uma mensagem de fora» pode nunca chegar: o timeout
        // encadeado nunca disparava e o intervalo cancelado continuava
        // batendo. E a conta do console, pela mesma razão: este fim de volta é
        // o único que um MOD só de relógio tem — e é o da espera que a ficha
        // que sobrou acordou, que existe só para dizer esta conta.
        self.fim_da_volta();
    }

    /// O código do MOD, uma vez, com as capacidades da API que o manifesto
    /// dele declarou.
    ///
    /// O prelúdio primeiro, e o código do MOD depois, na mesma volta: se o
    /// prelúdio não subir, o MOD não deve subir.
    ///
    /// **As capacidades entram antes do prelúdio**, como um vetor que ele lê e
    /// apaga. Passá-las por interpolação de texto faria o prelúdio deixar de
    /// ser uma constante — e uma constante é exatamente o que dá para revisar
    /// uma vez e confiar sempre. Quem as calcula é quem embrulha, pelo
    /// `capacidades_da_api` do `seele-proto`: este crate não conhece o
    /// manifesto.
    pub fn codigo(&mut self, fonte: &str, capacidades: &[&str]) {
        self.interrupcao.comecar();
        let volta = self.contexto.with(|ctx| {
            let resultado = (|| -> rquickjs::Result<()> {
                let lista = rquickjs::Array::new(ctx.clone())?;
                for (onde, nome) in capacidades.iter().enumerate() {
                    lista.set(onde, *nome)?;
                }
                ctx.globals().set("__seeleCapacidades", lista)?;
                // O teto por mensagem, pelo mesmo caminho e pela mesma
                // razão: o prelúdio precisa dele para separar «não
                // coube» de «a fila encheu», e interpolá-lo no texto
                // faria o prelúdio deixar de ser uma constante.
                ctx.globals()
                    .set("__seeleTetoDaMensagem", TETO_DA_MENSAGEM as u32)?;
                ctx.eval::<(), _>(PRELUDIO.as_bytes())?;
                ctx.eval::<(), _>(fonte.as_bytes())
            })();
            com_o_que_lancou(&ctx, resultado)
        });
        relatar(
            &self.manda,
            &self.fila,
            &self.interrupcao,
            &self.balde_do_console,
            volta,
        );
        self.fim_da_volta();
    }

    /// Uma resposta a um pedido que o MOD fez (`aoResponder` do prelúdio).
    ///
    /// A fila de entrada é de quem embrulha: o lugar dela volta antes desta
    /// chamada, ao tirar a mensagem da fila, que é quando ela deixa de ocupar
    /// memória.
    pub fn resposta(&mut self, json: &str) {
        self.interrupcao.comecar();
        let volta = self.contexto.with(|ctx| {
            let resultado = match ctx.globals().get::<_, Function<'_>>("aoResponder") {
                Ok(ao_responder) => ao_responder.call::<_, ()>((json,)),
                Err(_) => Ok(()),
            };
            com_o_que_lancou(&ctx, resultado)
        });
        relatar(
            &self.manda,
            &self.fila,
            &self.interrupcao,
            &self.balde_do_console,
            volta,
        );
        self.fim_da_volta();
    }

    /// Conta o que o anfitrião tem de pé por este MOD.
    ///
    /// Existe porque «a tabela ficou vazia» não se observa de fora: ela é do
    /// anfitrião, e o MOD não a enxerga. Sem isto, provar que um intervalo
    /// cancelado saiu da tabela seria provar pela ausência de batidas — o que
    /// também acontece quando o motor morre.
    pub fn diagnostico(&mut self) {
        let _ = self.manda.send(ParaOFora::Diagnostico {
            relogios: self.relogios.len(),
        });
        self.fim_da_volta();
    }

    /// Quanto o motor alocou, em bytes: para quem mede o custo.
    #[must_use]
    pub fn memoria(&self) -> i64 {
        self.runtime.memory_usage().malloc_size
    }

    /// O fim de toda entrada: as microtarefas, os pedidos de temporizador que
    /// o MOD fez nesta volta, e a conta do console que nenhuma linha levou, se
    /// sobrar uma ficha além da que fica com o MOD.
    fn fim_da_volta(&mut self) {
        escoar_jobs(
            &self.runtime,
            &self.interrupcao,
            &self.manda,
            &self.fila,
            &self.balde_do_console,
            &self.rejeicoes,
        );
        recolher_pedidos_de_relogio(&self.contexto, &mut self.relogios);
        dizer_as_seguradas(&self.balde_do_console, &self.manda, |balde| {
            balde.pendentes_se_couber(Instant::now())
        });
    }

    /// **Para, diz o que ficou e confirma.**
    ///
    /// As rejeições que ainda esperavam o fim de uma volta saem antes da conta:
    /// a instância parou, e ninguém mais vai pegá-las. E antes do descarte, de
    /// qualquer jeito — cada uma segura a promessa dela, e um runtime não se
    /// desfaz com valor dele vivo do lado de fora.
    ///
    /// **A conta do console antes da confirmação de parada**, com ficha ou sem:
    /// depois do `Parou` quem lê não lê mais nada, e a conta cairia com o
    /// contexto.
    ///
    /// O descarte acontece **aqui**, na dona do runtime, e não noutra thread. O
    /// `Parou` é a confirmação que `encerrou()` espera no contrato da
    /// interface: revogar é imediato e nosso, parar é do motor.
    pub fn parar(self) {
        let Self {
            contexto,
            runtime,
            manda,
            fila,
            balde_do_console,
            rejeicoes,
            ..
        } = self;
        dizer_as_rejeitadas(&rejeicoes, &balde_do_console, &manda, &fila);
        dizer_as_seguradas(&balde_do_console, &manda, BaldeDoConsole::pendentes);
        drop(contexto);
        drop(runtime);
        let _ = manda.send(ParaOFora::Parou);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// **O balde segura a rajada, devolve no ritmo e conta o que segurou.**
    ///
    /// Com o relógio passado por quem chama: o teste mede o balde sem dormir.
    #[test]
    fn o_balde_do_console_segura_a_rajada_e_conta_o_que_segurou() {
        let inicio = Instant::now();
        let mut balde = BaldeDoConsole::novo(inicio);
        for n in 0..RAJADA_DO_CONSOLE {
            assert_eq!(
                balde.admitir(inicio),
                Some(0),
                "a linha {n} da rajada foi segurada"
            );
        }
        assert_eq!(balde.admitir(inicio), None, "o balde passou da rajada");
        assert_eq!(balde.admitir(inicio), None, "o balde passou da rajada");
        // Uma ficha a cada `1 / LINHAS_DE_CONSOLE_POR_SEGUNDO` segundo.
        let depois =
            inicio + Duration::from_millis(1000 / u64::from(LINHAS_DE_CONSOLE_POR_SEGUNDO));
        assert_eq!(
            balde.admitir(depois),
            Some(2),
            "a linha que passa depois do aperto não disse quantas ficaram para trás"
        );
        assert_eq!(balde.admitir(depois), None, "uma ficha virou duas");
        // Uma hora parado enche o balde — e a conta recomeça depois de dita:
        // a próxima linha diz só a que ficou para trás desde a última.
        let muito_depois = depois + Duration::from_secs(3600);
        assert_eq!(
            balde.admitir(muito_depois),
            Some(1),
            "a conta não recomeçou depois de dita: a linha seguinte repetiria as que já tinham \
             sido contadas, e o registro as somaria duas vezes"
        );
        // E encher não passa da rajada: um MOD parado não ganha crédito para
        // inundar o registro.
        let passaram = 1
            + (1..RAJADA_DO_CONSOLE * 2)
                .filter(|_| balde.admitir(muito_depois).is_some())
                .count();
        assert_eq!(
            passaram, RAJADA_DO_CONSOLE as usize,
            "o balde encheu além da rajada, e um MOD parado ganharia crédito para inundar o registro"
        );
    }

    /// **A conta que nenhuma linha levou sai do balde por conta própria**: ao
    /// fim de uma volta, só com duas fichas — a dela e a da próxima linha do
    /// MOD —, tenha a volta escrito ou não; no fim da instância, sempre.
    #[test]
    fn o_balde_devolve_a_conta_que_nenhuma_linha_levou() {
        let inicio = Instant::now();
        let uma_ficha = Duration::from_millis(1000 / u64::from(LINHAS_DE_CONSOLE_POR_SEGUNDO));
        let mut balde = BaldeDoConsole::novo(inicio);
        assert_eq!(
            balde.pendentes_se_couber(inicio),
            None,
            "um balde que não segurou nada inventou uma conta ao fim da volta"
        );
        assert_eq!(
            balde.pendentes(),
            None,
            "um balde que não segurou nada inventou uma conta no fim"
        );
        let passaram = (0..RAJADA_DO_CONSOLE + 3)
            .filter(|_| balde.admitir(inicio).is_some())
            .count();
        assert_eq!(
            passaram, RAJADA_DO_CONSOLE as usize,
            "uma volta sem nada a contar gastou ficha, e o MOD perdeu uma linha da rajada para nada"
        );
        assert_eq!(
            balde.pendentes_se_couber(inicio),
            None,
            "a conta saiu sem ficha: uma linha a mais por volta passaria do teto de vazão"
        );
        // Uma volta sem linha nenhuma, com uma ficha: ela é da próxima linha
        // do MOD.
        assert_eq!(
            balde.pendentes_se_couber(inicio + uma_ficha),
            None,
            "o fim de uma volta sem linha disse a conta com uma ficha só: a próxima linha do MOD \
             acha o balde seco, e quem recebe mais do que escreve perde as linhas para a conta"
        );
        // Com duas, a conta sai e deixa uma.
        let duas = inicio + uma_ficha * 2;
        assert_eq!(
            balde.pendentes_se_couber(duas),
            Some(3),
            "com duas fichas ao fim de uma volta sem linha, a conta das seguradas não saiu, ou a \
             volta de antes a perdeu"
        );
        assert_eq!(
            balde.admitir(duas),
            Some(0),
            "a conta dita com duas fichas não deixou a do MOD"
        );
        // Uma volta em que a linha do MOD foi segurada: a ficha que se
        // completa depois, ainda nela, é da próxima linha dele.
        assert_eq!(balde.admitir(duas), None, "uma ficha virou duas");
        assert_eq!(
            balde.pendentes_se_couber(duas + uma_ficha),
            None,
            "numa volta em que a linha do MOD foi segurada, a conta tomou a ficha que se \
             completou depois: a próxima linha do MOD acha o balde seco, e quem escreve e segue \
             trabalhando perde as linhas para a conta"
        );
        // Com duas, sai — e deixa uma.
        let quatro = duas + uma_ficha * 2;
        assert_eq!(
            balde.pendentes_se_couber(quatro),
            Some(1),
            "com duas fichas ao fim de uma volta em que a linha do MOD foi segurada, a conta não \
             saiu"
        );
        assert_eq!(
            balde.admitir(quatro),
            Some(0),
            "a conta dita com duas fichas não deixou a do MOD"
        );
        assert_eq!(
            balde.admitir(quatro),
            None,
            "a conta dita ao fim da volta não gastou a ficha, e o teto de vazão deixa de valer"
        );
        assert_eq!(
            balde.pendentes(),
            Some(1),
            "no fim da instância, sem ficha, a conta do que ficou para trás não saiu"
        );
        assert_eq!(
            balde.pendentes(),
            None,
            "a conta do fim não recomeçou depois de dita, e o registro a somaria duas vezes"
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

    /// **A espera das rejeições tem teto, e a que não coube é contada.**
    ///
    /// Ela guarda cada promessa e a linha dela do lado de fora do motor, onde
    /// o teto de memória do MOD não alcança: sem teto, um MOD que rejeita num
    /// laço enche a memória do processo dentro de uma volta só. A que chega com
    /// a espera cheia não é guardada nem dita na hora: entra na conta, que sai
    /// no fim da volta. E sai da conta quando o tratamento dela chega, como as
    /// guardadas saem da espera.
    #[test]
    fn a_espera_das_rejeicoes_tem_teto_e_conta_a_que_nao_coube() {
        let runtime = Runtime::new().expect("o motor");
        let contexto = Context::full(&runtime).expect("o contexto");
        contexto.with(|ctx| {
            let promessa = || {
                rquickjs::Object::new(ctx.clone())
                    .expect("um objeto")
                    .into_value()
            };
            let mut rejeicoes = RejeicoesDaVolta::default();
            for n in 0..REJEICOES_GUARDADAS {
                rejeicoes.guardar(
                    &ctx,
                    promessa(),
                    format!("linha {n}"),
                    format!("falhou {n}"),
                );
            }
            let de_fora = [promessa(), promessa()];
            for ela in &de_fora {
                rejeicoes.guardar(
                    &ctx,
                    ela.clone(),
                    "a de fora".to_owned(),
                    "falhou de fora".to_owned(),
                );
            }
            assert_eq!(
                rejeicoes.guardadas.len(),
                REJEICOES_GUARDADAS,
                "a espera guardou além do teto de {REJEICOES_GUARDADAS}, ou recusou uma abaixo dele"
            );
            assert_eq!(
                rejeicoes.alem, 2,
                "as rejeições que chegaram com a espera cheia não foram contadas"
            );
            rejeicoes.tratada(&ctx, &de_fora[0]);
            assert_eq!(
                rejeicoes.alem, 1,
                "a rejeição contada além da espera ganhou tratamento e continuou na conta: o fim \
                 da volta diria «sem tratamento» sobre uma que o MOD pegou"
            );
            let (linhas, alem) = rejeicoes.sem_tratamento();
            assert_eq!(
                (linhas.len(), alem),
                (REJEICOES_GUARDADAS, 1),
                "a espera não devolveu, no fim da volta, todas as que guardou e a conta das de fora"
            );
            assert!(
                rejeicoes.guardadas.is_empty() && rejeicoes.alem == 0,
                "a espera não ficou vazia depois do fim da volta, e a próxima repetiria as linhas \
                 ou a conta"
            );
        });
    }

    /// **A linha que espera o fim da volta já cabe no teto do registro, e não
    /// é cortada de novo quando sai.**
    ///
    /// A linha leva a mensagem e a pilha da exceção, do tamanho que o MOD
    /// quiser: guardada inteira, a espera segurava até `REJEICOES_GUARDADAS`
    /// delas do lado de fora do motor, e o corte só vinha na saída (F-Mods-m2
    /// da revisão final do Plano 1). Cortada de novo na saída, ela perderia o
    /// aviso do primeiro corte e ganharia outro, com o tamanho já cortado.
    ///
    /// **E o `Falhou` da janela sai logo depois dela, cortado do mesmo jeito**
    /// (P51-01): ele espera o fim da volta com a linha, e não sai antes dela.
    #[test]
    fn a_linha_que_espera_o_fim_da_volta_ja_cabe_no_teto_e_sai_sem_novo_corte() {
        let runtime = Runtime::new().expect("o motor");
        let contexto = Context::full(&runtime).expect("o contexto");
        contexto.with(|ctx| {
            let promessa = rquickjs::Object::new(ctx.clone())
                .expect("um objeto")
                .into_value();
            let prefixo = format!("{REJEITADA_SEM_TRATAMENTO}: ");
            let mut rejeicoes = RejeicoesDaVolta::default();
            rejeicoes.guardar(
                &ctx,
                promessa,
                "x".repeat(1_000_000),
                format!("{prefixo}{}", "y".repeat(1_000_000)),
            );
            let aviso = " […cortado: 1000000 caracteres]";
            let guardada = match rejeicoes.guardadas.as_slice() {
                [Rejeitada { linha, .. }] => linha.0.clone(),
                outras => panic!("esperava uma linha na espera, há {}", outras.len()),
            };
            assert_eq!(
                guardada,
                format!("{}{aviso}", "x".repeat(crate::TETO_DA_FRASE_NO_REGISTRO)),
                "a linha entrou na espera sem o corte do registro: a espera segura o texto \
                 inteiro que o MOD lançou, do lado de fora do motor, até o fim da volta"
            );

            let (manda, recebe) = std::sync::mpsc::channel();
            let balde = std::cell::RefCell::new(BaldeDoConsole::novo(Instant::now()));
            let fila = Arc::new(Fila::default());
            dizer_as_rejeitadas(&std::cell::RefCell::new(rejeicoes), &balde, &manda, &fila);
            let Ok(ParaOFora::Console { texto, .. }) = recebe.try_recv() else {
                panic!("a linha guardada não saiu no fim da volta, ou saiu depois do `Falhou`");
            };
            assert_eq!(
                texto, guardada,
                "a linha guardada foi cortada de novo ao sair: o aviso do primeiro corte, que \
                 diz o tamanho do que o MOD lançou, foi trocado por outro"
            );
            let Ok(ParaOFora::Falhou(falhou)) = recebe.try_recv() else {
                panic!(
                    "o `Falhou` da rejeição não saiu depois da linha, no fim da volta: a janela \
                     não fica sabendo da promessa que ninguém pegou"
                );
            };
            let cabe = crate::TETO_DA_FRASE_NO_REGISTRO - prefixo.chars().count();
            assert_eq!(
                falhou,
                format!(
                    "{prefixo}{} […cortado: {} caracteres]",
                    "y".repeat(cabe),
                    prefixo.chars().count() + 1_000_000
                ),
                "o `Falhou` da rejeição não saiu no teto do registro, cortado uma vez só: ou o \
                 canal leva o texto inteiro que o MOD lançou, ou o aviso do corte mente o tamanho"
            );
        });
    }

    /// **A conta das rejeições além da espera chega à janela com o sujeito.**
    ///
    /// Na linha do registro ela vem depois das guardadas, e «e mais N
    /// rejeições…» se lê com elas. O `Falhou` vai para a gestão sem as linhas
    /// que o cercam no registro: sem «promessa rejeitada sem tratamento» na
    /// frente, «e mais 9 além da espera» não diz de quê.
    #[test]
    fn o_falhou_da_conta_alem_da_espera_diz_de_que_e_a_conta() {
        let (manda, recebe) = std::sync::mpsc::channel();
        let balde = std::cell::RefCell::new(BaldeDoConsole::novo(Instant::now()));
        let fila = Arc::new(Fila::default());
        let rejeicoes = RejeicoesDaVolta {
            guardadas: Vec::new(),
            alem: 3,
        };
        dizer_as_rejeitadas(&std::cell::RefCell::new(rejeicoes), &balde, &manda, &fila);
        let Ok(ParaOFora::Console { texto, .. }) = recebe.try_recv() else {
            panic!("a linha da conta além da espera não saiu, ou saiu depois do `Falhou`");
        };
        assert_eq!(
            texto, "e mais 3 rejeições sem tratamento além da espera",
            "a linha do registro da conta além da espera mudou"
        );
        let Ok(ParaOFora::Falhou(falhou)) = recebe.try_recv() else {
            panic!("a conta além da espera não chegou à janela como `Falhou`");
        };
        assert_eq!(
            falhou,
            format!("{REJEITADA_SEM_TRATAMENTO}: e mais 3 além da espera"),
            "o `Falhou` da conta além da espera não diz de que é a conta, e a gestão mostra «e \
             mais 3» sem sujeito"
        );
    }
}
