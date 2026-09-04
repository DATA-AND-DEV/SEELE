//! O que o instalador pede ao Windows além de escrever arquivos.
//!
//! Firewall, atalhos, e as conferências que têm de acontecer **antes** de
//! qualquer coisa ser escrita. Todas são obrigações do contrato do ADR 0043.
#![cfg(windows)]
// FFI e COM, os dois `unsafe` por natureza. Ver a folha de lints no `Cargo.toml`.
#![allow(unsafe_code)]

use std::os::windows::process::CommandExt as _;
use std::path::Path;

use windows::core::{Interface as _, PCWSTR};

/// Roda um programa sem piscar um console preto na tela.
///
/// Sem `CREATE_NO_WINDOW` cada chamada ao `netsh` abre e fecha uma janela de
/// console por cima do instalador. Não quebra nada e parece exatamente com o que
/// um programa mal-intencionado faz — numa tela em que a pessoa está decidindo
/// se confia no que acabou de baixar.
const SEM_CONSOLE: u32 = 0x0800_0000;

/// Texto para o Win32: UTF-16 terminado em zero.
fn larga(texto: &str) -> Vec<u16> {
    texto.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Põe ou tira a regra de entrada da porta 8383.
///
/// **Do programa, e não da porta solta.** A regra vale para o executável do
/// SEELE e para mais nada: uma regra por número de porta abriria a 8383 para
/// qualquer coisa que a escutasse depois, inclusive o que for instalado amanhã.
///
/// A regra antiga sai sempre, marcada ou não. Sem isso, desmarcar a caixa numa
/// reinstalação deixaria de pé a regra da instalação anterior — e a caixa teria
/// mentido.
///
/// # `profile=any`, e ela já foi estreitada e voltou
///
/// **04/09/2026, ida.** Passou a ser `domain,private`, deixando de fora o perfil
/// público — o que o Windows escolhe para uma rede em que não se confia, e o que
/// ele dá ao Wi-Fi de uma cafeteria. O argumento era bom e continua sendo:
/// hospedar de uma cafeteria é caso raro, carregar o notebook para uma é o caso
/// comum, e as três paredes que respondem depois (o balde por endereço, o
/// segredo, a portaria) são paredes e não a ausência de contato.
///
/// **04/09/2026, volta.** Quem hospeda pediu de volta, depois de o Windows
/// continuar recusando conexão. Fica registrado que **a evidência aponta para
/// outro lugar**: numa máquina examinada, a regra nomeava
/// `C:\Program Files\SEELE\SEELE.exe` e essa pasta não existia — e uma regra
/// presa a um programa ausente não permite nada, em perfil nenhum. O perfil não
/// era a parede.
///
/// A volta é decisão de quem opera, e o custo dela está escrito aqui para quando
/// alguém reabrir esta função: com `any`, uma máquina que hospedou uma vez
/// continua aceitando UDP não solicitado no Wi-Fi de qualquer lugar onde ela for
/// aberta. Estreitar de novo é seguro **depois** de a causa de verdade estar
/// resolvida — e o aviso que o app agora dá, quando a regra não cobre o
/// executável que está rodando, é o que faltava para essa conversa acontecer
/// com dado em vez de com tentativa.
///
/// # Errors
///
/// Devolve o que o `netsh` respondeu quando ele recusa. Falhar aqui não desfaz a
/// instalação: o produto funciona, só não recebe conexão de fora até alguém
/// abrir a porta à mão.
pub(crate) fn regra_de_firewall(executavel: &Path, ligada: bool) -> Result<(), String> {
    let netsh = std::path::Path::new(&std::env::var("SystemRoot").unwrap_or_default())
        .join("System32")
        .join("netsh.exe");

    let apagar = std::process::Command::new(&netsh)
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            "name=SEELE",
            "dir=in",
        ])
        .creation_flags(SEM_CONSOLE)
        .output();
    // A ausência da regra não é falha: instalar pela primeira vez é o caso comum,
    // e não há o que apagar.
    drop(apagar);

    if !ligada {
        return Ok(());
    }

    let saida = std::process::Command::new(&netsh)
        .args([
            "advfirewall",
            "firewall",
            "add",
            "rule",
            "name=SEELE",
            "dir=in",
            "action=allow",
            "protocol=udp",
            // Ver o cabeçalho: isto já foi `domain,private` e voltou, a pedido
            // de quem hospeda.
            "profile=any",
            "enable=yes",
            &format!("program={}", executavel.display()),
            "description=Deixa entrar conexão para o servidor SEELE hospedado nesta máquina.",
        ])
        .creation_flags(SEM_CONSOLE)
        .output()
        .map_err(|erro| format!("não chamei o netsh: {erro}"))?;

    if saida.status.success() {
        Ok(())
    } else {
        Err(format!(
            "o Windows recusou criar a regra de firewall: {}",
            String::from_utf8_lossy(&saida.stdout).trim()
        ))
    }
}

/// Escreve um atalho `.lnk`.
///
/// # Errors
///
/// Devolve o que o COM respondeu. Um atalho que não sai não impede o produto de
/// funcionar — mas impede alguém de achá-lo, que é quase a mesma coisa para quem
/// acabou de instalar.
pub(crate) fn atalho(onde: &Path, para: &Path, descricao: &str) -> Result<(), String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    if let Some(pasta) = onde.parent() {
        std::fs::create_dir_all(pasta)
            .map_err(|erro| format!("não criei {}: {erro}", pasta.display()))?;
    }

    // SAFETY: o COM é iniciado e encerrado neste par, e as interfaces morrem
    // antes do `CoUninitialize` — elas saem de escopo no fim do bloco interno.
    unsafe {
        // O apartamento pode já estar iniciado por outra chamada; o COM diz isso
        // com um `S_FALSE` que não é erro, e por isso o resultado é descartado.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let resultado = (|| -> Result<(), String> {
            let ligacao: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|erro| format!("não criei o objeto de atalho: {erro}"))?;

            let alvo = larga(&para.display().to_string());
            ligacao
                .SetPath(PCWSTR(alvo.as_ptr()))
                .map_err(|erro| format!("não apontei o atalho: {erro}"))?;

            // O ícone sai do próprio executável, e não de um `.ico` ao lado: um
            // arquivo de ícone ao lado é mais um arquivo para alguém apagar sem
            // saber o que era, e o atalho ficaria em branco.
            ligacao
                .SetIconLocation(PCWSTR(alvo.as_ptr()), 0)
                .map_err(|erro| format!("não pus o ícone no atalho: {erro}"))?;

            if let Some(pasta) = para.parent() {
                let trabalho = larga(&pasta.display().to_string());
                ligacao
                    .SetWorkingDirectory(PCWSTR(trabalho.as_ptr()))
                    .map_err(|erro| format!("não pus a pasta de trabalho: {erro}"))?;
            }

            let texto = larga(descricao);
            ligacao
                .SetDescription(PCWSTR(texto.as_ptr()))
                .map_err(|erro| format!("não descrevi o atalho: {erro}"))?;

            let arquivo: IPersistFile = ligacao
                .cast()
                .map_err(|erro| format!("o atalho não sabe se gravar: {erro}"))?;
            let destino = larga(&onde.display().to_string());
            arquivo
                .Save(PCWSTR(destino.as_ptr()), true)
                .map_err(|erro| format!("não gravei {}: {erro}", onde.display()))
        })();

        CoUninitialize();
        resultado
    }
}

/// Onde ficam os atalhos do menu Iniciar, para todo mundo desta máquina.
pub(crate) fn menu_iniciar() -> Option<std::path::PathBuf> {
    // `ProgramData` e não `AppData`: a instalação é da máquina, e um atalho no
    // menu de um usuário só some para os outros.
    std::env::var("ProgramData").ok().map(|base| {
        Path::new(&base)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
    })
}

/// A área de trabalho de todo mundo desta máquina.
pub(crate) fn area_de_trabalho() -> Option<std::path::PathBuf> {
    std::env::var("PUBLIC")
        .ok()
        .map(|base| Path::new(&base).join("Desktop"))
}

/// O SEELE está aberto agora?
///
/// **Conferido antes de escrever qualquer arquivo.** O Windows não deixa
/// sobrescrever um executável em uso, e a instalação morreria no meio — com
/// parte dos arquivos novos e parte velhos, que é o estado mais difícil de
/// explicar e o mais fácil de evitar.
pub(crate) fn produto_aberto() -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: o retrato é fechado pelo `Owned` no fim do escopo, e `entrada` tem
    // o `dwSize` que a API exige antes da primeira chamada.
    unsafe {
        let Ok(retrato) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            // Sem retrato não dá para afirmar que está fechado. Dizer «aberto»
            // aqui pararia a instalação por uma dúvida; dizer «fechado» arrisca
            // um arquivo em uso, que é o erro que o Windows explica sozinho.
            return false;
        };
        let mut entrada = PROCESSENTRY32W {
            dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0),
            ..Default::default()
        };
        let mut achou = false;
        if Process32FirstW(retrato, &raw mut entrada).is_ok() {
            loop {
                let fim = entrada
                    .szExeFile
                    .iter()
                    .position(|u| *u == 0)
                    .unwrap_or(entrada.szExeFile.len());
                let nome = entrada
                    .szExeFile
                    .get(..fim)
                    .map(String::from_utf16_lossy)
                    .unwrap_or_default();
                if nome.eq_ignore_ascii_case("SEELE.exe") {
                    achou = true;
                    break;
                }
                if Process32NextW(retrato, &raw mut entrada).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(retrato);
        achou
    }
}

/// A instalação por usuário da 0.7.1, se ela ficou para trás.
///
/// Até a 0.7.1 o SEELE se instalava em `%LOCALAPPDATA%\SEELE`. A 0.7.2 passou a
/// instalar para a máquina, e o instalador novo **não enxerga o antigo**: ele
/// procura no registro da máquina, e a antiga mora no do usuário. O resultado
/// relatado na época foi «o aplicativo volta de versão» — o atalho velho continua
/// abrindo a cópia velha, parada onde foi deixada.
pub(crate) fn instalacao_por_usuario() -> Option<std::path::PathBuf> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    let pasta = Path::new(&base).join("SEELE");
    pasta.join("SEELE.exe").is_file().then_some(pasta)
}

/// O runtime do WebView2 está nesta máquina?
///
/// **Sem ele o SEELE não abre.** O app é uma casca do Tauri sobre o WebView2;
/// numa máquina limpa — Windows 10 sem atualizações recentes — ele não vem, e o
/// produto instalado abre uma janela em branco que não explica nada. É por isso
/// que o instalador do NSIS tinha uma seção inteira para ele, e por isso este
/// aqui não pode ser uma janela WebView2.
///
/// A chave é a do Edge Update, e o valor é a versão. `WOW6432Node` porque a
/// entrada é gravada pelo instalador de 32 bits do runtime mesmo num Windows de
/// 64 — procurar só fora dela responde «não tem» numa máquina que tem.
pub(crate) fn webview2_instalado() -> bool {
    const CLIENTE: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let lugares = [
        (
            windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{CLIENTE}"),
        ),
        (
            windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{CLIENTE}"),
        ),
        (
            windows::Win32::System::Registry::HKEY_CURRENT_USER,
            format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{CLIENTE}"),
        ),
    ];

    lugares.iter().any(|(raiz, caminho)| {
        let caminho_largo = larga(caminho);
        let nome = larga("pv");
        let mut dados = [0_u16; 64];
        let mut bytes = u32::try_from(std::mem::size_of_val(&dados)).unwrap_or(0);
        // SAFETY: os dois nomes vivem até o fim da iteração, e `bytes` é o
        // tamanho real de `dados`.
        let estado = unsafe {
            windows::Win32::System::Registry::RegGetValueW(
                *raiz,
                PCWSTR(caminho_largo.as_ptr()),
                PCWSTR(nome.as_ptr()),
                windows::Win32::System::Registry::RRF_RT_REG_SZ,
                None,
                Some(dados.as_mut_ptr().cast()),
                Some(&raw mut bytes),
            )
        };
        // Uma versão vazia conta como ausente: a chave sobrevive a uma remoção
        // do runtime, e o que ela guarda nesse caso é uma cadeia de zero
        // caractere.
        estado == windows::Win32::Foundation::ERROR_SUCCESS && bytes > 2
    })
}

/// Baixa e roda o instalador do WebView2 da Microsoft.
///
/// **O endereço é o oficial e permanente** — `go.microsoft.com/fwlink/p/?LinkId=2124703`
/// é o mesmo que o modelo do NSIS do Tauri usa. Ele baixa o *bootstrapper*, de
/// 1,7 MB, que por sua vez busca o runtime; é por isso que este passo precisa de
/// rede, e é por isso que a falha aqui diz isso por extenso.
///
/// `URLDownloadToFileW` do `urlmon`, e não um cliente HTTP: uma dependência a
/// mais neste binário é uma dependência a auditar para instalar um programa que
/// se vende por não depender de ninguém.
///
/// # Errors
///
/// Devolve o que impediu. Falhar aqui **não** derruba a instalação: o SEELE fica
/// instalado, e quem abrir vê uma janela em branco até o runtime existir. Por
/// isso a mensagem tem de ser guardada e mostrada, e não engolida.
pub(crate) fn instalar_webview2() -> Result<(), String> {
    use windows::Win32::System::Com::Urlmon::URLDownloadToFileW;

    let destino = std::env::temp_dir().join("MicrosoftEdgeWebview2Setup.exe");
    let endereco = larga("https://go.microsoft.com/fwlink/p/?LinkId=2124703");
    let arquivo = larga(&destino.display().to_string());

    // SAFETY: as duas cadeias vivem até o fim da chamada; os dois ponteiros de
    // COM que a API aceita são nulos, que é o uso documentado sem callback.
    unsafe {
        URLDownloadToFileW(
            None,
            PCWSTR(endereco.as_ptr()),
            PCWSTR(arquivo.as_ptr()),
            0,
            None,
        )
    }
    .map_err(|erro| {
        format!(
            "não baixei o runtime do WebView2: {erro}\n\
             Este passo precisa de rede. Sem o runtime o SEELE instala e abre \
             uma janela em branco — instale-o depois pelo site da Microsoft."
        )
    })?;

    let saida = std::process::Command::new(&destino)
        .args(["/silent", "/install"])
        .creation_flags(SEM_CONSOLE)
        .status()
        .map_err(|erro| format!("não rodei o instalador do WebView2: {erro}"))?;

    if saida.success() {
        Ok(())
    } else {
        Err(format!(
            "o instalador do WebView2 saiu com {}. O SEELE está instalado, mas \
             pode abrir uma janela em branco até o runtime existir.",
            saida.code().unwrap_or(-1)
        ))
    }
}

/// Este Windows é novo o bastante para o SEELE?
///
/// **Windows 10 é o piso, e quem o define não somos nós:** é o WebView2, que o
/// produto precisa para abrir. Instalar num Windows 8 escreveria os arquivos e
/// deixaria alguém com um programa que nunca abre.
///
/// A pergunta é feita ao registro, e não a `GetVersionEx`, que mente: sem
/// manifesto ele responde «Windows 8» para sempre, e com manifesto responde a
/// verdade — mas depender do manifesto para uma resposta correta é depender de um
/// arquivo que alguém pode editar sem entender o que quebra.
///
/// `CurrentMajorVersionNumber` **só existe a partir do Windows 10**. A ausência
/// dele é a resposta, e é uma resposta que não depende de interpretar número
/// nenhum.
pub(crate) fn windows_novo_o_bastante() -> bool {
    let caminho = larga(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let nome = larga("CurrentMajorVersionNumber");
    let mut maior = 0_u32;
    let mut bytes = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
    // SAFETY: as duas cadeias vivem até o fim da chamada, e `bytes` é o tamanho
    // de `maior`.
    let estado = unsafe {
        windows::Win32::System::Registry::RegGetValueW(
            windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            PCWSTR(caminho.as_ptr()),
            PCWSTR(nome.as_ptr()),
            windows::Win32::System::Registry::RRF_RT_REG_DWORD,
            None,
            Some((&raw mut maior).cast()),
            Some(&raw mut bytes),
        )
    };
    estado == windows::Win32::Foundation::ERROR_SUCCESS && maior >= 10
}
