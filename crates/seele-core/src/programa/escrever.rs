//! `maquina.programa.escrever`: um arquivo de texto pequeno dentro da pasta do
//! servidor de jogo (API 8 parte 4).
//!
//! **Por que o produto, e não o MOD:** o servidor de jogo lê a configuração de
//! um arquivo (`server.properties`) e os termos aceitos de outro (`eula.txt`),
//! e a metade de máquina não tem disco nenhum fora do `programa`. Os dois são
//! texto de poucos KiB, e o que muda neles é o que o esquema da receita diz
//! que **não** pede aceite novo (spec §4): por isso escrever não passa pela
//! conferência do aceite, só pela da pasta e pela do disco.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::{caminho, Programa, Recusa};

/// O maior texto que `escrever` grava: 64 KiB. Um `server.properties` tem
/// cerca de 1,5 KiB; o teto é para um MOD que confunda `escrever` com um
/// download.
pub const MAX_BYTES_ESCRITOS: u64 = 64 * 1024;

impl Programa {
    /// Escreve `texto` em `destino`, no lugar do que houver, de uma vez:
    /// grava ao lado e renomeia, para o jogo nunca ler meio arquivo.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], [`Recusa::TextoGrandeDemais`], as do caminho,
    /// [`Recusa::DiscoCheio`] e [`Recusa::Disco`]. Toda recusa é contada.
    pub fn escrever(&self, destino: &str, texto: &str) -> Result<PathBuf, Recusa> {
        self.escrever_sem_contar(destino, texto)
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    fn escrever_sem_contar(&self, destino: &str, texto: &str) -> Result<PathBuf, Recusa> {
        let interior = &self.interior;
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let tamanho = texto.len() as u64;
        if tamanho > MAX_BYTES_ESCRITOS {
            return Err(Recusa::TextoGrandeDemais {
                tamanho,
                teto: MAX_BYTES_ESCRITOS,
            });
        }
        let alvo = caminho::dentro_da_raiz(&interior.raiz, destino, false)?;
        let disco = |caminho: &std::path::Path| {
            let caminho = caminho.to_path_buf();
            move |erro: std::io::Error| Recusa::Disco {
                caminho,
                motivo: erro.to_string(),
            }
        };
        // O mesmo cadeado da reserva dos downloads, e a mesma conta: o que a
        // pasta ocupa, mais o que os downloads em andamento reservaram, mais o
        // que este texto acrescenta ao arquivo que ele substitui.
        let reservado = interior.reservado.lock().map_err(|_| Recusa::Disco {
            caminho: interior.raiz.clone(),
            motivo: "o cadeado da reserva de disco foi envenenado".to_owned(),
        })?;
        let usado = caminho::ocupado(&interior.raiz, &|c| {
            c.extension()
                .is_some_and(|e| e == super::baixar::EXTENSAO_PARCIAL)
        })
        .saturating_add(*reservado);
        let substitui = std::fs::symlink_metadata(&alvo)
            .ok()
            .filter(std::fs::Metadata::is_file)
            .map_or(0, |m| m.len());
        let pedido = tamanho.saturating_sub(substitui);
        let teto = interior.politica.teto_de_disco;
        if usado.saturating_add(pedido) > teto {
            return Err(Recusa::DiscoCheio {
                usado,
                pedido,
                teto,
            });
        }
        let pasta = alvo.parent().unwrap_or(&interior.raiz).to_path_buf();
        std::fs::create_dir_all(&pasta).map_err(disco(&pasta))?;
        // Conferido de novo depois de criar: criar pode ter passado por um link.
        caminho::dentro_da_raiz(&interior.raiz, destino, false)?;
        let nome = alvo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parcial = pasta.join(format!(
            ".{nome}.{}.{}",
            interior.proximo_numero(),
            super::baixar::EXTENSAO_PARCIAL
        ));
        let gravar = || -> std::io::Result<()> {
            let mut arquivo = std::fs::File::create(&parcial)?;
            arquivo.write_all(texto.as_bytes())?;
            arquivo.sync_all()?;
            std::fs::rename(&parcial, &alvo)
        };
        let gravado = gravar();
        drop(reservado);
        if let Err(erro) = gravado {
            let _ = std::fs::remove_file(&parcial);
            return Err(disco(&alvo)(erro));
        }
        tracing::info!(destino = %alvo.display(), bytes = tamanho, "programa escreveu");
        Ok(alvo)
    }
}
