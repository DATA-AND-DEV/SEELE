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
//! Um teste que **precisa do degrau 4** pula **num caso só**, e pula
//! **escrevendo `PULADO` no stderr**, sem passar em silêncio: quando o convite
//! não leva `enc=` **e** a escada não guardou recusa nenhuma
//! (`Alcance::encontro_recusado()` é `None`). Quer dizer que o degrau 4 nem foi
//! tentado, e ele só é tentado numa máquina sem IPv4 global (`alcance.rs`,
//! `tem_ipv4_global`): numa VPS o degrau 1 já resolveu tudo, e não há encontro a
//! largar. Nesse caso o teste não mede nada, e fingir que passou seria pior que
//! dizer que não mediu.
//!
//! Quando a recusa **existe** (`Some(motivo)`), o degrau foi tentado, com o ponto
//! local no ar, e não deu. Isso é **falha**, com o motivo na mensagem, e não
//! pulo: se pulasse, uma regressão em `abrir` faria o guarda calar justamente
//! nas máquinas em que ele mede.
//!
//! O que isso custa é dito aqui, e não escondido: **numa máquina com IPv4 global
//! estes testes não provam o guarda**. Quem roda a prova por reversão precisa de
//! uma máquina sem IPv4 global, e confere que a linha `PULADO` **não** saiu (o
//! `--nocapture` mostra). Um verde com `PULADO` é um verde que não conta.
//!
//! E um resto que este arquivo não separa: `None` é também o que a escada guarda
//! quando o degrau 4 abre e ela o larga, por o endereço do furo não virar
//! candidato da escuta (`Escada::subir`, «o furo de NAT não virou candidato»).
//! Esse caso pula como o da VPS, e só o registro do servidor o distingue.
//!
//! # A variável `$SEELE_ENCONTRO`
//!
//! É do processo, e o `libtest` roda os outros testes deste binário nele. Quem a
//! troca a troca **por um [`AmbienteDoEncontro`]**, criado logo depois da vaga e
//! antes de qualquer troca: o `Drop` a devolve, inclusive em pânico, e antes de a
//! vaga ser devolvida (as variáveis locais caem na ordem inversa).

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

/// Guarda `$SEELE_ENCONTRO` e a devolve ao que era quando o teste acaba,
/// **inclusive em pânico**.
///
/// A variável é do processo, e o `libtest` roda os outros testes deste binário
/// nele: um teste que a troca e entra em pânico antes de devolvê-la deixaria o
/// ponto de encontro trocado para todo teste que vier depois. É o padrão de
/// `vaga::Vaga`: quem devolve é o `Drop`, e nenhum caminho precisa lembrar de
/// chamar nada à mão.
///
/// Crie-o logo depois de `let _vaga = vaga::minha();` e **antes** de qualquer
/// troca. As variáveis locais caem na ordem inversa, então a variável volta
/// antes de a vaga ser devolvida, e o próximo teste da fila a encontra como ela
/// era. Trocar a variável só se faz por ele ([`AmbienteDoEncontro::pedir`]).
#[must_use = "a variável só volta quando o guarda cai; soltá-lo na hora a devolve cedo demais"]
struct AmbienteDoEncontro {
    /// O que `$SEELE_ENCONTRO` era antes: `None` é «não estava definida».
    antes: Option<std::ffi::OsString>,
}

impl AmbienteDoEncontro {
    /// Guarda o valor de agora, sem trocar nada.
    fn guardar() -> Self {
        Self {
            antes: std::env::var_os(seele_server::alcance::encontro::VARIAVEL),
        }
    }

    /// Troca `$SEELE_ENCONTRO` por `valor`, até este guarda cair.
    fn pedir(&self, valor: &str) {
        std::env::set_var(seele_server::alcance::encontro::VARIAVEL, valor);
    }
}

impl Drop for AmbienteDoEncontro {
    fn drop(&mut self) {
        match self.antes.take() {
            Some(valor) => std::env::set_var(seele_server::alcance::encontro::VARIAVEL, valor),
            None => std::env::remove_var(seele_server::alcance::encontro::VARIAVEL),
        }
    }
}

#[test]
fn o_ambiente_do_encontro_volta_mesmo_quando_o_teste_entra_em_panico() {
    let _vaga = vaga::minha();
    // Uma reserva por fora: se o guarda de dentro estiver quebrado, é ela que
    // devolve a variável, e a falha deste teste não contamina os outros.
    let _reserva = AmbienteDoEncontro::guardar();
    let antes = std::env::var_os(seele_server::alcance::encontro::VARIAVEL);

    let resultado = std::panic::catch_unwind(|| {
        let ambiente = AmbienteDoEncontro::guardar();
        ambiente.pedir("valor-que-o-panico-nao-pode-deixar-para-tras");
        panic!("pânico provocado de propósito, com a variável já trocada");
    });

    assert!(
        resultado.is_err(),
        "o pânico provocado não aconteceu: este teste não mede nada"
    );
    assert_eq!(
        std::env::var_os(seele_server::alcance::encontro::VARIAVEL),
        antes,
        "um teste que entrou em pânico com `$SEELE_ENCONTRO` trocada deixou a troca para trás: \
         todo teste seguinte deste binário fala com o ponto de encontro errado"
    );
}

#[tokio::test]
async fn largar_uma_hospedagem_sem_encerrar_devolve_a_porta() {
    let _vaga = vaga::minha();
    let ambiente = AmbienteDoEncontro::guardar();
    // O defeito: `Encontro` não tinha `Drop`. Uma `Hospedagem` descartada sem
    // `encerrar`, que é o que fechar a janela faz, largava o `JoinHandle` do
    // degrau 4, e a tarefa seguia viva segurando uma cópia do socket do
    // servidor. Hospedar de novo na mesma porta falhava com «endereço já em
    // uso» até o app fechar.
    let (ponto, _) = ponto_na_porta_padrao();

    // O ponto sem porta, como `PONTO_PADRAO` é escrito.
    ambiente.pedir(&ponto.ip().to_string());
    let primeira = Hospedagem::iniciar(0, Location::Memory, "Casa", None)
        .await
        .expect("a primeira hospedagem sobe");
    // Daqui em diante nenhum `iniciar` fala com ponto de encontro nenhum: o
    // que se mede é a porta, e nenhum pacote precisa sair desta máquina.
    ambiente.pedir("nao");

    let endereco = primeira.endereco();
    if !primeira.convite().contains("enc=") {
        // O degrau 4 só é tentado sem IPv4 global (`alcance.rs`,
        // `tem_ipv4_global`). Numa VPS ele nem é tentado, e sem ele não há
        // tarefa a vazar: `encontro_recusado()` é `None`, e isso é PULO. Se foi
        // tentado, com o ponto local no ar, e a escada guardou por que não deu,
        // isso é FALHA: pular aí faria uma regressão em `abrir` calar o guarda
        // justamente nas máquinas em que ele mede.
        let recusa = primeira
            .alcance()
            .and_then(seele_server::alcance::Alcance::encontro_recusado)
            .map(str::to_owned);
        primeira.encerrar().await;
        let Some(motivo) = recusa else {
            let _ = writeln!(
                std::io::stderr(),
                "PULADO: largar_uma_hospedagem_sem_encerrar_devolve_a_porta: o degrau 4 nem foi \
                 tentado nesta máquina (IPv4 global?), e sem ele este teste não mede nada"
            );
            return;
        };
        panic!(
            "o degrau 4 foi tentado contra o ponto local em {ponto} e a escada o recusou: \
             {motivo}. Sem ele o convite não leva `enc=` e o guarda não mede nada, e pular aqui \
             esconderia uma regressão em `abrir`"
        );
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
