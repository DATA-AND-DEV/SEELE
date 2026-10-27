//! O envelope de cada mensagem num canal que não é a voz (contrato D4).
//!
//! `tag(1) ‖ corpo`:
//!
//! | tag | o quê | corpo |
//! |---|---|---|
//! | `0x00` | dados | 1 a [`crate::MAX_DADOS`] bytes; concatenados formam o fluxo |
//! | `0x01` | fim | vazio |
//! | `0x02` | corte | código u32 big-endian |
//! | `0x03` | apresentação | 0 a [`crate::MAX_APRESENTACAO`] bytes; só no `ctl`, só do anfitrião, só como a primeira |
//! | `0x04` | crédito | limite u64 big-endian: o total de bytes de dados que quem escreve neste canal pode ter mandado |
//!
//! # O crédito (`0x04`), a contrapressão por canal
//!
//! O SCTP não tem controle de fluxo por canal, e o str0m lê todo canal assim que
//! os dados chegam. Sem crédito, quem lê devagar só podia cortar quem escreve
//! depressa (a fila de [`crate::MAX_FILA_DE_ENTRADA`]). Com ele, quem lê anuncia
//! um **limite absoluto**: «você pode ter mandado, no total desde a abertura, até
//! N bytes de dados». Quem escreve para nesse limite e espera o próximo. Quem lê
//! anuncia `consumidos + MAX_FILA_DE_ENTRADA` ao abrir e depois de consumir um
//! quarto da janela. Absoluto, e não incremental, para uma corrida entre o
//! anúncio e os dados nunca contar o mesmo byte duas vezes.
//!
//! Só os corpos de dados (`0x00`) contam; fim, corte, apresentação e o próprio
//! crédito nunca esperam crédito.
//!
//! Tudo o que não for exatamente isto é violação, e quem lê corta o canal com
//! [`crate::CORTE_VIOLACAO`]. Puro, sem E/S: o mesmo formato que o navegador
//! escreve em JavaScript, provado pelos testes daqui.

/// Uma mensagem lida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mensagem<'a> {
    /// Bytes do fluxo.
    Dados(&'a [u8]),
    /// O fim do fluxo neste sentido.
    Fim,
    /// O fluxo foi cortado, com um código.
    Corte(u32),
    /// A apresentação do anfitrião.
    Apresentacao(&'a [u8]),
    /// O limite de dados que quem lê este canal aceita (total desde a abertura).
    Credito(u64),
}

/// Tag dos dados.
pub const TAG_DADOS: u8 = 0x00;
/// Tag do fim.
pub const TAG_FIM: u8 = 0x01;
/// Tag do corte.
pub const TAG_CORTE: u8 = 0x02;
/// Tag da apresentação.
pub const TAG_APRESENTACAO: u8 = 0x03;
/// Tag do crédito.
pub const TAG_CREDITO: u8 = 0x04;

/// Por que uma mensagem não é envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Violacao {
    /// Mensagem vazia: nem a tag veio.
    #[error("mensagem sem tag")]
    Vazia,
    /// Tag que o enlace não conhece.
    #[error("tag desconhecida: {0:#04x}")]
    TagDesconhecida(u8),
    /// Corpo de tamanho errado para a tag.
    #[error("corpo de {tamanho} bytes para a tag {tag:#04x}")]
    Tamanho {
        /// A tag.
        tag: u8,
        /// O tamanho do corpo que veio.
        tamanho: usize,
    },
}

/// Lê uma mensagem.
///
/// # Errors
///
/// [`Violacao`] para qualquer coisa fora da tabela do módulo.
pub fn ler(bytes: &[u8]) -> Result<Mensagem<'_>, Violacao> {
    let (&tag, corpo) = bytes.split_first().ok_or(Violacao::Vazia)?;
    let tamanho = corpo.len();
    let errado = Violacao::Tamanho { tag, tamanho };
    match tag {
        TAG_DADOS if (1..=crate::MAX_DADOS).contains(&tamanho) => Ok(Mensagem::Dados(corpo)),
        TAG_FIM if tamanho == 0 => Ok(Mensagem::Fim),
        TAG_CORTE => {
            let codigo: [u8; 4] = corpo.try_into().map_err(|_| errado)?;
            Ok(Mensagem::Corte(u32::from_be_bytes(codigo)))
        }
        TAG_APRESENTACAO if tamanho <= crate::MAX_APRESENTACAO => Ok(Mensagem::Apresentacao(corpo)),
        TAG_CREDITO => {
            let limite: [u8; 8] = corpo.try_into().map_err(|_| errado)?;
            Ok(Mensagem::Credito(u64::from_be_bytes(limite)))
        }
        TAG_DADOS | TAG_FIM | TAG_APRESENTACAO => Err(errado),
        outra => Err(Violacao::TagDesconhecida(outra)),
    }
}

/// Uma mensagem de dados. O chamador garante `1..=MAX_DADOS` bytes.
#[must_use]
pub fn dados(corpo: &[u8]) -> Vec<u8> {
    let mut saida = Vec::with_capacity(corpo.len() + 1);
    saida.push(TAG_DADOS);
    saida.extend_from_slice(corpo);
    saida
}

/// A mensagem de fim.
#[must_use]
pub fn fim() -> Vec<u8> {
    vec![TAG_FIM]
}

/// A mensagem de corte.
#[must_use]
pub fn corte(codigo: u32) -> Vec<u8> {
    let mut saida = vec![TAG_CORTE];
    saida.extend_from_slice(&codigo.to_be_bytes());
    saida
}

/// A mensagem de crédito: quem escreve pode ter mandado até `limite` bytes de dados.
#[must_use]
pub fn credito(limite: u64) -> Vec<u8> {
    let mut saida = vec![TAG_CREDITO];
    saida.extend_from_slice(&limite.to_be_bytes());
    saida
}

/// A mensagem de apresentação. O chamador garante `..=MAX_APRESENTACAO`.
#[must_use]
pub fn apresentacao(corpo: &[u8]) -> Vec<u8> {
    let mut saida = Vec::with_capacity(corpo.len() + 1);
    saida.push(TAG_APRESENTACAO);
    saida.extend_from_slice(corpo);
    saida
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_mensagem_volta_igual() {
        assert_eq!(ler(&dados(b"abc")), Ok(Mensagem::Dados(b"abc")));
        assert_eq!(ler(&fim()), Ok(Mensagem::Fim));
        assert_eq!(ler(&corte(0xDEAD_BEEF)), Ok(Mensagem::Corte(0xDEAD_BEEF)));
        assert_eq!(ler(&apresentacao(b"")), Ok(Mensagem::Apresentacao(b"")));
        assert_eq!(ler(&credito(1 << 40)), Ok(Mensagem::Credito(1 << 40)));
    }

    #[test]
    fn os_bytes_sao_os_do_contrato() {
        // O navegador escreve estes mesmos bytes à mão: eles ficam presos aqui.
        assert_eq!(dados(b"\x07"), vec![0x00, 0x07]);
        assert_eq!(fim(), vec![0x01]);
        assert_eq!(corte(2), vec![0x02, 0, 0, 0, 2]);
        assert_eq!(apresentacao(b"\xAA"), vec![0x03, 0xAA]);
        assert_eq!(credito(0x0102), vec![0x04, 0, 0, 0, 0, 0, 0, 0x01, 0x02]);
    }

    #[test]
    fn o_que_nao_e_envelope_e_recusado() {
        assert_eq!(ler(&[]), Err(Violacao::Vazia));
        assert_eq!(ler(&[0x00]), Err(Violacao::Tamanho { tag: 0, tamanho: 0 }));
        assert_eq!(
            ler(&[0x01, 0]),
            Err(Violacao::Tamanho { tag: 1, tamanho: 1 })
        );
        assert_eq!(
            ler(&[0x02, 0, 0, 0]),
            Err(Violacao::Tamanho { tag: 2, tamanho: 3 })
        );
        assert_eq!(
            ler(&[0x04, 1, 2]),
            Err(Violacao::Tamanho { tag: 4, tamanho: 2 })
        );
        assert_eq!(ler(&[0x05]), Err(Violacao::TagDesconhecida(5)));
        let grande = vec![0_u8; crate::MAX_DADOS + 2];
        assert!(ler(&grande).is_err());
        let mut apresentacao_grande = vec![TAG_APRESENTACAO];
        apresentacao_grande.extend(std::iter::repeat_n(0, crate::MAX_APRESENTACAO + 1));
        assert!(ler(&apresentacao_grande).is_err());
    }
}
