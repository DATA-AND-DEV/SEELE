//! API v2: private request/reply over the authenticated control connection.
//! Replies never use the shared chat/event bus. A fresh bounded runtime handles
//! each request; the database lock serializes read/modify/commit across people.

use crate::{
    permissions::Permissions,
    persistence::{channels::Channels, mods},
    server::Server,
};
use seele_proto::{
    control::Permission,
    ids::{ChannelId, PersonId},
};
use std::{collections::BTreeMap, path::Path, sync::Arc};

fn package(root: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if dir == root && entry.file_name() == "dados" {
            continue;
        }
        anyhow::ensure!(!entry.file_type()?.is_symlink(), "symlink");
        let path = entry.path();
        if path.is_dir() {
            package(root, &path, files)?;
        } else {
            files.push((
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                std::fs::read(&path)?,
            ));
        }
    }
    Ok(())
}

/// Executes a request with identity supplied by the authenticated session.
/// Failure is a stable identifier; the client supplies the wording.
pub async fn executar(
    server: &Arc<Server>,
    person: PersonId,
    channel: ChannelId,
    id: &str,
    payload: &str,
) -> String {
    match executar_inner(server, person, channel, id, payload).await {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(mod_id = id, %error, "MOD request refused");
            r#"{"ok":false,"error":"bridge-refused"}"#.into()
        }
    }
}

async fn executar_inner(
    server: &Arc<Server>,
    person: PersonId,
    channel: ChannelId,
    id: &str,
    payload: &str,
) -> anyhow::Result<String> {
    // **O banco é lido e soltado.** Não levado ao trabalhador.
    //
    // R11 da revisão da v15: o guarda dono era tomado aqui e mantido enquanto o
    // trabalhador relia os arquivos do pacote, recalculava o hash, criava o
    // runtime e executava o pedido — inclusive as chamadas nativas que o MOD faz,
    // que têm prazo próprio de dois segundos. Ler histórico e escrever texto
    // esperavam por tudo isso.
    //
    // O que o guarda longo dava de graça — a atomicidade do quintal do MOD —
    // volta pelo cadeado por MOD, mais abaixo. Ver `Server::mods_em_curso`.
    let (enabled, pode_ler) = {
        let db = server.persistence.lock().await;
        let enabled = mods::enabled(&db)?;
        let pode_ler = Permissions::new(&db)
            .may(person, Permission::ReadChannel)
            .unwrap_or(false);
        (enabled, pode_ler)
    };
    anyhow::ensure!(pode_ler, "cannot read");
    if id.is_empty() {
        // **A identidade do conjunto vai junto.** Sem ela, a janela vê quais
        // MODs faltam e não tem como saber se o que está anunciado agora é o
        // mesmo a que esta pessoa disse sim — e buscar o que falta sem essa
        // resposta seria ampliar um consentimento antigo para uma lista nova.
        //
        // Campo de JSON, e não variante nova: o corpo desta resposta é do
        // produto, e um campo a mais nele não toca na janela de
        // compatibilidade do protocolo.
        let identidade = {
            let db = server.persistence.lock().await;
            super::anuncio::conjunto_exigido(&db)
                .map(|c| c.identidade)
                .unwrap_or_default()
        };
        return Ok(serde_json::json!({
            "ok": true,
            "conjunto": identidade,
            "mods": enabled.iter().map(|m| serde_json::json!({"id":m.id,"hash":m.hash,"version":m.version})).collect::<Vec<_>>(),
        })
        .to_string());
    }
    let active = enabled
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow::anyhow!("disabled"))?;
    // **Zero quer dizer «nenhum canal», e é o pedido de escopo de servidor.**
    //
    // O plano de 18/09 escreve o defeito: «para MOD de escopo servidor, leitura
    // não deveria exigir que exista um canal de texto selecionado; hoje a
    // interface comum e a ponte associam pedidos a canal». Um MOD que lê a
    // configuração do servidor — tema, perfis, ficha — não é sobre canal
    // nenhum, e falhava com `unknown channel` quando a janela ainda não tinha
    // um aberto: um motivo que não tem nada a ver com ele.
    //
    // Zero, e não um `Option` no fio: `ChannelId` é a chave primária do SQLite,
    // que começa em 1, então zero nunca é um canal de verdade. Um campo novo na
    // variante quebraria a janela de compatibilidade do protocolo sem
    // necessidade — o mesmo byte já diz a coisa nova.
    let com_canal = channel.0 != 0;
    // As três perguntas restantes ao banco, numa tomada curta e só de leitura.
    let (canal_existe, admin, write) = {
        let db = server.persistence.lock().await;
        let canal_existe = !com_canal
            || Channels::new(&db)
                .channels()?
                .iter()
                .any(|c| c.id == channel);
        let permissions = Permissions::new(&db);
        (
            canal_existe,
            permissions
                .may(person, Permission::AdministerServer)
                .unwrap_or(false),
            permissions
                .may(person, Permission::WriteChannel)
                .unwrap_or(false),
        )
    };
    anyhow::ensure!(canal_existe, "unknown channel");
    let raizes = server
        .mods_dir
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no directory"))?;
    let root = raizes.pacote_de(&active.hash);
    // **Da instância, e não da máquina.** Ver `RaizesDosMods`: o mesmo MOD em
    // dois servidores lia e escrevia nos mesmos arquivos.
    let dados = raizes.dados_de(id);
    let hash = active.hash.clone();
    // `channel` é **nulo** quando não há canal, e não zero: um MOD que
    // comparasse com zero estaria lendo um identificador que não existe, e
    // `null` é a única forma de dizer «esta pergunta não é sobre um canal».
    let contexto_do_canal = if com_canal {
        serde_json::Value::from(channel.0)
    } else {
        serde_json::Value::Null
    };
    let context = serde_json::json!({"person":person.0.to_string(),"channel":contexto_do_canal,"admin":admin,"write":write}).to_string();
    let id = id.to_owned();
    let payload = payload.to_owned();
    // A lista de esperas é do servidor, e não deste pedido: ela tem de
    // sobreviver à resposta para o fluxo de volume encontrar o token.
    let esperas = Arc::clone(&server.esperas);

    // **O cadeado deste MOD**, e é ele que substitui a atomicidade que o guarda
    // longo dava. Ver `Server::mods_em_curso`: dois pedidos do mesmo MOD esperam
    // um pelo outro — porque o quintal é lido, mexido pelo JavaScript e gravado
    // de volta, e sobrepor isso perde atualização —, e dois MODs diferentes
    // correm juntos.
    let cadeado = {
        let mut em_curso = server
            .mods_em_curso
            .lock()
            .map_err(|_| anyhow::anyhow!("a tabela de cadeados de MOD foi envenenada"))?;
        Arc::clone(
            em_curso
                .entry(id.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    };
    let _vez = cadeado.lock().await;

    // **O pacote é conferido uma vez por hash**, e não a cada pedido: a
    // conferência relê todos os arquivos e recalcula o hash do conteúdo, e a
    // resposta é sempre a mesma enquanto o hash for o mesmo. Ver
    // `Server::pacotes_conferidos`.
    let conferidos = Arc::clone(&server.pacotes_conferidos);
    let fonte = {
        let guardado = conferidos
            .lock()
            .map_err(|_| anyhow::anyhow!("o cache de pacotes de MOD foi envenenado"))?
            .get(&hash)
            .cloned();
        match guardado {
            Some(pacote) => pacote.fonte,
            None => {
                let hash_para_conferir = hash.clone();
                let id_do_pacote = id.clone();
                let raiz = root.clone();
                // Fora do Tokio: são leituras de disco e um SHA-256 sobre o
                // pacote inteiro.
                let fonte = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
                    let manifest = seele_proto::mods::read_manifest(&std::fs::read_to_string(
                        raiz.join("mod.json"),
                    )?)?;
                    anyhow::ensure!(manifest.id == id_do_pacote, "wrong id");
                    let mut files = Vec::new();
                    package(&raiz, &raiz, &mut files)?;
                    anyhow::ensure!(
                        seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut files))
                            == hash_para_conferir,
                        "package changed"
                    );
                    let entry = manifest
                        .server
                        .ok_or_else(|| anyhow::anyhow!("no server half"))?;
                    let relative =
                        seele_proto::mods::inner_path(&entry.split('/').collect::<Vec<_>>())
                            .ok_or_else(|| anyhow::anyhow!("bad path"))?;
                    Ok(std::fs::read_to_string(raiz.join(relative))?)
                })
                .await??;
                conferidos
                    .lock()
                    .map_err(|_| anyhow::anyhow!("o cache de pacotes de MOD foi envenenado"))?
                    .insert(
                        hash.clone(),
                        crate::server::PacoteConferido {
                            fonte: fonte.clone(),
                        },
                    );
                fonte
            }
        }
    };

    // O quintal é lido numa tomada curta, **antes** de o JavaScript rodar.
    let mut data: BTreeMap<String, String> = {
        let db = server.persistence.lock().await;
        mods::ler_quintal(&db, &id)?
    };
    let before = data.clone();
    let id_do_pedido = id.clone();
    // O JavaScript nunca roda no Tokio, e agora também não roda com o banco na
    // mão.
    let (response, data) = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(String, BTreeMap<String, String>)> {
            let mut host = super::Anfitriao::novo()?;
            // **Antes de `carregar`.** As ligações do QuickJS clonam este `Arc` ao
            // serem montadas; compartilhar depois deixaria o MOD escrevendo numa
            // lista que ninguém mais lê. Ver `Anfitriao::compartilhar_esperas`.
            host.compartilhar_esperas(esperas);
            host.carregar(&id_do_pedido, &fonte, &dados)?;
            let response = host.pedir(&id_do_pedido, person, &context, &payload, &mut data)?;
            Ok((response, data))
        },
    )
    .await??;

    // E gravado noutra tomada curta. O cadeado deste MOD ainda está na mão, e é
    // ele que garante que ninguém leu o quintal entre a leitura acima e esta
    // gravação — que é a perda de atualização que soltar o mutex introduziria se
    // ele não existisse.
    if data != before {
        let mut db = server.persistence.lock().await;
        mods::gravar_quintal(&mut db, &id, &data)?;
    }
    Ok(response)
}

/// Splits only at character boundaries, keeping each wire frame under its ceiling.
#[must_use]
pub fn partes(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = rest.len().min(10 * 1024);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        result.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn replies_preserve_unicode_and_frame_limits() {
        let original = "á🧙魔".repeat(4096);
        let chunks = super::partes(&original);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|part| part.len() <= 10 * 1024));
        assert_eq!(chunks.concat(), original);
        assert_eq!(super::partes(""), vec![String::new()]);
    }
}
