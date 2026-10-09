//! O quarto perguntado **junto** com a corrida, e não antes dela (a pendência
//! 55, a metade de núcleo da tarefa 14b da F7).
//!
//! Quem volta a um servidor guardado pergunta ao quarto onde ele mora hoje. O
//! `connect` do app perguntava primeiro e só depois corria: com o ponto ou o DNS
//! mudos, até 1,5 s de prazo do quarto, pagos também por quem estava na mesma
//! casa. A `Chegada` com o quarto (`Chegada::com_o_quarto`) pergunta junto:
//!
//! - quem está na mesma casa entra sem esperar um quarto mudo;
//! - um servidor que mudou de endereço entra pelo endereço de hoje, que chega
//!   à corrida depois de ela começar;
//! - e a trilha de uma volta que não entrou termina em `Desistiu` mesmo com o
//!   candidato que chegou depois: só o último candidato desiste, e a conta dos
//!   candidatos sobe quando ele chega.
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
use std::time::{Duration, Instant};

use seele_core::chegada::{Chegada, Etapa, Passo, Quarto};
use seele_core::encontro::{onde_mora_hoje, Marcas, PRAZO_DO_QUARTO};
use seele_core::enlace::{Destino, Esperada};
use seele_core::{MemoryPinStore, PinStore, SigningKey};
use seele_proto::encontro::{moro, Vizinhanca, TAMANHO};
use seele_proto::uri::Bilhete;
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;

mod vaga;

fn chave() -> SigningKey {
    SigningKey::from_bytes(&[42; 32])
}

fn pins() -> Arc<dyn PinStore> {
    Arc::new(MemoryPinStore::new())
}

/// Um destino guardado, conferido pela impressão: o que a FFI monta
/// (`build_destino`) para um endereço da lista.
fn guardado(servidor: SocketAddr, impressao: &str) -> Destino {
    Destino {
        servidor,
        nome_tls: "localhost".into(),
        chave_do_pin: servidor.to_string(),
        apelido: "marcela".into(),
        segredo: None,
        esperada: Some(Esperada::Certificado(impressao.to_owned())),
        o_pino_prova_o_servidor: false,
        aceito: None,
    }
}

/// O quarto desta volta, que monta o endereço de hoje como os outros.
fn quarto(impressao: &str) -> Quarto {
    let impressao_do_candidato = impressao.to_owned();
    Quarto::novo(impressao, move |endereco| {
        guardado(endereco, &impressao_do_candidato)
    })
}

/// Um endereço onde ninguém atende: um socket aberto e devolvido.
fn morto() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
    let endereco = socket.local_addr().expect("o socket sabe onde ligou");
    drop(socket);
    endereco
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

/// `$SEELE_ENCONTRO` trocada até o guarda cair, **inclusive em pânico**. O
/// molde é o `AmbienteDoEncontro` de `quarto_pelo_link.rs`; quem serializa é a
/// vaga, e por isso este guarda é criado logo depois dela.
#[must_use = "a variável só volta quando o guarda cai"]
struct Encontro {
    antes: Option<std::ffi::OsString>,
}

impl Encontro {
    fn apontar(valor: &str) -> Self {
        let antes = std::env::var_os(seele_server::alcance::encontro::VARIAVEL);
        std::env::set_var(seele_server::alcance::encontro::VARIAVEL, valor);
        Self { antes }
    }
}

impl Drop for Encontro {
    fn drop(&mut self) {
        match self.antes.take() {
            Some(valor) => std::env::set_var(seele_server::alcance::encontro::VARIAVEL, valor),
            None => std::env::remove_var(seele_server::alcance::encontro::VARIAVEL),
        }
    }
}

async fn subir_o_servidor() -> Hospedagem {
    Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
        .await
        .expect("o servidor não subiu")
}

/// `127.0.0.1` na porta do servidor: ele escuta em `[::]`.
fn local(hospedagem: &Hospedagem) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], hospedagem.endereco().port()))
}

/// Os nomes da trilha, em ordem.
fn nomes(trilha: &[Passo]) -> Vec<&'static str> {
    trilha.iter().map(|passo| passo.etapa.nome()).collect()
}

#[tokio::test]
async fn quem_esta_na_mesma_casa_entra_sem_esperar_um_quarto_mudo() {
    // A pendência 55: o ponto mudo custava o prazo inteiro do quarto antes de
    // a corrida começar, e quem estava na mesma casa pagava junto.
    let _vaga = vaga::minha();
    let _sem_degrau_4 = Encontro::apontar("nao");
    let servidor = subir_o_servidor().await;
    let impressao = servidor.impressao_digital().to_owned();
    let mudo = std::net::UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
    let bilhete =
        Bilhete::novo(mudo.local_addr().unwrap().to_string(), "198.51.100.4:41000").unwrap();

    let comeco = Instant::now();
    let chegada = Chegada::nova(vec![guardado(local(&servidor), &impressao)], Some(bilhete))
        .com_o_quarto(quarto(&impressao));
    let chegado = chegada
        .chegar(chave(), pins())
        .await
        .unwrap_or_else(|frustrada| {
            panic!(
                "quem está na mesma casa não entrou: {:?} depois de {:?}",
                frustrada.motivo(),
                frustrada.trilha()
            )
        });
    let levou = comeco.elapsed();

    assert!(
        levou < PRAZO_DO_QUARTO,
        "a entrada pela rede de casa levou {levou:?}, e o prazo do quarto é {PRAZO_DO_QUARTO:?}: \
         ela esperou o quarto mudo"
    );
    drop(chegado);
    drop(mudo);
    servidor.encerrar().await;
}

#[tokio::test]
async fn um_servidor_que_mudou_de_endereco_entra_pelo_que_o_quarto_deu() {
    // O endereço guardado morreu (o roteador deu outra porta ao servidor), e o
    // de hoje só chega pelo quarto, com a corrida já andando. A corrida de
    // antes acabava com a lista dela, e ele chegaria a uma corrida que já tinha
    // terminado.
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let _encontro = Encontro::apontar(&ponto.to_string());
    let servidor = subir_o_servidor().await;
    let impressao = servidor.impressao_digital().to_owned();

    // O servidor se registra na subida, se o degrau 4 é tentado nesta máquina.
    let marcas = Marcas::do_servidor(&impressao).expect("a impressão dá marcas");
    let ate = Instant::now() + Duration::from_secs(5);
    let hoje = loop {
        let achado = onde_mora_hoje(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await;
        if let Some(endereco) = achado.servidor() {
            break endereco;
        }
        assert!(
            Instant::now() < ate,
            "o servidor não se registrou no ponto ({achado:?}): o degrau 4 não foi tentado \
             nesta máquina, e este teste precisa dele"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(hoje.port(), servidor.endereco().port());

    let Some(bilhete) = seele_proto::uri::analisar(&servidor.convite())
        .expect("o convite se lê")
        .bilhete
    else {
        panic!("o convite saiu sem o `enc=`");
    };
    let ontem = morto();

    let chegada = Chegada::nova(vec![guardado(ontem, &impressao)], Some(bilhete))
        .com_o_quarto(quarto(&impressao));
    let chegado = chegada
        .chegar(chave(), pins())
        .await
        .unwrap_or_else(|frustrada| {
            panic!(
                "o endereço de hoje se perdeu: {:?} depois de {:?}",
                frustrada.motivo(),
                frustrada.trilha()
            )
        });

    let tentados: Vec<SocketAddr> = chegado
        .trilha
        .iter()
        .filter_map(|passo| match passo.etapa {
            Etapa::Tentando { onde, .. } => Some(onde),
            _ => None,
        })
        .collect();
    assert_eq!(tentados, vec![ontem, hoje], "{:?}", chegado.trilha);
    assert_eq!(nomes(&chegado.trilha).last(), Some(&"Dentro"));
    drop(chegado);
    servidor.encerrar().await;
}

#[tokio::test]
async fn a_trilha_de_uma_volta_que_nao_entrou_acaba_no_candidato_que_chegou_depois() {
    // Só o último candidato desiste (`Etapa::transicao_legal`). Com o endereço
    // de hoje chegando depois, a conta dos candidatos tem de subir junto; sem
    // isso, a desistência dele seria uma transição que a máquina não tem, e a
    // trilha acabaria sem dizer que acabou.
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let impressao = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";
    let marcas = Marcas::do_servidor(impressao).expect("a impressão dá marcas");

    // O registro de um servidor que caiu logo depois: o quarto dá um endereço
    // de hoje onde ninguém atende.
    let fantasma = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let de_hoje = fantasma.local_addr().unwrap();
    let mut balde = [0_u8; TAMANHO];
    fantasma
        .send_to(&moro(marcas.servidor()), ponto)
        .await
        .unwrap();
    fantasma.recv_from(&mut balde).await.unwrap();
    drop(fantasma);
    assert_eq!(
        onde_mora_hoje(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO)
            .await
            .servidor(),
        Some(de_hoje)
    );

    let bilhete = Bilhete::novo(ponto.to_string(), "198.51.100.4:41000").unwrap();
    let ontem = morto();
    let chegada = Chegada::nova(vec![guardado(ontem, impressao)], Some(bilhete))
        .com_o_quarto(quarto(impressao));
    let Err(frustrada) = chegada.chegar(chave(), pins()).await else {
        panic!("dois endereços mortos deixaram alguém entrar");
    };

    let trilha = frustrada.trilha();
    assert_eq!(
        nomes(trilha),
        ["Parada", "Avisando", "Tentando", "Tentando", "Desistiu"],
        "{trilha:?}"
    );
    assert!(
        matches!(
            trilha.get(3).map(|passo| &passo.etapa),
            Some(Etapa::Tentando {
                candidato: 1,
                de: 2,
                onde,
                ..
            }) if *onde == de_hoje
        ),
        "o endereço de hoje não entrou como o segundo de dois: {trilha:?}"
    );
}
