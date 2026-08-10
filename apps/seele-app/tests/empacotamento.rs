//! O que é do empacotamento não pode virar exigência de quem só compila.
//!
//! `plug` e `seeled` viajam **dentro** do instalador do app, e o Tauri chama
//! isso de `externalBin`. A tentação é declarar isso no `tauri.conf.json`, e foi
//! o que eu fiz: o CI dos três sistemas quebrou na hora, porque o build script
//! do Tauri lê `externalBin` em **toda** compilação e exige os arquivos. Um
//! `cargo test --workspace` num clone limpo parava com
//! `resource path binaries/plug-… doesn't exist`.
//!
//! A separação é a mesma de sempre neste projeto, só que aplicada a build:
//! compilar o produto não pode depender de artefato que só o pipeline de
//! entrega produz. Os acompanhantes moram em `tauri.release.conf.json`, passado
//! com `--config` na hora de empacotar, e o Linux nem disso precisa — lá o
//! `.deb` põe os dois em `/usr/bin` pelo `deb.files`.
//!
//! O Linux, aliás, foi o único que passou naquele CI quebrado, porque tinha um
//! override. Um sistema verde entre três vermelhos é uma pista, não um alívio.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

fn app() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn ler(nome: &str) -> String {
    let caminho = app().join(nome);
    std::fs::read_to_string(&caminho).unwrap_or_else(|erro| panic!("não li {nome}: {erro}"))
}

#[test]
fn compilar_o_app_nao_exige_binario_nenhum_pronto() {
    // A regra. Quem clona o repositório e roda `cargo build` não tem
    // `apps/seele-app/binaries/` — ele é gerado pelo workflow de release, e
    // está no `.gitignore`.
    //
    // Num clone limpo este teste nunca chega a rodar: sem os arquivos, o
    // crate não compila e a falha aparece antes. Ele existe para o caso que
    // **de fato** acontece — alguém que acabou de empacotar tem `binaries/`
    // em disco, acrescenta `externalBin`, vê tudo funcionar na própria
    // máquina, e quebra os três sistemas no CI. Foi assim que quebrou.
    let config = ler("tauri.conf.json");
    assert!(
        !config.contains("externalBin"),
        "tauri.conf.json declara `externalBin`.\n\
         O build script do Tauri lê isso em toda compilação e exige os arquivos, \
         então um clone limpo para de compilar.\n\
         Acompanhante é assunto de empacotamento: vai em tauri.release.conf.json."
    );
}

#[test]
fn o_empacotamento_leva_as_duas_ferramentas() {
    // A outra metade da regra: separar não pode virar esquecer. Se este
    // arquivo sumir, o instalador volta a ser só a parte gráfica — que era o
    // problema que ele existe para resolver.
    let release = ler("tauri.release.conf.json");
    for ferramenta in ["binaries/plug", "binaries/seeled"] {
        assert!(
            release.contains(ferramenta),
            "tauri.release.conf.json não leva `{ferramenta}`: o instalador sairia \
             sem a metade de terminal"
        );
    }
}

#[test]
fn no_linux_as_ferramentas_vao_para_o_path() {
    // `.deb` é o formato escolhido justamente por isto. Se o mapeamento sumir,
    // o instalador do Linux vira só o app e ninguém percebe até tentar digitar
    // `plug` num terminal.
    let linux = ler("tauri.linux.conf.json");
    for destino in ["/usr/bin/plug", "/usr/bin/seeled"] {
        assert!(
            linux.contains(destino),
            "tauri.linux.conf.json não instala em `{destino}`"
        );
    }
    assert!(
        !linux.contains("externalBin"),
        "o Linux não deve levar acompanhante: os binários iriam duas vezes no \
         `.deb`, uma em /usr/bin e outra dentro do diretório do app"
    );
}

#[test]
fn o_app_declara_para_que_quer_o_microfone() {
    // Sem esta chave o macOS nega o microfone **sem perguntar nada**: nenhum
    // alerta, nenhuma entrada em Ajustes, e o programa recebe uma falha de
    // dispositivo. No SEELE isso aparecia como "ÁUDIO LOCAL FALHANDO" — texto
    // certo para a coisa errada, porque a máquina não estava falhando.
    //
    // A TUI nunca sofreu disso, e a diferença enganava: a permissão é atribuída
    // ao aplicativo que iniciou o processo, e o terminal já tem a dele.
    let plist = ler("Info.plist");
    assert!(
        plist.contains("NSMicrophoneUsageDescription"),
        "o Info.plist não diz para que o app quer o microfone; o macOS nega calado"
    );

    // E o direito, para o dia em que a assinatura entrar: o hardened runtime
    // que a notarização exige bloqueia o microfone sem ele.
    let direitos = ler("Entitlements.plist");
    assert!(
        direitos.contains("com.apple.security.device.audio-input"),
        "sem este direito o microfone volta a falhar assim que o app for assinado"
    );
}
