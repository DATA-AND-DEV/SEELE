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
