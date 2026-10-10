//! O toque da sala pessoal chega à casca por evento (tarefa 24 da F7).
//!
//! Uma tarefa assina a campainha da sala (`Recepcao::assinar`) e, a cada
//! `Tocou`:
//!
//! - emite [`CANAL_DO_TOQUE`] com quem toca: o apelido (o que a pessoa diz), o
//!   código (a prova) e o nome que a agenda deu, quando deu;
//! - começa o [`Alarme`]: o som (`seele_ffi::Toque`, só a saída) e o pedido de
//!   atenção do sistema. **Nunca** `set_focus` nem `unminimize`: o toque
//!   chama, e não toma a tela de ninguém.
//!
//! O alarme para quando o último toque para: atendido, recusado, vencido, ou
//! com quem ligava desistindo. **E para quando a sala fecha**: sem isso, desligar
//! a sala no meio de um toque deixaria o som tocando para sempre.
//!
//! Cada toque que para é anotado na agenda: atendido como `Recebi`, e o resto
//! como `Perdida`, a «chamada perdida» da spec.

use std::collections::HashMap;
use std::time::Duration;

use seele_server::campainha::{Desfecho, IdDoToque};
use seele_server::recepcao::Aviso;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// O canal por onde o toque chega à casca.
pub(crate) const CANAL_DO_TOQUE: &str = "seele://toque";

/// De quanto em quanto o pedido de atenção é renovado no macOS.
///
/// Lá o `Informational` pula o ícone do Dock uma vez só; repetido, ele vira um
/// toque que se vê sem roubar a tela. No Windows o `Critical` pisca até alguém
/// olhar, e não precisa de lembrete.
const LEMBRETE: Duration = Duration::from_secs(4);

/// Quem está tocando agora, pela impressão da chave de quem liga.
#[derive(Default)]
pub(crate) struct Toques {
    por_impressao: HashMap<String, Tocando>,
}

impl Toques {
    /// O que a faixa mostra, para a tela que abre com o toque já tocando.
    pub(crate) fn na_faixa(&self) -> Vec<NaFaixa> {
        self.por_impressao
            .iter()
            .map(|(quem, tocando)| tocando.na_faixa(quem))
            .collect()
    }

    /// O toque desta pessoa, para atender ou recusar.
    pub(crate) fn de(&self, quem: &str) -> Option<IdDoToque> {
        self.por_impressao.get(quem).map(|tocando| tocando.toque)
    }
}

/// Um toque tocando.
struct Tocando {
    toque: IdDoToque,
    codigo: seele_ffi::Codigo,
    apelido: String,
    nome_na_agenda: Option<String>,
}

impl Tocando {
    fn na_faixa(&self, quem: &str) -> NaFaixa {
        NaFaixa {
            quem: quem.to_owned(),
            codigo: self.codigo.to_string(),
            apelido: self.apelido.clone(),
            nome_na_agenda: self.nome_na_agenda.clone(),
        }
    }
}

/// Quem toca, do jeito que a faixa o mostra.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct NaFaixa {
    /// A impressão da chave de quem liga: a chave do toque para atender ou
    /// recusar.
    pub(crate) quem: String,
    /// O código de quem liga: a prova.
    pub(crate) codigo: String,
    /// O apelido que veio no `Hello`: o que a pessoa diz, e a faixa o mostra
    /// entre aspas.
    pub(crate) apelido: String,
    /// Como **eu** chamo esta pessoa, quando a agenda tem um nome.
    pub(crate) nome_na_agenda: Option<String>,
}

/// O que chega à casca por [`CANAL_DO_TOQUE`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "evento")]
pub(crate) enum AvisoDoToque {
    /// Alguém está tocando. A mesma pessoa de novo (outro candidato da corrida)
    /// chega como outro `Tocou` com o mesmo `quem`: a faixa troca, e não soma.
    Tocou {
        #[serde(flatten)]
        faixa: NaFaixa,
        /// Se o som abriu. Sem som, a faixa é o único aviso, e diz isso.
        com_som: bool,
    },
    /// O toque desta pessoa parou, e por quê.
    Parou {
        /// A impressão de quem ligava.
        quem: String,
        /// `atendida`, `recusada`, `ninguem_atendeu`, `caiu_antes`, `cheia` ou
        /// `sala_fechou`.
        desfecho: &'static str,
    },
}

/// O que o toque faz fora da tela: o som e o pedido de atenção do sistema.
pub(crate) trait Alarme: Send + 'static {
    /// Começa o som e pede atenção. `false` se o som não abriu.
    fn comecar(&mut self) -> bool;
    /// Pede atenção de novo, enquanto toca.
    fn lembrar(&mut self);
    /// Para o som e retira o pedido de atenção.
    fn parar(&mut self);
}

/// O alarme da janela de verdade.
pub(crate) struct AlarmeDaJanela<R: Runtime> {
    app: AppHandle<R>,
    config: String,
    som: Option<seele_ffi::Toque>,
}

impl<R: Runtime> AlarmeDaJanela<R> {
    pub(crate) fn novo(app: AppHandle<R>, config: String) -> Self {
        Self {
            app,
            config,
            som: None,
        }
    }

    /// Pede (ou retira, com `None`) a atenção do sistema para a janela.
    /// **Só isso**: nem foco, nem desminimizar.
    fn atencao(&self, tipo: Option<tauri::UserAttentionType>) {
        let Some(janela) = self.app.get_webview_window("main") else {
            return;
        };
        if let Err(erro) = janela.request_user_attention(tipo) {
            tracing::warn!(%erro, "o pedido de atenção do toque não chegou ao sistema");
        }
    }
}

/// O tipo de atenção que o toque pede: `Critical` no Windows, que pisca até
/// alguém olhar; `Informational` no resto, renovado por [`LEMBRETE`].
const ATENCAO: tauri::UserAttentionType = if cfg!(windows) {
    tauri::UserAttentionType::Critical
} else {
    tauri::UserAttentionType::Informational
};

impl<R: Runtime> Alarme for AlarmeDaJanela<R> {
    fn comecar(&mut self) -> bool {
        // A saída escolhida nas configurações do SEELE; a padrão quando ela
        // sumiu, e o `seele.log` diz (`seele_ffi::Toque::tocar`).
        let saida = seele_ffi::preferences::Preferences::open(
            std::path::Path::new(&self.config).join("preferences"),
        )
        .ok()
        .and_then(|preferencias| preferencias.playback().map(str::to_owned));
        self.som = seele_ffi::Toque::tocar(saida, seele_ffi::VOLUME_DO_TOQUE).ok();
        self.atencao(Some(ATENCAO));
        self.som.is_some()
    }

    fn lembrar(&mut self) {
        if !cfg!(windows) {
            self.atencao(Some(ATENCAO));
        }
    }

    fn parar(&mut self) {
        self.som = None;
        self.atencao(None);
    }
}

/// A agenda desta máquina, ao lado das preferências.
pub(crate) fn caminho_da_agenda(config: &str) -> std::path::PathBuf {
    std::path::Path::new(config).join("agenda")
}

/// O desfecho de um toque numa palavra só, a que a casca lê.
fn palavra(desfecho: Desfecho) -> &'static str {
    match desfecho {
        Desfecho::Atendida => "atendida",
        Desfecho::Recusada => "recusada",
        Desfecho::NaoAtendida => "ninguem_atendeu",
        Desfecho::Cancelada => "caiu_antes",
        Desfecho::Cheia => "cheia",
    }
}

/// Ouve a campainha da sala até ela fechar. Roda numa tarefa própria, desde a
/// subida da sala.
pub(crate) async fn ouvir<R: Runtime>(
    app: AppHandle<R>,
    mut avisos: tokio::sync::broadcast::Receiver<Aviso>,
    config: String,
    mut alarme: impl Alarme,
) {
    let mut com_som = false;
    loop {
        let tocando = toques_agora(&app) > 0;
        let aviso = if tocando {
            tokio::select! {
                aviso = avisos.recv() => aviso,
                () = tokio::time::sleep(LEMBRETE) => {
                    alarme.lembrar();
                    continue;
                }
            }
        } else {
            avisos.recv().await
        };
        match aviso {
            Ok(Aviso::Tocou {
                toque,
                impressao,
                codigo,
                apelido,
            }) => {
                let nome_na_agenda = seele_ffi::agenda::Agenda::abrir(caminho_da_agenda(&config))
                    .ok()
                    .and_then(|agenda| agenda.buscar(&codigo).and_then(|c| c.nome.clone()));
                let tocando = Tocando {
                    toque,
                    codigo,
                    apelido,
                    nome_na_agenda,
                };
                let faixa = tocando.na_faixa(&impressao);
                let primeiro = guardar(&app, impressao, tocando);
                if primeiro {
                    com_som = alarme.comecar();
                }
                anunciar(&app, &AvisoDoToque::Tocou { faixa, com_som });
            }
            Ok(Aviso::ParouDeTocar {
                toque,
                impressao,
                desfecho,
            }) => {
                // Um toque substituído (a mesma pessoa tocando por outro
                // candidato) não é dela mais: quem toca agora é o novo.
                let Some(parou) = tirar_se_for(&app, &impressao, toque) else {
                    continue;
                };
                anotar(&config, &parou, desfecho);
                anunciar(
                    &app,
                    &AvisoDoToque::Parou {
                        quem: impressao,
                        desfecho: palavra(desfecho),
                    },
                );
                if toques_agora(&app) == 0 {
                    alarme.parar();
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(perdidos)) => {
                tracing::warn!(
                    perdidos,
                    "avisos da campainha perdidos: a faixa pode atrasar"
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                // A sala fechou: quem tocava para de tocar, e o som também.
                let restantes: Vec<String> = app
                    .state::<crate::Session>()
                    .toques
                    .lock()
                    .map(|mut toques| toques.por_impressao.drain().map(|(quem, _)| quem).collect())
                    .unwrap_or_default();
                for quem in restantes {
                    anunciar(
                        &app,
                        &AvisoDoToque::Parou {
                            quem,
                            desfecho: "sala_fechou",
                        },
                    );
                }
                alarme.parar();
                return;
            }
        }
    }
}

/// Quantos tocam agora.
fn toques_agora<R: Runtime>(app: &AppHandle<R>) -> usize {
    app.state::<crate::Session>()
        .toques
        .lock()
        .map_or(0, |toques| toques.por_impressao.len())
}

/// Guarda quem toca. `true` se ninguém tocava antes: é aí que o alarme começa.
fn guardar<R: Runtime>(app: &AppHandle<R>, quem: String, tocando: Tocando) -> bool {
    let sessao = app.state::<crate::Session>();
    let Ok(mut toques) = sessao.toques.lock() else {
        return false;
    };
    let primeiro = toques.por_impressao.is_empty();
    toques.por_impressao.insert(quem, tocando);
    primeiro
}

/// Tira quem tocava, se o toque que parou é o que está guardado para ela.
fn tirar_se_for<R: Runtime>(app: &AppHandle<R>, quem: &str, toque: IdDoToque) -> Option<Tocando> {
    let sessao = app.state::<crate::Session>();
    let mut toques = sessao.toques.lock().ok()?;
    if toques.por_impressao.get(quem)?.toque != toque {
        return None;
    }
    toques.por_impressao.remove(quem)
}

/// Anota na agenda o toque que parou: atendido é `Recebi`, e o resto é a
/// chamada perdida.
fn anotar(config: &str, parou: &Tocando, desfecho: Desfecho) {
    let direcao = match desfecho {
        Desfecho::Atendida => seele_ffi::agenda::Direcao::Recebi,
        Desfecho::Recusada | Desfecho::NaoAtendida | Desfecho::Cancelada | Desfecho::Cheia => {
            seele_ffi::agenda::Direcao::Perdida
        }
    };
    let quando = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |desde| {
            i64::try_from(desde.as_secs()).unwrap_or(i64::MAX)
        });
    let anotado =
        seele_ffi::agenda::Agenda::abrir(caminho_da_agenda(config)).and_then(|mut agenda| {
            agenda.anotar_ligacao(parou.codigo, &parou.apelido, direcao, quando)
        });
    if let Err(erro) = anotado {
        tracing::warn!(erro = %format!("{erro:#}"), "o toque não foi anotado na agenda");
    }
}

fn anunciar<R: Runtime>(app: &AppHandle<R>, aviso: &AvisoDoToque) {
    tracing::info!(?aviso, "toque da sala pessoal");
    if let Err(erro) = app.emit(CANAL_DO_TOQUE, aviso) {
        tracing::warn!(%erro, "o toque não chegou à casca");
    }
}

/// O alarme que anota em vez de tocar: nenhum teste toca som de verdade. Aqui,
/// e não dentro de um módulo de teste, porque os testes da sala também o usam.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct AlarmeAnotado(std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>);

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "num teste, o pânico é o relatório")]
impl Alarme for AlarmeAnotado {
    fn comecar(&mut self) -> bool {
        self.0.lock().unwrap().push("comecar");
        true
    }
    fn lembrar(&mut self) {
        self.0.lock().unwrap().push("lembrar");
    }
    fn parar(&mut self) {
        self.0.lock().unwrap().push("parar");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "num teste, o pânico é o relatório")]
impl AlarmeAnotado {
    pub(crate) fn visto(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().clone()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]
mod o_toque_por_evento {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use seele_server::recepcao::{Entrada, Recepcao};
    use tauri::{Listener, Manager};

    use super::{caminho_da_agenda, ouvir, AlarmeAnotado, CANAL_DO_TOQUE};

    struct Banca {
        app: tauri::App<tauri::test::MockRuntime>,
        /// `Option` para o teste da sala que fecha poder largá-la.
        recepcao: Option<Recepcao>,
        alarme: AlarmeAnotado,
        avisos: Arc<Mutex<Vec<serde_json::Value>>>,
        casa: tempfile::TempDir,
    }

    const DONO: [u8; 32] = [1; 32];
    const RAFA: [u8; 32] = [2; 32];

    fn rafa() -> seele_ffi::Codigo {
        seele_ffi::Codigo::da_chave(&RAFA)
    }

    /// Uma recepção de verdade, sem rede: `chegou` toca como a sessão tocaria.
    fn montar() -> Banca {
        let app = tauri::test::mock_builder()
            .manage(crate::Session::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let (eventos, _) = tokio::sync::broadcast::channel(8);
        let recepcao = Recepcao::da_sala(seele_server::recepcao::Sala::de(DONO), eventos);
        let avisos = Arc::new(Mutex::new(Vec::new()));
        let anotados = Arc::clone(&avisos);
        app.listen_any(CANAL_DO_TOQUE, move |evento| {
            anotados
                .lock()
                .unwrap()
                .push(serde_json::from_str(evento.payload()).unwrap());
        });
        Banca {
            app,
            recepcao: Some(recepcao),
            alarme: AlarmeAnotado::default(),
            avisos,
            casa: tempfile::tempdir().unwrap(),
        }
    }

    impl Banca {
        fn ouvir(&self) -> tokio::task::JoinHandle<()> {
            tokio::spawn(ouvir(
                self.app.handle().clone(),
                self.recepcao().assinar(),
                self.casa.path().to_str().unwrap().to_owned(),
                self.alarme.clone(),
            ))
        }

        fn recepcao(&self) -> &Recepcao {
            self.recepcao.as_ref().expect("a campainha já fechou")
        }

        fn tocar(&self, chave: [u8; 32], apelido: &str) -> Entrada {
            self.recepcao().chegou(chave, apelido)
        }

        /// Espera chegar o `n`-ésimo aviso à casca.
        async fn aviso(&self, n: usize) -> serde_json::Value {
            for _ in 0..200 {
                if let Some(aviso) = self.avisos.lock().unwrap().get(n) {
                    return aviso.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!(
                "o aviso {n} não chegou à casca; chegaram {:?}",
                self.avisos.lock().unwrap()
            );
        }

        fn tocando(&self) -> usize {
            self.app
                .state::<crate::Session>()
                .toques
                .lock()
                .unwrap()
                .na_faixa()
                .len()
        }
    }

    #[tokio::test]
    async fn quem_toca_chega_a_casca_por_evento_com_o_nome_da_agenda() {
        let banca = montar();
        seele_ffi::agenda::Agenda::abrir(caminho_da_agenda(banca.casa.path().to_str().unwrap()))
            .unwrap()
            .dar_nome(rafa(), "Rafa")
            .unwrap();
        let _ouvindo = banca.ouvir();
        let mut da_campainha = banca.recepcao().assinar();

        let Entrada::Toca { .. } = banca.tocar(RAFA, "rafa") else {
            panic!("a campainha não tocou");
        };
        let tocou = banca.aviso(0).await;
        let Ok(seele_server::recepcao::Aviso::Tocou { impressao, .. }) = da_campainha.recv().await
        else {
            panic!("a campainha não anunciou o toque");
        };

        assert_eq!(
            tocou,
            serde_json::json!({
                "evento": "Tocou",
                "quem": impressao,
                "codigo": rafa().to_string(),
                "apelido": "rafa",
                "nome_na_agenda": "Rafa",
                "com_som": true,
            })
        );
        assert_eq!(banca.alarme.visto(), ["comecar"]);
        assert_eq!(banca.tocando(), 1);
    }

    #[tokio::test]
    async fn recusar_para_o_alarme_e_anota_a_chamada_perdida() {
        let banca = montar();
        let _ouvindo = banca.ouvir();
        let Entrada::Toca { toque, .. } = banca.tocar(RAFA, "rafa") else {
            panic!("a campainha não tocou");
        };
        banca.aviso(0).await;

        assert!(banca.recepcao().recusar(toque));
        let parou = banca.aviso(1).await;

        assert_eq!(parou["evento"], "Parou");
        assert_eq!(parou["desfecho"], "recusada");
        assert_eq!(banca.alarme.visto(), ["comecar", "parar"]);
        assert_eq!(banca.tocando(), 0);
        let agenda = seele_ffi::agenda::Agenda::abrir(caminho_da_agenda(
            banca.casa.path().to_str().unwrap(),
        ))
        .unwrap();
        let anotado = agenda
            .buscar(&rafa())
            .expect("o toque não foi anotado na agenda");
        assert_eq!(anotado.apelido.as_deref(), Some("rafa"));
        assert_eq!(anotado.direcao, Some(seele_ffi::agenda::Direcao::Perdida));
    }

    /// A mesma pessoa tocando de novo, por outro candidato da corrida, troca o
    /// toque e não soma uma pessoa: o alarme não começa duas vezes, e atender
    /// ou recusar vale para o toque novo.
    #[tokio::test]
    async fn a_mesma_pessoa_tocando_de_novo_troca_o_toque_e_nao_soma() {
        let banca = montar();
        let _ouvindo = banca.ouvir();
        let Entrada::Toca { toque: velho, .. } = banca.tocar(RAFA, "rafa") else {
            panic!("a campainha não tocou");
        };
        banca.aviso(0).await;
        let Entrada::Toca { toque: novo, .. } = banca.tocar(RAFA, "rafa") else {
            panic!("a campainha não tocou de novo");
        };
        assert_eq!(banca.aviso(1).await["evento"], "Tocou");
        assert_ne!(velho, novo);

        assert_eq!(banca.tocando(), 1, "a mesma pessoa virou duas na faixa");
        assert_eq!(
            banca.alarme.visto(),
            ["comecar"],
            "o alarme começou de novo"
        );
        let quem = banca.aviso(0).await["quem"].as_str().unwrap().to_owned();
        assert_eq!(
            banca
                .app
                .state::<crate::Session>()
                .toques
                .lock()
                .unwrap()
                .de(&quem),
            Some(novo),
            "atender iria para o toque que já não existe"
        );

        assert!(banca.recepcao().recusar(novo));
        assert_eq!(banca.aviso(2).await["desfecho"], "recusada");
        assert_eq!(banca.alarme.visto(), ["comecar", "parar"]);
    }

    /// **A sala que fecha no meio de um toque cala o alarme.** Sem isto,
    /// desligar a sala com alguém tocando deixaria o som tocando para sempre:
    /// nenhum `ParouDeTocar` vem de uma campainha que deixou de existir.
    #[tokio::test]
    async fn a_sala_que_fecha_no_meio_de_um_toque_cala_o_alarme() {
        let mut banca = montar();
        let ouvindo = banca.ouvir();
        let entrada = banca.tocar(RAFA, "rafa");
        banca.aviso(0).await;

        // A campainha some com a sala.
        drop(entrada);
        drop(banca.recepcao.take());
        tokio::time::timeout(Duration::from_secs(2), ouvindo)
            .await
            .expect("o vigia do toque não terminou com a sala")
            .unwrap();

        assert_eq!(banca.aviso(1).await["desfecho"], "sala_fechou");
        assert_eq!(banca.alarme.visto(), ["comecar", "parar"]);
        assert_eq!(banca.tocando(), 0);
    }

    /// O mesmo com a sala de verdade: desligá-la (`encerrar`) solta a
    /// campainha, e o vigia termina. Uma referência à recepção esquecida em
    /// algum canto do servidor deixaria o alarme tocando depois do desligar.
    #[tokio::test]
    async fn desligar_a_sala_de_verdade_no_meio_de_um_toque_cala_o_alarme() {
        let banca = montar();
        std::fs::write(banca.casa.path().join("identity.key"), DONO).unwrap();
        let sala = crate::sala::subir_em(banca.casa.path().to_str().unwrap(), 0, None)
            .await
            .unwrap();
        let ouvindo = {
            let recepcao = sala.recepcao().expect("a sala subiu sem campainha");
            let ouvindo = tokio::spawn(ouvir(
                banca.app.handle().clone(),
                recepcao.assinar(),
                banca.casa.path().to_str().unwrap().to_owned(),
                banca.alarme.clone(),
            ));
            let Entrada::Toca { .. } = recepcao.chegou(RAFA, "rafa") else {
                panic!("a campainha da sala não tocou");
            };
            ouvindo
        };
        banca.aviso(0).await;

        sala.encerrar().await;
        tokio::time::timeout(Duration::from_secs(5), ouvindo)
            .await
            .expect(
                "o vigia do toque continuou depois de a sala fechar: o alarme tocaria para sempre",
            )
            .unwrap();

        assert_eq!(banca.aviso(1).await["desfecho"], "sala_fechou");
        assert_eq!(banca.alarme.visto(), ["comecar", "parar"]);
    }
}
