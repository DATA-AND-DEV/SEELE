//! **O poder `programa` na metade de máquina, no app** (API 8 parte 4).
//!
//! A metade de máquina pede `maquina.programa.baixar`, `escrever`,
//! `desempacotar` e `rodar`; o pedido chega aqui pela bomba, e quem faz é o
//! núcleo (`seele_ffi::programa`), com um [`Programa`] por metade de pé.
//!
//! # A política sai do aceite guardado, e só dele
//!
//! [`politica_para`] lê o aceite que o dono deu **nesta máquina** e que está
//! no disco, e só o aceita se o resumo e o pacote são os do pedido que subiu
//! esta metade. Nada do que a metade pede em tempo de execução entra na
//! política: `maquina.oferta` é uma cópia para ela ler, e reescrevê-la não
//! muda nada. Sem aceite que confira, a metade sobe sem `Programa`, e todo
//! pedido do `programa` é recusado com o motivo ([`SemPrograma`]), contado e
//! dito no log.
//!
//! # Nada fica rodando
//!
//! Cada processo tem uma tarefa que lê a saída dele e a entrega à metade,
//! esperando a fila do executor quando ela enche (o jogo não perde linha por
//! o MOD estar ocupado). Parar a metade, por qualquer motivo, chama
//! [`Programa::encerrar_tudo`] e **espera** (`Viva::encerrar`); soltar a
//! metade solta o `Programa`, que manda matar o que sobrou.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use seele_ffi::mods::AceiteDeMaquinaGuardado;
use seele_ffi::programa::{
    AlgoritmoDeHash, EventoDoDownload, EventoDoProcesso, HashDeclarado, PedidoDeDesempacotar,
    PedidoDeDownload, PedidoDeExecucao, Politica, Programa, Recusa,
};
use tokio::sync::{mpsc, oneshot};

use super::Viva;

/// O `Programa` de uma metade, ou por que ela não tem um.
pub(crate) type ProgramaDaMetade = Result<Arc<Programa>, SemPrograma>;

/// Por que uma metade de pé não tem `Programa`. O nome vai na recusa de cada
/// pedido, e no log ao subir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SemPrograma {
    /// O manifesto não declara o poder `programa`.
    SemOPoder,
    /// Nenhum aceite guardado para esta chave.
    SemAceite,
    /// O aceite guardado é de outra oferta ou de outro pacote: a versão, o
    /// pack ou o comando mudaram, e o dono ainda não aceitou o novo.
    AceiteDiferente,
    /// O aceite guardado não monta uma política (oferta inválida, endereço).
    AceiteInvalido,
    /// O núcleo não montou o `Programa` (TLS, disco).
    NaoMontou,
}

impl SemPrograma {
    pub(crate) const fn nome(self) -> &'static str {
        match self {
            Self::SemOPoder => "sem-o-poder",
            Self::SemAceite => "sem-aceite",
            Self::AceiteDiferente => "aceite-diferente",
            Self::AceiteInvalido => "aceite-invalido",
            Self::NaoMontou => "programa-nao-montou",
        }
    }
}

/// **A política só do aceite guardado**, e só se ele é o deste pedido.
///
/// `resumo` e `hash` são os do pedido que subiu a metade; o aceite guardado
/// tem de ter os dois iguais. É o que faz «mudar a versão ou o pack» não
/// rodar até o aceite novo: a oferta nova tem outro resumo, e o aceite velho
/// não vale para ela.
///
/// # Errors
///
/// [`SemPrograma::SemAceite`], [`SemPrograma::AceiteDiferente`] e
/// [`SemPrograma::AceiteInvalido`].
pub(crate) fn politica_para(
    resumo: &str,
    hash: &str,
    guardado: Option<&AceiteDeMaquinaGuardado>,
    home: &str,
) -> Result<Politica, SemPrograma> {
    let guardado = guardado.ok_or(SemPrograma::SemAceite)?;
    if guardado.resumo != resumo || guardado.hash != hash {
        return Err(SemPrograma::AceiteDiferente);
    }
    seele_ffi::programa::politica_do_aceite(home, guardado).map_err(|_| SemPrograma::AceiteInvalido)
}

/// O `Programa` da metade que sobe, lido do disco agora.
pub(crate) fn programa_para(
    capacidades: &[&str],
    resumo: &str,
    hash: &str,
    guardado: Option<&AceiteDeMaquinaGuardado>,
    home: &str,
) -> ProgramaDaMetade {
    if !capacidades.contains(&"maquina.programa") {
        return Err(SemPrograma::SemOPoder);
    }
    let politica = politica_para(resumo, hash, guardado, home)?;
    let origem: Arc<dyn seele_ffi::programa::Origem> =
        Arc::new(seele_ffi::programa::OrigemHttps::nova().map_err(|_| SemPrograma::NaoMontou)?);
    Programa::novo(politica, origem)
        .map(Arc::new)
        .map_err(|_| SemPrograma::NaoMontou)
}

// ------------------------------------------------------------ o pedido

/// O que a metade pediu ao `programa`, já lido do JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PedidoDoPrograma {
    Baixar(PedidoDeDownload),
    Escrever {
        destino: String,
        texto: String,
    },
    Desempacotar(PedidoDeDesempacotar),
    Rodar(PedidoDeExecucao),
    /// O pack que a metade de servidor guardou (API 8 parte 6): `pedido` vai
    /// ao `aoPedir` dela, que responde com o caminho no volume.
    Trazer {
        pedido: String,
        destino: String,
    },
    /// Um texto da pasta do servidor de jogo (o índice do pack).
    Ler {
        arquivo: String,
    },
    /// **A cópia do mundo** (API 8 parte 7): uma pasta num `.tar.gz`.
    Empacotar {
        origem: String,
        arquivo: String,
    },
    /// Oferece um pacote na porta de um túnel TCP aceito.
    OferecerCopia {
        arquivo: String,
        porta: u16,
    },
    /// Recebe a cópia de quem é a dona, pelo túnel `tunel` da oferta.
    ReceberCopia {
        tunel: String,
        codigo: String,
        sha256: String,
        tamanho: u64,
        destino: String,
    },
    EscreverNoProcesso {
        processo: u64,
        texto: String,
    },
    Matar {
        processo: u64,
    },
    Recusas,
}

/// Os tipos do `programa`, para `tratar` saber que são dele.
pub(crate) const TIPOS: &[&str] = &[
    "programa-baixar",
    "programa-escrever",
    "programa-desempacotar",
    "programa-rodar",
    "processo-escrever",
    "processo-matar",
    "programa-recusas",
    "programa-trazer",
    "programa-ler",
    "programa-empacotar",
    "programa-oferecer-copia",
    "programa-receber-copia",
];

/// Lê o pedido de um dos [`TIPOS`]; `pedido-invalido` quando falta um campo.
///
/// # Errors
///
/// `pedido-invalido`.
pub(crate) fn ler(tipo: &str, valor: &serde_json::Value) -> Result<PedidoDoPrograma, &'static str> {
    let texto = |campo: &str| {
        valor
            .get(campo)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let numero = |campo: &str| valor.get(campo).and_then(serde_json::Value::as_u64);
    let invalido = "pedido-invalido";
    Ok(match tipo {
        "programa-baixar" => {
            let hashes = [
                ("sha1", AlgoritmoDeHash::Sha1),
                ("sha256", AlgoritmoDeHash::Sha256),
                ("sha512", AlgoritmoDeHash::Sha512),
            ]
            .into_iter()
            .filter_map(|(campo, algoritmo)| {
                texto(campo).map(|hex| HashDeclarado { algoritmo, hex })
            })
            .collect();
            PedidoDoPrograma::Baixar(PedidoDeDownload {
                url: texto("url").ok_or(invalido)?,
                hashes,
                tamanho: numero("tamanho").ok_or(invalido)?,
                destino: texto("destino").ok_or(invalido)?,
            })
        }
        "programa-escrever" => PedidoDoPrograma::Escrever {
            destino: texto("destino").ok_or(invalido)?,
            texto: texto("texto").ok_or(invalido)?,
        },
        "programa-desempacotar" => PedidoDoPrograma::Desempacotar(PedidoDeDesempacotar {
            arquivo: texto("arquivo").ok_or(invalido)?,
            destino: texto("destino").ok_or(invalido)?,
            raiz: texto("raiz").unwrap_or_default(),
            mesclar: valor
                .get("mesclar")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }),
        "programa-rodar" => PedidoDoPrograma::Rodar(PedidoDeExecucao {
            comando: texto("comando").ok_or(invalido)?,
            argumentos: valor
                .get("argumentos")
                .and_then(serde_json::Value::as_array)
                .ok_or(invalido)?
                .iter()
                .map(|a| a.as_str().map(str::to_owned).ok_or(invalido))
                .collect::<Result<_, _>>()?,
            pasta: texto("pasta").unwrap_or_default(),
        }),
        "processo-escrever" => PedidoDoPrograma::EscreverNoProcesso {
            processo: numero("processo").ok_or(invalido)?,
            texto: texto("texto").ok_or(invalido)?,
        },
        "processo-matar" => PedidoDoPrograma::Matar {
            processo: numero("processo").ok_or(invalido)?,
        },
        "programa-recusas" => PedidoDoPrograma::Recusas,
        "programa-trazer" => PedidoDoPrograma::Trazer {
            pedido: serde_json::to_string(valor.get("pedido").unwrap_or(&serde_json::Value::Null))
                .unwrap_or_else(|_| "null".to_owned()),
            destino: texto("destino").ok_or(invalido)?,
        },
        "programa-ler" => PedidoDoPrograma::Ler {
            arquivo: texto("arquivo").ok_or(invalido)?,
        },
        "programa-empacotar" => PedidoDoPrograma::Empacotar {
            origem: texto("origem").ok_or(invalido)?,
            arquivo: texto("arquivo").ok_or(invalido)?,
        },
        "programa-oferecer-copia" => PedidoDoPrograma::OferecerCopia {
            arquivo: texto("arquivo").ok_or(invalido)?,
            porta: numero("porta")
                .and_then(|p| u16::try_from(p).ok())
                .ok_or(invalido)?,
        },
        "programa-receber-copia" => PedidoDoPrograma::ReceberCopia {
            tunel: texto("tunel").ok_or(invalido)?,
            codigo: texto("codigo").ok_or(invalido)?,
            sha256: texto("sha256").ok_or(invalido)?,
            tamanho: numero("tamanho").ok_or(invalido)?,
            destino: texto("destino").ok_or(invalido)?,
        },
        _ => return Err("tipo-desconhecido"),
    })
}

// ------------------------------------------------------------ a recusa

/// **A recusa com os dados dela**, como a metade a lê num `catch`: o nome em
/// `erro`, a frase inteira em `frase` (é ela que vai à tela do MOD e ao
/// log), e os números que a pessoa precisa em campos.
pub(crate) fn resposta_da_recusa(n: u64, recusa: &Recusa) -> serde_json::Value {
    let mut r = serde_json::json!({
        "n": n,
        "ok": false,
        "erro": recusa.nome(),
        "frase": recusa.to_string(),
    });
    let campos = match recusa {
        Recusa::DominioForaDoAceito { host, aceitos } => {
            serde_json::json!({ "host": host, "aceitos": aceitos })
        }
        Recusa::RedirecionouParaFora { de, para } => serde_json::json!({ "de": de, "para": para }),
        Recusa::HashNaoBate {
            url,
            algoritmo,
            esperado,
            obtido,
        } => {
            serde_json::json!({ "url": url, "algoritmo": algoritmo.nome(), "esperado": esperado, "obtido": obtido })
        }
        Recusa::DiscoCheio {
            usado,
            pedido,
            teto,
        } => {
            serde_json::json!({ "usado": usado, "pedido": pedido, "teto": teto })
        }
        Recusa::DiferenteDoAceito {
            url,
            campo,
            aceito,
            pedido,
        } => serde_json::json!({ "url": url, "campo": campo, "aceito": aceito, "pedido": pedido }),
        Recusa::ForaDosArtefatosAceitos { url, origens } => {
            serde_json::json!({ "url": url, "origens": origens })
        }
        Recusa::ArtefatoDeOutroSistema { url } => serde_json::json!({ "url": url }),
        Recusa::ComandoForaDoAceito { pedido, aceito } => {
            serde_json::json!({ "pedido": pedido, "aceito": aceito })
        }
        Recusa::ArgumentosForaDoAceito { pedidos, aceitos } => {
            serde_json::json!({ "pedidos": pedidos, "aceitos": aceitos })
        }
        Recusa::OrigemRespondeu { url, status } => {
            serde_json::json!({ "url": url, "status": status })
        }
        Recusa::TamanhoAlegadoDiferente {
            url,
            declarado,
            alegado,
        } => serde_json::json!({ "url": url, "declarado": declarado, "alegado": alegado }),
        Recusa::PackDiferente {
            algoritmo,
            esperado,
            obtido,
        } => {
            serde_json::json!({ "algoritmo": algoritmo.nome(), "esperado": esperado, "obtido": obtido })
        }
        Recusa::TextoGrandeDemais { tamanho, teto } => {
            serde_json::json!({ "tamanho": tamanho, "teto": teto })
        }
        Recusa::NaoETexto { arquivo } => serde_json::json!({ "arquivo": arquivo }),
        Recusa::SemPackAceito { destino } => serde_json::json!({ "destino": destino }),
        Recusa::NaoEPasta { origem } => serde_json::json!({ "origem": origem }),
        Recusa::ArquivoDentroDaOrigem { arquivo, origem } => {
            serde_json::json!({ "arquivo": arquivo, "origem": origem })
        }
        Recusa::PortaForaDoAceite { porta, aceitas } => {
            serde_json::json!({ "porta": porta, "aceitas": aceitas })
        }
        Recusa::PortaOcupada { porta, motivo } => {
            serde_json::json!({ "porta": porta, "motivo": motivo })
        }
        Recusa::CopiaRecusada { motivo } => serde_json::json!({ "motivo": motivo }),
        Recusa::CopiaCaiu {
            recebidos,
            tamanho,
            tentativas,
            motivo,
        } => {
            serde_json::json!({ "recebidos": recebidos, "tamanho": tamanho, "tentativas": tentativas, "motivo": motivo })
        }
        Recusa::CopiaNaoBate { esperado, obtido } => {
            serde_json::json!({ "esperado": esperado, "obtido": obtido })
        }
        _ => serde_json::Value::Null,
    };
    if let (Some(r), Some(campos)) = (r.as_object_mut(), campos.as_object()) {
        for (k, v) in campos {
            r.insert(k.clone(), v.clone());
        }
    }
    r
}

// ------------------------------------------------------------ os processos

/// O que a tarefa de um processo recebe de fora.
pub(crate) enum Comando {
    Escrever(String, oneshot::Sender<Result<(), Recusa>>),
    Matar,
}

/// Os processos de uma metade, pelo número que o núcleo deu.
#[derive(Default)]
pub(crate) struct Processos(Mutex<HashMap<u64, mpsc::UnboundedSender<Comando>>>);

impl Processos {
    fn mapa(&self) -> std::sync::MutexGuard<'_, HashMap<u64, mpsc::UnboundedSender<Comando>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// As recusas que a cola deu sozinha (sem `Programa`, pedido mal formado,
/// processo que não existe), somadas às do núcleo em `programa-recusas`.
#[derive(Default)]
pub(crate) struct RecusasDaCola(Mutex<BTreeMap<&'static str, u64>>);

impl RecusasDaCola {
    pub(crate) fn contar(&self, nome: &'static str) {
        *self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(nome)
            .or_default() += 1;
    }

    fn todas(&self) -> BTreeMap<&'static str, u64> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn todas_para_o_teste(&self) -> BTreeMap<&'static str, u64> {
        self.todas()
    }
}

/// Quanto uma linha espera a fila do executor esvaziar antes de ser perdida
/// (e dita no log).
const ESPERA_DA_FILA: Duration = Duration::from_secs(30);

/// Entrega uma mensagem à metade **esperando a fila** quando ela enche; para
/// quando o executor caiu, ou depois de [`ESPERA_DA_FILA`].
///
/// **Também parando**: o fim do processo é o que o `aoParar` espera para
/// terminar, e perdê-lo por a fila estar cheia faria a parada custar o teto.
async fn entregar_esperando(viva: &Viva, json: &str) {
    let ate = std::time::Instant::now() + ESPERA_DA_FILA;
    loop {
        match viva.tentar_entregar(json) {
            Ok(()) => return,
            Err(motivo) if motivo.contains("cheia") && std::time::Instant::now() < ate => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(motivo) => {
                tracing::warn!(mod_id = %viva.chave.mod_id, servidor = %viva.chave.servidor, motivo, "metade de máquina: linha do processo não entregue");
                return;
            }
        }
    }
}

/// **A tarefa de um processo**: cada linha e o fim vão à metade, e a entrada
/// e o matar chegam por `comandos`. Vive até o processo terminar.
async fn vigiar(
    viva: Arc<Viva>,
    mut processo: seele_ffi::programa::Processo,
    mut comandos: mpsc::UnboundedReceiver<Comando>,
) {
    let numero = processo.numero();
    let (mod_id, servidor) = (viva.chave.mod_id.clone(), viva.chave.servidor.clone());
    loop {
        tokio::select! {
            evento = processo.proximo() => {
                let Some(evento) = evento else { break };
                match evento {
                    EventoDoProcesso::Saida(linha) => {
                        let json = serde_json::json!({ "tipo": "processo-linha", "processo": numero, "fluxo": "saida", "linha": linha });
                        entregar_esperando(&viva, &json.to_string()).await;
                    }
                    EventoDoProcesso::Erro(linha) => {
                        let json = serde_json::json!({ "tipo": "processo-linha", "processo": numero, "fluxo": "erro", "linha": linha });
                        entregar_esperando(&viva, &json.to_string()).await;
                    }
                    EventoDoProcesso::Terminou(fim) => {
                        // Dito no log do produto, e não só à metade: um jogo
                        // que caiu é o que a pessoa da máquina pergunta depois.
                        if fim.morto_a_pedido {
                            tracing::info!(mod_id = %mod_id, servidor = %servidor, processo = numero, codigo = ?fim.codigo, "processo do servidor de jogo encerrado a pedido");
                        } else {
                            tracing::warn!(mod_id = %mod_id, servidor = %servidor, processo = numero, codigo = ?fim.codigo, linhas_perdidas = fim.linhas_perdidas, linhas_cortadas = fim.linhas_cortadas, "processo do servidor de jogo terminou sozinho");
                        }
                        viva.processos.mapa().remove(&numero);
                        let json = serde_json::json!({
                            "tipo": "processo-fim",
                            "processo": numero,
                            "codigo": fim.codigo,
                            "mortoAPedido": fim.morto_a_pedido,
                            "linhasPerdidas": fim.linhas_perdidas,
                            "linhasCortadas": fim.linhas_cortadas,
                        });
                        entregar_esperando(&viva, &json.to_string()).await;
                        break;
                    }
                }
            }
            comando = comandos.recv() => match comando {
                Some(Comando::Escrever(texto, volta)) => {
                    let _ = volta.send(processo.escrever(&texto).await);
                }
                Some(Comando::Matar) => processo.matar(),
                // Ninguém mais manda nada (a metade parou): o processo segue
                // até o `encerrar_tudo` o matar e o `Terminou` chegar.
                None => {}
            },
        }
    }
    viva.processos.mapa().remove(&numero);
}

/// **Faz o pedido do `programa`.** Responde `n` com o resultado ou com a
/// recusa e os dados dela.
#[allow(clippy::too_many_lines)]
pub(crate) fn atender(viva: &Arc<Viva>, n: u64, pedido: PedidoDoPrograma) {
    let (mod_id, servidor) = (viva.chave.mod_id.clone(), viva.chave.servidor.clone());
    let programa = match &viva.programa {
        Ok(programa) => Arc::clone(programa),
        Err(sem) => {
            // **Sem aceite, nada baixa e nada roda** — e é dito e contado.
            let nome = sem.nome();
            viva.recusas_da_cola.contar(nome);
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo = nome, pedido = ?pedido, "programa recusado: a metade não tem um aceite que valha");
            viva.responder(n, Err(nome));
            return;
        }
    };
    // **Parando, nada novo começa** — mas o que já roda ainda ouve: o
    // `aoParar` manda `stop` ao jogo e espera ele sair, e é isso que salva o
    // mundo antes de `encerrar_tudo` matar (decisão Q9; medido no Minecraft).
    let comeca_algo = !matches!(
        pedido,
        PedidoDoPrograma::EscreverNoProcesso { .. }
            | PedidoDoPrograma::Matar { .. }
            | PedidoDoPrograma::Recusas
    );
    if comeca_algo && !viva.de_pe() {
        viva.recusas_da_cola.contar("parando");
        viva.responder(n, Err("parando"));
        return;
    }
    let recusar = move |viva: &Viva, recusa: &Recusa| {
        viva.entregar(&resposta_da_recusa(n, recusa).to_string());
    };
    match pedido {
        PedidoDoPrograma::Baixar(pedido) => {
            let dela = Arc::clone(viva);
            let destino = pedido.destino.clone();
            tauri::async_runtime::spawn(async move {
                let mut download = match programa.baixar(pedido) {
                    Ok(download) => download,
                    Err(recusa) => return recusar(&dela, &recusa),
                };
                while let Some(evento) = download.proximo().await {
                    match evento {
                        // O progresso não espera a fila: um que não coube é
                        // trocado pelo próximo, que diz mais.
                        EventoDoDownload::Progresso { recebidos, total } => {
                            let _ = dela.tentar_entregar(
                                &serde_json::json!({ "tipo": "programa-progresso", "n": n, "recebidos": recebidos, "total": total }).to_string(),
                            );
                        }
                        EventoDoDownload::Pronto(baixado) => {
                            dela.responder(
                                n,
                                Ok(serde_json::json!({
                                    "destino": destino,
                                    "tamanho": baixado.tamanho,
                                    "algoritmo": baixado.algoritmo.nome(),
                                    "hash": baixado.hash,
                                    "jaEstava": baixado.ja_estava,
                                })),
                            );
                            return;
                        }
                        EventoDoDownload::Recusado(recusa) => return recusar(&dela, &recusa),
                    }
                }
                recusar(&dela, &Recusa::Encerrado);
            });
        }
        PedidoDoPrograma::Escrever { destino, texto } => {
            match programa.escrever(&destino, &texto) {
                Ok(_) => viva.responder(
                    n,
                    Ok(serde_json::json!({ "destino": destino, "bytes": texto.len() })),
                ),
                Err(recusa) => recusar(viva, &recusa),
            }
        }
        PedidoDoPrograma::Desempacotar(pedido) => {
            let dela = Arc::clone(viva);
            tauri::async_runtime::spawn(async move {
                let feito =
                    tokio::task::spawn_blocking(move || programa.desempacotar(&pedido)).await;
                match feito {
                    Ok(Ok(feito)) => dela.responder(
                        n,
                        Ok(serde_json::json!({
                            "arquivos": feito.arquivos,
                            "bytes": feito.bytes,
                            "linksPulados": feito.links_pulados,
                            "foraDaRaiz": feito.fora_da_raiz,
                        })),
                    ),
                    Ok(Err(recusa)) => recusar(&dela, &recusa),
                    Err(_) => recusar(&dela, &Recusa::Encerrado),
                }
            });
        }
        PedidoDoPrograma::Rodar(pedido) => {
            // `rodar` cria o filho com o Tokio: dentro do runtime.
            let dela = Arc::clone(viva);
            tauri::async_runtime::spawn(async move {
                let processo = match programa.rodar(&pedido) {
                    Ok(processo) => processo,
                    Err(recusa) => return recusar(&dela, &recusa),
                };
                let numero = processo.numero();
                tracing::info!(mod_id = %dela.chave.mod_id, servidor = %dela.chave.servidor, processo = numero, pid = ?processo.pid(), comando = %pedido.comando, "processo do servidor de jogo começou");
                let (manda, recebe) = mpsc::unbounded_channel();
                dela.processos.mapa().insert(numero, manda);
                // A resposta antes da primeira linha: a metade monta o objeto
                // do processo com ela, e o prelúdio guarda o que chegar antes.
                dela.responder(
                    n,
                    Ok(serde_json::json!({ "processo": numero, "pid": processo.pid() })),
                );
                vigiar(dela, processo, recebe).await;
            });
        }
        PedidoDoPrograma::Trazer { pedido, destino } => {
            let dela = Arc::clone(viva);
            tauri::async_runtime::spawn(async move {
                trazer(&dela, n, &programa, pedido, destino).await;
            });
        }
        PedidoDoPrograma::Empacotar { origem, arquivo } => {
            let dela = Arc::clone(viva);
            tauri::async_runtime::spawn(async move {
                let alvo = arquivo.clone();
                let feito =
                    tokio::task::spawn_blocking(move || programa.empacotar(&origem, &alvo)).await;
                match feito {
                    Ok(Ok(feito)) => dela.responder(
                        n,
                        Ok(serde_json::json!({
                            "arquivo": arquivo,
                            "tamanho": feito.tamanho,
                            "sha256": feito.sha256,
                            "arquivos": feito.arquivos,
                            "bytes": feito.bytes,
                            "linksPulados": feito.links_pulados,
                        })),
                    ),
                    Ok(Err(recusa)) => recusar(&dela, &recusa),
                    Err(_) => recusar(&dela, &Recusa::Encerrado),
                }
            });
        }
        PedidoDoPrograma::OferecerCopia { arquivo, porta } => {
            let dela = Arc::clone(viva);
            tauri::async_runtime::spawn(async move {
                match programa.oferecer_copia(&arquivo, porta).await {
                    Ok(oferta) => dela.responder(
                        n,
                        Ok(serde_json::json!({
                            "codigo": oferta.codigo,
                            "tamanho": oferta.tamanho,
                            "sha256": oferta.sha256,
                            "porta": oferta.porta,
                        })),
                    ),
                    Err(recusa) => recusar(&dela, &recusa),
                }
            });
        }
        PedidoDoPrograma::ReceberCopia {
            tunel,
            codigo,
            sha256,
            tamanho,
            destino,
        } => {
            let dela = Arc::clone(viva);
            tauri::async_runtime::spawn(async move {
                receber_copia(
                    &dela,
                    n,
                    &programa,
                    tunel,
                    seele_ffi::programa::PedidoDeCopia {
                        porta_local: 0,
                        codigo,
                        sha256,
                        tamanho,
                        destino,
                    },
                )
                .await;
            });
        }
        PedidoDoPrograma::Ler { arquivo } => match programa.ler(&arquivo) {
            Ok(texto) => viva.responder(n, Ok(serde_json::Value::String(texto))),
            Err(recusa) => recusar(viva, &recusa),
        },
        PedidoDoPrograma::EscreverNoProcesso { processo, texto } => {
            let Some(manda) = viva.processos.mapa().get(&processo).cloned() else {
                viva.recusas_da_cola.contar("processo-desconhecido");
                viva.responder(n, Err("processo-desconhecido"));
                return;
            };
            let dela = Arc::clone(viva);
            tauri::async_runtime::spawn(async move {
                let (volta, resposta) = oneshot::channel();
                if manda.send(Comando::Escrever(texto, volta)).is_err() {
                    dela.responder(n, Err("processo-desconhecido"));
                    return;
                }
                match resposta.await {
                    Ok(Ok(())) => dela.responder(n, Ok(serde_json::Value::Null)),
                    Ok(Err(recusa)) => recusar(&dela, &recusa),
                    Err(_) => dela.responder(n, Err("processo-desconhecido")),
                }
            });
        }
        PedidoDoPrograma::Matar { processo } => {
            let enviado = viva
                .processos
                .mapa()
                .get(&processo)
                .is_some_and(|manda| manda.send(Comando::Matar).is_ok());
            if enviado {
                viva.responder(n, Ok(serde_json::Value::Null));
            } else {
                viva.recusas_da_cola.contar("processo-desconhecido");
                viva.responder(n, Err("processo-desconhecido"));
            }
        }
        PedidoDoPrograma::Recusas => {
            let mut todas: BTreeMap<String, u64> = programa
                .recusas()
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect();
            for (nome, vezes) in viva.recusas_da_cola.todas() {
                *todas.entry(nome.to_owned()).or_default() += vezes;
            }
            viva.responder(n, Ok(serde_json::json!(todas)));
        }
    }
}

/// **Traz o pack da metade de servidor** (API 8 parte 6; spec §8): o que já
/// está no destino com o hash aceito volta sem rede; o resto vem pelo fluxo
/// de volume (pela estrela), e o núcleo só o grava se o hash for o do aceite.
/// Uma recusa do servidor chega à metade com o texto dele (`pack-nao-veio`),
/// é contada e dita no log.
async fn trazer(
    viva: &Arc<Viva>,
    n: u64,
    programa: &Arc<Programa>,
    pedido: String,
    destino: String,
) {
    let pronto = |trazido: &seele_ffi::programa::Trazido| {
        serde_json::json!({
            "destino": destino,
            "tamanho": trazido.tamanho,
            "hash": trazido.hash,
            "jaEstava": trazido.ja_estava,
        })
    };
    match programa.pack_ja_esta(&destino) {
        Ok(Some(trazido)) => return viva.responder(n, Ok(pronto(&trazido))),
        Ok(None) => {}
        Err(recusa) => return viva.entregar(&resposta_da_recusa(n, &recusa).to_string()),
    }
    let bytes = match viva.fio.ler_volume(&viva.chave.mod_id, pedido).await {
        Ok(bytes) => bytes,
        Err(motivo) => {
            viva.recusas_da_cola.contar("pack-nao-veio");
            tracing::warn!(mod_id = %viva.chave.mod_id, servidor = %viva.chave.servidor, motivo = %motivo, "metade de máquina: o pack não veio da metade de servidor");
            return viva.entregar(
                &serde_json::json!({
                    "n": n, "ok": false, "erro": "pack-nao-veio",
                    "frase": format!("o pack não veio da metade de servidor: {motivo}"),
                    "motivo": motivo,
                })
                .to_string(),
            );
        }
    };
    let dele = Arc::clone(programa);
    let alvo = destino.clone();
    let feito = tokio::task::spawn_blocking(move || dele.trazer(&alvo, &bytes)).await;
    match feito {
        Ok(Ok(trazido)) => viva.responder(n, Ok(pronto(&trazido))),
        Ok(Err(recusa)) => viva.entregar(&resposta_da_recusa(n, &recusa).to_string()),
        Err(_) => viva.entregar(&resposta_da_recusa(n, &Recusa::Encerrado).to_string()),
    }
}

/// Quanto a cópia espera a ligação direta antes de começar pela estrela: um
/// túnel começa na estrela, só as ligações **novas** vão pelo direto, e uma
/// cópia de GiB que começa na estrela fica nela até o fim.
pub(crate) const ESPERA_PELA_MALHA: Duration = Duration::from_secs(5);

/// **Recebe a cópia do mundo** (API 8 parte 7): entra no túnel da cópia como
/// quem joga, espera a malha até [`ESPERA_PELA_MALHA`], recebe pela porta
/// local (o núcleo confere o sha256 e retoma), e sai do túnel. A recusa de
/// entrar volta como `tunel-recusado`, com o nome do servidor.
async fn receber_copia(
    viva: &Arc<Viva>,
    n: u64,
    programa: &Arc<Programa>,
    tunel: String,
    mut pedido: seele_ffi::programa::PedidoDeCopia,
) {
    let mod_id = viva.chave.mod_id.clone();
    let porta = match viva.fio.entrar_tunel(&mod_id, &tunel).await {
        Ok(porta) => porta,
        Err(motivo) => {
            viva.recusas_da_cola.contar("tunel-recusado");
            tracing::warn!(mod_id = %mod_id, servidor = %viva.chave.servidor, tunel = %tunel, motivo, "metade de máquina: a cópia não entrou no túnel da dona");
            return viva.entregar(
                &serde_json::json!({
                    "n": n, "ok": false, "erro": "tunel-recusado",
                    "frase": format!("não deu para entrar no túnel {tunel} da máquina dona: {motivo}"),
                    "tunel": tunel, "motivo": motivo,
                })
                .to_string(),
            );
        }
    };
    let ate = std::time::Instant::now() + ESPERA_PELA_MALHA;
    while viva.fio.direto(&mod_id, &tunel) != Some(true) && std::time::Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let direto = viva.fio.direto(&mod_id, &tunel) == Some(true);
    pedido.porta_local = porta;
    let destino = pedido.destino.clone();
    let dela = Arc::clone(viva);
    let feito = programa
        .receber_copia(pedido, move |recebidos, total| {
            let _ = dela.tentar_entregar(
                &serde_json::json!({ "tipo": "programa-progresso", "n": n, "recebidos": recebidos, "total": total }).to_string(),
            );
        })
        .await;
    viva.fio.sair_tunel(&mod_id, &tunel);
    match feito {
        Ok(recebida) => viva.responder(
            n,
            Ok(serde_json::json!({
                "destino": destino,
                "tamanho": recebida.tamanho,
                "sha256": recebida.sha256,
                "retomadas": recebida.retomadas,
                "jaEstavam": recebida.ja_estavam,
                "caminho": if direto { "direto" } else { "estrela" },
            })),
        ),
        Err(recusa) => viva.entregar(&resposta_da_recusa(n, &recusa).to_string()),
    }
}

/// **Encerra tudo o que o `programa` desta metade começou, e espera.**
/// Chamado por `Viva::encerrar` (todo motivo) e por `descartar`.
pub(crate) fn encerrar_tudo(viva: &Viva, espera: Duration) {
    let Ok(programa) = &viva.programa else {
        return;
    };
    let programa = Arc::clone(programa);
    let (manda, recebe) = std::sync::mpsc::channel();
    tauri::async_runtime::spawn(async move {
        let _ = manda.send(programa.encerrar_tudo().await);
    });
    match recebe.recv_timeout(espera) {
        Ok(mortos) => {
            tracing::info!(mod_id = %viva.chave.mod_id, servidor = %viva.chave.servidor, mortos, "programa da metade de máquina encerrado");
        }
        Err(_) => {
            tracing::warn!(mod_id = %viva.chave.mod_id, servidor = %viva.chave.servidor, espera = ?espera, "programa da metade de máquina não encerrou no prazo");
        }
    }
}
