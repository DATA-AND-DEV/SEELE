//! O que é do empacotamento não pode virar exigência de quem só compila.
//!
//! O `seeled` viaja **dentro** do instalador do app, e o Tauri chama
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
    let texto =
        std::fs::read_to_string(&caminho).unwrap_or_else(|erro| panic!("não li {nome}: {erro}"));
    // **Fim de linha normalizado**, e isto é conserto de um defeito que só o
    // Windows mostrava.
    //
    // Os guardas daqui procuram texto com `\n` dentro — `!macro NOME\n`, por
    // exemplo. O git faz checkout com CRLF no Windows, e lá as quatro provas do
    // firewall falhavam dizendo que o gancho não existia, num arquivo em que ele
    // está. No macOS passavam. Um guarda que reprova conforme o sistema em que
    // roda é um guarda que ensina a ser ignorado.
    //
    // Achado rodando a bateria numa máquina Windows de verdade, que é o que
    // esta sessão passou a conseguir fazer.
    texto.replace("\r\n", "\n")
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
fn o_empacotamento_leva_o_servidor_e_nao_leva_o_cliente_de_terminal() {
    // A outra metade da regra: separar não pode virar esquecer. Se este
    // arquivo sumir, o instalador volta a ser só a parte gráfica — que era o
    // problema que ele existe para resolver.
    //
    // **Este teste cobrava `binaries/seele` junto até 2026-08-24**, com a frase
    // «o instalador sairia sem a metade de terminal». O cliente de terminal
    // deixou de ser distribuído, e a razão está registrada: enquanto ele ia
    // dentro do instalador, a instalação não distinguia quem o quis de quem o
    // recebeu — e é essa distinção que decide se ele tem público.
    //
    // A ausência é cobrada, e não só deixada de cobrar. Um teste que apenas
    // perdesse um item da lista deixaria a volta acontecer sem que ninguém
    // percebesse, que é exatamente o que a versão anterior existia para impedir.
    let release = ler("tauri.release.conf.json");
    assert!(
        release.contains("binaries/seeled"),
        "tauri.release.conf.json não leva `binaries/seeled`: o instalador sairia \
         sem o servidor, e hospedar deixaria de funcionar"
    );
    assert!(
        !release.contains("binaries/seele\""),
        "tauri.release.conf.json voltou a levar o cliente de terminal — se isso \
         é intencional, é decisão de produto e o comentário acima precisa mudar \
         junto"
    );
}

#[test]
fn no_linux_o_servidor_vai_para_o_path() {
    // `.deb` é o formato escolhido justamente por isto. Se o mapeamento sumir,
    // o instalador do Linux vira só o app e ninguém percebe até tentar digitar
    // `seeled` num terminal.
    //
    // **Cobrava `/usr/bin/seele` junto até 2026-08-24**, pelo mesmo motivo e com
    // a mesma data do teste acima: o cliente de terminal saiu do pacote. Sem o
    // mapeamento, o `.deb` deixaria `/usr/bin/seele` órfão apontando para um
    // arquivo que não é mais empacotado.
    let linux = ler("tauri.linux.conf.json");
    assert!(
        linux.contains("/usr/bin/seeled"),
        "tauri.linux.conf.json não instala em `/usr/bin/seeled`"
    );
    assert!(
        !linux.contains("/usr/bin/seele\""),
        "tauri.linux.conf.json voltou a instalar o cliente de terminal"
    );
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

#[test]
fn o_modulo_de_video_e_procurado_fora_do_pacote() {
    // O defeito que este teste tranca custou dois testes de campo num dia.
    //
    // O módulo do OpenH264 morava dentro do pacote, e um pacote é da versão,
    // não do computador: toda instalação nova chega vazia, e quem já tinha o
    // módulo volta a ver que ele falta. Nas duas vezes eu procurei o erro no
    // empacotamento, e nas duas ele estava em **onde o arquivo era guardado**.
    //
    // Ao lado do banco ele é do computador. Se alguém devolver a busca para
    // dentro do pacote — ou trocar `config_dir` por um caminho relativo ao
    // executável — o ciclo recomeça, e demora uma versão inteira para
    // aparecer, porque na máquina de quem compila o módulo sempre está lá.
    let main = ler("src/main.rs");
    let mut onde = main.match_indices("SEELE_OPENH264").peekable();
    assert!(
        onde.peek().is_some(),
        "o app não aponta mais onde procurar o módulo de vídeo"
    );
    // Uma vizinhança, e não a linha: `config_dir` é chamado algumas linhas
    // acima do `set_var`, e exigir os dois na mesma linha só ensinaria a
    // próxima pessoa a reescrever o teste.
    let perto = onde.any(|(inicio, _)| {
        let comeco = inicio.saturating_sub(700);
        let fim = main.len().min(inicio + 700);
        main.get(comeco..fim)
            .is_some_and(|volta| volta.contains("config_dir"))
    });
    assert!(
        perto,
        "o módulo de vídeo voltou a ser procurado num caminho que não é a pasta \
         de configuração; toda atualização vai apagá-lo de novo"
    );
}

/// Só o corpo de um dos ganchos do `instalador.nsh`.
///
/// Recortado porque as asserções deste arquivo falam de **ordem dentro de um
/// gancho**, e procurar no arquivo inteiro daria verde para um `delete` que
/// mora no gancho de desinstalar e um `add` que mora no de instalar — que é
/// justamente o arranjo errado que estes testes existem para pegar.
fn gancho(nome: &str) -> String {
    let nsh = ler("instalador.nsh");
    let inicio = nsh
        .find(&format!("!macro {nome}\n"))
        .unwrap_or_else(|| panic!("o instalador.nsh não tem o gancho {nome}"));
    let resto = &nsh[inicio..];
    let fim = resto
        .find("!macroend")
        .unwrap_or_else(|| panic!("o gancho {nome} não fecha com !macroend"));
    resto[..fim].to_string()
}

#[test]
fn o_instalador_cria_a_regra_de_firewall_de_entrada() {
    // No Windows, um programa que escuta não recebe de fora sem regra. Sem
    // este gancho a regra só nasce se a pessoa acertar um diálogo que aparece
    // uma vez — e que não aparece de novo se ela errar. O sintoma de errar é o
    // pior que este projeto tem: o anfitrião sobe o servidor, vê tudo verde do
    // lado dele, e ninguém consegue entrar.
    let pos = gancho("NSIS_HOOK_POSTINSTALL");
    assert!(
        pos.contains("advfirewall firewall add rule"),
        "o gancho de pós-instalação não cria mais a regra de firewall; \
         quem hospedar no Windows volta a depender de acertar o diálogo do sistema"
    );
    assert!(
        pos.contains("dir=in") && pos.contains("protocol=udp"),
        "a regra de firewall deixou de ser entrada em UDP.\n\
         O transporte é QUIC, que é UDP, e é entrada que o Windows barra: \
         uma regra de saída ou de TCP não resolve nada e parece resolver"
    );
}

#[test]
fn a_regra_de_firewall_e_por_programa_e_nao_por_porta() {
    // Duas razões, e a segunda é a que morde.
    //
    // A porta do encontro é a 8384, ao lado da 8383 do servidor, então uma
    // regra presa a uma porta deixaria metade do caminho fechada.
    //
    // E `alcance::firewall::ha_regra_para` reconhece a nossa regra comparando a
    // linha `Program` da saída do `netsh`. Uma regra por porta não tem essa
    // linha: ela funcionaria, e o código que confere continuaria respondendo
    // `Barrada` para uma máquina liberada.
    let pos = gancho("NSIS_HOOK_POSTINSTALL");
    assert!(
        pos.contains("program="),
        "a regra de firewall deixou de ser presa ao executável"
    );
    assert!(
        !pos.contains("localport="),
        "a regra de firewall virou regra de porta.\n\
         Isso abre a porta para qualquer programa, deixa a 8384 do encontro de \
         fora, e fica invisível para `alcance::firewall::ha_regra_para`, que \
         compara a linha `Program`"
    );
}

#[test]
fn a_regra_de_firewall_e_apagada_antes_de_ser_criada() {
    // O `netsh` aceita duas regras com o mesmo nome sem reclamar, e este gancho
    // roda de novo a cada atualização. Sem o `delete` antes, a lista de regras
    // do Windows cresceria uma entrada por versão instalada, para sempre.
    let pos = gancho("NSIS_HOOK_POSTINSTALL");
    let apaga = pos
        .find("delete rule")
        .expect("o gancho de pós-instalação não apaga a regra antes de criá-la");
    let cria = pos.find("add rule").expect("já coberto pelo teste de cima");
    assert!(
        apaga < cria,
        "o `delete` da regra de firewall passou a vir depois do `add`.\n\
         Nessa ordem cada atualização deixa uma regra a mais na máquina de quem instala"
    );
}

#[test]
fn a_regra_de_firewall_sai_junto_com_o_programa() {
    // Desinstalar tem de devolver a máquina ao estado anterior. Uma regra de
    // firewall apontando para um `.exe` que não existe mais é lixo que só quem
    // sabe procurar encontra.
    assert!(
        gancho("NSIS_HOOK_PREUNINSTALL").contains("delete rule"),
        "o desinstalador deixou de remover a regra de firewall"
    );
}

#[test]
fn a_instalacao_e_da_maquina_porque_a_regra_de_firewall_depende_disso() {
    // Este teste guarda uma dependência que não se vê olhando para nenhum dos
    // dois arquivos sozinho.
    //
    // Criar regra de firewall exige administrador. A instalação da máquina
    // eleva; a por usuário não. Voltar `installMode` para `currentUser` não
    // quebraria build nenhum e não faria o gancho falhar de forma visível: o
    // `netsh` recusaria, o código de saída seria ignorado — como tem de ser,
    // porque isso não pode derrubar a instalação — e o instalador terminaria
    // dizendo que deu tudo certo, com o firewall fechado.
    //
    // O comentário no topo do `instalador.nsh` conta que a instalação virou da
    // máquina na 0.7.2 **por causa disto**, entre outras coisas. A regra só foi
    // criada muito depois; o motivo estava lá antes do efeito.
    let config = config();
    let modo = config["bundle"]["windows"]["nsis"]["installMode"].as_str();
    assert_eq!(
        modo,
        Some("perMachine"),
        "o `installMode` do NSIS saiu de `perMachine`.\n\
         Sem elevação o gancho de pós-instalação não cria a regra de firewall, \
         e ele falha em silêncio: a instalação termina dizendo que deu certo e \
         ninguém consegue entrar em quem hospedar."
    );
}

#[test]
fn o_rastro_do_windows_fica_ao_lado_das_preferencias() {
    // `arquivo_de_log` roda antes de existir `AppHandle`, então não pode
    // perguntar ao Tauri onde fica a pasta de configuração: ela é montada à mão
    // como `%APPDATA%\<identificador>`. Isso duplica o identificador, e cópia é
    // o que apodrece — renomear o pacote separaria o log das preferências sem
    // que nada quebrasse, e o sintoma seria alguém procurando um arquivo que
    // está noutro lugar.
    //
    // A pergunta que gerou isto veio de quem usa: «onde fica o seele.log no
    // Windows?». Antes deste caminho a resposta era «depende de onde o
    // executável foi iniciado», porque o Windows não define `HOME`,
    // `XDG_CONFIG_HOME` nem `SEELE_HOME` e a última opção era `"."`.
    let identificador = config()["identifier"]
        .as_str()
        .expect("o tauri.conf.json tem de declarar um `identifier`")
        .to_owned();
    let fonte = ler("src/main.rs");
    assert!(
        fonte.contains(&format!("join(\"{identificador}\")")),
        "o `arquivo_de_log` não monta o caminho do Windows com o identificador \
         do pacote (`{identificador}`).\n\
         Sem isso o rastro não fica ao lado das preferências, e quem for buscá-lo \
         não vai achar."
    );
}

#[test]
fn todo_crate_do_produto_pode_falar_no_log() {
    // O filtro padrão listava três crates de sete, e a falta era invisível: o
    // `tracing` não reclama de um alvo ausente, ele apenas não mostra. O custo
    // apareceu em campo — a linha que diz qual codificador de vídeo pegou mora
    // em `seele_video`, e eu passei uma sessão inteira pedindo ao dono um log
    // para responder uma pergunta que o log estava proibido de responder.
    //
    // A regra é «todo crate deste repositório», e não «os que o app liga»:
    // acertar a lista de ligados exigiria resolver o grafo de dependências num
    // teste, e uma diretiva a mais no filtro não custa nada — o `EnvFilter`
    // ignora em silêncio um alvo que ninguém emite. Errar para o lado de sobrar
    // é de graça; errar para o lado de faltar custou esta sessão.
    let fonte = ler("src/main.rs");
    let raiz = app().join("../../crates");
    let mut faltando = Vec::new();
    for entrada in std::fs::read_dir(&raiz).expect("li a pasta de crates") {
        let entrada = entrada.expect("li uma entrada");
        if !entrada.path().is_dir() {
            continue;
        }
        let nome = entrada.file_name().to_string_lossy().replace('-', "_");
        // Estes dois não entram num binário do produto: um é o cliente de
        // terminal e o outro é a bateria de conformidade.
        if nome == "seele_tui" || nome == "seele_conformance" {
            continue;
        }
        if !fonte.contains(&format!("{nome}=info")) {
            faltando.push(nome);
        }
    }
    faltando.sort();
    assert!(
        faltando.is_empty(),
        "estes crates não aparecem no filtro padrão do log e por isso não \
         conseguem falar: {faltando:?}.\n\
         Um alvo que falta não dá erro — ele só não aparece, e quem for depurar \
         vai procurar uma linha que nunca vai chegar."
    );
}

/// A versão do `tauri-bundler` de onde `instalador.nsi` foi bifurcado.
///
/// Escrita no cabeçalho daquele arquivo **e** aqui, de propósito: são as duas
/// pontas da mesma dívida, e um número que só existe num comentário é um número
/// que ninguém confere.
fn origem_do_modelo() -> String {
    let modelo = ler("instalador.nsi");
    modelo
        .lines()
        .find_map(|linha| {
            linha
                .trim_start_matches("; ")
                .strip_prefix("ORIGEM: tauri-bundler ")
        })
        .and_then(|resto| resto.split(',').next())
        .map(str::trim)
        .map(str::to_owned)
        .expect(
            "o `instalador.nsi` perdeu a linha `ORIGEM:` do cabeçalho — sem ela \
             ninguém sabe de que modelo ele saiu, nem quando rebifurcar",
        )
}

#[test]
fn o_modelo_bifurcado_guarda_o_que_faz_a_atualizacao_funcionar() {
    // **Este `.exe` tem dois usos, e o segundo não tem ninguém olhando.**
    //
    // Uma pessoa clica no instalador e vê as páginas. O atualizador do Tauri
    // baixa o mesmo arquivo e o roda com `/S`, sem tela nenhuma — e é assim que
    // toda atualização do SEELE é aplicada.
    //
    // `SkipIfPassive` é o que faz cada página sumir nesse modo. Uma página nova
    // que esqueça de passar por ele para a atualização de todo mundo, e o
    // sintoma chega como «o app não atualiza mais», sem nada na tela para
    // explicar — porque tela é justamente o que não há.
    let modelo = ler("instalador.nsi");

    assert!(
        modelo.contains("SkipIfPassive"),
        "o modelo bifurcado perdeu o `SkipIfPassive`.\n\
         Sem ele as páginas aparecem no modo silencioso, e o atualizador fica \
         parado esperando alguém apertar um botão numa janela que ninguém vê."
    );
    assert!(
        modelo.contains("{{#if installer_hooks}}") && modelo.contains("{{installer_hooks}}"),
        "o modelo bifurcado deixou de incluir os ganchos.\n\
         São eles que criam a regra de firewall da 8383 e removem a instalação \
         por usuário — ver `instalador.nsh`."
    );
    assert!(
        modelo.contains("{{product_name}}") && modelo.contains("{{version}}"),
        "o modelo bifurcado perdeu as chaves do Handlebars.\n\
         Elas não são do NSIS: apagar uma não dá erro de compilação nenhum, sai \
         um instalador com o campo vazio."
    );
}

#[test]
fn a_bifurcacao_do_modelo_ainda_corresponde_ao_tauri_instalado() {
    // A dívida que a bifurcação criou, com alguém conferindo.
    //
    // Um modelo bifurcado não recebe as correções de quem o escreveu, e uma
    // delas pode ser justamente a que conserta a instalação num Windows que
    // ninguém aqui tem. Quando o `tauri-bundler` subir de versão, isto reprova e
    // pede para comparar o modelo novo com o nosso.
    //
    // **Não reprova quando não há o que comparar.** O registro do cargo pode não
    // ter o `tauri-bundler` — é ferramenta de quem empacota, não dependência do
    // app. Aí a resposta honesta é dizer que não conferiu, e não inventar um
    // veredito.
    let origem = origem_do_modelo();

    let Some(registro) = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|casa| std::path::PathBuf::from(casa).join(".cargo"))
        })
        .map(|casa| casa.join("registry/src"))
        .filter(|caminho| caminho.is_dir())
    else {
        println!("PARCIAL: não achei o registro do cargo; não confiro a bifurcação daqui.");
        return;
    };

    let mut instaladas: Vec<String> = Vec::new();
    let Ok(indices) = std::fs::read_dir(&registro) else {
        println!(
            "PARCIAL: não li {}; não confiro a bifurcação daqui.",
            registro.display()
        );
        return;
    };
    for indice in indices.flatten() {
        let Ok(pacotes) = std::fs::read_dir(indice.path()) else {
            continue;
        };
        for pacote in pacotes.flatten() {
            let nome = pacote.file_name().to_string_lossy().into_owned();
            if let Some(versao) = nome.strip_prefix("tauri-bundler-") {
                instaladas.push(versao.to_owned());
            }
        }
    }

    if instaladas.is_empty() {
        println!(
            "PARCIAL: o `tauri-bundler` não está no registro desta máquina; \
             a bifurcação diz vir da {origem} e ninguém a contradiz aqui."
        );
        return;
    }

    assert!(
        instaladas.iter().any(|versao| versao == &origem),
        "o `instalador.nsi` foi bifurcado do tauri-bundler {origem}, e esta \
         máquina tem {instaladas:?}.\n\
         O modelo novo pode trazer correções que o nosso não tem. Rebifurque: \
         compare o `installer.nsi` da versão nova com o nosso, traga o que mudou \
         e atualize a linha `ORIGEM:` do cabeçalho."
    );
}

#[test]
fn nenhum_comentario_do_modelo_parece_handlebars() {
    // **O Handlebars lê o arquivo inteiro, comentário incluído.**
    //
    // O primeiro build desta bifurcação morreu num par de chaves duplas escrito
    // dentro de um comentário — justamente o comentário que explicava o que as
    // chaves duplas são. O erro que voltou foi de sintaxe de template apontando
    // para uma linha de prosa, que é o tipo de mensagem em que ninguém acredita
    // na primeira leitura.
    //
    // O guarda é grosseiro de propósito: toda chave dupla do arquivo tem de
    // parecer uma expressão de verdade. Prosa entre chaves não parece, e é essa
    // a única coisa que ele precisa pegar.
    let modelo = ler("instalador.nsi");

    let mut suspeitas = Vec::new();
    let mut resto = modelo.as_str();
    while let Some(inicio) = resto.find("{{") {
        let apos = &resto[inicio + 2..];
        let Some(fim) = apos.find("}}") else {
            break;
        };
        let dentro = apos[..fim].trim();
        // O que o modelo de verdade usa: `nome`, `#bloco`, `/bloco`, e as formas
        // de `#each … as |x| ~`. Nenhuma delas tem espaço solto no começo nem
        // fica vazia, que é como a prosa entra.
        let plausivel = !dentro.is_empty()
            && dentro
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '#' || c == '/' || c == '_');
        if !plausivel {
            suspeitas.push(format!("{{{{{dentro}}}}}"));
        }
        resto = &apos[fim + 2..];
    }

    assert!(
        suspeitas.is_empty(),
        "há chaves duplas no `instalador.nsi` que não parecem expressão: \
         {suspeitas:?}\n\
         O Handlebars do bundler lê o arquivo inteiro, comentário incluído — um \
         par de chaves na prosa derruba o empacotamento com um erro de sintaxe \
         apontando para uma frase."
    );
}
