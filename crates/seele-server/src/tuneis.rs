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
//! (`crate::frame`, o mesmo dos canais `b`); no `j`, o primeiro datagrama (o
//! `control::encode` da abertura, sem prefixo de comprimento). O anfitrião
//! reescreve o campo `de` com quem de fato abriu o canal, e é esse o motivo de
//! a máquina não precisar confiar no que a rede diz.
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
    AberturaDeTunel, MotivoDoFimDoTunel, ProtocoloDeTunel, RecusaDeTunel, TipoDeCanalDeTunel,
    ACAO_ENTRAR_NO_JOGO,
};

use crate::server::{Event, Server};
use crate::transporte::Conexao;

/// Quantos túneis um MOD pode servir neste servidor.
pub const MAX_TUNEIS_POR_MOD: usize = 8;

/// Quantos túneis o servidor inteiro guarda.
pub const MAX_TUNEIS_POR_SERVIDOR: usize = 32;

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

/// O livro dos túneis.
#[derive(Default)]
pub struct Tuneis {
    servidos: HashMap<Chave, Entrada>,
    /// As conexões das máquinas que servem, por `stable_id`.
    conexoes: HashMap<u64, Conexao>,
    contadores: HashMap<Chave, Arc<AtomicU64>>,
    sem_concessao: AtomicU64,
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

    /// Quem joga deixa o túnel. Diz se havia o que tirar.
    pub fn sair(&mut self, pessoa: PersonId, mod_id: &str, nome: &str) -> bool {
        self.servidos
            .get_mut(&chave(mod_id, nome))
            .is_some_and(|entrada| entrada.dentro.remove(&pessoa).is_some())
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
    pub fn saiu(&mut self, pessoa: PersonId, conexao: u64) -> Vec<Encerramento> {
        self.conexoes.remove(&conexao);
        let acabaram: Vec<Chave> = self
            .servidos
            .iter()
            .filter(|(_, e)| e.maquina == pessoa && e.conexao == conexao)
            .map(|(k, _)| k.clone())
            .collect();
        let mut fim = Vec::new();
        for k in acabaram {
            if let Some(entrada) = self.servidos.remove(&k) {
                fim.extend(Self::encerrar_para_quem_joga(
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
    pub fn sem_permissao(&mut self, pessoa: PersonId, mod_id: &str) -> Vec<Encerramento> {
        let mut fim = Vec::new();
        for (k, entrada) in &mut self.servidos {
            if k.0 == mod_id && entrada.dentro.remove(&pessoa).is_some() {
                fim.push(Encerramento {
                    para: pessoa,
                    mod_id: k.0.clone(),
                    nome: k.1.clone(),
                    motivo: MotivoDoFimDoTunel::SemPermissao,
                });
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
        let contador = self.contadores.get(&k).cloned().unwrap_or_default();
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

    /// Conta um canal cortado por falta de concessão.
    pub fn contar_sem_concessao(&self) {
        self.sem_concessao.fetch_add(1, Ordering::Relaxed);
    }

    /// Quantos canais foram cortados por falta de concessão.
    #[must_use]
    pub fn canais_sem_concessao(&self) -> u64 {
        self.sem_concessao.load(Ordering::Relaxed)
    }
}

/// `t` leva TCP (e, num túnel UDP, o canal de sinais); `j` leva UDP.
fn tipo_combina(
    canal_ordenado: bool,
    tipo: TipoDeCanalDeTunel,
    protocolo: ProtocoloDeTunel,
) -> bool {
    matches!(
        (canal_ordenado, tipo, protocolo),
        (true, TipoDeCanalDeTunel::Tcp, ProtocoloDeTunel::Tcp)
            | (true, TipoDeCanalDeTunel::Controle, ProtocoloDeTunel::Udp)
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

    /// Quantos canais de túnel chegaram sem concessão e foram cortados.
    #[must_use]
    pub fn canais_sem_concessao(&self) -> u64 {
        self.tuneis().canais_sem_concessao()
    }
}

// ---- o que depende do banco e do disco ----

/// Os poderes que o manifesto do MOD **ligado** declara; vazio para um MOD que
/// não está ligado ou cujo pacote não abre.
async fn poderes_do_mod(server: &Server, mod_id: &str) -> Vec<Poder> {
    let Some(raizes) = &server.mods_dir else {
        return Vec::new();
    };
    let hash = {
        let banco = server.persistence.lock().await;
        crate::persistence::mods::enabled(&banco)
            .ok()
            .and_then(|ligados| ligados.into_iter().find(|m| m.id == mod_id))
            .map(|m| m.hash)
    };
    let Some(hash) = hash else {
        return Vec::new();
    };
    // O mesmo `mod.json` da pasta do hash que as permissões declaradas leem.
    let Ok(texto) = tokio::fs::read_to_string(raizes.pacote_de(&hash).join("mod.json")).await
    else {
        return Vec::new();
    };
    seele_proto::mods::read_manifest(&texto)
        .map(|m| m.poderes)
        .unwrap_or_default()
}

/// A máquina de uma sessão passa a servir um túnel.
///
/// # Errors
///
/// A recusa nomeada de [`Tuneis::servir`].
pub async fn atender_servir(
    server: &Server,
    maquina: PersonId,
    id_da_conexao: u64,
    conexao: &Conexao,
    pedido: PedidoDeTunel<'_>,
) -> Result<(), RecusaDeTunel> {
    // O que vem de fora antes do cadeado do livro.
    let poderes = poderes_do_mod(server, pedido.mod_id).await;
    let mut livro = server.tuneis();
    livro.servir(maquina, id_da_conexao, pedido, &poderes)?;
    livro.registrar_conexao(id_da_conexao, conexao.clone());
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
        let _ = server.tuneis().sem_permissao(pessoa, mod_id);
        return Err(recusa);
    }
    Ok(servido)
}

/// Diz a quem precisa saber que túneis acabaram: no log, com `mod_id=` e
/// `tunel=`, e pelo barramento, endereçado.
pub fn anunciar(server: &Server, fim: Vec<Encerramento>) {
    for e in fim {
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
    let mut fim = Vec::new();
    {
        let mut livro = server.tuneis();
        for (pessoa, mod_id) in perderam {
            fim.extend(livro.sem_permissao(pessoa, &mod_id));
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
/// pacote por um que não declara a porta) fecha os túneis dele, e uma mudança
/// no conjunto confere de novo quem está dentro.
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
) -> Result<(), RecusaDeTunel> {
    match canal {
        CanalDeTunel::Ordenado(envio, recebe) => repassar_t(server, de, envio, recebe).await,
        CanalDeTunel::SemOrdem(datagramas) => repassar_j(server, de, datagramas).await,
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
    let rota = server.tuneis().rota(de, &abertura, true);
    let rota = match rota {
        Ok(rota) => rota,
        Err(por_que) => {
            cortar_t(&mut envio, &mut recebe);
            return Err(corta_e_conta(server, de, &abertura, por_que));
        }
    };
    let mut vigia = rota.vigia;
    let aberto = rota.conexao.abrir_tunel(Confiabilidade::Ordenada).await;
    let (mut m_envio, mut m_recebe) = match aberto {
        Ok(CanalDeTunel::Ordenado(e, r)) => (e, r),
        Ok(CanalDeTunel::SemOrdem(_)) => {
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

async fn repassar_j(
    server: &Server,
    de: PersonId,
    mut datagramas: seele_enlace::Datagramas,
) -> Result<(), RecusaDeTunel> {
    let primeiro = tokio::time::timeout(PRAZO_DA_ABERTURA, datagramas.receber()).await;
    let abertura = match primeiro {
        Ok(Some(bytes)) => seele_proto::control::decode::<AberturaDeTunel>(&bytes).ok(),
        _ => None,
    };
    let Some(abertura) = abertura else {
        server.tuneis().contar_sem_concessao();
        tracing::warn!(pessoa = %de, "túnel: canal j sem abertura legível, cortado");
        return Err(RecusaDeTunel::SemPermissao);
    };
    let rota = server.tuneis().rota(de, &abertura, false);
    let rota = match rota {
        Ok(rota) => rota,
        Err(por_que) => return Err(corta_e_conta(server, de, &abertura, por_que)),
    };
    let mut vigia = rota.vigia;
    let mut da_maquina = match rota.conexao.abrir_tunel(Confiabilidade::SemOrdem).await {
        Ok(CanalDeTunel::SemOrdem(d)) => d,
        Ok(CanalDeTunel::Ordenado(..)) => return Err(RecusaDeTunel::MaquinaSaiu),
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
    let quadro =
        seele_proto::control::encode(&reescrita).map_err(|_| RecusaDeTunel::MaquinaSaiu)?;
    if da_maquina.mandar(&quadro).is_err() {
        return Err(RecusaDeTunel::MaquinaSaiu);
    }
    let antes = rota.contador.load(Ordering::Relaxed);
    let mut descartados = 0_u64;
    let fim = loop {
        tokio::select! {
            recebido = datagramas.receber() => {
                let Some(bytes) = recebido else { break "quem joga fechou o canal" };
                match da_maquina.mandar(&bytes) {
                    Ok(()) => { rota.contador.fetch_add(bytes.len() as u64, Ordering::Relaxed); }
                    Err(Recusa::Fechada) => break "a máquina fechou o canal",
                    Err(_) => descartados += 1,
                }
            }
            recebido = da_maquina.receber() => {
                let Some(bytes) = recebido else { break "a máquina fechou o canal" };
                match datagramas.mandar(&bytes) {
                    Ok(()) => { rota.contador.fetch_add(bytes.len() as u64, Ordering::Relaxed); }
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
        assert!(t.saiu(MAQUINA, 99).is_empty());
        let fim = t.saiu(MAQUINA, 7);
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

    #[test]
    fn a_pessoa_que_sai_ou_perde_a_permissao_some_das_concessoes() {
        let mut t = com_tunel();
        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        assert_eq!(t.mods_em_que_esta(JOGA), vec!["x/jogo".to_owned()]);
        let fim = t.sem_permissao(JOGA, "x/jogo");
        assert_eq!(fim.len(), 1);
        assert_eq!(fim[0].para, JOGA);
        assert_eq!(fim[0].motivo, MotivoDoFimDoTunel::SemPermissao);
        assert!(t.mods_em_que_esta(JOGA).is_empty());

        t.entrar(JOGA, 3, "x/jogo", "eco").expect("entra");
        // A conexão velha saindo não tira quem já voltou por outra.
        t.entrar(JOGA, 4, "x/jogo", "eco").expect("volta");
        t.saiu(JOGA, 3);
        assert_eq!(t.mods_em_que_esta(JOGA).len(), 1);
        t.saiu(JOGA, 4);
        assert!(t.mods_em_que_esta(JOGA).is_empty());
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
}
