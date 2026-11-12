use super::{
    imagem_lida_dita, ler_midia_do_servidor, motivo_de, som_declarado_do_mod, BaldesDoRegistro,
    FalhaNoMod,
};
use crate::pacote_de_teste::{pasta, publicar, ID, WAV};
use crate::rastro_de_teste::capturar;
use crate::relogio_de_teste::origem;
use seele_ffi::mods::TETO_DE_MIDIA;
use seele_ffi::som::{
    bytes_decodificados_minimos, TAXA_DA_DECODIFICACAO, TETO_DO_SOM_DECODIFICADO,
};

/// Um WAV PCM com `dados` bytes de silêncio.
fn wav(canais: u16, taxa: u32, bits: u16, dados: usize) -> Vec<u8> {
    let por_quadro = canais * bits.div_ceil(8);
    let tamanho = u32::try_from(dados).expect("o WAV de teste cabe em 32 bits");
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&(36 + tamanho).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&canais.to_le_bytes());
    wav.extend_from_slice(&taxa.to_le_bytes());
    wav.extend_from_slice(&(taxa * u32::from(por_quadro)).to_le_bytes());
    wav.extend_from_slice(&por_quadro.to_le_bytes());
    wav.extend_from_slice(&bits.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&tamanho.to_le_bytes());
    wav.resize(44 + dados, 0);
    wav
}

/// Uma página Ogg com um pacote só, de menos de 255 bytes. O CRC fica em
/// zero: a leitura segue as páginas pelo tamanho, e não o confere.
fn pagina_ogg(tipo: u8, granule: u64, sequencia: u32, pacote: &[u8]) -> Vec<u8> {
    let mut pagina = b"OggS\0".to_vec();
    pagina.push(tipo);
    pagina.extend_from_slice(&granule.to_le_bytes());
    pagina.extend_from_slice(&0x5EE1_E000_u32.to_le_bytes());
    pagina.extend_from_slice(&sequencia.to_le_bytes());
    pagina.extend_from_slice(&[0; 4]);
    pagina.push(1);
    pagina.push(u8::try_from(pacote.len()).expect("o pacote de teste cabe num segmento"));
    pagina.extend_from_slice(pacote);
    pagina
}

/// Um Ogg de três páginas: a da identificação, uma de dados logo no
/// começo, e a última, que diz até onde o som vai.
fn ogg(identificacao: &[u8], granule_final: u64) -> Vec<u8> {
    [
        pagina_ogg(0x02, 0, 0, identificacao),
        pagina_ogg(0x00, 1024, 1, &[0; 16]),
        pagina_ogg(0x04, granule_final, 2, &[0; 16]),
    ]
    .concat()
}

/// O cabeçalho de identificação do Vorbis.
fn vorbis(canais: u8, taxa: u32) -> Vec<u8> {
    let mut id = b"\x01vorbis".to_vec();
    id.extend_from_slice(&0u32.to_le_bytes());
    id.push(canais);
    id.extend_from_slice(&taxa.to_le_bytes());
    id.extend_from_slice(&[0; 12]);
    id.extend_from_slice(&[0xB8, 1]);
    id
}

/// O cabeçalho de identificação do Opus.
fn opus(canais: u8, pre_skip: u16) -> Vec<u8> {
    let mut id = b"OpusHead\x01".to_vec();
    id.push(canais);
    id.extend_from_slice(&pre_skip.to_le_bytes());
    id.extend_from_slice(&44_100u32.to_le_bytes());
    id.extend_from_slice(&[0; 3]);
    id
}

/// Um quadro MPEG-1 camada III a 48 kHz e 320 kbps — 960 bytes e 1152
/// amostras —, com o quadro Xing quando `xing` diz quantos são.
fn quadro_mp3(canais: u8, xing: Option<u32>) -> Vec<u8> {
    let modo = if canais == 1 { 0xC0 } else { 0x00 };
    let mut quadro = vec![0xFF, 0xFB, 0xE4, modo];
    if let Some(quadros) = xing {
        quadro.resize(if canais == 1 { 4 + 17 } else { 4 + 32 }, 0);
        quadro.extend_from_slice(b"Xing");
        quadro.extend_from_slice(&1u32.to_be_bytes());
        quadro.extend_from_slice(&quadros.to_be_bytes());
    }
    quadro.resize(960, 0);
    quadro
}

/// Um MP3 de `tamanho` bytes, quadro depois de quadro; o último fica
/// cortado onde o tamanho acaba.
fn mp3(canais: u8, xing: Option<u32>, tamanho: usize) -> Vec<u8> {
    let mut mp3 = quadro_mp3(canais, xing);
    let quadro = quadro_mp3(canais, None);
    while mp3.len() < tamanho {
        mp3.extend_from_slice(&quadro);
    }
    mp3.truncate(tamanho);
    mp3
}

/// Publica `bytes` em `caminho` e confere que o som é recusado pelo nome,
/// com os números, e dito no registro.
fn recusado(caminho: &str, bytes: &[u8]) {
    let (raiz, hash) = publicar(&[(caminho, bytes)]);
    let (resultado, rastro) = capturar(|| {
        som_declarado_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            caminho,
        )
    });
    let falha = resultado.err().unwrap_or_else(|| {
        panic!(
            "`{caminho}`, que decodificado passa do teto do som, foi servido: a janela o \
                 decodifica inteiro antes de poder recusá-lo, e o pico que o teto existe para \
                 evitar acontece — o piso foi {:?}",
            bytes_decodificados_minimos(bytes)
        )
    });
    recusado_pelo_teto(caminho, &falha);
    assert!(
        rastro.lines().any(|linha| linha.contains("WARN")
            && linha.contains("mod_id=prova/midia")
            && linha.contains("som-grande-demais-decodificado")),
        "a recusa de `{caminho}` não chegou ao registro, e quem hospeda não sabe por que o \
             som não tocou: {rastro}"
    );
}

/// A recusa é a do som grande demais decodificado, com os números.
fn recusado_pelo_teto(caminho: &str, falha: &FalhaNoMod) {
    let motivo = motivo_de(falha);
    assert!(
        motivo.starts_with("som-grande-demais-decodificado")
            && motivo.contains(&TETO_DO_SOM_DECODIFICADO.to_string()),
        "`{caminho}` foi recusado por outro motivo, ou sem o teto: a janela não o diz como o \
             teto do som, e o MOD fica sem os números — {motivo}"
    );
}

/// Publica `bytes` em `caminho` e confere que o som sai, intacto.
fn servido(caminho: &str, bytes: &[u8]) {
    let (raiz, hash) = publicar(&[(caminho, bytes)]);
    let (resultado, _) = capturar(|| {
        som_declarado_do_mod(
            &BaldesDoRegistro::default(),
            origem(),
            &pasta(&raiz),
            ID,
            &hash,
            caminho,
        )
    });
    let servido = resultado.unwrap_or_else(|falha| {
        panic!(
            "`{caminho}`, que cabe decodificado, foi recusado — {}",
            motivo_de(&falha)
        )
    });
    assert!(
        servido == bytes,
        "os bytes de `{caminho}` mudaram no caminho"
    );
}

/// O piso em bytes de `segundos` de som a 48 kHz, com `canais`: o que a
/// janela segura, menos o quadro que a conversão de taxa pode arredondar.
fn segundos(segundos: u64, canais: u64) -> u64 {
    (segundos * TAXA_DA_DECODIFICACAO - 1) * canais * 4
}

/// **WAV: exato, pelo `fmt ` e pelo `data`.** Oito bits, mono, 8 kHz, do
/// tamanho do teto de arquivo: cada byte vira seis quadros de quatro bytes,
/// e o arquivo de dez MiB, uns 250 MB decodificado.
#[test]
fn um_wav_que_passa_do_teto_decodificado_e_recusado_antes() {
    let grande = wav(1, 8_000, 8, TETO_DE_MIDIA - 44);
    assert_eq!(
        grande.len(),
        TETO_DE_MIDIA,
        "o WAV grande não é do tamanho do teto"
    );
    recusado("som/grande.wav", &grande);
    assert_eq!(
        bytes_decodificados_minimos(&grande),
        Some(((TETO_DE_MIDIA as u64 - 44) * 6 - 1) * 4),
        "o piso do WAV não é o exato que o `fmt ` e o `data` dizem"
    );

    // Um segundo passa, com os bytes exatos.
    let curto = wav(1, 8_000, 8, 8_000);
    assert_eq!(
        bytes_decodificados_minimos(&curto),
        Some(segundos(1, 1)),
        "o piso de um segundo de WAV não é um segundo a 48 kHz"
    );
    servido("som/curto.wav", &curto);

    // **O `data` diz mais do que o arquivo tem**, como num WAV gravado
    // aos poucos: o piso conta os bytes que estão lá.
    let mut aos_poucos = wav(1, 8_000, 8, 8_000);
    if let Some(tamanho) = aos_poucos.get_mut(40..44) {
        tamanho.copy_from_slice(&u32::MAX.to_le_bytes());
    }
    assert_eq!(
        bytes_decodificados_minimos(&aos_poucos),
        Some(segundos(1, 1)),
        "o piso do WAV contou o tamanho que o `data` alega, e não os bytes que estão no \
             arquivo: um WAV gravado aos poucos seria recusado pelo que não tem"
    );

    // O WAV mínimo do pacote de teste não tem `fmt ` nem `data`
    // completos: o piso não existe, e o som segue para a janela.
    assert_eq!(
        bytes_decodificados_minimos(WAV),
        None,
        "um WAV sem `fmt ` nem `data` ganhou um piso, que só pode ter sido inventado"
    );
}

/// **Ogg: pela granule da última página.** Três minutos de Vorbis estéreo a
/// 44,1 kHz são uns 69 MB a 48 kHz; no Opus, a granule já está a 48 kHz,
/// menos o pre-skip.
#[test]
fn um_ogg_que_passa_do_teto_decodificado_e_recusado_antes() {
    recusado("som/vorbis.ogg", &ogg(&vorbis(2, 44_100), 180 * 44_100));
    recusado("som/opus.ogg", &ogg(&opus(2, 312), 180 * 48_000 + 312));

    // Um minuto passa: pouco menos que um minuto a 48 kHz, estéreo.
    let piso = bytes_decodificados_minimos(&ogg(&vorbis(2, 44_100), 60 * 44_100));
    assert!(
        piso.is_some_and(|piso| piso <= segundos(60, 2) && piso > segundos(59, 2)),
        "o piso de um minuto de Vorbis não é um minuto: {piso:?}"
    );
    // O Opus mono conta um canal, e o pre-skip sai.
    let piso = bytes_decodificados_minimos(&ogg(&opus(1, 312), 60 * 48_000 + 312));
    assert!(
        piso.is_some_and(|piso| piso <= segundos(60, 1) && piso > segundos(59, 1)),
        "o piso de um minuto de Opus mono não é um minuto: {piso:?}"
    );
}

/// **MP3: pelos quadros.** Um Xing de 7600 quadros estéreo são uns 70 MB
/// decodificado; sem Xing, a cadeia de quadros diz o mesmo. Um mono sem
/// Xing do tamanho do teto — a 320 kbps, a taxa mais alta do MPEG-1 camada
/// III, o mais curto que dez MiB podem ser — fica em uns 50 MB, e passa.
#[test]
fn um_mp3_que_passa_do_teto_decodificado_e_recusado_antes() {
    recusado("som/xing.mp3", &mp3(2, Some(7_600), 7_600 * 960));
    recusado("som/estereo.mp3", &mp3(2, None, TETO_DE_MIDIA));

    let mono = bytes_decodificados_minimos(&mp3(1, None, TETO_DE_MIDIA));
    assert!(
        mono.is_some_and(|piso| piso > 45_000_000 && piso <= TETO_DO_SOM_DECODIFICADO),
        "o MP3 mono do tamanho do teto não ficou abaixo do teto do som, e seria recusado \
             por um piso que passa do real: {mono:?}"
    );

    // **O Xing de um arquivo cortado conta o que não está nele.** O piso
    // fica nos quadros que estão: cem, e não 7600.
    let cortado = bytes_decodificados_minimos(&mp3(2, Some(7_600), 100 * 960));
    assert!(
        cortado.is_some_and(|piso| piso < segundos(3, 2)),
        "o piso do MP3 cortado contou os quadros que o Xing alega, e não os que estão no \
             arquivo: um download interrompido seria recusado pelo que não tem — {cortado:?}"
    );
}

/// **As margens que fazem do valor um piso**, uma por teste. Sem elas os
/// casos acima continuam verdes, e o piso passa do real: medido pela
/// revisão contra o `decodeAudioData` do Chromium e do WebKit — o motor do
/// produto no Mac —, um WAV com o bloco declarado menor dava quatro vezes
/// o real, um Opus com pre-skip de 3840 passava 23 032 bytes, e um MP3
/// comum, de 16 a 25 mil.
///
/// O WAV: o bloco declarado (1) é menor que os canais vezes os bytes da
/// amostra (2 × 2). O quadro conta pelo maior: um segundo estéreo de
/// 16 bits a 8 kHz, e não quatro.
#[test]
fn o_bloco_menor_que_a_amostra_nao_multiplica_o_piso_do_wav() {
    let mut wav = wav(2, 8_000, 16, 8_000 * 4);
    if let Some(bloco) = wav.get_mut(32..34) {
        bloco.copy_from_slice(&1u16.to_le_bytes());
    }
    assert_eq!(
        bytes_decodificados_minimos(&wav),
        Some(segundos(1, 2)),
        "o piso do WAV contou os quadros pelo bloco que o cabeçalho declara, menor que a \
             amostra: quatro vezes o real, e um som que cabe seria recusado"
    );
}

/// **Um segundo `fmt ` deixa o WAV sem piso.** O Chromium decodifica pelo
/// primeiro, e o piso que lesse o último recusaria pelo formato que o
/// motor não usa: a revisão mediu 127 MB de piso contra 11,5 MB reais. Qual
/// dos dois vale é pergunta de motor, e um piso não escolhe.
#[test]
fn um_segundo_fmt_deixa_o_wav_sem_piso() {
    let primeiro = wav(2, 48_000, 16, 48_000 * 4);
    let segundo = wav(1, 8_000, 8, 0);
    let mut dois = [
        primeiro.get(..36).expect("o cabeçalho e o primeiro `fmt `"),
        segundo.get(12..36).expect("o segundo `fmt `"),
        primeiro.get(36..).expect("o `data`"),
    ]
    .concat();
    let riff = u32::try_from(dois.len() - 8).expect("o WAV de teste cabe em 32 bits");
    if let Some(tamanho) = dois.get_mut(4..8) {
        tamanho.copy_from_slice(&riff.to_le_bytes());
    }
    assert_eq!(
        bytes_decodificados_minimos(&dois),
        None,
        "um WAV com dois `fmt ` ganhou um piso por um deles, e o motor pode decodificar pelo \
             outro: a revisão mediu a recusa de um som que cabia"
    );
}

/// **O `data` não passa do tamanho do RIFF.** Um `data` gravado aos poucos
/// (`0xFFFFFFFF`) seguido de outro pedaço: o WebKit para onde o RIFF diz
/// que o arquivo acaba, e o piso que contasse até o fim do arquivo passaria
/// do real — a revisão mediu 2,28 MB a mais.
#[test]
fn o_data_do_wav_nao_passa_do_tamanho_do_riff() {
    let mut wav = wav(1, 8_000, 8, 8_000);
    if let Some(tamanho) = wav.get_mut(40..44) {
        tamanho.copy_from_slice(&u32::MAX.to_le_bytes());
    }
    wav.extend_from_slice(b"LIST");
    wav.extend_from_slice(&4_000u32.to_le_bytes());
    wav.resize(wav.len() + 4_000, 0);
    assert_eq!(
        bytes_decodificados_minimos(&wav),
        Some(segundos(1, 1)),
        "o piso do WAV contou como som o pedaço que vem depois do fim que o RIFF declara, e \
             o WebKit para ali: o piso passa do real"
    );
}

/// O Opus: o pre-skip (3840) é maior que a granule da primeira página de
/// dados (1024, um pacote só). O que sai do começo é o maior dos dois: um
/// segundo depois do pre-skip, e não 2816 amostras a mais.
#[test]
fn o_pre_skip_maior_que_a_primeira_pagina_sai_do_piso_do_opus() {
    let opus = ogg(&opus(2, 3_840), 48_000 + 3_840);
    assert_eq!(
        bytes_decodificados_minimos(&opus),
        Some(segundos(1, 2)),
        "o piso do Opus tirou do começo só a granule da primeira página, menor que o \
             pre-skip: o decodificador corta o pre-skip inteiro, e o piso passa do real"
    );
}

/// O MP3: cem quadros estéreo de 1152 amostras são 115 200; o corte do
/// decodificador — o quadro Xing/Info, o atraso e o enchimento do LAME, e o
/// atraso do decodificador — tira 1152 + 2 × 4095 + 529 = 9871, a
/// conversão de taxa tira um quadro, e o resto, vezes dois canais e quatro
/// bytes, é 842 624.
#[test]
fn o_corte_do_decodificador_sai_do_piso_do_mp3() {
    assert_eq!(
        bytes_decodificados_minimos(&mp3(2, None, 100 * 960)),
        Some(842_624),
        "o piso do MP3 contou as amostras que o decodificador corta — o quadro do Xing, o \
             atraso e o enchimento —, e passa do real em todo MP3 comum"
    );
}

/// **As duas portas do servidor recusam o mesmo som**: a resposta de
/// `midia_em_bytes` e a leitura por volume chegam à janela pelo `data:`, e
/// a janela o decodifica inteiro, como o do pacote.
#[test]
fn o_som_do_servidor_que_passa_do_teto_decodificado_e_recusado_antes_de_servir() {
    let grande = wav(1, 8_000, 8, TETO_DE_MIDIA - 44);
    let falha = ler_midia_do_servidor(&seele_ffi::base64_de(&grande), None)
        .err()
        .expect("o som do servidor que passa do teto decodificado foi servido no `data:`");
    recusado_pelo_teto("o som do servidor", &falha);

    let (resultado, rastro) =
        capturar(|| imagem_lida_dita(&BaldesDoRegistro::default(), origem(), ID, Ok(grande), None));
    let falha = resultado
        .err()
        .expect("o som lido por volume que passa do teto decodificado foi servido");
    recusado_pelo_teto("o som lido por volume", &falha);
    assert!(
        rastro.lines().any(|linha| linha.contains("WARN")
            && linha.contains("origem=\"volume\"")
            && linha.contains("som-grande-demais-decodificado")),
        "a recusa do som lido por volume não chegou ao registro: {rastro}"
    );

    let curto = ler_midia_do_servidor(&seele_ffi::base64_de(&wav(1, 8_000, 8, 8_000)), None);
    assert!(
        curto.is_ok_and(|midia| midia.papel == "som" && midia.uri.is_some()),
        "o som curto do servidor deixou de sair com o `data:`, que é o único caminho dele \
             até a janela"
    );
}

/// **Os números do som decodificado são os da janela.** O Rust repete o
/// teto e a taxa de `mods-regiao.js`: com um maior que o da janela, o Rust
/// serviria o som que a janela vai recusar depois do pico; com um menor,
/// recusaria o que cabe.
#[test]
fn os_numeros_do_som_decodificado_sao_os_da_janela() {
    let regiao = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/mods-regiao.js"),
    )
    .expect("mods-regiao.js legível")
    .replace("\r\n", "\n");
    let taxa = regiao
        .lines()
        .find_map(|linha| linha.strip_prefix("const TAXA_DA_DECODIFICACAO = "))
        .and_then(|valor| numero_em_js(valor.trim_end_matches(';')));
    assert_eq!(
        taxa,
        Some(TAXA_DA_DECODIFICACAO),
        "`TAXA_DA_DECODIFICACAO` de `mods-regiao.js` não é a do Rust: o piso conta os \
             quadros numa taxa e a janela decodifica noutra"
    );
    for tabela in ["LIMITES_DA_REGIAO", "LIMITES_DO_CARTAO"] {
        let teto = regiao
            .split(&format!("const {tabela} = Object.freeze({{"))
            .nth(1)
            .and_then(|resto| resto.split("\n});").next())
            .and_then(|corpo| {
                corpo
                    .lines()
                    .find_map(|linha| linha.trim().strip_prefix("bytesDeSomDecodificado:"))
            })
            .and_then(|valor| numero_em_js(valor.trim().trim_end_matches(',')));
        assert_eq!(
            teto,
            Some(TETO_DO_SOM_DECODIFICADO),
            "`{tabela}.bytesDeSomDecodificado` não é o teto do Rust: o Rust serve o som que a \
                 janela recusa depois do pico, ou recusa o que cabe"
        );
    }
}

/// Um número de JavaScript escrito como produto de inteiros — `48000`,
/// `64 * 1024 * 1024` —, ou nada.
fn numero_em_js(texto: &str) -> Option<u64> {
    texto.split('*').try_fold(1u64, |produto, fator| {
        produto.checked_mul(fator.trim().replace('_', "").parse().ok()?)
    })
}
