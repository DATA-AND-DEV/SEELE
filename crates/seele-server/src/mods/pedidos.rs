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
            // O pedido do catálogo (id vazio) não é de MOD nenhum, e um
            // `mod_id=` em branco casaria com qualquer `grep "mod_id="`.
            let legivel = id_seguro(id);
            if id.is_empty() {
                tracing::warn!(catalogo = true, %error, "MOD request refused");
            } else if legivel.trim().is_empty() {
                // Um id que não é vazio, mas é feito só do que o `id_seguro`
                // tira (e de espaço): escrito, ele seria o mesmo `mod_id=` em
                // branco. A linha diz que ele não se lia.
                tracing::warn!(mod_id_ilegivel = true, %error, "MOD request refused");
            } else {
                // `%`, e não o `str` cru: cru, o `fmt` o escreve pelo `Debug`,
                // entre aspas, e o `grep "mod_id=autor/nome"` do guia não acha
                // a linha. É a grafia das outras linhas do MOD, nas duas
                // metades. Mas o `%` escreve cru, e este id vem do fio: é o
                // `id_seguro` que impede quem pediu de escrever uma linha.
                tracing::warn!(mod_id = %legivel, %error, "MOD request refused");
            }
            r#"{"ok":false,"error":"bridge-refused"}"#.into()
        }
    }
}

/// **O maior id de MOD que a linha da recusa leva**, em caracteres.
///
/// Um id de verdade (`autor/nome`) é bem menor; o teto é para o que chega pelo
/// fio, onde a única conferência do id é o tamanho. É o número do app
/// (`TETO_DO_ID_NO_REGISTRO`, em `apps/seele-app/src/main.rs`).
const TETO_DO_ID_NO_REGISTRO: usize = 128;

/// **O id de um pedido como a linha da recusa o leva**: cortado em
/// [`TETO_DO_ID_NO_REGISTRO`] e sem caractere que quebre ou inverta a linha
/// ([`quebra_ou_inverte_a_linha`]).
///
/// O id vem do fio, por dois caminhos que chegam a [`executar`]: o
/// `ClientMessage::ModRequest` e o pedido de imagem do fluxo de volume. Sem
/// aspas, porque `mod_id=autor/nome` é a grafia que o guia manda procurar: é
/// o filtro, e não o escape, que impede um `\n` no id de escrever uma segunda
/// linha com a cara do produto.
///
/// A regra é a de `id_no_registro` do app, reescrita aqui porque o servidor
/// não depende do app.
fn id_seguro(id: &str) -> String {
    id.chars()
        .take(TETO_DO_ID_NO_REGISTRO)
        .filter(|&c| !quebra_ou_inverte_a_linha(c))
        .collect()
}

/// **Um caractere que, escrito cru numa linha do `seele.log`, a quebra ou
/// reordena o que vem depois dele.**
///
/// Os de controle (`char::is_control`: C0, DEL e C1, com o U+0085 que outros
/// sistemas usam como quebra de linha); os separadores de linha e de parágrafo
/// (U+2028 e U+2029), que um editor quebra como um `\n`; e os de controle
/// bidirecional — a propriedade `Bidi_Control` do Unicode: U+061C, U+200E,
/// U+200F, U+202A a U+202E e U+2066 a U+2069. Um U+202E no id inverte, num
/// editor que segue o bidi, o `error=` que vem depois dele na mesma linha.
fn quebra_ou_inverte_a_linha(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}'
                | '\u{2029}'
                | '\u{061C}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
        )
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

    /// O que o `fmt` do `tracing` escreveu, guardado para o teste ler.
    #[derive(Clone, Default)]
    struct Registro(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Registro {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("o registro trancou")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Um servidor em memória, sem pasta de MODs e sem ninguém com permissão:
    /// todo pedido de MOD é recusado antes de chegar ao QuickJS, e a recusa é
    /// a linha que o teste lê.
    fn servidor_que_recusa() -> std::sync::Arc<crate::server::Server> {
        use crate::persistence::{Location, Persistence};
        use crate::server::{spawn_writer, Server, Telas};
        use std::sync::Arc;
        use tokio::sync::{broadcast, Mutex};

        let persistence = Arc::new(Mutex::new(
            Persistence::open(&Location::Memory).expect("banco em memória"),
        ));
        let (events, _) = broadcast::channel(64);
        let writes = spawn_writer(Arc::clone(&persistence), events.clone());
        Arc::new(Server {
            esperas: Arc::new(std::sync::Mutex::new(Default::default())),
            mods_em_curso: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            pacotes_conferidos: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            persistence,
            events,
            writes,
            slots: Arc::new(Mutex::new(crate::server::Slots::default())),
            occupancy: Arc::new(Mutex::new(crate::server::Occupancy::default())),
            presentes: Arc::new(Mutex::new(crate::server::Presentes::default())),
            subida: Arc::new(Mutex::new(crate::tela::Subida::nova())),
            portaria: Arc::new(Mutex::new(crate::taxa::Portaria::nova())),
            atrasos: Arc::new(crate::server::Atrasos::default()),
            desassentamentos: Arc::new(crate::server::Desassentamentos::default()),
            pares: Arc::new(Mutex::new(crate::pares::Pares::default())),
            versao_do_anuncio: seele_proto::mods::VERSAO_DO_ANUNCIO,
            telas: Arc::new(Mutex::new(Telas::default())),
            anexos: None,
            caminho_bps: None,
            mods_dir: None,
        })
    }

    /// Pede `id` a [`servidor_que_recusa`] e devolve **tudo** o que o
    /// registro recebeu enquanto isso, como o `fmt` de `seele.log` o escreve.
    ///
    /// O servidor é montado **antes** de o registro começar a ouvir: as linhas
    /// das migrações do banco não são da recusa, e um teste que conta linhas as
    /// contaria.
    async fn o_registro_da_recusa(id: &str) -> String {
        let servidor = servidor_que_recusa();
        let registro = Registro::default();
        let escritor = registro.clone();
        let assinante = tracing_subscriber::fmt()
            .with_writer(move || escritor.clone())
            .with_ansi(false)
            .without_time()
            .finish();
        let _guarda = tracing::subscriber::set_default(assinante);

        let resposta = super::executar(
            &servidor,
            seele_proto::ids::PersonId(7),
            seele_proto::ids::ChannelId(0),
            id,
            "{}",
        )
        .await;
        assert!(
            resposta.contains("bridge-refused"),
            "o pedido de quem não pode ler não foi recusado, e o teste não chegou à linha da \
             recusa: {resposta}"
        );

        let texto =
            String::from_utf8_lossy(&registro.0.lock().expect("o registro trancou")).into_owned();
        texto
    }

    /// **A recusa de um pedido de MOD escreve `mod_id=` como as outras linhas
    /// do MOD**, e o `grep` do guia a acha.
    ///
    /// `docs/como-se-faz-um-mod.md` («Ler a linha») manda procurar um MOD com
    /// `grep "mod_id=fulano/meu-mod"`, e diz que o que o servidor disse dele
    /// usa o mesmo campo. Esta linha — a que diz que um pedido do MOD foi
    /// recusado, inclusive quando o `aoPedir` dele lança — escrevia o id pelo
    /// `Debug` de um `str`, entre aspas (`mod_id="fulano/meu-mod"`), e o grep
    /// não a achava. As outras portas que escrevem `mod_id=` usam `%`, e a
    /// janela, `display` (FD-m3 da revisão do Lote F-Docs).
    #[tokio::test(flavor = "current_thread")]
    async fn a_recusa_do_pedido_escreve_o_mod_id_que_o_grep_do_guia_acha() {
        let texto = o_registro_da_recusa("fulano/meu-mod").await;
        let Some(linha) = texto
            .lines()
            .find(|linha| linha.contains("MOD request refused"))
        else {
            panic!("a recusa do pedido de MOD não chegou ao registro: {texto:?}");
        };
        assert!(
            linha.contains("mod_id=fulano/meu-mod"),
            "a recusa do pedido de MOD não traz `mod_id=fulano/meu-mod`, e o `grep` do guia \
             (`docs/como-se-faz-um-mod.md`, «Ler a linha») não a acha. As outras linhas do MOD \
             escrevem o id por `%`; o `str` cru sai pelo `Debug`, entre aspas: {linha}"
        );
    }

    /// O id que vai à linha da recusa perde o que inverte a linha, e não só
    /// o que a quebra, e é cortado no teto.
    ///
    /// Um U+202E no id inverte, num editor que segue o bidi, o `error=` que
    /// vem depois dele; e o fio confere o id só pelo tamanho.
    #[test]
    fn o_id_da_recusa_perde_o_que_inverte_a_linha_e_cabe_no_teto() {
        assert_eq!(
            super::id_seguro("a/\u{202E}b\u{2028}c\u{2066}d\u{85}e"),
            "a/bcde",
            "o id de um pedido de MOD vai à linha da recusa com um caractere que quebra ou \
             inverte a linha"
        );
        assert_eq!(
            super::id_seguro(&"x".repeat(300)).chars().count(),
            super::TETO_DO_ID_NO_REGISTRO,
            "o id de um pedido de MOD vai à linha da recusa sem o teto, e quem pede escolhe o \
             tamanho da linha"
        );
        assert_eq!(
            super::id_seguro("fulano/meu-mod"),
            "fulano/meu-mod",
            "um id de verdade mudou ao ir à linha da recusa, e o grep do guia não o acha"
        );
    }

    /// **O pedido do catálogo não escreve um `mod_id=` em branco** (P51-26).
    ///
    /// A janela pede a lista de MODs com o id vazio (`pedirAoServidor("", …)`,
    /// em `base.js`). A recusa dele saía como `mod_id= error=…`, um campo que
    /// casa com qualquer `grep "mod_id="` e não é de MOD nenhum.
    #[tokio::test(flavor = "current_thread")]
    async fn a_recusa_do_catalogo_diz_catalogo_e_nao_um_mod_id_em_branco() {
        let texto = o_registro_da_recusa("").await;
        let Some(linha) = texto
            .lines()
            .find(|linha| linha.contains("MOD request refused"))
        else {
            panic!("a recusa do pedido do catálogo não chegou ao registro: {texto:?}");
        };
        assert!(
            linha.contains("catalogo=true") && !linha.contains("mod_id="),
            "a recusa do pedido do catálogo não diz que era o catálogo, ou escreve um \
             `mod_id=` em branco que o `grep` de qualquer MOD acha: {linha}"
        );
    }

    /// **Um id feito só do que o filtro tira também não escreve um `mod_id=`
    /// em branco.**
    ///
    /// O id não é vazio, e por isso não é o catálogo; mas o [`super::id_seguro`]
    /// o reduz a um espaço, e a linha sairia `mod_id=  error=…`, a mesma forma
    /// que o ramo do catálogo evita. A linha diz que o id não se lia.
    #[tokio::test(flavor = "current_thread")]
    async fn um_id_so_do_que_o_filtro_tira_diz_que_nao_se_lia_e_nao_um_mod_id_em_branco() {
        let texto = o_registro_da_recusa("\u{202E}\n \u{2028}").await;
        let Some(linha) = texto
            .lines()
            .find(|linha| linha.contains("MOD request refused"))
        else {
            panic!("a recusa do pedido de id ilegível não chegou ao registro: {texto:?}");
        };
        assert!(
            linha.contains("mod_id_ilegivel=true") && !linha.contains("mod_id="),
            "a recusa de um pedido cujo id é feito só do que quebra ou inverte a linha não diz \
             que o id não se lia, ou escreve um `mod_id=` em branco: {linha}"
        );
        assert_eq!(
            texto.lines().count(),
            1,
            "o id ilegível escreveu mais de uma linha no seele.log de quem hospeda: {texto:?}"
        );
    }

    /// **O id que chega pelo fio não escreve uma segunda linha no `seele.log`
    /// de quem hospeda** (P51-26).
    ///
    /// O id de um `ModRequest` vem de um cliente autenticado, e a única
    /// conferência dele no fio é o tamanho. Com o `%`, o `fmt` o escreve cru:
    /// um `\n` nele fechava a linha da recusa e abria outra, com a cara do
    /// produto, escrita por quem pediu.
    #[tokio::test(flavor = "current_thread")]
    async fn um_id_com_quebra_de_linha_nao_forja_uma_linha_no_registro() {
        let texto = o_registro_da_recusa("a/b\nWARN seele_server: forjada").await;
        assert_eq!(
            texto.lines().count(),
            1,
            "o id de um pedido de MOD, que chega pelo fio, escreveu mais de uma linha no \
             seele.log de quem hospeda: {texto:?}"
        );
        assert!(
            texto.contains("MOD request refused") && texto.contains("mod_id=a/b"),
            "a linha da recusa sumiu, ou perdeu o começo do id: {texto:?}"
        );
    }
}
