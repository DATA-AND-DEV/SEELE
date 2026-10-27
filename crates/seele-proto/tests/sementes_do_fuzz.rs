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
//! certo: todo quadro de controle começa com ela, e o fuzz passaria a mutar
//! quadros que o analisador recusa no primeiro byte. O datagrama de mídia
//! começa com o número da voz, `MEDIA_HEADER_VERSION`, que desde o Servidor 1.0
//! não sobe junto com o protocolo — e é conferido pelo número dele.
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
use seele_proto::{
    oldest_supported_version, MediaHeader, MAX_PAYLOAD_LEN, MEDIA_HEADER_VERSION, PROTOCOL_VERSION,
};

/// A origem do navegador nos testes e no fuzz: de mentira, e não a do produto
/// (`ORIGEM_DO_NAVEGADOR`). A mesma de `fuzz/fuzz_targets/uri.rs`.
const ORIGEM_DOS_TESTES: &str = "origem.invalid";

/// Uma impressão digital qualquer, no formato de verdade: 64 hexadecimais.
const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

/// As sementes de um alvo: o nome do arquivo e os bytes dele.
type DoAlvo = BTreeMap<String, Vec<u8>>;

/// Os alvos cujo primeiro byte é um número de versão, e qual: o do protocolo
/// no controle, e o da voz na mídia (adendo de 08/10/2026, §7).
const ALVOS_COM_VERSAO: [(&str, u8); 2] = [
    ("control_frame", PROTOCOL_VERSION),
    ("media_header", MEDIA_HEADER_VERSION),
];

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
            faixa: seele_proto::FAIXA,
            client: "seele 0.15.0".to_owned(),
            nickname: "ritsuko".to_owned(),
            public_key: vec![0x11; 32],
            material_e2ee: None,
            apis_de_mod: Vec::new(),
        }),
    );
    controle.insert(
        "cliente-hello-com-convite".to_owned(),
        quadro(&ClientMessage::Hello {
            join_secret: Some("7K4MNPQRSTVWXYZ23456".to_owned()),
            faixa: seele_proto::FAIXA,
            client: "seele 0.15.0".to_owned(),
            nickname: "maya".to_owned(),
            public_key: vec![0x12; 32],
            material_e2ee: None,
            apis_de_mod: Vec::new(),
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
    controle.insert(
        "servidor-identidade-apresentada".to_owned(),
        quadro(&ServerMessage::IdentidadeApresentada {
            pessoa: seele_proto::ids::PersonId(1),
            base: "é".repeat(16),
            tag: "A001".into(),
        }),
    );
    todas.insert("control_frame", controle);

    let mut midia = DoAlvo::new();
    let cabecalho = MediaHeader {
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
    // Dois bytes: o alvo lê o primeiro como carimbo e os dois como a faixa de
    // um `Hello`, e esta é a faixa desta build.
    versao.insert(
        "faixa-desta-build".to_owned(),
        vec![seele_proto::FAIXA.minima, seele_proto::FAIXA.maxima],
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
    // As duas formas novas da 0.16: o link do navegador, na origem dos testes,
    // e o link da sala pessoal. Desde a 0.17 o link do navegador não leva mais
    // `wh=` nem `ate=` (os certificados curtos saíram com a WebTransport, ADR
    // 0063), e a versão é a do Servidor 2.0.
    convites.insert(
        "navegador".to_owned(),
        Convite::novo("198.51.100.7:8383")
            .com_impressao_digital(IMPRESSAO)
            .com_token("7K4MNPQRSTVWXYZ23456")
            .com_versao("2.0")
            .no_navegador(ORIGEM_DOS_TESTES, "/e")
            .expect("o convite tem fp")
            .into_bytes(),
    );
    convites.insert(
        "sala".to_owned(),
        uri::link_da_sala(&seele_proto::codigo::Codigo::da_chave(&[0; 32])).into_bytes(),
    );
    todas.insert("uri", convites);

    let marcas = Marcas::do_servidor(IMPRESSAO).expect("a impressão forma marcas");
    let mut do_encontro = DoAlvo::new();
    do_encontro.insert("onde".to_owned(), encontro::onde(marcas.escuta()));
    do_encontro.insert(
        "leve".to_owned(),
        encontro::leve(
            "198.51.100.7:41234".parse().expect("endereço"),
            marcas.aviso(),
        ),
    );
    do_encontro.insert("moro".to_owned(), encontro::moro(marcas.servidor()));
    do_encontro.insert("quem".to_owned(), encontro::quem(marcas.servidor()));
    do_encontro.insert(
        "aqui".to_owned(),
        encontro::aqui(
            marcas.aviso(),
            "203.0.113.9:50000".parse().expect("endereço"),
        ),
    );
    // O ENC/2 (ADR 0062): cada pedido e cada resposta, pelos montadores do
    // produto. A marca é a do servidor da sala da chave de zeros, e a
    // assinatura é de mentira: o alvo confere o fio, e não a chave.
    let sala = Marcas::da_sala(&seele_proto::codigo::Codigo::da_chave(&[0; 32]));
    let dica: std::net::SocketAddr = "192.168.0.10:8385".parse().expect("endereço");
    let publico: std::net::SocketAddr = "203.0.113.9:50000".parse().expect("endereço");
    do_encontro.insert(
        "moro2".to_owned(),
        encontro::moro2(&encontro::Moro2 {
            marca: sala.servidor().clone(),
            chave: [0; 32],
            carimbo: 1_791_000_000,
            dicas: vec![dica],
            assinatura: [0xAB; 64],
            cookie: Some([0xC0; encontro::BYTES_DO_COOKIE]),
        }),
    );
    do_encontro.insert("quem2".to_owned(), encontro::quem2(sala.servidor()));
    for (nome, resposta) in [
        (
            "morou",
            encontro::Resposta2::Morou {
                marca: sala.servidor().clone(),
                endereco: publico,
                cookie: [0xC0; encontro::BYTES_DO_COOKIE],
            },
        ),
        (
            "desafio",
            encontro::Resposta2::Desafio {
                marca: sala.servidor().clone(),
                cookie: [0xC0; encontro::BYTES_DO_COOKIE],
            },
        ),
        (
            "aqui2",
            encontro::Resposta2::Aqui2 {
                marca: sala.servidor().clone(),
                endereco: publico,
                dicas: vec![dica],
            },
        ),
        (
            "ninguem",
            encontro::Resposta2::Ninguem {
                marca: sala.servidor().clone(),
            },
        ),
        (
            "recusa",
            encontro::Resposta2::Recusa {
                marca: sala.servidor().clone(),
                motivo: encontro::Motivo::Relogio,
                agora: 1_791_000_000,
            },
        ),
    ] {
        do_encontro.insert(nome.to_owned(), encontro::resposta2(&resposta));
    }

    // O ENC/3 (ADR 0063): cada pedido e cada resposta, pelos montadores do
    // produto, no mesmo alvo do datagrama, porque o mesmo socket os lê.
    let livres = Marcas::do_servidor(IMPRESSAO).expect("a impressão forma marcas");
    let credencial = encontro::Credencial::nova(&"Ab3".repeat(11)[..32])
        .expect("32 alfanuméricos são credencial");
    let candidato: std::net::SocketAddr = "198.51.100.20:50123".parse().expect("endereço");
    let id = [0x5A; encontro::BYTES_DO_ID];
    let chama = encontro::Chama {
        servidor: livres.servidor().clone(),
        escuta: livres.escuta().clone(),
        credencial: credencial.clone(),
        candidatos: vec![candidato],
    };
    do_encontro.insert(
        "moro3".to_owned(),
        encontro::moro3(&encontro::Moro2 {
            marca: sala.servidor().clone(),
            chave: [0; 32],
            carimbo: 1_792_000_000,
            dicas: vec![dica],
            assinatura: [0xAB; 64],
            cookie: Some([0xC0; encontro::BYTES_DO_COOKIE]),
        }),
    );
    do_encontro.insert(
        "mora3".to_owned(),
        encontro::mora3(livres.servidor(), Some(&[0xC0; encontro::BYTES_DO_COOKIE])),
    );
    do_encontro.insert("quem3".to_owned(), encontro::quem3(livres.servidor()));
    do_encontro.insert("chama".to_owned(), encontro::chama(&chama));
    do_encontro.insert(
        "atende".to_owned(),
        encontro::atende(&encontro::Atende {
            escuta: livres.escuta().clone(),
            id,
            endereco: publico,
            dicas: vec![dica],
        }),
    );
    for (nome, resposta) in [
        (
            "morou3",
            encontro::Resposta3::Morou {
                marca: livres.servidor().clone(),
                endereco: publico,
                cookie: [0xC0; encontro::BYTES_DO_COOKIE],
            },
        ),
        (
            "desafio3",
            encontro::Resposta3::Desafio {
                marca: livres.servidor().clone(),
                cookie: [0xC0; encontro::BYTES_DO_COOKIE],
            },
        ),
        (
            "aqui3",
            encontro::Resposta3::Aqui {
                marca: livres.servidor().clone(),
                endereco: publico,
                dicas: vec![dica],
            },
        ),
        (
            "ninguem3",
            encontro::Resposta3::Ninguem {
                marca: livres.servidor().clone(),
            },
        ),
        (
            "recusa3",
            encontro::Resposta3::Recusa {
                marca: livres.servidor().clone(),
                motivo: encontro::Motivo::Taxa,
                agora: 1_792_000_000,
            },
        ),
        (
            "calado3",
            encontro::Resposta3::Calado {
                marca: livres.servidor().clone(),
            },
        ),
        (
            "chamado3",
            encontro::Resposta3::Chamado(encontro::Chamado {
                escuta: livres.escuta().clone(),
                id,
                credencial: credencial.clone(),
                candidatos: vec![candidato, publico],
            }),
        ),
    ] {
        do_encontro.insert(nome.to_owned(), encontro::resposta3(&resposta));
    }
    todas.insert("datagrama_do_encontro", do_encontro);

    // O corpo do `POST /sinal/1` (SEELE-SINAL/1), que o ponto lê de qualquer
    // navegador, e as respostas que a página lê.
    let mut do_sinal = DoAlvo::new();
    do_sinal.insert(
        "chama".to_owned(),
        encontro::pedido_do_sinal(&chama).into_bytes(),
    );
    for (nome, resposta) in [
        (
            "aqui",
            encontro::RespostaDoSinal::Aqui {
                endereco: publico,
                dicas: vec![dica],
            },
        ),
        ("ninguem", encontro::RespostaDoSinal::Ninguem),
        (
            "recusa",
            encontro::RespostaDoSinal::Recusa {
                motivo: encontro::Motivo::Cheio,
                agora: 1_792_000_000,
            },
        ),
        ("calado", encontro::RespostaDoSinal::Calado),
    ] {
        do_sinal.insert(
            nome.to_owned(),
            encontro::resposta_do_sinal(&resposta).into_bytes(),
        );
    }
    todas.insert("sinal_do_encontro", do_sinal);

    // O envelope dos canais (contrato D4, `seele_enlace::envelope`). O
    // `seele-proto` não depende do `seele-enlace` (ADR 0002), então os bytes
    // vão escritos aqui, pela tabela do D4. Quem confere que o montador do
    // enlace dá exatamente estes bytes é o teste
    // `as_sementes_do_fuzz_sao_as_do_montador`, no `seele-enlace` (pedido ao A).
    let mut do_envelope = DoAlvo::new();
    do_envelope.insert("dados".to_owned(), vec![0x00, 0x0a, 0x0b, 0x00, 0x00]);
    do_envelope.insert("fim".to_owned(), vec![0x01]);
    do_envelope.insert("corte".to_owned(), vec![0x02, 0x00, 0x00, 0x00, 0x02]);
    do_envelope.insert("apresentacao-vazia".to_owned(), vec![0x03]);
    let mut da_sala = vec![0x03];
    da_sala.extend_from_slice(&[0x11; 32]);
    da_sala.extend_from_slice(&[0x22; 64]);
    do_envelope.insert("apresentacao-da-sala".to_owned(), da_sala);
    // O crédito `0x04` (enlace-api.md Δ8): `limite` u64 big-endian, 9 bytes
    // exatos. O valor é o primeiro anúncio de quem lê, a janela de 1 MiB
    // (`MAX_FILA_DE_ENTRADA`), que é o que todo canal da linha 2 recebe ao abrir.
    do_envelope.insert(
        "credito".to_owned(),
        vec![0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00],
    );
    todas.insert("envelope", do_envelope);

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
    for (alvo, versao) in ALVOS_COM_VERSAO {
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
                Some(versao),
                "fuzz/sementes/{alvo}/{nome} não começa com a versão do fio ({versao}), \
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
            seele_proto::version::confere_o_carimbo(versao).is_ok(),
            "a semente version_negotiation/{nome} é uma versão que a negociação recusa"
        );
    }
    for (nome, bytes) in do_alvo(&todas, "uri") {
        let texto = std::str::from_utf8(bytes).expect("um convite é texto");
        assert!(
            uri::analisar(texto).is_ok()
                || uri::analisar_no_navegador(texto, ORIGEM_DOS_TESTES).is_ok()
                || matches!(uri::codigo_do_link(texto), Some(Ok(_))),
            "a semente uri/{nome} não é um convite, um link do navegador nem um link \
             da sala que se leia: {texto}"
        );
    }
    for (nome, bytes) in do_alvo(&todas, "datagrama_do_encontro") {
        let aceita = encontro::analisar(bytes).is_some()
            || encontro::ler_aqui(bytes).is_some()
            || encontro::analisar2(bytes).is_some()
            || encontro::ler_resposta2(bytes).is_some()
            || encontro::analisar3(bytes).is_some()
            || encontro::ler_resposta3(bytes).is_some();
        assert!(
            aceita,
            "a semente datagrama_do_encontro/{nome} não é um pedido nem uma resposta, de \
             nenhuma das três versões, que se leia"
        );
    }
    for (nome, bytes) in do_alvo(&todas, "sinal_do_encontro") {
        let texto = std::str::from_utf8(bytes).expect("o sinal é texto");
        assert!(
            bytes.len() <= encontro::LIMITE_DO_SINAL,
            "a semente sinal_do_encontro/{nome} passa do teto do corpo"
        );
        assert!(
            encontro::ler_pedido_do_sinal(texto).is_some()
                || encontro::ler_resposta_do_sinal(texto).is_some(),
            "a semente sinal_do_encontro/{nome} não é um pedido nem uma resposta do sinal: {texto}"
        );
    }
    // O envelope: a tabela do D4, conferida aqui sem o `seele-enlace`. Uma tag
    // de cada, e cada corpo no tamanho da tag dele.
    for (nome, bytes) in do_alvo(&todas, "envelope") {
        let (tag, corpo) = bytes.split_first().expect("o envelope tem a tag");
        let certo = match tag {
            0x00 => (1..=16 * 1024).contains(&corpo.len()),
            0x01 => corpo.is_empty(),
            0x02 => corpo.len() == 4,
            0x03 => corpo.len() <= 512,
            // O crédito: o limite em 8 bytes, nem mais nem menos (Δ8).
            0x04 => corpo.len() == 8,
            _ => false,
        };
        assert!(certo, "a semente envelope/{nome} não é um envelope do D4");
    }
    // E o crédito tem semente: sem ela o fuzz começa sem nenhum `0x04` válido,
    // e o ramo que o lê só seria achado por sorte.
    assert!(
        do_alvo(&todas, "envelope")
            .iter()
            .any(|(_, bytes)| bytes.first() == Some(&0x04)),
        "o alvo envelope não tem semente do crédito 0x04"
    );
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
