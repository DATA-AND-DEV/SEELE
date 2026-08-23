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

use seele_video::codec::{
    Cadencia, Codificador, ConfigDoCodificador, Decodificador, QuadroI420, Resolucao,
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
            eprintln!(
                "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe isso.\n  \
                 Busque {onde} e aponte-o com SEELE_OPENH264."
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

    let voltou = decodificador
        .decodificar(&primeiro.bytes)
        .expect("decodificar")
        .expect("um quadro-chave sozinho tem de bastar para o decoder");

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
