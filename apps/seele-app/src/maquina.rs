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

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use seele_ffi::mods::AceiteDeMaquinaGuardado;
use seele_ffi::{Connection, Event, OfertaDeMaquinaFfi, TunelOferecidoFfi};
use tauri::{AppHandle, Emitter as _, Manager as _};

use crate::executor;

mod programa;

pub(crate) use programa::SemPrograma;

/// Quanto o `aoParar` da metade tem para terminar antes de o executor ser
/// encerrado de qualquer jeito.
///
/// **Um teto, e não uma espera fixa**: a metade diz quando acabou
/// (`maquina-parada-pronta`), e no caso comum a parada leva milissegundos. O
/// teto é o que impede uma metade travada de segurar o DESLIGAR — ou a saída
/// do app — para sempre.
pub(crate) const ESPERA_DO_AO_PARAR: Duration = Duration::from_secs(2);

/// **O teto do `aoParar` de uma metade com processo vivo** (API 8 parte 4;
/// decisão Q9). O servidor do jogo salva o mundo ao receber `stop`, e matar
/// no meio perde chunks; o `aoParar` que manda `stop` e espera a saída tem
/// este tempo. Medido no Minecraft 1.21.4 deste Mac: ver as decisões.
pub(crate) const ESPERA_DO_AO_PARAR_COM_PROCESSO: Duration = Duration::from_secs(20);

/// Quanto `encerrar_tudo` tem para matar e esperar cada processo.
pub(crate) const ESPERA_DO_ENCERRAR_TUDO: Duration = Duration::from_secs(10);

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
    /// Em que papel a pessoa é chamada (`principal` ou `reserva`), como o fio
    /// trouxe: a tela diz que é a reserva, e o aceite guarda o papel.
    pub(crate) papel: String,
    /// O resumo que o núcleo calculou da oferta, em hex.
    pub(crate) resumo: String,
    /// O apelido de quem pediu, já trocado ([`apelido_de`]).
    pub(crate) pedido_por: String,
    /// De qual sessão cliente ele veio.
    pub(crate) sessao: u64,
    /// A ordem de chegada ([`Estado::numerar`]): o que diz, a quem o atende
    /// numa thread, se ele ainda é o último da chave ou se já foi dispensado.
    pub(crate) numero: u64,
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
    // O papel entra no resumo; a tela diz quando é ele que mudou (a pessoa
    // aceitou ser a reserva, e agora é chamada a principal, ou o contrário).
    if guardado.papel != pedido.papel {
        mudou.push("papel");
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
        instalar,
        por_sistema,
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
    se(*instalar != agora.instalar, "instalar");
    se(*por_sistema != agora.por_sistema, "por_sistema");
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
    /// `maquina.programa.*`: o pedido já lido.
    Programa {
        n: u64,
        pedido: programa::PedidoDoPrograma,
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
    let com_programa = capacidades.contains(&"maquina.programa");
    let recusar = |erro, tipo: String| Acao::Recusar { n, erro, tipo };
    match tipo.as_str() {
        // O poder `programa` (parte 4), só com a capacidade: sem ela, os
        // tipos dele são desconhecidos como todo outro.
        //
        // Escritos aqui, e não só em `programa::TIPOS`: o guarda do prelúdio
        // (`tests/frontend.rs`) confere cada tipo que ele pede contra este
        // corpo, e uma lista noutro arquivo o faria olhar o lugar errado.
        t @ ("programa-baixar"
        | "programa-escrever"
        | "programa-desempacotar"
        | "programa-rodar"
        | "processo-escrever"
        | "processo-matar"
        | "programa-recusas"
        | "programa-trazer"
        | "programa-ler"
        | "programa-empacotar"
        | "programa-oferecer-copia"
        | "programa-receber-copia")
            if com_programa =>
        {
            // A cópia do mundo (parte 7) só entra pelo túnel da própria
            // oferta: o nome tem de ser de um túnel TCP que o dono aceitou.
            if t == "programa-receber-copia"
                && !oferta
                    .tuneis
                    .iter()
                    .any(|tunel| tunel.nome == texto("tunel") && tunel.protocolo == "tcp")
            {
                return recusar("fora-do-aceite", tipo);
            }
            debug_assert!(programa::TIPOS.contains(&t));
            match programa::ler(t, &valor) {
                Ok(pedido) => Acao::Programa { n, pedido },
                Err(erro) => recusar(erro, tipo),
            }
        }
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
    /// **Um arquivo do volume da metade de servidor** (API 8 parte 6), pelo
    /// fluxo de volume de sempre: `payload` vai ao `aoPedir` dela, que
    /// autoriza respondendo o caminho. O erro é o texto do servidor.
    fn ler_volume(&self, mod_id: &str, payload: String) -> Futuro<Result<Vec<u8>, String>>;
    /// **Entra num túnel do MOD como quem joga** (API 8 parte 7): é por ele
    /// que a cópia do mundo vem da máquina dona. Devolve a porta local.
    fn entrar_tunel(&self, mod_id: &str, nome: &str) -> Futuro<Result<u16, &'static str>>;
    /// Sai do túnel em que [`Fio::entrar_tunel`] entrou.
    fn sair_tunel(&self, mod_id: &str, nome: &str);
    /// Se as conexões novas do túnel em que se entrou vão pela ligação direta
    /// (`Some(true)`), pela estrela (`Some(false)`), ou se não há relatório.
    fn direto(&self, mod_id: &str, nome: &str) -> Option<bool>;
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

    fn ler_volume(&self, mod_id: &str, payload: String) -> Futuro<Result<Vec<u8>, String>> {
        let conexao = Arc::clone(&self.0);
        let mod_id = mod_id.to_owned();
        Box::pin(async move { conexao.ler_imagem_mod(mod_id, 0, payload).await })
    }

    fn entrar_tunel(&self, mod_id: &str, nome: &str) -> Futuro<Result<u16, &'static str>> {
        let conexao = Arc::clone(&self.0);
        let (mod_id, nome) = (mod_id.to_owned(), nome.to_owned());
        Box::pin(async move {
            conexao
                .entrar_no_tunel(mod_id, nome)
                .await
                .map(|porta| porta.porta)
        })
    }

    fn sair_tunel(&self, mod_id: &str, nome: &str) {
        let _ = self.0.sair_do_tunel(mod_id.to_owned(), nome.to_owned());
    }

    fn direto(&self, mod_id: &str, nome: &str) -> Option<bool> {
        self.0
            .tuneis_abertos()
            .into_iter()
            .find(|r| r.mod_id == mod_id && r.nome == nome)
            .map(|r| matches!(r.caminho, seele_ffi::CaminhoDeTunel::Direto { .. }))
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
    /// O servidor dispensou com um motivo que este app não conhece (um
    /// servidor mais novo). Parar é o lado seguro: seguir servindo seria
    /// supor que o motivo novo não diz «pare».
    DispensaDesconhecida,
    /// **Quem hospeda não a chamou de volta a tempo**: ela seguiu rodando sem
    /// ele ([`ESPERA_PELA_VOLTA`]), e nenhum pedido igual ao aceito a religou.
    SemVolta,
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
            Self::DispensaDesconhecida => "dispensa-desconhecida",
            Self::SemVolta => "sem-volta",
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
            Self::ModDesligado
            | Self::OutroPedido
            | Self::SemAceite
            | Self::ServidorSaiu
            | Self::DispensaDesconhecida
            // Quem hospeda não a tem mais no livro: não há a quem dizer.
            | Self::SemVolta => None,
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
    /// O poder `programa` desta metade, montado do aceite guardado ao subir,
    /// ou por que não há um ([`programa::programa_para`]).
    programa: programa::ProgramaDaMetade,
    /// Os processos que ela rodou e que ainda vivem.
    processos: programa::Processos,
    /// As recusas do `programa` que a cola deu sozinha.
    recusas_da_cola: programa::RecusasDaCola,
    /// Num cadeado porque o executor tem o canal de saída (que não é `Sync`)
    /// e é tocado pela bomba, pelas respostas que voltam do fio e pela parada.
    executor: Mutex<executor::ExecutorQuickJs>,
    /// A conexão por onde ela fala. **Trocável**: a metade sobrevive à queda
    /// de quem hospeda, e religada numa sessão nova fala pela conexão dela
    /// ([`Maquinas::religar_se_orfa`]).
    fio: Mutex<Arc<dyn Fio>>,
    /// Os túneis que ela serve, pelo nome: é o que a parada derruba.
    tuneis: Mutex<BTreeSet<String>>,
    /// Pediu-se o fim: uma falha depois disto é o fim que se pediu, e não
    /// outra falha.
    parando: AtomicBool,
    sinais: Mutex<Sinais>,
    acordar: Condvar,
    /// **Quem ouve o jogo começar e acabar** (e2e de 31/10, passo 13): a
    /// faixa dizia «este computador serve…» enquanto a metade vivia, com ou sem
    /// o jogo rodando nela. Posto por [`ligar`]; `None` nas provas.
    ao_jogo: Mutex<Option<AoJogo>>,
}

/// Quem é chamado quando a metade falha sozinha.
pub(crate) type AoFalhar = Box<dyn Fn(&Arc<Viva>, String) + Send + Sync>;

/// Quem é chamado quando o jogo desta metade começa ou acaba: a chave, e se
/// algum processo do `programa` dela roda agora.
pub(crate) type AoJogo = Box<dyn Fn(&Chave, bool) + Send + Sync>;

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
        programa: programa::ProgramaDaMetade,
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
            programa,
            processos: programa::Processos::default(),
            recusas_da_cola: programa::RecusasDaCola::default(),
            executor: Mutex::new(executor),
            fio: Mutex::new(fio),
            tuneis: Mutex::new(BTreeSet::new()),
            parando: AtomicBool::new(false),
            sinais: Mutex::new(Sinais::default()),
            acordar: Condvar::new(),
            ao_jogo: Mutex::new(None),
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
            "plataforma": seele_ffi::programa::esta_plataforma(),
        })
        .to_string();
        self.executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iniciar_de_maquina(fonte, self.capacidades.clone(), dados)
            .map_err(|_| "executor-nao-aceitou-o-codigo")
    }

    /// **Encerra sem falar com ninguém**: a metade subiu mas o aceite dela
    /// nunca saiu (ou saiu e foi dispensado antes do código entrar). Sem
    /// `aoParar` — não há código para ouvi-lo — e sem `MaquinaParada`, porque o
    /// servidor nunca a teve como máquina.
    fn descartar(&self) {
        if self.parando.swap(true, Ordering::AcqRel) {
            return;
        }
        self.executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pedir_encerramento();
        programa::encerrar_tudo(self, ESPERA_DO_ENCERRAR_TUDO);
        tracing::debug!(mod_id = %self.chave.mod_id, servidor = %self.chave.servidor, instancia = self.numero, "metade de máquina descartada antes de ligar");
    }

    /// A conexão por onde ela fala agora.
    pub(crate) fn fio(&self) -> Arc<dyn Fio> {
        Arc::clone(&self.fio.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Passa a falar por outra conexão (a da sessão em que foi religada).
    fn trocar_fio(&self, fio: Arc<dyn Fio>) {
        *self.fio.lock().unwrap_or_else(PoisonError::into_inner) = fio;
    }

    /// **A metade foi religada** depois de quem hospeda voltar: o
    /// `maquina.aoReconectar` dela ouve quanto tempo ela ficou sem ele, e é
    /// a vez de ela servir de novo o túnel e perguntar quem é a dona.
    pub(crate) fn reconectar(&self, fora: Duration) {
        if !self.de_pe() {
            return;
        }
        self.entregar(
            &serde_json::json!({ "tipo": "reconectar-maquina", "fora": fora.as_secs() })
                .to_string(),
        );
    }

    /// Quem ouve o jogo começar e acabar ([`AoJogo`]).
    pub(crate) fn ao_mudar_o_jogo(&self, ouvinte: AoJogo) {
        *self.ao_jogo.lock().unwrap_or_else(PoisonError::into_inner) = Some(ouvinte);
    }

    /// **Algum processo que ela rodou ainda roda**: o jogo (ou o instalador
    /// dele) está de pé neste computador.
    pub(crate) fn jogo_rodando(&self) -> bool {
        self.processos.quantos() > 0
    }

    /// Diz a quem ouve se o jogo roda agora; chamado quando um processo começa
    /// e quando acaba.
    pub(crate) fn dizer_o_jogo(&self) {
        let rodando = self.jogo_rodando();
        if let Some(ouvinte) = self
            .ao_jogo
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            ouvinte(&self.chave, rodando);
        }
    }

    /// Quantos processos do `programa` dela estão vivos.
    pub(crate) fn processos_vivos(&self) -> usize {
        self.programa.as_ref().map_or(0, |p| p.processos_vivos())
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

    /// Manda ao executor, e devolve a recusa dele sem dizer nada.
    fn tentar_entregar(&self, json: &str) -> Result<(), &'static str> {
        self.executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entregar(json)
    }

    /// Manda ao executor; uma recusa é dita, com o tamanho.
    fn entregar(&self, json: &str) {
        let resultado = self.tentar_entregar(json);
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
        // **Sob o cadeado dos túneis**: um `servir` que volta do fio confere
        // `de_pe()` com o mesmo cadeado na mão. Ou ele entra no conjunto antes
        // desta marca (e a tomada de baixo o derruba), ou vê a marca e se
        // derruba sozinho — nunca entra num conjunto que já foi tomado.
        let tuneis_trancados = self.tuneis.lock().unwrap_or_else(PoisonError::into_inner);
        let ja_parando = self.parando.swap(true, Ordering::AcqRel);
        drop(tuneis_trancados);
        if ja_parando {
            return false;
        }
        let (mod_id, servidor) = (&self.chave.mod_id, &self.chave.servidor);
        if fim.chama_ao_parar() {
            self.entregar(
                &serde_json::json!({ "tipo": "parar-maquina", "motivo": fim.nome() }).to_string(),
            );
            let relogio = Instant::now();
            // Com um processo vivo, o `aoParar` tem o tempo de o jogo salvar.
            let teto = if self.processos_vivos() > 0 {
                ESPERA_DO_AO_PARAR_COM_PROCESSO
            } else {
                ESPERA_DO_AO_PARAR
            };
            let sinais = self.sinais.lock().unwrap_or_else(PoisonError::into_inner);
            let (sinais, espera) = self
                .acordar
                .wait_timeout_while(sinais, teto, |s| !s.pronta && !s.parou)
                .unwrap_or_else(PoisonError::into_inner);
            drop(sinais);
            if espera.timed_out() {
                // Dito: uma parada que custa sempre o teto inteiro é uma metade
                // cujo `aoParar` não termina, e quem escreveu precisa saber.
                tracing::warn!(
                    mod_id = %mod_id,
                    servidor = %servidor,
                    motivo = fim.nome(),
                    teto = ?teto,
                    "metade de máquina: o aoParar não terminou no teto; encerrada assim mesmo"
                );
            } else {
                tracing::debug!(mod_id = %mod_id, servidor = %servidor, levou = ?relogio.elapsed(), "metade de máquina: aoParar terminou");
            }
        }
        // **Todo processo e todo download do `programa` morrem aqui**, em todo
        // motivo (DESLIGAR, REVOGAR, o app fechando, o MOD desligado, a
        // sessão que acabou, a metade que falhou), e a parada espera.
        programa::encerrar_tudo(self, ESPERA_DO_ENCERRAR_TUDO);
        let tuneis =
            std::mem::take(&mut *self.tuneis.lock().unwrap_or_else(PoisonError::into_inner));
        for nome in tuneis {
            if let Err(motivo) = self.fio().parar_tunel(mod_id, &nome) {
                tracing::info!(mod_id = %mod_id, servidor = %servidor, tunel = %nome, motivo, "metade de máquina: o túnel não foi parado no fio");
            }
        }
        self.executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pedir_encerramento();
        if let Some(parada) = fim.parada_no_fio() {
            if let Err(motivo) = self.fio().parar_maquina(mod_id, servidor, parada) {
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
        Acao::Programa { n, pedido } => programa::atender(viva, n, pedido),
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
                viva.fio().pedir(&mod_id, &servidor, payload),
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
                viva.fio().servir(&mod_id, &nome, &protocolo, porta),
            );
            tauri::async_runtime::spawn(async move {
                match futuro.await {
                    Ok(()) => {
                        // **O fim pode ter sido pedido enquanto o servir estava
                        // no fio.** Conferido com o cadeado dos túneis na mão,
                        // o mesmo sob que `encerrar` marca o fim e toma o
                        // conjunto: sem isto, o túnel entrava num conjunto já
                        // tomado e ficava servido sem dono até a sessão acabar
                        // — o DESLIGAR do dono sem desligar.
                        let mut tuneis = dela.tuneis.lock().unwrap_or_else(PoisonError::into_inner);
                        if !dela.de_pe() {
                            drop(tuneis);
                            tracing::info!(mod_id = %dela.chave.mod_id, servidor = %dela.chave.servidor, tunel = %nome, porta, "metade de máquina: servir voltou depois do fim pedido; o túnel é parado já");
                            if let Err(motivo) = dela.fio().parar_tunel(&dela.chave.mod_id, &nome) {
                                tracing::warn!(mod_id = %dela.chave.mod_id, servidor = %dela.chave.servidor, tunel = %nome, motivo, "metade de máquina: o túnel servido tarde não foi parado no fio");
                            }
                            dela.responder(n, Err("sem-aceite"));
                            return;
                        }
                        tuneis.insert(nome.clone());
                        drop(tuneis);
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
            let resultado = viva.fio().parar_tunel(&mod_id, &nome);
            viva.tuneis
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&nome);
            viva.responder(n, resultado.map(|()| serde_json::Value::Null));
        }
        Acao::Quedas { n, nome } => {
            let (dela, futuro) = (Arc::clone(viva), viva.fio().quedas(&mod_id, &nome));
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
    /// **Órfã**: a conexão com quem hospeda caiu, e ela segue rodando sem ele
    /// (M18 revista: ver [`Maquinas::orfanar`]).
    orfa: Option<Orfa>,
}

/// Desde quando uma metade segue sem quem hospeda, e a vez dessa orfandade:
/// o prazo de uma vez não para a metade religada e órfã de novo depois.
#[derive(Debug, Clone, Copy)]
struct Orfa {
    desde: Instant,
    vez: u64,
}

/// **Quanto uma metade segue rodando sem ser chamada de volta** (M18 revista).
///
/// Sem quem hospeda, a dona segue rodando (spec «A reserva», último item); na
/// volta dele, a metade de servidor a chama de novo (`MaquinaVoltou`, ou o
/// MOD que chama quem guardou), e o pedido igual ao aceito a religa sem subir
/// de novo. Uma metade que ninguém chama nesse tempo para, salva, e a faixa
/// diz por quê (`sem-volta`). Dez minutos cobrem os cinco da bateria do enlace
/// e uma volta de quem hospeda que reinicia o computador.
pub(crate) const ESPERA_PELA_VOLTA: Duration = Duration::from_secs(10 * 60);

/// Um `MachineRequest` que chegou antes de a sessão estar no registro.
struct ChegadoCedo {
    sessao: u64,
    alvo: String,
    mod_id: String,
    hash: String,
    servidor: String,
    pedido_por: u64,
    oferta: OfertaDeMaquinaFfi,
    papel: String,
    resumo: String,
    /// A ordem de chegada, dada na thread do enlace ([`Estado::numerar`]).
    numero: u64,
}

#[derive(Default)]
struct Estado {
    pendentes: BTreeMap<Chave, PedidoRecebido>,
    chegados_cedo: Vec<ChegadoCedo>,
    ligadas: BTreeMap<Chave, Ligada>,
    proxima: u64,
    /// **A ordem dos pedidos, dada na thread do enlace.** Cada chegada é
    /// atendida numa thread (decidir lê o disco), e a `vez` serializa as
    /// threads mas não as ordena: um pedido mais velho podia escrever por cima
    /// do diálogo do mais novo, e uma dispensa que chegou depois de um pedido
    /// podia rodar antes dele — e o diálogo abria para um pedido que já tinha
    /// acabado. O número é dado aqui, na ordem do fio, e conferido onde o
    /// pedido vira diálogo ou metade.
    numero_do_pedido: u64,
    /// O último pedido de cada chave: `(número, sessão)`.
    ultimos: BTreeMap<Chave, (u64, u64)>,
    /// Por `(chave, sessão)`: todo pedido de número menor foi dispensado.
    cortes: BTreeMap<(Chave, u64), u64>,
    /// Sobe a cada metade que fica órfã ([`Orfa::vez`]).
    vez_da_orfa: u64,
    /// Quantas metades foram religadas sem subir de novo, desde que o app abriu.
    religadas: u64,
}

impl Estado {
    /// Um pedido chegou do fio: ele passa a ser o último da chave.
    fn numerar(&mut self, chave: &Chave, sessao: u64) -> u64 {
        self.numero_do_pedido = self.numero_do_pedido.saturating_add(1);
        let numero = self.numero_do_pedido;
        self.ultimos.insert(chave.clone(), (numero, sessao));
        numero
    }

    /// Uma dispensa da chave nesta sessão: os pedidos dela que já chegaram e
    /// ainda estão numa thread não viram nem diálogo nem metade.
    fn cortar(&mut self, chave: &Chave, sessao: u64) {
        let ate = self.numero_do_pedido.saturating_add(1);
        self.cortes.insert((chave.clone(), sessao), ate);
    }

    /// O pedido `numero` da chave foi dispensado depois de chegar.
    fn cortado(&self, chave: &Chave, sessao: u64, numero: u64) -> bool {
        self.cortes
            .get(&(chave.clone(), sessao))
            .is_some_and(|ate| numero < *ate)
    }

    /// O pedido ainda é o que a chave espera: nenhum mais novo chegou e
    /// nenhuma dispensa o cortou. Só ele vira diálogo.
    fn e_o_ultimo(&self, chave: &Chave, sessao: u64, numero: u64) -> bool {
        self.ultimos.get(chave) == Some(&(numero, sessao)) && !self.cortado(chave, sessao, numero)
    }

    /// A sessão acabou: a ordem dela não vale mais nada.
    fn esquecer_sessao(&mut self, sessao: u64) {
        self.ultimos.retain(|_, (_, s)| *s != sessao);
        self.cortes.retain(|(_, s), _| *s != sessao);
    }
}

/// O que uma `MachineDispensed` tira do registro.
struct Dispensa {
    fim: Fim,
    /// Os pedidos que esperavam a tela, com o resumo de cada um.
    desfeitos: Vec<(Chave, String)>,
    vivas: Vec<Arc<Viva>>,
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
    /// [`ESPERA_PELA_VOLTA`], ou outra (em ms) nas provas.
    espera_pela_volta_ms: std::sync::atomic::AtomicU64,
    /// Os jogos de uma execução anterior que este app achou rodando e parou
    /// ao abrir ([`procurar_sobras`]), para a faixa de uma janela que
    /// recarrega.
    sobras: Mutex<Vec<SobraNaTela>>,
    /// **Nenhuma metade sobe enquanto um jogo que sobrou ainda salva**
    /// (revisão da 0.19, I1): [`PortaDasSobras`].
    porta_das_sobras: PortaDasSobras,
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

    /// **Tira os pedidos que ainda esperam a tela** e diz quais eram.
    ///
    /// Quem tira é quem avisa: a tela só sabe que um pedido deixou de existir se
    /// alguém lhe disser (`maquina-estado`), e só os que estavam aqui merecem o
    /// aviso — uma chave sem pedido não tem diálogo aberto.
    fn tirar_pendentes(
        &self,
        qual: impl Fn(&Chave, &PedidoRecebido) -> bool,
    ) -> Vec<(Chave, String)> {
        let mut estado = self.estado();
        let chaves: Vec<Chave> = estado
            .pendentes
            .iter()
            .filter(|(chave, pedido)| qual(chave, pedido))
            .map(|(chave, _)| chave.clone())
            .collect();
        chaves
            .into_iter()
            .filter_map(|chave| {
                estado
                    .pendentes
                    .remove(&chave)
                    .map(|pedido| (chave, pedido.resumo))
            })
            .collect()
    }

    /// Esta chave está ligada agora?
    fn ligada(&self, chave: &Chave) -> bool {
        self.estado().ligadas.contains_key(chave)
    }

    /// O jogo desta chave roda agora neste computador?
    fn jogo_rodando(&self, chave: &Chave) -> bool {
        self.estado()
            .ligadas
            .get(chave)
            .is_some_and(|l| l.viva.jogo_rodando())
    }

    /// **O que uma dispensa do servidor tira**, para encerrar e dizer fora do
    /// cadeado.
    ///
    /// `sem-aceite` fala só de uma metade de pé: é a resposta a um aceite que
    /// o livro não tomou, ou a um pedido de quem não é a máquina, e nunca de
    /// um pedido pendente — fechar o diálogo aberto de outro pedido da mesma
    /// chave seria tirar da tela uma pergunta que ainda vale. Um motivo que
    /// este app não conhece também para a metade (e só ela): seguir servindo
    /// seria supor que ele não diz «pare». Os outros motivos levam os pedidos
    /// da chave nesta sessão, inclusive os que ainda estão numa thread.
    fn dispensar(&self, chave: &Chave, sessao: u64, motivo: &str) -> Dispensa {
        let conhecido = Fim::da_dispensa(motivo);
        let fim = conhecido.unwrap_or(Fim::DispensaDesconhecida);
        let leva_os_pedidos = conhecido.is_some_and(|f| f != Fim::SemAceite);
        let desfeitos = if leva_os_pedidos {
            self.estado().cortar(chave, sessao);
            self.tirar_pendentes(|c, p| c == chave && p.sessao == sessao)
        } else {
            Vec::new()
        };
        let vivas = self.tirar_ligadas(|c, l| c == chave && l.sessao == sessao);
        Dispensa {
            fim,
            desfeitos,
            vivas,
        }
    }

    /// **O pedido vira o diálogo — se ainda for o último da chave.** A
    /// conferência, a entrada nos pendentes e o `mostrar` (o `maquina-pedido`)
    /// acontecem sob o mesmo cadeado que a dispensa toma: ou a dispensa vem
    /// antes e o diálogo não abre, ou vem depois e acha o pendente para fechar.
    fn guardar_pendente_se_vale(&self, pedido: PedidoRecebido, mostrar: impl FnOnce()) -> bool {
        let mut estado = self.estado();
        if !estado.e_o_ultimo(&pedido.chave, pedido.sessao, pedido.numero) {
            return false;
        }
        estado.pendentes.insert(pedido.chave.clone(), pedido);
        mostrar();
        true
    }

    /// **A metade entra no registro — se o pedido dela não foi dispensado.**
    /// Antes do `AceiteDeMaquina` sair: a resposta a ele (um `sem-aceite`) pode
    /// voltar antes de quem liga chegar à linha seguinte, e precisa achar a
    /// metade aqui para pará-la.
    fn registrar_se_vale(&self, chave: &Chave, numero: u64, ligada: Ligada) -> bool {
        let mut estado = self.estado();
        if estado.cortado(chave, ligada.sessao, numero) {
            return false;
        }
        estado.ligadas.insert(chave.clone(), ligada);
        true
    }

    /// Quanto uma órfã espera ser chamada de volta.
    pub(crate) fn espera_pela_volta(&self) -> Duration {
        match self.espera_pela_volta_ms.load(Ordering::Relaxed) {
            0 => ESPERA_PELA_VOLTA,
            ms => Duration::from_millis(ms),
        }
    }

    /// **As metades que ficam sem quem hospeda** (M18 revista, «sem quem
    /// hospeda»): marcadas órfãs, **e não paradas** — o executor, os processos
    /// e os túneis delas seguem, e a dona segue rodando. Devolve cada uma que
    /// ficou órfã agora, com a vez da orfandade, para o prazo
    /// ([`ESPERA_PELA_VOLTA`]). Uma que já era órfã segue com o prazo que tinha.
    fn orfanar(&self, qual: impl Fn(&Chave, &Ligada) -> bool) -> Vec<(Arc<Viva>, u64)> {
        let mut estado = self.estado();
        let agora = Instant::now();
        let mut vez = estado.vez_da_orfa;
        let orfas: Vec<(Arc<Viva>, u64)> = estado
            .ligadas
            .iter_mut()
            .filter(|(chave, ligada)| ligada.orfa.is_none() && qual(chave, ligada))
            .map(|(_, ligada)| {
                vez = vez.saturating_add(1);
                ligada.orfa = Some(Orfa { desde: agora, vez });
                (Arc::clone(&ligada.viva), vez)
            })
            .collect();
        estado.vez_da_orfa = vez;
        orfas
    }

    /// **As metades desta sessão de antes de uma reconexão do enlace.** O
    /// servidor, ao ver a conexão velha cair, tirou o aceite delas do livro
    /// (`maquinas::saiu`). Antes (M18) elas paravam aqui com `servidor-saiu`
    /// — e o jogo da dona parava junto, mesmo de quem jogava pela malha. Agora
    /// ficam órfãs até o pedido igual ao aceito as religar.
    fn orfanar_de_geracao_velha(&self, sessao: u64, geracao: u64) -> Vec<(Arc<Viva>, u64)> {
        self.orfanar(|_, l| l.sessao == sessao && l.geracao < geracao)
    }

    /// **O pedido igual ao aceito religa a órfã**, sem subir de novo: a mesma
    /// metade, os mesmos processos, a conexão da sessão que a chamou. Devolve
    /// a metade e quanto ela ficou sem quem hospeda; `None` se não há órfã
    /// desta chave com este resumo e este pacote.
    fn religar_se_orfa(
        &self,
        pedido: &PedidoRecebido,
        geracao: u64,
        fio: Arc<dyn Fio>,
    ) -> Option<(Arc<Viva>, Duration)> {
        let mut estado = self.estado();
        let ligada = estado.ligadas.get_mut(&pedido.chave)?;
        let orfa = ligada.orfa?;
        if ligada.resumo != pedido.resumo || ligada.hash != pedido.hash || !ligada.viva.de_pe() {
            return None;
        }
        ligada.orfa = None;
        ligada.sessao = pedido.sessao;
        ligada.geracao = geracao;
        ligada.viva.trocar_fio(fio);
        let viva = Arc::clone(&ligada.viva);
        estado.religadas = estado.religadas.saturating_add(1);
        Some((viva, orfa.desde.elapsed()))
    }

    /// **A órfã cujo prazo venceu**: se ela ainda é órfã **daquela vez**, sai
    /// do registro para parar.
    fn tirar_orfa_vencida(&self, chave: &Chave, numero: u64, vez: u64) -> Option<Arc<Viva>> {
        let mut estado = self.estado();
        let vencida = estado
            .ligadas
            .get(chave)
            .is_some_and(|l| l.viva.numero == numero && l.orfa.is_some_and(|o| o.vez == vez));
        if !vencida {
            return None;
        }
        estado.ligadas.remove(chave).map(|l| l.viva)
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
    /// **O nome do servidor do SEELE** de onde veio o pedido (e2e de 31/10,
    /// passo 5): a tela dizia «no servidor minecraft-1», o id do jogo, como
    /// se fosse o servidor do SEELE.
    pub(crate) servidor_seele: String,
    /// O apelido, ou «alguém deste servidor».
    pub(crate) pedido_por: String,
    pub(crate) oferta: OfertaDeMaquinaFfi,
    /// `principal` ou `reserva`: a tela diz, no alto e numa linha, que este
    /// computador é a reserva — só roda o jogo se a principal sair, com uma
    /// cópia do mundo.
    pub(crate) papel: String,
    pub(crate) resumo: String,
    /// Onde a parte 4 vai pôr o jogo (`pasta_do_servidor_de_jogo`).
    pub(crate) pasta: String,
    pub(crate) mudou: Option<Vec<&'static str>>,
    /// **Os artefatos que este computador baixa** (parte 4), pela regra da
    /// política (`seele_ffi::programa::artefatos_desta_maquina`): a tela lista
    /// e soma só estes.
    pub(crate) artefatos_daqui: Vec<seele_ffi::ArtefatoOferecidoFfi>,
    /// Quantos artefatos da oferta são de outros sistemas.
    pub(crate) de_outros_sistemas: usize,
    /// **Os comandos que este computador roda** (decisão W1), pela regra da
    /// política (`seele_ffi::programa::comandos_desta_maquina`): o par da
    /// entrada de `por_sistema` deste sistema, ou o de cima. A tela mostra
    /// estes, e não os da oferta crua.
    pub(crate) rodar_daqui: Option<seele_ffi::ComandoOferecidoFfi>,
    /// O passo de instalar deste computador.
    pub(crate) instalar_daqui: Option<seele_ffi::ComandoOferecidoFfi>,
    /// Quantos outros sistemas têm comandos próprios na oferta.
    pub(crate) sistemas_com_outros_comandos: usize,
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
    /// **De que pedido o aviso fala**, só quando é o fim de um pedido pendente.
    /// Sem ele (a máquina que estava de pé), a tela não fecha diálogo nenhum:
    /// um pedido novo da mesma chave não é o que acabou.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resumo: Option<String>,
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
    /// Algum processo do jogo roda agora neste computador.
    pub(crate) jogo: bool,
    pub(crate) tuneis: Vec<TunelOferecidoFfi>,
}

/// `maquina-jogo`: o jogo desta chave começou ou acabou neste computador.
pub(crate) const EVENTO_DO_JOGO: &str = "maquina-jogo";

/// `maquina-jogo`, como a casca o lê.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct JogoNaTela {
    pub(crate) alvo: String,
    pub(crate) mod_id: String,
    pub(crate) servidor: String,
    pub(crate) rodando: bool,
}

fn emitir_o_jogo<R: tauri::Runtime>(app: &AppHandle<R>, chave: &Chave, rodando: bool) {
    let emitido = app.emit(
        EVENTO_DO_JOGO,
        JogoNaTela {
            alvo: chave.alvo.clone(),
            mod_id: chave.mod_id.clone(),
            servidor: chave.servidor.clone(),
            rodando,
        },
    );
    if let Err(erro) = emitido {
        tracing::warn!(mod_id = %chave.mod_id, servidor = %chave.servidor, rodando, %erro, "maquina-jogo não chegou à janela");
    }
}

fn emitir_estado<R: tauri::Runtime>(
    app: &AppHandle<R>,
    chave: &Chave,
    ligada: bool,
    motivo: Option<&'static str>,
) {
    emitir_estado_de_pedido(app, chave, ligada, motivo, None);
}

fn emitir_estado_de_pedido<R: tauri::Runtime>(
    app: &AppHandle<R>,
    chave: &Chave,
    ligada: bool,
    motivo: Option<&'static str>,
    resumo: Option<String>,
) {
    let emitido = app.emit(
        EVENTO_DO_ESTADO,
        EstadoNaTela {
            alvo: chave.alvo.clone(),
            mod_id: chave.mod_id.clone(),
            servidor: chave.servidor.clone(),
            ligada,
            motivo,
            resumo,
        },
    );
    if let Err(erro) = emitido {
        tracing::warn!(mod_id = %chave.mod_id, servidor = %chave.servidor, ligada, motivo, %erro, "maquina-estado não chegou à janela");
    }
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
    geracao: u64,
    alvo: &str,
    evento: &Event,
) -> bool {
    // Todo evento traz a geração do enlace: o primeiro de uma volta nova diz
    // que a conexão velha caiu, e com ela o aceite das metades de antes. Elas
    // **não param** (M18 revista): ficam órfãs, rodando, até o pedido igual
    // ao aceito as religar — ou até o prazo, que as para com `sem-volta`.
    if let Some(estado) = sessoes(app) {
        let orfas = estado.maquinas.orfanar_de_geracao_velha(sessao, geracao);
        if !orfas.is_empty() {
            tracing::warn!(sessao, geracao, quantas = orfas.len(), "o enlace voltou de uma queda: as metades de máquina de antes seguem rodando, órfãs, até serem chamadas de volta");
            orfanar_e_vigiar(app, &estado.maquinas, orfas);
        }
    }
    match evento {
        Event::MachineRequest {
            mod_id,
            hash,
            servidor,
            pedido_por,
            oferta,
            papel,
            resumo,
        } => {
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
            // O número sai aqui, na thread do enlace e na ordem do fio: é ele
            // que a thread que atende confere antes de mostrar ou ligar.
            let numero = maquinas.numerar(&Chave::de(alvo, mod_id, servidor), sessao);
            let chegado = ChegadoCedo {
                sessao,
                alvo: alvo.to_owned(),
                mod_id: mod_id.clone(),
                hash: hash.clone(),
                servidor: servidor.clone(),
                pedido_por: *pedido_por,
                oferta: (**oferta).clone(),
                papel: papel.clone(),
                resumo: resumo.clone(),
                numero,
            };
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
            let Some(estado) = sessoes(app) else {
                return true;
            };
            let dispensa = estado.maquinas.dispensar(&chave, sessao, motivo);
            if dispensa.fim == Fim::DispensaDesconhecida {
                tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo, metades = dispensa.vivas.len(), "dispensa de máquina com motivo desconhecido: a metade para assim mesmo");
            } else {
                tracing::info!(mod_id = %mod_id, servidor = %servidor, motivo, pedidos = dispensa.desfeitos.len(), metades = dispensa.vivas.len(), "o servidor dispensou a máquina");
            }
            // O diálogo aberto para este pedido não sabe que ele acabou: o
            // `maquina-estado` é o que o fecha.
            dizer_pedidos_desfeitos(app, &dispensa.desfeitos, dispensa.fim);
            encerrar_e_dizer(app, dispensa.vivas, dispensa.fim);
            true
        }
        Event::Ended { reason } => {
            // **Quem hospeda saiu, e a sessão acabou com ele** (a bateria
            // descarregou, ou ele fechou o servidor): as metades seguem
            // rodando, órfãs, e uma sessão nova com o mesmo servidor as religa
            // pelo pedido igual. Nos outros fins (expulso, banido, recusado),
            // param como antes. A saída pedida pela pessoa já as parou antes
            // de chegar aqui (`desmontar_o_cliente`).
            if sem_quem_hospeda(*reason) {
                if let Some(estado) = sessoes(app) {
                    let orfas = estado.maquinas.orfanar(|_, l| l.sessao == sessao);
                    if !orfas.is_empty() {
                        tracing::warn!(sessao, motivo = ?reason, quantas = orfas.len(), "a sessão acabou sem quem hospeda: as metades de máquina seguem rodando, órfãs");
                        orfanar_e_vigiar(app, &estado.maquinas, orfas);
                    }
                }
            }
            sessao_saiu(app, sessao);
            false
        }
        _ => false,
    }
}

/// **Os fins de sessão que são quem hospeda indo embora**, e não esta pessoa
/// sendo tirada: a conexão perdida (a bateria que descarregou), o servidor
/// encerrando, a manutenção. Neles a metade de máquina segue órfã.
fn sem_quem_hospeda(motivo: seele_ffi::EndReason) -> bool {
    matches!(
        motivo,
        seele_ffi::EndReason::LinkLost
            | seele_ffi::EndReason::ServerShuttingDown
            | seele_ffi::EndReason::ScheduledMaintenance
    )
}

/// **As órfãs ficam na faixa, e cada uma tem o seu prazo.** A faixa diz que
/// a metade segue sem quem hospeda (`sem-anfitriao`, ligada); passado
/// [`ESPERA_PELA_VOLTA`] sem ser chamada de volta, ela para, salva, e a faixa
/// diz `sem-volta`.
fn orfanar_e_vigiar<R: tauri::Runtime>(
    app: &AppHandle<R>,
    maquinas: &Maquinas,
    orfas: Vec<(Arc<Viva>, u64)>,
) {
    let espera = maquinas.espera_pela_volta();
    for (viva, vez) in orfas {
        tracing::warn!(mod_id = %viva.chave.mod_id, servidor = %viva.chave.servidor, instancia = viva.numero, processos = viva.processos_vivos(), espera = ?espera, "metade de máquina sem quem hospeda: segue rodando, órfã, até ser chamada de volta");
        emitir_estado(app, &viva.chave, true, Some(MOTIVO_DA_ORFA));
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(espera);
            let Some(estado) = sessoes(&app) else { return };
            let Some(vencida) = estado
                .maquinas
                .tirar_orfa_vencida(&viva.chave, viva.numero, vez)
            else {
                return;
            };
            tracing::warn!(mod_id = %vencida.chave.mod_id, servidor = %vencida.chave.servidor, instancia = vencida.numero, espera = ?espera, motivo = Fim::SemVolta.nome(), "metade de máquina órfã que ninguém chamou de volta: para");
            if vencida.encerrar(Fim::SemVolta) {
                emitir_estado(&app, &vencida.chave, false, Some(Fim::SemVolta.nome()));
            }
        });
    }
}

/// O motivo que a faixa lê numa metade **ligada** e órfã.
pub(crate) const MOTIVO_DA_ORFA: &str = "sem-anfitriao";

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

/// **Diz à tela que estes pedidos acabaram**: `maquina-estado` desligada, com o
/// motivo da dispensa (ou `servidor-saiu`), para cada chave que ainda esperava.
/// O `resumo` vai junto: é o que diz à tela QUAL pedido acabou.
fn dizer_pedidos_desfeitos<R: tauri::Runtime>(
    app: &AppHandle<R>,
    desfeitos: &[(Chave, String)],
    fim: Fim,
) {
    for (chave, resumo) in desfeitos {
        emitir_estado_de_pedido(app, chave, false, Some(fim.nome()), Some(resumo.clone()));
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
    let desfeitos = estado.maquinas.tirar_pendentes(|_, p| p.sessao == sessao);
    {
        let mut maquinas = estado.maquinas.estado();
        maquinas.chegados_cedo.retain(|c| c.sessao != sessao);
        maquinas.esquecer_sessao(sessao);
    }
    dizer_pedidos_desfeitos(app, &desfeitos, Fim::ServidorSaiu);
    // **As órfãs não são mais desta sessão**: seguem até serem chamadas de
    // volta ou até o prazo delas. Dispensar a placa de uma sessão que quem
    // hospeda encerrou não para o jogo; DESLIGAR, na faixa, para.
    let vivas = estado
        .maquinas
        .tirar_ligadas(|_, l| l.sessao == sessao && l.orfa.is_none());
    encerrar_e_dizer(app, vivas, Fim::ServidorSaiu);
}

/// O mesmo, para quem não tem `AppHandle` (a entrada solta): sem evento,
/// porque essa sessão nunca chegou à tela.
pub(crate) fn sessao_saiu_sem_tela(maquinas: &Maquinas, sessao: u64) {
    {
        let mut estado = maquinas.estado();
        estado.pendentes.retain(|_, p| p.sessao != sessao);
        estado.chegados_cedo.retain(|c| c.sessao != sessao);
        estado.esquecer_sessao(sessao);
    }
    for viva in maquinas.tirar_ligadas(|_, l| l.sessao == sessao && l.orfa.is_none()) {
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
    // **Antes da vez**: a espera pelo jogo que sobrou é de segundos, e um
    // pedido de outra chave não fica preso atrás dela no cadeado.
    esperar_as_sobras(
        &estado.maquinas,
        &Chave::de(&chegado.alvo, &chegado.mod_id, &chegado.servidor),
    );
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
    // Um pedido mais novo da chave, ou uma dispensa, chegou depois deste: quem
    // atende é o mais novo, e o dispensado não abre diálogo nem liga.
    if !estado
        .maquinas
        .estado()
        .e_o_ultimo(&chave, chegado.sessao, chegado.numero)
    {
        tracing::info!(mod_id = %chegado.mod_id, servidor = %chegado.servidor, numero = chegado.numero, "pedido de máquina passado por um mais novo ou dispensado antes de ser atendido: descartado");
        return;
    }
    let pedido = PedidoRecebido {
        chave: chave.clone(),
        hash: chegado.hash,
        oferta: chegado.oferta,
        papel: chegado.papel,
        resumo: chegado.resumo,
        pedido_por: apelido_de(&conexao, chegado.pedido_por),
        sessao: chegado.sessao,
        numero: chegado.numero,
    };

    // **A volta de quem hospeda**: o pedido igual ao aceito religa a órfã
    // desta chave — a mesma metade, os mesmos processos —, e não sobe outra.
    // O aceite sai de novo (o livro dele nasceu vazio, ou tirou o de antes), e
    // só depois o `aoReconectar` dela: servir o túnel antes do aceite seria
    // ouvir `fora-do-aceite`.
    let geracao_agora = conexao.carimbo().geracao;
    if let Some((viva, fora)) = estado.maquinas.religar_se_orfa(
        &pedido,
        geracao_agora,
        Arc::new(FioDaConexao(Arc::clone(&conexao))),
    ) {
        tracing::info!(mod_id = %chave.mod_id, servidor = %chave.servidor, instancia = viva.numero, processos = viva.processos_vivos(), fora = ?fora, "metade de máquina órfã religada pelo pedido igual, sem subir de novo");
        if let Err(motivo) = conexao.aceitar_maquina(
            chave.mod_id.clone(),
            chave.servidor.clone(),
            pedido.resumo.clone(),
        ) {
            tracing::warn!(mod_id = %chave.mod_id, servidor = %chave.servidor, motivo, "o aceite da metade religada não saiu");
        }
        viva.reconectar(fora);
        emitir_estado(app, &chave, true, None);
        return;
    }

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
            let (artefatos_daqui, de_outros_sistemas) =
                seele_ffi::programa::artefatos_desta_maquina(&pedido.oferta);
            // A oferta já passou na régua do fio ao chegar; se não voltar ao
            // núcleo aqui, a tela mostra o par de cima, e o log diz.
            let (rodar_daqui, instalar_daqui, sistemas_com_outros_comandos) =
                seele_ffi::programa::comandos_desta_maquina(&pedido.oferta).unwrap_or_else(
                    |motivo| {
                        tracing::warn!(mod_id = %chave.mod_id, servidor = %chave.servidor, motivo, "a oferta não voltou ao núcleo; a tela mostra os comandos de cima");
                        (pedido.oferta.rodar.clone(), pedido.oferta.instalar.clone(), 0)
                    },
                );
            let na_tela = PedidoNaTela {
                alvo: chave.alvo.clone(),
                mod_id: chave.mod_id.clone(),
                mod_nome: chave.mod_id.clone(),
                servidor: chave.servidor.clone(),
                servidor_seele: conexao.snapshot().server,
                pedido_por: pedido.pedido_por.clone(),
                oferta: pedido.oferta.clone(),
                papel: pedido.papel.clone(),
                resumo: pedido.resumo.clone(),
                pasta,
                mudou,
                artefatos_daqui,
                de_outros_sistemas,
                rodar_daqui,
                instalar_daqui,
                sistemas_com_outros_comandos,
            };
            let mostrou = estado.maquinas.guardar_pendente_se_vale(pedido, || {
                if let Err(erro) = app.emit(EVENTO_DO_PEDIDO, na_tela) {
                    tracing::warn!(mod_id = %chave.mod_id, servidor = %chave.servidor, %erro, "maquina-pedido não chegou à janela");
                }
            });
            if !mostrou {
                tracing::info!(mod_id = %chave.mod_id, servidor = %chave.servidor, "pedido de máquina passado por um mais novo ou dispensado enquanto o disco era lido: sem diálogo");
            }
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
    let geracao_agora = conexao.carimbo().geracao;
    let fim_da_anterior =
        estado
            .maquinas
            .estado()
            .ligadas
            .get(chave)
            .map_or(Fim::OutroPedido, |l| {
                if l.sessao == pedido.sessao && l.geracao == geracao_agora {
                    Fim::OutroPedido
                } else {
                    Fim::ServidorSaiu
                }
            });
    let anteriores = estado.maquinas.tirar_ligadas(|c, _| c == chave);
    for anterior in anteriores {
        anterior.encerrar(fim_da_anterior);
    }

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
    let aceitar_no_fio = || {
        conexao
            .aceitar_maquina(mod_id.clone(), servidor.clone(), pedido.resumo.clone())
            .map_err(|motivo| {
                tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo, "o AceiteDeMaquina não saiu");
                motivo.to_owned()
            })
    };
    // **O `programa` sai do aceite que está no disco agora** — o que acabou
    // de ser guardado, ou o que fez o pedido se aceitar sozinho —, e só se ele
    // é deste resumo e deste pacote.
    let guardado =
        seele_ffi::mods::aceite_de_maquina(&home, &chave.alvo, mod_id, servidor).unwrap_or_else(|motivo| {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo = %motivo, "o aceite guardado não foi lido; a metade sobe sem programa");
            None
        });
    let programa = programa::programa_para(
        &metade.capacidades,
        &pedido.resumo,
        &pedido.hash,
        guardado.as_ref(),
        &home,
    );
    match &programa {
        Ok(p) => {
            tracing::info!(mod_id = %mod_id, servidor = %servidor, raiz = %p.raiz().display(), "programa da metade de máquina montado do aceite");
            // **Cada processo deixa um registro fora da raiz** (e2e de 31/10,
            // passo 14): um app que morre sem desligar o jogo o deixa órfão, e
            // o app reaberto o acha por este registro ([`procurar_sobras`]).
            p.anotar_processos_em(
                pasta_dos_processos(&home),
                etiqueta_da_sobra(chave, &pedido.oferta.titulo),
            );
        }
        Err(SemPrograma::SemOPoder) => {}
        Err(sem) => {
            tracing::warn!(mod_id = %mod_id, servidor = %servidor, motivo = sem.nome(), "a metade de máquina sobe sem programa: todo baixar e rodar será recusado")
        }
    }
    let viva = match Viva::subir(
        chave.clone(),
        numero,
        pedido.oferta.clone(),
        metade.capacidades,
        programa,
        Arc::new(FioDaConexao(Arc::clone(conexao))),
        ao_falhar,
    ) {
        Ok(viva) => viva,
        Err(motivo) => {
            // O aceite sai e a parada logo atrás: é o par que diz ao MOD «a
            // máquina aceitou e falhou», o mesmo que ele ouvia quando a
            // metade subia depois do aceite.
            tracing::error!(mod_id = %mod_id, servidor = %servidor, motivo, "metade de máquina não subiu");
            aceitar_no_fio()?;
            if let Err(erro) =
                conexao.parar_maquina(mod_id.clone(), servidor.clone(), "metade-falhou")
            {
                tracing::info!(mod_id = %mod_id, servidor = %servidor, motivo = erro, "MaquinaParada não saiu");
            }
            emitir_estado(app, chave, false, Some(Fim::MetadeFalhou.nome()));
            return Err(motivo.to_owned());
        }
    };
    // **A faixa diz se o jogo roda**, e não só que a metade vive (e2e de
    // 31/10, passo 13): cada processo que começa ou acaba é dito à tela.
    let app_do_jogo = app.clone();
    viva.ao_mudar_o_jogo(Box::new(move |chave, rodando| {
        emitir_o_jogo(&app_do_jogo, chave, rodando);
    }));
    // **No registro antes de o aceite sair** (e com a geração de agora, a da
    // conexão que o leva): o `sem-aceite` que volta de um aceite que o livro
    // não tomou pode chegar antes da linha seguinte, e precisa achá-la aqui.
    let geracao = conexao.carimbo().geracao;
    let registrada = estado.maquinas.registrar_se_vale(
        chave,
        pedido.numero,
        Ligada {
            viva: Arc::clone(&viva),
            sessao: pedido.sessao,
            geracao,
            hash: pedido.hash.clone(),
            resumo: pedido.resumo.clone(),
            orfa: None,
        },
    );
    if !registrada {
        // O servidor dispensou este pedido enquanto ele era atendido: nada
        // saiu, e a dispensa já disse à tela o que acabou.
        tracing::info!(mod_id = %mod_id, servidor = %servidor, "pedido de máquina dispensado antes de ligar: a metade não sobe");
        viva.descartar();
        return Err("sem-pedido".to_owned());
    }
    if let Err(motivo) = aceitar_no_fio() {
        estado
            .maquinas
            .tirar_ligadas(|c, l| c == chave && l.viva.numero == numero);
        viva.descartar();
        return Err(motivo);
    }
    // Uma dispensa pode já ter tirado a metade do registro e pedido o fim
    // dela: o código não entra numa metade que está parando.
    let iniciada = if viva.de_pe() {
        viva.iniciar(&metade.fonte)
    } else {
        Ok(())
    };
    if let Err(motivo) = iniciada {
        let ainda_minha = !estado
            .maquinas
            .tirar_ligadas(|c, l| c == chave && l.viva.numero == numero)
            .is_empty();
        // `encerrar` já manda o `MaquinaParada { MetadeFalhou }`.
        tracing::error!(mod_id = %mod_id, servidor = %servidor, motivo, "metade de máquina não subiu");
        if ainda_minha && viva.encerrar(Fim::MetadeFalhou) {
            emitir_estado(app, chave, false, Some(Fim::MetadeFalhou.nome()));
        }
        return Err(motivo.to_owned());
    }
    // **«Ligada» só se ela ainda está no registro**, e dito sob o cadeado: a
    // dispensa que a tira toma o mesmo cadeado, então o «desligada» dela vem
    // sempre depois deste — nunca a faixa acesa por cima de um fim já dito.
    let maquinas = estado.maquinas.estado();
    if maquinas
        .ligadas
        .get(chave)
        .is_some_and(|l| l.viva.numero == numero)
    {
        tracing::info!(mod_id = %mod_id, servidor = %servidor, instancia = numero, "metade de máquina ligada");
        emitir_estado(app, chave, true, None);
    } else {
        tracing::info!(mod_id = %mod_id, servidor = %servidor, instancia = numero, "metade de máquina dispensada antes de acabar de ligar");
    }
    drop(maquinas);
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
    // O aceite pela tela também espera o jogo que sobrou (revisão da 0.19, I1).
    esperar_as_sobras(&estado.maquinas, &chave);
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
        papel: pedido.papel.clone(),
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
    // A mesma vez de `aceitar` e de `processar_pedido`: sem ela, um aceite em
    // curso com uma cópia do pendente e esta recusa saíam os dois.
    let _vez = estado
        .maquinas
        .vez
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
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
    // **A mesma vez de `processar_pedido`**, que segura «ler o aceite → ligar»
    // (até 2 s). Sem ela, um REVOGAR no meio de um aceite sozinho depois de uma
    // reconexão não achava nada ligado, apagava o aceite, dizia «feito» — e o
    // `ligar` em curso punha a metade de pé sem aceite guardado.
    let _vez = estado
        .maquinas
        .vez
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
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
                jogo: estado
                    .as_ref()
                    .is_some_and(|e| e.maquinas.jogo_rodando(&chave)),
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

// --------------------------------------------- o jogo que sobrou

/// Onde cada processo de jogo deixa o registro dele: na casa do app, fora da
/// raiz de qualquer jogo (o MOD escreve na raiz).
pub(crate) fn pasta_dos_processos(home: &str) -> std::path::PathBuf {
    std::path::Path::new(home).join("processos-de-jogo")
}

/// A etiqueta que o registro leva: o que a faixa precisa para dizer de que
/// jogo se trata, sem o MOD rodando.
fn etiqueta_da_sobra(chave: &Chave, titulo: &str) -> String {
    serde_json::json!({
        "alvo": chave.alvo,
        "mod_id": chave.mod_id,
        "servidor": chave.servidor,
        "titulo": titulo,
    })
    .to_string()
}

/// `maquina-sobra`: um jogo de uma execução anterior que o app achou rodando
/// e parou, como a casca o lê.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct SobraNaTela {
    pub(crate) alvo: String,
    pub(crate) mod_id: String,
    pub(crate) servidor: String,
    pub(crate) titulo: String,
    pub(crate) pid: u32,
    /// `parando` (o pedido de salvar e sair acabou de sair, e ele ainda roda),
    /// `saiu` (com o pedido de salvar e sair), `forcada` (não saiu no prazo
    /// e foi morto) ou `nao-parou`.
    pub(crate) desfecho: &'static str,
    /// Quanto se esperou, em segundos.
    pub(crate) segundos: f64,
    /// O que o sistema disse, quando não parou.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) motivo: Option<String>,
}

/// `maquina-sobra`.
pub(crate) const EVENTO_DA_SOBRA: &str = "maquina-sobra";

/// O que a tela recebe de uma sobra encontrada.
pub(crate) fn sobra_na_tela(achada: &seele_ffi::programa::SobraEncontrada) -> SobraNaTela {
    use seele_ffi::programa::DesfechoDaSobra;
    let (desfecho, segundos, motivo) = match &achada.desfecho {
        DesfechoDaSobra::SaiuAoPedido { levou } => ("saiu", levou.as_secs_f64(), None),
        DesfechoDaSobra::Forcada { esperou } => ("forcada", esperou.as_secs_f64(), None),
        DesfechoDaSobra::NaoParou { motivo } => ("nao-parou", 0.0, Some(motivo.clone())),
    };
    sobra_com(&achada.registro, desfecho, segundos, motivo)
}

/// O que a tela recebe de uma sobra que acabou de receber o pedido de salvar
/// e sair (`parando`): a faixa diz que ela ainda roda, e que um jogo novo
/// espera ela sair (revisão da 0.19, I1).
pub(crate) fn sobra_parando_na_tela(
    registro: &seele_ffi::programa::RegistroDeProcesso,
) -> SobraNaTela {
    sobra_com(registro, "parando", 0.0, None)
}

fn sobra_com(
    registro: &seele_ffi::programa::RegistroDeProcesso,
    desfecho: &'static str,
    segundos: f64,
    motivo: Option<String>,
) -> SobraNaTela {
    let etiqueta: serde_json::Value = serde_json::from_str(&registro.etiqueta).unwrap_or_default();
    let campo = |nome: &str| {
        etiqueta
            .get(nome)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    SobraNaTela {
        alvo: campo("alvo"),
        mod_id: campo("mod_id"),
        servidor: campo("servidor"),
        titulo: campo("titulo"),
        pid: registro.pid,
        desfecho,
        segundos,
        motivo,
    }
}

/// **A porta que segura as metades enquanto um jogo que sobrou para**
/// (revisão da 0.19, I1).
///
/// O app reaberto manda `SIGTERM` ao `java` órfão, e ele leva até
/// [`seele_ffi::programa::prazo_das_sobras`] salvando o mundo. Nesse meio, o
/// `MetadeSubiu` de quem acabou de hospedar fazia o MOD chamar esta máquina de
/// volta, o aceite guardado ligava a metade sozinho, e um segundo `java`
/// subia na mesma raiz — com o `session.lock` e a porta ainda do primeiro.
///
/// **Toda metade espera, e não só a da mesma chave**: a porta do jogo é da
/// máquina inteira, e o mesmo servidor alcançado por outro endereço é outra
/// chave com a mesma porta. A espera só existe na abertura, e acaba quando a
/// última sobra sai.
///
/// A procura é marcada **antes** de a thread começar ([`Self::abrir`]): um
/// pedido que chega antes de a pasta ser lida espera também. Depois que ela
/// fecha, nada a reabre — passar por ela uma vez vale para sempre.
#[derive(Default)]
pub(crate) struct PortaDasSobras {
    estado: Mutex<SobrasEmCurso>,
    mudou: std::sync::Condvar,
}

#[derive(Default)]
struct SobrasEmCurso {
    /// A pasta ainda está sendo lida.
    procurando: bool,
    /// Sobras achadas e ainda não paradas.
    parando: usize,
}

impl SobrasEmCurso {
    fn livre(&self) -> bool {
        !self.procurando && self.parando == 0
    }
}

/// O que a procura diz pelo caminho, para a tela.
#[derive(Debug)]
pub(crate) enum AvisoDaSobra<'a> {
    /// Achada, e o pedido de salvar e sair vai sair agora.
    Parando(&'a seele_ffi::programa::RegistroDeProcesso),
    /// Parou (ou não), com o desfecho.
    Parou(&'a seele_ffi::programa::SobraEncontrada),
}

impl PortaDasSobras {
    fn estado(&self) -> std::sync::MutexGuard<'_, SobrasEmCurso> {
        self.estado.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A procura vai começar: quem pedir uma metade daqui em diante espera.
    pub(crate) fn abrir(&self) {
        self.estado().procurando = true;
    }

    /// A procura não vai acontecer (sem thread): a porta abre.
    fn desistir(&self) {
        *self.estado() = SobrasEmCurso::default();
        self.mudou.notify_all();
    }

    /// **Acha e para as sobras de `pasta`**, abrindo a porta quando a última
    /// sai. Bloqueia: numa thread. Uma queda no meio abre a porta também — uma
    /// porta presa seria nenhuma máquina subir até o app fechar.
    pub(crate) fn procurar(
        &self,
        pasta: &std::path::Path,
        prazo: std::time::Duration,
        mut avisar: impl FnMut(AvisoDaSobra<'_>),
    ) {
        struct AbreNoFim<'a>(&'a PortaDasSobras);
        impl Drop for AbreNoFim<'_> {
            fn drop(&mut self) {
                self.0.desistir();
            }
        }
        let _abre = AbreNoFim(self);
        let achadas = seele_ffi::programa::achar_sobras(pasta);
        {
            let mut estado = self.estado();
            estado.procurando = false;
            estado.parando = achadas.len();
        }
        self.mudou.notify_all();
        for sobra in achadas {
            avisar(AvisoDaSobra::Parando(&sobra.registro));
            let parada = seele_ffi::programa::parar_sobra(sobra, prazo);
            {
                let mut estado = self.estado();
                estado.parando = estado.parando.saturating_sub(1);
            }
            self.mudou.notify_all();
            avisar(AvisoDaSobra::Parou(&parada));
        }
    }

    /// Espera a porta abrir. Devolve quanto esperou, se esperou.
    pub(crate) fn esperar(&self) -> Option<std::time::Duration> {
        let mut estado = self.estado();
        if estado.livre() {
            return None;
        }
        let comeco = std::time::Instant::now();
        while !estado.livre() {
            estado = self
                .mudou
                .wait(estado)
                .unwrap_or_else(PoisonError::into_inner);
        }
        Some(comeco.elapsed())
    }

    /// A porta está fechada agora?
    fn fechada(&self) -> bool {
        !self.estado().livre()
    }
}

/// **Antes de uma metade subir, o jogo que sobrou sai** (revisão da 0.19,
/// I1): espera a [`PortaDasSobras`], e o log diz quanto e por quê. Chamada sem
/// a `vez` na mão: a espera é de até ~22 s por sobra.
fn esperar_as_sobras(maquinas: &Maquinas, chave: &Chave) {
    if maquinas.porta_das_sobras.fechada() {
        tracing::warn!(mod_id = %chave.mod_id, servidor = %chave.servidor, "pedido de máquina enquanto um jogo de uma execução anterior ainda salva e sai: a metade espera ele sair para não subir um segundo jogo por cima do mundo e da porta dele");
    }
    if let Some(esperou) = maquinas.porta_das_sobras.esperar() {
        tracing::info!(mod_id = %chave.mod_id, servidor = %chave.servidor, esperou = ?esperou, "o jogo que sobrou saiu: o pedido de máquina segue");
    }
}

/// **Acha e para o jogo que um app anterior deixou rodando** (e2e de 31/10,
/// passo 14), numa thread: o pedido de salvar e sair, o prazo, e então à
/// força ([`seele_ffi::programa::parar_sobra`]). Cada um vai à faixa
/// (`maquina-sobra`) duas vezes — `parando`, e então o desfecho — e fica
/// guardado para a janela que relê ([`sobras`]). Enquanto houver sobra
/// parando, nenhuma metade sobe ([`PortaDasSobras`]).
pub(crate) fn procurar_sobras<R: tauri::Runtime>(app: AppHandle<R>) {
    let pasta = pasta_dos_processos(&crate::config_dir(&app));
    // Fechada aqui, na thread do `setup`, e não na da procura: um pedido que
    // chegue antes de a thread ler a pasta já espera.
    if let Some(estado) = sessoes(&app) {
        estado.maquinas.porta_das_sobras.abrir();
    }
    let da_thread = app.clone();
    let feito = std::thread::Builder::new()
        .name("jogos que sobraram".into())
        .spawn(move || {
            let app = da_thread;
            let Some(estado) = sessoes(&app) else {
                return;
            };
            let dizer = |na_tela: SobraNaTela| {
                {
                    let mut guardadas = estado
                        .maquinas
                        .sobras
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    // O desfecho toma o lugar do «parando» do mesmo processo.
                    guardadas.retain(|s| s.pid != na_tela.pid);
                    guardadas.push(na_tela.clone());
                }
                if let Err(erro) = app.emit(EVENTO_DA_SOBRA, na_tela) {
                    tracing::warn!(%erro, "maquina-sobra não chegou à janela");
                }
            };
            estado.maquinas.porta_das_sobras.procurar(
                &pasta,
                seele_ffi::programa::prazo_das_sobras(),
                |aviso| match aviso {
                    AvisoDaSobra::Parando(registro) => {
                        dizer(sobra_parando_na_tela(registro));
                    }
                    AvisoDaSobra::Parou(achada) => {
                        let na_tela = sobra_na_tela(achada);
                        tracing::warn!(pid = na_tela.pid, mod_id = %na_tela.mod_id, servidor = %na_tela.servidor, desfecho = na_tela.desfecho, segundos = na_tela.segundos, "um jogo de uma execução anterior do app rodava sem ninguém; a faixa diz à pessoa");
                        dizer(na_tela);
                    }
                },
            );
        });
    if let Err(erro) = feito {
        tracing::error!(%erro, "a procura de jogos que sobraram não conseguiu thread: um jogo órfão segue rodando");
        if let Some(estado) = sessoes(&app) {
            estado.maquinas.porta_das_sobras.desistir();
        }
    }
}

/// Os jogos que sobraram, achados e parados ao abrir.
pub(crate) fn sobras<R: tauri::Runtime>(app: &AppHandle<R>) -> Vec<SobraNaTela> {
    sessoes(app).map_or_else(Vec::new, |estado| {
        estado
            .maquinas
            .sobras
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    })
}

#[cfg(test)]
mod testes {
    /// Quanto um teste espera por algo que depende de um processo de verdade
    /// subir (a metade roda este mesmo executável de teste como filho).
    ///
    /// Eram 5 s (e 20 s num deles), e a bateria inteira os estourava: o filho
    /// é um executável de centenas de megabytes, e o macOS o confere antes de
    /// deixá-lo rodar. Medido em 11/11/2026, em 150 rodadas, quanto o filho
    /// leva para dar o primeiro sinal: mediana de 0,8 s e máximo de 1,3 s com
    /// um hard link deste executável; mediana de 1,4 s e máximo de 2,3 s com
    /// um arquivo novo a cada rodada, que é o que
    /// [`por_este_executavel_em`] faz no macOS — e isso com outras baterias
    /// rodando na mesma máquina, não com ela parada. O teste volta assim que
    /// a coisa acontece; o prazo só é gasto quando ela não acontece.
    ///
    /// **Este prazo não era o defeito**: os testes falhavam em paralelo com
    /// qualquer prazo, porque o filho morria ([`por_este_executavel_em`]).
    const ESPERA_DOS_TESTES: Duration = Duration::from_secs(60);

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
            instalar: None,
            por_sistema: vec![],
        }
    }

    fn pedido(oferta: OfertaDeMaquinaFfi, resumo: &str, hash: &str) -> PedidoRecebido {
        PedidoRecebido {
            chave: Chave::de("casa", "x/jogo", "mundo"),
            hash: hash.to_owned(),
            oferta,
            papel: "principal".to_owned(),
            resumo: resumo.to_owned(),
            pedido_por: "ana".to_owned(),
            sessao: 1,
            numero: 0,
        }
    }

    fn guardado(oferta: OfertaDeMaquinaFfi, resumo: &str, hash: &str) -> AceiteDeMaquinaGuardado {
        AceiteDeMaquinaGuardado {
            alvo: "casa:8383".to_owned(),
            mod_id: "x/jogo".to_owned(),
            hash: hash.to_owned(),
            servidor: "mundo".to_owned(),
            papel: "principal".to_owned(),
            resumo: resumo.to_owned(),
            oferta,
            pedido_por: "ana".to_owned(),
            quando: 1,
        }
    }

    #[test]
    fn so_o_pedido_da_sessao_e_da_chave_dispensada_e_anunciado() {
        let maquinas = Maquinas::default();
        let mut daqui = pedido(oferta(), "aaaa", "h1");
        let mut de_outra_sessao = pedido(oferta(), "aaaa", "h1");
        de_outra_sessao.chave = Chave::de("casa", "x/jogo", "outro");
        de_outra_sessao.sessao = 2;
        daqui.sessao = 1;
        {
            let mut estado = maquinas.estado();
            estado.pendentes.insert(daqui.chave.clone(), daqui.clone());
            estado
                .pendentes
                .insert(de_outra_sessao.chave.clone(), de_outra_sessao.clone());
        }
        let chave = daqui.chave.clone();
        // A dispensa nomeia uma chave e uma sessão: só ela sai e é anunciada.
        let desfeitos = maquinas.tirar_pendentes(|c, p| *c == chave && p.sessao == 1);
        // O resumo do pedido vai junto: é ele que diz à tela qual pedido acabou.
        assert_eq!(desfeitos, vec![(chave.clone(), "aaaa".to_owned())]);
        assert!(!maquinas.estado().pendentes.contains_key(&chave));
        assert!(maquinas
            .estado()
            .pendentes
            .contains_key(&de_outra_sessao.chave));
        // Sem pedido na chave, não há o que anunciar: a tela não tem diálogo.
        assert!(maquinas
            .tirar_pendentes(|c, p| *c == chave && p.sessao == 1)
            .is_empty());
        // O fim da sessão leva os pedidos dela, todos, e diz cada um.
        let desfeitos = maquinas.tirar_pendentes(|_, p| p.sessao == 2);
        assert_eq!(
            desfeitos,
            vec![(de_outra_sessao.chave.clone(), "aaaa".to_owned())]
        );
        // O motivo que vai à tela é o nome da dispensa, ou `servidor-saiu`.
        assert_eq!(
            Fim::da_dispensa("outro-pedido").map(Fim::nome),
            Some("outro-pedido")
        );
        assert_eq!(Fim::ServidorSaiu.nome(), "servidor-saiu");
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

    /// **O aceite de principal não liga sozinho o pedido de reserva**, e a
    /// tela diz que foi o papel que mudou. O resumo já difere (o papel entra
    /// nele); o `mudou` é o que a pessoa lê.
    #[test]
    fn outro_papel_pede_tela_e_diz_que_o_papel_mudou() {
        let mut como_reserva = pedido(oferta(), "bbbb", "h1");
        como_reserva.papel = "reserva".to_owned();
        assert_eq!(
            decidir(&como_reserva, Some(&guardado(oferta(), "aaaa", "h1"))),
            Decisao::Mostrar {
                mudou: Some(vec!["papel"])
            },
            "a tela não diz que o pedido de agora é o da reserva"
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
            plataformas: vec![],
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

    /// **A cópia do mundo entra só pelo túnel da própria oferta** (API 8
    /// parte 7): `receberCopia` num túnel que o dono não aceitou nem chega ao
    /// fio; os três tipos da cópia são do poder `programa`, e sem ele não
    /// existem.
    #[test]
    fn a_copia_so_entra_pelo_tunel_tcp_da_oferta() {
        let mut oferta = oferta();
        oferta.tuneis.push(tunel("mc-copia", "tcp", 35565));
        oferta.tuneis.push(tunel("mc-voz", "udp", 24454));
        let com = ["maquina", "maquina.porta", "maquina.programa"];
        let receber = |tunel: &str| {
            format!(
                r#"{{"tipo":"programa-receber-copia","n":7,"tunel":"{tunel}","codigo":"k","sha256":"{}","tamanho":9,"destino":"copias/r.tar.gz"}}"#,
                "c".repeat(64)
            )
        };
        assert!(matches!(
            tratar(&oferta, &com, &receber("mc-copia")),
            Acao::Programa { n: 7, .. }
        ));
        for fora in ["outro", "mc-voz"] {
            assert_eq!(
                tratar(&oferta, &com, &receber(fora)),
                Acao::Recusar {
                    n: 7,
                    erro: "fora-do-aceite",
                    tipo: "programa-receber-copia".to_owned()
                },
                "receberCopia entrou no túnel {fora}"
            );
        }
        for tipo in ["programa-empacotar", "programa-oferecer-copia"] {
            let json = format!(
                r#"{{"tipo":"{tipo}","n":8,"origem":"world","arquivo":"a.tar.gz","porta":35565}}"#
            );
            assert!(
                matches!(tratar(&oferta, &com, &json), Acao::Programa { n: 8, .. }),
                "{tipo}"
            );
            assert!(
                matches!(
                    tratar(&oferta, &["maquina", "maquina.porta"], &json),
                    Acao::Recusar {
                        erro: "tipo-desconhecido",
                        ..
                    }
                ),
                "{tipo} sem o poder programa"
            );
        }
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
        /// Quanto o `servir` demora no fio.
        atraso_do_servir: Duration,
        /// O que o volume da metade de servidor devolve (`None`: recusa).
        volume: Mutex<Option<Vec<u8>>>,
        /// O que a metade de servidor responde a `maquina.pedir` (`None`: `"ok"`).
        resposta: Mutex<Option<String>>,
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
            let fim = Instant::now() + ESPERA_DOS_TESTES;
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
            let resposta = self
                .resposta
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "\"ok\"".to_owned());
            Box::pin(async move { Ok(resposta) })
        }
        fn servir(
            &self,
            _: &str,
            nome: &str,
            protocolo: &str,
            porta: u16,
        ) -> Futuro<Result<(), &'static str>> {
            self.anotar(format!("servir {nome} {protocolo} {porta}"));
            let atraso = self.atraso_do_servir;
            Box::pin(async move {
                // Dormir na thread do runtime basta num teste: é o servir que
                // ainda está no fio quando o fim é pedido.
                std::thread::sleep(atraso);
                Ok(())
            })
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
        fn ler_volume(&self, _: &str, payload: String) -> Futuro<Result<Vec<u8>, String>> {
            self.anotar(format!("ler-volume {payload}"));
            let bytes = self.volume.lock().unwrap().clone();
            Box::pin(async move { bytes.ok_or_else(|| "leitura recusada pelo MOD".to_owned()) })
        }
        fn entrar_tunel(&self, _: &str, nome: &str) -> Futuro<Result<u16, &'static str>> {
            self.anotar(format!("entrar-tunel {nome}"));
            Box::pin(async { Err("sem-resposta") })
        }
        fn sair_tunel(&self, _: &str, nome: &str) {
            self.anotar(format!("sair-tunel {nome}"));
        }
        fn direto(&self, _: &str, _: &str) -> Option<bool> {
            None
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
            Err(SemPrograma::SemOPoder),
            Arc::clone(fio) as Arc<dyn Fio>,
            Box::new(move |_, texto| falhas.lock().unwrap().push(texto)),
        )
        .expect("a metade sobe");
        viva.iniciar(fonte).expect("o código entra");
        viva
    }

    // ------------------------------------------- o registro, na ordem do fio

    /// Uma metade de pé no registro, de `sessao` e na `geracao` do enlace.
    fn ligada_de(sessao: u64, geracao: u64) -> Ligada {
        let fio = Arc::new(FioDeMentira::default());
        Ligada {
            viva: subir_com(&fio, "", Arc::default()),
            sessao,
            geracao,
            hash: "h1".to_owned(),
            resumo: "aaaa".to_owned(),
            orfa: None,
        }
    }

    fn com_pedido_e_metade(chave: &Chave) -> Maquinas {
        let maquinas = Maquinas::default();
        {
            let mut estado = maquinas.estado();
            let mut pendente = pedido(oferta(), "bbbb", "h1");
            pendente.chave = chave.clone();
            estado.pendentes.insert(chave.clone(), pendente);
            estado.ligadas.insert(chave.clone(), ligada_de(1, 1));
        }
        maquinas
    }

    fn parar_todas(vivas: Vec<Arc<Viva>>) {
        for viva in vivas {
            viva.descartar();
        }
    }

    /// **`sem-aceite` para a metade de pé e não fecha o diálogo.** Ele é a
    /// resposta a um aceite que o livro não tomou (ou a um pedido de quem não
    /// é a máquina) e nunca fala de um pedido pendente: o diálogo aberto da
    /// mesma chave é outra pergunta, que ainda vale.
    #[test]
    fn sem_aceite_para_a_metade_e_nao_fecha_o_pedido() {
        let chave = Chave::de("casa", "x/jogo", "mundo");
        let maquinas = com_pedido_e_metade(&chave);
        let dispensa = maquinas.dispensar(&chave, 1, "sem-aceite");
        assert_eq!(dispensa.fim, Fim::SemAceite);
        assert!(
            dispensa.desfeitos.is_empty(),
            "sem-aceite fechou um pedido pendente"
        );
        assert_eq!(
            dispensa.vivas.len(),
            1,
            "sem-aceite não parou a metade de pé"
        );
        assert!(maquinas.estado().pendentes.contains_key(&chave));
        assert!(!maquinas.ligada(&chave));
        parar_todas(dispensa.vivas);

        // Os outros motivos levam os dois.
        let maquinas = com_pedido_e_metade(&chave);
        let dispensa = maquinas.dispensar(&chave, 1, "outro-pedido");
        assert_eq!(dispensa.desfeitos, vec![(chave.clone(), "bbbb".to_owned())]);
        assert_eq!(dispensa.vivas.len(), 1);
        parar_todas(dispensa.vivas);
    }

    /// **Um motivo de dispensa que este app não conhece para a metade.** Antes
    /// ele era ignorado e a metade seguia servindo: o servidor dizia «pare»
    /// numa palavra nova e o app não ouvia.
    #[test]
    fn uma_dispensa_de_motivo_desconhecido_para_a_metade() {
        let chave = Chave::de("casa", "x/jogo", "mundo");
        let maquinas = com_pedido_e_metade(&chave);
        let dispensa = maquinas.dispensar(&chave, 1, "um-motivo-do-futuro");
        assert_eq!(dispensa.fim, Fim::DispensaDesconhecida);
        assert_eq!(
            dispensa.vivas.len(),
            1,
            "o motivo desconhecido deixou a metade de pé"
        );
        assert!(dispensa.desfeitos.is_empty());
        assert_eq!(
            dispensa.fim.parada_no_fio(),
            None,
            "quem decidiu foi o servidor"
        );
        parar_todas(dispensa.vivas);
    }

    /// **O pedido mais velho não escreve por cima do diálogo do mais novo.**
    /// Cada chegada é atendida numa thread, e a ordem em que elas chegam ao
    /// registro não é a do fio: o número dado na chegada é o que decide.
    #[test]
    fn o_pedido_mais_velho_nao_escreve_por_cima_do_mais_novo() {
        let maquinas = Maquinas::default();
        let chave = Chave::de("casa", "x/jogo", "mundo");
        let mut r1 = pedido(oferta(), "r1", "h1");
        let mut r2 = pedido(oferta(), "r2", "h1");
        r1.numero = maquinas.estado().numerar(&chave, 1);
        r2.numero = maquinas.estado().numerar(&chave, 1);

        // A thread do R2 chega primeiro; a do R1, atrasada no disco, depois.
        let mut mostrados = Vec::new();
        assert!(maquinas.guardar_pendente_se_vale(r2, || mostrados.push("r2")));
        assert!(!maquinas.guardar_pendente_se_vale(r1, || mostrados.push("r1")));
        assert_eq!(mostrados, vec!["r2"], "o pedido velho abriu diálogo");
        assert_eq!(
            maquinas
                .estado()
                .pendentes
                .get(&chave)
                .map(|p| p.resumo.clone()),
            Some("r2".to_owned())
        );
    }

    /// **A dispensa que chega depois do pedido vale contra ele**, mesmo que a
    /// thread do pedido só chegue ao registro depois: nem diálogo, nem metade.
    /// A dispensa de outra sessão não corta este.
    #[test]
    fn a_dispensa_que_chega_depois_do_pedido_vale_contra_ele() {
        let maquinas = Maquinas::default();
        let chave = Chave::de("casa", "x/jogo", "mundo");
        let mut r = pedido(oferta(), "r", "h1");
        r.numero = maquinas.estado().numerar(&chave, 1);

        let de_outra = maquinas.dispensar(&chave, 2, "mod-desligado");
        assert!(de_outra.desfeitos.is_empty() && de_outra.vivas.is_empty());
        assert!(maquinas.estado().e_o_ultimo(&chave, 1, r.numero));

        let dispensa = maquinas.dispensar(&chave, 1, "mod-desligado");
        assert!(dispensa.desfeitos.is_empty(), "ainda não havia pendente");
        let mut mostrou = false;
        assert!(!maquinas.guardar_pendente_se_vale(r.clone(), || mostrou = true));
        assert!(!mostrou, "o diálogo abriu para um pedido já dispensado");
        let ligada = ligada_de(1, 1);
        let viva = Arc::clone(&ligada.viva);
        assert!(
            !maquinas.registrar_se_vale(&chave, r.numero, ligada),
            "a metade de um pedido dispensado entrou no registro"
        );
        assert!(!maquinas.ligada(&chave));
        viva.descartar();

        // Um pedido que chega depois da dispensa vale normalmente.
        let mut novo = pedido(oferta(), "novo", "h1");
        novo.numero = maquinas.estado().numerar(&chave, 1);
        assert!(maquinas.guardar_pendente_se_vale(novo, || ()));

        // `sem-aceite` não corta pedido nenhum.
        let mut outro = pedido(oferta(), "outro", "h1");
        outro.numero = maquinas.estado().numerar(&chave, 1);
        let _ = maquinas.dispensar(&chave, 1, "sem-aceite");
        assert!(maquinas.guardar_pendente_se_vale(outro, || ()));
    }

    /// **A reconexão do enlace deixa as metades de antes dela órfãs, e não as
    /// para** (M18 revista: sem quem hospeda, a dona segue rodando). Só as
    /// desta sessão e de antes da geração nova ficam órfãs; nenhuma sai do
    /// registro, e a que já era órfã não ganha outro prazo.
    #[test]
    fn a_reconexao_deixa_as_metades_de_antes_orfas() {
        let maquinas = Maquinas::default();
        let velha = Chave::de("casa", "x/jogo", "velha");
        let nova = Chave::de("casa", "x/jogo", "nova");
        let de_outra = Chave::de("casa", "x/jogo", "outra");
        {
            let mut estado = maquinas.estado();
            estado.ligadas.insert(velha.clone(), ligada_de(1, 1));
            estado.ligadas.insert(nova.clone(), ligada_de(1, 2));
            estado.ligadas.insert(de_outra.clone(), ligada_de(2, 1));
        }
        assert!(
            maquinas.orfanar_de_geracao_velha(1, 1).is_empty(),
            "a mesma geração não é queda"
        );
        let orfas = maquinas.orfanar_de_geracao_velha(1, 2);
        assert_eq!(
            orfas.len(),
            1,
            "a queda não deixou órfã só a metade de antes"
        );
        assert!(orfas[0].0.de_pe(), "a órfã foi parada");
        assert!(maquinas.ligada(&velha), "a órfã saiu do registro");
        assert!(maquinas.estado().ligadas[&velha].orfa.is_some());
        assert!(maquinas.estado().ligadas[&nova].orfa.is_none());
        assert!(maquinas.estado().ligadas[&de_outra].orfa.is_none());
        let de_novo = maquinas.orfanar_de_geracao_velha(1, 3);
        assert_eq!(de_novo.len(), 1, "só a da geração 2 fica órfã agora");
        assert!(
            !de_novo.iter().any(|(v, _)| Arc::ptr_eq(v, &orfas[0].0)),
            "a órfã ganhou outro prazo numa segunda queda"
        );
        parar_todas(maquinas.tirar_ligadas(|_, _| true));
    }

    /// Uma metade com um processo de verdade, no registro como ligada da
    /// sessão 1 na geração 1.
    fn ligada_com_processo(
        casa: &PastaDoTeste,
        filho: &str,
        fonte: &str,
    ) -> (Maquinas, Arc<FioDeMentira>, Arc<Viva>, std::path::PathBuf) {
        let argumentos = ["--ignored", "--exact", filho, "--test-threads", "1"];
        let (programa, oferta, raiz) = programa_que_roda_o_teste(casa, &argumentos);
        let fio = Arc::new(FioDeMentira::default());
        let fonte = fonte.replace("ARGUMENTOS", &serde_json::to_string(&argumentos).unwrap());
        let viva = subir_com_programa(&fio, oferta, programa, &fonte);
        fio.esperar(r#"pedir "rodou""#);
        esperar_o_filho_de_pe(&viva, &raiz);
        let maquinas = Maquinas::default();
        maquinas.estado().ligadas.insert(
            viva.chave.clone(),
            Ligada {
                viva: Arc::clone(&viva),
                sessao: 1,
                geracao: 1,
                hash: "h1".to_owned(),
                resumo: "aaaa".to_owned(),
                orfa: None,
            },
        );
        (maquinas, fio, viva, raiz)
    }

    /// Espera o batimento passar de `antes`, até 5 s.
    fn bate_depois_de(raiz: &std::path::Path, antes: u64) -> bool {
        let fim = Instant::now() + ESPERA_DOS_TESTES;
        while Instant::now() < fim {
            if batimento(raiz).is_some_and(|n| n > antes) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// **A volta de quem hospeda religa a órfã, sem subir de novo** (o caminho
    /// do app de «sem quem hospeda»). A metade roda um processo de verdade; a
    /// conexão cai e ela fica órfã com o processo de pé; o pedido igual ao
    /// aceito a religa — a mesma instância, o mesmo processo, que nunca subiu
    /// de novo (o batimento só cresce, e o código dela rodou uma vez) —, ela
    /// passa a falar pela conexão nova, e o `aoReconectar` dela ouve. Um
    /// pedido com outro resumo não religa.
    #[test]
    fn a_volta_religa_a_orfa_sem_subir_de_novo() {
        let casa = pasta_do_teste("religa");
        let (maquinas, fio, viva, raiz) = ligada_com_processo(
            &casa,
            O_QUE_BATE,
            "maquina.aoReconectar((v) => maquina.pedir('voltei ' + v.fora));             maquina.programa.rodar({ comando: 'bin/servidor', argumentos: ARGUMENTOS, pasta: '' })               .then(() => maquina.pedir('rodou'), (e) => maquina.pedir('recusou ' + e.erro));",
        );
        assert_eq!(viva.processos_vivos(), 1);
        // O `fora` que o `aoReconectar` lê é o tempo de órfã, em segundos
        // inteiros: sob carga, as esperas pelo batimento abaixo passam de 1 s,
        // e a frase certa é «voltei 1» (a suíte inteira, 31/10). A prova lê o
        // número e o confere contra o relógio, em vez de supor que é 0.
        let orfanou = Instant::now();
        let orfas = maquinas.orfanar_de_geracao_velha(1, 2);
        assert_eq!(orfas.len(), 1);
        let antes = batimento(&raiz).unwrap_or(0);
        assert!(bate_depois_de(&raiz, antes), "o jogo parou com a queda");
        assert_eq!(viva.processos_vivos(), 1, "o processo da órfã morreu");

        // Outro resumo é outra oferta: não religa.
        let novo = Arc::new(FioDeMentira::default());
        let mut outro = pedido(oferta(), "bbbb", "h1");
        outro.sessao = 1;
        assert!(maquinas
            .religar_se_orfa(&outro, 2, Arc::clone(&novo) as Arc<dyn Fio>)
            .is_none());
        assert!(maquinas.estado().ligadas[&viva.chave].orfa.is_some());

        // O pedido igual religa.
        let igual = pedido(oferta(), "aaaa", "h1");
        let (religada, fora) = maquinas
            .religar_se_orfa(&igual, 2, Arc::clone(&novo) as Arc<dyn Fio>)
            .expect("o pedido igual não religou a órfã");
        assert!(Arc::ptr_eq(&religada, &viva), "subiu outra metade");
        assert!(maquinas.estado().ligadas[&viva.chave].orfa.is_none());
        assert_eq!(maquinas.estado().ligadas[&viva.chave].geracao, 2);
        religada.reconectar(fora);
        let teto = orfanou.elapsed().as_secs();
        let voltas: Vec<String> = novo
            .esperar(r#"pedir "voltei "#)
            .into_iter()
            .filter(|l| l.starts_with(r#"pedir "voltei "#))
            .collect();
        assert_eq!(
            voltas.len(),
            1,
            "o aoReconectar não falou uma vez só: {voltas:?}"
        );
        let segundos: u64 = voltas[0]
            .trim_start_matches(r#"pedir "voltei "#)
            .trim_end_matches('"')
            .parse()
            .unwrap_or_else(|_| panic!("o fora não é um número: {voltas:?}"));
        assert!(
            segundos <= teto,
            "o aoReconectar leu {segundos} s fora, e a órfã ficou no máximo {teto} s"
        );
        assert_eq!(
            fio.anotado()
                .iter()
                .filter(|l| *l == r#"pedir "rodou""#)
                .count(),
            1,
            "o código da metade rodou de novo"
        );
        assert!(
            !fio.anotado()
                .iter()
                .any(|l| l.starts_with(r#"pedir "voltei"#)),
            "a religada falou pela conexão velha"
        );
        let agora = batimento(&raiz).unwrap_or(0);
        assert!(
            agora >= antes,
            "o batimento recomeçou: o jogo subiu de novo"
        );
        assert!(bate_depois_de(&raiz, agora), "o jogo não segue de pé");
        assert_eq!(viva.processos_vivos(), 1);
        assert_eq!(maquinas.estado().religadas, 1);
        parar_todas(maquinas.tirar_ligadas(|_, _| true));
    }

    /// **A faixa ouve o jogo acabar** (e2e de 31/10, passo 13): com a metade
    /// de pé e o processo rodando, `jogo_rodando` diz que sim; quando o
    /// processo acaba, quem ouve ([`Viva::ao_mudar_o_jogo`]) ouve `false`, e a
    /// lista de aceites deixa de dizer que roda. Sem o aviso no fim do
    /// processo (`vigiar`), a faixa diria «serve» para sempre.
    #[test]
    fn o_jogo_que_acaba_e_dito_a_quem_ouve() {
        let casa = pasta_do_teste("jogo-acaba");
        let (maquinas, _fio, viva, _raiz) = ligada_com_processo(
            &casa,
            O_QUE_BATE,
            "maquina.programa.rodar({ comando: 'bin/servidor', argumentos: ARGUMENTOS, pasta: '' })               .then(() => maquina.pedir('rodou'), (e) => maquina.pedir('recusou ' + e.erro));",
        );
        assert!(
            viva.jogo_rodando(),
            "o processo roda e a metade diz que não"
        );
        assert!(maquinas.jogo_rodando(&viva.chave));
        let ouvidos: Arc<Mutex<Vec<bool>>> = Arc::default();
        let anota = Arc::clone(&ouvidos);
        viva.ao_mudar_o_jogo(Box::new(move |_, rodando| {
            anota.lock().unwrap().push(rodando);
        }));
        parar_todas(maquinas.tirar_ligadas(|_, _| true));
        let fim = Instant::now() + ESPERA_DOS_TESTES;
        while Instant::now() < fim && !ouvidos.lock().unwrap().contains(&false) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ouvidos = ouvidos.lock().unwrap().clone();
        assert_eq!(
            ouvidos.last(),
            Some(&false),
            "o fim do jogo não foi dito: {ouvidos:?}"
        );
        assert!(!viva.jogo_rodando());
    }

    /// **A órfã que ninguém chama de volta para, com o motivo**: o prazo dela
    /// a tira do registro, o `aoParar` ouve `sem-volta` e o processo morre. O
    /// prazo de uma orfandade não para a metade religada depois dela.
    #[test]
    fn a_orfa_que_ninguem_chama_de_volta_para_com_o_motivo() {
        let casa = pasta_do_teste("sem-volta");
        let (maquinas, fio, viva, _raiz) = ligada_com_processo(
            &casa,
            O_QUE_BATE,
            "maquina.aoParar((motivo) => maquina.pedir('parou ' + motivo));             maquina.programa.rodar({ comando: 'bin/servidor', argumentos: ARGUMENTOS, pasta: '' })               .then(() => maquina.pedir('rodou'), (e) => maquina.pedir('recusou ' + e.erro));",
        );
        let (_, antiga) = maquinas.orfanar_de_geracao_velha(1, 2).pop().unwrap();
        // Religada antes do prazo: o prazo daquela vez não a para.
        let igual = pedido(oferta(), "aaaa", "h1");
        assert!(maquinas
            .religar_se_orfa(&igual, 2, Arc::clone(&fio) as Arc<dyn Fio>)
            .is_some());
        assert!(
            maquinas
                .tirar_orfa_vencida(&viva.chave, viva.numero, antiga)
                .is_none(),
            "o prazo de antes parou a metade religada"
        );
        // Órfã de novo, e ninguém chama. O prazo da primeira orfandade, que
        // vence no meio desta, também não a para: só o desta vez.
        let (_, vez) = maquinas.orfanar_de_geracao_velha(1, 3).pop().unwrap();
        assert!(
            maquinas
                .tirar_orfa_vencida(&viva.chave, viva.numero, antiga)
                .is_none(),
            "o prazo de uma orfandade velha parou a órfã de agora"
        );
        let vencida = maquinas
            .tirar_orfa_vencida(&viva.chave, viva.numero, vez)
            .expect("a órfã vencida não saiu do registro");
        assert!(!maquinas.ligada(&viva.chave));
        assert!(vencida.encerrar(Fim::SemVolta));
        fio.esperar(r#"pedir "parou sem-volta""#);
        assert_eq!(
            viva.processos_vivos(),
            0,
            "o processo da órfã vencida segue vivo"
        );
        assert!(
            !fio.anotado().iter().any(|l| l.starts_with("parar-maquina")),
            "a parada foi dita a quem hospeda, que não a tem mais"
        );
    }

    /// **Sair da sessão não para as órfãs**: elas já não são dela (a sessão
    /// caiu com quem hospeda). A ligada que não é órfã para como sempre.
    #[test]
    fn sair_da_sessao_nao_para_as_orfas() {
        let maquinas = Maquinas::default();
        let orfa = Chave::de("casa", "x/jogo", "orfa");
        let viva = Chave::de("casa", "x/jogo", "viva");
        {
            let mut estado = maquinas.estado();
            estado.ligadas.insert(orfa.clone(), ligada_de(1, 1));
            estado.ligadas.insert(viva.clone(), ligada_de(1, 2));
        }
        assert_eq!(maquinas.orfanar_de_geracao_velha(1, 2).len(), 1);
        sessao_saiu_sem_tela(&maquinas, 1);
        assert!(maquinas.ligada(&orfa), "sair da sessão parou a órfã");
        assert!(!maquinas.ligada(&viva), "a ligada da sessão não parou");
        parar_todas(maquinas.tirar_ligadas(|_, _| true));
    }

    /// **A principal que volta sem a posse desliga o jogo e diz** — no
    /// caminho do app, agora possível: a metade seguiu com o jogo de pé sem
    /// quem hospeda, foi religada, pergunta a posse no `aoReconectar`, ouve
    /// `dona: false`, manda `stop`, espera o jogo sair e conta à metade de
    /// servidor (que conta e mostra, `voltouRodando`). Nenhum processo fica.
    #[test]
    fn a_principal_que_volta_sem_a_posse_mata_o_jogo_e_diz() {
        let casa = pasta_do_teste("sem-posse");
        let (maquinas, _fio, viva, raiz) = ligada_com_processo(
            &casa,
            O_QUE_PARA_NO_STOP,
            "maquina.programa.rodar({ comando: 'bin/servidor', argumentos: ARGUMENTOS, pasta: '' })               .then((p) => {                 maquina.aoReconectar(async () => {                   const r = JSON.parse(await maquina.pedir({ op: 'quem-e-a-dona' }));                   if (r.dona === false) {                     await p.escrever('stop');                     const fim = await p.fim;                     await maquina.pedir({ op: 'perdeu-a-posse', codigo: fim.codigo });                   }                 });                 maquina.pedir('rodou');               }, (e) => maquina.pedir('recusou ' + e.erro));",
        );
        let _ = maquinas.orfanar_de_geracao_velha(1, 2);
        assert_eq!(
            viva.processos_vivos(),
            1,
            "o jogo não seguiu de pé sem quem hospeda"
        );
        let novo = Arc::new(FioDeMentira {
            resposta: Mutex::new(Some(
                r#"{"ok":true,"dona":false,"ordem":"desligar"}"#.to_owned(),
            )),
            ..FioDeMentira::default()
        });
        let (religada, fora) = maquinas
            .religar_se_orfa(
                &pedido(oferta(), "aaaa", "h1"),
                2,
                Arc::clone(&novo) as Arc<dyn Fio>,
            )
            .expect("não religou");
        religada.reconectar(fora);
        let anotado = novo.esperar(r#"pedir {"codigo":0,"op":"perdeu-a-posse"}"#);
        assert_eq!(
            anotado.first().map(String::as_str),
            Some(r#"pedir {"op":"quem-e-a-dona"}"#),
            "a religada não perguntou a posse antes: {anotado:?}"
        );
        assert!(
            raiz.join("parou-no-stop").exists(),
            "o stop não chegou ao jogo"
        );
        let fim = Instant::now() + ESPERA_DOS_TESTES;
        while viva.processos_vivos() > 0 && Instant::now() < fim {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            viva.processos_vivos(),
            0,
            "o jogo de quem não é a dona segue de pé"
        );
        parar_todas(maquinas.tirar_ligadas(|_, _| true));
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

    /// **Um servir que volta do fio depois do fim não deixa túnel sem dono.**
    /// O servir sai, o fim é pedido enquanto ele está no fio (aqui sem
    /// `aoParar`, a janela mais larga), e quando ele volta `Ok` o túnel é parado
    /// na hora, em vez de entrar num conjunto que a parada já tomou.
    #[test]
    fn um_servir_que_volta_depois_do_fim_e_parado() {
        let fio = Arc::new(FioDeMentira {
            atraso_do_servir: Duration::from_millis(300),
            ..FioDeMentira::default()
        });
        let viva = subir_com(
            &fio,
            "maquina.porta.servir({ nome: 'mc', protocolo: 'tcp', porta: 25565 }).catch(() => {});",
            Arc::default(),
        );
        fio.esperar("servir mc");
        assert!(viva.encerrar(Fim::MetadeFalhou));
        let anotado = fio.esperar("parar-tunel mc");
        assert_eq!(
            anotado,
            vec![
                "servir mc tcp 25565".to_owned(),
                "parar-maquina metade-falhou".to_owned(),
                "parar-tunel mc".to_owned(),
            ]
        );
        assert!(
            viva.tuneis_servidos().is_empty(),
            "o túnel tardio entrou no conjunto"
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
        let fim = Instant::now() + ESPERA_DOS_TESTES;
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

    // ------------------------------------------------- o poder `programa`

    /// Uma pasta temporária, apagada ao sair.
    struct PastaDoTeste(std::path::PathBuf);

    impl Drop for PastaDoTeste {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn pasta_do_teste(nome: &str) -> PastaDoTeste {
        use std::sync::atomic::AtomicU64;
        static N: AtomicU64 = AtomicU64::new(0);
        let caminho = std::env::temp_dir().join(format!(
            "seele-app-programa-{nome}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&caminho);
        std::fs::create_dir_all(&caminho).unwrap();
        PastaDoTeste(caminho)
    }

    /// A oferta com o `server.jar` da Mojang e um comando aceito.
    fn oferta_com_programa(comando: &str, argumentos: &[&str]) -> OfertaDeMaquinaFfi {
        let mut o = oferta();
        o.artefatos.push(seele_ffi::types::ArtefatoOferecidoFfi {
            nome: "server.jar".to_owned(),
            url: "https://piston-data.mojang.com/v1/objects/x/server.jar".to_owned(),
            tamanho: 10,
            hash: seele_ffi::types::HashDeArtefatoFfi {
                algoritmo: "sha1".to_owned(),
                hex: "a".repeat(40),
            },
            plataformas: vec![],
        });
        o.rodar = Some(seele_ffi::types::ComandoOferecidoFfi {
            comando: comando.to_owned(),
            argumentos: argumentos.iter().map(|a| (*a).to_owned()).collect(),
        });
        o
    }

    fn subir_com_programa(
        fio: &Arc<FioDeMentira>,
        oferta: OfertaDeMaquinaFfi,
        programa: programa::ProgramaDaMetade,
        fonte: &str,
    ) -> Arc<Viva> {
        let viva = Viva::subir(
            Chave::de("casa", "x/jogo", "mundo"),
            7,
            oferta,
            vec!["maquina", "maquina.programa"],
            programa,
            Arc::clone(fio) as Arc<dyn Fio>,
            Box::new(|_, _| {}),
        )
        .expect("a metade sobe");
        viva.iniciar(fonte).expect("o código entra");
        viva
    }

    /// **O pack vem da metade de servidor, e só o aceito entra no disco**
    /// (API 8 parte 6; spec §8): pela metade de máquina de verdade (QuickJS e
    /// prelúdio), `maquina.programa.trazer` pede o volume ao fio, e o núcleo
    /// confere os bytes contra o `pack` do aceite guardado. Bytes trocados no
    /// servidor voltam `pack-diferente` com os dois hashes, e nada é gravado;
    /// o volume recusado volta `pack-nao-veio` com o texto do servidor; e o
    /// pack que já está não é buscado de novo.
    #[test]
    fn o_pack_trazido_so_entra_com_o_hash_do_aceite() {
        use sha2::Digest as _;
        let casa = pasta_do_teste("trazer");
        let home = casa.0.to_string_lossy().into_owned();
        let pack = b"PK\x03\x04 um pack de verdade".to_vec();
        let hex: String = sha2::Sha512::digest(&pack)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut oferta = oferta_com_programa("java/bin/java", &["-jar", "server.jar"]);
        oferta.pack = Some(seele_ffi::types::HashDeArtefatoFfi {
            algoritmo: "sha512".to_owned(),
            hex: hex.clone(),
        });
        let aceite = guardado(oferta, "aaaa", "h1");
        let caps = ["maquina", "maquina.programa"];
        let rodar = |volume: Option<Vec<u8>>| {
            let programa = programa::programa_para(&caps, "aaaa", "h1", Some(&aceite), &home)
                .expect("o aceite monta o programa");
            let raiz = programa.raiz().to_path_buf();
            let fio = Arc::new(FioDeMentira {
                volume: Mutex::new(volume),
                ..FioDeMentira::default()
            });
            let _viva = subir_com_programa(
                &fio,
                aceite.oferta.clone(),
                Ok(programa),
                "maquina.programa.trazer({ pedido: { op: 'pack' }, destino: 'pack.mrpack' })\
                   .then((r) => maquina.pedir('trouxe ' + r.jaEstava + ' ' + r.tamanho + ' ' + r.hash.slice(0, 8)),\
                         (e) => maquina.pedir('recusou ' + e.erro + ' ' + (e.esperado || e.motivo || '').slice(0, 26) + ' ' + (e.obtido || '').slice(0, 8)));",
            );
            let anotado = fio.esperar("pedir ");
            (anotado, raiz)
        };

        let (anotado, raiz) = rodar(Some(b"PK\x03\x04 outro pack".to_vec()));
        let trocado: String = sha2::Sha512::digest(b"PK\x03\x04 outro pack")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(
            anotado
                .iter()
                .any(|l| l.starts_with(r#"ler-volume {"op":"pack"}"#)),
            "{anotado:?}"
        );
        let ultima = anotado.iter().find(|l| l.starts_with("pedir ")).unwrap();
        assert_eq!(
            ultima,
            &format!(
                "pedir \"recusou pack-diferente {} {}\"",
                &hex[..26],
                &trocado[..8]
            )
        );
        assert!(
            !raiz.join("pack.mrpack").exists(),
            "o pack trocado foi gravado"
        );

        let (anotado, _) = rodar(None);
        let ultima = anotado.iter().find(|l| l.starts_with("pedir ")).unwrap();
        assert_eq!(
            ultima,
            "pedir \"recusou pack-nao-veio leitura recusada pelo MOD \""
        );

        let (anotado, raiz) = rodar(Some(pack.clone()));
        let ultima = anotado.iter().find(|l| l.starts_with("pedir ")).unwrap();
        assert_eq!(
            ultima,
            &format!("pedir \"trouxe false {} {}\"", pack.len(), &hex[..8])
        );
        assert_eq!(std::fs::read(raiz.join("pack.mrpack")).unwrap(), pack);

        // Já está: sem pedir o volume.
        let (anotado, _) = rodar(None);
        assert!(
            !anotado.iter().any(|l| l.starts_with("ler-volume")),
            "{anotado:?}"
        );
        let ultima = anotado.iter().find(|l| l.starts_with("pedir ")).unwrap();
        assert_eq!(
            ultima,
            &format!("pedir \"trouxe true {} {}\"", pack.len(), &hex[..8])
        );
    }

    /// **Sem o aceite, nenhum `baixar` e nenhum `rodar`** (spec, «Como provar
    /// que funciona»). Sem aceite guardado, `programa_para` não monta nada; a
    /// metade sobe sem `Programa`, e cada pedido volta `sem-aceite`, contado,
    /// sem tocar o disco.
    #[test]
    fn sem_o_aceite_nada_baixa_e_nada_roda() {
        let casa = pasta_do_teste("sem-aceite");
        let home = casa.0.to_string_lossy().into_owned();
        let caps = ["maquina", "maquina.programa"];
        let sem = programa::programa_para(&caps, "aaaa", "h1", None, &home);
        assert_eq!(sem.err(), Some(SemPrograma::SemAceite));

        let fio = Arc::new(FioDeMentira::default());
        let viva = subir_com_programa(
            &fio,
            oferta_com_programa("java/bin/java", &[]),
            Err(SemPrograma::SemAceite),
            "const r = (p) => p.then(() => 'passou', (e) => e.erro);\
             (async () => {\
               const b = await r(maquina.programa.baixar({ url: 'https://piston-data.mojang.com/v1/objects/x/server.jar', sha1: 'a'.repeat(40), tamanho: 10, destino: 'server.jar' }));\
               const x = await r(maquina.programa.rodar({ comando: 'java/bin/java', argumentos: [], pasta: '' }));\
               const e = await r(maquina.programa.escrever({ destino: 'eula.txt', texto: 'eula=true' }));\
               await maquina.pedir([b, x, e].join(','));\
             })();",
        );
        let anotado = fio.esperar("pedir ");
        assert_eq!(
            anotado,
            vec![r#"pedir "sem-aceite,sem-aceite,sem-aceite""#.to_owned()]
        );
        assert_eq!(
            viva.recusas_da_cola.todas_para_o_teste().get("sem-aceite"),
            Some(&3)
        );
        assert!(
            std::fs::read_dir(&casa.0).unwrap().next().is_none(),
            "a metade sem aceite tocou o disco"
        );
        viva.descartar();
    }

    /// **Mudar a versão ou o pack não roda até o aceite novo.** A oferta nova
    /// tem outro resumo (o servidor o calcula de tudo o que a tela mostra), e
    /// o aceite guardado é o da velha: a política não sai dele. O pacote
    /// trocado, com o mesmo resumo, também não.
    #[test]
    fn a_versao_ou_o_pack_mudados_nao_rodam_sem_aceite_novo() {
        let casa = pasta_do_teste("mudou");
        let home = casa.0.to_string_lossy().into_owned();
        let velho = guardado(oferta_com_programa("java/bin/java", &[]), "aaaa", "h1");
        assert!(programa::politica_para("aaaa", "h1", Some(&velho), &home).is_ok());
        assert_eq!(
            programa::politica_para("bbbb", "h1", Some(&velho), &home).err(),
            Some(SemPrograma::AceiteDiferente),
            "a oferta nova rodou com o aceite da velha"
        );
        assert_eq!(
            programa::politica_para("aaaa", "h2", Some(&velho), &home).err(),
            Some(SemPrograma::AceiteDiferente),
            "o pacote novo rodou com o aceite do velho"
        );
        // E a tela pergunta de novo, dizendo o que mudou.
        let mut nova = velho.oferta.clone();
        nova.versao_do_jogo = "1.21.5".to_owned();
        nova.pack = Some(seele_ffi::types::HashDeArtefatoFfi {
            algoritmo: "sha512".to_owned(),
            hex: "c".repeat(128),
        });
        assert_eq!(
            decidir(&pedido(nova, "bbbb", "h1"), Some(&velho)),
            Decisao::Mostrar {
                mudou: Some(vec!["versao_do_jogo", "pack"])
            }
        );
    }

    /// **Uma URL fora do aceite é recusada**, com o host e os aceitos, sem
    /// pedido à rede e sem arquivo; e a do artefato aceito com outro hash
    /// também.
    #[test]
    fn uma_url_fora_do_aceite_e_recusada_com_os_dados() {
        let casa = pasta_do_teste("fora");
        let home = casa.0.to_string_lossy().into_owned();
        let aceite = guardado(oferta_com_programa("java/bin/java", &[]), "aaaa", "h1");
        let caps = ["maquina", "maquina.programa"];
        let programa = programa::programa_para(&caps, "aaaa", "h1", Some(&aceite), &home)
            .expect("o aceite monta o programa");
        let raiz = programa.raiz().to_path_buf();
        let fio = Arc::new(FioDeMentira::default());
        let viva = subir_com_programa(
            &fio,
            aceite.oferta.clone(),
            Ok(programa),
            "const r = (p) => p.then(() => 'passou', (e) => [e.erro, e.host || e.campo, (e.aceitos || []).join('+')].join(' '));\
             (async () => {\
               const fora = await r(maquina.programa.baixar({ url: 'https://evil.example/server.jar', sha1: 'a'.repeat(40), tamanho: 10, destino: 'server.jar' }));\
               const outro = await r(maquina.programa.baixar({ url: 'https://piston-data.mojang.com/v1/objects/x/server.jar', sha1: 'b'.repeat(40), tamanho: 10, destino: 'server.jar' }));\
               const recusas = await maquina.programa.recusas();\
               await maquina.pedir(fora + ' | ' + outro + ' | ' + JSON.stringify(recusas));\
             })();",
        );
        let anotado = fio.esperar("pedir ");
        assert_eq!(
            anotado,
            vec![concat!(
                r#"pedir "dominio-fora-do-aceito evil.example piston-data.mojang.com | "#,
                r#"diferente-do-aceito hash  | "#,
                r#"{\"diferente-do-aceito\":1,\"dominio-fora-do-aceito\":1}""#
            )
            .to_owned()]
        );
        assert!(!raiz.join("server.jar").exists());
        viva.descartar();
    }

    /// O nome deste teste, para o filho o achar com `--exact`.
    const O_QUE_BATE: &str = "maquina::testes::um_processo_que_bate";

    /// **Não é um teste**: é o processo que os testes de baixo rodam pelo
    /// `programa` (o próprio binário de testes, ligado na pasta do servidor
    /// de jogo), no Mac e no Windows, sem `sh`. Escreve um contador em
    /// `batimento`, na pasta em que roda, a cada 20 ms, por até 60 s.
    #[test]
    #[ignore = "é o processo filho dos testes do programa, e não um teste"]
    fn um_processo_que_bate() {
        let _ = std::fs::write(DE_PE, "");
        for n in 0_u64..3000 {
            let _ = std::fs::write("batimento", n.to_string());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// O nome do segundo processo filho.
    const O_QUE_PARA_NO_STOP: &str = "maquina::testes::um_processo_que_para_no_stop";

    /// **Não é um teste**: um servidor de jogo de mentira que lê a entrada e
    /// sai com código 0 quando lê `stop`, como o Minecraft. Desiste em 60 s.
    #[test]
    #[ignore = "é o processo filho dos testes do programa, e não um teste"]
    fn um_processo_que_para_no_stop() {
        let _ = std::fs::write(DE_PE, "");
        let (manda, recebe) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for linha in std::io::stdin().lines() {
                if linha.is_ok_and(|l| l.trim() == "stop") {
                    let _ = manda.send(());
                    return;
                }
            }
        });
        println!("pronto");
        if recebe.recv_timeout(Duration::from_secs(60)).is_ok() {
            let _ = std::fs::write("parou-no-stop", "sim");
        }
    }

    /// Põe este executável de testes em `alvo`, para um teste o rodar como
    /// filho pelo `programa`.
    ///
    /// **No macOS, uma cópia com inode próprio, e nunca um hard link.** Era o
    /// que estes testes dividiam, e por que falhavam em paralelo e passavam
    /// um de cada vez: com hard links, todos rodavam o **mesmo arquivo** por
    /// nomes diferentes. O macOS confere um executável que sobe
    /// (`syspolicyd`, a pedido do kernel) lendo o arquivo por **um** nome,
    /// que o kernel tira do arquivo e não de quem o rodou — e, com vários
    /// nomes em uso ao mesmo tempo, esse nome era o da pasta de outro teste,
    /// que acabava e a apagava, ou um que nunca existiu (a pasta de um, o
    /// nome de outro). A leitura falhava (`errno: 2`) e o sistema matava o
    /// filho com `SIGKILL`, ainda subindo: «Terminating process due to
    /// Malware rejection», no log do sistema.
    ///
    /// Medido em 11/11/2026, `maquina` em paralelo: com hard link, 7 de 150
    /// rodadas falharam, com 9 filhos de 900 mortos assim (os 9 com aquela
    /// linha no log); com a cópia e a espera de [`esperar_o_filho_de_pe`],
    /// nenhuma de 235.
    ///
    /// **A cópia sozinha não basta**, e troca um defeito por outro: é um
    /// arquivo novo a cada rodada, que o sistema confere inteiro (o filho
    /// leva perto de meio segundo a mais para subir), e apagá-lo no meio
    /// dessa conferência é o que [`esperar_o_filho_de_pe`] evita. No APFS a
    /// cópia é um clone, e não gasta disco.
    ///
    /// **Fora do macOS segue o hard link**, como era: nada disto foi medido
    /// lá, e no Linux uma cópia escrita por um teste enquanto outro cria um
    /// processo pode ter a execução recusada (`ETXTBSY`).
    fn por_este_executavel_em(alvo: &std::path::Path) {
        let eu = std::env::current_exe().unwrap();
        if cfg!(target_os = "macos") || std::fs::hard_link(&eu, alvo).is_err() {
            std::fs::copy(&eu, alvo).unwrap();
        }
    }

    /// O arquivo que o filho de um teste grava, na pasta em que roda, quando
    /// começa a rodar ([`esperar_o_filho_de_pe`]).
    const DE_PE: &str = "de-pe";

    /// **Espera o filho de um teste começar a rodar de verdade**, e não só
    /// existir: o `rodar` volta quando o processo foi criado, e no macOS ele
    /// fica parado, antes da primeira instrução, até o sistema acabar de
    /// conferir o executável (perto de 1 s num arquivo novo deste tamanho).
    ///
    /// **Todo teste que roda um filho passa por aqui antes de seguir**, porque
    /// seguir é matar o filho e apagar a pasta — e fazer isso com a
    /// conferência do executável ainda em curso derruba o `syspolicyd`
    /// (`SIGSEGV` em `SecStaticCodeCheckValidityWithErrors`; três relatórios
    /// em `/Library/Logs/DiagnosticReports`, 11/11/2026, cada um a
    /// milissegundos de um teste fazer isso). Com ele caído, os executáveis
    /// novos da máquina esperam mais de um minuto para subir: 3 de 34 rodadas
    /// de `maquina` falharam assim, quatro testes de uma vez. O filho que
    /// roda já foi conferido — com a espera, nenhuma queda em 235 rodadas.
    ///
    /// Um filho que morre antes de rodar é dito na hora, e não depois do
    /// prazo: é o que o sistema faz com um executável que ele recusa.
    fn esperar_o_filho_de_pe(viva: &Viva, raiz: &std::path::Path) {
        let fim = Instant::now() + ESPERA_DOS_TESTES;
        while !raiz.join(DE_PE).exists() {
            assert!(
                viva.processos_vivos() > 0,
                "o filho do teste morreu antes de rodar (no macOS, o log do \
                 sistema diz por quê: procure o pid dele em `syspolicyd`)"
            );
            assert!(Instant::now() < fim, "o filho do teste não subiu");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// **O guarda do de cima**: no macOS, o executável que um teste roda como
    /// filho não divide o arquivo com o de nenhum outro teste (nem com este
    /// binário). Com um hard link, o arquivo teria mais de um nome.
    // `unix` escrito por extenso: o guarda de plataforma do xtask procura a
    // palavra antes de todo `std::os::unix`.
    #[cfg(all(unix, target_os = "macos"))]
    #[test]
    fn o_filho_de_cada_teste_e_um_arquivo_so_dele() {
        use std::os::unix::fs::MetadataExt as _;
        let casa = pasta_do_teste("inode");
        let (um, outro) = (casa.0.join("um"), casa.0.join("outro"));
        por_este_executavel_em(&um);
        por_este_executavel_em(&outro);
        let eu = std::fs::metadata(std::env::current_exe().unwrap()).unwrap();
        let (um, outro) = (
            std::fs::metadata(um).unwrap(),
            std::fs::metadata(outro).unwrap(),
        );
        assert_eq!(um.len(), eu.len(), "a cópia não é o executável inteiro");
        assert_eq!(
            (um.nlink(), outro.nlink()),
            (1, 1),
            "o filho de um teste é um hard link: divide o arquivo com outro nome"
        );
        assert_ne!(um.ino(), outro.ino(), "dois testes dividem o arquivo");
        assert_ne!(um.ino(), eu.ino(), "o teste divide o arquivo com o binário");
    }

    /// Um `Programa` que roda o próprio binário de testes, com `argumentos`.
    fn programa_que_roda_o_teste(
        casa: &PastaDoTeste,
        argumentos: &[&str],
    ) -> (
        programa::ProgramaDaMetade,
        OfertaDeMaquinaFfi,
        std::path::PathBuf,
    ) {
        let home = casa.0.to_string_lossy().into_owned();
        let aceite = guardado(
            oferta_com_programa("bin/servidor", argumentos),
            "aaaa",
            "h1",
        );
        let caps = ["maquina", "maquina.programa"];
        let programa = programa::programa_para(&caps, "aaaa", "h1", Some(&aceite), &home)
            .expect("o aceite monta o programa");
        let raiz = programa.raiz().to_path_buf();
        std::fs::create_dir_all(raiz.join("bin")).unwrap();
        let alvo = raiz
            .join("bin")
            .join(format!("servidor{}", std::env::consts::EXE_SUFFIX));
        por_este_executavel_em(&alvo);
        (Ok(programa), aceite.oferta, raiz)
    }

    /// **O `aoParar` fala com o jogo**: parando, a metade não começa nada
    /// novo, mas ainda escreve na entrada do processo dela — é por ali que o
    /// `stop` chega e o mundo é salvo antes de o produto matar. Medido no
    /// Minecraft 1.21.4 (31/10/2026): com o `stop` recusado, a parada esperava
    /// o prazo do MOD inteiro (15 s) e o jogo morria sem salvar.
    #[test]
    fn o_ao_parar_manda_stop_e_o_jogo_sai_sozinho() {
        let casa = pasta_do_teste("stop");
        let argumentos = [
            "--ignored",
            "--exact",
            O_QUE_PARA_NO_STOP,
            "--test-threads",
            "1",
        ];
        let (programa, oferta, raiz) = programa_que_roda_o_teste(&casa, &argumentos);
        let fio = Arc::new(FioDeMentira::default());
        let viva = subir_com_programa(
            &fio,
            oferta,
            programa,
            &format!(
                "maquina.programa.rodar({{ comando: 'bin/servidor', argumentos: {}, pasta: '' }}).then((p) => {{                   maquina.aoParar(async () => {{ await p.escrever('stop'); const fim = await p.fim; await maquina.pedir('fim ' + fim.codigo + ' ' + fim.mortoAPedido); }});                   maquina.pedir('rodou');                 }}, (e) => maquina.pedir('recusou ' + e.erro));",
                serde_json::to_string(&argumentos).unwrap()
            ),
        );
        fio.esperar(r#"pedir "rodou""#);
        esperar_o_filho_de_pe(&viva, &raiz);
        let relogio = Instant::now();
        assert!(viva.encerrar(Fim::PeloDono));
        let levou = relogio.elapsed();
        let anotado = fio.anotado();
        assert!(
            anotado.iter().any(|l| l == r#"pedir "fim 0 false""#),
            "o jogo não saiu sozinho com o stop: {anotado:?} (levou {levou:?})"
        );
        assert!(
            raiz.join("parou-no-stop").exists(),
            "o stop não chegou ao processo"
        );
        assert!(
            levou < Duration::from_secs(10),
            "a parada esperou o prazo: {levou:?}"
        );
    }

    fn batimento(raiz: &std::path::Path) -> Option<u64> {
        std::fs::read_to_string(raiz.join("batimento"))
            .ok()
            .and_then(|t| t.trim().parse().ok())
    }

    /// **Os processos morrem quando a metade para**, e a parada espera: com o
    /// jogo de pé, `encerrar` (o caminho de todo motivo: DESLIGAR, REVOGAR, o
    /// app fechando, o MOD desligado) volta só com o processo morto.
    #[test]
    fn os_processos_morrem_quando_a_metade_para() {
        let casa = pasta_do_teste("morre");
        let home = casa.0.to_string_lossy().into_owned();
        let argumentos = ["--ignored", "--exact", O_QUE_BATE, "--test-threads", "1"];
        let aceite = guardado(
            oferta_com_programa("bin/servidor", &argumentos),
            "aaaa",
            "h1",
        );
        let caps = ["maquina", "maquina.programa"];
        let programa = programa::programa_para(&caps, "aaaa", "h1", Some(&aceite), &home)
            .expect("o aceite monta o programa");
        let raiz = programa.raiz().to_path_buf();
        std::fs::create_dir_all(raiz.join("bin")).unwrap();
        let alvo = raiz
            .join("bin")
            .join(format!("servidor{}", std::env::consts::EXE_SUFFIX));
        por_este_executavel_em(&alvo);
        let fio = Arc::new(FioDeMentira::default());
        let viva = subir_com_programa(
            &fio,
            aceite.oferta.clone(),
            Ok(programa),
            &format!(
                "maquina.programa.rodar({{ comando: 'bin/servidor', argumentos: {}, pasta: '' }})\
                   .then((p) => maquina.pedir('rodou ' + p.numero), (e) => maquina.pedir('recusou ' + e.erro + ' ' + e.frase));",
                serde_json::to_string(&argumentos).unwrap()
            ),
        );
        let anotado = fio.esperar("pedir ");
        assert!(anotado[0].starts_with(r#"pedir "rodou "#), "{anotado:?}");
        esperar_o_filho_de_pe(&viva, &raiz);
        let fim = Instant::now() + ESPERA_DOS_TESTES;
        while batimento(&raiz).unwrap_or(0) < 3 {
            assert!(Instant::now() < fim, "o processo não bateu");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(viva.processos_vivos(), 1);

        assert!(viva.encerrar(Fim::PeloDono));
        assert_eq!(
            viva.processos_vivos(),
            0,
            "encerrar voltou com processo vivo"
        );
        let parado = batimento(&raiz);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            batimento(&raiz),
            parado,
            "o processo seguiu batendo depois da parada"
        );
    }

    // ------------------------------------- o Minecraft de verdade (rede)

    /// O status de um servidor Minecraft Java (handshake + status request), e
    /// o JSON que ele devolve: é o que o cliente do jogo faz na lista de
    /// servidores.
    fn status_do_minecraft(porta: u16) -> std::io::Result<String> {
        use std::io::{Read as _, Write as _};
        fn varint(mut v: u32, saida: &mut Vec<u8>) {
            loop {
                let b = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 {
                    saida.push(b);
                    return;
                }
                saida.push(b | 0x80);
            }
        }
        fn ler_varint(r: &mut impl std::io::Read) -> std::io::Result<u32> {
            let mut v = 0_u32;
            for i in 0..5 {
                let mut b = [0_u8];
                r.read_exact(&mut b)?;
                v |= u32::from(b[0] & 0x7f) << (7 * i);
                if b[0] & 0x80 == 0 {
                    return Ok(v);
                }
            }
            Err(std::io::Error::other("varint longo demais"))
        }
        let mut fluxo = std::net::TcpStream::connect(("127.0.0.1", porta))?;
        fluxo.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut corpo = vec![0x00];
        varint(769, &mut corpo); // 1.21.4
        varint(9, &mut corpo);
        corpo.extend_from_slice(b"127.0.0.1");
        corpo.extend_from_slice(&porta.to_be_bytes());
        varint(1, &mut corpo);
        let mut pacote = Vec::new();
        varint(u32::try_from(corpo.len()).unwrap(), &mut pacote);
        pacote.extend_from_slice(&corpo);
        pacote.extend_from_slice(&[0x01, 0x00]);
        fluxo.write_all(&pacote)?;
        let _tamanho = ler_varint(&mut fluxo)?;
        let _id = ler_varint(&mut fluxo)?;
        let n = ler_varint(&mut fluxo)? as usize;
        let mut json = vec![0_u8; n];
        fluxo.read_exact(&mut json)?;
        Ok(String::from_utf8_lossy(&json).into_owned())
    }

    /// O lado de servidor do MOD, para a metade de máquina de verdade: o
    /// `plano` que a metade de servidor dá à máquina aceita, e cada estado e
    /// falha anotados com a hora.
    struct FioDoMinecraft {
        plano: serde_json::Value,
        /// O `.mrpack` que o volume da metade de servidor guardaria.
        pack: Option<String>,
        comeco: Instant,
        anotado: Mutex<Vec<(Duration, String)>>,
    }

    impl FioDoMinecraft {
        fn anotar(&self, linha: String) {
            eprintln!("[{:>7.1?}] {linha}", self.comeco.elapsed());
            self.anotado
                .lock()
                .unwrap()
                .push((self.comeco.elapsed(), linha));
        }
        fn quando(&self, prefixo: &str) -> Option<Duration> {
            self.anotado
                .lock()
                .unwrap()
                .iter()
                .find(|(_, l)| l.starts_with(prefixo))
                .map(|(t, _)| *t)
        }
    }

    impl Fio for FioDoMinecraft {
        fn pedir(&self, _: &str, _: &str, payload: String) -> Futuro<Result<String, &'static str>> {
            let valor: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
            let resposta = match valor["op"].as_str() {
                Some("plano") => serde_json::json!({ "ok": true, "plano": self.plano }),
                Some("estado") => {
                    let mut linha = format!("estado {}", valor["estado"].as_str().unwrap_or("?"));
                    if let Some(j) = valor["jogadores"].as_array().filter(|j| !j.is_empty()) {
                        linha.push_str(&format!(" jogadores={j:?}"));
                    }
                    self.anotar(linha);
                    serde_json::json!({ "ok": true })
                }
                Some("falha") => {
                    self.anotar(format!("falha {payload}"));
                    serde_json::json!({ "ok": true })
                }
                _ => serde_json::json!({ "ok": false, "erro": "op-desconhecida" }),
            };
            let texto = resposta.to_string();
            Box::pin(async move { Ok(texto) })
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
        fn quedas(&self, _: &str, _: &str) -> Futuro<Result<Vec<(String, u32)>, &'static str>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn parar_maquina(&self, _: &str, _: &str, motivo: &str) -> Result<(), &'static str> {
            self.anotar(format!("parar-maquina {motivo}"));
            Ok(())
        }
        /// O pack «enviado»: os bytes do `.mrpack` que o plano aponta, como a
        /// metade de servidor os serviria pelo volume.
        fn ler_volume(&self, _: &str, payload: String) -> Futuro<Result<Vec<u8>, String>> {
            self.anotar(format!("ler-volume {payload}"));
            let lido = self
                .pack
                .as_ref()
                .ok_or_else(|| "sem pack no plano".to_owned())
                .and_then(|c| std::fs::read(c).map_err(|e| e.to_string()));
            Box::pin(async move { lido })
        }
        fn entrar_tunel(&self, _: &str, nome: &str) -> Futuro<Result<u16, &'static str>> {
            self.anotar(format!("entrar-tunel {nome}"));
            Box::pin(async { Err("sem-resposta") })
        }
        fn sair_tunel(&self, _: &str, nome: &str) {
            self.anotar(format!("sair-tunel {nome}"));
        }
        fn direto(&self, _: &str, _: &str) -> Option<bool> {
            None
        }
    }

    /// A lista de processos do sistema: `(pid, pai, linha de comando)`. No
    /// Unix pelo `ps`; no Windows pelo `Win32_Process` do PowerShell (não há
    /// `ps`, e o `tasklist` não diz o pai).
    fn processos_do_sistema() -> Vec<(u32, u32, String)> {
        #[cfg(unix)]
        let saida = std::process::Command::new("ps")
            .args(["-axo", "pid=,ppid=,command="])
            .output();
        #[cfg(windows)]
        let saida = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId) $($_.ParentProcessId) $($_.CommandLine)\" }",
            ])
            .output();
        let Ok(saida) = saida else {
            return Vec::new();
        };
        String::from_utf8_lossy(&saida.stdout)
            .lines()
            .filter_map(|l| {
                let mut p = l.split_whitespace();
                Some((
                    p.next()?.parse().ok()?,
                    p.next()?.parse().ok()?,
                    p.collect::<Vec<_>>().join(" "),
                ))
            })
            .collect()
    }

    /// **Um Minecraft Vanilla de verdade pela metade de máquina de verdade**
    /// (API 8 parte 4, «Como provar que funciona»): o executor QuickJS do app
    /// com a metade de máquina do MOD de multiplayer, o `programa` montado de
    /// um aceite com a oferta do MOD, o `server.jar` (sha1) e o Java da
    /// Temurin (sha256) baixados da internet, o pacote aberto, o `eula.txt` e
    /// o `server.properties` escritos, o Java rodando, o «Done (» lido pela
    /// metade, o túnel pedido, o status do jogo respondido com o motd, e a
    /// parada pelo dono que manda `stop` e espera.
    ///
    /// Rede de verdade, então fora da suíte. Para rodar:
    ///
    /// ```text
    /// node <SEELE-MOD-MULTIPLAYER>/ferramentas/plano-de-verdade.cjs plano.json   # a oferta e o plano do MOD
    /// SEELE_MINECRAFT_DE_VERDADE=plano.json \
    /// SEELE_MOD_MULTIPLAYER=<SEELE-MOD-MULTIPLAYER> \
    /// SEELE_MINECRAFT_CASA=<pasta que fica> \
    ///   cargo test -p seele-app --bin seele-app minecraft_de_verdade -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "rede de verdade e ~105 MB: SEELE_MINECRAFT_DE_VERDADE=<plano.json>"]
    fn minecraft_de_verdade_pela_metade_de_maquina() {
        let Ok(plano) = std::env::var("SEELE_MINECRAFT_DE_VERDADE") else {
            eprintln!("pulado: sem SEELE_MINECRAFT_DE_VERDADE");
            return;
        };
        let pasta_do_mod = std::env::var("SEELE_MOD_MULTIPLAYER").expect("SEELE_MOD_MULTIPLAYER");
        let casa = std::env::var("SEELE_MINECRAFT_CASA").expect("SEELE_MINECRAFT_CASA");
        let lido: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(plano).unwrap()).unwrap();
        let servidor = lido["servidor"].as_str().unwrap().to_owned();
        let oferta: OfertaDeMaquinaFfi = serde_json::from_value(lido["oferta"].clone()).unwrap();
        let (daqui, outros) = seele_ffi::programa::artefatos_desta_maquina(&oferta);
        eprintln!(
            "esta máquina ({}) baixa {:?}; {outros} de outros sistemas",
            seele_ffi::programa::esta_plataforma(),
            daqui
                .iter()
                .map(|a| (&a.nome, a.tamanho))
                .collect::<Vec<_>>()
        );
        let aceite = AceiteDeMaquinaGuardado {
            alvo: "casa:8383".to_owned(),
            mod_id: "seele/multiplayer".to_owned(),
            hash: "h1".to_owned(),
            servidor: servidor.clone(),
            papel: "principal".to_owned(),
            resumo: "r1".to_owned(),
            oferta: oferta.clone(),
            pedido_por: "ana".to_owned(),
            quando: 1,
        };
        let caps = ["maquina", "maquina.porta", "maquina.programa"];
        let programa = programa::programa_para(&caps, "r1", "h1", Some(&aceite), &casa)
            .expect("o aceite monta o programa");
        eprintln!("pasta do servidor de jogo: {}", programa.raiz().display());
        let raiz = programa.raiz().to_path_buf();
        // **Os netos** (parte 6): o `programa` mata o filho, e não a árvore. A
        // cada 250 ms, a lista de processos do sistema: os que rodam da pasta
        // deste servidor de jogo, e quem tem um deles como pai.
        let vigia_ligada = Arc::new(AtomicBool::new(true));
        let vistos: Arc<Mutex<BTreeMap<u32, (u32, String)>>> = Arc::default();
        let amostrador = {
            let (ligada, vistos, raiz) = (
                Arc::clone(&vigia_ligada),
                Arc::clone(&vistos),
                // Sem o `\\?\` do caminho canônico do Windows, e sem caixa: a
                // linha de comando de um `java.exe` traz a pasta como foi dada.
                raiz.display()
                    .to_string()
                    .trim_start_matches(r"\\?\")
                    .to_lowercase(),
            );
            std::thread::spawn(move || {
                while ligada.load(Ordering::Relaxed) {
                    {
                        let linhas = processos_do_sistema();
                        let do_jogo: Vec<u32> = linhas
                            .iter()
                            .filter(|(_, _, c)| c.to_lowercase().contains(&raiz))
                            .map(|(p, _, _)| *p)
                            .collect();
                        let mut v = vistos.lock().unwrap();
                        for (pid, pai, cmd) in linhas {
                            if do_jogo.contains(&pid) || do_jogo.contains(&pai) {
                                // O começo é a pasta; o que diz quem é, o fim.
                                let fim: String = cmd
                                    .chars()
                                    .rev()
                                    .take(110)
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .rev()
                                    .collect();
                                v.insert(pid, (pai, fim));
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
        };
        let fio = Arc::new(FioDoMinecraft {
            plano: lido["plano"].clone(),
            pack: lido["pack_local"].as_str().map(str::to_owned),
            comeco: Instant::now(),
            anotado: Mutex::default(),
        });
        let viva = Viva::subir(
            Chave::de("casa:8383", "seele/multiplayer", &servidor),
            99,
            oferta,
            caps.to_vec(),
            Ok(programa),
            Arc::clone(&fio) as Arc<dyn Fio>,
            Box::new(|_, texto| eprintln!("A METADE FALHOU: {texto}")),
        )
        .unwrap();
        let fonte = std::fs::read_to_string(format!("{pasta_do_mod}/maquina/main.js")).unwrap();
        viva.iniciar(&fonte).unwrap();

        let fim = Instant::now() + Duration::from_secs(900);
        while fio.quando("servir ").is_none() {
            assert!(Instant::now() < fim, "o túnel não foi pedido em 15 min");
            assert!(
                fio.quando("falha ").is_none(),
                "a metade falhou: {:?}",
                fio.anotado.lock().unwrap()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        let porta = 25565;
        let status = status_do_minecraft(porta).expect("o jogo respondeu ao status");
        eprintln!("status do jogo em 127.0.0.1:{porta}: {status}");
        assert!(
            status.contains("Casa"),
            "o motd não é o da configuração: {status}"
        );

        // Com mods: o que o loader carregou, e o que o pack deixou na pasta.
        // O plano diz o que esperar de qualquer loader (`pack_esperado`, do
        // índice do pack): cada mod do servidor em `mods/`, e nenhum dos
        // `env.server: unsupported`.
        if lido.get("pack_local").is_some() {
            let log =
                std::fs::read_to_string(raiz.join("logs").join("latest.log")).unwrap_or_default();
            for linha in log.lines().filter(|l| {
                (l.contains("Loading") && l.contains("mod"))
                    || l.contains("]: Done (")
                    || l.contains("Found mod file")
            }) {
                eprintln!("log do jogo: {linha}");
            }
            let mods: Vec<String> = std::fs::read_dir(raiz.join("mods"))
                .map(|d| {
                    d.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            eprintln!("mods/ na pasta: {} arquivos: {mods:?}", mods.len());
            eprintln!(
                "config/seele-prova.txt: {:?}; config/so-overrides.txt: {:?}",
                std::fs::read_to_string(raiz.join("config/seele-prova.txt")).ok(),
                std::fs::read_to_string(raiz.join("config/so-overrides.txt")).ok()
            );
            let nomes = |chave: &str| -> Vec<String> {
                lido["pack_esperado"][chave]
                    .as_array()
                    .map(|v| {
                        v.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            for m in nomes("presentes") {
                assert!(mods.contains(&m), "sem {m} em mods/: {mods:?}");
            }
            for m in nomes("ausentes") {
                assert!(!mods.contains(&m), "o mod só do cliente {m} foi baixado");
            }
        }

        let parada = Instant::now();
        assert!(viva.encerrar(Fim::PeloDono));
        let levou = parada.elapsed();
        assert_eq!(viva.processos_vivos(), 0);
        vigia_ligada.store(false, Ordering::Relaxed);
        let _ = amostrador.join();
        let vistos = vistos.lock().unwrap().clone();
        let do_jogo: Vec<u32> = vistos.keys().copied().collect();
        let netos: Vec<(u32, u32, String)> = vistos
            .iter()
            .filter(|(_, (pai, _))| do_jogo.contains(pai))
            .map(|(pid, (pai, cmd))| (*pid, *pai, cmd.clone()))
            .collect();
        eprintln!("processos da pasta do jogo vistos: {}", vistos.len());
        for (pid, (pai, cmd)) in &vistos {
            eprintln!("  pid {pid} (pai {pai}): {cmd}");
        }
        eprintln!("netos (filhos de um processo da pasta): {netos:?}");
        // Algum sobreviveu à parada?
        std::thread::sleep(Duration::from_millis(500));
        // **Nenhum processo da árvore fica** (I9; no Windows pela tarefa,
        // decisão W3): o que a amostra viu da pasta do jogo, e os filhos.
        let vivos_agora: Vec<u32> = processos_do_sistema().iter().map(|(p, _, _)| *p).collect();
        let mut sobreviventes = Vec::new();
        for (pid, (_, cmd)) in &vistos {
            let vivo = vivos_agora.contains(pid);
            eprintln!("  depois da parada, pid {pid} vivo: {vivo} ({cmd})");
            if vivo {
                sobreviventes.push((*pid, cmd.clone()));
            }
        }
        assert!(
            vistos.is_empty() || !vivos_agora.is_empty(),
            "a lista de processos do sistema veio vazia: a amostra não mede nada"
        );
        assert!(
            sobreviventes.is_empty(),
            "processos da árvore do jogo seguiram vivos depois da parada: {sobreviventes:?}"
        );
        let porta_fechada = std::net::TcpStream::connect(("127.0.0.1", porta)).is_err();
        eprintln!(
            "medidas: baixando {:?}, pronto {:?}, ligando {:?}, servir {:?}, a parada pelo dono levou {levou:?}, a porta do jogo fechou: {porta_fechada}",
            fio.quando("estado baixando"),
            fio.quando("estado pronto"),
            fio.quando("estado ligando"),
            fio.quando("servir "),
        );
        assert!(porta_fechada, "o jogo seguiu escutando depois da parada");
    }

    // ------------------------------------------- o jogo que sobrou, e a porta

    /// **Nenhuma metade sobe enquanto o jogo que sobrou ainda salva** (revisão
    /// da 0.19, I1). O pior caso, de propósito: o pedido chega antes de a
    /// procura sequer ler a pasta, e o órfão leva 1 s para sair depois do
    /// `SIGTERM` (o mundo salvando). Quem pede espera exatamente o que
    /// `processar_pedido` e `aceitar` esperam ([`esperar_as_sobras`]) e então
    /// olha o órfão — no lugar em que a metade subiria. Sem a espera, o órfão
    /// ainda roda nesse instante (ele nem recebeu o sinal) e este teste reprova.
    #[cfg(unix)]
    #[test]
    fn nenhuma_metade_sobe_enquanto_o_jogo_que_sobrou_salva() {
        use seele_ffi::programa::{anotar_processo, inicio_do_processo, RegistroDeProcesso};
        use std::os::unix::process::CommandExt as _;

        struct Matar(u32);
        impl Drop for Matar {
            fn drop(&mut self) {
                let _ = std::process::Command::new("kill")
                    .args(["-KILL", &format!("-{}", self.0)])
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
        let vivo = |pid: u32| inicio_do_processo(pid).is_some();

        let casa = pasta_do_teste("sobra-porta");
        let mut filho = std::process::Command::new("sh")
            .args([
                "-c",
                "trap 'sleep 1; exit 0' TERM; while :; do sleep 0.05; done",
            ])
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = filho.id();
        let _solto = Matar(pid);
        std::thread::spawn(move || {
            let _ = filho.wait();
        });
        let fim = Instant::now() + Duration::from_secs(2);
        while !vivo(pid) && Instant::now() < fim {
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut morto = std::process::Command::new("true").spawn().unwrap();
        let dono = morto.id();
        morto.wait().unwrap();
        let chave = Chave::de("127.0.0.1:7000", "x/jogo", "srv-1");
        anotar_processo(
            &casa.0,
            &RegistroDeProcesso {
                pid,
                grupo: pid,
                inicio: inicio_do_processo(pid).expect("o órfão de teste não subiu"),
                dono,
                dono_inicio: "Thu Jan  1 00:00:00 1970".to_owned(),
                etiqueta: etiqueta_da_sobra(&chave, "Minecraft"),
            },
        )
        .unwrap();

        let maquinas = Maquinas::default();
        // Como `procurar_sobras` faz no `setup`, antes de a thread existir.
        maquinas.porta_das_sobras.abrir();
        let avisos: Mutex<Vec<String>> = Mutex::default();
        let vivo_ao_subir = std::thread::scope(|escopo| {
            let pedido = escopo.spawn(|| {
                esperar_as_sobras(&maquinas, &chave);
                vivo(pid)
            });
            // O pedido já está na porta antes de a procura começar.
            std::thread::sleep(Duration::from_millis(50));
            escopo.spawn(|| {
                maquinas
                    .porta_das_sobras
                    .procurar(&casa.0, Duration::from_secs(5), |aviso| {
                        let dito = match aviso {
                            AvisoDaSobra::Parando(r) => sobra_parando_na_tela(r),
                            AvisoDaSobra::Parou(a) => sobra_na_tela(a),
                        };
                        avisos
                            .lock()
                            .unwrap()
                            .push(format!("{} {} {}", dito.desfecho, dito.servidor, dito.pid));
                    });
            });
            pedido.join().unwrap()
        });
        assert!(
            !vivo_ao_subir,
            "a metade subiria com o jogo que sobrou ainda salvando na mesma raiz"
        );
        assert_eq!(
            *avisos.lock().unwrap(),
            vec![format!("parando srv-1 {pid}"), format!("saiu srv-1 {pid}")],
            "a faixa não ouviu o «parando» e então o desfecho"
        );
        assert!(
            !maquinas.porta_das_sobras.fechada(),
            "a porta ficou fechada"
        );
        // Sem sobra nenhuma, a porta não segura ninguém.
        assert_eq!(maquinas.porta_das_sobras.esperar(), None);
    }
}
