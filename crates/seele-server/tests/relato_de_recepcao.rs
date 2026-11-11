//! **O relato de recepção, do fio até o sinal anunciado** (protocolo 13).
//!
//! Quem está calado e sem vídeo não tem perda que o servidor meça; o cliente
//! relata a que ele mesmo mede da voz que ouve (`ClientMessage::RelatoDeRecepcao`)
//! e o servidor a põe na conta do sinal **daquela** pessoa. Este arquivo prova
//! a metade do servidor com um par cru em laço local, que escreve o relato à
//! mão e lê o `PersonState` que volta:
//!
//! - um relato de perda derruba o sinal de quem está calado, e o sinal volta
//!   quando o relato cessa (a validade de `seele_server::sinal`);
//! - um relato fora da faixa é ignorado e a sessão segue, lendo o que vem
//!   depois;
//! - o relato de uma pessoa não mexe no sinal de outra;
//! - mil relatos de uma vez são limitados como qualquer quadro de controle.
//!
//! O relato **vindo do cliente de verdade**, com perda de verdade no caminho,
//! está em `seele-conformance/tests/sinal_de_quem_so_ouve.rs`: o ADR 0002
//! proíbe este crate de ver o cliente. A conta em si, sem rede, está em
//! `seele_server::sinal`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{AlertReason, ClientMessage, DisconnectReason, ServerMessage};
use seele_proto::ids::PersonId;
use seele_proto::signal::{Recepcao, INTERVALO_DO_RELATO_MS, NOMINAL_FLOOR};
use seele_server::persistence::Location;
use seele_server::sinal::VALIDADE_DO_RELATO;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

/// O compasso de um cliente de verdade.
const COMPASSO: Duration = Duration::from_millis(INTERVALO_DO_RELATO_MS);

/// Quanto cada fase observa. O sinal sai uma vez por segundo, a primeira
/// medida do ICE leva até dois, e a 10% de perda a suavização cruza o piso
/// nominal no sexto anúncio: doze dão folga para uma máquina carregada (a
/// mesma conta de `sinal_segue_a_rede.rs`).
const OBSERVACAO: Duration = Duration::from_secs(12);

async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

struct Par {
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    eu: PersonId,
}

/// Disca o servidor e faz o aperto de mão à mão. Fica **fora de toda sala e
/// calado**: o caso de quem o servidor não mede perda nenhuma.
async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    // A ponta é dona do socket: tem de viver enquanto a conexão for usada.
    std::mem::forget(ponta);
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&comum::prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session { person, .. } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    // A conexão vive enquanto o `ctl` viver; o teste só usa o `ctl`.
    std::mem::forget(conexao);
    Ok(Par {
        envio,
        recebe,
        eu: person,
    })
}

fn relato(perda: Option<f32>, jitter_ms: Option<f32>) -> ClientMessage {
    ClientMessage::RelatoDeRecepcao(Recepcao { perda, jitter_ms })
}

/// Lê o `ctl` por `quanto`, mandando `relatar()` a cada [`COMPASSO`] (quando
/// ele devolve uma mensagem), e devolve os sinais que o servidor anunciou
/// **sobre `de_quem`**, na ordem. Erra se o `ctl` fechar: a sessão tem de
/// seguir de pé.
async fn observar(
    par: &mut Par,
    de_quem: PersonId,
    quanto: Duration,
    mut relatar: impl FnMut() -> Option<ClientMessage>,
) -> Result<Vec<u8>> {
    let fim = tokio::time::Instant::now() + quanto;
    let mut compasso = tokio::time::interval(COMPASSO);
    let mut sinais = Vec::new();
    while tokio::time::Instant::now() < fim {
        tokio::select! {
            _ = compasso.tick() => {
                if let Some(mensagem) = relatar() {
                    frame::write(&mut par.envio, &mensagem).await?;
                }
            }
            lido = tokio::time::timeout(
                Duration::from_millis(500),
                frame::read::<ServerMessage>(&mut par.recebe),
            ) => match lido {
                Ok(Ok(ServerMessage::PersonState(estado))) if estado.person == de_quem => {
                    sinais.push(estado.signal);
                }
                Ok(Ok(ServerMessage::Disconnecting { reason })) => {
                    anyhow::bail!("o servidor encerrou a sessão: {reason:?}");
                }
                Ok(Ok(_)) | Err(_) => {}
                Ok(Err(erro)) => anyhow::bail!("o ctl fechou: {erro}"),
            }
        }
    }
    Ok(sinais)
}

/// O último terço das amostras: o regime, depois de a média móvel assentar.
fn regime(sinais: &[u8]) -> &[u8] {
    sinais.get(sinais.len() * 2 / 3..).unwrap_or(sinais)
}

/// **O buraco, fechado.** Calada, sem vídeo e num caminho limpo, a pessoa
/// fica com o sinal alto; relatando 10% de perda no que ouve, o sinal dela
/// cai da faixa nominal; quando o relato cessa, passa a validade e o sinal
/// volta.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_relato_de_perda_derruba_o_sinal_de_quem_esta_calado_e_some_quando_cessa() -> Result<()> {
    let (endereco, servidor) = server().await?;
    let mut par = abrir(endereco, 21).await?;
    let eu = par.eu;

    let antes = observar(&mut par, eu, Duration::from_secs(6), || None).await?;
    assert!(antes.len() >= 3, "poucos anúncios: {antes:?}");
    assert!(
        regime(&antes).iter().all(|s| *s >= 95),
        "calada, num caminho limpo e sem relato, o sinal não ficou alto: {antes:?}"
    );

    let relatando = observar(&mut par, eu, OBSERVACAO, || relato(Some(0.10), None).into()).await?;
    assert!(relatando.len() >= 5, "poucos anúncios: {relatando:?}");
    assert!(
        regime(&relatando).iter().all(|s| *s < NOMINAL_FLOOR),
        "10% de perda relatada e o sinal de quem está calado continuou nominal: {relatando:?}"
    );

    // O relato cessa. A validade passa, e a média volta: de 70, cruza os 90
    // no quinto anúncio depois de a perda sair da conta. Vinte segundos além
    // da validade deixam o último terço começar no décimo, com folga para uma
    // máquina carregada.
    let depois = observar(
        &mut par,
        eu,
        VALIDADE_DO_RELATO + Duration::from_secs(20),
        || None,
    )
    .await?;
    assert!(depois.len() >= 8, "poucos anúncios: {depois:?}");
    assert!(
        regime(&depois).iter().all(|s| *s >= 90),
        "o relato cessou e a perda relatada continuou valendo: {depois:?}"
    );

    println!("sem relato {antes:?}; relatando 10% {relatando:?}; depois de cessar {depois:?}");
    servidor.shutdown();
    Ok(())
}

/// **Fora da faixa é ignorado, e a sessão segue.** Nenhum destes relatos
/// derruba o `ctl` nem mexe no sinal; o relato bom que vem depois é lido —
/// o que prova que a sessão continuou lendo, e não só que ficou aberta.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn um_relato_fora_da_faixa_e_ignorado_e_a_sessao_segue() -> Result<()> {
    let (endereco, servidor) = server().await?;
    let mut par = abrir(endereco, 22).await?;
    let eu = par.eu;

    let tortos = [
        relato(Some(f32::NAN), Some(1.0)),
        relato(Some(7.0), None),
        relato(Some(-0.5), None),
        relato(Some(0.0), Some(-1.0)),
        relato(Some(0.0), Some(f32::INFINITY)),
        relato(None, Some(1.0e9)),
        relato(None, None),
        relato(Some(f32::NEG_INFINITY), Some(f32::NAN)),
    ];
    let mut vez = tortos.iter().cycle();
    let com_tortos = observar(&mut par, eu, Duration::from_secs(9), || vez.next().cloned()).await?;
    assert!(com_tortos.len() >= 4, "poucos anúncios: {com_tortos:?}");
    assert!(
        regime(&com_tortos).iter().all(|s| *s >= 95),
        "um relato fora da faixa entrou na conta do sinal: {com_tortos:?}"
    );

    let com_o_bom = observar(&mut par, eu, OBSERVACAO, || relato(Some(0.10), None).into()).await?;
    assert!(
        regime(&com_o_bom).iter().all(|s| *s < NOMINAL_FLOOR),
        "depois dos relatos tortos a sessão deixou de ler os bons: {com_o_bom:?}"
    );

    println!("relatos tortos {com_tortos:?}; depois, o bom {com_o_bom:?}");
    servidor.shutdown();
    Ok(())
}

/// **O relato é de quem manda sobre si.** Uma pessoa relata perda total e
/// jitter no teto; o sinal **dela** vai ao chão, e o da outra, que não
/// relatou nada, fica onde estava.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_relato_de_uma_pessoa_nao_mexe_no_sinal_de_outra() -> Result<()> {
    let (endereco, servidor) = server().await?;
    let mut quem_relata = abrir(endereco, 23).await?;
    let mut a_outra = abrir(endereco, 24).await?;
    let (relatora, outra) = (quem_relata.eu, a_outra.eu);

    let relatando = tokio::spawn(async move {
        observar(&mut quem_relata, relatora, OBSERVACAO, || {
            relato(Some(1.0), Some(seele_proto::signal::JITTER_FULL_MS)).into()
        })
        .await
    });
    let da_outra = observar(&mut a_outra, outra, OBSERVACAO, || None).await?;
    let de_quem_relata = relatando.await??;

    assert!(
        regime(&de_quem_relata).iter().all(|s| *s <= 50),
        "perda total e jitter no teto relatados, e o sinal de quem relatou em {de_quem_relata:?}"
    );
    assert!(da_outra.len() >= 5, "poucos anúncios: {da_outra:?}");
    assert!(
        regime(&da_outra).iter().all(|s| *s >= 95),
        "o relato de uma pessoa mexeu no sinal de outra: {da_outra:?}"
    );

    println!("quem relatou {de_quem_relata:?}; a outra {da_outra:?}");
    servidor.shutdown();
    Ok(())
}

/// **Mil relatos de uma vez não custam mais que o limitador deixa.** O
/// relato é um quadro de controle como os outros: passada a rajada vem o
/// aviso, depois o descarte, e por fim a despedida por excesso — e o servidor
/// segue atendendo quem chega.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mil_relatos_de_uma_vez_sao_limitados_como_qualquer_quadro() -> Result<()> {
    let (endereco, servidor) = server().await?;
    let mut par = abrir(endereco, 25).await?;

    for _ in 0..1000 {
        if frame::write(&mut par.envio, &relato(Some(1.0), Some(100.0)))
            .await
            .is_err()
        {
            // O servidor já fechou o `ctl`: é o fim esperado.
            break;
        }
    }

    let mut avisado = false;
    let mut despedido = false;
    let prazo = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < prazo {
        match tokio::time::timeout(
            Duration::from_millis(500),
            frame::read::<ServerMessage>(&mut par.recebe),
        )
        .await
        {
            Ok(Ok(ServerMessage::Alert {
                reason: AlertReason::RateLimited,
                ..
            })) => avisado = true,
            Ok(Ok(ServerMessage::Disconnecting {
                reason: DisconnectReason::RateLimited,
            })) => {
                despedido = true;
                break;
            }
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(_)) => break,
        }
    }
    assert!(avisado, "mil relatos e nenhum aviso de excesso");
    assert!(
        despedido,
        "mil relatos de uma vez e a sessão não foi encerrada por excesso"
    );

    // E o servidor segue de pé: outra pessoa entra e é anunciada.
    let mut outra = abrir(endereco, 26).await?;
    let eu = outra.eu;
    let sinais = observar(&mut outra, eu, Duration::from_secs(5), || None).await?;
    assert!(
        !sinais.is_empty(),
        "depois da enxurrada o servidor deixou de anunciar quem chega"
    );

    servidor.shutdown();
    Ok(())
}
