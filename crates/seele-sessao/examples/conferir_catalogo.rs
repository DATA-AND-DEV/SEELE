//! Confere o catálogo publicado com o MESMO código e a MESMA chave do app: `ler_catalogo` e
//! `ler_revogacoes` de [`seele_sessao::catalogo`], contra `apps/seele-app/chaves/mods.pub`.
//!
//! O preflight do `publicar.sh` baixa os quatro arquivos de `mods.seele.sh` e passa por aqui. Um
//! `200` não diz nada: o Pages devolve a página da SPA com `200` para qualquer caminho, e um
//! catálogo copiado sem a assinatura, ou com a de outro arquivo, também responde `200`.
//!
//! **E a assinatura sozinha também não basta.** O catálogo de hoje, assinado e íntegro, não tem
//! as versões da API 7 dos MODs oficiais: a 0.17 sairia com o índice certo e sem os MODs que ela
//! pede. Cada `--exige <id>@<versão>@<api>` tem de estar no catálogo, com aquela API, e fora das
//! revogações.
//!
//!   cargo run -q -p seele-sessao --example conferir_catalogo -- \
//!       <catalogo.json> <catalogo.json.minisig> <revogacoes.json> <revogacoes.json.minisig> <chave.pub> \
//!       [--exige seele/mesa@5.0.0@7 ...]

use seele_sessao::catalogo::{Catalogo, Revogacoes};

fn ler(caminho: &str) -> Result<Vec<u8>, String> {
    std::fs::read(caminho).map_err(|erro| format!("{caminho}: {erro}"))
}

fn texto(caminho: &str) -> Result<String, String> {
    String::from_utf8(ler(caminho)?).map_err(|_| format!("{caminho}: não é texto"))
}

/// O que falta no catálogo para cada exigência `<id>@<versão>@<api>`; vazio se nada falta.
fn faltas(catalogo: &Catalogo, revogacoes: &Revogacoes, exigidas: &[String]) -> Vec<String> {
    let mut faltas = Vec::new();
    for exigida in exigidas {
        let partes: Vec<&str> = exigida.split('@').collect();
        let [id, versao, api] = *partes.as_slice() else {
            faltas.push(format!("«{exigida}» não é <id>@<versão>@<api>"));
            continue;
        };
        let Ok(api) = api.parse::<u32>() else {
            faltas.push(format!("«{exigida}»: a API não é número"));
            continue;
        };
        let achada = catalogo
            .mods
            .iter()
            .filter(|m| m.id == id)
            .flat_map(|m| m.versoes.iter())
            .find(|v| v.versao == versao);
        match achada {
            None => faltas.push(format!("{id} {versao} não está no catálogo")),
            Some(v) if v.api != api => {
                faltas.push(format!(
                    "{id} {versao} está com a API {}, e a exigida é {api}",
                    v.api
                ));
            }
            Some(_)
                if revogacoes
                    .mods
                    .iter()
                    .any(|r| r.id == id && r.versao == versao) =>
            {
                faltas.push(format!("{id} {versao} está REVOGADO"));
            }
            Some(_) => {}
        }
    }
    faltas
}

fn conferir(argumentos: &[String]) -> Result<String, String> {
    let (arquivos, exigencias) = argumentos.split_at(argumentos.len().min(5));
    let [catalogo, assinatura, revogacoes, assinatura_das_revogacoes, chave] = arquivos else {
        return Err("uso: conferir_catalogo <catalogo.json> <.minisig> <revogacoes.json> <.minisig> <chave.pub> [--exige <id>@<versão>@<api> ...]".to_owned());
    };
    let mut exigidas = Vec::new();
    let mut resto = exigencias.iter();
    while let Some(bandeira) = resto.next() {
        match (bandeira.as_str(), resto.next()) {
            ("--exige", Some(valor)) => exigidas.push(valor.clone()),
            _ => {
                return Err(format!(
                    "não entendi «{bandeira}»: só --exige <id>@<versão>@<api>"
                ))
            }
        }
    }
    let chave = texto(chave)?;
    let lido = seele_sessao::catalogo::ler_catalogo(&ler(catalogo)?, &texto(assinatura)?, &chave)
        .map_err(|falha| format!("catalogo.json: {falha:?}"))?;
    let retiradas = seele_sessao::catalogo::ler_revogacoes(
        &ler(revogacoes)?,
        &texto(assinatura_das_revogacoes)?,
        &chave,
    )
    .map_err(|falha| format!("revogacoes.json: {falha:?}"))?;
    let faltando = faltas(&lido, &retiradas, &exigidas);
    if !faltando.is_empty() {
        return Err(format!(
            "o catálogo é assinado e íntegro, mas NÃO tem as versões exigidas: {}",
            faltando.join("; ")
        ));
    }
    Ok(format!(
        "catálogo assinado: esquema {}, {} MODs; revogações assinadas: {} retiradas; exigidas presentes: {}",
        lido.esquema,
        lido.mods.len(),
        retiradas.mods.len(),
        if exigidas.is_empty() { "nenhuma pedida".to_owned() } else { exigidas.join(", ") }
    ))
}

fn main() -> std::process::ExitCode {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    match conferir(&argumentos) {
        Ok(relato) => {
            println!("{relato}");
            std::process::ExitCode::SUCCESS
        }
        Err(motivo) => {
            eprintln!("{motivo}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod testes {
    #![allow(clippy::unwrap_used, reason = "num teste, o pânico é o relatório")]

    use super::*;

    fn catalogo(versoes: &str) -> Catalogo {
        serde_json::from_str(&format!(
            r#"{{"esquema":1,"mods":[{{"id":"seele/mesa","versoes":[{versoes}]}}]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn o_catalogo_de_hoje_sem_a_5_0_0_reprova_mesmo_assinado() {
        // O catálogo publicado hoje: a 4.0.0 (API 6) é a última. Assinado, ele passava.
        let hoje = catalogo(r#"{"versao":"4.0.0","api":6,"hash":"h"}"#);
        let falta = faltas(
            &hoje,
            &Revogacoes::default(),
            &["seele/mesa@5.0.0@7".to_owned()],
        );
        assert_eq!(
            falta,
            vec!["seele/mesa 5.0.0 não está no catálogo".to_owned()]
        );
    }

    #[test]
    fn a_versao_exigida_com_outra_api_ou_revogada_reprova() {
        let errada = catalogo(r#"{"versao":"5.0.0","api":6,"hash":"h"}"#);
        assert!(faltas(
            &errada,
            &Revogacoes::default(),
            &["seele/mesa@5.0.0@7".to_owned()]
        )
        .first()
        .is_some_and(|falta| falta.contains("API 6")));
        let certa = catalogo(
            r#"{"versao":"4.0.0","api":6,"hash":"a"},{"versao":"5.0.0","api":7,"hash":"h"}"#,
        );
        assert!(faltas(
            &certa,
            &Revogacoes::default(),
            &["seele/mesa@5.0.0@7".to_owned()]
        )
        .is_empty());
        let revogada: Revogacoes =
            serde_json::from_str(r#"{"esquema":1,"mods":[{"id":"seele/mesa","versao":"5.0.0"}]}"#)
                .unwrap();
        assert!(
            faltas(&certa, &revogada, &["seele/mesa@5.0.0@7".to_owned()])
                .first()
                .is_some_and(|falta| falta.contains("REVOGADO"))
        );
        assert!(faltas(
            &certa,
            &Revogacoes::default(),
            &["seele/mesa-5.0.0".to_owned()]
        )
        .first()
        .is_some_and(|falta| falta.contains("não é")));
    }
}
