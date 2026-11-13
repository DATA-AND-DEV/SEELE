use std::sync::{Arc, Mutex};

use super::{Carimbo, Command, Connection, ConnectionError, Event, EventListener, SessaoId};

/// Uma `Connection` sem rede, com a fila dos comandos à vista.
fn conexao() -> (Connection, tokio::sync::mpsc::UnboundedReceiver<Command>) {
    let (commands, fila) = tokio::sync::mpsc::unbounded_channel();
    let conexao = Connection {
        commands,
        shared: super::sem_rede::shared("marcela"),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    (conexao, fila)
}

#[test]
fn o_contador_nunca_da_zero_nem_repete() {
    let mut anterior = SessaoId::nova();
    assert_ne!(anterior, 0, "o contador deu 0, que não é sessão");
    for _ in 0..1000 {
        let numero = SessaoId::nova();
        assert_ne!(numero, 0, "o contador deu 0, que não é sessão");
        assert!(
            numero > anterior,
            "o contador não subiu: {anterior} e depois {numero}"
        );
        anterior = numero;
    }
    // Linhas pedindo ao mesmo tempo nunca recebem o mesmo número.
    let linhas: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| (0..500).map(|_| SessaoId::nova()).collect::<Vec<_>>()))
        .collect();
    let mut todos: Vec<u64> = linhas
        .into_iter()
        .flat_map(|linha| linha.join().unwrap_or_default())
        .collect();
    let quantos = todos.len();
    assert_eq!(quantos, 8 * 500);
    todos.sort_unstable();
    todos.dedup();
    assert_eq!(
        todos.len(),
        quantos,
        "duas sessões do mesmo processo receberam o mesmo número"
    );
    assert!(!todos.contains(&0));
}

#[test]
fn duas_conexoes_tem_sessoes_diferentes_e_comecam_na_geracao_1() {
    let (uma, _fila_de_uma) = conexao();
    let (outra, _fila_da_outra) = conexao();
    assert_eq!(uma.carimbo().geracao, 1);
    assert_eq!(outra.carimbo().geracao, 1);
    assert_ne!(uma.carimbo().sessao, 0);
    assert_ne!(
        uma.carimbo().sessao,
        outra.carimbo().sessao,
        "duas conexões com a mesma sessão: um comando de uma passaria na outra"
    );
}

#[test]
fn o_comando_com_o_carimbo_certo_sai() {
    let (conexao, mut fila) = conexao();
    let carimbo = conexao.carimbo();
    let aceita = conexao.com(carimbo);
    assert!(aceita.is_ok(), "o carimbo da própria conexão foi recusado");
    if let Ok(conexao) = aceita {
        assert_eq!(conexao.open_channel(7), Ok(()));
    }
    assert!(
        matches!(fila.try_recv(), Ok(Command::OpenChannel(canal)) if canal.0 == 7),
        "o comando aceito não chegou à fila do driver"
    );
}

#[test]
fn o_comando_de_outra_sessao_e_recusado_e_nada_sai() {
    let (conexao, mut fila) = conexao();
    let esperado = conexao.carimbo();
    let veio = Carimbo {
        sessao: esperado.sessao + 1,
        geracao: esperado.geracao,
    };
    let recusa = conexao
        .com(veio)
        .and_then(|conexao| conexao.open_channel(7));
    assert_eq!(
        recusa,
        Err(ConnectionError::SessaoNaoConfere { esperado, veio }),
        "um comando com a sessão de outra conexão passou"
    );
    assert!(
        fila.try_recv().is_err(),
        "o comando recusado chegou à fila do driver mesmo assim"
    );
}

#[test]
fn o_comando_de_antes_da_reconexao_e_recusado() {
    let (conexao, mut fila) = conexao();
    let velho = conexao.carimbo();
    // O que o braço `Aviso::Reconectado` de `drive` faz.
    assert_eq!(conexao.shared.nova_geracao(), 2);
    let novo = conexao.carimbo();
    assert_eq!(
        novo,
        Carimbo {
            sessao: velho.sessao,
            geracao: 2
        },
        "a reconexão mudou a sessão, ou não subiu a geração"
    );
    assert_eq!(
        conexao.com(velho).map(|_| ()),
        Err(ConnectionError::SessaoNaoConfere {
            esperado: novo,
            veio: velho
        }),
        "um comando de antes da reconexão passou"
    );
    assert!(fila.try_recv().is_err());
    assert!(conexao.com(novo).is_ok(), "o carimbo novo foi recusado");
}

#[test]
fn a_recusa_tem_nome_e_os_dois_carimbos_no_serde() {
    let recusa = ConnectionError::SessaoNaoConfere {
        esperado: Carimbo {
            sessao: 4,
            geracao: 2,
        },
        veio: Carimbo {
            sessao: 4,
            geracao: 1,
        },
    };
    assert_eq!(
        serde_json::to_value(&recusa).ok(),
        Some(serde_json::json!({
            "SessaoNaoConfere": {
                "esperado": { "sessao": 4, "geracao": 2 },
                "veio": { "sessao": 4, "geracao": 1 }
            }
        }))
    );
}

/// Quem implementa o `on_event_carimbado`.
struct Carimbado(Mutex<Vec<(Carimbo, Event)>>);
impl EventListener for Carimbado {
    fn on_event(&self, _: Event) {
        panic!("o evento chegou sem carimbo a quem pediu o carimbo");
    }
    fn on_event_carimbado(&self, carimbo: Carimbo, event: Event) {
        if let Ok(mut ouvidos) = self.0.lock() {
            ouvidos.push((carimbo, event));
        }
    }
}

/// Quem só implementa o `on_event`, como toda casca de antes da 0.17.
struct SoOEvento(Mutex<Vec<Event>>);
impl EventListener for SoOEvento {
    fn on_event(&self, event: Event) {
        if let Ok(mut ouvidos) = self.0.lock() {
            ouvidos.push(event);
        }
    }
}

#[test]
fn o_evento_sai_com_o_carimbo_de_agora() {
    let (conexao, _fila) = conexao();
    let ouvinte = Arc::new(Carimbado(Mutex::new(Vec::new())));
    conexao.subscribe(Arc::clone(&ouvinte) as Arc<dyn EventListener>);
    let antes = conexao.carimbo();
    conexao.shared.messages_changed();
    conexao.shared.nova_geracao();
    let depois = conexao.carimbo();
    conexao.shared.messages_changed();
    let ouvidos = ouvinte.0.lock().map(|o| o.clone()).unwrap_or_default();
    assert_eq!(
        ouvidos,
        vec![
            (antes, Event::MessagesChanged),
            (depois, Event::MessagesChanged)
        ],
        "o evento não saiu com o carimbo da sessão e da geração em que aconteceu"
    );
}

/// **A despedida recuperável chega à casca com nome e carimbo, e não como
/// fim.** O `FellBehind` (pelo quadro, ou pelo código de fechamento que o
/// core traduz no mesmo quadro) vira um `CaiuComMotivo` com o carimbo da
/// geração que caiu, e nenhum `Ended`. A expulsão continua sendo fim, e não
/// ganha o aviso de queda.
#[test]
fn a_despedida_recuperavel_avisa_com_carimbo_e_nao_encerra() {
    use seele_core::{DisconnectReason, ServerMessage};
    let (para_tras, _fila) = conexao();
    let ouvinte = Arc::new(Carimbado(Mutex::new(Vec::new())));
    para_tras.subscribe(Arc::clone(&ouvinte) as Arc<dyn EventListener>);
    let da_geracao_que_caiu = para_tras.carimbo();

    super::receber_a_mensagem_do_motor(
        &para_tras.shared,
        &ServerMessage::Disconnecting {
            reason: DisconnectReason::FellBehind,
        },
    );
    let ouvidos = ouvinte.0.lock().map(|o| o.clone()).unwrap_or_default();
    let quedas: Vec<_> = ouvidos
        .iter()
        .filter(|(_, evento)| matches!(evento, Event::CaiuComMotivo { .. }))
        .collect();
    assert_eq!(
        quedas,
        vec![&(
            da_geracao_que_caiu,
            Event::CaiuComMotivo {
                reason: DisconnectReason::FellBehind.into()
            }
        )],
        "o «ficou para trás» não chegou à casca como uma queda com nome e carimbo"
    );
    assert!(
        !ouvidos
            .iter()
            .any(|(_, evento)| matches!(evento, Event::Ended { .. })),
        "o «ficou para trás» virou fim: {ouvidos:?}"
    );
    assert!(
        para_tras.snapshot().ended.is_none(),
        "o retrato marcou fim numa despedida recuperável"
    );

    // A expulsão é fim, e não ganha o aviso de queda.
    let (expulsa, _fila) = conexao();
    let ouvinte = Arc::new(Carimbado(Mutex::new(Vec::new())));
    expulsa.subscribe(Arc::clone(&ouvinte) as Arc<dyn EventListener>);
    super::receber_a_mensagem_do_motor(
        &expulsa.shared,
        &ServerMessage::Disconnecting {
            reason: DisconnectReason::Kicked,
        },
    );
    let ouvidos = ouvinte.0.lock().map(|o| o.clone()).unwrap_or_default();
    assert!(
        !ouvidos
            .iter()
            .any(|(_, evento)| matches!(evento, Event::CaiuComMotivo { .. })),
        "a expulsão ganhou o aviso de queda recuperável"
    );
    assert!(
        expulsa.snapshot().ended.is_some(),
        "a expulsão deixou de ser fim"
    );
}

#[test]
fn quem_so_implementa_on_event_continua_ouvindo() {
    let (conexao, _fila) = conexao();
    let ouvinte = Arc::new(SoOEvento(Mutex::new(Vec::new())));
    conexao.subscribe(Arc::clone(&ouvinte) as Arc<dyn EventListener>);
    conexao.shared.messages_changed();
    let ouvidos = ouvinte.0.lock().map(|o| o.clone()).unwrap_or_default();
    assert_eq!(ouvidos, vec![Event::MessagesChanged]);
}

/// Lido da fonte, como `nobody_raises_the_event_without_moving_the_revision`:
/// fora do corpo padrão do trait, nenhum evento é entregue sem carimbo, e a
/// geração sobe no braço da volta antes do primeiro evento dele.
#[test]
fn nenhum_evento_sai_sem_carimbo_e_a_volta_sobe_a_geracao() {
    let fonte = include_str!("../lib.rs");
    let producao = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
    let sem_comentarios: String = producao
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        sem_comentarios.matches(".on_event(").count(),
        1,
        "alguém entrega evento por `on_event` direto, sem o carimbo: só o corpo \
             padrão de `on_event_carimbado` pode"
    );
    let Some(braco) = sem_comentarios
        .split("seele_core::enlace::Aviso::Reconectado {")
        .nth(1)
    else {
        panic!("o braço `Aviso::Reconectado` de `drive` sumiu");
    };
    let geracao = braco.find("shared.nova_geracao()");
    let online = braco.find("shared.gravar_enlace(");
    assert!(
        matches!((geracao, online), (Some(g), Some(o)) if g < o),
        "no braço da volta, a geração não sobe antes do primeiro evento: um \
             comando de antes da queda passaria"
    );
}
