//! **O foco de voz: quem tem o microfone desta máquina** (E3 da 0.17,
//! multi-sessão).
//!
//! # Por que existe
//!
//! Desde a 0.17 o app mantém várias sessões cliente conectadas ao mesmo tempo,
//! em servidores diferentes. Cada [`crate::Connection`] tem a sua
//! [`crate::VozDaSala`], e a pendência 52 abre a voz quando a pessoa senta numa
//! sala confirmada. Sem um árbitro, duas sessões sentadas em salas abririam
//! duas `Voice`: dois microfones abertos, cada palavra indo a dois grupos de
//! gente, e a pessoa sem como saber quem a ouve. É a falha mais cara deste
//! produto (`specs/03-audio.md`, ADR 0031), promovida a recurso.
//!
//! Aqui mora a regra: **no máximo uma conexão tem a voz**, e trocar é uma ação
//! explícita, em ordem total — a voz da antiga está fechada, a abertura dela
//! que ainda estivesse no aparelho terminou e foi largada, e o aparelho dela
//! fechou, **antes** de a nova poder abrir.
//!
//! **A promessa, medida no fio: o microfone desta máquina nunca é transmitido a
//! dois servidores.** É o critério de aceite
//! (`seele-conformance/tests/multi_sessao_foco_de_voz.rs`: os datagramas de voz
//! que cada servidor real recebe, contados durante as trocas). Não é «nunca dois
//! aparelhos abertos»: reabrir o aparelho dentro da mesma sessão, de propósito
//! (a troca de microfone, a volta de uma queda), pode ter duas capturas por um
//! instante, e as duas são da mesma sessão e do mesmo servidor.
//!
//! # O que fica fora do foco
//!
//! Nada toca e nada sai. A sessão sem o foco, sentada numa sala, fica em
//! [`crate::AberturaDoAudio::EmOutraSessao`]: nenhum aparelho aberto. O som da
//! tela mora na mesma `Voice` e segue a mesma regra. Texto, presença, moderação
//! e quadros de tela continuam chegando a todas.
//!
//! # Quem ganha o foco
//!
//! - A conexão que se inscreve sozinha (nenhuma outra inscrita): é o caso de uma
//!   sessão só, que se comporta exatamente como antes do foco existir.
//! - Fora isso, só por [`FocoDeVoz::focar`], que a casca chama num ato explícito
//!   da pessoa. Nunca por um `MovePerson` remoto, por conectar, por reconectar
//!   ou por outra sessão cair.
//!
//! Quem sai deixa o foco vazio: ele não passa sozinho a ninguém.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

/// Quanto a troca espera o driver da antiga confirmar que largou a abertura
/// em curso.
///
/// A voz viva da antiga já caiu antes desta espera, da thread de quem troca. O
/// que a confirmação acrescenta é a abertura **em curso**, que só o laço do
/// driver segura: sem ela, uma voz que acabou de abrir poderia estar a caminho
/// do lugar enquanto a nova abre. Dois segundos é muito mais que uma volta do
/// laço; um driver preso além disso é dito no `seele.log`, e a conferência sob
/// o cadeado da voz (`VozDaSala::instalar`) ainda larga a que chegar sem foco.
pub const PRAZO_DA_CONFIRMACAO: Duration = Duration::from_secs(2);

/// O que a troca precisa de cada conexão inscrita.
///
/// Um traço, e não a conexão: a troca é a mesma com a `Voice` de verdade e com
/// a de mentira dos testes, e é a ordem dela que se prova.
pub(crate) trait Concorrente: Send + Sync {
    /// Larga a voz viva já, desta thread, com a tecla solta.
    fn calar(&self);
    /// Acorda o driver para olhar a voz de novo. Com `resposta`, ele responde
    /// depois de olhar — e olhar sem o foco larga a abertura em curso.
    ///
    /// `false` quando o driver já não existe.
    fn acordar(&self, resposta: Option<std::sync::mpsc::Sender<()>>) -> bool;
    /// Solta a tecla de falar guardada: uma pressão perdida enquanto a sessão
    /// não tinha o foco não pode transmitir no instante em que ela o ganha.
    fn soltar_a_tecla(&self);
}

/// O que uma troca fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Troca {
    /// Quem tinha o foco.
    pub antes: Option<u64>,
    /// Quem tem agora.
    pub agora: Option<u64>,
    /// A antiga confirmou dentro de [`PRAZO_DA_CONFIRMACAO`]. Falso também
    /// quando não havia antiga a confirmar.
    pub confirmada: bool,
}

/// Por que uma troca não aconteceu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum FalhaNoFoco {
    /// O foco foi pedido para uma conexão que não está inscrita: já saiu, ou
    /// nunca entrou com este foco.
    Desconhecida,
}

/// O microfone e a saída de voz desta máquina, e qual conexão os tem.
///
/// Um por máquina, atrás de um `Arc`, entregue a cada conexão por
/// [`crate::Connection::connect_com_foco`].
pub struct FocoDeVoz {
    /// A conexão com o foco; zero é ninguém. Atômico porque o driver de cada
    /// conexão o lê a cada volta do laço, e nunca pode esperar uma troca.
    dono: AtomicU64,
    /// O número da próxima inscrição. Nunca volta, nunca é zero.
    proximo: AtomicU64,
    /// As conexões inscritas.
    inscritas: Mutex<BTreeMap<u64, Arc<dyn Concorrente>>>,
    /// Uma troca por vez: é o que faz delas uma ordem total.
    troca: Mutex<()>,
    /// **A vez de aparelho da máquina.** Toda abertura de voz de toda conexão
    /// com este foco a toma antes de abrir e só a solta depois de entregar ou
    /// largar o que abriu. A troca a toma depois de calar a antiga: uma
    /// abertura da antiga que ainda estava no aparelho termina antes.
    vez: Arc<Mutex<()>>,
    /// Quantas trocas aconteceram. Diagnóstico.
    trocas: AtomicU64,
    /// Quantas vezes a antiga não confirmou no prazo. Diagnóstico: um número
    /// acima de zero é um driver preso, e o `seele.log` diz qual.
    sem_confirmacao: AtomicU64,
    /// Este foco já teve dono alguma vez: daí em diante ninguém o toma sozinho
    /// ([`Self::tomar_se_primeira`]).
    ja_teve_dono: std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for FocoDeVoz {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FocoDeVoz")
            .field("dono", &self.dono())
            .field("trocas", &self.trocas())
            .finish_non_exhaustive()
    }
}

impl FocoDeVoz {
    /// Um foco sem ninguém.
    #[must_use]
    pub fn novo() -> Arc<Self> {
        Arc::new(Self {
            dono: AtomicU64::new(0),
            proximo: AtomicU64::new(1),
            inscritas: Mutex::new(BTreeMap::new()),
            troca: Mutex::new(()),
            vez: Arc::new(Mutex::new(())),
            trocas: AtomicU64::new(0),
            sem_confirmacao: AtomicU64::new(0),
            ja_teve_dono: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Qual conexão tem o foco agora, pelo número de inscrição dela
    /// ([`crate::Connection::id_no_foco`]).
    #[must_use]
    pub fn dono(&self) -> Option<u64> {
        match self.dono.load(Ordering::Acquire) {
            0 => None,
            id => Some(id),
        }
    }

    /// Quantas trocas já aconteceram.
    #[must_use]
    pub fn trocas(&self) -> u64 {
        self.trocas.load(Ordering::Relaxed)
    }

    /// Quantas trocas seguiram sem a confirmação da antiga.
    #[must_use]
    pub fn sem_confirmacao(&self) -> u64 {
        self.sem_confirmacao.load(Ordering::Relaxed)
    }

    /// Quantas conexões estão inscritas.
    #[must_use]
    pub fn inscritas(&self) -> usize {
        self.inscritas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// A vez de aparelho da máquina, para a abertura de voz de uma conexão.
    pub(crate) fn vez(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.vez)
    }

    /// Inscreve uma conexão, e devolve o número dela. **Ninguém ganha o foco
    /// aqui**: a conexão ainda nem entrou, e uma que não chega a entrar não
    /// pode ter sido dona (nenhum dono fantasma). Ver [`Self::tomar_se_primeira`].
    pub(crate) fn inscrever(&self, concorrente: Arc<dyn Concorrente>) -> u64 {
        let id = self.proximo.fetch_add(1, Ordering::Relaxed);
        self.inscritas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, concorrente);
        tracing::debug!(id, "uma conexão entrou no foco de voz");
        id
    }

    /// **A primeira conexão da vida deste foco, já de pé, o toma sozinha** — e
    /// só ela, e só se for a única inscrita. É o caso de uma sessão só, que se
    /// comporta como antes do foco existir.
    ///
    /// Depois de o foco ter tido um dono, ele nunca mais é tomado assim: uma
    /// saída ou uma queda deixa o foco vazio, e uma conexão nova que entra
    /// depois **não o rouba** (a mesma semântica da web, fronteiras.md §14).
    /// Quem traz a voz, então, é um ato da pessoa: entrar numa sala de voz, ou
    /// [`Self::focar`].
    ///
    /// Sob o cadeado das inscritas, como toda mudança de dono fora da troca.
    pub(crate) fn tomar_se_primeira(&self, id: u64) -> bool {
        let inscritas = self
            .inscritas
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let sozinha = inscritas.len() == 1 && inscritas.contains_key(&id);
        let tomou = sozinha
            && !self.ja_teve_dono.load(Ordering::Acquire)
            && self
                .dono
                .compare_exchange(0, id, Ordering::AcqRel, Ordering::Acquire)
                .is_ok();
        if tomou {
            self.ja_teve_dono.store(true, Ordering::Release);
        }
        drop(inscritas);
        tracing::debug!(id, tomou, "uma conexão de pé olhou o foco de voz");
        tomou
    }

    /// Tira uma conexão do foco. Se ela era a dona, o foco fica vazio.
    /// Idempotente.
    ///
    /// Sob o cadeado das inscritas: uma troca que esteja terminando para esta
    /// conexão revalida ali, e vê que ela saiu (o alvo encerrado cancela a
    /// troca).
    pub(crate) fn desinscrever(&self, id: u64) {
        let mut inscritas = self
            .inscritas
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let saiu = inscritas.remove(&id).is_some();
        let era_dona = self
            .dono
            .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        drop(inscritas);
        if saiu {
            tracing::debug!(id, era_dona, "uma conexão saiu do foco de voz");
        }
        if era_dona {
            tracing::info!(
                id,
                "a conexão com o foco de voz saiu: o foco ficou vazio, e não passa sozinho a \
                 nenhuma outra"
            );
        }
    }

    /// **A troca.** Passa o foco para `nova` (ou para ninguém), em ordem total:
    ///
    /// 1. o dono passa a ser ninguém;
    /// 2. a voz da antiga cai já, desta thread, e o driver dela confirma que
    ///    largou também a abertura em curso (até [`PRAZO_DA_CONFIRMACAO`]);
    /// 3. a vez de aparelho da máquina é tomada e solta: uma abertura antiga que
    ///    ainda estava no aparelho terminou, e foi largada sem o foco;
    /// 4. a tecla guardada da nova volta a solta;
    /// 5. o dono passa a ser a nova, e o driver dela é acordado.
    ///
    /// **Bloqueia** até o passo 5. Não é para a thread de um driver.
    ///
    /// # Errors
    ///
    /// [`FalhaNoFoco::Desconhecida`] quando `nova` não está inscrita. O foco não
    /// muda nesse caso: uma troca recusada não cala ninguém.
    pub fn focar(&self, nova: Option<u64>) -> Result<Troca, FalhaNoFoco> {
        let _uma_por_vez = self.troca.lock().unwrap_or_else(PoisonError::into_inner);
        let antes = self.dono();
        let (da_antiga, da_nova) = {
            let inscritas = self
                .inscritas
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let da_nova = match nova {
                Some(id) => Some(
                    inscritas
                        .get(&id)
                        .cloned()
                        .ok_or(FalhaNoFoco::Desconhecida)?,
                ),
                None => None,
            };
            (antes.and_then(|id| inscritas.get(&id).cloned()), da_nova)
        };
        if antes == nova {
            return Ok(Troca {
                antes,
                agora: nova,
                confirmada: false,
            });
        }

        // 1. Ninguém tem o foco a partir daqui: o driver da antiga, que o lê a
        //    cada volta, não abre mais nada, e `instalar` larga o que chegar.
        {
            let _inscritas = self
                .inscritas
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            self.dono.store(0, Ordering::Release);
        }

        // 2. A voz viva da antiga cai, e o driver dela confirma.
        let mut confirmada = false;
        if let Some(antiga) = da_antiga {
            antiga.calar();
            let (resposta, confirmacao) = std::sync::mpsc::channel();
            if antiga.acordar(Some(resposta)) {
                confirmada = confirmacao.recv_timeout(PRAZO_DA_CONFIRMACAO).is_ok();
                if !confirmada {
                    self.sem_confirmacao.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        antes = ?antes,
                        "o driver da sessão que tinha a voz não confirmou em {:?} que largou a \
                         abertura em curso: a voz viva dela já caiu, e uma voz que chegar sem o \
                         foco é largada ao chegar",
                        PRAZO_DA_CONFIRMACAO
                    );
                }
            }
        }

        // 3. Uma abertura antiga que ainda estava no aparelho termina antes.
        drop(self.vez.lock().unwrap_or_else(PoisonError::into_inner));
        //    E o aparelho da antiga **fecha de fato**: largar a voz só marca o
        //    `stop`, e a thread de áudio dela fecha o aparelho na volta seguinte
        //    do laço. Medido na conformidade: sem esta espera, dois aparelhos
        //    ficavam abertos por alguns quadros durante a troca.
        if !seele_core::voice::esperar_nenhuma_voz_viva(PRAZO_DA_CONFIRMACAO) {
            self.sem_confirmacao.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                vivas = seele_core::voice::vozes_vivas(),
                "o aparelho da sessão que tinha a voz não fechou em {:?}: a nova abre assim \
                 mesmo, e o `seele.log` fica com esta linha",
                PRAZO_DA_CONFIRMACAO
            );
        }

        // 4 e 5, **revalidados depois da espera**: a nova pode ter saído (ou
        // caído de vez) enquanto a antiga soltava. Conferido e gravado sob o
        // cadeado das inscritas, o mesmo da saída: ou ela ainda está inscrita e
        // passa a dona, ou saiu e a troca é cancelada, com o foco vazio.
        self.trocas.fetch_add(1, Ordering::Relaxed);
        if let (Some(id), Some(nova)) = (nova, da_nova) {
            nova.soltar_a_tecla();
            let ainda = {
                let inscritas = self
                    .inscritas
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let ainda = inscritas.contains_key(&id);
                if ainda {
                    self.dono.store(id, Ordering::Release);
                    self.ja_teve_dono.store(true, Ordering::Release);
                }
                ainda
            };
            if !ainda {
                tracing::info!(
                    ?antes,
                    id,
                    "a sessão que ia ganhar a voz saiu durante a troca: a troca foi cancelada, e \
                     o foco ficou vazio"
                );
                return Err(FalhaNoFoco::Desconhecida);
            }
            // **Tomar que falha deixa o foco vazio**: um driver que já não
            // existe não abriria nada, e um dono que não abre é um dono
            // fantasma.
            if !nova.acordar(None) {
                let _ = self
                    .dono
                    .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire);
                tracing::warn!(
                    id,
                    "a sessão que ganhou a voz não tem mais driver: o foco ficou vazio"
                );
                return Err(FalhaNoFoco::Desconhecida);
            }
        }
        tracing::info!(?antes, agora = ?nova, confirmada, "o foco de voz mudou de sessão");
        Ok(Troca {
            antes,
            agora: nova,
            confirmada,
        })
    }
}

/// O lugar de uma conexão num foco: o foco e o número dela.
///
/// Mora na `VozDaSala` da conexão. Largado, tira a conexão do foco — é o que
/// garante que uma conexão que acabou, por qualquer caminho, não fica dona.
pub(crate) struct NoFoco {
    pub(crate) foco: Arc<FocoDeVoz>,
    pub(crate) id: u64,
}

impl NoFoco {
    /// Esta conexão tem o foco agora?
    pub(crate) fn tem(&self) -> bool {
        self.foco.dono() == Some(self.id)
    }
}

impl Drop for NoFoco {
    fn drop(&mut self) {
        self.foco.desinscrever(self.id);
    }
}

/// O concorrente de uma conexão de verdade: a voz dela, e a fila do driver.
pub(crate) struct NaConexao {
    pub(crate) shared: Weak<crate::Shared>,
    pub(crate) comandos: tokio::sync::mpsc::UnboundedSender<crate::Command>,
}

impl Concorrente for NaConexao {
    fn calar(&self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.voice.calar_sem_foco();
        }
    }

    fn acordar(&self, resposta: Option<std::sync::mpsc::Sender<()>>) -> bool {
        self.comandos
            .send(crate::Command::OlharAVoz(resposta))
            .is_ok()
    }

    fn soltar_a_tecla(&self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.voice.soltar_a_tecla_guardada();
        }
    }
}

#[cfg(test)]
mod a_troca_e_ordem_total {
    //! A troca, com a `VozDaSala` e a `PortaDaVoz` de verdade e uma voz de
    //! mentira que conta quantas estão vivas ao mesmo tempo, em todas as
    //! sessões. Cada sessão tem um «driver» que é o laço de `drive` reduzido
    //! ao que a voz faz: o comando `OlharAVoz`, a chegada da abertura e o olhar
    //! do fim da volta.

    use super::{Concorrente, FocoDeVoz, NoFoco};
    use crate::{AberturaDoAudio, ControlesGuardados, Futuro, PortaDaVoz, VozDaSala, VozViva};
    use seele_core::{DeviceChoice, Ssrc, VoiceRoomId};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Quantas vozes de mentira estão vivas agora, somando todas as sessões, e o
    /// máximo que já estiveram ao mesmo tempo.
    #[derive(Default)]
    struct Contagem {
        vivas: AtomicUsize,
        maximo: AtomicUsize,
    }

    struct Voz {
        contagem: Arc<Contagem>,
        tecla: Mutex<bool>,
    }

    impl Voz {
        fn nova(contagem: &Arc<Contagem>) -> Self {
            let vivas = contagem.vivas.fetch_add(1, Ordering::SeqCst) + 1;
            contagem.maximo.fetch_max(vivas, Ordering::SeqCst);
            Self {
                contagem: Arc::clone(contagem),
                tecla: Mutex::new(false),
            }
        }
    }

    impl Drop for Voz {
        fn drop(&mut self) {
            self.contagem.vivas.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl VozViva for Voz {
        fn vestir(&self, escolhida: &ControlesGuardados) {
            *self.tecla.lock().expect("a tecla") = escolhida.tecla_segura;
        }
        fn segurar(&self, segura: bool) {
            *self.tecla.lock().expect("a tecla") = segura;
        }
        fn ganhar(&self, _: Ssrc, _: f32) {}
    }

    enum Pedido {
        Olhar(Option<std::sync::mpsc::Sender<()>>),
        Sentar(Option<VoiceRoomId>),
        Parar,
    }

    /// Uma sessão: a voz dela, a fila do driver, e onde ela está sentada.
    struct Sessao {
        voz: Arc<VozDaSala<Voz>>,
        fila: tokio::sync::mpsc::UnboundedSender<Pedido>,
        driver: Option<std::thread::JoinHandle<()>>,
    }

    struct NaSessao {
        voz: Arc<VozDaSala<Voz>>,
        fila: tokio::sync::mpsc::UnboundedSender<Pedido>,
    }

    impl Concorrente for NaSessao {
        fn calar(&self) {
            self.voz.calar_sem_foco();
        }
        fn acordar(&self, resposta: Option<std::sync::mpsc::Sender<()>>) -> bool {
            self.fila.send(Pedido::Olhar(resposta)).is_ok()
        }
        fn soltar_a_tecla(&self) {
            self.voz.soltar_a_tecla_guardada();
        }
    }

    /// Sobe uma sessão no foco. A abertura da voz demora `demora`, na thread
    /// de `esperar_o_audio` de verdade, com a vez da máquina; o driver leva
    /// `ocupado` para dar a volta do laço depois de um pedido.
    fn sessao_ocupada(
        foco: &Arc<FocoDeVoz>,
        contagem: &Arc<Contagem>,
        demora: Duration,
        ocupado: Duration,
    ) -> Sessao {
        let (fila, mut pedidos) = tokio::sync::mpsc::unbounded_channel();
        let voz = Arc::new(VozDaSala::<Voz>::nova(true, DeviceChoice::default()));
        let id = foco.inscrever(Arc::new(NaSessao {
            voz: Arc::clone(&voz),
            fila: fila.clone(),
        }));
        voz.entrar_no_foco(NoFoco {
            foco: Arc::clone(foco),
            id,
        });
        // A conexão ficou de pé (o `ready` do `abrir`).
        voz.tomar_o_foco_se_primeira();
        let no_driver = Arc::clone(&voz);
        let contagem = Arc::clone(contagem);
        let driver = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("o runtime do driver");
            runtime.block_on(async move {
                let voz = no_driver;
                let mut porta = PortaDaVoz::<Voz>::nova();
                let mut sala = None;
                let abrir = |voz: &VozDaSala<Voz>, contagem: &Arc<Contagem>| {
                    let vez = voz.vez();
                    let contagem = Arc::clone(contagem);
                    let tem_foco = voz.conferidor_do_foco();
                    move |_: &DeviceChoice| -> Futuro<Voz> {
                        Box::pin(async move {
                            crate::esperar_o_audio(
                                &vez,
                                move || {
                                    // O aparelho fica vivo **durante** a
                                    // abertura, como o CoreAudio: a voz conta
                                    // desde já, e a demora vem depois.
                                    let aberta = Voz::nova(&contagem);
                                    std::thread::sleep(demora);
                                    if tem_foco() {
                                        Ok(aberta)
                                    } else {
                                        Err("sem o foco de voz".to_owned())
                                    }
                                },
                                Duration::from_secs(5),
                            )
                            .await
                        })
                    }
                };
                loop {
                    tokio::select! {
                        pedido = pedidos.recv() => match pedido {
                            Some(Pedido::Olhar(resposta)) => {
                                // Um driver de verdade está no meio de um
                                // `await` de rede quando a troca chega: a volta
                                // dele demora. Sem isto, uma troca ingênua
                                // (acordar as duas e torcer) passaria aqui.
                                tokio::time::sleep(ocupado).await;
                                voz.esperar_o_olhar(resposta);
                            }
                            Some(Pedido::Sentar(nova)) => sala = nova,
                            Some(Pedido::Parar) | None => break,
                        },
                        aberta = porta.chegada() => {
                            porta.chegou(&voz, aberta, abrir(&voz, &contagem));
                        }
                    }
                    porta.olhar(&voz, sala, abrir(&voz, &contagem));
                }
                drop(porta);
                voz.largar();
                // O fim de `drive`: a sessão que acabou sai do foco.
                voz.sair_do_foco();
            });
        });
        Sessao {
            voz,
            fila,
            driver: Some(driver),
        }
    }

    /// Uma sessão com o driver livre.
    fn sessao(foco: &Arc<FocoDeVoz>, contagem: &Arc<Contagem>, demora: Duration) -> Sessao {
        sessao_ocupada(foco, contagem, demora, Duration::ZERO)
    }

    impl Drop for Sessao {
        fn drop(&mut self) {
            let _ = self.fila.send(Pedido::Parar);
            if let Some(driver) = self.driver.take() {
                let _ = driver.join();
            }
        }
    }

    fn esperar(prazo: Duration, mut pronto: impl FnMut() -> bool) -> bool {
        let fim = std::time::Instant::now() + prazo;
        while std::time::Instant::now() < fim {
            if pronto() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        pronto()
    }

    const SALA: VoiceRoomId = VoiceRoomId(1);

    /// A única inscrita ganha o foco sozinha; a segunda entra sem.
    #[test]
    fn so_a_unica_inscrita_ganha_o_foco_sozinha() {
        let foco = FocoDeVoz::novo();
        let contagem = Arc::new(Contagem::default());
        let a = sessao(&foco, &contagem, Duration::ZERO);
        let id_a = a.voz.id_no_foco().expect("a entrou no foco");
        assert_eq!(
            foco.dono(),
            Some(id_a),
            "a única conexão não ficou com a voz"
        );
        let b = sessao(&foco, &contagem, Duration::ZERO);
        assert_eq!(
            foco.dono(),
            Some(id_a),
            "conectar a segunda sessão tirou a voz da primeira: conectar não é falar"
        );
        assert!(!b.voz.tem_foco());
        drop(b);
        drop(a);
        assert_eq!(
            foco.inscritas(),
            0,
            "uma sessão que acabou continuou inscrita"
        );
        assert_eq!(foco.dono(), None, "o foco ficou com uma sessão que acabou");
    }

    /// **Sentada numa sala sem o foco, nada abre**, e o estado diz por quê.
    #[test]
    fn sem_o_foco_a_sala_nao_abre_aparelho_e_diz_em_outra_sessao() {
        let foco = FocoDeVoz::novo();
        let contagem = Arc::new(Contagem::default());
        let a = sessao(&foco, &contagem, Duration::ZERO);
        let b = sessao(&foco, &contagem, Duration::ZERO);
        b.fila
            .send(Pedido::Sentar(Some(SALA)))
            .expect("o driver de b");
        b.fila.send(Pedido::Olhar(None)).expect("o driver de b");
        assert!(esperar(Duration::from_secs(2), || b.voz.abertura()
            == AberturaDoAudio::EmOutraSessao));
        assert_eq!(contagem.vivas.load(Ordering::SeqCst), 0);
        drop((a, b));
    }

    /// **A ordem total, medida.** Três sessões sentadas, a voz trocando de uma
    /// para outra vinte vezes, com a abertura demorando no aparelho e a troca
    /// pedida no meio dela: em nenhum instante duas vozes vivas.
    ///
    /// Os drivers têm pressas diferentes — um preso 40 ms num `await`, outro
    /// livre —, que é o caso em que uma troca que só acordasse as duas e
    /// confiasse na ordem dos laços deixaria a nova abrir com a antiga viva.
    /// Medido: com o `focar` reduzido a «dono = nova; acordar as duas», este
    /// teste reprova.
    #[test]
    fn em_nenhum_instante_duas_vozes_vivas_durante_as_trocas() {
        let foco = FocoDeVoz::novo();
        let contagem = Arc::new(Contagem::default());
        let sessoes: Vec<Sessao> = [40_u64, 0, 15]
            .into_iter()
            .map(|ocupado| {
                sessao_ocupada(
                    &foco,
                    &contagem,
                    Duration::from_millis(30),
                    Duration::from_millis(ocupado),
                )
            })
            .collect();
        for sessao in &sessoes {
            sessao
                .fila
                .send(Pedido::Sentar(Some(SALA)))
                .expect("o driver");
            sessao.fila.send(Pedido::Olhar(None)).expect("o driver");
        }
        // A primeira tem o foco e abre.
        assert!(esperar(Duration::from_secs(2), || contagem
            .vivas
            .load(Ordering::SeqCst)
            == 1));

        // Uma vigia lê a contagem sem parar, por fora da troca.
        let parar = Arc::new(AtomicBool::new(false));
        let vigia = {
            let parar = Arc::clone(&parar);
            let contagem = Arc::clone(&contagem);
            std::thread::spawn(move || {
                let mut maximo = 0;
                while !parar.load(Ordering::SeqCst) {
                    maximo = maximo.max(contagem.vivas.load(Ordering::SeqCst));
                }
                maximo
            })
        };
        for volta in 0..20_usize {
            let alvo = &sessoes[(volta + 1) % 3];
            let id = alvo.voz.id_no_foco().expect("inscrita");
            let troca = foco.focar(Some(id)).expect("a troca");
            assert_eq!(troca.agora, Some(id));
            // Metade das voltas troca de novo no meio da abertura da nova.
            if volta % 2 == 0 {
                std::thread::sleep(Duration::from_millis(10));
            } else {
                assert!(esperar(Duration::from_secs(2), || alvo.voz.abertura()
                    == AberturaDoAudio::Aberto));
            }
        }
        parar.store(true, Ordering::SeqCst);
        let visto = vigia.join().expect("a vigia");
        assert!(
            visto <= 1 && contagem.maximo.load(Ordering::SeqCst) <= 1,
            "duas sessões tiveram voz viva ao mesmo tempo (vigia {visto}, máximo {}): o \
             microfone foi a dois servidores",
            contagem.maximo.load(Ordering::SeqCst)
        );
        assert_eq!(foco.sem_confirmacao(), 0, "um driver não confirmou a troca");
        drop(sessoes);
        assert_eq!(contagem.vivas.load(Ordering::SeqCst), 0, "sobrou voz viva");
    }

    /// A tecla segurada sem o foco não transmite ao ganhá-lo.
    #[test]
    fn a_tecla_segurada_fora_do_foco_chega_solta() {
        let foco = FocoDeVoz::novo();
        let contagem = Arc::new(Contagem::default());
        let a = sessao(&foco, &contagem, Duration::ZERO);
        let b = sessao(&foco, &contagem, Duration::ZERO);
        b.fila
            .send(Pedido::Sentar(Some(SALA)))
            .expect("o driver de b");
        b.voz.segurar_a_tecla(true);
        foco.focar(b.voz.id_no_foco()).expect("a troca");
        assert!(esperar(Duration::from_secs(2), || b.voz.abertura()
            == AberturaDoAudio::Aberto));
        let tecla = b
            .voz
            .lock()
            .expect("a voz")
            .as_ref()
            .map(|viva| *viva.tecla.lock().expect("a tecla"));
        assert_eq!(
            tecla,
            Some(false),
            "a pressão perdida fora do foco abriu o microfone transmitindo ao ganhar a voz"
        );
        drop((a, b));
    }

    /// Um concorrente controlável: o teste decide quando ele confirma a troca
    /// e se o driver dele ainda existe.
    struct Controlavel {
        /// A resposta da troca, guardada até o teste a soltar.
        resposta: Mutex<Option<std::sync::mpsc::Sender<()>>>,
        /// Avisa o teste de que a troca chegou a este concorrente.
        chegou: Mutex<Option<std::sync::mpsc::Sender<()>>>,
        /// O driver ainda existe?
        vivo: AtomicBool,
        calado: AtomicBool,
    }

    impl Controlavel {
        fn novo(vivo: bool) -> Arc<Self> {
            Arc::new(Self {
                resposta: Mutex::new(None),
                chegou: Mutex::new(None),
                vivo: AtomicBool::new(vivo),
                calado: AtomicBool::new(false),
            })
        }
        fn confirmar(&self) {
            if let Some(resposta) = self.resposta.lock().expect("a resposta").take() {
                let _ = resposta.send(());
            }
        }
    }

    impl Concorrente for Controlavel {
        fn calar(&self) {
            self.calado.store(true, Ordering::SeqCst);
        }
        fn acordar(&self, resposta: Option<std::sync::mpsc::Sender<()>>) -> bool {
            if !self.vivo.load(Ordering::SeqCst) {
                return false;
            }
            if let Some(resposta) = resposta {
                *self.resposta.lock().expect("a resposta") = Some(resposta);
                if let Some(chegou) = self.chegou.lock().expect("o aviso").take() {
                    let _ = chegou.send(());
                }
            }
            true
        }
        fn soltar_a_tecla(&self) {}
    }

    /// **O alvo encerrado durante a troca cancela a troca**, com o foco vazio:
    /// a antiga segura a confirmação; enquanto isso a nova sai; quando a
    /// antiga confirma, a troca revalida e não dá a voz a quem saiu.
    #[test]
    fn o_alvo_que_sai_durante_a_troca_cancela_a_troca_com_o_foco_vazio() {
        let foco = FocoDeVoz::novo();
        let antiga = Controlavel::novo(true);
        let nova = Controlavel::novo(true);
        let (chegou, quando_chegar) = std::sync::mpsc::channel();
        *antiga.chegou.lock().expect("o aviso") = Some(chegou);
        let id_antiga = foco.inscrever(Arc::clone(&antiga) as Arc<dyn Concorrente>);
        assert!(foco.tomar_se_primeira(id_antiga));
        let id_nova = foco.inscrever(Arc::clone(&nova) as Arc<dyn Concorrente>);

        let na_troca = Arc::clone(&foco);
        let troca = std::thread::spawn(move || na_troca.focar(Some(id_nova)));
        quando_chegar
            .recv_timeout(Duration::from_secs(2))
            .expect("a troca não chegou à antiga");
        assert!(
            antiga.calado.load(Ordering::SeqCst),
            "a antiga não foi calada antes"
        );
        assert_eq!(foco.dono(), None, "durante a troca, alguém tinha o foco");
        // A nova sai no meio da troca, e só então a antiga confirma.
        foco.desinscrever(id_nova);
        antiga.confirmar();
        assert_eq!(
            troca.join().expect("a troca"),
            Err(super::FalhaNoFoco::Desconhecida),
            "a troca deu a voz a uma sessão que saiu no meio dela"
        );
        assert_eq!(
            foco.dono(),
            None,
            "o foco ficou com quem saiu durante a troca"
        );
    }

    /// **Tomar que falha deixa o foco vazio**: o driver da nova já não existe.
    #[test]
    fn tomar_que_falha_deixa_o_foco_vazio() {
        let foco = FocoDeVoz::novo();
        let antiga = Controlavel::novo(true);
        let sem_driver = Controlavel::novo(false);
        let id_antiga = foco.inscrever(Arc::clone(&antiga) as Arc<dyn Concorrente>);
        assert!(foco.tomar_se_primeira(id_antiga));
        let id_nova = foco.inscrever(sem_driver as Arc<dyn Concorrente>);
        let na_troca = Arc::clone(&foco);
        let troca = std::thread::spawn(move || na_troca.focar(Some(id_nova)));
        // A antiga confirma assim que a troca a acorda.
        let fim = std::time::Instant::now() + Duration::from_secs(2);
        while antiga.resposta.lock().expect("a resposta").is_none()
            && std::time::Instant::now() < fim
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        antiga.confirmar();
        assert_eq!(
            troca.join().expect("a troca"),
            Err(super::FalhaNoFoco::Desconhecida)
        );
        assert_eq!(foco.dono(), None, "um dono sem driver ficou com o foco");
    }

    /// **Depois de uma saída, o foco fica vazio, e uma conexão nova não o
    /// rouba.** E uma criação que falha (inscrita e largada antes de ficar de
    /// pé) não deixa registro fantasma nem conta como «já teve dono».
    #[test]
    fn depois_de_uma_saida_ninguem_toma_o_foco_sozinho_e_a_falha_nao_deixa_fantasma() {
        let foco = FocoDeVoz::novo();
        // A criação que falha: inscrita, nunca de pé, e largada.
        let falhou = foco.inscrever(Controlavel::novo(true) as Arc<dyn Concorrente>);
        foco.desinscrever(falhou);
        assert_eq!(
            foco.inscritas(),
            0,
            "a criação que falhou deixou um registro fantasma"
        );
        assert_eq!(foco.dono(), None);
        // A primeira de pé, sozinha, toma.
        let primeira = foco.inscrever(Controlavel::novo(true) as Arc<dyn Concorrente>);
        assert!(
            foco.tomar_se_primeira(primeira),
            "uma criação que falhou antes impediu a primeira de verdade de ter a voz"
        );
        // Ela sai: o foco fica vazio.
        foco.desinscrever(primeira);
        assert_eq!(foco.dono(), None);
        // Uma nova entra, sozinha, e não rouba.
        let nova = foco.inscrever(Controlavel::novo(true) as Arc<dyn Concorrente>);
        assert!(
            !foco.tomar_se_primeira(nova),
            "depois de uma saída, uma conexão nova tomou o foco sozinha"
        );
        assert_eq!(foco.dono(), None);
        // Só um ato explícito traz a voz.
        assert_eq!(
            foco.focar(Some(nova)).map(|troca| troca.agora),
            Ok(Some(nova))
        );
    }

    /// Uma troca para quem não está inscrito é recusada e não cala ninguém.
    #[test]
    fn a_troca_para_uma_desconhecida_e_recusada_e_nao_cala_ninguem() {
        let foco = FocoDeVoz::novo();
        let contagem = Arc::new(Contagem::default());
        let a = sessao(&foco, &contagem, Duration::ZERO);
        let dono = foco.dono();
        assert_eq!(
            foco.focar(Some(9_999)),
            Err(super::FalhaNoFoco::Desconhecida)
        );
        assert_eq!(foco.dono(), dono);
        drop(a);
    }
}
