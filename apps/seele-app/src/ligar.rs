//! **Do lado de quem liga** (tarefas 28 e 30 da F7; spec de 23/09, «Do lado de
//! quem liga»).
//!
//! A `sala.rs` é o lado de quem recebe: a sala que sobe, toca e atende, e que
//! mexe nas hospedagens deste app. Quem liga não hospeda nada: lê o texto colado
//! em CONECTAR e disca, pela ponte, o alvo `@CÓDIGO`. Por isso mora à parte: a
//! ponte do navegador separa os comandos que mexem na hospedagem local, e uma
//! leitura de texto não é um deles.

/// O que um texto colado em CONECTAR é, para a casca (tarefa 30 da F7, a 6g;
/// spec de 23/09, «Como um texto colado é classificado» e «Do lado de quem
/// liga», 1).
///
/// A leitura é a do núcleo (`seele_ffi::entrada::ler_entrada`): a casca manda
/// **todo** texto, e o `startsWith("seele://")` dela sai. A pergunta «é o meu
/// código?» é respondida aqui, com a chave desta máquina, e não na tela.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum EntradaColada {
    /// Um endereço, como foi escrito.
    Endereco(String),
    /// Um convite `seele://…`, como foi escrito.
    Convite(String),
    /// O código de outra pessoa.
    Codigo {
        /// O código, como a tela o mostra.
        codigo: String,
        /// O alvo canônico de quem liga, `@CÓDIGO`
        /// (`seele_ffi::alvo_do_codigo`): ele se basta sozinho, e sobrevive ao
        /// `ejetar`.
        alvo: String,
        /// O nome que esta máquina deu a essa pessoa na agenda, quando deu: é o
        /// «ALICE» de «CHAMANDO ALICE…» (tarefa 28 da F7). Sem ele, a tela diz o
        /// código.
        nome: Option<String>,
    },
    /// Tem a forma de um código, e não fecha: «ESSE CÓDIGO TEM UM ERRO DE
    /// DIGITAÇÃO», dito sem consultar a rede.
    CodigoComErro,
    /// O código desta máquina: «ESSE É O SEU CÓDIGO».
    SeuCodigo,
}

/// [`EntradaColada`] de um texto, com o código desta máquina, quando ele se
/// lê, e o nome que a agenda dá a um código. Sem o código desta máquina (a
/// identidade não abriu), nenhum código é «o seu».
pub(crate) fn ler_entrada_em(
    texto: &str,
    meu: Option<&seele_ffi::Codigo>,
    nome_de: impl Fn(&seele_ffi::Codigo) -> Option<String>,
) -> EntradaColada {
    match seele_ffi::entrada::ler_entrada(texto) {
        seele_ffi::entrada::Entrada::Endereco(endereco) => EntradaColada::Endereco(endereco),
        seele_ffi::entrada::Entrada::Convite(convite) => EntradaColada::Convite(convite),
        seele_ffi::entrada::Entrada::CodigoComErro => EntradaColada::CodigoComErro,
        seele_ffi::entrada::Entrada::Codigo(codigo) if meu == Some(&codigo) => {
            EntradaColada::SeuCodigo
        }
        seele_ffi::entrada::Entrada::Codigo(codigo) => EntradaColada::Codigo {
            codigo: codigo.to_string(),
            alvo: seele_ffi::alvo_do_codigo(&codigo),
            nome: nome_de(&codigo),
        },
    }
}

/// O nome que esta máquina deu a um código na agenda, quando deu. Uma agenda
/// que não abre não tem nome nenhum, e o `seele.log` diz por quê.
pub(crate) fn nome_na_agenda(config: &str, codigo: &seele_ffi::Codigo) -> Option<String> {
    let agenda = seele_ffi::agenda::Agenda::abrir(crate::toque::caminho_da_agenda(config))
        .map_err(|erro| tracing::warn!(erro = %format!("{erro:#}"), "a agenda não abriu"))
        .ok()?;
    agenda
        .buscar(codigo)
        .and_then(|contato| contato.nome.clone())
        .filter(|nome| !nome.trim().is_empty())
}

/// **A ligação em voo**, do `connect` que a abriu até ele voltar (tarefa 28 da
/// F7, o CANCELAR de quem está chamando alguém).
///
/// O [`seele_ffi::Cancelamento`] fica na sessão enquanto a chegada anda, e o
/// [`cancelar_em`] o acha lá. Ao cair, o guarda devolve a vaga, **mas só se ela
/// ainda é dele**: uma tentativa mais nova pode ter tomado o lugar (o E54), e o
/// CANCELAR passa a ser dela.
pub(crate) struct LigacaoEmVoo<'a> {
    sessao: &'a crate::Session,
    cancelamento: std::sync::Arc<seele_ffi::Cancelamento>,
}

impl<'a> LigacaoEmVoo<'a> {
    /// Abre a vaga desta tentativa, por cima da de qualquer outra.
    pub(crate) fn abrir(sessao: &'a crate::Session) -> Self {
        let cancelamento = seele_ffi::Cancelamento::novo();
        if let Ok(mut vaga) = sessao.ligacao_em_voo.lock() {
            *vaga = Some(std::sync::Arc::clone(&cancelamento));
        }
        Self {
            sessao,
            cancelamento,
        }
    }

    /// O cancelamento que vai à ponte (`Connection::connect_cancelavel`).
    pub(crate) fn cancelamento(&self) -> std::sync::Arc<seele_ffi::Cancelamento> {
        std::sync::Arc::clone(&self.cancelamento)
    }
}

impl Drop for LigacaoEmVoo<'_> {
    fn drop(&mut self) {
        if let Ok(mut vaga) = self.sessao.ligacao_em_voo.lock() {
            if vaga
                .as_ref()
                .is_some_and(|em_voo| std::sync::Arc::ptr_eq(em_voo, &self.cancelamento))
            {
                *vaga = None;
            }
        }
    }
}

/// CANCELAR: larga a ligação em voo, se houver uma (tarefa 28 da F7). O QUIC de
/// cada candidato fecha, a sala que tocava vê `ParouDeTocar(Cancelada)`, e o
/// `connect` volta com `ConnectionError::Cancelada`. Devolve se havia o que
/// cancelar: apertado duas vezes, a segunda não acha nada.
pub(crate) fn cancelar_em(sessao: &crate::Session) -> bool {
    let em_voo = sessao
        .ligacao_em_voo
        .lock()
        .ok()
        .and_then(|mut vaga| vaga.take());
    let Some(em_voo) = em_voo else {
        return false;
    };
    em_voo.cancelar_ligacao();
    tracing::info!("CANCELAR: a ligação em voo foi largada");
    true
}

/// **O que uma chegada anota, e se ela segue para a lista de visitados.**
///
/// - **Uma ligação não é visita** (decisão de 16/10 na F7): quem liga para uma
///   pessoa não volta a um servidor. A ligação fica só na agenda, com a
///   direção e a hora.
/// - **O critério é a origem da tentativa**, e quem a sabe é a ponte: a
///   conexão discada pelo código diz por qual
///   (`Connection::codigo_da_ligacao`), e `ligou_para` é essa resposta. O
///   texto do alvo não entra, e uma sala achada pelo endereço é visita, mesmo
///   tendo tocado.
/// - **Um servidor desta máquina também não segue**: `127.0.0.1` não é lugar
///   aonde se volta, é o botão HOSPEDAR.
///
/// O alvo da sessão fica só com uma visita. É por ele que o caminho medido e a
/// aparência do servidor vão para a lista, e o de uma visita anterior, que
/// ficava na sessão, levaria para aquele servidor o que esta chegada mediu.
pub(crate) fn anotar_a_chegada(
    sessao: &crate::Session,
    config: &str,
    alvo: &str,
    ligou_para: Option<seele_ffi::Codigo>,
) -> bool {
    let visita = match ligou_para {
        Some(codigo) => {
            anotar_a_ligacao_feita(config, codigo);
            false
        }
        None => !crate::endereco_desta_maquina(alvo),
    };
    if let Ok(mut guardado) = sessao.alvo.lock() {
        *guardado = visita.then(|| alvo.to_owned());
    }
    visita
}

/// A ligação feita, na agenda: para que lado e quando. O nome que esta máquina
/// deu fica, e o apelido também — é o que a pessoa disse ao ligar para cá, e
/// uma ligação daqui não traz outro.
fn anotar_a_ligacao_feita(config: &str, codigo: seele_ffi::Codigo) {
    let quando = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |desde| {
            i64::try_from(desde.as_secs()).unwrap_or(i64::MAX)
        });
    let anotado = seele_ffi::agenda::Agenda::abrir(crate::toque::caminho_da_agenda(config))
        .and_then(|mut agenda| {
            let apelido = agenda
                .buscar(&codigo)
                .and_then(|contato| contato.apelido.clone())
                .unwrap_or_default();
            agenda.anotar_ligacao(codigo, &apelido, seele_ffi::agenda::Direcao::Liguei, quando)
        });
    if let Err(erro) = anotado {
        tracing::warn!(
            erro = %format!("{erro:#}"),
            "a ligação feita não foi anotada na agenda"
        );
    }
}

/// A entrada colada em CONECTAR (tarefa 30 da F7): a leitura do núcleo, com «é o
/// seu código?» respondido pela chave desta máquina.
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod a_entrada_colada {
    use super::{ler_entrada_em, nome_na_agenda, EntradaColada};
    use seele_ffi::Codigo;

    /// O código da chave de zeros, o mesmo dos vetores do `seele-proto`.
    const DE_ZEROS: &str = "CSM7-NBFR-CAYQ-EV4Y";

    fn zeros() -> Codigo {
        Codigo::da_chave(&[0; 32])
    }

    /// Uma agenda sem nome nenhum.
    fn sem_nome(_: &Codigo) -> Option<String> {
        None
    }

    #[test]
    fn enderecos_e_convites_passam_como_foram_escritos() {
        for endereco in ["casa.exemplo:8383", "10.0.0.1:8383", "[::1]:8383"] {
            assert_eq!(
                ler_entrada_em(endereco, None, sem_nome),
                EntradaColada::Endereco(endereco.to_owned())
            );
        }
        assert_eq!(
            ler_entrada_em("seele://casa.exemplo:8383", None, sem_nome),
            EntradaColada::Convite("seele://casa.exemplo:8383".to_owned())
        );
    }

    #[test]
    fn o_codigo_de_outra_pessoa_vem_com_o_alvo_de_quem_liga() {
        let outro = Codigo::da_chave(&[1; 32]);
        assert_eq!(
            ler_entrada_em(DE_ZEROS, Some(&outro), sem_nome),
            EntradaColada::Codigo {
                codigo: DE_ZEROS.to_owned(),
                alvo: seele_ffi::alvo_do_codigo(&zeros()),
                nome: None,
            }
        );
        // O link da sala também é código, e o alvo é o mesmo.
        assert_eq!(
            ler_entrada_em(&format!("seele://@{DE_ZEROS}"), None, sem_nome),
            ler_entrada_em(DE_ZEROS, None, sem_nome)
        );
        // E o nome é o que a agenda dá àquele código, e só a ele.
        let alice = |codigo: &Codigo| (*codigo == zeros()).then(|| "Alice".to_owned());
        assert!(matches!(
            ler_entrada_em(DE_ZEROS, Some(&outro), alice),
            EntradaColada::Codigo { nome: Some(nome), .. } if nome == "Alice"
        ));
    }

    #[test]
    fn o_codigo_desta_maquina_e_o_seu_como_quer_que_seja_escrito() {
        for escrito in [
            DE_ZEROS,
            "csm7 nbfr cayq ev4y",
            &format!("seele://@{DE_ZEROS}"),
        ] {
            assert_eq!(
                ler_entrada_em(escrito, Some(&zeros()), sem_nome),
                EntradaColada::SeuCodigo,
                "`{escrito}`"
            );
        }
        // Sem a chave desta máquina, nenhum código é o seu.
        assert!(matches!(
            ler_entrada_em(DE_ZEROS, None, sem_nome),
            EntradaColada::Codigo { .. }
        ));
    }

    #[test]
    fn o_nome_vem_da_agenda_desta_maquina() {
        let casa = tempfile::tempdir().expect("a pasta de teste não abriu");
        let config = casa.path().display().to_string();
        let alice = Codigo::da_chave(&[1; 32]);
        assert_eq!(nome_na_agenda(&config, &alice), None, "sem agenda, um nome");
        crate::sala::dar_nome_em(&config, &alice.to_string(), "Alice").unwrap();
        assert_eq!(nome_na_agenda(&config, &alice).as_deref(), Some("Alice"));
        assert_eq!(
            nome_na_agenda(&config, &zeros()),
            None,
            "o nome de uma pessoa foi dado a outro código"
        );
    }

    #[test]
    fn um_codigo_que_nao_fecha_e_erro_de_digitacao() {
        assert_eq!(
            ler_entrada_em("CSM7-NBFR-CAYQ-EV4Z", Some(&zeros()), sem_nome),
            EntradaColada::CodigoComErro
        );
    }

    #[test]
    fn a_casca_le_a_entrada_pelo_nome_da_variante() {
        // A tela escolhe a frase e o caminho por estes nomes.
        assert_eq!(
            serde_json::to_value(ler_entrada_em(DE_ZEROS, None, sem_nome)).ok(),
            Some(serde_json::json!({
                "Codigo": { "codigo": DE_ZEROS, "alvo": format!("@{DE_ZEROS}"), "nome": null }
            }))
        );
        assert_eq!(
            serde_json::to_value(EntradaColada::SeuCodigo).ok(),
            Some(serde_json::json!("SeuCodigo"))
        );
        assert_eq!(
            serde_json::to_value(EntradaColada::CodigoComErro).ok(),
            Some(serde_json::json!("CodigoComErro"))
        );
    }
}

/// **CANCELAR larga a ligação que toca** (tarefa 28 da F7). Pelo `connect` de
/// verdade, no runtime falso do Tauri, contra uma sala de verdade: ela toca, o
/// CANCELAR larga a chegada, a sala vê `ParouDeTocar(Cancelada)`, o `connect`
/// volta `Cancelada`, e a vaga fica vazia para a próxima.
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod o_cancelar_da_ligacao {
    use super::{cancelar_em, LigacaoEmVoo};
    use crate::a_entrada_velha_que_chega_tarde::{app, runtime, servidor, vez};
    use crate::Session;
    use seele_server::campainha::Desfecho;
    use seele_server::recepcao::{Aviso, Sala};
    use seele_server::{Daemon, ServerConfig};
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;
    use tauri::Manager;

    /// A semente de quem recebe.
    const DONO: u8 = 1;

    async fn sala() -> Arc<Daemon> {
        // A chave do dono só precisa não ser a de quem liga (a do app, gerada na
        // casa do teste): quem não é o dono toca.
        let dono = [DONO; 32];
        let config = ServerConfig {
            identidade: Some(seele_server::tls::Identity::da_semente(&[DONO; 32]).unwrap()),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            voice_room_limit: 6,
            sala: Some(Sala {
                dono,
                prazo_do_toque: Duration::from_secs(30),
            }),
            ..ServerConfig::default()
        };
        let servidor = Arc::new(Daemon::bind(config).await.expect("a sala não subiu"));
        let rodando = Arc::clone(&servidor);
        tokio::spawn(async move { rodando.run().await });
        servidor
    }

    #[test]
    fn cancelar_larga_a_ligacao_que_toca_e_a_sala_ve_cancelada() {
        let _vez = vez();
        runtime().block_on(async {
            let sala = sala().await;
            let recepcao = sala.server().recepcao.clone().expect("a sala tem recepção");
            let mut avisos = recepcao.assinar();
            let app = app();
            let alvo = sala.local_addr().unwrap().to_string();
            let ligando = std::thread::spawn(move || {
                runtime().block_on(crate::connect(
                    app.clone(),
                    app.state::<Session>(),
                    alvo,
                    "quem-liga".to_owned(),
                    false,
                    None,
                ))
            });

            let toque = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if let Ok(Aviso::Tocou { toque, .. }) = avisos.recv().await {
                        return toque;
                    }
                }
            })
            .await
            .expect("a sala não tocou");

            assert!(
                cancelar_em(&app.state::<Session>()),
                "o CANCELAR não achou a ligação em voo"
            );
            let desfecho = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Ok(Aviso::ParouDeTocar {
                        toque: qual,
                        desfecho,
                        ..
                    }) = avisos.recv().await
                    {
                        if qual == toque {
                            return desfecho;
                        }
                    }
                }
            })
            .await
            .expect("a sala não viu a ligação parar em 3 s depois do CANCELAR");
            assert_eq!(desfecho, Desfecho::Cancelada);

            let fim = tokio::time::Instant::now() + Duration::from_secs(5);
            while !ligando.is_finished() {
                assert!(
                    tokio::time::Instant::now() < fim,
                    "o connect cancelado não voltou"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let resultado = ligando.join().expect("a linha do connect caiu");
            let Err(falha) = resultado else {
                panic!("a ligação cancelada entrou");
            };
            let dita = serde_json::to_value(&falha).unwrap_or_default();
            assert_eq!(
                dita.get("error"),
                Some(&serde_json::json!("Cancelada")),
                "o connect cancelado não voltou como «cancelada»: {dita}"
            );
            assert!(
                !cancelar_em(&app.state::<Session>()),
                "a vaga da ligação ficou ocupada depois de o connect voltar"
            );
        });
    }

    /// A vaga é da tentativa mais nova: a velha que cai depois não a esvazia (o
    /// E54), e a nova, ao cair, sim.
    #[test]
    fn a_vaga_e_da_tentativa_mais_nova() {
        let sessao = Session::default();
        let velha = LigacaoEmVoo::abrir(&sessao);
        let nova = LigacaoEmVoo::abrir(&sessao);
        let dela = nova.cancelamento();
        drop(velha);
        let na_vaga = sessao.ligacao_em_voo.lock().unwrap().clone();
        assert!(
            na_vaga.is_some_and(|em_voo| Arc::ptr_eq(&em_voo, &dela)),
            "a tentativa velha, ao cair, esvaziou a vaga da nova"
        );
        drop(nova);
        assert!(
            sessao.ligacao_em_voo.lock().unwrap().is_none(),
            "a tentativa que caiu deixou a vaga ocupada"
        );
    }

    /// Depois de uma entrada sem CANCELAR, não sobra ligação a cancelar.
    #[test]
    fn depois_de_entrar_nao_sobra_ligacao_a_cancelar() {
        let _vez = vez();
        runtime().block_on(async {
            let (endereco, servidor) = servidor("Sem sala").await;
            let app = app();
            let entrou = crate::connect(
                app.clone(),
                app.state::<Session>(),
                endereco.to_string(),
                "quem-entra".to_owned(),
                false,
                None,
            )
            .await;
            assert!(entrou.is_ok(), "a entrada num servidor sem sala não entrou");
            assert!(
                !cancelar_em(&app.state::<Session>()),
                "a vaga da ligação ficou ocupada depois de o connect voltar"
            );
            if let Ok(sessao) = app.state::<Session>().connection() {
                sessao.disconnect();
            }
            servidor.shutdown();
        });
    }
}

/// **Uma ligação não é visita** (decisão de 16/10 na F7): a chegada pelo
/// código fica só na agenda, com a direção e a hora, e a lista de visitados
/// não a vê. Que a chegada foi uma ligação, quem diz é a ponte, pela origem da
/// tentativa — provado contra o ponto de encontro e a sala de verdade em
/// `crates/seele-conformance/tests/ligar_pela_ponte.rs`.
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]
mod a_chegada {
    use super::anotar_a_chegada;
    use crate::Session;
    use seele_ffi::agenda::{Agenda, Direcao};
    use seele_ffi::conhecidos::Conhecidos;
    use seele_ffi::Codigo;

    /// Um servidor visitado antes desta chegada.
    const VISITADO: &str = "casa.exemplo:8383";

    fn agora() -> i64 {
        let desde = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        i64::try_from(desde.as_secs()).unwrap()
    }

    /// A casa depois de uma visita: o servidor na lista, e o alvo da sessão
    /// ainda nele.
    fn depois_de_uma_visita() -> (tempfile::TempDir, String, Session) {
        let casa = tempfile::tempdir().expect("a pasta de teste não abriu");
        let config = casa.path().display().to_string();
        Conhecidos::abrir(casa.path().join("conhecidos"))
            .unwrap()
            .registrar(VISITADO, "eu", None)
            .unwrap();
        let sessao = Session::default();
        *sessao.alvo.lock().unwrap() = Some(VISITADO.to_owned());
        (casa, config, sessao)
    }

    #[test]
    fn a_ligacao_vai_para_a_agenda_e_nao_para_os_visitados() {
        let (casa, config, sessao) = depois_de_uma_visita();
        let alice = Codigo::da_chave(&[1; 32]);
        crate::sala::dar_nome_em(&config, &alice.to_string(), "Alice").unwrap();
        let visitados = std::fs::read(casa.path().join("conhecidos")).unwrap();

        let antes = agora();
        let visita = anotar_a_chegada(
            &sessao,
            &config,
            &seele_ffi::alvo_do_codigo(&alice),
            Some(alice),
        );

        assert!(!visita, "a ligação seguiu para a lista de visitados");
        assert_eq!(
            std::fs::read(casa.path().join("conhecidos")).unwrap(),
            visitados,
            "a ligação mexeu na lista de visitados"
        );
        assert_eq!(
            *sessao.alvo.lock().unwrap(),
            None,
            "a sessão da ligação ficou com o alvo da visita de antes: o caminho medido e a \
             aparência dela iriam para aquele servidor"
        );
        let agenda = Agenda::abrir(crate::toque::caminho_da_agenda(&config)).unwrap();
        let contato = agenda
            .buscar(&alice)
            .expect("a ligação feita não foi para a agenda");
        assert_eq!(
            contato.direcao,
            Some(Direcao::Liguei),
            "a agenda não diz para que lado foi a ligação"
        );
        assert!(
            contato
                .visto_em
                .is_some_and(|quando| (antes..=agora()).contains(&quando)),
            "a agenda não tem a hora da ligação: {:?}",
            contato.visto_em
        );
        assert_eq!(
            contato.nome.as_deref(),
            Some("Alice"),
            "a ligação apagou o nome que esta máquina deu"
        );
    }

    #[test]
    fn o_que_chega_por_um_endereco_e_visita_e_nao_ligacao() {
        let (_casa, config, sessao) = depois_de_uma_visita();
        let outro = "outro.exemplo:8383";

        assert!(
            anotar_a_chegada(&sessao, &config, outro, None),
            "uma entrada por um endereço não seguiu para a lista de visitados"
        );
        assert_eq!(sessao.alvo.lock().unwrap().as_deref(), Some(outro));
        let agenda = Agenda::abrir(crate::toque::caminho_da_agenda(&config)).unwrap();
        assert!(
            agenda.listar().is_empty(),
            "uma entrada por um endereço foi anotada como ligação: {:?}",
            agenda.listar()
        );
    }

    /// **E o `connect` decide com a resposta da ponte.** Um código não se
    /// disca sem um ponto de encontro, e o app não sobe um: o `seele-encontro`
    /// só entra na conformidade (`cargo xtask check-deps`). As duas metades
    /// se provam cada uma onde dá — a origem lá, contra o ponto e a sala de
    /// verdade, e a decisão acima —, e esta é a costura entre elas, lida no
    /// código sem os comentários.
    #[test]
    fn o_connect_decide_a_chegada_pela_origem_que_a_ponte_deu() {
        let fonte = crate::fonte_sem_comentarios();
        let connect: String = crate::corpo_de(&fonte, "async fn connect")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(
            connect.contains("letligou_para=connection.codigo_da_ligacao();")
                && connect.contains("ifligar::anotar_a_chegada(&session,&casa,&alvo,ligou_para){"),
            "o `connect` não decide a chegada pela origem que a ponte deu \
             (`Connection::codigo_da_ligacao`): uma ligação voltaria para a lista de visitados"
        );
    }

    #[test]
    fn um_servidor_desta_maquina_nao_e_visita_nem_herda_o_alvo_de_antes() {
        let (_casa, config, sessao) = depois_de_uma_visita();

        assert!(
            !anotar_a_chegada(&sessao, &config, "127.0.0.1:8383", None),
            "o servidor desta máquina seguiu para a lista de visitados"
        );
        assert_eq!(
            *sessao.alvo.lock().unwrap(),
            None,
            "a sessão no servidor desta máquina ficou com o alvo da visita de antes"
        );
    }
}
