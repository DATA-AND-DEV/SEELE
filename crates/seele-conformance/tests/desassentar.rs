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

    // (0) **A sessão velha morreu, e o guarda a defendeu.** Antes das três
    // asserções de estado, esta: sem ela, as outras três passariam também num
    // mundo em que o desmonte da sessão velha simplesmente não tivesse
    // acontecido dentro do prazo medido — passariam por ausência de evento, e não
    // por defesa, que é o contrário do que este arquivo existe para provar.
    //
    // O contador só anda quando uma conexão chega ao fim e encontra a pessoa dela
    // já presente por outra conexão: exatamente a queda silenciosa encenada
    // acima.
    assert!(
        daemon.server().desassentamentos.conexoes_velhas() >= 1,
        "a sessão velha não chegou a ser desmontada dentro de {:?}: o servidor \
         não registrou nenhuma conexão velha morrendo depois da volta desta \
         pessoa. As asserções seguintes passariam por nada ter acontecido, e não \
         porque o guarda defendeu — então elas não valem nada aqui. Aumente a \
         margem ou confira o tempo ocioso do transporte.",
        seele_proto::transport::IDLE_TIMEOUT + MARGEM
    );

    // (0b) **E a sala também se defendeu**, pela metade de mídia do guarda.
    //
    // A asserção (c) abaixo prova o efeito — o datagrama chega —, mas o efeito
    // sozinho não distingue «a sala barrou a saída velha» de «a saída velha
    // nunca chegou à sala». Este contador distingue: ele só anda dentro da
    // tarefa da sala, quando um `Leave` chega de uma sessão que já não é a desta
    // pessoa. Sem ele, o contador existia e ninguém de ponta a ponta o lia.
    let contadores = daemon
        .voice_rooms()
        .contadores(VoiceRoomId(1))
        .await
        .expect("a sala 1 está viva: as duas pessoas entraram nela");
    assert!(
        contadores.saida_de_sessao_velha >= 1,
        "a tarefa da sala não registrou nenhuma saída de sessão velha barrada. \
         Ou o desmonte da sessão velha não chegou até a sala, ou ele chegou sem \
         dizer de qual sessão era — e aí a asserção (c) abaixo passaria por \
         acaso. Contadores da sala: {contadores:?}"
    );

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
    let presente = daemon
        .server()
        .presentes
        .lock()
        .await
        .todos()
        .iter()
        .any(|quem_esta| quem_esta.person == quem);
    assert!(
        presente,
        "a sessão velha tirou o visitante dos presentes, e ele está conectado"
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

    daemon.shutdown();
    Ok(())
}
