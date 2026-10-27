//! O transporte do cliente desde a 0.17: WebRTC pelo `seele-enlace`.
//!
//! Até a 0.16 o núcleo inteiro falava `quinn`. A 0.17 troca o QUIC por WebRTC
//! (ICE, DTLS e DataChannels), e o crate que fala o fio é o `seele-enlace`, o
//! mesmo do `seele-server` (contrato da 0.17, D1). Este módulo é o único lugar
//! do núcleo que nomeia o enlace diretamente: os outros veem os tipos daqui.
//!
//! # O que muda para quem lia o QUIC
//!
//! - **A identidade do servidor é a do DTLS.** O certificado DTLS do `seeled` é o
//!   certificado longo de sempre, então a impressão que o DTLS mostra é o `fp=`
//!   do link. Quem liga confere essa impressão contra o pino e o TOFU **antes**
//!   de mandar o `Hello` ([`crate::client`]).
//! - **Os candidatos vão juntos.** Um agente ICE só recebe todos os endereços de
//!   um anfitrião ([`seele_enlace::Discagem::alvos`]), em vez de uma corrida de
//!   apertos de mão.
//! - **Não há contador de perda nem de congestionamento** no SCTP do enlace; a
//!   sonda de subida lê o que existe ([`crate::caminho::Transporte`]).

pub use seele_enlace::{
    CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, Envio, FalhaAoDiscar, FalhaDoCanal,
    Leitura, Motivo, Pino, Ponta, Recebe,
};

/// O código de fechamento do «ficou para trás», o **mesmo** número do servidor:
/// o do `seele_proto::transport`, reexportado, e não uma cópia.
///
/// O cliente o lê como a mesma despedida que o `Disconnecting { FellBehind }`
/// daria, quando o quadro não pôde sair (`crate::client`, tarefa leitora).
pub use seele_proto::transport::CODIGO_FICOU_PARA_TRAS;

/// Espera quem recebe confirmar o fim de um canal que esta ponta escreveu e
/// fechou, e diz como a transferência terminou.
///
/// Só o canal lido até o fim é entrega. Um corte que chega **depois** do último
/// bloco é recusa, e não sucesso: num arquivo pequeno tudo cabe numa escrita, e
/// se a recusa aparece como escrita que falhou ou como corte aqui é corrida com
/// a rede (o defeito intermitente que o `stopped` do QUIC fechou).
pub(crate) async fn esperar_a_entrega(envio: &mut Envio, bytes: u64) -> crate::client::Sent {
    match envio.entregue().await {
        Ok(()) => crate::client::Sent::Delivered { bytes },
        Err(FalhaDoCanal::Cortado(_)) => crate::client::Sent::Stopped { bytes },
        Err(_) => crate::client::Sent::Interrupted { bytes },
    }
}

/// A ponta WebRTC de **uma** sessão cliente, com a identidade de par dela.
///
/// # Uma por sessão (E3 da 0.17, R5 da revisão)
///
/// Cada sessão (e cada tentativa de entrar numa) tem o seu socket: o `CHAMA`
/// do degrau 4 sai da porta dela, os pares de tela a alcançam pela porta que o
/// servidor dela viu, e a reconexão de uma sessão nunca fecha a ponta de outra.
/// Custa um socket por sessão, o que é nada para três a dez.
///
/// # Por que ela também atende
///
/// O caminho entre pares (`crate::par`) faz um cliente servir tela a outro **na
/// mesma porta** que fala com o servidor (spec de 05/09, §3.1): é ali que o NAT
/// já está aberto. Então a ponta nasce anfitriã (ICE completo e controlado), com o certificado
/// DTLS da identidade de par desta sessão — mas com o portão **fechado**: o
/// `admitir` recusa todo Binding que chega, antes de criar estado nenhum, até
/// [`PontaDaSessao::armar`]. Fechado, ela é exatamente uma ponta que só disca.
#[derive(Clone)]
pub struct PontaDaSessao {
    ponta: Ponta,
    identidade: CertificadoDtls,
    portao: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for PontaDaSessao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PontaDaSessao")
            .field("ponta", &self.ponta)
            .field("armada", &self.armada())
            .finish()
    }
}

impl PontaDaSessao {
    /// A ponta, para discar e para os datagramas do ENC/3.
    #[must_use]
    pub fn ponta(&self) -> &Ponta {
        &self.ponta
    }

    /// O certificado DTLS que esta sessão apresenta como par de tela: a
    /// impressão que ela declara ao servidor em `EmprestarSubida`.
    #[must_use]
    pub fn identidade(&self) -> &CertificadoDtls {
        &self.identidade
    }

    /// Passa a aceitar quem disca para esta ponta (um par de tela).
    ///
    /// Quem chega ainda é conferido pela impressão do certificado DTLS dele
    /// (`crate::par`), e quem não confere é derrubado antes de qualquer canal
    /// levar tela.
    pub fn armar(&self) {
        self.portao.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Volta a recusar todo Binding que chega, antes de criar estado.
    pub fn desarmar(&self) {
        self.portao
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Se o portão está aberto.
    #[must_use]
    pub fn armada(&self) -> bool {
        self.portao.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Abre a ponta de quem conecta: o socket de pilha dupla de
/// [`crate::client::socket_de_quem_conecta`], ou o IPv4 de recuo, ou o socket
/// que quem chama já abriu.
///
/// # Errors
///
/// [`crate::ConnectError::LocalEndpoint`] quando nenhum socket abre, ou a
/// identidade de par não pôde ser gerada.
pub(crate) fn abrir_ponta(
    local: Option<std::net::UdpSocket>,
) -> Result<PontaDaSessao, crate::ConnectError> {
    let identidade = CertificadoDtls::efemero().map_err(|erro| {
        tracing::error!(%erro, "não deu para gerar a identidade de par desta sessão");
        crate::ConnectError::LocalEndpoint
    })?;
    let portao = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let anfitriao = ConfigDoAnfitriao {
        certificado: identidade.clone(),
        apresentacao: Vec::new(),
        admitir: {
            let portao = std::sync::Arc::clone(&portao);
            std::sync::Arc::new(move |_| portao.load(std::sync::atomic::Ordering::SeqCst))
        },
    };
    let ponta = abrir_socket_e_ponta(local, anfitriao)?;
    Ok(PontaDaSessao {
        ponta,
        identidade,
        portao,
    })
}

/// O socket e a ponta em cima dele.
fn abrir_socket_e_ponta(
    local: Option<std::net::UdpSocket>,
    anfitriao: ConfigDoAnfitriao,
) -> Result<Ponta, crate::ConnectError> {
    let socket = match local {
        Some(socket) => socket,
        None => match crate::client::socket_de_quem_conecta() {
            Ok(socket) => socket,
            Err(erro) => {
                tracing::info!(
                    %erro,
                    "sem socket local de pilha dupla: recuando para um socket só IPv4"
                );
                std::net::UdpSocket::bind(std::net::SocketAddr::from(([0, 0, 0, 0], 0))).map_err(
                    |erro| {
                        tracing::error!(%erro, "não deu para abrir o socket local");
                        crate::ConnectError::LocalEndpoint
                    },
                )?
            }
        },
    };
    Ponta::de_socket(socket, Some(anfitriao)).map_err(|erro| {
        tracing::error!(%erro, "não deu para abrir a ponta WebRTC local");
        crate::ConnectError::LocalEndpoint
    })
}

/// Um par de conexões já abertas em `127.0.0.1`, para as baterias que medem o
/// que corre por cima do transporte (tela, bomba, enquadramento).
///
/// Devolve também as duas pontas, que precisam continuar vivas enquanto as
/// conexões existirem, e o `ctl` de cada lado, que o enlace entrega uma vez só.
#[cfg(test)]
pub(crate) mod teste {
    use super::{CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, Pino, Ponta};

    /// Quem liga e quem atende, já com o DTLS fechado.
    pub(crate) struct Par {
        /// O lado de quem discou.
        pub(crate) quem_liga: Conexao,
        /// O lado de quem atendeu.
        pub(crate) quem_atende: Conexao,
        /// As duas pontas, vivas enquanto o par existir.
        pub(crate) _pontas: (Ponta, Ponta),
    }

    /// Abre o par. `apresentacao` é a mensagem 0x03 do anfitrião.
    pub(crate) async fn par_com(apresentacao: Vec<u8>) -> Par {
        let anfitriao = Ponta::abrir(
            std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            Some(ConfigDoAnfitriao {
                certificado: CertificadoDtls::efemero().expect("certificado"),
                apresentacao,
                admitir: std::sync::Arc::new(|_| true),
            }),
        )
        .await
        .expect("a ponta de quem atende");
        let cliente = Ponta::abrir(std::net::SocketAddr::from(([127, 0, 0, 1], 0)), None)
            .await
            .expect("a ponta de quem liga");
        let alvo = anfitriao.local_addr();
        let atende = {
            let anfitriao = anfitriao.clone();
            tokio::spawn(async move { anfitriao.aceitar().await.expect("chegou") })
        };
        let quem_liga = cliente
            .discar(Discagem::nova(vec![alvo], Pino::Qualquer))
            .await
            .unwrap_or_else(|falha| panic!("discar em loopback: {falha:?}"));
        let quem_atende = atende.await.expect("junção");
        Par {
            quem_liga,
            quem_atende,
            _pontas: (cliente, anfitriao),
        }
    }

    /// Abre o par com a apresentação de um `seeled`, vazia.
    pub(crate) async fn par() -> Par {
        par_com(Vec::new()).await
    }

    /// Um anfitrião em `127.0.0.1`, e uma ponta de sessão (`local`, ou a de
    /// produção, de pilha dupla, com `None`) que disca para ele.
    async fn a_sessao_disca(local: Option<std::net::UdpSocket>) -> Result<Conexao, String> {
        let anfitriao = Ponta::abrir(
            std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            Some(ConfigDoAnfitriao::novo(
                CertificadoDtls::efemero().expect("certificado"),
            )),
        )
        .await
        .expect("a ponta de quem atende");
        let alvo = anfitriao.local_addr();
        let atende = {
            let anfitriao = anfitriao.clone();
            tokio::spawn(async move { anfitriao.aceitar().await })
        };
        let sessao = super::abrir_ponta(local).expect("a ponta da sessão");
        let mut discagem = Discagem::nova(vec![alvo], Pino::Qualquer);
        discagem.prazo = std::time::Duration::from_secs(5);
        let conexao = sessao
            .ponta()
            .discar(discagem)
            .await
            .map_err(|falha| format!("{falha:?}"))?;
        let _ = atende.await;
        Ok(conexao)
    }

    #[tokio::test]
    async fn a_ponta_de_cliente_do_enlace_de_pilha_dupla_disca_um_anfitriao_ipv4() {
        let anfitriao = Ponta::abrir(
            std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            Some(ConfigDoAnfitriao::novo(
                CertificadoDtls::efemero().expect("certificado"),
            )),
        )
        .await
        .expect("a ponta de quem atende");
        let alvo = anfitriao.local_addr();
        let _atende = {
            let anfitriao = anfitriao.clone();
            tokio::spawn(async move { anfitriao.aceitar().await })
        };
        let cliente = Ponta::abrir_cliente().await.expect("cliente");
        let mut discagem = Discagem::nova(vec![alvo], Pino::Qualquer);
        discagem.prazo = std::time::Duration::from_secs(5);
        cliente
            .discar(discagem)
            .await
            .expect("abrir_cliente não discou");
    }

    #[tokio::test]
    async fn a_ponta_da_sessao_em_ipv4_disca_um_anfitriao() {
        // A ponta da sessão nasce anfitriã (para os pares), com o portão
        // fechado: discar tem de funcionar igual a uma ponta só de cliente.
        let local = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        a_sessao_disca(Some(local))
            .await
            .expect("a ponta da sessão não discou");
    }

    #[tokio::test]
    async fn a_ponta_da_sessao_de_pilha_dupla_disca_um_anfitriao_ipv4() {
        // O socket de produção é `[::]` de pilha dupla, e a maioria dos
        // servidores é IPv4: o caso comum, e não a borda.
        a_sessao_disca(None)
            .await
            .expect("a ponta de pilha dupla não discou um anfitrião IPv4");
    }
}
