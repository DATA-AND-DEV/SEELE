use super::{
    liberar_reserva, registrar_reserva, revogar_em, EstadoNativo, InstanciaNativa, ModsNativos,
};
use crate::executor::{ExecutorQuickJs, Limites};

/// Uma instância com um executor de verdade, na geração dada.
fn instancia(geracao: u64) -> InstanciaNativa {
    InstanciaNativa {
        id: "prova/mod".to_owned(),
        geracao,
        hash: "a".repeat(64),
        capacidades: seele_ffi::mods::capacidades_da_api(seele_ffi::mods::MOD_API_VERSION).to_vec(),
        estado: EstadoNativo::Ativa,
        executor: ExecutorQuickJs::novo(Limites::default()).expect("motor"),
        pendentes: std::collections::VecDeque::new(),
        bytes_pendentes: 0,
        avisos: std::collections::VecDeque::new(),
        avisos_perdidos: 0,
        parou: false,
        codigo: Some("/* o fonte do MOD */".to_owned()),
        avisado: false,
    }
}

/// Enche a fila de mensagens como a bomba encheria, reservando crédito.
fn produzir(instancia: &mut InstanciaNativa, quantas: usize, bytes: usize) {
    for _ in 0..quantas {
        let corpo = "x".repeat(bytes);
        assert!(
            instancia.executor.fila().cabe(corpo.len()),
            "a fila recusou antes do esperado"
        );
        instancia.guardar_fala("mensagem", corpo);
    }
}

/// **Sair com mensagem na fila solta tudo, e sem a janela.**
///
/// A revisão: «sair com uma mensagem ainda na fila deixa dados e a
/// instância retidos mesmo depois de `Parou`, sem caminho de colheita
/// válido» — porque a colheita recusa a geração que saiu.
///
/// Provar que a fila parou de crescer não basta: **ela precisa ser
/// liberada**.
#[test]
fn revogar_solta_o_que_a_janela_nao_colheu() {
    let mut mods = ModsNativos::default();
    let mut viva = instancia(7);
    produzir(&mut viva, 20, 1024);
    let (antes, bytes_antes) = viva.executor.fila().ocupacao();
    assert_eq!(antes, 20, "a produção não reservou crédito");
    assert!(bytes_antes > 0);

    let numero = mods.guardar(viva);
    // Ninguém colhe. A sessão acaba.
    mods.revogar_geracao(7);

    // A instância foi para a tabela de encerramento — ela ainda não
    // confirmou — mas **os dados já saíram**.
    let retida = mods
        .encerrando
        .get(&numero)
        .expect("a instância sumiu antes de confirmar");
    assert!(
        retida.pendentes.is_empty() && retida.bytes_pendentes == 0,
        "a revogação não soltou as mensagens não colhidas"
    );
    let (depois, bytes_depois) = retida.executor.fila().ocupacao();
    assert_eq!(
        (depois, bytes_depois),
        (0, 0),
        "os créditos ficaram reservados depois da saída"
    );

    // E quando a parada chega, ela sai da supervisão — sem a janela.
    mods.confirmar_parada(numero);
    assert!(
        mods.vivos.is_empty() && mods.encerrando.is_empty(),
        "a instância continuou supervisionada depois de parar"
    );
}

/// **A saída do app manda os MODs nativos parar, e diz quantos.**
///
/// Fechar a janela não passava por [`super::Session::revogar`]: a
/// despedida era `disconnect` e um `sleep`, e um MOD nativo de pé seguia
/// rodando até o processo morrer, sem nenhuma linha no `seele.log` dizendo
/// que houve MOD algum. Aqui ninguém confirma a parada — não há bomba
/// ouvindo o executor —, então a instância fica onde a revogação a pôs:
/// em `encerrando`, que é o retrato de «mandei parar».
#[test]
fn despedir_da_sessao_manda_os_mods_nativos_parar_e_diz_quantos() {
    let sessao = super::Session::default();
    let mut reservada = instancia(sessao.geracao());
    reservada.estado = EstadoNativo::Reservada;
    let numero = sessao
        .mods_nativos
        .lock()
        .expect("a supervisão")
        .guardar(reservada);

    let ((), rastro) = crate::rastro_de_teste::capturar(|| {
        super::despedir_da_sessao(&sessao, std::time::Duration::from_millis(20));
    });

    let mods = sessao.mods_nativos.lock().expect("a supervisão");
    assert!(
        mods.encerrando.contains_key(&numero) && !mods.vivos.contains_key(&numero),
        "a saída do app não mandou parar o MOD nativo de pé: ele continua entre os vivos, e \
             roda até o processo morrer"
    );
    assert!(
        rastro.lines().any(|linha| linha.contains("INFO")
            && linha.contains("saída do app: MODs nativos mandados parar")
            && linha.contains("mods=1")),
        "a saída do app não disse no seele.log quantos MODs nativos mandou parar: {rastro}"
    );
}

/// Uma instância da geração `geracao` entre as vivas, e o número dela.
fn de_pe(sessao: &super::Session, geracao: u64) -> u64 {
    sessao
        .mods_nativos
        .lock()
        .expect("a supervisão")
        .guardar(instancia(geracao))
}

/// Confirma a parada de `numero` depois de `depois`, tomando o cadeado da
/// supervisão como a bomba de um MOD toma.
fn confirmar_depois(sessao: &super::Session, numero: u64, depois: std::time::Duration) {
    std::thread::sleep(depois);
    sessao
        .mods_nativos
        .lock()
        .expect("a supervisão")
        .confirmar_parada(numero);
}

/// Roda `despedir_da_sessao` com um MOD que confirma a parada 30 ms depois,
/// noutra thread, e devolve quanto ela levou e o rastro.
fn despedir_com_um_mod_que_confirma(
    sessao: &super::Session,
    numero: u64,
    teto: std::time::Duration,
) -> (std::time::Duration, String) {
    std::thread::scope(|escopo| {
        escopo.spawn(|| confirmar_depois(sessao, numero, std::time::Duration::from_millis(30)));
        let comeco = std::time::Instant::now();
        let ((), rastro) =
            crate::rastro_de_teste::capturar(|| super::despedir_da_sessao(sessao, teto));
        (comeco.elapsed(), rastro)
    })
}

/// **A saída do app espera os MODs sem segurar o cadeado da supervisão.**
///
/// A bomba de cada MOD confirma a parada tomando esse cadeado
/// (`assentar_fala`). Uma espera que o segurasse de uma conferência a outra
/// deixaria a confirmação parada atrás dela até o teto, e toda saída do app
/// pagaria o teto inteiro dizendo que os MODs não confirmaram.
#[test]
fn despedir_da_sessao_espera_os_mods_sem_segurar_o_cadeado() {
    let sessao = super::Session::default();
    let numero = de_pe(&sessao, sessao.geracao());
    let teto = std::time::Duration::from_secs(2);

    let (levou, rastro) = despedir_com_um_mod_que_confirma(&sessao, numero, teto);

    assert!(
        levou < teto / 2,
        "a saída do app levou {levou:?} com um MOD que confirmou a parada em 30 ms: a espera \
             segurou o cadeado da supervisão, e a confirmação ficou parada atrás dela até o teto"
    );
    assert!(
        !rastro.contains("sem confirmar a parada"),
        "a saída do app disse que um MOD não confirmou a parada, e ele confirmou em 30 ms: \
             {rastro}"
    );
}

/// **A saída do app conta e espera só os MODs da sessão que ela fechou.**
///
/// Uma instância de uma sessão anterior que nunca confirmou a parada fica em
/// `encerrando` para sempre, de propósito (ver [`ModsNativos::encerrando`]).
/// Contada junto, a linha dizia MODs a mais, e a espera corria sempre até o
/// teto por uma instância que nenhuma saída vai ver parar.
#[test]
fn despedir_da_sessao_conta_e_espera_so_a_geracao_que_revogou() {
    let sessao = super::Session::default();
    let antiga = {
        let mut mods = sessao.mods_nativos.lock().expect("a supervisão");
        let antiga = mods.guardar(instancia(sessao.geracao()));
        mods.encerrar(antiga);
        antiga
    };
    // A sessão da antiga acabou, e ela não confirmou.
    let _ = sessao.revogar();
    let atual = de_pe(&sessao, sessao.geracao());
    let teto = std::time::Duration::from_secs(2);

    let (levou, rastro) = despedir_com_um_mod_que_confirma(&sessao, atual, teto);

    assert!(
        sessao
            .mods_nativos
            .lock()
            .expect("a supervisão")
            .encerrando
            .contains_key(&antiga),
        "a instância antiga, que nunca confirmou, saiu da supervisão: o teste não mede mais \
             o que diz medir"
    );
    assert!(
        rastro.lines().any(
            |linha| linha.contains("saída do app: MODs nativos mandados parar")
                && linha.split_whitespace().any(|campo| campo == "mods=1")
        ),
        "a saída do app não disse `mods=1`, o único MOD que ela mandou parar: contou também \
             a instância de uma sessão anterior que nunca confirmou: {rastro}"
    );
    assert!(
        levou < teto / 2,
        "a saída do app levou {levou:?}, com o MOD da sessão dela confirmando em 30 ms: a \
             espera correu até o teto por outra coisa — a instância de uma sessão anterior, que \
             nunca vai confirmar, ou o cadeado da supervisão preso durante a espera (o teste \
             irmão, `despedir_da_sessao_espera_os_mods_sem_segurar_o_cadeado`, separa os dois)"
    );
}

/// **Uma parada espontânea muda o estado nativo.**
///
/// A revisão: «`confirmar_parada` escreve `parou = true` somente na tabela
/// `encerrando`. Para a tabela `vivos`, ele verifica `i.parou` sem tê-lo
/// alterado». Uma instância cujo motor cai sozinha continuava descrita como
/// ativa para sempre.
#[test]
fn uma_parada_espontanea_tira_a_instancia_de_viva() {
    let mut mods = ModsNativos::default();
    let numero = mods.guardar(instancia(7));
    assert_eq!(mods.vivos.len(), 1);

    // O motor caiu sozinho: ninguém pediu para encerrar.
    mods.confirmar_parada(numero);

    assert!(
        mods.vivos.is_empty(),
        "a instância parada continuou entre as vivas"
    );
    assert!(
        mods.encerrando.is_empty(),
        "sem nada a colher, ela devia ter saído da supervisão"
    );
}

/// E se ela tinha fala a colher, continua supervisionada até esvaziar.
#[test]
fn uma_parada_espontanea_com_fala_continua_visivel() {
    let mut mods = ModsNativos::default();
    let mut viva = instancia(7);
    produzir(&mut viva, 3, 64);
    let numero = mods.guardar(viva);

    mods.confirmar_parada(numero);
    let retida = mods
        .encerrando
        .get(&numero)
        .expect("sumiu com fala por colher");
    assert!(retida.parou, "parou sem ser marcada");
    assert_eq!(retida.pendentes.len(), 3);
    assert_eq!(retida.estado, EstadoNativo::Encerrando);
}

/// **Um erro não reserva crédito, e colhê-lo não devolve crédito nenhum.**
///
/// A revisão: «colher um erro sem crédito reservado pode fazer o contador
/// atômico dar a volta; repetir erros também contorna o teto da fila de
/// mensagens».
#[test]
fn um_erro_fica_fora_da_contabilidade_de_dados() {
    let mut viva = instancia(7);
    // Só erros, nenhuma mensagem — pelo mesmo caminho da bomba.
    for _ in 0..5 {
        viva.guardar_fala("falhou", "o MOD lançou".to_owned());
    }
    let (mensagens, bytes) = viva.executor.fila().ocupacao();
    assert_eq!(
        (mensagens, bytes),
        (0, 0),
        "um erro reservou crédito de dados"
    );
    // E os cinco iguais viraram um, com a contagem.
    assert_eq!(viva.avisos.len(), 1, "os avisos repetidos não agregaram");
    assert_eq!(viva.avisos.front().map(|a| a.vezes), Some(5));
}

/// **Erros repetidos não empurram mensagens para fora do teto.**
///
/// Com a janela parada, um MOD que só falha não pode consumir a cota do que
/// está funcionando — e não pode deixar a contabilidade num estado em que
/// mensagens futuras sejam recusadas para sempre.
#[test]
fn uma_chuva_de_erros_nao_bloqueia_mensagens_futuras() {
    let mut viva = instancia(7);
    for n in 0..10_000 {
        // Alternados para a agregação não os esconder todos.
        viva.guardar_fala("falhou", format!("erro {}", n % 3));
    }
    assert!(
        viva.avisos.len() <= super::AVISOS_POR_INSTANCIA,
        "a lista de avisos passou do teto: {}",
        viva.avisos.len()
    );
    assert!(viva.avisos_perdidos > 0, "nada foi contado como perdido");

    // E a fila de dados continua intacta: uma mensagem ainda cabe.
    assert!(
        viva.executor.fila().cabe(1024),
        "uma chuva de erros bloqueou as mensagens futuras"
    );
    let (mensagens, bytes) = viva.executor.fila().ocupacao();
    assert_eq!((mensagens, bytes), (1, 1024));
}

/// **Uma reserva que nunca foi ativada não guarda o fonte do MOD.**
///
/// A revisão: «`codigos_reservados` […] só perde uma entrada na ativação.
/// Reservar e sair sem ativar pode conservar o fonte».
///
/// Medido sobre a mesma lista que a revogação usa: se ela não nomear a
/// reserva, o código dela fica na memória desta janela para sempre.
#[test]
fn uma_reserva_sem_ativacao_nao_deixa_fonte_orfao() {
    let mut mods = ModsNativos::default();
    let mut reservada = instancia(7);
    reservada.estado = EstadoNativo::Reservada;
    let numero = mods.guardar(reservada);
    assert!(
        mods.vivos
            .get(&numero)
            .and_then(|i| i.codigo.as_ref())
            .is_some(),
        "a reserva devia nascer com o fonte dentro"
    );

    // Ninguém ativa. A sessão acaba.
    mods.revogar_geracao(7);

    // **Não há «entre».** O fonte mora dentro da instância, e a revogação o
    // solta pelo mesmo ato que solta o resto. Na versão anterior ele morava
    // numa tabela ao lado, inserida **depois** de o cadeado da instância ser
    // liberado — e uma revogação no meio apagava um fonte que ainda não
    // existia, deixando o comando inseri-lo depois do descarte.
    let orfao = mods
        .vivos
        .values()
        .chain(mods.encerrando.values())
        .any(|i| i.codigo.is_some());
    assert!(!orfao, "sobrou fonte de uma reserva que nunca foi ativada");

    // E ela continua supervisionada até confirmar — encerrar não é parar.
    assert!(
        mods.encerrando.contains_key(&numero),
        "a reserva revogada saiu da supervisão sem ter confirmado"
    );
    mods.confirmar_parada(numero);
    assert!(mods.encerrando.is_empty() && mods.vivos.is_empty());
}

/// **A reserva que chega numa geração morta é contada** em
/// `comandos_de_geracao_morta`, como a leitura de `codigo_do_mod` que
/// chega atrasada.
///
/// Ela era recusada sem conta: `comandos_de_geracao_morta`, o contador que
/// diz «alguma coisa da sessão anterior ainda estava falando», não subia
/// para a reserva atrasada — e o doc da recusa de carga dizia que subia.
/// A que vale não é contada.
#[test]
fn a_reserva_recusada_por_geracao_morta_e_contada() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let mods = std::sync::Mutex::new(ModsNativos::default());
    let mortos = AtomicU64::new(0);
    let mut atrasada = instancia(7);
    atrasada.estado = EstadoNativo::Reservada;
    let recusa = registrar_reserva(&mods, &|| false, &mortos, atrasada)
        .expect_err("a reserva de uma geração morta foi aceita");
    assert_eq!(
        super::motivo_de(&recusa),
        "sessao-encerrada",
        "a reserva de uma geração morta foi recusada por outro motivo"
    );
    assert_eq!(
        mortos.load(Ordering::Relaxed),
        1,
        "a reserva recusada por geração morta não foi contada em \
             `comandos_de_geracao_morta`, e o contador diz que nada da sessão anterior falou"
    );

    let mut viva = instancia(7);
    viva.estado = EstadoNativo::Reservada;
    registrar_reserva(&mods, &|| true, &mortos, viva).expect("a reserva viva foi recusada");
    assert_eq!(
        mortos.load(Ordering::Relaxed),
        1,
        "a reserva de uma geração viva foi contada como de geração morta"
    );
}

/// **A ativação que chega numa geração morta também é contada e dita**,
/// como a reserva logo acima e como os comandos que passam por
/// [`super::Session::recusar_se_morta`].
///
/// Ela era a ressalva do doc de `registrar_reserva`: recusada sem conta, e
/// um zero em `comandos_de_geracao_morta` não queria dizer que nada da
/// sessão anterior tinha falado. Depois de contada, ainda não dizia: os
/// outros comandos de MOD escreviam a linha DEBUG de `confere_geracao`, e
/// a ativação, não. A que vale não é contada nem dita, nem quando é
/// recusada por outro motivo.
#[test]
fn a_ativacao_recusada_por_geracao_morta_e_contada_e_dita() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let mods = std::sync::Mutex::new(ModsNativos::default());
    let nenhum = AtomicU64::new(0);
    let mut reservada = instancia(7);
    reservada.estado = EstadoNativo::Reservada;
    let numero = registrar_reserva(&mods, &|| true, &nenhum, reservada)
        .expect("a reserva viva foi recusada");

    let mortos = AtomicU64::new(0);
    let (recusa, rastro) =
        crate::rastro_de_teste::capturar(|| liberar_reserva(&mods, &|| false, &mortos, 7, numero));
    let recusa = recusa.expect_err("a ativação de uma geração morta foi aceita");
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("comando recusado: a sessão dele já acabou"))
        .unwrap_or_else(|| {
            panic!(
                "a ativação recusada por geração morta foi contada e não foi dita: os \
                     outros comandos de MOD deixam a linha DEBUG de `confere_geracao`, e esta \
                     some do registro — {rastro}"
            )
        });
    for pedaco in ["DEBUG", "geracao=7", &format!("instancia={numero}")] {
        assert!(
            linha.contains(pedaco),
            "a linha da ativação recusada por geração morta não traz `{pedaco}`, e quem lê \
                 o registro não acha de que sessão e de que instância ela era: {linha}"
        );
    }
    assert_eq!(
        super::motivo_de(&recusa),
        "sessao-encerrada",
        "a ativação de uma geração morta foi recusada por outro motivo"
    );
    assert_eq!(
        mortos.load(Ordering::Relaxed),
        1,
        "a ativação recusada por geração morta não foi contada em \
             `comandos_de_geracao_morta`, e o contador diz que nada da sessão anterior falou"
    );
    assert!(
        mods.lock()
            .expect("cadeado")
            .encerrando
            .contains_key(&numero),
        "a reserva de uma ativação recusada por geração morta não foi encerrada"
    );

    let (recusa, rastro) =
        crate::rastro_de_teste::capturar(|| liberar_reserva(&mods, &|| true, &mortos, 7, numero));
    let recusa = recusa.expect_err("a ativação de uma reserva já encerrada foi aceita");
    assert_eq!(
        super::motivo_de(&recusa),
        "instancia-nao-esta-reservada",
        "a ativação viva de uma reserva encerrada foi recusada por outro motivo"
    );
    assert_eq!(
        mortos.load(Ordering::Relaxed),
        1,
        "a ativação de uma geração viva foi contada como de geração morta"
    );
    assert!(
        !rastro.contains("comando recusado: a sessão dele já acabou"),
        "a ativação de uma geração viva foi dita como de uma sessão que acabou: {rastro}"
    );
}

/// **Um comando de MOD de uma geração morta é recusado, contado e dito**,
/// pela mesma função em todos eles ([`super::Session::recusar_se_morta`]).
///
/// Eram treze conferências com um `if` cada uma, em dez funções, e sete não
/// contavam: `comandos_de_geracao_morta` dizia zero com a sessão anterior
/// ainda falando. Nenhum teste daqui monta um `State`, e por isso a prova é
/// da função; o guarda de fonte logo abaixo prende que os comandos a usam.
#[test]
fn o_comando_de_uma_geracao_morta_e_recusado_contado_e_dito() {
    use std::sync::atomic::Ordering;

    let sessao = super::Session::default();
    sessao.geracao.store(3, Ordering::Release);
    let (resultado, rastro) = crate::rastro_de_teste::capturar(|| sessao.recusar_se_morta(2));
    let falha = resultado.expect_err("um comando de uma geração morta foi admitido");
    assert_eq!(
        super::motivo_de(&falha),
        "sessao-encerrada",
        "um comando de uma geração morta foi recusado por outro motivo, e a janela não o \
             lê como a sessão que acabou"
    );
    assert_eq!(
        sessao.comandos_de_geracao_morta.load(Ordering::Relaxed),
        1,
        "um comando recusado por geração morta não foi contado em \
             `comandos_de_geracao_morta`, e o contador diz que nada da sessão anterior falou"
    );
    assert!(
        rastro.contains("DEBUG") && rastro.contains("comando recusado: a sessão dele já acabou"),
        "um comando recusado por geração morta não deixou a linha DEBUG de \
             `confere_geracao`: {rastro}"
    );

    sessao
        .recusar_se_morta(0)
        .expect_err("o zero, que é «a casca não sabe», foi admitido");
    assert_eq!(
        sessao.comandos_de_geracao_morta.load(Ordering::Relaxed),
        2,
        "o comando com geração zero foi recusado sem ser contado"
    );

    sessao
        .recusar_se_morta(3)
        .expect("um comando da geração de pé foi recusado");
    assert_eq!(
        sessao.comandos_de_geracao_morta.load(Ordering::Relaxed),
        2,
        "um comando da geração de pé foi contado como de geração morta"
    );
}

/// **Nenhuma conferência de geração recusa por fora de
/// [`super::Session::recusar_se_morta`].**
///
/// O arquivo inteiro, menos `impl Session`, e não só os corpos de
/// `#[tauri::command]`: a conferência de `ler_codigo_do_mod` mora num
/// auxiliar, e um guarda que olhasse só os comandos não a veria. Os fechos
/// `&|| session.geracao_vale(geracao)` não são negação, e ficam: quem os
/// recebe (`registrar_reserva`, `liberar_reserva`) conta em `mortos`.
///
/// E cada função que conferia continua conferindo: tirar a conferência, em
/// vez de trocá-la, também deixaria este guarda sem negação nenhuma. A
/// lista é a conta inteira — as chamadas de `recusar_se_morta` fora de
/// `impl Session` e dos testes têm de ser as dela, uma por uma —, e uma
/// conferência nova só passa depois de entrar nela.
#[test]
fn nenhuma_geracao_morta_e_recusada_por_fora_de_recusar_se_morta() {
    let fonte = super::fonte_sem_comentarios();
    let (antes, resto) = fonte
        .split_once("\nimpl Session {\n")
        .expect("`impl Session` não está mais neste arquivo");
    let (_, depois) = resto.split_once("\n}\n").expect(
        "`impl Session` não fecha mais na coluna zero: sem o fim dela, o guarda leria só o \
             que vem antes, e uma conferência crua depois dela passaria calada",
    );
    let fora_da_sessao = [antes, depois].concat();
    let sem_espaco =
        |texto: &str| -> String { texto.chars().filter(|c| !c.is_whitespace()).collect() };
    // Os módulos de teste deste arquivo moram em `testes/`, e o `fonte` não os
    // traz mais. A conferência crua continua olhando para eles, como olhava
    // quando eram texto do `main.rs`. Só ela: a conta de `recusar_se_morta`,
    // mais abaixo, é a do produto.
    let dos_testes: String = super::fonte_dos_modulos_de_teste()
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let fora = sem_espaco(&[fora_da_sessao.as_str(), dos_testes.as_str()].concat());
    // Montado, e não escrito: escrito, o padrão acharia este teste.
    let chamada = format!(".{}(", "geracao_vale");
    let cruas: Vec<String> = fora
        .match_indices(&chamada)
        .filter_map(|(posicao, _)| {
            let receptor = fora
                .get(..posicao)?
                .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
            receptor.ends_with('!').then(|| {
                let inicio = receptor.len().saturating_sub(48);
                fora.get(inicio..posicao + chamada.len())
                    .unwrap_or_default()
                    .to_owned()
            })
        })
        .collect();
    assert!(
        cruas.is_empty(),
        "{} conferência(s) de geração recusam com um `if` próprio, e não por \
             `recusar_se_morta`: o comando de uma geração morta volta à janela sem ser contado \
             em `comandos_de_geracao_morta`, e o zero dele mente — {cruas:#?}",
        cruas.len()
    );

    // Montado também.
    let regra = format!(".{}(", "recusar_se_morta");
    let mut na_lista = 0;
    for (funcao, minimas) in [
        ("fn mod_nativo_colher(", 1),
        ("fn mod_nativo_entregar(", 1),
        ("fn ler_codigo_do_mod(", 1),
        ("async fn entrar_no_tunel_do_mod(", 1),
        ("fn reservar_no_executor(", 1),
        ("fn midia_do_mod(", 1),
        ("fn som_do_mod(", 1),
        ("fn midia_em_bytes(", 1),
        ("async fn enviar_imagem_mod(", 2),
        ("async fn ler_imagem_mod(", 2),
        ("async fn salvar_do_mod(", 2),
        ("async fn escolher_para_o_mod(", 2),
        ("fn pedaco_do_escolhido(", 1),
    ] {
        let feitas = sem_espaco(super::corpo_de(&fonte, funcao))
            .matches(&regra)
            .count();
        na_lista += feitas;
        assert!(
            feitas >= minimas,
            "`{funcao}` confere a geração {feitas} vez(es) por `recusar_se_morta`, e \
                 conferia {minimas}: um comando de uma geração morta passa a ser admitido"
        );
    }
    let na_producao = sem_espaco(&sem_itens_de_teste(&fora_da_sessao))
        .matches(&regra)
        .count();
    assert_eq!(
        na_producao, na_lista,
        "há {na_producao} chamada(s) de `recusar_se_morta` fora de `impl Session` e dos \
             testes, e as funções da lista deste guarda somam {na_lista}: uma conferência ficou \
             fora da lista, e apagá-la não reprovaria nada"
    );
}

/// O fonte sem os itens `#[cfg(test)]` do topo do arquivo, para que a
/// conta de [`nenhuma_geracao_morta_e_recusada_por_fora_de_recusar_se_morta`]
/// seja a do produto, e não a dos testes que chamam `recusar_se_morta`.
///
/// Um item vai até a chave que fecha na coluna zero, ou até a linha na
/// coluna zero que termina em `;` (a `const` do fonte). Errar o fim só
/// pode deixar teste na conta, e a igualdade reprova: o corte falha
/// fechado.
fn sem_itens_de_teste(fonte: &str) -> String {
    let mut producao = Vec::new();
    let mut linhas = fonte.lines();
    while let Some(linha) = linhas.next() {
        if linha != "#[cfg(test)]" {
            producao.push(linha);
            continue;
        }
        for linha in linhas.by_ref() {
            let de_uma_linha =
                linha.ends_with(';') && !linha.starts_with(' ') && !linha.starts_with('#');
            if de_uma_linha || linha == "}" {
                break;
            }
        }
    }
    producao.join("\n")
}

/// **A corrida entre registrar uma reserva e revogar a geração.**
///
/// A revisão pedia «uma prova com barreira entre registrar a instância e
/// guardar o fonte». **Essa barreira não tem onde ser posta**: o fonte mora
/// dentro da instância, e não há instante entre as duas inserções porque
/// não são duas. O que resta para provar é a corrida que sobrou — a que
/// existe de verdade — e ela se prova com threads, não com prosa.
///
/// A ordem que a torna segura mora em dois lugares e só funciona nos dois:
/// [`revogar_em`] incrementa a geração **antes** de tomar o cadeado;
/// [`registrar_reserva`] confere a geração **depois** de tomá-lo. Os dois
/// desfechos possíveis são bons: quem registra primeiro é varrido pela
/// revogação, quem registra depois é recusado.
///
/// **A prova precisa de disputa pelo cadeado**, e não de duas threads
/// soltas. A janela que a inversão abre fica entre soltar o cadeado e
/// incrementar a geração — nanossegundos — e duzentas rodadas sem disputa
/// não caem nela nenhuma vez: foi o que a primeira versão deste guarda fez,
/// e ela passava com a ordem invertida. Com a tabela povoada, a varredura
/// demora, os registradores se empilham no cadeado e acordam exatamente ali
/// dentro.
#[test]
fn registrar_e_revogar_ao_mesmo_tempo_nao_deixa_fonte_orfao() {
    use std::sync::atomic::{AtomicU64, Ordering};

    const POVOAR: usize = 24;
    const REGISTRADORES: usize = 4;

    for rodada in 0..30u64 {
        let mods = std::sync::Arc::new(std::sync::Mutex::new(ModsNativos::default()));
        let geracao = std::sync::Arc::new(AtomicU64::new(7));
        let mortos = std::sync::Arc::new(AtomicU64::new(0));
        {
            let mut vivos = mods.lock().expect("cadeado");
            for _ in 0..POVOAR {
                let mut habitante = instancia(7);
                produzir(&mut habitante, 2, 256);
                vivos.guardar(habitante);
            }
        }
        // As instâncias nascem antes da largada: criar um motor QuickJS
        // custa, e pagar esse custo dentro da corrida a desfaria.
        let prontas: Vec<InstanciaNativa> = (0..REGISTRADORES)
            .map(|_| {
                let mut nova = instancia(7);
                nova.estado = EstadoNativo::Reservada;
                nova
            })
            .collect();

        let largada = std::sync::Arc::new(std::sync::Barrier::new(REGISTRADORES + 1));
        let registradores: Vec<_> = prontas
            .into_iter()
            .map(|nova| {
                let mods = std::sync::Arc::clone(&mods);
                let geracao = std::sync::Arc::clone(&geracao);
                let mortos = std::sync::Arc::clone(&mortos);
                let largada = std::sync::Arc::clone(&largada);
                std::thread::spawn(move || {
                    largada.wait();
                    // Um respiro para o revogador tomar o cadeado antes:
                    // é dentro da varredura dele que se quer chegar.
                    for _ in 0..64 {
                        std::hint::spin_loop();
                    }
                    registrar_reserva(
                        &mods,
                        &|| geracao.load(Ordering::Acquire) == 7,
                        &mortos,
                        nova,
                    )
                })
            })
            .collect();
        let revogador = {
            let mods = std::sync::Arc::clone(&mods);
            let geracao = std::sync::Arc::clone(&geracao);
            let largada = std::sync::Arc::clone(&largada);
            // **A revogação de verdade**, e não uma cópia da ordem dela:
            // uma cópia provaria a cópia.
            std::thread::spawn(move || {
                largada.wait();
                revogar_em(&geracao, &mods)
            })
        };
        let registradas: Vec<_> = registradores
            .into_iter()
            .map(|t| t.join().expect("registrador"))
            .collect();
        revogador.join().expect("revogador");
        // Cada reserva que a revogação recusou é contada, uma vez.
        let recusadas = registradas.iter().filter(|r| r.is_err()).count() as u64;
        assert_eq!(
            mortos.load(Ordering::Relaxed),
            recusadas,
            "rodada {rodada}: as reservas recusadas pela revogação e as contadas em \
                 `comandos_de_geracao_morta` não batem"
        );

        let vivos = mods.lock().expect("cadeado");
        let orfao = vivos
            .vivos
            .values()
            .chain(vivos.encerrando.values())
            .any(|i| i.codigo.is_some());
        assert!(
            !orfao,
            "rodada {rodada}: sobrou fonte de uma geração revogada"
        );
        // Registrou antes: a revogação tem de tê-la varrido para o
        // encerramento, e não pode tê-la deixado viva. Recusou: nada
        // entrou, nem para ser varrido.
        for numero in registradas.into_iter().flatten() {
            assert!(
                !vivos.vivos.contains_key(&numero),
                "rodada {rodada}: a reserva {numero} ficou viva numa \
                     geração que já tinha sido revogada"
            );
        }
        assert!(
            vivos.vivos.is_empty(),
            "rodada {rodada}: sobrou instância viva de uma geração revogada"
        );
    }
}

/// **Enquanto alguém registra, a geração não muda.**
///
/// Este é o guarda que a corrida por thread não conseguia ser. A janela que
/// a ordem antiga deixava durava poucas instruções, e nenhuma disputa caía
/// nela: trinta rodadas com quatro registradores, com a ordem invertida de
/// propósito, passaram todas. Um guarda que não falha quando o defeito está
/// lá não é um guarda.
///
/// Com o incremento **dentro** do cadeado, a propriedade vira observável:
/// segurando o cadeado — que é exatamente o que [`registrar_reserva`] faz
/// enquanto confere e insere —, a revogação não anda. Nem a geração sobe,
/// nem a varredura roda. Tirar o incremento de dentro do cadeado faz esta
/// asserção falhar na hora.
#[test]
fn registrar_e_revogar_se_excluem_inteiros() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let mods = std::sync::Arc::new(std::sync::Mutex::new(ModsNativos::default()));
    let geracao = std::sync::Arc::new(AtomicU64::new(7));
    let (avisou, chegou) = std::sync::mpsc::channel::<()>();

    let segurando = mods.lock().expect("cadeado");

    let revogador = {
        let mods = std::sync::Arc::clone(&mods);
        let geracao = std::sync::Arc::clone(&geracao);
        std::thread::spawn(move || {
            let _ = avisou.send(());
            revogar_em(&geracao, &mods)
        })
    };
    chegou.recv().expect("o revogador não largou");

    // Espera limitada, e sem prazo fixo: a asserção não é «ainda não
    // mudou», é «não pode mudar». Sob o arranjo certo isto roda até o fim
    // sem nunca ver 8; sob o errado, a primeira volta já vê.
    for volta in 0..2_000 {
        assert_eq!(
            geracao.load(Ordering::Acquire),
            7,
            "volta {volta}: a geração morreu enquanto um registro estava \
                 em curso — a revogação voltou a andar fora do cadeado"
        );
        std::thread::yield_now();
    }

    // E o que este registro inseriu é varrido assim que o cadeado sai.
    let mut reservada = instancia(7);
    reservada.estado = EstadoNativo::Reservada;
    let numero = {
        let mut vivos = segurando;
        vivos.guardar(reservada)
    };
    assert_eq!(revogador.join().expect("revogador"), 8);

    let vivos = mods.lock().expect("cadeado");
    assert!(
        !vivos.vivos.contains_key(&numero),
        "a reserva inserida sob o cadeado escapou da varredura"
    );
    assert!(
        vivos
            .vivos
            .values()
            .chain(vivos.encerrando.values())
            .all(|i| i.codigo.is_none()),
        "sobrou fonte de uma geração revogada"
    );
}

/// Uma instância montada em volta de um executor já escutado.
///
/// A da corrida nasce com o executor dentro e o canal ainda fechado; aqui o
/// canal precisa ficar na mão de quem testa, porque é ele que faz o papel
/// da bomba lenta.
fn com_canal(
    geracao: u64,
) -> (
    InstanciaNativa,
    std::sync::mpsc::Receiver<crate::executor::ParaOFora>,
    std::sync::Arc<crate::executor::Fila>,
) {
    let mut nova = instancia(geracao);
    let (recebedor, fila) = nova.executor.escutar().expect("canal");
    (nova, recebedor, fila)
}

/// Espera uma condição, com teto, **afirmando a condição e não o tempo**.
fn ate(prazo: std::time::Duration, o_que: &str, mut condicao: impl FnMut() -> bool) {
    let limite = std::time::Instant::now() + prazo;
    while std::time::Instant::now() < limite {
        if condicao() {
            return;
        }
        std::thread::yield_now();
    }
    panic!("passou o prazo sem {o_que}");
}

/// **Um produtor de erros de verdade, com a bomba e a janela paradas.**
///
/// A revisão: «testar produtor de erros real com bomba/consumidor lentos e
/// entrega à janela suspensa, contando todos os pontos de retenção e
/// notificações». Aqui o MOD lança a cada resposta entregue — não é um
/// `ParaOFora::Falhou` fabricado pelo teste, é o motor traduzindo uma
/// exceção —, ninguém escoa o canal, e ninguém colhe.
///
/// São **três** pontos de retenção, e a cota é uma só atravessando os três:
/// o que o motor recusou antes de mandar, o que está no canal esperando a
/// bomba, e o que está na instância esperando a janela.
#[test]
fn um_produtor_de_erros_real_para_nas_tres_cotas() {
    use crate::executor::AVISOS_NA_FILA;

    let (viva, recebedor, fila) = com_canal(7);
    viva.executor
        .iniciar(
            "globalThis.aoResponder = () => { throw new Error('sempre'); };\
                 seele.postar('pronto');",
        )
        .expect("código");

    // O MOD lança em toda resposta. Entregar bem mais que a cota é o que
    // faz a recusa acontecer no motor, antes de ocupar o canal.
    ate(
        std::time::Duration::from_secs(5),
        "o MOD ficar de pé",
        || viva.executor.entregar("{\"v\":1}").is_ok(),
    );
    for _ in 0..(AVISOS_NA_FILA * 8) {
        // Recusa de entrada não é o que se mede aqui: o teto de entrada é
        // outro, e ele se solta sozinho quando o motor consome.
        let _ = viva.executor.entregar("{\"v\":1}");
        std::thread::yield_now();
    }

    // **Primeiro ponto: o motor recusa.** Sem cota lá, um MOD que lança num
    // laço enche o canal entre o motor e a bomba, que nenhum teto alcança.
    ate(
        std::time::Duration::from_secs(10),
        "o motor recusar um aviso",
        || fila.avisos_recusados() > 0,
    );
    assert_eq!(
        fila.avisos_de_pe(),
        AVISOS_NA_FILA,
        "a cota de avisos parou num número que não é o teto"
    );

    // **Segundo ponto: o canal.** A bomba é lenta — é este laço — e cada
    // fala que ela assenta carrega a mesma reserva para a instância.
    let mods = std::sync::Mutex::new(ModsNativos::default());
    let numero = mods.lock().expect("cadeado").guardar(viva);
    let mut notificacoes = 0usize;
    while let Ok(fala) = recebedor.recv_timeout(std::time::Duration::from_millis(50)) {
        if let super::Assentada::Guardada { avisar: true } | super::Assentada::Fim { .. } =
            super::assentar_fala(&mods, &|| true, &fila, numero, fala, "prova/mod")
        {
            notificacoes += 1;
        }
    }

    // **A notificação é agregada.** A janela suspensa recebe **um** aviso
    // sem carga, e não um por fala: a versão anterior emitia um evento por
    // fala, inclusive pelas que ela mesma agregava ou descartava.
    assert_eq!(
        notificacoes, 1,
        "a janela suspensa recebeu mais de um aviso para a mesma colheita"
    );

    // **Terceiro ponto: a instância.** Um MOD que lança num laço manda
    // sempre o mesmo texto, e é por isso que a agregação é o que contém
    // este caso: as repetições viram contagem, e cada uma **devolve** o
    // lugar na cota, porque nenhuma entrada nova foi ocupada. A perda do
    // mais velho é o outro caminho — avisos diferentes entre si —, e quem
    // o cobre é `uma_chuva_de_erros_nao_bloqueia_mensagens_futuras`.
    let mut vivos = mods.lock().expect("cadeado");
    let dentro = vivos.achar_mut(numero).expect("instância");
    assert_eq!(
        dentro.avisos.len(),
        1,
        "o mesmo erro repetido ocupou mais de uma entrada"
    );
    let repetido = dentro.avisos.front().expect("o aviso agregado");
    assert!(
        repetido.vezes > 1,
        "as repetições não foram agregadas: vezes = {}",
        repetido.vezes
    );
    assert!(
        dentro.avisos.len() <= super::AVISOS_POR_INSTANCIA,
        "a lista de avisos da instância passou do teto dela"
    );
    assert_eq!(
        fila.avisos_de_pe(),
        1,
        "a cota de avisos não acompanhou a agregação: o que está retido é \
             uma entrada, e é uma que tem de estar reservada"
    );

    // E é a colheita que solta o que sobrou, e que rearma o aviso.
    let colhidas = dentro.colher_falas(64);
    assert!(!colhidas.is_empty());
    assert_eq!(
        fila.avisos_de_pe(),
        0,
        "a colheita não devolveu a cota de avisos"
    );
    assert!(
        dentro.precisa_avisar(),
        "a janela drenou e o aviso não rearmou"
    );
}

/// **Um erro tardio no meio de mensagens ainda contabilizadas.**
///
/// A revisão: «provar um erro tardio intercalado com mensagens ainda
/// contabilizadas, em vez de testar apenas contadores já zerados». Um teste
/// que drena tudo antes de conferir não distingue a contabilidade certa da
/// que devolve crédito da classe errada: com os dois contadores em zero,
/// tirar da cota de mensagens ou da de avisos dá no mesmo.
///
/// Aqui o MOD fala, e **só então** lança. As mensagens continuam retidas
/// enquanto o erro chega, e cada classe tem de voltar pela cota dela.
#[test]
fn um_erro_tardio_nao_come_o_credito_das_mensagens() {
    let (viva, recebedor, fila) = com_canal(7);
    viva.executor
        .iniciar(
            "globalThis.aoResponder = (j) => {\
                   if (JSON.parse(j).falhar) { throw new Error('tarde'); }\
                   seele.postar('x'.repeat(512));\
                 };\
                 seele.postar('pronto');",
        )
        .expect("código");
    ate(
        std::time::Duration::from_secs(5),
        "o MOD ficar de pé",
        || viva.executor.entregar("{\"falhar\":false}").is_ok(),
    );
    for _ in 0..4 {
        viva.executor
            .entregar("{\"falhar\":false}")
            .expect("entrega");
    }
    // O erro vem **depois**, com as mensagens ainda na fila e ninguém
    // colhendo.
    ate(
        std::time::Duration::from_secs(5),
        "as mensagens saírem do motor",
        || fila.ocupacao().0 >= 6,
    );
    viva.executor
        .entregar("{\"falhar\":true}")
        .expect("entrega do erro");
    ate(
        std::time::Duration::from_secs(5),
        "o erro sair do motor",
        || fila.avisos_de_pe() == 1,
    );

    let (mensagens_antes, bytes_antes) = fila.ocupacao();
    assert!(
        mensagens_antes >= 6,
        "as mensagens foram consumidas antes do erro chegar, e o teste \
             voltaria a medir contadores zerados"
    );

    let mods = std::sync::Mutex::new(ModsNativos::default());
    let numero = mods.lock().expect("cadeado").guardar(viva);
    while let Ok(fala) = recebedor.recv_timeout(std::time::Duration::from_millis(50)) {
        super::assentar_fala(&mods, &|| true, &fila, numero, fala, "prova/mod");
    }

    let mut vivos = mods.lock().expect("cadeado");
    let dentro = vivos.achar_mut(numero).expect("instância");
    assert_eq!(dentro.avisos.len(), 1, "o erro tardio não chegou inteiro");
    assert!(
        dentro.pendentes.len() >= 6,
        "as mensagens sumiram na passagem do erro"
    );
    // **Nada foi solto na travessia.** Esta é a asserção que os contadores
    // zerados não conseguiam fazer.
    assert_eq!(
        (fila.ocupacao(), fila.avisos_de_pe()),
        ((mensagens_antes, bytes_antes), 1),
        "assentar as falas mexeu na cota antes de a janela colher"
    );

    dentro.colher_falas(64);
    assert_eq!(
        (fila.ocupacao(), fila.avisos_de_pe()),
        ((0, 0), 0),
        "a colheita não devolveu cada classe pela cota dela"
    );
}

/// **A imagem com um uso chega reduzida a ele, e diz quanto pesa
/// decodificada** (`reducao`; tarefa 6 da F3, Step 5), pelos dois caminhos
/// da mídia do servidor: o texto em base64 e a leitura por volume. Sem uso,
/// ela segue como veio.
#[test]
fn a_imagem_com_uso_chega_reduzida_e_diz_quanto_pesa_decodificada() {
    use image::ImageEncoder as _;
    let foto = image::RgbaImage::from_pixel(2000, 700, image::Rgba([40, 200, 120, 255]));
    let mut grande = Vec::new();
    image::codecs::png::PngEncoder::new(&mut grande)
        .write_image(&foto, 2000, 700, image::ExtendedColorType::Rgba8)
        .expect("gravar a foto de teste");
    let base64 = seele_ffi::base64_de(&grande);
    let teto = crate::reducao::TETO_DA_CAPA;
    // 2000 × 700 cabe em 960 × 320 como 914 × 320.
    let esperado = Some(914 * 320 * 4);

    // Sem uso, o teto geral da mídia de MOD (1920 × 1080): 2000 × 700
    // vira 1920 × 672, e só o uso nomeado diz quanto pesa decodificada.
    let sem_uso = super::ler_midia_do_servidor(&base64, None).expect("imagem aceita");
    assert_eq!(sem_uso.decodificados, None);
    let reduzida = seele_ffi::de_base64(
        sem_uso
            .uri
            .as_deref()
            .and_then(|uri| uri.split_once(','))
            .expect("um data:")
            .1,
    )
    .expect("base64");
    assert_eq!(seele_ffi::reducao::medidas(&reduzida), Ok((1920, 672)));

    let com_uso = super::ler_midia_do_servidor(&base64, Some(&teto)).expect("imagem reduzida");
    assert!(
        com_uso.bytes < grande.len(),
        "a imagem não encolheu: {} bytes",
        com_uso.bytes
    );
    assert_eq!(com_uso.decodificados, esperado);

    let lida = super::imagem_lida_dita(
        &super::BaldesDoRegistro::default(),
        std::time::Instant::now(),
        "seele/perfis",
        Ok(grande.clone()),
        Some(&teto),
    )
    .expect("imagem reduzida pelo volume");
    assert_eq!(lida.decodificados, esperado);
}

#[test]
fn imagem_do_perfis_em_data_uri_chega_ao_decodificador() {
    // Imagens de verdade, e não só a assinatura: as medidas saem do
    // cabeçalho antes de a imagem ir à janela (o teto de pixels).
    let gravada = |formato: image::ImageFormat| {
        let quadro = image::RgbImage::from_pixel(4, 3, image::Rgb([40, 200, 120]));
        let mut saida = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(quadro)
            .write_to(&mut saida, formato)
            .expect("gravar a imagem de teste");
        saida.into_inner()
    };
    for (mime, bytes) in [
        ("image/png", gravada(image::ImageFormat::Png)),
        ("image/jpeg", gravada(image::ImageFormat::Jpeg)),
        ("image/webp", gravada(image::ImageFormat::WebP)),
    ] {
        let bytes = bytes.as_slice();
        let base64 = seele_ffi::base64_de(bytes);
        for entrada in [base64.clone(), format!("data:{mime};base64,{base64}")] {
            let midia = super::ler_midia_do_servidor(&entrada, None).expect("imagem aceita");
            assert_eq!(midia.bytes, bytes.len());
            assert_eq!(
                midia.uri,
                Some(format!("data:{mime};base64,{base64}")),
                "a mídia do servidor saiu sem o `data:` que os bytes provaram, e a janela \
                     fica sem o que mostrar"
            );
        }
    }
}

#[test]
fn midia_do_servidor_confere_bytes_e_limite_mesmo_com_cabecalho() {
    let png = crate::pacote_de_teste::PNG;
    let alegada = format!("data:image/jpeg;base64,{}", seele_ffi::base64_de(png));
    assert!(
        super::ler_midia_do_servidor(&alegada, None)
            .unwrap()
            .uri
            .is_some_and(|uri| uri.starts_with("data:image/png;")),
        "o tipo da mídia do servidor saiu do cabeçalho que o terceiro escreveu, e não dos \
             bytes"
    );
    for invalida in [
        "data:image/png,abc",
        "data:image/png;base64",
        "data:image/png;base64,%%%",
        "https://example.com/a.png",
    ] {
        assert!(super::ler_midia_do_servidor(invalida, None).is_err());
    }
    let mut bytes = vec![0; seele_ffi::mods::TETO_DE_MIDIA];
    // `get_mut` e não um fatiamento: `indexing_slicing` é `warn` no
    // workspace e o CI roda `-D warnings`. O `expect` mantém o teste
    // honesto — se o teto de mídia um dia não couber uma assinatura de
    // PNG, isto tem de gritar, não passar de lado.
    bytes
        .get_mut(..png.len())
        .expect("o teto de mídia cabe um PNG de 1 × 1")
        .copy_from_slice(png);
    let uri = format!("data:image/png;base64,{}", seele_ffi::base64_de(&bytes));
    assert_eq!(
        super::ler_midia_do_servidor(&uri, None).unwrap().bytes,
        bytes.len()
    );
    bytes.push(0);
    let uri = format!("data:image/png;base64,{}", seele_ffi::base64_de(&bytes));
    assert!(super::ler_midia_do_servidor(&uri, None).is_err());
}

/// **Um arquivo escolhido não sobrevive à sessão que o escolheu.**
///
/// É a mesma promessa que a sonda E1 mediu e encontrou falsa no Worker de
/// `blob:` — o que um MOD guardou lá reapareceu depois de o aplicativo ser
/// reiniciado. Aqui os bytes são do produto, e a saída os solta.
///
/// Dez megabytes por arquivo, quatro arquivos: uma sessão que sai sem
/// soltar deixa quarenta megabytes presos por um MOD que já não roda.
#[test]
fn a_saida_solta_os_arquivos_escolhidos_daquela_geracao() {
    let mut guardados = super::ArquivosEscolhidos::default();
    let de = |geracao: u64| super::Escolhido {
        de_quem: "prova/mod".to_owned(),
        geracao,
        tipo: "image/png",
        tamanho: 16,
        conteudo: vec![0; 16],
    };
    assert!(guardados.guardar(de(7)).is_ok());
    assert!(guardados.guardar(de(7)).is_ok());
    let da_seguinte = guardados.guardar(de(8)).expect("guardou");

    assert_eq!(guardados.soltar_geracao(7).len(), 2);
    // E o da geração seguinte fica: soltar o que não é dela seria derrubar
    // a sessão que acabou de começar.
    assert_eq!(guardados.len(), 1);
    assert!(guardados.de(8, "prova/mod", da_seguinte).is_ok());
}

/// **O pedaço só sai para quem escolheu, e o `soltar` devolve agora**
/// (`actions.pedaco` e `actions.soltar-arquivo`; a auditoria do `v6.json`,
/// tabela 2). O teste dos pedaços provava a constante, e não a entrega:
/// tirar a conferência de dono passava, e o `soltar` não tinha prova
/// nenhuma. O corpo dos dois comandos, pela mesma função que eles chamam.
#[test]
fn o_pedaco_so_sai_para_quem_escolheu_e_o_soltar_devolve_agora() {
    let mut guardados = super::ArquivosEscolhidos::default();
    let bytes: Vec<u8> = (0..=255).collect();
    let numero = guardados
        .guardar(super::Escolhido {
            de_quem: "prova/mod".to_owned(),
            geracao: 7,
            tipo: "image/png",
            tamanho: bytes.len(),
            conteudo: bytes.clone(),
        })
        .expect("guardou");
    let pedaco = |guardados: &super::ArquivosEscolhidos, geracao, id: &str, inicio| {
        super::pedaco_em_base64(guardados, geracao, id, numero, inicio)
    };
    let motivo = |r: Result<String, super::FalhaNoMod>| match r {
        Ok(_) => "(saiu)".to_owned(),
        Err(falha) => super::motivo_de(&falha).to_owned(),
    };

    assert_eq!(
        pedaco(&guardados, 7, "prova/mod", 0).ok(),
        Some(seele_ffi::base64_de(&bytes)),
        "quem escolheu não recebeu o arquivo"
    );
    assert_eq!(
        pedaco(&guardados, 7, "prova/mod", 250).ok(),
        Some(seele_ffi::base64_de(bytes.get(250..).unwrap_or_default())),
        "o pedaço do meio não começou onde o MOD pediu"
    );
    assert_eq!(
        motivo(pedaco(&guardados, 7, "outro/mod", 0)),
        "arquivo-de-outro-mod",
        "um MOD que sabe o número leu o arquivo que outro escolheu"
    );
    assert_eq!(
        motivo(pedaco(&guardados, 8, "prova/mod", 0)),
        "arquivo-de-outro-mod",
        "a sessão seguinte leu o arquivo da anterior"
    );

    // O `soltar` de quem não é o dono não solta; o do dono solta agora.
    guardados.soltar(7, "outro/mod", numero);
    assert!(
        guardados.de(7, "prova/mod", numero).is_ok(),
        "outro MOD soltou o arquivo que não era dele"
    );
    guardados.soltar(7, "prova/mod", numero);
    assert_eq!(
        motivo(pedaco(&guardados, 7, "prova/mod", 0)),
        "arquivo-nao-esta-de-pe",
        "o arquivo solto continuou de pé"
    );
    // E soltar de novo não é erro.
    guardados.soltar(7, "prova/mod", numero);
}

/// **O escolhido tem teto de 100 MiB, e o MOD só o aperta** (`volume.cliente`
/// e `recusas.volume[3]`). A 0.19 subiu o teto de 10 para 100 MiB; a API 7
/// publicada promete «até 10 MiB», e a promessa segue cumprida: o teto
/// desta máquina nunca fica abaixo do número do texto. O texto da API 8
/// diz o número novo. O comando abre o seletor do sistema e não sobe em
/// teste, então o número é conferido contra o texto e contra o teto do
/// servidor, que tem de ser o mesmo.
#[test]
fn o_escolhido_tem_teto_de_100_mib_e_o_mod_so_o_aperta() {
    const MIB: usize = 1024 * 1024;
    let v7: serde_json::Value =
        serde_json::from_str(include_str!("../../../../api/v7.json")).expect("o v7.json");
    let texto = v7
        .get("volume")
        .and_then(|volume| volume.get("cliente"))
        .and_then(serde_json::Value::as_str)
        .expect("o texto do volume.cliente");
    let prometido: usize = texto
        .split("até ")
        .nth(1)
        .and_then(|resto| resto.split(" MiB").next())
        .and_then(|numero| numero.trim().parse().ok())
        .expect("o v7.json não diz «até N MiB» no volume.cliente");
    assert!(
        super::TETO_DO_ESCOLHIDO >= prometido * MIB,
        "o teto do escolhido ({} MiB) ficou abaixo do que a API 7 promete ({prometido} MiB)",
        super::TETO_DO_ESCOLHIDO / MIB
    );
    assert_eq!(
        super::TETO_DO_ESCOLHIDO,
        seele_proto::midia_de_mod::TETO_DE_ARQUIVO,
        "a pessoa escolhe um arquivo que o servidor não grava, ou o contrário"
    );
    assert_eq!(super::teto_do_escolhido(None), 100 * MIB);
    assert_eq!(
        super::teto_do_escolhido(Some(0)),
        100 * MIB,
        "zero é «não declarou»"
    );
    assert_eq!(
        super::teto_do_escolhido(Some(MIB)),
        MIB,
        "o MOD não apertou o teto"
    );
    assert_eq!(
        super::teto_do_escolhido(Some(200 * MIB)),
        100 * MIB,
        "o MOD ampliou o que esta máquina aceita ler"
    );
}

/// **O papel que o MOD não pediu é `papel-nao-serve`, com o papel**
/// (`recusas` do `v6.json`, «leva papel»; `actions.escolher-arquivo`, a
/// auditoria, tabela 2). A conferência morava dentro do comando, que abre o
/// seletor do sistema e não sobe em teste: tirá-la passava.
#[test]
fn o_papel_que_o_mod_nao_pediu_e_papel_nao_serve_com_o_papel() {
    let v6 = include_str!("../../../../api/v6.json");
    assert!(
        v6.contains("papel-nao-serve (o arquivo não é do papel pedido; leva papel)"),
        "o v6.json não diz mais o papel-nao-serve como este teste o confere"
    );
    let pedidos = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let motivo =
        |r: Result<(), super::FalhaNoMod>| r.err().map(|f| super::motivo_de(&f).to_owned());
    assert_eq!(
        motivo(super::o_papel_serve(None, "som")),
        None,
        "sem pedido, o papel não foi aceito"
    );
    assert_eq!(
        motivo(super::o_papel_serve(Some(&[]), "imagem")),
        None,
        "a lista vazia é «não pediu», e recusou"
    );
    assert_eq!(
        motivo(super::o_papel_serve(Some(&pedidos(&["imagem"])), "imagem")),
        None,
        "o papel pedido foi recusado"
    );
    assert_eq!(
        motivo(super::o_papel_serve(Some(&pedidos(&["imagem"])), "som")).as_deref(),
        Some("papel-nao-serve:som"),
        "o som, para um MOD que pediu imagem, passou ou saiu sem o papel"
    );
    assert_eq!(
        motivo(super::o_papel_serve(
            Some(&pedidos(&["som", "imagem"])),
            "som"
        )),
        None,
        "um dos dois papéis pedidos foi recusado"
    );
}

/// **A mídia do servidor acima do teto é `arquivo-grande-demais`**
/// (`recusas.volume[3]`): antes de decodificar, pelo tamanho do texto, e
/// depois, pelo dos bytes. Nenhum teste passava do teto em nenhum dos
/// caminhos.
#[test]
fn a_midia_do_servidor_acima_do_teto_e_arquivo_grande_demais() {
    let teto = seele_ffi::mods::TETO_DE_MIDIA;
    let motivo = |base64: &str| match super::ler_midia_do_servidor(base64, None) {
        Ok(_) => "(aceita)".to_owned(),
        Err(falha) => super::motivo_de(&falha).to_owned(),
    };
    // Caracteres que não são base64: só a conferência de antes, pelo
    // tamanho do texto, diz `arquivo-grande-demais`; decodificado, seria
    // `nao-e-base64`.
    assert_eq!(
        motivo(&"!".repeat(teto * 2 + 4)),
        "arquivo-grande-demais",
        "o texto acima do dobro do teto foi decodificado antes de ser medido"
    );
    let um_a_mais = seele_ffi::base64_de(&vec![0_u8; teto + 1]);
    assert!(
        um_a_mais.len() <= teto * 2,
        "o caso não mede o que diz: o texto já passa do corte de antes"
    );
    assert_eq!(
        motivo(&um_a_mais),
        "arquivo-grande-demais",
        "os bytes acima do teto passaram"
    );
}

/// **Dois pedaços seguidos, concatenados, são o arquivo.**
///
/// O base64 codifica três bytes em quatro caracteres. Uma janela que não
/// fecha um grupo de três sai com `=` no meio, e o que o MOD junta deixa de
/// ser o arquivo — uma imagem que chega corrompida sem nada acusar, porque
/// cada pedaço, sozinho, é base64 válido.
///
/// `65536` não é múltiplo de três. `65535` é.
#[test]
fn os_pedacos_de_um_arquivo_escolhido_se_juntam_de_volta() {
    assert_eq!(
        super::PEDACO_DO_ESCOLHIDO % 3,
        0,
        "o pedaço deixou de fechar grupos de três, e dois seguidos não se juntam"
    );
    // Dois pedaços e meio, para o último ser o único com enchimento.
    let bytes: Vec<u8> = (0..super::PEDACO_DO_ESCOLHIDO * 2 + 7)
        .map(|i| u8::try_from(i % 251).unwrap_or(0))
        .collect();
    let inteiro = seele_ffi::base64_de(&bytes);
    let mut juntado = String::new();
    let mut inicio = 0;
    while inicio < bytes.len() {
        let fim = inicio
            .saturating_add(super::PEDACO_DO_ESCOLHIDO)
            .min(bytes.len());
        juntado.push_str(&seele_ffi::base64_de(
            bytes.get(inicio..fim).expect("a fatia"),
        ));
        inicio = fim;
    }
    assert_eq!(juntado, inteiro, "os pedaços não se juntam no arquivo");
    // E só o último tem enchimento: um `=` no meio é o defeito com nome.
    assert!(
        !juntado[..juntado.len() - 4].contains('='),
        "sobrou enchimento no meio do que foi juntado"
    );
}

/// **E há teto: um MOD que abre o seletor num laço não enche a memória.**
#[test]
fn os_arquivos_de_pe_tem_teto_e_o_numero_nunca_se_repete() {
    let mut guardados = super::ArquivosEscolhidos::default();
    let de = || super::Escolhido {
        de_quem: "prova/mod".to_owned(),
        geracao: 1,
        tipo: "image/png",
        tamanho: 0,
        conteudo: Vec::new(),
    };
    let numeros: Vec<u64> = (0..super::ARQUIVOS_DE_PE)
        .map(|_| guardados.guardar(de()).expect("cabe"))
        .collect();
    assert!(
        guardados.guardar(de()).is_err(),
        "o teto de arquivos de pé não segurou"
    );

    // **Monotônico.** Soltar um e guardar outro não reaproveita o número:
    // um pedido atrasado com o número velho leria o arquivo que ocupou o
    // lugar dele, e seria o arquivo errado sem nada dizer.
    assert!(guardados.soltar(
        1,
        "prova/mod",
        *numeros.first().expect("o primeiro guardado")
    ));
    let novo = guardados.guardar(de()).expect("cabe depois de soltar");
    assert!(
        !numeros.contains(&novo),
        "um número de arquivo foi reaproveitado: {novo} já tinha sido usado"
    );
}

/// **Uma montagem que não completa desfaz tudo.**
///
/// Sem o desfazer, a instância ficava na tabela com o fonte dentro e sem
/// ninguém para escutá-la — e só uma ativação futura, que pode nunca vir, a
/// tiraria de lá.
#[test]
fn uma_montagem_que_falha_no_meio_desfaz_tudo() {
    let mut mods = ModsNativos::default();
    let mut viva = instancia(7);
    produzir(&mut viva, 4, 128);
    let numero = mods.guardar(viva);

    // É o que o comando faz quando a bomba não sobe.
    mods.encerrar(numero);
    mods.descartar(numero);

    assert!(
        mods.vivos.is_empty() && mods.encerrando.is_empty(),
        "a montagem desfeita continuou na supervisão"
    );
}

/// **Colher um erro não devolve crédito que ninguém tomou.**
///
/// A revisão: «`mod_nativo_colher` subtrai quantidade e bytes de **todas**
/// as falas colhidas» — e erro nunca reservou nada. Colhendo mensagens e
/// erros juntos, a contabilidade devolvia mais do que tinha sido tomado, e
/// a fila passava a aceitar acima do teto.
#[test]
fn colher_devolve_o_credito_das_mensagens_e_so_delas() {
    let mut viva = instancia(7);
    produzir(&mut viva, 4, 100);
    // Intercalados, como acontece de verdade: o MOD fala e falha.
    viva.guardar_fala("falhou", "um".to_owned());
    viva.guardar_fala("interrompido", String::new());
    produzir(&mut viva, 2, 100);

    let (antes, bytes_antes) = viva.executor.fila().ocupacao();
    assert_eq!(
        (antes, bytes_antes),
        (6, 600),
        "só as mensagens deviam ter reservado crédito"
    );

    // **Colhe uma mensagem só, e os avisos vêm junto.** É o estado
    // intermediário que denuncia a conta errada: com mensagens ainda
    // pendentes, devolver crédito por um aviso aparece como crédito a
    // menos. Drenando tudo de uma vez o contador satura em zero e o erro
    // fica invisível — foi assim que a primeira versão deste teste passou
    // com o defeito de volta.
    let parcial = viva.colher_falas(1);
    assert_eq!(parcial.len(), 3, "uma mensagem e os dois avisos");
    let (meio, bytes_meio) = viva.executor.fila().ocupacao();
    assert_eq!(
        (meio, bytes_meio),
        (5, 500),
        "a devolução não fechou: sobraram cinco mensagens de cem bytes"
    );

    let colhidas = viva.colher_falas(64);
    assert_eq!(colhidas.len(), 5, "a colheita não trouxe o resto");
    let (depois, bytes_depois) = viva.executor.fila().ocupacao();
    assert_eq!(
        (depois, bytes_depois),
        (0, 0),
        "a devolução não fechou com o que foi tomado"
    );

    // **E a fila volta a aceitar exatamente o teto**, nem mais nem menos:
    // um crédito devolvido a mais faria caber acima dele.
    let mut couberam = 0;
    while viva.executor.fila().cabe(1024) {
        couberam += 1;
        if couberam > crate::executor::MENSAGENS_NA_FILA + 1 {
            break;
        }
    }
    assert_eq!(
        couberam,
        crate::executor::MENSAGENS_NA_FILA,
        "a fila aceitou fora do teto depois de colher erros"
    );
}

/// E o descarte não encosta em quem é de outra geração.
#[test]
fn o_descarte_de_uma_geracao_nao_toca_na_outra() {
    let mut mods = ModsNativos::default();
    let mut da_sete = instancia(7);
    produzir(&mut da_sete, 5, 128);
    mods.guardar(da_sete);

    let mut da_oito = instancia(8);
    produzir(&mut da_oito, 5, 128);
    let outro = mods.guardar(da_oito);

    mods.revogar_geracao(7);

    let sobrevivente = mods.vivos.get(&outro).expect("a de outra geração caiu");
    assert_eq!(
        sobrevivente.pendentes.len(),
        5,
        "o descarte de uma geração levou a fala de outra"
    );
    let (mensagens, _) = sobrevivente.executor.fila().ocupacao();
    assert_eq!(mensagens, 5, "os créditos de outra geração foram soltos");
}
