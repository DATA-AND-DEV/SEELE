//! A troca de dispositivo que volta `Ok` tem de ter acontecido.
//!
//! Não há como abrir uma placa de som num teste, então o que se prende aqui
//! é a **forma** da conferência: que ela exista no caminho dos dois lados, e
//! que ela compare o pedido com o aberto em vez de confiar no `Ok`.
//!
//! O defeito que ela existe para acusar foi relatado assim: «mostra EM USO
//! num e ESCOLHIDO no que eu escolhi, mas não muda». `EM USO` sai do
//! dispositivo que o `Voice` abriu e `ESCOLHIDO` da preferência gravada —
//! os dois discordando é o produto dizendo, sem saber, que a ordem foi
//! gravada e não cumprida. Nada no caminho devolvia erro.

const FONTE: &str = include_str!("../lib.rs");

fn corpo(assinatura: &str) -> &'static str {
    let depois = FONTE
        .split(assinatura)
        .nth(1)
        .unwrap_or_else(|| panic!("`{assinatura}` mudou de forma"));
    depois
        .split("\n    }")
        .next()
        .unwrap_or_else(|| panic!("`{assinatura}` nunca fecha"))
}

/// Este continua lendo texto-fonte, e de propósito.
///
/// O que ele guarda não é o que a conferência decide — isso agora é
/// comportamento, no `mod comportamento` logo abaixo. O que ele guarda é a
/// casca **chamar** a conferência: `set_capture_device` e a irmã dela
/// abrem aparelho de verdade, e conformidade roda sem placa de som, então
/// não há como exercitá-las aqui. Quem apaga a chamada volta a devolver
/// `Ok` para uma troca que não aconteceu, e nenhum tipo acusa isso.
#[test]
fn as_duas_trocas_conferem_o_que_pediram() {
    for assinatura in [
        "pub fn set_capture_device(&self, device: Option<String>)",
        "pub fn set_playback_device(&self, device: Option<String>)",
    ] {
        let corpo = corpo(assinatura);
        assert!(
            corpo.contains("conferir_troca"),
            "`{assinatura}` devolve `Ok` sem conferir se a troca aconteceu, \
                 e um sucesso que não é sucesso é o defeito que custou três \
                 versões nesta casca:\n{corpo}"
        );
    }
}

/// A conferência, exercitada pelo que ela **faz**.
///
/// Substitui o guarda que lia o corpo de `conferir_troca` com
/// `include_str!` e conferia se certas palavras estavam lá. Aquele guarda
/// passaria com a comparação invertida, com o lado trocado, e com um `Ok`
/// devolvido sempre — nenhuma dessas três coisas é texto que falte, e todas
/// as três são o defeito que ele existe para acusar.
mod comportamento {
    use super::super::{conferencia, ConnectionError, Lado};

    #[test]
    fn abrir_o_que_se_pediu_passa() {
        assert!(conferencia(Some("fone-usb"), Some("fone-usb"), Lado::Saida).is_ok());
    }

    #[test]
    fn abrir_outro_aparelho_e_recusado_pelo_lado_certo() {
        // O defeito relatado: «mostra EM USO num e ESCOLHIDO no que eu
        // escolhi, mas não muda». Voltar `Ok` aqui é o produto afirmando
        // que fez o que não fez.
        assert_eq!(
            conferencia(Some("fone-usb"), Some("caixas-da-mesa"), Lado::Saida),
            Err(ConnectionError::PlaybackDeviceGone)
        );
        assert_eq!(
            conferencia(
                Some("microfone-bom"),
                Some("microfone-do-laptop"),
                Lado::Entrada
            ),
            Err(ConnectionError::CaptureDeviceGone)
        );
    }

    #[test]
    fn nao_abrir_nada_tambem_e_recusado() {
        // Um aparelho que o sistema não descreve não é um aparelho que
        // atende ao pedido: `None` não é «qualquer um serve».
        assert_eq!(
            conferencia(Some("fone-usb"), None, Lado::Saida),
            Err(ConnectionError::PlaybackDeviceGone)
        );
    }

    #[test]
    fn sem_pedido_qualquer_aparelho_que_abra_esta_certo() {
        // O caso mais comum de todos: o padrão da máquina. Exigir igualdade
        // aqui recusaria toda sessão que nunca escolheu nada.
        assert!(conferencia(None, Some("qualquer-um"), Lado::Entrada).is_ok());
        assert!(conferencia(None, None, Lado::Saida).is_ok());
    }
}
