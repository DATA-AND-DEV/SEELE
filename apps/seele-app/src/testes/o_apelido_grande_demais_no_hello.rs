use super::a_entrada_velha_que_chega_tarde::{app, runtime, servidor, vez};
use super::{connect, Session};
use std::time::Duration;
use tauri::Manager;

#[test]
fn o_apelido_grande_demais_e_recusado_com_nome_antes_de_abrir_a_conexao() {
    let _vez = vez();
    runtime().block_on(async {
        let teto = seele_ffi::Connection::limites_dos_campos().apelido;
        let (endereco, servidor) = servidor("Apelido").await;
        let mut vermelhos = Vec::new();

        // Vinte letras `á` para o teto de hoje: 40 bytes, a medida da base.
        let grande = "á".repeat(teto / 2 + 4);
        let primeira = app();
        let resultado = tokio::time::timeout(
            Duration::from_secs(30),
            connect(
                primeira.clone(),
                primeira.state::<Session>(),
                endereco.to_string(),
                grande.clone(),
                false,
                None,
                None,
            ),
        )
        .await
        .expect("o connect com o apelido grande demais não voltou");
        match &resultado {
            Ok(entrada) => vermelhos.push(format!(
                "um apelido de {} bytes entrou (geração {}), e o fio leva {teto}",
                grande.len(),
                entrada.geracao
            )),
            Err(falha) => {
                let dita = serde_json::to_value(falha).unwrap_or_default();
                let esperada =
                    serde_json::json!({ "ApelidoGrandeDemais": { "limite_bytes": teto } });
                if dita.get("error") != Some(&esperada) {
                    vermelhos.push(format!(
                        "um apelido de {} bytes não voltou recusado com nome e teto \
                             ({esperada}), e a janela não sabe o que dizer: {dita}",
                        grande.len()
                    ));
                }
                let tentou = !matches!(
                    dita.get("trail").and_then(serde_json::Value::as_array),
                    Some(trilha) if trilha.is_empty()
                );
                if tentou {
                    vermelhos.push(format!(
                        "a conexão foi aberta para um apelido que o `Hello` não leva: a \
                             trilha não está vazia: {}",
                        dita.get("trail").cloned().unwrap_or_default()
                    ));
                }
            }
        }
        if let Ok(sessao) = primeira.state::<Session>().connection() {
            sessao.disconnect();
        }

        // E um apelido no teto ainda entra: sem isto, um `connect` que
        // recusasse tudo passaria.
        let certo = "á".repeat(teto / 2);
        let segunda = app();
        match tokio::time::timeout(
            Duration::from_secs(30),
            connect(
                segunda.clone(),
                segunda.state::<Session>(),
                endereco.to_string(),
                certo.clone(),
                false,
                None,
                None,
            ),
        )
        .await
        .expect("o connect com o apelido no teto não voltou")
        {
            Ok(_) => {
                let presentes = servidor.server().presentes.lock().await.todos().len();
                if presentes != 1 {
                    vermelhos.push(format!(
                        "depois das duas tentativas, o servidor tem {presentes} presentes, \
                             e não só a do apelido no teto"
                    ));
                }
            }
            Err(falha) => vermelhos.push(format!(
                "um apelido de {} bytes, no teto, não entrou: {}",
                certo.len(),
                serde_json::to_value(&falha).unwrap_or_default()
            )),
        }
        if let Ok(sessao) = segunda.state::<Session>().connection() {
            sessao.disconnect();
        }

        assert!(
            vermelhos.is_empty(),
            "o apelido grande demais no `Hello`:\n  {}",
            vermelhos.join("\n  ")
        );
    });
}
