//! A hospedagem da sala pessoal (tarefa 4f do Plano 5): a segunda hospedagem do
//! app, em memória e sem MODs, com a identidade da pessoa, a campainha, e o
//! registro assinado no ponto de encontro.
//!
//! O que se prova aqui, de ponta a ponta:
//!
//! - a sala apresenta a chave da pessoa, e quem recebe entra nela pelo código;
//! - **um convidado atendido antes de quem recebe não fica com a sala**: na sala
//!   não há portaria, e o dono só conecta ao atender. Sem a conta do dono criada
//!   antes de aceitar, o primeiro atendido viraria a Comandante (o achado A2);
//! - a sala não manda a lista de membros (D-fio-4);
//! - a sala se registra assinada no ponto, e quem liga a acha pelo código;
//! - **a página acha a sala pelo sinal, com a impressão DTLS que ela apresenta**
//!   (ADR 0068): o que o ponto devolve é o certificado com que a sala atende;
//! - **três pessoas na mesma ligação** (ADR 0068): a terceira liga para o código
//!   de quem hospeda, é atendida com a conversa em curso, os três se ouvem, e
//!   todos leem o fim da chamada quando quem hospeda sai.
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

use ed25519_dalek::SigningKey;
use seele_core::confere_codigo::chave_do_pino;
use seele_core::encontro::{localizar3, Localizacao, PRAZO_DO_QUARTO};
use seele_core::enlace::{Aviso as AvisoDoEnlace, Destino, Enlace, Esperada};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::codigo::Codigo;
use seele_proto::control::{DisconnectReason, Permission, ServerMessage};
use seele_proto::encontro::{Marcas, Vizinhanca};
use seele_server::alcance::encontro::EstadoDoRegistro;
use seele_server::hospedagem::Hospedagem;
use seele_server::permissions::Permissions;
use seele_server::recepcao::Aviso;
use seele_server::tls::Identity;

mod par_cru;
mod vaga;

/// A semente de quem recebe.
const DONO: u8 = 1;
/// A semente de quem liga.
const CONVIDADO: u8 = 2;
/// A semente da terceira pessoa, que liga com a conversa em curso.
const TERCEIRA: u8 = 3;

fn chave(semente: u8) -> SigningKey {
    SigningKey::from_bytes(&[semente; 32])
}

fn publica(semente: u8) -> [u8; 32] {
    chave(semente).verifying_key().to_bytes()
}

fn codigo_de(semente: u8) -> Codigo {
    Codigo::da_chave(&publica(semente))
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

/// `127.0.0.1` na porta da sala: ela escuta em `[::]`.
fn local(sala: &Hospedagem) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], sala.endereco().port()))
}

/// Um par cru até a prova: quem liga ainda não lê o `Tocando` (tarefa 6e).
///
/// Desde a 0.17, pelo enlace (`par_cru`): a sala se apresenta com a chave dela
/// na `0x03`, e a prova D5 amarra a impressão dessa chave e o DTLS desta ponta.
type Par = par_cru::Par;

async fn par_cru(endereco: SocketAddr, semente: u8) -> Par {
    par_cru::ate_a_prova(endereco, &chave(semente), &format!("pessoa{semente:03}")).await
}

#[tokio::test]
async fn a_sala_apresenta_a_chave_da_pessoa_e_quem_recebe_entra_pelo_codigo() {
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    // Desde a 0.17 (D3) a sala não se apresenta mais por um certificado Ed25519
    // da semente: o DTLS dela é P-256 efêmero, e a identidade vai na
    // apresentação `0x03`, assinada pela chave da pessoa. A impressão que quem
    // liga por código confere é o SHA-256 dos 32 bytes dessa chave — e é ela,
    // e não a do certificado de antes, que a sala tem de dizer.
    let da_pessoa = chave(DONO).verifying_key().to_bytes();
    assert_eq!(
        sala.impressao_digital(),
        seele_proto::transport::certificate_fingerprint(&da_pessoa),
        "a sala não apresentou a identidade da pessoa"
    );
    assert_ne!(
        sala.impressao_digital(),
        Identity::da_semente(&[DONO; 32]).unwrap().fingerprint(),
        "a sala ainda diz a impressão do certificado Ed25519 da 0.16, que o DTLS não negocia"
    );
    assert!(sala.recepcao().is_some(), "a sala subiu sem campainha");

    let codigo = codigo_de(DONO);
    Enlace::conectar(
        Destino {
            servidor: local(&sala),
            nome_tls: seele_proto::transport::NOME_DA_SALA.into(),
            chave_do_pin: chave_do_pino(&codigo),
            apelido: "dona".into(),
            segredo: None,
            esperada: Some(Esperada::Codigo(codigo)),
            o_pino_prova_o_servidor: false,
            aceito: None,
        },
        chave(DONO),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await
    .expect("quem recebe não entrou na própria sala pelo código");
    sala.encerrar().await;
}

#[tokio::test]
async fn um_convidado_atendido_antes_de_quem_recebe_nao_fica_com_a_sala() {
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    let recepcao = sala.recepcao().unwrap();
    let mut avisos = recepcao.assinar();

    let mut convidado = par_cru(local(&sala), CONVIDADO).await;
    assert!(matches!(
        convidado.proximo().await.unwrap(),
        ServerMessage::Tocando { .. }
    ));
    let toque = loop {
        if let Aviso::Tocou { toque, .. } =
            tokio::time::timeout(Duration::from_secs(5), avisos.recv())
                .await
                .unwrap()
                .unwrap()
        {
            break toque;
        }
    };
    assert!(recepcao.atender(toque));
    assert!(matches!(
        convidado.proximo().await.unwrap(),
        ServerMessage::Session { .. }
    ));

    let banco = sala.persistence();
    let banco = banco.lock().await;
    let permissoes = Permissions::new(&banco);
    let do_convidado = permissoes
        .register_or_find(&publica(CONVIDADO), "pessoa002")
        .unwrap();
    assert!(
        !permissoes
            .may(do_convidado.id, Permission::AdministerServer)
            .unwrap(),
        "o primeiro atendido ficou com a sala de outra pessoa"
    );
    let do_dono = permissoes.register_or_find(&publica(DONO), "dona").unwrap();
    assert!(
        permissoes
            .may(do_dono.id, Permission::AdministerServer)
            .unwrap(),
        "quem recebe não é a Comandante da própria sala"
    );
}

/// Os quadros que chegam até `prazo` de silêncio.
async fn o_que_chega(par: &mut Par, prazo: Duration) -> Vec<ServerMessage> {
    let mut chegou = Vec::new();
    while let Ok(Ok(quadro)) = tokio::time::timeout(prazo, par.proximo()).await {
        chegou.push(quadro);
    }
    chegou
}

#[tokio::test]
async fn a_sala_nao_manda_a_lista_de_membros_e_um_servidor_guardado_manda() {
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    let mut dono = par_cru(local(&sala), DONO).await;
    let na_sala = o_que_chega(&mut dono, Duration::from_millis(800)).await;
    assert!(
        na_sala
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Session { .. })),
        "quem recebe não entrou: {na_sala:?}"
    );
    assert!(
        !na_sala
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Membros { .. })),
        "a sala mandou a lista de membros (D-fio-4): {na_sala:?}"
    );
    sala.encerrar().await;

    // O controle: um servidor guardado manda, e é o mesmo par cru que lê.
    let guardado = Hospedagem::iniciar(
        0,
        seele_server::persistence::Location::Memory,
        "Casa",
        None,
        |_| Ok(()),
    )
    .await
    .unwrap();
    let mut alguem = par_cru(
        SocketAddr::from(([127, 0, 0, 1], guardado.endereco().port())),
        DONO,
    )
    .await;
    let no_guardado = o_que_chega(&mut alguem, Duration::from_millis(800)).await;
    assert!(
        no_guardado
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Membros { .. })),
        "o controle não viu a lista: o teste de cima não prova nada. {no_guardado:?}"
    );
    guardado.encerrar().await;
}

#[tokio::test]
async fn a_sala_se_registra_assinada_e_quem_liga_a_acha_pelo_codigo() {
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
        "a sala não se registrou no ponto: {:?}",
        *registro.borrow()
    );

    // Pelo `QUEM3`: desde a 0.17 a sala registra pelo `MORO3` (ENC/3), e o
    // `QUEM2` de antes só acharia uma sala da linha 1.
    let achado = localizar3(
        &ponto.to_string(),
        &Marcas::da_sala(&codigo_de(DONO)),
        PRAZO_DO_QUARTO,
    )
    .await;
    let Localizacao::Achada {
        servidor: Some(onde),
        ..
    } = achado
    else {
        panic!("quem liga não achou a sala pelo código: {achado:?}");
    };
    assert_eq!(
        onde.port(),
        sala.endereco().port(),
        "o ponto deu outro endereço que o do servidor da sala"
    );
    sala.encerrar().await;
}

#[tokio::test]
async fn a_sala_que_fecha_diz_chamada_encerrada_e_nao_servidor_encerrando() {
    // O app que fecha se despede das duas hospedagens (tarefa 21). Para quem
    // está num servidor guardado, o servidor está encerrando; para quem está
    // numa ligação, a ligação acabou. A frase de cada um é outra, e a razão que
    // chega é o que a escolhe.
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    let mut dono = par_cru(local(&sala), DONO).await;
    let entrou = o_que_chega(&mut dono, Duration::from_millis(800)).await;
    assert!(
        entrou
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Session { .. })),
        "quem recebe não entrou: {entrou:?}"
    );

    sala.despedir(Duration::from_secs(2)).await;

    let depois = o_que_chega(&mut dono, Duration::from_millis(800)).await;
    assert!(
        depois.iter().any(|quadro| matches!(
            quadro,
            ServerMessage::Disconnecting {
                reason: seele_proto::control::DisconnectReason::ChamadaEncerrada
            }
        )),
        "a sala se despediu sem dizer que a ligação acabou: {depois:?}"
    );
}

// ---------------------------------------------------------------------------
// Três na mesma ligação (ADR 0068, decisão 4)
// ---------------------------------------------------------------------------

/// Liga para a sala de `DONO` pelo código, com o `Enlace` do produto. A chave
/// de `DONO` entra sem tocar; qualquer outra toca e espera quem recebe.
async fn ligar(sala: SocketAddr, semente: u8, apelido: &str) -> Enlace {
    let codigo = codigo_de(DONO);
    Enlace::conectar(
        Destino {
            servidor: sala,
            nome_tls: seele_proto::transport::NOME_DA_SALA.into(),
            chave_do_pin: chave_do_pino(&codigo),
            apelido: apelido.into(),
            segredo: None,
            esperada: Some(Esperada::Codigo(codigo)),
            o_pino_prova_o_servidor: false,
            aceito: None,
        },
        chave(semente),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await
    .unwrap_or_else(|falha| panic!("{apelido} não entrou na ligação: {falha:?}"))
}

/// Espera o toque de `semente` na recepção, e o atende.
async fn atender_quem_toca(
    recepcao: &seele_server::recepcao::Recepcao,
    avisos: &mut tokio::sync::broadcast::Receiver<Aviso>,
    semente: u8,
) {
    let toque = loop {
        let aviso = tokio::time::timeout(Duration::from_secs(10), avisos.recv())
            .await
            .expect("quem recebe não ficou sabendo que tocava")
            .unwrap();
        if let Aviso::Tocou { toque, codigo, .. } = aviso {
            if codigo == codigo_de(semente) {
                break toque;
            }
        }
    };
    assert!(recepcao.atender(toque), "o toque não estava mais tocando");
}

/// Um datagrama de voz bem-formado de quem tem este `ssrc`.
fn voz(de: &Enlace, seq: u16) -> Vec<u8> {
    let cabecalho = seele_proto::MediaHeader {
        ssrc: de.sessao().ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut bytes = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = cabecalho
        .encode_datagram(b"tres na mesma ligacao", &mut bytes)
        .expect("um datagrama bem-formado");
    bytes.truncate(tamanho);
    bytes
}

/// A voz de `de` chega a `para`? Tenta por até dez segundos, com sequências
/// novas a cada volta: sentar na sala é confirmado por silêncio, e a primeira
/// voz pode sair antes de o servidor ter sentado os dois. A máquina de quem
/// valida roda outras baterias junto, e por isso a margem é larga.
async fn ouve(de: &Enlace, para: &Enlace, base: u16) -> bool {
    let canal = para.media();
    let fim = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut seq = base;
    while tokio::time::Instant::now() < fim {
        let mandadas: Vec<Vec<u8>> = (0..3).map(|i| voz(de, seq.wrapping_add(i))).collect();
        seq = seq.wrapping_add(3);
        for uma in &mandadas {
            de.media().send(uma.clone()).expect("mandar a voz");
        }
        let volta = tokio::time::Instant::now() + Duration::from_millis(400);
        loop {
            let falta = volta.saturating_duration_since(tokio::time::Instant::now());
            if falta.is_zero() {
                break;
            }
            match tokio::time::timeout(falta, canal.next()).await {
                Ok(Ok(bytes)) if mandadas.contains(&bytes) => return true,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
    }
    false
}

/// Lê o enlace até a despedida, e devolve a razão dela. `None` quando ele
/// acaba sem dizer por quê, ou não diz nada em dez segundos.
async fn despedida(enlace: &mut Enlace) -> Option<DisconnectReason> {
    let fim = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let falta = fim.saturating_duration_since(tokio::time::Instant::now());
        if falta.is_zero() {
            return None;
        }
        match tokio::time::timeout(falta, enlace.proximo()).await {
            Ok(AvisoDoEnlace::Mensagem(mensagem)) => {
                if let ServerMessage::Disconnecting { reason } = *mensagem {
                    return Some(reason);
                }
            }
            Ok(AvisoDoEnlace::Encerrado(seele_core::enlace::Motivo::Moderado(razao))) => {
                return Some(razao);
            }
            Ok(AvisoDoEnlace::Encerrado(_)) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

/// **Três na mesma ligação** (ADR 0068, decisão 4): A liga para B, e B atende;
/// C liga para o código de B com a conversa em curso, e B atende; os três
/// recebem a voz uns dos outros; B sai, e A e C leem o fim da chamada.
///
/// É o grupo da 1.0, de ponta a ponta, com o `Enlace` do produto e a sala de
/// verdade. O protocolo não mudou para isto: o `Tocando`, a campainha e o teto
/// de 6 (`TETO_DA_LIGACAO`, ADR 0059) já existiam, e o que faltava era alguém
/// passar por aqui. Baixar o teto para 2 reprova este teste na ligação de C.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tres_na_mesma_ligacao_se_ouvem_e_leem_o_fim_quando_quem_hospeda_sai() {
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    let recepcao = sala.recepcao().expect("a sala subiu sem campainha");
    let mut avisos = recepcao.assinar();
    let onde = local(&sala);

    // A liga para B. B atende, e entra na própria sala (o que o app faz ao
    // atender): a chave de quem hospeda não toca.
    let a_ligando = tokio::spawn(ligar(onde, CONVIDADO, "ana"));
    atender_quem_toca(&recepcao, &mut avisos, CONVIDADO).await;
    let mut b = ligar(onde, DONO, "dona").await;
    let mut a = tokio::time::timeout(Duration::from_secs(10), a_ligando)
        .await
        .expect("quem foi atendido não entrou em 10 s")
        .unwrap();

    // C liga para o código de B com a conversa em curso, e B atende.
    let c_ligando = tokio::spawn(ligar(onde, TERCEIRA, "caio"));
    atender_quem_toca(&recepcao, &mut avisos, TERCEIRA).await;
    let mut c = tokio::time::timeout(Duration::from_secs(10), c_ligando)
        .await
        .expect("a terceira pessoa foi atendida e não entrou em 10 s")
        .unwrap();

    // Os três sentam na sala de voz da ligação.
    let sala_de_voz = b
        .sessao()
        .voice_rooms
        .first()
        .expect("a sala pessoal não tem sala de voz")
        .id;
    for enlace in [&a, &b, &c] {
        enlace
            .entrar_na_voice_room(sala_de_voz, None)
            .await
            .expect("sentar na sala da ligação");
    }

    // Os seis sentidos: cada um recebe a voz dos outros dois.
    let pares: [(&Enlace, &Enlace, &str, u16); 6] = [
        (&a, &b, "A → B", 100),
        (&b, &a, "B → A", 1_100),
        (&a, &c, "A → C", 2_100),
        (&c, &a, "C → A", 3_100),
        (&b, &c, "B → C", 4_100),
        (&c, &b, "C → B", 5_100),
    ];
    for (de, para, sentido, base) in pares {
        assert!(
            ouve(de, para, base).await,
            "numa ligação de três, a voz não chegou no sentido {sentido}"
        );
    }

    // B sai: quem hospeda saindo encerra a ligação, e os dois leem isso.
    b.sair().await;
    for (enlace, quem) in [(&mut a, "A"), (&mut c, "C")] {
        assert_eq!(
            despedida(enlace).await,
            Some(DisconnectReason::ChamadaEncerrada),
            "quem hospeda saiu, e {quem} não leu o fim da chamada"
        );
    }
    sala.encerrar().await;
}

// ---------------------------------------------------------------------------
// A página acha a sala pelo sinal (ADR 0068)
// ---------------------------------------------------------------------------

/// O ponto de encontro com o sinal HTTP do navegador, sobre o mesmo quarto.
fn subir_o_ponto_com_o_sinal() -> (SocketAddr, SocketAddr) {
    let quarto = Arc::new(seele_encontro::Quarto::novo());
    let ponto = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Vizinhanca::TambemAqui,
        Arc::clone(&quarto),
    )
    .expect("a escuta tem que abrir");
    let onde = ponto.endereco().expect("o socket sabe onde ligou");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    let sinal = seele_encontro::sinal::Sinal::abrir(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        quarto,
        Vizinhanca::TambemAqui,
        vec!["*".to_owned()],
    )
    .expect("o sinal tem que abrir");
    let do_sinal = sinal.endereco().expect("o sinal sabe onde ligou");
    std::thread::spawn(move || {
        let _ = sinal.servir();
    });
    (onde, do_sinal)
}

/// O `POST /sinal/1` de uma página, e o corpo da resposta.
fn chamar_pelo_sinal(sinal: SocketAddr, corpo: &str) -> String {
    use std::io::{Read as _, Write as _};
    let mut conexao = std::net::TcpStream::connect(sinal).expect("o sinal não aceitou");
    conexao
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        conexao,
        "POST /sinal/1 HTTP/1.1\r\nHost: x\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{corpo}",
        corpo.len()
    )
    .unwrap();
    let mut texto = String::new();
    conexao.read_to_string(&mut texto).unwrap();
    texto
        .split_once("\r\n\r\n")
        .map(|(_, corpo)| corpo.to_owned())
        .unwrap_or_default()
}

/// **O que o sinal devolve para o código de uma pessoa é o certificado DTLS com
/// que a sala dela atende** (ADR 0068), de ponta a ponta: a sala de verdade se
/// registra, a página chama pelo sinal com as marcas do código, e a impressão
/// que volta é a do certificado que a sala apresenta a quem disca.
///
/// É essa impressão que o navegador escreve como `a=fingerprint` antes de
/// fechar o DTLS. Uma que não fosse a do certificado de verdade daria «a
/// conexão segura não se verificou» em toda ligação da página.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn o_sinal_devolve_a_impressao_do_certificado_dtls_com_que_a_sala_atende() {
    let _vaga = vaga::minha();
    let (ponto, sinal) = subir_o_ponto_com_o_sinal();
    let sala = subir_a_sala(Some(ponto.to_string())).await;
    let mut registro = sala
        .registro_no_quarto()
        .expect("a sala subiu sem registro no ponto");
    tokio::time::timeout(
        Duration::from_secs(10),
        registro.wait_for(|estado| *estado == EstadoDoRegistro::Registrado),
    )
    .await
    .expect("a sala não se registrou no ponto em 10 s")
    .unwrap();

    let marcas = Marcas::da_sala(&codigo_de(DONO));
    let pedido = seele_proto::encontro::pedido_do_sinal(&seele_proto::encontro::Chama {
        servidor: marcas.servidor().clone(),
        escuta: marcas.escuta().clone(),
        credencial: seele_proto::encontro::Credencial::nova(&"a".repeat(32)).unwrap(),
        candidatos: vec![SocketAddr::from(([127, 0, 0, 1], 51_000))],
    });
    let corpo = tokio::task::spawn_blocking(move || chamar_pelo_sinal(sinal, &pedido))
        .await
        .unwrap();
    let Some(seele_proto::encontro::RespostaDoSinal::Aqui {
        endereco,
        impressao,
        ..
    }) = seele_proto::encontro::ler_resposta_do_sinal(&corpo)
    else {
        panic!("a página não achou a sala pelo sinal: {corpo:?}");
    };
    assert_eq!(
        endereco.port(),
        sala.endereco().port(),
        "o sinal deu outro endereço que o do servidor da sala"
    );
    let dita = impressao.expect("o sinal achou a sala e não devolveu a impressão DTLS dela");

    // O certificado que a sala apresenta a quem disca, lido do próprio enlace.
    let (_ponta, conexao) = par_cru::discar(local(&sala)).await;
    assert_eq!(
        dita,
        conexao.impressao_dtls_do_par(),
        "a impressão que o sinal devolve não é a do certificado DTLS com que a sala atende"
    );
    sala.encerrar().await;
}
