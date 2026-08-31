//! Quanto da voz de alguém não chega, medido por quem recebe.
//!
//! # Por que aqui, e por que uma medida nova
//!
//! O `Telemetry::loss_fraction` que esta sessão já manda vem de `stats.path` do
//! quinn, e não serve para comandar um encoder por duas razões que não se
//! corrigem uma à outra:
//!
//! - **é a direção errada** — mede o que o servidor mandou e se perdeu, ou seja
//!   o *download* de quem escuta. Encolher o microfone de alguém porque o
//!   download dele está ruim é o oposto do que `specs/03-audio.md` pede;
//! - **é cumulativo desde o início da conexão** — uma razão monótona que, uma
//!   vez subida, só decai assintoticamente. «Sobe de volta gradualmente» é
//!   aritmeticamente impossível a partir dela.
//!
//! O número certo já passa debaixo do nariz do servidor: a `VoiceRoom` decodifica
//! o cabeçalho de mídia para conferir que o `ssrc` não foi forjado, e com ele
//! `seq` vem de graça.
//!
//! # Por que lacuna de `seq` é perda, e nunca silêncio
//!
//! Porque o DTX **não** incrementa `seq`. O carimbo de tempo conta amostras
//! decorridas e a sequência conta pacotes emitidos — é a separação que M1.9
//! introduziu, e está escrita no `seele-core::voice`:
//!
//! > The timestamp counts elapsed samples whether or not anything goes out; the
//! > sequence counts only what does.
//!
//! Quem cala não produz lacuna: produz ausência de pacote com `seq` parado, e o
//! pacote seguinte continua de onde parou. Toda lacuna é, então, um pacote que
//! saiu e não chegou. Não há heurística a calibrar.
//!
//! # O que este módulo não faz
//!
//! Não toca no payload. `specs/08-seguranca.md` proíbe, e a promessa de que E2EE
//! é incremento e não reescrita depende disso.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Sobre quanto tempo a fração é calculada.
///
/// Cinco segundos, e o número é uma troca de verdade: curto demais e a malha do
/// cliente persegue ruído; longo demais e ela reage tarde. A 50 quadros por
/// segundo são ~250 pacotes, e 5% de 250 são 12 — amostra grande o bastante para
/// o limiar da spec não ser decidido por dois pacotes. Abaixo disso, «5%» vira
/// uma frase sobre meia dúzia deles.
pub const JANELA: Duration = Duration::from_secs(5);

/// Quantos pacotes a janela precisa ter antes de a fração significar algo.
///
/// Um segundo de fala. Abaixo disso a divisão tem denominador pequeno demais
/// para o limiar de 5% distinguir rede de acaso, e [`PerdaDeSubida::fracao`]
/// responde «não sei» em vez de um número que seria ruído com aparência de
/// medida.
pub const MINIMO_DE_PACOTES: u32 = 50;

/// Uma lacuna maior que isto é tratada como recomeço, e não como perda.
///
/// O `seq` é um `u16` e dá a volta. Uma diferença enorme é, na prática, uma
/// conexão que recomeçou ou um `seq` que voltou — nunca mil pacotes perdidos de
/// uma vez, porque a essa altura não haveria conversa para medir. Contá-la como
/// perda enfiaria um degrau falso na janela inteira.
const MAIOR_LACUNA_CRIVEL: u32 = 1_000;

/// Um pedaço da janela: quando, quantos se esperavam, quantos chegaram.
#[derive(Debug, Clone, Copy)]
struct Amostra {
    quando: Instant,
    esperados: u32,
    chegados: u32,
}

/// A perda de subida de um `ssrc`, sobre uma janela deslizante.
#[derive(Debug)]
pub struct PerdaDeSubida {
    janela: Duration,
    amostras: VecDeque<Amostra>,
    ultimo_seq: Option<u16>,
}

impl Default for PerdaDeSubida {
    fn default() -> Self {
        Self::nova()
    }
}

impl PerdaDeSubida {
    /// Um estimador com a janela do projeto.
    #[must_use]
    pub fn nova() -> Self {
        Self::com_janela(JANELA)
    }

    /// Um estimador com a janela escolhida.
    #[must_use]
    pub fn com_janela(janela: Duration) -> Self {
        Self {
            janela,
            amostras: VecDeque::new(),
            ultimo_seq: None,
        }
    }

    /// Um pacote chegou com este `seq`.
    pub fn chegou(&mut self, seq: u16, agora: Instant) {
        let Some(anterior) = self.ultimo_seq else {
            // O primeiro pacote não tem antecessor com que comparar: ele
            // estabelece o ponto de partida e não afirma nada sobre perda.
            self.ultimo_seq = Some(seq);
            return;
        };

        let avanco = u32::from(seq.wrapping_sub(anterior));
        if avanco == 0 {
            // Duplicata, ou um pacote reordenado que voltou ao mesmo número.
            // Não é perda e não é chegada nova.
            return;
        }
        if avanco > MAIOR_LACUNA_CRIVEL {
            // Recomeço. Ver `MAIOR_LACUNA_CRIVEL`.
            self.ultimo_seq = Some(seq);
            return;
        }

        self.ultimo_seq = Some(seq);
        self.amostras.push_back(Amostra {
            quando: agora,
            esperados: avanco,
            chegados: 1,
        });
    }

    /// A fração perdida na janela, ou `None` enquanto não há amostra bastante.
    ///
    /// `&mut self` porque a leitura é o momento natural de descartar o que saiu
    /// da janela: um estimador de alguém que parou de falar não tem por que ser
    /// varrido por um relógio próprio.
    pub fn fracao(&mut self, agora: Instant) -> Option<f32> {
        while let Some(primeira) = self.amostras.front() {
            if agora.duration_since(primeira.quando) > self.janela {
                self.amostras.pop_front();
            } else {
                break;
            }
        }

        let mut esperados = 0_u32;
        let mut chegados = 0_u32;
        for amostra in &self.amostras {
            esperados = esperados.saturating_add(amostra.esperados);
            chegados = chegados.saturating_add(amostra.chegados);
        }

        if esperados < MINIMO_DE_PACOTES {
            return None;
        }
        let perdidos = esperados.saturating_sub(chegados);
        #[allow(
            clippy::cast_precision_loss,
            reason = "contagens de uma janela de cinco segundos; muito abaixo do que f32 representa exatamente"
        )]
        Some(perdidos as f32 / esperados as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::{PerdaDeSubida, MINIMO_DE_PACOTES};
    use std::time::{Duration, Instant};

    /// Manda `quantos` pacotes seguidos, um a cada 20 ms, a partir de `primeiro`.
    fn seguidos(perda: &mut PerdaDeSubida, inicio: Instant, primeiro: u16, quantos: u16) {
        for passo in 0..quantos {
            perda.chegou(
                primeiro.wrapping_add(passo),
                inicio + Duration::from_millis(u64::from(passo) * 20),
            );
        }
    }

    #[test]
    fn uma_sequencia_sem_lacuna_nao_perde_nada() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, 0, 200);
        assert_eq!(perda.fracao(inicio + Duration::from_secs(4)), Some(0.0));
    }

    /// O teste que mais importa deste módulo.
    ///
    /// Silêncio de DTX não é perda. Quem cala não manda pacote e **não**
    /// incrementa `seq`, então a sequência continua de onde parou depois de uma
    /// pausa longa no relógio. Se isto virasse perda, a malha do cliente
    /// derrubaria o bitrate de quem simplesmente parou de falar — e o faria
    /// justamente nas conversas mais calmas.
    #[test]
    fn silencio_de_dtx_nao_conta_como_perda() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));

        // Um segundo de fala.
        seguidos(&mut perda, inicio, 0, 50);
        // Trinta segundos calado: nenhum pacote, e `seq` parado em 49. Depois, a
        // fala recomeça exatamente em 50.
        let volta = inicio + Duration::from_secs(30);
        seguidos(&mut perda, volta, 50, 50);

        assert_eq!(
            perda.fracao(volta + Duration::from_secs(1)),
            Some(0.0),
            "o silêncio do DTX foi contado como perda"
        );
    }

    #[test]
    fn uma_lacuna_de_seq_e_perda() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));

        // Cem pacotes, com o de número 50 faltando.
        for passo in 0..100_u16 {
            if passo == 50 {
                continue;
            }
            perda.chegou(passo, inicio + Duration::from_millis(u64::from(passo) * 20));
        }

        let fracao = perda
            .fracao(inicio + Duration::from_secs(2))
            .expect("cem pacotes passam do mínimo");
        assert!(
            (fracao - 0.01).abs() < 0.005,
            "um perdido em cem deu {fracao}"
        );
    }

    /// Dez pacotes são poucos, e o teste acima só prova algo se continuarem
    /// sendo. Em tempo de compilação, porque a relação entre o número do teste e
    /// a constante do módulo não é uma condição de execução — é uma premissa.
    const _: () = assert!(
        MINIMO_DE_PACOTES > 10,
        "o mínimo caiu para dez ou menos, e o teste abaixo deixou de provar o que prova"
    );

    #[test]
    fn abaixo_do_minimo_a_resposta_e_nao_sei() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, 0, 10);
        assert_eq!(
            perda.fracao(inicio + Duration::from_millis(300)),
            None,
            "afirmou uma fração com dez pacotes de amostra"
        );
    }

    /// A propriedade que o número cumulativo de hoje não tem, e a razão de
    /// existir medida nova: quando o enlace melhora, a fração **desce**.
    #[test]
    fn a_janela_desliza_e_a_fracao_volta_a_cair() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(5));

        // Dois segundos ruins: metade dos pacotes some.
        for passo in 0..100_u16 {
            if passo % 2 == 0 {
                continue;
            }
            perda.chegou(passo, inicio + Duration::from_millis(u64::from(passo) * 20));
        }
        let ruim = perda
            .fracao(inicio + Duration::from_secs(2))
            .expect("cem pacotes");
        assert!(ruim > 0.4, "a fração ruim deu {ruim}");

        // Seis segundos depois, tudo limpo. O trecho ruim saiu da janela.
        //
        // A sequência continua em 100, e não salta: `seq` só anda quando um
        // pacote **sai**, então um salto aqui significaria que os pacotes do
        // meio saíram e se perderam — o que seria perda de verdade, e não a
        // trégua que este teste quer encenar. A primeira redação deste teste
        // saltava para 200 e reprovava com razão.
        let limpo = inicio + Duration::from_secs(8);
        seguidos(&mut perda, limpo, 100, 200);
        let bom = perda
            .fracao(limpo + Duration::from_secs(4))
            .expect("duzentos pacotes");
        assert_eq!(bom, 0.0, "o trecho ruim não saiu da janela");
    }

    #[test]
    fn duplicata_nao_conta() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, 0, 100);
        let antes = perda.fracao(inicio + Duration::from_secs(2));

        perda.chegou(99, inicio + Duration::from_secs(2));
        let depois = perda.fracao(inicio + Duration::from_secs(2));
        assert_eq!(antes, depois, "uma duplicata mexeu na medida");
    }

    /// `seq` é `u16` e dá a volta no meio de uma chamada longa. A volta é o caso
    /// normal, e não um recomeço: dezesseis bits a cinquenta pacotes por segundo
    /// dão a volta a cada 22 minutos.
    #[test]
    fn a_volta_do_u16_e_continuidade_e_nao_lacuna() {
        let inicio = Instant::now();
        let mut perda = PerdaDeSubida::com_janela(Duration::from_secs(60));
        seguidos(&mut perda, inicio, u16::MAX - 49, 50);
        seguidos(&mut perda, inicio + Duration::from_secs(1), 0, 50);
        assert_eq!(
            perda.fracao(inicio + Duration::from_secs(2)),
            Some(0.0),
            "a volta do contador foi lida como perda"
        );
    }
}
