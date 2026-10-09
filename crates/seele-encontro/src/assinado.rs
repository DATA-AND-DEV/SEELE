//! O quarto assinado do SEELE-ENC/2 (ADR 0062): onde a sala pessoal de cada
//! pessoa mora, guardado só quando a chave dela assina.
//!
//! **Separado do quarto da versão 1**, e é de propósito. O `QUEM2` só lê daqui,
//! então um `MORO` sem assinatura nunca chega a quem pergunta pela sala de
//! alguém: «o registro assinado vence o não assinado» vale por construção. E um
//! `MORO` da versão 1, que qualquer um manda de graça, não consegue encher o
//! lugar das salas.
//!
//! O tempo entra por parâmetro: o relógio monotônico decide até quando um
//! registro responde, e o de parede decide se o carimbo está na tolerância.
//! Nada daqui vai a disco nem ao `--barulhento`, como o quarto da versão 1.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use seele_proto::codigo::Codigo;
use seele_proto::encontro::{Marca, Marcas, Moro2, Motivo, TOLERANCIA_DO_CARIMBO};

use crate::taxa::{prefixo, Prefixo};

/// Quantas marcas vivas um prefixo (`/32` no IPv4, `/64` no IPv6) ocupa no
/// quarto assinado (D-enc-3, decidido pelo dono em 08/10/2026).
///
/// Sessenta SEELE abertos atrás de um CGNAT, duas marcas cada, com folga. Sem
/// ele, um IP com chaves à vontade enchia o quarto sozinho, a 8 registros por
/// segundo; com ele, encher as [`MARCAS_ASSINADAS`] pede 128 prefixos, e o
/// cookie do `MORO2` (D-enc-4) faz cada um deles ter de ser de verdade.
pub const MARCAS_POR_PREFIXO: usize = 128;

/// Por quanto tempo um registro responde ao `QUEM2` depois do último `MORO2`
/// aceito. O mesmo do ENC/1: quatro reavivamentos de quinze segundos.
pub const PRAZO_DO_REGISTRO: Duration = Duration::from_secs(60);

/// Quantas marcas cabem no quarto assinado (D-enc-1, decidido pelo dono em
/// 08/10/2026).
///
/// Duas por app aberto da 0.16, que registra a sala sempre: cerca de 8 000 apps.
/// Cada registro custa perto de 280 bytes com o mapa, então o quarto cheio
/// custa uns 4,6 MB. Separado das 4096 marcas do quarto da versão 1.
///
/// **O teto conta os registros vivos.** Um registro vencido continua lembrado
/// enquanto o carimbo dele passaria na tolerância, e o carimbo pode vir até
/// 300 s no futuro: contado no teto, cada `MORO2` seguraria a vaga por ~600 s,
/// e quatro IPv4 a 8 pedidos por segundo encheriam o quarto para sempre
/// (medido na revisão de 08/10/2026). Com o quarto no teto, os só lembrados
/// saem antes de um registro novo ouvir «cheio». O preço é perder, sob pressão,
/// a lembrança que recusa a repetição de um `MORO2` copiado no caminho.
pub const MARCAS_ASSINADAS: usize = 16_384;

/// Se este `MORO2` é de quem tem a chave: a marca é a escuta ou o servidor da
/// sala desta chave ([`Marcas::da_sala`]), e a assinatura confere.
///
/// O aviso não se registra: ele é a marca que o `LEVE` de quem liga traz, e
/// não mora no quarto.
///
/// `verify_strict`, e não `verify`: ele recusa chave de ordem pequena e
/// assinatura maleável. Com o `verify`, a chave identidade e a assinatura
/// `R = identidade, s = 0` conferem com qualquer mensagem, e qualquer um
/// registraria a sala do código dela
/// (`uma_chave_fraca_nao_registra_com_a_assinatura_que_vale_para_tudo`).
#[must_use]
pub fn conferir(pedido: &Moro2) -> bool {
    let marcas = Marcas::da_sala(&Codigo::da_chave(&pedido.chave));
    if pedido.marca != *marcas.escuta() && pedido.marca != *marcas.servidor() {
        return false;
    }
    let Ok(chave) = ed25519_dalek::VerifyingKey::from_bytes(&pedido.chave) else {
        return false;
    };
    let assinatura = ed25519_dalek::Signature::from_bytes(&pedido.assinatura);
    let mensagem =
        seele_proto::assinatura::moro2(&pedido.marca, &pedido.chave, pedido.carimbo, &pedido.dicas);
    chave.verify_strict(&mensagem, &assinatura).is_ok()
}

/// Um registro, e o que é preciso para julgar o próximo.
#[derive(Debug)]
struct Registro {
    endereco: SocketAddr,
    /// O prefixo de `endereco`, para o teto por prefixo.
    prefixo: Prefixo,
    desde: Instant,
    chave: [u8; 32],
    /// O maior carimbo aceito desta marca.
    carimbo: u64,
    dicas: Vec<SocketAddr>,
}

impl Registro {
    /// Se ele responde ao `QUEM2`.
    fn vale(&self, agora: Instant) -> bool {
        agora.saturating_duration_since(self.desde) < PRAZO_DO_REGISTRO
    }

    /// Se ele ainda precisa ser lembrado: vale, ou o carimbo dele ainda
    /// passaria na tolerância, e um `MORO2` copiado no caminho poderia ser
    /// repetido por outra pessoa.
    fn lembrado(&self, agora: Instant, agora_unix: u64) -> bool {
        self.vale(agora) || self.carimbo.saturating_add(TOLERANCIA_DO_CARIMBO) >= agora_unix
    }
}

/// O quarto assinado. Ver o cabeçalho do módulo.
#[derive(Debug)]
pub struct QuartoAssinado {
    registros: Mutex<HashMap<String, Registro>>,
    teto: usize,
    por_prefixo: usize,
}

impl Default for QuartoAssinado {
    fn default() -> Self {
        Self::novo()
    }
}

impl QuartoAssinado {
    /// Um quarto vazio, com o teto de [`MARCAS_ASSINADAS`].
    #[must_use]
    pub fn novo() -> Self {
        Self::com_teto(MARCAS_ASSINADAS)
    }

    /// Um quarto vazio com outro teto: o teste do teto não precisa de 16 384
    /// chaves.
    #[must_use]
    pub fn com_teto(teto: usize) -> Self {
        Self::com_tetos(teto, MARCAS_POR_PREFIXO)
    }

    /// Um quarto vazio com outros tetos, o do quarto e o de cada prefixo.
    #[must_use]
    pub fn com_tetos(teto: usize, por_prefixo: usize) -> Self {
        Self {
            registros: Mutex::new(HashMap::new()),
            teto,
            por_prefixo,
        }
    }

    /// Guarda o registro, ou diz por que não. Quem chama já conferiu a
    /// assinatura ([`conferir`]).
    ///
    /// As regras (ADR 0062):
    ///
    /// - carimbo fora de [`TOLERANCIA_DO_CARIMBO`]: [`Motivo::Relogio`];
    /// - carimbo mais novo que o guardado: vence na hora, de qualquer endereço.
    ///   Uma troca de rede vale no reavivamento seguinte;
    /// - carimbo igual ou mais velho: só do mesmo socket (uma retransmissão, ou
    ///   o relógio de quem registra voltou), e o guardado continua o maior. De
    ///   outro endereço, [`Motivo::Velho`]: é um `MORO2` copiado no caminho e
    ///   repetido de outro lugar;
    /// - outra chave com a mesma marca não entra enquanto o registro vale;
    /// - marca nova com o quarto no teto: varre o que já não é lembrado, e se
    ///   continuar cheio, [`Motivo::Cheio`]. Quem já mora continua reavivando;
    /// - uma marca que passaria a viver num prefixo que já tem
    ///   [`MARCAS_POR_PREFIXO`] vivas: [`Motivo::Cheio`]. Contar os vivos do
    ///   prefixo varre o quarto, e por isso só se conta quando a marca não está
    ///   viva ali: o reavivamento de sempre não paga a varredura.
    ///
    /// # Errors
    ///
    /// O [`Motivo`] da recusa, que o ponto devolve a quem pediu.
    pub fn morar(
        &self,
        pedido: &Moro2,
        de: SocketAddr,
        agora: Instant,
        agora_unix: u64,
    ) -> Result<(), Motivo> {
        if pedido.carimbo.abs_diff(agora_unix) > TOLERANCIA_DO_CARIMBO {
            return Err(Motivo::Relogio);
        }
        let mut registros = self.trancar();
        let aqui = prefixo(de.ip());
        let ja_vive_aqui = registros
            .get(pedido.marca.texto())
            .is_some_and(|atual| atual.vale(agora) && atual.prefixo == aqui);
        let prefixo_cheio = !ja_vive_aqui
            && registros
                .values()
                .filter(|registro| registro.prefixo == aqui && registro.vale(agora))
                .count()
                >= self.por_prefixo;
        let novo = || Registro {
            endereco: de,
            prefixo: aqui,
            desde: agora,
            chave: pedido.chave,
            carimbo: pedido.carimbo,
            dicas: pedido.dicas.clone(),
        };
        let marca = pedido.marca.texto();
        match registros
            .get_mut(marca)
            .filter(|atual| atual.lembrado(agora, agora_unix))
        {
            Some(atual) if atual.chave != pedido.chave => {
                if atual.vale(agora) {
                    return Err(Motivo::Velho);
                }
                if prefixo_cheio {
                    return Err(Motivo::Cheio);
                }
                *atual = novo();
            }
            Some(atual) if pedido.carimbo > atual.carimbo => {
                if prefixo_cheio {
                    return Err(Motivo::Cheio);
                }
                *atual = novo();
            }
            Some(atual) if atual.endereco == de => {
                if prefixo_cheio {
                    return Err(Motivo::Cheio);
                }
                atual.desde = agora;
                atual.dicas.clone_from(&pedido.dicas);
            }
            Some(_) => return Err(Motivo::Velho),
            None => {
                if prefixo_cheio {
                    return Err(Motivo::Cheio);
                }
                if !registros.contains_key(marca) && registros.len() >= self.teto {
                    // Primeiro o que nem é mais lembrado; depois, se ainda não
                    // couber, o que só é lembrado. Os vivos ficam.
                    registros.retain(|_, registro| registro.lembrado(agora, agora_unix));
                    if registros.len() >= self.teto {
                        registros.retain(|_, registro| registro.vale(agora));
                    }
                    if registros.len() >= self.teto {
                        return Err(Motivo::Cheio);
                    }
                }
                registros.insert(marca.to_owned(), novo());
            }
        }
        Ok(())
    }

    /// Onde esta marca mora, e as dicas dela, se o registro ainda vale.
    #[must_use]
    pub fn onde_mora(
        &self,
        marca: &Marca,
        agora: Instant,
    ) -> Option<(SocketAddr, Vec<SocketAddr>)> {
        let registros = self.trancar();
        let registro = registros
            .get(marca.texto())
            .filter(|registro| registro.vale(agora))?;
        Some((registro.endereco, registro.dicas.clone()))
    }

    /// Quantos registros o quarto guarda agora, os vencidos lembrados
    /// inclusive. Para o teste.
    #[must_use]
    pub fn quantos(&self) -> usize {
        self.trancar().len()
    }

    /// O mapa, mesmo com o `Mutex` envenenado.
    ///
    /// Um pânico noutra linha com o quarto na mão envenena o `Mutex`, e o mapa
    /// não fica inconsistente: cada operação aqui é inteira. Recusar daí em
    /// diante como cheio, ou dizer `NINGUEM` para toda marca, seria mentir para
    /// sempre a quem pergunta.
    fn trancar(&self) -> MutexGuard<'_, HashMap<String, Registro>> {
        self.registros
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const AGORA: u64 = 1_791_000_000;
    const A: [u8; 4] = [203, 0, 113, 7];
    const B: [u8; 4] = [198, 51, 100, 66];

    fn chave(semente: u8) -> SigningKey {
        SigningKey::from_bytes(&[semente; 32])
    }

    fn marcas_de(chave: &SigningKey) -> Marcas {
        Marcas::da_sala(&Codigo::da_chave(&chave.verifying_key().to_bytes()))
    }

    /// Um `MORO2` assinado de verdade, com uma marca qualquer.
    fn assinado(chave: &SigningKey, marca: Marca, carimbo: u64, dicas: Vec<SocketAddr>) -> Moro2 {
        let publica = chave.verifying_key().to_bytes();
        let mensagem = seele_proto::assinatura::moro2(&marca, &publica, carimbo, &dicas);
        Moro2 {
            marca,
            chave: publica,
            carimbo,
            dicas,
            assinatura: chave.sign(&mensagem).to_bytes(),
            cookie: None,
        }
    }

    /// O `MORO2` do servidor da sala desta chave.
    fn do_servidor(chave: &SigningKey, carimbo: u64, dicas: Vec<SocketAddr>) -> Moro2 {
        assinado(chave, marcas_de(chave).servidor().clone(), carimbo, dicas)
    }

    fn em(ip: [u8; 4], porta: u16) -> SocketAddr {
        SocketAddr::from((ip, porta))
    }

    #[test]
    fn um_moro2_de_quem_tem_a_chave_confere() {
        let minha = chave(1);
        assert!(conferir(&do_servidor(
            &minha,
            AGORA,
            vec![em([192, 168, 0, 10], 8385)]
        )));
        assert!(conferir(&assinado(
            &minha,
            marcas_de(&minha).escuta().clone(),
            AGORA,
            Vec::new()
        )));
    }

    #[test]
    fn uma_assinatura_de_outra_chave_nao_confere() {
        let mut forjado = do_servidor(&chave(1), AGORA, Vec::new());
        forjado.assinatura = do_servidor(&chave(2), AGORA, Vec::new()).assinatura;
        assert!(!conferir(&forjado));
    }

    #[test]
    fn um_campo_trocado_depois_de_assinar_nao_confere() {
        let base = do_servidor(&chave(1), AGORA, vec![em([192, 168, 0, 10], 8385)]);
        assert!(conferir(&base), "o caso de controle tem de conferir");
        assert!(!conferir(&Moro2 {
            carimbo: AGORA + 1,
            ..base.clone()
        }));
        assert!(!conferir(&Moro2 {
            dicas: vec![em([192, 168, 0, 11], 8385)],
            ..base.clone()
        }));
        assert!(!conferir(&Moro2 {
            dicas: Vec::new(),
            ..base
        }));
    }

    #[test]
    fn uma_marca_que_nao_sai_da_chave_nao_confere() {
        // A assinatura vale sobre a marca de outra pessoa, e mesmo assim não
        // confere: a marca tem de sair desta chave. É o que impede alguém de
        // registrar, com a própria chave, a sala de outro.
        let dele = marcas_de(&chave(2)).servidor().clone();
        assert!(!conferir(&assinado(&chave(1), dele, AGORA, Vec::new())));
    }

    #[test]
    fn a_marca_do_aviso_nao_se_registra() {
        // O aviso é a marca que o `LEVE` de quem liga traz; ela não mora no
        // quarto, e registrá-la mandaria os `LEVE` para o lugar errado.
        let minha = chave(1);
        let aviso = marcas_de(&minha).aviso().clone();
        assert!(!conferir(&assinado(&minha, aviso, AGORA, Vec::new())));
    }

    #[test]
    fn uma_chave_fraca_nao_registra_com_a_assinatura_que_vale_para_tudo() {
        // A chave é o ponto identidade, de ordem pequena, e a assinatura é
        // R = identidade e s = 0: no `verify` de sempre ela confere com
        // **qualquer** mensagem. Sem o `verify_strict`, qualquer um registraria
        // a sala do código desta chave, com o endereço que quisesse.
        let mut identidade = [0_u8; 32];
        identidade[0] = 1;
        let marca = Marcas::da_sala(&Codigo::da_chave(&identidade))
            .servidor()
            .clone();
        let mut assinatura = [0_u8; 64];
        assinatura[0] = 1;
        let pedido = Moro2 {
            marca,
            chave: identidade,
            carimbo: AGORA,
            dicas: Vec::new(),
            assinatura,
            cookie: None,
        };
        assert!(
            !conferir(&pedido),
            "uma chave fraca registrou com a assinatura universal"
        );
    }

    #[test]
    fn um_carimbo_fora_da_tolerancia_e_recusado_pelo_relogio() {
        let quarto = QuartoAssinado::novo();
        let agora = Instant::now();
        for carimbo in [AGORA - 301, AGORA + 301] {
            assert_eq!(
                quarto.morar(
                    &do_servidor(&chave(1), carimbo, Vec::new()),
                    em(A, 4000),
                    agora,
                    AGORA
                ),
                Err(Motivo::Relogio),
                "o carimbo {carimbo} passou com o relógio do ponto em {AGORA}"
            );
        }
        for (semente, carimbo) in [(2, AGORA - 300), (3, AGORA + 300)] {
            assert_eq!(
                quarto.morar(
                    &do_servidor(&chave(semente), carimbo, Vec::new()),
                    em(A, 4000),
                    agora,
                    AGORA
                ),
                Ok(()),
                "o carimbo {carimbo}, na beira da tolerância, foi recusado"
            );
        }
    }

    #[test]
    fn a_mesma_chave_noutro_ip_com_carimbo_novo_vence_na_hora() {
        // Uma troca de rede vale no próximo reavivamento, e não depois dos
        // 60 s que o ENC/1 trava.
        let quarto = QuartoAssinado::novo();
        let agora = Instant::now();
        let minha = chave(1);
        let marca = marcas_de(&minha).servidor().clone();
        let dicas = vec![em([192, 168, 0, 10], 8385)];
        quarto
            .morar(
                &do_servidor(&minha, AGORA, Vec::new()),
                em(A, 4000),
                agora,
                AGORA,
            )
            .unwrap();

        let depois = agora + Duration::from_secs(15);
        assert_eq!(
            quarto.morar(
                &do_servidor(&minha, AGORA + 15, dicas.clone()),
                em(B, 5000),
                depois,
                AGORA + 15
            ),
            Ok(())
        );
        assert_eq!(quarto.onde_mora(&marca, depois), Some((em(B, 5000), dicas)));
    }

    #[test]
    fn o_mesmo_moro2_repetido_de_outro_endereco_nao_toma_a_marca() {
        let quarto = QuartoAssinado::novo();
        let agora = Instant::now();
        let original = do_servidor(&chave(1), AGORA, Vec::new());
        quarto.morar(&original, em(A, 4000), agora, AGORA).unwrap();

        assert_eq!(
            quarto.morar(&original, em(B, 4000), agora, AGORA),
            Err(Motivo::Velho)
        );
        // Do mesmo IP, noutra porta, também não: só do mesmo socket.
        assert_eq!(
            quarto.morar(&original, em(A, 4001), agora, AGORA),
            Err(Motivo::Velho)
        );
        // E do mesmo socket, sim: é uma retransmissão.
        assert_eq!(quarto.morar(&original, em(A, 4000), agora, AGORA), Ok(()));
        assert_eq!(
            quarto
                .onde_mora(&original.marca, agora)
                .map(|(onde, _)| onde),
            Some(em(A, 4000))
        );
    }

    #[test]
    fn depois_do_prazo_a_marca_some_mas_a_repeticao_continua_recusada() {
        let quarto = QuartoAssinado::novo();
        let agora = Instant::now();
        let minha = chave(1);
        let original = do_servidor(&minha, AGORA, Vec::new());
        quarto.morar(&original, em(A, 4000), agora, AGORA).unwrap();

        let depois = agora + PRAZO_DO_REGISTRO + Duration::from_secs(1);
        assert_eq!(
            quarto.onde_mora(&original.marca, depois),
            None,
            "a marca venceu e ainda responde"
        );
        assert_eq!(
            quarto.morar(&original, em(B, 4000), depois, AGORA + 61),
            Err(Motivo::Velho),
            "um MORO2 copiado no caminho tomou a marca depois do prazo, dentro da tolerância"
        );
        // Um registro novo de verdade, de outro lugar, entra.
        assert_eq!(
            quarto.morar(
                &do_servidor(&minha, AGORA + 62, Vec::new()),
                em(B, 4000),
                depois,
                AGORA + 62
            ),
            Ok(())
        );
    }

    #[test]
    fn o_relogio_que_volta_no_mesmo_socket_nao_derruba_o_registro() {
        let quarto = QuartoAssinado::novo();
        let agora = Instant::now();
        let minha = chave(1);
        quarto
            .morar(
                &do_servidor(&minha, AGORA, Vec::new()),
                em(A, 4000),
                agora,
                AGORA,
            )
            .unwrap();

        let depois = agora + Duration::from_secs(15);
        assert_eq!(
            quarto.morar(
                &do_servidor(&minha, AGORA - 100, Vec::new()),
                em(A, 4000),
                depois,
                AGORA + 15
            ),
            Ok(()),
            "o relógio desta máquina voltou 100 s e o registro do mesmo socket foi recusado"
        );
        // O carimbo guardado continua o maior: um MORO2 de carimbo
        // intermediário, de outro endereço, continua recusado.
        assert_eq!(
            quarto.morar(
                &do_servidor(&minha, AGORA - 50, Vec::new()),
                em(B, 4000),
                depois,
                AGORA + 15
            ),
            Err(Motivo::Velho)
        );
    }

    #[test]
    fn outra_chave_com_a_mesma_marca_nao_toma_o_lugar_enquanto_ele_vale() {
        // Uma colisão de 75 bits custa ~2^75 gerações de chave. O quarto não
        // conta com isso: a marca guarda a chave que a registrou.
        let quarto = QuartoAssinado::novo();
        let agora = Instant::now();
        let original = do_servidor(&chave(1), AGORA, Vec::new());
        quarto.morar(&original, em(A, 4000), agora, AGORA).unwrap();
        let colisao = Moro2 {
            chave: [0xEE; 32],
            carimbo: AGORA + 15,
            ..original.clone()
        };
        assert_eq!(
            quarto.morar(&colisao, em(B, 4000), agora, AGORA + 15),
            Err(Motivo::Velho)
        );
        assert_eq!(
            quarto
                .onde_mora(&original.marca, agora)
                .map(|(onde, _)| onde),
            Some(em(A, 4000))
        );
    }

    #[test]
    fn o_teto_recusa_dizendo_cheio_e_varre_o_que_ja_nao_vale() {
        let quarto = QuartoAssinado::com_teto(2);
        let agora = Instant::now();
        quarto
            .morar(
                &do_servidor(&chave(1), AGORA, Vec::new()),
                em(A, 1),
                agora,
                AGORA,
            )
            .unwrap();
        quarto
            .morar(
                &do_servidor(&chave(2), AGORA, Vec::new()),
                em(A, 2),
                agora,
                AGORA,
            )
            .unwrap();
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(3), AGORA, Vec::new()),
                em(A, 3),
                agora,
                AGORA
            ),
            Err(Motivo::Cheio)
        );
        // Quem já mora continua podendo reavivar com o quarto cheio.
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(1), AGORA + 15, Vec::new()),
                em(A, 1),
                agora,
                AGORA + 15
            ),
            Ok(())
        );
        // Vencidos o prazo **e** a tolerância do carimbo, o lugar se libera.
        let muito_depois = agora + PRAZO_DO_REGISTRO + Duration::from_secs(1);
        let unix = AGORA + 15 + TOLERANCIA_DO_CARIMBO + 1;
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(3), unix, Vec::new()),
                em(A, 3),
                muito_depois,
                unix
            ),
            Ok(())
        );
        assert!(
            quarto.quantos() <= 2,
            "o quarto passou do teto: {}",
            quarto.quantos()
        );
    }

    #[test]
    fn um_registro_so_lembrado_nao_segura_vaga_contra_um_novo() {
        // O achado da revisão de 08/10: um registro conta enquanto o carimbo
        // passaria na tolerância, e o carimbo pode vir 300 s no futuro. Contado
        // assim, um MORO2 só segurava a vaga por ~600 s, e quatro IPv4 enchiam
        // o quarto para sempre. O teto conta os registros **vivos**: um
        // registro só lembrado (vencido, guardado contra a repetição) cede a
        // vaga a um registro novo.
        let quarto = QuartoAssinado::com_teto(2);
        let agora = Instant::now();
        quarto
            .morar(
                &do_servidor(&chave(1), AGORA + 300, Vec::new()),
                em(A, 1),
                agora,
                AGORA,
            )
            .unwrap();
        quarto
            .morar(
                &do_servidor(&chave(2), AGORA, Vec::new()),
                em(A, 2),
                agora,
                AGORA,
            )
            .unwrap();

        let depois = agora + PRAZO_DO_REGISTRO + Duration::from_secs(1);
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(3), AGORA + 61, Vec::new()),
                em(A, 3),
                depois,
                AGORA + 61
            ),
            Ok(()),
            "dois registros vencidos, só lembrados, seguraram o quarto contra um registro vivo"
        );
        assert!(
            quarto.quantos() <= 2,
            "o quarto passou do teto: {}",
            quarto.quantos()
        );

        // Com o quarto cheio de vivos, continua «cheio».
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(4), AGORA + 61, Vec::new()),
                em(A, 4),
                depois,
                AGORA + 61
            ),
            Ok(()),
            "havia um só lembrado, e ele não cedeu a vaga"
        );
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(5), AGORA + 61, Vec::new()),
                em(A, 5),
                depois,
                AGORA + 61
            ),
            Err(Motivo::Cheio),
            "com o quarto cheio de registros vivos, um novo entrou"
        );
    }

    #[test]
    fn um_mutex_envenenado_nao_cala_o_quarto() {
        // Um pânico noutra linha com o quarto na mão envenena o `Mutex`. O mapa
        // não fica inconsistente (cada operação é inteira), e recusar para
        // sempre seria mentir: o quarto continua guardando.
        let quarto = std::sync::Arc::new(QuartoAssinado::novo());
        let outro = std::sync::Arc::clone(&quarto);
        let _ = std::thread::spawn(move || {
            let _guarda = outro.registros.lock().unwrap();
            panic!("pânico de propósito, com o quarto na mão");
        })
        .join();
        assert!(
            quarto.registros.is_poisoned(),
            "o teste não envenenou o Mutex"
        );
        let pedido = do_servidor(&chave(1), AGORA, Vec::new());
        let agora = Instant::now();
        assert_eq!(quarto.morar(&pedido, em(A, 1), agora, AGORA), Ok(()));
        assert_eq!(
            quarto.onde_mora(&pedido.marca, agora).map(|(onde, _)| onde),
            Some(em(A, 1))
        );
    }

    #[test]
    fn um_prefixo_nao_ocupa_mais_marcas_vivas_que_o_teto_dele() {
        // D-enc-3: sem forjar nada, um IP com chaves à vontade enchia o quarto
        // sozinho, a 8 registros por segundo. Com o teto por prefixo, encher
        // pede tantos prefixos quanto o quarto dividido pelo teto.
        let quarto = QuartoAssinado::com_tetos(1000, 2);
        let agora = Instant::now();
        quarto
            .morar(
                &do_servidor(&chave(1), AGORA, Vec::new()),
                em(A, 1),
                agora,
                AGORA,
            )
            .unwrap();
        quarto
            .morar(
                &do_servidor(&chave(2), AGORA, Vec::new()),
                em(A, 2),
                agora,
                AGORA,
            )
            .unwrap();
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(3), AGORA, Vec::new()),
                em(A, 3),
                agora,
                AGORA
            ),
            Err(Motivo::Cheio),
            "um prefixo passou do teto de marcas vivas"
        );
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(3), AGORA, Vec::new()),
                em(B, 3),
                agora,
                AGORA
            ),
            Ok(()),
            "o teto de um prefixo segurou outro"
        );
        // Quem já mora no prefixo continua reavivando com ele no teto.
        let depois = agora + Duration::from_secs(15);
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(1), AGORA + 15, Vec::new()),
                em(A, 1),
                depois,
                AGORA + 15
            ),
            Ok(())
        );
        // Mudar para um prefixo no teto conta como entrar nele.
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(3), AGORA + 15, Vec::new()),
                em(A, 9),
                depois,
                AGORA + 15
            ),
            Err(Motivo::Cheio),
            "uma marca mudou para um prefixo no teto"
        );
        // Vencidas as do prefixo, o lugar se libera.
        let muito_depois = agora + PRAZO_DO_REGISTRO + Duration::from_secs(16);
        assert_eq!(
            quarto.morar(
                &do_servidor(&chave(4), AGORA + 80, Vec::new()),
                em(A, 4),
                muito_depois,
                AGORA + 80
            ),
            Ok(())
        );
    }

    #[test]
    fn os_numeros_sao_os_da_decisao_do_dono() {
        // D-enc-1, decidido pelo dono em 08/10/2026 (ADR 0062).
        assert_eq!(MARCAS_ASSINADAS, 16_384);
        assert_eq!(MARCAS_POR_PREFIXO, 128);
        assert_eq!(PRAZO_DO_REGISTRO, Duration::from_secs(60));
    }
}
