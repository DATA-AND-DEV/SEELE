//! MEDIA encaminha a tela, como já encaminha a voz.
//!
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` §5.1,
//! decidido em 22/08/2026: **o servidor encaminha.** A alternativa B pedia um
//! caminho cliente↔cliente que este produto nunca teve e só trocaria quem paga;
//! a C — só quem hospeda compartilha — entregava o recurso pela metade.
//!
//! A onda 1 deixou o plano de controle inteiro (quem começou, quem parou, quem
//! pediu quadro-chave) e **nada bombeando os bytes**. Isto é a bomba.
//!
//! # Fluxo, e nunca datagrama
//!
//! O §3.1 é medido e não argumentado: `send_datagram` põe voz e vídeo na mesma
//! fila FIFO do `quinn-proto`, que descarta o **mais velho** quando enche —
//! 16,1% da voz perdida e 2,16 s de atraso com o buffer padrão, e 98,1%
//! descartada ao encolher o buffer. Nada aqui toca em datagrama: o que chega
//! num fluxo unidirecional sai em fluxos unidirecionais.
//!
//! # O servidor nunca olha dentro do quadro
//!
//! É a mesma regra que `specs/04-servidor-seele.md` dá para o Opus, e pelo
//! mesmo motivo: é ela que mantém a CPU do servidor plana e que deixa o E2EE de
//! mídia (`specs/09`) ser um acréscimo em vez de uma reescrita. O que o
//! [`Enquadramento`] lê são os cinco bytes que separam um quadro do outro —
//! tipo e tamanho — e nada além disso. Cinco bytes não são um decodificador.
//!
//! # Por que estas constantes estão repetidas
//!
//! O ADR 0002 proíbe o daemon de depender do `seele-core`, que é o *cliente*.
//! `CABECALHO_DE_QUADRO_LEN`, `MAX_QUADRO_LEN`, [`FRACAO_DO_CAMINHO`] e
//! [`PISO_DE_BANDA_BPS`] existem lá em `seele_core::tela` com estes mesmos
//! valores, pela mesma razão que `crate::frame` é gêmeo de `seele_core::frame`
//! e que o balde de bytes é gêmeo do daqui: quarenta linhas repetidas custam
//! menos que um crate de transporte que os dois dependeriam e nenhum seria
//! dono. **O que não pode divergir é o formato**, e ele está escrito nos dois
//! lados a partir do mesmo §3.

use std::time::{Duration, Instant};

use seele_proto::ids::ScreenId;
use tokio::sync::mpsc;

/// Bytes de cabeçalho na frente de cada quadro codificado.
///
/// Um byte de tipo e quatro de tamanho, big-endian. Gêmeo de
/// `seele_core::tela::CABECALHO_DE_QUADRO_LEN` — ver o cabeçalho deste módulo.
pub const CABECALHO_DE_QUADRO_LEN: usize = 5;
/// O byte de tipo de um quadro de imagem que basta a si mesmo.
///
/// Gêmeo de `seele_core::tela::TipoDeQuadro::Chave` — este módulo não depende
/// daquele crate, e os dois lados do fio concordam por escrito e não por tipo.
pub const TIPO_CHAVE: u8 = 1;
/// O byte de tipo de um quadro de som.
///
/// Gêmeo de `seele_core::tela::TipoDeQuadro::Som`. O maior que este
/// enquadramento aceita: ver a conferência em [`Enquadramento::entrada`].
pub const TIPO_SOM: u8 = 2;

/// Maior quadro codificado que este servidor repassa, em bytes.
///
/// `specs/08-seguranca.md`: o tamanho anunciado por um par é conferido **antes**
/// de qualquer alocação. Aqui ele não aloca nada — o encaminhamento é por
/// pedaço, sem remontar quadro —, mas um tamanho absurdo é a única coisa que
/// distingue um fluxo de tela de um fluxo de lixo, e um enquadramento que
/// aceitasse 4 GiB nunca perceberia que perdeu o passo.
pub const MAX_QUADRO_LEN: usize = 512 * 1024;

/// Que fração do caminho medido o vídeo pode ocupar, em por cento.
///
/// 60, e é medida: com o vídeo pedindo 1200 kbps num caminho de 2000, a voz
/// volta para 23,1 ms de p50 e 0% de perda; solto, ela vai a 225,7 ms no mesmo
/// cano (§3.2).
pub const FRACAO_DO_CAMINHO: u32 = 60;

/// Abaixo deste teto o compartilhamento **para**, em bits por segundo.
///
/// §2 pede piso com nome: *«se o encoder não sustenta nem o piso, o
/// compartilhamento para, com motivo enumerado»*. O número é extrapolação —
/// nenhuma linha de `spikes/tela-no-codec` rodou abaixo de 1200 kbps de teto —
/// e está aqui com o mesmo valor que `seele_core::tela::PISO_DE_BANDA_BPS`.
pub const PISO_DE_BANDA_BPS: u32 = 200_000;

/// O caminho de subida que se **assume** para este servidor, em bits por segundo.
///
/// **Hipótese, e escrita como hipótese.** O §8 pergunta 2 continua aberta —
/// ninguém mede quanto cabe num caminho que não está sendo enchido — e o
/// produto não tem resposta. Assume-se o cano sobre o qual as duas provas
/// rodaram, 2000 kbps de subida, que é a única suposição com número atrás.
///
/// É o número que o §5.1 chama de *«caminho de quem hospeda»*, e é a perna que
/// o produto até agora **não media**: o teto saía do caminho de quem
/// compartilha, e com o servidor encaminhando é a subida do servidor que estoura
/// primeiro.
///
/// Só a admissão deste lado sai daqui. **No fio ele não vai** — ver
/// [`caminho_no_fio`], e a diferença entre os dois é o assunto inteiro destas
/// vinte linhas.
pub const CAMINHO_DO_SERVER_BPS: u32 = 2_000_000;

/// A subida por que este servidor divide N ao admitir uma transmissão.
///
/// O que o operador declarou, ou a hipótese de [`CAMINHO_DO_SERVER_BPS`].
/// Zero declarado é tratado como nada declarado: um caminho de zero bit por
/// segundo pararia toda transmissão deste servidor, e um campo de configuração em
/// branco não é um pedido para desligar o recurso.
///
/// Recusar-se a admitir sem um número seria a outra escolha, e é pior: um
/// servidor que não sabe a própria subida ainda tem de decidir o que faz quando
/// alguém aperta o botão, e a hipótese de 2000 kbps é conservadora — ela erra
/// para o lado de encerrar cedo, que é o lado que o §3.2 manda errar.
#[must_use]
pub fn caminho_do_server(declarado: Option<u32>) -> u32 {
    declarado
        .filter(|bps| *bps > 0)
        .unwrap_or(CAMINHO_DO_SERVER_BPS)
}

/// O que o servidor diz da própria subida no `HostUplink`, em bits por segundo.
///
/// **Zero quer dizer «não medi»**, o mesmo contrato do `——` que o resto do
/// produto usa, e quem recebe trata isso como ausência — o termo do §5.1 some
/// do `min` em vez de virar um teto de zero.
///
/// A diferença para [`caminho_do_server`] é que a hipótese **não atravessa o
/// fio**. Dentro desta máquina ela é uma decisão de admissão, e assumir é o que
/// se faz na falta de medida; posta no fio ela vira uma promessa de banda que
/// ninguém conferiu, e o cliente a usaria para escolher resolução. Uma medida
/// inventada é pior que a ausência declarada.
///
/// **Agora há medida, e a objeção que estava escrita aqui continua certa.** Este
/// parágrafo dizia que somar o que já saiu daria «um piso demonstrado, não uma
/// capacidade», e que num servidor parado isso desabaria abaixo do piso do §2 e
/// pararia transmissões que cabiam. Está certo — e é exatamente por isso que
/// [`Subida`] **descarta** toda janela que não encheu o teto: uma sala parada
/// não é notícia sobre o cano. Só janela cheia move a estimativa, e é a mesma
/// disciplina que a sonda do cliente usa.
///
/// A ordem é medida, depois declarado, depois nada. A medida vence o declarado
/// porque o operador declara de memória e a medida vem do cano; e a **hipótese**
/// continua sem atravessar o fio, porque ela não é nem uma coisa nem outra —
/// ver [`Subida::medida`].
#[must_use]
pub fn caminho_no_fio(declarado: Option<u32>, medido: Option<u32>) -> u32 {
    medido
        .or_else(|| declarado.filter(|bps| *bps > 0))
        .unwrap_or(0)
}

/// Quantas aberturas de transmissão esperam por espectador.
///
/// Abrir é raro — uma por transmissão —, então isto é folga e não medida. O
/// que precisa de fila é o corpo, e ele tem a sua em [`Pedaco`].
pub const ABERTURAS_DEPTH: usize = 4;

/// Quantos pedaços de tela esperam por espectador antes do corte.
///
/// O teto de memória do servidor sai daqui: no pior caso são
/// `ABERTURAS_DEPTH + PEDACOS_DEPTH` pedaços de [`LEITURA_LEN`] por espectador,
/// meio megabyte, e um servidor é dimensionado em 512 MB
/// (`specs/04-servidor-seele.md`). Cheia, a fila **corta aquele espectador** —
/// nunca descarta um pedaço — pelo motivo que [`Pedaco`] escreve.
pub const PEDACOS_DEPTH: usize = 64;

/// Quantos bytes do fluxo de quem compartilha se lê de uma vez.
pub const LEITURA_LEN: usize = 8 * 1024;

/// Prioridade do fluxo de tela, abaixo de tudo o mais que o servidor escreve.
///
/// O controle é `crate::transfer::CONTROL_PRIORITY` e as transferências são
/// `TRANSFER_PRIORITY`. A tela fica abaixo das duas, e a ordem importa menos do
/// que parece: o §3.2 é explícito em que prioridade dentro do QUIC **não
/// alcança** a fila do gargalo, que é onde a voz sofre. Isto só arruma a ordem
/// de saída desta máquina.
pub const PRIORIDADE_DA_TELA: i32 = -2;

/// Código com que um fluxo de tela é cortado.
///
/// Cortar e não terminar, e a diferença é a frase que quem assiste lê: um fluxo
/// **terminado** é «a transmissão acabou», e um fluxo **cortado** é «a sua
/// cópia se perdeu». Terminar um fluxo truncado ensinaria o espectador a
/// chamar de fim o que foi uma queda.
pub const CODIGO_DE_CORTE: u32 = 1;

/// O teto que a subida do servidor impõe a cada cópia, em bits por segundo.
///
/// É a **primeira linha** do `min` do §5.1, e é a linha que faltava:
///
/// ```text
/// teto = min(
///     caminho de quem HOSPEDA × 60% ÷ N espectadores,   ← esta
///     caminho de quem COMPARTILHA × 60%,
///     o que a pessoa escolheu (§5),
/// )
/// ```
///
/// `None` quando nem [`PISO_DE_BANDA_BPS`] cabe: aí não há teto baixo, há a
/// resposta de que esta subida não carrega esta sala. Zero espectador conta
/// como um, porque uma transmissão que ninguém assiste ainda é uma transmissão
/// que a primeira pessoa a entrar vai assistir.
#[must_use]
pub fn teto_do_hospedeiro(caminho_bps: u32, espectadores: usize) -> Option<u32> {
    // Em `u64` pelo motivo que `seele_core::tela` já dá: `caminho × 60` estoura
    // `u32` a partir de uns 71 Mbit/s, que é uma fibra doméstica comum, e um
    // teto que dá a volta vira um teto minúsculo — o defeito apareceria só na
    // casa boa.
    let cabe = (u64::from(caminho_bps) * u64::from(FRACAO_DO_CAMINHO)) / 100;
    let n = espectadores.max(1) as u64;
    let por_espectador = u32::try_from(cabe / n).unwrap_or(u32::MAX);
    (por_espectador >= PISO_DE_BANDA_BPS).then_some(por_espectador)
}

// ---------------------------------------------------------------------------
// A subida deste servidor, medida em vez de suposta
// ---------------------------------------------------------------------------

/// De quanto em quanto tempo a subida é reavaliada.
///
/// Um segundo, e é o mesmo número da sonda do cliente de propósito: uma janela
/// curta demais mede rajada de codificador em vez de caminho, e uma longa demais
/// deixa a estimativa atrás da realidade justamente quando a sala cresce.
pub const JANELA_DA_SUBIDA: Duration = Duration::from_secs(1);

/// Quanto do permitido a janela tem de ter gasto para valer como medida.
///
/// **A objeção que este módulo escreveu contra si mesmo, e a resposta.** O doc
/// de [`caminho_no_fio`] dizia que somar o que saiu daria «um piso demonstrado,
/// não uma capacidade», e que num servidor parado isso desabaria abaixo do piso
/// e pararia transmissões que cabiam. Está certo, e é exatamente por isso que
/// uma janela que **não** encheu o teto nunca move a estimativa: ela não diz
/// nada sobre o cano, diz que não havia o que mandar.
///
/// 85% pelo mesmo motivo da sonda do cliente: `spikes/tela-no-codec` mediu o
/// OpenH264 entregando 872 de 1200 kbps em 720p, e exigir mais descartaria toda
/// janela boa.
pub const OCUPACAO_MINIMA: u32 = 85;
/// A partir de que fração de perda a estimativa recua, em por cento.
///
/// **A estimativa recuava com um pacote perdido, e era isso que a fazia
/// oscilar.** A condição era binária — «perdeu algum, ou houve algum evento de
/// congestionamento» —, e um pacote perdido por segundo é rotina em qualquer
/// rede com wifi no meio. O efeito em campo foi relatado assim: «às vezes ela
/// ficava boa, às vezes ficava péssima, não tinha uma ordem certa». A estimativa
/// subia 25%, um pacote caía, ela desabava, e recomeçava.
///
/// Nenhum controle de congestionamento sério lê perda assim. O do WebRTC recua
/// acima de 10%, sobe abaixo de 2%, e **segura** entre os dois — porque a perda
/// que importa é a que o cano produz por estar cheio, e um pacote solto não diz
/// nada sobre o tamanho do cano.
///
/// Os dois números daqui são os de lá, e não uma invenção: são o ponto de
/// operação de uma década de vídeo em tempo real na internet.
pub const PERDA_QUE_DOI: u32 = 10;
/// Abaixo de que fração de perda a estimativa pode subir, em por cento.
///
/// Entre [`PERDA_QUE_DOI`] e este número a estimativa **segura**: nem sobe nem
/// desce. É a faixa em que a perda existe e não é o cano falando — insistir em
/// subir ali é procurar a dor, e recuar é abrir mão de banda que está lá.
pub const PERDA_QUE_ACALMA: u32 = 2;

/// Quanto a estimativa sobe por janela cheia e sem dor, em por cento.
pub const SUBIDA: u32 = 125;

/// Quanto a estimativa recua quando doeu e não dá para dizer o tamanho.
pub const QUEDA: u32 = 80;

/// O maior valor que a estimativa pode alcançar, em bits por segundo.
///
/// Cinquenta megabits, e o número importa menos do que parece: a subida é de
/// 25% por janela **cheia**, e encher exige o vídeo de fato gastar o que recebeu.
/// Sair de 2 Mbps e chegar aqui leva uns quinze segundos de tráfego real — se o
/// cano não carrega, a estimativa para no caminho sozinha.
pub const TETO_DA_SUBIDA_BPS: u32 = 50_000_000;

/// O menor valor que a estimativa pode alcançar.
///
/// Conta, e não literal: é o ponto em que [`FRACAO_DO_CAMINHO`] ainda deixa
/// [`PISO_DE_BANDA_BPS`] para um espectador. Abaixo daqui quem responde é o
/// `None` de [`teto_do_hospedeiro`], que é a resposta com motivo — e não uma
/// estimativa cada vez menor.
///
/// **`div_ceil` e não `/`.** A divisão inteira truncava para 333 333, e
/// 333 333 × 60% devolve 199 999 — um bit abaixo do piso, fazendo
/// [`teto_do_hospedeiro`] responder `None` por arredondamento em vez de por
/// decisão. O teste `a_subida_nunca_cai_abaixo_do_que_o_piso_do_video_exige`
/// existe por causa disso e confere as duas pontas, não só a constante.
pub const PISO_DA_SUBIDA_BPS: u32 =
    ((PISO_DE_BANDA_BPS as u64 * 100).div_ceil(FRACAO_DO_CAMINHO as u64)) as u32;

const _: () = assert!(
    (PISO_DA_SUBIDA_BPS as u64 * FRACAO_DO_CAMINHO as u64) / 100 >= PISO_DE_BANDA_BPS as u64,
    "o piso da subida não sustenta o piso da banda depois da fração"
);

/// O que as conexões do servidor contaram neste instante, somadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LeituraDaSubida {
    /// `udp_tx.bytes` de **todas** as conexões, somado.
    ///
    /// Todas e não só as que assistem: o cano é um só, e voz, controle e tela
    /// disputam o mesmo. O que se mede é o cano.
    pub bytes_enviados: u64,
    /// `path.lost_packets`, somado.
    pub pacotes_perdidos: u64,
    /// Quantos datagramas saíram, para a perda virar **fração**.
    ///
    /// Sem este número a perda é uma contagem, e uma contagem não distingue um
    /// pacote em dez mil de mil em dez mil. Ver a nota de [`PERDA_QUE_DOI`].
    pub pacotes_enviados: u64,
    /// `path.congestion_events`, somado.
    pub eventos_de_congestionamento: u64,
    /// O que a sala tinha licença de gastar na janela, em bits por segundo.
    ///
    /// [`teto_do_hospedeiro`] vezes o número de espectadores — o teto é **por
    /// cópia**, e o que sai do cano são todas elas.
    pub permitido_bps: u32,
}

/// Onde uma janela começou.
#[derive(Debug, Clone, Copy)]
struct JanelaDaSubida {
    abriu: Instant,
    leitura: LeituraDaSubida,
}

/// A profundidade da subida deste servidor, medida enquanto ele empurra cópias.
///
/// **Parente da sonda do cliente, e não a mesma coisa.** Aquela mede *um*
/// caminho — o do compartilhador até aqui. Esta mede o cano deste servidor
/// carregando N cópias para N pares diferentes, então soma sobre conexões e
/// trata dor em qualquer uma como dor no cano. A regra de descartar janela
/// limitada por conteúdo é a mesma, e é o que impede as duas de confundir
/// «ninguém mandou nada» com «o cano é estreito».
///
/// Pura: recebe o instante e a leitura, devolve a estimativa. Não lê relógio,
/// não fala com o `quinn`.
#[derive(Debug, Clone, Copy)]
pub struct SondaDaSubida {
    estimativa_bps: u32,
    /// Até onde a subida larga pode ir depois da primeira dor.
    limite_bps: Option<u32>,
    janela: Option<JanelaDaSubida>,
}

impl Default for SondaDaSubida {
    fn default() -> Self {
        Self::nova()
    }
}

impl SondaDaSubida {
    /// Começa na suposição de sempre, que continua sendo o único número com
    /// medida atrás — ver [`CAMINHO_DO_SERVER_BPS`].
    #[must_use]
    pub const fn nova() -> Self {
        Self {
            estimativa_bps: CAMINHO_DO_SERVER_BPS,
            limite_bps: None,
            janela: None,
        }
    }

    /// A subida que este servidor acredita ter agora.
    #[must_use]
    pub const fn estimativa(&self) -> u32 {
        self.estimativa_bps
    }

    /// Uma leitura. Devolve a estimativa nova **quando ela mudou**.
    ///
    /// `None` na maior parte das vezes, e isso é o normal: a janela ainda não
    /// fechou, ou fechou sem dizer nada sobre o cano.
    pub fn observar(&mut self, agora: Instant, leitura: &LeituraDaSubida) -> Option<u32> {
        let Some(janela) = self.janela else {
            self.janela = Some(JanelaDaSubida {
                abriu: agora,
                leitura: *leitura,
            });
            return None;
        };
        let decorrido = agora.saturating_duration_since(janela.abriu);
        if decorrido < JANELA_DA_SUBIDA {
            return None;
        }
        self.janela = Some(JanelaDaSubida {
            abriu: agora,
            leitura: *leitura,
        });

        let antes = self.estimativa_bps;
        let bytes = leitura
            .bytes_enviados
            .saturating_sub(janela.leitura.bytes_enviados);
        let segundos = decorrido.as_secs_f64().max(0.001);
        let entregue_bps =
            u32::try_from(((bytes as f64 * 8.0) / segundos) as u64).unwrap_or(u32::MAX);

        // A perda como **fração do que saiu**, e não como contagem. Ver
        // `PERDA_QUE_DOI`: a contagem fazia um pacote solto derrubar a
        // estimativa, e um pacote solto por segundo é rotina.
        let perdidos = leitura
            .pacotes_perdidos
            .saturating_sub(janela.leitura.pacotes_perdidos);
        let enviados = leitura
            .pacotes_enviados
            .saturating_sub(janela.leitura.pacotes_enviados);
        // Sem pacote nenhum na janela não há fração a calcular, e zero é a
        // resposta certa: nada saiu, nada se perdeu.
        let perda_pct = perdidos
            .saturating_mul(100)
            .checked_div(enviados)
            .and_then(|pct| u32::try_from(pct).ok())
            .unwrap_or(0);
        let doeu = perda_pct >= PERDA_QUE_DOI;
        // A faixa do meio segura: nem sobe nem desce. É onde a perda existe e
        // não é o cano falando.
        let calma = perda_pct <= PERDA_QUE_ACALMA;
        let permitido = janela.leitura.permitido_bps;
        let cheia = permitido > 0
            && u64::from(entregue_bps) * 100 >= u64::from(permitido) * u64::from(OCUPACAO_MINIMA);

        if doeu && cheia {
            // Doeu enchendo: o cano acaba onde a janela entregou. É a única
            // leitura que dá um número em vez de uma direção.
            self.estimativa_bps = entregue_bps;
            self.limite_bps = Some(entregue_bps);
        } else if doeu {
            // Doeu sem encher — outra coisa apertou o caminho. Recua um passo,
            // sem fingir saber o tamanho.
            self.estimativa_bps = (u64::from(antes) * u64::from(QUEDA) / 100) as u32;
        } else if cheia && calma {
            let passo = (u64::from(antes) * u64::from(SUBIDA) / 100) as u32;
            let teto = self.limite_bps.unwrap_or(TETO_DA_SUBIDA_BPS);
            self.estimativa_bps = passo.min(teto).min(TETO_DA_SUBIDA_BPS);
        }
        // E o quarto caso — não doeu e não encheu — não move nada. É a tela
        // parada, e ela não é notícia sobre o cano.

        self.estimativa_bps = self
            .estimativa_bps
            .clamp(PISO_DA_SUBIDA_BPS, TETO_DA_SUBIDA_BPS);
        (self.estimativa_bps != antes).then_some(self.estimativa_bps)
    }
}

/// O que **uma** conexão contou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LeituraDaConexao {
    /// `udp_tx.bytes`.
    pub bytes_enviados: u64,
    /// `path.lost_packets`.
    pub pacotes_perdidos: u64,
    /// Quantos datagramas saíram, para a perda virar **fração**.
    ///
    /// Sem este número a perda é uma contagem, e uma contagem não distingue um
    /// pacote em dez mil de mil em dez mil. Ver a nota de [`PERDA_QUE_DOI`].
    pub pacotes_enviados: u64,
    /// `path.congestion_events`.
    pub eventos_de_congestionamento: u64,
}

impl From<&quinn::ConnectionStats> for LeituraDaConexao {
    fn from(stats: &quinn::ConnectionStats) -> Self {
        Self {
            bytes_enviados: stats.udp_tx.bytes,
            pacotes_perdidos: stats.path.lost_packets,
            pacotes_enviados: stats.udp_tx.datagrams,
            eventos_de_congestionamento: stats.path.congestion_events,
        }
    }
}

/// A subida do servidor, somada sobre as conexões que existem agora.
///
/// **Por que somar aqui e não em cada sessão.** O cano é um só e as conexões são
/// muitas: uma sessão sozinha vê a fatia dela e nunca o total, e é o total que
/// enche o cano. Este é o único lugar do servidor que enxerga todas.
///
/// Guarda o último contador de cada conexão, e não um acumulado próprio, porque
/// os contadores do `quinn` são por conexão e morrem com ela — somar deltas de
/// uma conexão que fechou contaria a saída dela duas vezes, uma no delta e outra
/// na ausência.
#[derive(Debug, Default)]
pub struct Subida {
    sonda: SondaDaSubida,
    por_conexao: std::collections::HashMap<u64, LeituraDaConexao>,
    /// Se a estimativa já saiu da suposição alguma vez.
    ///
    /// **A hipótese não atravessa o fio**, e o doc de [`caminho_no_fio`] diz por
    /// quê: posta lá ela vira uma promessa de banda que ninguém conferiu. Só
    /// depois de uma janela cheia mover a estimativa é que existe medida para
    /// declarar.
    mediu: bool,
    /// Quantos assistem à transmissão de agora. Zero quando não há nenhuma.
    ///
    /// Mora aqui porque é ele que decide **o que o cano tinha licença de
    /// gastar**, e sem isso não há como distinguir «não coube» de «não havia o
    /// que mandar» — a distinção inteira desta medida.
    espectadores: u32,
}

impl Subida {
    /// Uma subida que ainda não mediu nada.
    #[must_use]
    pub fn nova() -> Self {
        Self {
            sonda: SondaDaSubida::nova(),
            por_conexao: std::collections::HashMap::new(),
            mediu: false,
            espectadores: 0,
        }
    }

    /// A subida **medida**, ou `None` enquanto ela ainda é a suposição.
    ///
    /// É o único valor que pode ir para o fio.
    #[must_use]
    pub const fn medida(&self) -> Option<u32> {
        if self.mediu {
            Some(self.sonda.estimativa())
        } else {
            None
        }
    }

    /// Quantos estão assistindo agora. Zero quando a transmissão acabou.
    pub fn assistindo(&mut self, quantos: u32) {
        self.espectadores = quantos;
    }

    /// O que o cano tinha licença de gastar, em bits por segundo.
    ///
    /// [`teto_do_hospedeiro`] é **por cópia**; o que sai do cano são todas elas.
    /// Zero sem transmissão, e é o que faz toda janela ser descartada nesse
    /// caso.
    fn permitido_bps(&self) -> u32 {
        if self.espectadores == 0 {
            return 0;
        }
        teto_do_hospedeiro(self.sonda.estimativa(), self.espectadores as usize)
            .map_or(0, |por_copia| por_copia.saturating_mul(self.espectadores))
    }

    /// A subida que este servidor acredita ter agora.
    #[must_use]
    pub const fn estimativa(&self) -> u32 {
        self.sonda.estimativa()
    }

    /// Uma leitura de uma conexão. Devolve a estimativa **quando ela mudou**.
    ///
    /// O que o cano tinha licença de gastar sai de [`Self::permitido_bps`], e
    /// é **zero quando não há transmissão nenhuma** — aí toda janela é
    /// descartada: um servidor sem tela não está enchendo o cano, e o que ele
    /// mede não diz nada sobre ele.
    pub fn observar(
        &mut self,
        agora: Instant,
        quem: u64,
        conexao: LeituraDaConexao,
    ) -> Option<u32> {
        self.por_conexao.insert(quem, conexao);
        let mut soma = LeituraDaSubida {
            permitido_bps: self.permitido_bps(),
            ..LeituraDaSubida::default()
        };
        for leitura in self.por_conexao.values() {
            soma.bytes_enviados = soma.bytes_enviados.saturating_add(leitura.bytes_enviados);
            soma.pacotes_perdidos = soma
                .pacotes_perdidos
                .saturating_add(leitura.pacotes_perdidos);
            soma.eventos_de_congestionamento = soma
                .eventos_de_congestionamento
                .saturating_add(leitura.eventos_de_congestionamento);
        }
        let andou = self.sonda.observar(agora, &soma);
        if andou.is_some() {
            self.mediu = true;
        }
        andou
    }

    /// Uma conexão foi embora.
    ///
    /// Sem isto a soma carregaria para sempre os contadores de quem saiu, e a
    /// janela seguinte veria os bytes dela sumirem de uma vez — um delta
    /// negativo que o `saturating_sub` transforma em zero, e uma janela boa
    /// virando «não entregou nada».
    pub fn esquecer(&mut self, quem: u64) {
        self.por_conexao.remove(&quem);
    }
}

/// Um pedaço de uma transmissão a caminho de um espectador.
///
/// **Não há variante de descarte, e é de propósito.** Um fluxo QUIC é uma
/// sequência ordenada de bytes: descartar um pedaço no meio não atrasa um
/// espectador, desloca o enquadramento dele para sempre — o quadro seguinte
/// leria o meio do anterior como cabeçalho. Onde o áudio descarta
/// (`VoiceRoom::forward`, «old audio helps nobody»), a tela **corta**: quem não
/// acompanha perde a transmissão inteira e sabe disso, o que é uma frase
/// verdadeira, em vez de receber lixo indistinguível de um encoder quebrado.
#[derive(Debug)]
pub enum Pedaco {
    /// Bytes crus do fluxo de quem compartilha, como chegaram.
    Bytes(Vec<u8>),
    /// A transmissão acabou por vontade de quem a mandava.
    Fim,
}

/// O convite a assistir uma transmissão, entregue à sessão de um espectador.
///
/// Um cano novo por transmissão, e não um cano por sessão: é o fechamento dele
/// que diz a [`bombear`] se o fluxo termina ou é cortado, sem uma segunda
/// bandeira que pudesse discordar do canal.
#[derive(Debug)]
pub struct AberturaDeTela {
    /// Qual transmissão.
    pub screen: ScreenId,
    /// O cabeçalho de abertura, **byte por byte como quem compartilha o
    /// escreveu**. O servidor não o reescreve: ele já foi conferido, e o
    /// `ScreenId` dentro dele é o que o próprio servidor atribuiu.
    pub abertura: Vec<u8>,
    /// Por onde o corpo chega.
    pub pedacos: mpsc::Receiver<Pedaco>,
}

/// Por que o servidor encerrou uma transmissão que ninguém mandou parar.
///
/// Enumerado, como `specs/02-protocolo.md` manda em toda razão: quem recebe
/// isto tem de escrever uma frase, e uma string de erro não deixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FimDaTela {
    /// A sala cresceu além do que a subida deste servidor carrega.
    ///
    /// §5.1: a subida do hospedeiro é `N × teto`, e com N grande o suficiente
    /// nem o piso do §2 cabe. Parar é a escalada que o §3.2 escreve — *«quando
    /// o sinal cai de faixa, quem baixa é o vídeo; se continuar caindo, quem
    /// para é o vídeo»* —, e a alternativa seria a sala inteira picotando por
    /// causa da tela, que é o produto quebrado.
    AlemDoQueOHospedeiroCarrega,
    /// O fluxo deixou de ser um fluxo de tela.
    ///
    /// Um tamanho de quadro impossível ou um byte de tipo que não é 0 nem 1.
    /// Encaminhar depois disso seria encaminhar lixo para N pessoas.
    FluxoMalformado,
}

/// Onde o enquadramento de um fluxo de tela está, quadro a quadro.
///
/// Existe por causa de uma pergunta só, e ela é o §5.1 em movimento: **gente
/// entra na sala no meio da transmissão.** Um espectador ligado num byte
/// qualquer leria o meio de um quadro como cabeçalho e nunca mais acertaria o
/// passo. Ligado num começo de **quadro-chave** ele acerta o passo e ainda
/// consegue decodificar, que são as duas coisas de que precisa.
///
/// O que ele **não** faz é remontar quadro. Remontar o quadro-chave para
/// reenviá-lo inteiro desfaria o §3.3, que é a medida mais barata de toda a
/// spec: espalhar o mesmo quadro-chave em quatro tiques leva o p95 da voz de
/// 78,9 para 35,8 ms **com o mesmo bitrate entregue**. O servidor repassa o pedaço
/// que chegou, quando chegou, e só conta os bytes para saber onde está.
#[derive(Debug, Default)]
pub struct Enquadramento {
    /// Quantos bytes faltam do quadro que está passando.
    restam: usize,
    /// O cabeçalho do próximo quadro, enquanto ele chega partido em dois
    /// pedaços.
    cabecalho: Vec<u8>,
}

impl Enquadramento {
    /// Um enquadramento no começo de um fluxo, esperando o primeiro cabeçalho.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Passa um pedaço pelo enquadramento e diz onde alguém pode entrar.
    ///
    /// Devolve o deslocamento, dentro deste pedaço, do primeiro cabeçalho de
    /// **quadro-chave que começa e termina neste mesmo pedaço**. Um cabeçalho
    /// partido entre dois pedaços não vira porta de entrada: os bytes da
    /// primeira metade já saíram, e quem entrasse agora receberia a segunda
    /// metade de um cabeçalho como se fosse a primeira. Perder essa
    /// oportunidade custa um quadro-chave de espera, e quem entra pede um
    /// (`ClientMessage::RequestKeyFrame`, que a onda 1 já atende).
    ///
    /// # Errors
    ///
    /// [`FimDaTela::FluxoMalformado`] para um tamanho de quadro fora de
    /// [`MAX_QUADRO_LEN`], um quadro vazio, ou um byte de tipo que não é 0 nem
    /// 1.
    pub fn entrada(&mut self, bytes: &[u8]) -> Result<Option<usize>, FimDaTela> {
        let mut entrada = None;
        let mut i = 0;
        while i < bytes.len() {
            if self.restam > 0 {
                let anda = self.restam.min(bytes.len().saturating_sub(i));
                self.restam -= anda;
                i += anda;
                continue;
            }
            let comeca_aqui = self.cabecalho.is_empty();
            let inicio = i;
            let falta = CABECALHO_DE_QUADRO_LEN.saturating_sub(self.cabecalho.len());
            let anda = falta.min(bytes.len().saturating_sub(i));
            self.cabecalho
                .extend_from_slice(bytes.get(i..i.saturating_add(anda)).unwrap_or_default());
            i += anda;
            if self.cabecalho.len() < CABECALHO_DE_QUADRO_LEN {
                break;
            }
            let tipo = self.cabecalho.first().copied().unwrap_or(u8::MAX);
            let tamanho = self
                .cabecalho
                .get(1..CABECALHO_DE_QUADRO_LEN)
                .and_then(|quatro| <[u8; 4]>::try_from(quatro).ok())
                .map_or(0, u32::from_be_bytes) as usize;
            self.cabecalho.clear();
            // Os três tipos que este fluxo carrega — imagem comum, imagem-chave
            // e som —, e nada mais. Aceitar um quarto seria aceitar que o fluxo
            // já não é o que se pensa que é, e o resto da leitura seria
            // adivinhação sobre um tamanho que o outro lado escolheu.
            //
            // O `2` entrou quando o som passou a viajar junto com a imagem. Este
            // servidor **não o entende** e não precisa: ele repassa por pedaço,
            // sem remontar quadro. O que ele precisa saber é que o byte é
            // legítimo, para não confundir um fluxo bom com lixo.
            if tipo > TIPO_SOM || tamanho == 0 || tamanho > MAX_QUADRO_LEN {
                return Err(FimDaTela::FluxoMalformado);
            }
            self.restam = tamanho;
            // **A porta de entrada é um quadro-chave de imagem, e só ele.** Um
            // quadro de som não serve a quem chega no meio: ele não começa nada,
            // e entrar por ele entregaria imagem pela metade.
            if tipo == TIPO_CHAVE && comeca_aqui && entrada.is_none() {
                entrada = Some(inicio);
            }
        }
        Ok(entrada)
    }
}

/// Escreve as transmissões de uma sala no fluxo de um espectador.
///
/// Uma tarefa por sessão, dona da conexão de saída daquele espectador. O laço é
/// deliberadamente burro: abre um fluxo por transmissão, escreve o que vem, e
/// **termina ou corta** conforme o cano tenha dito [`Pedaco::Fim`] ou tenha
/// simplesmente fechado. Essas são as duas maneiras de uma transmissão acabar
/// para uma pessoa, e elas são frases diferentes na tela dela.
///
/// O `write_all` é onde a contrapressão mora: um espectador lento faz esta
/// tarefa parar, a fila dele encher, e o [`crate::voice_room::VoiceRoom`] cortá-lo. Nunca
/// faz o servidor esperar e nunca faz os outros esperarem.
pub async fn bombear(conexao: quinn::Connection, mut aberturas: mpsc::Receiver<AberturaDeTela>) {
    while let Some(mut convite) = aberturas.recv().await {
        let Ok(mut fluxo) = conexao.open_uni().await else {
            return;
        };
        let _ = fluxo.set_priority(PRIORIDADE_DA_TELA);
        // O byte de tipo antes do cabeçalho de abertura, e a regra vale nos
        // dois sentidos: quem recebe tem um `accept_uni` só e mais de um uso
        // para ele. Separar por aritmética sobre o conteúdo é o que o §5.2
        // chama de dívida, e o pior erro de protocolo que existe é um fluxo
        // lido como o tipo errado.
        let marca = [seele_proto::stream::StreamType::Screen.byte()];
        if fluxo.write_all(&marca).await.is_err() {
            continue;
        }
        if fluxo.write_all(&convite.abertura).await.is_err() {
            continue;
        }
        let mut limpo = false;
        while let Some(pedaco) = convite.pedacos.recv().await {
            match pedaco {
                Pedaco::Bytes(bytes) => {
                    if fluxo.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Pedaco::Fim => {
                    limpo = true;
                    break;
                }
            }
        }
        if limpo {
            let _ = fluxo.finish();
        } else {
            // `reset` e não `finish`: ver [`CODIGO_DE_CORTE`].
            let _ = fluxo.reset(quinn::VarInt::from_u32(CODIGO_DE_CORTE));
        }
    }
}

#[cfg(test)]
mod tests {
    // ---- a sonda da subida ----

    /// Uma leitura com os contadores acumulados que se quer.
    /// Uma leitura de janela.
    ///
    /// `pacotes_enviados` sai de uma conta e não de um argumento: a perda virou
    /// **fração**, e uma leitura de teste que não dissesse quantos pacotes
    /// saíram faria toda perda parecer 100%. Mil e duzentos bytes por
    /// datagrama, que é o que cabe num caminho comum sem fragmentar.
    fn leitura(bytes: u64, perdidos: u64, permitido_bps: u32) -> LeituraDaSubida {
        LeituraDaSubida {
            bytes_enviados: bytes,
            pacotes_perdidos: perdidos,
            pacotes_enviados: bytes / 1200,
            eventos_de_congestionamento: 0,
            permitido_bps,
        }
    }

    /// Quantos bytes saem em um segundo a uma dada taxa.
    fn bytes_em_um_segundo(bps: u32) -> u64 {
        u64::from(bps) / 8
    }

    #[test]
    fn uma_sala_parada_nao_derruba_a_subida() {
        // **A objeção que o doc de `caminho_no_fio` levantou contra medir**, e a
        // razão de ela não valer: somar o que saiu daria um piso demonstrado, e
        // num servidor parado desabaria abaixo do piso do §2. Não desaba, porque
        // janela que não encheu o teto não move nada.
        let mut sonda = SondaDaSubida::nova();
        let inicio = Instant::now();
        let permitido = 1_000_000;
        let mut bytes = 0;
        for segundo in 1..=60 {
            bytes += bytes_em_um_segundo(20_000);
            sonda.observar(
                inicio + Duration::from_secs(segundo),
                &leitura(bytes, 0, permitido),
            );
        }
        assert_eq!(
            sonda.estimativa(),
            CAMINHO_DO_SERVER_BPS,
            "uma sala parada mexeu na estimativa da subida: o servidor passou a \
             confundir «ninguém mandou nada» com «o cano é estreito»"
        );
    }

    #[test]
    fn encher_o_teto_sem_dor_levanta_a_subida() {
        // O caso que o relato de campo pedia: numa rede que aguenta, a
        // estimativa tem de sair da suposição em vez de ficar presa nela.
        let mut sonda = SondaDaSubida::nova();
        let inicio = Instant::now();
        let mut bytes = 0;
        for segundo in 1..=12 {
            let permitido =
                (u64::from(sonda.estimativa()) * u64::from(FRACAO_DO_CAMINHO) / 100) as u32;
            bytes += bytes_em_um_segundo(permitido);
            sonda.observar(
                inicio + Duration::from_secs(segundo),
                &leitura(bytes, 0, permitido),
            );
        }
        assert!(
            sonda.estimativa() > CAMINHO_DO_SERVER_BPS * 4,
            "doze janelas cheias e sem dor levaram a subida só a {} bps: a \
             estimativa não sai da suposição, e uma LAN continua tratada como \
             internet ruim",
            sonda.estimativa()
        );
    }

    #[test]
    fn a_dor_com_o_teto_cheio_fixa_a_subida_no_que_passou() {
        // Doer enchendo é a única leitura que dá um **número**: o cano acaba
        // onde a janela entregou. Um recuo por fator aqui chutaria.
        let mut sonda = SondaDaSubida::nova();
        let inicio = Instant::now();
        let permitido = 1_000_000;
        let entregue = 900_000;
        // **Perda de 20%**, e não sete pacotes soltos. A regra deixou de ser
        // «perdeu algum» e passou a ser fração — ver `PERDA_QUE_DOI`. Sete
        // pacotes em noventa e três são 7,5%, que hoje é a faixa que segura, e é
        // a mesma leitura que fazia a estimativa oscilar quando ela contava.
        let saidos = bytes_em_um_segundo(entregue) / 1200;
        sonda.observar(inicio, &leitura(0, 0, permitido));
        let novo = sonda.observar(
            inicio + Duration::from_secs(1),
            &leitura(bytes_em_um_segundo(entregue), saidos / 5, permitido),
        );
        assert_eq!(
            novo,
            Some(entregue),
            "doeu com o teto cheio e a subida não virou o que a janela de fato \
             carregou"
        );
    }

    #[test]
    fn a_subida_nunca_cai_abaixo_do_que_o_piso_do_video_exige() {
        // Abaixo daqui quem responde é o `None` de `teto_do_hospedeiro`, que é
        // uma resposta com motivo. Uma estimativa cada vez menor seria o produto
        // desistindo em silêncio.
        let mut sonda = SondaDaSubida::nova();
        let inicio = Instant::now();
        let mut bytes = 0;
        let mut perdidos = 0;
        for segundo in 1..=40 {
            perdidos += 3;
            bytes += bytes_em_um_segundo(10_000);
            sonda.observar(
                inicio + Duration::from_secs(segundo),
                &leitura(bytes, perdidos, 1_000_000),
            );
        }
        assert!(
            sonda.estimativa() >= PISO_DA_SUBIDA_BPS,
            "a subida caiu a {} bps, abaixo do piso de {}",
            sonda.estimativa(),
            PISO_DA_SUBIDA_BPS
        );
        assert!(
            teto_do_hospedeiro(sonda.estimativa(), 1).is_some(),
            "a subida caiu tanto que o teto do hospedeiro virou `None` por \
             aritmética, e não por decisão"
        );
    }

    #[test]
    fn antes_da_janela_fechar_a_sonda_nao_responde() {
        // Ler mais vezes que o necessário é barato; **mover** a estimativa antes
        // da janela fechar mediria rajada de codificador em vez de caminho.
        let mut sonda = SondaDaSubida::nova();
        let inicio = Instant::now();
        sonda.observar(inicio, &leitura(0, 0, 1_000_000));
        for centesimo in 1..=9 {
            assert_eq!(
                sonda.observar(
                    inicio + Duration::from_millis(centesimo * 100),
                    &leitura(bytes_em_um_segundo(900_000), 0, 1_000_000)
                ),
                None,
                "a sonda respondeu antes de a janela de {JANELA_DA_SUBIDA:?} fechar"
            );
        }
    }

    use super::*;

    /// Um quadro como `seele_core::tela` o escreve: tipo, tamanho, corpo.
    fn quadro(chave: bool, tamanho: usize) -> Vec<u8> {
        let mut bytes = vec![u8::from(chave)];
        bytes.extend_from_slice(&(tamanho as u32).to_be_bytes());
        bytes.extend(std::iter::repeat_n(0xAB, tamanho));
        bytes
    }

    #[test]
    fn o_teto_do_hospedeiro_e_dividido_pelos_espectadores() {
        // A primeira linha do min do §5.1, e a razão inteira desta onda: com o
        // servidor encaminhando, a subida que estoura é a dele.
        assert_eq!(teto_do_hospedeiro(2_000_000, 1), Some(1_200_000));
        assert_eq!(teto_do_hospedeiro(2_000_000, 4), Some(300_000));
        // Zero espectador conta como um: quem entrar daqui a um segundo assiste
        // à mesma transmissão.
        assert_eq!(
            teto_do_hospedeiro(2_000_000, 0),
            teto_do_hospedeiro(2_000_000, 1)
        );
    }

    #[test]
    fn a_hipotese_admite_aqui_dentro_e_nao_atravessa_o_fio() {
        // As duas respostas à mesma pergunta, e elas **têm** de discordar. Sem
        // número declarado, a admissão daqui cai na hipótese das provas — que
        // erra para o lado de encerrar cedo — e o fio leva zero, que pelo §5.1
        // quer dizer «não medi» e faz o termo sumir do `min` do outro lado.
        // Mandar a hipótese seria prometer 2000 kbps que ninguém conferiu.
        assert_eq!(caminho_do_server(None), CAMINHO_DO_SERVER_BPS);
        assert_eq!(caminho_no_fio(None, None), 0);
    }

    #[test]
    fn o_que_o_operador_declara_vale_nos_dois_lugares() {
        // Declarado, as duas contas partem do mesmo número — que é a regra 2 do
        // §3.2: nada de um segundo medidor discordando do primeiro.
        assert_eq!(caminho_do_server(Some(50_000_000)), 50_000_000);
        assert_eq!(caminho_no_fio(Some(50_000_000), None), 50_000_000);
    }

    #[test]
    fn zero_declarado_e_um_campo_em_branco_e_nao_um_pedido_para_desligar() {
        // Um caminho de zero bit por segundo pararia toda transmissão deste
        // servidor, e no fio ele já quer dizer «não medi». Ler os dois como
        // ausência é o que impede uma configuração meio preenchida de desligar
        // o recurso em silêncio.
        assert_eq!(caminho_do_server(Some(0)), CAMINHO_DO_SERVER_BPS);
        assert_eq!(caminho_no_fio(Some(0), None), 0);
    }

    #[test]
    fn a_medida_vence_o_que_o_operador_declarou() {
        // Precedência, e ela tem um lado certo: o operador declara de memória e
        // a medida vem do cano. Quando as duas existem, a que foi conferida
        // manda — e é a única forma de um número errado no arquivo de
        // configuração deixar de estrangular a tela para sempre.
        assert_eq!(
            caminho_no_fio(Some(2_000_000), Some(40_000_000)),
            40_000_000
        );
        // E sem medida, o declarado continua valendo: ele é melhor que nada.
        assert_eq!(caminho_no_fio(Some(2_000_000), None), 2_000_000);
    }

    #[test]
    fn a_suposicao_nunca_atravessa_o_fio() {
        // A regra que o doc de `caminho_no_fio` escreve: posta no fio, a
        // hipótese vira uma promessa de banda que ninguém conferiu. Uma sonda
        // recém-criada tem estimativa — a suposição — e **não** tem medida.
        let subida = Subida::nova();
        assert_eq!(subida.estimativa(), CAMINHO_DO_SERVER_BPS);
        assert_eq!(subida.medida(), None, "a suposição vazou para o fio");
        assert_eq!(caminho_no_fio(None, subida.medida()), 0);
    }

    #[test]
    fn abaixo_do_piso_nao_ha_teto_baixo_ha_parada() {
        // §2: «se o encoder não sustenta nem o piso, o compartilhamento para,
        // com motivo enumerado». Um teto de 170 kbps devolvido como número
        // faria o produto prometer uma imagem que não existe.
        assert_eq!(teto_do_hospedeiro(2_000_000, 6), Some(PISO_DE_BANDA_BPS));
        assert_eq!(teto_do_hospedeiro(2_000_000, 7), None);
    }

    #[test]
    fn o_teto_nao_da_a_volta_numa_casa_de_fibra() {
        // `caminho × 60` estoura `u32` a partir de uns 71 Mbit/s. Feito em
        // `u32` isto devolveria um teto minúsculo, e só na casa boa.
        assert_eq!(teto_do_hospedeiro(900_000_000, 1), Some(540_000_000));
    }

    #[test]
    fn a_porta_de_entrada_e_o_comeco_de_um_quadro_chave() {
        let mut enq = Enquadramento::novo();
        let mut fluxo = quadro(false, 10);
        let onde = fluxo.len();
        fluxo.extend(quadro(true, 20));
        fluxo.extend(quadro(false, 5));
        assert_eq!(enq.entrada(&fluxo), Ok(Some(onde)));
    }

    #[test]
    fn sem_quadro_chave_nao_ha_porta() {
        let mut enq = Enquadramento::novo();
        let mut fluxo = quadro(false, 10);
        fluxo.extend(quadro(false, 20));
        assert_eq!(enq.entrada(&fluxo), Ok(None));
    }

    #[test]
    fn um_cabecalho_partido_ao_meio_nao_vira_porta() {
        // O caso que uma divisão exata esconderia: os bytes da primeira metade
        // do cabeçalho já saíram para quem já assistia, e ligar alguém agora o
        // faria ler a segunda metade como se fosse a primeira. A porta é
        // pulada; quem entrou pede um quadro-chave e espera o próximo.
        let mut enq = Enquadramento::novo();
        let chave = quadro(true, 20);
        assert_eq!(enq.entrada(chave.get(..3).unwrap_or_default()), Ok(None));
        assert_eq!(enq.entrada(chave.get(3..).unwrap_or_default()), Ok(None));
        // E o passo continua certo: o quadro seguinte é reconhecido.
        assert_eq!(enq.entrada(&quadro(true, 8)), Ok(Some(0)));
    }

    #[test]
    fn o_enquadramento_atravessa_pedacos_de_qualquer_tamanho() {
        // O tamanho do pedaço é do QUIC, não nosso: o mesmo fluxo tem de dar a
        // mesma resposta byte a byte e de uma vez só.
        let mut fluxo = quadro(false, 300);
        let onde = fluxo.len();
        fluxo.extend(quadro(true, 100));
        let mut inteiro = Enquadramento::novo();
        assert_eq!(inteiro.entrada(&fluxo), Ok(Some(onde)));

        let mut picado = Enquadramento::novo();
        let mut achado = None;
        for (i, byte) in fluxo.iter().enumerate() {
            if let Ok(Some(_)) = picado.entrada(&[*byte]) {
                achado = Some(i);
            }
        }
        // Um cabeçalho que chega byte a byte nunca começa e termina no mesmo
        // pedaço, então não há porta — e é exatamente o que a regra diz.
        assert_eq!(achado, None);
    }

    #[test]
    fn um_tamanho_impossivel_encerra_o_fluxo() {
        let mut enq = Enquadramento::novo();
        let mut cabecalho = vec![0_u8];
        cabecalho.extend_from_slice(&(MAX_QUADRO_LEN as u32 + 1).to_be_bytes());
        assert_eq!(enq.entrada(&cabecalho), Err(FimDaTela::FluxoMalformado));
    }

    #[test]
    fn um_quadro_vazio_encerra_o_fluxo() {
        let mut enq = Enquadramento::novo();
        assert_eq!(
            enq.entrada(&[0, 0, 0, 0, 0]),
            Err(FimDaTela::FluxoMalformado)
        );
    }

    #[test]
    fn um_byte_de_tipo_que_este_fluxo_nao_conhece_o_encerra() {
        // O `2` **deixou de ser desconhecido**: é o som, que passou a viajar no
        // mesmo fluxo da imagem. Este teste subiu para o primeiro byte que
        // continua não sendo nada, e é o que ele sempre guardou — que um fluxo
        // que já não é o que se pensa não é lido por adivinhação sobre um
        // tamanho que o outro lado escolheu.
        let mut enq = Enquadramento::novo();
        assert_eq!(
            enq.entrada(&[3, 0, 0, 0, 8]),
            Err(FimDaTela::FluxoMalformado)
        );
    }

    #[test]
    fn um_quadro_de_som_atravessa_e_nao_e_porta_de_entrada() {
        // O servidor **não entende** o som: ele repassa por pedaço, sem
        // remontar quadro. O que ele precisa saber é que o byte é legítimo, para
        // não confundir um fluxo bom com lixo.
        let mut enq = Enquadramento::novo();
        let som = {
            let mut fluxo = vec![TIPO_SOM, 0, 0, 0, 4];
            fluxo.extend_from_slice(&[1, 2, 3, 4]);
            fluxo
        };
        assert_eq!(
            enq.entrada(&som),
            Ok(None),
            "um quadro de som foi tratado como fluxo malformado"
        );

        // E ele não abre a porta para quem chega no meio: quem entra precisa de
        // uma imagem que baste a si mesma, e som não começa imagem nenhuma.
        let mut enq = Enquadramento::novo();
        let mut fluxo = vec![TIPO_SOM, 0, 0, 0, 4];
        fluxo.extend_from_slice(&[1, 2, 3, 4]);
        let onde = fluxo.len();
        fluxo.extend(quadro(true, 20));
        assert_eq!(
            enq.entrada(&fluxo),
            Ok(Some(onde)),
            "a porta de entrada não é o quadro-chave de imagem"
        );
    }
}

#[cfg(test)]
mod a_sonda_em_lan {
    use super::{
        LeituraDaConexao, Subida, CAMINHO_DO_SERVER_BPS, JANELA_DA_SUBIDA, TETO_DA_SUBIDA_BPS,
    };
    use std::time::Instant;

    /// Numa LAN a sonda tem de **subir**, e a pergunta é se ela sobe.
    ///
    /// O sintoma relatado em campo: áudio perfeito e vídeo muito pixelado entre
    /// duas máquinas na mesma rede. A suspeita é que a perna do anfitrião fica
    /// travada na suposição de 2 Mbps — que dá teto de 1,2 Mbps, e 1,2 Mbps para
    /// jogo em 720p é exatamente «pixelado».
    ///
    /// Este teste encena o caso: uma pessoa assistindo, e janelas que entregam
    /// o que a estimativa permitiu, sem perda e sem congestionamento. É a LAN.
    #[test]
    fn com_uma_pessoa_assistindo_e_sem_dor_a_estimativa_sobe() {
        let inicio = Instant::now();
        let mut subida = Subida::nova();
        subida.assistindo(1);

        assert_eq!(
            subida.estimativa(),
            CAMINHO_DO_SERVER_BPS,
            "a sonda não parte da suposição"
        );

        let mut bytes = 0_u64;
        // Vinte segundos de transmissão saudável.
        for volta in 1..=20_u32 {
            // O que a estimativa de agora permite, entregue inteiro: é o que a
            // LAN faz. Oito bits por byte, uma janela de um segundo.
            let permitido = subida.estimativa() / 100 * super::FRACAO_DO_CAMINHO;
            bytes += u64::from(permitido) / 8;
            subida.observar(
                inicio + JANELA_DA_SUBIDA * volta,
                1,
                LeituraDaConexao {
                    bytes_enviados: bytes,
                    pacotes_perdidos: 0,
                    pacotes_enviados: bytes / 1200,
                    eventos_de_congestionamento: 0,
                },
            );
        }

        assert!(
            subida.estimativa() > CAMINHO_DO_SERVER_BPS * 4,
            "em vinte segundos de LAN sem dor a estimativa saiu de {} e chegou só a {} — \
             a perna do anfitrião está travada na suposição, e é ela que trava o vídeo",
            CAMINHO_DO_SERVER_BPS,
            subida.estimativa()
        );
        assert!(subida.estimativa() <= TETO_DA_SUBIDA_BPS);
        assert!(
            subida.medida().is_some(),
            "a estimativa andou e a medida continuou sendo `None`, então nada disso \
             atravessa o fio e o cliente segue com a suposição"
        );
    }
}
