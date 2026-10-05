//! Onde um anexo recebido vai parar no disco, e com que nome.
//!
//! # O nome é de quem mandou, e quem decide é quem recebe
//!
//! O fio leva quase qualquer nome — `seele_proto::attachment` só recusa o que é
//! vazio ou só espaço, o que tem NUL e o que passa de [`MAX_FILE_NAME_LEN`]
//! bytes, de propósito: do lado do servidor o nome é uma coluna, e o blob se
//! chama pelo hash do próprio conteúdo. **Do lado de quem recebe, o nome vira
//! caminho**, e é aqui que ele é conferido antes disso. Um nome com `../`
//! gravaria fora da pasta; `CON` abriria um dispositivo no Windows; um U+202E
//! faria «foto\u{202E}gnp.exe» aparecer na confirmação como «fotoexe.png».
//!
//! # No nível do texto, e não do `Path`
//!
//! [`nome_seguro`] nunca usa `Path::components` nem `Path::file_name`. Os dois
//! respondem com a regra do sistema em que o código roda — no Mac, `\` não é
//! separador e `C:` não é unidade —, e um anexo que chega no Mac pode ser salvo
//! no Windows por outra pessoa com o mesmo binário. Lendo o texto, a regra é uma
//! só, e um teste que roda no Mac prova o caso do Windows.
//!
//! # Nada que já existe é substituído
//!
//! [`abrir_sem_sobrescrever`] cria com `create_new`, que falha em vez de truncar
//! — e falha também diante de um link simbólico que já estava lá, em vez de
//! segui-lo. Um arquivo da pessoa com o mesmo nome fica onde estava, e o novo
//! grava ao lado, como «foto (2).png».

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use seele_proto::control::MAX_FILE_NAME_LEN;

/// Quantos nomes [`abrir_sem_sobrescrever`] tenta antes de desistir.
///
/// «foto.png», «foto (2).png»… até «foto (99).png». Uma pasta com noventa e nove
/// arquivos do mesmo nome é uma pasta em que mais um não ajuda ninguém a achar
/// nada, e desistir com erro é melhor que procurar para sempre.
pub const TENTATIVAS: u32 = 99;

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
    let corte = radical
        .char_indices()
        .map(|(inicio, letra)| inicio + letra.len_utf8())
        .take_while(|&fim| fim <= cabe)
        .last()
        .unwrap_or(0);
    format!(
        "{}{sufixo}{extensao}",
        radical.get(..corte).unwrap_or_default()
    )
}

/// Cria um arquivo novo em `pasta`, com o nome alegado ou ao lado dele.
///
/// **Nunca substitui nada.** Cada tentativa usa `create_new`, que falha se o
/// nome já existir — arquivo, pasta ou link simbólico, que ele não segue — e é o
/// sistema de arquivos que responde, não uma conferência feita antes e vencida
/// por uma corrida. `AlreadyExists` passa ao nome seguinte, de «foto.png» a
/// «foto (99).png»; qualquer outro erro volta como veio.
///
/// O nome é conferido aqui de novo, com [`nome_seguro`], e não só por quem
/// chama: esta é a última porta antes do disco, e uma regra que dependesse de
/// cada chamador lembrar dela é uma regra que um chamador novo esquece.
///
/// Devolve o arquivo aberto **e o caminho real**, que é o que a tela tem de
/// mostrar depois: «foto (2).png», se foi lá que ele ficou.
///
/// # Errors
///
/// `InvalidInput` quando o nome não passa em [`nome_seguro`]; `AlreadyExists`
/// quando os [`TENTATIVAS`] nomes estão tomados; e o erro do sistema, como veio,
/// em qualquer outro caso.
pub fn abrir_sem_sobrescrever(pasta: &Path, nome: &str) -> io::Result<(File, PathBuf)> {
    let nome =
        nome_seguro(nome).map_err(|recusa| io::Error::new(io::ErrorKind::InvalidInput, recusa))?;
    for vez in 1..=TENTATIVAS {
        let candidato = nome_ao_lado(nome, vez);
        // O sufixo só acrescenta espaço, parênteses e algarismos a pedaços de um
        // nome que já passou, então isto não recusa nada que se saiba. Fica
        // porque «não se sabe de nada» não é prova, e o custo é uma passada.
        nome_seguro(&candidato)
            .map_err(|recusa| io::Error::new(io::ErrorKind::InvalidInput, recusa))?;
        let caminho = pasta.join(&candidato);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&caminho)
        {
            Ok(arquivo) => return Ok((arquivo, caminho)),
            Err(erro) if erro.kind() == io::ErrorKind::AlreadyExists => {}
            Err(erro) => return Err(erro),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("os {TENTATIVAS} nomes ao lado de «{nome}» já existem nesta pasta"),
    ))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::Write as _;

    /// Uma pasta só deste teste, vazia.
    fn pasta(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "seele-anexo-no-disco-{nome}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("criar a pasta temporária");
        dir
    }

    #[test]
    fn um_nome_alegado_so_vira_nome_de_arquivo_se_for_so_um_nome() {
        for hostil in [
            "../.zshrc",
            "..\\..\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\x.bat",
            "/etc/passwd",
            "C:\\x.bat",
            "C:x.bat",
            "a.txt:Zone.Identifier",
            "..",
            ".",
            "CON",
            "nul.txt",
            "x.bat.",
            "x.bat ",
            "a\u{0}b",
            "a\nb",
            "a?b",
            "a<b",
            "foto\u{202E}gnp.exe",
            // Os que a lista da regra acrescenta: o sobrescrito que o Windows lê
            // como `COM1`, o espaço que ele tira do radical, e o espaço de
            // largura zero.
            "COM¹.txt",
            "CON .txt",
            "a\u{200B}.png",
            // O zero de COM e LPT, que a lista da Microsoft acrescentou, e os dois
            // nomes do console: sozinhos, com extensão e em minúsculas, porque
            // a regra compara o radical sem caixa.
            "COM0",
            "lpt0",
            "com0.txt",
            "LPT0.tar.gz",
            "CONIN$",
            "CONOUT$",
            "conin$.txt",
            "Conout$.log",
        ] {
            assert!(
                nome_seguro(hostil).is_err(),
                "{hostil:?} passou como nome de arquivo, e não é um nome que sirva \
                 no Windows, no Mac e no Linux ao mesmo tempo"
            );
        }
        // E a regra dos reservados olha o radical inteiro, não o começo dele.
        for nome in [
            "foto.png",
            "relatório final.pdf",
            ".bashrc",
            "console.log",
            "com10.txt",
            "conin.txt",
            "lpt00.txt",
        ] {
            assert_eq!(
                nome_seguro(nome),
                Ok(nome),
                "{nome:?} é só um nome e foi recusado, e quem recebeu não consegue \
                 salvar um arquivo comum"
            );
        }
    }

    #[test]
    fn um_nome_repetido_grava_ao_lado_e_nunca_por_cima() {
        let dir = pasta("ao-lado");
        let mut nomes = Vec::new();
        for vez in 0..4_u8 {
            let (mut arquivo, caminho) =
                abrir_sem_sobrescrever(&dir, "foto.png").expect("abrir um nome livre");
            arquivo.write_all(&[vez]).expect("gravar um byte");
            nomes.push(
                caminho
                    .strip_prefix(&dir)
                    .expect("o arquivo ficou dentro da pasta")
                    .display()
                    .to_string(),
            );
        }
        assert_eq!(
            nomes,
            ["foto.png", "foto (2).png", "foto (3).png", "foto (4).png"],
            "três colisões seguidas não deram « (2)», « (3)» e « (4)»"
        );
        assert_eq!(
            std::fs::read(dir.join("foto.png")).expect("o primeiro continua lá"),
            [0],
            "o primeiro «foto.png» foi substituído pelo que chegou depois"
        );
        // A extensão é o que vem depois do último ponto que não é o primeiro
        // caractere: um arquivo oculto não tem extensão, tem nome.
        assert_eq!(
            nome_ao_lado(".bashrc", 2),
            ".bashrc (2)",
            "o sufixo entrou no meio do nome de um arquivo oculto"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_nome_recusado_e_mostrado_com_o_invisivel_por_extenso() {
        assert_eq!(
            para_mostrar("foto\u{202E}gnp.exe"),
            "foto\\u{202E}gnp.exe",
            "a frase que recusa o nome citaria o U+202E como está, e ele inverteria \
             a própria frase"
        );
        assert_eq!(
            para_mostrar("a\nb"),
            "a\\u{A}b",
            "uma quebra de linha no nome quebraria a frase que o recusa"
        );
        assert_eq!(
            para_mostrar("../.zshrc"),
            "../.zshrc",
            "o que é visível no nome foi reescrito, e a pessoa não reconhece o que veio"
        );
    }

    #[test]
    fn um_nome_de_255_bytes_que_colide_continua_cabendo_em_255() {
        // «é» tem dois bytes: o corte do radical tem de cair numa fronteira de
        // caractere, e um radical só de «é» depois de um «a» é o que obriga.
        let nome = format!("a{}.png", "é".repeat(125));
        assert_eq!(
            nome.len(),
            MAX_FILE_NAME_LEN,
            "o nome do teste não tem 255 bytes"
        );
        assert_eq!(
            nome_seguro(&nome),
            Ok(nome.as_str()),
            "um nome de exatamente 255 bytes foi recusado, e o fio aceita até 255"
        );

        let dir = pasta("longo");
        let (_, primeiro) = abrir_sem_sobrescrever(&dir, &nome).expect("o nome livre");
        let (_, segundo) = abrir_sem_sobrescrever(&dir, &nome).expect(
            "o nome ao lado de um nome de 255 bytes não abriu: sem o corte do radical, \
             ele passa do limite e quem recebe não consegue salvar",
        );
        assert_ne!(primeiro, segundo, "a colisão gravou no mesmo nome");
        let ao_lado = segundo
            .strip_prefix(&dir)
            .expect("o arquivo ficou dentro da pasta")
            .to_str()
            .expect("o nome continua sendo texto")
            .to_owned();
        assert!(
            ao_lado.len() <= MAX_FILE_NAME_LEN,
            "o nome ao lado tem {} bytes e passa de {MAX_FILE_NAME_LEN}: no disco, \
             isso é ENAMETOOLONG em vez de um arquivo",
            ao_lado.len()
        );
        assert!(
            ao_lado.ends_with(" (2).png"),
            "o nome ao lado perdeu o sufixo ou a extensão: {ao_lado}"
        );
        let _ = std::fs::remove_dir_all(&dir);

        // E quando a «extensão» sozinha já ocupa quase tudo, o nome inteiro vira
        // radical: o sufixo não cabe ao lado dela, e o nome ao lado tem de caber
        // assim mesmo.
        let extensao_enorme = format!("a.{}", "x".repeat(253));
        let ao_lado = nome_ao_lado(&extensao_enorme, 2);
        assert!(
            ao_lado.len() <= MAX_FILE_NAME_LEN && nome_seguro(&ao_lado).is_ok(),
            "o nome ao lado de um nome com extensão enorme não cabe em \
             {MAX_FILE_NAME_LEN} bytes ou não é mais só um nome: {} bytes",
            ao_lado.len()
        );
    }
}
