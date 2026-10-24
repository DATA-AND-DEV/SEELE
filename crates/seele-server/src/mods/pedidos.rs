//! API v2: private request/reply over the authenticated control connection.
//! Replies never use the shared chat/event bus. A fresh bounded runtime handles
//! each request; the database lock serializes read/modify/commit across people.

use crate::{
    autorizacao::{self, Acesso},
    permissions::Permissions,
    persistence::{channels::Channels, mods},
    server::Server,
};
use seele_proto::{
    control::Permission,
    ids::{ChannelId, PersonId},
};
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// Os arquivos de um pacote na ordem e na forma em que o hash dele é conferido:
/// sem `dados/` na raiz, e sem atalho.
pub(crate) fn package(
    root: &Path,
    dir: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> anyhow::Result<()> {
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
            // **O quintal cheio, pelo nome** (pedido da MESA, item 9; rascunho
            // da API 6, §8): o MOD recebe por que a gravação não aconteceu, e
            // não a recusa genérica de qualquer outra falha.
            if matches!(
                error.downcast_ref::<super::Falha>(),
                Some(super::Falha::QuintalCheio)
            ) {
                return r#"{"ok":false,"error":"quintal-cheio"}"#.into();
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
    // As perguntas restantes ao banco, numa tomada curta e só de leitura.
    //
    // **O escopo de um pedido** (rascunho da API 6, §8; tarefa 16 da F3): num
    // canal, ler e escrever são **os do canal**, com o papel mínimo dele, por
    // `autorizacao::pode_no_canal`, como no texto. Antes era a permissão
    // global com o canal existindo: um pedido num canal que a pessoa não lê
    // chegava ao MOD, e o `write` dele ignorava o papel de escrita do canal. No
    // escopo de servidor (canal zero), o `write` é a permissão global.
    //
    // Hoje nenhum caminho do produto grava o papel mínimo, e as duas formas dão
    // o mesmo resultado (medido em 10/10, pendência 62); o guarda
    // `o_papel_minimo_de_um_canal_so_e_gravado_pela_migracao_e_pelos_testes`
    // avisa quem criar um. Uma falha de banco lê como negação, como em todo
    // `pode_no_canal`.
    let (canal_existe, le_o_canal, admin, write) = {
        let db = server.persistence.lock().await;
        let permissions = Permissions::new(&db);
        let admin = permissions
            .may(person, Permission::AdministerServer)
            .unwrap_or(false);
        if com_canal {
            let existe = Channels::new(&db)
                .channels()?
                .iter()
                .any(|c| c.id == channel);
            let le =
                autorizacao::pode_no_canal(&db, person, channel, Acesso::Leitura).unwrap_or(false);
            let escreve =
                autorizacao::pode_no_canal(&db, person, channel, Acesso::Escrita).unwrap_or(false);
            (existe, le, admin, escreve)
        } else {
            let escreve = permissions
                .may(person, Permission::WriteChannel)
                .unwrap_or(false);
            (true, true, admin, escreve)
        }
    };
    anyhow::ensure!(canal_existe, "unknown channel");
    anyhow::ensure!(le_o_canal, "cannot read channel");
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
    // **A sala de quem pede, dita pelo servidor** (rascunho da API 6, §5; a
    // pergunta da MESA): a da presença, ou `null` fora de sala. Escrita aqui, e
    // nunca lida do pedido — como `person`, ela é do servidor, e um pedido que
    // mandasse outra sala não muda nada.
    let sala = server
        .occupancy
        .lock()
        .await
        .onde_esta(person)
        .map(|sala| sala.0);
    let context = serde_json::json!({"person":person.0.to_string(),"channel":contexto_do_canal,"sala":sala,"admin":admin,"write":write}).to_string();
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
    let (fonte, portas) = {
        let guardado = conferidos
            .lock()
            .map_err(|_| anyhow::anyhow!("o cache de pacotes de MOD foi envenenado"))?
            .get(&hash)
            .cloned();
        match guardado {
            Some(pacote) => (pacote.fonte, pacote.portas),
            None => {
                let hash_para_conferir = hash.clone();
                let id_do_pacote = id.clone();
                let raiz = root.clone();
                // Fora do Tokio: são leituras de disco e um SHA-256 sobre o
                // pacote inteiro.
                let (fonte, portas) = tokio::task::spawn_blocking(
                    move || -> anyhow::Result<(String, super::Portas)> {
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
                        Ok((
                            std::fs::read_to_string(raiz.join(relative))?,
                            super::Portas {
                                tema: manifest.tema,
                            },
                        ))
                    },
                )
                .await??;
                conferidos
                    .lock()
                    .map_err(|_| anyhow::anyhow!("o cache de pacotes de MOD foi envenenado"))?
                    .insert(
                        hash.clone(),
                        crate::server::PacoteConferido {
                            fonte: fonte.clone(),
                            portas,
                        },
                    );
                (fonte, portas)
            }
        }
    };

    // O quintal é lido numa tomada curta, **antes** de o JavaScript rodar — e,
    // na mesma tomada, quem pinta este servidor, para `mundo.tema` responder
    // `tema-de-outro-mod` na hora (rascunho da API 6, §9).
    let (mut data, pintor): (BTreeMap<String, String>, Option<String>) = {
        let db = server.persistence.lock().await;
        (
            mods::ler_quintal(&db, &id)?,
            crate::persistence::tema::escolhido_ligado(&db)?,
        )
    };
    let before = data.clone();
    let id_do_pedido = id.clone();
    // O JavaScript nunca roda no Tokio, e agora também não roda com o banco na
    // mão.
    let DoPedido {
        resposta: response,
        quintal: data,
        tema,
        avisos,
    } = tokio::task::spawn_blocking(move || -> anyhow::Result<DoPedido> {
        let mut host = super::Anfitriao::novo()?;
        // **Antes de `carregar`.** As ligações do QuickJS clonam este `Arc` ao
        // serem montadas; compartilhar depois deixaria o MOD escrevendo numa
        // lista que ninguém mais lê. Ver `Anfitriao::compartilhar_esperas`.
        host.compartilhar_esperas(esperas);
        host.carregar_com_portas(&id_do_pedido, &fonte, &dados, portas)?;
        host.saber_quem_pinta(pintor);
        let response = host.pedir(&id_do_pedido, person, &context, &payload, &mut data)?;
        Ok(DoPedido {
            resposta: response,
            quintal: data,
            tema: host.tomar_tema_declarado(),
            avisos: host.tomar_avisos(),
        })
    })
    .await??;

    // E gravado noutra tomada curta. O cadeado deste MOD ainda está na mão, e é
    // ele que garante que ninguém leu o quintal entre a leitura acima e esta
    // gravação — que é a perda de atualização que soltar o mutex introduziria se
    // ele não existisse.
    //
    // O tema vai **depois** do quintal, na mesma tomada, e o `TemaMudou` só sai
    // depois dos dois gravados: um quintal que falhasse com o tema já
    // publicado seria o MOD contando a todos uma aparência que o estado dele
    // não sustenta. A mesma ordem que o aviso do servidor terá (rascunho §7.3).
    if data != before || tema.is_some() {
        let mut db = server.persistence.lock().await;
        if data != before {
            mods::gravar_quintal(&mut db, &id, &data)?;
        }
        if let Some(tema) = tema {
            publicar_o_tema(server, &db, &id, tema)?;
        }
    }
    // **O aviso depois do quintal gravado** (pedido da MESA, 09/10; rascunho
    // da API 6, §7.3): se a gravação acima falhou, o `?` já saiu, e o aviso cai
    // com ela. E pela taxa do avisador, que é do servidor, e não deste pedido.
    if !avisos.is_empty() {
        let barramento = server.events.clone();
        server.avisador.avisar(&id, avisos, move |saindo| {
            let _ = barramento.send(crate::server::Event::ModAviso {
                mod_id: saindo.mod_id,
                canal: saindo.canal,
                carga: saindo.carga,
            });
        });
    }
    Ok(response)
}

/// Grava o tema que um MOD declarou e, se o tema em vigor mudou, o manda a
/// todos.
///
/// Chamada pelos dois caminhos que rodam a metade de servidor — o pedido e o
/// momento —, com o banco já na mão.
///
/// # Errors
///
/// Falha se o banco não responder. Uma recusa por outro MOD não é erro: o MOD
/// já leu `ok` porque o banco mudou entre a leitura de quem pinta e a gravação
/// (dois MODs com a porta declarando ao mesmo tempo), e é dita no registro de
/// quem hospeda — o MOD não tem mais como ler.
pub fn publicar_o_tema(
    server: &Server,
    db: &crate::persistence::Persistence,
    id: &str,
    tema: super::TemaDeclarado,
) -> anyhow::Result<()> {
    publicar_no_barramento(&server.events, db, id, tema)
}

/// [`publicar_o_tema`], com o barramento em vez do servidor: é o que o
/// despachante dos momentos tem na mão.
///
/// # Errors
///
/// As de [`publicar_o_tema`].
pub fn publicar_no_barramento(
    barramento: &tokio::sync::broadcast::Sender<crate::server::Event>,
    db: &crate::persistence::Persistence,
    id: &str,
    tema: super::TemaDeclarado,
) -> anyhow::Result<()> {
    use crate::persistence::tema::{declarar, em_vigor, Declaracao};

    let antes = em_vigor(db)?;
    match declarar(db, id, tema.valores.as_deref())? {
        Declaracao::Aceita => {
            let agora = em_vigor(db)?;
            // **A cada troca**, e não a cada declaração: redeclarar o mesmo tema
            // não é notícia para cinquenta sessões.
            if agora != antes {
                let _ = barramento.send(crate::server::Event::TemaMudou { tema: agora });
            }
        }
        Declaracao::DeOutroMod { escolhido } => {
            tracing::warn!(
                mod_id = %id_seguro(id),
                escolhido = %id_seguro(&escolhido),
                "o MOD declarou um tema e leu ok, mas outro MOD passou a pintar este                  servidor antes da gravação; o tema dele não foi publicado"
            );
        }
    }
    Ok(())
}

/// O que um pedido traz de volta do QuickJS: a resposta, o quintal como ficou, e
/// o que a metade de servidor declarou — o tema e os avisos —, que só valem
/// depois de o quintal gravar.
struct DoPedido {
    resposta: String,
    quintal: BTreeMap<String, String>,
    tema: Option<super::TemaDeclarado>,
    avisos: Vec<super::aviso::AvisoDeclarado>,
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
        servidor_em_memoria(None)
    }

    /// Um servidor em memória, com ou sem a pasta de MODs.
    fn servidor_em_memoria(
        mods_dir: Option<crate::RaizesDosMods>,
    ) -> std::sync::Arc<crate::server::Server> {
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
            avisador: Arc::new(crate::mods::aviso::Avisador::default()),
            impressao: [0; 32],
            janelas_do_navegador: Vec::new(),
            recepcao: None,
            persistence,
            events,
            writes,
            slots: Arc::new(Mutex::new(crate::server::Slots::default())),
            occupancy: Arc::new(Mutex::new(crate::server::Occupancy::default())),
            presentes: Arc::new(Mutex::new(crate::server::Presentes::default())),
            subida: Arc::new(Mutex::new(crate::tela::Subida::nova())),
            portaria: Arc::new(Mutex::new(crate::taxa::Portaria::nova())),
            atrasos: Arc::new(crate::server::Atrasos::default()),
            desconhecidas: Arc::new(crate::server::MensagensDesconhecidas::default()),
            desassentamentos: Arc::new(crate::server::Desassentamentos::default()),
            pares: Arc::new(Mutex::new(crate::pares::Pares::default())),
            versao_do_anuncio: seele_proto::mods::VERSAO_DO_ANUNCIO,
            telas: Arc::new(Mutex::new(Telas::default())),
            anexos: None,
            caminho_bps: None,
            mods_dir,
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
        // Sem ele, a linha da recusa pode sumir quando a bateria roda em
        // paralelo (pendência 56): ver `rastro_de_teste::segundo_despachante`.
        crate::rastro_de_teste::segundo_despachante();
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

    /// O MOD de eco: devolve o contexto que a ponte lhe entregou, e só.
    const ECO: &str =
        "globalThis.aoPedir = (contexto) => JSON.stringify({ ok: true, contexto: JSON.parse(contexto) });";

    /// Um servidor em memória com o MOD de eco ligado, o canal 1, e duas pessoas:
    /// a primeira, que toma o comando, e uma Pessoa comum — quem pede.
    async fn servidor_com_eco(
        pasta: &std::path::Path,
    ) -> (
        std::sync::Arc<crate::server::Server>,
        seele_proto::ids::PersonId,
    ) {
        servidor_com_o_mod(pasta, ECO).await
    }

    /// O mesmo servidor, com outra metade de servidor no `prova/eco`.
    async fn servidor_com_o_mod(
        pasta: &std::path::Path,
        fonte: &str,
    ) -> (
        std::sync::Arc<crate::server::Server>,
        seele_proto::ids::PersonId,
    ) {
        let obras = pasta.join("em-obras");
        std::fs::create_dir_all(obras.join("servidor")).expect("a pasta do pacote");
        let manifesto = serde_json::json!({
            "schema": 1,
            "id": "prova/eco",
            "version": "1.0.0",
            "api": seele_proto::mods::MOD_API_VERSION,
            "repo": "https://example.invalid/eco",
            "reach": ["estado no servidor"],
            "server": "servidor/main.js",
        })
        .to_string();
        let mut arquivos = vec![
            ("mod.json".to_owned(), manifesto.into_bytes()),
            ("servidor/main.js".to_owned(), fonte.as_bytes().to_vec()),
        ];
        for (caminho, bytes) in &arquivos {
            std::fs::write(obras.join(caminho), bytes).expect("o arquivo do pacote");
        }
        let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
        let raizes = crate::RaizesDosMods {
            pacotes: pasta.to_path_buf(),
            dados: pasta.join("mod-data"),
        };
        let destino = raizes.pacote_de(&hash);
        std::fs::create_dir_all(destino.parent().expect("o pai")).expect("o cache");
        std::fs::rename(&obras, &destino).expect("o pacote no cache");

        let servidor = servidor_em_memoria(Some(raizes));
        let pessoa = {
            let db = servidor.persistence.lock().await;
            db.connection()
                .execute("INSERT INTO channels (id, name) VALUES (1, 'geral')", [])
                .expect("o canal");
            crate::persistence::mods::enable(
                &db,
                &crate::persistence::mods::EnabledMod {
                    id: "prova/eco".to_owned(),
                    version: "1.0.0".to_owned(),
                    hash,
                    repo: "https://example.invalid/eco".to_owned(),
                    reach: vec!["estado no servidor".to_owned()],
                    server_half: true,
                    api: seele_proto::mods::MOD_API_VERSION,
                    tema: false,
                },
            )
            .expect("ligar o MOD");
            let permissoes = crate::permissions::Permissions::new(&db);
            permissoes
                .register_or_find(&[1; 32], "comando")
                .expect("a primeira conta");
            permissoes
                .register_or_find(&[2; 32], "pessoa")
                .expect("a pessoa")
                .id
        };
        (servidor, pessoa)
    }

    /// Grava um papel mínimo no canal 1, como nenhum caminho do produto grava
    /// hoje (medido em 10/10: só a migração 1 cria as colunas, e só os testes
    /// de `autorizacao.rs` as escrevem).
    async fn papel_minimo_no_canal_1(
        servidor: &crate::server::Server,
        coluna: &str,
        papel: seele_proto::ids::RoleId,
    ) {
        let db = servidor.persistence.lock().await;
        db.connection()
            .execute(
                &format!("UPDATE channels SET {coluna} = ?1 WHERE id = 1"),
                [i64::from(papel.0)],
            )
            .expect("o papel mínimo");
    }

    async fn pedir(
        servidor: &std::sync::Arc<crate::server::Server>,
        pessoa: seele_proto::ids::PersonId,
        canal: u32,
    ) -> serde_json::Value {
        let resposta = super::executar(
            servidor,
            pessoa,
            seele_proto::ids::ChannelId(canal),
            "prova/eco",
            "{}",
        )
        .await;
        serde_json::from_str(&resposta).expect("a resposta é JSON")
    }

    /// **Um pedido de MOD num canal que a pessoa não lê não chega ao MOD**
    /// (tarefa 16 da F3; rascunho da API 6, §8, «O escopo de um pedido»). A
    /// leitura é a do canal (`autorizacao::pode_no_canal`), e não só a
    /// permissão global com o canal existindo.
    #[tokio::test(flavor = "current_thread")]
    async fn um_pedido_num_canal_que_a_pessoa_nao_le_nao_chega_ao_mod() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let (servidor, pessoa) = servidor_com_eco(pasta.path()).await;
        let antes = pedir(&servidor, pessoa, 1).await;
        assert_eq!(
            antes["ok"], true,
            "sem papel mínimo, o MOD não respondeu: {antes}"
        );

        papel_minimo_no_canal_1(
            &servidor,
            "minimum_read_role",
            crate::permissions::OPERATOR_ROLE,
        )
        .await;
        let depois = pedir(&servidor, pessoa, 1).await;
        assert_eq!(
            depois["error"], "bridge-refused",
            "um pedido de MOD num canal que a pessoa não lê chegou ao MOD: {depois}"
        );
    }

    /// **O `write` que o MOD recebe é o do canal**, e no escopo de servidor
    /// (canal zero) é a permissão global de escrita.
    #[tokio::test(flavor = "current_thread")]
    async fn o_write_que_o_mod_recebe_e_o_do_canal() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let (servidor, pessoa) = servidor_com_eco(pasta.path()).await;
        let antes = pedir(&servidor, pessoa, 1).await;
        assert_eq!(
            antes["contexto"]["write"], true,
            "sem papel mínimo: {antes}"
        );

        papel_minimo_no_canal_1(
            &servidor,
            "minimum_write_role",
            crate::permissions::OPERATOR_ROLE,
        )
        .await;
        let no_canal = pedir(&servidor, pessoa, 1).await;
        assert_eq!(
            no_canal["contexto"]["write"], false,
            "o MOD leu que a pessoa escreve num canal cujo papel de escrita ela não tem: {no_canal}"
        );
        let sem_canal = pedir(&servidor, pessoa, 0).await;
        assert_eq!(
            sem_canal["contexto"]["write"], true,
            "no escopo de servidor, o `write` deixou de ser a permissão global: {sem_canal}"
        );
    }

    /// **O contexto diz quem pediu, e se ela administra** (`reads.contexto.pessoa`
    /// e `reads.servidor.contexto`; a auditoria do `v6.json`, tabela 1). As
    /// provas de fora conferiam só que o campo não é vazio e que não se forja:
    /// um `person` constante, ou um `admin` sempre falso, passavam. Aqui, o
    /// `person` é o `PersonId` de quem pediu, em texto, e o `admin` é o do
    /// papel dela — a primeira conta, que tem o comando, administra; uma Pessoa
    /// comum, não.
    #[tokio::test(flavor = "current_thread")]
    async fn o_contexto_diz_quem_pediu_e_se_ela_administra() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let (servidor, pessoa) = servidor_com_eco(pasta.path()).await;
        let comando = {
            let db = servidor.persistence.lock().await;
            crate::permissions::Permissions::new(&db)
                .register_or_find(&[1; 32], "comando")
                .expect("a primeira conta")
                .id
        };
        assert_ne!(
            comando, pessoa,
            "o caso não mede o que diz: as duas contas são a mesma"
        );
        for (quem, administra) in [(pessoa, false), (comando, true)] {
            let resposta = pedir(&servidor, quem, 0).await;
            assert_eq!(
                resposta["contexto"]["person"],
                serde_json::Value::from(quem.0.to_string()),
                "o MOD não leu quem pediu: {resposta}"
            );
            assert_eq!(
                resposta["contexto"]["admin"], administra,
                "o MOD leu o `admin` errado de {quem:?}: {resposta}"
            );
        }
    }

    /// Os `.rs` do `src/` de cada crate e app, sem `target/` e `node_modules/`.
    fn fontes_do_produto(dir: &std::path::Path, saida: &mut Vec<std::path::PathBuf>) {
        let Ok(entradas) = std::fs::read_dir(dir) else {
            return;
        };
        for entrada in entradas.flatten() {
            let caminho = entrada.path();
            let nome = entrada.file_name();
            if caminho.is_dir() {
                if nome != "target" && nome != "node_modules" && nome != ".git" {
                    fontes_do_produto(&caminho, saida);
                }
            } else if caminho.extension().is_some_and(|e| e == "rs")
                && caminho.components().any(|parte| parte.as_os_str() == "src")
            {
                saida.push(caminho);
            }
        }
    }

    /// As linhas de um arquivo antes do módulo de testes dele: o `#[cfg(test)]`
    /// seguido, depois dos atributos, de um `mod` com corpo, que neste
    /// repositório vai até o fim do arquivo. Um `#[cfg(test)]` sobre uma função
    /// ou um `mod x;` de fora não corta nada.
    fn antes_dos_testes(texto: &str) -> Vec<&str> {
        let linhas: Vec<&str> = texto.lines().collect();
        for (i, linha) in linhas.iter().enumerate() {
            if linha.trim() != "#[cfg(test)]" {
                continue;
            }
            let item = linhas.iter().skip(i + 1).map(|l| l.trim_start()).find(|l| {
                [
                    "mod ",
                    "pub mod ",
                    "pub(crate) mod ",
                    "fn ",
                    "pub ",
                    "pub(crate) ",
                    "async ",
                    "const ",
                    "static ",
                    "use ",
                    "impl",
                    "struct ",
                    "enum ",
                    "type ",
                    "macro_rules!",
                ]
                .iter()
                .any(|inicio| l.starts_with(inicio))
            });
            if let Some(item) = item {
                let modulo = ["mod ", "pub mod ", "pub(crate) mod "]
                    .iter()
                    .any(|m| item.starts_with(m));
                if modulo && !item.trim_end().ends_with(';') {
                    return linhas[..i].to_vec();
                }
            }
        }
        linhas
    }

    /// **O papel mínimo de um canal só é gravado pela migração e pelos testes**
    /// (pedido da orquestradora, 10/10; pendência 62).
    ///
    /// Até a tarefa 16, um pedido de MOD num canal conferia só a permissão
    /// global: ele chegava ao MOD num canal que a pessoa não lê, e o `write`
    /// ignorava o papel de escrita do canal. Não se alcançava em nenhuma versão
    /// porque **nenhum caminho do produto grava `minimum_read_role` nem
    /// `minimum_write_role`** (medido em 10/10, no v0.15.1, na linha 0.15 e
    /// aqui). O dia em que um recurso gravar o papel mínimo é o dia em que
    /// essa conferência passa a valer — e este guarda reprova nesse dia, com o
    /// lugar e o que conferir, para quem cria o recurso ver a dependência.
    ///
    /// De texto, e simples de propósito: lê o `src/` de cada crate e app, sem
    /// os módulos de teste e sem comentários. As menções permitidas são a
    /// migração que cria as colunas e a leitura em `Acesso::coluna`.
    #[test]
    fn o_papel_minimo_de_um_canal_so_e_gravado_pela_migracao_e_pelos_testes() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut fontes = Vec::new();
        for topo in ["crates", "apps"] {
            fontes_do_produto(&raiz.join(topo), &mut fontes);
        }
        assert!(
            fontes
                .iter()
                .any(|f| f.ends_with("seele-server/src/autorizacao.rs")),
            "o guarda não achou as fontes do produto em {}",
            raiz.display()
        );
        let mut achados = Vec::new();
        for fonte in &fontes {
            let texto = std::fs::read_to_string(fonte).expect("a fonte");
            // Com `/` em todo sistema: no Windows o `display()` vem com `\`, e
            // o `ends_with` de texto abaixo não casava com as duas menções
            // permitidas (reprovou no PC Windows, em `51900612`).
            let relativo = fonte
                .strip_prefix(&raiz)
                .unwrap_or(fonte)
                .display()
                .to_string()
                .replace('\\', "/");
            for (n, linha) in antes_dos_testes(&texto).iter().enumerate() {
                let limpa = linha.trim_start();
                if limpa.starts_with("//")
                    || !(limpa.contains("minimum_read_role")
                        || limpa.contains("minimum_write_role"))
                {
                    continue;
                }
                let permitida = relativo.ends_with("seele-server/src/persistence/schema.rs")
                    || (relativo.ends_with("seele-server/src/autorizacao.rs")
                        && limpa.starts_with("Self::")
                        && limpa.contains("=> \"minimum_"));
                if !permitida {
                    achados.push(format!("{relativo}:{}: {limpa}", n + 1));
                }
            }
        }
        assert!(
            achados.is_empty(),
            "um caminho do produto passou a mencionar o papel mínimo de um canal fora da migração \
             e da leitura de `autorizacao.rs`:\n{}\n\nO pedido de MOD num canal confere a leitura e \
             a escrita pelo papel mínimo (`autorizacao::pode_no_canal`, em `mods/pedidos.rs`; \
             tarefa 16 da F3, pendência 62), e é a partir de agora que isso vale. Confira que ele \
             ainda o faz, e que `um_pedido_num_canal_que_a_pessoa_nao_le_nao_chega_ao_mod` e \
             `o_write_que_o_mod_recebe_e_o_do_canal` cobrem o recurso novo; então ponha o lugar \
             novo como permitido neste guarda.",
            achados.join("\n")
        );
    }

    /// **O quintal cheio chega ao MOD pelo nome** (pedido da MESA, item 9;
    /// rascunho da API 6, §8). Medido em 10/10: o `aoPedir` termina, a gravação
    /// recusa (`Falha::QuintalCheio`), e a janela recebia a recusa genérica,
    /// `bridge-refused`, a mesma de qualquer outra falha.
    #[tokio::test(flavor = "current_thread")]
    async fn o_quintal_cheio_chega_ao_mod_pelo_nome() {
        let pasta = tempfile::tempdir().expect("a pasta");
        let fonte = format!(
            "globalThis.aoPedir = () => {{ dados.grande = 'a'.repeat({}); return '{{}}'; }};",
            crate::mods::Anfitriao::TETO_DO_QUINTAL + 1
        );
        let (servidor, pessoa) = servidor_com_o_mod(pasta.path(), &fonte).await;
        let resposta = pedir(&servidor, pessoa, 0).await;
        assert_eq!(
            resposta["error"], "quintal-cheio",
            "o quintal cheio não chegou ao MOD pelo nome: {resposta}"
        );
    }
}
