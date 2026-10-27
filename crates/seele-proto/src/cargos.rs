//! CARGOS — os tipos e a regra dos cargos e permissões (SEELE 0.17, ADR 0064).
//!
//! # Por que a regra mora aqui, e não no servidor
//!
//! O servidor decide; ninguém mais. Mas a **mesma** regra tem de ser lida por
//! quem explica: a tela de cargos diz «você não pode dar Banir porque não tem
//! Banir», e uma explicação escrita à parte da decisão é uma explicação que um
//! dia discorda dela. Então a decisão é uma função pura sobre um [`Estado`], o
//! servidor monta o `Estado` do banco e aplica a [`Mudanca`] que ela devolve, e
//! o cliente usa a mesma função só para explicar ([`Estado::explicar`]).
//!
//! # A regra, numa tela
//!
//! ```text
//! efetivas(p):
//!   proprietário            → todas
//!   banido                  → nenhuma
//!   C = concede(@everyone) ∪ concede(cada cargo de p)      — a soma (Discord)
//!   R = restringe(@everyone) ∪ restringe(cada cargo de p)  — decisão SEELE
//!   Administrador ∈ C − R   → C = todas
//!   efetivas = C − R
//! ```
//!
//! A posição dos cargos **não** entra na soma: ela só delimita quem administra
//! quem e quais cargos ([`Estado::decidir`], [`Estado::pode_moderar`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::control::{check_bounds, ControlError, Permission, MAX_ID_DO_MOD_NOS_PEDIDOS};
use crate::ids::{PersonId, RoleId};

/// O id do `@everyone`: o cargo que toda pessoa tem sem ninguém atribuir.
///
/// Zero, como o do Discord é o id do próprio servidor: nenhum cargo criado
/// pode recebê-lo, porque a tabela começa em 1 e os ids novos vêm do maior já
/// usado.
pub const TODOS: RoleId = RoleId(0);

/// O nome do `@everyone`, que não se edita.
pub const NOME_DO_TODOS: &str = "@everyone";

/// Quantos cargos um servidor pode ter, fora o `@everyone`.
pub const MAX_CARGOS: usize = 64;

/// O nome de um cargo, em bytes.
pub const MAX_NOME_DO_CARGO: usize = 32;

/// Quantas entradas cabem em `concede` ou em `restringe`.
pub const MAX_PERMISSOES_NA_LISTA: usize = 64;

/// Quantos bytes tem, no máximo, a ação de uma permissão de MOD.
pub const MAX_ACAO_LEN: usize = 32;

/// O nome de tela de uma permissão de MOD, em bytes.
pub const MAX_NOME_DA_PERMISSAO_DE_MOD: usize = 48;

/// Quantas permissões um MOD pode declarar.
pub const MAX_PERMISSOES_POR_MOD: usize = 8;

/// Quantas permissões de MOD o quadro leva.
pub const MAX_PERMISSOES_DE_MOD_NO_QUADRO: usize = 64;

/// Quantas pessoas o quadro leva nas atribuições.
///
/// O teto de verdade é o do quadro de 16 KiB, conferido antes de gravar
/// ([`RecusaDeCargo::QuadroCheio`]); este é o do `validate`, para quem lê.
pub const MAX_ATRIBUICOES_NO_QUADRO: usize = 2048;

/// Todas as permissões do produto, na ordem do enum.
pub const PERMISSOES_DO_PRODUTO: [Permission; 15] = [
    Permission::ViewVoiceRoom,
    Permission::EnterVoiceRoom,
    Permission::Speak,
    Permission::ReadChannel,
    Permission::WriteChannel,
    Permission::RemoveMessage,
    Permission::MovePerson,
    Permission::Kick,
    Permission::Ban,
    Permission::ManageVoiceRooms,
    Permission::ManageRoles,
    Permission::AdministerServer,
    Permission::AttachFile,
    Permission::ShareScreen,
    Permission::Administrator,
];

/// As treze permissões que existiam até a 0.16.
///
/// É contra elas que a migração prova que ninguém ganhou nada: depois dela, o
/// conjunto efetivo de cada combinação de papéis, nestas treze, é o mesmo.
pub const LEGADAS: [Permission; 13] = [
    Permission::ViewVoiceRoom,
    Permission::EnterVoiceRoom,
    Permission::Speak,
    Permission::ReadChannel,
    Permission::WriteChannel,
    Permission::RemoveMessage,
    Permission::MovePerson,
    Permission::Kick,
    Permission::Ban,
    Permission::ManageVoiceRooms,
    Permission::ManageRoles,
    Permission::AdministerServer,
    Permission::AttachFile,
];

/// O nome com que uma permissão do produto é gravada no banco.
///
/// Escrito à mão, e não derivado do `Debug`, para que renomear uma variante não
/// mude em silêncio o que um banco existente quer dizer — a lição das migrações
/// 5 a 9.
#[must_use]
pub const fn nome_gravado(permissao: Permission) -> &'static str {
    match permissao {
        Permission::ViewVoiceRoom => "ViewVoiceRoom",
        Permission::EnterVoiceRoom => "EnterVoiceRoom",
        Permission::Speak => "Speak",
        Permission::ReadChannel => "ReadChannel",
        Permission::WriteChannel => "WriteChannel",
        Permission::RemoveMessage => "RemoveMessage",
        Permission::MovePerson => "MovePerson",
        Permission::Kick => "Kick",
        Permission::Ban => "Ban",
        Permission::ManageVoiceRooms => "ManageVoiceRooms",
        Permission::ManageRoles => "ManageRoles",
        Permission::AdministerServer => "AdministerServer",
        Permission::AttachFile => "AttachFile",
        Permission::ShareScreen => "ShareScreen",
        Permission::Administrator => "Administrator",
    }
}

/// A permissão do produto gravada com este nome. `None` para um nome que esta
/// build não conhece — e um nome desconhecido **nunca concede**.
#[must_use]
pub fn do_nome_gravado(nome: &str) -> Option<Permission> {
    PERMISSOES_DO_PRODUTO
        .iter()
        .copied()
        .find(|permissao| nome_gravado(*permissao) == nome)
}

/// A chave de uma permissão de MOD: **qual MOD** e **qual ação ele declarou**.
///
/// Estruturada, e nunca um nome livre: uma ação só é autorizável se o MOD
/// `mod_id` a declarou no `mod.json` dele, e a chave de um MOD não alcança a de
/// outro.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChaveDeMod {
    /// `autor/nome`, como o manifesto e o anúncio o dizem.
    pub mod_id: String,
    /// A ação, como o MOD a declarou: `[a-z0-9-]`, até [`MAX_ACAO_LEN`].
    pub acao: String,
}

impl ChaveDeMod {
    /// Se a chave tem a forma que o produto aceita.
    #[must_use]
    pub fn e_valida(&self) -> bool {
        id_de_mod_valido(&self.mod_id) && acao_valida(&self.acao)
    }

    /// Como a chave é gravada numa lista do banco: `mod:autor/nome#acao`.
    #[must_use]
    pub fn gravada(&self) -> String {
        format!("mod:{}#{}", self.mod_id, self.acao)
    }
}

/// Se `id` é um `autor/nome` que pode viajar numa chave.
///
/// Uma barra só, nenhum dos dois lados vazio, sem espaço, sem caractere de
/// controle e sem `#`, que é o separador da forma gravada.
#[must_use]
pub fn id_de_mod_valido(id: &str) -> bool {
    let Some((autor, nome)) = id.split_once('/') else {
        return false;
    };
    !autor.is_empty()
        && !nome.is_empty()
        && !nome.contains('/')
        && id.len() <= MAX_ID_DO_MOD_NOS_PEDIDOS
        && !id
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '#')
}

/// Se `acao` é uma ação de MOD aceitável: `[a-z0-9-]`, de 1 a
/// [`MAX_ACAO_LEN`], sem `-` nas pontas.
#[must_use]
pub fn acao_valida(acao: &str) -> bool {
    !acao.is_empty()
        && acao.len() <= MAX_ACAO_LEN
        && !acao.starts_with('-')
        && !acao.ends_with('-')
        && acao
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Uma permissão: do produto, ou de um MOD.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permissao {
    /// Uma das [`PERMISSOES_DO_PRODUTO`].
    Produto(Permission),
    /// Uma ação que um MOD declarou.
    DeMod(ChaveDeMod),
}

impl Permissao {
    /// Como esta permissão é gravada numa lista JSON do banco.
    #[must_use]
    pub fn gravada(&self) -> String {
        match self {
            Self::Produto(permissao) => nome_gravado(*permissao).to_owned(),
            Self::DeMod(chave) => chave.gravada(),
        }
    }

    /// Lê uma entrada gravada. `None` para o que esta build não reconhece: o
    /// chamador descarta, e descartar é não conceder.
    #[must_use]
    pub fn da_gravada(texto: &str) -> Option<Self> {
        if let Some(resto) = texto.strip_prefix("mod:") {
            let (mod_id, acao) = resto.rsplit_once('#')?;
            let chave = ChaveDeMod {
                mod_id: mod_id.to_owned(),
                acao: acao.to_owned(),
            };
            return chave.e_valida().then_some(Self::DeMod(chave));
        }
        do_nome_gravado(texto).map(Self::Produto)
    }

    fn validar(&self) -> Result<(), ControlError> {
        match self {
            Self::Produto(_) => Ok(()),
            Self::DeMod(chave) if chave.e_valida() => Ok(()),
            Self::DeMod(_) => Err(ControlError::FieldOutOfRange {
                field: "permissao_de_mod",
            }),
        }
    }
}

/// Uma permissão como um MOD a declara no `mod.json` (campo `permissoes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissaoDeclarada {
    /// A ação: `[a-z0-9-]`, até [`MAX_ACAO_LEN`].
    pub acao: String,
    /// O nome que a tela de cargos mostra.
    pub nome: String,
    /// Se, na primeira vez que o MOD é ligado neste servidor, a permissão vai
    /// para o `@everyone`. Falsa sem a chave: só o proprietário e quem tem
    /// Administrador a têm até alguém concedê-la.
    #[serde(default)]
    pub todos: bool,
}

/// Por que uma lista de permissões declaradas foi recusada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecusaDaDeclaracao {
    /// Mais de [`MAX_PERMISSOES_POR_MOD`].
    #[error("o MOD declara {quantas} permissões, e o teto é {MAX_PERMISSOES_POR_MOD}")]
    Demais {
        /// Quantas vieram.
        quantas: usize,
    },
    /// Uma ação fora de `[a-z0-9-]` ou do tamanho.
    #[error("a permissão {posicao} tem uma ação inválida")]
    AcaoInvalida {
        /// Qual, contando do zero.
        posicao: usize,
    },
    /// Um nome vazio ou longo demais.
    #[error("a permissão {posicao} tem um nome inválido")]
    NomeInvalido {
        /// Qual, contando do zero.
        posicao: usize,
    },
    /// A mesma ação duas vezes.
    #[error("a permissão {posicao} repete uma ação")]
    Repetida {
        /// Qual, contando do zero.
        posicao: usize,
    },
}

/// Confere a lista `permissoes` de um manifesto.
///
/// # Errors
///
/// A primeira [`RecusaDaDeclaracao`] encontrada.
pub fn validar_declaradas(lista: &[PermissaoDeclarada]) -> Result<(), RecusaDaDeclaracao> {
    if lista.len() > MAX_PERMISSOES_POR_MOD {
        return Err(RecusaDaDeclaracao::Demais {
            quantas: lista.len(),
        });
    }
    let mut vistas = BTreeSet::new();
    for (posicao, declarada) in lista.iter().enumerate() {
        if !acao_valida(&declarada.acao) {
            return Err(RecusaDaDeclaracao::AcaoInvalida { posicao });
        }
        if !nome_de_permissao_valido(&declarada.nome) {
            return Err(RecusaDaDeclaracao::NomeInvalido { posicao });
        }
        if !vistas.insert(declarada.acao.as_str()) {
            return Err(RecusaDaDeclaracao::Repetida { posicao });
        }
    }
    Ok(())
}

fn nome_de_permissao_valido(nome: &str) -> bool {
    !nome.trim().is_empty()
        && nome.len() <= MAX_NOME_DA_PERMISSAO_DE_MOD
        && !nome.chars().any(char::is_control)
}

/// Um cargo, como o fio e a tela o veem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cargo {
    /// O id, deste servidor e de nenhum outro. [`TODOS`] é o `@everyone`.
    pub id: RoleId,
    /// O nome.
    pub nome: String,
    /// A posição: 0 é o `@everyone`; os outros vão de 1 em diante, sem empate,
    /// e quanto maior, mais alto.
    pub posicao: u32,
    /// O que o cargo concede. Soma com os outros cargos da pessoa.
    pub concede: Vec<Permissao>,
    /// O que o cargo restringe. Uma restrição em qualquer cargo vence uma
    /// concessão em qualquer outro — menos para o proprietário.
    pub restringe: Vec<Permissao>,
}

impl Cargo {
    /// Se é o `@everyone`.
    #[must_use]
    pub fn e_todos(&self) -> bool {
        self.id == TODOS
    }

    fn validar(&self) -> Result<(), ControlError> {
        check_bounds(
            "nome_do_cargo",
            self.nome.len(),
            MAX_NOME_DO_CARGO.max(NOME_DO_TODOS.len()),
        )?;
        validar_lista(&self.concede)?;
        validar_lista(&self.restringe)
    }
}

fn validar_lista(lista: &[Permissao]) -> Result<(), ControlError> {
    check_bounds("permissoes", lista.len(), MAX_PERMISSOES_NA_LISTA)?;
    lista.iter().try_for_each(Permissao::validar)
}

/// Os cargos de uma pessoa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Atribuicao {
    /// A pessoa.
    pub pessoa: PersonId,
    /// Os cargos atribuídos a ela, fora o `@everyone`, que todos têm.
    pub cargos: Vec<RoleId>,
}

/// Uma permissão de MOD que o quadro apresenta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissaoDeModNoQuadro {
    /// A chave.
    pub chave: ChaveDeMod,
    /// O nome que o MOD declarou.
    pub nome: String,
}

/// Tudo o que a tela de cargos precisa para desenhar um servidor.
///
/// Vai em resposta ao `PedirCargos`, logo depois do `Session` e a todos, a
/// cada mudança. Não é segredo: no Discord também, quem está no servidor vê os
/// cargos de quem está nele.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuadroDeCargos {
    /// Os cargos, do mais alto para o mais baixo. O `@everyone` é o último.
    pub cargos: Vec<Cargo>,
    /// Quem tem qual cargo. Só pessoas com ao menos um cargo além do
    /// `@everyone`.
    pub atribuicoes: Vec<Atribuicao>,
    /// O proprietário, se ele já tem conta neste servidor.
    pub proprietario: Option<PersonId>,
    /// As permissões que os MODs ligados declararam.
    pub de_mods: Vec<PermissaoDeModNoQuadro>,
    /// A revisão do conjunto de cargos. A tela a manda de volta em todo verbo
    /// de gestão.
    pub revisao: u64,
    /// Se a máquina que hospeda declarou um proprietário. Falso quer dizer que
    /// este servidor não tem dono: a tela diz «este servidor não tem
    /// proprietário declarado», e quem hospeda resolve pela máquina.
    pub dono_declarado: bool,
}

impl QuadroDeCargos {
    /// Confere os tetos.
    ///
    /// # Errors
    ///
    /// O primeiro campo fora do teto.
    pub fn validar(&self) -> Result<(), ControlError> {
        check_bounds("cargos", self.cargos.len(), MAX_CARGOS + 1)?;
        self.cargos.iter().try_for_each(Cargo::validar)?;
        check_bounds(
            "atribuicoes",
            self.atribuicoes.len(),
            MAX_ATRIBUICOES_NO_QUADRO,
        )?;
        for atribuicao in &self.atribuicoes {
            check_bounds("cargos_da_pessoa", atribuicao.cargos.len(), MAX_CARGOS)?;
        }
        check_bounds(
            "de_mods",
            self.de_mods.len(),
            MAX_PERMISSOES_DE_MOD_NO_QUADRO,
        )?;
        for de_mod in &self.de_mods {
            if !de_mod.chave.e_valida() {
                return Err(ControlError::FieldOutOfRange {
                    field: "permissao_de_mod",
                });
            }
            check_bounds(
                "nome_da_permissao_de_mod",
                de_mod.nome.len(),
                MAX_NOME_DA_PERMISSAO_DE_MOD,
            )?;
        }
        Ok(())
    }
}

/// A altura de uma pessoa na hierarquia: a posição do cargo mais alto dela, ou
/// o proprietário, que fica acima de tudo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Altura {
    /// A posição do cargo mais alto; 0 para quem só tem o `@everyone`.
    Cargo(u32),
    /// O proprietário.
    Proprietario,
}

/// O que **esta** pessoa pode, como o servidor resolveu agora.
///
/// Conveniência para a tela esconder e explicar. O servidor confere de novo
/// em toda ação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuasPermissoes {
    /// As permissões do produto.
    pub permissoes: Vec<Permission>,
    /// As permissões de MOD.
    pub de_mods: Vec<ChaveDeMod>,
    /// Se é o proprietário.
    pub proprietario: bool,
    /// A altura na hierarquia.
    pub altura: Altura,
}

impl SuasPermissoes {
    /// Confere os tetos.
    ///
    /// # Errors
    ///
    /// O primeiro campo fora do teto.
    pub fn validar(&self) -> Result<(), ControlError> {
        check_bounds(
            "permissoes",
            self.permissoes.len(),
            PERMISSOES_DO_PRODUTO.len(),
        )?;
        check_bounds(
            "de_mods",
            self.de_mods.len(),
            MAX_PERMISSOES_DE_MOD_NO_QUADRO,
        )?;
        if self.de_mods.iter().all(ChaveDeMod::e_valida) {
            Ok(())
        } else {
            Err(ControlError::FieldOutOfRange {
                field: "permissao_de_mod",
            })
        }
    }
}

/// Por que um verbo de cargo foi recusado. Uma variante por motivo, para a
/// casca dizer a frase certa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum RecusaDeCargo {
    /// Quem pediu não tem Gerenciar cargos.
    #[error("sem a permissão Gerenciar cargos")]
    SemPermissao,
    /// O cargo está na posição de quem pediu, ou acima.
    #[error("o cargo não está abaixo do seu cargo mais alto")]
    CargoAcimaDeVoce,
    /// A pessoa-alvo está na altura de quem pediu, ou acima.
    #[error("a pessoa não está abaixo de você")]
    PessoaAcimaDeVoce,
    /// A pessoa-alvo é o proprietário.
    #[error("ninguém mexe nos cargos do proprietário")]
    Proprietario,
    /// Conceder, ou atribuir um cargo que concede, algo que quem pediu não tem.
    #[error("você não pode conceder o que não tem")]
    NaoPodeConceder {
        /// A primeira permissão que faltou.
        permissao: Permissao,
    },
    /// A mudança mexeria no que pode alguém que está na sua altura ou acima.
    #[error("a mudança atingiria quem não está abaixo de você")]
    AtingeQuemEstaAcima {
        /// A primeira pessoa atingida.
        pessoa: PersonId,
    },
    /// A mudança daria a alguém (inclusive a você) uma permissão que você não
    /// tem efetivamente — por exemplo, tirando uma restrição.
    #[error("a mudança daria uma permissão que você não tem")]
    GanhoAlemDaSuaAutoridade {
        /// Quem ganharia.
        pessoa: PersonId,
        /// O quê.
        permissao: Permissao,
    },
    /// Uma permissão que nenhum MOD ligado declarou.
    #[error("permissão desconhecida")]
    PermissaoDesconhecida {
        /// Qual.
        permissao: Permissao,
    },
    /// Não há cargo com esse id.
    #[error("o cargo não existe")]
    CargoNaoExiste,
    /// Não há conta com esse id.
    #[error("a pessoa não existe")]
    PessoaNaoExiste,
    /// O `@everyone` não se apaga, não se move e não se atribui.
    #[error("o @everyone não se apaga, não se move e não se atribui")]
    CargoProtegido,
    /// Nome vazio, longo demais ou com caractere de controle.
    #[error("nome de cargo inválido")]
    NomeInvalido,
    /// Outro cargo já tem esse nome.
    #[error("já existe um cargo com esse nome")]
    NomeTomado,
    /// Já há [`MAX_CARGOS`] cargos.
    #[error("o servidor já tem cargos demais")]
    LimiteDeCargos,
    /// A posição pedida não existe.
    #[error("posição inválida")]
    PosicaoInvalida,
    /// Depois da mudança, o quadro não caberia num quadro de controle.
    #[error("o quadro de cargos ficaria grande demais")]
    QuadroCheio,
    /// O banco não respondeu.
    #[error("falha do servidor")]
    FalhaDoServidor,
    /// Outro gestor mudou os cargos depois que esta tela os leu. Nada foi
    /// gravado: recarregue (o `QuadroDeCargos` de agora já foi mandado) e veja
    /// o que mudou.
    #[error("outro gestor mudou os cargos; recarregue")]
    RevisaoVelha {
        /// A revisão de agora.
        atual: u64,
    },
}

/// Por que uma ação de MOD foi recusada antes do despacho.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum RecusaDeAcaoDeMod {
    /// Nenhum MOD ligado declarou essa ação.
    #[error("ação de MOD não declarada")]
    NaoDeclarada,
    /// A pessoa não tem a permissão da ação, agora.
    #[error("sem a permissão desta ação de MOD")]
    SemPermissao,
}

impl RecusaDeAcaoDeMod {
    /// O nome que vai no JSON da resposta (`{"ok":false,"recusa":…}`).
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::NaoDeclarada => "acao-nao-declarada",
            Self::SemPermissao => "sem-permissao",
        }
    }
}

/// Um verbo de gestão de cargos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedidoDeCargo {
    /// Criar um cargo, na posição 1.
    Criar {
        /// O nome.
        nome: String,
        /// O que concede.
        concede: Vec<Permissao>,
        /// O que restringe.
        restringe: Vec<Permissao>,
    },
    /// Trocar o nome e as listas de um cargo.
    Editar {
        /// Qual.
        cargo: RoleId,
        /// O nome novo (ignorado no `@everyone`).
        nome: String,
        /// O que passa a conceder.
        concede: Vec<Permissao>,
        /// O que passa a restringir.
        restringe: Vec<Permissao>,
    },
    /// Pôr um cargo noutra posição.
    Mover {
        /// Qual.
        cargo: RoleId,
        /// A posição nova, de 1 em diante.
        posicao: u32,
    },
    /// Apagar um cargo e todas as atribuições dele.
    Apagar {
        /// Qual.
        cargo: RoleId,
    },
    /// Dar um cargo a uma pessoa.
    Atribuir {
        /// Quem.
        pessoa: PersonId,
        /// Qual.
        cargo: RoleId,
    },
    /// Tirar um cargo de uma pessoa.
    Revogar {
        /// Quem.
        pessoa: PersonId,
        /// Qual.
        cargo: RoleId,
    },
}

/// O que uma decisão aceita manda gravar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mudanca {
    /// Um cargo novo. As posições dos outros já vêm em `posicoes`.
    Criou {
        /// O cargo.
        cargo: Cargo,
        /// A posição de cada cargo depois da mudança.
        posicoes: Vec<(RoleId, u32)>,
    },
    /// Um cargo com nome ou listas novas.
    Editou(Cargo),
    /// As posições de todos os cargos depois de mover um.
    Reordenou(Vec<(RoleId, u32)>),
    /// Um cargo apagado, com as posições dos que sobraram.
    Apagou {
        /// Qual.
        cargo: RoleId,
        /// A posição de cada cargo que sobrou.
        posicoes: Vec<(RoleId, u32)>,
    },
    /// Uma atribuição nova.
    Atribuiu {
        /// Quem.
        pessoa: PersonId,
        /// Qual.
        cargo: RoleId,
    },
    /// Uma atribuição a menos.
    Revogou {
        /// Quem.
        pessoa: PersonId,
        /// Qual.
        cargo: RoleId,
    },
}

/// As permissões efetivas de uma pessoa, resolvidas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Efetivas {
    /// As do produto.
    pub produto: BTreeSet<Permission>,
    /// As de MOD.
    pub de_mods: BTreeSet<ChaveDeMod>,
    /// Se é o proprietário.
    pub proprietario: bool,
    /// A altura na hierarquia.
    pub altura: Altura,
}

impl Efetivas {
    /// Ninguém, com nada.
    #[must_use]
    pub fn nenhuma() -> Self {
        Self {
            produto: BTreeSet::new(),
            de_mods: BTreeSet::new(),
            proprietario: false,
            altura: Altura::Cargo(0),
        }
    }

    /// Se tem esta permissão do produto.
    #[must_use]
    pub fn pode(&self, permissao: Permission) -> bool {
        self.produto.contains(&permissao)
    }

    /// Se tem esta permissão, do produto ou de MOD.
    #[must_use]
    pub fn tem(&self, permissao: &Permissao) -> bool {
        match permissao {
            Permissao::Produto(permissao) => self.pode(*permissao),
            Permissao::DeMod(chave) => self.de_mods.contains(chave),
        }
    }

    /// Se administra: Gerenciar servidor ou Administrador, efetivos.
    #[must_use]
    pub fn administra(&self) -> bool {
        self.pode(Permission::AdministerServer) || self.pode(Permission::Administrator)
    }

    /// As ações de um MOD que esta pessoa tem.
    #[must_use]
    pub fn acoes_do_mod(&self, mod_id: &str) -> Vec<String> {
        self.de_mods
            .iter()
            .filter(|chave| chave.mod_id == mod_id)
            .map(|chave| chave.acao.clone())
            .collect()
    }

    /// Como vai no fio.
    #[must_use]
    pub fn suas(&self) -> SuasPermissoes {
        SuasPermissoes {
            permissoes: self.produto.iter().copied().collect(),
            de_mods: self.de_mods.iter().cloned().collect(),
            proprietario: self.proprietario,
            altura: self.altura,
        }
    }
}

/// Por que uma pessoa tem, ou não tem, uma permissão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Motivo {
    /// É o proprietário.
    Proprietario,
    /// Está banida.
    Banida,
    /// Uma permissão de MOD que nenhum MOD ligado declarou.
    Desconhecida,
    /// Restringida por estes cargos.
    Restringida {
        /// Quais.
        por: Vec<RoleId>,
    },
    /// Concedida por estes cargos.
    Concedida {
        /// Quais.
        por: Vec<RoleId>,
    },
    /// Concedida pelo Administrador destes cargos.
    PeloAdministrador {
        /// Quais.
        por: Vec<RoleId>,
    },
    /// Nenhum cargo dela concede.
    NinguemConcede,
}

/// A resposta de [`Estado::explicar`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explicacao {
    /// Se tem.
    pub tem: bool,
    /// Por quê.
    pub motivo: Motivo,
}

/// Uma linha de [`Estado::explicar_tudo`]: a permissão, se a pessoa a tem, e
/// por quê.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissaoExplicada {
    /// Qual.
    pub permissao: Permissao,
    /// Se tem.
    pub tem: bool,
    /// Por quê.
    pub motivo: Motivo,
}

/// O que um verbo mudaria no efetivo de UMA pessoa ([`Estado::prever`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EfeitoNaPessoa {
    /// Quem.
    pub pessoa: PersonId,
    /// O que ela passaria a poder.
    pub ganha: Vec<Permissao>,
    /// O que ela deixaria de poder.
    pub perde: Vec<Permissao>,
    /// Do que ela ganharia, o que o autor NÃO tem efetivamente — o que o guarda
    /// do ganho efetivo recusa. Vazio num verbo que o servidor aceitaria.
    pub alem_do_autor: Vec<Permissao>,
}

/// Tudo o que a regra precisa saber de um servidor.
///
/// O servidor monta do banco; o cliente monta do [`QuadroDeCargos`]
/// ([`Estado::do_quadro`]) para explicar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Estado {
    /// Todos os cargos, inclusive o `@everyone`.
    pub cargos: BTreeMap<RoleId, Cargo>,
    /// Os cargos atribuídos a cada pessoa, fora o `@everyone`.
    pub atribuicoes: BTreeMap<PersonId, BTreeSet<RoleId>>,
    /// As contas que existem.
    pub pessoas: BTreeSet<PersonId>,
    /// As contas banidas agora.
    pub banidos: BTreeSet<PersonId>,
    /// O proprietário, se tem conta.
    pub proprietario: Option<PersonId>,
    /// As permissões que os MODs ligados declararam, com o nome de tela.
    pub declaradas: BTreeMap<ChaveDeMod, String>,
    /// Se a máquina que hospeda declarou um proprietário (mesmo que ele ainda
    /// não tenha conta).
    pub dono_declarado: bool,
    /// O maior id de cargo já usado neste servidor. Um id apagado nunca volta.
    pub maior_id: u32,
    /// A revisão do conjunto de cargos: sobe a cada mudança aceita. Todo verbo
    /// de gestão diz a revisão que viu, e uma velha é recusada
    /// ([`RecusaDeCargo::RevisaoVelha`]) em vez de sobrescrever a mudança de
    /// outro gestor.
    pub revisao: u64,
}

impl Estado {
    /// O estado que um quadro descreve, para o cliente explicar.
    ///
    /// Sem banidos: o quadro não os leva, e quem está banido não está
    /// conectado para ser explicado.
    #[must_use]
    pub fn do_quadro(quadro: &QuadroDeCargos) -> Self {
        let cargos: BTreeMap<RoleId, Cargo> = quadro
            .cargos
            .iter()
            .map(|cargo| (cargo.id, cargo.clone()))
            .collect();
        let atribuicoes: BTreeMap<PersonId, BTreeSet<RoleId>> = quadro
            .atribuicoes
            .iter()
            .map(|atribuicao| {
                (
                    atribuicao.pessoa,
                    atribuicao.cargos.iter().copied().collect(),
                )
            })
            .collect();
        let mut pessoas: BTreeSet<PersonId> = atribuicoes.keys().copied().collect();
        pessoas.extend(quadro.proprietario);
        let maior_id = cargos.keys().map(|id| id.get()).max().unwrap_or(0);
        Self {
            cargos,
            atribuicoes,
            pessoas,
            banidos: BTreeSet::new(),
            proprietario: quadro.proprietario,
            declaradas: quadro
                .de_mods
                .iter()
                .map(|de_mod| (de_mod.chave.clone(), de_mod.nome.clone()))
                .collect(),
            dono_declarado: quadro.dono_declarado,
            maior_id,
            revisao: quadro.revisao,
        }
    }

    /// O mesmo estado, sabendo de mais contas — as que o quadro não lista
    /// porque só têm o `@everyone` (o FORA DO AR do cliente). Sem isto, a
    /// prévia de uma edição do `@everyone` não veria quem não tem cargo.
    #[must_use]
    pub fn com_pessoas(mut self, pessoas: impl IntoIterator<Item = PersonId>) -> Self {
        self.pessoas.extend(pessoas);
        self
    }

    /// Por que esta pessoa tem, ou não, CADA permissão: as do produto e as de
    /// MOD declaradas, nessa ordem.
    #[must_use]
    pub fn explicar_tudo(&self, pessoa: PersonId) -> Vec<PermissaoExplicada> {
        PERMISSOES_DO_PRODUTO
            .iter()
            .copied()
            .map(Permissao::Produto)
            .chain(self.declaradas.keys().cloned().map(Permissao::DeMod))
            .map(|permissao| {
                let Explicacao { tem, motivo } = self.explicar(pessoa, &permissao);
                PermissaoExplicada {
                    permissao,
                    tem,
                    motivo,
                }
            })
            .collect()
    }

    /// A prévia de um verbo: a recusa que o servidor daria, ou o que cada
    /// pessoa atingida ganharia e perderia. A MESMA conta do guarda do ganho
    /// efetivo, para a prévia nunca discordar da recusa — e só informativa: o
    /// servidor revalida tudo sobre o estado de agora.
    ///
    /// # Errors
    ///
    /// A [`RecusaDeCargo`] que [`Self::decidir`] daria.
    pub fn prever(
        &self,
        autor: PersonId,
        revisao: u64,
        pedido: &PedidoDeCargo,
    ) -> Result<Vec<EfeitoNaPessoa>, RecusaDeCargo> {
        let mudanca = self.decidir(autor, revisao, pedido)?;
        let mut depois = self.clone();
        depois.aplicar(&mudanca);
        let do_autor = self.efetivas(autor);
        let lista = |efetivas: &Efetivas| -> BTreeSet<Permissao> {
            efetivas
                .produto
                .iter()
                .copied()
                .map(Permissao::Produto)
                .chain(efetivas.de_mods.iter().cloned().map(Permissao::DeMod))
                .collect()
        };
        Ok(self
            .pessoas
            .iter()
            .filter_map(|pessoa| {
                let antes = lista(&self.efetivas(*pessoa));
                let agora = lista(&depois.efetivas(*pessoa));
                let ganha: Vec<Permissao> = agora.difference(&antes).cloned().collect();
                let perde: Vec<Permissao> = antes.difference(&agora).cloned().collect();
                if ganha.is_empty() && perde.is_empty() {
                    return None;
                }
                let alem_do_autor = if do_autor.proprietario {
                    Vec::new()
                } else {
                    ganha.iter().filter(|p| !do_autor.tem(p)).cloned().collect()
                };
                Some(EfeitoNaPessoa {
                    pessoa: *pessoa,
                    ganha,
                    perde,
                    alem_do_autor,
                })
            })
            .collect())
    }

    /// Os cargos que valem para a pessoa: o `@everyone` (se existe) e os
    /// atribuídos que existem.
    fn cargos_de(&self, pessoa: PersonId) -> Vec<&Cargo> {
        let mut cargos: Vec<&Cargo> = self.cargos.get(&TODOS).into_iter().collect();
        if let Some(atribuidos) = self.atribuicoes.get(&pessoa) {
            cargos.extend(
                atribuidos
                    .iter()
                    .filter(|id| **id != TODOS)
                    .filter_map(|id| self.cargos.get(id)),
            );
        }
        cargos
    }

    /// A altura de uma pessoa.
    #[must_use]
    pub fn altura(&self, pessoa: PersonId) -> Altura {
        if self.proprietario == Some(pessoa) {
            return Altura::Proprietario;
        }
        Altura::Cargo(
            self.cargos_de(pessoa)
                .iter()
                .map(|cargo| cargo.posicao)
                .max()
                .unwrap_or(0),
        )
    }

    fn todas(&self) -> (BTreeSet<Permission>, BTreeSet<ChaveDeMod>) {
        (
            PERMISSOES_DO_PRODUTO.iter().copied().collect(),
            self.declaradas.keys().cloned().collect(),
        )
    }

    /// As permissões efetivas de uma pessoa. A regra inteira está no
    /// cabeçalho deste módulo.
    #[must_use]
    pub fn efetivas(&self, pessoa: PersonId) -> Efetivas {
        let altura = self.altura(pessoa);
        if self.proprietario == Some(pessoa) {
            let (produto, de_mods) = self.todas();
            return Efetivas {
                produto,
                de_mods,
                proprietario: true,
                altura,
            };
        }
        if self.banidos.contains(&pessoa) || !self.pessoas.contains(&pessoa) {
            return Efetivas {
                altura,
                ..Efetivas::nenhuma()
            };
        }
        let cargos = self.cargos_de(pessoa);
        let concedidas: BTreeSet<&Permissao> = cargos
            .iter()
            .flat_map(|cargo| cargo.concede.iter())
            .collect();
        let restritas: BTreeSet<&Permissao> = cargos
            .iter()
            .flat_map(|cargo| cargo.restringe.iter())
            .collect();
        let administrador = Permissao::Produto(Permission::Administrator);
        let (mut produto, mut de_mods) =
            if concedidas.contains(&administrador) && !restritas.contains(&administrador) {
                self.todas()
            } else {
                let produto = concedidas
                    .iter()
                    .filter_map(|permissao| match permissao {
                        Permissao::Produto(permissao) => Some(*permissao),
                        Permissao::DeMod(_) => None,
                    })
                    .collect();
                let de_mods = concedidas
                    .iter()
                    .filter_map(|permissao| match permissao {
                        Permissao::DeMod(chave) if self.declaradas.contains_key(chave) => {
                            Some(chave.clone())
                        }
                        _ => None,
                    })
                    .collect();
                (produto, de_mods)
            };
        for restrita in restritas {
            match restrita {
                Permissao::Produto(permissao) => {
                    produto.remove(permissao);
                }
                Permissao::DeMod(chave) => {
                    de_mods.remove(chave);
                }
            }
        }
        Efetivas {
            produto,
            de_mods,
            proprietario: false,
            altura,
        }
    }

    /// Por que esta pessoa tem, ou não tem, esta permissão. A resposta é a da
    /// mesma regra de [`Self::efetivas`], e um teste prende as duas juntas.
    #[must_use]
    pub fn explicar(&self, pessoa: PersonId, permissao: &Permissao) -> Explicacao {
        let tem = self.efetivas(pessoa).tem(permissao);
        let motivo = if self.proprietario == Some(pessoa) {
            Motivo::Proprietario
        } else if self.banidos.contains(&pessoa) {
            Motivo::Banida
        } else if matches!(permissao, Permissao::DeMod(chave) if !self.declaradas.contains_key(chave))
        {
            Motivo::Desconhecida
        } else {
            let cargos = self.cargos_de(pessoa);
            let ids = |filtro: &dyn Fn(&Cargo) -> bool| -> Vec<RoleId> {
                cargos
                    .iter()
                    .filter(|cargo| filtro(cargo))
                    .map(|cargo| cargo.id)
                    .collect()
            };
            let restringem = ids(&|cargo| cargo.restringe.contains(permissao));
            let concedem = ids(&|cargo| cargo.concede.contains(permissao));
            let administrador = Permissao::Produto(Permission::Administrator);
            let administram = ids(&|cargo| cargo.concede.contains(&administrador));
            let admin_restrito = cargos
                .iter()
                .any(|cargo| cargo.restringe.contains(&administrador));
            if !restringem.is_empty() {
                Motivo::Restringida { por: restringem }
            } else if !concedem.is_empty() {
                Motivo::Concedida { por: concedem }
            } else if !administram.is_empty() && !admin_restrito {
                Motivo::PeloAdministrador { por: administram }
            } else {
                Motivo::NinguemConcede
            }
        };
        Explicacao { tem, motivo }
    }

    /// Se `autor` pode usar `permissao` (expulsar, banir, mover, remover
    /// mensagem alheia) contra `alvo`.
    ///
    /// - [Discord] o alvo está abaixo do autor;
    /// - [SEELE] o alvo nunca é o proprietário, e quem não administra não age
    ///   sobre quem administra (a regra que já existia no `moderavel`);
    /// - sobre si mesmo, basta a permissão.
    #[must_use]
    pub fn pode_moderar(&self, autor: PersonId, alvo: PersonId, permissao: Permission) -> bool {
        let quem = self.efetivas(autor);
        if !quem.pode(permissao) {
            return false;
        }
        if autor == alvo {
            return true;
        }
        if self.proprietario == Some(alvo) {
            return false;
        }
        if quem.proprietario {
            return true;
        }
        let contra = self.efetivas(alvo);
        if contra.administra() && !quem.administra() {
            return false;
        }
        quem.altura > contra.altura
    }

    /// Os cargos fora o `@everyone`, do mais baixo para o mais alto.
    fn em_ordem(&self) -> Vec<&Cargo> {
        let mut cargos: Vec<&Cargo> = self
            .cargos
            .values()
            .filter(|cargo| !cargo.e_todos())
            .collect();
        cargos.sort_by_key(|cargo| (cargo.posicao, cargo.id));
        cargos
    }

    /// As posições 1..N depois de pôr `ordem` em sequência.
    fn numerar(ordem: &[RoleId]) -> Vec<(RoleId, u32)> {
        ordem
            .iter()
            .zip(1_u32..)
            .map(|(id, posicao)| (*id, posicao))
            .collect()
    }

    /// Decide um verbo de cargo de `autor`. Não muda nada: a [`Mudanca`] é o
    /// que o servidor grava, e [`Self::aplicar`] é como ela se lê.
    ///
    /// # Errors
    ///
    /// A [`RecusaDeCargo`] que explica a recusa.
    #[allow(clippy::too_many_lines, reason = "uma regra por verbo, lidas juntas")]
    pub fn decidir(
        &self,
        autor: PersonId,
        revisao_esperada: u64,
        pedido: &PedidoDeCargo,
    ) -> Result<Mudanca, RecusaDeCargo> {
        // A revisão primeiro: uma tela que viu um estado velho não decide
        // sobre o de agora. A prévia da tela é só informativa; quem confirma é
        // esta função, sobre o estado vigente.
        if revisao_esperada != self.revisao {
            return Err(RecusaDeCargo::RevisaoVelha {
                atual: self.revisao,
            });
        }
        let quem = self.efetivas(autor);
        if !quem.pode(Permission::ManageRoles) {
            return Err(RecusaDeCargo::SemPermissao);
        }
        let abaixo =
            |cargo: &Cargo| quem.proprietario || Altura::Cargo(cargo.posicao) < quem.altura;
        let cargo_de = |id: RoleId| self.cargos.get(&id).ok_or(RoleCargoNaoExiste);

        let mudanca = match pedido {
            PedidoDeCargo::Criar {
                nome,
                concede,
                restringe,
            } => {
                if self.em_ordem().len() >= MAX_CARGOS {
                    return Err(RecusaDeCargo::LimiteDeCargos);
                }
                let nome = self.nome_livre(nome, None)?;
                let concede = normalizar(concede);
                let restringe = normalizar(restringe);
                self.conhecidas(&concede, &[])?;
                self.conhecidas(&restringe, &[])?;
                cabe_conceder(&quem, &concede)?;
                // Nasce na posição 1, logo acima do `@everyone`, como no
                // Discord: sempre abaixo de quem cria.
                let id = RoleId(self.maior_id.saturating_add(1));
                let mut ordem = vec![id];
                ordem.extend(self.em_ordem().iter().map(|cargo| cargo.id));
                if !quem.proprietario && quem.altura == Altura::Cargo(0) {
                    // Sem cargo nenhum, não há «abaixo de você» onde nascer.
                    return Err(RecusaDeCargo::CargoAcimaDeVoce);
                }
                Mudanca::Criou {
                    cargo: Cargo {
                        id,
                        nome,
                        posicao: 1,
                        concede,
                        restringe,
                    },
                    posicoes: Self::numerar(&ordem),
                }
            }
            PedidoDeCargo::Editar {
                cargo,
                nome,
                concede,
                restringe,
            } => {
                let antes = cargo_de(*cargo).map_err(RecusaDeCargo::from)?;
                if !abaixo(antes) {
                    return Err(RecusaDeCargo::CargoAcimaDeVoce);
                }
                let nome = if antes.e_todos() {
                    NOME_DO_TODOS.to_owned()
                } else {
                    self.nome_livre(nome, Some(*cargo))?
                };
                let concede = normalizar(concede);
                let restringe = normalizar(restringe);
                self.conhecidas(&concede, &antes.concede)?;
                self.conhecidas(&restringe, &antes.restringe)?;
                // Só o ACRESCENTADO precisa caber: tirar uma concessão de um
                // cargo que já está abaixo de você só o diminui.
                let acrescentadas: Vec<Permissao> = concede
                    .iter()
                    .filter(|permissao| !antes.concede.contains(permissao))
                    .cloned()
                    .collect();
                cabe_conceder(&quem, &acrescentadas)?;
                // Tirar uma restrição, ou acrescentar uma, NÃO é livre: as duas
                // mudam o efetivo de quem tem o cargo. Quem confere é o guarda
                // do ganho efetivo, no fim desta função, para todo verbo.
                Mudanca::Editou(Cargo {
                    id: *cargo,
                    nome,
                    posicao: antes.posicao,
                    concede,
                    restringe,
                })
            }
            PedidoDeCargo::Mover { cargo, posicao } => {
                let antes = cargo_de(*cargo).map_err(RecusaDeCargo::from)?;
                if antes.e_todos() {
                    return Err(RecusaDeCargo::CargoProtegido);
                }
                if !abaixo(antes) {
                    return Err(RecusaDeCargo::CargoAcimaDeVoce);
                }
                let ordem: Vec<RoleId> = self.em_ordem().iter().map(|cargo| cargo.id).collect();
                let total = u32::try_from(ordem.len()).unwrap_or(u32::MAX);
                if *posicao == 0 || *posicao > total {
                    return Err(RecusaDeCargo::PosicaoInvalida);
                }
                if !quem.proprietario && Altura::Cargo(*posicao) >= quem.altura {
                    return Err(RecusaDeCargo::CargoAcimaDeVoce);
                }
                let mut ordem: Vec<RoleId> = ordem.into_iter().filter(|id| id != cargo).collect();
                let indice = usize::try_from(*posicao - 1)
                    .unwrap_or(usize::MAX)
                    .min(ordem.len());
                ordem.insert(indice, *cargo);
                Mudanca::Reordenou(Self::numerar(&ordem))
            }
            PedidoDeCargo::Apagar { cargo } => {
                let antes = cargo_de(*cargo).map_err(RecusaDeCargo::from)?;
                if antes.e_todos() {
                    return Err(RecusaDeCargo::CargoProtegido);
                }
                if !abaixo(antes) {
                    return Err(RecusaDeCargo::CargoAcimaDeVoce);
                }
                let ordem: Vec<RoleId> = self
                    .em_ordem()
                    .iter()
                    .map(|cargo| cargo.id)
                    .filter(|id| id != cargo)
                    .collect();
                Mudanca::Apagou {
                    cargo: *cargo,
                    posicoes: Self::numerar(&ordem),
                }
            }
            PedidoDeCargo::Atribuir { pessoa, cargo }
            | PedidoDeCargo::Revogar { pessoa, cargo } => {
                let alvo = cargo_de(*cargo).map_err(RecusaDeCargo::from)?;
                if alvo.e_todos() {
                    return Err(RecusaDeCargo::CargoProtegido);
                }
                if !self.pessoas.contains(pessoa) {
                    return Err(RecusaDeCargo::PessoaNaoExiste);
                }
                if !abaixo(alvo) {
                    return Err(RecusaDeCargo::CargoAcimaDeVoce);
                }
                if self.proprietario == Some(*pessoa) && !quem.proprietario {
                    return Err(RecusaDeCargo::Proprietario);
                }
                if *pessoa != autor && !quem.proprietario && self.altura(*pessoa) >= quem.altura {
                    return Err(RecusaDeCargo::PessoaAcimaDeVoce);
                }
                if let PedidoDeCargo::Atribuir { .. } = pedido {
                    // «Só concede o que tem» vale também para atribuir: sem
                    // isto, quem gerencia cargos e não bane daria a si mesmo
                    // um cargo de baixo que bane (decisão SEELE).
                    cabe_conceder(&quem, &alvo.concede)?;
                    Mudanca::Atribuiu {
                        pessoa: *pessoa,
                        cargo: *cargo,
                    }
                } else {
                    Mudanca::Revogou {
                        pessoa: *pessoa,
                        cargo: *cargo,
                    }
                }
            }
        };

        let mut depois = self.clone();
        depois.aplicar(&mudanca);
        self.conferir_o_ganho_efetivo(autor, &quem, &mudanca, &depois)?;
        if !depois.quadro().cabe_num_quadro() {
            return Err(RecusaDeCargo::QuadroCheio);
        }
        Ok(mudanca)
    }

    /// **O guarda do ganho efetivo** (revisão do Codex de 24/10): o que vale é
    /// o efeito, e não o bit cru que o verbo mexeu.
    ///
    /// Para cada pessoa, o efetivo ANTES e DEPOIS da mudança, com as
    /// restrições, o Administrador expandido e as permissões de MOD:
    ///
    /// - quem mudou (ganhou ou perdeu) tem de ser o próprio autor ou estar
    ///   abaixo dele — senão [`RecusaDeCargo::AtingeQuemEstaAcima`];
    /// - tudo o que alguém GANHOU tem de estar no efetivo do autor (antes) —
    ///   senão [`RecusaDeCargo::GanhoAlemDaSuaAutoridade`]. O autor, então,
    ///   nunca ganha nada por um verbo próprio.
    ///
    /// Pega o que as conferências por verbo não pegam: tirar uma restrição,
    /// revogar de si um cargo que restringe, apagar um cargo que restringe, e
    /// conceder Administrador estando restrito em alguma coisa. O proprietário
    /// é a exceção documentada.
    fn conferir_o_ganho_efetivo(
        &self,
        autor: PersonId,
        quem: &Efetivas,
        mudanca: &Mudanca,
        depois: &Self,
    ) -> Result<(), RecusaDeCargo> {
        if quem.proprietario {
            return Ok(());
        }
        // A altura também, DEPOIS da mudança: as posições se renumeram, e
        // «abaixo de mim» pela numeração de antes pode não ser abaixo pela de
        // depois (achado pelo teste de propriedade, com posições com buraco).
        let altura_do_autor_depois = depois.altura(autor);
        for pessoa in &self.pessoas {
            if *pessoa != autor
                && self.altura(*pessoa) < quem.altura
                && depois.altura(*pessoa) >= altura_do_autor_depois
            {
                return Err(RecusaDeCargo::CargoAcimaDeVoce);
            }
        }
        if let Mudanca::Criou { cargo, .. } | Mudanca::Editou(cargo) = mudanca {
            let id = cargo.id;
            if depois.cargos.get(&id).is_some_and(|cargo| {
                !cargo.e_todos() && Altura::Cargo(cargo.posicao) >= altura_do_autor_depois
            }) {
                return Err(RecusaDeCargo::CargoAcimaDeVoce);
            }
        }
        if let Mudanca::Reordenou(posicoes) = mudanca {
            for (id, _) in posicoes {
                let antes = self
                    .cargos
                    .get(id)
                    .map(|cargo| Altura::Cargo(cargo.posicao));
                let agora = depois
                    .cargos
                    .get(id)
                    .map(|cargo| Altura::Cargo(cargo.posicao));
                if antes < Some(quem.altura) && agora >= Some(altura_do_autor_depois) {
                    return Err(RecusaDeCargo::CargoAcimaDeVoce);
                }
            }
        }
        for pessoa in &self.pessoas {
            let antes_dela = self.efetivas(*pessoa);
            let depois_dela = depois.efetivas(*pessoa);
            if antes_dela.produto == depois_dela.produto
                && antes_dela.de_mods == depois_dela.de_mods
            {
                continue;
            }
            if *pessoa != autor && antes_dela.altura >= quem.altura {
                return Err(RecusaDeCargo::AtingeQuemEstaAcima { pessoa: *pessoa });
            }
            let ganho_do_produto = depois_dela
                .produto
                .difference(&antes_dela.produto)
                .map(|permissao| Permissao::Produto(*permissao));
            let ganho_de_mod = depois_dela
                .de_mods
                .difference(&antes_dela.de_mods)
                .map(|chave| Permissao::DeMod(chave.clone()));
            if let Some(permissao) = ganho_do_produto
                .chain(ganho_de_mod)
                .find(|permissao| !quem.tem(permissao))
            {
                return Err(RecusaDeCargo::GanhoAlemDaSuaAutoridade {
                    pessoa: *pessoa,
                    permissao,
                });
            }
        }
        Ok(())
    }

    /// Se `pessoa` pode, agora, executar a ação `chave` do MOD dela.
    ///
    /// É o portão do despacho de `ClientMessage::AcaoDeMod`: o servidor só
    /// chama a metade de servidor do MOD depois de isto dizer sim, naquela
    /// hora, para aquela pessoa e aquela ação.
    ///
    /// # Errors
    ///
    /// [`RecusaDeAcaoDeMod::NaoDeclarada`] quando nenhum MOD ligado declarou a
    /// ação; [`RecusaDeAcaoDeMod::SemPermissao`] quando a pessoa não a tem.
    pub fn autorizar_acao_de_mod(
        &self,
        pessoa: PersonId,
        chave: &ChaveDeMod,
    ) -> Result<(), RecusaDeAcaoDeMod> {
        if !self.declaradas.contains_key(chave) {
            return Err(RecusaDeAcaoDeMod::NaoDeclarada);
        }
        if self.efetivas(pessoa).de_mods.contains(chave) {
            Ok(())
        } else {
            Err(RecusaDeAcaoDeMod::SemPermissao)
        }
    }

    /// Aplica uma mudança. Idempotente para atribuir e revogar.
    pub fn aplicar(&mut self, mudanca: &Mudanca) {
        let reposicionar = |cargos: &mut BTreeMap<RoleId, Cargo>, posicoes: &[(RoleId, u32)]| {
            for (id, posicao) in posicoes {
                if let Some(cargo) = cargos.get_mut(id) {
                    cargo.posicao = *posicao;
                }
            }
        };
        match mudanca {
            Mudanca::Criou { cargo, posicoes } => {
                self.maior_id = self.maior_id.max(cargo.id.get());
                self.cargos.insert(cargo.id, cargo.clone());
                reposicionar(&mut self.cargos, posicoes);
            }
            Mudanca::Editou(cargo) => {
                self.cargos.insert(cargo.id, cargo.clone());
            }
            Mudanca::Reordenou(posicoes) => reposicionar(&mut self.cargos, posicoes),
            Mudanca::Apagou { cargo, posicoes } => {
                self.cargos.remove(cargo);
                for cargos in self.atribuicoes.values_mut() {
                    cargos.remove(cargo);
                }
                reposicionar(&mut self.cargos, posicoes);
            }
            Mudanca::Atribuiu { pessoa, cargo } => {
                self.atribuicoes.entry(*pessoa).or_default().insert(*cargo);
            }
            Mudanca::Revogou { pessoa, cargo } => {
                if let Some(cargos) = self.atribuicoes.get_mut(pessoa) {
                    cargos.remove(cargo);
                }
            }
        }
        self.atribuicoes.retain(|_, cargos| !cargos.is_empty());
        self.revisao = self.revisao.saturating_add(1);
    }

    /// O quadro que o fio leva.
    #[must_use]
    pub fn quadro(&self) -> QuadroDeCargos {
        let mut cargos: Vec<Cargo> = self.em_ordem().into_iter().rev().cloned().collect();
        cargos.extend(self.cargos.get(&TODOS).cloned());
        QuadroDeCargos {
            cargos,
            atribuicoes: self
                .atribuicoes
                .iter()
                .filter(|(_, cargos)| !cargos.is_empty())
                .map(|(pessoa, cargos)| Atribuicao {
                    pessoa: *pessoa,
                    cargos: cargos.iter().copied().collect(),
                })
                .collect(),
            proprietario: self.proprietario,
            de_mods: self
                .declaradas
                .iter()
                .map(|(chave, nome)| PermissaoDeModNoQuadro {
                    chave: chave.clone(),
                    nome: nome.clone(),
                })
                .collect(),
            revisao: self.revisao,
            dono_declarado: self.dono_declarado,
        }
    }

    fn nome_livre(&self, nome: &str, proprio: Option<RoleId>) -> Result<String, RecusaDeCargo> {
        let nome = nome.trim();
        if nome.is_empty()
            || nome.len() > MAX_NOME_DO_CARGO
            || nome.chars().any(char::is_control)
            || nome.eq_ignore_ascii_case(NOME_DO_TODOS)
        {
            return Err(RecusaDeCargo::NomeInvalido);
        }
        if self
            .cargos
            .values()
            .any(|cargo| Some(cargo.id) != proprio && cargo.nome == nome)
        {
            return Err(RecusaDeCargo::NomeTomado);
        }
        Ok(nome.to_owned())
    }

    /// Recusa uma chave de MOD que nenhum MOD ligado declarou — menos as que o
    /// cargo já tinha, que ficam guardadas (ADR 0045: desligar preserva) e não
    /// concedem nada enquanto não forem declaradas de novo.
    fn conhecidas(&self, lista: &[Permissao], ja_tinha: &[Permissao]) -> Result<(), RecusaDeCargo> {
        for permissao in lista {
            if let Permissao::DeMod(chave) = permissao {
                if !self.declaradas.contains_key(chave) && !ja_tinha.contains(permissao) {
                    return Err(RecusaDeCargo::PermissaoDesconhecida {
                        permissao: permissao.clone(),
                    });
                }
            }
        }
        if lista.len() > MAX_PERMISSOES_NA_LISTA {
            return Err(RecusaDeCargo::QuadroCheio);
        }
        Ok(())
    }
}

/// O erro interno de «não há cargo», convertido na recusa do fio.
struct RoleCargoNaoExiste;

impl From<RoleCargoNaoExiste> for RecusaDeCargo {
    fn from(_: RoleCargoNaoExiste) -> Self {
        Self::CargoNaoExiste
    }
}

fn normalizar(lista: &[Permissao]) -> Vec<Permissao> {
    lista
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// «Só concede o que tem» (Discord), com o proprietário de fora.
fn cabe_conceder(quem: &Efetivas, lista: &[Permissao]) -> Result<(), RecusaDeCargo> {
    if quem.proprietario {
        return Ok(());
    }
    match lista.iter().find(|permissao| !quem.tem(permissao)) {
        Some(permissao) => Err(RecusaDeCargo::NaoPodeConceder {
            permissao: permissao.clone(),
        }),
        None => Ok(()),
    }
}

impl QuadroDeCargos {
    /// Se este quadro cabe num quadro de controle.
    #[must_use]
    pub fn cabe_num_quadro(&self) -> bool {
        crate::control::encode(&crate::control::ServerMessage::QuadroDeCargos(self.clone())).is_ok()
    }
}

/// O verbo de cargo que esta mensagem pede, com a revisão que ela viu, se ela
/// pede um.
#[must_use]
pub fn pedido_do_fio(mensagem: &crate::control::ClientMessage) -> Option<(u64, PedidoDeCargo)> {
    use crate::control::ClientMessage as M;
    Some(match mensagem {
        M::CriarCargo {
            revisao,
            nome,
            concede,
            restringe,
        } => (
            *revisao,
            PedidoDeCargo::Criar {
                nome: nome.clone(),
                concede: concede.clone(),
                restringe: restringe.clone(),
            },
        ),
        M::EditarCargo {
            revisao,
            cargo,
            nome,
            concede,
            restringe,
        } => (
            *revisao,
            PedidoDeCargo::Editar {
                cargo: *cargo,
                nome: nome.clone(),
                concede: concede.clone(),
                restringe: restringe.clone(),
            },
        ),
        M::MoverCargo {
            revisao,
            cargo,
            posicao,
        } => (
            *revisao,
            PedidoDeCargo::Mover {
                cargo: *cargo,
                posicao: *posicao,
            },
        ),
        M::ApagarCargo { revisao, cargo } => (*revisao, PedidoDeCargo::Apagar { cargo: *cargo }),
        M::AtribuirCargo {
            revisao,
            pessoa,
            cargo,
        } => (
            *revisao,
            PedidoDeCargo::Atribuir {
                pessoa: *pessoa,
                cargo: *cargo,
            },
        ),
        M::RevogarCargo {
            revisao,
            pessoa,
            cargo,
        } => (
            *revisao,
            PedidoDeCargo::Revogar {
                pessoa: *pessoa,
                cargo: *cargo,
            },
        ),
        _ => return None,
    })
}

/// A mensagem do fio que pede este verbo, com esta revisão — o inverso de
/// [`pedido_do_fio`], para os clientes montarem o comando.
#[must_use]
pub fn mensagem_do_pedido(revisao: u64, pedido: PedidoDeCargo) -> crate::control::ClientMessage {
    use crate::control::ClientMessage as M;
    match pedido {
        PedidoDeCargo::Criar {
            nome,
            concede,
            restringe,
        } => M::CriarCargo {
            revisao,
            nome,
            concede,
            restringe,
        },
        PedidoDeCargo::Editar {
            cargo,
            nome,
            concede,
            restringe,
        } => M::EditarCargo {
            revisao,
            cargo,
            nome,
            concede,
            restringe,
        },
        PedidoDeCargo::Mover { cargo, posicao } => M::MoverCargo {
            revisao,
            cargo,
            posicao,
        },
        PedidoDeCargo::Apagar { cargo } => M::ApagarCargo { revisao, cargo },
        PedidoDeCargo::Atribuir { pessoa, cargo } => M::AtribuirCargo {
            revisao,
            pessoa,
            cargo,
        },
        PedidoDeCargo::Revogar { pessoa, cargo } => M::RevogarCargo {
            revisao,
            pessoa,
            cargo,
        },
    }
}

/// Confere um verbo de cargo do fio (o braço do `Validate`).
///
/// # Errors
///
/// O primeiro campo fora do teto.
pub fn validar_pedido(
    nome: Option<&str>,
    concede: &[Permissao],
    restringe: &[Permissao],
) -> Result<(), ControlError> {
    if let Some(nome) = nome {
        check_bounds("nome_do_cargo", nome.len(), MAX_NOME_DO_CARGO)?;
    }
    validar_lista(concede)?;
    validar_lista(restringe)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod testes {
    /// `decidir` com a revisão vigente: os testes da regra não são sobre a revisão,
    /// e os que são chamam `decidir` direto.
    trait Agora {
        fn decidir_agora(
            &self,
            autor: PersonId,
            pedido: &PedidoDeCargo,
        ) -> Result<Mudanca, RecusaDeCargo>;
    }
    impl Agora for Estado {
        fn decidir_agora(
            &self,
            autor: PersonId,
            pedido: &PedidoDeCargo,
        ) -> Result<Mudanca, RecusaDeCargo> {
            self.decidir(autor, self.revisao, pedido)
        }
    }
    use super::*;
    use proptest::prelude::*;

    const DONO: PersonId = PersonId(1);
    const ADMIN: PersonId = PersonId(2);
    const MODERA: PersonId = PersonId(3);
    const GENTE: PersonId = PersonId(4);
    const OUTRA: PersonId = PersonId(5);

    const ADMINS: RoleId = RoleId(10);
    const MODERADORES: RoleId = RoleId(11);
    const MEMBROS: RoleId = RoleId(12);
    const SILENCIO: RoleId = RoleId(13);

    fn p(permissao: Permission) -> Permissao {
        Permissao::Produto(permissao)
    }

    fn cargo(
        id: RoleId,
        nome: &str,
        posicao: u32,
        concede: &[Permission],
        restringe: &[Permission],
    ) -> Cargo {
        Cargo {
            id,
            nome: nome.into(),
            posicao,
            concede: concede.iter().copied().map(p).collect(),
            restringe: restringe.iter().copied().map(p).collect(),
        }
    }

    fn chave(acao: &str) -> ChaveDeMod {
        ChaveDeMod {
            mod_id: "seele/mesa".into(),
            acao: acao.into(),
        }
    }

    /// Um servidor como o de uma noite comum: dono, um administrador, um
    /// moderador, duas pessoas comuns.
    fn servidor() -> Estado {
        use Permission as P;
        let mut estado = Estado {
            proprietario: Some(DONO),
            maior_id: 13,
            ..Estado::default()
        };
        for cargo in [
            cargo(
                TODOS,
                NOME_DO_TODOS,
                0,
                &[P::ViewVoiceRoom, P::ReadChannel],
                &[],
            ),
            cargo(ADMINS, "Admins", 4, &[P::Administrator], &[]),
            cargo(
                MODERADORES,
                "Moderação",
                3,
                &[P::Kick, P::MovePerson, P::ManageRoles, P::Speak],
                &[],
            ),
            cargo(
                MEMBROS,
                "Membros",
                2,
                &[P::EnterVoiceRoom, P::Speak, P::WriteChannel],
                &[],
            ),
            cargo(SILENCIO, "Silêncio", 1, &[], &[P::Speak, P::WriteChannel]),
        ] {
            estado.cargos.insert(cargo.id, cargo);
        }
        for pessoa in [DONO, ADMIN, MODERA, GENTE, OUTRA] {
            estado.pessoas.insert(pessoa);
        }
        estado.atribuicoes.insert(ADMIN, [ADMINS].into());
        estado
            .atribuicoes
            .insert(MODERA, [MODERADORES, MEMBROS].into());
        estado.atribuicoes.insert(GENTE, [MEMBROS].into());
        estado.atribuicoes.insert(OUTRA, [MEMBROS].into());
        estado
            .declaradas
            .insert(chave("fechar-mesa"), "Fechar a mesa".into());
        estado
    }

    #[test]
    fn as_concessoes_somam_com_as_do_everyone() {
        let estado = servidor();
        let gente = estado.efetivas(GENTE);
        assert!(gente.pode(Permission::ReadChannel), "a base do @everyone");
        assert!(gente.pode(Permission::Speak), "o cargo Membros");
        assert!(!gente.pode(Permission::Kick));
    }

    #[test]
    fn a_restricao_vence_a_concessao_de_outro_cargo() {
        let mut estado = servidor();
        estado.atribuicoes.get_mut(&GENTE).unwrap().insert(SILENCIO);
        let gente = estado.efetivas(GENTE);
        assert!(!gente.pode(Permission::Speak));
        assert!(!gente.pode(Permission::WriteChannel));
        assert!(gente.pode(Permission::EnterVoiceRoom), "o resto sobrevive");
        assert_eq!(
            estado.explicar(GENTE, &p(Permission::Speak)).motivo,
            Motivo::Restringida {
                por: vec![SILENCIO]
            }
        );
    }

    #[test]
    fn o_administrador_implica_tudo_inclusive_as_de_mod_mas_nao_a_restricao() {
        let mut estado = servidor();
        let admin = estado.efetivas(ADMIN);
        for permissao in PERMISSOES_DO_PRODUTO {
            assert!(admin.pode(permissao), "{permissao:?}");
        }
        assert!(admin.de_mods.contains(&chave("fechar-mesa")));
        estado.atribuicoes.get_mut(&ADMIN).unwrap().insert(SILENCIO);
        assert!(
            !estado.efetivas(ADMIN).pode(Permission::Speak),
            "o Administrador passou por cima da restrição"
        );
    }

    #[test]
    fn o_proprietario_sempre_pode_ate_restrito_e_banido() {
        let mut estado = servidor();
        estado.atribuicoes.insert(DONO, [SILENCIO].into());
        estado.banidos.insert(DONO);
        let dono = estado.efetivas(DONO);
        for permissao in PERMISSOES_DO_PRODUTO {
            assert!(dono.pode(permissao), "{permissao:?}");
        }
        assert!(dono.de_mods.contains(&chave("fechar-mesa")));
        assert_eq!(dono.altura, Altura::Proprietario);
    }

    #[test]
    fn quem_nao_existe_ou_esta_banido_nao_pode_nada() {
        let mut estado = servidor();
        assert!(estado.efetivas(PersonId(99)).produto.is_empty());
        estado.banidos.insert(ADMIN);
        assert!(estado.efetivas(ADMIN).produto.is_empty());
    }

    #[test]
    fn a_permissao_de_mod_nao_declarada_nunca_concede() {
        let mut estado = servidor();
        let estranha = Permissao::DeMod(chave("apagar-tudo"));
        estado
            .cargos
            .get_mut(&MEMBROS)
            .unwrap()
            .concede
            .push(estranha.clone());
        assert!(!estado.efetivas(GENTE).tem(&estranha));
        assert_eq!(
            estado.explicar(GENTE, &estranha).motivo,
            Motivo::Desconhecida
        );
        // Nem pelo Administrador: «todas» são as declaradas.
        assert!(!estado.efetivas(ADMIN).tem(&estranha));
        // E criar um cargo com ela é recusado.
        assert_eq!(
            estado.decidir_agora(
                DONO,
                &PedidoDeCargo::Criar {
                    nome: "x".into(),
                    concede: vec![estranha.clone()],
                    restringe: vec![]
                }
            ),
            Err(RecusaDeCargo::PermissaoDesconhecida {
                permissao: estranha
            })
        );
    }

    #[test]
    fn um_nome_gravado_desconhecido_nao_vira_permissao() {
        assert_eq!(Permissao::da_gravada("Superpoder"), None);
        assert_eq!(Permissao::da_gravada("mod:sem-barra#x"), None);
        assert_eq!(Permissao::da_gravada("mod:a/b#Maiuscula"), None);
        assert_eq!(Permissao::da_gravada("Speak"), Some(p(Permission::Speak)));
        let de_mod = Permissao::DeMod(chave("fechar-mesa"));
        assert_eq!(Permissao::da_gravada(&de_mod.gravada()), Some(de_mod));
        for permissao in PERMISSOES_DO_PRODUTO {
            assert_eq!(do_nome_gravado(nome_gravado(permissao)), Some(permissao));
        }
    }

    // ---- a hierarquia ----

    #[test]
    fn so_se_atribui_cargo_abaixo_do_proprio_mais_alto() {
        let estado = servidor();
        // Moderação (3) atribui Membros (2) e Silêncio (1)...
        assert!(estado
            .decidir_agora(
                MODERA,
                &PedidoDeCargo::Atribuir {
                    pessoa: GENTE,
                    cargo: SILENCIO
                }
            )
            .is_ok());
        // ...mas não Moderação, que é a dela, nem Admins.
        for cargo in [MODERADORES, ADMINS] {
            assert_eq!(
                estado.decidir_agora(
                    MODERA,
                    &PedidoDeCargo::Atribuir {
                        pessoa: GENTE,
                        cargo
                    }
                ),
                Err(RecusaDeCargo::CargoAcimaDeVoce)
            );
        }
    }

    #[test]
    fn ninguem_se_da_um_cargo_que_concede_o_que_nao_tem() {
        // A escalada clássica: um cargo de baixo que bane, e quem gerencia
        // cargos sem banir o atribui a si mesmo.
        let mut estado = servidor();
        estado.cargos.insert(
            RoleId(14),
            cargo(RoleId(14), "Banidores", 1, &[Permission::Ban], &[]),
        );
        estado.cargos.get_mut(&SILENCIO).unwrap().posicao = 2;
        estado.cargos.get_mut(&MEMBROS).unwrap().posicao = 3;
        estado.cargos.get_mut(&MODERADORES).unwrap().posicao = 4;
        estado.cargos.get_mut(&ADMINS).unwrap().posicao = 5;
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Atribuir {
                    pessoa: MODERA,
                    cargo: RoleId(14)
                }
            ),
            Err(RecusaDeCargo::NaoPodeConceder {
                permissao: p(Permission::Ban)
            })
        );
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Atribuir {
                    pessoa: GENTE,
                    cargo: RoleId(14)
                }
            ),
            Err(RecusaDeCargo::NaoPodeConceder {
                permissao: p(Permission::Ban)
            })
        );
    }

    #[test]
    fn ninguem_concede_num_cargo_o_que_nao_tem() {
        let estado = servidor();
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Editar {
                    cargo: MEMBROS,
                    nome: "Membros".into(),
                    concede: vec![
                        p(Permission::EnterVoiceRoom),
                        p(Permission::Speak),
                        p(Permission::WriteChannel),
                        p(Permission::Ban)
                    ],
                    restringe: vec![],
                }
            ),
            Err(RecusaDeCargo::NaoPodeConceder {
                permissao: p(Permission::Ban)
            })
        );
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Criar {
                    nome: "Novo".into(),
                    concede: vec![p(Permission::Administrator)],
                    restringe: vec![],
                }
            ),
            Err(RecusaDeCargo::NaoPodeConceder {
                permissao: p(Permission::Administrator)
            })
        );
    }

    #[test]
    fn ninguem_edita_o_proprio_cargo_mais_alto_nem_um_acima() {
        let estado = servidor();
        for cargo in [MODERADORES, ADMINS] {
            assert_eq!(
                estado.decidir_agora(
                    MODERA,
                    &PedidoDeCargo::Editar {
                        cargo,
                        nome: "x".into(),
                        concede: vec![],
                        restringe: vec![]
                    }
                ),
                Err(RecusaDeCargo::CargoAcimaDeVoce)
            );
        }
    }

    #[test]
    fn ninguem_mexe_nos_cargos_de_quem_esta_acima_nem_do_proprietario() {
        let estado = servidor();
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Revogar {
                    pessoa: ADMIN,
                    cargo: MEMBROS
                }
            ),
            Err(RecusaDeCargo::PessoaAcimaDeVoce)
        );
        assert_eq!(
            estado.decidir_agora(
                ADMIN,
                &PedidoDeCargo::Atribuir {
                    pessoa: DONO,
                    cargo: SILENCIO
                }
            ),
            Err(RecusaDeCargo::Proprietario)
        );
    }

    // ---- o ganho efetivo: os cinco casos da revisão do Codex ----

    /// Um gestor que gerencia cargos e expulsa, sem banir, e um cargo de baixo
    /// que concede e restringe Banir. `GESTOR` é o MODERA do servidor.
    fn com_banir_restrito() -> Estado {
        let mut estado = servidor();
        let restrito = estado.cargos.get_mut(&MEMBROS).unwrap();
        restrito.concede.push(p(Permission::Ban));
        restrito.restringe.push(p(Permission::Ban));
        estado
    }

    fn membros_com(estado: &Estado, restringe: Vec<Permissao>) -> PedidoDeCargo {
        let membros = &estado.cargos[&MEMBROS];
        PedidoDeCargo::Editar {
            cargo: MEMBROS,
            nome: membros.nome.clone(),
            concede: membros.concede.clone(),
            restringe,
        }
    }

    #[test]
    fn caso_1_tirar_a_restricao_de_banir_sem_ter_banir_e_recusado() {
        let estado = com_banir_restrito();
        assert!(!estado.efetivas(GENTE).pode(Permission::Ban));
        let pedido = membros_com(&estado, vec![]);
        assert_eq!(
            estado.decidir_agora(MODERA, &pedido),
            Err(RecusaDeCargo::GanhoAlemDaSuaAutoridade {
                pessoa: MODERA,
                permissao: p(Permission::Ban)
            }),
            "o próprio gestor tem Membros, e ganharia Banir primeiro"
        );
        // Sem o gestor no cargo, quem ganharia é a GENTE — recusado igual.
        let mut sem_ele = estado.clone();
        sem_ele
            .atribuicoes
            .get_mut(&MODERA)
            .unwrap()
            .remove(&MEMBROS);
        assert!(matches!(
            sem_ele.decidir_agora(MODERA, &pedido),
            Err(RecusaDeCargo::GanhoAlemDaSuaAutoridade { permissao, .. }) if permissao == p(Permission::Ban)
        ));
        // O permitido equivalente: quem tem Banir efetivo pode tirar.
        let mut com_banir = sem_ele.clone();
        com_banir
            .cargos
            .get_mut(&MODERADORES)
            .unwrap()
            .concede
            .push(p(Permission::Ban));
        assert!(com_banir.decidir_agora(MODERA, &pedido).is_ok());
    }

    #[test]
    fn caso_2_revogar_de_si_o_cargo_que_restringe_e_recusado() {
        let mut estado = servidor();
        estado
            .cargos
            .get_mut(&MODERADORES)
            .unwrap()
            .concede
            .push(p(Permission::Ban));
        estado
            .atribuicoes
            .get_mut(&MODERA)
            .unwrap()
            .insert(SILENCIO);
        estado
            .cargos
            .get_mut(&SILENCIO)
            .unwrap()
            .restringe
            .push(p(Permission::Ban));
        assert!(!estado.efetivas(MODERA).pode(Permission::Ban));
        assert!(matches!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Revogar {
                    pessoa: MODERA,
                    cargo: SILENCIO
                }
            ),
            Err(RecusaDeCargo::GanhoAlemDaSuaAutoridade { pessoa: MODERA, .. })
        ));
        // O permitido equivalente: quem tem Banir efetivo (sem o Silêncio)
        // revoga o Silêncio de alguém abaixo, devolvendo só o que ele tem.
        estado
            .atribuicoes
            .get_mut(&MODERA)
            .unwrap()
            .remove(&SILENCIO);
        estado.atribuicoes.get_mut(&GENTE).unwrap().insert(SILENCIO);
        assert!(estado.efetivas(MODERA).pode(Permission::Ban));
        assert!(estado
            .decidir_agora(
                MODERA,
                &PedidoDeCargo::Revogar {
                    pessoa: GENTE,
                    cargo: SILENCIO
                }
            )
            .is_ok());
    }

    #[test]
    fn caso_3_apagar_o_cargo_que_restringe_e_devolve_o_que_o_autor_nao_tem_e_recusado() {
        let mut estado = servidor();
        estado
            .cargos
            .get_mut(&MEMBROS)
            .unwrap()
            .concede
            .push(p(Permission::Ban));
        estado
            .cargos
            .get_mut(&SILENCIO)
            .unwrap()
            .restringe
            .push(p(Permission::Ban));
        estado.atribuicoes.get_mut(&GENTE).unwrap().insert(SILENCIO);
        estado
            .atribuicoes
            .get_mut(&MODERA)
            .unwrap()
            .remove(&MEMBROS);
        assert!(!estado.efetivas(GENTE).pode(Permission::Ban));
        let recusa = estado.decidir_agora(MODERA, &PedidoDeCargo::Apagar { cargo: SILENCIO });
        let Err(RecusaDeCargo::GanhoAlemDaSuaAutoridade {
            pessoa: GENTE,
            permissao,
        }) = recusa
        else {
            panic!("apagar o cargo que restringe passou: {recusa:?}");
        };
        assert!(!estado.efetivas(MODERA).tem(&permissao), "{permissao:?}");
        // O permitido: o proprietário apaga.
        assert!(estado
            .decidir_agora(DONO, &PedidoDeCargo::Apagar { cargo: SILENCIO })
            .is_ok());
    }

    #[test]
    fn caso_4_tirar_a_restricao_de_administrador_e_recusado() {
        let mut estado = servidor();
        let membros = estado.cargos.get_mut(&MEMBROS).unwrap();
        membros.concede.push(p(Permission::Administrator));
        membros.restringe.push(p(Permission::Administrator));
        estado
            .atribuicoes
            .get_mut(&MODERA)
            .unwrap()
            .remove(&MEMBROS);
        assert!(!estado.efetivas(GENTE).pode(Permission::Ban));
        let pedido = membros_com(&estado, vec![]);
        assert!(matches!(
            estado.decidir_agora(MODERA, &pedido),
            Err(RecusaDeCargo::GanhoAlemDaSuaAutoridade { pessoa: GENTE, .. })
        ));
        // Permitido para quem é Administrador de verdade (e está acima).
        assert!(estado.decidir_agora(ADMIN, &pedido).is_ok());
    }

    #[test]
    fn caso_5_administrador_restrito_nao_concede_administrador() {
        let mut estado = servidor();
        estado.atribuicoes.get_mut(&ADMIN).unwrap().insert(SILENCIO);
        estado
            .cargos
            .get_mut(&SILENCIO)
            .unwrap()
            .restringe
            .push(p(Permission::Ban));
        let admin = estado.efetivas(ADMIN);
        assert!(admin.pode(Permission::Administrator) && !admin.pode(Permission::Ban));
        // Dar o cargo Admins (Administrador cru) à GENTE: ela ganharia Banir.
        // Admins está na altura do ADMIN, então antes disso vem a hierarquia;
        // o caso de verdade é um cargo de baixo que concede Administrador.
        let pedido = PedidoDeCargo::Editar {
            cargo: MEMBROS,
            nome: "Membros".into(),
            concede: vec![
                p(Permission::EnterVoiceRoom),
                p(Permission::Speak),
                p(Permission::WriteChannel),
                p(Permission::Administrator),
            ],
            restringe: vec![],
        };
        assert!(matches!(
            estado.decidir_agora(ADMIN, &pedido),
            Err(RecusaDeCargo::GanhoAlemDaSuaAutoridade { permissao, .. }) if permissao == p(Permission::Ban)
        ));
        // Permitido: o mesmo Administrador sem a restrição.
        estado
            .atribuicoes
            .get_mut(&ADMIN)
            .unwrap()
            .remove(&SILENCIO);
        assert!(estado.decidir_agora(ADMIN, &pedido).is_ok());
    }

    #[test]
    fn uma_restricao_nova_nao_alcanca_quem_esta_acima() {
        // A sabotagem: o moderador restringe Banir no @everyone, ou num cargo
        // de baixo que um administrador também tem, e tira o Banir de quem
        // está acima dele.
        let mut estado = servidor();
        estado.atribuicoes.get_mut(&ADMIN).unwrap().insert(MEMBROS);
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Editar {
                    cargo: TODOS,
                    nome: String::new(),
                    concede: vec![p(Permission::ViewVoiceRoom), p(Permission::ReadChannel)],
                    restringe: vec![p(Permission::Kick)],
                }
            ),
            Err(RecusaDeCargo::AtingeQuemEstaAcima { pessoa: ADMIN })
        );
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Editar {
                    cargo: MEMBROS,
                    nome: "Membros".into(),
                    concede: vec![
                        p(Permission::EnterVoiceRoom),
                        p(Permission::Speak),
                        p(Permission::WriteChannel)
                    ],
                    restringe: vec![p(Permission::Kick)],
                }
            ),
            Err(RecusaDeCargo::AtingeQuemEstaAcima { pessoa: ADMIN })
        );
        // O proprietário pode.
        assert!(estado
            .decidir_agora(
                DONO,
                &PedidoDeCargo::Editar {
                    cargo: TODOS,
                    nome: String::new(),
                    concede: vec![],
                    restringe: vec![p(Permission::Kick)],
                }
            )
            .is_ok());
    }

    // ---- a revisão (revisão do Codex sobre concorrência) ----

    #[test]
    fn uma_revisao_velha_e_recusada_e_nada_muda() {
        let estado = servidor();
        let pedido = PedidoDeCargo::Atribuir {
            pessoa: GENTE,
            cargo: SILENCIO,
        };
        assert_eq!(
            estado.decidir(MODERA, estado.revisao + 1, &pedido),
            Err(RecusaDeCargo::RevisaoVelha {
                atual: estado.revisao
            })
        );
        assert!(estado.decidir(MODERA, estado.revisao, &pedido).is_ok());
    }

    #[test]
    fn a_restricao_de_a_nao_e_desfeita_pela_edicao_velha_de_b() {
        // Os dois gestores leem a revisão 0. A acrescenta uma restrição ao
        // Membros; B, com a tela velha, manda o Membros como era — sem ela.
        let mut estado = servidor();
        let lida = estado.revisao;
        let sem_restricao = membros_com(&estado, vec![]);
        let com_restricao = membros_com(&estado, vec![p(Permission::AttachFile)]);
        let de_a = estado.decidir(ADMIN, lida, &com_restricao).unwrap();
        estado.aplicar(&de_a);
        assert_eq!(
            estado.decidir(ADMIN, lida, &sem_restricao),
            Err(RecusaDeCargo::RevisaoVelha { atual: lida + 1 })
        );
        assert_eq!(
            estado.cargos[&MEMBROS].restringe,
            vec![p(Permission::AttachFile)],
            "a edição velha desfez a restrição"
        );
    }

    #[test]
    fn a_edicao_depois_da_revogacao_do_gestor_e_negada() {
        // A autoridade é a do estado de agora: o dono revoga a Moderação do
        // gestor, e o pedido que ele manda em seguida — mesmo com a revisão
        // nova — já não tem Gerenciar cargos.
        let mut estado = servidor();
        let revogou = estado
            .decidir(
                DONO,
                estado.revisao,
                &PedidoDeCargo::Revogar {
                    pessoa: MODERA,
                    cargo: MODERADORES,
                },
            )
            .unwrap();
        estado.aplicar(&revogou);
        assert_eq!(
            estado.decidir(
                MODERA,
                estado.revisao,
                &PedidoDeCargo::Atribuir {
                    pessoa: GENTE,
                    cargo: SILENCIO
                }
            ),
            Err(RecusaDeCargo::SemPermissao)
        );
    }

    #[test]
    fn sem_gerenciar_cargos_nada_se_gerencia() {
        let estado = servidor();
        assert_eq!(
            estado.decidir_agora(GENTE, &PedidoDeCargo::Apagar { cargo: SILENCIO }),
            Err(RecusaDeCargo::SemPermissao)
        );
    }

    #[test]
    fn o_everyone_nao_se_apaga_nem_se_move_nem_se_atribui() {
        let estado = servidor();
        assert_eq!(
            estado.decidir_agora(DONO, &PedidoDeCargo::Apagar { cargo: TODOS }),
            Err(RecusaDeCargo::CargoProtegido)
        );
        assert_eq!(
            estado.decidir_agora(
                DONO,
                &PedidoDeCargo::Mover {
                    cargo: TODOS,
                    posicao: 1
                }
            ),
            Err(RecusaDeCargo::CargoProtegido)
        );
        assert_eq!(
            estado.decidir_agora(
                DONO,
                &PedidoDeCargo::Atribuir {
                    pessoa: GENTE,
                    cargo: TODOS
                }
            ),
            Err(RecusaDeCargo::CargoProtegido)
        );
    }

    #[test]
    fn mover_so_abaixo_de_si_e_as_posicoes_ficam_sem_buraco() {
        let mut estado = servidor();
        assert_eq!(
            estado.decidir_agora(
                MODERA,
                &PedidoDeCargo::Mover {
                    cargo: SILENCIO,
                    posicao: 3
                }
            ),
            Err(RecusaDeCargo::CargoAcimaDeVoce)
        );
        let mudanca = estado
            .decidir_agora(
                MODERA,
                &PedidoDeCargo::Mover {
                    cargo: SILENCIO,
                    posicao: 2,
                },
            )
            .unwrap();
        estado.aplicar(&mudanca);
        assert_eq!(estado.cargos[&SILENCIO].posicao, 2);
        assert_eq!(estado.cargos[&MEMBROS].posicao, 1);
        assert_eq!(estado.cargos[&MODERADORES].posicao, 3);
        let mut posicoes: Vec<u32> = estado
            .em_ordem()
            .iter()
            .map(|cargo| cargo.posicao)
            .collect();
        posicoes.sort_unstable();
        assert_eq!(posicoes, vec![1, 2, 3, 4]);
    }

    #[test]
    fn criar_poe_na_posicao_1_com_id_novo_e_apagar_nao_devolve_o_id() {
        let mut estado = servidor();
        let criou = estado
            .decidir_agora(
                MODERA,
                &PedidoDeCargo::Criar {
                    nome: "Música".into(),
                    concede: vec![p(Permission::Speak)],
                    restringe: vec![],
                },
            )
            .unwrap();
        estado.aplicar(&criou);
        let novo = RoleId(14);
        assert_eq!(estado.cargos[&novo].posicao, 1);
        assert_eq!(estado.cargos[&SILENCIO].posicao, 2);
        let apagou = estado
            .decidir_agora(MODERA, &PedidoDeCargo::Apagar { cargo: novo })
            .unwrap();
        estado.aplicar(&apagou);
        let de_novo = estado
            .decidir_agora(
                MODERA,
                &PedidoDeCargo::Criar {
                    nome: "Música".into(),
                    concede: vec![],
                    restringe: vec![],
                },
            )
            .unwrap();
        assert!(matches!(de_novo, Mudanca::Criou { cargo, .. } if cargo.id == RoleId(15)));
    }

    #[test]
    fn a_moderacao_respeita_a_hierarquia() {
        let estado = servidor();
        assert!(estado.pode_moderar(MODERA, GENTE, Permission::Kick));
        assert!(
            !estado.pode_moderar(MODERA, ADMIN, Permission::Kick),
            "abaixo expulsou acima"
        );
        assert!(
            !estado.pode_moderar(ADMIN, DONO, Permission::Kick),
            "expulsaram o proprietário"
        );
        assert!(estado.pode_moderar(DONO, ADMIN, Permission::Ban));
        assert!(
            !estado.pode_moderar(GENTE, OUTRA, Permission::Kick),
            "sem a permissão"
        );
        // Dois na mesma altura não se expulsam.
        let mut empate = estado.clone();
        empate.atribuicoes.insert(OUTRA, [MODERADORES].into());
        assert!(!empate.pode_moderar(MODERA, OUTRA, Permission::Kick));
    }

    #[test]
    fn a_explicacao_e_a_regra_concordam() {
        let estado = servidor();
        for pessoa in [DONO, ADMIN, MODERA, GENTE, OUTRA, PersonId(99)] {
            for permissao in PERMISSOES_DO_PRODUTO {
                assert_eq!(
                    estado.explicar(pessoa, &p(permissao)).tem,
                    estado.efetivas(pessoa).pode(permissao)
                );
            }
        }
    }

    #[test]
    fn o_quadro_ida_e_volta_e_o_mesmo_estado() {
        let estado = servidor();
        let de_volta = Estado::do_quadro(&estado.quadro());
        for pessoa in [DONO, ADMIN, MODERA, GENTE] {
            assert_eq!(de_volta.efetivas(pessoa), estado.efetivas(pessoa));
        }
        assert!(estado.quadro().validar().is_ok());
        assert_eq!(
            estado.quadro().cargos.last().map(|cargo| cargo.id),
            Some(TODOS)
        );
    }

    #[test]
    fn as_declaracoes_de_mod_sao_conferidas() {
        let boa = PermissaoDeclarada {
            acao: "fechar-mesa".into(),
            nome: "Fechar".into(),
            todos: false,
        };
        assert!(validar_declaradas(std::slice::from_ref(&boa)).is_ok());
        let ruim = PermissaoDeclarada {
            acao: "Fechar Mesa".into(),
            ..boa.clone()
        };
        assert_eq!(
            validar_declaradas(&[ruim]),
            Err(RecusaDaDeclaracao::AcaoInvalida { posicao: 0 })
        );
        assert_eq!(
            validar_declaradas(&[boa.clone(), boa.clone()]),
            Err(RecusaDaDeclaracao::Repetida { posicao: 1 })
        );
        assert!(matches!(
            validar_declaradas(&vec![boa; 9]),
            Err(RecusaDaDeclaracao::Demais { quantas: 9 })
        ));
    }

    // ---- propriedades ----

    fn uma_permissao() -> impl Strategy<Value = Permission> + Clone {
        proptest::sample::select(PERMISSOES_DO_PRODUTO.to_vec())
    }

    fn um_estado() -> impl Strategy<Value = (Estado, Vec<PersonId>)> {
        let forma = (
            proptest::collection::btree_set(uma_permissao(), 0..6),
            proptest::collection::btree_set(uma_permissao(), 0..3),
        );
        (
            proptest::collection::vec(forma, 1..6),
            proptest::collection::vec(proptest::collection::btree_set(0_usize..6, 0..4), 4),
        )
            .prop_map(|(cargos, atribuicoes)| {
                let mut estado = Estado {
                    proprietario: Some(PersonId(1)),
                    ..Estado::default()
                };
                estado
                    .cargos
                    .insert(TODOS, cargo(TODOS, NOME_DO_TODOS, 0, &[], &[]));
                for (indice, (concede, restringe)) in cargos.iter().enumerate() {
                    let id = RoleId(u32::try_from(indice).unwrap() + 1);
                    let concede: Vec<Permission> = concede.iter().copied().collect();
                    let restringe: Vec<Permission> = restringe.iter().copied().collect();
                    estado.cargos.insert(
                        id,
                        cargo(id, &format!("c{indice}"), id.get(), &concede, &restringe),
                    );
                }
                estado.maior_id = u32::try_from(cargos.len()).unwrap();
                let pessoas: Vec<PersonId> = (1..=4).map(PersonId).collect();
                for (pessoa, escolhidos) in pessoas.iter().zip(atribuicoes) {
                    estado.pessoas.insert(*pessoa);
                    let ids: BTreeSet<RoleId> = escolhidos
                        .into_iter()
                        .filter(|indice| *indice < cargos.len())
                        .map(|indice| RoleId(u32::try_from(indice).unwrap() + 1))
                        .collect();
                    if !ids.is_empty() {
                        estado.atribuicoes.insert(*pessoa, ids);
                    }
                }
                (estado, pessoas)
            })
    }

    fn um_pedido(pessoas: Vec<PersonId>) -> impl Strategy<Value = PedidoDeCargo> {
        let pessoa = proptest::sample::select(pessoas);
        let cargo = (0_u32..7).prop_map(RoleId);
        let lista = proptest::collection::vec(uma_permissao().prop_map(Permissao::Produto), 0..4);
        prop_oneof![
            (lista.clone(), lista.clone()).prop_map(|(concede, restringe)| PedidoDeCargo::Criar {
                nome: "novo".into(),
                concede,
                restringe
            }),
            (cargo.clone(), lista.clone(), lista).prop_map(|(cargo, concede, restringe)| {
                PedidoDeCargo::Editar {
                    cargo,
                    nome: format!("e{}", cargo.get()),
                    concede,
                    restringe,
                }
            }),
            (cargo.clone(), 0_u32..8)
                .prop_map(|(cargo, posicao)| PedidoDeCargo::Mover { cargo, posicao }),
            cargo
                .clone()
                .prop_map(|cargo| PedidoDeCargo::Apagar { cargo }),
            (pessoa.clone(), cargo.clone())
                .prop_map(|(pessoa, cargo)| PedidoDeCargo::Atribuir { pessoa, cargo }),
            (pessoa, cargo).prop_map(|(pessoa, cargo)| PedidoDeCargo::Revogar { pessoa, cargo }),
        ]
    }

    proptest! {
        /// Nenhum verbo aceito de quem não é proprietário dá a ninguém —
        /// nem a quem pediu — uma permissão que quem pediu não tinha.
        #[test]
        fn nenhum_verbo_aceito_cria_privilegio_que_o_autor_nao_tinha(
            (estado, pessoas) in um_estado(),
            autor_indice in 1_usize..4,
            posicao_do_gestor in 1_u32..7,
            extra_do_gestor in proptest::collection::btree_set(uma_permissao(), 0..5),
            pedidos in proptest::collection::vec(um_pedido((1..=4).map(PersonId).collect()), 1..8),
        ) {
            let autor = pessoas[autor_indice];
            let mut estado = estado;
            // Sem Gerenciar cargos todo verbo é recusado na primeira linha, e
            // a propriedade passaria sem provar nada (medido: com o guarda do
            // ganho efetivo desligado, ela passava). O autor ganha um cargo
            // de gestão próprio, numa posição sorteada entre os outros.
            let gestor = RoleId(estado.maior_id + 1);
            estado.maior_id += 1;
            let mut gestao = cargo(gestor, "gestao", posicao_do_gestor, &[Permission::ManageRoles], &[]);
            gestao.concede.extend(extra_do_gestor.iter().copied().map(Permissao::Produto));
            for outro in estado.cargos.values_mut().filter(|c| !c.e_todos() && c.posicao >= posicao_do_gestor) {
                outro.posicao += 1;
            }
            estado.cargos.insert(gestor, gestao);
            estado.atribuicoes.entry(autor).or_default().insert(gestor);
            for pedido in pedidos {
                let antes = estado.clone();
                let do_autor = antes.efetivas(autor);
                if let Ok(mudanca) = antes.decidir_agora(autor, &pedido) {
                    estado.aplicar(&mudanca);
                    for pessoa in &pessoas {
                        let ganhou: BTreeSet<Permission> = estado
                            .efetivas(*pessoa)
                            .produto
                            .difference(&antes.efetivas(*pessoa).produto)
                            .copied()
                            .collect();
                        prop_assert!(
                            ganhou.is_subset(&do_autor.produto),
                            "{pedido:?} deu {ganhou:?} a {pessoa:?}, e o autor só tinha {:?}",
                            do_autor.produto
                        );
                    }
                    // O proprietário nunca perde nada.
                    prop_assert_eq!(estado.efetivas(PersonId(1)).produto.len(), PERMISSOES_DO_PRODUTO.len());
                    // Ninguém sobe acima de quem pediu.
                    for pessoa in &pessoas {
                        if *pessoa != autor && antes.altura(*pessoa) < do_autor.altura {
                            prop_assert!(estado.altura(*pessoa) < estado.altura(autor));
                        }
                        // E quem estava na altura do autor ou acima não mudou.
                        if *pessoa != autor && antes.altura(*pessoa) >= do_autor.altura {
                            prop_assert_eq!(&estado.efetivas(*pessoa).produto, &antes.efetivas(*pessoa).produto);
                        }
                    }
                }
            }
        }

        /// A explicação concorda com a regra em qualquer estado.
        #[test]
        fn a_explicacao_concorda_sempre((estado, pessoas) in um_estado(), permissao in uma_permissao()) {
            for pessoa in pessoas {
                prop_assert_eq!(
                    estado.explicar(pessoa, &Permissao::Produto(permissao)).tem,
                    estado.efetivas(pessoa).pode(permissao)
                );
            }
        }
    }
}
