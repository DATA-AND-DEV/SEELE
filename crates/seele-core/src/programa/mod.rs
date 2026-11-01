//! O poder `programa` da API 8 no núcleo: baixar com hash e tamanho declarados,
//! e rodar um processo (spec `2026-10-29-api-8-e-mod-multiplayer-design.md`,
//! §5).
//!
//! # O que este módulo é, e o que fica para a cola
//!
//! Tudo aqui recebe uma [`Politica`] explícita: **o que foi aceito** na máquina
//! (os domínios de origem, o comando e os argumentos, o teto de disco, a raiz
//! do servidor de jogo). Este módulo não sabe de onde ela vem. Ligar a
//! política ao registro do aceite (parte 3), os objetos `maquina.programa` do
//! QuickJS, os comandos do Tauri e a casca ficam para depois, e a ideia é que
//! cada um seja fino: um [`Programa`] por servidor de jogo aceito, criado com a
//! política daquele aceite, e jogado fora quando o aceite cai.
//!
//! # O produto sabe e conta
//!
//! Toda recusa é uma [`Recusa`] com os dados que a pessoa precisa (o domínio
//! pedido e os aceitos, o hash esperado e o obtido, o disco usado e o teto),
//! vai ao log com o nome dela, e **é contada** por nome
//! ([`Programa::recusas`]): a spec pede a contagem na tela do MOD.
//!
//! # Nada fica órfão
//!
//! O [`Programa`] é o dono de todo download e de todo processo que começou.
//! [`Programa::encerrar_tudo`] mata e espera cada um; soltar o `Programa` sem
//! isso manda matar todos ([`Drop`]), e cada filho é criado com
//! `kill_on_drop`, então nem o runtime caindo deixa um para trás.

mod aceite;
mod baixar;
mod caminho;
mod copia;
mod desempacotar;
mod empacotar;
mod escrever;
mod hash;
mod ler;
mod reservado;
mod rodar;
mod sobras;
#[cfg(windows)]
mod tarefa_windows;
mod trazer;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub use aceite::{plataforma_desta_maquina, politica_do_aceite};
pub use baixar::{
    Baixado, Corpo, Download, EventoDoDownload, FuturoDe, Origem, OrigemHttps, PedidoDeDownload,
    Resposta, MAX_REDIRECIONAMENTOS, MAX_TENTATIVAS,
};
pub use caminho::dentro_da_raiz;
pub use copia::{
    CopiaOferecida, CopiaRecebida, PedidoDeCopia, MAX_TENTATIVAS_DA_COPIA, PRAZO_SEM_BYTES_DA_COPIA,
};
pub use desempacotar::{Desempacotado, PedidoDeDesempacotar, MAX_ENTRADAS_DESEMPACOTADAS};
pub use empacotar::{Empacotado, MAX_TRAVADOS_NOMEADOS};
pub use escrever::MAX_BYTES_ESCRITOS;
pub use hash::{o_mais_forte, AlgoritmoDeHash, HashDeclarado};
pub use ler::MAX_BYTES_LIDOS;
pub use rodar::{
    EventoDoProcesso, FimDoProcesso, PedidoDeExecucao, Processo, MAX_BYTES_POR_LINHA,
    MAX_LINHAS_NA_FILA,
};
pub use sobras::{
    achar_sobras, anotar as anotar_processo, inicio_do_processo, parar_sobra, parar_sobras,
    prazo as prazo_das_sobras, DesfechoDaSobra, RegistroDeProcesso, SobraAParar, SobraEncontrada,
};
pub use trazer::Trazido;

/// O comando aceito: o programa, relativo à raiz, e os argumentos, um a um.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComandoAceito {
    /// O programa, relativo à raiz do servidor de jogo (por exemplo,
    /// `java/bin/java`). Nunca procurado no `PATH`.
    pub comando: String,
    /// Os argumentos, exatamente como foram mostrados no aceite.
    pub argumentos: Vec<String>,
}

/// Um artefato que o aceite mostrou: a URL dele só baixa com **este** hash e
/// **este** tamanho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtefatoAceito {
    /// A URL, como a oferta a trouxe (comparada como texto exato).
    pub url: String,
    /// O hash que a tela mostrou.
    pub hash: HashDeclarado,
    /// O tamanho que a tela somou.
    pub tamanho: u64,
}

/// O que foi aceito para um servidor de jogo nesta máquina.
///
/// **Duas portas para um download** (decisão Q1 de
/// `2026-10-31-api-8-programa-decisoes.md`): uma URL que é de um artefato
/// aceito só baixa com o hash e o tamanho dele; qualquer outra só baixa de um
/// host de `origens`, com o hash que o MOD declarar. É o que deixa a parte 6
/// baixar os arquivos de um modpack sem pôr cada um no aceite, sem deixar o
/// `server.jar` mostrado ser trocado por outro do mesmo host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Politica {
    /// Todo host que um download ou um salto de redirecionamento pode tocar:
    /// os dos artefatos aceitos e as origens. Vale o host exato ou um
    /// subdomínio dele (`mojang.com` aceita `piston-data.mojang.com`).
    pub dominios: Vec<String>,
    /// Os artefatos aceitos **para esta máquina**.
    pub artefatos: Vec<ArtefatoAceito>,
    /// As URLs dos artefatos aceitos para outros sistemas: recusadas pelo nome,
    /// para a tela que não os contou não ser desmentida pelo disco.
    pub de_outros_sistemas: Vec<String>,
    /// Os hosts de onde uma URL que não é artefato aceito pode baixar.
    pub origens: Vec<String>,
    /// O comando que pode rodar. `None`: nenhum.
    pub comando: Option<ComandoAceito>,
    /// **O passo de instalar** (API 8 parte 6): um segundo comando aceito,
    /// com os argumentos exatos (o instalador do loader de um modpack). `rodar`
    /// aceita um ou outro, e nada além dos dois.
    pub instalar: Option<ComandoAceito>,
    /// **O pack aceito, pelo hash** (`oferta.pack`). [`Programa::trazer`]
    /// só grava bytes que tenham este hash; `None`: nenhum pack.
    pub pack: Option<HashDeclarado>,
    /// O teto de disco da raiz, em bytes.
    pub teto_de_disco: u64,
    /// **As portas em que esta máquina pode oferecer uma cópia do mundo**
    /// (API 8 parte 7): as portas TCP dos túneis da oferta aceita. A cópia só
    /// sai desta máquina por um túnel que o dono aceitou servir; uma porta que
    /// a tela não mostrou não se abre.
    pub portas_de_copia: Vec<u16>,
    /// A pasta deste servidor de jogo, dentro da área do MOD. Criada se não
    /// existir.
    pub raiz: PathBuf,
}

/// Por que o `programa` recusou, com o que a pessoa precisa para entender.
///
/// O nome de cada uma ([`Recusa::nome`]) é de lista fechada, e é o que a tela
/// do MOD conta; a frase é do `Display`, para o log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Recusa {
    /// A URL não é `https://`.
    #[error("{url} não é https: só https é baixado")]
    NaoEHttps {
        /// A URL pedida.
        url: String,
    },
    /// A URL não se lê.
    #[error("{url} não é uma URL: {motivo}")]
    UrlInvalida {
        /// O texto pedido.
        url: String,
        /// O que o leitor de URL disse.
        motivo: String,
    },
    /// O host não é um dos domínios aceitos nem subdomínio de um.
    #[error("{host} está fora dos domínios aceitos ({})", aceitos.join(", "))]
    DominioForaDoAceito {
        /// O host pedido.
        host: String,
        /// Os domínios do aceite.
        aceitos: Vec<String>,
    },
    /// Um redirecionamento levou para fora dos domínios aceitos, ou para fora
    /// do `https`.
    #[error("{de} redirecionou para {para}, fora do aceito")]
    RedirecionouParaFora {
        /// De onde.
        de: String,
        /// Para onde.
        para: String,
    },
    /// Redirecionamentos demais.
    #[error("{url} redirecionou mais de {saltos} vezes")]
    RedirecionamentosDemais {
        /// A URL pedida.
        url: String,
        /// O teto ([`MAX_REDIRECIONAMENTOS`]).
        saltos: usize,
    },
    /// O pedido não declarou hash nenhum.
    #[error("{url} sem hash declarado: todo download tem hash e tamanho antes")]
    SemHash {
        /// A URL pedida.
        url: String,
    },
    /// O hash declarado não tem o tamanho do algoritmo, ou não é hex.
    #[error("o {algoritmo} declarado não é um {algoritmo} ({hex})")]
    HashMalFormado {
        /// O algoritmo declarado.
        algoritmo: AlgoritmoDeHash,
        /// O texto declarado.
        hex: String,
    },
    /// Caminho absoluto.
    #[error("{caminho} é absoluto: tudo é relativo à pasta do servidor de jogo")]
    CaminhoAbsoluto {
        /// O caminho pedido.
        caminho: String,
    },
    /// Caminho com `..`.
    #[error("{caminho} tem «..»: tudo fica dentro da pasta do servidor de jogo")]
    CaminhoComPai {
        /// O caminho pedido.
        caminho: String,
    },
    /// Caminho vazio onde é preciso um nome.
    #[error("o destino está vazio")]
    CaminhoVazio,
    /// Um pedaço do caminho que não é nome (`.`, uma letra de unidade no meio).
    #[error("{caminho} tem um pedaço que não é nome")]
    CaminhoInvalido {
        /// O caminho pedido.
        caminho: String,
    },
    /// Um link no caminho aponta para fora da raiz.
    #[error("{caminho} passa por um link que leva a {}", aponta_para.display())]
    LinkSaiDaRaiz {
        /// O caminho pedido.
        caminho: String,
        /// Onde o link cai.
        aponta_para: PathBuf,
    },
    /// O download não cabe no teto de disco.
    #[error("não cabe: a pasta usa {usado} bytes, o download pede {pedido}, e o teto é {teto}")]
    DiscoCheio {
        /// O que a raiz ocupa, mais o que os downloads em andamento reservaram.
        usado: u64,
        /// O que este download acrescenta (o tamanho declarado, menos o
        /// arquivo que ele substitui).
        pedido: u64,
        /// O teto do aceite.
        teto: u64,
    },
    /// A origem alegou (`Content-Length`) um tamanho diferente do declarado.
    #[error("{url}: o MOD declarou {declarado} bytes e a origem diz {alegado}")]
    TamanhoAlegadoDiferente {
        /// A URL.
        url: String,
        /// O que o MOD declarou.
        declarado: u64,
        /// O que a origem alegou.
        alegado: u64,
    },
    /// O corpo passou do tamanho declarado, e foi cortado ali.
    #[error("{url}: o corpo passou dos {declarado} bytes declarados e foi cortado em {recebido}")]
    MaiorQueODeclarado {
        /// A URL.
        url: String,
        /// O que o MOD declarou.
        declarado: u64,
        /// Quanto tinha chegado quando o corte aconteceu.
        recebido: u64,
    },
    /// O corpo acabou antes do tamanho declarado.
    #[error("{url}: o MOD declarou {declarado} bytes e chegaram {recebido}")]
    MenorQueODeclarado {
        /// A URL.
        url: String,
        /// O que o MOD declarou.
        declarado: u64,
        /// O que chegou.
        recebido: u64,
    },
    /// O hash não bate. O arquivo foi apagado.
    #[error("{url}: o {algoritmo} esperado era {esperado} e o obtido foi {obtido}; o arquivo foi apagado")]
    HashNaoBate {
        /// A URL.
        url: String,
        /// O algoritmo conferido (o mais forte declarado).
        algoritmo: AlgoritmoDeHash,
        /// O hash declarado, em minúsculas.
        esperado: String,
        /// O hash calculado.
        obtido: String,
    },
    /// A origem respondeu com um status que não é sucesso.
    #[error("{url} respondeu {status}")]
    OrigemRespondeu {
        /// A URL (a do último salto).
        url: String,
        /// O status HTTP.
        status: u16,
    },
    /// A origem não respondeu, ou o corpo parou no meio.
    #[error("{url} falhou: {motivo}")]
    OrigemFalhou {
        /// A URL (a do último salto).
        url: String,
        /// O que a camada de rede disse.
        motivo: String,
    },
    /// O disco recusou (criar a pasta, escrever, renomear).
    #[error("o disco recusou {}: {motivo}", caminho.display())]
    Disco {
        /// Onde.
        caminho: PathBuf,
        /// O que o sistema disse.
        motivo: String,
    },
    /// O comando pedido não é o aceito.
    #[error("{pedido} não é o comando aceito ({})", aceito.as_deref().unwrap_or("nenhum"))]
    ComandoForaDoAceito {
        /// O pedido.
        pedido: String,
        /// O aceito, se algum.
        aceito: Option<String>,
    },
    /// Os argumentos pedidos não são os aceitos.
    #[error("os argumentos pedidos ({pedidos:?}) não são os aceitos ({aceitos:?})")]
    ArgumentosForaDoAceito {
        /// Os pedidos.
        pedidos: Vec<String>,
        /// Os aceitos.
        aceitos: Vec<String>,
    },
    /// O sistema não iniciou o processo.
    #[error("{} não iniciou: {motivo}", comando.display())]
    NaoIniciou {
        /// O caminho do programa.
        comando: PathBuf,
        /// O que o sistema disse.
        motivo: String,
    },
    /// A entrada do processo fechou (ele terminou, ou fechou o `stdin`).
    #[error("a entrada do processo {processo} fechou: {motivo}")]
    EntradaFechada {
        /// O número do processo neste [`Programa`].
        processo: u64,
        /// O que o sistema disse.
        motivo: String,
    },
    /// O [`Programa`] já foi encerrado; nada novo começa.
    #[error("o programa deste servidor de jogo foi encerrado")]
    Encerrado,
    /// A URL é de um artefato aceito, e o pedido traz outro hash ou outro
    /// tamanho que o que a tela mostrou.
    #[error("{url}: o aceite mostrou {campo} {aceito}, e o pedido traz {pedido}")]
    DiferenteDoAceito {
        /// A URL.
        url: String,
        /// `hash` ou `tamanho`.
        campo: &'static str,
        /// O que o aceite mostrou.
        aceito: String,
        /// O que o MOD pediu.
        pedido: String,
    },
    /// A URL é de um artefato aceito para outro sistema.
    #[error("{url} é o artefato de outro sistema; esta máquina não o baixa")]
    ArtefatoDeOutroSistema {
        /// A URL.
        url: String,
    },
    /// A URL não é de artefato aceito, e o host dela não é uma das origens.
    #[error("{url} não é um artefato aceito, e o host não é uma das origens ({})", origens.join(", "))]
    ForaDosArtefatosAceitos {
        /// A URL.
        url: String,
        /// As origens do aceite.
        origens: Vec<String>,
    },
    /// O texto a escrever passa do teto ([`MAX_BYTES_ESCRITOS`]).
    #[error("o texto tem {tamanho} bytes, e o teto é {teto}")]
    TextoGrandeDemais {
        /// O tamanho do texto.
        tamanho: u64,
        /// O teto.
        teto: u64,
    },
    /// O arquivo a desempacotar não é `.tar.gz` nem `.zip`.
    #[error("{arquivo} não é .tar.gz nem .zip")]
    FormatoDesconhecido {
        /// O arquivo pedido.
        arquivo: String,
    },
    /// O pacote não se lê, ou traz uma entrada que sairia da pasta.
    #[error("{arquivo} não se desempacota: {motivo}")]
    PacoteInvalido {
        /// O arquivo pedido.
        arquivo: String,
        /// O que deu errado.
        motivo: String,
    },
    /// Nenhuma entrada do pacote está sob a raiz pedida.
    #[error("{arquivo} não tem nada sob {raiz}")]
    NadaSobARaiz {
        /// O arquivo pedido.
        arquivo: String,
        /// A raiz pedida.
        raiz: String,
    },
    /// O aceite não mostrou pack nenhum, e a metade pediu para trazer um.
    #[error("o aceite não mostrou pack nenhum: nada a trazer para {destino}")]
    SemPackAceito {
        /// Onde a metade queria o pack.
        destino: String,
    },
    /// Os bytes trazidos da metade de servidor não têm o hash do pack aceito.
    /// Nada foi gravado.
    #[error("o pack trazido não é o aceito: o {algoritmo} aceito era {esperado} e o dos bytes é {obtido}; nada foi gravado")]
    PackDiferente {
        /// O algoritmo do aceite.
        algoritmo: AlgoritmoDeHash,
        /// O hash que a tela mostrou.
        esperado: String,
        /// O hash dos bytes que chegaram.
        obtido: String,
    },
    /// O arquivo pedido a `ler` não é texto UTF-8.
    #[error("{arquivo} não é texto (UTF-8)")]
    NaoETexto {
        /// O arquivo pedido.
        arquivo: String,
    },
    /// A origem de `empacotar` não é uma pasta (API 8 parte 7).
    #[error("{origem} não é uma pasta: só uma pasta se empacota")]
    NaoEPasta {
        /// A origem pedida.
        origem: String,
    },
    /// O pacote cairia dentro da pasta que se empacota: ele se leria a si
    /// mesmo enquanto cresce.
    #[error("{arquivo} fica dentro de {origem}: o pacote não pode morar na pasta que ele copia")]
    ArquivoDentroDaOrigem {
        /// O pacote pedido.
        arquivo: String,
        /// A pasta que se empacota.
        origem: String,
    },
    /// A porta pedida para oferecer a cópia não é a de um túnel TCP aceito.
    #[error("a porta {porta} não é de um túnel aceito (as aceitas são {aceitas:?})")]
    PortaForaDoAceite {
        /// A porta pedida.
        porta: u16,
        /// As portas TCP dos túneis da oferta aceita.
        aceitas: Vec<u16>,
    },
    /// O sistema não deixou escutar na porta (outro programa a usa).
    #[error("a porta {porta} não abriu em 127.0.0.1: {motivo}")]
    PortaOcupada {
        /// A porta pedida.
        porta: u16,
        /// O que o sistema disse.
        motivo: String,
    },
    /// A máquina que oferece a cópia a recusou (outra cópia, código errado).
    #[error("a máquina que tem a cópia a recusou: {motivo}")]
    CopiaRecusada {
        /// O nome do motivo que ela deu (`codigo-errado`, `outra-copia`,
        /// `copia-mudou`, `alem-do-fim`, `tamanho-diferente`, `fala-errada`).
        motivo: String,
    },
    /// A ligação com a máquina que tem a cópia caiu mais vezes que o teto;
    /// o que chegou fica guardado, e a próxima vez continua dali.
    #[error("a cópia parou em {recebidos} de {tamanho} bytes depois de {tentativas} tentativas: {motivo}")]
    CopiaCaiu {
        /// Os bytes que já estão no disco.
        recebidos: u64,
        /// O tamanho da cópia.
        tamanho: u64,
        /// Quantas vezes se tentou.
        tentativas: u32,
        /// O que deu errado na última.
        motivo: String,
    },
    /// A cópia inteira chegou e o sha256 não é o anunciado. Nada foi gravado.
    #[error(
        "a cópia chegou com o sha256 {obtido}, e o anunciado era {esperado}; nada foi gravado"
    )]
    CopiaNaoBate {
        /// O sha256 anunciado.
        esperado: String,
        /// O sha256 do que chegou.
        obtido: String,
    },
    /// **A escrita cairia sobre o que o aceite roda** (revisão final, I1):
    /// o comando aceito (a pasta dele inteira, `java/`), ou um argumento que é
    /// caminho (`server.jar`). Só os bytes de um artefato aceito escrevem ali.
    #[error("{caminho} cairia sobre {reservado}, que o aceite prende: só um artefato aceito escreve ali")]
    ReservadoAoAceite {
        /// O caminho pedido.
        caminho: String,
        /// O caminho preso que ele tocaria, como a política o escreve.
        reservado: String,
    },
    /// **O comando aceito roda na pasta do servidor de jogo**, e só nela
    /// (revisão final, I1): os argumentos aceitos (`-jar server.jar`) se leem
    /// a partir da pasta, e uma pasta escolhida pelo MOD rodaria outro
    /// `server.jar`.
    #[error("o aceite roda o comando na pasta do servidor de jogo, e o pedido pede {pasta}")]
    PastaForaDoAceite {
        /// A pasta pedida.
        pasta: String,
    },
}

/// **Todo nome de [`Recusa`]**, na ordem do enum: o que `api/v8.json` escreve
/// em `recusas.programa`, e o que o guarda de conformidade amarra ao contrato.
pub const NOMES_DAS_RECUSAS: &[&str] = &[
    "nao-e-https",
    "url-invalida",
    "dominio-fora-do-aceito",
    "redirecionou-para-fora",
    "redirecionamentos-demais",
    "sem-hash",
    "hash-mal-formado",
    "caminho-absoluto",
    "caminho-com-pai",
    "caminho-vazio",
    "caminho-invalido",
    "link-sai-da-raiz",
    "disco-cheio",
    "tamanho-alegado-diferente",
    "maior-que-o-declarado",
    "menor-que-o-declarado",
    "hash-nao-bate",
    "origem-respondeu",
    "origem-falhou",
    "disco",
    "comando-fora-do-aceito",
    "argumentos-fora-do-aceito",
    "nao-iniciou",
    "entrada-fechada",
    "encerrado",
    "diferente-do-aceito",
    "artefato-de-outro-sistema",
    "fora-dos-artefatos-aceitos",
    "texto-grande-demais",
    "formato-desconhecido",
    "pacote-invalido",
    "nada-sob-a-raiz",
    "sem-pack-aceito",
    "pack-diferente",
    "nao-e-texto",
    "nao-e-pasta",
    "arquivo-dentro-da-origem",
    "porta-fora-do-aceite",
    "porta-ocupada",
    "copia-recusada",
    "copia-caiu",
    "copia-nao-bate",
    "reservado-ao-aceite",
    "pasta-fora-do-aceite",
];

impl Recusa {
    /// O nome, de lista fechada, como a tela do MOD o conta.
    #[must_use]
    pub const fn nome(&self) -> &'static str {
        match self {
            Self::NaoEHttps { .. } => "nao-e-https",
            Self::UrlInvalida { .. } => "url-invalida",
            Self::DominioForaDoAceito { .. } => "dominio-fora-do-aceito",
            Self::RedirecionouParaFora { .. } => "redirecionou-para-fora",
            Self::RedirecionamentosDemais { .. } => "redirecionamentos-demais",
            Self::SemHash { .. } => "sem-hash",
            Self::HashMalFormado { .. } => "hash-mal-formado",
            Self::CaminhoAbsoluto { .. } => "caminho-absoluto",
            Self::CaminhoComPai { .. } => "caminho-com-pai",
            Self::CaminhoVazio => "caminho-vazio",
            Self::CaminhoInvalido { .. } => "caminho-invalido",
            Self::LinkSaiDaRaiz { .. } => "link-sai-da-raiz",
            Self::DiscoCheio { .. } => "disco-cheio",
            Self::TamanhoAlegadoDiferente { .. } => "tamanho-alegado-diferente",
            Self::MaiorQueODeclarado { .. } => "maior-que-o-declarado",
            Self::MenorQueODeclarado { .. } => "menor-que-o-declarado",
            Self::HashNaoBate { .. } => "hash-nao-bate",
            Self::OrigemRespondeu { .. } => "origem-respondeu",
            Self::OrigemFalhou { .. } => "origem-falhou",
            Self::Disco { .. } => "disco",
            Self::ComandoForaDoAceito { .. } => "comando-fora-do-aceito",
            Self::ArgumentosForaDoAceito { .. } => "argumentos-fora-do-aceito",
            Self::NaoIniciou { .. } => "nao-iniciou",
            Self::EntradaFechada { .. } => "entrada-fechada",
            Self::Encerrado => "encerrado",
            Self::DiferenteDoAceito { .. } => "diferente-do-aceito",
            Self::ArtefatoDeOutroSistema { .. } => "artefato-de-outro-sistema",
            Self::ForaDosArtefatosAceitos { .. } => "fora-dos-artefatos-aceitos",
            Self::TextoGrandeDemais { .. } => "texto-grande-demais",
            Self::FormatoDesconhecido { .. } => "formato-desconhecido",
            Self::PacoteInvalido { .. } => "pacote-invalido",
            Self::NadaSobARaiz { .. } => "nada-sob-a-raiz",
            Self::SemPackAceito { .. } => "sem-pack-aceito",
            Self::PackDiferente { .. } => "pack-diferente",
            Self::NaoETexto { .. } => "nao-e-texto",
            Self::NaoEPasta { .. } => "nao-e-pasta",
            Self::ArquivoDentroDaOrigem { .. } => "arquivo-dentro-da-origem",
            Self::PortaForaDoAceite { .. } => "porta-fora-do-aceite",
            Self::PortaOcupada { .. } => "porta-ocupada",
            Self::CopiaRecusada { .. } => "copia-recusada",
            Self::CopiaCaiu { .. } => "copia-caiu",
            Self::CopiaNaoBate { .. } => "copia-nao-bate",
            Self::ReservadoAoAceite { .. } => "reservado-ao-aceite",
            Self::PastaForaDoAceite { .. } => "pasta-fora-do-aceite",
        }
    }
}

/// Se `host` é um dos `dominios` ou subdomínio de um.
///
/// Sem diferença de caixa e sem o ponto final (`mojang.com.` é `mojang.com`).
/// `evilmojang.com` **não** é subdomínio de `mojang.com`: a comparação é por
/// rótulo, com o ponto, e não por sufixo de texto.
#[must_use]
pub fn host_aceito(host: &str, dominios: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    dominios.iter().any(|dominio| {
        let dominio = dominio.trim_end_matches('.').to_ascii_lowercase();
        !dominio.is_empty()
            && (host == dominio
                || host
                    .strip_suffix(dominio.as_str())
                    .is_some_and(|antes| antes.ends_with('.')))
    })
}

/// O estado de um [`Programa`], dividido com as tarefas que ele começou.
pub(crate) struct Interior {
    pub(crate) politica: Politica,
    /// **O que o aceite prende** (I1): os caminhos que só os bytes de um
    /// artefato aceito escrevem. Sai da política, uma vez.
    pub(crate) presos: reservado::Presos,
    /// A raiz, canônica: é contra ela que um link é comparado.
    pub(crate) raiz: PathBuf,
    pub(crate) origem: Arc<dyn Origem>,
    recusas: Mutex<BTreeMap<&'static str, u64>>,
    /// Bytes declarados dos downloads em andamento, ainda não no disco.
    pub(crate) reservado: Mutex<u64>,
    /// **Os parciais dos downloads em andamento** (revisão final, I8): só
    /// estes ficam fora da conta do disco, porque a reserva já os conta pelo
    /// tamanho declarado. Antes, todo `*.seele-parcial` ficava fora — e um
    /// `destino: "a.seele-parcial"` passava do teto que o dono aceitou.
    pub(crate) parciais: Mutex<std::collections::HashSet<PathBuf>>,
    pub(crate) encerrado: AtomicBool,
    proximo: AtomicU64,
    /// Os downloads em andamento, para encerrar.
    pub(crate) downloads: Mutex<HashMap<u64, tokio::task::AbortHandle>>,
    /// Os processos vivos: o sinal de matar e a tarefa que os vigia.
    pub(crate) processos: Mutex<HashMap<u64, rodar::Vigia>>,
    /// A cópia do mundo que esta máquina oferece agora (API 8 parte 7): a
    /// escuta em `127.0.0.1`, uma só por vez.
    pub(crate) copia: Mutex<Option<tokio::task::AbortHandle>>,
    /// **Onde cada processo deixa o seu registro, e com que etiqueta**
    /// ([`Programa::anotar_processos_em`]; `sobras`). `None`: nenhum registro.
    pub(crate) anotacao: Mutex<Option<(PathBuf, String)>>,
}

impl Interior {
    /// Conta uma recusa, a escreve no log e a devolve.
    pub(crate) fn recusar(&self, recusa: Recusa) -> Recusa {
        tracing::warn!(
            recusa = recusa.nome(),
            raiz = %self.raiz.display(),
            "programa recusou: {recusa}"
        );
        if let Ok(mut contagem) = self.recusas.lock() {
            *contagem.entry(recusa.nome()).or_default() += 1;
        }
        recusa
    }

    /// Se `caminho` é o parcial de um download em andamento, que a reserva
    /// já conta: o que [`caminho::ocupado`] deixa de fora.
    pub(crate) fn parcial_vivo(&self, caminho: &Path) -> bool {
        self.parciais.lock().is_ok_and(|p| p.contains(caminho))
    }

    pub(crate) fn proximo_numero(&self) -> u64 {
        self.proximo.fetch_add(1, Ordering::Relaxed)
    }
}

/// O poder `programa` de um servidor de jogo aceito.
///
/// Um por aceite. Os métodos que começam algo em segundo plano
/// ([`Programa::baixar`], [`Programa::rodar`]) precisam ser chamados de dentro
/// de um runtime do Tokio.
pub struct Programa {
    pub(crate) interior: Arc<Interior>,
}

impl std::fmt::Debug for Programa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Programa")
            .field("raiz", &self.interior.raiz)
            .finish_non_exhaustive()
    }
}

impl Programa {
    /// Um `programa` com o que foi aceito, baixando por `origem`.
    ///
    /// Cria a raiz se ela não existir.
    ///
    /// # Errors
    ///
    /// [`Recusa::Disco`] se a raiz não pode ser criada nem lida.
    pub fn novo(politica: Politica, origem: Arc<dyn Origem>) -> Result<Self, Recusa> {
        let disco = |erro: std::io::Error| Recusa::Disco {
            caminho: politica.raiz.clone(),
            motivo: erro.to_string(),
        };
        std::fs::create_dir_all(&politica.raiz).map_err(disco)?;
        let raiz = std::fs::canonicalize(&politica.raiz).map_err(disco)?;
        Ok(Self {
            interior: Arc::new(Interior {
                presos: reservado::Presos::da_politica(&politica),
                politica,
                raiz,
                origem,
                recusas: Mutex::new(BTreeMap::new()),
                reservado: Mutex::new(0),
                parciais: Mutex::new(std::collections::HashSet::new()),
                encerrado: AtomicBool::new(false),
                proximo: AtomicU64::new(1),
                downloads: Mutex::new(HashMap::new()),
                processos: Mutex::new(HashMap::new()),
                copia: Mutex::new(None),
                anotacao: Mutex::new(None),
            }),
        })
    }

    /// A raiz, canônica.
    #[must_use]
    pub fn raiz(&self) -> &Path {
        &self.interior.raiz
    }

    /// Quantas recusas de cada nome ([`Recusa::nome`]) houve.
    #[must_use]
    pub fn recusas(&self) -> BTreeMap<&'static str, u64> {
        self.interior
            .recusas
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// **Cada processo que este `programa` rodar deixa um registro em
    /// `pasta`**, com `etiqueta`, até terminar (e2e de 31/10, passo 14): é o
    /// que deixa o app reaberto achar e parar o jogo de um app que morreu sem
    /// desligá-lo ([`parar_sobras`]). A pasta é do app, e não da raiz: o MOD
    /// escreve na raiz.
    pub fn anotar_processos_em(&self, pasta: PathBuf, etiqueta: String) {
        if let Ok(mut anotacao) = self.interior.anotacao.lock() {
            *anotacao = Some((pasta, etiqueta));
        }
    }

    /// Quantos processos estão vivos.
    #[must_use]
    pub fn processos_vivos(&self) -> usize {
        self.interior.processos.lock().map(|p| p.len()).unwrap_or(0)
    }

    /// Encerra tudo: cancela cada download (o parcial é apagado), mata cada
    /// processo e **espera** cada um terminar. Depois disso, nada novo começa
    /// ([`Recusa::Encerrado`]).
    ///
    /// Devolve quantos processos foram mortos.
    pub async fn encerrar_tudo(&self) -> usize {
        self.interior.encerrado.store(true, Ordering::SeqCst);
        let downloads: Vec<_> = self
            .interior
            .downloads
            .lock()
            .map(|mut d| d.drain().map(|(_, a)| a).collect())
            .unwrap_or_default();
        for download in downloads {
            download.abort();
        }
        copia::parar_de_oferecer(&self.interior);
        let vigias: Vec<rodar::Vigia> = self
            .interior
            .processos
            .lock()
            .map(|mut p| p.drain().map(|(_, v)| v).collect())
            .unwrap_or_default();
        let mortos = vigias.len();
        for vigia in vigias {
            vigia.matar_e_esperar().await;
        }
        mortos
    }
}

impl Drop for Programa {
    /// Sem `encerrar_tudo`, ao menos manda: cada download é cancelado e cada
    /// processo recebe a ordem de morrer, sem esperar.
    fn drop(&mut self) {
        self.interior.encerrado.store(true, Ordering::SeqCst);
        if let Ok(mut downloads) = self.interior.downloads.lock() {
            for (_, download) in downloads.drain() {
                download.abort();
            }
        }
        copia::parar_de_oferecer(&self.interior);
        if let Ok(processos) = self.interior.processos.lock() {
            for vigia in processos.values() {
                vigia.mandar_matar();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(lista: &[&str]) -> Vec<String> {
        lista.iter().map(|s| (*s).to_owned()).collect()
    }

    /// **`NOMES_DAS_RECUSAS` é cada `Recusa::nome`, na ordem do enum**: a
    /// lista que o contrato escreve (`recusas.programa`) não pode esquecer um
    /// nome que a metade de máquina recebe.
    #[test]
    fn os_nomes_das_recusas_sao_os_de_cada_variante() {
        let s = String::new;
        let p = PathBuf::new;
        let a = AlgoritmoDeHash::Sha1;
        let todas = [
            Recusa::NaoEHttps { url: s() },
            Recusa::UrlInvalida {
                url: s(),
                motivo: s(),
            },
            Recusa::DominioForaDoAceito {
                host: s(),
                aceitos: vec![],
            },
            Recusa::RedirecionouParaFora { de: s(), para: s() },
            Recusa::RedirecionamentosDemais {
                url: s(),
                saltos: 0,
            },
            Recusa::SemHash { url: s() },
            Recusa::HashMalFormado {
                algoritmo: a,
                hex: s(),
            },
            Recusa::CaminhoAbsoluto { caminho: s() },
            Recusa::CaminhoComPai { caminho: s() },
            Recusa::CaminhoVazio,
            Recusa::CaminhoInvalido { caminho: s() },
            Recusa::LinkSaiDaRaiz {
                caminho: s(),
                aponta_para: p(),
            },
            Recusa::DiscoCheio {
                usado: 0,
                pedido: 0,
                teto: 0,
            },
            Recusa::TamanhoAlegadoDiferente {
                url: s(),
                declarado: 0,
                alegado: 0,
            },
            Recusa::MaiorQueODeclarado {
                url: s(),
                declarado: 0,
                recebido: 0,
            },
            Recusa::MenorQueODeclarado {
                url: s(),
                declarado: 0,
                recebido: 0,
            },
            Recusa::HashNaoBate {
                url: s(),
                algoritmo: a,
                esperado: s(),
                obtido: s(),
            },
            Recusa::OrigemRespondeu {
                url: s(),
                status: 0,
            },
            Recusa::OrigemFalhou {
                url: s(),
                motivo: s(),
            },
            Recusa::Disco {
                caminho: p(),
                motivo: s(),
            },
            Recusa::ComandoForaDoAceito {
                pedido: s(),
                aceito: None,
            },
            Recusa::ArgumentosForaDoAceito {
                pedidos: vec![],
                aceitos: vec![],
            },
            Recusa::NaoIniciou {
                comando: p(),
                motivo: s(),
            },
            Recusa::EntradaFechada {
                processo: 0,
                motivo: s(),
            },
            Recusa::Encerrado,
            Recusa::DiferenteDoAceito {
                url: s(),
                campo: "hash",
                aceito: s(),
                pedido: s(),
            },
            Recusa::ArtefatoDeOutroSistema { url: s() },
            Recusa::ForaDosArtefatosAceitos {
                url: s(),
                origens: vec![],
            },
            Recusa::TextoGrandeDemais {
                tamanho: 0,
                teto: 0,
            },
            Recusa::FormatoDesconhecido { arquivo: s() },
            Recusa::PacoteInvalido {
                arquivo: s(),
                motivo: s(),
            },
            Recusa::NadaSobARaiz {
                arquivo: s(),
                raiz: s(),
            },
            Recusa::SemPackAceito { destino: s() },
            Recusa::PackDiferente {
                algoritmo: a,
                esperado: s(),
                obtido: s(),
            },
            Recusa::NaoETexto { arquivo: s() },
            Recusa::NaoEPasta { origem: s() },
            Recusa::ArquivoDentroDaOrigem {
                arquivo: s(),
                origem: s(),
            },
            Recusa::PortaForaDoAceite {
                porta: 0,
                aceitas: vec![],
            },
            Recusa::PortaOcupada {
                porta: 0,
                motivo: s(),
            },
            Recusa::CopiaRecusada { motivo: s() },
            Recusa::CopiaCaiu {
                recebidos: 0,
                tamanho: 0,
                tentativas: 0,
                motivo: s(),
            },
            Recusa::CopiaNaoBate {
                esperado: s(),
                obtido: s(),
            },
            Recusa::ReservadoAoAceite {
                caminho: s(),
                reservado: s(),
            },
            Recusa::PastaForaDoAceite { pasta: s() },
        ];
        let nomes: Vec<&str> = todas.iter().map(Recusa::nome).collect();
        assert_eq!(nomes, NOMES_DAS_RECUSAS);
    }

    #[test]
    fn host_exato_ou_subdominio_por_rotulo() {
        let aceitos = d(&["mojang.com", "Adoptium.NET."]);
        assert!(host_aceito("mojang.com", &aceitos));
        assert!(host_aceito("piston-data.mojang.com", &aceitos));
        assert!(host_aceito("PISTON-DATA.MOJANG.COM.", &aceitos));
        assert!(host_aceito("api.adoptium.net", &aceitos));
        assert!(!host_aceito("evilmojang.com", &aceitos));
        assert!(!host_aceito("mojang.com.evil.net", &aceitos));
        assert!(!host_aceito("com", &aceitos));
        assert!(!host_aceito("", &aceitos));
        assert!(!host_aceito("mojang.com", &d(&[""])));
    }
}
