//! O convite conferido contra um Dogma de verdade.
//!
//! O ADR 0006 inventou o link para transformar o primeiro contato de cego em
//! verificado. A política que decide isso é uma tabela pura, testada em
//! `seele-core`; o que **nenhum** teste alcançava era a fiação: o `Destino`
//! carregando a impressão do convite até a conferência, a recusa derrubando a
//! conexão, e a recusa desfazendo o pin que o TLS já tinha escrito. Cada uma
//! dessas três pode sumir sem que a suíte de unidade note, porque nenhuma delas
//! é uma decisão sobre valores — são efeitos que só existem com um servidor do
//! outro lado.
//!
//! # As três coisas que este arquivo segura
//!
//! **A impressão chega.** Se a chamada passasse `None` no lugar de
//! `destino.impressao_esperada`, todo primeiro contato voltaria a ser cego e a
//! conferência inteira viraria enfeite. O primeiro teste conecta com a
//! impressão real do Dogma e exige `FirstContactVerified` — que é o único
//! veredito que não existe sem a fiação.
//!
//! **A recusa não deixa sessão de pé.** A recusa acontece depois do aperto de
//! mão, com uma sessão já servindo do lado do Dogma; deixá-la lá daria conexão
//! viva a quem acabou de ser rejeitado. O segundo teste espera o endpoint do
//! servidor ficar ocioso, com prazo. Ele afirma o resultado, não o mecanismo —
//! ver a nota no corpo do teste sobre o que essa asserção não distingue.
//!
//! **A recusa desfaz o pin.** É a metade que faltaria sem ninguém notar: o
//! verificador fixa dentro do retorno de chamada do TLS, bem antes de haver
//! veredito. Recusar sem desfixar deixaria a visita seguinte — sem link para
//! conferir — ver `Matches` e entrar no servidor recusado sem hesitar. O
//! segundo teste reconecta **sem** link e exige primeiro contato de novo.
//!
//! # O que este arquivo **não** afirma
//!
//! Não afirma que uma chave trocada seja recusada aqui: isso morre no TLS e
//! sobe como `ConnectError::PinChanged`, sem nunca virar veredito. E não afirma
//! nada sobre o desenho da tela — as cascas leem o veredito, e o que fazem com
//! ele é assunto delas.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{ConnectError, Link, MemoryPinStore, PinStore, Verdict};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{CageId, ClientMessageId, LineId};
use seele_server::casper::Location;
use seele_server::{DogmaConfig, Server};

const CAGE: u32 = 1;
const LINE: u32 = 1;

/// Uma impressão digital com a forma certa e dona nenhuma.
///
/// Sessenta e quatro dígitos hexadecimais, como manda
/// `seele_proto::transport::certificate_fingerprint`, para que o que este
/// arquivo mede seja a conferência e não um comprimento errado. Nenhum
/// certificado tem SHA-256 zerado.
const NAO_E_DE_NINGUEM: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Sobe um Dogma numa porta que o sistema escolhe.
///
/// Mesma forma do `ejetar.rs`, e pelo mesmo motivo: porta zero, nunca um número
/// escrito à mão, que colidiria com o Dogma que a pessoa deixou rodando na
/// própria máquina. Os três testes deste arquivo sobem cada um o seu, então
/// nenhum vê o pin nem a lotação do outro.
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

/// O destino, com o que o convite prometeu.
///
/// O apelido entra por fora porque o ADR 0017 o prende à identidade: uma chave
/// nova com o apelido de outra pessoa é recusada com `CredentialRejected`. Aqui
/// a identidade nunca muda dentro de um teste, justamente para que a única
/// coisa que varia seja a impressão esperada.
fn destino(endereco: SocketAddr, apelido: &str, impressao_esperada: Option<&str>) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        impressao_esperada: impressao_esperada.map(str::to_owned),
    }
}

/// Conecta, e devolve o erro em vez de estourar.
///
/// Separado do `falar` abaixo, e não junto como no `ejetar.rs`: lá conectar que
/// falha é sempre defeito, aqui é metade do que se mede. A loja de pins vem de
/// fora porque em dois destes testes o que interessa é o que **sobrou** nela
/// depois da primeira tentativa.
async fn conectar(
    endereco: SocketAddr,
    semente: u8,
    apelido: &str,
    impressao_esperada: Option<&str>,
    pins: &Arc<MemoryPinStore>,
) -> Result<Enlace, ConnectError> {
    Enlace::conectar(
        destino(endereco, apelido, impressao_esperada),
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::clone(pins) as Arc<dyn PinStore>,
    )
    .await
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

/// Prova que a sessão **serve**, e não só que o construtor devolveu `Ok`.
///
/// Entrar no Cage, abrir a Linha, dizer algo e ouvir de volta é o menor caminho
/// que passa pelo Dogma inteiro — copiado do `conectar_e_falar` do `ejetar.rs`,
/// que é onde esta forma nasceu. É o que distingue "a conferência avisou e
/// seguiu" de "a conferência avisou e derrubou": um enlace derrubado devolve
/// `Ok` do mesmo jeito, e só cala quando alguém fala com ele.
async fn falar_e_ouvir(enlace: &mut Enlace, o_que: &str) -> Result<()> {
    enlace.inserir_plug(CageId(CAGE)).await?;
    enlace.abrir_linha(LineId(LINE)).await?;
    enlace
        .dizer(LineId(LINE), o_que.to_owned(), ClientMessageId(1))
        .await?;

    let ouviu = esperar(enlace, Duration::from_secs(15), |aviso| {
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
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_impressao_que_o_convite_promete_verifica_o_primeiro_contato() -> Result<()> {
    let (endereco, servidor) = dogma().await?;
    // A impressão de verdade, lida do Dogma que está de pé: é isto que o
    // `seeled convite` põe no link, e o que um link honesto carrega.
    let impressao = servidor.fingerprint().to_owned();

    let loja = Arc::new(MemoryPinStore::new());
    let mut enlace = conectar(endereco, 46, "ayanami", Some(&impressao), &loja)
        .await
        .expect("o convite promete a impressão deste Dogma; não havia o que recusar");

    // `FirstContactVerified` é o veredito que **só** existe com a impressão do
    // convite chegando até a conferência. Passar `None` no lugar dela daria
    // `FirstContact`, a conexão seguiria igual, e o ADR 0006 estaria desligado
    // sem nenhum teste ficar vermelho. É esta asserção, e nenhuma outra, que
    // segura aquela linha de fiação.
    assert_eq!(
        enlace.veredito(),
        &Verdict::FirstContactVerified {
            fingerprint: impressao.clone()
        },
        "o convite conferia e o enlace não disse que conferiu"
    );
    assert_eq!(enlace.estado(), Link::Online);

    // Conferir não substitui fixar: o TOFU do ADR 0003 continua valendo, e a
    // visita seguinte tem que reconhecer este servidor.
    assert_eq!(
        loja.pinned(&endereco.to_string()),
        Some(impressao),
        "o convite conferiu e a chave não ficou fixada"
    );

    falar_e_ouvir(&mut enlace, "primeiro contato verificado").await?;

    drop(enlace);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_impressao_que_nao_confere_derruba_a_conexao_e_desfaz_o_pin() -> Result<()> {
    let (endereco, servidor) = dogma().await?;
    let real = servidor.fingerprint().to_owned();
    let chave_do_pin = endereco.to_string();
    let loja = Arc::new(MemoryPinStore::new());

    let erro = conectar(endereco, 46, "ayanami", Some(NAO_E_DE_NINGUEM), &loja)
        .await
        .expect_err("um convite que não confere tinha que derrubar a conexão");

    // Quem prometeu é o link, quem ofereceu é o servidor. Trocar os dois faria
    // a casca acusar o lado errado.
    assert_eq!(
        erro,
        ConnectError::InviteMismatch {
            expected: NAO_E_DE_NINGUEM.to_owned(),
            offered: real.clone(),
        }
    );

    // Metade um: não sobrou pin. Sem esta asserção o teste passaria com a
    // recusa decorativa — erro devolvido, chave do impostor fixada — e a visita
    // de baixo entraria como se conhecesse o servidor de sempre.
    assert_eq!(
        loja.pinned(&chave_do_pin),
        None,
        "a recusa deixou fixada a chave do servidor que ela acabou de recusar"
    );

    // Metade dois: a conexão caiu **do lado do Dogma** também. A recusa vem
    // depois do aperto de mão, com a sessão já de pé lá, e o cliente mantém
    // keepalive a cada 5 s — uma conexão esquecida aqui não expira sozinha, o
    // ADR 0003 não a protege mais, e o Dogma serviria uma sessão a quem foi
    // recusado. `wait_idle` só volta quando o endpoint não tem mais conexão
    // nenhuma, e o prazo é folga de loopback: na prática ela some em dezenas de
    // milissegundos.
    //
    // O que esta asserção **não** distingue, e vale dito para ninguém confiar
    // demais nela: apagar o `cliente.disconnect()` de `Enlace::conectar` não a
    // deixa vermelha. Medido — 84 ms com a queda explícita, 87 ms sem ela.
    // Soltar o `Client` acaba fechando a conexão pelo caminho longo (a tarefa
    // leitora só se descobre sozinha quando o Dogma fala de novo), então o que
    // se guarda aqui é o **resultado** — não sobra sessão —, e não qual das
    // duas coisas o produziu. A queda explícita continua sendo a certa: ela não
    // depende de o servidor dizer alguma coisa.
    tokio::time::timeout(Duration::from_secs(10), servidor.wait_idle())
        .await
        .expect("a conexão recusada continuou de pé no Dogma");

    // Metade três, que é a que dá sentido às outras: a visita seguinte, sem
    // link nenhum para conferir, tem que ser primeiro contato de novo. Se o pin
    // tivesse sobrado, este veredito seria `Known` — a pessoa entraria calada
    // no servidor recusado, e a recusa teria sido um susto sem consequência.
    let mut de_novo = conectar(endereco, 46, "ayanami", None, &loja)
        .await
        .expect("sem link não há o que conferir; a conexão tinha que subir");
    assert_eq!(
        de_novo.veredito(),
        &Verdict::FirstContact {
            fingerprint: real.clone()
        },
        "depois da recusa o Dogma foi tratado como já conhecido"
    );
    assert_eq!(loja.pinned(&chave_do_pin), Some(real));

    falar_e_ouvir(&mut de_novo, "de volta, e cega").await?;

    drop(de_novo);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_link_velho_contra_um_dogma_ja_conhecido_avisa_e_nao_derruba() -> Result<()> {
    // A metade oposta da recusa, e a que some sem ninguém notar. Com pin
    // estabelecido, o TOFU já provou que este é o servidor de ontem: quem está
    // errado é o link. Derrubar aqui trancaria a pessoa para fora de um Dogma
    // que ela usa porque um amigo mandou um convite velho.
    let (endereco, servidor) = dogma().await?;
    let real = servidor.fingerprint().to_owned();
    let chave_do_pin = endereco.to_string();
    let loja = Arc::new(MemoryPinStore::new());

    let primeiro = conectar(endereco, 46, "ayanami", None, &loja)
        .await
        .expect("quem digitou o endereço à mão não tem o que conferir");
    assert_eq!(
        primeiro.veredito(),
        &Verdict::FirstContact {
            fingerprint: real.clone()
        }
    );
    // A mesma chave e o mesmo apelido voltando, como no `ejetar.rs`: o ADR 0017
    // prende o apelido à identidade, e trocar a semente aqui trocaria o assunto
    // do teste de "link velho" para "outra pessoa".
    drop(primeiro);

    let mut segundo = conectar(endereco, 46, "ayanami", Some(NAO_E_DE_NINGUEM), &loja)
        .await
        .expect("um link velho não pode trancar ninguém para fora de um Dogma conhecido");
    assert_eq!(
        segundo.veredito(),
        &Verdict::InviteDisagrees {
            expected: NAO_E_DE_NINGUEM.to_owned(),
            offered: real.clone(),
        },
        "o link discordava do pin e o enlace não avisou"
    );

    // Que o construtor devolva `Ok` não prova que a sessão está viva: a queda é
    // um `close` na conexão, e um enlace derrubado só se denuncia quando alguém
    // fala com ele. Sem isto, trocar o aviso por uma recusa não faria este
    // teste ficar vermelho da forma que importa.
    assert_eq!(segundo.estado(), Link::Online);
    falar_e_ouvir(&mut segundo, "o link estava velho, o Dogma é o mesmo").await?;

    // E o aviso não desfaz nada. Desfixar aqui é o erro simétrico: a visita
    // seguinte entraria cega num servidor que já era conhecido.
    assert_eq!(
        loja.pinned(&chave_do_pin),
        Some(real),
        "o aviso desfez o pin, e a próxima visita entraria cega"
    );

    drop(segundo);
    servidor.shutdown();
    Ok(())
}
