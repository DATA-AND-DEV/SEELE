use super::*;

/// A impressão esperada só existe para um certificado: quem liga para um
/// código espera uma chave, e não uma impressão (spec de 23/09, «Quem liga
/// confere o código dentro do TLS»). O veredito, que lê a impressão, não
/// vê nenhuma num destino de código, e não uma impressão inventada.
#[test]
fn a_impressao_esperada_so_existe_para_um_certificado() {
    let mut destino = destino_de_teste(Some("ab"));
    assert_eq!(destino.impressao_esperada(), Some("ab"));

    destino.esperada = Some(Esperada::Codigo(seele_proto::codigo::Codigo::da_chave(
        &[7; 32],
    )));
    assert_eq!(
        destino.impressao_esperada(),
        None,
        "um destino de código passou a ter uma impressão esperada"
    );

    destino.esperada = None;
    assert_eq!(destino.impressao_esperada(), None);
}

#[test]
fn uma_ligacao_tem_a_bateria_de_um_minuto_e_um_servidor_a_de_cinco() {
    let agora = Duration::from_secs(100);
    let carga = |esperada: Option<Esperada>| {
        let mut destino = destino_de_teste(None);
        destino.esperada = esperada;
        let mut bateria = bateria_de(&destino);
        bateria.on_connection_lost(agora);
        bateria.remaining(agora)
    };

    assert_eq!(
        carga(Some(Esperada::Codigo(
            seele_proto::codigo::Codigo::da_chave(&[7; 32])
        ))),
        Some(crate::battery::CARGA_DA_LIGACAO),
        "uma ligação ganhou a bateria de cinco minutos"
    );
    assert_eq!(
        carga(Some(Esperada::Certificado("ab".into()))),
        Some(seele_proto::transport::SESSION_GRACE)
    );
    assert_eq!(carga(None), Some(seele_proto::transport::SESSION_GRACE));
}

/// A marca do aviso é a de quem se espera que atenda: a do servidor, tirada
/// da impressão, ou a da sala, tirada do código (ADR 0062). Antes da 6c a
/// batida lia a impressão, e um destino de código não tem impressão: uma
/// ligação não avisava ponto nenhum, e o furo do lado de quem recebe nunca
/// abria.
#[test]
fn a_marca_do_aviso_sai_do_que_se_espera() {
    const FP: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";
    let mut destino = destino_de_teste(Some(FP));
    assert_eq!(
        destino.marca_do_aviso(),
        crate::encontro::Marcas::do_servidor(FP).map(|marcas| marcas.aviso().clone())
    );
    assert!(destino.marca_do_aviso().is_some());

    // Uma impressão que não dá marca não bate em ponto nenhum.
    destino.esperada = Some(Esperada::Certificado("curto".into()));
    assert_eq!(destino.marca_do_aviso(), None);

    let codigo = seele_proto::codigo::Codigo::da_chave(&[7; 32]);
    destino.esperada = Some(Esperada::Codigo(codigo));
    assert_eq!(
        destino.marca_do_aviso(),
        Some(crate::encontro::Marcas::da_sala(&codigo).aviso().clone()),
        "um destino de código não avisa com a marca do aviso da sala"
    );

    destino.esperada = None;
    assert_eq!(destino.marca_do_aviso(), None);
}

/// **A recusa local desfaz a entrada que ela recusou, e não a mais antiga
/// no ar** (R1-M4 da revisão do P61).
///
/// O caso do revisor: ela na sala 1; a entrada A (sala 2) no ar; a entrada
/// B (sala 3) com a senha grande demais, recusada aqui. A recusa local ia
/// pela porta do servidor, que recusa o pedido mais antigo no ar — A —, e
/// depois do `Pong` de A (o servidor a pôs na 2) o motor ficava na 3, com a
/// confirmada na 1 e B no ar para sempre.
#[test]
fn a_recusa_local_desfaz_a_entrada_que_ela_recusou() {
    use seele_proto::control::ControlError;
    let grande = ControlError::FieldTooLong {
        field: "password",
        len: 40,
        limit: 32,
    };
    let pong = |numero: u64| ServerMessage::Pong {
        timestamp: crate::assento::carimbo_da_entrada(numero),
    };
    let mut motor = motor_de_teste();
    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(1), None));
    motor.ouvir_o_assento(&pong(1));
    assert_eq!(motor.voice_room, Some(VoiceRoomId(1)));

    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2), None));
    motor.lembrar(&Comando::EntrarNaVoiceRoom(
        VoiceRoomId(3),
        Some("x".repeat(40)),
    ));
    let numero_de_b = motor.entradas.ultimo_numero().expect("B foi pedida");
    motor.recusar_aqui("EntrarNaVoiceRoom", &grande, Some(numero_de_b));
    assert_eq!(
        (
            motor.voice_room,
            motor.entradas.confirmada(motor.voice_room)
        ),
        (Some(VoiceRoomId(2)), Some(VoiceRoomId(1))),
        "a recusa local não devolveu o assento a A, ainda no ar"
    );

    // O servidor decide A: a pôs na 2.
    motor.ouvir_o_assento(&pong(2));
    assert_eq!(
        (
            motor.voice_room,
            motor.entradas.confirmada(motor.voice_room),
            motor.entradas.len()
        ),
        (Some(VoiceRoomId(2)), Some(VoiceRoomId(2)), 0),
        "o motor, o áudio e o servidor discordam depois da recusa local"
    );
}

/// **Um campo grande demais é recusa daquele comando, e não queda** (M2 da
/// revisão do P61).
///
/// A revisão mediu: o apelido, o nome de canal e a senha da sala acima do
/// teto do fio faziam o `encode` falhar, e `executar` chamava `cair()`. A
/// prova de ponta a ponta é `queda_recuperavel::um_campo_grande_demais_nao_derruba_o_enlace`;
/// esta prova as três peças dela: o que é recusa e o que é queda, o que vai
/// ao `seele.log` (o comando e o campo, **nunca o valor**), e a entrada
/// desfeita no motor.
#[test]
fn um_campo_grande_demais_e_recusa_local_e_nao_queda() {
    use seele_proto::control::ControlError;
    let grande = || ControlError::FieldTooLong {
        field: "password",
        len: 52,
        limit: 32,
    };
    assert!(recusa_local(&anyhow::Error::from(grande())).is_some());
    assert!(
        recusa_local(&anyhow::Error::from(grande()).context("escrevendo o quadro")).is_some(),
        "com um contexto em cima, a recusa virou queda"
    );
    assert!(
        recusa_local(&anyhow::anyhow!("o fluxo de controle fechou")).is_none(),
        "um erro de transporte virou recusa: o enlace caído não começaria a bateria"
    );

    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::WARN);
    let _guarda = tracing::subscriber::set_default(rastro.clone());
    let mut motor = motor_de_teste();
    let senha = "segredo-que-nao-vai-ao-log".repeat(2);
    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2), Some(senha)));
    assert_eq!(motor.voice_room, Some(VoiceRoomId(2)));
    let numero = motor.entradas.ultimo_numero();
    motor.recusar_aqui("EntrarNaVoiceRoom", &grande(), numero);

    assert_eq!(
        motor.voice_room, None,
        "a entrada recusada aqui ficou no motor, e a volta a refaria a cada queda"
    );
    let linhas = rastro.linhas();
    assert!(
        linhas
            .iter()
            .any(|linha| linha.contains("EntrarNaVoiceRoom") && linha.contains("password")),
        "o `seele.log` não diz qual comando nem qual campo: {linhas:?}"
    );
    assert!(
        !linhas.iter().any(|linha| linha.contains("segredo")),
        "a senha foi ao `seele.log`: {linhas:?}"
    );
}

/// **O motor acaba a sessão exatamente onde o `Room` acaba** (lote P61,
/// pendência 61).
///
/// O motor tratava `FellBehind` como queda e o `Room` tratava toda
/// `Disconnecting` como fim: a janela mostrava a tela de fim enquanto este
/// motor reconectava por baixo. Os dois passaram a chamar a mesma regra,
/// `seele_sessao::despedida::a_sessao_acabou_aqui`, e é lá que mora o teste
/// exaustivo dela, com a tabela escrita à parte.
///
/// Este é o par do `toda_despedida_acaba_onde_o_motor_acaba`, no `state.rs`,
/// e olha pelo outro lado: o nome que **este** motor chama é conferido
/// contra o `Room`, razão por razão. Uma lista própria escrita aqui, que
/// sombreasse a importada, reprova.
#[test]
fn o_motor_acaba_a_sessao_onde_o_room_acaba() {
    for motivo in [
        DisconnectReason::Incompatible,
        DisconnectReason::CredentialRejected,
        DisconnectReason::HandshakeTimeout,
        DisconnectReason::Kicked,
        DisconnectReason::Banned,
        DisconnectReason::ServerFull,
        DisconnectReason::ScheduledMaintenance,
        DisconnectReason::ServerShuttingDown,
        DisconnectReason::Timeout,
        DisconnectReason::ProtocolViolation,
        DisconnectReason::RateLimited,
        DisconnectReason::FellBehind,
        DisconnectReason::AdmissionPending,
        DisconnectReason::AdmissionDenied,
        DisconnectReason::NicknameTaken,
        DisconnectReason::ModsRecusados,
        DisconnectReason::ModsMudaram,
        DisconnectReason::ModsIndisponiveis,
        DisconnectReason::ChamadaEncerrada,
        DisconnectReason::NinguemAtendeu,
        DisconnectReason::ChamadaRecusada,
        DisconnectReason::ModsDeOutraApi { apis: 1 << 7 },
    ] {
        let mut room = crate::state::Room::new();
        let fim = room
            .apply(&ServerMessage::Disconnecting { reason: motivo })
            .ended;
        assert_eq!(
            a_sessao_acabou_aqui(motivo),
            fim,
            "{motivo:?}: o motor e o `Room` discordam sobre o fim"
        );
    }
}

#[test]
fn so_quem_empresta_publica_os_enderecos_da_propria_maquina() {
    // **O guarda do achado de privacidade do fix round 3.** O round 2
    // passou a publicar os endereços de interface de *todo* cliente,
    // inclusive de quem não empresta — contra o §5 da spec, que nomeia
    // privacidade como a primeira das duas razões do opt-in.
    //
    // **E consentir em assistir por par não basta**, que é a metade nova:
    // aquele consentimento decide se o endereço **público** desta máquina
    // pode ser entregue a quem a serve, e não se a topologia interna dela
    // é publicada. Destrancar a segunda com a primeira daria a quem
    // consentiu no menor o custo do maior — ver o doc desta função.
    //
    // Os endereços são inventados de propósito: a decisão não pode
    // depender de quais interfaces a máquina que roda o teste tem, senão
    // o guarda passa a valer só onde há uma LAN.
    let da_maquina = || vec![SocketAddr::from(([192, 168, 1, 10], 4444))];
    for calado in [
        ConsentimentoDePar::de_ninguem(),
        ConsentimentoDePar {
            pares_que_atende: 0,
            assiste_por_par: true,
        },
    ] {
        assert!(
            locais_a_publicar(calado, da_maquina).is_empty(),
            "quem não empresta ({calado:?}) publicou a topologia de rede interna da máquina"
        );
    }
    assert_eq!(
        locais_a_publicar(
            ConsentimentoDePar {
                pares_que_atende: 1,
                assiste_por_par: false,
            },
            da_maquina
        ),
        da_maquina(),
        "quem optou por emprestar deixou de publicar por onde ser alcançado"
    );
}

/// Uma máquina com muitas interfaces não pode montar uma declaração que o
/// servidor recusa: o teto do fio é cortado aqui, antes de mandar.
#[test]
fn os_locais_publicados_cabem_no_teto_do_fio() {
    let muitos = || {
        (1..=40u8)
            .map(|n| SocketAddr::from(([192, 168, 1, n], 4444)))
            .collect::<Vec<_>>()
    };
    let publicados = locais_a_publicar(
        ConsentimentoDePar {
            pares_que_atende: 1,
            assiste_por_par: false,
        },
        muitos,
    );
    assert_eq!(publicados.len(), seele_proto::control::MAX_LOCAIS_DE_PAR);
    assert_eq!(publicados[0], muitos()[0]);
}

#[test]
fn enderecos_locais_excluem_loopback_nao_especificado_e_link_local() {
    // A parte que `locais_de_pares` não pode errar: publicar loopback ou
    // um não-especificado não ajuda ninguém a discar, e link-local não
    // sai do cabo — mesmo motivo que
    // `seele-server::alcance::interfaces::descobrir` já documenta para o
    // convite.
    let locais = [IpAddr::from([192, 168, 1, 10]), IpAddr::from([10, 0, 0, 5])];
    for ip in locais {
        assert!(e_endereco_de_rede_local(ip), "{ip} devia contar como local");
    }
    let excluidos = [
        IpAddr::from([127, 0, 0, 1]),
        IpAddr::from([0, 0, 0, 0]),
        IpAddr::from([169, 254, 1, 1]),
        IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
        IpAddr::V6(std::net::Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
    ];
    for ip in excluidos {
        assert!(
            !e_endereco_de_rede_local(ip),
            "{ip} não devia contar como local"
        );
    }
}

#[test]
fn insistir_contra_recusa_nao_muda_a_resposta() {
    // A diferença entre reconectar e martelar. Uma chave trocada é o alerta
    // do ADR 0003 e tentar de novo só o repetiria a cada backoff.
    assert!(!vale_insistir(&ConnectError::PinChanged {
        pinned: "a".into(),
        offered: "b".into(),
    }));
    // Um convite que não confere também não melhora com repetição: seria o
    // mesmo link errado contra o mesmo servidor a cada backoff.
    assert!(!vale_insistir(&ConnectError::InviteMismatch {
        expected: "a".into(),
        offered: "b".into(),
    }));
    assert!(vale_insistir(&ConnectError::Unreachable));
    assert!(vale_insistir(&ConnectError::HandshakeTimeout));
}

#[test]
fn quem_nao_prova_a_chave_respondeu_e_insistir_nao_muda_a_resposta() {
    // Era `TlsRefused`, que a bateria repete: insistir mandaria um aperto
    // de mão a quem já não provou ter a chave, a cada batida, por cinco
    // minutos. E é resposta de alguém, como a impressão que não confere:
    // perder a corrida para o «não alcancei» de um endereço morto
    // esconderia o impostor atrás de uma falha de rede.
    assert!(
        alguem_respondeu(&ConnectError::ChaveNaoProvada),
        "quem mostrou o certificado sem a chave perde a corrida para um endereço \
             morto, e a tela diz «não alcancei» sobre um impostor"
    );
    assert!(
        !vale_insistir(&ConnectError::ChaveNaoProvada),
        "a bateria insiste contra quem não provou a chave do servidor, e a volta \
             martela o impostor em vez de acabar"
    );
}

/// Um conjunto de MODs a aceitar não é uma queda de rede.
///
/// # As duas metades, e por que elas estavam faltando
///
/// [`ConnectError::ModsNaoAceitos`] entrou na enumeração e ficou de fora
/// das duas decisões que a usam. Sem a primeira, a pergunta com a lista
/// perde a corrida de `conectar_entre` para o «não alcancei» de um endereço
/// morto — que falha primeiro porque falhar é rápido — e a casca fica sem o
/// que mostrar. Sem a segunda, a bateria interna reconecta por cinco
/// minutos contra uma decisão que só uma pessoa pode tomar, mandando
/// `RecusarMods` a cada batida.
///
/// É a metade simétrica do que `a_sessao_acabou_aqui` já dizia para
/// `DisconnectReason::ModsMudaram`, e agora as duas estão escritas.
#[test]
fn um_conjunto_de_mods_a_aceitar_nao_e_falha_de_rede() {
    let pergunta = || ConnectError::ModsNaoAceitos {
        mods: Vec::new(),
        conjunto: "9f86d0".into(),
        plataformas: None,
    };

    assert!(
        alguem_respondeu(&pergunta()),
        "a lista de MODs veio de um servidor que respondeu o aperto de mão inteiro"
    );
    assert!(
        !vale_insistir(&pergunta()),
        "a reconexão automática encontra o mesmo anúncio e o mesmo não"
    );
}

/// **E a pergunta sobrevive à corrida.** O guarda de comportamento do
/// achado acima: não basta o predicado dizer «respondeu», é a escolha da
/// falha que precisa mudar.
///
/// A ordem aqui é a do caso típico e não uma conveniência: o endereço morto
/// é anotado **primeiro**, porque um destino inalcançável falha em
/// milissegundos enquanto o anúncio exige o aperto de mão inteiro.
/// Revertendo qualquer uma das duas metades do conserto, este teste vê
/// `Unreachable` — que é a casca dizendo «não alcancei» sobre um servidor
/// que estava ali, com a lista na mão.
#[test]
fn a_pergunta_dos_mods_ganha_do_endereco_morto_que_falhou_antes() {
    let mut a_mostrar = FalhaAMostrar::default();
    a_mostrar.anotar(ConnectError::Unreachable);
    a_mostrar.anotar(ConnectError::ModsNaoAceitos {
        mods: Vec::new(),
        conjunto: "9f86d0".into(),
        plataformas: None,
    });

    assert!(
        matches!(a_mostrar.escolhida(), ConnectError::ModsNaoAceitos { .. }),
        "a pergunta com a lista de MODs foi descartada em favor de um «não alcancei»"
    );
}

/// Sem ninguém que tenha respondido, a primeira falha é a que sobra — e sem
/// falha nenhuma sobra a verdade. As duas outras pernas de
/// [`FalhaAMostrar`], para que o teste acima não seja o único a passar por
/// aqui.
#[test]
fn sem_ninguem_que_respondeu_sobra_a_primeira_falha() {
    let mut a_mostrar = FalhaAMostrar::default();
    a_mostrar.anotar(ConnectError::SemResposta);
    a_mostrar.anotar(ConnectError::Unreachable);
    assert!(matches!(a_mostrar.escolhida(), ConnectError::SemResposta));

    assert!(matches!(
        FalhaAMostrar::default().escolhida(),
        ConnectError::Unreachable
    ));
}

#[test]
fn uma_recusa_desfaz_o_pin_que_o_verificador_acabou_de_escrever() {
    // Sem isto a recusa é decorativa: a visita seguinte, sem link para
    // conferir, veria `Matches` e entraria no servidor recém-rejeitado.
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    let decisao = PinDecision::FirstContact {
        fingerprint: "aaaa1111".into(),
    };
    let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

    aplicar_veredito(&veredito, &loja, "casa");

    assert_eq!(loja.pinned("casa"), None, "a recusa deixou o pin para trás");
}

/// **Um primeiro contato pelo código é conferido, e não cego.** O
/// `ConfereCodigo` provou dentro do TLS que a chave é a do código, como a
/// impressão de um link prova um certificado. Dizer `FirstContact` faria a
/// casca perguntar «confia nesta chave?» a quem acabou de prová-la, e na
/// própria sala de quem recebe (o ATENDER, tarefa 27 da F7).
#[test]
fn um_primeiro_contato_pelo_codigo_e_conferido_e_nao_cego() {
    let mut destino = destino_de_teste(None);
    destino.esperada = Some(Esperada::Codigo(seele_proto::codigo::Codigo::da_chave(
        &[7; 32],
    )));
    let loja = crate::tofu::MemoryPinStore::new();

    let primeiro = conferir(
        &destino,
        &PinDecision::FirstContact {
            fingerprint: "aaaa1111".into(),
        },
        &loja,
    );
    assert!(
        matches!(
            &primeiro,
            Ok(Verdict::FirstContactVerified { fingerprint }) if fingerprint == "aaaa1111"
        ),
        "o primeiro contato pelo código saiu {primeiro:?}, e não conferido"
    );

    let depois = conferir(
        &destino,
        &PinDecision::Matches {
            fingerprint: "aaaa1111".into(),
        },
        &loja,
    );
    assert!(matches!(depois, Ok(Verdict::Known)), "{depois:?}");
}

#[test]
fn um_veredito_que_nao_recusa_deixa_o_pin_onde_esta() {
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    let decisao = PinDecision::Matches {
        fingerprint: "aaaa1111".into(),
    };
    let veredito = crate::tofu::verdict(&decisao, Some("bbbb2222"));

    aplicar_veredito(&veredito, &loja, "casa");

    assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
}

/// Um destino de teste onde a chave do pin e o nome TLS são **diferentes**.
///
/// Diferentes de propósito: confundir os dois já custou caro a este projeto
/// uma vez — dois servidores numa LAN dividindo a entrada `localhost`, e o
/// segundo parecendo o primeiro com a chave trocada (`tofu.rs`). Um teste
/// em que os dois valores são iguais não pega essa troca.
fn destino_de_teste(impressao_esperada: Option<&str>) -> Destino {
    Destino {
        servidor: "127.0.0.1:1".parse().expect("endereço"),
        nome_tls: "localhost".into(),
        chave_do_pin: "casa".into(),
        apelido: "pessoa".into(),
        segredo: None,
        esperada: impressao_esperada.map(|fp| Esperada::Certificado(fp.to_owned())),
        o_pino_prova_o_servidor: true,
        aceito: None,
    }
}

#[test]
fn um_convite_que_nao_confere_derruba_a_conexao_e_desfaz_o_pin() {
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());
    let decisao = PinDecision::FirstContact {
        fingerprint: "aaaa1111".into(),
    };

    let erro = conferir(&destino_de_teste(Some("bbbb2222")), &decisao, &loja)
        .expect_err("um convite que não confere tinha que derrubar a conexão");

    // Quem prometeu é o link, quem ofereceu é o servidor. Trocar os dois
    // faria a casca acusar o lado errado.
    assert_eq!(
        erro,
        ConnectError::InviteMismatch {
            expected: "bbbb2222".into(),
            offered: "aaaa1111".into(),
        }
    );
    // Sob a **chave do pin**, não sob o nome TLS.
    assert_eq!(loja.pinned("casa"), None, "a recusa deixou o pin para trás");
}

#[test]
fn um_convite_que_confere_deixa_seguir_e_diz_que_foi_conferido() {
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());
    let decisao = PinDecision::FirstContact {
        fingerprint: "aaaa1111".into(),
    };

    let veredito = conferir(&destino_de_teste(Some("aaaa1111")), &decisao, &loja)
        .expect("o convite confere; não havia o que recusar");

    assert_eq!(
        veredito,
        Verdict::FirstContactVerified {
            fingerprint: "aaaa1111".into()
        }
    );
    assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
}

#[test]
fn sem_convite_o_primeiro_contato_segue_cego_como_sempre_foi() {
    // Quem digitou o endereço à mão não tem o que conferir, e recusar aqui
    // trancaria para fora todo mundo que não veio de um link.
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());
    let decisao = PinDecision::FirstContact {
        fingerprint: "aaaa1111".into(),
    };

    let veredito = conferir(&destino_de_teste(None), &decisao, &loja)
        .expect("sem convite não há o que recusar");

    assert_eq!(
        veredito,
        Verdict::FirstContact {
            fingerprint: "aaaa1111".into()
        }
    );
    assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
}

#[test]
fn um_convite_velho_contra_um_pin_que_bate_avisa_e_nao_derruba() {
    // A metade oposta da recusa, e a que some sem ninguém notar: com pin
    // estabelecido, o TOFU já provou que este é o servidor de ontem, então
    // quem está errado é o link. Derrubar aqui trancaria a pessoa para fora
    // de um servidor que ela usa porque um amigo mandou um link velho.
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());
    let decisao = PinDecision::Matches {
        fingerprint: "aaaa1111".into(),
    };

    let veredito = conferir(&destino_de_teste(Some("bbbb2222")), &decisao, &loja)
        .expect("um link velho não derruba um servidor já conhecido");

    assert_eq!(
        veredito,
        Verdict::InviteDisagrees {
            expected: "bbbb2222".into(),
            offered: "aaaa1111".into(),
        }
    );
    // Esta é a asserção que segura a política: sem ela o teste passa mesmo
    // se o aviso virar recusa, porque o veredito continuaria o mesmo e só o
    // efeito mudaria.
    assert_eq!(
        loja.pinned("casa"),
        Some("aaaa1111".into()),
        "o aviso desfez o pin, e a próxima visita entraria cega"
    );
}

#[test]
fn um_convite_que_concorda_com_o_pin_nao_tem_nada_a_dizer() {
    // Completa a tabela: pin bate, link concorda, nada acontece.
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());
    let decisao = PinDecision::Matches {
        fingerprint: "aaaa1111".into(),
    };

    let veredito = conferir(&destino_de_teste(Some("aaaa1111")), &decisao, &loja)
        .expect("não havia nada de errado para recusar");

    assert_eq!(veredito, Verdict::Known);
    assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
}

#[test]
fn um_aperto_de_mao_que_falhou_nao_deixa_o_pin_que_o_tls_escreveu() {
    // `desfazer_pin_orfao`, a limpeza de depois da corrida: o candidato cujo
    // prazo estourou foi derrubado no meio do aperto de mão, depois de o
    // verificador aceitar a chave e fixá-la, e não teve chance de se limpar
    // (a limpeza do próprio aperto, `desfazer_o_pin_deste_aperto`, vai com
    // ele). Sem nada fixado antes da corrida (`None`), o pin que está lá é
    // órfão e cai. O que sobrasse faria a visita seguinte ver `Matches`, e aí,
    // num alvo de escopo público, um convite que **não** confere viraria
    // `InviteDisagrees` — de recusar para avisar, sem ninguém decidir isso.
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    desfazer_pin_orfao(&loja, "casa", None, &ConnectError::SemResposta);

    assert_eq!(loja.pinned("casa"), None);
}

#[test]
fn um_pin_que_ja_existia_sobrevive_a_um_aperto_que_falhou() {
    // Só o que este aperto escreveu é órfão. Apagar um pin antigo porque a
    // rede caiu jogaria fora a memória de que o ADR 0003 depende.
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    desfazer_pin_orfao(&loja, "casa", Some("aaaa1111"), &ConnectError::SemResposta);

    assert_eq!(loja.pinned("casa"), Some("aaaa1111".into()));
}

#[test]
fn o_pin_que_este_aperto_escreveu_e_desfeito_quando_ele_falha() {
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    desfazer_o_pin_deste_aperto(&loja, "casa", Some("aaaa1111"), &ConnectError::SemResposta);

    assert_eq!(
        loja.pinned("casa"),
        None,
        "o pin que o aperto que falhou escreveu ficou na loja, e a visita seguinte, \
             sem link, entraria calada no servidor recusado"
    );
}

#[test]
fn o_que_um_aperto_nao_escreveu_nao_e_dele_para_desfazer() {
    // A recusa pela impressão, a chave trocada e toda falha de um aperto
    // que não chegou a aceitar um primeiro contato não escrevem nada. Um
    // pin que exista na hora é de um vizinho da corrida, e desfazê-lo é a
    // confiança de primeiro contato do ADR 0003 sumindo calada.
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    desfazer_o_pin_deste_aperto(&loja, "casa", None, &ConnectError::SemResposta);

    assert_eq!(
        loja.pinned("casa"),
        Some("aaaa1111".into()),
        "um aperto que não escreveu nada apagou o pin de outro"
    );
}

#[test]
fn um_pin_que_o_vizinho_trocou_depois_fica_com_o_vizinho() {
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "bbbb2222".into());

    desfazer_o_pin_deste_aperto(&loja, "casa", Some("aaaa1111"), &ConnectError::SemResposta);

    assert_eq!(
        loja.pinned("casa"),
        Some("bbbb2222".into()),
        "o aperto que falhou apagou um pin que já não era o que ele tinha escrito"
    );
}

#[test]
fn o_pin_que_um_aperto_desfaz_vai_ao_log_com_a_chave_e_o_motivo() {
    // «Por que meu pino sumiu» não tinha resposta: as duas limpezas apagavam
    // calado. `info`, que é o nível que o `seele.log` grava.
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
    let _guarda = tracing::subscriber::set_default(rastro.clone());
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    desfazer_o_pin_deste_aperto(
        &loja,
        "casa",
        Some("aaaa1111"),
        &ConnectError::HandshakeTimeout,
    );

    let linhas = rastro.linhas();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("INFO")
            && linha.contains("casa")
            && linha.contains("aaaa1111")
            && linha.contains("HandshakeTimeout")),
        "o pin que o aperto que falhou tinha fixado foi apagado sem dizer no log qual \
             chave, qual impressão e por quê. Rastro: {linhas:?}"
    );
}

#[test]
fn o_pin_orfao_desfeito_depois_da_corrida_vai_ao_log_com_a_chave_e_o_motivo() {
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
    let _guarda = tracing::subscriber::set_default(rastro.clone());
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    desfazer_pin_orfao(&loja, "casa", None, &ConnectError::SemResposta);

    let linhas = rastro.linhas();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("INFO")
            && linha.contains("casa")
            && linha.contains("aaaa1111")
            && linha.contains("SemResposta")),
        "o pin órfão de um candidato que perdeu a corrida foi apagado sem dizer no log \
             qual chave, qual impressão e por quê. Rastro: {linhas:?}"
    );
}

#[test]
fn a_limpeza_que_nao_apaga_nada_nao_escreve_nada() {
    // A linha é de quando um pino some. Uma por candidato que não apagou nada
    // enterraria as que importam.
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
    let _guarda = tracing::subscriber::set_default(rastro.clone());
    let loja = crate::tofu::MemoryPinStore::new();
    loja.pin("casa", "aaaa1111".into());

    desfazer_o_pin_deste_aperto(&loja, "casa", None, &ConnectError::SemResposta);
    desfazer_o_pin_deste_aperto(&loja, "casa", Some("bbbb2222"), &ConnectError::SemResposta);
    desfazer_pin_orfao(&loja, "casa", Some("aaaa1111"), &ConnectError::SemResposta);

    assert_eq!(
        loja.pinned("casa"),
        Some("aaaa1111".into()),
        "uma limpeza que não devia apagar nada apagou, e o teste mediu outra coisa"
    );
    let linhas = rastro.linhas();
    assert!(
        linhas.is_empty(),
        "uma limpeza que não apagou pino nenhum escreveu no log: {linhas:?}"
    );
}
#[tokio::test]
async fn o_pin_orfao_de_um_candidato_que_estourou_o_prazo_vai_ao_log_com_a_falha_dele() {
    // A limpeza de depois da corrida, pelo caminho de verdade. Um candidato
    // termina o TLS (o verificador fixa a chave), manda o `Hello` e não
    // ouve nada: o prazo do candidato estoura, a tentativa é derrubada no
    // meio, e o pin fica órfão. A linha que o apaga tem de dizer a falha
    // que este candidato deu (`SemResposta`, o prazo), e não uma escolhida
    // pelo caminho.
    //
    // Desde a 0.17 o prazo é o do **lote**, e só existe quando há corrida:
    // aqui o degrau 4 é o segundo lote, por um ponto de encontro mudo. O
    // lote direto disca, fixa a chave, manda o `Hello` e não ouve nada até
    // o prazo dele (o da chamada mais o da discagem, 8 s).
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
    let _guarda = tracing::subscriber::set_default(rastro.clone());

    let (escuta, onde_de_quem_segura, impressao_de_quem_segura) = ponta_com_certificado_proprio();
    let segura = tokio::spawn(async move {
        let conexao = escuta.aceitar().await;
        std::future::pending::<()>().await;
        drop(conexao);
    });
    let chave = "chave-de-quem-segura";
    let quem_nao_sai = destino_na_chave(
        "127.0.0.1:9".parse().expect("endereço"),
        "chave-de-quem-nao-sai",
        None,
    );
    let loja = Arc::new(crate::tofu::MemoryPinStore::new());
    let ponto_mudo = std::net::UdpSocket::bind("127.0.0.1:0").expect("ponto mudo");
    let bilhete = seele_proto::uri::Bilhete::novo(
        ponto_mudo.local_addr().expect("endereço").to_string(),
        "198.51.100.4:41000",
    )
    .expect("bilhete de teste");

    let resultado = tokio::time::timeout(
        Duration::from_secs(20),
        Enlace::conectar_entre_com_bilhete(
            vec![
                destino_na_chave(onde_de_quem_segura, chave, Some(&impressao_de_quem_segura)),
                quem_nao_sai,
            ],
            Some(bilhete),
            SigningKey::from_bytes(&[9; 32]),
            Arc::clone(&loja) as Arc<dyn PinStore>,
        ),
    )
    .await
    .expect("a corrida não terminou");
    segura.abort();

    assert!(
        resultado.is_err(),
        "um candidato entrou numa corrida sem servidor que respondesse, e o teste mediu \
             outra coisa"
    );
    assert_eq!(
        loja.pinned(chave),
        None,
        "o pin órfão do candidato que estourou o prazo ficou na loja depois da corrida"
    );
    let linhas = rastro.linhas();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("INFO")
            && linha.contains(chave)
            && linha.contains(&impressao_de_quem_segura)
            && linha.contains("SemResposta")),
        "o pin órfão foi apagado sem dizer no log a falha do candidato que o deixou \
             (`SemResposta`, o prazo): a falha que a corrida passa à limpeza não é a dele. \
             Rastro: {linhas:?}"
    );
}

#[test]
fn a_loja_do_aperto_repassa_tudo_e_anota_so_o_que_ele_escreve() {
    let de_verdade = Arc::new(crate::tofu::MemoryPinStore::new());
    de_verdade.pin("velha", "cccc3333".into());
    let deste_aperto = PinsDesteAperto::nova(Arc::clone(&de_verdade) as Arc<dyn PinStore>);

    assert_eq!(
        deste_aperto.pinned("velha"),
        Some("cccc3333".into()),
        "a loja do aperto não repassou a leitura: o verificador veria um pin \
             que não existe, ou nenhum que existe"
    );
    assert_eq!(
        deste_aperto.o_que_escreveu(),
        None,
        "a loja do aperto anotou como escrito um pin que já estava lá"
    );

    deste_aperto.pin("casa", "aaaa1111".into());

    assert_eq!(
        de_verdade.pinned("casa"),
        Some("aaaa1111".into()),
        "a loja do aperto não repassou a escrita: o pin fixado no TLS se perdia"
    );
    assert_eq!(
        deste_aperto.o_que_escreveu(),
        Some("aaaa1111".into()),
        "a loja do aperto não anotou o que o TLS escreveu, e a limpeza não \
             teria o que desfazer"
    );

    deste_aperto.unpin("casa");
    assert_eq!(
        de_verdade.pinned("casa"),
        None,
        "a loja do aperto não repassou o desfazer"
    );
}

#[test]
fn o_que_a_reconexao_restaura_e_o_que_a_pessoa_escolheu() {
    let mut motor = motor_de_teste();

    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2), None));
    motor.lembrar(&Comando::AbrirLinha(ChannelId(7)));
    motor.lembrar(&Comando::Muted(true));
    motor.lembrar(&Comando::Isolamento(true));

    assert_eq!(motor.voice_room, Some(VoiceRoomId(2)));
    assert_eq!(motor.linha, Some(ChannelId(7)));
    assert!(motor.muted);
    assert!(motor.isolamento);

    // Ejetar não é uma queda: quem saiu da sala de voz não volta para ele.
    motor.lembrar(&Comando::SairDaVoiceRoom);
    assert_eq!(motor.voice_room, None);
}

/// A sala que a volta refaz segue o que o servidor disse, pelos mesmos
/// caminhos que o assento da casca segue em `Room::apply`.
///
/// Desde a pendência 52 isto deixou de ser só o roster: a casca senta a
/// pessoa, na volta, na sala que `Aviso::Reconectado` diz que o motor refez,
/// e o áudio abre por esse assento. Um motor que guardasse a sala apagada,
/// ou a que recusou a entrada, a refaria na volta — e a casca abriria o
/// microfone numa sala que não existe, ou que acabou de dizer não, até a
/// recusa chegar.
#[test]
fn a_voice_room_que_a_volta_refaz_segue_o_que_o_servidor_disse() {
    let alerta = |reason| ServerMessage::Alert {
        severity: seele_proto::control::AlertSeverity::Warning,
        reason,
        operator_text: None,
    };
    let mut motor = motor_de_teste();

    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2), None));
    motor.ouvir_o_assento(&entrada_decidida());
    motor.ouvir_o_assento(&ServerMessage::VoiceRoomDeleted {
        voice_room: VoiceRoomId(9),
    });
    assert_eq!(
        motor.voice_room,
        Some(VoiceRoomId(2)),
        "apagar outra sala tirou esta pessoa da dela"
    );
    motor.ouvir_o_assento(&ServerMessage::VoiceRoomDeleted {
        voice_room: VoiceRoomId(2),
    });
    assert_eq!(
        motor.voice_room, None,
        "a volta refaria a entrada numa sala que foi apagada"
    );

    for recusa in [
        seele_proto::control::AlertReason::VoiceRoomEntryRefused,
        seele_proto::control::AlertReason::VoiceRoomFull,
    ] {
        motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(3), None));
        motor.ouvir_o_assento(&alerta(recusa));
        // E o `Pong` do `Ping` marcado, que o servidor manda depois de
        // decidir cada pedido.
        motor.ouvir_o_assento(&entrada_decidida());
        assert_eq!(
            motor.voice_room, None,
            "a volta refaria a entrada que o servidor recusou ({recusa:?})"
        );
    }

    motor.ouvir_o_assento(&ServerMessage::MovedToVoiceRoom {
        voice_room: VoiceRoomId(5),
    });
    assert_eq!(motor.voice_room, Some(VoiceRoomId(5)));
}

/// O `Pong` do `Ping` marcado que segue cada pedido de entrada.
fn entrada_decidida() -> ServerMessage {
    ServerMessage::Pong {
        // O `Pong` de um pedido mais novo que todos os destes testes: fecha
        // o que está no ar, como o `Pong` do pedido que acabou de sair.
        timestamp: crate::assento::carimbo_da_entrada(u64::from(u32::MAX)),
    }
}

/// M3 no motor: a volta com o mesmo `ssrc`, numa sessão de número maior, é
/// o servidor devolvendo o assento, e a recusa da entrada que `restabelecer`
/// refaz sem a senha devolve a pessoa a ele. Com `ssrc` novo não havia
/// assento a devolver, e com o mesmo `ssrc` numa sessão de número igual o
/// servidor subiu de novo (N1 da rodada 2): também não.
///
/// O servidor de teste devolve sempre o `ssrc` 1 na sessão 1.
#[tokio::test(flavor = "multi_thread")]
async fn a_volta_com_o_assento_resgatado_nao_o_perde_para_a_reentrada_recusada() {
    let (endereco, impressao, _hellos) = servidor_que_conta_os_hellos();
    let recusa = ServerMessage::Alert {
        severity: seele_proto::control::AlertSeverity::Warning,
        reason: seele_proto::control::AlertReason::VoiceRoomEntryRefused,
        operator_text: None,
    };

    for (ssrc_da_queda, sessao_da_queda, depois) in [
        (
            seele_proto::ids::Ssrc(1),
            seele_proto::ids::SessionId(0),
            Some(VoiceRoomId(5)),
        ),
        (
            seele_proto::ids::Ssrc(9),
            seele_proto::ids::SessionId(0),
            None,
        ),
        (
            seele_proto::ids::Ssrc(1),
            seele_proto::ids::SessionId(1),
            None,
        ),
    ] {
        let (mut motor, _) = motor_que_volta_a(endereco, &impressao, true);
        let (avisos, _avisos_rx) = mpsc::unbounded_channel();
        motor.avisos = avisos;
        motor.voice_room = Some(VoiceRoomId(5));
        motor.queda = Some(Queda {
            ssrc: ssrc_da_queda,
            sessao: sessao_da_queda,
            confirmada: Some(VoiceRoomId(5)),
        });

        motor.tentar().await;
        assert!(motor.cliente.is_some(), "a volta não reconectou");
        assert_eq!(
            motor.voice_room,
            Some(VoiceRoomId(5)),
            "a volta não refez a sala"
        );
        motor.ouvir_o_assento(&recusa);
        motor.ouvir_o_assento(&entrada_decidida());
        assert_eq!(
                motor.voice_room, depois,
                "com o `ssrc` {ssrc_da_queda:?} e a sessão {sessao_da_queda:?} da queda, a recusa da entrada refeita deixou a \
                 volta em {:?}",
                motor.voice_room
            );
    }
}

/// I1 da rodada 1 do lote I-a: o braço de `Alert` do motor esquece só o
/// pedido recusado, e não a sala em que o servidor manteve a pessoa.
///
/// Esquecer a sala a cada recusa fazia a volta de uma queda sair sem sala
/// — e a casca, que segue o assento do motor, fechar o áudio — de quem o
/// servidor ainda tinha sentado: a troca de sala recusada, a volta numa
/// sala com senha, o apelido tomado.
#[test]
fn uma_recusa_esquece_so_o_pedido_e_nao_a_sala_que_a_volta_refaz() {
    let recusa = ServerMessage::Alert {
        severity: seele_proto::control::AlertSeverity::Warning,
        reason: seele_proto::control::AlertReason::VoiceRoomEntryRefused,
        operator_text: None,
    };
    let mut motor = motor_de_teste();
    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2), None));
    motor.ouvir_o_assento(&entrada_decidida());

    // Uma recusa sem pedido no ar: o apelido tomado.
    motor.ouvir_o_assento(&recusa);
    assert_eq!(
        motor.voice_room,
        Some(VoiceRoomId(2)),
        "uma recusa sem pedido de entrada no ar fez a volta esquecer a sala em que o \
             servidor manteve a pessoa"
    );

    // A troca recusada: volta à sala de antes, e não a lugar nenhum.
    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(3), None));
    motor.ouvir_o_assento(&recusa);
    motor.ouvir_o_assento(&entrada_decidida());
    assert_eq!(
        motor.voice_room,
        Some(VoiceRoomId(2)),
        "a troca de sala recusada fez a volta esquecer a sala de antes, onde o servidor \
             manteve a pessoa"
    );
}

/// Um destino guardado em `servidor`, com uma impressão de verdade: é dela
/// que sai a marca do aviso.
fn guardado_em(servidor: &str) -> Destino {
    Destino {
        servidor: servidor.parse().expect("endereço"),
        nome_tls: "localhost".into(),
        chave_do_pin: servidor.to_owned(),
        apelido: "pessoa".into(),
        segredo: None,
        esperada: Some(Esperada::Certificado(
            "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9".to_owned(),
        )),
        o_pino_prova_o_servidor: false,
        aceito: None,
    }
}

#[tokio::test]
async fn quem_esta_na_mesma_casa_comeca_sem_esperar_o_quarto() {
    // A pendência 55: com o ponto ou o DNS mudos, quem estava na mesma casa
    // pagava o prazo inteiro do quarto antes de tentar o endereço de casa.
    // Aqui o quarto nunca responde, e a tentativa de casa tem de começar na
    // hora.
    let ponto = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("o loopback não abriu");
    let guardado = seele_proto::uri::Bilhete::novo(
        ponto.local_addr().expect("endereço").to_string(),
        "198.51.100.4:41000",
    )
    .expect("bilhete de teste");
    let (_entrega, quarto) = tokio::sync::oneshot::channel::<DoQuarto>();
    let (olhos, mut tentativas) = mpsc::unbounded_channel();

    let tentativa = tokio::spawn(Enlace::conectar_entre_com_o_quarto(
        vec![
            guardado_em("127.0.0.1:9"),
            guardado_em("[::ffff:127.0.0.1]:9"),
        ],
        Some(guardado),
        SigningKey::from_bytes(&[7; 32]),
        Arc::new(crate::tofu::MemoryPinStore::new()),
        olhos,
        Some(quarto),
    ));
    let primeira = tokio::time::timeout(Duration::from_millis(500), tentativas.recv()).await;
    // E quem precisa de furo continua esperando o quarto, que não veio.
    let segunda = tokio::time::timeout(Duration::from_millis(500), tentativas.recv()).await;
    tentativa.abort();

    let Ok(Some(super::Andamento::Tentando(primeira))) = primeira else {
        panic!("a tentativa de casa não começou em meio segundo: esperou o quarto");
    };
    assert_eq!(primeira.onde.to_string(), "127.0.0.1:9");
    assert!(
        segunda.is_err(),
        "quem precisa de furo começou sem a resposta do quarto: {segunda:?}"
    );
}

/// Um ponto de encontro de mentira que responde todo `CHAMA` com `AQUI`
/// para `endereco`, e conta quantos chegaram.
fn ponto_que_atende_o_chama(
    endereco: SocketAddr,
) -> (seele_proto::uri::Bilhete, Arc<std::sync::atomic::AtomicU32>) {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("ponto");
    socket.set_nonblocking(true).expect("não bloqueante");
    let onde = socket.local_addr().expect("endereço");
    let chamados = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let contando = Arc::clone(&chamados);
    tokio::spawn(async move {
        let socket = tokio::net::UdpSocket::from_std(socket).expect("tokio");
        let mut balde = [0_u8; seele_proto::encontro::TAMANHO3 + 1];
        while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
            let Some(seele_proto::encontro::Pedido3::Chama(chama)) =
                seele_proto::encontro::analisar3(balde.get(..lidos).unwrap_or_default())
            else {
                continue;
            };
            contando.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let resposta =
                seele_proto::encontro::resposta3(&seele_proto::encontro::Resposta3::Aqui {
                    marca: chama.servidor,
                    endereco,
                    dicas: Vec::new(),
                });
            let _ = socket.send_to(&resposta, de).await;
        }
    });
    let bilhete =
        seele_proto::uri::Bilhete::novo(onde.to_string(), "198.51.100.4:41000").expect("bilhete");
    (bilhete, chamados)
}

#[tokio::test(flavor = "multi_thread")]
async fn quando_o_direto_cala_o_chama_do_ponto_da_o_caminho() {
    // O degrau 4 da 0.17 pelo caminho de produção: o endereço do convite
    // não responde, o `CHAMA` sai pela ponta da sessão, o ponto devolve
    // onde o anfitrião atende, e a discagem vai para lá.
    let (onde_de_verdade, impressao, hellos) = servidor_que_conta_os_hellos();
    let (bilhete, chamados) = ponto_que_atende_o_chama(onde_de_verdade);
    let morto = std::net::UdpSocket::bind("127.0.0.1:0").expect("morto");
    let mut destino = destino_na_chave(
        morto.local_addr().expect("endereço"),
        "chave-da-volta",
        Some(&impressao),
    );
    destino.o_pino_prova_o_servidor = false;
    let (olhos, mut andamentos) = mpsc::unbounded_channel();

    let entrou = tokio::time::timeout(
        Duration::from_secs(20),
        Enlace::conectar_entre_observado(
            vec![destino],
            Some(bilhete),
            SigningKey::from_bytes(&[9; 32]),
            Arc::new(crate::tofu::MemoryPinStore::new()),
            olhos,
        ),
    )
    .await
    .expect("a chegada pelo ponto não terminou");
    assert!(
        entrou.is_ok(),
        "o CHAMA não deu caminho: {:?}",
        entrou.err()
    );
    assert!(chamados.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    assert_eq!(hellos.load(std::sync::atomic::Ordering::SeqCst), 1);
    let mut pelo_ponto = false;
    while let Ok(andamento) = andamentos.try_recv() {
        if let Andamento::Tentando(tentativa) = andamento {
            pelo_ponto |= tentativa.avisou && tentativa.onde == onde_de_verdade;
        }
    }
    assert!(
        pelo_ponto,
        "a trilha não diz que o endereço que entrou veio do ponto de encontro"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn o_ponto_atende_e_o_caminho_nao_abre_e_e_nat_que_nao_abriu() {
    // O `AQUI` chegou e o ICE para o endereço dele não voltou: é a falha
    // que o degrau 4 tem de nomear, e não «não alcancei».
    let mudo_do_anfitriao = std::net::UdpSocket::bind("127.0.0.1:0").expect("mudo");
    let (bilhete, _) = ponto_que_atende_o_chama(mudo_do_anfitriao.local_addr().expect("endereço"));
    let morto = std::net::UdpSocket::bind("127.0.0.1:0").expect("morto");
    let destino = destino_na_chave(
        morto.local_addr().expect("endereço"),
        "chave-que-nao-abre",
        Some(&"c".repeat(64)),
    );
    let resultado = tokio::time::timeout(
        Duration::from_secs(25),
        Enlace::conectar_entre_com_bilhete(
            vec![destino],
            Some(bilhete),
            SigningKey::from_bytes(&[9; 32]),
            Arc::new(crate::tofu::MemoryPinStore::new()),
        ),
    )
    .await
    .expect("a chegada não terminou");
    assert_eq!(resultado.err(), Some(ConnectError::NatNaoAbriu));
}

#[test]
fn o_endereco_de_hoje_entra_logo_depois_do_alvo_que_espera() {
    // A posição que ele tinha quando o `connect` do app o punha na frente
    // dos alternativos: logo depois do alvo. Desde a 0.17 os que esperavam
    // o quarto e o endereço de hoje vão num lote só, um agente ICE.
    let pins = crate::tofu::MemoryPinStore::new();
    let candidato = |onde: &str| Candidato {
        destino: guardado_em(onde),
        antes: None,
    };
    let de_hoje = |onde: &str| DoQuarto {
        candidato: Some(guardado_em(onde)),
    };
    let ordem = |lotes: Vec<Lote>| -> Vec<String> {
        assert_eq!(lotes.len(), 1, "o quarto soltou mais de um lote");
        lotes
            .first()
            .map(|lote| {
                lote.candidatos()
                    .iter()
                    .map(|candidato| candidato.destino.servidor.to_string())
                    .collect()
            })
            .unwrap_or_default()
    };

    let alvo_espera = lote_do_quarto(
        de_hoje("203.0.113.9:8383"),
        vec![candidato("203.0.113.1:8383"), candidato("203.0.113.2:8383")],
        true,
        &pins,
    );
    assert_eq!(
        ordem(alvo_espera),
        ["203.0.113.1:8383", "203.0.113.9:8383", "203.0.113.2:8383"]
    );

    let alvo_ja_corre = lote_do_quarto(
        de_hoje("203.0.113.9:8383"),
        vec![candidato("203.0.113.2:8383")],
        false,
        &pins,
    );
    assert_eq!(
        ordem(alvo_ja_corre),
        ["203.0.113.9:8383", "203.0.113.2:8383"]
    );
    assert!(
        lote_do_quarto(DoQuarto::default(), Vec::new(), false, &pins).is_empty(),
        "um quarto que não trouxe nada soltou um lote vazio na corrida"
    );
}

#[test]
fn a_segunda_chamada_disca_o_que_o_ponto_deu_e_os_ipv6_do_convite() {
    // O `AQUI` só diz o endereço que o ponto viu e as dicas de rede local.
    // O anfitrião furou para o IPv6 de quem chama, e é ao IPv6 dele, que o
    // convite já trazia, que a discagem da segunda chamada tem de ir.
    let candidato = |onde: &str, prova: bool| Candidato {
        destino: Destino {
            o_pino_prova_o_servidor: prova,
            ..guardado_em(onde)
        },
        antes: Some(format!("pino de {onde}")),
    };
    let modelo = candidato("203.0.113.1:8383", true);
    let atendida = crate::encontro::Atendida {
        endereco: "198.51.100.7:14178".parse().expect("endereço"),
        dicas: vec!["192.168.0.9:8383".parse().expect("endereço")],
        credencial: "u".repeat(32),
    };
    let do_convite_v6 = [
        candidato("[2804:7f0:b0c0:1234::10]:8383", false),
        candidato("[2804:7f0:b0c0:1234::20]:8383", false),
    ];
    let onde = |destinos: &[Candidato]| -> Vec<String> {
        destinos
            .iter()
            .map(|candidato| candidato.destino.servidor.to_string())
            .collect()
    };

    // A primeira chamada é a de sempre: só o que o ponto deu.
    assert_eq!(
        onde(&destinos_da_chamada(&modelo, &atendida)),
        ["198.51.100.7:14178", "192.168.0.9:8383"],
        "a discagem da primeira chamada mudou: ela é o caminho IPv4 de sempre"
    );

    let destinos = destinos_da_segunda_chamada(&modelo, &atendida, &do_convite_v6);
    assert_eq!(
        onde(&destinos),
        [
            "198.51.100.7:14178",
            "192.168.0.9:8383",
            "[2804:7f0:b0c0:1234::10]:8383",
            "[2804:7f0:b0c0:1234::20]:8383",
        ],
        "a discagem da segunda chamada não foi aos IPv6 do anfitrião: o furo IPv6 abriria \
             e ninguém passaria por ele"
    );
    // Os do `AQUI` herdam as regras do alvo, sem a prova pelo pino: são
    // endereços que ninguém escolheu.
    for do_aqui in destinos.iter().take(2) {
        assert_eq!(do_aqui.destino.chave_do_pin, modelo.destino.chave_do_pin);
        assert!(!do_aqui.destino.o_pino_prova_o_servidor);
    }
    // Os do convite ficam como o convite os trouxe: a chave de pino e o
    // pin de antes deles, e não os do alvo.
    for (do_convite, original) in destinos.iter().skip(2).zip(&do_convite_v6) {
        assert_eq!(
            do_convite.destino.chave_do_pin,
            original.destino.chave_do_pin
        );
        assert_eq!(do_convite.antes, original.antes);
    }

    // Um `AQUI` que já é o IPv6 do convite (um anfitrião registrado no
    // ponto por IPv6) não o põe duas vezes no mesmo agente ICE.
    let por_ipv6 = crate::encontro::Atendida {
        endereco: "[2804:7f0:b0c0:1234::10]:8383".parse().expect("endereço"),
        dicas: Vec::new(),
        credencial: "u".repeat(32),
    };
    assert_eq!(
        destinos_da_segunda_chamada(&modelo, &por_ipv6, &do_convite_v6).len(),
        2
    );
    // Quem liga por código não tem IPv6 nenhum do anfitrião: disca o que o
    // ponto deu, e aprende o IPv6 dele da checagem que chega.
    assert_eq!(
        destinos_da_segunda_chamada(&modelo, &atendida, &[]).len(),
        2
    );

    // O que conta como IPv6 do convite: IPv6 de verdade, e nunca o IPv4
    // escrito como IPv6.
    for (endereco, e) in [
        ("[2804:7f0:b0c0:1234::10]:8383", true),
        ("[fd12:3456::10]:8383", true),
        ("[::1]:8383", true),
        ("203.0.113.7:8383", false),
        ("[::ffff:203.0.113.7]:8383", false),
    ] {
        assert_eq!(
            e_ipv6(endereco.parse().expect("endereço")),
            e,
            "errou sobre {endereco}"
        );
    }

    // E a segunda chamada cabe no prazo do lote: sem o dobro, ela nasceria
    // com o prazo já gasto pela discagem da primeira.
    assert_eq!(prazo_do_lote(true), prazo_do_lote(false) * 2);
    assert_eq!(
        prazo_do_lote(false),
        crate::encontro::PRAZO_DA_CHAMADA + PRAZO_DA_DISCAGEM_NA_CORRIDA
    );
}

#[test]
fn a_volta_disca_de_novo_o_ipv6_da_sessao_depois_do_furo() {
    let atendida = crate::encontro::Atendida {
        endereco: "198.51.100.7:14178".parse().expect("endereço"),
        dicas: vec!["192.168.0.9:8383".parse().expect("endereço")],
        credencial: "u".repeat(32),
    };
    let texto =
        |alvos: Vec<SocketAddr>| -> Vec<String> { alvos.iter().map(ToString::to_string).collect() };
    // A sessão estava no IPv6 do anfitrião: a discagem direta a ele deu em
    // silêncio, o anfitrião furou, e a discagem seguinte volta a ele.
    assert_eq!(
        texto(alvos_da_segunda_volta(
            "[2804:7f0:b0c0:1234::10]:8383".parse().expect("endereço"),
            &atendida
        )),
        [
            "198.51.100.7:14178",
            "192.168.0.9:8383",
            "[2804:7f0:b0c0:1234::10]:8383"
        ],
        "a volta não discou de novo o IPv6 que o furo acabou de abrir"
    );
    // A sessão estava num IPv4: o `AQUI` já diz o de hoje, e nada muda.
    assert_eq!(
        texto(alvos_da_segunda_volta(
            "203.0.113.1:8383".parse().expect("endereço"),
            &atendida
        )),
        ["198.51.100.7:14178", "192.168.0.9:8383"]
    );
}

/// Uma ponta anfitriã de teste (ICE completo controlado, DTLS passivo) com certificado
/// próprio, em `127.0.0.1`, e a impressão dele.
///
/// Só fecha o aperto do enlace (DTLS, `ctl` e a apresentação vazia de um
/// `seeled`): o que acontece com a conexão depois é de quem chama, por
/// `Ponta::aceitar`.
fn ponta_com_certificado_proprio() -> (crate::transporte::Ponta, SocketAddr, String) {
    let certificado = crate::transporte::CertificadoDtls::efemero().expect("certificado");
    let impressao = crate::client::hex(&certificado.impressao());
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("socket");
    let ponta = crate::transporte::Ponta::de_socket(
        socket,
        Some(crate::transporte::ConfigDoAnfitriao::novo(certificado)),
    )
    .expect("ponta de teste");
    let endereco = ponta.local_addr();
    (ponta, endereco, impressao)
}

/// Discar `destino` e fazer o aperto do protocolo, como a entrada de um
/// endereço só faz, sem declarar identidade de par.
async fn conectar_por_teste(
    destino: Destino,
    chave: SigningKey,
    pins: Arc<dyn PinStore>,
) -> Result<Enlace, ConnectError> {
    let ponta = crate::transporte::abrir_ponta(None)?;
    let conexao = crate::client::discar(
        ponta.ponta(),
        vec![destino.servidor],
        None,
        seele_proto::transport::HANDSHAKE_TIMEOUT,
    )
    .await?;
    Enlace::conectar_sobre(ponta, conexao, None, destino, chave, pins, None).await
}

/// Uma loja de pins que finge o vizinho de uma corrida.
///
/// Desarmada, é uma loja comum. Armada, a **primeira** leitura devolve o que
/// havia e, logo depois de olhar, grava `vizinho` na chave: o instante entre
/// o verificador de um candidato decidir e ele desistir, em que o outro
/// candidato fixa a chave dele. Existe porque esse instante dura
/// microssegundos, e um teste que dependesse de o pegar por acaso não
/// provaria nada.
#[derive(Debug)]
struct LojaComVizinho {
    dentro: crate::tofu::MemoryPinStore,
    armada: std::sync::atomic::AtomicBool,
    vizinho: std::sync::Mutex<Option<(String, String)>>,
}

impl LojaComVizinho {
    fn nova(chave: &str, impressao_do_vizinho: &str) -> Self {
        Self {
            dentro: crate::tofu::MemoryPinStore::new(),
            armada: std::sync::atomic::AtomicBool::new(false),
            vizinho: std::sync::Mutex::new(Some((
                chave.to_owned(),
                impressao_do_vizinho.to_owned(),
            ))),
        }
    }

    fn armar(&self) {
        self.armada.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl PinStore for LojaComVizinho {
    fn pinned(&self, host: &str) -> Option<String> {
        let visto = self.dentro.pinned(host);
        if self.armada.swap(false, std::sync::atomic::Ordering::SeqCst) {
            if let Some((chave, impressao)) = self.vizinho.lock().expect("vizinho").take() {
                self.dentro.pin(&chave, impressao);
            }
        }
        visto
    }

    fn pin(&self, host: &str, fingerprint: String) {
        self.dentro.pin(host, fingerprint);
    }

    fn unpin(&self, host: &str) {
        self.dentro.unpin(host);
    }
}

/// Destino de teste: `nome_tls` de sempre, a chave de pino que o teste
/// escolhe, e a impressão esperada que ele quiser.
fn destino_na_chave(onde: SocketAddr, chave: &str, esperada: Option<&str>) -> Destino {
    Destino {
        servidor: onde,
        nome_tls: "localhost".into(),
        chave_do_pin: chave.to_owned(),
        apelido: "pessoa".into(),
        segredo: None,
        esperada: esperada.map(|fp| Esperada::Certificado(fp.to_owned())),
        o_pino_prova_o_servidor: true,
        aceito: None,
    }
}

#[tokio::test]
async fn a_recusa_pela_impressao_nao_apaga_o_pin_que_o_vizinho_fixa_logo_depois() {
    // O pino órfão que a corrida do ADR 0037 não protegia. Dois candidatos
    // de um convite com a mesma chave de pino (`chave_do_pin` é `host:porta`
    // do nome, e alternativos do mesmo nome colidem): o verdadeiro fixa a
    // chave dele, e o outro (um impostor, ou a porta velha de um NAT) é
    // recusado pela impressão. O verificador do impostor olhou e não viu pin
    // nenhum; o vizinho fixa; o impostor desiste. A limpeza de
    // `conectar_por` achava `nada antes, algo agora` e apagava o pin que o
    // vizinho acabou de escrever — sem o guarda do vencedor que a limpeza de
    // depois da corrida tem, e sem dizer nada.
    let (ponta_b, onde_b, impressao_b) = ponta_com_certificado_proprio();
    let impressao_de_a = "a".repeat(64);
    let chave = "casa:8383";
    let loja = Arc::new(LojaComVizinho::nova(chave, &impressao_de_a));

    // Quem atende fecha o aperto do enlace e fica de pé. A loja é armada
    // antes: a primeira leitura é a do verificador, depois do DTLS e antes
    // do `Hello`, e é ela que dispara o vizinho.
    let segura_b = tokio::spawn(async move {
        let conexao = ponta_b.aceitar().await;
        std::future::pending::<()>().await;
        drop(conexao);
    });
    loja.armar();

    let resultado = tokio::time::timeout(
        Duration::from_secs(20),
        conectar_por_teste(
            destino_na_chave(onde_b, chave, Some(&impressao_de_a)),
            SigningKey::from_bytes(&[8; 32]),
            Arc::clone(&loja) as Arc<dyn PinStore>,
        ),
    )
    .await
    .expect("a tentativa recusada não terminou");
    segura_b.abort();

    let Err(erro) = resultado else {
        panic!("a tentativa que a impressão desmente entrou");
    };
    assert_eq!(
        erro,
        ConnectError::InviteMismatch {
            expected: impressao_de_a.clone(),
            offered: impressao_b
        },
        "o candidato que a impressão desmente não foi recusado por ela, e o \
             teste mediu outra coisa"
    );
    assert_eq!(
        loja.pinned(chave),
        Some(impressao_de_a),
        "a recusa do impostor apagou o pin que o candidato verdadeiro tinha \
             acabado de fixar: a confiança de primeiro contato foi desfeita calada"
    );
}

#[tokio::test]
async fn um_candidato_de_outra_chave_nao_apaga_o_pin_que_o_vizinho_fixou() {
    // O mesmo desenho com servidores de verdade, e a ordem que sobra quando
    // o verdadeiro é o primeiro a fixar: o outro candidato encontra o pin do
    // vizinho, vê outra chave e falha com `PinChanged`. Também não escreveu
    // nada, e também não pode desfazer o que era do vizinho — era assim que
    // o pin do candidato verdadeiro sumia mesmo antes de a impressão chegar
    // ao TLS.
    //
    // **A**, o verdadeiro, termina o TLS, fixa a chave dele e nunca
    // responde ao `Hello`: fica de pé no meio do aperto de mão. **B** tem
    // outro certificado, e o teste o segura no meio do TLS até **A** ter
    // fixado, sem depender de relógio:
    //
    // 1. **B** começa, e não vê pin nenhum;
    // 2. **A** fixa a chave dele;
    // 3. **B** falha, sem ter escrito nada.
    let (ponta_a, onde_a, impressao_a) = ponta_com_certificado_proprio();
    let (ponta_b, onde_b, impressao_b) = ponta_com_certificado_proprio();
    let chave = "casa:8383";
    let loja = Arc::new(crate::tofu::MemoryPinStore::new());

    let segura_a = tokio::spawn(async move {
        let conexao = ponta_a.aceitar().await;
        std::future::pending::<()>().await;
        drop(conexao);
    });
    let segura_b = tokio::spawn(async move {
        let conexao = ponta_b.aceitar().await;
        std::future::pending::<()>().await;
        drop(conexao);
    });

    let a = conectar_por_teste(
        destino_na_chave(onde_a, chave, Some(&impressao_a)),
        SigningKey::from_bytes(&[7; 32]),
        Arc::clone(&loja) as Arc<dyn PinStore>,
    );
    // **B** só disca depois de **A** fixar: a ordem é a do texto acima, sem
    // relógio.
    let b = async {
        while loja.pinned(chave).as_deref() != Some(impressao_a.as_str()) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        conectar_por_teste(
            destino_na_chave(onde_b, chave, None),
            SigningKey::from_bytes(&[8; 32]),
            Arc::clone(&loja) as Arc<dyn PinStore>,
        )
        .await
    };
    let resultado_de_b = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::select! {
            biased;
            _ = a => panic!(
                "a tentativa que devia ficar parada no `Hello` terminou: o \
                 servidor de teste respondeu, ou o aperto de mão falhou"
            ),
            resultado = b => resultado,
        }
    })
    .await
    .expect("a disputa entre as duas tentativas não terminou");
    segura_a.abort();
    segura_b.abort();

    let Err(erro) = resultado_de_b else {
        panic!("a tentativa de outra chave entrou");
    };
    assert_eq!(
        erro,
        ConnectError::PinChanged {
            pinned: impressao_a.clone(),
            offered: impressao_b
        },
        "o candidato de outra chave não falhou como chave trocada, e o teste \
             mediu outra coisa"
    );
    assert_eq!(
        loja.pinned(chave),
        Some(impressao_a),
        "um candidato que falhou sem escrever nada apagou o pin do vizinho"
    );
}

/// Um servidor que fala só o aperto de mão do protocolo e derruba a
/// conexão assim que ele termina — antes que quem está do outro lado
/// consiga escrever mais nada por ela.
///
/// Existe para reproduzir o §2.2 do relatório sem depender de uma queda de
/// rede de verdade acontecer bem no instante certo — isso seria uma
/// corrida contra o relógio, e uma corrida não prova a mesma coisa duas
/// vezes seguidas. Aqui não há corrida: a conexão morre porque **este
/// servidor a derruba**, no mesmo fôlego em que confirma o aperto de mão
/// como bom. `Client::connect_por` sai com sucesso; a escrita seguinte,
/// na mesma conexão, é a que encontra a queda.
async fn servidor_que_aperta_a_mao_e_cai() -> SocketAddr {
    // O TLS é o de qualquer ponta de teste. O que este servidor tem de seu
    // é o que faz depois dele, e a impressão não interessa a quem o chama.
    let (escuta, endereco, _) = ponta_com_certificado_proprio();

    tokio::spawn(async move {
        let Some(conexao) = escuta.aceitar().await else {
            return;
        };
        let Some((mut envio, mut recebe)) = conexao.controle() else {
            return;
        };

        // `Hello`, ignorado: este servidor de teste aceita qualquer um.
        if crate::frame::read::<seele_proto::control::ClientMessage>(&mut recebe)
            .await
            .is_err()
        {
            return;
        }
        if crate::frame::write(
            &mut envio,
            &seele_proto::control::ServerMessage::Challenge { nonce: vec![0; 32] },
        )
        .await
        .is_err()
        {
            return;
        }
        // `Response`, também ignorado: não há assinatura para conferir
        // aqui, só o aperto de mão precisa completar.
        if crate::frame::read::<seele_proto::control::ClientMessage>(&mut recebe)
            .await
            .is_err()
        {
            return;
        }
        // A parada sai **antes** do `Session`, e não depois — de propósito.
        // `stop_sending` e o quadro do `Session` seguem os dois na mesma
        // direção (deste servidor para o cliente), então chegam juntos ou
        // quase juntos, e o processamento das duas mensagens acontece na
        // mesma passada de leitura da conexão do cliente, antes que o
        // código dele sequer acorde do `await` que lê o `Session`. Fechar
        // a conexão inteira **depois** do `Session` sair é uma corrida —
        // ou o fecho alcança o `Session` no meio do caminho, ou não
        // alcança a próxima escrita do cliente a tempo, dependendo de
        // quanto a rede demorar. `stop_sending` na direção que o cliente
        // escreve não tem essa corrida: uma vez que o cliente processa o
        // pacote, a próxima escrita dele volta erro na hora, sem depender
        // de outro round-trip.
        let _ = recebe.stop(0);
        let _ = crate::frame::write(
            &mut envio,
            &seele_proto::control::ServerMessage::Session {
                id: seele_proto::ids::SessionId(1),
                person: PersonId(1),
                ssrc: seele_proto::ids::Ssrc(1),
                server: "servidor de teste".into(),
                voice_rooms: vec![],
                channels: vec![],
                roles: vec![],
                permissions: vec![],
            },
        )
        .await;
        // A conexão fica viva de propósito — é o `stop_sending` acima que
        // derruba só a direção que importa. Vazam porque o teste é curto e
        // derrubar os endpoints fecharia a conexão que ele ainda mede.
        std::mem::forget(escuta);
        std::mem::forget(conexao);
    });

    endereco
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reentrada_na_sala_falha_na_reconexao_e_o_cliente_nao_se_da_por_sentado() {
    // O mecanismo do §2.2 do relatório: o servidor confirma por silêncio,
    // então uma falha de envio que ninguém confere vira divergência
    // permanente. É a terceira via para o mesmo sintoma relatado — «pra
    // mim ele não está na call, pra ele sim» — e aqui é o espelho: pra
    // esta pessoa ela está, e o servidor nunca recebeu o pedido.
    let endereco = servidor_que_aperta_a_mao_e_cai().await;

    let mut motor = motor_de_teste();
    let (avisos, mut avisos_rx) = mpsc::unbounded_channel();
    motor.avisos = avisos;
    motor.destino.servidor = endereco;
    motor.destino.nome_tls = "localhost".into();
    motor.destino.chave_do_pin = endereco.to_string();
    // A sala que a pessoa tinha escolhido antes de cair — o que
    // `Motor::tentar` tenta restaurar nesta reconexão, e o que a queda
    // desta conexão vai impedir de restaurar.
    motor.voice_room = Some(VoiceRoomId(9));

    motor.tentar().await;

    assert!(
        motor.cliente.is_none(),
        "a reentrada na sala falhou, mas o motor se deu por reconectado do mesmo jeito \
             — exatamente o defeito: o cliente se acha dentro da sala, e o servidor nunca \
             recebeu o pedido."
    );

    while let Ok(aviso) = avisos_rx.try_recv() {
        assert!(
            !matches!(aviso, Aviso::Reconectado { .. }),
            "o motor anunciou reconexão mesmo sem ter restaurado a sala de voz — é isto \
                 que faz a casca se dar por dentro da sala para si mesma enquanto o servidor \
                 nunca a teve."
        );
    }

    // A intenção continua guardada: a próxima tentativa da bateria tenta
    // de novo a mesma sala, e não esquece o que a pessoa escolheu.
    assert_eq!(motor.voice_room, Some(VoiceRoomId(9)));
}

/// Uma ponta com certificado próprio que atende o aperto de mão do protocolo
/// inteiro e fica de pé, contando os `Hello` que chegaram até ela.
///
/// Devolve o endereço, a impressão do certificado e a contagem. É o lado do
/// servidor que os testes da volta da bateria olham: um `Hello` que chega
/// aqui levou o apelido e a assinatura de quem reconectava.
fn servidor_que_conta_os_hellos() -> (SocketAddr, String, Arc<std::sync::atomic::AtomicU32>) {
    servidor_que_conta_os_hellos_depois_de(Duration::ZERO)
}

/// O mesmo, com o socket aberto na hora e a ponta só começando a ler
/// depois de `atraso`: as checagens ICE que chegam antes esperam no socket,
/// e quem responde primeiro é qualquer outro candidato do mesmo lote.
fn servidor_que_conta_os_hellos_depois_de(
    atraso: Duration,
) -> (SocketAddr, String, Arc<std::sync::atomic::AtomicU32>) {
    let certificado = crate::transporte::CertificadoDtls::efemero().expect("certificado");
    let impressao = crate::client::hex(&certificado.impressao());
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("socket");
    let endereco = socket.local_addr().expect("endereço");
    let hellos = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let contando = Arc::clone(&hellos);
    tokio::spawn(async move {
        tokio::time::sleep(atraso).await;
        let Ok(escuta) = crate::transporte::Ponta::de_socket(
            socket,
            Some(crate::transporte::ConfigDoAnfitriao::novo(certificado)),
        ) else {
            return;
        };
        // As conexões ficam guardadas aqui, de pé, até o teste acabar: a
        // reconexão tem de encontrar a sessão viva depois do `Session`.
        let mut de_pe = Vec::new();
        while let Some(conexao) = escuta.aceitar().await {
            let Some((mut envio, mut recebe)) = conexao.controle() else {
                continue;
            };
            if crate::frame::read::<seele_proto::control::ClientMessage>(&mut recebe)
                .await
                .is_err()
            {
                continue;
            }
            contando.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if crate::frame::write(
                &mut envio,
                &seele_proto::control::ServerMessage::Challenge { nonce: vec![0; 32] },
            )
            .await
            .is_err()
            {
                continue;
            }
            // `Response`, ignorado: não há assinatura a conferir aqui.
            if crate::frame::read::<seele_proto::control::ClientMessage>(&mut recebe)
                .await
                .is_err()
            {
                continue;
            }
            let _ = crate::frame::write(
                &mut envio,
                &seele_proto::control::ServerMessage::Session {
                    id: seele_proto::ids::SessionId(1),
                    person: PersonId(1),
                    ssrc: seele_proto::ids::Ssrc(1),
                    server: "servidor de teste".into(),
                    voice_rooms: vec![],
                    channels: vec![],
                    roles: vec![],
                    permissions: vec![],
                },
            )
            .await;
            de_pe.push((conexao, envio, recebe));
        }
    });
    (endereco, impressao, hellos)
}

/// Um motor que volta pela bateria a `endereco`, com a chave de quem atende
/// ali já fixada e uma impressão esperada que a desmente.
fn motor_que_volta_a(
    endereco: SocketAddr,
    fixada: &str,
    o_pino_prova_o_servidor: bool,
) -> (Motor, Arc<crate::tofu::MemoryPinStore>) {
    let loja = Arc::new(crate::tofu::MemoryPinStore::new());
    loja.pin(&endereco.to_string(), fixada.to_owned());
    let mut motor = motor_de_teste();
    motor.pins = Arc::clone(&loja) as Arc<dyn PinStore>;
    motor.destino.servidor = endereco;
    motor.destino.nome_tls = "localhost".into();
    motor.destino.chave_do_pin = endereco.to_string();
    motor.destino.esperada = Some(Esperada::Certificado("ab".repeat(32)));
    motor.destino.o_pino_prova_o_servidor = o_pino_prova_o_servidor;
    (motor, loja)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_volta_da_bateria_onde_o_pino_nao_prova_o_servidor_e_recusada_antes_do_hello() {
    // `Motor::tentar` não passa por `conferir`: a conferência da volta é só
    // a do verificador, e ele só sabe se o pino prova o servidor pelo que
    // `tentar` lhe passa do destino. Aqui o destino é o de um candidato que
    // a pessoa não escolheu, ou de um alvo de LAN: quem atende está fixado
    // com a chave dele, e a esperada é de outro servidor. A volta tem de ser
    // recusada dentro do TLS, como a entrada foi.
    let (endereco, impressao, hellos) = servidor_que_conta_os_hellos();
    let (mut motor, loja) = motor_que_volta_a(endereco, &impressao, false);
    let (avisos, mut avisos_rx) = mpsc::unbounded_channel();
    motor.avisos = avisos;

    motor.tentar().await;

    assert_eq!(
        hellos.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a volta da bateria mandou o `Hello` a quem o pino não prova: o apelido e a \
             assinatura saíram para um servidor que a impressão esperada desmente"
    );
    assert!(
        motor.cliente.is_none(),
        "a volta da bateria se deu por reconectada num endereço cujo pino não prova o \
             servidor, com a impressão esperada discordando"
    );
    let mut recusada = false;
    while let Ok(aviso) = avisos_rx.try_recv() {
        recusada |= matches!(
            aviso,
            Aviso::Encerrado(Motivo::Recusado(ConnectError::InviteMismatch { .. }))
        );
    }
    assert!(
        recusada,
        "a volta recusada pela impressão não encerrou a sessão como recusa: a bateria \
             insistiria contra um servidor que não é o do link"
    );
    assert_eq!(
        loja.pinned(&endereco.to_string()),
        Some(impressao),
        "a recusa da volta mexeu no pino daquele endereço, que continua sendo a prova de \
             quem atende ali"
    );
}

#[tokio::test]
async fn a_volta_recusada_diz_no_registro_por_que_a_sessao_acabou() {
    // A volta recusada acabava com a sessão sem uma linha no `seele.log`:
    // só a tentativa que falha e deixa a bateria correndo tinha o `debug!`
    // dela. A que acaba com tudo saía calada, e quem relatava «caiu e não
    // voltou» não tinha o que mostrar.
    //
    // `#[tokio::test]` de thread única: `set_default` fixa o rastro nesta
    // thread, e `tentar` roda nela.
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::WARN);
    let _guarda = tracing::subscriber::set_default(rastro.clone());

    let (endereco, impressao, _) = servidor_que_conta_os_hellos();
    let (mut motor, _) = motor_que_volta_a(endereco, &impressao, false);
    let (avisos, mut avisos_rx) = mpsc::unbounded_channel();
    motor.avisos = avisos;

    motor.tentar().await;

    let mut recusada = false;
    while let Ok(aviso) = avisos_rx.try_recv() {
        recusada |= matches!(aviso, Aviso::Encerrado(Motivo::Recusado(_)));
    }
    assert!(
        recusada,
        "a volta pela impressão que não confere não acabou recusada, e o teste mediu \
             outra coisa"
    );
    let linhas = rastro.linhas();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("WARN")
            && linha.contains("a volta da bateria foi recusada")
            && linha.contains("InviteMismatch")),
        "a volta recusada acabou com a sessão sem dizer no `seele.log` qual recusa foi: \
             quem relata «caiu e não voltou» não tem o que mostrar. Linhas: {linhas:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_volta_da_bateria_onde_o_pino_prova_o_servidor_reconecta_com_a_esperada_divergente() {
    // A metade que a recusa de cima não pode levar junto (ADR 0003). No alvo
    // de escopo público, ou no loopback, o pino prova o servidor: a volta
    // passa com a esperada discordando, como a entrada passou e avisou.
    let (endereco, impressao, hellos) = servidor_que_conta_os_hellos();
    let (mut motor, _) = motor_que_volta_a(endereco, &impressao, true);
    let (avisos, mut avisos_rx) = mpsc::unbounded_channel();
    motor.avisos = avisos;

    motor.tentar().await;

    assert!(
        motor.cliente.is_some(),
        "a volta da bateria ao alvo cujo pino prova o servidor foi recusada pela impressão \
             esperada: quem entrou avisado seria derrubado na primeira queda de rede"
    );
    let mut reconectou = false;
    while let Ok(aviso) = avisos_rx.try_recv() {
        reconectou |= matches!(aviso, Aviso::Reconectado { .. });
    }
    assert!(
        reconectou,
        "o motor voltou sem anunciar a reconexão à casca"
    );
    assert_eq!(
        hellos.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a volta que reconectou não passou por um aperto de mão do protocolo com o \
             servidor de teste, e o teste mediu outra coisa"
    );
}

/// O aviso da volta diz em que sala de voz o motor sentou a pessoa de novo.
///
/// A pendência 52 abre o áudio pelo assento, e a casca não tem outro jeito de
/// saber dele depois de uma queda: a sessão nova chega sem assento, e o
/// servidor confirma a entrada por silêncio. Quem estava fora de sala volta
/// com `None`, e a casca não abre aparelho nenhum.
///
/// # A colisão de porta, em toda rodada
///
/// Este teste reprovava uma vez em muitas, com `None` contra
/// `Some(Some(VoiceRoomId(5)))` depois de 20,0 s em `tentar` (lote Y da F5).
/// Medido: o socket `[::]:0` de quem volta caía numa porta que um socket
/// IPv4 em `127.0.0.1` já segurava — numa rodada sem nada de fora, o que
/// `servidor_que_aperta_a_mao_e_cai` esquece de pé de propósito, no teste
/// anterior —, a resposta do servidor ao aperto de mão ia para ele, e o QUIC
/// desistia pela ociosidade (`TimedOut`). O servidor de teste ficava no
/// `entrada.await` daquela conexão.
///
/// A primeira volta encena isso de propósito onde o sistema deixa (o
/// macOS): a proposta de socket que o sistema daria colide com um socket
/// IPv4 que o teste segura. Sem a conferência de
/// [`crate::client::socket_de_quem_conecta`], a volta reprova sempre, com a
/// mensagem de antes. Onde o sistema recusa a colisão, a volta corre com os
/// sockets IPv4 de pé, e a recusa fica medida em
/// `client::testes_do_socket`.
#[tokio::test(flavor = "multi_thread")]
async fn a_volta_avisa_a_voice_room_em_que_o_motor_sentou_a_pessoa_de_novo() {
    let (endereco, impressao, _hellos) = servidor_que_conta_os_hellos();

    for sala in [Some(VoiceRoomId(5)), None] {
        let (mut motor, _) = motor_que_volta_a(endereco, &impressao, true);
        let (avisos, mut avisos_rx) = mpsc::unbounded_channel();
        motor.avisos = avisos;
        motor.voice_room = sala;

        // Os ladrões ficam de pé até a volta acabar: é um deles que recebe a
        // resposta do servidor se o socket de quem volta dividir a porta com
        // ele. Onde o sistema recusa a colisão, a volta corre com eles de pé,
        // e não há proposta a encenar.
        let (_ladroes, encenada) = if sala.is_some() {
            use crate::client::testes_do_socket::{encenar_a_colisao, Encenacao};
            match encenar_a_colisao() {
                Encenacao::Colisao { ladrao, proposta } => {
                    crate::client::PROPOSTA_DE_TESTE
                        .with(|vaga| *vaga.borrow_mut() = Some(Ok(proposta)));
                    (vec![ladrao], true)
                }
                Encenacao::Recusa { ladroes, .. } => (ladroes, false),
            }
        } else {
            (Vec::new(), false)
        };

        // `tentar` roda nesta thread — é o futuro do `block_on` do teste —, e
        // é nela que a proposta guardada vale.
        motor.tentar().await;
        if encenada {
            assert!(
                crate::client::PROPOSTA_DE_TESTE.with(|vaga| vaga.borrow().is_none()),
                "a volta não abriu o socket pela proposta guardada, e o teste não \
                     encenou a colisão"
            );
        }

        let mut avisada = None;
        while let Ok(aviso) = avisos_rx.try_recv() {
            if let Aviso::Reconectado { voice_room, .. } = aviso {
                avisada = Some(voice_room);
            }
        }
        assert_eq!(
            avisada,
            Some(sala),
            "a volta não avisou a sala que o motor refez nela: a casca desenha fora \
                 de sala quem está sentado, e o áudio que segue o assento fecha a voz \
                 de quem só caiu e voltou"
        );
    }
}

/// Uma ponta com certificado próprio que aceita todo aperto de mão do TLS e
/// não fala nada depois: o bastante para o verificador de quem conecta ver
/// a chave dela e decidir.
fn ponta_que_so_mostra_a_chave() -> (SocketAddr, String) {
    let (escuta, endereco, impressao) = ponta_com_certificado_proprio();
    tokio::spawn(async move {
        let mut de_pe = Vec::new();
        while let Some(conexao) = escuta.aceitar().await {
            de_pe.push(conexao);
        }
    });
    (endereco, impressao)
}

#[tokio::test]
async fn a_recusa_de_um_candidato_perdedor_vai_ao_log_mesmo_quando_outro_entra() {
    // O evento que o Plano 1B existe para barrar é um impostor num endereço
    // da corrida (o do quarto, por exemplo) com a chave errada. Quando outro
    // candidato vencia, as falhas da corrida eram descartadas sem uma linha,
    // e esse evento não deixava rastro no `seele.log`.
    //
    // Desde a 0.17 os quatro vão num agente ICE só, e o ICE fica com quem
    // responde primeiro. O verdadeiro começa a ler o socket um segundo
    // depois, de propósito, para os outros dois responderem antes:
    // - um que a impressão esperada desmente (`InviteMismatch`);
    // - um cuja chave fixada mudou (`PinChanged`);
    // - um endereço onde ninguém responde;
    // - o verdadeiro.
    // O lote redisca sem quem recusou, e cada recusa vai ao log.
    //
    // A chave de pino de cada um não é o texto do endereço, de propósito: o
    // verificador também registra as duas recusas, sob a chave de pino, e
    // um guarda que casasse com a linha dele não provaria nada sobre a
    // corrida.
    //
    // `#[tokio::test]` de thread única: `set_default` fixa o rastro nesta
    // thread, e a corrida inteira roda nela.
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
    let _guarda = tracing::subscriber::set_default(rastro.clone());

    let (onde_do_impostor, impressao_do_impostor) = ponta_que_so_mostra_a_chave();
    let (onde_do_trocado, impressao_do_trocado) = ponta_que_so_mostra_a_chave();
    let (onde_do_verdadeiro, impressao_do_verdadeiro, _) =
        servidor_que_conta_os_hellos_depois_de(Duration::from_secs(1));
    let fixada_antes = "f".repeat(64);
    let onde_de_quem_nao_sai: SocketAddr = "127.0.0.1:9".parse().expect("endereço");

    let loja = Arc::new(crate::tofu::MemoryPinStore::new());
    loja.pin("chave-do-trocado", fixada_antes.clone());
    let quem_nao_sai = destino_na_chave(onde_de_quem_nao_sai, "chave-de-quem-nao-sai", None);
    let destinos = vec![
        destino_na_chave(
            onde_do_impostor,
            "chave-do-impostor",
            Some(&impressao_do_verdadeiro),
        ),
        destino_na_chave(onde_do_trocado, "chave-do-trocado", None),
        quem_nao_sai,
        destino_na_chave(
            onde_do_verdadeiro,
            "chave-do-verdadeiro",
            Some(&impressao_do_verdadeiro),
        ),
    ];

    let entrou = tokio::time::timeout(
        Duration::from_secs(20),
        Enlace::conectar_entre_com_bilhete(
            destinos,
            None,
            SigningKey::from_bytes(&[9; 32]),
            Arc::clone(&loja) as Arc<dyn PinStore>,
        ),
    )
    .await
    .expect("a corrida não terminou");
    assert!(
        entrou.is_ok(),
        "o candidato verdadeiro não venceu a corrida ({:?}), e o teste mediu outra coisa",
        entrou.err()
    );

    let linhas = rastro.linhas();
    let do_impostor = onde_do_impostor.to_string();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("WARN")
            && linha.contains(&do_impostor)
            && linha.contains(&impressao_do_verdadeiro)
            && linha.contains(&impressao_do_impostor)),
        "a recusa pela impressão de um candidato perdedor sumiu do log quando outro entrou: \
             quem lê o `seele.log` não fica sabendo que um servidor com a chave errada atendeu \
             em {do_impostor}. Rastro: {linhas:?}"
    );
    let do_trocado = onde_do_trocado.to_string();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("WARN")
            && linha.contains(&do_trocado)
            && linha.contains(&fixada_antes)
            && linha.contains(&impressao_do_trocado)),
        "a chave trocada de um candidato perdedor sumiu do log quando outro entrou: o \
             alarme do ADR 0003 em {do_trocado} não deixou rastro. Rastro: {linhas:?}"
    );
}

#[tokio::test]
async fn sem_vencedor_a_recusa_de_um_candidato_vai_ao_log_do_mesmo_jeito() {
    // O caminho sem vencedor já registrava cada falha, em `info`. A recusa
    // pela impressão sobe para `warn`, com o endereço e as duas impressões,
    // pela mesma regra do caminho com vencedor: as duas voltas escrevem a
    // falha de um candidato pela mesma função, e um guarda só no caminho
    // com vencedor deixaria este mudar calado.
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
    let _guarda = tracing::subscriber::set_default(rastro.clone());

    let (onde_do_impostor, impressao_do_impostor) = ponta_que_so_mostra_a_chave();
    let esperada = "e".repeat(64);
    let quem_nao_sai = destino_na_chave(
        "127.0.0.1:9".parse().expect("endereço"),
        "chave-de-quem-nao-sai",
        None,
    );
    let destinos = vec![
        destino_na_chave(onde_do_impostor, "chave-do-impostor", Some(&esperada)),
        quem_nao_sai,
    ];

    let resultado = tokio::time::timeout(
        Duration::from_secs(20),
        Enlace::conectar_entre_com_bilhete(
            destinos,
            None,
            SigningKey::from_bytes(&[9; 32]),
            Arc::new(crate::tofu::MemoryPinStore::new()) as Arc<dyn PinStore>,
        ),
    )
    .await
    .expect("a corrida não terminou");
    assert!(
        resultado.is_err(),
        "um candidato entrou numa corrida sem servidor verdadeiro, e o teste mediu outra coisa"
    );

    let linhas = rastro.linhas();
    let do_impostor = onde_do_impostor.to_string();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("WARN")
            && linha.contains(&do_impostor)
            && linha.contains(&esperada)
            && linha.contains(&impressao_do_impostor)),
        "sem vencedor, a recusa pela impressão de um candidato não foi ao log em `warn` com \
             o endereço e as duas impressões. Rastro: {linhas:?}"
    );
}

#[test]
fn um_anexo_que_nao_foi_salvo_deixa_o_porque_no_log() {
    // A tela recebe `Falhou` e mais nada: o `NaoSalvou` do core não leva
    // motivo. Mas quem investiga depois precisa saber se foi o hash, o
    // prazo ou o disco, e só o motor sabe — se ele não escreve, a pergunta
    // volta dias depois sem dado nenhum junto.
    let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::WARN);
    let _guarda = tracing::subscriber::set_default(rastro.clone());

    let fim = fim_do_salvar(
        AttachmentId(41),
        std::path::Path::new("/tmp/pasta/.foto.png.seele-parcial"),
        Err(anyhow::anyhow!(
            "o arquivo não chegou inteiro e foi descartado"
        )),
    );
    assert_eq!(
        fim,
        Transferencia::NaoSalvou {
            anexo: AttachmentId(41)
        },
        "um anexo que não fechou com o hash não chegou à tela como não salvo"
    );
    let linhas = rastro.linhas();
    assert!(
        linhas.iter().any(|linha| linha.starts_with("WARN")
            && linha.contains("anexo=41")
            && linha.contains("o arquivo não chegou inteiro")),
        "um anexo que não foi salvo não deixou no `seele.log` qual anexo foi \
             nem por quê, e o motor era o único que sabia. Rastro: {linhas:?}"
    );

    let salvo = fim_do_salvar(
        AttachmentId(42),
        std::path::Path::new("/tmp/pasta/.foto.png.seele-parcial"),
        Ok((3_000, std::path::PathBuf::from("/tmp/pasta/foto (2).png"))),
    );
    assert_eq!(
        salvo,
        Transferencia::Salvo {
            anexo: AttachmentId(42),
            caminho: std::path::PathBuf::from("/tmp/pasta/foto (2).png"),
        },
        "um anexo salvo não levou à tela o caminho real em que ficou"
    );
}

#[test]
fn dizer_nao_e_lembrado() {
    // Só estado é restaurado. Reenviar mensagens numa reconexão duplicaria
    // o que a pessoa disse, e a idempotência do protocolo protege contra
    // reenvio do **mesmo** identificador, não contra este erro.
    let mut motor = motor_de_teste();
    motor.lembrar(&Comando::Dizer {
        linha: ChannelId(1),
        corpo: "oi".into(),
        responde_a: None,
        id: ClientMessageId(1),
    });
    assert_eq!(motor.linha, None);
}

#[test]
fn moderar_nao_e_lembrado() {
    // Um `Expulsar` guardado seria refeito ao voltar da bateria: cinco
    // minutos depois, alguém que já tinha reconectado cairia de novo, sem
    // ninguém ter pedido nada. O mesmo para banir.
    let mut motor = motor_de_teste();
    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2), None));

    motor.lembrar(&Comando::Expulsar {
        pessoa: PersonId(9),
    });
    motor.lembrar(&Comando::Banir {
        pessoa: PersonId(9),
        motivo: None,
        expira_em: None,
    });
    motor.lembrar(&Comando::MoverPersono {
        pessoa: PersonId(9),
        voice_room: VoiceRoomId(5),
    });
    motor.lembrar(&Comando::RemoverMensagem {
        mensagem: MessageId(1),
    });
    // E os do protocolo 13: uma edição refeita minutos depois reescreveria
    // por cima de uma edição mais nova, e um desbanimento refeito desfaria
    // o banimento que alguém pôs de novo no intervalo.
    motor.lembrar(&Comando::EditarMensagem {
        mensagem: MessageId(1),
        corpo: "outra coisa".into(),
    });
    motor.lembrar(&Comando::Desbanir {
        pessoa: PersonId(9),
    });
    motor.lembrar(&Comando::ListarBanidos);

    assert_eq!(
        motor.voice_room,
        Some(VoiceRoomId(2)),
        "moderar outra pessoa mexeu em onde este cliente está"
    );
}

/// **Um servidor do 12 não ouve o relato de recepção**, e antes do aperto
/// de mão (zero) ninguém ouve. O 12 é a v0.19.6 publicada: ele pularia a
/// variante calado, a cada segundo, e contaria cada uma como desconhecida.
#[test]
fn so_um_servidor_do_treze_ouve_o_relato_de_recepcao() {
    assert!(!o_servidor_ouve_o_relato(0));
    for antigo in seele_proto::version::VERSAO_MINIMA..13 {
        assert!(!o_servidor_ouve_o_relato(antigo), "protocolo {antigo}");
    }
    assert!(o_servidor_ouve_o_relato(13));
    assert!(o_servidor_ouve_o_relato(seele_proto::PROTOCOL_VERSION));
}

/// O relato não é intenção que se refaz na volta: é uma medida de agora.
#[test]
fn o_relato_de_recepcao_nao_e_lembrado() {
    let mut motor = motor_de_teste();
    motor.lembrar(&Comando::EntrarNaVoiceRoom(VoiceRoomId(2), None));
    motor.lembrar(&Comando::RelatarRecepcao(Recepcao {
        perda: Some(0.1),
        jitter_ms: Some(3.0),
    }));
    assert_eq!(motor.voice_room, Some(VoiceRoomId(2)));
    assert_eq!(
        Comando::RelatarRecepcao(Recepcao {
            perda: None,
            jitter_ms: None
        })
        .nome(),
        "RelatarRecepcao"
    );
}

// ------------------------------------------------------- a tela

/// Uma captura de mentira: entrega sempre um quadro do tamanho pedido.
///
/// A da máquina precisa de monitor e de permissão, e nenhum dos dois existe
/// num agente de integração contínua. O que estes testes provam é a máquina
/// de estados, não a ScreenCaptureKit.
#[derive(Debug)]
struct CapturaDeMentira;

#[derive(Debug)]
struct FonteDeMentira {
    largura: usize,
    altura: usize,
    passo: std::sync::atomic::AtomicUsize,
}

impl crate::video::FonteDeQuadros for FonteDeMentira {
    fn tomar(&self) -> Option<seele_video::codec::QuadroI420> {
        let passo = self
            .passo
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (largura, altura) = (self.largura, self.altura);
        let mut luma = Vec::with_capacity(largura * altura);
        for linha in 0..altura {
            for coluna in 0..largura {
                // Bordas duras: um quadro chapado sairia com trinta bytes e
                // não provaria que o codificador rodou.
                let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
                luma.push(if claro { 235 } else { 16 });
            }
        }
        let croma = vec![128; largura.div_ceil(2) * altura.div_ceil(2)];
        seele_video::codec::QuadroI420::novo(largura, altura, luma, croma.clone(), croma).ok()
    }
}

impl crate::video::Captura for CapturaDeMentira {
    type Fonte = FonteDeMentira;

    fn iniciar(
        &mut self,
        resolucao: seele_video::codec::Resolucao,
        _cadencia: seele_video::codec::Cadencia,
    ) -> Result<Self::Fonte, crate::video::CapturaRecusou> {
        Ok(FonteDeMentira {
            largura: resolucao.largura(),
            altura: resolucao.altura(),
            passo: std::sync::atomic::AtomicUsize::new(0),
        })
    }
}

/// O módulo do Cisco, ou `None` com o motivo impresso.
///
/// **Pula em vez de falhar, e o motivo é a licença**: o módulo não pode
/// morar neste repositório, e um teste que o exigisse seria vermelho em toda
/// máquina limpa — um teste sempre vermelho é um teste que todo mundo
/// aprende a ignorar. Mesma decisão de `crate::bomba` e de `crate::video`.
fn biblioteca_de_teste() -> Option<seele_video::BibliotecaDeVideo> {
    let mut pastas = Vec::new();
    if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
        let caminho = std::path::PathBuf::from(apontado);
        pastas.push(if caminho.is_dir() {
            caminho
        } else {
            caminho.parent().map_or_else(|| caminho.clone(), Into::into)
        });
    }
    pastas.push(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target"),
    );
    match seele_video::BibliotecaDeVideo::procurar_e_carregar(&pastas) {
        Ok(biblioteca) => Some(biblioteca),
        Err(motivo) => {
            // Ver `seele-video/tests/ida_e_volta.rs`: onde o codec é
            // exigido, faltar é falha e não licença para pular. Um teste
            // que volta cedo conta como passado, e é assim que uma suíte
            // fica verde sem nunca ter rodado.
            // Só onde há módulo publicado; no Linux o Cisco não publica.
            assert!(
                std::env::var_os("SEELE_EXIGE_CODEC").is_none()
                    || seele_video::modulo::publicado_para_este_sistema().is_none(),
                "SEELE_EXIGE_CODEC está ligado, este sistema tem módulo publicado \
                     e ele não está aqui: {motivo}"
            );
            eprintln!(
                "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe \
                     isso. Aponte-o com SEELE_OPENH264.\n  Ligue SEELE_EXIGE_CODEC para que \
                     faltar vire falha em vez de pulo."
            );
            None
        }
    }
}

fn pedido_de_teste(biblioteca: seele_video::BibliotecaDeVideo) -> PedidoDeTela {
    PedidoDeTela::DoSistema {
        biblioteca,
        captura: crate::video::CapturaEmCaixa::nova(CapturaDeMentira),
        origem: seele_proto::screen::ScreenSource::Monitor,
    }
}

#[test]
fn a_bomba_so_nasce_quando_o_server_da_nome_a_transmissao() {
    let Some(biblioteca) = biblioteca_de_teste() else {
        return;
    };
    let mut motor = motor_de_teste();

    // O botão foi apertado: o que existe é o pedido, e mais nada. Uma bomba
    // aqui seria uma bomba sem `ScreenId` para pôr no cabeçalho do fluxo.
    motor.tela_pedida = Some((
        Box::new(pedido_de_teste(biblioteca)),
        crate::video::LimitesDeTela::default(),
    ));
    assert!(
        motor.tela_viva.is_none(),
        "o pedido sozinho já tinha ligado a captura"
    );

    // O servidor respondeu, e a transmissão ganhou nome.
    let mut canal = None;
    motor.nascer_a_tela(ScreenId(7), |origem, eventos, _, _| {
        canal = Some((origem, eventos));
    });

    let viva = motor.tela_viva.as_ref().expect("a bomba não nasceu");
    assert_eq!(viva.tela, ScreenId(7), "a bomba nasceu com outro nome");
    assert!(
        motor.tela_pedida.is_none(),
        "o pedido sobreviveu à transmissão que ele abriu, e o próximo \
             `ScreenShareStarted` abriria uma segunda"
    );

    let (origem, mut eventos) = canal.expect("quem escoa não recebeu o canal");
    assert_eq!(
        origem,
        seele_proto::screen::ScreenSource::Monitor,
        "o cabeçalho do fluxo sairia dizendo que é uma janela"
    );

    // E ela está codificando de verdade: o primeiro evento é o fluxo a
    // abrir, que só sai depois de a captura e o codificador armarem.
    match eventos.blocking_recv() {
        Some(crate::EventoDaBomba::Fluxo { geracao, .. }) => assert_eq!(geracao, 1),
        outro => panic!("a bomba não abriu fluxo nenhum: {outro:?}"),
    }

    // E morre com o enlace. Uma bomba que sobrevivesse à queda seria uma
    // thread codificando para uma conexão morta.
    motor.cair();
    assert!(
        motor.tela_viva.is_none(),
        "a bomba sobreviveu à queda do enlace"
    );
    assert!(
        motor.tela_pedida.is_none(),
        "o pedido sobreviveu à queda, e a reconexão poria a tela de alguém \
             no ar sem que ninguém apertasse nada"
    );

    // A thread acabou de fato: o `Fim` é a última coisa que ela manda, e o
    // canal fecha depois dele.
    let mut acabou = false;
    while let Some(evento) = eventos.blocking_recv() {
        if matches!(evento, crate::EventoDaBomba::Fim(None)) {
            acabou = true;
        }
    }
    assert!(acabou, "a thread do codificador não disse que acabou");
}

/// O fecho: do pedido guardado ao primeiro quadro lido do outro lado.
///
/// **É a pergunta que esta tarefa existe para responder** — apertar
/// compartilhar faz um quadro sair pela conexão? — e ela não se responde
/// olhando `tela_viva`: o que prova é o cabeçalho que quem recebe lê e os
/// bytes que vêm depois dele. `crate::bomba` já prova a bomba contra uma
/// conexão; o que estava sem prova é a costura do meio, que é o
/// `Escoadouro` que o [`Client`] embrulha e a tarefa que o motor solta.
#[tokio::test(flavor = "multi_thread")]
async fn do_pedido_guardado_ao_quadro_lido_do_outro_lado() {
    let Some(biblioteca) = biblioteca_de_teste() else {
        return;
    };
    // O par paciente, e não o `par`: a bomba arma depois do aperto, e o
    // prazo de ociosidade do QUIC não pode ser menor que a paciência com
    // ela. Ver `crate::bomba::tests::par_paciente`.
    let (saida, entrada) = crate::bomba::tests::par_paciente().await;
    let escoadouro = crate::bomba::Escoadouro::nova(saida);
    let tela = ScreenId(0x00C0_FFEE);

    let mut motor = motor_de_teste();
    motor.tela_pedida = Some((
        Box::new(pedido_de_teste(biblioteca)),
        crate::video::LimitesDeTela::default(),
    ));

    // A mesma forma da produção: uma tarefa própria, porque escoar dura o
    // que a transmissão durar.
    let mut escoando = None;
    motor.nascer_a_tela(tela, |origem, mut eventos, _, _| {
        escoando = Some(tokio::spawn(async move {
            escoadouro.escoar(tela, origem, &mut eventos).await
        }));
    });
    let escoando = escoando.expect("quem escoa não foi chamado");

    let mut recepcao =
        crate::bomba::tests::aceitar_com_paciencia(&entrada, || escoando.is_finished()).await;
    assert_eq!(
        recepcao.cabecalho().screen,
        tela,
        "o fluxo abriu com o nome de outra transmissão"
    );
    assert_eq!(
        recepcao.cabecalho().source,
        seele_proto::screen::ScreenSource::Monitor,
        "o cabeçalho diz janela sobre um monitor"
    );

    let primeiro = recepcao
        .proximo_quadro()
        .await
        .expect("ler o primeiro quadro")
        .expect("o fluxo não podia ter acabado");
    assert!(primeiro.chave(), "o primeiro quadro de um fluxo é chave");
    assert!(!primeiro.bytes.is_empty(), "saiu um quadro vazio");

    // E a queda do enlace fecha tudo: a bomba morre e o fluxo termina.
    motor.cair();
    let contagem = escoando
        .await
        .expect("a tarefa de escoar")
        .expect("escoar até o fim");
    assert!(contagem.enviados >= 1, "nenhum quadro chegou ao fio");
    assert_eq!(contagem.fluxos, 1);
}

#[test]
fn sem_pedido_guardado_um_nome_de_transmissao_nao_liga_nada() {
    // O servidor reenvia `ScreenShareStarted` a cada pessoa que entra numa sala de voz
    // onde já há transmissão — inclusive a quem está transmitindo. Sem esta
    // guarda, cada pessoa entrando na sala ligaria outra captura da mesma
    // tela.
    let mut motor = motor_de_teste();
    let mut chamou = false;
    motor.nascer_a_tela(ScreenId(1), |_, _, _, _| chamou = true);
    assert!(!chamou, "ligou uma bomba que ninguém pediu");
    assert!(motor.tela_viva.is_none());
}

/// **Uma transmissão que cai acaba de verdade, e o motor diz por quê.**
///
/// O defeito: quando a tarefa que escoa acabava com erro — o codificador
/// recusando um quadro, a captura recusando recomeçar, o fluxo que não abriu —
/// só saía um `warn`. `tela_viva` continuava `Some` com um produtor morto
/// dentro, e nenhum aviso ia à casca: o botão seguia em PARAR DE COMPARTILHAR
/// sobre uma tela que ninguém via.
///
/// Sem codec nenhum, de propósito: a câmera entra por `crate::de_fora`, que não
/// carrega o módulo do Cisco, e o que se mede aqui é o motor — o que ele faz
/// com o fim com erro, e com qual transmissão. O servidor recebendo o
/// `StopScreenShare` é de `seele-conformance` (`a_transmissao_que_cai.rs`),
/// que é onde há um servidor.
#[tokio::test]
async fn uma_transmissao_que_cai_acaba_de_verdade_e_o_motor_diz_por_que() {
    let mut motor = motor_de_teste();
    let (avisos, mut ouvidos) = mpsc::unbounded_channel();
    motor.avisos = avisos;
    motor.protocolo.store(
        seele_proto::screen::PROTOCOLO_DA_CAMERA,
        std::sync::atomic::Ordering::Relaxed,
    );

    let (_entrada, quadros) = crate::de_fora::canal_de_fora(640, 360);
    motor.tela_pedida = Some((
        Box::new(PedidoDeTela::DeFora(quadros)),
        crate::video::LimitesDeTela::default(),
    ));
    // O canal fica na mão do teste: quem escoa, aqui, é ninguém.
    let mut canal = None;
    motor.nascer_a_tela(ScreenId(9), |_, eventos, _, leitura| {
        canal = Some((eventos, leitura));
    });
    let leitura = canal.as_ref().expect("quem escoa não foi chamado").1;
    assert_eq!(
        motor.tela_viva.as_ref().map(|viva| viva.leitura),
        Some(leitura),
        "a transmissão viva e quem a escoa têm números diferentes: um fim com erro nunca \
         acharia a transmissão dele"
    );
    // O sinal de banda da abertura, que não é assunto deste teste.
    while ouvidos.try_recv().is_ok() {}

    // **O fim com erro de outra transmissão não derruba esta.** É o caso de
    // toda parada decidida: o motor tira `tela_viva` antes, a tarefa da
    // transmissão anterior acaba depois — às vezes com erro, porque o fluxo
    // dela foi cortado —, e a esta altura já pode haver outra no ar.
    motor
        .a_tela_caiu(FimComErroDaTela {
            leitura: leitura + 1_000_000,
            motivo: crate::tela::MotivoDeQueda::Fluxo,
        })
        .await;
    assert!(
        motor.tela_viva.is_some(),
        "o fim de uma transmissão que já tinha acabado derrubou a que está no ar"
    );
    assert!(
        !matches!(ouvidos.try_recv(), Ok(Aviso::TelaCaiu { .. })),
        "a casca ouviu «caiu» de uma transmissão que este motor encerrou por decisão"
    );

    // O desta: ela acaba, e o aviso leva qual, o que era e por quê.
    motor
        .a_tela_caiu(FimComErroDaTela {
            leitura,
            motivo: crate::tela::MotivoDeQueda::Codificador,
        })
        .await;
    assert!(
        motor.tela_viva.is_none(),
        "a transmissão caiu e o motor continuou com ela «viva»: o botão ficaria em PARAR DE \
         COMPARTILHAR sobre uma tela que ninguém vê"
    );
    let mut caiu = None;
    while let Ok(aviso) = ouvidos.try_recv() {
        if let Aviso::TelaCaiu {
            tela,
            origem,
            motivo,
        } = aviso
        {
            assert!(caiu.is_none(), "a mesma queda foi dita duas vezes");
            caiu = Some((tela, origem, motivo));
        }
    }
    assert_eq!(
        caiu,
        Some((
            ScreenId(9),
            seele_proto::screen::ScreenSource::Camera,
            crate::tela::MotivoDeQueda::Codificador
        )),
        "a transmissão caiu e a casca não ficou sabendo qual, o que era ou por quê"
    );

    // Dito uma vez: o mesmo fim, de novo, não acha transmissão nenhuma.
    motor
        .a_tela_caiu(FimComErroDaTela {
            leitura,
            motivo: crate::tela::MotivoDeQueda::Codificador,
        })
        .await;
    assert!(
        !matches!(ouvidos.try_recv(), Ok(Aviso::TelaCaiu { .. })),
        "a queda foi dita duas vezes"
    );
    drop(canal);
}

#[test]
fn compartilhar_tela_nao_e_lembrado_para_a_reconexao() {
    // Refeito depois de cinco minutos de bateria, poria o monitor de alguém
    // no ar sem que ninguém tivesse apertado nada — e minutos depois de a
    // pessoa ter desistido.
    let mut motor = motor_de_teste();
    motor.voice_room = Some(VoiceRoomId(3));
    motor.lembrar(&Comando::PararDeCompartilhar);
    assert_eq!(motor.voice_room, Some(VoiceRoomId(3)));
}

/// A metade da regra de aceite do §3.2 que faltava.
///
/// O teto respondia ao `HostUplink` e ao número de espectadores e **não**
/// ao sinal da voz piorando: numa sala onde a voz começava a doer, a tela
/// não cedia sozinha. A perna que faltava já vinha pelo fio — o servidor
/// devolve a taxa de cada pessoa em `PersonState`, uma vez por segundo — e
/// ninguém guardava a sua.
#[test]
fn a_faixa_da_voz_desce_pelo_que_o_server_devolve() {
    use seele_proto::control::{PersonState, Presence};

    let eu = PersonId(7);
    let outra = PersonId(9);
    let estado = |pessoa: PersonId, taxa: u8| PersonState {
        person: pessoa,
        signal: taxa,
        speaking: false,
        muted: false,
        total_isolation: false,
        presence: Presence::Available,
    };

    // A voz doendo derruba a faixa, e é isto que faz a tela ceder.
    assert_eq!(
        faixa_nova(SignalBand::Nominal, &estado(eu, 10), Some(eu)),
        Some(SignalBand::Critical),
        "a taxa despencou e a faixa não acompanhou"
    );

    // A de outra pessoa não move nada. Sem esta guarda, a tela cederia
    // porque a conexão **de outro** piorou — e quem compartilha ficaria
    // pagando pelo vizinho.
    assert_eq!(
        faixa_nova(SignalBand::Nominal, &estado(outra, 10), Some(eu)),
        None
    );

    // Sem sessão não há «esta pessoa». Uma mensagem antes do aperto de mão
    // terminar não é sobre ninguém que este motor conheça.
    assert_eq!(faixa_nova(SignalBand::Nominal, &estado(eu, 10), None), None);

    // E a mesma faixa não vira ordem: a taxa chega uma vez por segundo e
    // quase sempre no mesmo degrau, e refazer o teto a cada chegada
    // acordaria a thread do codificador para lhe dizer o que ela já sabe.
    assert_eq!(
        faixa_nova(SignalBand::Critical, &estado(eu, 10), Some(eu)),
        None
    );

    // E ela sobe de volta: ceder não pode ser de mão única, ou a tela
    // ficaria pequena para sempre depois do primeiro engasgo.
    assert_eq!(
        faixa_nova(SignalBand::Critical, &estado(eu, 100), Some(eu)),
        Some(SignalBand::Nominal)
    );
}

/// **A perna que faltava no teto, ligada.**
///
/// O motor usava `TetoDeVideo::novo()` para a perna de quem compartilha, o
/// que quer dizer o cano das provas — 2 Mbps — para sempre, em toda casa.
/// Quem tinha fibra via 720p a sessão inteira. Agora aquela perna sai da
/// [`crate::caminho::Sonda`], e este teste é o fio entre as duas: janelas
/// cheias e calmas entram, e o teto que sai da mesma função sobe.
#[test]
fn o_teto_sai_do_caminho_que_a_sonda_mediu_e_nao_da_suposicao() {
    use crate::caminho::{Amostra, Transporte};
    use crate::tela::Teto;

    let mut motor = motor_de_teste();
    // O servidor declarou uma subida larga, então a perna dele sai da frente e
    // quem manda no `min` do §5.1 é a desta máquina. Sem isto o teto ficaria
    // preso em 1200 kbps por causa da **outra** perna, que é um achado
    // separado — ver `caminho::tests::sem_a_subida_do_server_...`.
    motor.caminho_de_quem_hospeda_bps = Some(100_000_000);

    // Antes de medir, a suposição de sempre: a primeira transmissão de uma
    // sessão abre exatamente com o teto que abria antes deste módulo.
    assert_eq!(
        motor.teto_de_video(None).teto(SignalBand::Nominal),
        Teto::Bps(1_200_000)
    );

    // Cinco janelas cheias e sem piora, como a tica do motor as entregaria.
    let inicio = Instant::now();
    let mut bytes = 0_u64;
    for segundo in 0..12_u32 {
        let teto = motor.teto_de_video(None).teto(SignalBand::Nominal);
        // O que sai pelo soquete numa janela em que a tela encheu o teto: o
        // orçamento inteiro mais a voz.
        bytes += u64::from(teto.bps() + 60_000) / 8;
        let amostra = Amostra {
            transporte: Transporte {
                bytes_enviados: bytes,
                // A telemetria da 0.17: sem perda nem congestionamento, com a
                // ida e volta do `Ping` da sonda andando a cada janela.
                eventos_de_congestionamento: None,
                pacotes_perdidos: None,
                ida_e_volta: Some(crate::caminho::IdaEVolta {
                    valor: Duration::from_millis(20),
                    amostras: u64::from(segundo) * 2 + 1,
                    em_voo: None,
                }),
            },
            teto,
            faixa: SignalBand::Nominal,
        };
        motor
            .caminho
            .observar(inicio + Duration::from_secs(u64::from(segundo)), &amostra);
    }

    let teto = motor.teto_de_video(None).teto(SignalBand::Nominal);
    assert!(
        teto.bps() > 1_200_000,
        "doze janelas cheias e o teto continuou na suposição: {teto:?}"
    );
    assert_eq!(
        teto.resolucao_estimada(),
        Some(seele_video::codec::Resolucao::P720),
        "o caminho medido comprava 720p e a tela continuou menor"
    );
}

/// Sem transmissão não há quem encha o cano, e ler o transporte ali seria
/// medir a voz — que diz que está bom a 40 kbps e não diz quanto cabe.
#[test]
fn sem_tela_no_ar_a_sonda_nao_e_alimentada() {
    let mut motor = motor_de_teste();
    let antes = motor.caminho.estimativa();
    motor.medir_o_caminho();
    assert_eq!(motor.caminho.estimativa(), antes);
    assert_eq!(
        motor.teto_de_video(None).teto(SignalBand::Nominal),
        crate::tela::Teto::Bps(1_200_000)
    );
}

/// **Sem `HostUplink`, a perna do hospedeiro sai da escolha.**
///
/// É o pedido inteiro: escolher 1080p e não esperar a escada subir de
/// 2 Mbps em oito janelas.
#[test]
fn sem_host_uplink_a_perna_do_hospedeiro_sai_da_escolha() {
    use crate::tela::Teto;
    use crate::video::LimitesDeTela;
    use seele_video::codec::Resolucao;

    let mut motor = motor_de_teste();
    // A perna de quem compartilha sai da frente, para o teto revelar a do
    // hospedeiro sozinha. Em produção ela também parte da escolha — o
    // `Comando::CompartilharTela` semeia a sonda com o mesmo número —, e aí
    // as duas dão o mesmo valor; aqui ela é posta alta de propósito para
    // que este teste falhe pelo motivo que ele nomeia.
    motor.caminho = crate::caminho::Sonda::partindo_de(100_000_000);
    motor.caminho_de_quem_hospeda_bps = None;
    let limites = LimitesDeTela {
        resolucao: Resolucao::P1080,
        ..LimitesDeTela::default()
    };

    let teto = motor
        .teto_de_video(Some(&limites))
        .teto(SignalBand::Nominal);

    // 10 400 000 de piso × 60% = 6 240 000, o limiar de 1080p em cheio.
    assert_eq!(
        teto,
        Teto::Bps(6_240_000),
        "escolher 1080p sem medida nenhuma do hospedeiro tinha de abrir no \
             teto que 1080p custa; abriu em {teto:?} — a escolha só passaria a \
             valer oito janelas depois, que é o que este piso existe para não \
             fazer"
    );
}

/// **A semente lembrada é grampeada e não volta ao disco como medida.**
///
/// Decisão da 0.17: quem chama `lembrar_o_caminho` pode ter lido um número
/// sem data nem rede, e o motor não dá a ele confiança ilimitada; e uma
/// sessão que só recebeu a semente publica zero, que quem grava lê como
/// «não mediu».
#[test]
fn o_caminho_lembrado_tem_teto_e_nao_vira_medida() {
    let mut motor = motor_de_teste();
    motor.lembrar(&Comando::LembrarCaminho(90_000_000));
    assert_eq!(
        motor.caminho.estimativa(),
        crate::caminho::TETO_DA_SEMENTE_BPS,
        "a semente passou do teto da semente"
    );
    assert_eq!(
        motor
            .caminho_medido
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "a semente foi publicada como medida, e voltaria ao disco com data nova"
    );
}

/// **Uma medida baixa desmente a escolha, e vence.**
///
/// Repetir o palpite por cima de um número que a máquina já mediu seria
/// insistir no que ela sabe estar errado. O lado alto está coberto pelo
/// teste acima, onde o piso manda porque não há medida nenhuma.
#[test]
fn a_medida_baixa_do_hospedeiro_vence_o_piso_da_escolha() {
    use crate::tela::Teto;
    use crate::video::LimitesDeTela;
    use seele_video::codec::Resolucao;

    let mut motor = motor_de_teste();
    motor.caminho = crate::caminho::Sonda::partindo_de(100_000_000);
    // Um mega medido: muito abaixo dos 10,4 que 1080p pediria de palpite.
    motor.caminho_de_quem_hospeda_bps = Some(1_000_000);
    let limites = LimitesDeTela {
        resolucao: Resolucao::P1080,
        ..LimitesDeTela::default()
    };

    let teto = motor
        .teto_de_video(Some(&limites))
        .teto(SignalBand::Nominal);

    assert_eq!(
        teto,
        Teto::Bps(600_000),
        "com 1 Mbps medidos do hospedeiro o teto saiu {teto:?}: o piso da \
             escolha passou por cima de uma medida de verdade, e o produto \
             voltou a prometer banda que ninguém conferiu"
    );
}

/// **Fora de uma transmissão não há escolha**, e o padrão segue a suposição.
#[test]
fn sem_transmissao_o_padrao_do_hospedeiro_segue_a_suposicao() {
    use crate::tela::Teto;

    let mut motor = motor_de_teste();
    motor.caminho_de_quem_hospeda_bps = None;

    let teto = motor.teto_de_video(None).teto(SignalBand::Nominal);

    assert_eq!(
        teto,
        Teto::Bps(1_200_000),
        "sem escolha nenhuma o teto deixou de ser o de CAMINHO_DA_PROVA_BPS: \
             {teto:?}"
    );
}

#[tokio::test]
async fn sem_sessao_o_pedido_de_tunel_e_sem_resposta_e_o_servidor_onze_e_antigo() {
    use crate::client::NaoSaiu;
    let mut motor = motor_de_teste();
    let (resposta, caixa) = tokio::sync::oneshot::channel();
    motor
        .entrar_no_tunel("seele/jogo".into(), "mundo".into(), resposta)
        .await;
    assert_eq!(
        caixa.await.unwrap().unwrap_err(),
        RecusaDeTunel::SemResposta
    );
    let (resposta, caixa) = tokio::sync::oneshot::channel();
    motor
        .servir_tunel(
            "seele/jogo".into(),
            "mundo".into(),
            ProtocoloDeTunel::Tcp,
            1,
            resposta,
        )
        .await;
    assert_eq!(
        caixa.await.unwrap().unwrap_err(),
        RecusaDeTunel::SemResposta
    );
    // E o que o cliente diz do verbo que não saiu vira a recusa certa.
    assert_eq!(
        recusa_do_verbo(&NaoSaiu::ServidorAntigo),
        RecusaDeTunel::ServidorAntigo
    );
    assert_eq!(
        recusa_do_verbo(&NaoSaiu::Escrita("fechado".into())),
        RecusaDeTunel::SemResposta
    );
}

fn motor_de_teste() -> Motor {
    let (avisos, _) = mpsc::unbounded_channel();
    let (resultados_do_par_tx, resultados_do_par) = mpsc::unbounded_channel();
    let (quedas_da_tela_tx, quedas_da_tela) = mpsc::unbounded_channel();
    let tuneis = crate::tunel::Tuneis::nova(avisos.clone());
    Motor {
        bilhete: None,
        destino: Destino {
            servidor: "127.0.0.1:1".parse().expect("endereço"),
            nome_tls: "localhost".into(),
            chave_do_pin: "127.0.0.1:1".into(),
            apelido: "pessoa".into(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        chave: SigningKey::from_bytes(&[7; 32]),
        pins: Arc::new(crate::tofu::MemoryPinStore::new()),
        cliente: None,
        bateria: Battery::new(),
        inicio: Instant::now(),
        voice_room: None,
        entradas: crate::assento::EntradasPendentes::default(),
        queda: None,
        linha: None,
        muted: false,
        isolamento: false,
        avisos,
        rtt: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        tela_pedida: None,
        tela_viva: None,
        faixa: FAIXA_INICIAL,
        caminho_de_quem_hospeda_bps: None,
        espectadores: 0,
        caminho: crate::caminho::Sonda::nova(),
        estimativa_da_midia: None,
        ultima_chave_pela_midia: None,
        chave_devida: false,
        ultima_mudanca_da_midia: None,
        caminho_medido: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        protocolo: Arc::new(std::sync::atomic::AtomicU8::new(0)),
        atendendo_pares: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        consentimento: ConsentimentoDePar::de_ninguem(),
        repasse: Arc::new(RepasseDeTela::default()),
        resultados_do_par,
        resultados_do_par_tx,
        quedas_da_tela,
        quedas_da_tela_tx,
        tarefas_de_par: TarefasDePar::default(),
        tuneis,
        lado_da_maquina: LadoDaMaquina::default(),
    }
}

/// **Uma tarefa de par não sobrevive ao motor que a criou.**
///
/// # A alça largada não aborta nada
///
/// `Motor::caminhos_de_par` guardava `JoinHandle`s num mapa comum, e
/// **largar um `JoinHandle` desprende a tarefa — não a cancela**. Os dois
/// caminhos que matam este motor largam o mapa sem tocá-lo:
/// `Enlace::drop` aborta a tarefa de `rodar`, e um `Comando::Sair` a faz
/// voltar; nos dois o `Motor` é solto, o mapa vai junto, e cada tarefa de
/// par fica viva — lendo do par, com a conexão QUIC de pé, para uma sessão
/// que já acabou.
///
/// O que este teste prende é a **posse**: quem for dono das alças tem de
/// abortá-las ao ser solto, e não confiar em ninguém se lembrar de pedir.
/// A tarefa aqui é um contador que não para sozinho nunca — se ele
/// continuar subindo depois de o motor sumir, é porque ninguém a cancelou.
#[tokio::test(flavor = "multi_thread")]
async fn destruir_o_motor_aborta_as_tarefas_de_par_que_ele_guarda() {
    let mut motor = motor_de_teste();

    let batidas = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let contando = Arc::clone(&batidas);
    let tarefa = tokio::spawn(async move {
        loop {
            contando.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
    motor.tarefas_de_par.assistir(ScreenId(1), tarefa);

    // Andar de verdade antes do corte: um contador parado em zero passaria
    // este teste sem que tarefa nenhuma tivesse chegado a existir.
    ate_que(
        "a tarefa de par dar sinal de vida antes de o motor ser destruído",
        Duration::from_secs(5),
        || batidas.load(std::sync::atomic::Ordering::Relaxed) > 0,
    )
    .await;

    drop(motor);

    // `abort` é um pedido, não um corte instantâneo: o runtime solta a
    // tarefa no próximo toque nela. A margem é para isso, e não para
    // esconder um cancelamento que não aconteceu — o que decide é o
    // contador ter **parado**, não onde ele parou.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let parou_em = batidas.load(std::sync::atomic::Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let depois = batidas.load(std::sync::atomic::Ordering::Relaxed);

    assert_eq!(
        depois, parou_em,
        "a tarefa de par continuou correndo depois de o motor ser destruído (subiu de \
             {parou_em} para {depois}): a alça foi largada em vez de abortada, e quem fechou a \
             sessão continua lendo do par com a conexão de pé"
    );
}

/// **A despedida cala o caminho do par antes de dizer que acabou.**
///
/// O `Encerrado` é o último aviso que a casca ouve de uma sessão que ela
/// mesma fechou. `Motor::despedir` segura o motor de pé depois dele,
/// esperando o fechamento sair (`ESPERA_DA_DESPEDIDA`); se as tarefas de
/// par ainda estivessem vivas nesse intervalo, cada quadro que elas
/// lessem do par entraria na fila **depois** do fim — e a soltura do
/// `Motor` só as aborta quando a espera acaba.
///
/// A tarefa daqui é a forma de [`Motor::assistir_por_par`] sem a rede: um
/// laço que escreve quadro na fila de avisos até alguém a parar. O motor
/// fica de pé depois da despedida, como fica durante a espera de verdade.
/// O que este teste prova em `seele-core` é a ordem; a prova pela rede, com
/// um par de verdade, é `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para`,
/// em `seele-conformance`.
#[tokio::test(flavor = "multi_thread")]
async fn a_despedida_cala_as_tarefas_de_par_antes_do_encerrado() {
    let mut motor = motor_de_teste();
    let (avisos, mut ouvidos) = mpsc::unbounded_channel();
    motor.avisos = avisos;

    let entregando = motor.avisos.clone();
    let quadros = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let contando = Arc::clone(&quadros);
    let tarefa = tokio::spawn(async move {
        loop {
            let quadro = Aviso::TelaQuadro {
                tela: ScreenId(1),
                chave: false,
                bytes: vec![0],
                leitura: 1,
            };
            if entregando.send(quadro).is_err() {
                return;
            }
            contando.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    motor.tarefas_de_par.assistir(ScreenId(1), tarefa);
    ate_que(
        "a tarefa de par entregar um quadro antes da despedida",
        Duration::from_secs(5),
        || quadros.load(std::sync::atomic::Ordering::Relaxed) > 0,
    )
    .await;

    motor.despedir().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut viu_o_fim = false;
    let mut depois_do_fim = 0_u32;
    while let Ok(aviso) = ouvidos.try_recv() {
        match aviso {
            Aviso::Encerrado(Motivo::Pedido) => viu_o_fim = true,
            Aviso::TelaQuadro { .. } if viu_o_fim => depois_do_fim += 1,
            _ => {}
        }
    }
    assert!(
        viu_o_fim,
        "a despedida não disse `Encerrado(Pedido)`, e a casca não fica sabendo que a sessão \
             que ela fechou acabou"
    );
    assert_eq!(
        depois_do_fim, 0,
        "{depois_do_fim} quadros do par entraram na fila depois do `Encerrado`: a despedida \
             avisou o fim com o caminho do par de pé, e a casca recebe imagem de uma sessão que \
             ela já fechou"
    );
    drop(motor);
}

/// **Abortar quem servia um par devolve a vaga, em vez de prendê-la.**
///
/// # O risco que o conserto vizinho criou
///
/// Cancelar as tarefas de par fecha um defeito e abre outro. A vaga de
/// `Motor::atendendo_pares` era devolvida na **última linha** do corpo da
/// tarefa que serve — e um `abort` faz a última linha nunca correr. Uma
/// vaga que não volta é esta máquina fora da malha para sempre e **sem erro
/// em lugar nenhum**: o servidor continua apontando este par, porque a vaga
/// dele voltou na queda, e o cliente recusa cada pedido em silêncio pela
/// vaga que ficou. Ninguém do outro lado fica sabendo; só a imagem para de
/// aliviar.
///
/// É por isso que a vaga virou [`VagaDeAtendimento`], um punho com `Drop`:
/// soltar a tarefa solta o guarda, e soltar o guarda devolve a vaga.
///
/// # O que é de produção aqui, e o que não é
///
/// De produção: quem toma a vaga (`VagaDeAtendimento::tomar`), quem põe a
/// tarefa de pé e guarda a alça (`Motor::passar_a_servir`) e quem corta
/// (`Motor::cair`). Montado à mão: só o que `Motor::servir_par` tiraria de
/// um `Client` — a ponta QUIC, a identidade e o endereço do outro lado —,
/// porque um `Client` precisa de um servidor e isso mora em
/// `seele-conformance`.
///
/// # Por que o prazo curto é o discriminador
///
/// O alvo é um **buraco negro**: uma ponta bindada de verdade, sem
/// `ServerConfig` nenhum, que nunca responde — o mesmo truque de
/// [`servir_um_par_desarma_a_ponta_de_quem_empresta_depois_de_servir`], e
/// pela mesma razão de não arriscar um `ECONNREFUSED` rápido do sistema.
/// Contra ele `servir_um_par` gasta [`PRAZO_DO_PAR`] inteiro — três
/// segundos — antes de desistir e devolver a vaga pelo fim do corpo.
///
/// Então a vaga voltar **dentro de um segundo** só pode ter vindo do
/// cancelamento, e não do fim natural. É esse intervalo que separa o
/// conserto do defeito, e é por isso que a espera aqui tem prazo próprio em
/// vez do de [`ate_que`]: sem a alça guardada não há o que abortar, e sem o
/// `Drop` do guarda o `abort` prende a vaga para sempre.
#[tokio::test(flavor = "multi_thread")]
async fn cair_devolve_a_vaga_de_quem_estava_servindo_um_par() {
    let mut motor = motor_de_teste();
    let punho = Arc::clone(&motor.atendendo_pares);

    let vaga = VagaDeAtendimento::tomar(&punho).expect("a vaga nasce livre");
    assert!(
        punho.load(std::sync::atomic::Ordering::Acquire),
        "tomar a vaga não a marcou como tomada, e o resto deste teste não mede nada"
    );
    assert!(
        VagaDeAtendimento::tomar(&punho).is_none(),
        "a vaga foi tomada duas vezes: `crate::par::atender` só aceita uma ligação, e um \
             segundo `SirvaTelaPara` entraria por cima de um repasse em curso"
    );

    let ponta = ponta_de_sessao_de_teste();
    let buraco_negro = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let onde_ninguem_atende = buraco_negro.local_addr().unwrap();
    let impressao = par::impressao(&par::identidade_efemera().unwrap());
    motor.passar_a_servir(
        vaga,
        ponta,
        ScreenId(1),
        vec![onde_ninguem_atende],
        impressao,
    );

    motor.cair();

    ate_que(
        "a vaga de quem servia um par voltar depois da queda, antes de `PRAZO_DO_PAR`",
        Duration::from_secs(1),
        || !punho.load(std::sync::atomic::Ordering::Acquire),
    )
    .await;

    // E a prova que importa: a vaga não voltou só no papel. A conexão que
    // substitui a que caiu precisa **conseguir tomá-la**, senão esta
    // máquina reconecta viva e fora da malha.
    assert!(
        VagaDeAtendimento::tomar(&punho).is_some(),
        "a vaga voltou a `false` e mesmo assim não pôde ser tomada de novo"
    );
}

/// **A reativação tardia, e ela não é hipótese.** O servidor escolhe um
/// par, difunde `AssistaTelaPor` pelo barramento, e só então a retirada de
/// consentimento desta máquina chega lá. As duas se cruzam no fio, e sem o
/// guarda o pedido velho seria atendido: esta máquina discaria — e seria
/// discada — com o endereço que acabou de tirar de circulação.
///
/// # Por que a asserção é sobre o relato, e não sobre a alça
///
/// **Porque a alça não distingue nada, e a primeira versão deste teste
/// passava sem o guarda.** Um `Motor` de unidade não tem `Client`, então
/// `ponta_de_pares` devolve `None` e `assistir_por_par` sai cedo de
/// qualquer maneira — `caminhos` fica vazio nos dois casos, e o teste
/// media um estado idêntico chamando-o de prova. Medido revertendo o
/// guarda: a suíte continuava verde.
///
/// O que **de fato** difere é o relato. A saída por falta de ponta manda
/// `ParFalhou { NaoAlcancou }` ao servidor; a saída por consentimento
/// retirado não manda nada, e não deve mandar — nada falhou na rede, esta
/// máquina é que recusou. Um relato de falha aqui seria o produto dizendo
/// ao servidor uma coisa que não aconteceu.
/// **Quem pede precisa saber o que valeu, e não o que pediu.**
///
/// O teto desta versão do cliente é um par
/// ([`PARES_QUE_ESTA_VERSAO_ATENDE`]), e um pedido acima dele é **baixado**
/// antes de virar declaração — declarar o que esta máquina não atende faria
/// a segunda pessoa esperar o prazo do par vencer por uma promessa que
/// nunca teve como ser cumprida.
///
/// Se essa correção fosse calada, a casca desenharia um deslizante em
/// quatro e o servidor trabalharia com um, sem nada em lugar nenhum
/// dizendo qual dos dois números vale. É a forma do defeito que mais custou
/// caro neste repositório: o produto sabe e não conta. Por isso
/// [`Enlace::consentir_no_caminho_entre_pares`] **devolve** o que valeu.
///
/// # Por que o teste entra por [`cabivel`], e por que isso basta aqui
///
/// Porque construir um [`Enlace`] de mentira exigiria um construtor que só
/// os testes usam, dentro de um tipo de produção — e a casa já pagou por
/// um guarda que existia e não funcionava porque o teste entrava por uma
/// porta que a produção não usa.
///
/// O que sobraria para aquele teste provar — que o valor devolvido é o
/// mesmo que saiu no fio — **não é uma propriedade testável, é uma
/// propriedade estrutural**: os dois saem do mesmo `let`, e não há caminho
/// no código em que possam diferir. Uma asserção sobre isso mediria o
/// compilador.
#[test]
fn o_teto_pedido_acima_do_que_esta_versao_atende_e_baixado() {
    assert_eq!(
        cabivel(ConsentimentoDePar {
            pares_que_atende: 9,
            assiste_por_par: true,
        }),
        ConsentimentoDePar {
            pares_que_atende: PARES_QUE_ESTA_VERSAO_ATENDE,
            assiste_por_par: true,
        },
        "o teto pedido não foi baixado para o que esta versão atende"
    );
}

#[test]
fn um_teto_que_cabe_atravessa_intacto() {
    // A outra metade: um guarda que baixasse tudo para zero passaria no
    // teste acima e desligaria a malha inteira sem um erro em lugar
    // nenhum. E a retirada — `de_ninguem` — tem de atravessar como está,
    // senão retirar o consentimento viraria um pedido de emprestar.
    for cabe in [
        ConsentimentoDePar::de_ninguem(),
        ConsentimentoDePar {
            pares_que_atende: PARES_QUE_ESTA_VERSAO_ATENDE,
            assiste_por_par: false,
        },
        ConsentimentoDePar {
            pares_que_atende: 0,
            assiste_por_par: true,
        },
    ] {
        assert_eq!(cabivel(cabe), cabe, "{cabe:?} foi alterado sem precisar");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn um_pedido_para_assistir_por_par_que_chega_depois_da_retirada_nao_e_atendido() {
    let mut motor = motor_de_teste();
    motor.consentimento = ConsentimentoDePar::de_ninguem();

    motor.assistir_por_par(ScreenId(1), vec![endereco_qualquer()], impressao_qualquer());

    assert!(
        motor.resultados_do_par.try_recv().is_err(),
        "um `AssistaTelaPor` que chegou depois da retirada foi atendido: a discagem correu e \
             relatou uma falha de rede que não aconteceu"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deixar_de_emprestar_cancela_o_repasse_que_ja_estava_em_curso() {
    // **A retirada alcança o que já está no ar, e não só o pedido
    // seguinte.** Sem isto, desligar o empréstimo pararia de aceitar
    // pedidos novos e deixaria a cópia em curso subindo — a pessoa
    // continuaria pagando pela decisão que acabou de desfazer, e o único
    // aviso seria a conta de internet dela.
    //
    // A vaga voltar **antes** de `PRAZO_DO_PAR` é o discriminador, e ele é
    // escolhido e não arbitrário: o alvo é um buraco negro (uma ponta sem
    // `ServerConfig`, que nunca responde), contra o qual a tarefa gasta os
    // três segundos inteiros antes de desistir pelo fim do corpo.
    let mut motor = motor_de_teste();
    let punho = Arc::clone(&motor.atendendo_pares);
    let vaga = VagaDeAtendimento::tomar(&punho).expect("a vaga nasce livre");

    let ponta = ponta_de_sessao_de_teste();
    let buraco_negro = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let onde_ninguem_atende = buraco_negro.local_addr().unwrap();
    let impressao = par::impressao(&par::identidade_efemera().unwrap());
    motor.passar_a_servir(
        vaga,
        ponta,
        ScreenId(1),
        vec![onde_ninguem_atende],
        impressao,
    );

    motor
        .passar_a_consentir(ConsentimentoDePar::de_ninguem())
        .await;

    ate_que(
        "a vaga de quem servia um par voltar depois da retirada, antes de `PRAZO_DO_PAR`",
        Duration::from_secs(1),
        || !punho.load(std::sync::atomic::Ordering::Acquire),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn deixar_de_assistir_por_par_derruba_os_caminhos_de_par_abertos() {
    // A retirada do outro consentimento, e o mesmo princípio: quem diz
    // «não quero mais o meu endereço no ar» tem o caminho que existe por
    // causa daquele endereço desfeito agora, e não na próxima tela que
    // pedir. Quem reabre o cano do servidor é o servidor, ao receber a
    // declaração nova — ver `session::devolver_ao_servidor`.
    let mut motor = motor_de_teste();
    motor.tarefas_de_par.assistir(
        ScreenId(1),
        tokio::spawn(async { std::future::pending::<()>().await }),
    );
    assert_eq!(motor.tarefas_de_par.caminhos.len(), 1);

    motor
        .passar_a_consentir(ConsentimentoDePar {
            pares_que_atende: 1,
            assiste_por_par: false,
        })
        .await;

    assert!(
        motor.tarefas_de_par.caminhos.is_empty(),
        "retirar o consentimento de assistir por par deixou o caminho aberto de pé"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn continuar_consentindo_no_mesmo_nao_derruba_nada() {
    // O guarda contra o oposto, e ele é o que impede a malha de morrer na
    // primeira bateria interna: a declaração é repetida a **cada
    // reconexão** (`Motor::declarar_identidade_de_par`), e uma retirada
    // escrita larga demais derrubaria todo caminho vivo a cada volta.
    let mut motor = motor_de_teste();
    motor.consentimento = CONSENTE_TUDO;
    motor.tarefas_de_par.assistir(
        ScreenId(1),
        tokio::spawn(async { std::future::pending::<()>().await }),
    );

    motor.passar_a_consentir(CONSENTE_TUDO).await;

    assert_eq!(
        motor.tarefas_de_par.caminhos.len(),
        1,
        "redeclarar o mesmo consentimento derrubou um caminho de par vivo"
    );
}

fn endereco_qualquer() -> SocketAddr {
    SocketAddr::from(([192, 168, 1, 10], 4444))
}

fn impressao_qualquer() -> String {
    "a".repeat(64)
}

/// Quem consentiu nas duas metades, com o teto desta versão.
const CONSENTE_TUDO: ConsentimentoDePar = ConsentimentoDePar {
    pares_que_atende: PARES_QUE_ESTA_VERSAO_ATENDE,
    assiste_por_par: true,
};

/// **A fila da conexão que caiu não fala pela que a substitui.**
///
/// # Por que um relato velho é pior do que um relato perdido
///
/// `resultados_do_par` é um canal sem fundo, e o braço que o lê em
/// `Motor::rodar` só existe **quando há cliente**. Durante a bateria não há:
/// tudo o que as tarefas de par mandaram fica parado na fila. Quando
/// `Motor::tentar` põe um cliente novo no lugar do que morreu, o primeiro
/// braço a rodar entrega esses relatos **à conexão nova** — um `ParFalhou`
/// de uma nomeação que morreu com a sessão anterior, pedindo ao servidor
/// que desfaça um caminho que ele acabou de montar para a substituta.
///
/// A segunda metade é a que impede o conserto de virar mudez: o que
/// acontecer **depois** da queda continua tendo de chegar. Esvaziar a fila
/// não pode ser fechá-la.
#[tokio::test]
async fn cair_nao_deixa_a_fila_da_conexao_velha_alcancar_a_substituta() {
    let mut motor = motor_de_teste();

    // O relato da conexão que está morrendo, ainda em voo quando ela morre.
    let _ = motor.resultados_do_par_tx.send(ResultadoDoPar::ParFalhou {
        screen: ScreenId(1),
        motivo: MotivoDeFalhaDePar::ParouDeMandar,
    });

    motor.cair();

    assert!(
        motor.resultados_do_par.try_recv().is_err(),
        "o relato da conexão que caiu continuou na fila: a próxima conexão o receberia como \
             se fosse dela, e mandaria o servidor desfazer um caminho que ele montou depois"
    );

    // E a atividade nova passa: a fila foi esvaziada, não fechada.
    let _ = motor.resultados_do_par_tx.send(ResultadoDoPar::ParFalhou {
        screen: ScreenId(2),
        motivo: MotivoDeFalhaDePar::CaiuNoMeio,
    });
    assert!(
        matches!(
            motor.resultados_do_par.try_recv(),
            Ok(ResultadoDoPar::ParFalhou {
                screen: ScreenId(2),
                ..
            })
        ),
        "esvaziar a fila da conexão velha calou também o que veio depois da queda"
    );
}

/// Espera a condição valer, ou desiste com a mensagem de quem esperava.
///
/// Dormir um tanto e afirmar é o que produz o teste que passa só na máquina
/// de quem o escreveu.
///
/// A paciência é de quem chama, e não uma constante daqui: em
/// [`cair_devolve_a_vaga_de_quem_estava_servindo_um_par`] ela **é** a
/// afirmação — esperar mais que [`PRAZO_DO_PAR`] deixaria o fim natural da
/// tarefa passar por cancelamento.
async fn ate_que<F: FnMut() -> bool>(o_que: &str, paciencia: Duration, mut condicao: F) {
    let fim = Instant::now() + paciencia;
    while Instant::now() < fim {
        if condicao() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("a paciência acabou esperando: {o_que}");
}

/// O ponto de chamada de `par::parar_de_atender` dentro de `servir_um_par`
/// desarma a ponta de quem empresta depois de servir.
///
/// O sinal mora do lado de quem **atende**: pôr alguém pronto para aceitar
/// (`par::atender`) depois do desarme, e só então discar. Armada, o aperto
/// completa; desarmada, o portão recusa o Binding antes de criar estado e
/// quem disca esgota o próprio prazo. Contra `servir_um_par`, o ponto de
/// chamada de produção, e não a função pura.
#[tokio::test(flavor = "multi_thread")]
async fn servir_um_par_desarma_a_ponta_de_quem_empresta_depois_de_servir() {
    let ponta_empresta = ponta_de_sessao_de_teste();
    let onde_empresta = ponta_empresta.ponta().local_addr();
    let impressao_empresta = par::impressao(ponta_empresta.identidade());

    // Quem "assiste": só disca — o papel de `Motor::assistir_por_par`.
    let ponta_assiste = ponta_de_sessao_de_teste();
    let impressao_assiste = par::impressao(ponta_assiste.identidade());

    // O furo de quem empresta vai a um endereço onde ninguém responde: ele
    // só abre o NAT, e não decide nada.
    let buraco_negro = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let onde_buraco_negro = buraco_negro.local_addr().unwrap();

    let servindo = tokio::spawn(servir_um_par(
        ponta_empresta.clone(),
        vec![onde_buraco_negro],
        impressao_assiste.clone(),
    ));

    let ligado = par::ligar(
        &ponta_assiste,
        &[onde_empresta],
        impressao_empresta.clone(),
        Duration::from_secs(3),
    )
    .await
    .expect("a discagem de quem assiste tinha tudo para fechar e não fechou");

    let resultado = servindo.await.expect("a tarefa de servir_um_par");
    assert!(
        resultado.is_some(),
        "servir_um_par não serviu ninguém, e o resto deste teste não prova nada"
    );
    drop(ligado);
    assert!(
        !ponta_empresta.armada(),
        "servir_um_par devolveu com a ponta de quem empresta ainda armada"
    );

    // **O discriminador.** Alguém pronto para aceitar, depois de
    // `servir_um_par` já ter devolvido: se a ponta continuasse armada, é
    // este `atender` quem completaria o aperto.
    let _atendendo = {
        let ponta_empresta = ponta_empresta.clone();
        let impressao_assiste = impressao_assiste.clone();
        tokio::spawn(async move {
            par::atender(
                &ponta_empresta,
                &impressao_assiste,
                Duration::from_secs(3),
                Duration::from_secs(2),
            )
            .await
        })
    };
    let tentou = par::ligar(
        &ponta_assiste,
        &[onde_empresta],
        impressao_empresta,
        Duration::from_millis(800),
    )
    .await;
    assert!(
        matches!(tentou, Err(par::ErroDePar::NaoAlcancou)),
        "a ponta de quem empresta continuou atendendo depois de servir_um_par devolver: \
             {tentou:?}"
    );
}

/// **Duas telas na mesma sala: a segunda não é repassada, e a primeira
/// sobrevive ao fim dela.**
///
/// O cenário é o do §0 do desenho — «numa call de 5 pessoas, 2 querem
/// transmitir a tela» —, e antes deste guarda ele produzia o pior defeito
/// que este repositório sabe nomear: a segunda `abriu()` sobrescrevia a
/// primeira, os quadros das duas entravam intercalados no mesmo fluxo do
/// par, quem recebia rotulava tudo com o `screen` do fluxo, e o primeiro
/// `fechou()` matava o repasse da outra. Duas telas fundidas numa só, sem
/// erro em lugar nenhum.
#[tokio::test]
async fn so_uma_tela_por_vez_e_repassada_e_o_fim_da_outra_nao_a_derruba() {
    let repasse = RepasseDeTela::default();
    let primeira = ScreenId(1);
    let segunda = ScreenId(2);
    let (para_o_par, mut do_par) = mpsc::channel(8);

    repasse.abriu(primeira, b"abertura-da-primeira".to_vec());
    repasse.ligar(para_o_par);

    // A segunda transmissão chega e **não** assume.
    repasse.abriu(segunda, b"abertura-da-segunda".to_vec());
    assert_eq!(
        repasse.abertura_de(primeira),
        Some(b"abertura-da-primeira".to_vec()),
        "a segunda tela assumiu o repasse da primeira"
    );
    assert_eq!(
        repasse.abertura_de(segunda),
        None,
        "o repasse aceitou servir a segunda tela com a abertura de outra"
    );

    // E os quadros dela não entram no fluxo do par.
    repasse.pedaco(segunda, b"quadro-da-segunda".to_vec());

    // O fim da segunda não derruba o repasse da primeira.
    repasse.fechou(segunda);
    repasse.pedaco(primeira, b"quadro-da-primeira".to_vec());

    assert_eq!(
        do_par.try_recv().ok(),
        Some(b"quadro-da-primeira".to_vec()),
        "o fim da segunda tela derrubou o repasse da primeira"
    );
    assert!(
        do_par.try_recv().is_err(),
        "um quadro da segunda tela entrou no fluxo do par da primeira — as duas chegam \
             fundidas a quem assiste"
    );

    // E o fim da primeira, esse sim, encerra o repasse.
    repasse.fechou(primeira);
    assert_eq!(repasse.abertura_de(primeira), None);
}

/// **Sem endereço nenhum para furar, `atender` ainda serve.**
///
/// O furo de quem empresta existe por um efeito só, abrir o mapeamento de
/// NAT desta ponta, e uma lista vazia de endereços (o servidor não soube de
/// nenhum) não pode desistir de servir quem está chegando por outro
/// caminho.
#[tokio::test(flavor = "multi_thread")]
async fn sem_endereco_para_furar_quem_empresta_ainda_atende() {
    let ponta_empresta = ponta_de_sessao_de_teste();
    let onde_empresta = ponta_empresta.ponta().local_addr();
    let impressao_empresta = par::impressao(ponta_empresta.identidade());
    let ponta_assiste = ponta_de_sessao_de_teste();
    let impressao_assiste = par::impressao(ponta_assiste.identidade());

    let servindo = tokio::spawn(servir_um_par(
        ponta_empresta.clone(),
        Vec::new(),
        impressao_assiste,
    ));
    let ligado = par::ligar(
        &ponta_assiste,
        &[onde_empresta],
        impressao_empresta,
        Duration::from_secs(3),
    )
    .await;

    let resultado = servindo.await.expect("a tarefa de servir_um_par");
    assert!(
        resultado.is_some(),
        "sem endereço para furar, quem empresta desistiu de servir alguém que chegava"
    );
    assert!(
        ligado.is_ok(),
        "quem assiste discou para uma ponta que devia estar atendendo e não fechou: {:?}",
        ligado.err()
    );
}

/// A ponta de uma sessão em `127.0.0.1`.
fn ponta_de_sessao_de_teste() -> crate::transporte::PontaDaSessao {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("socket");
    crate::transporte::abrir_ponta(Some(socket)).expect("ponta da sessão")
}

/// **O que o espelho diz vira aviso, e a pausa não fica pelo caminho.**
///
/// O `EspelhoDaTela::Parou` é novo, e o `match` que o traduz tem um braço que
/// devolve `None` (o quadro da câmera): é o lugar em que uma pausa calada
/// caberia sem nada reprovar. A tela e a câmera passam pelo mesmo espelho, e
/// as duas têm de ouvir a pausa — com a origem, que é o que deixa a casca
/// escolher entre «a tela» e «a câmera».
#[test]
fn a_pausa_do_espelho_vira_aviso_com_a_origem_e_o_motivo() {
    use crate::bomba::EspelhoDaTela;
    use crate::tela::MotivoDeParada;
    use seele_proto::screen::ScreenSource;

    for origem in [
        ScreenSource::Monitor,
        ScreenSource::Window,
        ScreenSource::Camera,
    ] {
        for motivo in [MotivoDeParada::SinalCritico, MotivoDeParada::AbaixoDoPiso] {
            match aviso_do_espelho(EspelhoDaTela::Parou(motivo), ScreenId(9), origem, 3) {
                Some(Aviso::TelaParou {
                    tela,
                    origem: dita,
                    motivo: dito,
                }) => assert_eq!((tela, dita, dito), (ScreenId(9), origem, motivo)),
                outro => panic!(
                    "o teto pausou a transmissão ({origem:?}, {motivo:?}) e o espelho disse \
                     {outro:?}: quem transmite não ficaria sabendo que a imagem parou"
                ),
            }
        }
    }
    // A volta é a abertura de sempre, da mesma tela.
    // E leva o nome do codificador que abriu o fluxo, como a bomba o disse.
    match aviso_do_espelho(
        EspelhoDaTela::Abriu {
            largura: 960,
            altura: 540,
            codificador: Some("OpenH264 (software)".to_owned()),
        },
        ScreenId(9),
        ScreenSource::Monitor,
        3,
    ) {
        Some(Aviso::TelaAbriu {
            tela: ScreenId(9),
            leitura: 3,
            codificador,
            ..
        }) => assert_eq!(
            codificador.as_deref(),
            Some("OpenH264 (software)"),
            "o espelho abriu o fluxo e o aviso perdeu o nome do codificador: o rodapé de \
             quem transmite não tem como mostrá-lo"
        ),
        outro => panic!("a volta não é a abertura da mesma tela: {outro:?}"),
    }
    // Sem nome (a câmera, que a janela codifica), o aviso não inventa um.
    assert!(matches!(
        aviso_do_espelho(
            EspelhoDaTela::Abriu {
                largura: 960,
                altura: 540,
                codificador: None,
            },
            ScreenId(9),
            ScreenSource::Camera,
            3,
        ),
        Some(Aviso::TelaAbriu {
            codificador: None,
            ..
        })
    ));
    // E o quadro da câmera segue não voltando: quem a codifica é a janela.
    assert!(aviso_do_espelho(
        EspelhoDaTela::Quadro {
            chave: true,
            bytes: &[0, 0, 0, 1],
        },
        ScreenId(9),
        ScreenSource::Camera,
        3,
    )
    .is_none());
}
