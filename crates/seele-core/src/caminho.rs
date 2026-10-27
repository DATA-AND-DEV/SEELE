//! Quanto o caminho de subida de **quem compartilha** aguenta, medido.
//!
//! Esta é a resposta à pergunta 2 do §8 de
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` —
//! *«como se mede o caminho quando ninguém está enchendo?»*. A resposta é curta:
//! **enquanto a tela transmite, alguém está enchendo, e é a tela.** O
//! `quinn-proto` já conta cada byte que sai pelo soquete e cada vez que o
//! controle de congestionamento reagiu; a medida sai de dentro do processo, de
//! graça, sem sondagem nenhuma na rede e sem crate novo.
//!
//! Até aqui, [`crate::tela::CAMINHO_DA_PROVA_BPS`] era a resposta: 2000 kbps
//! **supostos**, o cano sobre o qual os dois spikes rodaram. Ele continua sendo
//! por onde a sonda começa, e deixou de ser onde ela para.
//!
//! # Por que um módulo, e não mais um pedaço de `tela.rs`
//!
//! `crate::tela` escreve, no doc de [`TetoDeVideo`], que *«nada nesta estrutura
//! lê relógio nem guarda histórico: ela é uma conta sobre o que se sabe agora»*
//! — e é essa frase que faz `a_voz_nunca_cede_a_tela` ser provável por
//! aritmética, sem rede e sem tempo. A sonda é exatamente o contrário: ela é
//! histórico, e sem histórico não há medida. Juntá-las apagaria a linha que
//! torna uma das duas demonstrável.
//!
//! O que a sonda **mantém** do estilo de lá é a segunda metade da mesma frase: o
//! tempo entra por parâmetro. [`Sonda::observar`] recebe o [`Instant`] e a
//! amostra e devolve a estimativa; não lê relógio, não abre soquete, não fala
//! com o `quinn` — quem faz isso é `crate::enlace`. É o que deixa a coisa toda
//! ser conferida em janelas simuladas, minutos por milissegundo, sem uma
//! conexão de verdade.
//!
//! # As duas armadilhas, que matariam isto em silêncio
//!
//! 1. **Tela parada quase não gasta bits.** Uma janela em que a captura não
//!    tinha o que mandar entrega 40 ou 80 kbps, e lê-la como *«o cano tem
//!    80 kbps»* trancaria todo mundo em 540p permanentemente — um defeito que
//!    não aparece em teste nenhum de laboratório, porque em laboratório a tela
//!    está sempre mexendo. Por isso uma janela só diz **quanto o cano aguenta**
//!    quando a transmissão de fato encheu o teto que tinha
//!    ([`OCUPACAO_MINIMA`]); nas outras, o número é descartado em vez de usado
//!    — a janela ainda pode mandar recuar, se doeu, mas nunca dizer um tamanho.
//! 2. **A faixa da voz não pode derrubar a estimativa.**
//!    [`TetoDeVideo::teto`] **já** corta o teto pela metade em
//!    [`SignalBand::Degraded`] e **já** para em [`SignalBand::Critical`]. Se a sonda
//!    também baixasse por causa da faixa, o vídeo levaria dois cortes pelo mesmo
//!    sintoma e voltaria devagar de um buraco que ele mesmo cavou. São dois
//!    sinais independentes **de propósito**: a faixa da voz é a **dor**, o
//!    `PathStats` é a **capacidade**. A faixa entra aqui por um motivo só, e é
//!    outro — saber se o teto daquela janela era o **nosso** ou o de outra perna
//!    do §5.1; ver [`Sonda::observar`].
//!
//! # A terceira armadilha, que não estava no enunciado e é a pior
//!
//! O jeito óbvio de medir é subir até doer e chamar de caminho o valor em que
//! doeu. **Isso destrói a reserva da voz**, e vale escrever a conta porque ela
//! não é evidente.
//!
//! O teto do vídeo é [`FRACAO_DO_CAMINHO`] — 60% — da estimativa. Quem sobe até
//! doer sobe até o **teto** encostar no cano, ou seja, até o vídeo sozinho
//! ocupar 100% da subida. Nesse ponto a estimativa vale 1/0,6 ≈ **1,67 vezes o
//! caminho de verdade**, e 60% de um número 67% grande demais são 100% do cano:
//! os 40% que `spikes/tela-no-transporte` reservou para a voz — os mesmos 40%
//! que separam 23,1 ms de p50 de 225,7 ms — evaporam, e o produto volta a ser
//! exatamente o que o §3.2 mediu e recusou.
//!
//! Então a sonda **não** guarda o valor em que doeu. Ela guarda o que a janela
//! **carregou** quando doeu, que é `udp_tx.bytes` daquela janela — e isso é o
//! caminho, não 1,67 vezes ele, porque no momento da dor o que saiu pelo soquete
//! é tudo o que o cano deixou sair. Daí em diante o teto volta a ser 60% de um
//! número honesto. O teste `a_reserva_da_voz_sobrevive_a_sonda` é essa conta
//! escrita como asserção, e é o irmão de `a_voz_nunca_cede_a_tela`.
//!
//! # Por que sobe por passo largo e desce para a medida
//!
//! Subir é aposta e descer é notícia, e as duas não merecem o mesmo tipo de
//! número. Subir multiplicativamente ([`SUBIDA`]) é o que faz «começar alto»
//! acontecer sem o custo de começar alto: da suposição de 2 Mbps até 6 Mbps são
//! cinco janelas, cinco segundos. Descer usa a medida, porque no instante da dor
//! existe uma.
//!
//! # A histerese, e por que não é uma margem em volta do degrau
//!
//! Trocar de degrau de resolução custa um quadro-chave inteiro — 65 KiB e 446 ms
//! do orçamento de 1200 kbps —, e `crate::video::Compartilhamento::refazer_com`
//! já avisa que *«um degrau que oscila entre dois valores queimaria o orçamento
//! em quadros-chave e não mostraria tela nenhuma»*. Uma estimativa que balança
//! em volta de um limiar de [`crate::tela::resolucao_estimada_para`] faz
//! exatamente isso.
//!
//! A tentação é pôr uma margem em volta do limiar: só sobe de degrau quem passar
//! dele por X%. **Não funciona aqui**, e o motivo é a mesma aritmética de cima:
//! o teto é 60% da estimativa, então a única maneira de descobrir se o degrau de
//! cima cabe é **deixar o teto subir até lá**. Uma margem que segurasse a
//! estimativa abaixo do limiar seguraria junto o teto, e o degrau de cima nunca
//! seria testado — a escada subiria uma vez, na primeira sessão, e nunca mais.
//!
//! O que funciona é a estimativa **parar de balançar**, e é o que [`Sonda`] faz:
//! depois da primeira dor ela fica onde a medida a pôs e **não volta a subir**,
//! porque o passo largo passa a ser limitado pelo que já se sabe
//! ([`Sonda::limite_bps`]). Medido em três minutos de um cano parado exatamente
//! em cima do limiar de 720p: oito trocas de degrau contra 150 janelas, e 92 sem
//! o limite.
//!
//! # A sondagem, que é uma pergunta e não uma escada
//!
//! Ficar na primeira medida para sempre seria tratar a rede de agora como a rede
//! de sempre: quem estava baixando um arquivo termina, e o cano de meio minuto
//! atrás não é o de agora. Então, a cada [`ESQUECIMENTO`] de calmaria, a sonda
//! pergunta — e pergunta de uma vez só, pondo o teto exatamente onde a última
//! medida disse que o cano acaba. A janela seguinte responde: ou dói, e o cano é
//! o mesmo, e a conta da pergunta foi **uma** janela de fila; ou não dói, e o
//! cano cresceu, e a subida livre recomeça para medir a borda nova.
//!
//! Perguntar aos poucos custaria o mesmo em fila e cobraria por muito mais
//! tempo — a estimativa passaria um terço da conversa perto da borda, comendo a
//! reserva da voz de raspão em vez de tocá-la uma vez. E, porque cada pergunta
//! custa uma janela, a espera **dobra** a cada resposta negativa até
//! [`TETO_DA_ESPERA`]: numa rede que não muda a sondagem some sozinha, e numa
//! rede que mudou a resposta positiva devolve a espera ao começo.
//!
//! # O que esta sonda não faz
//!
//! **Não atravessa sessões.** A estimativa morre com o processo, e guardá-la em
//! disco é decisão de formato em disco que ninguém tomou. **Não mede o caminho
//! de quem hospeda** — aquele chega pelo fio, no `HostUplink`, e é a outra perna
//! do §5.1. E **não mede nada enquanto ninguém compartilha**: sem transmissão
//! não há quem encha o cano, e a pergunta 2 do §8 volta a não ter resposta. Aí a
//! estimativa é a última que se mediu, ou a suposição de
//! [`CAMINHO_DA_PROVA_BPS`] enquanto não houve nenhuma.
//!
//! [`TetoDeVideo`]: crate::tela::TetoDeVideo
//! [`TetoDeVideo::teto`]: crate::tela::TetoDeVideo::teto
//! [`FRACAO_DO_CAMINHO`]: crate::tela::FRACAO_DO_CAMINHO
//! [`CAMINHO_DA_PROVA_BPS`]: crate::tela::CAMINHO_DA_PROVA_BPS

use std::time::{Duration, Instant};

use seele_proto::signal::SignalBand;

use crate::tela::{Teto, CAMINHO_DA_PROVA_BPS, FRACAO_DO_CAMINHO, PISO_DE_BANDA_BPS};

// ---------------------------------------------------------------------------
// Os números escolhidos, e o que em cada um é escolha
// ---------------------------------------------------------------------------

/// Quanto tempo uma janela de amostragem dura.
///
/// **Um segundo, e é escolha e não medida** — mas não é um número redondo à toa.
/// É a unidade em que o teto é dito e em que o balde de
/// `crate::tela::Transmissao` se enche: a capacidade dele é *um segundo de
/// orçamento*. Uma janela mais curta mediria um balde que ainda não repôs e
/// leria a rajada de um quadro-chave como se fosse o cano. É também o intervalo
/// em que o servidor manda `PersonState`, então é o passo em que o resto da malha
/// já anda.
pub const JANELA: Duration = Duration::from_secs(1);

/// Quanto da janela a transmissão tem de ter enchido para a amostra valer, em
/// por cento do teto.
///
/// **85, e é escolha** — com a única medida disponível atrás dela. O teto não é
/// preenchido a 100% nem quando tudo vai bem: `spikes/tela-no-codec` mediu o
/// controle de taxa do OpenH264 jogando fora 11,1% dos quadros em 720p e 16,2%
/// em 1080p **dentro** do teto de 1200 kbps, e o que sai do outro lado são 872 e
/// 1146 kbps de 1200. Exigir mais que 85% descartaria toda janela de uma
/// transmissão que está funcionando; exigir muito menos aceitaria como medida
/// uma tela parada, que é a primeira armadilha do cabeçalho deste módulo.
pub const OCUPACAO_MINIMA: u32 = 85;

/// De quanto a estimativa sobe por janela cheia e sem piora, em por cento.
///
/// **125, e é escolha.** O que a fixa é o tempo até o produto parecer certo: da
/// suposição de [`CAMINHO_DA_PROVA_BPS`] — 2 Mbps — até 6 Mbps são cinco
/// janelas, cinco segundos. Um passo menor faria alguém de fibra passar o
/// primeiro minuto em 540p sem motivo; um passo maior chegaria à dor num salto
/// grande demais para a janela seguinte descrever.
pub const SUBIDA: u32 = 125;

/// De quanto ela sobe quando o caminho é **curto**, em por cento.
///
/// **O passo passa a depender de um número medido, e não de uma suposição.**
///
/// O que torna um passo grande arriscado é o tempo até saber que ele errou:
/// numa janela de um segundo sobre um caminho de cem milissegundos, a dor chega
/// tarde e já contaminou muito. Sobre um caminho de dois milissegundos, o
/// retorno é quase imediato — errar para cima custa uma janela e volta.
///
/// E o freio de evidência continua: a subida nunca passa do que a janela de fato
/// carregou, escalado. Um passo maior encurta o caminho até a medida, não
/// substitui a medida.
///
/// Medido em campo, numa LAN: da suposição de 2 Mbps até 6 Mbps de teto foram
/// **dezessete segundos** — e são justamente os primeiros segundos que alguém
/// olha. O relato foi «momentos de muito movimento pixeliza demais», e a imagem
/// feia era a sonda ainda engatinhando. Dobrando, os mesmos 6 Mbps chegam em
/// torno de quatro segundos.
pub const SUBIDA_DE_CAMINHO_CURTO: u32 = 200;

/// Até que ida e volta um caminho conta como curto.
///
/// **Cinco milissegundos**, e a escolha é conservadora de propósito: uma LAN fica
/// abaixo de dois, e mesmo um servidor na mesma cidade por fibra costuma passar
/// de cinco. Quem estiver na faixa duvidosa continua com o passo antigo, que é o
/// que já funcionava.
pub const IDA_E_VOLTA_CURTA: Duration = Duration::from_millis(5);

/// De quanto a estimativa desce quando doeu e a janela não sabe dizer o tamanho
/// do cano, em por cento.
///
/// **80, e é escolha.** É o desconto para os dois casos em que a janela viu dor
/// e não tem como dizer quanto o cano aguenta: a tela não estava enchendo o
/// teto, ou quem estava apertando era outra perna do §5.1 — e aí o que saiu pelo
/// soquete descreve **aquela** perna. Sem medida não há para onde saltar, e o
/// que resta é recuar um passo e olhar de novo na janela seguinte.
///
/// Quando a janela **é** nossa e está cheia este número não é usado: ali existe
/// medida, e a medida é melhor que qualquer fator — ver [`Sonda::observar`].
pub const QUEDA: u32 = 80;

/// Quanto tempo de calmaria antes da **primeira** sondagem.
///
/// **Meio minuto, e é escolha.** Depois da primeira dor a estimativa fica onde a
/// medida a pôs, e ficar lá para sempre seria tratar a rede de agora como a rede
/// de sempre: quem estava baixando um arquivo termina, o vizinho desliga a TV, e
/// o cano de meio minuto atrás não é o de agora. Trinta segundos são trinta
/// `PersonState` — tempo de a faixa da voz ter dito, e redito, que está tudo bem
/// antes de a tela pedir mais.
///
/// **Uma sondagem custa uma janela de fila**, e é por isso que a espera dobra a
/// cada sondagem que doeu, até [`TETO_DA_ESPERA`]: numa rede que não muda, a
/// sondagem some sozinha em vez de cobrar um segundo ruim por meio minuto de
/// conversa. Numa rede que mudou de verdade, a sondagem que dá certo devolve a
/// espera ao começo, e a próxima vem depressa. Ver [`Sonda::observar`].
pub const ESQUECIMENTO: Duration = Duration::from_secs(30);

/// Até onde a espera entre duas sondagens dobra.
///
/// **Oito minutos, e é escolha:** quatro dobras a partir de [`ESQUECIMENTO`]. A
/// quinta passaria da duração de muitas conversas, e aí a sondagem deixaria de
/// existir em vez de ficar rara — que é o que se quer.
pub const TETO_DA_ESPERA: Duration = Duration::from_secs(480);

/// Quanta fila no gargalo já conta como piora, acima da menor ida e volta vista.
///
/// **40 ms, e é escolha.** Perda e evento de congestionamento chegam **tarde**:
/// o `quinn` só os conta quando a fila do gargalo transbordou, e a fila que
/// `spikes/tela-no-transporte` mediu tem 262 ms — quer dizer, quando a perda
/// aparece a voz já está há um quarto de segundo atrasada, que é precisamente o
/// estrago que este ciclo existe para não repetir. O tempo de ida e volta subir
/// é o mesmo aviso, um quarto de segundo antes.
///
/// 40 ms é quase o dobro dos 23,1 ms de p50 que o spike viu com a fila vazia, e
/// é o menor número que não confunde o zigue-zague de um Wi-Fi comum com fila de
/// verdade. **Não foi medido**, e é o candidato número um a mudar quando alguém
/// rodar isto em Wi-Fi ruim — que é a pergunta 1 do §8, ainda aberta.
pub const FILA_TOLERADA: Duration = Duration::from_millis(40);

/// O maior caminho que a sonda chega a afirmar, em bits por segundo.
///
/// **20,8 Mbps, e é o degrau mais alto da escada do §5 pagando o próprio
/// preço.** Não é um número escolhido: é
/// `bits_por_quadro(1080p) × 60 quadros ÷ FRACAO_DO_CAMINHO`, ou seja o que o
/// caminho precisa carregar para que 1080p a 60 quadros receba os 0,10 bits por
/// pixel que fazem a borda de uma fonte não virar bloco.
///
/// # Por que derivado, e o que os dois números anteriores erraram
///
/// Ele valeu **10 Mbps**, justificados como «quatro vezes os 2,5 Mbps de
/// caminho que compram 1080p». O erro estava do outro lado da conta:
/// `TETO_ESTIMADO_PARA_1080P_BPS` valia 1500 kbps, que é onde 1080p passa a ser
/// **comprável** — 0,024 bits por pixel — e não onde ele fica bom.
///
/// Depois valeu **14 Mbps**, e esse errou de outro jeito: era o pedido de
/// produto de 8 Mbps de vídeo dividido por 60%. Oito megabits compram 1080p a
/// **30** quadros com folga, ou 720p a 60 — e não compram 1080p60, que a 8 Mbps
/// recebe 133 kbit por quadro, 0,064 bpp. Pior: com aquele teto,
/// `cadencia_para` nem oferecia 60 quadros em 1080p — `8_400_000 / 208_000 = 40`
/// compra Q30 —, então **o degrau de cima da lista fechada do §5 era
/// inalcançável por construção**, em qualquer rede, inclusive numa LAN.
///
/// Um teto escrito à mão erra sempre que a escada muda. Derivado dela, não pode:
/// no dia em que a lista do §5 ganhar um degrau, este número acompanha.
///
/// Continua havendo um teto, e pela razão de sempre: acima de 1080p a 60
/// quadros não há degrau que a lista fechada do §5 ofereça, então uma
/// estimativa maior só produziria saltos que nada compra.
pub const TETO_DA_ESTIMATIVA_BPS: u32 =
    crate::tela::bits_por_quadro(seele_video::codec::Resolucao::P1080) * 60 * 100
        / FRACAO_DO_CAMINHO;

/// O menor caminho que a sonda chega a afirmar, em bits por segundo.
///
/// É o ponto exato em que [`FRACAO_DO_CAMINHO`] ainda dá [`PISO_DE_BANDA_BPS`],
/// e por isso é uma conta e não um literal: se um dos dois mudar, este anda
/// junto. Abaixo daqui a sonda **não** desce, e a razão é que parar não é
/// trabalho dela — quem para é `crate::tela::TetoDeVideo::teto`, com
/// [`crate::tela::MotivoDeParada`] enumerado, que é a única forma de parada que
/// o §2 aceita. Uma sonda que zerasse a estimativa pararia o vídeo pela porta
/// dos fundos, sem frase para quem está olhando.
pub const PISO_DA_ESTIMATIVA_BPS: u32 =
    ((PISO_DE_BANDA_BPS as u64 * 100).div_ceil(FRACAO_DO_CAMINHO as u64)) as u32;

// A conta acima tem de fechar: 60% do piso da estimativa não pode cair abaixo do
// piso de banda por causa de um arredondamento. Conferido em tempo de
// compilação porque um erro de uma unidade aqui vira um compartilhamento que
// para sozinho, e o teste que o pegaria é justamente o que ninguém escreve.
const _: () = assert!(
    (PISO_DA_ESTIMATIVA_BPS as u64 * FRACAO_DO_CAMINHO as u64) / 100 >= PISO_DE_BANDA_BPS as u64
);

// ---------------------------------------------------------------------------
// O que entra
// ---------------------------------------------------------------------------

/// O que se sabe do caminho, acumulado desde o começo da conexão — e **o que não
/// se sabe**, dito como ausência e nunca como zero.
///
/// # Por que cada medida é um `Option`
///
/// Até a 0.16 os quatro números vinham do `quinn`, e os quatro existiam sempre.
/// Desde a 0.17 o transporte é o enlace WebRTC, cujo SCTP não conta perda nem
/// reação de congestionamento. Um zero ali seria lido pela [`Sonda`] como
/// «nada doeu» — a ausência de dor confundida com a ausência de medida —, e a
/// estimativa subiria sem evidência, em cima da reserva da voz. Por isso o que o
/// transporte não mede é `None`, e a sonda trata `None` como não sabido
/// ([`Sonda::observar`]).
///
/// Os acumulados são subtraídos pela [`Sonda`], porque só ela sabe onde a janela
/// começou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Transporte {
    /// Tudo o que saiu pelo soquete desta conexão (`Leitura::bytes_enviados`).
    ///
    /// **Tudo**, e não só a tela: voz, controle, retransmissão e a tela. É o que
    /// se quer — o caminho carrega tudo isso junto.
    pub bytes_enviados: u64,
    /// Quantas vezes o controle de congestionamento reagiu, quando o transporte
    /// conta. `None` no enlace da 0.17.
    pub eventos_de_congestionamento: Option<u64>,
    /// Quantos pacotes o caminho comeu, quando o transporte conta. `None` no
    /// enlace da 0.17.
    pub pacotes_perdidos: Option<u64>,
    /// A ida e volta medida, quando há medida. Ver [`IdaEVolta`].
    pub ida_e_volta: Option<IdaEVolta>,
}

/// Uma medida de ida e volta, com o que diz se ela é desta janela.
///
/// Desde a 0.17 ela vem do **`Ping` da sonda** (`Client::pingar_para_a_sonda`),
/// e não do transporte: um `Ping` no `ctl` a cada meio segundo enquanto a tela
/// transmite, respondido pelo servidor na mesma volta em que o lê. O `ctl` e a
/// voz saem na frente dos canais de volume no escalonador do enlace, então o que
/// atrasa o `Pong` é a fila do caminho — o soquete, o gargalo, a volta —, que é
/// exatamente o atraso que a voz sofre. É a medida de dor que sobrou, e é de
/// aplicação: o QUIC não foi recriado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdaEVolta {
    /// A última ida e volta respondida.
    pub valor: Duration,
    /// Quantas foram respondidas até agora. A janela só conta como medida se
    /// este número andou dentro dela: uma medida velha não é evidência de agora.
    pub amostras: u64,
    /// Há quanto tempo o `Ping` que ainda não voltou saiu, se há um.
    ///
    /// É um **piso** da ida e volta de agora: um `Pong` atrasado por uma fila de
    /// segundos não chega dentro da janela, e sem isto a dor maior seria a que
    /// nunca aparece.
    pub em_voo: Option<Duration>,
}

/// A estimativa mais alta que uma janela **sem medida de dor** sustenta, em
/// bits por segundo.
///
/// **É [`CAMINHO_DA_PROVA_BPS`], e é decisão (revisão do Codex na 0.17).** Sem
/// perda, sem congestionamento e sem ida e volta fresca, a sonda não tem como
/// saber se a tela está comendo a reserva da voz, e então não sobe e volta, se
/// estava acima, ao cano das provas: o caminho sobre o qual
/// `spikes/tela-no-transporte` mediu a reserva de 40% da voz, e a suposição com
/// que o produto transmitia antes de a sonda existir. O piso seria ainda mais
/// seguro e pararia a tela por falta de telemetria; o teto da estimativa seria
/// a esperança que este módulo existe para não ter.
pub const TETO_SEM_MEDIDA_BPS: u32 = CAMINHO_DA_PROVA_BPS;

// ---------------------------------------------------------------------------
// A lembrança
// ---------------------------------------------------------------------------

/// A semente mais alta que uma lembrança dá, em bits por segundo: **8 Mbps**,
/// quatro vezes o cano das provas.
///
/// **Decisão da 0.17 (coordenador).** Uma estimativa gravada é de outro dia, e
/// talvez de outra rede: ela encurta a escada, e não fala pelo caminho de
/// hoje. Acima disto a sonda sobe **por evidência** desta sessão — numa LAN, a
/// ida e volta curta já troca o passo para [`SUBIDA_DE_CAMINHO_CURTO`]. Os
/// 12,48 Mbps medidos em campo começam em 8 e reencontram o topo em poucas
/// janelas calmas, em vez de começar onde ontem estava.
pub const TETO_DA_SEMENTE_BPS: u32 = 4 * CAMINHO_DA_PROVA_BPS;

/// Por quanto tempo uma lembrança vale: **sete dias**, em segundos.
///
/// Uma semana cobre quem volta ao mesmo servidor nos mesmos dias, e não
/// atravessa o mês em que o plano de internet, o roteador ou a casa mudaram.
pub const IDADE_MAXIMA_DA_LEMBRANCA_S: i64 = 7 * 24 * 60 * 60;

/// Quanto de relógio adiantado se tolera numa lembrança, em segundos. Uma data
/// mais no futuro que isso é relógio errado, e lembrança de relógio errado não
/// tem idade que se confira.
const FOLGA_DO_RELOGIO_S: i64 = 5 * 60;

/// A linha de transporte com que as medidas desta versão são tiradas.
///
/// Entra no [`ambiente_desta_maquina`]: uma medida tirada sobre o QUIC da 0.16
/// não descreve o enlace da 0.17, que tem outro controle de congestionamento e
/// outra telemetria.
const LINHA_DA_MEDIDA: &str = "enlace-0.17";

/// Uma medida do caminho gravada em disco, com o que diz **quando** e **onde**.
///
/// `crate::conhecidos` (por servidor) e `crate::preferences` (da máquina) a
/// guardam. Sem data ou sem ambiente — o número solto que a 0.16 gravava —
/// não há lembrança: [`Self::ler`] devolve `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaminhoLembrado {
    /// O que a sonda mediu.
    pub bps: u32,
    /// Quando, em segundos desde a época.
    pub medido_em: i64,
    /// Onde: ver [`ambiente_desta_maquina`].
    pub ambiente: String,
}

impl CaminhoLembrado {
    /// A semente que esta lembrança dá **agora, neste ambiente**, ou `None`.
    ///
    /// `None` quando a rede é outra, quando passou de
    /// [`IDADE_MAXIMA_DA_LEMBRANCA_S`] ou quando a data está no futuro. Quando
    /// vale, é grampeada em [`TETO_DA_SEMENTE_BPS`].
    #[must_use]
    pub fn semente(&self, agora: i64, ambiente: &str) -> Option<u32> {
        if self.ambiente != ambiente
            || self.medido_em > agora.saturating_add(FOLGA_DO_RELOGIO_S)
            || agora.saturating_sub(self.medido_em) > IDADE_MAXIMA_DA_LEMBRANCA_S
            || self.bps == 0
        {
            return None;
        }
        Some(self.bps.min(TETO_DA_SEMENTE_BPS))
    }

    /// Como vai ao disco: `bps@medido_em@ambiente`, numa coluna só.
    #[must_use]
    pub fn escrever(&self) -> String {
        format!("{}@{}@{}", self.bps, self.medido_em, self.ambiente)
    }

    /// O inverso de [`Self::escrever`]. Um número solto — o que a 0.16
    /// gravava — não é lembrança: não diz quando nem onde.
    #[must_use]
    pub fn ler(texto: &str) -> Option<Self> {
        let mut partes = texto.trim().splitn(3, '@');
        let bps = partes.next()?.parse().ok()?;
        let medido_em = partes.next()?.parse().ok()?;
        let ambiente = partes.next()?.to_owned();
        if ambiente.is_empty() || ambiente.contains(['\t', '\n', '\r']) {
            return None;
        }
        Some(Self {
            bps,
            medido_em,
            ambiente,
        })
    }
}

/// Em que rede esta máquina está agora, como a lembrança o confere.
///
/// A linha de transporte ([`LINHA_DA_MEDIDA`]) e o endereço local pelo qual o
/// sistema sairia para a internet, em IPv4 e IPv6. Esse endereço é o que o
/// sistema escolhe pela rota padrão, e muda quando muda a rede: Wi-Fi de casa,
/// do trabalho, o celular roteando. Conferido sem mandar pacote nenhum — um
/// `connect` UDP só escolhe a rota.
///
/// **O que ele não vê**, e por isso há também a idade e o teto: duas redes
/// domésticas que dão o mesmo `192.168.0.10`. A idade e
/// [`TETO_DA_SEMENTE_BPS`] limitam o que uma confusão dessas custa: no máximo
/// uma semente de 8 Mbps que a primeira janela sem medida já desfaz.
#[must_use]
pub fn ambiente_desta_maquina() -> String {
    let saida = |local: &str, longe: &str| {
        std::net::UdpSocket::bind(local)
            .and_then(|socket| {
                socket.connect(longe)?;
                socket.local_addr()
            })
            .map_or_else(|_| "-".to_owned(), |endereco| endereco.ip().to_string())
    };
    // Endereços de documentação (RFC 5737 e RFC 3849): nunca respondem, e
    // nenhum pacote sai — o `connect` UDP só consulta a tabela de rotas.
    format!(
        "{LINHA_DA_MEDIDA}|{}|{}",
        saida("0.0.0.0:0", "192.0.2.1:9"),
        saida("[::]:0", "[2001:db8::1]:9")
    )
}

/// Uma leitura: o que o transporte contou, sob que teto, em que faixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amostra {
    /// Os contadores acumulados do transporte neste instante.
    pub transporte: Transporte,
    /// O teto que está valendo — o resultado do `min` das três pernas do §5.1.
    pub teto: Teto,
    /// A faixa em que o sinal da voz está.
    ///
    /// **Não serve para baixar a estimativa**, e a segunda armadilha do
    /// cabeçalho deste módulo diz por quê. Serve para uma coisa só: saber quanto
    /// da nossa perna o teto acima representa, já que [`SignalBand::Degraded`]
    /// corta as duas pernas de rede pela metade.
    pub faixa: SignalBand,
}

// ---------------------------------------------------------------------------
// A sonda
// ---------------------------------------------------------------------------

/// Onde uma janela de amostragem começou.
#[derive(Debug, Clone, Copy)]
struct Janela {
    abriu: Instant,
    transporte: Transporte,
    teto_bps: u32,
    faixa: SignalBand,
}

/// A profundidade do caminho de subida desta máquina, medida enquanto a tela
/// enche.
///
/// Pura no sentido que interessa: recebe o instante e a amostra, devolve a
/// estimativa. Não lê relógio, não abre soquete, não fala com o `quinn`. Quem a
/// alimenta é `crate::enlace`, na tica que ele já tem.
#[derive(Debug, Clone, Copy)]
pub struct Sonda {
    estimativa_bps: u32,
    /// Até onde o passo largo pode ir, quando já se sabe alguma coisa.
    ///
    /// `None` antes da primeira dor: aí a subida é livre até
    /// [`TETO_DA_ESTIMATIVA_BPS`], que é como a sonda sai da suposição depressa.
    limite_bps: Option<u32>,
    /// A menor ida e volta já vista nesta sonda, que é a linha de base da fila.
    ///
    /// Menor e não primeira: a primeira pode ter sido tirada com a fila já
    /// cheia, e aí a fila nunca mais pareceria fila. Nunca esquecida — é o
    /// mesmo defeito ao contrário, e uma rota que mudou de verdade aparece como
    /// uma sonda pessimista, que é o erro barato.
    menor_ida_e_volta: Option<Duration>,
    /// Desde quando não dói nada. É o relógio da sondagem.
    calmo_desde: Option<Instant>,
    /// Quanto de calmaria a próxima sondagem exige.
    ///
    /// Dobra a cada sondagem que doeu e volta a [`ESQUECIMENTO`] a cada
    /// sondagem que deu certo. É o que faz o custo da sondagem sumir numa rede
    /// que não muda e voltar depressa numa rede que mudou.
    espera_da_sondagem: Duration,
    /// Há uma sondagem no ar, esperando a janela que a julga.
    sondando: bool,
    janela: Option<Janela>,
    /// Quantas janelas fecharam sem medida de dor nenhuma (sem perda, sem
    /// congestionamento, sem ida e volta fresca). É o que `crate::enlace` conta
    /// no log: uma sonda que não está medindo nada tem de dizer.
    janelas_sem_medida: u64,
    /// Quantas janelas fecharam **com** medida de dor nesta sonda. Zero numa
    /// sonda que só recebeu a semente ([`Self::partindo_de`]): ver
    /// [`Self::estimativa_medida`].
    janelas_medidas: u64,
}

impl Default for Sonda {
    fn default() -> Self {
        Self::nova()
    }
}

impl Sonda {
    /// Uma sonda que ainda não mediu nada, começando na suposição do §8.
    ///
    /// [`CAMINHO_DA_PROVA_BPS`] e não zero, e não um chute otimista: é o cano
    /// sobre o qual as duas provas rodaram, a única suposição com número atrás.
    /// A diferença para antes deste módulo é que agora ele é **por onde se
    /// começa**, e não a resposta.
    #[must_use]
    pub const fn nova() -> Self {
        Self {
            estimativa_bps: CAMINHO_DA_PROVA_BPS,
            limite_bps: None,
            menor_ida_e_volta: None,
            calmo_desde: None,
            espera_da_sondagem: ESQUECIMENTO,
            sondando: false,
            janela: None,
            janelas_sem_medida: 0,
            janelas_medidas: 0,
        }
    }

    /// Quantas janelas fecharam sem medida de dor nenhuma. Ver
    /// [`TETO_SEM_MEDIDA_BPS`].
    #[must_use]
    pub const fn janelas_sem_medida(&self) -> u64 {
        self.janelas_sem_medida
    }

    /// A estimativa, **se esta sonda mediu alguma coisa**; `None` se ela só
    /// carrega a semente ou a suposição.
    ///
    /// É o que se pode gravar como medida. Gravar [`Self::estimativa`] fazia
    /// uma sessão sem medida nenhuma regravar a semente que recebeu, com data
    /// nova — e a lembrança se renovava sozinha para sempre, sem que o caminho
    /// de hoje a tivesse confirmado uma vez.
    #[must_use]
    pub const fn estimativa_medida(&self) -> Option<u32> {
        if self.janelas_medidas == 0 {
            None
        } else {
            Some(self.estimativa_bps)
        }
    }

    /// Uma sonda que começa de uma medida, e não do palpite de
    /// [`CAMINHO_DA_PROVA_BPS`].
    ///
    /// # O defeito que ela conserta
    ///
    /// A escada existe porque ninguém sabe o cano. Quem já mediu **sabe**, e
    /// recomeçar do palpite universal é jogar a medida fora — toda transmissão
    /// partia supondo uma subida doméstica de 2 Mbps, inclusive entre duas
    /// máquinas no mesmo switch que mediram 12,48 Mbps entre si vinte minutos
    /// antes. Medido em campo, LAN, duas máquinas:
    ///
    /// ```text
    /// 2.400.000 →  4.524.054   → 720p
    /// 4.524.054 →  9.048.108   → 1080p
    /// 9.048.108 → 12.480.000   ← topo
    /// ```
    ///
    /// Doze segundos de imagem ruim para reaprender um fato que era verdade
    /// desde o primeiro milissegundo — e o código já suspeitava, porque
    /// [`IDA_E_VOLTA_CURTA`] tinha reconhecido o cano curto e trocado o passo
    /// para [`SUBIDA_DE_CAMINHO_CURTO`]. Ele sabia que era uma LAN e mesmo
    /// assim começava do palpite.
    ///
    /// # O que ela não muda
    ///
    /// O valor é grampeado na faixa que a escada admite: memória não autoriza
    /// começar fora dela. E a sonda continua sendo uma sonda — o §5 não é
    /// tocado, porque a escolha da pessoa continua sendo **teto e nunca piso**,
    /// e a primeira janela que doer corrige este número como corrigiria
    /// qualquer outro.
    ///
    /// **Não põe [`Self::limite_bps`]**, e isso é decisão: uma medida lembrada é
    /// de onde partir, não uma borda encontrada. A histerese existe para o que
    /// doeu **nesta** sessão, e herdá-la de ontem impediria a sonda de descobrir
    /// que o cano cresceu.
    #[must_use]
    pub const fn partindo_de(bps: u32) -> Self {
        let lembrado = if bps < PISO_DA_ESTIMATIVA_BPS {
            PISO_DA_ESTIMATIVA_BPS
        } else if bps > TETO_DA_ESTIMATIVA_BPS {
            TETO_DA_ESTIMATIVA_BPS
        } else {
            bps
        };
        Self {
            estimativa_bps: lembrado,
            ..Self::nova()
        }
    }

    /// O caminho de subida desta máquina, como a sonda o conhece agora.
    ///
    /// É o número que vai para `crate::tela::TetoDeVideo::com_caminho`.
    #[must_use]
    pub const fn estimativa(&self) -> u32 {
        self.estimativa_bps
    }

    /// Até onde o passo largo pode ir, ou `None` enquanto nada doeu.
    #[must_use]
    pub const fn limite_bps(&self) -> Option<u32> {
        self.limite_bps
    }

    /// Fecha a janela aberta, porque o que vem a seguir não continua o que veio
    /// antes.
    ///
    /// Dois motivos, e os dois são de `crate::enlace`: **a conexão caiu** — os
    /// contadores do `quinn` voltam a zero na próxima, e a subtração contra os
    /// da anterior daria uma janela absurda —, ou **a transmissão parou**, e aí
    /// entre esta e a próxima o cano fica vazio; uma janela que atravessasse o
    /// buraco mediria o silêncio.
    ///
    /// **Não mexe na estimativa**, e isso é decisão: o que uma conexão nova zera
    /// são os contadores dela, não a casa em que esta pessoa está sentada. Quem
    /// cai e volta em cinco segundos volta para o mesmo cano.
    pub fn esquecer_a_conexao(&mut self) {
        self.janela = None;
    }

    /// De quanto a estimativa sobe nesta janela, conforme o comprimento do
    /// caminho.
    ///
    /// Ver [`SUBIDA_DE_CAMINHO_CURTO`]. Sem uma medida de ida e volta ainda, o
    /// passo é o de sempre: supor caminho curto sem evidência seria a suposição
    /// que este módulo inteiro existe para não fazer.
    #[must_use]
    fn passo_de_subida(&self) -> u32 {
        match self.menor_ida_e_volta {
            Some(volta) if volta <= IDA_E_VOLTA_CURTA => SUBIDA_DE_CAMINHO_CURTO,
            _ => SUBIDA,
        }
    }

    /// Uma leitura. Devolve a estimativa nova quando ela andou, e `None` quando
    /// não.
    ///
    /// `None` na maior parte das chamadas, e é assim que tem de ser: quem chama
    /// tica cinco vezes por segundo e a janela dura [`JANELA`]. Devolver `Some`
    /// à toa acordaria a thread do codificador para lhe dizer o que ela já sabe
    /// — a mesma preocupação que `crate::enlace::faixa_nova` já tem.
    ///
    /// # O que decide uma janela fechada
    ///
    /// Três perguntas, e as três são independentes:
    ///
    /// - **doeu?** — `congestion_events` ou `lost_packets` cresceram, ou a ida e
    ///   volta subiu mais que [`FILA_TOLERADA`] acima do mínimo já visto. É a
    ///   pergunta sobre a **capacidade**, e não sobre a dor da voz: a faixa de
    ///   `SignalBand` não entra aqui, pelo motivo que o cabeçalho do módulo dá.
    /// - **encheu?** — o que saiu pelo soquete chegou a [`OCUPACAO_MINIMA`] do
    ///   teto que valia. Uma janela que não encheu não mediu nada, e é a
    ///   primeira armadilha do cabeçalho.
    /// - **o teto era o nosso?** — o `min` das três pernas do §5.1 escolheu a
    ///   desta máquina, e não a de quem hospeda dividida pelos espectadores nem
    ///   a escolha da pessoa. Uma janela apertada por outra perna mediu **aquela
    ///   perna**.
    ///
    /// Daí, e nesta ordem:
    ///
    /// | | o teto era o nosso | era de outra perna |
    /// |---|---|---|
    /// | **doeu**, e encheu | a estimativa vira o que a janela **carregou** | recua [`QUEDA`] |
    /// | **doeu**, e não encheu | recua [`QUEDA`] | recua [`QUEDA`] |
    /// | **não doeu**, e encheu | sobe [`SUBIDA`] | sobe [`SUBIDA`], travada no que a janela sustenta |
    /// | **não doeu**, e não encheu | nada | nada |
    ///
    /// O canto de cima à esquerda é o coração disto, e a terceira armadilha do
    /// cabeçalho é por que ele é uma medida e não um fator. A última coluna da
    /// terceira linha é o freio que impede a estimativa de virar esperança
    /// numa sala cheia, e está escrita por extenso no corpo da função.
    pub fn observar(&mut self, agora: Instant, amostra: &Amostra) -> Option<u32> {
        // A linha de base da fila é atualizada em **toda** leitura, e não só
        // quando uma janela fecha. A primeira ida e volta de uma sonda nova é a
        // única que ela tem: se ela só fosse anotada no fim da primeira janela,
        // uma janela que já nasceu com fila viraria a linha de base, e daí em
        // diante fila nenhuma pareceria fila.
        // Só com medida: uma ida e volta que não existe não é um mínimo de
        // zero, e zero aqui faria todo caminho parecer curto.
        if let Some(medida) = amostra.transporte.ida_e_volta.filter(|m| m.amostras > 0) {
            self.menor_ida_e_volta = Some(match self.menor_ida_e_volta {
                Some(menor) => menor.min(medida.valor),
                None => medida.valor,
            });
        }

        let teto_bps = match amostra.teto {
            Teto::Bps(bps) => bps,
            // Vídeo parado não enche cano nenhum, e os bytes desta janela são a
            // voz. Ler isso como caminho é a primeira armadilha em pessoa.
            Teto::Parado(_) => {
                self.janela = None;
                return None;
            }
        };

        let Some(janela) = self.janela else {
            self.abrir(agora, amostra, teto_bps);
            return None;
        };

        // Uma janela com dois tetos não mede nem um nem o outro: a ocupação
        // seria comparada com um orçamento que não valeu o tempo todo. Recomeça,
        // e recomeçar custa no máximo uma tica.
        if janela.teto_bps != teto_bps || janela.faixa != amostra.faixa {
            self.abrir(agora, amostra, teto_bps);
            return None;
        }

        let decorrido = agora.saturating_duration_since(janela.abriu);
        if decorrido < JANELA {
            return None;
        }

        let antes = self.estimativa_bps;
        self.fechar(agora, amostra, &janela, decorrido);
        self.abrir(agora, amostra, teto_bps);
        (self.estimativa_bps != antes).then_some(self.estimativa_bps)
    }

    fn abrir(&mut self, agora: Instant, amostra: &Amostra, teto_bps: u32) {
        self.janela = Some(Janela {
            abriu: agora,
            transporte: amostra.transporte,
            teto_bps,
            faixa: amostra.faixa,
        });
    }

    fn fechar(&mut self, agora: Instant, amostra: &Amostra, janela: &Janela, decorrido: Duration) {
        let entregue_bps = bps_de(
            amostra
                .transporte
                .bytes_enviados
                .saturating_sub(janela.transporte.bytes_enviados),
            decorrido,
        );

        // **Cada pergunta de dor só responde com medida.** `None` é «não
        // sabido», e não «não doeu»: é a regressão que a revisão da 0.17 achou
        // (um zero sintético lido como caminho limpo).
        let cresceu = |agora: Option<u64>, antes: Option<u64>| match (agora, antes) {
            (Some(agora), Some(antes)) => Some(agora > antes),
            _ => None,
        };
        let perdeu = cresceu(
            amostra.transporte.pacotes_perdidos,
            janela.transporte.pacotes_perdidos,
        );
        let congestionou = cresceu(
            amostra.transporte.eventos_de_congestionamento,
            janela.transporte.eventos_de_congestionamento,
        );
        let base = self.menor_ida_e_volta;
        let fila_de = |volta: Duration| base.map(|base| volta.saturating_sub(base) > FILA_TOLERADA);
        // A ida e volta só conta se é **desta janela**: o número de respostas
        // andou dentro dela.
        let enfileirou = match (
            amostra.transporte.ida_e_volta,
            janela.transporte.ida_e_volta,
        ) {
            (Some(agora), antes) if agora.amostras > antes.map_or(0, |antes| antes.amostras) => {
                fila_de(agora.valor)
            }
            _ => None,
        };
        // E um `Ping` que ainda não voltou e já passou da fila tolerada é dor
        // medida, mesmo sem resposta: a fila de segundos é a que nunca responde
        // dentro da janela.
        let atrasou = amostra
            .transporte
            .ida_e_volta
            .and_then(|medida| medida.em_voo)
            .and_then(fila_de)
            .filter(|atrasou| *atrasou);

        let doeu = [perdeu, congestionou, enfileirou, atrasou].contains(&Some(true));
        // O que prova que **não** doeu: uma ida e volta fresca, ou perda e
        // congestionamento contados. Sem isso a janela não sabe, e não sobe.
        let mediu = enfileirou.is_some() || (perdeu.is_some() && congestionou.is_some());

        let cheia = u64::from(entregue_bps) * 100
            >= u64::from(janela.teto_bps) * u64::from(OCUPACAO_MINIMA);
        // O teto daquela janela era o **nosso**? A perna de quem compartilha é
        // 60% da estimativa, cortada pela metade em `Degraded` — exatamente o
        // que `TetoDeVideo` faz com ela. Se o teto que valeu foi menor que isso,
        // quem apertou foi outra perna do §5.1 (a de quem hospeda, dividida
        // pelos espectadores) ou a escolha da pessoa, e o que a janela mediu foi
        // aquela perna e não esta.
        let nossa = janela.teto_bps >= nossa_perna(self.estimativa_bps, janela.faixa);

        // Dor também é medida: o que se grava depois é o que alguma janela
        // desta sonda confirmou, para baixo ou para cima.
        if doeu || mediu {
            self.janelas_medidas = self.janelas_medidas.saturating_add(1);
        }

        if doeu {
            self.calmo_desde = Some(agora);
            // Uma sondagem que doeu é uma sondagem que respondeu: o cano é o que
            // a última medida disse. A próxima pergunta pode esperar o dobro.
            if self.sondando {
                self.sondando = false;
                self.espera_da_sondagem = (self.espera_da_sondagem * 2).min(TETO_DA_ESPERA);
            }
            let recuo = if cheia && nossa {
                // A medida. No instante em que o caminho reclamou, o que saiu
                // pelo soquete é tudo o que ele deixou sair — e isso é o
                // caminho, não 60% dele nem 1,67 vezes ele. Ver a terceira
                // armadilha no cabeçalho do módulo.
                //
                // Só quando o teto era o **nosso**: numa sala de seis, o teto é
                // a subida de quem hospeda dividida por seis, e o que a janela
                // carregou é aquela perna. Ler isso como «o meu cano tem tanto»
                // deixaria a sala cheia estragar a medida desta máquina, e a
                // deixaria estragada depois de todo mundo sair.
                entregue_bps.min(self.estimativa_bps)
            } else {
                escalar(self.estimativa_bps, QUEDA)
            };
            self.estimativa_bps = recuo.max(PISO_DA_ESTIMATIVA_BPS);
            // E fica sabido: o passo largo não volta a passar por aqui sozinho.
            // **Esta linha é a histerese inteira** — sem ela a estimativa
            // voltaria a subir até doer de novo, e o degrau de resolução
            // balançaria junto, uma troca a cada poucas janelas.
            self.limite_bps = Some(self.estimativa_bps);
            return;
        }

        // **Sem medida, conservador.** A janela não sabe se doeu: não sobe, não
        // sonda, e volta ao cano das provas se estava acima dele. Ver
        // [`TETO_SEM_MEDIDA_BPS`] para o porquê deste número.
        if !mediu {
            self.janelas_sem_medida = self.janelas_sem_medida.saturating_add(1);
            self.estimativa_bps = self
                .estimativa_bps
                .clamp(PISO_DA_ESTIMATIVA_BPS, TETO_SEM_MEDIDA_BPS);
            return;
        }

        if !cheia {
            return;
        }

        // **E a faixa não pode fazer uma janela parecer cheia.**
        //
        // `cheia` compara o que saiu com `janela.teto_bps`, e em
        // [`SignalBand::Degraded`] esse teto **já vem cortado pela metade** —
        // `TetoDeVideo::teto` o corta, e por uma razão que não tem nada a ver
        // com capacidade. Uma janela assim enche o próprio teto usando 30% do
        // cano e diz «coube», e a estimativa sobe um passo largo em cima de uma
        // prova que não existe. Repetido a cada janela de uma faixa ruim, o
        // número vai até [`TETO_DA_ESTIMATIVA_BPS`] sem nunca ter tocado a
        // borda; quando a faixa volta a `Nominal`, o teto salta para 60% de um
        // número inventado e come de uma vez a reserva da voz — que é
        // exatamente a terceira armadilha do cabeçalho deste módulo, entrando
        // pela porta que ela não vigiava.
        //
        // Descer continua permitido em qualquer faixa: `doeu` já retornou
        // acima, e dor é notícia verdadeira venha de onde vier. O que uma faixa
        // cortada não pode fazer é **dizer um tamanho**, que é a mesma frase da
        // primeira armadilha.
        //
        // Isto ficou escondido enquanto o teto da estimativa era 10 Mbps: com
        // ele, os dois lados de `a_faixa_da_voz_nao_corta_a_estimativa_uma_segunda_vez`
        // paravam no mesmo teto e a igualdade passava por empate, não por
        // acerto. Subir o teto para 14 Mbps separou os dois e mostrou o defeito.
        if !matches!(janela.faixa, SignalBand::Nominal) {
            return;
        }

        // Uma sondagem que **não** doeu é a notícia de que o cano cresceu, e a
        // última medida deixou de valer. O limite cai, e a subida volta a ser
        // livre: em poucas janelas ela reencontra a borda nova e a mede lá,
        // como mediu a primeira. E a próxima pergunta volta a ser cedo, porque
        // uma rede que acabou de mudar é uma rede que pode mudar de novo.
        if self.sondando {
            self.sondando = false;
            self.espera_da_sondagem = ESQUECIMENTO;
            self.limite_bps = None;
            self.estimativa_bps = entregue_bps.max(PISO_DA_ESTIMATIVA_BPS);
            self.calmo_desde = Some(agora);
            return;
        }

        match self.calmo_desde {
            Some(desde)
                if self.limite_bps.is_some()
                    && agora.saturating_duration_since(desde) >= self.espera_da_sondagem =>
            {
                // **A sondagem, e ela é uma pergunta e não um passo.**
                //
                // O teto vai para exatamente onde a última medida disse que o
                // cano acaba — `caminho_que_sustenta` é a volta da conta de
                // [`FRACAO_DO_CAMINHO`], então 60% desta estimativa nova são os
                // bits que a dor de antes mediu. Só há duas respostas, e a
                // janela seguinte dá uma delas: ou dói, e o cano é o mesmo — a
                // metade de cima desta função põe tudo de volta no lugar, e a
                // conta desta pergunta foi **uma** janela de fila; ou não dói,
                // e o cano cresceu, e é o ramo logo acima.
                //
                // Uma sondagem que subisse aos poucos custaria o mesmo em fila
                // e cobraria por muito mais tempo: a estimativa passaria um
                // terço da conversa perto da borda, e a reserva da voz —
                // aqueles 40% — seria comida de raspão em vez de tocada uma vez.
                self.sondando = true;
                self.calmo_desde = Some(agora);
                self.estimativa_bps = caminho_que_sustenta(self.estimativa_bps, janela.faixa)
                    .max(self.estimativa_bps);
                self.limite_bps = Some(self.estimativa_bps);
                return;
            }
            Some(_) => {}
            None => self.calmo_desde = Some(agora),
        }

        // O passo largo, e os três freios dele.
        //
        // O segundo é o que impede a estimativa de virar esperança quando a
        // janela não foi apertada por **nós**: numa sala de seis, ou com o
        // `HostUplink` calado, o teto que valeu é o de outra perna, a
        // transmissão o enche sem esforço e nenhuma janela chega a testar este
        // cano. Multiplicar por [`SUBIDA`] ali levaria a estimativa a
        // [`TETO_DA_ESTIMATIVA_BPS`] sem uma medida atrás — e no dia em que a
        // outra perna afrouxasse, o teto saltaria para um número que ninguém
        // provou, em cima da voz. [`caminho_que_sustenta`] é o maior caminho de
        // que a janela é evidência: aquele em que [`FRACAO_DO_CAMINHO`] dele
        // ainda cabe no que de fato saiu pelo soquete.
        // O passo, escolhido pelo comprimento do caminho. Ver
        // [`SUBIDA_DE_CAMINHO_CURTO`]: o que muda não é a confiança na medida,
        // é a pressa em chegar até ela.
        let passo = self.passo_de_subida();
        let alvo = escalar(self.estimativa_bps, passo)
            .min(escalar(
                caminho_que_sustenta(entregue_bps, janela.faixa),
                passo,
            ))
            .min(self.limite_bps.unwrap_or(TETO_DA_ESTIMATIVA_BPS))
            .min(TETO_DA_ESTIMATIVA_BPS);
        // Nunca para baixo por aqui: descer é assunto da metade de cima desta
        // função, que tem uma medida ou uma dor na mão. Um alvo abaixo da
        // estimativa de agora — o caso da sala cheia, de novo — não pode virar
        // uma queda silenciosa.
        self.estimativa_bps = alvo.max(self.estimativa_bps);
    }
}

/// O maior caminho de que uma janela que entregou `entregue_bps` é evidência.
///
/// A volta da conta de [`FRACAO_DO_CAMINHO`]: se o teto é 60% do caminho, um
/// caminho de `entregue / 0,6` é o maior em que o que saiu pelo soquete ainda
/// caberia no teto. Afirmar mais que isso é afirmar sobre bits que ninguém viu
/// passar.
fn caminho_que_sustenta(entregue_bps: u32, faixa: SignalBand) -> u32 {
    let por_cento = match faixa {
        SignalBand::Nominal => FRACAO_DO_CAMINHO,
        // A faixa degradada corta o teto pela metade, então metade da fração —
        // e é a mesma metade de `nossa_perna`, para que as duas sejam a ida e a
        // volta da mesma conta. Sem isto, a sonda leria uma janela degradada
        // como se o cano fosse metade do que é, e a faixa da voz cortaria a
        // estimativa por dentro: exatamente a segunda armadilha, entrando pela
        // porta dos fundos.
        SignalBand::Degraded => FRACAO_DO_CAMINHO / 2,
        // Vídeo parado; `observar` nem chega aqui.
        SignalBand::Critical => return TETO_DA_ESTIMATIVA_BPS,
    };
    let sustentado = (u64::from(entregue_bps) * 100) / u64::from(por_cento);
    u32::try_from(sustentado)
        .unwrap_or(u32::MAX)
        .min(TETO_DA_ESTIMATIVA_BPS)
}

/// A perna de quem compartilha, como [`crate::tela::TetoDeVideo`] a calcula.
///
/// Escrita aqui e não lá porque lá ela é privada e depende do estado inteiro do
/// teto; o que esta sonda precisa é da mesma conta sobre o número que ela mesma
/// produziu. As duas têm de concordar, e concordam por construção: são
/// [`FRACAO_DO_CAMINHO`] e a mesma metade da faixa degradada.
const fn nossa_perna(estimativa_bps: u32, faixa: SignalBand) -> u32 {
    let inteira = ((estimativa_bps as u64 * FRACAO_DO_CAMINHO as u64) / 100) as u32;
    match faixa {
        SignalBand::Nominal => inteira,
        SignalBand::Degraded => inteira / 2,
        // Faixa crítica não tem teto — o vídeo está parado, e `observar` nem
        // chega aqui. Zero é o que faz a comparação ser verdadeira à toa, e é o
        // certo: se houvesse janela, ela não seria sobre a nossa perna.
        SignalBand::Critical => 0,
    }
}

/// `bps` vezes `por_cento` por cento, sem dar a volta e sem passar do teto.
fn escalar(bps: u32, por_cento: u32) -> u32 {
    let escalado = (u64::from(bps) * u64::from(por_cento)) / 100;
    u32::try_from(escalado)
        .unwrap_or(u32::MAX)
        .min(TETO_DA_ESTIMATIVA_BPS)
}

/// Quantos bits por segundo são `bytes` em `decorrido`.
fn bps_de(bytes: u64, decorrido: Duration) -> u32 {
    let micros = decorrido.as_micros();
    if micros == 0 {
        return 0;
    }
    let bits = u128::from(bytes)
        .saturating_mul(8)
        .saturating_mul(1_000_000);
    u32::try_from(bits / micros).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use seele_video::codec::Resolucao;

    use super::*;
    use crate::tela::TetoDeVideo;

    /// A tica com que `crate::enlace` alimenta a sonda.
    const TICA: Duration = Duration::from_millis(200);

    /// O que a voz de uma pessoa gasta enquanto a tela transmite.
    ///
    /// `specs/03-audio.md` põe a voz em 40 kbps; 60 é isso com o cabeçalho de
    /// datagrama e o ACK que andam junto. Existe para que o cano simulado não
    /// seja um cano só de tela — a segunda perna do §3.2 é justamente o que a
    /// reserva protege.
    const VOZ_BPS: u32 = 60_000;

    /// Um cano com capacidade, fila e perda, que responde como o `quinn`
    /// responderia.
    ///
    /// Grosseiro de propósito: não modela CUBIC, nem MTU, nem o balde do
    /// `crate::tela`. Modela a única coisa de que estes testes precisam — **um
    /// cano tem fundo**, e quem pede mais que o fundo recebe o fundo, vê fila e
    /// vê perda.
    struct Cano {
        capacidade_bps: u32,
        transporte: Transporte,
        base: Duration,
    }

    impl Cano {
        /// Um cano que conta tudo: perda, congestionamento e ida e volta.
        fn de(capacidade_bps: u32) -> Self {
            Self {
                capacidade_bps,
                transporte: Transporte {
                    bytes_enviados: 0,
                    eventos_de_congestionamento: Some(0),
                    pacotes_perdidos: Some(0),
                    ida_e_volta: Some(IdaEVolta {
                        valor: Duration::from_millis(20),
                        amostras: 1,
                        em_voo: None,
                    }),
                },
                base: Duration::from_millis(20),
            }
        }

        /// O cano do enlace da 0.17: só bytes e a ida e volta do `Ping` da
        /// sonda — sem perda nem congestionamento contados.
        fn do_enlace(capacidade_bps: u32) -> Self {
            let mut cano = Self::de(capacidade_bps);
            cano.transporte.eventos_de_congestionamento = None;
            cano.transporte.pacotes_perdidos = None;
            cano
        }

        /// Um cano sem medida de dor nenhuma: só os bytes que saíram.
        fn cego(capacidade_bps: u32) -> Self {
            let mut cano = Self::do_enlace(capacidade_bps);
            cano.transporte.ida_e_volta = None;
            cano
        }

        /// Roda `duracao` com a tela pedindo `oferta_bps` além da voz.
        fn correr(&mut self, oferta_bps: u32, duracao: Duration) {
            let pedido = u64::from(oferta_bps) + u64::from(VOZ_BPS);
            let cabe = u64::from(self.capacidade_bps);
            let entregue = pedido.min(cabe);
            let bytes = (entregue * duracao.as_micros() as u64) / (8 * 1_000_000);
            self.transporte.bytes_enviados += bytes;
            let estourou = pedido > cabe;
            if estourou {
                if let Some(eventos) = self.transporte.eventos_de_congestionamento.as_mut() {
                    *eventos += 1;
                }
                if let Some(perdidos) = self.transporte.pacotes_perdidos.as_mut() {
                    *perdidos += 3;
                }
            }
            if let Some(medida) = self.transporte.ida_e_volta.as_mut() {
                medida.amostras += 1;
                medida.valor = if estourou {
                    self.base + Duration::from_millis(200)
                } else {
                    self.base
                };
            }
        }

        /// A fila que a voz pega neste cano, com a tela pedindo `oferta_bps`:
        /// zero enquanto cabe, e o que transborda dividido pela capacidade
        /// quando não cabe (um segundo de excesso acumulado na fila).
        fn atraso_da_voz(&self, oferta_bps: u32) -> Duration {
            let pedido = u64::from(oferta_bps) + u64::from(VOZ_BPS);
            let cabe = u64::from(self.capacidade_bps);
            if pedido <= cabe {
                return Duration::ZERO;
            }
            Duration::from_micros((pedido - cabe) * 1_000_000 / cabe)
        }

        /// Uma perda que só conta onde o cano conta perda.
        fn perder(&mut self) {
            if let Some(perdidos) = self.transporte.pacotes_perdidos.as_mut() {
                *perdidos += 1;
            }
        }

        fn amostra(&self, teto: Teto, faixa: SignalBand) -> Amostra {
            Amostra {
                transporte: self.transporte,
                teto,
                faixa,
            }
        }
    }

    /// A subida que o servidor declarou, nos testes em que a perna dele não é o
    /// assunto.
    ///
    /// **Larga de propósito, e é a única maneira de estes testes serem sobre a
    /// perna que a sonda mede.** `TetoDeVideo::com_caminho` deixa a perna de
    /// quem hospeda no cano das provas, e com ela ali o `min` do §5.1 trava o
    /// teto em 1200 kbps por mais que esta máquina meça — que é um achado sobre
    /// o produto e não sobre a sonda, e tem teste próprio em
    /// `sem_a_subida_do_server_a_medida_desta_maquina_nao_levanta_o_teto`.
    const SERVER_LARGO_BPS: u32 = 200_000_000;

    /// O teto com as pernas do §5.1 que estes testes querem: a nossa, medida,
    /// e a de quem hospeda fora do caminho.
    fn teto_de(estimativa_bps: u32, faixa: SignalBand) -> Teto {
        TetoDeVideo::com_caminho(estimativa_bps)
            .com_caminho_de_quem_hospeda(SERVER_LARGO_BPS)
            .teto(faixa)
    }

    /// Roda a malha inteira — sonda, teto, cano — pelo tempo pedido, do jeito
    /// que `crate::enlace` a roda.
    ///
    /// A ordem importa e é a de produção: o teto só é refeito **depois** de a
    /// sonda dizer que a estimativa andou. Refazê-lo a cada tica daria uma
    /// janela com dois tetos a cada volta, e a sonda descartaria todas elas.
    fn correr(
        sonda: &mut Sonda,
        cano: &mut Cano,
        faixa: SignalBand,
        ticas: u32,
        mut a_cada_tica: impl FnMut(u32, Teto),
    ) {
        let inicio = Instant::now();
        let mut teto = teto_de(sonda.estimativa(), faixa);
        for k in 1..=ticas {
            let agora = inicio + TICA * k;
            cano.correr(teto.bps(), TICA);
            if sonda.observar(agora, &cano.amostra(teto, faixa)).is_some() {
                teto = teto_de(sonda.estimativa(), faixa);
            }
            a_cada_tica(k, teto);
        }
    }

    /// Quantas ticas cabem em `segundos`.
    const fn ticas(segundos: u32) -> u32 {
        segundos * 5
    }

    // -----------------------------------------------------------------------

    /// A pergunta 2 do §8, respondida: a estimativa sobe **por evidência**.
    ///
    /// Num cano de 8 Mbps a suposição de 2 Mbps é uma calúnia, e o produto
    /// mostrava 720p onde cabia 1080p. Cinco janelas cheias e sem piora têm de
    /// levá-la a 6 Mbps — «começar alto» sem o custo de começar alto.
    #[test]
    fn a_estimativa_sobe_por_evidencia_e_chega_perto_do_cano_em_segundos() {
        let mut sonda = Sonda::nova();
        let mut cano = Cano::de(8_000_000);
        assert_eq!(sonda.estimativa(), CAMINHO_DA_PROVA_BPS);

        correr(
            &mut sonda,
            &mut cano,
            SignalBand::Nominal,
            ticas(10),
            |_, _| {},
        );

        assert!(
            sonda.estimativa() >= 6_000_000,
            "dez segundos de janela cheia num cano de 8 Mbps e a sonda ainda diz {}",
            sonda.estimativa()
        );
        // E a resolução acompanha, que é o defeito que o usuário relatava.
        assert_eq!(
            teto_de(sonda.estimativa(), SignalBand::Nominal).resolucao_estimada(),
            Some(Resolucao::P1080)
        );
    }

    /// A primeira armadilha: **tela parada quase não gasta bits**.
    ///
    /// Uma sequência de janelas em que a captura não tinha o que mandar entrega
    /// só a voz. Lida como caminho, ela trancaria todo mundo em 540p
    /// permanentemente — e o defeito só apareceria em campo, porque em
    /// laboratório a tela está sempre mexendo.
    ///
    /// Duas metades, e a segunda é a perigosa:
    ///
    /// 1. na calmaria, sessenta segundos de tela parada não mexem em nada;
    /// 2. **com uma perda esporádica no meio** — o Wi-Fi de qualquer casa —, a
    ///    sonda recua um passo e **não** confunde os 60 kbps que saíram com o
    ///    tamanho do cano. Sem a guarda, aquela janela viraria «o cano tem
    ///    60 kbps», a estimativa iria ao piso e a transmissão pararia por
    ///    [`crate::tela::MotivoDeParada::AbaixoDoPiso`] numa casa onde cabia
    ///    1080p.
    ///
    /// Confira por mutação: apague o `cheia &&` de `if cheia && nossa` na metade
    /// de cima de [`Sonda::observar`] e a segunda metade fica vermelha.
    #[test]
    fn uma_tela_parada_nao_derruba_a_estimativa() {
        let mut sonda = Sonda::nova();
        let mut cano = Cano::de(8_000_000);
        let inicio = Instant::now();
        let antes = sonda.estimativa();
        let teto = teto_de(antes, SignalBand::Nominal);

        // Sessenta segundos de tela parada: o teto continua o que era, e o que
        // sai pelo fio é a voz e mais nada.
        for k in 1..=ticas(60) {
            cano.correr(0, TICA);
            sonda.observar(inicio + TICA * k, &cano.amostra(teto, SignalBand::Nominal));
        }

        assert_eq!(
            sonda.estimativa(),
            antes,
            "uma tela parada mexeu na estimativa do caminho"
        );
        assert_eq!(
            sonda.limite_bps(),
            None,
            "uma tela parada ensinou à sonda um limite que ninguém mediu"
        );

        // E agora a metade perigosa: a mesma tela parada, e três perdas
        // esporádicas no meio — o Wi-Fi de qualquer casa, e não um enlace
        // quebrado, que é outro caso e tem outra resposta.
        let comeco = ticas(60);
        for k in 1..=ticas(60) {
            cano.correr(0, TICA);
            if k % ticas(20) == 0 {
                cano.perder();
            }
            sonda.observar(
                inicio + TICA * (comeco + k),
                &cano.amostra(teto, SignalBand::Nominal),
            );
        }

        // Três recuos de [`QUEDA`] sobre 2 Mbps são 1024 kbps, e é isso que a
        // sonda tem de dizer: ela recuou porque doeu, e não porque leu 60 kbps
        // de voz como o tamanho do cano.
        assert!(
            sonda.estimativa() >= 1_000_000,
            "os 60 kbps de uma tela parada viraram o tamanho do cano: a sonda \
             desceu a {} bps",
            sonda.estimativa()
        );
        assert!(
            sonda.estimativa() < CAMINHO_DA_PROVA_BPS,
            "doeu três vezes e a sonda não recuou nenhuma"
        );
    }

    /// **A terceira armadilha, e o irmão de `a_voz_nunca_cede_a_tela`.**
    ///
    /// Uma sonda que subisse até doer e chamasse de caminho o valor em que doeu
    /// pararia com a estimativa 1/0,6 ≈ 1,67 vezes maior que o cano — e 60% de
    /// um número 67% grande demais são 100% do cano. Os 40% que
    /// `spikes/tela-no-transporte` reservou para a voz, e que separam 23,1 ms de
    /// p50 de 225,7 ms, evaporariam **por causa desta melhoria**.
    ///
    /// Então a asserção é sobre o teto e não sobre a estimativa: depois de a
    /// sonda encontrar o fundo do cano, o que a tela pede tem de continuar
    /// sendo perto de 60% dele, em todo cano.
    #[test]
    fn a_reserva_da_voz_sobrevive_a_sonda() {
        for capacidade in [1_500_000, 3_000_000, 5_000_000, 12_000_000] {
            let mut sonda = Sonda::nova();
            let mut cano = Cano::de(capacidade);
            let mut ocupacoes = Vec::new();

            correr(
                &mut sonda,
                &mut cano,
                SignalBand::Nominal,
                ticas(180),
                |k, teto| {
                    // Os primeiros segundos são a subida, e ela é uma aposta
                    // por desenho. O que este teste cobra é o **regime**.
                    if k > ticas(30) {
                        let oferta = u64::from(teto.bps()) + u64::from(VOZ_BPS);
                        ocupacoes.push(oferta * 100 / u64::from(capacidade));
                    }
                },
            );

            ocupacoes.sort_unstable();
            let mediana = ocupacoes[ocupacoes.len() / 2];
            let encostadas = ocupacoes.iter().filter(|pedaco| **pedaco >= 100).count();

            // **A asserção que este módulo existe para não quebrar.** No regime,
            // a tela pede perto de 60% do cano e a voz fica com o resto — que é
            // a linha do §3.2, a mesma que separa 23,1 ms de p50 de 225,7. 75%
            // e não 60% porque a sonda erra, e tem de errar para algum lado; o
            // que não pode é o erro **morar** em cima da reserva, que é o que
            // «subir até doer e ficar lá» faz — ali este número daria 100.
            assert!(
                mediana <= 75,
                "num cano de {capacidade} bps a tela vive pedindo {mediana}% dele, \
                 e sobram {}% para a voz",
                100_u64.saturating_sub(mediana)
            );
            // E não pode errar tanto para baixo a ponto de o recurso não servir:
            // uma sonda covarde devolve 540p num cano de fibra.
            assert!(
                mediana >= 35,
                "num cano de {capacidade} bps a tela só ousou pedir {mediana}% dele"
            );
            // Encostar no fundo do cano é o preço de perguntar se ele cresceu, e
            // é cobrado numa janela por sondagem — nunca num regime. Cinco por
            // cento das ticas é o teto disso; acima daí a pergunta virou moradia.
            assert!(
                encostadas * 20 <= ocupacoes.len(),
                "num cano de {capacidade} bps a tela encostou no fundo em \
                 {encostadas} das {} ticas do regime",
                ocupacoes.len()
            );
        }
    }

    /// A segunda armadilha: **a faixa da voz não pode cortar duas vezes**.
    ///
    /// [`TetoDeVideo::teto`] já corta o teto pela metade em
    /// [`SignalBand::Degraded`]. Se a sonda também baixasse por causa da faixa, o
    /// vídeo levaria dois cortes pelo mesmo sintoma — e voltaria devagar de um
    /// buraco que ele mesmo cavou, porque a subida da sonda leva janelas e a
    /// faixa volta num `PersonState`.
    ///
    /// A prova: as mesmas janelas cheias, na faixa degradada, num cano largo.
    /// A estimativa tem de **subir do mesmo jeito**.
    #[test]
    fn a_sonda_que_lembra_nao_recomeca_do_palpite() {
        let lembrada = Sonda::partindo_de(12_480_000);
        assert_eq!(lembrada.estimativa(), 12_480_000);

        // E ela continua sendo uma sonda: um cano que encolheu a corrige na
        // primeira janela que doer, como corrigiria qualquer estimativa. Memória
        // que não cede a uma medida não é memória, é teimosia.
        let mut lembrada = lembrada;
        let mut cano = Cano::de(3_000_000);
        correr(
            &mut lembrada,
            &mut cano,
            SignalBand::Nominal,
            ticas(10),
            |_, _| {},
        );
        assert!(
            lembrada.estimativa() < 12_480_000,
            "o cano encolheu e a estimativa lembrada não desceu"
        );

        // A faixa que a escada admite continua valendo dos dois lados: memória
        // não é autorização para começar fora dela.
        assert_eq!(
            Sonda::partindo_de(u32::MAX).estimativa(),
            TETO_DA_ESTIMATIVA_BPS
        );
        assert_eq!(Sonda::partindo_de(1).estimativa(), PISO_DA_ESTIMATIVA_BPS);
    }

    #[test]
    fn a_faixa_da_voz_nao_corta_a_estimativa_uma_segunda_vez() {
        let mut nominal = Sonda::nova();
        let mut cano_nominal = Cano::de(8_000_000);
        correr(
            &mut nominal,
            &mut cano_nominal,
            SignalBand::Nominal,
            ticas(15),
            |_, _| {},
        );

        let mut degradada = Sonda::nova();
        let mut cano_degradado = Cano::de(8_000_000);
        correr(
            &mut degradada,
            &mut cano_degradado,
            SignalBand::Degraded,
            ticas(15),
            |_, _| {},
        );

        // **A faixa não pode DERRUBAR a estimativa** — é o que o cabeçalho
        // deste módulo escreve, e é a coisa que importa: se a sonda também
        // baixasse por causa da faixa, o vídeo levaria dois cortes pelo mesmo
        // sintoma e voltaria devagar de um buraco que ele mesmo cavou.
        assert!(
            degradada.estimativa() >= CAMINHO_DA_PROVA_BPS,
            "a faixa degradada derrubou a medida do caminho abaixo de onde ela começou"
        );

        // **E não pode LEVANTÁ-LA, que é o outro lado e é o que este teste
        // pedia por engano.** Aqui estava escrito que as duas estimativas têm de
        // ser iguais, e isso só é alcançável de um jeito: subindo em `Degraded`.
        // Subir ali é subir sem prova — o teto já vem cortado pela metade, então
        // a janela enche o próprio teto usando 30% do cano e diz «coube». Doze
        // janelas assim levam a estimativa a `TETO_DA_ESTIMATIVA_BPS` sem nunca
        // ter tocado a borda, e a volta para `Nominal` põe o teto em 60% de um
        // número inventado — a reserva da voz evaporando pela porta que a
        // terceira armadilha não vigiava.
        //
        // A igualdade passava por empate e não por acerto: com o teto da
        // estimativa em 10 Mbps os dois lados batiam nele. Subi-lo para 14
        // separou os dois e a igualdade caiu.
        assert!(
            degradada.estimativa() <= nominal.estimativa(),
            "a faixa degradada afirmou um caminho maior que o que a faixa nominal mediu"
        );

        // O que a faixa degradada **não** custa é permanente: voltando a
        // `Nominal`, a subida recomeça de onde parou e reencontra a borda. É
        // isto que «dois sinais independentes» quer dizer na prática.
        correr(
            &mut degradada,
            &mut cano_degradado,
            SignalBand::Nominal,
            ticas(15),
            |_, _| {},
        );
        assert_eq!(
            degradada.estimativa(),
            nominal.estimativa(),
            "depois de a faixa voltar, a medida tinha de reencontrar a mesma borda"
        );

        // E o corte da faixa continua existindo, no lugar dele: metade do teto,
        // pelas mãos de quem sempre o fez.
        assert_eq!(
            teto_de(degradada.estimativa(), SignalBand::Degraded).bps(),
            teto_de(degradada.estimativa(), SignalBand::Nominal).bps() / 2
        );
    }

    /// O caminho piorando derruba a estimativa, e cada sinal sozinho basta.
    ///
    /// Três sinais e não um, e o terceiro é o que chega a tempo: perda e evento
    /// de congestionamento só aparecem quando a fila do gargalo transbordou, e a
    /// fila que o spike mediu tem 262 ms — a voz já está atrasada um quarto de
    /// segundo quando o `quinn` conta o primeiro pacote perdido.
    #[test]
    fn cada_sinal_de_piora_sozinho_derruba_a_estimativa() {
        let inicio = Instant::now();
        let teto = teto_de(CAMINHO_DA_PROVA_BPS, SignalBand::Nominal);
        // Bytes de uma janela cheia a 1,2 Mbps de teto mais a voz.
        let cheios = u64::from(teto.bps() + VOZ_BPS) / 8;

        let piora = |quem: fn(&mut Transporte)| {
            let mut sonda = Sonda::nova();
            let mut fim = Transporte {
                bytes_enviados: 0,
                eventos_de_congestionamento: Some(0),
                pacotes_perdidos: Some(0),
                ida_e_volta: Some(IdaEVolta {
                    valor: Duration::from_millis(20),
                    amostras: 1,
                    em_voo: None,
                }),
            };
            let comeco = Amostra {
                transporte: fim,
                teto,
                faixa: SignalBand::Nominal,
            };
            assert_eq!(sonda.observar(inicio, &comeco), None);
            fim.bytes_enviados += cheios;
            quem(&mut fim);
            sonda.observar(
                inicio + Duration::from_secs(1),
                &Amostra {
                    transporte: fim,
                    teto,
                    faixa: SignalBand::Nominal,
                },
            );
            sonda.estimativa()
        };

        for (nome, sinal) in [
            (
                "eventos de congestionamento",
                (|t: &mut Transporte| {
                    t.eventos_de_congestionamento = t.eventos_de_congestionamento.map(|n| n + 1);
                }) as fn(&mut Transporte),
            ),
            ("pacotes perdidos", |t: &mut Transporte| {
                t.pacotes_perdidos = t.pacotes_perdidos.map(|n| n + 1);
            }),
            ("fila no gargalo", |t: &mut Transporte| {
                t.ida_e_volta = Some(IdaEVolta {
                    valor: Duration::from_millis(20) + FILA_TOLERADA * 2,
                    amostras: 2,
                    em_voo: None,
                });
            }),
        ] {
            let depois = piora(sinal);
            assert!(
                depois < CAMINHO_DA_PROVA_BPS,
                "{nome} cresceu e a sonda continuou dizendo {depois} bps"
            );
        }

        // E sem piora nenhuma, a mesma janela cheia **sobe**. Sem esta linha o
        // teste acima passaria com uma sonda que só sabe descer.
        let calma = piora(|_| {});
        assert!(calma > CAMINHO_DA_PROVA_BPS, "a janela calma não subiu");
    }

    /// A sonda não desce até apagar o vídeo pela porta dos fundos.
    ///
    /// §2: quem para é o teto, com [`crate::tela::MotivoDeParada`] enumerado,
    /// porque quem recebe a parada tem de poder escrever a frase na língua da
    /// pessoa. Uma sonda que zerasse a estimativa pararia a transmissão sem
    /// frase nenhuma.
    #[test]
    fn a_estimativa_tem_piso_e_teto_e_os_dois_tem_conta_atras() {
        let mut sonda = Sonda::nova();
        let mut cano = Cano::de(50_000);
        correr(
            &mut sonda,
            &mut cano,
            SignalBand::Nominal,
            ticas(120),
            |_, _| {},
        );

        assert_eq!(
            sonda.estimativa(),
            PISO_DA_ESTIMATIVA_BPS,
            "a sonda passou do piso num cano impossível"
        );
        // E o piso é exatamente onde a fração ainda compra o piso de banda: quem
        // para daqui para baixo é o teto, com nome.
        assert_eq!(
            teto_de(sonda.estimativa(), SignalBand::Nominal),
            Teto::Bps(PISO_DE_BANDA_BPS)
        );
        assert_eq!(
            teto_de(sonda.estimativa(), SignalBand::Degraded),
            Teto::Parado(crate::tela::MotivoDeParada::AbaixoDoPiso)
        );

        // E do outro lado: uma fibra não faz a escada subir para sempre.
        let mut larga = Sonda::nova();
        let mut fibra = Cano::de(900_000_000);
        correr(
            &mut larga,
            &mut fibra,
            SignalBand::Nominal,
            ticas(120),
            |_, _| {},
        );
        assert_eq!(larga.estimativa(), TETO_DA_ESTIMATIVA_BPS);
    }

    /// **Sem tempestade de quadro-chave.**
    ///
    /// Trocar de degrau custa um quadro-chave inteiro — 65 KiB e 446 ms do
    /// orçamento de 1200 kbps —, e `crate::video::Compartilhamento::refazer_com`
    /// avisa que *«um degrau que oscila entre dois valores queimaria o orçamento
    /// em quadros-chave e não mostraria tela nenhuma»*.
    ///
    /// Um cano que fica bem em cima de um limiar da escada é o caso adversário
    /// disto: 1,5 Mbps de cano dão 900 kbps de teto, que é exatamente
    /// [`crate::tela::TETO_ESTIMADO_PARA_720P_BPS`] — o degrau entre 540p e
    /// 720p. Com a estimativa balançando, cada janela seria uma troca.
    ///
    /// Três minutos de simulação são 150 janelas. **Medido: oito trocas** — as
    /// duas da subida inicial, as da medida que a corrigiu, e duas por sondagem
    /// (uma para perguntar, uma para voltar). Um quadro-chave a cada 22
    /// segundos são 24 kbps, abaixo do que o próprio encoder já descarta. Sem
    /// [`Sonda::limite_bps`] seriam ~150, uma por janela: confira por mutação,
    /// trocando `self.limite_bps = Some(...)` por `None` na metade de cima de
    /// [`Sonda::observar`].
    #[test]
    fn um_caminho_em_cima_de_um_limiar_nao_vira_tempestade_de_quadro_chave() {
        let mut sonda = Sonda::nova();
        // Oscilando de propósito em volta do limiar, para que nenhuma sorte de
        // arredondamento seja o que faz este teste passar.
        let mut cano = Cano::de(1_500_000);
        let mut trocas = 0_u32;
        let mut degrau = teto_de(sonda.estimativa(), SignalBand::Nominal).resolucao_estimada();

        let inicio = Instant::now();
        let faixa = SignalBand::Nominal;
        let mut teto = teto_de(sonda.estimativa(), faixa);
        for k in 1..=ticas(180) {
            // O cano respira: ±4% em volta do limiar, trocando a cada segundo.
            cano.capacidade_bps = if (k / 5) % 2 == 0 {
                1_440_000
            } else {
                1_560_000
            };
            let agora = inicio + TICA * k;
            cano.correr(teto.bps(), TICA);
            if sonda.observar(agora, &cano.amostra(teto, faixa)).is_some() {
                teto = teto_de(sonda.estimativa(), faixa);
                let novo = teto.resolucao_estimada();
                if novo != degrau {
                    trocas += 1;
                    degrau = novo;
                }
            }
        }

        assert!(
            trocas <= 10,
            "o degrau trocou {trocas} vezes em três minutos em cima do limiar; \
             cada troca custa um quadro-chave de 65 KiB"
        );
    }

    /// Uma janela apertada por **outra** perna do §5.1 não mede o nosso caminho.
    ///
    /// Seis pessoas assistindo apertam o teto pela subida de quem hospeda. A
    /// transmissão enche esse teto — ela enche o que tem —, e ler isso como «o
    /// meu cano tem tanto» faria a sala cheia estragar a medida da máquina de
    /// quem compartilha, e deixá-la estragada depois de todo mundo sair.
    #[test]
    fn a_perna_de_quem_hospeda_apertando_nao_ensina_nada_sobre_a_nossa() {
        let mut sonda = Sonda::nova();
        let inicio = Instant::now();
        let mut cano = Cano::de(8_000_000);

        // O teto de uma sala de seis, com a subida do anfitrião em 3 Mbps: 60%
        // de 3 Mbps divididos por seis são 300 kbps, muito abaixo dos 1200 que
        // a nossa perna daria.
        let teto = TetoDeVideo::com_caminho(sonda.estimativa())
            .com_caminho_de_quem_hospeda(3_000_000)
            .com_espectadores(6)
            .teto(SignalBand::Nominal);
        assert_eq!(teto, Teto::Bps(300_000));

        for k in 1..=ticas(60) {
            cano.correr(teto.bps(), TICA);
            sonda.observar(inicio + TICA * k, &cano.amostra(teto, SignalBand::Nominal));
        }

        assert_eq!(
            sonda.estimativa(),
            CAMINHO_DA_PROVA_BPS,
            "a sala cheia virou uma medida do caminho de quem compartilha"
        );
    }

    /// **Um achado sobre o produto, e não sobre a sonda: sem o `HostUplink`, o
    /// que esta máquina mede não levanta o teto.**
    ///
    /// `TetoDeVideo::com_caminho` deixa a perna de quem hospeda no cano das
    /// provas, e `Room::teto_de_video` só a troca quando o servidor declara a
    /// própria subida — `crate::tela::caminho_no_fio` do `seele-server` manda
    /// **zero** quando o operador não declarou nada, e zero é ausência. Então o
    /// `min` do §5.1 trava o teto em 1200 kbps por mais que esta ponta descubra
    /// que tem fibra.
    ///
    /// Isto **não** é um defeito desta sonda, e consertá-lo daqui seria
    /// inventar a perna mais cara da conta — o §5.1 chama isso de o defeito mais
    /// caro daquela seção. Fica escrito como teste para que a próxima pessoa
    /// saiba onde procurar: quem quiser 1080p numa fibra tem de fazer o servidor
    /// declarar a subida dele, e a sonda serve para os dois casos que sobram —
    /// **descer** quando o cano desta máquina é pior que a suposição, e subir
    /// quando o servidor declarou.
    #[test]
    fn sem_a_subida_do_server_a_medida_desta_maquina_nao_levanta_o_teto() {
        let mut sonda = Sonda::nova();
        let mut cano = Cano::de(50_000_000);
        let inicio = Instant::now();
        let faixa = SignalBand::Nominal;
        // Sem `com_caminho_de_quem_hospeda`: é o teto de uma sala cujo servidor
        // não declarou nada, que é o padrão de fábrica.
        let mut teto = TetoDeVideo::com_caminho(sonda.estimativa()).teto(faixa);
        for k in 1..=ticas(60) {
            cano.correr(teto.bps(), TICA);
            if sonda
                .observar(inicio + TICA * k, &cano.amostra(teto, faixa))
                .is_some()
            {
                teto = TetoDeVideo::com_caminho(sonda.estimativa()).teto(faixa);
            }
        }

        assert_eq!(
            teto,
            Teto::Bps(1_200_000),
            "a perna de quem hospeda deixou de ser o cano das provas sozinha"
        );
        // E a sonda não vira esperança por causa disso: sem janela que teste
        // este cano, ela para logo acima do que a última janela sustenta.
        assert!(
            sonda.estimativa() < 3_000_000,
            "a estimativa subiu sem evidência até {} bps",
            sonda.estimativa()
        );
    }

    /// Uma conexão nova zera os contadores do `quinn`, e não o que se aprendeu.
    #[test]
    fn a_conexao_nova_fecha_a_janela_e_guarda_a_estimativa() {
        let mut sonda = Sonda::nova();
        let mut cano = Cano::de(8_000_000);
        correr(
            &mut sonda,
            &mut cano,
            SignalBand::Nominal,
            ticas(10),
            |_, _| {},
        );
        let aprendido = sonda.estimativa();
        assert!(aprendido > CAMINHO_DA_PROVA_BPS);

        sonda.esquecer_a_conexao();
        assert_eq!(sonda.estimativa(), aprendido);

        // E a janela seguinte não lê a diferença contra os contadores da conexão
        // que morreu: os do `quinn` novo começam em zero, e a subtração daria
        // uma janela absurda.
        let inicio = Instant::now();
        let teto = teto_de(aprendido, SignalBand::Nominal);
        let mut novo = Cano::de(8_000_000);
        assert_eq!(
            sonda.observar(inicio, &novo.amostra(teto, SignalBand::Nominal)),
            None
        );
        novo.correr(teto.bps(), Duration::from_secs(1));
        let depois = sonda.observar(
            inicio + Duration::from_secs(1),
            &novo.amostra(teto, SignalBand::Nominal),
        );
        assert!(
            depois.is_none_or(|bps| bps <= TETO_DA_ESTIMATIVA_BPS),
            "a conexão nova produziu uma janela impossível"
        );
    }

    /// O vídeo parado não é um cano vazio: é ausência de amostra.
    #[test]
    fn um_teto_parado_nao_e_uma_medida_de_caminho() {
        let mut sonda = Sonda::nova();
        let inicio = Instant::now();
        let parado = Teto::Parado(crate::tela::MotivoDeParada::SinalCritico);
        let transporte = Cano::de(8_000_000).transporte;
        let amostra = Amostra {
            transporte,
            teto: parado,
            faixa: SignalBand::Critical,
        };

        for k in 1..=ticas(30) {
            assert_eq!(sonda.observar(inicio + TICA * k, &amostra), None);
        }
        assert_eq!(sonda.estimativa(), CAMINHO_DA_PROVA_BPS);
    }

    // ------------------------------------------------- telemetria da 0.17

    /// **Sem telemetria nenhuma, a sonda não sobe e não fica acima do cano das
    /// provas.** É a regressão que a revisão da 0.17 achou: a perda, o
    /// congestionamento e a ida e volta que o enlace não mede viravam zero, e
    /// zero se lia como caminho limpo e curto — a estimativa subia, com o passo
    /// largo do caminho curto, até o teto, em cima da reserva da voz.
    #[test]
    fn sem_telemetria_a_estimativa_nao_sobe_e_volta_ao_cano_das_provas() {
        // Partindo de uma medida lembrada alta, o caso que mais esconde o
        // defeito: sem medida, ela não pode continuar valendo.
        let mut sonda = Sonda::partindo_de(12_000_000);
        let mut cano = Cano::cego(30_000_000);
        let mut maior = 0;
        correr(
            &mut sonda,
            &mut cano,
            SignalBand::Nominal,
            ticas(60),
            |_, teto| {
                maior = maior.max(teto.bps());
            },
        );
        assert_eq!(
            sonda.estimativa(),
            TETO_SEM_MEDIDA_BPS,
            "sem medida de dor a estimativa ficou em {}, e não no cano das provas",
            sonda.estimativa()
        );
        assert!(
            sonda.janelas_sem_medida() >= 50,
            "a sonda não contou as janelas sem medida: {}",
            sonda.janelas_sem_medida()
        );
        assert_eq!(
            sonda.limite_bps(),
            None,
            "sem medida, a sonda inventou uma borda"
        );
        let _ = maior;
        // E nada disto é gravável como medida: a semente não volta ao disco.
        assert_eq!(
            sonda.estimativa_medida(),
            None,
            "uma sonda que nunca mediu ofereceu a semente como medida"
        );
    }

    /// Só a sonda que mediu oferece o que gravar; a que só tem semente, não.
    #[test]
    fn so_a_estimativa_medida_vai_ao_disco() {
        assert_eq!(Sonda::partindo_de(6_000_000).estimativa_medida(), None);
        assert_eq!(Sonda::nova().estimativa_medida(), None);
        let mut sonda = Sonda::nova();
        let mut cano = Cano::do_enlace(8_000_000);
        correr(
            &mut sonda,
            &mut cano,
            SignalBand::Nominal,
            ticas(5),
            |_, _| {},
        );
        assert_eq!(sonda.estimativa_medida(), Some(sonda.estimativa()));
    }

    /// **Sem ida e volta, e sem perda contada**: o enlace da 0.17 antes de o
    /// `Ping` da sonda responder. O mesmo veredito: não sabe, não sobe.
    #[test]
    fn sem_ida_e_volta_no_enlace_a_estimativa_nao_cresce() {
        let mut sonda = Sonda::nova();
        let mut cano = Cano::cego(8_000_000);
        correr(
            &mut sonda,
            &mut cano,
            SignalBand::Nominal,
            ticas(30),
            |_, _| {},
        );
        assert!(
            sonda.estimativa() <= CAMINHO_DA_PROVA_BPS,
            "sem ida e volta a estimativa cresceu para {}",
            sonda.estimativa()
        );
        // E uma ida e volta que não anda (a mesma medida velha, sem resposta
        // nova) também não é evidência.
        let mut sonda = Sonda::nova();
        let mut cano = Cano::do_enlace(8_000_000);
        let inicio = Instant::now();
        let teto = teto_de(sonda.estimativa(), SignalBand::Nominal);
        for k in 1..=ticas(30) {
            cano.correr(teto.bps(), TICA);
            if let Some(medida) = cano.transporte.ida_e_volta.as_mut() {
                medida.amostras = 1;
            }
            sonda.observar(inicio + TICA * k, &cano.amostra(teto, SignalBand::Nominal));
        }
        assert!(
            sonda.estimativa() <= CAMINHO_DA_PROVA_BPS,
            "uma ida e volta velha valeu como evidência: {}",
            sonda.estimativa()
        );
    }

    /// Com a ida e volta do `Ping` da sonda, e sem perda nem congestionamento,
    /// a sonda volta a medir: sobe por evidência num cano largo.
    #[test]
    fn com_a_ida_e_volta_da_sonda_a_estimativa_sobe_por_evidencia() {
        let mut sonda = Sonda::nova();
        let mut cano = Cano::do_enlace(8_000_000);
        correr(
            &mut sonda,
            &mut cano,
            SignalBand::Nominal,
            ticas(15),
            |_, _| {},
        );
        assert!(
            sonda.estimativa() > CAMINHO_DA_PROVA_BPS,
            "com ida e volta fresca e sem fila, a estimativa não subiu: {}",
            sonda.estimativa()
        );
        assert_eq!(sonda.janelas_sem_medida(), 0);
    }

    /// Um `Ping` que não volta e já passou da fila tolerada é dor medida.
    #[test]
    fn um_ping_em_voo_alem_da_fila_e_dor() {
        // A mesma história duas vezes; só muda o `Ping` preso. A janela sem
        // resposta nova cai no teto sem medida nas duas — o que se compara é
        // a dor, que só a do `Ping` preso tem (a [`Sonda`] desce abaixo do teto
        // sem medida).
        let historia = |preso: Option<Duration>| {
            let mut sonda = Sonda::nova();
            let mut cano = Cano::do_enlace(8_000_000);
            let inicio = Instant::now();
            let teto = teto_de(sonda.estimativa(), SignalBand::Nominal);
            // Janelas limpas para a base.
            for k in 1..=ticas(2) {
                cano.correr(teto.bps(), TICA);
                sonda.observar(inicio + TICA * k, &cano.amostra(teto, SignalBand::Nominal));
            }
            // E as seguintes sem resposta nova, cheias, com ou sem um `Ping` no ar.
            for k in (ticas(2) + 1)..=ticas(4) {
                cano.transporte.bytes_enviados += u64::from(teto.bps() + VOZ_BPS) / 8 / 10;
                if let Some(medida) = cano.transporte.ida_e_volta.as_mut() {
                    medida.em_voo = preso;
                }
                sonda.observar(inicio + TICA * k, &cano.amostra(teto, SignalBand::Nominal));
            }
            sonda.estimativa()
        };
        let sem_ping_preso = historia(None);
        let com_ping_preso = historia(Some(Duration::from_millis(500)));
        assert!(
            com_ping_preso < sem_ping_preso,
            "o Ping preso na fila não contou como dor: {com_ping_preso} com, {sem_ping_preso} sem"
        );
    }

    /// **Tela, anexo e voz num gargalo controlado, com a telemetria da 0.17.**
    ///
    /// Um cano de 3 Mbps; aos 20 s um anexo passa a tomar 1 Mbps dele. A sonda
    /// só tem a ida e volta do `Ping`. O que se prova:
    /// - nenhuma amostra do enlace afirma perda nem congestionamento (são
    ///   `None`: a sonda não anuncia «0 perda medida»);
    /// - a sonda não passa do cano sem evidência, e volta a caber quando o
    ///   anexo entra;
    /// - a voz se preserva: no fim, com o anexo correndo, a fila da voz é zero
    ///   em quase todas as janelas.
    #[test]
    fn tela_anexo_e_voz_num_gargalo_com_a_telemetria_da_0_17() {
        let mut sonda = Sonda::nova();
        let capacidade = 3_000_000;
        let mut cano = Cano::do_enlace(capacidade);
        let anexo_bps = 1_000_000;
        let inicio = Instant::now();
        let mut teto = teto_de(sonda.estimativa(), SignalBand::Nominal);
        let mut com_fila_no_fim = 0;
        let fim = ticas(60);
        for k in 1..=fim {
            let agora = inicio + TICA * k;
            let anexo = if k > ticas(20) { anexo_bps } else { 0 };
            let oferta = teto.bps() + anexo;
            cano.correr(oferta, TICA);
            assert_eq!(cano.transporte.pacotes_perdidos, None);
            assert_eq!(cano.transporte.eventos_de_congestionamento, None);
            if sonda
                .observar(agora, &cano.amostra(teto, SignalBand::Nominal))
                .is_some()
            {
                teto = teto_de(sonda.estimativa(), SignalBand::Nominal);
            }
            assert!(
                sonda.estimativa() <= TETO_DA_ESTIMATIVA_BPS,
                "a estimativa passou do teto"
            );
            if k > fim - ticas(20) && cano.atraso_da_voz(teto.bps() + anexo) > Duration::ZERO {
                com_fila_no_fim += 1;
            }
        }
        assert!(
            com_fila_no_fim <= ticas(2),
            "com o anexo correndo, a voz pegou fila em {com_fila_no_fim} de {} ticas do fim",
            ticas(20)
        );
        assert!(
            u64::from(teto.bps()) + u64::from(anexo_bps) + u64::from(VOZ_BPS)
                <= u64::from(capacidade),
            "no fim a tela e o anexo não cabem no cano com a voz: teto {}",
            teto.bps()
        );
    }
}
