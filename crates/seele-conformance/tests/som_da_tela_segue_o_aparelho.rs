//! O som da tela compartilhada seguindo a troca de aparelho.
//!
//! # O defeito
//!
//! `seele-core/src/video.rs` abria o *loopback* do Windows **uma vez** —
//! `CapturaDaSaida::abrir(None)`, o padrão do sistema naquele instante — e o
//! segurava pelo resto da transmissão. Trocar de fone pela bandeja no meio de
//! um compartilhamento deixava a transmissão presa ao aparelho antigo. E a
//! falha não parece falha: quem assiste ouve silêncio e conclui que o jogo
//! estava mudo.
//!
//! O aviso do `cpal` sempre esteve lá — `device::abrir_entrada` instala o mesmo
//! `retorno_de_erro` no fluxo do loopback que instala no da voz, de modo que
//! trocas e sumiços já eram contados. Faltava alguém para lê-los.
//!
//! # Por que este teste roda num Mac, sendo o defeito do Windows
//!
//! Porque a decisão foi tirada de dentro do `#[cfg(target_os = "windows")]`. Um
//! teste escrito lá dentro nunca rodaria aqui — é o caso que o `CLAUDE.md`
//! nomeia («um teste dentro de um `#![cfg(windows)]` que nunca roda no Mac»), e
//! ele já aconteceu neste repositório.
//!
//! O que corre aqui é `seele_core::som_que_segue::SomQueSegue`, que é o código
//! de produção inteiro no que diz respeito a **decidir**: ele lê o aviso do
//! aparelho aberto, conduz o `CicloDoAparelho` do supervisor — o mesmo que a
//! voz conduz, não uma segunda máquina de estados — e troca o que está aberto
//! pelo que a reabertura entregou. O que este teste finge é só a abertura, pela
//! mesma razão que a bateria da troca de aparelho da voz finge: nenhuma máquina
//! de CI tem duas placas de som.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use seele_audio::supervisor::{AvisoDeAparelho, DeviceState, Reabertura};
use seele_core::som_que_segue::{SomAberto, SomQueSegue};

/// Um loopback aberto, de mentira.
///
/// Ele carrega o **próprio** contador de avisos, e isso não é detalhe: em
/// produção cada `CapturaDaSaida::abrir` cria contadores novos e zerados, e o
/// seguidor passa a ler dos novos. Um teste que compartilhasse um contador
/// entre aberturas nunca veria a segunda troca acontecer.
struct LoopbackDeMentira {
    /// O que este aparelho entrega quando lhe pedem amostras.
    ///
    /// Uma amostra só, com um valor por aparelho: é o que permite afirmar **de
    /// qual** aparelho veio o som, que é a pergunta inteira deste arquivo.
    marca: f32,
    trocas: Arc<AtomicU64>,
    sumicos: Arc<AtomicU64>,
}

impl SomAberto for LoopbackDeMentira {
    fn tomar(&self, _teto: usize) -> Vec<f32> {
        vec![self.marca]
    }

    fn aviso(&self) -> AvisoDeAparelho {
        AvisoDeAparelho {
            trocas: self.trocas.load(Ordering::Relaxed),
            sumicos: self.sumicos.load(Ordering::Relaxed),
        }
    }
}

/// A máquina de quem transmite: qual saída é o padrão agora.
struct MaquinaDeQuemTransmite {
    padrao: Option<&'static str>,
    aberturas: u32,
}

impl MaquinaDeQuemTransmite {
    fn abre(aparelho: &'static str) -> LoopbackDeMentira {
        LoopbackDeMentira {
            marca: match aparelho {
                "fone-usb" => 1.0,
                "caixas-da-mesa" => 2.0,
                _ => 3.0,
            },
            trocas: Arc::new(AtomicU64::new(0)),
            sumicos: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Reabertura for MaquinaDeQuemTransmite {
    type Aberto = LoopbackDeMentira;
    type Erro = &'static str;

    fn reabrir(&mut self) -> Result<Self::Aberto, Self::Erro> {
        self.aberturas += 1;
        // `abrir(None)`, como a produção faz: o padrão de **agora**, e não o id
        // guardado de quando a transmissão começou.
        self.padrao
            .map(Self::abre)
            .ok_or("a máquina não está oferecendo saída nenhuma")
    }

    fn aviso_de(&self, aberto: &Self::Aberto) -> AvisoDeAparelho {
        aberto.aviso()
    }
}

/// Roda voltas do codificador por um tanto de tempo, e devolve a última amostra.
fn voltas(
    seguidor: &mut SomQueSegue<MaquinaDeQuemTransmite>,
    de_ms: f64,
    ate_ms: f64,
) -> (Vec<f32>, f64) {
    let mut agora = de_ms;
    let mut ultimo = Vec::new();
    while agora < ate_ms {
        ultimo = seguidor.tomar_em(9_600, agora);
        // Um tique de vídeo a 30 quadros. É a cadência real de quem chama.
        agora += 33.0;
    }
    (ultimo, agora)
}

#[test]
fn o_som_da_transmissao_reabre_no_padrao_novo_quando_o_sistema_troca() {
    let inicial = MaquinaDeQuemTransmite::abre("fone-usb");
    let trocas = Arc::clone(&inicial.trocas);
    let mut seguidor = SomQueSegue::novo(
        inicial,
        MaquinaDeQuemTransmite {
            // O sistema **já** elegeu as caixas: é o estado em que o `cpal`
            // manda o aviso. Abrir de novo agora tem de dar nas caixas.
            padrao: Some("caixas-da-mesa"),
            aberturas: 0,
        },
    );

    let (antes, agora) = voltas(&mut seguidor, 0.0, 100.0);
    assert_eq!(antes, vec![1.0], "o som não estava vindo do fone");

    trocas.fetch_add(1, Ordering::Relaxed);
    let (depois, _) = voltas(&mut seguidor, agora, agora + 2_000.0);

    assert_eq!(
        depois,
        vec![2.0],
        "o som da transmissão continuou vindo do aparelho antigo depois de o \
         sistema trocar de saída padrão. É o defeito inteiro: a transmissão \
         abria o loopback uma vez e ficava presa a ele até o aplicativo \
         reiniciar — e quem assiste ouve silêncio e conclui que o programa \
         estava mudo."
    );
    assert_eq!(
        seguidor.estado(),
        DeviceState::Running,
        "a reabertura aconteceu e o ciclo não voltou a dizer que está tudo bem"
    );
}

#[test]
fn a_saida_retirada_leva_o_som_para_a_que_sobrou() {
    let inicial = MaquinaDeQuemTransmite::abre("fone-usb");
    let sumicos = Arc::clone(&inicial.sumicos);
    let mut seguidor = SomQueSegue::novo(
        inicial,
        MaquinaDeQuemTransmite {
            padrao: Some("alto-falante-do-laptop"),
            aberturas: 0,
        },
    );

    let (_, agora) = voltas(&mut seguidor, 0.0, 100.0);
    // O fone sai da tomada: no macOS o cpal pausa o fluxo e devolve
    // `DeviceNotAvailable`; no Windows ele segue falando com um endpoint morto.
    // Nos dois, o contador de sumiços anda.
    sumicos.fetch_add(1, Ordering::Relaxed);

    let (depois, _) = voltas(&mut seguidor, agora, agora + 3_000.0);
    assert_eq!(
        depois,
        vec![3.0],
        "a transmissão ficou presa ao aparelho que foi retirado"
    );
}

#[test]
fn um_aviso_que_nao_anda_nao_reabre_nada() {
    // A metade que protege contra o conserto exagerado. Sem ela, um seguidor
    // que reabrisse a cada volta passaria nos testes de cima e reabriria o
    // loopback trinta vezes por segundo durante a transmissão inteira.
    let inicial = MaquinaDeQuemTransmite::abre("fone-usb");
    let mut seguidor = SomQueSegue::novo(
        inicial,
        MaquinaDeQuemTransmite {
            padrao: Some("caixas-da-mesa"),
            aberturas: 0,
        },
    );

    let (depois, _) = voltas(&mut seguidor, 0.0, 5_000.0);
    assert_eq!(
        depois,
        vec![1.0],
        "o som trocou de aparelho sem o `cpal` ter avisado nada: a transmissão \
         passou a reabrir sozinha e o aparelho de quem transmite deixou de ser \
         escolha dele"
    );
    assert_eq!(seguidor.estado(), DeviceState::Running);
}
