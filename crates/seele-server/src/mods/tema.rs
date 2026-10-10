//! **A escolha de quem administra** (rascunho da API 6, §9; D-api-5; tarefa
//! 17f.0 da F3).
//!
//! Com dois MODs com a porta do tema ligados, quem administra decide qual pinta
//! o servidor — como já decide quem desenha cada parte da tela. O banco guarda
//! o escolhido ([`crate::persistence::tema`]); o que este módulo acrescenta é o
//! que o banco não sabe: se o MOD tem a porta, que é uma linha do manifesto
//! dos bytes que este servidor exige, e avisar as sessões quando o tema em
//! vigor muda.

use crate::persistence::{mods::enabled, tema};
use crate::server::{Event, Server};

/// O desfecho de uma escolha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escolha {
    /// Gravada. As sessões ouviram o `TemaMudou` se o tema em vigor mudou.
    Feita,
    /// Recusada pelo nome, de uma lista fechada, e nada foi gravado:
    ///
    /// - `mod-desligado`: o MOD não está ligado neste servidor;
    /// - `sem-porta-de-tema`: o manifesto dos bytes que o servidor exige não
    ///   declara `"tema": true`.
    Recusada(&'static str),
}

/// Se o pacote com estes bytes declara a porta do tema.
///
/// Lido do manifesto **do hash que o servidor exige**, como
/// [`super::carregar_do_disco`] lê as portas: um manifesto que não abre é uma
/// porta fechada.
#[must_use]
pub fn tem_a_porta(raizes: &crate::RaizesDosMods, hash: &str) -> bool {
    std::fs::read_to_string(raizes.pacote_de(hash).join("mod.json"))
        .ok()
        .and_then(|texto| seele_proto::mods::read_manifest(&texto).ok())
        .is_some_and(|manifesto| manifesto.tema)
}

/// Faz de `mod_id` o MOD que pinta este servidor.
///
/// Só um MOD ligado e com a porta: escolher um desligado tiraria a marca do
/// escolhido de agora e entregaria a porta ao primeiro que declarasse, sem
/// ninguém decidir. O tema que cada um já declarou fica guardado, e é o do
/// novo escolhido que passa a valer — ou nenhum, se ele ainda não declarou.
///
/// # Errors
///
/// Falha se o banco não responder. Uma recusa não é erro: é
/// [`Escolha::Recusada`].
pub async fn escolher(server: &Server, mod_id: &str) -> anyhow::Result<Escolha> {
    let banco = server.persistence.lock().await;
    let Some(ligado) = enabled(&banco)?
        .into_iter()
        .find(|ligado| ligado.id == mod_id)
    else {
        return Ok(Escolha::Recusada("mod-desligado"));
    };
    let com_a_porta = server
        .mods_dir
        .as_ref()
        .is_some_and(|raizes| tem_a_porta(raizes, &ligado.hash));
    if !com_a_porta {
        return Ok(Escolha::Recusada("sem-porta-de-tema"));
    }
    let antes = tema::em_vigor(&banco)?;
    let agora = tema::escolher(&banco, mod_id)?;
    // **A cada troca**, como em `pedidos::publicar_no_barramento`: escolher quem
    // já pinta, ou trocar entre dois que ainda não declararam, não muda nada
    // na tela de ninguém.
    if agora != antes {
        let _ = server.events.send(Event::TemaMudou { tema: agora });
    }
    Ok(Escolha::Feita)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::persistence::mods::{disable, enable, EnabledMod};
    use crate::persistence::Location;
    use crate::RaizesDosMods;
    use seele_proto::control::TemaDoServidor;
    use std::sync::Arc;
    use tokio::sync::broadcast::error::TryRecvError;

    const CLARO: &str = r##"{"acento":"#c86b1f"}"##;

    /// Uma pasta só deste teste, apagada antes de usar.
    fn pasta(nome: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static QUAL: AtomicUsize = AtomicUsize::new(0);
        let n = QUAL.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seele-escolha-do-tema-{nome}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    /// Um servidor em memória com a pasta dos MODs, sem ninguém atendendo.
    async fn servidor(nome: &str) -> (Arc<crate::Daemon>, RaizesDosMods) {
        let raiz = pasta(nome);
        let raizes = RaizesDosMods {
            pacotes: raiz.clone(),
            dados: raiz.join("mod-data"),
        };
        let config = crate::ServerConfig {
            name: "Casa".into(),
            listen: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            mods_dir: Some(raizes.clone()),
            ..crate::ServerConfig::default()
        };
        let daemon = crate::Daemon::bind(config).await.expect("servidor");
        (Arc::new(daemon), raizes)
    }

    /// Liga `id` com os bytes de `hash_de`, e o manifesto deles com a porta ou
    /// sem ela. Na API em vigor, que é o que o `read_manifest` aceita hoje: a 6
    /// só entra no conjunto com o `v6.json` (tarefa 12).
    async fn ligar(
        daemon: &crate::Daemon,
        raizes: &RaizesDosMods,
        id: &str,
        hash_de: u8,
        porta: bool,
    ) {
        let hash = format!("{hash_de:02x}").repeat(32);
        let dir = raizes.pacote_de(&hash);
        std::fs::create_dir_all(&dir).expect("pasta do pacote");
        std::fs::write(
            dir.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":{api},"repo":"https://github.com/{id}","reach":[],"server":"servidor/main.js","tema":{porta}}}"#,
                api = seele_proto::mods::MOD_API_VERSION
            ),
        )
        .expect("manifesto");
        let banco = daemon.server().persistence.lock().await;
        enable(
            &banco,
            &EnabledMod {
                id: id.to_owned(),
                version: "1.0.0".to_owned(),
                hash,
                repo: format!("https://github.com/{id}"),
                reach: Vec::new(),
                server_half: true,
                api: seele_proto::mods::MOD_API_VERSION,
            },
        )
        .expect("habilitar");
    }

    async fn quem_pinta(daemon: &crate::Daemon) -> Option<String> {
        let banco = daemon.server().persistence.lock().await;
        tema::escolhido_ligado(&banco).expect("ler")
    }

    /// **A troca de quem administra chega a quem está dentro.** Com dois MODs
    /// de tema ligados, escolher o outro tira o tema do primeiro das sessões
    /// (o outro ainda não declarou), e voltar ao primeiro devolve o dele — o
    /// guardado, sem ele declarar de novo. Escolher quem já pinta não é
    /// notícia para ninguém.
    #[tokio::test(flavor = "multi_thread")]
    async fn quem_administra_troca_quem_pinta_e_as_sessoes_ouvem() {
        let (daemon, raizes) = servidor("troca").await;
        ligar(&daemon, &raizes, "seele/estilo", 0xa1, true).await;
        ligar(&daemon, &raizes, "seele/noite", 0xa2, true).await;
        {
            let banco = daemon.server().persistence.lock().await;
            tema::declarar(&banco, "seele/estilo", Some(CLARO)).expect("declarar");
        }
        let mut ouvido = daemon.server().events.subscribe();

        let escolha = escolher(daemon.server(), "seele/noite")
            .await
            .expect("escolher");
        assert_eq!(escolha, Escolha::Feita);
        assert_eq!(quem_pinta(&daemon).await.as_deref(), Some("seele/noite"));
        match ouvido.try_recv() {
            Ok(Event::TemaMudou { tema }) => assert_eq!(
                tema, None,
                "o novo escolhido não declarou: as sessões têm de ficar sem tema"
            ),
            outro => panic!("a troca tinha de mandar o TemaMudou, e chegou {outro:?}"),
        }

        escolher(daemon.server(), "seele/estilo")
            .await
            .expect("escolher");
        match ouvido.try_recv() {
            Ok(Event::TemaMudou { tema }) => assert_eq!(
                tema,
                Some(TemaDoServidor {
                    mod_id: "seele/estilo".to_owned(),
                    valores: CLARO.to_owned(),
                }),
                "voltar ao primeiro tinha de devolver o tema guardado dele"
            ),
            outro => panic!("a volta tinha de mandar o TemaMudou, e chegou {outro:?}"),
        }

        escolher(daemon.server(), "seele/estilo")
            .await
            .expect("escolher");
        assert!(
            matches!(ouvido.try_recv(), Err(TryRecvError::Empty)),
            "escolher quem já pinta não muda o tema, e não é notícia"
        );
    }

    /// **Só um MOD ligado e com a porta.** Um MOD sem `"tema": true` no
    /// manifesto dos bytes exigidos é recusado pelo nome, e um MOD de tema que
    /// foi desligado também — a linha dele continua no banco, e escolhê-lo
    /// tiraria a marca de quem pinta. Quem pinta continua pintando, sem aviso a
    /// ninguém.
    #[tokio::test(flavor = "multi_thread")]
    async fn um_mod_sem_a_porta_ou_desligado_nao_e_escolhido() {
        let (daemon, raizes) = servidor("recusa").await;
        ligar(&daemon, &raizes, "seele/estilo", 0xb1, true).await;
        ligar(&daemon, &raizes, "seele/perfis", 0xb2, false).await;
        ligar(&daemon, &raizes, "seele/noite", 0xb3, true).await;
        {
            let banco = daemon.server().persistence.lock().await;
            disable(&banco, "seele/noite").expect("desligar");
            tema::declarar(&banco, "seele/estilo", Some(CLARO)).expect("declarar");
        }
        let mut ouvido = daemon.server().events.subscribe();

        assert_eq!(
            escolher(daemon.server(), "seele/perfis")
                .await
                .expect("escolher"),
            Escolha::Recusada("sem-porta-de-tema")
        );
        assert_eq!(
            escolher(daemon.server(), "seele/noite")
                .await
                .expect("escolher"),
            Escolha::Recusada("mod-desligado")
        );
        assert_eq!(quem_pinta(&daemon).await.as_deref(), Some("seele/estilo"));
        assert!(
            matches!(ouvido.try_recv(), Err(TryRecvError::Empty)),
            "uma escolha recusada não muda o tema de ninguém"
        );
    }
}
