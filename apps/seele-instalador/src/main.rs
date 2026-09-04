//! O instalador do SEELE para Windows. Ver ADR 0043.
//!
//! # O que ele é
//!
//! Um programa próprio, com janela desenhada em Win32/GDI, no lugar do modelo do
//! `tauri-bundler` com páginas trocadas. O desenho está em `Instalador
//! SEELE.dc.html`, no projeto de design.
//!
//! # Por que não WebView2
//!
//! O desenho é HTML, e a saída óbvia seria renderizá-lo numa janela WebView2 — o
//! SEELE já depende dele. Mas o instalador é justamente quem **instala o runtime
//! do WebView2** quando ele falta, e é isso que faz o SEELE abrir numa máquina
//! limpa. Um instalador que dependesse dele precisaria dele para dizer que o
//! está instalando.
//!
//! O GDI dá conta porque o desenho colabora: retângulo chapado, borda de 1px,
//! zero arredondamento, zero sombra, zero gradiente — o que `tokens.css` já
//! impõe ao produto inteiro.
//!
//! # O contrato
//!
//! O que o NSIS fazia por baixo, e que aqui é obrigação escrita. A lista está no
//! ADR 0043 e repetida em [`OBRIGACOES`], que é lida por um teste: o modo de
//! falhar de um instalador é esquecer um item dela, e o esquecimento só aparece
//! semanas depois, na máquina de outra pessoa.
//!
//! O item que mais assusta é o **modo silencioso**: ele não tem tela, ninguém o
//! exercita à mão, e é por onde passa toda atualização do produto.
//!
//! # Este arquivo hoje
//!
//! O esqueleto. Ele existe para que a crate nasça na árvore com o contrato
//! escrito e conferido antes de qualquer janela ser desenhada — a ordem que o
//! ADR pede, e a que evita descobrir a obrigação esquecida depois de o
//! instalador estar pronto.

mod carga;
#[cfg(windows)]
mod desinstalar;
#[cfg(windows)]
mod instalacao;
#[cfg(windows)]
mod janela;
mod linha;
mod pele;
#[cfg(windows)]
mod registro;
#[cfg(windows)]
mod sistema;

/// Cada coisa que o instalador do NSIS fazia, e o que quebra sem ela.
///
/// **Escrita como dado, e não como prosa, porque um teste a lê.** Uma lista em
/// comentário é uma lista que ninguém confere; esta é comparada com o ADR 0043,
/// e as duas divergirem é um erro de teste — não uma nota de rodapé que envelhece
/// sozinha.
pub const OBRIGACOES: &[(&str, &str)] = &[
    (
        "copiar os arquivos e escrever o desinstalador",
        "não há produto instalado, nem como removê-lo",
    ),
    (
        "a entrada em «Aplicativos instalados»",
        "o app não sai mais pelo painel do Windows",
    ),
    (
        "`EstimatedSize`, `DisplayVersion`, `UninstallString` e os demais valores",
        "a entrada aparece pela metade, sem tamanho nem versão",
    ),
    (
        "atalhos do menu Iniciar e da área de trabalho",
        "e migrar os antigos, que apontam para o nome velho do binário",
    ),
    (
        "instalar o WebView2 se faltar",
        "o SEELE não abre numa máquina limpa",
    ),
    (
        "a regra de firewall da 8383, do programa, em rede confiável",
        "quem hospeda fica invisível e não descobre por quê",
    ),
    (
        "apagar a instalação por usuário da 0.7.1",
        "o app «volta de versão»: o atalho velho abre a cópia velha",
    ),
    (
        "o modo silencioso",
        "ninguém mais recebe atualização, e nada aparece na tela para dizer isso",
    ),
    (
        "recusar-se a rodar com o app aberto, em arquitetura errada ou Windows velho",
        "arquivo em uso, e uma instalação pela metade",
    ),
];

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    // **A janela abre e ainda não instala nada.** É a etapa 1 do ADR 0043: ver o
    // desenho de pé antes de escrever o motor. Um instalador que desenha certo e
    // instala errado é pior que um que não abre, então o motor vem depois, com
    // as obrigações desta lista uma a uma.
    // Os dois modos de remoção não abrem janela nenhuma: o painel do Windows
    // chama o `UninstallString` e espera um programa que faça e saia. Uma janela
    // aqui seria uma segunda tela pedindo a mesma confirmação que o painel já
    // pediu.
    let resultado = match linha::ler_pedido() {
        linha::Pedido::Instalar {
            tela: linha::Tela::Cheia,
            ..
        } => janela::abrir(),
        // **Sem tela, e este é o caminho que ninguém olha.** É por ele que
        // passa toda atualização do SEELE: o app baixa este `.exe` e o roda com
        // `/P /R`. As escolhas vêm do registro — quem tinha a porta aberta a
        // mantém —, e a pasta é a de antes, senão a atualização instalaria noutro
        // lugar e deixaria duas cópias.
        linha::Pedido::Instalar {
            tela,
            reiniciar,
            argumentos_do_app,
        } => sem_perguntar(tela, reiniciar, &argumentos_do_app),
        linha::Pedido::RemoverDeDentro => desinstalar::sair_de_dentro(),
        linha::Pedido::RemoverA(pasta) => desinstalar::remover(&pasta),
    };

    match resultado {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(motivo) => {
            eprintln!("seele-instalador: {motivo}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Fora do Windows ele compila e recusa.
///
/// **Compila de propósito**, em vez de sair do workspace por `cfg`: a bateria
/// roda `cargo test --workspace` nas duas máquinas, e uma crate que só existe
/// numa delas é uma crate cujos testes ninguém vê na outra. O que não compila
/// fora do Windows é a janela, e ela mora atrás de `cfg(windows)`.
#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("seele-instalador: este programa é do Windows.");
    std::process::ExitCode::FAILURE
}

/// Instalar sem perguntar nada — o caminho da atualização automática.
///
/// # Por que ele não abre janela nem no modo passivo
///
/// O `/P` do NSIS mostrava a página de andamento. Aqui ele não mostra, e a
/// diferença é deliberada: uma janela que aparece sozinha por cima do que a
/// pessoa está fazendo, durante uma atualização que ela não pediu, é uma
/// interrupção — e a atualização do SEELE acontece quando o app decide, não
/// quando alguém clica.
///
/// O que o `/P` continua significando aqui é «não pergunte nada», que é o que ele
/// sempre significou de fato.
///
/// # Errors
///
/// Devolve o que impediu. Ninguém está olhando, então a mensagem vai para a
/// saída de erro — e o código de saída é o que o atualizador lê.
#[cfg(windows)]
fn sem_perguntar(
    _tela: linha::Tela,
    reiniciar: bool,
    argumentos_do_app: &[String],
) -> Result<(), String> {
    let destino = instalacao::como_caminho(&instalacao::pasta_padrao());
    let escolhas = instalacao::Escolhas::de_antes();

    instalacao::executar(&destino, escolhas, &|passo| {
        // A saída padrão, e não silêncio: quem roda isto à mão quer ver, e quem
        // roda por atualização não lê nada de qualquer jeito.
        println!("{passo}");
    })?;

    if reiniciar {
        // **Os argumentos do app não vão junto, e isto diz quando havia algum.**
        //
        // Baixar o privilégio é `explorer.exe <programa>`, e o Explorer não
        // encaminha argumento nenhum — ver `abrir_o_produto`. Na prática não há
        // o que perder, porque o `seele-app` não lê `argv`; mas descartar em
        // silêncio é como o defeito anterior nasceu, e uma linha na saída custa
        // nada a quem não a lê.
        if !argumentos_do_app.is_empty() {
            println!(
                "os argumentos do app não foram repassados, porque o Explorer \
                 não os encaminha: {}",
                argumentos_do_app.join(" ")
            );
        }
        instalacao::abrir_o_produto(&destino);
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::OBRIGACOES;

    #[test]
    fn o_contrato_daqui_e_o_mesmo_que_o_adr_escreveu() {
        // Duas cópias da mesma lista — uma em prosa, para quem lê a decisão, e
        // uma em dado, para quem escreve o código — divergem sozinhas. Esta é a
        // única coisa que impede.
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/adr/0043-o-instalador-do-windows-e-nosso.md");
        // `let-else` e não `expect`: a folha de lints do workspace nega o
        // `expect`, e a exceção que os testes de integração têm não alcança um
        // teste unitário dentro do binário.
        let Ok(adr) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };

        for (obrigacao, _) in OBRIGACOES {
            assert!(
                adr.contains(obrigacao),
                "«{obrigacao}» está no código e não está no ADR 0043.\n\
                 A tabela de lá é o que alguém lê para decidir; a daqui é o que \
                 alguém lê para implementar. Elas divergirem é como uma \
                 obrigação some."
            );
        }
    }

    #[test]
    fn o_botao_que_diz_abrir_o_seele_abre_o_seele() {
        // **O defeito que este guarda existe para pegar já aconteceu.**
        //
        // O botão do passo 04 diz ABRIR O SEELE, e a primeira versão dele só
        // fechava a janela — com um comentário ao lado afirmando que o produto
        // abria no lugar dela. A intenção estava escrita e não implementada, e
        // relendo o código ela parecia certa: o comentário dizia o que deveria
        // acontecer.
        //
        // Quem apertava via o instalador sumir e mais nada. Não há erro, não há
        // log, não há tela — é um botão que promete e cala.
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/janela.rs");
        let Ok(janela) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };

        let Some(ramo) = janela
            .split("Passo::Pronto => {")
            .nth(1)
            .and_then(|resto| resto.split("\n            }").next())
        else {
            panic!("o passo 04 perdeu o ramo do botão que avança");
        };

        assert!(
            ramo.contains("abrir_o_produto"),
            "o botão do passo 04 fecha a janela e não abre o produto.\n\
             Ele diz ABRIR O SEELE: quem aperta vê o instalador sumir e mais \
             nada — sem erro, sem log, sem tela."
        );
    }

    #[test]
    fn nenhuma_obrigacao_esta_sem_consequencia() {
        // Uma obrigação sem o que quebra ao lado é uma linha que ninguém sabe
        // priorizar — e a primeira a ser cortada quando o prazo aperta.
        for (obrigacao, quebra) in OBRIGACOES {
            assert!(
                !quebra.trim().is_empty() && quebra.trim() != "—",
                "a obrigação «{obrigacao}» não diz o que quebra sem ela"
            );
        }
    }
    #[test]
    fn a_regra_de_firewall_nao_vale_em_rede_publica() {
        // **Era `profile=any`, e isso incluía o público.**
        //
        // «Público» é o que o Windows escolhe para uma rede em que não se
        // confia — o Wi-Fi de uma cafeteria, de um aeroporto, de um hotel. Com
        // `any`, a regra valia lá: todo mundo naquela rede podia bater na porta
        // do SEELE de quem só foi tomar café.
        //
        // A conta que mudou isso é simples: hospedar de uma cafeteria é caso
        // raro, e **carregar o notebook para uma é o caso comum**. As três
        // paredes que respondem depois — o balde por endereço, o segredo, a
        // portaria — são paredes, e não a ausência de contato.
        //
        // Não dá para exercitar o `netsh` num teste; o que dá é prender a lista
        // de argumentos, que é onde a decisão mora.
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sistema.rs");
        let Ok(fonte) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };
        // Sem os comentários: o cabeçalho da função explica que `any` era o que
        // havia antes, e uma âncora que casa com a própria explicação acusa o
        // código de ter o defeito que o comentário descreve. Já aconteceu duas
        // vezes neste repositório.
        let codigo: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            codigo.contains("\"profile=domain,private\""),
            "a regra de firewall deixou de nomear os perfis confiáveis"
        );
        assert!(
            !codigo.contains("\"profile=any\""),
            "a regra voltou a valer em rede pública, e passa a deixar entrar \
             conexão no Wi-Fi de qualquer lugar onde a máquina for aberta"
        );
    }
}
