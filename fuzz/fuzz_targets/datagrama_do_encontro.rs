//! O fuzz do datagrama do ponto de encontro, nas duas versões (`SEELE-ENC/1` e
//! `SEELE-ENC/2`, ADR 0062).
//!
//! Os dois lados leem bytes de qualquer um. O ponto de encontro lê o pedido com
//! `analisar` e `analisar2` antes de qualquer conferência — a assinatura do
//! `MORO2` só é conferida depois de lida —, e o anfitrião e quem liga leem o
//! `AQUI` e as respostas do ENC/2 de quem quer que tenha mandado um pacote para
//! eles. O analisador mora em `seele-proto`,
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
    analisar, analisar2, ler_aqui, ler_resposta2, moro, moro2, onde, quem, quem2,
    resposta2 as montar_resposta2, serve_de_dica, Pedido, Pedido2, Resposta2, LIMITE_DA_MARCA,
    LIMITE_DE_DICAS, TAMANHO, TAMANHO2,
};

fuzz_target!(|dados: &[u8]| {
    let pedido = analisar(dados);
    let aqui = ler_aqui(dados);
    // Antes do `match` de baixo, que consome o pedido.
    let leu_pedido = pedido.is_some();

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

    // O ENC/2: o mesmo, com o tamanho dele. As duas versões não se leem, e uma
    // resposta nunca é lida como pedido.
    let pedido2 = analisar2(dados);
    let resposta2 = ler_resposta2(dados);
    if pedido2.is_some() || resposta2.is_some() {
        assert_eq!(dados.len(), TAMANHO2);
    }
    assert!(pedido2.is_none() || resposta2.is_none());
    assert!(!leu_pedido || pedido2.is_none());
    assert!(aqui.is_none() || resposta2.is_none());

    // Cada campo tem uma escrita só, então o que se lê volta igual pelos
    // montadores, inclusive o endereço: `analisar2` só aceita o texto que o
    // `Display` escreve.
    match &pedido2 {
        Some(Pedido2::Moro2(registro)) => {
            assert!(registro.dicas.len() <= LIMITE_DE_DICAS);
            assert!(registro.dicas.iter().all(|dica| serve_de_dica(*dica)));
            assert_eq!(analisar2(&moro2(registro)), pedido2);
        }
        Some(Pedido2::Quem2 { marca }) => assert_eq!(analisar2(&quem2(marca)), pedido2),
        None => {}
    }
    if let Some(resposta) = &resposta2 {
        if let Resposta2::Aqui2 { dicas, .. } = resposta {
            assert!(dicas.len() <= LIMITE_DE_DICAS);
            assert!(dicas.iter().all(|dica| serve_de_dica(*dica)));
        }
        assert_eq!(ler_resposta2(&montar_resposta2(resposta)), resposta2);
    }
});
