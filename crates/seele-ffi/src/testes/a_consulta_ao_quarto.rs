//! A porta da casca para o quarto: `onde_mora_hoje` com uma impressão
//! digital, e não com marcas.

use super::{onde_mora_hoje, OndeMora};

#[tokio::test]
async fn uma_impressao_que_nao_forma_marca_nao_manda_pergunta_nenhuma() {
    // O ponto é um socket local **mudo**, e é ele quem prova que nada saiu:
    // se alguma pergunta partisse, ela pararia no `recv_from` dele. Só a
    // variante e a duração não bastam: uma consulta que perguntasse e
    // desistisse depressa passaria nas duas.
    let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let ponto = mudo.local_addr().unwrap().to_string();

    let comecou = std::time::Instant::now();
    let achado = onde_mora_hoje(&ponto, "curta").await;
    let levou = comecou.elapsed();
    assert_eq!(
        achado,
        OndeMora::SemMarca,
        "uma impressão digital que não forma marca virou outra resposta: quem ler o log vai \
             procurar defeito de rede onde não há"
    );
    assert!(
        levou < std::time::Duration::from_millis(200),
        "sem marca não há pergunta, e mesmo assim a consulta esperou {levou:?}"
    );

    let mut balde = [0_u8; 64];
    let chegou = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        mudo.recv_from(&mut balde),
    )
    .await;
    assert!(
        chegou.is_err(),
        "a consulta devolveu `SemMarca` mas mandou um datagrama ao ponto assim mesmo: sem \
             marca não há o que perguntar, e o que sai é tráfego sem resposta possível"
    );
}
