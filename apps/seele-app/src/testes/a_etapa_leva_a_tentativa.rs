use super::carga_do_evento;
use seele_ffi::{ConnectStage, Event};

fn etapa() -> Event {
    Event::ConnectStageChanged {
        stage: ConnectStage::Parada {
            candidatos: 2,
            com_bilhete_e_impressao: false,
        },
    }
}

/// **A etapa leva a tentativa da janela, e o resto vai como sempre** (item
/// 52 da pendência 58). A etapa continua com o `stage` de antes, e ganha a
/// `tentativa` ao lado dele; sem tentativa, ela sai igual ao evento; e um
/// evento que não é etapa não ganha nada, com tentativa ou sem.
#[test]
fn so_a_etapa_ganha_a_tentativa_e_sem_ela_nada_muda() {
    let com = carga_do_evento(&etapa(), Some(7));
    assert_eq!(
        com.pointer("/ConnectStageChanged/tentativa"),
        Some(&serde_json::Value::from(7)),
        "a etapa não levou a tentativa: {com}"
    );
    assert_eq!(
        com.pointer("/ConnectStageChanged/stage"),
        serde_json::to_value(etapa())
            .unwrap()
            .pointer("/ConnectStageChanged/stage"),
        "a etapa com a tentativa mudou o `stage`: {com}"
    );
    assert_eq!(
        carga_do_evento(&etapa(), None),
        serde_json::to_value(etapa()).unwrap()
    );
    for outro in [Event::RosterChanged, Event::MessagesChanged] {
        assert_eq!(
            carga_do_evento(&outro, Some(7)),
            serde_json::to_value(&outro).unwrap(),
            "um evento que não é etapa ganhou a tentativa"
        );
    }
}
