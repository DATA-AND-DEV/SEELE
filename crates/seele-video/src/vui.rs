//! Escreve a descrição de cor no SPS, para quem recebe não ter de adivinhar.
//!
//! # O defeito que isto conserta
//!
//! O `EncoderConfig` do `shiguredo_openh264` não tem campo de cor, então o SPS
//! que o OpenH264 emite sai com `vui_parameters_present_flag = 0`: **sem
//! descrição de cor nenhuma**.
//!
//! Quando ela falta, um decodificador aplica a regra de sempre — BT.601 até 576
//! linhas, BT.709 acima. E as três resoluções que o §5 oferece são 540p, 720p e
//! 1080p: **540p tem 540 linhas, abaixo de 576**. A captura converte em BT.709
//! nas três (ver `captura::windows::luma_de`, e o `420v` do macOS), então a
//! 540p o outro lado adivinha 601 sobre um quadro 709 e **a cor desloca**.
//!
//! Foi medido em campo entre um Mac e um Windows numa LAN: «mudança de
//! tonalidade de cor durante jogos» — que é a resolução caindo para 540p no meio
//! da partida, e a cor mudando com ela.
//!
//! O doc de `captura::windows::luma_de` já apontava o conserto e o lugar dele:
//! *«o codec não escreve VUI nenhum hoje — está no relatório, porque quem
//! conserta isso é `codec.rs`, e enquanto não escrever, quem recebe adivinha»*.
//! Este módulo é aquele conserto.
//!
//! # Por que na saída do encoder, e não na configuração
//!
//! Porque o binding não expõe. As alternativas eram esperar o upstream, trocar
//! de binding — o que reabriria o §2 inteiro, incluindo a questão de patente que
//! ele resolveu —, ou escrever os bits. Escrever os bits é a única que não
//! desfaz uma decisão tomada com medida.
//!
//! # O que se escreve, e nada além
//!
//! Só `video_signal_type`, com `colour_description`:
//!
//! - `video_format = 5` — não especificado. Não é tela de TV nem de câmera;
//! - `video_full_range_flag = 0` — faixa de TV, 16–235. É o que a captura
//!   produz nos dois sistemas, e o que `QuadroI420::preto` assume ao pintar 16;
//! - `colour_primaries`, `transfer_characteristics`, `matrix_coefficients` = 1,
//!   que é BT.709 nos três.
//!
//! Nada de `timing_info`, `hrd_parameters` ou `bitstream_restriction`. Cada
//! campo a mais é um campo a errar, e nenhum deles conserta cor.

/// Lê bits de um RBSP, do mais significativo para o menos.
struct LeitorDeBits<'a> {
    bytes: &'a [u8],
    posicao: usize,
}

impl<'a> LeitorDeBits<'a> {
    const fn novo(bytes: &'a [u8]) -> Self {
        Self { bytes, posicao: 0 }
    }

    /// Um bit, ou `None` quando o buffer acabou.
    fn bit(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.posicao / 8)?;
        let deslocamento = 7 - (self.posicao % 8);
        self.posicao += 1;
        Some((byte >> deslocamento) & 1)
    }

    /// `n` bits, como número.
    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut valor = 0_u32;
        for _ in 0..n {
            valor = (valor << 1) | u32::from(self.bit()?);
        }
        Some(valor)
    }

    /// Um `ue(v)` — Exp-Golomb sem sinal, que é como o H.264 escreve quase todo
    /// número de cabeçalho.
    ///
    /// Conta os zeros até o primeiro um, lê essa quantidade de bits, e soma.
    /// O teto de 32 zeros não é elegância: sem ele, um SPS corrompido faz este
    /// laço andar até o fim do buffer por um bit trocado.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0_u32;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 32 {
                return None;
            }
        }
        if zeros == 0 {
            return Some(0);
        }
        let resto = self.bits(zeros)?;
        Some((1_u32 << zeros) - 1 + resto)
    }

    /// Um `se(v)` — Exp-Golomb com sinal.
    fn se(&mut self) -> Option<i32> {
        let k = self.ue()?;
        let magnitude = i64::from(k).div_euclid(2) + i64::from(k) % 2;
        Some(if k % 2 == 1 {
            i32::try_from(magnitude).ok()?
        } else {
            i32::try_from(-magnitude).ok()?
        })
    }

    const fn posicao(&self) -> usize {
        self.posicao
    }
}

/// Escreve bits num RBSP, do mais significativo para o menos.
#[derive(Default)]
struct EscritorDeBits {
    bytes: Vec<u8>,
    bits_no_ultimo: u32,
}

impl EscritorDeBits {
    fn bit(&mut self, valor: u8) {
        if self.bits_no_ultimo == 0 {
            self.bytes.push(0);
            self.bits_no_ultimo = 8;
        }
        if let Some(ultimo) = self.bytes.last_mut() {
            *ultimo |= (valor & 1) << (self.bits_no_ultimo - 1);
        }
        self.bits_no_ultimo -= 1;
    }

    fn bits(&mut self, valor: u32, n: u32) {
        for i in (0..n).rev() {
            self.bit(((valor >> i) & 1) as u8);
        }
    }

    /// Copia `quantos` bits do começo de `origem`.
    fn copiar(&mut self, origem: &[u8], quantos: usize) {
        let mut leitor = LeitorDeBits::novo(origem);
        for _ in 0..quantos {
            match leitor.bit() {
                Some(bit) => self.bit(bit),
                None => return,
            }
        }
    }

    /// Copia os bits de `origem` no intervalo `[de, ate)`.
    fn copiar_de(&mut self, origem: &[u8], de: usize, ate: usize) {
        let mut leitor = LeitorDeBits::novo(origem);
        for _ in 0..de {
            if leitor.bit().is_none() {
                return;
            }
        }
        for _ in de..ate {
            match leitor.bit() {
                Some(bit) => self.bit(bit),
                None => return,
            }
        }
    }

    /// `rbsp_trailing_bits()`: um um, e zeros até fechar o byte.
    fn fechar(mut self) -> Vec<u8> {
        self.bit(1);
        while self.bits_no_ultimo != 0 {
            self.bit(0);
        }
        self.bytes
    }
}

/// Tira os bytes de prevenção de emulação de um NAL.
///
/// O H.264 proíbe `00 00 00`, `00 00 01`, `00 00 02` e `00 00 03` dentro de um
/// NAL, porque os dois primeiros seriam código de início. O encoder insere um
/// `03` depois de dois zeros; quem lê os bits tem de tirá-lo antes, ou o `03`
/// entra na conta como se fosse conteúdo.
fn desescapar(nal: &[u8]) -> Vec<u8> {
    let mut fora = Vec::with_capacity(nal.len());
    let mut zeros = 0_usize;
    for &byte in nal {
        if zeros == 2 && byte == 0x03 {
            zeros = 0;
            continue;
        }
        if byte == 0 {
            zeros += 1;
        } else {
            zeros = 0;
        }
        fora.push(byte);
    }
    fora
}

/// Põe os bytes de prevenção de emulação de volta.
fn escapar(rbsp: &[u8]) -> Vec<u8> {
    let mut fora = Vec::with_capacity(rbsp.len() + 8);
    let mut zeros = 0_usize;
    for &byte in rbsp {
        if zeros == 2 && byte <= 0x03 {
            fora.push(0x03);
            zeros = 0;
        }
        if byte == 0 {
            zeros += 1;
        } else {
            zeros = 0;
        }
        fora.push(byte);
    }
    fora
}

/// Os perfis cujo SPS carrega os campos extras de croma (`chroma_format_idc` e
/// companhia).
///
/// O OpenH264 não emite nenhum deles hoje — ele fica em Constrained Baseline —,
/// e a lista está aqui mesmo assim: um SPS de outro perfil que caísse neste
/// caminho sem ela seria lido com os campos deslocados, e o resultado é um SPS
/// reescrito errado em vez de um SPS não reconhecido.
const PERFIS_COM_CROMA: [u32; 13] = [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135];

/// Onde termina, em bits, tudo que vem antes do `vui_parameters_present_flag`.
///
/// `None` quando o SPS não é reconhecido — e aí quem chama **não mexe nele**.
/// Um SPS que não se entende é um SPS que não se reescreve.
fn ate_o_vui(rbsp: &[u8]) -> Option<Alvo> {
    // O cabeçalho do NAL, um byte, já foi tirado por quem chama.
    let mut leitor = LeitorDeBits::novo(rbsp);
    let perfil = leitor.bits(8)?;
    let _restricoes_e_reservado = leitor.bits(8)?;
    let _nivel = leitor.bits(8)?;
    let _id = leitor.ue()?;

    if PERFIS_COM_CROMA.contains(&perfil) {
        let croma = leitor.ue()?;
        if croma == 3 {
            let _separado = leitor.bit()?;
        }
        let _bits_luma = leitor.ue()?;
        let _bits_croma = leitor.ue()?;
        let _transformada = leitor.bit()?;
        let usa_listas = leitor.bit()?;
        if usa_listas == 1 {
            // As listas de escalonamento. Não são emitidas por este encoder, e
            // pular uma delas errado desloca todo o resto.
            let quantas = if croma == 3 { 12 } else { 8 };
            for i in 0..quantas {
                if leitor.bit()? == 1 {
                    let tamanho = if i < 6 { 16 } else { 64 };
                    let mut ultimo = 8_i32;
                    let mut proximo = 8_i32;
                    for _ in 0..tamanho {
                        if proximo != 0 {
                            let delta = leitor.se()?;
                            proximo = (ultimo + delta + 256) % 256;
                        }
                        ultimo = if proximo == 0 { ultimo } else { proximo };
                    }
                }
            }
        }
    }

    let _log2_frame_num = leitor.ue()?;
    let ordem = leitor.ue()?;
    if ordem == 0 {
        let _log2_poc = leitor.ue()?;
    } else if ordem == 1 {
        let _delta_sempre_zero = leitor.bit()?;
        let _offset_nao_ref = leitor.se()?;
        let _offset_topo = leitor.se()?;
        let quantos = leitor.ue()?;
        for _ in 0..quantos.min(256) {
            let _offset = leitor.se()?;
        }
    }
    let _max_refs = leitor.ue()?;
    let _lacunas = leitor.bit()?;
    let _largura = leitor.ue()?;
    let _altura = leitor.ue()?;
    let so_quadros = leitor.bit()?;
    if so_quadros == 0 {
        let _adaptativo = leitor.bit()?;
    }
    let _inferencia = leitor.bit()?;
    let corte = leitor.bit()?;
    if corte == 1 {
        for _ in 0..4 {
            let _borda = leitor.ue()?;
        }
    }
    let antes_do_vui = leitor.posicao();
    if leitor.bit()? == 0 {
        // Sem VUI nenhuma: escreve-se uma inteira a partir daqui.
        return Some(Alvo::SemVui { bit: antes_do_vui });
    }

    // **Tem VUI, e isso não quer dizer que tem cor.** O OpenH264 escreve uma
    // VUI com `timing_info` e deixa `video_signal_type_present_flag` em zero —
    // conferido no SPS de verdade que está nos testes. Recusar aqui, como esta
    // função fazia na primeira redação, era desistir justamente no caso real.
    let aspecto = leitor.bit()?;
    if aspecto == 1 {
        let idc = leitor.bits(8)?;
        if idc == 255 {
            let _largura = leitor.bits(16)?;
            let _altura = leitor.bits(16)?;
        }
    }
    let overscan = leitor.bit()?;
    if overscan == 1 {
        let _apropriado = leitor.bit()?;
    }
    let antes_do_sinal = leitor.posicao();
    if leitor.bit()? == 1 {
        // Já declarada. Não se sobrescreve a escolha de quem já escolheu.
        return Some(Alvo::JaTemCor);
    }
    Some(Alvo::SemCor {
        bit: antes_do_sinal,
    })
}

/// O que se achou dentro do SPS, e o que fazer com ele.
#[derive(Debug, PartialEq, Eq)]
enum Alvo {
    /// Não há VUI. Escreve-se uma inteira a partir deste bit.
    SemVui { bit: usize },
    /// Há VUI, e `video_signal_type_present_flag` está neste bit, em zero.
    SemCor { bit: usize },
    /// Há VUI e a cor já está declarada.
    JaTemCor,
}

/// A posição do bit de parada do RBSP — o último `1`.
///
/// `rbsp_trailing_bits()` é um `1` seguido de zeros até fechar o byte, então o
/// último bit ligado do buffer **é** aquele `1`. Tudo antes dele é conteúdo, e é
/// o que precisa ser copiado quando se insere um campo no meio.
fn bit_de_parada(rbsp: &[u8]) -> Option<usize> {
    for (i, byte) in rbsp.iter().enumerate().rev() {
        if *byte != 0 {
            let ultimo = 7 - byte.trailing_zeros() as usize;
            return Some(i * 8 + ultimo);
        }
    }
    None
}

/// O corpo de `video_signal_type`, **sem** o sinalizador que o precede.
///
/// Separado porque ele é escrito em dois contextos: numa VUI nova, logo depois
/// de dois zeros; e no meio de uma VUI que já existe, no lugar do zero que
/// estava lá.
fn escrever_video_signal_type(escritor: &mut EscritorDeBits) {
    escritor.bits(5, 3); // video_format: não especificado
    escritor.bit(0); // video_full_range_flag: faixa de TV
    escritor.bit(1); // colour_description_present_flag
    escritor.bits(1, 8); // colour_primaries: BT.709
    escritor.bits(1, 8); // transfer_characteristics: BT.709
    escritor.bits(1, 8); // matrix_coefficients: BT.709
}

/// O resto de uma VUI nova, tudo em zero.
fn escrever_resto_vazio(escritor: &mut EscritorDeBits) {
    escritor.bit(0); // chroma_loc_info_present_flag
    escritor.bit(0); // timing_info_present_flag
    escritor.bit(0); // nal_hrd_parameters_present_flag
    escritor.bit(0); // vcl_hrd_parameters_present_flag
    escritor.bit(0); // pic_struct_present_flag
    escritor.bit(0); // bitstream_restriction_flag
}

/// Reescreve um NAL de SPS com a descrição de cor.
///
/// Devolve `None` — e quem chama fica com o original — quando o SPS não é
/// reconhecido ou **já tem** VUI. Não se reescreve o que já foi escrito: um
/// encoder que passe a emitir VUI sozinho sobrescreveria a escolha dele por
/// esta, sem que ninguém pedisse.
fn com_cor(sps: &[u8]) -> Option<Vec<u8>> {
    let cabecalho = *sps.first()?;
    let rbsp = desescapar(sps.get(1..)?);
    let parada = bit_de_parada(&rbsp)?;
    let mut escritor = EscritorDeBits::default();

    match ate_o_vui(&rbsp)? {
        Alvo::JaTemCor => return None,
        Alvo::SemVui { bit } => {
            // Nada depois: a VUI é o último campo do SPS, então o que vinha
            // adiante era só o bit de parada.
            escritor.copiar(&rbsp, bit);
            escritor.bit(1); // vui_parameters_present_flag
            escrever_video_signal_type(&mut escritor);
            escrever_resto_vazio(&mut escritor);
        }
        Alvo::SemCor { bit } => {
            // Aqui há VUI depois do campo, e ela tem de sobreviver inteira: o
            // `timing_info` que o OpenH264 escreve mora ali. Copia-se até o
            // campo, escreve-se ele, e copia-se o resto **bit a bit** — os
            // campos seguintes deslocam, e é por isso que isto não pode ser
            // feito em bytes.
            escritor.copiar(&rbsp, bit);
            escritor.bit(1); // video_signal_type_present_flag
            escrever_video_signal_type(&mut escritor);
            escritor.copiar_de(&rbsp, bit + 1, parada);
        }
    }
    let novo_rbsp = escritor.fechar();

    let mut fora = Vec::with_capacity(novo_rbsp.len() + 8);
    fora.push(cabecalho);
    fora.extend_from_slice(&escapar(&novo_rbsp));
    Some(fora)
}

/// O tipo de NAL, dos cinco bits baixos do cabeçalho.
const fn tipo(cabecalho: u8) -> u8 {
    cabecalho & 0x1F
}

/// Quantos bytes tem o código de início que começa em `posicao`, ou zero.
fn codigo_de_inicio(bytes: &[u8], posicao: usize) -> usize {
    if bytes.get(posicao..posicao + 4) == Some(&[0, 0, 0, 1]) {
        return 4;
    }
    if bytes.get(posicao..posicao + 3) == Some(&[0, 0, 1]) {
        return 3;
    }
    0
}

/// Acrescenta a descrição de cor a todo SPS deste fluxo Annex B.
///
/// Devolve o fluxo inalterado quando não há SPS, quando ele já tem VUI, ou
/// quando não foi reconhecido. **Nunca devolve um fluxo pela metade**: ou a
/// reescrita inteira deu certo, ou nada é trocado.
///
/// Roda só em quadro-chave, que é onde o SPS viaja, e quadro-chave aqui é sob
/// demanda (§3.3) — então isto não é custo por quadro.
#[must_use]
pub fn com_descricao_de_cor(fluxo: &[u8]) -> Vec<u8> {
    let mut fora = Vec::with_capacity(fluxo.len() + 16);
    let mut posicao = 0_usize;
    let mut mexeu = false;

    while posicao < fluxo.len() {
        let tamanho_do_inicio = codigo_de_inicio(fluxo, posicao);
        if tamanho_do_inicio == 0 {
            // Fora de um NAL — não deveria acontecer num fluxo bem formado, e
            // se acontecer o byte é copiado sem interpretação.
            if let Some(byte) = fluxo.get(posicao) {
                fora.push(*byte);
            }
            posicao += 1;
            continue;
        }

        let comeco = posicao + tamanho_do_inicio;
        let mut fim = comeco;
        while fim < fluxo.len() && codigo_de_inicio(fluxo, fim) == 0 {
            fim += 1;
        }
        let Some(nal) = fluxo.get(comeco..fim) else {
            break;
        };
        if let Some(inicio) = fluxo.get(posicao..comeco) {
            fora.extend_from_slice(inicio);
        }

        let e_sps = nal.first().copied().is_some_and(|c| tipo(c) == 7);
        match e_sps.then(|| com_cor(nal)).flatten() {
            Some(reescrito) => {
                fora.extend_from_slice(&reescrito);
                mexeu = true;
            }
            None => fora.extend_from_slice(nal),
        }
        posicao = fim;
    }

    if mexeu {
        fora
    } else {
        fluxo.to_vec()
    }
}

#[cfg(test)]
mod testes {
    use super::{
        ate_o_vui, com_descricao_de_cor, desescapar, escapar, EscritorDeBits, LeitorDeBits,
    };

    #[test]
    fn o_leitor_le_exp_golomb_como_a_norma_manda() {
        // Os primeiros valores da tabela, concatenados:
        // `1` → 0, `010` → 1, `011` → 2, `00100` → 3.
        let mut leitor = LeitorDeBits::novo(&[0b1010_0110, 0b0100_0000]);
        assert_eq!(leitor.ue(), Some(0));
        assert_eq!(leitor.ue(), Some(1));
        assert_eq!(leitor.ue(), Some(2));
        assert_eq!(leitor.ue(), Some(3));
    }

    #[test]
    fn o_escritor_e_o_leitor_fecham_um_no_outro() {
        let mut escritor = EscritorDeBits::default();
        escritor.bits(0b101, 3);
        escritor.bits(0xFF, 8);
        escritor.bit(0);
        let bytes = escritor.fechar();

        let mut leitor = LeitorDeBits::novo(&bytes);
        assert_eq!(leitor.bits(3), Some(0b101));
        assert_eq!(leitor.bits(8), Some(0xFF));
        assert_eq!(leitor.bit(), Some(0));
    }

    #[test]
    fn escapar_e_desescapar_sao_inversos() {
        for bruto in [
            vec![0, 0, 0, 1, 2, 3],
            vec![0, 0, 1],
            vec![0, 0, 2, 0, 0, 3],
            vec![1, 2, 3, 4],
            vec![0, 0, 0, 0, 0, 0],
        ] {
            let escapado = escapar(&bruto);
            assert_eq!(desescapar(&escapado), bruto, "não fechou para {bruto:?}");
            // E o escapado não contém sequência proibida.
            for janela in escapado.windows(3) {
                assert!(
                    janela != [0, 0, 0] && janela != [0, 0, 1] && janela != [0, 0, 2],
                    "sobrou sequência proibida em {escapado:?}"
                );
            }
        }
    }

    /// Um SPS que não se entende **não** é reescrito.
    ///
    /// É a regra que separa este módulo de um gerador de fluxo corrompido: sem
    /// ela, um SPS de um perfil que o leitor não acompanha sairia daqui com os
    /// bits deslocados, e o defeito apareceria como imagem quebrada em vez de
    /// como recusa.
    #[test]
    fn um_sps_que_nao_se_entende_sai_intacto() {
        let lixo = [0x00, 0x00, 0x00, 0x01, 0x67, 0xFF];
        assert_eq!(com_descricao_de_cor(&lixo), lixo.to_vec());
    }

    #[test]
    fn um_fluxo_sem_sps_sai_intacto() {
        // NAL de tipo 1 (fatia não-IDR), que não é SPS.
        let fluxo = [0x00, 0x00, 0x00, 0x01, 0x41, 0x9A, 0x12, 0x34];
        assert_eq!(com_descricao_de_cor(&fluxo), fluxo.to_vec());
    }

    /// **A prova de que o SPS reescrito é válido não mora aqui, e é de
    /// propósito.**
    ///
    /// Montar um SPS à mão para este teste provaria que o código aceita o que
    /// este arquivo mesmo escreveu — o que é circular. A prova é a ida e volta
    /// por um encoder e um decodificador de verdade, e ela está em
    /// `examples/cor.rs`, que precisa do módulo do Cisco e por isso não pode
    /// ser exigida de `cargo test`.
    ///
    /// O que se cobra aqui é o contrato que se observa sem encoder: **o que não
    /// se entende não se mexe**, e o que não é SPS não se toca. É o que separa
    /// este módulo de um gerador de fluxo corrompido.
    /// O leitor para num SPS truncado em vez de andar até o fim do buffer.
    #[test]
    fn um_sps_truncado_nao_trava_o_leitor() {
        assert_eq!(ate_o_vui(&[0x42]), None);
        assert_eq!(ate_o_vui(&[]), None);
        // Zeros infinitos: o `ue` tem teto de trinta e dois.
        assert_eq!(ate_o_vui(&[0; 64]), None);
    }
}

#[cfg(test)]
mod sps_de_verdade {
    #![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

    use super::{ate_o_vui, com_descricao_de_cor, desescapar};

    /// Um SPS que o OpenH264 emitiu de verdade, capturado em 2026-08-31.
    ///
    /// 540p, CABAC ligado. O primeiro byte é o cabeçalho do NAL (`0x67`, tipo 7)
    /// e o resto é o RBSP. `0x64` é `profile_idc = 100` — **High** —, e não
    /// baseline: ligar CABAC fez o OpenH264 declarar High no SPS, ainda que a
    /// capacidade real fique em Constrained Baseline + CABAC.
    ///
    /// Está aqui como vetor porque foi ele que expôs o defeito: o leitor entrava
    /// no ramo de croma e não saía.
    const SPS_540P: [u8; 16] = [
        0x67, 0x64, 0x0C, 0x1F, 0xAC, 0x18, 0xD0, 0x1E, 0x02, 0x2F, 0xDC, 0x03, 0xC2, 0x21, 0x1A,
        0x80,
    ];

    #[test]
    fn o_sps_de_verdade_e_reconhecido() {
        let rbsp = desescapar(&SPS_540P[1..]);
        let lido = ate_o_vui(&rbsp);
        assert!(
            lido.is_some(),
            "o leitor não reconheceu um SPS que o encoder emitiu de verdade"
        );
        // **Ele tem VUI e não tem cor**, e essa combinação é a razão de este
        // módulo existir: o OpenH264 escreve `timing_info` e deixa
        // `video_signal_type_present_flag` em zero. A primeira redação recusava
        // todo SPS que já tivesse VUI, e desistia justamente no caso real.
        assert!(
            matches!(lido, Some(super::Alvo::SemCor { .. })),
            "esperava uma VUI sem cor e achei {lido:?}"
        );
    }

    #[test]
    fn o_sps_de_verdade_ganha_a_cor() {
        let mut annexb = vec![0, 0, 0, 1];
        annexb.extend_from_slice(&SPS_540P);
        let saida = com_descricao_de_cor(&annexb);
        assert_ne!(saida, annexb, "o SPS saiu intacto: a cor não foi escrita");

        // **Não se procura os três bytes de BT.709 crus**: os campos caem em
        // posição arbitrária de bit, e um `windows(3)` sobre bytes só os acharia
        // por acidente de alinhamento. Quem responde é o leitor, relendo o que
        // acabou de ser escrito.
        let relido = desescapar(saida.get(5..).expect("o SPS reescrito tem corpo"));
        assert_eq!(
            ate_o_vui(&relido),
            Some(super::Alvo::JaTemCor),
            "o SPS reescrito não declara a cor"
        );

        // E reescrever de novo não mexe: quem já tem cor não é sobrescrito.
        assert_eq!(
            com_descricao_de_cor(&saida),
            saida,
            "a segunda passada mexeu num SPS que já tinha cor"
        );
    }
}
