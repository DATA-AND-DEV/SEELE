//! diag-0151 (H): a reprodução do relato 0.15.1 → 0.16 sobre o código da 0.16.0
//! PUBLICADA (8faa3379b), numa exportação dela (`git archive 8faa3379b`) com este
//! arquivo em apps/seele-app/src/ e `#[cfg(test)] mod reproducao_0151;` no main.rs.
//!
//! Onde escreve: `$SEELE_DIAG_RAIZ` (as casas de cada caso e as respostas gravadas
//! para a bancada), ou, sem ela, uma pasta temporária nova deste processo.
//! A pasta completa da 0.15.1 (o caso [c]) vem de `$SEELE_DIAG_FIXTURE`, a saída do
//! `gerar_complemento.rs`; sem ela, o caso [c] diz que não mediu nada.
//!
//! Real: o código do app e do servidor da 0.16.0, a fixture da 0.15.1
//! (`testes/pasta-da-0.15.1`, escrita pelo código da 54f542f94), o banco SQLite
//! de verdade, a `Hospedagem` de verdade (porta 0, sem `$SEELE_ENCONTRO`), o
//! `connect` do app (comando Tauri sobre o runtime falso do `tauri::test`) e um
//! cliente FFI de verdade (QUIC) como a visita.
//!
//! Substituído: o download do catálogo (o `instalar_mod_do_catalogo` baixa e
//! confere a assinatura, e termina em `mods::publicar_de`; aqui o pacote entra
//! pelo `mods::instalar_de`, que chama o mesmo `publicar_de`). O seele/estilo
//! 4.0.0 é o pacote de teste do `versoes.rs` (API 6), não o do catálogo real.
//!
//! Rodar: cargo test -p seele-app --bin seele-app reproducao_0151 -- --nocapture --test-threads=1
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::indexing_slicing,
    reason = "diagnóstico: o pânico e o println são o relatório"
)]

use std::path::{Path, PathBuf};

use tauri::Manager;

const PASTA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testes/pasta-da-0.15.1/pasta");
/// A raiz do diagnóstico: `$SEELE_DIAG_RAIZ`, ou uma pasta temporária nova.
fn raiz() -> &'static Path {
    static RAIZ: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    RAIZ.get_or_init(|| {
        std::env::var_os("SEELE_DIAG_RAIZ").map_or_else(
            || {
                let pasta = std::env::temp_dir()
                    .join(format!("seele-diag-0151-{}", std::process::id()));
                println!("[raiz] SEELE_DIAG_RAIZ vazia: {}", pasta.display());
                pasta
            },
            PathBuf::from,
        )
    })
}

fn copiar(de: &Path, para: &Path) {
    std::fs::create_dir_all(para).unwrap();
    for entrada in std::fs::read_dir(de).unwrap() {
        let entrada = entrada.unwrap();
        let destino = para.join(entrada.file_name());
        if entrada.path().is_dir() {
            copiar(&entrada.path(), &destino);
        } else {
            std::fs::copy(entrada.path(), destino).unwrap();
        }
    }
}

fn casa(nome: &str) -> PathBuf {
    let dir = raiz().join(nome);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A pasta de dados de quem hospedou na 0.15.1, como a 0.15.1 a deixou.
fn restaurar_fixture(dir: &Path) {
    rusqlite::Connection::open(dir.join("seele.db"))
        .unwrap()
        .execute_batch(&std::fs::read_to_string(format!("{PASTA}/seele.sql")).unwrap())
        .unwrap();
    copiar(
        &Path::new(PASTA).join("mod-packages"),
        &dir.join("mod-packages"),
    );
}

/// O seele/estilo 4.0.0 da API deste build (o mesmo pacote de teste do versoes.rs).
fn estilo_4(origem: &Path) {
    std::fs::create_dir_all(origem.join("servidor")).unwrap();
    std::fs::create_dir_all(origem.join("cliente")).unwrap();
    std::fs::write(
        origem.join("mod.json"),
        serde_json::json!({
            "schema": 1,
            "id": "seele/estilo",
            "version": "4.0.0",
            "api": seele_ffi::mods::MOD_API_VERSION,
            "repo": "https://github.com/DATA-AND-DEV/ESTILO",
            "reach": ["tema do servidor"],
            "client": "cliente/main.js",
            "server": "servidor/main.js",
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        origem.join("servidor/main.js"),
        "globalThis.aoPedir = () => JSON.stringify({ ok: true, versao: '4.0.0', theme: dados.theme ?? null });",
    )
    .unwrap();
    std::fs::write(origem.join("cliente/main.js"), "/* sem interface */").unwrap();
}

/// O app com o runtime falso, com a casa em `casa` (SEELE_HOME é do processo:
/// rodar com --test-threads=1).
fn app(casa: &Path) -> &'static tauri::AppHandle<tauri::test::MockRuntime> {
    std::env::set_var("SEELE_HOME", casa);
    let app = tauri::test::mock_builder()
        .manage(super::Session::default())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let handle = app.handle().clone();
    assert_eq!(
        Path::new(&super::config_dir(&handle)),
        casa,
        "a casa do app falso não é a do diagnóstico"
    );
    Box::leak(Box::new(app));
    Box::leak(Box::new(handle))
}

fn esquema(banco: &Path) -> i64 {
    rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |l| l.get(0),
        )
        .unwrap()
}

fn linha_do_estilo(banco: &Path) -> (String, String, i64) {
    rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(
            "SELECT version, hash, enabled FROM mods WHERE id = 'seele/estilo'",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .unwrap()
}

/// Grava a resposta real de um comando, para a bancada a devolver pela ponte.
fn gravar<T: serde::Serialize>(nome: &str, valor: &T) {
    let pasta = raiz().join("respostas-0.16");
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(
        pasta.join(format!("{nome}.json")),
        serde_json::to_string_pretty(valor).unwrap(),
    )
    .unwrap();
}

fn json<T: serde::Serialize>(valor: &T) -> String {
    let texto = serde_json::to_string(valor).unwrap_or_else(|e| format!("<sem json: {e}>"));
    if texto.len() > 600 {
        format!("{}… ({} bytes)", &texto[..600], texto.len())
    } else {
        texto
    }
}

/// (a) O laço: a frase manda atualizar o MOD; quem atualiza pela gestão de MODs
/// (o único lugar de «atualizar MODs» fora das saídas da tela de entrada) não
/// muda o que o hospedar confere.
#[tokio::test(flavor = "multi_thread")]
async fn a_atualizar_pela_gestao_nao_tira_a_recusa_de_hospedar() {
    let dir = casa("a-laco");
    restaurar_fixture(&dir);
    let handle = app(&dir);
    let config = super::config_dir(handle);

    let feito = crate::versoes::primeira_abertura(&config);
    println!("[a] 0. primeira_abertura: {feito:?}");
    crate::mods::levar_pacotes_antigos_para_o_cache(&dir);
    let banco = crate::servidores::banco_de_hospedar(&config, None);
    println!("[a]    banco de hospedar: {} (esquema {})", banco.display(), esquema(&banco));

    let r1 = crate::mods::mods_de_api_antiga(&banco, &crate::servidores::raizes_dos_mods(&config, None));
    println!("[a] 1. hospedar recusa com ModsDeApiAntiga: {r1:?}");
    assert!(!r1.is_empty(), "a fixture da 0.15.1 não foi recusada");

    // 2. «Atualizar MODs» pela gestão: o que `instalar_mod_do_catalogo` com
    // uso "atualizar" decide (main.rs:9406) e grava (catalogo.rs:253 → publicar_de).
    let exigido = super::mod_exigido_aqui(&handle.state::<super::Session>(), "seele/estilo").await;
    let reaplicar = super::reaplica_no_hospedado(super::UsoDoCatalogo::Atualizar, exigido);
    let origem = casa("a-laco-origem-4.0.0");
    estilo_4(&origem);
    let publicado = crate::mods::instalar_de(&dir, &origem).unwrap();
    println!(
        "[a] 2. gestão «atualizar»: mod_exigido_aqui={exigido} → reaplica_no_hospedado={reaplicar}; \
         pacote 4.0.0 no cache, hash {}",
        publicado.hash
    );
    println!("[a]    a linha que o hospedar lê: {:?}", linha_do_estilo(&banco));

    let r2 = crate::mods::mods_de_api_antiga(&banco, &crate::servidores::raizes_dos_mods(&config, None));
    println!("[a] 3. hospedar de novo: {r2:?}");

    // 4. A saída 1 da tela de entrada, que é a única que regrava a linha.
    let saida = crate::mods::SaidaDoModAntigo::Atualizar {
        hash: publicado.hash.clone(),
    };
    let resolvido =
        crate::mods::resolver_mod_de_api_antiga(&config, None, "seele/estilo", &saida, false);
    println!("[a] 4. resolver_mod_de_api_antiga (botão ATUALIZAR E HOSPEDAR): {resolvido:?}");
    println!("[a]    a linha agora: {:?} (esquema {})", linha_do_estilo(&banco), esquema(&banco));
    let r3 = crate::mods::mods_de_api_antiga(&banco, &crate::servidores::raizes_dos_mods(&config, None));
    println!("[a] 5. hospedar depois da saída 1: {r3:?}");

    // O DEFEITO: atualizar pela gestão deixa a recusa igual.
    assert_eq!(
        r2, r1,
        "atualizar pela gestão TIROU a recusa: o laço não se reproduz assim"
    );
    assert!(r3.is_empty(), "a saída 1 não tirou a recusa: {r3:?}");
}

fn contar(banco: &Path, tabela: &str) -> i64 {
    rusqlite::Connection::open_with_flags(banco, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |l| l.get(0))
        .unwrap_or(-1)
}

/// (c) A fixture COMPLETA (gerar_complemento.rs, código da 0.15.1): os dados
/// sobem? A identidade TLS se mantém? A batida que esperava na 0.15.1 aparece?
#[tokio::test(flavor = "multi_thread")]
async fn c_os_dados_da_fixture_completa_sobem_e_a_batida_antiga_aparece() {
    let Some(completa) = std::env::var_os("SEELE_DIAG_FIXTURE")
        .map(|pasta| PathBuf::from(pasta).join("seele.db"))
        .filter(|banco| banco.exists())
    else {
        println!(
            "[c] NÃO MEDIDO: $SEELE_DIAG_FIXTURE não aponta a saída do gerar_complemento.rs \
             (docs/evidencias/diagnostico-0151/)"
        );
        return;
    };
    let completa = completa.as_path();
    let dir = casa("c-completa");
    std::fs::copy(completa, dir.join("seele.db")).unwrap();
    copiar(
        &Path::new(PASTA).join("mod-packages"),
        &dir.join("mod-packages"),
    );
    let handle = app(&dir);
    let config = super::config_dir(handle);
    let banco = crate::servidores::banco_de_hospedar(&config, None);
    let tabelas = ["people", "person_roles", "channels", "voice_rooms", "messages", "mods", "mod_data", "portaria"];
    let antes: Vec<_> = tabelas.iter().map(|t| (*t, contar(&banco, t))).collect();
    println!("[c] 0. 0.15.1, esquema {}: {antes:?}", esquema(&banco));
    crate::versoes::primeira_abertura(&config);
    crate::mods::levar_pacotes_antigos_para_o_cache(&dir);
    let origem = casa("c-completa-origem-4.0.0");
    estilo_4(&origem);
    let publicado = crate::mods::instalar_de(&dir, &origem).unwrap();
    crate::mods::resolver_mod_de_api_antiga(
        &config,
        None,
        "seele/estilo",
        &crate::mods::SaidaDoModAntigo::Atualizar { hash: publicado.hash },
        false,
    )
    .unwrap();
    let minha = seele_ffi::impressao_desta_maquina(&config).unwrap();
    let hospedagem = seele_server::hospedagem::Hospedagem::iniciar(
        0,
        seele_server::persistence::Location::File(banco.clone()),
        "Casa",
        Some(crate::servidores::raizes_dos_mods(&config, None)),
        move |persistence| {
            let _ = seele_server::portaria::semear_ligada(persistence);
            seele_server::portaria::admitir_o_dono(persistence, &minha)
        },
    )
    .await
    .expect("a hospedagem não subiu sobre a fixture completa");
    println!(
        "[c] 1. hospedado; impressão TLS {} (a gerada na 0.15.1 era 96e44bbf…)",
        &hospedagem.impressao_digital()[..16]
    );
    let depois: Vec<_> = tabelas.iter().map(|t| (*t, contar(&banco, t))).collect();
    println!("[c] 2. 0.16, esquema {}: {depois:?}", esquema(&banco));
    handle
        .state::<super::Session>()
        .hospedagens
        .lock()
        .unwrap()
        .guardado = Some(super::Hospedado {
        hospedagem,
        servidor: None,
    });
    let porta = super::estado_da_porta(handle.state()).await;
    let pedidos = super::pedidos_da_portaria(handle.state()).await.ok().unwrap_or_default();
    println!("[c] 3. estado_da_porta: {}", json(&porta.ok()));
    println!("[c] 4. pedidos_da_portaria: {}", json(&pedidos));
    // Todo seele.db da casa: o de hospedar e a cópia da primeira abertura (a linha 0.15).
    fn achar(dir: &Path, achados: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                achar(&p, achados);
            } else if p.file_name().is_some_and(|n| n == "seele.db") {
                achados.push(p);
            }
        }
    }
    let mut achados = Vec::new();
    achar(&dir, &mut achados);
    for b in achados {
        println!(
            "[c] 5. {}: esquema {} mensagens {} canais {}",
            b.display(),
            esquema(&b),
            contar(&b, "messages"),
            contar(&b, "channels")
        );
    }
    if let Some(g) = handle.state::<super::Session>().hospedagens.lock().unwrap().guardado.take() {
        g.hospedagem.encerrar().await;
    }
}

/// (b) Hospedado, o dono, a visita e a porta, sem a UI.
#[tokio::test(flavor = "multi_thread")]
async fn b_hospedado_o_dono_a_visita_e_a_porta() {
    let dir = casa("b-hospedar");
    restaurar_fixture(&dir);
    let handle = app(&dir);
    let config = super::config_dir(handle);
    crate::versoes::primeira_abertura(&config);
    crate::mods::levar_pacotes_antigos_para_o_cache(&dir);
    let banco = crate::servidores::banco_de_hospedar(&config, None);
    let origem = casa("b-hospedar-origem-4.0.0");
    estilo_4(&origem);
    let publicado = crate::mods::instalar_de(&dir, &origem).unwrap();
    crate::mods::resolver_mod_de_api_antiga(
        &config,
        None,
        "seele/estilo",
        &crate::mods::SaidaDoModAntigo::Atualizar {
            hash: publicado.hash.clone(),
        },
        false,
    )
    .unwrap();
    assert!(
        crate::mods::mods_de_api_antiga(&banco, &crate::servidores::raizes_dos_mods(&config, None))
            .is_empty()
    );

    // O hospedar, com o gancho do comando (main.rs:3704-3725), em porta 0.
    let minha = seele_ffi::impressao_desta_maquina(&config).unwrap();
    let dono = minha.clone();
    let hospedagem = seele_server::hospedagem::Hospedagem::iniciar(
        0,
        seele_server::persistence::Location::File(banco.clone()),
        "Casa",
        Some(crate::servidores::raizes_dos_mods(&config, None)),
        move |persistence| {
            if let Err(erro) = seele_server::portaria::semear_ligada(persistence) {
                println!("[b] semear_ligada falhou: {erro:#}");
            }
            seele_server::portaria::admitir_o_dono(persistence, &dono)
        },
    )
    .await
    .expect("a hospedagem não subiu sobre a fixture");
    let porta = hospedagem.endereco().port();
    let impressao = hospedagem.impressao_digital().to_owned();
    // O `Anfitriao` que o comando `hospedar` montaria (main.rs:3730-3742),
    // com o convite real deste servidor.
    gravar(
        "hospedar",
        &serde_json::json!({
            "aqui": format!("127.0.0.1:{porta}"),
            "convite": hospedagem.convite_com_nome(None),
            "alcance": hospedagem.alcance().map_or("SoRedeLocal", |a| a.degrau().nome()),
            "porta_recusada": null,
            "encontro_recusado": null,
            "firewall": null,
            "porta_nao_canonica": null,
        }),
    );
    println!(
        "[b] 1. hospedado: {} fp {}… alcance {:?}; esquema {}; dono admitido {}…",
        hospedagem.endereco(),
        &impressao[..16],
        hospedagem.alcance().map(|a| a.degrau().nome()),
        esquema(&banco),
        &minha[..16]
    );
    handle
        .state::<super::Session>()
        .hospedagens
        .lock()
        .unwrap()
        .guardado = Some(super::Hospedado {
        hospedagem,
        servidor: None,
    });

    let porta0 = super::estado_da_porta(handle.state()).await;
    println!("[b] 2. estado_da_porta, sem sessão do dono: {}", json(&porta0.as_ref().ok()));
    gravar("estado_da_porta-0", &porta0.ok());

    // O `conectar()` que a tela de entrada faz depois de hospedar.
    let entrada = super::connect(
        handle.clone(),
        handle.state(),
        format!("127.0.0.1:{porta}"),
        String::new(),
        false,
        None,
        None,
    )
    .await;
    match &entrada {
        Ok(e) => println!(
            "[b] 3. connect do dono: OK, veredito {} geracao {}",
            json(&e.veredito),
            e.geracao
        ),
        Err(f) => {
            println!("[b] 3. connect do dono: FALHOU {}", json(f));
            gravar("connect-dono", f);
        }
    }
    // 3b. O dono ACEITA a pergunta dos MODs do próprio servidor (o sim que o
    // ACEITAR E ENTRAR grava) e entra de novo.
    let conjunto = serde_json::to_value(entrada.as_ref().err())
        .ok()
        .and_then(|v| v.pointer("/error/ModsNaoAceitos/conjunto").and_then(|c| c.as_str().map(str::to_owned)));
    if let Some(conjunto) = conjunto {
        let aceito = seele_ffi::mods::aceitar(&config, &format!("127.0.0.1:{porta}"), &conjunto);
        let de_novo = super::connect(
            handle.clone(),
            handle.state(),
            format!("127.0.0.1:{porta}"),
            String::new(),
            false,
            None,
            None,
        )
        .await;
        println!(
            "[b] 3b. o dono aceita ({:?}) e entra de novo: {}",
            aceito,
            match &de_novo {
                Ok(e) => format!("OK, geracao {}", e.geracao),
                Err(f) => format!("FALHOU {}", json(f)),
            }
        );
        let onde = super::onde_estou_na(&handle.state::<super::Session>());
        println!("[b]     onde_estou: {onde:?}");
    }

    // A visita: outra chave, cliente FFI de verdade.
    let visita = casa("b-hospedar-visita");
    let conectar_visita = |visita: &Path| {
        let cfg = seele_ffi::ConnectConfig {
            server: format!("127.0.0.1:{porta}"),
            alternate_servers: Vec::new(),
            nickname: "visita".to_owned(),
            home: visita.to_string_lossy().into_owned(),
            audio: false,
            join_secret: None,
            expected_fingerprint: Some(impressao.clone()),
            codigo_esperado: None,
            bilhete: None,
            capture_device: None,
            playback_device: None,
        };
        tauri::async_runtime::spawn_blocking(move || seele_ffi::Connection::connect(cfg))
    };
    let r = conectar_visita(&visita).await.unwrap();
    println!(
        "[b] 4. a visita bate: {}",
        match &r {
            Ok(_) => "ENTROU (sem portaria?)".to_owned(),
            Err(e) => format!("{e:?}"),
        }
    );
    let porta1 = super::estado_da_porta(handle.state()).await;
    println!("[b] 5. estado_da_porta: {}", json(&porta1.as_ref().ok()));
    gravar("estado_da_porta-1", &porta1.ok());
    let pedidos = super::pedidos_da_portaria(handle.state()).await.ok().unwrap_or_default();
    println!("[b] 6. pedidos_da_portaria: {}", json(&pedidos));
    gravar("pedidos_da_portaria-1", &pedidos);
    let pendente = pedidos.iter().find(|p| p.decidido_em.is_none()).map(|p| p.impressao.clone());
    if let Some(impressao_da_visita) = pendente {
        let decidido = super::decidir_pedido(handle.state(), impressao_da_visita, true).await;
        println!("[b] 7. decidir_pedido(admitir): {:?}", decidido.is_ok());
        let r2 = conectar_visita(&visita).await.unwrap();
        println!(
            "[b] 8. a visita de novo: {}",
            match &r2 {
                Ok(_) => "ENTROU".to_owned(),
                Err(e) => format!("{e:?}"),
            }
        );
    }
    let porta2 = super::estado_da_porta(handle.state()).await;
    println!("[b] 9. estado_da_porta: {}", json(&porta2.ok()));

    let guardado = handle
        .state::<super::Session>()
        .hospedagens
        .lock()
        .unwrap()
        .guardado
        .take();
    if let Some(g) = guardado {
        g.hospedagem.encerrar().await;
    }
}
