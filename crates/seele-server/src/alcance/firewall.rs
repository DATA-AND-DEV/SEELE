//! Se o firewall desta máquina deixa alguém de fora chegar até o Dogma.
//!
//! Só existe no Windows, e o motivo é assimetria real e não preguiça: no macOS
//! e no Linux o firewall padrão não barra entrada de um programa que já está
//! escutando, e no Windows barra — a regra nasce quando alguém clica «permitir»
//! num diálogo que aparece uma vez, na primeira execução, e que some se a pessoa
//! apertar Cancelar ou se a rede estiver marcada como pública.
//!
//! # O que este módulo se recusa a fazer
//!
//! **Não cria regra nenhuma.** Criar exige administrador, o instalador do SEELE
//! roda por usuário (`installMode: currentUser`) e um app de conversa que pede
//! elevação está pedindo uma coisa grande por uma coisa pequena.
//!
//! **E não adivinha.** [`Entrada::NaoSei`] é uma resposta de primeira classe, e
//! é a que sai sempre que a consulta não pôde ser feita ou não pôde ser lida.
//! Quem chama tem de calar nesse caso: uma tela que diz «o seu firewall está
//! barrando» sem saber é pior que uma tela que não diz nada, porque manda a
//! pessoa mexer em segurança para consertar um problema que talvez não exista.

use std::path::Path;

/// O que se sabe sobre entrada de fora, nesta máquina, para este programa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entrada {
    /// Há regra de entrada para este executável. Ninguém precisa fazer nada.
    Liberada,
    /// Não há regra, e no Windows isso significa que conexão de fora não chega.
    Barrada,
    /// Não deu para saber: a consulta falhou, não existe neste sistema, ou a
    /// saída dela veio numa forma que este código não entende.
    ///
    /// **Nunca vire uma frase.** Ver o cabeçalho do módulo.
    NaoSei,
}

/// Lê a saída de `netsh advfirewall firewall show rule` e diz se algum dos
/// programas liberados é este.
///
/// Separado da chamada ao sistema porque é a metade que dá para testar: o
/// `netsh` só existe no Windows, e quem escreveu isto não tem um. A metade que
/// não dá para testar aqui é a de cima, e ela está marcada como tal.
///
/// A comparação é pelo **nome do arquivo**, sem caminho e sem diferenciar
/// maiúsculas: o Windows escreve o caminho com a capitalização que estava no
/// disco no dia em que a regra nasceu, e uma pessoa que moveu a instalação
/// continua com a regra válida.
#[must_use]
pub fn ha_regra_para(saida: &str, executavel: &Path) -> Entrada {
    let Some(nosso) = executavel.to_str().map(nome_de_arquivo) else {
        return Entrada::NaoSei;
    };

    // Uma saída que não tem nenhuma linha de programa não é «nenhuma regra»: é
    // uma saída que este código não entendeu — locale diferente, versão do
    // `netsh` diferente, erro escrito na saída padrão. Dizer «barrada» aqui
    // seria inventar.
    let mut viu_alguma = false;
    for linha in saida.lines() {
        let Some((rotulo, valor)) = linha.split_once(':') else {
            continue;
        };
        // `Program` em inglês, `Programa` em português: o Windows traduz a
        // saída do `netsh`, e um SEELE que só entendesse inglês responderia
        // «não sei» em toda máquina brasileira — que são justamente as que este
        // projeto tem hoje.
        let rotulo = rotulo.trim();
        if !rotulo.eq_ignore_ascii_case("Program") && !rotulo.eq_ignore_ascii_case("Programa") {
            continue;
        }
        viu_alguma = true;
        if nome_de_arquivo(valor.trim()).eq_ignore_ascii_case(nosso) {
            return Entrada::Liberada;
        }
    }

    if viu_alguma {
        Entrada::Barrada
    } else {
        Entrada::NaoSei
    }
}

/// O último pedaço de um caminho **do Windows**, venha ele com barra invertida
/// ou com barra.
///
/// Escrito à mão em vez de `Path::file_name`, e o motivo é um defeito que os
/// testes pegaram: `Path` usa o separador do sistema em que o código **roda**, e
/// esta função lê caminhos do Windows de qualquer lugar — inclusive de um Mac,
/// que é onde ela é testada. Com `Path`, `D:\\SEELE\\seele.exe` no macOS vira um
/// nome de arquivo inteiro e nenhuma regra é reconhecida.
fn nome_de_arquivo(caminho: &str) -> &str {
    caminho
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(caminho)
        .trim_matches('"')
}

/// O comando que cria a regra, pronto para a pessoa colar.
///
/// `Direction=Inbound` e UDP, que é o que o Dogma precisa; e sem `Profile`, para
/// herdar o que o sistema achar certo — escolher perfil por ela seria decidir
/// sobre a segurança da máquina de outra pessoa a partir daqui.
#[must_use]
pub fn comando_para_liberar(executavel: &Path) -> String {
    format!(
        "New-NetFirewallRule -DisplayName \"SEELE\" -Direction Inbound \
         -Program \"{}\" -Protocol UDP -Action Allow",
        executavel.display()
    )
}

/// Pergunta ao sistema. **A metade que não é testável nesta máquina.**
#[cfg(windows)]
#[must_use]
pub fn entrada_para_este_programa() -> Entrada {
    let Ok(eu) = std::env::current_exe() else {
        return Entrada::NaoSei;
    };
    let saida = std::process::Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            "name=all",
            "dir=in",
            "verbose",
        ])
        .output();
    let Ok(saida) = saida else {
        return Entrada::NaoSei;
    };
    if !saida.status.success() {
        return Entrada::NaoSei;
    }
    // A saída do `netsh` vem na página de código do console, que não é UTF-8.
    // `from_utf8_lossy` basta porque só se procura um nome de arquivo ASCII: um
    // acento trocado por losango num caminho não muda o `file_name` que
    // interessa.
    ha_regra_para(&String::from_utf8_lossy(&saida.stdout), &eu)
}

/// Nos outros sistemas não há o que perguntar, e a resposta honesta é essa.
#[cfg(not(windows))]
#[must_use]
pub fn entrada_para_este_programa() -> Entrada {
    Entrada::NaoSei
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A forma que o `netsh` devolve, encurtada. Duas regras, nenhuma nossa.
    const DE_OUTROS: &str = "\
Rule Name:                            Google Chrome
----------------------------------------------------------------------
Enabled:                              Yes
Direction:                            In
Program:                              C:\\Program Files\\Google\\Chrome\\chrome.exe
Action:                               Allow

Rule Name:                            Spotify
----------------------------------------------------------------------
Enabled:                              Yes
Direction:                            In
Program:                              C:\\Users\\alex\\AppData\\Roaming\\Spotify\\Spotify.exe
Action:                               Allow
";

    #[test]
    fn sem_regra_para_este_programa_a_entrada_esta_barrada() {
        let saida = ha_regra_para(DE_OUTROS, Path::new("C:\\Program Files\\SEELE\\SEELE.exe"));
        assert_eq!(saida, Entrada::Barrada);
    }

    #[test]
    fn com_regra_para_este_programa_a_entrada_esta_liberada() {
        let com_a_nossa = format!(
            "{DE_OUTROS}\nRule Name:  SEELE\nProgram:    C:\\Program Files\\SEELE\\SEELE.exe\n"
        );
        let saida = ha_regra_para(
            &com_a_nossa,
            Path::new("C:\\Program Files\\SEELE\\SEELE.exe"),
        );
        assert_eq!(saida, Entrada::Liberada);
    }

    #[test]
    fn a_regra_vale_mesmo_com_o_programa_noutro_lugar() {
        // O Windows guarda o caminho de quando a regra nasceu. Quem moveu a
        // instalação, ou instalou por usuário e depois para a máquina toda,
        // continua com a regra valendo — e dizer «barrada» ali mandaria a pessoa
        // criar uma segunda regra idêntica.
        let com_a_nossa = format!("{DE_OUTROS}\nProgram: D:\\Programas\\SEELE\\seele.EXE\n");
        let saida = ha_regra_para(
            &com_a_nossa,
            Path::new("C:\\Program Files\\SEELE\\SEELE.exe"),
        );
        assert_eq!(saida, Entrada::Liberada);
    }

    #[test]
    fn a_saida_em_portugues_e_entendida() {
        // O Windows traduz o `netsh`. Um SEELE que só lesse inglês responderia
        // «não sei» em toda máquina brasileira — que são as que este projeto
        // tem hoje.
        let em_portugues = "\
Nome da Regra:                        SEELE
Habilitado:                           Sim
Programa:                             C:\\Arquivos de Programas\\SEELE\\SEELE.exe
";
        let saida = ha_regra_para(
            em_portugues,
            Path::new("C:\\Program Files\\SEELE\\SEELE.exe"),
        );
        assert_eq!(saida, Entrada::Liberada);
    }

    #[test]
    fn uma_saida_que_nao_se_entende_vira_nao_sei_e_nunca_barrada() {
        // A distinção que este módulo existe para fazer. Uma saída sem nenhuma
        // linha de programa pode ser «o firewall não tem regra nenhuma» ou «o
        // `netsh` respondeu noutra língua, noutra versão, ou com um erro» — e as
        // duas são indistinguíveis daqui. Chamar de «barrada» mandaria a pessoa
        // mexer no firewall por causa de um problema que talvez não exista.
        for nada in ["", "erro: acesso negado", "Ok.\n\n"] {
            assert_eq!(
                ha_regra_para(nada, Path::new("C:\\SEELE\\SEELE.exe")),
                Entrada::NaoSei,
                "«{nada}» não diz que não há regra; diz que não deu para saber"
            );
        }
    }

    #[test]
    fn o_comando_nomeia_o_executavel_que_precisa_da_regra() {
        // Um comando genérico faria a pessoa liberar o programa errado, ou
        // colar e não funcionar. O caminho é o desta instalação.
        let comando = comando_para_liberar(Path::new("C:\\Program Files\\SEELE\\SEELE.exe"));
        assert!(comando.contains("C:\\Program Files\\SEELE\\SEELE.exe"));
        assert!(comando.contains("-Direction Inbound"));
        assert!(comando.contains("UDP"));
    }
}
