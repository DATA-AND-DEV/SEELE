//! A entrada em «Aplicativos instalados», e o que ela precisa ter.
//!
//! # Por que isto é uma obrigação e não um enfeite
//!
//! É por esta chave que o Windows sabe que o SEELE existe. Sem ela o produto
//! está no disco e **não sai mais pelo painel do sistema** — a pessoa procura em
//! «Aplicativos instalados», não acha, e a única saída que sobra é apagar a
//! pasta à mão, deixando atalhos e chaves para trás.
//!
//! # Os valores que ninguém lembra
//!
//! `DisplayName` e `UninstallString` são os óbvios. Os outros não são enfeite:
//! sem `DisplayVersion` a lista não diz que versão está instalada; sem
//! `EstimatedSize` ela mostra um espaço em branco onde deveria estar o tamanho;
//! sem `NoModify`/`NoRepair` o painel oferece botões de «modificar» e «reparar»
//! que este instalador não sabe fazer — e um botão que não faz nada é pior que a
//! ausência dele.
//!
//! É a mesma lista que o modelo do NSIS escrevia, e está no contrato do ADR 0043
//! por isso.
#![cfg(windows)]
// Todo registro do Windows é FFI, e todo FFI é `unsafe`. A folha de lints desta
// crate põe `unsafe_code` em `deny` justamente para o `allow` poder existir aqui
// e em `janela.rs`, e em mais lugar nenhum — ver o `Cargo.toml`.
#![allow(unsafe_code)]

use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
};

/// Onde o Windows lista o que se pode desinstalar.
const LISTA: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\SEELE";

/// Onde o produto guarda onde foi instalado.
///
/// Lida pelo próprio instalador na próxima vez, para reencontrar uma instalação
/// anterior — é assim que uma atualização sabe por cima de que ela passa.
const CASA: &str = r"Software\DATA-AND-DEV\SEELE";

/// Texto para o registro: UTF-16 terminado em zero.
fn larga(texto: &str) -> Vec<u16> {
    texto.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Escreve um valor de texto.
fn escrever_texto(chave: HKEY, nome: &str, valor: &str) -> Result<(), String> {
    let nome_largo = larga(nome);
    let valor_largo = larga(valor);
    // Os bytes incluem o zero final: o Windows guarda o terminador dentro do
    // valor, e um tamanho sem ele devolve um texto truncado na leitura.
    let bytes = std::mem::size_of_val(valor_largo.as_slice());
    // SAFETY: os dois vetores vivem até o fim da chamada, e `bytes` é o tamanho
    // real de `valor_largo`.
    let estado = unsafe {
        RegSetValueExW(
            chave,
            PCWSTR(nome_largo.as_ptr()),
            None,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                valor_largo.as_ptr().cast::<u8>(),
                bytes,
            )),
        )
    };
    if estado == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("não gravei «{nome}»: erro {}", estado.0))
    }
}

/// Escreve um valor numérico.
fn escrever_numero(chave: HKEY, nome: &str, valor: u32) -> Result<(), String> {
    let nome_largo = larga(nome);
    let bytes = valor.to_le_bytes();
    // SAFETY: `nome_largo` e `bytes` vivem até o fim da chamada.
    let estado = unsafe {
        RegSetValueExW(
            chave,
            PCWSTR(nome_largo.as_ptr()),
            None,
            REG_DWORD,
            Some(&bytes),
        )
    };
    if estado == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("não gravei «{nome}»: erro {}", estado.0))
    }
}

/// Abre (ou cria) uma chave em `HKLM`.
fn abrir(caminho: &str) -> Result<HKEY, String> {
    let caminho_largo = larga(caminho);
    let mut chave = HKEY::default();
    // SAFETY: `caminho_largo` vive até o fim da chamada e `chave` é escrita pela
    // API.
    let estado = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(caminho_largo.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &raw mut chave,
            None,
        )
    };
    if estado == ERROR_SUCCESS {
        Ok(chave)
    } else {
        Err(format!(
            "não abri `HKLM\\{caminho}`: erro {}.\n\
             Escrever aqui exige elevação — o instalador tem de estar rodando \
             como administrador.",
            estado.0
        ))
    }
}

/// Fecha uma chave, ignorando a falha.
///
/// Falhar ao fechar é um recurso vazado num processo que está terminando, e não
/// há o que dizer a quem instala sobre isso.
fn fechar(chave: HKEY) {
    // SAFETY: `chave` veio de `RegCreateKeyExW` e não foi fechada antes.
    unsafe {
        let _ = RegCloseKey(chave);
    }
}

/// Anuncia a instalação ao Windows.
///
/// `tamanho_em_kib` vai para o `EstimatedSize`, que o painel mostra em MB. É o
/// tamanho **no disco**, medido depois de escrever — e não o da carga
/// comprimida, que é o número que o instalador tinha à mão e não é o que a
/// pessoa vê no painel.
///
/// # Errors
///
/// Devolve o primeiro valor que não gravou. Uma entrada pela metade é pior que
/// nenhuma: ela aparece na lista e não desinstala.
pub(crate) fn anunciar(
    pasta: &str,
    versao: &str,
    desinstalador: &str,
    tamanho_em_kib: u32,
) -> Result<(), String> {
    let chave = abrir(LISTA)?;
    let resultado = (|| {
        escrever_texto(chave, "DisplayName", "SEELE")?;
        escrever_texto(chave, "DisplayIcon", &format!("{pasta}\\SEELE.exe"))?;
        escrever_texto(chave, "DisplayVersion", versao)?;
        escrever_texto(chave, "Publisher", "DATA AND DEV")?;
        escrever_texto(chave, "InstallLocation", pasta)?;
        // **Com o `--desinstalar`, e é ele que faz a diferença.**
        //
        // `desinstalar.exe` é uma cópia do instalador — um binário, dois modos,
        // decididos pela linha de comando. Sem argumento nenhum, `linha::ler`
        // responde `Instalar`: o painel do Windows chamava o arquivo cru, e
        // «desinstalar» abria a janela de instalação. Relatado assim: «o
        // desinstalador não desinstala».
        //
        // O nome do arquivo enganava quem lia esta linha. O que faz dele um
        // desinstalador é o argumento, e está escrito no cabeçalho de
        // `desinstalar.rs` desde que ele nasceu.
        escrever_texto(
            chave,
            "UninstallString",
            &format!("\"{desinstalador}\" --desinstalar"),
        )?;
        escrever_texto(
            chave,
            "URLInfoAbout",
            "https://github.com/DATA-AND-DEV/SEELE",
        )?;
        // Sem «modificar» e sem «reparar»: este instalador não sabe fazer nem um
        // nem outro, e o painel oferece os dois se ninguém disser que não.
        escrever_numero(chave, "NoModify", 1)?;
        escrever_numero(chave, "NoRepair", 1)?;
        escrever_numero(chave, "EstimatedSize", tamanho_em_kib)
    })();
    fechar(chave);
    resultado?;

    let casa = abrir(CASA)?;
    let resultado = escrever_texto(casa, "InstallDir", pasta);
    fechar(casa);
    resultado
}

/// Apaga o que [`anunciar`] escreveu.
///
/// Chamado pelo desinstalador. Uma entrada que sobrevive à desinstalação é uma
/// linha em «Aplicativos instalados» que aponta para um programa que não existe
/// mais — e que, ao ser clicada, não faz nada.
pub(crate) fn esquecer() -> Result<(), String> {
    for caminho in [LISTA, CASA] {
        let largo = larga(caminho);
        // SAFETY: `largo` vive até o fim da chamada.
        let estado = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(largo.as_ptr())) };
        // A ausência não é falha: desinstalar duas vezes tem de dar certo as
        // duas, e uma chave que já não existe é o estado que se queria.
        if estado != ERROR_SUCCESS && estado != windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
            return Err(format!("não apaguei `HKLM\\{caminho}`: erro {}", estado.0));
        }
    }
    Ok(())
}

/// A entrada que o instalador do NSIS deixava no painel.
///
/// **Não é a nossa.** A nossa é `Uninstall\SEELE`; a dele é o identificador do
/// pacote, `tech.datadev.seele`. Enquanto as duas existem, «Programas e
/// Recursos» mostra dois SEELE e cada um remove metade — e quem clica no errado
/// fica com a outra metade e nenhuma explicação.
///
/// Ela só existe em máquina que passou pelo instalador do Tauri, que era por
/// onde as atualizações passavam antes de o manifesto apontar para o nosso.
const LISTA_DO_NSIS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\tech.datadev.seele";

/// Apaga a entrada do NSIS, se ela estiver lá.
///
/// Devolve `true` quando havia uma para apagar — é o que separa «removi» de
/// «não havia nada», e os dois merecem linhas diferentes no relato da
/// instalação.
///
/// # Errors
///
/// O que o Windows respondeu, quando a chave existe e não sai.
pub(crate) fn esquecer_a_do_nsis() -> Result<bool, String> {
    let largo = larga(LISTA_DO_NSIS);
    // SAFETY: `largo` vive até o fim da chamada.
    let estado = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(largo.as_ptr())) };
    if estado == ERROR_SUCCESS {
        return Ok(true);
    }
    if estado == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    Err(format!(
        "não apaguei `HKLM\\{LISTA_DO_NSIS}`: erro {}",
        estado.0
    ))
}

/// As escolhas guardadas, quando há.
///
/// `None` para quem instalou antes desta versão — e é o chamador que decide o
/// que fazer com a ausência. Ver [`crate::instalacao::Escolhas::de_antes`].
pub(crate) fn escolhas_guardadas() -> (Option<bool>, Option<bool>) {
    (
        ler_numero("AtalhoNaAreaDeTrabalho"),
        ler_numero("FirewallUDP"),
    )
}

/// Guarda o que se escolheu, para a atualização silenciosa reencontrar.
///
/// # Errors
///
/// Devolve o que impediu. Falhar aqui não desfaz a instalação, mas custa a
/// próxima atualização: sem esses valores ela cai no padrão, que pode não ser o
/// que esta pessoa escolheu.
pub(crate) fn guardar_escolhas(atalho: bool, porta: bool) -> Result<(), String> {
    let chave = abrir(CASA)?;
    let resultado = escrever_numero(chave, "AtalhoNaAreaDeTrabalho", u32::from(atalho))
        .and_then(|()| escrever_numero(chave, "FirewallUDP", u32::from(porta)));
    fechar(chave);
    resultado
}

/// Onde o SEELE já está instalado, se estiver.
pub(crate) fn onde_esta_instalado() -> Option<String> {
    ler_texto("InstallDir")
}

/// Lê um valor de texto de [`CASA`].
fn ler_texto(nome: &str) -> Option<String> {
    let caminho = larga(CASA);
    let nome_largo = larga(nome);
    let mut dados = [0_u16; 512];
    let mut bytes = u32::try_from(std::mem::size_of_val(&dados)).unwrap_or(0);
    // SAFETY: os dois nomes vivem até o fim da chamada, e `bytes` é o tamanho
    // real de `dados`.
    let estado = unsafe {
        windows::Win32::System::Registry::RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(caminho.as_ptr()),
            PCWSTR(nome_largo.as_ptr()),
            windows::Win32::System::Registry::RRF_RT_REG_SZ,
            None,
            Some(dados.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if estado != ERROR_SUCCESS {
        return None;
    }
    let unidades = (bytes as usize / 2).saturating_sub(1);
    dados.get(..unidades).map(String::from_utf16_lossy)
}

/// Lê um valor numérico de [`CASA`], como sim ou não.
fn ler_numero(nome: &str) -> Option<bool> {
    let caminho = larga(CASA);
    let nome_largo = larga(nome);
    let mut valor = 0_u32;
    let mut bytes = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
    // SAFETY: os dois nomes vivem até o fim da chamada, e `bytes` é o tamanho de
    // `valor`.
    let estado = unsafe {
        windows::Win32::System::Registry::RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(caminho.as_ptr()),
            PCWSTR(nome_largo.as_ptr()),
            windows::Win32::System::Registry::RRF_RT_REG_DWORD,
            None,
            Some((&raw mut valor).cast()),
            Some(&raw mut bytes),
        )
    };
    (estado == ERROR_SUCCESS).then_some(valor != 0)
}
