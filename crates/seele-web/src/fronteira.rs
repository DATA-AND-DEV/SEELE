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
//! **A ligação** (Tarefa 2.6; [`crate::ligacao`]): o aperto de mão e a sessão,
//! uma por página. A página carrega os bytes do fluxo de controle, e assina.
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `ligacao_abrir(pedido)` | 0 | nada: o `Hello` espera em `ligacao_para_o_fio` |
//! | | 1 | `{"recusa": ConnectionError}` ou `{"erro": "json", "texto": …}` |
//! | `ligacao_chegou(bytes)` | 0 | `{"passos": […]}` |
//! | | 1 | `{"erro": "sem-ligacao"}` |
//! | `ligacao_assinado(assinatura)` | 0 | `{"passos": […]}` |
//! | `ligacao_fluxo_acabou()` | 0 | `{"passos": […]}` |
//! | `ligacao_para_o_fio()` | 0 | os bytes enquadrados (talvez nenhum) |
//! | `ligacao_retrato()` | 0 | `{"sala": RetratoDaSala, "contados": {…}, "telemetria": {…}}` |
//! | | 1 | `{"erro": "sem-sessao"}` |
//! | `ligacao_medir(rtt_micros, jitter_ms, perda)` | 1 se mudou, 0 se não | — |
//! | `ligacao_semente(semente)` | — | — |
//! | `ligacao_atender(pedido)` | 0 | `{"resposta", "eventos", "nao_saiu"}` |
//! | | 1 | `{"erro": …}`, o erro do desktop |
//! | | 2 | `{"nao_e_da_sessao": true}` |
//! | `ligacao_largar()` | 0 | os eventos da queda: a sessão fica guardada para a volta |
//! | `ligacao_retomar(pedido, escolhas)` | 0 | nada: o `Hello` da volta espera em `ligacao_para_o_fio` |
//! | | 1 | `{"recusa": …}` ou `{"erro": …}` |
//! | `bateria_nova()`, `bateria_pong()`, `bateria_voltou()` | — | — |
//! | `bateria_caiu(agora_ms)`, `bateria_falhou(agora_ms)` | — | — |
//! | `bateria_poll(agora_ms)` | 0 esperar, 1 ping, 2 voltar, 3 encerrar | — |
//! | `bateria_enlace(agora_ms)` | 0 | o `link` do `Snapshot` |
//! | `ligacao_fechar()` | — | — |
//!
//! **Os fluxos que o servidor abre** (Tarefas 2.6 e 3.9; [`crate::fluxos`]): a
//! tela assistida e o anexo pedido, um leitor por fluxo.
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `fluxo_novo()` | a alça | — |
//! | `fluxo_chegou(alca, pedaco)` | 0 | `{"passos": […]}` |
//! | | 1 | `{"erro": "sem-fluxo"}` |
//! | `fluxo_acabou(alca)` | 0 | `{"passos": […]}`, e a alça é esquecida |
//! | `previa_julgar(pedido, bytes)` | 0 | o `Preview` do desktop |
//! | `tela_quadro(tipo, tamanho)` | 0 | os cinco bytes do cabeçalho de um quadro que sobe (3.10) |
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
//! | `voz_ganhos()` | 0 | `[[alça, ganho], …]`: os volumes escolhidos na sessão, pela alça (Tarefa 2.6) |
//!
//! **O teste do microfone do PC** (lote C), na instância da página, à parte da
//! voz da sala:
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `voz_teste_abrir(abertura_milesimos, com_supressao)` | — | — |
//! | `voz_teste_quadro(pcm)` | quantos quadros o portão soltou | como no `voz_capturou` |
//! | | `u32::MAX` | `{"erro": …}` |
//! | `voz_teste_estado()` | 0 | `{"nivel_dbfs", "aberto", "aberturas", "quadros_retidos", "corte_dbfs"}` |
//! | | 1 | `{"erro": …}` |
//! | `voz_teste_fechar()` | — | — |
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

use crate::comandos::DaPagina;
use crate::fluxos::FluxoQueChega;
use crate::ligacao::{Ligacao, Passo, Pedido};
use crate::voz::{Envio, Recepcao, Reproducao, TesteDoMicrofone};
use crate::{codificar_do_cliente, decodificar_do_servidor, DoServidor, ErroDaPonte};

thread_local! {
    static SAIDA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static LIGACAO: RefCell<Option<Ligacao>> = const { RefCell::new(None) };
    static PAGINA: RefCell<Option<DaPagina>> = const { RefCell::new(None) };
    static BATERIA: RefCell<Option<seele_sessao::battery::Battery>> = const { RefCell::new(None) };
    static FLUXOS: RefCell<(u32, std::collections::BTreeMap<u32, FluxoQueChega>)> =
        const { RefCell::new((0, std::collections::BTreeMap::new())) };
    static ENVIO: RefCell<Option<Envio>> = const { RefCell::new(None) };
    static RECEPCAO: RefCell<Option<Recepcao>> = const { RefCell::new(None) };
    static REPRODUCAO: RefCell<Option<Reproducao>> = const { RefCell::new(None) };
    static TESTE: RefCell<Option<TesteDoMicrofone>> = const { RefCell::new(None) };
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
/// (`seele_proto::uri::ORIGEM_DO_NAVEGADOR`, a D-web-2). Devolve 0
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

// ------------------------------------------------------------- a ligação

fn dizer_os_passos(passos: &[Passo]) -> u32 {
    let passos: Vec<serde_json::Value> = passos.iter().map(Passo::em_json).collect();
    dizer(
        serde_json::json!({ "passos": passos })
            .to_string()
            .into_bytes(),
        0,
    )
}

fn sem_ligacao() -> u32 {
    dizer(br#"{"erro":"sem-ligacao"}"#.to_vec(), 1)
}

/// O que fazer com a ligação desta página, se houver uma.
fn com_a_ligacao(fazer: impl FnOnce(&mut Ligacao) -> u32) -> u32 {
    LIGACAO.with(|l| match l.borrow_mut().as_mut() {
        Some(ligacao) => fazer(ligacao),
        None => sem_ligacao(),
    })
}

/// Abre a ligação desta página, no lugar de qualquer outra: o pedido em JSON
/// ([`Pedido::de_json`]). Devolve 0 com o `Hello` esperando em
/// [`ligacao_para_o_fio`], ou 1 com `{"recusa": ConnectionError}` (o apelido
/// grande demais, antes de discar) ou `{"erro": "json", "texto": …}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn ligacao_abrir(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    LIGACAO.with(|l| *l.borrow_mut() = None);
    let pedido = std::str::from_utf8(bytes)
        .map_err(|e| e.to_string())
        .and_then(Pedido::de_json);
    let pedido = match pedido {
        Ok(pedido) => pedido,
        Err(texto) => return erro(&ErroDaPonte::Json(texto)),
    };
    match Ligacao::abrir(pedido) {
        Ok(ligacao) => {
            LIGACAO.with(|l| *l.borrow_mut() = Some(ligacao));
            dizer(Vec::new(), 0)
        }
        Err(recusa) => dizer(
            serde_json::json!({ "recusa": recusa })
                .to_string()
                .into_bytes(),
            1,
        ),
    }
}

/// Bytes do fluxo de controle. Devolve 0 com `{"passos": […]}`
/// ([`Passo::em_json`]), ou 1 com `{"erro": "sem-ligacao"}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn ligacao_chegou(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    com_a_ligacao(|ligacao| dizer_os_passos(&ligacao.chegou(bytes)))
}

/// A assinatura da página sobre o [`Passo::Assinar`]. Como
/// [`ligacao_chegou`].
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn ligacao_assinado(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let assinatura = unsafe { entrada(ptr, len) };
    com_a_ligacao(|ligacao| dizer_os_passos(&ligacao.assinado(assinatura)))
}

/// O fluxo de controle acabou. Como [`ligacao_chegou`].
#[no_mangle]
pub extern "C" fn ligacao_fluxo_acabou() -> u32 {
    com_a_ligacao(|ligacao| dizer_os_passos(&ligacao.fluxo_acabou()))
}

/// Os bytes que esperam para ir ao fluxo de controle (0, e os bytes na saída;
/// vazia quando não há nada), ou 1 sem ligação.
#[no_mangle]
pub extern "C" fn ligacao_para_o_fio() -> u32 {
    com_a_ligacao(|ligacao| dizer(ligacao.para_o_fio(), 0))
}

/// O que sai do `Room` para o retrato: 0 com `{"sala": RetratoDaSala,
/// "contados": {…}}`, ou 1 com `{"erro": "sem-sessao"}` antes da entrada (e
/// `"sem-ligacao"` sem ligação).
#[no_mangle]
pub extern "C" fn ligacao_retrato() -> u32 {
    com_a_ligacao(|ligacao| match ligacao.sessao() {
        Some(sessao) => {
            let (perda, sala) = sessao.foto();
            match serde_json::to_value(sala) {
                Ok(sala) => dizer(
                    serde_json::json!({
                        "sala": sala,
                        "contados": sessao.contados().em_json(),
                        "telemetria": ligacao.telemetria(perda),
                    })
                    .to_string()
                    .into_bytes(),
                    0,
                ),
                Err(e) => erro(&ErroDaPonte::Json(e.to_string())),
            }
        }
        None => dizer(br#"{"erro":"sem-sessao"}"#.to_vec(), 1),
    })
}

/// A semente das chaves das mensagens desta página: só a primeira vale, e a
/// contagem segue pelas ligações seguintes (ver [`DaPagina`]).
#[no_mangle]
pub extern "C" fn ligacao_semente(semente: u32) {
    PAGINA.with(|p| {
        p.borrow_mut()
            .get_or_insert_with(|| DaPagina::nova(semente));
    });
}

/// Um comando da casca: `{"comando", "args", "agora_s"}`. Devolve 0 com
/// `{"resposta", "eventos", "nao_saiu"}`, 1 com `{"erro": …}` (o JSON do erro
/// do desktop), ou 2 quando o comando não é da sessão (a ponte o atende).
/// Sem semente, as chaves começam do zero alto: a página chama
/// [`ligacao_semente`] antes.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn ligacao_atender(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let pedido: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(pedido) => pedido,
        Err(e) => return erro(&ErroDaPonte::Json(e.to_string())),
    };
    let comando = pedido
        .get("comando")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let args = pedido
        .get("args")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let agora_s = pedido
        .get("agora_s")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default();
    let atendido = LIGACAO.with(|l| {
        PAGINA.with(|p| {
            let mut pagina = p.borrow_mut();
            let pagina = pagina.get_or_insert_with(|| DaPagina::nova(0));
            crate::comandos::atender(l.borrow_mut().as_mut(), comando, &args, pagina, agora_s)
        })
    });
    match atendido {
        Some(Ok(feito)) => dizer(feito.em_json().to_string().into_bytes(), 0),
        Some(Err(motivo)) => dizer(
            serde_json::json!({ "erro": motivo })
                .to_string()
                .into_bytes(),
            1,
        ),
        None => dizer(br#"{"nao_e_da_sessao":true}"#.to_vec(), 2),
    }
}

/// Uma volta da medida ([`Ligacao::medir`]): o tempo de ida e volta em
/// microssegundos, o jitter de chegada em milissegundos e a pior perda da voz.
/// Devolve 1 quando o que a casca mostra mudou, e 0 quando não (ou sem
/// ligação).
#[no_mangle]
pub extern "C" fn ligacao_medir(rtt_micros: u64, jitter_ms: f32, perda: f32) -> u32 {
    LIGACAO.with(|l| {
        l.borrow_mut().as_mut().map_or(0, |ligacao| {
            u32::from(ligacao.medir(rtt_micros, jitter_ms, perda))
        })
    })
}

/// **A prévia de um anexo** ([`crate::previa`]): o pedido em JSON
/// (`{"attachment", "claimed", "chegada": "bytes" | "nao_veio" |
/// "grande_demais"}`) e os bytes que chegaram. Devolve 0 com o `Preview` do
/// desktop, ou 1 com `{"erro": "json", …}`.
///
/// # Safety
///
/// Os dois pares `ptr`/`len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn previa_julgar(
    pedido_ptr: *const u8,
    pedido_len: usize,
    bytes_ptr: *const u8,
    bytes_len: usize,
) -> u32 {
    use crate::previa::{previa_do_anexo, Chegada};
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = unsafe { entrada(pedido_ptr, pedido_len) };
    // SAFETY: idem.
    let bytes = unsafe { entrada(bytes_ptr, bytes_len) };
    let pedido: serde_json::Value = match serde_json::from_slice(pedido) {
        Ok(pedido) => pedido,
        Err(e) => return erro(&ErroDaPonte::Json(e.to_string())),
    };
    let anexo = pedido
        .get("attachment")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default();
    let alegado = pedido
        .get("claimed")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let chegada = match pedido.get("chegada").and_then(serde_json::Value::as_str) {
        Some("bytes") => Chegada::Bytes(bytes),
        Some("grande_demais") => Chegada::GrandeDemais,
        _ => Chegada::NaoVeio,
    };
    dizer(
        previa_do_anexo(anexo, alegado, chegada)
            .to_string()
            .into_bytes(),
        0,
    )
}

/// O cabeçalho de um quadro da tela que sobe ([`crate::tela::cabecalho`]):
/// devolve 0 com os cinco bytes na saída, ou 1 para um tipo que não existe.
#[no_mangle]
pub extern "C" fn tela_quadro(tipo: u32, tamanho: u32) -> u32 {
    match crate::tela::cabecalho(tipo, tamanho) {
        Some(cabecalho) => dizer(cabecalho.to_vec(), 0),
        None => dizer(br#"{"erro":"tipo"}"#.to_vec(), 1),
    }
}

// ------------------------------------------- os fluxos que o servidor abre

fn dizer_os_passos_do_fluxo(passos: &[crate::fluxos::Passo]) -> u32 {
    let passos: Vec<serde_json::Value> = passos.iter().map(crate::fluxos::Passo::em_json).collect();
    dizer(
        serde_json::json!({ "passos": passos })
            .to_string()
            .into_bytes(),
        0,
    )
}

/// Um fluxo unidirecional que o servidor abriu: devolve a alça dele.
#[no_mangle]
pub extern "C" fn fluxo_novo() -> u32 {
    FLUXOS.with(|f| {
        let mut fluxos = f.borrow_mut();
        fluxos.0 = fluxos.0.wrapping_add(1);
        let alca = fluxos.0;
        fluxos.1.insert(alca, FluxoQueChega::novo());
        alca
    })
}

/// Um pedaço do fluxo `alca`. Devolve 0 com `{"passos": […]}`
/// ([`crate::fluxos::Passo::em_json`]), ou 1 com `{"erro": "sem-fluxo"}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn fluxo_chegou(alca: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedaco = unsafe { entrada(ptr, len) };
    let passos = FLUXOS.with(|f| {
        f.borrow_mut()
            .1
            .get_mut(&alca)
            .map(|fluxo| fluxo.chegou(pedaco))
    });
    match passos {
        Some(passos) => dizer_os_passos_do_fluxo(&passos),
        None => dizer(br#"{"erro":"sem-fluxo"}"#.to_vec(), 1),
    }
}

/// O fluxo `alca` acabou: os últimos passos, e a alça é esquecida.
#[no_mangle]
pub extern "C" fn fluxo_acabou(alca: u32) -> u32 {
    let passos = FLUXOS.with(|f| {
        f.borrow_mut()
            .1
            .remove(&alca)
            .map(|mut fluxo| fluxo.acabou())
    });
    match passos {
        Some(passos) => dizer_os_passos_do_fluxo(&passos),
        None => dizer(br#"{"erro":"sem-fluxo"}"#.to_vec(), 1),
    }
}

/// O enlace caiu, e a página vai tentar voltar: a sessão fica guardada.
/// Devolve 0 com os eventos (`[…]`, o JSON do `seele_ffi::Event`).
#[no_mangle]
pub extern "C" fn ligacao_largar() -> u32 {
    let eventos = LIGACAO.with(|l| {
        l.borrow_mut()
            .as_mut()
            .map(Ligacao::largar_para_voltar)
            .unwrap_or_default()
    });
    dizer(serde_json::json!(eventos).to_string().into_bytes(), 0)
}

/// **A volta**: o pedido de [`ligacao_abrir`] e as escolhas
/// (`{"mudo", "isolado"}`), com a sessão guardada. Devolve 0 com o `Hello`
/// esperando em [`ligacao_para_o_fio`], ou 1 com `{"recusa": …}` ou `{"erro":
/// …}`.
///
/// # Safety
///
/// Os dois pares `ptr`/`len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn ligacao_retomar(
    pedido_ptr: *const u8,
    pedido_len: usize,
    escolhas_ptr: *const u8,
    escolhas_len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = unsafe { entrada(pedido_ptr, pedido_len) };
    // SAFETY: idem.
    let escolhas = unsafe { entrada(escolhas_ptr, escolhas_len) };
    let pedido = match std::str::from_utf8(pedido)
        .map_err(|e| e.to_string())
        .and_then(Pedido::de_json)
    {
        Ok(pedido) => pedido,
        Err(texto) => return erro(&ErroDaPonte::Json(texto)),
    };
    let escolhas: serde_json::Value = serde_json::from_slice(escolhas).unwrap_or_default();
    let escolhas = crate::sessao::Escolhas {
        mudo: escolhas
            .get("mudo")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        isolado: escolhas
            .get("isolado")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    };
    com_a_ligacao(|ligacao| match ligacao.retomar(pedido, escolhas) {
        Ok(()) => dizer(Vec::new(), 0),
        Err(recusa) => dizer(
            serde_json::json!({ "recusa": recusa })
                .to_string()
                .into_bytes(),
            1,
        ),
    })
}

// ----------------------------------------------------------- a bateria

/// O relógio da página em `Duration`: os milissegundos do `performance.now()`.
fn agora_da_pagina(ms: f64) -> std::time::Duration {
    std::time::Duration::from_secs_f64((ms / 1000.0).max(0.0))
}

fn com_a_bateria<T>(fazer: impl FnOnce(&mut seele_sessao::battery::Battery) -> T) -> T {
    BATERIA.with(|b| {
        fazer(
            b.borrow_mut()
                .get_or_insert_with(seele_sessao::battery::Battery::new),
        )
    })
}

/// Uma bateria nova, para uma sessão nova.
#[no_mangle]
pub extern "C" fn bateria_nova() {
    BATERIA.with(|b| *b.borrow_mut() = Some(seele_sessao::battery::Battery::new()));
}

/// **A bateria do desktop** (`seele_sessao::battery`), no relógio da página: o
/// que fazer agora. 0 esperar, 1 mandar o `Ping` de rotina, 2 tentar voltar, 3
/// encerrar (a bateria descarregou).
#[no_mangle]
pub extern "C" fn bateria_poll(agora_ms: f64) -> u32 {
    use seele_sessao::battery::Action;
    match com_a_bateria(|bateria| bateria.poll(agora_da_pagina(agora_ms))) {
        Action::Wait => 0,
        Action::SendPing => 1,
        Action::Reconnect => 2,
        Action::EndSession => 3,
    }
}

/// Chegou o `Pong` de um `Ping` de rotina.
#[no_mangle]
pub extern "C" fn bateria_pong() {
    com_a_bateria(seele_sessao::battery::Battery::on_pong);
}

/// O enlace caiu (a conexão fechou antes de a bateria perceber).
#[no_mangle]
pub extern "C" fn bateria_caiu(agora_ms: f64) {
    com_a_bateria(|bateria| bateria.on_connection_lost(agora_da_pagina(agora_ms)));
}

/// A volta deu certo.
#[no_mangle]
pub extern "C" fn bateria_voltou() {
    com_a_bateria(seele_sessao::battery::Battery::on_reconnected);
}

/// A tentativa de voltar falhou: a próxima espera o recuo.
#[no_mangle]
pub extern "C" fn bateria_falhou(agora_ms: f64) {
    com_a_bateria(|bateria| bateria.on_reconnect_failed(agora_da_pagina(agora_ms)));
}

/// O enlace como o `Snapshot` o diz: 0 com `"Online"` ou
/// `{"InternalBattery": {"remaining_seconds", "attempts"}}`. A bateria que
/// descarregou é `"Online"`, como o `gravar_enlace` da FFI: o `LinkState` do
/// desktop não tem o terceiro estado, e a sessão acaba logo em seguida.
#[no_mangle]
pub extern "C" fn bateria_enlace(agora_ms: f64) -> u32 {
    use seele_sessao::battery::Link;
    let agora = agora_da_pagina(agora_ms);
    let enlace = com_a_bateria(|bateria| match bateria.state() {
        Link::InternalBattery { attempts } => serde_json::json!({ "InternalBattery": {
            "remaining_seconds": bateria.remaining(agora).map_or(0, |resta| resta.as_secs()),
            "attempts": attempts,
        } }),
        Link::Online | Link::Discharged => serde_json::json!("Online"),
    });
    dizer(enlace.to_string().into_bytes(), 0)
}

/// Esquece a ligação desta página.
#[no_mangle]
pub extern "C" fn ligacao_fechar() {
    LIGACAO.with(|l| *l.borrow_mut() = None);
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
        dizer_os_quadros(&envio.capturou(&quadro, mudo != 0))
    })
}

/// Os quadros soltos pelo portão na saída: o carimbo e as amostras de cada um.
fn dizer_os_quadros(saem: &[(u32, Vec<f32>)]) -> u32 {
    let mut saida = Vec::new();
    for (carimbo, pcm) in saem {
        saida.extend_from_slice(&carimbo.to_le_bytes());
        for amostra in pcm {
            saida.extend_from_slice(&amostra.to_le_bytes());
        }
    }
    dizer(saida, u32::try_from(saem.len()).unwrap_or(u32::MAX))
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

/// **O ganho de cada voz com volume escolhido, pela alça da recepção**: o
/// `seguir_os_volumes` do desktop, com a SSRC traduzida aqui dentro — a página
/// recebe a alça, que é o que a mistura conhece, e nunca a SSRC. Quem ainda não
/// falou nesta recepção não tem alça, e fica de fora até falar. Devolve 0 com
/// `[[alça, ganho], …]`.
#[no_mangle]
pub extern "C" fn voz_ganhos() -> u32 {
    let ganhos = LIGACAO.with(|l| {
        l.borrow()
            .as_ref()
            .and_then(Ligacao::sessao)
            .map(crate::sessao::Sessao::ganhos)
            .unwrap_or_default()
    });
    let por_alca: Vec<(u32, f32)> = RECEPCAO.with(|r| {
        let recepcao = r.borrow();
        ganhos
            .into_iter()
            .filter_map(|(ssrc, ganho)| {
                recepcao
                    .as_ref()
                    .and_then(|recepcao| recepcao.alca_de(ssrc))
                    .map(|alca| (alca, ganho))
            })
            .collect()
    });
    dizer(serde_json::json!(por_alca).to_string().into_bytes(), 0)
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

// ------------------------------------------------- o teste do microfone

/// Abre o teste do microfone com a régua dos controles guardados. Um teste
/// aberto antes é trocado, como no desktop.
#[no_mangle]
pub extern "C" fn voz_teste_abrir(abertura_milesimos: i32, com_supressao: u32) {
    TESTE.with(|t| {
        *t.borrow_mut() = Some(TesteDoMicrofone::abrir(
            abertura_milesimos,
            com_supressao != 0,
        ));
    });
}

/// Um quadro de 20 ms do microfone para o teste: quantos quadros o portão
/// soltou, que são os que o monitor toca.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn voz_teste_quadro(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let Some(quadro) = amostras(bytes) else {
        erro_da_voz("o quadro não é de f32");
        return u32::MAX;
    };
    TESTE.with(|t| match t.borrow_mut().as_mut() {
        Some(teste) => dizer_os_quadros(&teste.quadro(&quadro)),
        None => {
            erro_da_voz("não há teste do microfone aberto");
            u32::MAX
        }
    })
}

/// O que o teste do microfone tem a contar.
#[no_mangle]
pub extern "C" fn voz_teste_estado() -> u32 {
    TESTE.with(|t| match t.borrow().as_ref() {
        Some(teste) => {
            let e = teste.estado();
            dizer(
                serde_json::json!({
                    "nivel_dbfs": e.nivel_dbfs,
                    "aberto": e.aberto,
                    "aberturas": e.aberturas,
                    "quadros_retidos": e.quadros_retidos,
                    "corte_dbfs": e.corte_dbfs,
                })
                .to_string()
                .into_bytes(),
                0,
            )
        }
        None => erro_da_voz("não há teste do microfone aberto"),
    })
}

/// Fecha o teste do microfone. Idempotente.
#[no_mangle]
pub extern "C" fn voz_teste_fechar() {
    TESTE.with(|t| *t.borrow_mut() = None);
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
    /// F1 usam `origem.invalid`, e a do produto é a `ORIGEM_DO_NAVEGADOR`.
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
