//! A conexão entre **duas máquinas de verdade**, e não duas pontas do loopback.
//!
//! # Por que ela precisa existir
//!
//! Tudo o mais neste crate roda em `127.0.0.1`, onde não há firewall, não há
//! roteador, não há NAT e não há pilha dupla decidindo nada. O que sobra fora
//! dali é justamente onde os relatos de campo acontecem — «tentei entrar por LAN
//! e não foi», «o Mac só dá tempo esgotado na sincronização» — e cada um deles
//! custou horas de leitura de código porque não havia como reproduzir.
//!
//! # Como se usa
//!
//! Numa máquina, sobe o servidor:
//!
//! ```text
//! seeled --escuta 0.0.0.0:8383
//! ```
//!
//! Na outra, com o endereço dela na rede:
//!
//! ```text
//! SEELE_ALVO=192.168.50.30:8383 cargo test -p seele-conformance \
//!     --test duas_maquinas -- --ignored --nocapture
//! ```
//!
//! `#[ignore]` porque ela precisa de duas máquinas e de alguém para arrumá-las:
//! um teste que reprova por falta de ambiente é um teste que se aprende a
//! ignorar, e este projeto já tem a lição escrita em três lugares.
//!
//! # O que ela responde
//!
//! Uma pergunta só, e é a que separa dois mundos: **os pacotes chegaram do outro
//! lado?** `SemResposta` diz que não — e aí o problema está no caminho, quase
//! sempre no firewall de quem hospeda. Qualquer outra resposta diz que sim, e a
//! investigação muda de lugar.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use seele_core::{Client, MemoryPinStore, PinStore};

#[tokio::test]
#[ignore = "precisa de duas máquinas: ver o cabeçalho deste arquivo"]
async fn o_caminho_ate_a_outra_maquina_esta_aberto() {
    let Ok(alvo) = std::env::var("SEELE_ALVO") else {
        panic!(
            "faltou `SEELE_ALVO`. Exemplo:\n  \
             SEELE_ALVO=192.168.50.30:8383 cargo test -p seele-conformance \\\n    \
             --test duas_maquinas -- --ignored --nocapture"
        );
    };
    let endereco: std::net::SocketAddr = alvo
        .parse()
        .unwrap_or_else(|erro| panic!("«{alvo}» não é um endereço: {erro}"));

    println!("tentando {endereco}…");
    let comecou = std::time::Instant::now();
    let resultado = Client::connect(
        endereco,
        "localhost",
        "sonda-de-duas-maquinas",
        "sonda",
        &ed25519_dalek::SigningKey::from_bytes(&[42_u8; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
        None,
    )
    .await;
    let levou = comecou.elapsed();

    match resultado {
        Ok(_) => println!("entrou em {levou:?} — o caminho está aberto."),
        Err(seele_core::ConnectError::SemResposta) => panic!(
            "os pacotes saíram e nada voltou, em {levou:?}.\n\
             O endereço existe e o outro lado não respondeu. No Windows isto é \
             quase sempre a regra de firewall: ela é por programa, e a caixa que \
             a cria nasce desmarcada no instalador.\n\
             Confira lá com:\n  \
             netsh advfirewall firewall show rule name=all dir=in verbose | findstr /I SEELE"
        ),
        // Todo o resto é notícia boa para esta pergunta: os pacotes chegaram, o
        // outro lado leu e respondeu alguma coisa. O que ele respondeu é assunto
        // de outro teste.
        Err(outro) => println!(
            "o outro lado respondeu em {levou:?}, e a resposta foi {outro:?}.\n\
             O caminho está aberto; o que impede é o que essa resposta diz."
        ),
    }
}
