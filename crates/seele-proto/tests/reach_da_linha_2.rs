//! **O teto do `reach`, na linha 2: o anúncio e a instalação dizem o mesmo.**
//!
//! O MESA 4.0.0 publicado declara itens de 43 e 34 bytes (`vetores/
//! mesa-4.0.0.mod.json`, o `mod.json` do commit `8e78e091` do repositório do
//! MESA, conferido contra o catálogo publicado). Com o teto de 32 por item, a
//! 0.17 o instalava — o `read_manifest` não olhava o `reach` — e recusava o
//! anúncio do servidor que o ligasse (`check_anuncio`): um servidor que sobe e
//! não deixa ninguém entrar.
//!
//! O teto passou a 48 bytes UTF-8 por item, só na linha 2 (esta build não fala
//! outra: `VERSAO_MINIMA` é o primeiro da linha 2). Os 16 itens e o quadro de
//! 16384 bytes ficam: o anúncio cheio, 12 MODs com 16 itens de 48, dá 14948
//! bytes. E o que o anúncio recusa a instalação recusa antes, com o nome.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use seele_proto::control::{
    decode, encode, ControlError, ServerMessage, MAX_FRAME_LEN, MAX_MODS_NO_ANUNCIO,
    MAX_MOD_ID_LEN, MAX_MOD_REPO_LEN, MAX_MOD_VERSION_LEN, MAX_REACH_ITENS, MAX_REACH_LEN,
};
use seele_proto::mods::{hex, identidade_do_conjunto, read_manifest, ModAnunciado, Refused};

const MESA: &str = include_str!("vetores/mesa-4.0.0.mod.json");

fn um_mod(id: &str, n: u8, reach: Vec<String>) -> ModAnunciado {
    ModAnunciado {
        id: id.to_owned(),
        version: "4.0.0".to_owned(),
        hash: format!("{n:02x}").repeat(32),
        repo: "https://github.com/DATA-AND-DEV/MESA".to_owned(),
        reach,
        no_servidor: true,
        api: 6,
        tema: false,
    }
}

fn anuncio(mut mods: Vec<ModAnunciado>) -> ServerMessage {
    let conjunto = hex(&identidade_do_conjunto(&mut mods));
    ServerMessage::ModsExigidos { mods, conjunto }
}

/// O anúncio de um MOD com este `reach`: ida e volta pelo fio.
fn o_anuncio_passa(reach: &[String]) -> Result<(), ControlError> {
    let quadro = encode(&anuncio(vec![um_mod("seele/mesa", 0x4a, reach.to_vec())]))?;
    let _: ServerMessage = decode(&quadro)?;
    Ok(())
}

/// O manifesto mínimo com este `reach`, pela leitura da instalação.
fn o_manifesto_com(reach: &[String]) -> Result<(), Refused> {
    // A API lida da constante, e não escrita: o guarda
    // `nenhum_manifesto_de_teste_crava_a_versao_da_api` (seele-conformance).
    let manifesto = serde_json::json!({
        "schema": 1, "id": "seele/mesa", "version": "4.0.0", "api": seele_proto::mods::MOD_API_VERSION,
        "repo": "https://github.com/DATA-AND-DEV/MESA",
        "reach": reach, "client": "cliente/main.js",
    });
    read_manifest(&manifesto.to_string()).map(|_| ())
}

/// `n` bytes UTF-8 que terminam num caractere de dois bytes (`ç`), na fronteira.
fn com_multibyte_no_fim(n: usize) -> String {
    let texto = format!("{}ç", "a".repeat(n - 2));
    assert_eq!(texto.len(), n);
    texto
}

#[test]
fn o_teto_por_item_da_linha_2_e_48_bytes() {
    assert_eq!(MAX_REACH_LEN, 48);
    assert_eq!(MAX_REACH_ITENS, 16);
    assert_eq!(MAX_FRAME_LEN, 16384);
}

#[test]
fn o_reach_real_do_mesa_4_0_0_instala_e_anuncia() {
    let manifesto = read_manifest(MESA).expect("a 0.17 instala o MESA 4.0.0 publicado");
    let maior = manifesto.reach.iter().map(String::len).max().unwrap();
    assert_eq!(maior, 43, "o vetor deixou de ser o do MESA 4.0.0");
    assert!(manifesto.reach.iter().any(|item| item.len() == 34));
    o_anuncio_passa(&manifesto.reach).expect("a 0.17 anuncia o MESA 4.0.0 publicado");
}

#[test]
fn quarenta_e_oito_bytes_passam_e_quarenta_e_nove_nao_contando_bytes_utf8() {
    for item in ["a".repeat(48), com_multibyte_no_fim(48)] {
        assert_eq!(
            o_anuncio_passa(std::slice::from_ref(&item)),
            Ok(()),
            "{item}"
        );
        assert_eq!(
            o_manifesto_com(std::slice::from_ref(&item)),
            Ok(()),
            "{item}"
        );
    }
    for item in ["a".repeat(49), com_multibyte_no_fim(49)] {
        // 48 caracteres, 49 bytes: conta o byte, e não o caractere.
        assert_eq!(
            o_anuncio_passa(std::slice::from_ref(&item)),
            Err(ControlError::FieldTooLong {
                field: "reach",
                len: 49,
                limit: 48
            }),
            "{item}"
        );
        assert_eq!(
            o_manifesto_com(&["curto".to_owned(), item.clone()]),
            Err(Refused::AlcanceGrandeDemais {
                posicao: 1,
                tamanho: 49,
                teto: 48
            }),
            "{item}"
        );
    }
}

#[test]
fn dezessete_itens_sao_recusados_no_anuncio_e_na_instalacao() {
    let dezesseis = vec!["a".to_owned(); 16];
    assert_eq!(o_anuncio_passa(&dezesseis), Ok(()));
    assert_eq!(o_manifesto_com(&dezesseis), Ok(()));
    let dezessete = vec!["a".to_owned(); 17];
    assert_eq!(
        o_anuncio_passa(&dezessete),
        Err(ControlError::FieldTooLong {
            field: "reach",
            len: 17,
            limit: 16
        })
    );
    assert_eq!(
        o_manifesto_com(&dezessete),
        Err(Refused::AlcancesDemais {
            quantos: 17,
            teto: 16
        })
    );
}

/// O quadro global continua protegido: o anúncio cheio cabe, medido, e um
/// anúncio com MODs a mais é recusado.
#[test]
fn o_anuncio_cheio_da_linha_2_da_14948_bytes_e_cabe_no_quadro() {
    let cheio: Vec<ModAnunciado> = (0..MAX_MODS_NO_ANUNCIO)
        .map(|n| ModAnunciado {
            id: format!("{:0>width$}/{n:03}", "a", width = MAX_MOD_ID_LEN - 5),
            version: "9".repeat(MAX_MOD_VERSION_LEN),
            hash: format!("{n:02x}").repeat(32),
            repo: "r".repeat(MAX_MOD_REPO_LEN),
            reach: vec!["a".repeat(MAX_REACH_LEN); MAX_REACH_ITENS],
            no_servidor: true,
            api: 0,
            tema: false,
        })
        .collect();
    let quadro = encode(&anuncio(cheio.clone())).expect("o anúncio cheio sai");
    assert_eq!(quadro.len(), 14948);
    assert!(quadro.len() <= MAX_FRAME_LEN);

    let mut demais = cheio;
    demais.push(um_mod("seele/sobra", 0xee, vec![]));
    assert!(matches!(
        encode(&anuncio(demais)),
        Err(ControlError::FieldTooLong { field: "mods", .. })
    ));
}

/// **A instalação recusa o que o anúncio recusaria, e aceita o que ele
/// aceita**, numa tabela só: nenhum `reach` passa por uma porta e para na
/// outra.
#[test]
fn a_instalacao_recusa_o_mesmo_que_o_anuncio() {
    let casos: Vec<Vec<String>> = vec![
        vec![],
        vec!["a".repeat(47)],
        vec!["a".repeat(48)],
        vec!["a".repeat(49)],
        vec![com_multibyte_no_fim(48)],
        vec![com_multibyte_no_fim(49)],
        vec!["x".to_owned(); 16],
        vec!["x".to_owned(); 17],
        vec!["a".repeat(48); 16],
        vec!["a".repeat(200)],
        read_manifest(MESA).unwrap().reach,
    ];
    for reach in casos {
        assert_eq!(
            o_anuncio_passa(&reach).is_ok(),
            o_manifesto_com(&reach).is_ok(),
            "o anúncio e a instalação discordam de {reach:?}"
        );
    }
}
