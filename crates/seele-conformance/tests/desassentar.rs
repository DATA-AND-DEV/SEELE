//! A sessão velha não apaga a nova.
//!
//! O defeito, relatado de uma sessão real: «como anfitrião, eu não vejo um amigo
//! que se vê dentro da sala, e não ouço nem vejo esse amigo».
//!
//! # Por que não é uma corrida rara
//!
//! A aritmética está toda em constantes lidas. O cliente manda um `Ping` a cada
//! [`seele_proto::transport::KEEPALIVE`] e entra na bateria interna depois de três
//! perdidos — perto de 15 s. O servidor só desiste da conexão muda em
//! [`seele_proto::transport::IDLE_TIMEOUT`], 20 s. Numa queda **silenciosa** — o
//! Wi‑Fi oscilando, o notebook dormindo, o NAT trocando a porta, o
//! `CONNECTION_CLOSE` perdido, que é um pacote só e não é retransmitido — a
//! sessão nova sobe antes de a velha morrer. **É a ordem esperada**, com cerca de
//! cinco segundos de folga, e não um acidente de escalonamento.
//!
//! E o desmonte da sessão velha era chaveado por pessoa: ele tirava a pessoa da
//! tarefa da sala, encerrava a tela dela, desocupava o assento e a tirava dos
//! presentes — sem nunca perguntar se aquela pessoa ainda era dele. Quem morria
//! levava quem tinha acabado de chegar.
//!
//! # Por que isto só se vê contra um servidor de verdade
//!
//! As duas metades estão certas sozinhas. Nenhum teste de unidade tem duas
//! sessões da mesma pessoa vivas ao mesmo tempo, porque é o transporte que as
//! produz: uma conexão que emudece sem se despedir e outra que sobe antes de o
//! tempo ocioso da primeira estourar. Daí este arquivo morar aqui, e daí ele
//! custar o tempo ocioso inteiro para rodar — a espera **é** a medida.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{Client, MemoryPinStore, PinStore, Room};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{PersonId, Ssrc, VoiceRoomId};
use seele_proto::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

/// Quanto se espera além do tempo ocioso antes de conferir o estrago.
///
/// O desmonte da sessão velha acontece **depois** de o servidor desistir dela, e
/// é ele que este teste mede. Margem folgada porque a asserção positiva não deve
/// depender de quanto a máquina demorou.
const MARGEM: Duration = Duration::from_secs(8);

/// Quanto cada passo que fala com a rede tem para terminar.
///
/// Vinte segundos é folga larga para um aperto de mão contra `127.0.0.1` — o
/// mesmo prazo que a subida da conexão emudecível já usava. O número não é o
/// ponto: o ponto é **existir**. Sem prazo próprio, numa máquina saturada este
/// teste não reprova, ele pendura; e um teste pendurado não diz nada a quem
/// espera por ele, que foi a aresta de diagnóstico apontada na revisão.
const PRAZO_DE_REDE: Duration = Duration::from_secs(20);

/// Corre um passo de rede com prazo, e transforma o estouro em reprovação falante.
///
/// O pânico diz **qual** passo não voltou, porque «o teste travou» e «o aperto de
/// mão do anfitrião não voltou em 20 s» custam coisas muito diferentes a quem
/// investiga.
async fn com_prazo<F, T, E>(o_que: &str, passo: F) -> Result<T>
where
    F: Future<Output = std::result::Result<T, E>>,
    E: Into<anyhow::Error>,
{
    match tokio::time::timeout(PRAZO_DE_REDE, passo).await {
        Ok(pronto) => pronto.map_err(Into::into),
        Err(_) => panic!(
            "{o_que} não terminou em {PRAZO_DE_REDE:?}. Não é o defeito que este \
             arquivo mede: é a máquina, ou o servidor que não subiu. Reprovar aqui \
             é melhor que pendurar a bateria inteira."
        ),
    }
}

/// Sobe um servidor numa porta que o sistema escolhe.
async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
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

fn destino(endereco: SocketAddr, apelido: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        impressao_esperada: None,
        // Nenhum MOD neste teste: o que se mede é sessão, não conjunto.
        aceito: None,
    }
}

/// O anfitrião, que é quem tem de continuar vendo a verdade no fim.
async fn anfitriao(endereco: SocketAddr) -> Result<Enlace> {
    Ok(Enlace::conectar(
        destino(endereco, "anfitriao"),
        SigningKey::from_bytes(&[71; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?)
}

/// Uma conexão crua, que é o que permite derrubá-la sem despedida.
///
/// [`Client`] e não [`Enlace`]: o enlace tem bateria interna e reconecta sozinho,
/// e este teste precisa decidir **quando** a segunda sessão sobe. A chave vem de
/// fora porque as duas conexões do visitante têm de ser a mesma conta.
async fn visitante(endereco: SocketAddr, chave: &SigningKey) -> Result<Client> {
    Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "visitante",
        chave,
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await
    .map_err(Into::into)
}

/// Um datagrama de voz bem-formado vindo de uma fonte.
fn datagrama(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let header = MediaHeader {
        version: seele_proto::PROTOCOL_VERSION,
        ssrc: ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut bytes = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = header
        .encode_datagram(b"ainda estou aqui", &mut bytes)
        .expect("um datagrama bem-formado");
    bytes.truncate(tamanho);
    bytes
}

/// Dobra até a condição valer, ou até o prazo acabar.
async fn absorver_ate<F>(enlace: &mut Enlace, room: &mut Room, prazo: Duration, pronto: F) -> bool
where
    F: Fn(&Room) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if pronto(room) {
            return true;
        }
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(100), enlace.proximo()).await
        {
            room.apply(&mensagem);
        }
    }
    pronto(room)
}

/// Dobra tudo o que chegar durante um tempo, e conta se o enlace **deste** lado
/// piscou.
///
/// O segundo valor existe para que uma reprovação diga de quem é a culpa. Se o
/// enlace do anfitrião cair e voltar no meio da medida, ele recebe uma fotografia
/// nova e o roster dele passa a ser outra conta — uma reprovação aí é contenção
/// da máquina, e não o defeito que este arquivo mede. Sem este número, as duas
/// reprovações são a mesma linha vermelha.
async fn absorver(enlace: &mut Enlace, room: &mut Room, por: Duration) -> usize {
    let fim = tokio::time::Instant::now() + por;
    let mut piscadas = 0;
    while tokio::time::Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(100), enlace.proximo()).await {
            Ok(Aviso::Mensagem(mensagem)) => {
                let _ = room.apply(&mensagem);
            }
            Ok(Aviso::Reconectado { sessao, .. }) => {
                piscadas += 1;
                room.adopt(&sessao, "anfitriao");
            }
            _ => {}
        }
    }
    piscadas
}

/// Dobra tudo o que chegar durante um tempo, e conta quantas vezes o servidor
/// disse que **esta** pessoa entrou **nesta** sala.
///
/// Conta evento, e não estado, porque é o evento que não depende de quem correu
/// primeiro: duas conexões respondendo ao mesmo anúncio deixam a mesma fotografia
/// no fim, e dois `PersonJoined` no caminho. O segundo valor é o mesmo de
/// [`absorver`], e existe pela mesma razão.
async fn contar_entradas(
    enlace: &mut Enlace,
    room: &mut Room,
    por: Duration,
    sala: VoiceRoomId,
    quem: PersonId,
) -> (usize, usize) {
    let fim = tokio::time::Instant::now() + por;
    let mut entradas = 0;
    let mut piscadas = 0;
    while tokio::time::Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(100), enlace.proximo()).await {
            Ok(Aviso::Mensagem(mensagem)) => {
                if let ServerMessage::PersonJoined {
                    voice_room,
                    profile,
                    ..
                } = mensagem.as_ref()
                {
                    if *voice_room == sala && profile.id == quem {
                        entradas += 1;
                    }
                }
                let _ = room.apply(&mensagem);
            }
            Ok(Aviso::Reconectado { sessao, .. }) => {
                piscadas += 1;
                room.adopt(&sessao, "anfitriao");
            }
            _ => {}
        }
    }
    (entradas, piscadas)
}

/// Dobra até uma pergunta feita ao servidor responder que sim, ou até o prazo.
///
/// Serve para o que não passa pelo enlace do anfitrião: o estado que o servidor
/// tem por dentro. Devolver `bool` em vez de entrar em pânico é o que deixa cada
/// chamador escrever a própria reprovação — as duas esperas deste arquivo falham
/// por motivos diferentes e merecem frases diferentes.
async fn esperar<F, Fut>(prazo: Duration, mut condicao: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let limite = tokio::time::Instant::now() + prazo;
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

/// Os apelidos sentados numa sala, como a tela os desenharia.
fn sentados(room: &Room, sala: VoiceRoomId) -> Vec<String> {
    let mut nomes: Vec<String> = room
        .roster(sala)
        .map(|pessoa| pessoa.nickname.clone())
        .collect();
    nomes.sort();
    nomes
}

/// Uma conexão viva num runtime próprio, para poder ser emudecida de fora.
struct Emudecivel {
    quem: PersonId,
    matar: std::sync::mpsc::Sender<()>,
    fio: std::thread::JoinHandle<()>,
}

impl Emudecivel {
    /// Conecta o visitante, senta-o na sala, e devolve o punho que o emudece.
    ///
    /// Num runtime próprio, em outra linha de execução, porque é o runtime que
    /// responde ao keepalive do QUIC: desligá-lo é a única forma honesta de
    /// produzir **silêncio** em vez de uma despedida. Derrubar a conexão pelo
    /// `Drop` do quinn escreveria `CONNECTION_CLOSE`, o servidor desmontaria a
    /// sessão na hora, e o teste mediria o caminho que já funciona.
    fn subir(endereco: SocketAddr, chave: SigningKey) -> Self {
        let (pronto_tx, pronto_rx) = std::sync::mpsc::channel();
        let (matar, aviso_de_morte) = std::sync::mpsc::channel();
        let fio = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("um runtime próprio");
            let cliente = rt.block_on(async {
                let mut cliente = visitante(endereco, &chave)
                    .await
                    .expect("o visitante não conectou");
                cliente
                    .enter_voice_room(VoiceRoomId(1), None)
                    .await
                    .expect("o visitante não entrou na sala");
                cliente
            });
            pronto_tx
                .send(cliente.session().person)
                .expect("ninguém esperando pelo visitante");
            // Vivo — e respondendo ao keepalive nas linhas de execução deste
            // runtime — até o sinal.
            let _ = aviso_de_morte.recv();
            // **A queda silenciosa, em duas partes.** O objeto é esquecido para
            // que o `Drop` do quinn não enfileire a despedida, e o runtime é
            // desligado para que ninguém mais responda a pacote nenhum. Do ponto
            // de vista do servidor, este par simplesmente parou de existir — que
            // é exactamente o que um Wi‑Fi que cai faz.
            std::mem::forget(cliente);
            rt.shutdown_timeout(Duration::from_millis(0));
        });
        let quem = pronto_rx
            .recv_timeout(Duration::from_secs(20))
            .expect("o visitante não chegou a conectar");
        Self { quem, matar, fio }
    }

    /// Emudece a conexão e espera a linha de execução encerrar.
    fn emudecer(self) {
        let _ = self.matar.send(());
        let _ = self.fio.join();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_reconecta_antes_de_o_servidor_desistir_da_conexao_velha_continua_no_roster_do_host(
) -> Result<()> {
    let (endereco, daemon) = servidor().await?;

    let mut anfitriao = com_prazo("o aperto de mão do anfitrião", anfitriao(endereco)).await?;
    let mut sala_do_anfitriao = Room::new();
    sala_do_anfitriao.adopt(anfitriao.sessao(), "anfitriao");
    // O anfitrião senta na sala porque a terceira asserção é sobre **ouvir**: um
    // datagrama só é encaminhado a quem está na mesma sala.
    com_prazo(
        "a entrada do anfitrião na sala",
        anfitriao.entrar_na_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    sala_do_anfitriao.enter_voice_room(VoiceRoomId(1));

    let chave_do_visitante = SigningKey::from_bytes(&[72; 32]);
    let primeira = Emudecivel::subir(endereco, chave_do_visitante.clone());
    let quem = primeira.quem;

    // A entrada normal chega ao anfitrião **antes** de qualquer queda. Sem isto o
    // fim deste teste poderia passar por um `PersonJoined` que nunca existiu.
    assert!(
        absorver_ate(
            &mut anfitriao,
            &mut sala_do_anfitriao,
            Duration::from_secs(10),
            |sala| sentados(sala, VoiceRoomId(1)).contains(&"visitante".to_owned()),
        )
        .await,
        "o anfitrião não viu nem a entrada normal; o teste não chegou a medir a volta"
    );

    // A rede cai sem dizer nada, e o cliente volta **na hora** — como volta de
    // verdade, porque a bateria interna dele desiste em 15 s e o servidor só
    // desiste da conexão velha em 20 s.
    primeira.emudecer();
    let mut segunda = com_prazo(
        "a volta do visitante",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    com_prazo(
        "a reentrada do visitante na sala",
        segunda.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    assert_eq!(
        segunda.session().person,
        quem,
        "as duas conexões do visitante não são a mesma conta; o teste mediria outra coisa"
    );

    // E agora o tempo ocioso inteiro, com o anfitrião ouvindo tudo o que o
    // servidor tiver a dizer. É aqui que a sessão velha morre.
    let piscadas = absorver(
        &mut anfitriao,
        &mut sala_do_anfitriao,
        seele_proto::transport::IDLE_TIMEOUT + MARGEM,
    )
    .await;

    // (0) **A sessão velha chegou mesmo a morrer, e morreu depois da volta.**
    // Antes das asserções de estado, esta: sem ela, as outras passariam também
    // num mundo em que o desmonte da sessão velha simplesmente não tivesse
    // acontecido dentro do prazo medido — passariam por ausência de evento, e não
    // por defesa, que é o contrário do que este arquivo existe para provar.
    //
    // O contador anda quando uma conexão chega ao fim e encontra a pessoa dela já
    // presente por outra conexão: exatamente a queda silenciosa encenada acima.
    // **Ele mede a encenação, não o veredito** — anda igual com o guarda armado e
    // com ele desarmado. É de propósito: se ele parasse junto com o guarda, toda
    // prova de reversão reprovaria aqui, acusando margem curta, e a asserção que
    // descreve o defeito de verdade nunca chegaria a ser avaliada.
    assert!(
        daemon.server().desassentamentos.conexoes_velhas() >= 1,
        "a sessão velha não chegou a ser desmontada dentro de {:?}: o servidor \
         não registrou nenhuma conexão velha morrendo depois da volta desta \
         pessoa. As asserções seguintes passariam por nada ter acontecido — então \
         elas não valem nada aqui. Isto é a encenação falhando, e não o guarda: \
         aumente a margem ou confira o tempo ocioso do transporte.",
        seele_proto::transport::IDLE_TIMEOUT + MARGEM
    );

    // A leitura dos contadores da sala **antes** do datagrama de (c): ela serve de
    // linha de base para (d) e, depois de tudo, para (e). A asserção sobre
    // `saida_de_sessao_velha` fica lá no fim de propósito — achado da revisão de
    // 2026-09-15: quando alguém desarma o guarda da sala para provar que ele
    // importa, a primeira linha vermelha tem de falar do que a pessoa perde
    // («a voz deixou de ser encaminhada»), e não de um contador interno. Aqui em
    // cima ela reprovava primeiro e escondia o efeito.
    let contadores = daemon
        .voice_rooms()
        .contadores(VoiceRoomId(1))
        .await
        .expect("a sala 1 está viva: as duas pessoas entraram nela");

    // (a) O visitante continua no roster de quem ficou.
    //
    // A reprovação carrega **as três coisas** de que um diagnóstico precisa: o que
    // o anfitrião desenha, o que o servidor tem, e se o enlace do anfitrião
    // piscou no meio. Sem isso, contenção da máquina e o defeito de verdade dão a
    // mesma linha vermelha, e quem investiga começa supondo.
    let no_servidor: Vec<String> = daemon
        .server()
        .occupancy
        .lock()
        .await
        .in_voice_room(VoiceRoomId(1))
        .iter()
        .map(|quem_esta| format!("{}/sessão {}", quem_esta.nickname, quem_esta.sessao))
        .collect();
    assert!(
        sentados(&sala_do_anfitriao, VoiceRoomId(1)).contains(&"visitante".to_owned()),
        "a sessão velha, ao morrer, apagou a nova do roster do anfitrião: {:?}\n\
         O desmonte é chaveado por pessoa e não confere de qual sessão ele é.\n\
         No servidor, a sala tem: {no_servidor:?}\n\
         O enlace do anfitrião piscou {piscadas} vez(es) durante a medida — \
         se for mais que zero, ele trocou de fotografia e esta reprovação é da \
         máquina, não do servidor.",
        sentados(&sala_do_anfitriao, VoiceRoomId(1))
    );

    // (b) E continua na lista de presentes do servidor.
    //
    // Esta é a asserção que reprova quando o filtro de sessão de
    // `Presentes::saiu` é desarmado, e por isso ela carrega o diagnóstico
    // inteiro: quem o servidor acha que está aqui, e por qual sessão.
    let nos_presentes: Vec<String> = daemon
        .server()
        .presentes
        .lock()
        .await
        .todos()
        .iter()
        .map(|quem_esta| format!("{}/sessão {}", quem_esta.person, quem_esta.sessao))
        .collect();
    assert!(
        nos_presentes
            .iter()
            .any(|linha| linha.starts_with(&format!("{quem}/"))),
        "a sessão velha tirou o visitante dos presentes, e ele está conectado \
         agora por outra conexão. O servidor difundiu um `PersonGone` que não \
         aconteceu, e a própria pessoa não é avisada — ela continua se vendo \
         dentro da sala enquanto some da lista de todo mundo. É a outra metade do \
         defeito relatado.\n\
         Visitante procurado: {quem}\n\
         Presentes no servidor: {nos_presentes:?}"
    );

    // (c) E a voz dele ainda chega a quem está na sala — isto é, a tarefa da sala
    // não o esqueceu, e o datagrama não morre como `not_a_member`.
    let ouvido = anfitriao.media();
    let voz = datagrama(segunda.session().ssrc, 1);
    segunda.send_media(voz.clone())?;
    let chegou = tokio::time::timeout(Duration::from_secs(3), ouvido.next()).await;
    assert!(
        matches!(chegou, Ok(Ok(ref bytes)) if *bytes == voz),
        "a voz do visitante deixou de ser encaminhada: a sessão velha o tirou da \
         tarefa da sala ao morrer. É a metade do defeito que o usuário relata como \
         «não ouço nem vejo esse amigo»."
    );

    // (d) E a sala não descartou esse datagrama como de quem não é membro.
    //
    // Chegar ao anfitrião já é o efeito, mas o aceite pede a letra: o datagrama
    // é encaminhado **sem subir `not_a_member`**. A diferença importa porque o
    // encaminhamento pode sobreviver a um descarte parcial — um segundo ouvinte
    // barrado, por exemplo — e aí (c) passaria com a sala ainda esquecendo
    // alguém. Compara-se com a leitura de antes do envio, e não com zero cru,
    // para que a asserção fale do datagrama que este teste mandou.
    let depois = daemon
        .voice_rooms()
        .contadores(VoiceRoomId(1))
        .await
        .expect("a sala 1 continua viva: o datagrama acabou de atravessá-la");
    assert_eq!(
        depois.not_a_member, contadores.not_a_member,
        "a sala descartou o datagrama do visitante como de quem não é membro: \
         antes do envio eram {} descartes e agora são {}. A sessão velha, ao \
         morrer, tirou o visitante da tarefa da sala — o encaminhamento em (c) \
         sobreviveu por outro caminho, mas a sala já não o reconhece. \
         Contadores completos: {depois:?}",
        contadores.not_a_member, depois.not_a_member
    );

    // (e) **E a sala se defendeu de propósito**, pela metade de mídia do guarda.
    //
    // (c) e (d) provam o efeito — o datagrama chega e ninguém o descarta —, mas o
    // efeito sozinho não distingue «a sala barrou a saída da sessão velha» de «a
    // saída velha nunca chegou até a sala». Este contador distingue: ele só anda
    // dentro da tarefa da sala, quando um `Leave` chega de uma sessão que já não é
    // a desta pessoa.
    //
    // Ele vem por último porque é a asserção de **mecanismo**, e as de efeito são
    // as que descrevem o defeito relatado. Quem desarma o guarda para provar que
    // ele importa lê primeiro «a voz do visitante deixou de ser encaminhada», que
    // é a frase do usuário, e não um número interno.
    assert!(
        depois.saida_de_sessao_velha >= 1,
        "a tarefa da sala não registrou nenhuma saída de sessão velha barrada. \
         Ou o desmonte da sessão velha não chegou até a sala, ou ele chegou sem \
         dizer de qual sessão era — e aí as asserções (c) e (d) acima passaram por \
         acaso, e não por defesa. Contadores da sala: {depois:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// A saída **pedida** pela conexão que já não é a vigente.
///
/// # Por que este segundo teste existe
///
/// O primeiro mede a queda silenciosa, e nela a conexão velha morre calada: ela
/// não pede nada, e por isso todo o desmonte dela passa pelo ramo que varre a
/// lotação inteira (`Occupancy::vacate_everywhere_da_sessao`). A remoção **por
/// sala** — `Occupancy::vacate_da_sessao` — nunca era exercida contra um servidor
/// de verdade: ela só é alcançada por quem se lembra de qual sala estava, isto é,
/// por `LeaveVoiceRoom` e pelo anúncio de mudança de sala. Revisão independente
/// apontou o buraco: revertendo o filtro de sessão daquela função, a bateria
/// inteira continuava verde.
///
/// # O caminho que chega até lá
///
/// Duas conexões da mesma pessoa vivas ao mesmo tempo, e a velha **ainda
/// falando** — o caso do aparelho que ficou aberto, ou da janela antiga que o
/// dono fecha depois de já ter voltado por outra. A velha pede para sair da sala
/// de que ela se lembra; a nova está sentada nessa mesma sala. Sem o filtro de
/// sessão, o pedido da velha desocupa o assento da nova e anuncia um `PersonLeft`
/// que não aconteceu — o mesmo estrago da queda silenciosa, por uma porta que o
/// outro teste não abre.
///
/// Não custa o tempo ocioso: aqui ninguém espera o servidor desistir de nada.
#[tokio::test(flavor = "multi_thread")]
async fn a_saida_pedida_pela_conexao_velha_nao_tira_da_sala_a_conexao_nova() -> Result<()> {
    let (endereco, daemon) = servidor().await?;

    let mut anfitriao = com_prazo("o aperto de mão do anfitrião", anfitriao(endereco)).await?;
    let mut sala_do_anfitriao = Room::new();
    sala_do_anfitriao.adopt(anfitriao.sessao(), "anfitriao");
    com_prazo(
        "a entrada do anfitrião na sala",
        anfitriao.entrar_na_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    sala_do_anfitriao.enter_voice_room(VoiceRoomId(1));

    // Outra chave que a do primeiro teste: as duas execuções não compartilham
    // servidor, mas compartilhar a conta faria um diagnóstico confundir os dois
    // arquivos de log.
    let chave_do_visitante = SigningKey::from_bytes(&[73; 32]);
    let mut velha = com_prazo(
        "o aperto de mão da conexão velha",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    com_prazo(
        "a entrada da conexão velha na sala",
        velha.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    let quem = velha.session().person;

    assert!(
        absorver_ate(
            &mut anfitriao,
            &mut sala_do_anfitriao,
            Duration::from_secs(10),
            |sala| sentados(sala, VoiceRoomId(1)).contains(&"visitante".to_owned()),
        )
        .await,
        "o anfitrião não viu nem a entrada normal; o teste não chegou a medir a saída"
    );

    // A conexão nova sobe **sem** a velha se despedir, e toma a sala.
    let mut nova = com_prazo(
        "o aperto de mão da conexão nova",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    assert_eq!(
        nova.session().person,
        quem,
        "as duas conexões do visitante não são a mesma conta; o teste mediria outra coisa"
    );
    com_prazo(
        "a entrada da conexão nova na sala",
        nova.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;

    // E o servidor tem de **já** ter trocado o assento antes de a velha pedir
    // para sair. Sem esta espera, um pedido que chegasse primeiro sairia da sala
    // legitimamente, o guarda nunca seria consultado, e o teste passaria verde
    // sem ter medido nada. A fonte da nova (`ssrc`) é o que distingue as duas
    // conexões da mesma pessoa.
    let fonte_da_nova = nova.session().ssrc;
    assert!(
        esperar(Duration::from_secs(10), || async {
            daemon
                .server()
                .occupancy
                .lock()
                .await
                .in_voice_room(VoiceRoomId(1))
                .iter()
                .any(|quem_esta| quem_esta.ssrc == fonte_da_nova)
        })
        .await,
        "o servidor não chegou a sentar a conexão nova; a saída da velha seria legítima \
         e este teste não mediria o guarda"
    );

    // O pedido da conexão velha — a quinta cópia do desmonte, a que se lembra da
    // sala e por isso chega à remoção por sala.
    velha.leave_voice_room().await?;

    // A espera pelo contador da sala serve de **barreira**: ela segura o teste até
    // o pedido da conexão velha ter chegado mesmo à tarefa da sala, para que as
    // asserções de estado não passem por ausência de evento. O veredito sobre ela
    // fica no fim, como no outro teste — quem desarma o guarda para provar que ele
    // importa tem de ler primeiro o que a pessoa perde, e não um número interno.
    let barrado = esperar(Duration::from_secs(10), || async {
        daemon
            .voice_rooms()
            .contadores(VoiceRoomId(1))
            .await
            .is_some_and(|contadores| contadores.saida_de_sessao_velha >= 1)
    })
    .await;

    // E então o tempo de o estrago, se houvesse, chegar ao anfitrião. O
    // `PersonLeft` indevido sairia no mesmo instante do pedido; dois segundos são
    // folga larga contra `127.0.0.1`.
    let piscadas = absorver(
        &mut anfitriao,
        &mut sala_do_anfitriao,
        Duration::from_secs(2),
    )
    .await;

    let no_servidor: Vec<String> = daemon
        .server()
        .occupancy
        .lock()
        .await
        .in_voice_room(VoiceRoomId(1))
        .iter()
        .map(|quem_esta| format!("{}/sessão {}", quem_esta.nickname, quem_esta.sessao))
        .collect();
    assert!(
        sentados(&sala_do_anfitriao, VoiceRoomId(1)).contains(&"visitante".to_owned()),
        "a saída pedida pela conexão velha tirou a nova do roster do anfitrião: {:?}\n\
         `Occupancy::vacate_da_sessao` removeu por pessoa, sem conferir de qual sessão \
         era o assento.\n\
         No servidor, a sala tem: {no_servidor:?}\n\
         O enlace do anfitrião piscou {piscadas} vez(es) durante a medida — se for mais \
         que zero, ele trocou de fotografia e esta reprovação é da máquina.",
        sentados(&sala_do_anfitriao, VoiceRoomId(1))
    );

    // E o assento da nova continua lá, com a fonte dela. A asserção acima é sobre
    // o que o anfitrião desenha; esta é sobre o que o servidor tem, e as duas
    // juntas separam «o evento indevido não saiu» de «o evento saiu e o cliente o
    // ignorou».
    assert!(
        daemon
            .server()
            .occupancy
            .lock()
            .await
            .in_voice_room(VoiceRoomId(1))
            .iter()
            .any(|quem_esta| quem_esta.ssrc == fonte_da_nova),
        "a saída pedida pela conexão velha desocupou o assento da nova no servidor: \
         {no_servidor:?}"
    );

    // E, por último, a asserção de mecanismo: a sala barrou a saída velha de
    // propósito. As duas acima dizem que o estado ficou de pé; esta diz que ele
    // ficou porque alguém o defendeu, e não porque o pedido se perdeu no caminho.
    assert!(
        barrado,
        "a tarefa da sala não registrou nenhuma saída de sessão velha barrada: o pedido \
         da conexão velha não chegou, ou chegou sem dizer de qual sessão era — e aí as \
         asserções acima passaram por acaso, e não por defesa."
    );

    daemon.shutdown();
    Ok(())
}

/// A reserva de assento deixada por quem já não é a conexão desta pessoa.
///
/// # A porta que faltava
///
/// Os dois testes acima medem **remoção**: uma conexão velha apagando o que é da
/// nova. Este mede a outra metade da mesma família, que é **escrita**: a conexão
/// velha, ao morrer, guardava o assento da carência com a sala e o `ssrc` dela, e
/// essa reserva vale cinco minutos. Quem saiu de uma sala de propósito e voltou
/// caía de novo dentro dela sem ter pedido — por causa de uma conexão anterior.
///
/// Revisão independente apontou que o guarda dessa escrita tinha teste de unidade
/// e nenhum caminho de ponta a ponta. Este é o caminho.
///
/// # Como se chega até lá sem esperar o tempo ocioso
///
/// A reserva é escrita no fim de uma conexão que se lembra de uma sala. Não é
/// preciso silêncio nenhum: basta que a conexão que morre já não seja a vigente.
/// Duas conexões da mesma pessoa, a velha dentro da sala e a nova fora dela; a
/// velha se despede, e a pergunta «esta ainda é a conexão desta pessoa?» responde
/// não.
#[tokio::test(flavor = "multi_thread")]
async fn a_conexao_velha_nao_guarda_assento_para_quem_ja_voltou_por_outra() -> Result<()> {
    let (endereco, daemon) = servidor().await?;

    let mut anfitriao = com_prazo("o aperto de mão do anfitrião", anfitriao(endereco)).await?;
    let mut sala_do_anfitriao = Room::new();
    sala_do_anfitriao.adopt(anfitriao.sessao(), "anfitriao");
    com_prazo(
        "a entrada do anfitrião na sala",
        anfitriao.entrar_na_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    sala_do_anfitriao.enter_voice_room(VoiceRoomId(1));

    let chave_do_visitante = SigningKey::from_bytes(&[74; 32]);
    let mut velha = com_prazo(
        "o aperto de mão da conexão velha",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    com_prazo(
        "a entrada da conexão velha na sala",
        velha.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    let quem = velha.session().person;
    assert!(
        absorver_ate(
            &mut anfitriao,
            &mut sala_do_anfitriao,
            Duration::from_secs(10),
            |sala| sentados(sala, VoiceRoomId(1)).contains(&"visitante".to_owned()),
        )
        .await,
        "o anfitrião não viu nem a entrada normal; o teste não chegou a medir a reserva"
    );

    // A conexão nova sobe e **fica fora da sala**: é ela que passa a ser a
    // vigente, e é a sala vazia dela que torna a reserva da velha obsoleta.
    let nova = com_prazo(
        "o aperto de mão da conexão nova",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    assert_eq!(
        nova.session().person,
        quem,
        "as três conexões do visitante têm de ser a mesma conta"
    );

    // A velha se despede. Despedida limpa de propósito: o que se mede aqui é a
    // escrita da reserva, e não o tempo ocioso — o silêncio só adiaria o mesmo
    // caminho em vinte segundos.
    //
    // A barreira abaixo mede a **encenação**, e não o veredito: `conexoes_velhas`
    // anda quando uma conexão chega ao fim já não sendo a vigente da pessoa dela,
    // e isso vale com o guarda da reserva armado e desarmado — ele nem participa
    // dessa contagem. É o contrário de esperar por `reservas_obsoletas`, que só
    // anda quando o guarda recusa: desarmado o guarda, aquela espera estouraria
    // primeiro e a linha vermelha diria «não recusou reserva nenhuma», escondendo
    // o estrago que vem logo adiante. O veredito fica para o fim do teste.
    let velhas_antes = daemon.server().desassentamentos.conexoes_velhas();
    drop(velha);
    assert!(
        esperar(Duration::from_secs(10), || async {
            daemon.server().desassentamentos.conexoes_velhas() > velhas_antes
        })
        .await,
        "a conexão velha não chegou ao fim dentro do prazo, ou chegou ainda sendo a \
         vigente desta pessoa: o teste não encenou a reserva obsoleta e nada do que vem \
         depois mede o que promete."
    );

    // E agora a volta de verdade, que é onde a reserva obsoleta cobraria: a
    // conexão nova sai, e a pessoa reconecta.
    drop(nova);
    let terceira = com_prazo(
        "o aperto de mão da volta",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;

    // Ela não pode acordar dentro de uma sala que não pediu. Espera-se pelo
    // **defeito**: se ele aparecer em até três segundos, isto reprova; se não
    // aparecer, a espera custa os três segundos e é o preço de uma asserção
    // negativa honesta.
    let re_sentado = esperar(Duration::from_secs(3), || async {
        daemon
            .server()
            .occupancy
            .lock()
            .await
            .in_voice_room(VoiceRoomId(1))
            .iter()
            .any(|quem_esta| quem_esta.person == quem)
    })
    .await;
    assert!(
        !re_sentado,
        "a volta caiu dentro da sala 1 sem ter pedido: a conexão velha guardou o assento \
         da carência depois de a pessoa já ter voltado por outra conexão, e a reserva \
         valeu para a volta seguinte. É o mesmo defeito de sessão das remoções, pelo lado \
         da escrita."
    );

    // E quem ficou não viu ninguém entrar.
    let piscadas = absorver(
        &mut anfitriao,
        &mut sala_do_anfitriao,
        Duration::from_millis(500),
    )
    .await;
    assert!(
        !sentados(&sala_do_anfitriao, VoiceRoomId(1)).contains(&"visitante".to_owned()),
        "o anfitrião viu o visitante reaparecer na sala sem ter entrado: {:?}\n\
         O enlace do anfitrião piscou {piscadas} vez(es) durante a medida.",
        sentados(&sala_do_anfitriao, VoiceRoomId(1))
    );

    // Só agora o veredito, e depois das asserções de efeito: o guarda recusou a
    // reserva em vez de a pessoa ter escapado por outro caminho. Se esta reprovar
    // sozinha, o estrago não apareceu e o mecanismo que o evita também não — é
    // sinal de teste que parou de medir, não de defeito de quem usa.
    assert!(
        daemon.server().desassentamentos.reservas_obsoletas() >= 1,
        "ninguém caiu numa sala que não pediu, mas o servidor também não recusou reserva \
         nenhuma: o caminho da reserva de carência não foi exercido e este teste deixou \
         de cobrir o guarda que diz cobrir."
    );

    drop(terceira);
    daemon.shutdown();
    Ok(())
}

/// O anúncio de mudança de sala **respondido** pela conexão que já não é a vigente.
///
/// # A sétima porta, e por que ela precisava de um servidor de verdade
///
/// As outras seis passam por `desassentar`, e o compilador não deixa esquecer o
/// guarda nelas. Esta não: ela é uma pergunta solta num braço de `match`, feita
/// antes de responder a `Event::PersonMoved`. Apagar essa pergunta compila, e até
/// aqui a única coisa que a prendia era um teste que **lê o próprio arquivo-fonte**
/// — o que prova o local da chamada e nada sobre o efeito. Revisão independente
/// registrou a ressalva; este teste é a resposta a ela.
///
/// # O que acontece sem o guarda
///
/// `Event::PersonMoved` é difundido e chega a **todas** as conexões da pessoa
/// movida, inclusive à velha da queda silenciosa. Cada uma que o responde chama
/// `assentar`, que de propósito tira a pessoa de toda sala anterior sem conferir
/// sessão — e então a senta de novo, com o `ssrc` e o canal **dela**. Respondido
/// pelas duas, o mesmo movimento é feito duas vezes, e uma das duas senta a pessoa
/// com um canal morto.
///
/// # A medida, e por que ela não depende de quem correu primeiro
///
/// Qual das duas escreve por último é escalonamento, e um teste que dependa disso
/// passa verde na metade das execuções mesmo com o defeito. O que **não** depende
/// da ordem é a contagem: `assentar` anuncia `PersonJoined` sempre, ao fim. Com o
/// guarda, o anfitrião vê a pessoa entrar na sala nova **uma** vez; sem ele, duas.
/// É por isso que aqui se conta evento, e não se fotografa estado.
///
/// Não custa o tempo ocioso: ninguém espera o servidor desistir de conexão nenhuma.
#[tokio::test(flavor = "multi_thread")]
async fn o_anuncio_de_mudanca_de_sala_nao_e_respondido_pela_conexao_velha() -> Result<()> {
    let (endereco, daemon) = servidor().await?;

    let mut anfitriao = com_prazo("o aperto de mão do anfitrião", anfitriao(endereco)).await?;
    let mut sala_do_anfitriao = Room::new();
    sala_do_anfitriao.adopt(anfitriao.sessao(), "anfitriao");
    com_prazo(
        "a entrada do anfitrião na sala",
        anfitriao.entrar_na_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    sala_do_anfitriao.enter_voice_room(VoiceRoomId(1));

    // Precisa haver para onde mover. O anfitrião é quem tem a permissão, por ser
    // quem chegou primeiro ao servidor recém-criado.
    com_prazo(
        "o pedido de criar a sala de destino",
        anfitriao.criar_voice_room("FUNDOS".into(), 8, None),
    )
    .await?;
    assert!(
        absorver_ate(
            &mut anfitriao,
            &mut sala_do_anfitriao,
            Duration::from_secs(10),
            |sala| sala.find_voice_room("FUNDOS").is_some(),
        )
        .await,
        "a sala de destino não foi criada; não há para onde mover ninguém e este teste \
         não mediria nada"
    );
    let destino = sala_do_anfitriao
        .find_voice_room("FUNDOS")
        .expect("a sala de destino acabou de aparecer");

    // Outra chave ainda: cada teste deste arquivo tem a sua, para que um log não
    // confunda dois casos.
    let chave_do_visitante = SigningKey::from_bytes(&[75; 32]);
    let mut velha = com_prazo(
        "o aperto de mão da conexão velha",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    com_prazo(
        "a entrada da conexão velha na sala",
        velha.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    let quem = velha.session().person;
    assert!(
        absorver_ate(
            &mut anfitriao,
            &mut sala_do_anfitriao,
            Duration::from_secs(10),
            |sala| sentados(sala, VoiceRoomId(1)).contains(&"visitante".to_owned()),
        )
        .await,
        "o anfitrião não viu nem a entrada normal; o teste não chegou a medir a mudança \
         de sala"
    );

    // A conexão nova sobe sem a velha se despedir, e passa a ser a vigente.
    let mut nova = com_prazo(
        "o aperto de mão da conexão nova",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    assert_eq!(
        nova.session().person,
        quem,
        "as duas conexões do visitante não são a mesma conta; o teste mediria outra coisa"
    );
    com_prazo(
        "a entrada da conexão nova na sala",
        nova.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;

    // E o servidor tem de **já** ter trocado o assento antes do anúncio. Sem esta
    // espera, a velha ainda seria a vigente quando o evento saísse, responder a
    // ele seria legítimo, e o guarda nunca seria consultado.
    let fonte_da_nova = nova.session().ssrc;
    assert!(
        esperar(Duration::from_secs(10), || async {
            daemon
                .server()
                .occupancy
                .lock()
                .await
                .in_voice_room(VoiceRoomId(1))
                .iter()
                .any(|quem_esta| quem_esta.ssrc == fonte_da_nova)
        })
        .await,
        "o servidor não chegou a sentar a conexão nova; o anúncio seria respondido por \
         uma conexão que ainda é a vigente e este teste não mediria o guarda"
    );

    let recusas_antes = daemon.server().desassentamentos.conexoes_velhas();

    // O operador move a pessoa. O anúncio sai para as duas conexões dela.
    com_prazo(
        "o pedido de mover a pessoa",
        anfitriao.mover_pessoa(quem, destino),
    )
    .await?;

    // Três segundos de escuta: o segundo `PersonJoined`, se houvesse, sairia no
    // mesmo instante do primeiro, contra `127.0.0.1`.
    let (entradas, piscadas) = contar_entradas(
        &mut anfitriao,
        &mut sala_do_anfitriao,
        Duration::from_secs(3),
        destino,
        quem,
    )
    .await;

    assert_eq!(
        entradas, 1,
        "o anfitrião viu o visitante entrar {entradas} vez(es) na sala de destino, e a \
         mudança foi uma só.\n\
         Duas entradas significam que a conexão velha também respondeu ao anúncio: ela \
         chamou `assentar`, que tira a pessoa de toda sala anterior sem conferir sessão, \
         e a re-sentou com o `ssrc` e o canal dela — mortos. É o defeito desta pendência \
         pela sétima porta, a única em que o guarda não está dentro de `desassentar`.\n\
         Zero entradas significam que a mudança não chegou ao anfitrião no prazo, e aí \
         esta reprovação é da máquina.\n\
         O enlace do anfitrião piscou {piscadas} vez(es) durante a medida — se for mais \
         que zero, ele trocou de fotografia e esta reprovação também é da máquina."
    );

    // E a sensibilidade, depois do estrago: sem esta, «uma entrada só» também
    // seria o que se veria num mundo onde o anúncio nunca chegou à conexão velha.
    assert!(
        daemon.server().desassentamentos.conexoes_velhas() > recusas_antes,
        "o servidor não recusou anúncio nenhum: a conexão velha não chegou a receber a \
         mudança de sala desta pessoa, e a contagem acima passou por ausência de evento \
         e não por defesa."
    );

    // E quem ficou de pé é a conexão nova, com a fonte dela, na sala nova.
    let no_destino: Vec<String> = daemon
        .server()
        .occupancy
        .lock()
        .await
        .in_voice_room(destino)
        .iter()
        .map(|quem_esta| format!("{}/sessão {}", quem_esta.nickname, quem_esta.sessao))
        .collect();
    assert!(
        daemon
            .server()
            .occupancy
            .lock()
            .await
            .in_voice_room(destino)
            .iter()
            .any(|quem_esta| quem_esta.ssrc == fonte_da_nova),
        "a sala de destino não ficou com o assento da conexão nova: {no_destino:?}"
    );

    drop(velha);
    drop(nova);
    daemon.shutdown();
    Ok(())
}

/// A tela que a conexão nova abriu, e a conexão velha que morre depois dela.
///
/// # A ressalva que este teste fecha
///
/// Das portas do desmonte, o fim de tela era a única presa **só** por testes de
/// unidade e por uma prova que lê o próprio arquivo-fonte. Revisão independente
/// registrou a ressalva com a palavra certa: a prova de leitura de fonte diz
/// qual função é chamada onde, e não diz nada sobre efeito — ela passaria
/// inteira num servidor em que a chamada certa não produzisse o resultado certo,
/// e reprovaria numa reescrita equivalente e correta. Na mesma entrega uma prova
/// desse feitio saiu do guarda de mudança de sala por essa fragilidade; esta é a
/// resposta para a que sobrou.
///
/// # O que a pessoa sente quando isto quebra
///
/// É a metade **ver** do relato: «não ouço nem vejo esse amigo». A voz é a tarefa
/// da sala, e os outros testes a prendem; a imagem é este registro. Sem o guarda,
/// a conexão velha, ao morrer, encerra a transmissão da pessoa inteira — inclusive
/// a que a conexão nova acabou de abrir — e difunde um `ScreenShareStopped` que
/// não aconteceu. Quem transmite continua mandando quadros; quem assiste vê a
/// janela fechar sozinha.
///
/// # Como se chega até lá sem esperar o tempo ocioso
///
/// Pelo mesmo caminho do teste da reserva: o silêncio não é necessário, só é
/// necessário que a conexão que morre já não seja a vigente. A velha transmite, a
/// nova sobe e toma a sala — e aí a tela da velha é encerrada de propósito, por
/// `assentar`, que é a troca de sala e leva embora a transmissão anterior da
/// pessoa. A nova abre a dela, e só então a velha se despede.
#[tokio::test(flavor = "multi_thread")]
async fn a_conexao_velha_nao_encerra_a_tela_que_a_nova_abriu() -> Result<()> {
    let (endereco, daemon) = servidor().await?;

    let mut anfitriao = com_prazo("o aperto de mão do anfitrião", anfitriao(endereco)).await?;
    let mut sala_do_anfitriao = Room::new();
    sala_do_anfitriao.adopt(anfitriao.sessao(), "anfitriao");
    // O anfitrião entra na sala porque `ScreenShareStarted` vai a quem está nela:
    // é o enlace dele que diz se a transmissão ficou desenhada ou sumiu.
    com_prazo(
        "a entrada do anfitrião na sala",
        anfitriao.entrar_na_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    sala_do_anfitriao.enter_voice_room(VoiceRoomId(1));

    let chave_do_visitante = SigningKey::from_bytes(&[75; 32]);
    let mut velha = com_prazo(
        "o aperto de mão da conexão velha",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    com_prazo(
        "a entrada da conexão velha na sala",
        velha.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    let quem = velha.session().person;
    velha.start_screen_share().await?;

    // A transmissão da velha chegou mesmo a existir. Sem esta espera, o resto
    // mediria uma pessoa que nunca transmitiu, e o guarda nunca seria consultado.
    assert!(
        esperar(Duration::from_secs(10), || async {
            daemon
                .server()
                .telas
                .lock()
                .await
                .de(quem, velha.session().id)
                .is_some()
        })
        .await,
        "a conexão velha não chegou a transmitir; este teste não mediria fim de tela nenhum"
    );

    // A conexão nova sobe e toma a sala. A tela da velha acaba aqui, de propósito
    // e pela porta certa: `assentar` leva embora a transmissão anterior da pessoa,
    // porque quem muda de sala não pode continuar desenhado na de antes.
    let mut nova = com_prazo(
        "o aperto de mão da conexão nova",
        visitante(endereco, &chave_do_visitante),
    )
    .await?;
    assert_eq!(
        nova.session().person,
        quem,
        "as duas conexões do visitante não são a mesma conta; o teste mediria outra coisa"
    );
    com_prazo(
        "a entrada da conexão nova na sala",
        nova.enter_voice_room(VoiceRoomId(1), None),
    )
    .await?;
    nova.start_screen_share().await?;

    let sessao_nova = nova.session().id;
    assert!(
        esperar(Duration::from_secs(10), || async {
            daemon
                .server()
                .telas
                .lock()
                .await
                .de(quem, sessao_nova)
                .is_some()
        })
        .await,
        "o servidor não registrou a transmissão da conexão nova; sem ela não há o que \
         a conexão velha pudesse apagar"
    );
    let tela_da_nova = daemon
        .server()
        .telas
        .lock()
        .await
        .de(quem, sessao_nova)
        .expect("a asserção acima acabou de encontrá-la");

    // E o anfitrião a viu começar. A asserção do fim é sobre ela continuar lá:
    // sem esta, «continuar» poderia ser «nunca ter chegado».
    assert!(
        absorver_ate(
            &mut anfitriao,
            &mut sala_do_anfitriao,
            Duration::from_secs(10),
            |sala| sala.telas.contains_key(&tela_da_nova.1),
        )
        .await,
        "o anfitrião não viu a transmissão da conexão nova começar; o teste não chegou \
         a medir o fim de tela"
    );

    let conexoes_velhas_antes = daemon.server().desassentamentos.conexoes_velhas();

    // A velha se despede. Despedida limpa pelo mesmo motivo do teste da reserva:
    // o que se mede é de quem é o fim de tela, e o silêncio só adiaria o mesmo
    // caminho em vinte segundos.
    drop(velha);

    // Ela chegou mesmo ao fim, e chegou depois de a pessoa já estar aqui por
    // outra conexão — a encenação, antes do veredito. O contador anda igual com o
    // guarda armado e com ele desarmado, de propósito: é o que impede que a prova
    // de reversão reprove aqui, acusando margem curta, em vez de reprovar na
    // asserção que descreve o defeito.
    assert!(
        esperar(Duration::from_secs(10), || async {
            daemon.server().desassentamentos.conexoes_velhas() > conexoes_velhas_antes
        })
        .await,
        "a conexão velha não chegou ao fim dentro do prazo: o servidor não registrou \
         nenhuma conexão velha morrendo depois de esta pessoa já ter voltado por outra. \
         As asserções seguintes passariam por nada ter acontecido."
    );

    // E o tempo de o estrago, se houvesse, chegar ao anfitrião: o
    // `ScreenShareStopped` indevido sai no mesmo instante do desmonte.
    let piscadas = absorver(
        &mut anfitriao,
        &mut sala_do_anfitriao,
        Duration::from_secs(2),
    )
    .await;

    // (a) O servidor ainda tem a transmissão da conexão nova.
    let no_servidor = daemon.server().telas.lock().await.em(VoiceRoomId(1));
    assert_eq!(
        daemon.server().telas.lock().await.de(quem, sessao_nova),
        Some(tela_da_nova),
        "a conexão velha, ao morrer, encerrou a transmissão que a conexão nova tinha \
         aberto: o fim de tela do desmonte é da pessoa inteira e não confere de qual \
         conexão ele é.\n\
         Transmissão procurada: {tela_da_nova:?} da sessão {sessao_nova}\n\
         Transmitindo na sala 1 agora: {no_servidor:?}"
    );

    // (b) E o anfitrião continua vendo. A asserção acima é sobre o que o servidor
    // tem; esta é sobre o que quem assiste desenha, e as duas juntas separam «o
    // evento indevido não saiu» de «saiu e o cliente o ignorou».
    assert!(
        sala_do_anfitriao.telas.contains_key(&tela_da_nova.1),
        "o anfitrião recebeu o fim de uma transmissão que não acabou: a janela fecha \
         sozinha enquanto quem transmite continua mandando quadros. É a metade **ver** \
         do relato «não ouço nem vejo esse amigo».\n\
         Telas desenhadas pelo anfitrião: {:?}\n\
         O enlace do anfitrião piscou {piscadas} vez(es) durante a medida — se for mais \
         que zero, ele trocou de fotografia e esta reprovação é da máquina.",
        sala_do_anfitriao.telas.keys().collect::<Vec<_>>()
    );

    drop(nova);
    daemon.shutdown();
    Ok(())
}
