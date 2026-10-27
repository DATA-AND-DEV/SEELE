//! Os testes dos cargos contra o banco de verdade (ADR 0064).
//!
//! - a migração de um banco 0.16 (esquema 17, montado pelas migrações 1..17
//!   e com dados) preserva o efetivo de CADA combinação dos quatro papéis,
//!   contra a função legada copiada literalmente;
//! - cargos feitos à mão nunca ganham nada (propriedade);
//! - persistência ao reiniciar, concorrência com duas conexões no mesmo
//!   arquivo, isolamento entre dois servidores, ações de MOD.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::{params, Connection};
use seele_proto::cargos::{
    Cargo, ChaveDeMod, PedidoDeCargo, Permissao, PermissaoDeclarada, RecusaDeAcaoDeMod,
    RecusaDeCargo, LEGADAS, PERMISSOES_DO_PRODUTO, TODOS,
};
use seele_proto::control::{ClientMessage, Permission, ServerMessage};
use seele_proto::ids::{PersonId, RoleId};

use crate::permissions::{Permissions, COMMANDER_ROLE, OBSERVER_ROLE, OPERATOR_ROLE, PERSON_ROLE};
use crate::persistence::{schema, Location, Persistence};

/// O esquema da 0.16.
const ESQUEMA_DA_016: i64 = 17;

/// Um banco **da 0.16**: as migrações 1..17, aplicadas como o `migrate` as
/// aplicava, e nada da 18.
fn banco_016(arquivo: &Path) -> Connection {
    let conexao = Connection::open(arquivo).unwrap();
    conexao.pragma_update(None, "foreign_keys", "ON").unwrap();
    conexao
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                 version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL) STRICT;",
        )
        .unwrap();
    for migracao in schema::MIGRATIONS
        .iter()
        .filter(|m| m.version <= ESQUEMA_DA_016)
    {
        conexao.execute_batch(migracao.sql).unwrap();
        conexao
            .execute(
                "INSERT INTO schema_version (version, applied_at) VALUES (?1, 0)",
                [migracao.version],
            )
            .unwrap();
    }
    conexao
}

fn conta(conexao: &Connection, chave: u8, apelido: &str) -> PersonId {
    conexao
        .execute(
            "INSERT INTO people (nickname, public_key, created_at, last_seen_at) VALUES (?1, ?2, 0, 0)",
            params![apelido, &[chave; 32][..]],
        )
        .unwrap();
    PersonId(conexao.last_insert_rowid() as u64)
}

/// **A função legada, copiada literalmente** de `permissions.rs` da 0.16
/// (`Permissions::may`): o banimento nega tudo, e uma negação em qualquer
/// papel vence uma concessão em qualquer outro.
fn may_legado(conexao: &Connection, pessoa: PersonId, nome: &str) -> bool {
    let banido: i64 = conexao
        .query_row(
            "SELECT COUNT(*) FROM bans WHERE person_id = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
            params![pessoa.get() as i64, crate::persistence::now_seconds()],
            |linha| linha.get(0),
        )
        .unwrap();
    if banido > 0 {
        return false;
    }
    let mut consulta = conexao
        .prepare(
            "SELECT r.permissions, r.denials FROM roles r
             JOIN person_roles pr ON pr.role_id = r.id WHERE pr.person_id = ?1",
        )
        .unwrap();
    let linhas: Vec<(String, String)> = consulta
        .query_map([pessoa.get() as i64], |linha| {
            Ok((linha.get(0)?, linha.get(1)?))
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    let lista = |json: &str| serde_json::from_str::<Vec<String>>(json).unwrap_or_default();
    let mut concedida = false;
    for (permissoes, negacoes) in linhas {
        if lista(&negacoes).iter().any(|n| n == nome) {
            return false;
        }
        if lista(&permissoes).iter().any(|n| n == nome) {
            concedida = true;
        }
    }
    concedida
}

fn efetivo_legado(conexao: &Connection, pessoa: PersonId) -> BTreeSet<Permission> {
    LEGADAS
        .iter()
        .copied()
        .filter(|p| may_legado(conexao, pessoa, seele_proto::cargos::nome_gravado(*p)))
        .collect()
}

fn efetivo_novo(persistence: &Persistence, pessoa: PersonId) -> BTreeSet<Permission> {
    crate::cargos::efetivas(persistence, pessoa)
        .unwrap()
        .produto
}

/// **A prova do requisito do Codex:** para CADA uma das 16 combinações dos
/// quatro papéis legados, com e sem banimento, o efetivo depois da migração é
/// IGUAL ao de antes nas treze, e Transmitir tela é igual a Falar de antes.
#[test]
fn a_migracao_preserva_o_efetivo_de_cada_combinacao_dos_quatro_papeis() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let conexao = banco_016(&arquivo);
    let papeis = [COMMANDER_ROLE, OPERATOR_ROLE, PERSON_ROLE, OBSERVER_ROLE];
    let quem_bane = conta(&conexao, 200, "quem-bane");
    let mut casos = Vec::new();
    for mascara in 0_u8..16 {
        for banido in [false, true] {
            let chave = mascara * 2 + u8::from(banido) + 1;
            let pessoa = conta(&conexao, chave, &format!("p{mascara}-{banido}"));
            for (bit, papel) in papeis.iter().enumerate() {
                if mascara & (1 << bit) != 0 {
                    conexao
                        .execute(
                            "INSERT INTO person_roles (person_id, role_id) VALUES (?1, ?2)",
                            params![pessoa.get() as i64, i64::from(papel.get())],
                        )
                        .unwrap();
                }
            }
            if banido {
                conexao
                    .execute(
                        "INSERT INTO bans (person_id, issued_by, created_at) VALUES (?1, ?2, 0)",
                        params![pessoa.get() as i64, quem_bane.get() as i64],
                    )
                    .unwrap();
            }
            let antes = efetivo_legado(&conexao, pessoa);
            let falava = antes.contains(&Permission::Speak);
            casos.push((mascara, banido, pessoa, antes, falava));
        }
    }
    drop(conexao);

    let persistence = Persistence::open(&Location::File(arquivo)).unwrap();
    assert_eq!(
        persistence.schema_version().unwrap(),
        schema::MIGRATIONS.len() as i64
    );
    for (mascara, banido, pessoa, antes, falava) in casos {
        let depois = efetivo_novo(&persistence, pessoa);
        let depois_legadas: BTreeSet<Permission> = depois
            .iter()
            .copied()
            .filter(|p| LEGADAS.contains(p))
            .collect();
        assert_eq!(
            depois_legadas, antes,
            "papéis {mascara:04b}, banido={banido}: o efetivo mudou na migração"
        );
        assert_eq!(
            depois.contains(&Permission::ShareScreen),
            falava,
            "papéis {mascara:04b}: Transmitir tela deixou de ser igual a Falar"
        );
        // Ninguém vira dono numa migração.
        assert!(
            !crate::cargos::efetivas(&persistence, pessoa)
                .unwrap()
                .proprietario
        );
    }
}

#[test]
fn a_migracao_nao_perde_dado_nenhum_e_semeia_o_everyone_vazio() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let conexao = banco_016(&arquivo);
    let ana = conta(&conexao, 1, "ana");
    let bia = conta(&conexao, 2, "bia");
    conexao
        .execute_batch(
            "INSERT INTO channels (id, name) VALUES (1, 'geral');
             INSERT INTO person_roles (person_id, role_id) VALUES (1, 1), (2, 3), (2, 4);
             INSERT INTO messages (channel_id, author_id, body, created_at) VALUES (1, 1, 'oi', 0), (1, 2, 'olá', 1);
             INSERT INTO config (key, value) VALUES ('commandership_claimed', '1');
             INSERT INTO roles (id, name, permissions, denials) VALUES (7, 'Feito à mão', '[\"Kick\"]', '[]');
             INSERT INTO person_roles (person_id, role_id) VALUES (2, 7);",
        )
        .unwrap();
    let contar = |conexao: &Connection, tabela: &str| -> i64 {
        conexao
            .query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |l| l.get(0))
            .unwrap()
    };
    let antes: Vec<i64> = ["people", "person_roles", "messages", "channels", "config"]
        .iter()
        .map(|t| contar(&conexao, t))
        .collect();
    drop(conexao);

    let persistence = Persistence::open(&Location::File(arquivo)).unwrap();
    let depois: Vec<i64> = ["people", "person_roles", "messages", "channels", "config"]
        .iter()
        .map(|t| contar(persistence.connection(), t))
        .collect();
    assert_eq!(antes, depois, "a migração perdeu ou inventou linhas");

    let quadro = crate::cargos::quadro(&persistence).unwrap();
    let todos = quadro.cargos.last().unwrap();
    assert_eq!(todos.id, TODOS);
    assert!(
        todos.concede.is_empty() && todos.restringe.is_empty(),
        "o @everyone tem de nascer vazio"
    );
    // As posições: o feito à mão embaixo, depois Observer, Person, Operator, Commander.
    let ordem: Vec<RoleId> = quadro.cargos.iter().map(|c| c.id).collect();
    assert_eq!(
        ordem,
        vec![RoleId(1), RoleId(2), RoleId(3), RoleId(4), RoleId(7), TODOS]
    );
    assert!(!quadro.dono_declarado);
    assert_eq!(quadro.proprietario, None);
    // As atribuições continuam as mesmas.
    let bia_tem: Vec<RoleId> = quadro
        .atribuicoes
        .iter()
        .find(|a| a.pessoa == bia)
        .unwrap()
        .cargos
        .clone();
    assert_eq!(bia_tem, vec![RoleId(3), RoleId(4), RoleId(7)]);
    assert!(efetivo_novo(&persistence, ana).contains(&Permission::Administrator));
}

/// Cargos feitos à mão, com concessões e negações quaisquer: depois da
/// migração ninguém tem, nas treze, nada que não tinha. (Com os ids 1..4 de
/// fora, porque esses o teste acima prova por igualdade.)
#[test]
fn cargos_feitos_a_mao_nunca_ganham_nada_na_migracao() {
    // Um gerador determinístico (o seele-server não tem proptest): 64 bancos,
    // cada um com 1 a 4 cargos feitos à mão, concessões e negações sorteadas
    // entre as treze, misturados aos quatro semeados.
    let mut semente: u64 = 0x5EE1_E017;
    let mut sorteio = move |teto: u64| {
        semente = semente
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (semente >> 33) % teto
    };
    let nomes: Vec<&'static str> = LEGADAS
        .iter()
        .map(|p| seele_proto::cargos::nome_gravado(*p))
        .collect();
    for caso in 0..64 {
        let pasta = tempfile::tempdir().unwrap();
        let arquivo = pasta.path().join("seele.db");
        let conexao = banco_016(&arquivo);
        let quantos = 1 + sorteio(4) as usize;
        for indice in 0..quantos {
            let mut lista = || {
                let mut escolhidos: BTreeSet<&str> = BTreeSet::new();
                for _ in 0..sorteio(6) {
                    escolhidos.insert(nomes[sorteio(nomes.len() as u64) as usize]);
                }
                serde_json::to_string(&escolhidos.into_iter().collect::<Vec<_>>()).unwrap()
            };
            let (concede, nega) = (lista(), lista());
            conexao
                .execute(
                    "INSERT INTO roles (id, name, permissions, denials) VALUES (?1, ?2, ?3, ?4)",
                    params![10 + indice as i64, format!("c{indice}"), concede, nega],
                )
                .unwrap();
        }
        let mut pessoas = Vec::new();
        for n in 0..4_u8 {
            let pessoa = conta(&conexao, n + 1, &format!("p{n}"));
            let mascara = sorteio(256);
            let candidatos: Vec<i64> = (1..=4).chain((0..quantos as i64).map(|i| 10 + i)).collect();
            for (bit, cargo) in candidatos.iter().enumerate() {
                if mascara & (1 << bit) != 0 {
                    conexao
                        .execute(
                            "INSERT INTO person_roles (person_id, role_id) VALUES (?1, ?2)",
                            params![pessoa.get() as i64, cargo],
                        )
                        .unwrap();
                }
            }
            pessoas.push((pessoa, efetivo_legado(&conexao, pessoa)));
        }
        drop(conexao);
        let persistence = Persistence::open(&Location::File(arquivo)).unwrap();
        for (pessoa, antes) in pessoas {
            let depois: BTreeSet<Permission> = efetivo_novo(&persistence, pessoa)
                .into_iter()
                .filter(|p| LEGADAS.contains(p))
                .collect();
            assert!(
                depois.is_subset(&antes),
                "caso {caso}: {pessoa:?} ganhou {:?}",
                depois.difference(&antes).collect::<Vec<_>>()
            );
        }
    }
}

// ---- servidor de hoje: verbos, persistência, concorrência, isolamento ----

fn abrir(arquivo: &Path) -> Persistence {
    Persistence::open(&Location::File(arquivo.to_path_buf())).unwrap()
}

fn chave(n: u8) -> String {
    seele_proto::transport::key_fingerprint(&[n; 32])
}

/// Um servidor com dono (chave 1), uma gestora com Moderação (Gerenciar
/// cargos, Expulsar, Banir, Falar) e uma pessoa comum.
fn servidor(arquivo: &Path) -> (Persistence, PersonId, PersonId, PersonId, RoleId) {
    let mut persistence = abrir(arquivo);
    crate::cargos::declarar_proprietario(&persistence, &chave(1)).unwrap();
    let permissions = Permissions::new(&persistence);
    let dono = permissions.register_or_find(&[1; 32], "dono").unwrap().id;
    let gestora = permissions
        .register_or_find(&[2; 32], "gestora")
        .unwrap()
        .id;
    let pessoa = permissions.register_or_find(&[3; 32], "pessoa").unwrap().id;
    let revisao = crate::cargos::quadro(&persistence).unwrap().revisao;
    let concede = [
        Permission::ManageRoles,
        Permission::Kick,
        Permission::Ban,
        Permission::Speak,
    ]
    .into_iter()
    .map(Permissao::Produto)
    .collect();
    let (quadro, _) = crate::cargos::executar(
        &mut persistence,
        dono,
        revisao,
        &PedidoDeCargo::Criar {
            nome: "Moderação".into(),
            concede,
            restringe: vec![],
        },
    )
    .unwrap();
    let moderacao = quadro
        .cargos
        .iter()
        .find(|c| c.nome == "Moderação")
        .unwrap()
        .id;
    // Sobe a Moderação para cima do Pessoa e atribui à gestora.
    let topo = quadro.cargos.iter().filter(|c| c.id != TODOS).count() as u32;
    let (quadro, _) = crate::cargos::executar(
        &mut persistence,
        dono,
        quadro.revisao,
        &PedidoDeCargo::Mover {
            cargo: moderacao,
            posicao: topo - 1,
        },
    )
    .unwrap();
    crate::cargos::executar(
        &mut persistence,
        dono,
        quadro.revisao,
        &PedidoDeCargo::Atribuir {
            pessoa: gestora,
            cargo: moderacao,
        },
    )
    .unwrap();
    (persistence, dono, gestora, pessoa, moderacao)
}

fn revisao(persistence: &Persistence) -> u64 {
    crate::cargos::quadro(persistence).unwrap().revisao
}

/// `executar` com a revisão vigente lida na hora.
fn exec(
    persistence: &mut Persistence,
    autor: PersonId,
    pedido: &PedidoDeCargo,
) -> Result<(seele_proto::cargos::QuadroDeCargos, Vec<PersonId>), RecusaDeCargo> {
    let r = revisao(persistence);
    crate::cargos::executar(persistence, autor, r, pedido)
}

#[test]
fn atribuir_e_revogar_valem_na_hora_e_atravessam_o_reinicio() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, _dono, gestora, pessoa, _) = servidor(&arquivo);
    // Silenciar: a gestora atribui o Observador à pessoa; vale na próxima pergunta.
    assert!(crate::cargos::pode(&persistence, pessoa, Permission::Speak));
    let r = revisao(&persistence);
    let (_, afetadas) = crate::cargos::executar(
        &mut persistence,
        gestora,
        r,
        &PedidoDeCargo::Atribuir {
            pessoa,
            cargo: OBSERVER_ROLE,
        },
    )
    .unwrap();
    assert_eq!(afetadas, vec![pessoa]);
    assert!(
        !crate::cargos::pode(&persistence, pessoa, Permission::Speak),
        "a atribuição não valeu na hora"
    );
    let quadro_antes = crate::cargos::quadro(&persistence).unwrap();
    drop(persistence);

    // Reinício: o mesmo quadro, a mesma revisão, o mesmo efeito.
    let mut persistence = abrir(&arquivo);
    assert_eq!(crate::cargos::quadro(&persistence).unwrap(), quadro_antes);
    assert!(!crate::cargos::pode(
        &persistence,
        pessoa,
        Permission::Speak
    ));

    // E a revogação também vale na hora.
    let r = revisao(&persistence);
    crate::cargos::executar(
        &mut persistence,
        gestora,
        r,
        &PedidoDeCargo::Revogar {
            pessoa,
            cargo: OBSERVER_ROLE,
        },
    )
    .unwrap();
    assert!(crate::cargos::pode(&persistence, pessoa, Permission::Speak));
}

/// **O aviso de arranque manda para o que existe** (achado 7 da conformidade):
/// o `seeled` só aceita `--proprietario`, e não lê seele.toml nenhum. O guarda
/// confere as duas coisas: o aviso cita a flag que o `main.rs` do `seeled`
/// trata, e não cita arquivo de configuração.
#[test]
fn o_aviso_sem_dono_cita_a_flag_que_o_seeled_le_e_nenhum_arquivo() {
    let persistence = Persistence::open(&Location::Memory).unwrap();
    let aviso = crate::cargos::aviso_sem_dono(&persistence).expect("sem dono, há aviso");
    assert!(aviso.contains("seeled --proprietario"), "{aviso}");
    assert!(
        !aviso.contains(".toml") && !aviso.contains("proprietario ="),
        "o aviso manda a um arquivo que o seeled não lê: {aviso}"
    );
    let main = include_str!("../main.rs");
    assert!(
        main.contains("\"--proprietario\""),
        "o seeled deixou de tratar a flag que o aviso cita"
    );
    crate::cargos::declarar_proprietario(&persistence, &chave(1)).unwrap();
    assert!(crate::cargos::aviso_sem_dono(&persistence).is_none());
}

#[test]
fn o_efeito_de_seguranca_e_chamado_no_commit_antes_da_resposta_e_so_num_verbo_aceito() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, _dono, gestora, pessoa, _) = servidor(&arquivo);
    let mut vistos = Vec::new();
    let r = revisao(&persistence);
    // Aceito: o gancho vê a pessoa calada e sem tela, ANTES de `atender` voltar.
    let atendimento = crate::cargos::atender_com_efeito(
        &mut persistence,
        gestora,
        &ClientMessage::AtribuirCargo {
            revisao: r,
            pessoa,
            cargo: OBSERVER_ROLE,
        },
        &mut |efeitos| vistos.extend_from_slice(efeitos),
    )
    .unwrap();
    assert!(atendimento.difundir.is_some());
    assert_eq!(
        vistos,
        vec![crate::cargos::EfeitoDeSeguranca {
            pessoa,
            pode_falar: false,
            pode_transmitir: false,
            pode_entrar: true,
            pode_escrever: false,
        }]
    );
    // Recusado (revisão velha): o gancho não é chamado.
    vistos.clear();
    crate::cargos::atender_com_efeito(
        &mut persistence,
        gestora,
        &ClientMessage::RevogarCargo {
            revisao: r,
            pessoa,
            cargo: OBSERVER_ROLE,
        },
        &mut |efeitos| vistos.extend_from_slice(efeitos),
    )
    .unwrap();
    assert!(vistos.is_empty(), "uma recusa chamou o efeito de segurança");
}

#[test]
fn dois_gestores_no_mesmo_cargo_o_segundo_recebe_revisao_velha_e_a_restricao_fica() {
    // Duas conexões no MESMO arquivo, como dois processos: o que conta é a
    // transação imediata e a revisão, e não o mutex de um processo só.
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut a, dono, _gestora, _pessoa, _) = servidor(&arquivo);
    let mut b = abrir(&arquivo);
    let lida = revisao(&a);
    let pessoa_cargo = crate::cargos::quadro(&a)
        .unwrap()
        .cargos
        .into_iter()
        .find(|c| c.id == PERSON_ROLE)
        .unwrap();
    let com = PedidoDeCargo::Editar {
        cargo: PERSON_ROLE,
        nome: pessoa_cargo.nome.clone(),
        concede: pessoa_cargo.concede.clone(),
        restringe: vec![Permissao::Produto(Permission::AttachFile)],
    };
    let sem = PedidoDeCargo::Editar {
        cargo: PERSON_ROLE,
        nome: pessoa_cargo.nome.clone(),
        concede: pessoa_cargo.concede.clone(),
        restringe: vec![],
    };
    crate::cargos::executar(&mut a, dono, lida, &com).unwrap();
    assert_eq!(
        crate::cargos::executar(&mut b, dono, lida, &sem),
        Err(RecusaDeCargo::RevisaoVelha { atual: lida + 1 })
    );
    let agora = crate::cargos::quadro(&b).unwrap();
    let pessoa_agora = agora.cargos.iter().find(|c| c.id == PERSON_ROLE).unwrap();
    assert_eq!(
        pessoa_agora.restringe,
        vec![Permissao::Produto(Permission::AttachFile)],
        "a edição velha desfez a restrição"
    );
}

#[test]
fn a_edicao_depois_da_revogacao_da_gestora_e_negada_mesmo_de_outra_conexao() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut a, dono, gestora, pessoa, moderacao) = servidor(&arquivo);
    let mut b = abrir(&arquivo);
    exec(
        &mut a,
        dono,
        &PedidoDeCargo::Revogar {
            pessoa: gestora,
            cargo: moderacao,
        },
    )
    .unwrap();
    // A gestora, pela outra conexão e com a revisão nova, já não pode.
    let r = revisao(&b);
    assert_eq!(
        crate::cargos::executar(
            &mut b,
            gestora,
            r,
            &PedidoDeCargo::Atribuir {
                pessoa,
                cargo: OBSERVER_ROLE
            }
        ),
        Err(RecusaDeCargo::SemPermissao)
    );
}

#[test]
fn atender_responde_e_difunde_pelo_fio() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, _dono, gestora, pessoa, _) = servidor(&arquivo);
    let pedido =
        crate::cargos::atender(&mut persistence, pessoa, &ClientMessage::PedirCargos).unwrap();
    assert!(matches!(
        pedido.resposta,
        Some(ServerMessage::QuadroDeCargos(_))
    ));
    // Revisão velha: recusa nomeada, e o quadro de agora junto.
    let velha = crate::cargos::atender(
        &mut persistence,
        gestora,
        &ClientMessage::AtribuirCargo {
            revisao: 0,
            pessoa,
            cargo: OBSERVER_ROLE,
        },
    )
    .unwrap();
    assert!(matches!(
        velha.resposta,
        Some(ServerMessage::CargoRecusado {
            motivo: RecusaDeCargo::RevisaoVelha { .. }
        })
    ));
    assert!(matches!(
        velha.quadro_atual,
        Some(ServerMessage::QuadroDeCargos(_))
    ));
    assert!(velha.difundir.is_none());
    // Aceito: difusão a todos e a pessoa afetada.
    let r = revisao(&persistence);
    let aceito = crate::cargos::atender(
        &mut persistence,
        gestora,
        &ClientMessage::AtribuirCargo {
            revisao: r,
            pessoa,
            cargo: OBSERVER_ROLE,
        },
    )
    .unwrap();
    assert!(aceito.resposta.is_none());
    assert!(matches!(
        aceito.difundir,
        Some(ServerMessage::QuadroDeCargos(_))
    ));
    assert_eq!(aceito.afetadas, vec![pessoa]);
    // Uma mensagem que não é de cargo não é atendida aqui.
    assert!(
        crate::cargos::atender(&mut persistence, pessoa, &ClientMessage::LeaveVoiceRoom).is_none()
    );
}

#[test]
fn a_escalada_pelo_fio_e_negada() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, dono, gestora, pessoa, moderacao) = servidor(&arquivo);
    // Dar-se o Comandante (acima dela).
    let r = revisao(&persistence);
    assert_eq!(
        crate::cargos::executar(
            &mut persistence,
            gestora,
            r,
            &PedidoDeCargo::Atribuir {
                pessoa: gestora,
                cargo: COMMANDER_ROLE
            }
        ),
        Err(RecusaDeCargo::CargoAcimaDeVoce)
    );
    // Editar o próprio cargo para conceder Administrador.
    assert_eq!(
        crate::cargos::executar(
            &mut persistence,
            gestora,
            r,
            &PedidoDeCargo::Editar {
                cargo: moderacao,
                nome: "Moderação".into(),
                concede: vec![Permissao::Produto(Permission::Administrator)],
                restringe: vec![]
            }
        ),
        Err(RecusaDeCargo::CargoAcimaDeVoce)
    );
    // Conceder num cargo de baixo o que não tem (Mover pessoas).
    assert_eq!(
        crate::cargos::executar(
            &mut persistence,
            gestora,
            r,
            &PedidoDeCargo::Criar {
                nome: "Movedores".into(),
                concede: vec![Permissao::Produto(Permission::MovePerson)],
                restringe: vec![]
            }
        ),
        Err(RecusaDeCargo::NaoPodeConceder {
            permissao: Permissao::Produto(Permission::MovePerson)
        })
    );
    // Mexer nos cargos do dono.
    assert_eq!(
        crate::cargos::executar(
            &mut persistence,
            gestora,
            r,
            &PedidoDeCargo::Atribuir {
                pessoa: dono,
                cargo: OBSERVER_ROLE
            }
        ),
        Err(RecusaDeCargo::Proprietario)
    );
    // Uma pessoa comum, nada.
    assert_eq!(
        crate::cargos::executar(
            &mut persistence,
            pessoa,
            r,
            &PedidoDeCargo::Apagar {
                cargo: OBSERVER_ROLE
            }
        ),
        Err(RecusaDeCargo::SemPermissao)
    );
    // E nada disso mexeu na revisão.
    assert_eq!(revisao(&persistence), r);
}

#[test]
fn um_cargo_exigido_por_canal_nao_se_apaga() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, dono, ..) = servidor(&arquivo);
    persistence
        .connection()
        .execute_batch(
            "INSERT INTO channels (id, name, minimum_read_role) VALUES (9, 'fechado', 4);",
        )
        .unwrap();
    let r = revisao(&persistence);
    assert_eq!(
        crate::cargos::executar(
            &mut persistence,
            dono,
            r,
            &PedidoDeCargo::Apagar {
                cargo: OBSERVER_ROLE
            }
        ),
        Err(RecusaDeCargo::CargoProtegido)
    );
}

#[test]
fn dois_servidores_nao_se_misturam() {
    // A mesma chave nos dois: dona de um, Pessoa do outro. Um cargo criado num
    // não existe no outro.
    let pasta = tempfile::tempdir().unwrap();
    let (mut a, dono_a, ..) = servidor(&pasta.path().join("a.db"));
    let b = abrir(&pasta.path().join("b.db"));
    let mesma_no_b = Permissions::new(&b)
        .register_or_find(&[1; 32], "dono")
        .unwrap()
        .id;
    assert!(crate::cargos::efetivas(&a, dono_a).unwrap().proprietario);
    assert!(
        !crate::cargos::efetivas(&b, mesma_no_b)
            .unwrap()
            .proprietario
    );
    let r = revisao(&a);
    crate::cargos::executar(
        &mut a,
        dono_a,
        r,
        &PedidoDeCargo::Criar {
            nome: "Só no A".into(),
            concede: vec![],
            restringe: vec![],
        },
    )
    .unwrap();
    assert!(crate::cargos::quadro(&b)
        .unwrap()
        .cargos
        .iter()
        .all(|c| c.nome != "Só no A"));
    assert_eq!(revisao(&b), 0);
}

// ---- cada permissão: um caso permitido e um negado ----

#[test]
fn cada_permissao_do_produto_tem_um_permitido_e_um_negado() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, dono, ..) = servidor(&arquivo);
    for (n, permissao) in PERMISSOES_DO_PRODUTO.iter().copied().enumerate() {
        let com = Permissions::new(&persistence)
            .register_or_find(&[100 + n as u8; 32], &format!("com{n}"))
            .unwrap()
            .id;
        let sem = Permissions::new(&persistence)
            .register_or_find(&[150 + n as u8; 32], &format!("sem{n}"))
            .unwrap()
            .id;
        for pessoa in [com, sem] {
            Permissions::new(&persistence)
                .revoke_role(pessoa, PERSON_ROLE)
                .unwrap();
        }
        let r = revisao(&persistence);
        let (quadro, _) = crate::cargos::executar(
            &mut persistence,
            dono,
            r,
            &PedidoDeCargo::Criar {
                nome: format!("só-{n}"),
                concede: vec![Permissao::Produto(permissao)],
                restringe: vec![],
            },
        )
        .unwrap();
        let cargo = quadro
            .cargos
            .iter()
            .find(|c| c.nome == format!("só-{n}"))
            .unwrap()
            .id;
        crate::cargos::executar(
            &mut persistence,
            dono,
            quadro.revisao,
            &PedidoDeCargo::Atribuir { pessoa: com, cargo },
        )
        .unwrap();
        assert!(
            crate::cargos::pode(&persistence, com, permissao),
            "{permissao:?} não foi concedida"
        );
        let esperado_sem = false;
        assert_eq!(
            crate::cargos::pode(&persistence, sem, permissao),
            esperado_sem,
            "{permissao:?} sem cargo"
        );
        if permissao != Permission::Administrator {
            // Só aquela: nenhuma outra veio junto.
            assert_eq!(
                crate::cargos::efetivas(&persistence, com).unwrap().produto,
                [permissao].into()
            );
        }
    }
}

// ---- MODs ----

fn ligar_mod(persistence: &Persistence, id: &str, declaradas: &[PermissaoDeclarada]) {
    persistence
        .connection()
        .execute(
            "INSERT INTO mods (id, version, hash, enabled) VALUES (?1, '1', ?2, 1)
             ON CONFLICT(id) DO UPDATE SET enabled = 1",
            params![id, "ab".repeat(32)],
        )
        .unwrap();
    crate::cargos::gravar_permissoes_do_mod(persistence, id, declaradas).unwrap();
}

fn declarada(acao: &str, todos: bool) -> PermissaoDeclarada {
    PermissaoDeclarada {
        acao: acao.into(),
        nome: format!("Ação {acao}"),
        todos,
    }
}

#[test]
fn a_acao_de_mod_so_e_autorizada_se_declarada_e_concedida_e_a_revogacao_vale_no_proximo_pedido() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, dono, gestora, pessoa, _) = servidor(&arquivo);
    ligar_mod(
        &persistence,
        "seele/mesa",
        &[declarada("fechar-mesa", false)],
    );

    // Forjado: quem não tem a permissão é recusado no servidor.
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, pessoa, "seele/mesa", "fechar-mesa"),
        Err(RecusaDeAcaoDeMod::SemPermissao)
    );
    // Um nome arbitrário (não declarado) nunca é autorizável — nem pelo dono.
    for quem in [pessoa, dono] {
        assert_eq!(
            crate::cargos::autorizar_acao_de_mod(&persistence, quem, "seele/mesa", "apagar-tudo"),
            Err(RecusaDeAcaoDeMod::NaoDeclarada)
        );
    }
    // O dono e o Administrador têm as declaradas.
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, dono, "seele/mesa", "fechar-mesa"),
        Ok(())
    );

    // Concedida pelo cargo da gestora? Ela não tem; o dono concede ao Pessoa.
    let chave = Permissao::DeMod(ChaveDeMod {
        mod_id: "seele/mesa".into(),
        acao: "fechar-mesa".into(),
    });
    let pessoa_cargo: Cargo = crate::cargos::quadro(&persistence)
        .unwrap()
        .cargos
        .into_iter()
        .find(|c| c.id == PERSON_ROLE)
        .unwrap();
    let mut concede = pessoa_cargo.concede.clone();
    concede.push(chave.clone());
    let r = revisao(&persistence);
    // A gestora não pode conceder o que não tem.
    assert_eq!(
        crate::cargos::executar(
            &mut persistence,
            gestora,
            r,
            &PedidoDeCargo::Editar {
                cargo: PERSON_ROLE,
                nome: pessoa_cargo.nome.clone(),
                concede: concede.clone(),
                restringe: vec![]
            }
        ),
        Err(RecusaDeCargo::NaoPodeConceder {
            permissao: chave.clone()
        })
    );
    crate::cargos::executar(
        &mut persistence,
        dono,
        r,
        &PedidoDeCargo::Editar {
            cargo: PERSON_ROLE,
            nome: pessoa_cargo.nome.clone(),
            concede,
            restringe: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, pessoa, "seele/mesa", "fechar-mesa"),
        Ok(())
    );
    assert_eq!(
        crate::cargos::acoes_do_mod(&persistence, pessoa, "seele/mesa"),
        vec!["fechar-mesa".to_owned()]
    );

    // Revogada: o próximo pedido já é recusado.
    exec(
        &mut persistence,
        dono,
        &PedidoDeCargo::Revogar {
            pessoa,
            cargo: PERSON_ROLE,
        },
    )
    .unwrap();
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, pessoa, "seele/mesa", "fechar-mesa"),
        Err(RecusaDeAcaoDeMod::SemPermissao)
    );
}

#[test]
fn um_mod_da_api_6_sem_declaracao_nao_tem_acao_autorizavel_e_um_mod_desligado_tambem_nao() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (persistence, dono, ..) = servidor(&arquivo);
    // API 6: nada declarado. O `ModRequest` dele segue a regra de hoje (ler o
    // canal; `admin`/`write`); o `AcaoDeMod` é sempre «não declarada».
    ligar_mod(&persistence, "seele/estilo", &[]);
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, dono, "seele/estilo", "pintar"),
        Err(RecusaDeAcaoDeMod::NaoDeclarada)
    );
    // Declarado, mas desligado: também não.
    ligar_mod(
        &persistence,
        "seele/mesa",
        &[declarada("fechar-mesa", false)],
    );
    persistence
        .connection()
        .execute("UPDATE mods SET enabled = 0 WHERE id = 'seele/mesa'", [])
        .unwrap();
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, dono, "seele/mesa", "fechar-mesa"),
        Err(RecusaDeAcaoDeMod::NaoDeclarada)
    );
    // Uma ação com forma inválida também.
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, dono, "seele/mesa", "Fechar Mesa"),
        Err(RecusaDeAcaoDeMod::NaoDeclarada)
    );
}

#[test]
fn todos_true_vai_ao_everyone_so_na_primeira_vez() {
    let pasta = tempfile::tempdir().unwrap();
    let arquivo = pasta.path().join("seele.db");
    let (mut persistence, dono, _gestora, pessoa, _) = servidor(&arquivo);
    ligar_mod(&persistence, "seele/mesa", &[declarada("jogar", true)]);
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, pessoa, "seele/mesa", "jogar"),
        Ok(())
    );
    // O dono tira do @everyone; religar o MOD não devolve.
    let todos = crate::cargos::quadro(&persistence)
        .unwrap()
        .cargos
        .into_iter()
        .find(|c| c.id == TODOS)
        .unwrap();
    exec(
        &mut persistence,
        dono,
        &PedidoDeCargo::Editar {
            cargo: TODOS,
            nome: String::new(),
            concede: vec![],
            restringe: todos.restringe,
        },
    )
    .unwrap();
    ligar_mod(&persistence, "seele/mesa", &[declarada("jogar", true)]);
    assert_eq!(
        crate::cargos::autorizar_acao_de_mod(&persistence, pessoa, "seele/mesa", "jogar"),
        Err(RecusaDeAcaoDeMod::SemPermissao),
        "religar o MOD desfez a decisão do dono"
    );
}
