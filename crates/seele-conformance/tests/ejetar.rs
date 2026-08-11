//! Ejetar e voltar, no mesmo processo.
//!
//! A pendência #9 recusou trocar a conexão por baixo de uma sessão viva. O laço
//! externo do `plug` faz outra coisa: derruba tudo e reconstrói — é assim que
//! `:ejetar` volta para a tela de seleção em vez de matar o processo. Este
//! teste existe para provar que esse teardown fecha de verdade. Se ele falhar,
//! a decisão do laço estava errada, e é melhor descobrir aqui do que meses
//! depois no terminal de alguém.
//!
//! # O que este teste **não** afirma
//!
//! Não afirma que a conexão sumiu no instante em que a alça foi solta. Não
//! sumiu: `Drop for Enlace` é um `abort()`, que é assíncrono, e `Drop for Voice`
//! só levanta uma bandeira de parada sem esperar a thread que segura uma cópia
//! da conexão. Afirmar o contrário daria um teste que passa nesta máquina e
//! falha na integração contínua — e a culpa cairia no código, não no teste.
//!
//! O que dá para afirmar, e é o que importa para o laço, é: **reconectar
//! funciona**, e **encerrar a hospedagem devolve a porta**. Um teardown que
//! deixasse algo preso reprovaria numa dessas duas.
//!
//! A voz fica de fora de propósito: `Voice::start` abre um dispositivo de áudio
//! real, que não existe em máquina de integração contínua, e o que o laço
//! precisa saber dela — que soltar a alça não impede a próxima conexão — é o
//! que os testes abaixo medem pelo lado que se pode observar.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{Link, MemoryPinStore};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{CageId, ClientMessageId, LineId};
use seele_server::casper::Location;
use seele_server::hospedagem::Hospedagem;
use seele_server::{DogmaConfig, Server};

const CAGE: u32 = 1;
const LINE: u32 = 1;

/// Sobe um Dogma numa porta que o sistema escolhe.
///
/// Porta zero, e nunca um número escrito à mão: um número fixo colidiria com o
/// Dogma que a pessoa deixou rodando na própria máquina, ou com outro teste
/// desta mesma bateria rodando em paralelo.
async fn dogma() -> Result<(SocketAddr, Arc<Server>)> {
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

fn destino(endereco: SocketAddr) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: "ayanami".into(),
        segredo: None,
    }
}

/// Espera um aviso que interesse, ou desiste.
async fn esperar<F>(enlace: &mut Enlace, prazo: Duration, mut aceita: F) -> Option<Aviso>
where
    F: FnMut(&Aviso) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(300), enlace.proximo()).await {
            Ok(aviso) if aceita(&aviso) => return Some(aviso),
            Ok(_) | Err(_) => {}
        }
    }
    None
}

/// Conecta e prova que a sessão **serve**, e não só que o construtor devolveu
/// `Ok`.
///
/// Entrar no Cage, abrir a Linha, dizer algo e ouvir de volta é o menor
/// caminho que passa pelo Dogma inteiro. Uma conexão que só existisse no papel
/// falharia aqui, e é justamente essa a diferença que este arquivo precisa
/// enxergar na segunda volta.
async fn conectar_e_falar(endereco: SocketAddr, semente: u8, o_que: &str) -> Result<Enlace> {
    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.inserir_plug(CageId(CAGE)).await?;
    enlace.abrir_linha(LineId(LINE)).await?;
    enlace
        .dizer(LineId(LINE), o_que.to_owned(), ClientMessageId(1))
        .await?;

    let ouviu = esperar(&mut enlace, Duration::from_secs(15), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(&**mensagem, ServerMessage::MessageReceived { body, .. } if body == o_que)
        )
    })
    .await;
    assert!(
        ouviu.is_some(),
        "a sessão conectou e não fala: «{o_que}» não voltou do Dogma"
    );

    Ok(enlace)
}

#[tokio::test(flavor = "multi_thread")]
async fn conectar_ejetar_e_conectar_de_novo_no_mesmo_processo() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    let primeiro = conectar_e_falar(endereco, 46, "primeira volta").await?;
    assert!(primeiro.sessao().pilot.0 > 0, "a primeira sessão não subiu");
    assert_eq!(primeiro.estado(), Link::Online);
    let primeira = primeiro.sessao().id;

    // Ejetar é soltar o enlace. Se algo ficar segurando a conexão ou a thread
    // de áudio a ponto de impedir a próxima, é aqui que se vê.
    drop(primeiro);

    // Mesma chave e mesmo apelido: é a mesma pessoa voltando pela tela de
    // seleção, e não um segundo piloto. Se o Dogma tratasse a volta como
    // intrusa, o laço externo seria inviável — ADR 0017 prende o apelido à
    // identidade, e a identidade é a mesma.
    let segundo = conectar_e_falar(endereco, 46, "segunda volta").await?;
    assert!(
        segundo.sessao().pilot.0 > 0,
        "reconectar depois de ejetar falhou — o teardown não fechou"
    );
    assert_eq!(segundo.estado(), Link::Online);
    assert_ne!(
        segundo.sessao().id,
        primeira,
        "a segunda volta reaproveitou a sessão da primeira: isso não é reconectar"
    );

    servidor.shutdown();
    Ok(())
}

// Runtime de uma thread só, de propósito, e **não** `multi_thread` como os
// vizinhos: a espera do `encerrar` é o que este teste mede, e num runtime de
// várias threads a tarefa de aceitação se desfaz sozinha noutro núcleo enquanto
// o `Server::bind` seguinte gera certificado e abre banco. Medido: trocando o
// `encerrar` por um `drop`, este teste continua passando em `multi_thread` e
// reprova aqui com «Address already in use». Quem puser `flavor` de volta troca
// um teste que pega a falha por um que só a encontra às vezes.
#[tokio::test]
async fn hospedar_ejetar_e_hospedar_de_novo_libera_a_porta() -> Result<()> {
    // `Hospedagem::encerrar` espera a porta voltar. Sem essa espera, a segunda
    // volta do laço falharia com porta ocupada — e é esse o caminho que a tela
    // de seleção oferece quando diz «hospedar aqui».
    //
    // A porta sai da primeira hospedagem, que subiu com zero: pedir uma porta
    // ao sistema e reusá-la é a única forma de testar «a mesma porta» sem
    // escolher um número que possa ser de outra pessoa.
    let primeira = Hospedagem::iniciar(0, Location::Memory, "Casa").await?;
    let porta = primeira.endereco().port();
    assert_ne!(porta, 0, "o sistema não escolheu porta");
    primeira.encerrar().await;

    let segunda = Hospedagem::iniciar(porta, Location::Memory, "Casa").await;
    let segunda = match segunda {
        Ok(hospedagem) => hospedagem,
        Err(erro) => panic!("a porta não voltou depois de encerrar: {erro:?}"),
    };
    assert_eq!(
        segunda.endereco().port(),
        porta,
        "hospedou de novo, e noutra porta"
    );

    // Subir não é servir. Quem clicou em «hospedar aqui» pela segunda vez
    // espera gente entrando, então a prova é alguém entrando: um `bind` que
    // desse certo sobre um endpoint moribundo passaria na asserção de cima.
    let enlace = conectar_e_falar(
        SocketAddr::from(([127, 0, 0, 1], porta)),
        47,
        "hospedado de novo",
    )
    .await?;
    assert_eq!(enlace.estado(), Link::Online);

    drop(enlace);
    segunda.encerrar().await;
    Ok(())
}
