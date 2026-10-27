//! diag-0151 (H): a janela de permissão que não aparecia.
//!
//! Medido na 0.16.0 (docs/evidencias/diagnostico-0151/RELATORIO.md, bancada
//! `bancada/diagnostico-0151.cjs`): com o servidor hospedado e a janela na tela inicial — o
//! `connect` do próprio dono volta pedindo o aceite dos MODs e a pessoa recusa —, alguém bate, a
//! faixa «ALGUÉM ESTÁ BATENDO» aparece, e VER QUEM É abre `#portaria`… dentro de `#tela-sessao`,
//! que estava escondida. A janela ficava com 0×0 px, o cartão com ADMITIR e RECUSAR desenhado e
//! invisível, e desenhar a fila calava a faixa. O comentário do index.html da 0.16 dizia o
//! porquê: «quem hospeda pelo app está sempre numa sessão». A 0.17 exige o contrário: hospedar
//! não exige participar, e o pedido aparece e é decidido em QUALQUER tela.
//!
//! Este guarda lê o `index.html` e reprova se uma camada que tem de aparecer em qualquer tela
//! estiver dentro de uma `<section class="tela">`, que é o que a esconde junto com a tela. O
//! comportamento (a janela visível, com o pedido, na tela inicial) é da bancada; o que este
//! guarda segura é a causa estrutural, que é barata de reintroduzir.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]

const INDEX: &str = include_str!("../ui/index.html");

/// As faixas de bytes de cada `<section …class="…tela…"…>` até o `</section>` que a fecha.
fn telas(html: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let mut saida = Vec::new();
    let mut desde = 0;
    while let Some(rel) = html[desde..].find("<section") {
        let inicio = desde + rel;
        let fim_da_tag = inicio + html[inicio..].find('>').expect("tag sem fim");
        let tag = &html[inicio..=fim_da_tag];
        desde = fim_da_tag;
        let e_tela = tag
            .split("class=\"")
            .nth(1)
            .and_then(|resto| resto.split('"').next())
            .is_some_and(|classes| classes.split_whitespace().any(|c| c == "tela"));
        if !e_tela {
            continue;
        }
        let id = tag
            .split("id=\"")
            .nth(1)
            .and_then(|resto| resto.split('"').next())
            .unwrap_or("?")
            .to_owned();
        // Fecha pelo aninhamento, e não pelo primeiro </section>: uma tela tem seções dentro.
        let mut fundo = 0_i32;
        let mut cursor = inicio;
        let fim = loop {
            let abre = html[cursor..].find("<section").map(|n| cursor + n);
            let fecha = html[cursor..]
                .find("</section>")
                .map(|n| cursor + n)
                .expect("section sem fim");
            match abre {
                Some(a) if a < fecha => {
                    fundo += 1;
                    cursor = a + "<section".len();
                }
                _ => {
                    fundo -= 1;
                    cursor = fecha + "</section>".len();
                    if fundo == 0 {
                        break cursor;
                    }
                }
            }
        };
        saida.push((id, inicio..fim));
    }
    saida
}

/// Em qual tela o elemento `id` está, se está em alguma.
fn tela_que_contem(html: &str, id: &str) -> Option<String> {
    let onde = html
        .find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("o index.html não tem #{id}"));
    telas(html)
        .into_iter()
        .find(|(_, faixa)| faixa.contains(&onde))
        .map(|(tela, _)| tela)
}

/// O guarda contra o defeito medido: as camadas que decidem quem entra, e a que avisa, ficam fora
/// de toda tela.
#[test]
fn a_porta_a_faixa_e_a_pergunta_dos_mods_ficam_fora_de_toda_tela() {
    for id in ["portaria", "portaria-batendo", "mods-aceite"] {
        assert_eq!(
            tela_que_contem(INDEX, id),
            None,
            "#{id} está dentro de uma <section class=\"tela\">: com a janela em outra tela (a inicial, \
             com o servidor hospedado e quem hospeda fora da própria sala), ela abre com 0×0 px e o \
             pedido de entrada não pode ser decidido — o defeito do diag-0151 na 0.16.0"
        );
    }
}

// --------------------------------------------------------------------------------------------
// A prova integrada no backend desta versão (E7): a pasta da 0.15.1 hospedada pela `Hospedagem`
// de verdade, o dono numa sessão de OUTRO servidor de verdade, e visitas FFI de verdade batendo.
// A decisão é `portaria::decidir`, o que o comando `decidir_pedido` chama (o comando lê o
// guardado da sessão do app, que um teste de integração não alcança). Sem UI: a UI é da bancada.

const PASTA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testes/pasta-da-0.15.1/pasta");

fn copiar(de: &std::path::Path, para: &std::path::Path) {
    std::fs::create_dir_all(para).unwrap();
    for entrada in std::fs::read_dir(de).unwrap() {
        let entrada = entrada.unwrap();
        let destino = para.join(entrada.file_name());
        if entrada.path().is_dir() {
            copiar(&entrada.path(), &destino);
        } else {
            std::fs::copy(entrada.path(), destino).unwrap();
        }
    }
}

/// Uma chegada FFI, numa thread de bloqueio, com a casa (a chave) de quem chega.
async fn bater(
    casa: &std::path::Path,
    apelido: &str,
    porta: u16,
    impressao: &str,
) -> Result<std::sync::Arc<seele_ffi::Connection>, seele_ffi::ConnectionError> {
    let config = seele_ffi::ConnectConfig {
        server: format!("127.0.0.1:{porta}"),
        alternate_servers: Vec::new(),
        nickname: apelido.to_owned(),
        home: casa.to_string_lossy().into_owned(),
        join_secret: None,
        expected_fingerprint: Some(impressao.to_owned()),
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    tokio::task::spawn_blocking(move || seele_ffi::Connection::connect(config).map(|(c, _)| c))
        .await
        .unwrap()
}

fn passou_da_porta(
    r: &Result<std::sync::Arc<seele_ffi::Connection>, seele_ffi::ConnectionError>,
) -> bool {
    !format!("{:?}", r.as_ref().err()).contains("Admission")
}

async fn subir(
    banco: &std::path::Path,
    raizes: seele_server::RaizesDosMods,
    dono: String,
) -> seele_server::hospedagem::Hospedagem {
    seele_server::hospedagem::Hospedagem::iniciar(
        0,
        seele_server::persistence::Location::File(banco.to_path_buf()),
        "Casa",
        Some(raizes),
        move |persistence| {
            seele_server::portaria::semear_ligada(persistence)?;
            seele_server::portaria::admitir_o_dono(persistence, &dono)
        },
    )
    .await
    .expect("a hospedagem não subiu sobre a pasta da 0.15.1")
}

async fn fila(h: &seele_server::hospedagem::Hospedagem) -> Vec<(String, bool, bool, i64)> {
    let p = h.persistence();
    let p = p.lock().await;
    seele_server::portaria::pedidos(&p)
        .unwrap()
        .into_iter()
        .map(|x| (x.apelido, x.decidido_em.is_some(), x.admitido, x.batidas))
        .collect()
}

/// Aprovar, rejeitar, cancelar e retomar, reiniciar: o estado do host não depende da sessão de
/// quem hospeda, que está o tempo todo vendo OUTRO servidor.
#[tokio::test]
async fn aprovar_rejeitar_retomar_e_reiniciar_com_o_dono_em_outro_servidor() {
    let raiz = tempfile::tempdir().unwrap();
    let dados = raiz.path().join("dados");
    std::fs::create_dir_all(&dados).unwrap();
    rusqlite::Connection::open(dados.join("seele.db"))
        .unwrap()
        .execute_batch(&std::fs::read_to_string(format!("{PASTA}/seele.sql")).unwrap())
        .unwrap();
    copiar(
        &std::path::Path::new(PASTA).join("mod-packages"),
        &dados.join("mod-packages"),
    );
    let banco = dados.join("seele.db");
    let raizes = || seele_server::RaizesDosMods {
        pacotes: dados.clone(),
        dados: dados.join("mod-data"),
    };
    // O MOD da API 4 decidido antes de hospedar, como o produto exige (o `hospedar` do app recusa com
    // `ModsDeApiAntiga` antes de subir): a saída DESLIGAR, que é `resolver_mod_de_api_antiga` →
    // `persistence::mods::disable` (os dados dele ficam). Sem isto, MEDIDO em 24/10: o servidor desta
    // versão recusa toda visita com `ModsDeOutraApi` antes da portaria, e a batida nem chega à fila.
    {
        let aberto = seele_server::persistence::Persistence::open(
            &seele_server::persistence::Location::File(banco.clone()),
        )
        .unwrap();
        seele_server::persistence::mods::disable(&aberto, "seele/estilo").unwrap();
    }
    let casa = |nome: &str| {
        let c = raiz.path().join(nome);
        std::fs::create_dir_all(&c).unwrap();
        c
    };
    let casa_do_dono = casa("dono");
    let dono = seele_ffi::impressao_desta_maquina(&casa_do_dono.to_string_lossy()).unwrap();

    let hospedado = subir(&banco, raizes(), dono.clone()).await;
    let porta = hospedado.endereco().port();
    let impressao = hospedado.impressao_digital().to_owned();

    // O dono está numa sessão de OUTRO servidor (aberto, em memória).
    let outro = seele_server::hospedagem::Hospedagem::iniciar(
        0,
        seele_server::persistence::Location::Memory,
        "Outro",
        None,
        |_| Ok(()),
    )
    .await
    .unwrap();
    let na_vista = bater(
        &casa_do_dono,
        "dono",
        outro.endereco().port(),
        outro.impressao_digital(),
    )
    .await;
    println!(
        "[i] 0. o dono na sessão do OUTRO servidor: {:?}",
        na_vista.as_ref().err()
    );
    assert!(
        na_vista.is_ok(),
        "o dono não entrou no outro servidor: {:?}",
        na_vista.err()
    );

    // 1. Duas visitas batem.
    let (ana, bia, cai) = (casa("ana"), casa("bia"), casa("cai"));
    let a1 = bater(&ana, "ana", porta, &impressao).await;
    let b1 = bater(&bia, "bia", porta, &impressao).await;
    println!(
        "[i] 1. ana: {:?} | bia: {:?}",
        a1.as_ref().err(),
        b1.as_ref().err()
    );
    assert!(
        !passou_da_porta(&a1) && !passou_da_porta(&b1),
        "a portaria não segurou as visitas"
    );
    let f1 = fila(&hospedado).await;
    println!("[i]    fila: {f1:?}");

    // 2. Aprovar a ana, rejeitar a bia (o que `decidir_pedido` faz).
    {
        let p = hospedado.persistence();
        let mut p = p.lock().await;
        let pedidos = seele_server::portaria::pedidos(&p).unwrap();
        let de = |nome: &str| {
            pedidos
                .iter()
                .find(|x| x.apelido == nome)
                .unwrap()
                .impressao
                .clone()
        };
        let (ia, ib) = (de("ana"), de("bia"));
        seele_server::portaria::decidir(&mut p, &ia, true).unwrap();
        seele_server::portaria::decidir(&mut p, &ib, false).unwrap();
    }
    let a2 = bater(&ana, "ana", porta, &impressao).await;
    let b2 = bater(&bia, "bia", porta, &impressao).await;
    println!(
        "[i] 2. depois de decidir — ana: {:?} | bia: {:?}",
        a2.as_ref().err(),
        b2.as_ref().err()
    );
    assert!(
        passou_da_porta(&a2),
        "a aprovada continua parada na portaria: {:?}",
        a2.err()
    );
    assert!(b2.is_err(), "a recusada entrou");

    // 3. Cancelar e retomar: o cai bate, desiste, e bate de novo — um pedido só, duas batidas.
    let _ = bater(&cai, "cai", porta, &impressao).await;
    let _ = bater(&cai, "cai", porta, &impressao).await;
    let f3 = fila(&hospedado).await;
    println!("[i] 3. fila depois de o cai bater duas vezes: {f3:?}");
    assert_eq!(
        f3.iter()
            .filter(|x| x.0 == "cai")
            .map(|x| x.3)
            .collect::<Vec<_>>(),
        vec![2]
    );

    // 4. Reiniciar o host: a fila e as decisões continuam.
    hospedado.encerrar().await;
    let de_novo = subir(&banco, raizes(), dono).await;
    let porta2 = de_novo.endereco().port();
    let f4 = fila(&de_novo).await;
    println!("[i] 4. depois de reiniciar: {f4:?}");
    assert!(
        f4.iter().any(|x| x.0 == "cai" && !x.1),
        "o pedido pendente sumiu ao reiniciar"
    );
    assert!(
        f4.iter().any(|x| x.0 == "ana" && x.1 && x.2),
        "a aprovação sumiu ao reiniciar"
    );
    assert!(
        f4.iter().any(|x| x.0 == "bia" && x.1 && !x.2),
        "a recusa sumiu ao reiniciar"
    );
    let a4 = bater(&ana, "ana", porta2, de_novo.impressao_digital()).await;
    println!("[i]    ana depois do reinício: {:?}", a4.as_ref().err());
    assert!(
        passou_da_porta(&a4),
        "a aprovada voltou a bater depois do reinício: {:?}",
        a4.err()
    );

    // 5. A sessão do dono no outro servidor não foi tocada por nada disso.
    let enlace = na_vista.as_ref().map(|c| c.snapshot().link).ok();
    println!("[i] 5. o enlace da sessão do dono no outro servidor: {enlace:?}");
    assert!(
        matches!(enlace, Some(seele_ffi::LinkState::Online)),
        "a sessão do dono no outro servidor não está mais no ar: {enlace:?}"
    );
    de_novo.encerrar().await;
    outro.encerrar().await;
}

/// O guarda guarda: com a forma da 0.16.0 (a portaria dentro de `#tela-sessao`, que tem seções
/// dentro), ele acusa a tela certa; fora dela, não acusa.
#[test]
fn o_guarda_acusa_a_forma_da_0_16() {
    let da_0_16 = r#"<div id="portaria-batendo" hidden></div>
<section id="tela-boot" class="tela"><p>entrada</p></section>
<section id="tela-sessao" class="tela" hidden>
  <section class="painel"><p>canais</p></section>
  <div id="portaria" class="camada-portaria" hidden></div>
  <section class="painel"><p>pessoas</p></section>
</section>
<div id="mods-aceite" hidden></div>"#;
    assert_eq!(
        tela_que_contem(da_0_16, "portaria").as_deref(),
        Some("tela-sessao")
    );
    assert_eq!(tela_que_contem(da_0_16, "portaria-batendo"), None);
    assert_eq!(tela_que_contem(da_0_16, "mods-aceite"), None);
    let consertada = da_0_16.replace(
        "  <div id=\"portaria\" class=\"camada-portaria\" hidden></div>\n",
        "",
    ) + "\n<div id=\"portaria\" hidden></div>";
    assert_eq!(tela_que_contem(&consertada, "portaria"), None);
}
