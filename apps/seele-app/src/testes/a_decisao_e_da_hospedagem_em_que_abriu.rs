use super::{
    persistence_da_hospedagem, persistence_do_ato, FalhaNaPortaria, Hospedado, Hospedagens,
};
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;

#[tokio::test]
async fn a_decisao_aberta_em_a_e_recusada_em_b_e_aceita_em_a() {
    let a = Hospedagem::iniciar(0, Location::Memory, "A", None, |_| Ok(()))
        .await
        .expect("A subiu");
    let mut hospedagens = Hospedagens {
        guardado: Some(Hospedado::novo(a, None)),
        pessoal: None,
    };
    let de_a = hospedagens.guardado.as_ref().expect("A").identidade();
    assert!(
        persistence_da_hospedagem(&hospedagens, &de_a).is_ok(),
        "a decisão aberta em A foi recusada em A"
    );

    // Parar A, subir B (o mesmo servidor guardado, `None`).
    if let Some(parado) = hospedagens.guardado.take() {
        parado.hospedagem.encerrar().await;
    }
    assert!(matches!(
        persistence_da_hospedagem(&hospedagens, &de_a),
        Err(FalhaNaPortaria::NaoEstaHospedando)
    ));
    let b = Hospedagem::iniciar(0, Location::Memory, "B", None, |_| Ok(()))
        .await
        .expect("B subiu");
    hospedagens.guardado = Some(Hospedado::novo(b, None));
    let de_b = hospedagens.guardado.as_ref().expect("B").identidade();
    assert_eq!(
        de_a.servidor, de_b.servidor,
        "o teste quer o mesmo servidor guardado"
    );
    match persistence_da_hospedagem(&hospedagens, &de_a) {
        Err(FalhaNaPortaria::HospedagemNaoConfere { agora }) => {
            assert_eq!(agora, de_b, "a recusa não diz qual hospedagem está de pé");
        }
        Err(outra) => panic!("a decisão de A em B foi recusada pelo motivo errado: {outra:?}"),
        Ok(_) => panic!("a decisão aberta em A foi aplicada em B"),
    }
    assert!(persistence_da_hospedagem(&hospedagens, &de_b).is_ok());
    // **O ato sem hospedagem é recusado** (rodada 3), e não cai no
    // guardado de pé; com ela, passa pelo mesmo caminho dos comandos.
    match persistence_do_ato(&hospedagens, None) {
        Err(FalhaNaPortaria::HospedagemNaoConfere { agora }) => assert_eq!(agora, de_b),
        Err(outra) => panic!("o ato sem hospedagem foi recusado pelo motivo errado: {outra:?}"),
        Ok(_) => panic!("o ato sem hospedagem caiu no guardado de pé"),
    }
    assert!(persistence_do_ato(&hospedagens, Some(&de_b)).is_ok());
    assert!(matches!(
        persistence_do_ato(&hospedagens, Some(&de_a)),
        Err(FalhaNaPortaria::HospedagemNaoConfere { .. })
    ));
    if let Some(parado) = hospedagens.guardado.take() {
        parado.hospedagem.encerrar().await;
    }
}
