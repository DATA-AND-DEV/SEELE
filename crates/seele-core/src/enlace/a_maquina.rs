#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use super::*;
use seele_proto::maquina::{
    resumo_da_oferta, resumo_em_hex, ComandoOferecido, MotivoDaDispensa, MotivoDaRecusaDeMaquina,
    OfertaDeMaquina, PapelDaMaquina, TunelOferecido,
};

const MOD: &str = "seele/jogo";
const SERVIDOR: &str = "mundo";

fn hash() -> String {
    "ab".repeat(32)
}

fn oferta() -> OfertaDeMaquina {
    OfertaDeMaquina {
        titulo: "Minecraft".into(),
        receita: "minecraft-java".into(),
        versao_da_receita: "1.0.0".into(),
        versao_do_jogo: "1.21.4".into(),
        tuneis: vec![TunelOferecido {
            nome: "mc".into(),
            protocolo: ProtocoloDeTunel::Tcp,
            porta: 25565,
        }],
        artefatos: vec![],
        pack: None,
        origens: vec![],
        dominios_do_jogo: vec![],
        rodar: Some(ComandoOferecido {
            comando: "java".into(),
            argumentos: vec!["-jar".into(), "server.jar".into()],
        }),
        disco_maximo: 1024,
        termos: vec![],
        instalar: None,
        por_sistema: vec![],
    }
}

fn pedido() -> ServerMessage {
    ServerMessage::PedidoDeMaquina {
        mod_id: MOD.into(),
        hash: hash(),
        servidor: SERVIDOR.into(),
        pedido_por: PersonId(3),
        oferta: Box::new(oferta()),
        papel: PapelDaMaquina::Reserva,
    }
}

/// O servidor de mentira: `manda` escreve no fio, `ouvidos` é o que o
/// cliente escreveu depois do aperto de mão.
struct Servidor {
    manda: mpsc::UnboundedSender<ServerMessage>,
    ouvidos: mpsc::UnboundedReceiver<ClientMessage>,
}

impl Servidor {
    /// A próxima mensagem da máquina que o cliente escreveu, pulando o
    /// resto (pings, declarações), ou `None` se nada veio em `prazo`.
    async fn ouvir_da_maquina(&mut self, prazo: Duration) -> Option<ClientMessage> {
        tokio::time::timeout(prazo, async {
            loop {
                let mensagem = self.ouvidos.recv().await?;
                if matches!(
                    mensagem,
                    ClientMessage::AceiteDeMaquina { .. }
                        | ClientMessage::RecusaDeMaquina { .. }
                        | ClientMessage::MaquinaParada { .. }
                        | ClientMessage::PedidoDaMaquina { .. }
                ) {
                    return Some(mensagem);
                }
            }
        })
        .await
        .ok()
        .flatten()
    }
}

async fn enlace_com_servidor() -> (Enlace, Servidor) {
    let certificado = crate::transporte::CertificadoDtls::efemero().expect("certificado");
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("socket");
    let endereco = socket.local_addr().expect("endereço");
    let escuta = crate::transporte::Ponta::de_socket(
        socket,
        Some(crate::transporte::ConfigDoAnfitriao::novo(certificado)),
    )
    .expect("ponta");
    let (manda, mut a_mandar) = mpsc::unbounded_channel::<ServerMessage>();
    let (ouviu, ouvidos) = mpsc::unbounded_channel::<ClientMessage>();
    tokio::spawn(async move {
        let Some(conexao) = escuta.aceitar().await else {
            return;
        };
        let Some((mut envio, mut recebe)) = conexao.controle() else {
            return;
        };
        // `Hello` e `Response` ignorados: não há assinatura a conferir.
        let _ = crate::frame::read::<ClientMessage>(&mut recebe).await;
        let _ =
            crate::frame::write(&mut envio, &ServerMessage::Challenge { nonce: vec![0; 32] }).await;
        let _ = crate::frame::read::<ClientMessage>(&mut recebe).await;
        let _ = crate::frame::write(
            &mut envio,
            &ServerMessage::Session {
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
        tokio::spawn(async move {
            while let Ok(mensagem) = crate::frame::read::<ClientMessage>(&mut recebe).await {
                if ouviu.send(mensagem).is_err() {
                    break;
                }
            }
        });
        while let Some(mensagem) = a_mandar.recv().await {
            if crate::frame::write(&mut envio, &mensagem).await.is_err() {
                break;
            }
        }
        drop((conexao, escuta));
    });

    let ponta = crate::transporte::abrir_ponta(None).expect("ponta");
    let conexao = crate::client::discar(
        ponta.ponta(),
        vec![endereco],
        None,
        seele_proto::transport::HANDSHAKE_TIMEOUT,
    )
    .await
    .expect("discar");
    let destino = Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: "pessoa".into(),
        segredo: None,
        esperada: None,
        o_pino_prova_o_servidor: true,
        aceito: None,
    };
    let enlace = Enlace::conectar_sobre(
        ponta,
        conexao,
        None,
        destino,
        SigningKey::from_bytes(&[5; 32]),
        Arc::new(crate::tofu::MemoryPinStore::new()),
        None,
    )
    .await
    .expect("o aperto de mão com o servidor de mentira");
    (enlace, Servidor { manda, ouvidos })
}

/// Os avisos da máquina que chegaram em `prazo`, sem os de estado.
async fn avisos_da_maquina(enlace: &mut Enlace, prazo: Duration) -> Vec<Aviso> {
    let mut vistos = Vec::new();
    let fim = tokio::time::Instant::now() + prazo;
    while let Ok(aviso) = tokio::time::timeout_at(fim, enlace.proximo()).await {
        match aviso {
            Aviso::PedidoDeMaquina { .. }
            | Aviso::AvisoParaMaquina { .. }
            | Aviso::MaquinaDispensada { .. } => vistos.push(aviso),
            Aviso::Mensagem(mensagem)
                if matches!(
                    *mensagem,
                    ServerMessage::PedidoDeMaquina { .. }
                        | ServerMessage::AvisoParaMaquina { .. }
                        | ServerMessage::MaquinaDispensada { .. }
                        | ServerMessage::ModReply { .. }
                ) =>
            {
                vistos.push(Aviso::Mensagem(mensagem));
            }
            _ => {}
        }
    }
    vistos
}

/// Desvio 5: um enlace sem tela — o CLI, um teste, qualquer uso do núcleo
/// que não seja o app — não tem como mostrar a oferta, e diz isso ao
/// servidor na hora em vez de deixá-lo esperando. A casca não recebe nada.
#[tokio::test(flavor = "multi_thread")]
async fn sem_tela_o_pedido_e_recusado_sozinho() {
    let (mut enlace, mut servidor) = enlace_com_servidor().await;
    servidor.manda.send(pedido()).unwrap();

    let ouvido = servidor.ouvir_da_maquina(Duration::from_secs(5)).await;
    assert_eq!(
        ouvido,
        Some(ClientMessage::RecusaDeMaquina {
            mod_id: MOD.into(),
            servidor: SERVIDOR.into(),
            motivo: MotivoDaRecusaDeMaquina::SemTela,
        }),
        "o enlace sem tela não recusou o pedido sozinho"
    );
    let avisos = avisos_da_maquina(&mut enlace, Duration::from_millis(300)).await;
    assert!(
        avisos.is_empty(),
        "o pedido recusado sozinho chegou à casca mesmo assim: {avisos:?}"
    );
}

/// Com tela, o pedido vira aviso uma vez só, com o resumo já calculado — e
/// é esse resumo que volta no aceite, sem ninguém o recalcular.
#[tokio::test(flavor = "multi_thread")]
async fn com_tela_o_pedido_vira_aviso_com_o_resumo() {
    let (mut enlace, mut servidor) = enlace_com_servidor().await;
    enlace.aceitar_pedidos_de_maquina(true);
    // O mesmo pedido duas vezes na mesma conexão (o servidor pode
    // mandá-lo de novo enquanto espera): a casca o vê uma vez.
    servidor.manda.send(pedido()).unwrap();
    servidor.manda.send(pedido()).unwrap();

    let avisos = avisos_da_maquina(&mut enlace, Duration::from_secs(2)).await;
    let esperado = resumo_em_hex(&resumo_da_oferta(
        MOD,
        &hash(),
        SERVIDOR,
        PapelDaMaquina::Reserva,
        &oferta(),
    ));
    let [Aviso::PedidoDeMaquina {
        mod_id,
        hash: hash_do_aviso,
        servidor: servidor_do_aviso,
        pedido_por,
        oferta: oferta_do_aviso,
        papel,
        resumo,
    }] = avisos.as_slice()
    else {
        panic!("esperava um só aviso de pedido, vieram: {avisos:?}");
    };
    assert_eq!(mod_id, MOD);
    assert_eq!(*hash_do_aviso, hash());
    assert_eq!(servidor_do_aviso, SERVIDOR);
    assert_eq!(*pedido_por, PersonId(3));
    assert_eq!(**oferta_do_aviso, oferta());
    assert_eq!(
        *papel,
        PapelDaMaquina::Reserva,
        "o papel do fio não chegou à casca"
    );
    assert_eq!(*resumo, esperado);
    assert_eq!(
        servidor.ouvir_da_maquina(Duration::from_millis(300)).await,
        None,
        "com tela, o enlace respondeu sozinho"
    );

    enlace.aceitar_maquina(MOD, SERVIDOR, resumo).await.unwrap();
    assert_eq!(
        servidor.ouvir_da_maquina(Duration::from_secs(5)).await,
        Some(ClientMessage::AceiteDeMaquina {
            mod_id: MOD.into(),
            servidor: SERVIDOR.into(),
            resumo: esperado,
        })
    );
}

/// O pedido da máquina volta inteiro, montado das partes do `ModReply`
/// com o número que ele levou; e quem não é a máquina fica sabendo na
/// hora, em vez de esperar o prazo.
#[tokio::test(flavor = "multi_thread")]
async fn o_pedido_da_maquina_volta_inteiro_e_o_sem_aceite_e_dito() {
    let (mut enlace, mut servidor) = enlace_com_servidor().await;

    let (resultado, ouvido) = tokio::join!(
        enlace.pedido_da_maquina(MOD, SERVIDOR, "{\"oi\":1}".into()),
        async {
            let ouvido = servidor.ouvir_da_maquina(Duration::from_secs(5)).await;
            let Some(ClientMessage::PedidoDaMaquina { request, .. }) = ouvido.clone() else {
                panic!("o pedido não chegou ao servidor: {ouvido:?}");
            };
            for (part, payload) in ["{\"ok\":", "true}"].into_iter().enumerate() {
                servidor
                    .manda
                    .send(ServerMessage::ModReply {
                        request,
                        part: u32::try_from(part).unwrap(),
                        total: 2,
                        payload: payload.into(),
                    })
                    .unwrap();
            }
            ouvido
        }
    );
    assert_eq!(resultado, Ok("{\"ok\":true}".to_owned()));
    let Some(ClientMessage::PedidoDaMaquina {
        mod_id,
        servidor: servidor_ouvido,
        payload,
        ..
    }) = ouvido
    else {
        unreachable!()
    };
    assert_eq!(
        (mod_id.as_str(), servidor_ouvido.as_str(), payload.as_str()),
        (MOD, SERVIDOR, "{\"oi\":1}")
    );

    let (resultado, ()) = tokio::join!(
        enlace.pedido_da_maquina(MOD, SERVIDOR, "{}".into()),
        async {
            servidor
                .ouvir_da_maquina(Duration::from_secs(5))
                .await
                .expect("o pedido");
            servidor
                .manda
                .send(ServerMessage::MaquinaDispensada {
                    mod_id: MOD.into(),
                    servidor: SERVIDOR.into(),
                    motivo: MotivoDaDispensa::SemAceite,
                })
                .unwrap();
        }
    );
    assert_eq!(resultado, Err(FalhaDoPedidoDaMaquina::SemAceite));
    // A resposta montada não vazou para a casca como `ModReply`, e a
    // dispensa chegou como aviso próprio.
    let avisos = avisos_da_maquina(&mut enlace, Duration::from_millis(300)).await;
    assert!(
        matches!(
            avisos.as_slice(),
            [Aviso::MaquinaDispensada {
                motivo: MotivoDaDispensa::SemAceite,
                ..
            }]
        ),
        "{avisos:?}"
    );
}
