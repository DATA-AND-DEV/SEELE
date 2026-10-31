//! O registro dos túneis de jogo do servidor (protocolo 12): quem serve, quem
//! entrou, e o repasse pela estrela quando não há caminho direto.
//!
//! # O que este módulo guarda, e o que ele não decide
//!
//! [`Tuneis`] é só o livro: uma máquina serve `(mod_id, nome)`, e quem passou na
//! conferência da ação `entrar-no-jogo` (ADR 0064) fica na lista de quem está
//! dentro. **O livro não consulta o banco**: quem confere o cargo é
//! [`atender_entrar`], antes de tomar o cadeado, pela ordem de
//! `session::apontar_um_par` (ler o que vem de fora, o banco e a ocupação,
//! antes de tomar o que é nosso). O cadeado do livro é um `std::sync::Mutex` e
//! nunca atravessa um `await`.
//!
//! # O repasse
//!
//! Sem caminho direto, quem joga abre um canal `t` (TCP) ou `j` (UDP) **neste
//! servidor**, e o servidor abre outro do mesmo tipo na conexão da máquina e
//! copia nos dois sentidos ([`repassar`]). A primeira mensagem de todo canal
//! repassado é uma [`AberturaDeTunel`]: no `t`, um quadro de controle
//! (`crate::frame`, o mesmo dos canais `b`); no `j`, um datagrama marcado com
//! `TAG_DE_ABERTURA` (o `control::encode` da abertura depois do tag, sem prefixo
//! de comprimento), e todo datagrama de carga leva `TAG_DE_DADO` na frente. O
//! `j` não ordena nem retransmite, então a abertura não é «o primeiro
//! datagrama»: quem abre a repete até algo voltar, e o repasse **reencaminha**
//! cada repetição à máquina com a abertura reescrita. O anfitrião reescreve o
//! campo `de` com quem de fato abriu o canal, e é esse o motivo de a máquina
//! não precisar confiar no que a rede diz.
//!
//! **Um canal sem concessão é cortado e contado**, sem tocar na máquina
//! ([`Server::canais_sem_concessao`]), e uma concessão retirada derruba junto os
//! canais que andavam por ela: cada concessão carrega um sinal de vigia que
//! morre com ela.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use seele_enlace::{CanalDeTunel, Confiabilidade, Envio, FalhaDoCanal, Motivo, Recebe, Recusa};
use seele_proto::cargos::RecusaDeAcaoDeMod;
use seele_proto::ids::PersonId;
use seele_proto::mods::Poder;
use seele_proto::tunel::{
    datagrama_de_abertura, ler_datagrama_j, AberturaDeTunel, DatagramaJ, MotivoDoFimDoTunel,
    ProtocoloDeTunel, RecusaDeTunel, TipoDeCanalDeTunel, ACAO_ENTRAR_NO_JOGO,
};

use crate::server::{Event, Server};
use crate::transporte::Conexao;

/// Quantos túneis um MOD pode servir neste servidor.
pub const MAX_TUNEIS_POR_MOD: usize = 8;

/// Quantos túneis o servidor inteiro guarda.
pub const MAX_TUNEIS_POR_SERVIDOR: usize = 32;

/// Quantos canais repassados **vivos** uma pessoa pode ter ao mesmo tempo,
/// somados todos os túneis. A conexão da máquina tem `MAX_CANAIS` (64) canais
/// para todo mundo: sem este teto, uma pessoa sozinha os esgota e ninguém mais
/// entra no jogo. O teto fica bem abaixo, para que quatro pessoas no limite
/// ainda caibam.
pub const MAX_CANAIS_REPASSADOS_POR_PESSOA: usize = 16;

/// Quanto se espera pela [`AberturaDeTunel`] de um canal que chegou. Um canal
/// que não diz a que veio não ocupa vaga para sempre.
const PRAZO_DA_ABERTURA: Duration = Duration::from_secs(10);

/// Quantos contadores de repasse de túneis que já acabaram o livro lembra. O
/// teste que lê o contador depois de um túnel acabar precisa dele, mas um
/// servidor que vê nomes novos a toda hora não pode crescer sem fim.
const CONTADORES_LEMBRADOS: usize = 128;

/// Um túnel como a máquina o pediu.
#[derive(Debug, Clone, Copy)]
pub struct PedidoDeTunel<'a> {
    /// `autor/nome` do MOD.
    pub mod_id: &'a str,
    /// O nome do túnel.
    pub nome: &'a str,
    /// O que ele carrega.
    pub protocolo: ProtocoloDeTunel,
    /// A porta do jogo na máquina.
    pub porta: u16,
}

/// O que `EntrarNoTunel` devolve a quem entrou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TunelServido {
    /// Quem serve.
    pub maquina: PersonId,
    /// O que o túnel carrega.
    pub protocolo: ProtocoloDeTunel,
    /// A porta do jogo, na máquina.
    pub porta: u16,
}

/// Um túnel que acabou, para quem precisa saber: vira um `TunelEncerrado`
/// endereçado (`Event::TunelEncerrado`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encerramento {
    /// A quem dizer.
    pub para: PersonId,
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O nome do túnel.
    pub nome: String,
    /// Por quê.
    pub motivo: MotivoDoFimDoTunel,
}

/// A máquina que serve precisa saber que uma pessoa deixou de poder alcançar o
/// túnel: vira um `TunelRetirado` endereçado (`Event::TunelRetirado`).
///
/// `Encerramento` conta a quem jogava; esta conta a quem serve. Sem ela a
/// máquina não tem como cortar a ligação direta, onde o servidor não vê os
/// canais.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retirada {
    /// A máquina que serve o túnel.
    pub para: PersonId,
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O nome do túnel.
    pub nome: String,
    /// Quem perdeu o direito.
    pub quem: PersonId,
}

/// O que um evento do livro obriga a dizer: os túneis que acabaram e as
/// pessoas retiradas de túneis que seguem.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Fim {
    /// Quem precisa saber que um túnel acabou.
    pub encerramentos: Vec<Encerramento>,
    /// As máquinas que precisam saber que uma pessoa saiu do túnel delas.
    pub retiradas: Vec<Retirada>,
}

impl From<Vec<Encerramento>> for Fim {
    fn from(encerramentos: Vec<Encerramento>) -> Self {
        Self {
            encerramentos,
            retiradas: Vec::new(),
        }
    }
}

impl Fim {
    /// Se não há o que dizer a ninguém.
    #[must_use]
    pub fn vazio(&self) -> bool {
        self.encerramentos.is_empty() && self.retiradas.is_empty()
    }

    fn junta(&mut self, outro: Self) {
        self.encerramentos.extend(outro.encerramentos);
        self.retiradas.extend(outro.retiradas);
    }
}

/// A concessão de uma pessoa a um túnel.
struct Concessao {
    /// A conexão que entrou. A saída de uma conexão velha não tira quem já
    /// voltou por outra (a mesma lição de `Pares::saiu`).
    conexao: u64,
    /// O sinal de vigia dos canais repassados por esta concessão: quando ela
    /// some, o `Sender` cai e todo `Receiver` vê o fim.
    vigia: tokio::sync::watch::Sender<()>,
}

struct Entrada {
    maquina: PersonId,
    /// A conexão da máquina que serviu.
    conexao: u64,
    protocolo: ProtocoloDeTunel,
    porta: u16,
    dentro: HashMap<PersonId, Concessao>,
}

type Chave = (String, String);

fn chave(mod_id: &str, nome: &str) -> Chave {
    (mod_id.to_owned(), nome.to_owned())
}

/// Por que um canal que chegou não foi repassado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorteDeCanal {
    /// Quem abriu não tem concessão (ou o túnel não existe mais).
    SemConcessao,
    /// O tipo do canal não combina com o protocolo do túnel.
    TipoErrado,
    /// A máquina não tem conexão registrada.
    SemMaquina,
}

/// O contador em que os bytes de um canal entram: o do túnel para o jogo, e um
/// descartável para o controle (o ping de latência não é jogo repassado).
///
/// Sem esta separação o contador nunca ficaria em zero: o túnel de quem joga
/// nasce na estrela e manda o primeiro ping por ela, antes de o caminho direto
/// existir, e `contador_de_repasse` passaria a medir o ping, e não o jogo.
fn contador_do_canal(tipo: TipoDeCanalDeTunel, do_tunel: &Arc<AtomicU64>) -> Arc<AtomicU64> {
    match tipo {
        TipoDeCanalDeTunel::Controle => Arc::default(),
        TipoDeCanalDeTunel::Tcp | TipoDeCanalDeTunel::Udp => Arc::clone(do_tunel),
    }
}

/// Por onde um canal chegado vai: tudo o que o repasse lê do livro de uma vez.
pub struct Rota {
    /// Quem serve.
    pub maquina: PersonId,
    /// A conexão da máquina.
    pub conexao: Conexao,
    /// Morre quando a concessão de quem abriu o canal acaba.
    pub vigia: tokio::sync::watch::Receiver<()>,
    /// Os bytes repassados deste túnel.
    pub contador: Arc<AtomicU64>,
}

/// Uma vaga de canal repassado de uma pessoa. Devolve-a no `Drop`: o repasse
/// tem muitas saídas (erro, fim limpo, concessão acabada, tarefa abortada), e
/// um decremento escrito à mão em cada uma vaza vaga na primeira que se esquece.
pub struct CanalRepassado {
    de: PersonId,
    mapa: Arc<std::sync::Mutex<HashMap<PersonId, usize>>>,
}

impl Drop for CanalRepassado {
    fn drop(&mut self) {
        let mut mapa = self
            .mapa
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(vivos) = mapa.get_mut(&self.de) {
            *vivos = vivos.saturating_sub(1);
            if *vivos == 0 {
                mapa.remove(&self.de);
            }
        }
    }
}

/// O livro dos túneis.
#[derive(Default)]
pub struct Tuneis {
    servidos: HashMap<Chave, Entrada>,
    /// As conexões das máquinas que servem, por `stable_id`.
    conexoes: HashMap<u64, Conexao>,
    contadores: HashMap<Chave, Arc<AtomicU64>>,
    sem_concessao: AtomicU64,
    /// Quantos canais cada pessoa tem hoje repassados. Atrás do próprio
    /// `Arc<Mutex>` (e não do cadeado do livro) para o `Drop` do guarda poder
    /// devolver a vaga sem tomar `tuneis()`: é um cadeado-folha, nunca seguro
    /// junto de outro nem atravessando um `await`, então não cria ordem nova.
    repassados: Arc<std::sync::Mutex<HashMap<PersonId, usize>>>,
    /// Canais cortados por a pessoa já ter o teto de canais vivos.
    acima_do_teto: AtomicU64,
    /// Datagramas de carga que chegaram a um canal `j` antes da abertura dele.
    dados_antes_da_abertura: AtomicU64,
}

impl Tuneis {
    /// Uma máquina passa a servir `(mod_id, nome)`.
    ///
    /// `poderes` são os que o manifesto do MOD **ligado** declara (vazio para
    /// um MOD que não está ligado). A mesma máquina servindo de novo toma o
    /// túnel de volta, com quem já estava dentro: é a conexão nova depois de
    /// uma queda.
    ///
    /// # Errors
    ///
    /// [`RecusaDeTunel::ModSemPorta`], [`RecusaDeTunel::NomeTomado`] (outra
    /// máquina já serve o nome) ou [`RecusaDeTunel::TuneisDemais`].
    pub fn servir(
        &mut self,
        maquina: PersonId,
        conexao: u64,
        pedido: PedidoDeTunel<'_>,
        poderes: &[Poder],
    ) -> Result<(), RecusaDeTunel> {
        if !poderes.contains(&Poder::Porta) {
            return Err(RecusaDeTunel::ModSemPorta);
        }
        let k = chave(pedido.mod_id, pedido.nome);
        if let Some(existente) = self.servidos.get_mut(&k) {
            if existente.maquina != maquina {
                return Err(RecusaDeTunel::NomeTomado);
            }
            existente.conexao = conexao;
            existente.protocolo = pedido.protocolo;
            existente.porta = pedido.porta;
            return Ok(());
        }
        let do_mod = self
            .servidos
            .keys()
            .filter(|(mod_id, _)| mod_id == pedido.mod_id)
            .count();
        if do_mod >= MAX_TUNEIS_POR_MOD || self.servidos.len() >= MAX_TUNEIS_POR_SERVIDOR {
            return Err(RecusaDeTunel::TuneisDemais);
        }
        if self.contadores.len() >= CONTADORES_LEMBRADOS {
            let servidos = &self.servidos;
            self.contadores.retain(|k, _| servidos.contains_key(k));
        }
        self.contadores.entry(k.clone()).or_default();
        self.servidos.insert(
            k,
            Entrada {
                maquina,
                conexao,
                protocolo: pedido.protocolo,
                porta: pedido.porta,
                dentro: HashMap::new(),
            },
        );
        Ok(())
    }

    /// Guarda a conexão de uma máquina que serve, para o repasse abrir canais
    /// nela. Sai em [`Self::saiu`].
    pub fn registrar_conexao(&mut self, id: u64, conexao: Conexao) {
        self.conexoes.insert(id, conexao);
    }

    /// A máquina para de servir. Só a dona do nome pode; quem estava dentro
    /// fica sabendo.
    pub fn parar(&mut self, maquina: PersonId, mod_id: &str, nome: &str) -> Vec<Encerramento> {
        let k = chave(mod_id, nome);
        match self.servidos.get(&k) {
            Some(entrada) if entrada.maquina == maquina => {}
            _ => return Vec::new(),
        }
        let Some(entrada) = self.servidos.remove(&k) else {
            return Vec::new();
        };
        Self::encerrar_para_quem_joga(&entrada, mod_id, nome, MotivoDoFimDoTunel::MaquinaParou)
    }

    /// **O servidor para o túnel de uma máquina** (a reserva assumiu a
    /// posse, o aceite mudou os túneis, outro pedido): o mesmo que
    /// [`Self::parar`], e **a máquina também ouve** `MaquinaParou` (revisão
    /// final, I4). Sem isto, ela seguia aceitando as ligações diretas de quem
    /// já tinha sido apresentado — dois mundos com uma dona só no livro — e a
    /// metade dela seguia dizendo que servia.
    pub fn parar_pelo_servidor(
        &mut self,
        maquina: PersonId,
        mod_id: &str,
        nome: &str,
    ) -> Vec<Encerramento> {
        let k = chave(mod_id, nome);
        match self.servidos.get(&k) {
            Some(entrada) if entrada.maquina == maquina => {}
            _ => return Vec::new(),
        }
        let Some(entrada) = self.servidos.remove(&k) else {
            return Vec::new();
        };
        let mut fim = vec![Encerramento {
            para: entrada.maquina,
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
            motivo: MotivoDoFimDoTunel::MaquinaParou,
        }];
        fim.extend(Self::encerrar_para_quem_joga(
            &entrada,
            mod_id,
            nome,
            MotivoDoFimDoTunel::MaquinaParou,
        ));
        fim
    }

    fn encerrar_para_quem_joga(
        entrada: &Entrada,
        mod_id: &str,
        nome: &str,
        motivo: MotivoDoFimDoTunel,
    ) -> Vec<Encerramento> {
        entrada
            .dentro
            .keys()
            .filter(|p| **p != entrada.maquina)
            .map(|para| Encerramento {
                para: *para,
                mod_id: mod_id.to_owned(),
                nome: nome.to_owned(),
                motivo,
            })
            .collect()
    }

    /// Quem passou na conferência da ação passa a estar dentro. **A conferência
    /// não é daqui** (ver o módulo): este livro só guarda o resultado.
    ///
    /// # Errors
    ///
    /// [`RecusaDeTunel::NaoExiste`] quando ninguém serve o nome.
    pub fn entrar(
        &mut self,
        pessoa: PersonId,
        conexao: u64,
        mod_id: &str,
        nome: &str,
    ) -> Result<TunelServido, RecusaDeTunel> {
        let entrada = self
            .servidos
            .get_mut(&chave(mod_id, nome))
            .ok_or(RecusaDeTunel::NaoExiste)?;
        // Entrar duas vezes pela mesma conexão não derruba os canais que já
        // andam; vir por outra conexão troca a concessão, e os da velha caem.
        if entrada
            .dentro
            .get(&pessoa)
            .is_none_or(|c| c.conexao != conexao)
        {
            entrada.dentro.insert(
                pessoa,
                Concessao {
                    conexao,
                    vigia: tokio::sync::watch::Sender::new(()),
                },
            );
        }
        Ok(TunelServido {
            maquina: entrada.maquina,
            protocolo: entrada.protocolo,
            porta: entrada.porta,
        })
    }

    /// Quem joga deixa o túnel. Devolve a retirada a dizer à máquina (vazio se
    /// não havia o que tirar).
    pub fn sair(&mut self, pessoa: PersonId, mod_id: &str, nome: &str) -> Fim {
        let mut fim = Fim::default();
        if let Some(entrada) = self.servidos.get_mut(&chave(mod_id, nome)) {
            if entrada.dentro.remove(&pessoa).is_some() {
                fim.retiradas
                    .extend(Self::retirada(entrada, mod_id, nome, pessoa));
            }
        }
        fim
    }

    /// A retirada de `quem` do túnel de `entrada`, para a máquina. Nenhuma
    /// quando `quem` é a própria máquina.
    fn retirada(entrada: &Entrada, mod_id: &str, nome: &str, quem: PersonId) -> Option<Retirada> {
        (quem != entrada.maquina).then(|| Retirada {
            para: entrada.maquina,
            mod_id: mod_id.to_owned(),
            nome: nome.to_owned(),
            quem,
        })
    }

    /// Se alguém serve `(mod_id, nome)`.
    #[must_use]
    pub fn existe(&self, mod_id: &str, nome: &str) -> bool {
        self.servidos.contains_key(&chave(mod_id, nome))
    }

    /// Uma conexão acabou: os túneis que **ela** servia acabam (`MaquinaSaiu`
    /// para quem estava dentro), e as concessões que **ela** tinha somem
    /// (`PessoaSaiu`, só no log).
    ///
    /// A conexão é conferida nos dois casos: uma corrida de candidatos deixa
    /// duas conexões vivas da mesma pessoa, e o fim da que perdeu não pode levar
    /// o que a vencedora acabou de fazer.
    pub fn saiu(&mut self, pessoa: PersonId, conexao: u64) -> Fim {
        self.conexoes.remove(&conexao);
        let acabaram: Vec<Chave> = self
            .servidos
            .iter()
            .filter(|(_, e)| e.maquina == pessoa && e.conexao == conexao)
            .map(|(k, _)| k.clone())
            .collect();
        let mut fim = Fim::default();
        for k in acabaram {
            if let Some(entrada) = self.servidos.remove(&k) {
                fim.encerramentos.extend(Self::encerrar_para_quem_joga(
                    &entrada,
                    &k.0,
                    &k.1,
                    MotivoDoFimDoTunel::MaquinaSaiu,
                ));
            }
        }
        for (k, entrada) in &mut self.servidos {
            let eh_dela = entrada
                .dentro
                .get(&pessoa)
                .is_some_and(|c| c.conexao == conexao);
            if eh_dela {
                entrada.dentro.remove(&pessoa);
                // A máquina fecha a ligação direta de quem saiu, que o
                // servidor não enxerga.
                fim.retiradas
                    .extend(Self::retirada(entrada, &k.0, &k.1, pessoa));
                tracing::info!(
                    mod_id = %k.0,
                    tunel = %k.1,
                    %pessoa,
                    motivo = ?MotivoDoFimDoTunel::PessoaSaiu,
                    "túnel: quem jogava saiu"
                );
            }
        }
        fim
    }

    /// O MOD foi desligado (ou deixou de declarar o poder): todos os túneis dele
    /// acabam, para quem servia e para quem jogava.
    ///
    /// **Não conte com o `TunelEncerrado { ModDesligado }` chegando a um
    /// cliente ao vivo**: mudar o conjunto de MODs derruba as sessões com
    /// `ModsMudaram` antes, e quem está dentro costuma ver a queda da conexão
    /// (e `MaquinaSaiu`/`PessoaSaiu` no livro), não o aviso. O que este método
    /// garante é o estado do servidor: o túnel some e os canais caem.
    pub fn mod_desligado(&mut self, mod_id: &str) -> Vec<Encerramento> {
        let acabaram: Vec<Chave> = self
            .servidos
            .keys()
            .filter(|(m, _)| m == mod_id)
            .cloned()
            .collect();
        let mut fim = Vec::new();
        for k in acabaram {
            let Some(entrada) = self.servidos.remove(&k) else {
                continue;
            };
            fim.push(Encerramento {
                para: entrada.maquina,
                mod_id: k.0.clone(),
                nome: k.1.clone(),
                motivo: MotivoDoFimDoTunel::ModDesligado,
            });
            fim.extend(Self::encerrar_para_quem_joga(
                &entrada,
                &k.0,
                &k.1,
                MotivoDoFimDoTunel::ModDesligado,
            ));
        }
        fim
    }

    /// A pessoa perdeu a ação `entrar-no-jogo` do MOD: as concessões dela nos
    /// túneis dele somem, e os canais que andavam por elas caem junto.
    ///
    /// Devolve o `SemPermissao` de quem perdeu **e a retirada de quem serve**:
    /// pela estrela os canais caem com a concessão, mas a ligação direta é da
    /// máquina, e é ela que tem de saber.
    pub fn sem_permissao(&mut self, pessoa: PersonId, mod_id: &str) -> Fim {
        let mut fim = Fim::default();
        for (k, entrada) in &mut self.servidos {
            if k.0 == mod_id && entrada.dentro.remove(&pessoa).is_some() {
                fim.encerramentos.push(Encerramento {
                    para: pessoa,
                    mod_id: k.0.clone(),
                    nome: k.1.clone(),
                    motivo: MotivoDoFimDoTunel::SemPermissao,
                });
                fim.retiradas
                    .extend(Self::retirada(entrada, &k.0, &k.1, pessoa));
            }
        }
        fim
    }

    /// Os MODs em cujos túneis a pessoa está dentro, sem repetir.
    #[must_use]
    pub fn mods_em_que_esta(&self, pessoa: PersonId) -> Vec<String> {
        let mut mods: Vec<String> = self
            .servidos
            .iter()
            .filter(|(_, e)| e.dentro.contains_key(&pessoa))
            .map(|(k, _)| k.0.clone())
            .collect();
        mods.sort();
        mods.dedup();
        mods
    }

    /// Cada par `(pessoa, mod)` que tem uma concessão de pé, sem repetir.
    #[must_use]
    pub fn concessoes(&self) -> Vec<(PersonId, String)> {
        let mut todas: Vec<(PersonId, String)> = self
            .servidos
            .iter()
            .flat_map(|(k, e)| e.dentro.keys().map(|p| (*p, k.0.clone())))
            .collect();
        todas.sort();
        todas.dedup();
        todas
    }

    /// Os MODs que servem algum túnel, sem repetir.
    #[must_use]
    pub fn mods_servidos(&self) -> Vec<String> {
        let mut mods: Vec<String> = self.servidos.keys().map(|k| k.0.clone()).collect();
        mods.sort();
        mods.dedup();
        mods
    }

    /// Se um canal que `de` abriu pode ser repassado, e a quem: a máquina que
    /// serve, **somente** se `de` tem a concessão do túnel que a abertura nomeia.
    ///
    /// É a conferência de cada canal, e a única: o repasse não repete a conta
    /// com outra régua.
    #[must_use]
    pub fn pode_repassar(&self, de: PersonId, abertura: &AberturaDeTunel) -> Option<PersonId> {
        let entrada = self
            .servidos
            .get(&chave(&abertura.mod_id, &abertura.nome))?;
        entrada.dentro.contains_key(&de).then_some(entrada.maquina)
    }

    /// Tudo o que o repasse lê do livro para um canal que chegou.
    ///
    /// # Errors
    ///
    /// [`CorteDeCanal`], que o repasse conta e diz no log.
    pub fn rota(
        &self,
        de: PersonId,
        abertura: &AberturaDeTunel,
        canal_ordenado: bool,
    ) -> Result<Rota, CorteDeCanal> {
        let maquina = self
            .pode_repassar(de, abertura)
            .ok_or(CorteDeCanal::SemConcessao)?;
        let k = chave(&abertura.mod_id, &abertura.nome);
        let entrada = self.servidos.get(&k).ok_or(CorteDeCanal::SemConcessao)?;
        if !tipo_combina(canal_ordenado, abertura.tipo, entrada.protocolo) {
            return Err(CorteDeCanal::TipoErrado);
        }
        let conexao = self
            .conexoes
            .get(&entrada.conexao)
            .cloned()
            .ok_or(CorteDeCanal::SemMaquina)?;
        // Quem passou em `pode_repassar` tem concessão; um vigia já fechado
        // seria o canal nascendo cortado, que é o lado seguro do impossível.
        let vigia = entrada.dentro.get(&de).map_or_else(
            || tokio::sync::watch::channel(()).1,
            |c| c.vigia.subscribe(),
        );
        let contador = contador_do_canal(
            abertura.tipo,
            &self.contadores.get(&k).cloned().unwrap_or_default(),
        );
        Ok(Rota {
            maquina,
            conexao,
            vigia,
            contador,
        })
    }

    /// Quantos bytes o anfitrião repassou neste túnel, nos dois sentidos.
    #[must_use]
    pub fn contador(&self, mod_id: &str, nome: &str) -> u64 {
        self.contadores
            .get(&chave(mod_id, nome))
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }

    /// Reserva uma vaga de canal repassado para `de`, ou diz que o teto
    /// ([`MAX_CANAIS_REPASSADOS_POR_PESSOA`]) já foi atingido. A vaga volta
    /// sozinha quando o guarda cai, por qualquer caminho de saída do repasse.
    pub fn reservar_canal(&self, de: PersonId) -> Option<CanalRepassado> {
        let mut mapa = self
            .repassados
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let vivos = mapa.entry(de).or_default();
        if *vivos >= MAX_CANAIS_REPASSADOS_POR_PESSOA {
            return None;
        }
        *vivos += 1;
        Some(CanalRepassado {
            de,
            mapa: Arc::clone(&self.repassados),
        })
    }

    /// Conta um canal cortado por falta de concessão.
    pub fn contar_sem_concessao(&self) {
        self.sem_concessao.fetch_add(1, Ordering::Relaxed);
    }

    /// Quantos canais foram cortados por falta de concessão.
    #[must_use]
    pub fn canais_sem_concessao(&self) -> u64 {
        self.sem_concessao.load(Ordering::Relaxed)
    }

    /// Quantos canais foram cortados por a pessoa já ter o teto de canais vivos.
    #[must_use]
    pub fn canais_acima_do_teto(&self) -> u64 {
        self.acima_do_teto.load(Ordering::Relaxed)
    }

    /// Conta um datagrama de carga que chegou antes da abertura do canal `j`.
    pub fn contar_dado_antes_da_abertura(&self) {
        self.dados_antes_da_abertura.fetch_add(1, Ordering::Relaxed);
    }

    /// Quantos datagramas de carga chegaram a um canal `j` antes da abertura:
    /// descartados e contados, sem cortar o canal.
    #[must_use]
    pub fn dados_antes_da_abertura(&self) -> u64 {
        self.dados_antes_da_abertura.load(Ordering::Relaxed)
    }
}

/// `t` leva TCP e o canal de sinais (`Controle`, de qualquer túnel: é por ele
/// que quem joga mede a latência de ponta a ponta); `j` leva UDP.
fn tipo_combina(
    canal_ordenado: bool,
    tipo: TipoDeCanalDeTunel,
    protocolo: ProtocoloDeTunel,
) -> bool {
    matches!(
        (canal_ordenado, tipo, protocolo),
        (true, TipoDeCanalDeTunel::Tcp, ProtocoloDeTunel::Tcp)
            | (true, TipoDeCanalDeTunel::Controle, _)
            | (false, TipoDeCanalDeTunel::Udp, ProtocoloDeTunel::Udp)
    )
}

impl Server {
    /// O livro dos túneis, sem a poluição de um cadeado envenenado: o que ele
    /// guarda é consistente a cada passo, e um pânico noutra sessão não pode
    /// calar todas as outras.
    pub fn tuneis(&self) -> std::sync::MutexGuard<'_, Tuneis> {
        self.tuneis
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Quantos bytes o anfitrião repassou no túnel `(mod_id, nome)`, nos dois
    /// sentidos. **Zero quando os dois lados se alcançaram direto.**
    #[must_use]
    pub fn contador_de_repasse(&self, mod_id: &str, nome: &str) -> u64 {
        self.tuneis().contador(mod_id, nome)
    }

    /// Quantos canais de túnel foram cortados **antes de chegar à máquina**:
    /// sem concessão, com abertura ilegível ou fora do prazo, de tipo que não
    /// combina com o túnel, ou sem conexão de máquina registrada. (O nome é o
    /// do caso principal; o log de cada corte diz qual foi.)
    #[must_use]
    pub fn canais_sem_concessao(&self) -> u64 {
        self.tuneis().canais_sem_concessao()
    }

    /// Quantos canais foram cortados por a pessoa já ter o teto de canais
    /// repassados vivos ([`MAX_CANAIS_REPASSADOS_POR_PESSOA`]).
    #[must_use]
    pub fn canais_acima_do_teto(&self) -> u64 {
        self.tuneis().canais_acima_do_teto()
    }

    /// Quantos datagramas de carga chegaram a um canal `j` **antes** da
    /// abertura dele: descartados e contados, nunca motivo de corte (a
    /// abertura pode só ter se atrasado ou se perdido, e quem abre a repete).
    #[must_use]
    pub fn datagramas_j_antes_da_abertura(&self) -> u64 {
        self.tuneis().dados_antes_da_abertura()
    }
}

// ---- o que depende do banco e do disco ----

/// Os poderes que o manifesto do MOD **ligado** declara; vazio para um MOD que
/// não está ligado ou cujo pacote não abre.
async fn poderes_do_mod(server: &Server, mod_id: &str) -> Vec<Poder> {
    let Some(raizes) = &server.mods_dir else {
        tracing::warn!(mod_id = %mod_id, "túnel: o servidor não tem pasta de MODs; sem poderes");
        return Vec::new();
    };
    let hash = {
        let banco = server.persistence.lock().await;
        match crate::persistence::mods::enabled(&banco) {
            Ok(ligados) => ligados.into_iter().find(|m| m.id == mod_id).map(|m| m.hash),
            Err(erro) => {
                tracing::warn!(mod_id = %mod_id, %erro, "túnel: não deu para ler a tabela de MODs; sem poderes");
                None
            }
        }
    };
    let Some(hash) = hash else {
        tracing::info!(mod_id = %mod_id, "túnel: o MOD não está ligado; sem poderes");
        return Vec::new();
    };
    // O mesmo `mod.json` da pasta do hash que as permissões declaradas leem.
    let texto = match tokio::fs::read_to_string(raizes.pacote_de(&hash).join("mod.json")).await {
        Ok(texto) => texto,
        Err(erro) => {
            tracing::warn!(mod_id = %mod_id, %erro, "túnel: o mod.json do pacote não abriu; sem poderes");
            return Vec::new();
        }
    };
    match seele_proto::mods::read_manifest(&texto) {
        Ok(manifesto) => manifesto.poderes,
        Err(erro) => {
            tracing::warn!(mod_id = %mod_id, %erro, "túnel: o manifesto do pacote não passou na leitura; sem poderes");
            Vec::new()
        }
    }
}

/// A máquina de uma sessão passa a servir um túnel.
///
/// # Errors
///
/// [`RecusaDeTunel::ForaDoAceite`] (o túnel fora do aceite de `maquina`), ou
/// a recusa nomeada de [`Tuneis::servir`].
pub async fn atender_servir(
    server: &Server,
    maquina: PersonId,
    id_da_conexao: u64,
    conexao: &Conexao,
    pedido: PedidoDeTunel<'_>,
) -> Result<(), RecusaDeTunel> {
    // O que vem de fora antes dos cadeados dos livros.
    let poderes = poderes_do_mod(server, pedido.mod_id).await;
    // **O aceite é o único portão**: só serve quem é a máquina aceita de um
    // servidor de jogo deste MOD, pela conexão que aceitou, e só o túnel que a
    // oferta aceita trazia, com a mesma porta e o mesmo protocolo. Não há
    // interruptor ao lado: um servidor que deixasse servir sem aceite deixaria
    // qualquer sessão no 12 abrir uma porta na máquina de outra pessoa em nome
    // de um MOD — e a pessoa nunca teria visto a oferta.
    //
    // `maquinas()` fica tomado até o túnel entrar no livro: uma parada que
    // corra agora ou vê o túnel e o para, ou tira o aceite antes e este pedido
    // é recusado. Solto entre os dois, um túnel podia sobrar de uma máquina que
    // já parou. A ordem é a do doc de `Server::maquinas`: `maquinas` antes de
    // `tuneis`, e o banco já lido.
    let mut maquinas = server.maquinas();
    if let Err(porque) = maquinas.conferir_tunel(
        maquina,
        id_da_conexao,
        pedido.mod_id,
        pedido.nome,
        pedido.protocolo,
        pedido.porta,
    ) {
        tracing::info!(
            mod_id = %pedido.mod_id,
            tunel = %pedido.nome,
            quem = %maquina,
            protocolo = ?pedido.protocolo,
            porta = pedido.porta,
            porque = porque.nome(),
            "túnel: servir fora do aceite"
        );
        // A máquina aceita que não é a dona (API 8 parte 7) ouve o mesmo
        // `ForaDoAceite` no fio, e o servidor conta: é o caso da principal que
        // volta depois de a reserva assumir.
        if porque == crate::maquinas::TunelForaDoAceite::NaoEADona {
            let vezes = maquinas.contar_tunel_de_quem_nao_e_dona(pedido.mod_id);
            tracing::warn!(mod_id = %pedido.mod_id, tunel = %pedido.nome, quem = %maquina, vezes, "túnel: a máquina pediu para servir sem ser a dona do servidor de jogo; recusado");
        }
        return Err(RecusaDeTunel::ForaDoAceite);
    }
    let mut livro = server.tuneis();
    livro.servir(maquina, id_da_conexao, pedido, &poderes)?;
    livro.registrar_conexao(id_da_conexao, conexao.clone());
    drop(livro);
    drop(maquinas);
    Ok(())
}

async fn confere_a_acao(
    server: &Server,
    pessoa: PersonId,
    mod_id: &str,
) -> Result<(), RecusaDeTunel> {
    let banco = server.persistence.lock().await;
    match crate::cargos::autorizar_acao_de_mod(&banco, pessoa, mod_id, ACAO_ENTRAR_NO_JOGO) {
        Ok(()) => Ok(()),
        Err(RecusaDeAcaoDeMod::NaoDeclarada) => Err(RecusaDeTunel::AcaoNaoDeclarada),
        Err(RecusaDeAcaoDeMod::SemPermissao) => Err(RecusaDeTunel::SemPermissao),
    }
}

/// Quem joga pede para alcançar o túnel: existe? a pessoa tem a ação?
///
/// # Errors
///
/// [`RecusaDeTunel::NaoExiste`], [`RecusaDeTunel::AcaoNaoDeclarada`] ou
/// [`RecusaDeTunel::SemPermissao`].
pub async fn atender_entrar(
    server: &Server,
    pessoa: PersonId,
    id_da_conexao: u64,
    mod_id: &str,
    nome: &str,
) -> Result<TunelServido, RecusaDeTunel> {
    if !server.tuneis().existe(mod_id, nome) {
        return Err(RecusaDeTunel::NaoExiste);
    }
    confere_a_acao(server, pessoa, mod_id).await?;
    let servido = server
        .tuneis()
        .entrar(pessoa, id_da_conexao, mod_id, nome)?;
    // **De novo, depois de gravada.** Um cargo revogado entre a leitura de cima
    // e a gravação de cima passou por `reconferir` quando a concessão ainda não
    // existia; sem esta segunda conferência ela ficaria de pé para sempre.
    if let Err(recusa) = confere_a_acao(server, pessoa, mod_id).await {
        // A concessão recém-gravada some, e a máquina é avisada: a apresentação
        // ainda não saiu, mas a retirada é inofensiva sem ligação direta.
        // Quem pediu já recebe a recusa; só a máquina precisa do aviso.
        let retiradas = server.tuneis().sem_permissao(pessoa, mod_id).retiradas;
        anunciar(
            server,
            Fim {
                encerramentos: Vec::new(),
                retiradas,
            },
        );
        return Err(recusa);
    }
    Ok(servido)
}

/// Diz a quem precisa saber que túneis acabaram: no log, com `mod_id=` e
/// `tunel=`, e pelo barramento, endereçado.
pub fn anunciar(server: &Server, fim: impl Into<Fim>) {
    let Fim {
        encerramentos,
        retiradas,
    } = fim.into();
    for r in retiradas {
        tracing::info!(
            mod_id = %r.mod_id,
            tunel = %r.nome,
            maquina = %r.para,
            quem = %r.quem,
            "túnel: a máquina é avisada de que a pessoa saiu do túnel"
        );
        let _ = server.events.send(Event::TunelRetirado {
            para: r.para,
            mod_id: r.mod_id,
            nome: r.nome,
            quem: r.quem,
        });
    }
    for e in encerramentos {
        tracing::info!(
            mod_id = %e.mod_id,
            tunel = %e.nome,
            para = %e.para,
            motivo = ?e.motivo,
            "túnel encerrado"
        );
        let _ = server.events.send(Event::TunelEncerrado {
            para: e.para,
            mod_id: e.mod_id,
            nome: e.nome,
            motivo: e.motivo,
        });
    }
}

/// Os dois avisos da apresentação de um túnel, **na ordem em que saem**: o de
/// quem serve primeiro, o de quem joga depois. `None` quando o túnel vai pela
/// estrela.
///
/// Pura de propósito, para a regra ser afirmável sem servidor: a malha desligada
/// ou uma das pontas sem declaração de identidade (`Pares::apresentar`) são os
/// dois jeitos de o túnel ficar na estrela, e nenhum dos dois é um erro.
#[must_use]
pub fn apresentacao_do_tunel(
    pares: &crate::pares::Pares,
    malha_para_tuneis: bool,
    mod_id: &str,
    nome: &str,
    maquina: PersonId,
    quem_joga: PersonId,
) -> Option<[Event; 2]> {
    use crate::pares::{MotivoDaApresentacao, Papel};
    if !malha_para_tuneis {
        return None;
    }
    let (serve, busca) = pares.apresentar(maquina, quem_joga)?;
    let motivo = MotivoDaApresentacao::Tunel {
        mod_id: mod_id.to_owned(),
        nome: nome.to_owned(),
    };
    Some([
        // Quem serve passa a atender **antes** de quem joga discar: a porta
        // dele só é armada quando a mensagem chega (a razão de
        // `session::apontar_um_par`).
        Event::Apresentacao {
            motivo: motivo.clone(),
            para: maquina,
            com: quem_joga,
            papel: Papel::Serve,
            enderecos: busca.enderecos,
            impressao: busca.impressao,
        },
        Event::Apresentacao {
            motivo,
            para: quem_joga,
            com: maquina,
            papel: Papel::Busca,
            enderecos: serve.enderecos,
            impressao: serve.impressao,
        },
    ])
}

/// Apresenta a máquina a quem acabou de entrar, pela malha, se der. Chamada
/// **depois** do `TunelConcedido` ser escrito: quem joga já tem a resposta do
/// pedido quando o caminho direto começa a ser tentado.
///
/// Sem apresentação nada sai e o túnel vai pela estrela, dito em DEBUG: a
/// malha é alívio, nunca dependência.
pub async fn apresentar(
    server: &Server,
    mod_id: &str,
    nome: &str,
    maquina: PersonId,
    quem_joga: PersonId,
) {
    let avisos = {
        let pares = server.pares.lock().await;
        apresentacao_do_tunel(
            &pares,
            server.malha_para_tuneis,
            mod_id,
            nome,
            maquina,
            quem_joga,
        )
    };
    let Some(avisos) = avisos else {
        tracing::debug!(
            mod_id = %mod_id,
            tunel = %nome,
            malha = server.malha_para_tuneis,
            "túnel: sem apresentação das pontas (malha desligada ou alguém sem identidade \
             declarada); o túnel vai pela estrela"
        );
        return;
    };
    tracing::info!(mod_id = %mod_id, tunel = %nome, %maquina, %quem_joga, "túnel: as pontas foram apresentadas");
    for aviso in avisos {
        let _ = server.events.send(aviso);
    }
}

/// Confere de novo a ação `entrar-no-jogo` de quem está dentro de algum túnel
/// (de `pessoas`, ou de todos): quem a perdeu sai, com `SemPermissao`.
///
/// Chamado por quem gravou uma mudança de cargo, **antes** de difundir o quadro
/// (`session::aplicar_os_cargos`), e quando o conjunto de MODs muda.
pub async fn reconferir(server: &Server, pessoas: Option<&[PersonId]>) {
    let concessoes = server.tuneis().concessoes();
    let mut perderam = Vec::new();
    for (pessoa, mod_id) in concessoes {
        if pessoas.is_some_and(|so| !so.contains(&pessoa)) {
            continue;
        }
        if confere_a_acao(server, pessoa, &mod_id).await.is_err() {
            perderam.push((pessoa, mod_id));
        }
    }
    let mut fim = Fim::default();
    {
        let mut livro = server.tuneis();
        for (pessoa, mod_id) in perderam {
            fim.junta(livro.sem_permissao(pessoa, &mod_id));
        }
    }
    anunciar(server, fim);
}

/// Fecha os túneis dos MODs que não estão mais ligados, ou que deixaram de
/// declarar o poder `porta`.
async fn fechar_os_que_nao_valem_mais(server: &Server) {
    let servidos = server.tuneis().mods_servidos();
    for mod_id in servidos {
        if poderes_do_mod(server, &mod_id)
            .await
            .contains(&Poder::Porta)
        {
            continue;
        }
        let fim = server.tuneis().mod_desligado(&mod_id);
        anunciar(server, fim);
    }
}

/// A tarefa que acompanha o conjunto de MODs: desligar um MOD (ou trocar o
/// pacote por um que não declara a porta) fecha os túneis dele e dispensa as
/// máquinas dele (`crate::maquinas::fechar_as_que_nao_valem_mais`), e uma
/// mudança no conjunto confere de novo quem está dentro.
///
/// Termina quando o servidor acaba (o `Weak` não o segura de pé).
pub async fn acompanhar_os_mods(server: Weak<Server>) {
    let mut mudou = {
        let Some(s) = server.upgrade() else {
            return;
        };
        let banco = s.persistence.lock().await;
        banco.mods_mudaram()
    };
    while mudou.changed().await.is_ok() {
        let Some(s) = server.upgrade() else {
            break;
        };
        // As máquinas primeiro: a dispensa chega a quem opera antes de os
        // túneis dela caírem.
        crate::maquinas::fechar_as_que_nao_valem_mais(&s).await;
        fechar_os_que_nao_valem_mais(&s).await;
        reconferir(&s, None).await;
    }
}

// ---- o repasse pela estrela ----

fn cortar_t(envio: &mut Envio, recebe: &mut Recebe) {
    let _ = envio.reset(crate::tela::CODIGO_DE_CORTE);
    let _ = recebe.stop(crate::tela::CODIGO_DE_CORTE);
}

/// Copia `de` em `para` até o fim de `de`, e então termina `para`.
async fn copiar(
    de: &mut Recebe,
    para: &mut Envio,
    contador: &AtomicU64,
) -> Result<(), FalhaDoCanal> {
    let mut buf = vec![0_u8; seele_enlace::MAX_DADOS];
    loop {
        let Some(n) = de.read(&mut buf).await? else {
            return para.finish();
        };
        let Some(pedaco) = buf.get(..n) else {
            return Ok(());
        };
        para.write_all(pedaco).await?;
        contador.fetch_add(n as u64, Ordering::Relaxed);
    }
}

/// Repassa um canal de túnel que `de` abriu neste servidor para a máquina que
/// serve o túnel que a abertura nomeia, e copia nos dois sentidos até o fim de
/// um dos lados. Diz no log o total e o motivo do fim.
///
/// # Errors
///
/// A recusa nomeada quando o canal é cortado: `SemPermissao` (sem concessão, ou
/// uma abertura que não vale), `CanaisEsgotados` (a máquina no teto) ou
/// `MaquinaSaiu`.
pub async fn repassar(
    server: &Server,
    de: PersonId,
    canal: CanalDeTunel,
) -> Result<(), RepasseCortado> {
    let mut tunel = None;
    let resultado = match canal {
        CanalDeTunel::Ordenado(envio, recebe) => {
            repassar_t(server, de, envio, recebe, &mut tunel).await
        }
        CanalDeTunel::SemOrdem(datagramas) => repassar_j(server, de, datagramas, &mut tunel).await,
    };
    resultado.map_err(|motivo| RepasseCortado { tunel, motivo })
}

/// Um canal que o repasse cortou: o motivo enumerado e, quando a abertura foi
/// lida, o túnel que ela nomeava (`mod_id`, `nome`). Quem chama diz o motivo a
/// quem abriu o canal (`TunelRecusado`), em vez de deixá-lo com um socket
/// fechado e nenhuma razão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepasseCortado {
    /// `(mod_id, nome)` da abertura; `None` quando ela nem foi lida.
    pub tunel: Option<(String, String)>,
    /// Por que o canal foi cortado.
    pub motivo: RecusaDeTunel,
}

impl RepasseCortado {
    /// O `TunelRecusado` a mandar a quem abriu o canal, ou `None` quando não
    /// há o que dizer: a abertura não foi lida, ou o corte foi por falta de
    /// concessão (`SemPermissao`), que quem a perdeu já soube por
    /// `TunelEncerrado` (e quem nunca a teve não tem túnel a que se referir).
    #[must_use]
    pub fn aviso(&self, para: PersonId) -> Option<Event> {
        let (mod_id, nome) = self.tunel.clone()?;
        if self.motivo == RecusaDeTunel::SemPermissao {
            return None;
        }
        Some(Event::TunelRecusado {
            para,
            mod_id,
            nome,
            motivo: self.motivo,
        })
    }
}

fn corta_e_conta(
    server: &Server,
    de: PersonId,
    abertura: &AberturaDeTunel,
    por_que: CorteDeCanal,
) -> RecusaDeTunel {
    server.tuneis().contar_sem_concessao();
    tracing::warn!(
        mod_id = %abertura.mod_id,
        tunel = %abertura.nome,
        pessoa = %de,
        motivo = ?por_que,
        "túnel: canal cortado no anfitrião, sem tocar na máquina"
    );
    match por_que {
        CorteDeCanal::SemConcessao | CorteDeCanal::TipoErrado => RecusaDeTunel::SemPermissao,
        CorteDeCanal::SemMaquina => RecusaDeTunel::MaquinaSaiu,
    }
}

/// A vaga de canal de `de`, ou o corte por teto: contado e dito no log com
/// quem, MOD, túnel e o teto, porque quem joga só vê o canal recusado.
fn reservar_ou_corta(
    server: &Server,
    de: PersonId,
    abertura: &AberturaDeTunel,
) -> Result<CanalRepassado, RecusaDeTunel> {
    let vaga = server.tuneis().reservar_canal(de);
    vaga.ok_or_else(|| {
        server
            .tuneis()
            .acima_do_teto
            .fetch_add(1, Ordering::Relaxed);
        tracing::warn!(
            quem = %de,
            mod_id = %abertura.mod_id,
            tunel = %abertura.nome,
            teto = MAX_CANAIS_REPASSADOS_POR_PESSOA,
            "túnel: canal cortado, a pessoa já tem o teto de canais repassados vivos"
        );
        RecusaDeTunel::CanaisEsgotados
    })
}

fn motivo_de_abrir(motivo: &Motivo) -> RecusaDeTunel {
    match motivo {
        Motivo::CanaisEsgotados => RecusaDeTunel::CanaisEsgotados,
        _ => RecusaDeTunel::MaquinaSaiu,
    }
}

async fn repassar_t(
    server: &Server,
    de: PersonId,
    mut envio: Envio,
    mut recebe: Recebe,
    tunel: &mut Option<(String, String)>,
) -> Result<(), RecusaDeTunel> {
    let lida = tokio::time::timeout(
        PRAZO_DA_ABERTURA,
        crate::frame::read::<AberturaDeTunel>(&mut recebe),
    )
    .await;
    let abertura = match lida {
        Ok(Ok(abertura)) => abertura,
        outro => {
            server.tuneis().contar_sem_concessao();
            tracing::warn!(
                pessoa = %de,
                motivo = ?outro.map(|r| r.map(|_| ())),
                "túnel: canal sem abertura legível, cortado"
            );
            cortar_t(&mut envio, &mut recebe);
            return Err(RecusaDeTunel::SemPermissao);
        }
    };
    *tunel = Some((abertura.mod_id.clone(), abertura.nome.clone()));
    let rota = server.tuneis().rota(de, &abertura, true);
    let rota = match rota {
        Ok(rota) => rota,
        Err(por_que) => {
            cortar_t(&mut envio, &mut recebe);
            return Err(corta_e_conta(server, de, &abertura, por_que));
        }
    };
    let _vaga = match reservar_ou_corta(server, de, &abertura) {
        Ok(vaga) => vaga,
        Err(motivo) => {
            cortar_t(&mut envio, &mut recebe);
            return Err(motivo);
        }
    };
    let mut vigia = rota.vigia;
    let aberto = rota.conexao.abrir_tunel(Confiabilidade::Ordenada).await;
    let (mut m_envio, mut m_recebe) = match aberto {
        Ok(CanalDeTunel::Ordenado(e, r)) => (e, r),
        Ok(CanalDeTunel::SemOrdem(_)) => {
            tracing::warn!(
                mod_id = %abertura.mod_id,
                tunel = %abertura.nome,
                pessoa = %de,
                "túnel: o enlace devolveu um j onde se pediu um t; canal cortado"
            );
            cortar_t(&mut envio, &mut recebe);
            return Err(RecusaDeTunel::MaquinaSaiu);
        }
        Err(motivo) => {
            tracing::warn!(
                mod_id = %abertura.mod_id,
                tunel = %abertura.nome,
                pessoa = %de,
                %motivo,
                "túnel: a máquina não abriu o canal"
            );
            cortar_t(&mut envio, &mut recebe);
            return Err(motivo_de_abrir(&motivo));
        }
    };
    let reescrita = AberturaDeTunel {
        de: Some(de),
        ..abertura.clone()
    };
    if let Err(erro) = crate::frame::write(&mut m_envio, &reescrita).await {
        tracing::warn!(
            mod_id = %abertura.mod_id,
            tunel = %abertura.nome,
            pessoa = %de,
            %erro,
            "túnel: a abertura não chegou à máquina"
        );
        cortar_t(&mut envio, &mut recebe);
        cortar_t(&mut m_envio, &mut m_recebe);
        return Err(RecusaDeTunel::MaquinaSaiu);
    }
    let antes = rota.contador.load(Ordering::Relaxed);
    let fim = {
        let ida = copiar(&mut recebe, &mut m_envio, &rota.contador);
        let volta = copiar(&mut m_recebe, &mut envio, &rota.contador);
        tokio::select! {
            r = async { tokio::try_join!(ida, volta).map(|_| ()) } => match r {
                Ok(()) => "os dois lados terminaram".to_owned(),
                Err(erro) => format!("um dos lados caiu: {erro}"),
            },
            _ = vigia.changed() => "a concessão acabou".to_owned(),
        }
    };
    // O que não terminou limpo é cortado dos dois lados, para ninguém ficar
    // esperando um canal que o outro lado já largou.
    cortar_t(&mut envio, &mut recebe);
    cortar_t(&mut m_envio, &mut m_recebe);
    tracing::info!(
        mod_id = %abertura.mod_id,
        tunel = %abertura.nome,
        pessoa = %de,
        maquina = %rota.maquina,
        bytes = rota.contador.load(Ordering::Relaxed).saturating_sub(antes),
        motivo = %fim,
        "túnel: canal t repassado pela estrela terminou"
    );
    Ok(())
}

/// Lê o canal `j` de quem joga até a abertura (`TAG_DE_ABERTURA`).
///
/// Carga que chega antes dela é descartada e contada, e **não** corta o canal:
/// o `j` não ordena nem retransmite, e a abertura pode ter se atrasado ou se
/// perdido (quem abre a repete). Só uma abertura malformada, o fim do canal ou
/// o prazo vencido cortam.
async fn ler_a_abertura_j(
    server: &Server,
    de: PersonId,
    datagramas: &mut seele_enlace::Datagramas,
) -> Option<AberturaDeTunel> {
    let fim = tokio::time::Instant::now() + PRAZO_DA_ABERTURA;
    loop {
        let bytes = match tokio::time::timeout_at(fim, datagramas.receber()).await {
            Ok(Some(bytes)) => bytes,
            outro => {
                server.tuneis().contar_sem_concessao();
                tracing::warn!(pessoa = %de, fechou = outro.is_ok(), "túnel: canal j sem abertura no prazo, cortado");
                return None;
            }
        };
        match ler_datagrama_j(&bytes) {
            DatagramaJ::Abertura(corpo) => {
                return match seele_proto::control::decode::<AberturaDeTunel>(corpo) {
                    Ok(abertura) => Some(abertura),
                    Err(erro) => {
                        server.tuneis().contar_sem_concessao();
                        tracing::warn!(pessoa = %de, %erro, "túnel: abertura do canal j malformada, cortado");
                        None
                    }
                };
            }
            DatagramaJ::Dado(_) | DatagramaJ::Ilegivel => {
                server.tuneis().contar_dado_antes_da_abertura();
                tracing::debug!(pessoa = %de, "túnel: carga de canal j antes da abertura, descartada");
            }
        }
    }
}

async fn repassar_j(
    server: &Server,
    de: PersonId,
    mut datagramas: seele_enlace::Datagramas,
    tunel: &mut Option<(String, String)>,
) -> Result<(), RecusaDeTunel> {
    let Some(abertura) = ler_a_abertura_j(server, de, &mut datagramas).await else {
        return Err(RecusaDeTunel::SemPermissao);
    };
    *tunel = Some((abertura.mod_id.clone(), abertura.nome.clone()));
    let rota = server.tuneis().rota(de, &abertura, false);
    let rota = match rota {
        Ok(rota) => rota,
        Err(por_que) => return Err(corta_e_conta(server, de, &abertura, por_que)),
    };
    let _vaga = reservar_ou_corta(server, de, &abertura)?;
    let mut vigia = rota.vigia;
    let mut da_maquina = match rota.conexao.abrir_tunel(Confiabilidade::SemOrdem).await {
        Ok(CanalDeTunel::SemOrdem(d)) => d,
        Ok(CanalDeTunel::Ordenado(..)) => {
            tracing::warn!(
                mod_id = %abertura.mod_id,
                tunel = %abertura.nome,
                pessoa = %de,
                "túnel: o enlace devolveu um t onde se pediu um j; canal cortado"
            );
            return Err(RecusaDeTunel::MaquinaSaiu);
        }
        Err(motivo) => {
            tracing::warn!(
                mod_id = %abertura.mod_id,
                tunel = %abertura.nome,
                pessoa = %de,
                %motivo,
                "túnel: a máquina não abriu o canal j"
            );
            return Err(motivo_de_abrir(&motivo));
        }
    };
    let reescrita = AberturaDeTunel {
        de: Some(de),
        ..abertura.clone()
    };
    let quadro = match datagrama_de_abertura(&reescrita) {
        Ok(quadro) => quadro,
        Err(erro) => {
            tracing::warn!(
                mod_id = %abertura.mod_id,
                tunel = %abertura.nome,
                pessoa = %de,
                %erro,
                "túnel: a abertura do canal j não codificou; canal cortado"
            );
            return Err(RecusaDeTunel::MaquinaSaiu);
        }
    };
    if let Err(erro) = da_maquina.mandar(&quadro) {
        tracing::warn!(
            mod_id = %abertura.mod_id,
            tunel = %abertura.nome,
            pessoa = %de,
            %erro,
            "túnel: a abertura do canal j não chegou à máquina; canal cortado"
        );
        return Err(RecusaDeTunel::MaquinaSaiu);
    }
    let antes = rota.contador.load(Ordering::Relaxed);
    let mut descartados = 0_u64;
    let fim = loop {
        tokio::select! {
            recebido = datagramas.receber() => {
                let Some(bytes) = recebido else { break "quem joga fechou o canal" };
                // **O tag passa como veio** nos dois sentidos, exceto a
                // abertura: a que quem joga repete traz `de: None`, e a
                // máquina só pode ver a reescrita pelo anfitrião. Cada
                // repetição vira o `quadro` reescrito de novo, que cobre a
                // perda do primeiro no trecho até a máquina.
                let (a_enviar, carga): (&[u8], u64) = match ler_datagrama_j(&bytes) {
                    DatagramaJ::Abertura(_) => (&quadro, 0),
                    DatagramaJ::Dado(c) => (&bytes, c.len() as u64),
                    DatagramaJ::Ilegivel => { descartados += 1; continue; }
                };
                match da_maquina.mandar(a_enviar) {
                    Ok(()) => { rota.contador.fetch_add(carga, Ordering::Relaxed); }
                    Err(Recusa::Fechada) => break "a máquina fechou o canal",
                    Err(_) => descartados += 1,
                }
            }
            recebido = da_maquina.receber() => {
                let Some(bytes) = recebido else { break "a máquina fechou o canal" };
                // Só carga volta da máquina; o que não for é descartado em vez
                // de repassado a quem joga.
                let DatagramaJ::Dado(c) = ler_datagrama_j(&bytes) else { descartados += 1; continue; };
                let carga = c.len() as u64;
                match datagramas.mandar(&bytes) {
                    Ok(()) => { rota.contador.fetch_add(carga, Ordering::Relaxed); }
                    Err(Recusa::Fechada) => break "quem joga fechou o canal",
                    Err(_) => descartados += 1,
                }
            }
            _ = vigia.changed() => break "a concessão acabou",
        }
    };
    tracing::info!(
        mod_id = %abertura.mod_id,
        tunel = %abertura.nome,
        pessoa = %de,
        maquina = %rota.maquina,
        bytes = rota.contador.load(Ordering::Relaxed).saturating_sub(antes),
        descartados,
        motivo = fim,
        "túnel: canal j repassado pela estrela terminou"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_canal_de_controle_passa_em_qualquer_tunel_e_o_resto_so_no_seu_protocolo() {
        use TipoDeCanalDeTunel::{Controle, Tcp, Udp};
        for protocolo in [ProtocoloDeTunel::Tcp, ProtocoloDeTunel::Udp] {
            assert!(tipo_combina(true, Controle, protocolo));
            // O controle é ordenado: num `j` não faz sentido.
            assert!(!tipo_combina(false, Controle, protocolo));
        }
        assert!(tipo_combina(true, Tcp, ProtocoloDeTunel::Tcp));
        assert!(!tipo_combina(true, Tcp, ProtocoloDeTunel::Udp));
        assert!(tipo_combina(false, Udp, ProtocoloDeTunel::Udp));
        assert!(!tipo_combina(false, Udp, ProtocoloDeTunel::Tcp));
        assert!(!tipo_combina(true, Udp, ProtocoloDeTunel::Udp));
    }

    #[test]
    fn o_ping_do_controle_nao_conta_como_repasse_do_jogo() {
        use std::sync::atomic::Ordering;
        use TipoDeCanalDeTunel::{Controle, Tcp, Udp};
        let do_tunel = Arc::new(AtomicU64::new(0));
        for jogo in [Tcp, Udp] {
            assert!(
                Arc::ptr_eq(&contador_do_canal(jogo, &do_tunel), &do_tunel),
                "o tráfego do jogo tem de entrar no contador do túnel"
            );
        }
        // O ping que quem joga manda pela estrela antes de o direto existir
        // não é jogo repassado: se contasse, «direto é direto» (contador em
        // zero) nunca valeria, porque o túnel nasce na estrela.
        let do_ping = contador_do_canal(Controle, &do_tunel);
        do_ping.fetch_add(14, Ordering::Relaxed);
        assert_eq!(do_tunel.load(Ordering::Relaxed), 0);
    }

    use seele_proto::ids::PersonId;
    use seele_proto::mods::Poder;
    use seele_proto::tunel::{
        AberturaDeTunel, MotivoDoFimDoTunel, ProtocoloDeTunel, RecusaDeTunel, TipoDeCanalDeTunel,
    };

    const MAQUINA: PersonId = PersonId(1);
    const JOGA: PersonId = PersonId(2);

    fn pedido<'a>(mod_id: &'a str, nome: &'a str) -> PedidoDeTunel<'a> {
        PedidoDeTunel {
            mod_id,
            nome,
            protocolo: ProtocoloDeTunel::Tcp,
            porta: 25565,
        }
    }

    /// O teto é por pessoa e conta canais **vivos**: a vaga volta no `Drop`.
    #[test]
    fn o_teto_de_canais_e_por_pessoa_e_a_vaga_volta_no_drop() {
        let livro = Tuneis::default();
        let mut vagas: Vec<_> = (0..MAX_CANAIS_REPASSADOS_POR_PESSOA)
            .map(|_| livro.reservar_canal(PersonId(1)).expect("cabe"))
            .collect();
        assert!(livro.reservar_canal(PersonId(1)).is_none(), "o 17º passou");
        assert!(
            livro.reservar_canal(PersonId(2)).is_some(),
            "o teto é de todos"
        );
        drop(vagas.pop());
        assert!(
            livro.reservar_canal(PersonId(1)).is_some(),
            "a vaga não voltou"
        );
    }

    fn com_tunel() -> Tuneis {
        let mut t = Tuneis::default();
        t.servir(MAQUINA, 7, pedido("x/jogo", "eco"), &[Poder::Porta])
            .expect("serve");
        t
    }

    fn abertura(nome: &str) -> AberturaDeTunel {
        AberturaDeTunel {
            mod_id: "x/jogo".into(),
            nome: nome.into(),
            fluxo: 1,
            tipo: TipoDeCanalDeTunel::Tcp,
            de: None,
        }
    }

    #[test]
    fn servir_exige_o_poder_porta() {
        let mut t = Tuneis::default();
        for sem in [&[][..], &[Poder::Programa][..]] {
            assert_eq!(
                t.servir(MAQUINA, 7, pedido("x/jogo", "eco"), sem),
                Err(RecusaDeTunel::ModSemPorta)
            );
        }
        assert_eq!(
            t.servir(MAQUINA, 7, pedido("x/jogo", "eco"), &[Poder::Porta]),
            Ok(())
        );
    }

    #[test]
    fn o_nome_e_unico_por_mod() {
        let mut t = com_tunel();
        assert_eq!(
            t.servir(PersonId(9), 8, pedido("x/jogo", "eco"), &[Poder::Porta]),
            Err(RecusaDeTunel::NomeTomado)
        );
        // Outro MOD pode usar o mesmo nome; a mesma máquina servindo de novo
        // (a conexão nova depois de uma queda) toma o túnel de volta.
        assert_eq!(
            t.servir(PersonId(9), 8, pedido("y/jogo", "eco"), &[Poder::Porta]),
            Ok(())
        );
        assert_eq!(
            t.servir(MAQUINA, 11, pedido("x/jogo", "eco"), &[Poder::Porta]),
            Ok(())
        );
    }

    #[test]
    fn os_tetos_de_tuneis_por_mod_e_por_servidor() {
        let mut t = Tuneis::default();
        for i in 0..MAX_TUNEIS_POR_MOD {
            t.servir(
                MAQUINA,
                7,
                pedido("x/jogo", &format!("n{i}")),
                &[Poder::Porta],
            )
            .expect("cabe");
        }
        assert_eq!(
            t.servir(MAQUINA, 7, pedido("x/jogo", "extra"), &[Poder::Porta]),
            Err(RecusaDeTunel::TuneisDemais)
        );
        for m in 0..MAX_TUNEIS_POR_SERVIDOR {
            let mod_id = format!("a/m{m}");
            for i in 0..MAX_TUNEIS_POR_MOD {
                let r = t.servir(
                    MAQUINA,
                    7,
                    pedido(&mod_id, &format!("n{i}")),
                    &[Poder::Porta],
                );
                if r.is_err() {
                    assert_eq!(r, Err(RecusaDeTunel::TuneisDemais));
                    return;
                }
            }
        }
        panic!("o teto do servidor nunca recusou");
    }

    #[test]
    fn entrar_sem_tunel_e_nao_existe() {
        let mut t = Tuneis::default();
        assert_eq!(
            t.entrar(JOGA, 3, "x/jogo", "eco").map(|_| ()),
            Err(RecusaDeTunel::NaoExiste)
        );
        let mut t = com_tunel();
        let servido = t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        assert_eq!(servido.porta, 25565);
        assert_eq!(servido.protocolo, ProtocoloDeTunel::Tcp);
    }

    #[test]
    fn a_maquina_que_sai_encerra_para_quem_estava_dentro() {
        let mut t = com_tunel();
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        // Uma conexão velha da mesma pessoa que cai depois da nova não leva o
        // túnel junto.
        assert!(t.saiu(MAQUINA, 99).vazio());
        let fim = t.saiu(MAQUINA, 7).encerramentos;
        assert_eq!(fim.len(), 1);
        assert_eq!(fim[0].para, JOGA);
        assert_eq!(fim[0].motivo, MotivoDoFimDoTunel::MaquinaSaiu);
        assert_eq!(
            t.entrar(JOGA, 3, "x/jogo", "eco").map(|_| ()),
            Err(RecusaDeTunel::NaoExiste)
        );
    }

    #[test]
    fn desligar_o_mod_encerra_os_tuneis_dele() {
        let mut t = com_tunel();
        t.servir(MAQUINA, 7, pedido("y/jogo", "eco"), &[Poder::Porta])
            .expect("outro mod");
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        let mut fim = t.mod_desligado("x/jogo");
        fim.sort_by_key(|e| e.para.0);
        // Quem servia e quem jogava ficam sabendo, os dois.
        assert_eq!(fim.len(), 2);
        assert_eq!(fim[0].para, MAQUINA);
        assert_eq!(fim[1].para, JOGA);
        assert!(fim
            .iter()
            .all(|e| e.motivo == MotivoDoFimDoTunel::ModDesligado));
        // O de outro MOD segue de pé.
        assert!(t.entrar(JOGA, 3, "y/jogo", "eco").is_ok());
    }

    #[test]
    fn parar_encerra_para_quem_joga_e_so_a_dona_para() {
        let mut t = com_tunel();
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        assert!(t.parar(PersonId(9), "x/jogo", "eco").is_empty());
        let fim = t.parar(MAQUINA, "x/jogo", "eco");
        assert_eq!(fim.len(), 1);
        assert_eq!(fim[0].motivo, MotivoDoFimDoTunel::MaquinaParou);
    }

    /// **Quando o servidor para o túnel, a máquina ouve** (revisão final,
    /// I4): a reserva que assume a posse, o aceite que muda os túneis e o
    /// outro pedido passam por [`Tuneis::parar_pelo_servidor`], e a máquina
    /// recebe `MaquinaParou` ao lado de quem jogava — sem isso ela seguia
    /// servindo as ligações diretas. Só a dona do nome.
    #[test]
    fn parar_pelo_servidor_diz_a_maquina_e_a_quem_joga() {
        let mut t = com_tunel();
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        assert!(t
            .parar_pelo_servidor(PersonId(9), "x/jogo", "eco")
            .is_empty());
        let mut fim = t.parar_pelo_servidor(MAQUINA, "x/jogo", "eco");
        fim.sort_by_key(|e| e.para.0);
        let para: Vec<PersonId> = fim.iter().map(|e| e.para).collect();
        let mut esperado = vec![MAQUINA, JOGA];
        esperado.sort_by_key(|p| p.0);
        assert_eq!(para, esperado);
        assert!(fim
            .iter()
            .all(|e| e.motivo == MotivoDoFimDoTunel::MaquinaParou));
        assert!(!t.existe("x/jogo", "eco"));
    }

    #[test]
    fn a_pessoa_que_sai_ou_perde_a_permissao_some_das_concessoes() {
        let mut t = com_tunel();
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        assert_eq!(t.mods_em_que_esta(JOGA), vec!["x/jogo".to_owned()]);
        let fim = t.sem_permissao(JOGA, "x/jogo");
        assert_eq!(fim.encerramentos.len(), 1);
        assert_eq!(fim.encerramentos[0].para, JOGA);
        assert_eq!(
            fim.encerramentos[0].motivo,
            MotivoDoFimDoTunel::SemPermissao
        );
        // E a máquina é avisada: a ligação direta é dela.
        assert_eq!(
            fim.retiradas,
            vec![Retirada {
                para: MAQUINA,
                mod_id: "x/jogo".into(),
                nome: "eco".into(),
                quem: JOGA,
            }]
        );
        assert!(t.mods_em_que_esta(JOGA).is_empty());

        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        // A conexão velha saindo não tira quem já voltou por outra.
        t.entrar(JOGA, 4, "x/jogo", "eco").expect("volta");
        t.saiu(JOGA, 3);
        assert_eq!(t.mods_em_que_esta(JOGA).len(), 1);
        t.saiu(JOGA, 4);
        assert!(t.mods_em_que_esta(JOGA).is_empty());
    }

    /// **I1, o livro.** Toda saída de uma pessoa de um túnel que segue de pé
    /// (perder a ação, deixar o túnel, a conexão dela cair) diz à máquina que
    /// serve, e só a ela; a saída da própria máquina e o túnel que acaba por
    /// inteiro não geram retirada (o `Encerramento` já cobre).
    #[test]
    fn quem_sai_do_tunel_e_dito_a_maquina_por_todos_os_caminhos() {
        let retirada = |quem| Retirada {
            para: MAQUINA,
            mod_id: "x/jogo".into(),
            nome: "eco".into(),
            quem,
        };
        // Deixar o túnel.
        let mut t = com_tunel();
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        assert!(t.sair(PersonId(9), "x/jogo", "eco").vazio());
        assert_eq!(
            t.sair(JOGA, "x/jogo", "eco").retiradas,
            vec![retirada(JOGA)]
        );
        // Já fora: nada a dizer.
        assert!(t.sair(JOGA, "x/jogo", "eco").vazio());

        // A conexão que entrou cai.
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        // Uma conexão velha caindo não retira quem voltou por outra.
        t.entrar(JOGA, 4, "x/jogo", "eco").expect("volta");
        assert!(t.saiu(JOGA, 3).vazio());
        let fim = t.saiu(JOGA, 4);
        assert_eq!(fim.retiradas, vec![retirada(JOGA)]);
        assert!(fim.encerramentos.is_empty());

        // A máquina que entra no próprio túnel e sai não se retira de si.
        t.entrar(MAQUINA, 7, "x/jogo", "eco")
            .expect("a máquina entra");
        assert!(t.sem_permissao(MAQUINA, "x/jogo").retiradas.is_empty());

        // O túnel que acaba por inteiro: encerramentos, nenhuma retirada.
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        let fim = Fim::from(t.parar(MAQUINA, "x/jogo", "eco"));
        assert!(fim.retiradas.is_empty());
        assert!(!fim.encerramentos.is_empty());
    }

    #[test]
    fn so_repassa_quem_tem_concessao() {
        let mut t = com_tunel();
        assert_eq!(t.pode_repassar(JOGA, &abertura("eco")), None);
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        assert_eq!(t.pode_repassar(JOGA, &abertura("eco")), Some(MAQUINA));
        assert_eq!(t.pode_repassar(JOGA, &abertura("outro")), None);
        assert_eq!(t.pode_repassar(PersonId(9), &abertura("eco")), None);
        t.sem_permissao(JOGA, "x/jogo");
        assert_eq!(t.pode_repassar(JOGA, &abertura("eco")), None);
    }

    #[test]
    fn o_teto_de_canais_da_maquina_vira_a_recusa_nomeada() {
        assert_eq!(
            motivo_de_abrir(&Motivo::CanaisEsgotados),
            RecusaDeTunel::CanaisEsgotados
        );
        assert_eq!(
            motivo_de_abrir(&Motivo::Caiu("x".into())),
            RecusaDeTunel::MaquinaSaiu
        );
    }

    #[test]
    fn so_o_corte_que_nao_e_falta_de_concessao_vira_aviso_a_quem_abriu() {
        let corte = |tunel: Option<(&str, &str)>, motivo| RepasseCortado {
            tunel: tunel.map(|(m, n)| (m.to_owned(), n.to_owned())),
            motivo,
        };
        let jogador = PersonId(3);
        let aviso = corte(Some(("x/jogo", "eco")), RecusaDeTunel::CanaisEsgotados).aviso(jogador);
        assert!(matches!(
            aviso,
            Some(Event::TunelRecusado { para, motivo: RecusaDeTunel::CanaisEsgotados, .. })
                if para == jogador
        ));
        // Sem concessão quem perdeu já soube por `TunelEncerrado`; sem
        // abertura lida não há túnel a nomear.
        assert!(corte(Some(("x/jogo", "eco")), RecusaDeTunel::SemPermissao)
            .aviso(jogador)
            .is_none());
        assert!(corte(None, RecusaDeTunel::MaquinaSaiu)
            .aviso(jogador)
            .is_none());
    }

    #[test]
    fn a_apresentacao_do_tunel_sai_na_ordem_certa_e_nada_sem_malha_ou_sem_declaracao() {
        use crate::pares::{MotivoDaApresentacao, Papel};
        use seele_proto::control::ConsentimentoDePar;
        let ende = |n: u8| std::net::SocketAddr::from(([192, 168, 1, n], 8383));
        let mut pares = crate::pares::Pares::nova();
        for (pessoa, n) in [(MAQUINA, 1), (JOGA, 2)] {
            pares.declarou(
                pessoa,
                u64::from(n),
                ConsentimentoDePar::de_ninguem(),
                format!("{n}").repeat(64),
                vec![ende(n)],
                ende(n),
            );
        }
        let avisos = apresentacao_do_tunel(&pares, true, "x/jogo", "mundo", MAQUINA, JOGA)
            .expect("as duas declararam e a malha está ligada");
        let tunel = MotivoDaApresentacao::Tunel {
            mod_id: "x/jogo".into(),
            nome: "mundo".into(),
        };
        // `SirvaTunelPara` (a máquina) primeiro, `AlcanceTunelPor` (quem joga)
        // depois, cada um com o endereço e a impressão da OUTRA ponta.
        assert!(
            matches!(
                &avisos[0],
                Event::Apresentacao { motivo, para, com, papel: Papel::Serve, enderecos, impressao }
                    if *motivo == tunel && *para == MAQUINA && *com == JOGA
                        && enderecos.contains(&ende(2)) && *impressao == "2".repeat(64)
            ),
            "{:?}",
            avisos[0]
        );
        assert!(
            matches!(
                &avisos[1],
                Event::Apresentacao { motivo, para, com, papel: Papel::Busca, enderecos, impressao }
                    if *motivo == tunel && *para == JOGA && *com == MAQUINA
                        && enderecos.contains(&ende(1)) && *impressao == "1".repeat(64)
            ),
            "{:?}",
            avisos[1]
        );
        // A malha desligada leva o túnel pela estrela.
        assert!(apresentacao_do_tunel(&pares, false, "x/jogo", "mundo", MAQUINA, JOGA).is_none());
        // E quem não declarou identidade também.
        assert!(
            apresentacao_do_tunel(&pares, true, "x/jogo", "mundo", MAQUINA, PersonId(9)).is_none()
        );
    }
}
