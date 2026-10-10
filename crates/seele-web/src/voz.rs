//! A voz pela fronteira (Tarefa 3.3 da F4, lote A): o JavaScript move bytes e
//! alças, e a conta é a do desktop (`seele-voz`).
//!
//! **O que manda.** O relógio de mídia ([`RelogioDeMidia`]) e o cabeçalho
//! (`seele_proto::media`). A página diz que um quadro de 20 ms passou, pede o
//! carimbo de cada quadro que o portão soltou, e entrega o Opus de cada um com
//! o carimbo dele: a sequência e o cabeçalho são daqui, na mesma ordem do laço
//! do desktop.
//!
//! **O que recebe.** O cabeçalho lido aqui, e a [`TabelaDeFontes`] do desktop,
//! que guarda para cada fonte uma **alça** opaca. A página não conhece SSRC
//! nenhum: «uma casca que sabe nomear um `ssrc` já tem lógica dentro» (ADR
//! 0057).

use seele_proto::media::{MediaError, MediaHeader, HEADER_LEN, MEDIA_HEADER_VERSION};
use seele_voz::fontes::TabelaDeFontes;
use seele_voz::relogio::{carimbo_do_quadro, RelogioDeMidia};

/// A voz que sai.
#[derive(Debug)]
pub struct Envio {
    ssrc: u32,
    relogio: RelogioDeMidia,
}

impl Envio {
    /// O envio de quem o servidor chamou de `ssrc`, com o relógio em zero.
    #[must_use]
    pub const fn novo(ssrc: u32) -> Self {
        Self::de(ssrc, 0, 0)
    }

    /// O envio de `ssrc` com o relógio continuando de `seq` e `carimbo`.
    #[must_use]
    pub const fn de(ssrc: u32, seq: u16, carimbo: u32) -> Self {
        Self {
            ssrc,
            relogio: RelogioDeMidia::de(seq, carimbo),
        }
    }

    /// Um quadro de 20 ms passou, saia algo dele ou não. Devolve o carimbo de
    /// agora.
    pub fn quadro_passou(&mut self) -> u32 {
        self.relogio.quadro_passou()
    }

    /// O carimbo do quadro `indice` de `quantos` que o portão acabou de soltar.
    #[must_use]
    pub fn carimbo_do_quadro(&self, quantos: usize, indice: usize) -> u32 {
        carimbo_do_quadro(self.relogio.carimbo(), quantos, indice)
    }

    /// O datagrama de um quadro de Opus, com o carimbo dele.
    ///
    /// **Na ordem do laço do desktop:** um Opus vazio (o DTX) não sai e não
    /// anda a sequência; qualquer outro anda a sequência, e só então vira
    /// cabeçalho e datagrama.
    ///
    /// # Errors
    ///
    /// [`MediaError::EmptyPayload`] para o DTX, e o erro do cabeçalho para um
    /// Opus maior que o teto do datagrama.
    pub fn datagrama(&mut self, carimbo: u32, opus: &[u8]) -> Result<Vec<u8>, MediaError> {
        if opus.is_empty() {
            return Err(MediaError::EmptyPayload);
        }
        let seq = self.relogio.pacote_saiu();
        let cabecalho = MediaHeader {
            version: MEDIA_HEADER_VERSION,
            ssrc: self.ssrc,
            seq,
            timestamp: carimbo,
        };
        let mut datagrama = vec![0_u8; HEADER_LEN + opus.len()];
        cabecalho.encode_datagram(opus, &mut datagrama)?;
        Ok(datagrama)
    }

    /// Outro caminho: outro microfone (o mesmo `ssrc`) ou a reconexão (o
    /// `ssrc` novo). O relógio salta dois quadros, como no desktop.
    pub fn reabrir(&mut self, ssrc: u32) {
        self.ssrc = ssrc;
        self.relogio = self.relogio.salto();
    }

    /// O relógio agora: a sequência do último pacote e o carimbo de agora.
    #[must_use]
    pub const fn relogio(&self) -> (u16, u32) {
        (self.relogio.seq(), self.relogio.carimbo())
    }
}

/// Um quadro de voz que chegou, separado por fonte.
#[derive(Debug, PartialEq, Eq)]
pub struct Chegado<'a> {
    /// A alça da fonte: o que a página guarda no lugar do SSRC.
    pub alca: u32,
    /// A sequência do pacote.
    pub seq: u16,
    /// O carimbo do quadro, em amostras.
    pub carimbo: u32,
    /// O Opus.
    pub opus: &'a [u8],
}

/// A voz que chega.
#[derive(Debug)]
pub struct Recepcao {
    tabela: TabelaDeFontes<u32>,
    ultima_alca: u32,
}

impl Recepcao {
    /// A recepção de quem fala com o SSRC `meu`, que nunca entra.
    #[must_use]
    pub const fn nova(meu: u32) -> Self {
        Self {
            tabela: TabelaDeFontes::nova(meu),
            ultima_alca: 0,
        }
    }

    /// Um datagrama chegou em `agora_ms`: a fonte dele, com uma alça nova na
    /// primeira vez, ou `None` para a própria voz.
    ///
    /// # Errors
    ///
    /// O erro do cabeçalho, para um datagrama torto (o desktop o ignora).
    pub fn chegou<'a>(
        &mut self,
        datagrama: &'a [u8],
        agora_ms: f64,
    ) -> Result<Option<Chegado<'a>>, MediaError> {
        let (cabecalho, opus) = MediaHeader::decode(datagrama)?;
        let ultima = &mut self.ultima_alca;
        let alca = self.tabela.chegou(cabecalho.ssrc, agora_ms, || {
            *ultima = ultima.wrapping_add(1);
            Some(*ultima)
        });
        Ok(alca.map(|alca| Chegado {
            alca: *alca,
            seq: cabecalho.seq,
            carimbo: cabecalho.timestamp,
            opus,
        }))
    }

    /// Esquece quem calou, e devolve as alças esquecidas: a página fecha o
    /// decodificador de cada uma.
    pub fn esquecer_caladas(&mut self, agora_ms: f64) -> Vec<u32> {
        let antes: Vec<u32> = self.tabela.iter().map(|(_, alca)| *alca).collect();
        self.tabela.esquecer_caladas(agora_ms);
        let ficaram: Vec<u32> = self.tabela.iter().map(|(_, alca)| *alca).collect();
        antes
            .into_iter()
            .filter(|alca| !ficaram.contains(alca))
            .collect()
    }

    /// A alça de quem fala com `ssrc`, para quem a liga a uma pessoa (o volume
    /// de cada um).
    #[must_use]
    pub fn alca_de(&self, ssrc: u32) -> Option<u32> {
        self.tabela
            .iter()
            .find(|(de_quem, _)| *de_quem == ssrc)
            .map(|(_, alca)| *alca)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "um teste que não monta o datagrama não tem o que conferir"
)]
mod testes {
    use super::*;

    /// O cabeçalho de um datagrama, `(sequência, carimbo)`.
    fn lido(datagrama: &[u8]) -> (u16, u32) {
        let (cabecalho, _) = MediaHeader::decode(datagrama).expect("um datagrama");
        (cabecalho.seq, cabecalho.timestamp)
    }

    /// **A mesma sequência fixa do desktop** (`seele-core`,
    /// `voice::relogio_gravado`): o navegador manda os mesmos pacotes.
    #[test]
    fn o_envio_manda_os_pacotes_da_sequencia_fixa_do_desktop() {
        let opus = [0xAB_u8; 40];
        let mut envio = Envio::de(77, 65_534, 14_400_000);
        let mut pacotes = Vec::new();
        let mut abre = |envio: &mut Envio, quantos: usize, vazios: &[usize]| {
            envio.quadro_passou();
            for indice in 0..quantos {
                let carimbo = envio.carimbo_do_quadro(quantos, indice);
                let corpo: &[u8] = if vazios.contains(&indice) { &[] } else { &opus };
                if let Ok(datagrama) = envio.datagrama(carimbo, corpo) {
                    pacotes.push(lido(&datagrama));
                }
            }
        };
        envio.quadro_passou(); // calado
        envio.quadro_passou(); // o atraso da supressão
        abre(&mut envio, 3, &[]);
        abre(&mut envio, 1, &[0]);
        envio.quadro_passou(); // mudo
        abre(&mut envio, 1, &[]);
        envio.reabrir(77);
        abre(&mut envio, 2, &[0]);
        assert_eq!(
            pacotes,
            [
                (65_535, 14_400_960),
                (0, 14_401_920),
                (1, 14_402_880),
                (2, 14_405_760),
                (5, 14_408_640),
            ]
        );
        assert_eq!(envio.relogio(), (5, 14_408_640));
    }

    /// **O datagrama é o do protocolo**, com o SSRC de quem manda e o Opus
    /// intacto atrás do cabeçalho.
    #[test]
    fn o_datagrama_e_o_do_protocolo() {
        let mut envio = Envio::novo(0x0102_0304);
        let carimbo = envio.quadro_passou();
        let datagrama = envio.datagrama(carimbo, &[9, 8, 7]).expect("um datagrama");
        assert_eq!(datagrama, [1, 1, 2, 3, 4, 0, 1, 0, 0, 3, 0xC0, 9, 8, 7]);
    }

    fn de(ssrc: u32, seq: u16) -> Vec<u8> {
        let mut datagrama = vec![0_u8; HEADER_LEN + 2];
        MediaHeader {
            version: MEDIA_HEADER_VERSION,
            ssrc,
            seq,
            timestamp: u32::from(seq) * 960,
        }
        .encode_datagram(&[seq as u8, 0xEE], &mut datagrama)
        .expect("um datagrama");
        datagrama
    }

    /// **A mesma sequência de chegadas do desktop** (`seele-core`,
    /// `voice::fontes_gravadas`), com alças no lugar dos SSRCs.
    #[test]
    fn a_recepcao_separa_por_alca_na_sequencia_fixa_do_desktop() {
        const EU: u32 = 5;
        const ANA: u32 = 7;
        const BIA: u32 = 9;
        let mut recepcao = Recepcao::nova(EU);
        let alca = |r: &mut Recepcao, ssrc: u32, ms: f64| {
            r.chegou(&de(ssrc, 1), ms)
                .expect("um cabeçalho")
                .map(|c| c.alca)
        };
        assert_eq!(alca(&mut recepcao, ANA, 0.0), Some(1));
        assert_eq!(alca(&mut recepcao, BIA, 10.0), Some(2));
        assert_eq!(alca(&mut recepcao, EU, 20.0), None, "a própria voz entrou");
        assert_eq!(alca(&mut recepcao, ANA, 1_000.0), Some(1));
        assert_eq!(recepcao.esquecer_caladas(30_005.0), Vec::<u32>::new());
        assert_eq!(
            recepcao.esquecer_caladas(30_020.0),
            [2],
            "a Bia não foi esquecida"
        );
        assert_eq!(
            alca(&mut recepcao, BIA, 30_030.0),
            Some(3),
            "a Bia voltou com a alça velha"
        );
        assert_eq!(recepcao.esquecer_caladas(31_005.0), [1]);
        assert_eq!(recepcao.alca_de(BIA), Some(3));
        assert_eq!(recepcao.alca_de(ANA), None);
    }

    /// O que chega atravessa inteiro, e um datagrama torto é recusado.
    #[test]
    fn o_que_chega_atravessa_e_o_torto_e_recusado() {
        let mut recepcao = Recepcao::nova(5);
        let datagrama = de(7, 300);
        let chegado = recepcao
            .chegou(&datagrama, 0.0)
            .expect("lido")
            .expect("de alguém");
        assert_eq!(
            chegado,
            Chegado {
                alca: 1,
                seq: 300,
                carimbo: 288_000,
                opus: &[44, 0xEE],
            }
        );
        assert!(recepcao.chegou(&[1, 0, 0, 0, 7], 0.0).is_err());
    }
}
