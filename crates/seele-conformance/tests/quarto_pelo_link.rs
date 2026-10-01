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
//! forma de `encontro.seele.app.br`, e não depende de DNS. O ponto também
//! atende em `[::1]:8384`, com o mesmo quarto, como o de produção atende nas
//! duas famílias (ver [`ponto_na_porta_padrao`]): a porta tem de estar livre
//! nas duas, e a máquina precisa do loopback IPv6.
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
use std::net::{Ipv6Addr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use seele_core::encontro::{onde_mora_hoje, EscutaDoQuarto, Marcas, OndeMora, PRAZO_DO_QUARTO};
use seele_proto::encontro::{moro, Vizinhanca, PORTA_PADRAO, TAMANHO};
use seele_server::alcance::encontro::Convocacao;
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;

mod vaga;

/// O ponto de encontro deste arquivo, em `127.0.0.1:8384`, com o quarto dele.
///
/// O quarto volta junto para quem precisa saber quando um registro chegou:
/// [`seele_encontro::Quarto::quantos`] existe para isso.
///
/// **As duas famílias e um quarto só, como o `seele-encontro` de produção**
/// (`crates/seele-encontro/src/main.rs`, «um quarto para as duas»): a escuta
/// IPv4 em `127.0.0.1:8384`, que é a que o link procura, e uma IPv6 em
/// `[::1]:8384`. Um anfitrião que registrasse o servidor por uma família e a
/// escuta pela outra seria visto pelo quarto em dois IPs, e é assim que a
/// regra da escuta o deixaria de lado; com uma família só, o registro pela
/// outra se perderia, e o quarto mostraria outro defeito.
fn ponto_na_porta_padrao() -> (SocketAddr, Arc<seele_encontro::Quarto>) {
    static PONTO: OnceLock<(SocketAddr, Arc<seele_encontro::Quarto>)> = OnceLock::new();
    PONTO
        .get_or_init(|| {
            let escuta = SocketAddr::from(([127, 0, 0, 1], PORTA_PADRAO));
            let quarto = Arc::new(seele_encontro::Quarto::novo());
            for onde in [
                escuta,
                SocketAddr::from((Ipv6Addr::LOCALHOST, PORTA_PADRAO)),
            ] {
                let ponto = seele_encontro::Ponto::abrir_com_quarto(
                    onde,
                    Vizinhanca::TambemAqui,
                    Arc::clone(&quarto),
                )
                .unwrap_or_else(|erro| {
                    panic!(
                        "este teste precisa da porta {onde} livre, porque é na 8384 que um \
                         ponto escrito sem porta é procurado, e o ponto atende nas duas \
                         famílias, como o de produção. Algo nesta máquina já a ocupa \
                         (`lsof -nP -iUDP:{PORTA_PADRAO}`), ou ela não tem o loopback IPv6: \
                         {erro}"
                    )
                });
                std::thread::spawn(move || {
                    let _ = ponto.servir();
                });
            }
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
    // Sem reserva por fora: uma segunda `AmbienteDoEncontro` teria o mesmo
    // `Drop`, e quebraria junto com o guarda que este teste prova. Se ele
    // quebrar, os testes seguintes deste binário podem herdar a variável
    // trocada, e é a falha deste que diz por quê.
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
                "PULADO: largar_uma_hospedagem_sem_encerrar_devolve_a_porta: o convite saiu sem \
                 `enc=` e a escada não guardou recusa nenhuma (IPv4 global, ou o degrau 4 abriu e \
                 a escada o largou), e sem ele este teste não mede nada"
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

/// A impressão digital do teste da consulta sem porta.
const IMPRESSAO_DA_CONSULTA: &str =
    "0f0e0d0c0b0a09080706050403020100ffeeddccbbaa99887766554433221100";

#[tokio::test]
async fn um_ponto_escrito_sem_porta_e_procurado_na_porta_padrao() {
    let _vaga = vaga::minha();
    let (ponto, _) = ponto_na_porta_padrao();
    let marcas =
        Marcas::do_servidor(IMPRESSAO_DA_CONSULTA).expect("uma impressão digital dá marcas");
    let mut balde = [0_u8; TAMANHO];

    // Os dois moram no quarto. A resposta ao `MORO` sai depois do registro,
    // então esperar por ela é esperar o registro.
    let servidor = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    servidor
        .send_to(&moro(&marcas.servidor), ponto)
        .await
        .unwrap();
    servidor.recv_from(&mut balde).await.unwrap();
    let escuta = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    escuta.send_to(&moro(&marcas.escuta), ponto).await.unwrap();
    escuta.recv_from(&mut balde).await.unwrap();

    // O ponto como o link o carrega: sem porta.
    let sem_porta = ponto.ip().to_string();
    let achado = onde_mora_hoje(&sem_porta, &marcas, PRAZO_DO_QUARTO).await;

    assert_eq!(
        achado,
        OndeMora::Achado {
            servidor: Some(servidor.local_addr().unwrap()),
            escuta: Some(escuta.local_addr().unwrap()),
        },
        "o ponto escrito sem porta, que é como o `enc=` sai desde a v0.10.2, não foi \
         procurado na porta do ponto de encontro: a pergunta ao quarto nunca sai, e a lista \
         de servidores fica sem o endereço de hoje"
    );
}

/// A impressão digital do anfitrião de ponta a ponta.
///
/// As marcas saem dela dos dois lados: `Convocacao::para_servidor` no
/// anfitrião, e `seele_ffi::onde_mora_hoje` no cliente. O que se testa é que os
/// dois lados, cada um pelo seu caminho, chegam às mesmas.
const IMPRESSAO_DO_ANFITRIAO: &str =
    "5e1e0a0b0c0d0e0f00112233445566778899aabbccddeeff0011223344556677";

#[tokio::test]
async fn o_link_leva_ate_o_servidor_e_a_escuta_de_hoje() {
    let _vaga = vaga::minha();
    let (ponto, quarto) = ponto_na_porta_padrao();

    // O servidor como o de produção abre: `[::]`, com pilha dupla quando a
    // máquina deixa.
    let (socket, _) =
        seele_server::alcance::abrir_escuta(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)))
            .expect("a escuta do servidor abre");
    let socket = Arc::new(socket);

    // O ponto como `PONTO_PADRAO` é escrito: um nome sem porta.
    let sem_porta = ponto.ip().to_string();
    assert!(
        !sem_porta.contains(':'),
        "o ponto deste teste virou IPv6 ({sem_porta}), e um nome sem porta deixou de ser o que \
         ele mede"
    );

    let antes = quarto.quantos();
    let convocacao = Convocacao::para_servidor(
        Arc::clone(&socket),
        IMPRESSAO_DO_ANFITRIAO,
        sem_porta.clone(),
    )
    .expect("uma impressão digital dá uma convocação");
    let aberto = seele_server::alcance::encontro::abrir(&convocacao)
        .await
        .expect("o degrau 4 abre contra o ponto deste teste");
    let bilhete = aberto.bilhete();
    let aviso = bilhete.aviso().expect("o aviso do bilhete é um endereço");

    // O anfitrião registra as duas marcas na subida. A consulta que vem logo
    // depois não pode chegar ao ponto antes delas, ou este teste mede a ordem
    // do escalonador e não o produto. Sem o registro na subida, esta espera
    // vence em um segundo e a asserção de baixo falha.
    let ate = tokio::time::Instant::now() + Duration::from_secs(1);
    while quarto.quantos() < antes + 2 && tokio::time::Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Duas formas do mesmo ponto teriam de chegar:
    // - a que o bilhete deste anfitrião escreve no link;
    // - a que todo link emitido até a v0.15.0 carrega, e que a lista de
    //   conhecidos guardou: o nome cru, sem porta.
    //
    // Hoje as duas são a mesma: o `Bilhete` escreve o ponto como ele entrou na
    // convocação, sem porta. A segunda forma só é percorrida quando o bilhete
    // deixar de coincidir com ela, e não roda duas vezes o mesmo caso.
    let mut formas = vec![bilhete.ponto.clone()];
    if bilhete.ponto != sem_porta {
        formas.push(sem_porta.clone());
    }
    for ponto_do_link in formas {
        let achado = seele_ffi::onde_mora_hoje(&ponto_do_link, IMPRESSAO_DO_ANFITRIAO).await;
        // **A regra da escuta não cala o anfitrião de verdade.** O `connect`
        // só usa a escuta que o quarto deu quando o servidor da mesma resposta
        // mora no mesmo IP (`OndeMora::escuta_do_anfitriao`, o I1 da revisão
        // final do Plano 1). Os outros testes dela registram no quarto à mão:
        // os de unidade escrevem os endereços, e `ocupante_da_escuta.rs`
        // imita o datagrama do servidor. Este é o único em que quem registra
        // os dois sockets é o anfitrião de verdade (`abrir`, e o `registrar`
        // de `atender`). Uma mudança no anfitrião que registre o servidor e a
        // escuta por caminhos diferentes (outra família, outro socket), ou
        // uma regra que compare mais que o IP, faria o anfitrião de hoje ser
        // deixado de lado em silêncio: quem chega pelo quarto tentaria só o
        // aviso do link.
        //
        // Antes da asserção dos endereços, que também ficaria vermelha com o
        // servidor registrado noutra família: esta diz a consequência.
        assert_eq!(
            achado.escuta_do_anfitriao(),
            EscutaDoQuarto::DoAnfitriao(aviso),
            "o anfitrião de hoje deixou de ser avisado pela escuta do quarto: com o link \
             «{ponto_do_link}», a escuta que ele registrou não passou pela regra que a confere \
             com o servidor da mesma resposta, e quem chega pelo quarto manda o `LEVE` só ao \
             aviso do link, que pode ser de outra abertura. O quarto respondeu {achado:?}. `NaoConfirmada` com o servidor noutro IP é o \
             anfitrião registrando o servidor e a escuta por caminhos diferentes, ou a regra \
             comparando mais que o IP"
        );
        assert_eq!(
            achado,
            OndeMora::Achado {
                servidor: Some(aberto.publico()),
                escuta: Some(aviso),
            },
            "o link com o ponto «{ponto_do_link}» não levou aos dois endereços de hoje. Um \
             `PontoNaoResolve` é o ponto sem porta indo cru ao DNS; `escuta: None` é a escuta \
             registrada com outra marca; `NinguemMora` é o registro que não saiu na subida"
        );
    }

    // E largar, e não fechar: é o que acontece com o encontro quando uma
    // `Hospedagem` é descartada sem `encerrar`. A tarefa do degrau 4 tem de
    // soltar o socket do servidor, ou a porta fica presa até o app fechar.
    drop(convocacao);
    drop(aberto);
    let ate = tokio::time::Instant::now() + Duration::from_secs(1);
    while Arc::strong_count(&socket) > 1 && tokio::time::Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        Arc::strong_count(&socket),
        1,
        "largar o encontro deixou a tarefa do degrau 4 viva segurando o socket do servidor: a \
         porta fica presa, e hospedar de novo nela falha"
    );
}
