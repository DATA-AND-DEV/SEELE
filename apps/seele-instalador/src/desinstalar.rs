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

/// Segundo tempo: a regra de firewall, a pasta, os dados e o registro.
///
/// Contando cada passo, porque é este o que a janela da remoção mostra — e uma
/// remoção que apaga em silêncio é indistinguível de uma que não fez nada.
///
/// A ordem é: firewall, pasta, dados, registro. A entrada do painel sai por
/// último de propósito — uma pasta que ficou com a entrada já removida é lixo
/// silencioso no disco, e uma entrada que ficou com a pasta removida é uma linha
/// no painel que não faz nada quando clicada. Das duas, a segunda é a que a
/// pessoa vê e tenta usar.
///
/// # Os dados são uma escolha, e a única que não se desfaz
///
/// Tudo o mais aqui é reversível reinstalando: a pasta volta, o atalho volta, a
/// regra de firewall volta. `%APPDATA%\tech.datadev.seele` não volta — lá moram
/// a identidade (uma chave Ed25519 gerada uma vez, ADR 0004, sem recuperação de
/// conta em spec nenhuma), os servidores conhecidos, as conversas e os anexos.
///
/// Quem apagar entra nos servidores de novo como alguém que nunca esteve lá, e o
/// apelido que era dela fica preso à chave que morreu. Por isso a caixa nasce
/// desmarcada e a nota dela diz o que se perde.
///
/// # Errors
///
/// O primeiro passo que não deu certo. A ordem é a de sempre — apagar a pasta,
/// depois esquecer o registro —, e o `contar` é chamado **depois** de cada coisa
/// dar certo: um relato que anuncia o que vai tentar mente quando a tentativa
/// falha.
pub(crate) fn remover_contando(
    pasta: &std::path::Path,
    apagar_dados: bool,
    contar: &dyn Fn(&str),
) -> Result<(), String> {
    // O primeiro tempo ainda pode estar terminando de morrer. Um instante é o
    // bastante, e é preferível a um laço que espera por um processo pelo id —
    // que é o tipo de coisa que trava para sempre quando o id é reusado.
    std::thread::sleep(std::time::Duration::from_millis(500));

    // **A regra de firewall antes da pasta.** Ela nomeia um programa daquele
    // caminho; apagar a pasta primeiro deixaria uma regra apontando para o nada,
    // que é o estado exato que a tela de hospedar aprendeu a denunciar.
    //
    // Falhar aqui não interrompe: uma regra que sobra é uma linha morta no
    // firewall, e parar a remoção por causa dela deixaria o resto pela metade.
    match crate::sistema::regra_de_firewall(&pasta.join("SEELE.exe"), false) {
        Ok(()) => contar("regra de firewall removida"),
        Err(erro) => contar(&format!("a regra de firewall ficou: {erro}")),
    }

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
        contar(&format!("{} apagada", pasta.display()));
    } else {
        contar("a pasta já não estava lá");
    }

    // **Os dados por último, e só se pedirem.** Depois da pasta porque o que
    // está aqui não some ao reinstalar: se alguma coisa falhar antes, é melhor
    // que ela falhe com os dados ainda de pé.
    if apagar_dados {
        match dados_desta_maquina() {
            Some(pasta) if pasta.is_dir() => match std::fs::remove_dir_all(&pasta) {
                Ok(()) => contar(&format!("{} apagada", pasta.display())),
                // Não interrompe: o produto já saiu, e o que resta é um diretório
                // que a pessoa pode apagar à mão — dito com o caminho, para que
                // ela consiga.
                Err(erro) => contar(&format!("não apaguei {}: {erro}", pasta.display())),
            },
            Some(pasta) => contar(&format!("{} já não estava lá", pasta.display())),
            None => contar("não achei a pasta de dados desta máquina"),
        }
    } else {
        contar("os seus dados ficaram nesta máquina");
    }

    registro::esquecer()?;
    contar("entrada removida do painel do Windows");
    Ok(())
}

/// Onde o SEELE guarda o que é desta pessoa nesta máquina.
///
/// `%APPDATA%\tech.datadev.seele`, que é onde o `config_dir` do app vai parar
/// pelo `app_config_dir` do Tauri. Montado à mão pela mesma razão que o log do
/// app o monta à mão: aqui não há `AppHandle` a quem perguntar.
///
/// `None` quando o Windows não define `APPDATA`, que não acontece numa sessão de
/// verdade — e não saber onde é não é motivo para apagar palpite nenhum.
fn dados_desta_maquina() -> Option<std::path::PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|base| std::path::PathBuf::from(base).join("tech.datadev.seele"))
}
