//! Enforces the dependency rule from `specs/01-arquitetura.md`.
//!
//! > `proto` depends on nobody. `audio` depends only on `proto`. `core` depends
//! > on `proto` and `audio`. Everything else depends on `core`. Never the
//! > inverse.
//!
//! `specs/01-arquitetura.md` also calls the core/shell boundary "the most
//! important contract in the project — if it leaks, the project becomes three
//! applications". A contract checked only by review is a contract that erodes,
//! so this turns it into a red build.
//!
//! # Why this check is not redundant with Cargo
//!
//! Cargo already rejects an inverted *normal* dependency, because among the
//! existing crates every inversion forms a cycle. This check earns its keep on
//! the two classes Cargo does not catch:
//!
//! - **Dev-dependencies.** Cargo tolerates cycles through them, so
//!   `proto` dev-depending on `audio` compiles fine and silently inverts the
//!   graph.
//! - **Sideways edges.** `seele-tui` depending directly on `seele-proto` is not a
//!   cycle, so Cargo is happy — but it is exactly how protocol knowledge leaks
//!   into a shell.
//!
//! # Deliberate divergence from the literal text of specs/01-arquitetura.md
//!
//! The spec says "everything else depends on `core`". Taken literally that is
//! wrong in two places, and this table encodes the stricter reading instead.
//! See `docs/adr/0002-regra-de-dependencia.md`.
//!
//! - `seele-server` must **not** depend on `seele-core`, which is the headless
//!   *client*, nor on `seele-audio`. The server is an SFU: `specs/04` states it
//!   "never decodes Opus". Linking the audio crate would drag `cpal` and
//!   `libopus` into a daemon that is supposed to fit in 1 vCPU / 512 MB.
//! - Shells depend on `seele-core` **only**, not additionally on `proto` and
//!   `audio`. A shell that can name an `ssrc` already has logic in it.

use std::process::ExitCode;

use cargo_metadata::{DependencyKind, MetadataCommand};

/// The allow-list. See the module docs for where it tightens the spec.
///
/// A workspace crate absent from this table is a hard error rather than an
/// implicit pass: adding a crate must be a deliberate act that states where the
/// new crate sits in the graph.
const RULES: &[(&str, &[&str])] = &[
    ("seele-proto", &[]),
    // O ponto de encontro do degrau 4 do ADR 0022. Uma folha, ao lado de
    // `seele-audio`: vê o formato do que passa pelo fio e mais nada.
    //
    // A aresta que importa é a que **não** existe. Este processo apresenta duas
    // máquinas e sai do caminho; se um dia ele precisar de `seele-core` ou de
    // `seele-server`, quer dizer que deixou de ser uma apresentação e virou
    // parte da conversa — que é o degrau 5, e o ADR 0022 o deixou de fora por
    // decisão. A tabela é onde essa mudança teria de ser argumentada.
    ("seele-encontro", &["seele-proto"]),
    ("seele-audio", &["seele-proto"]),
    // Compartilhamento de tela: captura e codec, ao lado de `seele-audio` e com
    // exatamente a mesma vizinhança. Não é um degrau novo no grafo, é a segunda
    // folha de mídia.
    //
    // **A aresta que não existe é a que importa.** Este crate faz `dlopen` do
    // módulo do Cisco e codifica H.264; se um dia ele precisar de `seele-core`,
    // quer dizer que a decisão de o que transmitir e de quando parar migrou para
    // dentro do codec — e o §3.2 da spec de compartilhamento de tela põe essa
    // decisão do outro lado, pendurada no sinal da voz que `seele-core` já
    // calcula. A tabela é onde essa mudança teria de ser argumentada.
    ("seele-video", &["seele-proto"]),
    ("seele-core", &["seele-proto", "seele-audio", "seele-video"]),
    // The daemon speaks the wire format and nothing else.
    ("seele-server", &["seele-proto"]),
    // Shells translate events into pixels and input into commands. Nothing more.
    //
    // A exceção do `seele-server` é nomeada e tem **dois** motivos, e os dois
    // dizem a mesma coisa: este binário contém os dois papéis.
    //
    // 1. `--hospedar` sobe um servidor no próprio processo, para que quem hospeda
    //    entre amigos não precise saber o que é um daemon.
    // 2. `--rede` (`crates/seele-tui/src/rede.rs`) diagnostica o alcance **de
    //    quem hospeda**: enumera as interfaces por `alcance::interfaces`, abre o
    //    socket por `alcance::abrir_escuta`, e fala `SEELE-ENC/1` com pontos de
    //    encontro pela lista de reexportações de `seele_server::encontro`.
    //
    // Nenhum dos dois inverte a regra: o servidor não passa a conhecer o
    // cliente. É uma aresta lateral no topo do grafo.
    //
    // **Cuidado com o que esta checagem não vê.** Ela lê o grafo do Cargo, e uma
    // reexportação não é uma aresta do Cargo: `seele-server` reexportando um
    // pedaço de `seele-proto` passa por aqui em silêncio, e é assim que
    // `seele-tui` alcança o formato do fio do ponto de encontro sem declarar
    // dependência nenhuma. A guarda daquilo é a lista explícita em
    // `crates/seele-server/src/lib.rs` e a revisão dela — não este arquivo.
    ("seele-tui", &["seele-core", "seele-server"]),
    ("seele-ffi", &["seele-core"]),
    // The one deliberate exception, and it has a name so it cannot spread.
    // An end-to-end protocol test needs a server and a client at once, and the
    // rule above forbids either depending on the other. This crate ships
    // nothing: every dependency is a dev-dependency and its library is empty.
    (
        "seele-conformance",
        // The crate that proves the others meet the acceptance criteria is the
        // one place allowed to see all of them, both shells included.
        &[
            "seele-proto",
            "seele-audio",
            "seele-core",
            "seele-server",
            "seele-tui",
            "seele-ffi",
            // **O ponto de encontro, e é o mesmo argumento das duas cascas.**
            //
            // O quarto do ADR 0022 (emenda de 03/09/2026) só se prova com as
            // três peças ao mesmo tempo: um anfitrião registrando, um cliente
            // perguntando, e o serviço de verdade no meio. Sem esta linha o
            // teste teria de encenar o serviço — e um teste que encena a peça
            // que está sendo medida não mede nada.
            //
            // Não inverte a regra: `seele-encontro` é folha, vê `seele-proto` e
            // mais nada, e ninguém depende dele. É a mesma aresta lateral no
            // topo do grafo que `seele-server` já é aqui dentro.
            "seele-encontro",
        ],
    ),
    // The desktop shell. Sees `seele-ffi`, which sees `seele-core`. Reaching past
    // it would put protocol knowledge in a Tauri command — specs/06-clientes-gui.md.
    // Mesma exceção, mesmo motivo: o botão **Hospedar**. Ver `seele-tui`.
    ("seele-app", &["seele-ffi", "seele-server"]),
    // Tooling. Must not depend on the product, or `cargo xtask` would need the
    // product to compile before it could check the product.
    ("xtask", &[]),
    // O instalador, pela mesma razão e com a mesma força — ADR 0043. Se ele
    // dependesse do `seele-core`, construir o instalador exigiria construir o
    // produto, e uma mudança no produto poderia quebrar a instalação de todo
    // mundo por um caminho que ninguém liga aos dois.
    ("seele-instalador", &[]),
];

/// One offending edge in the workspace graph.
#[derive(Debug, PartialEq, Eq)]
struct Violation {
    message: String,
}

/// Is this a crate the architecture rule governs?
fn is_workspace_crate(name: &str) -> bool {
    RULES.iter().any(|(crate_name, _)| *crate_name == name)
}

/// Pure rule evaluation for a single crate, kept free of `cargo_metadata` so it
/// can be tested without a workspace on disk.
fn evaluate(name: &str, edges: &[(String, &'static str)]) -> Vec<Violation> {
    let Some((_, allowed)) = RULES.iter().find(|(crate_name, _)| *crate_name == name) else {
        return vec![Violation {
            message: format!(
                "crate `{name}` is a workspace member but has no entry in RULES.\n    \
                 Add it to xtask/src/check_deps.rs and state which crates it may depend on."
            ),
        }];
    };

    edges
        .iter()
        .filter(|(dependency, _)| is_workspace_crate(dependency))
        .filter(|(dependency, _)| !allowed.contains(&dependency.as_str()))
        .map(|(dependency, kind)| Violation {
            message: format!(
                "`{name}` declares a {kind} on `{dependency}`, which the dependency \
                 rule forbids.\n    \
                 Allowed for `{name}`: {}.\n    \
                 specs/01-arquitetura.md: proto → audio → core → shells, never the inverse.",
                if allowed.is_empty() {
                    "nothing in the workspace".to_owned()
                } else {
                    allowed.join(", ")
                }
            ),
        })
        .collect()
}

fn describe(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Development => "dev-dependency",
        DependencyKind::Build => "build-dependency",
        _ => "dependency",
    }
}

pub(crate) fn run() -> ExitCode {
    let metadata = match MetadataCommand::new().no_deps().exec() {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("xtask check-deps: could not read cargo metadata: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut violations: Vec<Violation> = Vec::new();
    let mut checked = 0_usize;

    for package in metadata.workspace_packages() {
        let name = package.name.to_string();
        let edges: Vec<(String, &'static str)> = package
            .dependencies
            .iter()
            .map(|dependency| (dependency.name.to_string(), describe(dependency.kind)))
            .collect();

        violations.extend(evaluate(&name, &edges));
        checked += 1;
    }

    if violations.is_empty() {
        println!("check-deps: dependency rule holds across {checked} workspace crates.");
        return ExitCode::SUCCESS;
    }

    eprintln!("check-deps: dependency rule violated.\n");
    for violation in &violations {
        eprintln!("  - {}", violation.message);
    }
    eprintln!();
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(name: &str) -> (String, &'static str) {
        (name.to_owned(), "dependency")
    }

    fn dev_edge(name: &str) -> (String, &'static str) {
        (name.to_owned(), "dev-dependency")
    }

    #[test]
    fn allowed_edges_pass() {
        assert!(evaluate("seele-audio", &[edge("seele-proto")]).is_empty());
        assert!(evaluate("seele-core", &[edge("seele-proto"), edge("seele-audio")]).is_empty());
        assert!(evaluate("seele-tui", &[edge("seele-core")]).is_empty());
    }

    #[test]
    fn third_party_edges_are_ignored() {
        assert!(evaluate("seele-proto", &[edge("serde"), edge("postcard")]).is_empty());
    }

    #[test]
    fn inverted_edge_is_rejected() {
        assert_eq!(evaluate("seele-proto", &[edge("seele-core")]).len(), 1);
        assert_eq!(evaluate("seele-audio", &[edge("seele-core")]).len(), 1);
    }

    #[test]
    fn inverted_dev_dependency_is_rejected() {
        // Cargo tolerates cycles through dev-dependencies; this is the class of
        // violation that only this check can catch.
        assert_eq!(evaluate("seele-proto", &[dev_edge("seele-audio")]).len(), 1);
    }

    #[test]
    fn shell_reaching_past_core_is_rejected() {
        // Not a cycle, so Cargo is perfectly happy — and this is exactly how
        // protocol knowledge leaks into a shell.
        assert_eq!(evaluate("seele-tui", &[edge("seele-proto")]).len(), 1);
        assert_eq!(evaluate("seele-ffi", &[edge("seele-audio")]).len(), 1);
    }

    #[test]
    fn server_must_not_link_the_client_or_the_audio_stack() {
        assert_eq!(evaluate("seele-server", &[edge("seele-core")]).len(), 1);
        assert_eq!(evaluate("seele-server", &[edge("seele-audio")]).len(), 1);
        assert!(evaluate("seele-server", &[edge("seele-proto")]).is_empty());
    }

    #[test]
    fn the_conformance_crate_may_see_both_ends() {
        // The documented exception. It exists so an end-to-end test has a home
        // that is not a hole in the rule.
        assert!(evaluate(
            "seele-conformance",
            &[
                edge("seele-server"),
                edge("seele-core"),
                edge("seele-proto"),
                // E o ponto de encontro: o quarto do ADR 0022 só se prova com o
                // serviço de verdade no meio das duas pontas.
                edge("seele-encontro"),
            ]
        )
        .is_empty());
    }

    #[test]
    fn the_exception_does_not_leak_to_anybody_else() {
        // If a second crate ever needs both ends, that is a design change to
        // argue about, not a table entry to copy.
        assert_eq!(evaluate("seele-server", &[edge("seele-core")]).len(), 1);
        assert_eq!(evaluate("seele-core", &[edge("seele-server")]).len(), 1);
    }

    #[test]
    fn the_meeting_point_sees_the_wire_format_and_nothing_else() {
        // ADR 0022, degrau 4: o ponto de encontro participa da apresentação e
        // não da conversa. Uma aresta daqui para o servidor ou para o cliente
        // seria a assinatura de que ele passou a participar da conversa.
        assert!(evaluate("seele-encontro", &[edge("seele-proto")]).is_empty());
        assert_eq!(evaluate("seele-encontro", &[edge("seele-core")]).len(), 1);
        assert_eq!(evaluate("seele-encontro", &[edge("seele-server")]).len(), 1);
    }

    #[test]
    fn the_screen_crate_is_a_leaf_next_to_audio() {
        // Mesma vizinhança de `seele-audio`, e pelo mesmo motivo: mídia vê o
        // formato do fio e mais nada. Uma aresta daqui para `seele-core` seria a
        // assinatura de que a decisão de quando o vídeo cede migrou para dentro
        // do codec, e o §3.2 da spec a põe do outro lado.
        assert!(evaluate("seele-video", &[edge("seele-proto")]).is_empty());
        assert_eq!(evaluate("seele-video", &[edge("seele-core")]).len(), 1);
        assert_eq!(evaluate("seele-video", &[edge("seele-audio")]).len(), 1);
        // E o servidor continua sem enxergá-lo: `specs/04` diz que ele nunca
        // decodifica Opus, e o §5 da spec de tela diz que ele nunca encaminha
        // vídeo. Ligar este crate a um daemon de 1 vCPU seria as duas frases
        // deixando de valer de uma vez.
        assert_eq!(evaluate("seele-server", &[edge("seele-video")]).len(), 1);
    }

    #[test]
    fn unknown_workspace_crate_is_rejected() {
        assert_eq!(evaluate("seele-something-new", &[]).len(), 1);
    }
}
