//! Fechar a janela tem de avisar quem hospeda.
//!
//! Um socket UDP que some não avisa ninguém: QUIC precisa de um
//! `CONNECTION_CLOSE` dito por extenso. Sem ele, quem hospeda só descobre a
//! ausência no tempo limite de ociosidade, e até lá a pessoa continua sentada na
//! sala para todo mundo — «se eu fecho o app no Mac, o usuário não sai da sala
//! para o Windows».
//!
//! Isto não vira teste de comportamento porque exigiria subir dois processos e
//! uma rede para provar meia dúzia de linhas. O que dá para prender é a ligação:
//! que o gancho de saída existe e que ele chama quem sabe se despedir. É o mesmo
//! recurso das outras invariantes desta casa que não viram tipo.

#![allow(clippy::expect_used)]

use std::path::Path;

fn main_rs() -> String {
    let caminho = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    std::fs::read_to_string(&caminho)
        .unwrap_or_else(|erro| panic!("não li o main.rs: {erro}"))
        .replace("\r\n", "\n")
}

#[test]
fn fechar_a_janela_avisa_o_servidor() {
    let fonte = main_rs();
    assert!(
        fonte.contains("RunEvent::ExitRequested"),
        "o app deixou de tratar o fim da última janela.\n\
         Sem esse gancho o processo morre em silêncio e quem hospeda mantém a \
         pessoa sentada na sala até o tempo limite de ociosidade."
    );
    let gancho = fonte
        .find("RunEvent::ExitRequested")
        .expect("já conferido acima");
    let despedida = fonte
        .find("fn despedir_se")
        .expect("a função que avisa o servidor sumiu");
    let chamada = fonte
        .find("despedir_se(handle)")
        .expect("o gancho de saída não chama mais quem avisa o servidor");
    assert!(
        gancho < chamada && despedida > 0,
        "o gancho de saída existe mas não é ele que chama a despedida"
    );
    assert!(
        fonte.contains("connection.disconnect()"),
        "a despedida deixou de encerrar a conexão, que é o que faz o QUIC \
         mandar o `CONNECTION_CLOSE`"
    );
}

#[test]
fn a_despedida_espera_o_quadro_sair() {
    // `disconnect` só **pede** o fim; o `CONNECTION_CLOSE` é um datagrama, e um
    // processo que sai no mesmo instante pode sair antes de ele chegar ao cartão
    // de rede. A espera é curta de propósito — muito para uma LAN, pouco para
    // alguém notar ao fechar uma janela.
    let fonte = main_rs();
    let despedida = fonte
        .split("fn despedir_se")
        .nth(1)
        .expect("a função que avisa o servidor sumiu");
    assert!(
        despedida.contains("sleep"),
        "a despedida deixou de esperar o quadro sair.\n\
         Sem a espera o processo pode morrer antes de o aviso deixar a máquina, \
         e o defeito volta sem nada mudar de aparência."
    );
}
