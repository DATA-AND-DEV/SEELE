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
//! A mesma pergunta, na mesma direção, feita aos blocos `moments` e
//! `eventos`: **todo momento e todo evento que este arquivo promete ainda têm
//! quem os despache?** Os momentos, `momento_de` no servidor; os eventos, o
//! `falar` da janela. A v1 prometeu 21 momentos e `momento_de` entrega 5, e
//! nenhum guarda percebeu. Ver [`PRIMEIRA_API_COBRADA`] para por que a
//! cobrança começa na 6.

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
    despachante: String,
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

/// Onde a janela entrega eventos a um MOD.
const JANELA: &str = "apps/seele-app/ui";

/// Os eventos que a janela entrega a um MOD: o `nome` de cada
/// `falar({ nome: "…" })` em [`JANELA`].
///
/// `falar` é o caminho, e o `base.js` diz por quê: é o dono da região quem
/// confere a instância e a geração antes de entregar o evento. Um evento
/// entregue de outro jeito — `falar(evento)` com o objeto montado antes,
/// digamos — não é achado aqui, e a promessa dele reprova. É o lado certo de
/// errar: quem abriu o caminho novo descobre aqui, e não quem escreveu o MOD.
///
/// Só a janela entra, e não os testes: um nome citado num guarda de
/// `tests/frontend.rs` resolveria por acidente, que é o defeito que [`colher`]
/// conta sobre os `.js`.
///
/// E pelo mesmo motivo textual do cabeçalho, um `falar({ nome: "…" })` citado
/// num comentário da janela também conta como despachado. Um exemplo em
/// comentário não escreve essa forma literal.
fn eventos_despachados(janela: &str) -> BTreeSet<String> {
    janela
        .split("falar({")
        .skip(1)
        .filter_map(|chamada| {
            let resto = chamada.trim_start().strip_prefix("nome:")?;
            let resto = resto.trim_start().strip_prefix('"')?;
            let (nome, _) = resto.split_once('"')?;
            Some(nome.to_owned())
        })
        .collect()
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
/// violações das duas `promessas` se é — vazias quando ele cumpre.
///
/// Não é cobrado o arquivo cujo nome não é uma versão ([`versao_do_arquivo`]),
/// nem a versão abaixo de [`PRIMEIRA_API_COBRADA`].
///
/// Fora do [`run`] para que a ligação dele com [`sem_despachante`] tenha teste:
/// sem ela, os testes da regra continuariam verdes e o `check-api` aprovaria
/// uma v6 sem conferir nada. E o resumo conta as cobradas pelo `Some` daqui, e
/// não por uma conta à parte: ele conta as versões pela mesma fronteira que
/// decide a cobrança.
///
/// O `Some` é só essa decisão sobre a versão, e não diz quais blocos foram
/// conferidos. As `promessas` são duas pelo tipo, e uma fatia vazia ou
/// parcial não compila; que sejam os momentos e os eventos, cada um contra o
/// próprio despachante, é o que [`promessas_cobradas`] monta, e o teste dela
/// confere.
fn cobrar(
    caminho: &std::path::Path,
    descritor: &serde_json::Value,
    promessas: &[Promessa<'_>; 2],
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

/// A promessa de `moments`, contra o que `momento_de` entrega.
///
/// O `run` e os testes montam a promessa por aqui, e por isso a frase que a
/// pessoa lê quando o `check-api` reprova é a que os testes conferem: a forma
/// que [`momentos_despachados`] procura, e onde.
fn promessa_de_momentos(entregues: &BTreeSet<String>) -> Promessa<'_> {
    Promessa {
        bloco: "moments",
        despachante: format!("nenhum braço `=> (\"Nome\", carga)` de `momento_de` ({DESPACHO})"),
        entregues,
    }
}

/// A promessa de `eventos`, contra o que a janela entrega pelo `falar`. A
/// frase, pelo mesmo motivo de [`promessa_de_momentos`]: a forma que
/// [`eventos_despachados`] procura, e onde.
fn promessa_de_eventos(entregues: &BTreeSet<String>) -> Promessa<'_> {
    Promessa {
        bloco: "eventos",
        despachante: format!("nenhum `falar({{ nome: … }})` da janela ({JANELA})"),
        entregues,
    }
}

/// O que o [`run`] cobra de cada versão a partir da [`PRIMEIRA_API_COBRADA`]:
/// os momentos contra `momento_de`, e os eventos contra a janela.
///
/// Numa função, e não escrita no `run`, para que um teste confira que as duas
/// estão aqui, cada uma com o próprio despachante. O tipo só garante que são
/// duas: um par com as duas do mesmo bloco faria o [`cobrar`] dar a versão por
/// cobrada sem conferir o outro.
fn promessas_cobradas<'a>(
    momentos: &'a BTreeSet<String>,
    eventos: &'a BTreeSet<String>,
) -> [Promessa<'a>; 2] {
    [promessa_de_momentos(momentos), promessa_de_eventos(eventos)]
}

/// Tudo o que o `check-api` confere num arquivo de `api/`: os nomes que
/// apontam para o interior ([`evaluate`], contra a `fonte`) e, se a versão é
/// cobrada, as `promessas` ([`cobrar`]). Devolve as violações das duas
/// conferências numa lista só, e se o arquivo foi cobrado.
///
/// Fora do [`run`] para que a volta das violações tenha teste. Quando o
/// processamento morava no `run`, cada conferência marcava a reprovação por
/// conta própria, e apagar a marca da cobrança deixava o `check-api` imprimir
/// a violação e sair com 0, com os testes, o clippy e o próprio `check-api`
/// verdes. Aqui as duas voltam na mesma lista, e o `run` decide a saída por
/// ela, num lugar só.
fn conferir_arquivo(
    caminho: &std::path::Path,
    json: &serde_json::Value,
    fonte: &str,
    promessas: &[Promessa<'_>; 2],
) -> (Vec<Violation>, bool) {
    let mut mapa: Vec<(&str, &str)> = Vec::new();
    for bloco in ["reads", "actions", "own", "world"] {
        if let Some(obj) = json.get(bloco).and_then(serde_json::Value::as_object) {
            for (nome, interior) in obj {
                if let Some(interior) = interior.as_str() {
                    mapa.push((nome.as_str(), interior));
                }
            }
        }
    }
    let mut violacoes = evaluate(&mapa, fonte);
    let cobranca = cobrar(caminho, json, promessas);
    let cobrada = cobranca.is_some();
    violacoes.extend(cobranca.unwrap_or_default());
    (violacoes, cobrada)
}

/// Reads `api/*.json` and every crate source, and reports orphans.
///
/// E, a partir da API 6, cobra de cada versão que o que ela promete em
/// `moments` e em `eventos` tenha quem despache (ver [`PRIMEIRA_API_COBRADA`]).
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

    // Quem entrega os eventos: a janela, pelo `falar` do dono da região. Lida
    // mesmo sem API 6 para cobrar, pelo mesmo motivo dos momentos.
    let mut janela = String::new();
    colher(&raiz.join(JANELA), &mut janela);
    let eventos = eventos_despachados(&janela);
    if eventos.is_empty() {
        eprintln!(
            "check-api: não achei nenhum `falar({{ nome: \"…\" }})` em {JANELA}. Se a janela \
             passou a entregar eventos de outro jeito, conserte `eventos_despachados` aqui: \
             sem ele a promessa de eventos da API 6 não tem contra o que ser conferida."
        );
        return ExitCode::FAILURE;
    }
    let promessas = promessas_cobradas(&momentos, &eventos);

    let mut houve = false;
    let Ok(entradas) = std::fs::read_dir(raiz.join("api")) else {
        eprintln!("check-api: não achei `api/`");
        return ExitCode::FAILURE;
    };
    let mut versoes = 0_usize;
    let mut cobradas = 0_usize;
    for entrada in entradas {
        // Pulada, a entrada levaria junto a cobrança da versão que ela fosse, e
        // o resumo contaria uma versão a menos sem dizer por quê.
        let entrada = match entrada {
            Ok(entrada) => entrada,
            Err(erro) => {
                eprintln!(
                    "check-api: uma entrada de `api/` não se lê ({erro}), e uma versão nela \
                     ficaria sem cobrança"
                );
                return ExitCode::FAILURE;
            }
        };
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

        let (violacoes, cobrada) = conferir_arquivo(&caminho, &json, &fonte, &promessas);
        cobradas += usize::from(cobrada);
        for violacao in &violacoes {
            eprintln!("check-api: {} — {violacao}", caminho.display());
        }
        houve |= !violacoes.is_empty();
    }

    if houve {
        ExitCode::FAILURE
    } else {
        println!(
            "check-api: toda a superfície de MOD ainda aponta para algo ({versoes} \
             versão(ões)); `momento_de` despacha {} momento(s) e a janela, {} evento(s), \
             cobrados em {cobradas} versão(ões) a partir da {PRIMEIRA_API_COBRADA}.",
            momentos.len(),
            eventos.len()
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

    /// Uma v6 no disco, com um momento que ninguém despacha e a lista de
    /// eventos vazia: o descritor dos três testes de [`cobrar`] abaixo, que só
    /// mudam o nome do arquivo.
    fn descritor_com_um_momento_sem_despachante() -> serde_json::Value {
        serde_json::json!({
            "version": 6,
            "moments": ["PersonJoined", "ScreenShareStarted"],
            "eventos": [],
        })
    }

    /// A ligação de [`cobrar`] com [`sem_despachante`]. Sem ela, os testes da
    /// regra continuam verdes e a v6 sai dada por cobrada sem que o momento sem
    /// despachante reprove.
    #[test]
    fn a_api_6_no_disco_e_cobrada() {
        let entregues = momentos_de_fixture();
        let Some(violacoes) = cobrar(
            std::path::Path::new("api/v6.json"),
            &descritor_com_um_momento_sem_despachante(),
            &promessas_cobradas(&entregues, &BTreeSet::new()),
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
            &promessas_cobradas(&entregues, &BTreeSet::new()),
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
            &promessas_cobradas(&entregues, &BTreeSet::new()),
        );
        assert!(
            cobranca.is_none(),
            "`api/rascunho.json` foi cobrado como se fosse uma versão, sem número nenhum no \
             nome: o `check-api` e o guarda de congelamento discordariam sobre o que é uma \
             versão. Veio {cobranca:?}"
        );
    }

    /// O arquivo inteiro, como o `run` o confere: uma v6 que promete um evento
    /// sem despachante volta com a violação e dada por cobrada. É da lista
    /// devolvida aqui que o `run` tira a reprovação; uma violação que ficasse
    /// fora dela seria impressa, ou nem isso, e o `check-api` sairia com 0.
    #[test]
    fn uma_api_6_que_falha_volta_com_a_violacao_e_cobrada() {
        let momentos = momentos_de_fixture();
        let eventos = eventos_despachados(JANELA_DE_FIXTURE);
        let v6 = serde_json::json!({
            "version": 6,
            "moments": ["PersonJoined"],
            "eventos": ["botao", "arrastar"],
        });
        let (violacoes, cobrada) = conferir_arquivo(
            std::path::Path::new("api/v6.json"),
            &v6,
            "",
            &promessas_cobradas(&momentos, &eventos),
        );
        assert!(
            cobrada,
            "`api/v6.json` não foi dada por cobrada: o resumo do `check-api` contaria uma \
             versão a menos"
        );
        assert!(
            violacoes.len() == 1 && violacoes.first().is_some_and(|v| v.contains("arrastar")),
            "a v6 promete `arrastar`, que ninguém despacha, e a violação não voltou ao `run`: \
             o `check-api` sairia com 0. Vieram {violacoes:?}"
        );
    }

    /// A outra metade do arquivo: um nome de uma versão congelada que aponta
    /// para o que sumiu volta como violação, e a versão não é dada por cobrada.
    #[test]
    fn um_nome_orfao_numa_api_congelada_volta_com_a_violacao_e_sem_cobranca() {
        let momentos = momentos_de_fixture();
        let eventos = eventos_despachados(JANELA_DE_FIXTURE);
        let v5 = serde_json::json!({
            "version": 5,
            "reads": { "pessoa.apelido": "Person::nick" },
        });
        let (violacoes, cobrada) = conferir_arquivo(
            std::path::Path::new("api/v5.json"),
            &v5,
            "pub struct Person { pub apelido: String }",
            &promessas_cobradas(&momentos, &eventos),
        );
        assert!(
            !cobrada,
            "`api/v5.json` está congelada e foi dada por cobrada: o resumo contaria uma \
             cobrança que não houve"
        );
        assert!(
            violacoes.len() == 1
                && violacoes
                    .first()
                    .is_some_and(|v| v.contains("pessoa.apelido") && v.contains("Person::nick")),
            "`pessoa.apelido` aponta para `Person::nick`, que sumiu, e a violação não voltou ao \
             `run`: o `check-api` sairia com 0. Vieram {violacoes:?}"
        );
    }

    /// As três formas que a janela usa hoje para entregar um evento — numa
    /// linha, quebrada, e de dentro de uma arrow function —, um `falar` com
    /// objeto montado antes, que não conta, e um `falar({` que não começa por
    /// `nome` seguido de um `nome` de outro objeto, que não pode contar.
    const JANELA_DE_FIXTURE: &str = r#"
this.dono.falar({ nome: "botao", chave: plano.no.chave ?? "" });
this.dono.falar({
  nome: "fechar-pedido",
  superficie: this.chave,
});
const dizer = (extra) => donoDaRegiao(mod, instancia).falar({ nome: "link", chave, ...extra });
dono.falar(eventoMontadoAntes);
dono.falar({ ...eventoMontadoAntes, chave });
const PERFIS = { regiao: { nome: "regiao" } };
"#;

    #[test]
    fn o_extrator_de_eventos_acha_as_tres_formas_e_so_elas() {
        assert_eq!(
            eventos_despachados(JANELA_DE_FIXTURE),
            BTreeSet::from([
                "botao".to_owned(),
                "fechar-pedido".to_owned(),
                "link".to_owned(),
            ]),
            "o extrator de eventos perdeu uma das formas de `falar`, ou contou um `nome` que \
             não é evento"
        );
    }

    /// O extrator contra a janela de verdade, cobrindo as quatro formas que
    /// existem nela hoje: numa linha (`botao`), quebrada (`campo`,
    /// `fechar-pedido`), dentro de uma arrow function (`link`) e chamada direto
    /// sobre `donoDaRegiao(…)` (`acao`).
    ///
    /// Os nomes conferidos são os 16 `eventos` de `api/v5.json`. O `check-api`
    /// não cobra a v5, que está congelada. Mas a janela entrega hoje os 16
    /// eventos dela, e a v5 continua aceita: este teste guarda que continue,
    /// cobrando do lado que ainda pode mudar.
    #[test]
    fn o_extrator_acha_os_eventos_da_janela_de_verdade() {
        let Ok(v5) = serde_json::from_str::<serde_json::Value>(include_str!("../../api/v5.json"))
        else {
            panic!("api/v5.json deixou de ser JSON");
        };
        let prometidos: Vec<&str> = v5
            .get("eventos")
            .and_then(serde_json::Value::as_array)
            .map_or_else(Vec::new, |lista| {
                lista.iter().filter_map(serde_json::Value::as_str).collect()
            });
        assert_eq!(
            prometidos.len(),
            16,
            "o teste não leu os 16 eventos de `api/v5.json` — leu {prometidos:?} —, e sem eles \
             o laço abaixo passaria sem conferir nada"
        );
        let janela = [
            include_str!("../../apps/seele-app/ui/base.js"),
            include_str!("../../apps/seele-app/ui/mods-regiao.js"),
            include_str!("../../apps/seele-app/ui/mods-superficies.js"),
        ]
        .join("\n");
        let eventos = eventos_despachados(&janela);
        for nome in prometidos {
            assert!(
                eventos.contains(nome),
                "o extrator não achou o evento `{nome}` na janela de verdade: ou a forma de \
                 `falar` mudou (conserte `eventos_despachados`), ou a janela deixou de \
                 entregar um evento que a v5 promete"
            );
        }
    }

    #[test]
    fn a_api_6_que_promete_um_evento_sem_despachante_reprova() {
        let entregues = eventos_despachados(JANELA_DE_FIXTURE);
        let v6 = serde_json::json!({
            "version": 6,
            "moments": [],
            "eventos": ["botao", "arrastar"],
        });
        let violacoes = sem_despachante(6, &v6, &promessa_de_eventos(&entregues));
        assert_eq!(
            violacoes.len(),
            1,
            "esperava uma violação, vieram {violacoes:?}"
        );
        assert!(
            violacoes
                .first()
                .is_some_and(|v| v.contains("arrastar") && v.contains("falar")),
            "a violação não diz qual evento ficou sem despachante nem por onde despachá-lo: \
             {violacoes:?}"
        );
    }

    #[test]
    fn a_api_6_que_so_promete_eventos_despachados_passa() {
        let entregues = eventos_despachados(JANELA_DE_FIXTURE);
        let v6 = serde_json::json!({ "version": 6, "moments": [], "eventos": ["botao", "link"] });
        let violacoes = sem_despachante(6, &v6, &promessa_de_eventos(&entregues));
        assert!(
            violacoes.is_empty(),
            "uma v6 que só promete eventos entregues reprovou: {violacoes:?}"
        );
    }

    #[test]
    fn a_api_6_que_nao_lista_os_eventos_reprova() {
        let entregues = eventos_despachados(JANELA_DE_FIXTURE);
        let v6 = serde_json::json!({ "version": 6, "extends": 5, "moments": [] });
        let violacoes = sem_despachante(6, &v6, &promessa_de_eventos(&entregues));
        assert_eq!(
            violacoes.len(),
            1,
            "esperava uma violação, vieram {violacoes:?}"
        );
        assert!(
            violacoes
                .first()
                .is_some_and(|v| v.contains("`eventos`") && v.contains("extends")),
            "uma v6 sem `eventos` passou, e o que ela promete ficaria implícito: {violacoes:?}"
        );
    }

    /// A v5 de verdade, contra uma janela que não entrega nada: congelada, ela
    /// não é cobrada; cobrada como 6, reprova nos 16.
    ///
    /// Versões escritas por extenso, pelo mesmo motivo de
    /// `as_apis_congeladas_nao_sao_cobradas`.
    #[test]
    fn a_v5_congelada_nao_tem_os_eventos_cobrados() {
        let Ok(v5) = serde_json::from_str::<serde_json::Value>(include_str!("../../api/v5.json"))
        else {
            panic!("api/v5.json deixou de ser JSON");
        };
        let nenhum = BTreeSet::new();
        let promessa = promessa_de_eventos(&nenhum);
        for versao in 1..=5 {
            assert!(
                sem_despachante(versao, &v5, &promessa).is_empty(),
                "a API {versao} está congelada e teve os eventos cobrados: a promessa dela não \
                 se edita"
            );
        }
        assert_eq!(
            sem_despachante(6, &v5, &promessa).len(),
            16,
            "os 16 eventos da v5 cobrados como API 6, contra uma janela que não entrega nada, \
             deviam reprovar todos"
        );
    }

    /// As duas promessas que [`promessas_cobradas`] monta conferem os dois
    /// blocos, cada um contra o próprio despachante. Com duas promessas do
    /// mesmo bloco, o [`cobrar`] daria a v6 por cobrada e aprovaria um evento
    /// que ninguém entrega; com os conjuntos trocados, reprovaria um que é
    /// entregue.
    ///
    /// O teste não vê o `run`. Que é este par que ele passa ao
    /// [`conferir_arquivo`], e não outro, está no código dele: o tipo
    /// `[Promessa; 2]` só garante que são duas.
    #[test]
    fn as_promessas_cobradas_conferem_os_momentos_e_os_eventos() {
        let momentos = momentos_de_fixture();
        let eventos = eventos_despachados(JANELA_DE_FIXTURE);
        let v6 = serde_json::json!({
            "version": 6,
            "moments": ["PersonJoined", "ScreenShareStarted"],
            "eventos": ["botao", "arrastar"],
        });
        let violacoes = cobrar(
            std::path::Path::new("api/v6.json"),
            &v6,
            &promessas_cobradas(&momentos, &eventos),
        )
        .unwrap_or_default();
        assert_eq!(
            violacoes.len(),
            2,
            "esperava uma violação por bloco — `ScreenShareStarted` em `moments` e `arrastar` \
             em `eventos` —, vieram {violacoes:?}"
        );
        for (nome, bloco) in [
            ("ScreenShareStarted", "`moments`"),
            ("arrastar", "`eventos`"),
        ] {
            assert!(
                violacoes
                    .iter()
                    .any(|v| v.contains(nome) && v.contains(bloco)),
                "as promessas cobradas não conferem {bloco}: `{nome}`, sem despachante, passou. \
                 Vieram {violacoes:?}"
            );
        }
    }

    /// A frase que a pessoa lê quando o `check-api` reprova é a de produção: o
    /// `run` e este teste montam as promessas pela mesma
    /// [`promessas_cobradas`]. Ela diz a forma que cada extrator procura e
    /// onde, que é o que se abre para consertar.
    #[test]
    fn a_violacao_diz_a_forma_e_o_lugar_que_o_extrator_procura() {
        let nenhum = BTreeSet::new();
        let v6 = serde_json::json!({
            "version": 6,
            "moments": ["PersonJoined"],
            "eventos": ["botao"],
        });
        let violacoes = cobrar(
            std::path::Path::new("api/v6.json"),
            &v6,
            &promessas_cobradas(&nenhum, &nenhum),
        )
        .unwrap_or_default();
        for frase in [
            "não é despachado por nenhum braço `=> (\"Nome\", carga)` de `momento_de` \
             (crates/seele-server/src/mods/despacho.rs)",
            "não é despachado por nenhum `falar({ nome: … })` da janela (apps/seele-app/ui)",
        ] {
            assert!(
                violacoes.iter().any(|v| v.contains(frase)),
                "a violação que o `check-api` imprime não diz a forma e o lugar do \
                 despachante, «{frase}»: vieram {violacoes:?}"
            );
        }
    }

    /// Um item que não é texto não é pulado calado, em nenhum dos dois blocos:
    /// é uma promessa que ninguém sabe ler, e reprova dizendo o que achou.
    #[test]
    fn um_item_que_nao_e_nome_reprova() {
        let nenhum = BTreeSet::new();
        let casos = [
            (
                serde_json::json!({ "version": 6, "moments": [1] }),
                promessa_de_momentos(&nenhum),
            ),
            (
                serde_json::json!({ "version": 6, "eventos": [1] }),
                promessa_de_eventos(&nenhum),
            ),
        ];
        for (v6, promessa) in &casos {
            let violacoes = sem_despachante(6, v6, promessa);
            assert_eq!(
                violacoes.len(),
                1,
                "esperava uma violação em `{}`, vieram {violacoes:?}",
                promessa.bloco
            );
            assert!(
                violacoes
                    .first()
                    .is_some_and(|v| v.contains("não é um nome") && v.contains("`1`")),
                "um item de `{}` que não é nome passou calado, ou a violação não diz o que \
                 achou: {violacoes:?}",
                promessa.bloco
            );
        }
    }
}
