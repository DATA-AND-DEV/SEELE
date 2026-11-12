use super::{assentar_fala, ModsNativos};
use crate::executor::{
    ExecutorQuickJs, Fila, Limites, ParaOFora, LINHAS_DE_CONSOLE_POR_SEGUNDO, RAJADA_DO_CONSOLE,
    REJEICOES_GUARDADAS,
};
use crate::rastro_de_teste::capturar;
use std::time::{Duration, Instant};

/// O id do MOD destes testes.
const ID: &str = "prova/falha";

/// O que um MOD de verdade deixou.
struct Deixou {
    /// As falas que não são de console, em ordem.
    falas: Vec<ParaOFora>,
    /// O que a bomba escreveu no registro.
    rastro: String,
    /// Quantas linhas de console saíram do motor.
    linhas: u32,
    /// Quanto tempo passou desde antes de o motor subir.
    levou: Duration,
}

/// Sobe `codigo` num executor de verdade, chama `depois` com ele de pé, e
/// assenta cada linha de console como a bomba assentaria — numa captura,
/// na thread do teste — até `basta` dizer que chega ou o `prazo` passar.
fn deixou(
    codigo: &str,
    depois: impl FnOnce(&ExecutorQuickJs),
    basta: impl Fn(&[ParaOFora], u32) -> bool,
    prazo: Duration,
) -> Deixou {
    let inicio = Instant::now();
    let ((falas, linhas), rastro) = capturar(|| {
        let executor = ExecutorQuickJs::novo(Limites::default()).expect("o motor");
        executor.iniciar(codigo).expect("o código");
        depois(&executor);
        let mods = std::sync::Mutex::new(ModsNativos::default());
        let fila = std::sync::Arc::new(Fila::default());
        let mut falas = Vec::new();
        let mut linhas = 0_u32;
        while inicio.elapsed() < prazo && !basta(&falas, linhas) {
            match executor.receber(Duration::from_millis(20)) {
                Some(fala @ ParaOFora::Console { .. }) => {
                    linhas += 1;
                    assentar_fala(&mods, &|| true, &fila, 1, fala, ID);
                }
                Some(fala) => falas.push(fala),
                None => {}
            }
        }
        (falas, linhas)
    });
    Deixou {
        falas,
        rastro,
        linhas,
        levou: inicio.elapsed(),
    }
}

/// O máximo de linhas que o balde do console deixa sair em `levou`: a
/// rajada, a recarga do tempo que passou e uma de folga pelo
/// arredondamento.
fn teto_do_balde(levou: Duration) -> u32 {
    let recarga = (levou.as_secs_f64() * f64::from(LINHAS_DE_CONSOLE_POR_SEGUNDO)).ceil();
    RAJADA_DO_CONSOLE + recarga as u32 + 1
}

/// A linha de erro do console deste MOD que traz todos os `pedacos`.
fn linha_de_erro<'a>(rastro: &'a str, pedacos: &[&str]) -> Option<&'a str> {
    rastro.lines().find(|linha| {
        linha.contains("ERROR")
            && linha.contains("console do MOD")
            && linha.contains(&format!("mod_id={ID}"))
            && pedacos.iter().all(|pedaco| linha.contains(pedaco))
    })
}

/// Uma das falas é uma mensagem para a janela que contém `trecho`?
fn tem_mensagem(falas: &[ParaOFora], trecho: &str) -> bool {
    falas
        .iter()
        .any(|fala| matches!(fala, ParaOFora::Mensagem(json) if json.contains(trecho)))
}

#[test]
fn uma_excecao_no_topo_do_mod_chega_ao_registro_com_o_texto() {
    let lancou = |falas: &[ParaOFora]| {
        falas
            .iter()
            .any(|fala| matches!(fala, ParaOFora::Falhou(texto) if texto == "o MOD lançou"))
    };
    let deixou = deixou(
        "const avatar = null; avatar.largura;",
        |_| {},
        |falas, linhas| lancou(falas) && linhas > 0,
        Duration::from_secs(5),
    );
    assert!(
        lancou(&deixou.falas),
        "a janela deixou de receber o `Falhou` da volta que lançou, e a gestão já não diz que \
             o MOD falhou: {:?}",
        deixou.falas
    );
    let linha =
        linha_de_erro(&deixou.rastro, &["o MOD lançou", "TypeError"]).unwrap_or_else(|| {
            panic!(
                "a exceção no topo do MOD não chegou ao seele.log com o id e o texto — quem \
                     escreveu o MOD fica com «o MOD lançou» e nada mais: {}",
                deixou.rastro
            )
        });
    assert!(
        linha.contains(r"\n    at "),
        "a linha da exceção perdeu a primeira linha da pilha, que diz onde o MOD lançou: \
             {linha}"
    );
}

/// **A falha dentro de uma promessa também chega ao registro.**
///
/// Todo pedido de MOD é uma promessa, e o erro mais comum de quem escreve
/// MOD — ler de `null` — dentro de uma função `async` sem `catch` só
/// chegava à gestão: o `Falhou` saía, e o `seele.log` ficava com uma linha
/// DEBUG que o filtro de produção nem grava (I2 da revisão final do Plano 1).
#[test]
fn uma_promessa_rejeitada_sem_tratamento_chega_ao_registro_com_o_texto() {
    let rejeitou = |falas: &[ParaOFora]| {
        falas.iter().any(|fala| {
            matches!(fala, ParaOFora::Falhou(texto)
                    if texto.starts_with("promessa rejeitada sem tratamento:"))
        })
    };
    let deixou = deixou(
        "(async () => { await 0; null.x; })();",
        |_| {},
        |falas, linhas| rejeitou(falas) && linhas > 0,
        Duration::from_secs(5),
    );
    assert!(
        rejeitou(&deixou.falas),
        "a janela deixou de receber o `Falhou` da promessa rejeitada, e a gestão já não diz \
             que o MOD falhou: {:?}",
        deixou.falas
    );
    let linha = linha_de_erro(
        &deixou.rastro,
        &["promessa rejeitada sem tratamento", "TypeError", "of null"],
    )
    .unwrap_or_else(|| {
        panic!(
            "a promessa rejeitada sem tratamento não chegou ao seele.log em ERROR, com o id e \
                 o texto — quem escreveu o MOD só a vê na gestão: {}",
            deixou.rastro
        )
    });
    assert!(
        linha.contains(r"\n    at "),
        "a linha da promessa rejeitada perdeu a primeira linha da pilha, que diz onde o MOD \
             lançou: {linha}"
    );
}

/// **A rejeição que o MOD pega logo depois não vira linha de erro, nem
/// `Falhou` na janela.**
///
/// `try { await f(); } catch {}`, com `f` uma função `async` que lança
/// antes do primeiro `await`: a promessa rejeita antes de o `await` prender
/// o tratamento, e o motor avisa a rejeição nessa hora. A linha do registro
/// e o `Falhou` da janela esperam o fim da volta, e saem só se ninguém a
/// pegou: dizer «sem tratamento» sobre uma rejeição tratada mandaria quem
/// escreveu o MOD procurar um defeito que não existe — no `seele.log`, em
/// ERROR, e na gestão, como «falhou». O `Falhou` saía na hora, de dentro
/// do rastreador (P51-01).
///
/// O `fim` vem de um temporizador, numa volta seguinte: quando ele chega,
/// as linhas e os avisos da primeira volta já passaram pelo canal.
#[test]
fn uma_rejeicao_pega_logo_depois_nao_vira_linha_de_erro() {
    let deixou = deixou(
        "(async () => { try { await (async () => { throw new Error('pega logo depois'); })(); } \
               catch (e) {} })();\
             (async () => { await 0; null.x; })();\
             setTimeout(() => seele.postar('fim'), 0);",
        |_| {},
        |falas, _| tem_mensagem(falas, "fim"),
        Duration::from_secs(5),
    );
    assert!(
        tem_mensagem(&deixou.falas, "fim"),
        "o MOD não chegou ao fim, e o teste não mediu nada: {:?}",
        deixou.falas
    );
    assert!(
        linha_de_erro(
            &deixou.rastro,
            &["promessa rejeitada sem tratamento", "of null"]
        )
        .is_some(),
        "a rejeição que ninguém pegou, na mesma volta, não chegou ao seele.log: {}",
        deixou.rastro
    );
    assert!(
        !deixou.rastro.contains("pega logo depois"),
        "uma rejeição que o MOD pegou com `try`/`await` saiu no seele.log como «sem \
             tratamento»: quem escreveu o MOD vai procurar um defeito que não existe: {}",
        deixou.rastro
    );
    assert!(
        !deixou.falas.iter().any(
            |fala| matches!(fala, ParaOFora::Falhou(texto) if texto.contains("pega logo depois"))
        ),
        "uma rejeição que o MOD pegou com `try`/`await` chegou à janela como `Falhou`, e a \
             gestão diz que o MOD falhou por uma promessa que ele tratou: {:?}",
        deixou.falas
    );
}

/// **A volta que um teto parou, sem microtarefa na fila, também termina
/// para as rejeições.**
///
/// Nada mais vai rodar nela que possa pegá-las, e a linha sai logo depois
/// da do teto. Esperar a próxima volta deixaria a rejeição de um MOD sem
/// temporizador nem resposta calada até a instância parar.
#[test]
fn a_rejeicao_de_uma_volta_parada_sem_microtarefa_na_fila_e_dita_sem_esperar_a_parada() {
    let deixou = deixou(
        "Promise.reject(new Error('antes do laço'));\
             (async () => { await 0; while (true) {} })();",
        |_| {},
        |_, linhas| linhas >= 2,
        Duration::from_secs(5),
    );
    assert!(
        linha_de_erro(&deixou.rastro, &["passou do"]).is_some(),
        "a volta do laço não foi parada por um teto, e o teste não mediu nada: {}",
        deixou.rastro
    );
    assert!(
        linha_de_erro(
            &deixou.rastro,
            &["promessa rejeitada sem tratamento", "antes do laço"]
        )
        .is_some(),
        "a volta parou por um teto sem microtarefa na fila, e a rejeição dela ficou \
             esperando uma volta seguinte que um MOD sem temporizador nem resposta não tem: {}",
        deixou.rastro
    );
}

/// **A rejeição que ainda esperava quando a instância parou é dita.**
///
/// Uma volta que um teto parou com microtarefas na fila deixa as rejeições
/// dela esperando a próxima volta, porque essas microtarefas ainda podiam
/// pegá-las. Se a instância para antes, ninguém mais as pega: a linha sai
/// na parada, antes do `Parou`. E a espera fica vazia antes de o runtime
/// ser desfeito, porque cada rejeição segura a promessa dela.
#[test]
fn a_rejeicao_que_esperava_quando_a_instancia_parou_e_dita() {
    let (falas, rastro) = capturar(|| {
        let executor = ExecutorQuickJs::novo(Limites::default()).expect("o motor");
        executor
            .iniciar(
                "Promise.reject(new Error('esperava quando parou'));\
                     (async () => { await 0; while (true) {} })();\
                     (async () => { await 0; })();",
            )
            .expect("o código");
        let mods = std::sync::Mutex::new(ModsNativos::default());
        let fila = std::sync::Arc::new(Fila::default());
        let mut falas = Vec::new();
        let mut pediu = false;
        let inicio = Instant::now();
        while inicio.elapsed() < Duration::from_secs(10) {
            match executor.receber(Duration::from_millis(20)) {
                Some(fala @ ParaOFora::Console { .. }) => {
                    assentar_fala(&mods, &|| true, &fila, 1, fala, ID);
                }
                Some(ParaOFora::Parou) => {
                    falas.push(ParaOFora::Parou);
                    break;
                }
                Some(fala) => falas.push(fala),
                None => {}
            }
            if !pediu
                && falas
                    .iter()
                    .any(|fala| matches!(fala, ParaOFora::Interrompido))
            {
                executor.pedir_encerramento();
                pediu = true;
            }
        }
        falas
    });
    assert!(
        falas
            .iter()
            .any(|fala| matches!(fala, ParaOFora::Interrompido)),
        "a volta do laço não foi parada por um teto, e o teste não mediu a espera: {falas:?}"
    );
    assert!(
        falas.iter().any(|fala| matches!(fala, ParaOFora::Parou)),
        "a instância não confirmou a parada: {falas:?}"
    );
    assert!(
        linha_de_erro(
            &rastro,
            &["promessa rejeitada sem tratamento", "esperava quando parou"]
        )
        .is_some(),
        "a rejeição que esperava o fim de uma volta parada por um teto sumiu quando a \
             instância parou, sem chegar ao seele.log: {rastro}"
    );
}

/// **A rejeição além do teto da espera não se perde: ela é contada, e a
/// conta sai no fim da volta.**
///
/// Até `REJEICOES_GUARDADAS` rejeições esperam o fim da volta, cada uma com
/// a linha dela; as de depois só são contadas, e a conta sai numa linha
/// depois das guardadas: «e mais N rejeições sem tratamento além da
/// espera». A primeira linha, que costuma ser a causa, continua saindo.
/// Quarenta numa volta só: as guardadas e a conta cabem juntas na rajada
/// do balde, e a conta não é segurada atrás delas.
#[test]
fn a_rejeicao_alem_do_teto_da_espera_nao_se_perde() {
    let deixou = deixou(
        "for (let i = 0; i < 40; i++) Promise.reject(new Error('r' + i));\
             setTimeout(() => seele.postar('fim'), 0);",
        |_| {},
        |falas, _| tem_mensagem(falas, "fim"),
        Duration::from_secs(5),
    );
    assert!(
        tem_mensagem(&deixou.falas, "fim"),
        "o MOD não chegou ao fim, e o teste não mediu nada: {:?}",
        deixou.falas
    );
    let ultima_guardada = format!("Error: r{}", REJEICOES_GUARDADAS - 1);
    for primeira_e_ultima in ["Error: r0", ultima_guardada.as_str()] {
        assert!(
            linha_de_erro(
                &deixou.rastro,
                &["promessa rejeitada sem tratamento", primeira_e_ultima]
            )
            .is_some(),
            "a rejeição «{primeira_e_ultima}», dentro do teto da espera, não chegou ao \
                 seele.log com a linha dela: {}",
            deixou.rastro
        );
    }
    let alem = 40 - REJEICOES_GUARDADAS;
    assert!(
        linha_de_erro(
            &deixou.rastro,
            &[&format!(
                "e mais {alem} rejeições sem tratamento além da espera"
            )]
        )
        .is_some(),
        "as {alem} rejeições além do teto da espera não foram contadas no fim da volta — \
             caladas, ou a conta segurada pelo balde atrás das guardadas: {}",
        deixou.rastro
    );
    assert!(
        !deixou.rastro.contains("Error: r39"),
        "a quadragésima rejeição, além do teto da espera, saiu com a linha dela na hora, e \
             não só na conta: uma que o MOD pegasse logo depois sairia em ERROR como «sem \
             tratamento»: {}",
        deixou.rastro
    );
    assert!(
        deixou.linhas <= teto_do_balde(deixou.levou),
        "{} linhas de console em {:?}: as rejeições passaram por fora do balde",
        deixou.linhas,
        deixou.levou
    );
}

/// **Um `Promise.allSettled` de mais promessas que o teto da espera, que
/// lançam antes do primeiro `await`, não deixa linha de erro.**
///
/// Cada função `async` rejeita a promessa dela antes de o `allSettled`
/// prender o tratamento, e o motor avisa cada rejeição nessa hora. As que
/// cabem na espera saem dela quando o tratamento chega; as de depois, que
/// só foram contadas, saem da conta. Ditas na hora, como antes, elas
/// saíam em ERROR como «sem tratamento» — oito linhas sobre rejeições que o
/// MOD tratou (F-Mods-m1 da revisão final do Plano 1).
#[test]
fn um_allsettled_alem_do_teto_da_espera_nao_vira_linha_de_erro() {
    let deixou = deixou(
        "const lanca = async (i) => { throw new Error('tratada ' + i); };\
             Promise.allSettled(Array.from({ length: 40 }, (_, i) => lanca(i)))\
               .then((r) => seele.postar('assentou ' + \
                 r.filter((x) => x.status === 'rejected').length));\
             setTimeout(() => seele.postar('fim'), 0);",
        |_| {},
        |falas, _| tem_mensagem(falas, "fim"),
        Duration::from_secs(5),
    );
    assert!(
        tem_mensagem(&deixou.falas, "assentou 40") && tem_mensagem(&deixou.falas, "fim"),
        "o `allSettled` não assentou as quarenta rejeições, ou o MOD não chegou ao fim, e o \
             teste não mediu nada: {:?}",
        deixou.falas
    );
    assert!(
        !deixou.rastro.contains("tratada "),
        "uma rejeição que o `allSettled` tratou saiu no seele.log como «sem tratamento»: \
             quem escreveu o MOD vai procurar um defeito que não existe: {}",
        deixou.rastro
    );
    assert!(
        !deixou.rastro.contains("além da espera"),
        "as rejeições além do teto da espera foram tratadas pelo `allSettled`, e a conta \
             delas saiu mesmo assim: {}",
        deixou.rastro
    );
}

/// **Pelo mesmo balde do `console`.** Um MOD que lança em toda resposta
/// lança tanto quanto a janela responde; sem o balde, cada volta seria uma
/// linha no `seele.log`, num arquivo que só gira na abertura do app.
#[test]
fn uma_excecao_em_cada_resposta_nao_passa_do_balde_do_console() {
    let deixou = deixou(
        "globalThis.aoResponder = () => { const a = null; a.x; };",
        |executor| {
            let fim = Instant::now() + Duration::from_millis(600);
            while Instant::now() < fim {
                let _ = executor.entregar("{}");
                std::thread::yield_now();
            }
        },
        |_, _| false,
        Duration::from_millis(900),
    );
    assert!(
        linha_de_erro(&deixou.rastro, &["o MOD lançou", "TypeError"]).is_some(),
        "a exceção de uma resposta não chegou ao seele.log com o id e o texto: {}",
        deixou.rastro
    );
    let teto = teto_do_balde(deixou.levou);
    assert!(
        deixou.linhas <= teto,
        "{} linhas de console em {:?}, e o balde deixa {teto}: as exceções passaram por fora \
             dele",
        deixou.linhas,
        deixou.levou
    );
    assert!(
        deixou.rastro.contains("suprimidas="),
        "o balde não segurou nada, e o teste não mediu o teto: {}",
        deixou.rastro
    );
}

/// **Um temporizador que lança deixa linhas com o texto, no teto do
/// balde.** O mesmo laço de 4 ms mandava 174 mensagens por segundo à
/// janela, que as descartava.
///
/// **E com a pilha**, que diz onde no MOD: o `emTexto` do prelúdio a põe
/// numa linha abaixo de `nome: mensagem`, e no `seele.log` a quebra sai
/// escapada (`\n    at …`).
///
/// # Quem seca o balde é a conta dos temporizadores, e não o relógio
///
/// São `2 × RAJADA_DO_CONSOLE` laços de 4 ms, e não um. Com um só, secar o
/// balde dependia de o relógio da máquina dispará-lo mais de 32 vezes num
/// segundo: no macOS do runner do `ci.yml` ele disparou perto de 30
/// (12/11/2026), as 30 linhas couberam na rajada, e o teste reprovou o balde
/// por «não segurar nada» — de linhas que ninguém tinha escrito. Com o dobro
/// da rajada de pé, **a primeira volta** em que eles vencem já lança o dobro
/// do que o balde deixa passar, dispare o relógio quantas vezes disparar
/// depois.
#[test]
fn um_temporizador_que_lanca_deixa_linhas_limitadas_pelo_balde() {
    let lacos = 2 * RAJADA_DO_CONSOLE;
    let deixou = deixou(
        &format!(
            "for (let i = 0; i < {lacos}; i++) setInterval(() => {{ const a = null; a.b; }}, 4);"
        ),
        |_| {},
        |_, _| false,
        Duration::from_secs(1),
    );
    // Cada disparo que lança manda um `erro-no-relogio` à janela, e a janela
    // não passa pelo balde: é a conta do que o MOD de fato tentou escrever.
    let disparos = deixou
        .falas
        .iter()
        .filter(
            |fala| matches!(fala, ParaOFora::Mensagem(json) if json.contains("erro-no-relogio")),
        )
        .count();
    let linha = linha_de_erro(&deixou.rastro, &["erro num temporizador", "TypeError"])
        .unwrap_or_else(|| {
            panic!(
                "a exceção de um temporizador não chegou ao seele.log com o id e o texto: {}",
                deixou.rastro
            )
        });
    assert!(
        linha.contains(r"\n    at "),
        "a linha do erro de um temporizador perdeu a pilha, e quem escreveu o MOD não sabe \
             em que ponto dele o temporizador lançou: {linha}"
    );
    assert!(
        tem_mensagem(&deixou.falas, "erro-no-relogio"),
        "a mensagem `erro-no-relogio` deixou de ir à janela, e ela é do outro lado: {:?}",
        deixou.falas.first()
    );
    let teto = teto_do_balde(deixou.levou);
    assert!(
        deixou.linhas <= teto,
        "{} linhas de console em {:?} ({disparos} disparos), e o balde deixa {teto}: o laço \
             do temporizador passou por fora dele",
        deixou.linhas,
        deixou.levou
    );
    // **Primeiro o arranjo, e só depois o balde.** Sem mais disparos do que a
    // rajada não havia o que segurar, e a frase «o balde não segurou nada»
    // acusaria o produto de uma coisa que o teste não chegou a pedir.
    assert!(
        disparos > RAJADA_DO_CONSOLE as usize,
        "o arranjo do teste não secou o balde: {lacos} laços de 4 ms lançaram só {disparos} \
             vez(es) em {:?}, e a rajada é de {RAJADA_DO_CONSOLE} — nesta máquina os \
             temporizadores do MOD quase não dispararam. {} linhas de console: {}",
        deixou.levou,
        deixou.linhas,
        deixou.rastro
    );
    assert!(
        deixou.rastro.contains("suprimidas="),
        "o balde não segurou nada em {disparos} disparos (a rajada é de {RAJADA_DO_CONSOLE}), \
             e o teste não mediu o teto: {} linhas de console em {:?}: {}",
        deixou.linhas,
        deixou.levou,
        deixou.rastro
    );
}

/// **Um ouvinte de evento que lança deixa a linha, com a pilha** — como o
/// temporizador, e pelo mesmo `emTexto`.
#[test]
fn um_ouvinte_de_evento_que_lanca_deixa_a_linha() {
    let deixou = deixou(
        "SeeleUI.aoEvento(() => { const a = null; a.b; });",
        |executor| {
            executor
                .entregar(r#"{"tipo":"evento","nome":"tecla"}"#)
                .expect("o evento");
        },
        |falas, linhas| tem_mensagem(falas, "erro-no-evento") && linhas > 0,
        Duration::from_secs(5),
    );
    let linha = linha_de_erro(&deixou.rastro, &["erro num ouvinte de evento", "TypeError"])
        .unwrap_or_else(|| {
            panic!(
                "a exceção de um ouvinte de `SeeleUI.aoEvento` não chegou ao seele.log com o \
                     id e o texto: {}",
                deixou.rastro
            )
        });
    assert!(
        linha.contains(r"\n    at "),
        "a linha do erro de um ouvinte de evento perdeu a pilha, e quem escreveu o MOD não \
             sabe em que ponto dele o ouvinte lançou: {linha}"
    );
    assert!(
        tem_mensagem(&deixou.falas, "erro-no-evento"),
        "a mensagem `erro-no-evento` deixou de ir à janela, e ela é do outro lado: {:?}",
        deixou.falas
    );
}

/// **A volta que o produto parou não vira «o MOD lançou» no registro.**
///
/// O laço infinito é dos erros mais comuns de quem escreve MOD, e o
/// `seele.log` dizia `o MOD lançou: InternalError: interrupted` — quem
/// parou foi o produto, pelo prazo ou pelo teto de trabalho. Com os tetos
/// do produto, o que para primeiro depende da máquina, e por isso a frase
/// exigida é a que as duas paradas dizem.
#[test]
fn uma_volta_que_o_produto_parou_nao_vira_o_mod_lancou_no_registro() {
    let deixou = deixou(
        "while (true) {}",
        |_| {},
        |falas, linhas| !falas.is_empty() && linhas > 0,
        Duration::from_secs(5),
    );
    assert!(
        linha_de_erro(
            &deixou.rastro,
            &["o produto a interrompeu (o MOD continua de pé)"]
        )
        .is_some(),
        "a volta que o produto parou não chegou ao seele.log com o id e dizendo que só ela \
             foi interrompida — sem isso, quem lê entende que o MOD acabou: {}",
        deixou.rastro
    );
    assert!(
        !deixou.rastro.contains("lançou"),
        "a volta que o produto parou foi dita no seele.log como se o MOD tivesse lançado: {}",
        deixou.rastro
    );
}
