//! A **vaga**: uma permissão que serializa os testes que levantam um servidor.
//!
//! # Por que ela existe
//!
//! A pendência 29 mediu o defeito e o defeito não é de produto: cada teste
//! desta suíte levanta um servidor QUIC de verdade — aperto de mão, TLS e
//! banco —, o `libtest` roda os testes de um binário em paralelo, e sob carga
//! os apertos de mão concorrentes estouram o `IDLE_TIMEOUT` de 20 s do
//! transporte. O sintoma é uma reprovação por rodada, **sempre num teste
//! diferente**, sempre em ~20 s, e sempre passando em menos de um segundo
//! quando o teste roda sozinho. Uma suíte assim deixa de ser evidência.
//!
//! Os números que decidiram entre alargar o prazo e serializar estão na
//! pendência 29: em série o `acceptance_m5` leva 23,02 s e passa 15 de 15; em
//! paralelo termina em ~20,01 s e reprova um teste em cerca de duas de cada
//! três rodadas. O paralelismo não compra quase nada num crate onde cada teste
//! espera a rede; serializar custa segundos e devolve a evidência.
//!
//! # Por que pela duração do teste, e não em volta do `bind`
//!
//! Tentativas anteriores erraram esse ponto, e a pendência 29 registra a
//! medida: **o que estoura é o aperto de mão, não a abertura da porta**. Um
//! guarda solto logo depois de `Daemon::bind` devolveria a vaga antes de o
//! cliente sequer tentar conectar. Por isso a permissão é tomada na primeira
//! linha do teste e só é devolvida quando o teste acaba — inclusive quando
//! acaba em pânico, porque quem a devolve é o `Drop`.
//!
//! # Por que aqui, e não em `RUST_TEST_THREADS`
//!
//! `RUST_TEST_THREADS` em `.cargo/config.toml` vale para o workspace inteiro e
//! serializaria também os ~489 testes do `seele-server`, que não têm este
//! problema e não têm por que pagar por ele. Esta permissão alcança
//! **somente** este crate, e dentro dele somente quem a pede.
//!
//! Não é um binário de teste: `cargo` só compila como teste os arquivos soltos
//! em `tests/`, e não o que está dentro de uma pasta. Quem usa faz `mod vaga;`
//! — o mesmo padrão do módulo irmão `porta`.
//!
//! # Como desarmar, para provar que ela serve
//!
//! [`VAGAS`] é a largura da permissão. Pôr `usize::MAX` ali a desarma sem
//! apagar uma linha de código, e é assim que a prova de reversão da pendência
//! 29 foi feita: com a permissão desarmada e a máquina sob carga, a suíte volta
//! a reprovar um teste de conformidade por prazo em ~20 s.

use std::sync::{Condvar, Mutex, MutexGuard};

/// Quantos testes deste crate podem levantar um servidor ao mesmo tempo.
///
/// Um. Trocar por `usize::MAX` desarma a permissão — veja o cabeçalho.
const VAGAS: usize = 1;

/// Quantas vagas estão tomadas agora, e quem avisa quando uma sobra.
static OCUPADAS: Mutex<usize> = Mutex::new(0);
static DEVOLVEU: Condvar = Condvar::new();

/// A permissão em si. Enquanto este valor existir, o teste tem a vez.
///
/// Devolvida no `Drop`, o que inclui o caminho do pânico: um teste que reprova
/// não pode levar a vaga consigo, ou a suíte inteira trava atrás dele.
#[must_use = "a permissão vale enquanto o guarda existir; soltá-la na hora não serializa nada"]
pub(crate) struct Vaga;

/// Espera a vez e a toma. Bloqueia a thread do teste, que é o que se quer.
pub(crate) fn minha() -> Vaga {
    let mut ocupadas = quantas();
    while *ocupadas >= VAGAS {
        ocupadas = DEVOLVEU
            .wait(ocupadas)
            .unwrap_or_else(|envenenado| envenenado.into_inner());
    }
    *ocupadas += 1;
    Vaga
}

impl Drop for Vaga {
    fn drop(&mut self) {
        *quantas() -= 1;
        DEVOLVEU.notify_one();
    }
}

/// O contador, atravessando o envenenamento: um teste que entra em pânico
/// segurando a trava não pode derrubar os que ainda vão rodar.
fn quantas() -> MutexGuard<'static, usize> {
    OCUPADAS
        .lock()
        .unwrap_or_else(|envenenado| envenenado.into_inner())
}
