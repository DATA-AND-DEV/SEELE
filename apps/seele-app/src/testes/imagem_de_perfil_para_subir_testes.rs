use super::imagem_de_perfil_para_subir;
use seele_ffi::perfis::{ErroDoPerfil, LadoDoPerfil};

fn png(largura: u32, altura: u32) -> Vec<u8> {
    let imagem = image::RgbaImage::from_pixel(largura, altura, image::Rgba([9, 90, 200, 255]));
    let mut saida = Vec::new();
    image::DynamicImage::ImageRgba8(imagem)
        .write_to(
            &mut std::io::Cursor::new(&mut saida),
            image::ImageFormat::Png,
        )
        .expect("gravar o PNG de teste");
    saida
}

fn medidas(bytes: &[u8]) -> (u32, u32) {
    let imagem = image::load_from_memory(bytes).expect("decodifica");
    (imagem.width(), imagem.height())
}

/// **O que sobe ao servidor é do tamanho em que aparece.** Uma capa em 4K
/// subia inteira, e quem a recebia num cliente de antes da redução ao
/// mostrar ficava lento (relato de 10/11/2026).
#[test]
fn a_imagem_escolhida_sobe_reduzida() {
    tauri::async_runtime::block_on(async {
        let capa = imagem_de_perfil_para_subir(png(3840, 2160), LadoDoPerfil::Capa)
            .await
            .expect("reduz");
        assert_eq!(medidas(&capa), (1280, 720));
        let foto = imagem_de_perfil_para_subir(png(2000, 2000), LadoDoPerfil::Foto)
            .await
            .expect("reduz");
        assert_eq!(medidas(&foto), (512, 512));
        // O que já cabe sobe com os mesmos bytes.
        let pequena = png(640, 360);
        assert_eq!(
            imagem_de_perfil_para_subir(pequena.clone(), LadoDoPerfil::Capa)
                .await
                .expect("passa"),
            pequena
        );
        // Pixels demais: recusada. O que não é imagem segue, e quem a
        // recusa é a conferência do envio.
        assert!(matches!(
            imagem_de_perfil_para_subir(png(9000, 5000), LadoDoPerfil::Capa).await,
            Err(ErroDoPerfil::TipoRecusado)
        ));
        assert_eq!(
            imagem_de_perfil_para_subir(b"nada".to_vec(), LadoDoPerfil::Foto)
                .await
                .expect("segue"),
            b"nada"
        );
    });
}
