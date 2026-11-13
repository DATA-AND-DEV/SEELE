//! **A redução de uma imagem de MOD ao tamanho em que ela aparece** (rascunho
//! da API 6, §4 e §12; tarefa 6 da F3, Step 5).
//!
//! # Por que reduzir aqui, e não deixar a janela mostrar o que veio
//!
//! O custo de uma imagem na janela é o **quadro decodificado**, e não o
//! arquivo: uma capa de 960 × 320 tem 1,23 MB por quadro, e um GIF de 60
//! quadros, 74 MB (a conta do PERFIS). Um MOD que mande uma foto de câmera como
//! capa faria a janela decodificar dezenas de megapixels para mostrar uma faixa
//! de 320 px de altura. Reduzir no Rust, antes de a imagem atravessar a ponte,
//! poupa a ponte, a memória da janela e a decodificação dela.
//!
//! # O uso é um nome, e não um tamanho
//!
//! A janela diz **para que** a imagem serve (`"capa"`), e o teto daquele uso
//! mora aqui. Nem a janela nem o MOD escolhem números: o teto de cada uso é
//! deste arquivo, e a mídia de MOD sem uso conhecido vai pelo teto geral
//! ([`TETO_DA_MIDIA_DE_MOD`]) — até 11/11/2026 ela não era reduzida.
//!
//! # Os números são de partida
//!
//! O lado, os quadros e a duração são a proposta do rascunho (§4), e a medida
//! que os fixa é a M3 (tarefa 11 do plano da F3). Até lá eles ficam aqui, num
//! lugar só.
//!
//! **Mora no `seele-sessao`** desde a Tarefa 2.6 da F4, atrás da feature
//! `reducao` (que traz o `image`): o navegador reduz a capa pelos mesmos
//! números e pelo mesmo código, e o desktop a lê pelo `seele-ffi`.

use std::io::Cursor;

use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, Frame, ImageFormat};

/// O que uma imagem pode ser no máximo, para um uso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Teto {
    /// A largura do maior lugar em que ela aparece, em pixels.
    pub largura: u32,
    /// A altura do maior lugar em que ela aparece, em pixels.
    pub altura: u32,
    /// Quantos quadros uma imagem animada mantém.
    pub quadros: usize,
    /// Quanto tempo uma imagem animada dura, somados os quadros mantidos.
    pub milissegundos: u32,
}

/// **A capa de uma pessoa** (`pessoa.capa`): o maior lugar em que ela aparece é
/// o cartão da chamada, e 960 × 320 é a conta do PERFIS; 60 quadros e 10 s, a
/// proposta do rascunho §4. A M3 mede.
pub const TETO_DA_CAPA: Teto = Teto {
    largura: 960,
    altura: 320,
    quadros: 60,
    milissegundos: 10_000,
};

/// **A capa de um perfil oficial, para a janela.** A capa aparece cortada
/// (`background-size: cover`) numa faixa, e 1280 × 720 cobre a maior delas com
/// folga: uma capa 4K cabe nele com um nono dos pixels.
///
/// **O tamanho do arquivo também conta, e não só o dos pixels.** A imagem vai
/// à janela como `data:`, e a janela relê esse texto inteiro em cada elemento
/// que a mostra: medido em 10/11/2026, 37 ms por elemento com 10 MB, 3,7 ms
/// com 1 MB.
///
/// **Por que existe** (10/11/2026, relato do dono): uma capa em 4K deixou o
/// aplicativo inteiro lento. A imagem do perfil ia à janela do tamanho que
/// subiu, como um `data:` de megabytes, e a janela a decodificava em 33 MB por
/// lugar em que aparecia. As imagens de MOD já passavam por este arquivo; as
/// do perfil oficial, não.
pub const TETO_DA_CAPA_DO_PERFIL: Teto = Teto {
    largura: 1280,
    altura: 720,
    quadros: 60,
    milissegundos: 10_000,
};

/// **A foto de um perfil oficial, para a janela**: o maior lugar em que ela
/// aparece é o cartão do perfil, e 512 px cobrem uma tela de densidade dupla.
pub const TETO_DA_FOTO_DO_PERFIL: Teto = Teto {
    largura: 512,
    altura: 512,
    quadros: 60,
    milissegundos: 10_000,
};

/// **A imagem de um perfil oficial como ela vai à janela**: reduzida ao teto
/// do lado (`capa`, ou a foto). Uma que já cabe sai com os mesmos bytes.
///
/// # Errors
///
/// `formato-desconhecido`, quando os bytes não são uma imagem que este
/// produto decodifica.
pub fn imagem_do_perfil_para_a_janela(bytes: &[u8], capa: bool) -> Result<Vec<u8>, String> {
    let teto = if capa {
        TETO_DA_CAPA_DO_PERFIL
    } else {
        TETO_DA_FOTO_DO_PERFIL
    };
    reduzir(bytes, &teto).map(|reduzida| reduzida.bytes)
}

/// **O pedaço de uma imagem que a pessoa escolheu**, em pixels da imagem
/// **como ela é vista** (com a orientação do EXIF aplicada): o canto de cima à
/// esquerda e o tamanho. Quem o desenha é o passo de recorte do editor do
/// perfil (`ui/camada-recorte.js`), sobre a prévia de [`previa_do_recorte`], e
/// as medidas que ele usa são as que ela diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Recorte {
    /// A coluna do canto de cima à esquerda.
    pub x: u32,
    /// A linha do canto de cima à esquerda.
    pub y: u32,
    /// A largura do pedaço.
    pub largura: u32,
    /// A altura do pedaço.
    pub altura: u32,
}

impl Recorte {
    /// O recorte aparado a uma imagem de `largura × altura`: a conta da tela é
    /// em frações, e um pixel de arredondamento para fora não é recusa. `None`
    /// quando não sobra pixel nenhum. As contas saturam: o canto que começa
    /// depois da borda sobra com zero, e não com a volta de um `u32`.
    fn dentro_de(self, largura: u32, altura: u32) -> Option<Self> {
        let aparado = Self {
            x: self.x,
            y: self.y,
            largura: self.largura.min(largura.saturating_sub(self.x)),
            altura: self.altura.min(altura.saturating_sub(self.y)),
        };
        (aparado.largura > 0 && aparado.altura > 0).then_some(aparado)
    }

    /// Se o recorte é a imagem inteira.
    fn e_a_imagem_inteira(self, largura: u32, altura: u32) -> bool {
        self.x == 0 && self.y == 0 && self.largura == largura && self.altura == altura
    }
}

/// O nome da recusa de um recorte que não tem pixel nenhum dentro da imagem.
pub const RECORTE_VAZIO: &str = "recorte-vazio";

/// **A imagem de um perfil recortada, como ela sobe e fica guardada**: o
/// pedaço escolhido, e depois o **mesmo teto** de
/// [`imagem_do_perfil_para_a_janela`] (a capa em até 1280 × 720, a foto em
/// até 512; na animada, os mesmos quadros e a mesma duração).
///
/// **O formato é o de sempre**: JPEG fica JPEG, o resto parado vira PNG, e a
/// animação segue animada, em GIF — o recorte é feito quadro a quadro, e não
/// num canvas, que a congelaria. O recorte que é a imagem inteira, numa imagem
/// sem orientação de EXIF, sai pelo caminho de antes, byte a byte.
///
/// # Errors
///
/// `formato-desconhecido` e [`GRANDE_DEMAIS`], como [`reduzir`], e
/// [`RECORTE_VAZIO`].
pub fn imagem_do_perfil_recortada(
    bytes: &[u8],
    capa: bool,
    recorte: Recorte,
) -> Result<Vec<u8>, String> {
    let teto = if capa {
        TETO_DA_CAPA_DO_PERFIL
    } else {
        TETO_DA_FOTO_DO_PERFIL
    };
    recortar(bytes, &teto, recorte).map(|recortada| recortada.bytes)
}

/// **Recorta `bytes` e os reduz ao teto**, numa passada só: decodificar duas
/// vezes uma imagem em 8K, ou regravar um JPEG duas vezes, é pagar o dobro
/// para perder qualidade.
///
/// # Errors
///
/// `formato-desconhecido` e [`GRANDE_DEMAIS`], como [`reduzir`], e
/// [`RECORTE_VAZIO`].
pub fn recortar(bytes: &[u8], teto: &Teto, recorte: Recorte) -> Result<Reduzida, String> {
    let formato = image::guess_format(bytes).map_err(|_| desconhecido())?;
    let (largura, altura) = image::ImageReader::with_format(Cursor::new(bytes), formato)
        .into_dimensions()
        .map_err(|_| desconhecido())?;
    if u64::from(largura) * u64::from(altura) > TETO_DE_PIXELS {
        return Err(GRANDE_DEMAIS.to_owned());
    }
    // A animação não tem orientação de EXIF: as medidas do cabeçalho são as
    // que a pessoa viu.
    if e_animada(bytes, formato)? {
        let pedaco = recorte
            .dentro_de(largura, altura)
            .ok_or_else(|| RECORTE_VAZIO.to_owned())?;
        if pedaco.e_a_imagem_inteira(largura, altura) {
            return reduzir(bytes, teto);
        }
        return match quadros_dentro_do_teto(bytes, formato, teto, Some(pedaco))? {
            Some(quadros) => reduzir_animacao(quadros, teto),
            // Um quadro só dentro do teto de duração: o primeiro, parado.
            None => {
                let primeiro = image::load_from_memory_with_format(bytes, formato)
                    .map_err(|_| desconhecido())?;
                recortar_parada(&primeiro, formato, teto, pedaco)
            }
        };
    }
    let (imagem, orientada) = aberta_como_se_ve(bytes, formato)?;
    let pedaco = recorte
        .dentro_de(imagem.width(), imagem.height())
        .ok_or_else(|| RECORTE_VAZIO.to_owned())?;
    if !orientada && pedaco.e_a_imagem_inteira(imagem.width(), imagem.height()) {
        return reduzir(bytes, teto);
    }
    recortar_parada(&imagem, formato, teto, pedaco)
}

/// O pedaço de uma imagem parada, dentro do teto e gravado.
fn recortar_parada(
    imagem: &image::DynamicImage,
    formato: ImageFormat,
    teto: &Teto,
    pedaco: Recorte,
) -> Result<Reduzida, String> {
    let mut cortada = imagem.crop_imm(pedaco.x, pedaco.y, pedaco.largura, pedaco.altura);
    let (mut largura, mut altura) = (cortada.width(), cortada.height());
    if largura > teto.largura || altura > teto.altura {
        (largura, altura) = caber(largura, altura, teto);
        cortada = cortada.resize_exact(largura, altura, FilterType::Triangle);
    }
    Ok(Reduzida {
        bytes: gravar_parada(&cortada, formato)?,
        decodificados: decodificados(largura, altura, 1),
    })
}

/// Se os bytes são uma animação de mais de um quadro. Lê só os dois primeiros
/// quadros: contar os de um GIF longo seria decodificá-lo inteiro.
fn e_animada(bytes: &[u8], formato: ImageFormat) -> Result<bool, String> {
    let dois = Teto {
        largura: u32::MAX,
        altura: u32::MAX,
        quadros: 2,
        milissegundos: u32::MAX,
    };
    Ok(quadros_dentro_do_teto(bytes, formato, &dois, None)?.is_some())
}

/// **A imagem parada como a pessoa a vê**: decodificada, com a orientação do
/// EXIF aplicada. Uma foto de celular em pé é gravada deitada, com a
/// orientação anotada; o navegador a mostra em pé, e um recorte medido sobre a
/// imagem deitada cortaria outro pedaço. O `bool` diz se a orientação mudou
/// alguma coisa.
fn aberta_como_se_ve(
    bytes: &[u8],
    formato: ImageFormat,
) -> Result<(image::DynamicImage, bool), String> {
    use image::metadata::Orientation;
    use image::ImageDecoder as _;

    let mut decodificador = image::ImageReader::with_format(Cursor::new(bytes), formato)
        .into_decoder()
        .map_err(|_| desconhecido())?;
    let orientacao = decodificador
        .orientation()
        .unwrap_or(Orientation::NoTransforms);
    let mut imagem =
        image::DynamicImage::from_decoder(decodificador).map_err(|_| desconhecido())?;
    imagem.apply_orientation(orientacao);
    Ok((imagem, orientacao != Orientation::NoTransforms))
}

/// **A prévia do passo de recorte**: a imagem parada (o primeiro quadro de uma
/// animação) como a pessoa a vê, reduzida a [`TETO_DA_PREVIA_DO_RECORTE`], e as
/// medidas da imagem inteira, que são as do [`Recorte`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviaDoRecorte {
    /// A prévia: JPEG quando a escolhida é JPEG, PNG no resto.
    pub bytes: Vec<u8>,
    /// A largura da imagem inteira, como ela é vista.
    pub largura: u32,
    /// A altura da imagem inteira, como ela é vista.
    pub altura: u32,
    /// Se a imagem é uma animação: o recorte vale para todos os quadros.
    pub animada: bool,
}

/// **O maior lado da prévia do recorte.** A moldura do diálogo tem algumas
/// centenas de pixels e o zoom vai a oito vezes: 2048 seguram a nitidez do que
/// se vê sem levar à janela os megabytes de um papel de parede em 8K — que é o
/// defeito que este arquivo existe para impedir. O que sai do recorte é
/// cortado da imagem inteira, e não desta prévia.
pub const TETO_DA_PREVIA_DO_RECORTE: Teto = Teto {
    largura: 2048,
    altura: 2048,
    quadros: 2,
    milissegundos: u32::MAX,
};

/// **O que o passo de recorte mostra de uma imagem escolhida.**
///
/// # Errors
///
/// `formato-desconhecido` e [`GRANDE_DEMAIS`], como [`reduzir`].
pub fn previa_do_recorte(bytes: &[u8]) -> Result<PreviaDoRecorte, String> {
    let teto = &TETO_DA_PREVIA_DO_RECORTE;
    let formato = image::guess_format(bytes).map_err(|_| desconhecido())?;
    let (largura, altura) = image::ImageReader::with_format(Cursor::new(bytes), formato)
        .into_dimensions()
        .map_err(|_| desconhecido())?;
    if u64::from(largura) * u64::from(altura) > TETO_DE_PIXELS {
        return Err(GRANDE_DEMAIS.to_owned());
    }
    // Os dois primeiros quadros dizem se é animação, e o primeiro já chega
    // dentro do teto.
    if let Some(quadros) = quadros_dentro_do_teto(bytes, formato, teto, None)? {
        let primeiro = quadros
            .into_iter()
            .next()
            .map(Frame::into_buffer)
            .ok_or_else(desconhecido)?;
        return Ok(PreviaDoRecorte {
            bytes: gravar_parada(&image::DynamicImage::ImageRgba8(primeiro), ImageFormat::Png)?,
            largura,
            altura,
            animada: true,
        });
    }
    let (mut imagem, _) = aberta_como_se_ve(bytes, formato)?;
    let (largura, altura) = (imagem.width(), imagem.height());
    if largura > teto.largura || altura > teto.altura {
        let (menor_largura, menor_altura) = caber(largura, altura, teto);
        imagem = imagem.resize_exact(menor_largura, menor_altura, FilterType::Triangle);
    }
    Ok(PreviaDoRecorte {
        bytes: gravar_parada(&imagem, formato)?,
        largura,
        altura,
        animada: false,
    })
}

/// **A mídia de um MOD que não nomeia uso**: a imagem de uma região, o fundo
/// de uma `tela`, um retrato. O maior lugar em que ela aparece é uma página de
/// MOD do tamanho da janela, e 1920 × 1080 a cobre: uma imagem em 4K cabe nele
/// com um quarto dos pixels.
///
/// **Por que existe** (11/11/2026, a varredura do incidente da capa em 4K):
/// a redução só valia para o uso que a janela nomeava (`capa`, `avatar`), e
/// tudo o mais que um MOD mostra — do pacote ou do servidor dele — ia à janela
/// do tamanho que veio, como um `data:` de até 10 MiB, sem teto de pixels. Um
/// PNG de 30 000 × 30 000 chegava inteiro ao `decode()` da janela.
///
/// Os quadros e a duração só valem para a animação **maior** que o lugar, que
/// é regravada; a que cabe segue com os bytes que tinha ([`para_a_janela`]).
pub const TETO_DA_MIDIA_DE_MOD: Teto = Teto {
    largura: 1920,
    altura: 1080,
    quadros: 60,
    milissegundos: 10_000,
};

/// **O fundo de uma caixa de MOD** (`fundoDeMidia`): cortado
/// (`background-size: cover`) atrás do conteúdo, como a capa de um perfil, e
/// com o teto dela. É a imagem que mora no **estilo** do elemento, onde o
/// tamanho do texto do `data:` custa a cada leitura
/// ([`TETO_DA_CAPA_DO_PERFIL`]).
pub const TETO_DO_FUNDO_DE_MOD: Teto = TETO_DA_CAPA_DO_PERFIL;

/// **A prévia de uma imagem da conversa** — o anexo que alguém mandou, e a
/// imagem de um link. A folha a desenha com no máximo 320 px de altura
/// (`.anexo-desenho img`), e 1280 × 720 cobre isso numa tela de densidade
/// dupla.
///
/// **Por que existe** (11/11/2026, a varredura do incidente da capa em 4K): o
/// teto da prévia era só de bytes ([`crate::preview::PREVIEW_LIMIT`]), e o
/// próprio `preview.rs` dizia que os pixels não tinham limite. A imagem ia à
/// janela do tamanho que veio, e a conversa, refeita a cada mensagem, punha o
/// `data:` inteiro num `<img>` novo.
///
/// Os quadros e a duração só valem para a animação **maior** que o lugar; o
/// GIF que cabe segue inteiro ([`para_a_janela`]).
pub const TETO_DA_PREVIA: Teto = Teto {
    largura: 1280,
    altura: 720,
    quadros: 60,
    milissegundos: 10_000,
};

/// O que a prévia de uma imagem da conversa vira a caminho da janela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviaNaJanela {
    /// O `data:` inteiro, com o tipo que os bytes **de saída** provaram ser
    /// (a redução pode trocar o formato: um WebP parado sai PNG).
    Pronta(String),
    /// A imagem declara mais pixels que [`TETO_DE_PIXELS`]: recusada pelo
    /// cabeçalho, sem ser aberta aqui nem na janela.
    PixelsDemais {
        /// A largura declarada.
        largura: u32,
        /// A altura declarada.
        altura: u32,
    },
    /// Os bytes começam como os de uma imagem e não abrem como uma: sem as
    /// medidas, não há como saber o que a janela decodificaria.
    Ilegivel,
}

/// **A prévia de uma imagem da conversa, pronta para a janela**: recusada
/// pelo cabeçalho quando declara pixels demais, reduzida a [`TETO_DA_PREVIA`]
/// quando é maior que o lugar, e com os bytes que tinha quando já cabe. Os
/// dois produtores da prévia (o desktop e o navegador) passam por aqui.
#[must_use]
pub fn previa_na_janela(bytes: Vec<u8>) -> PreviaNaJanela {
    let Ok((largura, altura)) = medidas(&bytes) else {
        return PreviaNaJanela::Ilegivel;
    };
    if u64::from(largura) * u64::from(altura) > TETO_DE_PIXELS {
        return PreviaNaJanela::PixelsDemais { largura, altura };
    }
    let Ok(pronta) = para_a_janela(bytes, &TETO_DA_PREVIA) else {
        return PreviaNaJanela::Ilegivel;
    };
    match crate::preview::sniff(&pronta) {
        Some(formato) => PreviaNaJanela::Pronta(crate::preview::data_uri(formato, &pronta)),
        None => PreviaNaJanela::Ilegivel,
    }
}

/// O teto de um uso, pelo nome que a janela manda. `None` para um nome que
/// este produto não conhece: a mídia de MOD sem uso conhecido vai pelo teto
/// geral ([`TETO_DA_MIDIA_DE_MOD`], em `midia_de_mod::reduzir_ao_uso`).
pub fn teto_do_uso(uso: &str) -> Option<Teto> {
    match uso {
        "capa" => Some(TETO_DA_CAPA),
        // O avatar que um MOD põe numa pessoa: o mesmo lugar da foto do perfil
        // oficial, e o mesmo teto. Ia à janela do tamanho que o MOD mandou.
        "avatar" => Some(TETO_DA_FOTO_DO_PERFIL),
        "fundo" => Some(TETO_DO_FUNDO_DE_MOD),
        _ => None,
    }
}

/// **As medidas de uma imagem, lidas do cabeçalho**, sem decodificar um pixel.
///
/// **A imagem grande demais até para o decodificador abrir o cabeçalho** é o
/// caso do navegador: no `wasm32` o endereço tem 32 bits, e um PNG que declara
/// 30 000 × 30 000 não tem tamanho que caiba num `usize` — o decodificador o
/// recusa por limite antes de dizer as medidas (medido em 11/11/2026: no
/// desktop a mesma imagem dizia as medidas, e na página virava «não abriu»).
/// As do PNG saem então do `IHDR`, lidas à mão, para a recusa levar os números
/// nas duas pontas; a dos outros formatos volta como [`GRANDE_DEMAIS`].
///
/// # Errors
///
/// `formato-desconhecido`, quando os bytes não são uma imagem que este
/// produto decodifica, e [`GRANDE_DEMAIS`], quando o decodificador a recusa
/// por limite e as medidas não puderam ser lidas.
pub fn medidas(bytes: &[u8]) -> Result<(u32, u32), String> {
    let formato = image::guess_format(bytes).map_err(|_| desconhecido())?;
    match image::ImageReader::with_format(Cursor::new(bytes), formato).into_dimensions() {
        Ok(medidas) => Ok(medidas),
        Err(image::ImageError::Limits(_)) => {
            medidas_do_ihdr(bytes).ok_or_else(|| GRANDE_DEMAIS.to_owned())
        }
        Err(_) => Err(desconhecido()),
    }
}

/// A largura e a altura que o `IHDR` de um PNG declara: os oito bytes depois
/// da assinatura (8), do tamanho do pedaço (4) e do nome dele (4). `None`
/// para o que não começa como um PNG.
fn medidas_do_ihdr(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let lado = |de: usize| {
        let quatro: [u8; 4] = bytes.get(de..de + 4)?.try_into().ok()?;
        Some(u32::from_be_bytes(quatro))
    };
    Some((lado(16)?, lado(20)?))
}

/// **Uma imagem como ela vai à janela, sem abrir a que já cabe.** As medidas
/// saem do cabeçalho: a que passa de [`TETO_DE_PIXELS`] é recusada, a que cabe
/// no teto sai com **os mesmos bytes e sem ser decodificada** (uma animação
/// longa que cabe no lugar continua inteira), e só a maior que o lugar passa
/// por [`reduzir`].
///
/// É o caminho de quem é pedido a cada montagem (a mídia de um MOD) ou não
/// pode perder quadros (o GIF de uma conversa): [`reduzir`] decodifica sempre,
/// e regrava toda animação.
///
/// # Errors
///
/// `formato-desconhecido` e [`GRANDE_DEMAIS`], como [`reduzir`].
pub fn para_a_janela(bytes: Vec<u8>, teto: &Teto) -> Result<Vec<u8>, String> {
    let (largura, altura) = medidas(&bytes)?;
    if u64::from(largura) * u64::from(altura) > TETO_DE_PIXELS {
        return Err(GRANDE_DEMAIS.to_owned());
    }
    if largura <= teto.largura && altura <= teto.altura {
        return Ok(bytes);
    }
    reduzir(&bytes, teto).map(|reduzida| reduzida.bytes)
}

/// Uma imagem reduzida, e quanto ela pesa decodificada.
#[derive(Debug)]
pub struct Reduzida {
    /// Os bytes que atravessam a ponte. Os de entrada, intactos, quando a
    /// imagem já cabia no teto.
    pub bytes: Vec<u8>,
    /// Largura × altura × 4 × quadros: o que a janela vai decodificar. É o que
    /// o bolso da capa conta (rascunho §4), e não o tamanho do arquivo.
    pub decodificados: u64,
}

/// **Reduz `bytes` ao teto**: as dimensões, cabendo no retângulo do teto com a
/// proporção mantida; numa imagem animada, também os quadros e a duração.
///
/// Uma imagem que já cabe sai com os mesmos bytes. Uma animação reduzida sai em
/// GIF — o único formato animado que este crate grava.
///
/// # Errors
///
/// `formato-desconhecido`, quando os bytes não são uma imagem que este produto
/// decodifica; é o nome que a janela já conhece (rascunho §7.1).
pub fn reduzir(bytes: &[u8], teto: &Teto) -> Result<Reduzida, String> {
    let formato = image::guess_format(bytes).map_err(|_| desconhecido())?;
    // **As medidas, lidas do cabeçalho, antes de decodificar um pixel.** O
    // teto de bytes não limita o que a imagem ocupa aberta: um PNG de poucos
    // megabytes pode declarar 30 000 × 30 000, e decodificá-lo pediria 3,6 GB a
    // este processo. Quem manda a imagem é um par (o perfil de outra pessoa,
    // o MOD de um servidor).
    let (largura, altura) = image::ImageReader::with_format(Cursor::new(bytes), formato)
        .into_dimensions()
        .map_err(|_| desconhecido())?;
    if u64::from(largura) * u64::from(altura) > TETO_DE_PIXELS {
        return Err(GRANDE_DEMAIS.to_owned());
    }
    if let Some(quadros) = quadros_dentro_do_teto(bytes, formato, teto, None)? {
        return reduzir_animacao(quadros, teto);
    }
    let imagem = image::load_from_memory_with_format(bytes, formato).map_err(|_| desconhecido())?;
    let (largura, altura) = (imagem.width(), imagem.height());
    if largura <= teto.largura && altura <= teto.altura {
        return Ok(Reduzida {
            bytes: bytes.to_vec(),
            decodificados: decodificados(largura, altura, 1),
        });
    }
    let (nova_largura, nova_altura) = caber(largura, altura, teto);
    let reduzida = imagem.resize_exact(nova_largura, nova_altura, FilterType::Triangle);
    Ok(Reduzida {
        bytes: gravar_parada(&reduzida, formato)?,
        decodificados: decodificados(nova_largura, nova_altura, 1),
    })
}

/// Grava uma imagem parada no formato em que ela segue. **JPEG fica JPEG**:
/// uma foto regravada em PNG cresce, e o que se queria era o contrário. O
/// resto vira PNG, sem perda.
fn gravar_parada(imagem: &image::DynamicImage, formato: ImageFormat) -> Result<Vec<u8>, String> {
    let mut saida = Vec::new();
    let gravou = if formato == ImageFormat::Jpeg {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut saida, 85)
            .encode_image(&imagem.to_rgb8())
    } else {
        imagem.write_to(&mut Cursor::new(&mut saida), ImageFormat::Png)
    };
    gravou.map_err(|_| desconhecido())?;
    Ok(saida)
}

/// O lado máximo, em pixels, de um ícone de pessoa: o do protocolo
/// (`MAX_SERVER_ICON_SIDE`).
const LADO_DO_ICONE: u32 = seele_proto::control::MAX_SERVER_ICON_SIDE;

/// O teto, em bytes, de um ícone de pessoa: o do protocolo
/// (`MAX_SERVER_ICON_LEN`, 8 KiB). Ele viaja num quadro de controle de 16 KiB e
/// quem hospeda o reenvia a cada pessoa conectada.
const TETO_DO_ICONE: usize = seele_proto::control::MAX_SERVER_ICON_LEN;

/// Os degraus do ícone, do que dói menos ao que dói mais: (lado máximo, bits
/// por canal de cor). Primeiro perde cor no tamanho cheio, e só quando isso não
/// basta é que começa a perder tamanho. O primeiro degrau é o tamanho cheio com
/// as cores todas, então um ícone que já cabia atravessa sem perder nada.
const DEGRAUS_DO_ICONE: [(u32, u8); 8] = [
    (LADO_DO_ICONE, 8),
    (LADO_DO_ICONE, 5),
    (LADO_DO_ICONE, 4),
    (192, 4),
    (160, 4),
    (128, 3),
    (96, 3),
    (64, 2),
];

/// **O ícone nativo de uma pessoa, feito da foto do perfil** (0.19, §9 Q1 da
/// especificação dos perfis): um PNG de até 256 px de lado e 8 KiB, do **primeiro
/// quadro** da imagem.
///
/// Existe para os clientes 0.18, que só conhecem o `SetPersonIcon`: ao salvar
/// uma foto nova, o cliente 0.19 manda também este ícone, e quem ainda está na
/// 0.18 vê o mesmo rosto. O desktop e o navegador chamam esta função, e o
/// `icone::encolher` do `seele-app` é uma chamada a ela — a escada mora num
/// lugar só.
///
/// PNG não tem botão de qualidade: o que o faz encolher é ter menos pixels e
/// menos cores. Um distintivo aguenta perder cor muito antes de aguentar perder
/// tamanho, e é nessa ordem que a escada desce ([`DEGRAUS_DO_ICONE`]). O último
/// degrau (64 px, quatro níveis por canal) dá uns 2 KiB no pior caso que se
/// consegue construir, então `None` é só «não é imagem».
///
/// Um GIF ou WebP animado entra pelo primeiro quadro (`load_from_memory`), e a
/// proporção é mantida: uma foto retangular espremida num quadrado fica com a
/// cara achatada.
///
/// # A paleta vem antes dos degraus (0.19.1)
///
/// Cortar bits por canal fazia de uma foto um cartaz: no ícone de quem
/// hospeda, medido, 128 × 127 em 30 cores, com franjas rosa e verdes num rosto
/// cinza — cada canal arredondava para um degrau diferente. Um PNG indexado,
/// com uma paleta tirada da própria imagem e o erro espalhado
/// (Floyd–Steinberg), guarda a mesma foto em 128 px e 256 cores com 6,9 KiB.
/// Depois do tamanho cheio com as cores todas (que já cabia atravessa sem
/// perder nada), a paleta desce de [`LADOS_DA_PALETA`] por [`CORES_DA_PALETA`];
/// os degraus de [`DEGRAUS_DO_ICONE`] ficam como último recurso, e são os
/// mesmos que o canvas do navegador ainda usa (`REGRAS_DO_ICONE`).
#[must_use]
pub fn icone_do_perfil(escolhida: &[u8]) -> Option<Vec<u8>> {
    let imagem = image::load_from_memory(escolhida).ok()?;
    let cheio = png_rgba(&reduzido(&imagem, LADO_DO_ICONE))?;
    if cheio.len() <= TETO_DO_ICONE {
        return Some(cheio);
    }
    // Cor antes de tamanho: um avatar aparece com 30 a 90 px, e 128 px em 256
    // cores é uma foto, enquanto 160 px em 16 cores é um cartaz. As paletas
    // ricas passam por todos os lados antes de as pobres serem tentadas.
    for paletas in [&CORES_DA_PALETA[..3], &CORES_DA_PALETA[3..]] {
        for lado in LADOS_DA_PALETA {
            let quadro = reduzido(&imagem, lado);
            for &cores in paletas {
                if let Some(saida) = png_em_paleta(&quadro, cores) {
                    if saida.len() <= TETO_DO_ICONE {
                        return Some(saida);
                    }
                }
            }
        }
    }
    for (lado, bits) in DEGRAUS_DO_ICONE.into_iter().skip(1) {
        let mut quadro = reduzido(&imagem, lado);
        if bits < 8 {
            achatar_cores(&mut quadro, bits);
        }
        let saida = png_rgba(&quadro)?;
        if saida.len() <= TETO_DO_ICONE {
            return Some(saida);
        }
    }
    None
}

/// Os lados que a paleta tenta, do maior ao menor.
const LADOS_DA_PALETA: [u32; 6] = [LADO_DO_ICONE, 224, 192, 160, 128, 96];

/// As paletas que cada lado tenta, da mais rica à mais pobre.
const CORES_DA_PALETA: [usize; 5] = [256, 128, 64, 32, 16];

/// A imagem com o lado maior em `lado`, se passar dele, e a proporção mantida.
fn reduzido(imagem: &image::DynamicImage, lado: u32) -> image::RgbaImage {
    if imagem.width() > lado || imagem.height() > lado {
        // `Lanczos3` e não `Nearest`: reduzir foto sem filtro é o que
        // produz aquele serrilhado que parece defeito da tela.
        imagem.resize(lado, lado, FilterType::Lanczos3).to_rgba8()
    } else {
        imagem.to_rgba8()
    }
}

/// Um PNG RGBA, com a compressão mais forte.
fn png_rgba(quadro: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut saida = Vec::new();
    let escreveu = image::codecs::png::PngEncoder::new_with_quality(
        &mut saida,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    );
    image::ImageEncoder::write_image(
        escreveu,
        quadro.as_raw(),
        quadro.width(),
        quadro.height(),
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    Some(saida)
}

/// Um PNG indexado em `cores` cores, com a paleta tirada da própria imagem
/// (NeuQuant) e o erro espalhado (Floyd–Steinberg). O alfa vai na paleta
/// (`tRNS`), então uma borda suave continua suave.
fn png_em_paleta(quadro: &image::RgbaImage, cores: usize) -> Option<Vec<u8>> {
    let mapa = color_quant::NeuQuant::new(10, cores, quadro.as_raw());
    let mut espalhado = quadro.clone();
    // O `dither` do `image` olha o vizinho da direita e o de baixo: numa
    // imagem de um pixel de largura ou de altura, ele sai do quadro.
    if espalhado.width() > 1 && espalhado.height() > 1 {
        image::imageops::dither(&mut espalhado, &mapa);
    }
    let indices = image::imageops::index_colors(&espalhado, &mapa);
    let rgba = mapa.color_map_rgba();
    let paleta: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|cor| cor.iter().take(3).copied())
        .collect();
    let alfa: Vec<u8> = rgba
        .chunks_exact(4)
        .filter_map(|cor| cor.get(3).copied())
        .collect();
    // Uma foto sem transparência não leva a tabela do alfa: a paleta do
    // NeuQuant arredonda o 255 de algumas cores para 254, e a tabela custava
    // uns 256 bytes a troco de nada.
    let opaca = quadro.pixels().all(|pixel| pixel.0[3] == u8::MAX);

    // Sem filtro costuma ser o melhor para um PNG indexado, e nem sempre é:
    // os dois são tentados, e fica o menor.
    let mut menor: Option<Vec<u8>> = None;
    for filtro in [png::FilterType::NoFilter, png::FilterType::Paeth] {
        let mut saida = Vec::new();
        {
            let mut gravador = png::Encoder::new(&mut saida, quadro.width(), quadro.height());
            gravador.set_color(png::ColorType::Indexed);
            gravador.set_depth(png::BitDepth::Eight);
            gravador.set_palette(paleta.clone());
            if !opaca {
                gravador.set_trns(alfa.clone());
            }
            gravador.set_compression(png::Compression::Best);
            gravador.set_filter(filtro);
            let mut escrita = gravador.write_header().ok()?;
            escrita.write_image_data(indices.as_raw()).ok()?;
        }
        if menor.as_ref().is_none_or(|atual| saida.len() < atual.len()) {
            menor = Some(saida);
        }
    }
    menor
}

/// Reduz cada canal a `bits` níveis, mantendo preto preto e branco branco: a
/// conta ingênua (apagar os bits de baixo) escurece a imagem inteira e nunca
/// chega ao branco. O alfa não é achatado: um distintivo com borda suave vira
/// um recorte de tesoura se for, e o alfa custa pouco no PNG.
fn achatar_cores(quadro: &mut image::RgbaImage, bits: u8) {
    let niveis = (1_u32 << bits).saturating_sub(1).max(1);
    #[allow(clippy::cast_precision_loss, reason = "no máximo 255")]
    let passo = 255.0 / niveis as f32;
    for pixel in quadro.pixels_mut() {
        for canal in 0..3 {
            let Some(valor) = pixel.0.get_mut(canal) else {
                continue;
            };
            let degrau = (f32::from(*valor) / passo).round();
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "limitado a 0..=255 na linha"
            )]
            {
                *valor = (degrau * passo).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// O nome com que uma imagem ilegível volta à janela.
/// O maior número de pixels (largura × altura) que este produto decodifica:
/// cabe uma imagem em 8K (33 megapixels), e a decodificação de uma só não
/// passa de 144 MB.
pub const TETO_DE_PIXELS: u64 = 36_000_000;

/// O nome da recusa de uma imagem acima de [`TETO_DE_PIXELS`]. É um nome que
/// a janela já conhece (o da mídia grande demais).
pub const GRANDE_DEMAIS: &str = "midia-grande-demais";

fn desconhecido() -> String {
    "formato-desconhecido".to_owned()
}

/// Largura × altura × 4 × quadros, sem estourar.
fn decodificados(largura: u32, altura: u32, quadros: usize) -> u64 {
    u64::from(largura)
        .saturating_mul(u64::from(altura))
        .saturating_mul(4)
        .saturating_mul(u64::try_from(quadros).unwrap_or(u64::MAX))
}

/// O maior tamanho com a proporção de `largura × altura` que cabe no teto.
fn caber(largura: u32, altura: u32, teto: &Teto) -> (u32, u32) {
    let escala = (f64::from(teto.largura) / f64::from(largura.max(1)))
        .min(f64::from(teto.altura) / f64::from(altura.max(1)));
    let lado = |medida: u32, maximo: u32| {
        // O arredondamento de um `f64` positivo e limitado ao teto cabe num `u32`.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "limitado logo abaixo"
        )]
        let medido = (f64::from(medida) * escala).round().max(1.0) as u32;
        medido.min(maximo)
    };
    (lado(largura, teto.largura), lado(altura, teto.altura))
}

/// **Os quadros de uma animação, lidos só até o teto** de quadros e de duração
/// — o resto do arquivo nem é decodificado. `None` para o que não é animação
/// (ou é uma de um quadro só), que segue o caminho da imagem parada.
fn quadros_dentro_do_teto(
    bytes: &[u8],
    formato: ImageFormat,
    teto: &Teto,
    recorte: Option<Recorte>,
) -> Result<Option<Vec<Frame>>, String> {
    let quadros = match formato {
        ImageFormat::Gif => GifDecoder::new(Cursor::new(bytes))
            .map_err(|_| desconhecido())?
            .into_frames(),
        ImageFormat::WebP => {
            let decodificador = WebPDecoder::new(Cursor::new(bytes)).map_err(|_| desconhecido())?;
            if !decodificador.has_animation() {
                return Ok(None);
            }
            decodificador.into_frames()
        }
        _ => return Ok(None),
    };
    let mut mantidos = Vec::new();
    let mut duracao: u32 = 0;
    for quadro in quadros {
        let quadro = quadro.map_err(|_| desconhecido())?;
        let (numer, denom) = quadro.delay().numer_denom_ms();
        let deste = numer / denom.max(1);
        // O primeiro fica sempre: uma animação de um quadro longo é uma imagem.
        if !mantidos.is_empty()
            && (mantidos.len() >= teto.quadros
                || duracao.saturating_add(deste) > teto.milissegundos)
        {
            break;
        }
        duracao = duracao.saturating_add(deste);
        // **Recortado ao chegar, antes de reduzir**: o pedaço escolhido de cada
        // quadro, que é o que mantém a animação animada depois do recorte.
        let quadro = match recorte {
            Some(pedaco) => {
                let atraso = quadro.delay();
                let cortado = image::imageops::crop_imm(
                    quadro.buffer(),
                    pedaco.x,
                    pedaco.y,
                    pedaco.largura,
                    pedaco.altura,
                )
                .to_image();
                Frame::from_parts(cortado, 0, 0, atraso)
            }
            None => quadro,
        };
        // **Reduzido ao chegar, e não depois de juntar todos**: sessenta
        // quadros de uma animação em 4K, guardados inteiros, são 2 GB. Assim a
        // memória segura um quadro grande por vez.
        let (largura, altura) = quadro.buffer().dimensions();
        let quadro = if largura > teto.largura || altura > teto.altura {
            let (nova_largura, nova_altura) = caber(largura, altura, teto);
            let atraso = quadro.delay();
            let menor = image::imageops::resize(
                quadro.buffer(),
                nova_largura,
                nova_altura,
                FilterType::Triangle,
            );
            Frame::from_parts(menor, 0, 0, atraso)
        } else {
            quadro
        };
        mantidos.push(quadro);
    }
    Ok((mantidos.len() > 1).then_some(mantidos))
}

/// Grava os quadros mantidos em GIF, reduzidos quadro a quadro quando a
/// animação é maior que o lugar.
fn reduzir_animacao(quadros: Vec<Frame>, teto: &Teto) -> Result<Reduzida, String> {
    let quantos = quadros.len();
    let (largura, altura) = quadros
        .first()
        .map(|quadro| quadro.buffer().dimensions())
        .ok_or_else(desconhecido)?;
    let (nova_largura, nova_altura) = if largura <= teto.largura && altura <= teto.altura {
        (largura, altura)
    } else {
        caber(largura, altura, teto)
    };
    let mut saida = Vec::new();
    {
        // A velocidade 10 da quantização é o meio da escala do crate: a paleta
        // de uma capa não precisa da melhor, e 60 quadros na mais lenta somam
        // segundos.
        let mut gravador = GifEncoder::new_with_speed(&mut saida, 10);
        gravador
            .set_repeat(Repeat::Infinite)
            .map_err(|_| desconhecido())?;
        for quadro in quadros {
            let atraso = quadro.delay();
            let mut imagem = quadro.into_buffer();
            if (nova_largura, nova_altura) != (largura, altura) {
                imagem = image::imageops::resize(
                    &imagem,
                    nova_largura,
                    nova_altura,
                    FilterType::Triangle,
                );
            }
            gravador
                .encode_frame(Frame::from_parts(imagem, 0, 0, atraso))
                .map_err(|_| desconhecido())?;
        }
    }
    Ok(Reduzida {
        bytes: saida,
        decodificados: decodificados(nova_largura, nova_altura, quantos),
    })
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, ImageEncoder, RgbaImage};

    fn png(largura: u32, altura: u32) -> Vec<u8> {
        let imagem = RgbaImage::from_pixel(largura, altura, image::Rgba([40, 200, 120, 255]));
        let mut saida = Vec::new();
        image::codecs::png::PngEncoder::new(&mut saida)
            .write_image(&imagem, largura, altura, image::ExtendedColorType::Rgba8)
            .expect("gravar o PNG de teste");
        saida
    }

    fn gif(largura: u32, altura: u32, quadros: usize, milissegundos: u32) -> Vec<u8> {
        let mut saida = Vec::new();
        {
            let mut gravador = GifEncoder::new(&mut saida);
            gravador.set_repeat(Repeat::Infinite).expect("repetir");
            for i in 0..quadros {
                let cor = u8::try_from(i % 250).expect("cabe");
                let imagem = RgbaImage::from_pixel(largura, altura, image::Rgba([cor, 0, 0, 255]));
                let quadro =
                    Frame::from_parts(imagem, 0, 0, Delay::from_numer_denom_ms(milissegundos, 1));
                gravador.encode_frame(quadro).expect("gravar o quadro");
            }
        }
        saida
    }

    fn dimensoes(bytes: &[u8]) -> (u32, u32) {
        let imagem = image::load_from_memory(bytes).expect("decodificar a reduzida");
        (imagem.width(), imagem.height())
    }

    fn quadros_e_duracao(bytes: &[u8]) -> (usize, u32) {
        let quadros = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .expect("um GIF")
            .into_frames()
            .collect_frames()
            .expect("os quadros");
        let duracao = quadros
            .iter()
            .map(|quadro| {
                let (numer, denom) = quadro.delay().numer_denom_ms();
                numer / denom.max(1)
            })
            .sum();
        (quadros.len(), duracao)
    }

    /// Uma foto maior que o lugar cabe no retângulo, com a proporção mantida.
    #[test]
    fn uma_imagem_maior_que_o_lugar_cabe_nele_com_a_proporcao() {
        let reduzida = reduzir(&png(2000, 700), &TETO_DA_CAPA).expect("reduzir");
        let (largura, altura) = dimensoes(&reduzida.bytes);
        assert!(largura <= 960 && altura <= 320, "saiu {largura} × {altura}");
        assert!(
            altura == 320 || largura == 960,
            "não usou o lado que limita: {largura} × {altura}"
        );
        let proporcao = f64::from(largura) / f64::from(altura);
        assert!(
            (proporcao - 2000.0 / 700.0).abs() < 0.02,
            "a proporção mudou: {proporcao}"
        );
        assert_eq!(
            reduzida.decodificados,
            u64::from(largura) * u64::from(altura) * 4
        );
    }

    /// A que já cabe sai com os mesmos bytes.
    #[test]
    fn uma_imagem_que_ja_cabe_sai_intacta() {
        let original = png(300, 100);
        let reduzida = reduzir(&original, &TETO_DA_CAPA).expect("reduzir");
        assert_eq!(reduzida.bytes, original);
        assert_eq!(reduzida.decodificados, 300 * 100 * 4);
    }

    /// Uma animação longa perde os quadros que passam do teto de quadros.
    #[test]
    fn uma_animacao_com_quadros_demais_fica_no_teto_de_quadros() {
        let reduzida = reduzir(&gif(40, 20, 120, 50), &TETO_DA_CAPA).expect("reduzir");
        let (quadros, duracao) = quadros_e_duracao(&reduzida.bytes);
        assert_eq!(quadros, 60, "saiu com {quadros} quadros");
        assert!(duracao <= 10_000, "dura {duracao} ms");
        assert_eq!(reduzida.decodificados, 40 * 20 * 4 * 60);
    }

    /// E uma animação lenta perde os quadros que passam do teto de duração.
    #[test]
    fn uma_animacao_lenta_demais_fica_no_teto_de_duracao() {
        let reduzida = reduzir(&gif(40, 20, 40, 500), &TETO_DA_CAPA).expect("reduzir");
        let (quadros, duracao) = quadros_e_duracao(&reduzida.bytes);
        assert_eq!(quadros, 20, "saiu com {quadros} quadros e {duracao} ms");
        assert!(duracao <= 10_000, "dura {duracao} ms");
    }

    /// Uma animação maior que o lugar é reduzida quadro a quadro.
    #[test]
    fn uma_animacao_maior_que_o_lugar_e_reduzida_em_cada_quadro() {
        let reduzida = reduzir(&gif(1920, 640, 3, 100), &TETO_DA_CAPA).expect("reduzir");
        let (largura, altura) = dimensoes(&reduzida.bytes);
        assert!(largura <= 960 && altura <= 320, "saiu {largura} × {altura}");
        assert_eq!(quadros_e_duracao(&reduzida.bytes).0, 3);
    }

    /// Bytes que não são imagem são recusados pelo nome que a janela conhece.
    #[test]
    fn bytes_que_nao_sao_imagem_sao_formato_desconhecido() {
        assert_eq!(
            reduzir(b"nada disto", &TETO_DA_CAPA).unwrap_err(),
            "formato-desconhecido"
        );
    }

    /// **A imagem que cabe vai à janela sem ser aberta, a maior é reduzida, e
    /// a que declara pixels demais é recusada** ([`para_a_janela`]: a mídia de
    /// MOD sem uso, e a prévia de um anexo).
    #[test]
    fn para_a_janela_reduz_so_o_que_nao_cabe() {
        let teto = TETO_DA_MIDIA_DE_MOD;
        // 4K vira 1080p, com a proporção mantida.
        let reduzida = para_a_janela(png(3840, 2160), &teto).expect("reduz");
        assert_eq!(dimensoes(&reduzida), (1920, 1080));
        // O que cabe sai com os mesmos bytes.
        let pequena = png(1920, 1080);
        assert_eq!(
            para_a_janela(pequena.clone(), &teto).expect("passa"),
            pequena
        );
        // **Sem decodificar**: uma animação longa que cabe no lugar continua
        // com todos os quadros (`reduzir` a cortaria em 60).
        let longa = gif(40, 20, 120, 50);
        assert_eq!(para_a_janela(longa.clone(), &teto).expect("passa"), longa);
        // A animação maior que o lugar é reduzida quadro a quadro.
        let animada = para_a_janela(gif(2560, 1440, 3, 100), &teto).expect("reduz");
        assert_eq!(dimensoes(&animada), (1920, 1080));
        assert_eq!(quadros_e_duracao(&animada).0, 3);
        // 45 megapixels num arquivo pequeno: recusada pelo cabeçalho.
        assert_eq!(
            para_a_janela(png(9000, 5000), &teto).unwrap_err(),
            GRANDE_DEMAIS
        );
        assert_eq!(
            para_a_janela(b"nada disto".to_vec(), &teto).unwrap_err(),
            "formato-desconhecido"
        );
        assert_eq!(medidas(&png(300, 100)), Ok((300, 100)));
    }

    /// **A prévia de uma imagem da conversa vai à janela dentro do teto dela**
    /// (a varredura do incidente da capa em 4K): a grande é reduzida, a que
    /// cabe passa com os bytes que tinha, o GIF que cabe fica com todos os
    /// quadros, e a que declara pixels demais é recusada com as medidas.
    #[test]
    fn a_previa_vai_a_janela_dentro_do_teto() {
        let bytes_de = |previa: PreviaNaJanela| {
            let PreviaNaJanela::Pronta(uri) = previa else {
                panic!("a prévia não ficou pronta: {previa:?}");
            };
            let (cabecalho, conteudo) = uri.split_once(',').expect("um data:");
            (
                cabecalho.to_owned(),
                crate::base64::de_base64(conteudo).expect("base64"),
            )
        };
        // Uma foto em 4K: reduzida, e ainda PNG.
        let (tipo, reduzida) = bytes_de(previa_na_janela(png(3840, 2160)));
        assert_eq!(tipo, "data:image/png;base64");
        assert_eq!(dimensoes(&reduzida), (1280, 720));
        // Uma captura comprida cabe pela altura, com a proporção mantida.
        let (_, comprida) = bytes_de(previa_na_janela(png(1080, 4320)));
        assert_eq!(dimensoes(&comprida), (180, 720));
        // O que cabe passa com os mesmos bytes.
        let pequena = png(640, 360);
        assert_eq!(bytes_de(previa_na_janela(pequena.clone())).1, pequena);
        // O GIF que cabe no lugar não perde quadro nenhum.
        let longo = gif(64, 48, 120, 50);
        let (tipo, passou) = bytes_de(previa_na_janela(longo.clone()));
        assert_eq!(tipo, "data:image/gif;base64");
        assert_eq!(passou, longo);
        // 45 megapixels num arquivo pequeno: recusada com as medidas.
        assert_eq!(
            previa_na_janela(png(9000, 5000)),
            PreviaNaJanela::PixelsDemais {
                largura: 9000,
                altura: 5000
            }
        );
        // Só a assinatura: não há medidas para ler.
        assert_eq!(
            previa_na_janela(b"\x89PNG\r\n\x1a\n\0\0\0\x0d".to_vec()),
            PreviaNaJanela::Ilegivel
        );
    }

    /// As medidas do `IHDR`, lidas à mão: o que a recusa por pixels usa
    /// quando o decodificador nem abre o cabeçalho (o `wasm32`).
    #[test]
    fn as_medidas_do_ihdr_sao_as_que_o_png_declara() {
        assert_eq!(medidas_do_ihdr(&png(300, 100)), Some((300, 100)));
        assert_eq!(medidas_do_ihdr(&gif(40, 20, 2, 50)), None);
        assert_eq!(
            medidas_do_ihdr(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR\0\0"),
            None
        );
    }

    /// O fundo de uma caixa de MOD tem o teto da capa de um perfil.
    #[test]
    fn o_fundo_de_mod_tem_o_teto_da_capa_do_perfil() {
        assert_eq!(teto_do_uso("fundo"), Some(TETO_DA_CAPA_DO_PERFIL));
    }

    /// O uso é um nome de lista fechada: um nome desconhecido não tem teto
    /// próprio (a mídia de MOD vai então pelo teto geral).
    #[test]
    fn so_um_uso_conhecido_tem_teto() {
        assert_eq!(teto_do_uso("capa"), Some(TETO_DA_CAPA));
        assert_eq!(teto_do_uso("qualquer coisa"), None);
    }

    /// Ruído: o pior caso do PNG, que não tem forma nem cor repetida.
    fn ruido(lado: u32) -> Vec<u8> {
        let mut quadro = RgbaImage::new(lado, lado);
        let mut semente = 0x2545_F491_4F6C_DD1D_u64;
        for pixel in quadro.pixels_mut() {
            semente ^= semente << 13;
            semente ^= semente >> 7;
            semente ^= semente << 17;
            let b = semente.to_le_bytes();
            pixel.0 = [b[0], b[1], b[2], 255];
        }
        let mut saida = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(quadro)
            .write_to(&mut saida, ImageFormat::Png)
            .expect("gravar o ruído");
        saida.into_inner()
    }

    /// **O ícone do perfil cabe no que o protocolo aceita, sempre**: PNG de até
    /// 256 px e 8 KiB, mesmo para o pior caso, e `check_server_icon` (o portão
    /// do servidor) o aceita.
    #[test]
    fn o_icone_do_perfil_cabe_no_que_o_protocolo_aceita() {
        let pronto = icone_do_perfil(&ruido(1024)).expect("o ruído tinha de caber");
        assert!(
            pronto.len() <= TETO_DO_ICONE,
            "saiu com {} bytes",
            pronto.len()
        );
        let (largura, altura) = dimensoes(&pronto);
        assert!(largura <= LADO_DO_ICONE && altura <= LADO_DO_ICONE);
        assert_eq!(
            seele_proto::control::check_server_icon(Some(&pronto)),
            Ok(()),
            "o servidor recusaria o ícone que o cliente gerou"
        );
    }

    /// Um GIF animado entra pelo **primeiro quadro**, e a proporção fica.
    #[test]
    fn o_icone_do_perfil_de_um_gif_e_o_primeiro_quadro() {
        // Dois quadros, 300 × 150: o primeiro verde e o segundo vermelho.
        let mut animado = Vec::new();
        {
            let mut gravador = GifEncoder::new(&mut animado);
            gravador.set_repeat(Repeat::Infinite).expect("repetir");
            for cor in [[0_u8, 200, 0, 255], [200, 0, 0, 255]] {
                let imagem = RgbaImage::from_pixel(300, 150, image::Rgba(cor));
                let quadro = Frame::from_parts(imagem, 0, 0, Delay::from_numer_denom_ms(100, 1));
                gravador.encode_frame(quadro).expect("gravar o quadro");
            }
        }
        let pronto = icone_do_perfil(&animado).expect("um GIF é imagem");
        let imagem = image::load_from_memory(&pronto).expect("é PNG").to_rgba8();
        assert_eq!(imagem.dimensions(), (256, 128), "a proporção mudou");
        let pixel = imagem.get_pixel(10, 10).0;
        assert!(
            pixel[0] < 40 && pixel[1] > 150,
            "não é o primeiro quadro (o verde): {pixel:?}"
        );
    }

    #[test]
    fn o_que_nao_e_imagem_nao_vira_icone() {
        assert!(icone_do_perfil(b"isto nao e uma imagem").is_none());
        assert!(icone_do_perfil(&[]).is_none());
    }

    /// Uma «foto» em tons de cinza com o ruído de cor de uma WebP com perda:
    /// formas suaves, textura fina e cada canal um pouco fora do outro.
    fn foto_cinza(lado: u32) -> Vec<u8> {
        let mut quadro = RgbaImage::new(lado, lado);
        let mut semente = 0x9E37_79B9_7F4A_7C15_u64;
        for (x, y, pixel) in quadro.enumerate_pixels_mut() {
            semente ^= semente << 13;
            semente ^= semente >> 7;
            semente ^= semente << 17;
            let b = semente.to_le_bytes();
            let (fx, fy) = (f64::from(x), f64::from(y));
            let forma = 128.0 + 70.0 * (fx / 37.0).sin() * (fy / 53.0).cos();
            let textura = f64::from(b[0] % 40) - 20.0;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "limitado"
            )]
            let cinza = (forma + textura).clamp(0.0, 255.0) as i16;
            let canal = |desvio: u8| {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "limitado"
                )]
                let valor = (cinza + i16::from(desvio % 7) - 3).clamp(0, 255) as u8;
                valor
            };
            pixel.0 = [canal(b[1]), canal(b[2]), canal(b[3]), 255];
        }
        let mut saida = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(quadro)
            .write_to(&mut saida, ImageFormat::Png)
            .expect("gravar a foto");
        saida.into_inner()
    }

    /// **O ícone de uma foto cinza continua cinza, e com tons** (0.19.1).
    ///
    /// A escada cortava bits por canal: num cinza com ruído de cor, cada canal
    /// arredondava para um degrau diferente, e o ícone saía com franjas rosa e
    /// verdes em 30 cores — medido no ícone de quem hospeda, 128 × 127. Com
    /// uma paleta, o mesmo teto de 8 KiB guarda o cinza como cinza.
    #[test]
    fn o_icone_de_uma_foto_cinza_nao_ganha_cor_e_guarda_os_tons() {
        let pronto = icone_do_perfil(&foto_cinza(873)).expect("a foto tinha de caber");
        assert!(
            pronto.len() <= TETO_DO_ICONE,
            "saiu com {} bytes",
            pronto.len()
        );
        assert_eq!(
            seele_proto::control::check_server_icon(Some(&pronto)),
            Ok(())
        );
        let imagem = image::load_from_memory(&pronto).expect("é PNG").to_rgba8();
        let mut tons = std::collections::BTreeSet::new();
        let mut pior_cor = 0_u8;
        for pixel in imagem.pixels() {
            let [r, g, b, _] = pixel.0;
            pior_cor = pior_cor.max(r.max(g).max(b) - r.min(g).min(b));
            tons.insert(g);
        }
        assert!(
            pior_cor <= 16,
            "o cinza ganhou cor: canais até {pior_cor} apart"
        );
        assert!(tons.len() >= 24, "só {} tons de cinza", tons.len());
        assert!(
            imagem.width() >= 96,
            "o ícone encolheu demais: {}px",
            imagem.width()
        );
    }

    /// Uma imagem de um pixel de largura não derruba a redução (o `dither`
    /// do `image` olha o vizinho da direita).
    #[test]
    fn uma_imagem_de_um_pixel_de_largura_vira_icone() {
        let mut saida = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            1,
            300,
            image::Rgba([10, 20, 30, 255]),
        ))
        .write_to(&mut saida, ImageFormat::Png)
        .expect("gravar");
        let pronto = icone_do_perfil(saida.get_ref()).expect("é imagem");
        assert!(pronto.len() <= TETO_DO_ICONE);
    }

    /// **Uma capa em 4K vai à janela com um quarto dos pixels, e uma foto
    /// grande vira 512 px** (o relato de 10/11/2026: a capa em 4K deixou o
    /// aplicativo inteiro lento). O que já cabe sai com os mesmos bytes.
    #[test]
    fn a_imagem_do_perfil_vai_a_janela_dentro_do_teto() {
        let medidas = |bytes: &[u8]| {
            let imagem = image::load_from_memory(bytes).expect("decodifica");
            (imagem.width(), imagem.height())
        };
        let capa_4k = png(3840, 2160);
        let reduzida = imagem_do_perfil_para_a_janela(&capa_4k, true).expect("reduz");
        assert_eq!(medidas(&reduzida), (1280, 720));

        // Uma faixa larga cabe pela largura, com a proporção mantida.
        let faixa = imagem_do_perfil_para_a_janela(&png(3840, 1280), true).expect("reduz");
        let (largura, altura) = medidas(&faixa);
        assert_eq!(largura, 1280);
        assert!((426..=427).contains(&altura), "{altura}");

        let foto = imagem_do_perfil_para_a_janela(&png(3000, 3000), false).expect("reduz");
        assert_eq!(medidas(&foto), (512, 512));

        // O que já cabe não é regravado.
        let pequena = png(1024, 576);
        assert_eq!(
            imagem_do_perfil_para_a_janela(&pequena, true).expect("passa"),
            pequena
        );
        // E o que não é imagem é dito, e não entregue.
        assert!(imagem_do_perfil_para_a_janela(b"nada disto", true).is_err());
    }

    /// Uma capa animada grande também é reduzida: é o caso mais caro (cada
    /// quadro é uma imagem inteira decodificada).
    #[test]
    fn a_capa_animada_do_perfil_tambem_cabe_no_teto() {
        let animada = gif(2560, 1440, 4, 100);
        let reduzida = imagem_do_perfil_para_a_janela(&animada, true).expect("reduz");
        let decodificador = GifDecoder::new(Cursor::new(&reduzida)).expect("continua um GIF");
        let quadros = decodificador
            .into_frames()
            .collect_frames()
            .expect("quadros");
        assert_eq!(quadros.len(), 4);
        let primeiro = quadros[0].buffer();
        assert_eq!((primeiro.width(), primeiro.height()), (1280, 720));
    }

    /// **Uma imagem que declara mais pixels que o teto é recusada pelo
    /// cabeçalho, sem ser decodificada.** O arquivo é pequeno (uma cor só
    /// comprime a quase nada) e aberto pediria gigabytes.
    #[test]
    fn a_imagem_com_pixels_demais_e_recusada_antes_de_decodificar() {
        // 9000 × 5000 = 45 megapixels, acima do teto; 8K (7680 × 4320) passa.
        let bomba = png(9000, 5000);
        assert!(bomba.len() < 1024 * 1024, "o arquivo de teste é pequeno");
        assert_eq!(reduzir(&bomba, &TETO_DA_CAPA).unwrap_err(), GRANDE_DEMAIS,);
        assert_eq!(
            imagem_do_perfil_para_a_janela(&bomba, true).unwrap_err(),
            GRANDE_DEMAIS,
        );
        assert!(reduzir(&png(7680, 4320), &TETO_DA_CAPA).is_ok());
    }

    // ------------------------------------------------------------ o recorte

    const VERMELHO: [u8; 4] = [220, 30, 30, 255];
    const VERDE: [u8; 4] = [30, 200, 60, 255];
    const AZUL: [u8; 4] = [30, 60, 220, 255];
    const AMARELO: [u8; 4] = [230, 220, 40, 255];

    /// Quatro quadrantes de cores diferentes: vermelho em cima à esquerda,
    /// verde em cima à direita, azul embaixo à esquerda e amarelo embaixo à
    /// direita. Pelo que sobra no recorte se sabe que pedaço saiu.
    fn quadrantes(largura: u32, altura: u32) -> RgbaImage {
        RgbaImage::from_fn(largura, altura, |x, y| {
            image::Rgba(match (x < largura / 2, y < altura / 2) {
                (true, true) => VERMELHO,
                (false, true) => VERDE,
                (true, false) => AZUL,
                (false, false) => AMARELO,
            })
        })
    }

    fn png_de(imagem: &RgbaImage) -> Vec<u8> {
        let mut saida = Vec::new();
        image::codecs::png::PngEncoder::new(&mut saida)
            .write_image(
                imagem,
                imagem.width(),
                imagem.height(),
                image::ExtendedColorType::Rgba8,
            )
            .expect("gravar o PNG de teste");
        saida
    }

    fn jpeg_de(imagem: &RgbaImage) -> Vec<u8> {
        let mut saida = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut saida, 95)
            .encode_image(&image::DynamicImage::ImageRgba8(imagem.clone()).to_rgb8())
            .expect("gravar o JPEG de teste");
        saida
    }

    /// A cor de um pixel, com a folga de um JPEG.
    fn perto(pixel: image::Rgba<u8>, cor: [u8; 4]) -> bool {
        pixel
            .0
            .iter()
            .zip(cor)
            .take(3)
            .all(|(a, b)| a.abs_diff(b) <= 24)
    }

    fn pedaco(x: u32, y: u32, largura: u32, altura: u32) -> Recorte {
        Recorte {
            x,
            y,
            largura,
            altura,
        }
    }

    /// **O que sai é o pedaço escolhido**, e não o centro nem a imagem
    /// encolhida: cada quadrante pedido volta com a cor dele, inteiro.
    #[test]
    fn o_recorte_tira_o_pedaco_escolhido() {
        let original = png_de(&quadrantes(400, 300));
        for (recorte, cor) in [
            (pedaco(0, 0, 150, 150), VERMELHO),
            (pedaco(200, 0, 150, 150), VERDE),
            (pedaco(0, 150, 150, 150), AZUL),
            (pedaco(250, 150, 150, 150), AMARELO),
        ] {
            let saida = imagem_do_perfil_recortada(&original, false, recorte).expect("recorta");
            let imagem = image::load_from_memory(&saida).expect("abre").to_rgba8();
            assert_eq!(imagem.dimensions(), (150, 150), "{recorte:?}");
            assert!(
                imagem.pixels().all(|pixel| pixel.0 == cor),
                "{recorte:?} trouxe um pixel de outro quadrante"
            );
        }
        // O pedaço que pega os quatro traz os quatro, cada um no canto dele.
        let meio = imagem_do_perfil_recortada(&original, false, pedaco(150, 100, 100, 100))
            .expect("recorta");
        let meio = image::load_from_memory(&meio).expect("abre").to_rgba8();
        assert_eq!(meio.get_pixel(0, 0).0, VERMELHO);
        assert_eq!(meio.get_pixel(99, 0).0, VERDE);
        assert_eq!(meio.get_pixel(0, 99).0, AZUL);
        assert_eq!(meio.get_pixel(99, 99).0, AMARELO);
    }

    /// **O recorte segue pelo teto de sempre**: a foto sai em até 512 e a
    /// capa em até 1280 × 720, com a proporção do pedaço.
    #[test]
    fn o_recorte_sai_dentro_do_teto_de_sempre() {
        let grande = png_de(&quadrantes(4000, 3000));
        // O quadrante de baixo à direita, quadrado: 512 × 512, amarelo.
        let foto = imagem_do_perfil_recortada(&grande, false, pedaco(2200, 1600, 1400, 1400))
            .expect("recorta");
        let foto = image::load_from_memory(&foto).expect("abre").to_rgba8();
        assert_eq!(foto.dimensions(), (512, 512));
        assert!(foto.pixels().all(|pixel| pixel.0 == AMARELO));
        // Um 16:9 maior que o teto da capa: 1280 × 720.
        let capa = imagem_do_perfil_recortada(&grande, true, pedaco(400, 300, 3200, 1800))
            .expect("recorta");
        let capa = image::load_from_memory(&capa).expect("abre").to_rgba8();
        assert_eq!(capa.dimensions(), (1280, 720));
        assert_eq!(capa.get_pixel(4, 4).0, VERMELHO);
        assert_eq!(capa.get_pixel(1275, 715).0, AMARELO);
        // O pedaço menor que o teto não é ampliado.
        let pequena =
            imagem_do_perfil_recortada(&grande, true, pedaco(0, 0, 640, 360)).expect("recorta");
        assert_eq!(dimensoes(&pequena), (640, 360));
    }

    /// **JPEG fica JPEG** depois do recorte, como depois da redução: uma foto
    /// regravada em PNG cresce.
    #[test]
    fn o_recorte_de_um_jpeg_continua_jpeg() {
        let foto = jpeg_de(&quadrantes(800, 600));
        let saida =
            imagem_do_perfil_recortada(&foto, false, pedaco(420, 320, 200, 200)).expect("recorta");
        assert_eq!(
            image::guess_format(&saida).expect("é imagem"),
            ImageFormat::Jpeg
        );
        let imagem = image::load_from_memory(&saida).expect("abre").to_rgba8();
        assert_eq!(imagem.dimensions(), (200, 200));
        assert!(imagem.pixels().all(|pixel| perto(*pixel, AMARELO)));
    }

    /// **Um GIF animado continua animado depois do recorte**: o pedaço é
    /// tirado de cada quadro, com o atraso de cada um. Um canvas o congelaria
    /// no primeiro.
    #[test]
    fn o_recorte_de_uma_animacao_continua_animado() {
        let mut animada = Vec::new();
        {
            let mut gravador = GifEncoder::new(&mut animada);
            gravador.set_repeat(Repeat::Infinite).expect("repetir");
            for i in 0..5_u8 {
                // O quadrante de baixo à direita muda a cada quadro.
                let mut quadro = quadrantes(400, 300);
                for (x, y, pixel) in quadro.enumerate_pixels_mut() {
                    if x >= 200 && y >= 150 {
                        *pixel = image::Rgba([40 * i, 0, 250 - 40 * i, 255]);
                    }
                }
                gravador
                    .encode_frame(Frame::from_parts(
                        quadro,
                        0,
                        0,
                        Delay::from_numer_denom_ms(120, 1),
                    ))
                    .expect("gravar o quadro");
            }
        }
        let saida =
            imagem_do_perfil_recortada(&animada, false, pedaco(220, 160, 120, 120)).expect("corta");
        let quadros = GifDecoder::new(Cursor::new(&saida))
            .expect("continua um GIF")
            .into_frames()
            .collect_frames()
            .expect("quadros");
        assert_eq!(quadros.len(), 5);
        for (i, quadro) in quadros.iter().enumerate() {
            assert_eq!(quadro.buffer().dimensions(), (120, 120));
            assert_eq!(quadro.delay().numer_denom_ms(), (120, 1));
            let i = u8::try_from(i).expect("cabe");
            assert!(
                perto(
                    *quadro.buffer().get_pixel(60, 60),
                    [40 * i, 0, 250 - 40 * i, 255]
                ),
                "o quadro {i} não é o pedaço do quadro {i}"
            );
        }
        // E maior que o teto, cada quadro cabe nele.
        let grande = gif(1600, 1200, 3, 100);
        let saida = imagem_do_perfil_recortada(&grande, false, pedaco(100, 100, 1000, 1000))
            .expect("corta");
        let quadros = GifDecoder::new(Cursor::new(&saida))
            .expect("continua um GIF")
            .into_frames()
            .collect_frames()
            .expect("quadros");
        assert_eq!(quadros.len(), 3);
        assert_eq!(quadros[0].buffer().dimensions(), (512, 512));
    }

    /// **Quem confirma sem recortar uma imagem que já está na proporção não
    /// perde nada**: o recorte que é a imagem inteira sai pelo caminho de
    /// antes, com os mesmos bytes.
    #[test]
    fn o_recorte_que_e_a_imagem_inteira_sai_como_antes() {
        let pequena = png_de(&quadrantes(400, 400));
        assert_eq!(
            imagem_do_perfil_recortada(&pequena, false, pedaco(0, 0, 400, 400)).expect("passa"),
            pequena
        );
        // Num JPEG se vê: regravado, ele sairia com outros bytes (e pior).
        let foto = jpeg_de(&quadrantes(400, 400));
        assert_eq!(
            imagem_do_perfil_recortada(&foto, false, pedaco(0, 0, 400, 400)).expect("passa"),
            foto,
            "o recorte que é a imagem inteira regravou um JPEG que já cabia"
        );
        // Um pixel a menos já é recorte, e aí ele é regravado.
        assert_ne!(
            imagem_do_perfil_recortada(&foto, false, pedaco(0, 0, 399, 400)).expect("recorta"),
            foto
        );
        let grande = png_de(&quadrantes(2000, 2000));
        assert_eq!(
            imagem_do_perfil_recortada(&grande, false, pedaco(0, 0, 2000, 2000)).expect("reduz"),
            imagem_do_perfil_para_a_janela(&grande, false).expect("reduz")
        );
        let animada = gif(300, 300, 3, 100);
        assert_eq!(
            imagem_do_perfil_recortada(&animada, false, pedaco(0, 0, 300, 300)).expect("passa"),
            imagem_do_perfil_para_a_janela(&animada, false).expect("passa")
        );
    }

    /// O recorte que passa da borda é aparado a ela (a conta da tela é em
    /// frações), e o que não tem pixel nenhum é recusado pelo nome.
    #[test]
    fn o_recorte_e_aparado_a_imagem_e_o_vazio_e_recusado() {
        let original = png_de(&quadrantes(400, 300));
        let aparado = imagem_do_perfil_recortada(&original, false, pedaco(300, 200, 500, 500))
            .expect("apara");
        let aparado = image::load_from_memory(&aparado).expect("abre").to_rgba8();
        assert_eq!(aparado.dimensions(), (100, 100));
        assert!(aparado.pixels().all(|pixel| pixel.0 == AMARELO));
        // O tamanho que não cabe num `u32` somado ao canto também é aparado.
        let enorme =
            imagem_do_perfil_recortada(&original, false, pedaco(300, 200, u32::MAX, u32::MAX))
                .expect("apara");
        assert_eq!(dimensoes(&enorme), (100, 100));
        for vazio in [
            pedaco(400, 0, 10, 10),
            pedaco(0, 300, 10, 10),
            pedaco(10, 10, 0, 10),
            pedaco(10, 10, 10, 0),
            // Depois da borda, e no fim do `u32`: zero pixels, e não a volta.
            pedaco(5000, 0, 10, 10),
            pedaco(0, 5000, 10, 10),
            pedaco(u32::MAX, u32::MAX, u32::MAX, u32::MAX),
        ] {
            assert_eq!(
                imagem_do_perfil_recortada(&original, false, vazio).unwrap_err(),
                RECORTE_VAZIO,
                "{vazio:?}"
            );
        }
        // E os tetos de sempre continuam valendo com recorte.
        assert_eq!(
            imagem_do_perfil_recortada(&png(9000, 5000), true, pedaco(0, 0, 100, 100)).unwrap_err(),
            GRANDE_DEMAIS
        );
        assert!(imagem_do_perfil_recortada(b"nada disto", true, pedaco(0, 0, 1, 1)).is_err());
    }

    /// Um JPEG com a orientação 6 do EXIF (a foto de celular em pé: gravada
    /// deitada, com «gire 90° no sentido do relógio» anotado).
    fn jpeg_em_pe(deitada: &RgbaImage) -> Vec<u8> {
        let jpeg = jpeg_de(deitada);
        let exif: &[u8] = &[
            b'E', b'x', b'i', b'f', 0, 0, // o nome do bloco
            b'I', b'I', 0x2a, 0, 8, 0, 0, 0, // TIFF, little-endian, o IFD no byte 8
            1, 0, // uma entrada
            0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0,
            0, // 0x0112 (orientação), SHORT, 1, valor 6
            0, 0, 0, 0, // sem outro IFD
        ];
        let tamanho = u16::try_from(exif.len() + 2).expect("cabe");
        let mut saida = jpeg[..2].to_vec();
        saida.extend_from_slice(&[0xff, 0xe1]);
        saida.extend_from_slice(&tamanho.to_be_bytes());
        saida.extend_from_slice(exif);
        saida.extend_from_slice(&jpeg[2..]);
        saida
    }

    /// **O recorte é medido sobre a imagem como a pessoa a vê.** Uma foto em
    /// pé (orientação do EXIF) aparece em pé na prévia, e o pedaço de cima da
    /// prévia é o pedaço de cima da foto — e não o lado esquerdo do arquivo.
    #[test]
    fn o_recorte_respeita_a_orientacao_do_exif() {
        // Deitada no arquivo: a metade esquerda vermelha, a direita azul.
        let deitada = RgbaImage::from_fn(400, 200, |x, _| {
            image::Rgba(if x < 200 { VERMELHO } else { AZUL })
        });
        let foto = jpeg_em_pe(&deitada);
        // Girada 90° no sentido do relógio, a esquerda vai para cima.
        let previa = previa_do_recorte(&foto).expect("abre");
        assert_eq!((previa.largura, previa.altura), (200, 400));
        let vista = image::load_from_memory(&previa.bytes)
            .expect("abre")
            .to_rgba8();
        assert_eq!(vista.dimensions(), (200, 400));
        assert!(perto(*vista.get_pixel(100, 50), VERMELHO));
        assert!(perto(*vista.get_pixel(100, 350), AZUL));
        // O quadrado de baixo é azul, e o de cima, vermelho.
        for (recorte, cor) in [
            (pedaco(0, 0, 200, 180), VERMELHO),
            (pedaco(0, 220, 200, 180), AZUL),
        ] {
            let saida = imagem_do_perfil_recortada(&foto, false, recorte).expect("recorta");
            let imagem = image::load_from_memory(&saida).expect("abre").to_rgba8();
            assert_eq!(imagem.dimensions(), (200, 180));
            assert!(
                imagem.pixels().all(|pixel| perto(*pixel, cor)),
                "{recorte:?} não trouxe o pedaço que a pessoa viu"
            );
        }
        // A imagem inteira de uma foto orientada é regravada em pé: o caminho
        // de antes a reduziria deitada.
        let inteira =
            imagem_do_perfil_recortada(&foto, false, pedaco(0, 0, 200, 400)).expect("recorta");
        assert_eq!(dimensoes(&inteira), (200, 400));
    }

    /// **A prévia do recorte cabe no teto dela e diz as medidas da imagem
    /// inteira**, que são as do recorte; a de uma animação é o primeiro
    /// quadro, e diz que é animação.
    #[test]
    fn a_previa_do_recorte_cabe_no_teto_e_diz_as_medidas_da_inteira() {
        let grande = previa_do_recorte(&png_de(&quadrantes(4096, 3072))).expect("abre");
        assert_eq!((grande.largura, grande.altura), (4096, 3072));
        assert!(!grande.animada);
        let vista = image::load_from_memory(&grande.bytes)
            .expect("abre")
            .to_rgba8();
        assert_eq!(vista.dimensions(), (2048, 1536));
        assert_eq!(vista.get_pixel(10, 10).0, VERMELHO);
        assert_eq!(vista.get_pixel(2040, 1530).0, AMARELO);

        // A que cabe não é ampliada.
        let pequena = previa_do_recorte(&png_de(&quadrantes(64, 48))).expect("abre");
        assert_eq!(dimensoes(&pequena.bytes), (64, 48));

        // Uma foto em JPEG tem prévia em JPEG: a de um papel de parede em PNG
        // seria megabytes a mais na janela.
        let foto = previa_do_recorte(&jpeg_de(&quadrantes(3000, 2000))).expect("abre");
        assert_eq!(
            image::guess_format(&foto.bytes).expect("é imagem"),
            ImageFormat::Jpeg
        );

        let animada = previa_do_recorte(&gif(600, 400, 4, 100)).expect("abre");
        assert!(animada.animada);
        assert_eq!((animada.largura, animada.altura), (600, 400));
        assert_eq!(dimensoes(&animada.bytes), (600, 400));
        // Um GIF de um quadro só é uma imagem parada.
        assert!(
            !previa_do_recorte(&gif(60, 40, 1, 100))
                .expect("abre")
                .animada
        );

        assert_eq!(
            previa_do_recorte(&png(9000, 5000)).unwrap_err(),
            GRANDE_DEMAIS
        );
        assert!(previa_do_recorte(b"nada disto").is_err());
    }
}
