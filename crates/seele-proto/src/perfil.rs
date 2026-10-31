//! PERFIL — o perfil público de uma pessoa, nativo do produto (SEELE 0.19).
//!
//! Nasce do MOD PERFIS (`SEELE-MOD-PERFIS/servidor/main.js`), que guardava os
//! mesmos campos no quintal dele. Aqui o dado é do produto, e a regra de
//! validação mora **num lugar só** — [`validar`] —, lida pelo servidor (que
//! recusa) e pelo cliente (que explica antes de enviar). Duas cópias da regra
//! são duas regras que um dia discordam, e o jeito de discordarem é o editor
//! aceitar o que o servidor vai recusar.
//!
//! # O conjunto recusado vem do MOD, de propósito
//!
//! Os perfis migrados já passaram pelo conjunto do MOD. Um conjunto mais
//! largo aqui faria um perfil migrado falhar na primeira edição; um mais
//! estreito deixaria entrar o que o MOD recusava. O conjunto abaixo é a cópia
//! de `servidor/main.js:19-20` (`CONTROLS` e `CONTROLS_IN_BIO`):
//!
//! ```text
//! CONTROLS         = [U+0000-U+001F U+007F-U+009F U+061C U+200E U+200F U+202A-U+202E U+2066-U+2069]
//! CONTROLS_IN_BIO  = o mesmo, menos U+000A (a quebra de linha)
//! ```
//!
//! São os controles C0 e C1 e os controles de direção (U+202A–U+202E,
//! U+2066–U+2069) e as marcas (LRM, RLM, ALM): num nome, eles desenham um
//! texto que se lê como outro. A bio guarda a quebra de linha.

use serde::{Deserialize, Serialize};

use crate::ids::PersonId;

/// Quantos perfis cabem numa parte de [`crate::ServerMessage::Perfis`].
///
/// **É teto de quadro, e não de gosto:** o pior perfil (todos os textos no
/// limite, cada code point com 4 bytes, e as duas imagens) pesa ~1,8 KB, e
/// oito deles ficam bem abaixo dos 16 KiB de [`crate::control::MAX_FRAME_LEN`]
/// — preso em `uma_parte_de_perfis_cheia_cabe_num_quadro`. Um servidor com mais
/// perfis manda mais partes.
pub const MAX_PERFIS_POR_PARTE: usize = 8;

/// O maior tamanho de uma imagem de perfil: 10 MiB.
///
/// Não são os 100 MiB do volume dos MODs: toda pessoa que desenha a lista ou a
/// chamada baixa a imagem, o bolso de mídia de cada lugar é de 10 MiB e o
/// prazo de cada fluxo é de 60 s. O MOD já limitava a 10 MiB, então a migração
/// nunca encontra imagem maior.
pub const TETO_DA_IMAGEM: usize = 10 * 1024 * 1024;

/// O limite do nome exibido, em code points.
pub const MAX_NOME_EXIBIDO: usize = 40;
/// O limite dos pronomes, em code points.
pub const MAX_PRONOMES: usize = 30;
/// O limite do status, em code points.
pub const MAX_STATUS: usize = 60;
/// O limite da bio, em code points.
pub const MAX_BIO: usize = 280;

/// A cor de um perfil que nunca escolheu uma: `#f2521f`, a do MOD.
pub const COR_PADRAO: [u8; 3] = [0xf2, 0x52, 0x1f];

/// Os efeitos da moldura. Enumerado fechado: o servidor não guarda um nome
/// escolhido por quem envia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Efeito {
    /// Sem moldura animada.
    #[default]
    Nenhum,
    /// `aurora` no MOD.
    Aurora,
    /// `sparkle` no MOD.
    Brilho,
    /// `pulse` no MOD.
    Pulso,
}

/// Qual das duas imagens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LadoDoPerfil {
    /// O quadrado do avatar.
    Foto,
    /// A faixa 16:9 do cartão.
    Capa,
}

/// O tipo de uma imagem de perfil, conferido pelos bytes (`imagem::sniff`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoDaImagem {
    /// `image/png`.
    Png,
    /// `image/jpeg`.
    Jpeg,
    /// `image/gif`.
    Gif,
    /// `image/webp`.
    Webp,
}

impl From<crate::imagem::ImageFormat> for TipoDaImagem {
    fn from(formato: crate::imagem::ImageFormat) -> Self {
        match formato {
            crate::imagem::ImageFormat::Png => Self::Png,
            crate::imagem::ImageFormat::Jpeg => Self::Jpeg,
            crate::imagem::ImageFormat::Gif => Self::Gif,
            crate::imagem::ImageFormat::Webp => Self::Webp,
        }
    }
}

/// Uma imagem de perfil: endereçada pelo SHA-256 dos bytes.
///
/// O hash é o endereço, então o cliente guarda a imagem em cache para sempre:
/// um hash nunca passa a apontar para outros bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagemDoPerfil {
    /// SHA-256 dos bytes.
    pub hash: [u8; 32],
    /// O tipo, conferido pelo servidor.
    pub tipo: TipoDaImagem,
    /// O tamanho, até [`TETO_DA_IMAGEM`].
    pub bytes: u32,
}

/// O perfil como todos o veem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfilPublico {
    /// De quem é.
    pub pessoa: PersonId,
    /// Sobe a cada mudança, inclusive de imagem; é o que `GravarPerfil` confere.
    pub revisao: u32,
    /// Até [`MAX_NOME_EXIBIDO`] code points; vazio quando não há.
    pub nome_exibido: String,
    /// Até [`MAX_PRONOMES`] code points.
    pub pronomes: String,
    /// Até [`MAX_STATUS`] code points.
    pub status: String,
    /// Até [`MAX_BIO`] code points, com quebras de linha.
    pub bio: String,
    /// `#rrggbb` em três bytes.
    pub cor: [u8; 3],
    /// A moldura.
    pub efeito: Efeito,
    /// A foto, quando há.
    pub foto: Option<ImagemDoPerfil>,
    /// A capa, quando há.
    pub capa: Option<ImagemDoPerfil>,
}

/// Por que o servidor recusou um pedido de perfil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecusaDePerfil {
    /// A revisão esperada não é a atual: o perfil mudou em outra janela.
    RevisaoVelha,
    /// Algum campo de texto ou a cor não passou em [`validar`]; `campos` diz qual.
    Campos,
    /// HEIC: reconhecido, e não aceito.
    Heic,
    /// Os bytes não são PNG, JPEG, WebP nem GIF.
    TipoRecusado,
    /// Mais que [`TETO_DA_IMAGEM`].
    GrandeDemais,
    /// O envio terminou antes do tamanho anunciado.
    Incompleto,
    /// O servidor não tem pasta para as imagens (banco em memória).
    SemPastaDePerfis,
}

/// O campo de texto (ou a cor) a que uma recusa se refere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CampoDoPerfil {
    /// `nome_exibido`.
    NomeExibido,
    /// `pronomes`.
    Pronomes,
    /// `status`.
    Status,
    /// `bio`.
    Bio,
    /// `cor`.
    Cor,
}

/// O que há de errado num campo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotivoDoCampo {
    /// Mais code points que o limite.
    Longo,
    /// Um caractere de controle ou de direção (e, fora da bio, a quebra de linha).
    Controle,
    /// A cor não é `#rrggbb`.
    Formato,
}

/// Um erro de [`validar`]: qual campo, e por quê.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErroDeCampo {
    /// O campo.
    pub campo: CampoDoPerfil,
    /// O motivo.
    pub motivo: MotivoDoCampo,
}

/// O que [`validar`] recebe: os campos como a pessoa os digitou.
#[derive(Debug, Clone, Copy)]
pub struct EntradaDePerfil<'a> {
    /// Nome exibido.
    pub nome_exibido: &'a str,
    /// Pronomes.
    pub pronomes: &'a str,
    /// Status.
    pub status: &'a str,
    /// Bio.
    pub bio: &'a str,
    /// Cor, `#rrggbb` (maiúsculas ou minúsculas).
    pub cor: &'a str,
}

/// O que [`validar`] devolve quando tudo passa: os textos já aparados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerfilValidado {
    /// Nome exibido, aparado.
    pub nome_exibido: String,
    /// Pronomes, aparados.
    pub pronomes: String,
    /// Status, aparado.
    pub status: String,
    /// Bio, aparada.
    pub bio: String,
    /// A cor em três bytes.
    pub cor: [u8; 3],
}

/// Se `c` é recusado num campo de texto. Só a bio aceita a quebra de linha.
///
/// Cópia de `CONTROLS`/`CONTROLS_IN_BIO` de `SEELE-MOD-PERFIS/servidor/main.js`
/// (linhas 19-20); ver o topo do módulo.
#[must_use]
pub const fn e_recusado(c: char, na_bio: bool) -> bool {
    match c {
        '\n' => !na_bio,
        '\u{0}'..='\u{1f}'
        | '\u{7f}'..='\u{9f}'
        | '\u{061c}'
        | '\u{200e}'
        | '\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}' => true,
        _ => false,
    }
}

/// `#rrggbb` em três bytes; `None` para qualquer outra forma.
#[must_use]
pub fn cor_de_texto(texto: &str) -> Option<[u8; 3]> {
    let hex = texto.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Três bytes em `#rrggbb` minúsculo, como o MOD gravava.
#[must_use]
pub fn cor_em_texto(cor: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", cor[0], cor[1], cor[2])
}

fn campo(
    erros: &mut Vec<ErroDeCampo>,
    qual: CampoDoPerfil,
    texto: &str,
    max: usize,
    na_bio: bool,
) -> String {
    // O tamanho se mede antes de aparar, como no MOD: quem passa de 40 com um
    // espaço no fim ouve «grande demais», e não «cabe se aparar».
    if texto.chars().count() > max {
        erros.push(ErroDeCampo {
            campo: qual,
            motivo: MotivoDoCampo::Longo,
        });
    } else if texto.chars().any(|c| e_recusado(c, na_bio)) {
        erros.push(ErroDeCampo {
            campo: qual,
            motivo: MotivoDoCampo::Controle,
        });
    }
    texto.trim().to_owned()
}

/// A validação do perfil, **a única**: o servidor a aplica ao `GravarPerfil`, o
/// cliente a aplica antes de enviar.
///
/// Mede em code points (um emoji conta um, como o contador do editor), apara,
/// recusa os caracteres de [`e_recusado`] e exige a cor `#rrggbb`. Os erros
/// voltam **por campo**, todos de uma vez, para que o editor marque cada um no
/// lugar dele.
///
/// # Errors
///
/// Um [`ErroDeCampo`] por campo inválido, na ordem do formulário.
pub fn validar(entrada: &EntradaDePerfil<'_>) -> Result<PerfilValidado, Vec<ErroDeCampo>> {
    let mut erros = Vec::new();
    let nome_exibido = campo(
        &mut erros,
        CampoDoPerfil::NomeExibido,
        entrada.nome_exibido,
        MAX_NOME_EXIBIDO,
        false,
    );
    let pronomes = campo(
        &mut erros,
        CampoDoPerfil::Pronomes,
        entrada.pronomes,
        MAX_PRONOMES,
        false,
    );
    let status = campo(
        &mut erros,
        CampoDoPerfil::Status,
        entrada.status,
        MAX_STATUS,
        false,
    );
    let bio = campo(&mut erros, CampoDoPerfil::Bio, entrada.bio, MAX_BIO, true);
    let cor = cor_de_texto(entrada.cor);
    if cor.is_none() {
        erros.push(ErroDeCampo {
            campo: CampoDoPerfil::Cor,
            motivo: MotivoDoCampo::Formato,
        });
    }
    match (erros.is_empty(), cor) {
        (true, Some(cor)) => Ok(PerfilValidado {
            nome_exibido,
            pronomes,
            status,
            bio,
            cor,
        }),
        _ => Err(erros),
    }
}

/// O teto de forma dos quatro textos no fio: code points e, antes deles, bytes.
///
/// O fio confere o tamanho antes de alocar (`specs/08-seguranca.md`); um code
/// point ocupa até 4 bytes em UTF-8, então o teto em bytes é o quádruplo. Não
/// aplica [`validar`]: quem decide o que é aceito é o servidor, e o quadro só
/// precisa ser limitado.
pub(crate) fn conferir_tamanhos(
    nome_exibido: &str,
    pronomes: &str,
    status: &str,
    bio: &str,
) -> Result<(), crate::control::ControlError> {
    for (nome, texto, max) in [
        ("nome_exibido", nome_exibido, MAX_NOME_EXIBIDO),
        ("pronomes", pronomes, MAX_PRONOMES),
        ("status", status, MAX_STATUS),
        ("bio", bio, MAX_BIO),
    ] {
        crate::control::check_bounds(nome, texto.len(), max * 4)?;
        crate::control::check_bounds(nome, texto.chars().count(), max)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada<'a>() -> EntradaDePerfil<'a> {
        EntradaDePerfil {
            nome_exibido: "Maya",
            pronomes: "ela/dela",
            status: "ao vivo",
            bio: "linha um\nlinha dois",
            cor: "#F2521F",
        }
    }

    /// A lista recusada, escrita à mão e **independente** de [`e_recusado`]:
    /// tirar um code point de lá faz o teste abaixo reprovar.
    fn recusados() -> Vec<char> {
        let mut todos: Vec<u32> = (0x00..=0x1f).collect();
        todos.extend(0x7f..=0x9f);
        todos.extend([0x061c, 0x200e, 0x200f]);
        todos.extend(0x202a..=0x202e);
        todos.extend(0x2066..=0x2069);
        todos.into_iter().filter_map(char::from_u32).collect()
    }

    /// O pior perfil possível: todos os textos no limite, cada code point com
    /// 4 bytes, o maior `PersonId`, a maior revisão e as duas imagens.
    fn o_pior_perfil() -> PerfilPublico {
        let imagem = ImagemDoPerfil {
            hash: [0xff; 32],
            tipo: TipoDaImagem::Webp,
            bytes: u32::try_from(TETO_DA_IMAGEM).unwrap(),
        };
        PerfilPublico {
            pessoa: PersonId(u64::MAX),
            revisao: u32::MAX,
            nome_exibido: "😀".repeat(MAX_NOME_EXIBIDO),
            pronomes: "😀".repeat(MAX_PRONOMES),
            status: "😀".repeat(MAX_STATUS),
            bio: "😀".repeat(MAX_BIO),
            cor: [0xff; 3],
            efeito: Efeito::Pulso,
            foto: Some(imagem),
            capa: Some(imagem),
        }
    }

    /// **O teto de oito é de quadro.** Oito perfis do pior caso cabem em
    /// `MAX_FRAME_LEN`; é a conta que o doc de `MAX_PERFIS_POR_PARTE` promete.
    /// Subir a constante sem ela caber reprova aqui.
    #[test]
    fn uma_parte_de_perfis_cheia_cabe_num_quadro() {
        use crate::control::{encode_em, ControlError, MAX_FRAME_LEN};
        use crate::ServerMessage;

        let cheia = ServerMessage::Perfis {
            parte: vec![o_pior_perfil(); MAX_PERFIS_POR_PARTE],
            ultima: false,
        };
        let quadro = encode_em(12, &cheia).unwrap();
        assert!(quadro.len() <= MAX_FRAME_LEN, "{} bytes", quadro.len());
        // E uma margem: o pior perfil sozinho pesa menos de 2 KB.
        let um = encode_em(12, &ServerMessage::PerfilMudou(o_pior_perfil())).unwrap();
        assert!(um.len() < 2 * 1024, "{} bytes", um.len());

        // Nove passam do teto de forma, e o fio recusa antes de escrever.
        let demais = ServerMessage::Perfis {
            parte: vec![o_pior_perfil(); MAX_PERFIS_POR_PARTE + 1],
            ultima: true,
        };
        assert!(matches!(
            encode_em(12, &demais),
            Err(ControlError::FieldTooLong {
                field: "perfis",
                ..
            })
        ));
    }

    #[test]
    fn um_perfil_com_texto_acima_do_limite_nao_atravessa_o_fio() {
        use crate::control::{encode_em, ControlError};
        use crate::ServerMessage;

        let mut perfil = o_pior_perfil();
        perfil.nome_exibido.push('x');
        assert!(matches!(
            encode_em(12, &ServerMessage::PerfilMudou(perfil)),
            Err(ControlError::FieldTooLong {
                field: "nome_exibido",
                ..
            })
        ));
        let mut perfil = o_pior_perfil();
        perfil.capa = perfil.capa.map(|i| ImagemDoPerfil {
            bytes: u32::try_from(TETO_DA_IMAGEM).unwrap() + 1,
            ..i
        });
        assert!(encode_em(12, &ServerMessage::PerfilMudou(perfil)).is_err());
    }

    #[test]
    fn o_normal_passa_apara_e_baixa_a_cor() {
        let validado = validar(&EntradaDePerfil {
            nome_exibido: "  Maya  ",
            ..entrada()
        })
        .unwrap();
        assert_eq!(validado.nome_exibido, "Maya");
        assert_eq!(validado.bio, "linha um\nlinha dois");
        assert_eq!(validado.cor, [0xf2, 0x52, 0x1f]);
        assert_eq!(cor_em_texto(validado.cor), "#f2521f");
    }

    /// **Caractere por caractere**: cada code point até U+2100 (e mais alguns
    /// acima) é recusado exatamente quando está na lista, nos três campos de
    /// uma linha e na bio — onde só a quebra de linha é perdoada.
    #[test]
    fn cada_caractere_e_recusado_ou_aceito_como_a_lista_manda() {
        let lista = recusados();
        let mut candidatos: Vec<char> = (0..=0x2100).filter_map(char::from_u32).collect();
        candidatos.extend(['\u{feff}', '\u{1f600}', '\u{10ffff}', '\u{e0001}']);
        for c in candidatos {
            let texto = format!("a{c}b");
            let umas_linhas = |campo: &str| {
                let mut e = entrada();
                match campo {
                    "nome" => e.nome_exibido = &texto,
                    "pronomes" => e.pronomes = &texto,
                    _ => e.status = &texto,
                }
                validar(&e).is_ok()
            };
            let esperado_numa_linha = !lista.contains(&c);
            for campo in ["nome", "pronomes", "status"] {
                assert_eq!(
                    umas_linhas(campo),
                    esperado_numa_linha,
                    "U+{:04X} em {campo}",
                    u32::from(c)
                );
            }
            let na_bio = validar(&EntradaDePerfil {
                bio: &texto,
                ..entrada()
            })
            .is_ok();
            let esperado_na_bio = !lista.contains(&c) || c == '\n';
            assert_eq!(na_bio, esperado_na_bio, "U+{:04X} na bio", u32::from(c));
        }
    }

    #[test]
    fn a_quebra_de_linha_so_vale_na_bio() {
        let ruim = validar(&EntradaDePerfil {
            nome_exibido: "a\nb",
            status: "a\nb",
            ..entrada()
        })
        .unwrap_err();
        assert_eq!(
            ruim.iter().map(|e| e.campo).collect::<Vec<_>>(),
            [CampoDoPerfil::NomeExibido, CampoDoPerfil::Status]
        );
        assert!(ruim.iter().all(|e| e.motivo == MotivoDoCampo::Controle));
        assert!(validar(&entrada()).is_ok());
        // O retorno de carro é C0, e a bio também o recusa.
        assert!(validar(&EntradaDePerfil {
            bio: "a\r\nb",
            ..entrada()
        })
        .is_err());
    }

    #[test]
    fn os_limites_sao_em_code_points_e_antes_de_aparar() {
        let quarenta = "😀".repeat(40);
        assert!(validar(&EntradaDePerfil {
            nome_exibido: &quarenta,
            ..entrada()
        })
        .is_ok());
        let quarenta_e_um = "😀".repeat(41);
        let erros = validar(&EntradaDePerfil {
            nome_exibido: &quarenta_e_um,
            ..entrada()
        })
        .unwrap_err();
        assert_eq!(
            erros,
            [ErroDeCampo {
                campo: CampoDoPerfil::NomeExibido,
                motivo: MotivoDoCampo::Longo
            }]
        );
        // Quarenta mais um espaço: a medida é a do que foi digitado.
        let com_espaco = format!("{} ", "a".repeat(40));
        assert!(validar(&EntradaDePerfil {
            nome_exibido: &com_espaco,
            ..entrada()
        })
        .is_err());
        for (max, f) in [(MAX_PRONOMES, 1), (MAX_STATUS, 2), (MAX_BIO, 3)] {
            let ok = "x".repeat(max);
            let demais = "x".repeat(max + 1);
            let monta = |t: &str| {
                let mut e = entrada();
                match f {
                    1 => e.pronomes = t,
                    2 => e.status = t,
                    _ => e.bio = t,
                }
                validar(&e)
            };
            assert!(monta(&ok).is_ok(), "{max}");
            assert!(monta(&demais).is_err(), "{max}+1");
        }
    }

    #[test]
    fn a_cor_e_rrggbb_e_nada_mais() {
        for boa in ["#000000", "#ffffff", "#AbCdEf", "#f2521f"] {
            assert!(cor_de_texto(boa).is_some(), "{boa}");
        }
        for ma in [
            "", "#fff", "f2521f", "#f2521g", "#f2521f0", "##f2521", " #f2521f", "#é2521f",
        ] {
            assert!(cor_de_texto(ma).is_none(), "{ma:?}");
            let erros = validar(&EntradaDePerfil {
                cor: ma,
                ..entrada()
            })
            .unwrap_err();
            assert_eq!(erros[0].campo, CampoDoPerfil::Cor);
        }
    }

    #[test]
    fn os_erros_voltam_todos_e_por_campo() {
        let erros = validar(&EntradaDePerfil {
            nome_exibido: "a\u{202e}b",
            pronomes: &"p".repeat(31),
            status: "ok",
            bio: &"b".repeat(281),
            cor: "vermelho",
        })
        .unwrap_err();
        assert_eq!(
            erros
                .iter()
                .map(|e| (e.campo, e.motivo))
                .collect::<Vec<_>>(),
            [
                (CampoDoPerfil::NomeExibido, MotivoDoCampo::Controle),
                (CampoDoPerfil::Pronomes, MotivoDoCampo::Longo),
                (CampoDoPerfil::Bio, MotivoDoCampo::Longo),
                (CampoDoPerfil::Cor, MotivoDoCampo::Formato),
            ]
        );
    }
}
