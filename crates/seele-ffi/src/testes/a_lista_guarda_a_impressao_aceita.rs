use super::{impressao_a_guardar, Trust};

const DO_SERVIDOR: &str = "aaaa1111";
const DE_OUTRO: &str = "bbbb2222";

#[test]
fn um_link_que_discorda_do_pin_nao_vai_para_a_lista() {
    // A conexão ficou de pé com a chave fixada (ADR 0003), e é ela que vale
    // guardar. A do link é de outro servidor.
    let veredito = Trust::InviteDisagrees {
        expected: DE_OUTRO.into(),
        offered: DO_SERVIDOR.into(),
    };
    assert_eq!(
        impressao_a_guardar(&veredito, Some(DE_OUTRO), None),
        Some(DO_SERVIDOR.to_owned()),
        "a lista guardou a impressão do link que discordava do pin: a volta pela \
             lista passa a esperar a chave de outro servidor e recusa o verdadeiro em \
             todo endereço sem pin"
    );
}

#[test]
fn a_volta_pelo_alvo_que_discorda_do_pin_grava_a_ofertada() {
    // Sem link, quem discorda do pin é a guardada. `InviteDisagrees` só nasce
    // num alvo de escopo público (`seele_core::tofu::TofuVerifier::decide`,
    // com o pino que `build_destino` diz provar o servidor): ali o pin prova
    // a continuidade do servidor da entrada (ADR 0003), e a ofertada é a
    // chave dele. É assim que uma lista gravada pela 0.15.0, com a impressão
    // de um link que discordava do pin, se cura na primeira volta pelo alvo.
    // Num alvo de LAN, a mesma volta é recusada dentro do TLS
    // (`num_alvo_de_lan_o_pino_que_confere_nao_passa_por_cima_da_esperada`).
    let veredito = Trust::InviteDisagrees {
        expected: DE_OUTRO.into(),
        offered: DO_SERVIDOR.into(),
    };
    assert_eq!(
        impressao_a_guardar(&veredito, None, Some(DE_OUTRO)),
        Some(DO_SERVIDOR.to_owned()),
        "a volta pelo alvo, onde o pin prova o servidor, deixou na lista a impressão que \
             o pin desmente: uma lista envenenada pela 0.15.0 continua recusando o servidor \
             verdadeiro em todo endereço sem pin"
    );
}

#[test]
fn o_que_foi_conferido_vai_para_a_lista() {
    let conferido = Trust::FirstContactVerified {
        fingerprint: DO_SERVIDOR.into(),
    };
    assert_eq!(
        impressao_a_guardar(&conferido, Some(DO_SERVIDOR), None),
        Some(DO_SERVIDOR.to_owned()),
        "o primeiro contato conferido pelo link não foi para a lista"
    );
    // O que o app passa numa volta pela lista: sem link, com a guardada.
    assert_eq!(
        impressao_a_guardar(&conferido, None, Some(DO_SERVIDOR)),
        Some(DO_SERVIDOR.to_owned()),
        "o primeiro contato conferido pela guardada, num endereço novo, não foi \
             para a lista"
    );
    assert_eq!(
        impressao_a_guardar(&Trust::Known, Some(DO_SERVIDOR), None),
        Some(DO_SERVIDOR.to_owned()),
        "a impressão do link que concorda com o pin não foi para a lista"
    );
    assert_eq!(
        impressao_a_guardar(&Trust::Known, None, Some(DO_SERVIDOR)),
        Some(DO_SERVIDOR.to_owned()),
        "a guardada que concorda com o pin, numa volta pela lista, deixou de ser \
             regravada, e a lista fica com uma impressão que ninguém confirmou hoje"
    );
}

#[test]
fn o_que_ninguem_conferiu_deixa_a_lista_como_estava() {
    assert_eq!(
        impressao_a_guardar(&Trust::Known, None, None),
        None,
        "a volta sem impressão conferida inventou uma impressão para a lista"
    );
    assert_eq!(
        impressao_a_guardar(
            &Trust::FirstContact {
                fingerprint: DO_SERVIDOR.into()
            },
            None,
            None
        ),
        None,
        "um primeiro contato cego foi para a lista como se tivesse sido \
             conferido: a volta por outro endereço passaria a conferir contra uma \
             chave que ninguém prometeu"
    );
    assert_eq!(
        impressao_a_guardar(
            &Trust::InviteRefused {
                expected: DE_OUTRO.into(),
                offered: DO_SERVIDOR.into()
            },
            Some(DE_OUTRO),
            None
        ),
        None,
        "uma recusa deixou impressão na lista"
    );
}
