//! O endereço de hoje entra na corrida com a chave de pino que os outros
//! candidatos teriam. Era o que `ja_esta_na_corrida` garantia, quando o
//! `connect` do app comparava os textos antes de chamar a ponte. Desde a
//! tarefa 14b da F7, quem compara é a `Chegada`, pela chave de pino, e a
//! chave sai daqui.

use super::{candidato_do_quarto, resolve, ConnectConfig};

fn config() -> ConnectConfig {
    ConnectConfig {
        server: "casa.exemplo:8383".into(),
        alternate_servers: Vec::new(),
        nickname: "pessoa".into(),
        home: String::new(),
        join_secret: None,
        expected_fingerprint: Some("ab".repeat(32)),
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

fn chave_de(escrito: &str) -> String {
    resolve(escrito).expect("um endereço escrito resolve").2
}

#[test]
fn o_proprio_alvo_tem_a_mesma_chave() {
    let alvo = "187.255.97.152:9368";
    let de_hoje = candidato_do_quarto(&config(), alvo.parse().unwrap(), &chave_de(alvo));
    assert_eq!(
        de_hoje.chave_do_pin,
        chave_de(alvo),
        "o endereço que o quarto devolveu é o próprio alvo e ganhou outra chave: a \
             `Chegada` o poria de novo na corrida, com dois `Hello` para o mesmo servidor"
    );
}

#[test]
fn a_chave_e_a_canonica_e_nao_o_texto() {
    // O alvo escrito sem a porta padrão, e um IPv6 também sem ela.
    for (escrito, de_hoje) in [
        ("192.168.50.30", "192.168.50.30:8383"),
        ("[2001:db8::1]", "[2001:db8::1]:8383"),
    ] {
        let candidato =
            candidato_do_quarto(&config(), de_hoje.parse().unwrap(), &chave_de(escrito));
        assert_eq!(
            candidato.chave_do_pin,
            chave_de(escrito),
            "`{escrito}` e o `{de_hoje}` que o quarto deu têm a mesma chave de pino"
        );
    }
}

#[test]
fn a_porta_nova_e_outra_chave_e_o_pino_dela_nao_prova_o_servidor() {
    let alvo = "187.255.97.152:9368";
    let de_hoje = candidato_do_quarto(
        &config(),
        "187.255.97.152:9400".parse().unwrap(),
        &chave_de(alvo),
    );
    assert_ne!(
        de_hoje.chave_do_pin,
        chave_de(alvo),
        "a porta nova que o NAT deu ao servidor ficou com a chave do alvo, e a resposta do \
             quarto não entraria na corrida"
    );
    assert!(
        !de_hoje.o_pino_prova_o_servidor,
        "o pino do endereço que o quarto deu passou a provar o servidor: o quarto pode \
             apontar para outro servidor que esta máquina já fixou ali"
    );
}
