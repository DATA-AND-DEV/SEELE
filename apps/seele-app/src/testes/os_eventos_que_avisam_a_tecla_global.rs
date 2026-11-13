use super::{muda_a_sala_da_tecla_global, Event};

#[test]
fn os_de_sala_avisam_e_os_de_alta_frequencia_nao() {
    // Entrar e sair de sala chega como presença ou como canais; o fim da
    // sessão tira a pessoa da sala sem nenhum dos dois.
    assert!(muda_a_sala_da_tecla_global(&Event::RosterChanged));
    assert!(muda_a_sala_da_tecla_global(&Event::ChannelsChanged));
    // Um aviso por medida de rede ou por mensagem acordaria a conferida o
    // tempo todo, por nada.
    assert!(!muda_a_sala_da_tecla_global(&Event::TelemetryChanged));
    assert!(!muda_a_sala_da_tecla_global(&Event::MessagesChanged));
}
