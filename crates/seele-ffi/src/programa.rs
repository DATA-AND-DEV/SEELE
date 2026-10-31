//! **O poder `programa` da API 8, para a casca** (parte 4): o núcleo de
//! `seele_core::programa`, e a política montada **só do aceite guardado**.
//!
//! A casca não alcança o `seele-core` (ADR 0002): os tipos passam por aqui,
//! um a um, como os outros re-exports deste crate. O que este módulo soma é a
//! ponte do aceite (`AceiteDeMaquinaGuardado`, a forma que a casca tem) à
//! `Politica`, e a conta dos artefatos desta máquina que a tela do aceite
//! mostra — as duas pela mesma regra do núcleo, para a tela e o disco não
//! discordarem.

use std::sync::Arc;

pub use seele_core::programa::{
    plataforma_desta_maquina, AlgoritmoDeHash, ArtefatoAceito, Baixado, ComandoAceito,
    Desempacotado, Download, EventoDoDownload, EventoDoProcesso, FimDoProcesso, HashDeclarado,
    Origem, OrigemHttps, PedidoDeDesempacotar, PedidoDeDownload, PedidoDeExecucao, Politica,
    Processo, Programa, Recusa, Trazido, MAX_BYTES_ESCRITOS, MAX_BYTES_LIDOS, NOMES_DAS_RECUSAS,
};
// A cópia do mundo (API 8 parte 7).
pub use seele_core::programa::{
    CopiaOferecida, CopiaRecebida, Empacotado, PedidoDeCopia, MAX_TENTATIVAS_DA_COPIA,
};

use crate::mods::AceiteDeMaquinaGuardado;
use crate::{ArtefatoOferecidoFfi, ComandoOferecidoFfi, OfertaDeMaquinaFfi};

/// O sistema desta máquina, ou `"desconhecida"` num que nenhuma receita
/// conhece (e então nenhum artefato marcado serve aqui).
#[must_use]
pub fn esta_plataforma() -> &'static str {
    plataforma_desta_maquina().unwrap_or("desconhecida")
}

/// **Os artefatos que esta máquina baixa**, e quantos são de outros sistemas:
/// o que a tela do aceite lista e soma.
#[must_use]
pub fn artefatos_desta_maquina(oferta: &OfertaDeMaquinaFfi) -> (Vec<ArtefatoOferecidoFfi>, usize) {
    let plataforma = esta_plataforma();
    let daqui: Vec<ArtefatoOferecidoFfi> = oferta
        .artefatos
        .iter()
        .filter(|a| a.plataformas.is_empty() || a.plataformas.iter().any(|p| p == plataforma))
        .cloned()
        .collect();
    let outros = oferta.artefatos.len() - daqui.len();
    (daqui, outros)
}

/// **Os comandos que esta máquina roda** (decisão W1): o par da entrada de
/// `por_sistema` que nomeia este sistema, ou o de cima, e quantos outros
/// sistemas têm um par próprio. A tela mostra só este par, pela mesma regra
/// da política ([`seele_core::maquina::comandos_para`]).
///
/// # Errors
///
/// `"oferta-invalida"`, a oferta que não volta ao núcleo.
pub fn comandos_desta_maquina(
    oferta: &OfertaDeMaquinaFfi,
) -> Result<
    (
        Option<ComandoOferecidoFfi>,
        Option<ComandoOferecidoFfi>,
        usize,
    ),
    &'static str,
> {
    let nucleo = oferta.para_o_nucleo()?;
    let c = seele_core::maquina::comandos_para(&nucleo, esta_plataforma());
    let ffi = |r: &seele_core::maquina::ComandoOferecido| ComandoOferecidoFfi {
        comando: r.comando.clone(),
        argumentos: r.argumentos.clone(),
    };
    Ok((
        c.rodar.map(ffi),
        c.instalar.map(ffi),
        c.sistemas_com_outros_comandos,
    ))
}

/// **A política do aceite guardado**, na pasta do servidor de jogo dele
/// (`<config>/jogos/<alvo>/<mod>/<servidor>`, decisão M10/Q7).
///
/// # Errors
///
/// `"OfertaInvalida"` (a oferta guardada não passa na validação do fio) e
/// `"EnderecoInvalido"`.
pub fn politica_do_aceite(
    home: &str,
    aceite: &AceiteDeMaquinaGuardado,
) -> Result<Politica, String> {
    let oferta = aceite
        .oferta
        .para_o_nucleo()
        .map_err(|_| "OfertaInvalida".to_owned())?;
    let raiz = crate::mods::pasta_do_servidor_de_jogo(
        home,
        &aceite.alvo,
        &aceite.mod_id,
        &aceite.servidor,
    )?;
    Ok(seele_core::programa::politica_do_aceite(
        &oferta,
        raiz,
        esta_plataforma(),
    ))
}

/// Um [`Programa`] do aceite guardado, baixando pela internet de verdade.
///
/// # Errors
///
/// Os de [`politica_do_aceite`], `"SemTls"` (o cliente HTTPS não montou) e
/// `"Disco"` (a pasta não se cria).
pub fn programa_do_aceite(
    home: &str,
    aceite: &AceiteDeMaquinaGuardado,
) -> Result<Programa, String> {
    let politica = politica_do_aceite(home, aceite)?;
    let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::nova().map_err(|_| "SemTls".to_owned())?);
    Programa::novo(politica, origem).map_err(|_| "Disco".to_owned())
}
