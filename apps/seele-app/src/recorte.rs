//! **A imagem escolhida que espera o recorte** (1.0.1, pedido do dono: «às
//! vezes a imagem é muito maior, um wallpaper, e eu quero uma parte
//! específica»).
//!
//! Escolher a foto ou a capa do perfil virou dois passos. No primeiro
//! (`escolher_imagem_para_recortar`) o seletor abre, o arquivo é lido e
//! conferido como num envio, e a janela recebe uma **prévia** dele, dentro de
//! um teto; os bytes inteiros ficam aqui. No segundo, a janela devolve a alça
//! e o pedaço que a pessoa escolheu, e o comando de sempre
//! (`enviar_imagem_do_perfil`, `guardar_imagem_do_perfil_padrao`) recorta os
//! bytes inteiros e segue pelo caminho de redução e de envio que já existia.
//!
//! # Por que os bytes ficam deste lado
//!
//! Porque o recorte é tirado da **imagem inteira**, e não da prévia: quem
//! escolhe um canto de um papel de parede em 8K recebe os pixels de verdade
//! daquele canto. E porque levar a imagem inteira à janela é o defeito que a
//! redução existe para impedir (10/11/2026: uma capa em 4K, como um `data:` de
//! megabytes, deixou o aplicativo inteiro lento).
//!
//! # Uma por vez
//!
//! O diálogo de recorte é modal: só há uma imagem esperando. Escolher outra
//! solta a anterior, e a alça de uma imagem solta não vale mais — o comando
//! que a recebe devolve `Mudou`, e nada sobe.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use seele_ffi::perfis::{conferir_imagem, tipo_de_midia, ErroDoPerfil};
use seele_ffi::reducao::{self, Recorte};

/// A imagem que espera o recorte: até 10 MiB, uma por vez.
#[derive(Default)]
pub(crate) struct ImagensARecortar {
    /// A alça da próxima imagem. Nunca repete nesta execução: a alça de uma
    /// imagem que já saiu não abre a seguinte.
    proxima: AtomicU64,
    guardada: Mutex<Option<(u64, Vec<u8>)>>,
}

impl ImagensARecortar {
    /// Guarda `bytes` no lugar do que esperava, e devolve a alça deles.
    pub(crate) fn guardar(&self, bytes: Vec<u8>) -> u64 {
        let alca = self.proxima.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut guardada) = self.guardada.lock() {
            *guardada = Some((alca, bytes));
        }
        alca
    }

    /// Os bytes da alça, que deixam de esperar. `None` para a alça de uma
    /// imagem que já saiu ou foi trocada por outra.
    pub(crate) fn tirar(&self, alca: u64) -> Option<Vec<u8>> {
        let mut guardada = self.guardada.lock().ok()?;
        match guardada.take() {
            Some((dela, bytes)) if dela == alca => Some(bytes),
            outra => {
                *guardada = outra;
                None
            }
        }
    }
}

/// O que a janela recebe para desenhar o passo de recorte.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ParaRecortar {
    /// `false` é ter fechado o seletor sem escolher: o desfecho mais comum.
    pub(crate) escolheu: bool,
    /// O nome com que a janela devolve esta imagem ao comando que a recorta.
    pub(crate) alca: u64,
    /// A prévia em `data:`, dentro de `reducao::TETO_DA_PREVIA_DO_RECORTE`.
    pub(crate) previa: String,
    /// A largura da imagem inteira, como ela é vista: a medida do recorte.
    pub(crate) largura: u32,
    /// A altura da imagem inteira, como ela é vista.
    pub(crate) altura: u32,
    /// Se é uma animação: o recorte vale para todos os quadros.
    pub(crate) animada: bool,
}

impl ParaRecortar {
    /// O seletor fechado sem escolha.
    pub(crate) fn nada() -> Self {
        Self {
            escolheu: false,
            alca: 0,
            previa: String::new(),
            largura: 0,
            altura: 0,
            animada: false,
        }
    }
}

/// **Confere a imagem escolhida e prepara a prévia dela**, guardando os bytes
/// inteiros para o recorte. A conferência é a do envio (até 10 MiB, o tipo
/// pelos bytes, o HEIC nomeado), feita **antes** de o diálogo abrir: quem
/// escolhe um HEIC ouve que é HEIC, e não recorta uma imagem que não vai subir.
///
/// Decodifica a imagem: chame fora da linha principal.
///
/// # Errors
///
/// Os de [`conferir_imagem`], e [`ErroDoPerfil::TipoRecusado`] para o que não
/// decodifica ou tem pixels demais.
pub(crate) fn preparar(
    a_recortar: &ImagensARecortar,
    bytes: Vec<u8>,
) -> Result<ParaRecortar, ErroDoPerfil> {
    conferir_imagem(&bytes)?;
    let previa = reducao::previa_do_recorte(&bytes).map_err(|_| ErroDoPerfil::TipoRecusado)?;
    let tipo = conferir_imagem(&previa.bytes).map_err(|_| ErroDoPerfil::TipoRecusado)?;
    Ok(ParaRecortar {
        escolheu: true,
        alca: a_recortar.guardar(bytes),
        previa: format!(
            "data:{};base64,{}",
            tipo_de_midia(tipo),
            seele_ffi::base64_de(&previa.bytes)
        ),
        largura: previa.largura,
        altura: previa.altura,
        animada: previa.animada,
    })
}

/// **A imagem escolhida, recortada e do tamanho em que aparece**
/// (`reducao::imagem_do_perfil_recortada`: o pedaço, e depois o teto de
/// sempre — a capa em até 1280 × 720, a foto em até 512). É o que sobe ao
/// servidor e o que fica guardado no perfil padrão.
///
/// Decodifica a imagem: chame fora da linha principal.
///
/// # Errors
///
/// [`ErroDoPerfil::Mudou`] para um recorte sem pixel nenhum dentro da imagem (a
/// janela mediu outra), e [`ErroDoPerfil::TipoRecusado`] para o que não
/// decodifica ou tem pixels demais.
pub(crate) fn recortada(
    bytes: &[u8],
    capa: bool,
    recorte: Recorte,
) -> Result<Vec<u8>, ErroDoPerfil> {
    reducao::imagem_do_perfil_recortada(bytes, capa, recorte).map_err(|motivo| {
        if motivo == reducao::RECORTE_VAZIO {
            ErroDoPerfil::Mudou
        } else {
            ErroDoPerfil::TipoRecusado
        }
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn png(largura: u32, altura: u32) -> Vec<u8> {
        let imagem = image::RgbaImage::from_fn(largura, altura, |x, _| {
            image::Rgba(if x < largura / 2 {
                [200, 30, 30, 255]
            } else {
                [30, 30, 200, 255]
            })
        });
        let mut saida = Vec::new();
        image::DynamicImage::ImageRgba8(imagem)
            .write_to(
                &mut std::io::Cursor::new(&mut saida),
                image::ImageFormat::Png,
            )
            .expect("gravar o PNG de teste");
        saida
    }

    fn da_previa(previa: &str) -> image::RgbaImage {
        let base64 = previa
            .strip_prefix("data:image/png;base64,")
            .expect("a prévia de um PNG é um `data:` de PNG");
        let bytes = seele_ffi::de_base64(base64).expect("base64");
        image::load_from_memory(&bytes)
            .expect("decodifica")
            .to_rgba8()
    }

    /// **A janela recebe a prévia dentro do teto, e não a imagem**: um papel
    /// de parede em 4K chega com 2048 px de lado, com as medidas da inteira ao
    /// lado, e os bytes inteiros esperam aqui pela alça.
    #[test]
    fn a_previa_vai_a_janela_e_os_bytes_ficam() {
        let a_recortar = ImagensARecortar::default();
        let original = png(3840, 2160);
        let pronta = preparar(&a_recortar, original.clone()).expect("prepara");
        assert!(pronta.escolheu);
        assert_eq!((pronta.largura, pronta.altura), (3840, 2160));
        assert!(!pronta.animada);
        assert_eq!(da_previa(&pronta.previa).dimensions(), (2048, 1152));
        assert_eq!(a_recortar.tirar(pronta.alca), Some(original));
        // Tirada, a alça não vale mais.
        assert_eq!(a_recortar.tirar(pronta.alca), None);
    }

    /// **Uma por vez**: escolher outra solta a anterior, e a alça velha não
    /// abre a nova.
    #[test]
    fn escolher_outra_solta_a_anterior() {
        let a_recortar = ImagensARecortar::default();
        let primeira = preparar(&a_recortar, png(64, 64)).expect("prepara");
        let segunda = preparar(&a_recortar, png(32, 32)).expect("prepara");
        assert_ne!(primeira.alca, segunda.alca);
        assert_eq!(a_recortar.tirar(primeira.alca), None);
        // A alça errada não gasta a que espera.
        assert_eq!(a_recortar.tirar(segunda.alca), Some(png(32, 32)));
    }

    /// **A recusa vem antes do diálogo**: o HEIC pelo nome, o que não é
    /// imagem, e a que tem pixels demais. Nada fica esperando.
    #[test]
    fn o_que_nao_sobe_nao_chega_ao_recorte() {
        let a_recortar = ImagensARecortar::default();
        let heic = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic".to_vec();
        assert!(matches!(
            preparar(&a_recortar, heic),
            Err(ErroDoPerfil::Heic)
        ));
        assert!(matches!(
            preparar(&a_recortar, b"nada disto".to_vec()),
            Err(ErroDoPerfil::TipoRecusado)
        ));
        assert!(matches!(
            preparar(&a_recortar, Vec::new()),
            Err(ErroDoPerfil::ImagemVazia)
        ));
        let mut bomba = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(9000, 5000))
            .write_to(
                &mut std::io::Cursor::new(&mut bomba),
                image::ImageFormat::Png,
            )
            .expect("gravar");
        assert!(matches!(
            preparar(&a_recortar, bomba),
            Err(ErroDoPerfil::TipoRecusado)
        ));
        assert_eq!(a_recortar.tirar(1), None);
    }

    /// O recorte pedido sai, e o vazio é `Mudou`: a janela mediu outra imagem.
    #[test]
    fn o_pedaco_pedido_sai_e_o_vazio_e_dito() {
        let original = png(400, 200);
        let direita = Recorte {
            x: 220,
            y: 10,
            largura: 160,
            altura: 160,
        };
        let saida = recortada(&original, false, direita).expect("recorta");
        let imagem = image::load_from_memory(&saida).expect("abre").to_rgba8();
        assert_eq!(imagem.dimensions(), (160, 160));
        assert!(imagem.pixels().all(|pixel| pixel.0 == [30, 30, 200, 255]));
        let fora = Recorte {
            x: 400,
            y: 0,
            largura: 10,
            altura: 10,
        };
        assert!(matches!(
            recortada(&original, false, fora),
            Err(ErroDoPerfil::Mudou)
        ));
    }
}
