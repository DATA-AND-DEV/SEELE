//! Quem liga pelo código (tarefas 6c e 6d do Plano 5): a chegada pergunta ao
//! ponto de encontro onde a sala mora, e corre os candidatos que ele deu.
//!
//! O que se prova aqui, de ponta a ponta, com um ponto de verdade no loopback:
//!
//! - quem recebe disca o próprio código e entra, e a trilha diz `Localizando`
//!   antes da primeira tentativa;
//! - **sem *hairpin***: quando o endereço público que o ponto deu não atende, a
//!   ligação entra pela dica de rede local, e a dica é tentada primeiro;
//! - um código que não mora no ponto desiste com `NinguemMora`, e um ponto que
//!   não responde, com `PontoMudo` — cada um com o seu motivo, e não com um
//!   «não alcancei» de endereço nenhum.
//!
//! Todo ponto daqui é `127.0.0.1`, e os guardas das duas pontas reprovariam um
//! teste que perguntasse à VPS de produção.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use seele_core::chegada::{Caminho, Chegada, Etapa, Passo};
use seele_core::encontro::FalhaAoLocalizar;
use seele_core::{ConnectError, MemoryPinStore, PinStore};
use seele_proto::codigo::Codigo;
use seele_proto::encontro::{Marcas, Moro2, Resposta2, Vizinhanca};
use seele_server::alcance::encontro::EstadoDoRegistro;
use seele_server::alcance::{Escuta, Pilha};
use seele_server::hospedagem::Hospedagem;

mod vaga;

/// A semente de quem recebe.
const DONO: u8 = 1;

fn chave(semente: u8) -> SigningKey {
    SigningKey::from_bytes(&[semente; 32])
}

fn codigo_de(semente: u8) -> Codigo {
    Codigo::da_chave(&chave(semente).verifying_key().to_bytes())
}

fn pins() -> Arc<dyn PinStore> {
    Arc::new(MemoryPinStore::new())
}

/// Um ponto de encontro de verdade, atendendo numa linha própria.
fn subir_o_ponto() -> SocketAddr {
    let ponto = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Vizinhanca::TambemAqui,
        Arc::new(seele_encontro::Quarto::novo()),
    )
    .expect("a escuta tem que abrir");
    let onde = ponto.endereco().expect("o socket sabe onde ligou");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    onde
}

async fn subir_a_sala(ponto: Option<String>) -> Hospedagem {
    Hospedagem::iniciar_sala(0, &[DONO; 32], "dona", ponto)
        .await
        .expect("a sala não subiu")
}

/// Os nomes da trilha, em ordem.
fn nomes(trilha: &[Passo]) -> Vec<&'static str> {
    trilha.iter().map(|passo| passo.etapa.nome()).collect()
}

/// Onde a primeira tentativa bateu.
fn primeira_tentativa(trilha: &[Passo]) -> SocketAddr {
    trilha
        .iter()
        .find_map(|passo| match passo.etapa {
            Etapa::Tentando { onde, .. } => Some(onde),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a ligação não tentou endereço nenhum: {trilha:?}"))
}

#[tokio::test]
async fn quem_recebe_disca_o_proprio_codigo_pelo_ponto_e_entra() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let sala = subir_a_sala(Some(ponto.to_string())).await;
    let mut registro = sala
        .registro_no_quarto()
        .expect("a sala subiu sem registro no ponto");
    let registrou = tokio::time::timeout(
        Duration::from_secs(5),
        registro.wait_for(|estado| *estado == EstadoDoRegistro::Registrado),
    )
    .await
    .is_ok_and(|visto| visto.is_ok());
    assert!(
        registrou,
        "a sala não se registrou: {:?}",
        *registro.borrow()
    );

    // As dicas que a sala deixou no ponto: são elas que vêm primeiro.
    let dicas = seele_server::alcance::encontro::dicas_desta_maquina(
        Escuta::nova(sala.endereco().port(), Pilha::Dupla),
        &seele_server::alcance::interfaces::descobrir(),
    );

    let chegada = Chegada::para_o_codigo(codigo_de(DONO), Some(ponto.to_string()), "dona".into());
    let chegado = chegada
        .chegar(chave(DONO), pins())
        .await
        .unwrap_or_else(|frustrada| {
            panic!(
                "quem recebe não entrou na própria sala pelo código: {:?} depois de {:?}",
                frustrada.motivo(),
                frustrada.trilha()
            )
        });

    let trilha = nomes(&chegado.trilha);
    assert_eq!(
        trilha.get(..2),
        Some(["Parada", "Localizando"].as_slice()),
        "uma ligação começa perguntando ao ponto: {trilha:?}"
    );
    assert_eq!(trilha.last(), Some(&"Dentro"), "{trilha:?}");
    assert!(
        trilha.contains(&"Avisando"),
        "a sala deixou a escuta no ponto, e a ligação não a avisou: {trilha:?}"
    );
    let primeira = primeira_tentativa(&chegado.trilha);
    match dicas.first() {
        Some(dica) => assert_eq!(
            primeira, *dica,
            "a sala deixou a dica {dica}, e a primeira tentativa foi {primeira}"
        ),
        None => assert_eq!(
            primeira.port(),
            sala.endereco().port(),
            "sem dica, a primeira tentativa é o servidor"
        ),
    }
    drop(chegado);
    sala.encerrar().await;
}

/// Registra no ponto a marca do servidor da sala de `DONO` por um socket que
/// morre em seguida, com `dica` como a dica de rede local.
///
/// É o que o ponto vê de uma casa cujo roteador não faz *hairpin*: o endereço
/// público, visto de dentro da mesma casa, não atende.
async fn registrar_com_o_publico_morto(ponto: SocketAddr, dica: SocketAddr) {
    let fantasma = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("o loopback não abriu");
    let marca = Marcas::da_sala(&codigo_de(DONO)).servidor().clone();
    let publica = chave(DONO).verifying_key().to_bytes();
    let carimbo = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("o relógio anda")
        .as_secs();
    let dicas = vec![dica];
    let assinatura = chave(DONO)
        .sign(&seele_proto::assinatura::moro2(
            &marca, &publica, carimbo, &dicas,
        ))
        .to_bytes();
    let mut pedido = Moro2 {
        marca,
        chave: publica,
        carimbo,
        dicas,
        assinatura,
        cookie: None,
    };

    // O primeiro `MORO2` volta com o cookie deste IP; o segundo, com ele, mora.
    let mut balde = [0_u8; 512];
    for _ in 0..2 {
        fantasma
            .send_to(&seele_proto::encontro::moro2(&pedido), ponto)
            .await
            .expect("o registro não saiu");
        let (lidos, _) =
            tokio::time::timeout(Duration::from_secs(2), fantasma.recv_from(&mut balde))
                .await
                .expect("o ponto não respondeu ao registro")
                .expect("o socket não leu");
        match seele_proto::encontro::ler_resposta2(balde.get(..lidos).unwrap_or_default()) {
            Some(Resposta2::Desafio { cookie, .. }) => pedido.cookie = Some(cookie),
            Some(Resposta2::Morou { .. }) => return,
            outra => panic!("o ponto não aceitou o registro: {outra:?}"),
        }
    }
    panic!("o ponto pediu o cookie duas vezes");
}

#[tokio::test]
async fn sem_hairpin_a_ligacao_entra_pela_dica() {
    // Duas «casas» no mesmo host. A sala atende em todas as placas, e o ponto
    // só a conhece por um endereço público que não atende: o *hairpin* que o
    // roteador não faz. A dica é a placa de rede local desta máquina, na porta
    // da sala, como a sala de verdade a registraria.
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let sala = subir_a_sala(None).await;
    let Some(dica) = seele_server::alcance::encontro::dicas_desta_maquina(
        Escuta::nova(sala.endereco().port(), Pilha::Dupla),
        &seele_server::alcance::interfaces::descobrir(),
    )
    .first()
    .copied() else {
        panic!(
            "esta máquina não tem placa de rede local física com endereço de casa \
             (RFC 1918 ou ULA): o teste da dica precisa de uma"
        );
    };
    registrar_com_o_publico_morto(ponto, dica).await;

    let chegada = Chegada::para_o_codigo(codigo_de(DONO), Some(ponto.to_string()), "dona".into());
    let chegado = chegada
        .chegar(chave(DONO), pins())
        .await
        .unwrap_or_else(|frustrada| {
            panic!(
                "a ligação não entrou pela dica: {:?} depois de {:?}",
                frustrada.motivo(),
                frustrada.trilha()
            )
        });

    assert_eq!(
        primeira_tentativa(&chegado.trilha),
        dica,
        "a dica não foi a primeira tentativa: {:?}",
        chegado.trilha
    );
    assert_eq!(
        chegado.caminho(),
        Some(Caminho::RedeLocal),
        "a ligação não saiu pela rede de casa: {:?}",
        chegado.trilha
    );
    drop(chegado);
    sala.encerrar().await;
}

#[tokio::test]
async fn um_codigo_que_nao_mora_no_ponto_desiste_dizendo_isso() {
    // «ALICE NÃO ESTÁ DISPONÍVEL», e não «não alcancei»: o ponto disse que
    // ninguém mora ali, e esse é o motivo que a tela precisa.
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();

    let chegada = Chegada::para_o_codigo(codigo_de(7), Some(ponto.to_string()), "pessoa".into());
    let Err(frustrada) = chegada.chegar(chave(2), pins()).await else {
        panic!("uma ligação para um código que não mora no ponto entrou em algum lugar");
    };

    assert_eq!(
        frustrada.motivo(),
        &ConnectError::NaoLocalizou(FalhaAoLocalizar::NinguemMora)
    );
    assert_eq!(
        nomes(frustrada.trilha()),
        ["Parada", "Localizando", "Desistiu"],
        "{:?}",
        frustrada.trilha()
    );
}

#[tokio::test]
async fn um_ponto_que_nao_responde_desiste_com_o_ponto_mudo() {
    // «O PONTO DE ENCONTRO NÃO RESPONDEU». Um socket aberto que nunca lê: as
    // perguntas chegam, e nada volta.
    let _vaga = vaga::minha();
    let mudo = std::net::UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
    let ponto = mudo.local_addr().expect("o socket sabe onde ligou");

    let comeco = std::time::Instant::now();
    let chegada = Chegada::para_o_codigo(codigo_de(DONO), Some(ponto.to_string()), "pessoa".into());
    let Err(frustrada) = chegada.chegar(chave(2), pins()).await else {
        panic!("uma ligação por um ponto mudo entrou em algum lugar");
    };

    assert_eq!(
        frustrada.motivo(),
        &ConnectError::NaoLocalizou(FalhaAoLocalizar::PontoMudo)
    );
    assert_eq!(
        nomes(frustrada.trilha()),
        ["Parada", "Localizando", "Desistiu"]
    );
    assert!(
        comeco.elapsed() < seele_core::encontro::PRAZO_DO_QUARTO * 2,
        "um ponto mudo segurou a ligação {:?}",
        comeco.elapsed()
    );
    drop(mudo);
}
