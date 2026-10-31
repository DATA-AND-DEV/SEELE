//! **A metade de máquina de um MOD, no app** (API 8 parte 3).
//!
//! A metade de servidor de um MOD pede «rode o servidor de jogo X na máquina
//! de Fulano»; o pedido chega aqui como [`Event::MachineRequest`], a casca o
//! mostra numa tela **do produto**, e só depois do sim do dono a metade de
//! máquina do MOD é lida do pacote e posta de pé — num executor QuickJS sem
//! janela, com o global `maquina` e nenhum `SeeleUI`.
//!
//! # Duas metades neste arquivo
//!
//! **A decisão é pura** ([`decidir`], [`campos_que_mudaram`],
//! [`conferir_servir`], [`tratar`]): sem Tauri, sem rede, provada nos testes
//! de baixo. **O efeito** é o resto: o executor ([`Viva`]), a bomba que lê o
//! que a metade fala e o leva ao fio ([`Fio`]), o registro das ligadas e dos
//! pedidos pendentes ([`Maquinas`]) e os eventos para a casca. A separação é
//! a mesma de `assentar_fala` em `main.rs`: o que decide tem de poder ser
//! medido sem montar uma janela.
//!
//! # O que fica fora daqui
//!
//! `seele_ffi::este_processo_tem_tela(true)` é chamado uma vez, no `main` do
//! binário, antes de qualquer conexão: é por ele que toda conexão do app
//! mostra pedidos (o servidor reenvia os pendentes logo depois do `Session`,
//! antes de um comando por conexão poder chegar). Nunca aqui: os testes deste
//! arquivo rodam no mesmo binário, e o valor é do processo inteiro.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use seele_ffi::mods::AceiteDeMaquinaGuardado;
use seele_ffi::{Connection, Event, OfertaDeMaquinaFfi, TunelOferecidoFfi};
use tauri::{AppHandle, Emitter as _, Manager as _};

use crate::executor;

/// Quanto o `aoParar` da metade tem para terminar antes de o executor ser
/// encerrado de qualquer jeito.
///
/// **Um teto, e não uma espera fixa**: a metade diz quando acabou
/// (`maquina-parada-pronta`), e no caso comum a parada leva milissegundos. O
/// teto é o que impede uma metade travada de segurar o DESLIGAR — ou a saída
/// do app — para sempre.
pub(crate) const ESPERA_DO_AO_PARAR: Duration = Duration::from_secs(2);

/// O que a tela diz quando não há apelido para quem pediu: um momento da
/// metade de servidor (`pedido_por = 0`), ou alguém que já não está na lista.
pub(crate) const ALGUEM_DESTE_SERVIDOR: &str = "alguém deste servidor";

/// O canal do pedido que a casca mostra.
pub(crate) const EVENTO_DO_PEDIDO: &str = "maquina-pedido";
/// O canal de «ligou» e «desligou, e por quê».
pub(crate) const EVENTO_DO_ESTADO: &str = "maquina-estado";

/// Quantos pedidos esperam a sessão entrar no registro, no máximo.
///
/// O servidor reenvia os pedidos pendentes logo depois do `Session`, e isso
/// acontece **dentro** do `connect` — antes de a sessão ter conexão no
/// registro, de onde sai o apelido de quem pediu e a conexão que responde. Um
/// teto pequeno: um servidor tem poucos MODs com metade de máquina, e uma
/// fila sem teto alimentada pelo fio é memória que o outro lado escolhe.
const CHEGADOS_CEDO_NO_MAXIMO: usize = 64;

// ------------------------------------------------------------------ a chave

/// **Uma metade de máquina por `(alvo, mod_id, servidor)`.**
///
/// `alvo` é a chave do servidor SEELE (`seele_ffi::chave_do_servidor`), a
/// mesma sob que o aceite é arquivado: a casca recebe e devolve esta forma, e
/// um registro com o endereço como foi digitado não acharia o que a lista de
/// aceites devolve.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Chave {
    pub(crate) alvo: String,
    pub(crate) mod_id: String,
    pub(crate) servidor: String,
}

impl Chave {
    /// A chave com `alvo` canonizado.
    pub(crate) fn de(alvo: &str, mod_id: &str, servidor: &str) -> Self {
        Self {
            alvo: seele_ffi::chave_do_servidor(alvo).unwrap_or_else(|| alvo.to_owned()),
            mod_id: mod_id.to_owned(),
            servidor: servidor.to_owned(),
        }
    }
}

// ----------------------------------------------------------------- a decisão

/// Um pedido de máquina como o app o guardou ao receber o evento.
#[derive(Debug, Clone)]
pub(crate) struct PedidoRecebido {
    pub(crate) chave: Chave,
    /// O hash do pacote que o servidor tem ligado.
    pub(crate) hash: String,
    pub(crate) oferta: OfertaDeMaquinaFfi,
    /// O resumo que o núcleo calculou da oferta, em hex.
    pub(crate) resumo: String,
    /// O apelido de quem pediu, já trocado ([`apelido_de`]).
    pub(crate) pedido_por: String,
    /// De qual sessão cliente ele veio.
    pub(crate) sessao: u64,
}

/// O que fazer com um pedido.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decisao {
    /// O aceite guardado é deste mesmo resumo **e** deste mesmo pacote: o app
    /// responde sozinho (Desvio 3 do plano).
    AceitarSozinho,
    /// A tela pergunta. `mudou` são os campos da oferta que diferem do aceite
    /// guardado (mais `pacote` quando o hash difere), ou `None` quando não há
    /// aceite nenhum.
    Mostrar { mudou: Option<Vec<&'static str>> },
}

/// **Só o que foi mostrado se aceita sozinho.**
///
/// O resumo prende o que a tela mostrou — e o hash do pacote entra nele —,
/// mas o app confere os dois: um aceite guardado à mão, ou por uma versão que
/// calculava o resumo de outro jeito, não vira um sim para bytes que ninguém
/// viu.
pub(crate) fn decidir(
    pedido: &PedidoRecebido,
    guardado: Option<&AceiteDeMaquinaGuardado>,
) -> Decisao {
    let Some(guardado) = guardado else {
        return Decisao::Mostrar { mudou: None };
    };
    if guardado.resumo == pedido.resumo && guardado.hash == pedido.hash {
        return Decisao::AceitarSozinho;
    }
    let mut mudou = campos_que_mudaram(&guardado.oferta, &pedido.oferta);
    if guardado.hash != pedido.hash {
        mudou.push("pacote");
    }
    Decisao::Mostrar { mudou: Some(mudou) }
}

/// Os nomes dos campos da oferta que diferem, na ordem da oferta.
///
/// **Desmontada inteira**, sem `..`: um campo novo em `OfertaDeMaquinaFfi` é
/// um erro de compilação aqui, e não um campo que muda sem a tela dizer.
pub(crate) fn campos_que_mudaram(
    antes: &OfertaDeMaquinaFfi,
    agora: &OfertaDeMaquinaFfi,
) -> Vec<&'static str> {
    let OfertaDeMaquinaFfi {
        titulo,
        receita,
        versao_da_receita,
        versao_do_jogo,
        tuneis,
        artefatos,
        pack,
        origens,
        dominios_do_jogo,
        rodar,
        disco_maximo,
        termos,
    } = antes;
    let mut mudou = Vec::new();
    let mut se = |diferente: bool, nome: &'static str| {
        if diferente {
            mudou.push(nome);
        }
    };
    se(*titulo != agora.titulo, "titulo");
    se(*receita != agora.receita, "receita");
    se(
        *versao_da_receita != agora.versao_da_receita,
        "versao_da_receita",
    );
    se(*versao_do_jogo != agora.versao_do_jogo, "versao_do_jogo");
    se(*tuneis != agora.tuneis, "tuneis");
    se(*artefatos != agora.artefatos, "artefatos");
    se(*pack != agora.pack, "pack");
    se(*origens != agora.origens, "origens");
    se(
        *dominios_do_jogo != agora.dominios_do_jogo,
        "dominios_do_jogo",
    );
    se(*rodar != agora.rodar, "rodar");
    se(*disco_maximo != agora.disco_maximo, "disco_maximo");
    se(*termos != agora.termos, "termos");
    mudou
}

/// **A metade de máquina não sai do que foi aceito** (item 6 do foco da
/// revisão): servir um túnel que não está na oferta, ou com outra porta, ou
/// outro protocolo, é `fora-do-aceite` **aqui**, antes do fio. O servidor
/// confere de novo; esta conferência é a que não depende de o servidor estar
/// certo.
///
/// # Errors
///
/// `fora-do-aceite`.
pub(crate) fn conferir_servir(
    oferta: &OfertaDeMaquinaFfi,
    nome: &str,
    protocolo: &str,
    porta: u16,
) -> Result<(), &'static str> {
    if oferta
        .tuneis
        .iter()
        .any(|t| t.nome == nome && t.protocolo == protocolo && t.porta == porta)
    {
        Ok(())
    } else {
        Err("fora-do-aceite")
    }
}

/// O que a bomba faz com uma mensagem da metade de máquina.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Acao {
    /// `maquina.pedir(valor)`: à metade de servidor, com `payload` em JSON.
    Pedir { n: u64, payload: String },
    /// `maquina.porta.servir`, já conferido contra a oferta.
    Servir {
        n: u64,
        nome: String,
        protocolo: String,
        porta: u16,
    },
    /// `maquina.porta.parar`.
    PararTunel { n: u64, nome: String },
    /// `maquina.porta.quedas`.
    Quedas { n: u64, nome: String },
    /// Recusado aqui, sem ir ao fio; `tipo` vai ao log.
    Recusar {
        n: u64,
        erro: &'static str,
        tipo: String,
    },
    /// O `aoParar` acabou.
    ParadaPronta,
    /// Nem número de pedido tem: não há a quem responder, só o log.
    Ignorar { tipo: String },
}

/// **Decide o que uma mensagem da metade de máquina pede**, sem fazer nada.
///
/// Só quatro tipos existem (`maquina-pedir`, `porta-servir`, `porta-parar`,
/// `porta-quedas`), e os três `porta-*` só com a capacidade `maquina.porta`:
/// o prelúdio nem os monta sem ela, e um `seele.postar` escrito à mão que os
/// mandasse recebe `tipo-desconhecido`, como todo outro tipo.
pub(crate) fn tratar(oferta: &OfertaDeMaquinaFfi, capacidades: &[&str], json: &str) -> Acao {
    let valor: serde_json::Value = serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
    let tipo = valor
        .get("tipo")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    if tipo == "maquina-parada-pronta" {
        return Acao::ParadaPronta;
    }
    let Some(n) = valor.get("n").and_then(serde_json::Value::as_u64) else {
        return Acao::Ignorar { tipo };
    };
    let texto = |campo: &str| {
        valor
            .get(campo)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let com_porta = capacidades.contains(&"maquina.porta");
    let recusar = |erro, tipo: String| Acao::Recusar { n, erro, tipo };
    match tipo.as_str() {
        "maquina-pedir" => Acao::Pedir {
            n,
            payload: serde_json::to_string(valor.get("valor").unwrap_or(&serde_json::Value::Null))
                .unwrap_or_else(|_| "null".to_owned()),
        },
        "porta-servir" if com_porta => {
            let nome = texto("nome");
            let protocolo = texto("protocolo");
            // Uma porta que não cabe em `u16` não está em oferta nenhuma.
            let Some(porta) = valor
                .get("porta")
                .and_then(serde_json::Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
            else {
                return recusar("fora-do-aceite", tipo);
            };
            match conferir_servir(oferta, &nome, &protocolo, porta) {
                Ok(()) => Acao::Servir {
                    n,
                    nome,
                    protocolo,
                    porta,
                },
                Err(erro) => recusar(erro, tipo),
            }
        }
        // Parar e ler as quedas só dos túneis da oferta: são os únicos que esta
        // metade pode ter servido, e um nome de fora seria falar de um túnel de
        // outra metade do mesmo MOD na mesma conexão.
        "porta-parar" | "porta-quedas" if com_porta => {
            let nome = texto("nome");
            if !oferta.tuneis.iter().any(|t| t.nome == nome) {
                return recusar("fora-do-aceite", tipo);
            }
            if tipo == "porta-parar" {
                Acao::PararTunel { n, nome }
            } else {
                Acao::Quedas { n, nome }
            }
        }
        _ => recusar("tipo-desconhecido", tipo),
    }
}

// ------------------------------------------------------------------- o fio

/// Um futuro que atravessa threads.
pub(crate) type Futuro<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

/// **O que a metade de máquina alcança do lado de fora**: a conexão, e só os
/// verbos da máquina. Um traço, e não a `Connection` direto, para que a
/// bomba seja provada com um fio de mentira — e o teste veja que o túnel fora
/// da oferta nem chega a ele.
pub(crate) trait Fio: Send + Sync {
    fn pedir(
        &self,
        mod_id: &str,
        servidor: &str,
        payload: String,
    ) -> Futuro<Result<String, &'static str>>;
    fn servir(
        &self,
        mod_id: &str,
        nome: &str,
        protocolo: &str,
        porta: u16,
    ) -> Futuro<Result<(), &'static str>>;
    fn parar_tunel(&self, mod_id: &str, nome: &str) -> Result<(), &'static str>;
    fn quedas(&self, mod_id: &str, nome: &str) -> Futuro<Result<Vec<(String, u32)>, &'static str>>;
    /// `MaquinaParada`, com o motivo em kebab-case.
    fn parar_maquina(&self, mod_id: &str, servidor: &str, motivo: &str)
        -> Result<(), &'static str>;
}

/// O fio de verdade: a conexão da sessão em que o pedido chegou.
pub(crate) struct FioDaConexao(pub(crate) Arc<Connection>);

impl Fio for FioDaConexao {
    fn pedir(
        &self,
        mod_id: &str,
        servidor: &str,
        payload: String,
    ) -> Futuro<Result<String, &'static str>> {
        let conexao = Arc::clone(&self.0);
        let (mod_id, servidor) = (mod_id.to_owned(), servidor.to_owned());
        Box::pin(async move { conexao.pedido_da_maquina(mod_id, servidor, payload).await })
    }

    fn servir(
        &self,
        mod_id: &str,
        nome: &str,
        protocolo: &str,
        porta: u16,
    ) -> Futuro<Result<(), &'static str>> {
        let conexao = Arc::clone(&self.0);
        let (mod_id, nome, protocolo) = (mod_id.to_owned(), nome.to_owned(), protocolo.to_owned());
        Box::pin(async move { conexao.servir_tunel(mod_id, nome, &protocolo, porta).await })
    }

    fn parar_tunel(&self, mod_id: &str, nome: &str) -> Result<(), &'static str> {
        self.0
            .parar_tunel(mod_id.to_owned(), nome.to_owned())
            .map_err(|_| "sessao-encerrada")
    }

    fn quedas(&self, mod_id: &str, nome: &str) -> Futuro<Result<Vec<(String, u32)>, &'static str>> {
        let conexao = Arc::clone(&self.0);
        let (mod_id, nome) = (mod_id.to_owned(), nome.to_owned());
        Box::pin(async move {
            conexao
                .quedas_do_tunel_servido(mod_id, nome)
                .await
                .map(|quedas| {
                    quedas
                        .into_iter()
                        .map(|q| (q.motivo.to_owned(), q.vezes))
                        .collect()
                })
        })
    }

    fn parar_maquina(
        &self,
        mod_id: &str,
        servidor: &str,
        motivo: &str,
    ) -> Result<(), &'static str> {
        self.0
            .parar_maquina(mod_id.to_owned(), servidor.to_owned(), motivo)
    }
}

// --------------------------------------------------------- o fim, e por quê

/// Por que uma metade de máquina acabou — o motivo que `aoParar` recebe e que
/// `maquina-estado` diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fim {
    /// DESLIGAR.
    PeloDono,
    /// REVOGAR.
    Revogada,
    /// O app está fechando.
    AppFechou,
    /// O servidor dispensou: o MOD foi desligado lá.
    ModDesligado,
    /// O servidor dispensou: a metade de servidor pediu outra coisa.
    OutroPedido,
    /// O servidor dispensou: esta conexão não é a máquina aceita.
    SemAceite,
    /// A sessão em que ela foi aceita acabou.
    ServidorSaiu,
    /// A metade lançou sem pegar, ou o executor dela morreu.
    MetadeFalhou,
}

impl Fim {
    /// O nome em kebab-case.
    pub(crate) fn nome(self) -> &'static str {
        match self {
            Self::PeloDono => "pelo-dono",
            Self::Revogada => "revogada",
            Self::AppFechou => "app-fechou",
            Self::ModDesligado => "mod-desligado",
            Self::OutroPedido => "outro-pedido",
            Self::SemAceite => "sem-aceite",
            Self::ServidorSaiu => "servidor-saiu",
            Self::MetadeFalhou => "metade-falhou",
        }
    }

    /// O motivo de uma `MachineDismissed`.
    pub(crate) fn da_dispensa(nome: &str) -> Option<Self> {
        match nome {
            "mod-desligado" => Some(Self::ModDesligado),
            "outro-pedido" => Some(Self::OutroPedido),
            "sem-aceite" => Some(Self::SemAceite),
            _ => None,
        }
    }

    /// **O `MaquinaParada` que vai ao servidor**, quando ele ainda não sabe.
    ///
    /// Numa dispensa quem decidiu foi o servidor, e com a sessão acabada não há
    /// a quem dizer: nos dois, mandar seria falar sozinho.
    fn parada_no_fio(self) -> Option<&'static str> {
        match self {
            Self::PeloDono => Some("pelo-dono"),
            Self::Revogada => Some("revogada"),
            Self::AppFechou => Some("app-fechou"),
            Self::MetadeFalhou => Some("metade-falhou"),
            Self::ModDesligado | Self::OutroPedido | Self::SemAceite | Self::ServidorSaiu => None,
        }
    }

    /// A metade falhou: o `aoParar` dela não é chamado — quem lançou sem pegar
    /// não tem como garantir que o ouvinte dela ainda faz sentido, e esperar
    /// por ele seria esperar 2 s por quem já caiu.
    fn chama_ao_parar(self) -> bool {
        self != Self::MetadeFalhou
    }
}

// ---------------------------------------------------------------- a viva

/// O que a parada espera ouvir da bomba.
#[derive(Default)]
struct Sinais {
    /// O `aoParar` acabou (`maquina-parada-pronta`).
    pronta: bool,
    /// O executor confirmou que parou.
    parou: bool,
}

/// **Uma metade de máquina de pé**: o executor, o que ela pode servir e os
/// túneis que ela serve agora.
pub(crate) struct Viva {
    pub(crate) chave: Chave,
    /// Nunca se repete neste processo: é o que impede uma resposta atrasada
    /// de uma instância velha de cair na nova da mesma chave.
    pub(crate) numero: u64,
    oferta: OfertaDeMaquinaFfi,
    capacidades: Vec<&'static str>,
    /// Num cadeado porque o executor tem o canal de saída (que não é `Sync`)
    /// e é tocado pela bomba, pelas respostas que voltam do fio e pela parada.
    executor: Mutex<executor::ExecutorQuickJs>,
    fio: Arc<dyn Fio>,
    /// Os túneis que ela serve, pelo nome: é o que a parada derruba.
    tuneis: Mutex<BTreeSet<String>>,
    /// Pediu-se o fim: uma falha depois disto é o fim que se pediu, e não
    /// outra falha.
    parando: AtomicBool,
    sinais: Mutex<Sinais>,
    acordar: Condvar,
}

/// Quem é chamado quando a metade falha sozinha.
pub(crate) type AoFalhar = Box<dyn Fn(&Arc<Viva>, String) + Send + Sync>;

impl Viva {
    /// Sobe o executor e a bomba, **sem código ainda**.
    ///
    /// Em dois passos para que quem registra a instância a registre antes de
    /// ela rodar: uma metade que lança na primeira linha chama `ao_falhar`, e
    /// ele precisa achá-la no registro para tirá-la de lá.
    ///
    /// # Errors
    ///
    /// `executor-nao-subiu`.
    pub(crate) fn subir(
        chave: Chave,
        numero: u64,
        oferta: OfertaDeMaquinaFfi,
        capacidades: Vec<&'static str>,
        fio: Arc<dyn Fio>,
        ao_falhar: AoFalhar,
    ) -> Result<Arc<Self>, &'static str> {
        // **Os mesmos tetos da metade de janela.** Nada aqui ganha mais memória
        // ou mais tempo por volta por rodar sem tela.
        let mut executor = executor::ExecutorQuickJs::novo(executor::Limites::default())
            .map_err(|_| "executor-nao-subiu")?;
        let (recebedor, fila) = executor.escutar().ok_or("executor-nao-subiu")?;
        let viva = Arc::new(Self {
            chave,
            numero,
            oferta,
            capacidades,
            executor: Mutex::new(executor),
            fio,
            tuneis: Mutex::new(BTreeSet::new()),
            parando: AtomicBool::new(false),
            sinais: Mutex::new(Sinais::default()),
            acordar: Condvar::new(),
        });
        let dela = Arc::clone(&viva);
        std::thread::Builder::new()
            .name("mod-maquina".into())
            .spawn(move || bombear(&dela, &recebedor, &fila, &ao_falhar))
            .map_err(|_| "executor-nao-subiu")?;
        Ok(viva)
    }

    /// Entrega o código, com a oferta e o servidor de jogo que o prelúdio
    /// expõe em `maquina`.
    ///
    /// # Errors
    ///
    /// `executor-nao-aceitou-o-codigo`.
    pub(crate) fn iniciar(&self, fonte: &str) -> Result<(), &'static str> {
        let dados = serde_json::json!({
            "servidor": self.chave.servidor,
            "oferta": self.oferta,
        })
        .to_string();
        self.executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iniciar_de_maquina(fonte, self.capacidades.clone(), dados)
            .map_err(|_| "executor-nao-aceitou-o-codigo")
    }

    /// Ainda não se pediu o fim dela.
    pub(crate) fn de_pe(&self) -> bool {
        !self.parando.load(Ordering::Acquire)
    }

    /// Os túneis que ela serve agora.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn tuneis_servidos(&self) -> Vec<String> {
        self.tuneis
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// Manda ao executor; uma recusa é dita, com o tamanho.
    fn entregar(&self, json: &str) {
        let resultado = self
            .executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entregar(json);
        if let Err(motivo) = resultado {
            tracing::warn!(
                mod_id = %self.chave.mod_id,
                servidor = %self.chave.servidor,
                bytes = json.len(),
                motivo,
                "metade de máquina: entrega ao executor recusada"
            );
        }
    }

    /// A resposta de um pedido da metade, no formato que o `aoResponder` do
    /// prelúdio lê.
    fn responder(&self, n: u64, resultado: Result<serde_json::Value, &str>) {
        let json = match resultado {
            Ok(valor) => serde_json::json!({ "n": n, "ok": true, "valor": valor }),
            Err(erro) => serde_json::json!({ "n": n, "ok": false, "erro": erro }),
        };
        self.entregar(&json.to_string());
    }

    /// Um `AvisoParaMaquina` daquele servidor de jogo, para o `aoAviso`.
    pub(crate) fn avisar(&self, carga: &str) {
        if !self.de_pe() {
            return;
        }
        self.entregar(&serde_json::json!({ "tipo": "aviso-maquina", "carga": carga }).to_string());
    }

    fn sinalizar(&self, marcar: impl FnOnce(&mut Sinais)) {
        marcar(&mut self.sinais.lock().unwrap_or_else(PoisonError::into_inner));
        self.acordar.notify_all();
    }

    /// **Encerra, na ordem que o dono vê**: `aoParar(motivo)` (até
    /// [`ESPERA_DO_AO_PARAR`]), os túneis que ela serve, o executor, e o
    /// `MaquinaParada` quando o servidor ainda não sabe.
    ///
    /// Bloqueia até o teto: quem chama de um comando chama numa thread de
    /// bloqueio. Devolve `false` quando o fim já tinha sido pedido — pedir de
    /// novo é o desfecho de quem apertou duas vezes, e não repete nada.
    pub(crate) fn encerrar(&self, fim: Fim) -> bool {
        if self.parando.swap(true, Ordering::AcqRel) {
            return false;
        }
        let (mod_id, servidor) = (&self.chave.mod_id, &self.chave.servidor);
        if fim.chama_ao_parar() {
            self.entregar(
                &serde_json::json!({ "tipo": "parar-maquina", "motivo": fim.nome() }).to_string(),
            );
            let relogio = Instant::now();
            let sinais = self.sinais.lock().unwrap_or_else(PoisonError::into_inner);
            let (sinais, espera) = self
                .acordar
                .wait_timeout_while(sinais, ESPERA_DO_AO_PARAR, |s| !s.pronta && !s.parou)
                .unwrap_or_else(PoisonError::into_inner);
            drop(sinais);
            if espera.timed_out() {
                // Dito: uma parada que custa sempre o teto inteiro é uma metade
                // cujo `aoParar` não termina, e quem escreveu precisa saber.
                tracing::warn!(
                    mod_id = %mod_id,
                    servidor = %servidor,
                    motivo = fim.nome(),
                    "metade de máquina: o aoParar não terminou em 2 s; encerrada assim mesmo"
                );
            } else {
                tracing::debug!(mod_id = %mod_id, servidor = %servidor, levou = ?relogio.elapsed(), "metade de máquina: aoParar terminou");
            }
        }
        let tuneis =
            std::mem::take(&mut *self.tuneis.lock().unwrap_or_else(PoisonError::into_inner));
        for nome in tuneis {
            if let Err(motivo) = self.fio.parar_tunel(mod_id, &nome) {
                tracing::info!(mod_id = %mod_id, servidor = %servidor, tunel = %nome, motivo, "metade de máquina: o túnel não foi parado no fio");
            }
        }
        self.executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pedir_encerramento();
        if let Some(parada) = fim.parada_no_fio() {
            if let Err(motivo) = self.fio.parar_maquina(mod_id, servidor, parada) {
                tracing::info!(mod_id = %mod_id, servidor = %servidor, parada, motivo, "metade de máquina: MaquinaParada não saiu");
            }
        }
        tracing::info!(
            mod_id = %mod_id,
            servidor = %servidor,
            instancia = self.numero,
            motivo = fim.nome(),
            "metade de máquina encerrada"
        );
        true
    }
}

/// **A bomba da metade de máquina**: o que ela fala vai ao fio, e não à
/// janela; o que o fio responde volta a ela.
///
/// Vive até o executor confirmar a parada. Um `Falhou`, ou um `Parou` que
/// ninguém pediu, é a metade que caiu: [`AoFalhar`] a tira do registro e diz.
fn bombear(
    viva: &Arc<Viva>,
    recebedor: &std::sync::mpsc::Receiver<executor::ParaOFora>,
    fila: &Arc<executor::Fila>,
    ao_falhar: &AoFalhar,
) {
    let (mod_id, servidor) = (viva.chave.mod_id.clone(), viva.chave.servidor.clone());
    while let Ok(fala) = recebedor.recv() {
        match fala {
            executor::ParaOFora::Mensagem(json) => {
                // O lugar na fila volta ao tirar, como na bomba da janela.
                fila.tirar(json.len());
                atender(viva, tratar(&viva.oferta, &viva.capacidades, &json));
            }
            executor::ParaOFora::Falhou(texto) => {
                fila.tirar_aviso();
                if viva.de_pe() {
                    tracing::error!(mod_id = %mod_id, servidor = %servidor, erro = ?texto, "metade de máquina falhou");
                    ao_falhar(viva, texto);
                } else {
                    tracing::debug!(mod_id = %mod_id, servidor = %servidor, erro = ?texto, "metade de máquina: falha depois do fim pedido");
                }
            }
            executor::ParaOFora::Interrompido => {
                fila.tirar_aviso();
                tracing::warn!(mod_id = %mod_id, servidor = %servidor, "metade de máquina: uma volta foi interrompida por um teto");
            }
            executor::ParaOFora::Console {
                nivel,
                texto,
                suprimidas,
            } => crate::registrar_console_do_mod(&mod_id, nivel, &texto, suprimidas),
            executor::ParaOFora::Diagnostico { .. } => {}
            executor::ParaOFora::Parou => {
                viva.sinalizar(|s| s.parou = true);
                if viva.de_pe() {
                    let texto =
                        "o executor da metade de máquina parou sem ter sido pedido".to_owned();
                    tracing::error!(mod_id = %mod_id, servidor = %servidor, erro = %texto, "metade de máquina falhou");
                    ao_falhar(viva, texto);
                }
                return;
            }
        }
    }
}

/// Faz o que [`tratar`] decidiu.
fn atender(viva: &Arc<Viva>, acao: Acao) {
    let (mod_id, servidor) = (viva.chave.mod_id.clone(), viva.chave.servidor.clone());
    match acao {
        Acao::ParadaPronta => viva.sinalizar(|s| s.pronta = true),
        Acao::Ignorar { tipo } => {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, tipo = ?tipo, motivo = "tipo-desconhecido", "metade de máquina: mensagem sem número de pedido, ignorada");
        }
        Acao::Recusar { n, erro, tipo } => {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, tipo = ?tipo, motivo = erro, "metade de máquina: pedido recusado no app");
            viva.responder(n, Err(erro));
        }
        Acao::Pedir { n, payload } => {
            let (dela, futuro) = (
                Arc::clone(viva),
                viva.fio.pedir(&mod_id, &servidor, payload),
            );
            tauri::async_runtime::spawn(async move {
                let resposta = futuro.await;
                if let Err(motivo) = resposta {
                    tracing::info!(mod_id = %dela.chave.mod_id, servidor = %dela.chave.servidor, motivo, "metade de máquina: pedido à metade de servidor sem resposta");
                }
                dela.responder(n, resposta.map(serde_json::Value::String));
            });
        }
        Acao::Servir {
            n,
            nome,
            protocolo,
            porta,
        } => {
            // **Parando, não serve mais nada**: o `aoParar` pode parar túneis, e
            // um servir que entrasse aqui sobreviveria à parada.
            if !viva.de_pe() {
                viva.responder(n, Err("sem-aceite"));
                return;
            }
            let (dela, futuro) = (
                Arc::clone(viva),
                viva.fio.servir(&mod_id, &nome, &protocolo, porta),
            );
            tauri::async_runtime::spawn(async move {
                match futuro.await {
                    Ok(()) => {
                        dela.tuneis
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .insert(nome.clone());
                        tracing::info!(mod_id = %dela.chave.mod_id, servidor = %dela.chave.servidor, tunel = %nome, porta, "metade de máquina: túnel servido");
                        dela.responder(n, Ok(serde_json::Value::Null));
                    }
                    Err(motivo) => {
                        tracing::warn!(mod_id = %dela.chave.mod_id, servidor = %dela.chave.servidor, tunel = %nome, porta, motivo, "metade de máquina: servir recusado");
                        dela.responder(n, Err(motivo));
                    }
                }
            });
        }
        Acao::PararTunel { n, nome } => {
            let resultado = viva.fio.parar_tunel(&mod_id, &nome);
            viva.tuneis
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&nome);
            viva.responder(n, resultado.map(|()| serde_json::Value::Null));
        }
        Acao::Quedas { n, nome } => {
            let (dela, futuro) = (Arc::clone(viva), viva.fio.quedas(&mod_id, &nome));
            tauri::async_runtime::spawn(async move {
                let resposta = futuro.await.map(|quedas| {
                    serde_json::Value::Array(
                        quedas
                            .into_iter()
                            .map(|(motivo, vezes)| serde_json::json!([motivo, vezes]))
                            .collect(),
                    )
                });
                dela.responder(n, resposta);
            });
        }
    }
}

// ------------------------------------------------------------- o registro

/// Uma metade de máquina de pé, com de onde ela veio.
struct Ligada {
    viva: Arc<Viva>,
    /// A sessão cliente em que ela foi aceita.
    sessao: u64,
    /// A geração do enlace daquela sessão quando ela subiu: uma reconexão
    /// muda a geração, e o pedido que o servidor reenvia depois dela não é
    /// uma cópia do de antes.
    geracao: u64,
    hash: String,
    resumo: String,
}

/// Um `MachineRequest` que chegou antes de a sessão estar no registro.
struct ChegadoCedo {
    sessao: u64,
    alvo: String,
    mod_id: String,
    hash: String,
    servidor: String,
    pedido_por: u64,
    oferta: OfertaDeMaquinaFfi,
    resumo: String,
}

#[derive(Default)]
struct Estado {
    pendentes: BTreeMap<Chave, PedidoRecebido>,
    chegados_cedo: Vec<ChegadoCedo>,
    ligadas: BTreeMap<Chave, Ligada>,
    proxima: u64,
}

/// **As metades de máquina deste app**: as ligadas e os pedidos que a tela
/// ainda não respondeu. Mora na `Session`, como os MODs nativos.
#[derive(Default)]
pub(crate) struct Maquinas {
    estado: Mutex<Estado>,
    /// **Um pedido decidido por vez.** O mesmo pedido pode chegar duas vezes
    /// seguidas, e cada chegada é atendida numa thread: sem esta vez, as duas
    /// achavam a chave sem metade e as duas a ligavam — a segunda derrubando a
    /// primeira. Segurado durante decidir e ligar (inclusive pelo aceite da
    /// tela), e nunca junto do cadeado do estado por quem espera.
    vez: Mutex<()>,
}

impl Maquinas {
    fn estado(&self) -> std::sync::MutexGuard<'_, Estado> {
        self.estado.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Tira as ligadas que satisfazem `qual`, para encerrar fora do cadeado.
    fn tirar_ligadas(&self, qual: impl Fn(&Chave, &Ligada) -> bool) -> Vec<Arc<Viva>> {
        let mut estado = self.estado();
        let chaves: Vec<Chave> = estado
            .ligadas
            .iter()
            .filter(|(chave, ligada)| qual(chave, ligada))
            .map(|(chave, _)| chave.clone())
            .collect();
        chaves
            .into_iter()
            .filter_map(|chave| estado.ligadas.remove(&chave).map(|l| l.viva))
            .collect()
    }

    /// Esta chave está ligada agora?
    fn ligada(&self, chave: &Chave) -> bool {
        self.estado().ligadas.contains_key(chave)
    }
}

/// `maquina-pedido`, como a casca o lê.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct PedidoNaTela {
    pub(crate) alvo: String,
    pub(crate) mod_id: String,
    /// O manifesto não tem nome de exibição: vai o `autor/nome`.
    pub(crate) mod_nome: String,
    pub(crate) servidor: String,
    /// O apelido, ou «alguém deste servidor».
    pub(crate) pedido_por: String,
    pub(crate) oferta: OfertaDeMaquinaFfi,
    pub(crate) resumo: String,
    /// Onde a parte 4 vai pôr o jogo (`pasta_do_servidor_de_jogo`).
    pub(crate) pasta: String,
    pub(crate) mudou: Option<Vec<&'static str>>,
}

/// `maquina-estado`, como a casca o lê.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct EstadoNaTela {
    pub(crate) alvo: String,
    pub(crate) mod_id: String,
    pub(crate) servidor: String,
    pub(crate) ligada: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) motivo: Option<&'static str>,
}

/// Uma linha de `maquinas_aceitas`.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct MaquinaAceita {
    pub(crate) alvo: String,
    pub(crate) mod_id: String,
    pub(crate) servidor: String,
    pub(crate) titulo: String,
    pub(crate) pedido_por: String,
    pub(crate) quando: u64,
    pub(crate) ligada: bool,
    pub(crate) tuneis: Vec<TunelOferecidoFfi>,
}

fn emitir_estado<R: tauri::Runtime>(
    app: &AppHandle<R>,
    chave: &Chave,
    ligada: bool,
    motivo: Option<&'static str>,
) {
    let _ = app.emit(
        EVENTO_DO_ESTADO,
        EstadoNaTela {
            alvo: chave.alvo.clone(),
            mod_id: chave.mod_id.clone(),
            servidor: chave.servidor.clone(),
            ligada,
            motivo,
        },
    );
}

/// O apelido de quem pediu, pela lista de pessoas da sessão.
///
/// `0` é um momento da metade de servidor, sem pessoa; e quem já saiu da lista
/// não tem apelido a mostrar. Nos dois, a tela diz «alguém deste servidor» —
/// e não um número, que não diz nada a quem lê.
pub(crate) fn apelido_de(conexao: &Connection, pessoa: u64) -> String {
    if pessoa == 0 {
        return ALGUEM_DESTE_SERVIDOR.to_owned();
    }
    conexao
        .snapshot()
        .presentes
        .into_iter()
        .find(|p| p.id == pessoa)
        .map(|p| p.nickname)
        .or_else(|| {
            conexao
                .membros_fora_do_ar()
                .into_iter()
                .find(|m| m.id == pessoa)
                .map(|m| m.nickname)
        })
        .filter(|apelido| !apelido.trim().is_empty())
        .unwrap_or_else(|| ALGUEM_DESTE_SERVIDOR.to_owned())
}

fn sessoes<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<tauri::State<'_, crate::Session>> {
    app.try_state::<crate::Session>()
}

/// **O que a ponte de eventos faz com os eventos da máquina.** Devolve se o
/// evento era da máquina: esses não vão crus à janela — o pedido vai como
/// `maquina-pedido`, com o apelido no lugar do número, e o aviso é da metade
/// de máquina, e de mais ninguém.
pub(crate) fn ao_evento<R: tauri::Runtime>(
    app: &AppHandle<R>,
    sessao: u64,
    alvo: &str,
    evento: &Event,
) -> bool {
    match evento {
        Event::MachineRequest {
            mod_id,
            hash,
            servidor,
            pedido_por,
            oferta,
            resumo,
        } => {
            let chegado = ChegadoCedo {
                sessao,
                alvo: alvo.to_owned(),
                mod_id: mod_id.clone(),
                hash: hash.clone(),
                servidor: servidor.clone(),
                pedido_por: *pedido_por,
                oferta: (**oferta).clone(),
                resumo: resumo.clone(),
            };
            let Some(estado) = sessoes(app) else {
                return true;
            };
            // **Com o cadeado das máquinas tomado durante a pergunta**: o
            // `connect` põe a sessão no registro e só então chama
            // [`sessao_entrou`], que toma este cadeado para esvaziar a fila. Ou
            // a sessão já está lá e o pedido segue agora, ou ele entra na fila
            // antes de ela ser esvaziada — nunca depois, onde ficaria para
            // sempre.
            let mut maquinas = estado.maquinas.estado();
            if estado.sessoes.buscar(sessao).is_some() {
                drop(maquinas);
                // Fora da thread do enlace: decidir lê o disco, e aceitar
                // sozinho pode esperar a metade anterior parar.
                let app = app.clone();
                std::thread::spawn(move || processar_pedido(&app, chegado));
            } else if maquinas.chegados_cedo.len() >= CHEGADOS_CEDO_NO_MAXIMO {
                tracing::warn!(mod_id = %mod_id, servidor = %servidor, "pedido de máquina descartado: a fila dos que chegaram antes da sessão está cheia");
            } else {
                maquinas.chegados_cedo.push(chegado);
            }
            true
        }
        Event::MachineNotice {
            mod_id,
            servidor,
            carga,
        } => {
            let chave = Chave::de(alvo, mod_id, servidor);
            let viva = sessoes(app).and_then(|estado| {
                estado
                    .maquinas
                    .estado()
                    .ligadas
                    .get(&chave)
                    .filter(|l| l.sessao == sessao)
                    .map(|l| Arc::clone(&l.viva))
            });
            match viva {
                Some(viva) => viva.avisar(carga),
                None => {
                    tracing::debug!(mod_id = %mod_id, servidor = %servidor, "aviso para máquina sem metade ligada nesta sessão: descartado");
                }
            }
            true
        }
        Event::MachineDismissed {
            mod_id,
            servidor,
            motivo,
        } => {
            // **Só a instância que ela nomeia**: a dispensa pode chegar a toda
            // sessão da pessoa, e só a desta sessão é a que o servidor dispensou.
            let chave = Chave::de(alvo, mod_id, servidor);
            let Some(fim) = Fim::da_dispensa(motivo) else {
                tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo, "dispensa de máquina com motivo desconhecido: ignorada");
                return true;
            };
            tracing::info!(mod_id = %mod_id, servidor = %servidor, motivo, "o servidor dispensou a máquina");
            let Some(estado) = sessoes(app) else {
                return true;
            };
            {
                let mut maquinas = estado.maquinas.estado();
                if maquinas
                    .pendentes
                    .get(&chave)
                    .is_some_and(|p| p.sessao == sessao)
                {
                    maquinas.pendentes.remove(&chave);
                }
            }
            let vivas = estado
                .maquinas
                .tirar_ligadas(|c, l| *c == chave && l.sessao == sessao);
            encerrar_e_dizer(app, vivas, fim);
            true
        }
        Event::Ended { .. } => {
            sessao_saiu(app, sessao);
            false
        }
        _ => false,
    }
}

/// Encerra cada uma numa thread própria e diz `maquina-estado` ao fim.
fn encerrar_e_dizer<R: tauri::Runtime>(app: &AppHandle<R>, vivas: Vec<Arc<Viva>>, fim: Fim) {
    for viva in vivas {
        let app = app.clone();
        std::thread::spawn(move || {
            if viva.encerrar(fim) {
                emitir_estado(&app, &viva.chave, false, Some(fim.nome()));
            }
        });
    }
}

/// A sessão entrou no registro: os pedidos que chegaram antes dela agora têm
/// conexão e lista de pessoas.
pub(crate) fn sessao_entrou<R: tauri::Runtime>(app: &AppHandle<R>, sessao: u64) {
    let Some(estado) = sessoes(app) else {
        return;
    };
    let chegados: Vec<ChegadoCedo> = {
        let mut maquinas = estado.maquinas.estado();
        let (deste, outros) = std::mem::take(&mut maquinas.chegados_cedo)
            .into_iter()
            .partition(|c| c.sessao == sessao);
        maquinas.chegados_cedo = outros;
        deste
    };
    for chegado in chegados {
        let app = app.clone();
        std::thread::spawn(move || processar_pedido(&app, chegado));
    }
}

/// **A sessão acabou**: os pedidos dela somem e as metades dela param com
/// `servidor-saiu`. O aceite guardado fica: voltar ao servidor é o caminho de
/// sempre (o pedido chega de novo, e o resumo igual liga sozinho).
pub(crate) fn sessao_saiu<R: tauri::Runtime>(app: &AppHandle<R>, sessao: u64) {
    let Some(estado) = sessoes(app) else {
        return;
    };
    {
        let mut maquinas = estado.maquinas.estado();
        maquinas.pendentes.retain(|_, p| p.sessao != sessao);
        maquinas.chegados_cedo.retain(|c| c.sessao != sessao);
    }
    let vivas = estado.maquinas.tirar_ligadas(|_, l| l.sessao == sessao);
    encerrar_e_dizer(app, vivas, Fim::ServidorSaiu);
}

/// O mesmo, para quem não tem `AppHandle` (a entrada solta): sem evento,
/// porque essa sessão nunca chegou à tela.
pub(crate) fn sessao_saiu_sem_tela(maquinas: &Maquinas, sessao: u64) {
    {
        let mut estado = maquinas.estado();
        estado.pendentes.retain(|_, p| p.sessao != sessao);
        estado.chegados_cedo.retain(|c| c.sessao != sessao);
    }
    for viva in maquinas.tirar_ligadas(|_, l| l.sessao == sessao) {
        std::thread::spawn(move || viva.encerrar(Fim::ServidorSaiu));
    }
}

/// **O app está fechando**: toda metade recebe `aoParar("app-fechou")` ao
/// mesmo tempo, e quem chama espera todas — no máximo o teto de uma, e não a
/// soma. O `MaquinaParada { AppFechou }` sai antes de a conexão se despedir.
pub(crate) fn parar_todas_na_saida(maquinas: &Maquinas) {
    let vivas = maquinas.tirar_ligadas(|_, _| true);
    if vivas.is_empty() {
        return;
    }
    tracing::info!(
        quantas = vivas.len(),
        "metades de máquina mandadas parar na saída do app"
    );
    let threads: Vec<_> = vivas
        .into_iter()
        .map(|viva| std::thread::spawn(move || viva.encerrar(Fim::AppFechou)))
        .collect();
    for thread in threads {
        let _ = thread.join();
    }
}

/// O pedido, decidido: aceito sozinho, ou mostrado.
fn processar_pedido<R: tauri::Runtime>(app: &AppHandle<R>, chegado: ChegadoCedo) {
    let Some(estado) = sessoes(app) else {
        return;
    };
    let _vez = estado
        .maquinas
        .vez
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let Some(cliente) = estado.sessoes.buscar(chegado.sessao) else {
        tracing::info!(mod_id = %chegado.mod_id, servidor = %chegado.servidor, "pedido de máquina de uma sessão que já saiu: descartado");
        return;
    };
    let conexao = Arc::clone(&cliente.conexao);
    let chave = Chave::de(&chegado.alvo, &chegado.mod_id, &chegado.servidor);
    let pedido = PedidoRecebido {
        chave: chave.clone(),
        hash: chegado.hash,
        oferta: chegado.oferta,
        resumo: chegado.resumo,
        pedido_por: apelido_de(&conexao, chegado.pedido_por),
        sessao: chegado.sessao,
    };

    // **O mesmo pedido duas vezes** (o servidor pode mandá-lo de novo antes de
    // ver a primeira resposta): com a metade dele de pé, nesta sessão e nesta
    // geração do enlace, a resposta é repetida e nada mais — derrubar e subir a
    // metade a cada cópia derrubaria os túneis dela à toa.
    let geracao = conexao.carimbo().geracao;
    let copia = estado
        .maquinas
        .estado()
        .ligadas
        .get(&chave)
        .is_some_and(|l| {
            l.sessao == pedido.sessao
                && l.geracao == geracao
                && l.resumo == pedido.resumo
                && l.hash == pedido.hash
        });
    if copia {
        tracing::debug!(mod_id = %chave.mod_id, servidor = %chave.servidor, "pedido de máquina repetido com a metade de pé: aceite repetido");
        if let Err(motivo) = conexao.aceitar_maquina(
            chave.mod_id.clone(),
            chave.servidor.clone(),
            pedido.resumo.clone(),
        ) {
            tracing::info!(mod_id = %chave.mod_id, servidor = %chave.servidor, motivo, "aceite repetido não saiu");
        }
        return;
    }

    let home = crate::config_dir(app);
    let guardado = match seele_ffi::mods::aceite_de_maquina(
        &home,
        &chave.alvo,
        &chave.mod_id,
        &chave.servidor,
    ) {
        Ok(guardado) => guardado,
        Err(motivo) => {
            // Ler o aceite falhou: a tela pergunta de novo, e o log diz por quê.
            tracing::warn!(mod_id = %chave.mod_id, servidor = %chave.servidor, motivo = %motivo, "aceites de máquina não foram lidos; o pedido vai à tela");
            None
        }
    };
    match decidir(&pedido, guardado.as_ref()) {
        Decisao::AceitarSozinho => {
            tracing::info!(mod_id = %chave.mod_id, servidor = %chave.servidor, "pedido de máquina com o resumo aceito: ligada sem tela");
            let _ = ligar(app, &pedido, &conexao, None);
        }
        Decisao::Mostrar { mudou } => {
            tracing::info!(mod_id = %chave.mod_id, servidor = %chave.servidor, pedido_por = %pedido.pedido_por, mudou = ?mudou, "pedido de máquina à tela");
            let pasta = seele_ffi::mods::pasta_do_servidor_de_jogo(
                &home,
                &chave.alvo,
                &chave.mod_id,
                &chave.servidor,
            )
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
            let na_tela = PedidoNaTela {
                alvo: chave.alvo.clone(),
                mod_id: chave.mod_id.clone(),
                mod_nome: chave.mod_id.clone(),
                servidor: chave.servidor.clone(),
                pedido_por: pedido.pedido_por.clone(),
                oferta: pedido.oferta.clone(),
                resumo: pedido.resumo.clone(),
                pasta,
                mudou,
            };
            estado.maquinas.estado().pendentes.insert(chave, pedido);
            let _ = app.emit(EVENTO_DO_PEDIDO, na_tela);
        }
    }
}

/// **Liga a metade de máquina de um pedido aceito** — pela tela (com
/// `guardar`) ou sozinho (sem).
///
/// A ordem é a que não deixa um sim sem código nem código sem sim: o fonte é
/// lido do pacote **agora** (e só agora: antes do aceite ele nem é aberto); se
/// não dá, a máquina recusa com `pacote-diferente` e nada é guardado. Depois o
/// aceite é guardado, a metade anterior desta chave (se houver) para, o
/// `AceiteDeMaquina` sai, e a nova sobe.
fn ligar<R: tauri::Runtime>(
    app: &AppHandle<R>,
    pedido: &PedidoRecebido,
    conexao: &Arc<Connection>,
    guardar: Option<&AceiteDeMaquinaGuardado>,
) -> Result<(), String> {
    let chave = &pedido.chave;
    let (mod_id, servidor) = (&chave.mod_id, &chave.servidor);
    let home = crate::config_dir(app);
    let Some(estado) = sessoes(app) else {
        return Err("sem-sessao".to_owned());
    };

    let metade = seele_ffi::mods::codigo_da_maquina(&home, &pedido.hash).and_then(|m| {
        if m.id == *mod_id {
            Ok(m)
        } else {
            Err("conteudo-de-outro-mod".to_owned())
        }
    });
    let metade = match metade {
        Ok(metade) => metade,
        Err(motivo) => {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, hash = %pedido.hash, motivo = %motivo, "metade de máquina não lida do pacote: recusada com pacote-diferente");
            if let Err(erro) =
                conexao.recusar_maquina(mod_id.clone(), servidor.clone(), "pacote-diferente")
            {
                tracing::info!(mod_id = %mod_id, servidor = %servidor, motivo = erro, "a recusa não saiu");
            }
            estado.maquinas.estado().pendentes.remove(chave);
            emitir_estado(app, chave, false, Some("pacote-diferente"));
            return Err(motivo);
        }
    };

    if let Some(aceite) = guardar {
        seele_ffi::mods::guardar_aceite_de_maquina(&home, &chave.alvo, aceite).map_err(|motivo| {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo = %motivo, "o aceite de máquina não foi guardado; nada foi aceito");
            motivo
        })?;
    }
    estado.maquinas.estado().pendentes.remove(chave);

    // A anterior desta chave sai antes de a nova subir: as duas servindo o
    // mesmo túnel seria o servidor recusar a nova por um nome que a velha
    // ainda segura.
    // O motivo que o `aoParar` dela lê: a mesma sessão e o mesmo enlace é a
    // metade de servidor pedindo outra coisa; outra sessão, ou o enlace que
    // voltou de uma queda, é a sessão dela que acabou.
    let geracao = conexao.carimbo().geracao;
    let fim_da_anterior =
        estado
            .maquinas
            .estado()
            .ligadas
            .get(chave)
            .map_or(Fim::OutroPedido, |l| {
                if l.sessao == pedido.sessao && l.geracao == geracao {
                    Fim::OutroPedido
                } else {
                    Fim::ServidorSaiu
                }
            });
    let anteriores = estado.maquinas.tirar_ligadas(|c, _| c == chave);
    for anterior in anteriores {
        anterior.encerrar(fim_da_anterior);
    }

    conexao
        .aceitar_maquina(mod_id.clone(), servidor.clone(), pedido.resumo.clone())
        .map_err(|motivo| {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo, "o AceiteDeMaquina não saiu");
            motivo.to_owned()
        })?;

    let numero = {
        let mut maquinas = estado.maquinas.estado();
        maquinas.proxima = maquinas.proxima.saturating_add(1);
        maquinas.proxima
    };
    let app_da_falha = app.clone();
    let ao_falhar: AoFalhar = Box::new(move |viva, texto| {
        // A metade caiu sozinha: sai do registro (se ainda for ela), para com
        // `metade-falhou` e a faixa diz; o texto do erro já está no log.
        if let Some(estado) = sessoes(&app_da_falha) {
            let numero = viva.numero;
            estado
                .maquinas
                .tirar_ligadas(|c, l| *c == viva.chave && l.viva.numero == numero);
        }
        tracing::warn!(mod_id = %viva.chave.mod_id, servidor = %viva.chave.servidor, erro = ?texto, "metade de máquina encerrada por falha");
        let viva = Arc::clone(viva);
        let app = app_da_falha.clone();
        std::thread::spawn(move || {
            if viva.encerrar(Fim::MetadeFalhou) {
                emitir_estado(&app, &viva.chave, false, Some(Fim::MetadeFalhou.nome()));
            }
        });
    });
    let falhou = |motivo: &'static str| {
        tracing::error!(mod_id = %mod_id, servidor = %servidor, motivo, "metade de máquina não subiu");
        if let Err(erro) = conexao.parar_maquina(mod_id.clone(), servidor.clone(), "metade-falhou")
        {
            tracing::info!(mod_id = %mod_id, servidor = %servidor, motivo = erro, "MaquinaParada não saiu");
        }
        emitir_estado(app, chave, false, Some(Fim::MetadeFalhou.nome()));
        motivo.to_owned()
    };
    let viva = Viva::subir(
        chave.clone(),
        numero,
        pedido.oferta.clone(),
        metade.capacidades,
        Arc::new(FioDaConexao(Arc::clone(conexao))),
        ao_falhar,
    )
    .map_err(&falhou)?;
    estado.maquinas.estado().ligadas.insert(
        chave.clone(),
        Ligada {
            viva: Arc::clone(&viva),
            sessao: pedido.sessao,
            geracao,
            hash: pedido.hash.clone(),
            resumo: pedido.resumo.clone(),
        },
    );
    if let Err(motivo) = viva.iniciar(&metade.fonte) {
        estado
            .maquinas
            .tirar_ligadas(|c, l| c == chave && l.viva.numero == numero);
        // `encerrar` já manda o `MaquinaParada { MetadeFalhou }`: o `falhou`
        // de cima o mandaria de novo.
        tracing::error!(mod_id = %mod_id, servidor = %servidor, motivo, "metade de máquina não subiu");
        viva.encerrar(Fim::MetadeFalhou);
        emitir_estado(app, chave, false, Some(Fim::MetadeFalhou.nome()));
        return Err(motivo.to_owned());
    }
    tracing::info!(mod_id = %mod_id, servidor = %servidor, instancia = numero, "metade de máquina ligada");
    emitir_estado(app, chave, true, None);
    Ok(())
}

// ------------------------------------------------------------- os comandos

/// `maquina_aceitar`: o dono disse sim ao pedido pendente de `resumo`.
///
/// # Errors
///
/// `sem-pedido`, `pedido-velho` (o resumo não é o do pedido pendente),
/// `sem-conexao`, ou o motivo de [`ligar`].
pub(crate) fn aceitar<R: tauri::Runtime>(
    app: &AppHandle<R>,
    alvo: &str,
    mod_id: &str,
    servidor: &str,
    resumo: &str,
) -> Result<AceiteDeMaquinaGuardado, String> {
    let chave = Chave::de(alvo, mod_id, servidor);
    let estado = sessoes(app).ok_or("sem-sessao")?;
    let _vez = estado
        .maquinas
        .vez
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let pedido = estado
        .maquinas
        .estado()
        .pendentes
        .get(&chave)
        .cloned()
        .ok_or("sem-pedido")?;
    if pedido.resumo != resumo {
        // A tela mostrou uma oferta e o servidor já pediu outra: o sim é do
        // que foi mostrado, e o que foi mostrado não está mais em pedido.
        tracing::info!(
            mod_id,
            servidor,
            "aceite de máquina com resumo velho: recusado"
        );
        return Err("pedido-velho".to_owned());
    }
    let cliente = estado.sessoes.buscar(pedido.sessao).ok_or("sem-conexao")?;
    let conexao = Arc::clone(&cliente.conexao);
    let aceite = AceiteDeMaquinaGuardado {
        alvo: chave.alvo.clone(),
        mod_id: chave.mod_id.clone(),
        hash: pedido.hash.clone(),
        servidor: chave.servidor.clone(),
        resumo: pedido.resumo.clone(),
        oferta: pedido.oferta.clone(),
        pedido_por: pedido.pedido_por.clone(),
        quando: 0,
    };
    ligar(app, &pedido, &conexao, Some(&aceite))?;
    let home = crate::config_dir(app);
    Ok(
        seele_ffi::mods::aceite_de_maquina(&home, &chave.alvo, &chave.mod_id, &chave.servidor)
            .ok()
            .flatten()
            .unwrap_or(aceite),
    )
}

/// `maquina_recusar`: o dono disse não. Nada é guardado.
///
/// # Errors
///
/// `sem-pedido`, `sem-conexao`, ou o motivo de `recusar_maquina`.
pub(crate) fn recusar<R: tauri::Runtime>(
    app: &AppHandle<R>,
    alvo: &str,
    mod_id: &str,
    servidor: &str,
) -> Result<(), String> {
    let chave = Chave::de(alvo, mod_id, servidor);
    let estado = sessoes(app).ok_or("sem-sessao")?;
    let pedido = estado
        .maquinas
        .estado()
        .pendentes
        .remove(&chave)
        .ok_or("sem-pedido")?;
    let cliente = estado.sessoes.buscar(pedido.sessao).ok_or("sem-conexao")?;
    tracing::info!(mod_id, servidor, "pedido de máquina recusado pelo dono");
    cliente
        .conexao
        .recusar_maquina(chave.mod_id, chave.servidor, "pelo-dono")
        .map_err(str::to_owned)
}

/// `maquina_desligar` (`Fim::PeloDono`) e `maquina_revogar`
/// (`Fim::Revogada`, e o aceite guardado é apagado).
///
/// # Errors
///
/// `nao-esta-ligada` (desligar), `sem-aceite` (revogar sem nada ligado nem
/// guardado), ou o motivo de `revogar_aceite_de_maquina`.
pub(crate) fn desligar<R: tauri::Runtime>(
    app: &AppHandle<R>,
    alvo: &str,
    mod_id: &str,
    servidor: &str,
    fim: Fim,
) -> Result<(), String> {
    let chave = Chave::de(alvo, mod_id, servidor);
    let estado = sessoes(app).ok_or("sem-sessao")?;
    let vivas = estado.maquinas.tirar_ligadas(|c, _| *c == chave);
    let havia = !vivas.is_empty();
    for viva in vivas {
        if viva.encerrar(fim) {
            emitir_estado(app, &chave, false, Some(fim.nome()));
        }
    }
    if fim != Fim::Revogada {
        return if havia {
            Ok(())
        } else {
            Err("nao-esta-ligada".to_owned())
        };
    }
    let apagado = seele_ffi::mods::revogar_aceite_de_maquina(
        &crate::config_dir(app),
        &chave.alvo,
        &chave.mod_id,
        &chave.servidor,
    )?;
    tracing::info!(
        mod_id,
        servidor,
        apagado,
        "aceite de máquina revogado pelo dono"
    );
    if havia || apagado {
        Ok(())
    } else {
        Err("sem-aceite".to_owned())
    }
}

/// `maquinas_aceitas`: os aceites guardados, com quais estão de pé agora.
///
/// # Errors
///
/// O motivo de `aceites_de_maquina`.
pub(crate) fn aceitas<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<Vec<MaquinaAceita>, String> {
    let lidos = seele_ffi::mods::aceites_de_maquina(&crate::config_dir(app))?;
    if lidos.ilegiveis > 0 {
        tracing::warn!(
            ilegiveis = lidos.ilegiveis,
            "aceites de máquina com linhas ilegíveis: puladas"
        );
    }
    let estado = sessoes(app);
    Ok(lidos
        .aceites
        .into_iter()
        .map(|a| {
            let chave = Chave::de(&a.alvo, &a.mod_id, &a.servidor);
            MaquinaAceita {
                ligada: estado.as_ref().is_some_and(|e| e.maquinas.ligada(&chave)),
                alvo: a.alvo,
                mod_id: a.mod_id,
                servidor: a.servidor,
                titulo: a.oferta.titulo,
                pedido_por: a.pedido_por,
                quando: a.quando,
                tuneis: a.oferta.tuneis,
            }
        })
        .collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn tunel(nome: &str, protocolo: &str, porta: u16) -> TunelOferecidoFfi {
        TunelOferecidoFfi {
            nome: nome.to_owned(),
            protocolo: protocolo.to_owned(),
            porta,
        }
    }

    fn oferta() -> OfertaDeMaquinaFfi {
        OfertaDeMaquinaFfi {
            titulo: "Minecraft".to_owned(),
            receita: "minecraft-java".to_owned(),
            versao_da_receita: "1.0.0".to_owned(),
            versao_do_jogo: "1.21.4".to_owned(),
            tuneis: vec![tunel("mc", "tcp", 25565)],
            artefatos: Vec::new(),
            pack: None,
            origens: Vec::new(),
            dominios_do_jogo: Vec::new(),
            rodar: None,
            disco_maximo: 2_000_000_000,
            termos: Vec::new(),
        }
    }

    fn pedido(oferta: OfertaDeMaquinaFfi, resumo: &str, hash: &str) -> PedidoRecebido {
        PedidoRecebido {
            chave: Chave::de("casa", "x/jogo", "mundo"),
            hash: hash.to_owned(),
            oferta,
            resumo: resumo.to_owned(),
            pedido_por: "ana".to_owned(),
            sessao: 1,
        }
    }

    fn guardado(oferta: OfertaDeMaquinaFfi, resumo: &str, hash: &str) -> AceiteDeMaquinaGuardado {
        AceiteDeMaquinaGuardado {
            alvo: "casa:8383".to_owned(),
            mod_id: "x/jogo".to_owned(),
            hash: hash.to_owned(),
            servidor: "mundo".to_owned(),
            resumo: resumo.to_owned(),
            oferta,
            pedido_por: "ana".to_owned(),
            quando: 1,
        }
    }

    #[test]
    fn um_resumo_diferente_nao_se_aceita_sozinho() {
        let mut nova = oferta();
        nova.versao_do_jogo = "1.21.5".to_owned();
        let decisao = decidir(
            &pedido(nova, "bbbb", "h1"),
            Some(&guardado(oferta(), "aaaa", "h1")),
        );
        assert_eq!(
            decisao,
            Decisao::Mostrar {
                mudou: Some(vec!["versao_do_jogo"])
            },
            "um pedido com outro resumo foi aceito sem tela, ou a tela não diz o que mudou"
        );
        // E o igual se aceita, que é a outra metade da prova.
        assert_eq!(
            decidir(
                &pedido(oferta(), "aaaa", "h1"),
                Some(&guardado(oferta(), "aaaa", "h1"))
            ),
            Decisao::AceitarSozinho
        );
        // Sem aceite guardado, a tela pergunta sem «mudou».
        assert_eq!(
            decidir(&pedido(oferta(), "aaaa", "h1"), None),
            Decisao::Mostrar { mudou: None }
        );
    }

    #[test]
    fn o_mesmo_resumo_com_outro_pacote_tambem_pede_tela() {
        let decisao = decidir(
            &pedido(oferta(), "aaaa", "h2"),
            Some(&guardado(oferta(), "aaaa", "h1")),
        );
        assert_eq!(
            decisao,
            Decisao::Mostrar {
                mudou: Some(vec!["pacote"])
            },
            "um aceite de outro pacote virou sim para bytes que ninguém viu"
        );
    }

    #[test]
    fn servir_fora_da_oferta_e_recusado_antes_do_fio() {
        let oferta = oferta();
        assert_eq!(
            conferir_servir(&oferta, "mc", "tcp", 25566),
            Err("fora-do-aceite")
        );
        assert_eq!(
            conferir_servir(&oferta, "mc", "udp", 25565),
            Err("fora-do-aceite")
        );
        assert_eq!(
            conferir_servir(&oferta, "outro", "tcp", 25565),
            Err("fora-do-aceite")
        );
        assert_eq!(conferir_servir(&oferta, "mc", "tcp", 25565), Ok(()));
    }

    #[test]
    fn cada_campo_da_oferta_tem_nome_quando_muda() {
        let base = oferta();
        let mut tudo = base.clone();
        tudo.titulo.push('!');
        tudo.receita.push('!');
        tudo.versao_da_receita.push('!');
        tudo.versao_do_jogo.push('!');
        tudo.tuneis.push(tunel("voz", "udp", 9987));
        tudo.artefatos.push(seele_ffi::types::ArtefatoOferecidoFfi {
            nome: "server.jar".to_owned(),
            url: "https://x/server.jar".to_owned(),
            tamanho: 1,
            hash: seele_ffi::types::HashDeArtefatoFfi {
                algoritmo: "sha1".to_owned(),
                hex: "a".repeat(40),
            },
        });
        tudo.pack = Some(seele_ffi::types::HashDeArtefatoFfi {
            algoritmo: "sha1".to_owned(),
            hex: "b".repeat(40),
        });
        tudo.origens.push("x".to_owned());
        tudo.dominios_do_jogo.push("y".to_owned());
        tudo.rodar = Some(seele_ffi::types::ComandoOferecidoFfi {
            comando: "java".to_owned(),
            argumentos: Vec::new(),
        });
        tudo.disco_maximo += 1;
        tudo.termos.push(seele_ffi::types::TermoOferecidoFfi {
            titulo: "EULA".to_owned(),
            url: "https://x/eula".to_owned(),
        });
        assert_eq!(
            campos_que_mudaram(&base, &tudo),
            vec![
                "titulo",
                "receita",
                "versao_da_receita",
                "versao_do_jogo",
                "tuneis",
                "artefatos",
                "pack",
                "origens",
                "dominios_do_jogo",
                "rodar",
                "disco_maximo",
                "termos",
            ]
        );
        assert!(campos_que_mudaram(&base, &base).is_empty());
    }

    #[test]
    fn so_quatro_tipos_existem_e_porta_so_com_o_poder() {
        let oferta = oferta();
        let com = ["maquina", "maquina.porta"];
        assert_eq!(
            tratar(
                &oferta,
                &com,
                r#"{"tipo":"maquina-pedir","n":1,"valor":{"a":1}}"#
            ),
            Acao::Pedir {
                n: 1,
                payload: r#"{"a":1}"#.to_owned()
            }
        );
        assert_eq!(
            tratar(
                &oferta,
                &com,
                r#"{"tipo":"porta-servir","n":2,"nome":"mc","protocolo":"tcp","porta":25565}"#
            ),
            Acao::Servir {
                n: 2,
                nome: "mc".to_owned(),
                protocolo: "tcp".to_owned(),
                porta: 25565
            }
        );
        assert_eq!(
            tratar(
                &oferta,
                &com,
                r#"{"tipo":"porta-servir","n":3,"nome":"mc","protocolo":"tcp","porta":99999}"#
            ),
            Acao::Recusar {
                n: 3,
                erro: "fora-do-aceite",
                tipo: "porta-servir".to_owned()
            }
        );
        assert_eq!(
            tratar(
                &oferta,
                &["maquina"],
                r#"{"tipo":"porta-servir","n":4,"nome":"mc","protocolo":"tcp","porta":25565}"#
            ),
            Acao::Recusar {
                n: 4,
                erro: "tipo-desconhecido",
                tipo: "porta-servir".to_owned()
            },
            "sem o poder `porta`, um porta-servir escrito à mão passou"
        );
        assert_eq!(
            tratar(&oferta, &com, r#"{"tipo":"superficie-criar","n":5}"#),
            Acao::Recusar {
                n: 5,
                erro: "tipo-desconhecido",
                tipo: "superficie-criar".to_owned()
            }
        );
        assert_eq!(
            tratar(
                &oferta,
                &com,
                r#"{"tipo":"porta-parar","n":6,"nome":"outro"}"#
            ),
            Acao::Recusar {
                n: 6,
                erro: "fora-do-aceite",
                tipo: "porta-parar".to_owned()
            }
        );
        assert_eq!(
            tratar(&oferta, &com, r#"{"tipo":"maquina-parada-pronta"}"#),
            Acao::ParadaPronta
        );
    }

    // ------------------------------------------------- a bomba, de verdade

    /// Um fio que anota o que chegou a ele.
    #[derive(Default)]
    struct FioDeMentira {
        anotado: Mutex<Vec<String>>,
    }

    impl FioDeMentira {
        fn anotar(&self, linha: String) {
            self.anotado.lock().unwrap().push(linha);
        }
        fn anotado(&self) -> Vec<String> {
            self.anotado.lock().unwrap().clone()
        }
        /// Espera uma linha que comece com `prefixo`, até 5 s.
        fn esperar(&self, prefixo: &str) -> Vec<String> {
            let fim = Instant::now() + Duration::from_secs(5);
            while Instant::now() < fim {
                let anotado = self.anotado();
                if anotado.iter().any(|l| l.starts_with(prefixo)) {
                    return anotado;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("«{prefixo}» não chegou ao fio; chegou {:?}", self.anotado());
        }
    }

    impl Fio for FioDeMentira {
        fn pedir(&self, _: &str, _: &str, payload: String) -> Futuro<Result<String, &'static str>> {
            self.anotar(format!("pedir {payload}"));
            Box::pin(async { Ok("\"ok\"".to_owned()) })
        }
        fn servir(
            &self,
            _: &str,
            nome: &str,
            protocolo: &str,
            porta: u16,
        ) -> Futuro<Result<(), &'static str>> {
            self.anotar(format!("servir {nome} {protocolo} {porta}"));
            Box::pin(async { Ok(()) })
        }
        fn parar_tunel(&self, _: &str, nome: &str) -> Result<(), &'static str> {
            self.anotar(format!("parar-tunel {nome}"));
            Ok(())
        }
        fn quedas(&self, _: &str, nome: &str) -> Futuro<Result<Vec<(String, u32)>, &'static str>> {
            self.anotar(format!("quedas {nome}"));
            Box::pin(async { Ok(vec![("jogo-recusou".to_owned(), 2)]) })
        }
        fn parar_maquina(&self, _: &str, _: &str, motivo: &str) -> Result<(), &'static str> {
            self.anotar(format!("parar-maquina {motivo}"));
            Ok(())
        }
    }

    fn subir_com(
        fio: &Arc<FioDeMentira>,
        fonte: &str,
        falhas: Arc<Mutex<Vec<String>>>,
    ) -> Arc<Viva> {
        let viva = Viva::subir(
            Chave::de("casa", "x/jogo", "mundo"),
            1,
            oferta(),
            vec!["maquina", "maquina.porta"],
            Arc::clone(fio) as Arc<dyn Fio>,
            Box::new(move |_, texto| falhas.lock().unwrap().push(texto)),
        )
        .expect("a metade sobe");
        viva.iniciar(fonte).expect("o código entra");
        viva
    }

    /// **O túnel fora da oferta não chega ao fio**, pelo caminho inteiro: a
    /// metade de verdade, no executor de verdade, chama `maquina.porta.servir`
    /// com outra porta, a bomba recusa, e a metade lê `fora-do-aceite` no
    /// `catch` — que ela conta ao fio por `maquina.pedir`, o único jeito de o
    /// teste a ouvir.
    #[test]
    fn a_bomba_recusa_servir_fora_da_oferta() {
        let fio = Arc::new(FioDeMentira::default());
        let _viva = subir_com(
            &fio,
            "(async () => {\
               try { await maquina.porta.servir({ nome: 'mc', protocolo: 'tcp', porta: 25566 }); \
                     await maquina.pedir('serviu'); }\
               catch (e) { await maquina.pedir('recusado:' + e.erro); }\
               await maquina.porta.servir({ nome: 'mc', protocolo: 'tcp', porta: 25565 });\
               await maquina.pedir('serviu-o-de-dentro');\
             })();",
            Arc::default(),
        );
        let anotado = fio.esperar("pedir \"serviu-o-de-dentro\"");
        assert_eq!(
            anotado,
            vec![
                "pedir \"recusado:fora-do-aceite\"".to_owned(),
                "servir mc tcp 25565".to_owned(),
                "pedir \"serviu-o-de-dentro\"".to_owned(),
            ],
            "o túnel fora da oferta chegou ao fio, ou a metade não leu `fora-do-aceite`"
        );
    }

    /// **Desligar desliga, na ordem**: o `aoParar` recebe o motivo e roda
    /// antes de tudo, os túneis que ela serve caem, e o `MaquinaParada` sai.
    #[test]
    fn desligar_chama_o_ao_parar_derruba_os_tuneis_e_diz_ao_servidor() {
        let fio = Arc::new(FioDeMentira::default());
        let viva = subir_com(
            &fio,
            "maquina.aoParar(async (motivo) => { await maquina.pedir('parando:' + motivo); });\
             (async () => { await maquina.porta.servir({ nome: 'mc', protocolo: 'tcp', porta: 25565 });\
               await maquina.pedir('servindo'); })();",
            Arc::default(),
        );
        fio.esperar("pedir \"servindo\"");
        assert_eq!(viva.tuneis_servidos(), vec!["mc".to_owned()]);
        let relogio = Instant::now();
        assert!(viva.encerrar(Fim::PeloDono));
        assert!(
            relogio.elapsed() < ESPERA_DO_AO_PARAR,
            "a parada esperou o teto inteiro: o `aoParar` não disse que acabou"
        );
        assert!(
            !viva.encerrar(Fim::PeloDono),
            "o fim pedido duas vezes repetiu a parada"
        );
        assert_eq!(
            fio.anotado(),
            vec![
                "servir mc tcp 25565".to_owned(),
                "pedir \"servindo\"".to_owned(),
                "pedir \"parando:pelo-dono\"".to_owned(),
                "parar-tunel mc".to_owned(),
                "parar-maquina pelo-dono".to_owned(),
            ]
        );
    }

    /// Uma dispensa do servidor não vira `MaquinaParada`: quem decidiu foi ele.
    #[test]
    fn a_dispensa_nao_fala_de_volta_ao_servidor() {
        let fio = Arc::new(FioDeMentira::default());
        let viva = subir_com(
            &fio,
            "maquina.aoParar((m) => maquina.pedir('parando:' + m));",
            Arc::default(),
        );
        assert!(viva.encerrar(Fim::OutroPedido));
        assert_eq!(
            fio.anotado(),
            vec!["pedir \"parando:outro-pedido\"".to_owned()]
        );
    }

    /// **Uma promessa de topo que rejeita derruba a metade, dito.** `ao_falhar`
    /// é chamado com o texto do erro.
    #[test]
    fn a_metade_que_rejeita_no_topo_e_dada_por_falhada() {
        let fio = Arc::new(FioDeMentira::default());
        let falhas: Arc<Mutex<Vec<String>>> = Arc::default();
        let _viva = subir_com(
            &fio,
            "(async () => { throw new Error('quebrou'); })();",
            Arc::clone(&falhas),
        );
        let fim = Instant::now() + Duration::from_secs(5);
        while falhas.lock().unwrap().is_empty() && Instant::now() < fim {
            std::thread::sleep(Duration::from_millis(10));
        }
        let falhas = falhas.lock().unwrap().clone();
        assert_eq!(
            falhas.len(),
            1,
            "a rejeição de topo não derrubou a metade: {falhas:?}"
        );
        assert!(
            falhas.iter().all(|f| f.contains("quebrou")),
            "o texto do erro não veio: {falhas:?}"
        );
    }

    /// O aviso do servidor chega ao `aoAviso`, e as quedas voltam como pares.
    #[test]
    fn o_aviso_chega_e_as_quedas_voltam_como_pares() {
        let fio = Arc::new(FioDeMentira::default());
        let viva = subir_com(
            &fio,
            "maquina.aoAviso(async (carga) => {\
               const quedas = await maquina.porta.quedas('mc');\
               await maquina.pedir(carga + ':' + JSON.stringify(quedas));\
             });",
            Arc::default(),
        );
        viva.avisar("oi");
        let anotado = fio.esperar("pedir ");
        assert_eq!(
            anotado,
            vec![
                "quedas mc".to_owned(),
                r#"pedir "oi:[[\"jogo-recusou\",2]]""#.to_owned(),
            ]
        );
    }
}
