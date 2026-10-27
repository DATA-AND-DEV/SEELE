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
//! | `ligacao_fluxo_acabou_com(codigo)` | 0 | `{"passos": […]}`: com o `CODIGO_FICOU_PARA_TRAS`, antes a despedida `FellBehind` |
//! | `ligacao_para_o_fio()` | 0 | os bytes enquadrados (talvez nenhum) |
//! | `ligacao_retrato()` | 0 | `{"sala": RetratoDaSala, "contados": {…}, "telemetria": {…}}` |
//! | | 1 | `{"erro": "sem-sessao"}` |
//! | `ligacao_medir(rtt_micros, jitter_ms, perda)` | 1 se mudou, 0 se não | — |
//! | `ligacao_semente(semente)` | — | — |
//! | `ligacao_atender(pedido)` | 0 | `{"resposta", "eventos", "nao_saiu"}` |
//! | | 1 | `{"erro": …}`, o erro do desktop |
//! | | 2 | `{"nao_e_da_sessao": true}` |
//! | `ligacao_vez(agora_ms, agora_s)` | 0 | `{"eventos", "passos"}`: a vez do buraco e da repetição |
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
//! **Os MODs da sessão** (Tarefa 2.6; [`crate::mods`]): a página entrega os
//! pacotes que guardou (ou os bytes que baixou), um arquivo por vez, e o
//! comando lê o último entregue (ou todos, a lista) e os esquece.
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `pacotes_limpar()` | — | — |
//! | `pacote_novo(lugar)`, `pacote_arquivo(caminho, bytes)` | — | — |
//! | `mods_na_tela()` | 0 | a lista do `mods_instalados` do desktop |
//! | `mod_codigo(pedido)` | 0 | `{"fonte", "capacidades"}` |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `mod_midia(pedido)` | 0 | o `MidiaDoMod` do desktop |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `mod_som(pedido)` | 0 | os bytes crus do som |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `mod_midia_do_servidor(texto, uso)` | 0 | o `MidiaDoMod` do desktop |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `escolha_regras(pedido)` | 0 | `{"titulo", "extensoes", "teto"}` |
//! | `escolha_guardar(pedido, comeco)` | 0 | `{"id", "tipo", "papel", "bytes"}` |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `escolha_pedaco(pedido)` | 0 | `{"inicio", "fim"}` |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `escolha_soltar(pedido)` | 0 | `{"soltou"}` |
//! | `escolha_tirar(pedido)` | 0 | `{"tamanho"}` |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `escolhas_soltar_geracao(pedido)` | 0 | os números soltos |
//! | `base64_de(bytes)` | 0 | o texto em base64 |
//! | `volume_pedido(pedido)` | 0 | o começo do fluxo de imagem |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `volume_prazo()` | 0 | `{"ms", "motivo"}` |
//! | `volume_resposta_nova()` | a alça | — |
//! | `volume_resposta_chegou(alca, pedaco)` | — | — |
//! | `volume_resposta_acabou(alca, imagem, viva, uso)` | 0 | o `MidiaDoMod` (`imagem` 1) ou nada |
//! | | 1 | `{"Recusado": {"motivo"}}` |
//! | `catalogo_endereco()` | 0 | o endereço do catálogo |
//! | `catalogo_ler(catalogo, assinatura)` | 0 | nada: conferido e lido |
//! | | 1 | a `FalhaNoCatalogo` do desktop |
//! | `catalogo_planejar(catalogo, assinatura, revogacoes, assinatura, pedido)` | 0 | `{"arquivos": [[caminho, url], …], "hash"}` |
//! | | 1 | a `FalhaNoCatalogo` do desktop |
//! | `catalogo_conferir(hash)` | 0 | `{"id", "hash"}` |
//! | | 1 | a `FalhaNoCatalogo` do desktop |
//!
//! **A voz que sai e a que chega** (Tarefa 3.3, lotes A e C; [`crate::voz`]),
//! na instância da página, uma de cada lado:
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `voz_envio(ssrc, agora_ms)` | — | — |
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
    static PACOTES: RefCell<Vec<crate::mods::Guardado>> = const { RefCell::new(Vec::new()) };
    static CHAVE_DO_CATALOGO: RefCell<Option<String>> = const { RefCell::new(None) };
    static ESCOLHIDOS: RefCell<Option<seele_sessao::escolhidos::Escolhidos<()>>> =
        const { RefCell::new(None) };
    static RESPOSTAS: RefCell<(u32, std::collections::BTreeMap<u32, crate::volume::Resposta>)> =
        const { RefCell::new((0, std::collections::BTreeMap::new())) };
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
/// (`seele_proto::uri::ORIGEM_DO_NAVEGADOR`, a D-web-2, ou uma das anteriores,
/// `ORIGENS_LEGADAS_DO_NAVEGADOR`). Devolve 0
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
    // A origem do produto que o link TRAZ: a atual, ou uma anterior exatamente (a troca de domínio da 0.17, com
    // compatibilidade; `seele_proto::uri::origem_do_link`). Qualquer outra continua recusada.
    dizer_o_link(&crate::link_do_navegador(
        texto,
        seele_proto::uri::origem_do_link(texto),
    ))
}

/// Intenções locais e verificações estritas da chamada pessoal, em JSON.
///
/// # Safety
/// `ptr..ptr+len` deve ser uma região válida da memória linear, como em `alocar`.
#[no_mangle]
pub unsafe extern "C" fn chamada_operar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de entrada.
    let bytes = unsafe { entrada(ptr, len) };
    let lido = crate::chamada::operar(std::str::from_utf8(bytes).unwrap_or(""));
    let codigo = u32::from(lido.get("erro").is_some());
    dizer(lido.to_string().into_bytes(), codigo)
}

/// O corpo do `POST /sinal/1` (contrato da 0.17, D7): `pedido` é
/// `{"impressao", "credencial", "candidatos"}`. Devolve 0 com `{"corpo": …}`,
/// ou 1 com `{"erro": …}`. Ver [`crate::sinal_chama`].
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn sinal_chama(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let lido = crate::sinal_chama(std::str::from_utf8(bytes).unwrap_or(""));
    let codigo = u32::from(lido.get("corpo").is_none());
    dizer(lido.to_string().into_bytes(), codigo)
}

/// A resposta do ponto ao `POST /sinal/1`, lida. Devolve 0 com o JSON de
/// [`crate::sinal_resposta`], ou 1 com `{"erro": "ilegivel"}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn sinal_resposta(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let lido = crate::sinal_resposta(std::str::from_utf8(bytes).unwrap_or(""));
    let codigo = u32::from(lido.get("erro").is_some());
    dizer(lido.to_string().into_bytes(), codigo)
}

// ------------------------------------------------- os cargos (ADR 0064, de F)
//
// O núcleo é `crate::cargos` (de F). Estas fronteiras só leem UM objeto JSON
// por chamada — os dados DAQUELA sessão, que a página guardou dela — e
// devolvem o JSON do núcleo. Nada aqui guarda quadro: duas sessões na mesma
// página nunca se misturam porque cada chamada traz o seu. Um objeto só, e não
// `u64` soltos, porque um `i64` do wasm vira `BigInt` no JavaScript.

/// O campo `nome` do objeto, de volta em texto JSON (`null` quando falta).
fn campo_em_json(objeto: &serde_json::Value, nome: &str) -> String {
    objeto
        .get(nome)
        .map_or_else(|| "null".to_owned(), serde_json::Value::to_string)
}

/// O campo numérico `nome`, ou a recusa nomeada.
fn campo_numero(objeto: &serde_json::Value, nome: &str) -> Result<u64, serde_json::Value> {
    objeto
        .get(nome)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| serde_json::json!({ "erro": format!("falta o número «{nome}»") }))
}

/// A entrada como objeto JSON, ou a recusa nomeada.
fn objeto_de(bytes: &[u8]) -> Result<serde_json::Value, serde_json::Value> {
    match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(objeto) if objeto.is_object() => Ok(objeto),
        Ok(_) => Err(serde_json::json!({ "erro": "a entrada não é um objeto JSON" })),
        Err(e) => Err(serde_json::json!({ "erro": e.to_string() })),
    }
}

/// 0 com a resposta do núcleo, ou 1 quando ela traz `erro` ou `recusa` (a
/// mesma forma, `{"recusa": RecusaDeCargo}`, que o servidor diria).
fn dizer_dos_cargos(resposta: &serde_json::Value) -> u32 {
    let codigo = u32::from(resposta.get("erro").is_some() || resposta.get("recusa").is_some());
    dizer(resposta.to_string().into_bytes(), codigo)
}

fn cargos_prever_de(bytes: &[u8]) -> serde_json::Value {
    let lido = objeto_de(bytes).and_then(|o| {
        let autor = campo_numero(&o, "autor")?;
        let revisao = campo_numero(&o, "revisao")?;
        Ok((o, autor, revisao))
    });
    match lido {
        Ok((o, autor, revisao)) => crate::cargos::prever_na_sessao(
            &campo_em_json(&o, "quadro"),
            &campo_em_json(&o, "membros"),
            autor,
            revisao,
            &campo_em_json(&o, "pedido"),
        ),
        Err(recusa) => recusa,
    }
}

fn cargos_explicar_de(bytes: &[u8]) -> serde_json::Value {
    let lido = objeto_de(bytes).and_then(|o| Ok((campo_numero(&o, "pessoa")?, o)));
    match lido {
        Ok((pessoa, o)) => crate::cargos::explicar_na_sessao(
            &campo_em_json(&o, "quadro"),
            &campo_em_json(&o, "membros"),
            pessoa,
        ),
        Err(recusa) => recusa,
    }
}

fn cargos_na_tela_de(bytes: &[u8]) -> serde_json::Value {
    match objeto_de(bytes) {
        Ok(o) => crate::cargos::na_tela(
            &campo_em_json(&o, "quadro"),
            &campo_em_json(&o, "suas"),
            &campo_em_json(&o, "membros"),
        ),
        Err(recusa) => recusa,
    }
}

fn cargos_comando_de(bytes: &[u8]) -> serde_json::Value {
    let lido = objeto_de(bytes).and_then(|o| Ok((campo_numero(&o, "revisao")?, o)));
    match lido {
        Ok((revisao, o)) => crate::cargos::comando(revisao, &campo_em_json(&o, "pedido")),
        Err(recusa) => recusa,
    }
}

/// `prever_cargo` da página: a entrada é
/// `{"quadro", "membros", "autor", "revisao", "pedido"}` daquela sessão; a
/// saída, a de [`crate::cargos::prever_na_sessao`] (0 com `{"previa"}`, 1 com
/// `{"recusa"}` ou `{"erro"}`).
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn cargos_prever(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    dizer_dos_cargos(&cargos_prever_de(bytes))
}

/// `explicar_permissoes` da página: a entrada é `{"quadro", "membros",
/// "pessoa"}`; a saída, a de [`crate::cargos::explicar_na_sessao`].
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn cargos_explicar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    dizer_dos_cargos(&cargos_explicar_de(bytes))
}

/// `pedir_cargos` da página (o que a tela lê): a entrada é `{"quadro",
/// "suas", "membros"}` (`suas` pode faltar); a saída, o `CargosNaTela` de
/// [`crate::cargos::na_tela`].
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn cargos_na_tela(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    dizer_dos_cargos(&cargos_na_tela_de(bytes))
}

/// Os seis verbos: a entrada é `{"revisao", "pedido": PedidoDeCargo}`; a
/// saída, a `ClientMessage` em JSON para o [`codificar`] de sempre.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn cargos_comando(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    dizer_dos_cargos(&cargos_comando_de(bytes))
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

/// O fluxo de controle acabou com o código do fechamento (o `0x02 codigo` do
/// `ctl`): o `CODIGO_FICOU_PARA_TRAS` vira a despedida `FellBehind`, uma vez
/// por conexão; qualquer outro, a queda de sempre. Como [`ligacao_chegou`].
#[no_mangle]
pub extern "C" fn ligacao_fluxo_acabou_com(codigo: u32) -> u32 {
    com_a_ligacao(|ligacao| dizer_os_passos(&ligacao.fluxo_acabou_com(codigo)))
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

/// **A vez do relógio da página** ([`Ligacao::vez`]), a cada volta da medida:
/// `agora_ms` é o `performance.now()`, e `agora_s` os segundos desde 1970 (a
/// hora de uma pendente repetida). O que sai fica em [`ligacao_para_o_fio`].
/// Devolve 0 com `{"eventos": […], "passos": […]}` (os avisos do buraco).
#[no_mangle]
pub extern "C" fn ligacao_vez(agora_ms: f64, agora_s: f64) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "segundos desde 1970 cabem folgados num i64"
    )]
    let segundos = agora_s as i64;
    let (eventos, passos) = LIGACAO.with(|l| {
        l.borrow_mut()
            .as_mut()
            .map(|ligacao| ligacao.vez(agora_da_pagina(agora_ms), segundos))
            .unwrap_or_default()
    });
    let passos: Vec<serde_json::Value> = passos.iter().map(Passo::em_json).collect();
    dizer(
        serde_json::json!({ "eventos": eventos, "passos": passos })
            .to_string()
            .into_bytes(),
        0,
    )
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

/// A tela que o fluxo abriu ou fechou, contada à sessão: a que o servidor liga
/// sozinho entra no retrato (`Sessao::ouvir_o_fluxo`).
fn a_sessao_ouve_o_fluxo(passos: &[crate::fluxos::Passo], alca: u32) {
    LIGACAO.with(|l| {
        if let Some(sessao) = l.borrow_mut().as_mut().and_then(Ligacao::sessao_mut) {
            sessao.ouvir_o_fluxo(passos, alca);
        }
    });
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
    if let Some(passos) = passos.as_deref() {
        a_sessao_ouve_o_fluxo(passos, alca);
    }
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
    if let Some(passos) = passos.as_deref() {
        a_sessao_ouve_o_fluxo(passos, alca);
    }
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

// ------------------------------------------------------ os MODs da sessão

/// O texto de uma entrada, ou vazio quando ele não é UTF-8.
fn texto_de(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or_default()
}

/// A recusa de um comando de MOD, no `FalhaNoMod` do desktop.
fn recusa_de_mod(motivo: &str) -> u32 {
    dizer(
        serde_json::json!({ "Recusado": { "motivo": motivo } })
            .to_string()
            .into_bytes(),
        1,
    )
}

/// Esquece os pacotes que a página entregou.
#[no_mangle]
pub extern "C" fn pacotes_limpar() {
    PACOTES.with(|p| p.borrow_mut().clear());
}

/// Começa um pacote guardado, sob o lugar (o hash) em que a página o guardou.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn pacote_novo(ptr: *const u8, len: usize) {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let lugar = texto_de(unsafe { entrada(ptr, len) }).to_owned();
    PACOTES.with(|p| p.borrow_mut().push((lugar, Vec::new())));
}

/// Um arquivo do último pacote começado: o caminho de dentro, e os bytes.
///
/// # Safety
///
/// Os dois pares `ptr`/`len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn pacote_arquivo(
    caminho_ptr: *const u8,
    caminho_len: usize,
    bytes_ptr: *const u8,
    bytes_len: usize,
) {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let caminho = texto_de(unsafe { entrada(caminho_ptr, caminho_len) }).to_owned();
    // SAFETY: idem.
    let bytes = unsafe { entrada(bytes_ptr, bytes_len) }.to_vec();
    PACOTES.with(|p| {
        if let Some((_, arquivos)) = p.borrow_mut().last_mut() {
            arquivos.push((caminho, bytes));
        }
    });
}

/// **O `mods_instalados`** ([`crate::mods::na_tela`]): os pacotes entregues,
/// como a janela os desenha. Devolve 0 com a lista, e esquece os pacotes.
#[no_mangle]
pub extern "C" fn mods_na_tela() -> u32 {
    let guardados = PACOTES.with(|p| std::mem::take(&mut *p.borrow_mut()));
    dizer(crate::mods::na_tela(guardados).to_string().into_bytes(), 0)
}

/// O último pacote entregue, para um comando que lê um só.
fn o_pacote() -> Vec<(String, Vec<u8>)> {
    PACOTES.with(|p| {
        std::mem::take(&mut *p.borrow_mut())
            .pop()
            .map(|(_, arquivos)| arquivos)
            .unwrap_or_default()
    })
}

/// **O `codigo_do_mod`** ([`crate::mods::codigo`]): `{"id", "hash"}` sobre o
/// último pacote entregue. Devolve 0 com `{"fonte", "capacidades"}`, ou 1 com
/// `{"Recusado": {"motivo"}}`; e esquece os pacotes.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn mod_codigo(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido: serde_json::Value =
        serde_json::from_slice(unsafe { entrada(ptr, len) }).unwrap_or_default();
    let texto = |campo: &str| {
        pedido
            .get(campo)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match crate::mods::codigo(&mut o_pacote(), &texto("id"), &texto("hash")) {
        Ok(codigo) => dizer(codigo.to_string().into_bytes(), 0),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// Os campos de texto de um pedido em JSON (`{"id", "hash", "caminho"}`), vazios
/// quando faltam.
fn campos_do_pedido<const N: usize>(bytes: &[u8], nomes: [&str; N]) -> [String; N] {
    let pedido: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
    nomes.map(|nome| {
        pedido
            .get(nome)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    })
}

/// **O `midia_do_mod`** ([`crate::mods::midia`]): `{"id", "hash", "caminho"}`
/// sobre o último pacote entregue. Devolve 0 com o `MidiaDoMod` do desktop
/// (sem `uri` para o som), ou 1 com `{"Recusado": {"motivo"}}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn mod_midia(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let [id, hash, caminho] =
        campos_do_pedido(unsafe { entrada(ptr, len) }, ["id", "hash", "caminho"]);
    match crate::mods::midia(&mut o_pacote(), &id, &hash, &caminho) {
        Ok(midia) => dizer(midia.to_string().into_bytes(), 0),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// **O `som_do_mod`** ([`crate::mods::som`]): devolve 0 com os bytes crus do
/// som, ou 1 com `{"Recusado": {"motivo"}}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn mod_som(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let [id, hash, caminho] =
        campos_do_pedido(unsafe { entrada(ptr, len) }, ["id", "hash", "caminho"]);
    match crate::mods::som(&mut o_pacote(), &id, &hash, &caminho) {
        Ok(bytes) => dizer(bytes, 0),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// **O `midia_em_bytes`** ([`crate::mods::midia_do_servidor`]): o texto como o
/// servidor o deu (base64, ou um `data:`), e o uso (vazio para nenhum).
/// Devolve 0 com o `MidiaDoMod` do desktop, ou 1 com `{"Recusado": {"motivo"}}`.
///
/// # Safety
///
/// Os dois pares `ptr`/`len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn mod_midia_do_servidor(
    texto_ptr: *const u8,
    texto_len: usize,
    uso_ptr: *const u8,
    uso_len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let texto = texto_de(unsafe { entrada(texto_ptr, texto_len) });
    // SAFETY: idem.
    let uso = texto_de(unsafe { entrada(uso_ptr, uso_len) });
    match crate::mods::midia_do_servidor(texto, (!uso.is_empty()).then_some(uso)) {
        Ok(midia) => dizer(midia.to_string().into_bytes(), 0),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

// ------------------------------------------- o seletor e as imagens pelo volume

/// O pedido em JSON, para os campos que não são texto.
fn pedido_em_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap_or_default()
}

/// Um número do pedido (`u64`), zero quando falta.
fn numero_do_pedido(pedido: &serde_json::Value, campo: &str) -> u64 {
    pedido
        .get(campo)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default()
}

/// Uma lista de textos do pedido, `None` quando falta.
fn lista_do_pedido(pedido: &serde_json::Value, campo: &str) -> Option<Vec<String>> {
    pedido
        .get(campo)
        .and_then(serde_json::Value::as_array)
        .map(|lista| {
            lista
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
}

fn com_os_escolhidos<T>(
    fazer: impl FnOnce(&mut seele_sessao::escolhidos::Escolhidos<()>) -> T,
) -> T {
    ESCOLHIDOS.with(|e| fazer(e.borrow_mut().get_or_insert_with(Default::default)))
}

/// **As regras do seletor** ([`crate::mods::regras_do_seletor`]):
/// `{"finalidade", "extensoes", "limiteDeBytes"}`. Devolve 0 com `{"titulo",
/// "extensoes", "teto"}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn escolha_regras(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = pedido_em_json(unsafe { entrada(ptr, len) });
    let limite = pedido
        .get("limiteDeBytes")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| usize::try_from(n).ok());
    let regras = crate::mods::regras_do_seletor(
        pedido.get("finalidade").and_then(serde_json::Value::as_str),
        lista_do_pedido(&pedido, "extensoes").as_deref(),
        limite,
    );
    dizer(regras.to_string().into_bytes(), 0)
}

/// **Guarda um arquivo escolhido**: `{"id", "geracao", "papeis", "tamanho",
/// "viva"}` e o começo dos bytes. Na ordem do desktop: o tipo pelos bytes, o
/// papel, a geração ainda de pé (`viva`, que a página confere logo antes), e o
/// teto dos que ficam de pé. Devolve 0 com o `EscolhidoParaOMod` do desktop
/// (`{"id", "tipo", "papel", "bytes"}`), ou 1 com `{"Recusado": {"motivo"}}`.
///
/// # Safety
///
/// Os dois pares `ptr`/`len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn escolha_guardar(
    pedido_ptr: *const u8,
    pedido_len: usize,
    comeco_ptr: *const u8,
    comeco_len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = pedido_em_json(unsafe { entrada(pedido_ptr, pedido_len) });
    // SAFETY: idem.
    let comeco = unsafe { entrada(comeco_ptr, comeco_len) };
    let tipo =
        match crate::mods::o_escolhido_serve(comeco, lista_do_pedido(&pedido, "papeis").as_deref())
        {
            Ok(tipo) => tipo,
            Err(motivo) => return recusa_de_mod(&motivo),
        };
    if pedido.get("viva").and_then(serde_json::Value::as_bool) != Some(true) {
        return recusa_de_mod("sessao-encerrada");
    }
    let tamanho = usize::try_from(numero_do_pedido(&pedido, "tamanho")).unwrap_or(usize::MAX);
    let guardado = com_os_escolhidos(|escolhidos| {
        escolhidos.guardar(seele_sessao::escolhidos::Escolhido {
            de_quem: pedido
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            geracao: numero_do_pedido(&pedido, "geracao"),
            tipo: tipo.media_type,
            tamanho,
            conteudo: (),
        })
    });
    match guardado {
        Ok(numero) => dizer(
            serde_json::json!({
                "id": numero,
                "tipo": tipo.media_type,
                "papel": tipo.papel,
                "bytes": tamanho,
            })
            .to_string()
            .into_bytes(),
            0,
        ),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// O dono de um escolhido: `{"geracao", "id", "arquivo"}`.
fn dono_do_escolhido(pedido: &serde_json::Value) -> (u64, String, u64) {
    (
        numero_do_pedido(pedido, "geracao"),
        pedido
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        numero_do_pedido(pedido, "arquivo"),
    )
}

/// **Onde começa e acaba um pedaço** de um escolhido, para quem o escolheu:
/// `{"geracao", "id", "arquivo", "inicio"}`. Devolve 0 com `{"inicio",
/// "fim"}` (a página corta os bytes dela), ou 1 com `{"Recusado": {"motivo"}}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn escolha_pedaco(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = pedido_em_json(unsafe { entrada(ptr, len) });
    let (geracao, id, arquivo) = dono_do_escolhido(&pedido);
    let inicio = usize::try_from(numero_do_pedido(&pedido, "inicio")).unwrap_or(usize::MAX);
    match com_os_escolhidos(|escolhidos| escolhidos.pedaco(geracao, &id, arquivo, inicio)) {
        Ok(faixa) => dizer(
            serde_json::json!({ "inicio": faixa.start, "fim": faixa.end })
                .to_string()
                .into_bytes(),
            0,
        ),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// **Solta um escolhido**, se é de quem pede: devolve 0 com
/// `{"soltou": bool}` (o desktop não diz nada a quem não é o dono).
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn escolha_soltar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = pedido_em_json(unsafe { entrada(ptr, len) });
    let (geracao, id, arquivo) = dono_do_escolhido(&pedido);
    let soltou = com_os_escolhidos(|escolhidos| escolhidos.soltar(geracao, &id, arquivo));
    dizer(
        serde_json::json!({ "soltou": soltou })
            .to_string()
            .into_bytes(),
        0,
    )
}

/// **Tira um escolhido para mandá-lo** ao volume: consumido antes do envio.
/// Devolve 0 com `{"tamanho"}`, ou 1 com `{"Recusado": {"motivo"}}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn escolha_tirar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = pedido_em_json(unsafe { entrada(ptr, len) });
    let (geracao, id, arquivo) = dono_do_escolhido(&pedido);
    match com_os_escolhidos(|escolhidos| escolhidos.tirar(geracao, &id, arquivo)) {
        Ok(tirado) => dizer(
            serde_json::json!({ "tamanho": tirado.tamanho })
                .to_string()
                .into_bytes(),
            0,
        ),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// **A sessão acabou**: os escolhidos dela saem. `{"geracao"}`; devolve 0 com
/// os números soltos, para a página soltar os bytes deles.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn escolhas_soltar_geracao(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = pedido_em_json(unsafe { entrada(ptr, len) });
    let geracao = numero_do_pedido(&pedido, "geracao");
    let soltos = com_os_escolhidos(|escolhidos| escolhidos.soltar_geracao(geracao));
    dizer(serde_json::json!(soltos).to_string().into_bytes(), 0)
}

/// Os bytes em base64 padrão (`seele_sessao::base64`): o pedaço de um
/// escolhido, como o desktop o manda ao MOD. Devolve 0 com o texto.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn base64_de(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    dizer(seele_sessao::base64::base64_de(bytes).into_bytes(), 0)
}

/// **O começo de um fluxo de imagem** ([`crate::volume`]): `{"ler": {"id",
/// "channel", "payload"}}` ou `{"enviar": {"id", "token", "bytes"}}`. Devolve 0
/// com o pedido enquadrado, ou 1 com `{"Recusado": {"motivo"}}` (o texto da
/// recusa do protocolo).
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn volume_pedido(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedido = pedido_em_json(unsafe { entrada(ptr, len) });
    let texto = |de: &serde_json::Value, campo: &str| {
        de.get(campo)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let feito = if let Some(ler) = pedido.get("ler") {
        crate::volume::pedido_de_ler(
            &texto(ler, "id"),
            u32::try_from(numero_do_pedido(ler, "channel")).unwrap_or(u32::MAX),
            &texto(ler, "payload"),
        )
    } else if let Some(enviar) = pedido.get("enviar") {
        crate::volume::pedido_de_enviar(
            &texto(enviar, "id"),
            &texto(enviar, "token"),
            numero_do_pedido(enviar, "bytes"),
        )
    } else {
        Err("pedido-de-imagem-sem-forma".to_owned())
    };
    match feito {
        Ok(bytes) => dizer(bytes, 0),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// O prazo de uma transferência de imagem, em milissegundos, e o motivo que
/// o MOD lê quando ele acaba (`seele_sessao::transferencia`). Devolve 0 com
/// `{"ms", "motivo"}`.
#[no_mangle]
pub extern "C" fn volume_prazo() -> u32 {
    use seele_sessao::transferencia::{PRAZO_DA_IMAGEM_DE_MOD_S, PRAZO_ESGOTADO};
    dizer(
        serde_json::json!({ "ms": PRAZO_DA_IMAGEM_DE_MOD_S * 1000, "motivo": PRAZO_ESGOTADO })
            .to_string()
            .into_bytes(),
        0,
    )
}

/// Uma resposta de imagem nova; devolve a alça dela.
#[no_mangle]
pub extern "C" fn volume_resposta_nova() -> u32 {
    RESPOSTAS.with(|r| {
        let (proxima, respostas) = &mut *r.borrow_mut();
        *proxima = proxima.wrapping_add(1);
        respostas.insert(*proxima, crate::volume::Resposta::nova());
        *proxima
    })
}

/// Mais um pedaço da resposta `alca`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn volume_resposta_chegou(alca: u32, ptr: *const u8, len: usize) {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let pedaco = unsafe { entrada(ptr, len) };
    RESPOSTAS.with(|r| {
        if let Some(resposta) = r.borrow_mut().1.get_mut(&alca) {
            resposta.chegou(pedaco);
        }
    });
}

/// **A resposta `alca` acabou**, e é esquecida. `viva` 0 é a geração que
/// acabou durante a transferência (`sessao-encerrada`, conferida depois da
/// resposta, como no desktop). Com `uso` (vazio para nenhum) e `imagem` 1, é a
/// imagem lida: devolve 0 com o `MidiaDoMod` do desktop,
/// pronto pelo caminho comum; com `imagem` 0, é a confirmação de um envio:
/// devolve 0 sem nada. 1 com `{"Recusado": {"motivo"}}`.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn volume_resposta_acabou(
    alca: u32,
    imagem: u32,
    viva: u32,
    uso_ptr: *const u8,
    uso_len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let uso = texto_de(unsafe { entrada(uso_ptr, uso_len) });
    let Some(resposta) = RESPOSTAS.with(|r| r.borrow_mut().1.remove(&alca)) else {
        return recusa_de_mod("incompleto");
    };
    let bytes = match resposta.acabou() {
        Ok(bytes) => bytes,
        Err(motivo) => return recusa_de_mod(&motivo),
    };
    // A geração conferida de novo **depois** de a transferência dar certo,
    // como o desktop: a recusa do servidor chega como ela é.
    if viva == 0 {
        return recusa_de_mod("sessao-encerrada");
    }
    if imagem == 0 {
        return dizer(Vec::new(), 0);
    }
    let teto = (!uso.is_empty())
        .then(|| seele_sessao::reducao::teto_do_uso(uso))
        .flatten();
    match seele_sessao::midia_de_mod::pronta(bytes, teto.as_ref()) {
        Ok(pronta) => dizer(crate::mods::em_json(pronta).to_string().into_bytes(), 0),
        Err(motivo) => recusa_de_mod(&motivo),
    }
}

/// A chave do catálogo que esta página confere: a embutida, ou a da prova.
fn chave_do_catalogo() -> String {
    CHAVE_DO_CATALOGO.with(|c| {
        c.borrow()
            .clone()
            .unwrap_or_else(|| crate::mods::CHAVE_DO_CATALOGO.to_owned())
    })
}

/// Onde o catálogo mora (`seele_sessao::catalogo::ENDERECO`). Devolve 0 com o
/// endereço, sem a barra do fim.
#[no_mangle]
pub extern "C" fn catalogo_endereco() -> u32 {
    dizer(seele_sessao::catalogo::ENDERECO.as_bytes().to_vec(), 0)
}

/// **O catálogo, antes de tudo**: conferido contra a chave e lido, como o
/// `buscar_catalogo` do desktop faz antes de buscar as revogações. Devolve 0
/// sem nada, ou 1 com a `FalhaNoCatalogo`.
///
/// # Safety
///
/// Os dois pares `ptr`/`len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn catalogo_ler(
    catalogo_ptr: *const u8,
    catalogo_len: usize,
    assinatura_ptr: *const u8,
    assinatura_len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let catalogo = unsafe { entrada(catalogo_ptr, catalogo_len) };
    // SAFETY: idem.
    let assinatura = texto_de(unsafe { entrada(assinatura_ptr, assinatura_len) });
    match seele_sessao::catalogo::ler_catalogo(catalogo, assinatura, &chave_do_catalogo()) {
        Ok(_) => dizer(Vec::new(), 0),
        Err(falha) => dizer(serde_json::json!(falha).to_string().into_bytes(), 1),
    }
}

/// **O plano do `instalar_mod_do_catalogo`** ([`crate::mods::planejar`]): o
/// catálogo, as revogações e as duas assinaturas como a página as baixou, e
/// `{"id", "versao"}`. Devolve 0 com `{"arquivos": [[caminho, url], …],
/// "hash"}`, ou 1 com a `FalhaNoCatalogo` do desktop.
///
/// # Safety
///
/// Os cinco pares `ptr`/`len` como em [`entrada`].
#[no_mangle]
#[allow(clippy::too_many_arguments, reason = "cinco blocos, um par cada")]
pub unsafe extern "C" fn catalogo_planejar(
    catalogo_ptr: *const u8,
    catalogo_len: usize,
    assinatura_ptr: *const u8,
    assinatura_len: usize,
    revogacoes_ptr: *const u8,
    revogacoes_len: usize,
    assinatura_das_revogacoes_ptr: *const u8,
    assinatura_das_revogacoes_len: usize,
    pedido_ptr: *const u8,
    pedido_len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`, para cada par.
    let baixado = unsafe {
        crate::mods::Baixado {
            catalogo: entrada(catalogo_ptr, catalogo_len),
            assinatura: texto_de(entrada(assinatura_ptr, assinatura_len)),
            revogacoes: entrada(revogacoes_ptr, revogacoes_len),
            assinatura_das_revogacoes: texto_de(entrada(
                assinatura_das_revogacoes_ptr,
                assinatura_das_revogacoes_len,
            )),
        }
    };
    // SAFETY: idem.
    let pedido: serde_json::Value =
        serde_json::from_slice(unsafe { entrada(pedido_ptr, pedido_len) }).unwrap_or_default();
    let texto = |campo: &str| {
        pedido
            .get(campo)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match crate::mods::planejar(
        &baixado,
        &chave_do_catalogo(),
        &texto("id"),
        &texto("versao"),
    ) {
        Ok(plano) => dizer(plano.to_string().into_bytes(), 0),
        Err(falha) => dizer(serde_json::json!(falha).to_string().into_bytes(), 1),
    }
}

/// **Os bytes que o plano trouxe** ([`crate::mods::conferir_baixados`]),
/// entregues como o último pacote, contra o hash que o plano disse. Devolve 0
/// com `{"id", "hash"}` (o lugar em que a página o guarda), ou 1 com a
/// `FalhaNoCatalogo`; e esquece os pacotes.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn catalogo_conferir(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let esperado = texto_de(unsafe { entrada(ptr, len) }).to_owned();
    match crate::mods::conferir_baixados(&mut o_pacote(), &esperado) {
        Ok(guardar) => dizer(guardar.to_string().into_bytes(), 0),
        Err(falha) => dizer(serde_json::json!(falha).to_string().into_bytes(), 1),
    }
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

/// O envio de quem o servidor chamou de `ssrc`, com o relógio partindo da hora
/// da página `agora_ms` (`performance.timeOrigin + performance.now()`), e não
/// do zero: ver [`Envio::partindo_de`]. A página antiga, que não a manda, chega
/// com NaN, e isso dá o zero de antes.
#[no_mangle]
pub extern "C" fn voz_envio(ssrc: u32, agora_ms: f64) {
    ENVIO.with(|e| *e.borrow_mut() = Some(Envio::partindo_de(ssrc, agora_ms)));
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

/// **Os ganhos da sessão desta instância, opacos** (E3): com uma instância do
/// `.wasm` por sessão, os volumes moram na instância da sessão e a recepção da
/// voz na da página. Esta função dá, na instância da sessão, os ganhos num
/// bloco que a página não lê e só repassa a [`voz_ganhos_pela_recepcao`], na
/// instância da página. A SSRC continua sem atravessar para o JavaScript como
/// número que ele use: o bloco é opaco para ele. Devolve 0 com o bloco.
#[no_mangle]
pub extern "C" fn voz_ganhos_da_sessao() -> u32 {
    let ganhos = LIGACAO.with(|l| {
        l.borrow()
            .as_ref()
            .and_then(Ligacao::sessao)
            .map(crate::sessao::Sessao::ganhos)
            .unwrap_or_default()
    });
    dizer(serde_json::json!(ganhos).to_string().into_bytes(), 0)
}

/// O bloco de [`voz_ganhos_da_sessao`] (de outra instância), traduzido pela
/// recepção desta: `[[alça, ganho], …]`, como [`voz_ganhos`]. Um bloco torto
/// dá a lista vazia.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn voz_ganhos_pela_recepcao(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let pares: Vec<(u32, f32)> = serde_json::from_slice(bytes).unwrap_or_default();
    let por_alca: Vec<(u32, f32)> = RECEPCAO.with(|r| {
        let recepcao = r.borrow();
        pares
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

    use super::{dizer, dizer_o_link, entrada, erro, texto_de, CHAVE_DO_CATALOGO, ENVIO};
    use crate::voz::Envio;
    use crate::ErroDaPonte;

    /// **Só para a bancada**: a chave do catálogo que esta página confere, no
    /// formato do `.pub` do minisign. A bancada dos MODs serve um catálogo de
    /// teste assinado por ela; o `.wasm` da página não tem esta porta, e só
    /// confere a chave embutida (a regra 5 do `cargo xtask pacote-web`).
    ///
    /// # Safety
    ///
    /// `ptr` e `len` como em [`entrada`].
    #[no_mangle]
    pub unsafe extern "C" fn prova_chave_do_catalogo(ptr: *const u8, len: usize) {
        // SAFETY: repassa o contrato desta função ao de `entrada`.
        let chave = texto_de(unsafe { entrada(ptr, len) }).to_owned();
        CHAVE_DO_CATALOGO.with(|c| *c.borrow_mut() = Some(chave));
    }

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod os_cargos_pela_fronteira {
    use super::*;
    use seele_proto::cargos::{Atribuicao, Cargo, Permissao, QuadroDeCargos, NOME_DO_TODOS, TODOS};
    use seele_proto::control::Permission;
    use seele_proto::ids::{PersonId, RoleId};

    /// Uma chamada pela fronteira de verdade: o bloco de `alocar`, a função
    /// exportada, e a saída lida de `saida_ptr`/`saida_len`.
    fn chamar(
        f: unsafe extern "C" fn(*const u8, usize) -> u32,
        entrada: &serde_json::Value,
    ) -> (u32, serde_json::Value) {
        let bytes = entrada.to_string().into_bytes();
        let ptr = alocar(bytes.len());
        // SAFETY: `ptr` é um bloco de `alocar` com capacidade para `bytes`.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
        // SAFETY: o bloco é vivo e tem `bytes.len()` bytes escritos.
        let codigo = unsafe { f(ptr, bytes.len()) };
        // SAFETY: o bloco veio de `alocar(bytes.len())`.
        unsafe { liberar(ptr, bytes.len()) };
        let saida = SAIDA.with(|s| s.borrow().clone());
        (codigo, serde_json::from_slice(&saida).unwrap())
    }

    /// Um quadro em que a pessoa 7 tem a Gestão (`ManageRoles`), com a revisão dada.
    fn quadro(revisao: u64, nome: &str) -> serde_json::Value {
        serde_json::to_value(QuadroDeCargos {
            cargos: vec![
                Cargo {
                    id: RoleId(1),
                    nome: nome.into(),
                    posicao: 1,
                    concede: vec![Permissao::Produto(Permission::ManageRoles)],
                    restringe: vec![],
                },
                Cargo {
                    id: TODOS,
                    nome: NOME_DO_TODOS.into(),
                    posicao: 0,
                    concede: vec![],
                    restringe: vec![],
                },
            ],
            atribuicoes: vec![Atribuicao {
                pessoa: PersonId(7),
                cargos: vec![RoleId(1)],
            }],
            proprietario: None,
            de_mods: vec![],
            revisao,
            dono_declarado: false,
        })
        .unwrap()
    }

    #[test]
    fn os_quatro_comandos_atravessam_a_fronteira_e_cada_sessao_traz_o_seu_quadro() {
        let membros = serde_json::json!({ "7": "ana", "8": "bia" });
        let (a, b) = (quadro(12, "Gestão A"), quadro(40, "Gestão B"));

        // A tela de cada sessão: o quadro DELA, com os apelidos dela.
        let (codigo, tela_a) = chamar(
            cargos_na_tela,
            &serde_json::json!({ "quadro": a, "membros": membros }),
        );
        assert_eq!(codigo, 0, "{tela_a}");
        assert_eq!(tela_a["quadro"]["revisao"], 12);
        assert_eq!(tela_a["apelidos"]["7"], "ana");
        let (_, tela_b) = chamar(
            cargos_na_tela,
            &serde_json::json!({ "quadro": b, "membros": {}, "suas": null }),
        );
        assert_eq!(tela_b["quadro"]["revisao"], 40);
        assert_eq!(tela_b["quadro"]["cargos"][0]["nome"], "Gestão B");

        // A prévia com a revisão de A vale em A e é velha em B: a mesma recusa do servidor.
        let pedido = serde_json::json!({ "Apagar": { "cargo": 1 } });
        let (codigo, em_b) = chamar(
            cargos_prever,
            &serde_json::json!({ "quadro": b, "membros": membros, "autor": 7, "revisao": 12, "pedido": pedido }),
        );
        assert_eq!(codigo, 1);
        assert_eq!(em_b["recusa"]["RevisaoVelha"]["atual"], 40);
        let (codigo, recusa) = chamar(
            cargos_prever,
            &serde_json::json!({ "quadro": a, "membros": membros, "autor": 7, "revisao": 12,
                                 "pedido": { "Atribuir": { "pessoa": 8, "cargo": 1 } } }),
        );
        assert_eq!(codigo, 1);
        assert_eq!(recusa["recusa"], "CargoAcimaDeVoce");

        // Explicar: uma por permissão do catálogo.
        let (codigo, explicadas) = chamar(
            cargos_explicar,
            &serde_json::json!({ "quadro": a, "membros": membros, "pessoa": 8 }),
        );
        assert_eq!(codigo, 0, "{explicadas}");
        assert_eq!(
            explicadas["permissoes"].as_array().map(Vec::len),
            Some(seele_proto::cargos::PERMISSOES_DO_PRODUTO.len())
        );

        // O verbo sai como a ClientMessage que o `codificar` de sempre aceita.
        let (codigo, mensagem) = chamar(
            cargos_comando,
            &serde_json::json!({ "revisao": 12, "pedido": { "Atribuir": { "pessoa": 8, "cargo": 1 } } }),
        );
        assert_eq!(codigo, 0, "{mensagem}");
        assert_eq!(
            serde_json::from_value::<seele_proto::control::ClientMessage>(mensagem.clone())
                .unwrap(),
            seele_proto::control::ClientMessage::AtribuirCargo {
                revisao: 12,
                pessoa: PersonId(8),
                cargo: RoleId(1)
            }
        );
        assert!(
            crate::codificar_do_cliente(seele_proto::PROTOCOL_VERSION, &mensagem.to_string())
                .is_ok()
        );
    }

    #[test]
    fn a_entrada_torta_e_recusada_com_nome() {
        for (f, entrada) in [
            (
                cargos_prever as unsafe extern "C" fn(*const u8, usize) -> u32,
                serde_json::json!({ "quadro": quadro(1, "x"), "membros": {}, "revisao": 1, "pedido": {} }),
            ),
            (
                cargos_explicar,
                serde_json::json!({ "quadro": quadro(1, "x"), "membros": {} }),
            ),
            (
                cargos_comando,
                serde_json::json!({ "pedido": { "Apagar": { "cargo": 1 } } }),
            ),
            (cargos_na_tela, serde_json::json!([1, 2])),
            (
                cargos_comando,
                serde_json::json!({ "revisao": 1, "pedido": "nao e pedido" }),
            ),
        ] {
            let (codigo, saida) = chamar(f, &entrada);
            assert_eq!(codigo, 1, "{entrada} -> {saida}");
            assert!(saida["erro"].is_string(), "{saida}");
        }
    }
}

/// **A tela que o servidor liga sozinho aparece no retrato do navegador**,
/// pela fronteira de verdade: o fluxo entra por `fluxo_chegou`/`fluxo_acabou`
/// e o retrato sai da sessão guardada em `LIGACAO`. O mesmo defeito da FFI: o
/// `ScreenOpened` ia só à página, e o retrato lia só a escolha.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod a_tela_ligada_pelo_servidor {
    use super::*;
    use seele_proto::control::{encode_em, ServerMessage, VoiceRoomInfo};
    use seele_proto::ids::{PersonId, ScreenId, SessionId, Ssrc, VoiceRoomId};
    use seele_proto::quadro::enquadrar;
    use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, TipoDeQuadro};
    use seele_proto::stream::StreamType;
    use seele_proto::version::PROTOCOL_VERSION;

    fn do_servidor(mensagem: &ServerMessage) -> Vec<u8> {
        enquadrar(&encode_em(PROTOCOL_VERSION, mensagem).unwrap()).unwrap()
    }

    /// Dentro da sessão (somos a 7), sentada na sala 1, onde a pessoa 3
    /// transmite a tela 9 e a 4 a tela 11; guardada em `LIGACAO`.
    fn dentro() {
        let mut ligacao = Ligacao::abrir(Pedido {
            sala_pessoal: false,
            apelido: "marcela".into(),
            chave_publica: [5; 32],
            impressao: [7; 32],
            impressao_dtls: [8; 32],
            convite: None,
            aceito: None,
        })
        .unwrap();
        let _ = ligacao.chegou(&do_servidor(&ServerMessage::Challenge {
            nonce: vec![3; 32],
        }));
        let _ = ligacao.assinado(&[9; 64]);
        let _ = ligacao.chegou(&do_servidor(&ServerMessage::Session {
            id: SessionId(41),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: VoiceRoomId(1),
                name: "SALA-01".into(),
                limit: 20,
                password_required: false,
                channel: None,
            }],
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        }));
        let sessao = ligacao.sessao_mut().expect("dentro da sessão");
        sessao.room.enter_voice_room(VoiceRoomId(1));
        for (pessoa, tela) in [(3, 9), (4, 11)] {
            sessao.room.apply(&ServerMessage::ScreenShareStarted {
                voice_room: VoiceRoomId(1),
                person: PersonId(pessoa),
                screen: ScreenId(tela),
            });
        }
        LIGACAO.with(|l| *l.borrow_mut() = Some(ligacao));
    }

    /// Um fluxo de tela pela fronteira: a abertura e um quadro-chave. Devolve a
    /// alça, ainda aberta.
    fn abrir_o_fluxo(tela: u32) -> u32 {
        let mut bytes = vec![StreamType::Screen.byte()];
        let mut abertura = [0_u8; seele_proto::screen::SCREEN_HEADER_LEN];
        ScreenHeader {
            version: 1,
            screen: ScreenId(tela),
            source: ScreenSource::Monitor,
            codec: ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        }
        .encode(&mut abertura)
        .unwrap();
        bytes.extend(abertura);
        bytes.push(TipoDeQuadro::Chave as u8);
        bytes.extend(5_u32.to_be_bytes());
        bytes.extend(b"chave");
        let alca = fluxo_novo();
        let ptr = alocar(bytes.len());
        // SAFETY: `ptr` é um bloco de `alocar` com capacidade para `bytes`.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
        // SAFETY: o bloco é vivo e tem `bytes.len()` bytes escritos.
        let codigo = unsafe { fluxo_chegou(alca, ptr, bytes.len()) };
        // SAFETY: o bloco veio de `alocar(bytes.len())`.
        unsafe { liberar(ptr, bytes.len()) };
        assert_eq!(codigo, 0);
        let saida: serde_json::Value =
            serde_json::from_slice(&SAIDA.with(|s| s.borrow().clone())).unwrap();
        assert_eq!(
            saida["passos"][0]["evento"]["ScreenOpened"]["screen"], tela,
            "{saida}"
        );
        alca
    }

    /// A tela do retrato, e as que dizem `assistida`.
    fn no_retrato() -> (Option<u32>, Vec<u32>) {
        LIGACAO.with(|l| {
            let retrato = l.borrow().as_ref().unwrap().sessao().unwrap().retrato();
            (
                retrato.tela.map(|tela| tela.tela),
                retrato
                    .transmissoes
                    .iter()
                    .filter(|transmissao| transmissao.assistida)
                    .map(|transmissao| transmissao.tela)
                    .collect(),
            )
        })
    }

    fn atender(comando: &str, args: &serde_json::Value) {
        LIGACAO.with(|l| {
            let mut pagina = DaPagina::nova(1);
            let _ = l
                .borrow_mut()
                .as_mut()
                .unwrap()
                .atender(comando, args, &mut pagina, 0);
        });
    }

    #[test]
    fn a_abertura_automatica_aparece_e_o_fim_limpa() {
        dentro();
        assert_eq!(no_retrato(), (None, vec![]));
        let alca = abrir_o_fluxo(9);
        assert_eq!(
            no_retrato(),
            (Some(9), vec![9]),
            "o servidor ligou a tela 9 e o retrato do navegador diz que ninguém compartilha"
        );
        let _ = fluxo_acabou(alca);
        assert_eq!(no_retrato(), (None, vec![]));
    }

    #[test]
    fn na_alternancia_9_11_9_fica_a_9_nova_e_o_fim_velho_nao_a_apaga() {
        dentro();
        let primeira = abrir_o_fluxo(9);
        let onze = abrir_o_fluxo(11);
        let _nova = abrir_o_fluxo(9);
        assert_eq!(no_retrato(), (Some(9), vec![9]));
        let _ = fluxo_acabou(primeira);
        let _ = fluxo_acabou(onze);
        assert_eq!(
            no_retrato(),
            (Some(9), vec![9]),
            "um fim velho apagou a 9 nova"
        );
    }

    #[test]
    fn a_escolha_explicita_vence_a_automatica() {
        dentro();
        let _alca = abrir_o_fluxo(9);
        atender(
            "assistir",
            &serde_json::json!({ "tela": 11, "quero": true }),
        );
        assert_eq!(no_retrato(), (Some(11), vec![11]));
    }

    #[test]
    fn o_nao_ver_explicito_vence_a_automatica() {
        dentro();
        let _velha = abrir_o_fluxo(9);
        atender(
            "assistir",
            &serde_json::json!({ "tela": 9, "quero": false }),
        );
        assert_eq!(
            no_retrato(),
            (None, vec![]),
            "quem recusou aparece assistindo"
        );
        let _nova = abrir_o_fluxo(9);
        assert_eq!(no_retrato(), (None, vec![]));
    }

    #[test]
    fn a_queda_nao_deixa_tela_velha_e_o_fim_atrasado_nao_apaga_a_nova() {
        dentro();
        let velha = abrir_o_fluxo(9);
        let _ = ligacao_largar();
        LIGACAO.with(|l| {
            let mut l = l.borrow_mut();
            let ligacao = l.as_mut().unwrap();
            // A volta: a sessão guardada, readotada.
            let _ = ligacao.retomar(
                Pedido {
                    sala_pessoal: false,
                    apelido: "marcela".into(),
                    chave_publica: [5; 32],
                    impressao: [7; 32],
                    impressao_dtls: [8; 32],
                    convite: None,
                    aceito: None,
                },
                crate::sessao::Escolhas::default(),
            );
        });
        // Antes de a volta abrir a tela de novo, o retrato não a mostra.
        let sessao_ve = LIGACAO.with(|l| {
            l.borrow()
                .as_ref()
                .unwrap()
                .sessao()
                .map(|sessao| sessao.retrato().tela.map(|tela| tela.tela))
        });
        assert_eq!(sessao_ve.flatten(), None, "a tela de antes da queda ficou");
        // A volta entra e o servidor liga a 9 de novo; o fim atrasado da
        // leitura de antes da queda chega depois.
        let _ = LIGACAO.with(|l| {
            let mut l = l.borrow_mut();
            let ligacao = l.as_mut().unwrap();
            let _ = ligacao.chegou(&do_servidor(&ServerMessage::Challenge {
                nonce: vec![3; 32],
            }));
            let _ = ligacao.assinado(&[9; 64]);
            ligacao.chegou(&do_servidor(&ServerMessage::Session {
                id: SessionId(41),
                person: PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: vec![VoiceRoomInfo {
                    id: VoiceRoomId(1),
                    name: "SALA-01".into(),
                    limit: 20,
                    password_required: false,
                    channel: None,
                }],
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: Vec::new(),
            }))
        });
        assert!(
            LIGACAO.with(|l| l.borrow().as_ref().unwrap().sessao().is_some()),
            "a volta não entrou"
        );
        // O servidor reanuncia a transmissão a quem volta à sala.
        LIGACAO.with(|l| {
            let mut l = l.borrow_mut();
            let sessao = l.as_mut().unwrap().sessao_mut().unwrap();
            sessao.room.enter_voice_room(VoiceRoomId(1));
            sessao.room.apply(&ServerMessage::ScreenShareStarted {
                voice_room: VoiceRoomId(1),
                person: PersonId(3),
                screen: ScreenId(9),
            });
        });
        let _nova = abrir_o_fluxo(9);
        assert_eq!(no_retrato(), (Some(9), vec![9]));
        let _ = fluxo_acabou(velha);
        assert_eq!(
            no_retrato(),
            (Some(9), vec![9]),
            "o fim atrasado da leitura de antes da queda apagou a da volta"
        );
    }
}
