//! A 19 e a 20 da F7: sair não derruba nada, e o verbo que derruba é o
//! `parar_de_hospedar`, só para o guardado. O `meu_servidor` diz à entrada
//! para onde o VOLTAR AO MEU SERVIDOR leva.

use super::{meu_servidor_em, parar_de_hospedar_em, Hospedado, MeuServidor, Session};
use seele_server::hospedagem::Hospedagem;

/// Uma sala no lugar do guardado, com a porta dela: este arquivo só sobe
/// hospedagem pelo ponto padrão (o guarda
/// `o_ponto_de_encontro_de_quem_hospeda`), e um teste não pergunta à VPS de
/// produção. O que se prova aqui é o lugar do guardado na sessão.
async fn guardado_de_teste(sessao: &Session, servidor: Option<String>) -> u16 {
    let hospedagem = Hospedagem::iniciar_sala(0, &[2; 32], "casa", None)
        .await
        .expect("o guardado de teste não subiu");
    let porta = hospedagem.endereco().port();
    sessao.hospedagens.lock().unwrap().guardado = Some(Hospedado::novo(hospedagem, servidor));
    porta
}

#[tokio::test]
async fn parar_de_hospedar_derruba_o_guardado_devolve_a_porta_e_deixa_a_sala() {
    let sessao = Session::default();
    let porta = guardado_de_teste(&sessao, None).await;
    let sala = Hospedagem::iniciar_sala(0, &[1; 32], "dona", None)
        .await
        .expect("a sala não subiu");
    sessao.hospedagens.lock().unwrap().pessoal = Some(Hospedado::novo(sala, None));

    assert!(parar_de_hospedar_em(&sessao).await, "havia guardado no ar");

    {
        let hospedagens = sessao.hospedagens.lock().unwrap();
        assert!(hospedagens.guardado.is_none(), "o guardado ficou no ar");
        assert!(
            hospedagens.pessoal.is_some(),
            "PARAR DE HOSPEDAR derrubou a sala pessoal, que desce pelo «receber ligações»"
        );
    }
    assert!(
        std::net::UdpSocket::bind(("::", porta)).is_ok(),
        "o guardado parou e a porta {porta} ficou presa: hospedar de novo não subiria"
    );
    assert!(
        !parar_de_hospedar_em(&sessao).await,
        "parar duas vezes disse que havia o que parar"
    );

    let sala = sessao.hospedagens.lock().unwrap().pessoal.take();
    if let Some(sala) = sala {
        sala.hospedagem.encerrar().await;
    }
}

#[tokio::test]
async fn meu_servidor_diz_onde_voltar_e_o_nome_que_a_lista_mostra() {
    let casa = tempfile::tempdir().unwrap();
    let config = casa.path().to_str().unwrap();
    let registrado = crate::servidores::criar(config, "Casa da Rafa", "0.16.0");
    let sessao = Session::default();
    let meu = |sessao: &Session| meu_servidor_em(&sessao.hospedagens.lock().unwrap(), config);
    assert_eq!(
        meu(&sessao),
        None,
        "sem guardado no ar, a entrada ofereceu VOLTAR a lugar nenhum"
    );

    let porta = guardado_de_teste(&sessao, Some(registrado.id.clone())).await;
    assert_eq!(
        meu(&sessao),
        Some(MeuServidor {
            alvo: format!("127.0.0.1:{porta}"),
            nome: Some("Casa da Rafa".into()),
        })
    );

    // O legado (o banco de sempre, sem registro) volta sem nome, e a frase
    // diz «o seu servidor».
    sessao
        .hospedagens
        .lock()
        .unwrap()
        .guardado
        .as_mut()
        .unwrap()
        .servidor = None;
    assert_eq!(meu(&sessao).and_then(|meu| meu.nome), None);
    parar_de_hospedar_em(&sessao).await;
}
