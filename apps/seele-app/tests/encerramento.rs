//! Fechar a janela tem de avisar quem hospeda.
//!
//! Um socket UDP que some não avisa ninguém: QUIC precisa de um
//! `CONNECTION_CLOSE` dito por extenso. Sem ele, quem hospeda só descobre a
//! ausência no tempo limite de ociosidade, e até lá a pessoa continua sentada na
//! sala para todo mundo — «se eu fecho o app no Mac, o usuário não sai da sala
//! para o Windows».
//!
//! **A prova de comportamento mora em
//! `crates/seele-conformance/tests/saida_com_despedida.rs`**: quem sai pela FFI,
//! pelo enlace num runtime que cai em seguida, e por `encerrar_e_esperar` num
//! processo que morre na linha seguinte, some dos presentes dos outros em menos
//! de dois segundos. O que fica aqui é a ligação que só o `main.rs` tem e que
//! nenhum teste alcança sem casca: o gancho de saída chama quem se despede, e a
//! despedida espera a thread da conexão em vez de dormir às cegas. É o mesmo
//! recurso das outras invariantes desta casa que não viram tipo.

#![allow(clippy::expect_used)]

use std::path::Path;

fn main_rs() -> String {
    let caminho = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    std::fs::read_to_string(&caminho)
        .unwrap_or_else(|erro| panic!("não li o main.rs: {erro}"))
        .replace("\r\n", "\n")
}

/// O corpo da função de nome `nome`, da assinatura até a chave que a fecha na
/// coluna zero, **sem os comentários**.
///
/// Com eles dentro, a chamada certa escrita num comentário — `// era:
/// despedir_da_sessao(...)` em cima de um `disconnect()` — deixava os dois
/// guardas verdes com o app de volta ao defeito (medido na revisão do lote F).
fn corpo(fonte: &str, nome: &str) -> String {
    let assinatura = format!("\nfn {nome}(");
    let comeco = fonte
        .find(&assinatura)
        .unwrap_or_else(|| panic!("a função `{nome}` sumiu do main.rs"));
    let resto = fonte.get(comeco..).expect("o começo é de um `find`");
    let fim = resto
        .find("\n}\n")
        .unwrap_or_else(|| panic!("a função `{nome}` não fecha na coluna zero"));
    sem_comentarios(resto.get(..fim).expect("o fim é de um `find`"))
}

/// O texto sem os comentários de linha (`//` até o fim da linha) e de bloco
/// (`/* … */`), e com o que está entre aspas intacto: um `//` dentro de uma
/// frase não corta a linha.
///
/// Não conhece a string crua (`r"…"`) nem o caractere `'"'`: nenhum dos dois
/// aparece nas funções que estes guardas leem, e um deles aparecendo faz o
/// guarda reprovar em vez de passar, porque corta texto a mais.
fn sem_comentarios(texto: &str) -> String {
    let mut limpo = String::with_capacity(texto.len());
    let mut letras = texto.chars().peekable();
    let mut entre_aspas = false;
    while let Some(letra) = letras.next() {
        if entre_aspas {
            limpo.push(letra);
            match letra {
                '\\' => {
                    if let Some(escapada) = letras.next() {
                        limpo.push(escapada);
                    }
                }
                '"' => entre_aspas = false,
                _ => {}
            }
            continue;
        }
        match (letra, letras.peek()) {
            ('"', _) => {
                entre_aspas = true;
                limpo.push(letra);
            }
            ('/', Some('/')) => {
                for resto in letras.by_ref() {
                    if resto == '\n' {
                        limpo.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                letras.next();
                let mut anterior = ' ';
                for resto in letras.by_ref() {
                    if anterior == '*' && resto == '/' {
                        break;
                    }
                    anterior = resto;
                }
            }
            _ => limpo.push(letra),
        }
    }
    limpo
}

#[test]
fn fechar_a_janela_avisa_o_servidor() {
    let fonte = main_rs();
    assert!(
        fonte.contains("RunEvent::ExitRequested"),
        "o app deixou de tratar o fim da última janela.\n\
         Sem esse gancho o processo morre em silêncio e quem hospeda mantém a \
         pessoa sentada na sala até o tempo limite de ociosidade."
    );
    let gancho = fonte
        .find("RunEvent::ExitRequested")
        .expect("já conferido acima");
    let chamada = fonte
        .find("despedir_se(handle)")
        .expect("o gancho de saída não chama mais quem avisa o servidor");
    assert!(
        gancho < chamada,
        "o gancho de saída existe mas não é ele que chama a despedida"
    );
    assert!(
        corpo(&fonte, "despedir_se").contains("despedir_da_sessao("),
        "a despedida da janela deixou de passar por `despedir_da_sessao`, que é quem manda \
         os MODs parar e espera a thread da conexão"
    );
    assert!(
        corpo(&fonte, "despedir_da_sessao").contains("encerrar_e_esperar("),
        "a despedida deixou de encerrar a conexão por `encerrar_e_esperar`, que é o que \
         faz o QUIC mandar o `CONNECTION_CLOSE` antes de o processo morrer"
    );
}

#[test]
fn a_despedida_espera_o_quadro_sair() {
    // `disconnect` só **pede** o fim, e o `CONNECTION_CLOSE` sai pela despedida
    // do enlace, na thread da conexão. A espera tem de ser **por ela**, com um
    // teto: um `sleep` dormia às cegas, e o fechamento morria na fila com o
    // runtime da conexão. Ver `saida_com_despedida.rs`, em `seele-conformance`.
    let fonte = main_rs();
    let despedida = corpo(&fonte, "despedir_se");
    let da_sessao = corpo(&fonte, "despedir_da_sessao");
    assert!(
        !despedida.contains("sleep"),
        "a despedida voltou a dormir às cegas em vez de esperar a thread da conexão.\n\
         Um `sleep` não sabe se o `CONNECTION_CLOSE` saiu, e o defeito volta sem nada mudar \
         de aparência."
    );
    assert!(
        despedida.contains("despedir_da_sessao("),
        "a despedida da janela deixou de passar por `despedir_da_sessao`, e com ela pela \
         espera da thread da conexão: o processo pode morrer antes de o aviso deixar a máquina"
    );
    let teto = teto_da_espera(&da_sessao);
    assert!(
        teto.is_some_and(|milissegundos| milissegundos > 0),
        "a despedida deixou de esperar a thread da conexão com um teto que não seja zero \
         (lido: {teto:?} ms).\n\
         Sem a espera, ou com um teto zero, o processo pode morrer antes de o aviso deixar a \
         máquina; sem teto nenhum, uma thread presa segura o fim do app."
    );
}

/// O teto que o corpo passa a `encerrar_e_esperar`, em milissegundos, quando ele
/// está escrito como `Duration::from_millis(N)` ou `Duration::from_secs(N)`.
///
/// `None` para qualquer outra forma: um teto que este guarda não sabe ler é um
/// teto que ele não confere, e reprovar obriga quem o mudou a ensiná-lo.
fn teto_da_espera(corpo: &str) -> Option<u64> {
    let depois = corpo.split_once("encerrar_e_esperar(")?.1;
    let depois = depois.strip_prefix("std::time::").unwrap_or(depois);
    let depois = depois.strip_prefix("Duration::")?;
    let (escala, numero) = match depois.strip_prefix("from_millis(") {
        Some(numero) => (1, numero),
        None => (1000, depois.strip_prefix("from_secs(")?),
    };
    let digitos: String = numero
        .chars()
        .take_while(|letra| letra.is_ascii_digit() || *letra == '_')
        .filter(|letra| *letra != '_')
        .collect();
    digitos.parse::<u64>().ok()?.checked_mul(escala)
}
