use super::{fonte_sem_comentarios, modo_local, subir_o_guardado};

/// A variável é do processo: um teste por vez, e ela volta ao que era.
struct ComOModoLocal(Option<std::ffi::OsString>);

impl ComOModoLocal {
    fn ligar() -> Self {
        let antes = std::env::var_os("SEELE_MODO_LOCAL");
        std::env::set_var("SEELE_MODO_LOCAL", "1");
        Self(antes)
    }
}

impl Drop for ComOModoLocal {
    fn drop(&mut self) {
        match self.0.take() {
            Some(valor) => std::env::set_var("SEELE_MODO_LOCAL", valor),
            None => std::env::remove_var("SEELE_MODO_LOCAL"),
        }
    }
}

#[test]
fn o_guardado_e_a_sala_sobem_so_em_loopback_e_sem_escada() {
    let _vez = super::a_entrada_velha_que_chega_tarde::vez();
    let _modo = ComOModoLocal::ligar();
    assert!(
        modo_local(),
        "a variável não ligou o modo local num build de debug"
    );
    super::a_entrada_velha_que_chega_tarde::runtime().block_on(async {
        let guardado = subir_o_guardado(
            0,
            seele_server::persistence::Location::Memory,
            None,
            |_| Ok(()),
            modo_local(),
        )
        .await
        .expect("o guardado no modo local");
        assert!(
            guardado.endereco().ip().is_loopback(),
            "o guardado do modo local atende fora do loopback: {}",
            guardado.endereco()
        );
        assert_eq!(
            guardado.alcance().map(|alcance| alcance.degrau().nome()),
            Some("SoEstaMaquina"),
            "o guardado do modo local subiu a escada"
        );
        guardado.encerrar().await;

        let casa = tempfile::tempdir().expect("a casa");
        let sala = super::sala::subir_em(&casa.path().display().to_string(), 0, None)
            .await
            .expect("a sala no modo local");
        assert!(
            sala.endereco().ip().is_loopback(),
            "a sala do modo local atende fora do loopback: {}",
            sala.endereco()
        );
        assert_eq!(
            sala.alcance().map(|alcance| alcance.degrau().nome()),
            Some("SoEstaMaquina"),
            "a sala do modo local subiu a escada"
        );
        sala.encerrar().await;
    });
}

/// Num build otimizado o modo é `false` constante, e a variável nem é lida.
#[test]
fn o_modo_local_nao_existe_num_build_otimizado() {
    let fonte = fonte_sem_comentarios();
    let otimizado = fonte
        .split("#[cfg(not(debug_assertions))]\npub(crate) const fn modo_local() -> bool {")
        .nth(1)
        .and_then(|resto| resto.split('}').next())
        .expect("a versão otimizada do modo local sumiu");
    assert_eq!(
        otimizado.trim(),
        "false",
        "o modo local num build otimizado faz outra coisa que não `false`"
    );
    let debug = fonte
        .split("#[cfg(debug_assertions)]\npub(crate) fn modo_local() -> bool {")
        .nth(1)
        .expect("a versão de debug do modo local sumiu ou perdeu o cfg");
    assert!(debug
        .trim_start()
        .starts_with("std::env::var(\"SEELE_MODO_LOCAL\")"));
}
