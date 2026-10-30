//! O livro das máquinas de jogo do servidor (protocolo 12, API 8 parte 3):
//! quem foi chamado a rodar a máquina de cada servidor de jogo, quem aceitou, e
//! o que a máquina aceita pode servir.
//!
//! # Por que um livro, e não uma pergunta ao cliente
//!
//! Até a parte 2, servir um túnel dependia só do MOD: qualquer sessão no 12
//! podia servir o nome de túnel de qualquer MOD com o poder `porta`
//! (`ServerConfig::servir_tuneis`). O aceite muda a pergunta de «o MOD pode?»
//! para «**esta pessoa**, por **esta conexão**, aceitou **esta oferta**?» — e
//! essa resposta só o servidor tem, porque foi ele quem mostrou a oferta.
//!
//! # O que este módulo não decide
//!
//! Quem pede uma máquina é a metade de servidor do MOD (Task 5). Aqui só se
//! guarda o pendente, se confere o aceite contra o que foi mostrado, e se
//! responde se um túnel estava na oferta aceita. O cadeado é um
//! `std::sync::Mutex` que nunca atravessa um `await`; a ordem dele com o de
//! [`crate::tuneis::Tuneis`] e com o banco está em [`Server::maquinas`].

use std::collections::HashMap;

use seele_proto::ids::PersonId;
use seele_proto::maquina::{
    MotivoDaDispensa, MotivoDaRecusaDeMaquina, OfertaDeMaquina, TunelOferecido,
};
use seele_proto::tunel::ProtocoloDeTunel;

use crate::server::{Event, Server};

/// Quantos pedidos sem resposta um MOD pode ter de uma vez.
pub const MAX_PEDIDOS_PENDENTES_POR_MOD: usize = 16;

/// Quantas máquinas aceitas um MOD pode ter: o mesmo teto dos túneis
/// ([`crate::tuneis::MAX_TUNEIS_POR_MOD`]), porque uma máquina que não pode
/// servir túnel nenhum não tem para que existir.
pub const MAX_MAQUINAS_POR_MOD: usize = crate::tuneis::MAX_TUNEIS_POR_MOD;

type Chave = (String, String);

fn chave(mod_id: &str, servidor: &str) -> Chave {
    (mod_id.to_owned(), servidor.to_owned())
}

/// Um pedido mostrado e ainda sem resposta.
struct Pendente {
    quem: PersonId,
    pedido_por: PersonId,
    hash: String,
    resumo_hex: String,
    /// Inteira, e não só os túneis: uma conexão de `quem` que chega com o
    /// pedido pendente o recebe também ([`Maquinas::chegou`]).
    oferta: OfertaDeMaquina,
    /// As conexões de `quem` a que o pedido foi mostrado e que ainda não
    /// recusaram **sozinhas**. Ver [`Maquinas::recusar`].
    esperando: Vec<u64>,
    /// O motivo da última recusa automática, que é o que o MOD lê se todas as
    /// conexões recusarem sozinhas.
    ultima_automatica: Option<MotivoDaRecusaDeMaquina>,
}

/// O que [`Maquinas::recusar`] fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recusa {
    /// Não havia pedido pendente desta pessoa para esse servidor de jogo.
    SemPedido,
    /// Esta conexão recusou sozinha, e outra ainda pode aceitar: o pedido
    /// segue pendente.
    Esperando,
    /// O pedido saiu do livro, com este motivo.
    Recusado(MotivoDaRecusaDeMaquina),
}

/// Um pedido pendente reenviado a uma conexão que chegou depois dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reenvio {
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O servidor de jogo.
    pub servidor: String,
    /// O hash do pacote do pedido.
    pub hash: String,
    /// Quem pediu.
    pub pedido_por: PersonId,
    /// O que foi mostrado.
    pub oferta: OfertaDeMaquina,
}

/// Se a recusa é a que o cliente dá sozinho, sem a pessoa ver a oferta: o
/// núcleo sem tela (`SemTela`) e o navegador (`SoNoApp`) respondem na hora, e
/// a mesma pessoa pode estar com o app aberto ao lado.
const fn automatica(motivo: MotivoDaRecusaDeMaquina) -> bool {
    matches!(
        motivo,
        MotivoDaRecusaDeMaquina::SemTela | MotivoDaRecusaDeMaquina::SoNoApp
    )
}

/// Uma máquina aceita.
struct Aceitada {
    quem: PersonId,
    /// A conexão que aceitou. A sessão nova da mesma pessoa **não herda** o
    /// aceite: ela não viu a oferta, e a conexão velha pode ainda estar viva
    /// numa corrida de candidatos.
    conexao: u64,
    hash: String,
    resumo_hex: String,
    tuneis: Vec<TunelOferecido>,
}

/// Por que um aceite não valeu. Vai para o log, e o aceite inválido não muda
/// nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoDeAceiteInvalido {
    /// Não há pedido pendente para esse servidor de jogo.
    SemPedido,
    /// O pedido pendente é de outra pessoa.
    OutraPessoa,
    /// O resumo não é o do que foi mostrado.
    OutroResumo,
    /// O MOD já tem [`MAX_MAQUINAS_POR_MOD`] máquinas aceitas.
    MaquinasDemais,
}

/// Por que um pedido não foi guardado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Excesso {
    /// O MOD já tem [`MAX_PEDIDOS_PENDENTES_POR_MOD`] pedidos sem resposta.
    PendentesDemais,
    /// O MOD já tem [`MAX_MAQUINAS_POR_MOD`] máquinas aceitas.
    MaquinasDemais,
}

/// Uma máquina aceita que o servidor dispensou, para quem precisa saber.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispensa {
    /// Quem operava.
    pub quem: PersonId,
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O servidor de jogo.
    pub servidor: String,
    /// Por quê.
    pub motivo: MotivoDaDispensa,
    /// Os túneis que ela servia por este aceite e que param com ela.
    pub a_parar: Vec<String>,
}

/// O que [`Maquinas::pedir`] fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pedido {
    /// O pedido ficou pendente: o `PedidoDeMaquina` vai a `quem`. Se havia
    /// outra pessoa operando esse servidor de jogo, ela é dispensada.
    Guardado {
        /// O resumo que o aceite tem de trazer.
        resumo_hex: String,
        /// A máquina de outra pessoa que deixa de valer.
        dispensada: Option<Dispensa>,
        /// Outra pessoa que tinha sido chamada a este servidor de jogo e ainda
        /// não respondeu: o pedido dela sumiu, e a tela de aceite dela tem de
        /// fechar (`MaquinaDispensada { OutroPedido }`).
        pendente_dispensado: Option<PersonId>,
    },
    /// A mesma pessoa já aceitou exatamente isso: nada muda, nada sai.
    JaAceito,
    /// Um teto: nada muda.
    Recusado(Excesso),
}

/// Uma máquina que entrou no livro ou saiu dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aceita {
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O servidor de jogo.
    pub servidor: String,
    /// Quem opera.
    pub quem: PersonId,
    /// Os túneis que deixam de valer: na parada, todos os desta máquina; no
    /// aceite, os do aceite anterior da mesma pessoa que a oferta nova não
    /// traz mais.
    pub a_parar: Vec<String>,
}

/// Por que um túnel ficou fora do aceite: as três coisas que podem não bater,
/// para o log dizer qual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunelForaDoAceite {
    /// Quem pede não é máquina aceita de servidor de jogo nenhum deste MOD (ou
    /// é, mas por outra conexão).
    SemMaquinaAceita,
    /// É máquina aceita, mas nenhuma oferta aceita traz este nome de túnel.
    ForaDaOferta,
    /// O nome estava na oferta, mas com outra porta ou outro protocolo.
    OutraPortaOuProtocolo,
}

/// Uma mudança no livro, no barramento, para os momentos da metade de servidor
/// (Task 5). `motivo` é o nome em kebab-case, e não o enum, porque `Parou`
/// junta motivos de três lugares (os do cliente, `saiu`, `mod-desligado`,
/// `outro-pedido`) e `Recusou` ganha `nao-esta-aqui` do servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MudancaDeMaquina {
    /// A pessoa aceitou.
    Aceitou {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// Quem aceitou.
        quem: PersonId,
    },
    /// A pessoa recusou.
    Recusou {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// Quem recusou.
        quem: PersonId,
        /// O nome do motivo.
        motivo: &'static str,
    },
    /// A máquina parou ou foi dispensada.
    Parou {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// Quem operava.
        quem: PersonId,
        /// O nome do motivo.
        motivo: &'static str,
    },
}

/// O que [`Maquinas::mod_desligado`] tirou do livro.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Desligado {
    /// `(quem, servidor)` de cada máquina aceita que operava.
    pub maquinas: Vec<(PersonId, String)>,
    /// `(quem, servidor)` de cada pedido que esperava resposta: a tela de aceite
    /// de quem foi chamado tem de fechar.
    pub pendentes: Vec<(PersonId, String)>,
}

/// O livro das máquinas.
#[derive(Default)]
pub struct Maquinas {
    pendentes: HashMap<Chave, Pendente>,
    aceitas: HashMap<Chave, Aceitada>,
    /// As conexões de cada pessoa que negociaram o 12 e ainda estão abertas:
    /// é a quem um `PedidoDeMaquina` chega. Um pedido a quem não está aqui
    /// seria um pendente que ninguém vê, e a metade de servidor esperaria uma
    /// resposta que não vem.
    no_doze: HashMap<PersonId, Vec<u64>>,
    /// Quantos avisos de cada MOD não saíram por falta de máquina aceita.
    sem_maquina: HashMap<String, u64>,
    /// Quando cada recusa do servidor `(mod, servidor, quem, motivo)` foi dita
    /// ao MOD por último: é o que corta o laço de um momento que pede de novo
    /// (`mods::maquina::JANELA_DA_RECUSA_REPETIDA`).
    recusas_ditas: HashMap<(String, String, PersonId, &'static str), std::time::Instant>,
    /// Quantas recusas de cada MOD não voltaram como momento por isso.
    recusas_juntadas: HashMap<String, u64>,
}

impl Maquinas {
    /// O servidor pede a `quem` que rode a máquina de `servidor`, mostrando
    /// `oferta`.
    ///
    /// Um pedido novo para o mesmo servidor de jogo **substitui** o pendente: a
    /// metade de servidor mudou de ideia, e o aceite do pedido velho não pode
    /// valer pelo novo (o resumo já impede, e substituir tira o lixo). Se outra
    /// pessoa operava esse servidor de jogo, ela sai agora, com
    /// [`MotivoDaDispensa::OutroPedido`]: duas máquinas para um servidor de jogo
    /// seriam dois donos do mesmo nome de túnel.
    ///
    /// A mesma pessoa chamada de novo com outra oferta **continua** operando a
    /// aceita até aceitar a nova: o que ela aceitou antes segue sendo o que ela
    /// aceitou, e ninguém a dispensa por mudar o que se lhe pede.
    pub fn pedir(
        &mut self,
        mod_id: &str,
        hash: &str,
        servidor: &str,
        quem: PersonId,
        pedido_por: PersonId,
        oferta: &OfertaDeMaquina,
    ) -> Pedido {
        let k = chave(mod_id, servidor);
        let resumo_hex = seele_proto::maquina::resumo_em_hex(
            &seele_proto::maquina::resumo_da_oferta(mod_id, hash, servidor, oferta),
        );
        if self
            .aceitas
            .get(&k)
            .is_some_and(|a| a.quem == quem && a.resumo_hex == resumo_hex)
        {
            return Pedido::JaAceito;
        }
        if !self.pendentes.contains_key(&k)
            && self.pendentes.keys().filter(|(m, _)| m == mod_id).count()
                >= MAX_PEDIDOS_PENDENTES_POR_MOD
        {
            return Pedido::Recusado(Excesso::PendentesDemais);
        }
        if !self.aceitas.contains_key(&k) && self.aceitas_do_mod(mod_id) >= MAX_MAQUINAS_POR_MOD {
            return Pedido::Recusado(Excesso::MaquinasDemais);
        }
        let pendente_dispensado = self
            .pendentes
            .get(&k)
            .map(|p| p.quem)
            .filter(|chamada| *chamada != quem);
        let dispensada = match self.aceitas.get(&k) {
            Some(a) if a.quem != quem => self.aceitas.remove(&k).map(|velha| {
                let a_parar = self.so_desta(&velha, mod_id);
                Dispensa {
                    quem: velha.quem,
                    mod_id: mod_id.to_owned(),
                    servidor: servidor.to_owned(),
                    motivo: MotivoDaDispensa::OutroPedido,
                    a_parar,
                }
            }),
            _ => None,
        };
        self.pendentes.insert(
            k,
            Pendente {
                quem,
                pedido_por,
                hash: hash.to_owned(),
                resumo_hex: resumo_hex.clone(),
                oferta: oferta.clone(),
                esperando: self.no_doze.get(&quem).cloned().unwrap_or_default(),
                ultima_automatica: None,
            },
        );
        Pedido::Guardado {
            resumo_hex,
            dispensada,
            pendente_dispensado,
        }
    }

    fn aceitas_do_mod(&self, mod_id: &str) -> usize {
        self.aceitas.keys().filter(|(m, _)| m == mod_id).count()
    }

    /// Os nomes de túnel de `saindo` que nenhuma outra máquina aceita da mesma
    /// pessoa e conexão, no mesmo MOD, ainda oferece **igual** — nome,
    /// protocolo e porta. `saindo` já tem de ter saído do livro (ou sido
    /// trocado): parar um túnel que outra oferta aceita ainda cobre derrubaria
    /// um túnel legítimo, e deixar de pé um cujo nome sobrou mas a porta mudou
    /// deixaria servindo o que ninguém aceitou.
    fn so_desta(&self, saindo: &Aceitada, mod_id: &str) -> Vec<String> {
        saindo
            .tuneis
            .iter()
            .filter(|t| {
                !self.aceitas.iter().any(|((m, _), a)| {
                    m == mod_id
                        && a.quem == saindo.quem
                        && a.conexao == saindo.conexao
                        && a.tuneis.contains(t)
                })
            })
            .map(|t| t.nome.clone())
            .collect()
    }

    /// A pessoa aceitou o que lhe foi mostrado.
    ///
    /// Vale só se há pedido pendente para `(mod_id, servidor)`, chamado a
    /// `quem`, com este resumo. Aceito, o pendente sai e a máquina entra,
    /// presa a `conexao`.
    ///
    /// # Errors
    ///
    /// O motivo de o aceite não valer; **nada muda** no livro.
    pub fn aceitar(
        &mut self,
        quem: PersonId,
        conexao: u64,
        mod_id: &str,
        servidor: &str,
        resumo_hex: &str,
    ) -> Result<Aceita, MotivoDeAceiteInvalido> {
        let k = chave(mod_id, servidor);
        let pendente = self
            .pendentes
            .get(&k)
            .ok_or(MotivoDeAceiteInvalido::SemPedido)?;
        if pendente.quem != quem {
            return Err(MotivoDeAceiteInvalido::OutraPessoa);
        }
        if pendente.resumo_hex != resumo_hex {
            return Err(MotivoDeAceiteInvalido::OutroResumo);
        }
        // O teto também aqui: entre o pedido e o aceite outros pendentes do
        // mesmo MOD podem ter virado máquina.
        if !self.aceitas.contains_key(&k) && self.aceitas_do_mod(mod_id) >= MAX_MAQUINAS_POR_MOD {
            return Err(MotivoDeAceiteInvalido::MaquinasDemais);
        }
        let Some(pendente) = self.pendentes.remove(&k) else {
            return Err(MotivoDeAceiteInvalido::SemPedido);
        };
        let nova = Aceitada {
            quem,
            conexao,
            hash: pendente.hash,
            resumo_hex: pendente.resumo_hex,
            tuneis: pendente.oferta.tuneis,
        };
        // O aceite anterior da mesma pessoa (outra oferta, ou outra conexão):
        // o que ele cobria e a oferta nova não cobre para de valer.
        let a_parar = match self.aceitas.insert(k, nova) {
            Some(velha) => self.so_desta(&velha, mod_id),
            None => Vec::new(),
        };
        tracing::debug!(mod_id = %mod_id, servidor = %servidor, quem = %quem, pedido_por = %pendente.pedido_por, "máquina: aceite registrado no livro");
        Ok(Aceita {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            quem,
            a_parar,
        })
    }

    /// A pessoa recusou o pedido, pela conexão `conexao`.
    ///
    /// **Uma recusa automática é da conexão, e não da pessoa.** O pedido vai a
    /// toda sessão dela no 12; o navegador responde `SoNoApp` e o núcleo sem
    /// tela `SemTela` na hora, antes de ela ver a oferta no app aberto ao lado.
    /// Derrubar o pedido por isso seria o MOD ler «recusou» e o aceite do app
    /// cair em `SemPedido`. Então: a recusa automática só tira esta conexão das
    /// que esperam, e o pedido cai (com o último motivo automático) quando
    /// nenhuma sobra. Uma recusa da pessoa (`PeloDono`) ou do pacote
    /// (`PacoteDiferente`, `OfertaInvalida`) derruba na hora.
    pub fn recusar(
        &mut self,
        quem: PersonId,
        conexao: u64,
        mod_id: &str,
        servidor: &str,
        motivo: MotivoDaRecusaDeMaquina,
    ) -> Recusa {
        let k = chave(mod_id, servidor);
        let Some(pendente) = self.pendentes.get_mut(&k).filter(|p| p.quem == quem) else {
            return Recusa::SemPedido;
        };
        if automatica(motivo) {
            pendente.esperando.retain(|c| *c != conexao);
            pendente.ultima_automatica = Some(motivo);
            if !pendente.esperando.is_empty() {
                return Recusa::Esperando;
            }
        }
        self.pendentes.remove(&k);
        Recusa::Recusado(motivo)
    }

    /// A máquina parou. Só a própria, pela conexão que aceitou; devolve o que
    /// saiu do livro, com os túneis a parar.
    pub fn parar(
        &mut self,
        quem: PersonId,
        conexao: u64,
        mod_id: &str,
        servidor: &str,
    ) -> Option<Aceita> {
        let k = chave(mod_id, servidor);
        if !self
            .aceitas
            .get(&k)
            .is_some_and(|a| a.quem == quem && a.conexao == conexao)
        {
            return None;
        }
        let velha = self.aceitas.remove(&k)?;
        Some(Aceita {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            quem,
            a_parar: self.so_desta(&velha, mod_id),
        })
    }

    /// Uma conexão acabou: as máquinas que **ela** aceitou saem do livro, e
    /// volta `(mod_id, servidor)` de cada uma. Os túneis dela não são daqui:
    /// `Tuneis::saiu` os encerra com `MaquinaSaiu`.
    ///
    /// Os pendentes chamados a esta pessoa ficam: o pedido é à pessoa, e outra
    /// sessão dela pode responder.
    pub fn saiu(&mut self, quem: PersonId, conexao: u64) -> Vec<(String, String)> {
        for pendente in self.pendentes.values_mut().filter(|p| p.quem == quem) {
            pendente.esperando.retain(|c| *c != conexao);
        }
        if let Some(conexoes) = self.no_doze.get_mut(&quem) {
            conexoes.retain(|c| *c != conexao);
            if conexoes.is_empty() {
                self.no_doze.remove(&quem);
            }
        }
        let sairam: Vec<Chave> = self
            .aceitas
            .iter()
            .filter(|(_, a)| a.quem == quem && a.conexao == conexao)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &sairam {
            self.aceitas.remove(k);
        }
        sairam
    }

    /// O MOD foi desligado (ou trocou de pacote): todo pendente e toda máquina
    /// dele saem. Volta `(quem, servidor)` de cada máquina que operava e de cada
    /// pedido que esperava resposta, para a dispensa.
    pub fn mod_desligado(&mut self, mod_id: &str) -> Desligado {
        let mut desligado = Desligado::default();
        self.pendentes.retain(|(m, servidor), p| {
            if m == mod_id {
                desligado.pendentes.push((p.quem, servidor.clone()));
                false
            } else {
                true
            }
        });
        self.aceitas.retain(|(m, servidor), a| {
            if m == mod_id {
                desligado.maquinas.push((a.quem, servidor.clone()));
                false
            } else {
                true
            }
        });
        // Quem operava e tinha um pedido novo pendente para o mesmo servidor
        // de jogo recebe uma dispensa só, a da máquina.
        desligado
            .pendentes
            .retain(|p| !desligado.maquinas.contains(p));
        desligado
    }

    /// Uma conexão de `quem` negociou o 12: daqui até [`Self::saiu`] dela, um
    /// pedido de máquina a `quem` tem a quem chegar.
    ///
    /// Os pedidos que já esperam `quem` passam a esperar esta conexão também,
    /// e voltam para serem mostrados a ela: a pessoa que abre o app depois do
    /// pedido o vê ali, e não só na aba que recusou sozinha.
    pub fn chegou(&mut self, quem: PersonId, conexao: u64) -> Vec<Reenvio> {
        let conexoes = self.no_doze.entry(quem).or_default();
        if !conexoes.contains(&conexao) {
            conexoes.push(conexao);
        }
        let mut reenvios = Vec::new();
        for ((mod_id, servidor), pendente) in &mut self.pendentes {
            if pendente.quem != quem || pendente.esperando.contains(&conexao) {
                continue;
            }
            pendente.esperando.push(conexao);
            reenvios.push(Reenvio {
                mod_id: mod_id.clone(),
                servidor: servidor.clone(),
                hash: pendente.hash.clone(),
                pedido_por: pendente.pedido_por,
                oferta: pendente.oferta.clone(),
            });
        }
        reenvios
    }

    /// Os pedidos a `quem` que todas as conexões que sobraram já recusaram
    /// sozinhas: saem do livro, com o último motivo automático. É o caso da
    /// pessoa que fecha o app com a aba do navegador (que recusou `SoNoApp`)
    /// ainda aberta — ninguém mais vai ver a oferta.
    pub fn sem_quem_veja(
        &mut self,
        quem: PersonId,
    ) -> Vec<(String, String, MotivoDaRecusaDeMaquina)> {
        let mut caidos = Vec::new();
        self.pendentes
            .retain(|(mod_id, servidor), p| match p.ultima_automatica {
                Some(motivo) if p.quem == quem && p.esperando.is_empty() => {
                    caidos.push((mod_id.clone(), servidor.clone(), motivo));
                    false
                }
                _ => true,
            });
        caidos
    }

    /// Se `quem` tem alguma conexão aberta no 12 ([`Self::chegou`]).
    #[must_use]
    pub fn pode_ser_chamada(&self, quem: PersonId) -> bool {
        self.no_doze.contains_key(&quem)
    }

    /// Conta um aviso de `mod_id` que não saiu por falta de máquina aceita, e
    /// devolve quantos já são.
    pub fn contar_aviso_sem_maquina(&mut self, mod_id: &str) -> u64 {
        let n = self.sem_maquina.entry(mod_id.to_owned()).or_default();
        *n += 1;
        *n
    }

    /// Marca que a recusa `(mod_id, servidor, quem, motivo)` foi dita agora, e
    /// diz se ela é **nova** — não dita nos últimos `janela`. As velhas saem
    /// aqui mesmo: o mapa não cresce com o tempo.
    pub fn marcar_recusa(
        &mut self,
        mod_id: &str,
        servidor: &str,
        quem: PersonId,
        motivo: &'static str,
        agora: std::time::Instant,
        janela: std::time::Duration,
    ) -> bool {
        self.recusas_ditas
            .retain(|_, quando| agora.saturating_duration_since(*quando) < janela);
        let chave = (mod_id.to_owned(), servidor.to_owned(), quem, motivo);
        if self.recusas_ditas.contains_key(&chave) {
            return false;
        }
        self.recusas_ditas.insert(chave, agora);
        true
    }

    /// Conta uma recusa de `mod_id` que não voltou como momento, e devolve
    /// quantas já são.
    pub fn contar_recusa_juntada(&mut self, mod_id: &str) -> u64 {
        let n = self.recusas_juntadas.entry(mod_id.to_owned()).or_default();
        *n += 1;
        *n
    }

    /// Quantas recusas de `mod_id` não voltaram como momento.
    #[must_use]
    pub fn recusas_juntadas(&self, mod_id: &str) -> u64 {
        self.recusas_juntadas.get(mod_id).copied().unwrap_or(0)
    }

    /// Quem opera a máquina aceita de `(mod_id, servidor)`, e por qual conexão.
    #[must_use]
    pub fn operador(&self, mod_id: &str, servidor: &str) -> Option<(PersonId, u64)> {
        self.aceitas
            .get(&chave(mod_id, servidor))
            .map(|a| (a.quem, a.conexao))
    }

    /// Quantos avisos de `mod_id` não saíram por falta de máquina aceita.
    #[must_use]
    pub fn avisos_sem_maquina(&self, mod_id: &str) -> u64 {
        self.sem_maquina.get(mod_id).copied().unwrap_or(0)
    }

    /// Por que `quem`/`conexao` não pode servir este túnel, ou `Ok` se pode:
    /// é máquina aceita de algum servidor de jogo do MOD cuja oferta traz um
    /// [`TunelOferecido`] igual — nome, protocolo e porta.
    ///
    /// # Errors
    ///
    /// Qual das três coisas não bateu.
    pub fn conferir_tunel(
        &self,
        quem: PersonId,
        conexao: u64,
        mod_id: &str,
        nome: &str,
        protocolo: ProtocoloDeTunel,
        porta: u16,
    ) -> Result<(), TunelForaDoAceite> {
        let mut dela = self
            .aceitas
            .iter()
            .filter(|((m, _), a)| m == mod_id && a.quem == quem && a.conexao == conexao)
            .peekable();
        if dela.peek().is_none() {
            return Err(TunelForaDoAceite::SemMaquinaAceita);
        }
        let mut achou_o_nome = false;
        for (_, a) in dela {
            for t in a.tuneis.iter().filter(|t| t.nome == nome) {
                achou_o_nome = true;
                if t.protocolo == protocolo && t.porta == porta {
                    return Ok(());
                }
            }
        }
        Err(if achou_o_nome {
            TunelForaDoAceite::OutraPortaOuProtocolo
        } else {
            TunelForaDoAceite::ForaDaOferta
        })
    }

    /// Se `quem`/`conexao` pode servir este túnel ([`Self::conferir_tunel`]).
    #[must_use]
    pub fn tunel_permitido(
        &self,
        quem: PersonId,
        conexao: u64,
        mod_id: &str,
        nome: &str,
        protocolo: ProtocoloDeTunel,
        porta: u16,
    ) -> bool {
        self.conferir_tunel(quem, conexao, mod_id, nome, protocolo, porta)
            .is_ok()
    }

    /// Se `quem`, por `conexao`, é a máquina aceita de `(mod_id, servidor)`.
    #[must_use]
    pub fn maquina_de(&self, quem: PersonId, conexao: u64, mod_id: &str, servidor: &str) -> bool {
        self.aceitas
            .get(&chave(mod_id, servidor))
            .is_some_and(|a| a.quem == quem && a.conexao == conexao)
    }

    /// Quem opera a máquina aceita de `(mod_id, servidor)`, se alguém.
    #[must_use]
    pub fn quem_opera(&self, mod_id: &str, servidor: &str) -> Option<PersonId> {
        self.aceitas.get(&chave(mod_id, servidor)).map(|a| a.quem)
    }

    /// Cada `(mod_id, hash)` que o livro guarda, pendente ou aceito: o que a
    /// vigia do conjunto de MODs confere contra o pacote ligado.
    #[must_use]
    pub fn hashes(&self) -> Vec<(String, String)> {
        let mut todos: Vec<(String, String)> = self
            .pendentes
            .iter()
            .map(|((m, _), p)| (m.clone(), p.hash.clone()))
            .chain(
                self.aceitas
                    .iter()
                    .map(|((m, _), a)| (m.clone(), a.hash.clone())),
            )
            .collect();
        todos.sort();
        todos.dedup();
        todos
    }
}

// ---- o que fala com o servidor ----

impl Server {
    /// O livro das máquinas, sem a poluição de um cadeado envenenado (a mesma
    /// razão de [`Server::tuneis`]).
    ///
    /// # A ordem dos cadeados
    ///
    /// 1. O banco (`persistence`) é lido **antes**, e solto: os dois livros são
    ///    `std::sync::Mutex` e não podem esperar por ele.
    /// 2. `maquinas()` **antes** de `tuneis()` sempre que os dois forem tomados
    ///    juntos (`tuneis::atender_servir` segura os dois para que uma parada
    ///    não ache meio túnel). Nunca `maquinas()` com `tuneis()` já tomado.
    ///
    /// Dois cadeados tomados sempre na mesma ordem são uma ordem; em ordens
    /// diferentes, um travamento esperando o primeiro dia ruim.
    pub fn maquinas(&self) -> std::sync::MutexGuard<'_, Maquinas> {
        self.maquinas
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn publicar(server: &Server, mudanca: MudancaDeMaquina) {
    let _ = server.events.send(Event::Maquina(mudanca));
}

/// O servidor pede uma máquina: guarda o pendente, manda o `PedidoDeMaquina` a
/// `quem` e dispensa quem operava antes, se era outra pessoa.
///
/// É a porta da metade de servidor (Task 5) e das provas: o livro sozinho não
/// fala com ninguém.
pub fn pedir(
    server: &Server,
    mod_id: &str,
    hash: &str,
    servidor: &str,
    quem: PersonId,
    pedido_por: PersonId,
    oferta: &OfertaDeMaquina,
) -> Pedido {
    let pedido = server
        .maquinas()
        .pedir(mod_id, hash, servidor, quem, pedido_por, oferta);
    match &pedido {
        Pedido::Guardado {
            dispensada,
            pendente_dispensado,
            ..
        } => {
            tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, pedido_por = %pedido_por, "máquina: pedido mostrado a quem vai operá-la");
            // Quem operava e tinha um pedido novo pendente recebe uma dispensa
            // só, a da máquina.
            let ja_dispensada = dispensada.as_ref().map(|d| d.quem);
            if let Some(chamada) = pendente_dispensado.filter(|c| Some(*c) != ja_dispensada) {
                dispensar_o_pendente(
                    server,
                    chamada,
                    mod_id,
                    servidor,
                    MotivoDaDispensa::OutroPedido,
                );
            }
            if let Some(dispensa) = dispensada {
                dispensar(server, dispensa.clone());
            }
            let _ = server.events.send(Event::PedidoDeMaquina {
                para: quem,
                conexao: None,
                mod_id: mod_id.to_owned(),
                hash: hash.to_owned(),
                servidor: servidor.to_owned(),
                pedido_por,
                oferta: Box::new(oferta.clone()),
            });
        }
        Pedido::JaAceito => {
            tracing::debug!(mod_id = %mod_id, servidor = %servidor, quem = %quem, "máquina: pedido igual ao já aceito; nada muda");
        }
        Pedido::Recusado(excesso) => {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, quem = %quem, pedido_por = %pedido_por, ?excesso, "máquina: pedido recusado por um teto");
        }
    }
    pedido
}

/// Diz a quem tinha sido chamado, e ainda não respondeu, que o pedido sumiu: a
/// tela de aceite dele fecha, em vez de ficar mostrando uma oferta que, aceita,
/// cairia em `SemPedido`. Não há `Parou` para a metade de servidor: ela mesma
/// substituiu o pedido, ou foi desligada.
pub fn dispensar_o_pendente(
    server: &Server,
    quem: PersonId,
    mod_id: &str,
    servidor: &str,
    motivo: MotivoDaDispensa,
) {
    let nome = seele_proto::maquina::nome_da_dispensa(&motivo);
    tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, motivo = nome, "máquina: pedido sem resposta dispensado; quem foi chamado fica sabendo");
    let _ = server.events.send(Event::MaquinaDispensada {
        para: quem,
        mod_id: mod_id.to_owned(),
        servidor: servidor.to_owned(),
        motivo,
    });
}

/// Diz a quem operava que a máquina foi dispensada, **e só depois** para os
/// túneis dela: quem opera precisa saber por que eles caíram antes de vê-los
/// cair, ou lê `MaquinaParou` sem ter parado nada.
pub fn dispensar(server: &Server, dispensa: Dispensa) {
    let nome = seele_proto::maquina::nome_da_dispensa(&dispensa.motivo);
    tracing::info!(mod_id = %dispensa.mod_id, servidor = %dispensa.servidor, quem = %dispensa.quem, motivo = nome, "máquina: dispensada pelo servidor");
    let _ = server.events.send(Event::MaquinaDispensada {
        para: dispensa.quem,
        mod_id: dispensa.mod_id.clone(),
        servidor: dispensa.servidor.clone(),
        motivo: dispensa.motivo,
    });
    publicar(
        server,
        MudancaDeMaquina::Parou {
            mod_id: dispensa.mod_id.clone(),
            servidor: dispensa.servidor,
            quem: dispensa.quem,
            motivo: nome,
        },
    );
    parar_tuneis(server, dispensa.quem, &dispensa.mod_id, &dispensa.a_parar);
}

/// Para os túneis `nomes` que `quem` servia no MOD: `MaquinaParou` para quem
/// estava dentro. Toma `tuneis()` com `maquinas()` já solto.
pub fn parar_tuneis(server: &Server, quem: PersonId, mod_id: &str, nomes: &[String]) {
    if nomes.is_empty() {
        return;
    }
    let mut fim = Vec::new();
    {
        let mut livro = server.tuneis();
        for nome in nomes {
            fim.extend(livro.parar(quem, mod_id, nome));
        }
    }
    crate::tuneis::anunciar(server, fim);
}

/// Uma conexão de `quem` negociou o 12 ([`Maquinas::chegou`]): os pedidos que
/// já a esperavam vão a ela, e só a ela — as outras sessões já os têm.
pub fn chegou(server: &Server, quem: PersonId, conexao: u64) {
    let reenvios = server.maquinas().chegou(quem, conexao);
    for r in reenvios {
        tracing::info!(mod_id = %r.mod_id, servidor = %r.servidor, quem = %quem, conexao, "máquina: pedido pendente mostrado a uma sessão que chegou depois dele");
        let _ = server.events.send(Event::PedidoDeMaquina {
            para: quem,
            conexao: Some(conexao),
            mod_id: r.mod_id,
            hash: r.hash,
            servidor: r.servidor,
            pedido_por: r.pedido_por,
            oferta: Box::new(r.oferta),
        });
    }
}

/// Uma sessão acabou: as máquinas que a conexão dela aceitou saem do livro, e
/// a metade de servidor fica sabendo (`Parou { saiu }`). Os túneis são de
/// `Tuneis::saiu`, que os encerra com `MaquinaSaiu`.
pub fn saiu(server: &Server, quem: PersonId, conexao: u64) {
    let (sairam, caidos) = {
        let mut livro = server.maquinas();
        let sairam = livro.saiu(quem, conexao);
        (sairam, livro.sem_quem_veja(quem))
    };
    for (mod_id, servidor, motivo) in caidos {
        let nome = seele_proto::maquina::nome_da_recusa(&motivo);
        tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, motivo = nome, "máquina: a última sessão que podia ver o pedido saiu, e as outras recusaram sozinhas");
        publicar(
            server,
            MudancaDeMaquina::Recusou {
                mod_id,
                servidor,
                quem,
                motivo: nome,
            },
        );
    }
    for (mod_id, servidor) in sairam {
        tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, motivo = "saiu", "máquina: quem operava saiu");
        publicar(
            server,
            MudancaDeMaquina::Parou {
                mod_id,
                servidor,
                quem,
                motivo: "saiu",
            },
        );
    }
}

/// O hash do pacote ligado de cada MOD, lido do banco **sem** cadeado do livro
/// tomado.
async fn ligados(server: &Server) -> Option<HashMap<String, String>> {
    let banco = server.persistence.lock().await;
    match crate::persistence::mods::enabled(&banco) {
        Ok(ligados) => Some(ligados.into_iter().map(|m| (m.id, m.hash)).collect()),
        Err(erro) => {
            tracing::warn!(%erro, "máquina: não deu para ler a tabela de MODs; o livro fica como está");
            None
        }
    }
}

/// Dispensa as máquinas (e esquece os pendentes) dos MODs que não estão mais
/// ligados, ou que trocaram de pacote: o aceite foi sobre um pacote, e outro
/// pacote é outra coisa que ninguém aceitou.
///
/// Chamada pela vigia do conjunto de MODs (`tuneis::acompanhar_os_mods`)
/// **antes** de ela fechar os túneis, para a dispensa chegar antes do fim deles.
pub async fn fechar_as_que_nao_valem_mais(server: &Server) {
    let no_livro = server.maquinas().hashes();
    if no_livro.is_empty() {
        return;
    }
    let Some(ligados) = ligados(server).await else {
        return;
    };
    let mut a_desligar: Vec<String> = no_livro
        .into_iter()
        .filter(|(mod_id, hash)| ligados.get(mod_id) != Some(hash))
        .map(|(mod_id, _)| mod_id)
        .collect();
    a_desligar.dedup();
    for mod_id in a_desligar {
        let desligado = server.maquinas().mod_desligado(&mod_id);
        for (quem, servidor) in desligado.pendentes {
            dispensar_o_pendente(
                server,
                quem,
                &mod_id,
                &servidor,
                MotivoDaDispensa::ModDesligado,
            );
        }
        for (quem, servidor) in desligado.maquinas {
            dispensar(
                server,
                Dispensa {
                    quem,
                    mod_id: mod_id.clone(),
                    servidor,
                    motivo: MotivoDaDispensa::ModDesligado,
                    // Os túneis caem logo abaixo, todos de uma vez e com o
                    // motivo certo (`ModDesligado`), e não um a um como
                    // `MaquinaParou`.
                    a_parar: Vec::new(),
                },
            );
        }
        // Desligado, a vigia dos túneis o fecharia de qualquer jeito; trocado de
        // pacote com a porta ainda declarada, não — e o túnel servido pelo
        // aceite do pacote velho não pode seguir de pé.
        let fim = server.tuneis().mod_desligado(&mod_id);
        crate::tuneis::anunciar(server, fim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOD: &str = "x/jogo";
    const HASH: &str = "aa00000000000000000000000000000000000000000000000000000000000000";
    const A: PersonId = PersonId(1);
    const B: PersonId = PersonId(2);
    const GESTORA: PersonId = PersonId(9);

    fn oferta(porta: u16) -> OfertaDeMaquina {
        OfertaDeMaquina {
            titulo: "Minecraft".into(),
            receita: "minecraft-java".into(),
            versao_da_receita: "1.0.0".into(),
            versao_do_jogo: "1.21.4".into(),
            tuneis: vec![TunelOferecido {
                nome: "mc".into(),
                protocolo: ProtocoloDeTunel::Tcp,
                porta,
            }],
            artefatos: vec![],
            pack: None,
            origens: vec![],
            dominios_do_jogo: vec![],
            rodar: None,
            disco_maximo: 0,
            termos: vec![],
        }
    }

    fn resumo(servidor: &str, o: &OfertaDeMaquina) -> String {
        seele_proto::maquina::resumo_em_hex(&seele_proto::maquina::resumo_da_oferta(
            MOD, HASH, servidor, o,
        ))
    }

    #[test]
    fn so_quem_foi_chamado_aceita_e_so_o_que_foi_mostrado() {
        let mut m = Maquinas::default();
        let o = oferta(25565);
        let certo = resumo("mc-1", &o);

        // Sem pedido nenhum, nem o resumo certo vale.
        assert_eq!(
            m.aceitar(A, 10, MOD, "mc-1", &certo),
            Err(MotivoDeAceiteInvalido::SemPedido)
        );

        assert!(matches!(
            m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o),
            Pedido::Guardado {
                dispensada: None,
                ..
            }
        ));
        // Outra pessoa com o resumo certo: o resumo é público, a chamada não.
        assert_eq!(
            m.aceitar(B, 20, MOD, "mc-1", &certo),
            Err(MotivoDeAceiteInvalido::OutraPessoa)
        );
        // Quem foi chamado, com o resumo de outra oferta.
        assert_eq!(
            m.aceitar(A, 10, MOD, "mc-1", &resumo("mc-1", &oferta(25566))),
            Err(MotivoDeAceiteInvalido::OutroResumo)
        );
        // Outro servidor de jogo do mesmo MOD não tem pedido.
        assert_eq!(
            m.aceitar(A, 10, MOD, "mc-2", &resumo("mc-2", &o)),
            Err(MotivoDeAceiteInvalido::SemPedido)
        );
        // Nada disso mudou o livro: a máquina ainda não existe.
        assert!(!m.maquina_de(A, 10, MOD, "mc-1"));

        let aceita = m.aceitar(A, 10, MOD, "mc-1", &certo).expect("o certo");
        assert_eq!(aceita.quem, A);
        assert!(m.maquina_de(A, 10, MOD, "mc-1"));
        // A sessão nova da mesma pessoa não herda o aceite.
        assert!(!m.maquina_de(A, 11, MOD, "mc-1"));
        // O pendente saiu: o mesmo aceite de novo não tem pedido.
        assert_eq!(
            m.aceitar(A, 10, MOD, "mc-1", &certo),
            Err(MotivoDeAceiteInvalido::SemPedido)
        );
    }

    #[test]
    fn um_pedido_novo_a_outra_pessoa_dispensa_a_aceita() {
        let mut m = Maquinas::default();
        let o = oferta(25565);
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o);
        m.aceitar(A, 10, MOD, "mc-1", &resumo("mc-1", &o))
            .expect("A aceita");

        // A mesma pessoa, o mesmo resumo, já aceito: nada muda.
        assert_eq!(m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o), Pedido::JaAceito);
        assert!(m.maquina_de(A, 10, MOD, "mc-1"));

        let Pedido::Guardado { dispensada, .. } = m.pedir(MOD, HASH, "mc-1", B, GESTORA, &o) else {
            panic!("o pedido a B não ficou pendente");
        };
        assert_eq!(
            dispensada,
            Some(Dispensa {
                quem: A,
                mod_id: MOD.into(),
                servidor: "mc-1".into(),
                motivo: MotivoDaDispensa::OutroPedido,
                a_parar: vec!["mc".into()],
            })
        );
        assert!(!m.maquina_de(A, 10, MOD, "mc-1"));
        assert!(!m.tunel_permitido(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25565));
        // E B aceita no lugar dela.
        m.aceitar(B, 20, MOD, "mc-1", &resumo("mc-1", &o))
            .expect("B aceita");
        assert!(m.maquina_de(B, 20, MOD, "mc-1"));
    }

    /// Aceitar de novo, pela mesma conexão, uma oferta que mudou a porta de um
    /// túnel: o túnel da porta velha sai do aceite e tem de parar. O nome igual
    /// não basta para cobri-lo — o que foi aceito é nome, protocolo e porta.
    #[test]
    fn reaceitar_com_outra_porta_para_o_tunel_da_porta_velha() {
        let mut m = Maquinas::default();
        let velha = oferta(25565);
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &velha);
        m.aceitar(A, 10, MOD, "mc-1", &resumo("mc-1", &velha))
            .expect("o primeiro aceite");

        let nova = oferta(25566);
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &nova);
        let aceita = m
            .aceitar(A, 10, MOD, "mc-1", &resumo("mc-1", &nova))
            .expect("o segundo aceite");
        assert_eq!(aceita.a_parar, vec!["mc".to_owned()]);
        assert!(!m.tunel_permitido(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25565));
        assert!(m.tunel_permitido(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25566));

        // A mesma oferta aceita de novo não para nada.
        let _ = m.pedir(MOD, HASH, "mc-2", A, GESTORA, &nova);
        let outra = m
            .aceitar(A, 10, MOD, "mc-2", &resumo("mc-2", &nova))
            .expect("outro servidor de jogo, mesma oferta");
        assert!(outra.a_parar.is_empty());
    }

    #[test]
    fn sair_tira_a_maquina_e_devolve_os_servidores() {
        let mut m = Maquinas::default();
        let o = oferta(25565);
        for servidor in ["mc-1", "mc-2"] {
            let _ = m.pedir(MOD, HASH, servidor, A, GESTORA, &o);
            m.aceitar(A, 10, MOD, servidor, &resumo(servidor, &o))
                .expect("aceita");
        }
        // A conexão que não aceitou nada sai sem levar nada.
        assert!(m.saiu(A, 11).is_empty());
        assert!(m.maquina_de(A, 10, MOD, "mc-1"));

        let mut saiu = m.saiu(A, 10);
        saiu.sort();
        assert_eq!(
            saiu,
            vec![
                (MOD.to_owned(), "mc-1".to_owned()),
                (MOD.to_owned(), "mc-2".to_owned())
            ]
        );
        assert!(!m.maquina_de(A, 10, MOD, "mc-1"));
        assert!(!m.maquina_de(A, 10, MOD, "mc-2"));
        assert!(!m.tunel_permitido(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25565));
    }

    #[test]
    fn tunel_permitido_so_para_o_que_estava_na_oferta() {
        let mut m = Maquinas::default();
        let o = oferta(25565);
        // Sem aceite, nada.
        assert_eq!(
            m.conferir_tunel(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25565),
            Err(TunelForaDoAceite::SemMaquinaAceita)
        );
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o);
        // Pendente ainda não é aceite.
        assert!(!m.tunel_permitido(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25565));
        m.aceitar(A, 10, MOD, "mc-1", &resumo("mc-1", &o))
            .expect("aceita");

        assert!(m.tunel_permitido(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25565));
        assert_eq!(
            m.conferir_tunel(A, 10, MOD, "outro", ProtocoloDeTunel::Tcp, 25565),
            Err(TunelForaDoAceite::ForaDaOferta)
        );
        assert_eq!(
            m.conferir_tunel(A, 10, MOD, "mc", ProtocoloDeTunel::Udp, 25565),
            Err(TunelForaDoAceite::OutraPortaOuProtocolo)
        );
        assert_eq!(
            m.conferir_tunel(A, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25566),
            Err(TunelForaDoAceite::OutraPortaOuProtocolo)
        );
        // Outra conexão, outro MOD, outra pessoa: nada.
        assert!(!m.tunel_permitido(A, 11, MOD, "mc", ProtocoloDeTunel::Tcp, 25565));
        assert!(!m.tunel_permitido(A, 10, "x/outro", "mc", ProtocoloDeTunel::Tcp, 25565));
        assert!(!m.tunel_permitido(B, 10, MOD, "mc", ProtocoloDeTunel::Tcp, 25565));
    }

    /// **Um pendente que some diz a quem foi chamado** (a decisão da revisão
    /// do Task 3): substituído por um pedido a outra pessoa, ou com o MOD
    /// desligado. O mesmo `quem` chamado de novo não é dispensado — o pedido
    /// novo toma o lugar da tela dele.
    #[test]
    fn o_pendente_que_some_diz_quem_foi_chamado() {
        let mut m = Maquinas::default();
        let o = oferta(25565);
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o);
        let Pedido::Guardado {
            pendente_dispensado,
            ..
        } = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &oferta(25566))
        else {
            panic!("o pedido de novo a A não ficou pendente");
        };
        assert_eq!(
            pendente_dispensado, None,
            "A chamada de novo foi dispensada"
        );
        let Pedido::Guardado {
            pendente_dispensado,
            ..
        } = m.pedir(MOD, HASH, "mc-1", B, GESTORA, &o)
        else {
            panic!("o pedido a B não ficou pendente");
        };
        assert_eq!(pendente_dispensado, Some(A));

        let _ = m.pedir(MOD, HASH, "mc-2", A, GESTORA, &o);
        // B aceita mc-1 e é chamado de novo, com outra oferta: no desligar, ele
        // aparece só como máquina, e não também como pendente.
        m.aceitar(B, 20, MOD, "mc-1", &resumo("mc-1", &o))
            .expect("B aceita");
        let _ = m.pedir(MOD, HASH, "mc-1", B, GESTORA, &oferta(25566));
        let mut desligado = m.mod_desligado(MOD);
        desligado.pendentes.sort();
        assert_eq!(desligado.pendentes, vec![(A, "mc-2".to_owned())]);
        assert_eq!(desligado.maquinas, vec![(B, "mc-1".to_owned())]);
    }

    /// Quem pode ser chamado é quem tem uma conexão no 12 aberta, e deixa de
    /// ser quando a última sai.
    #[test]
    fn so_quem_esta_no_doze_pode_ser_chamado() {
        let mut m = Maquinas::default();
        assert!(!m.pode_ser_chamada(A));
        m.chegou(A, 10);
        m.chegou(A, 11);
        assert!(m.pode_ser_chamada(A));
        let _ = m.saiu(A, 10);
        assert!(
            m.pode_ser_chamada(A),
            "a outra conexão de A ainda está aberta"
        );
        let _ = m.saiu(A, 11);
        assert!(!m.pode_ser_chamada(A));
    }

    /// A mesma recusa só é **nova** uma vez por janela; outra chave, ou a
    /// janela passada, é nova de novo.
    #[test]
    fn a_mesma_recusa_so_e_nova_uma_vez_por_janela() {
        let mut m = Maquinas::default();
        let janela = std::time::Duration::from_secs(5);
        let t0 = std::time::Instant::now();
        assert!(m.marcar_recusa(MOD, "mc-1", A, "nao-esta-aqui", t0, janela));
        assert!(!m.marcar_recusa(MOD, "mc-1", A, "nao-esta-aqui", t0, janela));
        assert!(m.marcar_recusa(MOD, "mc-1", B, "nao-esta-aqui", t0, janela));
        assert!(m.marcar_recusa(MOD, "mc-1", A, "pendentes-demais", t0, janela));
        assert!(m.marcar_recusa(MOD, "mc-1", A, "nao-esta-aqui", t0 + janela, janela));
    }

    /// A recusa automática é da conexão; a da pessoa, do pedido. Uma conexão
    /// que chega com o pedido pendente passa a ser esperada e o recebe; a
    /// última que podia vê-lo saindo derruba o pedido com o motivo automático.
    #[test]
    fn a_recusa_automatica_e_da_conexao_e_a_da_pessoa_e_do_pedido() {
        use seele_proto::maquina::MotivoDaRecusaDeMaquina as M;
        let mut m = Maquinas::default();
        let o = oferta(25565);
        let _ = m.chegou(A, 10);
        let _ = m.chegou(A, 11);
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o);
        assert_eq!(m.recusar(A, 11, MOD, "mc-1", M::SoNoApp), Recusa::Esperando);
        assert_eq!(
            m.recusar(B, 10, MOD, "mc-1", M::PeloDono),
            Recusa::SemPedido
        );
        // Uma conexão nova é esperada e recebe o pedido; uma já esperada, não.
        assert_eq!(m.chegou(A, 12).len(), 1);
        assert!(m.chegou(A, 12).is_empty());
        assert_eq!(m.recusar(A, 10, MOD, "mc-1", M::SemTela), Recusa::Esperando);
        let _ = m.saiu(A, 12);
        assert_eq!(
            m.sem_quem_veja(A),
            vec![(MOD.to_owned(), "mc-1".to_owned(), M::SemTela)]
        );

        // `PeloDono` de qualquer conexão derruba na hora.
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o);
        assert_eq!(
            m.recusar(A, 11, MOD, "mc-1", M::PeloDono),
            Recusa::Recusado(M::PeloDono)
        );
        assert_eq!(
            m.aceitar(A, 10, MOD, "mc-1", &resumo("mc-1", &o)),
            Err(MotivoDeAceiteInvalido::SemPedido)
        );
    }
}
