//! O codec do `seele-proto`, chamado do mesmo jeito no nativo e no `wasm32`.
//!
//! `vetores()` codifica um conjunto fixo de mensagens com `control::encode`,
//! decodifica cada quadro de volta com `control::decode`, confere que voltou a
//! mesma mensagem, e devolve os quadros num buffer só, cada um com o prefixo de
//! 4 bytes do `frame.rs`. O nativo imprime esse buffer em hex; o Node instancia
//! o `.wasm` e imprime o dele. Os dois textos têm de ser iguais.
//!
//! Também passa por `uri::analisar`, que é texto puro e é o analisador que a
//! página vai precisar.

use seele_proto::control::{self, ClientMessage, ServerMessage};

fn quadro(saida: &mut Vec<u8>, corpo: &[u8]) {
    saida.extend_from_slice(&(corpo.len() as u32).to_be_bytes());
    saida.extend_from_slice(corpo);
}

/// O buffer de quadros, ou o primeiro defeito, como texto.
pub fn vetores() -> Result<Vec<u8>, String> {
    let clientes = vec![
        ClientMessage::Hello {
            join_secret: Some("7K4M-convite".into()),
            version: seele_proto::version::PROTOCOL_VERSION,
            client: "seele-web/0.16 (medição 2)".into(),
            nickname: "ana çãé 名前".into(),
            public_key: (0u8..32).collect(),
        },
        ClientMessage::Response { proof: vec![0xAB; 64] },
        ClientMessage::LeaveVoiceRoom,
    ];
    let servidor = vec![ServerMessage::Challenge { nonce: (100u8..132).collect() }];

    let mut saida = Vec::new();
    for m in &clientes {
        let q = control::encode(m).map_err(|e| format!("encode {m:?}: {e:?}"))?;
        let volta: ClientMessage = control::decode(&q).map_err(|e| format!("decode {m:?}: {e:?}"))?;
        if &volta != m {
            return Err(format!("ida e volta mudou {m:?} para {volta:?}"));
        }
        quadro(&mut saida, &q);
    }
    for m in &servidor {
        let q = control::encode(m).map_err(|e| format!("encode {m:?}: {e:?}"))?;
        let volta: ServerMessage = control::decode(&q).map_err(|e| format!("decode {m:?}: {e:?}"))?;
        if &volta != m {
            return Err(format!("ida e volta mudou {m:?} para {volta:?}"));
        }
        quadro(&mut saida, &q);
    }

    // Um byte de versão errado tem de ser recusado igual nos dois.
    let mut torto = control::encode(&ClientMessage::LeaveVoiceRoom).map_err(|e| format!("{e:?}"))?;
    torto[0] = torto[0].wrapping_add(100);
    let recusa = format!("{:?}", control::decode::<ClientMessage>(&torto).err());
    quadro(&mut saida, recusa.as_bytes());

    // O analisador do link, em texto.
    let fp = "3cbcfb02".repeat(8);
    let link = format!("seele://198.51.100.7:8383/?fp={fp}&convite=7K4M");
    let analisado = format!("{:?}", seele_proto::uri::analisar(&link));
    quadro(&mut saida, analisado.as_bytes());
    let https = format!("{:?}", seele_proto::uri::analisar("https://exemplo.invalid/e#fp=00"));
    quadro(&mut saida, https.as_bytes());

    Ok(saida)
}

static mut SAIDA: Vec<u8> = Vec::new();

/// Roda `vetores()` e guarda o resultado; devolve 0 se deu certo e 1 se não.
/// Em caso de defeito, o buffer guarda o texto do defeito.
#[no_mangle]
pub extern "C" fn rodar() -> u32 {
    let (codigo, bytes) = match vetores() {
        Ok(b) => (0, b),
        Err(e) => (1, e.into_bytes()),
    };
    // Um spike de uma thread só: ninguém mais toca no buffer.
    unsafe {
        let s = &mut *std::ptr::addr_of_mut!(SAIDA);
        *s = bytes;
    }
    codigo
}

#[no_mangle]
pub extern "C" fn saida_ptr() -> *const u8 {
    unsafe { (*std::ptr::addr_of!(SAIDA)).as_ptr() }
}

#[no_mangle]
pub extern "C" fn saida_len() -> usize {
    unsafe { (*std::ptr::addr_of!(SAIDA)).len() }
}
