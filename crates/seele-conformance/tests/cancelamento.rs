//! Uma regra de arquitetura que só se enxerga lendo o código-fonte.
//!
//! `frame::read` **não é cancel-safe**: são dois `read_exact`, tamanho e corpo,
//! e cancelado entre os dois ele descarta o tamanho já consumido. O `read`
//! seguinte lê os primeiros bytes do corpo como se fossem um tamanho, e o fluxo
//! fica deslocado para sempre. `crates/seele-core/src/frame.rs` tem o teste que
//! prova o mecanismo num par QUIC de verdade.
//!
//! `tokio::select!` cancela todos os ramos que perdem a corrida. Logo, `read`
//! dentro de um `select!` é um fluxo que uma hora dessincroniza — e o defeito
//! não aparece no ramo culpado: aparece depois, longe, como um cliente que
//! continua conectado e cujas mensagens o servidor deixa de entender.
//!
//! Este teste existe porque o defeito voltou. Na primeira vez eu corrigi só o
//! lado do cliente; o servidor guardou o mesmo `select!` com um `interval` de um
//! segundo dentro, ou seja, uma oportunidade de cancelar por segundo em toda
//! sessão. Um teste de comportamento não pega isso de forma confiável, porque
//! depende de tamanho e corpo caírem em pacotes separados. Ler o fonte pega.
//!
//! A forma certa é sempre a mesma: uma tarefa dona do fluxo, entregando quadros
//! inteiros por um canal, e o `select!` esperando no canal — `recv` de canal é
//! cancel-safe.
//!
//! Nota: `tokio::time::timeout` em volta de um `read` **não** cai nesta regra
//! quando o estouro encerra a conexão, como no handshake do servidor. O que
//! envenena é cancelar e continuar usando o mesmo fluxo.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

/// Os arquivos que falam QUIC. Se aparecer outro, entra aqui.
const VIGIADOS: &[&str] = &[
    "crates/seele-core/src/client.rs",
    "crates/seele-core/src/frame.rs",
    "crates/seele-server/src/session.rs",
    "crates/seele-server/src/lib.rs",
];

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("a raiz do workspace")
        .to_path_buf()
}

/// Tira comentários e literais de texto.
///
/// Necessário, e descoberto do jeito certo: a primeira versão deste guarda
/// acusou duas vezes o arquivo que eu **acabara de consertar**, porque o
/// comentário que explica o conserto contém as palavras `select!` e
/// `frame::read`. Um guarda que lê comentário mede prosa, não código.
fn so_codigo(fonte: &str) -> String {
    let mut saida = String::with_capacity(fonte.len());
    let bytes: Vec<char> = fonte.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        let atual = bytes[i];
        let proximo = bytes.get(i + 1).copied();

        match (atual, proximo) {
            ('/', Some('/')) => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            ('"', _) => {
                // Espaço no lugar do literal: some o conteúdo e as chaves que
                // ele pudesse ter, sem colar as palavras vizinhas.
                saida.push(' ');
                i += 1;
                while i < bytes.len() && bytes[i] != '"' {
                    i += if bytes[i] == '\\' { 2 } else { 1 };
                }
                i += 1;
            }
            _ => {
                saida.push(atual);
                i += 1;
            }
        }
    }

    saida
}

/// Os trechos de cada `tokio::select!` de um arquivo.
///
/// Conta chaves a partir do `select!`. Basta porque o que entra aqui já passou
/// por [`so_codigo`], então não há chave dentro de texto nem de comentário.
fn blocos_de_select(fonte: &str) -> Vec<String> {
    let mut blocos = Vec::new();
    let mut resto = fonte;

    while let Some(inicio) = resto.find("select!") {
        let depois = &resto[inicio..];
        let Some(abre) = depois.find('{') else { break };

        let mut profundidade = 0_i32;
        let mut fim = None;
        for (indice, caractere) in depois[abre..].char_indices() {
            match caractere {
                '{' => profundidade += 1,
                '}' => {
                    profundidade -= 1;
                    if profundidade == 0 {
                        fim = Some(abre + indice);
                        break;
                    }
                }
                _ => {}
            }
        }

        match fim {
            Some(fim) => {
                blocos.push(depois[abre..=fim].to_owned());
                resto = &depois[fim..];
            }
            None => break,
        }
    }

    blocos
}

#[test]
fn nenhum_select_le_um_quadro_direto_do_fluxo() {
    let mut culpados = Vec::new();

    for arquivo in VIGIADOS {
        let fonte = std::fs::read_to_string(raiz().join(arquivo))
            .unwrap_or_else(|erro| panic!("não li {arquivo}: {erro}"));

        for bloco in blocos_de_select(&so_codigo(&fonte)) {
            if bloco.contains("frame::read") {
                culpados.push(*arquivo);
            }
        }
    }

    assert!(
        culpados.is_empty(),
        "`frame::read` dentro de um `select!` em {culpados:?}.\n\
         O ramo que perde a corrida é cancelado no meio do quadro e o fluxo \
         dessincroniza para sempre.\n\
         Ponha uma tarefa dona do fluxo entregando por canal, e espere no canal."
    );
}

#[test]
fn o_teste_enxerga_o_defeito_que_existia() {
    // Um guarda que não é conferido contra o código errado é um guarda que
    // ninguém sabe se funciona. Este é o `select!` que estava na sessão.
    let como_era = r"
    loop {
        tokio::select! {
            incoming = frame::read::<ClientMessage>(&mut recv) => {
                let Ok(message) = incoming else { break };
                match message { _ => {} }
            }
            _ = telemetry.tick() => { announce(); }
        }
    }
    ";

    let blocos = blocos_de_select(&so_codigo(como_era));
    assert_eq!(blocos.len(), 1, "não achou o select");
    assert!(
        blocos[0].contains("frame::read"),
        "o guarda não veria o defeito que já aconteceu"
    );
}

#[test]
fn o_teste_nao_acusa_o_que_esta_certo() {
    // A forma correta, e a que a sessão usa hoje: canal no `select!`, leitura
    // numa tarefa fora dele.
    let como_e = r#"
    tokio::spawn(async move {
        loop {
            match frame::read::<ClientMessage>(&mut recv).await {
                Ok(mensagem) => { let _ = para_dentro.send(mensagem).await; }
                Err(_) => return,
            }
        }
    });
    loop {
        tokio::select! {
            incoming = entrada.recv() => { let Some(_m) = incoming else { break }; }
            _ = telemetry.tick() => { announce(); }
        }
    }
    "#;

    let blocos = blocos_de_select(&so_codigo(como_e));
    assert_eq!(blocos.len(), 1);
    assert!(!blocos[0].contains("frame::read"));
}

#[test]
fn um_comentario_que_fala_do_defeito_nao_e_o_defeito() {
    // Foi exatamente assim que este guarda deu falso positivo: o comentário que
    // explica por que `frame::read` saiu do `select!` menciona os dois.
    let arquivo_consertado = r"
    // Ler direto dentro do `select!` era um defeito: `frame::read` faz dois
    // `read_exact` e o cancelamento perde o tamanho.
    loop {
        tokio::select! {
            incoming = entrada.recv() => { break; }
        }
    }
    ";

    let blocos = blocos_de_select(&so_codigo(arquivo_consertado));
    assert_eq!(blocos.len(), 1, "achou select onde só havia comentário");
    assert!(!blocos[0].contains("frame::read"));
}
