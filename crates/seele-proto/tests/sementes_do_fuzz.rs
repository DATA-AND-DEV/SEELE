//! As sementes do fuzz: entradas válidas de cada alvo de `fuzz/`, montadas
//! pelo código de verdade e guardadas em `fuzz/sementes/<alvo>/`.
//!
//! # Por que elas existem
//!
//! O libFuzzer parte de um corpus, e o de cada máquina mora em `/fuzz/corpus/`,
//! que o `.gitignore` deixa de fora: é achado local, e não fonte. Um runner de
//! CI começa sem nada, e o job de fuzz do `ci.yml` roda cada alvo por sessenta
//! segundos. As sementes dão a ele uma entrada válida de cada tipo para mutar.
//!
//! Onde elas pesam, medido em 05/10/2026 com dez segundos de cada alvo neste
//! Mac, com e sem elas (o `cov:` do libFuzzer, pontos de cobertura): o convite
//! foi de 86 para 601, e o datagrama do encontro de 52 para 168. O quadro de
//! controle quase não mudou (3 820 sem, 3 828 com): o postcard ele aprende
//! sozinho, e ali a semente vale pela conferência abaixo, que a prende à
//! versão do fio.
//!
//! # Por que este teste as confere, e só as escreve quando pedido
//!
//! Quem as produz são o `encode` dos quadros, o `encode_datagram` da mídia, o
//! `Display` do convite e os montadores do encontro — os mesmos que o produto
//! usa. Este teste os chama e compara o resultado com o que está no disco,
//! byte a byte. Uma semente editada à mão, ou deixada para trás por uma mudança
//! do fio, reprova aqui. A **próxima subida de `PROTOCOL_VERSION`** é o caso
//! certo: todo quadro de controle e todo datagrama de mídia começam com ela. Um
//! datagrama de mídia velho é recusado no primeiro byte, e um quadro de
//! controle velho passa, porque a janela de compatibilidade é 1 — e o fuzz
//! passaria a mutar quadros da versão de antes, e não os do fio.
//!
//! Para reescrevê-las depois de uma mudança do fio:
//!
//! ```text
//! SEELE_REGERAR_SEMENTES=1 cargo test -p seele-proto --test sementes_do_fuzz
//! ```
//!
//! Ele escreve o que gera e não apaga nada: um arquivo que sobrar no disco
//! reprova a conferência, com o nome dele, e quem decide o que fazer com ele
//! é quem o leu.
//!
//! Mora no `seele-proto` porque o `xtask`, que guarda o `ci.yml`, não depende
//! dele e não deve passar a depender.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use seele_proto::control::{decode, encode, ClientMessage, DisconnectReason, ServerMessage};
use seele_proto::encontro::{self, Marcas};
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_proto::uri::{self, Bilhete, Convite};
use seele_proto::{oldest_supported_version, MediaHeader, MAX_PAYLOAD_LEN, PROTOCOL_VERSION};

/// Uma impressão digital qualquer, no formato de verdade: 64 hexadecimais.
const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

/// As sementes de um alvo: o nome do arquivo e os bytes dele.
type DoAlvo = BTreeMap<String, Vec<u8>>;

/// Os alvos cujo primeiro byte é a versão do fio.
const ALVOS_COM_VERSAO: [&str; 2] = ["control_frame", "media_header"];

fn quadro<T>(mensagem: &T) -> Vec<u8>
where
    T: serde::Serialize + seele_proto::control::Validate,
{
    encode(mensagem).expect("uma semente é um quadro que o próprio `encode` aceita")
}

/// O que cada alvo recebe, montado agora pelo código de verdade.
fn sementes() -> BTreeMap<&'static str, DoAlvo> {
    let mut todas = BTreeMap::new();

    // Os dois sentidos, porque o alvo decodifica os dois: o cliente lê o que o
    // servidor manda, o servidor lê o que o cliente manda, e só um dos lados
    // está atrás da autenticação.
    let mut controle = DoAlvo::new();
    controle.insert(
        "cliente-hello".to_owned(),
        quadro(&ClientMessage::Hello {
            join_secret: None,
            version: PROTOCOL_VERSION,
            client: "seele 0.15.0".to_owned(),
            nickname: "ritsuko".to_owned(),
            public_key: vec![0x11; 32],
        }),
    );
    controle.insert(
        "cliente-hello-com-convite".to_owned(),
        quadro(&ClientMessage::Hello {
            join_secret: Some("7K4MNPQRSTVWXYZ23456".to_owned()),
            version: PROTOCOL_VERSION,
            client: "seele 0.15.0".to_owned(),
            nickname: "maya".to_owned(),
            public_key: vec![0x12; 32],
        }),
    );
    controle.insert(
        "cliente-resposta".to_owned(),
        quadro(&ClientMessage::Response {
            proof: vec![0x22; 64],
        }),
    );
    controle.insert(
        "cliente-entrar-na-sala".to_owned(),
        quadro(&ClientMessage::EnterVoiceRoom {
            voice_room: VoiceRoomId(1),
            password: None,
        }),
    );
    controle.insert(
        "cliente-mensagem".to_owned(),
        quadro(&ClientMessage::SendMessage {
            channel: ChannelId(1),
            body: "olá, linha".to_owned(),
            replies_to: None,
            client_message_id: ClientMessageId(7),
        }),
    );
    controle.insert(
        "cliente-ping".to_owned(),
        quadro(&ClientMessage::Ping {
            timestamp: 1_700_000_000_000,
        }),
    );
    controle.insert(
        "servidor-desafio".to_owned(),
        quadro(&ServerMessage::Challenge {
            nonce: vec![0x33; 32],
        }),
    );
    controle.insert(
        "servidor-pong".to_owned(),
        quadro(&ServerMessage::Pong {
            timestamp: 1_700_000_000_000,
        }),
    );
    controle.insert(
        "servidor-desligando".to_owned(),
        quadro(&ServerMessage::Disconnecting {
            reason: DisconnectReason::ServerShuttingDown,
        }),
    );
    todas.insert("control_frame", controle);

    let mut midia = DoAlvo::new();
    let cabecalho = MediaHeader {
        version: PROTOCOL_VERSION,
        ssrc: 0x0102_0304,
        seq: 1,
        timestamp: 960,
    };
    for (nome, carga) in [
        ("voz-curta", vec![0xFC, 0xFF, 0xFE]),
        ("voz-cheia", vec![0x5A; MAX_PAYLOAD_LEN]),
    ] {
        let mut datagrama = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
        let tamanho = cabecalho
            .encode_datagram(&carga, &mut datagrama)
            .expect("uma semente é um datagrama que o próprio `encode_datagram` aceita");
        datagrama.truncate(tamanho);
        midia.insert(nome.to_owned(), datagrama);
    }
    todas.insert("media_header", midia);

    let mut versao = DoAlvo::new();
    versao.insert("versao-atual".to_owned(), vec![PROTOCOL_VERSION]);
    versao.insert(
        "versao-mais-antiga".to_owned(),
        vec![oldest_supported_version()],
    );
    todas.insert("version_negotiation", versao);

    let mut convites = DoAlvo::new();
    convites.insert(
        "minimo".to_owned(),
        Convite::novo("server.exemplo:8383")
            .to_string()
            .into_bytes(),
    );
    convites.insert(
        "completo".to_owned(),
        Convite::novo("192.168.0.7:8383")
            .com_alternativos(["203.0.113.5:8383", "[2001:db8::7]:8383"])
            .com_bilhete(
                Bilhete::novo("encontro.exemplo:8384", "198.51.100.7:41234")
                    .expect("as duas metades do bilhete são endereços"),
            )
            .com_impressao_digital(IMPRESSAO)
            .com_token("7K4MNPQRSTVWXYZ23456")
            .com_voice_room(2)
            .com_versao("0.15.0")
            .to_string()
            .into_bytes(),
    );
    todas.insert("uri", convites);

    let marcas = Marcas::do_servidor(IMPRESSAO).expect("a impressão forma marcas");
    let mut do_encontro = DoAlvo::new();
    do_encontro.insert("onde".to_owned(), encontro::onde(&marcas.escuta));
    do_encontro.insert(
        "leve".to_owned(),
        encontro::leve(
            "198.51.100.7:41234".parse().expect("endereço"),
            &marcas.aviso,
        ),
    );
    do_encontro.insert("moro".to_owned(), encontro::moro(&marcas.servidor));
    do_encontro.insert("quem".to_owned(), encontro::quem(&marcas.servidor));
    do_encontro.insert(
        "aqui".to_owned(),
        encontro::aqui(
            &marcas.aviso,
            "203.0.113.9:50000".parse().expect("endereço"),
        ),
    );
    todas.insert("datagrama_do_encontro", do_encontro);

    todas
}

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pasta_do(alvo: &str) -> PathBuf {
    raiz().join("fuzz/sementes").join(alvo)
}

/// O que está no disco para um alvo, por nome. Vazio se a pasta não existe.
fn no_disco(alvo: &str) -> DoAlvo {
    let mut achadas = DoAlvo::new();
    let Ok(leitura) = std::fs::read_dir(pasta_do(alvo)) else {
        return achadas;
    };
    for item in leitura {
        let caminho = item.expect("entrada de diretório").path();
        if !caminho.is_file() {
            continue;
        }
        let nome = caminho
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .expect("nome de semente em UTF-8")
            .to_owned();
        let bytes = std::fs::read(&caminho).expect("a semente é legível");
        achadas.insert(nome, bytes);
    }
    achadas
}

/// Com `SEELE_REGERAR_SEMENTES=1`, escreve as sementes uma vez por processo.
///
/// Uma vez só, e antes de qualquer leitura: os testes deste arquivo correm em
/// paralelo, e um que lesse enquanto outro escreve veria metade de um arquivo.
fn regerar_se_pedido() {
    static FEITO: OnceLock<()> = OnceLock::new();
    if std::env::var("SEELE_REGERAR_SEMENTES").as_deref() != Ok("1") {
        return;
    }
    FEITO.get_or_init(|| {
        for (alvo, do_alvo) in sementes() {
            let pasta = pasta_do(alvo);
            std::fs::create_dir_all(&pasta).expect("a pasta das sementes é criável");
            for (nome, bytes) in do_alvo {
                std::fs::write(pasta.join(&nome), bytes).expect("a semente é gravável");
            }
        }
    });
}

#[test]
fn as_sementes_do_fuzz_estao_na_versao_do_fio() {
    regerar_se_pedido();
    for alvo in ALVOS_COM_VERSAO {
        let achadas = no_disco(alvo);
        assert!(
            !achadas.is_empty(),
            "fuzz/sementes/{alvo} está vazia ou não existe, e o fuzz desse alvo começa do \
             nada no CI. Gere com SEELE_REGERAR_SEMENTES=1 cargo test -p seele-proto \
             --test sementes_do_fuzz"
        );
        for (nome, bytes) in &achadas {
            assert_eq!(
                bytes.first().copied(),
                Some(PROTOCOL_VERSION),
                "fuzz/sementes/{alvo}/{nome} não começa com a versão do fio ({PROTOCOL_VERSION}), \
                 e o primeiro byte é a primeira coisa que o analisador confere: o fuzz \
                 gastaria o tempo dele numa recusa só. Regere com SEELE_REGERAR_SEMENTES=1 \
                 cargo test -p seele-proto --test sementes_do_fuzz"
            );
        }
    }
}

#[test]
fn as_sementes_no_disco_sao_as_que_o_codigo_gera() {
    regerar_se_pedido();
    let mut divergencias = Vec::new();
    for (alvo, geradas) in sementes() {
        let achadas = no_disco(alvo);
        for (nome, bytes) in &geradas {
            match achadas.get(nome) {
                None => divergencias.push(format!("falta fuzz/sementes/{alvo}/{nome}")),
                Some(no_disco) if no_disco != bytes => divergencias.push(format!(
                    "fuzz/sementes/{alvo}/{nome} difere do que o código gera hoje"
                )),
                Some(_) => {}
            }
        }
        for nome in achadas.keys() {
            if !geradas.contains_key(nome) {
                divergencias.push(format!(
                    "fuzz/sementes/{alvo}/{nome} está no disco e o código não a gera: \
                     apague-a, ou acrescente quem a gera em `sementes()`"
                ));
            }
        }
    }
    assert!(
        divergencias.is_empty(),
        "as sementes do fuzz não são as que o código gera hoje — um fio que mudou, ou \
         uma semente editada à mão:\n  {}\n\
         Regere com SEELE_REGERAR_SEMENTES=1 cargo test -p seele-proto --test sementes_do_fuzz",
        divergencias.join("\n  ")
    );
}

/// As sementes de um alvo, que `sementes()` tem de ter.
fn do_alvo<'a>(todas: &'a BTreeMap<&'static str, DoAlvo>, alvo: &str) -> &'a DoAlvo {
    todas
        .get(alvo)
        .unwrap_or_else(|| panic!("`sementes()` não tem o alvo «{alvo}»"))
}

#[test]
fn toda_semente_e_uma_entrada_que_o_alvo_aceita() {
    // Uma semente recusada logo na entrada ensina ao fuzz só a recusa. Cada uma
    // tem de passar pelo analisador do alvo dela.
    let todas = sementes();

    for (nome, bytes) in do_alvo(&todas, "control_frame") {
        let aceita =
            decode::<ClientMessage>(bytes).is_ok() || decode::<ServerMessage>(bytes).is_ok();
        assert!(
            aceita,
            "a semente control_frame/{nome} não é um quadro que o `decode` aceite"
        );
    }
    for (nome, bytes) in do_alvo(&todas, "media_header") {
        assert!(
            MediaHeader::decode(bytes).is_ok(),
            "a semente media_header/{nome} não é um datagrama que o `decode` aceite"
        );
    }
    for (nome, bytes) in do_alvo(&todas, "version_negotiation") {
        let versao = bytes.first().copied().expect("a semente tem um byte");
        assert!(
            seele_proto::version::negotiate(versao).is_ok(),
            "a semente version_negotiation/{nome} é uma versão que a negociação recusa"
        );
    }
    for (nome, bytes) in do_alvo(&todas, "uri") {
        let texto = std::str::from_utf8(bytes).expect("um convite é texto");
        assert!(
            uri::analisar(texto).is_ok(),
            "a semente uri/{nome} não é um convite que o `analisar` aceite: {texto}"
        );
    }
    for (nome, bytes) in do_alvo(&todas, "datagrama_do_encontro") {
        let aceita = encontro::analisar(bytes).is_some() || encontro::ler_aqui(bytes).is_some();
        assert!(
            aceita,
            "a semente datagrama_do_encontro/{nome} não é um pedido nem um AQUI que se leia"
        );
    }
}

/// Os nomes dos `[[bin]]` de `fuzz/Cargo.toml`.
fn alvos_do_fuzz() -> Vec<String> {
    let manifesto =
        std::fs::read_to_string(raiz().join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml é legível");
    let mut alvos = Vec::new();
    let mut num_bin = false;
    for linha in manifesto.lines().map(str::trim) {
        if linha.starts_with('[') {
            num_bin = linha == "[[bin]]";
            continue;
        }
        if !num_bin {
            continue;
        }
        if let Some(valor) = linha
            .strip_prefix("name")
            .map(str::trim_start)
            .and_then(|resto| resto.strip_prefix('='))
        {
            alvos.push(valor.trim().trim_matches('"').to_owned());
        }
    }
    alvos
}

#[test]
fn todo_alvo_do_fuzz_tem_sementes() {
    // O job de fuzz do `ci.yml` roda cada alvo a partir de
    // `fuzz/sementes/<alvo>`, e o libFuzzer recusa uma pasta que não existe. Um
    // alvo novo sem semente reprovaria lá, no Actions; aqui reprova antes.
    let alvos = alvos_do_fuzz();
    assert!(
        !alvos.is_empty(),
        "não achei nenhum `[[bin]]` em fuzz/Cargo.toml, e isso não é aprovação"
    );
    let geradas = sementes();
    for alvo in &alvos {
        assert!(
            geradas.contains_key(alvo.as_str()),
            "o alvo de fuzz «{alvo}» não tem sementes em `sementes()`, e o job de fuzz do \
             ci.yml o rodaria a partir de uma pasta que não existe"
        );
    }
}

#[test]
fn as_sementes_ficam_fora_do_conversor_de_fim_de_linha() {
    // As sementes são conferidas byte a byte, e o `* text=auto eol=lf` do
    // `.gitattributes` normaliza CRLF para LF no commit de tudo o que o Git
    // achar que é texto. Um quadro de controle sem byte nulo, com um 0x0D 0x0A
    // no meio, sairia do commit com um byte a menos, e o clone limpo
    // reprovaria a conferência sem ninguém ter mexido em nada.
    let regras = std::fs::read_to_string(raiz().join(".gitattributes"))
        .expect("o .gitattributes da raiz é legível");
    let protegidas = regras
        .lines()
        .map(str::trim)
        .filter(|linha| !linha.starts_with('#'))
        .any(|linha| {
            let mut campos = linha.split_whitespace();
            campos.next() == Some("fuzz/sementes/**") && campos.any(|campo| campo == "-text")
        });
    assert!(
        protegidas,
        "o .gitattributes não tem a linha `fuzz/sementes/** -text`, e uma semente que o Git \
         tome por texto pode perder bytes no commit"
    );
}
