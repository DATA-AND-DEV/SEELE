//! O que é do empacotamento não pode virar exigência de quem só compila.
//!
//! `plug` e `seeled` viajam **dentro** do instalador do app, e o Tauri chama
//! isso de `externalBin`. A tentação é declarar isso no `tauri.conf.json`, e foi
//! o que eu fiz: o CI dos três sistemas quebrou na hora, porque o build script
//! do Tauri lê `externalBin` em **toda** compilação e exige os arquivos. Um
//! `cargo test --workspace` num clone limpo parava com
//! `resource path binaries/seele-… doesn't exist`.
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
    for ferramenta in ["binaries/seele", "binaries/seeled"] {
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
    // `seele` num terminal.
    let linux = ler("tauri.linux.conf.json");
    for destino in ["/usr/bin/seele", "/usr/bin/seeled"] {
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
fn o_build_ad_hoc_do_mac_nao_liga_o_hardened_runtime() {
    // As duas coisas juntas quebram a permissão de gravação de tela, e isso
    // custou quatro tentativas num teste de campo — depois de a chave do
    // `Info.plist` já estar no lugar.
    //
    // O hardened runtime é exigido pela **notarização**, que este build não
    // faz. Ligado sobre uma assinatura ad-hoc, o macOS fica sem Team ID para
    // formar um requisito estável, e a concessão de tela não gruda: conceder
    // nos Ajustes não muda nada, e as tentativas empilham entradas mortas — o
    // `tccutil` achou três do mesmo app.
    //
    // Desligado, o TCC guarda contra a impressão do binário e a concessão vale
    // enquanto o binário não mudar.
    //
    // **As duas voltam juntas ou nenhuma volta.** No dia em que houver conta de
    // desenvolvedor, `signingIdentity` deixa de ser `-` e aí sim o hardened
    // runtime volta a `true`. Religá-lo sozinho reintroduz o defeito inteiro.
    let limpo = ler("tauri.conf.json");
    let ad_hoc = limpo.contains("\"signingIdentity\": \"-\"");
    if ad_hoc {
        assert!(
            limpo.contains("\"hardenedRuntime\": false"),
            "assinatura ad-hoc com hardened runtime ligado: a permissão de \
             gravação de tela do macOS não gruda, e quem testar vai culpar o \
             app:\n{limpo}"
        );
    }
}

#[test]
fn o_app_declara_para_que_quer_a_tela() {
    // O mesmo defeito do microfone, a segunda vez. Sem esta chave o macOS nega
    // a gravação de tela **sem perguntar nada**, e o sintoma é a lista de telas
    // chegando vazia à interface — que quem olha lê como «este app não sabe
    // compartilhar», e não como «o sistema recusou».
    //
    // Custou um teste de campo: a 0.7.9 saiu sem a chave, o botão respondeu que
    // a funcionalidade não estava implementada, e ela estava. Nenhuma das três
    // ondas era dona deste arquivo.
    let plist = ler("Info.plist");
    assert!(
        plist.contains("NSScreenCaptureUsageDescription"),
        "o Info.plist não diz para que o app quer a tela; o macOS nega calado \
         e a interface parece quebrada"
    );

    // E o par que **não** existe, dito aqui para poupar a busca: não há direito
    // de hardened runtime para gravação de tela. Quem consertar «por simetria»
    // com o microfone vai procurar `com.apple.security.device.screen-capture` e
    // não vai achar, porque a Apple não o define.
    let direitos = ler("Entitlements.plist");
    assert!(
        !direitos.contains("screen-capture"),
        "apareceu um direito de captura de tela no Entitlements.plist, e ele \
         não existe — um direito inventado não é recusado, é ignorado, e some \
         no meio dos que valem"
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

#[test]
fn o_bundle_do_macos_e_assinado() {
    // Sem assinatura de **bundle** — e ter cada executável assinado pelo
    // linker não conta — o macOS não tem a que identidade prender a permissão
    // de microfone. O sintoma é cruel de diagnosticar: ele pergunta várias
    // vezes por sessão, a pessoa autoriza, e na vez seguinte pergunta de novo.
    // Medido no bundle antes do conserto: "code object is not signed at all".
    //
    // `-` é ad-hoc. Não engana o Gatekeeper, que continua exigindo
    // notarização, mas dá ao sistema uma identidade estável para guardar a
    // autorização — que é o problema que ele resolve.
    let config = ler("tauri.conf.json");
    assert!(
        config.contains("\"signingIdentity\""),
        "o bundle do macOS sairia sem assinatura, e a permissão de microfone \
         nunca ficaria guardada"
    );
}

/// O `tauri.conf.json` já analisado.
fn config() -> serde_json::Value {
    serde_json::from_str(&ler("tauri.conf.json")).expect("tauri.conf.json não é JSON válido")
}

#[test]
fn o_atualizador_esta_declarado_na_configuracao() {
    // Isto não é arranjo: `tauri_plugin_updater` declara o bloco
    // `plugins.updater` como **obrigatório**, com `pubkey` sem valor padrão.
    // Sem ele a inicialização do plugin falha, `Builder::run` devolve erro, e
    // `main` sai com código 1 — a janela não abre. Um app que não abre por
    // falta de três linhas de JSON é o tipo de defeito que ninguém suspeita,
    // porque o sintoma não fala de atualização.
    let config = config();
    let updater = &config["plugins"]["updater"];
    assert!(
        updater.is_object(),
        "tauri.conf.json não declara `plugins.updater`.\n\
         O plugin exige o bloco: sem ele o app não chega a abrir a janela."
    );
    assert!(
        updater
            .get("pubkey")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "`plugins.updater` sem `pubkey`. Vazia é o estado de repouso — enquanto \
         a chave do projeto não existir —, ausente não é."
    );

    // O endereço tem que ser https, e a recusa acontece **só em release**: em
    // desenvolvimento o plugin avisa e segue, então um `http` passaria por todo
    // teste local e quebraria no arquivo que as pessoas baixam.
    let enderecos = updater["endpoints"]
        .as_array()
        .expect("`plugins.updater.endpoints` tem que ser uma lista");
    assert!(!enderecos.is_empty(), "o atualizador não tem onde procurar");
    for endereco in enderecos {
        let endereco = endereco.as_str().unwrap_or_default();
        assert!(
            endereco.starts_with("https://"),
            "o endereço `{endereco}` não é https, e o plugin recusa isso em release"
        );
    }
}

#[test]
fn o_repositorio_nao_pede_artefatos_de_atualizacao() {
    // A outra metade, e a mais fácil de errar no dia em que a chave existir.
    //
    // `createUpdaterArtifacts` ligado faz a CLI do Tauri **exigir**
    // `TAURI_SIGNING_PRIVATE_KEY` no ambiente, e falhar sem ela — depois de
    // compilar tudo. Ligado no arquivo do repositório, quem clonou e rodou
    // `cargo tauri build` só para ver o app na própria máquina levaria um erro
    // sobre uma chave privada que não é dele e que ele não deveria ter.
    //
    // Quem liga isto é o `release.yml`, quando o segredo existe, e os scripts
    // de `empacotar/`, quando a variável está no ambiente. Nenhum dos dois
    // comita o arquivo: os dois o devolvem ao que era ao terminar.
    let config = config();
    assert!(
        config["bundle"].get("createUpdaterArtifacts").is_none(),
        "tauri.conf.json liga `createUpdaterArtifacts`.\n\
         Com ele o empacotamento exige a chave privada do projeto, e quem \
         clonou o repositório não a tem — o build passa a falhar no fim.\n\
         Quem liga isto é o release, com o segredo no ambiente."
    );
}
