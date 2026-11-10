//! **A menção**: quando o corpo de uma mensagem chama alguém pelo apelido.
//!
//! Uma função pura, e uma só, para a sessão (que marca
//! [`crate::state::Message::mentions_me`] e levanta o aviso), para a contagem
//! de menções não lidas de cada canal e para o autocompletar da casca: as três
//! perguntam a [`menciona`], e nenhuma repete a regra.
//!
//! Nada disto viaja no fio. O servidor não sabe o que é uma menção: quem recebe
//! a mensagem confere o próprio apelido contra o corpo.
//!
//! # A regra, por inteiro
//!
//! O corpo menciona `base` quando tem um `@` seguido do apelido, e:
//!
//! 1. **antes do `@`** está o começo do texto ou um caractere que não é letra,
//!    número nem `_`. É o que separa a menção do endereço: em `x@ana.com` o
//!    `@` vem depois de uma letra, e não é menção;
//! 2. **o apelido é comparado sem diferenciar maiúsculas** (`@ANA` chama
//!    `ana`), caractere a caractere, pela minúscula do Unicode. Um apelido com
//!    espaço casa inteiro, com o espaço;
//! 3. **depois do apelido** está o fim do texto ou um caractere que não o
//!    continua. Continuam um apelido: letra, número, `_`, e `-` seguido de
//!    letra ou número. Por isso `@ana` não casa em `@anabela`, em `@ana_2` nem
//!    em `@ana-maria`, e casa em `@ana,`, `@ana.` e `@ana!`;
//! 4. **a marca**, quando vem (`@ana#K7Q2`: `#` e quatro caracteres de `A-Z` e
//!    `0-9`, sem diferenciar maiúsculas), tem de ser a da pessoa. `@ana#K7Q2`
//!    chama só a `ana` dessa marca; `@ana`, sem marca, chama toda `ana`. Um
//!    `#` que não é seguido de uma marca bem formada é pontuação, e a menção
//!    vale.
//!
//! Um apelido vazio (ou só de espaços) não é mencionado por nada.

/// Se `corpo` menciona a pessoa de apelido `base` e marca `tag`.
///
/// `base` é o apelido **como a tela o mostra**: a base da identidade
/// apresentada (`ana`), e não o alias interno (`ana#K7Q2`). `tag` é a marca de
/// quatro caracteres, quando a pessoa tem uma. A regra inteira está no doc do
/// módulo.
///
/// # Examples
///
/// ```
/// use seele_sessao::mencao::menciona;
///
/// assert!(menciona("oi @Ana, vem cá", "ana", None));
/// assert!(!menciona("oi @anabela", "ana", None));
/// assert!(!menciona("escreva para x@ana.com", "ana", None));
/// assert!(menciona("@ana#K7Q2 e só ela", "ana", Some("K7Q2")));
/// assert!(!menciona("@ana#K7Q2 e só ela", "ana", Some("ZZZ9")));
/// ```
#[must_use]
pub fn menciona(corpo: &str, base: &str, tag: Option<&str>) -> bool {
    if base.trim().is_empty() {
        return false;
    }
    let mut anterior: Option<char> = None;
    for (posicao, atual) in corpo.char_indices() {
        let comeca = atual == '@' && !anterior.is_some_and(de_palavra);
        anterior = Some(atual);
        if !comeca {
            continue;
        }
        let depois_do_arroba = corpo.get(posicao + atual.len_utf8()..).unwrap_or_default();
        let Some(resto) = sem_o_prefixo(depois_do_arroba, base) else {
            continue;
        };
        if termina_aqui(resto, tag) {
            return true;
        }
    }
    false
}

/// Letra, número ou `_`: o que faz parte de uma palavra.
fn de_palavra(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `texto` sem `prefixo` no começo, comparando sem diferenciar maiúsculas.
/// `None` quando `texto` não começa por ele.
fn sem_o_prefixo<'a>(texto: &'a str, prefixo: &str) -> Option<&'a str> {
    let mut resto = texto.chars();
    for esperado in prefixo.chars() {
        let veio = resto.next()?;
        if !veio.to_lowercase().eq(esperado.to_lowercase()) {
            return None;
        }
    }
    Some(resto.as_str())
}

/// Se o apelido acabou onde `resto` começa — e, quando vem uma marca, se ela é
/// a de quem pergunta.
fn termina_aqui(resto: &str, tag: Option<&str>) -> bool {
    let mut caracteres = resto.chars();
    match caracteres.next() {
        None => true,
        Some(c) if de_palavra(c) => false,
        // `@ana-maria` é outro apelido; `@ana - vem` e `@ana-` são a `ana`.
        Some('-') => !caracteres.next().is_some_and(char::is_alphanumeric),
        Some('#') => match marca_escrita(caracteres.as_str()) {
            Some(escrita) => tag.is_some_and(|minha| minha.eq_ignore_ascii_case(escrita)),
            None => true,
        },
        Some(_) => true,
    }
}

/// A marca bem formada no começo de `texto`: quatro caracteres de `A-Z`,
/// `a-z` e `0-9`, sem um quinto de palavra atrás.
fn marca_escrita(texto: &str) -> Option<&str> {
    let marca = texto.get(..4)?;
    if !marca.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    if texto.get(4..)?.chars().next().is_some_and(de_palavra) {
        return None;
    }
    Some(marca)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_apelido_casa_sem_diferenciar_maiusculas() {
        assert!(menciona("@ana", "ana", None));
        assert!(menciona("@ANA", "ana", None));
        assert!(menciona("@ana", "Ana", None));
        assert!(menciona("oi @Ana tudo bem", "ana", None));
        assert!(menciona("@ÉRICA vem", "érica", None));
    }

    #[test]
    fn a_fronteira_de_palavra_vale_dos_dois_lados() {
        // Depois.
        assert!(!menciona("@anabela", "ana", None));
        assert!(!menciona("@ana_2", "ana", None));
        assert!(!menciona("@ana2", "ana", None));
        assert!(!menciona("@ana-maria", "ana", None));
        assert!(!menciona("@anaé", "ana", None));
        // Antes: o endereço não é menção.
        assert!(!menciona("x@ana.com", "ana", None));
        assert!(!menciona("fulano_@ana", "ana", None));
        assert!(!menciona("9@ana", "ana", None));
        // E a pontuação em volta não atrapalha.
        for corpo in [
            "@ana,",
            "@ana.",
            "@ana!",
            "@ana?",
            "(@ana)",
            "\"@ana\"",
            "@ana:",
            "@ana-",
            "@ana - oi",
            "oi,@ana",
            "\n@ana\n",
            "@@ana",
        ] {
            assert!(menciona(corpo, "ana", None), "{corpo:?}");
        }
    }

    #[test]
    fn so_o_arroba_chama() {
        assert!(!menciona("ana", "ana", None));
        assert!(!menciona("a ana disse", "ana", None));
        assert!(!menciona("@", "ana", None));
        assert!(!menciona("", "ana", None));
        assert!(!menciona("@an", "ana", None));
    }

    #[test]
    fn a_segunda_ocorrencia_tambem_conta() {
        assert!(menciona("@anabela e @ana", "ana", None));
        assert!(menciona("x@ana.com, mas @ana sim", "ana", None));
    }

    #[test]
    fn a_marca_escrita_tem_de_ser_a_da_pessoa() {
        assert!(menciona("@ana#K7Q2", "ana", Some("K7Q2")));
        assert!(menciona("@ana#k7q2 oi", "ana", Some("K7Q2")));
        assert!(!menciona("@ana#K7Q2", "ana", Some("ZZZ9")));
        // Quem não tem marca não é a `ana` de marca nenhuma.
        assert!(!menciona("@ana#K7Q2", "ana", None));
        // Sem marca escrita, toda `ana` é chamada.
        assert!(menciona("@ana", "ana", Some("K7Q2")));
        // Um `#` que não é marca é pontuação.
        assert!(menciona("@ana#", "ana", Some("K7Q2")));
        assert!(menciona("@ana#1", "ana", Some("K7Q2")));
        assert!(menciona("@ana#K7Q2X", "ana", Some("ZZZ9")));
        assert!(menciona("@ana #K7Q2", "ana", Some("ZZZ9")));
    }

    #[test]
    fn um_apelido_com_espaco_casa_inteiro() {
        assert!(menciona("oi @maria clara!", "Maria Clara", None));
        assert!(!menciona("oi @maria", "Maria Clara", None));
        assert!(!menciona("oi @maria clarabela", "Maria Clara", None));
    }

    #[test]
    fn um_apelido_vazio_nao_e_mencionado() {
        assert!(!menciona("@ oi", "", None));
        assert!(!menciona("@  oi", " ", None));
    }

    /// Total: nenhum corpo e nenhum apelido derrubam a função, e os índices
    /// de byte nunca caem no meio de um caractere de mais de um byte.
    #[test]
    fn nenhuma_entrada_derruba() {
        let pedacos = [
            "@", "é", "#", "-", "a", "日本", "🙂", " ", "K7Q2", "_", "\u{301}",
        ];
        for a in pedacos {
            for b in pedacos {
                for c in pedacos {
                    for d in pedacos {
                        let corpo = format!("{a}{b}{c}{d}");
                        for base in ["a", "é", "日本", "🙂", "a é", "#", "@"] {
                            let _ = menciona(&corpo, base, None);
                            let _ = menciona(&corpo, base, Some("K7Q2"));
                            // `@apelido` sozinho sempre chama o apelido.
                            assert!(menciona(&format!("{corpo} @{base}"), base, None));
                        }
                    }
                }
            }
        }
    }
}
