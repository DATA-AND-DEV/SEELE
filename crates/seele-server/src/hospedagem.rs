//! Subir um servidor dentro de outro programa.
//!
//! O `seeled` existe para quem quer um servidor no ar o tempo todo, numa VPS, sob
//! um supervisor. Este módulo é para o outro caso, que é o mais comum entre
//! amigos: **alguém quer conversar agora e está disposto a ser o anfitrião
//! enquanto a conversa dura.**
//!
//! Sem isto, essa pessoa precisa saber o que é uma linha de comando — e num
//! produto cujo argumento inteiro é "hospede você mesmo", exigir isso de quem
//! hospeda exclui justamente quem mais ganharia. Os dois clientes chamam daqui:
//! `connection --hospedar` e o botão **Hospedar** do app.
//!
//! # O que isto **não** é
//!
//! Não substitui o `seeled`. Um servidor hospedado por um cliente morre quando o
//! cliente fecha, e isso é correto para "estou hospedando uma conversa" e
//! errado para "mantenho um servidor no ar". São dois produtos e continuam sendo
//! dois programas.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;

use crate::persistence::Location;
use crate::{ServerConfig, Daemon};

/// O PERSISTENCE de um servidor hospedado, compartilhado com quem o hospeda.
///
/// Apelido, e não o tipo escrito por extenso, porque quem chama é a casca do
/// desktop e ela **não depende de `tokio`**. Sem este nome, expor o banco
/// obrigaria o app a declarar a dependência só para escrever o tipo de uma
/// variável — uma dependência inteira paga em nome de uma anotação.
pub type CasperCompartilhado = Arc<tokio::sync::Mutex<crate::persistence::Persistence>>;

/// Um servidor rodando dentro deste processo.
///
/// Descartar isto encerra o servidor e derruba quem estiver conectado. É o
/// comportamento certo: o anfitrião fechou.
pub struct Hospedagem {
    server: Arc<Daemon>,
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
    /// Sobe um servidor e começa a aceitar conexões.
    ///
    /// `porta` zero deixa o sistema escolher — útil para teste, e ruim para uso
    /// real, onde o anfitrião precisa dizer aos amigos onde bater.
    ///
    /// Escuta em `[::]` de propósito: um servidor hospedado que só aceitasse
    /// `localhost` não serviria para nada além de falar consigo mesmo, e o
    /// ponto todo é receber gente. `[::]` e não `0.0.0.0` porque o segundo
    /// atende só IPv4 — degrau 2 do ADR 0022, ver [`crate::alcance`].
    ///
    /// # Errors
    ///
    /// Falha se a porta já estiver em uso ou se o banco não abrir.
    pub async fn iniciar(porta: u16, banco: Location, nome: &str) -> Result<Self> {
        let config = ServerConfig {
            name: nome.to_owned(),
            listen: SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, porta)),
            database: banco,
            ..ServerConfig::default()
        };

        let server = Arc::new(Daemon::bind(config).await?);
        let endereco = server.local_addr()?;

        // A escada do ADR 0022, aqui e não no `Daemon`: quem hospeda de dentro
        // do cliente é justamente quem está atrás de um roteador doméstico. Um
        // `seeled` numa VPS já é o degrau 1 e não tem o que pedir a ninguém.
        //
        // Custa até `alcance::porta::PROCURA` no pior caso, e o pior caso é o
        // comum: numa rede sem UPnP a busca esgota o prazo inteiro. Foi por
        // isso que aquele prazo é curto.
        //
        // A escuta **inteira**, e não só a porta: numa máquina em que a pilha
        // dupla falhou o servidor atende só em IPv4, e a escada não pode prometer
        // um degrau que este socket não serve. Ver `alcance::Escuta`.
        //
        // O degrau 4 vai junto, e ele é opcional em três sentidos: só é tentado
        // se os degraus de cima não resolveram, só existe se o ambiente pedir
        // (`$SEELE_ENCONTRO`), e só funciona com o socket do próprio servidor — sem
        // ele não há furo possível, e o degrau simplesmente não acontece.
        //
        // Custa até `alcance::encontro::PRAZO` a mais, e só no caminho que hoje
        // termina em "só funciona na sua rede".
        let convocacao = server.espelho().and_then(|socket| {
            crate::alcance::encontro::Convocacao::do_ambiente(socket, server.fingerprint())
        });
        let escada = crate::alcance::Escada::subir(
            crate::alcance::Escuta::nova(endereco.port(), server.pilha()),
            convocacao,
        )
        .await;
        tracing::info!(alcance = ?escada.alcance(), "escada do ADR 0022 subida");

        // O laço de aceitação numa tarefa própria: quem chamou tem interface
        // para desenhar, e o `run` só volta quando o servidor acaba.
        let referencia = Arc::clone(&server);
        let aceitando = tokio::spawn(async move {
            if let Err(erro) = referencia.run().await {
                tracing::error!(%erro, "o servidor hospedado parou");
            }
        });

        Ok(Self {
            server,
            endereco,
            escada: Some(escada),
            aceitando: Some(aceitando),
        })
    }

    /// Até onde este servidor é alcançável, e por qual degrau do ADR 0022.
    ///
    /// É o que permite à casca dizer "só na sua rede, e foi por isto" em vez de
    /// deixar quem hospeda achando que abriu para o mundo.
    #[must_use]
    pub fn alcance(&self) -> Option<&crate::alcance::Alcance> {
        self.escada.as_ref().map(crate::alcance::Escada::alcance)
    }

    /// Onde o servidor está escutando.
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
    /// # Quais endereços entram aqui
    ///
    /// **Todos** os que a escada do ADR 0022 achou, na ordem em que se tenta:
    /// rede de casa primeiro, endereço global depois, porta do roteador em
    /// seguida, túnel por último. Ver `alcance::Alcance`.
    ///
    /// Foi um endereço só até 0.5.0, e essa é a história do defeito: com um
    /// endereço só, alguma situação sempre perde. Escolher o degrau mais alto
    /// fazia o link deixar de funcionar para quem estava na mesma casa — a
    /// maioria dos roteadores domésticos não devolve para dentro o próprio
    /// endereço externo.
    ///
    /// `SocketAddr` e nunca `format!("{ip}:{porta}")`: o `Display` do
    /// `SocketAddr` põe os colchetes num IPv6, e agora que estes endereços
    /// podem **ser** IPv6 isso deixou de ser detalhe.
    #[must_use]
    pub fn convite(&self) -> String {
        let alvos: Vec<String> = self.alcance().map_or_else(
            || vec![self.endereco_na_rede().unwrap_or(self.endereco).to_string()],
            |alcance| alcance.alvos().iter().map(SocketAddr::to_string).collect(),
        );
        let (primeiro, resto) = alvos.split_first().map_or_else(
            || (self.endereco.to_string(), &[][..]),
            |(um, resto)| (um.clone(), resto),
        );
        let convite = seele_proto::uri::Convite::novo(primeiro)
            .com_alternativos(resto.to_vec())
            .com_impressao_digital(self.impressao_digital());
        // O bilhete de encontro só entra quando o degrau 4 deu — e ele só é
        // tentado quando os de cima não deram. Um servidor alcançável de fora não
        // põe ponto de encontro nenhum no link de ninguém.
        match self
            .escada
            .as_ref()
            .and_then(crate::alcance::Escada::bilhete)
        {
            Some(bilhete) => convite.com_bilhete(bilhete).to_string(),
            None => convite.to_string(),
        }
    }

    /// O PERSISTENCE deste servidor, para quem hospeda mexer na própria porta.
    ///
    /// ADR 0030. É por aqui que a janela fecha o servidor, gera convite e decide
    /// quem entra — direto no banco da máquina, e não pelo fio como toda a
    /// moderação faz.
    ///
    /// Três motivos, no ADR: fechar a porta não pode depender de estar dentro,
    /// senão a defesa depende do canal que ela defende; a porta se fecha no
    /// mesmo gesto de hospedar, antes do primeiro pacote; e nenhum verbo novo de
    /// protocolo significa nenhuma superfície nova exposta à internet para uma
    /// decisão que é, por definição, de quem está na máquina.
    ///
    /// Devolve o `Arc` em vez de travar aqui de propósito: quem chama está
    /// segurando um `Mutex` de estado do app, e travar deste lado o faria
    /// segurar os dois. Clonar é barato, e o `await` acontece depois de o outro
    /// já ter sido solto.
    #[must_use]
    pub fn persistence(&self) -> CasperCompartilhado {
        Arc::clone(&self.server.server().persistence)
    }

    /// O link para mandar a uma pessoa, com um convite de uso único dentro.
    ///
    /// Igual ao [`Self::convite`] em tudo — os mesmos endereços, na mesma ordem,
    /// com a mesma impressão digital — mais o token. Reusa aquele em vez de
    /// remontar porque duas construções do mesmo link é uma que vai ficar para
    /// trás, que é o que o comentário dele já dizia.
    #[must_use]
    pub fn convite_com_token(&self, token: &str) -> String {
        match seele_proto::uri::analisar(&self.convite()) {
            Ok(convite) => convite.com_token(token).to_string(),
            // O link que acabamos de montar não parsear é impossível na prática;
            // devolver o sem token é melhor que entregar nada a quem convida.
            Err(_) => self.convite(),
        }
    }

    /// Encerra o servidor e devolve a porta.
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

/// O endereço desta máquina na rede de casa.
///
/// Pergunta às interfaces, e não à rota padrão: com uma VPN ligada a rota
/// padrão é a do túnel, e o endereço que ela devolve não é alcançável por
/// ninguém na mesma sala. Ver `alcance::endereco_de_rede_local`.
#[must_use]
pub fn endereco_de_rede() -> Option<std::net::IpAddr> {
    crate::alcance::endereco_de_rede_local()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn um_server_hospedado_aceita_conexao() {
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
        // Um servidor hospedado que só aceitasse localhost serviria para falar
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
    async fn o_convite_leva_o_endereco_da_rede_local_quando_ele_existe() {
        // O defeito de campo, do lado de quem gera o link: em 0.5.0 o convite
        // levava só o endereço do degrau mais alto, e quem estava na mesma casa
        // deixou de conseguir entrar. Agora o endereço da rede local está
        // sempre lá — e é o primeiro, porque é o caso comum.
        let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("subir");
        let convite = hospedagem.convite();
        let lido = seele_proto::uri::analisar(&convite).expect("o convite não se lê de volta");

        let Some(rede) = hospedagem.endereco_na_rede() else {
            eprintln!("pulado: esta máquina não tem endereço de rede local");
            return;
        };
        let candidatos: Vec<&str> = lido.candidatos().collect();
        assert!(
            candidatos.contains(&rede.to_string().as_str()),
            "o convite não leva o endereço da rede local ({rede}): {candidatos:?}"
        );
    }

    /// Um endereço para onde não há rota em lugar nenhum: TEST-NET-1 (RFC 5737).
    ///
    /// É o mais perto de "o ponto de encontro está fora do ar" que cabe num
    /// teste sem rede — um pacote sai e nunca volta resposta.
    const BURACO_NEGRO: &str = "192.0.2.1:8384";

    #[tokio::test]
    async fn um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada() {
        // O requisito do ADR 0022 escrito como asserção, e ele é sobre o degrau
        // 4 **não** poder virar ponto único de falha: com o ponto de encontro
        // inalcançável, hospedar continua funcionando, o link continua saindo
        // com os mesmos endereços de sempre, e a espera a mais é no máximo o
        // prazo escolhido — não uma espera de rede sem fim.
        let antes = std::env::var(crate::alcance::encontro::VARIAVEL).ok();
        let restaurar = |valor: Option<String>| match valor {
            Some(valor) => std::env::set_var(crate::alcance::encontro::VARIAVEL, valor),
            None => std::env::remove_var(crate::alcance::encontro::VARIAVEL),
        };

        // Sem degrau 4 nenhum: é o servidor de antes desta mudança, e é a régua.
        std::env::set_var(crate::alcance::encontro::VARIAVEL, "nao");
        let relogio = std::time::Instant::now();
        let sem = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("subir sem ponto de encontro");
        let sem_encontro = relogio.elapsed();
        let convite_de_antes = sem.convite();
        let degrau_de_antes = sem.alcance().map(crate::alcance::Alcance::degrau);
        sem.encerrar().await;

        // Com um ponto de encontro que não responde nunca.
        std::env::set_var(crate::alcance::encontro::VARIAVEL, BURACO_NEGRO);
        let relogio = std::time::Instant::now();
        let com = Hospedagem::iniciar(0, Location::Memory, "Casa")
            .await
            .expect("hospedar falhou porque o ponto de encontro está fora do ar");
        let com_encontro = relogio.elapsed();
        let convite_de_agora = com.convite();
        let degrau_de_agora = com.alcance().map(crate::alcance::Alcance::degrau);
        com.encerrar().await;
        restaurar(antes);

        // Nada do que funcionava mudou: mesmo degrau, mesmos endereços.
        assert_eq!(
            degrau_de_agora, degrau_de_antes,
            "o ponto de encontro fora do ar mudou o degrau da escada"
        );
        // As máquinas, e não as portas: cada `iniciar` pediu porta zero e o
        // sistema deu uma diferente. O que a comparação cobra é que nenhum
        // **endereço** tenha sumido do link.
        let candidatos = |texto: &str| {
            seele_proto::uri::analisar(texto)
                .expect("o convite não se lê de volta")
                .enderecos()
                .expect("um endereço do convite não se separa")
                .into_iter()
                .map(|alvo| alvo.maquina.to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            candidatos(&convite_de_agora),
            candidatos(&convite_de_antes),
            "o ponto de encontro fora do ar tirou endereços do convite"
        );
        assert!(
            !convite_de_agora.contains("enc="),
            "o convite levou um bilhete para um ponto de encontro que não respondeu: \
             {convite_de_agora}"
        );

        // E a espera a mais é a que foi escolhida, não uma espera de rede.
        let folga = com_encontro.saturating_sub(sem_encontro);
        assert!(
            folga <= crate::alcance::encontro::PRAZO + std::time::Duration::from_millis(750),
            "hospedar demorou {folga:?} a mais por causa de um ponto de encontro fora do ar"
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
