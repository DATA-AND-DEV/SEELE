//! Who this client is, across restarts.
//!
//! ADR 0004 makes identity an Ed25519 key pair. What it did not say is where
//! the key lives, and until M3 it did not matter: with no accounts, a fresh key
//! every run was simply a fresh person every run. PERSISTENCE changed that. A server
//! now binds a nickname to the identity that first claimed it, so a client that
//! forgets its key cannot come back — the server refuses the second connection
//! with "nickname belongs to a different identity", which is exactly the
//! protection it is supposed to offer and exactly the wrong answer to give
//! somebody reopening their own client.
//!
//! So the key is written to disk, and the pins beside it.
//!
//! # What this is not
//!
//! Not a key store. One identity, one file, no passphrase, no rotation. A key
//! sitting unencrypted in the user's own home directory is worth saying out
//! loud: anybody who can read that file can be this person. That is the same
//! trust boundary as an SSH private key without a passphrase, and it is a
//! deliberate M4 stopping point rather than an oversight — encryption at rest
//! belongs with the account work, not with the interface.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;

use crate::tofu::PinStore;

/// Loads the identity at `path`, creating one if there is none.
///
/// # Errors
///
/// Fails if the directory cannot be created, or the file exists and cannot be
/// read or is not a 32-byte key.
pub fn load_or_create(path: &Path) -> Result<SigningKey> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    if path.exists() {
        let bytes = std::fs::read(path)
            .with_context(|| format!("could not read the identity at {}", path.display()))?;
        let bytes: [u8; 32] = bytes.as_slice().try_into().with_context(|| {
            format!(
                "{} is {} bytes; an Ed25519 identity is 32",
                path.display(),
                bytes.len()
            )
        })?;
        return Ok(SigningKey::from_bytes(&bytes));
    }

    let key = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    write_private(path, key.as_bytes())
        .with_context(|| format!("could not write the identity to {}", path.display()))?;
    Ok(key)
}

/// Writes a file only this user can read, **atomically**.
///
/// The mode is set before the bytes go in, not after: a private key that is
/// world-readable for the width of one syscall has been world-readable.
///
/// # Por que ela escreve ao lado e depois renomeia
///
/// R13 da revisão da v15. A versão anterior abria o arquivo de destino com
/// `truncate(true)` e escrevia dentro dele. Entre o truncar e o `write_all` há
/// uma janela em que o arquivo existe e está **vazio** — e uma interrupção ali,
/// por disco cheio, por queda de energia ou por o processo morrer, deixa a
/// identidade ou a lista de pins como um arquivo vazio.
///
/// Um arquivo de pins vazio é pior que ausente: ele lê como «nunca confiei em
/// ninguém», e o aviso que mais importa — a chave do servidor mudou — deixa de
/// poder disparar, porque toda conexão volta a ser primeiro contato.
///
/// Escrever ao lado e renomear troca isso por um `rename`, que é atômico no
/// mesmo sistema de arquivos: ou o conteúdo antigo está lá, ou o novo está. Não
/// há terceiro estado.
///
/// O `sync_all` antes do `rename` é a outra metade, e ela é a que resiste a
/// queda de energia: sem ele o `rename` pode chegar ao disco antes dos bytes, e
/// o nome novo apontaria para um conteúdo que não existe.
#[cfg(unix)]
pub(crate) fn gravar_privado(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let ao_lado = temporario(path);
    // Um temporário que sobrou de uma tentativa interrompida não pode impedir a
    // próxima: `create(true).truncate(true)` o reaproveita.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&ao_lado)?;
    // Cada passo desfaz o temporário ao falhar. Sem isto, um disco cheio deixaria
    // um arquivo pela metade ao lado do bom, para sempre.
    if let Err(erro) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(&ao_lado);
        return Err(erro);
    }
    drop(file);
    if let Err(erro) = std::fs::rename(&ao_lado, path) {
        let _ = std::fs::remove_file(&ao_lado);
        return Err(erro);
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn gravar_privado(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // Windows inherits the directory's ACL, and the directory is under the
    // user's own profile. Weaker than the Unix mode, and worth knowing.
    //
    // Atômica pela mesma razão do caminho de cima, e com uma diferença do
    // sistema: `rename` no Windows falha quando o destino existe, então o
    // destino é removido antes. A janela que isso abre é de um `rename` e não de
    // uma escrita inteira, e é o melhor que a API oferece sem `MoveFileEx`, que
    // pediria uma dependência nova.
    let ao_lado = temporario(path);
    std::fs::write(&ao_lado, bytes)?;
    let _ = std::fs::remove_file(path);
    if let Err(erro) = std::fs::rename(&ao_lado, path) {
        let _ = std::fs::remove_file(&ao_lado);
        return Err(erro);
    }
    Ok(())
}

/// O nome antigo, para este arquivo e para os testes dele.
///
/// **Um apelido e não uma segunda cópia.** As duas cópias que existiam — esta e a
/// de `preferences` — divergiram uma vez: a de lá ficou sem o `sync_all` que esta
/// tinha. Uma função e dois nomes não tem como divergir.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    gravar_privado(path, bytes)
}

/// O caminho do arquivo temporário ao lado do definitivo.
///
/// **No mesmo diretório**, e é o que faz o `rename` ser atômico: entre sistemas
/// de arquivos diferentes ele deixa de ser uma troca de nome e passa a ser uma
/// cópia, que tem a mesma janela que este conserto está fechando.
fn temporario(path: &Path) -> PathBuf {
    let mut nome = path.as_os_str().to_owned();
    nome.push(".novo");
    PathBuf::from(nome)
}

/// A pin store backed by a file, so ADR 0003 survives a restart.
///
/// Trust on first use is only trust if the "first" is remembered. A store that
/// empties on exit makes every connection a first contact, which means the
/// warning that matters — the key changed — can never fire.
///
/// The format is one `host fingerprint` pair per line. Chosen so that somebody
/// who has been told their server's key changed can open the file, read it, and
/// compare by eye. A binary format would have made that a support conversation.
#[derive(Debug)]
pub struct FilePinStore {
    path: PathBuf,
    pins: Mutex<Vec<(String, String)>>,
    /// A última falha de gravação, se houve uma.
    ///
    /// # Por que ela precisa existir
    ///
    /// R13 da revisão da v15: `flush` fazia `let _ = write_private(...)`. Disco
    /// cheio, permissão negada ou escrita interrompida deixavam este processo
    /// lembrando a confiança **em RAM** e sem conseguir preservá-la para a
    /// próxima abertura — e ninguém era avisado de nada.
    ///
    /// O sintoma chega dias depois e não parece com a causa: na abertura
    /// seguinte, todo servidor volta a ser primeiro contato, e o aviso que mais
    /// importa — «a chave deste servidor mudou» — deixa de poder disparar.
    ///
    /// Guardada em vez de propagada porque [`PinStore::pin`] não devolve
    /// resultado, e não pode: quem a chama é o verificador de TLS, dentro do
    /// aperto de mão, e uma falha de disco ali não deve recusar a conexão — a
    /// confiança desta sessão está boa. O que ela deve fazer é ser **dita**, e
    /// [`Self::falha_de_gravacao`] é por onde a casca a lê.
    falha: Mutex<Option<String>>,
}

impl FilePinStore {
    /// Opens the store at `path`, reading whatever is already there.
    ///
    /// A file that cannot be parsed is treated as empty rather than fatal: the
    /// consequence is a re-pin on next contact, and refusing to start a chat
    /// client over a corrupt cache would be the worse trade.
    ///
    /// # Errors
    ///
    /// Fails only if the containing directory cannot be created.
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        // **Três estados, e não dois.** R13 pede a distinção por escrito:
        // arquivo ausente, ilegível e corrompido são coisas diferentes.
        //
        // - **ausente** é o caso normal de uma instalação nova, e não é notícia;
        // - **ilegível** é disco ou permissão, e é notícia: a lista existe e
        //   este processo não a alcança, então ele vai repinar tudo por cima de
        //   uma memória que continua lá;
        // - **linha torta** é corrupção parcial. As linhas boas ficam — jogar a
        //   lista inteira fora por causa de uma linha seria transformar um
        //   arquivo com um defeito num arquivo sem nada.
        let (pins, falha) = match std::fs::read_to_string(&path) {
            Ok(texto) => {
                let mut boas = Vec::new();
                let mut tortas = 0_usize;
                for linha in texto.lines() {
                    match linha.split_once(char::is_whitespace) {
                        Some((host, impressao)) if !host.trim().is_empty() => {
                            boas.push((host.trim().to_owned(), impressao.trim().to_owned()));
                        }
                        // Linha vazia não é corrupção: é o fim do arquivo.
                        _ if linha.trim().is_empty() => {}
                        _ => tortas += 1,
                    }
                }
                let falha = (tortas > 0).then(|| {
                    format!(
                        "{}: {tortas} linha(s) ilegível(is) na lista de chaves \
                         confiadas; as outras continuam valendo",
                        path.display()
                    )
                });
                if let Some(aviso) = &falha {
                    tracing::warn!("{aviso}");
                }
                (boas, falha)
            }
            Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None),
            Err(erro) => {
                let aviso = format!(
                    "{}: não deu para ler a lista de chaves confiadas ({erro}); esta \
                     sessão vai tratar todo servidor como primeiro contato",
                    path.display()
                );
                tracing::warn!("{aviso}");
                (Vec::new(), Some(aviso))
            }
        };

        Ok(Self {
            path,
            pins: Mutex::new(pins),
            falha: Mutex::new(falha),
        })
    }

    /// A última falha de leitura ou gravação da lista, se houve uma.
    ///
    /// `None` é o caso normal. Uma frase aqui quer dizer que a confiança desta
    /// sessão **não** vai sobreviver a fechar o aplicativo, e é a casca que a
    /// mostra — ver [`Self::falha`] para por que ela não é propagada como erro.
    #[must_use]
    pub fn falha_de_gravacao(&self) -> Option<String> {
        self.falha.lock().ok().and_then(|falha| falha.clone())
    }

    /// Grava a lista, e **anota** quando não consegue.
    ///
    /// Era `let _ = write_private(...)`. Ver [`Self::falha`].
    fn flush(&self, pins: &[(String, String)]) {
        let text: String = pins
            .iter()
            .map(|(host, fingerprint)| format!("{host} {fingerprint}\n"))
            .collect();
        match write_private(&self.path, text.as_bytes()) {
            Ok(()) => {
                if let Ok(mut falha) = self.falha.lock() {
                    // A gravação deu certo: a falha anterior deixou de ser
                    // verdade, e deixá-la ali faria a casca avisar para sempre.
                    *falha = None;
                }
            }
            Err(erro) => {
                let aviso = format!(
                    "{}: não deu para gravar a lista de chaves confiadas ({erro}); a \
                     confiança desta sessão não vai sobreviver a fechar o aplicativo",
                    self.path.display()
                );
                tracing::error!("{aviso}");
                if let Ok(mut falha) = self.falha.lock() {
                    *falha = Some(aviso);
                }
            }
        }
    }
}

impl PinStore for FilePinStore {
    fn pinned(&self, host: &str) -> Option<String> {
        self.pins.lock().ok().and_then(|pins| {
            pins.iter()
                .find(|(known, _)| known == host)
                .map(|(_, fingerprint)| fingerprint.clone())
        })
    }

    fn pin(&self, host: &str, fingerprint: String) {
        let Ok(mut pins) = self.pins.lock() else {
            return;
        };
        if let Some(slot) = pins.iter_mut().find(|(known, _)| known == host) {
            slot.1 = fingerprint;
        } else {
            pins.push((host.to_owned(), fingerprint));
        }
        self.flush(&pins);
    }

    fn unpin(&self, host: &str) {
        let Ok(mut pins) = self.pins.lock() else {
            return;
        };
        pins.retain(|(known, _)| known != host);
        self.flush(&pins);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("seele-identity-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn an_identity_survives_the_process_that_made_it() {
        // The whole point. Without this the same nickname cannot be reclaimed
        // from the same machine, because PERSISTENCE binds it to the first key.
        let dir = scratch("survives");
        let path = dir.join("identity.key");

        let first = load_or_create(&path).expect("create");
        let second = load_or_create(&path).expect("reload");

        assert_eq!(first.to_bytes(), second.to_bytes());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A gravação troca o arquivo, em vez de esvaziar o que está lá.**
    ///
    /// R13.
    ///
    /// # O que este teste mede, e o que não bastaria medir
    ///
    /// Matar o processo entre o truncar e o escrever não cabe num teste de
    /// unidade, e «o conteúdo novo está lá» não distingue as duas formas — a
    /// gravação direta também o deixa lá quando nada falha. Medido: a primeira
    /// versão deste teste conferia o conteúdo e a ausência do temporário, e
    /// **passava** com a gravação direta restaurada.
    ///
    /// O que distingue é o **inode**. Escrever ao lado e renomear troca a
    /// entrada do diretório, então o arquivo depois da segunda gravação é outro
    /// arquivo; truncar e escrever reusa o mesmo, e é dentro desse reuso que mora
    /// a janela em que ele está vazio.
    ///
    /// Só no Unix: no Windows a API não dá o número, e o caminho de lá está
    /// escrito com a ressalva dele em `write_private`.
    #[cfg(unix)]
    #[test]
    fn a_gravacao_troca_o_arquivo_em_vez_de_esvaziar_o_que_esta_la() {
        use std::os::unix::fs::MetadataExt as _;

        let dir = scratch("atomica");
        std::fs::create_dir_all(&dir).expect("pasta");
        let alvo = dir.join("pins");

        write_private(&alvo, b"um.exemplo.br aaaa\n").expect("primeira");
        let primeiro = std::fs::metadata(&alvo).expect("metadados").ino();
        write_private(&alvo, b"dois.exemplo.br bbbb\n").expect("segunda");
        let segundo = std::fs::metadata(&alvo).expect("metadados").ino();

        assert_eq!(
            std::fs::read_to_string(&alvo).expect("ler"),
            "dois.exemplo.br bbbb\n"
        );
        assert_ne!(
            primeiro, segundo,
            "a gravação reusou o mesmo arquivo: ela está truncando o destino e \
             escrevendo dentro dele, e entre as duas coisas existe um instante em \
             que a lista de chaves confiadas está **vazia** em disco"
        );
        assert!(
            !dir.join("pins.novo").exists(),
            "o temporário ficou em disco: uma tentativa interrompida deixaria \
             lixo ao lado do arquivo bom para sempre"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Uma lista ausente não é notícia; uma ilegível é.**
    ///
    /// R13 pede a distinção entre arquivo ausente, ilegível e corrompido.
    #[test]
    fn a_lista_ausente_abre_calada_e_a_torta_avisa() {
        let dir = scratch("tres-estados");
        std::fs::create_dir_all(&dir).expect("pasta");

        // Ausente: o caso de toda instalação nova.
        let vazia = FilePinStore::open(dir.join("nao-existe")).expect("abrir");
        assert_eq!(vazia.falha_de_gravacao(), None);
        assert_eq!(vazia.pinned("qualquer"), None);

        // Corrompida em parte: as linhas boas ficam, e a torta é dita.
        let torta = dir.join("torta");
        std::fs::write(
            &torta,
            "bom.exemplo.br aaaa
issoaquinaoehlinha
",
        )
        .expect("escrever");
        let store = FilePinStore::open(torta).expect("abrir");
        assert_eq!(
            store.pinned("bom.exemplo.br").as_deref(),
            Some("aaaa"),
            "uma linha torta levou a lista inteira: um arquivo com um defeito \
             virou um arquivo sem nada, e toda conexão volta a ser primeiro \
             contato"
        );
        assert!(
            store.falha_de_gravacao().is_some(),
            "a corrupção passou em silêncio"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **E uma gravação que falha é dita**, em vez de engolida por um `let _`.
    #[test]
    fn uma_gravacao_que_falha_e_anotada() {
        let dir = scratch("falha");
        std::fs::create_dir_all(&dir).expect("pasta");
        let alvo = dir.join("pins");

        // Aberta com o caminho livre: nada a acusar ainda.
        let store = FilePinStore::open(alvo.clone()).expect("abrir");
        assert_eq!(
            store.falha_de_gravacao(),
            None,
            "a abertura já acusou falha antes de qualquer gravação"
        );

        // **E o caminho é ocupado depois**, por um diretório não vazio: renomear
        // sobre ele falha. É uma falha de gravação que não depende de encher o
        // disco de quem roda o teste, e o momento — depois da abertura — é o que
        // separa «não deu para ler» de «não deu para gravar».
        std::fs::create_dir_all(alvo.join("ocupado")).expect("ocupar o caminho");

        store.pin("exemplo.br", "aaaa".into());
        assert!(
            store.falha_de_gravacao().is_some(),
            "a gravação falhou em silêncio: este processo lembra a confiança em \
             RAM e não vai preservá-la, e ninguém foi avisado"
        );
        // E a confiança desta sessão continua valendo, que é o certo: o disco
        // falhou, a conexão não.
        assert_eq!(store.pinned("exemplo.br").as_deref(), Some("aaaa"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_identity_file_is_not_readable_by_anybody_else() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("mode");
        let path = dir.join("identity.key");
        load_or_create(&path).expect("create");

        let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
        assert_eq!(mode & 0o077, 0, "the private key is readable by others");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_a_key_is_reported_rather_than_replaced() {
        // Overwriting it would destroy the identity of somebody whose disk
        // filled up mid-write. Refusing to start is recoverable; this is not.
        let dir = scratch("garbage");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("identity.key");
        std::fs::write(&path, b"not a key").expect("write");

        assert!(load_or_create(&path).is_err());
        assert_eq!(
            std::fs::read(&path).expect("read"),
            b"not a key",
            "a corrupt identity was silently replaced"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pin_survives_the_process_that_made_it() {
        // Trust on first use needs the first use remembered, or every contact
        // is a first contact and the warning can never fire.
        let dir = scratch("pins");
        let path = dir.join("pins");

        {
            let store = FilePinStore::open(path.clone()).expect("open");
            store.pin("seele.exemplo", "abc123".into());
        }

        let reopened = FilePinStore::open(path).expect("reopen");
        assert_eq!(reopened.pinned("seele.exemplo"), Some("abc123".into()));
        assert_eq!(reopened.pinned("outro"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unpin_survives_the_process_that_made_it() {
        // The opposite of `a_pin_survives_the_process_that_made_it`: a
        // refusal undoes the pin the verifier already wrote so the next
        // visit is a clean first contact again. If `unpin` forgot to flush,
        // the pin would come back on the next launch and the refusal would
        // silently undo itself on restart — exactly the decorative refusal
        // this feature exists to remove.
        let dir = scratch("unpin");
        let path = dir.join("pins");

        {
            let store = FilePinStore::open(path.clone()).expect("open");
            store.pin("seele.exemplo", "abc123".into());
            store.unpin("seele.exemplo");
        }

        let reopened = FilePinStore::open(path).expect("reopen");
        assert_eq!(reopened.pinned("seele.exemplo"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn repinning_a_host_replaces_rather_than_appends() {
        // A file with two entries for one host answers differently depending on
        // which one is found first, which is a coin flip on a security check.
        let dir = scratch("repin");
        let path = dir.join("pins");
        let store = FilePinStore::open(path.clone()).expect("open");

        store.pin("seele.exemplo", "antigo".into());
        store.pin("seele.exemplo", "novo".into());

        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text.lines().count(), 1, "two pins for one host:\n{text}");
        assert_eq!(store.pinned("seele.exemplo"), Some("novo".into()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_pin_file_costs_a_repin_and_not_a_failure_to_start() {
        let dir = scratch("corrupt-pins");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("pins");
        std::fs::write(&path, b"\x00\x01 sem sentido\n").expect("write");

        let store = FilePinStore::open(path).expect("a corrupt cache stopped the client");
        assert_eq!(store.pinned("seele.exemplo"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
