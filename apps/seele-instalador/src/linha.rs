//! A linha de comando, que é o contrato com quem chama.
//!
//! **Fora do `cfg(windows)` de propósito.** Isto é lógica pura, e é o caminho
//! por onde passa **toda** atualização do SEELE — um caminho sem tela, que
//! ninguém exercita à mão. Preso ao módulo do Windows, ele só seria testado na
//! máquina onde a bateria roda por último; aqui, os testes rodam nas duas.
//!
//! Fora do Windows quem consome isto é só o teste — o `main` que decide o que
//! fazer com um `Pedido` é `cfg(windows)`. É a mesma razão da `pele` e da
//! `carga`, e o `allow` existe para o guarda continuar rodando no Mac.
#![cfg_attr(not(windows), allow(dead_code))]

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
    ler(std::env::args().skip(1))
}

/// A leitura em si, separada do ambiente para poder ser exercitada.
///
/// **O ambiente fica de fora de propósito.** Esta função é o contrato com o
/// atualizador, e é por ele que passa toda atualização do SEELE — um caminho sem
/// tela, que ninguém exercita à mão. Presa a `std::env::args()`, ela só seria
/// testável rodando o instalador de verdade, com elevação, numa máquina Windows.
/// Separada, ela é uma função pura com um punhado de casos.
pub(crate) fn ler(argumentos: impl Iterator<Item = String>) -> Pedido {
    let argumentos: Vec<String> = argumentos.collect();

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

#[cfg(test)]
mod testes {
    use super::{ler, Pedido, Tela};

    /// Um pedido de instalação, decomposto para as asserções.
    fn instalar(argumentos: &[&str]) -> (Tela, bool, Vec<String>) {
        match ler(argumentos.iter().map(|a| (*a).to_owned())) {
            Pedido::Instalar {
                tela,
                reiniciar,
                argumentos_do_app,
            } => (tela, reiniciar, argumentos_do_app),
            _ => panic!("esperava um pedido de instalação"),
        }
    }

    #[test]
    fn sem_argumento_nenhum_a_janela_aparece() {
        let (tela, reiniciar, sobras) = instalar(&[]);
        assert!(tela == Tela::Cheia);
        assert!(!reiniciar);
        assert!(sobras.is_empty());
    }

    #[test]
    fn o_que_o_atualizador_passa_e_entendido() {
        // **O caso que importa.** `installMode: "passive"` no `tauri.conf.json`
        // faz o plugin do Tauri chamar o instalador com estes dois argumentos,
        // exatamente. Ler errado aqui é a atualização parar para todo mundo, sem
        // nada na tela — porque tela é o que não há neste caminho.
        let (tela, reiniciar, sobras) = instalar(&["/P", "/R"]);
        assert!(tela == Tela::Passiva, "o /P tem de calar as perguntas");
        assert!(reiniciar, "o /R tem de reabrir o produto");
        assert!(sobras.is_empty());
    }

    #[test]
    fn o_silencioso_do_nsis_tambem_vale() {
        // `installMode: "quiet"` manda `/S /R`. Não é o que este projeto usa
        // hoje, e é o que ele usaria se alguém trocasse uma linha do
        // `tauri.conf.json` — sem lembrar que o instalador precisa saber.
        let (tela, reiniciar, _) = instalar(&["/S", "/R"]);
        assert!(tela == Tela::Nenhuma);
        assert!(reiniciar);
    }

    #[test]
    fn a_caixa_da_letra_nao_importa() {
        // O NSIS aceitava os dois, e quem digita à mão escreve minúsculo.
        let (tela, reiniciar, _) = instalar(&["/s", "/r"]);
        assert!(tela == Tela::Nenhuma);
        assert!(reiniciar);
    }

    #[test]
    fn o_que_nao_e_nosso_volta_para_o_app() {
        // O atualizador repassa a linha de comando com que o SEELE estava
        // rodando, para devolvê-la ao reabrir. Engolir isso em silêncio perderia
        // o que a pessoa estava fazendo — um convite aberto, por exemplo.
        let (_, _, sobras) = instalar(&["/P", "seele://convite/abc", "/R", "--nick", "rafa"]);
        assert_eq!(
            sobras,
            vec!["seele://convite/abc", "--nick", "rafa"],
            "os argumentos do app têm de sobreviver na ordem em que vieram"
        );
    }

    #[test]
    fn desinstalar_nao_se_confunde_com_instalar() {
        assert!(matches!(
            ler(["--desinstalar".to_owned()].into_iter()),
            Pedido::RemoverDeDentro
        ));
        let de = ler(["--desinstalar-de".to_owned(), r"C:\SEELE".to_owned()].into_iter());
        match de {
            Pedido::RemoverA(pasta) => assert_eq!(pasta.display().to_string(), r"C:\SEELE"),
            _ => panic!("esperava a remoção de uma pasta nomeada"),
        }
    }
}
