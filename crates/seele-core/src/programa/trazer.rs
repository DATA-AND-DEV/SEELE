//! `maquina.programa.trazer`: o pack que a metade de servidor guardou, até a
//! pasta do servidor de jogo (API 8 parte 6; spec §8).
//!
//! # O caminho, e quem confere o quê
//!
//! O `.mrpack` enviado por quem configura mora no volume da metade de
//! servidor. A máquina o busca **pela estrela, pelo fluxo de volume de
//! sempre** (a cola do app, `ler_imagem_mod`), e os bytes chegam aqui. Este
//! módulo não sabe de onde eles vieram, e por isso não confia neles: só grava
//! os que têm **o hash do pack que a tela do aceite mostrou** (`oferta.pack`).
//! Um pack trocado no servidor depois do aceite não entra no disco, e a recusa
//! diz os dois hashes.
//!
//! # Já está aqui
//!
//! [`Programa::pack_ja_esta`] responde sem rede quando o destino já tem os
//! bytes aceitos: o MOD que sobe de novo a cada partida do app não busca o
//! pack outra vez.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::hash::Calculo;
use super::{caminho, Programa, Recusa};

/// O pack que chegou e bateu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trazido {
    /// Onde ficou.
    pub destino: PathBuf,
    /// Quantos bytes.
    pub tamanho: u64,
    /// O hash, no algoritmo do aceite.
    pub hash: String,
    /// Se ele já estava no destino, e nada foi buscado nem gravado.
    pub ja_estava: bool,
}

impl Programa {
    /// **O destino já tem o pack aceito?** `Some` quando o arquivo existe e
    /// tem o hash do aceite; `None` quando não existe ou não bate (e então a
    /// metade busca).
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], [`Recusa::SemPackAceito`] e as do caminho.
    pub fn pack_ja_esta(&self, destino: &str) -> Result<Option<Trazido>, Recusa> {
        self.pack_ja_esta_sem_contar(destino)
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    fn pack_ja_esta_sem_contar(&self, destino: &str) -> Result<Option<Trazido>, Recusa> {
        let interior = &self.interior;
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let Some(aceito) = interior.politica.pack.as_ref() else {
            return Err(Recusa::SemPackAceito {
                destino: destino.to_owned(),
            });
        };
        let alvo = caminho::dentro_da_raiz(&interior.raiz, destino, false)?;
        let Some((tamanho, hash)) = super::baixar::hash_do_arquivo(&alvo, aceito.algoritmo) else {
            return Ok(None);
        };
        Ok(
            (hash == aceito.hex.to_ascii_lowercase()).then_some(Trazido {
                destino: alvo,
                tamanho,
                hash,
                ja_estava: true,
            }),
        )
    }

    /// **Grava em `destino` os bytes do pack**, se forem os aceitos.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], [`Recusa::SemPackAceito`],
    /// [`Recusa::PackDiferente`] (nada gravado), as do caminho,
    /// [`Recusa::DiscoCheio`] e [`Recusa::Disco`]. Toda recusa é contada.
    pub fn trazer(&self, destino: &str, bytes: &[u8]) -> Result<Trazido, Recusa> {
        self.trazer_sem_contar(destino, bytes)
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    fn trazer_sem_contar(&self, destino: &str, bytes: &[u8]) -> Result<Trazido, Recusa> {
        let interior = &self.interior;
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let Some(aceito) = interior.politica.pack.as_ref() else {
            return Err(Recusa::SemPackAceito {
                destino: destino.to_owned(),
            });
        };
        let mut calculo = Calculo::novo(aceito.algoritmo);
        calculo.somar(bytes);
        let obtido = calculo.hex();
        let esperado = aceito.hex.to_ascii_lowercase();
        if obtido != esperado {
            return Err(Recusa::PackDiferente {
                algoritmo: aceito.algoritmo,
                esperado,
                obtido,
            });
        }
        let alvo = super::escrever::gravar_de_uma_vez(interior, destino, bytes)?;
        tracing::info!(destino = %alvo.display(), bytes = bytes.len(), "programa trouxe o pack aceito");
        Ok(Trazido {
            destino: alvo,
            tamanho: bytes.len() as u64,
            hash: obtido,
            ja_estava: false,
        })
    }
}
