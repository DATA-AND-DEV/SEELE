//! **Uma transmissão que cai acaba de verdade, e diz por quê.**
//!
//! # O defeito
//!
//! Medido num Windows com o codificador de software forçado: a bomba de tela
//! morre no meio da transmissão, sai um `warn "a transmissão de tela caiu"` no
//! `seele.log`, e mais nada. O botão segue em PARAR DE COMPARTILHAR, o
//! servidor segue anunciando a tela à sala, e quem entra depois recebe o
//! `ScreenShareStarted` de uma transmissão que não tem imagem. O produto sabia
//! e não contava.
//!
//! # O que este arquivo cobra, com o servidor de verdade
//!
//! Quando a tarefa que escoa a transmissão acaba **com erro** e ninguém mandou
//! parar:
//!
//! 1. **o servidor fica sabendo** — o registro dele deixa de ter a transmissão,
//!    e quem está na sala recebe o `ScreenShareStopped` de uma parada comum;
//! 2. **o estado local sai de «compartilhando»** — o retrato de quem
//!    transmitia, dobrado das mensagens como a casca o dobra, não tem mais a
//!    própria transmissão;
//! 3. **o motivo chega a quem desenha** — um `Aviso::TelaCaiu` com a
//!    transmissão, o que ela mostrava e o motivo enumerado.
//!
//! # Dois casos, e por que o primeiro não precisa de codec
//!
//! O módulo do Cisco não mora neste repositório (é a licença que impõe), e a
//! integração contínua não o tem: um teste que só rodasse com ele seria um
//! guarda que existe e não funciona. Por isso o primeiro caso derruba a
//! transmissão por uma porta que não passa pelo codec — a câmera
//! (`seele_core::de_fora`) pedida com medidas que o cabeçalho de abertura
//! recusa. O servidor já anunciou a transmissão quando o fluxo não abre; é o
//! mesmo silêncio, e o mesmo conserto.
//!
//! O segundo é o do relato: a bomba de verdade, com o codificador recusando um
//! quadro no meio. Pula, com o motivo impresso, onde o módulo não está.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{
    Cadencia, Captura, CapturaEmCaixa, CapturaRecusou, Client, FonteDeQuadros, LimitesDeTela,
    MemoryPinStore, MotivoDeQueda, PedidoDeTela, PinStore, Resolucao, Room,
};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{ScreenId, VoiceRoomId};
use seele_proto::screen::ScreenSource;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// Quanto cada passo que fala com a rede tem para terminar. Sem prazo próprio
/// um teste destes não reprova: pendura.
const PRAZO: Duration = Duration::from_secs(20);

async fn com_prazo<F, T, E>(o_que: &str, passo: F) -> Result<T>
where
    F: Future<Output = std::result::Result<T, E>>,
    E: Into<anyhow::Error>,
{
    match tokio::time::timeout(PRAZO, passo).await {
        Ok(pronto) => pronto.map_err(Into::into),
        Err(_) => panic!("{o_que} não terminou em {PRAZO:?}: é a máquina, e não o que se mede"),
    }
}

/// Um servidor numa porta que o sistema escolhe. Quem transmite (a semente 71)
/// é o dono declarado, que pode compartilhar.
async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let dono = SigningKey::from_bytes(&[71; 32]);
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        proprietario: Some(seele_core::key_fingerprint(dono.verifying_key().as_bytes())),
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let aceitando = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, daemon))
}

/// Quem transmite: o enlace de verdade, com o motor que este arquivo mede.
async fn quem_transmite(endereco: SocketAddr) -> Result<Enlace> {
    Ok(Enlace::conectar(
        Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: "transmite".into(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        SigningKey::from_bytes(&[71; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?)
}

/// Quem está na sala: uma conexão crua, que só anota o que o servidor diz.
async fn quem_assiste(endereco: SocketAddr) -> Result<Client> {
    Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "assiste",
        &SigningKey::from_bytes(&[75; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await
    .map_err(Into::into)
}

/// O que a casca de quem transmite ficou sabendo, na ordem.
#[derive(Default)]
struct Ouvido {
    /// O retrato, dobrado das mensagens como a casca o dobra.
    sala: Room,
    /// As aberturas do próprio espelho: a transmissão chegou a sair.
    abriu: Vec<ScreenId>,
    /// As quedas ditas: (tela, o que mostrava, por quê).
    caiu: Vec<(ScreenId, ScreenSource, MotivoDeQueda)>,
    /// Se a queda foi dita depois de o espelho abrir.
    caiu_depois_de_abrir: bool,
}

impl Ouvido {
    fn ouvir(&mut self, aviso: Aviso) {
        match aviso {
            Aviso::Mensagem(mensagem) => {
                let _ = self.sala.apply(&mensagem);
            }
            Aviso::TelaAbriu { tela, .. } => self.abriu.push(tela),
            Aviso::TelaCaiu {
                tela,
                origem,
                motivo,
            } => {
                self.caiu_depois_de_abrir = self.abriu.contains(&tela);
                self.caiu.push((tela, origem, motivo));
            }
            _ => {}
        }
    }

    /// Ouve o enlace até a condição valer, ou até o prazo acabar.
    async fn ate(&mut self, enlace: &mut Enlace, pronto: impl Fn(&Self) -> bool) -> bool {
        let fim = tokio::time::Instant::now() + PRAZO;
        while tokio::time::Instant::now() < fim {
            if pronto(self) {
                return true;
            }
            if let Ok(aviso) =
                tokio::time::timeout(Duration::from_millis(100), enlace.proximo()).await
            {
                self.ouvir(aviso);
            }
        }
        pronto(self)
    }
}

/// O que a sala ouviu sobre transmissões, na ordem: `(começou, tela)`.
async fn o_que_a_sala_ouviu(
    assiste: &mut Client,
    ate: impl Fn(&[(bool, ScreenId)]) -> bool,
) -> Vec<(bool, ScreenId)> {
    let mut ouvido = Vec::new();
    let fim = tokio::time::Instant::now() + PRAZO;
    while tokio::time::Instant::now() < fim && !ate(&ouvido) {
        match tokio::time::timeout(Duration::from_millis(200), assiste.next_event()).await {
            Ok(Ok(ServerMessage::ScreenShareStarted { screen, .. })) => ouvido.push((true, screen)),
            Ok(Ok(ServerMessage::ScreenShareStopped { screen, .. })) => {
                ouvido.push((false, screen));
            }
            Ok(Err(erro)) => panic!("a conexão de quem assiste caiu: {erro}"),
            _ => {}
        }
    }
    ouvido
}

async fn esperar<F, Fut>(mut condicao: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let limite = tokio::time::Instant::now() + PRAZO;
    loop {
        if condicao().await {
            return true;
        }
        if tokio::time::Instant::now() >= limite {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A encenação comum: os dois na sala, o pedido feito, a queda — e as três
/// cobranças. Devolve o que a casca de quem transmite ouviu, para o caso
/// conferir o motivo dele.
async fn a_transmissao_cai(pedido: PedidoDeTela, limites: LimitesDeTela) -> Result<Ouvido> {
    let (endereco, daemon) = servidor().await?;

    let mut transmite = com_prazo(
        "o aperto de mão de quem transmite",
        quem_transmite(endereco),
    )
    .await?;
    let mut ouvido = Ouvido::default();
    ouvido.sala.adopt(transmite.sessao(), "transmite");
    com_prazo(
        "a entrada de quem transmite na sala",
        transmite.entrar_na_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    ouvido.sala.enter_voice_room(VoiceRoomId(1));

    let mut assiste = com_prazo("o aperto de mão de quem assiste", quem_assiste(endereco)).await?;
    com_prazo(
        "a entrada de quem assiste na sala",
        assiste.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;

    let (pessoa, sessao) = (transmite.sessao().person, transmite.sessao().id);
    com_prazo(
        "o pedido de compartilhar",
        transmite.compartilhar_tela(pedido, limites),
    )
    .await?;

    // **A sala ouviu começar e ouviu acabar, sem ninguém apertar parar.** Antes
    // do conserto o segundo nunca chegava: a transmissão ficava anunciada, e
    // quem assistia olhava o palco de alguém sem imagem.
    let da_sala = o_que_a_sala_ouviu(&mut assiste, |ouvido| {
        ouvido.iter().any(|(comecou, _)| !comecou)
    })
    .await;
    let Some(&(true, tela)) = da_sala.first() else {
        panic!("a sala não ouviu a transmissão começar: {da_sala:?}; o teste não mediria queda nenhuma");
    };
    assert_eq!(
        da_sala,
        vec![(true, tela), (false, tela)],
        "a transmissão caiu e a sala não ouviu o fim dela: continuaria desenhada uma tela que \
         ninguém está mandando"
    );

    // E o registro do servidor não a tem mais: quem entrar na sala agora não
    // recebe o anúncio de uma transmissão morta.
    assert!(
        esperar(|| async {
            daemon
                .server()
                .telas
                .lock()
                .await
                .de(pessoa, sessao)
                .is_none()
        })
        .await,
        "o servidor continua com a transmissão registrada depois de ela cair"
    );

    // **A casca ficou sabendo, com o motivo**, e uma vez só.
    assert!(
        ouvido
            .ate(&mut transmite, |ouvido| !ouvido.caiu.is_empty())
            .await,
        "a transmissão caiu e nenhum `Aviso::TelaCaiu` chegou a quem desenha: o produto soube \
         e não contou"
    );
    assert_eq!(
        ouvido.caiu.len(),
        1,
        "a mesma queda foi dita mais de uma vez: {:?}",
        ouvido.caiu
    );
    assert_eq!(
        ouvido.caiu[0].0, tela,
        "a queda dita é de outra transmissão"
    );

    // **E o estado local saiu de «compartilhando»**: o retrato de quem
    // transmitia não tem mais a própria transmissão. É dele que o botão lê
    // COMPARTILHAR ou PARAR DE COMPARTILHAR.
    assert!(
        ouvido
            .ate(&mut transmite, |ouvido| {
                seele_core::retrato::minha_tela_de(&ouvido.sala, None).is_none()
            })
            .await,
        "a transmissão caiu e o retrato de quem transmitia continua dizendo que ela compartilha: \
         {:?}",
        ouvido.sala.telas
    );

    // A sessão segue: cair a tela não é cair o enlace.
    assert_eq!(
        transmite.estado(),
        seele_core::Link::Online,
        "a queda da transmissão levou o enlace junto"
    );
    transmite.sair().await;
    Ok(ouvido)
}

/// O fluxo que não abre: a transmissão é anunciada, e o cabeçalho de abertura
/// é recusado. **Sem codec.**
#[tokio::test(flavor = "multi_thread")]
async fn a_transmissao_cujo_fluxo_nao_abre_acaba_e_diz_por_que() -> Result<()> {
    let _vaga = vaga::minha();
    // Medidas que `ScreenHeader::check` recusa. A ponte (`seele-ffi`) as
    // recusa antes do pedido; aqui o pedido vai direto ao enlace, que é a
    // parede que não depende de quem chama.
    let (_entrada, quadros) = seele_core::de_fora::canal_de_fora(0, 0);
    let ouvido = a_transmissao_cai(PedidoDeTela::DeFora(quadros), LimitesDeTela::default()).await?;
    assert_eq!(
        (ouvido.caiu[0].1, ouvido.caiu[0].2),
        (ScreenSource::Camera, MotivoDeQueda::Fluxo),
        "o fluxo não abriu, e o motivo dito foi outro"
    );
    Ok(())
}

/// Entrega `bons` quadros do tamanho pedido e depois um do tamanho errado, que
/// o codificador recusa.
#[derive(Debug)]
struct FonteQueEntorta {
    largura: usize,
    altura: usize,
    bons: usize,
    passo: AtomicUsize,
}

fn quadro(largura: usize, altura: usize, passo: usize) -> seele_video::codec::QuadroI420 {
    let mut luma = Vec::with_capacity(largura * altura);
    for linha in 0..altura {
        for coluna in 0..largura {
            // Bordas duras: um quadro chapado sairia com trinta bytes.
            let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
            luma.push(if claro { 235 } else { 16 });
        }
    }
    let croma = vec![128; largura.div_ceil(2) * altura.div_ceil(2)];
    seele_video::codec::QuadroI420::novo(largura, altura, luma, croma.clone(), croma)
        .expect("um I420 bem-formado")
}

impl FonteDeQuadros for FonteQueEntorta {
    fn tomar(&self) -> Option<seele_video::codec::QuadroI420> {
        let passo = self.passo.fetch_add(1, Ordering::Relaxed);
        if passo < self.bons {
            Some(quadro(self.largura, self.altura, passo))
        } else {
            Some(quadro(self.largura / 2, self.altura / 2, passo))
        }
    }
}

#[derive(Debug)]
struct CapturaQueEntorta;

impl Captura for CapturaQueEntorta {
    type Fonte = FonteQueEntorta;

    fn iniciar(
        &mut self,
        resolucao: Resolucao,
        _cadencia: Cadencia,
    ) -> std::result::Result<Self::Fonte, CapturaRecusou> {
        Ok(FonteQueEntorta {
            largura: resolucao.largura(),
            altura: resolucao.altura(),
            bons: 6,
            passo: AtomicUsize::new(0),
        })
    }
}

/// O módulo do Cisco, ou `None` com o motivo impresso. A mesma decisão de
/// `seele_core::bomba`: pula em vez de falhar, e `SEELE_EXIGE_CODEC` faz
/// faltar virar falha onde há módulo publicado.
fn biblioteca() -> Option<seele_video::BibliotecaDeVideo> {
    let mut pastas = Vec::new();
    if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
        let caminho = std::path::PathBuf::from(apontado);
        pastas.push(if caminho.is_dir() {
            caminho
        } else {
            caminho.parent().map_or_else(|| caminho.clone(), Into::into)
        });
    }
    pastas.push(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target"),
    );
    match seele_video::BibliotecaDeVideo::procurar_e_carregar(&pastas) {
        Ok(biblioteca) => Some(biblioteca),
        Err(motivo) => {
            assert!(
                std::env::var_os("SEELE_EXIGE_CODEC").is_none()
                    || seele_video::modulo::publicado_para_este_sistema().is_none(),
                "SEELE_EXIGE_CODEC está ligado, este sistema tem módulo publicado e ele não \
                 está aqui: {motivo}"
            );
            eprintln!(
                "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe \
                 isso. Aponte-o com SEELE_OPENH264.\n  O caso sem codec deste arquivo \
                 (`a_transmissao_cujo_fluxo_nao_abre_acaba_e_diz_por_que`) cobra o mesmo motor."
            );
            None
        }
    }
}

/// O caso do relato: a bomba de verdade, e o codificador recusando um quadro
/// **no meio** da transmissão.
#[tokio::test(flavor = "multi_thread")]
async fn a_bomba_que_morre_no_meio_acaba_a_transmissao_e_diz_que_foi_o_codificador() -> Result<()> {
    let _vaga = vaga::minha();
    let Some(biblioteca) = biblioteca() else {
        return Ok(());
    };
    let pedido = PedidoDeTela::DoSistema {
        biblioteca,
        captura: CapturaEmCaixa::nova(CapturaQueEntorta),
        origem: ScreenSource::Monitor,
    };
    let ouvido = a_transmissao_cai(pedido, LimitesDeTela::default()).await?;
    assert_eq!(
        (ouvido.caiu[0].1, ouvido.caiu[0].2),
        (ScreenSource::Monitor, MotivoDeQueda::Codificador),
        "o codificador recusou um quadro, e o motivo dito foi outro"
    );
    assert!(
        ouvido.caiu_depois_de_abrir,
        "a queda veio antes de o fluxo abrir: o caso é o do meio da transmissão"
    );
    Ok(())
}
