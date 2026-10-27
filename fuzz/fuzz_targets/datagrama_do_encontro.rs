//! O fuzz do datagrama do ponto de encontro, nas três versões (`SEELE-ENC/1`,
//! `SEELE-ENC/2`, ADR 0062, e `SEELE-ENC/3`, ADR 0063).
//!
//! Desde a 0.17 o anfitrião lê o ENC/3 **no mesmo socket** do WebRTC (a 8383,
//! separada pelo primeiro byte), então um `CHAMADO` forjado chega a ele
//! antes de qualquer conferência, e é ele que manda os furos. O que este alvo
//! prende para o ENC/3: o tamanho fixo, nenhum pedido lido como resposta, os
//! tetos de candidatos e dicas, e a ida e volta pelos montadores.
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
    analisar, analisar2, analisar3, atende, chama, ler_aqui, ler_resposta2, ler_resposta3, mora3,
    moro, moro2, moro3, onde, quem, quem2, quem3, resposta2 as montar_resposta2,
    resposta3 as montar_resposta3, serve_de_candidato, serve_de_dica, Pedido, Pedido2, Pedido3,
    Resposta2, Resposta3, LIMITE_DA_MARCA, LIMITE_DE_CANDIDATOS, LIMITE_DE_DICAS, TAMANHO,
    TAMANHO2, TAMANHO3,
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

    // O ENC/3. O tamanho é o do ENC/2, e quem separa as duas é a magia: um
    // datagrama nunca é lido nas duas versões.
    let pedido3 = analisar3(dados);
    let resposta3 = ler_resposta3(dados);
    if pedido3.is_some() || resposta3.is_some() {
        assert_eq!(dados.len(), TAMANHO3);
    }
    assert!(pedido3.is_none() || resposta3.is_none());
    assert!(pedido3.is_none() || (pedido2.is_none() && !leu_pedido));
    assert!(resposta3.is_none() || (resposta2.is_none() && aqui.is_none()));
    match &pedido3 {
        Some(Pedido3::Moro3(registro)) => {
            assert!(registro.dicas.len() <= LIMITE_DE_DICAS);
            assert_eq!(analisar3(&moro3(registro)), pedido3);
        }
        Some(Pedido3::Mora3 { marca, cookie }) => {
            assert_eq!(analisar3(&mora3(marca, cookie.as_ref())), pedido3);
        }
        Some(Pedido3::Quem3 { marca }) => assert_eq!(analisar3(&quem3(marca)), pedido3),
        Some(Pedido3::Chama(pedido)) => {
            // Os candidatos que o anfitrião vai furar: um teto e a regra
            // (risco R2 da revisão do coordenador).
            assert!(pedido.candidatos.len() <= LIMITE_DE_CANDIDATOS);
            assert!(pedido.candidatos.iter().all(|c| serve_de_candidato(*c)));
            assert_eq!(analisar3(&chama(pedido)), pedido3);
        }
        Some(Pedido3::Atende(resposta)) => {
            assert!(resposta.dicas.iter().all(|dica| serve_de_dica(*dica)));
            assert_eq!(analisar3(&atende(resposta)), pedido3);
        }
        None => {}
    }
    if let Some(resposta) = &resposta3 {
        match resposta {
            Resposta3::Aqui { dicas, .. } => {
                assert!(dicas.len() <= LIMITE_DE_DICAS);
                assert!(dicas.iter().all(|dica| serve_de_dica(*dica)));
            }
            // O `CHAMADO` é o que o anfitrião recebe e fura: os candidatos de
            // quem liga mais o que o ponto viu, e nada além disso.
            Resposta3::Chamado(chamado) => {
                assert!(chamado.candidatos.len() <= LIMITE_DE_CANDIDATOS + 1);
                assert!(chamado.candidatos.iter().all(|c| serve_de_candidato(*c)));
            }
            _ => {}
        }
        assert_eq!(ler_resposta3(&montar_resposta3(resposta)), resposta3);
    }
});
