//! **As escolhas de MOD de quem hospeda, ESTAGIADAS até o CTA** (P1 da 0.17,
//! medido pelo Codex no app nativo, perfil com a pasta da 0.15.1).
//!
//! # O defeito
//!
//! No ANTES DE HOSPEDAR, «ATUALIZAR PARA 4.0.0» chamava o resolvedor, que abria
//! o banco com `Persistence::open`, e abrir **migra**: o banco passava do
//! esquema 13 ao 18 e o ESTILO 4.0.0 ficava ligado **antes** do «Criar cópia,
//! converter e hospedar», sem cópia nenhuma em `copias-de-seguranca`, com a
//! tela dizendo «Nada foi mudado».
//!
//! # A regra
//!
//! - Uma saída de quem hospeda (ATUALIZAR, DESLIGAR) **não toca no banco**: ela
//!   é conferida (o servidor existe, o MOD é mesmo o que o hospedar recusaria, os
//!   bytes novos servem) **só lendo**, e guardada aqui, num arquivo do app, por
//!   servidor. O banco fica com os mesmos bytes.
//! - O `hospedar` (o CTA) faz, nesta ordem: a cópia nova e verificada
//!   (`antes_de_hospedar::garantir_a_copia`), a conversão (a subida do servidor
//!   abre e migra) e, **antes de aceitar a primeira conexão**, aplica as
//!   escolhas ([`aplicar`]) e grava o sim do dono ao conjunto que resulta (b1
//!   do E7): a escolha foi dele, nesta máquina, e a entrada dele no próprio
//!   servidor não pergunta de novo.
//! - Cada escolha é **de um servidor só**: a pessoa escolhe o alvo, e nenhuma
//!   outra base é lida para escrita, convertida ou alterada.
//! - Desistir, voltar e reabrir não escondem nada: o banco não mudou, então o
//!   ANTES DE HOSPEDAR continua dizendo que vai converter, e as escolhas
//!   pendentes aparecem ao lado ([`pendentes`]).

use std::path::{Path, PathBuf};

use crate::mods::SaidaDoModAntigo;

/// A pasta das escolhas pendentes, dentro da pasta de dados do SEELE.
pub(crate) const PASTA_DAS_ESCOLHAS: &str = "escolhas-de-mods";

/// Uma escolha de quem hospeda para um MOD, ainda não aplicada.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Escolha {
    /// O MOD.
    pub(crate) id: String,
    /// O que fazer com ele ao hospedar.
    pub(crate) saida: SaidaDoModAntigo,
}

/// O nome do arquivo do banco de sempre (o servidor `None`). Os guardados são
/// `servidor-<id>.json`: um prefixo que nenhum id pode imitar.
const DE_SEMPRE: &str = "de-sempre.json";

/// Se o id de um servidor guardado serve de componente de caminho: só ASCII
/// alfanumérico, `-` e `_`, de 1 a 64 caracteres. Nada de separador, `.`,
/// `..`, caminho absoluto nem byte de controle (revisão do Codex: o id vem da
/// janela, e era concatenado cru no caminho de escrita e de remoção).
fn id_serve(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// **O arquivo das escolhas de um servidor**, conferido: o componente do id
/// passa por [`id_serve`], e o caminho final tem de ser filho **direto** da
/// pasta das escolhas. Quem chama já conferiu que o servidor é um guardado
/// desta máquina (`mods::servidor_guardado`); esta é a segunda tranca, a do
/// caminho.
///
/// # Errors
///
/// `servidor-invalido`, sem tocar no disco.
fn arquivo(config: &Path, servidor: Option<&str>) -> Result<PathBuf, String> {
    let nome = match servidor {
        None => DE_SEMPRE.to_owned(),
        Some(id) if id_serve(id) => format!("servidor-{id}.json"),
        Some(_) => return Err("servidor-invalido".to_owned()),
    };
    let pasta = config.join(PASTA_DAS_ESCOLHAS);
    let caminho = pasta.join(&nome);
    if caminho.parent() != Some(pasta.as_path())
        || caminho.file_name().and_then(|nome| nome.to_str()) != Some(nome.as_str())
    {
        return Err("servidor-invalido".to_owned());
    }
    Ok(caminho)
}

/// As escolhas pendentes deste servidor. Um arquivo que não se lê, ou um
/// servidor que não serve de nome, é «nenhuma»: o hospedar então recusa o MOD
/// de API antiga de novo, que é o lado seguro.
pub(crate) fn pendentes(config: &Path, servidor: Option<&str>) -> Vec<Escolha> {
    let caminho = match arquivo(config, servidor) {
        Ok(caminho) => caminho,
        Err(motivo) => {
            tracing::warn!(%motivo, "as escolhas de MOD de um servidor que não serve de nome não se leem");
            return Vec::new();
        }
    };
    match std::fs::read(&caminho) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|erro| {
            tracing::warn!(%erro, "as escolhas de MOD pendentes não se leem; nenhuma vale");
            Vec::new()
        }),
        Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(erro) => {
            tracing::warn!(%erro, "as escolhas de MOD pendentes não se leem; nenhuma vale");
            Vec::new()
        }
    }
}

/// Guarda (ou troca) a escolha para este MOD deste servidor. Escrita num
/// arquivo provisório e renomeada: uma queda no meio não deixa meio arquivo.
///
/// # Errors
///
/// `servidor-invalido`, ou o motivo do disco.
pub(crate) fn estagiar(
    config: &Path,
    servidor: Option<&str>,
    escolha: Escolha,
) -> Result<(), String> {
    let caminho = arquivo(config, servidor)?;
    let mut todas = pendentes(config, servidor);
    todas.retain(|outra| outra.id != escolha.id);
    todas.push(escolha);
    gravar(&caminho, &todas)
}

fn gravar(caminho: &Path, todas: &[Escolha]) -> Result<(), String> {
    if let Some(pasta) = caminho.parent() {
        std::fs::create_dir_all(pasta).map_err(|erro| erro.to_string())?;
    }
    let provisorio = caminho.with_extension("json.provisorio");
    let texto = serde_json::to_vec_pretty(todas).map_err(|erro| erro.to_string())?;
    std::fs::write(&provisorio, texto).map_err(|erro| erro.to_string())?;
    std::fs::rename(&provisorio, caminho).map_err(|erro| erro.to_string())
}

/// **Desfaz a escolha de um MOD** deste servidor (o DESFAZER do ANTES DE
/// HOSPEDAR). O banco nunca foi tocado: desfazer é só esquecer. Sem escolha
/// para ele, nada muda.
///
/// # Errors
///
/// `servidor-invalido`, ou o motivo do disco. **Uma remoção que falha é
/// erro** (revisão do Codex): a tela não pode dizer «retirada» de uma escolha
/// que reaparece na próxima leitura.
pub(crate) fn retirar(config: &Path, servidor: Option<&str>, id: &str) -> Result<(), String> {
    let caminho = arquivo(config, servidor)?;
    let mut todas = pendentes(config, servidor);
    let antes = todas.len();
    todas.retain(|escolha| escolha.id != id);
    if todas.len() == antes {
        return Ok(());
    }
    if todas.is_empty() {
        return remover(&caminho);
    }
    gravar(&caminho, &todas)
}

fn remover(caminho: &Path) -> Result<(), String> {
    match std::fs::remove_file(caminho) {
        Ok(()) => Ok(()),
        Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(erro) => Err(erro.to_string()),
    }
}

/// Esquece as escolhas pendentes deste servidor (depois de aplicadas).
///
/// # Errors
///
/// `servidor-invalido`, ou o motivo do disco.
pub(crate) fn descartar(config: &Path, servidor: Option<&str>) -> Result<(), String> {
    remover(&arquivo(config, servidor)?)
}

/// **Aplica as escolhas no banco JÁ convertido**, de dentro do
/// `antes_de_aceitar` do `hospedar` (depois da cópia e da migração, antes da
/// primeira conexão). Devolve a identidade do conjunto resultante, para o sim
/// do dono.
///
/// Os bytes da versão nova são conferidos de novo aqui: entre escolher e
/// hospedar, o pacote pode ter saído do cache.
///
/// # Errors
///
/// O primeiro MOD que não se aplicou, e por quê. Nesse caso nada sobe
/// (`antes_de_aceitar` falha), e as escolhas continuam pendentes.
pub(crate) fn aplicar(
    persistence: &seele_server::persistence::Persistence,
    config: &str,
    escolhas: &[Escolha],
) -> Result<String, String> {
    for escolha in escolhas {
        let gravado = match &escolha.saida {
            SaidaDoModAntigo::Atualizar { hash } => {
                let linha = crate::mods::linha_do_pacote(config, &escolha.id, hash)
                    .map_err(|recusa| format!("{}: {recusa}", escolha.id))?;
                seele_server::persistence::mods::enable(persistence, &linha)
            }
            SaidaDoModAntigo::Desligar => {
                seele_server::persistence::mods::disable(persistence, &escolha.id)
            }
        };
        gravado.map_err(|erro| format!("{}: {erro:#}", escolha.id))?;
        tracing::info!(mod_id = %escolha.id, saida = ?escolha.saida, "a escolha de MOD estagiada foi aplicada ao hospedar");
    }
    seele_server::mods::anuncio::conjunto_exigido(persistence)
        .map(|conjunto| conjunto.identidade)
        .map_err(|erro| format!("{erro:?}"))
}

/// **O passo do CTA depois do portão da cópia**: com a cópia nova e verificada
/// já feita (`antes_de_hospedar::garantir_a_copia`), abre o banco (é aqui que
/// ele converte), aplica as escolhas pendentes deste servidor ([`aplicar`]),
/// grava o sim do dono ao conjunto resultante sob `aqui` (o endereço por onde
/// a janela entra no próprio servidor) e esquece as escolhas, já aplicadas.
///
/// Antes da subida, e não dentro dela: a subida recusa um banco com MOD de API
/// antiga ligado antes de qualquer gancho, então a escolha tem de estar no
/// banco quando o servidor o abrir.
///
/// Sem escolha pendente, não abre nada (a subida converte, como sempre).
/// Devolve se aplicou alguma.
///
/// # Errors
///
/// O banco não abriu, ou uma escolha não se aplicou. As escolhas continuam
/// pendentes, e o `hospedar` não sobe.
pub(crate) fn aplicar_depois_do_portao(
    config: &str,
    servidor: Option<&str>,
    banco: &Path,
    aqui: &str,
) -> Result<bool, String> {
    let escolhas = pendentes(Path::new(config), servidor);
    if escolhas.is_empty() {
        return Ok(false);
    }
    let aberto = seele_server::persistence::Persistence::open(
        &seele_server::persistence::Location::File(banco.to_path_buf()),
    )
    .map_err(|erro| format!("{erro:#}"))?;
    let identidade = aplicar(&aberto, config, &escolhas)?;
    drop(aberto);
    // Já aplicadas: uma escolha que não sai do disco seria aplicada de novo no
    // próximo hospedar, o que é idempotente (`enable`/`disable`). Não impede
    // de hospedar; o `seele.log` diz.
    if let Err(motivo) = descartar(Path::new(config), servidor) {
        tracing::warn!(%motivo, "as escolhas de MOD já aplicadas não saíram do disco");
    }
    // O sim que não grava não impede de hospedar: a entrada do dono pergunta,
    // e o `seele.log` diz por quê.
    match seele_ffi::mods::aceitar(config, aqui, &identidade) {
        Ok(()) => tracing::info!("o sim do dono ao conjunto que ele escolheu foi gravado no CTA"),
        Err(motivo) => {
            tracing::warn!(%motivo, "o sim do dono ao conjunto que ele escolheu não gravou; a entrada dele vai perguntar")
        }
    }
    Ok(true)
}

#[cfg(test)]
mod testes {
    use super::{descartar, estagiar, pendentes, Escolha};
    use crate::mods::SaidaDoModAntigo;

    #[test]
    fn estagiar_troca_a_escolha_do_mesmo_mod_e_e_de_um_servidor_so() {
        let casa = tempfile::tempdir().expect("a casa");
        let config = casa.path();
        estagiar(
            config,
            Some("a"),
            Escolha {
                id: "x/y".into(),
                saida: SaidaDoModAntigo::Desligar,
            },
        )
        .expect("estagiar");
        estagiar(
            config,
            Some("a"),
            Escolha {
                id: "x/y".into(),
                saida: SaidaDoModAntigo::Atualizar { hash: "h".into() },
            },
        )
        .expect("estagiar de novo");
        assert_eq!(
            pendentes(config, Some("a")),
            vec![Escolha {
                id: "x/y".into(),
                saida: SaidaDoModAntigo::Atualizar { hash: "h".into() },
            }]
        );
        assert!(
            pendentes(config, Some("b")).is_empty(),
            "a escolha de A apareceu em B"
        );
        assert!(
            pendentes(config, None).is_empty(),
            "a escolha de A apareceu no de sempre"
        );
        descartar(config, Some("a")).expect("descartar");
        assert!(pendentes(config, Some("a")).is_empty());
    }

    #[test]
    fn retirar_desfaz_so_a_escolha_daquele_mod_daquele_servidor() {
        let casa = tempfile::tempdir().expect("a casa");
        let config = casa.path();
        for (servidor, id) in [
            (Some("a"), "x/um"),
            (Some("a"), "x/dois"),
            (Some("b"), "x/um"),
        ] {
            estagiar(
                config,
                servidor,
                Escolha {
                    id: id.into(),
                    saida: SaidaDoModAntigo::Desligar,
                },
            )
            .expect("estagiar");
        }
        super::retirar(config, Some("a"), "x/um").expect("retirar");
        let ids = |servidor| {
            pendentes(config, servidor)
                .into_iter()
                .map(|escolha| escolha.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(Some("a")), vec!["x/dois".to_owned()]);
        assert_eq!(
            ids(Some("b")),
            vec!["x/um".to_owned()],
            "desfazer em A mexeu em B"
        );
        super::retirar(config, Some("a"), "x/dois").expect("retirar a última");
        assert!(ids(Some("a")).is_empty());
        super::retirar(config, Some("a"), "x/nenhum").expect("retirar o que não há");
    }

    #[test]
    fn um_arquivo_torto_e_nenhuma_escolha() {
        let casa = tempfile::tempdir().expect("a casa");
        let pasta = casa.path().join(super::PASTA_DAS_ESCOLHAS);
        std::fs::create_dir_all(&pasta).expect("a pasta");
        std::fs::write(pasta.join(super::DE_SEMPRE), b"{torto").expect("escrever");
        assert!(pendentes(casa.path(), None).is_empty());
    }

    /// O SHA-256 e a existência de um arquivo, para «não foi tocado».
    fn marca(caminho: &std::path::Path) -> Option<String> {
        use sha2::Digest as _;
        std::fs::read(caminho).ok().map(|bytes| {
            sha2::Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        })
    }

    /// **Um id de servidor nunca vira caminho fora da pasta** (revisão do
    /// Codex, path traversal): `../sentinela`, um caminho absoluto, um
    /// separador e um `..` são recusados nos quatro atos (estagiar, retirar,
    /// descartar e ler), e o arquivo sentinela fora da pasta fica com a mesma
    /// existência e os mesmos bytes.
    #[test]
    fn um_id_com_caminho_nao_toca_nada_fora_da_pasta() {
        let casa = tempfile::tempdir().expect("a casa");
        let config = casa.path();
        let sentinela = config.join("sentinela.json");
        std::fs::write(&sentinela, b"[]").expect("a sentinela");
        let absoluto = config.join("absoluto");
        let absoluto_txt = absoluto.display().to_string();
        let antes = (marca(&sentinela), marca(&absoluto.with_extension("json")));
        for torto in [
            "../sentinela",
            "../../sentinela",
            absoluto_txt.as_str(),
            "a/b",
            "a\\b",
            "..",
            ".",
            "",
            "servidor-x/../../sentinela",
        ] {
            let escolha = Escolha {
                id: "x/y".into(),
                saida: SaidaDoModAntigo::Desligar,
            };
            assert_eq!(
                estagiar(config, Some(torto), escolha),
                Err("servidor-invalido".to_owned()),
                "estagiar aceitou {torto:?}"
            );
            assert_eq!(
                super::retirar(config, Some(torto), "x/y"),
                Err("servidor-invalido".to_owned()),
                "retirar aceitou {torto:?}"
            );
            assert_eq!(
                descartar(config, Some(torto)),
                Err("servidor-invalido".to_owned()),
                "descartar aceitou {torto:?}"
            );
            assert!(pendentes(config, Some(torto)).is_empty());
        }
        assert_eq!(
            (marca(&sentinela), marca(&absoluto.with_extension("json"))),
            antes,
            "um id com caminho tocou um arquivo fora da pasta das escolhas"
        );
        assert!(
            !config.join(super::PASTA_DAS_ESCOLHAS).exists(),
            "um id recusado ainda criou a pasta das escolhas"
        );
    }

    /// **A remoção que falha chega como erro** (revisão do Codex): sem isso a
    /// tela diria «retirada» de uma escolha que reaparece. Aqui a remoção
    /// falha porque o «arquivo» é uma pasta com conteúdo.
    #[test]
    fn a_remocao_que_falha_e_erro_e_a_escolha_continua() {
        let casa = tempfile::tempdir().expect("a casa");
        let config = casa.path();
        estagiar(
            config,
            Some("a"),
            Escolha {
                id: "x/y".into(),
                saida: SaidaDoModAntigo::Desligar,
            },
        )
        .expect("estagiar");
        let caminho = super::arquivo(config, Some("a")).expect("o caminho de A");
        let guardado = std::fs::read(&caminho).expect("as escolhas de A");
        std::fs::remove_file(&caminho).expect("tirar o arquivo");
        std::fs::create_dir_all(caminho.join("dentro")).expect("uma pasta no lugar");
        assert!(
            descartar(config, Some("a")).is_err(),
            "a remoção que falhou foi dita como feita"
        );
        // E pelo `retirar` das duas formas. A última escolha, com a pasta só
        // para leitura: a remoção falha, e é erro.
        std::fs::remove_dir_all(&caminho).expect("tirar a pasta");
        std::fs::write(&caminho, &guardado).expect("as escolhas de volta");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let pasta = caminho.parent().expect("a pasta").to_path_buf();
            std::fs::set_permissions(&pasta, std::fs::Permissions::from_mode(0o555))
                .expect("a pasta só para leitura");
            let retirada = super::retirar(config, Some("a"), "x/y");
            std::fs::set_permissions(&pasta, std::fs::Permissions::from_mode(0o755))
                .expect("a pasta de volta");
            assert!(
                retirada.is_err(),
                "retirar a última escolha com a remoção impossível foi dito como feito"
            );
            assert_eq!(
                pendentes(config, Some("a")).len(),
                1,
                "a escolha sumiu apesar do erro"
            );
        }
        // Uma de duas: a pasta no lugar do provisório faz a regravação falhar.
        estagiar(
            config,
            Some("a"),
            Escolha {
                id: "x/z".into(),
                saida: SaidaDoModAntigo::Desligar,
            },
        )
        .expect("a segunda escolha");
        std::fs::create_dir_all(caminho.with_extension("json.provisorio").join("dentro"))
            .expect("bloquear o provisório");
        assert!(
            super::retirar(config, Some("a"), "x/y").is_err(),
            "retirar com a regravação impossível foi dito como feito"
        );
        assert_eq!(
            pendentes(config, Some("a")).len(),
            2,
            "uma escolha sumiu apesar do erro"
        );
    }
}
