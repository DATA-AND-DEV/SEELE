use super::*;

#[test]
fn quem_nao_hospeda_diz_que_nao_hospeda_e_nada_mais() {
    assert_eq!(
        estado_do_servidor_de(None),
        EstadoDoServidor {
            hospedando: false,
            mensagens_desconhecidas: 0,
            sessoes_com_desconhecidas: 0,
        }
    );
}

#[test]
fn quem_hospeda_diz_os_numeros_do_servidor_e_o_zero_tambem() {
    let numeros = |mensagens, sessoes| {
        estado_do_servidor_de(Some(seele_server::hospedagem::EstadoDoServidor {
            mensagens_desconhecidas: mensagens,
            sessoes_com_desconhecidas: sessoes,
        }))
    };
    assert_eq!(
        numeros(0, 0),
        EstadoDoServidor {
            hospedando: true,
            mensagens_desconhecidas: 0,
            sessoes_com_desconhecidas: 0,
        }
    );
    assert_eq!(
        numeros(3, 1),
        EstadoDoServidor {
            hospedando: true,
            mensagens_desconhecidas: 3,
            sessoes_com_desconhecidas: 1,
        }
    );
}
