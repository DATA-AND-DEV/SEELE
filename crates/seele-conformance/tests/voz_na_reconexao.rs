//! As duas cascas têm que devolver a voz com os controles que ela tinha.
//!
//! # Por que este teste lê código-fonte
//!
//! O defeito que ele guarda não aparece em nenhum tipo e não quebra nenhuma
//! asserção de comportamento que caiba aqui: [`seele_core::Voice`] precisa de
//! hardware de áudio, e conformidade roda sem placa de som. O que sobra para
//! afirmar é a chamada — e ela é exatamente onde o erro esteve, duas vezes.
//!
//! É a mesma escolha que `apps/seele-app/tests/frontend.rs` já fez para o
//! JavaScript: quando o defeito é "a casca chamou a função errada" e o
//! compilador aceita as duas, o texto é o que resta.
//!
//! # O defeito, e por que ele é grave
//!
//! Numa reconexão o `ssrc` e o canal de mídia são novos, então a voz **tem**
//! que ser reaberta. Reabrir com [`seele_core::Voice::start`] ou `start_on`
//! constrói controles zerados: mudo desligado, Isolamento total
//! desligado, modo de volta em `PushToTalk`, ganhos por interlocutor perdidos.
//! `Voice::switch_capture` e as irmãs dela existem para carregar tudo isso por
//! cima da reabertura, e a documentação diz, com todas as letras, que a lista
//! mora no core "justamente para que nenhuma casca esqueça um item".
//!
//! Numa reconexão a chamada certa é `reopen`: ela carrega os controles **e**
//! pede de volta os dois dispositivos que aquela voz pediu. `switch_capture` e
//! `switch_playback` também carregam os controles, mas cada uma manda o outro
//! lado para um valor escolhido pela casca — numa reconexão isso é a casca
//! decidindo por onde a pessoa ouve, calada, no pior momento possível.
//!
//! As duas cascas esqueciam todos. E o que torna isto pior que "volta
//! desmutado" é a assimetria: `Enlace::tentar` **restaura** `muted` no
//! servidor ao reconectar, então o roster continua mostrando a pessoa muda,
//! enquanto o portão local — `speaking = open && !muted`, em `voice.rs` —
//! volta aberto. O indicador que todo mundo lê passa a mentir, e a primeira vez
//! que essa pessoa encosta na tecla de falar ela transmite achando que está
//! calada.

use std::path::{Path, PathBuf};

/// A raiz do repositório, a partir deste crate.
fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| panic!("este crate mora em crates/<nome>, e não moram mais"))
}

/// O trecho que trata `Aviso::Reconectado`, do braço até o começo do seguinte.
///
/// Recortado, e não o arquivo inteiro: as duas cascas mencionam `Voice::start`
/// no caminho de conexão, que é onde ele está certo. Um teste que lesse o
/// arquivo todo reprovaria o código correto, e um que buscasse só a menção
/// passaria com o braço errado — os dois falham do jeito que não se percebe.
fn braco_da_reconexao(fonte: &str, arquivo: &str) -> String {
    let Some(inicio) = fonte.find("Aviso::Reconectado") else {
        panic!("{arquivo} não trata `Aviso::Reconectado`; se o evento mudou de nome, este teste tem que mudar junto");
    };
    let resto = fonte
        .get(inicio + "Aviso::Reconectado".len()..)
        .unwrap_or("");
    let fim = resto.find("Aviso::").unwrap_or(resto.len());
    resto.get(..fim).unwrap_or("").to_owned()
}

#[test]
fn nenhuma_casca_reabre_a_voz_jogando_fora_os_controles() {
    let raiz = raiz();
    let cascas = [
        ("seele-ffi", "crates/seele-ffi/src/lib.rs"),
        ("seele", "crates/seele-tui/src/main.rs"),
    ];

    for (casca, caminho) in cascas {
        let fonte = std::fs::read_to_string(raiz.join(caminho))
            .unwrap_or_else(|erro| panic!("{caminho}: {erro}"));
        let braco = braco_da_reconexao(&fonte, caminho);

        assert!(
            braco.contains("reopen"),
            "a casca `{casca}` reabre a voz na reconexão sem `reopen`, então mudo, \
             Isolamento total, o modo e os ganhos voltam zerados. O roster continua \
             mostrando quem estava mudo como mudo — `Enlace::tentar` restaura isso no \
             servidor — e o microfone volta aberto."
        );

        for errada in [
            "Voice::start_on(",
            "Voice::start(",
            "Voice::start_preferring(",
        ] {
            assert!(
                !braco.contains(errada),
                "a casca `{casca}` chama `{errada}` no braço da reconexão. Ela constrói \
                 controles zerados; quem carrega os de agora por cima da reabertura é \
                 `reopen`."
            );
        }

        for um_lado_so in ["switch_capture(", "switch_playback("] {
            assert!(
                !braco.contains(um_lado_so),
                "a casca `{casca}` chama `{um_lado_so}` no braço da reconexão. Ela carrega os \
                 controles, mas manda o outro dispositivo para o que a casca escrever ali — \
                 e o que a casca escreve numa reconexão não é a escolha da pessoa. `reopen` \
                 pede de volta os dois que aquela voz pediu."
            );
        }
    }
}
