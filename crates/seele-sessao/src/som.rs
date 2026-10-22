//! **O piso do que um som de MOD ocupa decodificado** (P49-7a), para o
//! desktop e o navegador recusarem pelo mesmo número o som que a janela não
//! teria onde guardar (Tarefa 2.6 da F4). Saiu do `main.rs` do app; o
//! `seele-core` e o `seele-ffi` o reexportam.
//!
//! O guarda `os_numeros_do_som_decodificado_sao_os_da_janela`, nos testes do
//! `main.rs` do app, confere que os números daqui são os de `mods-regiao.js`.

/// **O teto do som decodificado, em bytes** — o mesmo da janela.
///
/// A janela soma, por lugar, o que o WebAudio segura, e recusa o som que passa
/// dele (`LIMITES_DA_REGIAO.bytesDeSomDecodificado` e o do cartão, em
/// `mods-regiao.js`). Aqui ele vale para um som sozinho: um som cujo piso
/// decodificado passa deste número não cabe em lugar nenhum, e a janela o
/// decodificaria inteiro para largá-lo em seguida. O guarda
/// `os_numeros_do_som_decodificado_sao_os_da_janela` confere que os dois
/// lados dizem o mesmo número.
pub const TETO_DO_SOM_DECODIFICADO: u64 = 64 * 1024 * 1024;

/// **A taxa em que a janela decodifica o som de MOD**, em quadros por
/// segundo — `TAXA_DA_DECODIFICACAO`, em `mods-regiao.js`, conferida pelo
/// mesmo guarda do teto.
pub const TAXA_DA_DECODIFICACAO: u64 = 48_000;

/// O nome da recusa do som que, decodificado, passa do teto.
///
/// É o começo do `motivo`, e os números vêm depois dele: a janela reconhece a
/// recusa por este nome e a diz como o teto do som, e não como falha de carga
/// (`recusaDoSomGrandeDemais`, em `mods-regiao.js`).
pub const SOM_GRANDE_DEMAIS_DECODIFICADO: &str = "som-grande-demais-decodificado";

/// **O som que, decodificado, não cabe no teto do som, recusado antes de a
/// janela o decodificar** (P49-7a).
///
/// `decodeAudioData` devolve o som inteiro em float32, de uma vez: a janela só
/// pode contá-lo depois de ele existir, e o pico — dez MiB de MP3 a 128 kbps
/// viram uns 250 MB — já aconteceu. Quando o cabeçalho permite, o piso
/// ([`bytes_decodificados_minimos`]) sai antes de os bytes saírem daqui, e o
/// som acima do teto é recusado com o nome
/// [`SOM_GRANDE_DEMAIS_DECODIFICADO`] e os números.
///
/// **Só o piso recusa.** Um som cujo cabeçalho não diz o bastante segue como
/// antes: a janela decodifica e confere o teto dela.
///
/// # Errors
///
/// O motivo, que começa por [`SOM_GRANDE_DEMAIS_DECODIFICADO`] e diz os
/// números: cada casca o põe na recusa dela.
pub fn som_cabe_decodificado(bytes: &[u8]) -> Result<(), String> {
    match bytes_decodificados_minimos(bytes) {
        Some(piso) if piso > TETO_DO_SOM_DECODIFICADO => Err(format!(
            "{SOM_GRANDE_DEMAIS_DECODIFICADO}: decodificado a {TAXA_DA_DECODIFICACAO} Hz, o som \
             ocupa ao menos {piso} bytes, acima do teto de {TETO_DO_SOM_DECODIFICADO} bytes de som"
        )),
        _ => Ok(()),
    }
}

/// **Um piso do que um som ocupa decodificado pela janela**, em bytes: os
/// quadros a [`TAXA_DA_DECODIFICACAO`], vezes os canais, vezes quatro — o
/// float32 que o `AudioBuffer` segura.
///
/// **Um piso, e nunca uma estimativa que possa passar do real** num arquivo
/// bem formado: só ele recusa ([`som_cabe_decodificado`]). Um MP3 com mais
/// quadros corrompidos do que o corte absorve é o caso em que ele passa — ver
/// [`cabecalho_de_som::mp3`]. O cabeçalho é lido à mão, sem crate de áudio
/// ([`cabecalho_de_som`]):
///
/// - **WAV**, exato, pelo `fmt ` e pelo `data`, com o `data` limitado aos
///   bytes que estão no arquivo e ao tamanho que o RIFF declara;
/// - **Ogg**, pelas páginas do fluxo Vorbis ou Opus: a granule da última,
///   menos a da primeira de dados — e, no Opus, que já conta a 48 kHz, menos o
///   pre-skip;
/// - **MP3**, pelos quadros que estão no arquivo, encadeados desde o primeiro,
///   e não mais do que o quadro Xing/Info conta quando há um.
///
/// Um quadro sai da conta por arredondamento: a conversão de taxa do
/// navegador conta em ponto flutuante, e pode ficar um quadro abaixo do
/// quociente exato.
///
/// `None` quando o cabeçalho não diz o bastante: um WAV comprimido, um Ogg de
/// outro codec, um MP3 de formato livre, um arquivo cortado antes do que a
/// leitura precisa, uma taxa ou um número de canais fora do que o WebAudio
/// decodifica. O som segue como antes, para a janela decodificar e conferir.
pub fn bytes_decodificados_minimos(bytes: &[u8]) -> Option<u64> {
    let piso = match crate::mods::ler_tipo(bytes)?.media_type {
        "audio/wav" => cabecalho_de_som::wav(bytes)?,
        "audio/ogg" => cabecalho_de_som::ogg(bytes)?,
        "audio/mpeg" => cabecalho_de_som::mp3(bytes)?,
        _ => return None,
    };
    // A faixa que o WebAudio aceita num `AudioBuffer`: fora dela, a janela
    // não decodifica, e o piso não teria do que ser piso.
    if !(3_000..=768_000).contains(&piso.taxa) || !(1..=32).contains(&piso.canais) {
        return None;
    }
    let quadros =
        u128::from(piso.amostras) * u128::from(TAXA_DA_DECODIFICACAO) / u128::from(piso.taxa);
    let bytes = quadros.saturating_sub(1) * u128::from(piso.canais) * 4;
    Some(u64::try_from(bytes).unwrap_or(u64::MAX))
}

/// **A leitura à mão dos cabeçalhos de som**, para
/// [`bytes_decodificados_minimos`].
///
/// Sem crate de áudio, e sem índice que entre em pânico: cada leitura passa
/// por `get`, e o que não está nos bytes é `None`. Cada leitura devolve um
/// piso dos quadros que o decodificador entrega: o que ela não sabe ao certo
/// conta para menos.
mod cabecalho_de_som {
    /// Um som como o cabeçalho o diz.
    pub(super) struct Piso {
        /// Um piso dos quadros, na taxa do arquivo.
        pub(super) amostras: u64,
        /// Quadros por segundo, no arquivo.
        pub(super) taxa: u32,
        /// Canais que o decodificador entrega.
        pub(super) canais: u32,
    }

    fn u16_le(bytes: &[u8], em: usize) -> Option<u16> {
        Some(u16::from_le_bytes(
            bytes.get(em..em.checked_add(2)?)?.try_into().ok()?,
        ))
    }

    fn u32_le(bytes: &[u8], em: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(em..em.checked_add(4)?)?.try_into().ok()?,
        ))
    }

    fn u32_be(bytes: &[u8], em: usize) -> Option<u32> {
        Some(u32::from_be_bytes(
            bytes.get(em..em.checked_add(4)?)?.try_into().ok()?,
        ))
    }

    fn u64_le(bytes: &[u8], em: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(em..em.checked_add(8)?)?.try_into().ok()?,
        ))
    }

    /// **WAV, exato**: os pedaços do RIFF em ordem, o `fmt ` antes do `data`.
    ///
    /// O tamanho que o `data` declara é limitado aos bytes que estão no
    /// arquivo: um WAV gravado aos poucos leva ali `0xFFFFFFFF`, e um cortado,
    /// mais do que tem. **E ao tamanho que o RIFF declara**, salvo quando o
    /// RIFF também foi gravado aos poucos (`0xFFFFFFFF`): o WebKit para onde o
    /// RIFF diz que o arquivo acaba, e o que vem depois não é som para ele.
    ///
    /// **Um segundo `fmt ` dá `None`.** O Chromium decodifica pelo primeiro, e
    /// qual deles vale é pergunta de motor: um piso não escolhe.
    pub(super) fn wav(bytes: &[u8]) -> Option<Piso> {
        let riff = u32_le(bytes, 4)?;
        let bytes = if riff == u32::MAX {
            bytes
        } else {
            let fim = usize::try_from(riff).ok()?.checked_add(8)?;
            bytes.get(..fim.min(bytes.len()))?
        };
        let mut resto = bytes.get(12..)?;
        let mut formato = None;
        loop {
            let id = resto.get(..4)?;
            let tamanho = usize::try_from(u32_le(resto, 4)?).ok()?;
            let corpo = resto.get(8..)?;
            if id == b"data" {
                let (canais, taxa, por_quadro) = formato?;
                let quadros = tamanho.min(corpo.len()) / por_quadro;
                return Some(Piso {
                    amostras: u64::try_from(quadros).ok()?,
                    taxa,
                    canais,
                });
            }
            if id == b"fmt " {
                if formato.is_some() {
                    return None;
                }
                formato = Some(fmt(corpo.get(..tamanho)?)?);
            }
            // Um pedaço de tamanho ímpar leva um byte de alinhamento.
            resto = corpo.get(tamanho.checked_add(tamanho % 2)?..)?;
        }
    }

    /// Canais, taxa e bytes por quadro de um `fmt `, só dos formatos em que
    /// um quadro é um bloco: PCM, float, A-law e µ-law, direto ou dentro do
    /// `WAVE_FORMAT_EXTENSIBLE`. Os comprimidos ficam de fora.
    ///
    /// O quadro conta pelo maior dos dois tamanhos que o cabeçalho permite
    /// ler — o bloco declarado e os canais vezes os bytes da amostra —, para
    /// que um cabeçalho que discorde de si mesmo dê menos quadros, e não mais.
    fn fmt(fmt: &[u8]) -> Option<(u32, u32, usize)> {
        let codigo = match u16_le(fmt, 0)? {
            0xFFFE => u16_le(fmt, 24)?,
            codigo => codigo,
        };
        if !matches!(codigo, 1 | 3 | 6 | 7) {
            return None;
        }
        let canais = u16_le(fmt, 2)?;
        let taxa = u32_le(fmt, 4)?;
        let bloco = usize::from(u16_le(fmt, 12)?);
        let amostra = usize::from(u16_le(fmt, 14)?).div_ceil(8);
        let por_quadro = bloco.max(usize::from(canais).checked_mul(amostra)?);
        (canais > 0 && por_quadro > 0).then_some((u32::from(canais), taxa, por_quadro))
    }

    /// Uma página Ogg, lida pelo cabeçalho de 27 bytes e pela tabela de
    /// segmentos.
    struct PaginaOgg<'a> {
        primeira: bool,
        ultima: bool,
        /// A posição ao fim da página, ou nada quando nenhum pacote termina
        /// nela (`-1`).
        granule: Option<u64>,
        serial: u32,
        corpo: &'a [u8],
        depois: &'a [u8],
    }

    fn pagina_ogg(bytes: &[u8]) -> Option<PaginaOgg<'_>> {
        if bytes.get(..4)? != b"OggS" || *bytes.get(4)? != 0 {
            return None;
        }
        let tipo = *bytes.get(5)?;
        let granule = u64_le(bytes, 6)?;
        let serial = u32_le(bytes, 14)?;
        let segmentos = usize::from(*bytes.get(26)?);
        let inicio = 27 + segmentos;
        let tamanho: usize = bytes.get(27..inicio)?.iter().map(|&s| usize::from(s)).sum();
        let fim = inicio + tamanho;
        Some(PaginaOgg {
            primeira: tipo & 0x02 != 0,
            ultima: tipo & 0x04 != 0,
            granule: (granule != u64::MAX).then_some(granule),
            serial,
            corpo: bytes.get(inicio..fim)?,
            depois: bytes.get(fim..)?,
        })
    }

    /// **Ogg, pelas páginas**, seguidas pelo tamanho de cada uma desde a
    /// primeira — e não por uma busca de `OggS`, que acharia a marca dentro dos
    /// dados.
    ///
    /// A primeira página é a da identificação, Vorbis ou Opus. O piso é a
    /// granule da última página do fluxo menos a da primeira de dados: um
    /// fluxo que não começa do zero — um pedaço de transmissão — decodifica
    /// menos do que a última granule diz. No Opus sai também o pre-skip, e a
    /// taxa é sempre 48 kHz: a do cabeçalho é a da entrada, e só informa.
    pub(super) fn ogg(bytes: &[u8]) -> Option<Piso> {
        let identificacao = pagina_ogg(bytes)?;
        if !identificacao.primeira {
            return None;
        }
        let pacote = identificacao.corpo;
        let (taxa, canais, pre_skip) = if pacote.starts_with(b"\x01vorbis") {
            if u32_le(pacote, 7)? != 0 {
                return None;
            }
            (u32_le(pacote, 12)?, u32::from(*pacote.get(11)?), 0)
        } else if pacote.starts_with(b"OpusHead") {
            // A versão de cima diferente de zero é um Opus incompatível.
            if *pacote.get(8)? >= 16 {
                return None;
            }
            (
                48_000,
                u32::from(*pacote.get(9)?),
                u64::from(u16_le(pacote, 10)?),
            )
        } else {
            return None;
        };
        let (mut primeira, mut ultima) = (None, None);
        let mut resto = identificacao.depois;
        while let Some(pagina) = pagina_ogg(resto) {
            resto = pagina.depois;
            if pagina.serial != identificacao.serial {
                continue;
            }
            if let Some(granule) = pagina.granule.filter(|&granule| granule > 0) {
                primeira.get_or_insert(granule);
                ultima = Some(granule);
            }
            if pagina.ultima {
                break;
            }
        }
        let comeco = primeira?.max(pre_skip);
        Some(Piso {
            amostras: ultima?.saturating_sub(comeco),
            taxa,
            canais,
        })
    }

    /// O que o cabeçalho de quatro bytes de um quadro MPEG áudio diz.
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct QuadroMpeg {
        mpeg1: bool,
        camada: u8,
        taxa: u32,
        canais: u32,
        amostras: u64,
        tamanho: usize,
    }

    /// As taxas de bits, em kbps, pelo índice do cabeçalho. O zero é o
    /// formato livre, que não diz o tamanho do quadro.
    #[rustfmt::skip]
    const KBPS: [[u16; 15]; 5] = [
        // MPEG-1, camadas I, II e III.
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
        // MPEG-2 e 2.5, camada I; e camadas II e III.
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    ];

    fn quadro_mpeg(bytes: &[u8]) -> Option<QuadroMpeg> {
        let [b0, b1, b2, b3]: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        if b0 != 0xFF || b1 & 0xE0 != 0xE0 {
            return None;
        }
        // Versão: `11` é MPEG-1, `10` MPEG-2, `00` MPEG-2.5, `01` reservada.
        let versao = (b1 >> 3) & 0b11;
        let camada = match (b1 >> 1) & 0b11 {
            0b01 => 3,
            0b10 => 2,
            0b11 => 1,
            _ => return None,
        };
        let mpeg1 = versao == 0b11;
        let tabela = match (mpeg1, camada) {
            (true, camada) => usize::from(camada - 1),
            (false, 1) => 3,
            (false, _) => 4,
        };
        let kbps = *KBPS.get(tabela)?.get(usize::from(b2 >> 4))?;
        if kbps == 0 {
            return None;
        }
        let taxa = match (versao, (b2 >> 2) & 0b11) {
            (0b11, 0) => 44_100,
            (0b11, 1) => 48_000,
            (0b11, 2) => 32_000,
            (0b10, 0) => 22_050,
            (0b10, 1) => 24_000,
            (0b10, 2) => 16_000,
            (0b00, 0) => 11_025,
            (0b00, 1) => 12_000,
            (0b00, 2) => 8_000,
            _ => return None,
        };
        let enchimento = u64::from((b2 >> 1) & 1);
        let amostras: u64 = match (camada, mpeg1) {
            (1, _) => 384,
            (2, _) | (3, true) => 1152,
            _ => 576,
        };
        let bits = u64::from(kbps) * 1000;
        let tamanho = if camada == 1 {
            (12 * bits / u64::from(taxa) + enchimento) * 4
        } else {
            amostras / 8 * bits / u64::from(taxa) + enchimento
        };
        Some(QuadroMpeg {
            mpeg1,
            camada,
            taxa,
            canais: if b3 >> 6 == 0b11 { 1 } else { 2 },
            amostras,
            tamanho: usize::try_from(tamanho)
                .ok()
                .filter(|&tamanho| tamanho > 4)?,
        })
    }

    /// Quantos quadros o quadro Xing/Info diz que o arquivo tem, quando o
    /// primeiro quadro é um e traz a contagem.
    fn quadros_do_xing(quadro: &[u8], cabecalho: QuadroMpeg) -> Option<u64> {
        if cabecalho.camada != 3 {
            return None;
        }
        // A marca vem depois da informação lateral, que depende da versão e
        // dos canais.
        let em = 4 + match (cabecalho.mpeg1, cabecalho.canais) {
            (true, 1) | (false, 2) => 17,
            (true, _) => 32,
            (false, _) => 9,
        };
        let marca = quadro.get(em..em + 4)?;
        if marca != b"Xing" && marca != b"Info" {
            return None;
        }
        if u32_be(quadro, em + 4)? & 1 == 0 {
            return None;
        }
        Some(u64::from(u32_be(quadro, em + 8)?))
    }

    /// **MP3, pelos quadros que estão no arquivo.**
    ///
    /// Depois das etiquetas ID3v2 do começo, o primeiro quadro tem de estar
    /// ali; dele em diante, cada quadro inteiro com a mesma versão, camada,
    /// taxa e canais conta, e a cadeia para no primeiro que não for um — uma
    /// etiqueta no fim, um arquivo cortado. Contar quadros, e não bytes pela
    /// taxa de bits mais alta, deixa de fora o que não é som: uma etiqueta
    /// APE com a capa no fim do arquivo não vira minutos de som.
    ///
    /// **O quadro Xing/Info limita a conta**: é o que o codificador escreveu,
    /// e o piso fica no menor dos dois. Um download interrompido tem um Xing
    /// maior que o arquivo, e a cadeia o segura; uma cadeia que seguisse além
    /// do que o codificador contou, o Xing a segura.
    ///
    /// **E o que o decodificador pode cortar sai**: o próprio quadro
    /// Xing/Info, o atraso e o enchimento que a etiqueta do LAME declara (até
    /// 4095 amostras cada), e o atraso do decodificador (529).
    ///
    /// **O «nunca passa do real» vale para um fluxo bem formado.** Um quadro
    /// com o cabeçalho certo e o conteúdo corrompido entra na cadeia, e o
    /// decodificador não o entrega. O Chromium o descarta. O WebKit, conforme
    /// o defeito, recusa o arquivo inteiro — medido com dez quadros
    /// corrompidos seguidos — ou descarta os quadros como o Chromium — com
    /// dez espalhados. O corte acima absorve uns oito quadros de 1152 amostras
    /// (9871 amostras): com um corrompido, o piso fica abaixo do real nos dois
    /// motores. Com mais que isso, e o arquivo decodificado, o piso pode
    /// passar do real **nos dois** — a revisão mediu, com dez espalhados no
    /// `City_Night_Crowd`, 18 952 bytes a mais no WebKit e 14 352 no
    /// Chromium —, e o som pode ser recusado pelo teto que, decodificado, ele
    /// talvez não passasse.
    pub(super) fn mp3(bytes: &[u8]) -> Option<Piso> {
        let mut inicio = 0usize;
        while bytes.get(inicio..)?.starts_with(b"ID3") {
            let etiqueta = bytes.get(inicio..inicio.checked_add(10)?)?;
            // O tamanho em quatro bytes de sete bits, sem o cabeçalho, e o
            // rodapé quando a bandeira o diz.
            let tamanho = etiqueta.get(6..10)?.iter().try_fold(0usize, |soma, &b| {
                (b < 0x80).then_some((soma << 7) | usize::from(b))
            })?;
            let rodape = if etiqueta.get(5)? & 0x10 != 0 { 10 } else { 0 };
            inicio = inicio.checked_add(10 + tamanho + rodape)?;
        }
        let primeiro = quadro_mpeg(bytes.get(inicio..)?)?;
        let mut quadros = 0u64;
        let mut em = inicio;
        while let Some(quadro) = bytes.get(em..).and_then(quadro_mpeg) {
            let mesmo = (quadro.mpeg1, quadro.camada, quadro.taxa, quadro.canais)
                == (
                    primeiro.mpeg1,
                    primeiro.camada,
                    primeiro.taxa,
                    primeiro.canais,
                );
            let fim = em.checked_add(quadro.tamanho)?;
            if !mesmo || fim > bytes.len() {
                break;
            }
            quadros += 1;
            em = fim;
        }
        if let Some(contados) = quadros_do_xing(bytes.get(inicio..)?, primeiro) {
            quadros = quadros.min(contados);
        }
        let corte = primeiro.amostras + 2 * 4095 + 529;
        Some(Piso {
            amostras: quadros
                .saturating_mul(primeiro.amostras)
                .saturating_sub(corte),
            taxa: primeiro.taxa,
            canais: primeiro.canais,
        })
    }
}
