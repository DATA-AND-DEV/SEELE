use super::{connect, send_message, Session};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{AppHandle, Manager};

/// A casa do app neste teste: identidade e pinos numa pasta temporária,
/// e não na de quem roda a bateria. Uma só para o processo inteiro, porque
/// `SEELE_HOME` é do processo.
fn casa() -> &'static std::path::Path {
    static CASA: OnceLock<tempfile::TempDir> = OnceLock::new();
    let pasta = CASA
        .get_or_init(|| tempfile::tempdir().expect("a pasta temporária não abriu"))
        .path();
    std::env::set_var("SEELE_HOME", pasta);
    pasta
}

/// Um teste deste módulo por vez: a casa é uma só, e a identidade e os
/// pinos dela são arquivos que dois `connect` de testes diferentes
/// escreveriam ao mesmo tempo. **E os do módulo vizinho que usa esta casa**
/// (`o_apelido_grande_demais_no_hello`, lote C55 da F5): a `SEELE_HOME` é
/// do processo, e uma segunda casa a trocaria por baixo destes testes.
pub(super) fn vez() -> std::sync::MutexGuard<'static, ()> {
    static VEZ: std::sync::Mutex<()> = std::sync::Mutex::new(());
    VEZ.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// O runtime de um teste deste módulo: uma thread só, a do teste, para o
/// `seele.log` capturado ver o que o `connect` escreve.
pub(super) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("o runtime do teste não subiu")
}

/// Um servidor de verdade, em porta zero e banco em memória.
pub(super) async fn servidor(nome: &str) -> (SocketAddr, Arc<Daemon>) {
    let config = ServerConfig {
        name: nome.into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.expect("o servidor não subiu"));
    let endereco = servidor
        .local_addr()
        .expect("o servidor não disse o endereço");
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    (endereco, servidor)
}

/// O app, com o runtime falso. Vazado: o `connect` corre numa tarefa
/// própria, e o `State` que ele recebe empresta do app.
///
/// **E sempre na casa temporária** (R1-I2 da re-revisão do E54). A casa
/// era montada por cada teste, e o teste do M1 esqueceu: rodado sozinho,
/// o `config_dir` dele caía em `$HOME/.config/seele`, e o `connect` lia a
/// identidade e o retrato de quem roda a bateria e gravava pinos de
/// `127.0.0.1` ao lado dos dela. Montada aqui, nenhum teste do módulo pode
/// esquecer; e a casa que o app vai usar é conferida antes de qualquer
/// `connect`, para um caminho novo do `config_dir` reprovar em vez de
/// escrever na casa de alguém.
pub(super) fn app() -> &'static AppHandle<MockRuntime> {
    let casa = casa();
    let app = mock_builder()
        .manage(Session::default())
        .build(mock_context(noop_assets()))
        .expect("o app falso não montou");
    let handle = app.handle().clone();
    let do_app = super::config_dir(&handle);
    assert_eq!(
        std::path::Path::new(&do_app),
        casa,
        "a casa do app falso não é a temporária deste módulo: um `connect` daqui escreveria \
             a identidade e os pinos em {do_app}"
    );
    Box::leak(Box::new(app));
    Box::leak(Box::new(handle))
}

/// Quem recebeu `corpo`, entre os servidores dados, esperando até `prazo`.
async fn quem_recebeu(
    ouvidos: &mut [(
        &str,
        tokio::sync::broadcast::Receiver<seele_server::server::Event>,
    )],
    corpo: &str,
    prazo: Duration,
) -> Vec<String> {
    let fim = tokio::time::Instant::now() + prazo;
    let mut receberam = Vec::new();
    while tokio::time::Instant::now() < fim {
        for (nome, ouvido) in ouvidos.iter_mut() {
            while let Ok(evento) = ouvido.try_recv() {
                if let seele_server::server::Event::MessagePosted(mensagem) = evento {
                    if mensagem.body == corpo && !receberam.contains(&(*nome).to_owned()) {
                        receberam.push((*nome).to_owned());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    receberam
}

/// Quantos estão presentes no servidor, esperando até `prazo` pelo número
/// que se espera.
async fn presentes(servidor: &Daemon, espera: usize, prazo: Duration) -> usize {
    let fim = tokio::time::Instant::now() + prazo;
    loop {
        let agora = servidor.server().presentes.lock().await.todos().len();
        if agora == espera || tokio::time::Instant::now() >= fim {
            return agora;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Um retrato desta máquina, para o `connect` ter um efeito de entrada que
/// o servidor vê: ele manda o retrato guardado logo depois de conectar.
fn guardar_um_retrato() {
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(1, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .expect("o retrato de teste não codificou");
    std::fs::write(casa().join("retrato.png"), png.into_inner())
        .expect("o retrato de teste não gravou");
}

/// Um `connect` numa thread só dele, com um runtime próprio.
///
/// Para os testes que seguram um cadeado da sessão e precisam que o
/// `connect` pare nele: no runtime de uma thread só em que os servidores
/// andam, um `connect` parado num cadeado pararia os servidores junto.
fn connect_noutra_thread(
    app: &'static AppHandle<MockRuntime>,
    servidor: String,
    apelido: &str,
) -> std::thread::JoinHandle<Result<super::Entrada, super::FalhaDaEntrada>> {
    let apelido = apelido.to_owned();
    std::thread::spawn(move || {
        runtime().block_on(connect(
            app.clone(),
            app.state::<Session>(),
            servidor,
            apelido,
            false,
            None,
            None,
        ))
    })
}

/// Espera uma thread terminar sem parar o runtime do teste, até `prazo`.
async fn quando_terminar<T>(thread: std::thread::JoinHandle<T>, prazo: Duration) -> T {
    let fim = tokio::time::Instant::now() + prazo;
    while !thread.is_finished() {
        assert!(
            tokio::time::Instant::now() < fim,
            "a thread não terminou no prazo"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    thread.join().expect("a thread caiu")
}

/// Segura, numa thread própria, a guarda que `tomar` devolve — um cadeado
/// da sessão —, até o `Sender` devolvido ser usado ou cair. Numa thread, e
/// não no teste, para nenhuma guarda atravessar um `await`.
fn segurar<G>(
    tomar: impl FnOnce() -> G + Send + 'static,
) -> (std::sync::mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (tomado, foi_tomado) = std::sync::mpsc::channel();
    let (soltar, quando_soltar) = std::sync::mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let guarda = tomar();
        let _ = tomado.send(());
        let _ = quando_soltar.recv();
        drop(guarda);
    });
    foi_tomado.recv().expect("o cadeado não foi tomado");
    (soltar, thread)
}

/// Espera o servidor anunciar um retrato novo, até `prazo`.
async fn um_retrato_chegou(
    ouvido: &mut tokio::sync::broadcast::Receiver<seele_server::server::Event>,
    prazo: Duration,
) -> bool {
    tokio::time::timeout(prazo, async {
        loop {
            match ouvido.recv().await {
                Ok(seele_server::server::Event::PersonIconChanged { .. }) => break true,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => break false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

/// O cenário inteiro, do começo da velha à mensagem: o que ficou errado.
async fn a_velha_termina_depois_da_nova() -> Vec<String> {
    guardar_um_retrato();
    let (velho, servidor_velho) = servidor("Velho").await;
    let (novo, servidor_novo) = servidor("Novo").await;
    let app = app();
    let sessao = app.state::<Session>();

    // A velha sai primeiro e fica presa no aperto de mão do servidor dela.
    let trava = servidor_velho.server().persistence.lock().await;
    let velha = tokio::spawn(connect(
        app.clone(),
        app.state::<Session>(),
        velho.to_string(),
        "velha".to_owned(),
        false,
        None,
        None,
    ));
    let comeco = tokio::time::Instant::now();
    while sessao.geracao() == 0 && comeco.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(sessao.geracao(), 1, "a tentativa velha não começou");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !velha.is_finished(),
        "a tentativa velha terminou antes da hora"
    );

    // A nova entra, inteira, com a velha ainda andando.
    let nova = tokio::time::timeout(
        Duration::from_secs(10),
        connect(
            app.clone(),
            app.state::<Session>(),
            novo.to_string(),
            "nova".to_owned(),
            false,
            None,
            None,
        ),
    )
    .await
    .expect("a tentativa nova não voltou")
    .map_err(|falha| format!("{falha:?}"))
    .expect("a tentativa nova não entrou");
    assert_eq!(nova.geracao, 2, "a tentativa nova não abriu a geração dois");
    let canal = nova
        .snapshot
        .channels
        .first()
        .map(|canal| canal.id)
        .expect("o servidor novo não tem canal nenhum");

    // Solta a velha, e ela termina depois da nova. O que o servidor velho
    // recebe dela daqui em diante é o que os efeitos de entrada fizeram.
    let mut do_velho = servidor_velho.server().events.subscribe();
    drop(trava);
    let velha = tokio::time::timeout(Duration::from_secs(10), velha)
        .await
        .expect("a tentativa velha não voltou")
        .expect("a tarefa da tentativa velha caiu");

    let mut vermelhos = Vec::new();
    match &velha {
        Ok(entrada) => vermelhos.push(format!(
            "a tentativa velha voltou como entrada (geração {}), depois de a nova ter entrado",
            entrada.geracao
        )),
        Err(falha) => {
            let dita = serde_json::to_value(falha).unwrap_or_default();
            if dita.get("substituida") != Some(&serde_json::Value::Bool(true)) {
                vermelhos.push(format!(
                    "a recusa da tentativa velha não diz que ela foi substituída, e a janela \
                         não tem como calá-la: {dita}"
                ));
            }
        }
    }
    // Pelo nome que cada servidor dá a si mesmo no aperto de mão, e não
    // pela impressão: a FFI da linha 0.15 não diz a impressão da conexão,
    // e o teste é o mesmo nas duas linhas.
    let qual = |nome: Option<&str>| match nome {
        Some("Novo") => "o servidor novo",
        Some("Velho") => "o servidor velho",
        Some(_) => "um servidor desconhecido",
        None => "nenhum",
    };
    let no_slot = sessao
        .connection()
        .ok()
        .map(|conexao| conexao.snapshot().server);
    if qual(no_slot.as_deref()) != "o servidor novo" {
        vermelhos.push(format!(
            "o slot da sessão ficou com {}, e não com a conexão da nova",
            qual(no_slot.as_deref())
        ));
    }
    if sessao.geracao() != nova.geracao {
        vermelhos.push(format!(
            "a geração de pé é {}, e não a da nova ({})",
            sessao.geracao(),
            nova.geracao
        ));
    }
    let onde = sessao
        .endereco_da_sessao
        .lock()
        .ok()
        .and_then(|onde| onde.clone());
    if onde.as_deref() != Some(novo.to_string().as_str()) {
        vermelhos.push(format!(
            "o endereço da sessão é {onde:?}, e não o do servidor novo ({novo})"
        ));
    }

    // A mensagem que a pessoa manda em seguida: qual servidor a recebe.
    let mut ouvidos = [
        (
            "o servidor velho",
            servidor_velho.server().events.subscribe(),
        ),
        ("o servidor novo", servidor_novo.server().events.subscribe()),
    ];
    let corpo = "para onde eu vou?";
    if let Err(erro) = send_message(
        app.state::<Session>(),
        super::sessoes::CarimboDaJanela::da(&nova),
        canal,
        corpo.to_owned(),
        None,
    ) {
        vermelhos.push(format!("a mensagem não saiu: {erro:?}"));
    }
    let receberam = quem_recebeu(&mut ouvidos, corpo, Duration::from_secs(2)).await;
    if receberam != ["o servidor novo"] {
        vermelhos.push(format!(
            "a mensagem mandada em seguida chegou a {receberam:?}, e não só ao servidor novo"
        ));
    }

    // E a conexão da velha não fica pendurada no servidor que a pessoa
    // deixou, nem a da nova sai do servidor em que ela está.
    let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
    if no_velho != 0 {
        vermelhos.push(format!(
            "a conexão da velha continua presente no servidor velho ({no_velho})"
        ));
    }
    let no_novo = presentes(&servidor_novo, 1, Duration::from_secs(2)).await;
    if no_novo != 1 {
        vermelhos.push(format!(
            "a pessoa não está presente no servidor novo ({no_novo})"
        ));
    }

    // Nem os efeitos de entrada valem para ela: o retrato desta máquina
    // não vai ao servidor que a pessoa deixou.
    let mut retratos = 0;
    while let Ok(evento) = do_velho.try_recv() {
        if matches!(
            evento,
            seele_server::server::Event::PersonIconChanged { .. }
        ) {
            retratos += 1;
        }
    }
    if retratos != 0 {
        vermelhos.push(format!(
            "a tentativa velha mandou o retrato ao servidor que a pessoa deixou ({retratos} \
                 vez(es)): os efeitos de entrada valeram para ela"
        ));
    }
    vermelhos
}

/// O cenário, com o `seele.log` capturado: a linha da velha descartada
/// sai na thread do teste, que é a do runtime em que o `connect` corre.
#[test]
fn a_velha_que_entra_depois_da_nova_nao_toma_a_conexao() {
    let _vez = vez();
    let runtime = runtime();
    let (mut vermelhos, rastro) =
        crate::rastro_de_teste::capturar(|| runtime.block_on(a_velha_termina_depois_da_nova()));
    let linha = rastro
        .lines()
        .find(|linha| linha.contains("substituída") && linha.contains(" WARN "));
    if linha.is_none() {
        vermelhos.push(
            "o seele.log não tem uma linha WARN dizendo que a entrada substituída foi \
                 descartada"
                .to_owned(),
        );
    }
    assert!(
        vermelhos.is_empty(),
        "a entrada velha que chegou tarde tomou a sessão nova:\n  {}",
        vermelhos.join("\n  ")
    );
}

/// **A outra ordem**: a velha entra no Rust antes de a nova chegar a ele.
///
/// A nova já começou na janela (`++tentativas`), e ainda não chamou o
/// `connect` — está no `conferir_esta_versao`, por exemplo. A geração da
/// velha ainda é a de pé, e ela entra. A janela não a quer, e a solta pela
/// geração dela. O que se cobra: a velha sai do slot e do servidor dela, a
/// nova entra em vez de receber `AlreadyConnected`, a mensagem vai à nova,
/// e um pedido com uma geração que já não é a de pé não derruba ninguém.
async fn a_janela_larga_a_velha_antes_de_a_nova_chegar() -> Vec<String> {
    casa();
    let (velho, servidor_velho) = servidor("Velho").await;
    let (novo, servidor_novo) = servidor("Novo").await;
    let app = app();
    let sessao = app.state::<Session>();
    let mut vermelhos = Vec::new();

    let velha = tokio::time::timeout(
        Duration::from_secs(10),
        connect(
            app.clone(),
            app.state::<Session>(),
            velho.to_string(),
            "velha".to_owned(),
            false,
            None,
            None,
        ),
    )
    .await
    .expect("a tentativa velha não voltou")
    .map_err(|falha| format!("{falha:?}"))
    .expect("a tentativa velha não entrou");
    assert_eq!(
        presentes(&servidor_velho, 1, Duration::from_secs(2)).await,
        1
    );

    // A janela larga a velha pela geração dela.
    if !super::soltar_a_entrada_em(&sessao, velha.geracao) {
        vermelhos.push(format!(
            "pedir para soltar a entrada da geração {} — a de pé — não soltou nada",
            velha.geracao
        ));
    }
    if sessao.connection().is_ok() {
        vermelhos.push("a conexão da velha continua no slot depois de a janela largá-la".into());
    }
    if sessao.geracao_vale(velha.geracao) {
        vermelhos.push(format!(
            "a geração da velha ({}) continua de pé depois de a janela largá-la: um comando \
                 dela ainda seria atendido",
            velha.geracao
        ));
    }
    let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
    if no_velho != 0 {
        vermelhos.push(format!(
            "a conexão largada continua presente no servidor velho ({no_velho})"
        ));
    }
    let onde = sessao
        .endereco_da_sessao
        .lock()
        .ok()
        .and_then(|onde| onde.clone());
    if onde.is_some() {
        vermelhos.push(format!(
            "o endereço da sessão continua {onde:?} depois de a janela largar a entrada"
        ));
    }

    // A nova chega ao `connect` e entra.
    let nova = tokio::time::timeout(
        Duration::from_secs(10),
        connect(
            app.clone(),
            app.state::<Session>(),
            novo.to_string(),
            "nova".to_owned(),
            false,
            None,
            None,
        ),
    )
    .await
    .expect("a tentativa nova não voltou");
    let nova = match nova {
        Ok(entrada) => entrada,
        Err(falha) => {
            vermelhos.push(format!(
                "a tentativa nova não entrou depois de a janela largar a velha: {}",
                serde_json::to_string(&falha).unwrap_or_default()
            ));
            return vermelhos;
        }
    };
    let canal = nova
        .snapshot
        .channels
        .first()
        .map(|canal| canal.id)
        .expect("o servidor novo não tem canal nenhum");

    // Um pedido com a geração da velha, que já não é a de pé, não derruba
    // a nova.
    if super::soltar_a_entrada_em(&sessao, velha.geracao) {
        vermelhos.push(
            "um pedido com a geração da velha, que já não era a de pé, soltou a entrada nova"
                .into(),
        );
    }
    if sessao.geracao() != nova.geracao {
        vermelhos.push("o pedido com a geração velha revogou a geração da nova".into());
    }
    let mut ouvidos = [
        (
            "o servidor velho",
            servidor_velho.server().events.subscribe(),
        ),
        ("o servidor novo", servidor_novo.server().events.subscribe()),
    ];
    let corpo = "e agora, para onde?";
    if let Err(erro) = send_message(
        app.state::<Session>(),
        super::sessoes::CarimboDaJanela::da(&nova),
        canal,
        corpo.to_owned(),
        None,
    ) {
        vermelhos.push(format!("a mensagem não saiu: {erro:?}"));
    }
    let receberam = quem_recebeu(&mut ouvidos, corpo, Duration::from_secs(2)).await;
    if receberam != ["o servidor novo"] {
        vermelhos.push(format!(
            "a mensagem mandada depois chegou a {receberam:?}, e não só ao servidor novo"
        ));
    }
    let no_novo = presentes(&servidor_novo, 1, Duration::from_secs(2)).await;
    if no_novo != 1 {
        vermelhos.push(format!(
            "a pessoa não está presente no servidor novo ({no_novo})"
        ));
    }
    vermelhos
}

/// **A nova começa entre as duas conferências da velha** (I1 da revisão
/// do E54).
///
/// A velha passa a primeira conferência e aplica os efeitos de entrada; a
/// nova começa e revoga a geração antes de a velha gravar o slot. O teste
/// para a velha no cadeado do slot, que ele segura: solta o banco do
/// servidor velho, espera o retrato da velha chegar a ele (ela passou da
/// primeira conferência), revoga a geração como o `connect` da nova faz ao
/// começar, e só então solta o slot. A velha tem de ser largada ali.
async fn a_nova_comeca_entre_as_duas_conferencias() -> Vec<String> {
    guardar_um_retrato();
    let (velho, servidor_velho) = servidor("Velho").await;
    let app = app();
    let sessao = app.state::<Session>();
    let mut vermelhos = Vec::new();

    let trava = servidor_velho.server().persistence.lock().await;
    let velha = connect_noutra_thread(app, velho.to_string(), "velha");
    let comeco = tokio::time::Instant::now();
    while sessao.geracao() == 0 && comeco.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(sessao.geracao(), 1, "a tentativa velha não começou");

    // O slot, seguro: a velha para nele, na segunda conferência.
    let (soltar_o_slot, quem_segura) =
        segurar(move || app.state::<Session>().inner().connection.lock());
    let mut do_velho = servidor_velho.server().events.subscribe();
    drop(trava);
    assert!(
        um_retrato_chegou(&mut do_velho, Duration::from_secs(8)).await,
        "o retrato da velha não chegou ao servidor velho: ela não passou da primeira \
             conferência, e este teste não mede a segunda"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !velha.is_finished(),
        "a velha terminou sem esperar o slot, e este teste não mede a segunda conferência"
    );

    // A nova começa, e revoga a geração da velha.
    sessao.revogar();
    let _ = soltar_o_slot.send(());
    quem_segura.join().expect("quem segurava o slot caiu");

    match quando_terminar(velha, Duration::from_secs(10)).await {
        Ok(entrada) => vermelhos.push(format!(
            "a velha voltou como entrada (geração {}) depois de a nova revogar a geração dela \
                 no meio dos efeitos de entrada",
            entrada.geracao
        )),
        Err(falha) => {
            let dita = serde_json::to_value(&falha).unwrap_or_default();
            if dita.get("substituida") != Some(&serde_json::Value::Bool(true)) {
                vermelhos.push(format!(
                    "a recusa da velha substituída entre as duas conferências não vem marcada: \
                         {dita}"
                ));
            }
        }
    }
    if let Ok(conexao) = sessao.connection() {
        vermelhos.push(format!(
            "o slot ficou com a conexão da velha ({})",
            conexao.snapshot().server
        ));
    }
    let onde = sessao
        .endereco_da_sessao
        .lock()
        .ok()
        .and_then(|onde| onde.clone());
    if onde.is_some() {
        vermelhos.push(format!(
            "o endereço da sessão ficou {onde:?}, o do servidor que a pessoa deixou"
        ));
    }
    let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
    if no_velho != 0 {
        vermelhos.push(format!(
            "a velha continua presente no servidor velho ({no_velho})"
        ));
    }
    vermelhos
}

#[test]
fn a_velha_que_a_nova_substitui_entre_as_duas_conferencias_nao_grava_o_slot() {
    let _vez = vez();
    let vermelhos = runtime().block_on(a_nova_comeca_entre_as_duas_conferencias());
    assert!(
        vermelhos.is_empty(),
        "a velha substituída entre as duas conferências do `connect` tomou a sessão:\n  {}",
        vermelhos.join("\n  ")
    );
}

/// **A nova que falha não deixa a velha órfã no slot** (M1 da revisão do
/// E54, a sonda P53).
///
/// A ordem forçada: a nova passa pelo `AlreadyConnected` com o slot vazio
/// e para antes de revogar (o teste segura `mods_nativos`, que a revogação
/// toma antes de somar); a velha passa as duas conferências com a geração
/// dela ainda de pé; a nova revoga e falha (a porta 99999 não existe, e a
/// recusa sai sem rede nenhuma). A janela larga a velha, porque a nova é a
/// mais nova, e pede para soltá-la pela geração dela — que já não é a de
/// pé. Sem conferir o slot e revogar com o cadeado dele na mão, a velha
/// ficava no slot e presente no servidor deixado, e toda entrada seguinte
/// recebia `AlreadyConnected` até o app fechar.
async fn a_nova_falha_no_meio_da_velha() -> Vec<String> {
    // O retrato é a prova de que a velha passou da primeira conferência
    // (R1-I2 da re-revisão: sem ele, o teste dependia de outro do módulo
    // tê-lo gravado antes).
    guardar_um_retrato();
    let (velho, servidor_velho) = servidor("Velho").await;
    let (novo, _servidor_novo) = servidor("Novo").await;
    let app = app();
    let sessao = app.state::<Session>();
    let mut vermelhos = Vec::new();

    let trava = servidor_velho.server().persistence.lock().await;
    let velha = connect_noutra_thread(app, velho.to_string(), "velha");
    let comeco = tokio::time::Instant::now();
    while sessao.geracao() == 0 && comeco.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(sessao.geracao(), 1, "a tentativa velha não começou");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // A nova começa e para antes de revogar.
    let (soltar_os_mods, quem_segura) =
        segurar(move || app.state::<Session>().inner().mods_nativos.lock());
    let nova = connect_noutra_thread(app, "127.0.0.1:99999".to_owned(), "nova");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        sessao.geracao(),
        1,
        "a nova revogou sem esperar os MODs, e a ordem não foi a forçada"
    );

    // A velha termina o aperto de mão e passa a primeira conferência.
    let mut do_velho = servidor_velho.server().events.subscribe();
    drop(trava);
    assert!(
        um_retrato_chegou(&mut do_velho, Duration::from_secs(8)).await,
        "o retrato da velha não chegou ao servidor velho: ela não passou da primeira \
             conferência, e a ordem não foi a forçada"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;

    // A nova revoga e falha.
    let _ = soltar_os_mods.send(());
    quem_segura.join().expect("quem segurava os MODs caiu");
    let da_nova = quando_terminar(nova, Duration::from_secs(10)).await;
    let da_velha = quando_terminar(velha, Duration::from_secs(10)).await;

    // **A ordem foi a forçada** (R1-M5 da re-revisão). A nova falha pela
    // porta que não existe, e não por `AlreadyConnected`: chegando ao slot
    // depois de a velha gravá-lo, a recusa seria essa, a janela soltaria a
    // velha pela geração ainda de pé, e o teste passaria sem medir nada.
    match &da_nova {
        Ok(_) => vermelhos.push("a nova a uma porta que não existe entrou".to_owned()),
        Err(falha) if falha.falha.error == super::ConnectionError::AlreadyConnected => {
            vermelhos.push(
                "a nova recebeu `AlreadyConnected`: chegou ao slot depois de a velha gravá-lo, \
                     e a ordem não foi a forçada"
                    .to_owned(),
            );
        }
        Err(_) => {}
    }
    // A velha cai na segunda conferência dela, marcada como substituída.
    match &da_velha {
        Ok(entrada) => vermelhos.push(format!(
            "a velha voltou como entrada (geração {}) com a geração da nova de pé",
            entrada.geracao
        )),
        Err(falha) => {
            let dita = serde_json::to_value(falha).unwrap_or_default();
            if dita.get("substituida") != Some(&serde_json::Value::Bool(true)) {
                vermelhos.push(format!(
                    "a recusa da velha não vem marcada como substituída: {dita}"
                ));
            }
        }
    }
    // O slot fica vazio sem pedido nenhum da janela.
    if let Ok(conexao) = sessao.connection() {
        vermelhos.push(format!(
            "a velha ficou no slot ({}) sem a janela pedir nada",
            conexao.snapshot().server
        ));
    }
    // E o que a janela faz em seguida, para o vermelho dizer a
    // consequência: ela larga a velha que voltou entrando, pela geração
    // dela, que já não é a de pé.
    if let Ok(entrada) = &da_velha {
        let _ = super::soltar_a_entrada_em(&sessao, entrada.geracao);
        if let Ok(conexao) = sessao.connection() {
            vermelhos.push(format!(
                "a velha ficou órfã no slot ({}): a janela a largou e o Rust não a soltou",
                conexao.snapshot().server
            ));
        }
    }
    let no_velho = presentes(&servidor_velho, 0, Duration::from_secs(5)).await;
    if no_velho != 0 {
        vermelhos.push(format!(
            "a velha continua presente no servidor que a pessoa deixou ({no_velho})"
        ));
    }
    let seguinte = quando_terminar(
        connect_noutra_thread(app, novo.to_string(), "seguinte"),
        Duration::from_secs(10),
    )
    .await;
    if let Err(falha) = seguinte {
        vermelhos.push(format!(
            "a entrada seguinte não entrou: {}",
            serde_json::to_string(&falha).unwrap_or_default()
        ));
    }
    vermelhos
}

#[test]
fn a_nova_que_falha_no_meio_da_velha_nao_a_deixa_orfa_no_slot() {
    let _vez = vez();
    let vermelhos = runtime().block_on(a_nova_falha_no_meio_da_velha());
    assert!(
        vermelhos.is_empty(),
        "a nova falhou no meio da velha, e a velha ficou com a sessão sem tela:\n  {}",
        vermelhos.join("\n  ")
    );
}

/// **As duas conferências do `connect`, pela forma.**
///
/// O comportamento das duas tem teste: a primeira em
/// `a_velha_que_entra_depois_da_nova_nao_toma_a_conexao`, e a segunda em
/// `a_velha_que_a_nova_substitui_entre_as_duas_conferencias_nao_grava_o_slot`,
/// que para o `connect` no cadeado do slot. (Este doc dizia que nenhum teste
/// conseguia parar o `connect` ali, e não era verdade: a sonda P2 da
/// revisão o parou, e o teste dela entrou aqui.) Este guarda prende a forma
/// que dá aquele comportamento, sem ler comentário:
///
/// 1. entre o `connect_watching` voltar e o primeiro efeito de entrada há a
///    conferência da geração, e ela **volta** com a substituída largada;
/// 2. o slot só é gravado com o cadeado dele tomado e a geração conferida
///    ali, sem espera no meio;
/// 3. quem não gravou **volta** largada, antes de o endereço da sessão ser
///    escrito. Largar sem voltar (o mutante R3b da revisão) fechava a
///    conexão e seguia gravando o endereço e a lista da velha, e devolvia
///    `Ok`: o guarda só conferia a ordem do texto, e o deixava passar.
#[test]
fn o_connect_confere_a_geracao_antes_dos_efeitos_e_ao_gravar_o_slot() {
    let fonte = super::fonte_sem_comentarios();
    let corpo = super::corpo_de(&fonte, "async fn connect<");
    let Some(volta) = corpo.find("let (connection, veredito) = resultado") else {
        panic!("o `connect` não recebe mais a conexão do `connect_watching`: {corpo}");
    };
    let Some(primeiro_efeito) = [
        "connection.subscribe(",
        "vestir_as_escolhas_gravadas(",
        "set_person_icon(",
        "consentir_no_caminho_entre_pares(",
    ]
    .iter()
    .filter_map(|efeito| corpo[volta..].find(efeito).map(|n| volta + n))
    .min() else {
        panic!("o `connect` não tem mais efeitos de entrada que este guarda conheça: {corpo}");
    };
    let antes_dos_efeitos = &corpo[volta..primeiro_efeito];
    assert!(
        antes_dos_efeitos.contains("session.confere_geracao(geracao).is_err()")
            && antes_dos_efeitos.contains("return Err(largar_a_substituida("),
        "o `connect` aplica os efeitos de entrada sem conferir antes se a geração dele \
             ainda é a de pé: a tentativa que outra substituiu manda o retrato e o \
             consentimento ao servidor que a pessoa deixou.\n{antes_dos_efeitos}"
    );
    // Desde a 0.17 (E3), gravar é pôr a sessão nova na vista, com o cadeado
    // da conexão da vista na mão.
    let Some(gravar) = corpo.find("session.por_na_vista_com(&mut slot, Some(&nova))") else {
        panic!("o `connect` não grava mais o slot: {corpo}");
    };
    let Some(cadeado) = corpo[..gravar].rfind("session.connection.lock()") else {
        panic!("o `connect` grava o slot sem tomar o cadeado dele: {corpo}");
    };
    let ao_gravar = &corpo[cadeado..gravar];
    assert!(
        ao_gravar.contains("session.confere_geracao(geracao).is_ok()")
            && !ao_gravar.contains(".await"),
        "o `connect` grava o slot sem conferir a geração com o cadeado na mão: a tentativa \
             que outra substituiu no meio dos efeitos de entrada grava a conexão dela por cima \
             da nova.\n{ao_gravar}"
    );
    let depois = &corpo[gravar..];
    let Some(ramo) = depois.find("if let Some(connection) = substituida {") else {
        panic!(
            "depois de gravar o slot, o `connect` não tem mais o ramo da tentativa que não \
                 gravou: {depois}"
        );
    };
    let Some(fim_do_ramo) = depois[ramo..].find("\n    }\n").map(|n| ramo + n) else {
        panic!("o ramo da tentativa que não gravou não fecha: {depois}");
    };
    let bloco = &depois[ramo..fim_do_ramo];
    let endereco = depois.find("endereco_da_sessao");
    assert!(
        bloco.contains("return Err(largar_a_substituida(")
            && endereco.is_some_and(|endereco| fim_do_ramo < endereco),
        "depois de gravar o slot, a tentativa que não gravou não volta largada antes de o \
             endereço da sessão ser escrito: largada sem voltar, ela fecha a conexão e segue \
             gravando o endereço, o alvo e a lista de visitados dela, e volta `Ok`.\n{bloco}"
    );
}

#[test]
fn a_entrada_que_a_janela_larga_e_solta_pela_geracao_dela() {
    let _vez = vez();
    let runtime = runtime();
    let (mut vermelhos, rastro) = crate::rastro_de_teste::capturar(|| {
        runtime.block_on(a_janela_larga_a_velha_antes_de_a_nova_chegar())
    });
    let linhas = rastro
        .lines()
        .filter(|linha| linha.contains("a janela largou") && linha.contains(" WARN "))
        .count();
    if linhas != 1 {
        vermelhos.push(format!(
            "o seele.log tem {linhas} linha(s) WARN de entrada que a janela largou, e não uma: \
                 a que soltou diz, a que não soltou não diz como se tivesse soltado"
        ));
    }
    assert!(
        vermelhos.is_empty(),
        "a entrada que a janela largou não foi solta como devia:\n  {}",
        vermelhos.join("\n  ")
    );
}
