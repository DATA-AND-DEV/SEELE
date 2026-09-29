//! O quarto pelo caminho que o link percorre, e não pelo atalho.
//!
//! `quarto.rs` fala com o ponto de encontro por `127.0.0.1:<porta>` e faz o
//! `MORO` à mão. Foi por isso que nenhum teste viu os três defeitos do link
//! (análise de 22/09/2026, §2.1): o link carrega o ponto **sem porta**, o
//! anfitrião registrava a escuta com uma marca que ninguém perguntava, e o
//! primeiro registro saía quinze segundos depois da subida. Aqui o ponto é
//! escrito como o link o escreve, e quem registra é o anfitrião de verdade.
//!
//! # Por que o ponto destes testes mora na 8384
//!
//! «Sem porta» só se testa com a porta padrão de verdade. As outras saídas
//! foram pesadas e recusadas:
//!
//! - **tornar a porta padrão injetável** poria em produção um parâmetro, ou uma
//!   variável, que só o teste usa. Pior: o teste deixaria de provar a regra
//!   real, e trocar `PORTA_PADRAO`, ou a porta que `separar_ponto` aplica,
//!   passaria verde;
//! - **escrever a porta no ponto** é exatamente o atalho que escondeu o defeito.
//!
//! O preço é que a porta 8384 deste computador precisa estar livre. Quando não
//! está (um `seele-encontro` rodando aqui, por exemplo), o teste **falha**
//! dizendo isso, e não pula: um guarda que se cala quando o ambiente não ajuda
//! é um guarda que ninguém vê falhar. Os testes que usam a 8384 moram todos
//! neste arquivo e tomam a vaga, e o `cargo test` roda um binário de teste por
//! vez. O ponto sobe uma vez por processo e fica.
//!
//! `127.0.0.1` faz o papel do nome: é um endereço escrito sem porta, com a
//! forma de `encontro.seele.app.br`, e não depende de DNS.
//!
//! # A exceção declarada: quando um teste daqui pula
//!
//! Um teste que **precisa do degrau 4** pula, e pula **escrevendo `PULADO` no
//! stderr** com o motivo, sem passar em silêncio. O degrau 4 só é tentado numa
//! máquina sem IPv4 global (`alcance.rs`, `tem_ipv4_global`): numa VPS o degrau 1
//! já resolveu tudo, o convite nunca leva `enc=`, e não há encontro a largar.
//! Também não há degrau 4 quando o ponto não responde. Nesses dois casos o teste
//! não mede nada, e fingir que passou seria pior que dizer que não mediu.
//!
//! O que isso custa é dito aqui, e não escondido: **numa máquina com IPv4 global
//! estes testes não provam o guarda**. Quem roda a prova por reversão precisa de
//! uma máquina sem IPv4 global, e confere que a linha `PULADO` **não** saiu (o
//! `--nocapture` mostra). Um verde com `PULADO` é um verde que não conta.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use seele_proto::encontro::{Vizinhanca, PORTA_PADRAO};
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;

mod vaga;

/// O ponto de encontro deste arquivo, em `127.0.0.1:8384`, com o quarto dele.
///
/// O quarto volta junto para quem precisa saber quando um registro chegou:
/// [`seele_encontro::Quarto::quantos`] existe para isso.
fn ponto_na_porta_padrao() -> (SocketAddr, Arc<seele_encontro::Quarto>) {
    static PONTO: OnceLock<(SocketAddr, Arc<seele_encontro::Quarto>)> = OnceLock::new();
    PONTO
        .get_or_init(|| {
            let escuta = SocketAddr::from(([127, 0, 0, 1], PORTA_PADRAO));
            let quarto = Arc::new(seele_encontro::Quarto::novo());
            let ponto = seele_encontro::Ponto::abrir_com_quarto(
                escuta,
                Vizinhanca::TambemAqui,
                Arc::clone(&quarto),
            )
            .unwrap_or_else(|erro| {
                panic!(
                    "este teste precisa da porta {escuta} livre, porque é nela que um ponto \
                     escrito sem porta é procurado. Algo nesta máquina já a ocupa \
                     (`lsof -nP -iUDP:{PORTA_PADRAO}`): {erro}"
                )
            });
            std::thread::spawn(move || {
                let _ = ponto.servir();
            });
            (escuta, quarto)
        })
        .clone()
}

/// Devolve `$SEELE_ENCONTRO` ao que era.
fn restaurar(antes: Option<String>) {
    match antes {
        Some(valor) => std::env::set_var(seele_server::alcance::encontro::VARIAVEL, valor),
        None => std::env::remove_var(seele_server::alcance::encontro::VARIAVEL),
    }
}

#[tokio::test]
async fn largar_uma_hospedagem_sem_encerrar_devolve_a_porta() {
    let _vaga = vaga::minha();
    // O defeito: `Encontro` não tinha `Drop`. Uma `Hospedagem` descartada sem
    // `encerrar`, que é o que fechar a janela faz, largava o `JoinHandle` do
    // degrau 4, e a tarefa seguia viva segurando uma cópia do socket do
    // servidor. Hospedar de novo na mesma porta falhava com «endereço já em
    // uso» até o app fechar.
    let (ponto, _) = ponto_na_porta_padrao();
    let variavel = seele_server::alcance::encontro::VARIAVEL;
    let antes = std::env::var(variavel).ok();

    // O ponto sem porta, como `PONTO_PADRAO` é escrito.
    std::env::set_var(variavel, ponto.ip().to_string());
    let primeira = Hospedagem::iniciar(0, Location::Memory, "Casa", None)
        .await
        .expect("a primeira hospedagem sobe");
    // Daqui em diante nenhum `iniciar` fala com ponto de encontro nenhum: o
    // que se mede é a porta, e nenhum pacote precisa sair desta máquina.
    std::env::set_var(variavel, "nao");

    let endereco = primeira.endereco();
    if !primeira.convite().contains("enc=") {
        // O degrau 4 só é tentado sem IPv4 global (`alcance.rs`,
        // `tem_ipv4_global`). Numa VPS ele não abre, e sem ele não há tarefa a
        // vazar. O motivo que a escada guardou vai na linha, para o `PULADO` não
        // ser um silêncio com outro nome.
        let motivo = primeira
            .alcance()
            .and_then(seele_server::alcance::Alcance::encontro_recusado)
            .unwrap_or("nenhum motivo registrado: o degrau 4 nem foi tentado")
            .to_owned();
        primeira.encerrar().await;
        restaurar(antes);
        let _ = writeln!(
            std::io::stderr(),
            "PULADO: largar_uma_hospedagem_sem_encerrar_devolve_a_porta: o degrau 4 não abriu \
             nesta máquina (IPv4 global?), e sem ele este teste não mede nada. Motivo: {motivo}"
        );
        return;
    }

    // Largar, e não encerrar.
    drop(primeira);

    // O servidor também leva um instante para devolver o socket depois de
    // `shutdown`. O que este teste separa é «um instante» de «nunca», e quem
    // conta o instante é o número de tentativas: um `iniciar` que dá certo já
    // gasta até `PROCURA` (3 s) de UPnP, então o tempo sozinho não distingue
    // uma porta lenta de uma máquina lenta.
    let comecou = std::time::Instant::now();
    let ate = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut tentativas = 0_u32;
    let segunda = loop {
        tentativas += 1;
        match Hospedagem::iniciar(endereco.port(), Location::Memory, "Casa", None).await {
            // A mesma escuta de antes, inteira. Um socket que caiu para IPv4
            // porque o IPv6 da porta continuava preso não é a porta devolvida.
            Ok(segunda) if segunda.endereco() == endereco => break Some(segunda),
            Ok(outra) => {
                outra.encerrar().await;
                if tokio::time::Instant::now() >= ate {
                    break None;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(_) if tokio::time::Instant::now() < ate => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(_) => break None,
        }
    };
    restaurar(antes);

    let Some(segunda) = segunda else {
        panic!(
            "a porta {endereco} continuou presa cinco segundos depois de a hospedagem ser \
             largada ({tentativas} tentativas): a tarefa do degrau 4 seguiu viva segurando o \
             socket do servidor"
        );
    };
    // A medida que o Step 4 pede, e que atravessa a captura do `libtest`.
    let _ = writeln!(
        std::io::stderr(),
        "largar_uma_hospedagem_sem_encerrar_devolve_a_porta: a porta {endereco} voltou em \
         {tentativas} tentativa(s), {:?} depois de a hospedagem ser largada",
        comecou.elapsed()
    );
    segunda.encerrar().await;
}
