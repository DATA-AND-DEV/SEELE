//! Handing real room events to the MODs that are on.
//!
//! ADR 0045. This is where MODs stop being a laboratory: until here nothing
//! called `aoAcontecer` from a room, only tests.
//!
//! # One subscriber, and not twenty-one hooks
//!
//! The server already fans every event out over a `broadcast::Sender<Event>`,
//! so a MOD dispatcher is a **subscriber** and touches none of the places that
//! emit. Twenty-one call sites edited by hand would be twenty-one chances to
//! forget the twenty-second, and the twenty-second is always the one that
//! matters.
//!
//! # Why a thread of its own
//!
//! A QuickJS `Runtime` is `!Send`: it cannot cross an `await`, and it cannot
//! sit behind a `tokio::Mutex`. So the host lives on one OS thread and is fed
//! by a channel. That is a constraint of the engine, not a preference — and it
//! buys something anyway: **MOD code cannot occupy a Tokio worker**, which is
//! the thread the audio path shares. ADR 0045 wrote "MOD code never touches the
//! audio path" as a design boundary; this is the mechanism that holds it.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;
use tokio::sync::{mpsc, oneshot};

use super::{Anfitriao, Falha};

/// One MOD's yard, as it crosses to the thread and back.
pub type Quintal = BTreeMap<String, String>;

/// What the dispatcher is asked to do.
/// O que a fila carrega até a thread dos MODs.
enum Pedido {
    /// Um momento para entregar a quem estiver carregado.
    Momento {
        momento: String,
        carga: String,
        quintais: BTreeMap<String, Quintal>,
        resposta: oneshot::Sender<Resposta>,
    },
    /// **O conjunto mudou; troque o código carregado.**
    ///
    /// Sem isto, o despachante nascia no arranque com as fontes de então e
    /// ficava com elas: ligar um MOD enquanto o servidor está de pé mudava o
    /// que o anúncio exige e o que os pedidos resolvem, e **não** mudava o que
    /// reage a evento. Um MOD recém-ligado ficava mudo aos eventos até alguém
    /// reiniciar o servidor, e um recém-desligado continuava reagindo.
    ///
    /// Pela fila, e não por um caminho à parte: a thread é serial de propósito,
    /// e trocar o código enquanto um MOD roda seria a corrida que essa
    /// serialização existe para não ter.
    Recarregar {
        /// `(id, fonte, pasta de dados)`, como `carregar_do_disco` devolve.
        fontes: Vec<(String, String, std::path::PathBuf)>,
        /// Quem não compilou, para quem pediu a troca poder dizer em voz alta.
        resposta: oneshot::Sender<Vec<(String, Falha)>>,
    },
}

/// What came back.
pub struct Resposta {
    /// **O que cada MOD mudou**, e não o quintal inteiro como ele ficou.
    ///
    /// A diferença é uma corrida. O caminho de eventos lê o quintal, **solta o
    /// cadeado do banco** — um MOD lento não pode segurar quem está
    /// conversando —, roda no QuickJS, e só então volta para gravar. Um pedido
    /// que chegue nesse meio lê, roda e grava antes. Devolvendo o mapa inteiro,
    /// a gravação do evento passa por cima do que o pedido escreveu, com um
    /// retrato tirado antes de ele existir.
    ///
    /// Devolvendo a **diferença**, a gravação aplica sobre o que está no banco
    /// agora: as duas escritas convivem quando tocaram chaves diferentes, e só
    /// colidem quando tocaram a mesma — onde a última vence, que é o que
    /// qualquer ordem daria.
    pub quintais: BTreeMap<String, MudancaNoQuintal>,
    /// Who failed, and how. ADR 0045: a MOD that throws is disabled and the
    /// room continues.
    pub falharam: Vec<(String, Falha)>,
}

/// O que um MOD mudou no quintal dele, por chave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MudancaNoQuintal {
    /// Chaves escritas ou reescritas, com o valor novo.
    pub escritas: BTreeMap<String, String>,
    /// Chaves que o MOD apagou.
    pub apagadas: Vec<String>,
}

impl MudancaNoQuintal {
    /// A diferença entre o quintal como ele estava e como ficou.
    #[must_use]
    pub fn entre(antes: &Quintal, depois: &Quintal) -> Self {
        let escritas = depois
            .iter()
            .filter(|(chave, valor)| antes.get(*chave) != Some(*valor))
            .map(|(chave, valor)| (chave.clone(), valor.clone()))
            .collect();
        let apagadas = antes
            .keys()
            .filter(|chave| !depois.contains_key(*chave))
            .cloned()
            .collect();
        Self { escritas, apagadas }
    }

    /// Há algo a gravar?
    #[must_use]
    pub fn vazia(&self) -> bool {
        self.escritas.is_empty() && self.apagadas.is_empty()
    }

    /// Aplica esta diferença sobre um quintal — o que estiver no banco **agora**.
    pub fn aplicar(&self, quintal: &mut Quintal) {
        for chave in &self.apagadas {
            quintal.remove(chave);
        }
        for (chave, valor) in &self.escritas {
            quintal.insert(chave.clone(), valor.clone());
        }
    }
}

/// Relê o conjunto exigido e troca o código carregado por ele.
///
/// Uma falha aqui **não desliga nada**: o MOD que não compila é dito em voz
/// alta e o conjunto anterior continua de pé, que é o critério da etapa 4 do
/// plano — «falha mantém conjunto anterior íntegro».
async fn recarregar_o_conjunto(
    persistence: &std::sync::Arc<tokio::sync::Mutex<crate::persistence::Persistence>>,
    despachante: &Despachante,
    raizes: &crate::RaizesDosMods,
) {
    let exigidos: Vec<(String, String)> = {
        let banco = persistence.lock().await;
        match crate::persistence::mods::enabled(&banco) {
            Ok(ligados) => ligados
                .into_iter()
                .map(|ligado| (ligado.id, ligado.hash))
                .collect(),
            Err(erro) => {
                tracing::error!(%erro, "não deu para reler o conjunto de MODs");
                return;
            }
        }
    };
    let (fontes, queixas) = super::carregar_do_disco(raizes, &exigidos);
    for queixa in queixas {
        tracing::error!("MOD exigido e não carregado — {queixa}");
    }
    if let Some(recusados) = despachante.recarregar(fontes).await {
        for (id, falha) in recusados {
            tracing::error!(mod_id = %id, %falha, "MOD recusado ao recarregar; a sala segue sem ele");
        }
    }
}

/// The handle the async side holds.
pub struct Despachante {
    fila: mpsc::UnboundedSender<Pedido>,
}

impl Despachante {
    /// Starts the MOD thread with the MODs already loaded into it.
    ///
    /// `fontes` is `id → (source, data folder)`. A MOD whose source does not
    /// compile is reported and the others still start: one broken MOD is not a
    /// server that will not host.
    #[must_use]
    pub fn iniciar(
        fontes: Vec<(String, String, std::path::PathBuf)>,
    ) -> (Self, Vec<(String, Falha)>) {
        let (para_dentro, mut de_fora) = mpsc::unbounded_channel::<Pedido>();
        let (avisar, recolher) = std::sync::mpsc::channel::<Vec<(String, Falha)>>();

        std::thread::Builder::new()
            .name("seele-mods".to_owned())
            .spawn(move || {
                let mut anfitriao = match Anfitriao::novo() {
                    Ok(anfitriao) => anfitriao,
                    Err(_) => {
                        let _ = avisar.send(Vec::new());
                        return;
                    }
                };

                let mut recusados = Vec::new();
                for (id, fonte, pasta) in fontes {
                    if let Err(falha) = anfitriao.carregar(&id, &fonte, &pasta) {
                        recusados.push((id, falha));
                    }
                }
                let _ = avisar.send(recusados);

                // O laço: um pedido por vez, na ordem em que chegam. Serial de
                // propósito — dois MODs escrevendo no mesmo quintal ao mesmo
                // tempo seria uma corrida que nenhum autor de MOD pode ver.
                // `blocking_recv` porque esta thread não é do Tokio: ela existe
                // justamente para que código de MOD nunca ocupe um worker, que é
                // a thread que o caminho de áudio divide.
                while let Some(pedido) = de_fora.blocking_recv() {
                    match pedido {
                        Pedido::Recarregar { fontes, resposta } => {
                            // `carregar` troca o que existe com o mesmo
                            // identificador, que é o que faz um MOD ser
                            // atualizável sem reiniciar o servidor.
                            let mut recusados = Vec::new();
                            for (id, fonte, pasta) in fontes {
                                if let Err(falha) = anfitriao.carregar(&id, &fonte, &pasta) {
                                    recusados.push((id, falha));
                                }
                            }
                            let _ = resposta.send(recusados);
                        }
                        Pedido::Momento {
                            momento,
                            carga,
                            quintais: entrada,
                            resposta,
                        } => {
                            let mut quintais = BTreeMap::new();
                            let mut falharam = Vec::new();

                            for (id, mut quintal) in entrada {
                                let antes = quintal.clone();
                                match anfitriao.chamar(&id, &momento, &carga, &mut quintal) {
                                    Ok(()) => {
                                        let mudanca = MudancaNoQuintal::entre(&antes, &quintal);
                                        if !mudanca.vazia() {
                                            quintais.insert(id, mudanca);
                                        }
                                    }
                                    Err(falha) => falharam.push((id, falha)),
                                }
                            }

                            let _ = resposta.send(Resposta { quintais, falharam });
                        }
                    }
                }
            })
            .ok();

        let recusados = recolher.recv().unwrap_or_default();
        (Self { fila: para_dentro }, recusados)
    }

    /// Hands one moment to every MOD whose yard is passed in.
    ///
    /// Returns `None` when the MOD thread is gone — which is not a failure to
    /// recover from here: it means the server is shutting down, and the caller
    /// simply stops asking.
    ///
    /// Assíncrono, e é o que impede um MOD lento de segurar o barramento: a
    /// espera é um `await`, não um bloqueio, então a tarefa que assina os
    /// eventos continua livre enquanto a thread dos MODs trabalha.
    /// Troca o código carregado pelo do conjunto que está valendo agora.
    ///
    /// Devolve quem não compilou. `None` quando a thread dos MODs já saiu — o
    /// servidor está descendo, e não há o que recarregar.
    pub async fn recarregar(
        &self,
        fontes: Vec<(String, String, std::path::PathBuf)>,
    ) -> Option<Vec<(String, Falha)>> {
        let (responder, esperar) = oneshot::channel();
        self.fila
            .send(Pedido::Recarregar {
                fontes,
                resposta: responder,
            })
            .ok()?;
        esperar.await.ok()
    }

    /// Hands one moment to every MOD whose yard is passed in.
    ///
    /// Returns `None` when the MOD thread is gone — which is not a failure to
    /// recover from here: it means the server is shutting down, and the caller
    /// simply stops asking.
    ///
    /// Assíncrono, e é o que impede um MOD lento de segurar o barramento: a
    /// espera é um `await`, não um bloqueio, então a tarefa que assina os
    /// eventos continua livre enquanto a thread dos MODs trabalha.
    pub async fn entregar(
        &self,
        momento: &str,
        carga: &str,
        quintais: BTreeMap<String, Quintal>,
    ) -> Option<Resposta> {
        let (responder, esperar) = oneshot::channel();
        self.fila
            .send(Pedido::Momento {
                momento: momento.to_owned(),
                carga: carga.to_owned(),
                quintais,
                resposta: responder,
            })
            .ok()?;
        esperar.await.ok()
    }
}

/// Follows the server's event bus and hands every moment to the MODs that are on.
///
/// The whole integration, and it edits none of the twenty-one places that emit:
/// the bus already fans out, so this is a subscriber.
///
/// # What it does when a MOD fails
///
/// **Disables it, and the room continues** — ADR 0045, "falha isolada". The
/// disable is written to the database, so it survives a restart: a MOD that
/// throws on every message would otherwise be re-enabled by the next boot and
/// throw again, forever, and whoever hosts would watch the same line scroll
/// past without a way to make it stop.
///
/// # Why it never gives up on the loop
///
/// A lagged bus is a MOD that missed events, and that is worth saying out loud;
/// it is not worth stopping for. The only exit is the bus closing, which is the
/// server going down.
pub async fn acompanhar(
    mut eventos: tokio::sync::broadcast::Receiver<crate::server::Event>,
    persistence: std::sync::Arc<tokio::sync::Mutex<crate::persistence::Persistence>>,
    despachante: Despachante,
    raizes: crate::RaizesDosMods,
) {
    // **O mesmo sinal que o anúncio observa.** Quem muda o conjunto — o SALVAR
    // da tela de gestão — o bumpa uma vez, na transação. Sem observá-lo, este
    // laço ficava para sempre com o código que carregou no arranque: um MOD
    // recém-ligado era mudo aos eventos, e um recém-desligado continuava
    // reagindo, até alguém reiniciar o servidor.
    let mut conjunto_mudou = {
        let banco = persistence.lock().await;
        banco.mods_mudaram()
    };

    // **Reconcilia ao entrar, e não só quando o sinal chega.**
    //
    // Esta tarefa é criada com `spawn`, e uma tarefa criada não é uma tarefa
    // que já rodou: entre a criação e a primeira polida cabe uma escrita no
    // conjunto. Quem escreveu nesse intervalo bumpou o contador antes de haver
    // quem o observasse, e o sinal nunca mais chega — o conjunto ficaria
    // divergente para sempre, com o código do arranque.
    //
    // Encontrado por teste, e não por leitura: a primeira versão confiava em
    // ter assinado a tempo, e o teste de ligar um MOD com o servidor de pé
    // falhava sem um diagnóstico sequer, porque o laço simplesmente nunca
    // acordava.
    //
    // Custa uma releitura na subida, e é idempotente: carregar o que já está
    // carregado troca o código pelo mesmo código.
    recarregar_o_conjunto(&persistence, &despachante, &raizes).await;

    loop {
        let evento = tokio::select! {
            // **A troca tem prioridade sobre o evento seguinte.** Entregar um
            // momento ao código antigo depois de o conjunto ter mudado é
            // entregá-lo a quem já não é exigido.
            biased;
            trocou = conjunto_mudou.changed() => {
                if trocou.is_err() {
                    break;
                }
                recarregar_o_conjunto(&persistence, &despachante, &raizes).await;
                continue;
            }
            recebido = eventos.recv() => match recebido {
                Ok(evento) => evento,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(quantos)) => {
                    // Um MOD perdeu momentos, e ele não tem como saber sozinho.
                    tracing::warn!(quantos, "o barramento passou na frente dos MODs");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
        };

        let Some((momento, carga)) = momento_de(&evento) else {
            continue;
        };

        // Carregar e soltar o cadeado **antes** de entregar: um MOD lento não
        // pode segurar o banco de quem está conversando.
        let quintais = {
            let banco = persistence.lock().await;
            let Ok(ligados) = crate::persistence::mods::enabled(&banco) else {
                continue;
            };
            let mut quintais = BTreeMap::new();
            for ligado in ligados {
                let quintal =
                    crate::persistence::mods::ler_quintal(&banco, &ligado.id).unwrap_or_default();
                quintais.insert(ligado.id, quintal);
            }
            quintais
        };
        if quintais.is_empty() {
            continue;
        }

        let Some(resposta) = despachante.entregar(momento, &carga, quintais).await else {
            // A thread dos MODs sumiu. Não há o que reiniciar aqui: o servidor
            // está descendo, ou subiu sem MOD nenhum.
            break;
        };

        let mut banco = persistence.lock().await;
        for (id, mudanca) in &resposta.quintais {
            // **Relido sob o cadeado, e a diferença aplicada sobre ele.** O
            // retrato que este evento levou pode ter envelhecido enquanto o MOD
            // rodava: um pedido que chegou no meio já leu, rodou e gravou.
            // Gravar o retrato antigo apagaria o que ele escreveu.
            let mut atual = crate::persistence::mods::ler_quintal(&banco, id).unwrap_or_default();
            mudanca.aplicar(&mut atual);
            if let Err(erro) = crate::persistence::mods::gravar_quintal(&mut banco, id, &atual) {
                tracing::error!(%erro, mod_id = %id, "não deu para gravar o quintal do MOD");
            }
        }
        for (id, falha) in &resposta.falharam {
            // **Não carregado não é quebrado.** O conjunto vive no banco e o
            // código vive na thread dos MODs; entre uma escrita e a recarga há
            // um instante, e um evento que caia nele encontra um MOD exigido e
            // ainda sem código. Desabilitar por isso era o produto desligar
            // sozinho o MOD que alguém acabara de ligar — sem ninguém pedir, e
            // sem dizer por quê. A recarga já está a caminho; este momento se
            // perde, e o seguinte o encontra de pé.
            if matches!(falha, crate::mods::Falha::NaoCarregadoAqui) {
                tracing::debug!(
                    mod_id = %id,
                    %momento,
                    "MOD exigido e ainda não carregado; este momento não o alcança"
                );
                continue;
            }
            tracing::error!(
                mod_id = %id,
                %momento,
                %falha,
                "MOD desabilitado: a sala continua sem ele"
            );
            if let Err(erro) = crate::persistence::mods::disable(&banco, id) {
                tracing::error!(%erro, mod_id = %id, "não deu para desabilitar o MOD que falhou");
            }
        }
    }
}

/// The name and payload one `Event` reaches a MOD as.
///
/// The name is the **wire's own name** — `PersonJoined`, and not `pessoaEntrou`
/// — so a bug report that says `PersonJoined` finds the same word in the
/// protocol, in `api/v1.json` and in the MOD, with nothing in between to
/// translate. `docs/glossario.md` governs what a person reads on screen; this
/// is what code reads.
///
/// `None` for events a MOD has no business seeing. Today that is the telemetry
/// — uplink, screen viewers, key frames: numbers that change many times a
/// second and would turn every MOD into a busy loop. `api/v1.json` does not
/// list them, and this is where that list is enforced.
#[must_use]
pub fn momento_de(evento: &crate::server::Event) -> Option<(&'static str, String)> {
    use crate::server::Event;

    let (nome, carga) = match evento {
        Event::PersonJoined {
            voice_room,
            profile,
            ..
        } => (
            "PersonJoined",
            format!(
                r#"{{"sala":{},"pessoa":{},"apelido":{}}}"#,
                voice_room.0,
                profile.id.0,
                texto_json(&profile.nickname)
            ),
        ),
        Event::PersonLeft {
            voice_room, person, ..
        } => (
            "PersonLeft",
            format!(r#"{{"sala":{},"pessoa":{}}}"#, voice_room.0, person.0),
        ),
        Event::MessagePosted(mensagem) => (
            "MessageReceived",
            format!(
                r#"{{"canal":{},"mensagem":{},"autor":{},"texto":{}}}"#,
                mensagem.channel.0,
                mensagem.id.0,
                mensagem.author.0,
                texto_json(&mensagem.body)
            ),
        ),
        Event::MessageEdited { channel, id, body } => (
            "MessageEdited",
            format!(
                r#"{{"canal":{},"mensagem":{},"texto":{}}}"#,
                channel.0,
                id.0,
                texto_json(body)
            ),
        ),
        Event::MessageRemoved { channel, id } => (
            "MessageRemoved",
            format!(r#"{{"canal":{},"mensagem":{}}}"#, channel.0, id.0),
        ),
        // Telemetria e o que mais não está no `api/v1.json`.
        _ => return None,
    };
    Some((nome, carga))
}

/// A JSON string, escaped by hand.
///
/// By hand and not by `serde_json` because the payload is built by `format!`
/// above and pulling a serialiser in for one field would be the heavier half of
/// the job. What it has to get right is what a nickname or a message body can
/// contain, and that is the four escapes below plus control characters.
fn texto_json(bruto: &str) -> String {
    let mut fora = String::with_capacity(bruto.len() + 2);
    fora.push('"');
    for c in bruto.chars() {
        match c {
            '"' => fora.push_str("\\\""),
            '\\' => fora.push_str("\\\\"),
            '\n' => fora.push_str("\\n"),
            '\r' => fora.push_str("\\r"),
            '\t' => fora.push_str("\\t"),
            c if (c as u32) < 0x20 => fora.push_str(&format!("\\u{:04x}", c as u32)),
            c => fora.push(c),
        }
    }
    fora.push('"');
    fora
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta(nome: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("seele-despacho-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    /// **Ponta a ponta, e é o teste que tira isto do laboratório.**
    ///
    /// Um evento de verdade, no barramento de verdade, com banco de verdade: o
    /// MOD é chamado, o quintal dele é gravado, e nada nos vinte e um lugares
    /// que emitem precisou mudar.
    #[tokio::test]
    async fn um_evento_do_barramento_chega_ao_mod_e_o_quintal_e_gravado() {
        use crate::persistence::{Location, Persistence};
        use seele_proto::ids::{PersonId, VoiceRoomId};

        let banco = Persistence::open(&Location::Memory).expect("banco");
        crate::persistence::mods::enable(
            &banco,
            &crate::persistence::mods::EnabledMod {
                id: "seele/contador".to_owned(),
                version: "1.0.0".to_owned(),
                hash: "h".to_owned(),
                repo: String::new(),
                reach: Vec::new(),
                server_half: true,
            },
        )
        .expect("ligar");
        let persistence = std::sync::Arc::new(tokio::sync::Mutex::new(banco));

        let (despachante, _) = Despachante::iniciar(vec![(
            "seele/contador".to_owned(),
            "globalThis.aoAcontecer = (m, c) => { \
               const o = JSON.parse(c); \
               dados.ultimo = m; \
               dados.sala = String(o.sala); \
             };"
            .to_owned(),
            pasta("barramento"),
        )]);

        let (bus, receptor) = tokio::sync::broadcast::channel(16);
        let acompanhando = tokio::spawn(acompanhar(
            receptor,
            std::sync::Arc::clone(&persistence),
            despachante,
            // Uma raiz que não existe: estes testes não recarregam, e uma raiz
            // vazia deixa isso dito em vez de apontar para a pasta de outro.
            crate::RaizesDosMods {
                pacotes: std::path::PathBuf::from("sem-pacotes-neste-teste"),
                dados: std::path::PathBuf::from("sem-dados-neste-teste"),
            },
        ));

        bus.send(crate::server::Event::PersonLeft {
            voice_room: VoiceRoomId(3),
            person: PersonId(7),
        })
        .expect("mandar");

        // O barramento é assíncrono: espera o quintal aparecer, com teto.
        let mut quintal = BTreeMap::new();
        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let banco = persistence.lock().await;
            quintal = crate::persistence::mods::ler_quintal(&banco, "seele/contador")
                .expect("ler quintal");
            if !quintal.is_empty() {
                break;
            }
        }

        assert_eq!(
            quintal.get("ultimo").map(String::as_str),
            Some("PersonLeft")
        );
        assert_eq!(quintal.get("sala").map(String::as_str), Some("3"));

        drop(bus);
        let _ = acompanhando.await;
    }

    /// **E um MOD que quebra é desabilitado no banco, não só na memória.**
    ///
    /// Se o desabilitar não fosse gravado, o arranque seguinte religaria o MOD,
    /// ele quebraria de novo, e quem hospeda veria a mesma linha rolar para
    /// sempre sem jeito de parar.
    #[tokio::test]
    async fn um_mod_que_quebra_e_desabilitado_no_banco() {
        use crate::persistence::{Location, Persistence};
        use seele_proto::ids::{PersonId, VoiceRoomId};

        let banco = Persistence::open(&Location::Memory).expect("banco");
        crate::persistence::mods::enable(
            &banco,
            &crate::persistence::mods::EnabledMod {
                id: "seele/ruim".to_owned(),
                version: "1.0.0".to_owned(),
                hash: "h".to_owned(),
                repo: String::new(),
                reach: Vec::new(),
                server_half: true,
            },
        )
        .expect("ligar");
        let persistence = std::sync::Arc::new(tokio::sync::Mutex::new(banco));

        let (despachante, _) = Despachante::iniciar(vec![(
            "seele/ruim".to_owned(),
            "globalThis.aoAcontecer = () => { throw new Error('sempre'); };".to_owned(),
            pasta("desabilita"),
        )]);

        let (bus, receptor) = tokio::sync::broadcast::channel(16);
        let acompanhando = tokio::spawn(acompanhar(
            receptor,
            std::sync::Arc::clone(&persistence),
            despachante,
            // Uma raiz que não existe: estes testes não recarregam, e uma raiz
            // vazia deixa isso dito em vez de apontar para a pasta de outro.
            crate::RaizesDosMods {
                pacotes: std::path::PathBuf::from("sem-pacotes-neste-teste"),
                dados: std::path::PathBuf::from("sem-dados-neste-teste"),
            },
        ));

        bus.send(crate::server::Event::PersonLeft {
            voice_room: VoiceRoomId(1),
            person: PersonId(1),
        })
        .expect("mandar");

        let mut ainda_ligado = true;
        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let banco = persistence.lock().await;
            ainda_ligado = !crate::persistence::mods::enabled(&banco)
                .expect("listar")
                .is_empty();
            if !ainda_ligado {
                break;
            }
        }

        assert!(
            !ainda_ligado,
            "o MOD que quebrou continuou habilitado no banco"
        );

        drop(bus);
        let _ = acompanhando.await;
    }

    /// **Um apelido hostil não escapa do JSON que chega ao MOD.**
    ///
    /// O apelido é escolhido por outra pessoa e atravessa até dentro do
    /// interpretador. Sem escape, `"},{"x":"` fecharia o objeto e o MOD leria
    /// campos que ninguém mandou — a injeção clássica, num lugar onde o
    /// atacante é quem entrou na sala.
    #[tokio::test]
    async fn um_apelido_hostil_nao_quebra_o_json_que_chega_ao_mod() {
        for hostil in [
            r#"aspas" e mais"#,
            r#"},{"invadi":"sim"#,
            "barra\\ invertida",
            "linha\nnova",
            "tabulação\tdentro",
            "controle\u{1}aqui",
        ] {
            let escapado = texto_json(hostil);
            let objeto = format!(r#"{{"apelido":{escapado}}}"#);

            let (despachante, _) = Despachante::iniciar(vec![(
                "seele/leitor".to_owned(),
                "globalThis.aoAcontecer = (m, c) => { \
                   const o = JSON.parse(c); \
                   dados.apelido = o.apelido; \
                   dados.campos = String(Object.keys(o).length); \
                 };"
                .to_owned(),
                pasta("json"),
            )]);

            let resposta = despachante
                .entregar(
                    "PersonJoined",
                    &objeto,
                    BTreeMap::from([("seele/leitor".to_owned(), Quintal::new())]),
                )
                .await
                .expect("a thread morreu");

            assert!(
                resposta.falharam.is_empty(),
                "`{hostil}` fez o JSON.parse do MOD falhar"
            );
            let quintal = &resposta.quintais["seele/leitor"].escritas;
            assert_eq!(
                quintal.get("apelido").map(String::as_str),
                Some(hostil),
                "`{hostil}` chegou diferente ao MOD"
            );
            assert_eq!(
                quintal.get("campos").map(String::as_str),
                Some("1"),
                "`{hostil}` acrescentou campo que ninguém mandou"
            );
        }
    }

    #[tokio::test]
    async fn um_momento_chega_a_todo_mod_ligado() {
        let (despachante, recusados) = Despachante::iniciar(vec![
            (
                "seele/um".to_owned(),
                "globalThis.aoAcontecer = (m) => { dados.viu = m; };".to_owned(),
                pasta("um"),
            ),
            (
                "seele/dois".to_owned(),
                "globalThis.aoAcontecer = (m) => { dados.viu = m + '!'; };".to_owned(),
                pasta("dois"),
            ),
        ]);
        assert!(recusados.is_empty());

        let resposta = despachante
            .entregar(
                "PersonJoined",
                "{}",
                BTreeMap::from([
                    ("seele/um".to_owned(), Quintal::new()),
                    ("seele/dois".to_owned(), Quintal::new()),
                ]),
            )
            .await
            .expect("a thread morreu");

        assert!(resposta.falharam.is_empty());
        assert_eq!(
            resposta.quintais["seele/um"]
                .escritas
                .get("viu")
                .map(String::as_str),
            Some("PersonJoined")
        );
        assert_eq!(
            resposta.quintais["seele/dois"]
                .escritas
                .get("viu")
                .map(String::as_str),
            Some("PersonJoined!")
        );
    }

    /// **A garantia central do ADR 0045:** um MOD que quebra é desabilitado e a
    /// sala continua. Aqui isso quer dizer que o vizinho dele é chamado do
    /// mesmo jeito, no mesmo momento.
    #[tokio::test]
    async fn um_mod_que_lanca_nao_leva_o_vizinho_junto() {
        let (despachante, _) = Despachante::iniciar(vec![
            (
                "seele/ruim".to_owned(),
                "globalThis.aoAcontecer = () => { throw new Error('eu'); };".to_owned(),
                pasta("ruim"),
            ),
            (
                "seele/bom".to_owned(),
                "globalThis.aoAcontecer = () => { dados.ok = 'sim'; };".to_owned(),
                pasta("bom"),
            ),
        ]);

        let resposta = despachante
            .entregar(
                "MessagePosted",
                "{}",
                BTreeMap::from([
                    ("seele/ruim".to_owned(), Quintal::new()),
                    ("seele/bom".to_owned(), Quintal::new()),
                ]),
            )
            .await
            .expect("a thread morreu");

        assert_eq!(resposta.falharam.len(), 1);
        assert_eq!(
            resposta.falharam.first().map(|(id, _)| id.as_str()),
            Some("seele/ruim")
        );
        assert_eq!(
            resposta.quintais["seele/bom"]
                .escritas
                .get("ok")
                .map(String::as_str),
            Some("sim"),
            "o MOD bom não foi chamado porque o vizinho quebrou"
        );
    }

    /// Um MOD que não compila é recusado ao subir, nomeado, e o servidor sobe
    /// mesmo assim. Um servidor que se recusa a hospedar porque um MOD tem erro
    /// de sintaxe é pior que uma sala sem aquele MOD.
    #[tokio::test]
    async fn um_mod_que_nao_compila_e_nomeado_e_o_resto_sobe() {
        let (despachante, recusados) = Despachante::iniciar(vec![
            (
                "seele/quebrado".to_owned(),
                "isto ( não é javascript".to_owned(),
                pasta("quebrado"),
            ),
            (
                "seele/inteiro".to_owned(),
                "globalThis.aoAcontecer = () => { dados.ok = 'sim'; };".to_owned(),
                pasta("inteiro"),
            ),
        ]);

        assert_eq!(recusados.len(), 1);
        assert_eq!(
            recusados.first().map(|(id, _)| id.as_str()),
            Some("seele/quebrado")
        );

        let resposta = despachante
            .entregar(
                "PersonJoined",
                "{}",
                BTreeMap::from([("seele/inteiro".to_owned(), Quintal::new())]),
            )
            .await
            .expect("a thread morreu");
        assert_eq!(
            resposta.quintais["seele/inteiro"]
                .escritas
                .get("ok")
                .map(String::as_str),
            Some("sim")
        );
    }

    /// Um laço infinito num MOD não segura o barramento: o teto corta, o MOD
    /// entra na lista de falhas, e o momento seguinte é entregue.
    #[tokio::test]
    async fn um_mod_eterno_nao_segura_a_fila() {
        let (despachante, _) = Despachante::iniciar(vec![(
            "seele/eterno".to_owned(),
            "globalThis.aoAcontecer = () => { while (true) {} };".to_owned(),
            pasta("eterno"),
        )]);

        let inicio = std::time::Instant::now();
        let resposta = despachante
            .entregar(
                "PersonJoined",
                "{}",
                BTreeMap::from([("seele/eterno".to_owned(), Quintal::new())]),
            )
            .await
            .expect("a thread morreu");
        assert_eq!(resposta.falharam.len(), 1);

        // E a fila segue: o segundo pedido é atendido.
        assert!(despachante
            .entregar("PersonLeft", "{}", BTreeMap::new())
            .await
            .is_some());
        assert!(
            inicio.elapsed() < std::time::Duration::from_secs(10),
            "o MOD eterno segurou a fila por {:?}",
            inicio.elapsed()
        );
    }

    /// **Um evento lento não apaga o que um pedido escreveu no meio.**
    ///
    /// A corrida que este desenho fecha, contada pelos tempos: o caminho de
    /// eventos lê o quintal, solta o cadeado do banco — um MOD lento não pode
    /// segurar quem está conversando —, roda no QuickJS, e só então volta para
    /// gravar. Um pedido que chegue nesse meio lê, roda e grava antes.
    ///
    /// Com o mapa inteiro de volta, a gravação do evento usava um retrato
    /// tirado **antes de o pedido existir**, e o apagava. Com a diferença, ela
    /// aplica sobre o que está no banco agora.
    #[test]
    fn a_mudanca_de_um_evento_aplica_sobre_o_que_esta_no_banco_agora() {
        use super::MudancaNoQuintal;
        use std::collections::BTreeMap;

        // O quintal como o evento o leu.
        let antes: BTreeMap<String, String> =
            [("a".to_owned(), "1".to_owned())].into_iter().collect();
        // O evento mexeu em `b`, e não tocou em `a`.
        let depois: BTreeMap<String, String> = [
            ("a".to_owned(), "1".to_owned()),
            ("b".to_owned(), "do evento".to_owned()),
        ]
        .into_iter()
        .collect();
        let mudanca = MudancaNoQuintal::entre(&antes, &depois);

        // Enquanto o MOD rodava, um pedido escreveu em `a` e em `c`.
        let mut agora: BTreeMap<String, String> = [
            ("a".to_owned(), "do pedido".to_owned()),
            ("c".to_owned(), "do pedido".to_owned()),
        ]
        .into_iter()
        .collect();

        mudanca.aplicar(&mut agora);

        assert_eq!(
            agora.get("a").map(String::as_str),
            Some("do pedido"),
            "o evento apagou o que o pedido escreveu numa chave que ele nem tocou"
        );
        assert_eq!(agora.get("b").map(String::as_str), Some("do evento"));
        assert_eq!(
            agora.get("c").map(String::as_str),
            Some("do pedido"),
            "o evento apagou uma chave que só o pedido conhecia"
        );
    }

    /// E apagar continua apagando: o MOD que remove uma chave tem a remoção
    /// levada, e não só as escritas.
    #[test]
    fn uma_chave_que_o_mod_apagou_e_apagada_de_verdade() {
        use super::MudancaNoQuintal;
        use std::collections::BTreeMap;

        let antes: BTreeMap<String, String> = [
            ("fica".to_owned(), "1".to_owned()),
            ("sai".to_owned(), "2".to_owned()),
        ]
        .into_iter()
        .collect();
        let depois: BTreeMap<String, String> =
            [("fica".to_owned(), "1".to_owned())].into_iter().collect();

        let mudanca = MudancaNoQuintal::entre(&antes, &depois);
        assert_eq!(mudanca.apagadas, vec!["sai".to_owned()]);

        let mut agora = antes.clone();
        mudanca.aplicar(&mut agora);
        assert!(!agora.contains_key("sai"), "a remoção não atravessou");
        assert!(agora.contains_key("fica"));
    }

    /// Um MOD que não mexeu em nada não gera gravação nenhuma: sem isto, todo
    /// evento reescreveria todo quintal de todo MOD habilitado.
    #[test]
    fn quem_nao_mexeu_em_nada_nao_grava() {
        use super::MudancaNoQuintal;
        use std::collections::BTreeMap;

        let igual: BTreeMap<String, String> =
            [("a".to_owned(), "1".to_owned())].into_iter().collect();
        assert!(MudancaNoQuintal::entre(&igual, &igual).vazia());
    }

    /// **Ligar um MOD com o servidor de pé faz ele passar a reagir a evento.**
    ///
    /// P1 do plano de isolamento: «runtime de eventos e runtime de pedidos
    /// podem divergir do conjunto». O despachante nascia no arranque com as
    /// fontes de então e ficava com elas. Os pedidos resolviam pelo banco a
    /// cada chamada e o anúncio também — só os **eventos** continuavam com o
    /// código antigo. Um MOD recém-ligado era mudo aos eventos, e um
    /// recém-desligado continuava reagindo, até alguém reiniciar o servidor.
    ///
    /// O sinal já existia: `mods_mudaram`, que a transação do SALVAR bumpa uma
    /// vez. Faltava alguém deste lado observá-lo.
    #[tokio::test(flavor = "multi_thread")]
    async fn ligar_um_mod_com_o_servidor_de_pe_faz_ele_reagir_a_evento() {
        use crate::persistence::{Location, Persistence};
        use seele_proto::ids::{PersonId, VoiceRoomId};

        let raiz = pasta("recarrega");
        // O pacote em disco, endereçado pelo conteúdo, como o instalador o
        // deixa — é daí que a recarga vai lê-lo.
        let obras = raiz.join("em-obras");
        std::fs::create_dir_all(obras.join("servidor")).expect("criar");
        // **A API vem da constante.** Cravada, ela faz este teste reprovar no dia
        // em que a versão sobe — e reprovar dizendo «o MOD continuou mudo aos
        // eventos», que fala do despachante e é sobre o manifesto do fixture.
        let api = seele_proto::mods::MOD_API_VERSION;
        std::fs::write(
            obras.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"seele/tardio","version":"1.0.0","api":{api},
                "repo":"https://example.invalid/t","reach":["estado no servidor"],
                "server":"servidor/main.js"}}"#
            ),
        )
        .expect("manifesto");
        std::fs::write(
            obras.join("servidor/main.js"),
            b"globalThis.aoAcontecer = (m) => { dados.viu = m; };",
        )
        .expect("fonte");
        // O mesmo hash que o `seele-proto` calcula, que é o que o produto usa.
        let mut arquivos = vec![
            (
                "mod.json".to_owned(),
                std::fs::read(obras.join("mod.json")).expect("ler manifesto"),
            ),
            (
                "servidor/main.js".to_owned(),
                std::fs::read(obras.join("servidor/main.js")).expect("ler fonte"),
            ),
        ];
        let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
        let destino = raiz.join("mod-packages").join(&hash);
        std::fs::create_dir_all(destino.parent().expect("pai")).expect("raiz");
        std::fs::rename(&obras, &destino).expect("publicar");

        // O servidor sobe **sem MOD nenhum** ligado.
        let banco = Persistence::open(&Location::Memory).expect("banco");
        let persistence = std::sync::Arc::new(tokio::sync::Mutex::new(banco));
        let (despachante, _) = Despachante::iniciar(Vec::new());
        let (bus, receptor) = tokio::sync::broadcast::channel(16);
        let acompanhando = tokio::spawn(acompanhar(
            receptor,
            std::sync::Arc::clone(&persistence),
            despachante,
            crate::RaizesDosMods {
                pacotes: raiz.clone(),
                dados: raiz.join("mod-data"),
            },
        ));

        // E o MOD é ligado depois, como o SALVAR da tela faz.
        {
            let banco = persistence.lock().await;
            crate::persistence::mods::enable(
                &banco,
                &crate::persistence::mods::EnabledMod {
                    id: "seele/tardio".to_owned(),
                    version: "1.0.0".to_owned(),
                    hash: hash.clone(),
                    repo: String::new(),
                    reach: Vec::new(),
                    server_half: true,
                },
            )
            .expect("ligar");
        }

        // Um evento depois disso tem de alcançá-lo.
        //
        // **Oito segundos, e não dois.** Este teste reprovou uma vez na bateria
        // do workspace inteiro — `left: None`, o MOD mudo — e não se reproduziu
        // depois: vinte corridas isoladas e doze com os quinhentos e dez do
        // `--lib` juntos, todas verdes. O que difere entre as duas situações é
        // quantos binários de teste disputam a máquina ao mesmo tempo, e o que
        // este laço espera é uma máquina de JavaScript subir numa thread que
        // pode não estar recebendo fatia nenhuma.
        //
        // O orçamento maior não custa nada quando passa — sai no primeiro
        // acerto — e custa seis segundos a mais só quando já vai reprovar. O
        // que ele **não** faz é descartar a outra explicação: se voltar a
        // reprovar com oito segundos, a lentidão deixa de servir de resposta e
        // o que sobra é ordem, no despachante.
        let mut viu = None;
        for _ in 0..200 {
            let _ = bus.send(crate::server::Event::PersonLeft {
                voice_room: VoiceRoomId(1),
                person: PersonId(7),
            });
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            let banco = persistence.lock().await;
            let quintal =
                crate::persistence::mods::ler_quintal(&banco, "seele/tardio").unwrap_or_default();
            if let Some(v) = quintal.get("viu") {
                viu = Some(v.clone());
                break;
            }
        }
        acompanhando.abort();
        let _ = std::fs::remove_dir_all(&raiz);

        assert_eq!(
            viu.as_deref(),
            Some("PersonLeft"),
            "o MOD ligado com o servidor de pé continuou mudo aos eventos por oito segundos: o \
             despachante ficou com o código do arranque"
        );
    }

    /// **Um MOD exigido e sem código não é desligado sozinho.**
    ///
    /// O conjunto vive no banco e o código vive na thread dos MODs. Eles podem
    /// divergir: o pacote pode não ter chegado ao disco ainda, a recarga pode
    /// estar a caminho, ou o pacote pode ter sido apagado por fora.
    ///
    /// Antes, um evento nesse estado chamava um MOD que não estava carregado,
    /// recebia falha, e o caminho de eventos trata falha como «o MOD quebrou»
    /// — desabilitando no banco. O produto desligava sozinho o MOD que alguém
    /// acabara de ligar, sem ninguém pedir e sem dizer por quê, e o conjunto
    /// exigido mudava por conta própria.
    ///
    /// Encontrado por teste: o teste da recarga ao vivo falhava com o contador
    /// de mudanças em **2** — o segundo bump era o produto se desligando.
    #[tokio::test(flavor = "multi_thread")]
    async fn um_mod_exigido_sem_codigo_no_disco_nao_e_desligado_pelo_evento() {
        use crate::persistence::{Location, Persistence};
        use seele_proto::ids::{PersonId, VoiceRoomId};

        let raiz = pasta("sem-codigo");
        let banco = Persistence::open(&Location::Memory).expect("banco");
        crate::persistence::mods::enable(
            &banco,
            &crate::persistence::mods::EnabledMod {
                id: "seele/fantasma".to_owned(),
                version: "1.0.0".to_owned(),
                // Um hash que não existe em disco nenhum.
                hash: "f".repeat(64),
                repo: String::new(),
                reach: Vec::new(),
                server_half: true,
            },
        )
        .expect("ligar");
        let persistence = std::sync::Arc::new(tokio::sync::Mutex::new(banco));

        let (despachante, _) = Despachante::iniciar(Vec::new());
        let (bus, receptor) = tokio::sync::broadcast::channel(16);
        let acompanhando = tokio::spawn(acompanhar(
            receptor,
            std::sync::Arc::clone(&persistence),
            despachante,
            crate::RaizesDosMods {
                pacotes: raiz.clone(),
                dados: raiz.join("mod-data"),
            },
        ));

        for _ in 0..5 {
            let _ = bus.send(crate::server::Event::PersonLeft {
                voice_room: VoiceRoomId(1),
                person: PersonId(7),
            });
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }

        let continua_ligado = {
            let banco = persistence.lock().await;
            crate::persistence::mods::enabled(&banco)
                .expect("ler")
                .iter()
                .any(|m| m.id == "seele/fantasma")
        };
        acompanhando.abort();
        let _ = std::fs::remove_dir_all(&raiz);

        assert!(
            continua_ligado,
            "o produto desligou sozinho um MOD que ninguém pediu para desligar: \
             o conjunto exigido mudou por conta própria"
        );
    }
}
