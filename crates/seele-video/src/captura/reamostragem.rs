//! De onde cada pixel de destino vem: a reamostragem e a conversão de cor.
//!
//! # Por que este módulo existe separado da captura
//!
//! Tudo aqui é aritmética pura — não toca a WGC, o D3D11 nem o Windows. Morava
//! dentro do `captura/windows.rs`, e o preço disso era que **quatro dos cinco
//! testes desta lógica só rodavam no Windows**: a proporção com tarja, o
//! vermelho que não pode virar azul, o passo de linha que não pode vazar e a
//! média que lê a área inteira. Nenhum deles fala de Windows.
//!
//! O módulo é compilado sob `test` em qualquer plataforma, e em produção só
//! onde é usado.
//!
//! # O caminho, e por que ele é o que é
//!
//! O `spikes/tela-na-cor` mediu a função que existia aqui e achou o contrário
//! do que todo mundo supunha. Ela fazia **duas** coisas num laço só —
//! redimensionar por média de área e converter BGRA em I420 — e a repartição do
//! custo era:
//!
//! | | mediana, M5 Pro | fatia |
//! |---|---|---|
//! | o laço inteiro | 8,03 ms | 100% |
//! | só a conversão de cor | 0,51 ms | **6%** |
//! | só o redimensionamento | 5,72 ms | **94%** |
//!
//! **A conversão de cor nunca foi o gasto.** O comentário que o
//! `captura/windows.rs` carregava — «a conversão é o maior gasto de CPU do
//! compartilhamento, e não o codificador», 16,5 ms contra 0,105 de núcleo do
//! OpenH264 num Ryzen 7 5800X3D — estava certo sobre a função inteira e
//! apontava para dentro dela o lugar errado.
//!
//! Então o laço à mão sai inteiro e vira dois passos, cada um numa biblioteca
//! que faz o trabalho em blocos de pixels em vez de um por um:
//!
//! 1. **`dcv-color-primitives`** converte BGRA em I420 **na resolução da
//!    fonte**, respeitando o passo de linha;
//! 2. **`fast_image_resize`** reduz os três planos até a área útil.
//!
//! Converter antes de reduzir, e não o contrário, porque I420 tem 1,5 byte por
//! pixel contra 4 do BGRA: o redimensionamento — que é o caro — toca menos de
//! metade da memória se vier depois.
//!
//! # O ganho, medido nas duas máquinas
//!
//! Os dois lados da conta vêm do mesmo teste, com o mesmo conteúdo e o mesmo
//! número de rodadas — o `medicao::quanto_custa_de_verdade` para o caminho
//! novo, e o mesmo corpo aplicado ao laço antigo no commit `1acbdb3`. Comparar
//! contra a medição de bancada anterior daria um número melhor e falso.
//!
//! | 1440p → 1080p | laço antigo | agora | ganho |
//! |---|---|---|---|
//! | **Ryzen 7 5800X3D** (o alvo) | 17,69 ms · 0,531 núcleo | **7,42 ms · 0,223** | **2,4×** |
//! | Apple M5 Pro (onde se desenvolve) | 8,03 ms · 0,241 núcleo | 2,89 ms · 0,087 | 2,8× |
//!
//! Os 17,69 ms confirmam os **16,5** que o `captura/windows.rs` documentava de
//! uma bancada diferente — captura real de monitor em vez de conteúdo
//! sintético. A medição antiga estava certa.
//!
//! O ganho é menor no Ryzen que no Mac (2,4× contra 2,8×), e é o lembrete de
//! que razão medida numa máquina não transfere para outra: uma previsão feita
//! aqui dizia «~6 ms no Ryzen» e o número real foi 7,42.
//!
//! # E os 60 quadros por segundo
//!
//! A conversão mora numa thread só — a que o `windows-capture` cria —, então o
//! teto dela é um núcleo. A 60 quadros, um quadro chega a cada 16,6 ms:
//!
//! - **antes**: 17,69 ms por quadro. Maior que o intervalo entre quadros, então
//!   a thread ficava permanentemente para trás. Impossível, não difícil.
//! - **agora**: 7,42 ms, com 55% de folga no intervalo — e 0,45 de núcleo ao
//!   longo do segundo, contra 1,06 de antes, que já não cabia.
//!
//! Isso **abre** os 60 quadros no Windows; não os entrega. A `Cadencia::Q60`
//! continua fora do que o §6 item 10 oferece, e mexer nisso é outra decisão.
//!
//! # A diferença que a troca traz, e que não é defeito
//!
//! O filtro caixa da `fast_image_resize` **não devolve os mesmos pixels** que o
//! laço antigo: diferença média abaixo de 1,2 nível em 255, com pior pixel de
//! 24 na luma e 42 no croma. A causa é estrutural e está medida no spike — o
//! `faixas()` que existia aqui punha cada pixel de origem inteiro num balde só,
//! e um filtro caixa pesa cobertura fracionária. Em 2560→1920 cada pixel de
//! destino cobre 1,333 de origem, então os dois discordam nas bordas de bloco,
//! e o da biblioteca é o matematicamente correto.
//!
//! O que **não** muda é a propriedade pela qual a média de área foi escolhida:
//! ela continua lendo a área inteira, e não o vizinho mais próximo. Era o
//! ponto do `a_media_le_a_area_inteira_e_nao_um_vizinho`, que por isso deixou
//! de testar uma função interna e passou a testar o resultado.
//!
//! # O que a origem entrega
//!
//! BGRA empacotado, com **passo** (`row_pitch`) que o driver escolhe e que
//! quase nunca é `largura × 4`. Ler linha após linha sem respeitar o passo
//! entorta a imagem em diagonal — o `o_passo_maior_que_a_largura_nao_vaza`
//! existe para prender isso.

#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use dcv_color_primitives::{convert_image, ColorSpace, ImageFormat, PixelFormat};
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

use crate::codec::{bytes_de_croma, bytes_de_luma, QuadroI420, Resolucao};
use crate::erro::ErroDeVideo;

/// O preto do intervalo de TV, o mesmo que [`QuadroI420::preto`] usa.
///
/// É com ele que as tarjas se preenchem quando a fonte não tem a proporção do
/// destino. Zerar daria verde, que é o defeito clássico de quem trata I420 como
/// se fosse RGB.
pub(super) const PRETO_LUMA: u8 = 16;

/// O centro do croma, que é o cinza sem cor.
pub(super) const CENTRO_CROMA: u8 = 128;

/// **BT.709 de faixa de TV**, que é o espaço em que este projeto transmite.
///
/// **709 e não 601**, e o motivo é o conteúdo: as três resoluções que o §5
/// oferece são 540p, 720p e 1080p, e um decodificador que precisa adivinhar
/// adivinha 709 para as duas maiores. Desde o `vui.rs` o codec **escreve** a
/// descrição de cor no SPS, então quem recebe não adivinha mais — mas a
/// escolha aqui é o que aquele SPS declara, e as duas têm de continuar
/// dizendo a mesma coisa.
///
/// Faixa de TV (16..235 na luma, 16..240 no croma) e não faixa cheia, porque é
/// o que [`QuadroI420::preto`] já assume ao pintar 16 e 128. Na
/// `dcv-color-primitives` é o que [`ColorSpace::Bt709`] significa; a faixa
/// cheia tem nome próprio lá.
const ESPACO: ColorSpace = ColorSpace::Bt709;

/// Média de área, que é o que reduz texto sem comê-lo.
///
/// Caixa e não Lanczos: o argumento original — «texto reduzido por vizinho mais
/// próximo perde linhas de pixel inteiras: some a barra do `t` e o `e` vira
/// `c`» — pedia média da área coberta, e caixa é exatamente isso. Lanczos daria
/// mais nitidez e um halo em volta de cada letra, que é o defeito oposto.
const FILTRO: ResizeAlg = ResizeAlg::Convolution(FilterType::Box);

/// De onde cada pixel de destino vem, calculado uma vez por tamanho de origem.
///
/// A conta é refeita quando a fonte muda de tamanho — uma janela redimensionada
/// no meio da transmissão —, e só então. Os buffers intermediários moram aqui
/// pelo mesmo motivo: são cinco megabytes em 1440p, e alocá-los por quadro
/// seria trocar o custo que acabamos de tirar por outro.
pub(super) struct Mapa {
    /// O tamanho da fonte, lido pelo `windows.rs` para saber se ela mudou de
    /// tamanho no meio da transmissão e o mapa precisa ser refeito.
    pub(super) origem: (usize, usize),
    destino: Resolucao,
    /// A área útil dentro do destino, em pixels. Sempre par nos dois lados,
    /// porque o croma de um I420 anda de dois em dois.
    ativa: (usize, usize),
    /// O canto de cima e à esquerda da área útil. Também par, pelo mesmo motivo.
    canto: (usize, usize),
    /// A origem recortada para lados pares.
    ///
    /// I420 não tem meio pixel de croma, então uma janela de 1023×769 perde a
    /// última coluna e a última linha. É um pixel na borda de uma imagem que
    /// vai ser reduzida; a alternativa seria inventar a coluna que falta.
    fonte: (usize, usize),
    /// I420 na resolução da fonte, antes de reduzir. Reaproveitado.
    fonte_y: Vec<u8>,
    fonte_u: Vec<u8>,
    fonte_v: Vec<u8>,
    /// I420 já reduzido à área útil, antes de assentar no destino.
    reduzido_y: Image<'static>,
    reduzido_u: Image<'static>,
    reduzido_v: Image<'static>,
    reamostrador: Resizer,
}

/// Arredonda para baixo até um número par, com teto e piso.
const fn par_ate(valor: usize, teto: usize) -> usize {
    let cortado = if valor > teto { teto } else { valor };
    let par = cortado & !1;
    if par < 2 {
        2
    } else {
        par
    }
}

/// Copia um plano reduzido para dentro do plano de destino, no canto.
///
/// O que fica de fora são as tarjas, e elas **não** são escritas aqui: o
/// destino já nasceu pintado com o preto de TV e o centro do croma. Escrever a
/// tarja seria escrever duas vezes a mesma coisa.
fn assentar(
    destino: &mut [u8],
    largura_destino: usize,
    origem: &[u8],
    (largura_origem, altura_origem): (usize, usize),
    (canto_x, canto_y): (usize, usize),
) {
    for y in 0..altura_origem {
        let (Some(linha_origem), Some(linha_destino)) = (
            origem.get(y * largura_origem..(y + 1) * largura_origem),
            destino.get_mut(
                (canto_y + y) * largura_destino + canto_x
                    ..(canto_y + y) * largura_destino + canto_x + largura_origem,
            ),
        ) else {
            continue;
        };
        linha_destino.copy_from_slice(linha_origem);
    }
}

impl Mapa {
    /// Monta o mapa de uma origem para uma resolução de destino.
    ///
    /// **A proporção é preservada, e o que sobra vira tarja preta.** Esticar a
    /// imagem seria a única alternativa sem tarja, e ela deforma texto — que é
    /// exatamente o conteúdo que o §2 diz que se está transmitindo. Uma sessão
    /// SSH do Windows enxerga 1024×768, ou seja 4:3 dentro de um destino 16:9:
    /// a tarja não é caso raro, é o caso da máquina onde isto foi medido.
    pub(super) fn novo(origem: (usize, usize), destino: Resolucao) -> Self {
        let (largura_origem, altura_origem) = origem;
        let largura_destino = destino.largura();
        let altura_destino = destino.altura();

        let (ativa_largura, ativa_altura) = if largura_origem == 0 || altura_origem == 0 {
            (largura_destino, altura_destino)
        } else if largura_origem * altura_destino <= largura_destino * altura_origem {
            // A altura é quem limita: a origem é mais «alta» que o destino.
            (
                par_ate(
                    largura_origem * altura_destino / altura_origem,
                    largura_destino,
                ),
                altura_destino,
            )
        } else {
            (
                largura_destino,
                par_ate(
                    altura_origem * largura_destino / largura_origem,
                    altura_destino,
                ),
            )
        };

        let canto = (
            ((largura_destino - ativa_largura) / 2) & !1,
            ((altura_destino - ativa_altura) / 2) & !1,
        );

        let fonte = (largura_origem & !1, altura_origem & !1);
        let croma_fonte = (fonte.0 / 2, fonte.1 / 2);

        Self {
            origem,
            destino,
            ativa: (ativa_largura, ativa_altura),
            canto,
            fonte,
            fonte_y: vec![PRETO_LUMA; fonte.0 * fonte.1],
            fonte_u: vec![CENTRO_CROMA; croma_fonte.0 * croma_fonte.1],
            fonte_v: vec![CENTRO_CROMA; croma_fonte.0 * croma_fonte.1],
            reduzido_y: Image::new(ativa_largura as u32, ativa_altura as u32, PixelType::U8),
            reduzido_u: Image::new(
                (ativa_largura / 2) as u32,
                (ativa_altura / 2) as u32,
                PixelType::U8,
            ),
            reduzido_v: Image::new(
                (ativa_largura / 2) as u32,
                (ativa_altura / 2) as u32,
                PixelType::U8,
            ),
            reamostrador: Resizer::new(),
        }
    }

    /// Converte um quadro BGRA de `passo` bytes por linha num [`QuadroI420`].
    ///
    /// **O `passo` não é `largura × 4`**, e tratá-lo como se fosse é o defeito
    /// clássico deste caminho: a textura que o D3D11 mapeia tem o `row_pitch`
    /// que o driver quiser, e ler linha após linha sem ele entorta a imagem em
    /// diagonal. É o que o teste `o_passo_maior_que_a_largura_nao_vaza` prende.
    ///
    /// Um quadro que não dá para converter — fonte de lado zero, buffer mais
    /// curto do que o passo promete — sai **preto**, e não em erro: derrubar a
    /// captura inteira por causa de um quadro é pior que perder o quadro. Quem
    /// chama já conta as falhas.
    pub(super) fn converter(
        &mut self,
        bytes: &[u8],
        passo: usize,
    ) -> Result<QuadroI420, ErroDeVideo> {
        let largura = self.destino.largura();
        let altura = self.destino.altura();

        let mut luma = vec![PRETO_LUMA; bytes_de_luma(largura, altura)];
        let mut croma_u = vec![CENTRO_CROMA; bytes_de_croma(largura, altura)];
        let mut croma_v = vec![CENTRO_CROMA; bytes_de_croma(largura, altura)];

        if self.reduzir(bytes, passo).is_some() {
            let largura_croma = largura.div_ceil(2);
            let croma_ativo = (self.ativa.0 / 2, self.ativa.1 / 2);
            let croma_canto = (self.canto.0 / 2, self.canto.1 / 2);
            assentar(
                &mut luma,
                largura,
                self.reduzido_y.buffer(),
                self.ativa,
                self.canto,
            );
            assentar(
                &mut croma_u,
                largura_croma,
                self.reduzido_u.buffer(),
                croma_ativo,
                croma_canto,
            );
            assentar(
                &mut croma_v,
                largura_croma,
                self.reduzido_v.buffer(),
                croma_ativo,
                croma_canto,
            );
        }

        QuadroI420::novo(largura, altura, luma, croma_u, croma_v)
    }

    /// Converte a fonte em I420 e a reduz à área útil. `None` quando não dá.
    fn reduzir(&mut self, bytes: &[u8], passo: usize) -> Option<()> {
        if self.fonte.0 < 2 || self.fonte.1 < 2 {
            return None;
        }
        // A `dcv` lê `passo × altura` bytes. Um buffer mais curto do que isso
        // não é quadro: é a captura entregando menos do que anunciou.
        let uteis = bytes.get(..passo.checked_mul(self.fonte.1)?)?;

        convert_image(
            u32::try_from(self.fonte.0).ok()?,
            u32::try_from(self.fonte.1).ok()?,
            &ImageFormat {
                pixel_format: PixelFormat::Bgra,
                color_space: ColorSpace::Rgb,
                num_planes: 1,
            },
            Some(&[passo]),
            &[uteis],
            &ImageFormat {
                pixel_format: PixelFormat::I420,
                color_space: ESPACO,
                num_planes: 3,
            },
            None,
            &mut [&mut self.fonte_y, &mut self.fonte_u, &mut self.fonte_v],
        )
        .ok()?;

        let croma_fonte = (self.fonte.0 / 2, self.fonte.1 / 2);
        for (plano, tamanho, alvo) in [
            (&self.fonte_y, self.fonte, &mut self.reduzido_y),
            (&self.fonte_u, croma_fonte, &mut self.reduzido_u),
            (&self.fonte_v, croma_fonte, &mut self.reduzido_v),
        ] {
            let entrada = ImageRef::new(
                u32::try_from(tamanho.0).ok()?,
                u32::try_from(tamanho.1).ok()?,
                plano,
                PixelType::U8,
            )
            .ok()?;
            self.reamostrador
                .resize(&entrada, alvo, &ResizeOptions::new().resize_alg(FILTRO))
                .ok()?;
        }
        Some(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Uma origem BGRA de uma cor só, com o preenchimento de fim de linha
    /// pintado de branco para que quem o ler por engano seja pego.
    fn origem(largura: usize, altura: usize, passo: usize, cor: (u8, u8, u8)) -> Vec<u8> {
        let mut bytes = vec![0xFF; passo * altura];
        for linha in bytes.chunks_exact_mut(passo) {
            if let Some(util) = linha.get_mut(..largura * 4) {
                for pixel in util.chunks_exact_mut(4) {
                    if let [azul, verde, vermelho, alfa] = pixel {
                        *azul = cor.0;
                        *verde = cor.1;
                        *vermelho = cor.2;
                        *alfa = 0xFF;
                    }
                }
            }
        }
        bytes
    }

    fn luma_em(quadro: &QuadroI420, x: usize, y: usize) -> u8 {
        quadro
            .luma()
            .get(y * quadro.largura() + x)
            .copied()
            .unwrap_or(0)
    }

    fn croma_em(quadro: &QuadroI420, x: usize, y: usize) -> (u8, u8) {
        let indice = y * quadro.largura().div_ceil(2) + x;
        (
            quadro.croma_u().get(indice).copied().unwrap_or(0),
            quadro.croma_v().get(indice).copied().unwrap_or(0),
        )
    }

    #[test]
    fn a_proporcao_e_preservada_e_o_resto_vira_tarja() {
        // 1024×768 é o que uma sessão SSH do Windows enxerga, e 4:3 num destino
        // 16:9 é o caso desta casa, não um caso de borda.
        let mut mapa = Mapa::novo((1024, 768), Resolucao::P720);
        assert_eq!(mapa.ativa, (960, 720));
        assert_eq!(mapa.canto, (160, 0));

        let quadro = mapa
            .converter(&origem(1024, 768, 1024 * 4, (0xFF, 0xFF, 0xFF)), 1024 * 4)
            .expect("os planos fecham");

        // Dentro da área útil, branco; nas tarjas, o preto de TV.
        assert_eq!(luma_em(&quadro, 600, 360), 235);
        assert_eq!(luma_em(&quadro, 0, 360), PRETO_LUMA);
        assert_eq!(luma_em(&quadro, 1279, 360), PRETO_LUMA);
        assert_eq!(croma_em(&quadro, 0, 180), (CENTRO_CROMA, CENTRO_CROMA));
    }

    #[test]
    fn o_vermelho_nao_vira_azul() {
        // A origem é BGRA. Trocar os canais dá uma imagem que parece certa em
        // cinza e erra em tudo que tem cor, e é o defeito mais fácil de deixar
        // passar. Em BT.709 de faixa de TV, vermelho puro dá U ≈ 103 e V ≈ 239;
        // se azul e vermelho trocassem de lugar, viria U ≈ 240 e V ≈ 118, que é
        // o que estes dois limites separam.
        let mut mapa = Mapa::novo((16, 16), Resolucao::P540);
        let quadro = mapa
            .converter(&origem(16, 16, 16 * 4, (0x00, 0x00, 0xFF)), 16 * 4)
            .expect("os planos fecham");

        let (u, v) = croma_em(&quadro, 240, 135);
        assert!((100..=106).contains(&u), "U de vermelho puro veio {u}");
        assert!((235..=240).contains(&v), "V de vermelho puro veio {v}");
        // 63, e não os 62 que este teste pedia antes da troca de biblioteca.
        // O valor exato de BT.709 para vermelho puro é 16 + 219 × 0,2126 =
        // **62,56**, que arredonda para 63. O laço antigo fazia `47/256` com
        // `div_euclid`, que trunca, e devolvia 62: era o nosso erro de
        // arredondamento que este número prendia, não a cor.
        //
        // Vale para a imagem inteira e não só para o vermelho: a conversão
        // passou a arredondar onde truncava, então tudo fica em média meio
        // nível mais claro. Meio nível em 255 não se vê; ficar registrado é o
        // que impede alguém de «consertar» isto de volta um dia.
        assert_eq!(luma_em(&quadro, 480, 270), 63);
    }

    #[test]
    fn o_passo_maior_que_a_largura_nao_vaza() {
        // A textura que o D3D11 mapeia tem o `row_pitch` que o driver quiser, e
        // ler `largura × 4` por linha entorta a imagem em diagonal. Aqui o
        // preenchimento é branco e a imagem é preta: qualquer luma acima do
        // preto de TV é preenchimento lido por engano.
        let largura = 8;
        let altura = 8;
        let passo = largura * 4 + 96;
        let bytes = origem(largura, altura, passo, (0x00, 0x00, 0x00));

        let quadro = Mapa::novo((largura, altura), Resolucao::P540)
            .converter(&bytes, passo)
            .expect("os planos fecham");

        let maior = quadro.luma().iter().copied().max().unwrap_or(0);
        assert_eq!(maior, PRETO_LUMA, "o preenchimento de fim de linha vazou");
    }

    #[test]
    fn a_media_le_a_area_inteira_e_nao_um_vizinho() {
        // **A propriedade que a troca de biblioteca põe em risco**, e por isso
        // este teste deixou de olhar uma função interna e passou a olhar o
        // resultado: reduzir tem de ler a área coberta inteira, e não escolher
        // um vizinho. Texto reduzido por vizinho mais próximo perde linhas de
        // pixel inteiras — some a barra do «t» e o «e» vira «c».
        //
        // Colunas de um pixel, alternando preto e branco, reduzidas de 1920
        // para 960: cada pixel de destino cobre exatamente um preto e um
        // branco. Média de área dá cinza; vizinho mais próximo daria uma das
        // duas pontas, que é o que os limites abaixo separam.
        let (largura, altura) = (1920, 1080);
        let passo = largura * 4;
        let mut bytes = vec![0u8; passo * altura];
        for linha in bytes.chunks_exact_mut(passo) {
            for (x, pixel) in linha.chunks_exact_mut(4).enumerate() {
                let tom = if x % 2 == 0 { 0x00 } else { 0xFF };
                if let [azul, verde, vermelho, alfa] = pixel {
                    (*azul, *verde, *vermelho, *alfa) = (tom, tom, tom, 0xFF);
                }
            }
        }

        let quadro = Mapa::novo((largura, altura), Resolucao::P540)
            .converter(&bytes, passo)
            .expect("os planos fecham");

        // Cinza médio em faixa de TV é ~125; preto é 16 e branco é 235.
        let meio = luma_em(&quadro, 480, 270);
        assert!(
            (110..=140).contains(&meio),
            "reduzir escolheu um vizinho em vez de medir a área: luma {meio}"
        );
    }
}

/// A medição que fecha a conta, e que precisa ser refeita no Windows.
///
/// Ignorada por padrão porque é um cronômetro, não uma afirmação: numa máquina
/// carregada ela dá outro número e falharia sem nada estar errado. Existe para
/// ser rodada de propósito, e o número dela é o que os cabeçalhos deste módulo
/// citam.
///
/// ```text
/// cargo test --release -p seele-video --lib quanto_custa -- --ignored --nocapture
/// ```
///
/// **Já rodou no Windows**, no Ryzen 7 5800X3D que é o alvo: 7,42 ms. O
/// cabeçalho do módulo traz a tabela com os dois lados da conta. Rode de novo
/// quando mexer nesta função — é ela que diz se a mexida custou caro.
#[cfg(test)]
mod medicao {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore = "cronômetro, não afirmação: rode de propósito"]
    fn quanto_custa_de_verdade() {
        let (largura, altura) = (2560, 1440);
        let passo = largura * 4;

        // Conteúdo de tela sintético: gradiente, blocos e ruído. Cor chapada
        // seria injustamente amigável ao cache e mediria um caso que não
        // existe.
        let mut bytes = vec![0u8; passo * altura];
        let mut semente = 0x2545_F491_4F6C_DD1Du64;
        for (y, linha) in bytes.chunks_exact_mut(passo).enumerate() {
            for (x, pixel) in linha.chunks_exact_mut(4).enumerate() {
                semente ^= semente << 13;
                semente ^= semente >> 7;
                semente ^= semente << 17;
                if let [azul, verde, vermelho, alfa] = pixel {
                    *azul = (x % 256) as u8 ^ (semente & 0x1F) as u8;
                    *verde = (y % 256) as u8;
                    *vermelho = ((x + y) % 256) as u8;
                    *alfa = 255;
                }
            }
        }

        let mut mapa = Mapa::novo((largura, altura), Resolucao::P1080);
        for _ in 0..3 {
            let _ = mapa.converter(&bytes, passo);
        }

        let mut tempos = Vec::with_capacity(30);
        for _ in 0..30 {
            let comeco = Instant::now();
            let quadro = mapa.converter(&bytes, passo).expect("converte");
            tempos.push(comeco.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(quadro);
        }
        tempos.sort_by(|a, b| a.partial_cmp(b).expect("sem NaN"));

        let mediana = tempos.get(tempos.len() / 2).copied().unwrap_or(0.0);
        let minimo = tempos.first().copied().unwrap_or(0.0);
        println!(
            "\n  converter() real, 1440p->1080p: mediana {mediana:.2} ms | \
             min {minimo:.2} ms | {:.3} de nucleo a 30fps\n  \
             (o laco antigo, mesmo teste no spike: 8.03 ms)\n",
            mediana / (1000.0 / 30.0)
        );
    }
}
