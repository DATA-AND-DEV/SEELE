//! **As regras do nome de um anexo**, que não tocam em disco nenhum (Tarefa 2.6
//! da F4): o nome que veio de quem mandou vira nome de arquivo só se servir no
//! Windows, no Mac e no Linux, e a frase que o recusa o mostra com o invisível
//! escrito por extenso.
//!
//! Moravam no `seele_core::anexo_no_disco`, ao lado do que grava; saíram para
//! cá porque o navegador recusa os mesmos nomes antes de entregar o arquivo ao
//! download dele, e o navegador não leva o `seele-core`. O `anexo_no_disco` as
//! reexporta, e o desktop não mudou. Os testes delas continuam lá, junto dos
//! casos de disco que as exercitam.

use seele_proto::control::MAX_FILE_NAME_LEN;

/// Por que um nome alegado não vira nome de arquivo.
///
/// Diz qual regra pegou, para o `seele.log`, e não para a tela: a pessoa recebe
/// uma frase só, e quem investiga depois precisa saber qual das dez foi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NomeRecusado {
    /// Vazio, ou só espaço.
    #[error("o nome está vazio")]
    Vazio,
    /// `.`, `..`, ou só pontos.
    #[error("o nome é só de pontos")]
    SoPontos,
    /// Tem `/` ou `\`, que são separador em algum dos três sistemas.
    #[error("o nome tem um separador de pasta")]
    Separador,
    /// Tem `:` — a unidade do Windows (`C:x.bat`) e o fluxo alternativo do NTFS
    /// (`a.txt:Zone.Identifier`).
    #[error("o nome tem dois-pontos")]
    DoisPontos,
    /// Tem um de `< > " | ? *`, que o Windows não aceita em nome de arquivo.
    #[error("o nome tem um caractere que o Windows proíbe")]
    ProibidoNoWindows,
    /// Tem um caractere de controle, o NUL incluso.
    #[error("o nome tem um caractere de controle")]
    Controle,
    /// Tem um caractere de formatação invisível, que disfarça a extensão.
    #[error("o nome tem um caractere de formatação invisível")]
    Formatacao,
    /// O radical é um dos nomes que o Windows reserva para dispositivo.
    #[error("o nome é um dos que o Windows reserva")]
    ReservadoNoWindows,
    /// Termina em ponto ou em espaço, que o Windows apaga em silêncio.
    #[error("o nome termina em ponto ou em espaço")]
    PontoOuEspacoNoFim,
    /// Passa de [`MAX_FILE_NAME_LEN`] bytes.
    #[error("o nome passa de {MAX_FILE_NAME_LEN} bytes")]
    LongoDemais,
}

/// Os caracteres de formatação que disfarçam um nome, um por um.
///
/// Por lista, e não pela categoria Unicode `Cf` inteira: a std não a expõe, e
/// um crate só para isto seria uma dependência nova por uma tabela de seis
/// faixas. São as que reordenam ou escondem texto — a marca árabe (U+061C), os
/// de largura zero e as marcas de direção (U+200B–U+200F), os embutidos e
/// sobrepostos de direção (U+202A–U+202E, onde mora o RLO), os invisíveis de
/// junção e operação (U+2060–U+2064), os isolados de direção (U+2066–U+2069) e o
/// BOM (U+FEFF).
const FORMATACAO: [(char, char); 6] = [
    ('\u{061C}', '\u{061C}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{202A}', '\u{202E}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{2069}'),
    ('\u{FEFF}', '\u{FEFF}'),
];

/// Os nomes que o Windows reserva para dispositivo, em maiúsculas.
///
/// De onde vem a lista: a página «Naming Files, Paths, and Namespaces» da
/// documentação do Win32, que manda não usar `CON`, `PRN`, `AUX`, `NUL`, `COM0`
/// a `COM9`, `COM¹`, `COM²`, `COM³`, `LPT0` a `LPT9`, `LPT¹`, `LPT²` e `LPT³` —
/// os três sobrescritos porque o Windows os lê como `COM1`, `COM2`… —, nem
/// esses nomes seguidos de extensão. E mais `CONIN$` e `CONOUT$`, a entrada e a
/// saída do console, que o `CreateFile` abre como dispositivo.
///
/// O `ntpath.isreserved` do Python, que cita a mesma página, recusa `CONIN$` e
/// `CONOUT$` e não recusa `COM0` nem `LPT0` (conferido no 3.14). Os dois zeros
/// ficam aqui assim mesmo: recusar um nome a mais custa a quem mandou escolher
/// outro, e gravar num dispositivo custa o arquivo.
///
/// Valem com qualquer extensão — `nul.txt` é o NUL — e sem caixa.
const RESERVADOS: [&str; 32] = [
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM0", "COM1", "COM2", "COM3", "COM4",
    "COM5", "COM6", "COM7", "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
];

fn e_formatacao(letra: char) -> bool {
    FORMATACAO
        .iter()
        .any(|&(de, ate)| (de..=ate).contains(&letra))
}

/// O nome alegado, se ele for só um nome de arquivo.
///
/// A regra é a mesma nos três sistemas, e é a mais estrita das três: um nome que
/// o Windows recusa é recusado também no Mac, porque quem mandou não sabe onde o
/// arquivo vai ser salvo.
///
/// # Errors
///
/// [`NomeRecusado`], com a regra que pegou.
pub fn nome_seguro(alegado: &str) -> Result<&str, NomeRecusado> {
    if alegado.trim().is_empty() {
        return Err(NomeRecusado::Vazio);
    }
    if alegado.chars().all(|letra| letra == '.') {
        return Err(NomeRecusado::SoPontos);
    }
    if alegado.len() > MAX_FILE_NAME_LEN {
        return Err(NomeRecusado::LongoDemais);
    }
    for letra in alegado.chars() {
        if matches!(letra, '/' | '\\') {
            return Err(NomeRecusado::Separador);
        }
        if letra == ':' {
            return Err(NomeRecusado::DoisPontos);
        }
        if matches!(letra, '<' | '>' | '"' | '|' | '?' | '*') {
            return Err(NomeRecusado::ProibidoNoWindows);
        }
        if letra.is_control() {
            return Err(NomeRecusado::Controle);
        }
        if e_formatacao(letra) {
            return Err(NomeRecusado::Formatacao);
        }
    }
    if alegado.ends_with(['.', ' ']) {
        return Err(NomeRecusado::PontoOuEspacoNoFim);
    }
    // O radical é o que vem antes do **primeiro** ponto: `nul.tar.gz` é o NUL.
    // Os espaços no fim do radical saem antes de comparar, porque o Windows os
    // tira ao resolver o nome e `CON .txt` acaba no mesmo dispositivo.
    let radical = alegado
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_uppercase();
    if RESERVADOS.contains(&radical.as_str()) {
        return Err(NomeRecusado::ReservadoNoWindows);
    }
    Ok(alegado)
}

/// O nome com um caractere invisível ou de controle escrito por extenso.
///
/// Para a frase que recusa um nome: a pessoa precisa ver o que veio, e um
/// U+202E citado como está inverteria a própria frase que o denuncia — assim
/// como uma quebra de linha no nome quebraria a frase no meio. O resto do nome
/// fica como veio.
#[must_use]
pub fn para_mostrar(alegado: &str) -> String {
    use std::fmt::Write as _;

    let mut saida = String::with_capacity(alegado.len());
    for letra in alegado.chars() {
        if letra.is_control() || e_formatacao(letra) {
            // Escrever numa `String` não falha; o `Result` é do trait.
            let _ = write!(saida, "\\u{{{:X}}}", u32::from(letra));
        } else {
            saida.push(letra);
        }
    }
    saida
}

/// O nome que o arquivo ganha na `vez`-ésima tentativa: «foto (2).png».
///
/// A primeira vez é o nome como veio. Da segunda em diante, o sufixo entra antes
/// da extensão — o que vem depois do último ponto que **não** é o primeiro
/// caractere, então «.bashrc» vira «.bashrc (2)» e não « (2).bashrc».
///
/// O radical é cortado numa fronteira de caractere para o nome inteiro caber em
/// [`MAX_FILE_NAME_LEN`] bytes: sem o corte, um nome de 255 bytes que colide vira
/// ENAMETOOLONG em vez de um arquivo. Se a extensão e o sufixo sozinhos já
/// ocupam o limite inteiro, o nome todo é tratado como radical e cortado.
#[must_use]
pub fn nome_ao_lado(nome: &str, vez: u32) -> String {
    if vez <= 1 {
        return nome.to_owned();
    }
    let sufixo = format!(" ({vez})");
    let (radical, extensao) = match nome.rfind('.') {
        Some(ponto) if ponto > 0 => nome.split_at(ponto),
        _ => (nome, ""),
    };
    let (radical, extensao) = if extensao.len() + sufixo.len() < MAX_FILE_NAME_LEN {
        (radical, extensao)
    } else {
        (nome, "")
    };
    let cabe = MAX_FILE_NAME_LEN.saturating_sub(sufixo.len() + extensao.len());
    format!("{}{sufixo}{extensao}", ate_caber(radical, cabe))
}

/// O pedaço do começo de `texto` que cabe em `cabe` bytes, cortado numa
/// fronteira de caractere.
pub fn ate_caber(texto: &str, cabe: usize) -> &str {
    let corte = texto
        .char_indices()
        .map(|(inicio, letra)| inicio + letra.len_utf8())
        .take_while(|&fim| fim <= cabe)
        .last()
        .unwrap_or(0);
    texto.get(..corte).unwrap_or_default()
}
