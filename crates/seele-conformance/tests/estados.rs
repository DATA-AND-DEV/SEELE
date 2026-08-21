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
use std::sync::Arc;

use seele_core::chegada::{Chegada, Etapa};
use seele_core::enlace::Destino;
use seele_core::{MemoryPinStore, PinStore, SigningKey};
use seele_ffi::{ConnectConfig, ConnectStage, Plug};

/// Um endereço onde não há ninguém.
///
/// Aberto e devolvido, para a porta ser real e estar livre: um número escolhido
/// à mão poderia ser o Dogma de outra pessoa na máquina que roda os testes. É o
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

/// Dois candidatos que ninguém atende — um convite cujo Dogma não existe.
fn destinos_mortos_de_teste() -> Vec<Destino> {
    (0..2)
        .map(|_| {
            let servidor = endereco_morto();
            Destino {
                servidor,
                nome_tls: "localhost".into(),
                chave_do_pin: servidor.to_string(),
                apelido: "piloto".into(),
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

    let falha = match Plug::connect_with_trail(ConnectConfig {
        server: primeiro.to_string(),
        alternate_servers: vec![segundo.to_string()],
        nickname: "piloto".into(),
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
            seele_ffi::PlugError::Unreachable | seele_ffi::PlugError::HandshakeTimeout
        ),
        "erro inesperado: {:?}",
        falha.error
    );
}
