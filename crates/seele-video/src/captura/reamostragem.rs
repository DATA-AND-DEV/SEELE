//! De onde cada pixel de destino vem: a reamostragem e a conversão de cor.
//!
//! # Por que este módulo existe separado da captura
//!
//! Tudo aqui é aritmética pura — não toca a WGC, o D3D11 nem o Windows. Morava
//! dentro do `captura/windows.rs`, e o preço disso era que **quatro dos cinco
//! testes desta lógica só rodavam no Windows**: a proporção com tarja, o
//! vermelho que não pode virar azul, o passo de linha que não pode vazar e a
//! média que lê a área inteira. Nenhum deles fala de Windows; todos ficaram
//! presos atrás do `cfg(target_os = "windows")`, sem rodar no Mac de quem
//! desenvolve nem no CI de Linux.
//!
//! O módulo é compilado sob `test` em qualquer plataforma, e em produção só
//! onde é usado. Fora do `test` num Mac ele seria código morto, e código morto
//! que o compilador aceita em silêncio é como a lógica apodrece.
//!
//! # O que a origem entrega
//!
//! BGRA empacotado, com **passo** (`row_pitch`) que o driver escolhe e que
//! quase nunca é `largura × 4`. Ler linha após linha sem respeitar o passo
//! entorta a imagem em diagonal — é o defeito clássico deste caminho, e o
//! `o_passo_maior_que_a_largura_nao_vaza` existe para prendê-lo.

#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

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

/// De onde cada pixel de destino vem, calculado uma vez por tamanho de origem.
///
/// A conta é refeita quando a fonte muda de tamanho — uma janela redimensionada
/// no meio da transmissão —, e só então: por quadro ela seria uma divisão por
/// pixel de destino, que é dois milhões de divisões em 1080p.
#[derive(Debug, PartialEq, Eq)]
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
    /// Para cada coluna útil, a faixa de colunas de origem que a alimenta.
    colunas: Vec<(usize, usize)>,
    /// Para cada linha útil, a faixa de linhas de origem que a alimenta.
    linhas: Vec<(usize, usize)>,
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

/// Para cada um dos `destino` pixels, a faixa de pixels de origem que ele cobre.
fn faixas(origem: usize, destino: usize) -> Vec<(usize, usize)> {
    (0..destino)
        .map(|i| {
            let inicio = i * origem / destino;
            let fim = ((i + 1) * origem / destino).max(inicio + 1).min(origem);
            (inicio, fim)
        })
        .collect()
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

        Self {
            origem,
            destino,
            ativa: (ativa_largura, ativa_altura),
            canto,
            colunas: faixas(largura_origem.max(1), ativa_largura),
            linhas: faixas(altura_origem.max(1), ativa_altura),
        }
    }

    /// Converte um quadro BGRA de `passo` bytes por linha num [`QuadroI420`].
    ///
    /// **O `passo` não é `largura × 4`**, e tratá-lo como se fosse é o defeito
    /// clássico deste caminho: a textura que o D3D11 mapeia tem o `row_pitch`
    /// que o driver quiser, e ler linha após linha sem ele entorta a imagem em
    /// diagonal. É o que o teste `o_passo_maior_que_a_largura_nao_vaza` prende.
    pub(super) fn converter(&self, bytes: &[u8], passo: usize) -> Result<QuadroI420, ErroDeVideo> {
        let largura = self.destino.largura();
        let altura = self.destino.altura();
        let largura_croma = largura.div_ceil(2);
        let (ativa_largura, ativa_altura) = self.ativa;
        let (canto_x, canto_y) = self.canto;

        let mut luma = vec![PRETO_LUMA; bytes_de_luma(largura, altura)];
        let mut croma_u = vec![CENTRO_CROMA; bytes_de_croma(largura, altura)];
        let mut croma_v = vec![CENTRO_CROMA; bytes_de_croma(largura, altura)];

        let pares_de_linha = luma
            .chunks_exact_mut(largura * 2)
            .skip(canto_y / 2)
            .take(ativa_altura / 2);
        let linhas_u = croma_u
            .chunks_exact_mut(largura_croma)
            .skip(canto_y / 2)
            .take(ativa_altura / 2);
        let linhas_v = croma_v
            .chunks_exact_mut(largura_croma)
            .skip(canto_y / 2)
            .take(ativa_altura / 2);

        for (bloco_y, ((par_de_linhas, linha_u), linha_v)) in
            pares_de_linha.zip(linhas_u).zip(linhas_v).enumerate()
        {
            let (
                Some(&(origem_y0_cima, origem_y1_cima)),
                Some(&(origem_y0_baixo, origem_y1_baixo)),
            ) = (
                self.linhas.get(bloco_y * 2),
                self.linhas.get(bloco_y * 2 + 1),
            )
            else {
                continue;
            };

            let (linha_cima, linha_baixo) = par_de_linhas.split_at_mut(largura);
            let (Some(faixa_cima), Some(faixa_baixo)) = (
                linha_cima.get_mut(canto_x..canto_x + ativa_largura),
                linha_baixo.get_mut(canto_x..canto_x + ativa_largura),
            ) else {
                continue;
            };
            let (Some(faixa_u), Some(faixa_v)) = (
                linha_u.get_mut(canto_x / 2..canto_x / 2 + ativa_largura / 2),
                linha_v.get_mut(canto_x / 2..canto_x / 2 + ativa_largura / 2),
            ) else {
                continue;
            };

            for (bloco_x, (((alto, baixo), destino_u), destino_v)) in faixa_cima
                .chunks_exact_mut(2)
                .zip(faixa_baixo.chunks_exact_mut(2))
                .zip(faixa_u.iter_mut())
                .zip(faixa_v.iter_mut())
                .enumerate()
            {
                let (Some(&(origem_x0_esq, origem_x1_esq)), Some(&(origem_x0_dir, origem_x1_dir))) = (
                    self.colunas.get(bloco_x * 2),
                    self.colunas.get(bloco_x * 2 + 1),
                ) else {
                    continue;
                };

                // As quatro cores médias do bloco 2×2. A média é sobre a área de
                // origem inteira que cada pixel de destino cobre — e não uma
                // amostra do vizinho mais próximo —, porque texto reduzido por
                // vizinho mais próximo perde linhas de pixel inteiras: some a
                // barra do «t» e o «e» vira «c». Custa ler cada pixel de origem
                // uma vez, que é o mesmo que qualquer conversão faria.
                let cor_ce = media(
                    bytes,
                    passo,
                    origem_x0_esq,
                    origem_x1_esq,
                    origem_y0_cima,
                    origem_y1_cima,
                );
                let cor_cd = media(
                    bytes,
                    passo,
                    origem_x0_dir,
                    origem_x1_dir,
                    origem_y0_cima,
                    origem_y1_cima,
                );
                let cor_be = media(
                    bytes,
                    passo,
                    origem_x0_esq,
                    origem_x1_esq,
                    origem_y0_baixo,
                    origem_y1_baixo,
                );
                let cor_bd = media(
                    bytes,
                    passo,
                    origem_x0_dir,
                    origem_x1_dir,
                    origem_y0_baixo,
                    origem_y1_baixo,
                );

                if let [esquerda, direita] = alto {
                    *esquerda = luma_de(cor_ce);
                    *direita = luma_de(cor_cd);
                }
                if let [esquerda, direita] = baixo {
                    *esquerda = luma_de(cor_be);
                    *direita = luma_de(cor_bd);
                }

                // O croma do I420 é um por bloco de 2×2, então ele sai da cor
                // média dos quatro — e não de um dos quatro. Escolher um canto
                // deslocaria a cor meio pixel para aquele lado em toda a imagem.
                let media_do_bloco = (
                    (cor_ce.0 + cor_cd.0 + cor_be.0 + cor_bd.0) / 4,
                    (cor_ce.1 + cor_cd.1 + cor_be.1 + cor_bd.1) / 4,
                    (cor_ce.2 + cor_cd.2 + cor_be.2 + cor_bd.2) / 4,
                );
                let (u, v) = croma_de(media_do_bloco);
                *destino_u = u;
                *destino_v = v;
            }
        }

        QuadroI420::novo(largura, altura, luma, croma_u, croma_v)
    }
}

/// A cor média de um retângulo da origem, em (R, G, B).
///
/// A origem é BGRA — que é a ordem que a WGC entrega e que
/// [`ColorFormat::Bgra8`] pede. Trocar B por R aqui é o defeito mais fácil de
/// cometer e o mais difícil de ver num teste de tela cinza; o teste
/// `o_vermelho_nao_vira_azul` existe só para isso.
///
/// # O que esta função custa, medido
///
/// **A conversão é o maior gasto de CPU do compartilhamento de tela, e não o
/// codificador.** Num Ryzen 7 5800X3D, capturando um monitor de 2560×1440:
/// 9,0 ms por quadro para 720p e 16,5 ms para 1080p, ou seja **0,27 e 0,50 de
/// um núcleo a 30 quadros por segundo**. `spikes/tela-no-codec` mediu o
/// OpenH264 na mesma máquina em **0,105** de núcleo a 1080p30: converter custa
/// cinco vezes codificar, e o spike disse, na cara, que este número faltava.
///
/// **O gasto anda com o número de pixels de destino, não de origem** — a mesma
/// tela de 2560×1440 custa 1,8× mais para 1080p que para 720p —, e duas
/// tentativas de espremê-lo não mudaram nada de medido: tirar a conferência de
/// limite por byte (`first_chunk`) e trocar três divisões inteiras por um
/// recíproco deram 16,5 e 16,9 ms contra os mesmos 16,6 de antes. O que sobra é
/// custo de laço por pixel de destino, e baixá-lo é **mudança de desenho** —
/// converter na GPU, ou converter só as regiões sujas que a WGC já sabe
/// apontar —, não ajuste. Está no relatório para quem coordena decidir.
fn media(
    bytes: &[u8],
    passo: usize,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
) -> (u32, u32, u32) {
    let mut soma = (0u32, 0u32, 0u32);
    let mut quantos = 0u32;

    for y in y0..y1 {
        let Some(faixa) = bytes.get(y * passo + x0 * 4..y * passo + x1 * 4) else {
            continue;
        };
        for pixel in faixa.chunks_exact(4) {
            if let [azul, verde, vermelho, _alfa] = pixel {
                soma.0 += u32::from(*vermelho);
                soma.1 += u32::from(*verde);
                soma.2 += u32::from(*azul);
                quantos += 1;
            }
        }
    }

    if quantos == 0 {
        return (0, 0, 0);
    }
    (soma.0 / quantos, soma.1 / quantos, soma.2 / quantos)
}

/// Luma de uma cor, em BT.709 de faixa de TV.
///
/// **709 e não 601**, e o motivo é o conteúdo: as três resoluções que o §5
/// oferece são 540p, 720p e 1080p, e um decodificador que precisa adivinhar
/// adivinha 709 para as duas maiores. O codec não escreve VUI nenhum hoje —
/// está no relatório, porque quem conserta isso é `codec.rs`, e enquanto não
/// escrever, quem recebe adivinha.
///
/// Faixa de TV (16..235 para luma, 16..240 para croma) e não faixa cheia,
/// porque é o que [`QuadroI420::preto`] já assume ao pintar 16 e 128.
fn luma_de((vermelho, verde, azul): (u32, u32, u32)) -> u8 {
    let soma = 47 * vermelho as i32 + 157 * verde as i32 + 16 * azul as i32;
    (16 + soma.div_euclid(256)).clamp(16, 235) as u8
}

/// Croma de uma cor, em BT.709 de faixa de TV, na ordem (U, V).
fn croma_de((vermelho, verde, azul): (u32, u32, u32)) -> (u8, u8) {
    let (vermelho, verde, azul) = (vermelho as i32, verde as i32, azul as i32);
    let u = 128 + (-25 * vermelho - 87 * verde + 112 * azul).div_euclid(256);
    let v = 128 + (112 * vermelho - 102 * verde - 10 * azul).div_euclid(256);
    (u.clamp(16, 240) as u8, v.clamp(16, 240) as u8)
}

#[cfg(test)]
mod testes {
    use super::*;

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
        let mapa = Mapa::novo((1024, 768), Resolucao::P720);
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
        let mapa = Mapa::novo((16, 16), Resolucao::P540);
        let quadro = mapa
            .converter(&origem(16, 16, 16 * 4, (0x00, 0x00, 0xFF)), 16 * 4)
            .expect("os planos fecham");

        let (u, v) = croma_em(&quadro, 240, 135);
        assert!((100..=106).contains(&u), "U de vermelho puro veio {u}");
        assert!((235..=240).contains(&v), "V de vermelho puro veio {v}");
        assert_eq!(luma_em(&quadro, 480, 270), 62);
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
        // Metade branca e metade preta reduzidas a um pixel dão cinza. Com
        // vizinho mais próximo dariam uma das duas pontas — que é como texto
        // reduzido perde linhas de pixel inteiras.
        let mut bytes = origem(2, 1, 8, (0x00, 0x00, 0x00));
        if let Some([azul, verde, vermelho, _alfa]) = bytes.get_mut(4..8) {
            *azul = 0xFF;
            *verde = 0xFF;
            *vermelho = 0xFF;
        }

        let cor = media(&bytes, 8, 0, 2, 0, 1);
        assert_eq!(cor, (127, 127, 127));
    }
}
