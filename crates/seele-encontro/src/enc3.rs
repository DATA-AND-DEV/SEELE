//! O SEELE-ENC/3 no ponto (contrato da 0.17, D7): o registro da 0.17, a
//! pergunta, e a chamada que atravessa o NAT.
//!
//! # O que muda da versão 2
//!
//! - **Dois quartos novos, separados dos de antes.** O anfitrião da 0.17 se
//!   registra só aqui, e quem pergunta pelo ENC/1 ou pelo ENC/2 (uma 0.16) não o
//!   acha: é a recusa da linha 1 dita pelo ponto (D6). O quarto assinado do ENC/3
//!   guarda as salas pessoais (`MORO3`, as regras do ADR 0062); o quarto livre
//!   ([`QuartoLivre`]) guarda os servidores guardados (`MORA3`), com as regras do
//!   quarto do ENC/1 e mais o cookie.
//! - **A chamada.** O `CHAMA` de quem liga vira um `CHAMADO` à escuta do
//!   anfitrião, com um `id` sorteado que só ela recebe; o `ATENDE` dela, com
//!   aquele `id` e do IP dela, vira o `AQUI` de quem liga. Sem `ATENDE` em
//!   [`PRAZO_DA_CHAMADA`], quem liga ouve `CALADO`. As chamadas pendentes são o
//!   único estado novo, com teto ([`CHAMADAS_PENDENTES`]).
//!
//! # O que o ponto escreve a quem não pediu
//!
//! Só o `CHAMADO`, e só a uma escuta registrada no quarto do ENC/3, com o
//! tamanho do pedido que o causou. O `CHAMA` gasta o balde próprio dele
//! ([`crate::taxa::Classe::Chamada`]), e o `id` de 128 bits é o que impede
//! um terceiro de forjar o `ATENDE`.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use seele_proto::encontro::{
    alcancavel, e_marca_de_sala, resposta3, Atende, Chama, Chamado, Marca, Motivo, Resposta3,
    Vizinhanca, BYTES_DO_ID, LIMITE_DE_CANDIDATOS,
};

/// Quanto tempo o ponto espera o `ATENDE` de uma escuta.
///
/// Três segundos: o anfitrião responde assim que fura, e a perna mais longa
/// medida (ponto em Atlanta, ADR 0022) somava ~280 ms. Mais que isto é um
/// anfitrião que não está lá, e quem liga merece ouvir `CALADO` logo.
pub const PRAZO_DA_CHAMADA: Duration = Duration::from_secs(3);

/// Quantas chamadas podem esperar `ATENDE` ao mesmo tempo, no ponto inteiro.
///
/// Cada uma custa uns cem bytes por [`PRAZO_DA_CHAMADA`]; com o balde de
/// chamadas por prefixo, encher isto pede dezenas de prefixos ao mesmo tempo, e
/// quem chega com ele cheio ouve `RECUSA cheio`.
pub const CHAMADAS_PENDENTES: usize = 1024;

/// O prazo de uma entrada do quarto livre, como o do ENC/1.
const PRAZO_DO_LIVRE: Duration = Duration::from_secs(60);

/// O teto do quarto livre, como o do ENC/1.
const MARCAS_NO_LIVRE: usize = 4096;

/// O quarto livre do ENC/3: onde cada servidor guardado disse morar.
///
/// As regras do quarto do ENC/1 (`Quarto::morar`): quem escreveu primeiro fica
/// enquanto o prazo vale, e o próprio morador muda de porta. A diferença é
/// que aqui só se chega com o cookie do IP, conferido antes, e que a recusa é
/// dita.
#[derive(Debug, Default)]
pub struct QuartoLivre {
    moradores: Mutex<HashMap<String, (SocketAddr, Instant)>>,
}

impl QuartoLivre {
    /// Registra que esta marca mora neste endereço.
    ///
    /// # Errors
    ///
    /// [`Motivo::Velho`] quando outro IP mora nela e o prazo dele ainda vale;
    /// [`Motivo::Cheio`] com o quarto no teto, depois de varrer o que venceu.
    pub fn morar(&self, marca: &str, onde: SocketAddr, agora: Instant) -> Result<(), Motivo> {
        let mut moradores = self
            .moradores
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match moradores.get(marca) {
            Some((antigo, _)) if antigo.ip() == onde.ip() => {}
            Some((_, desde)) if agora.saturating_duration_since(*desde) < PRAZO_DO_LIVRE => {
                return Err(Motivo::Velho);
            }
            Some(_) | None => {}
        }
        if moradores.len() >= MARCAS_NO_LIVRE && !moradores.contains_key(marca) {
            moradores
                .retain(|_, (_, desde)| agora.saturating_duration_since(*desde) < PRAZO_DO_LIVRE);
            if moradores.len() >= MARCAS_NO_LIVRE {
                return Err(Motivo::Cheio);
            }
        }
        moradores.insert(marca.to_owned(), (onde, agora));
        Ok(())
    }

    /// Onde esta marca mora, se o prazo não venceu.
    #[must_use]
    pub fn onde_mora(&self, marca: &str, agora: Instant) -> Option<SocketAddr> {
        let moradores = self
            .moradores
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (onde, desde) = moradores.get(marca)?;
        (agora.saturating_duration_since(*desde) < PRAZO_DO_LIVRE).then_some(*onde)
    }
}

/// Para quem vai a resposta de uma chamada.
#[derive(Debug)]
pub enum QuemLiga {
    /// Quem ligou pelo ENC/3: a resposta sai pelo socket que recebeu o `CHAMA`.
    Udp {
        /// O socket da família de quem ligou.
        socket: Arc<UdpSocket>,
        /// O endereço de quem ligou.
        para: SocketAddr,
    },
    /// Quem ligou pelo sinal: a linha de execução do pedido HTTP espera aqui.
    Sinal(SyncSender<Resposta3>),
}

impl QuemLiga {
    /// Entrega a resposta. Uma que não sai (o socket que falhou, o pedido HTTP
    /// que já desistiu) não é erro do ponto.
    pub fn entregar(self, resposta: Resposta3) {
        match self {
            Self::Udp { socket, para } => {
                let _ = socket.send_to(&resposta3(&resposta), para);
            }
            Self::Sinal(espera) => {
                let _ = espera.try_send(resposta);
            }
        }
    }
}

#[derive(Debug)]
struct Pendente {
    escuta_ip: IpAddr,
    servidor: Marca,
    endereco_do_servidor: SocketAddr,
    desde: Instant,
    quem: QuemLiga,
}

/// As chamadas que esperam `ATENDE`.
#[derive(Debug, Default)]
pub struct Chamadas {
    pendentes: Mutex<HashMap<[u8; BYTES_DO_ID], Pendente>>,
}

impl Chamadas {
    fn trancar(&self) -> std::sync::MutexGuard<'_, HashMap<[u8; BYTES_DO_ID], Pendente>> {
        self.pendentes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Quantas esperam agora. Para o teste.
    #[must_use]
    pub fn quantas(&self) -> usize {
        self.trancar().len()
    }

    /// Esquece uma chamada (a linha do sinal que desistiu no prazo).
    pub fn esquecer(&self, id: &[u8; BYTES_DO_ID]) {
        self.trancar().remove(id);
    }

    /// Diz `CALADO` a quem ligou por UDP e esperou mais que
    /// [`PRAZO_DA_CHAMADA`]; esquece as do sinal, cuja linha já respondeu.
    pub fn varrer(&self, agora: Instant) {
        let vencidas: Vec<Pendente> = {
            let mut pendentes = self.trancar();
            let ids: Vec<_> = pendentes
                .iter()
                .filter(|(_, pendente)| {
                    agora.saturating_duration_since(pendente.desde) >= PRAZO_DA_CHAMADA
                })
                .map(|(id, _)| *id)
                .collect();
            ids.iter().filter_map(|id| pendentes.remove(id)).collect()
        };
        for pendente in vencidas {
            if matches!(pendente.quem, QuemLiga::Udp { .. }) {
                let marca = pendente.servidor.clone();
                pendente.quem.entregar(Resposta3::Calado { marca });
            }
        }
    }

    /// Guarda uma chamada, ou diz [`Motivo::Cheio`].
    fn guardar(&self, id: [u8; BYTES_DO_ID], pendente: Pendente) -> Result<(), Box<Pendente>> {
        let mut pendentes = self.trancar();
        if pendentes.len() >= CHAMADAS_PENDENTES {
            return Err(Box::new(pendente));
        }
        pendentes.insert(id, pendente);
        Ok(())
    }

    /// O `ATENDE`: o `id` pendente, do IP da escuta. Devolve a chamada e a
    /// resposta para quem ligou; `None` cala (um `id` que não existe, ou que
    /// veio de outro IP: um terceiro tentando forjar).
    pub fn atendida(&self, atende: &Atende, de: SocketAddr) -> Option<(QuemLiga, Resposta3)> {
        let mut pendentes = self.trancar();
        let pendente = pendentes.get(&atende.id)?;
        if pendente.escuta_ip.to_canonical() != de.ip().to_canonical() {
            return None;
        }
        let pendente = pendentes.remove(&atende.id)?;
        drop(pendentes);
        // O endereço do `ATENDE` só vale no IP da escuta: a escuta e o servidor
        // de um anfitrião saem pelo mesmo IP público. Fora disso, o que o
        // próprio ponto viu no registro.
        let endereco = if atende.endereco.ip().to_canonical() == pendente.escuta_ip.to_canonical() {
            atende.endereco
        } else {
            pendente.endereco_do_servidor
        };
        let resposta = Resposta3::Aqui {
            marca: pendente.servidor,
            endereco,
            dicas: atende.dicas.clone(),
        };
        Some((pendente.quem, resposta))
    }
}

/// Os candidatos que vão no `CHAMADO`: os de quem liga que a vizinhança deixa
/// alcançar e, pelo ENC/3, o endereço que o ponto viu, sem repetir.
#[must_use]
pub fn candidatos_do_chamado(
    chama: &Chama,
    visto: Option<SocketAddr>,
    vizinhanca: Vizinhanca,
) -> Vec<SocketAddr> {
    let deixa = |endereco: &SocketAddr| match vizinhanca {
        Vizinhanca::Internet => alcancavel(*endereco),
        Vizinhanca::TambemAqui => endereco.port() >= 1024,
    };
    let mut candidatos: Vec<SocketAddr> = chama
        .candidatos
        .iter()
        .copied()
        .filter(deixa)
        .take(LIMITE_DE_CANDIDATOS)
        .collect();
    if let Some(visto) = visto.filter(deixa) {
        if !candidatos.contains(&visto) {
            candidatos.push(visto);
        }
    }
    candidatos
}

/// O que o ponto sabe de uma marca no ENC/3: o endereço, e as dicas.
#[must_use]
pub fn onde_mora3(
    quarto: &crate::Quarto,
    marca: &Marca,
    agora: Instant,
) -> Option<(SocketAddr, Vec<SocketAddr>)> {
    if e_marca_de_sala(marca) {
        quarto.assinados3.onde_mora(marca, agora)
    } else {
        quarto
            .livres3
            .onde_mora(marca.texto(), agora)
            .map(|endereco| (endereco, Vec::new()))
    }
}

/// O que fazer com um `CHAMA` já contado na taxa.
///
/// `Ok(id)` quando o `CHAMADO` saiu: a chamada guardou `quem`, e a resposta
/// vai a ele pelo `ATENDE` (ou pelo `CALADO`).
///
/// # Errors
///
/// A resposta que sai na hora (`NINGUEM`, `RECUSA cheio`), com `quem` de volta
/// para entregá-la.
pub fn chamar(
    quarto: &crate::Quarto,
    chama: &Chama,
    visto: Option<SocketAddr>,
    vizinhanca: Vizinhanca,
    quem: QuemLiga,
    agora_unix: u64,
) -> Result<[u8; BYTES_DO_ID], Box<(QuemLiga, Resposta3)>> {
    let agora = Instant::now();
    let servidor = chama.servidor.clone();
    let achados =
        onde_mora3(quarto, &chama.servidor, agora).zip(onde_mora3(quarto, &chama.escuta, agora));
    // A escuta só vale no IP do servidor: a regra do ENC/1
    // (`docs/ponto-de-encontro.md`, «a escuta só vale no IP do servidor»).
    let Some(((endereco_do_servidor, _), (escuta, _))) =
        achados.filter(|((servidor, _), (escuta, _))| {
            servidor.ip().to_canonical() == escuta.ip().to_canonical()
        })
    else {
        return Err(Box::new((quem, Resposta3::Ninguem { marca: servidor })));
    };
    let mut id = [0_u8; BYTES_DO_ID];
    if getrandom::fill(&mut id).is_err() {
        // Sem sorteio não há `id` que um terceiro não adivinhe: melhor recusar.
        return Err(Box::new((
            quem,
            Resposta3::Recusa {
                marca: servidor,
                motivo: Motivo::Cheio,
                agora: agora_unix,
            },
        )));
    }
    let chamado = Resposta3::Chamado(Chamado {
        escuta: chama.escuta.clone(),
        id,
        credencial: chama.credencial.clone(),
        candidatos: candidatos_do_chamado(chama, visto, vizinhanca),
    });
    let pendente = Pendente {
        escuta_ip: escuta.ip(),
        servidor: servidor.clone(),
        endereco_do_servidor,
        desde: agora,
        quem,
    };
    if let Err(pendente) = quarto.chamadas.guardar(id, pendente) {
        return Err(Box::new((
            pendente.quem,
            Resposta3::Recusa {
                marca: servidor,
                motivo: Motivo::Cheio,
                agora: agora_unix,
            },
        )));
    }
    quarto.enviar(escuta, &resposta3(&chamado));
    Ok(id)
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;
    use seele_proto::encontro::Credencial;

    fn de(texto: &str) -> SocketAddr {
        texto.parse().unwrap()
    }

    #[test]
    fn o_quarto_livre_tem_as_regras_do_enc1_e_diz_a_recusa() {
        let livre = QuartoLivre::default();
        let agora = Instant::now();
        livre.morar("abcs", de("203.0.113.7:8383"), agora).unwrap();
        // O mesmo IP muda de porta.
        livre.morar("abcs", de("203.0.113.7:9000"), agora).unwrap();
        assert_eq!(livre.onde_mora("abcs", agora), Some(de("203.0.113.7:9000")));
        // Outro IP não toma enquanto vale, e ouve por quê.
        assert_eq!(
            livre.morar("abcs", de("198.51.100.1:8383"), agora),
            Err(Motivo::Velho)
        );
        // Vencido, toma.
        let depois = agora + PRAZO_DO_LIVRE;
        assert_eq!(livre.onde_mora("abcs", depois), None);
        livre
            .morar("abcs", de("198.51.100.1:8383"), depois)
            .unwrap();
    }

    #[test]
    fn o_chamado_leva_os_candidatos_que_a_vizinhanca_deixa_e_o_visto() {
        let chama = Chama {
            servidor: Marca::nova("as").unwrap(),
            escuta: Marca::nova("ae").unwrap(),
            credencial: Credencial::nova(&"a".repeat(32)).unwrap(),
            candidatos: vec![de("192.168.0.2:50000"), de("45.33.10.9:50000")],
        };
        let visto = Some(de("45.33.10.9:50001"));
        assert_eq!(
            candidatos_do_chamado(&chama, visto, Vizinhanca::Internet),
            vec![de("45.33.10.9:50000"), de("45.33.10.9:50001")],
            "na internet, a rede local de quem liga não vira alvo de furo"
        );
        assert_eq!(
            candidatos_do_chamado(&chama, visto, Vizinhanca::TambemAqui).len(),
            3
        );
        // O visto repetido não entra duas vezes.
        assert_eq!(
            candidatos_do_chamado(&chama, Some(de("45.33.10.9:50000")), Vizinhanca::Internet),
            vec![de("45.33.10.9:50000")]
        );
    }
}
