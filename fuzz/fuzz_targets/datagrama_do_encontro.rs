//! O fuzz do datagrama do ponto de encontro (`SEELE-ENC/1`).
//!
//! Os dois lados leem bytes de qualquer um. O ponto de encontro lê o pedido com
//! `analisar` antes de qualquer conferência — o protocolo não tem chave nenhuma
//! —, e o anfitrião lê o `AQUI` com `ler_aqui` de quem quer que tenha mandado
//! um pacote para a escuta de avisos dele. O analisador mora em `seele-proto`,
//! e não no `seele-encontro`, e é por isso que o alvo mora aqui.
//!
//! Rode com:
//! cargo +nightly fuzz run datagrama_do_encontro fuzz/corpus/datagrama_do_encontro fuzz/sementes/datagrama_do_encontro
//!
//! A pasta de corpus vem primeiro de propósito: o libFuzzer grava o que acha
//! na primeira pasta que recebe, e `fuzz/sementes/` é versionada e conferida
//! byte a byte por `crates/seele-proto/tests/sementes_do_fuzz.rs`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use seele_proto::encontro::{
    analisar, ler_aqui, moro, onde, quem, Pedido, LIMITE_DA_MARCA, TAMANHO,
};

fuzz_target!(|dados: &[u8]| {
    let pedido = analisar(dados);
    let aqui = ler_aqui(dados);

    // Todo datagrama deste protocolo tem `TAMANHO` bytes, e é essa a defesa
    // contra a amplificação: um pedido menor que a resposta que ele provoca
    // faria do ponto de encontro um amplificador.
    if pedido.is_some() || aqui.is_some() {
        assert_eq!(dados.len(), TAMANHO);
    }

    // Um `AQUI` nunca é lido como pedido. É o que sai do ponto de encontro, e
    // um ponto que respondesse a respostas seria um laço entre dois deles.
    assert!(pedido.is_none() || aqui.is_none());

    if let Some((marca, _)) = &aqui {
        assert!(!marca.texto().is_empty() && marca.texto().len() <= LIMITE_DA_MARCA);
        assert!(marca.texto().chars().all(|c| c.is_ascii_alphanumeric()));
    }

    // Os pedidos que só levam a marca se remontam pelos montadores do próprio
    // protocolo e voltam iguais: a linha cabe, porque é a mesma que entrou. O
    // `LEVE` fica de fora, porque o endereço dele é reescrito pelo `Display`
    // de `SocketAddr`, que nem sempre devolve o texto que entrou.
    match pedido {
        Some(Pedido::Onde { marca }) => {
            assert_eq!(analisar(&onde(&marca)), Some(Pedido::Onde { marca }));
        }
        Some(Pedido::Moro { marca }) => {
            assert_eq!(analisar(&moro(&marca)), Some(Pedido::Moro { marca }));
        }
        Some(Pedido::Quem { marca }) => {
            assert_eq!(analisar(&quem(&marca)), Some(Pedido::Quem { marca }));
        }
        Some(Pedido::Leve { .. }) | None => {}
    }
});
