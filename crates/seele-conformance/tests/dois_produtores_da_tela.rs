//! Os eventos da **tela assistida** saem na forma do desktop (Tarefa 3.9 da F4).
//!
//! O `palco-imagem.js` desenha a tela de quem transmite pelos eventos
//! `ScreenOpened`, `ScreenFrame`, `ScreenClosed` e `ScreenUnreadable`, que o
//! desktop manda a partir do `seele_ffi::Event`. O navegador os monta no `.wasm`
//! (`seele_web::fluxos`), lendo o fluxo da tela; aqui, o mesmo fluxo dá os
//! mesmos eventos que a FFI serializaria, campo a campo.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_ffi::Event;
use seele_proto::ids::ScreenId;
use seele_proto::screen::{
    ScreenCodec, ScreenHeader, ScreenSource, TipoDeQuadro, SCREEN_HEADER_LEN,
};
use seele_proto::stream::StreamType;
use seele_web::fluxos::{FluxoQueChega, Passo};

fn evento(passo: &Passo) -> Option<serde_json::Value> {
    match passo {
        Passo::Evento(evento) => Some(evento.clone()),
        _ => None,
    }
}

#[test]
fn a_tela_sai_nos_eventos_do_desktop() {
    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: 1,
        screen: ScreenId(9),
        source: ScreenSource::Window,
        codec: ScreenCodec::H264Baseline,
        width: 1920,
        height: 1080,
    }
    .encode(&mut abertura)
    .unwrap();
    let mut fluxo = vec![StreamType::Screen.byte()];
    fluxo.extend(abertura);
    for (tipo, corpo) in [
        (TipoDeQuadro::Chave, &b"\x00\x00\x00\x01idr"[..]),
        (TipoDeQuadro::Comum, b"p"),
    ] {
        fluxo.push(tipo as u8);
        fluxo.extend(u32::try_from(corpo.len()).unwrap().to_be_bytes());
        fluxo.extend(corpo);
    }

    let mut leitor = FluxoQueChega::novo();
    let mut do_navegador: Vec<serde_json::Value> =
        leitor.chegou(&fluxo).iter().filter_map(evento).collect();
    do_navegador.extend(leitor.acabou().iter().filter_map(evento));

    let do_desktop: Vec<serde_json::Value> = [
        Event::ScreenOpened {
            screen: 9,
            width: 1920,
            height: 1080,
        },
        Event::ScreenFrame {
            screen: 9,
            key: true,
            data: seele_ffi::base64_de(b"\x00\x00\x00\x01idr"),
        },
        Event::ScreenFrame {
            screen: 9,
            key: false,
            data: seele_ffi::base64_de(b"p"),
        },
        Event::ScreenClosed { screen: 9 },
    ]
    .iter()
    .map(|evento| serde_json::to_value(evento).unwrap())
    .collect();
    assert_eq!(do_navegador, do_desktop);

    // E a abertura que não serve sai como a ilegível do desktop: o campo é o
    // mesmo (o motivo é o texto de quem leu).
    let mut torto = vec![StreamType::Screen.byte()];
    torto.extend([0xff_u8; SCREEN_HEADER_LEN]);
    let passos = FluxoQueChega::novo().chegou(&torto);
    let ilegivel = passos.iter().find_map(evento).expect("um evento");
    let molde = serde_json::to_value(Event::ScreenUnreadable {
        reason: String::new(),
    })
    .unwrap();
    assert_eq!(
        ilegivel.as_object().unwrap().keys().collect::<Vec<_>>(),
        molde.as_object().unwrap().keys().collect::<Vec<_>>()
    );
    assert_eq!(
        ilegivel["ScreenUnreadable"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        molde["ScreenUnreadable"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>()
    );
}
