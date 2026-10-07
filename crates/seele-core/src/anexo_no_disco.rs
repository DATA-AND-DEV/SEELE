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
//! # O parcial nunca tem o nome final, e nada que já existe é substituído
//!
//! [`abrir_parcial`] cria o arquivo em que os bytes chegam com um nome de
//! parcial — `.foto.png.seele-parcial` — e com `create_new`, que falha em vez
//! de truncar e falha também diante de um link simbólico que já estava lá, em
//! vez de segui-lo. Só depois de o hash conferir, [`Parcial::nomear`] dá o nome
//! final, com um link físico que também falha se o nome existir — ou, num
//! volume sem link físico, com uma reserva do nome que falha do mesmo jeito: um
//! arquivo da pessoa com o mesmo nome fica onde estava, e o novo ganha o nome ao
//! lado, como «foto (2).png». Um processo que morre no meio do download deixa
//! um parcial — que o ponto na frente esconde no Mac e no Linux, mas não no
//! Explorer do Windows —, e não um «foto.png» truncado com cara de completo.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use seele_proto::control::MAX_FILE_NAME_LEN;

/// Quantos nomes finais o app deixa [`Parcial::nomear`] tentar antes de
/// desistir — é o `tentativas` que ele passa a [`abrir_parcial`] —, e quantos
/// nomes de parcial [`abrir_parcial`] tenta sempre.
///
/// [`Parcial::nomear`] tenta o `tentativas` que [`abrir_parcial`] recebeu, e não
/// esta constante: quem escolhe o caminho inteiro, como a porta da
/// conformidade, passa um, e só o nome exato serve.
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
    format!("{}{sufixo}{extensao}", ate_caber(radical, cabe))
}

/// O fim do nome com que um anexo é gravado enquanto chega.
///
/// O nome inteiro do parcial de «foto.png» é `.foto.png.seele-parcial`, e cada
/// pedaço tem um motivo:
///
/// - o ponto na frente esconde o parcial no Finder e no `ls`, onde ninguém o
///   confunde com o arquivo (no Explorer do Windows quem esconde é um atributo,
///   e não o nome);
/// - o nome que veio, no meio, diz a quem achar o parcial de que anexo ele é;
/// - o sufixo troca a extensão: o parcial de um `x.exe` não é um `.exe`, e um
///   duplo clique nele não executa nada nem abre metade de uma imagem;
/// - «parcial» é a palavra que o servidor já usa para o mesmo papel — ele grava
///   o blob que está chegando com o sufixo `.parcial` —, e «seele» diz qual
///   programa o deixou na pasta da pessoa.
pub const SUFIXO_DO_PARCIAL: &str = ".seele-parcial";

/// O pedaço do começo de `texto` que cabe em `cabe` bytes, cortado numa
/// fronteira de caractere.
fn ate_caber(texto: &str, cabe: usize) -> &str {
    let corte = texto
        .char_indices()
        .map(|(inicio, letra)| inicio + letra.len_utf8())
        .take_while(|&fim| fim <= cabe)
        .last()
        .unwrap_or(0);
    texto.get(..corte).unwrap_or_default()
}

/// O nome do parcial de `nome` na `vez`-ésima tentativa:
/// `.foto.png.seele-parcial`, `.foto.png (2).seele-parcial`…
///
/// O nome que veio é cortado numa fronteira de caractere para o parcial caber
/// em [`MAX_FILE_NAME_LEN`] bytes: o ponto e o sufixo somam quinze, e sem o
/// corte um nome de 255 bytes não conseguiria nem começar a chegar.
fn nome_do_parcial(nome: &str, vez: u32) -> String {
    let vez = if vez <= 1 {
        String::new()
    } else {
        format!(" ({vez})")
    };
    let cabe = MAX_FILE_NAME_LEN.saturating_sub(1 + vez.len() + SUFIXO_DO_PARCIAL.len());
    format!(".{}{vez}{SUFIXO_DO_PARCIAL}", ate_caber(nome, cabe))
}

/// Um nome recusado por [`nome_seguro`], como erro de disco.
fn recusado(recusa: NomeRecusado) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, recusa)
}

/// Um anexo a caminho do disco: gravado com um nome de parcial, e ainda sem o
/// nome que vai ter.
///
/// Sai de [`abrir_parcial`], junto com o arquivo aberto em que os bytes
/// chegam. Quem recebe confere o hash e chama [`Parcial::nomear`], que dá o nome
/// final. **Solto sem nome, ele apaga o parcial**: é o que faz toda falha — o
/// prazo, o disco, o hash, um `?` qualquer no meio, uma tarefa largada antes do
/// fim — levar o parcial embora, sem depender de quem chama lembrar.
/// Só um processo morto não passa por aqui, e o que ele deixa é um arquivo
/// com [`SUFIXO_DO_PARCIAL`] — oculto no Mac e no Linux, à vista no Explorer do
/// Windows —, e não um arquivo com o nome final.
#[derive(Debug)]
#[must_use = "solto sem nome, o parcial é apagado"]
pub struct Parcial {
    /// Onde os bytes estão chegando: `.foto.png.seele-parcial`.
    caminho: PathBuf,
    /// A pasta do nome final, que é a mesma do parcial.
    pasta: PathBuf,
    /// O nome que veio, já conferido por [`nome_seguro`].
    nome: String,
    /// Quantos nomes o final pode tentar: «foto.png», «foto (2).png»…
    tentativas: u32,
    /// Se o nome de parcial ainda está na pasta e tem de sair ao soltar.
    ainda_na_pasta: bool,
}

/// Cria em `pasta` o arquivo em que um anexo vai chegar, com um nome de parcial.
///
/// **O arquivo nunca começa com o nome final.** Ele é criado como
/// `.foto.png.seele-parcial` (ver [`SUFIXO_DO_PARCIAL`]), na mesma pasta, e só
/// ganha o nome que veio — ou um ao lado dele — em [`Parcial::nomear`], depois de
/// o hash conferir. Um processo que morre no meio do download deixa o parcial, e
/// não um «foto.png» truncado com cara de completo.
///
/// O parcial é criado com `create_new`, que falha se o nome já existir —
/// arquivo, pasta ou link simbólico, que ele não segue — e é o sistema de
/// arquivos que responde, não uma conferência feita antes e vencida por uma
/// corrida. Um parcial que já está lá — de outro anexo do mesmo nome chegando
/// agora, ou de um processo que morreu — passa ao nome seguinte, e não é
/// tocado: daqui não se sabe se ele ainda está sendo escrito.
///
/// O nome é conferido aqui com [`nome_seguro`], e não só por quem chama: esta é
/// a última porta antes do disco, e uma regra que dependesse de cada chamador
/// lembrar dela é uma regra que um chamador novo esquece. `tentativas` é quantos
/// nomes finais [`Parcial::nomear`] pode tentar: [`TENTATIVAS`] para o app, que
/// grava ao lado; um para quem escolheu o caminho inteiro e não quer outro.
///
/// # Errors
///
/// `InvalidInput` quando o nome não passa em [`nome_seguro`]; `AlreadyExists`
/// quando os [`TENTATIVAS`] nomes de parcial estão tomados; e o erro do
/// sistema, como veio, em qualquer outro caso.
pub fn abrir_parcial(pasta: &Path, nome: &str, tentativas: u32) -> io::Result<(File, Parcial)> {
    let nome = nome_seguro(nome).map_err(recusado)?;
    for vez in 1..=TENTATIVAS {
        let candidato = nome_do_parcial(nome, vez);
        // O parcial só acrescenta ponto, espaço, parênteses, algarismos e o
        // sufixo a um pedaço de um nome que já passou, então isto não recusa
        // nada que se saiba. Fica porque «não se sabe de nada» não é prova, e o
        // custo é uma passada.
        nome_seguro(&candidato).map_err(recusado)?;
        let caminho = pasta.join(&candidato);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&caminho)
        {
            Ok(arquivo) => {
                let parcial = Parcial {
                    caminho,
                    pasta: pasta.to_owned(),
                    nome: nome.to_owned(),
                    tentativas,
                    ainda_na_pasta: true,
                };
                return Ok((arquivo, parcial));
            }
            // Pular em silêncio seria o produto sabendo de um arquivo na pasta
            // da pessoa e não contando: quem achar um parcial esquecido e abrir
            // o `seele.log` tem de achar esta linha com o caminho dele.
            Err(erro) if erro.kind() == io::ErrorKind::AlreadyExists => tracing::info!(
                caminho = %caminho.display(),
                "já havia na pasta um arquivo com o nome de parcial deste anexo, de outro \
                 anexo do mesmo nome chegando agora ou de um processo que morreu; ele \
                 ficou como estava, e o anexo vai para o nome de parcial seguinte"
            ),
            Err(erro) => return Err(erro),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("os {TENTATIVAS} nomes de parcial de «{nome}» já existem nesta pasta"),
    ))
}

impl Parcial {
    /// Onde os bytes estão chegando.
    #[must_use]
    pub fn caminho(&self) -> &Path {
        &self.caminho
    }

    /// Dá ao parcial o nome final, **sem substituir nada**, e devolve o caminho
    /// real: «foto (2).png», se foi lá que ele ficou.
    ///
    /// Chame só depois de o hash conferir: é este o instante em que o arquivo
    /// passa a ter cara de completo.
    ///
    /// Cada nome, de «foto.png» a «foto (N).png» (N é o `tentativas` de
    /// [`abrir_parcial`]), é tentado com `std::fs::hard_link`, que falha se o
    /// nome já existir — e é o sistema de arquivos que responde, sem corrida
    /// entre olhar e gravar. Depois o nome de parcial sai, e o arquivo fica só
    /// com o final. A quarentena que quem recebe pôs no parcial vai junto: o
    /// link é outro nome para o mesmo arquivo.
    ///
    /// **Num volume sem link físico** — FAT e exFAT, onde o `link` volta ENOTSUP
    /// no macOS (medido) e, pelo `vfs_link` do kernel, EPERM no Linux —, o
    /// recuo reserva o nome final com
    /// `create_new` e troca a reserva pelo parcial com `rename`. Ele também
    /// nunca passa por cima de um arquivo da pessoa: o `rename` só substitui a
    /// reserva vazia que esta mesma chamada acabou de criar. O que ele não tem é
    /// o link num passo só: se outro programa trocasse a reserva por um arquivo
    /// dele no instante entre os dois passos, o `rename` passaria por cima; e um
    /// processo que morra entre os dois deixa uma reserva vazia com o nome
    /// final — vazia, e não truncada. Medido num volume FAT32 e num exFAT
    /// montados no macOS: o `link` recusa, a troca funciona, e a quarentena vai
    /// junto com o arquivo.
    ///
    /// # Errors
    ///
    /// `AlreadyExists` quando os nomes permitidos estão todos tomados, e o erro
    /// do sistema em qualquer outro caso: como veio, ou, quando o recuo rodou e
    /// falhou, com o tipo dele e um texto que diz também que o volume recusou o
    /// link e com que erro. Em todos, o parcial é apagado ao soltar.
    pub fn nomear(self) -> io::Result<PathBuf> {
        self.nomear_com(
            |de, para| std::fs::hard_link(de, para),
            |de, para| std::fs::rename(de, para),
        )
    }

    /// [`Self::nomear`], com o link físico e a troca de nome trocáveis: o teste
    /// recusa o link de propósito para provar o recuo num volume que o
    /// aceitaria, e faz a troca falhar para provar o que sobra dela.
    fn nomear_com(
        mut self,
        ligar: impl Fn(&Path, &Path) -> io::Result<()>,
        trocar: impl Fn(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<PathBuf> {
        let mut sem_link: Option<io::Error> = None;
        for vez in 1..=self.tentativas {
            let candidato = nome_ao_lado(&self.nome, vez);
            nome_seguro(&candidato).map_err(recusado)?;
            let destino = self.pasta.join(&candidato);
            let feito = if sem_link.is_some() {
                reservar_e_trocar(&self.caminho, &destino, &trocar)
            } else {
                match ligar(&self.caminho, &destino) {
                    // O nome final e o de parcial são agora o mesmo arquivo, e
                    // o de parcial sai quando este valor é solto, na volta.
                    Ok(()) => Ok(()),
                    Err(erro) if erro.kind() == io::ErrorKind::AlreadyExists => Err(erro),
                    // Qualquer outro erro é lido como «este volume não faz
                    // link»: o macOS diz ENOTSUP, que o std não classifica, o
                    // Linux diria EPERM, e o recuo também não substitui nada. Um
                    // erro de verdade — sem permissão, disco cheio — volta do
                    // recuo do mesmo jeito.
                    Err(erro) => {
                        sem_link = Some(erro);
                        reservar_e_trocar(&self.caminho, &destino, &trocar)
                    }
                }
            };
            if feito.is_ok() && sem_link.is_some() {
                // A troca levou o nome de parcial junto: não há o que apagar.
                self.ainda_na_pasta = false;
            }
            match feito {
                Ok(()) => {
                    if let Some(erro) = &sem_link {
                        tracing::info!(
                            caminho = %destino.display(),
                            %erro,
                            "o volume não aceitou link físico, e o anexo ganhou o nome final pela reserva e troca"
                        );
                    }
                    return Ok(destino);
                }
                Err(erro) if erro.kind() == io::ErrorKind::AlreadyExists => {}
                Err(erro) => {
                    return Err(match &sem_link {
                        // O erro que volta é o que a linha «o anexo não foi
                        // salvo» leva ao `seele.log`. Só com o da troca, quem
                        // investiga vê um nome que não pôde ser trocado e não
                        // sabe por que houve troca: o link recusado vai junto.
                        // O tipo é o do recuo, que é o erro que impediu o nome.
                        Some(do_link) => io::Error::new(
                            erro.kind(),
                            format!(
                                "o volume não aceitou link físico ({do_link}), e o recuo, \
                                 que reserva o nome final e troca a reserva pelo parcial, \
                                 falhou: {erro}"
                            ),
                        ),
                        None => erro,
                    });
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "os {} nomes permitidos para «{}» já existem nesta pasta",
                self.tentativas, self.nome
            ),
        ))
    }
}

/// O recuo de [`Parcial::nomear`] num volume sem link físico.
///
/// Reserva `destino` com `create_new` — que falha se o nome existir, como o
/// link — e troca a reserva pelo parcial. Se a troca falhar, a reserva, que é
/// desta chamada e está vazia, é apagada, e um apagar que falhe vai para o
/// `seele.log`: é o único jeito de uma falha deixar o nome final na pasta.
fn reservar_e_trocar(
    parcial: &Path,
    destino: &Path,
    trocar: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    drop(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destino)?,
    );
    trocar(parcial, destino)
        .inspect_err(|_| apagar_dizendo(destino, "a reserva vazia com o nome final de um anexo"))
}

/// Apaga `caminho`, e diz no `seele.log` quando não consegue.
///
/// O que não sai fica na pasta da pessoa — o parcial, com o nome de parcial; a
/// reserva do recuo, vazia e com o nome final —, e só quem tentou apagar sabe
/// disso. Um `let _` aqui era o produto sabendo e não contando: a tela dizia
/// que nada tinha ficado, e a pergunta voltava sem dado nenhum.
///
/// `NotFound` não é falha: o que se queria era que o arquivo não estivesse lá,
/// e ele não está.
fn apagar_dizendo(caminho: &Path, o_que: &str) {
    match std::fs::remove_file(caminho) {
        Ok(()) => {}
        Err(erro) if erro.kind() == io::ErrorKind::NotFound => {}
        Err(erro) => tracing::warn!(
            caminho = %caminho.display(),
            %erro,
            "não consegui apagar {o_que}, e o arquivo ficou na pasta"
        ),
    }
}

impl Drop for Parcial {
    fn drop(&mut self) {
        if self.ainda_na_pasta {
            apagar_dizendo(&self.caminho, "o parcial de um anexo");
        }
    }
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
            let (mut arquivo, parcial) =
                abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir um parcial");
            arquivo.write_all(&[vez]).expect("gravar um byte");
            drop(arquivo);
            let caminho = parcial.nomear().expect("dar um nome livre");
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
        let salvar = || {
            let (_, parcial) = abrir_parcial(&dir, &nome, TENTATIVAS)?;
            parcial.nomear()
        };
        let primeiro = salvar().expect("o nome livre");
        let segundo = salvar().expect(
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

    /// Os nomes que estão em `dir` agora, em ordem.
    fn nomes(dir: &Path) -> Vec<String> {
        let mut nomes: Vec<String> = std::fs::read_dir(dir)
            .expect("ler a pasta do teste")
            .flatten()
            .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
            .collect();
        nomes.sort();
        nomes
    }

    #[test]
    fn o_parcial_nunca_tem_o_nome_final_e_so_ganha_o_nome_no_fim() {
        let dir = pasta("parcial");
        let (mut arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir o parcial numa pasta vazia");
        arquivo.write_all(b"metade").expect("gravar no parcial");

        // Enquanto chega, o arquivo não tem o nome final: um processo que morre
        // aqui deixa um parcial, e não um «foto.png» truncado.
        assert!(
            !nomes(&dir).iter().any(|nome| nome == "foto.png"),
            "o arquivo que ainda está chegando já tem o nome final, e um processo \
             que morra agora deixa um «foto.png» truncado com cara de completo: {:?}",
            nomes(&dir)
        );
        let parcial_nome = parcial
            .caminho()
            .file_name()
            .and_then(|nome| nome.to_str())
            .expect("o parcial tem um nome de texto")
            .to_owned();
        assert!(
            parcial_nome.starts_with('.')
                && parcial_nome.ends_with(SUFIXO_DO_PARCIAL)
                && parcial_nome.contains("foto.png"),
            "o parcial não começa pelo ponto que o esconde no Mac e no Linux, não \
             tem o nome que veio ou não tem o sufixo de parcial: «{parcial_nome}»"
        );
        assert_eq!(
            parcial.caminho().parent(),
            Some(dir.as_path()),
            "o parcial não está na mesma pasta do nome final, e o link para o nome \
             final atravessaria volumes"
        );

        arquivo.write_all(b" e o resto").expect("gravar o resto");
        drop(arquivo);
        let final_ = parcial.nomear().expect("dar o nome final ao parcial");
        assert_eq!(
            final_,
            dir.join("foto.png"),
            "o arquivo não ganhou o nome que veio com ele"
        );
        assert_eq!(
            nomes(&dir),
            ["foto.png"],
            "depois de ganhar o nome final, o parcial continua na pasta"
        );
        assert_eq!(
            std::fs::read(&final_).expect("ler o arquivo com o nome final"),
            b"metade e o resto",
            "o arquivo com o nome final não tem os bytes que foram gravados no parcial"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dar_o_nome_final_nunca_substitui_o_que_ja_estava_la() {
        let dir = pasta("nome-final");
        std::fs::write(dir.join("foto.png"), "original").expect("o arquivo da pessoa");

        // Dois parciais do mesmo nome ao mesmo tempo, como dois anexos «foto.png»
        // salvos um logo depois do outro.
        let (mut primeiro, parcial_1) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("o primeiro parcial");
        let (mut segundo, parcial_2) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("o segundo parcial");
        assert_ne!(
            parcial_1.caminho(),
            parcial_2.caminho(),
            "dois anexos do mesmo nome chegando ao mesmo tempo gravam no mesmo parcial"
        );
        primeiro.write_all(b"1").expect("gravar o primeiro");
        segundo.write_all(b"2").expect("gravar o segundo");
        drop((primeiro, segundo));

        let ao_lado = parcial_1.nomear().expect("o primeiro ganha nome");
        let mais_ao_lado = parcial_2.nomear().expect("o segundo ganha nome");
        assert_eq!(
            (ao_lado, mais_ao_lado),
            (dir.join("foto (2).png"), dir.join("foto (3).png")),
            "os anexos não foram gravados ao lado do «foto.png» que já estava na pasta"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("foto.png")).expect("o da pessoa continua lá"),
            "original",
            "dar o nome final a um anexo substituiu o «foto.png» que já estava na pasta"
        );
        assert_eq!(
            nomes(&dir),
            ["foto (2).png", "foto (3).png", "foto.png"],
            "sobrou um parcial na pasta, ou faltou um dos arquivos"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sem_link_fisico_o_recuo_tambem_nao_substitui_nada() {
        // FAT e exFAT não têm link físico: no macOS o `link` deles volta ENOTSUP,
        // o erro 45, que o std não classifica (medido num volume de cada,
        // montado de uma imagem de disco). O recuo tem de dar o nome final sem
        // passar por cima do que já está lá — aqui com o link recusado de
        // propósito, com o mesmo erro, num volume que o aceitaria.
        let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(rastro.clone());
        let dir = pasta("sem-link");
        std::fs::write(dir.join("foto.png"), "original").expect("o arquivo da pessoa");
        let (mut arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir o parcial");
        arquivo.write_all(b"chegou").expect("gravar no parcial");
        drop(arquivo);

        let final_ = parcial
            .nomear_com(
                |_, _| Err(io::Error::from_raw_os_error(45)),
                |de, para| std::fs::rename(de, para),
            )
            .expect(
                "sem link físico, o anexo não ganhou nome nenhum: o recuo não rodou, \
                 ou falhou",
            );
        assert_eq!(
            final_,
            dir.join("foto (2).png"),
            "sem link físico, o anexo não foi gravado ao lado do que já estava na pasta"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("foto.png")).expect("o da pessoa continua lá"),
            "original",
            "sem link físico, dar o nome final substituiu o arquivo que já estava na pasta"
        );
        assert_eq!(
            std::fs::read(&final_).expect("ler o arquivo salvo"),
            b"chegou",
            "sem link físico, o arquivo com o nome final não tem os bytes do parcial"
        );
        assert_eq!(
            nomes(&dir),
            ["foto (2).png", "foto.png"],
            "sem link físico, sobrou um parcial ou uma reserva vazia na pasta"
        );
        let linhas = rastro.linhas();
        assert!(
            linhas
                .iter()
                .any(|linha| linha.contains("link físico") && linha.contains("foto (2).png")),
            "o anexo ganhou o nome pelo recuo e o `seele.log` não diz que o volume \
             recusou o link: quem investigar um salvo num pendrive não sabe que \
             caminho ele fez. Rastro: {linhas:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn um_parcial_que_nao_ganhou_nome_sai_da_pasta() {
        let dir = pasta("descartado");

        // Uma falha no meio do download: o parcial é solto sem nome.
        let (mut arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir o parcial");
        arquivo.write_all(b"metade").expect("gravar no parcial");
        drop(arquivo);
        drop(parcial);
        assert_eq!(
            nomes(&dir),
            Vec::<String>::new(),
            "um anexo que não chegou deixou alguma coisa na pasta"
        );

        // E um nome que não dá para dar: o único permitido já está tomado.
        std::fs::write(dir.join("foto.png"), "original").expect("o arquivo da pessoa");
        let (arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", 1).expect("o parcial abre com o nome final tomado");
        drop(arquivo);
        let erro = parcial
            .nomear()
            .expect_err("o único nome permitido estava tomado e o anexo ganhou nome assim mesmo");
        assert_eq!(
            erro.kind(),
            io::ErrorKind::AlreadyExists,
            "o nome tomado não voltou como já existente: {erro}"
        );
        assert_eq!(
            nomes(&dir),
            ["foto.png"],
            "o anexo que não ganhou nome deixou o parcial na pasta"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("foto.png")).expect("o da pessoa continua lá"),
            "original",
            "o anexo que não ganhou nome mexeu no arquivo que já estava na pasta"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_parcial_de_um_nome_de_255_bytes_cabe_em_255() {
        let nome = format!("a{}.png", "é".repeat(125));
        let dir = pasta("parcial-longo");
        let (arquivo, parcial) = abrir_parcial(&dir, &nome, TENTATIVAS).expect(
            "o parcial de um nome de 255 bytes não abriu: sem o corte, o sufixo de \
             parcial passa do limite e quem recebe não consegue salvar",
        );
        drop(arquivo);
        let parcial_nome = parcial
            .caminho()
            .file_name()
            .and_then(|nome| nome.to_str())
            .expect("o parcial tem um nome de texto")
            .to_owned();
        assert!(
            parcial_nome.len() <= MAX_FILE_NAME_LEN && parcial_nome != nome,
            "o parcial de um nome de 255 bytes tem {} bytes, ou tem o nome final",
            parcial_nome.len()
        );
        assert_eq!(
            parcial.nomear().expect("dar o nome final"),
            dir.join(&nome),
            "o anexo de nome comprido não ganhou o nome que veio com ele"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn um_parcial_que_nao_sai_da_pasta_fica_dito_no_log() {
        // Apagar pode falhar, e o que não sai fica na pasta da pessoa. Antes,
        // o `let _` engolia a falha: a tela dizia «nada foi gravado pela
        // metade», e ninguém sabia do parcial. Para o apagar falhar em qualquer
        // sistema, e também para quem roda como root, o parcial vira uma pasta:
        // `remove_file` não apaga pasta.
        let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::WARN);
        let _guarda = tracing::subscriber::set_default(rastro.clone());
        let dir = pasta("parcial-preso");
        let (arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir o parcial");
        drop(arquivo);
        let preso = parcial.caminho().to_path_buf();
        std::fs::remove_file(&preso).expect("tirar o parcial do lugar");
        std::fs::create_dir(&preso).expect("pôr uma pasta no lugar do parcial");

        drop(parcial);
        assert!(
            preso.exists(),
            "o parcial preso saiu, e este teste não fez o apagar falhar"
        );
        let linhas = rastro.linhas();
        let caminho = preso.display().to_string();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("WARN")
                && linha.contains(&caminho)
                && linha.contains("erro=")),
            "o parcial de um anexo não saiu da pasta e o `seele.log` não diz qual \
             nem por quê: a pessoa fica com um arquivo que ninguém explica. \
             Rastro: {linhas:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uma_reserva_que_nao_sai_da_pasta_fica_dita_no_log() {
        // A reserva do recuo tem o nome final. Se a troca falha e a reserva não
        // sai, fica na pasta um arquivo vazio com o nome do anexo — o único jeito
        // de o nome final sobrar de uma falha —, e o `seele.log` tem de dizer.
        let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::WARN);
        let _guarda = tracing::subscriber::set_default(rastro.clone());
        let dir = pasta("reserva-presa");
        let (arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir o parcial");
        drop(arquivo);

        let erro = parcial
            .nomear_com(
                |_, _| Err(io::Error::from_raw_os_error(45)),
                |_, reserva| {
                    // A troca falha, e a reserva vira uma pasta para que o
                    // apagar dela falhe também.
                    std::fs::remove_file(reserva)?;
                    std::fs::create_dir(reserva)?;
                    Err(io::Error::other("a troca foi recusada pelo teste"))
                },
            )
            .expect_err("a troca recusada deu nome ao anexo assim mesmo");
        assert!(
            erro.to_string().contains("a troca foi recusada pelo teste"),
            "o erro da troca não voltou a quem chamou: {erro}"
        );
        let reserva = dir.join("foto.png");
        assert!(
            reserva.exists(),
            "a reserva presa saiu, e este teste não fez o apagar falhar"
        );
        let linhas = rastro.linhas();
        let caminho = reserva.display().to_string();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("WARN")
                && linha.contains(&format!("caminho={caminho} "))
                && linha.contains("erro=")),
            "a reserva com o nome final não saiu da pasta e o `seele.log` não diz \
             qual nem por quê: a pessoa acha um «foto.png» vazio que ninguém \
             explica. Rastro: {linhas:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn um_recuo_que_falha_diz_que_o_volume_recusou_o_link() {
        // Num FAT ou exFAT em que a troca falha, o erro que volta daqui é o que
        // a linha «o anexo não foi salvo» do `seele.log` leva. Só com o erro da
        // troca, quem investiga vê um nome que não pôde ser trocado e não sabe
        // por que houve troca: o link recusado, que fez o recuo rodar, tem de
        // ir junto.
        let dir = pasta("recuo-que-falha");
        let (arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir o parcial");
        drop(arquivo);
        let do_link = io::Error::from_raw_os_error(45).to_string();

        let erro = parcial
            .nomear_com(
                |_, _| Err(io::Error::from_raw_os_error(45)),
                |_, _| {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "a troca foi recusada pelo teste",
                    ))
                },
            )
            .expect_err("a troca recusada deu nome ao anexo assim mesmo");
        let texto = erro.to_string();
        assert!(
            texto.contains("a troca foi recusada pelo teste"),
            "o erro da troca, que é o que impediu o nome final, não voltou a quem \
             chamou: {texto}"
        );
        assert!(
            texto.contains("link físico") && texto.contains(&do_link),
            "o recuo falhou e o erro que volta não diz que o volume recusou o link \
             nem com que erro («{do_link}»): a linha «o anexo não foi salvo» do \
             `seele.log` mostra uma troca de nome que falhou sem dizer por que \
             houve troca. Erro: {texto}"
        );
        assert_eq!(
            erro.kind(),
            io::ErrorKind::PermissionDenied,
            "o erro do recuo voltou com outro tipo, e quem lê o tipo deixa de ver \
             o erro do sistema que impediu o nome final"
        );
        assert_eq!(
            nomes(&dir),
            Vec::<String>::new(),
            "o recuo que falhou deixou o parcial ou a reserva na pasta"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn um_parcial_que_ja_estava_na_pasta_fica_como_estava_e_e_dito_no_log() {
        // Um parcial com o nome que este anexo tomaria já está na pasta — de
        // outro anexo do mesmo nome chegando agora, ou de um processo que
        // morreu. Ele não é tocado, porque daqui não se sabe se ainda está sendo
        // escrito, e o anexo vai para o nome seguinte. Mas o produto sabe que ele
        // está lá, e quem achar o arquivo e abrir o `seele.log` tem de achar a
        // linha com o caminho dele.
        let rastro = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(rastro.clone());
        let dir = pasta("parcial-que-ja-estava");
        let ja_estava = dir.join(format!(".foto.png{SUFIXO_DO_PARCIAL}"));
        std::fs::write(&ja_estava, "de antes").expect("o parcial que já estava");

        let (arquivo, parcial) =
            abrir_parcial(&dir, "foto.png", TENTATIVAS).expect("abrir o parcial");
        drop(arquivo);
        assert_eq!(
            parcial.caminho(),
            dir.join(format!(".foto.png (2){SUFIXO_DO_PARCIAL}")),
            "o anexo não foi para o nome de parcial seguinte ao que já estava na pasta"
        );
        assert_eq!(
            std::fs::read_to_string(&ja_estava).expect("o parcial de antes continua lá"),
            "de antes",
            "abrir um parcial mexeu no parcial que já estava na pasta"
        );
        let linhas = rastro.linhas();
        let caminho = ja_estava.display().to_string();
        assert!(
            linhas
                .iter()
                .any(|linha| linha.starts_with("INFO") && linha.contains(&caminho)),
            "um parcial que já estava na pasta foi pulado e o `seele.log` não diz \
             qual: quem achar um «.foto.png.seele-parcial» esquecido não acha \
             nenhuma linha sobre ele. Rastro: {linhas:?}"
        );
        drop(parcial);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
