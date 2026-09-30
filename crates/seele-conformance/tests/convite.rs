//! O convite conferido contra um servidor de verdade.
//!
//! O ADR 0006 inventou o link para transformar o primeiro contato de cego em
//! verificado. A política que decide isso é uma tabela pura, testada em
//! `seele-core`; o que **nenhum** teste alcançava era a fiação: o `Destino`
//! carregando a impressão do convite até o verificador TLS, a recusa
//! acontecendo antes de o `Hello` sair, e a recusa sem deixar pin nem sessão
//! para trás. Cada uma dessas pode sumir sem que a suíte de unidade note,
//! porque nenhuma delas é uma decisão sobre valores — são efeitos que só
//! existem com um servidor do outro lado.
//!
//! # O que este arquivo segura
//!
//! **A impressão chega.** Se a chamada passasse `None` no lugar de
//! `destino.impressao_esperada`, todo primeiro contato voltaria a ser cego e a
//! conferência inteira viraria enfeite. O primeiro teste conecta com a
//! impressão real do servidor e exige `FirstContactVerified` — que é o único
//! veredito que não existe sem a fiação.
//!
//! **A recusa acontece dentro do TLS, antes do `Hello`.** Até a 0.15.0 ela
//! vinha depois do aperto de mão inteiro. Àquela altura o `Hello` já tinha
//! levado o convite, a chave e o apelido, a portaria já tinha anotado a batida,
//! e o convite de uso único já tinha sido gasto por quem atendeu no lugar do
//! servidor. É o S2b da análise de 22/09. Os dois últimos testes observam **o
//! servidor**: o convite ainda entra, e a portaria não tem pedido. Eles olham
//! para lá porque o erro que o cliente devolve é `InviteMismatch` antes e
//! depois do conserto.
//!
//! **A recusa não deixa sessão de pé, nem pin.** O verificador não fixa uma
//! chave que a impressão esperada recusa. A conferência de `Enlace::conectar`
//! continua como segunda linha e desfaz o pin se algum dia a primeira falhar.
//! O segundo teste reconecta **sem** link e exige primeiro contato de novo.
//!
//! A mesma conferência na volta da bateria interna, quando o pin sumiu entre a
//! queda e a volta, é guardada em `bateria_interna.rs`.
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
use seele_core::{ConnectError, MemoryPinStore, PinStore, Verdict};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_server::persistence::{Location, Persistence};
use seele_server::{admissao, portaria, Daemon, ServerConfig};

mod vaga;

const VOICE_ROOM: u32 = 1;
const LINE: u32 = 1;

/// Uma impressão digital com a forma certa e dona nenhuma.
///
/// Sessenta e quatro dígitos hexadecimais, como manda
/// `seele_proto::transport::certificate_fingerprint`, para que o que este
/// arquivo mede seja a conferência e não um comprimento errado. Nenhum
/// certificado tem SHA-256 zerado.
const NAO_E_DE_NINGUEM: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Sobe um servidor numa porta que o sistema escolhe.
///
/// Mesma forma do `ejetar.rs`, e pelo mesmo motivo: porta zero, nunca um número
/// escrito à mão, que colidiria com o servidor que a pessoa deixou rodando na
/// própria máquina. Os três testes deste arquivo sobem cada um o seu, então
/// nenhum vê o pin nem a lotação do outro.
async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
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
        e_o_alvo: true,
        aceito: None,
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
/// Entrar na sala de voz, abrir a Linha, dizer algo e ouvir de volta é o menor caminho
/// que passa pelo servidor inteiro — copiado do `conectar_e_falar` do `ejetar.rs`,
/// que é onde esta forma nasceu. É o que distingue "a conferência avisou e
/// seguiu" de "a conferência avisou e derrubou": um enlace derrubado devolve
/// `Ok` do mesmo jeito, e só cala quando alguém fala com ele.
async fn falar_e_ouvir(enlace: &mut Enlace, o_que: &str) -> Result<()> {
    enlace
        .entrar_na_voice_room(VoiceRoomId(VOICE_ROOM), None)
        .await?;
    enlace.abrir_linha(ChannelId(LINE)).await?;
    enlace
        .dizer(ChannelId(LINE), o_que.to_owned(), ClientMessageId(1))
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
        "a sessão conectou e não fala: «{o_que}» não voltou do servidor"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_impressao_que_o_convite_promete_verifica_o_primeiro_contato() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = server().await?;
    // A impressão de verdade, lida do servidor que está de pé: é isto que o
    // `seeled convite` põe no link, e o que um link honesto carrega.
    let impressao = servidor.fingerprint().to_owned();

    let loja = Arc::new(MemoryPinStore::new());
    let mut enlace = conectar(endereco, 46, "marcela", Some(&impressao), &loja)
        .await
        .expect("o convite promete a impressão deste servidor; não havia o que recusar");

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
async fn o_convite_que_nao_confere_nao_deixa_conexao_nem_pin() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = server().await?;
    let real = servidor.fingerprint().to_owned();
    let chave_do_pin = endereco.to_string();
    let loja = Arc::new(MemoryPinStore::new());

    let erro = conectar(endereco, 46, "marcela", Some(NAO_E_DE_NINGUEM), &loja)
        .await
        .expect_err("um convite que não confere tinha que ser recusado");

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

    // Metade dois: não sobrou conexão do lado do servidor. Desde que a
    // impressão é conferida dentro do TLS, o aperto de mão falha antes do
    // `Hello` e não chega a haver sessão lá. A queda explícita que este trecho
    // guardava, o `cliente.close(INVITE_REFUSED)` de `Enlace::conectar`, ficou
    // como segunda linha e já não roda para esta recusa: quem a conferência de
    // depois do aperto de mão derrubaria é um servidor que o verificador deixou
    // passar, e o verificador não deixa. O que se guarda aqui é o **resultado**,
    // qualquer que seja o caminho: `wait_idle` só volta quando o endpoint não
    // tem conexão nenhuma, e o prazo é folga de loopback.
    //
    // O que esta asserção não distingue, e vale dito: ela passava igual quando
    // a recusa vinha depois do aperto de mão, com ou sem a queda explícita.
    // Isso foi medido na época: 84 ms com ela e 87 sem. Quem distingue «a
    // recusa foi antes do `Hello`» são os dois últimos testes deste arquivo, que
    // olham o convite e a portaria do lado do servidor.
    tokio::time::timeout(Duration::from_secs(10), servidor.wait_idle())
        .await
        .expect("a conexão recusada continuou de pé no servidor");

    // Metade três, que é a que dá sentido às outras: a visita seguinte, sem
    // link nenhum para conferir, tem que ser primeiro contato de novo. Se o pin
    // tivesse sobrado, este veredito seria `Known` — a pessoa entraria calada
    // no servidor recusado, e a recusa teria sido um susto sem consequência.
    let mut de_novo = conectar(endereco, 46, "marcela", None, &loja)
        .await
        .expect("sem link não há o que conferir; a conexão tinha que subir");
    assert_eq!(
        de_novo.veredito(),
        &Verdict::FirstContact {
            fingerprint: real.clone()
        },
        "depois da recusa o servidor foi tratado como já conhecido"
    );
    assert_eq!(loja.pinned(&chave_do_pin), Some(real));

    falar_e_ouvir(&mut de_novo, "de volta, e cega").await?;

    drop(de_novo);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_link_velho_contra_um_server_ja_conhecido_avisa_e_nao_derruba() -> Result<()> {
    let _vaga = vaga::minha();
    // A metade oposta da recusa, e a que some sem ninguém notar. Com pin
    // estabelecido, o TOFU já provou que este é o servidor de ontem: quem está
    // errado é o link. Derrubar aqui trancaria a pessoa para fora de um servidor
    // que ela usa porque um amigo mandou um convite velho.
    let (endereco, servidor) = server().await?;
    let real = servidor.fingerprint().to_owned();
    let chave_do_pin = endereco.to_string();
    let loja = Arc::new(MemoryPinStore::new());

    let primeiro = conectar(endereco, 46, "marcela", None, &loja)
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

    let mut segundo = conectar(endereco, 46, "marcela", Some(NAO_E_DE_NINGUEM), &loja)
        .await
        .expect("um link velho não pode trancar ninguém para fora de um servidor conhecido");
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
    // fala com ele. Falar é a asserção; sem ela, trocar o aviso por uma recusa
    // não faria este teste ficar vermelho da forma que importa.
    //
    // `enlace.estado()` **não** serve para isso, e a tentação é grande porque a
    // linha existe no `ejetar.rs`: `Enlace::conectar` põe `Link::Online` sem
    // condição no caminho de `Ok`, e o campo só muda dentro de `proximo()`.
    // Antes de alguém drenar avisos, a asserção não tem como reprovar — e este
    // é o arquivo que existe para provar uma coisa, não para parecer que prova.
    falar_e_ouvir(&mut segundo, "o link estava velho, o servidor é o mesmo").await?;

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

/// Sobe um servidor com banco em arquivo.
///
/// Os três testes de cima usam banco em memória porque só olham o cliente. Os
/// dois de baixo olham **o servidor** — o convite que ele guarda, a fila da
/// portaria —, e para isso o teste precisa abrir o mesmo banco por fora, como
/// `acceptance_seguranca.rs` faz.
async fn server_com_banco(banco: &std::path::Path) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco.to_path_buf()),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

/// Conecta levando o convite de uso único do ADR 0021.
///
/// O `destino` de cima deixa o segredo em `None`, porque os três primeiros
/// testes medem a conferência num servidor aberto. Aqui o que se mede é o que
/// acontece com o segredo.
async fn conectar_com_convite(
    endereco: SocketAddr,
    semente: u8,
    apelido: &str,
    convite: &str,
    impressao_esperada: &str,
    pins: &Arc<MemoryPinStore>,
) -> Result<Enlace, ConnectError> {
    let mut alvo = destino(endereco, apelido, Some(impressao_esperada));
    alvo.segredo = Some(convite.to_owned());
    Enlace::conectar(
        alvo,
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::clone(pins) as Arc<dyn PinStore>,
    )
    .await
}

/// **O convite não sai quando a impressão não confere.**
///
/// O S2b da análise de 22/09. A conferência acontecia depois do aperto de mão
/// inteiro, e o `Hello` leva o convite: quem atendesse no lugar do servidor
/// recebia o token, a chave e o apelido, e o servidor **gastava** o convite de
/// uso único antes de o cliente recusar. A pessoa ficava com um link morto,
/// num servidor em que nunca entrou.
///
/// Quem se observa aqui é o servidor. O erro que o cliente devolve é
/// `InviteMismatch` antes e depois do conserto, porque a conferência depois do
/// aperto de mão continua como segunda linha. Só o que sobrou do lado de lá
/// distingue os dois casos: o mesmo convite, levado por **outra** chave, ainda
/// tem de entrar. Antes do conserto ele volta `CredentialRejected`, porque o
/// convite foi gasto pelo `Hello` que não devia ter saído.
///
/// **A segunda conexão usa outro apelido, e é de propósito.** O ADR 0017
/// prende o apelido à chave que o usou primeiro, e o `Hello` de antes do
/// conserto prendia `marcela` à chave da primeira tentativa. Com o mesmo
/// apelido, o vermelho teria duas causas possíveis (o convite gasto ou o
/// apelido preso), e o teste não diria qual. Com outro, só o convite explica a
/// recusa.
#[tokio::test(flavor = "multi_thread")]
async fn a_impressao_que_nao_confere_nao_gasta_o_convite() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let convite = {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        admissao::criar_convite(&mut persistence, "para a marcela")?
    };
    let (endereco, servidor) = server_com_banco(&banco).await?;
    let real = servidor.fingerprint().to_owned();
    let loja = Arc::new(MemoryPinStore::new());

    let erro = conectar_com_convite(endereco, 46, "marcela", &convite, NAO_E_DE_NINGUEM, &loja)
        .await
        .expect_err("a impressão não confere; não havia o que aceitar");
    assert_eq!(
        erro,
        ConnectError::InviteMismatch {
            expected: NAO_E_DE_NINGUEM.to_owned(),
            offered: real.clone(),
        },
        "a recusa pela impressão chegou com outro nome, e a casca acusaria a \
         rede ou a versão em vez do link"
    );
    assert_eq!(
        loja.pinned(&endereco.to_string()),
        None,
        "a chave recusada ficou fixada"
    );

    // Nenhuma conexão de pé lá. Sozinha, esta linha não distingue antes de
    // depois (ver a nota do teste da recusa, acima), e é por isso que o que
    // vem a seguir existe.
    tokio::time::timeout(Duration::from_secs(10), servidor.wait_idle())
        .await
        .expect("a conexão recusada continuou de pé no servidor");

    // Outra chave e **outro apelido**: a única coisa em comum com a tentativa
    // de cima é o convite, e é só ele que este teste mede.
    let entrou = conectar_com_convite(endereco, 47, "joana", &convite, &real, &loja)
        .await
        .expect(
            "o convite foi gasto por um servidor que o cliente recusou: o `Hello` \
             saiu antes da conferência",
        );
    assert_eq!(
        entrou.veredito(),
        &Verdict::FirstContactVerified { fingerprint: real },
        "o convite entrou sem a impressão conferida"
    );

    drop(entrou);
    servidor.shutdown();
    Ok(())
}

/// **E nem bate à porta.**
///
/// Com a portaria do ADR 0030 ligada, um `Hello` com convite válido vira um
/// pedido na fila de quem hospeda, com o apelido e a observação do convite. A
/// pessoa que hospeda passaria a ver, e poderia aprovar, uma batida que o
/// próprio cliente recusou.
#[tokio::test(flavor = "multi_thread")]
async fn a_impressao_que_nao_confere_nao_deixa_pedido_na_portaria() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let convite = {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        portaria::ligar(&mut persistence, true)?;
        admissao::criar_convite(&mut persistence, "para a marcela")?
    };
    let (endereco, servidor) = server_com_banco(&banco).await?;
    let real = servidor.fingerprint().to_owned();
    let loja = Arc::new(MemoryPinStore::new());

    let erro = conectar_com_convite(endereco, 46, "marcela", &convite, NAO_E_DE_NINGUEM, &loja)
        .await
        .expect_err("a impressão não confere; não havia o que aceitar");
    // Antes do conserto isto era `Refused { AdmissionPending }`: o servidor
    // respondia ao `Hello` antes de a conferência rodar, e a recusa pela
    // impressão nunca chegava a acontecer.
    assert_eq!(
        erro,
        ConnectError::InviteMismatch {
            expected: NAO_E_DE_NINGUEM.to_owned(),
            offered: real,
        },
        "o servidor respondeu ao `Hello` de quem o cliente devia ter recusado"
    );

    let persistence = Persistence::open(&Location::File(banco.clone()))?;
    let fila = portaria::pedidos(&persistence)?;
    assert!(
        fila.is_empty(),
        "a portaria recebeu uma batida de quem o cliente recusou: {fila:?}"
    );

    servidor.shutdown();
    Ok(())
}
