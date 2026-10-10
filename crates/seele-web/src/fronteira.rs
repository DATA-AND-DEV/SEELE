//! A fronteira com o JavaScript: o único arquivo deste crate com `unsafe`.
//!
//! Emenda de 08/10/2026 ao ADR 0057, no molde do `seele-video` (ADR 0041) e do
//! `seele-instalador` (ADR 0043). O `.wasm` não usa `wasm-bindgen`: são seis
//! funções que falam por ponteiro e tamanho, e a casca as chama com uma cola de
//! poucas linhas. A memória é de quem a pede: o JavaScript pede um bloco com
//! [`alocar`], escreve nele, chama, lê a [saída](saida_ptr) e devolve o bloco com
//! [`liberar`].
//!
//! # O contrato
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `codificar(carimbo, json)` | 0 | os bytes do quadro |
//! | | 1 | `{"erro": "json" \| "fio", "texto": …}` |
//! | `decodificar(quadro)` | 0 | `{"mensagem": …}` ou `{"desconhecida": N}` |
//! | | 1 | `{"erro": …, "texto": …, "versao": N}`, com `versao` quando o carimbo é de outra versão |
//!
//! **A voz que sai e a que chega** (Tarefa 3.3, lotes A e C; [`crate::voz`]),
//! na instância da página, uma de cada lado:
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `voz_envio(ssrc)` | — | — |
//! | `voz_regua(abertura_milesimos, com_supressao)` | — | — |
//! | `voz_modo(modo)` (0 aperte-para-falar, 1 por voz, 2 aberto) | — | — |
//! | `voz_tecla(segurando)` | — | — |
//! | `voz_capturou(pcm, mudo)` | quantos quadros saem, de 0 a 3 | cada um: o carimbo (u32) e as 960 amostras (f32), em little-endian |
//! | | `u32::MAX` | `{"erro": …}` |
//! | `voz_portao_contas()` | 0 | `{"nivel_dbfs", "abertura_dbfs", …}` |
//! | | 1 | `{"erro": …}` |
//! | `voz_empacotar(carimbo, opus)` | 0 | o datagrama |
//! | | 2 | nada: o DTX não sai |
//! | | 1 | `{"erro": …}` |
//! | `voz_reabrir(ssrc)` | — | — |
//! | `voz_recepcao(ssrc)` | — | — |
//! | `voz_chegou(datagrama, agora_ms)` | 0 | a alça (u32), a sequência (u16) e o carimbo (u32), em little-endian, e o Opus |
//! | | 2 | nada: a própria voz |
//! | | 1 | `{"erro": …}` |
//! | `voz_esquecer(agora_ms)` | quantas alças | cada alça esquecida, u32 em little-endian |
//!
//! **A voz que toca** (lote C), na instância do `AudioWorklet`. O mesmo
//! `.wasm`, compilado lá dentro a partir dos bytes: o Chromium recusa um
//! `WebAssembly.Module` mandado pela porta (`spikes/voz-em-wasm/`).
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `voz_tocar()` | — | — |
//! | `voz_tocar_chegou(alca, seq, carimbo, chegada_ms, pcm)` | 0 | — |
//! | | 1 | `{"erro": …}` |
//! | `voz_tocar_quadro()` | 0 | as 960 amostras da mistura (f32, little-endian) |
//! | | 1 | `{"erro": …}` |
//! | `voz_tocar_ganho(alca, ganho)` | — | — |
//! | `voz_tocar_isolado(isolado)` | — | — |
//! | `voz_tocar_esquecer(alca)` | — | — |
//! | `voz_tocar_contas()` | 0 | `{"fontes": {alça: …}, "fades", "mistura", …}` |
//! | | 1 | `{"erro": …}` |
//!
//! O som atravessa como bytes de `f32` em little-endian, e não como um
//! `Float32Array` sobre a memória: um bloco de [`alocar`] não promete
//! alinhamento de quatro bytes.
//!
//! Uma chamada sobrescreve a saída da anterior, e o `.wasm` é de uma thread só.

#![allow(unsafe_code)]
#![deny(clippy::undocumented_unsafe_blocks)]

use std::cell::RefCell;

use seele_proto::control::ControlError;
use seele_voz::gate::GateMode;

use crate::voz::{Envio, Recepcao, Reproducao};
use crate::{codificar_do_cliente, decodificar_do_servidor, DoServidor, ErroDaPonte};

thread_local! {
    static SAIDA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ENVIO: RefCell<Option<Envio>> = const { RefCell::new(None) };
    static RECEPCAO: RefCell<Option<Recepcao>> = const { RefCell::new(None) };
    static REPRODUCAO: RefCell<Option<Reproducao>> = const { RefCell::new(None) };
}

fn dizer(bytes: Vec<u8>, codigo: u32) -> u32 {
    SAIDA.with(|s| *s.borrow_mut() = bytes);
    codigo
}

fn erro(e: &ErroDaPonte) -> u32 {
    let corpo = match e {
        ErroDaPonte::Json(texto) => serde_json::json!({ "erro": "json", "texto": texto }),
        ErroDaPonte::Fio(ControlError::UnsupportedVersion { found, .. }) => {
            serde_json::json!({ "erro": "fio", "texto": format!("{e:?}"), "versao": found })
        }
        ErroDaPonte::Fio(_) => serde_json::json!({ "erro": "fio", "texto": format!("{e:?}") }),
    };
    dizer(corpo.to_string().into_bytes(), 1)
}

/// Os bytes que o JavaScript escreveu num bloco de [`alocar`].
///
/// # Safety
///
/// `ptr` e `len` têm de ser um bloco devolvido por [`alocar`] e ainda não
/// liberado, com `len` bytes escritos pelo JavaScript.
unsafe fn entrada<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        return &[];
    }
    // SAFETY: o contrato desta função: `ptr` é o começo de um bloco vivo de
    // pelo menos `len` bytes, alocado como `Vec<u8>` por `alocar`, e ninguém
    // mais o escreve durante a chamada (o `.wasm` tem uma thread só).
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

/// Um bloco de `n` bytes para o JavaScript escrever.
#[no_mangle]
pub extern "C" fn alocar(n: usize) -> *mut u8 {
    let mut bloco = Vec::<u8>::with_capacity(n.max(1));
    let ptr = bloco.as_mut_ptr();
    std::mem::forget(bloco);
    ptr
}

/// Devolve um bloco de [`alocar`].
///
/// # Safety
///
/// `ptr` e `n` têm de ser os de um [`alocar`] ainda não liberado.
#[no_mangle]
pub unsafe extern "C" fn liberar(ptr: *mut u8, n: usize) {
    // SAFETY: `ptr` veio de um `Vec::with_capacity(n.max(1))` em `alocar`, que
    // foi esquecido sem mudar de capacidade; refazer o `Vec` com comprimento 0
    // e a mesma capacidade o devolve ao alocador sem ler o conteúdo.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, n.max(1)) });
}

/// Onde está a saída da última chamada.
#[no_mangle]
pub extern "C" fn saida_ptr() -> *const u8 {
    SAIDA.with(|s| s.borrow().as_ptr())
}

/// Quantos bytes tem a saída da última chamada.
#[no_mangle]
pub extern "C" fn saida_len() -> usize {
    SAIDA.with(|s| s.borrow().len())
}

/// [`codificar_do_cliente`] pela fronteira.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn codificar(carimbo: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let Ok(json) = std::str::from_utf8(bytes) else {
        return erro(&ErroDaPonte::Json("o texto não é UTF-8".into()));
    };
    let Ok(carimbo) = u8::try_from(carimbo) else {
        return erro(&ErroDaPonte::Json(format!(
            "carimbo {carimbo} não cabe num byte"
        )));
    };
    match codificar_do_cliente(carimbo, json) {
        Ok(quadro) => dizer(quadro, 0),
        Err(e) => erro(&e),
    }
}

/// [`decodificar_do_servidor`] pela fronteira.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn decodificar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let quadro = unsafe { entrada(ptr, len) };
    match decodificar_do_servidor(quadro) {
        // O JSON da mensagem já é JSON: entra no envelope sem ser relido.
        Ok(DoServidor::Mensagem(json)) => dizer(format!("{{\"mensagem\":{json}}}").into_bytes(), 0),
        Ok(DoServidor::Desconhecida { ordinal }) => {
            dizer(format!("{{\"desconhecida\":{ordinal}}}").into_bytes(), 0)
        }
        Err(e) => erro(&e),
    }
}

fn dizer_o_link(lido: &serde_json::Value) -> u32 {
    let codigo = u32::from(lido.get("link").is_none());
    dizer(lido.to_string().into_bytes(), codigo)
}

/// Um link do navegador, lido contra a origem do produto
/// (`seele_proto::uri::ORIGEM_DO_NAVEGADOR`, vazia até o D-web-2). Devolve 0
/// com `{"link": …}` na saída, ou 1 com `{"erro": …}`.
///
/// **Sem parâmetro de origem, de propósito**: a página não escolhe contra que
/// origem um link é conferido.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn analisar_link(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let texto = std::str::from_utf8(bytes).unwrap_or("");
    dizer_o_link(&crate::link_do_navegador(
        texto,
        seele_proto::uri::ORIGEM_DO_NAVEGADOR,
    ))
}

// ------------------------------------------------------------------- a voz

fn erro_da_voz(texto: &str) -> u32 {
    dizer(
        serde_json::json!({ "erro": texto })
            .to_string()
            .into_bytes(),
        1,
    )
}

/// O envio de quem o servidor chamou de `ssrc`, com o relógio em zero.
#[no_mangle]
pub extern "C" fn voz_envio(ssrc: u32) {
    ENVIO.with(|e| *e.borrow_mut() = Some(Envio::novo(ssrc)));
}

/// A régua do portão: a sensibilidade escolhida em milésimos de dBFS (zero é
/// o padrão) e se a supressão está ligada.
#[no_mangle]
pub extern "C" fn voz_regua(abertura_milesimos: i32, com_supressao: u32) {
    ENVIO.with(|e| {
        if let Some(envio) = e.borrow_mut().as_mut() {
            envio.regua(abertura_milesimos, com_supressao != 0);
        }
    });
}

/// Como o microfone abre, nos bytes do `VoiceMode` do desktop: 1 por voz, 2
/// aberto, e qualquer outro aperte-para-falar.
#[no_mangle]
pub extern "C" fn voz_modo(modo: u32) {
    let modo = match modo {
        1 => GateMode::VoiceActivated,
        2 => GateMode::Open,
        _ => GateMode::PushToTalk,
    };
    ENVIO.with(|e| {
        if let Some(envio) = e.borrow_mut().as_mut() {
            envio.modo(modo);
        }
    });
}

/// A tecla do aperte-para-falar.
#[no_mangle]
pub extern "C" fn voz_tecla(segurando: u32) {
    ENVIO.with(|e| {
        if let Some(envio) = e.borrow_mut().as_mut() {
            envio.tecla(segurando != 0);
        }
    });
}

/// As amostras `f32` em little-endian de um bloco, ou `None` se o tamanho
/// não for múltiplo de quatro.
fn amostras(bytes: &[u8]) -> Option<Vec<f32>> {
    let pedacos = bytes.chunks_exact(4);
    if !pedacos.remainder().is_empty() {
        return None;
    }
    Some(
        pedacos
            .map(|b| <[u8; 4]>::try_from(b).map_or(0.0, f32::from_le_bytes))
            .collect(),
    )
}

/// Um quadro de 20 ms do microfone: o portão, o relógio e os carimbos, numa
/// chamada. Devolve quantos quadros saem para o `AudioEncoder`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn voz_capturou(ptr: *const u8, len: usize, mudo: u32) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let falhou = |texto: &str| {
        erro_da_voz(texto);
        u32::MAX
    };
    let Some(quadro) = amostras(bytes) else {
        return falhou("o quadro não é de f32");
    };
    ENVIO.with(|e| {
        let mut envio = e.borrow_mut();
        let Some(envio) = envio.as_mut() else {
            return falhou("a voz não tem envio");
        };
        let saem = envio.capturou(&quadro, mudo != 0);
        let mut saida = Vec::with_capacity(saem.len() * (4 + 4 * quadro.len()));
        for (carimbo, pcm) in &saem {
            saida.extend_from_slice(&carimbo.to_le_bytes());
            for amostra in pcm {
                saida.extend_from_slice(&amostra.to_le_bytes());
            }
        }
        dizer(saida, u32::try_from(saem.len()).unwrap_or(u32::MAX))
    })
}

/// O que o portão decidiu por último, para o medidor.
#[no_mangle]
pub extern "C" fn voz_portao_contas() -> u32 {
    ENVIO.with(|e| {
        let envio = e.borrow();
        let Some(envio) = envio.as_ref() else {
            return erro_da_voz("a voz não tem envio");
        };
        let m = envio.portao();
        dizer(
            serde_json::json!({
                "nivel_dbfs": seele_voz::gate::dbfs_de_rms(m.input_rms),
                "abertura_dbfs": m.abertura_efetiva_dbfs,
                "aberturas": m.openings,
                "abertos": m.frames_open,
                "fechados": m.frames_closed,
                "retidos_entregues": m.quadros_retidos_entregues,
            })
            .to_string()
            .into_bytes(),
            0,
        )
    })
}

/// O datagrama de um quadro de Opus com o carimbo dele.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn voz_empacotar(carimbo: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let opus = unsafe { entrada(ptr, len) };
    ENVIO.with(|e| {
        let mut envio = e.borrow_mut();
        let Some(envio) = envio.as_mut() else {
            return erro_da_voz("a voz não tem envio");
        };
        match envio.datagrama(carimbo, opus) {
            Ok(datagrama) => dizer(datagrama, 0),
            Err(seele_proto::media::MediaError::EmptyPayload) => dizer(Vec::new(), 2),
            Err(falha) => erro_da_voz(&falha.to_string()),
        }
    })
}

/// Outro caminho (outro microfone, ou a reconexão com outro `ssrc`).
#[no_mangle]
pub extern "C" fn voz_reabrir(ssrc: u32) {
    ENVIO.with(|e| {
        if let Some(envio) = e.borrow_mut().as_mut() {
            envio.reabrir(ssrc);
        }
    });
}

/// A recepção de quem fala com o `ssrc` próprio.
#[no_mangle]
pub extern "C" fn voz_recepcao(ssrc: u32) {
    RECEPCAO.with(|r| *r.borrow_mut() = Some(Recepcao::nova(ssrc)));
}

/// Um datagrama que chegou em `agora_ms`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn voz_chegou(ptr: *const u8, len: usize, agora_ms: f64) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let datagrama = unsafe { entrada(ptr, len) };
    RECEPCAO.with(|r| {
        let mut recepcao = r.borrow_mut();
        let Some(recepcao) = recepcao.as_mut() else {
            return erro_da_voz("a voz não tem recepção");
        };
        match recepcao.chegou(datagrama, agora_ms) {
            Ok(Some(chegado)) => {
                let mut saida = Vec::with_capacity(10 + chegado.opus.len());
                saida.extend_from_slice(&chegado.alca.to_le_bytes());
                saida.extend_from_slice(&chegado.seq.to_le_bytes());
                saida.extend_from_slice(&chegado.carimbo.to_le_bytes());
                saida.extend_from_slice(chegado.opus);
                dizer(saida, 0)
            }
            Ok(None) => dizer(Vec::new(), 2),
            Err(falha) => erro_da_voz(&falha.to_string()),
        }
    })
}

/// Esquece quem calou até `agora_ms`: quantas alças, e cada uma na saída.
#[no_mangle]
pub extern "C" fn voz_esquecer(agora_ms: f64) -> u32 {
    let alcas = RECEPCAO.with(|r| {
        r.borrow_mut()
            .as_mut()
            .map(|recepcao| recepcao.esquecer_caladas(agora_ms))
            .unwrap_or_default()
    });
    let quantas = u32::try_from(alcas.len()).unwrap_or(u32::MAX);
    dizer(
        alcas.iter().flat_map(|alca| alca.to_le_bytes()).collect(),
        quantas,
    )
}

// ------------------------------------------------------- a voz que toca

fn com_a_reproducao(f: impl FnOnce(&mut Reproducao) -> u32) -> u32 {
    REPRODUCAO.with(|r| match r.borrow_mut().as_mut() {
        Some(reproducao) => f(reproducao),
        None => erro_da_voz("a voz não tem reprodução"),
    })
}

/// A reprodução, sem fonte nenhuma. Na instância do worklet.
#[no_mangle]
pub extern "C" fn voz_tocar() {
    REPRODUCAO.with(|r| *r.borrow_mut() = Some(Reproducao::nova()));
}

/// Um quadro decodificado da fonte `alca`, com o cabeçalho que ele trouxe e a
/// hora em que o datagrama chegou à página.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn voz_tocar_chegou(
    alca: u32,
    seq: u32,
    carimbo: u32,
    chegada_ms: f64,
    ptr: *const u8,
    len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let Some(pcm) = amostras(bytes) else {
        return erro_da_voz("o quadro não é de f32");
    };
    let Ok(seq) = u16::try_from(seq) else {
        return erro_da_voz("a sequência não cabe em 16 bits");
    };
    com_a_reproducao(|reproducao| {
        reproducao.chegou(alca, seq, carimbo, chegada_ms, pcm);
        0
    })
}

/// Os próximos 20 ms da mistura.
#[no_mangle]
pub extern "C" fn voz_tocar_quadro() -> u32 {
    com_a_reproducao(|reproducao| {
        let bytes = reproducao
            .quadro()
            .iter()
            .flat_map(|amostra| amostra.to_le_bytes())
            .collect();
        dizer(bytes, 0)
    })
}

/// O ganho de quem fala pela alça `alca`.
#[no_mangle]
pub extern "C" fn voz_tocar_ganho(alca: u32, ganho: f32) {
    let _ = com_a_reproducao(|reproducao| {
        reproducao.ganho(alca, ganho);
        0
    });
}

/// O isolamento total.
#[no_mangle]
pub extern "C" fn voz_tocar_isolado(isolado: u32) {
    let _ = com_a_reproducao(|reproducao| {
        reproducao.isolar(isolado != 0);
        0
    });
}

/// Esquece a fonte `alca`.
#[no_mangle]
pub extern "C" fn voz_tocar_esquecer(alca: u32) {
    let _ = com_a_reproducao(|reproducao| {
        reproducao.esquecer(alca);
        0
    });
}

/// As contas de cada fonte e da mistura.
#[no_mangle]
pub extern "C" fn voz_tocar_contas() -> u32 {
    com_a_reproducao(|reproducao| dizer(reproducao.contas().to_string().into_bytes(), 0))
}

/// As funções só de teste: o `cargo xtask web` confere com elas os bytes-ouro
/// e o relógio de mídia dentro do `.wasm`. O `.wasm` da página sai sem a
/// feature.
#[cfg(feature = "prova")]
pub mod prova {
    use seele_proto::control::{decode, encode_em, ClientMessage, ServerMessage};

    use super::{dizer, dizer_o_link, entrada, erro, ENVIO};
    use crate::voz::Envio;
    use crate::ErroDaPonte;

    /// Um quadro de 20 ms passou: o carimbo de agora (0 sem envio).
    ///
    /// **Só para a prova**: a página usa [`super::voz_capturou`], que anda o
    /// relógio junto com o portão. As duas peças soltas servem para repetir,
    /// dentro do `.wasm`, a sequência fixa que o desktop grava.
    #[no_mangle]
    pub extern "C" fn voz_quadro_passou() -> u32 {
        ENVIO.with(|e| e.borrow_mut().as_mut().map_or(0, Envio::quadro_passou))
    }

    /// O carimbo do quadro `indice` de `quantos` que o portão soltou. Só para
    /// a prova, como [`voz_quadro_passou`].
    #[no_mangle]
    pub extern "C" fn voz_carimbo_do_quadro(quantos: u32, indice: u32) -> u32 {
        ENVIO.with(|e| {
            e.borrow().as_ref().map_or(0, |envio| {
                envio.carimbo_do_quadro(quantos as usize, indice as usize)
            })
        })
    }

    /// O JSON de uma mensagem do cliente, a partir do quadro.
    ///
    /// # Safety
    ///
    /// `ptr` e `len` como em [`entrada`].
    #[no_mangle]
    pub unsafe extern "C" fn json_do_cliente(ptr: *const u8, len: usize) -> u32 {
        // SAFETY: repassa o contrato desta função ao de `entrada`.
        let quadro = unsafe { entrada(ptr, len) };
        match decode::<ClientMessage>(quadro) {
            Ok(m) => match serde_json::to_string(&m) {
                Ok(json) => dizer(json.into_bytes(), 0),
                Err(e) => erro(&ErroDaPonte::Json(e.to_string())),
            },
            Err(e) => erro(&ErroDaPonte::Fio(e)),
        }
    }

    /// O quadro de uma mensagem do servidor, a partir do JSON.
    ///
    /// # Safety
    ///
    /// `ptr` e `len` como em [`entrada`].
    #[no_mangle]
    pub unsafe extern "C" fn quadro_do_servidor(carimbo: u32, ptr: *const u8, len: usize) -> u32 {
        // SAFETY: repassa o contrato desta função ao de `entrada`.
        let bytes = unsafe { entrada(ptr, len) };
        let lida = std::str::from_utf8(bytes)
            .map_err(|e| ErroDaPonte::Json(e.to_string()))
            .and_then(|t| {
                serde_json::from_str::<ServerMessage>(t)
                    .map_err(|e| ErroDaPonte::Json(e.to_string()))
            });
        match lida {
            Ok(m) => match encode_em(u8::try_from(carimbo).unwrap_or(0), &m) {
                Ok(q) => dizer(q, 0),
                Err(e) => erro(&ErroDaPonte::Fio(e)),
            },
            Err(e) => erro(&e),
        }
    }

    /// [`analisar_link`](super::analisar_link) com a origem dada: os casos da
    /// F1 usam `origem.invalid`, e a do produto é vazia até o D-web-2.
    ///
    /// # Safety
    ///
    /// Os dois pares `ptr`/`len` como em [`entrada`].
    #[no_mangle]
    pub unsafe extern "C" fn analisar_link_em(
        texto_ptr: *const u8,
        texto_len: usize,
        origem_ptr: *const u8,
        origem_len: usize,
    ) -> u32 {
        // SAFETY: repassa o contrato desta função ao de `entrada`.
        let texto = unsafe { entrada(texto_ptr, texto_len) };
        // SAFETY: idem.
        let origem = unsafe { entrada(origem_ptr, origem_len) };
        let texto = std::str::from_utf8(texto).unwrap_or("");
        let origem = std::str::from_utf8(origem).unwrap_or("");
        dizer_o_link(&crate::link_do_navegador(texto, origem))
    }
}
