use super::*;

fn cabecalhos(pares: &[(&'static str, &'static str)]) -> tauri::http::HeaderMap {
    let mut mapa = tauri::http::HeaderMap::new();
    for (nome, valor) in pares {
        mapa.insert(*nome, tauri::http::HeaderValue::from_static(valor));
    }
    mapa
}

/// **O quadro leva a sessão, a geração e a marca de chave nos cabeçalhos**,
/// porque o corpo do pedido é o quadro cru e não tem onde levá-las.
#[test]
fn o_carimbo_e_a_chave_do_quadro_vem_dos_cabecalhos() {
    let lido = cabecalhos_do_quadro(&cabecalhos(&[
        ("seele-sessao", "7"),
        ("seele-geracao", "3"),
        ("seele-chave", "1"),
    ]));
    assert_eq!(
        lido,
        Ok((
            sessoes::CarimboDaJanela {
                sessao: 7,
                geracao: 3
            },
            true
        ))
    );
    // Sem a marca, e com qualquer coisa que não seja `1`: não é chave. É o
    // lado seguro — quem assiste só começa a decodificar por uma chave.
    for marca in [None, Some("0"), Some("true"), Some("")] {
        let mut pares = vec![("seele-sessao", "7"), ("seele-geracao", "3")];
        if let Some(marca) = marca {
            pares.push(("seele-chave", marca));
        }
        assert_eq!(
            cabecalhos_do_quadro(&cabecalhos(&pares)).map(|(_, chave)| chave),
            Ok(false),
            "{marca:?}"
        );
    }
}

/// **Um quadro sem dono não vai a sessão nenhuma.** Sem a sessão, sem a
/// geração, ou com um número que não se lê, a resposta é a de um comando
/// sem sessão — e nunca a sessão que estiver na vista.
#[test]
fn um_quadro_sem_sessao_ou_sem_geracao_nao_tem_dono() {
    for pares in [
        vec![],
        vec![("seele-sessao", "7")],
        vec![("seele-geracao", "3")],
        vec![("seele-sessao", "sete"), ("seele-geracao", "3")],
        vec![("seele-sessao", "7"), ("seele-geracao", "-1")],
    ] {
        assert_eq!(
            cabecalhos_do_quadro(&cabecalhos(&pares)),
            Err(ConnectionError::NotConnected),
            "{pares:?}"
        );
    }
}
