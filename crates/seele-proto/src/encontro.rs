//! O que se fala com um ponto de encontro. Degrau 4 do ADR 0022.
//!
//! ```text
//! SEELE-ENC/1 ONDE <marca>
//! SEELE-ENC/1 LEVE <destino> <marca>
//! SEELE-ENC/1 MORO <marca>
//! SEELE-ENC/1 QUEM <marca>
//! SEELE-ENC/1 AQUI <marca> <endereço>
//! ```
//!
//! Cinco linhas, e é o protocolo inteiro. Quatro entram no ponto de encontro e
//! uma sai dele. Três delas só sabem **dizer de onde um pacote veio** — para
//! quem o mandou (`ONDE`, `MORO`) ou para um terceiro endereço (`LEVE`). A
//! quarta, `QUEM`, é a única que precisa de memória, e a seção sobre o quarto
//! diz o que ela custa.
//!
//! # Por que isto basta para furar um NAT
//!
//! Nenhuma máquina atrás de NAT sabe o próprio endereço público: o roteador
//! reescreve isso na saída e o interior nunca vê o resultado (ADR 0022). Alguém
//! de fora precisa contar — e é só isso que o ponto de encontro faz.
//!
//! O anfitrião pergunta `ONDE` e aprende o endereço público da escuta de avisos
//! dele; pergunta `LEVE <aquele endereço>` **pelo socket do servidor** e aprende o
//! endereço público do servidor, que é o que vai no convite. Quem recebe o convite
//! manda um `LEVE <endereço de avisos do anfitrião>` pelo socket com que vai
//! conectar; o ponto de encontro conta ao anfitrião de onde aquele pacote veio,
//! e o anfitrião manda alguns pacotes para lá. Os dois roteadores passam a ter
//! uma saída registrada para o outro lado, e daí em diante o tráfego é direto.
//!
//! # O quarto, e o que ele custa
//!
//! **O ADR 0022 recusou isto, e a recusa foi revista em 03/09/2026.** O texto
//! dele diz: *«a alternativa óbvia era um número opaco que o ponto de encontro
//! traduzisse para o endereço do anfitrião. Isso obrigaria o ponto de encontro a
//! guardar essa tradução.»* O argumento continua correto; o que mudou foi o
//! preço do outro lado.
//!
//! Sem memória, o endereço de um anfitrião atrás de NAT vive só enquanto o
//! servidor está no ar: o mapeamento nasce quando um pacote sai, e o roteador dá
//! outro na abertura seguinte. O `seele://` e a lista de servidores conhecidos
//! guardam aquele endereço, e ele morre no fechar. Foi relatado assim: *«o ip
//! que fica salvo na lista de servidores ainda dá problema de reconexão,
//! possivelmente porque a porta muda quando abre e fecha o server. Se não, a
//! lista de servidores fica inútil.»*
//!
//! Então o ponto de encontro passa a guardar **uma** coisa: `marca → endereço`,
//! em memória, com prazo. Nunca em disco, nunca em log. O anfitrião escreve com
//! `MORO` no mesmo pacote de quinze segundos que já mandava; quem tem o link
//! pergunta com `QUEM`.
//!
//! ## O que isso entrega a quem opera o ponto de encontro
//!
//! **Que uma marca está no ar, e em que endereço.** É metadado vivo, e é
//! exatamente o que o ADR não queria que existisse. O que ele **não** entrega
//! continua não entregando: quem falou com quem não passa por aqui, e a marca
//! são os 16 primeiros dos 64 caracteres da impressão digital do servidor (com
//! uma letra no fim, nas duas marcas que moram no quarto; ver [`Marcas`]) — um
//! número, não um nome.
//!
//! ## Por que ninguém rouba o lugar de ninguém
//!
//! Qualquer um pode mandar `MORO` com a marca de outro, e mandar quem pergunta
//! para o endereço errado. Três coisas contêm isso, e nenhuma delas é
//! autenticação — este serviço não tem chave nenhuma para conferir:
//!
//! 1. **Quem chega confere a impressão digital dentro do TLS** (ADR 0003). A
//!    esperada vem do link ou da lista de conhecidos, e é conferida no aperto
//!    de mão, antes de qualquer `Hello`: um endereço errado vindo do quarto
//!    falha ali, e o convite, a senha e o apelido não chegam a quem atendeu com
//!    a chave errada. O prejuízo é não entrar. Isso vale também onde esta
//!    máquina já fixou uma chave naquele endereço: o endereço que o quarto
//!    devolve entra como um candidato que a pessoa não escolheu (quando é o
//!    próprio endereço escolhido, não entra de novo), e ali um pino que
//!    confere não passa por cima da impressão esperada (o adendo de
//!    2026-09-29 ao ADR 0003). Se a chave fixada ali mudou, a conexão é
//!    recusada do mesmo jeito.
//! 2. **Quem escreveu primeiro fica**, enquanto o prazo não vencer. O anfitrião
//!    reavive as duas marcas dele a cada quinze segundos, então o lugar dele
//!    só está livre em três janelas: com ele fora do ar há mais que o prazo;
//!    quando ele não se registra naquele ponto (com IPv4 global ele não abre o
//!    degrau 4, e `$SEELE_ENCONTRO` desliga o degrau ou troca o ponto, enquanto
//!    o bilhete que a lista de conhecidos guardou continua apontando o de
//!    antes); e logo depois de o ponto reiniciar, até o próximo registro dele.
//!    Nas três, o que se toma é o lugar de quem não está lá, ou ainda não
//!    voltou a se registrar. A marca da escuta de um anfitrião 0.15.0 é a
//!    exceção: ele nunca a registra, e ela está sempre livre.
//! 3. **A escuta só vale no IP do servidor.** O endereço da escuta vira o
//!    destino do `LEVE` de quem chega, e o `LEVE` sai antes do TLS: o ponto
//!    repassa a esse destino o IP, a porta e o instante de quem tenta chegar.
//!    Quem chega só usa a escuta que o quarto devolve quando ela mora no mesmo
//!    IP que o servidor da mesma resposta, porque a escuta e o servidor de um
//!    anfitrião saem da mesma máquina, pelo mesmo IP público. A lista de
//!    conhecidos não a guarda (`seele_core::encontro::bilhete_desta_volta`).
//!
//! O que sobra num servidor guardado, que continua nesta versão: quem toma as
//! duas marcas, ou só a da escuta saindo pelo mesmo IP público que o anfitrião,
//! recebe pelo `LEVE` o IP, a porta e o instante de quem tenta chegar. O
//! conteúdo, o convite, a senha e o apelido não saem: o TLS recusa o servidor
//! errado (o item 1). A sala pessoal não tem essa sobra: ela se registra pela
//! versão 2, abaixo.
//!
//! # A versão 2: o registro assinado (ADR 0062)
//!
//! A sala pessoal só é achável pelo quarto, porque um código não carrega
//! endereço nenhum. Ela se registra com um `MORO2` assinado pela chave da pessoa
//! ([`Moro2`], [`crate::assinatura::moro2`]) e com o cookie que o ponto deu ao
//! IP dela (um `MORO2` sem ele ouve [`Resposta2::Desafio`]), e quem liga
//! pergunta com um `QUEM2`,
//! que sempre responde: o endereço e as dicas de rede local
//! ([`Resposta2::Aqui2`]), ou [`Resposta2::Ninguem`]. As duas versões convivem
//! no mesmo ponto, separadas por [`MAGIA2`] e por [`TAMANHO2`], e um ponto de
//! antes da versão 2 cala para ela. O ponto guarda o registro assinado num
//! quarto separado (`seele_encontro::assinado`), e é lá que estão as regras.
//!
//! # O que ele não faz, e por construção
//!
//! **Não guarda nada além do quarto.** [`responder`] continua sendo uma função
//! livre: não tem `self`, não tem tabela, não recebe estado. Um `LEVE` funciona
//! sem nenhum `ONDE` antes dele, e reiniciar o processo não muda resposta
//! nenhuma dela. O quarto mora fora daqui, em `seele_encontro::Quarto`, e um
//! ponto de encontro que não o queira continua chamando esta função e continua
//! correto.
//!
//! **Não repassa conteúdo.** A resposta é **montada** aqui, campo a campo, e
//! nunca copiada do pedido: o que sai é o verbo, a marca (alfanumérica, curta) e
//! um endereço que o próprio ponto observou. Não há caminho por onde um byte
//! escolhido por quem manda chegue a quem recebe.
//!
//! **Não decide em quem se confia.** Quem chega com o bilhete do ponto de
//! encontro e com uma impressão a conferir lê a resposta do quarto (o `AQUI`
//! que responde a um `QUEM`), e o endereço que ela traz entra na lista de
//! candidatos, na frente dos guardados. Um ponto de encontro hostil, ou quem
//! ocupou a marca, consegue mandar essa conexão para o endereço errado. O que
//! ele não escolhe é a impressão digital, que sai do `seele://` ou da lista de
//! conhecidos e é conferida dentro do TLS, antes de qualquer `Hello` (ADR
//! 0003): um servidor com a chave errada falha ali e não vira conexão com o
//! impostor, também num endereço que esta máquina já fixou com a chave de
//! outro, como o item 1 acima escreve. E a conferência protege apenas quando
//! há impressão digital contra a qual conferir. A escuta que a resposta traz
//! não passa por TLS nenhum, e a barreira dela é o item 3 acima.
//!
//! # Amplificação, e por que os pedidos são gordos
//!
//! `LEVE` manda um pacote para um endereço que quem pede escolheu. Isso é um
//! refletor, e um refletor que responde mais do que recebe é uma arma. Por isso
//! todo datagrama tem [`TAMANHO`] bytes — os pedidos vão com enchimento — e a
//! resposta nunca é maior que o pedido que a causou. O ganho de banda de quem
//! abusar é, no máximo, 1:1. Na versão 2 a regra é a mesma, com [`TAMANHO2`].
//!
//! O resto das defesas está em [`responder`]: destino global, porta alta, e
//! nada que não seja exatamente uma destas três linhas.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Porta padrão de um ponto de encontro.
///
/// 8384, ao lado do 8383 do servidor: quem for abrir uma no firewall vai abrir as
/// duas na mesma tarde.
pub const PORTA_PADRAO: u16 = 8383 + 1;

/// O que todo datagrama deste protocolo começa dizendo.
///
/// O `/1` é a versão. Um ponto de encontro que receba `/2` não responde, que é
/// o comportamento certo: ele não sabe o que fazer com aquilo, e adivinhar é
/// pior que calar.
pub const MAGIA: &str = "SEELE-ENC/1";

/// O tamanho de todo datagrama, pedido ou resposta.
///
/// Fixo de propósito. Ver a nota sobre amplificação no cabeçalho: um pedido
/// menor que a resposta que ele provoca transformaria o ponto de encontro num
/// amplificador, e quem paga por isso é uma vítima que nunca ouviu falar deste
/// projeto. Cabe o maior endereço possível — um IPv6 com colchetes e porta —
/// com folga.
pub const TAMANHO: usize = 96;

/// O caractere de enchimento. Escolhido por ser visível num `tcpdump`.
const ENCHIMENTO: u8 = b'.';

/// Quantos caracteres uma marca pode ter.
///
/// 32 é a metade dos 64 caracteres de uma impressão digital, com folga para o
/// uso previsto: os 16 primeiros, e uma letra no fim das marcas que moram no
/// quarto — ver [`Marca`] e [`Marcas`].
pub const LIMITE_DA_MARCA: usize = 32;

/// A etiqueta que volta na resposta, e o único campo que quem pede escolhe.
///
/// Alfanumérica e curta, e as duas coisas são defesa: ela é o único pedaço do
/// pedido que reaparece na resposta, então ela é também o único caminho por onde
/// alguém tentaria enfiar bytes escolhidos num datagrama que outra pessoa vai
/// receber.
///
/// # Para que ela serve de verdade
///
/// Para o anfitrião saber quem bateu. Ele conhece a marca que ele mesmo mandou,
/// e conhece a que quem teve o convite vai usar — os primeiros dígitos da
/// impressão digital dele. Quem entra os tira do `seele://`, ou da lista de
/// conhecidos, que guardou a impressão que a conexão aceitou; e quem opera o
/// ponto de encontro os lê no começo das marcas que o anfitrião registra no
/// quarto. Uma marca que não é nenhuma das duas é ruído da internet, e não vira
/// furo de NAT nenhum.
///
/// Não é autenticação e não tenta ser: quem teve o link, tem. É o que impede um
/// varredor de portas de fazer um servidor mandar pacotes para onde ele quiser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marca(String);

impl Marca {
    /// Uma marca, se este texto puder ser uma.
    ///
    /// `None` para vazio, longo demais, ou com qualquer coisa que não seja
    /// letra ou número.
    #[must_use]
    pub fn nova(texto: &str) -> Option<Self> {
        if texto.is_empty() || texto.len() > LIMITE_DA_MARCA {
            return None;
        }
        texto
            .chars()
            .all(|c| c.is_ascii_alphanumeric())
            .then(|| Self(texto.to_owned()))
    }

    /// O texto dela.
    #[must_use]
    pub fn texto(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Marca {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// As três marcas de um anfitrião no quarto.
///
/// # Por que três, e por que diferentes
///
/// O quarto guarda **um endereço por marca**. Um anfitrião tem dois sockets
/// atrás do mesmo IP público, em portas diferentes: o do servidor, para onde
/// quem chega conecta, e a escuta de avisos, para onde vai o `LEVE`. Daí saem
/// [`Marcas::servidor`] e [`Marcas::escuta`]. A terceira, [`Marcas::aviso`],
/// não mora no quarto: é a que o `LEVE` de quem chega traz, e a que o
/// anfitrião confere antes de furar.
///
/// **A escuta nunca tem a marca do aviso**, e é isso que filtra o eco. O
/// `MORO` da escuta é respondido com um `AQUI` que volta para a própria
/// escuta, com a marca do `MORO`. Com a marca do aviso, esse `AQUI` passaria
/// no filtro do anfitrião, e o anfitrião furaria o caminho para si mesmo a
/// cada reavivamento.
///
/// # Um lugar só
///
/// Quem hospeda registra com estas marcas, quem bate traz a do aviso, e quem
/// procura pergunta pelas outras duas. Enquanto cada lado as derivava por
/// conta própria, os dois divergiram: o anfitrião registrava a escuta como
/// `anfitriao`, o cliente perguntava pela impressão digital, e a resposta
/// nunca veio (análise de 22/09/2026, §2.1).
///
/// # Por que os campos são privados
///
/// Com os campos públicos, quem estivesse fora deste módulo montava
/// `Marcas { escuta: aviso.clone(), … }` sem passar por [`Marcas::do_servidor`],
/// e a escuta com a marca do aviso é exatamente o eco que vira furo contra si
/// mesmo. O único jeito de ter `Marcas` é [`Marcas::do_servidor`], e o que se
/// lê delas sai por [`Marcas::aviso`], [`Marcas::escuta`] e
/// [`Marcas::servidor`].
///
/// O exemplo abaixo é o guarda disto: ele **não pode compilar**, e o teste de
/// documentação reprova no dia em que um campo voltar a ser público.
///
/// ```compile_fail,E0451
/// use seele_proto::encontro::{Marca, Marcas};
///
/// let aviso = Marca::nova("3cbcfb0212da738f").expect("é uma marca");
/// let _eco = Marcas {
///     aviso: aviso.clone(),
///     escuta: aviso.clone(),
///     servidor: aviso,
/// };
/// ```
///
/// **O código de erro (`E0451`) só é conferido no nightly.** No stable, um
/// `compile_fail` passa com qualquer erro: com um nome trocado ou um import
/// que sumiu, o exemplo de cima deixaria de compilar por outro motivo e o
/// guarda ficaria vazio (medido no 1.97.1, com os campos públicos e um
/// `E0599`). O gêmeo abaixo, com os mesmos imports e a mesma `Marca::nova`,
/// **tem** de compilar: ele prova que os imports e a `Marca::nova` de cima
/// estão certos, e só isso. Um campo renomeado ou a mais em `Marcas`, com os
/// campos públicos, daria outro erro (`E0560`, `E0063`) que o stable também
/// aceita, e esse caso o gêmeo não cobre.
///
/// ```
/// use seele_proto::encontro::{Marca, Marcas};
///
/// let aviso = Marca::nova("3cbcfb0212da738f").expect("é uma marca");
/// let marcas = Marcas::do_servidor(
///     "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9",
/// )
/// .expect("uma impressão digital dá marcas");
/// assert_eq!(marcas.aviso(), &aviso);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marcas {
    aviso: Marca,
    escuta: Marca,
    servidor: Marca,
}

impl Marcas {
    /// A que o `LEVE` de quem chega traz, e a que o anfitrião confere antes de
    /// furar.
    #[must_use]
    pub fn aviso(&self) -> &Marca {
        &self.aviso
    }

    /// Sob a qual a escuta de avisos se registra no quarto.
    #[must_use]
    pub fn escuta(&self) -> &Marca {
        &self.escuta
    }

    /// Sob a qual o socket do servidor se registra no quarto.
    #[must_use]
    pub fn servidor(&self) -> &Marca {
        &self.servidor
    }

    /// As marcas de um servidor guardado, tiradas da impressão digital.
    ///
    /// Partem dos dezesseis primeiros caracteres, que o `seele://` carrega em
    /// `fp=`. O aviso são esses dezesseis; a escuta, eles e um `e`; o servidor,
    /// eles e um `s`.
    ///
    /// **O aviso e o servidor são os da v0.15.0 e não mudam.** Um cliente que
    /// não atualizou continua batendo com `LEVE <fp16>` e perguntando
    /// `QUEM <fp16>s`, e trocar qualquer um dos dois o deixaria falando sozinho.
    ///
    /// Devolve `None` para um texto que não forma marca: curto demais, ou com
    /// algo que não é letra nem número **em qualquer posição**, e não só nos
    /// dezesseis primeiros.
    #[must_use]
    pub fn do_servidor(impressao_digital: &str) -> Option<Self> {
        if !impressao_digital
            .chars()
            .all(|caractere| caractere.is_ascii_alphanumeric())
        {
            return None;
        }
        let prefixo = impressao_digital.get(..16)?;
        Some(Self {
            aviso: Marca::nova(prefixo)?,
            escuta: Marca::nova(&format!("{prefixo}e"))?,
            servidor: Marca::nova(&format!("{prefixo}s"))?,
        })
    }

    /// As marcas da sala pessoal de quem tem este código (spec de 23/09/2026,
    /// «O código»).
    ///
    /// O aviso é `p` e os 15 símbolos de dado do código; a escuta, eles e um
    /// `e`; o servidor, eles e um `s`. **O `p` separa este espaço do das
    /// marcas de servidor**, que saem de uma impressão em hexadecimal: `p` não
    /// é hexadecimal, então uma sala e um servidor guardado nunca dividem
    /// marca.
    ///
    /// **Formato congelado, com a caixa das letras.** [`Marca::nova`] não
    /// normaliza e o ponto compara byte a byte: os símbolos vão maiúsculos,
    /// como [`crate::codigo::Codigo::dado`] os dá, e o `p`, o `e` e o `s`
    /// minúsculos. Mudar qualquer um deixa todo código já distribuído falando
    /// sozinho no quarto.
    ///
    /// Não falha: 15 símbolos alfanuméricos e uma letra cabem com folga em
    /// [`LIMITE_DA_MARCA`], e `toda_marca_da_sala_e_uma_marca_valida` confere
    /// que [`Marca::nova`] aceitaria cada uma.
    #[must_use]
    pub fn da_sala(codigo: &crate::codigo::Codigo) -> Self {
        let aviso = format!("p{}", codigo.dado());
        Self {
            escuta: Marca(format!("{aviso}e")),
            servidor: Marca(format!("{aviso}s")),
            aviso: Marca(aviso),
        }
    }
}

/// O que se pede a um ponto de encontro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pedido {
    /// "De onde você vê que este pacote veio?" A resposta volta para a origem.
    Onde {
        /// A etiqueta que volta na resposta.
        marca: Marca,
    },
    /// "Conte a este endereço de onde você vê que este pacote veio."
    ///
    /// É a apresentação inteira: o pacote que o anfitrião recebe por causa
    /// disto é o aviso de que alguém quer entrar, **e** o endereço para onde
    /// furar.
    Leve {
        /// Quem recebe o aviso.
        destino: SocketAddr,
        /// A etiqueta que vai nele.
        marca: Marca,
    },
    /// "Guarde que quem manda isto é esta marca, e me diga onde eu estou."
    ///
    /// O **quarto**: o único pedido deste protocolo que pede memória. Ver o
    /// cabeçalho do módulo, seção «O quarto, e o que ele custa».
    ///
    /// É um [`Self::Onde`] que também registra, e não um verbo à parte, porque
    /// o anfitrião já manda um `ONDE` a cada quinze segundos para manter o
    /// mapeamento de NAT vivo. Registrar de graça naquele pacote é o que faz
    /// isto não custar tráfego nenhum a mais.
    Moro {
        /// Sob que nome guardar. São os primeiros dígitos da impressão digital
        /// do servidor, que é o que o `seele://` carrega.
        marca: Marca,
    },
    /// "Onde está a marca X agora?"
    ///
    /// A resposta volta **para quem perguntou**, e nunca para um terceiro: isto
    /// não é um refletor como o [`Self::Leve`], e por isso não precisa das
    /// defesas de destino que aquele precisa.
    Quem {
        /// De quem se quer o endereço.
        marca: Marca,
    },
}

/// O que um ponto de encontro tem a dizer, e para quem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resposta {
    /// Para onde este datagrama vai.
    pub destino: SocketAddr,
    /// Os bytes, já com [`TAMANHO`].
    pub datagrama: Vec<u8>,
}

/// Monta um `ONDE`.
#[must_use]
pub fn onde(marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} ONDE {marca}"))
}

/// Monta um `LEVE`.
#[must_use]
pub fn leve(destino: SocketAddr, marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} LEVE {destino} {marca}"))
}

/// Monta um `AQUI`.
#[must_use]
pub fn aqui(marca: &Marca, origem: SocketAddr) -> Vec<u8> {
    encher(format!("{MAGIA} AQUI {marca} {origem}"))
}

/// Monta um `MORO`.
#[must_use]
pub fn moro(marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} MORO {marca}"))
}

/// Monta um `QUEM`.
#[must_use]
pub fn quem(marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} QUEM {marca}"))
}

/// Monta um `FURO`: o pacote que abre o caminho, mandado de uma ponta à outra.
///
/// **Não é para o ponto de encontro**, e ele não responde a isto — `analisar`
/// não conhece este verbo. É o pacote que o anfitrião manda ao endereço que o
/// aviso trouxe, para que o roteador dele passe a ter uma saída registrada para
/// lá. Quem recebe descarta: do outro lado quem está lendo o socket é o QUIC.
///
/// Existe como verbo nomeado por uma razão só: quem estiver olhando um
/// `tcpdump` para entender por que uma conexão não sobe merece ver o que é
/// aquele pacote de 96 bytes, em vez de bytes anônimos.
#[must_use]
pub fn furo(marca: &Marca) -> Vec<u8> {
    encher(format!("{MAGIA} FURO {marca}"))
}

/// Completa a linha até [`TAMANHO`], para que nada amplifique.
fn encher(mut linha: String) -> Vec<u8> {
    linha.push('\n');
    let mut bytes = linha.into_bytes();
    bytes.resize(TAMANHO, ENCHIMENTO);
    bytes
}

/// A decisão inteira de um ponto de encontro, sem estado e sem rede.
///
/// Recebe o datagrama e de onde ele veio; devolve o que mandar e para quem, ou
/// `None` para calar. É uma função livre porque a decisão sobre `ONDE` e `LEVE`
/// é: o que ela responde depende só do datagrama e de onde ele veio, e não é
/// possível fazê-la guardar nada entre um datagrama e o outro sem mudar esta
/// assinatura. O processo que a chama guarda uma coisa, o quarto
/// (`seele_encontro::Quarto`), por onde passam `MORO` e `QUEM`; ele fica fora
/// daqui, e o custo dele está escrito no cabeçalho deste módulo.
///
/// # Quando ela cala, e por quê
///
/// - Tamanho diferente de [`TAMANHO`] — inclusive **menor**, e essa é a defesa
///   contra amplificação: sem ela, um pedido curto provocaria uma resposta
///   longa contra uma origem forjada.
/// - Não começa com [`MAGIA`], ou traz um verbo desconhecido, ou uma marca com
///   caractere que marca não tem. Nada aqui tenta adivinhar.
/// - `AQUI`. É o que sai daqui, não o que entra: um ponto de encontro que
///   respondesse a respostas seria um laço entre dois pontos de encontro.
/// - `LEVE` para um endereço que não é da internet — privado, loopback,
///   multicast, CGNAT, documentação. Um refletor que alcança rede privada é uma
///   forma de bater em máquinas que não estão na internet a partir de uma que
///   está.
/// - `LEVE` para porta abaixo de 1024. É onde moram os serviços que respondem a
///   qualquer coisa (DNS, NTP), e mandar pacote para lá em nome de outra pessoa
///   é o começo de uma reflexão em cadeia.
#[must_use]
pub fn responder(datagrama: &[u8], origem: SocketAddr) -> Option<Resposta> {
    responder_em(datagrama, origem, Vizinhanca::Internet)
}

/// Até onde um `LEVE` pode apontar.
///
/// Existe porque a regra certa na internet — só endereço da internet —
/// impossibilita experimentar o mecanismo numa máquina só, e um mecanismo que
/// não dá para experimentar é um que ninguém confere antes de apontar o mundo
/// para ele.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Vizinhanca {
    /// Só endereços da internet. O que um ponto de encontro público usa.
    #[default]
    Internet,
    /// Também rede local e loopback.
    ///
    /// Para o teste automatizado e para quem sobe um ponto de encontro dentro
    /// da própria rede — numa VPN de rede, por exemplo — antes de subir um de
    /// verdade. Num ponto público isto é um refletor apontado para dentro da
    /// rede de quem hospeda, e é por isso que não é o padrão.
    TambemAqui,
}

/// [`responder`], dizendo até onde um `LEVE` alcança.
#[must_use]
pub fn responder_em(
    datagrama: &[u8],
    origem: SocketAddr,
    vizinhanca: Vizinhanca,
) -> Option<Resposta> {
    match analisar(datagrama)? {
        // `MORO` responde como `ONDE` **e nada mais aqui dentro**: esta função
        // não tem onde guardar, e é o que a assinatura dela promete. Quem guarda
        // é `seele_encontro::Quarto`, do lado de fora, e é lá que o custo do
        // quarto está escrito. Um ponto de encontro que não queira o quarto
        // continua chamando esta função e continua correto — ele só passa a
        // responder `MORO` como se fosse `ONDE`, que é a resposta honesta de
        // quem não guardou.
        Pedido::Onde { marca } | Pedido::Moro { marca } => Some(Resposta {
            destino: origem,
            datagrama: aqui(&marca, origem),
        }),
        // E `QUEM` cala, aqui. Sem quarto não há endereço guardado, e inventar
        // um seria mandar quem perguntou para o lugar errado. Calar é a resposta
        // que este módulo dá para tudo o que ele não sabe.
        Pedido::Quem { .. } => None,
        Pedido::Leve { destino, marca } => {
            let permitido = match vizinhanca {
                Vizinhanca::Internet => alcancavel(destino),
                // A porta continua valendo: refletir para a porta 53 de um
                // resolvedor da rede local é o mesmo estrago, mais perto.
                Vizinhanca::TambemAqui => destino.port() >= 1024,
            };
            if !permitido {
                return None;
            }
            Some(Resposta {
                destino,
                datagrama: aqui(&marca, origem),
            })
        }
    }
}

/// Lê um pedido, sem julgar o destino. Ver [`responder`].
#[must_use]
pub fn analisar(datagrama: &[u8]) -> Option<Pedido> {
    let linha = linha_de(datagrama)?;
    let resto = linha.strip_prefix(MAGIA)?.strip_prefix(' ')?;
    let (verbo, resto) = resto.split_once(' ')?;
    match verbo {
        "ONDE" => Some(Pedido::Onde {
            marca: Marca::nova(resto)?,
        }),
        "MORO" => Some(Pedido::Moro {
            marca: Marca::nova(resto)?,
        }),
        "QUEM" => Some(Pedido::Quem {
            marca: Marca::nova(resto)?,
        }),
        "LEVE" => {
            let (destino, marca) = resto.split_once(' ')?;
            Some(Pedido::Leve {
                destino: destino.parse().ok()?,
                marca: Marca::nova(marca)?,
            })
        }
        _ => None,
    }
}

/// Lê um `AQUI`: a marca que voltou e o endereço que o ponto de encontro viu.
///
/// É o que o anfitrião chama ao receber um aviso. Devolve `None` para qualquer
/// outra coisa, incluindo os pedidos — quem espera resposta não trata pedido.
#[must_use]
pub fn ler_aqui(datagrama: &[u8]) -> Option<(Marca, SocketAddr)> {
    let linha = linha_de(datagrama)?;
    let resto = linha.strip_prefix(MAGIA)?.strip_prefix(' ')?;
    let resto = resto.strip_prefix("AQUI ")?;
    let (marca, endereco) = resto.split_once(' ')?;
    Some((Marca::nova(marca)?, endereco.parse().ok()?))
}

/// A linha útil de um datagrama do tamanho certo, sem o enchimento.
fn linha_de(datagrama: &[u8]) -> Option<&str> {
    if datagrama.len() != TAMANHO {
        return None;
    }
    let texto = std::str::from_utf8(datagrama).ok()?;
    texto.split('\n').next()
}

/// Se um `LEVE` pode apontar para cá.
///
/// A pergunta não é "este endereço existe" e sim "mandar um pacote para cá em
/// nome de um desconhecido faz mal a alguém". Ver [`responder`].
#[must_use]
pub fn alcancavel(destino: SocketAddr) -> bool {
    // Serviços de sistema respondem a qualquer coisa; um pacote nosso batendo
    // lá em nome de outra pessoa é o primeiro passo de uma reflexão em cadeia.
    if destino.port() < 1024 {
        return false;
    }
    match destino.ip() {
        IpAddr::V4(quatro) => global_v4(quatro),
        IpAddr::V6(seis) => global_v6(seis),
    }
}

/// Se este IPv4 é da internet.
///
/// Escrito à mão porque `Ipv4Addr::is_global` ainda não é estável e o MSRV do
/// ADR 0011 não espera por ele. A faixa `100.64.0.0/10` está aqui pelo mesmo
/// motivo que está no degrau 3: a RFC 6598 a reservou para CGNAT, e ela não é
/// privada para a `std`.
fn global_v4(endereco: Ipv4Addr) -> bool {
    let [a, b, _, _] = endereco.octets();
    let cgnat = a == 100 && (64..128).contains(&b);
    !endereco.is_private()
        && !endereco.is_loopback()
        && !endereco.is_link_local()
        && !endereco.is_broadcast()
        && !endereco.is_documentation()
        && !endereco.is_unspecified()
        && !endereco.is_multicast()
        && !cgnat
        && a != 0
        && a < 240
}

/// Se este IPv6 é da internet.
fn global_v6(endereco: Ipv6Addr) -> bool {
    let primeiro = endereco.segments().first().copied().unwrap_or_default();
    !endereco.is_loopback()
        && !endereco.is_unspecified()
        && !endereco.is_multicast()
        // fe80::/10, link-local.
        && primeiro & 0xffc0 != 0xfe80
        // fc00::/7, unique-local.
        && primeiro & 0xfe00 != 0xfc00
        // 2001:db8::/32, documentação (RFC 3849).
        && !(primeiro == 0x2001 && endereco.segments().get(1) == Some(&0x0db8))
}

// ---------------------------------------------------------------------------
// SEELE-ENC/2: o registro assinado (ADR 0062)
// ---------------------------------------------------------------------------

/// O que todo datagrama da versão 2 começa dizendo.
///
/// É a magia que separa as duas versões no mesmo ponto (ADR 0062). Um ponto de
/// antes do ENC/2 cala para ela, e é isso que deixa quem liga distinguir «o
/// ponto não fala a versão 2» de «ninguém mora lá»: o `ONDE` da versão 1 corre
/// junto, e só ele responde.
pub const MAGIA2: &str = "SEELE-ENC/2";

/// O tamanho de todo datagrama do ENC/2, pedido ou resposta: quatro vezes o
/// da versão 1.
///
/// Fixo, pelo motivo de [`TAMANHO`]: toda resposta tem o tamanho do pedido que
/// a causou, e o ponto nunca amplifica. O maior `MORO2` possível tem 322 bytes
/// com a quebra de linha, o cookie incluído (o teste
/// `o_maior_moro2_cabe_no_datagrama`).
///
/// **A folga não compra campo novo sem versão nova.** Cada linha tem um número
/// exato de campos, e cada motivo da recusa é um de quatro: um campo ou um
/// motivo a mais faz todo leitor de hoje ver lixo, e calar. Isso é um ENC/3.
pub const TAMANHO2: usize = 4 * TAMANHO;

/// Quantas dicas de rede local um registro leva, no máximo.
pub const LIMITE_DE_DICAS: usize = 2;

/// Quanto o carimbo de um `MORO2` pode se afastar do relógio do ponto, para
/// mais ou para menos, em segundos.
///
/// Cinco minutos, da spec de 23/09/2026. É também por quanto tempo um `MORO2`
/// copiado no caminho poderia ser repetido, e é por isso que o ponto lembra o
/// carimbo de cada marca durante esse tempo (ADR 0062).
pub const TOLERANCIA_DO_CARIMBO: u64 = 300;

/// Quantos bytes tem o cookie do `MORO2` (D-enc-4, ADR 0062).
///
/// É a prova de ida e volta: o ponto o dá num [`Resposta2::Desafio`] ou num
/// [`Resposta2::Morou`], preso ao IP de quem pediu, e só um `MORO2` que o traz
/// de volta gasta o limite e chega ao quarto. Quem forja a origem não recebe o
/// cookie do IP que forja.
pub const BYTES_DO_COOKIE: usize = 16;

/// O motor de base64 do fio: base64url, sem `=`, e sem bits sobrando no fim.
/// Uma chave tem uma escrita só.
const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Um registro assinado: «esta marca mora de onde este pacote veio, e quem diz
/// é a chave dela».
///
/// A assinatura cobre a marca, a chave, o carimbo e as dicas
/// ([`crate::assinatura::moro2`]). O endereço público não vai assinado: quem o
/// diz é o ponto, que o vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moro2 {
    /// A marca da escuta ou do servidor da sala desta chave ([`Marcas::da_sala`]).
    pub marca: Marca,
    /// A chave pública Ed25519 da pessoa.
    pub chave: [u8; 32],
    /// Segundos Unix no relógio de quem assinou.
    pub carimbo: u64,
    /// Até [`LIMITE_DE_DICAS`] endereços de rede local ([`serve_de_dica`]).
    pub dicas: Vec<SocketAddr>,
    /// A assinatura Ed25519 de [`crate::assinatura::moro2`].
    pub assinatura: [u8; 64],
    /// O cookie que o ponto deu a este IP, ou `None` na primeira vez.
    ///
    /// **Não vai assinado**: ele prova o caminho de volta, e não a chave. A
    /// sala o junta ao `MORO2` sem assinar de novo.
    pub cookie: Option<[u8; BYTES_DO_COOKIE]>,
}

/// O que se pede a um ponto de encontro na versão 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pedido2 {
    /// O registro assinado.
    Moro2(Moro2),
    /// «Onde mora esta marca, e que dicas ela deixou?» Responde sempre: com
    /// [`Resposta2::Aqui2`] ou com [`Resposta2::Ninguem`].
    Quem2 {
        /// De quem se quer o endereço.
        marca: Marca,
    },
}

/// Por que o ponto recusou um pedido da versão 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motivo {
    /// O carimbo se afasta do relógio do ponto mais que
    /// [`TOLERANCIA_DO_CARIMBO`].
    Relogio,
    /// O ponto já guarda um carimbo igual ou mais novo desta chave, vindo de
    /// outro endereço.
    Velho,
    /// O quarto assinado chegou ao teto.
    Cheio,
    /// Este prefixo passou do limite de pedidos.
    Taxa,
}

impl Motivo {
    /// Todos, na ordem em que o fio os conhece.
    const TODOS: [Self; 4] = [Self::Relogio, Self::Velho, Self::Cheio, Self::Taxa];

    /// Como o motivo vai escrito no fio.
    #[must_use]
    pub fn texto(self) -> &'static str {
        match self {
            Self::Relogio => "relogio",
            Self::Velho => "velho",
            Self::Cheio => "cheio",
            Self::Taxa => "taxa",
        }
    }

    fn ler(texto: &str) -> Option<Self> {
        Self::TODOS
            .into_iter()
            .find(|motivo| motivo.texto() == texto)
    }
}

/// O que o ponto responde na versão 2. Sempre para quem pediu, e sempre com
/// [`TAMANHO2`] bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resposta2 {
    /// O `MORO2` foi guardado. Leva o endereço que o ponto viu, como o `AQUI`
    /// que responde a um `MORO`, e o cookie da janela corrente, para a sala
    /// renová-lo antes de ele vencer.
    Morou {
        /// A marca registrada.
        marca: Marca,
        /// O endereço público de quem registrou, visto pelo ponto.
        endereco: SocketAddr,
        /// O cookie deste IP, na janela corrente.
        cookie: [u8; BYTES_DO_COOKIE],
    },
    /// O `MORO2` veio sem cookie, ou com um que não confere para o IP de onde
    /// ele veio. O ponto não guardou nada, e diz o cookie: o mesmo `MORO2`,
    /// com ele, mora.
    Desafio {
        /// A marca do pedido.
        marca: Marca,
        /// O cookie deste IP, na janela corrente.
        cookie: [u8; BYTES_DO_COOKIE],
    },
    /// Onde a marca mora, e as dicas que ela deixou.
    Aqui2 {
        /// A marca perguntada.
        marca: Marca,
        /// O endereço público que o `MORO2` dela registrou.
        endereco: SocketAddr,
        /// As dicas de rede local do registro.
        dicas: Vec<SocketAddr>,
    },
    /// Ninguém mora nesta marca agora. Dito, e não calado: ver
    /// [`Pedido2::Quem2`].
    Ninguem {
        /// A marca perguntada.
        marca: Marca,
    },
    /// O ponto recusou o pedido, e diz por quê e que horas ele acha que são.
    Recusa {
        /// A marca do pedido.
        marca: Marca,
        /// Por quê.
        motivo: Motivo,
        /// Segundos Unix no relógio do ponto.
        agora: u64,
    },
}

impl Resposta2 {
    /// A marca de que esta resposta fala.
    #[must_use]
    pub fn marca(&self) -> &Marca {
        match self {
            Self::Morou { marca, .. }
            | Self::Desafio { marca, .. }
            | Self::Aqui2 { marca, .. }
            | Self::Ninguem { marca }
            | Self::Recusa { marca, .. } => marca,
        }
    }
}

/// Se este endereço pode ir como dica de rede local num `MORO2`.
///
/// Só a rede de casa: IPv4 privado (RFC 1918) ou IPv6 ULA (`fc00::/7`), sem
/// escopo, e porta de 1024 para cima. O ponto recusa um registro com dica fora
/// disso, e quem liga recusa uma resposta com dica fora disso: uma dica
/// inventada por um ponto hostil faria quem liga bater dentro da própria rede,
/// ou num serviço da internet.
#[must_use]
pub fn serve_de_dica(endereco: SocketAddr) -> bool {
    if endereco.port() < 1024 {
        return false;
    }
    match endereco {
        SocketAddr::V4(quatro) => quatro.ip().is_private(),
        SocketAddr::V6(seis) => {
            let primeiro = seis.ip().segments().first().copied().unwrap_or_default();
            seis.scope_id() == 0 && primeiro & 0xfe00 == 0xfc00
        }
    }
}

/// Monta um `MORO2`.
///
/// Escreve o que recebe: quem monta já assinou estas dicas, e filtrá-las aqui
/// faria a assinatura deixar de conferir. As dicas saem de quem registra já
/// passadas por [`serve_de_dica`].
#[must_use]
pub fn moro2(pedido: &Moro2) -> Vec<u8> {
    use base64::Engine as _;
    encher2(format!(
        "{MAGIA2} MORO2 {} {} {} {} {} {}",
        pedido.marca,
        BASE64.encode(pedido.chave),
        pedido.carimbo,
        escrever_dicas(&pedido.dicas),
        BASE64.encode(pedido.assinatura),
        pedido
            .cookie
            .map_or_else(|| "-".to_owned(), |cookie| BASE64.encode(cookie)),
    ))
}

/// Monta um `QUEM2`.
#[must_use]
pub fn quem2(marca: &Marca) -> Vec<u8> {
    encher2(format!("{MAGIA2} QUEM2 {marca}"))
}

/// Monta uma resposta da versão 2.
#[must_use]
pub fn resposta2(resposta: &Resposta2) -> Vec<u8> {
    use base64::Engine as _;
    encher2(match resposta {
        Resposta2::Morou {
            marca,
            endereco,
            cookie,
        } => format!(
            "{MAGIA2} MOROU {marca} {endereco} {}",
            BASE64.encode(cookie)
        ),
        Resposta2::Desafio { marca, cookie } => {
            format!("{MAGIA2} DESAFIO {marca} {}", BASE64.encode(cookie))
        }
        Resposta2::Aqui2 {
            marca,
            endereco,
            dicas,
        } => format!(
            "{MAGIA2} AQUI2 {marca} {endereco} {}",
            escrever_dicas(dicas)
        ),
        Resposta2::Ninguem { marca } => format!("{MAGIA2} NINGUEM {marca}"),
        Resposta2::Recusa {
            marca,
            motivo,
            agora,
        } => format!("{MAGIA2} RECUSA {marca} {} {agora}", motivo.texto()),
    })
}

/// Lê um pedido da versão 2, sem conferir assinatura nem relógio.
///
/// Como [`analisar`], nada aqui tenta adivinhar: tamanho diferente de
/// [`TAMANHO2`], outra magia, verbo desconhecido, campo a mais ou a menos, ou
/// um campo fora da forma canônica (um número com sinal ou zero à esquerda,
/// base64 com `=` ou com bits sobrando, um endereço que não volta igual depois
/// de lido e escrito, uma dica que não é de rede local) dão `None`.
#[must_use]
pub fn analisar2(datagrama: &[u8]) -> Option<Pedido2> {
    let (verbo, resto) = verbo2(datagrama)?;
    match verbo {
        "MORO2" => {
            let [marca, chave, carimbo, dicas, assinatura, cookie] = campos(resto)?;
            Some(Pedido2::Moro2(Moro2 {
                marca: Marca::nova(marca)?,
                chave: ler_base64(chave)?,
                carimbo: ler_numero(carimbo)?,
                dicas: ler_dicas(dicas)?,
                assinatura: ler_base64(assinatura)?,
                cookie: match cookie {
                    "-" => None,
                    cookie => Some(ler_base64(cookie)?),
                },
            }))
        }
        "QUEM2" => {
            let [marca] = campos(resto)?;
            Some(Pedido2::Quem2 {
                marca: Marca::nova(marca)?,
            })
        }
        _ => None,
    }
}

/// Lê uma resposta da versão 2, com as regras de [`analisar2`].
///
/// Devolve `None` para os pedidos: quem espera resposta não trata pedido.
#[must_use]
pub fn ler_resposta2(datagrama: &[u8]) -> Option<Resposta2> {
    let (verbo, resto) = verbo2(datagrama)?;
    match verbo {
        "MOROU" => {
            let [marca, endereco, cookie] = campos(resto)?;
            Some(Resposta2::Morou {
                marca: Marca::nova(marca)?,
                endereco: ler_endereco(endereco)?,
                cookie: ler_base64(cookie)?,
            })
        }
        "DESAFIO" => {
            let [marca, cookie] = campos(resto)?;
            Some(Resposta2::Desafio {
                marca: Marca::nova(marca)?,
                cookie: ler_base64(cookie)?,
            })
        }
        "AQUI2" => {
            let [marca, endereco, dicas] = campos(resto)?;
            Some(Resposta2::Aqui2 {
                marca: Marca::nova(marca)?,
                endereco: ler_endereco(endereco)?,
                dicas: ler_dicas(dicas)?,
            })
        }
        "NINGUEM" => {
            let [marca] = campos(resto)?;
            Some(Resposta2::Ninguem {
                marca: Marca::nova(marca)?,
            })
        }
        "RECUSA" => {
            let [marca, motivo, agora] = campos(resto)?;
            Some(Resposta2::Recusa {
                marca: Marca::nova(marca)?,
                motivo: Motivo::ler(motivo)?,
                agora: ler_numero(agora)?,
            })
        }
        _ => None,
    }
}

/// Completa a linha até [`TAMANHO2`]. Ver [`encher`].
fn encher2(mut linha: String) -> Vec<u8> {
    linha.push('\n');
    let mut bytes = linha.into_bytes();
    bytes.resize(TAMANHO2, ENCHIMENTO);
    bytes
}

/// O verbo e o resto da linha de um datagrama da versão 2 do tamanho certo.
fn verbo2(datagrama: &[u8]) -> Option<(&str, &str)> {
    if datagrama.len() != TAMANHO2 {
        return None;
    }
    let linha = std::str::from_utf8(datagrama).ok()?.split('\n').next()?;
    linha
        .strip_prefix(MAGIA2)?
        .strip_prefix(' ')?
        .split_once(' ')
}

/// Exatamente `N` campos, separados por um espaço cada.
fn campos<const N: usize>(resto: &str) -> Option<[&str; N]> {
    let mut partes = resto.split(' ');
    let mut saida = [""; N];
    for campo in &mut saida {
        *campo = partes.next()?;
    }
    partes.next().is_none().then_some(saida)
}

/// `N` bytes em base64url, só na escrita canônica: o comprimento exato, sem
/// `=` e sem bits sobrando no fim.
fn ler_base64<const N: usize>(texto: &str) -> Option<[u8; N]> {
    use base64::Engine as _;
    if texto.len() != (N * 4).div_ceil(3) {
        return None;
    }
    BASE64.decode(texto).ok()?.try_into().ok()
}

/// Um número decimal sem sinal e sem zero à esquerda.
///
/// Escrito à mão porque `u64::from_str` aceita `+1` e `007`, e cada número
/// deste fio tem uma escrita só.
fn ler_numero(texto: &str) -> Option<u64> {
    let canonico = !texto.is_empty()
        && texto.bytes().all(|byte| byte.is_ascii_digit())
        && (texto == "0" || !texto.starts_with('0'));
    if !canonico {
        return None;
    }
    texto.parse().ok()
}

/// Um endereço, só se o texto for exatamente como o `Display` o escreve.
///
/// `SocketAddr::from_str` aceita porta com zero à esquerda (`:08385`), que o
/// `Display` não escreve.
fn ler_endereco(texto: &str) -> Option<SocketAddr> {
    let endereco: SocketAddr = texto.parse().ok()?;
    (endereco.to_string() == texto).then_some(endereco)
}

/// As dicas no fio: `-` para nenhuma, ou separadas por vírgula.
fn escrever_dicas(dicas: &[SocketAddr]) -> String {
    if dicas.is_empty() {
        return "-".to_owned();
    }
    dicas
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Lê as dicas: `-`, ou até [`LIMITE_DE_DICAS`] endereços canônicos que
/// [`serve_de_dica`] aceita. Uma só que não sirva recusa todas.
fn ler_dicas(texto: &str) -> Option<Vec<SocketAddr>> {
    if texto == "-" {
        return Some(Vec::new());
    }
    let dicas = texto
        .split(',')
        .map(|uma| ler_endereco(uma).filter(|dica| serve_de_dica(*dica)))
        .collect::<Option<Vec<_>>>()?;
    (dicas.len() <= LIMITE_DE_DICAS).then_some(dicas)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn marca(texto: &str) -> Marca {
        Marca::nova(texto).expect("marca de teste inválida")
    }

    fn de(texto: &str) -> SocketAddr {
        texto.parse().expect("endereço de teste inválido")
    }

    #[test]
    fn um_onde_volta_para_quem_perguntou_com_o_endereco_que_o_ponto_viu() {
        // O degrau 4 inteiro depende disto: ninguém atrás de NAT sabe o próprio
        // endereço público, e esta é a única forma de descobrir.
        let origem = de("45.33.32.156:41234");
        let resposta = responder(&onde(&marca("abc123")), origem).expect("calou num ONDE");
        assert_eq!(resposta.destino, origem, "respondeu para outro endereço");
        assert_eq!(
            ler_aqui(&resposta.datagrama),
            Some((marca("abc123"), origem))
        );
    }

    #[test]
    fn um_leve_apresenta_os_dois_sem_nunca_ter_ouvido_falar_de_nenhum() {
        // "Sem estado" não é uma promessa deste módulo: é a assinatura dele.
        // Este `LEVE` funciona sem nenhum `ONDE` antes, de nenhum dos dois
        // lados, porque não existe cadastro para consultar.
        let visitante = de("200.160.2.3:50000");
        let anfitriao = de("45.33.32.156:41234");
        let resposta =
            responder(&leve(anfitriao, &marca("3cbcfb0212da738f")), visitante).expect("calou");
        assert_eq!(resposta.destino, anfitriao);
        assert_eq!(
            ler_aqui(&resposta.datagrama),
            Some((marca("3cbcfb0212da738f"), visitante)),
            "o anfitrião não recebeu o endereço para onde furar"
        );
    }

    #[test]
    fn a_mesma_pergunta_tem_a_mesma_resposta_sempre() {
        // Um ponto de encontro que aprendesse alguma coisa teria uma segunda
        // resposta diferente da primeira. Este não tem como.
        let origem = de("45.33.32.156:41234");
        let pedido = leve(de("200.160.2.3:50000"), &marca("umamarca"));
        let primeira = responder(&pedido, origem);
        let segunda = responder(&pedido, origem);
        assert_eq!(primeira, segunda);
        assert!(primeira.is_some());
    }

    #[test]
    fn nenhuma_resposta_e_maior_que_o_pedido_que_a_causou() {
        // A defesa contra amplificação, e ela vale para todo pedido aceito: um
        // refletor que devolve mais do que recebe é uma arma apontada para
        // alguém que nunca ouviu falar deste projeto.
        let origens = [de("45.33.32.156:41234"), de("[2001:4860:4860::8888]:65535")];
        let pedidos = [
            onde(&marca("a")),
            onde(&marca(&"z".repeat(LIMITE_DA_MARCA))),
            leve(de("200.160.2.3:50000"), &marca("a")),
            leve(
                de("[2606:4700:4700::1111]:65535"),
                &marca(&"z".repeat(LIMITE_DA_MARCA)),
            ),
        ];
        for origem in origens {
            for pedido in &pedidos {
                let Some(resposta) = responder(pedido, origem) else {
                    panic!("recusou um pedido legítimo");
                };
                assert!(
                    resposta.datagrama.len() <= pedido.len(),
                    "amplificou: {} bytes de resposta para {} de pedido",
                    resposta.datagrama.len(),
                    pedido.len()
                );
            }
        }
    }

    #[test]
    fn um_pedido_curto_e_recusado_mesmo_estando_certo() {
        // É o mesmo `ONDE` do primeiro teste, sem o enchimento. Aceitá-lo seria
        // aceitar amplificação de graça, e o formato existe para isso.
        let curto = b"SEELE-ENC/1 ONDE abc123\n";
        assert_eq!(responder(curto, de("45.33.32.156:41234")), None);
    }

    #[test]
    fn um_leve_nao_alcanca_rede_que_nao_e_da_internet() {
        // Um refletor que entra em rede privada é uma forma de bater em
        // máquinas que não estão na internet a partir de uma que está — a
        // mesma família de estrago que um SSRF.
        for destino in [
            "192.168.0.1:8383",
            "10.0.0.1:8383",
            "172.16.0.1:8383",
            "127.0.0.1:8383",
            "169.254.1.1:8383",
            "100.64.0.1:8383",
            "[::1]:8383",
            "[fe80::1]:8383",
            "[fd12:3456::1]:8383",
            "[2001:db8::1]:8383",
            "[ff02::1]:8383",
            "0.0.0.0:8383",
            "224.0.0.1:8383",
        ] {
            let pedido = leve(de(destino), &marca("abc"));
            assert_eq!(
                responder(&pedido, de("45.33.32.156:41234")),
                None,
                "aceitou refletir para {destino}"
            );
        }
    }

    #[test]
    fn um_leve_nao_alcanca_porta_de_servico() {
        // DNS, NTP e companhia respondem a qualquer coisa. Um pacote nosso
        // batendo lá em nome de outra pessoa começa uma reflexão em cadeia.
        for porta in [53_u16, 123, 161, 1023] {
            let destino = SocketAddr::from(([200, 160, 2, 3], porta));
            assert_eq!(
                responder(&leve(destino, &marca("abc")), de("45.33.32.156:41234")),
                None,
                "aceitou refletir para a porta {porta}"
            );
        }
        // E a porta logo acima continua valendo: a regra é /1024, não "portas
        // que eu conheço".
        let alta = SocketAddr::from(([200, 160, 2, 3], 1024));
        assert!(responder(&leve(alta, &marca("abc")), de("45.33.32.156:41234")).is_some());
    }

    #[test]
    fn nada_do_pedido_atravessa_para_a_resposta_alem_da_marca() {
        // "Não consegue ler nem repassar" — a resposta é montada campo a campo
        // e nunca copiada. O único pedaço que reaparece é a marca, e ela é
        // alfanumérica e curta justamente por isso.
        let mut pedido = leve(de("200.160.2.3:50000"), &marca("abc"));
        // Bytes escolhidos por quem manda, no enchimento, onde caberia um
        // repasse desatento.
        for (indice, byte) in pedido.iter_mut().enumerate().skip(40) {
            *byte = if indice % 2 == 0 { b'X' } else { b'Y' };
        }
        let Some(resposta) = responder(&pedido, de("45.33.32.156:41234")) else {
            panic!("o enchimento sujo derrubou um pedido válido");
        };
        assert!(
            !resposta.datagrama.windows(2).any(|par| par == b"XY"),
            "bytes escolhidos por quem pediu chegaram a quem recebe"
        );
    }

    #[test]
    fn o_pacote_do_furo_nao_e_coisa_do_ponto_de_encontro() {
        // O furo vai de uma ponta à outra e nunca ao meio. Um ponto de encontro
        // que respondesse a ele estaria a um passo de repassar pacote entre as
        // duas pontas — que é retransmissão, e o ADR 0022 a deixou fora de
        // escopo por decisão.
        let pacote = furo(&marca("abc"));
        assert_eq!(responder(&pacote, de("45.33.32.156:41234")), None);
        assert_eq!(analisar(&pacote), None);
        assert_eq!(ler_aqui(&pacote), None);
    }

    #[test]
    fn uma_resposta_nao_e_um_pedido() {
        // Dois pontos de encontro apontados um para o outro não podem virar um
        // laço, e um `AQUI` que entra é ou ruído ou exatamente essa tentativa.
        let aqui = aqui(&marca("abc"), de("45.33.32.156:41234"));
        assert_eq!(responder(&aqui, de("200.160.2.3:50000")), None);
        assert_eq!(analisar(&aqui), None);
    }

    #[test]
    fn lixo_nao_vira_resposta() {
        let origem = de("45.33.32.156:41234");
        let mut casos: Vec<Vec<u8>> = vec![
            vec![0_u8; TAMANHO],
            vec![b'A'; TAMANHO],
            encher("SEELE-ENC/2 ONDE abc".to_owned()),
            encher("SEELE-ENC/1 ONDE".to_owned()),
            encher("SEELE-ENC/1 ONDE tem espaço".to_owned()),
            encher("SEELE-ENC/1 VAI 45.33.32.156:1 abc".to_owned()),
            encher(format!(
                "SEELE-ENC/1 ONDE {}",
                "z".repeat(LIMITE_DA_MARCA + 1)
            )),
            encher("SEELE-ENC/1 LEVE naoehendereco abc".to_owned()),
            encher("SEELE-ENC/1 LEVE 45.33.32.156 abc".to_owned()),
            Vec::new(),
        ];
        // Um datagrama com um byte que não é UTF-8 no meio da linha útil.
        let mut invalido = onde(&marca("abc"));
        if let Some(byte) = invalido.get_mut(13) {
            *byte = 0xff;
        }
        casos.push(invalido);

        for caso in casos {
            assert_eq!(
                responder(&caso, origem),
                None,
                "respondeu a lixo: {:?}",
                String::from_utf8_lossy(&caso)
            );
        }
    }

    #[test]
    fn uma_marca_e_curta_e_alfanumerica() {
        assert!(Marca::nova("").is_none());
        assert!(Marca::nova(&"z".repeat(LIMITE_DA_MARCA + 1)).is_none());
        assert!(Marca::nova("tem espaço").is_none());
        assert!(Marca::nova("tem\nlinha").is_none());
        assert!(Marca::nova("3cbcfb0212da738f").is_some());
        assert!(Marca::nova(&"z".repeat(LIMITE_DA_MARCA)).is_some());
    }

    #[test]
    fn a_vizinhanca_maior_abre_a_rede_local_e_nao_a_porta_de_servico() {
        // O que o teste automatizado usa, e é a única frouxidão que existe:
        // continua sendo um datagrama do tamanho certo, com verbo conhecido, e
        // continua sem alcançar porta de serviço.
        let local = de("127.0.0.1:40000");
        let pedido = leve(local, &marca("abc"));
        assert_eq!(responder(&pedido, de("45.33.32.156:41234")), None);
        assert!(responder_em(&pedido, de("45.33.32.156:41234"), Vizinhanca::TambemAqui).is_some());

        let servico = leve(de("127.0.0.1:53"), &marca("abc"));
        assert_eq!(
            responder_em(&servico, de("45.33.32.156:41234"), Vizinhanca::TambemAqui),
            None,
            "a vizinhança maior virou vale-tudo"
        );
    }

    #[test]
    fn a_porta_padrao_fica_ao_lado_da_do_server() {
        assert_eq!(PORTA_PADRAO, 8384);
    }

    const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

    #[test]
    fn as_marcas_de_um_servidor_saem_da_impressao_digital() {
        // O aviso e o servidor são os que a v0.15.0 já usa: `LEVE <fp16>` de
        // quem chega e `MORO`/`QUEM <fp16>s` do socket do servidor. Mudar um
        // dos dois deixa falando sozinho todo cliente que não atualizou. A
        // escuta é a única nova.
        let marcas =
            Marcas::do_servidor(IMPRESSAO).expect("uma impressão digital sempre dá marcas");
        assert_eq!(
            marcas.aviso().texto(),
            "3cbcfb0212da738f",
            "a marca do aviso mudou: todo cliente 0.15.0 passa a bater com uma marca que o anfitrião ignora"
        );
        assert_eq!(
            marcas.servidor().texto(),
            "3cbcfb0212da738fs",
            "a marca do servidor mudou: todo cliente 0.15.0 pergunta por uma marca que ninguém registra"
        );
        assert_eq!(
            marcas.escuta().texto(),
            "3cbcfb0212da738fe",
            "a marca da escuta mudou: quem procura o anfitrião pergunta por uma marca que a escuta não registra"
        );
    }

    #[test]
    fn a_escuta_nunca_tem_a_marca_do_aviso() {
        // É o que filtra o eco. O `MORO` da escuta é respondido para a própria
        // escuta, com a marca do `MORO`. Com a marca do aviso, a resposta passa
        // no filtro de `atender` e vira um furo do anfitrião contra si mesmo.
        for impressao in [
            IMPRESSAO,
            "0000000000000000000000000000000000000000000000000000000000000000",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ] {
            let marcas =
                Marcas::do_servidor(impressao).expect("uma impressão digital sempre dá marcas");
            assert_ne!(
                marcas.escuta(),
                marcas.aviso(),
                "com a impressão {impressao}, a escuta se registra com a marca do aviso: o AQUI \
                 que responde ao próprio MORO passa no filtro e vira um furo contra si mesmo"
            );
            assert_ne!(
                marcas.servidor(),
                marcas.aviso(),
                "com a impressão {impressao}, o servidor se registra com a marca do aviso"
            );
            assert_ne!(
                marcas.servidor(),
                marcas.escuta(),
                "com a impressão {impressao}, servidor e escuta têm a mesma marca: o quarto guarda \
                 um endereço por marca, e um apaga o outro"
            );
        }
    }

    #[test]
    fn um_texto_que_nao_e_impressao_digital_nao_da_marca_nenhuma() {
        assert_eq!(
            Marcas::do_servidor("curto"),
            None,
            "uma impressão digital curta demais deu marcas: um fp truncado bateria com uma marca que nenhum servidor tem"
        );
        assert_eq!(
            Marcas::do_servidor(""),
            None,
            "um texto vazio deu marcas: sem impressão digital não há marca que quem procura saiba perguntar"
        );
        assert_eq!(
            Marcas::do_servidor("3cbcfb02 2da738f89c156de86eb280a"),
            None,
            "um espaço virou marca: a marca é o único campo que volta na resposta do ponto"
        );
        // O T1a da pendência 51, item 17: o texto inteiro, e não só os 16
        // primeiros. O doc promete `None` para «algo que não é letra nem
        // número», e o que vinha depois do 16º caractere não era olhado.
        assert_eq!(
            Marcas::do_servidor("0123456789abcdef!!!"),
            None,
            "um texto com algo que não é letra nem número depois do 16º caractere deu marcas: \
             o doc de `do_servidor` promete recusá-lo, e uma impressão torta passa por boa"
        );
    }
}

#[cfg(test)]
mod as_marcas_da_sala {
    //! As marcas da sala pessoal no quarto (spec de 23/09/2026, «O código»):
    //! `p` e os 15 símbolos de dado do código; a escuta com `e` no fim, o
    //! servidor com `s`. Formato congelado: um código já distribuído responde
    //! nestas marcas para sempre.
    use super::*;
    use crate::codigo::Codigo;

    /// Calculadas pela implementação de referência independente, em Python,
    /// que também confere os códigos (`codigo.rs`).
    #[test]
    fn as_marcas_de_cada_codigo_sao_as_da_referencia() {
        for (chave, aviso) in [
            ([0_u8; 32], "pCSM7NBFRCAYQEV4"),
            ([0xFF; 32], "pNYB16XGFE9HNZFD"),
        ] {
            let marcas = Marcas::da_sala(&Codigo::da_chave(&chave));
            assert_eq!(marcas.aviso().texto(), aviso);
            assert_eq!(marcas.escuta().texto(), format!("{aviso}e"));
            assert_eq!(marcas.servidor().texto(), format!("{aviso}s"));
        }
    }

    /// **A caixa das letras é formato.** `Marca::nova` não normaliza, e o
    /// ponto compara byte a byte: o `p`, o `e` e o `s` minúsculos, e os
    /// símbolos do código maiúsculos, como `Codigo::dado` os dá.
    #[test]
    fn a_caixa_das_letras_e_a_do_codigo_com_o_p_minusculo() {
        let marcas = Marcas::da_sala(&Codigo::da_chave(&[0; 32]));
        assert!(marcas.aviso().texto().starts_with('p'));
        assert_eq!(
            &marcas.aviso().texto()[1..],
            Codigo::da_chave(&[0; 32]).dado()
        );
    }

    /// Toda marca da sala é uma marca que o quarto aceita.
    #[test]
    fn toda_marca_da_sala_e_uma_marca_valida() {
        let marcas = Marcas::da_sala(&Codigo::da_chave(&[7; 32]));
        for marca in [marcas.aviso(), marcas.escuta(), marcas.servidor()] {
            assert_eq!(Marca::nova(marca.texto()).as_ref(), Some(marca));
        }
    }

    /// **Uma marca da sala nunca colide com a de um servidor guardado.**
    ///
    /// As de servidor saem dos 16 primeiros caracteres de uma impressão, que o
    /// link só aceita em hexadecimal (`uri.rs`); as da sala começam todas por
    /// `p`, que não é hexadecimal. É a primeira letra que separa os dois
    /// espaços, e é ela que este teste prende.
    ///
    /// Escrito assim, e não como uma propriedade sobre chaves e impressões
    /// sorteadas: sortear não acha a colisão nem quando o `p` some, porque a
    /// chance de um código cair inteiro em hexadecimal é ínfima. Um guarda que
    /// não reprova na reversão não guarda nada.
    #[test]
    fn o_p_separa_a_sala_dos_servidores() {
        assert!(!'p'.is_ascii_hexdigit());
        for chave in [[0_u8; 32], [0xFF; 32], [7; 32]] {
            let sala = Marcas::da_sala(&Codigo::da_chave(&chave));
            for marca in [sala.aviso(), sala.escuta(), sala.servidor()] {
                assert!(marca.texto().starts_with('p'), "{}", marca.texto());
            }
        }
        for impressao in ["0".repeat(64), "f".repeat(64), "3c".repeat(32)] {
            let servidor = Marcas::do_servidor(&impressao).unwrap();
            for marca in [servidor.aviso(), servidor.escuta(), servidor.servidor()] {
                assert!(!marca.texto().starts_with('p'), "{}", marca.texto());
            }
        }
    }
}

#[cfg(test)]
mod enc2 {
    //! O fio do SEELE-ENC/2 (ADR 0062): tamanho fixo, forma canônica, e as
    //! duas versões que não se leem uma à outra.
    use super::*;

    fn marca(texto: &str) -> Marca {
        Marca::nova(texto).expect("marca de teste inválida")
    }

    fn de(texto: &str) -> SocketAddr {
        texto.parse().expect("endereço de teste inválido")
    }

    fn um_moro2() -> Moro2 {
        Moro2 {
            marca: marca("pCSM7NBFRCAYQEV4s"),
            chave: [7; 32],
            carimbo: 1_791_000_000,
            dicas: vec![de("192.168.0.10:8385"), de("[fd12:3456::1]:8385")],
            assinatura: [9; 64],
            cookie: Some([5; BYTES_DO_COOKIE]),
        }
    }

    /// A linha útil de um datagrama, sem a quebra e sem o enchimento.
    fn linha(datagrama: &[u8]) -> String {
        let texto = String::from_utf8(datagrama.to_vec()).expect("é texto");
        texto.split('\n').next().expect("tem linha").to_owned()
    }

    #[test]
    fn o_maior_moro2_cabe_no_datagrama() {
        // A conta de `TAMANHO2`: marca de 32, carimbo `u64::MAX`, duas dicas
        // IPv6 do maior tamanho que uma dica pode ter. Se ela deixar de caber,
        // `encher2` corta a linha, e o pedido deixa de voltar igual.
        let pedido = Moro2 {
            marca: marca(&"z".repeat(LIMITE_DA_MARCA)),
            chave: [0xFF; 32],
            carimbo: u64::MAX,
            dicas: vec![de("[fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535"); 2],
            assinatura: [0xFF; 64],
            cookie: Some([0xFF; BYTES_DO_COOKIE]),
        };
        let datagrama = moro2(&pedido);
        assert_eq!(datagrama.len(), TAMANHO2);
        assert_eq!(analisar2(&datagrama), Some(Pedido2::Moro2(pedido)));
    }

    #[test]
    fn todo_datagrama_do_enc2_volta_igual_e_tem_o_tamanho_fixo() {
        let pedidos = [
            Pedido2::Moro2(um_moro2()),
            Pedido2::Moro2(Moro2 {
                dicas: Vec::new(),
                ..um_moro2()
            }),
            Pedido2::Moro2(Moro2 {
                cookie: None,
                ..um_moro2()
            }),
            Pedido2::Quem2 {
                marca: marca("pCSM7NBFRCAYQEV4s"),
            },
        ];
        for pedido in pedidos {
            let datagrama = match &pedido {
                Pedido2::Moro2(moro) => moro2(moro),
                Pedido2::Quem2 { marca } => quem2(marca),
            };
            assert_eq!(datagrama.len(), TAMANHO2, "{pedido:?}");
            assert_eq!(analisar2(&datagrama), Some(pedido));
        }
        let respostas = [
            Resposta2::Morou {
                marca: marca("pAs"),
                endereco: de("203.0.113.7:41234"),
                cookie: [3; BYTES_DO_COOKIE],
            },
            Resposta2::Desafio {
                marca: marca("pAs"),
                cookie: [0xFE; BYTES_DO_COOKIE],
            },
            Resposta2::Aqui2 {
                marca: marca("pAs"),
                endereco: de("[2001:4860::1]:41234"),
                dicas: vec![de("10.0.0.2:8385")],
            },
            Resposta2::Aqui2 {
                marca: marca("pAe"),
                endereco: de("203.0.113.7:1"),
                dicas: Vec::new(),
            },
            Resposta2::Ninguem {
                marca: marca("pAs"),
            },
            Resposta2::Recusa {
                marca: marca("pAs"),
                motivo: Motivo::Relogio,
                agora: 1_791_000_000,
            },
            Resposta2::Recusa {
                marca: marca("pAs"),
                motivo: Motivo::Velho,
                agora: 0,
            },
            Resposta2::Recusa {
                marca: marca("pAs"),
                motivo: Motivo::Cheio,
                agora: 1,
            },
            Resposta2::Recusa {
                marca: marca("pAs"),
                motivo: Motivo::Taxa,
                agora: u64::MAX,
            },
        ];
        for resposta in respostas {
            let datagrama = resposta2(&resposta);
            assert_eq!(datagrama.len(), TAMANHO2, "{resposta:?}");
            assert_eq!(ler_resposta2(&datagrama), Some(resposta));
        }
    }

    #[test]
    fn nenhuma_resposta_do_enc2_e_maior_que_o_pedido_que_a_causou() {
        // A maior resposta possível ao menor pedido, um `QUEM2`: uma marca de
        // 32, um endereço IPv6 do maior tamanho, e duas dicas IPv6.
        let pedido = quem2(&marca(&"z".repeat(LIMITE_DA_MARCA)));
        let maior = resposta2(&Resposta2::Aqui2 {
            marca: marca(&"z".repeat(LIMITE_DA_MARCA)),
            endereco: de("[2001:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535"),
            dicas: vec![de("[fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535"); 2],
        });
        assert!(
            maior.len() <= pedido.len(),
            "amplificou: {} bytes de resposta para {} de pedido",
            maior.len(),
            pedido.len()
        );
        assert!(
            ler_resposta2(&maior).is_some(),
            "a maior resposta não coube no datagrama e foi cortada"
        );
    }

    #[test]
    fn as_duas_versoes_nao_se_leem() {
        let v1 = [
            onde(&marca("abc")),
            moro(&marca("abc")),
            quem(&marca("abc")),
            aqui(&marca("abc"), de("203.0.113.7:4000")),
        ];
        for datagrama in &v1 {
            assert_eq!(analisar2(datagrama), None);
            assert_eq!(ler_resposta2(datagrama), None);
        }
        let v2 = [
            moro2(&um_moro2()),
            quem2(&marca("abc")),
            resposta2(&Resposta2::Ninguem {
                marca: marca("abc"),
            }),
        ];
        for datagrama in &v2 {
            assert_eq!(analisar(datagrama), None);
            assert_eq!(ler_aqui(datagrama), None);
            assert_eq!(responder(datagrama, de("45.33.32.156:41234")), None);
        }
    }

    #[test]
    fn uma_resposta_do_enc2_nao_e_um_pedido() {
        // Dois pontos apontados um para o outro não viram um laço.
        let resposta = resposta2(&Resposta2::Morou {
            marca: marca("abc"),
            endereco: de("203.0.113.7:4000"),
            cookie: [1; BYTES_DO_COOKIE],
        });
        assert_eq!(analisar2(&resposta), None);
        assert_eq!(ler_resposta2(&moro2(&um_moro2())), None);
        assert_eq!(ler_resposta2(&quem2(&marca("abc"))), None);
    }

    /// O `MORO2` de `um_moro2`, com o campo `indice` trocado por `texto`:
    /// 0 a magia, 1 o verbo, 2 a marca, 3 a chave, 4 o carimbo, 5 as dicas,
    /// 6 a assinatura e 7 o cookie.
    fn com_campo(indice: usize, texto: &str) -> Vec<u8> {
        let linha = linha(&moro2(&um_moro2()));
        let mut campos: Vec<&str> = linha.split(' ').collect();
        campos[indice] = texto;
        encher2(campos.join(" "))
    }

    #[test]
    fn um_campo_fora_da_forma_canonica_e_lixo() {
        let chave = "A".repeat(43);
        assert!(
            analisar2(&com_campo(3, &chave)).is_some(),
            "o caso de controle, com a chave canônica, tem de ser aceito"
        );
        assert!(
            analisar2(&com_campo(7, "-")).is_some(),
            "o caso de controle, sem cookie, tem de ser aceito"
        );
        let inteira = linha(&moro2(&um_moro2()));
        let faltando: Vec<&str> = inteira.split(' ').take(6).collect();
        let casos = [
            com_campo(4, "+1791000000"),
            com_campo(4, "01791000000"),
            com_campo(4, "18446744073709551616"),
            com_campo(4, ""),
            com_campo(3, &format!("{chave}=")),
            com_campo(3, &"A".repeat(42)),
            com_campo(3, &format!("{}B", "A".repeat(42))),
            com_campo(3, &format!("{}+", "A".repeat(42))),
            com_campo(5, "192.168.0.10:08385"),
            com_campo(5, ""),
            com_campo(5, "192.168.0.10:8385,192.168.0.11:8385,192.168.0.12:8385"),
            com_campo(6, &"A".repeat(85)),
            com_campo(7, &"A".repeat(21)),
            com_campo(7, &"A".repeat(23)),
            com_campo(7, &format!("{}=", "A".repeat(22))),
            com_campo(7, ""),
            com_campo(7, "--"),
            encher2(format!("{inteira} extra")),
            encher2(faltando.join(" ")),
        ];
        for caso in casos {
            assert_eq!(
                analisar2(&caso),
                None,
                "aceitou {:?}",
                String::from_utf8_lossy(&caso)
            );
        }
    }

    #[test]
    fn uma_dica_e_um_endereco_da_rede_de_casa() {
        for sim in [
            "192.168.0.10:8385",
            "10.0.0.2:8385",
            "172.16.0.1:8385",
            "[fd12::1]:8385",
            "192.168.0.1:1024",
        ] {
            assert!(serve_de_dica(de(sim)), "recusou {sim}");
        }
        for nao in [
            "8.8.8.8:8385",
            "100.64.0.1:8385",
            "127.0.0.1:8385",
            "169.254.0.1:8385",
            "[fe80::1]:8385",
            "[fd00::1%3]:8385",
            "[::ffff:192.168.0.1]:8385",
            "[2001:db8::1]:8385",
            "[2001:4860::1]:8385",
            "192.168.0.1:53",
        ] {
            assert!(!serve_de_dica(de(nao)), "aceitou {nao}");
        }
    }

    #[test]
    fn um_aqui2_com_dica_que_nao_e_da_rede_de_casa_e_lixo() {
        // Um ponto hostil não manda quem liga bater dentro da própria rede, nem
        // num serviço da internet, por uma dica inventada.
        let linha = format!("{MAGIA2} AQUI2 pAs 203.0.113.7:41234 8.8.8.8:8385");
        assert_eq!(ler_resposta2(&encher2(linha)), None);
    }

    #[test]
    fn lixo_do_enc2_nao_vira_pedido_nem_resposta() {
        let mut casos = vec![
            vec![0_u8; TAMANHO2],
            vec![b'A'; TAMANHO2],
            encher2(format!("{MAGIA2} VAI abc")),
            encher2(format!("{MAGIA2} QUEM2")),
            encher2(format!("{MAGIA2} QUEM2 tem espaço")),
            encher2(format!("{MAGIA2} RECUSA abc talvez 1")),
            encher2(format!("{MAGIA2} NINGUEM")),
            encher2(format!("{MAGIA2}  QUEM2 abc")),
        ];
        let mut curto = quem2(&marca("abc"));
        curto.pop();
        casos.push(curto);
        let mut longo = quem2(&marca("abc"));
        longo.push(b'.');
        casos.push(longo);
        for caso in casos {
            assert_eq!(
                analisar2(&caso),
                None,
                "{:?}",
                String::from_utf8_lossy(&caso)
            );
            assert_eq!(
                ler_resposta2(&caso),
                None,
                "{:?}",
                String::from_utf8_lossy(&caso)
            );
        }
    }

    #[test]
    fn os_numeros_do_enc2_sao_os_do_adr() {
        assert_eq!(TOLERANCIA_DO_CARIMBO, 300);
        assert_eq!(LIMITE_DE_DICAS, 2);
        assert_eq!(TAMANHO2, 384);
        assert_eq!(TAMANHO2, 4 * TAMANHO);
        assert_eq!(MAGIA2, "SEELE-ENC/2");
        assert_eq!(BYTES_DO_COOKIE, 16);
    }

    #[test]
    fn o_cookie_nao_entra_na_mensagem_assinada() {
        // Ele prova o caminho de volta, e não a chave: a sala o junta ao
        // MORO2 sem assinar de novo. A mensagem assinada não o recebe, e o
        // mesmo pedido com e sem cookie difere só no último campo.
        let com = linha(&moro2(&um_moro2()));
        let sem = linha(&moro2(&Moro2 {
            cookie: None,
            ..um_moro2()
        }));
        let (com_resto, com_cookie) = com.rsplit_once(' ').expect("tem campos");
        let (sem_resto, sem_cookie) = sem.rsplit_once(' ').expect("tem campos");
        assert_eq!(com_resto, sem_resto);
        assert_eq!(sem_cookie, "-");
        assert_eq!(com_cookie.len(), 22);
    }
}
