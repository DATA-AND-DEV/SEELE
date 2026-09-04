//! O que só se prova com o módulo do Cisco na mão.
//!
//! # Por que estes testes pulam em vez de falhar
//!
//! Porque **o módulo não pode estar no repositório**, e isso não é preguiça: a
//! cobertura de patente do H.264 acompanha o binário que o Cisco distribui, e
//! guardá-lo aqui nos poria como distribuidores sem ela (§2 da spec de
//! compartilhamento de tela). Um teste que exigisse o arquivo seria um teste
//! vermelho em toda máquina limpa, e um teste sempre vermelho é um teste que
//! todo mundo aprende a ignorar.
//!
//! Então eles pulam **dizendo em voz alta que pularam**, e o que falta para
//! rodar. Aponte o módulo com `SEELE_OPENH264`, ou deixe-o em
//! `target/libopenh264.dylib` na raiz do workspace, que é onde
//! `spikes/tela-no-codec/README.md` manda pôr:
//!
//! ```text
//! curl -L -o m.bz2 https://ciscobinary.openh264.org/libopenh264-2.6.0-mac-arm64.dylib.bz2
//! bunzip2 m.bz2 && mv m target/libopenh264.dylib
//! ```
//!
//! **Nada aqui baixa nada.** Buscar na rede durante um teste faria dele um
//! teste da rede.

// `specs/10-convencoes.md` permite `expect` em teste com a exceção nomeada.
// Aqui uma falha é falha do teste, e a mensagem do `expect` é o que diz qual
// dos passos do caminho de ida e volta caiu.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::Instant;

use seele_video::codec::{
    armar, Cadencia, Codificador, ConfigDoCodificador, Decodificador, QuadroI420, Resolucao,
};
use seele_video::modulo::{self, BibliotecaDeVideo};

/// Onde procurar, na ordem: o que quem roda apontou, depois a pasta de build.
fn pastas() -> Vec<PathBuf> {
    let mut pastas = Vec::new();
    if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
        let caminho = PathBuf::from(apontado);
        // Aceita tanto a pasta quanto o arquivo: quem exporta a variável quase
        // sempre exporta o arquivo, e fazer isso falhar em silêncio seria a
        // pior maneira de o teste pular.
        pastas.push(if caminho.is_dir() {
            caminho
        } else {
            caminho.parent().map(PathBuf::from).unwrap_or(caminho)
        });
    }
    pastas.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target"),
    );
    pastas
}

/// A biblioteca, ou `None` com o motivo impresso.
fn biblioteca() -> Option<BibliotecaDeVideo> {
    match BibliotecaDeVideo::procurar_e_carregar(&pastas()) {
        Ok(biblioteca) => Some(biblioteca),
        Err(motivo) => {
            let onde =
                modulo::publicado_para_este_sistema().map_or_else(|| "—".to_owned(), |m| m.url());
            // **Onde o codec é exigido, faltar é falha e não licença para
            // pular.** Um teste que volta cedo conta como passado, e ninguém
            // distingue «rodou» de «pulou» num relatório de CI — foi assim que
            // o `ida_e_volta` ficou verde por anos enquanto **falhava** com o
            // módulo que o próprio produto baixa, e assim que o som da tela do
            // Windows atravessou seis versões quebrado.
            // Só onde **há** módulo publicado. No Linux o Cisco não publica
            // nada, e ali pular é a resposta certa e não um buraco.
            assert!(
                std::env::var_os("SEELE_EXIGE_CODEC").is_none()
                    || modulo::publicado_para_este_sistema().is_none(),
                "SEELE_EXIGE_CODEC está ligado, este sistema tem módulo publicado e ele \
                 não está aqui: {motivo}.\n\
                 Quem liga essa variável está dizendo «neste lugar o codec tem de \
                 existir», e é o CI que a liga. Buscar: {onde}"
            );
            eprintln!(
                "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe isso.\n  \
                 Busque {onde} e aponte-o com SEELE_OPENH264.\n  \
                 Ligue SEELE_EXIGE_CODEC para que faltar vire falha em vez de pulo."
            );
            None
        }
    }
}

/// Um quadro com bordas duras, que é o conteúdo caro de uma tela de trabalho —
/// texto de contraste altíssimo é a borda mais cara que existe para uma DCT.
/// Um quadro chapado sairia com trinta bytes e não provaria nada.
fn quadro_com_textura(resolucao: Resolucao, passo: usize) -> QuadroI420 {
    let (largura, altura) = (resolucao.largura(), resolucao.altura());
    let mut y = Vec::with_capacity(largura * altura);
    for linha in 0..altura {
        for coluna in 0..largura {
            let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
            y.push(if claro { 235 } else { 16 });
        }
    }
    let croma = vec![128; largura.div_ceil(2) * altura.div_ceil(2)];
    QuadroI420::novo(largura, altura, y, croma.clone(), croma).expect("planos de um I420")
}

/// Os tipos dos NALs de um fluxo Annex-B, na ordem em que aparecem.
fn tipos_de_nal(bytes: &[u8]) -> Vec<u8> {
    let mut achados = Vec::new();
    let mut i = 0;
    while i + 4 < bytes.len() {
        match bytes.get(i..i + 4) {
            Some([0, 0, 0, 1]) => {
                if let Some(cabeca) = bytes.get(i + 4) {
                    achados.push(cabeca & 0x1F);
                }
                i += 4;
            }
            _ => i += 1,
        }
    }
    achados
}

#[test]
fn um_quadro_atravessa_o_codec_e_volta_com_o_tamanho_certo() {
    let Some(biblioteca) = biblioteca() else {
        return;
    };

    let mut codificador = Codificador::novo(
        &biblioteca,
        ConfigDoCodificador {
            resolucao: Resolucao::P540,
            cadencia: Cadencia::Q30,
            teto_bps: 0,
        },
    )
    .expect("armar o codificador");
    let mut decodificador = Decodificador::novo(&biblioteca).expect("armar o decodificador");

    let primeiro = codificador
        .codificar(&quadro_com_textura(Resolucao::P540, 0), false)
        .expect("codificar")
        .expect("o primeiro quadro nunca é pulado: não há nada de que ele dependa");

    // **É esta a asserção que justifica o teste existir.** A binding devolve
    // SPS e PPS separados do resto e sem código de início; se `montar_annex_b`
    // não os recolocasse na frente, o fluxo sairia daqui com bytes e nenhum
    // decoder do mundo o abriria. O sintoma seria uma tela preta do outro lado,
    // sem erro nenhum de nenhum dos dois lados.
    assert!(primeiro.chave, "o primeiro quadro é sempre um quadro-chave");
    assert_eq!(
        tipos_de_nal(&primeiro.bytes),
        vec![7, 8, 5],
        "o quadro-chave tem de sair como SPS, PPS e IDR, nessa ordem"
    );

    // **A primeira decodificação não devolve imagem, e isso é do OpenH264.**
    //
    // Medido: alimentando o mesmo IDR três vezes, a primeira chamada devolve
    // nada e a segunda e a terceira devolvem o quadro. O decodificador gasta a
    // primeira consumindo SPS e PPS e se armando; o `DecodeFrameNoDelay` não
    // muda isso, porque o atraso não é de reordenação, é de inicialização.
    //
    // Este teste afirmava «um quadro-chave sozinho tem de bastar para o
    // decoder» e **falhava** — com o módulo que o próprio produto baixa. Ficou
    // anos verde porque sem `SEELE_OPENH264` ele pula, que é como o CI o via.
    // A afirmação era sobre a biblioteca, não sobre o nosso fluxo, e estava
    // errada: o fluxo sempre esteve certo, e a linha acima é quem prova isso.
    //
    // Nada disto alcança quem usa: quem assiste decodifica pelo sistema, e não
    // por este módulo — só quem transmite precisa dele.
    let armando = decodificador
        .decodificar(&primeiro.bytes)
        .expect("decodificar a primeira vez");
    assert!(
        armando.is_none(),
        "o decodificador devolveu imagem na primeira chamada.\n\
         Se o OpenH264 mudou, ótimo — mas o comentário acima deixou de valer e \
         quem mexer aqui tem de refazer a medição antes de confiar nele."
    );

    let voltou = decodificador
        .decodificar(&primeiro.bytes)
        .expect("decodificar")
        .expect("com o decodificador armado, o mesmo quadro-chave tem de abrir");

    assert_eq!(voltou.largura(), Resolucao::P540.largura());
    assert_eq!(voltou.altura(), Resolucao::P540.altura());
    assert_eq!(voltou.luma().len(), 960 * 540);
    assert_eq!(voltou.croma_u().len(), 480 * 270);

    // E a imagem que voltou é a que foi: H.264 é com perdas, então o que se
    // afirma é a forma. Um plano de luma inteiro no mesmo valor seria o
    // resultado de os planos terem sido embaralhados ou o passo, perdido.
    let claros = voltou.luma().iter().filter(|&&v| v > 128).count();
    assert!(
        claros > 960 * 540 / 5 && claros < 960 * 540 * 4 / 5,
        "a luma que voltou não tem as duas metades do xadrez: {claros} pixels claros"
    );
}

#[test]
fn sem_pedido_nao_ha_quadro_chave_depois_do_primeiro() {
    // §3.3: quadro-chave **sob demanda**, não periódico — numa conversa entre
    // dois pares não há quem entre no meio da transmissão. O preço do
    // periódico está medido: forçar um a cada 2 s tira 21% dos quadros por
    // segundo e sobe o descarte de 16,2% para 17,6%, para um receptor que não
    // precisava.
    //
    // **Este teste não guarda a linha `intra_period = Some(0)`** — tirá-la
    // deixa isto verde, porque o padrão do OpenH264 hoje já é zero, e a mutação
    // foi feita para descobrir isso. O que ele guarda é a propriedade em si, de
    // quem quer que ela venha: um período que aparecesse por mudança de padrão,
    // por um `set_config` novo ou por uma opção acrescentada sem pensar cai
    // aqui.
    let Some(biblioteca) = biblioteca() else {
        return;
    };

    let mut codificador = Codificador::novo(
        &biblioteca,
        ConfigDoCodificador {
            resolucao: Resolucao::P540,
            cadencia: Cadencia::Q30,
            teto_bps: 0,
        },
    )
    .expect("armar o codificador");

    let mut chaves = 0_usize;
    // Mais de dois segundos de vídeo a 30 quadros: se houvesse período, ele já
    // teria batido pelo menos duas vezes.
    for passo in 0..90 {
        if let Some(saida) = codificador
            .codificar(&quadro_com_textura(Resolucao::P540, passo), false)
            .expect("codificar")
        {
            if saida.chave {
                chaves += 1;
            }
        }
    }
    assert_eq!(chaves, 1, "só o primeiro quadro pode ser chave sem pedido");
}

#[test]
fn quem_recebe_pede_e_recebe_um_quadro_chave() {
    // A outra metade do §3.3: o pedido existe e funciona. Sem ele, quem chega
    // depois de uma perda fica sem imagem para sempre.
    let Some(biblioteca) = biblioteca() else {
        return;
    };

    let mut codificador = Codificador::novo(&biblioteca, ConfigDoCodificador::default())
        .expect("armar o codificador");

    for passo in 0..5 {
        codificador
            .codificar(&quadro_com_textura(Resolucao::P720, passo), false)
            .expect("codificar");
    }
    let pedido = codificador
        .codificar(&quadro_com_textura(Resolucao::P720, 5), true)
        .expect("codificar")
        .expect("um quadro-chave pedido não pode ser pulado pelo controle de taxa");

    assert!(pedido.chave);
}

#[test]
fn o_teto_de_banda_muda_sem_derrubar_o_fluxo() {
    // §3.2 virando código: quando o sinal da voz cai de faixa, quem baixa é o
    // vídeo — e baixar não pode custar um quadro-chave de 65 KiB, que são
    // 446 ms do orçamento inteiro. Por isso é `SetOption` e não reconstrução.
    let Some(biblioteca) = biblioteca() else {
        return;
    };

    let mut codificador = Codificador::novo(&biblioteca, ConfigDoCodificador::default())
        .expect("armar o codificador");
    assert_eq!(
        codificador.teto_bps(),
        seele_video::codec::TETO_DA_PROVA_BPS
    );

    // Um quadro antes de mexer no teto, e ele é o que faz este teste dizer
    // alguma coisa: **o primeiro quadro de um codificador é sempre chave**, por
    // construção — sem gastá-lo aqui, o quadro conferido lá embaixo seria o
    // primeiro, e a asserção reprovaria o codificador por obedecer ao formato.
    let primeiro = codificador
        .codificar(&quadro_com_textura(Resolucao::P720, 6), false)
        .expect("codificar o primeiro")
        .expect("o primeiro quadro não é pulado pelo controle de taxa");
    assert!(primeiro.chave, "o primeiro quadro de um fluxo é chave");

    codificador.ajustar_teto(400_000).expect("baixar o teto");
    assert_eq!(codificador.teto_bps(), 400_000);

    let depois = codificador
        .codificar(&quadro_com_textura(Resolucao::P720, 7), false)
        .expect("codificar depois de mudar o teto");
    assert!(
        depois.is_none_or(|q| !q.chave),
        "mudar o teto não pode custar um quadro-chave"
    );
}

#[test]
fn um_quadro_do_tamanho_errado_e_recusado_por_nome() {
    let Some(biblioteca) = biblioteca() else {
        return;
    };

    let mut codificador = Codificador::novo(&biblioteca, ConfigDoCodificador::default())
        .expect("armar o codificador");

    // A binding devolveria «invalid input YUV size», que não diz qual dos dois
    // lados errou nem em quanto.
    let erro = codificador
        .codificar(&quadro_com_textura(Resolucao::P540, 0), false)
        .expect_err("540p num codificador armado para 720p");

    assert_eq!(
        erro,
        seele_video::ErroDeVideo::QuadroDeTamanhoErrado {
            esperado: (1280, 720),
            recebido: (960, 540),
        }
    );
}

#[test]
fn o_modulo_que_esta_nesta_maquina_bate_com_o_hash_fixado() {
    // O §2 manda fixar e conferir. Fixar um hash e nunca conferi-lo contra o
    // arquivo de verdade é a maneira mais educada de ter um hash errado no
    // repositório durante meses.
    let Some(modulo) = modulo::publicado_para_este_sistema() else {
        eprintln!("PULADO: não há módulo publicado para este alvo.");
        return;
    };
    let Ok(caminho) = modulo::procurar_em(&pastas()) else {
        eprintln!("PULADO: o módulo não está nesta máquina.");
        return;
    };

    let bytes = std::fs::read(&caminho).expect("ler o módulo");
    assert_eq!(bytes.len() as u64, modulo.bytes_expandido);
    modulo
        .conferir_expandido(&bytes)
        .expect("o módulo em disco tem de ser o que este build fixou");
}

/// O `profile_idc` que o SPS declara, ou `None` se não houver SPS aqui.
///
/// É o fio e não a API de nenhum sistema: 66 é Baseline, 77 é Main, 100 é High.
/// O byte seguinte ao cabeçalho de um NAL de tipo 7 é ele.
fn profile_idc(annex_b: &[u8]) -> Option<u8> {
    let mut i = 0;
    while i + 5 < annex_b.len() {
        let (salto, cabeca) = match annex_b.get(i..i + 4) {
            Some([0, 0, 0, 1]) => (4, i + 4),
            Some([0, 0, 1, _]) => (3, i + 3),
            _ => {
                i += 1;
                continue;
            }
        };
        if annex_b.get(cabeca).is_some_and(|b| b & 0x1F == 7) {
            return annex_b.get(cabeca + 1).copied();
        }
        i += salto;
    }
    None
}

/// O codificador que o produto arma não pode sair em Baseline.
///
/// # Por que isto é teste, e por que o número está no fio
///
/// Baseline é CAVLC, e o caminho de **software** deste crate usa CABAC desde
/// 2026-08-31 — `examples/entropia.rs` mediu 13,4% menos bytes a 540p e 15,7% a
/// 720p. Um codificador do sistema em Baseline entrega **pior que o de
/// software** pelo mesmo teto, que é o avesso da razão de ele existir.
///
/// No Windows era o caso: `fn tipo` montava o tipo de saída sem
/// `MF_MT_MPEG2_PROFILE`, e o codificador H.264 da Microsoft sem perfil
/// declarado entrega Baseline. Foi o relato «está mais pixelado que antes».
///
/// O teste lê o `profile_idc` do SPS em vez de perguntar à API porque é o fio
/// que decide: o que o outro lado vê é isto, em qualquer sistema.
#[test]
fn o_codificador_armado_nao_sai_em_baseline() {
    let Some(biblioteca) = biblioteca() else {
        return;
    };
    let mut codificador = armar(
        &biblioteca,
        ConfigDoCodificador {
            resolucao: Resolucao::P540,
            cadencia: Cadencia::Q30,
            teto_bps: 1_200_000,
        },
    )
    .expect("armar o codificador");

    let quadro = QuadroI420::preto(960, 540);
    let mut chave = None;
    // O controle de taxa pode pular quadro, e `Ok(None)` não é erro: insiste-se
    // até sair um, que é o que `codec.rs` manda quem chama fazer.
    for _ in 0..30 {
        if let Some(saida) = codificador.codificar(&quadro, true).expect("codificar") {
            chave = Some(saida);
            break;
        }
    }
    let chave = chave.expect("trinta tentativas e nenhum quadro saiu");
    assert!(chave.chave, "o primeiro quadro pedido tem de ser chave");

    let perfil = profile_idc(&chave.bytes).expect("um quadro-chave carrega SPS");
    eprintln!(
        "PERFIL NO FIO: profile_idc={perfil} ({}), quadro-chave de {} bytes",
        match perfil {
            66 => "Baseline",
            77 => "Main",
            100 => "High",
            _ => "?",
        },
        chave.bytes.len(),
    );
    assert_ne!(
        perfil, 66,
        "o codificador armado saiu em Baseline/CAVLC, atrás do caminho de software"
    );
    assert!(
        perfil == 77 || perfil == 100,
        "perfil inesperado no fio: {perfil}"
    );

    // **E o outro lado abre.** Uma economia que só nós entendêssemos seria
    // incompatibilidade e não economia — é a razão 4 do §2, e é ela que decide
    // entre Main e High enquanto o `Decodificador` for o do OpenH264.
    //
    // **O mesmo quadro-chave duas vezes**, como `o_quadro_volta_do_outro_lado`
    // já documenta neste arquivo: o OpenH264 devolve `None` na primeira chamada,
    // que é ele armando-se com o SPS, e a imagem sai na segunda. Afirmar
    // `is_some()` na primeira acusa incompatibilidade onde não há nenhuma — foi
    // o que este teste fez na primeira versão, e o `profile_idc=77` do
    // Media Foundation levou a culpa por um quadro que só faltava repetir.
    let mut decodificador = Decodificador::novo(&biblioteca).expect("armar o decodificador");
    let _ = decodificador
        .decodificar(&chave.bytes)
        .expect("armar o decodificador com o SPS");
    assert!(
        decodificador
            .decodificar(&chave.bytes)
            .expect("decodificar o quadro-chave")
            .is_some(),
        "o quadro-chave saiu num perfil que este decodificador não abre"
    );
}

/// Armar o codificador custa o mesmo na thread do teste e numa thread nova.
///
/// # Por que esta pergunta existe
///
/// O §2 manda o codificador morar numa **thread própria**, e `crate::bomba` a
/// cria com `thread::spawn`. Os sete testes daquele módulo reprovam por tempo:
/// a bomba se dá cinco segundos para emitir o primeiro evento e leva dezenove.
/// Cronometrado, o gasto é de uma chamada só — `VTCompressionSessionCreate` —,
/// e todo o resto do caminho é microssegundo.
///
/// Falta saber se os dezenove segundos são **da thread** ou **do ambiente de
/// teste**, e a diferença decide se isto é um teste para consertar ou um defeito
/// que alcança quem usa: se for da thread, começar a compartilhar no macOS e
/// cada troca de degrau da escada custam dezenove segundos em produção.
///
/// **Aquece antes de medir.** A primeira sessão de um processo paga a subida do
/// serviço de codificação do sistema, e cobrá-la da primeira medida faria a
/// ordem das duas decidir o resultado.
///
/// Não reprova por tempo, pelo mesmo motivo que `qualidade-do-codec.rs` não
/// reprova por PSNR: o número muda por máquina. Ele vai para a saída.
#[test]
fn armar_o_codificador_custa_o_mesmo_em_qualquer_thread() {
    let Some(biblioteca) = biblioteca() else {
        return;
    };
    let config = ConfigDoCodificador {
        resolucao: Resolucao::P1080,
        cadencia: Cadencia::Q30,
        teto_bps: 12_000_000,
    };

    // O aquecimento, descartado.
    drop(armar(&biblioteca, config).expect("armar para aquecer"));

    let marca = Instant::now();
    drop(armar(&biblioteca, config).expect("armar na thread do teste"));
    let na_thread_do_teste = marca.elapsed();

    let emprestada = biblioteca.clone();
    let numa_thread_nova = std::thread::spawn(move || {
        let marca = Instant::now();
        drop(armar(&emprestada, config).expect("armar na thread nova"));
        marca.elapsed()
    })
    .join()
    .expect("a thread do codificador");

    eprintln!(
        "\n  MEDIDA armar 1080p:\n    thread do teste: {na_thread_do_teste:?}\n    \
         thread nova:     {numa_thread_nova:?}\n"
    );
}
