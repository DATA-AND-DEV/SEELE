//! What a MOD declares about itself, and what the product refuses.
//!
//! ADR 0045. A MOD is a directory with a `mod.json`, an optional client half
//! and an optional server half. This module is the manifest and nothing else:
//! reading disk belongs to `seele-core`, storing state belongs to
//! `seele-server`.
//!
//! # Why it lives here and not in `seele-core`
//!
//! `xtask/src/check_deps.rs` forbids `seele-server` from depending on
//! `seele-core`, and both ends need the same type. `seele-proto` is the only
//! crate both are allowed to reach, and it already holds two non-wire parsers
//! for the same reason — `uri` and `attachment`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Version of the `mod.json` format this build reads.
///
/// An integer and not semver, for the reason ADR 0029 gave and 0045 keeps:
/// semver invites an argument about what is compatible, and here there is none
/// — the schema only grows.
pub const MANIFEST_SCHEMA: u32 = 1;

/// Version of the MOD API this build offers.
///
/// Separate from [`MANIFEST_SCHEMA`] because they move for different reasons:
/// the schema changes when the *manifest* gains a field, the API when what a
/// MOD can *call* changes. ADR 0045 freezes each API version in its own file,
/// never edited once shipped.
/// **3 desde 18/09/2026, e é uma ruptura** — ADR 0049.
///
/// Um MOD deixa de rodar na janela do produto: a lógica vai para um `Worker`
/// sem DOM, e o desenho passa a ser declarado por uma API de regiões e tema. O
/// que a API 2 permitia — alcançar `document`, o global do Tauri e tudo o mais
/// que a página tem — deixa de existir, e não há caminho de compatibilidade: um
/// MOD de API 2 não roda aqui.
///
/// A decisão de não ter caminho de compatibilidade está no ADR: enquanto
/// houvesse um MOD antigo carregado, a promessa «o que um MOD faz some quando
/// você sai do servidor» continuaria falsa, e o produto prometeria duas coisas
/// diferentes ao mesmo tempo.
pub const MOD_API_VERSION: u32 = 5;

/// **As versões que este build executa**, da mais nova para a mais velha.
///
/// # Por que um conjunto, e não um número
///
/// Até a API 3 a conferência era igualdade: `manifest.api == MOD_API_VERSION`,
/// com `ApiTooOld` para tudo abaixo. Isso era correto *naquela* ruptura — a
/// API 3 **tirou** capacidades, e «serve uma API mais velha» tinha deixado de
/// ser verdade.
///
/// A API 4 não tira nada. Ela acrescenta superfícies, contribuições,
/// composição e estilos sobre o mesmo executor, a mesma ponte e o mesmo
/// renderer. Um pacote de API 3 continua sendo exatamente o que ele era: uma
/// região, um tema e cartões.
///
/// Manter a igualdade aqui significaria que subir a constante para 4
/// **quebraria todo pacote publicado no mesmo instante** — e o §13 do plano
/// aponta essa armadilha pelo nome. Um conjunto é o que permite publicar o
/// aplicativo compatível antes de publicar os pacotes novos, que é a única
/// ordem em que ninguém fica sem MOD.
///
/// # O que um conjunto **não** concede
///
/// Aceitar a 3 não é dar à 3 o que a 4 tem. As capacidades são por versão, e
/// quem as aplica é o prelúdio do executor: um pacote que declara `api: 3`
/// recebe `SeeleUI` sem `superficies` e sem `contribuicoes`, e chamar o que
/// não está lá falha no MOD, onde quem o escreveu consegue ver.
///
/// A API 2 não volta. Ela executava na janela, e o ADR 0049 explica por que
/// não há caminho de volta disso.
pub const APIS_ACEITAS: &[u32] = &[5, 4, 3];

/// A API mais velha que este build ainda executa.
pub const MOD_API_MINIMA: u32 = 3;

/// O que um pacote de cada versão pode chamar.
///
/// Devolvido ao carregar, e é o que o prelúdio usa para montar `SeeleUI`. Uma
/// capacidade que não está aqui **não existe** para aquele pacote: não é um
/// erro em tempo de execução escondido, é um método ausente.
#[must_use]
pub fn capacidades_da_api(api: u32) -> &'static [&'static str] {
    match api {
        5 => &[
            "regiao",
            "tema",
            "cartoes",
            "arquivo",
            "superficies",
            "contribuicoes",
            "estilos",
            "classes",
            "volume",
        ],
        4 => &[
            "regiao",
            "tema",
            "cartoes",
            "arquivo",
            "superficies",
            "contribuicoes",
            "estilos",
            "classes",
        ],
        3 => &["regiao", "tema", "cartoes", "arquivo"],
        _ => &[],
    }
}

/// What a MOD declares about itself.
///
/// `deny_unknown_fields` is the decision, not the default: a misspelled key
/// installs a MOD missing the thing its author thought was there, and the
/// author never finds out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Manifest format this file is written against.
    pub schema: u32,
    /// `author/name`. The identity a server announces; the hash is what it
    /// proves.
    pub id: String,
    /// The author's own version string. Never parsed by us.
    pub version: String,
    /// MOD API version this MOD was written against.
    pub api: u32,
    /// The public repository. Required — ADR 0045 makes it a condition of
    /// publication, and this is where it is stated.
    pub repo: String,
    /// What this MOD asks to reach, for the acceptance screen to show before
    /// anything is downloaded.
    ///
    /// **Read and validated, and nothing consumes it yet.** The acceptance
    /// screen is a later plan, and the runtime that would enforce it is later
    /// still. It is in schema 1 anyway for the reason ADR 0029 gave about
    /// `ansi256`: a manifest written today has to still parse on the day the
    /// consumer arrives, and `deny_unknown_fields` would refuse it otherwise.
    #[serde(default)]
    pub reach: Vec<String>,
    /// Version of this MOD's own data schema, for its own migrations.
    ///
    /// Read and not consumed, for the same reason as [`Self::reach`].
    #[serde(default)]
    pub state: Option<u32>,
    /// Path, relative to the MOD directory, of the script the window loads.
    #[serde(default)]
    pub client: Option<String>,
    /// Path, relative to the MOD directory, of the script the server runs.
    /// Read but not executed until the runtime lands.
    #[serde(default)]
    pub server: Option<String>,
    /// Os arquivos que este MOD traz para mostrar ou tocar, relativos à pasta.
    ///
    /// **Declarados, e não descobertos.** A regra de `serve` é «só o que o
    /// manifesto declara», e a mídia entra por ela em vez de abrir a pasta: um
    /// arquivo que ninguém nomeou não é lido, e o que o MOD é fica escrito num
    /// lugar que quem revisa o pacote lê antes de instalar.
    ///
    /// O **tipo** de cada um vem dos bytes, nunca desta lista nem da extensão:
    /// ver `midia_de_mod::sniff`.
    #[serde(default)]
    pub arquivos: Vec<String>,
}

/// Why a manifest was refused.
///
/// One variant per refusal, with the numbers inside. `specs/02-protocolo.md`:
/// "no free-form string reaches the interface — the shell decides how to
/// present each variant". The `Display` text is for `tracing`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Refused {
    /// Not JSON, or a key this build does not know.
    ///
    /// Carries the position rather than `serde_json`'s message, so no
    /// free-form string crosses into a shell.
    #[error("manifest is malformed at line {line}, column {column}")]
    Malformed {
        /// 1-based line where parsing stopped.
        line: usize,
        /// 1-based column where parsing stopped.
        column: usize,
    },

    /// The manifest is written against a newer format than this build reads.
    #[error("manifest schema {found}, this build reads {ours}")]
    SchemaTooNew {
        /// Schema the file declares.
        found: u32,
        /// Schema this build reads.
        ours: u32,
    },

    /// The MOD targets a newer API than this build offers.
    #[error("mod targets API {wanted}, this build offers {ours}")]
    ApiTooNew {
        /// API the MOD asks for.
        wanted: u32,
        /// API this build offers.
        ours: u32,
    },

    /// The MOD targets an API this build no longer offers.
    ///
    /// **Existe porque o ADR 0049 é uma ruptura, e não um alargamento.** Antes
    /// dele, uma API mais velha era o caso normal de todo MOD antigo: o que a
    /// API 1 oferecia a API 2 continuava oferecendo. A API 3 tirou coisa —
    /// `document`, o global do Tauri, a página inteira —, e um MOD escrito
    /// contra a 2 não é um MOD que pede menos: é um MOD que pede o que não
    /// existe mais.
    ///
    /// Carregá-lo assim mesmo o faria falhar na primeira linha, e a pessoa que
    /// o instalou leria «não carregou» sem saber que o motivo era a versão.
    #[error("mod targets API {wanted}, this build offers {ours} and no older")]
    ApiTooOld {
        /// API the MOD asks for.
        wanted: u32,
        /// API this build offers, and the only one it offers.
        ours: u32,
    },

    /// The identifier is not `author/name`.
    #[error("mod id is not `author/name`")]
    MalformedId,

    /// Neither half is declared, so the MOD does nothing.
    #[error("mod declares neither a client nor a server half")]
    Empty,

    /// Um arquivo declarado não é um caminho dentro do pacote.
    ///
    /// **Recusado no manifesto, e não na leitura.** Um `..` que só fosse
    /// recusado na hora de ler seria um pacote instalado que pede, a cada
    /// sessão, um arquivo de fora — e a recusa apareceria como mídia que não
    /// toca, em vez de como um MOD que não devia ter sido instalado.
    #[error("mod declares file {posicao}, which is not a path inside the package")]
    ArquivoInvalido {
        /// Qual da lista, contando do zero.
        posicao: usize,
    },

    /// A lista de arquivos passa do que o produto carrega.
    #[error("mod declares {quantos} files, the ceiling is {teto}")]
    ArquivosDemais {
        /// Quantos o manifesto traz.
        quantos: usize,
        /// Quantos cabem.
        teto: usize,
    },
}

/// **Quantos arquivos um MOD pode declarar.**
///
/// Dezesseis. A mídia de um MOD é ilustração e efeito, e uma lista maior que
/// isto é um acervo — que é outra coisa, com outro custo, e que pediria
/// carregamento por demanda em vez de declaração no manifesto.
pub const TETO_DE_ARQUIVOS: usize = 16;

/// O que um manifesto **diz de si**, mesmo quando ele é recusado.
///
/// # Por que isto existe
///
/// U10 da auditoria de 20/09/2026: «MODs antigos aparecem como hashes
/// compridos, `api-too-old`, versão vazia e 0 B. O usuário perde a identidade
/// do pacote justamente quando precisa atualizá-lo.»
///
/// A recusa era total: um manifesto que não passava não devolvia nada, e a tela
/// ficava com o nome da pasta — um hash de 64 caracteres — e um código de erro.
/// Quem precisava decidir se apagava ou atualizava aquele pacote não tinha o
/// nome dele nem a versão.
///
/// A recusa continua total **para executar**: nada aqui faz um pacote de API 2
/// rodar. O que ela deixa de ser é total para **descrever**, e são duas
/// perguntas diferentes.
///
/// Só os três campos que uma pessoa usa para reconhecer um pacote, e nenhum que
/// o produto vá obedecer: nem `client`, nem `arquivos`, nem `reach`. Um
/// manifesto recusado não autoriza nada, e ler dele um caminho de arquivo seria
/// justamente autorizar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ComoEleSeChama {
    /// `autor/nome`, como o manifesto o escreve.
    #[serde(default)]
    pub id: String,
    /// A versão que o autor declara.
    #[serde(default)]
    pub version: String,
    /// A API que ele pede. Zero quando o campo falta ou não é número.
    #[serde(default)]
    pub api: u32,
}

/// Lê só o que um manifesto diz de si, sem validar nada.
///
/// **Nunca falha**: um JSON quebrado devolve os campos vazios, que é a mesma
/// resposta honesta de antes — só que sem levar junto o que dava para ler.
#[must_use]
pub fn como_ele_se_chama(text: &str) -> ComoEleSeChama {
    serde_json::from_str(text).unwrap_or_default()
}

/// Reads a `mod.json`.
///
/// # Errors
///
/// Returns [`Refused`] for malformed JSON, an unknown key, a schema or API
/// this build cannot serve, a malformed identifier, or a MOD with no halves.
pub fn read_manifest(text: &str) -> Result<Manifest, Refused> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|error| Refused::Malformed {
        line: error.line(),
        column: error.column(),
    })?;

    if manifest.schema > MANIFEST_SCHEMA {
        return Err(Refused::SchemaTooNew {
            found: manifest.schema,
            ours: MANIFEST_SCHEMA,
        });
    }
    // **Um conjunto, e não uma igualdade** — ver [`APIS_ACEITAS`]. A API 3
    // continua executando porque a 4 não tirou nada dela; a 2 não, porque ela
    // rodava na janela.
    if manifest.api > MOD_API_VERSION {
        return Err(Refused::ApiTooNew {
            wanted: manifest.api,
            ours: MOD_API_VERSION,
        });
    }
    if !APIS_ACEITAS.contains(&manifest.api) {
        return Err(Refused::ApiTooOld {
            wanted: manifest.api,
            ours: MOD_API_MINIMA,
        });
    }
    if !is_well_formed_id(&manifest.id) {
        return Err(Refused::MalformedId);
    }
    if manifest.client.is_none() && manifest.server.is_none() {
        return Err(Refused::Empty);
    }
    if manifest.arquivos.len() > TETO_DE_ARQUIVOS {
        return Err(Refused::ArquivosDemais {
            quantos: manifest.arquivos.len(),
            teto: TETO_DE_ARQUIVOS,
        });
    }
    for (posicao, arquivo) in manifest.arquivos.iter().enumerate() {
        // O mesmo caminho que a leitura usa, e conferido aqui: uma lista que
        // passasse por esta porta e fosse recusada lá seria um pacote válido
        // com mídia que nunca toca.
        if inner_path(&arquivo.split('/').collect::<Vec<_>>()).is_none() {
            return Err(Refused::ArquivoInvalido { posicao });
        }
    }
    Ok(manifest)
}

#[cfg(test)]
mod testes_de_arquivos {
    use super::*;

    fn manifesto(arquivos: &str) -> String {
        format!(
            r#"{{"schema":1,"id":"a/b","version":"1","api":{MOD_API_VERSION},
                "repo":"https://example.invalid/x","client":"c.js","arquivos":{arquivos}}}"#
        )
    }

    #[test]
    fn um_arquivo_que_sobe_de_pasta_e_recusado_no_manifesto() {
        // Recusado aqui, e não na leitura: um pacote instalado que pede um
        // arquivo de fora a cada sessão é um pacote que não devia ter entrado.
        for fora in [r#"["../fora.wav"]"#, r#"["/etc/senha"]"#, r#"[""]"#] {
            assert_eq!(
                read_manifest(&manifesto(fora)),
                Err(Refused::ArquivoInvalido { posicao: 0 }),
                "aceitou «{fora}»"
            );
        }
        // E diz **qual**: com quinze arquivos na lista, «um deles» não ajuda
        // quem está consertando o pacote.
        assert_eq!(
            read_manifest(&manifesto(r#"["som/a.wav","../fora.wav"]"#)),
            Err(Refused::ArquivoInvalido { posicao: 1 })
        );
    }

    #[test]
    fn a_lista_tem_teto_e_o_teto_diz_os_dois_numeros() {
        let demais = format!(
            "[{}]",
            (0..=TETO_DE_ARQUIVOS)
                .map(|n| format!("\"a{n}.wav\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(
            read_manifest(&manifesto(&demais)),
            Err(Refused::ArquivosDemais {
                quantos: TETO_DE_ARQUIVOS + 1,
                teto: TETO_DE_ARQUIVOS,
            })
        );
    }

    #[test]
    fn um_manifesto_sem_a_lista_continua_valendo() {
        // O campo nasceu com `default`: um MOD publicado antes dele não
        // declara arquivo nenhum, e não pode passar a ser recusado por isso.
        let antigo = format!(
            r#"{{"schema":1,"id":"a/b","version":"1","api":{MOD_API_VERSION},
                "repo":"https://example.invalid/x","client":"c.js"}}"#
        );
        assert_eq!(read_manifest(&antigo).map(|m| m.arquivos), Ok(Vec::new()));
    }
}

/// `author/name`, both halves non-empty, and nothing that could climb out of a
/// directory.
fn is_well_formed_id(id: &str) -> bool {
    let mut halves = id.split('/');
    let (Some(author), Some(name), None) = (halves.next(), halves.next(), halves.next()) else {
        return false;
    };
    [author, name].iter().all(|half| {
        !half.is_empty()
            && half
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

/// The identity of a MOD's bytes.
///
/// A server announces `author/name` and a version; this is what proves the
/// bytes are the ones that were reviewed. ADR 0026, alternative 5, is the
/// reason it exists at all: "TLS says which server the file came from, not who
/// produced it".
///
/// Deterministic across machines, which is the whole requirement:
///
/// - **paths are sorted**, because directory order is not a promise any
///   filesystem makes;
/// - **every length is fed in before its bytes**, so `("ab", "c")` and
///   `("a", "bc")` cannot collide;
/// - lengths go in as fixed 8-byte big-endian, so a length is never itself
///   ambiguous.
///
/// Takes `&mut` so the sort happens in place and no caller has to remember to
/// sort first — a caller that forgot would produce a hash that is right on one
/// machine and wrong on the next.
#[must_use]
pub fn content_hash(files: &mut [(String, Vec<u8>)]) -> [u8; 32] {
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = Sha256::new();
    hasher.update((files.len() as u64).to_be_bytes());
    for (path, bytes) in files.iter() {
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    hasher.finalize().into()
}

/// The path inside a MOD's own directory, rebuilt from components, or nothing.
///
/// Two callers need exactly this rule and it must not drift between them:
///
/// - the desktop shell, serving a MOD's files under `mod://`;
/// - the server, scoping a MOD's own folder under `mods/<author>/<name>/dados/`.
///
/// A second copy would be two places to fix and one to forget, so it lives in
/// the one crate both are allowed to reach.
///
/// # Why refusing beats resolving
///
/// A `..` anywhere is refused rather than resolved, and so is an absolute
/// component, a root, or an empty piece. Resolving is where a path that looks
/// contained stops being contained.
///
/// This has its own tests because of what proving it taught: while the rule
/// lived inside the `mod://` handler, the traversal test passed with the whole
/// rebuilding deleted — another check was catching those paths for another
/// reason, and the guard agreed with its own comment without doing anything.
/// `CLAUDE.md`: "existir não é funcionar".
#[must_use]
pub fn inner_path(parts: &[&str]) -> Option<std::path::PathBuf> {
    use std::path::{Component, Path, PathBuf};

    let mut relative = PathBuf::new();
    for part in parts {
        let mut components = Path::new(part).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(piece)), None) => relative.push(piece),
            _ => return None,
        }
    }
    Some(relative)
}

// ---------------------------------------------------------------- o anúncio
//
// ADR 0045, «o que quem entra vê, e o que ele pode recusar». O servidor diz o
// que exige **antes** de a pessoa entrar, e quem não aceita não entra.
//
// Estes tipos moram aqui, e não em `control.rs`, pela razão do cabeçalho deste
// módulo: o manifesto é lido nas duas pontas, e `seele-proto` é o único crate
// que `seele-core` e `seele-server` alcançam os dois.

/// Versão do protocolo em que o anúncio de MODs viaja.
///
/// # Por que ela é uma constante separada, e não `PROTOCOL_VERSION`
///
/// Um par que não conhece uma variante **não a ignora**: o postcard indexa
/// variante por posição, e um quadro desconhecido desloca a leitura do fluxo
/// para sempre. Quem manda uma variante nova tem de perguntar antes a versão do
/// par — é o que `session.rs` já faz com o `UplinkLoss` e com as duas mensagens
/// da v4.
///
/// **Este número nasceu em 5 enquanto `PROTOCOL_VERSION` ainda era 4, e isso
/// era deliberado.** Esta entrega e a da malha acrescentavam variantes ao mesmo
/// par de listas ao mesmo tempo; se cada uma subisse a versão global por conta
/// própria, as duas chamariam «5» a vocabulários diferentes — que é exatamente
/// o defeito que o guarda dos ordinais em `control.rs` existe para pegar, e que
/// já custou uma tela preta sem mensagem nenhuma. Enquanto isso o portão ficava
/// **dormente**, porque não teria a quem proteger: nenhuma conexão negocia acima
/// da versão global, então não existia par capaz de aceitar, e recusar quem não
/// alcança seria recusar todo mundo.
///
/// **O contrato está cumprido desde 14/09/2026.** As duas entregas se juntaram,
/// as variantes das duas convivem numa ordem única,
/// [`crate::version::PROTOCOL_VERSION`] subiu para 5 **uma vez só**, e esta
/// constante continuou em 5 — como estava escrito que aconteceria, e nada além
/// disso. Nenhuma linha de código precisou mudar aqui nem no portão: o teste
/// abaixo e `o_anuncio_alcanca_alguem` comparam as duas, e a comparação virou
/// verdadeira sozinha.
///
/// **O que mudou para quem usa:** habilitar um MOD num servidor passou a valer
/// no fio. Quem entra recebe a lista antes de entrar e pode recusar; quem recusa
/// não entra. Antes, ligar o interruptor gravava a exigência e não trancava
/// ninguém — e a casca **tinha como** dizer isso, por
/// `seele_server::mods::anuncio::exigencia_vale_na_rede`, que chega a ela junto
/// com a lista de MODs instalados. O campo já viaja até a casca — `ModNaTela`
/// em `apps/seele-app/src/main.rs` o carrega em toda resposta de
/// `mods_instalados` —, mas o frontend ainda não o desenha:
/// `apps/seele-app/ui/base.js` lê dessa lista só `enabled` e `client`, para
/// carregar os scripts. Então quem hospeda continua sem ver o efeito do
/// interruptor, não por falta do dado e sim por falta da tela. A pendência #39,
/// que esperava esta subida, fechou em 14/09/2026; a tela que falta ganhou
/// endereço próprio na pendência #44, e a tela de aceite — do outro lado do fio
/// — segue no fim da #39.
///
/// **Ela continua sendo uma constante separada** e não vira um alias de
/// `PROTOCOL_VERSION`: é ela que diz *a partir de qual versão* o anúncio viaja,
/// e na próxima subida global as duas voltam a divergir — 5 aqui, 6 lá — sem que
/// o anúncio precise de nada. Ver
/// `seele_server::mods::anuncio::o_anuncio_alcanca_alguem` e
/// `docs/superpowers/specs/2026-09-10-anuncio-e-aceite-de-mods.md`.
pub const VERSAO_DO_ANUNCIO: u8 = 5;

/// Quantos dígitos tem um hash de conteúdo escrito em hexadecimal.
pub const HASH_EM_HEX_LEN: usize = 64;

/// Um MOD que este servidor exige, como ele atravessa o fio.
///
/// # O que cada campo está aqui para responder
///
/// Os três primeiros são **identidade**: quem é, qual versão, e quais bytes. O
/// hash é o que prova; o nome é conveniência (ADR 0026, alternativa 5).
///
/// Os três últimos existem para a **tela de aceite**, e não para o servidor:
/// ADR 0045 exige que a pessoa leia «nome, autor, versão, repositório, e o
/// `reach` declarado» *antes* de qualquer byte ser baixado. Sem eles no
/// anúncio, a única forma de a tela saber o que o MOD alcança seria baixá-lo
/// primeiro — o que é decidir antes de perguntar.
///
/// **Nada aqui pode divergir do hash sem que o hash mude**, e é a propriedade
/// que faz o anúncio valer alguma coisa: `repo`, `reach` e a metade de servidor
/// saem do `mod.json`, e o `mod.json` está dentro de [`content_hash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModAnunciado {
    /// `autor/nome`.
    pub id: String,
    /// A versão que o autor declara.
    pub version: String,
    /// O hash do conteúdo, em hexadecimal minúsculo.
    pub hash: String,
    /// O repositório público. ADR 0045 o torna condição de publicação.
    pub repo: String,
    /// O que este MOD declara alcançar, como o manifesto o escreve.
    pub reach: Vec<String>,
    /// Se este MOD roda na máquina de quem hospeda.
    ///
    /// **É o campo que a tela de aceite não pode omitir.** Um MOD com metade de
    /// servidor alcança o bloco `world` do `api/v1.json` — rede de saída,
    /// relógio e log — a partir da máquina de quem hospeda, e o plano do runtime
    /// nomeia isso como «a parte que a tela de aceite tem de dizer em voz alta».
    ///
    /// Um MOD só de aparência não tem nada disso, e a diferença entre os dois é
    /// a diferença entre repintar uma janela e abrir conexões da casa de
    /// alguém.
    pub no_servidor: bool,
}

/// Um hash de conteúdo em hexadecimal minúsculo, e nada mais.
///
/// **Tamanho exato e caixa fixa, e não um teto.** O mesmo argumento de
/// `control::check_impressao`, mais um que é só daqui: a identidade do conjunto
/// é calculada sobre este texto, então `ABC…` e `abc…` dariam **duas
/// identidades para os mesmos bytes** — e um aceite guardado deixaria de valer
/// por causa de uma letra maiúscula.
#[must_use]
pub fn e_hash_de_conteudo(texto: &str) -> bool {
    texto.len() == HASH_EM_HEX_LEN
        && texto
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() && c.is_ascii_hexdigit())
}

/// Um hash de 32 bytes em hexadecimal minúsculo.
///
/// Uma implementação só para os dois lados: `seele-core` a reexporta em vez de
/// escrever a segunda, porque duas seriam dois lugares para a formatação
/// derivar — e derivar aqui lê-se como um MOD que mudou.
#[must_use]
pub fn hex(hash: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    hash.iter().fold(String::new(), |mut texto, byte| {
        let _ = write!(texto, "{byte:02x}");
        texto
    })
}

/// A identidade do conjunto que um servidor exige.
///
/// # Por que o conjunto tem identidade própria
///
/// O aceite de quem entra vale para **o conjunto que ele leu**, e não para cada
/// MOD solto. ADR 0045: «um servidor que troca de MOD pergunta de novo». Sem um
/// número que resuma o conjunto inteiro, «trocou» não teria como ser percebido:
/// acrescentar um MOD deixaria os aceites dos outros de pé e a pessoa entraria
/// sem nunca ter lido o novo.
///
/// Deterministica pelas mesmas três regras de [`content_hash`], e pelo mesmo
/// motivo — duas máquinas têm de chegar ao mesmo número:
///
/// - **a lista é ordenada**, porque a ordem do banco não é promessa de ninguém;
/// - **todo tamanho entra antes dos bytes dele**, então `("ab","c")` e
///   `("a","bc")` não colidem;
/// - tamanhos entram como oito bytes big-endian, então um tamanho nunca é ele
///   mesmo ambíguo.
///
/// **Só identidade entra na conta** — `id`, `version` e `hash`. `repo`, `reach`
/// e a metade de servidor saem do `mod.json`, que está dentro do `hash`: incluí-los
/// seria contar a mesma coisa duas vezes, e deixá-los de fora não abre folga
/// nenhuma.
#[must_use]
pub fn identidade_do_conjunto(mods: &mut [ModAnunciado]) -> [u8; 32] {
    mods.sort_by(|esquerda, direita| esquerda.id.cmp(&direita.id));

    let mut hasher = Sha256::new();
    hasher.update((mods.len() as u64).to_be_bytes());
    for anunciado in mods.iter() {
        for campo in [&anunciado.id, &anunciado.version, &anunciado.hash] {
            hasher.update((campo.len() as u64).to_be_bytes());
            hasher.update(campo.as_bytes());
        }
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nenhum_caminho_com_dois_pontos_vira_caminho_interno() {
        for parte in [
            vec!["..", "etc", "passwd"],
            vec!["cliente", "..", "..", "mod.json"],
            vec!["cliente", "../main.js"],
            vec!["/etc", "passwd"],
            vec![""],
            vec!["."],
        ] {
            assert_eq!(
                inner_path(&parte),
                None,
                "`{parte:?}` virou caminho interno"
            );
        }
    }

    #[test]
    fn um_caminho_comum_vira_caminho_interno() {
        assert_eq!(
            inner_path(&["cliente", "main.js"]),
            Some(std::path::PathBuf::from("cliente").join("main.js"))
        );
    }

    fn manifesto_minimo() -> String {
        format!(
            r#"{{
                "schema": {MANIFEST_SCHEMA},
                "id": "seele/exemplo",
                "version": "1.0.0",
                "api": {MOD_API_VERSION},
                "repo": "https://github.com/seele/exemplo",
                "reach": ["dom"],
                "state": 1,
                "client": "cliente/main.js"
            }}"#
        )
    }

    #[test]
    fn um_manifesto_completo_e_lido() {
        let lido = read_manifest(&manifesto_minimo()).expect("manifesto válido recusado");
        assert_eq!(lido.id, "seele/exemplo");
        assert_eq!(lido.client.as_deref(), Some("cliente/main.js"));
        assert_eq!(lido.server, None);
    }

    /// Chave desconhecida é recusada, e o motivo é do ADR 0029 e sobrevive no 0045.
    ///
    /// `vesion` com um `s` a menos instalaria calado um MOD sem versão, e o
    /// autor nunca ficaria sabendo. Recusar é a única forma de retorno que um
    /// autor de MOD tem.
    #[test]
    fn uma_chave_desconhecida_e_recusada() {
        let texto = manifesto_minimo().replace("\"version\"", "\"vesion\"");
        assert!(matches!(
            read_manifest(&texto),
            Err(Refused::Malformed { .. })
        ));
    }

    #[test]
    fn um_esquema_do_futuro_e_recusado_com_os_dois_numeros() {
        let texto = manifesto_minimo().replace(
            &format!("\"schema\": {MANIFEST_SCHEMA}"),
            &format!("\"schema\": {}", MANIFEST_SCHEMA + 1),
        );
        assert_eq!(
            read_manifest(&texto),
            Err(Refused::SchemaTooNew {
                found: MANIFEST_SCHEMA + 1,
                ours: MANIFEST_SCHEMA,
            })
        );
    }

    /// ADR 0045: «o MOD não carrega, e diz» — nomeando os dois números.
    #[test]
    fn uma_api_do_futuro_e_recusada_com_os_dois_numeros() {
        let texto = manifesto_minimo().replace(
            &format!("\"api\": {MOD_API_VERSION}"),
            &format!("\"api\": {}", MOD_API_VERSION + 1),
        );
        assert_eq!(
            read_manifest(&texto),
            Err(Refused::ApiTooNew {
                wanted: MOD_API_VERSION + 1,
                ours: MOD_API_VERSION,
            })
        );
    }

    /// **O pacote da API 3 continua executando na 4**, e este caso é o guarda.
    ///
    /// A conferência era igualdade, e subir a constante para 4 teria recusado
    /// todo pacote publicado no mesmo instante — os três MODs oficiais, na
    /// máquina de quem já os tinha instalado. O §13 do plano aponta essa
    /// armadilha pelo nome; este teste é o que impede alguém de reintroduzi-la
    /// voltando a comparar por igual.
    #[test]
    fn um_pacote_da_api_anterior_continua_carregando() {
        for aceita in APIS_ACEITAS {
            let texto = manifesto_minimo().replace(
                &format!("\"api\": {MOD_API_VERSION}"),
                &format!("\"api\": {aceita}"),
            );
            let lido = read_manifest(&texto)
                .unwrap_or_else(|erro| panic!("a API {aceita} devia carregar, e deu {erro:?}"));
            assert_eq!(lido.api, *aceita);
        }
    }

    /// E a API 2 não volta: ela executava na janela, e o ADR 0049 diz por que
    /// não há caminho de volta disso.
    #[test]
    fn a_api_que_rodava_na_janela_nao_volta() {
        let texto =
            manifesto_minimo().replace(&format!("\"api\": {MOD_API_VERSION}"), "\"api\": 2");
        assert_eq!(
            read_manifest(&texto),
            Err(Refused::ApiTooOld {
                wanted: 2,
                ours: MOD_API_MINIMA,
            })
        );
    }

    /// Aceitar uma versão não é dar a ela o que a versão nova tem.
    #[test]
    fn cada_versao_tem_as_capacidades_dela() {
        let cinco = capacidades_da_api(5);
        let quatro = capacidades_da_api(4);
        assert!(cinco.contains(&"volume"));
        assert!(!quatro.contains(&"volume"));
        for capacidade in quatro {
            assert!(cinco.contains(capacidade));
        }
        let tres = capacidades_da_api(3);
        assert!(quatro.contains(&"superficies"));
        assert!(quatro.contains(&"contribuicoes"));
        assert!(
            !tres.contains(&"superficies"),
            "a API 3 recebeu por acidente uma capacidade da 4",
        );
        // O que a 3 tinha, a 4 continua tendo: a 4 não tirou nada.
        for capacidade in tres {
            assert!(
                quatro.contains(capacidade),
                "a API 4 perdeu «{capacidade}», que a 3 tinha",
            );
        }
        assert!(capacidades_da_api(2).is_empty());
    }

    #[test]
    fn um_identificador_sem_autor_e_recusado() {
        let texto = manifesto_minimo().replace("\"seele/exemplo\"", "\"exemplo\"");
        assert!(matches!(read_manifest(&texto), Err(Refused::MalformedId)));
    }

    /// Um MOD que não declara nenhuma das duas metades não faz nada, e um MOD
    /// que não faz nada instalado em silêncio é a instalação parcial silenciosa
    /// que o ADR 0029 nomeia e o 0045 herda.
    #[test]
    fn um_mod_sem_nenhuma_das_duas_metades_e_recusado() {
        let texto = format!(
            r#"{{"schema":{MANIFEST_SCHEMA},"id":"seele/exemplo","version":"1.0.0",
                "api":{MOD_API_VERSION},"repo":"https://example.invalid/x"}}"#
        );
        assert_eq!(read_manifest(&texto), Err(Refused::Empty));
    }

    /// `reach` e `state` são lidos e ainda não valem nada, e é de propósito.
    ///
    /// ADR 0029, sobre `ansi256` e `ansi16`: «o arquivo que alguém escrever
    /// hoje já está completo no dia em que o produto os ler». Sem eles no
    /// esquema 1, `deny_unknown_fields` recusaria amanhã os manifestos escritos
    /// corretamente hoje — e o esquema só cresce, então não há conserto barato.
    #[test]
    fn reach_e_state_sao_lidos_mesmo_sem_consumidor() {
        let lido = read_manifest(&manifesto_minimo()).expect("manifesto válido recusado");
        assert_eq!(lido.reach, vec!["dom".to_owned()]);
        assert_eq!(lido.state, Some(1));
    }

    /// Um MOD que não pede alcance nenhum é legítimo — um MOD de cor não
    /// alcança nada além do que a janela já lhe dá.
    #[test]
    fn um_manifesto_sem_reach_nem_state_e_lido() {
        let texto = format!(
            r#"{{"schema":{MANIFEST_SCHEMA},"id":"seele/exemplo","version":"1.0.0",
                "api":{MOD_API_VERSION},"repo":"https://example.invalid/x",
                "client":"cliente/main.js"}}"#
        );
        let lido = read_manifest(&texto).expect("manifesto sem os opcionais recusado");
        assert!(lido.reach.is_empty());
        assert_eq!(lido.state, None);
    }

    fn arquivos() -> Vec<(String, Vec<u8>)> {
        vec![
            ("mod.json".to_owned(), b"{}".to_vec()),
            ("cliente/main.js".to_owned(), b"console.log(1)".to_vec()),
        ]
    }

    #[test]
    fn o_mesmo_conteudo_da_o_mesmo_hash() {
        let mut a = arquivos();
        let mut b = arquivos();
        assert_eq!(content_hash(&mut a), content_hash(&mut b));
    }

    /// A ordem em que o disco devolve os arquivos não pode mudar a identidade
    /// de um MOD: um mesmo diretório em duas máquinas tem de dar o mesmo
    /// número, ou a conferência do `mod://` recusa o MOD certo.
    #[test]
    fn a_ordem_dos_arquivos_nao_muda_o_hash() {
        let mut direta = arquivos();
        let mut invertida = arquivos();
        invertida.reverse();
        assert_eq!(content_hash(&mut direta), content_hash(&mut invertida));
    }

    #[test]
    fn um_byte_diferente_da_um_hash_diferente() {
        let mut original = arquivos();
        let mut mexido = arquivos();
        mexido[1].1 = b"console.log(2)".to_vec();
        assert_ne!(content_hash(&mut original), content_hash(&mut mexido));
    }

    /// Sem separador contado, `a/bc` + `d` e `a/b` + `cd` colidiriam, e dois
    /// MODs diferentes teriam a mesma identidade.
    #[test]
    fn mover_bytes_do_nome_para_o_conteudo_muda_o_hash() {
        let mut um = vec![("ab".to_owned(), b"c".to_vec())];
        let mut outro = vec![("a".to_owned(), b"bc".to_vec())];
        assert_ne!(content_hash(&mut um), content_hash(&mut outro));
    }

    /// A primeira coisa que toca texto de terceiro. Totalidade é o ponto —
    /// mesma disciplina de `version::negotiate`.
    #[test]
    fn nenhuma_entrada_causa_panico() {
        for texto in ["", "{", "null", "[]", "{\"schema\":}", "\u{0}"] {
            let _ = read_manifest(texto);
        }
    }
    // ------------------------------------------------- o conjunto anunciado

    fn anunciado(id: &str, version: &str, hash: &str) -> ModAnunciado {
        ModAnunciado {
            id: id.to_owned(),
            version: version.to_owned(),
            hash: hash.to_owned(),
            repo: "https://github.com/seele/exemplo".to_owned(),
            reach: vec!["dom".to_owned()],
            no_servidor: false,
        }
    }

    fn conjunto() -> Vec<ModAnunciado> {
        vec![
            anunciado("seele/cor", "1.0.0", &"a1".repeat(32)),
            anunciado("seele/placar", "2.1.0", &"b2".repeat(32)),
        ]
    }

    #[test]
    fn o_mesmo_conjunto_da_a_mesma_identidade() {
        assert_eq!(
            identidade_do_conjunto(&mut conjunto()),
            identidade_do_conjunto(&mut conjunto())
        );
    }

    /// A ordem em que o banco devolve as linhas não pode mudar a identidade: o
    /// aceite que uma pessoa guardou tem de continuar valendo na reconexão
    /// seguinte, e `ORDER BY` não é promessa que atravesse versão de SQLite.
    #[test]
    fn a_ordem_das_linhas_nao_muda_a_identidade() {
        let mut invertido = conjunto();
        invertido.reverse();
        assert_eq!(
            identidade_do_conjunto(&mut conjunto()),
            identidade_do_conjunto(&mut invertido)
        );
    }

    /// ADR 0045: «um servidor que troca de MOD pergunta de novo». Sem isto,
    /// quem já tinha aceito entraria sem nunca ter lido o MOD novo.
    #[test]
    fn acrescentar_um_mod_muda_a_identidade() {
        let mut com_mais_um = conjunto();
        com_mais_um.push(anunciado("seele/tunel", "1.0.0", &"c3".repeat(32)));
        assert_ne!(
            identidade_do_conjunto(&mut conjunto()),
            identidade_do_conjunto(&mut com_mais_um)
        );
    }

    #[test]
    fn tirar_um_mod_muda_a_identidade() {
        let mut com_um_so = vec![conjunto().swap_remove(0)];
        assert_ne!(
            identidade_do_conjunto(&mut conjunto()),
            identidade_do_conjunto(&mut com_um_so)
        );
    }

    /// Subir a versão de um MOD é trocar o MOD, e o aceite anterior não vale.
    #[test]
    fn subir_a_versao_de_um_mod_muda_a_identidade() {
        let mut outra_versao = conjunto();
        outra_versao[0].version = "1.0.1".to_owned();
        assert_ne!(
            identidade_do_conjunto(&mut conjunto()),
            identidade_do_conjunto(&mut outra_versao)
        );
    }

    /// Os mesmos nome e versão com outros bytes é outro MOD, e é o caso que o
    /// nome sozinho não pega — o motivo de o hash existir (ADR 0026,
    /// alternativa 5).
    #[test]
    fn outros_bytes_sob_o_mesmo_nome_mudam_a_identidade() {
        let mut remendado = conjunto();
        remendado[0].hash = "f0".repeat(32);
        assert_ne!(
            identidade_do_conjunto(&mut conjunto()),
            identidade_do_conjunto(&mut remendado)
        );
    }

    /// Sem tamanho contado antes de cada campo, `("ab","c")` e `("a","bc")`
    /// colidiriam — e dois conjuntos diferentes teriam a mesma identidade.
    #[test]
    fn mover_bytes_de_um_campo_para_o_outro_muda_a_identidade() {
        let mut um = vec![anunciado("a/b", "10", "0")];
        let mut outro = vec![anunciado("a/b", "1", "00")];
        assert_ne!(
            identidade_do_conjunto(&mut um),
            identidade_do_conjunto(&mut outro)
        );
    }

    /// `repo` e `reach` saem do `mod.json`, e o `mod.json` está dentro do hash:
    /// contá-los de novo seria contar a mesma coisa duas vezes. Este teste
    /// prende a decisão para que ela não vire acidente.
    #[test]
    fn o_que_o_hash_ja_cobre_nao_entra_na_identidade() {
        let mut outro_repo = conjunto();
        outro_repo[0].repo = "https://example.invalid/outro".to_owned();
        outro_repo[0].reach = vec!["ler".to_owned()];
        outro_repo[0].no_servidor = true;
        assert_eq!(
            identidade_do_conjunto(&mut conjunto()),
            identidade_do_conjunto(&mut outro_repo)
        );
    }

    #[test]
    fn o_hash_de_um_conteudo_e_um_hash_de_conteudo() {
        let mut arquivos = arquivos();
        assert!(e_hash_de_conteudo(&hex(&content_hash(&mut arquivos))));
    }

    /// Caixa fixa, e o motivo não é purismo: a identidade do conjunto é
    /// calculada sobre este texto, então maiúscula e minúscula dariam duas
    /// identidades para os mesmos bytes, e um aceite guardado deixaria de valer
    /// por causa de uma letra.
    #[test]
    fn um_hash_que_nao_e_hexadecimal_minusculo_de_64_e_recusado() {
        for ruim in [
            String::new(),
            "a".repeat(63),
            "a".repeat(65),
            "A1".repeat(32),
            "g1".repeat(32),
            " ".repeat(64),
        ] {
            assert!(!e_hash_de_conteudo(&ruim), "`{ruim}` passou por hash");
        }
    }

    /// O anúncio não saía enquanto a versão global não o alcançasse, e a linha
    /// que decidia isso era uma só. Este teste ficou preso aqui para que a
    /// integração conjunta com a malha fosse uma decisão e não um descuido — e
    /// agora diz a outra metade: a versão global alcançou, e o anúncio vale.
    ///
    /// **A igualdade das duas é o que o guarda protege**, e não o número 5 em
    /// si. Um anúncio acima da global volta a ser dormente; um anúncio abaixo
    /// dela sairia para pares que não sabem lê-lo, e o postcard não ignora uma
    /// variante desconhecida — ele desloca a leitura do fluxo para sempre.
    #[test]
    fn o_anuncio_pede_uma_versao_que_a_global_alcanca() {
        // O anúncio continua pedindo 5 e a global passou a 7: ele pede um
        // **limiar**, e o que o guarda protege é que a global o alcance — não
        // que os dois números sejam iguais. A subida para 7 não mexeu no
        // anúncio, e não devia: nenhuma variante dele mudou.
        assert_eq!(VERSAO_DO_ANUNCIO, 5);
        // `const` e não `assert!` de execução: os dois lados são constantes, e
        // o clippy recusa a afirmação de valor fixo — com razão. O que se quer
        // é que **o compilador** a verifique, e ele verifica: a desigualdade
        // deixa de compilar no dia em que o anúncio pedir mais do que a global
        // entrega, que é exatamente quando ela tem de morder.
        const { assert!(crate::version::PROTOCOL_VERSION >= VERSAO_DO_ANUNCIO) };
        // A desigualdade que importa — `VERSAO_DO_ANUNCIO <= PROTOCOL_VERSION`,
        // sem a qual o portão volta a ser dormente — já está dita pelas duas
        // igualdades acima, e escrevê-la de novo seria uma asserção de valor
        // constante que o clippy recusa com razão. Quem a cobra como relação, e
        // não como par de números, é
        // `version::tests::a_versao_subiu_uma_vez_so_para_a_malha_e_para_os_mods`.
    }
}
