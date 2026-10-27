//! Caixa de sinalização pessoal: só convites e SDP/ICE assinados, em memória.
//! A caixa não atesta identidade: as pontas conferem novamente cada envelope.
//!
//! Limites: 256 registros (16 por prefixo), 128 chamadas/tombstones, 8 eventos
//! e 128 KiB por caixa; 8 MiB incluindo respostas de retry, 8192 nonces por no
//! máximo 60 s. Registros duram 60 s; chamadas sem atividade expiram em 120 s
//! (60 s enquanto tocam). Poll imediato a cada 2 s; Retry-After no HTTP 429.
//! Revogação impede novas admissões. Convite vencido não encerra chamada aceita.
//! A época é sorteada ao abrir o HTTP; reiniciar não restaura estado nem aceita
//! requests do processo anterior. Nada deste módulo é gravado ou logado.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::taxa::{prefixo, Classe, Decisao, Prefixo, Taxa};
use seele_proto::chamada::{self, Convite, Operacao, Papel, Pedido};
use sha2::{Digest, Sha256};

const REGISTROS: usize = 256;
const POR_PREFIXO: usize = 16;
const CHAMADAS: usize = 128;
const EVENTOS: usize = 8;
const BYTES_CAIXA: usize = 128 * 1024;
const BYTES_TOTAL: usize = 8 * 1024 * 1024;
const NONCES: usize = 8192;
const LEASE: Duration = Duration::from_secs(60);
const OCIOSA: Duration = Duration::from_secs(120);
const ZERO: [u8; 32] = [0; 32];
type Id = [u8; 32];

/// Uma recusa da caixa, sem informações sobre outras pessoas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recusa {
    Invalido,
    Assinatura,
    Epoca,
    Repetido,
    Cheio,
    Ausente,
    Estado,
    Ocupado,
}

/// Evento original, ainda assinado por quem o enviou.
#[derive(Debug, Clone)]
pub(crate) struct Evento {
    pub cursor: u64,
    pub pedido: Pedido,
    bytes: usize,
}

/// Resultado local; a serialização pertence ao protocolo.
#[derive(Debug, Clone)]
pub(crate) enum Resultado {
    Ok,
    Eventos(Vec<Evento>, u64),
    Recusa(Recusa),
}

impl Resultado {
    fn bytes(&self) -> usize {
        match self {
            Self::Eventos(e, _) => e.iter().map(|e| e.bytes).sum(),
            _ => 0,
        }
    }
    pub(crate) fn no_fio(self) -> chamada::Resposta {
        use chamada::{Motivo, Resposta};
        match self {
            Self::Ok => Resposta::Ok,
            Self::Eventos(eventos, cursor) => Resposta::Eventos {
                cursor,
                eventos: eventos
                    .into_iter()
                    .map(|e| chamada::Evento {
                        cursor: e.cursor,
                        pedido: e.pedido,
                    })
                    .collect(),
            },
            Self::Recusa(r) => Resposta::Recusa {
                motivo: match r {
                    Recusa::Invalido => Motivo::Invalido,
                    Recusa::Assinatura => Motivo::Assinatura,
                    Recusa::Epoca => Motivo::Epoca,
                    Recusa::Repetido => Motivo::Replay,
                    Recusa::Cheio => Motivo::Cheio,
                    Recusa::Ausente => Motivo::SemRegistro,
                    Recusa::Estado => Motivo::SemPermissao,
                    Recusa::Ocupado => Motivo::Ocupado,
                },
            },
        }
    }
}

#[derive(Debug, Default)]
struct Caixa {
    eventos: VecDeque<Evento>,
    proximo: u64,
    entregue: u64,
    confirmado: u64,
}
impl Caixa {
    fn bytes(&self) -> usize {
        self.eventos.iter().map(|e| e.bytes).sum()
    }
    fn cabe(&self, bytes: usize) -> bool {
        self.eventos.len() < EVENTOS && self.bytes().saturating_add(bytes) <= BYTES_CAIXA
    }
    fn colocar(&mut self, pedido: &Pedido, bytes: usize) {
        self.proximo += 1;
        self.eventos.push_back(Evento {
            cursor: self.proximo,
            pedido: pedido.clone(),
            bytes,
        });
    }
    fn buscar(&mut self, cursor: u64, confirmado: u64) -> Result<Resultado, Recusa> {
        if confirmado > self.entregue || cursor > self.entregue || confirmado > cursor {
            return Err(Recusa::Invalido);
        }
        self.confirmado = self.confirmado.max(confirmado);
        self.eventos.retain(|e| e.cursor > self.confirmado);
        let eventos: Vec<_> = self
            .eventos
            .iter()
            .filter(|e| e.cursor > cursor)
            .cloned()
            .collect();
        let ultimo = eventos.last().map_or(cursor, |e| e.cursor);
        self.entregue = self.entregue.max(ultimo);
        Ok(Resultado::Eventos(eventos, ultimo))
    }
}
#[derive(Debug)]
struct Registro {
    dono: Id,
    prefixo: Prefixo,
    convite: Convite,
    lease: Instant,
    revogado: bool,
    caixa: Caixa,
}
#[derive(Debug)]
struct Chamada {
    convite: Id,
    dono: Id,
    visitante: Id,
    geracao: u32,
    aceita: bool,
    atividade: Instant,
    encerrada: bool,
    seq_dono: u64,
    seq_visitante: u64,
    caixa: Caixa,
}
#[derive(Debug)]
struct Repeticao {
    ate: Instant,
    hash: [u8; 32],
    resposta: Resultado,
}
#[derive(Debug, Default)]
struct Estado {
    ultimo_unix: u64,
    registros: HashMap<Id, Registro>,
    chamadas: HashMap<Id, Chamada>,
    nonces: HashMap<(Id, [u8; 16]), Repeticao>,
    terminadas: HashMap<Id, Instant>,
}
impl Estado {
    fn varrer(&mut self, agora: Instant, unix: u64) {
        self.nonces.retain(|_, r| r.ate > agora);
        self.terminadas.retain(|_, prazo| *prazo > agora);
        let vencidas: Vec<_> = self
            .chamadas
            .iter()
            .filter(|(_, c)| {
                agora.saturating_duration_since(c.atividade)
                    >= if c.aceita { OCIOSA } else { LEASE }
            })
            .map(|(id, _)| *id)
            .collect();
        for id in vencidas {
            self.chamadas.remove(&id);
            self.terminadas.insert(id, agora + OCIOSA);
        }
        for r in self.registros.values_mut() {
            r.caixa.eventos.retain(|e| e.pedido.valido_ate > unix);
        }
        for c in self.chamadas.values_mut() {
            c.caixa.eventos.retain(|e| e.pedido.valido_ate > unix);
        }
        self.registros.retain(|id, r| {
            self.chamadas.values().any(|c| c.convite == *id)
                || if r.revogado {
                    r.convite.expira_em > unix
                } else {
                    r.lease > agora
                }
        });
    }
    fn bytes(&self) -> usize {
        self.registros
            .values()
            .map(|r| r.caixa.bytes())
            .sum::<usize>()
            + self
                .chamadas
                .values()
                .map(|c| c.caixa.bytes())
                .sum::<usize>()
            + self
                .nonces
                .values()
                .map(|r| r.resposta.bytes())
                .sum::<usize>()
    }
}

/// Estado separado do ENC/3; uma época nova invalida pedidos do processo anterior.
#[derive(Debug)]
pub struct ChamadasPessoais {
    epoca: Id,
    estado: Mutex<Estado>,
    taxa: Taxa,
    global: Taxa,
}
impl ChamadasPessoais {
    /// Sorteia a época. Sem entropia o endpoint não abre.
    pub fn nova() -> std::io::Result<Self> {
        let mut epoca = ZERO;
        getrandom::fill(&mut epoca).map_err(std::io::Error::other)?;
        Ok(Self {
            epoca,
            estado: Mutex::new(Estado::default()),
            taxa: Taxa::com(8, 16, 4096),
            global: Taxa::com(128, 256, 1),
        })
    }
    /// Época pública; não é credencial nem atestado de identidade.
    pub fn epoca(&self) -> Id {
        self.epoca
    }
    /// Orçamento antes de decodificar e verificar assinaturas.
    pub(crate) fn admitir(&self, ip: IpAddr, agora: Instant) -> bool {
        matches!(
            self.global
                .pedir(IpAddr::V4(Ipv4Addr::UNSPECIFIED), Classe::Chamada, agora),
            Decisao::Passa
        ) && matches!(self.taxa.pedir(ip, Classe::Chamada, agora), Decisao::Passa)
    }
    pub(crate) fn responder(
        &self,
        pedido: &Pedido,
        ip: IpAddr,
        agora: Instant,
        unix: u64,
    ) -> Resultado {
        self.aplicar(pedido, ip, agora, unix)
            .unwrap_or_else(Resultado::Recusa)
    }
    fn aplicar(
        &self,
        p: &Pedido,
        ip: IpAddr,
        agora: Instant,
        unix: u64,
    ) -> Result<Resultado, Recusa> {
        chamada::validar_pedido(p, unix).map_err(|_| Recusa::Invalido)?;
        if p.epoca != self.epoca {
            return Err(Recusa::Epoca);
        }
        let mensagem = chamada::mensagem_do_pedido(p).map_err(|_| Recusa::Invalido)?;
        if !confere(&p.remetente, &mensagem, &p.assinatura.bytes()) {
            return Err(Recusa::Assinatura);
        }
        let mut e = self.estado.lock().unwrap_or_else(PoisonError::into_inner);
        e.ultimo_unix = e.ultimo_unix.max(unix);
        let unix = e.ultimo_unix;
        chamada::validar_pedido(p, unix).map_err(|_| Recusa::Invalido)?;
        e.varrer(agora, unix);
        let nonce = (p.remetente, p.nonce);
        let hash: [u8; 32] = Sha256::digest(&mensagem).into();
        if let Some(r) = e.nonces.get(&nonce) {
            return if r.hash == hash {
                Ok(r.resposta.clone())
            } else {
                Err(Recusa::Repetido)
            };
        }
        // Reserva o maior retorno e um evento antes de qualquer mutação.
        if e.nonces.len() >= NONCES
            || e.bytes().saturating_add(BYTES_CAIXA + chamada::MAX_PEDIDO) > BYTES_TOTAL
        {
            return Err(Recusa::Cheio);
        }
        let resposta = self
            .aplicar_verificado(&mut e, p, ip, agora, unix)
            .unwrap_or_else(Resultado::Recusa);
        e.nonces.insert(
            nonce,
            Repeticao {
                ate: agora + Duration::from_secs(p.valido_ate.saturating_sub(unix)),
                hash,
                resposta: resposta.clone(),
            },
        );
        Ok(resposta)
    }
    fn aplicar_verificado(
        &self,
        e: &mut Estado,
        p: &Pedido,
        ip: IpAddr,
        agora: Instant,
        unix: u64,
    ) -> Result<Resultado, Recusa> {
        let bytes = chamada::escrever_pedido(p)
            .map_err(|_| Recusa::Invalido)?
            .len();
        match &p.operacao {
            Operacao::Registrar { convite } => {
                conferir_convite(convite, unix)?;
                if convite.id != p.convite
                    || convite.dono != p.remetente
                    || p.destinatario != p.remetente
                    || p.chamada != ZERO
                {
                    return Err(Recusa::Invalido);
                }
                if let Some(r) = e.registros.get_mut(&p.convite) {
                    if r.revogado || r.dono != p.remetente || r.convite != *convite {
                        return Err(Recusa::Estado);
                    }
                    r.lease = agora + LEASE;
                    return Ok(Resultado::Ok);
                }
                let pr = prefixo(ip);
                if e.registros.len() >= REGISTROS
                    || e.registros.values().filter(|r| r.prefixo == pr).count() >= POR_PREFIXO
                {
                    return Err(Recusa::Cheio);
                }
                e.registros.insert(
                    p.convite,
                    Registro {
                        dono: p.remetente,
                        prefixo: pr,
                        convite: convite.clone(),
                        lease: agora + LEASE,
                        revogado: false,
                        caixa: Caixa::default(),
                    },
                );
            }
            Operacao::Chamar { convite, .. } => {
                conferir_convite(convite, unix)?;
                if p.convite != convite.id
                    || p.destinatario != convite.dono
                    || p.remetente == convite.dono
                    || p.chamada == ZERO
                {
                    return Err(Recusa::Invalido);
                }
                if e.chamadas.contains_key(&p.chamada) || e.terminadas.contains_key(&p.chamada) {
                    return Err(Recusa::Estado);
                }
                if e.chamadas.len()
                    + e.terminadas
                        .keys()
                        .filter(|id| !e.chamadas.contains_key(*id))
                        .count()
                    >= CHAMADAS
                    || e.bytes().saturating_add(bytes) > BYTES_TOTAL
                {
                    return Err(Recusa::Cheio);
                }
                let r = e.registros.get_mut(&p.convite).ok_or(Recusa::Ausente)?;
                if r.revogado || r.lease <= agora || r.convite != *convite {
                    return Err(Recusa::Ausente);
                }
                if !r.caixa.cabe(bytes) {
                    return Err(Recusa::Cheio);
                }
                r.caixa.colocar(p, bytes);
                e.chamadas.insert(
                    p.chamada,
                    Chamada {
                        convite: p.convite,
                        dono: p.destinatario,
                        visitante: p.remetente,
                        geracao: p.geracao,
                        aceita: false,
                        atividade: agora,
                        encerrada: false,
                        seq_dono: 0,
                        seq_visitante: p.sequencia,
                        caixa: Caixa::default(),
                    },
                );
            }
            Operacao::Buscar { cursor, confirmado } => {
                if p.chamada == ZERO {
                    let r = e.registros.get_mut(&p.convite).ok_or(Recusa::Ausente)?;
                    if r.dono != p.remetente
                        || p.destinatario != p.remetente
                        || p.papel != Papel::QuemRecebe
                    {
                        return Err(Recusa::Assinatura);
                    }
                    r.lease = agora + LEASE;
                    return r.caixa.buscar(*cursor, *confirmado);
                }
                let c = e.chamadas.get_mut(&p.chamada).ok_or(Recusa::Ausente)?;
                if p.papel != Papel::QuemLiga
                    || c.convite != p.convite
                    || c.visitante != p.remetente
                    || c.dono != p.destinatario
                    || c.geracao != p.geracao
                {
                    return Err(Recusa::Assinatura);
                }
                if !c.encerrada {
                    c.atividade = agora;
                }
                return c.caixa.buscar(*cursor, *confirmado);
            }
            Operacao::Revogar => {
                let r = e.registros.get_mut(&p.convite).ok_or(Recusa::Ausente)?;
                if r.dono != p.remetente || p.destinatario != p.remetente || p.chamada != ZERO {
                    return Err(Recusa::Assinatura);
                }
                r.revogado = true;
                let ids: Vec<_> = e
                    .chamadas
                    .iter()
                    .filter(|(_, c)| c.convite == p.convite && !c.aceita)
                    .map(|(id, _)| *id)
                    .collect();
                for id in ids {
                    e.chamadas.remove(&id);
                    e.terminadas.insert(id, agora + OCIOSA);
                }
            }
            _ => self.da_chamada(e, p, bytes, agora, unix)?,
        }
        Ok(Resultado::Ok)
    }
    fn da_chamada(
        &self,
        e: &mut Estado,
        p: &Pedido,
        bytes: usize,
        agora: Instant,
        unix: u64,
    ) -> Result<(), Recusa> {
        let c = e.chamadas.get(&p.chamada).ok_or(Recusa::Ausente)?;
        if c.encerrada || e.terminadas.contains_key(&p.chamada) {
            return Err(Recusa::Estado);
        }
        let dono = p.remetente == c.dono && p.destinatario == c.visitante;
        let visitante = p.remetente == c.visitante && p.destinatario == c.dono;
        if (!dono && !visitante)
            || (dono && p.papel != Papel::QuemRecebe)
            || (visitante && p.papel != Papel::QuemLiga)
            || p.convite != c.convite
            || p.geracao != c.geracao
        {
            return Err(Recusa::Assinatura);
        }
        if p.sequencia <= if dono { c.seq_dono } else { c.seq_visitante } {
            return Err(Recusa::Repetido);
        }
        if matches!(p.operacao, Operacao::Atender)
            && e.chamadas.iter().any(|(id, outra)| {
                *id != p.chamada
                    && outra.aceita
                    && !outra.encerrada
                    && [outra.dono, outra.visitante]
                        .iter()
                        .any(|k| *k == c.dono || *k == c.visitante)
            })
        {
            return Err(Recusa::Ocupado);
        }
        let r = e.registros.get(&c.convite).ok_or(Recusa::Ausente)?;
        match p.operacao {
            Operacao::Atender if dono && !c.aceita && !r.revogado && r.convite.expira_em > unix => {
            }
            Operacao::Recusar if dono && !c.aceita => {}
            Operacao::Cancelar if visitante && !c.aceita => {}
            Operacao::Encerrar if c.aceita => {}
            Operacao::Sinal(_) if c.aceita => {}
            _ => return Err(Recusa::Estado),
        }
        if e.bytes().saturating_add(bytes) > BYTES_TOTAL {
            return Err(Recusa::Cheio);
        }
        let caixa = if dono {
            &mut e.chamadas.get_mut(&p.chamada).ok_or(Recusa::Ausente)?.caixa
        } else {
            &mut e
                .registros
                .get_mut(&p.convite)
                .ok_or(Recusa::Ausente)?
                .caixa
        };
        if !caixa.cabe(bytes) {
            return Err(Recusa::Cheio);
        }
        caixa.colocar(p, bytes);
        let c = e.chamadas.get_mut(&p.chamada).ok_or(Recusa::Ausente)?;
        if dono {
            c.seq_dono = p.sequencia;
        } else {
            c.seq_visitante = p.sequencia;
        }
        c.atividade = agora;
        match p.operacao {
            Operacao::Atender => c.aceita = true,
            Operacao::Recusar | Operacao::Cancelar | Operacao::Encerrar => {
                // Retém a caixa final por 60s para o par ler/confirmar; bloqueia nova atividade.
                c.encerrada = true;
                e.terminadas.insert(p.chamada, agora + OCIOSA);
            }
            _ => {}
        }
        Ok(())
    }
}
fn confere(chave: &Id, mensagem: &[u8], assinatura: &[u8; 64]) -> bool {
    ed25519_dalek::VerifyingKey::from_bytes(chave).is_ok_and(|chave| {
        !chave.is_weak()
            && chave
                .verify_strict(mensagem, &ed25519_dalek::Signature::from_bytes(assinatura))
                .is_ok()
    })
}
fn conferir_convite(convite: &Convite, unix: u64) -> Result<(), Recusa> {
    chamada::validar_convite(convite, unix).map_err(|_| Recusa::Invalido)?;
    if confere(
        &convite.dono,
        &chamada::mensagem_do_convite(convite),
        &convite.assinatura.bytes(),
    ) {
        Ok(())
    } else {
        Err(Recusa::Assinatura)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "fixtures sintéticas e asserções de teste"
)]
mod testes {
    use super::*;
    use chamada::{Assinatura, Sinal};
    use ed25519_dalek::{Signer, SigningKey};
    fn chave(n: u8) -> SigningKey {
        SigningKey::from_bytes(&[n; 32])
    }
    fn convite(dono: &SigningKey) -> Convite {
        let mut c = Convite {
            id: [9; 32],
            dono: dono.verifying_key().to_bytes(),
            expira_em: 1200,
            assinatura: Assinatura::default(),
        };
        c.assinatura =
            Assinatura::de_bytes(dono.sign(&chamada::mensagem_do_convite(&c)).to_bytes());
        c
    }
    fn assinar(p: &mut Pedido, chave: &SigningKey) {
        p.assinatura = Assinatura::de_bytes(
            chave
                .sign(&chamada::mensagem_do_pedido(p).unwrap())
                .to_bytes(),
        );
    }
    fn pedido(
        s: &ChamadasPessoais,
        origem: &SigningKey,
        destino: Id,
        n: u8,
        op: Operacao,
    ) -> Pedido {
        let de = origem.verifying_key().to_bytes();
        let papel = if de == chave(1).verifying_key().to_bytes() {
            Papel::QuemRecebe
        } else {
            Papel::QuemLiga
        };
        let mut p = Pedido {
            epoca: s.epoca(),
            nonce: [n; 16],
            convite: [9; 32],
            chamada: [7; 32],
            remetente: de,
            destinatario: destino,
            geracao: 1,
            sequencia: u64::from(n),
            valido_ate: 1060,
            papel,
            operacao: op,
            assinatura: Assinatura::default(),
        };
        if matches!(p.operacao, Operacao::Registrar { .. } | Operacao::Revogar) {
            p.chamada = ZERO;
        }
        assinar(&mut p, origem);
        p
    }
    fn enviar(s: &ChamadasPessoais, p: &Pedido, t: Instant, unix: u64) -> Resultado {
        s.responder(p, "127.0.0.1".parse().unwrap(), t, unix)
    }
    fn ok(r: Resultado) {
        assert!(matches!(r, Resultado::Ok), "{r:?}");
    }
    fn base() -> (ChamadasPessoais, Instant, Convite) {
        let s = ChamadasPessoais::nova().unwrap();
        let t = Instant::now();
        let c = convite(&chave(1));
        let p = pedido(
            &s,
            &chave(1),
            c.dono,
            1,
            Operacao::Registrar { convite: c.clone() },
        );
        ok(enviar(&s, &p, t, 1000));
        (s, t, c)
    }
    fn chamar(s: &ChamadasPessoais, t: Instant, c: &Convite) {
        ok(enviar(
            s,
            &pedido(
                s,
                &chave(2),
                c.dono,
                2,
                Operacao::Chamar {
                    convite: c.clone(),
                    apelido: "Pessoa".into(),
                },
            ),
            t,
            1000,
        ));
    }
    fn poll(s: &ChamadasPessoais, quem: u8, n: u8, cursor: u64, confirmado: u64) -> Pedido {
        let mut p = pedido(
            s,
            &chave(quem),
            chave(1).verifying_key().to_bytes(),
            n,
            Operacao::Buscar { cursor, confirmado },
        );
        if quem == 1 {
            p.chamada = ZERO;
        }
        assinar(&mut p, &chave(quem));
        p
    }
    #[test]
    fn ambas_pontas_assinam_e_o_aceite_precede_sinal_sem_consumir_a_leitura() {
        let (s, t, c) = base();
        chamar(&s, t, &c);
        let p = poll(&s, 1, 3, 0, 0);
        let Resultado::Eventos(eventos, cursor) = enviar(&s, &p, t, 1000) else {
            panic!()
        };
        assert_eq!(cursor, 1);
        assert_eq!(eventos.len(), 1);
        assert!(confere(
            &eventos[0].pedido.remetente,
            &chamada::mensagem_do_pedido(&eventos[0].pedido).unwrap(),
            &eventos[0].pedido.assinatura.bytes()
        ));
        let Resultado::Eventos(de_novo, _) = enviar(&s, &p, t, 1000) else {
            panic!()
        };
        assert_eq!(de_novo[0].pedido, eventos[0].pedido);
        let Resultado::Eventos(ack, _) = enviar(&s, &poll(&s, 1, 4, 1, 1), t, 1000) else {
            panic!()
        };
        assert!(ack.is_empty());
        let sinal = pedido(
            &s,
            &chave(2),
            c.dono,
            5,
            Operacao::Sinal(Sinal::FimDosCandidatos),
        );
        assert!(matches!(
            enviar(&s, &sinal, t, 1000),
            Resultado::Recusa(Recusa::Estado)
        ));
        ok(enviar(
            &s,
            &pedido(
                &s,
                &chave(1),
                chave(2).verifying_key().to_bytes(),
                6,
                Operacao::Atender,
            ),
            t,
            1000,
        ));
        let Resultado::Eventos(eventos, _) = enviar(&s, &poll(&s, 2, 7, 0, 0), t, 1000) else {
            panic!()
        };
        assert!(matches!(eventos[0].pedido.operacao, Operacao::Atender));
        ok(enviar(
            &s,
            &pedido(
                &s,
                &chave(2),
                c.dono,
                8,
                Operacao::Sinal(Sinal::FimDosCandidatos),
            ),
            t,
            1000,
        ));
    }
    #[test]
    fn nonce_alterado_chave_fraca_e_outra_epoca_nao_mutam_caixa() {
        let (s, t, c) = base();
        let mut p = pedido(
            &s,
            &chave(2),
            c.dono,
            2,
            Operacao::Chamar {
                convite: c,
                apelido: "Pessoa".into(),
            },
        );
        p.nonce = [3; 16];
        assert!(matches!(
            enviar(&s, &p, t, 1000),
            Resultado::Recusa(Recusa::Assinatura)
        ));
        p.remetente = [0; 32];
        p.remetente[0] = 1;
        p.assinatura = Assinatura::default();
        p.assinatura.primeira[0] = 1;
        assert!(matches!(
            enviar(&s, &p, t, 1000),
            Resultado::Recusa(Recusa::Assinatura)
        ));
        let outro = ChamadasPessoais::nova().unwrap();
        p.epoca = s.epoca();
        assert!(matches!(
            enviar(&outro, &p, t, 1000),
            Resultado::Recusa(Recusa::Epoca)
        ));
        assert!(s.estado.lock().unwrap().chamadas.is_empty());
    }
    #[test]
    fn retry_nao_renova_registro_nonce_reutilizado_nao_muda_pedido() {
        let (s, t, c) = base();
        let p = pedido(&s, &chave(1), c.dono, 1, Operacao::Registrar { convite: c });
        ok(enviar(&s, &p, t + Duration::from_secs(40), 1040));
        assert_eq!(
            s.estado
                .lock()
                .unwrap()
                .registros
                .get(&[9; 32])
                .unwrap()
                .lease,
            t + LEASE
        );
        let mut alterado = p;
        alterado.valido_ate = 1059;
        assinar(&mut alterado, &chave(1));
        assert!(matches!(
            enviar(&s, &alterado, t, 1000),
            Resultado::Recusa(Recusa::Repetido)
        ));
    }
    #[test]
    fn terminal_nao_ressuscita_e_retry_preserva_resposta_final() {
        let (s, t, c) = base();
        chamar(&s, t, &c);
        let fim = pedido(&s, &chave(2), c.dono, 3, Operacao::Cancelar);
        ok(enviar(&s, &fim, t, 1000));
        ok(enviar(&s, &fim, t, 1000));
        let atende = pedido(
            &s,
            &chave(1),
            chave(2).verifying_key().to_bytes(),
            4,
            Operacao::Atender,
        );
        assert!(matches!(
            enviar(&s, &atende, t, 1000),
            Resultado::Recusa(Recusa::Estado)
        ));
        let Resultado::Eventos(eventos, _) = enviar(&s, &poll(&s, 1, 5, 0, 0), t, 1000) else {
            panic!()
        };
        assert!(eventos
            .iter()
            .any(|e| matches!(e.pedido.operacao, Operacao::Cancelar)));
    }
    #[test]
    fn revogar_fecha_admissao_sem_encerrar_chamada_aceita() {
        let (s, t, c) = base();
        chamar(&s, t, &c);
        ok(enviar(
            &s,
            &pedido(
                &s,
                &chave(1),
                chave(2).verifying_key().to_bytes(),
                3,
                Operacao::Atender,
            ),
            t,
            1000,
        ));
        ok(enviar(
            &s,
            &pedido(&s, &chave(1), c.dono, 4, Operacao::Revogar),
            t,
            1000,
        ));
        ok(enviar(
            &s,
            &pedido(
                &s,
                &chave(2),
                c.dono,
                5,
                Operacao::Sinal(Sinal::FimDosCandidatos),
            ),
            t,
            1000,
        ));
        let mut novo = pedido(
            &s,
            &chave(3),
            c.dono,
            6,
            Operacao::Chamar {
                convite: c,
                apelido: "Outra".into(),
            },
        );
        novo.chamada = [8; 32];
        assinar(&mut novo, &chave(3));
        assert!(matches!(
            enviar(&s, &novo, t, 1000),
            Resultado::Recusa(Recusa::Ausente)
        ));
    }
    #[test]
    fn prazo_registro_e_convite_nao_sao_ressuscitados_por_poll() {
        let (s, t, _) = base();
        let mut p = poll(&s, 1, 4, 0, 0);
        p.valido_ate = 1121;
        assinar(&mut p, &chave(1));
        assert!(matches!(
            enviar(&s, &p, t + Duration::from_secs(61), 1061),
            Resultado::Recusa(Recusa::Ausente)
        ));
    }
    #[test]
    fn caixa_cheia_nao_apaga_eventos_ou_confirma_cursor_nao_entregue() {
        let (s, t, c) = base();
        for n in 2..=9 {
            let mut p = pedido(
                &s,
                &chave(2),
                c.dono,
                n,
                Operacao::Chamar {
                    convite: c.clone(),
                    apelido: "Pessoa".into(),
                },
            );
            p.chamada = [n; 32];
            assinar(&mut p, &chave(2));
            ok(enviar(&s, &p, t, 1000));
        }
        let mut p = pedido(
            &s,
            &chave(2),
            c.dono,
            10,
            Operacao::Chamar {
                convite: c,
                apelido: "Pessoa".into(),
            },
        );
        p.chamada = [10; 32];
        assinar(&mut p, &chave(2));
        assert!(matches!(
            enviar(&s, &p, t, 1000),
            Resultado::Recusa(Recusa::Cheio)
        ));
        assert!(matches!(
            enviar(&s, &poll(&s, 1, 11, 8, 8), t, 1000),
            Resultado::Recusa(Recusa::Invalido)
        ));
        assert_eq!(
            s.estado
                .lock()
                .unwrap()
                .registros
                .get(&[9; 32])
                .unwrap()
                .caixa
                .eventos
                .len(),
            8
        );
    }
    #[test]
    fn dois_pares_no_mesmo_nat_poll_2s_e_rajada_limitada() {
        let s = ChamadasPessoais::nova().unwrap();
        let t = Instant::now();
        let ip = "192.0.2.1".parse().unwrap();
        for segundo in (0..30).step_by(2) {
            for _ in 0..4 {
                assert!(s.admitir(ip, t + Duration::from_secs(segundo)));
            }
        }
        let ultimo = t + Duration::from_secs(30);
        for _ in 0..16 {
            assert!(s.admitir(ip, ultimo));
        }
        assert!(!s.admitir(ip, ultimo));
    }
    #[test]
    fn convite_expira_sem_encerrar_aceita_e_outro_par_nao_le_caixa() {
        let (s, t, c) = base();
        chamar(&s, t, &c);
        ok(enviar(
            &s,
            &pedido(
                &s,
                &chave(1),
                chave(2).verifying_key().to_bytes(),
                3,
                Operacao::Atender,
            ),
            t,
            1000,
        ));
        let terceiro = pedido(
            &s,
            &chave(3),
            c.dono,
            4,
            Operacao::Buscar {
                cursor: 0,
                confirmado: 0,
            },
        );
        assert!(matches!(
            enviar(&s, &terceiro, t, 1000),
            Resultado::Recusa(Recusa::Assinatura)
        ));
        for (n, segundos) in [(5, 50), (6, 100), (7, 150), (8, 201)] {
            let mut p = poll(&s, 2, n, 0, 0);
            p.valido_ate = 1000 + segundos + 60;
            assinar(&mut p, &chave(2));
            assert!(matches!(
                enviar(&s, &p, t + Duration::from_secs(segundos), 1000 + segundos),
                Resultado::Eventos(..)
            ));
        }
        let mut p = pedido(
            &s,
            &chave(2),
            c.dono,
            9,
            Operacao::Sinal(Sinal::FimDosCandidatos),
        );
        p.valido_ate = 1261;
        assinar(&mut p, &chave(2));
        ok(enviar(&s, &p, t + Duration::from_secs(201), 1201));
    }
    #[test]
    fn aceite_nao_abre_segunda_chamada_para_pessoa_ocupada() {
        let (s, t, c) = base();
        chamar(&s, t, &c);
        ok(enviar(
            &s,
            &pedido(
                &s,
                &chave(1),
                chave(2).verifying_key().to_bytes(),
                3,
                Operacao::Atender,
            ),
            t,
            1000,
        ));
        let mut outra = pedido(
            &s,
            &chave(3),
            c.dono,
            4,
            Operacao::Chamar {
                convite: c,
                apelido: "Outra".into(),
            },
        );
        outra.chamada = [8; 32];
        assinar(&mut outra, &chave(3));
        ok(enviar(&s, &outra, t, 1000));
        let mut atende = pedido(
            &s,
            &chave(1),
            chave(3).verifying_key().to_bytes(),
            5,
            Operacao::Atender,
        );
        atende.chamada = [8; 32];
        assinar(&mut atende, &chave(1));
        assert!(matches!(
            enviar(&s, &atende, t, 1000),
            Resultado::Recusa(Recusa::Ocupado)
        ));
    }
}
