//! O quarto do ponto de encontro: onde um servidor mora **hoje**.
//!
//! O relato que o pediu: «o ip que fica salvo na lista de servidores ainda dá
//! problema de reconexão, possivelmente porque a porta muda quando abre e fecha
//! o server. Precisamos achar um jeito de resolver isso, por que se não, a lista
//! de servidores fica inútil.»
//!
//! Tudo o que a lista guarda é endereço, e endereço atrás de NAT é perecível: o
//! mapeamento nasce quando um pacote sai e o roteador dá outro na abertura
//! seguinte. O que não envelhece é a impressão digital, e é dela que sai a marca
//! com que se pergunta.
//!
//! Aqui, e não dentro de um crate só: o teste precisa do anfitrião, do cliente e
//! do serviço no meio, e o ADR 0002 proíbe qualquer um deles de ver o outro.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;

use seele_core::encontro::{onde_mora, Marca};
use seele_proto::encontro::{moro, Vizinhanca, TAMANHO};

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

#[tokio::test]
async fn um_servidor_que_trocou_de_porta_ainda_e_achado_pela_impressao() {
    let onde_fica = subir_o_ponto();
    let marca = Marca::nova("abcdef0123456789").expect("é uma marca");
    let mut balde = [0_u8; TAMANHO];

    // A primeira abertura do servidor.
    let ontem = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let porta_de_ontem = ontem.local_addr().unwrap();
    ontem.send_to(&moro(&marca), onde_fica).await.unwrap();
    ontem.recv_from(&mut balde).await.unwrap();

    // Ele fecha, e volta noutra porta — que é o que o NAT faz.
    drop(ontem);
    let hoje = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let porta_de_hoje = hoje.local_addr().unwrap();
    assert_ne!(
        porta_de_ontem, porta_de_hoje,
        "o teste precisa de duas portas diferentes para dizer alguma coisa"
    );
    hoje.send_to(&moro(&marca), onde_fica).await.unwrap();
    hoje.recv_from(&mut balde).await.unwrap();

    // E quem guardou só a impressão digital pergunta.
    let achado = onde_mora(
        &onde_fica.to_string(),
        &marca,
        std::time::Duration::from_secs(2),
    )
    .await;

    assert_eq!(
        achado,
        Some(porta_de_hoje),
        "quem perguntou recebeu a porta de ontem, ou não recebeu nada — e é \
         exatamente por isso que a lista de servidores ficava inútil"
    );
}

#[tokio::test]
async fn um_ponto_que_nao_conhece_a_pergunta_apenas_cala() {
    // O caso de campo mais provável durante a migração: o serviço no ar é o de
    // antes desta mudança. Ele não responde a `QUEM`, e o que tem de acontecer é
    // a espera vencer e a conexão seguir com os endereços guardados — nunca uma
    // falha que derrube a tentativa.
    let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let onde_fica = mudo.local_addr().unwrap();
    let marca = Marca::nova("abcdef0123456789").expect("é uma marca");

    let comecou = std::time::Instant::now();
    let achado = onde_mora(
        &onde_fica.to_string(),
        &marca,
        std::time::Duration::from_millis(200),
    )
    .await;

    assert_eq!(achado, None, "não havia resposta a inventar");
    assert!(
        comecou.elapsed() < std::time::Duration::from_secs(2),
        "a espera não pode passar do prazo pedido: quem paga é toda conexão, \
         inclusive a de quem está na mesma casa"
    );
}

#[tokio::test]
async fn quem_esta_no_ar_nao_perde_o_lugar_para_quem_chega_dizendo_o_nome_dele() {
    // Qualquer um manda `MORO` com a marca de outro. Isto não é autenticação —
    // este serviço não tem chave nenhuma para conferir — e não precisa ser: quem
    // chega confere a impressão digital de qualquer jeito (ADR 0003), então um
    // endereço errado falha no aperto de mão em vez de virar conexão com o
    // impostor.
    //
    // O que a regra de «quem escreveu primeiro fica» compra é que o impostor não
    // consiga nem isso enquanto o dono estiver no ar.
    let onde_fica = subir_o_ponto();
    let marca = Marca::nova("abcdef0123456789").expect("é uma marca");
    let mut balde = [0_u8; TAMANHO];

    let dono = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let do_dono = dono.local_addr().unwrap();
    dono.send_to(&moro(&marca), onde_fica).await.unwrap();
    dono.recv_from(&mut balde).await.unwrap();

    // No mesmo IP, então este teste não consegue distinguir o impostor pelo IP —
    // e é por isso que ele afirma o que afirma com o `127.0.0.1`: a regra
    // compara IP, e num teste de laço os dois são o mesmo. O que se prende aqui
    // é que o **dono** consegue se mudar, que é o outro lado da mesma regra.
    let mudou = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let novo = mudou.local_addr().unwrap();
    mudou.send_to(&moro(&marca), onde_fica).await.unwrap();
    mudou.recv_from(&mut balde).await.unwrap();

    let achado = onde_mora(
        &onde_fica.to_string(),
        &marca,
        std::time::Duration::from_secs(2),
    )
    .await;
    assert_eq!(
        achado,
        Some(novo),
        "o dono mudou de porta no mesmo IP e o quarto ficou com a antiga: é o \
         remapeamento de NAT, e é o caso que este mecanismo existe para cobrir \
         (o de {do_dono})"
    );
}
