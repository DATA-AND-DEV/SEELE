//! O endereço de um Dogma como um texto que se manda para alguém.
//!
//! Fecha o ADR 0006, que estava `proposto` desde M2.
//!
//! ```text
//! seele://dogma.exemplo:8383/?fp=3cbcfb02…&convite=7K4M…
//! ```
//!
//! # O que carrega, e por quê
//!
//! **Endereço** — obrigatório. Sem ele não há nada.
//!
//! **`fp`, a impressão digital** — opcional e o motivo principal de isto
//! existir. O ADR 0003 fixa a chave do servidor no primeiro contato, e até aqui
//! esse primeiro contato era **cego**: o cliente aceitava o que aparecesse e
//! pedia para a pessoa conferir por outro canal, o que ninguém faz. Com a
//! impressão digital dentro do link, o primeiro contato é verificado — o
//! cliente compara antes de fixar, e um servidor no meio do caminho não passa.
//!
//! **`convite`, o token de uso único** — opcional. Um convite gasto que vaze
//! depois não vale nada, e é exatamente por isso que ele pode viajar num link
//! e uma senha não deveria. `specs/08-seguranca.md` já recomendava tokens por
//! esse motivo.
//!
//! # O que deliberadamente não carrega
//!
//! **A senha do Dogma.** Seria conveniente e é a coisa errada: uma senha vale
//! para sempre e para todo mundo, e um link acaba em histórico de terminal,
//! backup de conversa, captura de tela. O convite existe justamente para
//! ocupar esse lugar. Quem usa senha digita a senha.
//!
//! **O apelido.** É de quem recebe, não de quem convida.

use std::fmt;

/// Esquema da URI. Registrado no ADR 0006.
pub const ESQUEMA: &str = "seele://";

/// Porta padrão, quando o texto não traz uma.
pub const PORTA_PADRAO: u16 = crate::transport::DEFAULT_PORT;

/// Um convite para entrar num Dogma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Convite {
    /// `host` ou `host:porta`, como estava escrito.
    pub alvo: String,
    /// Impressão digital esperada do certificado, se o link a trouxe.
    pub impressao_digital: Option<String>,
    /// Token de uso único, se o link o trouxe.
    pub token: Option<String>,
    /// Cage a entrar assim que conectar.
    pub cage: Option<u32>,
}

impl Convite {
    /// Um convite com o endereço e mais nada.
    #[must_use]
    pub fn novo(alvo: impl Into<String>) -> Self {
        Self {
            alvo: alvo.into(),
            impressao_digital: None,
            token: None,
            cage: None,
        }
    }

    /// Acrescenta a impressão digital do certificado.
    #[must_use]
    pub fn com_impressao_digital(mut self, impressao: impl Into<String>) -> Self {
        self.impressao_digital = Some(impressao.into());
        self
    }

    /// Acrescenta um token de convite.
    #[must_use]
    pub fn com_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Acrescenta o Cage de destino.
    #[must_use]
    pub fn com_cage(mut self, cage: u32) -> Self {
        self.cage = Some(cage);
        self
    }

    /// O alvo separado em máquina e porta. Ver [`separar`].
    ///
    /// # Errors
    ///
    /// Não falha para um convite vindo de [`analisar`], que já validou o alvo;
    /// falha para um montado à mão com [`Convite::novo`].
    pub fn endereco(&self) -> Result<Alvo<'_>, ErroDeUri> {
        separar(&self.alvo)
    }
}

/// Por que um texto não é um convite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDeUri {
    /// Não começa com `seele://`.
    EsquemaDesconhecido,
    /// Não há endereço depois do esquema.
    SemEndereco,
    /// O endereço tem caractere que endereço não tem.
    EnderecoInvalido,
    /// Um IPv6 escrito sem colchetes.
    ///
    /// Erro próprio, e não [`ErroDeUri::EnderecoInvalido`], porque a correção é
    /// específica e quem colou o endereço consegue fazê-la: `2001:db8::1` vira
    /// `[2001:db8::1]`. Ver [`separar`].
    EnderecoIpv6SemColchetes,
    /// A impressão digital não é hexadecimal de 64 caracteres.
    ImpressaoDigitalInvalida,
    /// O token tem caractere fora do alfabeto de convites.
    TokenInvalido,
    /// O Cage não é um número.
    CageInvalido,
}

impl fmt::Display for ErroDeUri {
    fn fmt(&self, formatador: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatador, "{self:?}")
    }
}

impl std::error::Error for ErroDeUri {}

impl fmt::Display for Convite {
    /// Monta a URI. O que sai daqui é o que se manda para alguém.
    fn fmt(&self, formatador: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatador, "{ESQUEMA}{}", self.alvo)?;

        let mut separador = '?';
        if let Some(impressao) = &self.impressao_digital {
            write!(formatador, "{separador}fp={impressao}")?;
            separador = '&';
        }
        if let Some(token) = &self.token {
            write!(formatador, "{separador}convite={token}")?;
            separador = '&';
        }
        if let Some(cage) = self.cage {
            write!(formatador, "{separador}cage={cage}")?;
        }
        Ok(())
    }
}

/// Lê um convite escrito como texto.
///
/// Tudo é validado antes de virar `Convite`. Este texto chega colado de uma
/// conversa, e um endereço que passa sem conferência acaba num `connect`.
///
/// # Errors
///
/// [`ErroDeUri`] dizendo o que está errado. Aqui a mensagem específica ajuda e
/// não vaza nada: quem colou o link é quem vai corrigi-lo.
pub fn analisar(texto: &str) -> Result<Convite, ErroDeUri> {
    let texto = texto.trim();
    let resto = texto
        .strip_prefix(ESQUEMA)
        .ok_or(ErroDeUri::EsquemaDesconhecido)?;

    // A barra antes da consulta é opcional: `seele://host?x=1` e
    // `seele://host/?x=1` são a mesma coisa, e as duas formas serão coladas.
    let (alvo, consulta) = match resto.split_once('?') {
        Some((alvo, consulta)) => (alvo.trim_end_matches('/'), consulta),
        None => (resto.trim_end_matches('/'), ""),
    };

    if alvo.is_empty() {
        return Err(ErroDeUri::SemEndereco);
    }
    validar_alvo(alvo)?;

    let mut convite = Convite::novo(alvo);
    for parte in consulta.split('&').filter(|p| !p.is_empty()) {
        let (chave, valor) = parte.split_once('=').unwrap_or((parte, ""));
        match chave {
            "fp" => {
                validar_impressao_digital(valor)?;
                convite.impressao_digital = Some(valor.to_ascii_lowercase());
            }
            "convite" => {
                validar_token(valor)?;
                convite.token = Some(valor.to_ascii_uppercase());
            }
            "cage" => {
                convite.cage = Some(valor.parse().map_err(|_| ErroDeUri::CageInvalido)?);
            }
            // Parâmetro desconhecido é ignorado, e de propósito: é o que
            // permite acrescentar um campo depois sem que clientes velhos
            // recusem links novos.
            _ => {}
        }
    }

    Ok(convite)
}

/// Um alvo já separado em máquina e porta.
///
/// `maquina` vem **sem colchetes**: `[2001:db8::1]:8383` devolve
/// `2001:db8::1`. É a forma que `(maquina, porta).to_socket_addrs()` aceita e a
/// forma que se compara com um [`std::net::IpAddr`] — as duas coisas que quem
/// chama vai fazer em seguida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alvo<'a> {
    /// Nome ou endereço, sem colchetes e sem porta.
    pub maquina: &'a str,
    /// A porta escrita, ou [`PORTA_PADRAO`] se o texto não trouxe uma.
    pub porta: u16,
}

/// Separa `host[:porta]` em máquina e porta, entendendo IPv6.
///
/// # Por que isto não é um `rsplit_once(':')`
///
/// O separador de porta e o separador de um IPv6 são o mesmo caractere, e as
/// cascas resolviam isso com `rsplit_once(':')` — que em `[2001:db8::1]:8383`
/// devolve a máquina com colchetes (que nenhum resolvedor aceita) e em
/// `2001:db8::1` devolve `2001:db8:` na porta `1`, um endereço que não existe.
/// A regra da RFC 3986 é a dos colchetes, e ela mora aqui, uma vez.
///
/// # Errors
///
/// [`ErroDeUri::EnderecoIpv6SemColchetes`] quando o texto tem mais de um `:` e
/// nenhum colchete — o caso de quem copiou o endereço de um `ip addr` e colou
/// cru. [`ErroDeUri::EnderecoInvalido`] para o resto.
pub fn separar(alvo: &str) -> Result<Alvo<'_>, ErroDeUri> {
    if let Some(depois_do_colchete) = alvo.strip_prefix('[') {
        let (dentro, resto) = depois_do_colchete
            .split_once(']')
            .ok_or(ErroDeUri::EnderecoInvalido)?;
        if dentro.is_empty() {
            return Err(ErroDeUri::EnderecoInvalido);
        }
        let porta = if resto.is_empty() {
            PORTA_PADRAO
        } else {
            resto
                .strip_prefix(':')
                .ok_or(ErroDeUri::EnderecoInvalido)?
                .parse()
                .map_err(|_| ErroDeUri::EnderecoInvalido)?
        };
        return Ok(Alvo {
            maquina: dentro,
            porta,
        });
    }

    match alvo.rsplit_once(':') {
        // Dois `:` e nenhum colchete só pode ser um IPv6 escrito cru. Tratá-lo
        // como `host:porta` produziria uma máquina que não existe e uma porta
        // que não foi pedida, e o erro apareceria muito longe daqui.
        Some((antes, _)) if antes.contains(':') => Err(ErroDeUri::EnderecoIpv6SemColchetes),
        Some((maquina, porta)) => {
            if maquina.is_empty() {
                return Err(ErroDeUri::EnderecoInvalido);
            }
            Ok(Alvo {
                maquina,
                porta: porta.parse().map_err(|_| ErroDeUri::EnderecoInvalido)?,
            })
        }
        None => Ok(Alvo {
            maquina: alvo,
            porta: PORTA_PADRAO,
        }),
    }
}

/// Só o que aparece num `host[:porta]`.
///
/// Estreito por escolha. Este texto vira endereço de conexão, e a lista curta é
/// mais fácil de defender do que a lista de tudo que já foi visto por aí.
///
/// A separação faz parte da validação de propósito: um `Convite` que saiu de
/// [`analisar`] sempre se separa, e quem chama [`separar`] depois não precisa
/// tratar um erro que não pode acontecer.
fn validar_alvo(alvo: &str) -> Result<(), ErroDeUri> {
    if alvo.len() > 255 {
        return Err(ErroDeUri::EnderecoInvalido);
    }
    let permitido = alvo
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']' | '_'));
    if !permitido {
        return Err(ErroDeUri::EnderecoInvalido);
    }
    // Uma porta que não é número transformaria o erro num `connect` estranho
    // lá adiante, longe daqui.
    separar(alvo).map(|_| ())
}

/// SHA-256 em hexadecimal: 64 caracteres, nada mais.
fn validar_impressao_digital(valor: &str) -> Result<(), ErroDeUri> {
    if valor.len() == 64 && valor.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ErroDeUri::ImpressaoDigitalInvalida)
    }
}

/// O alfabeto dos convites: base32 de Crockford, maiúsculas.
fn validar_token(valor: &str) -> Result<(), ErroDeUri> {
    if valor.is_empty() || valor.len() > 64 {
        return Err(ErroDeUri::TokenInvalido);
    }
    if valor.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(ErroDeUri::TokenInvalido)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FP: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

    #[test]
    fn o_minimo_e_um_endereco() {
        let convite = analisar("seele://dogma.exemplo:8383").expect("analisar");
        assert_eq!(convite.alvo, "dogma.exemplo:8383");
        assert_eq!(convite.impressao_digital, None);
        assert_eq!(convite.token, None);
    }

    #[test]
    fn ida_e_volta_com_tudo() {
        let original = Convite::novo("dogma.exemplo:8383")
            .com_impressao_digital(FP)
            .com_token("7K4MNPQRSTVWXYZ23456")
            .com_cage(2);

        let texto = original.to_string();
        assert_eq!(analisar(&texto).expect("analisar"), original);
    }

    #[test]
    fn a_barra_antes_da_consulta_e_opcional() {
        // As duas formas vão ser coladas, porque as duas parecem certas.
        let com = analisar(&format!("seele://host:8383/?fp={FP}")).expect("com");
        let sem = analisar(&format!("seele://host:8383?fp={FP}")).expect("sem");
        assert_eq!(com, sem);
    }

    #[test]
    fn espaco_em_volta_nao_atrapalha() {
        // Colar de uma conversa traz espaço e quebra de linha junto.
        let convite = analisar("  seele://host:8383\n").expect("analisar");
        assert_eq!(convite.alvo, "host:8383");
    }

    #[test]
    fn outro_esquema_e_recusado() {
        assert_eq!(
            analisar("https://dogma.exemplo"),
            Err(ErroDeUri::EsquemaDesconhecido)
        );
        assert_eq!(
            analisar("dogma.exemplo"),
            Err(ErroDeUri::EsquemaDesconhecido)
        );
    }

    #[test]
    fn um_endereco_com_coisa_estranha_nao_passa() {
        // Este texto vira um `connect`. A porta de entrada é aqui.
        for hostil in [
            "seele://host:8383/../../etc/passwd",
            "seele://host 8383",
            "seele://host;rm -rf /",
            "seele://host\u{0}mais",
            "seele://host|nc atacante 1",
        ] {
            assert_eq!(
                analisar(hostil),
                Err(ErroDeUri::EnderecoInvalido),
                "passou: {hostil}"
            );
        }
    }

    #[test]
    fn uma_impressao_digital_que_nao_e_uma_nao_passa() {
        // Aceitar uma impressão digital truncada seria pior que não ter
        // nenhuma: o cliente compararia e passaria achando que verificou.
        for ruim in ["abc", "", &FP[..63], &format!("{FP}0"), &"z".repeat(64)] {
            assert_eq!(
                analisar(&format!("seele://host?fp={ruim}")),
                Err(ErroDeUri::ImpressaoDigitalInvalida),
                "passou: {ruim}"
            );
        }
    }

    #[test]
    fn a_impressao_digital_normaliza_para_minusculas() {
        // O servidor imprime em minúsculas; alguém vai digitar em maiúsculas, e
        // a comparação depois é byte a byte.
        let convite =
            analisar(&format!("seele://host?fp={}", FP.to_uppercase())).expect("analisar");
        assert_eq!(convite.impressao_digital.as_deref(), Some(FP));
    }

    #[test]
    fn um_token_com_caractere_estranho_nao_passa() {
        for ruim in ["", "tem espaço", "tem/barra", "tem;ponto"] {
            assert!(
                matches!(
                    analisar(&format!("seele://host?convite={ruim}")),
                    Err(ErroDeUri::TokenInvalido)
                ),
                "passou: {ruim}"
            );
        }
    }

    #[test]
    fn parametro_desconhecido_e_ignorado_em_vez_de_recusado() {
        // É o que permite acrescentar um campo depois sem que cliente velho
        // recuse link novo.
        let convite = analisar("seele://host:8383?futuro=1&cage=3").expect("analisar");
        assert_eq!(convite.cage, Some(3));
    }

    #[test]
    fn a_senha_nunca_viaja_no_link() {
        // Decisão registrada, e um teste porque decisões sem teste voltam.
        // Uma senha vale para sempre; um convite gasto não vale nada.
        let texto = Convite::novo("host:8383")
            .com_impressao_digital(FP)
            .com_token("7K4MNPQRSTVWXYZ23456")
            .to_string();
        assert!(!texto.contains("senha"));
        assert!(!texto.contains("password"));
    }

    #[test]
    fn um_endereco_ipv6_atravessa_inteiro() {
        let convite = analisar("seele://[2001:db8::1]:8383").expect("analisar");
        assert_eq!(convite.alvo, "[2001:db8::1]:8383");
    }

    #[test]
    fn uma_porta_que_nao_e_numero_e_recusada_aqui_e_nao_la_adiante() {
        assert_eq!(
            analisar("seele://host:porta"),
            Err(ErroDeUri::EnderecoInvalido)
        );
    }

    #[test]
    fn um_ipv6_se_separa_sem_os_colchetes_e_com_a_porta_escrita() {
        // Os colchetes são sintaxe de URI, não parte do endereço: nenhum
        // resolvedor aceita `[2001:db8::1]` e todo `IpAddr` aceita
        // `2001:db8::1`. Quem separava com `rsplit_once(':')` entregava o
        // primeiro.
        let alvo = separar("[2001:db8::1]:8383").expect("separar");
        assert_eq!(alvo.maquina, "2001:db8::1");
        assert_eq!(alvo.porta, 8383);
        assert!(
            alvo.maquina.parse::<std::net::IpAddr>().is_ok(),
            "a máquina não é um endereço: {}",
            alvo.maquina
        );
    }

    #[test]
    fn um_ipv6_sem_porta_fica_com_a_porta_padrao() {
        let alvo = separar("[2001:db8::1]").expect("separar");
        assert_eq!(alvo.maquina, "2001:db8::1");
        assert_eq!(alvo.porta, PORTA_PADRAO);
    }

    #[test]
    fn um_ipv6_sem_colchetes_diz_que_faltam_os_colchetes() {
        // O caso real: a pessoa roda `ip addr`, copia o endereço e cola cru.
        // `rsplit_once(':')` lia `2001:db8::1` como a máquina `2001:db8:` na
        // porta `1` — um endereço que não existe, e um erro que só aparecia lá
        // adiante, num `connect` que ninguém liga a este texto.
        for cru in ["2001:db8::1", "::1", "fe80::1%eth0"] {
            assert_eq!(
                separar(cru),
                Err(ErroDeUri::EnderecoIpv6SemColchetes),
                "passou cru: {cru}"
            );
        }
    }

    #[test]
    fn um_nome_e_um_ipv4_continuam_se_separando_como_antes() {
        assert_eq!(
            separar("dogma.exemplo:8383"),
            Ok(Alvo {
                maquina: "dogma.exemplo",
                porta: 8383
            })
        );
        assert_eq!(
            separar("192.0.2.10"),
            Ok(Alvo {
                maquina: "192.0.2.10",
                porta: PORTA_PADRAO
            })
        );
    }

    #[test]
    fn um_alvo_que_analisar_aceitou_sempre_se_separa() {
        // A invariante que faz `Convite::endereco` não precisar ser tratada
        // como falha provável: a validação e a separação são a mesma regra.
        for bom in [
            "seele://host",
            "seele://host:8383",
            "seele://[2001:db8::1]",
            "seele://[2001:db8::1]:8383",
            "seele://192.0.2.10:1",
        ] {
            let convite = analisar(bom).unwrap_or_else(|erro| panic!("{bom}: {erro:?}"));
            assert!(convite.endereco().is_ok(), "analisou e não separa: {bom}");
        }
    }

    #[test]
    fn um_ipv6_cru_no_link_e_recusado_com_o_motivo() {
        // O link inteiro, não só a separação: é `analisar` que a casca chama, e
        // é ele que tem de dizer o que fazer. `EnderecoInvalido` mandaria a
        // pessoa procurar um caractere errado que não existe.
        assert_eq!(
            analisar("seele://2001:db8::1"),
            Err(ErroDeUri::EnderecoIpv6SemColchetes)
        );
    }

    #[test]
    fn um_colchete_sem_fecho_nao_passa() {
        for torto in ["[2001:db8::1", "[]:8383", "[2001:db8::1]8383"] {
            assert_eq!(
                separar(torto),
                Err(ErroDeUri::EnderecoInvalido),
                "passou: {torto}"
            );
        }
    }
}
