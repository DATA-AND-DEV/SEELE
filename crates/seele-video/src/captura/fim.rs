//! O fim que ninguém pediu: a janela transmitida fechou, ou o sistema parou o
//! fluxo.
//!
//! # O defeito que este arquivo fecha
//!
//! As duas capturas **sabiam** que tinham acabado e não contavam. No Windows o
//! `on_closed` devolvia `Ok(())` e o `esta_viva` não tinha quem o chamasse; no
//! macOS o `SCStream` nascia sem delegado, e o `didStopWithError` do sistema
//! morria num `eprintln!` da biblioteca. Para quem lê a vaga, uma captura
//! morta e uma tela parada são a mesma coisa — `None` atrás de `None` —, e a
//! transmissão ficava «no ar», congelada no último quadro, para sempre.
//!
//! # Por que mora aqui, e não num dos dois sistemas
//!
//! Porque a decisão é a mesma nos dois e precisa de teste que rode em qualquer
//! máquina: um guarda dentro de `#![cfg(windows)]` não roda no Mac de quem
//! escreve, e existir não é funcionar. Cada sistema só **marca** o sinal, do
//! retorno de chamada que for o dele; quem transmite só **pergunta**.

use std::sync::OnceLock;

/// O recado de uma captura que acabou sem ninguém mandar.
///
/// Escrito pela thread do sistema operacional e lido pela do codificador, e é
/// por isso que é um [`OnceLock`]: marca-se uma vez, sem cadeado no caminho de
/// quem lê a cada tique.
///
/// **A primeira marca vence.** Um fluxo que acaba costuma dizê-lo mais de uma
/// vez — o estado do último quadro e depois o erro do delegado —, e o primeiro
/// recado é o mais próximo da causa.
#[derive(Debug, Default)]
pub struct SinalDeFim(OnceLock<String>);

impl SinalDeFim {
    /// Marca que a captura acabou, com o que o sistema disse.
    pub fn marcar(&self, detalhe: impl Into<String>) {
        // `Err` aqui é «já estava marcado», que é o desenho e não uma falha.
        let _ = self.0.set(detalhe.into());
    }

    /// O que o sistema disse ao acabar, ou `None` enquanto a captura vive.
    #[must_use]
    pub fn acabou(&self) -> Option<&str> {
        self.0.get().map(String::as_str)
    }
}

/// O que se diz quando a thread da captura sumiu sem deixar recado.
pub const A_THREAD_ACABOU: &str = "the capture thread ended on its own";

/// Decide se a captura acabou, a partir das duas coisas que um sistema sabe.
///
/// `recado` é o que foi marcado no [`SinalDeFim`]; `de_pe` é se quem entrega
/// os quadros continua vivo. **Qualquer um dos dois basta**: o recado sem a
/// thread ter saído ainda é o `on_closed` do Windows visto antes do `WM_QUIT`,
/// e a thread morta sem recado é um retorno de chamada que devolveu erro. Nos
/// dois casos não vem mais quadro nenhum.
///
/// `None` é a captura viva — inclusive a que não entrega nada porque a tela
/// não muda, que é estado normal e não fim.
#[must_use]
pub fn fim_da_captura(recado: Option<&str>, de_pe: bool) -> Option<String> {
    match (recado, de_pe) {
        (Some(recado), _) => Some(recado.to_owned()),
        (None, false) => Some(A_THREAD_ACABOU.to_owned()),
        (None, true) => None,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_captura_viva_e_sem_recado_nao_acabou() {
        // É o caso da tela parada: nenhum quadro chega, e nada acabou.
        assert_eq!(fim_da_captura(None, true), None);
        assert_eq!(SinalDeFim::default().acabou(), None);
    }

    #[test]
    fn o_recado_basta_mesmo_com_a_thread_de_pe() {
        // O `on_closed` roda antes de a thread sair: esperar a thread seria
        // transmitir mais alguns tiques de uma janela que já não existe.
        let sinal = SinalDeFim::default();
        sinal.marcar("a janela fechou");
        assert_eq!(
            fim_da_captura(sinal.acabou(), true).as_deref(),
            Some("a janela fechou")
        );
    }

    #[test]
    fn a_thread_morta_sem_recado_tambem_e_fim() {
        assert_eq!(
            fim_da_captura(None, false).as_deref(),
            Some(A_THREAD_ACABOU)
        );
    }

    #[test]
    fn a_primeira_marca_vence() {
        let sinal = SinalDeFim::default();
        sinal.marcar("a janela fechou");
        sinal.marcar("o fluxo parou depois");
        assert_eq!(sinal.acabou(), Some("a janela fechou"));
    }
}
