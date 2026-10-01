//! O MOD de referência: a API 3 provada por um vetor, e não por um guia.
//!
//! # Por que ele existe
//!
//! O ADR 0049 rompeu com a API 2 e entregou só três coisas: a API, o tempo de
//! execução e o guia. Os três MODs oficiais são reescritos noutro lugar, por
//! outra pessoa, contra um documento.
//!
//! **Um documento não reprova.** Se a API mudar de forma e o guia não mudar
//! junto, quem descobre é quem está reescrevendo um MOD, num repositório que
//! esta bateria não vê. Este vetor é a mitigação escrita no ADR: um MOD mínimo,
//! versionado aqui dentro, que usa cada coisa que a API oferece — e nada além.
//!
//! # O que ele prova, e o que não prova
//!
//! Prova:
//!
//! - **a metade de servidor roda**, de verdade, no mesmo anfitrião QuickJS que o
//!   produto usa: `aoPedir` responde, o quintal persiste entre pedidos, e um
//!   canal desconhecido é recusado pelo nome;
//! - **a metade de janela só chama o que existe**: cada `SeeleMods.x` e
//!   `SeeleUI.x` que ela usa é exposto pelo `PRELUDIO` de
//!   `apps/seele-app/src/executor.rs`, fora o `console`, cada
//!   `forma` que ela declara está na gramática de `montarODeclarado`, e cada
//!   chave de tema está em `TEMA_DA_API`;
//! - **o pacote atravessa a ponte inteiro**: instalado pelo caminho do produto,
//!   ele é lido de volta por onde `codigo_do_mod` lê, byte a byte.
//!
//! Não prova que um `Worker` de verdade executa aquele arquivo numa janela de
//! verdade. Isso é fumaça no binário nativo de cada sistema, e continua sendo
//! um passo à parte — o mesmo que
//! `o_carregador_de_mods_diz_na_tela_e_monta_a_url_pelo_tauri` já registra.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const RAIZ: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/seele-app/testes/mod-de-referencia"
);

fn vetor(relativo: &str) -> String {
    std::fs::read_to_string(Path::new(RAIZ).join(relativo))
        .unwrap_or_else(|erro| panic!("o vetor `{relativo}` sumiu: {erro}"))
}

fn base_js() -> String {
    arquivo_da_janela("base.js")
}

/// Um arquivo da janela do produto, pelo nome.
fn arquivo_da_janela(nome: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/seele-app/ui")
            .join(nome),
    )
    .unwrap_or_else(|erro| panic!("ui/{nome} sumiu: {erro}"))
}

fn temporario(nome: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seele-referencia-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temporário");
    dir
}

/// O manifesto do vetor declara a API que este build oferece.
///
/// Escrito à mão ele envelhece calado: o vetor continuaria no repositório,
/// continuaria passando nos outros testes, e deixaria de ser um MOD que este
/// produto carrega. Quando `MOD_API_VERSION` subir, este teste é o que lembra
/// de subir o arquivo junto.
#[test]
fn o_vetor_declara_a_api_que_este_build_oferece() {
    let manifesto: serde_json::Value =
        serde_json::from_str(&vetor("mod.json")).expect("o manifesto do vetor é JSON");
    let api = manifesto
        .get("api")
        .and_then(serde_json::Value::as_u64)
        .expect("o manifesto do vetor declara `api`");
    assert_eq!(
        u32::try_from(api).unwrap(),
        seele_proto::mods::MOD_API_VERSION,
        "o MOD de referência pede uma API que este build não oferece; \
         `apps/seele-app/testes/mod-de-referencia/mod.json` ficou para trás"
    );

    // E ele é lido pelo leitor de verdade, com as recusas de verdade.
    let lido = seele_proto::mods::read_manifest(&vetor("mod.json"))
        .expect("o manifesto do vetor tem de ser aceito pelo produto");
    assert_eq!(lido.id, "seele/referencia");
    assert_eq!(lido.client.as_deref(), Some("cliente/main.js"));
    assert_eq!(lido.server.as_deref(), Some("servidor/main.js"));
}

/// **A metade de servidor roda, e o quintal atravessa dois pedidos.**
///
/// No anfitrião de verdade, com `dados` de verdade: um MOD que não persistisse
/// nada responderia `1` nas duas vezes, e um que persistisse em memória
/// responderia `2` sem nada ter sido gravado.
#[test]
fn a_metade_de_servidor_do_vetor_responde_e_guarda() {
    let mut anfitriao = seele_server::mods::Anfitriao::novo().expect("anfitrião");
    let pasta = temporario("servidor");
    anfitriao
        .carregar("seele/referencia", &vetor("servidor/main.js"), &pasta)
        .expect("o vetor tem de compilar no anfitrião do produto");

    // O contexto tem a forma que `mods/pedidos.rs` monta, e `channel` vem
    // **nulo** porque este vetor é de escopo de servidor: ele manda canal zero,
    // e zero quer dizer «nenhum canal». Um vetor que lesse a operação do
    // contexto, ou que exigisse um canal de verdade, passaria aqui e falharia
    // no produto — que é o que este arquivo existe para impedir.
    let contexto = r#"{"person":"7","channel":null,"admin":false,"write":true}"#;
    let mut quintal = BTreeMap::new();
    let mut vistos = Vec::new();
    for _ in 0..2 {
        let resposta = anfitriao
            .pedir(
                "seele/referencia",
                seele_proto::ids::PersonId(7),
                contexto,
                r#"{"op":"contar"}"#,
                &mut quintal,
            )
            .expect("o vetor tem de responder");
        let corpo: serde_json::Value = serde_json::from_str(&resposta).expect("resposta é JSON");
        // Nomeado antes de medido: se o vetor tirar o canal do lugar errado —
        // do pedido, e não do contexto —, ele recusa o próprio canal e o teste
        // dizia só «`vezes` não é número», que não aponta para nada.
        assert!(
            corpo.get("erro").is_none(),
            "o vetor recusou um pedido que o produto manda assim: {resposta}"
        );
        vistos.push(
            corpo
                .get("vezes")
                .and_then(serde_json::Value::as_u64)
                .expect("`vezes` é número"),
        );
    }
    assert_eq!(
        vistos,
        vec![1, 2],
        "o quintal do vetor não atravessou dois pedidos: {quintal:?}"
    );
    assert_eq!(
        quintal.get("vezes").map(String::as_str),
        Some("2"),
        "o quintal foi devolvido sem o que o MOD gravou"
    );

    // E um canal que ele não conhece é recusado **pelo nome**, em vez de
    // responder vazio: quem chamou fica sabendo por que não veio resposta.
    let recusa = anfitriao
        .pedir(
            "seele/referencia",
            seele_proto::ids::PersonId(7),
            contexto,
            r#"{"op":"inventada"}"#,
            &mut quintal,
        )
        .expect("uma operação desconhecida é resposta, e não pânico");
    assert!(
        recusa.contains("inventada"),
        "a recusa não nomeia a operação que ela recusou: {recusa}"
    );
    assert_eq!(
        quintal.get("vezes").map(String::as_str),
        Some("2"),
        "uma operação recusada mexeu no quintal"
    );
}

/// **A metade de janela só chama o que o prelúdio oferece.**
///
/// O guarda que o ADR pediu: um `SeeleUI.audio(...)` neste arquivo reprova
/// aqui, em vez de virar um `Error: a API de MODs não conhece «audio»` na
/// máquina de quem estiver reescrevendo um MOD.
#[test]
fn a_metade_de_janela_do_vetor_so_chama_o_que_a_api_expoe() {
    let cliente = vetor("cliente/main.js");
    // **O prelúdio mora no executor**, e o executor é um só. Ele esteve
    // duplicado em `base.js` enquanto o Worker de `blob:` ainda era o caminho
    // do produto; o Worker foi reprovado por medição e saiu, e com ele a
    // segunda cópia — que era a que podia divergir em silêncio.
    let executor = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/seele-app/src/executor.rs"),
    )
    .expect("apps/seele-app/src/executor.rs");
    let preludio = executor
        .split_once("const PRELUDIO: &str = r#\"")
        .expect("o prelúdio dos MODs")
        .1
        .split_once("\"#;")
        .expect("o fim do prelúdio")
        .0;

    // Sem comentários dos dois lados. O cabeçalho deste vetor **lista** a API
    // em prosa, e contar aquelas linhas como uso faria o guarda passar com um
    // arquivo que não chama nada — que é a forma exata de «existir não é
    // funcionar» que o `CLAUDE.md` deste repositório nomeia.
    let executado = sem_comentarios(&cliente);
    // **Sem o `console`, nas duas metades.** Ele mora no mesmo prelúdio, mas
    // não é API de MOD: é da metade de janela, não está em nenhum `vN.json`, e
    // o `api/README.md` o diz fora da superfície congelada («Nada disto está
    // num `vN.json`»). Aqui, um `SeeleUI.log(…)` no vetor acharia o `log:` do
    // console e passaria como «exposto», e no produto seria um `TypeError`.
    // Embaixo, os membros dele que casam com `nome: (` — `assert` e os nove
    // que não fazem nada, de `group` a `clear` — entravam na lista do que a
    // API oferece, e o guarda cobrava do vetor um `.assert(` que a API não
    // oferece. Quem prova o console são os testes do executor, e não este
    // vetor.
    let api_do_preludio = sem_o_console(preludio);
    for (objeto, metodo) in chamadas_de(&executado) {
        // **Um membro, ou um espaço de nomes.** A API 4 agrupou o que ela
        // acrescentou — `SeeleUI.superficies.criar`, `SeeleUI.contribuicoes.
        // registrar` —, e os grupos entram no objeto por abreviação
        // (`{ superficies }`), sem `superficies:` em lugar nenhum. Procurar só
        // por `nome:` reprovava o vetor por chamar uma coisa que existe.
        let existe = api_do_preludio.contains(&format!("{metodo}:"))
            || api_do_preludio.contains(&format!("const {metodo} = comCapacidade("));
        assert!(
            existe,
            "o vetor chama `{objeto}.{metodo}`, e o prelúdio do executor não o expõe"
        );
    }

    // E cada coisa que a API oferece é exercitada. Um vetor que deixasse de
    // chamar uma delas deixaria aquela metade da API sem prova nenhuma, calado.
    //
    // A lista sai do **prelúdio**, e não é escrita aqui: escrita, ela ficaria
    // para trás no dia em que a API crescesse — e o guarda passaria a dizer
    // «tudo exercitado» sobre uma lista velha. Foi o que ia acontecer quando
    // `aoEvento` entrou. Ela também sai de `api_do_preludio`, sem o console.
    let oferecidos: Vec<String> = api_do_preludio
        .lines()
        .filter_map(|linha| {
            let corte = linha.trim().split_once(": (")?;
            let nome = corte.0.trim();
            nome.chars()
                .all(|c| c.is_ascii_alphanumeric())
                .then(|| nome.to_owned())
        })
        .collect();
    assert!(
        oferecidos.len() >= 5,
        "o prelúdio passou a oferecer {} coisas, e a leitura desta lista \
         provavelmente quebrou: {oferecidos:?}",
        oferecidos.len()
    );
    // **Chamado em qualquer receptor, e não só em `SeeleUI`.** A API 4 devolve
    // punhos: `superficies.criar` responde um objeto local, e `montar`,
    // `mostrar`, `fechar` e `descartar` são chamados nele. Procurar só por
    // `SeeleUI.montar(` diria que o vetor deixou de exercitar `montar` num
    // arquivo que o exercita em quatro linhas.
    //
    // O que isto não distingue é um `.fechar(` em algum outro objeto do vetor.
    // A outra metade deste teste é que fecha essa porta: cada chamada em
    // `SeeleMods`/`SeeleUI` precisa existir no prelúdio, e um vetor que
    // inventasse um receptor para enganar a contagem não estaria exercitando
    // coisa nenhuma — o que a homologação nativa mostra na primeira volta.
    for oferecido in &oferecidos {
        assert!(
            executado.contains(&format!(".{oferecido}(")),
            "o vetor deixou de exercitar `{oferecido}`, que o prelúdio oferece"
        );
    }

    // Toda `forma` que ele declara está na gramática que o produto monta.
    let regiao = arquivo_da_janela("mods-regiao.js");
    let montar = regiao
        .split_once("const FORMAS_DA_REGIAO = Object.freeze({")
        .expect("a gramática da região")
        .1
        .split_once("});")
        .expect("o fim da gramática")
        .0;
    let mut formas = 0;
    for pedaco in cliente.split("forma: \"").skip(1) {
        let nome = pedaco.split('"').next().unwrap_or_default();
        assert!(
            montar.contains(&format!("{nome}:")),
            "o vetor declara a forma «{nome}», que `montarODeclarado` não conhece"
        );
        formas += 1;
    }
    assert!(formas >= 4, "o vetor desenha pouco demais para provar algo");

    // E toda chave de tema é uma das quatro.
    let base = base_js();
    let tema = base
        .split_once("const TEMA_DA_API = Object.freeze({")
        .expect("`TEMA_DA_API`")
        .1
        .split_once("});")
        .expect("o fim de `TEMA_DA_API`")
        .0;
    let pedido = cliente
        .split_once("SeeleUI.tema({")
        .expect("o vetor pede tema")
        .1
        .split_once("})")
        .expect("o fim do pedido de tema")
        .0;
    for chave in pedido.split(':').rev().skip(1) {
        let nome = chave
            .rsplit([' ', ',', '{'])
            .next()
            .unwrap_or_default()
            .trim();
        if nome.is_empty() {
            continue;
        }
        assert!(
            tema.contains(&format!("{nome}:")),
            "o vetor pede o token de tema «{nome}», que a API não conhece"
        );
    }
}

/// O prelúdio sem o bloco `globalThis.console = { … };`.
///
/// **Cortado pelo recuo, e não pelo primeiro `};`.** O bloco é a linha que o
/// abre e as linhas recuadas além dela; a primeira linha no recuo de quem o
/// abre tem de ser o `};` que o fecha. Cortar no primeiro `};` depois da abertura erraria
/// sem dizer onde: um console que fechasse sem o `;`, que o JavaScript aceita,
/// faria o corte seguir até o `};` de `marcar`, lá embaixo, e levar junto
/// `SeeleMods` e `SeeleUI` — a API que este guarda existe para cobrar.
///
/// **Não achar é falhar.** Um prelúdio sem a linha que abre o console, ou com
/// um console que não fecha onde devia, reprova aqui pelo nome: calado, o
/// guarda voltaria a cobrar o console do vetor, ou deixaria de cobrar a API.
fn sem_o_console(preludio: &str) -> String {
    const ABRE: &str = "globalThis.console = {";
    let linhas: Vec<&str> = preludio.lines().collect();
    let abre = linhas
        .iter()
        .position(|linha| linha.trim() == ABRE)
        .unwrap_or_else(|| {
            panic!(
                "o prelúdio do executor não tem mais a linha `{ABRE}`, e o guarda do vetor \
                 não sabe onde o console começa: sem cortá-lo, ele cobraria do vetor o \
                 console, que não é API de MOD; ache o console no PRELUDIO de \
                 apps/seele-app/src/executor.rs e conserte `sem_o_console`"
            )
        });
    let (antes, desde_a_abertura) = linhas.split_at(abre);
    let (linha_que_abre, depois_da_abertura) = desde_a_abertura
        .split_first()
        .expect("`position` achou a linha que abre o console");
    let recuo = linha_que_abre
        .trim_end()
        .strip_suffix(ABRE)
        .expect("a linha que abre o console é recuo e marca");
    let dentro_do_bloco = |linha: &str| {
        linha.trim().is_empty()
            || linha
                .strip_prefix(recuo)
                .is_some_and(|resto| resto.starts_with(char::is_whitespace))
    };
    let fecha = depois_da_abertura
        .iter()
        .position(|linha| !dentro_do_bloco(linha))
        .unwrap_or_else(|| {
            panic!(
                "o console do prelúdio abre em `{ABRE}` e nenhuma linha depois volta ao \
                 recuo dele: o guarda do vetor não sabe onde o console termina"
            )
        });
    let (_console, desde_o_fechamento) = depois_da_abertura.split_at(fecha);
    let (linha_que_fecha, depois) = desde_o_fechamento
        .split_first()
        .expect("`position` achou a linha que fecha o console");
    assert_eq!(
        *linha_que_fecha,
        format!("{recuo}}};"),
        "o console do prelúdio não fecha com `}};` no recuo de quem o abre, e o guarda \
         do vetor não sabe onde ele termina: cortar no próximo `}};` levaria junto a \
         API que vem depois dele"
    );
    antes
        .iter()
        .chain(depois)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// O que sobra de um arquivo JS quando os comentários saem.
///
/// Linha a linha e bloco a bloco, sem entender aspas: nenhum destes vetores tem
/// `//` dentro de uma string, e um analisador de verdade aqui seria mais código
/// do que o que ele guarda.
fn sem_comentarios(fonte: &str) -> String {
    let sem_bloco = {
        let mut saida = String::new();
        let mut resto = fonte;
        while let Some(at) = resto.find("/*") {
            saida.push_str(&resto[..at]);
            let Some(fim) = resto[at..].find("*/") else {
                break;
            };
            resto = &resto[at + fim + 2..];
        }
        saida.push_str(resto);
        saida
    };
    sem_bloco
        .lines()
        .map(|linha| linha.split_once("//").map_or(linha, |(antes, _)| antes))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `SeeleMods.x(` e `SeeleUI.x(` que aparecem num texto.
fn chamadas_de(fonte: &str) -> Vec<(String, String)> {
    let mut saida = Vec::new();
    for objeto in ["SeeleMods", "SeeleUI"] {
        for pedaco in fonte.split(&format!("{objeto}.")).skip(1) {
            let metodo: String = pedaco
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !metodo.is_empty() {
                saida.push((objeto.to_owned(), metodo));
            }
        }
    }
    saida
}

/// Copia uma árvore inteira, criando as pastas do caminho.
fn copiar_arvore(de: &Path, para: &Path) {
    std::fs::create_dir_all(para).expect("criar destino");
    for entrada in std::fs::read_dir(de).expect("ler origem") {
        let entrada = entrada.expect("entrada");
        let origem = entrada.path();
        let destino = para.join(entrada.file_name());
        if origem.is_dir() {
            copiar_arvore(&origem, &destino);
        } else {
            std::fs::copy(&origem, &destino).expect("copiar");
        }
    }
}

/// **O pacote atravessa a ponte inteiro.**
///
/// Instalado pelo caminho do produto — endereçado pelo conteúdo —, e lido de
/// volta por onde `codigo_do_mod` lê. Um instalador que copiasse metade, ou um
/// leitor que servisse o pacote de outro MOD, morre aqui.
#[test]
fn o_vetor_atravessa_a_instalacao_e_volta_byte_a_byte() {
    let raiz = temporario("ponte");
    let lido = seele_ffi::mods::ler_pasta(RAIZ).expect("o vetor tem de ser um pacote válido");
    let destino = raiz.join(seele_ffi::mods::PACOTES).join(&lido.hash);
    // **A árvore inteira, e não uma lista escrita aqui.** A lista dizia três
    // arquivos, e no dia em que o vetor passou a trazer um som ela continuou
    // dizendo três: o pacote chegava do outro lado sem o som, o hash não batia,
    // e a mensagem falava de conteúdo em vez de falar do que faltou. Um
    // instalador que copia metade é exatamente o que este teste existe para
    // pegar — e ele só pega se for o instalador a decidir o que copiar.
    copiar_arvore(Path::new(RAIZ), &destino);

    let de_volta = seele_ffi::mods::ler_por_hash(&raiz.to_string_lossy(), &lido.hash)
        .expect("o pacote publicado tem de ser lido de volta");
    assert_eq!(de_volta.id, "seele/referencia");
    assert_eq!(
        de_volta.hash, lido.hash,
        "o hash mudou entre publicar e ler de volta"
    );
    let _ = std::fs::remove_dir_all(&raiz);
}
