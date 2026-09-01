//! A bomba: quem gira o laço da tela, e onde ele gira.
//!
//! [`crate::video`] sabe transformar um quadro capturado em [`QuadroCodificado`]
//! e [`crate::tela`] sabe escrever bytes num fluxo QUIC dentro de um teto.
//! **Nada chamava os dois em sequência**, e o sintoma era exato: apertar
//! «compartilhar» devolvia `ScreenShareUnavailable` com todas as peças prontas
//! na máquina. Este módulo é o laço que faltava.
//!
//! # As quatro decisões que este arquivo carrega
//!
//! 1. **Uma thread do sistema operacional, e não uma tarefa do runtime.** O §2
//!    da spec de compartilhamento de tela é explícito: *«o encoder mora numa
//!    thread própria, com prioridade abaixo do normal, e **nunca** no runtime
//!    que carrega os datagramas de voz nem perto do caminho de áudio»*. Uma
//!    `tokio::task` — mesmo com `spawn_blocking` — mora no mesmo executor que
//!    entrega os datagramas de voz; a única maneira de a promessa ser verdade é
//!    a thread ser nossa. O que este crate **não** consegue fazer é baixar a
//!    prioridade dela: `unsafe_code` é `forbid` no workspace e não existe API
//!    segura de prioridade de thread na `std`. Por isso [`ligar`] recebe
//!    `ao_nascer`, que roda **na thread nova** antes do primeiro quadro — é o
//!    lugar de quem tem a exceção nomeada (o `seele-ffi`, as bindings de áudio)
//!    chamar `setpriority`/`SetThreadPriority`. Quem não tem passa `|| {}`, e a
//!    ausência fica visível na chamada em vez de ficar escondida aqui.
//!
//! 2. **A thread não escreve na rede.** Ela devolve [`EventoDaBomba`] por um
//!    canal e quem tem a conexão escreve — [`escoar`]. Codificar é CPU e
//!    escrever é E/S; juntar os dois poria o encoder do lado de dentro do
//!    runtime da voz de novo, pela porta dos fundos.
//!
//! 3. **Trocar de degrau é recomeçar três coisas, não ajustar uma.** A
//!    resolução mora no cabeçalho de abertura do fluxo (§3.6), a
//!    `SCStreamConfiguration` é armada com largura e altura fixas, e o
//!    [`Codificador`](seele_video::codec::Codificador) recusa quadro de outro
//!    tamanho. Então um degrau novo é **captura nova, codificador novo, fluxo
//!    novo**, e é isso que a [`geração`](QuadroParaOFio::geracao) numera: um
//!    quadro só vale no fluxo da geração dele. O gatilho é o
//!    [`Ajuste::ResolucaoPedida`] que a onda 2 deixou pronto.
//!
//! 4. **Descarta, nunca enfileira — inclusive na entrega.** A regra é do §1 e
//!    `spikes/tela-no-codec` a mediu: enfileirando, a idade do quadro que sai
//!    cresce sem limite (958 ms de mediana em oito segundos, e nada nesse
//!    caminho para de crescer); descartando, ela fica em 3 ms. A captura já
//!    obedece por dentro, e aqui a mesma regra vale na fronteira com quem
//!    escreve: quadro que não cabe no canal é **largado e contado**
//!    ([`Bomba::largados`]), nunca acumulado. Os eventos de controle é que
//!    esperam — ver `pendentes`, e o comentário lá diz por que a ordem entre
//!    eles e os quadros não pode se perder.
//!
//! # O que este módulo **não** faz
//!
//! **Não mede o caminho, e agora alguém mede.** Quem mede é a
//! [`crate::caminho::Sonda`], do outro lado do canal: ela lê os contadores do
//! transporte enquanto **esta bomba** enche o cano — a tela é a coisa que
//! enche, e era essa a pergunta 2 do §8. O que chega aqui continua sendo o
//! resultado pronto, um [`TetoDeVideo`] com as três pernas do §5.1 já dentro
//! ([`crate::state::Room::teto_de_video`] junta os espectadores, a subida de
//! quem hospeda e a medida desta máquina), mais a [`SignalBand`] à parte.
//!
//! E a divisão de trabalho não mudou, porque ela é de propósito: RTT, jitter e
//! perda (ADR 0024) dizem se o caminho está **doendo**, e é a voz que os lê; o
//! `PathStats` do `quinn` diz quanto o caminho **aguentou**, e é a sonda que o
//! lê. São duas perguntas diferentes, e responder uma com a outra seria
//! inventar uma medida — ou cobrar duas vezes pelo mesmo sintoma, que é o que o
//! cabeçalho de [`crate::caminho`] chama de segunda armadilha.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use seele_proto::ids::ScreenId;
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource};
use seele_proto::signal::SignalBand;
use seele_proto::version::PROTOCOL_VERSION;
use seele_video::codec::{Cadencia, QuadroCodificado, Resolucao};
use seele_video::BibliotecaDeVideo;
use tokio::sync::mpsc as canal;

use crate::tela::{intervalo_de_quadro, ErroDeTela, MotivoDeParada, TetoDeVideo, Transmissao};
use crate::video::{
    Ajuste, Captura, CapturaRecusou, Compartilhamento, ErroDeCompartilhamento, FonteDeQuadros as _,
    Passo,
};

/// O nome da thread do codificador.
///
/// Fixo e público porque é por ele que se acha o culpado num `sample`, num
/// Instruments ou num despejo de pilha — uma thread sem nome num relatório de
/// travamento é uma thread que ninguém sabe de quem é. E porque quem tiver como
/// baixar a prioridade dela de fora precisa saber o que procurar.
pub const NOME_DA_THREAD: &str = "seele-tela";

/// Quantos eventos cabem no canal entre a bomba e quem escreve no fio.
///
/// Oito, e o número tem forma: a 30 quadros por segundo são 266 ms de folga, que
/// é mais do que a fila de 262 ms do gargalo que `spikes/tela-no-transporte`
/// mediu. Maior que isso seria a fila que o §1 recusa; menor faria uma pausa de
/// escalonamento virar quadro largado.
pub const CAPACIDADE_DO_CANAL: usize = 8;

/// Quantas vezes a thread tenta entregar o próprio [`EventoDaBomba::Fim`].
///
/// Vinte tentativas de 5 ms são 100 ms de teimosia, que é o teto de quanto
/// [`Bomba::parar`] pode esperar por causa disto. O número existe para haver
/// teto: sem ele, um consumidor vivo e travado prenderia a junção para sempre.
const TENTATIVAS_DO_FIM: u32 = 20;

/// Quanto se espera entre duas tentativas de entregar o fim.
const ESPERA_ENTRE_TENTATIVAS: Duration = Duration::from_millis(5);

// ---------------------------------------------------------------------------
// O que entra e o que sai
// ---------------------------------------------------------------------------

/// O que se manda para a bomba.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordem {
    /// O teto andou.
    ///
    /// Uma ordem só para as três coisas que o mexem, porque as três chegam pelo
    /// mesmo lugar e nenhuma delas é mais verdadeira que a outra: o sinal da voz
    /// mudando de faixa, alguém entrando ou saindo da sala (`ScreenViewers`), e
    /// a subida de quem hospeda sendo medida de novo (`HostUplink`). O N já mora
    /// **dentro** do [`TetoDeVideo`] pela perna de quem hospeda (§5.1) — quem
    /// mandasse a contagem por fora estaria pedindo à bomba que refizesse a
    /// conta que [`crate::tela`] já faz.
    Teto {
        /// O teto de agora, com as três pernas do §5.1 dentro dele.
        teto: TetoDeVideo,
        /// A faixa em que o sinal da voz está.
        faixa: SignalBand,
    },
    /// Alguém que está assistindo não tem de que predizer (§3.3).
    ///
    /// Sob demanda e nunca periódico: um quadro-chave de 1080p custa 65 KiB,
    /// quatro vezes um quadro comum, que são 446 ms do orçamento de 1200 kbps.
    Chave,
    /// A pessoa mexeu nos próprios tetos (§5).
    ///
    /// Separada de [`Self::Teto`] porque as duas mexem em coisas diferentes e
    /// por motivos diferentes: aquela é a medida — o que a rede e a voz
    /// permitem agora — e esta é a escolha, que é teto e nunca piso. Quem as
    /// juntasse numa só faria a próxima medida apagar a escolha da pessoa, ou o
    /// contrário.
    ///
    /// **Recomeça o fluxo.** Uma resolução nova pede um cabeçalho novo, e o
    /// cabeçalho vai na abertura: não há como dizer «daqui para frente é 720p»
    /// dentro de um fluxo que abriu dizendo 1080p. Quem assiste vê a imagem
    /// piscar uma vez, e é o preço honesto de trocar de tamanho.
    Escolha {
        /// A altura máxima escolhida.
        resolucao: Resolucao,
        /// A cadência máxima escolhida.
        cadencia: Cadencia,
    },
    /// A pessoa apertou parar.
    Parar,
}

/// Um quadro codificado, endereçado a um fluxo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroParaOFio {
    /// De que fluxo este quadro é.
    ///
    /// **Sem isto o degrau novo entrega lixo.** Entre o instante em que a bomba
    /// refaz o codificador e o instante em que quem tem a conexão abre o fluxo
    /// novo cabe um quadro do codificador antigo; escrevê-lo no cabeçalho novo
    /// faria quem recebe decodificar 1080p como se fosse 720p. A geração é o
    /// que deixa [`escoar`] jogá-lo fora sem precisar adivinhar.
    pub geracao: u64,
    /// Se dá para começar a decodificar por ele.
    pub chave: bool,
    /// Os bytes, em Annex-B, como o encoder os produziu.
    pub bytes: Vec<u8>,
}

/// O que a bomba tem a dizer.
///
/// **A ordem entre eles é o contrato.** Um [`EventoDaBomba::Fluxo`] sempre chega
/// antes do primeiro quadro daquela geração, e um quadro nunca chega antes do
/// fluxo dele. É o que permite a [`escoar`] ser um laço sem estado escondido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoDaBomba {
    /// Um fluxo novo tem de ser aberto antes do próximo quadro.
    ///
    /// Sai na abertura e a cada troca de degrau, e as duas são a mesma coisa
    /// pelo motivo do §3.6: a resolução mora no cabeçalho, então mudá-la é abrir
    /// outro fluxo. Um caminho só para os dois casos é um caminho só para errar.
    Fluxo {
        /// A geração que os próximos quadros vão carregar.
        geracao: u64,
        /// O degrau com que o codificador foi armado — o que **está saindo**, e
        /// não o que foi pedido (§5).
        resolucao: Resolucao,
        /// O teto que está valendo, para o balde do fluxo novo.
        teto_bps: u32,
    },
    /// Só a banda mudou; o fluxo continua o mesmo.
    ///
    /// É um `SetOption` no encoder e um balde reajustado no fluxo, e nenhum dos
    /// dois custa quadro-chave — que é a razão de [`Ajuste`] separar este caso
    /// do de cima.
    Teto {
        /// O teto que passou a valer.
        teto_bps: u32,
    },
    /// Um quadro pronto para [`Transmissao::enviar_quadro`].
    Quadro(QuadroParaOFio),
    /// Um pacote de som já codificado, para ir junto com a imagem.
    ///
    /// Separado do [`Self::Quadro`] porque o destino no fio é outro tipo de
    /// quadro, e porque ele **não passa pelo teto**: o som custa menos de 3% do
    /// orçamento do vídeo e é a metade da transmissão que continua útil quando a
    /// imagem engasga.
    Som(Vec<u8>),
    /// Um tique sem quadro novo, para o que ficou pela metade poder andar.
    ///
    /// Existe por um defeito de campo: «a tela ficou travada pra mim que estou
    /// compartilhando e pra quem tá assistindo, em um frame só».
    ///
    /// Um quadro-chave viaja em [`FATIAS_DO_QUADRO_CHAVE`] fatias, e
    /// [`Transmissao::enviar_quadro`] escreve **uma por chamada** — e só é
    /// chamada quando um quadro novo chega. Nos dois tiques que não produzem
    /// quadro, `SemQuadro` e `PuladoPeloTeto`, ela não era chamada, e o
    /// quadro-chave em voo parava no ar. Enquanto ele está em voo todo quadro
    /// que chega é descartado, inclusive o do espelho local: por isso quem
    /// compartilha congela junto com quem assiste, e não sobra nem sintoma que
    /// separe os dois.
    ///
    /// Não é preciso a tela estar parada. `PuladoPeloTeto` é o controle de taxa
    /// do OpenH264 pulando por conta própria — 16,2% dos quadros em 1080p no
    /// teto de 1200 kbps —, e quatro pulos seguidos bastam.
    ///
    /// É a metade que faltava do conserto que o som já tinha: «um tique sem
    /// imagem nova — a tela parada — não é um tique sem som» está escrito em
    /// [`Bomba::escoar_som`] desde então. Também não é um tique sem fatia.
    ///
    /// [`FATIAS_DO_QUADRO_CHAVE`]: crate::tela::FATIAS_DO_QUADRO_CHAVE
    Escoar,
    /// O vídeo parou, com motivo (§3.2).
    ///
    /// **A bomba continua viva.** Parar é a resposta do produto a um sinal
    /// crítico ou a um teto abaixo do piso, e as duas coisas voltam sozinhas: a
    /// próxima [`Ordem::Teto`] que couber rearma o codificador e sai um
    /// [`EventoDaBomba::Fluxo`] novo. Uma bomba que morresse aqui obrigaria a
    /// casca a distinguir «parou porque o sinal caiu» de «parou porque a pessoa
    /// apertou», que é justamente a distinção que o §3.6 quer manter.
    Parou(MotivoDeParada),
    /// A thread acabou, e com que erro se houve um.
    ///
    /// Sempre o último. `None` é o fim pedido — [`Ordem::Parar`], ou a
    /// [`Bomba`] largada.
    Fim(Option<String>),
}

// ---------------------------------------------------------------------------
// O arranjo e a alça
// ---------------------------------------------------------------------------

/// Com o que a bomba começa.
///
/// Uma estrutura e não quatro argumentos porque três deles são teto (§5) e o
/// quarto é a medida por baixo — vê-los juntos na chamada é o que impede alguém
/// de passar a escolha da pessoa onde vai o degrau do teto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arranjo {
    /// O teto, com as três pernas do §5.1.
    pub teto: TetoDeVideo,
    /// A faixa em que o sinal da voz está agora.
    pub faixa: SignalBand,
    /// A resolução que a pessoa escolheu, que é teto e nunca piso (§5).
    pub escolha_de_resolucao: Resolucao,
    /// A cadência que a pessoa escolheu, também teto.
    pub cadencia: Cadencia,
    /// O que cede primeiro quando o orçamento aperta. Ver
    /// [`seele_core::tela::Prioridade`](crate::tela::Prioridade).
    pub prioridade: crate::tela::Prioridade,
}

/// A alça de uma bomba viva.
///
/// Largar a alça para a bomba: o [`Drop`] manda [`Ordem::Parar`] e espera a
/// thread. É a segunda maneira de parar, e existe para que um caminho de erro
/// que esqueça de parar não deixe uma thread codificando uma tela que ninguém
/// está vendo.
#[derive(Debug)]
pub struct Bomba {
    ordens: mpsc::Sender<Ordem>,
    thread: Option<JoinHandle<()>>,
    largados: Arc<AtomicU64>,
}

impl Bomba {
    /// Diz que o teto andou.
    ///
    /// Devolve `false` quando a thread já acabou — o que não é erro: é a mesma
    /// coisa que o `Fim` que já saiu pelo canal de eventos disse.
    pub fn teto(&self, teto: TetoDeVideo, faixa: SignalBand) -> bool {
        self.ordens.send(Ordem::Teto { teto, faixa }).is_ok()
    }

    /// Diz que a pessoa mexeu nos próprios tetos. Ver [`Ordem::Escolha`].
    ///
    /// Devolve `false` quando a thread já acabou, como [`Self::teto`].
    pub fn escolha(&self, resolucao: Resolucao, cadencia: Cadencia) -> bool {
        self.ordens
            .send(Ordem::Escolha {
                resolucao,
                cadencia,
            })
            .is_ok()
    }

    /// Pede um quadro-chave (§3.3).
    ///
    /// Pedir duas vezes antes do próximo tique produz **um** quadro-chave: a
    /// bomba guarda uma vaga e não uma fila, pela mesma conta que
    /// [`crate::state::Room::chave_pedida`] faz do outro lado.
    pub fn chave(&self) -> bool {
        self.ordens.send(Ordem::Chave).is_ok()
    }

    /// Quantos quadros foram largados por quem escreve estar atrasado.
    ///
    /// Não é perda de rede e não é erro: é o §1 valendo na fronteira. Vale para
    /// a tela de diagnóstico, ao lado dos descartes do próprio teto.
    #[must_use]
    pub fn largados(&self) -> u64 {
        self.largados.load(Ordering::Relaxed)
    }

    /// Para a bomba e espera a thread acabar.
    ///
    /// Bloqueia por no máximo um intervalo de quadro — 200 ms no pior caso, que
    /// é o piso de 5 quadros por segundo. Chamar isto é melhor que largar a alça
    /// só por ser explícito; o [`Drop`] faz o mesmo.
    pub fn parar(mut self) {
        self.encerrar();
    }

    fn encerrar(&mut self) {
        let _ = self.ordens.send(Ordem::Parar);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Bomba {
    fn drop(&mut self) {
        self.encerrar();
    }
}

/// Liga a bomba: cria a thread e devolve a alça e o canal de eventos.
///
/// `ao_nascer` roda **na thread nova**, antes de qualquer captura, e é onde a
/// prioridade do §2 é baixada por quem tiver como — ver o cabeçalho deste
/// módulo. `|| {}` é uma resposta legítima, e é a que este crate consegue dar
/// sozinho.
///
/// Nada de vídeo acontece aqui: a captura e o codificador nascem **dentro** da
/// thread, e o primeiro [`EventoDaBomba::Fluxo`] é o que diz que deu certo. Um
/// erro de armar volta por [`EventoDaBomba::Fim`], pelo mesmo caminho de todo o
/// resto — quem chama tem um lugar só para olhar em vez de dois.
///
/// # Errors
///
/// [`std::io::Error`] se o sistema não cria a thread.
pub fn ligar<C>(
    biblioteca: BibliotecaDeVideo,
    captura: C,
    arranjo: Arranjo,
    ao_nascer: impl FnOnce() + Send + 'static,
) -> std::io::Result<(Bomba, canal::Receiver<EventoDaBomba>)>
where
    C: Captura,
{
    let (manda_ordem, recebe_ordem) = mpsc::channel();
    let (manda_evento, recebe_evento) = canal::channel(CAPACIDADE_DO_CANAL);
    let largados = Arc::new(AtomicU64::new(0));

    let laco = Laco {
        // Um codificador por transmissão, com o teto de voz. `None` é uma
        // máquina cujo Opus não abriu: a imagem continua indo, muda.
        som: seele_audio::codec::VoiceEncoder::with_defaults().ok(),
        conta_do_som: (0, 0),
        sobra: VecDeque::new(),
        biblioteca,
        captura,
        fonte: None,
        arranjo,
        compartilhamento: None,
        geracao: 0,
        pendentes: VecDeque::new(),
        fechado: false,
        ordens: recebe_ordem,
        para: manda_evento,
        largados: Arc::clone(&largados),
    };

    let thread = thread::Builder::new()
        .name(NOME_DA_THREAD.to_owned())
        .spawn(move || {
            ao_nascer();
            laco.rodar();
        })?;

    Ok((
        Bomba {
            ordens: manda_ordem,
            thread: Some(thread),
            largados,
        },
        recebe_evento,
    ))
}

// ---------------------------------------------------------------------------
// O laço, que mora na thread
// ---------------------------------------------------------------------------

/// O que fazer depois de uma ordem.
enum Andamento {
    /// Continuar.
    Segue,
    /// Acabou — a pessoa pediu, ou não há mais ninguém do outro lado.
    Para,
}

struct Laco<C: Captura> {
    biblioteca: BibliotecaDeVideo,
    captura: C,
    fonte: Option<C::Fonte>,
    arranjo: Arranjo,
    compartilhamento: Option<Compartilhamento>,
    geracao: u64,
    /// Eventos de controle que não couberam no canal ainda.
    ///
    /// **Eles esperam e os quadros não**, e a assimetria é o ponto: perder um
    /// [`EventoDaBomba::Fluxo`] faria os quadros da geração seguinte serem
    /// escritos num cabeçalho que anuncia outra resolução, enquanto perder um
    /// quadro é a política desta casa desde `specs/03-audio.md`. Enquanto houver
    /// pendente, nenhum quadro passa na frente — é o que mantém a ordem que o
    /// [`EventoDaBomba`] promete.
    pendentes: VecDeque<EventoDaBomba>,
    fechado: bool,
    ordens: mpsc::Receiver<Ordem>,
    para: canal::Sender<EventoDaBomba>,
    largados: Arc<AtomicU64>,
    /// O codificador do som da tela, quando esta máquina tem um.
    ///
    /// `None` é uma transmissão muda, e não uma transmissão que não sai: a
    /// imagem é o assunto, e o som é o que a acompanha.
    som: Option<seele_audio::codec::VoiceEncoder>,
    /// Quantas amostras a captura já entregou, e quantos pacotes saíram.
    ///
    /// **Existe para o log dizer onde o som parou**, e não para telemetria. As
    /// duas metades separadas porque elas apontam para lugares diferentes: zero
    /// amostras é a captura não entregando — o dispositivo não abriu, ou o
    /// sistema não empresta a saída. Amostras sem pacotes é tudo o que chegou
    /// ser silêncio exato, ou o codificador recusando.
    ///
    /// Sem os dois separados, «a transmissão saiu muda» é uma frase sem lugar
    /// para procurar — e foi assim que ela chegou de campo duas vezes.
    conta_do_som: (u64, u64),
    /// As amostras que sobraram de um pacote de 20 ms para o próximo.
    ///
    /// A captura entrega o que juntou desde o último tique, e o tique do vídeo
    /// não é múltiplo do passo do codec. Sem esta sobra, cada tique jogaria fora
    /// o resto — que é um pedaço de som a cada 33 ms, ou seja, um chiado.
    sobra: VecDeque<f32>,
}

impl<C: Captura> Laco<C> {
    fn rodar(mut self) {
        let erro = self.girar();
        // **O `Fim` é a única mensagem que não pode ser largada**, e por isso
        // ele é o único ponto deste arquivo que insiste: é ela que distingue
        // «a thread parou porque mandaram» de «a thread morreu», e quem espera
        // não tem outra maneira de saber qual das duas foi. Insiste por pouco
        // tempo e com teto — o canal pode estar cheio de quadros que ninguém
        // teve tempo de ler —, e desiste calado se o outro lado sumiu, porque
        // aí o fim já aconteceu de fato.
        let mut evento = EventoDaBomba::Fim(erro);
        for _ in 0..TENTATIVAS_DO_FIM {
            match self.para.try_send(evento) {
                Ok(()) => return,
                Err(canal::error::TrySendError::Full(devolvido)) => {
                    evento = devolvido;
                    thread::sleep(ESPERA_ENTRE_TENTATIVAS);
                }
                Err(canal::error::TrySendError::Closed(_)) => return,
            }
        }
    }

    /// O laço inteiro. Devolve o erro que o matou, se houve um.
    fn girar(&mut self) -> Option<String> {
        let mut pedido_de_chave = false;

        if let Err(erro) = self.armar() {
            return Some(erro);
        }
        let mut proximo = Instant::now();

        loop {
            // Parado não gira em vazio: sem teto não há o que codificar, e
            // queimar CPU para não produzir nada é exatamente a disputa com a
            // voz que o §2 proíbe. Espera por uma ordem, que é a única coisa
            // que pode desparar.
            while self.compartilhamento.is_none() {
                let Ok(ordem) = self.ordens.recv() else {
                    return None;
                };
                match self.aplicar(ordem, &mut pedido_de_chave) {
                    Ok(Andamento::Segue) => {}
                    Ok(Andamento::Para) => return None,
                    Err(erro) => return Some(erro),
                }
                if self.fechado {
                    return None;
                }
                proximo = Instant::now();
            }

            loop {
                match self.ordens.try_recv() {
                    Ok(ordem) => match self.aplicar(ordem, &mut pedido_de_chave) {
                        Ok(Andamento::Segue) => {}
                        Ok(Andamento::Para) => return None,
                        Err(erro) => return Some(erro),
                    },
                    Err(mpsc::TryRecvError::Empty) => break,
                    // A alça morreu sem passar por `parar`. É o mesmo fim.
                    Err(mpsc::TryRecvError::Disconnected) => return None,
                }
            }
            if self.fechado {
                return None;
            }
            // Uma ordem pode ter parado o vídeo. Volta ao topo para esperar em
            // vez de codificar sem teto.
            if self.compartilhamento.is_none() {
                continue;
            }

            self.escoar_pendentes();
            match self.tique(pedido_de_chave) {
                Ok(Passo::SemQuadro | Passo::PuladoPeloTeto) => self.pedir_escoamento(),
                Ok(Passo::Quadro(codificado)) => {
                    // O pedido só é atendido quando o quadro-chave de fato saiu:
                    // o controle de taxa do OpenH264 pula quadros por conta
                    // própria — 16,2% em 1080p no teto de 1200 kbps —, e apagar
                    // o pedido num quadro pulado deixaria quem pediu esperando
                    // um quadro-chave que nunca foi feito.
                    if codificado.chave {
                        pedido_de_chave = false;
                    }
                    self.entregar(codificado);
                }
                Err(erro) => return Some(erro.to_string()),
            }
            // O som, a cada tique, e **fora** do `match` do quadro: um tique
            // sem imagem nova — a tela parada — não é um tique sem som.
            self.escoar_som();

            if self.fechado {
                return None;
            }

            proximo = self.dormir(proximo);
        }
    }

    /// Um tique do codificador, com os empréstimos separados por campo.
    /// Tira o som que a captura juntou e o entrega em pacotes de 20 ms.
    ///
    /// # Por que aqui, e não numa thread própria
    ///
    /// Porque é aqui que a captura está viva. Uma thread só para o som teria de
    /// receber a fonte, e a fonte é a mesma do vídeo no macOS — o som sai do
    /// `SCStream` da imagem. Duas donas do mesmo objeto seriam dois lugares
    /// para o mesmo ciclo de vida.
    ///
    /// O ritmo do vídeo é o do som: a 30 quadros por segundo este laço acorda a
    /// cada 33 ms e entrega um ou dois pacotes de 20 ms. Não é jitter que
    /// alguém ouça — o outro lado recebe por fluxo ordenado e toca em fila —, e
    /// o custo de acordar uma thread a mais para 32 kbps não se paga.
    ///
    /// # Silêncio não viaja
    ///
    /// Um pacote todo em zero é descartado antes de entrar no fio. Quem
    /// compartilha uma janela parada não paga nada, e o Opus já distingue
    /// silêncio de som baixo melhor do que um limiar aqui distinguiria.
    fn escoar_som(&mut self) {
        let Some(fonte) = self.fonte.as_ref() else {
            return;
        };
        if self.som.is_none() {
            return;
        }
        let chegaram = fonte.tomar_som();
        let primeiras = self.conta_do_som.0 == 0 && !chegaram.is_empty();
        self.conta_do_som.0 += chegaram.len() as u64;
        if primeiras {
            tracing::info!("o som da tela começou a chegar da captura");
        }
        self.sobra.extend(chegaram);

        // Os pacotes são feitos antes de qualquer um sair, para o empréstimo do
        // codificador acabar antes de `entregar_evento` pegar `self` de novo.
        let mut pacotes = Vec::new();
        while self.sobra.len() >= seele_audio::FRAME_SAMPLES {
            let quadro: Vec<f32> = self.sobra.drain(..seele_audio::FRAME_SAMPLES).collect();
            // Silêncio exato é o que uma máquina que não toca nada produz, e
            // mandá-lo é gastar 32 kbps para dizer «nada».
            if quadro.iter().all(|amostra| *amostra == 0.0) {
                continue;
            }
            if let Some(Ok(pacote)) = self.som.as_mut().map(|codec| codec.encode(&quadro)) {
                pacotes.push(pacote);
            }
        }
        let primeiros = self.conta_do_som.1 == 0 && !pacotes.is_empty();
        self.conta_do_som.1 += pacotes.len() as u64;
        if primeiros {
            tracing::info!("o som da tela começou a sair para o fio");
        }
        for pacote in pacotes {
            self.entregar_evento(EventoDaBomba::Som(pacote));
        }
    }

    fn tique(&mut self, pedido_de_chave: bool) -> Result<Passo, ErroDeCompartilhamento> {
        let (Some(compartilhamento), Some(fonte)) =
            (self.compartilhamento.as_mut(), self.fonte.as_ref())
        else {
            return Ok(Passo::SemQuadro);
        };
        compartilhamento.passo(fonte, pedido_de_chave)
    }

    /// Dorme até o próximo tique e devolve o instante do tique seguinte.
    ///
    /// **Não acumula dívida.** Se um tique demorou mais que o intervalo — o
    /// quadro-chave de 1080p custa 8,4 ms, e uma máquina que estrangula por calor
    /// custa mais —, o relógio é reancorado no agora em vez de a bomba tentar
    /// «recuperar» disparando tiques em rajada. Recuperar aqui seria pedir à CPU
    /// justamente no instante em que ela já não está dando conta.
    fn dormir(&self, proximo: Instant) -> Instant {
        let intervalo = self
            .compartilhamento
            .as_ref()
            .map_or(Duration::ZERO, |compartilhamento| {
                intervalo_de_quadro(compartilhamento.quadros_por_segundo())
            });
        let alvo = proximo + intervalo;
        let agora = Instant::now();
        if alvo > agora {
            thread::sleep(alvo - agora);
            alvo
        } else {
            agora
        }
    }

    fn aplicar(&mut self, ordem: Ordem, pedido_de_chave: &mut bool) -> Result<Andamento, String> {
        match ordem {
            Ordem::Parar => Ok(Andamento::Para),
            Ordem::Chave => {
                *pedido_de_chave = true;
                Ok(Andamento::Segue)
            }
            Ordem::Teto { teto, faixa } => {
                self.arranjo.teto = teto;
                self.arranjo.faixa = faixa;
                self.teto_andou()?;
                Ok(Andamento::Segue)
            }
            Ordem::Escolha {
                resolucao,
                cadencia,
            } => {
                // Nada a fazer quando a escolha é a que já vale: rearmar
                // recomeçaria o fluxo, e a imagem de todo mundo piscaria porque
                // alguém apertou APLICAR sem ter mexido em nada.
                if self.arranjo.escolha_de_resolucao == resolucao
                    && self.arranjo.cadencia == cadencia
                {
                    return Ok(Andamento::Segue);
                }
                self.arranjo.escolha_de_resolucao = resolucao;
                self.arranjo.cadencia = cadencia;
                // `armar` e não `teto_andou`: a escolha entra na conta lá
                // dentro, junto com o teto medido, e é ela que decide o degrau.
                // O `Compartilhamento` de agora foi construído com a escolha
                // velha e não sabe reconsiderá-la.
                self.armar()?;
                Ok(Andamento::Segue)
            }
        }
    }

    /// Reage a um teto novo, que é o §3.2 e o §5.1 virando movimento.
    fn teto_andou(&mut self) -> Result<(), String> {
        let (teto, faixa) = (self.arranjo.teto, self.arranjo.faixa);
        let ajuste = match self.compartilhamento.as_mut() {
            Some(compartilhamento) => compartilhamento.ajustar(&teto, faixa),
            // Estava parado. Um teto que agora cabe rearma tudo, e é assim que
            // um sinal que voltou devolve a tela sem ninguém apertar nada.
            None => return self.armar(),
        };
        match ajuste {
            Ok(Ajuste::Igual) => Ok(()),
            Ok(Ajuste::TetoNovo { teto_bps }) => {
                self.emitir(EventoDaBomba::Teto { teto_bps });
                Ok(())
            }
            // O degrau caiu ou subiu: captura nova, codificador novo, fluxo
            // novo. Nesta ordem — a captura primeiro, porque o codificador
            // recusa quadro do tamanho antigo e um quadro do tamanho antigo é o
            // que a captura antiga ainda tem na vaga.
            Ok(Ajuste::ResolucaoPedida { para, teto_bps, .. }) => {
                self.trocar_de_degrau(para, teto_bps)
            }
            Ok(Ajuste::Parou(motivo)) => {
                // A captura vai junto: continuar capturando uma tela que não
                // sai da máquina é gastar CPU e memória para nada, e no macOS
                // é manter o indicador do sistema aceso mentindo para quem
                // compartilha.
                self.compartilhamento = None;
                self.fonte = None;
                self.emitir(EventoDaBomba::Parou(motivo));
                Ok(())
            }
            Err(erro) => Err(erro.to_string()),
        }
    }

    fn trocar_de_degrau(&mut self, para: Resolucao, teto_bps: u32) -> Result<(), String> {
        let cadencia = self.arranjo.cadencia;
        self.fonte = None;
        let fonte = self
            .captura
            .iniciar(para, cadencia)
            .map_err(|erro: CapturaRecusou| erro.to_string())?;
        let Some(compartilhamento) = self.compartilhamento.as_mut() else {
            return Ok(());
        };
        compartilhamento
            .refazer_com(para)
            .map_err(|erro| erro.to_string())?;
        self.fonte = Some(fonte);
        self.geracao += 1;
        self.emitir(EventoDaBomba::Fluxo {
            geracao: self.geracao,
            resolucao: para,
            teto_bps,
        });
        Ok(())
    }

    /// Arma captura e codificador para o teto de agora.
    ///
    /// Um teto que mandou parar **não** é erro: sai como
    /// [`EventoDaBomba::Parou`] e a bomba fica esperando o próximo teto.
    fn armar(&mut self) -> Result<(), String> {
        let arranjo = self.arranjo;
        let compartilhamento = match Compartilhamento::abrir(
            self.biblioteca.clone(),
            &arranjo.teto,
            arranjo.faixa,
            arranjo.escolha_de_resolucao,
            arranjo.cadencia,
            arranjo.prioridade,
        ) {
            Ok(compartilhamento) => compartilhamento,
            Err(ErroDeCompartilhamento::Parado(motivo)) => {
                self.compartilhamento = None;
                self.fonte = None;
                self.emitir(EventoDaBomba::Parou(motivo));
                return Ok(());
            }
            Err(ErroDeCompartilhamento::Video(erro)) => return Err(erro.to_string()),
        };

        let resolucao = compartilhamento.resolucao();
        let teto_bps = compartilhamento.teto().bps();
        // Depois do codificador: se o OpenH264 recusar a configuração, não vale
        // ter acendido o indicador de captura do sistema para nada.
        let fonte = self
            .captura
            .iniciar(resolucao, compartilhamento.cadencia())
            .map_err(|erro: CapturaRecusou| erro.to_string())?;

        self.compartilhamento = Some(compartilhamento);
        self.fonte = Some(fonte);
        self.geracao += 1;
        self.emitir(EventoDaBomba::Fluxo {
            geracao: self.geracao,
            resolucao,
            teto_bps,
        });
        Ok(())
    }

    // -----------------------------------------------------------------------
    // A saída
    // -----------------------------------------------------------------------

    fn emitir(&mut self, evento: EventoDaBomba) {
        self.pendentes.push_back(evento);
        self.escoar_pendentes();
    }

    fn escoar_pendentes(&mut self) {
        while let Some(evento) = self.pendentes.pop_front() {
            match self.para.try_send(evento) {
                Ok(()) => {}
                Err(canal::error::TrySendError::Full(evento)) => {
                    self.pendentes.push_front(evento);
                    return;
                }
                Err(canal::error::TrySendError::Closed(_)) => {
                    self.pendentes.clear();
                    self.fechado = true;
                    return;
                }
            }
        }
    }

    /// Põe um evento na fila de saída, respeitando a ordem dos pendentes.
    /// Pede ao laço de envio que ande com o quadro-chave que ficou no ar.
    ///
    /// `try_send` cru e descarte no cheio, em vez de [`Self::entregar_evento`]:
    /// dois pedidos de escoar são o mesmo pedido, e enfileirá-los encheria a
    /// fila de cópias de uma ordem que o próximo tique repete de graça.
    ///
    /// E nada quando há pendente. A ordem entre os eventos é o contrato desta
    /// bomba — um `Fluxo` sempre antes do primeiro quadro dele —, e furar a fila
    /// para escoar seria pagar com a única garantia que o laço de envio tem. Uma
    /// fila com pendente também não é o caso que este pedido existe para
    /// resolver: ali o canal está cheio de quadro, e quadro escoa sozinho.
    fn pedir_escoamento(&mut self) {
        if !self.pendentes.is_empty() {
            return;
        }
        match self.para.try_send(EventoDaBomba::Escoar) {
            Ok(()) | Err(canal::error::TrySendError::Full(_)) => {}
            Err(canal::error::TrySendError::Closed(_)) => self.fechado = true,
        }
    }

    fn entregar_evento(&mut self, evento: EventoDaBomba) {
        if !self.pendentes.is_empty() {
            self.pendentes.push_back(evento);
            self.escoar_pendentes();
            return;
        }
        match self.para.try_send(evento) {
            Ok(()) => {}
            Err(canal::error::TrySendError::Full(evento)) => {
                self.pendentes.push_back(evento);
            }
            Err(canal::error::TrySendError::Closed(_)) => self.fechado = true,
        }
    }

    fn entregar(&mut self, codificado: QuadroCodificado) {
        // Ordem antes de tudo: um quadro que passasse na frente de um `Fluxo`
        // pendente seria escrito no cabeçalho errado do outro lado.
        if !self.pendentes.is_empty() {
            self.largados.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let quadro = QuadroParaOFio {
            geracao: self.geracao,
            chave: codificado.chave,
            bytes: codificado.bytes,
        };
        match self.para.try_send(EventoDaBomba::Quadro(quadro)) {
            Ok(()) => {}
            // Descarta, nunca enfileira. §1, e medido: enfileirar leva a idade
            // do quadro que sai de 3 ms a 958 ms de mediana, crescendo sem
            // limite.
            Err(canal::error::TrySendError::Full(_)) => {
                self.largados.fetch_add(1, Ordering::Relaxed);
            }
            Err(canal::error::TrySendError::Closed(_)) => self.fechado = true,
        }
    }
}

// ---------------------------------------------------------------------------
// O outro lado: quem escreve no fio
// ---------------------------------------------------------------------------

/// O que uma transmissão inteira somou.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Contagem {
    /// Quantos fluxos foram abertos. Um por degrau (§3.6).
    pub fluxos: u64,
    /// Quantos quadros saíram inteiros.
    pub enviados: u64,
    /// Quantos o teto ou o quadro-chave em voo descartaram.
    pub descartados: u64,
    /// Quantos bytes foram para o fio, cabeçalhos incluídos.
    pub bytes: u64,
}

/// Um punho da conexão que só sabe escoar uma tela.
///
/// Existe para que a conexão continue privada dentro do
/// [`Client`](crate::Client) e mesmo assim dê para escoar **de outra tarefa**:
/// [`escoar`] pede um empréstimo, e um empréstimo do `Client` não atravessa um
/// `tokio::spawn` — quem escoa vive tanto quanto a transmissão, e o motor do
/// enlace precisa continuar lendo a conexão enquanto isso.
///
/// É barato: uma [`quinn::Connection`] é um punho de `Arc`.
#[derive(Debug, Clone)]
pub struct Escoadouro {
    conexao: quinn::Connection,
}

impl Escoadouro {
    /// Um escoadouro sobre esta conexão.
    #[must_use]
    pub const fn nova(conexao: quinn::Connection) -> Self {
        Self { conexao }
    }

    /// O mesmo que [`escoar`], sobre a conexão que este punho segura.
    ///
    /// # Errors
    ///
    /// O que [`escoar`] devolve.
    pub async fn escoar(
        &self,
        tela: ScreenId,
        fonte: ScreenSource,
        eventos: &mut canal::Receiver<EventoDaBomba>,
    ) -> Result<Contagem, ErroDeTela> {
        escoar(&self.conexao, tela, fonte, eventos).await
    }

    /// O mesmo, com um espelho que vê o que está saindo.
    ///
    /// # Errors
    ///
    /// O que [`escoar`] devolve.
    pub async fn escoar_espelhado(
        &self,
        tela: ScreenId,
        fonte: ScreenSource,
        eventos: &mut canal::Receiver<EventoDaBomba>,
        espelho: impl FnMut(EspelhoDaTela<'_>),
    ) -> Result<Contagem, ErroDeTela> {
        escoar_com_espelho(&self.conexao, tela, fonte, eventos, espelho).await
    }
}

/// O que o espelho de quem transmite vê passar.
///
/// # Por que existe
///
/// O servidor não devolve a transmissão a quem a produziu — `VoiceRoom::ligar` tira o
/// autor da lista de espectadores de propósito, porque mandar de volta o que
/// acabou de subir é pagar a banda duas vezes. O efeito colateral é que quem
/// compartilha era a única pessoa da sala que não via o que estava mostrando.
///
/// Este espelho fecha isso do lado de cá: os quadros já existem, já estão
/// comprimidos, e passar um punho deles para a casca não custa nem um byte de
/// rede. É também o que permite conferir a transmissão inteira numa máquina só.
#[derive(Debug)]
pub enum EspelhoDaTela<'a> {
    /// Um fluxo abriu, com este tamanho.
    Abriu {
        /// Largura em pixels.
        largura: u16,
        /// Altura em pixels.
        altura: u16,
    },
    /// Um quadro foi escrito no fio.
    Quadro {
        /// Se dá para começar a decodificar por ele.
        chave: bool,
        /// O quadro, em Annex-B.
        bytes: &'a [u8],
    },
}

/// Escoa o que a bomba produz para dentro da conexão que já existe.
///
/// É a metade de E/S do laço, e mora do lado assíncrono de propósito: escrever
/// num fluxo QUIC é esperar, e esperar é o que um runtime faz bem. O que **não**
/// pode estar aqui é o encoder — ver o cabeçalho deste módulo.
///
/// Volta quando a bomba disser [`EventoDaBomba::Fim`] ou quando o canal fechar,
/// e fecha o fluxo aberto antes de voltar: o fim do fluxo é a segunda maneira de
/// dizer «parei» (§3.6).
///
/// # Errors
///
/// [`ErroDeTela`] quando a conexão não abre um fluxo, quando o cabeçalho não
/// passa em `ScreenHeader::check`, ou quando o fluxo morre no meio de um quadro.
pub async fn escoar(
    conexao: &quinn::Connection,
    tela: ScreenId,
    fonte: ScreenSource,
    eventos: &mut canal::Receiver<EventoDaBomba>,
) -> Result<Contagem, ErroDeTela> {
    escoar_com_espelho(conexao, tela, fonte, eventos, |_| {}).await
}

/// [`escoar`], com alguém olhando o que passa. Ver [`EspelhoDaTela`].
///
/// O espelho é chamado **depois** de o byte ir para o fluxo, e nunca antes:
/// mostrar a quem transmite um quadro que a rede recusou seria a única tela da
/// sala mentindo, e ela é justamente a de quem tem de decidir se para.
///
/// # Errors
///
/// As mesmas de [`escoar`].
pub async fn escoar_com_espelho(
    conexao: &quinn::Connection,
    tela: ScreenId,
    fonte: ScreenSource,
    eventos: &mut canal::Receiver<EventoDaBomba>,
    mut espelho: impl FnMut(EspelhoDaTela<'_>),
) -> Result<Contagem, ErroDeTela> {
    let mut transmissao: Option<Transmissao> = None;
    let mut geracao = 0_u64;
    let mut contagem = Contagem::default();

    while let Some(evento) = eventos.recv().await {
        match evento {
            EventoDaBomba::Fluxo {
                geracao: nova,
                resolucao,
                teto_bps,
            } => {
                if let Some(velha) = transmissao.take() {
                    somar(&mut contagem, &velha);
                    velha.encerrar();
                }
                let cabecalho = ScreenHeader {
                    version: PROTOCOL_VERSION,
                    screen: tela,
                    source: fonte,
                    codec: ScreenCodec::H264Baseline,
                    // Os três degraus do §5 cabem num `u16` com folga de uma
                    // ordem de grandeza; o `unwrap_or` está aqui porque
                    // `expect` é `deny` neste workspace e não porque o caso
                    // exista.
                    width: u16::try_from(resolucao.largura()).unwrap_or(u16::MAX),
                    height: u16::try_from(resolucao.altura()).unwrap_or(u16::MAX),
                };
                transmissao =
                    Some(Transmissao::abrir(conexao, cabecalho, teto_bps, Instant::now()).await?);
                geracao = nova;
                contagem.fluxos += 1;
                espelho(EspelhoDaTela::Abriu {
                    largura: cabecalho.width,
                    altura: cabecalho.height,
                });
            }
            EventoDaBomba::Teto { teto_bps } => {
                if let Some(viva) = transmissao.as_mut() {
                    viva.ajustar_teto(teto_bps, Instant::now());
                }
            }
            EventoDaBomba::Som(pacote) => {
                // Sem passar pelo teto e sem esperar quadro-chave em voo:
                // ver `Transmissao::enviar_som` sobre por que o som não cede.
                if let Some(viva) = transmissao.as_mut() {
                    viva.enviar_som(&pacote).await?;
                }
            }
            EventoDaBomba::Escoar => {
                if let Some(viva) = transmissao.as_mut() {
                    viva.escoar_chave().await?;
                }
            }
            EventoDaBomba::Quadro(quadro) => {
                // De um fluxo que já fechou. Escrevê-lo no fluxo de agora seria
                // entregar 1080p sob um cabeçalho que anuncia 720p.
                if quadro.geracao != geracao {
                    continue;
                }
                if let Some(viva) = transmissao.as_mut() {
                    let envio = viva
                        .enviar_quadro(&quadro.bytes, quadro.chave, Instant::now())
                        .await?;
                    // Só o que de fato saiu. Um quadro descartado pelo teto não
                    // chega a ninguém, e ver no próprio espelho um quadro que a
                    // sala não recebeu é o tipo de mentira que faz alguém
                    // concluir que está tudo bem enquanto ninguém vê nada.
                    if !matches!(envio, crate::tela::Envio::Descartado(_)) {
                        espelho(EspelhoDaTela::Quadro {
                            chave: quadro.chave,
                            bytes: &quadro.bytes,
                        });
                    }
                }
            }
            // §3.2: quem para é o vídeo. O fluxo fecha e a sala fica sabendo
            // pelo fim dele; a bomba continua viva esperando o teto voltar, e
            // quando voltar chega um `Fluxo` novo.
            EventoDaBomba::Parou(_) => {
                if let Some(velha) = transmissao.take() {
                    somar(&mut contagem, &velha);
                    velha.encerrar();
                }
            }
            EventoDaBomba::Fim(erro) => {
                if let Some(velha) = transmissao.take() {
                    somar(&mut contagem, &velha);
                    velha.encerrar();
                }
                return match erro {
                    Some(erro) => Err(ErroDeTela::Fluxo(erro)),
                    None => Ok(contagem),
                };
            }
        }
    }

    if let Some(velha) = transmissao.take() {
        somar(&mut contagem, &velha);
        velha.encerrar();
    }
    Ok(contagem)
}

fn somar(contagem: &mut Contagem, transmissao: &Transmissao) {
    let (enviados, descartados, bytes) = transmissao.contagem();
    contagem.enviados += enviados;
    contagem.descartados += descartados;
    contagem.bytes += bytes;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    use seele_video::codec::QuadroI420;
    use seele_video::modulo;

    use super::*;
    use crate::tela::{Recepcao, CAMINHO_DA_PROVA_BPS};
    use crate::video::FonteDeQuadros;

    /// Quanto um teste espera por um evento antes de desistir.
    ///
    /// Generoso de propósito: a bomba dorme um intervalo de quadro entre tiques
    /// (33 ms a 30 quadros) e uma máquina de integração contínua carregada
    /// atrasa mais que isso. Um teste que falha por escalonamento é um teste que
    /// ensina a gente a re-rodar em vez de a olhar.
    const PACIENCIA: Duration = Duration::from_secs(5);

    // -----------------------------------------------------------------------
    // As mentiras
    // -----------------------------------------------------------------------

    /// Uma captura de mentira, que anota em que degrau foi mandada começar.
    ///
    /// A anotação é o ponto: o §3.6 diz que trocar de degrau é recomeçar a
    /// captura, e sem esta lista o teste provaria só que o codificador mudou —
    /// que é a metade que não quebra.
    #[derive(Debug, Default, Clone)]
    struct CapturaDeMentira {
        inicios: Arc<Mutex<Vec<(usize, usize)>>>,
    }

    impl CapturaDeMentira {
        fn inicios(&self) -> Vec<(usize, usize)> {
            self.inicios.lock().unwrap().clone()
        }
    }

    /// Uma fonte que sempre tem quadro novo.
    ///
    /// **Não imita o descarte do §1 de propósito**, pela mesma razão que a
    /// `FonteDeMentira` de [`crate::video`] não imita: quem descarta é a captura
    /// de verdade, e uma imitação que descartasse provaria a imitação.
    #[derive(Debug)]
    struct FonteDeMentira {
        largura: usize,
        altura: usize,
        passo: AtomicUsize,
    }

    impl FonteDeQuadros for FonteDeMentira {
        fn tomar(&self) -> Option<QuadroI420> {
            let passo = self.passo.fetch_add(1, Ordering::Relaxed);
            Some(quadro(self.largura, self.altura, passo))
        }
    }

    impl Captura for CapturaDeMentira {
        type Fonte = FonteDeMentira;

        fn iniciar(
            &mut self,
            resolucao: Resolucao,
            _cadencia: Cadencia,
        ) -> Result<Self::Fonte, CapturaRecusou> {
            self.inicios
                .lock()
                .unwrap()
                .push((resolucao.largura(), resolucao.altura()));
            Ok(FonteDeMentira {
                largura: resolucao.largura(),
                altura: resolucao.altura(),
                passo: AtomicUsize::new(0),
            })
        }
    }

    /// Uma tela que não muda: a captura abre, e a fonte nunca tem quadro novo.
    ///
    /// Não é um caso de laboratório. É o §1: macOS e Windows só entregam quadro
    /// quando a imagem mudou, então uma janela parada produz exatamente isto —
    /// tique atrás de tique com `tomar()` devolvendo `None`.
    #[derive(Debug, Default, Clone)]
    struct CapturaParada;

    #[derive(Debug)]
    struct FonteParada;

    impl FonteDeQuadros for FonteParada {
        fn tomar(&self) -> Option<QuadroI420> {
            None
        }
    }

    impl Captura for CapturaParada {
        type Fonte = FonteParada;

        fn iniciar(
            &mut self,
            _resolucao: Resolucao,
            _cadencia: Cadencia,
        ) -> Result<Self::Fonte, CapturaRecusou> {
            Ok(FonteParada)
        }
    }

    /// Uma fonte muda de imagem e **com** som, para provar o elo seguinte.
    ///
    /// O som escoa fora do `match` do quadro — «um tique sem imagem nova não é
    /// um tique sem som» —, então não ter imagem não atrapalha: separa.
    #[derive(Debug, Default, Clone)]
    struct CapturaComTom;

    #[derive(Debug)]
    struct FonteComTom;

    impl FonteDeQuadros for FonteComTom {
        fn tomar(&self) -> Option<QuadroI420> {
            None
        }

        fn tomar_som(&self) -> Vec<f32> {
            // Um tom, e não uma constante: o silêncio exato é descartado de
            // propósito antes de entrar no fio, e um bloco de `0.5` repetido
            // passaria por qualquer coisa. Meio período de seno por amostra dá
            // um sinal que o Opus tem o que codificar.
            (0..seele_audio::FRAME_SAMPLES * 2)
                .map(|i| {
                    let fase = i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU;
                    fase.sin() * 0.25
                })
                .collect()
        }
    }

    impl Captura for CapturaComTom {
        type Fonte = FonteComTom;

        fn iniciar(
            &mut self,
            _resolucao: Resolucao,
            _cadencia: Cadencia,
        ) -> Result<Self::Fonte, CapturaRecusou> {
            Ok(FonteComTom)
        }
    }

    /// O som que a captura entrega chega ao fio como pacote Opus.
    ///
    /// O elo entre «a captura ouviu» e «o outro lado ouviu». O de baixo está
    /// provado em `seele-video`, onde o teste do macOS toca um som e exige
    /// ouvi-lo; deste ponto em diante quem responde é esta bomba, e o relato de
    /// campo — «coloquei uma música no computador que estava transmitindo e não
    /// ouvi» — não dizia em qual dos dois o som se perdia.
    #[test]
    fn o_som_da_captura_vira_pacote_no_fio() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        let fibra = TetoDeVideo::com_caminho(6_000_000)
            .com_caminho_de_quem_hospeda(6_000_000)
            .com_espectadores(1);
        let (bomba, mut eventos) = ligar(
            biblioteca,
            CapturaComTom,
            arranjo(fibra, SignalBand::Nominal, Resolucao::P1080),
            || {},
        )
        .expect("criar a thread da tela");

        let mut viu_som = false;
        for _ in 0..40 {
            match esperar(&mut eventos) {
                Some(EventoDaBomba::Som(pacote)) => {
                    assert!(!pacote.is_empty(), "o pacote de som saiu vazio");
                    viu_som = true;
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }
        drop(bomba);

        assert!(
            viu_som,
            "a captura entregou som e nenhum pacote saiu para o fio.\n\
             É a metade de baixo do relato «não ouvi»: a máquina ouviu, o \
             codificador de voz não produziu, e a transmissão sai muda sem erro."
        );
    }

    /// A tela parada tem de pedir o escoamento do que ficou pela metade.
    ///
    /// O defeito de campo: «a tela ficou travada pra mim que estou
    /// compartilhando e pra quem tá assistindo, em um frame só». O quadro-chave
    /// viaja em fatias e cada fatia precisa de uma chamada; as chamadas vinham
    /// só dos quadros novos; sem quadro novo a chave parava no ar. E enquanto
    /// ela está no ar todo quadro que chega é descartado — o do espelho local
    /// junto —, que é por que os dois lados congelam iguais.
    #[test]
    fn um_tique_sem_imagem_pede_para_escoar_o_que_ficou_no_ar() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        let fibra = TetoDeVideo::com_caminho(6_000_000)
            .com_caminho_de_quem_hospeda(6_000_000)
            .com_espectadores(1);
        let (bomba, mut eventos) = ligar(
            biblioteca,
            CapturaParada,
            arranjo(fibra, SignalBand::Nominal, Resolucao::P1080),
            || {},
        )
        .expect("criar a thread da tela");

        assert_eq!(
            esperar(&mut eventos),
            Some(EventoDaBomba::Fluxo {
                geracao: 1,
                resolucao: Resolucao::P1080,
                teto_bps: 3_600_000,
            })
        );
        assert_eq!(
            esperar(&mut eventos),
            Some(EventoDaBomba::Escoar),
            "a tela parada não pediu escoamento, e um quadro-chave em voo \
             ficaria no ar para sempre — com os dois lados congelados"
        );
        drop(bomba);
    }

    /// Um quadro com bordas duras, que é o conteúdo caro de uma tela de
    /// trabalho. Um quadro chapado sairia com trinta bytes e não provaria nada.
    fn quadro(largura: usize, altura: usize, passo: usize) -> QuadroI420 {
        let mut y = Vec::with_capacity(largura * altura);
        for linha in 0..altura {
            for coluna in 0..largura {
                let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
                y.push(if claro { 235 } else { 16 });
            }
        }
        let croma = vec![128; largura.div_ceil(2) * altura.div_ceil(2)];
        QuadroI420::novo(largura, altura, y, croma.clone(), croma)
            .expect("os planos de um I420 montado aqui")
    }

    /// Onde procurar o módulo do Cisco, na ordem: o que quem roda apontou,
    /// depois a pasta de build.
    fn pastas() -> Vec<PathBuf> {
        let mut pastas = Vec::new();
        if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
            let caminho = PathBuf::from(apontado);
            pastas.push(if caminho.is_dir() {
                caminho
            } else {
                caminho.parent().map_or(caminho.clone(), PathBuf::from)
            });
        }
        pastas.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target"),
        );
        pastas
    }

    /// A biblioteca, ou `None` com o motivo impresso.
    ///
    /// **Pula em vez de falhar, e o motivo é a licença**, o mesmo de
    /// `crate::video`: o módulo do Cisco não pode morar neste repositório, e um
    /// teste que o exigisse seria vermelho em toda máquina limpa.
    fn biblioteca() -> Option<BibliotecaDeVideo> {
        match BibliotecaDeVideo::procurar_e_carregar(&pastas()) {
            Ok(biblioteca) => Some(biblioteca),
            Err(motivo) => {
                let onde = modulo::publicado_para_este_sistema()
                    .map_or_else(|| "—".to_owned(), |m| m.url());
                // Ver `seele-video/tests/ida_e_volta.rs`: onde o codec é
                // exigido, faltar é falha e não licença para pular.
                // Só onde **há** módulo publicado. No Linux o Cisco não
                // publica nada, e ali pular é a resposta certa e não um buraco.
                assert!(
                    std::env::var_os("SEELE_EXIGE_CODEC").is_none()
                        || modulo::publicado_para_este_sistema().is_none(),
                    "SEELE_EXIGE_CODEC está ligado, este sistema tem módulo publicado \
                     e ele não está aqui: {motivo}.\n  Buscar: {onde}"
                );
                eprintln!(
                    "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe \
                     isso.\n  Busque {onde} e aponte-o com SEELE_OPENH264.\n  Ligue \
                     SEELE_EXIGE_CODEC para que faltar vire falha em vez de pulo."
                );
                None
            }
        }
    }

    /// O próximo evento, ou `None` se a paciência acabar.
    ///
    /// Sondagem e não `blocking_recv` porque um teste que espera para sempre não
    /// é um teste que falha: é uma bateria que trava.
    fn esperar(eventos: &mut canal::Receiver<EventoDaBomba>) -> Option<EventoDaBomba> {
        let limite = Instant::now() + PACIENCIA;
        loop {
            match eventos.try_recv() {
                Ok(evento) => return Some(evento),
                Err(canal::error::TryRecvError::Empty) => {
                    if Instant::now() >= limite {
                        return None;
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                Err(canal::error::TryRecvError::Disconnected) => return None,
            }
        }
    }

    /// O próximo evento que não é quadro.
    fn esperar_controle(eventos: &mut canal::Receiver<EventoDaBomba>) -> Option<EventoDaBomba> {
        loop {
            match esperar(eventos)? {
                EventoDaBomba::Quadro(_) | EventoDaBomba::Som(_) => {}
                outro => return Some(outro),
            }
        }
    }

    fn arranjo(teto: TetoDeVideo, faixa: SignalBand, escolha: Resolucao) -> Arranjo {
        Arranjo {
            prioridade: crate::tela::Prioridade::Nitidez,
            teto,
            faixa,
            escolha_de_resolucao: escolha,
            cadencia: Cadencia::Q30,
        }
    }

    // -----------------------------------------------------------------------
    // As provas
    // -----------------------------------------------------------------------

    /// **A razão desta onda existir**, num teste: a bomba nasce, gira, entrega
    /// quadros, e morre quando mandam.
    ///
    /// Antes dela `Connection::compartilhar_tela` devolvia `ScreenShareUnavailable`
    /// com todas as peças prontas na máquina — captura, codec, teto e transporte
    /// existiam e ninguém os chamava em sequência. Se este teste ficar vermelho,
    /// o recurso voltou a ser um botão que não faz nada.
    #[test]
    fn a_bomba_vai_da_captura_ao_quadro_e_para_quando_mandam() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        let captura = CapturaDeMentira::default();
        let (bomba, mut eventos) = ligar(
            biblioteca,
            captura.clone(),
            arranjo(
                TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS),
                SignalBand::Nominal,
                Resolucao::P720,
            ),
            || {},
        )
        .expect("criar a thread da tela");

        // 1 — o primeiro evento é o fluxo, sempre. Quem escreve no fio precisa
        // do cabeçalho antes do primeiro byte de quadro (§3.6).
        assert_eq!(
            esperar(&mut eventos),
            Some(EventoDaBomba::Fluxo {
                geracao: 1,
                resolucao: Resolucao::P720,
                teto_bps: 1_200_000,
            }),
            "a bomba entregou quadro antes de dizer em que fluxo ele vai"
        );
        assert_eq!(
            captura.inicios(),
            vec![(1280, 720)],
            "a captura tinha de começar no degrau que o teto comprou"
        );

        // 2 — e depois saem quadros de verdade, no fluxo que acabou de ser
        // anunciado. O primeiro é chave, com SPS e PPS na frente.
        let mut primeiro = None;
        for _ in 0..4 {
            if let Some(EventoDaBomba::Quadro(quadro)) = esperar(&mut eventos) {
                primeiro = Some(quadro);
                break;
            }
        }
        let primeiro = primeiro.expect("um quadro codificado em quatro tiques");
        assert_eq!(primeiro.geracao, 1);
        assert!(primeiro.chave, "o primeiro quadro de um fluxo é chave");
        assert!(
            primeiro.bytes.starts_with(&[0, 0, 0, 1]),
            "Annex-B começa com um código de início"
        );

        // 3 — e a thread morre quando mandam, dizendo que morreu de propósito.
        bomba.parar();
        let fim = loop {
            match esperar(&mut eventos) {
                Some(EventoDaBomba::Quadro(_)) => {}
                outro => break outro,
            }
        };
        assert_eq!(
            fim,
            Some(EventoDaBomba::Fim(None)),
            "a thread tinha de dizer que o fim foi pedido, e não que morreu"
        );
    }

    /// §3.6 e §5.1: **trocar de degrau é abrir um fluxo novo**, e são três
    /// coisas que recomeçam juntas — captura, codificador e fluxo.
    ///
    /// A escada é a do §5.1, e a mesma de `crate::video`: entra gente, a perna
    /// de quem hospeda é dividida por mais um, o teto cai, e o degrau que o
    /// orçamento compra cai com ele.
    ///
    /// Se a captura **não** recomeçar, o codificador recusa o próximo quadro com
    /// `QuadroDeTamanhoErrado` e a bomba morre — que é exatamente o defeito que
    /// esta prova existe para prender.
    #[test]
    fn trocar_de_degrau_recomeca_captura_codificador_e_fluxo() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        let fibra = TetoDeVideo::com_caminho(6_000_000)
            .com_caminho_de_quem_hospeda(6_000_000)
            .com_espectadores(1);
        let captura = CapturaDeMentira::default();
        let (bomba, mut eventos) = ligar(
            biblioteca,
            captura.clone(),
            arranjo(fibra, SignalBand::Nominal, Resolucao::P1080),
            || {},
        )
        .expect("criar a thread da tela");

        assert_eq!(
            esperar(&mut eventos),
            Some(EventoDaBomba::Fluxo {
                geracao: 1,
                resolucao: Resolucao::P1080,
                teto_bps: 3_600_000,
            })
        );

        // Entram mais duas pessoas: 3,6 Mbps ÷ 3 = 1,2, que é onde o 1080p
        // passa a jogar fora um sexto do que captura.
        assert!(bomba.teto(fibra.com_espectadores(3), SignalBand::Nominal));

        assert_eq!(
            esperar_controle(&mut eventos),
            Some(EventoDaBomba::Fluxo {
                geracao: 2,
                resolucao: Resolucao::P720,
                teto_bps: 1_200_000,
            }),
            "o degrau caiu e ninguém abriu fluxo novo: quem recebe leria 720p \
             sob um cabeçalho que anuncia 1080p"
        );
        assert_eq!(
            captura.inicios(),
            vec![(1920, 1080), (1280, 720)],
            "a captura tinha de recomeçar no degrau novo"
        );

        // E os quadros que saem depois são do fluxo novo, não do velho.
        let mut vistos = 0_u32;
        for _ in 0..8 {
            if let Some(EventoDaBomba::Quadro(quadro)) = esperar(&mut eventos) {
                assert_eq!(quadro.geracao, 2, "quadro do fluxo velho no fluxo novo");
                vistos += 1;
                if vistos == 2 {
                    break;
                }
            }
        }
        assert!(vistos > 0, "nenhum quadro saiu depois de trocar de degrau");
        bomba.parar();
    }

    #[test]
    fn a_escolha_da_pessoa_troca_o_degrau_e_a_mesma_escolha_nao_troca_nada() {
        // Até a 0.7.14 este botão respondia erro. A bomba não tinha ordem que
        // trocasse a escolha da pessoa — `Ordem` era `Teto`, `Chave` e `Parar`
        // —, e a ponte preferiu recusar a aceitar um terço do pedido e escrever
        // os três números no painel como se todos tivessem pegado.
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        // Fibra de sobra, e uma pessoa só: assim o degrau é decidido pela
        // escolha e não pelo teto, que é justamente o que este teste mede.
        let fibra = TetoDeVideo::com_caminho(20_000_000)
            .com_caminho_de_quem_hospeda(20_000_000)
            .com_espectadores(1);
        let captura = CapturaDeMentira::default();
        let (bomba, mut eventos) = ligar(
            biblioteca,
            captura.clone(),
            arranjo(fibra, SignalBand::Nominal, Resolucao::P1080),
            || {},
        )
        .expect("criar a thread da tela");

        let Some(EventoDaBomba::Fluxo { resolucao, .. }) = esperar(&mut eventos) else {
            panic!("a bomba não abriu o primeiro fluxo");
        };
        assert_eq!(resolucao, Resolucao::P1080);

        // A pessoa baixa o próprio teto para 720p.
        assert!(bomba.escolha(Resolucao::P720, Cadencia::Q30));
        let Some(EventoDaBomba::Fluxo {
            geracao, resolucao, ..
        }) = esperar_controle(&mut eventos)
        else {
            panic!("a escolha não abriu fluxo novo");
        };
        assert_eq!(resolucao, Resolucao::P720, "a escolha não pegou");
        assert_eq!(
            geracao, 2,
            "o fluxo tinha de recomeçar: o tamanho vai no cabeçalho"
        );
        assert_eq!(
            captura.inicios(),
            vec![(1920, 1080), (1280, 720)],
            "a captura tinha de recomeçar no degrau novo"
        );

        // E a mesma escolha de novo não faz nada. Sem esta guarda, apertar
        // APLICAR duas vezes piscaria a imagem de todo mundo da sala por causa
        // de um clique que não mudou nada.
        assert!(bomba.escolha(Resolucao::P720, Cadencia::Q30));
        let mut fluxos = 0_u32;
        for _ in 0..12 {
            match esperar(&mut eventos) {
                Some(EventoDaBomba::Fluxo { .. }) => fluxos += 1,
                Some(_) => {}
                None => break,
            }
        }
        assert_eq!(fluxos, 0, "a mesma escolha recomeçou o fluxo à toa");
        assert_eq!(
            captura.inicios().len(),
            2,
            "a captura recomeçou por causa de uma escolha que não mudou"
        );

        bomba.parar();
    }

    /// §3.2: **quem para é o vídeo, e parar não é morrer.**
    ///
    /// *«Uma conversa com a tela travando é o produto funcionando; uma conversa
    /// picotando porque alguém abriu a tela é o produto quebrado.»* A outra
    /// metade da frase é esta: quando o sinal volta, a tela volta sozinha, sem
    /// ninguém apertar nada. Uma bomba que morresse no sinal crítico obrigaria a
    /// casca a distinguir «parou porque o sinal caiu» de «parou porque a pessoa
    /// apertou» — a distinção que o §3.6 existe para manter.
    #[test]
    fn o_sinal_critico_para_o_video_e_o_sinal_que_volta_o_rearma() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        let teto = TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS);
        let captura = CapturaDeMentira::default();
        let (bomba, mut eventos) = ligar(
            biblioteca,
            captura.clone(),
            arranjo(teto, SignalBand::Nominal, Resolucao::P720),
            || {},
        )
        .expect("criar a thread da tela");

        assert!(matches!(
            esperar(&mut eventos),
            Some(EventoDaBomba::Fluxo { geracao: 1, .. })
        ));

        assert!(bomba.teto(teto, SignalBand::Critical));
        assert_eq!(
            esperar_controle(&mut eventos),
            Some(EventoDaBomba::Parou(MotivoDeParada::SinalCritico)),
            "o sinal crítico tinha de parar o vídeo com motivo, e não em silêncio"
        );

        // E parou de verdade. Quatro intervalos de quadro depois, nada saiu:
        // uma bomba que continuasse codificando com o fluxo fechado gastaria a
        // CPU que o §2 mandou não disputar com a voz, e gastaria justamente
        // quando o sinal está crítico.
        thread::sleep(Duration::from_millis(120));
        assert!(
            matches!(eventos.try_recv(), Err(canal::error::TryRecvError::Empty)),
            "a bomba continuou produzindo depois de o vídeo ter parado"
        );

        // O sinal volta, e a tela volta com ele — fluxo novo, captura nova.
        assert!(bomba.teto(teto, SignalBand::Nominal));
        assert_eq!(
            esperar_controle(&mut eventos),
            Some(EventoDaBomba::Fluxo {
                geracao: 2,
                resolucao: Resolucao::P720,
                teto_bps: 1_200_000,
            }),
            "o sinal voltou e a tela não voltou com ele"
        );
        assert_eq!(captura.inicios(), vec![(1280, 720), (1280, 720)]);
        bomba.parar();
    }

    /// A bomba inteira, ligada num par QUIC de verdade: da captura ao byte que
    /// o outro lado lê.
    ///
    /// É o fecho do ciclo, e o que ele prende que nenhum dos testes acima prende
    /// é a tradução do [`EventoDaBomba::Fluxo`] em cabeçalho de abertura: a
    /// resolução que quem recebe lê é a que o **codificador** está usando, e não
    /// a que a pessoa pediu (§5, *a tela não promete a escolha*).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_bomba_atravessa_a_conexao_e_o_outro_lado_le_o_que_saiu() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        let (saida, entrada) = crate::tela::tests::par().await;

        let captura = CapturaDeMentira::default();
        // Quem escolheu 1080p num cano de 2 Mbps recebe 720p, e é o caso para o
        // qual a regra «a tela não promete a escolha» existe.
        let (bomba, mut eventos) = ligar(
            biblioteca,
            captura,
            arranjo(
                TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS),
                SignalBand::Nominal,
                Resolucao::P1080,
            ),
            || {},
        )
        .expect("criar a thread da tela");

        let escoando = tokio::spawn(async move {
            escoar(
                &saida,
                ScreenId(0x00C0_FFEE),
                ScreenSource::Monitor,
                &mut eventos,
            )
            .await
        });

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar o fluxo");
        assert_eq!(recepcao.cabecalho().width, 1280);
        assert_eq!(recepcao.cabecalho().height, 720);
        assert_eq!(recepcao.cabecalho().screen, ScreenId(0x00C0_FFEE));

        let primeiro = recepcao
            .proximo_quadro()
            .await
            .expect("ler o primeiro quadro")
            .expect("o fluxo não podia ter acabado");
        assert!(primeiro.chave(), "o primeiro quadro de um fluxo é chave");
        assert!(!primeiro.bytes.is_empty());

        // Bloqueia a thread do teste por no máximo um intervalo de quadro; a
        // bateria roda em runtime de várias threads justamente por isto.
        bomba.parar();
        let contagem = escoando
            .await
            .expect("a tarefa de escoar")
            .expect("escoar até o fim");
        assert_eq!(contagem.fluxos, 1);
        assert!(contagem.enviados >= 1, "nenhum quadro chegou ao fio");
        assert!(contagem.bytes > 0);
    }
}
