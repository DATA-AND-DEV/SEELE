//! O livro das máquinas de jogo do servidor (protocolo 12, API 8 parte 3):
//! quem foi chamado a rodar a máquina de cada servidor de jogo, quem aceitou, e
//! o que a máquina aceita pode servir.
//!
//! # Por que um livro, e não uma pergunta ao cliente
//!
//! Até a parte 2, servir um túnel dependia só do MOD: qualquer sessão no 12
//! podia servir o nome de túnel de qualquer MOD com o poder `porta`, atrás
//! de um interruptor provisório que a parte 3 tirou. O aceite muda a pergunta
//! de «o MOD pode?» para «**esta pessoa**, por **esta conexão**, aceitou
//! **esta oferta**?» — e essa resposta só o servidor tem, porque foi ele quem
//! mostrou a oferta.
//!
//! # O que este módulo não decide
//!
//! Quem pede uma máquina é a metade de servidor do MOD (Task 5). Aqui só se
//! guarda o pendente, se confere o aceite contra o que foi mostrado, e se
//! responde se um túnel estava na oferta aceita. O cadeado é um
//! `std::sync::Mutex` que nunca atravessa um `await`; a ordem dele com o de
//! [`crate::tuneis::Tuneis`] e com o banco está em [`Server::maquinas`].

use std::collections::HashMap;
use std::sync::Arc;

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

/// **O papel de uma máquina num servidor de jogo** (API 8 parte 7): cada um
/// tem uma principal e, se a metade de servidor quiser, uma reserva. As duas
/// ficam aceitas ao mesmo tempo; só a dona agora ([`Maquinas::dona`]) serve.
///
/// O pedido leva o papel no fio (`PedidoDeMaquina.papel`), para a tela dizer
/// que é a reserva e o resumo o amarrar. O aceite e a parada continuam
/// chegando pela pessoa e pelo servidor de jogo: a mesma pessoa não pode ter
/// os dois papéis ([`Excesso::MesmaPessoaNasDuas`]), então a pessoa diz o
/// papel.
pub use seele_proto::maquina::PapelDaMaquina as Papel;

type Chave = (String, String, Papel);

fn chave(mod_id: &str, servidor: &str, papel: Papel) -> Chave {
    (mod_id.to_owned(), servidor.to_owned(), papel)
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
    /// O pedido saiu do livro, com este motivo; era deste papel.
    Recusado(MotivoDaRecusaDeMaquina, Papel),
}

/// Um pedido pendente reenviado a uma conexão que chegou depois dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reenvio {
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O servidor de jogo.
    pub servidor: String,
    /// O papel do pedido: o reenvio diz o mesmo que o primeiro.
    pub papel: Papel,
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
pub const fn automatica(motivo: MotivoDaRecusaDeMaquina) -> bool {
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
    /// A pessoa já é a máquina do outro papel deste servidor de jogo
    /// (`mesma-pessoa-nas-duas`).
    MesmaPessoaNasDuas,
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
    /// O papel dela.
    pub papel: Papel,
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
    /// O papel dela.
    pub papel: Papel,
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
    /// O túnel estava na oferta aceita, mas esta máquina não é a dona do
    /// servidor de jogo agora (API 8 parte 7): só a dona serve.
    NaoEADona,
}

impl TunelForaDoAceite {
    /// O nome, para o log.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::SemMaquinaAceita => "sem-maquina-aceita",
            Self::ForaDaOferta => "fora-da-oferta",
            Self::OutraPortaOuProtocolo => "outra-porta-ou-protocolo",
            Self::NaoEADona => "nao-e-a-dona",
        }
    }
}

/// A posse que mudou ([`Maquinas::passar_a_posse`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosseMudou {
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O servidor de jogo.
    pub servidor: String,
    /// O papel que era o dono.
    pub antes: Papel,
    /// O papel que é o dono agora.
    pub agora: Papel,
    /// Quem operava a máquina que deixou de ser a dona, se havia uma aceita.
    pub quem_era: Option<PersonId>,
    /// Os túneis dela que param.
    pub a_parar: Vec<String>,
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
        /// O papel do pedido.
        papel: Papel,
        /// Quem aceitou.
        quem: PersonId,
    },
    /// A pessoa recusou.
    Recusou {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// O papel do pedido.
        papel: Papel,
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
        /// O papel dela.
        papel: Papel,
        /// Quem operava.
        quem: PersonId,
        /// O nome do motivo.
        motivo: &'static str,
    },
    /// **A máquina parou e não voltou no prazo** (API 8 parte 7): o lugar
    /// dela segue vazio [`Server::prazo_da_maquina_sumida`] depois do `Parou`.
    /// É o relógio que a reserva precisa — a metade de servidor não acorda
    /// sozinha, e quem decide passar a posse é ela.
    Sumiu {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// O papel dela.
        papel: Papel,
        /// Quem operava.
        quem: PersonId,
        /// O motivo do `Parou` que abriu o prazo.
        motivo: &'static str,
        /// Há quantos segundos ela parou.
        segundos: u64,
    },
    /// **A pessoa que operava a máquina caiu e voltou** (API 8 parte 7): uma
    /// conexão dela negociou o 12 de novo, e o lugar segue vazio. A máquina
    /// não volta sozinha: a metade de servidor decide se pede de novo.
    Voltou {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// O servidor de jogo.
        servidor: String,
        /// O papel que ela tinha.
        papel: Papel,
        /// Quem voltou.
        quem: PersonId,
    },
    /// **A metade de servidor de `mod_id` subiu** (API 8, `MetadeSubiu`): o
    /// servidor SEELE subiu com ela, ou o conjunto trocou o código dela (ela
    /// entrou, ou mudou de revisão). O estado de módulo dela é novo, e o livro
    /// das máquinas é o que `maquinas` diz — num servidor que reiniciou,
    /// nenhum. A metade de servidor que guardou «ligado» no quintal chama de
    /// novo quem rodava (o pedido igual ao aceito religa a órfã, S2), e o que
    /// não responder ela diz na tela.
    MetadeSubiu {
        /// `autor/nome` do MOD.
        mod_id: String,
        /// `servidor-subiu` (a primeira vez desde que o servidor subiu) ou
        /// `codigo-novo` (o conjunto trocou o código com o servidor de pé).
        motivo: &'static str,
        /// [`Maquinas::lugares_do_mod`] agora.
        maquinas: Vec<(String, Papel, PersonId, bool)>,
    },
}

/// O que [`Maquinas::mod_desligado`] tirou do livro.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Desligado {
    /// `(quem, servidor, papel)` de cada máquina aceita que operava.
    pub maquinas: Vec<(PersonId, String, Papel)>,
    /// `(quem, servidor, papel)` de cada pedido que esperava resposta: a tela
    /// de aceite de quem foi chamado tem de fechar.
    pub pendentes: Vec<(PersonId, String, Papel)>,
    /// Os túneis de cada máquina que saiu e que nenhuma que ficou cobre.
    pub a_parar: Vec<(PersonId, Vec<String>)>,
    /// Ficou máquina aceita do MOD (feita contra o pacote ligado agora): os
    /// túneis do MOD não podem cair todos de uma vez.
    pub restam_aceitas: bool,
}

/// O livro das máquinas.
#[derive(Default)]
pub struct Maquinas {
    pendentes: HashMap<Chave, Pendente>,
    aceitas: HashMap<Chave, Aceitada>,
    /// **Quem é a dona agora** de cada servidor de jogo `(mod_id, servidor)`
    /// (API 8 parte 7). Só a metade de servidor muda isto
    /// ([`Self::passar_a_posse`]); ausente é [`Papel::Principal`], que é o
    /// que um MOD com uma máquina só sempre teve.
    donas: HashMap<(String, String), Papel>,
    /// A vez de cada lugar `(mod, servidor, papel)`: sobe a cada aceite e a
    /// cada saída. O prazo da máquina que sumiu ([`Self::sumiu_de_vez`]) só
    /// vale se a vez de quando ela saiu ainda é a de agora.
    vezes: HashMap<Chave, u64>,
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
    /// Quantos túneis cada MOD pediu para servir por uma máquina que não era
    /// a dona (`nao-e-a-dona`): o servidor os recusa, e conta.
    tuneis_de_quem_nao_e_dona: HashMap<String, u64>,
    /// Os lugares que cada pessoa operava quando a conexão dela caiu
    /// ([`Self::saiu`]): quando ela volta ([`Self::voltou`]), a metade de
    /// servidor ouve `MaquinaVoltou` de cada um que ninguém ocupou — é a
    /// principal que volta e pergunta quem é a dona.
    ausentes: HashMap<PersonId, Vec<Chave>>,
}

/// Quantos lugares se lembram por pessoa que caiu.
const LUGARES_LEMBRADOS_POR_PESSOA: usize = MAX_MAQUINAS_POR_MOD * 2;

/// **Quantas pessoas ausentes o livro lembra** (revisão final, M7): o `quem`
/// de um pedido a quem não está vem da metade de servidor, e qualquer número
/// virava uma entrada para sempre. Passado o teto, a pessoa nova não é
/// lembrada, e o log diz.
pub const MAX_AUSENTES_LEMBRADOS: usize = 4096;

impl Maquinas {
    /// O servidor pede a `quem` que rode a máquina **principal** de
    /// `servidor` ([`Self::pedir_como`]).
    pub fn pedir(
        &mut self,
        mod_id: &str,
        hash: &str,
        servidor: &str,
        quem: PersonId,
        pedido_por: PersonId,
        oferta: &OfertaDeMaquina,
    ) -> Pedido {
        self.pedir_como(mod_id, hash, servidor, Papel::Principal, quem, pedido_por, oferta)
    }

    /// O servidor pede a `quem` que rode a máquina `papel` de `servidor`,
    /// mostrando `oferta`.
    ///
    /// Um pedido novo para o mesmo lugar (servidor de jogo **e papel**)
    /// **substitui** o pendente: a metade de servidor mudou de ideia, e o
    /// aceite do pedido velho não pode valer pelo novo (o resumo já impede, e
    /// substituir tira o lixo). Se outra pessoa operava esse lugar, ela sai
    /// agora, com [`MotivoDaDispensa::OutroPedido`]. **A máquina do outro
    /// papel fica**: a principal e a reserva são duas máquinas aceitas do
    /// mesmo servidor de jogo, e só a dona serve ([`Self::conferir_tunel`]).
    ///
    /// A mesma pessoa não pode ser as duas (`mesma-pessoa-nas-duas`): o
    /// pedido, o aceite e a parada chegam pela pessoa e pelo servidor de jogo,
    /// e não diriam de qual papel são; e uma reserva na mesma máquina não é
    /// reserva.
    ///
    /// A mesma pessoa chamada de novo com outra oferta **continua** operando a
    /// aceita até aceitar a nova: o que ela aceitou antes segue sendo o que ela
    /// aceitou, e ninguém a dispensa por mudar o que se lhe pede.
    #[allow(clippy::too_many_arguments)]
    pub fn pedir_como(
        &mut self,
        mod_id: &str,
        hash: &str,
        servidor: &str,
        papel: Papel,
        quem: PersonId,
        pedido_por: PersonId,
        oferta: &OfertaDeMaquina,
    ) -> Pedido {
        let k = chave(mod_id, servidor, papel);
        let outro = chave(mod_id, servidor, papel.o_outro());
        if self.pendentes.get(&outro).is_some_and(|p| p.quem == quem)
            || self.aceitas.get(&outro).is_some_and(|a| a.quem == quem)
        {
            return Pedido::Recusado(Excesso::MesmaPessoaNasDuas);
        }
        let resumo_hex = seele_proto::maquina::resumo_em_hex(
            &seele_proto::maquina::resumo_da_oferta(mod_id, hash, servidor, papel, oferta),
        );
        if self
            .aceitas
            .get(&k)
            .is_some_and(|a| a.quem == quem && a.resumo_hex == resumo_hex)
        {
            return Pedido::JaAceito;
        }
        if !self.pendentes.contains_key(&k)
            && self.pendentes.keys().filter(|(m, _, _)| m == mod_id).count()
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
                self.mudar_a_vez(&k);
                let a_parar = self.so_desta(&velha, mod_id);
                Dispensa {
                    quem: velha.quem,
                    mod_id: mod_id.to_owned(),
                    servidor: servidor.to_owned(),
                    papel,
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
        self.aceitas.keys().filter(|(m, _, _)| m == mod_id).count()
    }

    fn mudar_a_vez(&mut self, k: &Chave) -> u64 {
        let vez = self.vezes.entry(k.clone()).or_default();
        *vez += 1;
        *vez
    }

    /// A vez de agora do lugar `(mod_id, servidor, papel)`.
    #[must_use]
    pub fn vez(&self, mod_id: &str, servidor: &str, papel: Papel) -> u64 {
        self.vezes
            .get(&chave(mod_id, servidor, papel))
            .copied()
            .unwrap_or(0)
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
                !self.aceitas.iter().any(|((m, servidor, papel), a)| {
                    m == mod_id
                        && a.quem == saindo.quem
                        && a.conexao == saindo.conexao
                        && self.dona(m, servidor) == *papel
                        && a.tuneis.contains(t)
                })
            })
            .map(|t| t.nome.clone())
            .collect()
    }

    /// O papel em que `quem` tem um pendente de `(mod_id, servidor)`.
    fn papel_pendente(&self, mod_id: &str, servidor: &str, quem: PersonId) -> Option<Papel> {
        Papel::TODOS.into_iter().find(|papel| {
            self.pendentes
                .get(&chave(mod_id, servidor, *papel))
                .is_some_and(|p| p.quem == quem)
        })
    }

    /// O papel em que `quem`, por `conexao`, é máquina aceita de
    /// `(mod_id, servidor)`.
    #[must_use]
    pub fn papel_de(
        &self,
        quem: PersonId,
        conexao: u64,
        mod_id: &str,
        servidor: &str,
    ) -> Option<Papel> {
        Papel::TODOS.into_iter().find(|papel| {
            self.aceitas
                .get(&chave(mod_id, servidor, *papel))
                .is_some_and(|a| a.quem == quem && a.conexao == conexao)
        })
    }

    /// O papel em que `quem` (por qualquer conexão) é máquina aceita de
    /// `(mod_id, servidor)`: é o que vai em `contexto.maquina.papel`.
    #[must_use]
    pub fn papel_da_pessoa(&self, quem: PersonId, mod_id: &str, servidor: &str) -> Option<Papel> {
        Papel::TODOS.into_iter().find(|papel| {
            self.aceitas
                .get(&chave(mod_id, servidor, *papel))
                .is_some_and(|a| a.quem == quem)
        })
    }

    /// A pessoa aceitou o que lhe foi mostrado.
    ///
    /// Vale só se há pedido pendente para `(mod_id, servidor)`, chamado a
    /// `quem`, com este resumo. Aceito, o pendente sai e a máquina entra,
    /// presa a `conexao`, no papel do pedido.
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
        let papel = match self.papel_pendente(mod_id, servidor, quem) {
            Some(papel) => papel,
            None if Papel::TODOS
                .iter()
                .any(|p| self.pendentes.contains_key(&chave(mod_id, servidor, *p))) =>
            {
                return Err(MotivoDeAceiteInvalido::OutraPessoa)
            }
            None => return Err(MotivoDeAceiteInvalido::SemPedido),
        };
        let k = chave(mod_id, servidor, papel);
        let pendente = self
            .pendentes
            .get(&k)
            .ok_or(MotivoDeAceiteInvalido::SemPedido)?;
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
        self.mudar_a_vez(&k);
        // O aceite anterior da mesma pessoa (outra oferta, ou outra conexão):
        // o que ele cobria e a oferta nova não cobre para de valer.
        let a_parar = match self.aceitas.insert(k, nova) {
            Some(velha) => self.so_desta(&velha, mod_id),
            None => Vec::new(),
        };
        tracing::debug!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, pedido_por = %pendente.pedido_por, "máquina: aceite registrado no livro");
        Ok(Aceita {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            papel,
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
        let Some(papel) = self.papel_pendente(mod_id, servidor, quem) else {
            return Recusa::SemPedido;
        };
        let k = chave(mod_id, servidor, papel);
        let Some(pendente) = self.pendentes.get_mut(&k) else {
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
        Recusa::Recusado(motivo, papel)
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
        let papel = self.papel_de(quem, conexao, mod_id, servidor)?;
        let k = chave(mod_id, servidor, papel);
        let velha = self.aceitas.remove(&k)?;
        self.mudar_a_vez(&k);
        Some(Aceita {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            papel,
            quem,
            a_parar: self.so_desta(&velha, mod_id),
        })
    }

    /// Uma conexão acabou: as máquinas que **ela** aceitou saem do livro, e
    /// volta `(mod_id, servidor, papel, vez)` de cada uma — a vez nova do
    /// lugar, que o prazo da máquina que sumiu confere. Os túneis dela não são
    /// daqui: `Tuneis::saiu` os encerra com `MaquinaSaiu`.
    ///
    /// Os pendentes chamados a esta pessoa ficam: o pedido é à pessoa, e outra
    /// sessão dela pode responder.
    pub fn saiu(&mut self, quem: PersonId, conexao: u64) -> Vec<(String, String, Papel, u64)> {
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
        let mut voltas = Vec::new();
        for k in sairam {
            self.aceitas.remove(&k);
            let lembrados = self.ausentes.entry(quem).or_default();
            if !lembrados.contains(&k) && lembrados.len() < LUGARES_LEMBRADOS_POR_PESSOA {
                lembrados.push(k.clone());
            }
            let vez = self.mudar_a_vez(&k);
            let (mod_id, servidor, papel) = k;
            voltas.push((mod_id, servidor, papel, vez));
        }
        voltas
    }

    /// **Um pedido a quem não está** (`nao-esta-aqui`): o lugar é lembrado
    /// como o de quem caiu, e quando a pessoa chegar a metade de servidor ouve
    /// `MaquinaVoltou` dele — e decide pedir de novo. É o caminho de quem
    /// hospeda que volta: o livro nasce vazio, a metade de servidor chama de
    /// novo quem ela guardou, e quem ainda não reconectou é chamado quando
    /// reconectar, em vez de a chamada se perder.
    pub fn lembrar_quem_nao_esta(
        &mut self,
        quem: PersonId,
        mod_id: &str,
        servidor: &str,
        papel: Papel,
    ) {
        let k = chave(mod_id, servidor, papel);
        if !self.ausentes.contains_key(&quem) && self.ausentes.len() >= MAX_AUSENTES_LEMBRADOS {
            tracing::warn!(%mod_id, %servidor, %quem, teto = MAX_AUSENTES_LEMBRADOS, "máquina: ausentes demais no livro; o pedido a quem não está não é lembrado para a volta dela");
            return;
        }
        let lembrados = self.ausentes.entry(quem).or_default();
        if !lembrados.contains(&k) && lembrados.len() < LUGARES_LEMBRADOS_POR_PESSOA {
            lembrados.push(k);
        }
    }

    /// **A pessoa voltou** (uma conexão dela negociou o 12 de novo): os lugares
    /// que ela operava quando caiu e que **seguem vazios**, para a metade de
    /// servidor ouvir `MaquinaVoltou` e decidir — pedir de novo, ou dizer a
    /// ela que outra é a dona. Cada lugar é dito uma vez.
    pub fn voltou(&mut self, quem: PersonId) -> Vec<(String, String, Papel)> {
        let lembrados = self.ausentes.remove(&quem).unwrap_or_default();
        lembrados
            .into_iter()
            .filter(|k| !self.aceitas.contains_key(k))
            .collect()
    }

    /// **Quem saiu por uma conexão e segue aqui por outra** (revisão final,
    /// I7): a pessoa que piscou abre a conexão nova antes de a velha ser dada
    /// por morta, e o `chegou` dela rodou com o lugar ainda ocupado — o
    /// `voltou` dali não tinha o que dizer. Quando a velha enfim cai, os
    /// lugares que ela deixou são de alguém que **já está aqui**: saem de
    /// `ausentes` agora, para o `MaquinaVoltou` sair junto do `Parou`, em vez
    /// de esperar uma chegada que já aconteceu.
    pub fn voltas_de_quem_segue(&mut self, quem: PersonId) -> Vec<(String, String, Papel)> {
        if self.no_doze.contains_key(&quem) {
            self.voltou(quem)
        } else {
            Vec::new()
        }
    }

    /// **O prazo da máquina que sumiu venceu**: se o lugar ainda está vazio e
    /// na mesma vez de quando ela saiu, devolve `true` — ela não voltou nem foi
    /// trocada. É o que decide se `MaquinaSumiu` sai.
    #[must_use]
    pub fn sumiu_de_vez(&self, mod_id: &str, servidor: &str, papel: Papel, vez: u64) -> bool {
        let k = chave(mod_id, servidor, papel);
        !self.aceitas.contains_key(&k) && self.vezes.get(&k).copied().unwrap_or(0) == vez
    }

    /// O MOD foi desligado (`ligado = None`) ou trocou de pacote (`ligado` é o
    /// hash de agora): todo pendente e toda máquina dele **feitos contra outro
    /// pacote** saem. Volta `(quem, servidor, papel)` de cada máquina que
    /// operava e de cada pedido que esperava resposta, para a dispensa.
    ///
    /// **O que já foi feito contra o pacote novo fica.** A vigia que chama isto
    /// vê o conjunto de MODs depois da troca, e entre a troca e ela a metade de
    /// servidor nova pode já ter pedido — e alguém aceitado. Varrer o MOD
    /// inteiro por haver uma entrada velha levava também essas, e a pessoa que
    /// acabou de aceitar o pacote novo era dispensada com `mod-desligado`.
    pub fn mod_desligado(&mut self, mod_id: &str, ligado: Option<&str>) -> Desligado {
        let mut desligado = Desligado::default();
        let sai = |m: &String, hash: &String| m == mod_id && Some(hash.as_str()) != ligado;
        self.pendentes.retain(|(m, servidor, papel), p| {
            if sai(m, &p.hash) {
                desligado.pendentes.push((p.quem, servidor.clone(), *papel));
                false
            } else {
                true
            }
        });
        let mut sairam = Vec::new();
        let mut chaves = Vec::new();
        self.aceitas.retain(|(m, servidor, papel), a| {
            if sai(m, &a.hash) {
                desligado.maquinas.push((a.quem, servidor.clone(), *papel));
                chaves.push((m.clone(), servidor.clone(), *papel));
                sairam.push(Aceitada {
                    quem: a.quem,
                    conexao: a.conexao,
                    hash: a.hash.clone(),
                    resumo_hex: a.resumo_hex.clone(),
                    tuneis: a.tuneis.clone(),
                });
                false
            } else {
                true
            }
        });
        for k in &chaves {
            self.mudar_a_vez(k);
        }
        desligado.restam_aceitas = self.aceitas.keys().any(|(m, _, _)| m == mod_id);
        if ligado.is_none() {
            self.donas.retain(|(m, _), _| m != mod_id);
            // **O MOD desligado leva o que o livro lembrava dele** (M7): as
            // vezes dos lugares e os lugares de quem estava ausente.
            self.vezes.retain(|(m, _, _), _| m != mod_id);
            for lembrados in self.ausentes.values_mut() {
                lembrados.retain(|(m, _, _)| m != mod_id);
            }
            self.ausentes.retain(|_, lembrados| !lembrados.is_empty());
        }
        desligado.a_parar = sairam
            .iter()
            .map(|a| (a.quem, self.so_desta(a, mod_id)))
            .filter(|(_, nomes)| !nomes.is_empty())
            .collect();
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
        for ((mod_id, servidor, papel), pendente) in &mut self.pendentes {
            if pendente.quem != quem || pendente.esperando.contains(&conexao) {
                continue;
            }
            pendente.esperando.push(conexao);
            reenvios.push(Reenvio {
                mod_id: mod_id.clone(),
                servidor: servidor.clone(),
                papel: *papel,
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
    ) -> Vec<(String, String, Papel, MotivoDaRecusaDeMaquina)> {
        let mut caidos = Vec::new();
        self.pendentes
            .retain(|(mod_id, servidor, papel), p| match p.ultima_automatica {
                Some(motivo) if p.quem == quem && p.esperando.is_empty() => {
                    caidos.push((mod_id.clone(), servidor.clone(), *papel, motivo));
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

    /// Se `quem` recusou **sozinho** (`SemTela`, `SoNoApp`) um pedido de
    /// `(mod_id, servidor)` nos últimos `janela`. Um momento que pede de novo
    /// logo depois disso receberia a mesma recusa na volta da rede, e assim
    /// para sempre: o despejo não manda (`mods::maquina::despejar`).
    #[must_use]
    pub fn recusa_automatica_recente(
        &self,
        mod_id: &str,
        servidor: &str,
        quem: PersonId,
        agora: std::time::Instant,
        janela: std::time::Duration,
    ) -> bool {
        [
            MotivoDaRecusaDeMaquina::SemTela,
            MotivoDaRecusaDeMaquina::SoNoApp,
        ]
        .iter()
        .any(|motivo| {
            let chave = (
                mod_id.to_owned(),
                servidor.to_owned(),
                quem,
                seele_proto::maquina::nome_da_recusa(motivo),
            );
            self.recusas_ditas
                .get(&chave)
                .is_some_and(|quando| agora.saturating_duration_since(*quando) < janela)
        })
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

    /// Quem opera a máquina `papel` de `(mod_id, servidor)`, e por qual
    /// conexão.
    #[must_use]
    pub fn operador(&self, mod_id: &str, servidor: &str, papel: Papel) -> Option<(PersonId, u64)> {
        self.aceitas
            .get(&chave(mod_id, servidor, papel))
            .map(|a| (a.quem, a.conexao))
    }

    /// **O que o livro sabe agora das máquinas de `mod_id`**: cada lugar
    /// `(servidor, papel, quem, aceita)`, aceito ou com um pedido esperando
    /// resposta, em ordem. É a carga de `MetadeSubiu`: um servidor que acabou
    /// de subir tem o livro vazio, e a metade de servidor que guardou «ligado»
    /// no quintal precisa saber que ninguém aqui roda aquilo.
    #[must_use]
    pub fn lugares_do_mod(&self, mod_id: &str) -> Vec<(String, Papel, PersonId, bool)> {
        let mut lugares: Vec<(String, Papel, PersonId, bool)> = self
            .aceitas
            .iter()
            .filter(|((m, _, _), _)| m == mod_id)
            .map(|((_, s, p), a)| (s.clone(), *p, a.quem, true))
            .chain(
                self.pendentes
                    .iter()
                    .filter(|((m, s, p), _)| {
                        m == mod_id && !self.aceitas.contains_key(&chave(m, s, *p))
                    })
                    .map(|((_, s, p), pe)| (s.clone(), *p, pe.quem, false)),
            )
            .collect();
        lugares.sort_by(|a, b| (&a.0, a.1.nome()).cmp(&(&b.0, b.1.nome())));
        lugares
    }

    /// Quantos avisos de `mod_id` não saíram por falta de máquina aceita.
    #[must_use]
    pub fn avisos_sem_maquina(&self, mod_id: &str) -> u64 {
        self.sem_maquina.get(mod_id).copied().unwrap_or(0)
    }

    /// **Quem é a dona agora** de `(mod_id, servidor)`: o papel que a metade
    /// de servidor disse por último, ou [`Papel::Principal`].
    #[must_use]
    pub fn dona(&self, mod_id: &str, servidor: &str) -> Papel {
        self.donas
            .get(&(mod_id.to_owned(), servidor.to_owned()))
            .copied()
            .unwrap_or(Papel::Principal)
    }

    /// **A metade de servidor passa a posse** de `(mod_id, servidor)` a
    /// `papel`. Devolve `None` se a dona já era essa; senão, a máquina que
    /// deixou de ser a dona (se havia uma aceita) e os túneis dela que param
    /// — nenhuma máquina que não é a dona serve.
    pub fn passar_a_posse(
        &mut self,
        mod_id: &str,
        servidor: &str,
        papel: Papel,
    ) -> Option<PosseMudou> {
        let antes = self.dona(mod_id, servidor);
        if antes == papel {
            return None;
        }
        self.donas
            .insert((mod_id.to_owned(), servidor.to_owned()), papel);
        let velha = self
            .aceitas
            .get(&chave(mod_id, servidor, antes))
            .map(|a| Aceitada {
                quem: a.quem,
                conexao: a.conexao,
                hash: a.hash.clone(),
                resumo_hex: a.resumo_hex.clone(),
                tuneis: a.tuneis.clone(),
            });
        let a_parar = velha
            .as_ref()
            .map(|v| self.so_desta(v, mod_id))
            .unwrap_or_default();
        Some(PosseMudou {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            antes,
            agora: papel,
            quem_era: velha.map(|v| v.quem),
            a_parar,
        })
    }

    /// Conta um túnel que uma máquina sem a posse pediu para servir.
    pub fn contar_tunel_de_quem_nao_e_dona(&mut self, mod_id: &str) -> u64 {
        let n = self
            .tuneis_de_quem_nao_e_dona
            .entry(mod_id.to_owned())
            .or_default();
        *n += 1;
        *n
    }

    /// Quantos túneis de `mod_id` foram recusados por a máquina não ser a
    /// dona.
    #[must_use]
    pub fn tuneis_de_quem_nao_e_dona(&self, mod_id: &str) -> u64 {
        self.tuneis_de_quem_nao_e_dona
            .get(mod_id)
            .copied()
            .unwrap_or(0)
    }

    /// Por que `quem`/`conexao` não pode servir este túnel, ou `Ok` se pode:
    /// é máquina aceita de algum servidor de jogo do MOD, **e a dona dele
    /// agora**, cuja oferta traz um [`TunelOferecido`] igual — nome, protocolo
    /// e porta.
    ///
    /// # Errors
    ///
    /// Qual das quatro coisas não bateu.
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
            .filter(|((m, _, _), a)| m == mod_id && a.quem == quem && a.conexao == conexao)
            .peekable();
        if dela.peek().is_none() {
            return Err(TunelForaDoAceite::SemMaquinaAceita);
        }
        let mut achou_o_nome = false;
        let mut de_quem_nao_e_dona = false;
        for ((m, servidor, papel), a) in dela {
            for t in a.tuneis.iter().filter(|t| t.nome == nome) {
                achou_o_nome = true;
                if t.protocolo == protocolo && t.porta == porta {
                    if self.dona(m, servidor) == *papel {
                        return Ok(());
                    }
                    de_quem_nao_e_dona = true;
                }
            }
        }
        Err(if de_quem_nao_e_dona {
            TunelForaDoAceite::NaoEADona
        } else if achou_o_nome {
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

    /// Se `quem`, por `conexao`, é uma máquina aceita de `(mod_id, servidor)`,
    /// em qualquer papel.
    #[must_use]
    pub fn maquina_de(&self, quem: PersonId, conexao: u64, mod_id: &str, servidor: &str) -> bool {
        self.papel_de(quem, conexao, mod_id, servidor).is_some()
    }

    /// `conexao` de `quem` já é a máquina aceita de `(mod_id, servidor)` com
    /// este mesmo `resumo_hex`: um aceite repetido, que não muda nada.
    ///
    /// É o único aceite inválido que não volta como `SemAceite`: dispensar aqui
    /// derrubaria, no app, a metade que o livro diz valer.
    #[must_use]
    pub fn ja_aceita_igual(
        &self,
        quem: PersonId,
        conexao: u64,
        mod_id: &str,
        servidor: &str,
        resumo_hex: &str,
    ) -> bool {
        Papel::TODOS.into_iter().any(|papel| {
            self.aceitas
                .get(&chave(mod_id, servidor, papel))
                .is_some_and(|a| {
                    a.quem == quem && a.conexao == conexao && a.resumo_hex == resumo_hex
                })
        })
    }

    /// Quem opera a máquina **principal** de `(mod_id, servidor)`, se alguém.
    #[must_use]
    pub fn quem_opera(&self, mod_id: &str, servidor: &str) -> Option<PersonId> {
        self.quem_opera_como(mod_id, servidor, Papel::Principal)
    }

    /// Quem opera a máquina `papel` de `(mod_id, servidor)`, se alguém.
    #[must_use]
    pub fn quem_opera_como(&self, mod_id: &str, servidor: &str, papel: Papel) -> Option<PersonId> {
        self.aceitas
            .get(&chave(mod_id, servidor, papel))
            .map(|a| a.quem)
    }

    /// Cada `(mod_id, hash)` que o livro guarda, pendente ou aceito: o que a
    /// vigia do conjunto de MODs confere contra o pacote ligado.
    #[must_use]
    pub fn hashes(&self) -> Vec<(String, String)> {
        let mut todos: Vec<(String, String)> = self
            .pendentes
            .iter()
            .map(|((m, _, _), p)| (m.clone(), p.hash.clone()))
            .chain(
                self.aceitas
                    .iter()
                    .map(|((m, _, _), a)| (m.clone(), a.hash.clone())),
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

/// O servidor pede a máquina **principal** ([`pedir_como`]).
pub fn pedir(
    server: &Server,
    mod_id: &str,
    hash: &str,
    servidor: &str,
    quem: PersonId,
    pedido_por: PersonId,
    oferta: &OfertaDeMaquina,
) -> Pedido {
    pedir_como(server, mod_id, hash, servidor, Papel::Principal, quem, pedido_por, oferta)
}

/// O servidor pede uma máquina: guarda o pendente, manda o `PedidoDeMaquina` a
/// `quem` e dispensa quem operava antes **aquele papel**, se era outra pessoa.
///
/// É a porta da metade de servidor (Task 5) e das provas: o livro sozinho não
/// fala com ninguém.
#[allow(clippy::too_many_arguments)]
pub fn pedir_como(
    server: &Server,
    mod_id: &str,
    hash: &str,
    servidor: &str,
    papel: Papel,
    quem: PersonId,
    pedido_por: PersonId,
    oferta: &OfertaDeMaquina,
) -> Pedido {
    let pedido = server
        .maquinas()
        .pedir_como(mod_id, hash, servidor, papel, quem, pedido_por, oferta);
    match &pedido {
        Pedido::Guardado {
            dispensada,
            pendente_dispensado,
            ..
        } => {
            tracing::info!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, pedido_por = %pedido_por, "máquina: pedido mostrado a quem vai operá-la");
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
                papel,
            });
        }
        Pedido::JaAceito => {
            tracing::debug!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, "máquina: pedido igual ao já aceito; nada muda");
        }
        Pedido::Recusado(excesso) => {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, pedido_por = %pedido_por, ?excesso, "máquina: pedido recusado por um teto");
        }
    }
    pedido
}

/// **A metade de servidor passou a posse** de `(mod_id, servidor)` a
/// `papel` (API 8 parte 7): os túneis da dona de antes param **agora**, e não
/// só quando ela parar — uma máquina que não é a dona não serve ninguém,
/// mesmo se o processo dela segue vivo.
pub fn passar_a_posse(server: &Server, mod_id: &str, servidor: &str, papel: Papel) {
    let mudou = server.maquinas().passar_a_posse(mod_id, servidor, papel);
    let Some(mudou) = mudou else {
        tracing::debug!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), "máquina: a posse já era desse papel; nada muda");
        return;
    };
    tracing::info!(mod_id = %mod_id, servidor = %servidor, antes = mudou.antes.nome(), agora = mudou.agora.nome(), quem_era = ?mudou.quem_era, tuneis_que_param = ?mudou.a_parar, "máquina: a posse do servidor de jogo mudou");
    if let Some(quem) = mudou.quem_era {
        parar_tuneis(server, quem, mod_id, &mudou.a_parar);
    }
}

/// **Abre o prazo da máquina que parou** (API 8 parte 7): passado
/// [`Server::prazo_da_maquina_sumida`] com o lugar ainda vazio e na mesma
/// vez, a metade de servidor ouve `MaquinaSumiu`. Sem runtime do Tokio (um
/// teste de unidade), não abre.
#[allow(clippy::too_many_arguments)]
fn abrir_o_prazo(
    server: &Arc<Server>,
    mod_id: String,
    servidor: String,
    papel: Papel,
    quem: PersonId,
    motivo: &'static str,
    vez: u64,
) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let prazo = server.prazo_da_maquina_sumida;
    // Fraco: um servidor que desce não fica de pé por um prazo aberto.
    let fraco = Arc::downgrade(server);
    runtime.spawn(async move {
        tokio::time::sleep(prazo).await;
        let Some(server) = fraco.upgrade() else {
            return;
        };
        if !server
            .maquinas()
            .sumiu_de_vez(&mod_id, &servidor, papel, vez)
        {
            tracing::debug!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, "máquina: voltou (ou foi trocada) dentro do prazo; MaquinaSumiu não sai");
            return;
        }
        tracing::info!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, motivo, segundos = prazo.as_secs(), "máquina: sumiu por mais que o prazo; a metade de servidor fica sabendo");
        publicar(
            &server,
            MudancaDeMaquina::Sumiu {
                mod_id,
                servidor,
                papel,
                quem,
                motivo,
                segundos: prazo.as_secs(),
            },
        );
    });
}

/// A máquina pediu para parar (`MaquinaParada`): sai do livro, a metade de
/// servidor ouve `MaquinaParou`, os túneis dela param e o prazo abre.
pub fn parou(
    server: &Arc<Server>,
    quem: PersonId,
    conexao: u64,
    mod_id: &str,
    servidor: &str,
    motivo: &'static str,
) -> bool {
    let parada = server.maquinas().parar(quem, conexao, mod_id, servidor);
    let Some(parada) = parada else {
        return false;
    };
    let vez = server.maquinas().vez(mod_id, servidor, parada.papel);
    tracing::info!(mod_id = %mod_id, servidor = %servidor, papel = parada.papel.nome(), quem = %quem, motivo, "máquina: parou");
    publicar(
        server,
        MudancaDeMaquina::Parou {
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
            papel: parada.papel,
            quem,
            motivo,
        },
    );
    parar_tuneis(server, quem, mod_id, &parada.a_parar);
    abrir_o_prazo(
        server,
        mod_id.to_owned(),
        servidor.to_owned(),
        parada.papel,
        quem,
        motivo,
        vez,
    );
    true
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
            papel: dispensa.papel,
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
            fim.extend(livro.parar_pelo_servidor(quem, mod_id, nome));
        }
    }
    crate::tuneis::anunciar(server, fim);
}

/// Uma conexão de `quem` negociou o 12 ([`Maquinas::chegou`]): os pedidos que
/// já a esperavam vão a ela, e só a ela — as outras sessões já os têm.
pub fn chegou(server: &Server, quem: PersonId, conexao: u64) {
    let (reenvios, voltas) = {
        let mut livro = server.maquinas();
        (livro.chegou(quem, conexao), livro.voltou(quem))
    };
    for (mod_id, servidor, papel) in voltas {
        tracing::info!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, conexao, "máquina: quem operava caiu e voltou; a metade de servidor decide");
        publicar(
            server,
            MudancaDeMaquina::Voltou {
                mod_id,
                servidor,
                papel,
                quem,
            },
        );
    }
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
            papel: r.papel,
        });
    }
}

/// Guarda que `quem` recusou o pedido de `(mod_id, servidor)` agora, quando a
/// recusa foi automática: é a janela que [`Maquinas::recusa_automatica_recente`]
/// lê para um momento não pedir de novo no passo da rede.
pub fn lembrar_a_recusa(
    server: &Server,
    mod_id: &str,
    servidor: &str,
    quem: PersonId,
    motivo: MotivoDaRecusaDeMaquina,
) {
    if automatica(motivo) {
        let _ = server.maquinas().marcar_recusa(
            mod_id,
            servidor,
            quem,
            seele_proto::maquina::nome_da_recusa(&motivo),
            std::time::Instant::now(),
            crate::mods::maquina::JANELA_DA_RECUSA_REPETIDA,
        );
    }
}

/// Uma sessão acabou: as máquinas que a conexão dela aceitou saem do livro, e
/// a metade de servidor fica sabendo (`Parou { saiu }`). Os túneis são de
/// `Tuneis::saiu`, que os encerra com `MaquinaSaiu`.
pub fn saiu(server: &Arc<Server>, quem: PersonId, conexao: u64) {
    let (sairam, caidos, seguem) = {
        let mut livro = server.maquinas();
        let sairam = livro.saiu(quem, conexao);
        let caidos = livro.sem_quem_veja(quem);
        (sairam, caidos, livro.voltas_de_quem_segue(quem))
    };
    for (mod_id, servidor, papel, motivo) in caidos {
        let nome = seele_proto::maquina::nome_da_recusa(&motivo);
        lembrar_a_recusa(server, &mod_id, &servidor, quem, motivo);
        tracing::info!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, motivo = nome, "máquina: a última sessão que podia ver o pedido saiu, e as outras recusaram sozinhas");
        publicar(
            server,
            MudancaDeMaquina::Recusou {
                mod_id,
                servidor,
                papel,
                quem,
                motivo: nome,
            },
        );
    }
    for (mod_id, servidor, papel, vez) in sairam {
        tracing::info!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, motivo = "saiu", "máquina: quem operava saiu");
        publicar(
            server,
            MudancaDeMaquina::Parou {
                mod_id: mod_id.clone(),
                servidor: servidor.clone(),
                papel,
                quem,
                motivo: "saiu",
            },
        );
        abrir_o_prazo(server, mod_id, servidor, papel, quem, "saiu", vez);
    }
    for (mod_id, servidor, papel) in seguem {
        tracing::info!(mod_id = %mod_id, servidor = %servidor, papel = papel.nome(), quem = %quem, conexao, "máquina: a conexão velha de quem operava caiu, e a pessoa segue por outra; a metade de servidor decide");
        publicar(
            server,
            MudancaDeMaquina::Voltou {
                mod_id,
                servidor,
                papel,
                quem,
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
        let ligado = ligados.get(&mod_id).map(String::as_str);
        let desligado = server.maquinas().mod_desligado(&mod_id, ligado);
        for (quem, servidor, _) in desligado.pendentes {
            dispensar_o_pendente(
                server,
                quem,
                &mod_id,
                &servidor,
                MotivoDaDispensa::ModDesligado,
            );
        }
        for (quem, servidor, papel) in desligado.maquinas {
            dispensar(
                server,
                Dispensa {
                    quem,
                    mod_id: mod_id.clone(),
                    servidor,
                    papel,
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
        // aceite do pacote velho não pode seguir de pé. Com máquina do pacote
        // novo de pé, só caem os túneis das que saíram: os dela são do aceite
        // que vale.
        if desligado.restam_aceitas {
            for (quem, nomes) in &desligado.a_parar {
                parar_tuneis(server, *quem, &mod_id, nomes);
            }
        } else {
            let fim = server.tuneis().mod_desligado(&mod_id);
            crate::tuneis::anunciar(server, fim);
        }
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
            instalar: None,
            por_sistema: vec![],
        }
    }

    fn resumo(servidor: &str, o: &OfertaDeMaquina) -> String {
        resumo_como(Papel::Principal, servidor, o)
    }

    fn resumo_como(papel: Papel, servidor: &str, o: &OfertaDeMaquina) -> String {
        seele_proto::maquina::resumo_em_hex(&seele_proto::maquina::resumo_da_oferta(
            MOD, HASH, servidor, papel, o,
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
                papel: Papel::Principal,
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

        let mut saiu: Vec<(String, String)> =
            m.saiu(A, 10).into_iter().map(|(m, s, _, _)| (m, s)).collect();
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
        let mut desligado = m.mod_desligado(MOD, None);
        desligado.pendentes.sort();
        assert_eq!(desligado.pendentes, vec![(A, "mc-2".to_owned(), Papel::Principal)]);
        assert_eq!(desligado.maquinas, vec![(B, "mc-1".to_owned(), Papel::Principal)]);
    }

    /// **A troca de pacote leva só o que foi feito contra o pacote velho.** A
    /// vigia vê a troca depois de a metade de servidor nova já ter pedido (e
    /// alguém aceitado): essas entradas são do pacote ligado e ficam, com os
    /// túneis delas.
    #[test]
    fn trocar_de_pacote_nao_leva_o_que_ja_e_do_pacote_novo() {
        const NOVO: &str = "bb00000000000000000000000000000000000000000000000000000000000000";
        let resumo_de = |hash: &str, servidor: &str, o: &OfertaDeMaquina| {
            seele_proto::maquina::resumo_em_hex(&seele_proto::maquina::resumo_da_oferta(
                MOD,
                hash,
                servidor,
                Papel::Principal,
                o,
            ))
        };
        let mut m = Maquinas::default();
        let velha = oferta(25565);
        let nova = oferta(25566);
        // A aceitou mc-1 no pacote velho; B aceitou mc-2 no novo; A tem um
        // pedido do pacote novo pendente para mc-3, e B um do velho para mc-4.
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &velha);
        m.aceitar(A, 10, MOD, "mc-1", &resumo_de(HASH, "mc-1", &velha))
            .expect("A aceita no pacote velho");
        let _ = m.pedir(MOD, NOVO, "mc-2", B, GESTORA, &nova);
        m.aceitar(B, 20, MOD, "mc-2", &resumo_de(NOVO, "mc-2", &nova))
            .expect("B aceita no pacote novo");
        let _ = m.pedir(MOD, NOVO, "mc-3", A, GESTORA, &nova);
        let _ = m.pedir(MOD, HASH, "mc-4", B, GESTORA, &velha);

        let desligado = m.mod_desligado(MOD, Some(NOVO));
        assert_eq!(desligado.maquinas, vec![(A, "mc-1".to_owned(), Papel::Principal)]);
        assert_eq!(desligado.pendentes, vec![(B, "mc-4".to_owned(), Papel::Principal)]);
        assert!(desligado.restam_aceitas);
        assert_eq!(desligado.a_parar, vec![(A, vec!["mc".to_owned()])]);
        assert_eq!(
            m.quem_opera(MOD, "mc-2"),
            Some(B),
            "a máquina do pacote novo saiu"
        );
        assert_eq!(
            m.hashes(),
            vec![(MOD.to_owned(), NOVO.to_owned())],
            "o pendente do pacote novo saiu, ou um do velho ficou"
        );

        // Desligado de vez, tudo sai.
        let desligado = m.mod_desligado(MOD, None);
        assert_eq!(desligado.maquinas, vec![(B, "mc-2".to_owned(), Papel::Principal)]);
        assert_eq!(desligado.pendentes, vec![(A, "mc-3".to_owned(), Papel::Principal)]);
        assert!(!desligado.restam_aceitas);
        assert!(m.hashes().is_empty());
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

    /// **Quem hospeda volta e chama quem ainda não reconectou**: o pedido a
    /// quem não está é lembrado, e a chegada dela diz o lugar uma vez
    /// (`MaquinaVoltou`). Um lugar que alguém ocupou nesse meio não é dito.
    #[test]
    fn o_pedido_a_quem_nao_esta_volta_quando_ela_chega() {
        let mut m = Maquinas::default();
        m.lembrar_quem_nao_esta(A, MOD, "mc-1", Papel::Reserva);
        m.lembrar_quem_nao_esta(A, MOD, "mc-1", Papel::Reserva);
        m.lembrar_quem_nao_esta(A, MOD, "mc-2", Papel::Principal);
        let _ = m.chegou(A, 10);
        assert_eq!(
            m.voltou(A),
            vec![
                (MOD.to_owned(), "mc-1".to_owned(), Papel::Reserva),
                (MOD.to_owned(), "mc-2".to_owned(), Papel::Principal),
            ]
        );
        assert!(m.voltou(A).is_empty(), "cada lugar é dito uma vez");
    }

    /// **O livro não cresce sem fim** (revisão final, M7): o MOD desligado
    /// leva as vezes e os ausentes dele, e passado o teto de ausentes uma
    /// pessoa nova não é lembrada.
    #[test]
    fn o_livro_esquece_o_mod_desligado_e_tem_teto_de_ausentes() {
        let mut m = Maquinas::default();
        m.lembrar_quem_nao_esta(A, MOD, "mc-1", Papel::Principal);
        m.lembrar_quem_nao_esta(A, "x/outro", "mc-1", Papel::Principal);
        let _ = m.mudar_a_vez(&chave(MOD, "mc-1", Papel::Principal));
        let _ = m.mod_desligado(MOD, None);
        assert!(m.vezes.keys().all(|(mod_id, _, _)| mod_id != MOD));
        assert_eq!(
            m.ausentes.get(&A).cloned().unwrap_or_default(),
            vec![chave("x/outro", "mc-1", Papel::Principal)]
        );
        let _ = m.mod_desligado("x/outro", None);
        assert!(m.ausentes.is_empty());

        for n in 0..MAX_AUSENTES_LEMBRADOS as u64 + 10 {
            m.lembrar_quem_nao_esta(PersonId(10_000 + n), MOD, "mc-1", Papel::Principal);
        }
        assert_eq!(m.ausentes.len(), MAX_AUSENTES_LEMBRADOS);
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
        // O mesmo pedido chegou duas vezes à conexão 11 (o anúncio e o reenvio):
        // a segunda recusa dela não derruba nada.
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
            vec![(MOD.to_owned(), "mc-1".to_owned(), Papel::Principal, M::SemTela)]
        );
        // E depois de cair, outra recusa da mesma conexão não acha o que recusar
        // (o servidor não publica duas vezes).
        assert_eq!(m.recusar(A, 10, MOD, "mc-1", M::SemTela), Recusa::SemPedido);

        // `PeloDono` de qualquer conexão derruba na hora.
        let _ = m.pedir(MOD, HASH, "mc-1", A, GESTORA, &o);
        assert_eq!(
            m.recusar(A, 11, MOD, "mc-1", M::PeloDono),
            Recusa::Recusado(M::PeloDono, Papel::Principal)
        );
        assert_eq!(
            m.aceitar(A, 10, MOD, "mc-1", &resumo("mc-1", &o)),
            Err(MotivoDeAceiteInvalido::SemPedido)
        );
    }

    // ---- a reserva (API 8 parte 7) ----

    /// Aceita `quem` como `papel` de `mc-1`, pela conexão `conexao`.
    fn aceita_como(m: &mut Maquinas, papel: Papel, quem: PersonId, conexao: u64) {
        let o = oferta(25565);
        let pedido = m.pedir_como(MOD, HASH, "mc-1", papel, quem, GESTORA, &o);
        assert!(
            matches!(pedido, Pedido::Guardado { dispensada: None, .. }),
            "{pedido:?}"
        );
        let aceita = m
            .aceitar(quem, conexao, MOD, "mc-1", &resumo_como(papel, "mc-1", &o))
            .expect("aceita");
        assert_eq!(aceita.papel, papel);
    }

    /// **O pedido da reserva diz que é a reserva, e o aceite dela é o dela.**
    /// O papel entra no resumo: o «sim» à mesma oferta como principal não
    /// aceita o pedido de reserva — o app aceitaria sozinho um pelo outro.
    #[test]
    fn o_aceite_da_principal_nao_aceita_o_pedido_da_reserva() {
        let mut m = Maquinas::default();
        let o = oferta(25565);
        let pedido = m.pedir_como(MOD, HASH, "mc-1", Papel::Reserva, B, GESTORA, &o);
        let Pedido::Guardado { resumo_hex, .. } = pedido else {
            panic!("{pedido:?}");
        };
        assert_eq!(resumo_hex, resumo_como(Papel::Reserva, "mc-1", &o));
        assert_ne!(resumo_hex, resumo("mc-1", &o));
        assert!(
            m.aceitar(B, 20, MOD, "mc-1", &resumo("mc-1", &o)).is_err(),
            "o resumo da principal aceitou o pedido da reserva"
        );
        assert_eq!(
            m.aceitar(B, 20, MOD, "mc-1", &resumo_hex)
                .expect("aceita")
                .papel,
            Papel::Reserva
        );
    }

    /// **Duas máquinas aceitas para o mesmo servidor de jogo, e só a dona
    /// serve**: pedir a reserva não dispensa a principal, e passar a posse
    /// para os túneis da de antes sem aceite novo.
    #[test]
    fn a_principal_e_a_reserva_ficam_aceitas_e_so_a_dona_serve() {
        let mut m = Maquinas::default();
        aceita_como(&mut m, Papel::Principal, A, 10);
        aceita_como(&mut m, Papel::Reserva, B, 20);
        assert_eq!(m.papel_de(A, 10, MOD, "mc-1"), Some(Papel::Principal));
        assert_eq!(m.papel_de(B, 20, MOD, "mc-1"), Some(Papel::Reserva));
        assert_eq!(m.dona(MOD, "mc-1"), Papel::Principal);
        let tcp = ProtocoloDeTunel::Tcp;
        assert_eq!(m.conferir_tunel(A, 10, MOD, "mc", tcp, 25565), Ok(()));
        assert_eq!(
            m.conferir_tunel(B, 20, MOD, "mc", tcp, 25565),
            Err(TunelForaDoAceite::NaoEADona)
        );

        let mudou = m
            .passar_a_posse(MOD, "mc-1", Papel::Reserva)
            .expect("a posse mudou");
        assert_eq!(mudou.quem_era, Some(A));
        assert_eq!(mudou.a_parar, vec!["mc".to_owned()]);
        assert_eq!(
            m.conferir_tunel(A, 10, MOD, "mc", tcp, 25565),
            Err(TunelForaDoAceite::NaoEADona)
        );
        assert_eq!(m.conferir_tunel(B, 20, MOD, "mc", tcp, 25565), Ok(()));
        // As duas seguem aceitas: a posse não é um aceite.
        assert!(m.maquina_de(A, 10, MOD, "mc-1") && m.maquina_de(B, 20, MOD, "mc-1"));
        assert_eq!(m.passar_a_posse(MOD, "mc-1", Papel::Reserva), None);
    }

    #[test]
    fn a_mesma_pessoa_nao_e_as_duas() {
        let mut m = Maquinas::default();
        aceita_como(&mut m, Papel::Principal, A, 10);
        assert_eq!(
            m.pedir_como(MOD, HASH, "mc-1", Papel::Reserva, A, GESTORA, &oferta(25565)),
            Pedido::Recusado(Excesso::MesmaPessoaNasDuas)
        );
    }

    /// **O prazo da máquina que sumiu só vale se ela não voltou**: a vez do
    /// lugar muda a cada saída e a cada aceite.
    #[test]
    fn o_prazo_so_vale_com_o_lugar_vazio_e_na_mesma_vez() {
        let mut m = Maquinas::default();
        aceita_como(&mut m, Papel::Principal, A, 10);
        let saiu = m.saiu(A, 10);
        let [(_, _, papel, vez)] = saiu.as_slice() else {
            panic!("{saiu:?}");
        };
        assert_eq!(*papel, Papel::Principal);
        assert!(m.sumiu_de_vez(MOD, "mc-1", Papel::Principal, *vez));
        aceita_como(&mut m, Papel::Principal, A, 11);
        assert!(!m.sumiu_de_vez(MOD, "mc-1", Papel::Principal, *vez), "voltou, e o prazo valeu");
        let _ = m.saiu(A, 11);
        assert!(!m.sumiu_de_vez(MOD, "mc-1", Papel::Principal, *vez), "outra saída, a vez velha valeu");
    }
}
