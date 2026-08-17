//! Subir um Dogma dentro de outro programa.
//!
//! O `seeled` existe para quem quer um Dogma no ar o tempo todo, numa VPS, sob
//! um supervisor. Este módulo é para o outro caso, que é o mais comum entre
//! amigos: **alguém quer conversar agora e está disposto a ser o anfitrião
//! enquanto a conversa dura.**
//!
//! Sem isto, essa pessoa precisa saber o que é uma linha de comando — e num
//! produto cujo argumento inteiro é "hospede você mesmo", exigir isso de quem
//! hospeda exclui justamente quem mais ganharia. Os dois clientes chamam daqui:
//! `plug --hospedar` e o botão **Hospedar** do app.
//!
//! # O que isto **não** é
//!
//! Não substitui o `seeled`. Um Dogma hospedado por um cliente morre quando o
//! cliente fecha, e isso é correto para "estou hospedando uma conversa" e
//! errado para "mantenho um Dogma no ar". São dois produtos e continuam sendo
//! dois programas.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;

use crate::casper::Location;
use crate::{DogmaConfig, Server};

/// Um Dogma rodando dentro deste processo.
///
/// Descartar isto encerra o Dogma e derruba quem estiver conectado. É o
/// comportamento certo: o anfitrião fechou.
pub struct Hospedagem {
    server: Arc<Server>,
    endereco: SocketAddr,
    /// A escada do ADR 0022, já subida, com a porta do roteador presa nela.
    escada: Option<crate::alcance::Escada>,
    /// A tarefa que aceita conexões.
    ///
    /// Guardada para poder ser esperada: ela segura uma referência ao servidor,
    /// e o socket UDP só é devolvido ao sistema quando a **última** referência
    /// some. Sem esperá-la, encerrar e reabrir na mesma porta falha.
    aceitando: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for Hospedagem {
    fn fmt(&self, formatador: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatador
            .debug_struct("Hospedagem")
            .field("endereco", &self.endereco)
            .finish()
    }
}

impl Hospedagem {
    /// Sobe um Dogma e começa a aceitar conexões.
    ///
    /// `porta` zero deixa o sistema escolher — útil para teste, e ruim para uso
    /// real, onde o anfitrião precisa dizer aos amigos onde bater.
    ///
    /// Escuta em `[::]` de propósito: um Dogma hospedado que só aceitasse
    /// `localhost` não serviria para nada além de falar consigo mesmo, e o
    /// ponto todo é receber gente. `[::]` e não `0.0.0.0` porque o segundo
    /// atende só IPv4 — degrau 2 do ADR 0022, ver [`crate::alcance`].
    ///
    /// # Errors
    ///
    /// Falha se a porta já estiver em uso ou se o banco não abrir.
    pub async fn iniciar(porta: u16, banco: Location, nome: &str) -> Result<Self> {
        let config = DogmaConfig {
            name: nome.to_owned(),
            listen: SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, porta)),
            database: banco,
            ..DogmaConfig::default()
        };

        let server = Arc::new(Server::bind(config).await?);
        let endereco = server.local_addr()?;

        // A escada do ADR 0022, aqui e não no `Server`: quem hospeda de dentro
        // do cliente é justamente quem está atrás de um roteador doméstico. Um
        // `seeled` numa VPS já é o degrau 1 e não tem o que pedir a ninguém.
        //
        // Custa até `alcance::porta::PROCURA` no pior caso, e o pior caso é o
        // comum: numa rede sem UPnP a busca esgota o prazo inteiro. Foi por
        // isso que aquele prazo é curto.
        let escada = crate::alcance::Escada::subir(endereco.port()).await;
        tracing::info!(alcance = ?escada.alcance(), "escada do ADR 0022 subida");

        // O laço de aceitação numa tarefa própria: quem chamou tem interface
        // para desenhar, e o `run` só volta quando o Dogma acaba.
        let referencia = Arc::clone(&server);
        let aceitando = tokio::spawn(async move {
            if let Err(erro) = referencia.run().await {
                tracing::error!(%erro, "o Dogma hospedado parou");
            }
        });

        Ok(Self {
            server,
            endereco,
            escada: Some(escada),
            aceitando: Some(aceitando),
        })
    }

    /// Até onde este Dogma é alcançável, e por qual degrau do ADR 0022.
    ///
    /// É o que permite à casca dizer "só na sua rede, e foi por isto" em vez de
    /// deixar quem hospeda achando que abriu para o mundo.
    #[must_use]
    pub fn alcance(&self) -> Option<&crate::alcance::Alcance> {
        self.escada.as_ref().map(crate::alcance::Escada::alcance)
    }

    /// Onde o Dogma está escutando.
    #[must_use]
    pub fn endereco(&self) -> SocketAddr {
        self.endereco
    }

    /// O endereço que se manda para os amigos.
    ///
    /// `0.0.0.0` é onde se escuta, não um lugar aonde alguém possa ir. Isto
    /// devolve o endereço desta máquina na rede — o mesmo raciocínio do
    /// `seeled` ao imprimir o que digitar na outra máquina.
    #[must_use]
    pub fn endereco_na_rede(&self) -> Option<SocketAddr> {
        endereco_de_rede().map(|ip| SocketAddr::new(ip, self.endereco.port()))
    }

    /// A impressão digital que os clientes vão fixar. ADR 0003.
    #[must_use]
    pub fn impressao_digital(&self) -> &str {
        self.server.fingerprint()
    }

    /// O link para mandar aos amigos.
    ///
    /// Mora aqui, e não em cada casca, porque quem sabe montá-lo é quem tem as
    /// duas partes: o endereço em que dá para chegar e a impressão digital
    /// desta instância. Os dois clientes montavam o mesmo link à mão, e duas
    /// cópias de uma construção é uma que vai ficar para trás.
    ///
    /// Sem rede, cai no endereço de escuta — que não serve para convidar
    /// ninguém, mas é a resposta honesta, e quem chamou pode dizer isso.
    ///
    /// # Qual dos endereços entra aqui
    ///
    /// O do degrau mais alto que a escada do ADR 0022 alcançou, e não mais
    /// sempre o da rede local. Um IPv4 público vindo do roteador alcança
    /// praticamente todo cliente; um IPv6 global alcança de qualquer lugar,
    /// mas só quem também tem IPv6; o da LAN só alcança quem está na sala ao
    /// lado. O link carrega a porta, então uma porta externa diferente da
    /// interna — o roteador pode ter dado outra — não atrapalha quem recebe.
    ///
    /// `SocketAddr` e nunca `format!("{ip}:{porta}")`: o `Display` do
    /// `SocketAddr` põe os colchetes num IPv6, e agora que este endereço pode
    /// **ser** IPv6 isso deixou de ser detalhe.
    #[must_use]
    pub fn convite(&self) -> String {
        let alvo = self.alcance().map_or_else(
            || self.endereco_na_rede().unwrap_or(self.endereco).to_string(),
            |alcance| alcance.alvo.to_string(),
        );
        seele_proto::uri::Convite::novo(alvo)
            .com_impressao_digital(self.impressao_digital())
            .to_string()
    }

    /// Encerra o Dogma e devolve a porta.
    ///
    /// Consome, e a espera está aqui de propósito. Fechar uma conversa e abrir
    /// outra é o caso normal, e sem esperar isso falha com "endereço já em
    /// uso" — um erro que não diz nada a quem só clicou em parar e começar de
    /// novo. O custo cai onde a pessoa já espera uma pausa, ao fechar, e não
    /// onde ela espera rapidez, ao abrir.
    ///
    /// São três esperas e cada uma tem um motivo: as conexões terminarem, a
    /// tarefa de aceitação soltar a referência dela, e o driver do QUIC
    /// devolver o socket depois que a última referência some.
    pub async fn encerrar(mut self) {
        // Primeiro a porta do roteador. Uma regra deixada para trás aponta para
        // uma máquina que não vai mais atender, e só some quando o prazo vence.
        if let Some(escada) = self.escada.take() {
            escada.descer().await;
        }
        self.server.shutdown();
        self.server.wait_idle().await;
        if let Some(aceitando) = self.aceitando.take() {
            let _ = aceitando.await;
        }
        drop(self);
        // Medido: sem isto, reabrir a mesma porta em seguida falha. O driver do
        // endpoint fecha o socket ao ser recolhido, e isso leva um instante que
        // não dá para observar de fora.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

impl Drop for Hospedagem {
    /// Sinaliza o fechamento, sem esperar — `drop` não pode aguardar.
    ///
    /// Quem vai reabrir a porta em seguida deve chamar [`Hospedagem::encerrar`]
    /// antes de descartar.
    fn drop(&mut self) {
        self.server.shutdown();
    }
}

/// O endereço desta máquina na rede que ela usaria para sair.
///
/// Sem dependência e sem enumerar interfaces: conectar um socket UDP escolhe
/// uma rota e associa um endereço local sem enviar pacote nenhum, que é
/// exatamente a pergunta — "qual dos meus endereços outra pessoa veria". O
/// alvo é TEST-NET-3 (`203.0.113.0/24`, RFC 5737), reservado para
/// documentação, então nada é insinuado sobre alcançar um host real.
#[must_use]
pub fn endereco_de_rede() -> Option<std::net::IpAddr> {
    crate::alcance::endereco_de_saida_v4()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn um_dogma_hospedado_aceita_conexao() {
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("subir");

        assert_ne!(hospedagem.endereco().port(), 0, "não escolheu porta");
        assert_eq!(
            hospedagem.impressao_digital().len(),
            64,
            "a impressão digital não é um SHA-256"
        );
    }

    #[tokio::test]
    async fn escuta_em_todas_as_interfaces_e_nao_so_em_localhost() {
        // Um Dogma hospedado que só aceitasse localhost serviria para falar
        // sozinho, que é o oposto do motivo de existir.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("subir");
        assert!(hospedagem.endereco().ip().is_unspecified());
    }

    #[tokio::test]
    async fn o_endereco_para_os_amigos_nao_e_o_de_escuta() {
        // `0.0.0.0` é onde se escuta, não um lugar aonde alguém possa ir.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("subir");

        if let Some(rede) = hospedagem.endereco_na_rede() {
            assert!(!rede.ip().is_unspecified());
            assert!(!rede.ip().is_loopback());
            assert_eq!(rede.port(), hospedagem.endereco().port());
        }
        // Numa máquina sem rede não há o que devolver, e `None` é a resposta
        // honesta — quem chamou mostra o endereço de escuta e avisa.
    }

    #[tokio::test]
    async fn o_convite_carrega_a_impressao_digital_desta_instancia() {
        // Sem ela o primeiro contato volta a ser cego, e o convite deixa de ser
        // a coisa que torna o TOFU verificável.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("subir");

        let convite = hospedagem.convite();
        let lido = seele_proto::uri::analisar(&convite).expect("o convite não se lê de volta");
        assert_eq!(
            lido.impressao_digital.as_deref(),
            Some(hospedagem.impressao_digital())
        );
        assert!(
            !convite.contains("0.0.0.0"),
            "convidou para o nada: {convite}"
        );
    }

    #[tokio::test]
    async fn encerrar_libera_a_porta_para_hospedar_de_novo() {
        // Fechar uma conversa e abrir outra é o caso normal, e sem esperar o
        // endpoint terminar isso falha com "endereço já em uso".
        let primeira = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("subir");
        let porta = primeira.endereco().port();
        primeira.encerrar().await;

        let segunda = Hospedagem::iniciar(porta, Location::Memory, "Casa").await;
        assert!(
            segunda.is_ok(),
            "a porta continuou presa: {:?}",
            segunda.err()
        );
    }
}
