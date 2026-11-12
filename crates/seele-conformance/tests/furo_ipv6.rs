//! **O furo IPv6 de ponta a ponta, com as peças de verdade**: duas casas atrás
//! de CGNAT no IPv4, cada uma com IPv6 global atrás de um firewall que só deixa
//! entrar o que responde a algo que saiu.
//!
//! # O que estava faltando
//!
//! O `CHAMA` de quem liga ia sem candidato, e o ponto junta ao `CHAMADO` só o
//! endereço que ele viu, numa família: a do caminho até ele, IPv4. O anfitrião
//! furava esse IPv4 e mais nada. Com os dois atrás de CGNAT o IPv4 não abre, e
//! o IPv6 dos dois, que não tem NAT nenhum, ficava fechado pelo firewall do
//! roteador do anfitrião, porque ninguém de dentro tinha mandado nada para quem
//! chamava (pendência 26, medido em 24/08/2026). Agora, quando o anfitrião
//! atende e o IPv4 não abre, quem liga **chama de novo**, dizendo o IPv6 global
//! dele (`seele_core::encontro::Chamada::chamar_pelo_ipv6`); o anfitrião fura
//! também por ele, e a discagem ao IPv6 do anfitrião, que o convite já trazia,
//! passa.
//!
//! # O que é de verdade aqui, e o que é de mentira
//!
//! De verdade: o `seele-encontro` (o `Ponto`, com o ENC/3 inteiro), o `seeled`
//! em `[::]` de pilha dupla registrado nele pelo socket do servidor, e quem
//! liga pelo caminho do produto (`seele_ffi::Connection`, o quarto, a corrida,
//! o `CHAMA`, a discagem, o aperto do protocolo).
//!
//! De mentira, porque a máquina é uma só:
//!
//! - **O CGNAT** ([`Cgnat`]): um repasse IPv4 por onde cada lado fala com o
//!   ponto. O ponto vê a porta de fora do repasse, e essa porta só deixa entrar
//!   o ponto: é o NAT que não abre para mais ninguém. O IPv4 que o `AQUI` e o
//!   convite dizem é essa porta, e quem disca para ela não ouve nada.
//! - **O firewall IPv6 dos dois roteadores** ([`FirewallV6`]): um repasse em
//!   `[::1]` com duas faces, uma por lado, que só deixa um pacote entrar numa
//!   casa depois de sair um dela para aquele endereço. Sem tradução: cada lado
//!   vê o outro sempre no mesmo endereço.
//! - **O IPv6 «global» de quem liga**: a face do firewall que o anfitrião vê.
//!   `[::1]` nunca é global, e é o global que o produto anuncia; por isso o
//!   candidato da segunda chamada é dito pela costura de teste
//!   (`seele_core::encontro::costura_de_teste`). **O que não se prova aqui é a
//!   escolha do endereço** (a origem que o sistema usa, o filtro de global):
//!   isso está nos testes do `seele-core`, com os endereços de uma máquina de
//!   verdade encenados.
//!
//! **Nenhum roteador de verdade, nenhum CGNAT de verdade, nenhuma rede de fora.**

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use seele_ffi::{ConnectConfig, Connection};
use seele_proto::encontro::Vizinhanca;
use seele_server::alcance::encontro::{abrir, Convocacao, Encontro, PeloServidor};
use seele_server::alcance::Escuta;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};
use tokio::net::UdpSocket;

mod vaga;

/// Um ponto de encontro de verdade em `127.0.0.1`, numa linha de execução
/// própria. Só IPv4, como o caminho por onde os dois lados falam com o ponto de
/// produção quando o anfitrião se registrou por IPv4.
fn ponto_de_verdade() -> SocketAddr {
    let ponto = seele_encontro::Ponto::abrir(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        Vizinhanca::TambemAqui,
    )
    .expect("o ponto");
    let endereco = ponto.endereco().expect("o endereço do ponto");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    endereco
}

/// Um CGNAT de mentira entre uma casa e o ponto de encontro.
///
/// Quem está dentro fala com [`Cgnat::dentro`] como se fosse o ponto. Cada
/// socket de dentro ganha uma porta de fora própria, que é o que o ponto vê, e
/// essa porta **só deixa entrar o ponto**: o mapeamento não serve a mais
/// ninguém, que é o que faz o furo IPv4 não abrir entre duas casas assim.
struct Cgnat {
    dentro: SocketAddr,
    /// Datagramas que bateram numa porta de fora sem vir do ponto.
    recusados: Arc<AtomicUsize>,
    tarefa: tokio::task::JoinHandle<()>,
}

impl Cgnat {
    async fn diante_de(ponto: SocketAddr) -> Self {
        let dentro = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let onde = dentro.local_addr().unwrap();
        let recusados = Arc::new(AtomicUsize::new(0));
        let contador = Arc::clone(&recusados);
        let tarefa = tokio::spawn(async move {
            let mut de_fora: HashMap<SocketAddr, Arc<UdpSocket>> = HashMap::new();
            let mut balde = vec![0_u8; 2048];
            while let Ok((lidos, de)) = dentro.recv_from(&mut balde).await {
                let fora = match de_fora.get(&de) {
                    Some(fora) => Arc::clone(fora),
                    None => {
                        let fora = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
                        de_fora.insert(de, Arc::clone(&fora));
                        // A volta: só o ponto entra por esta porta.
                        let (volta, dentro, contador) = (
                            Arc::clone(&fora),
                            Arc::clone(&dentro),
                            Arc::clone(&contador),
                        );
                        tokio::spawn(async move {
                            let mut balde = vec![0_u8; 2048];
                            while let Ok((lidos, quem)) = volta.recv_from(&mut balde).await {
                                if quem == ponto {
                                    let _ = dentro.send_to(&balde[..lidos], de).await;
                                } else {
                                    contador.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        });
                        fora
                    }
                };
                let _ = fora.send_to(&balde[..lidos], ponto).await;
            }
        });
        Self {
            dentro: onde,
            recusados,
            tarefa,
        }
    }
}

impl Drop for Cgnat {
    fn drop(&mut self) {
        self.tarefa.abort();
    }
}

/// O que o firewall de mentira contou.
#[derive(Debug, Default)]
struct ContasDoFirewall {
    /// De quem liga para o anfitrião, barrados: o anfitrião ainda não tinha
    /// mandado nada para o endereço de quem liga.
    barrados_no_anfitriao: AtomicUsize,
    /// De quem liga para o anfitrião, que entraram.
    entraram_no_anfitriao: AtomicUsize,
    /// Do anfitrião para o endereço de quem liga: o furo, e depois a conversa.
    sairam_do_anfitriao: AtomicUsize,
    /// Desses, os que entraram na casa de quem liga.
    entraram_em_quem_liga: AtomicUsize,
}

/// O firewall IPv6 dos dois roteadores, numa peça só.
///
/// Duas faces em `[::1]`: [`FirewallV6::o_anfitriao`] é o IPv6 do anfitrião
/// como quem liga o vê (o que vai no convite), e [`FirewallV6::quem_liga`] é o
/// IPv6 de quem liga como o anfitrião o vê (o candidato do `CHAMA`). Um pacote
/// só entra numa casa depois de **sair um dela para aquele endereço**, que é a
/// regra de um firewall com estado, e é a que o padrão de fábrica dos roteadores
/// domésticos aplica à entrada IPv6.
struct FirewallV6 {
    o_anfitriao: SocketAddr,
    quem_liga: SocketAddr,
    contas: Arc<ContasDoFirewall>,
    tarefas: Vec<tokio::task::JoinHandle<()>>,
}

impl FirewallV6 {
    /// `anfitriao` é o endereço IPv6 de verdade do servidor, atrás do firewall.
    async fn entre_os_dois(anfitriao: SocketAddr) -> Self {
        let face_do_anfitriao = Arc::new(UdpSocket::bind("[::1]:0").await.unwrap());
        let face_de_quem_liga = Arc::new(UdpSocket::bind("[::1]:0").await.unwrap());
        let contas = Arc::new(ContasDoFirewall::default());
        // De onde quem liga mandou para o anfitrião: é para lá, e só para lá,
        // que o firewall dele deixa a resposta entrar.
        let de_quem_liga: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));
        // Se o anfitrião já mandou algo para o endereço de quem liga.
        let anfitriao_saiu = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Quem liga → anfitrião.
        let ida = {
            let (entra, sai) = (
                Arc::clone(&face_do_anfitriao),
                Arc::clone(&face_de_quem_liga),
            );
            let (contas, de_quem_liga, anfitriao_saiu) = (
                Arc::clone(&contas),
                Arc::clone(&de_quem_liga),
                Arc::clone(&anfitriao_saiu),
            );
            tokio::spawn(async move {
                let mut balde = vec![0_u8; 2048];
                while let Ok((lidos, de)) = entra.recv_from(&mut balde).await {
                    *de_quem_liga.lock().unwrap() = Some(de);
                    if anfitriao_saiu.load(Ordering::SeqCst) {
                        contas.entraram_no_anfitriao.fetch_add(1, Ordering::Relaxed);
                        let _ = sai.send_to(&balde[..lidos], anfitriao).await;
                    } else {
                        contas.barrados_no_anfitriao.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        };
        // Anfitrião → quem liga.
        let volta = {
            let (entra, sai) = (
                Arc::clone(&face_de_quem_liga),
                Arc::clone(&face_do_anfitriao),
            );
            let contas = Arc::clone(&contas);
            tokio::spawn(async move {
                let mut balde = vec![0_u8; 2048];
                while let Ok((lidos, de)) = entra.recv_from(&mut balde).await {
                    if de != anfitriao {
                        continue;
                    }
                    anfitriao_saiu.store(true, Ordering::SeqCst);
                    contas.sairam_do_anfitriao.fetch_add(1, Ordering::Relaxed);
                    let destino = *de_quem_liga.lock().unwrap();
                    if let Some(destino) = destino {
                        contas.entraram_em_quem_liga.fetch_add(1, Ordering::Relaxed);
                        let _ = sai.send_to(&balde[..lidos], destino).await;
                    }
                }
            })
        };
        Self {
            o_anfitriao: face_do_anfitriao.local_addr().unwrap(),
            quem_liga: face_de_quem_liga.local_addr().unwrap(),
            contas,
            tarefas: vec![ida, volta],
        }
    }
}

impl Drop for FirewallV6 {
    fn drop(&mut self) {
        for tarefa in &self.tarefas {
            tarefa.abort();
        }
    }
}

/// Um `seeled` de verdade em `[::]` de pilha dupla, registrado no ponto
/// **através do CGNAT dele**: o ponto o vê na porta de fora do repasse.
///
/// `porta_das_dicas` é a porta que as dicas de rede local do `ATENDE` levam.
/// Num anfitrião de verdade é a do servidor, e as dicas são os endereços dele
/// na rede de casa, que só alcança quem está nela. Aqui as duas casas são a
/// mesma máquina, e a dica com a porta do servidor seria um atalho por dentro
/// que duas casas não têm: ela vai com uma porta onde ninguém atende.
async fn anfitriao(ponto_pelo_cgnat: SocketAddr, porta_das_dicas: u16) -> (Arc<Daemon>, Encontro) {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await.expect("o seeled"));
    let pelo = PeloServidor {
        ponta: daemon.ponta(),
        enc: daemon.mensagens_enc().expect("a caixa do ENC/3"),
    };
    let escuta = Escuta::nova(porta_das_dicas, daemon.pilha());
    let convocacao = Convocacao::para_servidor(
        pelo,
        daemon.fingerprint(),
        escuta,
        ponto_pelo_cgnat.to_string(),
    )
    .expect("a convocação");
    let encontro = abrir(&convocacao).await.expect("o registro no ponto");
    let rodando = Arc::clone(&daemon);
    tokio::spawn(async move { rodando.run().await });
    (daemon, encontro)
}

/// As duas casas montadas: o ponto, os dois CGNATs, o firewall IPv6 e o
/// anfitrião registrado.
struct DuasCasas {
    daemon: Arc<Daemon>,
    _encontro: Encontro,
    cgnat_do_anfitriao: Cgnat,
    cgnat_de_quem_liga: Cgnat,
    firewall: FirewallV6,
    /// O convite, como quem hospeda o emitiria nessa rede: o IPv4 público que o
    /// ponto viu (a porta de fora do CGNAT), o IPv6 dele, e o bilhete do ponto.
    convite: ConnectConfig,
    _casa: tempfile::TempDir,
    /// A porta das dicas de rede local: aberta, e muda.
    _outra_rede: std::net::UdpSocket,
}

/// `None` numa máquina sem `[::1]` ou sem pilha dupla: não há o que medir.
async fn duas_casas() -> Option<DuasCasas> {
    if std::net::UdpSocket::bind("[::1]:0").is_err() {
        eprintln!("pulado: esta máquina não tem IPv6 nem em loopback");
        return None;
    }
    let ponto = ponto_de_verdade();
    let cgnat_do_anfitriao = Cgnat::diante_de(ponto).await;
    let cgnat_de_quem_liga = Cgnat::diante_de(ponto).await;
    let outra_rede = std::net::UdpSocket::bind("0.0.0.0:0").expect("a porta das dicas");
    let porta_das_dicas = outra_rede.local_addr().unwrap().port();
    let (daemon, encontro) = anfitriao(cgnat_do_anfitriao.dentro, porta_das_dicas).await;
    if daemon.pilha() != seele_server::alcance::Pilha::Dupla {
        eprintln!("pulado: esta máquina não abre socket de pilha dupla");
        daemon.shutdown();
        return None;
    }
    let porta = daemon.local_addr().unwrap().port();
    let firewall = FirewallV6::entre_os_dois(SocketAddr::from((Ipv6Addr::LOCALHOST, porta))).await;

    // O IPv4 público do anfitrião é o que o ponto viu: a porta de fora do CGNAT
    // dele. Na forma mapeada, que a classificação de quem liga lê como pública
    // (o mesmo recurso de `estados.rs` e de `furo.rs`).
    let publico = encontro.publico();
    assert!(
        publico.ip().to_canonical().is_loopback() && publico.port() != porta,
        "o ponto viu o anfitrião direto ({publico}), e não pela porta de fora do CGNAT: o IPv4 \
         abriria, e este teste não mediria o IPv6"
    );
    let casa = tempfile::tempdir().expect("a casa de quem liga");
    let bilhete =
        seele_core::uri::Bilhete::novo(cgnat_de_quem_liga.dentro.to_string(), "45.33.32.156:41234")
            .expect("o bilhete");
    let convite = ConnectConfig {
        server: format!("[::ffff:127.0.0.1]:{}", publico.port()),
        // O IPv6 do anfitrião, como o `alt=` do convite o leva.
        alternate_servers: vec![firewall.o_anfitriao.to_string()],
        nickname: "pessoa".into(),
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: Some(daemon.fingerprint().to_owned()),
        codigo_esperado: None,
        bilhete: Some(bilhete),
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    Some(DuasCasas {
        daemon,
        _encontro: encontro,
        cgnat_do_anfitriao,
        cgnat_de_quem_liga,
        firewall,
        convite,
        _casa: casa,
        _outra_rede: outra_rede,
    })
}

fn contas(casas: &DuasCasas) -> String {
    format!(
        "firewall {:?}; recusados no CGNAT do anfitrião {}, no de quem liga {}",
        casas.firewall.contas,
        casas.cgnat_do_anfitriao.recusados.load(Ordering::Relaxed),
        casas.cgnat_de_quem_liga.recusados.load(Ordering::Relaxed),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn duas_casas_atras_de_cgnat_se_acham_pelo_ipv6_que_a_segunda_chamada_abre() {
    let _vaga = vaga::minha();
    let Some(casas) = duas_casas().await else {
        return;
    };
    // O IPv6 «global» de quem liga é a face do firewall que o anfitrião vê.
    let _ditos = seele_core::encontro::costura_de_teste::anunciar_a_este_ponto(
        casas.cgnat_de_quem_liga.dentro,
        vec![casas.firewall.quem_liga],
    );

    let convite = casas.convite.clone();
    let entrada = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::task::spawn_blocking(move || Connection::connect_with_trail(convite)),
    )
    .await
    .expect("a chegada não terminou")
    .expect("a thread que conecta caiu");
    let (connection, _) = entrada.unwrap_or_else(|falha| {
        panic!(
            "duas casas com IPv6 global não se acharam: o anfitrião não furou o firewall dele \
             para o IPv6 de quem liga. {falha:?}. {}",
            contas(&casas)
        )
    });
    assert_eq!(
        connection.snapshot().server,
        "Casa",
        "entrou, e não no servidor do convite"
    );

    let f = &casas.firewall.contas;
    // «Não chegando antes»: o firewall estava fechado quando a discagem direta
    // ao IPv6 do anfitrião começou. Sem isto, o teste não distinguiria um
    // firewall aberto pelo furo de um que nunca fechou.
    assert!(
        f.barrados_no_anfitriao.load(Ordering::Relaxed) >= 1,
        "nenhum pacote de quem liga foi barrado antes do furo: o firewall de mentira já \
         estava aberto, e este teste não mede nada. {}",
        contas(&casas)
    );
    // E a conversa passou por ele, nos dois sentidos: o aperto do enlace e do
    // protocolo são dezenas de datagramas.
    for (nome, quantos) in [
        ("entraram no anfitrião", &f.entraram_no_anfitriao),
        ("entraram em quem liga", &f.entraram_em_quem_liga),
    ] {
        assert!(
            quantos.load(Ordering::Relaxed) >= 10,
            "só {} datagramas {nome} pelo firewall IPv6: a conversa não foi por ele. {}",
            quantos.load(Ordering::Relaxed),
            contas(&casas)
        );
    }
    // O IPv4 foi tentado e não abriu, que é a casa que este teste encena: o
    // endereço que o `AQUI` e o convite dizem só deixa entrar o ponto.
    assert!(
        casas.cgnat_do_anfitriao.recusados.load(Ordering::Relaxed) >= 1,
        "ninguém discou o IPv4 público do anfitrião: o `AQUI` não chegou a quem liga. {}",
        contas(&casas)
    );
    eprintln!("{}", contas(&casas));
    drop(connection);
    casas.daemon.shutdown();
}

/// **O controle.** As mesmas duas casas, e quem liga sem IPv6 global para
/// anunciar: nenhuma segunda chamada, o anfitrião fura só o IPv4, o firewall
/// IPv6 dele nunca abre, e ninguém entra. É o que o produto fazia.
///
/// Sem este teste, o de cima passaria também com um firewall de mentira que
/// deixasse tudo entrar.
#[tokio::test(flavor = "multi_thread")]
async fn sem_o_ipv6_de_quem_liga_no_chama_o_firewall_do_anfitriao_nao_abre() {
    let _vaga = vaga::minha();
    let Some(casas) = duas_casas().await else {
        return;
    };
    let _ditos = seele_core::encontro::costura_de_teste::anunciar_a_este_ponto(
        casas.cgnat_de_quem_liga.dentro,
        Vec::new(),
    );

    let convite = casas.convite.clone();
    let entrada = tokio::time::timeout(
        Duration::from_secs(120),
        tokio::task::spawn_blocking(move || Connection::connect_with_trail(convite)),
    )
    .await
    .expect("a chegada não terminou")
    .expect("a thread que conecta caiu");
    assert!(
        entrada.is_err(),
        "entrou sem o anfitrião ter furado o IPv6: há um caminho que este teste não fechou. {}",
        contas(&casas)
    );
    let f = &casas.firewall.contas;
    assert_eq!(
        (
            f.entraram_no_anfitriao.load(Ordering::Relaxed),
            f.sairam_do_anfitriao.load(Ordering::Relaxed)
        ),
        (0, 0),
        "o anfitrião mandou algo ao IPv6 de quem liga sem ter sido avisado dele. {}",
        contas(&casas)
    );
    assert!(
        f.barrados_no_anfitriao.load(Ordering::Relaxed) >= 1,
        "quem liga nem tentou o IPv6 do anfitrião. {}",
        contas(&casas)
    );
    eprintln!("{}", contas(&casas));
    casas.daemon.shutdown();
}

/// **Quem entra pelo IPv4 entra como sempre, e não conta o IPv6 a ninguém.**
///
/// Sem CGNAT e sem firewall: o caminho de sempre do degrau 4, em que o `AQUI`
/// diz um IPv4 que abre. Quem liga tem IPv6 global para anunciar (dito pela
/// costura: um socket mudo em `[::1]`, que só conta o que chega), e **não o
/// anuncia**: a segunda chamada só existe depois de o IPv4 dar em silêncio. O
/// anfitrião não recebe candidato IPv6 nenhum, e não manda nada a ele.
///
/// É a metade «não mudou» da prova: o ponto e o anfitrião deste teste são os
/// de antes (o código deles é o da 0.19.6), e o que quem liga manda a eles,
/// quando o IPv4 abre, é o que a 0.19.6 mandava.
///
/// Quem liga entra por `Enlace::conectar_entre_com_bilhete`, sem a consulta ao
/// quarto que a FFI faz antes: em loopback o quarto diria o endereço de verdade
/// do servidor, a discagem direta entraria por ele, e nenhum `CHAMA` sairia.
#[tokio::test(flavor = "multi_thread")]
async fn quem_entra_pelo_ipv4_nao_anuncia_o_ipv6_e_o_anfitriao_nao_o_fura() {
    let _vaga = vaga::minha();
    let Ok(mudo) = UdpSocket::bind("[::1]:0").await else {
        eprintln!("pulado: esta máquina não tem IPv6 nem em loopback");
        return;
    };
    let ponto = ponto_de_verdade();
    // Direto no ponto, e com as dicas de verdade: nada aqui é de mentira além
    // do candidato dito.
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await.expect("o seeled"));
    let pelo = PeloServidor {
        ponta: daemon.ponta(),
        enc: daemon.mensagens_enc().expect("a caixa do ENC/3"),
    };
    let escuta = Escuta::nova(daemon.local_addr().unwrap().port(), daemon.pilha());
    let convocacao =
        Convocacao::para_servidor(pelo, daemon.fingerprint(), escuta, ponto.to_string())
            .expect("a convocação");
    let _encontro = abrir(&convocacao).await.expect("o registro no ponto");
    let rodando = Arc::clone(&daemon);
    tokio::spawn(async move { rodando.run().await });

    let candidato = mudo.local_addr().unwrap();
    let _ditos =
        seele_core::encontro::costura_de_teste::anunciar_a_este_ponto(ponto, vec![candidato]);
    // Conta as Binding Indications (20 B, tipo 0x0011) que o anfitrião manda
    // ao candidato IPv6.
    let furos = Arc::new(AtomicUsize::new(0));
    let contando = {
        let furos = Arc::clone(&furos);
        tokio::spawn(async move {
            let mut balde = [0_u8; 2048];
            while let Ok((lidos, _)) = mudo.recv_from(&mut balde).await {
                if lidos == 20 && balde[..2] == [0x00, 0x11] {
                    furos.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
    };

    // O endereço do convite está morto: quem entra, entra pelo `AQUI`.
    let morto = std::net::UdpSocket::bind("127.0.0.1:0").expect("a porta morta");
    let servidor: SocketAddr = format!("[::ffff:127.0.0.1]:{}", morto.local_addr().unwrap().port())
        .parse()
        .expect("o endereço morto");
    let destino = seele_core::enlace::Destino {
        servidor,
        nome_tls: "localhost".to_owned(),
        chave_do_pin: servidor.to_string(),
        apelido: "pessoa".to_owned(),
        segredo: None,
        esperada: Some(seele_core::enlace::Esperada::Certificado(
            daemon.fingerprint().to_owned(),
        )),
        o_pino_prova_o_servidor: true,
        aceito: None,
    };
    let bilhete =
        seele_core::uri::Bilhete::novo(ponto.to_string(), "45.33.32.156:41234").expect("o bilhete");
    let entrada = tokio::time::timeout(
        Duration::from_secs(90),
        seele_core::enlace::Enlace::conectar_entre_com_bilhete(
            vec![destino],
            Some(bilhete),
            seele_core::SigningKey::from_bytes(&[13; 32]),
            Arc::new(seele_core::MemoryPinStore::default()) as Arc<dyn seele_core::PinStore>,
        ),
    )
    .await
    .expect("a chegada não terminou");
    let connection = entrada.unwrap_or_else(|falha| {
        panic!("quem entrava pelo IPv4 do `AQUI` deixou de entrar: {falha:?}")
    });
    assert!(
        daemon.ponta().contadores().pares_do_chamado >= 1,
        "ninguém chamou o anfitrião pelo ponto: a conexão subiu por outro caminho, e este \
         teste não mediu o degrau 4"
    );

    // O furo são cinco Indications espaçadas por 200 ms: se o candidato
    // tivesse ido, em um segundo e meio alguma teria chegado.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        furos.load(Ordering::Relaxed),
        0,
        "o anfitrião furou o IPv6 de quem liga numa entrada que o IPv4 resolveu: o IPv6 \
         desta máquina foi ao ponto sem precisar"
    );
    contando.abort();
    drop(connection);
    daemon.shutdown();
}
