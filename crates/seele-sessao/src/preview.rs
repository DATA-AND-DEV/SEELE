//! What a file has to prove before a SEELE window draws it. ADR 0027.
//!
//! The ADR wrote one sentence about this and left it unbuilt: *only a short
//! list of image types is ever drawn inline, and only when the bytes agree with
//! the claim.* This module is that sentence, executable.
//!
//! # The claim is text somebody else chose
//!
//! A file arrives with a name and a declared type, and **both were written by
//! whoever sent it**. Drawing a picture because the name ends in `.png` is
//! trusting a stranger's spelling with the choice of which decoder reads their
//! bytes. So the decision is taken here, from the first twelve bytes, and the
//! media type this module hands to the window is written from **what was
//! found**, never from what was claimed. [`Verdict::Draw`] carries an
//! [`ImageFormat`], not a string off the wire.
//!
//! # Both halves have to agree
//!
//! Sniffing alone would make the name decoration; the claim alone would be
//! trusting the sender. A picture is drawn only when the two say the same
//! thing, and when they disagree that disagreement is [`Verdict::Disagrees`] —
//! its own outcome, with its own sentence. It is not a transfer error: the hash
//! already answered "did it arrive whole", and this answers "is it what it says
//! it is". `NOTAS-DE-RELEASE.md` keeps those two questions apart, and so does
//! this.
//!
//! Not drawing is also not hiding. The file stays on the screen with its name,
//! its size and its save button; what it loses is the picture.
//!
//! # Which formats, and why each one
//!
//! Four, and every one of them is a decoder that will read a stranger's bytes.
//! The WebView's engine does that work, but choosing to hand it a format is a
//! decision, not an accident, so each is named with its reason:
//!
//! - **PNG** and **JPEG** — what a screenshot and a camera produce. Without
//!   these two the feature does not exist.
//! - **GIF** — what people actually paste into a conversation.
//! - **WebP** — what a browser's "save image" now writes. Leaving it out would
//!   make "saved it off the web and sent it" the one thing that does not draw.
//!
//! And what stays out, because a list nobody argued for grows:
//!
//! - **SVG** is markup, not an image: it goes to the same parser as the page,
//!   it can carry script, and it has no signature to sniff because it is text.
//! - **PDF** is a document with an interpreter behind it. The ADR's whole point
//!   about which decoder the bytes go to applies hardest here.
//! - **HEIC** and **AVIF** are sniffed by brand inside an ISO-BMFF box that
//!   `mp4` shares, so telling a picture from a video container means trusting a
//!   four-byte brand string; and WebView support for them differs across the
//!   three targets, which would make the same file draw on macOS and not on
//!   Linux. One product, three systems.
//! - **BMP**, **ICO** and **TIFF** have signatures two bytes long or shorter —
//!   `BM` is the start of plenty of text files — and nobody sends them.

/// A tabela dos quatro formatos, que agora mora no crate do protocolo.
///
/// **Ela desceu, e o julgamento ficou** — ADR 0048. O servidor passou a
/// conferir os bytes de um volume de MOD com a mesma tabela, e não enxerga este
/// crate: o ADR 0002 proíbe o daemon de ligar o cliente. A escolha era uma
/// segunda cópia da assinatura do lado de lá, ou uma cópia só, no crate que os
/// dois enxergam.
///
/// Reexportado daqui porque é daqui que este produto sempre perguntou, e mudar
/// o caminho de quem pergunta não era o ponto da mudança.
pub use seele_proto::imagem::{sniff, ImageFormat, SNIFF_LEN};

/// How many bytes a window will pull down to look at a file.
///
/// **Decided apart from the server's per-file limit, and it has to be.** That
/// one is a fraction of a ceiling whoever hosts chose, and it protects *their*
/// disk: at the default it is 64 MiB. This one protects the machine of whoever
/// is reading, which is a different machine and a different resource — a
/// 64 MiB image decoded whole is measured in gigabytes of pixels, and the
/// window that tried would stop answering.
///
/// So it is a constant in the client and not a number the host sends. What a
/// window spends on memory is not the host's to decide.
///
/// Four mebibytes is above every photograph a phone produces and every
/// screenshot a laptop takes, which is what a preview is for.
///
/// What it bounds and what it does not, said rather than assumed: it bounds the
/// download and the bytes held in the window. It does **not** bound the decoded
/// pixels — that would need each format's header read for its dimensions, which
/// is four more parsers and is not built. The drawn size is capped in CSS; the
/// decode is not.
pub const PREVIEW_LIMIT: u64 = 4 * 1024 * 1024;

/// What a sender's declared type claims the file is.
///
/// Only the four are recognised. Anything else — `application/pdf`,
/// `application/octet-stream`, a type nobody has heard of — is `None`, and a
/// file whose claim is `None` is never drawn even if its bytes are a perfectly
/// good picture. That is the "agree" in "the bytes agree with the claim", and
/// it cuts both ways on purpose.
#[must_use]
pub fn claimed(declared_type: &str) -> Option<ImageFormat> {
    // Case-folded because a media type is case-insensitive and a sender writing
    // `IMAGE/PNG` is not lying about anything.
    match declared_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::Webp),
        _ => None,
    }
}

/// What a window may do with one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Draw it, in this format — which was read from the bytes.
    Draw(ImageFormat),
    /// The file is not what it said it was.
    ///
    /// Its own outcome and not an error, because it deserves its own sentence:
    /// nothing went wrong on the way, and the thing that arrived presented
    /// itself as one kind of file and is another.
    Disagrees {
        /// What the sender said it was.
        claimed: ImageFormat,
        /// What the leading bytes are, when they are one of the four at all.
        /// `None` means they are not a picture this product knows.
        found: Option<ImageFormat>,
    },
    /// Never a picture in the first place: a PDF, a build, a text file.
    ///
    /// Nothing is said about these, because nothing surprising happened.
    NotAPicture,
}

/// Reads the claim and the bytes, and says which of the three this is.
///
/// The order matters. The claim is checked first only to decide whether a
/// picture was ever on offer; it never chooses a decoder, and it never survives
/// into what the window is handed.
#[must_use]
pub fn judge(declared_type: &str, bytes: &[u8]) -> Verdict {
    let Some(claimed) = claimed(declared_type) else {
        return Verdict::NotAPicture;
    };
    let found = sniff(bytes);
    if found == Some(claimed) {
        // Both halves said the same thing, and the format handed on is the one
        // that came out of `sniff`.
        match found {
            Some(format) => Verdict::Draw(format),
            None => Verdict::Disagrees {
                claimed,
                found: None,
            },
        }
    } else {
        Verdict::Disagrees { claimed, found }
    }
}

/// The whole picture as one `data:` URI, ready for an `<img>`.
///
/// Built here, whole, rather than handing a window the bytes and a media type
/// to join up: a page that composes the URI is a page that could compose it
/// from the sender's claim, and the point of everything above is that it
/// cannot. The `format` argument comes out of [`judge`].
///
/// `data:` and not a URL because the Content Security Policy of this app is
/// `default-src 'self'` and **does not move** — it already permits `data:` for
/// images, and no picture is worth an entry in it.
#[must_use]
pub fn data_uri(format: ImageFormat, bytes: &[u8]) -> String {
    let mut uri = String::with_capacity(bytes.len().div_ceil(3) * 4 + 32);
    uri.push_str("data:");
    uri.push_str(format.media_type());
    uri.push_str(";base64,");
    encode_base64(bytes, &mut uri);
    uri
}

/// O mesmo codificador, para `mods::ler_midia`.
///
/// A mídia de um MOD atravessa como `data:` pela mesma razão que a prévia de
/// um anexo, e escrever um segundo codificador ali seria um segundo alfabeto
/// para o mesmo RFC — a forma de defeito que `base64_de` já nomeia.
pub fn encode_base64_publico(bytes: &[u8], out: &mut String) {
    encode_base64(bytes, out);
}

/// The sixty-four, in order.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64, written out rather than depended on.
///
/// The account, in the shape ADR 0026 and ADR 0027 both used: the `base64`
/// crate would be one crate in the tree — two, in fact, since two versions of
/// it are already there transitively and neither is ours to reach for — in
/// exchange for the twenty channels below. This encodes and does not decode, which
/// is the whole of what is needed, and it is exercised by its own test against
/// the vectors in RFC 4648. A crate does not pay for itself at that size.
fn encode_base64(bytes: &[u8], out: &mut String) {
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk.first().copied().unwrap_or(0));
        let second = u32::from(chunk.get(1).copied().unwrap_or(0));
        let third = u32::from(chunk.get(2).copied().unwrap_or(0));
        let packed = (first << 16) | (second << 8) | third;

        out.push(symbol(packed >> 18));
        out.push(symbol(packed >> 12));
        match chunk.len() {
            1 => out.push_str("=="),
            2 => {
                out.push(symbol(packed >> 6));
                out.push('=');
            }
            _ => {
                out.push(symbol(packed >> 6));
                out.push(symbol(packed));
            }
        }
    }
}

/// One six-bit group as its character.
fn symbol(bits: u32) -> char {
    let index = usize::try_from(bits & 0x3F).unwrap_or(0);
    char::from(ALPHABET.get(index).copied().unwrap_or(b'A'))
}

/// Why a picture will not do as a server's icon.
///
/// Two outcomes and not one, because they need different sentences and have
/// different next steps: a photograph that is too heavy can be shrunk, and a
/// PDF cannot be made into an icon at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconRefusal {
    /// Not a PNG, or a PNG declaring a picture larger than the accepted square.
    ///
    /// The two are one outcome on purpose: from where the person stands they
    /// are the same act — «this file is not the kind of picture that goes
    /// here» — and splitting them would ask a shell to explain a chunk layout
    /// to somebody who chose a photo of a cat.
    NotAnIcon,
    /// A PNG, and heavier than a server accepts.
    TooBig {
        /// The ceiling, in bytes, so the sentence can carry the number.
        limit_bytes: u64,
    },
}

/// Whether these bytes may be a server's icon, asked before anything is sent.
///
/// `None` — taking the picture down — always passes.
///
/// # Why a shell asks at all, when the server refuses anyway
///
/// Because of *how* it would refuse. The rule lives on the wire, in
/// `seele_proto::control::check_server_icon`, and a picture that fails it makes
/// the frame unbuildable — which reaches `seele_core::enlace` as a failed send,
/// and a failed send is how a dropped connection looks from there. Somebody
/// choosing the wrong file would watch the app start its five-minute internal
/// battery. So the answer is taken here, **with the same function**, before a
/// command is queued.
///
/// # Errors
///
/// [`IconRefusal`], which is the sentence a shell writes.
pub fn check_server_icon(icon: Option<&[u8]>) -> Result<(), IconRefusal> {
    match seele_proto::control::check_server_icon(icon) {
        Ok(()) => Ok(()),
        Err(seele_proto::ControlError::FieldTooLong { limit, .. }) => Err(IconRefusal::TooBig {
            limit_bytes: u64::try_from(limit).unwrap_or(u64::MAX),
        }),
        Err(_) => Err(IconRefusal::NotAnIcon),
    }
}

// ------------------------------------------------- a prévia de um link
//
// As regras da prévia de um link que não buscam nada: aonde ela não pode ir, e
// que mídia uma página de GIF declara. Moravam no `main.rs` do app; vieram para
// cá na Tarefa 2.6 da F4, porque o navegador as aplica igual antes do `fetch`
// dele. Os testes delas continuam no app, junto dos casos de busca.

/// Se este endereço IP é de um lugar que uma prévia **não** pode alcançar.
///
/// ADR 0053. A lista é do que é reservado, e não do que é «interno»: loopback,
/// privado, link-local, `0.0.0.0/8`, compartilhado entre operadoras, multicast e
/// o resto do que não é internet pública. Em IPv6 o mesmo, mais os endereços
/// mapeados de IPv4 — sem eles, `http://[::ffff:127.0.0.1]/` atravessaria.
///
/// # Por que ela existe
///
/// Porque desenhar um link é buscá-lo, e quem escreve a mensagem escolhia o
/// destino. `http://127.0.0.1:9999/private.png` numa conversa fazia esta janela
/// bater na porta de um serviço da própria máquina de quem lê — e
/// `http://192.168.0.1/` na do roteador da casa dele. Não demonstrei extração de
/// conteúdo; a requisição sozinha já é o que não pode acontecer.
pub fn e_endereco_reservado(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;

    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || v4.is_unspecified()
                // `0.0.0.0/8`, que em várias pilhas é «esta máquina».
                || a == 0
                // `100.64.0.0/10`, compartilhado entre operadoras.
                || (a == 100 && (64..128).contains(&b))
                // `192.0.0.0/24`, atribuições de protocolo do IETF.
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0)
                // `198.18.0.0/15`, bancada de testes.
                || (a == 198 && (18..20).contains(&b))
                // `240.0.0.0/4` em diante, reservado.
                || a >= 240
        }
        IpAddr::V6(v6) => {
            // Um IPv4 mapeado é um IPv4, e tem de passar pela régua dele: sem
            // isto, `::ffff:127.0.0.1` é um IPv6 «qualquer» e atravessa.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return e_endereco_reservado(IpAddr::V4(v4));
            }
            let primeiro = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // `fc00::/7`, endereços locais únicos.
                || (primeiro & 0xfe00) == 0xfc00
                // `fe80::/10`, link-local.
                || (primeiro & 0xffc0) == 0xfe80
                // `2001:db8::/32`, documentação.
                || (primeiro == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

/// Se este host, **como está escrito**, pode ser buscado por uma prévia.
///
/// Duas perguntas numa função porque as duas são sobre o texto do host e não
/// sobre a resolução dele:
///
///   - um endereço literal não pode ser reservado — `http://127.0.0.1/x.png` e
///     `http://[::ffff:127.0.0.1]/x.png` são os dois casos, e o segundo é o que
///     uma régua só de IPv4 deixa passar;
///   - um nome não pode ser dos que por definição não são da internet pública.
///     Conferido aqui **além** da conferência sobre os endereços resolvidos,
///     porque um resolvedor local pode devolver um endereço público para
///     `algo.local` e a conferência de baixo o aceitaria.
pub fn host_de_previa(host: &str) -> bool {
    // Um IPv6 numa URL vem entre colchetes, e `parse` não os quer.
    let literal = host
        .strip_prefix('[')
        .and_then(|resto| resto.strip_suffix(']'));
    if let Ok(ip) = literal.unwrap_or(host).parse::<std::net::IpAddr>() {
        return !e_endereco_reservado(ip);
    }
    let nome = host.to_ascii_lowercase();
    // O ponto final de um nome absoluto não muda o nome: `localhost.` é
    // `localhost`, e sem isto ele atravessaria.
    let nome = nome.strip_suffix('.').unwrap_or(&nome);
    !(nome == "localhost"
        || nome.ends_with(".localhost")
        || nome.ends_with(".local")
        || nome.ends_with(".internal")
        || nome.ends_with(".home.arpa"))
}

/// O endereço da mídia que uma página declara para quem a compartilha.
///
/// `og:image` é o que Tenor, Giphy e Imgur escrevem para que a prévia apareça
/// em qualquer lugar onde o link seja colado. Ler isso é usar o que a página
/// publicou para este fim exato, e não raspar o HTML atrás de um `<img>`:
/// aquele é o endereço que o site escolheu mostrar, e ele não muda quando o
/// desenho da página muda.
///
/// `twitter:image` entra como recuo porque parte das páginas antigas do Imgur
/// só trazem esse.
///
/// **Não resolve endereço relativo.** Um `og:image` relativo teria de ser
/// juntado à base da página, e os três escrevem absoluto; devolver o relativo
/// faria a conferência de domínio recusá-lo, que é o desfecho certo para o que
/// este código não entende.
pub fn midia_declarada_pela_pagina(html: &str) -> Option<String> {
    for etiqueta in etiquetas_meta(html) {
        let atributos = atributos_de(etiqueta);
        let acha = |nome: &str| {
            atributos
                .iter()
                .find(|(chave, _)| chave == nome)
                .map(|(_, valor)| valor.clone())
        };
        let Some(qual) = acha("property").or_else(|| acha("name")) else {
            continue;
        };
        if !matches!(
            qual.trim().to_ascii_lowercase().as_str(),
            "og:image" | "og:image:secure_url" | "twitter:image"
        ) {
            continue;
        }
        let Some(conteudo) = acha("content") else {
            continue;
        };
        // `&amp;` e mais nada: um endereço de mídia carrega parâmetros de
        // consulta, e é essa a entidade que um gerador de HTML escreve neles.
        let conteudo = conteudo.replace("&amp;", "&");
        let conteudo = conteudo.trim();
        if !conteudo.is_empty() {
            return Some(conteudo.to_owned());
        }
    }
    None
}

/// Os trechos entre `<meta` e o `>` que o fecha.
///
/// A procura é na cópia em minúsculas e o corte é no original: `<META` e
/// `<meta` são a mesma etiqueta, e `to_ascii_lowercase` não muda o tamanho em
/// bytes de nada — então um deslocamento achado numa vale na outra.
fn etiquetas_meta(html: &str) -> Vec<&str> {
    let minusculo = html.to_ascii_lowercase();
    minusculo
        .match_indices("<meta")
        .filter_map(|(inicio, _)| {
            let resto = html.get(inicio + "<meta".len()..)?;
            resto.get(..resto.find('>')?)
        })
        .collect()
}

/// Os pares `nome="valor"` de uma etiqueta, com o nome em minúsculas.
///
/// Escrito à mão e não com um analisador de HTML: a árvore deste app diz «nada
/// além disto» sobre dependências, e o que se lê aqui são atributos de uma
/// etiqueta que não aninha nada. O que ele **não** faz — entidades além de
/// `&amp;`, atributo sem valor com significado, HTML malformado — só o faz
/// devolver menos, e devolver menos é a página não ter prévia.
fn atributos_de(etiqueta: &str) -> Vec<(String, String)> {
    /// Onde o varredor está dentro da etiqueta.
    enum Onde {
        /// Juntando letras de um nome.
        Nome,
        /// O nome acabou num espaço, e o `=` ainda pode vir.
        AntesDoIgual,
        /// O `=` veio; o valor começa no próximo que não for espaço.
        AntesDoValor,
        /// Dentro do valor. `Some` são as aspas que o fecham; `None`, o espaço.
        Valor(Option<char>),
    }

    let mut saida: Vec<(String, String)> = Vec::new();
    let mut nome = String::new();
    let mut valor = String::new();
    let mut onde = Onde::Nome;

    // Por caractere e não por índice: um `&str` não se fatia em qualquer byte,
    // e um valor de atributo com acento faria o corte cair no meio de um.
    for c in etiqueta.chars() {
        match onde {
            Onde::Nome => {
                if c == '=' {
                    onde = Onde::AntesDoValor;
                } else if c.is_whitespace() {
                    if !nome.is_empty() {
                        onde = Onde::AntesDoIgual;
                    }
                } else if c == '/' {
                    nome.clear();
                } else {
                    nome.push(c);
                }
            }
            Onde::AntesDoIgual => {
                if c == '=' {
                    onde = Onde::AntesDoValor;
                } else if !c.is_whitespace() {
                    // Atributo sem valor. Nenhum dos que interessam é assim, e
                    // o que começou aqui é o nome do próximo.
                    nome.clear();
                    nome.push(c);
                    onde = Onde::Nome;
                }
            }
            Onde::AntesDoValor => {
                if c == '"' || c == '\'' {
                    onde = Onde::Valor(Some(c));
                } else if !c.is_whitespace() {
                    valor.push(c);
                    onde = Onde::Valor(None);
                }
            }
            Onde::Valor(fecha) => {
                let acabou = fecha.map_or_else(|| c.is_whitespace(), |aspa| c == aspa);
                if acabou {
                    saida.push((nome.to_ascii_lowercase(), std::mem::take(&mut valor)));
                    nome.clear();
                    onde = Onde::Nome;
                } else {
                    valor.push(c);
                }
            }
        }
    }
    // Um valor sem aspas encostado no fim da etiqueta ainda é um valor. Um
    // valor com aspas que nunca fecham é etiqueta quebrada, e cai fora.
    if matches!(onde, Onde::Valor(None)) {
        saida.push((nome.to_ascii_lowercase(), valor));
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG that really is one: the signature and nothing after it.
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];
    const GIF: &[u8] = b"GIF89a\x10\x00\x10\x00\x80\x00";
    const WEBP: &[u8] = b"RIFF\x24\x00\x00\x00WEBPVP8 ";

    #[test]
    fn the_four_are_recognised_by_their_leading_bytes() {
        assert_eq!(sniff(PNG), Some(ImageFormat::Png));
        assert_eq!(sniff(JPEG), Some(ImageFormat::Jpeg));
        assert_eq!(sniff(GIF), Some(ImageFormat::Gif));
        assert_eq!(
            sniff(b"GIF87a\x10\x00\x10\x00\x80\x00"),
            Some(ImageFormat::Gif)
        );
        assert_eq!(sniff(WEBP), Some(ImageFormat::Webp));
    }

    #[test]
    fn what_is_not_one_of_the_four_is_not_sniffed_into_one() {
        // A PDF, a Windows executable, an SVG, a RIFF that is a wave file, and
        // nothing at all. Each of these is a file somebody will send.
        assert_eq!(sniff(b"%PDF-1.7\n%\xE2\xE3"), None);
        assert_eq!(sniff(b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00"), None);
        assert_eq!(sniff(b"<svg xmlns=\"h"), None);
        assert_eq!(sniff(b"RIFF\x24\x00\x00\x00WAVEfmt "), None);
        assert_eq!(sniff(b""), None);
        // And a file that starts with most of a PNG signature is not a PNG. The
        // eight bytes are eight bytes.
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0B]),
            None
        );
    }

    #[test]
    fn a_file_whose_bytes_disagree_with_its_name_is_never_drawn() {
        // The case this whole module exists for. Named `.png` by a sender, and
        // the bytes are a JPEG. It is not drawn — and, the part that matters,
        // it is not helpfully drawn as the JPEG it turned out to be either.
        // Drawing it either way would mean the name decided nothing and the
        // sender's file decided everything.
        let verdict = judge("image/png", JPEG);
        assert_eq!(
            verdict,
            Verdict::Disagrees {
                claimed: ImageFormat::Png,
                found: Some(ImageFormat::Jpeg),
            }
        );
        assert!(!matches!(verdict, Verdict::Draw(_)));

        // And the harder one: named `.png`, and the bytes are a Windows
        // executable. Same outcome, with nothing found.
        assert_eq!(
            judge("image/png", b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00"),
            Verdict::Disagrees {
                claimed: ImageFormat::Png,
                found: None,
            }
        );
    }

    #[test]
    fn agreement_in_both_directions_is_what_draws() {
        assert_eq!(judge("image/png", PNG), Verdict::Draw(ImageFormat::Png));
        assert_eq!(judge("image/jpeg", JPEG), Verdict::Draw(ImageFormat::Jpeg));
        assert_eq!(judge("image/gif", GIF), Verdict::Draw(ImageFormat::Gif));
        assert_eq!(judge("image/webp", WEBP), Verdict::Draw(ImageFormat::Webp));
        // A media type is case-insensitive, and shouting it is not a lie.
        assert_eq!(judge("IMAGE/PNG", PNG), Verdict::Draw(ImageFormat::Png));
    }

    #[test]
    fn good_bytes_under_a_claim_that_is_not_a_picture_are_not_drawn() {
        // The other direction of "agree". Real PNG bytes arriving as
        // `application/octet-stream` — which is what an extension nobody
        // recognises produces — are not drawn, because sniffing alone would
        // make the sender's name decoration.
        assert_eq!(judge("application/octet-stream", PNG), Verdict::NotAPicture);
        assert_eq!(judge("application/pdf", PNG), Verdict::NotAPicture);
        assert_eq!(judge("image/svg+xml", PNG), Verdict::NotAPicture);
        assert_eq!(judge("", PNG), Verdict::NotAPicture);
    }

    #[test]
    fn the_media_type_handed_on_comes_from_the_bytes() {
        // Every drawable outcome carries an `ImageFormat`, and the URI is built
        // from it. There is no path from `declared_type` into the string a
        // window receives — this asserts the shape that makes that true.
        let Verdict::Draw(format) = judge("image/png", PNG) else {
            panic!("agreeing bytes are not drawn");
        };
        let uri = data_uri(format, PNG);
        assert!(uri.starts_with("data:image/png;base64,"), "{uri}");
        assert!(!uri.contains("octet-stream"));
    }

    #[test]
    fn base64_matches_rfc_4648() {
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in vectors {
            let mut out = String::new();
            encode_base64(plain.as_bytes(), &mut out);
            assert_eq!(out, encoded, "{plain}");
        }
        // And every byte value, so the alphabet is exercised end to end rather
        // than only over ASCII.
        let all: Vec<u8> = (0..=255_u8).collect();
        let mut out = String::new();
        encode_base64(&all, &mut out);
        assert_eq!(out.len(), 344);
        assert!(
            out.starts_with("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"),
            "{out}"
        );
        assert!(out.ends_with("+fr7/P3+/w=="), "{out}");
    }

    /// A PNG header the protocol will accept: signature, `IHDR`, and a side.
    fn icone(lado: u32, enchimento: usize) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13_u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&lado.to_be_bytes());
        bytes.extend_from_slice(&lado.to_be_bytes());
        bytes.extend(std::iter::repeat_n(0_u8, enchimento));
        bytes
    }

    #[test]
    fn a_picture_that_will_not_do_as_an_icon_is_refused_before_it_is_sent() {
        // The whole point of the check being here: without it the refusal
        // happens where the frame is built, and a frame that cannot be built
        // looks exactly like a connection that fell over.
        assert_eq!(check_server_icon(None), Ok(()));
        assert_eq!(check_server_icon(Some(&icone(128, 32))), Ok(()));

        assert_eq!(check_server_icon(Some(JPEG)), Err(IconRefusal::NotAnIcon));
        assert_eq!(
            check_server_icon(Some(b"%PDF-1.7")),
            Err(IconRefusal::NotAnIcon)
        );
        // A real PNG declaring a picture nobody could draw. Small file, huge
        // image — the two are not the same question, and this is the one the
        // byte ceiling cannot answer.
        assert_eq!(
            check_server_icon(Some(&icone(20_000, 32))),
            Err(IconRefusal::NotAnIcon)
        );
    }

    #[test]
    fn too_heavy_is_its_own_answer_and_carries_the_ceiling() {
        // A photograph can be shrunk and a PDF cannot be made into an icon, so
        // the two refusals have different next steps — and the sentence about
        // the first one wants the number, which no shell can name for itself.
        let gorda = icone(128, seele_proto::control::MAX_SERVER_ICON_LEN);
        let Err(IconRefusal::TooBig { limit_bytes }) = check_server_icon(Some(&gorda)) else {
            panic!("a picture over the ceiling was accepted, or refused as the wrong thing");
        };
        assert_eq!(
            limit_bytes,
            seele_proto::control::MAX_SERVER_ICON_LEN as u64,
            "the ceiling a shell would print is not the one the protocol enforces"
        );
    }

    #[test]
    fn the_preview_limit_is_far_under_the_server_default_per_file_limit() {
        // The server's default ceiling is 1 GiB and its per-file limit is a
        // sixteenth of it. This has to be a small fraction of that, or the
        // separate limit is not a separate limit.
        let per_file = (1024 * 1024 * 1024_u64) / 16;
        assert!(
            PREVIEW_LIMIT * 8 < per_file,
            "the preview limit is not meaningfully below the per-file limit"
        );
    }
}
