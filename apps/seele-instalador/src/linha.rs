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
    ler_com_nome(nome_do_executavel().as_deref(), std::env::args().skip(1))
}

/// Como este executável se chama, em minúsculas e sem o caminho.
fn nome_do_executavel() -> Option<String> {
    std::env::current_exe()
        .ok()?
        .file_name()?
        .to_str()
        .map(str::to_lowercase)
}

/// A leitura em si, separada do ambiente para poder ser exercitada.
///
/// **O ambiente fica de fora de propósito.** Esta função é o contrato com o
/// atualizador, e é por ele que passa toda atualização do SEELE — um caminho sem
/// tela, que ninguém exercita à mão. Presa a `std::env::args()`, ela só seria
/// testável rodando o instalador de verdade, com elevação, numa máquina Windows.
/// Separada, ela é uma função pura com um punhado de casos.
#[cfg(test)]
pub(crate) fn ler(argumentos: impl Iterator<Item = String>) -> Pedido {
    ler_com_nome(None, argumentos)
}

/// O nome do arquivo entra na decisão, e não só a linha de comando.
///
/// # Por que o nome, se já há um argumento
///
/// Porque o argumento vem do `UninstallString`, e o `UninstallString` foi
/// **gravado no passado**. Quem instalou com uma versão que o escrevia sem
/// `--desinstalar` tem, no registro, uma linha que manda rodar o arquivo cru — e
/// rodá-lo cru abria a janela de **instalar**.
///
/// Consertar o que se grava conserta a próxima instalação e não a de ninguém que
/// já instalou; e o relato veio duas vezes, a segunda com «isso já está ficando
/// irritante». Está certo: um conserto que depende de um valor guardado por uma
/// versão anterior não é um conserto, é um pedido de reinstalação.
///
/// O nome do arquivo é a única coisa que este processo sabe sobre si mesmo sem
/// perguntar a ninguém. A instalação copia o binário como `desinstalar.exe`, e a
/// partir daí ele **é** o desinstalador — com argumento ou sem.
///
/// # O que isto não confunde
///
/// O instalador baixado chama-se `SEELE_<versão>_x64-instalador.exe`, e o que o
/// atualizador roda é esse mesmo arquivo. Nenhum dos dois se chama
/// `desinstalar.exe`; só a cópia que a instalação escreve dentro da pasta.
fn ler_com_nome(nome: Option<&str>, argumentos: impl Iterator<Item = String>) -> Pedido {
    let argumentos: Vec<String> = argumentos.collect();

    // O segundo tempo tem argumento e caminho, e ele vence o nome: os dois
    // arquivos se chamam igual — o de dentro da pasta e a cópia no temporário —
    // e é o argumento que diz qual deles está rodando.
    if !argumentos.first().is_some_and(|a| a == "--desinstalar-de")
        && nome == Some("desinstalar.exe")
    {
        return Pedido::RemoverDeDentro;
    }

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
            // **`/UPDATE` e `/ARGS`, que o atualizador do Tauri sempre manda.**
            //
            // A linha inteira que ele monta é `/P /R /UPDATE /ARGS <args do
            // app>` — está em `updater.rs` do plugin, no braço `Nsis`. Estes
            // dois não estavam aqui, então caíam no `_` e viravam «argumentos do
            // app»; e `abrir_o_produto` os entregava ao `explorer.exe`, que não
            // encaminha argumento nenhum e não abria coisa nenhuma.
            //
            // Relatado assim: «ele não reabre o app após atualizar». O botão
            // ABRIR O SEELE da janela sempre funcionou, e a diferença entre os
            // dois caminhos era exatamente esta lista: vazia num, com `/UPDATE
            // /ARGS` no outro.
            //
            // `/UPDATE` é marca e não pedido: ele diz «isto é uma atualização»,
            // que aqui já se sabe por não haver tela. Reconhecido para não virar
            // lixo, e ignorado por não ter o que fazer.
            "/UPDATE" => {}
            // `/ARGS` é separador: o que vem **depois** dele é do app, e o que
            // vem antes é do instalador. Sem ele os dois se misturavam.
            "/ARGS" => do_app.clear(),
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
    #[test]
    fn a_linha_inteira_que_o_atualizador_manda() {
        // **É esta, e não a que era cômoda de imaginar.** O plugin monta
        // `/P /R /UPDATE /ARGS <args do app>` — está em `updater.rs`, no braço
        // `Nsis`. Os testes vizinhos exercitavam `/P /R` e paravam ali, que é
        // por onde o defeito passou: `/UPDATE` e `/ARGS` caíam no ramo de sobra,
        // viravam «argumentos do app», e iam parar na linha do `explorer.exe`,
        // que não encaminha argumento nenhum e não abria o produto.
        //
        // Relatado assim: «ele não reabre o app após atualizar».
        let (tela, reiniciar, sobras) = instalar(&["/P", "/R", "/UPDATE", "/ARGS"]);
        assert!(
            tela == Tela::Passiva,
            "o /P continua pedindo a tela passiva"
        );
        assert!(reiniciar, "o /R continua sendo o pedido de reabrir");
        assert!(
            sobras.is_empty(),
            "as marcas do atualizador viraram argumentos do app: {sobras:?}"
        );
    }

    #[test]
    fn o_que_vem_depois_de_args_e_do_app_e_o_que_vem_antes_nao() {
        // `/ARGS` é separador. Sem ele, o que o instalador não reconhecesse se
        // misturava com o que é do app, e os dois iam para o mesmo lugar.
        let (_, _, sobras) = instalar(&["/P", "/R", "/UPDATE", "/ARGS", "--sala", "7"]);
        assert_eq!(
            sobras,
            vec!["--sala".to_owned(), "7".to_owned()],
            "o que vem depois de /ARGS é do app, e nada além disso"
        );
    }
    /// **Chamar-se `desinstalar.exe` já é o pedido.**
    ///
    /// O modo vinha só da linha de comando, e a linha de comando vem do
    /// `UninstallString` — que foi **gravado no passado**. Uma máquina que
    /// instalou com a versão que o escrevia sem argumento tem, no registro, uma
    /// linha que manda rodar o arquivo cru; e rodá-lo cru abria a janela de
    /// instalar.
    ///
    /// Consertar o que se grava conserta a próxima instalação e não a de quem já
    /// instalou. O relato veio duas vezes, a segunda com «isso já está ficando
    /// irritante» — e estava certo: um conserto que depende de um valor guardado
    /// por uma versão anterior não é conserto, é pedido de reinstalação.
    #[test]
    fn o_arquivo_chamado_desinstalar_desinstala_mesmo_sem_argumento() {
        let sem_nada: Vec<String> = Vec::new();
        assert!(
            matches!(
                super::ler_com_nome(Some("desinstalar.exe"), sem_nada.into_iter()),
                Pedido::RemoverDeDentro
            ),
            "o `desinstalar.exe` rodado sem argumento voltou a abrir o instalador"
        );

        // Maiúscula não muda nada: o Windows não distingue, e o `UninstallString`
        // de outra versão pode ter escrito de outro jeito.
        let sem_nada: Vec<String> = Vec::new();
        assert!(matches!(
            super::ler_com_nome(
                Some("DESINSTALAR.EXE".to_lowercase().as_str()),
                sem_nada.into_iter()
            ),
            Pedido::RemoverDeDentro
        ));
    }

    /// O segundo tempo tem o mesmo nome, e o argumento é que os separa.
    ///
    /// Os dois arquivos se chamam `desinstalar.exe`: o de dentro da pasta e a
    /// cópia no temporário. Sem esta distinção, a cópia se trataria como o
    /// primeiro tempo e se copiaria de novo, para sempre.
    #[test]
    fn o_segundo_tempo_vence_o_nome() {
        let pedido = super::ler_com_nome(
            Some("desinstalar.exe"),
            [
                "--desinstalar-de".to_owned(),
                r"C:\Program Files\SEELE".to_owned(),
            ]
            .into_iter(),
        );
        assert!(
            matches!(pedido, Pedido::RemoverA(_)),
            "a cópia no temporário voltou a se tratar como o primeiro tempo"
        );
    }

    /// E o instalador baixado continua instalando.
    ///
    /// Ele se chama `SEELE_<versão>_x64-instalador.exe`, e é esse mesmo arquivo
    /// que o atualizador roda. Nenhum dos dois se chama `desinstalar.exe`.
    #[test]
    fn o_instalador_baixado_nao_e_confundido_com_o_desinstalador() {
        let sem_nada: Vec<String> = Vec::new();
        assert!(matches!(
            super::ler_com_nome(
                Some("seele_0.10.4-3_x64-instalador.exe"),
                sem_nada.into_iter()
            ),
            Pedido::Instalar { .. }
        ));
        assert!(matches!(
            super::ler_com_nome(
                Some("seele_0.10.4-3_x64-instalador.exe"),
                ["/P".to_owned(), "/R".to_owned()].into_iter()
            ),
            Pedido::Instalar { .. }
        ));
    }
}
