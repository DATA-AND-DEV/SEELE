//! A sala pessoal desta máquina (tarefa 23 da F7; ADRs 0059 e 0060): a segunda
//! hospedagem do app, a que atende pelo código da pessoa.
//!
//! **Sobe sozinha, em segundo plano, no `setup`.** A escada custa até ~4 s, e
//! a janela não espera por ela: o estado chega à casca pelo canal
//! [`CANAL_DA_SALA`], e quem abrir a tela depois do anúncio lê o mesmo estado
//! pelo comando `estado_da_sala`, no `main.rs` com os outros comandos.
//!
//! - **A semente nunca atravessa a webview.** Ela sai da FFI
//!   (`seele_ffi::semente_da_sala`) só dentro de [`subir_em`], e vai direto a
//!   `Hospedagem::iniciar_sala`. Nenhum comando a devolve; o guarda é
//!   `a_semente_da_sala_nunca_atravessa_a_webview`, no `frontend.rs`.
//! - **O ponto é o da produção**, o mesmo de quem liga
//!   (`seele_ffi::ponto_de_encontro`). Enquanto a VPS não fala o ENC/2, a sala
//!   sobe e registra assim mesmo: sem resposta em dois reavivamentos, o
//!   registro vira [`Registro::SemResposta`], o `seele.log` diz (a linha é do
//!   `seele-server`), e a casca diz «SEU CÓDIGO NÃO ESTÁ ACHÁVEL AGORA». Não há
//!   volta pela versão 1: as marcas da sala só existem assinadas.
//! - **Uma porta 8385 ocupada é dita**, e não engolida: [`FalhaDaSala::PortaOcupada`]
//!   vira a frase «NINGUÉM CONSEGUE TE LIGAR AGORA».

use tauri::{AppHandle, Emitter, Manager, Runtime};

/// O canal por onde o estado da sala chega à casca.
///
/// Separado do canal de eventos da sessão pelo mesmo motivo do canal do
/// atualizador: a sala vive sem sessão nenhuma, e quem ouve a conversa não tem
/// por que peneirar o estado dela.
pub(crate) const CANAL_DA_SALA: &str = "seele://sala";

/// Como está a sala pessoal desta máquina, do jeito que a casca a recebe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "estado")]
pub(crate) enum EstadoDaSala {
    /// Subindo: a escada leva até ~4 s. É também o estado antes de a tarefa
    /// do `setup` começar.
    Subindo,
    /// Desligada pela pessoa: «Não receber ligações», no PERFIL. Ninguém
    /// consegue ligar, e é escolha, e não falha.
    Desligada,
    /// No ar, atendendo na porta da sala.
    NoAr {
        /// Se o código está achável pelo ponto de encontro, e por que não.
        registro: Registro,
        /// Windows: a regra do firewall não cobre este executável, e quem liga
        /// de fora da casa não chega (D-sala-1). `None` fora do Windows e quando
        /// está tudo certo. A mesma conferência do HOSPEDAR.
        firewall_nao_cobre: Option<String>,
    },
    /// Não subiu, e o motivo.
    Falhou {
        /// Por que não subiu.
        motivo: FalhaDaSala,
    },
}

impl Default for EstadoDaSala {
    /// Subindo: a sala sobe no `setup`, e até a tarefa dele responder, é o que
    /// é verdade.
    fn default() -> Self {
        Self::Subindo
    }
}

/// Se o código desta máquina está achável pelo ponto de encontro (ENC/2), o
/// espelho de `seele_server::alcance::encontro::EstadoDoRegistro` que a casca
/// recebe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "registro")]
pub(crate) enum Registro {
    /// Sem ponto de encontro: `$SEELE_ENCONTRO` o desligou. Só quem está na
    /// mesma rede acha a sala, e só pelo endereço.
    SemPonto,
    /// Logo depois da subida, antes da primeira resposta do ponto.
    Esperando,
    /// Quem liga pelo código acha esta máquina.
    Registrado,
    /// O ponto recusou porque os relógios diferem demais. Positivo: o daqui
    /// está adiantado.
    RelogioTorto {
        /// O relógio daqui menos o do ponto, em segundos.
        diferenca_s: i64,
    },
    /// O ponto recusou: `velho`, `cheio`, `taxa` ou `relogio`, como o fio diz.
    Recusado {
        /// O motivo, como o fio o escreve.
        motivo: &'static str,
    },
    /// O ponto não aceitou o próprio cookie: esta máquina parece sair para a
    /// internet por mais de um endereço.
    CookieRecusado,
    /// O ponto não respondeu a dois registros seguidos: fora do ar, ou anterior
    /// ao ENC/2.
    SemResposta,
}

/// Por que a sala não subiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum FalhaDaSala {
    /// A porta da sala (8385) está em uso: outro SEELE aberto, ou outro
    /// programa. «NINGUÉM CONSEGUE TE LIGAR AGORA».
    PortaOcupada,
    /// A identidade desta máquina não abriu: sem ela não há semente, e sem
    /// semente não há sala.
    IdentidadeIndisponivel,
    /// Qualquer outra falha da subida; o motivo vai ao `seele.log`.
    NaoSubiu,
}

/// O registro, dito à casca.
pub(crate) fn registro_de(
    estado: Option<seele_server::alcance::encontro::EstadoDoRegistro>,
) -> Registro {
    use seele_server::alcance::encontro::EstadoDoRegistro as Estado;
    match estado {
        None => Registro::SemPonto,
        Some(Estado::Esperando) => Registro::Esperando,
        Some(Estado::Registrado) => Registro::Registrado,
        Some(Estado::RelogioTorto { diferenca_s }) => Registro::RelogioTorto { diferenca_s },
        Some(Estado::Recusado(motivo)) => Registro::Recusado {
            motivo: motivo.texto(),
        },
        Some(Estado::CookieRecusado) => Registro::CookieRecusado,
        Some(Estado::SemResposta) => Registro::SemResposta,
    }
}

/// Sobe a sala desta máquina: a semente e o apelido da pasta de dados
/// `config`, na `porta`, registrando no `ponto` quando há.
///
/// Separada de [`subir`] para o teste subir uma sala de verdade sem janela:
/// porta zero e sem ponto.
pub(crate) async fn subir_em(
    config: &str,
    porta: u16,
    ponto: Option<String>,
) -> Result<seele_server::hospedagem::Hospedagem, FalhaDaSala> {
    let semente = seele_ffi::semente_da_sala(config).map_err(|erro| {
        tracing::warn!(
            ?erro,
            "a sala não sobe: a identidade desta máquina não abriu"
        );
        FalhaDaSala::IdentidadeIndisponivel
    })?;
    // O apelido com que se entra, quando a pessoa já disse um. É o nome da
    // conta de quem recebe, na sala: quem liga vê o nome que quem recebe usa.
    let apelido =
        seele_ffi::preferences::Preferences::open(std::path::Path::new(config).join("preferences"))
            .ok()
            .and_then(|preferencias| preferencias.nickname().map(str::to_owned))
            .unwrap_or_else(|| "SEELE".to_owned());
    seele_server::hospedagem::Hospedagem::iniciar_sala(porta, semente.bytes(), &apelido, ponto)
        .await
        .map_err(|erro| {
            let falha = match crate::classificar(&erro) {
                crate::FalhaAoHospedar::PortaOcupada => FalhaDaSala::PortaOcupada,
                _ => FalhaDaSala::NaoSubiu,
            };
            tracing::warn!(?falha, erro = %format!("{erro:#}"), "a sala pessoal não subiu");
            falha
        })
}

/// Onde e como a sala sobe. A janela de verdade usa [`Subida::da_producao`];
/// os testes sobem com porta zero e sem ponto.
pub(crate) struct Subida {
    /// A pasta de dados: a identidade, o apelido e a escolha de receber.
    pub(crate) config: String,
    /// A porta da sala.
    pub(crate) porta: u16,
    /// O ponto de encontro em que a sala se registra.
    pub(crate) ponto: Option<String>,
}

impl Subida {
    /// A da janela: a pasta de dados do app, a porta da sala (8385), e o ponto
    /// de quem liga, que é a leitura única do ambiente.
    pub(crate) fn da_producao(app: &AppHandle) -> Self {
        Self {
            config: crate::config_dir(app),
            porta: seele_server::hospedagem::PORTA_DA_SALA,
            ponto: seele_ffi::ponto_de_encontro(),
        }
    }
}

/// A subida do `setup`: respeita «Não receber ligações». Roda numa tarefa
/// própria.
pub(crate) async fn subir<R: Runtime>(app: AppHandle<R>, subida: Subida) {
    let receber = seele_ffi::preferences::Preferences::open(
        std::path::Path::new(&subida.config).join("preferences"),
    )
    .map_or(true, |preferencias| preferencias.receber_ligacoes());
    if !receber {
        anunciar(&app, EstadoDaSala::Desligada);
        return;
    }
    subir_agora(app, subida).await;
}

/// Sobe a sala, guarda-a nas hospedagens e anuncia cada mudança do registro
/// até ela cair.
pub(crate) async fn subir_agora<R: Runtime>(app: AppHandle<R>, subida: Subida) {
    anunciar(&app, EstadoDaSala::Subindo);
    let sala = match subir_em(&subida.config, subida.porta, subida.ponto).await {
        Ok(sala) => sala,
        Err(motivo) => {
            anunciar(&app, EstadoDaSala::Falhou { motivo });
            return;
        }
    };
    let registro = sala.registro_no_quarto();
    // Assinada **antes** de a sala ir para as hospedagens: um toque que chegue
    // no instante seguinte já tem quem o ouça.
    let campainha = sala.recepcao().map(|recepcao| recepcao.assinar());
    if let Some(largada) = guardar_se_ainda_subindo(&app.state::<crate::Session>(), sala) {
        tracing::info!("a sala terminou de subir depois de desligada, e desce");
        largada.encerrar().await;
        return;
    }
    match campainha {
        Some(avisos) => {
            let alarme = crate::toque::AlarmeDaJanela::novo(app.clone(), subida.config.clone());
            tokio::spawn(crate::toque::ouvir(
                app.clone(),
                avisos,
                subida.config.clone(),
                alarme,
            ));
        }
        None => tracing::error!("a sala subiu sem campainha: ninguém vai ouvir quem tocar"),
    }
    let firewall_nao_cobre = crate::firewall_nao_cobre_este_executavel();
    let no_ar = |registro| EstadoDaSala::NoAr {
        registro,
        firewall_nao_cobre: firewall_nao_cobre.clone(),
    };
    let Some(mut registro) = registro else {
        anunciar(&app, no_ar(Registro::SemPonto));
        return;
    };
    let primeiro = *registro.borrow_and_update();
    anunciar(&app, no_ar(registro_de(Some(primeiro))));
    // Até a sala cair: o emissor do registro vive com a escada dela.
    while registro.changed().await.is_ok() {
        let agora = *registro.borrow_and_update();
        anunciar(&app, no_ar(registro_de(Some(agora))));
    }
}

/// Guarda nas hospedagens a sala que acabou de subir, **se ninguém a desligou
/// enquanto ela subia**: a escada leva segundos, e a pessoa pode desligar no
/// meio dela. Devolve a sala a quem chamou, para descer, quando o estado já
/// não é [`EstadoDaSala::Subindo`]; `None` é a sala guardada.
pub(crate) fn guardar_se_ainda_subindo(
    sessao: &crate::Session,
    sala: seele_server::hospedagem::Hospedagem,
) -> Option<seele_server::hospedagem::Hospedagem> {
    let Ok(mut hospedagens) = sessao.hospedagens.lock() else {
        tracing::error!("a sala subiu e não há onde guardá-la: as hospedagens envenenaram");
        return Some(sala);
    };
    let subindo = sessao
        .sala
        .lock()
        .is_ok_and(|estado| *estado == EstadoDaSala::Subindo);
    if !subindo {
        return Some(sala);
    }
    hospedagens.pessoal = Some(crate::Hospedado {
        hospedagem: sala,
        servidor: None,
    });
    None
}

/// Desliga a sala: «Não receber ligações». Quem estiver nela, ou tocando,
/// ouve «chamada encerrada».
///
/// O estado vira [`EstadoDaSala::Desligada`] **antes** da despedida: uma
/// subida em curso confere o estado ao terminar
/// ([`guardar_se_ainda_subindo`]), e desce sozinha.
///
/// `encerrar`, e não o `despedir` da saída do app: os dois avisam quem está
/// dentro, e só o `encerrar` espera a porta voltar ao sistema. O app continua
/// aberto, e religar em seguida tem de achar a 8385 livre.
pub(crate) async fn desligar<R: Runtime>(app: &AppHandle<R>) {
    let sala = app
        .state::<crate::Session>()
        .hospedagens
        .lock()
        .ok()
        .and_then(|mut hospedagens| hospedagens.pessoal.take());
    anunciar(app, EstadoDaSala::Desligada);
    if let Some(sala) = sala {
        sala.hospedagem.encerrar().await;
    }
}

/// Guarda o estado, para quem perguntar depois, e o anuncia à casca.
fn anunciar<R: Runtime>(app: &AppHandle<R>, estado: EstadoDaSala) {
    tracing::info!(?estado, "sala pessoal");
    if let Ok(mut guardado) = app.state::<crate::Session>().sala.lock() {
        guardado.clone_from(&estado);
    }
    if let Err(erro) = app.emit(CANAL_DA_SALA, &estado) {
        tracing::warn!(%erro, "o estado da sala não chegou à casca");
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod a_sala_pessoal {
    use super::{
        desligar, guardar_se_ainda_subindo, registro_de, subir, subir_em, EstadoDaSala,
        FalhaDaSala, Registro, Subida,
    };
    use tauri::Manager;

    /// Uma pasta de dados com a chave desta «máquina» dentro.
    fn casa_com_a_chave(semente: u8) -> tempfile::TempDir {
        let casa = tempfile::tempdir().unwrap();
        std::fs::write(casa.path().join("identity.key"), [semente; 32]).unwrap();
        casa
    }

    #[tokio::test]
    async fn a_sala_sobe_com_a_chave_desta_maquina_e_com_a_campainha() {
        let casa = casa_com_a_chave(3);
        let sala = subir_em(casa.path().to_str().unwrap(), 0, None)
            .await
            .expect("a sala não subiu");
        assert_eq!(
            sala.impressao_digital(),
            seele_server::tls::Identity::da_semente(&[3; 32])
                .unwrap()
                .fingerprint(),
            "a sala subiu com outra chave: o código desta máquina não tocaria nela"
        );
        assert!(sala.recepcao().is_some(), "a sala subiu sem campainha");
        sala.encerrar().await;
    }

    #[tokio::test]
    async fn a_porta_ocupada_e_dita() {
        let casa = casa_com_a_chave(3);
        let ocupante = std::net::UdpSocket::bind("[::]:0").unwrap();
        let porta = ocupante.local_addr().unwrap().port();
        let subida = subir_em(casa.path().to_str().unwrap(), porta, None).await;
        assert_eq!(
            subida.err(),
            Some(FalhaDaSala::PortaOcupada),
            "a porta ocupada virou outra falha, e a casca não diria «a porta 8385 está em uso»"
        );
        drop(ocupante);
    }

    #[tokio::test]
    async fn sem_a_identidade_a_sala_diz_por_que() {
        // A pasta de dados é um arquivo: a chave não tem onde ser criada.
        let arquivo = tempfile::NamedTempFile::new().unwrap();
        let subida = subir_em(arquivo.path().to_str().unwrap(), 0, None).await;
        assert_eq!(subida.err(), Some(FalhaDaSala::IdentidadeIndisponivel));
    }

    #[test]
    fn cada_estado_do_registro_chega_a_casca_com_o_seu_nome() {
        use seele_server::alcance::encontro::EstadoDoRegistro as E;
        assert_eq!(registro_de(None), Registro::SemPonto);
        assert_eq!(registro_de(Some(E::Esperando)), Registro::Esperando);
        assert_eq!(registro_de(Some(E::Registrado)), Registro::Registrado);
        assert_eq!(
            registro_de(Some(E::RelogioTorto { diferenca_s: -400 })),
            Registro::RelogioTorto { diferenca_s: -400 }
        );
        assert_eq!(
            registro_de(Some(E::CookieRecusado)),
            Registro::CookieRecusado
        );
        assert_eq!(registro_de(Some(E::SemResposta)), Registro::SemResposta);
        // O `Recusado` não se monta daqui: o motivo é do `seele-proto`, que o
        // app não vê, e chega como `Motivo::texto`, o nome do fio.
    }

    #[test]
    fn o_estado_chega_a_casca_com_os_nomes_que_ela_le() {
        let estado = EstadoDaSala::NoAr {
            registro: Registro::RelogioTorto { diferenca_s: 400 },
            firewall_nao_cobre: None,
        };
        assert_eq!(
            serde_json::to_value(&estado).unwrap(),
            serde_json::json!({
                "estado": "NoAr",
                "registro": { "registro": "RelogioTorto", "diferenca_s": 400 },
                "firewall_nao_cobre": null,
            })
        );
        assert_eq!(
            serde_json::to_value(EstadoDaSala::Falhou {
                motivo: FalhaDaSala::PortaOcupada
            })
            .unwrap(),
            serde_json::json!({ "estado": "Falhou", "motivo": "PortaOcupada" })
        );
    }

    /// O app de verdade não sobe sem janela; o runtime falso do Tauri sobe, com a
    /// mesma `Session` que o `main` gerencia.
    fn app_de_teste() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(crate::Session::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("o app de teste não montou")
    }

    /// A subida de um teste: porta zero e sem ponto de encontro.
    fn subida_de_teste(casa: &tempfile::TempDir) -> Subida {
        Subida {
            config: casa.path().to_str().unwrap().to_owned(),
            porta: 0,
            ponto: None,
        }
    }

    fn estado(app: &tauri::App<tauri::test::MockRuntime>) -> EstadoDaSala {
        app.state::<crate::Session>().sala.lock().unwrap().clone()
    }

    fn no_ar(app: &tauri::App<tauri::test::MockRuntime>) -> bool {
        app.state::<crate::Session>()
            .hospedagens
            .lock()
            .unwrap()
            .pessoal
            .is_some()
    }

    #[tokio::test]
    async fn quem_escolheu_nao_receber_ligacoes_abre_o_app_sem_sala() {
        let casa = casa_com_a_chave(4);
        seele_ffi::preferences::Preferences::open(casa.path().join("preferences"))
            .unwrap()
            .set_receber_ligacoes(false)
            .unwrap();
        let app = app_de_teste();

        subir(app.handle().clone(), subida_de_teste(&casa)).await;

        assert_eq!(estado(&app), EstadoDaSala::Desligada);
        assert!(
            !no_ar(&app),
            "a sala subiu para quem escolheu «Não receber ligações»"
        );
    }

    #[tokio::test]
    async fn a_sala_que_sobe_fica_nas_hospedagens_e_desligar_a_despede() {
        let casa = casa_com_a_chave(4);
        let app = app_de_teste();

        subir(app.handle().clone(), subida_de_teste(&casa)).await;
        assert_eq!(
            estado(&app),
            EstadoDaSala::NoAr {
                registro: Registro::SemPonto,
                firewall_nao_cobre: crate::firewall_nao_cobre_este_executavel(),
            }
        );
        assert!(no_ar(&app), "a sala subiu e não ficou nas hospedagens");
        let porta = app
            .state::<crate::Session>()
            .hospedagens
            .lock()
            .unwrap()
            .portas()
            .1;

        desligar(app.handle()).await;

        assert_eq!(estado(&app), EstadoDaSala::Desligada);
        assert!(!no_ar(&app), "a sala desligada ficou nas hospedagens");
        // A porta voltou: a sala foi despedida, e não só esquecida.
        let porta = porta.expect("a sala atendia numa porta");
        assert!(
            std::net::UdpSocket::bind(("::", porta)).is_ok(),
            "a sala desligada continua segurando a porta {porta}"
        );
    }

    /// **A sala que termina de subir depois de desligada desce.** A escada
    /// leva segundos, e a pessoa pode desligar no meio dela: sem esta conferência,
    /// a sala ficaria no ar com o PERFIL dizendo que ela está desligada.
    #[tokio::test]
    async fn a_sala_que_termina_de_subir_depois_do_desligar_nao_fica() {
        let casa = casa_com_a_chave(4);
        let app = app_de_teste();
        let sessao = app.state::<crate::Session>();

        *sessao.sala.lock().unwrap() = EstadoDaSala::Desligada;
        let sala = subir_em(casa.path().to_str().unwrap(), 0, None)
            .await
            .unwrap();
        let Some(largada) = guardar_se_ainda_subindo(&sessao, sala) else {
            panic!("a sala que terminou de subir depois do desligar ficou nas hospedagens");
        };
        largada.encerrar().await;
        assert!(!no_ar(&app));

        *sessao.sala.lock().unwrap() = EstadoDaSala::Subindo;
        let sala = subir_em(casa.path().to_str().unwrap(), 0, None)
            .await
            .unwrap();
        assert!(
            guardar_se_ainda_subindo(&sessao, sala).is_none(),
            "a sala que subiu sem ninguém desligar não foi guardada"
        );
        desligar(app.handle()).await;
    }

    /// **Produção sobe na porta da sala, com o ponto de quem liga.** Os testes
    /// sobem com porta zero e sem ponto, e só este texto prova o que a janela
    /// de verdade faz: a subida dela é a [`Subida::da_producao`], e toda subida
    /// do `main.rs` passa por ela.
    #[test]
    fn a_janela_sobe_a_sala_na_porta_dela_com_o_ponto_de_quem_liga() {
        let compacto =
            |texto: &str| -> String { texto.chars().filter(|c| !c.is_whitespace()).collect() };
        let fonte = include_str!("sala.rs");
        let da_producao = fonte
            .split("pub(crate) fn da_producao(app: &AppHandle) -> Self {")
            .nth(1)
            .and_then(|depois| depois.split("\n    }\n").next())
            .unwrap();
        assert!(
            compacto(da_producao).contains(
                "config:crate::config_dir(app),porta:seele_server::hospedagem::PORTA_DA_SALA,ponto:seele_ffi::ponto_de_encontro(),"
            ),
            "a janela não sobe a sala na 8385 com o ponto de quem liga: quem liga pelo código \
             pergunta num ponto em que esta sala não se registrou"
        );
        let main = compacto(include_str!("main.rs"));
        let subidas =
            main.matches("sala::subir(").count() + main.matches("sala::subir_agora(").count();
        assert!(
            subidas >= 2,
            "o `main.rs` não sobe a sala no `setup` e no religar"
        );
        assert_eq!(
            main.matches("sala::Subida::da_producao(").count(),
            subidas,
            "uma subida do `main.rs` não passa pela `Subida::da_producao`"
        );
    }
}
