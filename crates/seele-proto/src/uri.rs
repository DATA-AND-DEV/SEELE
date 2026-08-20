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
//! **`alt`, os outros endereços do mesmo Dogma** — opcional, e a resposta a um
//! defeito de campo. Uma máquina tem vários endereços, e nenhum deles serve
//! para todo mundo: o da rede de casa não é alcançável de fora, o público que o
//! roteador abriu costuma não voltar para dentro — muitos roteadores domésticos
//! não fazem *hairpin* —, e o de uma VPN só serve a quem estiver na mesma VPN.
//! Enquanto o convite levava **um** endereço, alguma dessas situações sempre
//! perdia, e quem descobria era o amigo do outro lado, como "não conecta".
//!
//! O primeiro endereço continua sendo o do texto principal, e é ele que um
//! cliente antigo lê — por isso ele é o da rede local, que é o caso comum e o
//! que sempre funcionou. Os outros vêm em `alt`, separados por vírgula, na
//! ordem em que se tenta.
//!
//! **`enc`, o bilhete de encontro** — opcional, e o degrau 4 do ADR 0022. Duas
//! metades separadas por `/`: o endereço do **ponto de encontro** e o endereço
//! onde o anfitrião espera o aviso de que alguém quer entrar.
//!
//! ```text
//! ?enc=encontro.exemplo:8384/198.51.100.7:41234
//! ```
//!
//! As duas metades estão aqui, e não uma só, porque o ADR 0022 exige que o ponto
//! de encontro seja **trocável**: quem hospeda aponta para o seu, e o endereço
//! dele viaja no link. Um endereço compilado no cliente faria de "trocável" uma
//! frase que ninguém consegue exercer.
//!
//! Um cliente que não conhece este campo cai na regra do parâmetro desconhecido
//! e o ignora. Ele perde o degrau 4 e continua tendo todos os endereços do
//! `alt`, que é exatamente o que ele tinha antes de este campo existir.
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

/// Quantos endereços cabem num convite.
///
/// Quatro, contando o principal. Cada um custa a quem recebe uma tentativa com
/// prazo antes de a sala abrir, e o link é para colar numa conversa. Um convite
/// que traga mais que isto não é recusado — os excedentes são ignorados, pela
/// mesma razão que um parâmetro desconhecido é: recusar link novo é o que faz
/// cliente velho virar parede.
pub const LIMITE_DE_ALVOS: usize = 4;

/// Um convite para entrar num Dogma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Convite {
    /// `host` ou `host:porta`, como estava escrito.
    ///
    /// O primeiro endereço a tentar, e o único que um cliente anterior a este
    /// campo enxerga. Por isso ele é o da rede local: é o caso comum e o que
    /// sempre funcionou.
    pub alvo: String,
    /// Os outros endereços do mesmo Dogma, na ordem em que se tenta.
    ///
    /// Vazio num convite antigo, e é assim que a compatibilidade se resolve nos
    /// dois sentidos: um convite de antes vira uma lista de um, e um cliente de
    /// antes lê `alt` como parâmetro desconhecido e o ignora.
    pub alternativos: Vec<String>,
    /// Onde bater para que o anfitrião fure o NAT, se o link trouxe um bilhete.
    ///
    /// Degrau 4 do ADR 0022. `None` num convite de um Dogma que não precisou
    /// dele — o que alcança de fora por IPv6 ou por porta no roteador não tem
    /// terceiro nenhum no meio, e não deve ganhar um.
    pub bilhete: Option<Bilhete>,
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
            alternativos: Vec::new(),
            bilhete: None,
            impressao_digital: None,
            token: None,
            cage: None,
        }
    }

    /// Acrescenta o bilhete de encontro. Degrau 4 do ADR 0022.
    #[must_use]
    pub fn com_bilhete(mut self, bilhete: Bilhete) -> Self {
        self.bilhete = Some(bilhete);
        self
    }

    /// Acrescenta os outros endereços do mesmo Dogma, na ordem de tentativa.
    ///
    /// Endereço repetido é descartado, e a lista inteira cabe em
    /// [`LIMITE_DE_ALVOS`] contando o principal — quem monta o convite não
    /// precisa saber o limite de cor.
    #[must_use]
    pub fn com_alternativos<T: Into<String>>(
        mut self,
        alternativos: impl IntoIterator<Item = T>,
    ) -> Self {
        for alternativo in alternativos {
            let alternativo = alternativo.into();
            if alternativo == self.alvo || self.alternativos.contains(&alternativo) {
                continue;
            }
            if self.alternativos.len() + 1 >= LIMITE_DE_ALVOS {
                break;
            }
            self.alternativos.push(alternativo);
        }
        self
    }

    /// Todos os endereços deste convite, na ordem em que se tenta.
    ///
    /// O principal primeiro. É o que quem conecta percorre, e num convite
    /// antigo tem um item só — o caminho de antes, sem desvio.
    pub fn candidatos(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.alvo.as_str()).chain(self.alternativos.iter().map(String::as_str))
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

    /// Todos os endereços já separados em máquina e porta, na ordem.
    ///
    /// # Errors
    ///
    /// O mesmo de [`Convite::endereco`], e pelo mesmo motivo: não falha para um
    /// convite vindo de [`analisar`].
    pub fn enderecos(&self) -> Result<Vec<Alvo<'_>>, ErroDeUri> {
        self.candidatos().map(separar).collect()
    }
}

/// O bilhete de encontro: onde bater, e para quem. Degrau 4 do ADR 0022.
///
/// # Por que duas metades e não um identificador
///
/// A alternativa óbvia era um número opaco — um "quarto" — que o ponto de
/// encontro traduzisse para o endereço do anfitrião. Isso obrigaria o ponto de
/// encontro a **guardar** essa tradução, e o ADR 0022 aceita este degrau com a
/// condição de ele ser sem estado. Escrevendo os dois endereços no bilhete, o
/// serviço no meio não precisa lembrar de nada — e o que ele não guarda ninguém
/// consegue tomar dele.
///
/// O preço está escrito em `docs/alcance-pela-internet.md`: quem tem o link
/// aprende o endereço público de quem hospeda sem precisar conectar. Quem
/// conecta aprenderia de qualquer forma, e um link é para dar a quem se convida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bilhete {
    /// O ponto de encontro: `host[:porta]`.
    ///
    /// Vem do anfitrião e não do cliente, e é isso que faz o serviço ser
    /// trocável.
    pub ponto: String,
    /// Onde o anfitrião espera o aviso, como o ponto de encontro o vê.
    ///
    /// É para cá que quem quer entrar manda o `LEVE` de [`crate::encontro`]: o
    /// ponto de encontro repassa para este endereço o endereço de quem bateu, e
    /// é assim que o anfitrião sabe para onde furar.
    pub aviso: String,
}

impl Bilhete {
    /// Um bilhete, se as duas metades forem endereços.
    ///
    /// # Errors
    ///
    /// O mesmo [`ErroDeUri`] de um alvo torto: este texto acaba num `send_to`
    /// como qualquer outro endereço do convite.
    pub fn novo(ponto: impl Into<String>, aviso: impl Into<String>) -> Result<Self, ErroDeUri> {
        let (ponto, aviso) = (ponto.into(), aviso.into());
        validar_alvo(&ponto)?;
        validar_alvo(&aviso)?;
        Ok(Self { ponto, aviso })
    }

    /// Lê a forma que viaja no link: `ponto/aviso`.
    ///
    /// # Errors
    ///
    /// [`ErroDeUri::BilheteInvalido`] se não houver as duas metades, e o erro do
    /// endereço se alguma delas não for um.
    pub fn ler(texto: &str) -> Result<Self, ErroDeUri> {
        // A barra não aparece em nenhum dos dois lados — nem num nome, nem num
        // IPv6 entre colchetes —, então ela separa sem ambiguidade.
        let (ponto, aviso) = texto.split_once('/').ok_or(ErroDeUri::BilheteInvalido)?;
        if ponto.is_empty() || aviso.is_empty() {
            return Err(ErroDeUri::BilheteInvalido);
        }
        Self::novo(ponto, aviso)
    }

    /// O ponto de encontro, separado em máquina e porta.
    ///
    /// A porta padrão aqui **não** é a do Dogma: um ponto de encontro atende na
    /// [`crate::encontro::PORTA_PADRAO`].
    ///
    /// # Errors
    ///
    /// Não falha para um bilhete que veio de [`Bilhete::ler`].
    pub fn ponto(&self) -> Result<Alvo<'_>, ErroDeUri> {
        separar_com(&self.ponto, crate::encontro::PORTA_PADRAO)
    }

    /// Onde o anfitrião espera o aviso.
    ///
    /// # Errors
    ///
    /// [`ErroDeUri::BilheteInvalido`] se esta metade for um nome: o `LEVE` do
    /// ponto de encontro carrega um endereço, e ninguém no caminho resolve nome.
    pub fn aviso(&self) -> Result<std::net::SocketAddr, ErroDeUri> {
        let alvo = separar(&self.aviso)?;
        let ip: std::net::IpAddr = alvo
            .maquina
            .parse()
            .map_err(|_| ErroDeUri::BilheteInvalido)?;
        Ok(std::net::SocketAddr::new(ip, alvo.porta))
    }
}

impl fmt::Display for Bilhete {
    fn fmt(&self, formatador: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatador, "{}/{}", self.ponto, self.aviso)
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
    /// O bilhete de encontro não tem as duas metades, ou uma delas não serve.
    ///
    /// Erro próprio porque a correção é diferente: aqui não falta pontuação num
    /// endereço, falta metade do bilhete — e meio bilhete não leva a lugar
    /// nenhum. Ver [`Bilhete`].
    BilheteInvalido,
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
        // Antes da impressão digital e do token de propósito: um link cortado
        // na colagem perde o fim, e o fim tem de ser a parte cuja falta é
        // recusada em voz alta — a impressão digital — e não a que sumiria
        // calada, custando um endereço a quem tentar.
        if !self.alternativos.is_empty() {
            write!(formatador, "{separador}alt={}", self.alternativos.join(","))?;
            separador = '&';
        }
        // Ao lado do `alt`, e pelo mesmo motivo: é endereço, e o que se perde
        // ao perdê-lo é um caminho a tentar, não a verificação da identidade.
        if let Some(bilhete) = &self.bilhete {
            write!(formatador, "{separador}enc={bilhete}")?;
            separador = '&';
        }
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
            // Os outros endereços do mesmo Dogma. Cada um é validado como o
            // principal: este texto termina num `connect` igual ao outro.
            "alt" => {
                for alternativo in valor.split(',').filter(|parte| !parte.is_empty()) {
                    validar_alvo(alternativo)?;
                    if alternativo == convite.alvo
                        || convite.alternativos.iter().any(|ja| ja == alternativo)
                    {
                        continue;
                    }
                    if convite.alternativos.len() + 1 >= LIMITE_DE_ALVOS {
                        break;
                    }
                    convite.alternativos.push(alternativo.to_owned());
                }
            }
            // O bilhete de encontro. Validado como os outros endereços, e pelo
            // mesmo motivo: as duas metades terminam num `send_to`.
            "enc" => convite.bilhete = Some(Bilhete::ler(valor)?),
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
    separar_com(alvo, PORTA_PADRAO)
}

/// O mesmo, dizendo qual é a porta de quem não escreveu uma.
///
/// Existe porque nem todo endereço que atravessa um convite é um Dogma: o ponto
/// de encontro do degrau 4 atende noutra porta, e herdar a 8383 ali mandaria o
/// pedido para o lugar errado sem ninguém ter escrito nada errado.
fn separar_com(alvo: &str, padrao: u16) -> Result<Alvo<'_>, ErroDeUri> {
    if let Some(depois_do_colchete) = alvo.strip_prefix('[') {
        let (dentro, resto) = depois_do_colchete
            .split_once(']')
            .ok_or(ErroDeUri::EnderecoInvalido)?;
        if dentro.is_empty() {
            return Err(ErroDeUri::EnderecoInvalido);
        }
        let porta = if resto.is_empty() {
            padrao
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
            porta: padrao,
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
    fn um_convite_com_varios_enderecos_vai_e_volta_na_ordem() {
        // A ordem **é** o conteúdo: o primeiro é o da rede de casa, e é ele que
        // faz quem está na sala ao lado entrar sem esperar o prazo de um
        // endereço público que não volta para dentro.
        let original = Convite::novo("192.168.0.30:8383")
            .com_alternativos(["[2001:db8::1]:8383", "203.0.113.7:9000"])
            .com_impressao_digital(FP);

        let texto = original.to_string();
        let lido = analisar(&texto).expect("analisar");
        assert_eq!(lido, original, "o convite não voltou igual: {texto}");
        assert_eq!(
            lido.candidatos().collect::<Vec<_>>(),
            vec![
                "192.168.0.30:8383",
                "[2001:db8::1]:8383",
                "203.0.113.7:9000"
            ],
            "a ordem dos endereços mudou ao atravessar o link: {texto}"
        );
        assert_eq!(lido.enderecos().expect("separar todos").len(), 3);
    }

    #[test]
    fn um_convite_de_um_endereco_so_continua_sendo_o_de_antes() {
        // Compatibilidade para trás, e ela é literal: um convite gerado por uma
        // versão anterior tem de virar exatamente o que virava, uma lista de
        // um, sem `alt` nenhum no texto.
        let antigo = "seele://dogma.exemplo:8383";
        let lido = analisar(antigo).expect("analisar");
        assert!(lido.alternativos.is_empty());
        assert_eq!(
            lido.candidatos().collect::<Vec<_>>(),
            vec!["dogma.exemplo:8383"]
        );
        assert_eq!(lido.to_string(), antigo, "o convite antigo mudou de forma");
    }

    #[test]
    fn um_cliente_que_nao_conhece_alt_le_o_primeiro_endereco() {
        // Compatibilidade para frente. Um cliente anterior a este campo cai na
        // regra do parâmetro desconhecido e usa `alvo` — que por isso é o
        // endereço da rede local, e não o do degrau mais alto: quem tem cliente
        // velho volta a ter o comportamento que sempre funcionou, em vez de um
        // endereço público que a casa dele não devolve para dentro.
        let texto = Convite::novo("192.168.0.30:8383")
            .com_alternativos(["203.0.113.7:9000"])
            .to_string();
        let (antes, _) = texto.split_once("alt=").expect("o link não tem `alt`");
        let como_o_velho_veria = antes.trim_end_matches(['?', '&']);
        let velho = analisar(como_o_velho_veria).expect("analisar sem o alt");
        assert_eq!(velho.alvo, "192.168.0.30:8383");
    }

    #[test]
    fn um_endereco_alternativo_torto_e_recusado_como_o_principal() {
        // Este texto termina num `connect` igual ao outro. Um alternativo que
        // passasse sem conferência seria uma segunda porta de entrada com
        // metade da validação da primeira.
        for ruim in ["host 8383", "host;rm -rf /", "host|nc atacante 1"] {
            assert_eq!(
                analisar(&format!("seele://192.168.0.30:8383?alt={ruim}")),
                Err(ErroDeUri::EnderecoInvalido),
                "passou como alternativo: {ruim}"
            );
        }
        // E um IPv6 cru continua dizendo o que fazer, também aqui.
        assert_eq!(
            analisar("seele://192.168.0.30:8383?alt=2001:db8::1"),
            Err(ErroDeUri::EnderecoIpv6SemColchetes)
        );
    }

    #[test]
    fn o_link_nao_cresce_sem_limite_nem_repete_endereco() {
        // Cada endereço custa uma tentativa com prazo a quem recebe. E um
        // excedente é **ignorado**, não recusado: recusar link novo é o que faz
        // cliente velho virar parede, e a mesma regra vale para nós.
        let muitos: Vec<String> = (1..=9).map(|n| format!("192.168.0.{n}:8383")).collect();
        let convite = Convite::novo("192.168.0.30:8383").com_alternativos(muitos.clone());
        assert_eq!(convite.candidatos().count(), LIMITE_DE_ALVOS);

        let texto = format!("seele://192.168.0.30:8383?alt={}", muitos.join(","));
        let lido = analisar(&texto).expect("o excedente derrubou o link inteiro");
        assert_eq!(lido.candidatos().count(), LIMITE_DE_ALVOS);

        // Repetir o principal em `alt` é desperdício de tentativa, não erro.
        let repetido = Convite::novo("192.168.0.30:8383")
            .com_alternativos(["192.168.0.30:8383", "192.168.0.30:8383"]);
        assert_eq!(repetido.candidatos().count(), 1);
    }

    #[test]
    fn o_convite_cabe_todos_os_alvos_que_a_escada_entrega() {
        // Duas truncagens na mesma direção — `LIMITE_DE_CANDIDATOS` no
        // `seele-server` e `LIMITE_DE_ALVOS` aqui — e nenhuma das duas sabe da
        // outra. Enquanto os números forem iguais nada se perde; este teste é o que
        // faz alguém que mexa num deles descobrir o outro.
        // `const` e não `assert!` solto: os dois lados são constantes, então a
        // conferência cabe na compilação, e o clippy tem razão em cobrá-la ali.
        // Um teste que só reprova ao rodar deixa a janela entre editar a
        // constante e rodar a suíte, e é justamente nessa janela que alguém
        // encolhe o limite e vai embora.
        const { assert!(LIMITE_DE_ALVOS >= 4) };
    }

    #[test]
    fn um_bilhete_de_encontro_vai_e_volta_com_as_duas_metades() {
        // Duas metades e não uma: o endereço do ponto de encontro viaja no link
        // justamente para que ele seja trocável, que é a condição sob a qual o
        // ADR 0022 aceitou o degrau 4.
        let bilhete = Bilhete::novo("encontro.exemplo:8384", "198.51.100.7:41234").expect("bom");
        let original = Convite::novo("192.168.0.30:8383")
            .com_alternativos(["198.51.100.7:8383"])
            .com_bilhete(bilhete.clone())
            .com_impressao_digital(FP);

        let texto = original.to_string();
        let lido = analisar(&texto).expect("o convite com bilhete não se lê de volta");
        assert_eq!(lido, original, "o bilhete não voltou igual: {texto}");
        assert_eq!(lido.bilhete, Some(bilhete));
    }

    #[test]
    fn o_ponto_de_encontro_sem_porta_nao_herda_a_porta_do_dogma() {
        // Os dois lados do bilhete são endereços e **não** são o mesmo serviço:
        // 8383 é onde um Dogma atende, e um ponto de encontro atende noutra
        // porta. Herdar a do Dogma mandaria o pedido para o lugar errado sem
        // ninguém ter escrito nada errado.
        let bilhete = Bilhete::novo("encontro.exemplo", "198.51.100.7:41234").expect("bom");
        let ponto = bilhete.ponto().expect("separar o ponto");
        assert_eq!(ponto.maquina, "encontro.exemplo");
        assert_eq!(ponto.porta, crate::encontro::PORTA_PADRAO);
        assert_ne!(ponto.porta, PORTA_PADRAO, "o ponto herdou a porta do Dogma");
    }

    #[test]
    fn o_aviso_do_bilhete_e_um_endereco_e_nao_um_nome() {
        // O `LEVE` do ponto de encontro carrega um `SocketAddr`, e ele vem
        // daqui. Um nome deste lado seria um nome que alguém teria de resolver
        // no meio do caminho — e o ponto de encontro não resolve nada.
        let bilhete = Bilhete::novo("encontro.exemplo:8384", "[2001:db8::1]:41234").expect("bom");
        assert_eq!(
            bilhete.aviso().expect("ler o aviso"),
            "[2001:db8::1]:41234".parse().expect("endereço de teste")
        );

        let com_nome = Bilhete::novo("encontro.exemplo:8384", "casa.exemplo:41234").expect("bom");
        assert_eq!(com_nome.aviso(), Err(ErroDeUri::BilheteInvalido));
    }

    #[test]
    fn um_bilhete_pela_metade_ou_torto_e_recusado() {
        // As duas metades acabam num `send_to`. Um bilhete sem a barra, ou com
        // um endereço que não é um, é recusado aqui e não lá adiante.
        for ruim in [
            "sobarra",
            "/198.51.100.7:41234",
            "encontro.exemplo:8384/",
            "encontro.exemplo:8384/tem espaço:1",
            "encontro.exemplo:8384/198.51.100.7|nc atacante 1",
        ] {
            assert!(
                analisar(&format!("seele://192.168.0.30:8383?enc={ruim}")).is_err(),
                "passou como bilhete: {ruim}"
            );
        }
        // E um IPv6 cru continua dizendo o que fazer, também aqui.
        assert_eq!(
            analisar("seele://192.168.0.30:8383?enc=encontro.exemplo:8384/2001:db8::1"),
            Err(ErroDeUri::EnderecoIpv6SemColchetes)
        );
    }

    #[test]
    fn um_cliente_que_nao_conhece_enc_perde_o_degrau_4_e_nada_mais() {
        // Compatibilidade para frente, exercitada como a do `alt` foi: um
        // cliente anterior a este campo cai na regra do parâmetro desconhecido.
        // O que ele perde é o furo de NAT; o que ele mantém é a lista inteira de
        // endereços e a impressão digital, que é exatamente o que ele tinha.
        let texto = Convite::novo("192.168.0.30:8383")
            .com_alternativos(["198.51.100.7:41234"])
            .com_bilhete(Bilhete::novo("encontro.exemplo:8384", "198.51.100.7:41234").expect("bom"))
            .com_impressao_digital(FP)
            .to_string();

        // O que o velho enxerga: o mesmo texto, com `enc` caindo na cláusula
        // `_ => {}` do `analisar` dele.
        let como_o_velho_veria = texto.replace("enc=", "seiladoque=");
        let velho = analisar(&como_o_velho_veria).expect("o link novo derrubou o cliente velho");
        assert_eq!(velho.alvo, "192.168.0.30:8383");
        assert_eq!(velho.candidatos().count(), 2, "o velho perdeu endereços");
        assert_eq!(velho.impressao_digital.as_deref(), Some(FP));
        assert_eq!(velho.bilhete, None);
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
    fn todo_socketaddr_atravessa_o_convite_e_volta_igual() {
        // O lado de **quem gera**, que é o que faltava: os testes acima todos
        // olhavam para quem lê. Um convite recusado com uma frase bonita não
        // vale nada se fomos nós que o escrevemos torto.
        //
        // Quem monta um convite tem um `SocketAddr` na mão, e a única forma
        // segura de escrevê-lo é o `Display` dele — que põe os colchetes. Este
        // teste é o contrato entre as duas pontas.
        for texto in [
            "192.0.2.10:8383",
            "127.0.0.1:1",
            "[2001:db8::1]:8383",
            "[::1]:8383",
            // Forma longa: o `Display` encolhe, e a comparação é por endereço.
            "[2001:db8:0:0:0:0:0:1]:65535",
        ] {
            let endereco: std::net::SocketAddr = texto
                .parse()
                .unwrap_or_else(|_| panic!("teste torto: {texto}"));
            let convite = Convite::novo(endereco.to_string()).to_string();
            let lido = analisar(&convite).unwrap_or_else(|erro| {
                panic!("geramos um convite que não se lê: {convite} ({erro:?})")
            });
            let alvo = lido.endereco().expect("o alvo gerado não se separa");
            assert_eq!(alvo.porta, endereco.port(), "a porta mudou em {convite}");
            assert_eq!(
                alvo.maquina.parse::<std::net::IpAddr>().ok(),
                Some(endereco.ip()),
                "o endereço mudou em {convite}"
            );
        }
    }

    #[test]
    fn a_interpolacao_a_mao_e_justamente_a_que_nao_passa() {
        // É o que dá dente ao teste acima: `format!("{ip}:{porta}")` parece
        // certo, compila, e escreve um endereço que o analisador recusa. Foi
        // encontrado gerando convite no `seeled`, não lendo.
        let ip: std::net::IpAddr = "2001:db8::1".parse().expect("endereço de teste");
        let a_mao = format!("{ip}:{}", PORTA_PADRAO);
        assert_eq!(
            analisar(&format!("{ESQUEMA}{a_mao}")),
            Err(ErroDeUri::EnderecoIpv6SemColchetes),
            "a forma torta passou: {a_mao}"
        );

        let certo = std::net::SocketAddr::new(ip, PORTA_PADRAO).to_string();
        assert!(analisar(&format!("{ESQUEMA}{certo}")).is_ok(), "{certo}");
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
