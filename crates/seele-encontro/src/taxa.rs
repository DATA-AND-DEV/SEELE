//! O limite de pedidos do ENC/2 por prefixo (D-enc-1, decidido pelo dono em
//! 08/10/2026; ADR 0062).
//!
//! Um balde por `/32` no IPv4 e por `/64` no IPv6, que enche a [`POR_SEGUNDO`]
//! pedidos por segundo até [`RAJADA`]. Vale **só para a versão 2**: o ENC/1 não
//! muda, e a 0.15 não muda.
//!
//! **Dois baldes por prefixo, um para cada [`Classe`]**: o registro (`MORO2`)
//! e a pergunta (`QUEM2`). Com um balde só, nove `QUEM2` por segundo de um
//! vizinho de CGNAT, ou de quem forja o IP de alguém (o IP que o próprio `QUEM2`
//! entrega a quem tem o código dela), calavam os `MORO2` daquela rede, e a sala
//! sumia do quarto em 60 s (a revisão de 08/10/2026). A pergunta é o pedido
//! mais barato de mandar, e ela não esvazia mais o balde do registro.
//!
//! O que sobra: quem forja a origem ainda consegue esvaziar o balde do registro
//! de uma rede com `MORO2` de chaves dele, porque o limite vem antes da
//! assinatura (é ela que custa). Só uma prova de ida e volta (um cookie)
//! fecha isso, e ela muda o fio (ADR 0062).
//!
//! # Por que estes números
//!
//! Um app aberto da 0.16 registra a sala sempre: dois `MORO2` a cada quinze
//! segundos, 0,133 por segundo. Oito por segundo aguentam 60 SEELE abertos
//! atrás do mesmo IPv4 de CGNAT, para sempre. O que conta num CGNAT não é
//! quantos assinantes dividem o IP, e sim quantos SEELE abertos o dividem. A
//! rajada de 64 cobre dez ligações tentadas ao mesmo tempo (seis `QUEM2` cada).
//!
//! Um prefixo sozinho renova no máximo 8 × 60 = 480 marcas no prazo do quarto,
//! então encher o quarto assinado pede dezenas de prefixos. Um `/48` IPv6 tem
//! 65 536 `/64`, e um limite por `/48` é o segundo degrau, se um dia for
//! preciso.
//!
//! # Quem passa do limite ouve
//!
//! [`Decisao::Recusa`] com `dizer` uma vez por segundo por prefixo, e o ponto
//! responde `RECUSA taxa`: o anfitrião diz no `seele.log` que o limite é por
//! endereço público, e quem liga ouve que o ponto recusou. O resto da
//! enxurrada é calado, porque uma recusa para cada pedido faria do ponto um
//! refletor 1:1 de quem forja a origem.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Quantos pedidos por segundo cada prefixo ganha.
pub const POR_SEGUNDO: u64 = 8;

/// Quantos pedidos cabem no balde cheio.
pub const RAJADA: u64 = 64;

/// Quantos prefixos a tabela guarda.
///
/// Um balde cheio está ocioso e sai na varredura; só um ataque com mais de
/// 65 536 prefixos ativos ao mesmo tempo enche a tabela, e aí o prefixo novo
/// ouve a recusa (perto de 4,6 MB, cheia).
pub const PREFIXOS: usize = 65_536;

/// De quanto em quanto tempo um prefixo acima do limite ouve a recusa.
const INTERVALO_DAS_RECUSAS: Duration = Duration::from_secs(1);

/// Um pedido, em milésimos de pedido: o balde conta assim para não usar ponto
/// flutuante.
const MIL: u64 = 1000;

/// De que pedido se trata: cada classe tem o seu balde em cada prefixo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Classe {
    /// Um `MORO2`: o registro de uma sala.
    Registro,
    /// Um `QUEM2`: a pergunta por uma sala.
    Pergunta,
}

/// O que fazer com um pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    /// Dentro do limite.
    Passa,
    /// Acima do limite.
    Recusa {
        /// Se este pedido ouve a `RECUSA taxa`, ou é calado.
        dizer: bool,
    },
}

/// O pedaço do endereço que divide um balde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Prefixo {
    V4(u32),
    V6(u64),
}

/// O `/32` de um IPv4, ou o `/64` de um IPv6. Um IPv4 escrito como IPv6
/// mapeado é o IPv4 dele.
fn prefixo(ip: IpAddr) -> Prefixo {
    match ip.to_canonical() {
        IpAddr::V4(quatro) => Prefixo::V4(u32::from(quatro)),
        IpAddr::V6(seis) => Prefixo::V6(u64::try_from(u128::from(seis) >> 64).unwrap_or(u64::MAX)),
    }
}

#[derive(Debug)]
struct Balde {
    milesimos: u64,
    /// Até quando o enchimento já foi contado.
    contado: Instant,
    recusou: Option<Instant>,
}

impl Balde {
    /// Põe no balde o que encheu desde a última conta.
    ///
    /// Conta milissegundos inteiros e **avança só o que contou**: a fração que
    /// sobra fica para a próxima. Contando a partir de agora, pedidos mais
    /// próximos que um milissegundo nunca enchiam nada.
    fn encher(&mut self, agora: Instant, por_segundo: u64, cheio: u64) {
        let passou = agora.saturating_duration_since(self.contado);
        let ms = u64::try_from(passou.as_millis()).unwrap_or(u64::MAX);
        self.milesimos = self
            .milesimos
            .saturating_add(ms.saturating_mul(por_segundo))
            .min(cheio);
        self.contado = if self.milesimos >= cheio {
            agora
        } else {
            self.contado
                .checked_add(Duration::from_millis(ms))
                .unwrap_or(agora)
        };
    }
}

/// O limite por prefixo. Ver o cabeçalho do módulo.
#[derive(Debug)]
pub struct Taxa {
    baldes: Mutex<HashMap<(Prefixo, Classe), Balde>>,
    por_segundo: u64,
    rajada: u64,
    prefixos: usize,
}

impl Default for Taxa {
    fn default() -> Self {
        Self::nova()
    }
}

impl Taxa {
    /// O limite com [`POR_SEGUNDO`], [`RAJADA`] e [`PREFIXOS`].
    #[must_use]
    pub fn nova() -> Self {
        Self::com(POR_SEGUNDO, RAJADA, PREFIXOS)
    }

    /// O limite com outros valores, para o teste.
    #[must_use]
    pub fn com(por_segundo: u64, rajada: u64, prefixos: usize) -> Self {
        Self {
            baldes: Mutex::new(HashMap::new()),
            por_segundo,
            rajada,
            prefixos,
        }
    }

    /// Conta um pedido desta classe, deste endereço, e diz o que fazer com ele.
    pub fn pedir(&self, de: IpAddr, classe: Classe, agora: Instant) -> Decisao {
        // Um `Mutex` envenenado é um pânico noutra linha com a tabela na mão.
        // A tabela não fica inconsistente (cada conta é inteira), e recusar
        // dali em diante, dizendo a cada pedido, seria um refletor sem limite.
        let mut baldes = self.baldes.lock().unwrap_or_else(PoisonError::into_inner);
        let cheio = self.rajada.saturating_mul(MIL);
        let chave = (prefixo(de), classe);
        if !baldes.contains_key(&chave) && baldes.len() >= self.prefixos {
            baldes.retain(|_, balde| {
                balde.encher(agora, self.por_segundo, cheio);
                balde.milesimos < cheio
            });
            if baldes.len() >= self.prefixos {
                // Sem o teto de uma por segundo: este prefixo não tem balde
                // onde anotá-lo. A recusa tem o tamanho do pedido, como o
                // `ONDE` de sempre.
                return Decisao::Recusa { dizer: true };
            }
        }
        let balde = baldes.entry(chave).or_insert(Balde {
            milesimos: cheio,
            contado: agora,
            recusou: None,
        });
        balde.encher(agora, self.por_segundo, cheio);
        if balde.milesimos >= MIL {
            balde.milesimos -= MIL;
            return Decisao::Passa;
        }
        let dizer = balde
            .recusou
            .is_none_or(|antes| agora.saturating_duration_since(antes) >= INTERVALO_DAS_RECUSAS);
        if dizer {
            balde.recusou = Some(agora);
        }
        Decisao::Recusa { dizer }
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

    fn ip(texto: &str) -> IpAddr {
        texto.parse().unwrap()
    }

    #[test]
    fn sessenta_apps_atras_do_mesmo_ip_nunca_sao_recusados() {
        // A conta do D-enc-1: cada app aberto da 0.16 manda 2 MORO2 a cada
        // 15 s. Sessenta deles atrás do mesmo IPv4 de CGNAT, por dez minutos,
        // cada um no seu instante do ciclo.
        let taxa = Taxa::nova();
        let comeco = Instant::now();
        let mut pedidos: Vec<Duration> = (0..60_u64)
            .flat_map(|app| {
                (0..40_u64).flat_map(move |tique| {
                    let quando = Duration::from_millis(tique * 15_000 + app * 250);
                    [quando, quando]
                })
            })
            .collect();
        pedidos.sort();
        for quando in pedidos {
            assert_eq!(
                taxa.pedir(ip("100.64.0.1"), Classe::Pergunta, comeco + quando),
                Decisao::Passa,
                "um dos 60 apps atrás do mesmo IP foi recusado em {quando:?}"
            );
        }
    }

    #[test]
    fn uma_enxurrada_e_recusada_e_a_recusa_e_dita_uma_vez_por_segundo() {
        let taxa = Taxa::nova();
        let comeco = Instant::now();
        let (mut passou, mut ditas, mut caladas) = (0_u64, 0, 0);
        for i in 0..1000_u64 {
            match taxa.pedir(
                ip("203.0.113.7"),
                Classe::Pergunta,
                comeco + Duration::from_micros(i * 999),
            ) {
                Decisao::Passa => passou += 1,
                Decisao::Recusa { dizer: true } => ditas += 1,
                Decisao::Recusa { dizer: false } => caladas += 1,
            }
        }
        assert!(
            (RAJADA..=RAJADA + POR_SEGUNDO).contains(&passou),
            "passaram {passou} pedidos num segundo"
        );
        assert_eq!(ditas, 1, "a recusa foi dita {ditas} vezes num segundo");
        assert!(caladas > 900, "só {caladas} recusas caladas");
    }

    #[test]
    fn o_balde_enche_mesmo_com_pedidos_mais_rapidos_que_um_milissegundo() {
        // Com o balde vazio e um pedido a cada 100 µs por dois segundos, passam
        // os 16 que o enchimento de 8 por segundo dá. Contar só milissegundos
        // inteiros entre um pedido e o seguinte não enchia nada.
        let taxa = Taxa::com(8, 1, 16);
        let comeco = Instant::now();
        assert_eq!(
            taxa.pedir(ip("192.0.2.1"), Classe::Pergunta, comeco),
            Decisao::Passa
        );
        let passou = (1..=20_000_u64)
            .filter(|i| {
                taxa.pedir(
                    ip("192.0.2.1"),
                    Classe::Pergunta,
                    comeco + Duration::from_micros(i * 100),
                ) == Decisao::Passa
            })
            .count();
        assert!(
            (15..=16).contains(&passou),
            "passaram {passou} em dois segundos"
        );
    }

    #[test]
    fn o_ipv6_conta_por_64_e_o_ipv4_por_endereco() {
        let taxa = Taxa::com(1, 1, 16);
        let agora = Instant::now();
        assert_eq!(
            taxa.pedir(ip("2001:db8:1:2::1"), Classe::Pergunta, agora),
            Decisao::Passa
        );
        assert!(
            matches!(
                taxa.pedir(ip("2001:db8:1:2::ffff"), Classe::Pergunta, agora),
                Decisao::Recusa { .. }
            ),
            "o mesmo /64 não dividiu o balde"
        );
        assert_eq!(
            taxa.pedir(ip("2001:db8:1:3::1"), Classe::Pergunta, agora),
            Decisao::Passa,
            "outro /64 dividiu o balde"
        );
        assert_eq!(
            taxa.pedir(ip("192.0.2.1"), Classe::Pergunta, agora),
            Decisao::Passa
        );
        assert_eq!(
            taxa.pedir(ip("192.0.2.2"), Classe::Pergunta, agora),
            Decisao::Passa,
            "outro IPv4 dividiu o balde"
        );
        assert!(
            matches!(
                taxa.pedir(ip("::ffff:192.0.2.1"), Classe::Pergunta, agora),
                Decisao::Recusa { .. }
            ),
            "o IPv4 mapeado não contou como o IPv4 dele"
        );
    }

    #[test]
    fn a_tabela_tem_teto_e_varre_os_ociosos() {
        let taxa = Taxa::com(1, 1, 2);
        let agora = Instant::now();
        assert_eq!(
            taxa.pedir(ip("192.0.2.1"), Classe::Pergunta, agora),
            Decisao::Passa
        );
        assert_eq!(
            taxa.pedir(ip("192.0.2.2"), Classe::Pergunta, agora),
            Decisao::Passa
        );
        assert_eq!(
            taxa.pedir(ip("192.0.2.3"), Classe::Pergunta, agora),
            Decisao::Recusa { dizer: true },
            "um prefixo novo, com a tabela cheia de prefixos ativos, não ouviu a recusa"
        );
        // Um segundo depois os dois baldes encheram de novo: estão ociosos, e
        // saem da tabela para o novo entrar.
        let depois = agora + Duration::from_secs(1);
        assert_eq!(
            taxa.pedir(ip("192.0.2.3"), Classe::Pergunta, depois),
            Decisao::Passa
        );
    }

    #[test]
    fn uma_enxurrada_de_perguntas_nao_cala_o_registro() {
        // O achado da revisão de 08/10: com um balde só, nove QUEM2 por
        // segundo de um vizinho de CGNAT, ou de quem forja o IP de alguém (o
        // IP que o próprio QUEM2 entrega a quem tem o código), calavam os MORO2
        // daquela rede, e a sala sumia em 60 s. A pergunta e o registro têm
        // baldes separados.
        let taxa = Taxa::com(1, 1, 16);
        let agora = Instant::now();
        assert_eq!(
            taxa.pedir(ip("203.0.113.7"), Classe::Pergunta, agora),
            Decisao::Passa
        );
        assert!(matches!(
            taxa.pedir(ip("203.0.113.7"), Classe::Pergunta, agora),
            Decisao::Recusa { .. }
        ));
        assert_eq!(
            taxa.pedir(ip("203.0.113.7"), Classe::Registro, agora),
            Decisao::Passa,
            "as perguntas esvaziaram o balde do registro"
        );
        assert!(matches!(
            taxa.pedir(ip("203.0.113.7"), Classe::Registro, agora),
            Decisao::Recusa { .. }
        ));
    }

    #[test]
    fn um_mutex_envenenado_nao_vira_recusa_sem_limite() {
        let taxa = std::sync::Arc::new(Taxa::com(1, 1, 16));
        let outra = std::sync::Arc::clone(&taxa);
        let _ = std::thread::spawn(move || {
            let _guarda = outra.baldes.lock().unwrap();
            panic!("pânico de propósito, com a tabela na mão");
        })
        .join();
        assert!(taxa.baldes.is_poisoned(), "o teste não envenenou o Mutex");
        let agora = Instant::now();
        assert_eq!(
            taxa.pedir(ip("192.0.2.1"), Classe::Pergunta, agora),
            Decisao::Passa
        );
        assert_eq!(
            taxa.pedir(ip("192.0.2.1"), Classe::Pergunta, agora),
            Decisao::Recusa { dizer: true }
        );
        assert_eq!(
            taxa.pedir(ip("192.0.2.1"), Classe::Pergunta, agora),
            Decisao::Recusa { dizer: false },
            "com o Mutex envenenado, toda recusa passou a ser dita"
        );
    }

    #[test]
    fn os_valores_sao_os_da_decisao_do_dono() {
        // D-enc-1, decidido pelo dono em 08/10/2026 (ADR 0062).
        assert_eq!((POR_SEGUNDO, RAJADA, PREFIXOS), (8, 64, 65_536));
    }
}
