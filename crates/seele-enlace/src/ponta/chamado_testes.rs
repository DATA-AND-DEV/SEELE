//! O `CHAMADO` atrás de um NAT de casa que muda a porta conforme o destino.
//!
//! **O caso medido em 26/10/2026** (`relay5g-metadados.txt`): o Windows falava com o
//! encontro por 187.255.97.152:14178 e o furo dele para o relay do navegador saía por
//! 14224. Quem liga só conhecia o 14178 (o `publico` do ATENDE), e o 14178 só deixa
//! passar o encontro. O anfitrião ICE-lite nunca checava, então quem liga nunca
//! aprendia o 14224.
//!
//! O NAT daqui é de mentira, mas o resto é o de verdade: duas `Ponta`s em loopback,
//! ICE, DTLS e SCTP inteiros. Para cada destino de fora, o NAT tem uma face de
//! dentro (o endereço que o anfitrião usa para falar com aquele destino) e uma de
//! fora. Dependente do destino: uma porta de fora por destino, e cada uma só deixa
//! entrar o destino dela (o mapeamento e o filtro do NAT medido). Independente: uma
//! porta de fora só, que deixa entrar qualquer um.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;

use super::*;

/// O filtro de produção recusa loopback; o teste passa um que o aceita.
const ACEITA: fn(SocketAddr) -> bool = |_| true;

/// Um NAT de mentira na frente de `anfitriao`.
struct NatDeMentira {
    /// Por destino de fora: a face de dentro (para onde o anfitrião manda).
    dentro: HashMap<SocketAddr, SocketAddr>,
    /// Por destino de fora: a porta de fora que esse destino vê.
    fora: HashMap<SocketAddr, SocketAddr>,
    _tarefas: Vec<tokio::task::JoinHandle<()>>,
}

impl NatDeMentira {
    async fn montar(dependente: bool, anfitriao: SocketAddr, destinos: &[SocketAddr]) -> Self {
        let mut dentro = HashMap::new();
        let mut fora = HashMap::new();
        let mut tarefas = Vec::new();
        let mut faces = HashMap::new();
        for destino in destinos {
            let face = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
            dentro.insert(*destino, face.local_addr().unwrap());
            faces.insert(*destino, face);
        }
        let externas: HashMap<SocketAddr, Arc<UdpSocket>> = if dependente {
            let mut externas = HashMap::new();
            for destino in destinos {
                externas.insert(
                    *destino,
                    Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
                );
            }
            externas
        } else {
            let unica = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
            destinos.iter().map(|d| (*d, Arc::clone(&unica))).collect()
        };
        for destino in destinos {
            fora.insert(*destino, externas[destino].local_addr().unwrap());
        }
        // De dentro para fora: o que o anfitrião manda à face de D sai pela porta de fora de D.
        for destino in destinos {
            let face = Arc::clone(&faces[destino]);
            let externa = Arc::clone(&externas[destino]);
            let destino = *destino;
            tarefas.push(tokio::spawn(async move {
                let mut buf = vec![0_u8; 2048];
                while let Ok((n, de)) = face.recv_from(&mut buf).await {
                    if de == anfitriao {
                        let _ = externa.send_to(&buf[..n], destino).await;
                    }
                }
            }));
        }
        // De fora para dentro: cada porta de fora só deixa entrar os destinos dela, e o
        // que entra chega ao anfitrião pela face daquele destino.
        let mut lidas: Vec<(Arc<UdpSocket>, Vec<SocketAddr>)> = Vec::new();
        for destino in destinos {
            let externa = &externas[destino];
            match lidas.iter_mut().find(|(e, _)| Arc::ptr_eq(e, externa)) {
                Some((_, quem)) => quem.push(*destino),
                None => lidas.push((Arc::clone(externa), vec![*destino])),
            }
        }
        for (externa, quem) in lidas {
            let faces: HashMap<SocketAddr, Arc<UdpSocket>> =
                quem.iter().map(|d| (*d, Arc::clone(&faces[d]))).collect();
            tarefas.push(tokio::spawn(async move {
                let mut buf = vec![0_u8; 2048];
                while let Ok((n, de)) = externa.recv_from(&mut buf).await {
                    if let Some(face) = faces.get(&de) {
                        let _ = face.send_to(&buf[..n], anfitriao).await;
                    }
                }
            }));
        }
        Self {
            dentro,
            fora,
            _tarefas: tarefas,
        }
    }
}

/// O NAT é o caminho: ele vive enquanto o teste o segura, e ao cair derruba os proxies.
impl Drop for NatDeMentira {
    fn drop(&mut self) {
        for tarefa in &self._tarefas {
            tarefa.abort();
        }
    }
}

async fn anfitriao_com(admitir: bool) -> (Ponta, CertificadoDtls) {
    let certificado = CertificadoDtls::efemero().unwrap();
    let mut config = ConfigDoAnfitriao::novo(certificado.clone());
    if !admitir {
        config.admitir = Arc::new(|_| false);
    }
    let ponta = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), Some(config))
        .await
        .unwrap();
    (ponta, certificado)
}

async fn cliente() -> Ponta {
    Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap()
}

/// O que o anfitrião faz com um `CHAMADO` (o caminho de `alcance::encontro`).
fn atender(ponta: &Ponta, u: &str, candidatos: &[SocketAddr]) -> usize {
    ponta.chamado_com(u, candidatos, ACEITA)
}

/// O cenário medido: quem liga só conhece a porta que o NAT abriu para o encontro, e o
/// anfitrião fica sabendo de quem liga pelo `CHAMADO` (a face de dentro do NAT para ele).
async fn ligar_pelo_nat(
    dependente: bool,
    u_do_chamado: Option<String>,
    prazo: Duration,
) -> (Result<Conexao, FalhaAoDiscar>, Ponta, Ponta, NatDeMentira) {
    let (host, certificado) = anfitriao_com(true).await;
    let quem_liga = cliente().await;
    let encontro = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let encontro = encontro.local_addr().unwrap();
    let nat = NatDeMentira::montar(
        dependente,
        host.local_addr(),
        &[encontro, quem_liga.local_addr()],
    )
    .await;
    let u = sortear_credencial();
    let discagem = Discagem {
        alvos: vec![nat.fora[&encontro]],
        pino: Pino::Impressao(certificado.impressao()),
        credencial: Some(u.clone()),
        prazo,
        certificado: None,
    };
    let discando = {
        let quem_liga = quem_liga.clone();
        tokio::spawn(async move { quem_liga.discar(discagem).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let u_do_chamado = u_do_chamado.unwrap_or(u);
    assert!(atender(&host, &u_do_chamado, &[nat.dentro[&quem_liga.local_addr()]]) > 0);
    let resultado = discando.await.unwrap();
    // O NAT volta junto e fica vivo até o fim do teste: os proxies dele são o caminho.
    (resultado, host, quem_liga, nat)
}

/// **O defeito medido.** Com o NAT de casa dependente do destino, quem liga entra pelo
/// endereço que ele aprende das checagens do anfitrião (o peer-reflexive), e o DTLS
/// continua preso à impressão.
#[tokio::test]
async fn com_o_nat_de_casa_dependente_do_destino_quem_liga_entra_pelo_reflexivo_do_chamado() {
    let (resultado, host, _quem_liga, _nat) =
        ligar_pelo_nat(true, None, Duration::from_secs(6)).await;
    let conexao = resultado.unwrap_or_else(|erro| {
        panic!(
            "quem liga não entrou pelo NAT dependente do destino: {erro}; contadores do anfitrião {:?}",
            host.contadores()
        )
    });
    let aceita = tokio::time::timeout(Duration::from_secs(5), host.aceitar())
        .await
        .expect("o anfitrião não entregou a conexão")
        .unwrap();
    // Os dados nos dois sentidos, pelo `ctl`.
    let (mut envio_de_quem_liga, mut recebe_de_quem_liga) = conexao.controle().unwrap();
    let (mut envio_do_anfitriao, mut recebe_do_anfitriao) = aceita.controle().unwrap();
    envio_de_quem_liga
        .write_all(b"pelo reflexivo")
        .await
        .unwrap();
    let mut buf = [0_u8; 14];
    recebe_do_anfitriao.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pelo reflexivo");
    envio_do_anfitriao.write_all(b"e de volta").await.unwrap();
    let mut buf = [0_u8; 10];
    recebe_de_quem_liga.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"e de volta");
    assert_eq!(host.contadores().pares_do_chamado, 1);
}

/// O controle: com o NAT independente do destino, o caminho de antes já abria, e segue abrindo.
#[tokio::test]
async fn com_o_nat_independente_do_destino_quem_liga_entra_como_antes() {
    let (resultado, host, _quem_liga, _nat) =
        ligar_pelo_nat(false, None, Duration::from_secs(6)).await;
    resultado.unwrap_or_else(|erro| panic!("{erro}; {:?}", host.contadores()));
}

/// Um `CHAMADO` com outra credencial não abre caminho: quem liga confere a integridade
/// das checagens do anfitrião contra o U dele, e ignora as que não conferem.
#[tokio::test]
async fn um_chamado_com_a_credencial_errada_nao_abre_caminho() {
    let (resultado, _host, _quem_liga, _nat) =
        ligar_pelo_nat(true, Some(sortear_credencial()), Duration::from_secs(3)).await;
    assert!(resultado.is_err(), "entrou com a credencial errada");
}

/// O mesmo `CHAMADO` duas vezes (o ponto repete, ou alguém o reenvia) não cria um segundo
/// par, e um par criado pelo `CHAMADO` que ninguém usa sai no prazo do aperto e vira lápide.
#[tokio::test]
async fn um_chamado_repetido_reaproveita_o_par_e_o_par_sem_uso_expira() {
    let (host, _) = anfitriao_com(true).await;
    let ninguem = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let alvo = ninguem.local_addr().unwrap();
    let u = sortear_credencial();
    assert!(atender(&host, &u, &[alvo]) > 0);
    assert!(atender(&host, &u, &[alvo]) > 0);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(host.conexoes_vivas(), 1, "um U, um par");
    tokio::time::sleep(crate::APERTO + Duration::from_millis(700)).await;
    assert_eq!(
        host.conexoes_vivas(),
        0,
        "o par sem uso saiu no prazo do aperto"
    );
    assert!(atender(&host, &u, &[alvo]) > 0);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        host.conexoes_vivas(),
        0,
        "o U enterrado não cria par de novo"
    );
}

/// O Binding direto que chega **antes** do `CHAMADO` (o NAT de casa deixa passar) cria o
/// par, e o `CHAMADO` com o mesmo U só junta candidatos a ele: um U, um par.
#[tokio::test]
async fn o_chamado_depois_do_binding_direto_reaproveita_o_par_pelo_u() {
    let (host, certificado) = anfitriao_com(true).await;
    let quem_liga = cliente().await;
    let u = sortear_credencial();
    let discagem = Discagem {
        alvos: vec![host.local_addr()],
        pino: Pino::Impressao(certificado.impressao()),
        credencial: Some(u.clone()),
        prazo: Duration::from_secs(6),
        certificado: None,
    };
    let discando = {
        let quem_liga = quem_liga.clone();
        tokio::spawn(async move { quem_liga.discar(discagem).await })
    };
    let ate = tokio::time::Instant::now() + Duration::from_secs(3);
    while host.contadores().pares_criados == 0 && tokio::time::Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(atender(&host, &u, &[quem_liga.local_addr()]) > 0);
    discando.await.unwrap().unwrap();
    let contadores = host.contadores();
    assert_eq!(contadores.pares_criados, 1, "{contadores:?}");
    assert_eq!(contadores.pares_do_chamado, 0, "{contadores:?}");
}

/// Os portões de antes valem para o `CHAMADO`: quem hospeda que não admite o endereço
/// não ganha par nenhum.
#[tokio::test]
async fn um_chamado_passa_pelos_mesmos_portoes_que_um_binding() {
    let (host, _) = anfitriao_com(false).await;
    let alvo = UdpSocket::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    assert!(atender(&host, &sortear_credencial(), &[alvo]) > 0);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(host.conexoes_vivas(), 0);
    assert_eq!(host.contadores().recusa_de_quem_hospeda, 1);
}

use crate::sortear_credencial;
