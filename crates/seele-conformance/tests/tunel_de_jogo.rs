//! **A prova do túnel de jogo** (protocolo 12): a máquina e quem joga são
//! `seele_core::enlace::Enlace` de verdade, o servidor é o de verdade, e o
//! «jogo» é um eco TCP e um eco UDP em `127.0.0.1`.
//!
//! O que cada teste prova:
//!
//! - `o_eco_vai_direto_e_o_anfitriao_nao_repassa`: pelo caminho direto o eco
//!   chega, e o contador de repasse do anfitrião fica em **zero** nos dois
//!   túneis. É a prova negativa: um servidor que repassasse sem dizer faria o
//!   eco passar do mesmo jeito, e só o contador distingue.
//! - `sem_malha_o_eco_vai_pela_estrela`: o espelho. Com a malha desligada o
//!   mesmo eco passa, o caminho é a estrela e o contador **sobe**.
//! - `dois_tuneis_ao_mesmo_tempo_nao_se_misturam`.
//! - `quem_sai_do_servidor_fecha_a_porta` e
//!   `a_maquina_que_para_derruba_o_tunel_com_o_motivo`: o fim de um túnel não
//!   é silencioso, nem para quem joga nem para o jogo.
//!
//! Toda máquina daqui serve **depois do aceite** (`maquina_de_jogo`), pelo
//! caminho do produto: a metade de servidor pede, o núcleo mostra, a máquina
//! aceita a oferta que traz exatamente os túneis que ela serve.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::tunel::{Caminho, Queda, RelatorioDoTunel};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::tunel::{ProtocoloDeTunel, ACAO_ENTRAR_NO_JOGO};
use seele_server::Daemon;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

mod maquina_de_jogo;
use maquina_de_jogo::{aceitar_como_maquina, MOD, SERVIDOR_DE_JOGO};

/// Quanto se espera por qualquer coisa antes de dar o teste por falho. Larga
/// porque o ping do canal de controle é de 2 s e uma máquina carregada é lenta;
/// um teste que falha por relógio curto ensina a ignorá-lo.
const PACIENCIA: Duration = Duration::from_secs(30);

// ------------------------------------------------------------ o servidor

use maquina_de_jogo::Servidor;

/// Um servidor com o MOD `x/jogo` ligado, com a metade de servidor que pede a
/// máquina (ver `maquina_de_jogo::servidor`).
async fn servidor(malha_para_tuneis: bool) -> Result<Servidor> {
    maquina_de_jogo::servidor(malha_para_tuneis, None).await
}

/// Como [`servidor`], com o dono declarado por impressão (para um teste que
/// mexe nos cargos pelo fio).
async fn servidor_com_dono(
    malha_para_tuneis: bool,
    proprietario: Option<String>,
) -> Result<Servidor> {
    maquina_de_jogo::servidor(malha_para_tuneis, proprietario).await
}

// ------------------------------------------------------------ os clientes

fn destino(endereco: SocketAddr, apelido: &str, conjunto: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        esperada: None,
        o_pino_prova_o_servidor: true,
        // O sim aos MODs deste servidor, como a portaria o grava.
        aceito: Some(conjunto.to_owned()),
    }
}

/// Um `Enlace` de verdade, com os relatórios de túnel que já viu.
struct Lado {
    enlace: Enlace,
    /// Todo relatório recebido, em ordem. Guardados todos, e não só o último:
    /// o de «tem vazão» pode vir antes do de «tem latência».
    relatorios: Vec<RelatorioDoTunel>,
}

impl Lado {
    async fn novo(
        endereco: SocketAddr,
        conjunto: &str,
        semente: u8,
        apelido: &str,
    ) -> Result<Self> {
        let pins = Arc::new(MemoryPinStore::default());
        let enlace = Enlace::conectar_entre(
            vec![destino(endereco, apelido, conjunto)],
            SigningKey::from_bytes(&[semente; 32]),
            Arc::clone(&pins) as Arc<dyn PinStore>,
        )
        .await?;
        Ok(Self {
            enlace,
            relatorios: Vec::new(),
        })
    }

    /// Lê avisos até `serve` aprovar um relatório do túnel `nome` — dos que já
    /// chegaram ou dos que vierem — ou até a paciência acabar.
    async fn relatorio(
        &mut self,
        o_que: &str,
        nome: &str,
        serve: impl Fn(&RelatorioDoTunel) -> bool,
    ) -> Result<RelatorioDoTunel> {
        if let Some(r) = self.relatorios.iter().find(|r| r.nome == nome && serve(r)) {
            return Ok(r.clone());
        }
        let fim = Instant::now() + PACIENCIA;
        while Instant::now() < fim {
            let Ok(aviso) =
                tokio::time::timeout(Duration::from_millis(500), self.enlace.proximo()).await
            else {
                continue;
            };
            if let Aviso::Tunel(r) = aviso {
                let achou = r.nome == nome && serve(&r);
                self.relatorios.push(r.clone());
                if achou {
                    return Ok(r);
                }
            }
        }
        anyhow::bail!(
            "a paciência acabou esperando: {o_que}; relatórios vistos: {:#?}",
            self.relatorios
                .iter()
                .filter(|r| r.nome == nome)
                .collect::<Vec<_>>()
        )
    }
}

/// Espera o servidor ter a identidade das duas pontas. **Sem isto o teste
/// disputa uma corrida**: o `Enlace` declara a identidade de par numa tarefa,
/// e um `EntrarNoTunel` que chegasse antes dela iria pela estrela sem que nada
/// estivesse errado — o teste do caminho direto reprovaria de vez em quando, e
/// o da estrela passaria por acaso.
async fn esperar_as_identidades(daemon: &Daemon, a: &Lado, b: &Lado) -> Result<()> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        let pronto = {
            let pares = daemon.server().pares.lock().await;
            pares
                .apresentar(a.enlace.sessao().person, b.enlace.sessao().person)
                .is_some()
        };
        if pronto {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    anyhow::bail!("o servidor não ficou sabendo da identidade das duas pontas")
}

// ------------------------------------------------------------ o «jogo»

/// Um eco TCP em `127.0.0.1`. `prefixo` é posto na frente de **cada** leitura
/// devolvida (vazio, o eco é fiel), e `acabou` recebe um aviso cada vez que
/// uma conexão termina do lado do jogo.
async fn eco_tcp(prefixo: &'static str) -> (u16, tokio::sync::mpsc::UnboundedReceiver<()>) {
    let escuta = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = escuta.local_addr().unwrap().port();
    let (acabou, recebe) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let Ok((mut fluxo, _)) = escuta.accept().await else {
                return;
            };
            let acabou = acabou.clone();
            tokio::spawn(async move {
                let mut buf = vec![0_u8; 16 * 1024];
                loop {
                    match fluxo.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut saida = Vec::with_capacity(prefixo.len() + n);
                            saida.extend_from_slice(prefixo.as_bytes());
                            saida.extend_from_slice(&buf[..n]);
                            if fluxo.write_all(&saida).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                let _ = acabou.send(());
            });
        }
    });
    (porta, recebe)
}

/// Um eco UDP em `127.0.0.1`: devolve cada datagrama a quem o mandou.
async fn eco_udp() -> u16 {
    let soquete = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let porta = soquete.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = vec![0_u8; 64 * 1024];
        while let Ok((n, de)) = soquete.recv_from(&mut buf).await {
            let _ = soquete.send_to(&buf[..n], de).await;
        }
    });
    porta
}

/// Manda `corpo` por `fluxo` e devolve o que voltou, com o mesmo tamanho.
/// Escreve e lê ao mesmo tempo: um eco de 256 KiB lido só depois de escrito
/// tudo enche os buffers dos dois lados e trava.
async fn ida_e_volta(mut fluxo: TcpStream, corpo: Vec<u8>, a_mais: usize) -> Vec<u8> {
    let esperado = corpo.len() + a_mais;
    let (mut le, mut escreve) = fluxo.split();
    let escrita = async {
        escreve.write_all(&corpo).await.unwrap();
    };
    let leitura = async {
        let mut volta = vec![0_u8; esperado];
        le.read_exact(&mut volta).await.unwrap();
        volta
    };
    let (_, volta) = tokio::join!(escrita, leitura);
    volta
}

/// Um padrão que denuncia byte fora do lugar: nenhum par de bytes seguidos se
/// repete a cada 251.
fn padrao(n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| u8::try_from((i * 7 + i / 251) % 251).unwrap())
        .collect()
}

/// 50 datagramas de 1000 B pela porta `porta`, devolvidos pelo eco. Volta
/// quantos voltaram **iguais** (e nenhum pode voltar diferente).
async fn rajada_udp(porta: u16) -> usize {
    let soquete = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    soquete.connect(("127.0.0.1", porta)).await.unwrap();
    let mandar = async {
        for seq in 0_u16..50 {
            let mut datagrama = padrao(1000);
            datagrama[..2].copy_from_slice(&seq.to_be_bytes());
            soquete.send(&datagrama).await.unwrap();
            // Rajada, mas não um despejo: 50 KB de uma vez enchem o
            // buffer do soquete do eco antes de ele ler o primeiro.
            tokio::time::sleep(Duration::from_millis(3)).await;
        }
    };
    let receber = async {
        let mut vistos = std::collections::BTreeSet::new();
        let mut buf = vec![0_u8; 2048];
        // Até 3 s de silêncio: o último datagrama já saiu há tempo.
        while let Ok(Ok(n)) =
            tokio::time::timeout(Duration::from_secs(3), soquete.recv(&mut buf)).await
        {
            let seq = u16::from_be_bytes([buf[0], buf[1]]);
            let mut esperado = padrao(1000);
            esperado[..2].copy_from_slice(&seq.to_be_bytes());
            assert_eq!(
                &buf[..n],
                &esperado[..],
                "o datagrama {seq} voltou diferente"
            );
            vistos.insert(seq);
        }
        vistos.len()
    };
    let (_, voltaram) = tokio::join!(mandar, receber);
    voltaram
}

/// A máquina serve dois túneis e quem joga entra nos dois. Devolve tudo o que
/// os testes de eco precisam.
struct Cena {
    daemon: Arc<Daemon>,
    _maquina: Lado,
    joga: Lado,
    /// Porta local do túnel TCP, na máquina de quem joga.
    porta_tcp: u16,
    /// Porta local do túnel UDP.
    porta_udp: u16,
    _temp: tempfile::TempDir,
}

async fn cena_de_eco(malha: bool) -> Result<Cena> {
    let (endereco, daemon, conjunto, temp) = servidor(malha).await?;
    let mut maquina = Lado::novo(endereco, &conjunto, 1, "maquina").await?;
    let joga = Lado::novo(endereco, &conjunto, 2, "joga").await?;
    esperar_as_identidades(&daemon, &maquina, &joga).await?;

    let (jogo_tcp, _) = eco_tcp("").await;
    let jogo_udp = eco_udp().await;
    aceitar_como_maquina(
        &daemon,
        &mut maquina.enlace,
        MOD,
        SERVIDOR_DE_JOGO,
        &[
            ("eco-tcp", ProtocoloDeTunel::Tcp, jogo_tcp),
            ("eco-udp", ProtocoloDeTunel::Udp, jogo_udp),
        ],
    )
    .await?;
    maquina
        .enlace
        .servir_tunel(MOD, "eco-tcp", ProtocoloDeTunel::Tcp, jogo_tcp)
        .await
        .map_err(|r| anyhow::anyhow!("servir eco-tcp: {r:?}"))?;
    maquina
        .enlace
        .servir_tunel(MOD, "eco-udp", ProtocoloDeTunel::Udp, jogo_udp)
        .await
        .map_err(|r| anyhow::anyhow!("servir eco-udp: {r:?}"))?;

    let tcp = joga
        .enlace
        .entrar_no_tunel(MOD, "eco-tcp")
        .await
        .map_err(|r| anyhow::anyhow!("entrar eco-tcp: {r:?}"))?;
    let udp = joga
        .enlace
        .entrar_no_tunel(MOD, "eco-udp")
        .await
        .map_err(|r| anyhow::anyhow!("entrar eco-udp: {r:?}"))?;
    Ok(Cena {
        daemon,
        _maquina: maquina,
        joga,
        porta_tcp: tcp.porta,
        porta_udp: udp.porta,
        _temp: temp,
    })
}

/// O eco dos dois túneis: 256 KiB pelo TCP e 50 datagramas pelo UDP.
async fn ecoar(cena: &Cena) {
    let corpo = padrao(256 * 1024);
    let fluxo = TcpStream::connect(("127.0.0.1", cena.porta_tcp))
        .await
        .unwrap();
    let volta = tokio::time::timeout(PACIENCIA, ida_e_volta(fluxo, corpo.clone(), 0))
        .await
        .expect("o eco TCP de 256 KiB não voltou a tempo");
    assert!(volta == corpo, "o eco TCP voltou diferente do que foi");
}

// ------------------------------------------------------------ os testes

#[tokio::test(flavor = "multi_thread")]
async fn o_eco_vai_direto_e_o_anfitriao_nao_repassa() -> Result<()> {
    let mut cena = cena_de_eco(true).await?;

    // **Espera o direto ANTES de qualquer eco.** Um fluxo UDP que começasse
    // pela estrela ficaria nela enquanto ativo, e o contador mediria o que
    // aconteceu antes do direto, e não o direto.
    for nome in ["eco-tcp", "eco-udp"] {
        cena.joga
            .relatorio("o caminho direto", nome, |r| {
                matches!(r.caminho, Caminho::Direto { .. })
            })
            .await?;
    }

    ecoar(&cena).await;
    let voltaram = rajada_udp(cena.porta_udp).await;
    assert!(
        voltaram >= 45,
        "só {voltaram} dos 50 datagramas voltaram pelo direto"
    );

    // O relatório de cada túnel: latência de verdade e vazão medida. (A
    // vazão da janela pode ter zerado depois do eco: basta algum relatório
    // que a tenha visto.)
    let tcp = cena
        .joga
        .relatorio("latência e vazão do TCP", "eco-tcp", |r| {
            r.latencia.is_some() && r.bytes_por_segundo_ida > 0
        })
        .await?;
    assert!(matches!(tcp.caminho, Caminho::Direto { .. }), "{tcp:?}");
    let udp = cena
        .joga
        .relatorio("latência e vazão do UDP", "eco-udp", |r| {
            r.latencia.is_some() && r.bytes_por_segundo_ida > 0
        })
        .await?;
    assert!(matches!(udp.caminho, Caminho::Direto { .. }), "{udp:?}");

    // O que sobrou do UDP está contado, e não some em silêncio.
    if voltaram < 50 {
        let ultimo = cena
            .joga
            .relatorios
            .iter()
            .rev()
            .find(|r| r.nome == "eco-udp")
            .unwrap();
        assert!(
            ultimo.udp_descartados as usize >= 50 - voltaram,
            "{} datagramas se perderam e o relatório contou {}",
            50 - voltaram,
            ultimo.udp_descartados
        );
    }

    // **A afirmação do teste:** o anfitrião não repassou um byte.
    for nome in ["eco-tcp", "eco-udp"] {
        assert_eq!(
            cena.daemon.server().contador_de_repasse(MOD, nome),
            0,
            "o anfitrião repassou o túnel {nome} apesar do caminho direto"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_malha_o_eco_vai_pela_estrela() -> Result<()> {
    let mut cena = cena_de_eco(false).await?;

    ecoar(&cena).await;
    let voltaram = rajada_udp(cena.porta_udp).await;
    assert!(
        voltaram >= 45,
        "só {voltaram} dos 50 datagramas voltaram pela estrela"
    );

    for nome in ["eco-tcp", "eco-udp"] {
        let r = cena
            .joga
            .relatorio("o relatório do túnel", nome, |r| {
                r.bytes_por_segundo_ida > 0
            })
            .await?;
        assert_eq!(r.caminho, Caminho::Estrela, "{nome}: {r:?}");
        assert!(
            cena.daemon.server().contador_de_repasse(MOD, nome) > 0,
            "o túnel {nome} andou pela estrela e o anfitrião não contou repasse"
        );
    }
    // E a malha desligada não deixou nenhum relatório dizer «direto».
    assert!(
        cena.joga
            .relatorios
            .iter()
            .all(|r| r.caminho == Caminho::Estrela),
        "{:#?}",
        cena.joga.relatorios
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn dois_tuneis_ao_mesmo_tempo_nao_se_misturam() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true).await?;
    let mut maquina = Lado::novo(endereco, &conjunto, 1, "maquina").await?;
    let joga = Lado::novo(endereco, &conjunto, 2, "joga").await?;
    esperar_as_identidades(&daemon, &maquina, &joga).await?;

    let (jogo_a, _) = eco_tcp("a:").await;
    let (jogo_b, _) = eco_tcp("b:").await;
    aceitar_como_maquina(
        &daemon,
        &mut maquina.enlace,
        MOD,
        SERVIDOR_DE_JOGO,
        &[
            ("eco-a", ProtocoloDeTunel::Tcp, jogo_a),
            ("eco-b", ProtocoloDeTunel::Tcp, jogo_b),
        ],
    )
    .await?;
    maquina
        .enlace
        .servir_tunel(MOD, "eco-a", ProtocoloDeTunel::Tcp, jogo_a)
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    maquina
        .enlace
        .servir_tunel(MOD, "eco-b", ProtocoloDeTunel::Tcp, jogo_b)
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    let a = joga
        .enlace
        .entrar_no_tunel(MOD, "eco-a")
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    let b = joga
        .enlace
        .entrar_no_tunel(MOD, "eco-b")
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    assert_ne!(a.porta, b.porta, "dois túneis na mesma porta local");

    // Os dois ao mesmo tempo, várias vezes, para a mistura ter onde aparecer.
    let um = async {
        let mut f = TcpStream::connect(("127.0.0.1", a.porta)).await.unwrap();
        for i in 0..20_u8 {
            f.write_all(&[i; 100]).await.unwrap();
            let mut volta = vec![0_u8; 102];
            f.read_exact(&mut volta).await.unwrap();
            assert_eq!(
                &volta[..2],
                b"a:",
                "a porta de «a» recebeu {:?}",
                &volta[..2]
            );
            assert!(volta[2..].iter().all(|b| *b == i));
        }
    };
    let outro = async {
        let mut f = TcpStream::connect(("127.0.0.1", b.porta)).await.unwrap();
        for i in 0..20_u8 {
            f.write_all(&[i; 100]).await.unwrap();
            let mut volta = vec![0_u8; 102];
            f.read_exact(&mut volta).await.unwrap();
            assert_eq!(
                &volta[..2],
                b"b:",
                "a porta de «b» recebeu {:?}",
                &volta[..2]
            );
            assert!(volta[2..].iter().all(|b| *b == i));
        }
    };
    tokio::time::timeout(PACIENCIA, async { tokio::join!(um, outro) })
        .await
        .expect("os dois ecos não terminaram a tempo");
    Ok(())
}

/// Se uma conexão com a porta falha (ou é fechada logo) dentro da paciência.
async fn a_porta_fecha(porta: u16) -> Result<()> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        if TcpStream::connect(("127.0.0.1", porta)).await.is_err() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("a porta {porta} continuou aceitando conexões")
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_sai_do_servidor_fecha_a_porta() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true).await?;
    let mut maquina = Lado::novo(endereco, &conjunto, 1, "maquina").await?;
    let joga = Lado::novo(endereco, &conjunto, 2, "joga").await?;
    esperar_as_identidades(&daemon, &maquina, &joga).await?;

    let (jogo, mut acabou) = eco_tcp("").await;
    aceitar_como_maquina(
        &daemon,
        &mut maquina.enlace,
        MOD,
        SERVIDOR_DE_JOGO,
        &[("eco-tcp", ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
    maquina
        .enlace
        .servir_tunel(MOD, "eco-tcp", ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    let porta = joga
        .enlace
        .entrar_no_tunel(MOD, "eco-tcp")
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?
        .porta;

    // Uma conexão viva, com o eco provando que o túnel anda.
    let mut aberta = TcpStream::connect(("127.0.0.1", porta)).await?;
    aberta.write_all(b"ola").await?;
    let mut volta = [0_u8; 3];
    tokio::time::timeout(PACIENCIA, aberta.read_exact(&mut volta)).await??;
    assert_eq!(&volta, b"ola");
    assert!(
        acabou.try_recv().is_err(),
        "o jogo já viu uma conexão acabar"
    );

    // Quem joga sai do servidor: o `Enlace` acaba.
    drop(joga);

    // A porta local fecha...
    a_porta_fecha(porta).await?;
    // ...a conexão que estava aberta acaba para o jogo, do lado da máquina...
    tokio::time::timeout(PACIENCIA, acabou.recv())
        .await
        .map_err(|_| anyhow::anyhow!("o fluxo da máquina ao jogo não acabou"))?
        .expect("o eco morreu");
    // ...e a conexão do cliente do jogo também acaba.
    let mut resto = Vec::new();
    let fim = tokio::time::timeout(PACIENCIA, aberta.read_to_end(&mut resto)).await;
    assert!(
        matches!(fim, Ok(Ok(_)) | Ok(Err(_))),
        "a conexão local de quem joga não acabou"
    );
    Ok(())
}

/// **I1, de ponta a ponta.** O dono revoga `entrar-no-jogo` com o túnel aberto
/// **pelo caminho direto**: o servidor, que não vê o que passa por ele, avisa a
/// máquina (`TunelRetirado`), e é a máquina que acaba com a conexão viva (o
/// motivo `SemPermissao` fica nas quedas dela, e não `CaminhoCaiu`, que seria
/// a conta de quem joga fechando a ligação por conta própria) e derruba a
/// ligação direta, que não tem mais nada permitido a essa pessoa.
///
/// O corte de um canal **novo** numa ligação que o cliente não fecha (um
/// cliente modificado) não cabe aqui, porque o cliente de verdade fecha a
/// ligação sozinho ao saber do `TunelEncerrado`: esse caso está provado na
/// máquina, em `seele-core::tunel`
/// (`a_retirada_corta_o_canal_direto_vivo_o_novo_e_derruba_a_ligacao_vazia`).
#[tokio::test(flavor = "multi_thread")]
async fn revogar_entrar_no_jogo_corta_o_direto_na_maquina() -> Result<()> {
    use seele_proto::cargos::{ChaveDeMod, PedidoDeCargo, Permissao, TODOS};

    let semente_do_dono = 9_u8;
    let impressao_do_dono = seele_proto::transport::key_fingerprint(
        &SigningKey::from_bytes(&[semente_do_dono; 32])
            .verifying_key()
            .to_bytes(),
    );
    let (endereco, daemon, conjunto, _temp) =
        servidor_com_dono(true, Some(impressao_do_dono)).await?;
    let mut maquina = Lado::novo(endereco, &conjunto, 1, "maquina").await?;
    let mut joga = Lado::novo(endereco, &conjunto, 2, "joga").await?;
    let dono = Lado::novo(endereco, &conjunto, semente_do_dono, "dono").await?;
    esperar_as_identidades(&daemon, &maquina, &joga).await?;

    let (jogo, mut acabou) = eco_tcp("").await;
    aceitar_como_maquina(
        &daemon,
        &mut maquina.enlace,
        MOD,
        SERVIDOR_DE_JOGO,
        &[("eco-tcp", ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
    maquina
        .enlace
        .servir_tunel(MOD, "eco-tcp", ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    let porta = joga
        .enlace
        .entrar_no_tunel(MOD, "eco-tcp")
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?
        .porta;
    joga.relatorio("o caminho direto", "eco-tcp", |r| {
        matches!(r.caminho, Caminho::Direto { .. })
    })
    .await?;

    // Uma conexão viva pelo direto, com o eco provado.
    let mut viva = TcpStream::connect(("127.0.0.1", porta)).await?;
    viva.write_all(b"ola").await?;
    let mut tres = [0_u8; 3];
    tokio::time::timeout(PACIENCIA, viva.read_exact(&mut tres)).await??;
    assert_eq!(&tres, b"ola");
    assert_eq!(maquina.enlace.ligacoes_diretas_da_maquina(), 1);

    // O dono tira `entrar-no-jogo` do @everyone, pelo fio.
    let quadro = {
        let db = daemon.server().persistence.lock().await;
        seele_server::cargos::quadro(&db)?
    };
    let todos = quadro
        .cargos
        .iter()
        .find(|c| c.id == TODOS)
        .expect("o @everyone")
        .clone();
    let mut restringe = todos.restringe.clone();
    restringe.push(Permissao::DeMod(ChaveDeMod {
        mod_id: MOD.into(),
        acao: ACAO_ENTRAR_NO_JOGO.into(),
    }));
    dono.enlace
        .cargo(Some((
            quadro.revisao,
            PedidoDeCargo::Editar {
                cargo: TODOS,
                nome: todos.nome.clone(),
                concede: todos.concede.clone(),
                restringe,
            },
        )))
        .await
        .map_err(|_| anyhow::anyhow!("o enlace do dono já tinha acabado"))?;

    // Quem joga fica sabendo, com o nome do motivo...
    let fim = joga
        .relatorio("o aviso do fim do túnel", "eco-tcp", |r| {
            r.encerrado.is_some()
        })
        .await?;
    assert_eq!(fim.encerrado, Some(Queda::SemPermissao), "{fim:?}");
    // ...a conexão do jogo acaba dos dois lados...
    let mut sobra = [0_u8; 16];
    let lida = tokio::time::timeout(PACIENCIA, viva.read(&mut sobra)).await?;
    assert!(matches!(lida, Ok(0) | Err(_)), "a conexão seguiu viva");
    tokio::time::timeout(PACIENCIA, acabou.recv())
        .await?
        .expect("o jogo não viu a conexão acabar");
    // ...e a máquina **foi quem a cortou**, pelo aviso do servidor.
    let fim_ate = Instant::now() + PACIENCIA;
    loop {
        let quedas = maquina.enlace.quedas_do_tunel_servido(MOD, "eco-tcp");
        if quedas
            .iter()
            .any(|(q, n)| *q == Queda::SemPermissao && *n >= 1)
        {
            break;
        }
        assert!(
            Instant::now() < fim_ate,
            "a máquina não cortou a conexão por falta de permissão: {quedas:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // A ligação direta dessa pessoa não tem mais nada permitido: cai.
    let fim_ate = Instant::now() + PACIENCIA;
    while maquina.enlace.ligacoes_diretas_da_maquina() != 0 {
        assert!(
            Instant::now() < fim_ate,
            "a máquina seguiu com a ligação direta de quem perdeu o túnel"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_maquina_que_para_derruba_o_tunel_com_o_motivo() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true).await?;
    let mut maquina = Lado::novo(endereco, &conjunto, 1, "maquina").await?;
    let mut joga = Lado::novo(endereco, &conjunto, 2, "joga").await?;
    esperar_as_identidades(&daemon, &maquina, &joga).await?;

    let (jogo, _) = eco_tcp("").await;
    aceitar_como_maquina(
        &daemon,
        &mut maquina.enlace,
        MOD,
        SERVIDOR_DE_JOGO,
        &[("eco-tcp", ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
    maquina
        .enlace
        .servir_tunel(MOD, "eco-tcp", ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?;
    let porta = joga
        .enlace
        .entrar_no_tunel(MOD, "eco-tcp")
        .await
        .map_err(|r| anyhow::anyhow!("{r:?}"))?
        .porta;
    // O túnel está de pé: a porta aceita.
    drop(TcpStream::connect(("127.0.0.1", porta)).await?);

    maquina
        .enlace
        .parar_tunel(MOD, "eco-tcp")
        .await
        .map_err(|_| anyhow::anyhow!("o enlace da máquina já tinha acabado"))?;

    let fim = joga
        .relatorio("o aviso do fim do túnel", "eco-tcp", |r| {
            r.encerrado.is_some()
        })
        .await?;
    assert_eq!(fim.encerrado, Some(Queda::MaquinaParou), "{fim:?}");
    a_porta_fecha(porta).await?;
    Ok(())
}
