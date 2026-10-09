//! **Todo teste que lê o log põe de pé o segundo despachante antes da
//! captura** (pendência 56).
//!
//! Com um despachante só registrado, o `tracing-core` 0.1.36 deixa a thread que
//! toca uma linha primeiro decidir se ela tem quem a ouça. Um teste sem captura
//! que passe antes pela linha a marca como «nunca», e o teste que a captura
//! deixa de vê-la. O conserto é um `segundo_despachante()` que fica vivo o
//! processo inteiro, e há uma cópia dele em cada lugar que captura: o app, o
//! `seele-core`, o `seele-server`, o `tests/desconhecida.rs` do servidor e o
//! `seele-ffi`.
//!
//! O defeito que este guarda pega é o teste de log **novo** que esquece a
//! chamada. Ele passa sozinho e na maior parte das rodadas da bateria, e
//! reprova só quando um vizinho sem captura chega antes, num runner com outro
//! número de threads. Ninguém o vê falhar até o dia em que ele falha sem dado
//! nenhum.
//!
//! O que ele confere, nos `.rs` de `crates/` e `apps/` (fora de `target/`, e
//! ignorando os comentários):
//! - toda captura (`with_default(` ou `set_default(`) tem `segundo_despachante()`
//!   acima dela, **na mesma função**. Uma `fn` aninhada não conta como a
//!   função. No `seele-core` vale também `Rastro::a_partir_de(`, porque ele
//!   chama o auxiliar;
//! - o corpo de `Rastro::a_partir_de` chama `segundo_despachante()`;
//! - ele achou capturas bastantes para não estar cego;
//! - toda cópia de `fn segundo_despachante` tem o mesmo corpo da do app, que é
//!   a referência. Uma cópia esvaziada passaria pelas outras conferências.
//!
//! Ele **lê** `apps/seele-app/src/main.rs`, e não o muda.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::{Path, PathBuf};

/// Abaixo disto, o guarda deixou de achar as capturas que existem, e um verde
/// dele não diz nada. Em 09/10 eram 38.
const MINIMO_DE_CAPTURAS: usize = 30;

/// Os arquivos com uma cópia do auxiliar, a partir da raiz. O primeiro é a
/// referência: o app, de onde o conserto veio.
const COPIAS: [&str; 5] = [
    "apps/seele-app/src/main.rs",
    "crates/seele-core/src/rastro_de_teste.rs",
    "crates/seele-server/src/rastro_de_teste.rs",
    "crates/seele-server/tests/desconhecida.rs",
    "crates/seele-ffi/src/lib.rs",
];

/// O único lugar em que `Rastro::a_partir_de(` vale como o auxiliar.
const CORE: &str = "crates/seele-core";

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("o xtask mora um nível abaixo da raiz")
        .to_path_buf()
}

/// Todo `.rs` de `pasta`, fora de `target/`.
fn fontes(pasta: &Path) -> Vec<PathBuf> {
    let mut achados = Vec::new();
    let mut pendentes = vec![pasta.to_path_buf()];
    while let Some(atual) = pendentes.pop() {
        for entrada in std::fs::read_dir(&atual).expect("legível") {
            let caminho = entrada.expect("entrada").path();
            if caminho.is_dir() {
                if caminho.file_name().is_some_and(|n| n != "target") {
                    pendentes.push(caminho);
                }
            } else if caminho.extension().is_some_and(|e| e == "rs") {
                achados.push(caminho);
            }
        }
    }
    achados.sort();
    achados
}

/// O código da linha, sem o comentário (`//`, `///`, `//!`).
fn codigo(linha: &str) -> &str {
    linha.split("//").next().unwrap_or("")
}

/// Quantos espaços abrem a linha.
fn recuo(linha: &str) -> usize {
    linha.len() - linha.trim_start().len()
}

/// Uma captura do log: `with_default(` ou `set_default(`.
/// `set_default_client_config` e `with_defaults` não são.
fn e_captura(linha: &str) -> bool {
    let codigo = codigo(linha);
    codigo.contains("with_default(") || codigo.contains("set_default(")
}

/// A linha abre uma função: `fn`, com o que pode vir antes dele.
fn abre_funcao(linha: &str) -> bool {
    let mut resto = codigo(linha).trim_start();
    loop {
        if resto.starts_with("fn ") {
            return true;
        }
        let Some((palavra, depois)) = resto.split_once(' ') else {
            return false;
        };
        if !matches!(
            palavra,
            "pub" | "pub(crate)" | "pub(super)" | "async" | "const" | "unsafe" | "extern" | "\"C\""
        ) {
            return false;
        }
        resto = depois;
    }
}

/// A linha em que fecha a função aberta na linha `inicio`: a própria, se ela
/// abre e fecha ali (ou só declara), e senão a primeira `}` no mesmo recuo.
fn fim_da_funcao(linhas: &[&str], inicio: usize) -> usize {
    let abertura = codigo(linhas[inicio]).trim_end();
    if abertura.ends_with('}') || abertura.ends_with(';') {
        return inicio;
    }
    let r = recuo(linhas[inicio]);
    (inicio + 1..linhas.len())
        .find(|&k| recuo(linhas[k]) == r && linhas[k].trim_start().starts_with('}'))
        .unwrap_or(linhas.len())
}

/// As linhas (contadas de 1) das capturas sem o auxiliar antes, na mesma
/// função. `aceita_a_partir_de` só no `seele-core`, onde
/// `Rastro::a_partir_de` chama o auxiliar.
///
/// A função de uma captura é a `fn` mais próxima acima dela com recuo menor. O
/// auxiliar vale do começo dessa função até a captura, fora do corpo de uma
/// `fn` aninhada no meio: ela não roda só por estar ali.
fn capturas_sem_o_auxiliar(texto: &str, aceita_a_partir_de: bool) -> Vec<usize> {
    let linhas: Vec<&str> = texto.lines().collect();
    let auxiliar = |linha: &str| {
        let codigo = codigo(linha);
        codigo.contains("segundo_despachante()")
            || (aceita_a_partir_de && codigo.contains("Rastro::a_partir_de("))
    };
    let mut sem = Vec::new();
    for (i, linha) in linhas.iter().enumerate() {
        if !e_captura(linha) {
            continue;
        }
        let funcao = (0..i)
            .rev()
            .find(|&j| abre_funcao(linhas[j]) && recuo(linhas[j]) < recuo(linha));
        let mut j = funcao.map_or(0, |f| f + 1);
        let mut tem = false;
        while j <= i {
            if j < i && abre_funcao(linhas[j]) {
                j = fim_da_funcao(&linhas, j) + 1;
                continue;
            }
            tem |= auxiliar(linhas[j]);
            j += 1;
        }
        if !tem {
            sem.push(i + 1);
        }
    }
    sem
}

/// O corpo de cada `fn <nome>(` do texto, uma linha de código por item, sem o
/// recuo, sem comentários e sem linhas vazias.
fn corpos_de(texto: &str, nome: &str) -> Vec<Vec<String>> {
    let linhas: Vec<&str> = texto.lines().collect();
    let assinatura = format!("fn {nome}(");
    let mut corpos = Vec::new();
    for (i, linha) in linhas.iter().enumerate() {
        if !abre_funcao(linha) || !codigo(linha).contains(&assinatura) {
            continue;
        }
        let fim = fim_da_funcao(&linhas, i);
        let corpo: Vec<String> = if fim == i {
            let abertura = codigo(linha);
            let dentro = abertura
                .find('{')
                .and_then(|a| abertura.rfind('}').map(|f| &abertura[a + 1..f]))
                .unwrap_or("")
                .trim();
            if dentro.is_empty() {
                Vec::new()
            } else {
                vec![dentro.to_owned()]
            }
        } else {
            linhas[i + 1..fim.min(linhas.len())]
                .iter()
                .map(|l| codigo(l).trim().to_owned())
                .filter(|l| !l.is_empty())
                .collect()
        };
        corpos.push(corpo);
    }
    corpos
}

#[test]
fn toda_captura_de_log_chama_o_segundo_despachante_antes() {
    let raiz = raiz();
    let mut todos = fontes(&raiz.join("crates"));
    todos.extend(fontes(&raiz.join("apps")));
    let mut capturas = 0;
    let mut faltam = Vec::new();
    for arquivo in &todos {
        let texto = std::fs::read_to_string(arquivo).expect("legível");
        let relativo = arquivo.strip_prefix(&raiz).expect("dentro da raiz");
        let no_core = relativo.starts_with(CORE);
        capturas += texto.lines().filter(|linha| e_captura(linha)).count();
        for n in capturas_sem_o_auxiliar(&texto, no_core) {
            faltam.push(format!("{}:{n}", relativo.display()));
        }
    }
    assert!(
        capturas >= MINIMO_DE_CAPTURAS,
        "o guarda ficou cego: achou só {capturas} capturas do log em crates/ e apps/, e um \
         verde dele não diz mais se os testes de log chamam o segundo despachante"
    );
    assert!(
        faltam.is_empty(),
        "estas capturas do log não chamam segundo_despachante() antes, na mesma função: a \
         linha que o teste procura pode sumir quando outro teste, sem captura, passa por ela \
         primeiro (pendência 56). Chame crate::rastro_de_teste::segundo_despachante() (ou, no \
         seele-core, crie o rastro por Rastro::a_partir_de) antes da captura: {faltam:?}"
    );
}

#[test]
fn o_rastro_do_core_chama_o_segundo_despachante_em_a_partir_de() {
    let texto = std::fs::read_to_string(raiz().join(CORE).join("src/rastro_de_teste.rs"))
        .expect("o guarda ficou cego: sem o rastro_de_teste.rs do seele-core");
    let corpos = corpos_de(&texto, "a_partir_de");
    assert_eq!(
        corpos.len(),
        1,
        "o guarda ficou cego: Rastro::a_partir_de não está uma vez só no rastro_de_teste.rs \
         do seele-core"
    );
    assert!(
        corpos[0]
            .iter()
            .any(|linha| linha.contains("segundo_despachante()")),
        "Rastro::a_partir_de deixou de chamar segundo_despachante(): os testes de log do \
         seele-core voltam a depender da ordem das threads, e este guarda aceita a_partir_de \
         como o auxiliar: {:?}",
        corpos[0]
    );
}

#[test]
fn toda_copia_do_segundo_despachante_tem_o_corpo_da_do_app() {
    let raiz = raiz();
    let referencia = {
        let texto = std::fs::read_to_string(raiz.join(COPIAS[0])).expect("legível");
        let corpos = corpos_de(&texto, "segundo_despachante");
        assert_eq!(
            corpos.len(),
            1,
            "o guarda ficou cego: o auxiliar de referência não está uma vez só em {}",
            COPIAS[0]
        );
        corpos[0].clone()
    };
    assert!(
        referencia
            .iter()
            .any(|linha| linha.contains("NoSubscriber")),
        "o guarda ficou cego: o auxiliar do app não põe mais de pé um NoSubscriber, e as \
         cópias seriam conferidas contra um corpo que não conserta nada: {referencia:?}"
    );

    let mut todos = fontes(&raiz.join("crates"));
    todos.extend(fontes(&raiz.join("apps")));
    let mut com_copia = Vec::new();
    let mut diferentes = Vec::new();
    for arquivo in &todos {
        let texto = std::fs::read_to_string(arquivo).expect("legível");
        let relativo = arquivo
            .strip_prefix(&raiz)
            .expect("dentro da raiz")
            .to_path_buf();
        for corpo in corpos_de(&texto, "segundo_despachante") {
            if corpo != referencia {
                diferentes.push(format!("{}: {corpo:?}", relativo.display()));
            }
            com_copia.push(relativo.clone());
        }
    }
    for copia in COPIAS {
        assert!(
            com_copia.iter().any(|achado| achado == Path::new(copia)),
            "o guarda ficou cego: {copia} não tem mais fn segundo_despachante, e os testes de \
             log de lá perderam o auxiliar ou o chamam de outro lugar"
        );
    }
    assert!(
        diferentes.is_empty(),
        "uma cópia de segundo_despachante() não tem o corpo da do app ({referencia:?}): uma \
         cópia que não põe de pé o segundo despachante deixa todos os testes de log daquele \
         binário dependendo da ordem das threads, e o resto deste guarda não a vê: \
         {diferentes:?}"
    );
}

#[test]
fn a_captura_sem_o_auxiliar_reprova_e_com_ele_passa() {
    let sem = "fn a() {\n    let _g = tracing::subscriber::set_default(x);\n}\n";
    assert_eq!(
        capturas_sem_o_auxiliar(sem, false),
        vec![2],
        "a captura sem o auxiliar passou pelo guarda"
    );
    let com = "fn a() {\n    segundo_despachante();\n    let _g = tracing::subscriber::set_default(x);\n}\n";
    assert!(
        capturas_sem_o_auxiliar(com, false).is_empty(),
        "a captura com o auxiliar logo acima reprovou"
    );
    let depois = "fn a() {\n    let _g = tracing::subscriber::set_default(x);\n    segundo_despachante();\n}\n";
    assert_eq!(
        capturas_sem_o_auxiliar(depois, false),
        vec![2],
        "o auxiliar chamado depois da captura passou pelo guarda"
    );
}

#[test]
fn o_auxiliar_de_outra_funcao_nao_vale() {
    let outra =
        "fn a() {\n    segundo_despachante();\n}\n\nfn b() {\n    with_default(x, || ());\n}\n";
    assert_eq!(
        capturas_sem_o_auxiliar(outra, false),
        vec![6],
        "o auxiliar de outra função valeu para esta captura"
    );
    let async_e_pub = "pub(crate) async fn a() {\n    segundo_despachante();\n}\n\npub async fn b() {\n    set_default(x);\n}\n";
    assert_eq!(
        capturas_sem_o_auxiliar(async_e_pub, false),
        vec![6],
        "o guarda não reconheceu uma pub async fn como função"
    );
}

#[test]
fn uma_fn_aninhada_nao_e_a_funcao_da_captura() {
    let antes_da_aninhada = "fn a() {\n    segundo_despachante();\n    fn diz() {}\n    let _g = tracing::subscriber::set_default(x);\n}\n";
    assert!(
        capturas_sem_o_auxiliar(antes_da_aninhada, false).is_empty(),
        "uma fn aninhada entre o auxiliar e a captura escondeu o auxiliar"
    );
    let dentro_da_aninhada = "fn a() {\n    fn prepara() {\n        segundo_despachante();\n    }\n    let _g = tracing::subscriber::set_default(x);\n}\n";
    assert_eq!(
        capturas_sem_o_auxiliar(dentro_da_aninhada, false),
        vec![5],
        "o auxiliar dentro de uma fn aninhada valeu para a captura da função de fora, e ele só          roda se alguém chamar a aninhada"
    );
}

#[test]
fn a_partir_de_so_vale_no_core() {
    let core = "fn a() {\n    let r = Rastro::a_partir_de(L);\n    let _g = tracing::subscriber::set_default(r);\n}\n";
    assert!(
        capturas_sem_o_auxiliar(core, true).is_empty(),
        "no seele-core, Rastro::a_partir_de não valeu como o auxiliar"
    );
    assert_eq!(
        capturas_sem_o_auxiliar(core, false),
        vec![3],
        "fora do seele-core, um a_partir_de que não é o do Rastro do core valeu como o auxiliar"
    );
}

#[test]
fn comentario_nao_conta_nem_como_captura_nem_como_auxiliar() {
    let comentado = "fn a() {\n    // segundo_despachante();\n    set_default(x);\n    // set_default(y);\n    /// with_default(z, || ());\n}\n";
    assert_eq!(
        capturas_sem_o_auxiliar(comentado, false),
        vec![3],
        "um comentário contou como captura ou como o auxiliar"
    );
    assert!(
        !e_captura("//! let _guarda = tracing::subscriber::set_default(rastro.clone());"),
        "o exemplo de uma doc de módulo contou como captura"
    );
}

#[test]
fn set_default_client_config_e_with_defaults_nao_sao_captura() {
    let falsos =
        "fn a() {\n    ponta.set_default_client_config(c);\n    let e = E::with_defaults();\n}\n";
    assert!(
        capturas_sem_o_auxiliar(falsos, false).is_empty(),
        "set_default_client_config ou with_defaults contaram como captura do log"
    );
    assert!(e_captura(
        "    tracing::subscriber::with_default(s, || ());"
    ));
    assert!(e_captura(
        "    let _g = tracing::subscriber::set_default(s);"
    ));
}

#[test]
fn o_corpo_de_uma_fn_e_lido_sem_recuo_nem_comentario() {
    let texto = "mod m {\n    /// doc\n    pub(crate) fn segundo_despachante() {\n        // por quê\n        static S: u8 = 0;\n\n        usar(&S);\n    }\n\n    fn outra() {}\n}\n";
    assert_eq!(
        corpos_de(texto, "segundo_despachante"),
        vec![vec!["static S: u8 = 0;".to_owned(), "usar(&S);".to_owned()]],
        "o corpo lido não é o da função"
    );
    let vazia = "fn segundo_despachante() {}\n";
    assert_eq!(
        corpos_de(vazia, "segundo_despachante"),
        vec![Vec::<String>::new()],
        "a fn de corpo vazio numa linha só não foi lida como corpo vazio"
    );
    assert!(
        corpos_de(
            "fn chama() {\n    segundo_despachante();\n}\n",
            "segundo_despachante"
        )
        .is_empty(),
        "uma chamada foi lida como a definição"
    );
}
