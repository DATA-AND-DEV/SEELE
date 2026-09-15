//! A máquina de estados da chegada, e as arestas que não existem.
//!
//! Quatro candidatos eram tentados em série atrás de um spinner mudo, e quando
//! o teste de campo das duas casas falhou ninguém soube dizer em que ponto —
//! porque não havia ponto nomeado. O que se afirma aqui é o que uma
//! `seele_core::chegada::Chegada` passou a responder: por onde ela andou, em que
//! ordem, e o que **não** pode acontecer com ela.
//!
//! # Por que aqui, e não só em `seele-core`
//!
//! Porque metade destas propriedades é fiação. Que a máquina de estados recuse
//! uma aresta é uma função pura, e mora ao lado dela; que a trilha atravesse o
//! `seele-ffi` até a casca, e com os nomes que o núcleo deu, só se observa com
//! os dois crates na mesa — e é exatamente o tipo de junção que passa verde em
//! todo teste de unidade enquanto está quebrada.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use seele_core::chegada::{Caminho, Chegada, Etapa};
use seele_core::enlace::Destino;
use seele_core::{MemoryPinStore, PinStore, SigningKey};
use seele_ffi::{ConnectConfig, ConnectStage, Connection, Event, EventListener};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

/// A base de uma configuração de entrada, para os testes que só trocam um campo.
fn config_de_teste(server: String, casa: &std::path::Path) -> ConnectConfig {
    ConnectConfig {
        server,
        alternate_servers: Vec::new(),
        nickname: "pessoa".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // Não há placa de som numa máquina de integração contínua.
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

/// Um endereço onde não há ninguém.
///
/// Aberto e devolvido, para a porta ser real e estar livre: um número escolhido
/// à mão poderia ser o servidor de outra pessoa na máquina que roda os testes. É o
/// mesmo auxiliar de `candidatos.rs`, e falha do jeito que um endereço errado
/// falha em campo.
fn endereco_morto() -> SocketAddr {
    let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") else {
        panic!("esta máquina não deixou abrir um socket em 127.0.0.1");
    };
    let Ok(endereco) = socket.local_addr() else {
        panic!("um socket aberto sem endereço local");
    };
    drop(socket);
    endereco
}

/// Dois candidatos que ninguém atende — um convite cujo servidor não existe.
fn destinos_mortos_de_teste() -> Vec<Destino> {
    (0..2)
        .map(|_| {
            let servidor = endereco_morto();
            Destino {
                servidor,
                nome_tls: "localhost".into(),
                chave_do_pin: servidor.to_string(),
                apelido: "pessoa".into(),
                segredo: None,
                impressao_esperada: None,
            }
        })
        .collect()
}

fn chave_de_teste() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn pins_de_teste() -> Arc<dyn PinStore> {
    Arc::new(MemoryPinStore::default())
}

#[test]
fn avisar_nunca_reprova_uma_chegada() {
    // A aresta que não pode existir. Um ponto de encontro fora do ar, um nome
    // que não resolve, um convite sem impressão digital — nenhum deles pode
    // reprovar uma chegada, porque **nenhum endereço do convite depende dele**.
    //
    // Hoje isso é garantido por um prazo de 600 ms em `encontro.rs`. Aqui vira
    // uma transição que não se pode escrever, que é mais barato de conferir.
    assert!(
        !Etapa::transicao_legal(
            &Etapa::Avisando {
                ponto: String::new()
            },
            "Desistiu"
        ),
        "o degrau 4 é o de cima da escada: perdê-lo não perde os de baixo"
    );
    assert!(Etapa::transicao_legal(
        &Etapa::Avisando {
            ponto: String::new()
        },
        "Tentando"
    ));
}

#[tokio::test]
async fn a_trilha_sobrevive_a_uma_chegada_que_falhou() {
    // «Tentei quatro candidatos, o primeiro deu prazo esgotado em 4 s, o quarto
    // recusou» é o dado que faltou quando o teste das duas casas falhou e
    // ninguém soube dizer por quê.
    //
    // Custa zero em privacidade: todo endereço da trilha já estava no convite de
    // quem a lê.
    let chegada = Chegada::nova(destinos_mortos_de_teste(), None);
    let resultado = chegada.chegar(chave_de_teste(), pins_de_teste()).await;
    assert!(resultado.is_err(), "endereços mortos não conectam");

    // A trilha é lida do erro, e não do objeto: a `Chegada` é de uso único.
    let Err(erro) = resultado else { return };
    let trilha = erro.trilha();
    assert!(
        trilha.len() >= 2,
        "uma chegada que tentou dois candidatos deixa ao menos dois passos"
    );
    assert!(
        trilha
            .iter()
            .any(|passo| matches!(passo.etapa, Etapa::Desistiu(_))),
        "a trilha termina no motivo, e o motivo é o do código"
    );
}

#[tokio::test]
async fn quem_acompanha_uma_chegada_recebe_a_ultima_etapa_dela() {
    // A metade que a tela lê. A trilha é o registro; o `watch` é o que apaga o
    // spinner mudo enquanto a travessia corre, e ele só vale se a publicação
    // estiver na mesma linha que escreve a trilha — um `send` que sumisse
    // deixaria a tela parada em «Parada» com a chegada inteira já acontecida.
    //
    // O `borrow` depois do fim é de propósito: um `watch` guarda o último valor
    // mesmo com o emissor já recolhido, e é assim que isto afirma a transição
    // final sem depender de quem correu primeiro.
    let chegada = Chegada::nova(destinos_mortos_de_teste(), None);
    let mut olhos = chegada.acompanhar();
    assert!(
        matches!(
            &*olhos.borrow_and_update(),
            Etapa::Parada { candidatos: 2, .. }
        ),
        "uma chegada nasce parada, e sabendo quantos endereços tem"
    );

    let resultado = chegada.chegar(chave_de_teste(), pins_de_teste()).await;
    assert!(resultado.is_err(), "endereços mortos não conectam");

    // `has_changed` não serve depois do fim: com o emissor recolhido ele
    // responde erro, e não «não mudou». O que se afirma é o valor, que é o que
    // a tela desenharia.
    assert!(
        matches!(&*olhos.borrow(), Etapa::Desistiu(_)),
        "quem acompanhava ficou sem saber que a chegada acabou: {:?}",
        *olhos.borrow()
    );
}

#[test]
fn cada_etapa_atravessa_o_ffi_com_o_nome_que_o_nucleo_deu() {
    // A casca escreve uma frase por nome. Um nome que se perde entre os dois
    // lados — ou duas etapas que atravessam com o mesmo — é uma tela muda no
    // instante em que ela mais precisa dizer alguma coisa, e o defeito não
    // aparece em lugar nenhum antes de acontecer com alguém.
    //
    // A lista vem de `Etapa::TODAS` e **não é escrita aqui**: uma cópia à mão
    // deixaria uma variante nova atravessar sem que este laço a visse, que é
    // exatamente o buraco que havia — três listas paralelas, nenhuma ligada ao
    // enum. `a_lista_de_etapas_tem_uma_entrada_por_variante_do_enum` é quem
    // guarda a que sobrou.
    let mut vistos: Vec<&'static str> = Vec::new();
    for etapa in &Etapa::TODAS {
        let atravessou = ConnectStage::from(etapa);
        assert_eq!(
            atravessou.nome(),
            etapa.nome(),
            "a etapa {} atravessou o seele-ffi como {}",
            etapa.nome(),
            atravessou.nome()
        );
        assert!(
            !vistos.contains(&atravessou.nome()),
            "duas etapas atravessaram com o mesmo nome `{}`",
            atravessou.nome()
        );
        vistos.push(atravessou.nome());
    }

    // O que a casca recebe é derivado da mesma lista, e é o que o guarda de
    // `frases.js` lê. Se a derivação parar de derivar, a cobertura da tela
    // passa a ser cobrada contra outra coisa.
    let da_casca: Vec<&'static str> = ConnectStage::todas()
        .iter()
        .map(ConnectStage::nome)
        .collect();
    assert_eq!(
        da_casca, vistos,
        "a lista que a casca recebe deixou de ser a lista que o núcleo publica"
    );
}

#[test]
fn a_lista_de_etapas_tem_uma_entrada_por_variante_do_enum() {
    // O buraco que 133 testes verdes não viram: um revisor acrescentou uma
    // variante a `Etapa`, fez **só o que o compilador cobrava**, e ela
    // atravessou o `seele-ffi` até cair no «falha que esta tela não sabe
    // nomear» no meio de uma conexão que ia bem. Havia três listas escritas à
    // mão — a do núcleo, a deste arquivo e a do guarda da casca — e nenhuma
    // delas o compilador ligava ao enum.
    //
    // Restou uma, `Etapa::TODAS`, e este é o guarda dela. O que o compilador
    // cobra de verdade em Rust estável é um `match` sem `_`, e `Etapa::nome`
    // tem um: a lista é lida **dele**, do texto do módulo, em vez de repetida
    // aqui. É o mesmo caminho de `the_alarm_names_the_rungs_the_ladder_actually_reports`,
    // que derruba a lista da escada do `matches!` de `alcanca_de_fora`.
    let caminho =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../seele-core/src/chegada.rs");
    let Ok(modulo) = std::fs::read_to_string(&caminho) else {
        panic!("`chegada.rs` não está em {}", caminho.display());
    };

    let Some(corpo) = modulo
        .split("pub fn nome(&self) -> &'static str {")
        .nth(1)
        .and_then(|resto| resto.split("\n    }").next())
    else {
        panic!(
            "`Etapa::nome` sumiu, e com ela o `match` que o compilador cobra — \
             a partir daqui nada liga a lista de etapas ao enum"
        );
    };

    // Cada braço é `Self::Variante ... => "Nome",`. O nome tem de ser o da
    // variante: sem isto, uma variante renomeada de um lado do `match` só
    // passaria despercebida.
    let mut do_match: Vec<String> = Vec::new();
    for braco in corpo.split("Self::").skip(1) {
        let Some(variante) = braco.split(|c: char| !c.is_alphanumeric()).next() else {
            continue;
        };
        if variante.is_empty() {
            continue;
        }
        assert!(
            braco.contains(&format!("=> \"{variante}\"")),
            "`{variante}` chega à casca com outro nome que não o seu, e a \
             frase da tela é indexada pelo nome"
        );
        do_match.push(variante.to_owned());
    }
    assert!(
        do_match.len() >= 5,
        "só {} braços foram lidos de `Etapa::nome`, então o `match` deixou de \
         ser lido como este teste supõe e o guarda não guarda nada: {do_match:?}",
        do_match.len()
    );

    let na_lista: Vec<String> = Etapa::TODAS
        .iter()
        .map(|etapa| etapa.nome().to_owned())
        .collect();

    let faltando: Vec<&String> = do_match
        .iter()
        .filter(|nome| !na_lista.contains(nome))
        .collect();
    assert!(
        faltando.is_empty(),
        "estas etapas existem no enum e não estão em `Etapa::TODAS`, então \
         atravessam o seele-ffi sem que a cobertura da tela as veja: {faltando:?}"
    );
    let inventadas: Vec<&String> = na_lista
        .iter()
        .filter(|nome| !do_match.contains(nome))
        .collect();
    assert!(
        inventadas.is_empty(),
        "`Etapa::TODAS` tem exemplares de etapas que `Etapa::nome` não conhece: \
         {inventadas:?}"
    );
}

#[test]
fn a_trilha_de_uma_chegada_que_falhou_atravessa_ate_a_casca() {
    // A fiação inteira, do laço de candidatos até o valor que a casca desenha.
    // Testar só o núcleo deixaria isto passar verde com a trilha sendo jogada
    // fora na travessia — que é onde ela era jogada fora antes desta tarefa: o
    // app recebia «não consegui conectar» sobre quatro tentativas das quais
    // nenhuma tinha nome.
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há identidade para conectar");
    };
    let primeiro = endereco_morto();
    let segundo = endereco_morto();

    let falha = match Connection::connect_with_trail(ConnectConfig {
        server: primeiro.to_string(),
        alternate_servers: vec![segundo.to_string()],
        nickname: "pessoa".into(),
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // Não há placa de som numa máquina de integração contínua.
        audio: false,
        capture_device: None,
        playback_device: None,
    }) {
        Ok(_) => panic!("dois endereços mortos deixaram alguém entrar"),
        Err(falha) => falha,
    };

    assert!(
        falha.trail.len() >= 2,
        "a trilha chegou à casca com {} passos: {:?}",
        falha.trail.len(),
        falha.trail
    );
    assert!(
        falha.trail.iter().any(|passo| matches!(
            &passo.etapa,
            ConnectStage::Tentando { onde, .. } if onde == &primeiro.to_string()
        )),
        "a trilha não diz qual endereço foi tentado primeiro: {:?}",
        falha.trail
    );
    assert!(
        matches!(
            falha.trail.last().map(|passo| &passo.etapa),
            Some(ConnectStage::Desistiu { .. })
        ),
        "a trilha que atravessou não termina no motivo: {:?}",
        falha.trail
    );
    // O erro de sempre continua sendo o erro de sempre. Os dois são o mesmo
    // fato — uma porta fechada em loopback ora recusa, ora deixa o aperto de
    // mão vencer o prazo —, e distingui-los aqui seria medir o sistema
    // operativo da máquina que roda o teste.
    assert!(
        matches!(
            falha.error,
            // `SemResposta` desde 05/09/2026: os pacotes saem e nada volta, que
            // é o que de fato acontece contra uma porta onde ninguém escuta.
            // Ver `an_unreachable_server_is_an_enum_and_not_a_message`.
            seele_ffi::ConnectionError::Unreachable
                | seele_ffi::ConnectionError::SemResposta
                | seele_ffi::ConnectionError::HandshakeTimeout
        ),
        "erro inesperado: {:?}",
        falha.error
    );
}

// ---------------------------------------------------------------------------
// O caminho por onde a conversa saiu
// ---------------------------------------------------------------------------
//
// A tabela de `Caminho` é fácil de testar como função pura e difícil de testar
// na fiação, e é a fiação que importa: que o `Snapshot` de uma conexão que
// venceu por um candidato avisado diga `FuroDeNat`. A tarefa 8 reprovou por
// exatamente isto — sete testes deste ciclo ficavam verdes com a propriedade
// que diziam cobrir quebrada, e três deles testavam o auxiliar puro e nunca a
// junção.
//
// # Por que `[::ffff:127.0.0.1]` é o endereço destes dois testes
//
// Porque ele é a única forma de exercitar o degrau 4 inteiro numa máquina só.
// `enlace::e_publico` pergunta por loopback na **forma escrita**, então a forma
// mapeada conta como pública — o `LEVE` sai por ela — e ainda assim o pacote
// chega a um socket desta máquina, porque na forma canônica ela é `127.0.0.1`.
// É o mesmo endereço que `furo.rs` usa, e pelo mesmo motivo.
//
// # O par, e por que ele é um par
//
// Os dois testes conectam ao **mesmo endereço**, ao mesmo servidor, com a mesma
// impressão digital. A única diferença é o bilhete, e portanto o `LEVE`. Um
// deles sozinho não separaria a forma do endereço do aviso: os dois são a
// linha da tabela em que a forma não decide, e é para isso que a tabela tem
// duas linhas de IPv4 público.

/// Sobe um servidor de verdade numa porta que o sistema escolhe.
async fn server_de_teste() -> Option<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.ok()?);
    let endereco = servidor.local_addr().ok()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Some((endereco, servidor))
}

/// Um ponto de encontro que só conta quantos avisos chegaram.
///
/// Ninguém responde: quem entra **nunca lê resposta do ponto de encontro** — é
/// a invariante do ADR 0022 —, então um contador é tudo que este lado precisa
/// para provar que o datagrama saiu.
async fn ponto_que_conta() -> Option<(SocketAddr, Arc<AtomicUsize>)> {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.ok()?;
    let onde = socket.local_addr().ok()?;
    let quantos = Arc::new(AtomicUsize::new(0));
    let contador = Arc::clone(&quantos);
    tokio::spawn(async move {
        let mut balde = [0_u8; 96];
        while socket.recv_from(&mut balde).await.is_ok() {
            contador.fetch_add(1, Ordering::Relaxed);
        }
    });
    Some((onde, quantos))
}

#[tokio::test(flavor = "multi_thread")]
async fn uma_conexao_que_venceu_por_um_candidato_avisado_diz_furo_de_nat() {
    let Some((server, servidor)) = server_de_teste().await else {
        panic!("o servidor de teste não subiu");
    };
    let Some((ponto, avisos)) = ponto_que_conta().await else {
        panic!("o ponto de encontro de teste não subiu");
    };
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há identidade para conectar");
    };
    let Ok(bilhete) = seele_core::uri::Bilhete::novo(ponto.to_string(), "45.33.32.156:41234")
    else {
        panic!("o bilhete de teste não se monta");
    };

    let mut config = config_de_teste(format!("[::ffff:127.0.0.1]:{}", server.port()), casa.path());
    config.expected_fingerprint = Some(servidor.fingerprint().to_owned());
    config.bilhete = Some(bilhete);

    let Ok(entrada) =
        tokio::task::spawn_blocking(move || Connection::connect_with_trail(config)).await
    else {
        panic!("a thread que conecta caiu");
    };
    let (connection, _) = match entrada {
        Ok(entrada) => entrada,
        Err(falha) => panic!("o servidor de teste não deixou entrar: {falha:?}"),
    };

    assert!(
        avisos.load(Ordering::Relaxed) >= 1,
        "nenhum `LEVE` chegou ao ponto de encontro, então este teste diria \
         `FuroDeNat` sobre uma conexão que não avisou ninguém"
    );
    assert_eq!(
        connection.snapshot().caminho,
        Some("FuroDeNat"),
        "a conexão subiu por um candidato público pelo qual avisamos, e o \
         snapshot não diz por onde ela saiu"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mesma_conexao_sem_aviso_nenhum_diz_endereco_publico() {
    // O outro lado do par. Mesmo endereço, mesmo server, mesma impressão
    // digital: só o bilhete sai. Sem ele nenhum `LEVE` é mandado, e o nome tem
    // de mudar — se não mudar, `Snapshot.caminho` está sendo decidido só pela
    // forma do endereço, que é a metade da tabela que não precisa do `avisou`.
    let Some((server, servidor)) = server_de_teste().await else {
        panic!("o servidor de teste não subiu");
    };
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há identidade para conectar");
    };

    let mut config = config_de_teste(format!("[::ffff:127.0.0.1]:{}", server.port()), casa.path());
    config.expected_fingerprint = Some(servidor.fingerprint().to_owned());

    let Ok(entrada) =
        tokio::task::spawn_blocking(move || Connection::connect_with_trail(config)).await
    else {
        panic!("a thread que conecta caiu");
    };
    let (connection, _) = match entrada {
        Ok(entrada) => entrada,
        Err(falha) => panic!("o servidor de teste não deixou entrar: {falha:?}"),
    };

    assert_eq!(
        connection.snapshot().caminho,
        Some("EnderecoPublico"),
        "sem bilhete não sai `LEVE` nenhum, e ainda assim a tela afirma que \
         alguém furou o caminho até aqui"
    );
}

#[test]
fn a_lista_de_caminhos_tem_uma_entrada_por_braco_do_match() {
    // O mesmo guarda de `a_lista_de_etapas_tem_uma_entrada_por_variante_do_enum`,
    // sobre a outra lista, e pelo mesmo motivo: a tarefa 8 tinha três listas
    // paralelas escritas à mão que o compilador não ligava ao enum, e uma
    // variante nova atravessou tudo sem um teste acender.
    //
    // O que o compilador cobra de verdade em Rust estável é um `match` sem `_`,
    // e `Caminho::nome` tem um. A lista é lida **dele**, do texto do módulo.
    let caminho =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../seele-core/src/chegada.rs");
    let Ok(modulo) = std::fs::read_to_string(&caminho) else {
        panic!("`chegada.rs` não está em {}", caminho.display());
    };

    // O segundo `pub fn nome` do arquivo é o de `Caminho`: o primeiro é o de
    // `Etapa`, e é por isso que este guarda conta em vez de procurar.
    let Some(corpo) = modulo
        .split("pub fn nome(&self) -> &'static str {")
        .nth(2)
        .and_then(|resto| resto.split("\n    }").next())
    else {
        panic!(
            "`Caminho::nome` sumiu, e com ela o `match` que o compilador cobra — \
             a partir daqui nada liga a lista de caminhos ao enum"
        );
    };

    let mut do_match: Vec<String> = Vec::new();
    for braco in corpo.split("Self::").skip(1) {
        let Some(variante) = braco.split(|c: char| !c.is_alphanumeric()).next() else {
            continue;
        };
        if variante.is_empty() {
            continue;
        }
        assert!(
            braco.contains(&format!("=> \"{variante}\"")),
            "`{variante}` chega à casca com outro nome que não o seu, e a frase \
             da tela é indexada pelo nome"
        );
        do_match.push(variante.to_owned());
    }
    assert_eq!(
        do_match.len(),
        4,
        "foram lidos {} braços de `Caminho::nome`, e a tabela da seção 5 do spec \
         tem quatro linhas: {do_match:?}",
        do_match.len()
    );

    let na_lista: Vec<String> = Caminho::TODOS
        .iter()
        .map(|caminho| caminho.nome().to_owned())
        .collect();
    assert_eq!(
        do_match, na_lista,
        "`Caminho::TODOS` deixou de ser a lista que `Caminho::nome` conhece, e é \
         dela que a casca recebe os nomes para escrever frase"
    );

    // E a lista que a casca recebe é derivada desta. `apps/seele-app` não pode
    // ver o núcleo (ADR 0002), então sem esta derivação o guarda de cobertura de
    // `frases.js` estaria cobrando contra uma quarta lista escrita à mão.
    assert_eq!(
        seele_ffi::caminhos(),
        na_lista,
        "a lista que atravessa o seele-ffi deixou de ser a do núcleo"
    );
}

/// Um ouvinte que anota tudo o que a ponte lhe entrega.
#[derive(Default)]
struct Anotador {
    eventos: Mutex<Vec<Event>>,
}

impl EventListener for Anotador {
    fn on_event(&self, event: Event) {
        if let Ok(mut lista) = self.eventos.lock() {
            lista.push(event);
        }
    }
}

#[test]
fn quem_liga_a_ponte_antes_do_bloqueio_ve_as_etapas_acontecerem() {
    // A limitação estrutural que a tarefa 8 deixou de pé, como teste.
    //
    // `Chegada::acompanhar` existia e não tinha **um só uso em produção**:
    // `Connection::connect` bloqueia, `Connection::subscribe` só existe depois de ela
    // voltar, e quando ela volta a travessia inteira já aconteceu. Consertar
    // exigia mudar a forma da chamada, e é o que `connect_watching` faz.
    //
    // Dois endereços mortos, porque o que se afirma é a travessia e não o
    // sucesso: a chegada anda, publica, e acaba.
    let Ok(casa) = tempfile::tempdir() else {
        panic!("sem diretório temporário não há identidade para conectar");
    };
    let primeiro = endereco_morto();
    let segundo = endereco_morto();

    let mut config = config_de_teste(primeiro.to_string(), casa.path());
    config.alternate_servers = vec![segundo.to_string()];

    let anotador = Arc::new(Anotador::default());
    let resultado =
        Connection::connect_watching(config, Arc::clone(&anotador) as Arc<dyn EventListener>);
    assert!(
        resultado.is_err(),
        "dois endereços mortos deixaram alguém entrar"
    );

    let Ok(eventos) = anotador.eventos.lock() else {
        panic!("o anotador ficou envenenado");
    };
    let etapas: Vec<&ConnectStage> = eventos
        .iter()
        .filter_map(|evento| match evento {
            Event::ConnectStageChanged { stage } => Some(stage),
            _ => None,
        })
        .collect();

    assert!(
        !etapas.is_empty(),
        "a ponte foi ligada antes do bloqueio e não recebeu etapa nenhuma: {eventos:?}"
    );
    assert!(
        etapas.iter().any(|etapa| matches!(
            etapa,
            ConnectStage::Tentando { onde, .. } if onde == &primeiro.to_string()
        )),
        "nenhuma etapa disse qual endereço estava sendo tentado: {etapas:?}"
    );
    assert!(
        matches!(etapas.last(), Some(ConnectStage::Desistiu { .. })),
        "a última etapa que a casca viu não é o fim da travessia: {etapas:?}"
    );
}
