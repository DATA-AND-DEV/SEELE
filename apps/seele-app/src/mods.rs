//! Reading a MOD's own files, for the bridge that hands them to a worker.
//!
//! ADR 0045 put these files behind a scheme of our own, `mod://`, because
//! `script-src 'self'` refuses any script not compiled into the binary.
//! **ADR 0049 took the scheme away**: a MOD no longer runs in the product's
//! window, so there is nothing in the page that loads a MOD's bytes as script,
//! and a scheme that served them would be a door with nobody on the other side.
//!
//! What is left is the reading, and it now answers one caller — `codigo_do_mod`,
//! which hands the text to a worker built from a `blob:`. The two guards that
//! made the scheme safe are the reason this module still exists, and neither is
//! politeness:
//!
//! - **only what the manifest declares.** A file sitting in the directory that
//!   `mod.json` never names is not read. A MOD is what it declared it was.
//! - **nothing climbs out.** The path is rebuilt from components, so a `..`
//!   anywhere is refused rather than resolved. Without this, reading would be
//!   arbitrary disk reads from a path a third party chooses.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

/// O manifesto que os testes deste módulo usam, com a API vinda da constante.
///
/// Escrita à mão ela era `1`, e passou a recusar no dia em que o ADR 0049 fez a
/// API ser uma só. Nenhum destes testes é sobre a versão: eles são sobre ler o
/// que o manifesto declara, copiar árvore, publicar por conteúdo e não deixar
/// estufa — e um `1` cravado os faria reprovar por um motivo que não é o deles.
#[cfg(test)]
fn manifesto_de_teste(id: &str) -> String {
    let api = seele_ffi::mods::MOD_API_VERSION;
    format!(
        r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":{api},
            "repo":"https://example.invalid/x","reach":["dom"],
            "client":"cliente/main.js"}}"#
    )
}

/// Reads one path inside a MOD's package, or nothing.
///
/// `url_path` is the path component of the URL, e.g.
/// `/seele/exemplo/cliente/main.js` — author, name, then the file inside the
/// MOD.
///
/// Returns `None` for anything that is not a file this MOD declared, which is
/// deliberately the same answer for "does not exist", "not declared" and
/// "tried to climb out": a scheme that distinguishes them is a scheme that
/// answers questions about the disk of whoever is running it.
#[must_use]
pub(crate) fn serve(config_dir: &Path, url_path: &str, hash: &str) -> Option<Vec<u8>> {
    let mut parts = url_path.trim_start_matches('/').split('/');
    let author = parts.next()?;
    let name = parts.next()?;
    let inside: Vec<&str> = parts.collect();
    if inside.is_empty() || author.is_empty() || name.is_empty() {
        return None;
    }

    let relative = seele_ffi::mods::caminho_interno(&inside)?;

    // **O mesmo pacote que a conferência acima aprovou**, achado pelo mesmo
    // hash. Achá-lo de outro jeito aqui seria conferir uma coisa e ler outra —
    // a forma clássica de uma checagem valer para um arquivo e a leitura
    // acontecer sobre outro.
    let id = format!("{author}/{name}");
    let pacote = seele_ffi::mods::ler_por_hash(&config_dir.to_string_lossy(), hash).ok()?;
    if pacote.id != id {
        return None;
    }
    // Only what the manifest declares. That is the client script **and** the
    // files the manifest lists under `arquivos` — a MOD may ship more than one
    // file now, so the list grew and this guard did not change shape.
    let declarados = pacote
        .client
        .iter()
        .chain(pacote.arquivos.iter())
        .any(|declarado| Path::new(declarado) == relative.as_path());
    if !declarados {
        return None;
    }

    std::fs::read(
        config_dir
            .join(seele_ffi::mods::PACOTES)
            .join(hash)
            .join(&relative),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn semear(raiz: &Path, id: &str) -> (PathBuf, String) {
        let dir = raiz.join("em-obras");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cliente")).expect("criar diretório");
        std::fs::write(dir.join("mod.json"), manifesto_de_teste(id)).expect("manifesto");
        std::fs::write(dir.join("cliente/main.js"), "globalThis.MOD_RODOU = true;")
            .expect("script");
        // Publicado pelo conteúdo, como o instalador faz: o nome da pasta é o
        // hash do que está dentro, e é por ele que o `mod://` resolve.
        let hash = seele_ffi::mods::ler_pasta(&dir.to_string_lossy())
            .expect("ler")
            .hash;
        let destino = raiz.join(seele_ffi::mods::PACOTES).join(&hash);
        std::fs::create_dir_all(destino.parent().expect("pai")).expect("raiz");
        std::fs::rename(&dir, &destino).expect("publicar");
        (destino, hash)
    }

    fn temporario(nome: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("seele-modserve-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    #[test]
    fn um_arquivo_declarado_no_manifesto_e_servido() {
        let raiz = temporario("ok");
        let (_, hash) = semear(&raiz, "seele/exemplo");
        let bytes = serve(&raiz, "/seele/exemplo/cliente/main.js", &hash).expect("devia servir");
        assert_eq!(bytes, b"globalThis.MOD_RODOU = true;");
    }

    /// O guarda que substitui o que a CSP deixa de garantir: um arquivo que o
    /// manifesto não declara não é servido, mesmo estando no diretório.
    #[test]
    fn um_arquivo_que_o_manifesto_nao_declara_nao_e_servido() {
        let raiz = temporario("nao-declarado");
        let (dir, hash) = semear(&raiz, "seele/exemplo");
        std::fs::write(dir.join("cliente/extra.js"), "// clandestino").expect("extra");
        assert_eq!(serve(&raiz, "/seele/exemplo/cliente/extra.js", &hash), None);
    }

    /// Caminho inteiro por `serve`, e **este teste não é a prova da travessia**.
    ///
    /// Vale dizer, porque a suposição contrária já custou uma vez: com a regra
    /// de `seele_proto::mods::inner_path` inteiramente quebrada, este teste
    /// continua verde — a conferência do manifesto recusa estes caminhos por
    /// outro motivo. A prova de verdade é
    /// `nenhum_caminho_com_dois_pontos_vira_caminho_interno`, lá, com os dois
    /// lados. Aqui é integração: o `serve` recusa, seja qual for a razão.
    #[test]
    fn um_caminho_que_tenta_subir_nao_e_servido() {
        let raiz = temporario("subir");
        let (_, hash) = semear(&raiz, "seele/exemplo");
        for caminho in [
            "/seele/exemplo/../../../etc/passwd",
            "/seele/exemplo/cliente/../../mod.json",
            "/../mods/seele/exemplo/cliente/main.js",
        ] {
            assert_eq!(serve(&raiz, caminho, &hash), None, "subiu com `{caminho}`");
        }
    }

    /// **O hash de um MOD com o nome de outro não serve nenhum dos dois.**
    ///
    /// Com o cache por conteúdo, a URL diz **quais bytes** quer. Servir só pelo
    /// hash entregaria o conteúdo de um MOD sob o caminho de outro — e o
    /// caminho é o que a janela usa para decidir de quem é o script. Servir só
    /// pelo identificador entregaria «o que estiver instalado», que pode ser o
    /// pacote que outro servidor desta máquina baixou depois.
    ///
    /// As duas conferências, então: os bytes existem sob aquele hash, **e**
    /// dizem ser deste MOD.
    #[test]
    fn o_hash_de_um_mod_com_o_nome_de_outro_e_recusado() {
        let raiz = temporario("trocado");
        let (_, um) = semear(&raiz, "seele/exemplo");
        let (_, outro) = semear(&raiz, "seele/outro");
        assert_ne!(um, outro);

        assert!(
            serve(&raiz, "/seele/exemplo/cliente/main.js", &um).is_some(),
            "o caminho certo com o hash certo tem de passar"
        );
        assert_eq!(
            serve(&raiz, "/seele/outro/cliente/main.js", &um),
            None,
            "o conteúdo de um MOD foi servido sob o caminho de outro"
        );
    }

    #[test]
    fn um_mod_que_nao_existe_nao_e_servido() {
        let raiz = temporario("ausente");
        assert_eq!(
            serve(&raiz, "/seele/fantasma/cliente/main.js", &"0".repeat(64)),
            None
        );
    }

    #[test]
    fn nenhum_caminho_causa_panico() {
        let raiz = temporario("panico");
        let (_, hash) = semear(&raiz, "seele/exemplo");
        for caminho in ["", "/", "//", "/a", "/a/b", "\u{0}", "/seele/exemplo/"] {
            let _ = serve(&raiz, caminho, &hash);
            let _ = serve(&raiz, caminho, "");
        }
    }
}

/// Por que um pacote não pôde ser instalado.
///
/// Uma variante por recusa, com o motivo dentro — a mesma regra do resto desta
/// ponte: a frase mora no `FRASES` do JavaScript, e ela precisa saber **qual**
/// recusa foi para dizer o conserto.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) enum FalhaAoInstalarMod {
    /// A pasta escolhida não tem `mod.json`.
    ///
    /// O engano mais provável de todos: apontar para a pasta que **contém** o
    /// MOD em vez da pasta do MOD.
    SemManifesto,
    /// O `mod.json` existe e não vale, com o nome da recusa do `seele-core`.
    ManifestoRecusado(String),
    /// Há um atalho (link simbólico) dentro da pasta, com o caminho dele.
    ///
    /// **A recusa está certa, e a variante própria é para dizê-la.** Ver
    /// [`copiar_arvore`] para o porquê de atalho não passar. O caso que a
    /// separou de [`FalhaAoInstalarMod::NaoCopiei`] é a pasta de quem
    /// desenvolve o MOD, com `node_modules/.bin/` cheia de atalhos: misturada
    /// às falhas do disco, a tela não tinha como dizer qual era o atalho nem o
    /// que fazer (pendência 53).
    TemAtalho(String),
    /// O disco recusou, e o que ele disse.
    ///
    /// **Aqui havia um `JaInstalado`**, e ele saiu com o endereçamento por
    /// conteúdo. A recusa existia porque havia um lugar por identificador, e
    /// escrever por cima de uma instalação boa podia quebrá-la. Pastas
    /// endereçadas pelo conteúdo não se sobrescrevem: bytes iguais são a mesma
    /// pasta, e bytes diferentes são outra.
    NaoCopiei(String),
}

/// Tira um pacote do cache local, e **só** o pacote.
///
/// # Por que ela não toca em `mod-data`
///
/// Plano de isolamento de 18/09: «apagar cache de convidado não apaga dados
/// remotos». O cache guarda os bytes que a janela carrega, e nada mais; o que
/// um MOD guardou vive no banco do servidor que o hospeda, e o que ele escreveu
/// em arquivo vive na raiz daquela instância. Apagar bytes que se baixa de novo
/// não pode apagar o que ninguém tem como recuperar.
///
/// Por isso ela remove exatamente `mod-packages/<hash>` e nunca desce em outro
/// lugar. [`seele_core::mods::caminho_do_pacote`] recusa um hash que não tenha
/// forma de hash, e é lá que a travessia morre.
///
/// # Errors
///
/// [`FalhaAoApagarPacote`] quando o hash não tem forma de hash, quando não há
/// pacote nenhum sob ele, ou quando o disco recusou.
pub(crate) fn apagar_do_cache(config_dir: &Path, hash: &str) -> Result<(), FalhaAoApagarPacote> {
    let Some(dir) = seele_ffi::mods::caminho_do_pacote(&config_dir.to_string_lossy(), hash) else {
        return Err(FalhaAoApagarPacote::HashTorto);
    };
    if !dir.is_dir() {
        return Err(FalhaAoApagarPacote::NaoEstaAqui);
    }
    std::fs::remove_dir_all(&dir).map_err(|erro| FalhaAoApagarPacote::NaoApaguei(erro.to_string()))
}

/// Por que um pacote não saiu do cache.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum FalhaAoApagarPacote {
    /// O hash não tem forma de hash, então não nomeia pacote nenhum.
    HashTorto,
    /// Não há pacote sob aquele hash. Dito, e não engolido: apagar duas vezes
    /// da mesma tela é apagar o que a outra aba já tinha apagado, e quem
    /// aperta merece saber que não foi ele.
    NaoEstaAqui,
    /// O disco recusou, e o que ele disse.
    NaoApaguei(String),
}

/// Instala um MOD a partir de uma pasta desta máquina.
///
/// # O que ele confere antes de tocar no disco
///
/// O manifesto, inteiro, pelo mesmo `read_one` que o resto do produto usa. Um
/// pacote sem `mod.json`, com identificador malformado ou com esquema que este
/// build não lê é recusado **antes** de qualquer byte ser copiado — pela mesma
/// ordem que o depósito de versões segue, e pelo mesmo motivo: meia instalação
/// é o estado que ninguém sabe desfazer.
///
/// # O que ele não faz
///
/// **Não habilita.** Instalar põe os bytes no disco; ligar é outra decisão, de
/// quem hospeda, e tem o verbo dela. Separá-las é o que permite examinar um MOD
/// instalado antes de deixá-lo rodar.
///
/// **Não desce da rede.** A origem é uma pasta que já está aqui. Baixar do
/// catálogo é trabalho do indexador, que não está no ar.
///
/// # Errors
///
/// [`FalhaAoInstalarMod`], uma variante por motivo.
/// Leva os pacotes do layout antigo para o cache por conteúdo, uma vez.
///
/// Até a v0.11.x o pacote morava em `mods/<autor>/<nome>/`. Quem já tem MODs
/// instalados tem os três oficiais lá, e a leitura passou a ser em
/// `mod-packages/<hash>/`: sem esta mudança eles sumiriam da lista, e quem
/// usasse leria isso como o produto ter perdido o que estava instalado.
///
/// **Copia e não move**, ao contrário da mudança dos dados. Os dois motivos:
/// o pacote é imutável, então uma segunda cópia não diverge da primeira; e o
/// layout antigo ainda é lido por `listar`, que a tela de gestão usa enquanto
/// a migração da interface não fecha. Apagar aqui deixaria a tela mostrando o
/// que já não existe.
///
/// Silenciosa quando não há o que levar, que é toda máquina depois da primeira
/// vez — o destino é o hash, e um pacote já levado já está lá.
pub(crate) fn levar_pacotes_antigos_para_o_cache(config_dir: &Path) {
    let antigos = config_dir.join("mods");
    let Ok(autores) = std::fs::read_dir(&antigos) else {
        return;
    };
    for autor in autores.flatten() {
        let Ok(nomes) = std::fs::read_dir(autor.path()) else {
            continue;
        };
        for nome in nomes.flatten() {
            let dir = nome.path();
            if !dir.join("mod.json").is_file() {
                continue;
            }
            let Ok(lido) = seele_ffi::mods::ler_pasta(&dir.to_string_lossy()) else {
                continue;
            };
            let destino = config_dir.join(seele_ffi::mods::PACOTES).join(&lido.hash);
            if destino.exists() {
                continue;
            }
            // Pela estufa, e não direto: uma cópia que falha no meio não pode
            // deixar no cache uma pasta cujo conteúdo não bate com o nome dela
            // — a listagem a recusaria, e a recusa apontaria para um defeito
            // que não existe.
            let estufa = config_dir.join(".mods-em-obras").join(&lido.hash);
            let _ = std::fs::remove_dir_all(&estufa);
            if copiar_arvore(&dir, &estufa).is_err() {
                let _ = std::fs::remove_dir_all(&estufa);
                continue;
            }
            if let Some(pai) = destino.parent() {
                if std::fs::create_dir_all(pai).is_err() {
                    let _ = std::fs::remove_dir_all(&estufa);
                    continue;
                }
            }
            match std::fs::rename(&estufa, &destino) {
                Ok(()) => tracing::info!(
                    mod_id = %lido.id,
                    hash = %lido.hash,
                    "pacote levado do layout antigo para o cache por conteúdo"
                ),
                Err(erro) => {
                    tracing::warn!(mod_id = %lido.id, %erro, "não consegui levar este pacote");
                    let _ = std::fs::remove_dir_all(&estufa);
                }
            }
        }
    }
}

/// O que uma instalação publicou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Publicado {
    /// `autor/nome`, lido do manifesto.
    pub(crate) id: String,
    /// O hash do conteúdo, que é o nome da pasta onde ele ficou.
    pub(crate) hash: String,
}

/// Põe um pacote no cache endereçado pelo conteúdo.
///
/// # Por que isto encolheu
///
/// Antes havia um lugar por identificador, e daí vinha tudo o mais: um sinal
/// de `substituir`, uma recusa `JaInstalado`, a troca em dois `rename` com
/// volta atrás, e a mudança do `dados/` de um lado para o outro para que
/// atualizar não apagasse a mesa de ninguém.
///
/// Endereçado pelo conteúdo, nada disso existe:
///
///   - **não há substituição.** Bytes iguais dão a mesma pasta; bytes
///     diferentes dão outra. Instalar por cima é uma operação que não tem como
///     acontecer;
///   - **não há «já instalado» como recusa.** Se a pasta existe, o trabalho
///     está feito — e responder erro a quem pediu exatamente o que já está lá
///     seria inventar um problema;
///   - **não há dados a preservar.** Eles saíram do pacote na separação de
///     raízes: vivem em `servidores/<id>/mod-data`, que é da instância.
///
/// A montagem ao lado fica, e pelo motivo de sempre (A10 da auditoria): uma
/// cópia que falha no meio não pode deixar uma pasta com parte do MOD onde a
/// leitura seguinte a encontre.
///
/// # Toda recusa fica no `seele.log`
///
/// Com a pasta de onde se tentou instalar e a recusa inteira, o caminho do
/// atalho incluído. A tela diz a quem está na frente dela; o registro é o que
/// chega a quem conserta, e antes da pendência 53 ele não ganhava linha
/// nenhuma — a recusa era certa, e o produto sabia o motivo e não contava.
///
/// # Errors
///
/// [`FalhaAoInstalarMod`] quando falta manifesto, quando ele é recusado,
/// quando há atalho dentro da pasta, ou quando a cópia não termina.
pub(crate) fn instalar_de(
    config_dir: &Path,
    origem: &Path,
) -> Result<Publicado, FalhaAoInstalarMod> {
    let publicado = publicar_de(config_dir, origem);
    if let Err(recusa) = &publicado {
        // **A pasta entre aspas e com escape**, por `caminho_no_registro`. O
        // nome dela vem de quem empacotou o MOD, e o formatador não escapa
        // quebra de linha num campo `%`: um `\n` no nome escreveria no
        // `seele.log` uma segunda linha com a cara do produto. A recusa vai
        // pelo `Debug`, que já escapa o caminho do atalho dentro dela.
        tracing::warn!(
            origem = %crate::caminho_no_registro(&origem.to_string_lossy()),
            ?recusa,
            "não instalei o MOD desta pasta"
        );
    }
    publicado
}

/// O trabalho de [`instalar_de`], sem o registro: toda saída `Err` daqui é uma
/// recusa, e é lá, num lugar só, que ela vira linha no `seele.log`. A outra
/// saída antecipada, a do pacote que já está no cache, é `Ok`.
fn publicar_de(config_dir: &Path, origem: &Path) -> Result<Publicado, FalhaAoInstalarMod> {
    if !origem.join("mod.json").is_file() {
        return Err(FalhaAoInstalarMod::SemManifesto);
    }
    let lido = seele_ffi::mods::ler_pasta(&origem.to_string_lossy())
        .map_err(FalhaAoInstalarMod::ManifestoRecusado)?;
    let id = lido.id;

    // A estufa fica **fora** da raiz dos pacotes, e não é detalhe: a listagem
    // confere o nome da pasta contra o conteúdo, e uma pasta de obras lá dentro
    // apareceria como um pacote recusado enquanto a cópia acontece.
    let estufa = config_dir.join(".mods-em-obras").join(&id);
    let _ = std::fs::remove_dir_all(&estufa);
    if let Err(falha) = copiar_arvore(origem, &estufa) {
        let _ = std::fs::remove_dir_all(&estufa);
        return Err(match falha {
            FalhaNaCopia::Atalho(caminho) => {
                FalhaAoInstalarMod::TemAtalho(caminho.display().to_string())
            }
            FalhaNaCopia::Disco(erro) => FalhaAoInstalarMod::NaoCopiei(erro.to_string()),
        });
    }

    // **O hash sai do que foi copiado, e não do que foi lido.** São a mesma
    // coisa quando a cópia deu certo — e quando não deu, é o que está em disco
    // que vai ser servido, então é dele que o nome tem de sair.
    let copiado = match seele_ffi::mods::ler_pasta(&estufa.to_string_lossy()) {
        Ok(copiado) => copiado,
        Err(motivo) => {
            let _ = std::fs::remove_dir_all(&estufa);
            return Err(FalhaAoInstalarMod::ManifestoRecusado(motivo));
        }
    };
    let hash = copiado.hash;

    let destino = config_dir.join(seele_ffi::mods::PACOTES).join(&hash);
    if destino.exists() {
        // Já está aqui, com estes bytes. Não é erro, e não há o que fazer.
        let _ = std::fs::remove_dir_all(&estufa);
        return Ok(Publicado { id, hash });
    }
    if let Some(pai) = destino.parent() {
        if let Err(erro) = std::fs::create_dir_all(pai) {
            let _ = std::fs::remove_dir_all(&estufa);
            return Err(FalhaAoInstalarMod::NaoCopiei(erro.to_string()));
        }
    }
    if let Err(erro) = std::fs::rename(&estufa, &destino) {
        let _ = std::fs::remove_dir_all(&estufa);
        return Err(FalhaAoInstalarMod::NaoCopiei(erro.to_string()));
    }
    Ok(Publicado { id, hash })
}

/// Copia uma árvore de arquivos, recusando atalhos.
///
/// **Atalho não passa**, e não é zelo: um link simbólico dentro do pacote faria
/// a instalação publicar como «arquivo deste MOD» algo que está noutro lugar do
/// disco — e o `mod://` serviria aquele conteúdo à janela. É a mesma recusa que
/// o `seele-lancador` faz ao ler um pacote de versão.
///
/// # Errors
///
/// [`FalhaNaCopia::Atalho`] com o caminho do atalho, e [`FalhaNaCopia::Disco`]
/// com o que o disco disse para o resto.
fn copiar_arvore(de: &Path, para: &Path) -> Result<(), FalhaNaCopia> {
    std::fs::create_dir_all(para)?;
    for entrada in std::fs::read_dir(de)? {
        let entrada = entrada?;
        let tipo = entrada.file_type()?;
        if tipo.is_symlink() {
            return Err(FalhaNaCopia::Atalho(entrada.path()));
        }
        let alvo = para.join(entrada.file_name());
        if tipo.is_dir() {
            copiar_arvore(&entrada.path(), &alvo)?;
        } else {
            std::fs::copy(entrada.path(), &alvo)?;
        }
    }
    Ok(())
}

/// Por que [`copiar_arvore`] parou.
///
/// **Dois motivos, e não um `std::io::Error`.** O atalho era um erro de disco
/// inventado, com o caminho dentro do texto, e por isso chegava à tela como
/// [`FalhaAoInstalarMod::NaoCopiei`], junto das falhas que o disco dá de
/// verdade. Separado aqui, ele vira [`FalhaAoInstalarMod::TemAtalho`] com o
/// caminho como dado, e a tela diz qual é ele (pendência 53).
#[derive(Debug)]
enum FalhaNaCopia {
    /// Um atalho dentro da árvore, com o caminho dele.
    Atalho(std::path::PathBuf),
    /// O disco recusou, e o que ele disse.
    Disco(std::io::Error),
}

impl From<std::io::Error> for FalhaNaCopia {
    fn from(erro: std::io::Error) -> Self {
        Self::Disco(erro)
    }
}

/// Um MOD ligado num servidor guardado que este app não roda (tarefa 13 do
/// plano da F3, Step 5b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModDeApiAntiga {
    pub(crate) id: String,
    /// A API que o pacote declara; `None` quando o pacote não está nesta
    /// máquina ou o `mod.json` dele não abre. **Nunca «API 0»**: zero é «não
    /// sei», e a tela diz que o pacote não está aqui, sem número nenhum.
    pub(crate) api: Option<u32>,
}

/// **Os MODs ligados num banco que este app não roda** — quem hospeda sabe ao
/// hospedar, e o servidor não sobe calado (tarefa 13 do plano da F3, Step 5b;
/// aprovado pela orquestradora em 10/10).
///
/// Depois do corte da API 6, um servidor 0.16 que ainda tem ligado um MOD da
/// 5 recusa todo app 0.16 (`ModsDeOutraApi`), e quem hospeda só ganhava uma
/// queixa no registro. Esta função responde antes de `Hospedagem::iniciar`.
///
/// **Uma função pura do app, que lê o banco só para leitura.** Ela não pode ler
/// a coluna `api`: o banco de quem vem da 0.15 está no esquema 13 e não a tem, e
/// quem a cria (migração 14) e a preenche (`preencher_as_apis`) é a subida do
/// servidor — depois do ponto em que esta pergunta é feita. Ela lê `id` e
/// `hash` das linhas ligadas, que existem do esquema 11 ao 15, e a API pelo
/// pacote, pela **mesma regra** da subida (`api_do_pacote`). Não migra nada.
///
/// Sem banco, ou sem a tabela, nenhum MOD: um servidor que nunca ligou MOD não
/// tem o que conferir. O motivo exato de um `None` vai ao `seele.log`.
pub(crate) fn mods_de_api_antiga(
    banco: &Path,
    raizes: &seele_server::RaizesDosMods,
) -> Vec<ModDeApiAntiga> {
    let Ok(conexao) =
        rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return Vec::new();
    };
    let Ok(mut consulta) =
        conexao.prepare("SELECT id, hash FROM mods WHERE enabled = 1 ORDER BY id")
    else {
        return Vec::new();
    };
    let Ok(linhas) = consulta.query_map([], |linha| {
        Ok((linha.get::<_, String>(0)?, linha.get::<_, String>(1)?))
    }) else {
        return Vec::new();
    };
    let mut antigos = Vec::new();
    for (id, hash) in linhas.flatten() {
        match seele_server::mods::api_do_pacote(raizes, &hash) {
            Ok(api) if api >= seele_ffi::mods::MOD_API_MINIMA => {}
            Ok(api) => antigos.push(ModDeApiAntiga { id, api: Some(api) }),
            Err(motivo) => {
                tracing::warn!(mod_id = %id, %motivo, "a API de um MOD ligado não se lê ao hospedar");
                antigos.push(ModDeApiAntiga { id, api: None });
            }
        }
    }
    antigos
}

/// **A linha que liga estes bytes**: os sete campos lidos do pacote por hash,
/// no mesmo ato (migrações 12 e 14), ou o nome da recusa.
///
/// Uma regra só para a gestão (`habilitar_mod`) e para a saída 1 de quem
/// hospeda: por hash, porque dois pacotes do mesmo MOD podem estar no disco, e
/// quem liga sabe quais bytes quer; e recusado quando os bytes são de outro
/// MOD. Um pacote de API que este build não roda não chega aqui: o
/// `read_manifest` o recusa ao ler.
pub(crate) fn linha_do_pacote(
    config: &str,
    id: &str,
    hash: &str,
) -> Result<seele_server::persistence::mods::EnabledMod, String> {
    let instalado = seele_ffi::mods::ler_por_hash(config, hash)?;
    if instalado.id != id {
        return Err("conteudo-de-outro-mod".to_owned());
    }
    Ok(seele_server::persistence::mods::EnabledMod {
        id: instalado.id,
        version: instalado.version,
        hash: instalado.hash,
        repo: instalado.repo,
        reach: instalado.reach,
        server_half: instalado.server,
        api: instalado.api,
    })
}

/// A saída que quem hospeda escolheu para um MOD de API antiga (decisão do
/// dono de 08/10). A saída 2, hospedar pelo Servidor 0.15, não mexe neste
/// banco: é o painel de versões.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SaidaDoModAntigo {
    /// A saída 1: liga os bytes deste hash, a versão do MOD feita para a API
    /// deste build, já no cache (a janela a baixa do catálogo antes).
    Atualizar { hash: String },
    /// A saída 3: desliga o MOD. A linha e os dados ficam (ADR 0045).
    Desligar,
}

/// Por que uma saída de quem hospeda não mexeu no banco. A frase mora em
/// `frases.js`, pela mesma razão de [`FalhaAoInstalarMod`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum FalhaAoResolverModAntigo {
    /// Esta janela está hospedando. O banco é do servidor que está de pé, e
    /// mudar a linha por baixo dele seria o servidor exigir um conjunto e o
    /// banco dizer outro (o mesmo limite do `apagar_servidor`).
    ServidorNoAr,
    /// O servidor pedido não está entre os guardados desta máquina.
    ServidorDesconhecido,
    /// Este MOD não está ligado com uma API antiga neste servidor: já foi
    /// resolvido, ou nunca foi. As saídas não são a gestão de MODs.
    NaoEDeApiAntiga,
    /// Os bytes da versão nova não servem, com o nome da recusa
    /// ([`linha_do_pacote`]).
    PacoteNaoServe(String),
    /// O banco não abriu, não migrou ou não gravou, com o que ele disse.
    BancoNaoRespondeu(String),
}

/// **As saídas 1 e 3 de quem hospeda com um MOD de API antiga, com o servidor
/// parado** (tarefa 13 do plano da F3, Step 5b; aprovado pela orquestradora em
/// 15/10, com os limites que esta função confere, nesta ordem):
///
/// 1. **só com o servidor parado** — `no_ar` vem da janela, e com ele de pé
///    nada é aberto;
/// 2. **só os servidores guardados desta máquina** — um id fora da lista é
///    recusado, e não cai no banco de sempre como `servidores::banco` cai;
/// 3. **só um MOD que hospedar recusaria** — pela mesma conferência do
///    `hospedar` ([`mods_de_api_antiga`]);
/// 4. **o mesmo banco e a mesma migração da subida** —
///    [`crate::servidores::banco_de_hospedar`] e `Persistence::open`;
/// 5. **os dados do MOD ficam** — `enable` e `disable` não tocam `mod_data`
///    (ADR 0045), e a versão nova lê o quintal que a antiga gravou;
/// 6. **o `seele.log` diz o que mudou e em qual servidor.**
pub(crate) fn resolver_mod_de_api_antiga(
    config: &str,
    servidor: Option<&str>,
    id: &str,
    saida: &SaidaDoModAntigo,
    no_ar: bool,
) -> Result<(), FalhaAoResolverModAntigo> {
    if no_ar {
        return Err(FalhaAoResolverModAntigo::ServidorNoAr);
    }
    if let Some(guardado) = servidor {
        if !crate::servidores::listar(config)
            .iter()
            .any(|s| s.id == guardado)
        {
            return Err(FalhaAoResolverModAntigo::ServidorDesconhecido);
        }
    }
    let banco = crate::servidores::banco_de_hospedar(config, servidor);
    let raizes = crate::servidores::raizes_dos_mods(config, servidor);
    let Some(antigo) = mods_de_api_antiga(&banco, &raizes)
        .into_iter()
        .find(|antigo| antigo.id == id)
    else {
        return Err(FalhaAoResolverModAntigo::NaoEDeApiAntiga);
    };
    // Os bytes novos conferidos **antes** de abrir o banco: abrir migra, e uma
    // recusa não deixa um banco migrado para trás.
    let nova = match saida {
        SaidaDoModAntigo::Atualizar { hash } => Some(
            linha_do_pacote(config, id, hash).map_err(FalhaAoResolverModAntigo::PacoteNaoServe)?,
        ),
        SaidaDoModAntigo::Desligar => None,
    };
    let banco_aberto = seele_server::persistence::Persistence::open(
        &seele_server::persistence::Location::File(banco),
    )
    .map_err(|erro| FalhaAoResolverModAntigo::BancoNaoRespondeu(format!("{erro:#}")))?;
    let gravado = match &nova {
        Some(linha) => seele_server::persistence::mods::enable(&banco_aberto, linha),
        None => seele_server::persistence::mods::disable(&banco_aberto, id),
    };
    gravado.map_err(|erro| FalhaAoResolverModAntigo::BancoNaoRespondeu(format!("{erro:#}")))?;

    let qual = servidor.unwrap_or("o de sempre");
    let api = antigo
        .api
        .map_or_else(|| "não lida".to_owned(), |api| api.to_string());
    match &nova {
        Some(linha) => tracing::info!(
            servidor = qual,
            mod_id = id,
            api_antiga = %api,
            versao = %linha.version,
            api = linha.api,
            "MOD de API antiga atualizado com o servidor parado"
        ),
        None => tracing::info!(
            servidor = qual,
            mod_id = id,
            api_antiga = %api,
            "MOD de API antiga desligado com o servidor parado; os dados dele ficam"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod mods_de_api_antiga {
    use super::{mods_de_api_antiga, ModDeApiAntiga};
    use std::path::Path;

    const PASTA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testes/pasta-da-0.15.1/pasta");

    fn copiar(de: &Path, para: &Path) {
        std::fs::create_dir_all(para).unwrap();
        for entrada in std::fs::read_dir(de).unwrap() {
            let entrada = entrada.unwrap();
            let destino = para.join(entrada.file_name());
            if entrada.path().is_dir() {
                copiar(&entrada.path(), &destino);
            } else {
                std::fs::copy(entrada.path(), destino).unwrap();
            }
        }
    }

    fn raizes(dir: &Path) -> seele_server::RaizesDosMods {
        seele_server::RaizesDosMods {
            pacotes: dir.to_path_buf(),
            dados: dir.join("mod-data"),
        }
    }

    fn esquema(banco: &Path) -> i64 {
        rusqlite::Connection::open(banco)
            .unwrap()
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |l| l.get(0),
            )
            .unwrap()
    }

    /// **A pasta da 0.15.1, sem migrar**: o `seele/estilo` 3.1.0 sai como da
    /// API 4, e nunca zero; e o banco continua no esquema 13 depois (conferir
    /// não migra). Lida a coluna `api`, que o 13 não tem, isto reprova.
    #[test]
    fn a_pasta_da_0_15_1_sem_migrar_diz_o_estilo_da_api_4_e_continua_no_13() {
        let dir = tempfile::tempdir().unwrap();
        let banco = dir.path().join("seele.db");
        rusqlite::Connection::open(&banco)
            .unwrap()
            .execute_batch(&std::fs::read_to_string(format!("{PASTA}/seele.sql")).unwrap())
            .unwrap();
        copiar(
            &Path::new(PASTA).join("mod-packages"),
            &dir.path().join("mod-packages"),
        );
        let antes = esquema(&banco);
        assert_eq!(antes, 13, "a pasta da 0.15.1 não está no esquema 13");

        assert_eq!(
            mods_de_api_antiga(&banco, &raizes(dir.path())),
            vec![ModDeApiAntiga {
                id: "seele/estilo".to_owned(),
                api: Some(4)
            }]
        );
        assert_eq!(esquema(&banco), antes, "conferir migrou o banco");
    }

    /// **O pacote que não está nesta máquina** sai como `None`, e não como
    /// «API 0»: a tela diz que o pacote não está aqui.
    #[test]
    fn uma_linha_ligada_sem_o_pacote_no_disco_sai_sem_api() {
        let dir = tempfile::tempdir().unwrap();
        let banco = dir.path().join("seele.db");
        rusqlite::Connection::open(&banco)
            .unwrap()
            .execute_batch(
                "CREATE TABLE mods (id TEXT PRIMARY KEY NOT NULL, version TEXT NOT NULL, \
                 hash TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1); \
                 INSERT INTO mods VALUES ('x/sumido', '1.0.0', 'ab', 1); \
                 INSERT INTO mods VALUES ('x/desligado', '1.0.0', 'cd', 0);",
            )
            .unwrap();
        assert_eq!(
            mods_de_api_antiga(&banco, &raizes(dir.path())),
            vec![ModDeApiAntiga {
                id: "x/sumido".to_owned(),
                api: None
            }],
            "a linha ligada sem pacote não saiu como «sem API», ou a desligada foi conferida"
        );
    }

    /// Sem banco, nenhum MOD — e o banco não é criado.
    #[test]
    fn sem_banco_nao_ha_o_que_conferir_e_nada_e_criado() {
        let dir = tempfile::tempdir().unwrap();
        let banco = dir.path().join("seele.db");
        assert!(mods_de_api_antiga(&banco, &raizes(dir.path())).is_empty());
        assert!(!banco.exists(), "conferir criou um banco");
    }
}

/// **As saídas 1 e 3 de quem hospeda, com o servidor parado** (tarefa 13 do
/// plano da F3, Step 5b; os limites da orquestradora de 15/10). Sobre a pasta
/// da 0.15.1: o `seele/estilo` 3.1.0, da API 4, e o quintal que ele gravou.
#[cfg(test)]
mod saidas_do_mod_antigo {
    use super::{
        instalar_de, mods_de_api_antiga, resolver_mod_de_api_antiga, FalhaAoResolverModAntigo,
        SaidaDoModAntigo,
    };
    use std::path::Path;

    const PASTA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testes/pasta-da-0.15.1/pasta");

    fn copiar(de: &Path, para: &Path) {
        std::fs::create_dir_all(para).unwrap();
        for entrada in std::fs::read_dir(de).unwrap() {
            let entrada = entrada.unwrap();
            let destino = para.join(entrada.file_name());
            if entrada.path().is_dir() {
                copiar(&entrada.path(), &destino);
            } else {
                std::fs::copy(entrada.path(), destino).unwrap();
            }
        }
    }

    /// A pasta da 0.15.1 como pasta de configuração, sem migrar: o banco de
    /// sempre (`banco_do_cliente`) e os pacotes no cache por hash.
    fn pasta_da_0_15_1() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        rusqlite::Connection::open(dir.path().join("seele.db"))
            .unwrap()
            .execute_batch(&std::fs::read_to_string(format!("{PASTA}/seele.sql")).unwrap())
            .unwrap();
        copiar(
            &Path::new(PASTA).join("mod-packages"),
            &dir.path().join("mod-packages"),
        );
        let config = dir.path().to_string_lossy().into_owned();
        (dir, config)
    }

    fn banco(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("seele.db")
    }

    fn esquema(banco: &Path) -> i64 {
        rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |l| l.get(0),
            )
            .unwrap()
    }

    /// A linha do `seele/estilo` em `mods`: versão, hash e ligado.
    fn linha_do_estilo(banco: &Path) -> (String, String, i64) {
        rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .query_row(
                "SELECT version, hash, enabled FROM mods WHERE id = 'seele/estilo'",
                [],
                |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
            )
            .unwrap()
    }

    /// O quintal do `seele/estilo`, linha por linha, como o disco o tem.
    fn quintal_do_estilo(banco: &Path) -> Vec<(String, Vec<u8>)> {
        let conexao = rusqlite::Connection::open_with_flags(
            banco,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut consulta = conexao
            .prepare("SELECT key, CAST(value AS BLOB) FROM mod_data WHERE mod_id = 'seele/estilo' ORDER BY key")
            .unwrap();
        consulta
            .query_map([], |l| Ok((l.get(0)?, l.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// O esquema que a subida deste build deixa: o que `Persistence::open` migra.
    fn esquema_deste_build() -> i64 {
        seele_server::persistence::Persistence::open(&seele_server::persistence::Location::Memory)
            .unwrap()
            .schema_version()
            .unwrap()
    }

    /// O `seele/estilo` 4.0.0, na API deste build, instalado no cache por hash.
    fn estilo_4_no_cache(config: &str) -> String {
        let origem = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(origem.path().join("servidor")).unwrap();
        std::fs::write(
            origem.path().join("mod.json"),
            serde_json::json!({
                "schema": 1,
                "id": "seele/estilo",
                "version": "4.0.0",
                "api": seele_ffi::mods::MOD_API_VERSION,
                "repo": "https://github.com/DATA-AND-DEV/ESTILO",
                "reach": ["tema do servidor"],
                "server": "servidor/main.js",
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(origem.path().join("servidor/main.js"), "/* nada */").unwrap();
        instalar_de(Path::new(config), origem.path()).unwrap().hash
    }

    /// **A saída 3: desligar, com o servidor parado.** A linha fica e passa a
    /// desligada, o quintal fica byte a byte (ADR 0045), o banco migra pelo
    /// mesmo `Persistence::open` da subida, e o `seele.log` diz o que mudou e
    /// em qual servidor.
    #[test]
    fn desligar_com_o_servidor_parado_desliga_e_o_quintal_fica() {
        let (dir, config) = pasta_da_0_15_1();
        let quintal = quintal_do_estilo(&banco(&dir));
        assert!(
            !quintal.is_empty(),
            "a pasta da 0.15.1 não tem quintal do estilo"
        );

        let (feito, rastro) = crate::rastro_de_teste::capturar(|| {
            resolver_mod_de_api_antiga(
                &config,
                None,
                "seele/estilo",
                &SaidaDoModAntigo::Desligar,
                false,
            )
        });
        assert_eq!(feito, Ok(()));

        let (versao, _, ligado) = linha_do_estilo(&banco(&dir));
        assert_eq!(
            (versao.as_str(), ligado),
            ("3.1.0", 0),
            "a linha não ficou, desligada"
        );
        assert_eq!(
            quintal_do_estilo(&banco(&dir)),
            quintal,
            "desligar mexeu no quintal"
        );
        assert_eq!(
            esquema(&banco(&dir)),
            esquema_deste_build(),
            "o banco não migrou como na subida"
        );
        assert!(
            mods_de_api_antiga(
                &banco(&dir),
                &crate::servidores::raizes_dos_mods(&config, None)
            )
            .is_empty(),
            "depois de desligar, hospedar ainda recusaria"
        );
        assert!(
            rastro.contains("seele/estilo")
                && rastro.contains("desligado")
                && rastro.contains("o de sempre"),
            "o seele.log não diz o que mudou e em qual servidor: {rastro}"
        );
    }

    /// **A saída 1: atualizar, com o servidor parado.** A linha passa aos bytes
    /// do 4.0.0, ligada, e o quintal fica: é o que a metade nova vai ler.
    #[test]
    fn atualizar_com_o_servidor_parado_liga_a_versao_nova_e_o_quintal_fica() {
        let (dir, config) = pasta_da_0_15_1();
        let quintal = quintal_do_estilo(&banco(&dir));
        let hash = estilo_4_no_cache(&config);

        let (feito, rastro) = crate::rastro_de_teste::capturar(|| {
            resolver_mod_de_api_antiga(
                &config,
                None,
                "seele/estilo",
                &SaidaDoModAntigo::Atualizar { hash: hash.clone() },
                false,
            )
        });
        assert_eq!(feito, Ok(()));

        assert_eq!(
            linha_do_estilo(&banco(&dir)),
            ("4.0.0".to_owned(), hash, 1),
            "a linha não passou ao 4.0.0, ligada"
        );
        assert_eq!(
            quintal_do_estilo(&banco(&dir)),
            quintal,
            "atualizar mexeu no quintal"
        );
        assert!(
            mods_de_api_antiga(
                &banco(&dir),
                &crate::servidores::raizes_dos_mods(&config, None)
            )
            .is_empty(),
            "depois de atualizar, hospedar ainda recusaria"
        );
        assert!(
            rastro.contains("seele/estilo")
                && rastro.contains("4.0.0")
                && rastro.contains("o de sempre"),
            "o seele.log não diz o que mudou e em qual servidor: {rastro}"
        );
    }

    /// **Com o servidor no ar, recusa, e o banco não é tocado**: nem migrado,
    /// nem a linha mudada. O banco é do servidor que está de pé.
    #[test]
    fn com_o_servidor_no_ar_recusa_e_o_banco_nao_e_tocado() {
        let (dir, config) = pasta_da_0_15_1();
        let antes = linha_do_estilo(&banco(&dir));
        for saida in [
            SaidaDoModAntigo::Desligar,
            SaidaDoModAntigo::Atualizar {
                hash: estilo_4_no_cache(&config),
            },
        ] {
            assert_eq!(
                resolver_mod_de_api_antiga(&config, None, "seele/estilo", &saida, true),
                Err(FalhaAoResolverModAntigo::ServidorNoAr),
                "com o servidor no ar, {saida:?} não foi recusada"
            );
        }
        assert_eq!(
            esquema(&banco(&dir)),
            13,
            "com o servidor no ar, o banco migrou"
        );
        assert_eq!(
            linha_do_estilo(&banco(&dir)),
            antes,
            "com o servidor no ar, a linha mudou"
        );
    }

    /// **Só os servidores guardados desta máquina.** Um id que a lista não tem
    /// é recusado — e o banco de sempre, que é para onde `servidores::banco`
    /// cai com um id desconhecido, não é tocado.
    #[test]
    fn um_servidor_que_esta_maquina_nao_guarda_e_recusado_sem_tocar_o_banco_de_sempre() {
        let (dir, config) = pasta_da_0_15_1();
        let antes = linha_do_estilo(&banco(&dir));
        assert_eq!(
            resolver_mod_de_api_antiga(
                &config,
                Some("nao-existe"),
                "seele/estilo",
                &SaidaDoModAntigo::Desligar,
                false
            ),
            Err(FalhaAoResolverModAntigo::ServidorDesconhecido)
        );
        assert_eq!(esquema(&banco(&dir)), 13, "o banco de sempre migrou");
        assert_eq!(
            linha_do_estilo(&banco(&dir)),
            antes,
            "o banco de sempre mudou"
        );
    }

    /// **Só um MOD de API antiga**: as saídas não são a gestão de MODs. Um que
    /// não está ligado com API antiga é recusado, sem tocar o banco.
    #[test]
    fn um_mod_que_nao_e_de_api_antiga_e_recusado_sem_tocar_o_banco() {
        let (dir, config) = pasta_da_0_15_1();
        assert_eq!(
            resolver_mod_de_api_antiga(
                &config,
                None,
                "x/outro",
                &SaidaDoModAntigo::Desligar,
                false
            ),
            Err(FalhaAoResolverModAntigo::NaoEDeApiAntiga)
        );
        assert_eq!(esquema(&banco(&dir)), 13, "recusado, e o banco migrou");
    }

    /// **Atualizar para os bytes de outro MOD é recusado** pela mesma regra da
    /// gestão (`linha_do_pacote`), antes de abrir o banco.
    #[test]
    fn atualizar_com_o_pacote_de_outro_mod_e_recusado_sem_tocar_o_banco() {
        let (dir, config) = pasta_da_0_15_1();
        let origem = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(origem.path().join("cliente")).unwrap();
        std::fs::write(
            origem.path().join("mod.json"),
            super::manifesto_de_teste("x/outro"),
        )
        .unwrap();
        std::fs::write(origem.path().join("cliente/main.js"), "/* nada */").unwrap();
        let hash = instalar_de(dir.path(), origem.path()).unwrap().hash;
        assert_eq!(
            resolver_mod_de_api_antiga(
                &config,
                None,
                "seele/estilo",
                &SaidaDoModAntigo::Atualizar { hash },
                false
            ),
            Err(FalhaAoResolverModAntigo::PacoteNaoServe(
                "conteudo-de-outro-mod".to_owned()
            ))
        );
        assert_eq!(esquema(&banco(&dir)), 13, "recusado, e o banco migrou");
    }
}

#[cfg(test)]
mod instalar {
    use super::{instalar_de, manifesto_de_teste, FalhaAoInstalarMod};

    fn pasta(nome: &str) -> std::path::PathBuf {
        let caminho = std::env::temp_dir().join(format!(
            "seele-instalar-mod-{}-{}-{nome}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&caminho).unwrap();
        caminho
    }

    /// Um pacote de MOD de mentira, válido. `extra` muda os bytes sem mudar o
    /// identificador — que é como dois pacotes do mesmo MOD existem.
    fn pacote(raiz: &std::path::Path, id: &str, extra: Option<&str>) -> std::path::PathBuf {
        let origem = raiz.join("origem");
        std::fs::create_dir_all(origem.join("cliente")).unwrap();
        std::fs::write(origem.join("mod.json"), manifesto_de_teste(id)).unwrap();
        std::fs::write(origem.join("cliente/main.js"), b"globalThis.x = 1;").unwrap();
        if let Some(conteudo) = extra {
            std::fs::write(origem.join("extra.txt"), conteudo).unwrap();
        }
        origem
    }

    #[test]
    fn um_pacote_bom_chega_inteiro_e_vai_para_a_pasta_do_conteudo() {
        let raiz = pasta("bom");
        let origem = pacote(&raiz, "autor/exemplo", None);
        let config = raiz.join("config");

        let publicado = instalar_de(&config, &origem).expect("um pacote válido tem de instalar");

        assert_eq!(
            publicado.id, "autor/exemplo",
            "o identificador não saiu do manifesto"
        );
        assert_eq!(publicado.hash.len(), 64, "{}", publicado.hash);
        let destino = config.join(seele_ffi::mods::PACOTES).join(&publicado.hash);
        assert!(
            destino.join("cliente/main.js").is_file(),
            "o pacote não chegou inteiro à pasta do conteúdo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Instalar o mesmo pacote de novo não é erro, e não duplica nada.**
    ///
    /// Antes havia um lugar por identificador, e instalar por cima era uma
    /// recusa — `JaInstalado`. Endereçado pelo conteúdo, a pergunta desaparece:
    /// bytes iguais são a mesma pasta, e responder erro a quem pediu
    /// exatamente o que já está lá seria inventar um problema.
    #[test]
    fn instalar_o_mesmo_pacote_de_novo_e_o_mesmo_lugar_e_nao_e_erro() {
        let raiz = pasta("de-novo");
        let origem = pacote(&raiz, "autor/exemplo", None);
        let config = raiz.join("config");

        let um = instalar_de(&config, &origem).expect("primeira");
        let outro = instalar_de(&config, &origem).expect("segunda");

        assert_eq!(um, outro, "o mesmo conteúdo foi para dois lugares");
        let quantos = std::fs::read_dir(config.join(seele_ffi::mods::PACOTES))
            .unwrap()
            .count();
        assert_eq!(quantos, 1, "o mesmo pacote foi guardado duas vezes");
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Dois conteúdos do mesmo MOD convivem**, que é o P1 que este layout
    /// conserta: por identificador, instalar a versão que um servidor exige
    /// apagava a que o outro exigia.
    #[test]
    fn dois_conteudos_do_mesmo_mod_ficam_lado_a_lado() {
        let raiz = pasta("dois");
        let config = raiz.join("config");

        let um = instalar_de(&config, &pacote(&raiz, "autor/exemplo", None)).expect("primeiro");
        let _ = std::fs::remove_dir_all(raiz.join("origem"));
        let outro = instalar_de(
            &config,
            &pacote(&raiz, "autor/exemplo", Some("outra coisa")),
        )
        .expect("segundo");

        assert_eq!(um.id, outro.id);
        assert_ne!(um.hash, outro.hash, "bytes diferentes deram o mesmo hash");
        assert!(config
            .join(seele_ffi::mods::PACOTES)
            .join(&um.hash)
            .is_dir());
        assert!(config
            .join(seele_ffi::mods::PACOTES)
            .join(&outro.hash)
            .is_dir());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn cada_recusa_diz_qual_foi_e_nada_e_copiado() {
        let raiz = pasta("recusas");
        let config = raiz.join("config");

        let vazia = raiz.join("vazia");
        std::fs::create_dir_all(&vazia).unwrap();
        assert!(matches!(
            instalar_de(&config, &vazia),
            Err(FalhaAoInstalarMod::SemManifesto)
        ));
        assert!(
            !config.join(seele_ffi::mods::PACOTES).exists(),
            "uma recusa deixou pacote no cache"
        );

        let torto = raiz.join("torto");
        std::fs::create_dir_all(&torto).unwrap();
        std::fs::write(torto.join("mod.json"), b"{isto nao e json}").unwrap();
        assert!(matches!(
            instalar_de(&config, &torto),
            Err(FalhaAoInstalarMod::ManifestoRecusado(_))
        ));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **O atalho é recusado com o nome dele**, e não como uma falha do disco.
    ///
    /// Um link simbólico faria o `mod://` servir à janela um arquivo de outro
    /// lugar do disco, como se fosse do MOD — a recusa está certa. O que a
    /// pendência 53 consertou é ela chegar à tela como `NaoCopiei`, junto das
    /// falhas do disco, e a tela não ter como dizer qual era o atalho.
    // **Só onde há atalho.** O Windows não tem `std::os::unix::fs::symlink`.
    #[cfg(unix)]
    #[test]
    fn um_atalho_dentro_do_pacote_nao_e_instalado() {
        let raiz = pasta("atalho");
        let origem = pacote(&raiz, "autor/exemplo", None);
        let segredo = raiz.join("segredo.txt");
        std::fs::write(&segredo, b"nao sou deste mod").unwrap();
        let atalho = origem.join("atalho.txt");
        std::os::unix::fs::symlink(&segredo, &atalho).unwrap();
        let config = raiz.join("config");

        match instalar_de(&config, &origem) {
            Err(FalhaAoInstalarMod::TemAtalho(caminho)) => assert_eq!(
                caminho,
                atalho.display().to_string(),
                "a recusa do atalho não diz qual é ele, e quem lê a tela não sabe o que tirar"
            ),
            outra => panic!(
                "um atalho dentro do pacote não foi recusado como atalho, e a tela não tem \
                 como dizer qual é ele: {outra:?}"
            ),
        }
        assert!(
            !config.join(seele_ffi::mods::PACOTES).exists()
                || std::fs::read_dir(config.join(seele_ffi::mods::PACOTES))
                    .unwrap()
                    .count()
                    == 0,
            "o pacote com atalho foi publicado assim mesmo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A recusa do atalho deixa linha no `seele.log`, com o caminho dele.**
    ///
    /// Sem ela, quem relata «não instala» chega sem dado nenhum: a tela diz o
    /// atalho a quem está na frente dela, e o registro é o que chega a quem
    /// conserta.
    #[cfg(unix)]
    #[test]
    fn a_recusa_do_atalho_fica_no_registro_com_o_caminho() {
        let raiz = pasta("atalho-no-registro");
        let origem = pacote(&raiz, "autor/exemplo", None);
        std::fs::create_dir_all(origem.join("node_modules/.bin")).unwrap();
        let atalho = origem.join("node_modules/.bin/playwright-core");
        std::os::unix::fs::symlink("/etc/hosts", &atalho).unwrap();
        let config = raiz.join("config");

        let (resultado, rastro) =
            crate::rastro_de_teste::capturar(|| instalar_de(&config, &origem));

        assert!(
            matches!(resultado, Err(FalhaAoInstalarMod::TemAtalho(_))),
            "o atalho não foi recusado como atalho: {resultado:?}"
        );
        let caminho = atalho.display().to_string();
        assert!(
            rastro
                .lines()
                .any(|linha| linha.contains("WARN") && linha.contains(&caminho)),
            "a recusa do atalho não deixou linha WARN com o caminho dele no `seele.log`, e \
             quem relata «não instala» chega sem dado nenhum:\n{rastro}"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **E toda recusa deixa a linha, com a pasta de onde se tentou instalar.**
    ///
    /// As que não dependem de atalho, para rodar em todo sistema.
    #[test]
    fn toda_recusa_de_instalar_fica_no_registro_com_a_pasta() {
        let raiz = pasta("recusas-no-registro");
        let config = raiz.join("config");

        let vazia = raiz.join("vazia");
        std::fs::create_dir_all(&vazia).unwrap();
        let torto = raiz.join("torto");
        std::fs::create_dir_all(&torto).unwrap();
        std::fs::write(torto.join("mod.json"), b"{isto nao e json}").unwrap();

        for origem in [vazia, torto] {
            let (resultado, rastro) =
                crate::rastro_de_teste::capturar(|| instalar_de(&config, &origem));
            assert!(resultado.is_err(), "{} instalou", origem.display());
            // **A forma que o registro escreve, e não `display()`.** A pasta
            // vai escapada, e no Windows o escape dobra a barra invertida:
            // comparar com `display()` reprovaria lá com o registro certo.
            let caminho = crate::caminho_no_registro(&origem.to_string_lossy());
            assert!(
                rastro
                    .lines()
                    .any(|linha| linha.contains("WARN") && linha.contains(&caminho)),
                "uma recusa de instalar não deixou linha WARN com a pasta no `seele.log`: \
                 {resultado:?}\n{rastro}"
            );
        }
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Uma pasta com quebra de linha no nome não forja linha no `seele.log`.**
    ///
    /// O nome da pasta vem de quem empacotou o MOD, e o formatador do registro
    /// não escapa `\n` nem `\r` num campo `%`: a pasta abaixo escrevia uma
    /// segunda linha com a cara do produto, e quem lê o registro não tinha como
    /// saber que ela era forjada. A pasta vai ao registro por
    /// `caminho_no_registro`, entre aspas e com escape, como o resto da casca.
    // **Só onde um nome de pasta aceita `\n`.** O Windows não aceita.
    #[cfg(unix)]
    #[test]
    fn uma_pasta_com_quebra_de_linha_no_nome_nao_forja_linha_no_registro() {
        const FORJADA: &str = "2026-10-08T00:00:00.000000Z  INFO seele_app: tudo certo";
        let raiz = pasta("forjada");
        let origem = raiz.join(format!("mod\n{FORJADA}"));
        std::fs::create_dir_all(&origem).unwrap();
        let config = raiz.join("config");

        let (resultado, rastro) =
            crate::rastro_de_teste::capturar(|| instalar_de(&config, &origem));

        assert!(
            matches!(resultado, Err(FalhaAoInstalarMod::SemManifesto)),
            "a pasta sem `mod.json` não foi recusada como tal: {resultado:?}"
        );
        assert!(
            !rastro
                .lines()
                .any(|linha| linha.trim_start().starts_with(FORJADA)),
            "o nome da pasta escreveu no `seele.log` uma linha com a cara do produto, e quem \
             lê o registro não tem como saber que ela é forjada:\n{rastro}"
        );
        let escapada = crate::caminho_no_registro(&origem.to_string_lossy());
        assert!(
            rastro
                .lines()
                .any(|linha| linha.contains("WARN") && linha.contains(&escapada)),
            "a recusa não ficou no `seele.log` com a pasta escapada, {escapada}:\n{rastro}"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A10 da auditoria: uma cópia que falha não deixa nada pela metade.**
    ///
    /// A falha é injetada com um arquivo que ninguém pode ler, dentro de uma
    /// subpasta: `copiar_arvore` tropeça nele **depois** de já ter criado
    /// diretório. A leitura do manifesto não tropeça, porque o hash pula o que
    /// não abre.
    ///
    /// **Era um atalho, e deixou de ser** com a pendência 53: o atalho ganhou
    /// recusa própria, `TemAtalho`, e este teste é sobre as outras falhas da
    /// cópia, que continuam `NaoCopiei`.
    // **Só onde há permissão de Unix.** O Windows não tem `PermissionsExt`.
    #[cfg(unix)]
    #[test]
    fn uma_copia_que_falha_no_meio_nao_deixa_pacote_pela_metade() {
        use std::os::unix::fs::PermissionsExt as _;

        let raiz = pasta("meia-copia");
        let origem = pacote(&raiz, "autor/exemplo", None);
        std::fs::create_dir_all(origem.join("extra")).unwrap();
        let trancado = origem.join("extra/trancado.txt");
        std::fs::write(&trancado, b"ninguem le").unwrap();
        std::fs::set_permissions(&trancado, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            std::fs::read(&trancado).is_err(),
            "o arquivo trancado ainda se lê (este teste roda como root?), e sem a falha \
             injetada a cópia termina e o teste não prova nada"
        );
        let config = raiz.join("config");

        let resultado = instalar_de(&config, &origem);
        assert!(
            matches!(resultado, Err(FalhaAoInstalarMod::NaoCopiei(_))),
            "uma falha da cópia que não é atalho não chegou como `NaoCopiei`: {resultado:?}"
        );
        assert!(
            !config.join(seele_ffi::mods::PACOTES).exists()
                || std::fs::read_dir(config.join(seele_ffi::mods::PACOTES))
                    .unwrap()
                    .count()
                    == 0,
            "a cópia que falhou publicou um pacote assim mesmo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **E uma cópia que falha não alcança o que já estava publicado.**
    ///
    /// Antes isto era uma dança de dois `rename` com volta atrás, porque o
    /// destino era o mesmo lugar. Agora é uma propriedade do layout: pacotes
    /// diferentes são pastas diferentes, e uma instalação que falha não tem
    /// como tocar na de outro.
    #[cfg(unix)]
    #[test]
    fn uma_copia_que_falha_nao_toca_no_que_ja_estava_publicado() {
        let raiz = pasta("nao-toca");
        let config = raiz.join("config");
        let bom = instalar_de(&config, &pacote(&raiz, "autor/exemplo", None)).expect("o bom");
        let _ = std::fs::remove_dir_all(raiz.join("origem"));

        let torto = pacote(&raiz, "autor/exemplo", Some("outro"));
        std::fs::create_dir_all(torto.join("extra")).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", torto.join("extra/atalho")).unwrap();
        assert!(instalar_de(&config, &torto).is_err());

        assert!(
            config
                .join(seele_ffi::mods::PACOTES)
                .join(&bom.hash)
                .join("cliente/main.js")
                .is_file(),
            "a instalação que falhou levou junto o pacote que já estava lá"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// E o caminho feliz não deixa rastro: uma estufa esquecida é disco ocupado
    /// que ninguém sabe explicar.
    #[test]
    fn uma_instalacao_que_deu_certo_nao_deixa_estufa() {
        let raiz = pasta("limpo");
        let origem = pacote(&raiz, "autor/exemplo", None);
        let config = raiz.join("config");

        instalar_de(&config, &origem).expect("instala");

        assert!(!config.join(".mods-em-obras/autor/exemplo").exists());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Quem já tinha MOD instalado não o perde.**
    ///
    /// Até a v0.11.x o pacote morava em `mods/<autor>/<nome>/`, e a leitura
    /// passou a ser por conteúdo. Sem a mudança, os três MODs oficiais de uma
    /// máquina em uso sumiriam da lista — e quem usa leria isso como o produto
    /// ter perdido o que estava instalado, que é o pior jeito de mudar um
    /// layout.
    /// **Apagar um pacote tira o pacote, e nada mais.**
    ///
    /// Plano de isolamento de 18/09: «apagar cache de convidado não apaga dados
    /// remotos». O que um MOD guardou não vive no cache — vive no banco do
    /// servidor e na raiz daquela instância —, e bytes que se baixa de novo não
    /// podem levar junto o que ninguém tem como recuperar.
    ///
    /// O teste põe as duas coisas lado a lado no mesmo `config_dir` e confere
    /// que só uma saiu.
    #[test]
    fn apagar_um_pacote_nao_encosta_no_que_um_mod_guardou() {
        let raiz = pasta("apagar");
        let config = raiz.join("config");
        let origem = pacote(&raiz, "seele/exemplo", None);
        let publicado = instalar_de(&config, &origem).expect("instalar");

        // O que um MOD guardou, na raiz da instância — o vizinho de `mod-packages`.
        let dados = config.join("mod-data").join("seele").join("exemplo");
        std::fs::create_dir_all(&dados).unwrap();
        std::fs::write(dados.join("retrato.png"), b"nao me apague").unwrap();

        super::apagar_do_cache(&config, &publicado.hash).expect("apagar");

        assert!(
            !config
                .join(seele_ffi::mods::PACOTES)
                .join(&publicado.hash)
                .exists(),
            "o pacote continua no cache"
        );
        assert_eq!(
            std::fs::read(dados.join("retrato.png")).expect("os dados do MOD"),
            b"nao me apague",
            "apagar o cache levou junto o que o MOD guardou"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Um hash que não tem forma de hash não apaga nada.**
    ///
    /// O hash chega da janela. Sem esta trava, `../..` seria um caminho para
    /// apagar o que alguém escolheu em vez de um pacote — e o teste prova isso
    /// deixando um arquivo no caminho que a travessia alcançaria.
    #[test]
    fn um_hash_torto_nao_apaga_o_que_esta_fora_do_cache() {
        let raiz = pasta("travessia");
        let config = raiz.join("config");
        // **O cache existe.** Sem ele, `mod-packages/..` não resolve e a recusa
        // sairia por «não está aqui» — o teste passaria sem a trava, que é o
        // guarda vacuoso que a reversão desta prova encontrou.
        std::fs::create_dir_all(config.join(seele_ffi::mods::PACOTES)).unwrap();
        std::fs::write(config.join("seele.db"), b"o banco").unwrap();

        for torto in [
            "..",
            "../..",
            "../seele.db",
            "..%2F..",
            "",
            "ABCDEF",
            &"g".repeat(64),
            &"a".repeat(63),
        ] {
            assert_eq!(
                super::apagar_do_cache(&config, torto),
                Err(super::FalhaAoApagarPacote::HashTorto),
                "`{torto}` não foi recusado pela forma"
            );
        }
        assert!(
            config.join("seele.db").is_file(),
            "a travessia alcançou o que está fora do cache"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// E um pacote que não está aqui é **dito**, e não engolido: apagar duas
    /// vezes da mesma tela é apagar o que a outra aba já apagou.
    #[test]
    fn apagar_o_que_nao_esta_aqui_e_dito() {
        let raiz = pasta("ausente");
        let config = raiz.join("config");
        std::fs::create_dir_all(&config).unwrap();
        assert_eq!(
            super::apagar_do_cache(&config, &"a".repeat(64)),
            Err(super::FalhaAoApagarPacote::NaoEstaAqui)
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_pacote_do_layout_antigo_e_levado_para_o_cache() {
        let raiz = pasta("migracao");
        let config = raiz.join("config");
        let antigo = config.join("mods").join("seele").join("exemplo");
        std::fs::create_dir_all(antigo.join("cliente")).unwrap();
        std::fs::write(
            antigo.join("mod.json"),
            manifesto_de_teste("seele/exemplo").as_bytes(),
        )
        .unwrap();
        std::fs::write(antigo.join("cliente/main.js"), b"globalThis.x = 1;").unwrap();
        let hash = seele_ffi::mods::ler_pasta(&antigo.to_string_lossy())
            .expect("ler")
            .hash;

        super::levar_pacotes_antigos_para_o_cache(&config);

        let destino = config.join(seele_ffi::mods::PACOTES).join(&hash);
        assert!(
            destino.join("cliente/main.js").is_file(),
            "o pacote do layout antigo não chegou ao cache"
        );
        // **Copiado, e não movido.** O pacote é imutável, então a segunda cópia
        // não diverge; e o layout antigo ainda é lido pela tela de gestão
        // enquanto a migração da interface não fecha. Apagar aqui a deixaria
        // mostrando o que já não existe.
        assert!(
            antigo.join("cliente/main.js").is_file(),
            "o layout antigo foi apagado antes de deixar de ser lido"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// E a segunda passagem não faz nada: o destino é o hash, e um pacote já
    /// levado já está lá.
    #[test]
    fn levar_os_pacotes_duas_vezes_nao_duplica_nada() {
        let raiz = pasta("migracao-dupla");
        let config = raiz.join("config");
        let antigo = config.join("mods").join("seele").join("exemplo");
        std::fs::create_dir_all(antigo.join("cliente")).unwrap();
        std::fs::write(
            antigo.join("mod.json"),
            manifesto_de_teste("seele/exemplo").as_bytes(),
        )
        .unwrap();
        std::fs::write(antigo.join("cliente/main.js"), b"globalThis.x = 1;").unwrap();

        super::levar_pacotes_antigos_para_o_cache(&config);
        super::levar_pacotes_antigos_para_o_cache(&config);

        let quantos = std::fs::read_dir(config.join(seele_ffi::mods::PACOTES))
            .unwrap()
            .count();
        assert_eq!(quantos, 1, "o mesmo pacote foi levado duas vezes");
        assert!(
            !config.join(".mods-em-obras").exists()
                || std::fs::read_dir(config.join(".mods-em-obras"))
                    .unwrap()
                    .count()
                    == 0,
            "a migração deixou estufa para trás"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
