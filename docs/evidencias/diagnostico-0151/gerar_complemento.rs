//! diag-0151 (H): o COMPLEMENTO da pasta da 0.15.1 (`apps/seele-app/testes/pasta-da-0.15.1`),
//! escrito **pelo código da v0.15.1** (`54f542f94`), com as chamadas públicas dela.
//!
//! A pasta de origem traz o esquema 13, a conta `5e5e…` (a de quem hospeda, que não assina),
//! os papéis, o `seele/estilo` 3.1.0 (API 4) ligado e o tema no quintal. O relato pede dados
//! representativos de um servidor de verdade, e faltavam:
//!
//! 1. a identidade TLS do servidor (`tls::Identity::load_or_create`, como o HOSPEDAR e o seeled
//!    da 0.15.1 a criam): sem ela, o primeiro HOSPEDAR da 0.16 cria outra, e a impressão do
//!    convite muda — o caso real tem a de antes;
//! 2. uma segunda conta (`7a7a…`, «convidada») com o papel Operator (`grant_role`);
//! 3. dois canais («geral», «avisos») e uma sala de voz («sala», 6 lugares, presa ao «geral»);
//! 4. três mensagens (`Messages::append_batch`), uma delas resposta a outra;
//! 5. a portaria ligada (`portaria::ligar`) e UMA batida pendente de antes da atualização
//!    (`portaria::bater`, impressão `ab…ab`, apelido «batedora»), para conferir se o pedido de
//!    entrada que esperava na 0.15.1 continua aparecendo depois de subir.
//!
//! **O que continua de fora, e por quê:** anexos (exigem a subida de arquivo pela sessão) e o
//! `identity.key` da máquina de quem hospeda (é da casa do app, e não do banco; o diagnóstico
//! usa a identidade gerada pelo app de teste). Nada aqui finge assinar com a conta `5e5e…`.
//!
//! A saída NÃO vai para o repositório: a identidade TLS é uma chave privada (de teste). Ela fica
//! na pasta `<saída>` que se passa ao gerador; os testes de reprodução a leem de `$SEELE_DIAG_FIXTURE`.
//!
//! ```text
//! cp gerar_complemento.rs <v0151>/crates/seele-server/examples/gerar-complemento-0151.rs
//! cargo run --manifest-path <v0151>/Cargo.toml -p seele-server --example gerar-complemento-0151 \
//!     -- <pasta-da-0.15.1>/pasta/seele.sql <saida>
//! sqlite3 <saida>/seele.db ".output <saida>/seele.sql" ".dump"
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use seele_proto::ids::{ChannelId, ClientMessageId, MessageId, RoleId};
use seele_server::permissions::Permissions;
use seele_server::persistence::channels::Channels;
use seele_server::persistence::messages::{Messages, PendingMessage};
use seele_server::persistence::{Location, Persistence};

fn main() {
    let mut argumentos = std::env::args().skip(1);
    let sql = std::path::PathBuf::from(argumentos.next().expect("uso: <seele.sql> <saída>"));
    let saida = std::path::PathBuf::from(argumentos.next().expect("uso: <seele.sql> <saída>"));
    assert!(!saida.exists(), "a saída já existe: {}", saida.display());
    std::fs::create_dir_all(&saida).unwrap();
    let banco = saida.join("seele.db");

    // 0. A pasta da 0.15.1, como ela é.
    rusqlite::Connection::open(&banco)
        .unwrap()
        .execute_batch(&std::fs::read_to_string(&sql).unwrap())
        .unwrap();
    let mut persistence = Persistence::open(&Location::File(banco.clone())).expect("o banco abriu");

    // 1. A identidade TLS.
    let identidade =
        seele_server::tls::Identity::load_or_create(&persistence, vec!["localhost".to_owned()])
            .expect("a identidade TLS");
    println!("identidade-tls {}", identidade.fingerprint());

    // 2. A segunda conta, Operator.
    let convidada = Permissions::new(&persistence)
        .register_or_find(&[0x7a; 32], "convidada")
        .expect("a conta convidada");
    Permissions::new(&persistence)
        .grant_role(convidada.id, RoleId(2))
        .expect("o papel Operator");
    println!("convidada {}", convidada.id.0);

    // 3. Canais e sala.
    let (geral, avisos) = {
        let canais = Channels::new(&persistence);
        let geral = canais.create_channel("geral").expect("geral");
        let avisos = canais.create_channel("avisos").expect("avisos");
        let sala = canais
            .create_voice_room("sala", 6, Some(geral.id))
            .expect("a sala");
        println!("canais {} {} sala {}", geral.id.0, avisos.id.0, sala.id.0);
        (geral.id, avisos.id)
    };

    // 4. Mensagens.
    let dono = seele_proto::ids::PersonId(1);
    let primeira = Messages::new(&mut persistence)
        .append_batch(&[
            PendingMessage {
                channel: geral,
                author: dono,
                author_nickname: "dono".to_owned(),
                body: "bem-vinda à casa (0.15.1)".to_owned(),
                replies_to: None,
                client_message_id: Some(ClientMessageId(1)),
            },
            PendingMessage {
                channel: avisos,
                author: dono,
                author_nickname: "dono".to_owned(),
                body: "o tema mudou".to_owned(),
                replies_to: None,
                client_message_id: Some(ClientMessageId(2)),
            },
        ])
        .expect("as mensagens");
    let id_da_primeira: MessageId = primeira
        .first()
        .and_then(|g| g.gravada())
        .map(|m| m.id)
        .expect("a primeira mensagem gravada");
    Messages::new(&mut persistence)
        .append_batch(&[PendingMessage {
            channel: geral,
            author: convidada.id,
            author_nickname: "convidada".to_owned(),
            body: "obrigada!".to_owned(),
            replies_to: Some(id_da_primeira),
            client_message_id: Some(ClientMessageId(1)),
        }])
        .expect("a resposta");
    let _: ChannelId = geral;

    // 5. A portaria, com uma batida que ficou esperando.
    seele_server::portaria::ligar(&mut persistence, true).expect("ligar a portaria");
    let resposta = seele_server::portaria::bater(
        &mut persistence,
        &"ab".repeat(32),
        "batedora",
        seele_server::portaria::Segredo::Aberto,
        "",
    )
    .expect("a batida");
    println!("batida {resposta:?}");
    println!(
        "pendentes {}",
        seele_server::portaria::pendentes(&persistence).expect("pendentes")
    );
}
