//! O executor de MODs do **cliente**: QuickJS, num runtime por instância.
//!
//! Protótipo da etapa E1, e a diretriz de 18/09 diz por que ele existe: o
//! Worker de `blob:` não satisfaz o contrato. A sonda de fronteira releu, num
//! aplicativo reiniciado, uma marca que um MOD tinha gravado em IndexedDB — e
//! armazenamento que sobrevive à sessão é o contrário da promessa deste
//! produto.
//!
//! # Onde mora a volta
//!
//! **No `seele-motor`, desde 09/10/2026.** Este arquivo é o embrulho do
//! desktop: a thread, o canal de entrada, a fila de entrada e a espera
//! ([`rodar`]). O que acontece numa volta — o prelúdio, as rejeições, o balde
//! do `console`, os temporizadores, os tetos — é do [`Motor`], que o celular
//! embrulha num Worker (`crates/seele-motor-web`). Os dois rodam a mesma volta,
//! e a tabela `crates/seele-motor/tests/casos.json` prova isso nos dois alvos.
//!
//! # O que muda de lugar, e o que não muda
//!
//! **Não muda:** a WebView é a mesma, o renderer confiável é o mesmo, e a API é
//! a mesma. Não há uma WebView por MOD, e não há um segundo motor de navegador
//! no instalador.
//!
//! **Muda:** a lógica do autor deixa de rodar num contexto de navegador. Não há
//! `indexedDB` porque não há origem; não há `BroadcastChannel` porque não há
//! outros contextos da mesma origem; não há `fetch` porque ninguém o ligou. O
//! que existe é o que este arquivo põe lá dentro, uma função por vez.
//!
//! # O que este arquivo deliberadamente não faz
//!
//! **Não importa os poderes da metade de servidor.** O `seele-server` tem
//! bindings de disco, de rede e de persistência, e eles existem porque um MOD
//! de servidor roda na máquina de quem hospeda, com o consentimento que aquela
//! tela pede. Trazê-los para cá por conveniência seria dar à metade de janela
//! uma autoridade que ninguém autorizou. A diretriz é explícita: «reaproveitar
//! a experiência com limites, não os poderes da metade de servidor».
//!
//! # O que ele ainda não é
//!
//! Um protótipo. Ele responde à pergunta da E1 — **que autoridade o código de
//! um MOD alcança?** — e não é o executor aprovado: falta medir custo, latência
//! de saída e o impacto na voz, e falta repetir a matriz de corridas da E2 com
//! ele. Enquanto isso, `ui/mods-runtime.js` continua sendo quem decide qual
//! executor sobe, e o de hoje continua sendo o Worker.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

// **A volta mora em `seele-motor`**, e o desktop a reexporta inteira: o
// `main.rs` e os testes abaixo continuam dizendo `executor::Fila`,
// `executor::ParaOFora` e `PRELUDIO`, como antes da extração.
pub(crate) use seele_motor::*;

/// O que entra no executor.
#[derive(Debug)]
pub(crate) enum ParaODentro {
    /// O código do MOD, uma vez.
    /// O fonte do MOD e a API que o manifesto dele declarou.
    Codigo {
        /// O que o pacote traz.
        fonte: String,
        /// O número do manifesto — ver `capacidades_da_api`.
        api: u32,
    },
    /// Uma resposta a um pedido que o MOD fez.
    Resposta(String),
    /// Pare.
    Encerrar,
    /// Conte o que você tem de pé.
    ///
    /// Existe porque «a tabela ficou vazia» não se observa de fora: ela é do
    /// anfitrião, e o MOD não a enxerga. Sem isto, provar que um intervalo
    /// cancelado saiu da tabela seria provar pela ausência de batidas — o que
    /// também acontece quando o motor morre.
    Diagnostico,
}

/// Um MOD de pé, com heap e contexto próprios.
///
/// A afinidade é de uma thread só: o `Runtime` do QuickJS não atravessa
/// threads, e o binding é explícito sobre isso. O dono é quem a criou, e o
/// descarte acontece nela — a diretriz proíbe o contrário: «não tentar matar
/// uma thread Rust à força».
pub(crate) struct ExecutorQuickJs {
    para_dentro: Sender<ParaODentro>,
    /// A fila de **entrada**, medida do mesmo jeito que a de saída.
    ///
    /// Ela não tinha teto, e a diretriz nomeia o buraco: `entregar` copiava e
    /// enfileirava o JSON sem limite. Um MOD que pede mais rápido do que o
    /// motor atende — ou uma janela que responde em rajada — fazia a memória
    /// crescer do lado de cá, onde nenhum teto de heap do motor conta.
    entrada: Arc<Fila>,
    /// **Some quando alguém escuta.** Ver [`Self::escutar`]: um canal tem um
    /// dono, e dois leitores dividiriam as mensagens em vez de vê-las.
    para_fora: Option<Receiver<ParaOFora>>,
    interrupcao: Arc<Interrupcao>,
    fila: Arc<Fila>,
    /// A thread do motor, para o `join` de quem esperou a confirmação.
    ///
    /// Lida só por [`Self::encerrou`], que é caminho de bancada: na janela
    /// ninguém espera, e a thread sai sozinha ao ver o canal fechado.
    #[cfg_attr(not(test), allow(dead_code))]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ExecutorQuickJs {
    /// Sobe um runtime numa thread própria.
    ///
    /// # Errors
    ///
    /// Falha quando a thread não sobe.
    pub(crate) fn novo(limites: Limites) -> std::io::Result<Self> {
        let (para_dentro, recebe) = std::sync::mpsc::channel::<ParaODentro>();
        let (manda, para_fora) = std::sync::mpsc::channel::<ParaOFora>();
        let interrupcao = Arc::new(Interrupcao::nova(limites));
        let dela = Arc::clone(&interrupcao);
        let fila = Arc::new(Fila::default());
        let dele = Arc::clone(&fila);
        let entrada = Arc::new(Fila::default());
        let dela_entrada = Arc::clone(&entrada);

        let thread = std::thread::Builder::new()
            .name("mod-quickjs".into())
            .spawn(move || rodar(&recebe, &manda, &dela, &dele, &dela_entrada, limites))?;

        Ok(Self {
            para_dentro,
            para_fora: Some(para_fora),
            interrupcao,
            fila,
            entrada,
            thread: Some(thread),
        })
    }

    /// Entrega o código do MOD.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn iniciar(&self, codigo: &str) -> Result<(), &'static str> {
        self.iniciar_com_api(codigo, seele_ffi::mods::MOD_API_VERSION)
    }

    /// Entrega o código do MOD, dizendo **que API ele declarou**.
    ///
    /// A versão não é decoração: o prelúdio monta `SeeleUI` a partir dela, e um
    /// pacote de API 3 recebe um `SeeleUI` sem `superficies` e sem
    /// `contribuicoes`. Aceitar a API 3 não é dar à API 3 o que a 4 tem — ver
    /// `seele_ffi::mods::capacidades_da_api`.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    pub(crate) fn iniciar_com_api(&self, codigo: &str, api: u32) -> Result<(), &'static str> {
        self.para_dentro
            .send(ParaODentro::Codigo {
                fonte: codigo.to_owned(),
                api,
            })
            .map_err(|_| "o executor não está de pé")
    }

    /// Entrega uma resposta a um pedido que o MOD fez.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn diagnostico(&self) -> Result<(), &'static str> {
        self.para_dentro
            .send(ParaODentro::Diagnostico)
            .map_err(|_| "o executor não está de pé")
    }

    /// Entrega uma resposta a um pedido que o MOD fez.
    ///
    /// # Errors
    ///
    /// Falha quando a thread já morreu.
    pub(crate) fn entregar(&self, json: &str) -> Result<(), &'static str> {
        // **O mesmo teto da saída, na entrada.** Sem ele, quem responde rápido
        // demais enche a memória de quem usa, e o motor nem chega a ver as
        // mensagens que já estão na fila.
        //
        // Recusado e dito: quem chamou recebe o motivo e decide — segurar,
        // descartar, ou avisar. Enfileirar sem teto decide por ele.
        if !self.entrada.cabe(json.len()) {
            return Err("a fila de entrada do executor está cheia");
        }
        let quantos = json.len();
        self.para_dentro
            .send(ParaODentro::Resposta(json.to_owned()))
            .map_err(|_| {
                self.entrada.tirar(quantos);
                "o executor não está de pé"
            })
    }

    /// A fila de entrada, para a bancada olhar.
    #[must_use]
    pub(crate) fn entrada(&self) -> &Fila {
        &self.entrada
    }

    /// O que o MOD postou, se já postou alguma coisa.
    ///
    /// **Só a bancada usa.** Na janela quem lê é a bomba de [`Self::escutar`],
    /// que não dorme entre mensagens; este caminho existe para os testes, onde
    /// esperar com prazo é mais simples que montar uma thread.
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) fn receber(&self, prazo: Duration) -> Option<ParaOFora> {
        let saiu = self.para_fora.as_ref()?.recv_timeout(prazo).ok();
        // A contabilidade é baixada **ao tirar**, e não ao entregar: é isto que
        // faz a fila voltar a aceitar assim que alguém a lê. Só mensagem ocupa
        // lugar; as outras variantes são avisos de tamanho fixo, menos
        // `Console`, que não reserva lugar nenhum: é registro, e não fala para
        // a janela. Cada linha vem cortada no teto de uma frase do registro, e
        // quem limita quantas vêm é o `BaldeDoConsole` da instância.
        if let Some(ParaOFora::Mensagem(json)) = &saiu {
            self.fila.tirar(json.len());
        }
        saiu
    }

    /// A fila de saída, para a bancada olhar.
    #[must_use]
    pub(crate) fn fila(&self) -> &Fila {
        &self.fila
    }

    /// Entrega o canal de saída a quem vai escutá-lo, uma vez só.
    ///
    /// **Um canal tem um dono.** Depois disto, [`Self::receber`] devolve
    /// `None` e [`Self::encerrou`] não tem como confirmar — quem escuta é que
    /// vê o [`ParaOFora::Parou`], e é por lá que a confirmação chega.
    ///
    /// Existe para a integração na janela: a fala de um MOD precisa virar
    /// evento assim que sai, e um laço de `receber` com prazo ou perderia
    /// tempo dormindo ou gastaria CPU acordando.
    ///
    /// A contabilidade da fila passa a ser de quem escuta: sem devolver o
    /// lugar, ela enche uma vez e não aceita mais nada.
    pub(crate) fn escutar(&mut self) -> Option<(Receiver<ParaOFora>, Arc<Fila>)> {
        self.para_fora
            .take()
            .map(|canal| (canal, Arc::clone(&self.fila)))
    }

    /// **Revoga agora e pede a parada.**
    ///
    /// A revogação é imediata e é a que importa para o §5 do contrato: a partir
    /// dela o motor para na primeira consulta, mesmo no meio de um laço que
    /// nunca terminaria. A parada física vem depois, e é [`Self::encerrou`] que
    /// a confirma.
    pub(crate) fn pedir_encerramento(&self) {
        self.interrupcao.revogar();
        let _ = self.para_dentro.send(ParaODentro::Encerrar);
    }

    /// Espera a thread confirmar que parou.
    ///
    /// **Diferente de revogar**, e a diretriz cobra a diferença: «até confirmar
    /// o descarte dos recursos, não anunciar que tudo foi limpo».
    ///
    /// # Errors
    ///
    /// Devolve o motivo quando a thread não confirmou dentro do prazo — o que é
    /// informação, e não um erro a esconder: um executor que não confirma é
    /// exatamente o que o contrato manda medir.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn encerrou(&mut self, prazo: Duration) -> Result<Duration, &'static str> {
        let relogio = Instant::now();
        self.pedir_encerramento();
        // Quem escuta é quem vê a confirmação. Dizer isso em vez de esperar
        // para sempre por uma mensagem que vai para outro lugar.
        if self.para_fora.is_none() {
            return Err("outro leitor tem o canal: a confirmação chega por lá");
        }
        // A confirmação vem pelo canal, e não pelo `join`: `join` bloqueia sem
        // prazo, e uma thread presa numa função nativa prenderia a saída junto.
        let Some(canal) = self.para_fora.as_ref() else {
            return Err("sem canal de saída");
        };
        let confirmou = loop {
            match canal.recv_timeout(prazo.saturating_sub(relogio.elapsed())) {
                Ok(ParaOFora::Parou) => break true,
                Ok(_) => {}
                Err(_) => break false,
            }
        };
        if !confirmou {
            return Err("o executor não confirmou a parada dentro do prazo");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Ok(relogio.elapsed())
    }
}

impl Drop for ExecutorQuickJs {
    fn drop(&mut self) {
        // O dono some sem ter pedido: revogar e avisar é tudo o que dá para
        // fazer sem bloquear um `drop`. A thread termina ao ver o canal fechado.
        self.interrupcao.revogar();
        let _ = self.para_dentro.send(ParaODentro::Encerrar);
    }
}

/// O laço da thread do motor: **só a espera**. A volta é do [`Motor`].
///
/// Espera até o pedido seguinte ou até o prazo que o motor dá
/// ([`Motor::proximo_prazo`]): sem temporizador e sem linha segurada a thread
/// dorme no canal, e com um dos dois ela dorme até a hora do mais próximo.
fn rodar(
    recebe: &Receiver<ParaODentro>,
    manda: &Sender<ParaOFora>,
    interrupcao: &Arc<Interrupcao>,
    fila: &Arc<Fila>,
    // Nomeada assim, e não `entrada`: o laço abaixo já chama de `entrada` a
    // mensagem que chegou, e dois nomes iguais para coisas diferentes na mesma
    // função é como um erro entra sem ninguém ver.
    fila_de_entrada: &Arc<Fila>,
    limites: Limites,
) {
    // O motor que não sobe já disse por quê e confirmou a parada.
    let Some(mut motor) = Motor::subir(manda, interrupcao, fila, limites) else {
        return;
    };
    loop {
        let entrada = match motor.proximo_prazo() {
            Some(quando) => match recebe.recv_timeout(quando) {
                Ok(entrada) => Some(entrada),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match recebe.recv() {
                Ok(entrada) => Some(entrada),
                Err(_) => break,
            },
        };
        if interrupcao.revogada() {
            break;
        }
        let Some(entrada) = entrada else {
            motor.vencer();
            continue;
        };
        match entrada {
            ParaODentro::Encerrar => break,
            ParaODentro::Diagnostico => motor.diagnostico(),
            ParaODentro::Codigo { fonte, api } => {
                motor.codigo(&fonte, seele_ffi::mods::capacidades_da_api(api));
            }
            ParaODentro::Resposta(json) => {
                // O lugar volta **ao tirar da fila**, que é quando ela deixa de
                // ocupar memória — e é o que a faz voltar a aceitar.
                fila_de_entrada.tirar(json.len());
                motor.resposta(&json);
            }
        }
    }
    motor.parar();
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um executor com os tetos do produto.
    fn executor() -> ExecutorQuickJs {
        ExecutorQuickJs::novo(Limites::default()).expect("a thread do motor")
    }

    /// Um executor em que **só um** mecanismo pode parar uma volta.
    ///
    /// Os outros dois ficam altos a ponto de não alcançarem, e é isso que faz
    /// cada teste abaixo medir o que ele diz medir.
    ///
    /// A primeira versão destes testes usava os tetos do produto, e dois deles
    /// **passavam com a revogação arrancada** — o teto de trabalho os salvava.
    /// Um guarda que passa sem o conserto não guarda nada, e a reversão foi o
    /// que mostrou isso.
    fn so_com(consultas: usize, prazo: Duration) -> ExecutorQuickJs {
        ExecutorQuickJs::novo(Limites {
            memoria: TETO_DE_MEMORIA,
            consultas_por_volta: consultas,
            prazo_por_volta: prazo,
        })
        .expect("a thread do motor")
    }

    /// Alto a ponto de não chegar dentro de um teste.
    const SEM_TETO_DE_TRABALHO: usize = usize::MAX;
    /// Longo a ponto de não chegar dentro de um teste.
    const SEM_PRAZO: Duration = Duration::from_secs(3600);

    fn uma_mensagem(executor: &ExecutorQuickJs) -> String {
        match executor.receber(Duration::from_secs(5)) {
            Some(ParaOFora::Mensagem(json)) => json,
            Some(outro) => panic!("veio {outro:?} em vez de mensagem"),
            None => panic!("o executor não respondeu"),
        }
    }

    /// As linhas de `console` que chegam antes da mensagem `fim`, em ordem.
    fn linhas_de_console_ate(
        executor: &ExecutorQuickJs,
        fim: &str,
    ) -> Vec<(NivelDoConsole, String)> {
        let mut linhas = Vec::new();
        loop {
            match executor.receber(Duration::from_secs(5)) {
                Some(ParaOFora::Console { nivel, texto, .. }) => linhas.push((nivel, texto)),
                Some(ParaOFora::Mensagem(json)) if json == fim => return linhas,
                outro => panic!("esperava linhas de console e depois «{fim}», e veio {outro:?}"),
            }
        }
    }

    /// Um campo de um JSON, ou `null` quando ele falta.
    fn campo<'a>(valor: &'a serde_json::Value, nome: &str) -> &'a serde_json::Value {
        valor.get(nome).unwrap_or(&serde_json::Value::Null)
    }

    /// O que um pacote vê de `SeeleUI` e `SeeleMods`, numa API escolhida.
    fn o_que_a_api_entrega(api: u32) -> serde_json::Value {
        let executor = executor();
        executor
            .iniciar_com_api(
                "seele.postar(JSON.stringify({\
                   contribuicoes: typeof SeeleUI.contribuicoes,\
                   superficies: typeof SeeleUI.superficies,\
                   enviar: typeof SeeleUI.enviar,\
                   tema: typeof SeeleUI.tema,\
                   snapshot: typeof SeeleMods.snapshot,\
                   retrato: typeof SeeleUI.retrato,\
                   regiao: typeof SeeleUI.regiao,\
                   cartoes: typeof SeeleUI.cartoes,\
                   capacidades: SeeleUI.capacidades(),\
                 }));",
                api,
            )
            .expect("código");
        serde_json::from_str(&uma_mensagem(&executor)).expect("o MOD posta JSON")
    }

    /// **A API 6 chega ao MOD com as capacidades dela** (tarefa 3 do plano da
    /// F3), e sem as que saíram.
    ///
    /// `regiao` e `cartoes` saem da 6 (decisão do dono de 08/10), e saem como
    /// o prelúdio promete para tudo que uma API não tem: **ausentes**, e não
    /// recusados em tempo de execução. Até aqui os dois eram montados em toda
    /// API, e a recusa só vinha quando a mensagem chegava à janela — um MOD da
    /// 6 descobriria a ausência num `catch`, em produção. A API 5 continua com
    /// os dois, e é a outra metade da prova.
    ///
    /// **E o retrato no lugar do `snapshot`** (tarefa 4): a 6 lê
    /// `SeeleUI.retrato()`, projetado por lista branca, e não tem
    /// `SeeleMods.snapshot`; a 5 tem o `snapshot` e não tem o retrato.
    #[test]
    fn um_pacote_da_api_6_recebe_as_capacidades_dela_e_nao_as_que_sairam() {
        let seis = o_que_a_api_entrega(6);
        for (nome, tipo) in [
            ("contribuicoes", "object"),
            ("superficies", "object"),
            ("enviar", "function"),
            ("tema", "function"),
            ("retrato", "function"),
        ] {
            assert_eq!(campo(&seis, nome), tipo, "a API 6 sem «{nome}»: {seis}");
        }
        for saiu in ["regiao", "cartoes", "snapshot"] {
            assert_eq!(
                campo(&seis, saiu),
                "undefined",
                "«{saiu}» saiu da API 6 e ainda é montado (no `SeeleUI` ou no `SeeleMods`): {seis}"
            );
        }
        assert_eq!(
            campo(&seis, "capacidades"),
            &serde_json::json!(seele_ffi::mods::capacidades_da_api(6)),
            "o MOD da API 6 lê outras capacidades que as do núcleo"
        );

        let cinco = o_que_a_api_entrega(5);
        for continua in ["regiao", "cartoes", "snapshot"] {
            assert_eq!(
                campo(&cinco, continua),
                "function",
                "a API 5 perdeu «{continua}», que ela tem: {cinco}"
            );
        }
        assert_eq!(
            campo(&cinco, "retrato"),
            "undefined",
            "a API 5 ganhou o retrato da 6: {cinco}"
        );
    }

    /// **A recusa chega ao MOD com os campos dela** (o I do PERFIS; rascunho da
    /// API 6, §7.1). O prelúdio rejeitava com `new Error(m.erro)`, e o
    /// `detectado: 'heic'` de um `tipo-recusado` não atravessava: o MOD não tinha
    /// como dizer «HEIC não é aceito». Agora o `Error` leva o nome em `message` e
    /// em `erro`, e os outros campos da resposta como propriedades — sem o `ok`,
    /// o número do pedido e o tipo da mensagem, que são da ponte.
    #[test]
    fn a_recusa_chega_ao_mod_com_os_campos_dela() {
        let executor = executor();
        executor
            .iniciar(
                "SeeleMods.request('a/b', 0, {}).catch((e) => seele.postar(JSON.stringify({\
                   message: e.message, erro: e.erro, detectado: e.detectado,\
                   ok: 'ok' in e, n: 'n' in e, tipo: 'tipo' in e,\
                 })));",
            )
            .expect("código");
        let pedido: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("o pedido é JSON");
        let n = campo(&pedido, "n").as_u64().expect("o pedido tem número");
        executor
            .entregar(&format!(
                r#"{{"tipo":"resposta","n":{n},"ok":false,"erro":"tipo-recusado","detectado":"heic"}}"#
            ))
            .expect("entregar");
        let visto: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("o MOD posta JSON");
        assert_eq!(
            visto,
            serde_json::json!({
                "message": "tipo-recusado", "erro": "tipo-recusado", "detectado": "heic",
                "ok": false, "n": false, "tipo": false,
            }),
            "a recusa não chegou ao MOD com os campos dela"
        );
    }

    /// **A superfície se monta por partes** (rascunho da API 6, §12; tarefa 7
    /// da F3, Step 5c): o punho que `criar` devolve tem `atualizar(chave,
    /// arvore)`, que manda `superficie-atualizar` com a superfície, a chave e a
    /// subárvore, numa mensagem própria — com o teto de 12 KiB de cada uma, e
    /// não o da ficha inteira.
    #[test]
    fn a_superficie_se_monta_por_partes() {
        let executor = executor();
        executor
            .iniciar_com_api(
                "SeeleUI.superficies.criar({ id: 'ficha', tipo: 'pagina' })\
                   .then((s) => s.atualizar('inventario', { forma: 'caixa', chave: 'inventario', dentro: ['x'] }));",
                6,
            )
            .expect("código");
        let criar: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("o pedido é JSON");
        assert_eq!(campo(&criar, "tipo"), "superficie-criar", "{criar}");
        let n = campo(&criar, "n").as_u64().expect("o pedido tem número");
        executor
            .entregar(&format!(
                r#"{{"tipo":"resposta","n":{n},"ok":true,"valor":{{"superficie":"ficha","reaproveitada":false}}}}"#
            ))
            .expect("entregar");
        let atualizar: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("o pedido é JSON");
        assert_eq!(
            (
                campo(&atualizar, "tipo"),
                campo(&atualizar, "superficie"),
                campo(&atualizar, "chave"),
                campo(&atualizar, "arvore"),
            ),
            (
                &serde_json::json!("superficie-atualizar"),
                &serde_json::json!("ficha"),
                &serde_json::json!("inventario"),
                &serde_json::json!({ "forma": "caixa", "chave": "inventario", "dentro": ["x"] }),
            ),
            "o punho da superfície não manda a parte: {atualizar}"
        );
    }

    /// **A recusa do servidor volta aos campos** (rascunho da API 6, §5; tarefa
    /// 7 da F3, Step 6b): o punho da superfície tem `formulario.erros(mapa)`,
    /// que manda `formulario-erros` com a superfície e o mapa de chave a frase.
    #[test]
    fn a_recusa_do_servidor_volta_aos_campos_do_formulario() {
        let executor = executor();
        executor
            .iniciar_com_api(
                "SeeleUI.superficies.criar({ id: 'estilo', tipo: 'dialogo' })\
                   .then((s) => s.formulario.erros({ texto: 'claro demais', fundo: 'claro demais' }));",
                6,
            )
            .expect("código");
        let criar: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("o pedido é JSON");
        let n = campo(&criar, "n").as_u64().expect("o pedido tem número");
        executor
            .entregar(&format!(
                r#"{{"tipo":"resposta","n":{n},"ok":true,"valor":{{"superficie":"estilo","reaproveitada":false}}}}"#
            ))
            .expect("entregar");
        let erros: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("o pedido é JSON");
        assert_eq!(
            (
                campo(&erros, "tipo"),
                campo(&erros, "superficie"),
                campo(&erros, "erros")
            ),
            (
                &serde_json::json!("formulario-erros"),
                &serde_json::json!("estilo"),
                &serde_json::json!({ "texto": "claro demais", "fundo": "claro demais" }),
            ),
            "o punho da superfície não manda os erros do formulário: {erros}"
        );
    }

    /// **A prévia do tema sai da janela com a superfície dona** (rascunho da
    /// API 6, §9, B4; tarefa 17g da F3): `SeeleUI.tema(valores, { previa: true,
    /// superficie })` manda `previa` e `superficie` no pedido `tema`.
    #[test]
    fn a_previa_do_tema_vai_com_a_superficie_dona() {
        let executor = executor();
        executor
            .iniciar_com_api(
                "SeeleUI.tema({ acento: '#c9a227' }, { previa: true, superficie: 'editor' });",
                6,
            )
            .expect("código");
        let pedido: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("o pedido é JSON");
        assert_eq!(
            (
                campo(&pedido, "tipo"),
                campo(&pedido, "previa"),
                campo(&pedido, "superficie"),
                campo(&pedido, "valores"),
            ),
            (
                &serde_json::json!("tema"),
                &serde_json::json!(true),
                &serde_json::json!("editor"),
                &serde_json::json!({ "acento": "#c9a227" }),
            ),
            "a prévia não saiu com a superfície dona: {pedido}"
        );
    }

    /// **Um MOD que chama `console.warn` não lança, e a linha sai do motor.**
    ///
    /// O executor nativo não definia `console`, e o `console.warn` que o autor
    /// escreve justamente no caminho de erro virava `ReferenceError` — a falha
    /// que ele queria contar virava outra, e a volta inteira saía como «o MOD
    /// lançou». Se o `console` sair do prelúdio, este teste reprova com o nome
    /// do erro na mensagem.
    #[test]
    fn um_mod_que_chama_console_warn_nao_lanca_e_a_linha_sai() {
        let executor = executor();
        executor
            .iniciar(
                "try {\
                   console.warn('avatar recusado:', { bytes: 12 }, 3, undefined, null);\
                   seele.postar('fim');\
                 } catch (erro) {\
                   seele.postar('lançou ' + erro.name + ': ' + erro.message);\
                 }",
            )
            .expect("código");
        let mut linhas = Vec::new();
        let depois = loop {
            match executor.receber(Duration::from_secs(5)) {
                Some(ParaOFora::Console { nivel, texto, .. }) => linhas.push((nivel, texto)),
                Some(ParaOFora::Mensagem(json)) => break json,
                outro => panic!("veio {outro:?} em vez da linha de console e da mensagem"),
            }
        };
        assert_eq!(
            depois, "fim",
            "o `console` do MOD lançou em vez de escrever: {depois}"
        );
        assert_eq!(
            linhas,
            vec![(
                NivelDoConsole::Aviso,
                "avatar recusado: {\"bytes\":12} 3 undefined null".to_owned()
            )],
            "a linha do console não chegou como o MOD a escreveu"
        );
    }

    /// **Trocar o `seele.console` ou o `console.error` não cala o erro que o
    /// produto diz do MOD.**
    ///
    /// O prelúdio lia `seele.console` na hora de escrever, e o `seele` é um
    /// objeto que o MOD alcança: um `seele.console = () => {}` no topo calava a
    /// linha de um temporizador que lança, e o comentário do prelúdio dizia
    /// que o MOD não calava o que o produto diz dele.
    #[test]
    fn trocar_o_console_do_anfitriao_nao_cala_o_erro_de_um_temporizador() {
        let executor = executor();
        executor
            .iniciar(
                "seele.console = () => {};\
                 console.error = () => {};\
                 setTimeout(() => { const a = null; a.x; }, 0);",
            )
            .expect("código");
        let mut linhas = Vec::new();
        let ate = Instant::now() + Duration::from_secs(5);
        loop {
            match executor.receber(Duration::from_millis(50)) {
                Some(ParaOFora::Console { nivel, texto, .. }) => linhas.push((nivel, texto)),
                Some(ParaOFora::Mensagem(json)) if json.contains("erro-no-relogio") => break,
                Some(_) | None => assert!(
                    Instant::now() < ate,
                    "o temporizador que lança não chegou à janela em cinco segundos: {linhas:?}"
                ),
            }
        }
        assert!(
            linhas.iter().any(|(nivel, texto)| {
                *nivel == NivelDoConsole::Erro
                    && texto.contains("erro num temporizador")
                    && texto.contains("TypeError")
            }),
            "um MOD que troca o `seele.console` calou a linha do erro que ele não pegou, e o \
             seele.log fica sem o porquê: {linhas:?}"
        );
    }

    /// **Um `NaN` chega como `NaN`, e um `Map` chega com o que tem.**
    ///
    /// O `JSON.stringify` diz `null` para `NaN` e `±Infinity`, `{}` para `Map`
    /// e `Set`, e lança num `bigint` aninhado. `NaN` é justamente o valor de
    /// um defeito de conta ou de layout: o registro que o diz `null` manda o
    /// autor procurar um valor que ele não tem — pior que o silêncio. Aninhado,
    /// o que o JSON não sabe dizer sai como texto, entre aspas.
    #[test]
    fn um_nan_ou_um_map_chega_como_e_e_nao_como_null_ou_vazio() {
        let executor = executor();
        executor
            .iniciar(
                "console.warn('largura:', NaN, Infinity, -Infinity);\
                 console.warn({ largura: NaN, altura: -Infinity, dentro: [NaN] });\
                 console.warn(new Map([['a', 1]]), new Set([1, 2]));\
                 console.warn({ mapa: new Map([[1, 2]]), conjunto: new Set(['x']) });\
                 console.warn({ grande: 10n, faltou: undefined }, [undefined]);\
                 seele.postar('fim');",
            )
            .expect("código");
        let textos: Vec<String> = linhas_de_console_ate(&executor, "fim")
            .into_iter()
            .map(|(_, texto)| texto)
            .collect();
        assert_eq!(
            textos,
            vec![
                "largura: NaN Infinity -Infinity",
                "{\"largura\":\"NaN\",\"altura\":\"-Infinity\",\"dentro\":[\"NaN\"]}",
                "[[\"a\",1]] [1,2]",
                "{\"mapa\":[[1,2]],\"conjunto\":[\"x\"]}",
                "{\"grande\":\"10\",\"faltou\":\"undefined\"} [\"undefined\"]",
            ],
            "o console disse um valor que o MOD não tem (`NaN` como `null`, um `Map` como `{{}}`), \
             e o registro manda o autor procurar o que não existe"
        );
    }

    /// **Um `Error`, uma função e um símbolo aninhados dizem o que são.**
    ///
    /// O `JSON.stringify` diz `{}` para um `Error` (o nome e a mensagem não são
    /// enumeráveis) e some com a função e o símbolo de um objeto: a linha de
    /// `console.log({ erro })` saía `{"erro":{}}`, e quem a lê não sabe nem que
    /// houve um erro (T1).
    #[test]
    fn um_error_uma_funcao_e_um_simbolo_aninhados_dizem_o_que_sao() {
        let executor = executor();
        executor
            .iniciar(
                "console.log({ e: new Error('x'), f() {}, s: Symbol('s') });\
                 seele.postar('fim');",
            )
            .expect("código");
        let linhas = linhas_de_console_ate(&executor, "fim");
        let [(_, texto)] = linhas.as_slice() else {
            panic!("esperava uma linha de console, vieram {linhas:?}");
        };
        for (o_que, esperado) in [
            ("um `Error`", "Error: x"),
            ("uma função", "[função f]"),
            ("um símbolo", "Symbol(s)"),
        ] {
            assert!(
                texto.contains(esperado),
                "{o_que} aninhado sumiu da linha do console, ou saiu como `{{}}`: esperava \
                 «{esperado}» em {texto}"
            );
        }
    }

    /// **Um objeto cíclico diz onde é cíclico**, e não `[object Object]`.
    ///
    /// O `JSON.stringify` lança num ciclo, e o recuo era `String(valor)`: um
    /// objeto de estado com uma referência para o pai saía `[object Object]`,
    /// sem nenhum dos campos que o autor queria ver. Uma segunda passada troca
    /// por «[cíclico]» só o que é ancestral do valor de agora — e não o que já
    /// apareceu antes noutro ramo: a mesma referência em dois campos é
    /// repetição, e não ciclo (T1).
    #[test]
    fn um_objeto_ciclico_diz_onde_e_ciclico_e_nao_vira_object_object() {
        let executor = executor();
        executor
            .iniciar(
                "const o = { nome: 'raiz' }; o.o = o; console.log(o);\
                 const r = { a: 1 }; const p = { x: r, y: r }; p.p = p; console.log(p);\
                 seele.postar('fim');",
            )
            .expect("código");
        let linhas = linhas_de_console_ate(&executor, "fim");
        let [(_, ciclico), (_, repetido)] = linhas.as_slice() else {
            panic!("esperava duas linhas de console, vieram {linhas:?}");
        };
        assert!(
            ciclico.contains("cíclico")
                && ciclico.contains("raiz")
                && !ciclico.contains("[object Object]"),
            "um objeto cíclico saiu sem os campos dele, ou sem dizer onde é cíclico: {ciclico}"
        );
        assert_eq!(
            repetido, "{\"x\":{\"a\":1},\"y\":{\"a\":1},\"p\":\"[cíclico]\"}",
            "a segunda passada marcou como cíclica uma referência só repetida, ou não marcou o \
             ciclo: o registro diz que há ciclo onde há só repetição"
        );
    }

    /// **Um grafo denso no `console` não estoura a volta.**
    ///
    /// Marcar só os ancestrais faz o `JSON.stringify` percorrer todos os
    /// caminhos simples do grafo, e eles crescem como (k−1)!. Medido na
    /// revisão do lote D: com dez nós ligados todos a todos, a segunda passada
    /// estourava o prazo da volta — o produto mandava `Falhou("o MOD
    /// lançou")`, e o código do MOD depois do `console.log` não rodava. Um DAG
    /// de vinte nós, cada um ligado a todos os de depois, pesa o mesmo na
    /// primeira passada, sem ciclo nenhum: 2¹⁹ caminhos. Antes do lote, o
    /// ciclo saía `[object Object]` na hora; o DAG segurava a thread do motor
    /// por 1,13 s e também saía `[object Object]`, e o MOD seguia sem `Falhou`
    /// (medido em `0f94be66` na re-revisão). Com as duas passadas deste lote e
    /// sem o orçamento na primeira, ele derruba a volta como o grafo completo:
    /// `Falhou("o MOD lançou")`, e o `fim` não chega (a reversão da rodada 1).
    ///
    /// Cada passada tem um orçamento de objetos ([`PRELUDIO`],
    /// `OBJETOS_POR_PASSADA`), e o que passa dele sai `[…]`.
    #[test]
    fn um_grafo_denso_no_console_nao_estoura_a_volta() {
        let executor = executor();
        executor
            .iniciar(
                "const completo = Array.from({ length: 10 }, (_, i) => ({ i }));\
                 for (const a of completo) for (const b of completo) if (a !== b) a['n' + b.i] = b;\
                 console.log(completo[0]);\
                 const dag = Array.from({ length: 20 }, (_, i) => ({ i }));\
                 for (let a = 0; a < 20; a++) for (let b = a + 1; b < 20; b++) dag[a]['n' + b] = dag[b];\
                 console.log(dag[0]);\
                 seele.postar('fim');",
            )
            .expect("código");
        let mut linhas = Vec::new();
        let inicio = Instant::now();
        loop {
            match executor.receber(Duration::from_secs(5)) {
                Some(ParaOFora::Console { texto, .. }) => linhas.push(texto),
                Some(ParaOFora::Mensagem(fim)) if fim == "fim" => break,
                outra => panic!(
                    "um `console.log` de um grafo denso não deixou o MOD chegar ao fim da volta \
                     ({:?} depois de subir, com {} linhas): veio {outra:?}",
                    inicio.elapsed(),
                    linhas.len()
                ),
            }
        }
        let [completo, dag] = linhas.as_slice() else {
            panic!("esperava duas linhas de console, vieram {linhas:?}");
        };
        assert!(
            completo.starts_with("{\"i\":0,\"n1\":{\"i\":1,\"n0\":\"[cíclico]\""),
            "o grafo completo saiu sem dizer o que tem no começo, ou sem o ciclo onde ele está: \
             {}",
            completo.chars().take(120).collect::<String>()
        );
        assert!(
            dag.starts_with("{\"i\":0,\"n1\":{\"i\":1,\"n2\":{\"i\":2,"),
            "o DAG saiu sem dizer o que tem no começo: {}",
            dag.chars().take(120).collect::<String>()
        );
    }

    /// **Uma parte que não vira texto não derruba a linha inteira.**
    ///
    /// Um objeto de protótipo nulo com um getter que lança não vira JSON (o
    /// getter lança nas duas passadas) nem texto (não tem `toString`): o
    /// `JSON.stringify` lança, o `String` do recuo também lança, e o `catch` de
    /// `escrever` engolia a linha toda — com o contexto que o autor escreveu
    /// antes do objeto. A parte que não vira texto sai como um recuo que diz
    /// isso, e o resto da linha chega.
    ///
    /// O exemplo era um objeto cíclico de protótipo nulo, até o ciclo virar
    /// «[cíclico]» (T1): esse agora vira texto.
    #[test]
    fn uma_parte_que_nao_vira_texto_nao_derruba_a_linha_do_console() {
        let executor = executor();
        executor
            .iniciar(
                "const c = Object.create(null);\
                 Object.defineProperty(c, 'x', { enumerable: true, get() { throw new Error('não'); } });\
                 console.warn('contexto importante:', c);\
                 seele.postar('fim');",
            )
            .expect("código");
        assert_eq!(
            linhas_de_console_ate(&executor, "fim"),
            vec![(
                NivelDoConsole::Aviso,
                "contexto importante: [valor que não virou texto]".to_owned()
            )],
            "uma parte que não virou texto derrubou a linha inteira do console, e o contexto que \
             o autor escreveu antes dela sumiu calado"
        );
    }

    /// **Cada método tem o seu nível**, e um `Error` chega com nome, mensagem
    /// e pilha — a pilha numa linha abaixo, com a quebra de verdade, que a
    /// bomba escapa ao escrever no `seele.log`.
    #[test]
    fn cada_metodo_do_console_escreve_no_seu_nivel() {
        let executor = executor();
        executor
            .iniciar(
                "console.debug('d'); console.log('l'); console.info('i');\
                 console.warn('w'); console.error(new Error('quebrou'));\
                 seele.postar('fim');",
            )
            .expect("código");
        let linhas = linhas_de_console_ate(&executor, "fim");
        let niveis: Vec<NivelDoConsole> = linhas.iter().map(|(nivel, _)| *nivel).collect();
        assert_eq!(
            niveis,
            vec![
                NivelDoConsole::Depuracao,
                NivelDoConsole::Informacao,
                NivelDoConsole::Informacao,
                NivelDoConsole::Aviso,
                NivelDoConsole::Erro,
            ],
            "um método do console caiu no nível de outro, e o filtro do seele.log o trataria errado"
        );
        let erro = &linhas.last().expect("a linha do erro").1;
        assert!(
            erro.starts_with("Error: quebrou"),
            "um `Error` passado ao console perdeu o nome e a mensagem: {erro}"
        );
        assert!(
            erro.contains("\n    at "),
            "um `Error` passado ao console perdeu a pilha, que diz onde no MOD ele nasceu: {erro:?}"
        );
    }

    /// **Os outros métodos do `console` do navegador também existem**, e
    /// nenhum lança.
    ///
    /// Quem escreve MOD vem do navegador, e lá `table`, `group` e `assert`
    /// existem. Um `console` só com os cinco que os MODs publicados usam
    /// adiaria o mesmo defeito de um passo: `console.table(…)` viraria
    /// `TypeError`, e a volta inteira sairia de novo como «o MOD lançou».
    ///
    /// Os que escrevem escrevem no nível mais próximo (`dir` e `table` em
    /// informação, `trace` em depuração, `assert` em erro e só quando a
    /// condição é falsa). Os que organizam a saída — `group*`, `time*`,
    /// `count*` e `clear` — não têm o que organizar num registro de linhas, e
    /// não fazem nada.
    #[test]
    fn os_outros_metodos_do_console_do_navegador_existem_e_nao_lancam() {
        let executor = executor();
        executor
            .iniciar(
                "try {\
                   console.table({}); console.dir({ a: 1 }); console.trace('t');\
                   console.group(); console.groupCollapsed('g'); console.groupEnd();\
                   console.time(); console.timeLog(); console.timeEnd();\
                   console.count(); console.countReset(); console.clear();\
                   console.assert(true, 'não sai'); console.assert(false, 'x');\
                   seele.postar('fim');\
                 } catch (erro) {\
                   seele.postar('lançou ' + erro.name + ': ' + erro.message);\
                 }",
            )
            .expect("código");
        let mut linhas = Vec::new();
        let depois = loop {
            match executor.receber(Duration::from_secs(5)) {
                Some(ParaOFora::Console { nivel, texto, .. }) => linhas.push((nivel, texto)),
                Some(ParaOFora::Mensagem(json)) => break json,
                outro => panic!("veio {outro:?} em vez das linhas de console e da mensagem"),
            }
        };
        assert_eq!(
            depois, "fim",
            "um método do `console` do navegador faltou, e o MOD que o chamou lançou: {depois}"
        );
        assert_eq!(
            linhas,
            vec![
                (NivelDoConsole::Informacao, "{}".to_owned()),
                (NivelDoConsole::Informacao, "{\"a\":1}".to_owned()),
                (NivelDoConsole::Depuracao, "t".to_owned()),
                (NivelDoConsole::Erro, "Assertion failed: x".to_owned()),
            ],
            "um método do console do navegador escreveu no nível errado, escreveu o que não \
             devia (um `assert` verdadeiro, um `group`) ou não escreveu o que devia"
        );
    }

    /// **Uma linha grande demais é cortada no teto de uma frase do registro.**
    ///
    /// O mesmo teto da janela, [`crate::TETO_DA_FRASE_NO_REGISTRO`]: as duas
    /// portas escrevem no mesmo `seele.log`, e a linha cortada diz que foi.
    #[test]
    fn uma_linha_de_console_grande_demais_e_cortada_e_diz_que_foi() {
        let executor = executor();
        executor
            .iniciar("console.log('a' + 'é'.repeat(1000)); seele.postar('fim');")
            .expect("código");
        let linhas = linhas_de_console_ate(&executor, "fim");
        let [(nivel, texto)] = linhas.as_slice() else {
            panic!("esperava uma linha de console, vieram {linhas:?}");
        };
        assert_eq!(
            *nivel,
            NivelDoConsole::Informacao,
            "o `console.log` caiu noutro nível"
        );
        let (corpo, aviso) = texto
            .split_once(" […cortado: ")
            .unwrap_or_else(|| panic!("a linha grande chegou sem dizer que foi cortada: {texto}"));
        assert_eq!(
            corpo.chars().count(),
            crate::TETO_DA_FRASE_NO_REGISTRO,
            "a linha passou do teto de uma frase do registro"
        );
        assert!(
            corpo.starts_with('a') && corpo.chars().skip(1).all(|c| c == 'é'),
            "o corte trocou ou partiu caracteres: {corpo}"
        );
        assert_eq!(
            aviso, "1001 caracteres]",
            "o corte não diz quanto a linha tinha"
        );
    }

    /// **A linha é cortada pelo tamanho que ela tem no registro**, e não pelo
    /// número de caracteres.
    ///
    /// O `seele.log` recebe o texto em `?` (`texto = ?texto`), e o `Debug` de
    /// um `str` escapa todo caractere que não se imprime: um `\u{200b}` são
    /// oito caracteres no arquivo. Cortada em 512 caracteres, uma linha deles
    /// ocupava 4096 no registro — oito vezes o teto que o corte promete, e o
    /// teto de vazão do `console` é contado sobre ele.
    ///
    /// O apóstrofo é o caso contrário: o `Debug` de um `char` o escapa, e o de
    /// um `str` não. Contado como escapado, uma linha deles cortaria na
    /// metade do que cabe.
    #[test]
    fn uma_linha_de_console_e_cortada_pelo_tamanho_que_tem_no_registro() {
        let executor = executor();
        executor
            .iniciar(
                r"console.log('\u{200b}'.repeat(600));
                  console.log(`'`.repeat(600));
                  seele.postar('fim');",
            )
            .expect("código");
        let linhas = linhas_de_console_ate(&executor, "fim");
        let [(_, invisiveis), (_, apostrofos)] = linhas.as_slice() else {
            panic!("esperava duas linhas de console, vieram {linhas:?}");
        };
        // Quantos caracteres o texto ocupa no registro: o `Debug` dele, que é
        // o que `texto = ?texto` escreve, sem as duas aspas.
        let no_registro = |texto: &str| format!("{texto:?}").chars().count() - 2;
        let teto = crate::TETO_DA_FRASE_NO_REGISTRO;
        for (texto, caractere) in [(invisiveis, '\u{200b}'), (apostrofos, '\'')] {
            let (corpo, aviso) = texto.split_once(" […cortado: ").unwrap_or_else(|| {
                panic!("a linha de {caractere:?} chegou sem dizer que foi cortada: {texto:?}")
            });
            assert_eq!(
                aviso, "600 caracteres]",
                "o corte da linha de {caractere:?} não diz quanto ela tinha"
            );
            assert!(
                !corpo.is_empty() && corpo.chars().all(|c| c == caractere),
                "o corte trocou ou partiu caracteres da linha de {caractere:?}: {corpo:?}"
            );
            let sufixo = format!(" […cortado: {aviso}");
            assert!(
                no_registro(texto) <= teto + sufixo.chars().count(),
                "a linha de {caractere:?} cortada ocupa {} caracteres no registro, e o teto é \
                 {teto} mais o aviso do corte: o teto de vazão do console, contado sobre ele, \
                 deixa de valer",
                no_registro(texto)
            );
            assert!(
                no_registro(&format!("{corpo}{caractere}")) > teto,
                "o corte da linha de {caractere:?} tirou mais do que precisava: cabia mais um, e \
                 o registro perde o que o MOD escreveu sem razão ({} de {teto})",
                no_registro(corpo)
            );
        }
    }

    /// **Um MOD em laço no `console` não passa da rajada, e nada some sem ser
    /// contado.**
    ///
    /// Mil linhas de uma vez: passam as da rajada, e a primeira linha que passa
    /// depois diz quantas ficaram para trás. A soma das duas coisas é mil — é
    /// essa conta que prova que o balde segura **e** conta.
    #[test]
    fn um_mod_em_laco_no_console_nao_passa_da_rajada_e_conta_o_resto() {
        let executor = executor();
        executor
            .iniciar(
                "for (let i = 0; i < 1000; i++) console.log('linha ' + i);\
                 setTimeout(() => console.warn('depois'), 400);",
            )
            .expect("código");
        let mut passaram_do_laco = 0_usize;
        let mut contadas = 0_usize;
        let prazo = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                Instant::now() < prazo,
                "a linha de depois do laço não chegou"
            );
            match executor.receber(Duration::from_secs(5)) {
                Some(ParaOFora::Console {
                    texto, suprimidas, ..
                }) => {
                    contadas += suprimidas as usize;
                    if texto == "depois" {
                        break;
                    }
                    passaram_do_laco += 1;
                }
                outro => panic!("veio {outro:?} no meio das linhas de console"),
            }
        }
        let rajada = RAJADA_DO_CONSOLE as usize;
        assert!(
            (rajada..=rajada + 2).contains(&passaram_do_laco),
            "passaram {passaram_do_laco} linhas de um laço de mil, e a rajada é {rajada}: o balde não segurou"
        );
        assert_eq!(
            passaram_do_laco + contadas,
            1000,
            "linhas sumiram sem ser contadas: {passaram_do_laco} passaram e {contadas} foram contadas"
        );
    }

    /// **O balde diz quanto falta para sobrar uma ficha — e só enquanto segura
    /// alguma linha.**
    ///
    /// É o prazo que a espera do laço de [`rodar`] usa para acordar e dizer a
    /// conta sem esperar o MOD parar. Sem linha segurada, `None`: a thread não
    /// acorda por nada. Com uma, o prazo é o de **duas** fichas: a conta gasta
    /// uma, e a outra fica com o MOD. E é exato, ao microssegundo: é o
    /// instante em que o fim da volta que a espera acordou — uma volta sem
    /// linha — acha as duas. Acordada no prazo, a conta sai e a linha seguinte
    /// do MOD ainda passa; um microssegundo antes, a volta não diria a conta.
    #[test]
    fn o_balde_diz_quanto_falta_para_sobrar_uma_ficha() {
        let inicio = Instant::now();
        let uma_ficha = Duration::from_millis(1000 / u64::from(LINHAS_DE_CONSOLE_POR_SEGUNDO));
        // Um balde seco por uma rajada inteira, uma linha segurada depois, e a
        // volta da rajada terminada.
        let seco_com_uma_segurada = || {
            let mut balde = BaldeDoConsole::novo(inicio);
            assert_eq!(
                balde.ate_sobrar_uma_ficha(inicio),
                None,
                "um balde cheio, sem nada segurado, pediu para acordar a thread do motor"
            );
            for _ in 0..RAJADA_DO_CONSOLE {
                assert!(balde.admitir(inicio).is_some(), "a rajada não passou");
            }
            assert_eq!(
                balde.ate_sobrar_uma_ficha(inicio),
                None,
                "um balde seco, sem nada segurado, pediu para acordar a thread do motor"
            );
            assert_eq!(balde.admitir(inicio), None, "o balde passou da rajada");
            assert_eq!(
                balde.pendentes_se_couber(inicio),
                None,
                "a conta saiu sem ficha no fim da volta da rajada"
            );
            balde
        };
        let balde = seco_com_uma_segurada();
        assert_eq!(
            balde.ate_sobrar_uma_ficha(inicio),
            Some(uma_ficha * 2),
            "com uma linha segurada e o balde seco, o prazo não é o de duas recargas: a conta \
             gastaria a única ficha, e a linha seguinte do MOD acharia o balde seco"
        );
        let um_pouco_depois = inicio + Duration::from_millis(100);
        assert_eq!(
            balde.ate_sobrar_uma_ficha(um_pouco_depois),
            Some(uma_ficha * 2 - Duration::from_millis(100)),
            "o tempo que passou não descontou do que falta para sobrar uma ficha"
        );
        let prazo = inicio + uma_ficha * 2;

        let mut cedo = seco_com_uma_segurada();
        assert_eq!(
            cedo.pendentes_se_couber(prazo - Duration::from_micros(1)),
            None,
            "um microssegundo antes do prazo, o fim de uma volta sem linha disse a conta: ou o \
             prazo não é exato, e a thread acordaria tarde, ou a volta sem linha não pede as duas \
             fichas"
        );

        let mut no_prazo = seco_com_uma_segurada();
        assert_eq!(
            no_prazo.pendentes_se_couber(prazo),
            Some(1),
            "a thread acordada no prazo que o balde deu não achou ficha para a conta"
        );
        assert_eq!(
            no_prazo.admitir(prazo),
            Some(0),
            "acordada no prazo que o balde deu, a conta tomou a ficha que era do MOD"
        );
        assert_eq!(
            no_prazo.ate_sobrar_uma_ficha(prazo),
            None,
            "a conta já foi dita, e o balde continua pedindo para acordar a thread"
        );
    }

    /// Mil linhas de `console` de uma vez, e nada depois delas.
    const RAJADA_DE_MIL: &str = "for (let i = 0; i < 1000; i++) console.log('linha ' + i);";

    /// O que as linhas de console de um teste dizem, somado.
    #[derive(Default)]
    struct Soma {
        /// As linhas do MOD que passaram.
        passaram: usize,
        /// As que o balde segurou e alguma linha contou.
        contadas: usize,
        /// O nível da linha de conta do produto ([`CONTA_DO_BALDE`]), quando veio.
        conta: Option<NivelDoConsole>,
    }

    impl Soma {
        /// Soma as linhas de console até a fala que `fim` reconhece, inclusive.
        ///
        /// Qualquer outra fala no meio reprova: aqui só se fala pelo `console`.
        fn ate(&mut self, executor: &ExecutorQuickJs, fim: fn(&ParaOFora) -> bool, porque: &str) {
            loop {
                let Some(fala) = executor.receber(Duration::from_secs(5)) else {
                    panic!("{porque}: nada mais chegou em cinco segundos");
                };
                let acabou = fim(&fala);
                match fala {
                    ParaOFora::Console {
                        nivel,
                        texto,
                        suprimidas,
                    } => {
                        self.contadas += suprimidas as usize;
                        if texto == CONTA_DO_BALDE {
                            self.conta = Some(nivel);
                        } else {
                            self.passaram += 1;
                        }
                    }
                    outra if !acabou => panic!("veio {outra:?} no meio das linhas de console"),
                    _ => {}
                }
                if acabou {
                    return;
                }
            }
        }

        /// Confere que nada sumiu sem ser contado, e que quem contou foi o
        /// produto, em aviso.
        fn conferir(&self, onde: &str) {
            let rajada = RAJADA_DO_CONSOLE as usize;
            assert!(
                (rajada..=rajada + 2).contains(&self.passaram),
                "passaram {} linhas de um laço de mil, e a rajada é {rajada}: o balde não segurou \
                 ({onde})",
                self.passaram
            );
            assert_eq!(
                self.conta,
                Some(NivelDoConsole::Aviso),
                "a conta do que o balde segurou não saiu em aviso ({onde}), e o `seele.log` mostra \
                 {} linhas como se fossem tudo",
                self.passaram
            );
            assert_eq!(
                self.passaram + self.contadas,
                1000,
                "linhas sumiram sem ser contadas ({onde}): {} passaram e {} foram contadas",
                self.passaram,
                self.contadas
            );
        }
    }

    /// **Uma rajada no `console` seguida de silêncio tem a conta dita até a
    /// instância parar.**
    ///
    /// O teste de laço acima só fecha a soma porque uma linha passa depois e
    /// leva a conta. Sem ela, a conta ficava no balde e caía com o contexto: o
    /// `seele.log` mostrava 32 linhas, e quem lê concluía que eram 32 itens.
    ///
    /// **Quem a diz depende do relógio**, e por isso o nome deixou de ser «ao
    /// parar»: o `Encerrar` sai logo depois do diagnóstico, e numa máquina
    /// normal chega antes de sobrar uma ficha (meio segundo) — a conta sai na
    /// parada. Numa máquina parada esse tempo, ela sai antes, pela espera do
    /// laço ([`BaldeDoConsole::ate_sobrar_uma_ficha`]). O que este teste prova
    /// sempre é que ela sai antes do `Parou`, e que a soma fecha; o caminho da
    /// parada tem um guarda que não depende do relógio
    /// (`a_conta_que_sobra_quando_a_instancia_para_sai_antes_do_parou`).
    #[test]
    fn uma_rajada_no_console_seguida_de_silencio_tem_a_conta_dita_ate_parar() {
        let executor = executor();
        executor.iniciar(RAJADA_DE_MIL).expect("código");
        // O diagnóstico entra na fila atrás do código: quando ele volta, a volta
        // do laço acabou, e o encerramento não o interrompe no meio.
        executor.diagnostico().expect("o executor de pé");
        let mut soma = Soma::default();
        soma.ate(
            &executor,
            |fala| matches!(fala, ParaOFora::Diagnostico { .. }),
            "a volta do laço não acabou",
        );
        executor.pedir_encerramento();
        soma.ate(
            &executor,
            |fala| matches!(fala, ParaOFora::Parou),
            "o executor não confirmou a parada",
        );
        soma.conferir("rajada e silêncio, até parar");
    }

    /// **A conta que sobra quando a instância para sai antes do `Parou`** —
    /// com ficha ou sem.
    ///
    /// O MOD escreve duas linhas a cada 4 ms: a primeira leva cada ficha que
    /// aparece, e a segunda é sempre segurada. Assim o balde nunca junta a
    /// ficha que sobraria, a espera do laço nunca acorda para dizer a conta, e
    /// o fim de cada volta acha o balde seco: o único caminho que diz as
    /// seguradas depois da última linha que passou é o da parada
    /// ([`BaldeDoConsole::pendentes`], no fim de [`rodar`]). E há sempre uma
    /// segurada quando o `Encerrar` chega, entre duas voltas — sem depender do
    /// relógio, como o teste de cima depende.
    #[test]
    fn a_conta_que_sobra_quando_a_instancia_para_sai_antes_do_parou() {
        let executor = executor();
        executor
            .iniciar("setInterval(() => { console.log('a'); console.log('b'); }, 4);")
            .expect("código");
        // A última linha de console antes do `Parou`, contando as que chegam
        // antes do `Encerrar`.
        let mut ultima_linha = None;
        let mut pediu = false;
        let inicio = Instant::now();
        loop {
            assert!(
                inicio.elapsed() < Duration::from_secs(5),
                "o executor não confirmou a parada em cinco segundos"
            );
            if !pediu && inicio.elapsed() >= Duration::from_millis(600) {
                executor.pedir_encerramento();
                pediu = true;
            }
            match executor.receber(Duration::from_millis(20)) {
                Some(ParaOFora::Parou) => break,
                Some(ParaOFora::Console {
                    texto, suprimidas, ..
                }) => ultima_linha = Some((texto, suprimidas)),
                Some(_) | None => {}
            }
        }
        let Some((texto, suprimidas)) = ultima_linha else {
            panic!("nenhuma linha de console chegou, e o teste não mediu nada");
        };
        assert!(
            texto == CONTA_DO_BALDE && suprimidas >= 1,
            "a última linha antes do `Parou` não foi a conta do que o balde segurou depois da \
             última linha que passou ({texto:?}, suprimidas={suprimidas}): a conta caiu com o \
             contexto, e o `seele.log` mostra menos do que o MOD escreveu como se fosse tudo"
        );
    }

    /// **A conta sai sem esperar o MOD parar**, e sem temporizador nem resposta
    /// que acorde a thread do motor.
    ///
    /// Um MOD que escreve cem linhas ao subir e se cala tinha a conta dita só
    /// quando parava: a thread dormia no canal, sem prazo, e a conta esperava
    /// no balde uma volta que não vinha. Numa sessão longa, o `seele.log`
    /// mostrava as linhas da rajada como se fossem tudo por horas (T2 N1).
    /// Agora a espera do laço também acorda quando sobra uma ficha além da que
    /// fica com o MOD — meio segundo depois da rajada —, e só enquanto há linha
    /// segurada. O segundo de prazo é o dobro disso.
    #[test]
    fn a_conta_das_seguradas_sai_sem_esperar_o_mod_parar() {
        let executor = executor();
        executor
            .iniciar("for (let i = 0; i < 100; i++) console.log(i);")
            .expect("código");
        let prazo = Instant::now() + Duration::from_secs(1);
        let mut passaram = 0_usize;
        let (nivel, suprimidas) = loop {
            match executor.receber(prazo.saturating_duration_since(Instant::now())) {
                Some(ParaOFora::Console {
                    nivel,
                    texto,
                    suprimidas,
                }) if texto == CONTA_DO_BALDE => break (nivel, suprimidas),
                Some(ParaOFora::Console { .. }) => passaram += 1,
                outro => panic!(
                    "um MOD que escreveu cem linhas e se calou, sem temporizador nem Encerrar, \
                     não teve a conta das seguradas dita em um segundo — ela espera a parada, e \
                     o seele.log mostra {passaram} linhas como se fossem tudo; veio {outro:?}"
                ),
            }
        };
        assert_eq!(
            nivel,
            NivelDoConsole::Aviso,
            "a conta das seguradas saiu noutro nível que o aviso"
        );
        assert!(
            suprimidas >= 60,
            "a conta que saiu sem esperar a parada diz {suprimidas} seguradas, e de cem linhas \
             passaram {passaram}: ela não conta o que o balde segurou"
        );
    }

    /// **Um MOD que continua escrevendo não perde a vez para a linha da
    /// conta.**
    ///
    /// A espera do laço acordava no instante em que a ficha se completava, e a
    /// linha da conta a gastava: a linha seguinte do MOD achava menos de uma
    /// ficha e era segurada, o que fazia o produto acordar de novo na ficha
    /// seguinte — e o ciclo se repetia. Medido na revisão do lote D: depois de
    /// uma rajada, um `setInterval` de 100 ms deixava em quatro segundos 1 linha
    /// do MOD e 14 da conta, e o `seele.log` dizia «quantas» no lugar de «o
    /// quê». Sem a espera da ficha eram 15 e 0.
    ///
    /// Agora a espera deixa a ficha ao MOD por um período inteiro antes de o
    /// produto falar ([`BaldeDoConsole::ate_sobrar_uma_ficha`]): quem escreve
    /// quatro linhas por segundo ou mais a gasta antes de juntar duas.
    #[test]
    fn um_mod_que_continua_escrevendo_nao_perde_a_vez_para_a_conta() {
        let executor = executor();
        executor
            .iniciar(
                "for (let i = 0; i < 100; i++) console.log('rajada ' + i);\
                 setInterval(() => console.log('do MOD'), 100);",
            )
            .expect("código");
        let fim = Instant::now() + Duration::from_secs(2);
        let (mut do_mod, mut conta) = (0_usize, 0_usize);
        while Instant::now() < fim {
            match executor.receber(fim.saturating_duration_since(Instant::now())) {
                Some(ParaOFora::Console { texto, .. }) if texto == "do MOD" => do_mod += 1,
                Some(ParaOFora::Console { texto, .. }) if texto == CONTA_DO_BALDE => conta += 1,
                Some(ParaOFora::Console { .. }) | None => {}
                Some(outra) => panic!("veio {outra:?} no meio das linhas de console"),
            }
        }
        assert!(
            conta <= 1 && do_mod > conta && do_mod >= 4,
            "um MOD que escreve a cada 100 ms deixou, em dois segundos depois de uma rajada, \
             {do_mod} linhas dele e {conta} da conta: a linha da conta toma a ficha que era do \
             MOD, e o seele.log diz «quantas» no lugar de «o quê»"
        );
    }

    /// **Um MOD que recebe mais do que escreve não perde a vez para a linha da
    /// conta.**
    ///
    /// O mesmo defeito da espera, pelo fim de volta: cada evento é uma volta
    /// em que o MOD não escreve, e ela dizia a conta com a primeira ficha que
    /// achasse. A linha seguinte do MOD, do temporizador, achava o balde seco.
    /// Medido na re-revisão do lote D, com eventos a cada 30 ms e o MOD
    /// escrevendo a cada 100 ms: 1 linha do MOD e 14 da conta em quatro
    /// segundos, igual antes do lote (`7953a3b0`). Agora o fim de toda volta
    /// pede duas fichas, como a espera ([`BaldeDoConsole::pendentes_se_couber`]).
    #[test]
    fn um_mod_que_recebe_mais_do_que_escreve_nao_perde_a_vez_para_a_conta() {
        let executor = executor();
        executor
            .iniciar(
                "SeeleUI.aoEvento(() => {});\
                 for (let i = 0; i < 100; i++) console.log('rajada ' + i);\
                 setInterval(() => console.log('do MOD'), 100);",
            )
            .expect("código");
        let inicio = Instant::now();
        let fim = inicio + Duration::from_secs(2);
        let mut proximo_evento = inicio;
        let (mut do_mod, mut conta) = (0_usize, 0_usize);
        while Instant::now() < fim {
            if Instant::now() >= proximo_evento {
                let _ = executor.entregar(r#"{"tipo":"evento","nome":"tecla"}"#);
                proximo_evento += Duration::from_millis(30);
            }
            let ate = proximo_evento.min(fim);
            match executor.receber(ate.saturating_duration_since(Instant::now())) {
                Some(ParaOFora::Console { texto, .. }) if texto == "do MOD" => do_mod += 1,
                Some(ParaOFora::Console { texto, .. }) if texto == CONTA_DO_BALDE => conta += 1,
                Some(ParaOFora::Console { .. }) | None => {}
                Some(outra) => panic!("veio {outra:?} no meio das linhas de console"),
            }
        }
        assert!(
            conta <= 1 && do_mod > conta && do_mod >= 4,
            "um MOD que recebe um evento a cada 30 ms e escreve a cada 100 ms deixou, em dois \
             segundos depois de uma rajada, {do_mod} linhas dele e {conta} da conta: o fim da \
             volta de cada evento toma a ficha que era do MOD"
        );
    }

    /// **Um MOD que escreve e depois trabalha, na mesma volta, não perde a vez
    /// para a linha da conta.**
    ///
    /// O fim de uma volta em que o MOD escreveu dizia a conta com uma ficha
    /// só: se a ficha se completava depois da linha segurada, ainda na volta,
    /// a conta a tomava, e a próxima linha do MOD achava o balde seco. Medido
    /// na re-revisão da rodada 2 do lote D, com um `setInterval` de 100 ms que
    /// escreve e trabalha 60 ms: 6 linhas do MOD e 9 da conta em quatro
    /// segundos; com 120 ms de trabalho, 0 e 15. Agora o fim de toda volta pede
    /// duas fichas, como a espera ([`BaldeDoConsole::pendentes_se_couber`]).
    #[test]
    fn um_mod_que_escreve_e_trabalha_na_mesma_volta_nao_perde_a_vez_para_a_conta() {
        let executor = executor();
        executor
            .iniciar(
                "for (let i = 0; i < 100; i++) console.log('rajada ' + i);\
                 setInterval(() => {\
                   console.log('do MOD');\
                   const desde = Date.now(); while (Date.now() - desde < 60) {}\
                 }, 100);",
            )
            .expect("código");
        let fim = Instant::now() + Duration::from_secs(3);
        let (mut do_mod, mut conta) = (0_usize, 0_usize);
        while Instant::now() < fim {
            match executor.receber(fim.saturating_duration_since(Instant::now())) {
                Some(ParaOFora::Console { texto, .. }) if texto == "do MOD" => do_mod += 1,
                Some(ParaOFora::Console { texto, .. }) if texto == CONTA_DO_BALDE => conta += 1,
                Some(ParaOFora::Console { .. }) | None => {}
                Some(outra) => panic!("veio {outra:?} no meio das linhas de console"),
            }
        }
        assert!(
            conta <= 1 && do_mod > conta && do_mod >= 6,
            "um MOD que escreve e trabalha 60 ms a cada 100 ms deixou, em três segundos depois \
             de uma rajada, {do_mod} linhas dele e {conta} da conta: o fim da volta dele toma a \
             ficha que se completou durante o trabalho, e que era da próxima linha"
        );
    }

    /// **A conta não espera um temporizador mais tarde que a ficha que sobra.**
    ///
    /// Com um temporizador de pé, a espera do laço é a menor das duas: o
    /// temporizador é de um minuto, e a conta só chega dentro dos cinco
    /// segundos de [`Soma::ate`] porque a thread acorda quando sobra uma ficha
    /// ([`BaldeDoConsole::ate_sobrar_uma_ficha`]), e não no vencimento.
    ///
    /// Era a metade do relógio de `a_conta_das_seguradas_sai_na_primeira_volta_com_ficha`.
    /// A metade da resposta virou, na rodada 2,
    /// `o_fim_de_uma_volta_em_que_o_mod_nao_escreveu_espera_sobrar_uma_ficha`,
    /// que prova o contrário do que ela provava.
    #[test]
    fn a_conta_das_seguradas_nao_espera_um_temporizador_mais_tarde() {
        let com_relogio = executor();
        com_relogio
            .iniciar(&format!("{RAJADA_DE_MIL} setTimeout(() => {{}}, 60000);"))
            .expect("código");
        let mut soma = Soma::default();
        soma.ate(
            &com_relogio,
            |fala| matches!(fala, ParaOFora::Console { texto, .. } if texto == CONTA_DO_BALDE),
            "a conta das seguradas esperou o temporizador de um minuto, e não a ficha: a espera \
             do laço usou o vencimento, e não o menor dos dois prazos",
        );
        soma.conferir("com um temporizador de um minuto de pé");
    }

    /// **O fim de uma volta em que o MOD não escreveu espera sobrar uma
    /// ficha.**
    ///
    /// Este teste se chamava
    /// `a_conta_das_seguradas_sai_no_fim_da_volta_de_uma_resposta_com_ficha`, e
    /// provava o contrário: que a resposta, sem escrever, dizia a conta com uma
    /// ficha só. Era o defeito da espera de antes, por outro caminho — a conta
    /// gastava a única ficha, e a linha seguinte do MOD achava o balde seco (ver
    /// [`um_mod_que_recebe_mais_do_que_escreve_nao_perde_a_vez_para_a_conta`]).
    ///
    /// A resposta chega entre a primeira ficha (um quarto de segundo depois da
    /// rajada) e a segunda (meio segundo), aos 260 ms, e o diagnóstico logo
    /// atrás dela: quando ele volta, a conta **não** pode ter saído. Ela sai
    /// depois, quando sobra a segunda ficha, pela espera do laço.
    ///
    /// **O relógio é conferido, e não suposto** ([`AINDA_SEM_A_SEGUNDA_FICHA`]):
    /// a asserção só vale se o diagnóstico voltou antes de a segunda ficha
    /// poder ter se completado. Numa máquina parada mais que isso, a tentativa
    /// é refeita, e três lentas seguidas reprovam dizendo que nada foi medido.
    #[test]
    fn o_fim_de_uma_volta_em_que_o_mod_nao_escreveu_espera_sobrar_uma_ficha() {
        for _ in 0..3 {
            let inicio = Instant::now();
            let com_resposta = executor();
            com_resposta
                .iniciar(&format!(
                    "globalThis.aoResponder = () => {{}}; {RAJADA_DE_MIL}"
                ))
                .expect("código");
            com_resposta.diagnostico().expect("o executor de pé");
            let mut soma = Soma::default();
            soma.ate(
                &com_resposta,
                |fala| matches!(fala, ParaOFora::Diagnostico { .. }),
                "a volta do laço não acabou",
            );
            std::thread::sleep(Duration::from_millis(260));
            // O diagnóstico entra na fila atrás da resposta: quando ele volta,
            // a volta da resposta acabou, e o que ela disse já passou pelo
            // canal.
            com_resposta.entregar("{}").expect("entregar");
            com_resposta.diagnostico().expect("o executor de pé");
            soma.ate(
                &com_resposta,
                |fala| matches!(fala, ParaOFora::Diagnostico { .. }),
                "a volta da resposta não acabou",
            );
            if inicio.elapsed() >= AINDA_SEM_A_SEGUNDA_FICHA {
                continue;
            }
            assert_eq!(
                soma.conta, None,
                "a volta de uma resposta que não escreveu disse a conta com uma ficha só: a \
                 linha seguinte do MOD acha o balde seco, e o seele.log diz «quantas» no lugar \
                 de «o quê»"
            );
            soma.ate(
                &com_resposta,
                |fala| matches!(fala, ParaOFora::Console { texto, .. } if texto == CONTA_DO_BALDE),
                "a conta não saiu quando sobrou a segunda ficha",
            );
            soma.conferir("quando sobrou a segunda ficha, depois de uma resposta que não escreveu");
            return;
        }
        panic!(
            "três tentativas seguidas passaram de {AINDA_SEM_A_SEGUNDA_FICHA:?} antes do \
             diagnóstico, e o teste não mediu nada: a máquina está parada demais"
        );
    }

    /// Até quando, desde antes de o código subir, a segunda ficha depois da
    /// rajada ainda não pode ter se completado.
    ///
    /// A rajada seca o balde depois de o código subir, e a segunda ficha vem
    /// meio segundo depois disso: medido desde antes de `iniciar`, o prazo de
    /// meio segundo é o mais cedo que ela pode chegar. Vinte milissegundos de
    /// folga para o canal.
    const AINDA_SEM_A_SEGUNDA_FICHA: Duration = Duration::from_millis(480);

    /// **O fim de uma volta em que o MOD escreveu também espera sobrar uma
    /// ficha.**
    ///
    /// Este teste se chamava
    /// `o_fim_de_uma_volta_em_que_o_mod_escreveu_diz_a_conta_com_uma_ficha`, e
    /// provava o ramo de uma ficha que a rodada 3 apagou: a linha do MOD
    /// segurada, e a ficha que se completava depois, ainda na volta, «não era
    /// de ninguém». Era da próxima linha do MOD (ver
    /// [`um_mod_que_escreve_e_trabalha_na_mesma_volta_nao_perde_a_vez_para_a_conta`]).
    ///
    /// A rajada e uma espera de 300 ms estão na mesma volta: no fim dela há uma
    /// ficha e um quinto, e a conta **não** pode sair no diagnóstico que vem
    /// atrás. Ela sai depois, quando sobra a segunda ficha. O relógio é
    /// conferido como no teste de cima.
    #[test]
    fn o_fim_de_uma_volta_em_que_o_mod_escreveu_tambem_espera_sobrar_uma_ficha() {
        for _ in 0..3 {
            let inicio = Instant::now();
            let executor = executor();
            executor
                .iniciar(&format!(
                    "{RAJADA_DE_MIL} const t = Date.now(); while (Date.now() - t < 300) {{}}"
                ))
                .expect("código");
            executor.diagnostico().expect("o executor de pé");
            let mut soma = Soma::default();
            soma.ate(
                &executor,
                |fala| matches!(fala, ParaOFora::Diagnostico { .. }),
                "a volta do laço não acabou",
            );
            if inicio.elapsed() >= AINDA_SEM_A_SEGUNDA_FICHA {
                continue;
            }
            assert_eq!(
                soma.conta, None,
                "o fim de uma volta em que o MOD escreveu e seguiu trabalhando disse a conta com \
                 uma ficha só: a ficha que se completou durante o trabalho era da próxima linha \
                 do MOD"
            );
            soma.ate(
                &executor,
                |fala| matches!(fala, ParaOFora::Console { texto, .. } if texto == CONTA_DO_BALDE),
                "a conta não saiu quando sobrou a segunda ficha",
            );
            soma.conferir("quando sobrou a segunda ficha, depois de uma volta que escreveu");
            return;
        }
        panic!(
            "três tentativas seguidas passaram de {AINDA_SEM_A_SEGUNDA_FICHA:?} antes do \
             diagnóstico, e o teste não mediu nada: a máquina está parada demais"
        );
    }

    /// **A fronteira do QuickJS, medida** — etapa E1.
    ///
    /// A sonda que rodou no WKWebView alcançou `indexedDB`, `caches`,
    /// `BroadcastChannel` e a rota do IPC do Tauri. Esta roda a mesma pergunta
    /// no motor que é candidato a substituí-lo.
    ///
    /// **A diferença que importa não é a lista, é de onde ela vem.** No
    /// navegador, o ambiente existe e a fronteira é tudo o que se consegue
    /// tirar dele. Aqui o contexto nasce sem ambiente nenhum, e a fronteira é
    /// tudo o que alguém escolheu pôr — uma função por vez, neste arquivo.
    ///
    /// Por isso este teste é automático e o da janela não era: ele não precisa
    /// de aplicativo, de servidor nem de clique. É a prova de E1 que dá para
    /// repetir a cada commit.
    #[test]
    fn um_mod_no_quickjs_nao_alcanca_ambiente_nenhum() {
        let executor = executor();
        executor.iniciar(SONDA).expect("entregar o código");
        let achados: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("a sonda devolve JSON");

        let mut alcancados = Vec::new();
        let mut globais = String::new();
        for achado in achados.as_array().expect("uma lista") {
            let nome = achado["nome"].as_str().unwrap_or_default();
            if nome == "globais" {
                globais = achado["detalhe"].as_str().unwrap_or_default().to_owned();
                continue;
            }
            if achado["alcancou"].as_bool().unwrap_or(false) {
                alcancados.push(nome.to_owned());
            }
        }

        assert!(
            alcancados.is_empty(),
            "o código de um MOD alcançou ambiente que ninguém lhe deu: {alcancados:?}"
        );

        // Impresso sempre: quem rodar com `--nocapture` lê a superfície inteira
        // do contexto, e é ela que precisa ser revisada quando alguém a mudar.
        println!("globais do contexto de um MOD: {globais}");
        assert!(
            globais.split(',').any(|nome| nome == "seele"),
            "a ponte não está no contexto: {globais}"
        );
        for proibido in ["indexedDB", "fetch", "Worker", "__TAURI"] {
            assert!(
                !globais.contains(proibido),
                "`{proibido}` apareceu na lista de globais: {globais}"
            );
        }
    }

    /// **Um laço infinito não prende a saída** — e quem o para é a revogação.
    ///
    /// O critério de aceite da E1 escreve isso, e é a propriedade que separa «o
    /// MOD está isolado» de «o MOD está isolado enquanto ele colaborar».
    ///
    /// **Sem teto de trabalho e sem prazo**: se o laço parar, foi porque a
    /// sessão foi revogada, e não porque o motor cansou.
    /// **E o laço tem de estar rodando quando a revogação chega.**
    ///
    /// A primeira versão deste teste passava em 0,00 s com a revogação
    /// arrancada, e passava porque o laço nunca começava: `pedir_encerramento`
    /// marca o revogado **antes** de mandar a mensagem, e a thread via a marca
    /// ao tirar o `Codigo` da fila e saía sem rodar nada. O que o teste media
    /// era a conferência de fila, não a interrupção.
    ///
    /// Por isso o MOD avisa que começou. Quando `comecei` chega, a thread está
    /// dentro do `eval`, e dali só o tratador de interrupção a tira.
    #[test]
    fn a_revogacao_sozinha_para_um_laco_infinito() {
        let mut executor = so_com(SEM_TETO_DE_TRABALHO, SEM_PRAZO);
        executor
            .iniciar("seele.postar('comecei'); while (true) {}")
            .expect("entregar o código");
        assert_eq!(
            uma_mensagem(&executor),
            "comecei",
            "o laço não chegou a rodar"
        );
        let levou = executor
            .encerrou(Duration::from_secs(10))
            .expect("o executor tem de confirmar a parada");
        assert!(
            levou < Duration::from_secs(5),
            "a saída levou {levou:?} com um MOD em laço infinito"
        );
    }

    /// **O prazo sozinho interrompe uma volta**, sem revogação e sem teto de
    /// trabalho.
    ///
    /// Contra o MOD que trava a si mesmo sem que ninguém tenha mandado sair.
    /// «Só contar consultas do motor não estabelece prazo real de saída» — a
    /// diretriz nomeia, e este teste é o que separa as duas coisas.
    #[test]
    fn o_prazo_sozinho_interrompe_uma_volta() {
        let executor = so_com(SEM_TETO_DE_TRABALHO, Duration::from_millis(50));
        executor.iniciar("while (true) {}").expect("código");
        match executor.receber(Duration::from_secs(5)) {
            Some(ParaOFora::Interrompido | ParaOFora::Falhou(_)) => {}
            outro => panic!("a volta não foi interrompida pelo prazo: {outro:?}"),
        }
        // Sem revogação nenhuma nesta linha: quem parou foi o relógio.
        assert!(!executor.interrupcao.revogada());
    }

    /// **O teto de trabalho sozinho interrompe uma volta**, sem prazo e sem
    /// revogação.
    ///
    /// Vale contra o laço que corre rápido demais para o relógio ser consultado
    /// com frequência útil — e por isso ele existe ao lado do prazo, e não no
    /// lugar dele.
    #[test]
    fn o_teto_de_trabalho_sozinho_interrompe_uma_volta() {
        let executor = so_com(1_000, SEM_PRAZO);
        executor.iniciar("while (true) {}").expect("código");
        match executor.receber(Duration::from_secs(5)) {
            Some(ParaOFora::Interrompido | ParaOFora::Falhou(_)) => {}
            outro => panic!("a volta não foi interrompida pelo trabalho: {outro:?}"),
        }
    }

    /// **Uma microtarefa não escapa do prazo da volta que a criou.**
    ///
    /// Sem isto, um MOD moveria o laço para dentro de uma `Promise` e correria
    /// fora do teto — pela porta de trás do motor.
    ///
    /// Medido com o prazo curto e **sem teto de trabalho**, para que a parada
    /// só possa ser explicada pelo relógio da volta.
    #[test]
    fn um_laco_dentro_de_uma_promessa_tambem_para() {
        let executor = so_com(SEM_TETO_DE_TRABALHO, Duration::from_millis(50));
        executor
            .iniciar("Promise.resolve().then(() => { while (true) {} });")
            .expect("código");
        match executor.receber(Duration::from_secs(10)) {
            Some(ParaOFora::Interrompido | ParaOFora::Falhou(_)) => {}
            outro => panic!("o laço dentro da promessa não foi interrompido: {outro:?}"),
        }
    }

    /// **O teto que parou a volta é dito uma vez, e não sobra para a
    /// seguinte.**
    ///
    /// Um teto lido não pode atribuir a parada a uma exceção seguinte da mesma
    /// volta. Os temporizadores vencidos rodam numa volta só, e depois de um
    /// parado pelo prazo ainda pode chegar a [`relatar`] uma exceção de
    /// verdade — um valor lançado cujo `toString` lança dentro do `catch` do
    /// prelúdio, ou um `__seeleRelogio` que o MOD trocou; o `TypeError` comum
    /// não chega, porque aquele `catch` o pega. Se o teto ficasse guardado
    /// depois de lido, essa exceção sairia como «o produto a interrompeu». E a
    /// revogação não é teto: ela tem a frase dela, e não a de um teto.
    #[test]
    fn o_teto_que_parou_a_volta_e_tirado_uma_vez_e_nao_sobra() {
        /// Chama o tratador até ele dizer sim, com um limite para o teste não
        /// rodar para sempre se ele nunca disser.
        fn ate_parar(interrupcao: &Interrupcao) -> bool {
            (0..1_000_000).any(|_| interrupcao.deve_parar())
        }
        let trabalho = Interrupcao::nova(Limites {
            memoria: TETO_DE_MEMORIA,
            consultas_por_volta: 3,
            prazo_por_volta: SEM_PRAZO,
        });
        trabalho.comecar();
        assert!(ate_parar(&trabalho), "o teto de trabalho de 3 nunca parou");
        assert_eq!(
            trabalho.tirar_teto(),
            Some(Teto::Trabalho),
            "o teto de trabalho parou a volta, e a interrupção não guardou que foi ele"
        );
        assert_eq!(
            trabalho.tirar_teto(),
            None,
            "o teto lido continuou guardado, e a próxima exceção de verdade da mesma volta \
             sairia como parada do produto"
        );

        let prazo = Interrupcao::nova(Limites {
            memoria: TETO_DE_MEMORIA,
            consultas_por_volta: SEM_TETO_DE_TRABALHO,
            prazo_por_volta: Duration::ZERO,
        });
        prazo.comecar();
        assert!(ate_parar(&prazo), "o prazo de zero nunca parou");
        assert_eq!(
            prazo.tirar_teto(),
            Some(Teto::Prazo),
            "o prazo parou a volta, e a interrupção não guardou que foi ele"
        );
        assert!(ate_parar(&prazo), "o prazo vencido deixou de parar");
        prazo.comecar();
        assert_eq!(
            prazo.tirar_teto(),
            None,
            "uma volta nova começou com o teto da anterior, e uma exceção de verdade nela sairia \
             como parada do produto"
        );

        let revogada = Interrupcao::nova(Limites {
            memoria: TETO_DE_MEMORIA,
            consultas_por_volta: SEM_TETO_DE_TRABALHO,
            prazo_por_volta: SEM_PRAZO,
        });
        revogada.comecar();
        revogada.revogar();
        assert!(revogada.deve_parar(), "a revogação não parou a volta");
        assert_eq!(
            revogada.tirar_teto(),
            None,
            "a revogação foi guardada como teto, e a sessão que acabou seria dita como um MOD \
             que passou do prazo ou do trabalho"
        );
    }

    /// O que uma volta parada deixou: as falas para a janela e as linhas de
    /// `console`. Espera a primeira linha por até cinco segundos e, depois
    /// dela, mais um pouco — uma segunda linha da mesma parada também conta.
    fn o_que_a_parada_deixou(
        executor: &ExecutorQuickJs,
    ) -> (Vec<ParaOFora>, Vec<(NivelDoConsole, String)>) {
        let mut fim = Instant::now() + Duration::from_secs(5);
        let mut falas = Vec::new();
        let mut linhas = Vec::new();
        while Instant::now() < fim {
            match executor.receber(Duration::from_millis(20)) {
                Some(ParaOFora::Console { nivel, texto, .. }) => {
                    if linhas.is_empty() {
                        fim = Instant::now() + Duration::from_millis(200);
                    }
                    linhas.push((nivel, texto));
                }
                Some(fala) => falas.push(fala),
                None => {}
            }
        }
        (falas, linhas)
    }

    /// A linha que uma volta parada por um teto deixa: uma só, em erro, com a
    /// frase de `teto`, sem «lançou» e, quando `com_onde`, com a primeira linha
    /// da pilha — onde o MOD estava quando o produto o parou.
    fn exigir_a_frase_do_teto(linhas: &[(NivelDoConsole, String)], teto: &str, com_onde: bool) {
        let [(nivel, texto)] = linhas else {
            panic!(
                "a volta que o produto parou deixou {} linha(s) de console, e deve deixar uma — \
                 sem ela o seele.log não diz por que o MOD parou: {linhas:?}",
                linhas.len()
            );
        };
        assert_eq!(
            *nivel,
            NivelDoConsole::Erro,
            "a linha da volta parada saiu num nível que não é erro, e o filtro do seele.log a \
             trataria como recado: {texto}"
        );
        assert!(
            !texto.contains("lançou"),
            "a volta que o produto parou foi dita como se o MOD tivesse lançado, e quem escreveu \
             o MOD procura um `throw` que não existe: {texto}"
        );
        assert!(
            texto.contains(teto),
            "a linha da volta parada não diz que teto ela passou («{teto}»): {texto}"
        );
        assert!(
            texto.contains("o produto a interrompeu (o MOD continua de pé)"),
            "a linha da volta parada não diz que o produto interrompeu só aquela volta e que o \
             MOD continua de pé, e quem lê o seele.log entende que o MOD acabou — ele segue \
             recebendo eventos e temporizadores: {texto}"
        );
        if com_onde {
            assert!(
                texto.contains("\n    at "),
                "a linha da volta parada perdeu a primeira linha da pilha, que diz onde o MOD \
                 estava quando o produto o parou: {texto}"
            );
        }
    }

    /// **Uma volta que o produto parou pelo prazo é dita como tal, e não como
    /// «o MOD lançou».**
    ///
    /// O QuickJS faz da interrupção uma exceção, `InternalError: interrupted`,
    /// e a volta chegava a [`relatar`] como qualquer outra: o `seele.log` dizia
    /// «o MOD lançou: InternalError: interrupted», e quem escreveu o MOD
    /// procurava um `throw` que não existe. Quem parou foi o produto, e a
    /// [`Interrupcao`] sabe por qual teto.
    #[test]
    fn uma_volta_parada_pelo_prazo_diz_o_prazo_e_nao_que_o_mod_lancou() {
        let executor = so_com(SEM_TETO_DE_TRABALHO, Duration::from_millis(50));
        executor.iniciar("while (true) {}").expect("código");
        let (falas, linhas) = o_que_a_parada_deixou(&executor);
        exigir_a_frase_do_teto(&linhas, "passou do prazo de 50 ms", true);
        assert!(
            falas
                .iter()
                .any(|fala| matches!(fala, ParaOFora::Falhou(_) | ParaOFora::Interrompido)),
            "a janela deixou de saber que a volta parou: {falas:?}"
        );
    }

    /// **E pelo teto de trabalho, com a frase dele.** O mesmo laço, com o
    /// prazo fora de alcance: só o trabalho explica a parada, e a frase tem de
    /// dizer isso, e não o prazo.
    #[test]
    fn uma_volta_parada_pelo_teto_de_trabalho_diz_o_teto_de_trabalho() {
        let executor = so_com(1_000, SEM_PRAZO);
        executor.iniciar("while (true) {}").expect("código");
        let (_, linhas) = o_que_a_parada_deixou(&executor);
        exigir_a_frase_do_teto(&linhas, "passou do teto de trabalho", true);
    }

    /// **O mesmo laço dentro de uma promessa também é dito.** A volta do
    /// código termina bem, e quem para é a microtarefa, em [`escoar_jobs`]: a
    /// janela recebia o `Interrompido`, e o `seele.log` não recebia nada.
    #[test]
    fn um_laco_numa_promessa_parado_pelo_prazo_tambem_e_dito() {
        let executor = so_com(SEM_TETO_DE_TRABALHO, Duration::from_millis(50));
        executor
            .iniciar("Promise.resolve().then(() => { while (true) {} });")
            .expect("código");
        let (_, linhas) = o_que_a_parada_deixou(&executor);
        exigir_a_frase_do_teto(&linhas, "passou do prazo de 50 ms", true);
    }

    /// **A mensagem que não caberia no fio não entra na fila.**
    ///
    /// O mesmo teto de 12 KiB do quadro de controle, conferido onde a mensagem
    /// nasce. Descobrir o tamanho depois de ela já estar na fila da janela
    /// seria pagar a memória duas vezes para recusar no fim.
    #[test]
    fn uma_mensagem_grande_demais_e_recusada_na_porta() {
        let executor = executor();
        executor
            .iniciar("seele.postar('x'.repeat(13 * 1024)); seele.postar('coube');")
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "coube");
    }

    /// **O prelúdio conta bytes UTF-8, e ele não tem `TextEncoder`.**
    ///
    /// O reteste nativo de `c4fe3ea` mediu o defeito no motor real: a contagem
    /// usava `new TextEncoder()` e recuava para `texto.length` quando ele não
    /// existisse — e ele nunca existe aqui. Sete mil «é» passavam pela
    /// conferência do prelúdio com «7.000» e chegavam ao Rust com 14.066
    /// bytes, que os recusava como `fila-cheia`. A contenção funcionava; o
    /// diagnóstico mentia, e mandava procurar saturação onde havia tamanho.
    ///
    /// Aqui o MOD pede uma região com texto acentuado e conta o que a recusa
    /// diz. `mensagem-grande` com o tamanho certo é o aceite; `fila-cheia` é o
    /// defeito de volta.
    #[test]
    fn o_preludio_conta_utf8_sem_textencoder() {
        let executor = executor();
        executor
            .iniciar(
                "seele.postar(JSON.stringify({ tem: typeof TextEncoder }));\
                 SeeleUI.regiao([{ forma: 'texto', dentro: 'é'.repeat(7000) }])\
                   .then(() => seele.postar('aceitou'),\
                         (erro) => seele.postar(String(erro.message)));",
            )
            .expect("código");

        // Primeiro a prova de que o atalho do navegador não existe mesmo.
        let ambiente: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("JSON");
        assert_eq!(
            ambiente.get("tem").and_then(serde_json::Value::as_str),
            Some("undefined"),
            "`TextEncoder` passou a existir no contexto do MOD; se isso for \
             deliberado, a contagem do prelúdio pode voltar a usá-lo"
        );

        // A recusa pode chegar pela mensagem que o MOD posta no `catch` ou pelo
        // relato de promessa rejeitada — o motor avisa antes de a microtarefa
        // do `then` rodar. O que se mede é o **texto**, e ele é o mesmo.
        let recusa = match executor.receber(Duration::from_secs(5)) {
            Some(ParaOFora::Mensagem(json)) => json,
            Some(ParaOFora::Falhou(texto)) => texto,
            Some(outro) => panic!("veio {outro:?}"),
            None => panic!("o executor não respondeu"),
        };
        assert!(
            recusa.contains("mensagem-grande"),
            "a recusa não diz que a mensagem não coube: {recusa}"
        );
        // Dois bytes por «é», mais a casca do JSON. O número exato não importa;
        // o que importa é que ele **não** é 7.000 — que é o que `length` diria.
        assert!(
            recusa.contains("14"),
            "o tamanho relatado não é o de UTF-8: sete mil «é» ocupam mais de \
             catorze mil bytes, e `length` diria sete mil: {recusa}"
        );
        assert!(
            !recusa.contains("fila-cheia"),
            "uma mensagem grande demais voltou a ser relatada como fila \
             saturada: {recusa}"
        );
    }

    /// **A volta completa: o MOD pergunta, o produto responde, o MOD continua.**
    ///
    /// É o quarto verbo do contrato — `entregar` — e sem este teste ele
    /// existiria sem nunca ter sido exercitado. O que ele prende é que a
    /// resposta chega ao código do autor pelo nome que a API promete, e que o
    /// que ele fizer com ela ainda sai pela mesma porta.
    #[test]
    fn a_resposta_volta_para_dentro_e_o_mod_segue() {
        let executor = executor();
        executor
            .iniciar(
                "globalThis.aoResponder = (json) => {                    seele.postar(JSON.stringify({ recebi: JSON.parse(json).valor }));                  };                  seele.postar('pronto');",
            )
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "pronto");

        executor.entregar(r#"{"valor":42}"#).expect("entregar");
        let volta: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("JSON");
        assert_eq!(
            volta.get("recebi").and_then(serde_json::Value::as_u64),
            Some(42)
        );
    }

    /// E uma resposta que chega **depois da revogação** não entra.
    ///
    /// §5 do contrato, passo 1: «nenhum novo efeito daquela instância é
    /// admitido, mesmo que mensagens já estejam na fila». A fila é literal
    /// aqui — é um canal — e a conferência acontece ao tirar dela.
    #[test]
    fn uma_resposta_depois_da_revogacao_nao_entra() {
        let executor = executor();
        executor
            .iniciar(
                "globalThis.aoResponder = () => seele.postar('entrou');                  seele.postar('pronto');",
            )
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "pronto");

        executor.interrupcao.revogar();
        executor.entregar(r#"{"valor":1}"#).expect("entregar");
        match executor.receber(Duration::from_millis(500)) {
            Some(ParaOFora::Parou) | None => {}
            Some(outro) => panic!("a resposta entrou depois da revogação: {outro:?}"),
        }
    }

    /// **A fila tem teto, e o MOD fica sabendo quando bate nele.**
    ///
    /// O teto de 12 KiB é da mensagem; este é do acumulado. Sem ele, um MOD num
    /// laço enche a memória de quem usa uma mensagem de cada vez, todas dentro
    /// do limite individual — e a fila cresce até o processo cair.
    #[test]
    fn a_fila_de_saida_para_de_crescer_e_o_mod_recebe_a_recusa() {
        let executor = executor();
        // Dez mil mensagens de um kibibyte: cento e poucos vezes o teto de
        // bytes, e mais de cem vezes o de quantidade.
        executor
            .iniciar(
                "let coube = 0;                  for (let i = 0; i < 10000; i++) { if (seele.postar('x'.repeat(1024))) coube++; }                  globalThis.__coube = coube;",
            )
            .expect("código");

        // Sem ler nada da fila: é assim que a saturação acontece de verdade —
        // o MOD escreve mais rápido do que a janela lê.
        std::thread::sleep(Duration::from_millis(300));
        let (mensagens, bytes) = executor.fila().ocupacao();
        assert!(
            mensagens <= MENSAGENS_NA_FILA,
            "a fila passou do teto de quantidade: {mensagens}"
        );
        assert!(
            bytes <= BYTES_NA_FILA,
            "a fila passou do teto de bytes: {bytes}"
        );
        assert!(
            executor.fila().recusadas() > 0,
            "nada foi recusado, então o teto não valeu"
        );
    }

    /// **E a fila cheia não prende o encerramento.**
    ///
    /// É a propriedade que separa uma fila com teto de uma fila que empurra o
    /// problema para outro lugar. Se `postar` esperasse por espaço, a thread do
    /// motor ficaria parada dentro de uma função nativa — e o tratador de
    /// interrupção não alcança função nativa, que é justamente o que a diretriz
    /// avisa.
    #[test]
    fn uma_fila_cheia_nao_prende_o_encerramento() {
        let mut executor = executor();
        executor
            .iniciar("while (true) { seele.postar('x'.repeat(4096)); }")
            .expect("código");
        // A fila enche em milissegundos, e o laço continua tentando.
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            executor.fila().recusadas() > 0,
            "a fila não chegou a encher, então o teste não mediu a saturação"
        );

        let levou = executor
            .encerrou(Duration::from_secs(10))
            .expect("a saturação prendeu o encerramento");
        assert!(
            levou < Duration::from_secs(5),
            "a saída levou {levou:?} com a fila cheia"
        );
    }

    /// E ler a fila devolve o lugar: a saturação é um estado, e não uma marca.
    #[test]
    fn ler_a_fila_devolve_o_lugar() {
        let executor = executor();
        executor
            .iniciar("for (let i = 0; i < 200; i++) seele.postar('x'.repeat(1024));")
            .expect("código");
        std::thread::sleep(Duration::from_millis(200));
        let (antes, _) = executor.fila().ocupacao();
        assert!(antes > 0, "a fila ficou vazia, e o teste não mede nada");

        let _ = executor.receber(Duration::from_secs(2));
        let (depois, _) = executor.fila().ocupacao();
        assert!(
            depois < antes,
            "ler não devolveu o lugar: {antes} antes, {depois} depois"
        );
    }

    /// Quanta memória residente este processo está usando, em KiB.
    ///
    /// Lida do sistema operacional, e não estimada: `ps` responde o que o
    /// gerente de tarefas responderia, que é a medida de que alguém reclama.
    /// Zero quando não deu para ler — e zero aparece no relatório em vez de
    /// virar um número inventado.
    /// **`None` é medição inválida, e não zero.**
    ///
    /// Converter uma leitura que falhou em zero faria o relatório dizer «este
    /// processo não usa memória», que é a coisa mais errada que ele poderia
    /// dizer — e ninguém saberia que o `ps` não respondeu.
    fn residente_kib() -> Option<u64> {
        campo_do_ps("rss=").and_then(|texto| texto.trim().parse().ok())
    }

    /// Quanto de CPU este processo já gastou, em centésimos de segundo.
    ///
    /// `ps` devolve `MM:SS.cc`. Centésimos porque é a resolução que ele dá — e
    /// dizer «dois centésimos» é mais honesto que converter para microssegundos
    /// um número que não os tem.
    fn cpu_centesimos() -> Option<u64> {
        let bruto = campo_do_ps("cputime=")?;
        let bruto = bruto.trim().to_owned();
        let (minutos, resto) = bruto.split_once(':').unwrap_or(("0", &bruto));
        let (segundos, centesimos) = resto.split_once('.').unwrap_or((resto, "0"));
        let m: u64 = minutos.trim().parse().ok()?;
        let s: u64 = segundos.parse().ok()?;
        let c: u64 = centesimos.parse().ok()?;
        Some((m * 60 + s) * 100 + c)
    }

    /// Um campo do `ps` sobre este processo, cru.
    fn campo_do_ps(campo: &str) -> Option<String> {
        std::process::Command::new("ps")
            .args(["-o", campo, "-p"])
            .arg(std::process::id().to_string())
            .output()
            .ok()
            .and_then(|saida| String::from_utf8(saida.stdout).ok())
    }

    /// **O custo de zero, uma e três instâncias** — medida da bancada.
    ///
    /// Não é um teste de aceite: ele não reprova por número, porque um número
    /// medido numa máquina não é um orçamento. Ele **imprime**, e o que decide
    /// é quem lê — a diretriz proíbe inventar um teto em MB sem medir, e este é
    /// o passo que dá o que medir.
    ///
    /// Rode com `--nocapture` para ver o relatório.
    ///
    /// O que ele mede, e o que ele não mede: memória residente do processo
    /// inteiro, tempo de subida e tempo de encerramento. **Não** mede impacto
    /// na voz nem latência de interação — isso precisa do aplicativo de
    /// verdade, com renderer e mídia, e está registrado como pendente.
    #[test]
    fn o_custo_de_zero_uma_e_tres_instancias() {
        // Um MOD que faz o que um MOD faz: guarda estado e responde.
        const TRABALHO: &str = "globalThis.n = 0;              globalThis.aoResponder = () => { globalThis.n++; seele.postar('ok'); };              seele.postar('pronto');";

        // Aquecido: a primeira alocação do processo não é o custo de ninguém.
        {
            let e = executor();
            e.iniciar(TRABALHO).expect("código");
            let _ = uma_mensagem(&e);
        }
        std::thread::sleep(Duration::from_millis(200));
        let base = residente_kib();

        let mut relatorio = vec![match base {
            Some(kib) => format!("sem instância: {kib} KiB"),
            None => "sem instância: MEDIÇÃO INVÁLIDA (o `ps` não respondeu)".to_owned(),
        }];
        for quantas in [1_usize, 3] {
            let subida = Instant::now();
            let mut instancias = Vec::new();
            for _ in 0..quantas {
                let e = executor();
                e.iniciar(TRABALHO).expect("código");
                assert_eq!(uma_mensagem(&e), "pronto");
                instancias.push(e);
            }
            let subiu_em = subida.elapsed();

            // Uma volta de trabalho em cada, para nenhuma estar apenas parada.
            for e in &instancias {
                e.entregar("{}").expect("entregar");
                assert_eq!(uma_mensagem(e), "ok");
            }
            let com = residente_kib();

            // **CPU ociosa.** Instâncias paradas não podem custar: um motor que
            // rodasse um laço de espera gastaria bateria por não estar fazendo
            // nada, e num aplicativo de voz isso disputa com o áudio.
            let cpu_antes = cpu_centesimos();
            std::thread::sleep(Duration::from_secs(1));
            let ociosa = match (cpu_antes, cpu_centesimos()) {
                // **Zero aqui é «nada observável nesta resolução»**, e não
                // «consumo nulo». O `ps` dá centésimos de segundo; o que cabe
                // abaixo disso não aparece, e afirmar que não existe seria
                // afirmar mais do que a medida diz.
                (Some(antes), Some(depois)) => {
                    format!("{} centésimos em 1 s", depois.saturating_sub(antes))
                }
                _ => "MEDIÇÃO INVÁLIDA".to_owned(),
            };

            let saida = Instant::now();
            for e in &mut instancias {
                e.encerrou(Duration::from_secs(5))
                    .expect("a instância tem de confirmar a parada");
            }
            let saiu_em = saida.elapsed();
            drop(instancias);

            let memoria = match (base, com) {
                (Some(base), Some(com)) => {
                    let sobre = i64::try_from(com).unwrap_or(0) - i64::try_from(base).unwrap_or(0);
                    let cada = (com as f64 - base as f64) / quantas as f64;
                    format!("{com} KiB ({sobre:+} KiB sobre a base, {cada:.0} KiB cada)")
                }
                _ => "MEDIÇÃO INVÁLIDA".to_owned(),
            };
            relatorio.push(format!(
                "{quantas} instância(s): {memoria} · subida {subiu_em:?} · CPU ociosa {ociosa} · encerramento {saiu_em:?}"
            ));
        }

        println!("---- custo do executor QuickJS ----");
        for linha in &relatorio {
            println!("{linha}");
        }
        println!(
            "medido em {} · `ps -o rss` e `-o cputime`, processo inteiro, uma coleta",
            std::env::consts::OS
        );
        println!(
            "**alcance**: binário de teste, sem bomba, sem WebView e sem mídia. \
             Não é o custo do produto, e não substitui a coleta integrada."
        );
    }

    /// Quantos temporizadores o anfitrião tem de pé por este MOD.
    fn relogios_de_pe(executor: &ExecutorQuickJs) -> usize {
        executor.diagnostico().expect("pedir diagnóstico");
        loop {
            match executor.receber(Duration::from_secs(2)) {
                Some(ParaOFora::Diagnostico { relogios }) => return relogios,
                Some(_) => {}
                None => panic!("o executor não respondeu ao diagnóstico"),
            }
        }
    }

    /// **Um timeout que agenda outro timeout dispara** — sem tráfego de fora.
    ///
    /// O ramo dos vencidos tinha um `continue` antes de recolher os pedidos, e
    /// o pedido do segundo timeout ficava parado até chegar uma mensagem
    /// externa. Num MOD que só usa relógio, ela pode nunca chegar.
    ///
    /// Este teste não manda nada depois de iniciar: se a corrente andar, foi
    /// porque a volta dos vencidos recolheu o pedido seguinte.
    #[test]
    fn um_timeout_que_agenda_outro_anda_sozinho() {
        let executor = executor();
        executor
            .iniciar(
                "let n = 0;                  const passo = () => { n++; seele.postar(String(n)); if (n < 3) setTimeout(passo, 5); };                  setTimeout(passo, 5);",
            )
            .expect("código");
        for esperado in ["1", "2", "3"] {
            assert_eq!(
                uma_mensagem(&executor),
                esperado,
                "a corrente de temporizadores parou"
            );
        }
    }

    /// **Um intervalo que se cancela para, e some da tabela do anfitrião.**
    ///
    /// Pelo mesmo `continue`: o cancelamento era anotado dentro do callback e
    /// ficava sem coleta, então o intervalo continuava batendo depois de o MOD
    /// tê-lo cancelado.
    #[test]
    fn um_intervalo_que_se_cancela_para_de_bater() {
        let executor = executor();
        executor
            .iniciar(
                "let n = 0;                  const id = setInterval(() => {                    n++; seele.postar(String(n));                    if (n === 2) clearInterval(id);                  }, 5);",
            )
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "1");
        assert_eq!(uma_mensagem(&executor), "2");
        // Se o cancelamento não tivesse sido recolhido, viria «3» em 5 ms.
        assert!(
            executor.receber(Duration::from_millis(400)).is_none(),
            "o intervalo continuou batendo depois de se cancelar"
        );
        // **E a tabela do anfitrião ficou vazia.** Sem esta linha, o teste
        // passaria também se o motor tivesse morrido: as duas coisas param de
        // mandar mensagem.
        assert_eq!(
            relogios_de_pe(&executor),
            0,
            "o intervalo cancelado continuou na tabela do anfitrião"
        );
    }

    /// **Um atraso absurdo não derruba a thread do motor.**
    ///
    /// `1e300` é finito, passa por qualquer conferência de «é número», e
    /// `Duration::from_secs_f64` entra em pânico com ele. Derrubar a thread por
    /// causa de um argumento é derrubar o MOD de quem está na sessão.
    #[test]
    fn um_atraso_absurdo_nao_derruba_o_motor() {
        let executor = executor();
        executor
            .iniciar(
                "setTimeout(() => seele.postar('absurdo'), 1e300);                  setTimeout(() => seele.postar('nan'), NaN);                  setTimeout(() => seele.postar('negativo'), -1);                  setTimeout(() => seele.postar('vivo'), 5);",
            )
            .expect("código");
        // `NaN` e `-1` viram zero na fachada, como num navegador, e zero vira o
        // piso de 4 ms — então os três curtos disparam. O de `1e300` é aparado
        // para um dia, e não chega.
        let mut chegaram = Vec::new();
        while let Some(ParaOFora::Mensagem(json)) = executor.receber(Duration::from_millis(400)) {
            chegaram.push(json);
        }
        chegaram.sort();
        assert_eq!(chegaram, vec!["nan", "negativo", "vivo"]);
        assert_eq!(
            relogios_de_pe(&executor),
            1,
            "o temporizador de um dia devia continuar na tabela, e só ele"
        );
    }

    /// **O teto de temporizadores é dito a quem chamou.**
    ///
    /// A fachada guardava o callback de um temporizador que o anfitrião tinha
    /// ignorado: `setTimeout` devolvia um número, o MOD acreditava que estava
    /// agendado, e ele nunca disparava.
    #[test]
    fn o_teto_de_temporizadores_chega_a_quem_pediu() {
        let executor = executor();
        executor
            .iniciar(
                "let ultimo = 1;                  for (let i = 0; i < 300; i++) ultimo = setTimeout(() => {}, 60000);                  seele.postar(JSON.stringify({ ultimo }));",
            )
            .expect("código");
        let resposta: serde_json::Value =
            serde_json::from_str(&uma_mensagem(&executor)).expect("JSON");
        assert_eq!(
            resposta.get("ultimo").and_then(serde_json::Value::as_u64),
            Some(0),
            "o tricentésimo `setTimeout` devolveu um número, e ele nunca vai disparar"
        );
    }

    /// **A fila de entrada também tem teto, e ela não tinha.**
    ///
    /// `entregar` copiava e enfileirava o JSON sem limite. Uma janela que
    /// responde em rajada — ou um MOD que pede mais rápido do que o motor
    /// atende — fazia a memória crescer deste lado, onde nenhum teto de heap do
    /// motor conta.
    #[test]
    fn a_fila_de_entrada_para_de_crescer_e_diz_a_quem_entrega() {
        let executor = executor();
        // O MOD nem precisa existir: o que se mede é a fila antes do motor.
        // Um laço grande o bastante para passar dos dois tetos.
        let mut recusou = false;
        for _ in 0..10_000 {
            let corpo = format!(r#"{{"n":1,"ok":true,"enchimento":"{}"}}"#, "x".repeat(1024));
            if executor.entregar(&corpo).is_err() {
                recusou = true;
                break;
            }
        }
        assert!(
            recusou,
            "a fila de entrada aceitou dez mil respostas de um kibibyte sem recusar nenhuma"
        );
        let (mensagens, bytes) = executor.entrada().ocupacao();
        assert!(
            mensagens <= MENSAGENS_NA_FILA && bytes <= BYTES_NA_FILA,
            "a fila de entrada passou do teto: {mensagens} mensagens, {bytes} bytes"
        );
    }

    /// **Saturar a entrada não impede revogar nem confirmar a parada.**
    ///
    /// É a mesma propriedade que já vale para a saída, e a diretriz pede as
    /// duas: «preservar revogação e confirmação de parada quando houver
    /// saturação».
    #[test]
    fn a_entrada_cheia_nao_prende_o_encerramento() {
        let mut executor = so_com(SEM_TETO_DE_TRABALHO, SEM_PRAZO);
        // O MOD entra num laço para o motor não drenar a fila de entrada.
        executor
            .iniciar("seele.postar('comecei'); while (true) {}")
            .expect("código");
        assert_eq!(uma_mensagem(&executor), "comecei");
        while executor.entregar(r#"{"n":1,"ok":true}"#).is_ok() {}

        let levou = executor
            .encerrou(Duration::from_secs(10))
            .expect("a saturação da entrada prendeu o encerramento");
        assert!(
            levou < Duration::from_secs(5),
            "a saída levou {levou:?} com a entrada cheia e o MOD em laço"
        );
    }

    /// **Um MOD que chama `seele.postar` direto também esbarra no teto.**
    ///
    /// A diretriz avisa: «não confiar no limite de oito pedidos do prelúdio: o
    /// autor pode chamar `seele.postar` diretamente». O teto de oito é uma
    /// conveniência do prelúdio, que é código do MOD; o que contém de verdade é
    /// a fila, que é nossa.
    #[test]
    fn o_teto_da_fila_nao_depende_do_preludio() {
        let executor = executor();
        executor
            .iniciar(
                "let coube = 0;                  for (let i = 0; i < 5000; i++) { if (seele.postar('x'.repeat(2048))) coube++; }                  globalThis.__coube = coube;",
            )
            .expect("código");
        std::thread::sleep(Duration::from_millis(300));
        let (mensagens, bytes) = executor.fila().ocupacao();
        assert!(
            mensagens <= MENSAGENS_NA_FILA && bytes <= BYTES_NA_FILA,
            "o teto foi ultrapassado por quem não usa o prelúdio: {mensagens}, {bytes}"
        );
        assert!(executor.fila().recusadas() > 0);
    }

    /// **Uma promessa que rejeita sem ninguém pegando é dita.**
    ///
    /// Medido no aplicativo nativo antes de ser escrito aqui: o MOD de
    /// referência chamava `SeeleMods.snapshot()` numa função `async` sem
    /// `catch`, e quando a sessão ainda não tinha subido o `snapshot` rejeitava
    /// — o MOD morria ali, na segunda linha, **sem uma palavra**. No registro
    /// aparecia como duas falas e silêncio; na tela, como um MOD que não
    /// desenha. Levei três execuções para separar isso de um travamento.
    ///
    /// É o defeito que o `CLAUDE.md` deste repositório nomeia como o mais caro:
    /// o produto sabe e não conta. O motor **sabe** — QuickJS rastreia a
    /// rejeição sem tratamento —, e quem escreveu o MOD é quem precisa saber.
    #[test]
    fn uma_promessa_rejeitada_sem_tratamento_nao_morre_calada() {
        let executor = executor();
        executor
            .iniciar(
                "globalThis.aoResponder = () => {};\
                 (async () => { throw new Error('ninguém me pega'); })();",
            )
            .expect("código");

        let prazo = std::time::Instant::now();
        let mut dito = None;
        while prazo.elapsed() < Duration::from_secs(3) {
            match executor.receber(Duration::from_millis(100)) {
                Some(ParaOFora::Falhou(motivo)) => {
                    dito = Some(motivo);
                    break;
                }
                Some(_) => {}
                None => {}
            }
        }
        let motivo = dito.expect(
            "um MOD cuja promessa rejeitou sem tratamento sumiu sem dizer nada — \
             é exatamente assim que ele desaparece na máquina de quem o instalou",
        );
        assert!(
            motivo.contains("ninguém me pega"),
            "o aviso não leva o que o MOD disse: {motivo}"
        );
    }

    /// **O `Falhou` de uma rejeição sem tratamento tem o teto do registro.**
    ///
    /// Ele saía do rastreador com o texto inteiro do que o MOD lançou, e só a
    /// cota dos avisos ([`AVISOS_NA_FILA`]) o limitava — em número, e não em
    /// tamanho: um MOD que rejeitasse com cem mil caracteres punha os cem mil
    /// em cada `Falhou` que coubesse, no canal e na fila da janela. Agora ele
    /// é cortado como a linha do registro ([`cortar_linha_do_console`]), e
    /// diz que foi.
    #[test]
    fn o_falhou_da_rejeicao_tem_o_teto_do_registro() {
        let executor = executor();
        executor
            .iniciar("Promise.reject(new Error('x'.repeat(100000)));")
            .expect("código");
        let prazo = Instant::now() + Duration::from_secs(5);
        let motivo = loop {
            assert!(
                Instant::now() < prazo,
                "a rejeição sem tratamento não chegou à janela como `Falhou` em cinco segundos"
            );
            if let Some(ParaOFora::Falhou(motivo)) = executor.receber(Duration::from_millis(100)) {
                break motivo;
            }
        };
        assert!(
            motivo.starts_with(&format!("{REJEITADA_SEM_TRATAMENTO}: ")),
            "o `Falhou` da rejeição perdeu a frase que diz o que houve: {}",
            motivo.chars().take(80).collect::<String>()
        );
        let (corpo, aviso) = motivo.split_once(" […cortado: ").unwrap_or_else(|| {
            panic!(
                "o `Falhou` de uma rejeição de cem mil caracteres saiu com {} caracteres, sem o \
                 corte do registro: o canal e a fila da janela levam o texto inteiro a cada aviso",
                motivo.chars().count()
            )
        });
        assert!(
            corpo.chars().count() <= crate::TETO_DA_FRASE_NO_REGISTRO,
            "o `Falhou` da rejeição foi cortado acima do teto do registro: {} caracteres antes \
             do aviso, e o teto é {}",
            corpo.chars().count(),
            crate::TETO_DA_FRASE_NO_REGISTRO
        );
        assert!(
            aviso.ends_with(" caracteres]"),
            "o aviso do corte do `Falhou` não diz quanto a rejeição tinha: {aviso}"
        );
    }

    /// E o descarte acontece na dona do runtime, sem matar thread à força.
    #[test]
    fn o_encerramento_confirma_e_a_thread_sai_sozinha() {
        let mut executor = executor();
        executor.iniciar("seele.postar('oi');").expect("código");
        assert_eq!(uma_mensagem(&executor), "oi");
        assert!(executor.encerrou(Duration::from_secs(5)).is_ok());
    }
}
