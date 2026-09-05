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
        } => janela::abrir(janela::Modo::Instalar, std::path::Path::new("")),
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
        // **A regra de firewall, sozinha, sem instalar nada.**
        //
        // O caminho vem do registro e não da linha de comando: este binário roda
        // elevado, e receber de fora para que programa a regra vale seria deixar
        // quem chama escolher isso.
        linha::Pedido::AbrirAPorta => {
            let destino = instalacao::como_caminho(&instalacao::pasta_padrao());
            let produto = destino.join("SEELE.exe");
            if !produto.is_file() {
                Err(format!(
                    "não achei o SEELE em {} — a regra seria sobre um programa \
                     que não está lá.",
                    produto.display()
                ))
            } else {
                match sistema::regra_de_firewall(&produto, true) {
                    Ok(()) => {
                        // A escolha fica guardada para a atualização
                        // silenciosa não desfazer o que acabou de ser decidido.
                        // Só a porta: quem apertou hospedar não disse nada sobre
                        // o atalho.
                        let _ = registro::guardar_uma_escolha(None, Some(true));
                        println!(
                            "porta 8383 UDP aberta no firewall para {}",
                            produto.display()
                        );
                        Ok(())
                    }
                    Err(erro) => Err(erro),
                }
            }
        }
        linha::Pedido::RemoverDeDentro => desinstalar::sair_de_dentro(),
        // **A janela da remoção roda no segundo tempo, e é o único lugar onde
        // ela pode rodar.**
        //
        // O primeiro tempo é o executável de dentro de `Program Files`, e uma
        // janela ali não poderia apagar a própria pasta — o Windows não deixa
        // remover um diretório com um executável em uso dentro. O segundo tempo
        // roda da cópia no temporário, e é dele que a pasta some.
        //
        // Para quem aperta «desinstalar» no painel a troca é invisível: o
        // primeiro tempo copia, chama e sai na hora, e o que aparece é a janela.
        linha::Pedido::RemoverA(pasta) => janela::abrir(janela::Modo::Remover, &pasta),
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
    fn a_regra_de_firewall_nomeia_o_perfil_que_ela_cobre() {
        // **Esta decisão foi tomada, revogada e registrada no mesmo dia.**
        //
        // Ela nasceu `profile=any`. Em 04/09/2026 passou a `domain,private`,
        // deixando de fora o perfil público — o que o Windows dá ao Wi-Fi de uma
        // cafeteria —, com o argumento de que hospedar de uma é caso raro e
        // carregar o notebook para uma é o caso comum.
        //
        // Voltou a `any` no mesmo dia, a pedido de quem hospeda, depois de o
        // Windows continuar recusando conexão. **A evidência aponta para outro
        // lugar**: numa máquina examinada a regra nomeava
        // `C:\Program Files\SEELE\SEELE.exe` e essa pasta não existia — e uma
        // regra presa a um programa ausente não permite nada, em perfil nenhum.
        //
        // O que este guarda protege agora não é qual perfil, e sim que **haja um
        // escrito**. Uma regra sem `profile` herda o padrão do `netsh`, e o
        // padrão é a coisa que ninguém decidiu.
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sistema.rs");
        let Ok(fonte) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };
        // Sem os comentários: o cabeçalho da função conta as duas decisões, e
        // uma âncora que casa com a explicação acusa o código do que o
        // comentário descreve. Já aconteceu três vezes neste repositório.
        let codigo: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            codigo.contains("\"profile="),
            "a regra de firewall deixou de dizer que perfis ela cobre, e passa a \
             herdar o padrão do netsh — que é a escolha que ninguém fez"
        );

        // E a regra continua presa ao **programa**, que é a parte que nunca
        // esteve em discussão: uma regra por número de porta abriria a 8383 para
        // qualquer coisa que a escutasse depois, inclusive o que for instalado
        // amanhã.
        assert!(
            codigo.contains("program="),
            "a regra deixou de ser do programa, e passou a abrir a porta para \
             qualquer coisa que a escute"
        );
    }
    // **Estes dois moram aqui e não em `registro.rs`.**
    //
    // Aquele arquivo é `#![cfg(windows)]` inteiro, então um módulo de teste
    // dentro dele não compila no Mac — e o Mac é onde a bateria roda a cada
    // mudança. Eles leem fonte como texto e não tocam no registro de ninguém,
    // então não têm por que ser presos ao sistema.

    /// **O `UninstallString` tem de mandar desinstalar.**
    ///
    /// `desinstalar.exe` é uma cópia do instalador — um binário, dois modos,
    /// decididos pela linha de comando. Sem argumento nenhum, `linha::ler`
    /// responde `Instalar`, e o painel do Windows abria a janela de instalação
    /// quando alguém pedia para remover. Relatado assim: «o desinstalador não
    /// desinstala».
    ///
    /// Lido da fonte porque escrever no registro pede elevação e uma máquina
    /// Windows. O que se prende é a decisão, que mora na linha.
    #[test]
    fn o_painel_do_windows_chama_o_desinstalador_no_modo_de_desinstalar() {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/registro.rs");
        let Ok(fonte) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };
        // Sem os comentários: o de cima explica o defeito e cita a linha antiga.
        let codigo: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            codigo.contains("--desinstalar"),
            "o `UninstallString` voltou a chamar o arquivo cru, e remover pelo \
             painel volta a abrir o instalador"
        );
    }

    /// A versão anunciada é a que está sendo instalada.
    ///
    /// Era `CARGO_PKG_VERSION`, e o workspace inteiro é `0.0.0`: a versão de
    /// verdade é injetada no `tauri.conf.json` na hora de empacotar e nunca
    /// chegava aqui. O painel do Windows registrava `DisplayVersion 0.0.0`, e a
    /// janela do instalador dizia «0.0.0 · WINDOWS 64 BITS». Relatado assim: «o
    /// instalador não mostra a versão que ta sendo instalada».
    #[test]
    fn a_versao_anunciada_nao_e_a_do_cargo_toml() {
        for arquivo in ["src/instalacao.rs", "src/janela.rs"] {
            let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(arquivo);
            let Ok(fonte) = std::fs::read_to_string(&caminho) else {
                panic!("não li {}", caminho.display());
            };
            assert!(
                !fonte.contains("env!(\"CARGO_PKG_VERSION\")"),
                "{arquivo} voltou a anunciar a versão do Cargo.toml, que é 0.0.0 \
                 no workspace inteiro"
            );
        }
    }
    /// **A remoção tem passos próprios, e eles não são os da instalação.**
    ///
    /// Ela abria a janela de instalar, com o campo da pasta e as caixas do
    /// atalho e do firewall. Relatado assim: «o desinstalador abre como
    /// instalar, com todo o passo a passo para instalação. Por quê? Ele deveria
    /// ter passos diferentes, como remoção dos dados locais opcional».
    ///
    /// As duas perguntas não se parecem: instalar pergunta **onde** e **o que
    /// mexer**; remover pergunta **o que levar junto**. E a única escolha da
    /// remoção é a que ninguém desfaz.
    #[test]
    fn a_janela_da_remocao_tem_os_passos_dela() {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/janela.rs");
        let Ok(fonte) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };
        let codigo: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for peca in [
            "enum Modo",
            "Passo::Confirmar",
            "Passo::Removendo",
            "Passo::Removido",
        ] {
            assert!(
                codigo.contains(peca),
                "«{peca}» sumiu: a remoção voltou a usar a trilha da instalação"
            );
        }
        assert!(
            codigo.contains("OPCOES_DA_REMOCAO"),
            "a remoção perdeu a escolha dela, e volta a mostrar as caixas do \
             atalho e do firewall — que não têm o que fazer numa remoção"
        );
        // O botão do fim não oferece abrir o que acabou de sair da máquina.
        assert!(
            codigo.contains("Self::Removido => \"FECHAR\""),
            "o passo final da remoção voltou a oferecer abrir o SEELE"
        );
    }

    /// A caixa dos dados nasce desmarcada, e a nota dela diz o que se perde.
    ///
    /// A identidade é uma chave Ed25519 gerada uma vez (ADR 0004) e não há
    /// recuperação de conta em spec nenhuma. Quem a apaga entra nos servidores
    /// de novo como alguém que nunca esteve lá, e o apelido que era dela fica
    /// preso à chave que morreu. Uma caixa marcada por padrão faria isso
    /// acontecer com quem só quis desinstalar.
    #[test]
    fn apagar_os_dados_e_escolha_e_a_nota_diz_o_preco() {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/janela.rs");
        let Ok(fonte) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };
        let tabela = fonte
            .split("const OPCOES_DA_REMOCAO")
            .nth(1)
            .and_then(|resto| resto.split("];").next())
            .unwrap_or_default()
            .to_owned();

        assert!(
            tabela.contains("não há como desfazer"),
            "a nota da caixa deixou de dizer que isto não se desfaz:\n{tabela}"
        );
        assert!(
            tabela.contains("identidade"),
            "a nota não nomeia o que se perde, e «dados» não diz a ninguém que a \
             identidade vai junto:\n{tabela}"
        );

        // E a remoção dos dados é condicional no código que apaga.
        let remocao = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/desinstalar.rs");
        let Ok(fonte) = std::fs::read_to_string(&remocao) else {
            panic!("não li {}", remocao.display());
        };
        assert!(
            fonte.contains("if apagar_dados {"),
            "os dados passaram a ser apagados sem a escolha"
        );

        // **E a caixa nasce desmarcada — conferido, e não afirmado.**
        //
        // Isto faltava, e a falta tem a forma que o CLAUDE.md descreve: o
        // comentário deste teste prometia «a caixa dos dados nasce desmarcada»
        // desde que ele foi escrito, e nenhuma linha dele olhava para o estado
        // inicial. O guarda casava com o próprio comentário.
        //
        // O que estava lá era `opcoes: [true, false]`, um arranjo só para os
        // dois modos: na instalação o `true` é o atalho, e na remoção o mesmo
        // `true` marcava «apagar também os meus dados». A escolha que não se
        // desfaz nascia feita, na tela que existe para perguntar. Só apareceu
        // quando alguém mandou uma captura da janela rodando.
        let janela = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/janela.rs");
        let Ok(fonte) = std::fs::read_to_string(&janela) else {
            panic!("não li {}", janela.display());
        };
        let inicial = sem_comentarios(&fonte);
        assert!(
            inicial.contains("opcoes: match modo"),
            "as caixas voltaram a nascer com um estado só para os dois modos, e \
             o que é conveniência na instalação é destruição na remoção"
        );
        assert!(
            inicial.contains("Modo::Remover => [false, false]"),
            "a caixa de apagar os dados voltou a nascer marcada"
        );
    }

    /// Lê o arquivo sem as linhas de comentário.
    ///
    /// Existe porque três guardas deste repositório já casaram com o próprio
    /// comentário: o texto que eles procuravam estava na prosa que explica a
    /// regra, e não no código que a cumpre.
    fn sem_comentarios(fonte: &str) -> String {
        fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// **A janela da remoção deixa de se desenhar como a da instalação.**
    ///
    /// Os quatro defeitos vieram na mesma captura, e são a mesma causa: o modo
    /// de remoção herdava a geometria e as palavras do modo de instalar.
    ///
    /// - a barra de cima dizia INSTALAR O SEELE numa janela cujo botão diz
    ///   REMOVER;
    /// - a fita dividia a largura por quatro, e sobrava um degrau vazio à
    ///   direita, porque a remoção tem três passos;
    /// - o rodapé anunciava «PASSO 01 DE 04» na fita de três;
    /// - e o parágrafo do que sai era escrito **por cima** do título do passo,
    ///   deixando as duas frases embaralhadas e ilegíveis.
    #[test]
    fn a_remocao_nao_se_desenha_com_as_medidas_da_instalacao() {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/janela.rs");
        let Ok(fonte) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };
        let codigo = sem_comentarios(&fonte);

        assert!(
            codigo.contains("Self::Remover => \"REMOVER O SEELE\""),
            "a barra de cima voltou a anunciar uma instalação numa janela que \
             remove"
        );
        assert!(
            !codigo.contains("DE 04"),
            "o rodapé voltou a contar quatro passos, e a remoção tem três"
        );
        assert!(
            codigo.contains("estado.modo.passos().len()"),
            "a fita ou o rodapé deixaram de contar os passos deste modo"
        );
        assert!(
            !codigo.contains("(largura - 2) / 4"),
            "a fita voltou a dividir a largura por quatro, e sobra um degrau \
             vazio no modo que tem três"
        );

        // E o parágrafo da remoção não pode dividir o `top` com o título.
        let titulo = codigo
            .split("estado.passo.titulo()")
            .next()
            .and_then(|antes| antes.rsplit("RECT {").next())
            .unwrap_or_default()
            .to_owned();
        let paragrafo = codigo
            .split("Saem desta máquina")
            .next()
            .and_then(|antes| antes.rsplit("RECT {").next())
            .unwrap_or_default()
            .to_owned();
        let altura_de = |trecho: &str| {
            trecho
                .split("top: topo + px(")
                .nth(1)
                .and_then(|resto| resto.split(')').next())
                .and_then(|numero| numero.parse::<i32>().ok())
        };
        let (Some(do_titulo), Some(do_paragrafo)) = (altura_de(&titulo), altura_de(&paragrafo))
        else {
            panic!("não achei os dois `top` para comparar:\n{titulo}\n---\n{paragrafo}");
        };
        assert!(
            do_paragrafo > do_titulo,
            "o parágrafo do que sai voltou a ser escrito em cima do título do \
             passo: título em {do_titulo}, parágrafo em {do_paragrafo}"
        );
    }
    /// **A pasta fica com o que é desta instalação, e não com três.**
    ///
    /// Relatado assim: «por que na pasta SEELE fica: seele-app, SEELE, seeled,
    /// plug, uninstall e desinstalar? Não tem coisas que estão se repetindo?».
    /// Tem, e são de três instaladores diferentes: a nossa carga escreve dois
    /// arquivos e o instalador acrescenta o desinstalador; `seele-app.exe` e
    /// `uninstall.exe` são do NSIS do Tauri, e `plug.exe` é de antes de o
    /// vocabulário mudar.
    ///
    /// `seele-app.exe` é o mesmo programa que o nosso `SEELE.exe` — a carga o
    /// renomeia ao empacotar —, e cada instalador aponta o atalho dele para o
    /// seu. Dois atalhos para dois nomes do mesmo binário é como uma máquina
    /// «volta de versão» sem ninguém entender.
    #[test]
    fn a_instalacao_leva_junto_o_entulho_das_anteriores() {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/instalacao.rs");
        let Ok(fonte) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };
        let codigo: String = fonte
            .lines()
            .filter(|linha| !linha.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for resto in ["seele-app.exe", "uninstall.exe", "plug.exe"] {
            assert!(
                codigo.contains(resto),
                "«{resto}» deixou de ser removido, e a pasta volta a acumular \
                 arquivos de instaladores que já não existem"
            );
        }
        assert!(
            codigo.contains("esquecer_a_do_nsis"),
            "a entrada do NSIS no painel deixou de sair, e «Programas e Recursos» \
             volta a mostrar dois SEELE, cada um removendo metade"
        );

        // **Lista nomeada, e não «tudo o que não é nosso».** Esta é uma pasta de
        // `Program Files`, e apagar por regra o que não se reconhece é como um
        // instalador leva junto o que outra pessoa pôs ali.
        assert!(
            !codigo.contains("read_dir(destino)"),
            "a limpeza passou a varrer a pasta em vez de nomear o que remove"
        );
    }
}
