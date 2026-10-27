//! **O áudio de mentira do app, só na build de teste** (para o Codex navegar o
//! ciclo do relato de 25/10 por CUA sem microfone nem placa de som de verdade).
//!
//! Com o recurso `aparelho-de-mentira` do app **e** `SEELE_AUDIO_DE_MENTIRA=1`,
//! num build de debug, toda entrada abre o aparelho de mentira do `seele-audio`
//! (um tom de 440 Hz no microfone; a saída conta os blocos com som que tocou)
//! no lugar do CoreAudio, e uma linha do `seele.log` diz a cada 5 s as contagens
//! do aparelho, o foco de voz e, por sessão, a abertura do áudio e o envio.
//!
//! **Nunca num build otimizado:** o `seele-audio` recusa compilar o recurso
//! fora de debug (`compile_error!`), e sem o recurso este módulo devolve
//! `None`/`false` constantes — um id com o prefixo de mentira, sem o recurso,
//! cairia no aparelho padrão da máquina, e é exatamente o que não pode
//! acontecer. Ver o mesmo molde em `crate::modo_local`.

/// O nome do aparelho de mentira do app (o que as contagens leem).
#[cfg_attr(
    not(all(debug_assertions, feature = "aparelho-de-mentira")),
    allow(
        dead_code,
        reason = "só a build de teste do CUA abre o aparelho de mentira"
    )
)]
pub(crate) const NOME: &str = "cua";

/// Se o áudio de mentira está pedido e existe nesta build.
#[cfg(all(debug_assertions, feature = "aparelho-de-mentira"))]
pub(crate) fn ligado() -> bool {
    std::env::var("SEELE_AUDIO_DE_MENTIRA").as_deref() == Ok("1")
}

/// Sem o recurso, ou num build otimizado: nunca.
#[cfg(not(all(debug_assertions, feature = "aparelho-de-mentira")))]
#[allow(
    dead_code,
    reason = "fora da build de teste do CUA, só os testes perguntam"
)]
pub(crate) const fn ligado() -> bool {
    false
}

/// O id do aparelho que a entrada abre, quando o áudio de mentira está ligado.
#[cfg(all(debug_assertions, feature = "aparelho-de-mentira"))]
pub(crate) fn aparelho() -> Option<String> {
    ligado().then(|| seele_ffi::aparelho_de_mentira::id(NOME))
}

/// Sem o recurso: nenhum; a entrada abre o que a pessoa escolheu.
#[cfg(not(all(debug_assertions, feature = "aparelho-de-mentira")))]
pub(crate) const fn aparelho() -> Option<String> {
    None
}

/// **O relatório no `seele.log`**, a cada 5 s, enquanto o app vive: o que o
/// Codex lê para conferir o ciclo (o foco, o envio, os blocos com som).
#[cfg(all(debug_assertions, feature = "aparelho-de-mentira"))]
pub(crate) fn relatar<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    if !ligado() {
        return;
    }
    tracing::info!(aparelho = NOME, "áudio de mentira: ligado (build de teste)");
    let _ = std::thread::Builder::new()
        .name("seele-audio-de-mentira".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let contagem = seele_ffi::aparelho_de_mentira::contagem(NOME);
            let session = app.state::<crate::Session>();
            let foco = session.sessoes.com_o_foco().map(|sessao| sessao.id());
            let sessoes: Vec<String> = session
                .sessoes
                .todas()
                .iter()
                .map(|sessao| {
                    let retrato = sessao.conexao.snapshot();
                    format!(
                        "{}:{:?}:bitrate={}:recusados={}",
                        sessao.id(),
                        retrato.abertura_do_audio,
                        retrato.telemetry.bitrate_bps,
                        retrato.telemetry.frames_refused
                    )
                })
                .collect();
            tracing::info!(
                abertos = contagem.abertos,
                aberturas = contagem.aberturas,
                blocos = contagem.blocos,
                blocos_com_som = contagem.blocos_com_som,
                ?foco,
                sessoes = %sessoes.join(" "),
                "áudio de mentira: contagem"
            );
        });
}

/// Sem o recurso: nada.
#[cfg(not(all(debug_assertions, feature = "aparelho-de-mentira")))]
pub(crate) fn relatar<R: tauri::Runtime>(_app: tauri::AppHandle<R>) {}

#[cfg(test)]
mod o_audio_de_mentira {
    /// A variável é do processo: um teste destes por vez.
    static VEZ: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Sem a variável, nada muda: a entrada abre o que a pessoa escolheu.
    #[test]
    fn sem_a_variavel_nada_muda() {
        let _vez = VEZ
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if std::env::var_os("SEELE_AUDIO_DE_MENTIRA").is_none() {
            assert!(!super::ligado());
            assert_eq!(super::aparelho(), None);
        }
    }

    /// Num build otimizado, ou sem o recurso, as duas funções são constantes:
    /// o guarda lê o fonte, como o do modo local.
    #[test]
    fn fora_da_build_de_teste_e_constante() {
        let fonte = include_str!("audio_de_mentira.rs");
        let fora = "#[cfg(not(all(debug_assertions, feature = \"aparelho-de-mentira\")))]";
        assert!(fonte.contains(&format!(
            "{fora}\n#[allow(\n    dead_code,\n    reason = \"fora da build de teste do CUA, só os testes perguntam\"\n)]\n\
             pub(crate) const fn ligado() -> bool {{\n    false\n}}"
        )));
        assert!(fonte.contains(&format!(
            "{fora}\npub(crate) const fn aparelho() -> Option<String> {{\n    None\n}}"
        )));
    }

    /// Com o recurso (a build do CUA), a variável liga o aparelho de mentira.
    #[cfg(feature = "aparelho-de-mentira")]
    #[test]
    fn com_o_recurso_a_variavel_liga_o_aparelho_de_mentira() {
        let _vez = VEZ
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let antes = std::env::var_os("SEELE_AUDIO_DE_MENTIRA");
        std::env::set_var("SEELE_AUDIO_DE_MENTIRA", "1");
        let aparelho = super::aparelho();
        match antes {
            Some(valor) => std::env::set_var("SEELE_AUDIO_DE_MENTIRA", valor),
            None => std::env::remove_var("SEELE_AUDIO_DE_MENTIRA"),
        }
        assert_eq!(
            aparelho,
            Some(seele_ffi::aparelho_de_mentira::id(super::NOME))
        );
    }
}
