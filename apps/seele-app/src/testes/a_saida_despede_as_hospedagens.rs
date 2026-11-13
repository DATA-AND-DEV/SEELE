use super::{despedir_das_hospedagens, Hospedado, Session};
use seele_server::hospedagem::Hospedagem;

#[tokio::test]
async fn as_duas_saem_da_sessao_e_se_despedem() {
    // O comportamento de cada despedida é da hospedagem (`quem_hospeda_fecha.rs`
    // e `sala_pessoal.rs`, no `seele-conformance`). O que é deste arquivo:
    // a saída leva as duas, e não só a que existia antes do Plano 2.
    let sessao = Session::default();
    // Duas salas, e não um servidor guardado e uma sala: este arquivo só
    // sobe hospedagem pelo ponto padrão (o guarda
    // `o_ponto_de_encontro_de_quem_hospeda`), e um teste não pergunta à VPS
    // de produção. O que se prova aqui é o lugar de cada uma na sessão.
    let guardado = Hospedagem::iniciar_sala(0, &[2; 32], "casa", None)
        .await
        .expect("a primeira não subiu");
    let sala = Hospedagem::iniciar_sala(0, &[1; 32], "dona", None)
        .await
        .expect("a sala não subiu");
    {
        let mut hospedagens = sessao.hospedagens.lock().expect("o cadeado");
        hospedagens.guardado = Some(Hospedado::novo(guardado, None));
        hospedagens.pessoal = Some(Hospedado::novo(sala, None));
    }

    despedir_das_hospedagens(&sessao, std::time::Duration::from_millis(400)).await;

    let hospedagens = sessao.hospedagens.lock().expect("o cadeado");
    assert!(hospedagens.guardado.is_none(), "o guardado ficou para trás");
    assert!(hospedagens.pessoal.is_none(), "a sala ficou para trás");
}
