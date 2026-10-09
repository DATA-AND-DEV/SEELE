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
    sem_comentarios_lido(
        &format!("o corpo de `{nome}`"),
        resto.get(..fim).expect("o fim é de um `find`"),
    )
}

/// O fecho que `main` passa a `app.run`, **sem os comentários**: é ali que o
/// `ExitRequested` chama a despedida.
///
/// Recortado do `app.run(|` até a chave que fecha `main` na coluna zero, e
/// limpo só esse trecho. O `main.rs` inteiro não passa por
/// [`sem_comentarios`]: ele tem `'"'` e strings cruas, que ela não lê.
fn fecho_do_app_run(fonte: &str) -> String {
    let comeco = fonte
        .find("app.run(|")
        .expect("o `main` deixou de rodar o app por `app.run` com um fecho");
    let resto = fonte.get(comeco..).expect("o começo é de um `find`");
    let fim = resto
        .find("\n}\n")
        .expect("o `main` não fecha na coluna zero depois de `app.run`");
    let limpo = sem_comentarios_lido(
        "o fim de `main`, de `app.run` em diante",
        resto.get(..fim).expect("o fim é de um `find`"),
    );
    bloco(&limpo, 0)
        .expect("o fecho de `app.run` não tem corpo entre chaves")
        .to_owned()
}

/// O que está entre a primeira `{` a partir de `desde` e a `}` que a fecha.
///
/// Conta chaves sem olhar aspas: serve para texto já sem comentários e sem
/// chave dentro de frase, que é o caso dos trechos que este guarda lê.
fn bloco(texto: &str, desde: usize) -> Option<&str> {
    let resto = texto.get(desde..)?;
    let abre = resto.find('{')?;
    let mut fundura = 0_usize;
    for (onde, letra) in resto.char_indices().skip_while(|(onde, _)| *onde < abre) {
        match letra {
            '{' => fundura += 1,
            '}' => {
                fundura = fundura.checked_sub(1)?;
                if fundura == 0 {
                    return resto.get(abre + 1..onde);
                }
            }
            _ => {}
        }
    }
    None
}

/// [`sem_comentarios`], com o guarda reprovando o trecho que ela não lê.
///
/// Medido na re-revisão do lote F: `let _aspa = '"';` antes de `//
/// despedir_da_sessao(...)` deixava os dois guardas verdes — a aspa do
/// caractere invertia o dentro e o fora das aspas, e o comentário passava por
/// código. Aqui o guarda reprova, com frase própria, em vez de ler errado.
fn sem_comentarios_lido(onde: &str, trecho: &str) -> String {
    sem_comentarios(trecho).unwrap_or_else(|perigo| {
        panic!(
            "{onde} tem `{perigo}`, e o leitor de comentários deste guarda não lê esse trecho \
             com segurança: um comentário pode passar por código. Tire o `{perigo}` dali, ou \
             ensine `sem_comentarios` a lê-lo."
        )
    })
}

/// O texto sem os comentários de linha (`//` até o fim da linha) e de bloco
/// (`/* … */`), e com o que está entre aspas intacto: um `//` dentro de uma
/// frase não corta a linha.
///
/// **Não lê** o caractere `'"'` (nem `'\"'`) nem a string crua (`r"…"`,
/// `r#"…"#`, e as de bytes): eles invertem o dentro e o fora das aspas.
/// Achando um deles no código — fora de aspas e de comentário —, ela devolve
/// qual foi, e quem a chama reprova. Um `r` no fim de uma frase (`"parar"`) não
/// é string crua: está entre aspas, e o `r` de uma string crua começa palavra.
fn sem_comentarios(texto: &str) -> Result<String, &'static str> {
    let letras: Vec<char> = texto.chars().collect();
    let em = |onde: usize| letras.get(onde).copied();
    let mut limpo = String::with_capacity(texto.len());
    let mut entre_aspas = false;
    let mut i = 0_usize;
    while let Some(letra) = em(i) {
        i += 1;
        if entre_aspas {
            limpo.push(letra);
            match letra {
                '\\' => {
                    if let Some(escapada) = em(i) {
                        limpo.push(escapada);
                        i += 1;
                    }
                }
                '"' => entre_aspas = false,
                _ => {}
            }
            continue;
        }
        let comeca_palavra = i
            .checked_sub(2)
            .and_then(em)
            .is_none_or(|antes| !(antes.is_alphanumeric() || antes == '_'));
        match letra {
            '\'' if em(i) == Some('"') || (em(i) == Some('\\') && em(i + 1) == Some('"')) => {
                return Err("'\"'");
            }
            'r' | 'b' if comeca_palavra => {
                if let Some(perigo) = string_crua(&letras, i - 1) {
                    return Err(perigo);
                }
                limpo.push(letra);
            }
            '"' => {
                entre_aspas = true;
                limpo.push(letra);
            }
            '/' if em(i) == Some('/') => {
                while let Some(resto) = em(i) {
                    i += 1;
                    if resto == '\n' {
                        limpo.push('\n');
                        break;
                    }
                }
            }
            '/' if em(i) == Some('*') => {
                i += 1;
                let mut anterior = ' ';
                while let Some(resto) = em(i) {
                    i += 1;
                    if anterior == '*' && resto == '/' {
                        break;
                    }
                    anterior = resto;
                }
            }
            _ => limpo.push(letra),
        }
    }
    Ok(limpo)
}

/// Se uma string crua começa em `onde`: `r"`, `r#…#"`, `br"` ou `br#…#"`.
/// Devolve como o guarda a nomeia. `r#tipo`, o identificador cru, não é.
fn string_crua(letras: &[char], onde: usize) -> Option<&'static str> {
    let mut i = onde;
    if letras.get(i) == Some(&'b') {
        i += 1;
    }
    if letras.get(i) != Some(&'r') {
        return None;
    }
    i += 1;
    match letras.get(i) {
        Some('"') => Some("r\""),
        Some('#') => {
            while letras.get(i) == Some(&'#') {
                i += 1;
            }
            (letras.get(i) == Some(&'"')).then_some("r#\"")
        }
        _ => None,
    }
}

#[test]
fn fechar_a_janela_avisa_o_servidor() {
    let fonte = main_rs();
    // No fecho de `app.run` e sem comentários: no texto cru, `//
    // despedir_se(handle);` deixava este guarda verde com o app saindo sem
    // despedida nenhuma (medido na re-revisão do lote F).
    let fecho = fecho_do_app_run(&fonte);
    let gancho = fecho.find("RunEvent::ExitRequested").unwrap_or_else(|| {
        panic!(
            "o app deixou de tratar o fim da última janela.\n\
             Sem esse gancho o processo morre em silêncio e quem hospeda mantém a \
             pessoa sentada na sala até o tempo limite de ociosidade."
        )
    });
    // O braço é o bloco do `if` que confere o `ExitRequested`. O padrão
    // `{ .. }` do `matches!` fecha antes do `)` dele, e o bloco vem depois.
    let fim_do_padrao = fecho
        .get(gancho..)
        .and_then(|resto| resto.find(')'))
        .map(|onde| gancho + onde)
        .expect("o `matches!` do `ExitRequested` não fecha");
    let braco = bloco(&fecho, fim_do_padrao).expect("o `ExitRequested` não tem braço entre chaves");
    assert!(
        braco.contains("despedir_se(handle)"),
        "o braço do `ExitRequested` não chama mais `despedir_se(handle)` fora de um \
         comentário: a última janela fecha e o processo morre sem avisar o servidor, ou avisa \
         a cada evento em vez de no fim.\nO braço lido:{braco}"
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
