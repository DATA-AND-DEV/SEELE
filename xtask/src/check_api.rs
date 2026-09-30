//! Enforces the MOD API façade from `api/`.
//!
//! ADR 0045. The question this asks, and the direction is the whole point:
//!
//! > Does every name in `api/vN.json` still point at something?
//!
//! And never the reverse. A check that asked "did every type reach the API?"
//! would drag the API along behind the protocol, and renaming a field inside
//! would repaint the façade and break every published MOD — which is exactly
//! the defect this file exists to prevent.
//!
//! Same argument as `check_deps`: "a contract checked only by review is a
//! contract that erodes".
//!
//! # Textual, and what that costs
//!
//! It looks for the symbol in the concatenated sources. A real resolver would
//! need the compiler, and what this has to catch — a rename — changes the text.
//! The cost is that it finds a symbol anywhere in the repository rather than
//! exactly where the mapping claims: it guards against a name **disappearing**,
//! not against it moving.
//!
//! # E, a partir da API 6, quem despacha
//!
//! A mesma pergunta, na mesma direção, feita ao bloco `moments`: **todo
//! momento que este arquivo promete ainda tem quem o despache?** A v1 prometeu
//! 21 momentos e `momento_de` entrega 5, e nenhum guarda percebeu. Ver
//! [`PRIMEIRA_API_COBRADA`] para por que a cobrança começa na 6.

use std::collections::BTreeSet;
use std::process::ExitCode;

/// One MOD-facing name that no longer resolves.
type Violation = String;

/// Pure rule evaluation, kept free of the filesystem so it can be tested.
fn evaluate(mapa: &[(&str, &str)], fonte: &str) -> Vec<Violation> {
    let mut violacoes = Vec::new();
    for (nome, interior) in mapa {
        let alvo = interior.rsplit("::").next().unwrap_or(interior);
        if !fonte.contains(alvo) {
            violacoes.push(format!(
                "`{nome}` aponta para `{interior}`, e `{alvo}` não existe mais. \
                 Conserte o mapeamento em `api/`, nunca o nome que o MOD escreve."
            ));
        }
    }
    violacoes
}

/// Onde mora `momento_de`, o único despachante de momentos do servidor.
///
/// O cabeçalho do `despacho.rs` explica por que é um só: um assinante do
/// barramento, e não vinte e um ganchos espalhados.
const DESPACHO: &str = "crates/seele-server/src/mods/despacho.rs";

/// A primeira API cuja promessa o `check-api` cobra contra quem a despacha.
///
/// # Por que não da 1 à 5
///
/// Porque elas estão congeladas (`api/congeladas.sha256`), e cobrar uma
/// promessa que não se pode editar é montar um guarda que só sabe ficar
/// vermelho. A v1 prometeu 21 momentos e `momento_de` entrega 5. O conserto que
/// o guarda pediria — tirar os outros 16 da v1 — é justamente o que o
/// congelamento proíbe, e nem consertaria nada: um MOD de API 1 que espera
/// `ChannelCreated` nunca foi chamado, e apagar a promessa não o faria ser. A
/// saída é a da especificação: a v6 promete só o que é entregue, e é dela em
/// diante que isso se cobra.
///
/// # E toda versão a partir da 6, e não só a mais nova
///
/// A v6 continua aceita depois que a v7 existir, e tirar o despachante de um
/// momento que ela promete quebra os MODs dela do mesmo jeito. Congelada, ela
/// não pode mudar a lista; por isso a cobrança recai sobre quem despacha, que é
/// o lado que ainda pode mudar.
const PRIMEIRA_API_COBRADA: u64 = 6;

/// Um bloco de `api/vN.json` que lista coisas que alguém tem de despachar.
struct Promessa<'a> {
    /// A chave do descritor que lista o prometido: `moments`, por exemplo.
    bloco: &'a str,
    /// Quem despacha, dito na violação: é o que a pessoa vai abrir para
    /// consertar.
    despachante: &'a str,
    /// O que esse despachante de fato entrega.
    entregues: &'a BTreeSet<String>,
}

/// O número da versão pelo nome do arquivo: `api/v6.json` → 6.
///
/// Pelo nome, e não pelo campo `version` de dentro: o nome é o que
/// `api/congeladas.sha256` lista, e os dois guardas precisam concordar sobre
/// qual arquivo é qual versão. Por isso a grafia é a mesma que o guarda de
/// congelamento aceita: `v6.json`, e não `v06.json`. Um `.json` com outro nome
/// não é cobrado aqui, e reprova no guarda de congelamento.
fn versao_do_arquivo(caminho: &std::path::Path) -> Option<u64> {
    let nome = caminho.file_stem()?.to_str()?;
    let digitos = nome.strip_prefix('v')?;
    if digitos.is_empty()
        || digitos.starts_with('0')
        || !digitos.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    digitos.parse().ok()
}

/// Os momentos que `momento_de` entrega, lidos do texto de [`DESPACHO`].
///
/// Cada braço do `match` devolve `("NomeDoMomento", carga)`, então o nome é a
/// primeira coisa depois de `=> (`. Tanto faz se o braço cabe numa linha ou se o
/// `rustfmt` o quebrou. Só o corpo de `momento_de` conta: ele acaba na primeira
/// chave fechada na coluna zero, que é como o `rustfmt` fecha uma função de
/// topo. Um nome citado em outra função do arquivo não é despachado por esta.
///
/// Dentro do corpo, uma linha de comentário `//` não conta: um braço comentado
/// não despacha nada. Um comentário `/* … */` não é filtrado, e um nome citado
/// nele na forma `=> ("Nome"` contaria como despachado.
///
/// Um braço fora dessa forma — o corpo num bloco `=> { … ("Nome", carga) }`,
/// ou o nome numa constante, `=> (NOME, carga)` — não é achado, e a promessa
/// dele reprova com «não é despachado». É o lado certo de errar, e é por isso
/// que a violação diz a forma que o extrator procura.
///
/// Textual pelo mesmo motivo do resto deste arquivo (ver o cabeçalho). O custo
/// aqui é que uma mudança de forma — os nomes numa tabela, digamos — faz o
/// extrator não achar nada. Por isso `None` e o conjunto vazio reprovam no
/// [`run`]: sem despachante conhecido não há contra o que cobrar, e isso é
/// reprovação, não aprovação.
fn momentos_despachados(despacho: &str) -> Option<BTreeSet<String>> {
    let (_, depois) = despacho.split_once("fn momento_de(")?;
    let corpo = depois.split_once("\n}").map_or(depois, |(corpo, _)| corpo);
    let corpo = corpo
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let nomes = corpo
        .split("=> (")
        .skip(1)
        .filter_map(|braco| {
            let resto = braco.trim_start().strip_prefix('"')?;
            let (nome, _) = resto.split_once('"')?;
            let mut letras = nome.chars();
            let primeira = letras.next()?;
            (primeira.is_ascii_uppercase() && letras.all(|c| c.is_ascii_alphanumeric()))
                .then(|| nome.to_owned())
        })
        .collect();
    Some(nomes)
}

/// O que uma versão promete num bloco e ninguém despacha.
///
/// A mesma direção de [`evaluate`]: do arquivo para o código. Um nome que o
/// despachante entrega e nenhuma versão lista não reprova — o protocolo cresce,
/// e a fachada escolhe o que mostrar.
///
/// Versões abaixo de [`PRIMEIRA_API_COBRADA`] não são cobradas. Uma versão
/// cobrada que não traz o bloco reprova: o `extends` não é seguido aqui.
fn sem_despachante(
    versao: u64,
    descritor: &serde_json::Value,
    promessa: &Promessa<'_>,
) -> Vec<Violation> {
    if versao < PRIMEIRA_API_COBRADA {
        return Vec::new();
    }
    let bloco = promessa.bloco;
    let Some(lista) = descritor.get(bloco).and_then(serde_json::Value::as_array) else {
        return vec![format!(
            "a API {versao} não lista `{bloco}`. A partir da {PRIMEIRA_API_COBRADA}, cada \
             versão diz por inteiro o que promete: o `extends` não é seguido aqui, e herdar \
             por ele traria de volta o que uma versão congelada prometeu e nunca foi \
             entregue. Escreva a lista, ainda que vazia."
        )];
    };
    let mut violacoes = Vec::new();
    for item in lista {
        let Some(nome) = item.as_str() else {
            violacoes.push(format!(
                "a API {versao} lista em `{bloco}` algo que não é um nome: `{item}`"
            ));
            continue;
        };
        if !promessa.entregues.contains(nome) {
            violacoes.push(format!(
                "a API {versao} promete `{nome}` em `{bloco}`, e ele não é despachado por {}. \
                 Ou passa a ser, ou sai da lista antes de a versão ser publicada: um MOD que \
                 espera por `{nome}` esperaria para sempre, sem erro nenhum.",
                promessa.despachante
            ));
        }
    }
    violacoes
}

/// A cobrança de um arquivo de `api/`: `None` se ele não é cobrado, e as
/// violações de todas as `promessas` se é — vazias quando ele cumpre.
///
/// Não é cobrado o arquivo cujo nome não é uma versão ([`versao_do_arquivo`]),
/// nem a versão abaixo de [`PRIMEIRA_API_COBRADA`].
///
/// Fora do [`run`] para que a ligação dele com [`sem_despachante`] tenha teste:
/// sem ela, os testes da regra continuariam verdes e o `check-api` aprovaria
/// uma v6 sem conferir nada. E o `run` conta as cobradas pelo `Some` daqui, da
/// mesma decisão que cobra, e não por uma conta à parte: o resumo não pode
/// dizer que cobrou uma versão que não cobrou.
fn cobrar(
    caminho: &std::path::Path,
    descritor: &serde_json::Value,
    promessas: &[&Promessa<'_>],
) -> Option<Vec<Violation>> {
    let versao = versao_do_arquivo(caminho)?;
    if versao < PRIMEIRA_API_COBRADA {
        return None;
    }
    Some(
        promessas
            .iter()
            .flat_map(|promessa| sem_despachante(versao, descritor, promessa))
            .collect(),
    )
}

/// Reads `api/*.json` and every crate source, and reports orphans.
///
/// E, a partir da API 6, cobra de cada versão que o que ela promete em
/// `moments` tenha quem despache (ver [`PRIMEIRA_API_COBRADA`]).
pub(crate) fn run() -> ExitCode {
    let raiz = match std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        Some(caminho) => caminho.to_path_buf(),
        None => {
            eprintln!("check-api: não achei a raiz do repositório");
            return ExitCode::FAILURE;
        }
    };

    let mut fonte = String::new();
    for pasta in ["crates", "apps"] {
        colher(&raiz.join(pasta), &mut fonte);
    }

    // Quem entrega os momentos. Lido mesmo sem API 6 para cobrar, de propósito:
    // um extrator que deixou de achar a função tem de ficar vermelho hoje, e não
    // no dia em que a v6 depender dele.
    let Ok(despacho) = std::fs::read_to_string(raiz.join(DESPACHO)) else {
        eprintln!("check-api: não deu para ler {DESPACHO}");
        return ExitCode::FAILURE;
    };
    let momentos = match momentos_despachados(&despacho) {
        Some(momentos) if !momentos.is_empty() => momentos,
        _ => {
            eprintln!(
                "check-api: não achei os momentos de `fn momento_de` em {DESPACHO}. Se ela \
                 mudou de nome, de lugar ou de forma, conserte `DESPACHO` e \
                 `momentos_despachados` aqui: sem eles a promessa de momentos da API 6 não \
                 tem contra o que ser conferida."
            );
            return ExitCode::FAILURE;
        }
    };
    let quem_despacha_momentos =
        format!("nenhum braço `=> (\"Nome\", carga)` de `momento_de` ({DESPACHO})");
    let promessa_de_momentos = Promessa {
        bloco: "moments",
        despachante: &quem_despacha_momentos,
        entregues: &momentos,
    };

    let mut houve = false;
    let Ok(entradas) = std::fs::read_dir(raiz.join("api")) else {
        eprintln!("check-api: não achei `api/`");
        return ExitCode::FAILURE;
    };
    let mut versoes = 0_usize;
    let mut cobradas = 0_usize;
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        versoes += 1;
        let Ok(texto) = std::fs::read_to_string(&caminho) else {
            eprintln!("check-api: não deu para ler {}", caminho.display());
            return ExitCode::FAILURE;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&texto) else {
            eprintln!("check-api: {} não é json", caminho.display());
            return ExitCode::FAILURE;
        };

        let mut mapa: Vec<(String, String)> = Vec::new();
        for bloco in ["reads", "actions", "own", "world"] {
            if let Some(obj) = json.get(bloco).and_then(serde_json::Value::as_object) {
                for (nome, interior) in obj {
                    if let Some(interior) = interior.as_str() {
                        mapa.push((nome.clone(), interior.to_owned()));
                    }
                }
            }
        }
        let emprestado: Vec<(&str, &str)> = mapa
            .iter()
            .map(|(nome, interior)| (nome.as_str(), interior.as_str()))
            .collect();
        for violacao in evaluate(&emprestado, &fonte) {
            eprintln!("check-api: {} — {violacao}", caminho.display());
            houve = true;
        }

        if let Some(violacoes) = cobrar(&caminho, &json, &[&promessa_de_momentos]) {
            cobradas += 1;
            for violacao in violacoes {
                eprintln!("check-api: {} — {violacao}", caminho.display());
                houve = true;
            }
        }
    }

    if houve {
        ExitCode::FAILURE
    } else {
        println!(
            "check-api: toda a superfície de MOD ainda aponta para algo ({versoes} \
             versão(ões)); `momento_de` despacha {} momento(s), cobrados em {cobradas} \
             versão(ões) a partir da {PRIMEIRA_API_COBRADA}.",
            momentos.len()
        );
        ExitCode::SUCCESS
    }
}

/// Concatenates every source file under `dir` that the façade can point into.
///
/// # Por que `.js` também, desde a API 4
///
/// Metade da superfície que um MOD enxerga é montada na janela: o renderer, o
/// host de superfícies e o registro de contribuições são JavaScript. Enquanto
/// esta função colhia só `.rs`, os nomes daquela metade **passavam por
/// acidente** — `desenharARegiaoDoMod` resolvia porque ele aparece citado num
/// guarda de `tests/frontend.rs`, e não porque a função existe.
///
/// Um guarda que passa por acidente é um guarda que deixa de passar quando o
/// acidente sai, e reprova sem que nada de verdade tenha mudado. Pior: ele não
/// pega o que existe para pegar — renomear `SuperficieDeMod.montar` não
/// produziria linha vermelha nenhuma.
///
/// Só `apps/seele-app/ui/` entra: é onde a janela do produto mora, e varrer
/// `node_modules` de um repositório vizinho seria fazer a fachada resolver
/// contra código que não é nosso.
fn colher(dir: &std::path::Path, destino: &mut String) {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            let nome = caminho.file_name().and_then(|n| n.to_str());
            if nome == Some("target") || nome == Some("node_modules") {
                continue;
            }
            colher(&caminho, destino);
        } else if matches!(
            caminho.extension().and_then(|e| e.to_str()),
            Some("rs" | "js")
        ) {
            if let Ok(texto) = std::fs::read_to_string(&caminho) {
                destino.push_str(&texto);
                destino.push('\n');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_nome_que_aponta_para_algo_que_existe_passa() {
        let fonte = "pub struct Person { pub nick: String }";
        assert!(evaluate(&[("pessoa.apelido", "Person::nick")], fonte).is_empty());
    }

    /// O guarda inteiro, numa frase: renomear por dentro fica vermelho **aqui**,
    /// e não na máquina de quem escreveu um MOD seis meses atrás.
    #[test]
    fn um_nome_que_aponta_para_o_que_sumiu_reprova() {
        let fonte = "pub struct Person { pub apelido: String }";
        let violacoes = evaluate(&[("pessoa.apelido", "Person::nick")], fonte);
        assert_eq!(violacoes.len(), 1);
        let primeira = violacoes.first().map_or("", String::as_str);
        assert!(primeira.contains("Person::nick"));
        assert!(
            primeira.contains("pessoa.apelido"),
            "a violação não diz qual nome de MOD ficou órfão"
        );
    }

    /// A direção importa, e é a correção que o dono achou no desenho:
    /// acrescentar coisa ao protocolo **não** reprova. Se reprovasse, todo tipo
    /// novo empurraria a API para a frente e a fachada viraria projeção de novo.
    #[test]
    fn um_tipo_novo_no_interior_que_a_api_nao_menciona_nao_reprova() {
        let fonte = "pub struct Person { pub nick: String, pub retrato: Vec<u8> }";
        assert!(evaluate(&[("pessoa.apelido", "Person::nick")], fonte).is_empty());
    }

    /// Um `momento_de` com a forma do de verdade: um braço quebrado pelo
    /// `rustfmt` e outro numa linha só. A função vizinha devolve outro nome do
    /// mesmo jeito, e ele não pode contar — quem não está no corpo de
    /// `momento_de` não é despachado por ela. Nem o braço comentado, que está no
    /// corpo e não despacha nada.
    const DESPACHO_DE_FIXTURE: &str = r##"
pub fn momento_de(evento: &crate::server::Event) -> Option<(&'static str, String)> {
    use crate::server::Event;

    let (nome, carga) = match evento {
        Event::PersonJoined { voice_room, .. } => (
            "PersonJoined",
            format!(r#"{{"sala":{}}}"#, voice_room.0),
        ),
        Event::PersonGone { person } => ("PersonGone", format!(r#"{{"pessoa":{}}}"#, person.0)),
        // Event::ServerRenamed { .. } => ("ServerRenamed", String::new()),
        _ => return None,
    };
    Some((nome, carga))
}

fn vizinha(x: u8) -> (&'static str, u8) {
    match x {
        _ => ("ScreenShareStarted", x),
    }
}
"##;

    fn momentos_de_fixture() -> BTreeSet<String> {
        momentos_despachados(DESPACHO_DE_FIXTURE).unwrap_or_default()
    }

    fn promessa_de_momentos(entregues: &BTreeSet<String>) -> Promessa<'_> {
        Promessa {
            bloco: "moments",
            despachante: "`momento_de`",
            entregues,
        }
    }

    #[test]
    fn o_numero_da_versao_vem_do_nome_do_arquivo() {
        use std::path::Path;
        assert_eq!(versao_do_arquivo(Path::new("api/v6.json")), Some(6));
        assert_eq!(versao_do_arquivo(Path::new("api/v12.json")), Some(12));
        assert_eq!(versao_do_arquivo(Path::new("api/rascunho.json")), None);
        assert_eq!(versao_do_arquivo(Path::new("api/v.json")), None);
        assert_eq!(
            versao_do_arquivo(Path::new("api/v06.json")),
            None,
            "`v06.json` foi lido como a API 6, e o guarda de congelamento não o aceita: \
             os dois guardas discordariam sobre qual arquivo é qual versão"
        );
    }

    #[test]
    fn o_extrator_le_so_o_corpo_de_momento_de() {
        assert_eq!(
            momentos_de_fixture(),
            BTreeSet::from(["PersonGone".to_owned(), "PersonJoined".to_owned()]),
            "o extrator leu além de `momento_de` ou contou um braço comentado, ou perdeu um \
             braço dela — o quebrado ou o de uma linha só"
        );
    }

    #[test]
    fn sem_momento_de_nao_ha_contra_o_que_cobrar() {
        assert!(
            momentos_despachados("fn outra() -> u8 {\n    0\n}\n").is_none(),
            "sem `momento_de` o extrator devolveu um conjunto, e o `run` aprovaria sem ter \
             conferido nada"
        );
    }

    /// O extrator contra o `despacho.rs` de verdade. Se isto reprova, ou
    /// `momento_de` mudou de forma e o extrator precisa acompanhar, ou um
    /// momento que é entregue hoje deixou de ser.
    #[test]
    fn o_extrator_acha_os_momentos_do_momento_de_de_verdade() {
        let despacho = include_str!("../../crates/seele-server/src/mods/despacho.rs");
        let momentos = momentos_despachados(despacho).unwrap_or_default();
        for nome in [
            "PersonJoined",
            "PersonLeft",
            "MessageReceived",
            "MessageEdited",
            "MessageRemoved",
        ] {
            assert!(
                momentos.contains(nome),
                "o extrator não achou `{nome}` no `momento_de` de verdade: ou a função mudou \
                 de forma (conserte `momentos_despachados`), ou um momento que a v1 prometeu \
                 e era entregue deixou de ser — e os MODs que o esperam ficaram mudos"
            );
        }
    }

    #[test]
    fn a_api_6_que_promete_um_momento_sem_despachante_reprova() {
        let entregues = momentos_de_fixture();
        let v6 = serde_json::json!({
            "version": 6,
            "extends": 5,
            "moments": ["PersonJoined", "ScreenShareStarted"],
        });
        let violacoes = sem_despachante(6, &v6, &promessa_de_momentos(&entregues));
        assert_eq!(
            violacoes.len(),
            1,
            "esperava uma violação, vieram {violacoes:?}"
        );
        let primeira = violacoes.first().map_or("", String::as_str);
        assert!(
            primeira.contains("ScreenShareStarted") && primeira.contains("momento_de"),
            "a violação não diz qual momento ficou sem despachante nem onde despachá-lo: \
             {primeira}"
        );
    }

    #[test]
    fn a_api_6_que_so_promete_o_que_e_despachado_passa() {
        let entregues = momentos_de_fixture();
        let v6 = serde_json::json!({ "version": 6, "moments": ["PersonJoined", "PersonGone"] });
        let violacoes = sem_despachante(6, &v6, &promessa_de_momentos(&entregues));
        assert!(
            violacoes.is_empty(),
            "uma v6 que só promete o que é entregue reprovou: {violacoes:?}"
        );
    }

    #[test]
    fn a_api_6_que_nao_lista_os_momentos_reprova() {
        let entregues = momentos_de_fixture();
        let v6 = serde_json::json!({ "version": 6, "extends": 5 });
        let violacoes = sem_despachante(6, &v6, &promessa_de_momentos(&entregues));
        assert_eq!(
            violacoes.len(),
            1,
            "esperava uma violação, vieram {violacoes:?}"
        );
        assert!(
            violacoes.first().is_some_and(|v| v.contains("extends")),
            "uma v6 sem `moments` passou, e herdaria pelo `extends` os momentos da v1 que \
             nunca foram entregues: {violacoes:?}"
        );
    }

    /// As congeladas não são cobradas, e a prova usa a v1 de verdade: 21
    /// momentos, dos quais este fixture entrega 2.
    ///
    /// As versões vão escritas por extenso, e não derivadas de
    /// [`PRIMEIRA_API_COBRADA`]: um laço `1..PRIMEIRA_API_COBRADA` passaria com
    /// qualquer valor dela — com 1, o laço fica vazio e as congeladas passam a
    /// ser cobradas sem este teste perceber.
    #[test]
    fn as_apis_congeladas_nao_sao_cobradas() {
        let Ok(v1) = serde_json::from_str::<serde_json::Value>(include_str!("../../api/v1.json"))
        else {
            panic!("api/v1.json deixou de ser JSON");
        };
        let entregues = momentos_de_fixture();
        let promessa = promessa_de_momentos(&entregues);
        for versao in 1..=5 {
            assert!(
                sem_despachante(versao, &v1, &promessa).is_empty(),
                "a API {versao} está congelada e foi cobrada: a promessa dela não se edita, \
                 e um guarda sobre ela só saberia ficar vermelho"
            );
        }
        // A isenção é pela versão, e não pelo conteúdo: a mesma lista cobrada
        // como 6 reprova nos 19 que o fixture não entrega.
        assert_eq!(
            sem_despachante(6, &v1, &promessa).len(),
            19,
            "a lista da v1 cobrada como API 6 devia reprovar nos 21 momentos menos os 2 que \
             o fixture entrega: ou a 6 deixou de ser cobrada, ou o extrator passou a contar \
             um nome que o fixture não despacha"
        );
    }

    /// Uma v6 no disco, com um momento que ninguém despacha: o descritor dos
    /// três testes de [`cobrar`] abaixo, que só mudam o nome do arquivo.
    fn descritor_com_um_momento_sem_despachante() -> serde_json::Value {
        serde_json::json!({ "version": 6, "moments": ["PersonJoined", "ScreenShareStarted"] })
    }

    /// A ligação do `run` com a cobrança. Sem ela, os testes de
    /// [`sem_despachante`] continuam verdes e o `check-api` aprova uma v6 com
    /// um momento sem despachante — e o resumo ainda conta a v6 como cobrada.
    #[test]
    fn a_api_6_no_disco_e_cobrada() {
        let entregues = momentos_de_fixture();
        let Some(violacoes) = cobrar(
            std::path::Path::new("api/v6.json"),
            &descritor_com_um_momento_sem_despachante(),
            &[&promessa_de_momentos(&entregues)],
        ) else {
            panic!(
                "`api/v6.json` não foi cobrada: o `check-api` aprovaria a promessa dela sem \
                 conferir nada, e o resumo não a contaria"
            );
        };
        assert_eq!(
            violacoes.len(),
            1,
            "`api/v6.json` foi dada por cobrada, mas `ScreenShareStarted` sem despachante não \
             reprovou: o resumo diria que cobrou uma versão que não cobrou. Vieram \
             {violacoes:?}"
        );
    }

    /// A mesma promessa num arquivo de API congelada não é cobrada, e por isso
    /// não entra na conta de `cobradas` do resumo.
    #[test]
    fn a_api_5_no_disco_nao_e_cobrada() {
        let entregues = momentos_de_fixture();
        let cobranca = cobrar(
            std::path::Path::new("api/v5.json"),
            &descritor_com_um_momento_sem_despachante(),
            &[&promessa_de_momentos(&entregues)],
        );
        assert!(
            cobranca.is_none(),
            "`api/v5.json` está congelada e foi dada por cobrada: o resumo do `check-api` \
             contaria uma cobrança que não houve. Veio {cobranca:?}"
        );
    }

    /// Um `.json` cujo nome não é uma versão não é cobrado aqui: é o guarda de
    /// congelamento que o recusa.
    #[test]
    fn um_json_que_nao_e_versao_nao_e_cobrado() {
        let entregues = momentos_de_fixture();
        let cobranca = cobrar(
            std::path::Path::new("api/rascunho.json"),
            &descritor_com_um_momento_sem_despachante(),
            &[&promessa_de_momentos(&entregues)],
        );
        assert!(
            cobranca.is_none(),
            "`api/rascunho.json` foi cobrado como se fosse uma versão, sem número nenhum no \
             nome: o `check-api` e o guarda de congelamento discordariam sobre o que é uma \
             versão. Veio {cobranca:?}"
        );
    }
}
