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
    let mut db = Arc::clone(&server.persistence).lock_owned().await;
    let enabled = mods::enabled(&db)?;
    anyhow::ensure!(
        Permissions::new(&db)
            .may(person, Permission::ReadChannel)
            .unwrap_or(false),
        "cannot read"
    );
    if id.is_empty() {
        return Ok(serde_json::json!({"ok":true,"mods":enabled.iter().map(|m| serde_json::json!({"id":m.id,"hash":m.hash,"version":m.version})).collect::<Vec<_>>()} ).to_string());
    }
    let active = enabled
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow::anyhow!("disabled"))?;
    anyhow::ensure!(
        Channels::new(&db)
            .channels()?
            .iter()
            .any(|c| c.id == channel),
        "unknown channel"
    );
    let admin = Permissions::new(&db)
        .may(person, Permission::AdministerServer)
        .unwrap_or(false);
    let write = Permissions::new(&db)
        .may(person, Permission::WriteChannel)
        .unwrap_or(false);
    let raizes = server
        .mods_dir
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no directory"))?;
    let root = raizes.pacote_de(id);
    // **Da instância, e não da máquina.** Ver `RaizesDosMods`: o mesmo MOD em
    // dois servidores lia e escrevia nos mesmos arquivos.
    let dados = raizes.dados_de(id);
    let hash = active.hash.clone();
    let context = serde_json::json!({"person":person.0.to_string(),"channel":channel.0,"admin":admin,"write":write}).to_string();
    let id = id.to_owned();
    let payload = payload.to_owned();
    // The owned guard is held on the blocking worker, never running JS on Tokio.
    tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let manifest =
            seele_proto::mods::read_manifest(&std::fs::read_to_string(root.join("mod.json"))?)?;
        anyhow::ensure!(manifest.id == id, "wrong id");
        let mut files = Vec::new();
        package(&root, &root, &mut files)?;
        anyhow::ensure!(
            seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut files)) == hash,
            "package changed"
        );
        let entry = manifest
            .server
            .ok_or_else(|| anyhow::anyhow!("no server half"))?;
        let relative = seele_proto::mods::inner_path(&entry.split('/').collect::<Vec<_>>())
            .ok_or_else(|| anyhow::anyhow!("bad path"))?;
        let mut host = super::Anfitriao::novo()?;
        host.carregar(&id, &std::fs::read_to_string(root.join(relative))?, &dados)?;
        let mut data: BTreeMap<String, String> = mods::ler_quintal(&db, &id)?;
        let before = data.clone();
        let response = host.pedir(&id, person, &context, &payload, &mut data)?;
        if data != before {
            mods::gravar_quintal(&mut db, &id, &data)?;
        }
        Ok(response)
    })
    .await?
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
