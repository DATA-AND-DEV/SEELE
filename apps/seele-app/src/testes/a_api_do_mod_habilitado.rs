use super::*;

/// Um pacote no cache de uma casa temporária, com a API que o manifesto
/// declara. Devolve a casa e o hash do pacote.
fn pacote_no_cache(api: u32) -> (std::path::PathBuf, String) {
    // Uma pasta por chamada: os testes deste módulo rodam em paralelo no mesmo
    // processo, e uma pasta só deixaria um apagar o pacote do outro.
    static QUAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = QUAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let raiz = std::env::temp_dir().join(format!("seele-app-p7b-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    let fonte = raiz.join("fonte");
    std::fs::create_dir_all(fonte.join("servidor")).expect("temporário");
    std::fs::write(
            fonte.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"seele/p7b","version":"1.0.0","api":{api},"repo":"https://github.com/seele/p7b","reach":[],"server":"servidor/main.js"}}"#
            ),
        )
        .expect("manifesto");
    std::fs::write(
        fonte.join("servidor/main.js"),
        "globalThis.aoPedir = () => '';",
    )
    .expect("metade de servidor");
    let lido =
        seele_ffi::mods::ler_pasta(&fonte.to_string_lossy()).expect("pacote válido recusado");
    let casa = raiz.join("casa");
    let guardado = casa.join(seele_ffi::mods::PACOTES).join(&lido.hash);
    std::fs::create_dir_all(guardado.join("servidor")).expect("cache");
    for arquivo in ["mod.json", "servidor/main.js"] {
        std::fs::copy(fonte.join(arquivo), guardado.join(arquivo)).expect("copiar");
    }
    (casa, lido.hash)
}

#[test]
fn exigir_um_mod_grava_no_banco_a_api_que_o_manifesto_declara() {
    // **Na linha do corte, só a 6 lê** (`APIS_ACEITAS = [6]`): a 4 que este
    // teste usava na versao/0.16 é recusada antes, com `api-too-old`. O
    // caminho inteiro, do pacote em disco à linha do banco, vai com a 6, que
    // não é o zero de «não preenchida»; e o «a do manifesto, e não a deste
    // build» é conferido na construção, com outra API posta no pacote lido.
    let declarada = seele_ffi::mods::MOD_API_VERSION;
    let (casa, hash) = pacote_no_cache(declarada);
    let instalado = seele_ffi::mods::ler_por_hash(&casa.to_string_lossy(), &hash)
        .expect("o pacote não foi achado por hash");
    let mut de_outra_api = instalado.clone();
    de_outra_api.api = 5;
    assert_eq!(
        habilitado_de(de_outra_api).api,
        5,
        "a construção gravou a API deste build, e não a que o pacote lido declara"
    );
    // **E o `tema` também** (migração 16): o conjunto o gravava falso, à mão,
    // para todo MOD, e a tela de aceite não diria que um MOD de tema pinta a
    // aparência do servidor. Os dois comandos passam por esta construção.
    let mut de_tema = instalado.clone();
    de_tema.tema = true;
    assert!(
        habilitado_de(de_tema).tema,
        "a construção não gravou o `tema` que o pacote lido declara"
    );
    let banco =
        seele_server::persistence::Persistence::open(&seele_server::persistence::Location::Memory)
            .expect("banco em memória");
    seele_server::persistence::mods::enable(&banco, &habilitado_de(instalado))
        .expect("exigir o MOD");
    let gravada = seele_server::persistence::mods::enabled(&banco)
        .expect("ler os exigidos")
        .into_iter()
        .find(|exigido| exigido.id == "seele/p7b")
        .map(|exigido| exigido.api);
    let _ = std::fs::remove_dir_all(casa.parent().unwrap_or(&casa));
    assert_eq!(
        gravada,
        Some(declarada),
        "a coluna `api` não recebeu a API que o manifesto declara: o anúncio diria outra"
    );
}

/// A terceira escrita da API, a da lista do cache que a tela de MODs
/// desenha: o mesmo pacote, pelo mesmo `listar_por_conteudo` do comando.
#[test]
fn a_lista_do_cache_mostra_a_api_que_o_manifesto_declara() {
    // A 6, pela razão do teste de cima; e a construção, com outra API.
    let declarada = seele_ffi::mods::MOD_API_VERSION;
    let (casa, hash) = pacote_no_cache(declarada);
    let pasta = casa.to_string_lossy().into_owned();
    let instalado = seele_ffi::mods::listar_por_conteudo(&pasta)
        .into_iter()
        .find(|instalado| instalado.hash == hash)
        .expect("o pacote não está na lista do cache");
    let mut de_outra_api = instalado.clone();
    de_outra_api.api = 5;
    assert_eq!(
        pacote_no_cache_de(&pasta, de_outra_api, &[]).api,
        5,
        "a lista do cache mostra a API deste build, e não a que o pacote lido declara"
    );
    let na_tela = pacote_no_cache_de(&pasta, instalado, &[]);
    let _ = std::fs::remove_dir_all(casa.parent().unwrap_or(&casa));
    assert_eq!(
        na_tela.api, declarada,
        "a lista do cache mostra outra API que a do manifesto, e a tela e o banco discordam"
    );
}

#[test]
fn os_dois_comandos_que_gravam_passam_pela_mesma_construcao() {
    let fonte = fonte_sem_comentarios();
    // Montada, e não escrita: escrita aqui, a própria agulha seria achada
    // no texto deste teste.
    let a_mao = concat!("EnabledMod", " {");
    for comando in [
        "async fn habilitar_mod(",
        "async fn aplicar_conjunto_de_mods(",
    ] {
        let corpo = corpo_de(&fonte, comando);
        assert!(
            corpo.contains("habilitado_de(") && !corpo.contains(a_mao),
            "`{comando}` deixou de gravar pela construção que leva a API do manifesto"
        );
    }
    let lista = corpo_de(&fonte, "async fn pacotes_no_cache(");
    assert!(
        lista.contains("pacote_no_cache_de(") && !lista.contains(concat!("PacoteNoCache", " {")),
        "`pacotes_no_cache` deixou de montar a lista pela construção que leva a API do manifesto"
    );
}
