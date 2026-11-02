//! Arquivos locais completos, sincronizados e recuperáveis após interrupção.
//!
//! O formato principal continua legível pelas versões anteriores. A cópia
//! `.recuperacao` recebe a nova versão completa antes da troca do principal.
//! Uma interrupção deixa a versão antiga ou a nova disponível. Nunca apagamos
//! o destino para renomear (inclusive no Windows), nem substituímos a única
//! cópia corrompida por valores padrão.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

static AVISOS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Avisos persistentes desta execução, sem conteúdo dos arquivos.
#[must_use]
pub fn avisos() -> Vec<String> {
    AVISOS.lock().map(|v| v.clone()).unwrap_or_default()
}
fn avisar(path: &Path, recuperado: bool) {
    let frase = if recuperado {
        format!(
            "Recuperei os dados locais de {} pela cópia de recuperação.",
            path.display()
        )
    } else {
        format!("Não consegui ler os dados locais de {}. Preservei os arquivos; não salve por cima deles. É preciso recuperar uma cópia válida.", path.display())
    };
    tracing::warn!("{frase}");
    if let Ok(mut avisos) = AVISOS.lock() {
        if !avisos.contains(&frase) {
            avisos.push(frase);
        }
    }
}
fn ao_lado(path: &Path, sufixo: &str) -> PathBuf {
    let mut nome = path.file_name().unwrap_or_default().to_os_string();
    nome.push(format!(".{sufixo}"));
    path.with_file_name(nome)
}
fn ler_opcional(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
/// Lê a versão principal ou a cópia validada. Só ausência de ambas significa instalação nova.
/// Não altera os arquivos durante a leitura.
/// # Errors
/// Erro de disco ou corrupção sem cópia válida, nunca convertido em ausência.
pub fn ler(path: &Path, validar: impl Fn(&[u8]) -> bool) -> io::Result<Option<Vec<u8>>> {
    let principal = ler_opcional(path).inspect_err(|_| avisar(path, false))?;
    // Um principal vazio após falta de energia é distinguível de uma limpeza
    // intencional: a limpeza grava uma cópia vazia ANTES de trocar o principal.
    if let Some(b) = &principal {
        if !b.is_empty() && validar(b) {
            return Ok(principal);
        }
    }
    let copia = ler_opcional(&ao_lado(path, "recuperacao")).inspect_err(|_| avisar(path, false))?;
    if let Some(b) = &copia {
        if validar(b) {
            if principal.as_ref() != copia.as_ref() {
                avisar(path, true);
            }
            return Ok(copia);
        }
    }
    if copia.is_none() && principal.as_ref().is_some_and(|b| validar(b)) {
        return Ok(principal);
    }
    if principal.is_none() && copia.is_none() {
        return Ok(None);
    }
    avisar(path, false);
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "dados locais corrompidos: {} (arquivos preservados)",
            path.display()
        ),
    ))
}
fn sincronizar_pasta(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(pai) = path.parent() {
        fs::File::open(pai)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    // Windows: cada arquivo é sincronizado e a troca não remove o destino.
    // Não prometemos resistência a hardware que ignora flush nem fsync de diretório no Windows.
    Ok(())
}
fn escrever_novo(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opcoes = OpenOptions::new();
    opcoes.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opcoes, 0o600);
    let mut f = opcoes.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}
fn trocar(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = ao_lado(path, &format!("nova-{:016x}", rand::random::<u64>()));
    let resultado = escrever_novo(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, path))
        .and_then(|()| sincronizar_pasta(path));
    if resultado.is_err() {
        let _ = fs::remove_file(tmp);
    }
    resultado
}
/// Valida, preserva uma cópia recuperável e troca o arquivo inteiro.
/// Um arquivo inválido só é substituído se existe cópia válida, e seus bytes
/// são preservados em `.corrompido-*` antes da substituição.
/// # Errors
/// Validação, disco, trava ou sincronização falhou; a única cópia não é truncada.
pub fn gravar(path: &Path, bytes: &[u8], validar: impl Fn(&[u8]) -> bool) -> io::Result<()> {
    if !validar(bytes) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "dados novos inválidos",
        ));
    }
    if let Some(pai) = path.parent() {
        fs::create_dir_all(pai)?;
    }
    let mut opcoes = OpenOptions::new();
    opcoes.create(true).write(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opcoes, 0o600);
    let trava = opcoes.open(ao_lado(path, "persistencia-trava"))?;
    trava.lock()?;
    let recuperada = ler(path, &validar)?;
    if let Some(antiga) = ler_opcional(path)? {
        if !validar(&antiga)
            || (antiga.is_empty() && recuperada.as_ref().is_some_and(|b| !b.is_empty()))
        {
            let guardada = ao_lado(path, &format!("corrompido-{:016x}", rand::random::<u64>()));
            escrever_novo(&guardada, &antiga)?;
            sincronizar_pasta(&guardada)?;
        }
    }
    trocar(&ao_lado(path, "recuperacao"), bytes)?;
    trocar(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pasta() -> PathBuf {
        let p = std::env::temp_dir().join(format!("seele-duravel-{:016x}", rand::random::<u64>()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn valido(b: &[u8]) -> bool {
        serde_json::from_slice::<Vec<String>>(b).is_ok()
    }
    #[test]
    fn corrupcao_recupera_sem_apagar_e_proxima_gravacao_preserva_o_original() {
        let dir = pasta();
        let p = dir.join("lista");
        gravar(&p, br#"["Casa"]"#, valido).unwrap();
        fs::write(&p, b"[quebrado").unwrap();
        assert_eq!(ler(&p, valido).unwrap().unwrap(), br#"["Casa"]"#);
        assert_eq!(fs::read(&p).unwrap(), b"[quebrado");
        gravar(&p, br#"["Casa","Amigos"]"#, valido).unwrap();
        assert!(fs::read_dir(&dir).unwrap().any(|e| {
            let p = e.unwrap().path();
            p.to_string_lossy().contains("corrompido-") && fs::read(p).unwrap() == b"[quebrado"
        }));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unica_copia_corrompida_bloqueia_escrita_sem_tocar_nos_bytes() {
        let dir = pasta();
        let p = dir.join("lista");
        fs::write(&p, b"[incompleto").unwrap();
        assert!(ler(&p, valido).is_err());
        assert!(gravar(&p, b"[]", valido).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"[incompleto");
        assert!(!ao_lado(&p, "recuperacao").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn interrupcao_antes_da_troca_deixa_antiga_ou_nova_e_vazio_intencional_nao_ressuscita() {
        let dir = pasta();
        let p = dir.join("lista");
        gravar(&p, br#"["Casa"]"#, valido).unwrap();
        // Ponto de falha: cópia nova durável, principal ainda antigo.
        trocar(&ao_lado(&p, "recuperacao"), br#"["Nova"]"#).unwrap();
        assert_eq!(ler(&p, valido).unwrap().unwrap(), br#"["Casa"]"#);
        fs::write(&p, b"").unwrap();
        assert_eq!(ler(&p, valido).unwrap().unwrap(), br#"["Nova"]"#);
        gravar(&p, b"[]", valido).unwrap();
        assert_eq!(ler(&p, valido).unwrap().unwrap(), b"[]");
        fs::remove_file(&p).unwrap();
        assert_eq!(ler(&p, valido).unwrap().unwrap(), b"[]");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn falha_ao_preparar_copia_nao_modifica_principal() {
        let dir = pasta();
        let p = dir.join("lista");
        fs::write(&p, b"[]").unwrap();
        fs::create_dir(ao_lado(&p, "recuperacao")).unwrap();
        assert!(gravar(&p, br#"["Casa"]"#, valido).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"[]");
        fs::remove_dir_all(dir).unwrap();
    }
}
