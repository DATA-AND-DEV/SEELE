//! A campainha da sala pessoal: quem está tocando, e o que quem recebe decidiu.
//!
//! Spec de 23/09, «A campainha», e o ADR 0059. É a irmã de
//! [`crate::portaria`] para a sala pessoal, e o contrário dela numa coisa: a
//! portaria foi desenhada para «nada espera» (ADR 0030), e a campainha existe
//! para **segurar** quem liga enquanto o app de quem recebe toca.
//!
//! # Só a decisão
//!
//! Nada aqui lê relógio, abre conexão ou manda mensagem. O instante entra por
//! parâmetro, no molde de [`crate::taxa`], e cada mudança volta como valor
//! para quem chamou: a sessão segura a conexão e manda a razão (o aperto em
//! três fases), e a hospedagem avisa o app (`Tocou` e `ParouDeTocar`). É o
//! que deixa provar cada desfecho sem um único `sleep`.
//!
//! # A chave de cada ligação
//!
//! A chave pública de quem liga, crua. O código e a impressão saem dela, e a
//! deduplicação é por ela: os candidatos da corrida (ADR 0037) chegam com a
//! mesma chave e não podem virar três toques.
//!
//! # Por ligação
//!
//! Uma aprovação vale para esta ligação. [`Campainha::desligar`] apaga todas,
//! e quem estava dentro recebe `ChamadaEncerrada`. Quem cai e volta em até
//! [`TOLERANCIA_DE_RECONEXAO`] entra sem tocar de novo.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use seele_proto::codigo::Codigo;
use seele_proto::control::DisconnectReason;

/// Quanto tempo um toque fica tocando antes de virar «não atendeu».
pub const PRAZO_DO_TOQUE: Duration = Duration::from_secs(45);

/// Quantos toques podem estar pendentes ao mesmo tempo.
///
/// Além disso, quem liga recebe `RateLimited` («muita gente ligando agora;
/// tente de novo»). Uma faixa de toque com dez nomes não é uma pergunta que
/// alguém consiga responder.
pub const TOQUES_PENDENTES: usize = 3;

/// Quantas pessoas cabem na ligação, **contando quem recebe**.
pub const TETO_DA_LIGACAO: usize = 6;

/// Quanto tempo a mesma chave espera depois de uma recusa.
///
/// Uma recusa que se pudesse refazer na hora seria um toque em laço.
pub const ESPERA_DEPOIS_DE_RECUSA: Duration = Duration::from_secs(60);

/// Quanto tempo quem já foi atendido pode cair e voltar sem tocar de novo.
pub const TOLERANCIA_DE_RECONEXAO: Duration = Duration::from_secs(60);

/// A chave pública Ed25519 de quem liga, crua.
pub type Chave = [u8; 32];

/// Um toque, do começo ao fim. Uma chave que liga de novo enquanto toca ganha
/// um número novo, e o velho deixa de existir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdDoToque(u64);

/// Uma ligação tocando agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toque {
    /// Qual toque.
    pub id: IdDoToque,
    /// A chave de quem liga.
    pub chave: Chave,
    /// O código dela, que é a prova. O apelido é só o que a pessoa diz.
    pub codigo: Codigo,
    /// O apelido que veio no `Hello`: uma alegação, e a faixa o mostra entre aspas.
    pub apelido: String,
    comecou: Instant,
}

/// O que fazer com quem acabou de chegar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chegada {
    /// Toca: a sessão manda `Tocando` e segura a conexão até um desfecho.
    Toca {
        /// O toque desta conexão.
        toque: IdDoToque,
        /// O toque da mesma chave que esta conexão substituiu, e cuja conexão
        /// a sessão fecha. Não é um desfecho: a mesma pessoa continua tocando.
        substituiu: Option<IdDoToque>,
    },
    /// Entra sem tocar: é quem recebe, ou quem já foi atendido nesta ligação.
    Entra,
    /// Não toca, e a sessão manda a razão.
    Recusada(Recusa),
}

/// Por que alguém não chegou a tocar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recusa {
    /// Está bloqueado na agenda de quem recebe.
    Bloqueado,
    /// Foi recusado há menos de [`ESPERA_DEPOIS_DE_RECUSA`].
    AcabouDeRecusar,
    /// Já há [`TOQUES_PENDENTES`] tocando.
    MuitosTocando,
    /// A ligação já tem [`TETO_DA_LIGACAO`] pessoas.
    Cheia,
}

impl Recusa {
    /// A razão que vai no `Disconnecting`.
    ///
    /// O bloqueado lê «RECUSOU», e não «bloqueado»: a spec de 23/09 não conta
    /// a quem liga que foi bloqueado.
    #[must_use]
    pub const fn razao(self) -> DisconnectReason {
        match self {
            Self::Bloqueado => DisconnectReason::ChamadaRecusada,
            Self::AcabouDeRecusar | Self::MuitosTocando => DisconnectReason::RateLimited,
            Self::Cheia => DisconnectReason::ServerFull,
        }
    }
}

/// Como um toque acabou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desfecho {
    /// Quem recebe atendeu: a sessão segue para a fase 3.
    Atendida,
    /// Quem recebe recusou, ou bloqueou quem tocava.
    Recusada,
    /// O prazo venceu.
    NaoAtendida,
    /// Quem ligava desistiu: a conexão fechou.
    Cancelada,
    /// Atendida, mas a ligação encheu enquanto tocava.
    Cheia,
}

impl Desfecho {
    /// A razão que vai no `Disconnecting`, quando vai alguma.
    ///
    /// `None` em [`Self::Atendida`], que segue, e em [`Self::Cancelada`], em
    /// que já não há a quem mandar.
    #[must_use]
    pub const fn razao(self) -> Option<DisconnectReason> {
        match self {
            Self::Atendida | Self::Cancelada => None,
            Self::Recusada => Some(DisconnectReason::ChamadaRecusada),
            Self::NaoAtendida => Some(DisconnectReason::NinguemAtendeu),
            Self::Cheia => Some(DisconnectReason::ServerFull),
        }
    }
}

/// Um toque que parou, e por quê. É o que a hospedagem empurra ao app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParouDeTocar {
    /// Qual toque.
    pub toque: IdDoToque,
    /// De quem.
    pub chave: Chave,
    /// Como acabou.
    pub desfecho: Desfecho,
}

/// O estado de todas as ligações de uma sala.
#[derive(Debug)]
pub struct Campainha {
    dono: Chave,
    prazo: Duration,
    bloqueados: HashSet<Codigo>,
    toques: Vec<Toque>,
    proximo: u64,
    /// Quem foi atendido nesta ligação. `None`: está dentro, ou entrando.
    /// `Some(quando)`: saiu nesse instante, e ainda pode voltar sem tocar.
    aprovadas: HashMap<Chave, Option<Instant>>,
    recusas: HashMap<Chave, Instant>,
}

impl Campainha {
    /// A campainha da sala de `dono`, sem ninguém tocando e sem bloqueados.
    #[must_use]
    pub fn nova(dono: Chave) -> Self {
        Self {
            dono,
            prazo: PRAZO_DO_TOQUE,
            bloqueados: HashSet::new(),
            toques: Vec::new(),
            proximo: 0,
            aprovadas: HashMap::new(),
            recusas: HashMap::new(),
        }
    }

    /// A mesma campainha, com outro prazo de toque.
    ///
    /// O padrão é [`PRAZO_DO_TOQUE`]. Um prazo curto existe para as provas do
    /// aperto em três fases não esperarem 45 s de verdade.
    #[must_use]
    pub fn com_prazo(self, prazo: Duration) -> Self {
        Self { prazo, ..self }
    }

    /// O prazo de cada toque.
    #[must_use]
    pub fn prazo(&self) -> Duration {
        self.prazo
    }

    /// Troca a lista de bloqueados, que vem da agenda do app, ao vivo.
    ///
    /// Quem já estava tocando e passou a estar bloqueado para de tocar como
    /// recusado.
    pub fn definir_bloqueados(
        &mut self,
        bloqueados: impl IntoIterator<Item = Codigo>,
    ) -> Vec<ParouDeTocar> {
        self.bloqueados = bloqueados.into_iter().collect();
        let (parados, ficam): (Vec<Toque>, Vec<Toque>) = std::mem::take(&mut self.toques)
            .into_iter()
            .partition(|toque| self.bloqueados.contains(&toque.codigo));
        self.toques = ficam;
        parados
            .into_iter()
            .map(|toque| ParouDeTocar {
                toque: toque.id,
                chave: toque.chave,
                desfecho: Desfecho::Recusada,
            })
            .collect()
    }

    /// Alguém provou a chave (fase 1 do aperto de mão) e quer entrar.
    pub fn chegou(&mut self, chave: Chave, apelido: &str, agora: Instant) -> Chegada {
        // Quem recebe é sempre admitido na própria sala (spec de 23/09).
        if chave == self.dono {
            return Chegada::Entra;
        }
        let codigo = Codigo::da_chave(&chave);
        if self.bloqueados.contains(&codigo) {
            return Chegada::Recusada(Recusa::Bloqueado);
        }
        if let Some(quando) = self.recusas.get(&chave).copied() {
            if agora.saturating_duration_since(quando) < ESPERA_DEPOIS_DE_RECUSA {
                return Chegada::Recusada(Recusa::AcabouDeRecusar);
            }
            self.recusas.remove(&chave);
        }
        match self.aprovadas.get(&chave).copied() {
            // Dentro, ou entrando: uma segunda conexão da mesma pessoa ocupa o
            // lugar que já era dela.
            Some(None) => return Chegada::Entra,
            Some(Some(saiu)) if agora.saturating_duration_since(saiu) < TOLERANCIA_DE_RECONEXAO => {
                if self.cheia() {
                    return Chegada::Recusada(Recusa::Cheia);
                }
                self.aprovadas.insert(chave, None);
                return Chegada::Entra;
            }
            Some(Some(_)) => {
                self.aprovadas.remove(&chave);
            }
            None => {}
        }
        if self.cheia() {
            return Chegada::Recusada(Recusa::Cheia);
        }
        let id = self.novo_id();
        if let Some(velho) = self.toques.iter_mut().find(|toque| toque.chave == chave) {
            let substituiu = velho.id;
            velho.id = id;
            velho.apelido = apelido.to_owned();
            velho.comecou = agora;
            return Chegada::Toca {
                toque: id,
                substituiu: Some(substituiu),
            };
        }
        if self.toques.len() >= TOQUES_PENDENTES {
            return Chegada::Recusada(Recusa::MuitosTocando);
        }
        self.toques.push(Toque {
            id,
            chave,
            codigo,
            apelido: apelido.to_owned(),
            comecou: agora,
        });
        Chegada::Toca {
            toque: id,
            substituiu: None,
        }
    }

    /// Quem recebe atendeu. `None` se o toque já tinha acabado.
    pub fn atender(&mut self, toque: IdDoToque) -> Option<ParouDeTocar> {
        let atendido = self.tirar(toque)?;
        let desfecho = if self.cheia() {
            Desfecho::Cheia
        } else {
            self.aprovadas.insert(atendido.chave, None);
            Desfecho::Atendida
        };
        Some(ParouDeTocar {
            toque,
            chave: atendido.chave,
            desfecho,
        })
    }

    /// Quem recebe recusou. `None` se o toque já tinha acabado.
    pub fn recusar(&mut self, toque: IdDoToque, agora: Instant) -> Option<ParouDeTocar> {
        let recusado = self.tirar(toque)?;
        self.recusas.insert(recusado.chave, agora);
        Some(ParouDeTocar {
            toque,
            chave: recusado.chave,
            desfecho: Desfecho::Recusada,
        })
    }

    /// A conexão de quem ligava fechou enquanto tocava. `None` se o toque já
    /// tinha acabado, inclusive substituído por outra conexão da mesma chave.
    pub fn desistiu(&mut self, toque: IdDoToque) -> Option<ParouDeTocar> {
        let desistente = self.tirar(toque)?;
        Some(ParouDeTocar {
            toque,
            chave: desistente.chave,
            desfecho: Desfecho::Cancelada,
        })
    }

    /// Os toques cujo prazo venceu até `agora`, que param como «não atendeu».
    pub fn vencer(&mut self, agora: Instant) -> Vec<ParouDeTocar> {
        let (vencidos, ficam): (Vec<Toque>, Vec<Toque>) = std::mem::take(&mut self.toques)
            .into_iter()
            .partition(|toque| agora.saturating_duration_since(toque.comecou) >= self.prazo);
        self.toques = ficam;
        vencidos
            .into_iter()
            .map(|toque| ParouDeTocar {
                toque: toque.id,
                chave: toque.chave,
                desfecho: Desfecho::NaoAtendida,
            })
            .collect()
    }

    /// Alguém que estava na ligação saiu. Ele volta sem tocar até
    /// [`TOLERANCIA_DE_RECONEXAO`] depois de `agora`.
    pub fn saiu(&mut self, chave: Chave, agora: Instant) {
        if let Some(estado) = self.aprovadas.get_mut(&chave) {
            *estado = Some(agora);
        }
    }

    /// Quem recebe desligou. Devolve quem estava dentro, para receber
    /// `ChamadaEncerrada`, em ordem de chave, e apaga as aprovações.
    ///
    /// Quem estava **tocando** continua tocando: quem recebe pode atender e
    /// começar outra ligação.
    pub fn desligar(&mut self) -> Vec<Chave> {
        let mut dentro: Vec<Chave> = self
            .aprovadas
            .drain()
            .filter_map(|(chave, estado)| estado.is_none().then_some(chave))
            .collect();
        dentro.sort_unstable();
        dentro
    }

    /// Os toques pendentes, do mais velho ao mais novo.
    #[must_use]
    pub fn tocando(&self) -> &[Toque] {
        &self.toques
    }

    /// Quem recebe, mais quem está dentro ou entrando, já é o teto.
    fn cheia(&self) -> bool {
        1 + self
            .aprovadas
            .values()
            .filter(|estado| estado.is_none())
            .count()
            >= TETO_DA_LIGACAO
    }

    fn novo_id(&mut self) -> IdDoToque {
        self.proximo = self.proximo.wrapping_add(1);
        IdDoToque(self.proximo)
    }

    fn tirar(&mut self, toque: IdDoToque) -> Option<Toque> {
        let posicao = self.toques.iter().position(|t| t.id == toque)?;
        Some(self.toques.remove(posicao))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DONO: Chave = [1; 32];

    fn chave(n: u8) -> Chave {
        [n; 32]
    }

    fn tocou(chegada: Chegada) -> IdDoToque {
        match chegada {
            Chegada::Toca { toque, .. } => toque,
            outra => panic!("devia tocar, e veio {outra:?}"),
        }
    }

    /// Põe `chave` dentro da ligação: tocou e foi atendida.
    fn atendida(campainha: &mut Campainha, chave: Chave, agora: Instant) {
        let toque = tocou(campainha.chegou(chave, "alguém", agora));
        assert_eq!(
            campainha.atender(toque).map(|p| p.desfecho),
            Some(Desfecho::Atendida)
        );
    }

    #[test]
    fn quem_recebe_sempre_entra_sem_tocar() {
        let mut campainha = Campainha::nova(DONO);
        assert_eq!(
            campainha.chegou(DONO, "dona", Instant::now()),
            Chegada::Entra
        );
        assert!(
            campainha.tocando().is_empty(),
            "o dono tocou na própria sala"
        );
    }

    #[test]
    fn quem_liga_toca_e_atender_deixa_entrar_de_novo_sem_tocar() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        let toque = tocou(campainha.chegou(chave(2), "rafa", agora));
        assert_eq!(campainha.tocando().len(), 1);
        assert_eq!(campainha.tocando()[0].apelido, "rafa");
        assert_eq!(campainha.tocando()[0].codigo, Codigo::da_chave(&chave(2)));

        let parou = campainha.atender(toque).expect("o toque existia");
        assert_eq!(parou.desfecho, Desfecho::Atendida);
        assert!(campainha.tocando().is_empty());
        assert_eq!(
            campainha.chegou(chave(2), "rafa", agora),
            Chegada::Entra,
            "a segunda conexão de quem já foi atendido tocou de novo"
        );
    }

    #[test]
    fn recusar_segura_a_mesma_chave_por_um_minuto() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        let toque = tocou(campainha.chegou(chave(2), "rafa", agora));
        assert_eq!(
            campainha.recusar(toque, agora).map(|p| p.desfecho),
            Some(Desfecho::Recusada)
        );

        let quase = agora + ESPERA_DEPOIS_DE_RECUSA - Duration::from_millis(1);
        assert_eq!(
            campainha.chegou(chave(2), "rafa", quase),
            Chegada::Recusada(Recusa::AcabouDeRecusar),
            "quem acabou de ser recusado tocou de novo antes do minuto"
        );
        assert!(
            matches!(
                campainha.chegou(chave(2), "rafa", agora + ESPERA_DEPOIS_DE_RECUSA),
                Chegada::Toca { .. }
            ),
            "depois do minuto, a mesma chave continuou sem poder ligar"
        );
    }

    #[test]
    fn o_prazo_vencido_para_como_nao_atendida() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        let toque = tocou(campainha.chegou(chave(2), "rafa", agora));

        assert!(
            campainha
                .vencer(agora + PRAZO_DO_TOQUE - Duration::from_millis(1))
                .is_empty(),
            "o toque venceu antes do prazo"
        );
        let vencidos = campainha.vencer(agora + PRAZO_DO_TOQUE);
        assert_eq!(
            vencidos,
            vec![ParouDeTocar {
                toque,
                chave: chave(2),
                desfecho: Desfecho::NaoAtendida
            }]
        );
        assert!(campainha.tocando().is_empty());
        assert_eq!(
            campainha.atender(toque),
            None,
            "um toque vencido foi atendido"
        );
    }

    #[test]
    fn um_prazo_curto_vence_no_prazo_dele() {
        let prazo = Duration::from_secs(1);
        let mut campainha = Campainha::nova(DONO).com_prazo(prazo);
        let agora = Instant::now();
        tocou(campainha.chegou(chave(2), "rafa", agora));
        assert!(campainha
            .vencer(agora + prazo - Duration::from_millis(1))
            .is_empty());
        assert_eq!(
            campainha.vencer(agora + prazo).len(),
            1,
            "o toque não venceu no prazo que a campainha recebeu"
        );
    }

    #[test]
    fn quem_desiste_cancela_o_toque() {
        let mut campainha = Campainha::nova(DONO);
        let toque = tocou(campainha.chegou(chave(2), "rafa", Instant::now()));
        assert_eq!(
            campainha.desistiu(toque).map(|p| p.desfecho),
            Some(Desfecho::Cancelada)
        );
        assert!(campainha.tocando().is_empty());
    }

    #[test]
    fn os_candidatos_da_corrida_nao_viram_tres_toques() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        let primeiro = tocou(campainha.chegou(chave(2), "rafa", agora));
        let segundo = campainha.chegou(chave(2), "rafa", agora);
        let Chegada::Toca {
            toque: novo,
            substituiu,
        } = segundo
        else {
            panic!("a segunda conexão da mesma chave não tocou: {segundo:?}");
        };
        assert_eq!(
            substituiu,
            Some(primeiro),
            "a mais nova não substituiu a mais velha"
        );
        assert_ne!(novo, primeiro);
        assert!(matches!(
            campainha.chegou(chave(2), "rafa", agora),
            Chegada::Toca { .. }
        ));
        assert_eq!(
            campainha.tocando().len(),
            1,
            "três candidatos da mesma chave viraram mais de um toque"
        );
        assert_eq!(
            campainha.desistiu(primeiro),
            None,
            "fechar a conexão substituída parou o toque que continua"
        );
    }

    #[test]
    fn o_quarto_toque_pendente_e_recusado() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        for n in 2..2 + u8::try_from(TOQUES_PENDENTES).unwrap() {
            tocou(campainha.chegou(chave(n), "alguém", agora));
        }
        assert_eq!(
            campainha.chegou(chave(9), "mais um", agora),
            Chegada::Recusada(Recusa::MuitosTocando),
            "a faixa aceitou mais toques do que alguém consegue responder"
        );
        assert_eq!(campainha.tocando().len(), TOQUES_PENDENTES);
    }

    #[test]
    fn a_ligacao_cheia_recusa_na_entrada_e_ao_atender() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        // Quem recebe conta: cabem TETO - 1 convidados.
        for n in 2..2 + u8::try_from(TETO_DA_LIGACAO - 2).unwrap() {
            atendida(&mut campainha, chave(n), agora);
        }
        let um = tocou(campainha.chegou(chave(20), "um", agora));
        let outro = tocou(campainha.chegou(chave(21), "outro", agora));
        assert_eq!(
            campainha.atender(um).map(|p| p.desfecho),
            Some(Desfecho::Atendida)
        );
        assert_eq!(
            campainha.atender(outro).map(|p| p.desfecho),
            Some(Desfecho::Cheia),
            "a ligação passou de {TETO_DA_LIGACAO} pessoas com dois atendidos ao mesmo tempo"
        );
        assert_eq!(
            campainha.chegou(chave(22), "tarde", agora),
            Chegada::Recusada(Recusa::Cheia),
            "uma ligação cheia ainda tocou"
        );
    }

    #[test]
    fn quem_foi_atendido_volta_em_um_minuto_sem_tocar() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        atendida(&mut campainha, chave(2), agora);
        campainha.saiu(chave(2), agora);

        let volta = agora + TOLERANCIA_DE_RECONEXAO - Duration::from_millis(1);
        assert_eq!(
            campainha.chegou(chave(2), "rafa", volta),
            Chegada::Entra,
            "quem caiu e voltou dentro da tolerância tocou de novo"
        );
        campainha.saiu(chave(2), volta);
        assert!(
            matches!(
                campainha.chegou(chave(2), "rafa", volta + TOLERANCIA_DE_RECONEXAO),
                Chegada::Toca { .. }
            ),
            "quem voltou depois da tolerância entrou sem tocar"
        );
    }

    #[test]
    fn desligar_encerra_quem_esta_dentro_e_apaga_as_aprovacoes() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        atendida(&mut campainha, chave(3), agora);
        atendida(&mut campainha, chave(2), agora);
        let tocando = tocou(campainha.chegou(chave(4), "esperando", agora));

        assert_eq!(
            campainha.desligar(),
            vec![chave(2), chave(3)],
            "quem estava dentro não foi encerrado"
        );
        assert!(
            matches!(
                campainha.chegou(chave(2), "rafa", agora),
                Chegada::Toca { .. }
            ),
            "uma aprovação sobreviveu ao desligar"
        );
        assert!(
            campainha.tocando().iter().any(|t| t.id == tocando),
            "desligar parou quem ainda estava tocando"
        );
    }

    #[test]
    fn o_bloqueado_nunca_toca() {
        let mut campainha = Campainha::nova(DONO);
        campainha.definir_bloqueados([Codigo::da_chave(&chave(2))]);
        assert_eq!(
            campainha.chegou(chave(2), "rafa", Instant::now()),
            Chegada::Recusada(Recusa::Bloqueado)
        );
        assert!(campainha.tocando().is_empty(), "um bloqueado tocou");
    }

    #[test]
    fn bloquear_quem_esta_tocando_para_o_toque_como_recusado() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        let toque = tocou(campainha.chegou(chave(2), "rafa", agora));
        tocou(campainha.chegou(chave(3), "outra", agora));

        let parados = campainha.definir_bloqueados([Codigo::da_chave(&chave(2))]);
        assert_eq!(
            parados,
            vec![ParouDeTocar {
                toque,
                chave: chave(2),
                desfecho: Desfecho::Recusada
            }]
        );
        assert_eq!(campainha.tocando().len(), 1, "bloquear um parou os outros");
    }

    #[test]
    fn um_toque_se_decide_uma_vez() {
        let mut campainha = Campainha::nova(DONO);
        let agora = Instant::now();
        let toque = tocou(campainha.chegou(chave(2), "rafa", agora));
        assert!(campainha.atender(toque).is_some());
        assert_eq!(
            campainha.atender(toque),
            None,
            "um toque foi atendido duas vezes"
        );
        assert_eq!(
            campainha.recusar(toque, agora),
            None,
            "um toque atendido foi recusado depois"
        );
        assert_eq!(campainha.desistiu(toque), None);
    }

    #[test]
    fn cada_recusa_e_cada_desfecho_tem_a_razao_do_fio() {
        assert_eq!(Recusa::Bloqueado.razao(), DisconnectReason::ChamadaRecusada);
        assert_eq!(
            Recusa::AcabouDeRecusar.razao(),
            DisconnectReason::RateLimited
        );
        assert_eq!(Recusa::MuitosTocando.razao(), DisconnectReason::RateLimited);
        assert_eq!(Recusa::Cheia.razao(), DisconnectReason::ServerFull);
        assert_eq!(Desfecho::Atendida.razao(), None);
        assert_eq!(Desfecho::Cancelada.razao(), None);
        assert_eq!(
            Desfecho::Recusada.razao(),
            Some(DisconnectReason::ChamadaRecusada)
        );
        assert_eq!(
            Desfecho::NaoAtendida.razao(),
            Some(DisconnectReason::NinguemAtendeu)
        );
        assert_eq!(Desfecho::Cheia.razao(), Some(DisconnectReason::ServerFull));
    }
}
