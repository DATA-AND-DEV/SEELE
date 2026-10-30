//! `programa.rodar`: o processo do servidor de jogo.
//!
//! # O que roda
//!
//! **Só o comando aceito, com os argumentos aceitos**, um a um, sem shell no
//! meio: o que o aceite mostrou é o que o sistema executa, e um `;` num
//! argumento é um `;` no argumento. O comando é um caminho dentro da raiz (o
//! Java que o MOD baixou), e nunca um nome procurado no `PATH`: o aceite diz
//! *qual* programa, e o `PATH` da pessoa decidiria outro.
//!
//! # O que volta
//!
//! A saída e o erro, linha a linha, como eventos; o código de saída no fim. A
//! entrada recebe texto (os comandos do console, como `save-all`).
//!
//! # O produto sabe e conta
//!
//! A fila de eventos tem teto ([`MAX_LINHAS_NA_FILA`]): um servidor de jogo
//! fala muito, e uma fila sem teto que ninguém lê é memória da pessoa gasta
//! pelo MOD. A linha que não cabe é **perdida e contada**, e não segura o
//! processo — um `stdout` cheio pararia o jogo. Uma linha maior que
//! [`MAX_BYTES_POR_LINHA`] é cortada e contada. Os dois números chegam no
//! [`FimDoProcesso`].
//!
//! # Nada fica órfão
//!
//! Todo filho nasce com `kill_on_drop`, e uma tarefa vigia cada um: ela o mata
//! quando pedem ([`Processo::matar`]), quando o [`Programa`] é encerrado ou
//! solto. O que **não** está coberto: um neto que o jogo crie por conta
//! própria. Matar a árvore pede grupo de processos no Unix e *job object* no
//! Windows, e os dois são FFI que este workspace proíbe (`unsafe_code =
//! "forbid"`); fica registrado como aberto.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt as _, AsyncRead, AsyncWriteExt as _, BufReader};
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, watch};

use super::{caminho, Programa, Recusa};

/// O maior pedaço de uma linha entregue; o resto até o fim da linha é
/// descartado e contado.
pub const MAX_BYTES_POR_LINHA: usize = 16 * 1024;

/// Quantos eventos esperam na fila de um processo antes de a saída começar a
/// ser perdida (e contada).
pub const MAX_LINHAS_NA_FILA: usize = 4096;

/// Quanto a vigia espera a saída terminar de chegar depois que o processo
/// saiu: um neto que herdou o `stdout` o seguraria aberto para sempre.
const PRAZO_DO_RESTO_DA_SAIDA: Duration = Duration::from_secs(2);

/// O que o MOD pede para rodar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoDeExecucao {
    /// O programa, relativo à raiz; tem de ser o aceito.
    pub comando: String,
    /// Os argumentos; têm de ser os aceitos.
    pub argumentos: Vec<String>,
    /// A pasta de trabalho, relativa à raiz; `""` é a raiz.
    pub pasta: String,
}

/// Como um processo terminou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FimDoProcesso {
    /// O código de saída; `None` quando foi morto por sinal (Unix) ou o
    /// sistema não disse.
    pub codigo: Option<i32>,
    /// Se foi morto a pedido ([`Processo::matar`], encerrar, soltar o
    /// [`Programa`]).
    pub morto_a_pedido: bool,
    /// Linhas que não couberam na fila e foram perdidas.
    pub linhas_perdidas: u64,
    /// Linhas maiores que [`MAX_BYTES_POR_LINHA`], entregues cortadas.
    pub linhas_cortadas: u64,
}

/// O que um processo avisa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoDoProcesso {
    /// Uma linha do `stdout`, sem o fim de linha.
    Saida(String),
    /// Uma linha do `stderr`, sem o fim de linha.
    Erro(String),
    /// Terminou. Último evento.
    Terminou(FimDoProcesso),
}

/// Um processo que roda.
///
/// Soltar o `Processo` **não** o mata: ele é do [`Programa`], que o mata ao
/// ser encerrado ou solto. Para matar este, [`Processo::matar`].
#[derive(Debug)]
pub struct Processo {
    numero: u64,
    pid: Option<u32>,
    entrada: Arc<tokio::sync::Mutex<Option<ChildStdin>>>,
    matar: Arc<watch::Sender<bool>>,
    eventos: mpsc::Receiver<EventoDoProcesso>,
}

impl Processo {
    /// O número deste processo no [`Programa`].
    #[must_use]
    pub const fn numero(&self) -> u64 {
        self.numero
    }

    /// O identificador do sistema, se ele deu.
    #[must_use]
    pub const fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// O próximo evento; `None` depois do [`EventoDoProcesso::Terminou`].
    pub async fn proximo(&mut self) -> Option<EventoDoProcesso> {
        self.eventos.recv().await
    }

    /// Manda uma linha para a entrada do processo. O fim de linha é posto
    /// aqui se faltar.
    ///
    /// # Errors
    ///
    /// [`Recusa::EntradaFechada`] se o processo terminou ou fechou a entrada.
    pub async fn escrever(&self, linha: &str) -> Result<(), Recusa> {
        let fechada = |motivo: String| Recusa::EntradaFechada {
            processo: self.numero,
            motivo,
        };
        let mut entrada = self.entrada.lock().await;
        let Some(stdin) = entrada.as_mut() else {
            return Err(fechada("o processo terminou".to_owned()));
        };
        let mut texto = linha.to_owned();
        if !texto.ends_with('\n') {
            texto.push('\n');
        }
        stdin
            .write_all(texto.as_bytes())
            .await
            .map_err(|e| fechada(e.to_string()))?;
        stdin.flush().await.map_err(|e| fechada(e.to_string()))
    }

    /// Manda matar. O [`EventoDoProcesso::Terminou`] diz quando morreu.
    pub fn matar(&self) {
        self.matar.send_replace(true);
    }
}

/// O que o [`Programa`] guarda de cada processo vivo.
pub(crate) struct Vigia {
    matar: Arc<watch::Sender<bool>>,
    tarefa: tokio::task::JoinHandle<()>,
}

impl Vigia {
    pub(crate) fn mandar_matar(&self) {
        self.matar.send_replace(true);
    }

    pub(crate) async fn matar_e_esperar(self) {
        self.mandar_matar();
        let _ = self.tarefa.await;
    }
}

/// O que as duas leituras e a vigia dividem.
struct Contas {
    perdidas: AtomicU64,
    cortadas: AtomicU64,
}

impl Programa {
    /// Roda o comando aceito.
    ///
    /// # Errors
    ///
    /// [`Recusa::Encerrado`], [`Recusa::ComandoForaDoAceito`],
    /// [`Recusa::ArgumentosForaDoAceito`], as do caminho (do comando e da
    /// pasta) e [`Recusa::NaoIniciou`]. Toda recusa é contada.
    ///
    /// # Panics
    ///
    /// Fora de um runtime do Tokio.
    pub fn rodar(&self, pedido: &PedidoDeExecucao) -> Result<Processo, Recusa> {
        self.rodar_sem_contar(pedido)
            .map_err(|recusa| self.interior.recusar(recusa))
    }

    fn rodar_sem_contar(&self, pedido: &PedidoDeExecucao) -> Result<Processo, Recusa> {
        let interior = &self.interior;
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let aceito = interior.politica.comando.as_ref();
        let Some(aceito) = aceito.filter(|a| a.comando == pedido.comando) else {
            return Err(Recusa::ComandoForaDoAceito {
                pedido: pedido.comando.clone(),
                aceito: aceito.map(|a| a.comando.clone()),
            });
        };
        if aceito.argumentos != pedido.argumentos {
            return Err(Recusa::ArgumentosForaDoAceito {
                pedidos: pedido.argumentos.clone(),
                aceitos: aceito.argumentos.clone(),
            });
        }
        let programa = resolver_executavel(caminho::dentro_da_raiz(
            &interior.raiz,
            &pedido.comando,
            false,
        )?);
        let pasta = caminho::dentro_da_raiz(&interior.raiz, &pedido.pasta, true)?;

        let mut comando = tokio::process::Command::new(&programa);
        comando
            .args(&pedido.argumentos)
            .current_dir(&pasta)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Sem isto, um programa de console iniciado pelo app de janela abre uma
        // janela de console própria no Windows.
        #[cfg(windows)]
        comando.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut filho = comando.spawn().map_err(|erro| Recusa::NaoIniciou {
            comando: programa.clone(),
            motivo: erro.to_string(),
        })?;

        let numero = interior.proximo_numero();
        let pid = filho.id();
        let (tx, rx) = mpsc::channel(MAX_LINHAS_NA_FILA);
        let contas = Arc::new(Contas {
            perdidas: AtomicU64::new(0),
            cortadas: AtomicU64::new(0),
        });
        let leituras = [
            filho.stdout.take().map(|saida| {
                tokio::spawn(ler_linhas(
                    saida,
                    tx.clone(),
                    EventoDoProcesso::Saida,
                    Arc::clone(&contas),
                ))
            }),
            filho.stderr.take().map(|erro| {
                tokio::spawn(ler_linhas(
                    erro,
                    tx.clone(),
                    EventoDoProcesso::Erro,
                    Arc::clone(&contas),
                ))
            }),
        ];
        let entrada = Arc::new(tokio::sync::Mutex::new(filho.stdin.take()));
        let (matar, mut morte) = watch::channel(false);
        let matar = Arc::new(matar);

        tracing::info!(
            processo = numero,
            pid,
            comando = %programa.display(),
            pasta = %pasta.display(),
            "programa iniciou um processo"
        );

        let mut processos = interior.processos.lock().map_err(|_| Recusa::NaoIniciou {
            comando: programa.clone(),
            motivo: "o registro de processos foi envenenado".to_owned(),
        })?;
        // Encerrado entre a primeira conferência e aqui: quem encerrou já
        // esvaziou o registro. O filho é solto, e `kill_on_drop` o mata.
        if interior.encerrado.load(Ordering::SeqCst) {
            return Err(Recusa::Encerrado);
        }
        let dono = Arc::clone(interior);
        let entrada_da_vigia = Arc::clone(&entrada);
        let tarefa = tokio::spawn(async move {
            let (estado, morto_a_pedido) = tokio::select! {
                estado = filho.wait() => (estado, false),
                () = esperar_a_ordem(&mut morte) => {
                    let _ = filho.start_kill();
                    (filho.wait().await, true)
                }
            };
            let codigo = estado.ok().and_then(|s| s.code());
            // O resto da saída, com prazo.
            for leitura in leituras.into_iter().flatten() {
                let abortar = leitura.abort_handle();
                if tokio::time::timeout(PRAZO_DO_RESTO_DA_SAIDA, leitura)
                    .await
                    .is_err()
                {
                    abortar.abort();
                }
            }
            entrada_da_vigia.lock().await.take();
            if let Ok(mut processos) = dono.processos.lock() {
                processos.remove(&numero);
            }
            let fim = FimDoProcesso {
                codigo,
                morto_a_pedido,
                linhas_perdidas: contas.perdidas.load(Ordering::SeqCst),
                linhas_cortadas: contas.cortadas.load(Ordering::SeqCst),
            };
            tracing::info!(
                processo = numero,
                codigo = ?fim.codigo,
                morto_a_pedido,
                linhas_perdidas = fim.linhas_perdidas,
                linhas_cortadas = fim.linhas_cortadas,
                "programa: o processo terminou"
            );
            // Há sempre lugar para este: as leituras deixam duas vagas livres.
            let _ = tx.try_send(EventoDoProcesso::Terminou(fim));
        });
        processos.insert(
            numero,
            Vigia {
                matar: Arc::clone(&matar),
                tarefa,
            },
        );
        drop(processos);

        Ok(Processo {
            numero,
            pid,
            entrada,
            matar,
            eventos: rx,
        })
    }
}

/// No Windows, `java/bin/java` é `java/bin/java.exe`: o aceite escreve o
/// mesmo comando nos dois sistemas.
fn resolver_executavel(caminho: PathBuf) -> PathBuf {
    let sufixo = std::env::consts::EXE_SUFFIX;
    if sufixo.is_empty() || caminho.exists() {
        return caminho;
    }
    let mut com_sufixo = caminho.clone().into_os_string();
    com_sufixo.push(sufixo);
    let com_sufixo = PathBuf::from(com_sufixo);
    if com_sufixo.exists() {
        com_sufixo
    } else {
        caminho
    }
}

/// Espera a ordem de matar. Sem ninguém que possa mandar, espera para sempre.
async fn esperar_a_ordem(morte: &mut watch::Receiver<bool>) {
    loop {
        if *morte.borrow_and_update() {
            return;
        }
        if morte.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// Lê linhas de `fonte` até o fim, com o teto por linha e o da fila.
async fn ler_linhas<R: AsyncRead + Unpin>(
    fonte: R,
    tx: mpsc::Sender<EventoDoProcesso>,
    evento: fn(String) -> EventoDoProcesso,
    contas: Arc<Contas>,
) {
    let mut leitor = BufReader::new(fonte);
    let mut linha: Vec<u8> = Vec::new();
    let mut descartando = false;
    let entregar = |linha: &mut Vec<u8>| {
        if linha.last() == Some(&b'\r') {
            linha.pop();
        }
        let texto = String::from_utf8_lossy(linha).into_owned();
        linha.clear();
        // Duas leituras dividem a fila, e cada uma deixa duas vagas: sobra
        // sempre uma para o `Terminou`.
        if tx.capacity() > 2 {
            if tx.try_send(evento(texto)).is_err() {
                contas.perdidas.fetch_add(1, Ordering::SeqCst);
            }
        } else {
            contas.perdidas.fetch_add(1, Ordering::SeqCst);
        }
    };
    while let Ok(buffer) = leitor.fill_buf().await {
        if buffer.is_empty() {
            if !linha.is_empty() || descartando {
                entregar(&mut linha);
            }
            break;
        }
        let (usado, fim_de_linha) = match buffer.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buffer.len(), false),
        };
        let conteudo = buffer
            .get(..usado - usize::from(fim_de_linha))
            .unwrap_or_default();
        if !descartando {
            let cabe = MAX_BYTES_POR_LINHA.saturating_sub(linha.len());
            if conteudo.len() > cabe {
                linha.extend_from_slice(conteudo.get(..cabe).unwrap_or_default());
                descartando = true;
                contas.cortadas.fetch_add(1, Ordering::SeqCst);
            } else {
                linha.extend_from_slice(conteudo);
            }
        }
        leitor.consume(usado);
        if fim_de_linha {
            entregar(&mut linha);
            descartando = false;
        }
    }
}
