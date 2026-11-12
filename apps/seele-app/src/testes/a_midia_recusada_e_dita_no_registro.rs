use super::{
    imagem_lida_dita, midia_declarada_do_mod, midia_do_servidor_dita, midia_servida_dita,
    motivo_de, recusa_de_midia_dita, registrar_da_janela_em, BaldesDoRegistro, FalhaNoMod,
    TETO_DA_FRASE_NO_REGISTRO, TETO_DO_ID_NO_REGISTRO,
};
use crate::pacote_de_teste::{pasta, publicar, ID, PNG, TEXTO};
use crate::rastro_de_teste::capturar;
use crate::relogio_de_teste::origem;

/// A linha de recusa de mídia no rastro, se houver.
fn recusa(rastro: &str) -> Option<&str> {
    rastro
        .lines()
        .find(|linha| linha.contains("mídia de MOD recusada"))
}

/// Confere os pedaços que fazem uma linha de recusa servir a quem lê.
fn traz(linha: &str, pedacos: &[&str]) {
    for pedaco in pedacos {
        assert!(
            linha.contains(pedaco),
            "a linha de recusa não traz `{pedaco}`, e é por ela que quem lê o registro \
                 acha o MOD e o motivo: {linha}"
        );
    }
}

#[test]
fn um_arquivo_que_o_manifesto_nao_declara_sai_como_warn_com_o_id_e_o_motivo() {
    let (raiz, hash) = publicar(&[("img/rosto.png", PNG)]);
    let (resultado, rastro) = capturar(|| {
        midia_declarada_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "img/escondido.png",
            None,
        )
    });
    let falha = resultado
        .err()
        .expect("um arquivo que o manifesto não declara foi servido");
    assert_eq!(
        motivo_de(&falha),
        "arquivo-nao-declarado",
        "a recusa mudou de nome, e a frase da janela deixa de achá-la"
    );
    let linha = recusa(&rastro).unwrap_or_else(|| {
        panic!(
            "a recusa não chegou ao registro: o MOD fica sem imagem e o seele.log sem \
                 uma palavra — {rastro}"
        )
    });
    traz(
        linha,
        &[
            "WARN",
            "mod_id=prova/midia",
            "motivo=arquivo-nao-declarado",
            "origem=\"img/escondido.png\"",
        ],
    );
}

#[test]
fn um_arquivo_que_nao_e_midia_sai_como_warn_com_o_motivo() {
    let (raiz, hash) = publicar(&[("doc/leia.txt", TEXTO)]);
    let (resultado, rastro) = capturar(|| {
        midia_declarada_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "doc/leia.txt",
            None,
        )
    });
    let falha = resultado.err().expect("texto foi servido como mídia");
    assert_eq!(
        motivo_de(&falha),
        "formato-desconhecido",
        "texto foi recusado por outro motivo, e o autor procuraria no lugar errado"
    );
    let linha =
        recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
    traz(
        linha,
        &["WARN", "mod_id=prova/midia", "motivo=formato-desconhecido"],
    );
}

/// Um PNG liso de verdade, com as medidas pedidas: comprime a quase nada.
fn png_de(largura: u32, altura: u32) -> Vec<u8> {
    use image::ImageEncoder as _;
    let quadro = image::RgbaImage::from_pixel(largura, altura, image::Rgba([40, 80, 120, 255]));
    let mut saida = Vec::new();
    image::codecs::png::PngEncoder::new(&mut saida)
        .write_image(&quadro, largura, altura, image::ExtendedColorType::Rgba8)
        .expect("gravar a imagem de teste");
    saida
}

/// As medidas da imagem que um `data:` carrega.
fn medidas_do_uri(uri: &str) -> (u32, u32) {
    let (_, conteudo) = uri.split_once(',').expect("um data:");
    let bytes = seele_ffi::de_base64(conteudo).expect("base64");
    seele_ffi::reducao::medidas(&bytes).expect("uma imagem")
}

/// **A imagem do pacote de um MOD vai à janela reduzida ao lugar em que
/// aparece** (a varredura do incidente da capa em 4K, 11/11/2026).
///
/// Só a mídia do servidor que nomeava uso era reduzida; a do pacote ia do
/// tamanho que o MOD a trouxe, como um `data:` de até 10 MiB. Sem uso, o
/// teto geral da mídia de MOD; como fundo de uma caixa, o da capa de um
/// perfil. A que já cabe sai com os bytes que tinha.
#[test]
fn a_imagem_do_pacote_vai_a_janela_reduzida() {
    let grande = png_de(3840, 2160);
    let pequena = png_de(640, 360);
    let (raiz, hash) = publicar(&[("img/4k.png", &grande), ("img/pequena.png", &pequena)]);
    let pedir = |caminho: &str, uso: &str| {
        let (resultado, rastro) = capturar(|| {
            midia_declarada_do_mod(
                &BaldesDoRegistro::default(),
                origem(),
                &pasta(&raiz),
                ID,
                &hash,
                caminho,
                crate::reducao::teto_do_uso(uso).as_ref(),
            )
        });
        assert!(
            recusa(&rastro).is_none(),
            "uma recusa foi escrita: {rastro}"
        );
        resultado.unwrap_or_else(|falha| panic!("`{caminho}` recusado: {}", motivo_de(&falha)))
    };
    let sem_uso = pedir("img/4k.png", "");
    let uri = sem_uso.uri.as_deref().expect("a imagem sai com o data:");
    assert_eq!(
        medidas_do_uri(uri),
        (1920, 1080),
        "a imagem em 4K do pacote foi à janela do tamanho que veio"
    );
    let fundo = pedir("img/4k.png", "fundo");
    assert_eq!(
        medidas_do_uri(fundo.uri.as_deref().expect("o data:")),
        (1280, 720),
        "o fundo de uma caixa não chegou com o teto dele"
    );
    let cabia = pedir("img/pequena.png", "");
    assert_eq!(
        cabia.bytes,
        pequena.len(),
        "a imagem que cabia foi regravada"
    );
}

/// **A imagem do pacote que declara pixels demais é recusada pelo nome,
/// com os números, e a recusa vai ao registro.** O arquivo cabe no teto de
/// bytes (uma cor só comprime a quase nada) e aberto pediria centenas de
/// megabytes à janela; ela saía daqui inteira, para o `decode()` de lá.
#[test]
fn a_imagem_do_pacote_com_pixels_demais_e_recusada_e_dita() {
    let bomba = png_de(9000, 5000);
    assert!(bomba.len() < seele_ffi::mods::TETO_DE_MIDIA);
    let (raiz, hash) = publicar(&[("img/bomba.png", &bomba)]);
    let (resultado, rastro) = capturar(|| {
        midia_declarada_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "img/bomba.png",
            None,
        )
    });
    let falha = resultado
        .err()
        .expect("uma imagem de 45 megapixels foi servida à janela");
    let motivo = motivo_de(&falha);
    assert!(
        motivo.starts_with("imagem-grande-demais-decodificada")
            && motivo.contains("9000 × 5000")
            && motivo.contains("36000000"),
        "a recusa não diz o nome, as medidas e o teto: {motivo}"
    );
    let linha = recusa(&rastro).unwrap_or_else(|| {
            panic!("a recusa não chegou ao registro: o MOD fica sem imagem e o seele.log sem uma palavra — {rastro}")
        });
    traz(
        linha,
        &[
            "WARN",
            "mod_id=prova/midia",
            "imagem-grande-demais-decodificada",
            "origem=\"img/bomba.png\"",
        ],
    );
}

#[test]
fn uma_imagem_aceita_nao_escreve_recusa() {
    let (raiz, hash) = publicar(&[("img/rosto.png", PNG)]);
    let (resultado, rastro) = capturar(|| {
        midia_declarada_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            "img/rosto.png",
            None,
        )
    });
    let midia = resultado.expect("uma imagem declarada foi recusada");
    assert_eq!(midia.papel, "imagem", "um PNG saiu com outro papel");
    assert!(
        recusa(&rastro).is_none(),
        "uma mídia aceita escreveu uma recusa, e o registro ensinaria a ignorá-las: {rastro}"
    );
}

/// **O nome da mídia servida não escreve uma segunda linha no registro.**
///
/// O mesmo vetor da linha do som servido: o manifesto só confere que cada
/// pedaço do caminho é um nome (`inner_path`), e uma quebra de linha passa.
/// O caminho vem em texto, e não num pacote no disco, porque um nome de
/// arquivo com `\n` não existe no Windows.
#[test]
fn um_caminho_com_quebra_de_linha_nao_forja_linha_na_midia_servida() {
    let ((), rastro) = capturar(|| {
        midia_servida_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            7,
            "img/a\nWARN seele_app: forjada.png",
            "imagem",
            8,
        );
    });
    assert_eq!(
        rastro.lines().count(),
        1,
        "o nome que o MOD declarou escreveu uma segunda linha no seele.log, com a cara do \
             produto: {rastro}"
    );
    for pedaco in [
        "INFO",
        "mídia de MOD servida",
        "mod_id=prova/midia",
        r#"caminho="img/a\nWARN seele_app: forjada.png""#,
        "papel=\"imagem\"",
    ] {
        assert!(
            rastro.contains(pedaco),
            "a linha da mídia servida não traz `{pedaco}`, e quem lê o registro já não acha \
                 o que o Rust serviu: {rastro}"
        );
    }
}

/// **O nome da mídia servida tem o teto das outras portas de texto de
/// terceiro.** As aspas impedem a linha forjada, e não a linha sem fim.
#[test]
fn um_caminho_enorme_sai_cortado_no_teto_de_frase_na_midia_servida() {
    let ((), rastro) = capturar(|| {
        midia_servida_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            7,
            &"i".repeat(TETO_DA_FRASE_NO_REGISTRO * 4),
            "imagem",
            8,
        );
    });
    assert!(
        rastro.contains(&format!(
            "caminho=\"{}\"",
            "i".repeat(TETO_DA_FRASE_NO_REGISTRO)
        )),
        "o nome que o MOD declarou entrou no registro sem o teto das outras portas de \
             texto de terceiro: {} caracteres numa linha",
        rastro.chars().count()
    );
}

#[test]
fn a_sessao_que_acabou_nao_vira_aviso() {
    let (falha, rastro) = capturar(|| {
        recusa_de_midia_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            "img/rosto.png",
            FalhaNoMod::Recusado {
                motivo: "sessao-encerrada".to_owned(),
            },
        )
    });
    assert_eq!(
        motivo_de(&falha),
        "sessao-encerrada",
        "a recusa mudou no caminho"
    );
    assert!(
        !rastro.contains("WARN") && rastro.contains("DEBUG"),
        "uma mídia que não montou porque a pessoa saiu virou aviso — é o desfecho \
             certo, e não um defeito: {rastro}"
    );
}

#[test]
fn a_midia_do_servidor_recusada_leva_o_id_do_mod() {
    let (resultado, rastro) = capturar(|| {
        midia_do_servidor_dita(
            &BaldesDoRegistro::default(),
            origem(),
            "seele/perfis",
            "data:image/png;base64,%%%",
            None,
        )
    });
    let falha = resultado.err().expect("base64 inválido virou mídia");
    assert_eq!(
        motivo_de(&falha),
        "nao-e-base64",
        "o base64 inválido foi recusado por outro motivo"
    );
    let linha =
        recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
    traz(
        linha,
        &[
            "WARN",
            "mod_id=seele/perfis",
            "motivo=nao-e-base64",
            "origem=\"servidor\"",
        ],
    );
}

/// Uma recusa de imagem por volume, com o motivo que o servidor mandou.
///
/// É o caminho de `ler_imagem_mod`, pela mesma função que o comando chama
/// ([`imagem_lida_dita`]): o `Err` da conexão é o texto do par remoto (o
/// `erro` da `RespostaDeImagem`, a razão com que a conexão fechou, o prazo
/// esgotado), e vai à recusa como veio. Chamar `recusa_de_midia_dita`
/// direto aqui mediria uma cópia do caminho, e os testes que passam por
/// este ajudante ficariam verdes com o comando calado.
fn recusa_do_servidor(motivo: &str) -> (FalhaNoMod, String) {
    let (lida, rastro) = capturar(|| {
        imagem_lida_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            Err(motivo.to_owned()),
            None,
        )
    });
    (
        lida.err()
            .expect("a recusa do servidor virou imagem, e não há recusa a medir"),
        rastro,
    )
}

#[test]
fn o_texto_do_servidor_no_motivo_nao_forja_linha() {
    let forjado = "x\n2026-09-30T00:00:00.000000Z  WARN seele_app: linha forjada pelo servidor";
    let (falha, rastro) = recusa_do_servidor(forjado);
    assert_eq!(
        motivo_de(&falha),
        forjado,
        "a recusa que volta à janela foi mexida: quem se protege é a linha do registro, \
             e a janela continua recebendo o que o servidor disse"
    );
    assert_eq!(
        rastro.lines().count(),
        1,
        "texto do servidor no motivo escreveu no seele.log mais de uma linha, e a \
             segunda tem a cara do produto: {rastro}"
    );
    assert!(
        rastro.contains("linha forjada pelo servidor"),
        "o que o servidor disse sumiu do registro, e era o dado da recusa: {rastro}"
    );
}

#[test]
fn o_id_que_a_janela_manda_nao_forja_linha() {
    let (_, rastro) = capturar(|| {
        recusa_de_midia_dita(
            &BaldesDoRegistro::default(),
            origem(),
            "prova/midia\n2026-09-30T00:00:00.000000Z  WARN seele_app: linha forjada pela janela",
            "servidor",
            FalhaNoMod::Recusado {
                motivo: "nao-e-base64".to_owned(),
            },
        )
    });
    assert_eq!(
        rastro.lines().count(),
        1,
        "o id do MOD que a janela manda escreveu no seele.log mais de uma linha, e a \
             segunda tem a cara do produto: {rastro}"
    );
}

#[test]
fn o_motivo_em_texto_livre_sai_entre_aspas() {
    let prazo = "Prazo da transferência esgotado. Confira a conexão e a versão do servidor.";
    let (_, rastro) = recusa_do_servidor(prazo);
    let linha =
        recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
    traz(
        linha,
        &["WARN", "mod_id=prova/midia", &format!("motivo=\"{prazo}\"")],
    );
}

/// **A imagem que o servidor recusou é dita**, com o id e o texto dele.
///
/// É a primeira das duas recusas de `ler_imagem_mod` depois do `await`, e
/// a que vem com o que o servidor mandou — o prazo esgotado, o `erro` da
/// resposta, a razão com que a conexão fechou.
#[test]
fn a_imagem_recusada_pelo_servidor_sai_como_warn_com_o_id_e_o_motivo() {
    let (resultado, rastro) = capturar(|| {
        imagem_lida_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            Err("Prazo esgotado".to_owned()),
            None,
        )
    });
    let falha = resultado.err().expect("a recusa do servidor virou imagem");
    assert_eq!(
        motivo_de(&falha),
        "Prazo esgotado",
        "a recusa do servidor voltou à janela mexida, e a janela deixa de dizer o que ele \
             disse"
    );
    let linha = recusa(&rastro).unwrap_or_else(|| {
        panic!(
            "a imagem que o servidor recusou não chegou ao registro: o MOD fica sem imagem \
                 e o seele.log sem uma palavra — {rastro}"
        )
    });
    traz(
        linha,
        &[
            "WARN",
            "mod_id=prova/midia",
            "motivo=\"Prazo esgotado\"",
            "origem=\"volume\"",
        ],
    );
}

/// **A imagem lida que não é mídia é dita**, pelo nome da recusa.
///
/// A segunda recusa de `ler_imagem_mod` depois do `await`: o servidor
/// entregou, e os bytes não são de um formato que este produto decodifica.
#[test]
fn a_imagem_lida_que_nao_e_midia_sai_como_warn_com_o_motivo() {
    let (resultado, rastro) = capturar(|| {
        imagem_lida_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            Ok(TEXTO.to_vec()),
            None,
        )
    });
    let falha = resultado.err().expect("texto lido por volume virou imagem");
    assert_eq!(
        motivo_de(&falha),
        "formato-desconhecido",
        "texto lido por volume foi recusado por outro motivo, e o autor procuraria no lugar \
             errado"
    );
    let linha = recusa(&rastro).unwrap_or_else(|| {
        panic!(
            "a imagem lida que não é mídia não chegou ao registro: o MOD fica sem imagem e \
                 o seele.log sem uma palavra — {rastro}"
        )
    });
    traz(
        linha,
        &[
            "WARN",
            "mod_id=prova/midia",
            "motivo=formato-desconhecido",
            "origem=\"volume\"",
        ],
    );
}

/// **A imagem lida que é mídia passa sem linha**: uma recusa só é dita
/// quando há recusa.
#[test]
fn a_imagem_lida_aceita_nao_escreve_linha() {
    let (resultado, rastro) = capturar(|| {
        imagem_lida_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            Ok(PNG.to_vec()),
            None,
        )
    });
    let midia = resultado.expect("um PNG lido por volume foi recusado");
    assert_eq!(
        midia.papel, "imagem",
        "um PNG lido por volume saiu com outro papel"
    );
    assert!(
        rastro.is_empty(),
        "uma imagem lida e aceita escreveu no registro, e ele ensinaria a ignorar as \
             recusas: {rastro}"
    );
}

#[test]
fn o_motivo_do_servidor_tem_teto() {
    let (_, rastro) = recusa_do_servidor(&"a".repeat(TETO_DA_FRASE_NO_REGISTRO * 4));
    let linha =
        recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
    assert!(
        linha.contains(&"a".repeat(TETO_DA_FRASE_NO_REGISTRO))
            && !linha.contains(&"a".repeat(TETO_DA_FRASE_NO_REGISTRO + 1)),
        "o motivo que o servidor manda entrou no registro sem o teto das outras portas \
             de texto de terceiro: {} caracteres numa linha",
        linha.chars().count()
    );
}

/// **A origem também tem teto.** É o caminho que a janela pede em
/// `midia_do_mod` e em `som_do_mod`: texto que chega pela ponte, e que ia
/// inteiro para a linha. As aspas impedem a linha forjada, e não a linha
/// sem fim.
#[test]
fn uma_origem_enorme_sai_cortada_no_teto_de_frase() {
    let (_, rastro) = capturar(|| {
        recusa_de_midia_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            &"c".repeat(TETO_DA_FRASE_NO_REGISTRO * 4),
            FalhaNoMod::Recusado {
                motivo: "arquivo-nao-declarado".to_owned(),
            },
        )
    });
    let linha =
        recusa(&rastro).unwrap_or_else(|| panic!("a recusa não chegou ao registro: {rastro}"));
    assert!(
        linha.contains(&format!(
            "origem=\"{}\"",
            "c".repeat(TETO_DA_FRASE_NO_REGISTRO)
        )),
        "o caminho que a janela pede entrou no registro sem o teto das outras portas de \
             texto de terceiro: {} caracteres numa linha",
        linha.chars().count()
    );
}

/// **O id do MOD sai igual pelas duas portas do registro.** A janela
/// (`registrar_da_janela`) e a recusa de mídia do Rust escrevem o mesmo
/// `mod_id=`, e é por ele que uma busca só junta o que as duas disseram do
/// mesmo MOD. Com o tratamento escrito à mão em cada porta, um teto que
/// mudasse numa e não na outra separaria as duas linhas sem que nada
/// reclamasse.
#[test]
fn o_id_do_mod_sai_igual_pelas_duas_portas_do_registro() {
    let enorme = format!("prova/{}", "m".repeat(TETO_DO_ID_NO_REGISTRO * 4));
    let esperado: String = enorme.chars().take(TETO_DO_ID_NO_REGISTRO).collect();
    let ((), da_janela) = capturar(|| {
        registrar_da_janela_em(
            &BaldesDoRegistro::default(),
            origem(),
            "aviso".to_owned(),
            "recusa-de-mod".to_owned(),
            "uma frase da janela".to_owned(),
            Some(enorme.clone()),
        );
    });
    let (_, do_rust) = capturar(|| {
        recusa_de_midia_dita(
            &BaldesDoRegistro::default(),
            origem(),
            &enorme,
            "volume",
            FalhaNoMod::Recusado {
                motivo: "nao-e-base64".to_owned(),
            },
        )
    });
    for (porta, rastro) in [
        ("registrar_da_janela", &da_janela),
        ("recusa_de_midia_dita", &do_rust),
    ] {
        let id = rastro
            .split("mod_id=")
            .nth(1)
            .and_then(|resto| resto.split_whitespace().next())
            .unwrap_or_else(|| panic!("`{porta}` não escreveu `mod_id=`: {rastro}"));
        assert_eq!(
            id, esperado,
            "`{porta}` escreveu o id do MOD com outro teto, e uma busca por `mod_id=` já \
                 não junta o que a janela e o Rust disseram do mesmo MOD"
        );
    }
}

/// **O pior caso sai com o mesmo teto pelas três portas que escapam.**
///
/// `\u{100000}` é o caractere que mais cresce no escape: um escrito, dez
/// no registro. O `console` do executor corta pelo tamanho escapado; o
/// motivo e o caminho de uma mídia de MOD cortavam em 512 caracteres
/// **antes** do escape, e quinhentos e doze deles viravam 5120 na linha —
/// dez vezes o teto, nas portas que não têm teto de vazão. Com uma regra só,
/// os campos saem do mesmo tamanho, e nenhum passa do teto.
///
/// O `console` vai pelo caminho real: o executor corta, e a mesma
/// `assentar_fala` da bomba escreve.
///
/// **E a frase da janela**, desde que ela vai em `?`: cortada antes do
/// escape, como era, quinhentos e doze `\u{100000}` eram 5120 caracteres
/// na linha. Ela é cortada pela mesma regra do `console`, com o mesmo aviso.
#[test]
fn o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas() {
    use crate::executor::{ExecutorQuickJs, Fila, Limites, ParaOFora};
    let pior = "\u{100000}".repeat(TETO_DA_FRASE_NO_REGISTRO * 4);
    // O que vai entre as aspas de `nome="…"`, como a linha o leva.
    let campo = |rastro: &str, nome: &str| -> String {
        rastro
            .split(&format!("{nome}=\""))
            .nth(1)
            .and_then(|resto| resto.split('"').next())
            .unwrap_or_else(|| panic!("a linha não traz `{nome}=\"…\"`: {rastro}"))
            .to_owned()
    };

    let executor = ExecutorQuickJs::novo(Limites::default()).expect("o motor");
    executor
        .iniciar(r"console.log('\u{100000}'.repeat(2048));")
        .expect("o código");
    let fala = loop {
        match executor.receber(std::time::Duration::from_secs(5)) {
            Some(fala @ ParaOFora::Console { .. }) => break fala,
            Some(_) => {}
            None => panic!("a linha de console não saiu do motor"),
        }
    };
    let mods = std::sync::Mutex::new(super::ModsNativos::default());
    let fila = std::sync::Arc::new(Fila::default());
    let (_, do_console) = capturar(|| super::assentar_fala(&mods, &|| true, &fila, 1, fala, ID));
    let do_console = campo(&do_console, "texto");
    let do_console = do_console
        .strip_suffix(" […cortado: 2048 caracteres]")
        .unwrap_or_else(|| panic!("a linha de console não diz que foi cortada: {do_console}"));

    let (_, do_motivo) = recusa_do_servidor(&pior);
    let (_, da_origem) = capturar(|| {
        recusa_de_midia_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            &pior,
            FalhaNoMod::Recusado {
                motivo: "arquivo-nao-declarado".to_owned(),
            },
        )
    });
    let ((), do_caminho) = capturar(|| {
        midia_servida_dita(
            &BaldesDoRegistro::default(),
            origem(),
            ID,
            7,
            &pior,
            "imagem",
            8,
        )
    });
    // A frase da janela é cortada pela mesma regra do `console`, com o
    // mesmo aviso: os dois são texto de terceiro que chega inteiro, e o
    // aviso diz o tamanho do que foi escrito.
    let ((), da_janela) = capturar(|| {
        registrar_da_janela_em(
            &BaldesDoRegistro::default(),
            origem(),
            "aviso".to_owned(),
            "recusa-de-mod".to_owned(),
            pior.clone(),
            Some(ID.to_owned()),
        );
    });
    let da_janela = campo(&da_janela, "frase");
    let da_janela = da_janela
        .strip_suffix(" […cortado: 2048 caracteres]")
        .unwrap_or_else(|| {
            panic!(
                "a frase da janela não diz que foi cortada, e quem lê o registro toma o \
                     pedaço pelo todo: {da_janela}"
            )
        });

    let um_a_mais = r"\u{100000}".chars().count();
    for (porta, escrito) in [
        ("console", do_console.to_owned()),
        ("janela", da_janela.to_owned()),
        ("motivo", campo(&do_motivo, "motivo")),
        ("origem", campo(&da_origem, "origem")),
        ("caminho", campo(&do_caminho, "caminho")),
    ] {
        let tamanho = escrito.chars().count();
        assert!(
            tamanho <= TETO_DA_FRASE_NO_REGISTRO,
            "`{porta}` levou {tamanho} caracteres ao registro, e o teto de uma frase de \
                 terceiro é {TETO_DA_FRASE_NO_REGISTRO}: o corte foi contado antes do escape"
        );
        assert!(
            tamanho + um_a_mais > TETO_DA_FRASE_NO_REGISTRO,
            "`{porta}` cortou mais do que precisava ({tamanho} de \
                 {TETO_DA_FRASE_NO_REGISTRO}): cabia mais um, e o registro perde o que o \
                 terceiro escreveu sem razão"
        );
        assert_eq!(
            escrito, do_console,
            "`{porta}` cortou o pior caso noutro tamanho que o `console`, e as portas do \
                 mesmo `seele.log` voltam a ter tetos diferentes"
        );
    }
}

/// **Os quatro comandos de mídia passam pela recusa dita.**
///
/// Os testes acima (e os do som) medem as funções; este prende que os
/// comandos as chamam. `ler_imagem_mod` fala com a rede e não sobe num
/// teste sem servidor — e um comando que devolvesse a recusa por fora seria
/// a mídia recusada em silêncio de novo, com os testes verdes.
///
/// Em `ler_imagem_mod` a chamada é `imagem_lida_dita`, que leva as duas
/// recusas de depois do `await`. Procurar `recusa_de_midia_dita` ali
/// passava com uma só delas: a outra saía calada, e o guarda verde. E a
/// conexão ausente de `ler_imagem_mod` vai ao registro sem ser contada.
///
/// O fonte é normalizado para `\n`: num checkout com CRLF o corte em
/// `"\n}\n"` não acharia o fim da função, e o guarda passaria a ler o
/// arquivo inteiro — inclusive este teste, que cita as quatro chamadas.
#[test]
fn os_quatro_comandos_de_midia_passam_pela_recusa_dita() {
    let fonte = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("main.rs legível")
    .replace("\r\n", "\n");
    for (comando, chamada) in [
        ("fn midia_do_mod(", "midia_declarada_do_mod("),
        ("fn midia_em_bytes(", "midia_do_servidor_dita("),
        ("async fn ler_imagem_mod(", "imagem_lida_dita("),
        ("fn som_do_mod(", "som_declarado_do_mod("),
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
        assert!(
            codigo.join("\n").contains(chamada),
            "`{comando}` não passa por `{chamada}`: a recusa dele volta à janela e não \
                 chega ao registro"
        );
    }

    // **A conexão ausente de `ler_imagem_mod` é dita, e não contada.** A
    // geração já foi conferida na entrada: com ela de pé, a conexão que
    // falta não é comando de uma sessão encerrada. Os outros guardas não
    // veem a diferença — um `recusar_se_morta` a mais ali sobe a conta da
    // função e a do arquivo juntas.
    let ler: String = fonte
        .split("async fn ler_imagem_mod(")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .expect("`async fn ler_imagem_mod(` sumiu de main.rs")
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .collect();
    let conexao = ler
        .split_once("session.connection()")
        .and_then(|(_, depois)| depois.split_once(")?;"))
        .map(|(conexao, _)| conexao)
        .expect(
            "a conexão de `ler_imagem_mod` não sai mais por `session.connection()…?;`, e \
                 este guarda já não acha a recusa dela",
        );
    for contar in [
        "comandos_de_geracao_morta",
        "confere_geracao(",
        "recusar_se_morta(",
    ] {
        assert!(
            !conexao.contains(contar),
            "a conexão ausente de `ler_imagem_mod` passou por `{contar}`: com a geração de \
                 pé, ela não é comando de uma sessão encerrada, e o contador diria que a sessão \
                 anterior falou sem ela ter falado"
        );
    }
    assert!(
        conexao.contains("recusa_de_midia_dita("),
        "a conexão ausente de `ler_imagem_mod` voltou à janela sem passar por \
             `recusa_de_midia_dita`, e não chega ao registro"
    );
    assert!(
        !ler.contains("comandos_de_geracao_morta"),
        "`ler_imagem_mod` mexe em `comandos_de_geracao_morta` por fora de \
             `recusar_se_morta`: uma conta à mão volta a conviver com a regra"
    );
}
