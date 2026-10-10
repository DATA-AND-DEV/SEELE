//! As chaves do tema no servidor, contra o vetor que a casca também lê
//! (rascunho da API 6, §9; tarefa 17j).
//!
//! O `mundo.tema` aceitava qualquer chave, e o MOD lia `ok` para um tema que a
//! casca ia recusar: o produto sabendo e não contando. A tabela que fecha isso
//! é `CHAVES_DO_TEMA`, e «a mesma da casca» quer dizer a mesma resposta a cada
//! caso de `vetores/chaves-do-tema.json` — que a bancada
//! `apps/seele-app/bancada/chaves-do-tema.cjs` confere do outro lado.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use seele_proto::mods::{
    recusa_das_chaves_do_tema, FormaDoValorDoTema, RecusaDasChavesDoTema, CHAVES_DO_TEMA,
};

fn vetor() -> serde_json::Value {
    let caminho =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vetores/chaves-do-tema.json");
    let texto = std::fs::read_to_string(&caminho).expect("ler o vetor das chaves do tema");
    serde_json::from_str(&texto).expect("o vetor das chaves do tema é JSON")
}

#[test]
fn o_servidor_responde_a_cada_caso_como_o_vetor() {
    let vetor = vetor();
    let casos = vetor["casos"].as_array().expect("`casos`");
    assert!(
        !casos.is_empty(),
        "o vetor não tem caso nenhum: o teste não conferiria nada"
    );
    for caso in casos {
        let nome = caso["nome"].as_str().expect("`nome`");
        let tema = caso["tema"].as_object().expect("`tema` é um objeto");
        let esperado = caso["recusa"].as_object().map(|recusa| {
            (
                recusa["erro"].as_str().expect("`erro`").to_owned(),
                recusa["chave"].as_str().expect("`chave`").to_owned(),
                recusa
                    .get("regiao")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            )
        });
        let lido = recusa_das_chaves_do_tema(tema).map(
            |RecusaDasChavesDoTema {
                 erro,
                 chave,
                 regiao,
             }| (erro.to_owned(), chave, regiao),
        );
        assert_eq!(lido, esperado, "{nome}: o servidor e o vetor divergem");
    }
}

/// **Todo nome da tabela aparece aceito em algum caso**, e toda escolha
/// também. Sem isto, uma chave somada à tabela e esquecida no vetor seria uma
/// chave que nenhum dos dois lados confere contra o outro.
#[test]
fn o_vetor_cobre_a_tabela_inteira() {
    let vetor = vetor();
    let mut aceitos: BTreeSet<(String, String)> = BTreeSet::new();
    for caso in vetor["casos"].as_array().expect("`casos`") {
        if !caso["recusa"].is_null() {
            continue;
        }
        for (chave, valor) in caso["tema"].as_object().expect("`tema`") {
            aceitos.insert((
                chave.clone(),
                valor.as_str().map(str::to_owned).unwrap_or_default(),
            ));
        }
    }
    for (nome, forma) in CHAVES_DO_TEMA {
        assert!(
            aceitos.iter().any(|(chave, _)| chave == nome),
            "«{nome}» está na tabela e nenhum caso aceito do vetor o usa"
        );
        if matches!(forma, FormaDoValorDoTema::Regioes) {
            regioes_cobertas(&vetor);
        }
        if let FormaDoValorDoTema::Escolha(escolhas) = forma {
            for escolha in *escolhas {
                assert!(
                    aceitos.contains(&((*nome).to_owned(), (*escolha).to_owned())),
                    "«{nome}: {escolha}» está na tabela e nenhum caso aceito do vetor o usa"
                );
            }
        }
    }
}

/// **Nenhum nome da tabela é um nome de CSS.** O MOD escreve nomes do
/// contrato, e o produto os traduz; um `--` aqui seria a porta para o MOD
/// escrever um token do produto — ou um `--nucleo-*` — pelo nome dele.
#[test]
fn nenhuma_chave_da_tabela_e_um_nome_de_css() {
    for (nome, _) in CHAVES_DO_TEMA {
        assert!(
            !nome.starts_with('-'),
            "«{nome}» na tabela das chaves do tema"
        );
    }
}

/// **Com mais de uma chave errada, o servidor nomeia a primeira em ordem
/// alfabética**, e não a primeira que o MOD escreveu: o `serde_json` deste
/// workspace guarda o objeto num `BTreeMap` (sem `preserve_order`). Se alguém
/// ligar o `preserve_order`, a resposta passa a depender da ordem do MOD e este
/// teste reprova. A casca ordena do mesmo jeito, e o vetor tem os casos que
/// provam isso nas duas pontas.
#[test]
fn com_mais_de_uma_chave_errada_o_servidor_nomeia_a_primeira_em_ordem_alfabetica() {
    let tema: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r##"{"zeta":"#000000","alfa":"#000000"}"##).expect("JSON");
    assert_eq!(
        recusa_das_chaves_do_tema(&tema),
        Some(RecusaDasChavesDoTema {
            erro: "chave-desconhecida",
            chave: "alfa".to_owned(),
            regiao: None,
        })
    );
}

/// **Cada região e cada cor de região aparecem aceitas em algum caso** — a
/// mesma regra das escolhas, para a forma `Regioes`.
fn regioes_cobertas(vetor: &serde_json::Value) {
    let mut aceitas: BTreeSet<(String, String)> = BTreeSet::new();
    for caso in vetor["casos"].as_array().expect("`casos`") {
        if !caso["recusa"].is_null() {
            continue;
        }
        let Some(regioes) = caso["tema"]
            .get("regioes")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for (regiao, cores) in regioes {
            for cor in cores.as_object().into_iter().flat_map(|cores| cores.keys()) {
                aceitas.insert((regiao.clone(), cor.clone()));
            }
        }
    }
    for regiao in seele_proto::mods::REGIOES_DO_TEMA {
        for cor in seele_proto::mods::CORES_DA_REGIAO {
            assert!(
                aceitas.contains(&((*regiao).to_owned(), (*cor).to_owned())),
                "«{regiao}: {cor}» está na tabela e nenhum caso aceito do vetor o usa"
            );
        }
    }
}
