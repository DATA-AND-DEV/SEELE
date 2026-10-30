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
//! `std::sync::Mutex` que nunca atravessa um `await`, e vem **antes** do de
//! [`crate::tuneis::Tuneis`] em todo lugar que toma os dois (a ordem do doc de
//! `session::apontar_um_par`): nunca os dois com o banco aberto.

use std::collections::HashMap;

use seele_proto::ids::PersonId;
use seele_proto::maquina::{MotivoDaDispensa, OfertaDeMaquina, TunelOferecido};
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
    tuneis: Vec<TunelOferecido>,
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

/// O livro das máquinas.
#[derive(Default)]
pub struct Maquinas {
    pendentes: HashMap<Chave, Pendente>,
    aceitas: HashMap<Chave, Aceitada>,
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
                tuneis: oferta.tuneis.clone(),
            },
        );
        Pedido::Guardado {
            resumo_hex,
            dispensada,
        }
    }

    fn aceitas_do_mod(&self, mod_id: &str) -> usize {
        self.aceitas.keys().filter(|(m, _)| m == mod_id).count()
    }

    /// Os nomes de túnel de `saindo` que nenhuma outra máquina aceita da mesma
    /// pessoa e conexão, no mesmo MOD, ainda oferece. `saindo` já tem de ter
    /// saído do livro: parar um nome que outra oferta aceita ainda cobre
    /// derrubaria um túnel legítimo.
    fn so_desta(&self, saindo: &Aceitada, mod_id: &str) -> Vec<String> {
        saindo
            .tuneis
            .iter()
            .filter(|t| {
                !self.aceitas.iter().any(|((m, _), a)| {
                    m == mod_id
                        && a.quem == saindo.quem
                        && a.conexao == saindo.conexao
                        && a.tuneis.iter().any(|o| o.nome == t.nome)
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
            tuneis: pendente.tuneis,
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

    /// A pessoa recusou o pedido. `true` se havia um pendente dela para esse
    /// servidor de jogo (e ele saiu); `false` se não havia o que recusar.
    pub fn recusar(&mut self, quem: PersonId, mod_id: &str, servidor: &str) -> bool {
        let k = chave(mod_id, servidor);
        if self.pendentes.get(&k).is_some_and(|p| p.quem == quem) {
            self.pendentes.remove(&k);
            true
        } else {
            false
        }
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
    /// dele saem. Volta `(quem, servidor)` de cada máquina que operava, para a
    /// dispensa.
    pub fn mod_desligado(&mut self, mod_id: &str) -> Vec<(PersonId, String)> {
        self.pendentes.retain(|(m, _), _| m != mod_id);
        let mut dispensadas = Vec::new();
        self.aceitas.retain(|(m, servidor), a| {
            if m == mod_id {
                dispensadas.push((a.quem, servidor.clone()));
                false
            } else {
                true
            }
        });
        dispensadas
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
    /// razão de [`Server::tuneis`]). **Antes** de `tuneis()` sempre que os dois
    /// forem tomados juntos.
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
        Pedido::Guardado { dispensada, .. } => {
            tracing::info!(mod_id = %mod_id, servidor = %servidor, quem = %quem, pedido_por = %pedido_por, "máquina: pedido mostrado a quem vai operá-la");
            if let Some(dispensa) = dispensada {
                dispensar(server, dispensa.clone());
            }
            let _ = server.events.send(Event::PedidoDeMaquina {
                para: quem,
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

/// Uma sessão acabou: as máquinas que a conexão dela aceitou saem do livro, e
/// a metade de servidor fica sabendo (`Parou { saiu }`). Os túneis são de
/// `Tuneis::saiu`, que os encerra com `MaquinaSaiu`.
pub fn saiu(server: &Server, quem: PersonId, conexao: u64) {
    let sairam = server.maquinas().saiu(quem, conexao);
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
        let dispensadas = server.maquinas().mod_desligado(&mod_id);
        for (quem, servidor) in dispensadas {
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
}
