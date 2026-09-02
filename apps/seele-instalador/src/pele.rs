//! A pele: cores, medidas e as faces embarcadas.
//!
//! **Compila nos dois sistemas, e fora do Windows ninguém a consome** — a janela
//! é `cfg(windows)`. O `allow` abaixo é para isso e só para isso.
//!
//! Poderia sumir junto com a janela, e não some de propósito: o teste que
//! compara estas cores com o `tokens.css` do produto é o que impede a primeira
//! tela do SEELE de ficar pintada com a cor de ontem, e ele precisa rodar onde a
//! bateria roda primeiro, que é o Mac. Um módulo que só existe no Windows é um
//! guarda que ninguém vê até o fim da corrida.
#![cfg_attr(not(windows), allow(dead_code))]
//!
//! Os valores vêm de `apps/seele-app/ui/tokens.css` e do desenho `Instalador
//! SEELE.dc.html`. Estão repetidos aqui e não lidos de lá de propósito: o CSS é
//! do produto e este binário não o lê em tempo nenhum — mas um teste compara os
//! dois, para a repetição não virar divergência.

/// Uma cor como o GDI a quer: `0x00BBGGRR`.
///
/// **Ao contrário do CSS e do `SetCtlColors`**, que falam em `RRGGBB`. Trocar a
/// ordem não dá erro nenhum: sai um azul onde devia sair laranja, e só quem olha
/// a tela descobre.
pub(crate) const fn cor(rgb: u32) -> u32 {
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    (b << 16) | (g << 8) | r
}

/// `--seele-negro-absoluto`: o fundo, e nunca um cinza-carvão neutro.
pub(crate) const NEGRO: u32 = cor(0x05_04_03);
/// `--seele-negro-painel`: a lombada, a barra de título e o rodapé.
pub(crate) const PAINEL: u32 = cor(0x0A_08_06);
/// `--seele-linha`: a borda de 1px que separa as faixas.
pub(crate) const LINHA: u32 = cor(0x24_1F_19);
/// `--seele-linha-forte`: a borda de um campo, e a moldura da janela.
pub(crate) const LINHA_FORTE: u32 = cor(0x3A_32_2A);
/// `--seele-osso`: texto corrido.
pub(crate) const OSSO: u32 = cor(0xEA_E3_CF);
/// `--seele-rotulo-painel`: rótulo miúdo, 9–10px, em caixa alta.
pub(crate) const ROTULO: u32 = cor(0x90_85_74);
/// `--seele-laranja-nerv`: o acento, e o passo em que se está.
pub(crate) const LARANJA: u32 = cor(0xF2_52_1F);
// `--seele-fosforo` entra junto com o log do passo 03, e não antes: a bateria
// roda o clippy com `-D warnings`, e uma constante sem uso reprova o release.

/// A janela do desenho, em pixels de 96 dpi. Escalada pelo dpi da tela.
pub(crate) const LARGURA: i32 = 680;
/// Barra de título 32 + corpo 392 + rodapé 56.
pub(crate) const ALTURA: i32 = 480;
/// A barra de título própria — a que o NSIS não sabia desenhar.
pub(crate) const BARRA: i32 = 32;
/// A lombada, com a marca e o que o SEELE é.
pub(crate) const LOMBADA: i32 = 188;
/// O rodapé, com o passo escrito e os dois botões.
pub(crate) const RODAPE: i32 = 56;

/// As faces, embutidas no binário.
///
/// Carregadas na memória com `AddFontMemResourceEx` e nunca instaladas: quem
/// roda o instalador não fica com fonte nova no sistema, e quem cancela no
/// primeiro passo não deixa rastro nenhum. Ver `fontes/PROCEDENCIA.md`.
pub(crate) const SAIRA_700: &[u8] = include_bytes!("../fontes/saira-condensed-700.ttf");
/// A condensada de peso médio, para rótulo em caixa alta.
pub(crate) const SAIRA_500: &[u8] = include_bytes!("../fontes/saira-condensed-500.ttf");
/// A monoespaçada, para todo o resto — prosa, caminho, medida.
pub(crate) const PLEX_400: &[u8] = include_bytes!("../fontes/ibm-plex-mono-400.otf");

/// A marca, já desenhada, para a lombada. Ver `empacotar/marca-do-instalador.py`.
pub(crate) const MARCA: &[u8] = include_bytes!("../../seele-app/marca-instalador.bmp");

#[cfg(test)]
mod testes {
    use super::{cor, LARANJA, NEGRO, OSSO};

    #[test]
    fn a_ordem_dos_canais_e_a_do_gdi_e_nao_a_do_css() {
        // O laranja do produto é `#F2521F`: muito vermelho, médio verde, pouco
        // azul. No formato do GDI ele tem de sair com o vermelho no byte baixo.
        //
        // Sem este teste, trocar a ordem é um defeito que compila, roda, e só
        // aparece para quem olha a tela — num azul onde devia haver laranja.
        assert_eq!(LARANJA, 0x00_1F_52_F2);
        assert_eq!(cor(0xFF_00_00), 0x00_00_00_FF, "vermelho puro");
        assert_eq!(cor(0x00_00_FF), 0x00_FF_00_00, "azul puro");
        assert_eq!(NEGRO, 0x00_03_04_05);
        assert_eq!(OSSO, 0x00_CF_E3_EA);
    }

    #[test]
    fn as_cores_daqui_sao_as_do_produto() {
        // A pele repete os tokens em vez de ler o CSS — este binário não lê CSS
        // em tempo nenhum. Repetição sem conferência é divergência esperando
        // acontecer: o produto muda de laranja e o instalador continua no
        // antigo, na única tela que aparece antes de o produto existir.
        let caminho =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../seele-app/ui/tokens.css");
        let Ok(css) = std::fs::read_to_string(&caminho) else {
            panic!("não li {}", caminho.display());
        };

        for (nome, valor) in [
            ("--seele-negro-absoluto", "#050403"),
            ("--seele-negro-painel", "#0A0806"),
            ("--seele-linha", "#241F19"),
            ("--seele-linha-forte", "#3A322A"),
            ("--seele-osso", "#EAE3CF"),
            ("--seele-rotulo-painel", "#908574"),
            ("--seele-laranja-nerv", "#F2521F"),
        ] {
            // A linha do token, e não uma busca por `nome: valor` grudados: o
            // `tokens.css` alinha os valores em coluna com vários espaços, e um
            // guarda preso ao espaçamento reprova quando alguém alinha a tabela.
            let declarada = css.lines().find(|linha| {
                linha
                    .trim_start()
                    .strip_prefix(nome)
                    .and_then(|resto| resto.trim_start().strip_prefix(':'))
                    .is_some_and(|resto| resto.trim_start().starts_with(valor))
            });
            assert!(
                declarada.is_some(),
                "o `tokens.css` não diz mais `{nome}: {valor}`.\n\
                 A pele do instalador copia esse valor, e as duas divergirem \
                 pinta a primeira tela do produto com a cor de ontem."
            );
        }
    }
}
