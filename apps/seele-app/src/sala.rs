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

use tauri::{AppHandle, Emitter, Manager};

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

/// Sobe a sala no ar de verdade: a porta da sala, o ponto da produção, e o
/// estado anunciado a cada mudança. Roda numa tarefa própria, chamada pelo
/// `setup`.
pub(crate) async fn subir(app: AppHandle) {
    anunciar(&app, EstadoDaSala::Subindo);
    let subida = subir_em(
        &crate::config_dir(&app),
        seele_server::hospedagem::PORTA_DA_SALA,
        seele_ffi::ponto_de_encontro(),
    )
    .await;
    let sala = match subida {
        Ok(sala) => sala,
        Err(motivo) => {
            anunciar(&app, EstadoDaSala::Falhou { motivo });
            return;
        }
    };
    let registro = sala.registro_no_quarto();
    let firewall_nao_cobre = crate::firewall_nao_cobre_este_executavel();
    let no_ar = |registro| EstadoDaSala::NoAr {
        registro,
        firewall_nao_cobre: firewall_nao_cobre.clone(),
    };
    {
        let sessao = app.state::<crate::Session>();
        let Ok(mut hospedagens) = sessao.hospedagens.lock() else {
            tracing::error!("a sala subiu e não há onde guardá-la: as hospedagens envenenaram");
            sala.encerrar().await;
            anunciar(
                &app,
                EstadoDaSala::Falhou {
                    motivo: FalhaDaSala::NaoSubiu,
                },
            );
            return;
        };
        hospedagens.pessoal = Some(crate::Hospedado {
            hospedagem: sala,
            servidor: None,
        });
    }
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

/// Guarda o estado, para quem perguntar depois, e o anuncia à casca.
fn anunciar(app: &AppHandle, estado: EstadoDaSala) {
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
    use super::{registro_de, subir_em, EstadoDaSala, FalhaDaSala, Registro};

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

    /// **Produção sobe na porta da sala, com o ponto de quem liga.** Os testes
    /// sobem com porta zero e sem ponto, e só este texto prova o que a janela
    /// de verdade faz.
    #[test]
    fn a_janela_sobe_a_sala_na_porta_dela_com_o_ponto_de_quem_liga() {
        let fonte = include_str!("sala.rs");
        let corpo = fonte
            .split("pub(crate) async fn subir(app: AppHandle)")
            .nth(1)
            .and_then(|depois| depois.split("\n}\n").next())
            .unwrap();
        let compacto: String = corpo.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            compacto.contains(
                "subir_em(&crate::config_dir(&app),seele_server::hospedagem::PORTA_DA_SALA,seele_ffi::ponto_de_encontro(),)"
            ),
            "a janela não sobe a sala na 8385 com o ponto de quem liga: quem liga pelo código \
             pergunta num ponto em que esta sala não se registrou"
        );
    }
}
