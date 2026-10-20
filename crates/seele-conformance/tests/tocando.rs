//! **Quem liga e ouve `Tocando` espera quem recebe decidir** (tarefa 16 da F7,
//! o 6e do Plano 5; spec de 23/09, «Do lado de quem liga»).
//!
//! A sala pessoal manda `Tocando { prazo_s }` depois da prova da chave e espera
//! quem recebe atender, recusar ou deixar o prazo vencer
//! (`seele-server/src/session.rs`, `tocar`). O cliente não conhecia o
//! `Tocando`: lia a resposta do `Response` dentro dos 10 s do aperto de mão, e
//! um quadro que não fosse `Session` virava violação de protocolo. Com o toque,
//! o aperto ganha um segundo prazo, o do toque e mais uma folga.
//!
//! As provas sobem a sala de verdade (o `Daemon` com `Sala`), ligam pelo código
//! pelo `Enlace`, e decidem pela recepção da sala, como o app de quem recebe
//! decide.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use seele_core::chegada::{Chegada, Etapa};
use seele_core::confere_codigo::chave_do_pino;
use seele_core::enlace::{Destino, Enlace, Esperada};
use seele_core::{ConnectError, MemoryPinStore, PinStore};
use seele_proto::codigo::Codigo;
use seele_proto::control::DisconnectReason;
use seele_proto::transport::{HANDSHAKE_TIMEOUT, NOME_DA_SALA};
use seele_server::campainha::IdDoToque;
use seele_server::recepcao::{Aviso, Recepcao, Sala};
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// A semente de quem recebe.
const DONO: u8 = 1;

/// A semente de quem liga.
const QUEM_LIGA: u8 = 2;

fn chave(semente: u8) -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[semente; 32])
}

fn publica(semente: u8) -> [u8; 32] {
    chave(semente).verifying_key().to_bytes()
}

fn pins() -> Arc<dyn PinStore> {
    Arc::new(MemoryPinStore::new())
}

/// A sala de `DONO`, em memória, com o prazo do toque dado.
async fn sala(prazo: Duration) -> Arc<Daemon> {
    let config = ServerConfig {
        identidade: Some(seele_server::tls::Identity::da_semente(&[DONO; 32]).unwrap()),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        voice_room_limit: 6,
        sala: Some(Sala {
            dono: publica(DONO),
            prazo_do_toque: prazo,
        }),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.expect("a sala não subiu"));
    let rodando = Arc::clone(&servidor);
    tokio::spawn(async move { rodando.run().await });
    servidor
}

/// A recepção da sala: por onde o app de quem recebe atende e recusa.
fn recepcao(sala: &Daemon) -> Arc<Recepcao> {
    sala.server().recepcao.clone().expect("a sala tem recepção")
}

/// Ligar pelo código de `DONO`, para `endereco`.
fn destino(endereco: SocketAddr) -> Destino {
    let codigo = Codigo::da_chave(&publica(DONO));
    Destino {
        servidor: endereco,
        nome_tls: NOME_DA_SALA.to_owned(),
        chave_do_pin: chave_do_pino(&codigo),
        apelido: "quem-liga".into(),
        segredo: None,
        esperada: Some(Esperada::Codigo(codigo)),
        o_pino_prova_o_servidor: false,
        aceito: None,
    }
}

/// Espera o `Tocou` de `semente` na recepção, e devolve o toque.
async fn tocou(avisos: &mut tokio::sync::broadcast::Receiver<Aviso>, semente: u8) -> IdDoToque {
    loop {
        let aviso = tokio::time::timeout(Duration::from_secs(5), avisos.recv())
            .await
            .expect("quem recebe não ficou sabendo que tocava")
            .unwrap();
        if let Aviso::Tocou { toque, codigo, .. } = aviso {
            if codigo == Codigo::da_chave(&publica(semente)) {
                return toque;
            }
        }
    }
}

/// O que a ligação deu, sem o `Enlace` (que não é `Debug`).
fn desfecho(resultado: &Result<Enlace, ConnectError>) -> Result<(), ConnectError> {
    resultado.as_ref().map(|_| ()).map_err(Clone::clone)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quem_ouve_tocando_nao_cai_nos_10_s_do_aperto_de_mao() {
    let _vaga = vaga::minha();
    let sala = sala(Duration::from_secs(30)).await;
    let recepcao = recepcao(&sala);
    let mut avisos = recepcao.assinar();
    let ligando = tokio::spawn(Enlace::conectar(
        destino(sala.local_addr().unwrap()),
        chave(QUEM_LIGA),
        pins(),
    ));
    let toque = tocou(&mut avisos, QUEM_LIGA).await;

    // Quem recebe demora mais que o aperto de mão inteiro para decidir.
    tokio::time::sleep(HANDSHAKE_TIMEOUT + Duration::from_secs(1)).await;
    if ligando.is_finished() {
        let resultado = ligando.await.unwrap();
        panic!(
            "quem liga desistiu antes de quem recebe decidir, com o toque ainda no prazo: {:?}",
            desfecho(&resultado)
        );
    }

    assert!(recepcao.atender(toque), "o toque não estava mais tocando");
    let resultado = tokio::time::timeout(Duration::from_secs(5), ligando)
        .await
        .expect("quem foi atendido não entrou em 5 s")
        .unwrap();
    assert!(
        resultado.is_ok(),
        "quem foi atendido não entrou: {:?}",
        desfecho(&resultado)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn o_prazo_do_toque_vencido_chega_como_ninguem_atendeu() {
    let _vaga = vaga::minha();
    let sala = sala(Duration::from_secs(2)).await;
    let resultado = tokio::time::timeout(
        Duration::from_secs(15),
        Enlace::conectar(
            destino(sala.local_addr().unwrap()),
            chave(QUEM_LIGA),
            pins(),
        ),
    )
    .await
    .expect("a ligação não terminou depois do prazo do toque");
    assert!(
        matches!(
            desfecho(&resultado),
            Err(ConnectError::Refused {
                reason: DisconnectReason::NinguemAtendeu
            })
        ),
        "o prazo do toque venceu, e quem liga não leu «ninguém atendeu»: {:?}",
        desfecho(&resultado)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recusa_chega_como_chamada_recusada() {
    let _vaga = vaga::minha();
    let sala = sala(Duration::from_secs(30)).await;
    let recepcao = recepcao(&sala);
    let mut avisos = recepcao.assinar();
    let ligando = tokio::spawn(Enlace::conectar(
        destino(sala.local_addr().unwrap()),
        chave(QUEM_LIGA),
        pins(),
    ));
    assert!(recepcao.recusar(tocou(&mut avisos, QUEM_LIGA).await));
    let resultado = tokio::time::timeout(Duration::from_secs(5), ligando)
        .await
        .expect("a recusa não chegou a quem ligava em 5 s")
        .unwrap();
    assert!(
        matches!(
            desfecho(&resultado),
            Err(ConnectError::Refused {
                reason: DisconnectReason::ChamadaRecusada
            })
        ),
        "a recusa não chegou a quem ligava como «chamada recusada»: {:?}",
        desfecho(&resultado)
    );
}

/// **Dois candidatos para a mesma sala tocam uma vez só** (tarefa 16 da F7).
///
/// A mesma pessoa por outro candidato chega à sala como outro `Tocou`, e o
/// toque de antes fecha calado (`recepcao::Aviso`). O segundo candidato sai 250
/// ms depois do primeiro; quem tocou primeiro tem de parar a corrida antes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dois_candidatos_para_a_mesma_sala_tocam_uma_vez_so() {
    let _vaga = vaga::minha();
    let sala = sala(Duration::from_secs(30)).await;
    let recepcao = recepcao(&sala);
    let mut avisos = recepcao.assinar();
    let onde = sala.local_addr().unwrap();
    let ligando = tokio::spawn(Enlace::conectar_entre(
        vec![destino(onde), destino(onde)],
        chave(QUEM_LIGA),
        pins(),
    ));
    let toque = tocou(&mut avisos, QUEM_LIGA).await;

    // Um segundo inteiro cobre a vez do segundo candidato, com folga.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let depois: Vec<Aviso> = std::iter::from_fn(|| avisos.try_recv().ok()).collect();
    assert!(
        !depois
            .iter()
            .any(|aviso| matches!(aviso, Aviso::Tocou { .. })),
        "a mesma pessoa tocou de novo por outro candidato: {depois:?}"
    );

    assert!(
        recepcao.atender(toque),
        "o primeiro toque não estava mais tocando: {depois:?}"
    );
    let resultado = tokio::time::timeout(Duration::from_secs(5), ligando)
        .await
        .expect("quem foi atendido não entrou em 5 s")
        .unwrap();
    assert!(
        resultado.is_ok(),
        "quem foi atendido não entrou: {:?}",
        desfecho(&resultado)
    );
}

/// **Quem toca numa corrida não cai no prazo do candidato** (tarefa 16 da F7).
///
/// O prazo de cada candidato (4 s numa rede local) existe para soltar quem
/// nunca respondeu. O primeiro aqui é um socket que não responde, e o segundo é
/// a sala: ela toca, e quem recebe demora mais que os 4 s para atender.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quem_toca_numa_corrida_nao_cai_no_prazo_do_candidato() {
    let _vaga = vaga::minha();
    let sala = sala(Duration::from_secs(30)).await;
    let recepcao = recepcao(&sala);
    let mut avisos = recepcao.assinar();
    // Um endereço que recebe e nunca responde.
    let mudo = std::net::UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    let chegada = Chegada::nova(
        vec![
            destino(mudo.local_addr().unwrap()),
            destino(sala.local_addr().unwrap()),
        ],
        None,
    );
    let ligando = tokio::spawn(chegada.chegar(chave(QUEM_LIGA), pins()));
    let toque = tocou(&mut avisos, QUEM_LIGA).await;

    tokio::time::sleep(Duration::from_secs(5)).await;
    if ligando.is_finished() {
        let resultado = ligando.await.unwrap();
        panic!(
            "quem tocava desistiu no prazo do candidato, com o toque ainda no prazo: {:?}",
            resultado.err().map(|frustrada| frustrada.motivo().clone())
        );
    }

    assert!(recepcao.atender(toque), "o toque não estava mais tocando");
    let chegado = tokio::time::timeout(Duration::from_secs(5), ligando)
        .await
        .expect("quem foi atendido não entrou em 5 s")
        .unwrap()
        .unwrap_or_else(|frustrada| {
            panic!("quem foi atendido não entrou: {:?}", frustrada.motivo())
        });
    // E a trilha diz quem tocou: o segundo, e não o mudo.
    let etapas: Vec<&Etapa> = chegado.trilha.iter().map(|passo| &passo.etapa).collect();
    assert!(
        matches!(
            etapas.as_slice(),
            [
                Etapa::Parada { .. },
                Etapa::Tentando { candidato: 0, .. },
                Etapa::Tentando { candidato: 1, .. },
                Etapa::Tocando { candidato: 1, .. },
                Etapa::Dentro,
            ]
        ),
        "a trilha da corrida não diz quem tocou: {etapas:?}"
    );
    drop(mudo);
}

/// **Quem acompanha a chegada vê `Tocando`** (tarefa 16 da F7): a etapa sai
/// quando a sala manda o toque, antes de alguém atender, e a trilha de uma
/// ligação atendida é `Parada → Tentando → Tocando → Dentro`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quem_acompanha_a_chegada_ve_tocando_antes_de_atenderem() {
    let _vaga = vaga::minha();
    let sala = sala(Duration::from_secs(30)).await;
    let recepcao = recepcao(&sala);
    let mut avisos = recepcao.assinar();
    let chegada = Chegada::nova(vec![destino(sala.local_addr().unwrap())], None);
    let mut etapas = chegada.acompanhar();
    let chegando = tokio::spawn(chegada.chegar(chave(QUEM_LIGA), pins()));

    let toque = tocou(&mut avisos, QUEM_LIGA).await;
    let vista = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Etapa::Tocando { prazo_s, .. } = &*etapas.borrow_and_update() {
                return *prazo_s;
            }
            if etapas.changed().await.is_err() {
                return 0;
            }
        }
    })
    .await
    .expect("a sala tocou, e quem acompanha a chegada não viu");
    assert_eq!(vista, 30, "o toque chegou à chegada com outro prazo");

    assert!(recepcao.atender(toque), "o toque não estava mais tocando");
    let chegado = tokio::time::timeout(Duration::from_secs(5), chegando)
        .await
        .expect("quem foi atendido não entrou em 5 s")
        .unwrap()
        .unwrap_or_else(|frustrada| {
            panic!("quem foi atendido não entrou: {:?}", frustrada.motivo())
        });
    let nomes: Vec<&str> = chegado
        .trilha
        .iter()
        .map(|passo| passo.etapa.nome())
        .collect();
    assert_eq!(nomes, ["Parada", "Tentando", "Tocando", "Dentro"]);
}

/// Cancela a ligação no instante em que vê `Tocando`, como o CANCELAR da tela
/// faria de outra linha.
struct CancelaAoTocar {
    cancelamento: Arc<seele_ffi::Cancelamento>,
}

impl seele_ffi::EventListener for CancelaAoTocar {
    fn on_event(&self, event: seele_ffi::Event) {
        if let seele_ffi::Event::ConnectStageChanged {
            stage: seele_ffi::ConnectStage::Tocando { .. },
        } = event
        {
            self.cancelamento.cancelar_ligacao();
        }
    }
}

/// **`cancelar_ligacao` fecha o QUIC, e a sala vê a ligação cancelada**
/// (tarefa 16 da F7). Pela FFI, como o app vai chamar: o `connect` espera numa
/// linha, e o cancelamento é usado de outra.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelar_a_ligacao_fecha_o_quic_e_a_sala_ve_cancelada() {
    let _vaga = vaga::minha();
    let sala = sala(Duration::from_secs(30)).await;
    let recepcao = recepcao(&sala);
    let mut avisos = recepcao.assinar();
    let casa = tempfile::tempdir().unwrap();
    std::fs::write(casa.path().join("identity.key"), [QUEM_LIGA; 32]).unwrap();
    let config = seele_ffi::ConnectConfig {
        server: sala.local_addr().unwrap().to_string(),
        alternate_servers: Vec::new(),
        nickname: "quem-liga".into(),
        codigo_esperado: Some(Codigo::da_chave(&publica(DONO))),
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // Não há placa de som numa máquina de integração contínua.
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    let cancelamento = seele_ffi::Cancelamento::novo();
    let olhos: Arc<dyn seele_ffi::EventListener> = Arc::new(CancelaAoTocar {
        cancelamento: Arc::clone(&cancelamento),
    });
    let ligando = tokio::task::spawn_blocking(move || {
        seele_ffi::Connection::connect_cancelavel(config, Some(olhos), cancelamento)
    });

    let toque = tocou(&mut avisos, QUEM_LIGA).await;
    let desfecho = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match avisos.recv().await {
                Ok(Aviso::ParouDeTocar {
                    toque: qual,
                    desfecho,
                    ..
                }) if qual == toque => return desfecho,
                Ok(_) => {}
                Err(erro) => panic!("a recepção calou: {erro}"),
            }
        }
    })
    .await
    .expect("a sala não viu a ligação parar em 3 s depois do CANCELAR: o QUIC não fechou");
    assert_eq!(
        desfecho,
        seele_server::campainha::Desfecho::Cancelada,
        "a sala viu a ligação parar por outro motivo"
    );

    let resultado = tokio::time::timeout(Duration::from_secs(5), ligando)
        .await
        .expect("o connect cancelado não voltou em 5 s")
        .unwrap();
    let Err(falha) = resultado else {
        panic!("a ligação cancelada entrou");
    };
    assert_eq!(falha.error, seele_ffi::ConnectionError::Cancelada);
    assert_eq!(
        falha.trail.last().map(|passo| &passo.etapa),
        Some(&seele_ffi::ConnectStage::Cancelada),
        "a trilha de quem cancelou não acaba em Cancelada: {:?}",
        falha.trail
    );
}
