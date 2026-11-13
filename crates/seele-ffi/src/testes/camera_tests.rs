#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn pedidos(altura: u32, quadros: u32) -> LimitesDeTela {
    LimitesDeTela {
        banda_bps: None,
        altura_maxima: altura,
        quadros_maximos: quadros,
        prioridade: crate::types::Prioridade::Nitidez,
        com_som: true,
    }
}

/// **As medidas de uma câmera passam pela régua do fio antes de qualquer
/// coisa sair.** Zero, um lado acima de 1920 ou uma área acima de 1080p são
/// recusados com o nome, e não viram um fluxo que não abre depois de a
/// sala já ter ouvido que há uma câmera.
#[test]
fn as_medidas_da_camera_passam_pela_regua_do_fio() {
    assert_eq!(medidas_da_camera(1280, 720), Ok((1280, 720)));
    assert_eq!(medidas_da_camera(720, 1280), Ok((720, 1280)));
    for (largura, altura) in [
        (0, 720),
        (1280, 0),
        (1922, 1080),
        (1920, 1920),
        (70_000, 720),
    ] {
        assert_eq!(
            medidas_da_camera(largura, altura),
            Err(ConnectionError::CameraBadSize),
            "{largura}×{altura}"
        );
    }
}

/// **A câmera tem teto de banda mesmo quando a casca não escolhe nenhum**
/// — e a casca não escolhe. Sem ele o orçamento seguia a sonda até 12 Mbps
/// e a câmera congelava num caminho sem perda (medida de 10/11/2026). Uma
/// escolha menor continua valendo; uma maior é cortada no teto.
#[test]
fn a_camera_tem_teto_de_banda_mesmo_sem_escolha() {
    let teto = teto_de_banda_da_camera(seele_core::Resolucao::P720);
    assert_eq!(teto, seele_core::tela::TETO_ESTIMADO_PARA_720P_BPS);
    assert_eq!(limites_da_camera(pedidos(720, 30)).banda_bps, Some(teto));
    let com = |banda_bps| {
        limites_da_camera(LimitesDeTela {
            banda_bps,
            ..pedidos(720, 30)
        })
        .banda_bps
    };
    assert_eq!(com(Some(900_000)), Some(900_000));
    assert_eq!(com(Some(50_000_000)), Some(teto));
    // O degrau de baixo tem o teto dele, menor.
    let de_540 = limites_da_camera(pedidos(540, 30)).banda_bps;
    assert!(de_540 < Some(teto) && de_540.is_some(), "{de_540:?}");
}

/// **A câmera nunca passa de 720p a 30, vai como movimento e sem som**,
/// peça a casca o que pedir.
#[test]
fn os_limites_da_camera_tem_teto_sao_de_movimento_e_nao_levam_som() {
    let do_nucleo = limites_da_camera(pedidos(1080, 60));
    assert_eq!(do_nucleo.resolucao, seele_core::Resolucao::P720);
    assert_eq!(do_nucleo.cadencia, seele_core::Cadencia::Q30);
    assert_eq!(
        do_nucleo.prioridade,
        seele_core::tela::Prioridade::Movimento
    );
    assert!(!do_nucleo.com_som);
    assert_eq!(
        limites_da_camera(pedidos(540, 15)).resolucao,
        seele_core::Resolucao::P540
    );

    let codificacao = codificacao_da_camera(1920, 1080, pedidos(1080, 60));
    assert_eq!(
        (codificacao.largura, codificacao.altura, codificacao.quadros),
        (1280, 720, 30)
    );
    assert_eq!(codificacao.codec, CODEC_DA_CAMERA);
    // Nunca amplia: a câmera de 480 linhas vai do tamanho dela.
    let pequena = codificacao_da_camera(640, 480, pedidos(720, 30));
    assert_eq!((pequena.largura, pequena.altura), (640, 480));
}

/// **Cada pedido da transmissão da câmera vira o evento dele**, com a tela
/// de quem é — e o `ScreenOpened` leva o tipo com o nome que a casca lê.
#[test]
fn os_pedidos_da_camera_e_o_tipo_da_tela_atravessam_pelo_nome() {
    use seele_core::de_fora::SinalParaFora;
    let tela = seele_core::ScreenId(7);
    let json = |evento: &Event| serde_json::to_value(evento).unwrap();
    assert_eq!(
        json(&evento_da_camera(tela, SinalParaFora::Chave)),
        serde_json::json!({ "CameraKeyFrameRequested": { "screen": 7 } })
    );
    assert_eq!(
        json(&evento_da_camera(
            tela,
            SinalParaFora::Banda { bps: 900_000 }
        )),
        serde_json::json!({ "CameraBudget": { "screen": 7, "bps": 900_000 } })
    );
    for motivo in [
        seele_core::MotivoDeParada::SinalCritico,
        seele_core::MotivoDeParada::AbaixoDoPiso,
    ] {
        assert_eq!(
            json(&evento_da_camera(tela, SinalParaFora::Parou(motivo))),
            serde_json::json!({ "CameraPaused": { "screen": 7, "reason": nome_da_parada(motivo) } })
        );
    }
    for (origem, nome) in [
        (seele_core::ScreenSource::Monitor, "monitor"),
        (seele_core::ScreenSource::Window, "window"),
        (seele_core::ScreenSource::Camera, "camera"),
    ] {
        assert_eq!(
            json(&Event::ScreenOpened {
                screen: 7,
                width: 1280,
                height: 720,
                source: tipo_da_tela(origem),
            }),
            serde_json::json!({ "ScreenOpened": { "screen": 7, "width": 1280, "height": 720, "source": nome } })
        );
    }
    assert_eq!(
        serde_json::to_value(ConnectionError::CameraQueueFull { dropped: 3 }).unwrap(),
        serde_json::json!({ "CameraQueueFull": { "dropped": 3 } })
    );
}
