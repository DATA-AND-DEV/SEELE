use super::{
    midia_declarada_do_mod, motivo_de, som_declarado_do_mod, som_servido_dito, BaldesDoRegistro,
    TETO_DA_FRASE_NO_REGISTRO,
};
use crate::pacote_de_teste::{pasta, publicar, ID, PNG, TEXTO, WAV};
use crate::rastro_de_teste::capturar;
use crate::relogio_de_teste::origem;

/// **O nome do som servido não escreve uma segunda linha no registro.**
///
/// Quando o som sai, o caminho já passou pelo manifesto, e o manifesto só
/// confere que cada pedaço dele é um nome (`inner_path`): uma quebra de
/// linha passa. Escrito cru, um MOD que declarasse
/// `som/a\nWARN seele_app: forjada.wav` escreveria, a cada som servido, uma
/// linha com a cara do produto. O caminho vem aqui em texto, e não num
/// pacote no disco, porque um nome de arquivo com `\n` não existe no
/// Windows.
#[test]
fn um_caminho_com_quebra_de_linha_nao_forja_linha_no_som_servido() {
    let ((), rastro) = capturar(|| {
        som_servido_dito(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            7,
            "som/a\nWARN seele_app: forjada.wav",
            16,
        )
    });
    assert_eq!(
        rastro.lines().count(),
        1,
        "o nome que o MOD declarou escreveu uma segunda linha no seele.log, com a cara do \
             produto: {rastro}"
    );
    assert!(
        rastro.contains("INFO")
            && rastro.contains("som de MOD servido em bytes")
            && rastro.contains("mod_id=prova/midia")
            && rastro.contains(r#"caminho="som/a\nWARN seele_app: forjada.wav""#),
        "a linha do som servido perdeu o id, a frase ou o caminho entre aspas, e quem lê o \
             registro já não acha o que o Rust serviu: {rastro}"
    );
}

/// **O nome do som servido tem o teto das outras portas de texto de
/// terceiro.** As aspas impedem a linha forjada, e não a linha sem fim.
#[test]
fn um_caminho_enorme_sai_cortado_no_teto_de_frase_no_som_servido() {
    let ((), rastro) = capturar(|| {
        som_servido_dito(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            7,
            &"s".repeat(TETO_DA_FRASE_NO_REGISTRO * 4),
            16,
        );
    });
    assert!(
        rastro.contains(&format!(
            "caminho=\"{}\"",
            "s".repeat(TETO_DA_FRASE_NO_REGISTRO)
        )),
        "o nome que o MOD declarou entrou no registro sem o teto das outras portas de \
             texto de terceiro: {} caracteres numa linha",
        rastro.chars().count()
    );
}

/// **Os dois comandos que servem o pacote dizem o que serviram pela função
/// que os testes medem**: `som_do_mod` por `som_servido_dito` (acima), e
/// `midia_do_mod` por `midia_servida_dita` (no módulo da mídia recusada).
/// Uma linha escrita à mão dentro de um deles deixaria esses testes verdes
/// e o caminho cru no registro de novo.
#[test]
fn os_comandos_de_midia_dizem_o_que_serviram_pela_funcao_medida() {
    let fonte = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("main.rs legível")
    .replace("\r\n", "\n");
    for (comando, funcao) in [
        ("fn som_do_mod(", "som_servido_dito("),
        ("fn midia_do_mod(", "midia_servida_dita("),
    ] {
        let corpo = fonte
            .split(comando)
            .nth(1)
            .and_then(|resto| resto.split("\n}\n").next())
            .unwrap_or_else(|| panic!("`{comando}` sumiu de main.rs"));
        let codigo: Vec<&str> = corpo
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect();
        let codigo = codigo.join("\n");
        assert!(
            codigo.contains(funcao) && !codigo.contains("tracing::"),
            "`{comando}` escreve o que serviu por fora de `{funcao}`, e o caminho que o MOD \
                 declarou pode voltar cru ao registro"
        );
    }
}

#[test]
fn um_som_declarado_sai_com_os_bytes_exatos() {
    let (raiz, hash) = publicar(&[("som/toque.wav", WAV)]);
    let bytes = som_declarado_do_mod(
        &BaldesDoRegistro::default(),
        origem(),
        &pasta(&raiz),
        ID,
        &hash,
        "som/toque.wav",
    )
    .expect("um som declarado foi recusado");
    assert_eq!(
        bytes, WAV,
        "os bytes do som mudaram no caminho, e a casca decodificaria outra coisa"
    );
}

/// **O som do pacote atravessa a ponte uma vez só** (P49-7b).
///
/// `montarMidia` pede `midia_do_mod` para saber o papel e os bytes a
/// contar, e o som em si vem cru por `som_do_mod`. O `data:` que
/// `midia_do_mod` montava para um som não tinha quem o lesse: o arquivo
/// era lido duas vezes, e uma delas atravessava a ponte em base64, um terço
/// maior, em JSON. A imagem continua com o `data:`, que é o que o `<img>`
/// mostra. O que se mede é o que atravessa a ponte: a resposta serializada.
#[test]
fn o_som_do_pacote_sai_sem_uri_e_a_imagem_com() {
    let (raiz, hash) = publicar(&[("som/toque.wav", WAV), ("img/rosto.png", PNG)]);
    let pela_ponte = |caminho: &str| {
        let (resultado, _) = capturar(|| {
            midia_declarada_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                caminho,
                None,
            )
        });
        let midia = resultado
            .unwrap_or_else(|falha| panic!("`{caminho}` foi recusado: {}", motivo_de(&falha)));
        match serde_json::to_value(&midia) {
            Ok(serde_json::Value::Object(mapa)) => mapa,
            outro => panic!("a mídia não atravessa a ponte como objeto: {outro:?}"),
        }
    };
    let som = pela_ponte("som/toque.wav");
    assert!(
        !som.contains_key("uri"),
        "o som do pacote voltou a atravessar a ponte em base64, no `uri` de `midia_do_mod`, \
             além de cru por `som_do_mod`: o arquivo é lido duas vezes, e ninguém lê esse \
             `data:` — {som:?}"
    );
    assert_eq!(
        (som.get("papel"), som.get("bytes")),
        (
            Some(&serde_json::json!("som")),
            Some(&serde_json::json!(WAV.len()))
        ),
        "o som do pacote saiu sem o papel ou sem os bytes, e `montarMidia` não sabe que \
             elemento montar nem quanto contar no teto: {som:?}"
    );
    let imagem = pela_ponte("img/rosto.png");
    assert!(
        imagem
            .get("uri")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|uri| uri.starts_with("data:image/png;base64,")),
        "a imagem do pacote saiu sem o `data:` que os bytes provaram, e o `<img>` da \
             região fica sem o que mostrar: {imagem:?}"
    );
}

#[test]
fn uma_imagem_pedida_como_som_e_recusada_pelo_nome_e_dita() {
    let (raiz, hash) = publicar(&[("img/rosto.png", PNG)]);
    let (resultado, rastro) = capturar(|| {
        som_declarado_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "img/rosto.png",
        )
    });
    let falha = resultado.expect_err("uma imagem saiu pelo caminho do som");
    assert_eq!(
        motivo_de(&falha),
        "nao-e-som",
        "a recusa não diz que o arquivo não é som, e o autor procuraria no lugar errado"
    );
    assert!(
        rastro.lines().any(|linha| linha.contains("WARN")
            && linha.contains("mod_id=prova/midia")
            && linha.contains("motivo=nao-e-som")),
        "a recusa do som não chegou ao registro: {rastro}"
    );
}

/// Dentro de `capturar`, como toda chamada de teste que passa por uma
/// linha que outro teste captura.
#[test]
fn um_som_que_o_manifesto_nao_declara_nao_sai() {
    let (raiz, hash) = publicar(&[("som/toque.wav", WAV)]);
    let (resultado, rastro) = capturar(|| {
        som_declarado_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "som/escondido.wav",
        )
    });
    let falha = resultado.expect_err("um som que o manifesto não declara saiu");
    assert_eq!(
        motivo_de(&falha),
        "arquivo-nao-declarado",
        "o som fora do manifesto foi recusado por outro motivo"
    );
    assert!(
        rastro.lines().any(|linha| linha.contains("WARN")
            && linha.contains("mod_id=prova/midia")
            && linha.contains("motivo=arquivo-nao-declarado")),
        "a recusa do som fora do manifesto não chegou ao registro: {rastro}"
    );
}

/// **Bytes que não são de formato nenhum, pedidos como som, são recusados
/// pelo nome e ditos.** É o braço `None` de `som_declarado_do_mod`: sem
/// ele, o texto sairia como som e viraria um `decodeAudioData` que falha
/// sem dizer por quê.
#[test]
fn um_texto_pedido_como_som_e_recusado_como_formato_desconhecido_e_dito() {
    let (raiz, hash) = publicar(&[("som/x.wav", TEXTO)]);
    let (resultado, rastro) = capturar(|| {
        som_declarado_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "som/x.wav",
        )
    });
    let falha = resultado.expect_err("texto saiu pelo caminho do som");
    assert_eq!(
        motivo_de(&falha),
        "formato-desconhecido",
        "texto pedido como som foi recusado por outro motivo, e o autor procuraria no \
             lugar errado"
    );
    assert!(
        rastro.lines().any(|linha| linha.contains("WARN")
            && linha.contains("mod_id=prova/midia")
            && linha.contains("motivo=formato-desconhecido")),
        "a recusa do som de formato desconhecido não chegou ao registro: {rastro}"
    );
}
