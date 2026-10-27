//! A janela em que um estranho virava o Comandante de um servidor recém-criado.
//!
//! **O defeito, como foi lido em `4fc31e9c`.** `Hospedagem::iniciar` punha o
//! laço de aceitação no ar, e só **depois** o app semeava a portaria e admitia
//! quem hospeda. Num banco novo a portaria nasce desligada, e a primeira conta
//! criada num servidor vira a Comandante (`permissions::seat_the_arrival`).
//! Quem chegasse nesse intervalo entrava sem bater e ficava com o servidor.
//!
//! **O conserto** (D-seg-1 do spec da 0.16, a tarefa 4e do Plano 5, que entra
//! pelo Plano 2): `Hospedagem::iniciar` recebe o que fazer **antes de
//! aceitar**, e roda isso com o banco já aberto e o laço ainda parado. A ordem
//! deixou de depender de quem chama. É o desenho de `ba99007`, que a linha
//! 0.15 leva na 0.15.2.
//!
//! **As duas formas de banco.** Em arquivo é o servidor guardado; em memória é
//! a forma da sala pessoal, que sobe pela mesma porta (tarefa 4f). As duas
//! passam pelo mesmo `iniciar`, e a prova roda nas duas.
//!
//! **A prova tem duas metades.**
//!
//! 1. **A estrutural:** enquanto o gancho roda, um estranho conecta e manda um
//!    primeiro quadro de versão desconhecida, e a sessão **não** responde. Na
//!    0.17 o enlace fecha ICE e DTLS sozinho, então a conexão existir não prova
//!    nada; a recusa por versão, que sai antes de qualquer leitura do banco,
//!    prova que o laço de sessão já estava no ar (`a_sessao_responde`). Esta é
//!    a metade que reprova com a ordem de antes.
//! 2. **A de quem usa:** depois, o estranho bate e fica de fora, e quem hospeda
//!    entra e é o Comandante.
//!
//! Por que a primeira metade não é «o estranho entrou durante a espera»: o
//! gancho roda com o banco travado, e a sessão do estranho precisa do banco
//! para criar a conta. A primeira tentativa desta prova, em `ba99007`, passou
//! revertida por isso. A conexão aceita não depende do banco, e é ela que diz
//! se o laço já estava no ar.
//!
//! # Por que um par cru, e não um `Client`
//!
//! O mesmo motivo de `personalizacao.rs`: `seele-core` não pode ser dependência
//! daqui (ADR 0002).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, DisconnectReason, Permission, ServerMessage};
use seele_server::frame;
use seele_server::hospedagem::Hospedagem;
use seele_server::permissions::Permissions;
use seele_server::persistence::Location;

mod comum;

/// A semente da chave de quem hospeda.
const DONO: u8 = 1;
/// A semente da chave de quem chega sem ter sido chamado.
const ESTRANHO: u8 = 7;

/// O que aconteceu com uma tentativa de entrar.
#[derive(Debug, PartialEq, Eq)]
enum Desfecho {
    /// O servidor mandou `Session`: a conta existe e está dentro.
    Entrou,
    /// O servidor encerrou o aperto de mão, com o motivo.
    Barrado(DisconnectReason),
}

/// Um aperto de mão inteiro, sem convite, com a chave desta semente.
async fn tentar(endereco: SocketAddr, semente: u8) -> Result<Desfecho> {
    let comum::Ligado {
        ponta: _ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
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

    loop {
        match frame::read::<ServerMessage>(&mut recebe).await? {
            ServerMessage::Challenge { nonce } => {
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
            }
            ServerMessage::Session { .. } => return Ok(Desfecho::Entrou),
            ServerMessage::Disconnecting { reason } => return Ok(Desfecho::Barrado(reason)),
            // Outro quadro do aperto de mão: segue lendo.
            _ => {}
        }
    }
}

/// **Se o laço de sessão já está no ar**, sem depender do banco.
///
/// No quinn, o aperto de mão do transporte só andava quando o laço chamava
/// `accept`, e uma conexão estabelecida já era a prova. No enlace da 0.17 o
/// ICE e o DTLS fecham na tarefa da ponta, antes de o laço pedir a conexão: a
/// conexão existir não prova mais nada. O que prova é uma **resposta da
/// sessão**, e a única que não toca no banco (que o gancho segura) é a recusa
/// por versão: um primeiro quadro com o carimbo 30 recebe `[10, 11, 0]` antes
/// de qualquer leitura do banco (`session::handshake`). Uma resposta durante o
/// gancho é o laço de sessão rodando antes dele.
async fn a_sessao_responde(endereco: SocketAddr) -> Result<bool> {
    let comum::Ligado {
        ponta: _ponta,
        conexao: _conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    envio
        .write_all(&seele_proto::quadro::enquadrar(&[30, 0, 1, 2, 3])?)
        .await?;
    let mut prefixo = [0_u8; seele_proto::quadro::PREFIXO_LEN];
    Ok(recebe.read_exact(&mut prefixo).await.is_ok())
}

/// Desliga o degrau 4 para o binário inteiro: nenhum teste daqui fala com o
/// ponto de encontro de produção.
///
/// Sem `$SEELE_ENCONTRO`, `Hospedagem::iniciar` caía no ponto padrão, e numa
/// máquina sem IPv4 global cada rodada registrava um servidor de teste no
/// quarto da VPS de produção. Desde a decisão (d) do lote G-servidor ele não
/// convoca ponto nenhum sem a variável, e o `nao` daqui é a intenção escrita:
/// o que este arquivo mede é a janela do Comandante, e nenhum pacote precisa
/// sair desta máquina para isso.
///
/// **Uma vez, e sem devolver.** Os dois testes daqui correm ao mesmo tempo, e
/// os dois querem o mesmo valor. Um guarda que devolvesse a variável ao cair
/// a devolveria no meio do `iniciar` do outro, que então leria o valor de antes
/// da troca: hoje, sem a variável, «nenhum ponto», e o teste nem perceberia;
/// mas um `$SEELE_ENCONTRO` que viesse do terminal de quem roda a bateria
/// voltaria a valer no meio do teste. Todo teste deste binário sobe
/// hospedagem; quem um dia acrescentar um que precise de outro valor troca
/// isto por um guarda com trava, como o `ambiente_de_teste` da biblioteca.
///
/// O guarda da biblioteca (`alcance::encontro::recusar_a_producao`) alcança
/// este arquivo só quando ele roda junto com o `seele-conformance`
/// (`cargo test --workspace`): ver a feature `recusar-encontro-de-producao` no
/// `Cargo.toml` deste crate. Rodado sozinho, quem o protege é o padrão.
fn sem_degrau_4() {
    static UMA_VEZ: std::sync::Once = std::sync::Once::new();
    UMA_VEZ.call_once(|| std::env::set_var(seele_server::alcance::encontro::VARIAVEL, "nao"));
}

/// Uma porta livre nesta máquina, para o estranho saber onde bater antes de
/// `iniciar` devolver o servidor.
fn porta_livre() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|e| e.port())
        .expect("uma porta livre")
}

fn chave_publica(semente: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[semente; 32])
        .verifying_key()
        .to_bytes()
}

/// A janela, medida num banco desta forma.
async fn a_janela_fica_fechada(banco: Location) {
    sem_degrau_4();
    let porta = porta_livre();
    let endereco = SocketAddr::from(([127, 0, 0, 1], porta));

    let dono = seele_proto::transport::key_fingerprint(&chave_publica(DONO));
    let aceito_durante_o_gancho = Arc::new(AtomicBool::new(false));
    let anotado = Arc::clone(&aceito_durante_o_gancho);
    let hospedagem = Hospedagem::iniciar(porta, banco, "Casa", None, move |persistence| {
        // O servidor já tem porta. Um estranho bate agora, numa linha de
        // execução própria (o gancho é síncrono), e tem um segundo e meio
        // para ser aceito. Se o laço estivesse no ar, seria.
        let tentativa = std::thread::spawn(move || {
            let tempo = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("um runtime para o estranho");
            tempo.block_on(async {
                tokio::time::timeout(Duration::from_millis(1500), a_sessao_responde(endereco)).await
            })
        });
        let aceito = matches!(tentativa.join(), Ok(Ok(Ok(true))));
        anotado.store(aceito, Ordering::SeqCst);

        seele_server::portaria::semear_ligada(persistence)?;
        // Como o app faz no `hospedar` (ADR 0064): o dono é declarado pela
        // máquina, e não sai de quem chega primeiro.
        seele_server::cargos::declarar_proprietario(persistence, &dono)?;
        seele_server::portaria::admitir_o_dono(persistence, &dono)
    })
    .await
    .expect("subir");

    // A metade estrutural.
    assert!(
        !aceito_durante_o_gancho.load(Ordering::SeqCst),
        "uma conexão foi aceita enquanto quem hospeda ainda semeava a portaria: \
         o laço de aceitação nasceu antes do gancho, e a janela do Comandante voltou"
    );

    // A metade de quem usa: o estranho bate agora e fica de fora.
    let desfecho_do_estranho =
        tokio::time::timeout(Duration::from_secs(20), tentar(endereco, ESTRANHO))
            .await
            .expect("o estranho teve resposta dentro do prazo")
            .expect("o aperto de mão do estranho terminou");
    assert!(
        matches!(desfecho_do_estranho, Desfecho::Barrado(_)),
        "um estranho entrou sem convite num servidor recém-criado: {desfecho_do_estranho:?}"
    );

    assert_eq!(
        tentar(endereco, DONO).await.expect("o dono conecta"),
        Desfecho::Entrou,
        "quem hospeda não entrou na própria casa"
    );

    let banco = hospedagem.persistence();
    let persistence = banco.lock().await;
    let permissoes = Permissions::new(&persistence);
    let do_dono = permissoes
        .register_or_find(&chave_publica(DONO), "pessoa001")
        .unwrap();
    assert!(
        permissoes
            .may(do_dono.id, Permission::AdministerServer)
            .unwrap(),
        "quem hospeda não ficou com o servidor"
    );
    let do_estranho = permissoes
        .register_or_find(&chave_publica(ESTRANHO), "pessoa007")
        .unwrap();
    assert!(
        !permissoes
            .may(do_estranho.id, Permission::AdministerServer)
            .unwrap(),
        "o estranho ficou com o servidor"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quem_chega_enquanto_o_servidor_guardado_sobe_nao_vira_comandante() {
    let pasta = tempfile::tempdir().unwrap();
    a_janela_fica_fechada(Location::File(pasta.path().join("seele.db"))).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quem_chega_enquanto_um_servidor_em_memoria_sobe_nao_vira_comandante() {
    a_janela_fica_fechada(Location::Memory).await;
}
