use super::imagem_de_perfil_para_subir;
use seele_ffi::perfis::{ErroDoPerfil, LadoDoPerfil};
use seele_ffi::reducao::Recorte;

/// A metade esquerda de uma cor e a direita de outra: pelo que sobra se sabe
/// que pedaço saiu.
const ESQUERDA: [u8; 4] = [9, 90, 200, 255];
const DIREITA: [u8; 4] = [220, 150, 20, 255];

fn png(largura: u32, altura: u32) -> Vec<u8> {
    let imagem = image::RgbaImage::from_fn(largura, altura, |x, _| {
        image::Rgba(if x < largura / 2 { ESQUERDA } else { DIREITA })
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

fn aberta(bytes: &[u8]) -> image::RgbaImage {
    image::load_from_memory(bytes)
        .expect("decodifica")
        .to_rgba8()
}

fn medidas(bytes: &[u8]) -> (u32, u32) {
    aberta(bytes).dimensions()
}

fn inteira(largura: u32, altura: u32) -> Recorte {
    Recorte {
        x: 0,
        y: 0,
        largura,
        altura,
    }
}

/// **O que sobe ao servidor é do tamanho em que aparece.** Uma capa em 4K
/// subia inteira, e quem a recebia num cliente de antes da redução ao
/// mostrar ficava lento (relato de 10/11/2026). Com o recorte que é a imagem
/// inteira, o caminho é o de antes do recorte.
#[test]
fn a_imagem_escolhida_sobe_reduzida() {
    tauri::async_runtime::block_on(async {
        let capa =
            imagem_de_perfil_para_subir(png(3840, 2160), LadoDoPerfil::Capa, inteira(3840, 2160))
                .await
                .expect("reduz");
        assert_eq!(medidas(&capa), (1280, 720));
        let foto =
            imagem_de_perfil_para_subir(png(2000, 2000), LadoDoPerfil::Foto, inteira(2000, 2000))
                .await
                .expect("reduz");
        assert_eq!(medidas(&foto), (512, 512));
        // O que já cabe sobe com os mesmos bytes.
        let pequena = png(640, 360);
        assert_eq!(
            imagem_de_perfil_para_subir(pequena.clone(), LadoDoPerfil::Capa, inteira(640, 360))
                .await
                .expect("passa"),
            pequena
        );
        // Pixels demais: recusada, e o que não é imagem também. Com o recorte
        // a conferência é antes do diálogo, e aqui nada segue «como veio».
        assert!(matches!(
            imagem_de_perfil_para_subir(png(9000, 5000), LadoDoPerfil::Capa, inteira(100, 100))
                .await,
            Err(ErroDoPerfil::TipoRecusado)
        ));
        assert!(matches!(
            imagem_de_perfil_para_subir(b"nada".to_vec(), LadoDoPerfil::Foto, inteira(1, 1)).await,
            Err(ErroDoPerfil::TipoRecusado)
        ));
    });
}

/// **O que sobe é o pedaço que a pessoa escolheu**, e não a imagem encolhida
/// nem o centro dela: de um papel de parede em 4K, o canto da direita sobe
/// inteiro da cor da direita, na proporção pedida e dentro do teto do lado.
#[test]
fn a_imagem_escolhida_sobe_recortada() {
    tauri::async_runtime::block_on(async {
        // A foto: um quadrado de 1800 px na metade direita, em 512 × 512.
        let direita = Recorte {
            x: 2000,
            y: 200,
            largura: 1800,
            altura: 1800,
        };
        let foto = imagem_de_perfil_para_subir(png(3840, 2160), LadoDoPerfil::Foto, direita)
            .await
            .expect("recorta");
        let foto = aberta(&foto);
        assert_eq!(foto.dimensions(), (512, 512));
        assert!(foto.pixels().all(|pixel| pixel.0 == DIREITA));

        // A capa: um 16:9 na metade esquerda, maior que o teto, em 1280 × 720.
        let esquerda = Recorte {
            x: 100,
            y: 400,
            largura: 1600,
            altura: 900,
        };
        let capa = imagem_de_perfil_para_subir(png(3840, 2160), LadoDoPerfil::Capa, esquerda)
            .await
            .expect("recorta");
        let capa = aberta(&capa);
        assert_eq!(capa.dimensions(), (1280, 720));
        assert!(capa.pixels().all(|pixel| pixel.0 == ESQUERDA));

        // O recorte sem pixel nenhum dentro da imagem não sobe nada.
        let fora = Recorte {
            x: 5000,
            y: 0,
            largura: 10,
            altura: 10,
        };
        assert!(matches!(
            imagem_de_perfil_para_subir(png(640, 360), LadoDoPerfil::Capa, fora).await,
            Err(ErroDoPerfil::Mudou)
        ));
    });
}
