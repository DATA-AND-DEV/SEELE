//! `maquina.programa.ler`: um arquivo de texto da pasta do servidor de jogo
//! (API 8 parte 6).
//!
//! **Por que existe:** a metade de máquina lê o `modrinth.index.json` do pack
//! que ela trouxe e abriu, para baixar cada arquivo com o `sha512` dele; e não
//! tem disco nenhum fora do `programa`. Só texto UTF-8, até [`MAX_BYTES_LIDOS`],
//! e só dentro da pasta — a mesma régua de caminho de `escrever`. Ler não
//! pede aceite: a pasta é do servidor de jogo, e o que está nela foi o
//! `programa` quem pôs ou o jogo quem escreveu.

use std::sync::atomic::Ordering;

use super::{caminho, Programa, Recusa};

/// O maior texto que `ler` devolve: 4 MiB. O índice de um modpack de 300 mods
/// tem cerca de 250 KiB.
pub const MAX_BYTES_LIDOS: u64 = 4 * 1024 * 1024;

impl Programa {
    /// O texto de `arquivo`.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], as do caminho, [`Recusa::TextoGrandeDemais`],
    /// [`Recusa::NaoETexto`] e [`Recusa::Disco`]. Toda recusa é contada.
    pub fn ler(&self, arquivo: &str) -> Result<String, Recusa> {
        self.ler_sem_contar(arquivo)
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    fn ler_sem_contar(&self, arquivo: &str) -> Result<String, Recusa> {
        let interior = &self.interior;
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let alvo = caminho::dentro_da_raiz(&interior.raiz, arquivo, false)?;
        let disco = |erro: std::io::Error| Recusa::Disco {
            caminho: alvo.clone(),
            motivo: erro.to_string(),
        };
        let tamanho = std::fs::metadata(&alvo).map_err(disco)?.len();
        if tamanho > MAX_BYTES_LIDOS {
            return Err(Recusa::TextoGrandeDemais {
                tamanho,
                teto: MAX_BYTES_LIDOS,
            });
        }
        let bytes = std::fs::read(&alvo).map_err(disco)?;
        String::from_utf8(bytes).map_err(|_| Recusa::NaoETexto {
            arquivo: arquivo.to_owned(),
        })
    }
}
