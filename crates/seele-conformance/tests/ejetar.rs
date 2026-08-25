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
//! funciona**, **a sala do Dogma esvazia**, e **encerrar a hospedagem devolve a
//! porta**. Um teardown que deixasse algo preso reprovaria numa dessas três.
//!
//! A do meio é a que enxerga vazamento. Sondar a lotação do Cage com prazo não
//! é a mesma coisa que afirmar fechamento instantâneo: não diz *quando* a
//! cadeira é liberada, só que ela é — a mesma forma do `esperar` logo abaixo,
//! e nada intermitente.
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
use seele_server::persistence::Location;
use seele_server::dogma::Occupant;
use seele_server::hospedagem::Hospedagem;
use seele_server::{DogmaConfig, Server};

const CAGE: u32 = 1;
const LINE: u32 = 1;

/// Sobe um Dogma numa porta que o sistema escolhe.
///
/// Porta zero, e nunca um número escrito à mão: um número fixo colidiria com o
/// Dogma que a pessoa deixou rodando na própria máquina.
///
/// Não protege de tudo. Os dois testes deste arquivo correm ao mesmo tempo, e o
/// `hospedar_...` devolve a porta efêmera dele e volta a ligar em `0.0.0.0`
/// enquanto este aqui liga em `127.0.0.1:0`; se o sistema devolver exatamente
/// aquele número no intervalo, colidem, porque ninguém liga `SO_REUSEADDR`. A
/// chance é remota e o padrão é o da casa — fica dito para não parecer garantia.
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

/// O apelido entra por fora porque o ADR 0017 o prende à identidade: uma chave
/// nova com o apelido de outra pessoa é recusada com `CredentialRejected`, e é
/// assim que tem que ser. Quem troca a semente troca o apelido junto.
fn destino(endereco: SocketAddr, apelido: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        impressao_esperada: None,
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
            Ok(_) => {}
            Err(_) => {}
        }
    }
    None
}

/// Uma chave de idempotência que nunca se repete nesta suíte.
///
/// Fixar `ClientMessageId(1)` aqui era o defeito, e ele escondia outro. As duas
/// conexões deste arquivo são a **mesma identidade** — mesma semente, logo mesmo
/// `author_id` —, e `Messages::append_batch` deduplica por `(author_id,
/// client_message_id)`. Então a segunda mensagem era lida como reenvio da
/// primeira e nunca era gravada.
///
/// O teste passava assim mesmo porque o servidor devolvia o **corpo que
/// chegou** vestindo o id da linha antiga: o eco batia, e ninguém via que nada
/// tinha sido escrito. Consertado o eco, este teste caiu — e o que ele estava
/// exercitando era o defeito (pendência 19), não a volta pela tela de seleção.
///
/// Um cliente correto não reusa a chave entre sessões, e desde a pendência 19 os
/// dois deste repositório sorteiam a metade alta no arranque. Esta função é o
/// mesmo contrato, do jeito que um teste pode tê-lo: determinística e sem
/// repetição.
fn proxima_chave() -> ClientMessageId {
    static PROXIMA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    ClientMessageId(PROXIMA.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// Conecta e prova que a sessão **serve**, e não só que o construtor devolveu
/// `Ok`.
///
/// Entrar no Cage, abrir a Linha, dizer algo e ouvir de volta é o menor
/// caminho que passa pelo Dogma inteiro. Uma conexão que só existisse no papel
/// falharia aqui, e é justamente essa a diferença que este arquivo precisa
/// enxergar na segunda volta.
async fn conectar_e_falar(
    endereco: SocketAddr,
    semente: u8,
    apelido: &str,
    o_que: &str,
) -> Result<Enlace> {
    let mut enlace = Enlace::conectar(
        destino(endereco, apelido),
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.inserir_plug(CageId(CAGE)).await?;
    enlace.abrir_linha(LineId(LINE)).await?;
    enlace
        .dizer(LineId(LINE), o_que.to_owned(), proxima_chave())
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

/// Quem o Dogma acha que está no Cage, esperando até a conta bater.
///
/// Sondar com prazo, e não olhar uma vez. O briefing proíbe afirmar que algo
/// fechou **no instante** em que a alça foi solta — e com razão, porque o
/// desmonte do lado do Dogma acontece quando a conexão morre, o que é
/// assíncrono e não dá para esperar de fora. Mas «olhar até bater, com prazo» é
/// outra forma: não afirma *quando* a cadeira é liberada, só que ela **é**. Não
/// é intermitente, e é a mesma forma que o `esperar` daqui de cima já usa.
///
/// Desiste devolvendo o que viu por último, para a asserção de quem chamou
/// poder dizer o número errado em vez de um tempo esgotado sem número.
async fn ocupantes(servidor: &Server, esperados: usize, prazo: Duration) -> Vec<Occupant> {
    let fim = tokio::time::Instant::now() + prazo;
    loop {
        let agora = servidor
            .dogma()
            .occupancy
            .lock()
            .await
            .in_cage(CageId(CAGE));
        if agora.len() == esperados || tokio::time::Instant::now() >= fim {
            return agora;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn conectar_ejetar_e_conectar_de_novo_no_mesmo_processo() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    let primeiro = conectar_e_falar(endereco, 46, "ayanami", "primeira volta").await?;
    assert!(primeiro.sessao().person.0 > 0, "a primeira sessão não subiu");
    assert_eq!(primeiro.estado(), Link::Online);
    let primeira = primeiro.sessao().id;

    // Ejetar é soltar o enlace. Se algo ficar segurando a conexão ou a thread
    // de áudio a ponto de impedir a próxima, é aqui que se vê.
    drop(primeiro);

    // Semente diferente, e de propósito: aqui o que se mede é o **rastro** que a
    // sessão ejetada deixa no Dogma, e com o mesmo pessoa não haveria rastro
    // para medir. `Occupancy::seat` começa por `vacate_everywhere(person)`
    // (`dogma.rs:171-174`), então o mesmo pessoa nunca aparece duas vezes na
    // lista, por construção — a asserção lá embaixo passaria mesmo com a
    // primeira sessão inteira pendurada. Com dois pessoas, uma sessão que não
    // se desfaz fica visível como uma cadeira ocupada a mais.
    //
    // A volta da *mesma* pessoa, que é o caso real do `:ejetar`, está no teste
    // seguinte, que não olha a lotação — ver a pendência 11 para o motivo.
    let segundo = conectar_e_falar(endereco, 48, "shikinami", "segunda volta").await?;
    assert!(
        segundo.sessao().person.0 > 0,
        "reconectar depois de ejetar falhou — o teardown não fechou"
    );
    assert_eq!(segundo.estado(), Link::Online);
    assert_ne!(
        segundo.sessao().id,
        primeira,
        "a segunda volta reaproveitou a sessão da primeira: isso não é reconectar"
    );

    // O que faltava: até aqui o teste via a sessão nova subir e não via a velha
    // sumir. Uma sessão vazada por volta do laço não impediria a próxima de
    // conectar — QUIC aceita quantas quiser —, e apareceria para as pessoas
    // como um roster com fantasmas, que é o defeito que este laço mais arrisca.
    let dentro = ocupantes(&servidor, 1, Duration::from_secs(10)).await;
    assert_eq!(
        dentro.len(),
        1,
        "o Cage ficou com {} ocupantes depois de um ejetar: a sessão soltar não a tirou da sala — {dentro:?}",
        dentro.len()
    );
    assert_eq!(
        dentro.first().map(|ocupante| ocupante.person),
        Some(segundo.sessao().person),
        "quem sobrou na sala não é quem está conectado"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mesma_pessoa_volta_pela_tela_de_selecao() -> Result<()> {
    // A mesma chave e o mesmo apelido: é isto que o `:ejetar` faz de verdade —
    // a pessoa cai na tela de seleção e entra de novo, sendo ela mesma. Se o
    // Dogma tratasse a volta como intrusa, o laço externo seria inviável, e o
    // ADR 0017 prende o apelido à identidade, que aqui não muda.
    let (endereco, servidor) = dogma().await?;

    let primeiro = conectar_e_falar(endereco, 46, "ayanami", "primeira volta").await?;
    let primeira = primeiro.sessao().id;
    let pessoa = primeiro.sessao().person;
    drop(primeiro);

    let segundo = conectar_e_falar(endereco, 46, "ayanami", "de novo eu").await?;
    assert_eq!(segundo.estado(), Link::Online);
    assert_eq!(
        segundo.sessao().person,
        pessoa,
        "a mesma identidade voltou como outro pessoa"
    );
    assert_ne!(
        segundo.sessao().id,
        primeira,
        "a volta reaproveitou a sessão anterior: isso não é reconectar"
    );

    // Este teste **não** olha a lotação do Cage, e a omissão é deliberada: com o
    // mesmo pessoa nos dois lados, o `vacate` da sessão que morre e o `seat` da
    // que nasce disputam a mesma chave, e a lista pode acabar vazia. É a
    // pendência 11, encontrada lendo este teste — o defeito é do Dogma, não
    // daqui, e afirmar lotação neste ponto seria trocar um teste que pega falha
    // por um que reprova sozinho de vez em quando.
    servidor.shutdown();
    Ok(())
}

// Runtime de uma thread só, de propósito, e **não** `multi_thread` como os
// vizinhos: a espera do `encerrar` é o que este teste mede, e o `flavor` é o que
// dá dentes à medição.
//
// O motivo é estrutural, e não estatístico. `Server::bind`
// (`seele-server/src/lib.rs:126-150`) não tem **nenhum** ponto de espera entre
// a entrada e o `quinn::Endpoint::server`: `Persistence::open`, `seed`,
// `Identity::load_or_create` e `tls::server_config` são todos síncronos. Numa
// thread só, a tarefa de aceitação portanto **não tem como ser escalonada**
// entre soltar a hospedagem e religar a porta — se o `encerrar` não a esperou,
// o `bind` acha o socket ocupado, sempre. Em `multi_thread` ela se desfaz noutro
// núcleo enquanto isso corre, e a falha some. Quem puser `flavor` de volta troca
// um teste que pega a falha por um que não pega.
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
        "ayanami",
        "hospedado de novo",
    )
    .await?;
    assert_eq!(enlace.estado(), Link::Online);

    drop(enlace);
    segunda.encerrar().await;
    Ok(())
}
