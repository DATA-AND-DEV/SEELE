use super::{motivo_da_trilha, Event};

#[test]
fn os_cargos_marcam_a_placa_e_a_telemetria_so_na_volta() {
    assert_eq!(
        motivo_da_trilha(&Event::CargosChanged { marca: 3 }, false),
        Some("cargos")
    );
    assert_eq!(
        motivo_da_trilha(&Event::MessagesChanged, false),
        Some("mensagens")
    );
    assert_eq!(motivo_da_trilha(&Event::TelemetryChanged, false), None);
    assert_eq!(
        motivo_da_trilha(&Event::TelemetryChanged, true),
        Some("enlace")
    );
}
