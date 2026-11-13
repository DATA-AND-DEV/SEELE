use std::time::{Duration, Instant};

use super::{
    midia_servida_dita, midia_vinda_do_servidor_dita, recusa_de_midia_dita, registrar_da_janela_em,
    som_servido_dito, BaldesDoRegistro, Classe, FalhaNoMod, Session, BALDES_NO_REGISTRO,
};
use crate::executor::RAJADA_DO_CONSOLE;
use crate::rastro_de_teste::capturar;
use crate::relogio_de_teste::origem;

/// Um aviso da janela em nome de `id`, no instante `agora`.
fn da_janela(baldes: &BaldesDoRegistro, agora: Instant, id: &str) {
    da_janela_no(baldes, agora, "aviso", id);
}

/// Uma linha da janela no `nivel` que a janela manda.
fn da_janela_no(baldes: &BaldesDoRegistro, agora: Instant, nivel: &str, id: &str) {
    registrar_da_janela_em(
        baldes,
        agora,
        nivel.to_owned(),
        "atender-mod".to_owned(),
        "uma frase".to_owned(),
        Some(id.to_owned()),
    );
}

/// Uma mídia de `id` não servida porque a sessão acabou: a linha DEBUG de
/// [`recusa_de_midia_dita`].
fn nao_servida(baldes: &BaldesDoRegistro, agora: Instant, id: &str) {
    recusa_de_midia_dita(
        baldes,
        agora,
        id,
        "img/rosto.png",
        FalhaNoMod::Recusado {
            motivo: "sessao-encerrada".to_owned(),
        },
    );
}

/// Uma recusa de mídia de `id`, no instante `agora`.
fn recusa(baldes: &BaldesDoRegistro, agora: Instant, id: &str) {
    recusa_de_midia_dita(
        baldes,
        agora,
        id,
        "img/rosto.png",
        FalhaNoMod::Recusado {
            motivo: "arquivo-nao-declarado".to_owned(),
        },
    );
}

/// As linhas do rastro com `mod_id=<id>` num campo.
fn de<'a>(rastro: &'a str, id: &str) -> Vec<&'a str> {
    let campo = format!("mod_id={id}");
    rastro
        .lines()
        .filter(|linha| linha.split_whitespace().any(|pedaco| pedaco == campo))
        .collect()
}

/// Um quarto de segundo: a recarga de uma ficha do balde.
const UMA_FICHA: Duration = Duration::from_millis(250);

#[test]
fn duzentas_linhas_de_um_mod_no_mesmo_instante_deixam_so_a_rajada() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..200 {
            da_janela(&baldes, agora, "a/b");
        }
    });
    assert_eq!(
        de(&rastro, "a/b").len(),
        RAJADA_DO_CONSOLE as usize,
        "duzentas linhas da janela no mesmo instante passaram de outro jeito que a rajada: \
             um MOD num laço escreve sem teto no seele.log de quem hospeda"
    );
}

/// **O que o balde segura é contado**, e a primeira linha que passa depois
/// leva a conta: o registro diz que houve mais do que ele mostra.
#[test]
fn a_linha_depois_do_aperto_diz_quantas_ficaram_para_tras() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..200 {
            da_janela(&baldes, agora, "a/b");
        }
        da_janela(&baldes, agora + UMA_FICHA, "a/b");
    });
    let linhas = de(&rastro, "a/b");
    let seguradas = 200 - RAJADA_DO_CONSOLE as usize;
    assert_eq!(
        linhas.len(),
        RAJADA_DO_CONSOLE as usize + 1,
        "um quarto de segundo depois da rajada, a linha seguinte não passou (ou passou mais \
             de uma): {rastro}"
    );
    let ultima = linhas.last().copied().unwrap_or_default();
    assert!(
        ultima.contains(&format!("suprimidas={seguradas}")),
        "a linha que passou depois do aperto não diz as {seguradas} que o balde segurou, e o \
             registro mostra a rajada como se fosse tudo: {ultima}"
    );
    assert!(
        linhas
            .iter()
            .take(RAJADA_DO_CONSOLE as usize)
            .all(|linha| !linha.contains("suprimidas")),
        "uma linha da rajada trouxe `suprimidas` sem nada ter sido segurado: {rastro}"
    );
}

#[test]
fn um_mod_nao_segura_as_linhas_de_outro() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..200 {
            da_janela(&baldes, agora, "a/b");
        }
        da_janela(&baldes, agora, "c/d");
    });
    assert_eq!(
        de(&rastro, "c/d").len(),
        1,
        "a linha de `c/d` foi segurada pelo balde de `a/b`: um MOD num laço cala o \
             diagnóstico de todos os outros"
    );
}

/// **Com o mapa cheio, um id novo gasta do balde de transbordo da classe
/// dele**, e nunca passa sem teto: o mapa não cresce, e o balde é um só
/// para todos os ids da classe que chegaram depois. E é um por classe: as
/// linhas servidas que transbordam não gastam a ficha de um aviso que
/// transborda.
#[test]
fn com_o_mapa_cheio_um_id_novo_passa_pelo_transbordo() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        // Um aviso de um id novo por balde próprio que o mapa aceita — e
        // não além: o que passasse disso gastaria do transbordo antes do
        // id medido.
        for n in 0..BALDES_NO_REGISTRO - Classe::TODAS.len() {
            da_janela(&baldes, agora, &format!("cheio/{n}"));
        }
        for _ in 0..200 {
            midia_servida_dita(&baldes, agora, "servida/x", 7, "img/a.png", "imagem", 8);
        }
        for _ in 0..200 {
            da_janela(&baldes, agora, "novo/x");
        }
        da_janela(&baldes, agora, "novo/y");
        for n in 0..BALDES_NO_REGISTRO {
            da_janela(&baldes, agora, &format!("mais/{n}"));
        }
        da_janela_no(&baldes, agora, "depuracao", "depuracao/x");
    });
    for id in ["servida/x", "novo/x"] {
        let do_novo = de(&rastro, id).len();
        assert!(
            (1..=RAJADA_DO_CONSOLE as usize).contains(&do_novo),
            "com o mapa cheio, `{id}` deixou {do_novo} linhas em duzentas: zero cala um MOD \
                 que nunca escreveu — ou outra classe secou o transbordo desta —, e mais que a \
                 rajada é um id novo passando sem teto"
        );
    }
    let depois: usize = std::iter::once("novo/y".to_owned())
        .chain((0..BALDES_NO_REGISTRO).map(|n| format!("mais/{n}")))
        .map(|id| de(&rastro, &id).len())
        .sum();
    assert_eq!(
        depois, 0,
        "os ids novos seguintes ganharam balde cheio, e não o transbordo que o primeiro \
             secou: uma janela que muda de id a cada linha escreve sem teto: {rastro}"
    );
    let guardados = baldes.lock().expect("os baldes").len();
    assert_eq!(
        guardados, BALDES_NO_REGISTRO,
        "o mapa dos baldes não ficou em {BALDES_NO_REGISTRO} entradas com as três classes \
             transbordando: passar delas faz de uma janela que manda um id novo por linha \
             memória sem teto"
    );
}

/// **A sessão que fecha diz a conta que nenhuma linha levou**, uma linha
/// por balde — o do catálogo também —, e esvazia o mapa.
#[test]
fn revogar_diz_a_conta_que_nenhuma_linha_levou() {
    let sessao = Session::default();
    let agora = origem();
    let a_mais = 8;
    let ((), rastro) = capturar(|| {
        for _ in 0..RAJADA_DO_CONSOLE as usize + a_mais {
            da_janela(&sessao.baldes_do_registro, agora, "a/b");
            registrar_da_janela_em(
                &sessao.baldes_do_registro,
                agora,
                "aviso".to_owned(),
                "pedido-de-mod".to_owned(),
                "o catálogo".to_owned(),
                None,
            );
        }
        sessao.revogar();
    });
    let contas: Vec<&str> = rastro
        .lines()
        .filter(|linha| linha.contains("seguradas pelo teto de vazão"))
        .collect();
    assert_eq!(
        contas.len(),
        2,
        "a sessão fechou com linhas seguradas em dois baldes e não disse uma conta por \
             balde: {rastro}"
    );
    for pedaco in ["mod_id=a/b", "catalogo=true"] {
        assert!(
            contas
                .iter()
                .any(|conta| conta.contains(pedaco)
                    && conta.contains(&format!("suprimidas={a_mais}"))),
            "a sessão fechou sem dizer as {a_mais} linhas seguradas de `{pedaco}`: a conta \
                 caiu com o balde, e o seele.log mostra a rajada como se fosse tudo: {rastro}"
        );
    }
    assert!(
        sessao
            .baldes_do_registro
            .lock()
            .expect("os baldes")
            .is_empty(),
        "os baldes atravessaram a sessão: a seguinte começa com o teto gasto pela anterior"
    );
}

/// **A mídia vinda do servidor tem teto, na classe da depuração.** É o par
/// bem-sucedido da mídia recusada de `midia_em_bytes`, em DEBUG: com
/// `RUST_LOG=debug`, um MOD que pede a mesma mídia num laço escrevia uma
/// linha por pedido. Na depuração, e não nas servidas: o filtro padrão não
/// a escreve, e ela não pode gastar a ficha de uma linha que ele escreve.
#[test]
fn a_midia_vinda_do_servidor_tem_teto_na_classe_da_depuracao() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..200 {
            midia_vinda_do_servidor_dita(&baldes, agora, "a/b", 7, "imagem", 8);
        }
        recusa(&baldes, agora, "a/b");
        midia_servida_dita(&baldes, agora, "a/b", 7, "img/a.png", "imagem", 8);
        midia_vinda_do_servidor_dita(&baldes, agora + UMA_FICHA, "a/b", 7, "imagem", 8);
    });
    let vindas: Vec<&str> = rastro
        .lines()
        .filter(|linha| linha.contains("mídia de MOD vinda do servidor"))
        .collect();
    assert_eq!(
        vindas.len(),
        RAJADA_DO_CONSOLE as usize + 1,
        "duzentas mídias vindas do servidor no mesmo instante passaram de outro jeito que a \
             rajada, e a seguinte um quarto de segundo depois: com RUST_LOG=debug, um MOD num \
             laço escreve sem teto"
    );
    assert!(
        vindas
            .iter()
            .all(|linha| linha.split_whitespace().next() == Some("DEBUG")),
        "a mídia vinda do servidor deixou de sair em DEBUG: {rastro}"
    );
    let recusada = rastro
        .lines()
        .find(|linha| linha.contains("mídia de MOD recusada"));
    assert!(
        recusada.is_some_and(|linha| linha.contains("mod_id=a/b") && !linha.contains("suprimidas")),
        "as mídias vindas do servidor gastaram a ficha dos avisos do mesmo MOD, e a recusa \
             sumiu — o defeito de 23/09 na direção oposta: {rastro}"
    );
    assert!(
        rastro.contains("mídia de MOD servida"),
        "as mídias vindas do servidor gastaram a ficha das servidas do mesmo MOD: {rastro}"
    );
    let seguradas = 200 - RAJADA_DO_CONSOLE as usize;
    assert!(
        vindas
            .last()
            .is_some_and(|linha| linha.contains(&format!("suprimidas={seguradas}"))),
        "a mídia vinda do servidor que passou depois do aperto não diz as {seguradas} que o \
             balde segurou: {rastro}"
    );
}

/// **Os comandos passam às portas os baldes da sessão.**
///
/// Os testes acima medem as portas com baldes que eles mesmos criam; os
/// comandos precisam de `AppHandle`, de conexão ou de sessão e não sobem
/// num teste. Um `&BaldesDoRegistro::default()` dentro de um comando
/// compila, deixa os testes acima verdes e tira o teto: cada chamada
/// ganharia um balde cheio. Aqui, toda chamada de porta nos cinco comandos
/// tem de levar `&session.baldes_do_registro` no primeiro argumento.
///
/// Em `ler_imagem_mod` as duas são a conexão ausente
/// (`recusa_de_midia_dita`) e `imagem_lida_dita`, que leva as duas recusas
/// de depois do `await`.
#[test]
fn os_comandos_passam_os_baldes_da_sessao() {
    const PORTAS: [&str; 9] = [
        "registrar_da_janela_em(",
        "midia_declarada_do_mod(",
        "midia_servida_dita(",
        "som_declarado_do_mod(",
        "som_servido_dito(",
        "midia_do_servidor_dita(",
        "midia_vinda_do_servidor_dita(",
        "recusa_de_midia_dita(",
        "imagem_lida_dita(",
    ];
    let fonte = super::fonte_sem_comentarios();
    for (comando, chamadas_minimas) in [
        ("fn registrar_da_janela(", 1),
        ("fn midia_do_mod(", 2),
        ("fn som_do_mod(", 2),
        ("fn midia_em_bytes(", 2),
        ("async fn ler_imagem_mod(", 2),
    ] {
        let codigo: String = super::corpo_de(&fonte, comando)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let mut chamadas = 0;
        for porta in PORTAS {
            for depois in codigo.split(porta).skip(1) {
                chamadas += 1;
                assert!(
                    depois.starts_with("&session.baldes_do_registro,"),
                    "`{comando}` chama `{porta}` sem `&session.baldes_do_registro`: com \
                         outros baldes, a porta perde o teto de vazão da sessão"
                );
            }
        }
        assert!(
            chamadas >= chamadas_minimas,
            "`{comando}` chama {chamadas} das portas medidas, e chamava \
                 {chamadas_minimas}: a linha dele saiu por fora delas, e do teto"
        );
    }
}

/// **A linha que passa pelo transbordo diz que passou.** A conta que ela
/// leva é do balde dividido — de todos os ids que caíram nele —, e sem o
/// campo ela se lia como sendo só do id da linha.
#[test]
fn a_linha_que_passa_pelo_transbordo_diz_que_passou() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for n in 0..BALDES_NO_REGISTRO - Classe::TODAS.len() {
            da_janela(&baldes, agora, &format!("cheio/{n}"));
        }
        da_janela(&baldes, agora, "novo/x");
        da_janela(&baldes, agora, "cheio/0");
    });
    assert!(
        de(&rastro, "novo/x")
            .iter()
            .all(|linha| linha.contains("transbordo=true"))
            && !de(&rastro, "novo/x").is_empty(),
        "a linha que passou pelo balde dividido não diz `transbordo=true`, e a conta dela se \
             lê como sendo só do id dela: {rastro}"
    );
    assert!(
        de(&rastro, "cheio/0")
            .iter()
            .all(|linha| !linha.contains("transbordo")),
        "uma linha do balde próprio do MOD saiu como transbordo: {rastro}"
    );
}

/// **Um id que o filtro esvazia é ilegível, e não o catálogo**: o balde e
/// o rótulo dele são os de `mod_id_ilegivel`, como o terceiro braço do
/// pedido de MOD. No balde do catálogo, ele gastava as fichas do catálogo
/// e a conta dele saía como `catalogo=true`.
#[test]
fn um_id_que_o_filtro_esvazia_conta_como_ilegivel_e_nao_como_catalogo() {
    let sessao = Session::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..RAJADA_DO_CONSOLE as usize + 2 {
            da_janela(&sessao.baldes_do_registro, agora, "\u{202E}\n");
        }
        registrar_da_janela_em(
            &sessao.baldes_do_registro,
            agora,
            "aviso".to_owned(),
            "pedido-de-mod".to_owned(),
            "o catálogo".to_owned(),
            None,
        );
        sessao.revogar();
    });
    assert!(
        rastro.contains("o catálogo"),
        "a linha do catálogo foi segurada pelas linhas de um id ilegível: {rastro}"
    );
    let contas: Vec<&str> = rastro
        .lines()
        .filter(|linha| linha.contains(super::CONTA_DO_BALDE_DO_REGISTRO))
        .collect();
    assert!(
        contas.len() == 1
            && contas
                .iter()
                .all(|conta| conta.contains("mod_id_ilegivel=true")
                    && conta.contains("suprimidas=2")
                    && !conta.contains("catalogo")),
        "a conta de um id que o filtro esvazia não saiu como `mod_id_ilegivel=true`: o \
             rótulo `catalogo=true` é de um pedido que não é de MOD nenhum: {rastro}"
    );
}

/// **A saída do app diz a conta que nenhuma linha levou.**
///
/// Fechar a janela não passava por `Session::revogar`, e a conta pendente de
/// um balde caía com o processo: o `seele.log` da última sessão mostrava a
/// rajada como se fosse tudo. `despedir_da_sessao` revoga primeiro, e
/// revogar diz as contas.
#[test]
fn despedir_da_sessao_diz_a_conta_que_nenhuma_linha_levou() {
    let sessao = Session::default();
    let agora = origem();
    let a_mais = 5;
    let ((), rastro) = capturar(|| {
        for _ in 0..RAJADA_DO_CONSOLE as usize + a_mais {
            da_janela(&sessao.baldes_do_registro, agora, "a/b");
        }
        super::despedir_da_sessao(&sessao, Duration::from_millis(20));
    });
    assert!(
        rastro
            .lines()
            .any(|linha| linha.contains(super::CONTA_DO_BALDE_DO_REGISTRO)
                && linha.contains("mod_id=a/b")
                && linha.contains(&format!("suprimidas={a_mais}"))),
        "a saída do app não disse as {a_mais} linhas seguradas do balde de `a/b`: a conta \
             caiu com o processo, e o seele.log mostra a rajada como se fosse tudo: {rastro}"
    );
}

/// **A conta também é dita sem fechar a sessão**, por
/// `dizer_as_contas_do_registro`. Ela diz uma vez e esvazia os baldes: chamada de novo, não diz nada, e a
/// geração não anda.
#[test]
fn dizer_as_contas_do_registro_diz_uma_vez_e_esvazia() {
    let sessao = Session::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..RAJADA_DO_CONSOLE as usize + 3 {
            da_janela(&sessao.baldes_do_registro, agora, "a/b");
        }
        sessao.dizer_as_contas_do_registro();
        sessao.dizer_as_contas_do_registro();
    });
    let contas: Vec<&str> = rastro
        .lines()
        .filter(|linha| linha.contains(super::CONTA_DO_BALDE_DO_REGISTRO))
        .collect();
    assert!(
        contas.len() == 1 && contas.iter().all(|conta| conta.contains("suprimidas=3")),
        "as contas não foram ditas uma vez, com as três linhas seguradas: a saída do app \
             perde a conta, ou a diz duas vezes e o registro as soma: {rastro}"
    );
    assert!(
        sessao
            .baldes_do_registro
            .lock()
            .expect("os baldes")
            .is_empty(),
        "dizer as contas não esvaziou os baldes"
    );
    assert_eq!(
        sessao.geracao(),
        0,
        "dizer as contas do registro fechou a sessão: ela só diz as contas, e quem revoga \
             é `revogar`"
    );
}

/// **A conta é dita com o cadeado dos baldes solto.**
///
/// Escrita com ele na mão, uma linha que demora no disco segura toda porta
/// do registro de todo MOD, em toda thread, até terminar — e um assinante
/// que escrevesse pelo registro dentro do evento travaria no mesmo
/// cadeado. A camada abaixo tenta o cadeado a cada evento, na thread que
/// escreve: com a guarda na mão, o `try_lock` volta `WouldBlock`.
#[test]
fn a_conta_e_dita_com_o_cadeado_dos_baldes_solto() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tracing_subscriber::layer::SubscriberExt as _;

    /// Conta os eventos que acharam o cadeado livre e os que o acharam
    /// preso.
    struct TentaOCadeado {
        baldes: &'static BaldesDoRegistro,
        livre: Arc<AtomicUsize>,
        preso: Arc<AtomicUsize>,
    }

    /// A mensagem do evento é a da conta dos baldes?
    struct EAConta(bool);

    impl tracing::field::Visit for EAConta {
        fn record_debug(&mut self, campo: &tracing::field::Field, valor: &dyn std::fmt::Debug) {
            if campo.name() == "message"
                && format!("{valor:?}") == super::CONTA_DO_BALDE_DO_REGISTRO
            {
                self.0 = true;
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for TentaOCadeado {
        fn on_event(
            &self,
            evento: &tracing::Event<'_>,
            _contexto: tracing_subscriber::layer::Context<'_, S>,
        ) {
            // Só a linha da conta: qualquer outra que o `revogar` passe a
            // escrever não pode fazer as vezes dela.
            let mut e_a_conta = EAConta(false);
            evento.record(&mut e_a_conta);
            if !e_a_conta.0 {
                return;
            }
            let contador = if self.baldes.try_lock().is_ok() {
                &self.livre
            } else {
                &self.preso
            };
            contador.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Como em `capturar`: com um segundo despachante vivo, o interesse da
    // linha da conta não fica preso ao de outra thread que passou por ela
    // antes, sem assinante.
    crate::rastro_de_teste::segundo_despachante();
    // `'static`, porque o assinante é: a sessão de um teste só.
    let sessao: &'static Session = Box::leak(Box::default());
    let agora = origem();
    for _ in 0..RAJADA_DO_CONSOLE as usize + 1 {
        da_janela(&sessao.baldes_do_registro, agora, "a/b");
    }
    let livre = Arc::new(AtomicUsize::new(0));
    let preso = Arc::new(AtomicUsize::new(0));
    let assinante = tracing_subscriber::registry().with(TentaOCadeado {
        baldes: &sessao.baldes_do_registro,
        livre: Arc::clone(&livre),
        preso: Arc::clone(&preso),
    });
    tracing::subscriber::with_default(assinante, || {
        sessao.revogar();
    });
    assert_eq!(
        preso.load(Ordering::Relaxed),
        0,
        "a conta dos baldes foi escrita com o cadeado deles na mão: uma linha lenta no disco \
             segura o registro de todo MOD até terminar"
    );
    assert_eq!(
        livre.load(Ordering::Relaxed),
        1,
        "a sessão fechou com um balde de linhas seguradas e não disse a conta dele uma vez"
    );
}

/// **A recusa de mídia tem o mesmo teto**, e a mesma conta.
#[test]
fn a_recusa_de_midia_tem_o_mesmo_teto_e_a_mesma_conta() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..200 {
            recusa(&baldes, agora, "a/b");
        }
        recusa(&baldes, agora + UMA_FICHA, "a/b");
    });
    let linhas = de(&rastro, "a/b");
    assert_eq!(
        linhas.len(),
        RAJADA_DO_CONSOLE as usize + 1,
        "duzentas recusas de mídia do mesmo MOD no mesmo instante passaram de outro jeito \
             que a rajada, e a seguinte um quarto de segundo depois: {rastro}"
    );
    let seguradas = 200 - RAJADA_DO_CONSOLE as usize;
    assert!(
        linhas
            .last()
            .is_some_and(|linha| linha.contains(&format!("suprimidas={seguradas}"))),
        "a recusa que passou depois do aperto não diz as {seguradas} que o balde segurou: \
             {rastro}"
    );
}

/// **Dentro de cada classe, as portas do mesmo MOD dividem o balde dele**:
/// é um teto por MOD e por classe, e não um por porta — senão um MOD teria
/// uma rajada por porta. Os avisos são a janela em aviso e a mídia
/// recusada; as servidas, a mídia e o som; a depuração, a janela em DEBUG e
/// a mídia não servida porque a sessão acabou.
#[test]
fn as_portas_do_mesmo_mod_dividem_o_balde() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let por_porta = RAJADA_DO_CONSOLE as usize / 2;
    let ((), rastro) = capturar(|| {
        for _ in 0..=por_porta {
            da_janela(&baldes, agora, "a/b");
            recusa(&baldes, agora, "a/b");
            midia_servida_dita(&baldes, agora, "a/b", 7, "img/rosto.png", "imagem", 8);
            som_servido_dito(&baldes, agora, "a/b", 7, "som/toque.wav", 16);
            da_janela_no(&baldes, agora, "depuracao", "a/b");
            nao_servida(&baldes, agora, "a/b");
        }
    });
    let linhas = de(&rastro, "a/b");
    for (classe, nivel) in [
        ("aviso", "WARN"),
        ("servida", "INFO"),
        ("depuração", "DEBUG"),
    ] {
        let da_classe = linhas
            .iter()
            .filter(|linha| linha.split_whitespace().next() == Some(nivel))
            .count();
        assert_eq!(
            da_classe, RAJADA_DO_CONSOLE as usize,
            "as duas portas da classe {classe} do mesmo MOD não deixaram uma rajada só: ou \
                 cada porta tem a dela, e o teto de um MOD é o dobro do escrito, ou outra classe \
                 gastou as fichas desta: {rastro}"
        );
    }
}

/// **Um aviso não é segurado pelas linhas servidas do mesmo MOD.**
///
/// Segurada, uma linha não atrasa: ela some, e só a conta segue, na
/// próxima linha do MOD — que pode ser outra «mídia de MOD servida». Um MOD
/// que monta mais de 32 mídias de uma vez gastava a rajada em INFO, e a
/// recusa da seguinte sumia: o defeito de 23/09, que o módulo da mídia
/// recusada conta, de volta pelo teto.
#[test]
fn uma_recusa_depois_de_quarenta_servidas_do_mesmo_mod_sai() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for n in 0..40 {
            midia_servida_dita(
                &baldes,
                agora,
                "a/b",
                7,
                &format!("img/{n}.png"),
                "imagem",
                8,
            );
        }
        recusa(&baldes, agora, "a/b");
    });
    let recusada = rastro
        .lines()
        .find(|linha| linha.contains("mídia de MOD recusada"));
    assert!(
        recusada.is_some_and(|linha| linha.contains("mod_id=a/b")),
        "quarenta mídias servidas do mesmo MOD gastaram a ficha da recusa, e ela sumiu do \
             seele.log — a mídia recusada em silêncio de novo: {rastro}"
    );
    assert!(
        recusada.is_some_and(|linha| !linha.contains("suprimidas")),
        "a recusa levou na conta as mídias servidas que o balde delas segurou, e quem lê \
             acha que avisos sumiram: {rastro}"
    );
}

/// **A depuração não gasta a ficha de um aviso, nem infla a conta dele.**
/// O DEBUG não está no `seele.log` com o filtro padrão: segurar um aviso
/// por causa dele era calar o que se lê pelo que não se lê.
#[test]
fn a_depuracao_nao_gasta_a_ficha_nem_infla_a_conta_de_um_aviso() {
    let baldes = BaldesDoRegistro::default();
    let agora = origem();
    let ((), rastro) = capturar(|| {
        for _ in 0..40 {
            da_janela_no(&baldes, agora, "depuracao", "a/b");
        }
        da_janela(&baldes, agora, "a/b");
    });
    let aviso = rastro
        .lines()
        .find(|linha| linha.split_whitespace().next() == Some("WARN"));
    assert!(
        aviso.is_some_and(|linha| linha.contains("mod_id=a/b") && !linha.contains("suprimidas")),
        "quarenta linhas DEBUG da janela seguraram o aviso do mesmo MOD, ou entraram na \
             conta dele: {rastro}"
    );
}
