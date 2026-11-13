//! **A sala pessoal de uma pessoa, para as bancadas** (ADR 0068): o que o app
//! hospeda quando alguém liga para o código dele, sem o app.
//!
//!     cargo run -p seele-server --example sala_da_bancada <ponto> [--recusar]
//!
//! Sobe a sala pelo [`Hospedagem::iniciar_sala`], como o app, registra-a
//! assinada no ponto de encontro dado e espera o registro. A primeira linha da
//! saída é um JSON:
//!
//! ```json
//! {"codigo": "XXXX-XXXX-XXXX-XXXX", "porta": 54321}
//! ```
//!
//! Depois, **uma linha por toque**: `{"tocou": "<código de quem liga>",
//! "apelido": "…", "atendeu": true}`. Quem recebe aqui é a própria bancada: o
//! exemplo atende todo toque na hora (ou recusa, com `--recusar`), que é o que
//! o ATENDER da faixa faz no app. Fecha quando a bancada fecha a entrada.
//!
//! **Não é o app**: sem agenda, sem bloqueados e sem a sessão de quem recebe.
//! O ponto é o da bancada, em `127.0.0.1`; o de produção é recusado pelas
//! paredes de sempre.

#![allow(clippy::print_stdout, reason = "a saída é o contrato com a bancada")]

use std::io::Read as _;
use std::time::Duration;

use anyhow::{anyhow, Result};
use seele_proto::codigo::Codigo;
use seele_server::alcance::encontro::EstadoDoRegistro;
use seele_server::hospedagem::Hospedagem;
use seele_server::recepcao::Aviso;

/// A semente de quem recebe: fixa, para a bancada conhecer o código de antemão
/// se quiser. É uma chave de teste, e não mora em disco.
const SEMENTE: [u8; 32] = [42; 32];

#[tokio::main]
async fn main() -> Result<()> {
    let mut argumentos = std::env::args().skip(1);
    let ponto = argumentos
        .next()
        .ok_or_else(|| anyhow!("uso: sala_da_bancada <ponto> [--recusar]"))?;
    let recusar = argumentos.any(|argumento| argumento == "--recusar");

    let sala = Hospedagem::iniciar_sala(0, &SEMENTE, "dona", Some(ponto)).await?;
    let mut registro = sala
        .registro_no_quarto()
        .ok_or_else(|| anyhow!("a sala subiu sem registro no ponto"))?;
    tokio::time::timeout(
        Duration::from_secs(10),
        registro.wait_for(|estado| *estado == EstadoDoRegistro::Registrado),
    )
    .await
    .map_err(|_| anyhow!("a sala não se registrou no ponto em 10 s"))??;

    let publica = ed25519_dalek::SigningKey::from_bytes(&SEMENTE)
        .verifying_key()
        .to_bytes();
    println!(
        "{}",
        serde_json::json!({
            "codigo": Codigo::da_chave(&publica).to_string(),
            "porta": sala.endereco().port(),
        })
    );

    let recepcao = sala
        .recepcao()
        .ok_or_else(|| anyhow!("a sala subiu sem campainha"))?;
    let mut avisos = recepcao.assinar();
    let atendendo = tokio::spawn(async move {
        while let Ok(aviso) = avisos.recv().await {
            if let Aviso::Tocou {
                toque,
                codigo,
                apelido,
                ..
            } = aviso
            {
                let feito = if recusar {
                    recepcao.recusar(toque)
                } else {
                    recepcao.atender(toque)
                };
                println!(
                    "{}",
                    serde_json::json!({
                        "tocou": codigo.to_string(),
                        "apelido": apelido,
                        "atendeu": feito && !recusar,
                    })
                );
            }
        }
    });

    // Até a bancada fechar o tubo.
    tokio::task::spawn_blocking(|| {
        let mut resto = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut resto);
    })
    .await?;
    atendendo.abort();
    sala.encerrar().await;
    Ok(())
}
