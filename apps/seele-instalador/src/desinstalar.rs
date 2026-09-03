//! Remover o SEELE — o outro modo do mesmo executável.
//!
//! # Um binário, dois modos
//!
//! O que a instalação copia para dentro da pasta é **este mesmo programa**.
//! Rodando de fora ele instala; rodando com `--desinstalar` ele remove. Assim
//! não há um segundo executável para manter, assinar e manter em dia — e o
//! desinstalador nunca fica numa versão diferente do instalador que o escreveu.
//!
//! # O problema de apagar a própria casa
//!
//! `desinstalar.exe` mora dentro da pasta que precisa apagar, e o Windows não
//! deixa apagar um executável que está rodando. É por isso que a remoção tem
//! dois tempos:
//!
//! 1. `--desinstalar`, rodando de dentro: copia-se para o temporário e chama a
//!    cópia com `--desinstalar-de <pasta>`. Depois sai na hora, para soltar o
//!    arquivo.
//! 2. `--desinstalar-de`, rodando do temporário: espera o primeiro morrer,
//!    apaga a pasta e esquece o registro.
//!
//! A cópia no temporário fica para trás, e é o Windows quem a limpa. Um
//! programa que tentasse apagar a si mesmo teria o mesmo problema um nível
//! adiante, para sempre.
#![cfg(windows)]

use crate::registro;

/// Quanta tela a instalação mostra.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tela {
    /// A janela inteira, com as quatro páginas. É o que uma pessoa vê.
    Cheia,
    /// Só o andamento, sem perguntar nada. `/P` — é o que o atualizador do
    /// SEELE usa, e está em `tauri.conf.json` como `installMode: "passive"`.
    Passiva,
    /// Nenhuma. `/S`.
    Nenhuma,
}

/// O que a linha de comando pediu.
pub(crate) enum Pedido {
    /// Instalar.
    Instalar {
        /// Quanta tela mostrar.
        tela: Tela,
        /// Abrir o produto no fim. `/R`.
        reiniciar: bool,
        /// O que o app estava rodando com, para devolver a ele ao reabrir.
        argumentos_do_app: Vec<String>,
    },
    /// Remover, rodando de dentro da pasta instalada.
    RemoverDeDentro,
    /// Remover a pasta nomeada, rodando de fora dela.
    RemoverA(std::path::PathBuf),
}

/// Lê a linha de comando.
///
/// # O contrato que não é nosso
///
/// `/P`, `/S` e `/R` são os argumentos que o **atualizador do Tauri** passa a um
/// instalador NSIS — `["/P", "/R"]` no modo passivo, que é o configurado em
/// `tauri.conf.json`. Este instalador substitui o NSIS e por isso herda a
/// linha de comando dele: quem chama é o SEELE já instalado, e ele não vai mudar
/// de opinião porque trocamos de instalador.
///
/// Os argumentos que sobram são os do próprio app, que o atualizador repassa
/// para devolvê-los na hora de reabrir. Eles não são para nós, e ignorá-los
/// silenciosamente seria perder o que a pessoa estava fazendo.
pub(crate) fn ler_pedido() -> Pedido {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();

    if argumentos.first().is_some_and(|a| a == "--desinstalar") {
        return Pedido::RemoverDeDentro;
    }
    if argumentos.first().is_some_and(|a| a == "--desinstalar-de") {
        return argumentos.get(1).map_or(
            Pedido::Instalar {
                tela: Tela::Cheia,
                reiniciar: false,
                argumentos_do_app: Vec::new(),
            },
            |pasta| Pedido::RemoverA(pasta.into()),
        );
    }

    let mut tela = Tela::Cheia;
    let mut reiniciar = false;
    let mut do_app = Vec::new();
    for argumento in argumentos {
        // Sem diferenciar maiúscula: quem escreve `/s` à mão espera o mesmo que
        // `/S`, e o NSIS aceitava os dois.
        match argumento.to_ascii_uppercase().as_str() {
            "/S" => tela = Tela::Nenhuma,
            "/P" => tela = Tela::Passiva,
            "/R" => reiniciar = true,
            _ => do_app.push(argumento),
        }
    }

    Pedido::Instalar {
        tela,
        reiniciar,
        argumentos_do_app: do_app,
    }
}

/// Primeiro tempo: sair de dentro da pasta antes de apagá-la.
///
/// # Errors
///
/// Devolve o que impediu a cópia ou o lançamento. Aqui nada foi apagado ainda,
/// então a falha é inteira e não deixa meia remoção.
pub(crate) fn sair_de_dentro() -> Result<(), String> {
    let eu = std::env::current_exe()
        .map_err(|erro| format!("não sei onde este programa está: {erro}"))?;
    let pasta = eu
        .parent()
        .ok_or_else(|| "o desinstalador não está dentro de pasta nenhuma".to_owned())?
        .to_path_buf();

    let copia = std::env::temp_dir().join("seele-desinstalar.exe");
    std::fs::copy(&eu, &copia)
        .map_err(|erro| format!("não me copiei para {}: {erro}", copia.display()))?;

    std::process::Command::new(&copia)
        .arg("--desinstalar-de")
        .arg(&pasta)
        .spawn()
        .map_err(|erro| format!("não chamei a cópia em {}: {erro}", copia.display()))?;
    Ok(())
}

/// Segundo tempo: apagar a pasta e esquecer o registro.
///
/// # Errors
///
/// Devolve o que sobrou. A ordem é apagar primeiro e esquecer depois: uma pasta
/// que ficou com a entrada já removida é lixo silencioso no disco, e uma entrada
/// que ficou com a pasta removida é uma linha no painel que não faz nada quando
/// clicada — das duas, a segunda é a que a pessoa vê e tenta usar.
pub(crate) fn remover(pasta: &std::path::Path) -> Result<(), String> {
    // O primeiro tempo ainda pode estar terminando de morrer. Um instante é o
    // bastante, e é preferível a um laço que espera por um processo pelo id —
    // que é o tipo de coisa que trava para sempre quando o id é reusado.
    std::thread::sleep(std::time::Duration::from_millis(500));

    // A pasta pode já não existir: desinstalar duas vezes tem de dar certo as
    // duas vezes.
    if pasta.is_dir() {
        std::fs::remove_dir_all(pasta).map_err(|erro| {
            format!(
                "não apaguei {}: {erro}\n\
                 Se o SEELE estiver aberto, feche-o e remova de novo pelo painel \
                 do Windows.",
                pasta.display()
            )
        })?;
    }
    registro::esquecer()
}
