//! O caminho entre pares: um cliente serve tela a outro.
//!
//! O `§5.1` do desenho de compartilhamento de tela recusou isto em 22/08 com
//! uma frase — *«custa um caminho que este produto nunca teve»*. Este módulo é
//! esse caminho, e a spec de 05/09 conta por que ele passou a valer a pena.
//!
//! # Desde a 0.17: WebRTC, e a ponta da sessão
//!
//! Até a 0.16 os dois pares falavam QUIC, e quem atendia armava um
//! `ServerConfig` na ponta de quem discava o servidor. Agora a ponta de cada
//! sessão ([`crate::transporte::PontaDaSessao`]) já nasce anfitriã (ICE completo e controlado), com
//! o certificado DTLS da **identidade de par** desta sessão, e com o portão
//! fechado. [`passar_a_atender`] abre o portão e [`parar_de_atender`] o fecha.
//!
//! # A identidade é apresentação, e não TOFU
//!
//! O ADR 0003 vale para o `seele://`, onde a primeira vez tem de ser confiada.
//! Aqui há um intermediário: os dois clientes já fixaram o mesmo servidor e se
//! autenticaram nele por chave (ADR 0004), e é o servidor quem diz a cada um a
//! impressão do outro (`SirvaTelaPara`, `AssistaTelaPor`). Os dois lados
//! conferem:
//!
//! - **quem disca** confere a impressão do certificado DTLS de quem atendeu,
//!   antes de trocar um byte de aplicação ([`ligar`]);
//! - **quem atende** confere a impressão do certificado DTLS de quem chegou, e
//!   derruba quem não é o apresentado antes de qualquer tela sair ([`atender`]).
//!
//! O formato da impressão é um só, `seele_proto::transport::certificate_fingerprint`.

use crate::transporte::{CertificadoDtls, Conexao, PontaDaSessao};

/// A identidade de par de uma sessão: o certificado DTLS que a ponta dela
/// apresenta. **Nunca vai para o disco** ([`identidade_efemera`]).
pub type Identidade = CertificadoDtls;

/// O código com que quem atende fecha a conexão de quem não foi apresentado.
pub const CODIGO_PAR_RECUSADO: u32 = 0x5345_0020;

/// Por que o caminho entre pares não deu certo.
///
/// Enumerado, e cada variante distingue um conserto diferente — ver a tabela do
/// §4 da spec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDePar {
    /// Não deu para gerar o certificado desta sessão.
    #[error("não deu para gerar o certificado deste par: {0}")]
    Certificado(String),
    /// Não deu para pôr a ponta a atender, ou a discagem não pôde sair.
    #[error("não deu para pôr esta ponta a atender: {0}")]
    Escuta(String),
    /// Nenhum dos endereços respondeu ao ICE.
    #[error("nenhum endereço deste par respondeu")]
    NaoAlcancou,
    /// Alguém respondeu, e não era quem o servidor apresentou.
    ///
    /// **Não é o mesmo que [`Self::NaoAlcancou`]**, e juntá-las apagaria a
    /// informação inteira: a diferença entre «não consegui falar com ele» e
    /// «alguém respondeu no lugar dele» é a que o ADR 0003 existe para nomear.
    #[error("o par apresentou {veio}, e o servidor tinha dito {esperada}")]
    ImpressaoNaoBate {
        /// O que o servidor apresentou.
        esperada: String,
        /// O que veio no aperto de mão.
        veio: String,
    },
    /// O DTLS fechou do lado de quem disca, e quem atendeu recusou.
    ///
    /// **Não é o mesmo que [`Self::NaoAlcancou`].** Alguém respondeu e o DTLS
    /// fechou; quem atende confere o certificado de quem chegou **depois**, e a
    /// recusa chega como o fechamento desta conexão, antes da confirmação.
    #[error("a ligação fechou logo depois de conectar: {0}")]
    RecusadoDepoisDeLigar(MotivoDaRecusa),
    /// Alguém completou o aperto de mão, e o prazo venceu antes da
    /// confirmação de aplicação.
    ///
    /// **Não é [`Self::NaoAlcancou`]**: pelo menos um respondeu e completou o
    /// DTLS — só não trocou o byte de confirmação a tempo.
    #[error("um candidato completou o aperto de mão, e o prazo venceu antes da confirmação")]
    ConfirmacaoNaoChegouATempo,
    /// O repasse de tela para um par parou no meio.
    ///
    /// **Não é [`Self::Escuta`]**: cobre o canal que não abriu ou parou de
    /// aceitar bytes (o par caiu, ou cortou), ou os bytes que chegam de quem
    /// está do outro lado de [`repassar`] que já não formam um enquadramento
    /// válido (ver [`crate::tela::Enquadramento::entrada`]).
    #[error("o repasse de tela para o par parou: {0}")]
    Repasse(String),
}

/// Por que a conexão fechou depois que quem disca já a considerava pronta.
///
/// **Enumerado, não texto solto** — `specs/02-protocolo.md` e o ADR 0012.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MotivoDaRecusa {
    /// Quem atendeu fechou com [`CODIGO_PAR_RECUSADO`]: o certificado de quem
    /// discou não é o que o servidor apresentou.
    ///
    /// Desde a 0.17 não há «sem certificado»: o DTLS sempre apresenta um.
    #[error("apresentou um certificado que não foi aceito")]
    CertificadoErrado,
    /// Fechou por outro motivo, sem relação com a conferência de certificado.
    #[error("{0}")]
    Outro(String),
}

/// Gera a identidade desta sessão. **Nunca vai para o disco.**
///
/// # Errors
///
/// Falha se o certificado não puder ser gerado.
pub fn identidade_efemera() -> Result<Identidade, ErroDePar> {
    CertificadoDtls::efemero().map_err(|erro| ErroDePar::Certificado(erro.to_string()))
}

/// A impressão digital que esta identidade apresenta, no formato de sempre.
#[must_use]
pub fn impressao(identidade: &Identidade) -> String {
    crate::client::hex(&identidade.impressao())
}

/// Diz à ponta desta sessão que ela também atende.
///
/// **Só é chamada quando a pessoa optou por emprestar a subida.** Quem não
/// optou nunca passa por aqui, e a ponta dela continua só discando.
///
/// Não muda a porta nem abre socket: é a mesma ponta que fala com o servidor,
/// e naquela porta o mapeamento de NAT já está vivo (§3.1 da spec de 05/09).
pub fn passar_a_atender(ponta: &PontaDaSessao) {
    ponta.armar();
}

/// Desarma o que [`passar_a_atender`] armou.
///
/// **Não é limpeza opcional — achado do fix round 3.** Armada, a ponta cria
/// estado para todo Binding que chegue. Desarmada, o `admitir` recusa antes de
/// criar qualquer coisa. Chame depois que [`atender`] devolver, sirva ele ou
/// não sirva. A conexão de controle desta mesma ponta não é afetada.
pub fn parar_de_atender(ponta: &PontaDaSessao) {
    ponta.desarmar();
}

/// Como a ligação com um par foi conseguida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComoChegou {
    /// Mesma rede: nenhum furo foi necessário.
    Local,
    /// Endereço público: o furo deu certo.
    Furo,
}

/// Um par ligado, e o que a ligação ensinou.
pub struct ParLigado {
    /// A conexão viva.
    pub conexao: Conexao,
    /// Como ela foi conseguida.
    pub como: ComoChegou,
    /// O ida e volta que o ICE mediu nela. É o custo de um salto.
    pub ida_e_volta: std::time::Duration,
}

impl std::fmt::Debug for ParLigado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParLigado")
            .field("par", &self.conexao.remote_address())
            .field("como", &self.como)
            .field("ida_e_volta", &self.ida_e_volta)
            .finish()
    }
}

/// A impressão de um texto hex de 64 dígitos, ou nada.
fn impressao_em_bytes(texto: &str) -> Option<[u8; 32]> {
    if texto.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (indice, byte) in bytes.iter_mut().enumerate() {
        let par = texto.get(indice * 2..indice * 2 + 2)?;
        *byte = u8::from_str_radix(par, 16).ok()?;
    }
    Some(bytes)
}

/// Disca para um par e devolve a ligação, conferida e confirmada.
///
/// **Todos os endereços num agente ICE só** (D2 da 0.17), como o servidor: o
/// ICE tenta os candidatos juntos e fica com o primeiro par que responde.
///
/// # Um impostor na lista não derruba a discagem
///
/// A lista de um par mistura endereço de LAN e endereço público, e um deles
/// estar obsoleto — reciclado por outra máquina — é cenário de todo dia. Se o
/// ICE ficar com quem responde no lugar do par, a impressão não bate: o evento
/// de segurança vai ao `warn!` na hora, a conexão fecha antes de qualquer byte
/// de aplicação, **e a discagem continua com os endereços que sobram**.
/// `ImpressaoNaoBate` só é devolvido se ninguém mais vencer.
///
/// # Errors
///
/// - [`ErroDePar::NaoAlcancou`] — nenhum endereço respondeu ao ICE no prazo.
/// - [`ErroDePar::ImpressaoNaoBate`] — alguém respondeu e não era quem o
///   servidor apresentou.
/// - [`ErroDePar::RecusadoDepoisDeLigar`] — quem atendeu recusou a identidade
///   desta sessão.
/// - [`ErroDePar::ConfirmacaoNaoChegouATempo`] — alguém completou o DTLS e o
///   prazo venceu antes da troca do byte de confirmação.
/// - [`ErroDePar::Escuta`] — a impressão pedida não é uma impressão, ou a
///   ponta fechou.
pub async fn ligar(
    ponta: &PontaDaSessao,
    enderecos: &[std::net::SocketAddr],
    impressao_esperada: String,
    prazo: std::time::Duration,
) -> Result<ParLigado, ErroDePar> {
    let Some(esperada) = impressao_em_bytes(&impressao_esperada.to_ascii_lowercase()) else {
        return Err(ErroDePar::Escuta(format!(
            "a impressão apresentada não é um SHA-256 em hex: {impressao_esperada}"
        )));
    };
    let ate = tokio::time::Instant::now() + prazo;
    let mut restantes: Vec<std::net::SocketAddr> = enderecos.to_vec();
    let mut ultimo = ErroDePar::NaoAlcancou;
    while !restantes.is_empty() {
        let falta = ate.saturating_duration_since(tokio::time::Instant::now());
        if falta.is_zero() {
            break;
        }
        let mut discagem =
            crate::transporte::Discagem::nova(restantes.clone(), crate::transporte::Pino::Qualquer);
        discagem.prazo = falta;
        discagem.certificado = Some(ponta.identidade().clone());
        let conexao = match ponta.ponta().discar(discagem).await {
            Ok(conexao) => conexao,
            Err(crate::transporte::FalhaAoDiscar::PontaFechada) => {
                return Err(ErroDePar::Escuta("a ponta desta sessão fechou".to_owned()));
            }
            Err(falha) => {
                tracing::debug!(%falha, ?restantes, "estes endereços do par não fecharam aperto");
                break;
            }
        };
        let veio = conexao.impressao_dtls_do_par();
        let onde = conexao.remote_address();
        if veio != esperada {
            let motivo = ErroDePar::ImpressaoNaoBate {
                esperada: impressao_esperada.clone(),
                veio: crate::client::hex(&veio),
            };
            // **Notícia, e não pode esperar o fim da discagem.**
            tracing::warn!(par = %onde, %motivo, "alguém respondeu no lugar do par");
            conexao.close(CODIGO_PAR_RECUSADO, b"impressao");
            restantes.retain(|endereco| {
                endereco.ip().to_canonical() != onde.ip().to_canonical()
                    || endereco.port() != onde.port()
            });
            ultimo = motivo;
            continue;
        }
        let falta = ate.saturating_duration_since(tokio::time::Instant::now());
        return match tokio::time::timeout(falta, confirmar_com_quem_atende(&conexao)).await {
            Ok(Ok(())) => {
                let ida_e_volta = conexao.rtt();
                let como = como_chegou(onde);
                tracing::info!(par = %onde, ?como, ?ida_e_volta, "um par ligou");
                Ok(ParLigado {
                    conexao,
                    como,
                    ida_e_volta,
                })
            }
            Ok(Err(motivo)) => {
                tracing::warn!(par = %onde, %motivo, "a ligação fechou logo depois de conectar");
                Err(ErroDePar::RecusadoDepoisDeLigar(motivo))
            }
            Err(_) => Err(ErroDePar::ConfirmacaoNaoChegouATempo),
        };
    }
    Err(ultimo)
}

/// Atende **uma** ligação de par, e devolve o que chegou.
///
/// Uma só, e não um laço: no A1 quem empresta serve um par por vez.
///
/// Quem chega e não apresenta `impressao_de_quem_vem` — a impressão que o
/// servidor apresentou em `SirvaTelaPara` — é derrubado com
/// [`CODIGO_PAR_RECUSADO`] antes de qualquer canal de aplicação, e a espera
/// continua pelo par certo até `prazo`.
///
/// `prazo_de_confirmacao` limita a espera **depois** do aperto de mão, pela
/// troca de byte de [`confirmar_com_quem_atende`]: sem ele, um par autenticado
/// e parado prenderia a vaga inteira.
///
/// `None` quando o prazo venceu sem o par certo, a ponta fechou, ou quem chegou
/// não confirmou a tempo.
pub async fn atender(
    ponta: &PontaDaSessao,
    impressao_de_quem_vem: &str,
    prazo: std::time::Duration,
    prazo_de_confirmacao: std::time::Duration,
) -> Option<ParLigado> {
    let esperada = impressao_em_bytes(&impressao_de_quem_vem.to_ascii_lowercase())?;
    let ate = tokio::time::Instant::now() + prazo;
    loop {
        let conexao = tokio::time::timeout_at(ate, ponta.ponta().aceitar())
            .await
            .ok()??;
        let remoto = conexao.remote_address();
        let veio = conexao.impressao_dtls_do_par();
        if veio != esperada {
            tracing::warn!(
                par = %remoto,
                esperada = %impressao_de_quem_vem,
                veio = %crate::client::hex(&veio),
                "um par que chegou apresentou uma impressão digital diferente da esperada"
            );
            conexao.close(CODIGO_PAR_RECUSADO, b"impressao");
            continue;
        }
        match tokio::time::timeout(prazo_de_confirmacao, confirmar_para_quem_ligou(&conexao)).await
        {
            Ok(Ok(())) => {}
            Ok(Err(erro)) => {
                tracing::warn!(par = %remoto, %erro, "a troca de confirmação com quem ligou falhou");
                return None;
            }
            Err(_) => {
                tracing::warn!(
                    par = %remoto,
                    ?prazo_de_confirmacao,
                    "quem ligou apertou a mão e não confirmou dentro do prazo"
                );
                conexao.close(CODIGO_PAR_RECUSADO, b"sem confirmacao");
                return None;
            }
        }
        let ida_e_volta = conexao.rtt();
        let como = como_chegou(remoto);
        tracing::info!(par = %remoto, ?como, ?ida_e_volta, "um par foi atendido");
        return Some(ParLigado {
            conexao,
            como,
            ida_e_volta,
        });
    }
}

/// Escreve uma transmissão de tela para um par, por pedaço, a partir do
/// primeiro quadro-chave.
///
/// # Por pedaço, e nunca remontando quadro
///
/// A mesma regra do encaminhamento do servidor: esperar o quadro inteiro para
/// repassar acrescenta um tempo de quadro de atraso **a cada salto**.
///
/// # A abertura vai primeiro, e pedaço nenhum antes dela
///
/// Quem recebe usa `crate::tela::Recepcao::do_fluxo`, que lê o byte de tipo e
/// o [`seele_proto::screen::ScreenHeader`] antes de qualquer coisa.
///
/// # Quem entra por um par entra num quadro-chave
///
/// Um [`crate::tela::Enquadramento`] **conta bytes** através dos pedaços para
/// achar onde um quadro-chave começa; tudo antes dele é descartado, e nada
/// depois.
///
/// # Errors
///
/// [`ErroDePar::Repasse`] quando o canal não abre, um pedaço não escreve, os
/// bytes que chegam já não formam um enquadramento válido, ou o par não
/// confirma o fim da transmissão.
pub async fn repassar(
    ligado: &ParLigado,
    abertura: &[u8],
    mut pedacos: tokio::sync::mpsc::Receiver<Vec<u8>>,
) -> Result<(), ErroDePar> {
    let par = ligado.conexao.remote_address();
    let mut fluxo = ligado
        .conexao
        .open_uni()
        .await
        .map_err(|erro| ErroDePar::Repasse(erro.to_string()))?;
    let _ = fluxo.set_priority(crate::tela::PRIORIDADE_DA_TELA);
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await
        .map_err(|erro| ErroDePar::Repasse(erro.to_string()))?;
    fluxo
        .write_all(abertura)
        .await
        .map_err(|erro| ErroDePar::Repasse(erro.to_string()))?;
    tracing::info!(%par, "repasse de tela para o par começou");

    let mut enquadramento = crate::tela::Enquadramento::novo();
    let mut viu_a_chave = false;
    let mut descartados = 0_u64;
    while let Some(pedaco) = pedacos.recv().await {
        let porta = enquadramento
            .entrada(&pedaco)
            .map_err(|erro| ErroDePar::Repasse(erro.to_string()))?;
        if viu_a_chave {
            fluxo
                .write_all(&pedaco)
                .await
                .map_err(|erro| ErroDePar::Repasse(erro.to_string()))?;
            continue;
        }
        let Some(deslocamento) = porta else {
            descartados += 1;
            continue;
        };
        viu_a_chave = true;
        tracing::debug!(%par, descartados, "quadro-chave achado; repasse ao par passa a valer");
        fluxo
            .write_all(pedaco.get(deslocamento..).unwrap_or_default())
            .await
            .map_err(|erro| ErroDePar::Repasse(erro.to_string()))?;
    }
    if !viu_a_chave {
        tracing::warn!(
            %par,
            descartados,
            "o canal de pedaços fechou e nenhum quadro-chave chegou a atravessar"
        );
    }
    fluxo
        .finish()
        .map_err(|erro| ErroDePar::Repasse(erro.to_string()))?;
    // Espera o par confirmar antes de devolver: sem isto, quem chama não teria
    // como saber quando é seguro soltar o par.
    match fluxo.entregue().await {
        Ok(()) => {
            tracing::info!(%par, "repasse de tela para o par terminou e foi confirmado");
            Ok(())
        }
        Err(erro) => Err(ErroDePar::Repasse(erro.to_string())),
    }
}

/// De onde a imagem desta transmissão vai vir.
#[derive(Debug)]
pub enum PorOndeAssistir {
    /// Por este par.
    Par(Box<ParLigado>),
    /// Pelo servidor, como sempre — e por quê.
    ///
    /// O motivo enumerado existe para o **servidor** saber o que aconteceu:
    /// `ImpressaoNaoBate` é o evento de segurança que o §4 da spec quer contado.
    Servidor(seele_proto::control::MotivoDeFalhaDePar),
}

/// Tenta o par, e cai para o servidor sem drama quando ele não vem.
///
/// **Nunca devolve erro**, e é de propósito: a malha é alívio; falhar nela é
/// voltar ao caminho de antes dela existir.
pub async fn por_onde(
    ponta: &PontaDaSessao,
    enderecos: &[std::net::SocketAddr],
    impressao: String,
    prazo: std::time::Duration,
) -> PorOndeAssistir {
    match ligar(ponta, enderecos, impressao, prazo).await {
        Ok(ligado) => PorOndeAssistir::Par(Box::new(ligado)),
        Err(erro) => {
            let motivo = motivo_de_falha(&erro);
            tracing::info!(%erro, ?motivo, "o par não veio; a tela vem do servidor");
            PorOndeAssistir::Servidor(motivo)
        }
    }
}

/// Traduz o motivo detalhado de [`ligar`] para o motivo enumerado que o
/// protocolo leva ao servidor. Duas traduções exatas, e o resto é rotina.
fn motivo_de_falha(erro: &ErroDePar) -> seele_proto::control::MotivoDeFalhaDePar {
    match erro {
        ErroDePar::ImpressaoNaoBate { .. } => {
            seele_proto::control::MotivoDeFalhaDePar::ImpressaoNaoBate
        }
        ErroDePar::RecusadoDepoisDeLigar(_) => {
            seele_proto::control::MotivoDeFalhaDePar::NaoFuiAceito
        }
        ErroDePar::Certificado(_)
        | ErroDePar::Escuta(_)
        | ErroDePar::NaoAlcancou
        | ErroDePar::ConfirmacaoNaoChegouATempo
        | ErroDePar::Repasse(_) => seele_proto::control::MotivoDeFalhaDePar::NaoAlcancou,
    }
}

/// Se este endereço é da mesma rede, e portanto não precisou de furo.
fn como_chegou(endereco: std::net::SocketAddr) -> ComoChegou {
    let local = match endereco.ip().to_canonical() {
        std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.segments().first().is_some_and(|s| {
                    // `fc00::/7` e `fe80::/10`, o par de `is_private()` e
                    // `is_link_local()` do lado v4.
                    s & 0xfe00 == 0xfc00 || s & 0xffc0 == 0xfe80
                })
        }
    };
    if local {
        ComoChegou::Local
    } else {
        ComoChegou::Furo
    }
}

/// Troca um byte com quem atende, e só então dá a ligação por boa.
///
/// **Por que existe.** Quem atende confere o certificado de quem chegou depois
/// do DTLS; uma recusa chega como o fechamento desta conexão. Um canal de
/// aplicação que só fecha depois de o outro lado responder é o sinal
/// determinístico: numa conexão recusada, a abertura ou a leitura seguinte
/// falha com o motivo de verdade.
async fn confirmar_com_quem_atende(conexao: &Conexao) -> Result<(), MotivoDaRecusa> {
    let (mut envio, mut recebe) = conexao
        .open_bi()
        .await
        .map_err(|motivo| motivo_da_recusa(conexao, &motivo.to_string()))?;
    envio
        .write_all(&[0])
        .await
        .map_err(|erro| motivo_da_recusa(conexao, &erro.to_string()))?;
    let mut resposta = [0_u8; 1];
    recebe
        .read_exact(&mut resposta)
        .await
        .map_err(|erro| motivo_da_recusa(conexao, &erro.to_string()))?;
    let _ = envio.finish();
    Ok(())
}

/// A metade de [`atender`] na troca de [`confirmar_com_quem_atende`].
async fn confirmar_para_quem_ligou(conexao: &Conexao) -> Result<(), std::io::Error> {
    let (mut envio, mut recebe) = conexao.accept_bi().await.map_err(std::io::Error::other)?;
    let mut byte = [0_u8; 1];
    recebe
        .read_exact(&mut byte)
        .await
        .map_err(std::io::Error::other)?;
    envio.write_all(&[0]).await.map_err(std::io::Error::other)?;
    let _ = envio.finish();
    Ok(())
}

/// Por que a conexão caiu durante a confirmação: o código de quem atendeu,
/// quando a conexão já sabe que acabou, e o texto do canal quando não.
fn motivo_da_recusa(conexao: &Conexao, texto: &str) -> MotivoDaRecusa {
    match conexao.acabou() {
        Some(crate::transporte::Motivo::Fechada {
            codigo: CODIGO_PAR_RECUSADO,
        }) => MotivoDaRecusa::CertificadoErrado,
        Some(outro) => MotivoDaRecusa::Outro(outro.to_string()),
        None => MotivoDaRecusa::Outro(texto.to_owned()),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
)]
mod testes {
    use super::*;

    /// O prazo de confirmação que os testes deste módulo dão a `atender`.
    const PRAZO_DE_CONFIRMACAO_NO_TESTE: std::time::Duration = std::time::Duration::from_secs(2);

    /// O prazo de espera de `atender` nos testes.
    const PRAZO_NO_TESTE: std::time::Duration = std::time::Duration::from_secs(5);

    /// A ponta de uma sessão em `127.0.0.1`.
    fn ponta() -> PontaDaSessao {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        crate::transporte::abrir_ponta(Some(socket)).unwrap()
    }

    /// Liga duas pontas de par: `a` atende `b`. Devolve a conexão do lado de
    /// quem atende, a ponta de quem discou e o [`ParLigado`] de quem discou.
    async fn duas_pontas_ligadas() -> (Conexao, PontaDaSessao, PontaDaSessao, ParLigado) {
        let a = ponta();
        let b = ponta();
        passar_a_atender(&a);
        let impressao_b = impressao(b.identidade());
        let impressao_a = impressao(a.identidade());
        let endereco_a = a.ponta().local_addr();
        let atendendo = {
            let a = a.clone();
            tokio::spawn(async move {
                atender(
                    &a,
                    &impressao_b,
                    PRAZO_NO_TESTE,
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };
        let ligado = ligar(
            &b,
            &[endereco_a],
            impressao_a,
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("os dois pares não se ligaram");
        let do_lado_de_quem_atende = atendendo
            .await
            .expect("a tarefa que atende travou")
            .expect("quem atende não recebeu ninguém");
        (do_lado_de_quem_atende.conexao, a, b, ligado)
    }

    #[test]
    fn cada_identidade_e_nova_e_a_impressao_a_distingue() {
        // **Efêmero é o ponto.** Guardar a identidade em disco criaria um
        // identificador estável da máquina de quem empresta.
        let uma = identidade_efemera().unwrap();
        let outra = identidade_efemera().unwrap();
        assert_ne!(impressao(&uma), impressao(&outra));
    }

    #[test]
    fn a_impressao_e_a_do_certificado_e_no_formato_de_sempre() {
        let identidade = identidade_efemera().unwrap();
        assert_eq!(
            impressao(&identidade),
            seele_proto::transport::certificate_fingerprint(identidade.der())
        );
        assert_eq!(impressao(&identidade).len(), 64);
        assert_eq!(
            impressao_em_bytes(&impressao(&identidade)),
            Some(identidade.impressao())
        );
    }

    #[tokio::test]
    async fn uma_ponta_de_cliente_passa_a_atender_sem_socket_novo() {
        // **Nem escuta nova, nem porta nova** (§3.1).
        let ponta = ponta();
        let porta_antes = ponta.ponta().local_addr();
        passar_a_atender(&ponta);
        assert!(ponta.armada());
        assert_eq!(ponta.ponta().local_addr(), porta_antes);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dois_pares_se_ligam_e_o_teste_sabe_como() {
        let (do_lado_de_quem_atende, _a, _b, ligado) = duas_pontas_ligadas().await;
        assert_eq!(
            ligado.como,
            ComoChegou::Local,
            "127.0.0.1 não é rede local?"
        );
        assert!(ligado.conexao.acabou().is_none(), "a conexão já morreu");
        assert!(do_lado_de_quem_atende.acabou().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_par_com_a_impressao_errada_nao_liga() {
        // O caso que separa «não consegui falar com ele» de «alguém respondeu
        // no lugar dele».
        let a = ponta();
        passar_a_atender(&a);
        let endereco_a = a.ponta().local_addr();
        let b = ponta();
        let _atendendo = {
            let a = a.clone();
            let impressao_b = impressao(b.identidade());
            tokio::spawn(async move {
                atender(
                    &a,
                    &impressao_b,
                    PRAZO_NO_TESTE,
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };
        let erro = ligar(
            &b,
            &[endereco_a],
            "f".repeat(64),
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        let ErroDePar::ImpressaoNaoBate { esperada, veio } = &erro else {
            panic!("a recusa saiu com o motivo errado: {erro:?}");
        };
        assert_eq!(esperada, &"f".repeat(64), "não foi a impressão pedida");
        assert_eq!(
            veio,
            &impressao(a.identidade()),
            "o que veio não é a de quem atendeu"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_impostor_na_lista_nao_derruba_a_discagem_inteira() {
        // **A impostura é notícia, não é motivo para desistir.** O ICE pode
        // ficar com o impostor primeiro; a discagem continua com o resto.
        let b = ponta();
        let impressao_b = impressao(b.identidade());

        let impostor = ponta();
        passar_a_atender(&impostor);
        let endereco_impostor = impostor.ponta().local_addr();
        let _atendendo_impostor = {
            let impostor = impostor.clone();
            let impressao_b = impressao_b.clone();
            tokio::spawn(async move {
                atender(
                    &impostor,
                    &impressao_b,
                    PRAZO_NO_TESTE,
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };

        let legitimo = ponta();
        passar_a_atender(&legitimo);
        let esperada = impressao(legitimo.identidade());
        let endereco_legitimo = legitimo.ponta().local_addr();
        let _atendendo_legitimo = {
            let legitimo = legitimo.clone();
            tokio::spawn(async move {
                atender(
                    &legitimo,
                    &impressao_b,
                    PRAZO_NO_TESTE,
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };

        let ligado = ligar(
            &b,
            &[endereco_impostor, endereco_legitimo],
            esperada,
            std::time::Duration::from_secs(8),
        )
        .await
        .expect("o impostor derrubou a discagem inteira");
        assert_eq!(
            ligado.conexao.remote_address().port(),
            endereco_legitimo.port(),
            "a ligação fechou com quem não era o par"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn quem_atende_recusa_quem_o_servidor_nao_apresentou() {
        // **A parede simétrica.** Quem atende confere quem chega, e o intruso
        // apresenta a identidade dele, que não é a esperada.
        let anfitriao = ponta();
        passar_a_atender(&anfitriao);
        let impressao_do_anfitriao = impressao(anfitriao.identidade());
        let onde = anfitriao.ponta().local_addr();
        let esperada_de_outro = impressao(&identidade_efemera().unwrap());
        let atendendo = {
            let anfitriao = anfitriao.clone();
            tokio::spawn(async move {
                atender(
                    &anfitriao,
                    &esperada_de_outro,
                    std::time::Duration::from_millis(1500),
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };

        let intruso = ponta();
        let tentou = ligar(
            &intruso,
            &[onde],
            impressao_do_anfitriao,
            std::time::Duration::from_secs(3),
        )
        .await;
        assert!(
            matches!(
                tentou,
                Err(ErroDePar::RecusadoDepoisDeLigar(
                    MotivoDaRecusa::CertificadoErrado
                ))
            ),
            "o intruso entrou, ou saiu com o motivo errado: {tentou:?}"
        );
        assert!(
            atendendo.await.unwrap().is_none(),
            "o anfitrião deu por boa uma ligação de quem o servidor não apresentou"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn quando_o_anfitriao_recusa_minha_identidade_o_motivo_e_naofuiaceito() {
        let anfitriao = ponta();
        passar_a_atender(&anfitriao);
        let impressao_do_anfitriao = impressao(anfitriao.identidade());
        let onde = anfitriao.ponta().local_addr();
        let esperada_de_outro = impressao(&identidade_efemera().unwrap());
        let _atendendo = {
            let anfitriao = anfitriao.clone();
            tokio::spawn(async move {
                atender(
                    &anfitriao,
                    &esperada_de_outro,
                    PRAZO_NO_TESTE,
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };
        let quem_disca = ponta();
        let onde = por_onde(
            &quem_disca,
            &[onde],
            impressao_do_anfitriao,
            std::time::Duration::from_secs(3),
        )
        .await;
        assert!(
            matches!(
                onde,
                PorOndeAssistir::Servidor(seele_proto::control::MotivoDeFalhaDePar::NaoFuiAceito)
            ),
            "o motivo não foi NaoFuiAceito: {onde:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn atender_nao_trava_para_sempre_com_um_par_que_aperta_a_mao_e_para() {
        // Um par que completa o DTLS e nunca abre o canal de confirmação.
        let anfitriao = ponta();
        passar_a_atender(&anfitriao);
        let quieto = ponta();
        let impressao_do_quieto = impressao(quieto.identidade());
        let onde = anfitriao.ponta().local_addr();
        let prazo_de_confirmacao = std::time::Duration::from_millis(150);
        let atendendo = {
            let anfitriao = anfitriao.clone();
            tokio::spawn(async move {
                atender(
                    &anfitriao,
                    &impressao_do_quieto,
                    PRAZO_NO_TESTE,
                    prazo_de_confirmacao,
                )
                .await
            })
        };
        let mut discagem =
            crate::transporte::Discagem::nova(vec![onde], crate::transporte::Pino::Qualquer);
        discagem.certificado = Some(quieto.identidade().clone());
        let conexao_quieta = quieto.ponta().discar(discagem).await.unwrap();

        let resultado = tokio::time::timeout(std::time::Duration::from_secs(3), atendendo)
            .await
            .expect("atender devia ter voltado dentro do próprio prazo, e travou")
            .expect("a tarefa de atender morreu");
        assert!(
            resultado.is_none(),
            "atender deu por boa uma ligação que apertou a mão e nunca confirmou"
        );
        drop(conexao_quieta);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ligar_nao_diz_naoalcancou_para_quem_apertou_a_mao_e_travou() {
        // O anfitrião completa o DTLS e não abre o canal de confirmação.
        let anfitriao = ponta();
        passar_a_atender(&anfitriao);
        let impressao_do_anfitriao = impressao(anfitriao.identidade());
        let onde = anfitriao.ponta().local_addr();
        let _segurando = {
            let anfitriao = anfitriao.clone();
            tokio::spawn(async move {
                if let Some(_conexao_apertada) = anfitriao.ponta().aceitar().await {
                    std::future::pending::<()>().await;
                }
            })
        };
        let quem_disca = ponta();
        let erro = ligar(
            &quem_disca,
            &[onde],
            impressao_do_anfitriao,
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(erro, ErroDePar::ConfirmacaoNaoChegouATempo),
            "o par apertou a mão e travou, e o motivo devolvido foi outro: {erro:?}"
        );
    }

    #[test]
    fn o_que_e_da_mesma_rede_nunca_conta_como_furo() {
        for texto in [
            "127.0.0.1:9",
            "10.0.0.1:9",
            "172.16.0.1:9",
            "192.168.1.10:9",
            "169.254.7.7:9",
            "[::1]:9",
            "[fd00::1]:9",
            "[fe80::1]:9",
            "[febf:ffff::1]:9",
            "[::ffff:192.168.1.10]:9",
        ] {
            assert_eq!(
                como_chegou(texto.parse().unwrap()),
                ComoChegou::Local,
                "{texto} é da mesma rede e foi contado como furo"
            );
        }
        for texto in ["203.0.113.5:9", "[2001:db8::1]:9"] {
            assert_eq!(como_chegou(texto.parse().unwrap()), ComoChegou::Furo);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn o_prazo_manda_quando_ninguem_responde() {
        let b = ponta();
        let comecou = std::time::Instant::now();
        let erro = ligar(
            &b,
            &[
                "192.0.2.1:9".parse().unwrap(),
                "198.51.100.1:9".parse().unwrap(),
            ],
            "f".repeat(64),
            std::time::Duration::from_millis(500),
        )
        .await
        .unwrap_err();
        assert!(matches!(erro, ErroDePar::NaoAlcancou), "{erro:?}");
        assert!(comecou.elapsed() < std::time::Duration::from_secs(3));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn parar_de_atender_desarma_o_que_passar_a_atender_armou() {
        // As identidades combinam de propósito: armada, esta discagem fecharia.
        let ponta_que_atende = ponta();
        passar_a_atender(&ponta_que_atende);
        let impressao_da_ponta = impressao(ponta_que_atende.identidade());
        let onde = ponta_que_atende.ponta().local_addr();
        parar_de_atender(&ponta_que_atende);
        assert!(!ponta_que_atende.armada());

        let discando = ponta();
        let _atendendo = {
            let ponta_que_atende = ponta_que_atende.clone();
            let impressao_de_quem_disca = impressao(discando.identidade());
            tokio::spawn(async move {
                atender(
                    &ponta_que_atende,
                    &impressao_de_quem_disca,
                    PRAZO_NO_TESTE,
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };
        let tentou = ligar(
            &discando,
            &[onde],
            impressao_da_ponta,
            std::time::Duration::from_millis(800),
        )
        .await;
        assert!(
            matches!(tentou, Err(ErroDePar::NaoAlcancou)),
            "a ponta continuou atendendo depois de parar_de_atender: {tentou:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_ponta_atende_um_par_sem_derrubar_a_conexao_que_ja_tinha() {
        // **A propriedade central do §3.1.** A ponta de `b` já tem uma conexão
        // viva (com `a`); passar a atender um terceiro na mesma ponta não a
        // derruba.
        let (do_lado_de_a, _a, b, ligado_com_a) = duas_pontas_ligadas().await;

        passar_a_atender(&b);
        let terceiro = ponta();
        let impressao_do_terceiro = impressao(terceiro.identidade());
        let onde_b = b.ponta().local_addr();
        let atendendo = {
            let b = b.clone();
            tokio::spawn(async move {
                atender(
                    &b,
                    &impressao_do_terceiro,
                    PRAZO_NO_TESTE,
                    PRAZO_DE_CONFIRMACAO_NO_TESTE,
                )
                .await
            })
        };
        let _ligado_pelo_terceiro = ligar(
            &terceiro,
            &[onde_b],
            impressao(b.identidade()),
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("o terceiro par não ligou para a ponta que também atende");
        let _atendido = atendendo
            .await
            .unwrap()
            .expect("a ponta não aceitou o terceiro");

        // A conexão de antes continua trocando bytes.
        let eco = tokio::spawn(async move {
            let (mut envio, mut recebe) = do_lado_de_a.accept_bi().await.unwrap();
            let mut buf = [0_u8; 10];
            recebe.read_exact(&mut buf).await.unwrap();
            envio.write_all(&buf).await.unwrap();
            let _ = envio.finish();
            do_lado_de_a
        });
        let (mut envio, mut recebe) = ligado_com_a.conexao.open_bi().await.unwrap();
        envio.write_all(b"ainda viva").await.unwrap();
        let mut resposta = [0_u8; 10];
        recebe.read_exact(&mut resposta).await.unwrap();
        assert_eq!(&resposta, b"ainda viva");
        let _ = eco.await.unwrap();
    }

    /// Os bytes de um quadro completo, pelo mesmo escritor que o emissor usa.
    fn quadro(tipo: crate::tela::TipoDeQuadro, corpo: &[u8]) -> Vec<u8> {
        let tamanho = u32::try_from(corpo.len()).unwrap();
        let mut bytes = crate::tela::cabecalho_de_quadro(tipo, tamanho).to_vec();
        bytes.extend_from_slice(corpo);
        bytes
    }

    /// Repassa `pedacos` e devolve o que o outro lado leu depois do byte de
    /// tipo.
    async fn repassar_e_ler(pedacos: Vec<Vec<u8>>) -> (Vec<u8>, Vec<u8>) {
        let (a, _pa, _pb, ligado) = duas_pontas_ligadas().await;
        let abertura = vec![9_u8; seele_proto::screen::SCREEN_HEADER_LEN];
        let abertura_clone = abertura.clone();
        let (manda, recebe) = tokio::sync::mpsc::channel(8);
        let repassando =
            tokio::spawn(async move { repassar(&ligado, &abertura_clone, recebe).await });
        for pedaco in pedacos {
            manda.send(pedaco).await.unwrap();
        }
        drop(manda);
        let mut fluxo = a.accept_uni().await.unwrap();
        let mut tipo = [0_u8; 1];
        fluxo.read_exact(&mut tipo).await.unwrap();
        assert_eq!(tipo[0], seele_proto::stream::StreamType::Screen.byte());
        let veio = fluxo.read_to_end(1024).await.unwrap();
        repassando.await.unwrap().unwrap();
        (abertura, veio)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn o_par_recebe_a_abertura_e_os_pedacos_na_ordem() {
        let chave = quadro(crate::tela::TipoDeQuadro::Chave, &[1, 2, 3]);
        let comum = quadro(crate::tela::TipoDeQuadro::Comum, &[4, 5]);
        let (abertura, veio) = repassar_e_ler(vec![chave.clone(), comum.clone()]).await;
        assert_eq!(veio, [abertura, chave, comum].concat());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn quem_entra_por_um_par_entra_num_quadro_chave() {
        let comum = quadro(crate::tela::TipoDeQuadro::Comum, &[1, 2]);
        let chave = quadro(crate::tela::TipoDeQuadro::Chave, &[3, 4]);
        let (abertura, veio) = repassar_e_ler(vec![comum, chave.clone()]).await;
        assert_eq!(
            veio,
            [abertura, chave].concat(),
            "o quadro comum que veio antes da chave atravessou, ou a chave não atravessou"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_pedaco_com_dois_quadros_nao_perde_o_quadro_chave_que_vem_depois_do_comum() {
        let comum = quadro(crate::tela::TipoDeQuadro::Comum, &[1, 2]);
        let chave = quadro(crate::tela::TipoDeQuadro::Chave, &[3, 4]);
        let (abertura, veio) = repassar_e_ler(vec![[comum, chave.clone()].concat()]).await;
        assert_eq!(veio, [abertura, chave].concat());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_quadro_comum_partido_entre_pedacos_nao_vira_cabecalho_falso() {
        let comum = quadro(crate::tela::TipoDeQuadro::Comum, &[9, 9, 9, 1, 9, 9]);
        let (primeiro, segundo) = comum.split_at(crate::tela::CABECALHO_DE_QUADRO_LEN + 3);
        assert_eq!(
            segundo.first().copied(),
            Some(crate::tela::TipoDeQuadro::Chave.byte())
        );
        let chave = quadro(crate::tela::TipoDeQuadro::Chave, &[7, 8]);
        let (abertura, veio) =
            repassar_e_ler(vec![primeiro.to_vec(), segundo.to_vec(), chave.clone()]).await;
        assert_eq!(veio, [abertura, chave].concat());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn quem_caiu_antes_do_repasse_e_relatado_como_repasse_e_nao_escuta() {
        let (a, _pa, _pb, ligado) = duas_pontas_ligadas().await;
        a.close(0, b"caiu antes do repasse");
        let _ = ligado.conexao.closed().await;
        let abertura = vec![9_u8; seele_proto::screen::SCREEN_HEADER_LEN];
        let (_manda, recebe) = tokio::sync::mpsc::channel(8);
        let erro = repassar(&ligado, &abertura, recebe)
            .await
            .expect_err("o par já caiu; repassar não deveria conseguir abrir um canal");
        assert!(matches!(erro, ErroDePar::Repasse(_)), "{erro:?}");
    }

    #[tokio::test]
    async fn repassar_avisa_quando_o_canal_fecha_sem_a_chave_chegar() {
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::WARN);
        let _guarda = tracing::subscriber::set_default(captura.clone());

        let (a, _pa, _pb, ligado) = duas_pontas_ligadas().await;
        let abertura = vec![9_u8; seele_proto::screen::SCREEN_HEADER_LEN];
        let (manda, recebe) = tokio::sync::mpsc::channel(8);
        manda
            .send(quadro(crate::tela::TipoDeQuadro::Comum, &[1, 2]))
            .await
            .unwrap();
        drop(manda);
        let lendo = tokio::spawn(async move {
            if let Ok(mut fluxo) = a.accept_uni().await {
                let _ = fluxo.read_to_end(1024).await;
            }
            a
        });
        repassar(&ligado, &abertura, recebe)
            .await
            .expect("nenhum par caiu; o repasse tem de terminar limpo mesmo sem chave");
        let _ = lendo.await;
        let eventos = captura.linhas();
        assert!(
            eventos
                .iter()
                .any(|linha| linha.starts_with("WARN") && linha.contains("nenhum quadro-chave")),
            "o canal fechou sem chave nenhuma atravessar, e nenhum WARN foi ao rastro: {eventos:?}"
        );
    }
}
