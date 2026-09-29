//! O quarto do ponto de encontro: onde um servidor mora **hoje**.
//!
//! O relato que o pediu: «o ip que fica salvo na lista de servidores ainda dá
//! problema de reconexão, possivelmente porque a porta muda quando abre e fecha
//! o server. Precisamos achar um jeito de resolver isso, por que se não, a lista
//! de servidores fica inútil.»
//!
//! Tudo o que a lista guarda é endereço, e endereço atrás de NAT é perecível: o
//! mapeamento nasce quando um pacote sai e o roteador dá outro na abertura
//! seguinte. O que não envelhece é a impressão digital, e é dela que sai a marca
//! com que se pergunta.
//!
//! Aqui, e não dentro de um crate só: o teste precisa do anfitrião, do cliente e
//! do serviço no meio, e o ADR 0002 proíbe qualquer um deles de ver o outro.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;

use seele_core::encontro::{onde_mora, onde_mora_hoje, Marca, Marcas, OndeMora, PRAZO_DO_QUARTO};
use seele_proto::encontro::{analisar, moro, Pedido, Vizinhanca, TAMANHO};

mod vaga;

/// Um ponto de encontro de verdade, atendendo numa linha própria.
fn subir_o_ponto() -> SocketAddr {
    let ponto = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Vizinhanca::TambemAqui,
        Arc::new(seele_encontro::Quarto::novo()),
    )
    .expect("a escuta tem que abrir");
    let onde = ponto.endereco().expect("o socket sabe onde ligou");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    onde
}

#[tokio::test]
async fn um_servidor_que_trocou_de_porta_ainda_e_achado_pela_impressao() {
    let _vaga = vaga::minha();
    let onde_fica = subir_o_ponto();
    let marca = Marca::nova("abcdef0123456789").expect("é uma marca");
    let mut balde = [0_u8; TAMANHO];

    // A primeira abertura do servidor.
    let ontem = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let porta_de_ontem = ontem.local_addr().unwrap();
    ontem.send_to(&moro(&marca), onde_fica).await.unwrap();
    ontem.recv_from(&mut balde).await.unwrap();

    // Ele fecha, e volta noutra porta — que é o que o NAT faz.
    drop(ontem);
    let hoje = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let porta_de_hoje = hoje.local_addr().unwrap();
    assert_ne!(
        porta_de_ontem, porta_de_hoje,
        "o teste precisa de duas portas diferentes para dizer alguma coisa"
    );
    hoje.send_to(&moro(&marca), onde_fica).await.unwrap();
    hoje.recv_from(&mut balde).await.unwrap();

    // E quem guardou só a impressão digital pergunta.
    let achado = onde_mora(
        &onde_fica.to_string(),
        &marca,
        std::time::Duration::from_secs(2),
    )
    .await;

    assert_eq!(
        achado,
        Some(porta_de_hoje),
        "quem perguntou recebeu a porta de ontem, ou não recebeu nada — e é \
         exatamente por isso que a lista de servidores ficava inútil"
    );
}

#[tokio::test]
async fn um_ponto_que_nao_conhece_a_pergunta_apenas_cala() {
    let _vaga = vaga::minha();
    // O caso de campo mais provável durante a migração: o serviço no ar é o de
    // antes desta mudança. Ele não responde a `QUEM`, e o que tem de acontecer é
    // a espera vencer e a conexão seguir com os endereços guardados — nunca uma
    // falha que derrube a tentativa.
    let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let onde_fica = mudo.local_addr().unwrap();
    let marca = Marca::nova("abcdef0123456789").expect("é uma marca");

    let comecou = std::time::Instant::now();
    let achado = onde_mora(
        &onde_fica.to_string(),
        &marca,
        std::time::Duration::from_millis(200),
    )
    .await;

    assert_eq!(achado, None, "não havia resposta a inventar");
    assert!(
        comecou.elapsed() < std::time::Duration::from_secs(2),
        "a espera não pode passar do prazo pedido: quem paga é toda conexão, \
         inclusive a de quem está na mesma casa"
    );
}

#[tokio::test]
async fn quem_esta_no_ar_nao_perde_o_lugar_para_quem_chega_dizendo_o_nome_dele() {
    let _vaga = vaga::minha();
    // Qualquer um manda `MORO` com a marca de outro. Isto não é autenticação —
    // este serviço não tem chave nenhuma para conferir — e não precisa ser: quem
    // chega confere a impressão digital de qualquer jeito (ADR 0003), então um
    // endereço errado falha no aperto de mão em vez de virar conexão com o
    // impostor.
    //
    // O que a regra de «quem escreveu primeiro fica» compra é que o impostor não
    // consiga nem isso enquanto o dono estiver no ar.
    let onde_fica = subir_o_ponto();
    let marca = Marca::nova("abcdef0123456789").expect("é uma marca");
    let mut balde = [0_u8; TAMANHO];

    let dono = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let do_dono = dono.local_addr().unwrap();
    dono.send_to(&moro(&marca), onde_fica).await.unwrap();
    dono.recv_from(&mut balde).await.unwrap();

    // No mesmo IP, então este teste não consegue distinguir o impostor pelo IP —
    // e é por isso que ele afirma o que afirma com o `127.0.0.1`: a regra
    // compara IP, e num teste de laço os dois são o mesmo. O que se prende aqui
    // é que o **dono** consegue se mudar, que é o outro lado da mesma regra.
    let mudou = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let novo = mudou.local_addr().unwrap();
    mudou.send_to(&moro(&marca), onde_fica).await.unwrap();
    mudou.recv_from(&mut balde).await.unwrap();

    let achado = onde_mora(
        &onde_fica.to_string(),
        &marca,
        std::time::Duration::from_secs(2),
    )
    .await;
    assert_eq!(
        achado,
        Some(novo),
        "o dono mudou de porta no mesmo IP e o quarto ficou com a antiga: é o \
         remapeamento de NAT, e é o caso que este mecanismo existe para cobrir \
         (o de {do_dono})"
    );
}

#[tokio::test]
async fn um_nome_que_nao_resolve_nao_atrasa_quem_esta_na_lan() {
    let _vaga = vaga::minha();
    // **É o pedaço que caía em cima de quem menos precisa dele.**
    //
    // O prazo cobria só a leitura do socket; resolver o nome do ponto de
    // encontro ficava de fora. Numa rede sem internet — duas máquinas na mesma
    // casa — `lookup_host` não falha rápido: ele espera o servidor de DNS que
    // não vai responder. E isto roda **antes** de qualquer tentativa de conexão,
    // então o atraso é pago por uma conexão de rede local que nunca precisaria
    // de ponto de encontro nenhum.
    //
    // **O que este teste prova, e o que ele não prova.**
    //
    // `.invalid` é reservado pela RFC 2606 e nenhum resolvedor tem resposta para
    // ele — mas a resposta «não existe» chega **rápido**, então este caso passava
    // mesmo com o defeito. O perigo de verdade é o resolvedor que não responde
    // nada, e simular isso pediria um servidor de DNS de mentira e uma rota para
    // ele; não é o que este arquivo faz.
    //
    // Então este teste prende a forma — o prazo existe e é obedecido no caminho
    // que dá para exercitar — e a asserção de estrutura logo abaixo prende o
    // resto: que resolver o nome esteja **dentro** dele.
    let marca = Marca::nova("abcdef0123456789").expect("é uma marca");

    let comecou = std::time::Instant::now();
    let achado = onde_mora(
        "nao-existe-em-lugar-nenhum.invalid:8384",
        &marca,
        std::time::Duration::from_millis(300),
    )
    .await;

    assert_eq!(achado, None, "não havia resposta a inventar");
    assert!(
        comecou.elapsed() < std::time::Duration::from_secs(2),
        "a pergunta passou do prazo: quem paga é a conexão que vem depois dela, \
         e numa LAN ela é a única que importa. Levou {:?}",
        comecou.elapsed()
    );
}

#[test]
fn resolver_o_nome_acontece_dentro_do_prazo_e_nao_antes_dele() {
    // O par do teste acima, e ele existe porque aquele não alcança o caso que
    // importa: um resolvedor que não responde. O que se pode afirmar sem um
    // servidor de DNS de mentira é a **forma** da função — se `lookup_host` está
    // dentro do que o `timeout` embrulha, nenhum resolvedor do mundo consegue
    // atrasar uma conexão de rede local além do prazo.
    let fonte = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../seele-core/src/encontro.rs"),
    )
    .expect("o módulo do encontro tem que ser legível");

    // **Sem os comentários.** O corpo desta função explica, em prosa, que
    // `lookup_host` ficava de fora do prazo — e a primeira versão deste guarda
    // casou com essa frase e acusou o código de ter o defeito que o comentário
    // descreve. É a segunda vez que uma âncora deste repositório encontra o
    // próprio comentário; da primeira foi a da bateria, no `publicar.sh`.
    let corpo: String = fonte
        .split("pub async fn onde_mora(")
        .nth(1)
        .and_then(|resto| resto.split("\nasync fn ").next())
        .unwrap_or_default()
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        corpo.contains("timeout(prazo"),
        "`onde_mora` deixou de embrulhar a pergunta num prazo:\n{corpo}"
    );
    assert!(
        !corpo.contains("lookup_host"),
        "resolver o nome voltou para fora do prazo. Numa rede sem internet isso \
         são segundos de espera cobrados de uma conexão de LAN que nunca \
         precisaria de ponto de encontro nenhum:\n{corpo}"
    );
}

/// A impressão digital dos testes da consulta. As marcas saem dela pelo mesmo
/// caminho que o app usa, e não escritas à mão: é esse caminho que se testa.
const IMPRESSAO: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

fn marcas_de_teste() -> Marcas {
    Marcas::do_servidor(IMPRESSAO).expect("uma impressão digital dá marcas")
}

#[tokio::test]
async fn o_quarto_devolve_o_servidor_e_a_escuta_e_nao_espera_o_prazo() {
    let _vaga = vaga::minha();
    let onde_fica = subir_o_ponto();
    let marcas = marcas_de_teste();
    let mut balde = [0_u8; TAMANHO];

    let servidor = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    servidor
        .send_to(&moro(&marcas.servidor), onde_fica)
        .await
        .unwrap();
    servidor.recv_from(&mut balde).await.unwrap();
    let escuta = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    escuta
        .send_to(&moro(&marcas.escuta), onde_fica)
        .await
        .unwrap();
    escuta.recv_from(&mut balde).await.unwrap();

    let comecou = std::time::Instant::now();
    let achado = onde_mora_hoje(&onde_fica.to_string(), &marcas, PRAZO_DO_QUARTO).await;

    assert_eq!(
        achado,
        OndeMora::Achado {
            servidor: Some(servidor.local_addr().unwrap()),
            escuta: Some(escuta.local_addr().unwrap()),
        },
        "o quarto tinha os dois endereços e a consulta não voltou com os dois"
    );
    assert!(
        comecou.elapsed() < std::time::Duration::from_millis(400),
        "as duas respostas chegaram e a consulta ainda esperou {:?}: quem paga é a conexão \
         que vem depois, inclusive a de quem está na mesma casa",
        comecou.elapsed()
    );
}

#[tokio::test]
async fn um_ponto_no_ar_sem_morador_diz_que_ninguem_mora_ali() {
    let _vaga = vaga::minha();
    let onde_fica = subir_o_ponto();

    let comecou = std::time::Instant::now();
    let achado = onde_mora_hoje(&onde_fica.to_string(), &marcas_de_teste(), PRAZO_DO_QUARTO).await;

    assert_eq!(
        achado,
        OndeMora::NinguemMora,
        "o ponto respondeu ao ONDE e a consulta não soube dizer que ele está no ar: «o ponto \
         caiu» e «o anfitrião não está» voltaram a ser o mesmo silêncio"
    );
    assert!(
        comecou.elapsed() < std::time::Duration::from_millis(1000),
        "o ponto provou que está no ar na primeira volta e a consulta ainda esperou {:?}",
        comecou.elapsed()
    );
}

#[tokio::test]
async fn sem_resposta_a_pergunta_sai_tres_vezes() {
    let _vaga = vaga::minha();
    let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let onde_fica = mudo.local_addr().unwrap().to_string();
    let marcas = marcas_de_teste();
    let do_servidor = marcas.servidor.clone();

    let consulta =
        tokio::spawn(async move { onde_mora_hoje(&onde_fica, &marcas, PRAZO_DO_QUARTO).await });

    let mut quantas = 0_u32;
    let mut balde = [0_u8; TAMANHO];
    let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO + std::time::Duration::from_millis(300);
    while let Ok(Ok((lidos, _))) = tokio::time::timeout_at(ate, mudo.recv_from(&mut balde)).await {
        if balde.get(..lidos).and_then(analisar)
            == Some(Pedido::Quem {
                marca: do_servidor.clone(),
            })
        {
            quantas += 1;
        }
    }
    let achado = consulta.await.unwrap();

    assert_eq!(
        quantas, 3,
        "a pergunta pelo servidor saiu {quantas} vez(es) a um ponto que não respondeu: um \
         datagrama perdido volta a custar a consulta inteira"
    );
    assert_eq!(
        achado,
        OndeMora::PontoMudo,
        "um ponto que não respondeu nem ao ONDE virou outra coisa"
    );
}

#[tokio::test]
async fn um_anfitriao_que_so_registrou_o_servidor_volta_sem_a_escuta_e_sem_esperar_o_prazo() {
    let _vaga = vaga::minha();
    // O anfitrião 0.15.0 registra a escuta como `anfitriao`, e a marca que a
    // consulta pergunta por ela nunca está no quarto. O ponto responde à do
    // servidor e cala à outra: a consulta tem de devolver o que achou, e não
    // ficar esperando uma resposta que não vem.
    let onde_fica = subir_o_ponto();
    let marcas = marcas_de_teste();
    let mut balde = [0_u8; TAMANHO];

    let servidor = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    servidor
        .send_to(&moro(&marcas.servidor), onde_fica)
        .await
        .unwrap();
    servidor.recv_from(&mut balde).await.unwrap();

    let comecou = std::time::Instant::now();
    let achado = onde_mora_hoje(&onde_fica.to_string(), &marcas, PRAZO_DO_QUARTO).await;

    assert_eq!(
        achado,
        OndeMora::Achado {
            servidor: Some(servidor.local_addr().unwrap()),
            escuta: None,
        },
        "o quarto só tinha o servidor e a consulta não voltou com ele e sem a escuta"
    );
    assert_eq!(
        achado.servidor(),
        Some(servidor.local_addr().unwrap()),
        "`servidor()` não devolve o endereço que o `Achado` carrega"
    );
    assert_eq!(
        achado.escuta(),
        None,
        "`escuta()` inventou um endereço que o `Achado` não tem"
    );
    assert!(
        comecou.elapsed() < std::time::Duration::from_millis(1000),
        "o ponto respondeu e a consulta ainda esperou {:?} pela escuta de um anfitrião que nunca \
         a registrou com esta marca: é o custo de toda conexão a um anfitrião 0.15.0",
        comecou.elapsed()
    );
}
