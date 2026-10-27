//! **A outra ponta de uma ligação pessoal, para o CUA validar o app nativo de
//! ponta a ponta** (0.17), sem produção e sem hardware: um ponto de encontro de
//! verdade no loopback e a sala pessoal de «outra pessoa», registrada nele, com
//! um código estável entre execuções. É uma fixture de TESTE: nenhum caminho de
//! produto novo, só a mesma montagem de `apps/seele-app/src/ligar.rs`
//! (`subir_o_ponto`, `sala_registrada`) e o mesmo atender da recepção.
//!
//! ```text
//! cargo run -p seele-conformance --example ligacao_pessoal_de_bancada -- \
//!     --ponto 127.0.0.1:18420 --casa /tmp/seele-fixture-casa \
//!     --atender-sozinho --voz-sintetica
//! ```
//!
//! O app de quem liga sobe com `SEELE_ENCONTRO=127.0.0.1:18420` e
//! `SEELE_MODO_LOCAL=1`, e disca o `alvo=` impresso.
//!
//! - `--ponto <ip:porta>`: onde o ponto de encontro escuta (obrigatório, e só
//!   loopback: nunca o de produção);
//! - `--casa <DIR>`: pasta descartável com `identity.key`; criada com uma
//!   semente aleatória se faltar, e reaproveitada depois (o código não muda);
//! - `--porta-da-sala <N>`: porta da sala (0 = qualquer);
//! - `--atender-sozinho`: flag explícita de teste. Ao tocar, atende pelo mesmo
//!   `Recepcao::atender` que o produto usa. Sem ela, só imprime o toque;
//! - `--voz-sintetica`: o DONO entra na própria sala (pelo endereço
//!   `127.0.0.1:<porta>`, conferindo o próprio código, como o ATENDER do app) e
//!   fala o tom do aparelho de mentira, para quem ligou ouvir alguém. Uma única
//!   conexão, uma única sala: nenhuma voz em outro servidor.
//!
//! Saída: linhas `chave=valor` com `hora=` UTC. A de abertura traz `ponto=`,
//! `porta_da_sala=`, `codigo=` e `alvo=`; depois, uma `evento=` por fato
//! (`registrado`, `tocou`, `atendeu`, `entrou`, `saiu`, `recusou`, ...).
//! Termina limpo em Ctrl-C ou SIGTERM.
//!
//! **Limites:** o toque, o atender, o ponto e o registro são os de verdade; o
//! tom é do aparelho de mentira (nenhum microfone nem saída de som). `recusou`
//! só aparece para o toque que a sala recusa depois de tocar; recusas sem tocar
//! (bloqueio, excesso) não passam pelo aviso da recepção.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "uma fixture de bancada: o pânico e a linha são o relatório"
)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use seele_audio::device::mentira;
use seele_ffi::{ConnectConfig, Connection, VoiceMode};
use seele_server::alcance::encontro::EstadoDoRegistro;
use seele_server::campainha::Desfecho;
use seele_server::hospedagem::Hospedagem;
use seele_server::recepcao::Aviso;
use seele_server::server::Event;

/// Os argumentos, já lidos.
#[derive(Debug, PartialEq, Eq)]
struct Opcoes {
    ponto: SocketAddr,
    casa: PathBuf,
    porta_da_sala: u16,
    atender_sozinho: bool,
    voz_sintetica: bool,
}

fn ler(args: impl IntoIterator<Item = String>) -> Result<Opcoes, String> {
    let mut args = args.into_iter();
    let (mut ponto, mut casa, mut porta, mut atender, mut voz) = (None, None, 0_u16, false, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ponto" => {
                let valor = args.next().ok_or("--ponto pede ip:porta")?;
                ponto = Some(
                    valor
                        .parse::<SocketAddr>()
                        .map_err(|erro| format!("--ponto {valor}: {erro}"))?,
                );
            }
            "--casa" => casa = Some(PathBuf::from(args.next().ok_or("--casa pede uma pasta")?)),
            "--porta-da-sala" => {
                let valor = args.next().ok_or("--porta-da-sala pede um número")?;
                porta = valor
                    .parse()
                    .map_err(|erro| format!("--porta-da-sala {valor}: {erro}"))?;
            }
            "--atender-sozinho" => atender = true,
            "--voz-sintetica" => voz = true,
            outro => return Err(format!("argumento desconhecido: {outro}")),
        }
    }
    let ponto = ponto.ok_or("--ponto 127.0.0.1:<porta> é obrigatório")?;
    if !ponto.ip().is_loopback() {
        return Err("--ponto só aceita loopback: esta fixture nunca fala com a produção".into());
    }
    Ok(Opcoes {
        ponto,
        casa: casa.ok_or("--casa <DIR> é obrigatório")?,
        porta_da_sala: porta,
        atender_sozinho: atender,
        voz_sintetica: voz,
    })
}

/// A hora UTC do relatório, `HH:MM:SS.mmmZ`, como a do log do app.
fn hora() -> String {
    hora_de(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default(),
    )
}

fn hora_de(desde_a_epoca: Duration) -> String {
    let segundos = desde_a_epoca.as_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}.{:03}Z",
        segundos / 3_600,
        segundos / 60 % 60,
        segundos % 60,
        desde_a_epoca.subsec_millis()
    )
}

/// Uma linha `chave=valor`, com a hora na frente.
fn linha(hora: &str, campos: &[(&str, String)]) -> String {
    let mut fora = format!("hora={hora}");
    for (chave, valor) in campos {
        fora.push(' ');
        fora.push_str(chave);
        fora.push('=');
        fora.push_str(&valor.replace(' ', "_"));
    }
    fora
}

fn evento(nome: &str, campos: &[(&str, String)]) {
    let mut todos = vec![("evento", nome.to_owned())];
    todos.extend(campos.iter().map(|(c, v)| (*c, v.clone())));
    println!("{}", linha(&hora(), &todos));
}

/// A casa com `identity.key`: cria com 32 bytes aleatórios se faltar, e deixa
/// em paz se existir (a identidade fica estável entre execuções).
fn preparar_a_casa(casa: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(casa)?;
    let chave = casa.join("identity.key");
    if !chave.exists() {
        std::fs::write(&chave, rand::random::<[u8; 32]>())?;
    }
    Ok(())
}

/// O ponto de encontro de verdade, na porta dita (a mesma montagem de
/// `subir_o_ponto` em `apps/seele-app/src/ligar.rs`).
fn subir_o_ponto(onde: SocketAddr) -> anyhow::Result<SocketAddr> {
    let ponto = seele_encontro::Ponto::abrir_com_quarto(
        onde,
        seele_proto::encontro::Vizinhanca::TambemAqui,
        Arc::new(seele_encontro::Quarto::novo()),
    )?;
    let ligado = ponto.endereco()?;
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    Ok(ligado)
}

fn palavra(desfecho: Desfecho) -> &'static str {
    match desfecho {
        Desfecho::Atendida => "atendeu",
        Desfecho::Recusada => "recusou",
        Desfecho::NaoAtendida => "ninguem_atendeu",
        Desfecho::Cancelada => "caiu_antes",
        Desfecho::Cheia => "cheia",
    }
}

/// Espera Ctrl-C ou SIGTERM.
async fn esperar_o_fim() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("o SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opcoes = match ler(std::env::args().skip(1)) {
        Ok(opcoes) => opcoes,
        Err(erro) => {
            eprintln!("{erro}");
            std::process::exit(2);
        }
    };
    anyhow::ensure!(
        std::env::var_os("SEELE_IDENTIDADE").is_none(),
        "SEELE_IDENTIDADE está posta e mandaria na --casa: tire-a deste processo"
    );
    preparar_a_casa(&opcoes.casa)?;
    let home = opcoes.casa.display().to_string();

    let ponto = subir_o_ponto(opcoes.ponto)?;
    let semente = seele_ffi::semente_da_sala(&home)?;
    let sala = Hospedagem::iniciar_sala(
        opcoes.porta_da_sala,
        semente.bytes(),
        "dona-da-fixture",
        Some(ponto.to_string()),
    )
    .await?;
    drop(semente);
    let mut registro = sala
        .registro_no_quarto()
        .ok_or_else(|| anyhow::anyhow!("a sala subiu sem registro no ponto"))?;
    let registrou = tokio::time::timeout(
        Duration::from_secs(10),
        registro.wait_for(|estado| *estado == EstadoDoRegistro::Registrado),
    )
    .await
    .is_ok_and(|visto| visto.is_ok());
    anyhow::ensure!(
        registrou,
        "a sala não se registrou no ponto: {:?}",
        *registro.borrow()
    );

    let codigo = seele_ffi::codigo_desta_maquina(&home)?;
    let porta_da_sala = sala.endereco().port();
    println!(
        "{}",
        linha(
            &hora(),
            &[
                ("ponto", ponto.to_string()),
                ("porta_da_sala", porta_da_sala.to_string()),
                ("codigo", codigo.to_string()),
                ("alvo", seele_ffi::alvo_do_codigo(&codigo)),
                ("atender_sozinho", opcoes.atender_sozinho.to_string()),
                ("voz_sintetica", opcoes.voz_sintetica.to_string()),
            ]
        )
    );
    evento("registrado", &[("codigo", codigo.to_string())]);

    // O registro depois do primeiro: se o ponto some, a linha diz.
    tokio::spawn(async move {
        while registro.changed().await.is_ok() {
            let estado = format!("{:?}", *registro.borrow());
            evento("registro", &[("estado", estado)]);
        }
    });

    // Quem entra e sai da sala (o servidor da sala, pelo barramento dele).
    let mut eventos = sala.servidor().events.subscribe();
    tokio::spawn(async move {
        let mut nomes = std::collections::HashMap::new();
        loop {
            match eventos.recv().await {
                Ok(Event::PersonPresent { quem }) => {
                    evento("entrou", &[("quem", quem.nickname.clone())]);
                    nomes.insert(quem.person, quem.nickname);
                }
                Ok(Event::PersonGone { person }) => {
                    let quem = nomes.remove(&person).unwrap_or_else(|| "?".to_owned());
                    evento("saiu", &[("quem", quem)]);
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });

    // A recepção: o toque e o atender, como em `ligar.rs`.
    let recepcao = sala
        .recepcao()
        .ok_or_else(|| anyhow::anyhow!("a sala não tem recepção"))?;
    let mut avisos = recepcao.assinar();
    let atender_sozinho = opcoes.atender_sozinho;
    let na_recepcao = Arc::clone(&recepcao);
    tokio::spawn(async move {
        loop {
            match avisos.recv().await {
                Ok(Aviso::Tocou {
                    toque,
                    impressao,
                    codigo,
                    apelido,
                }) => {
                    evento(
                        "tocou",
                        &[
                            ("toque", format!("{toque:?}")),
                            ("apelido", apelido),
                            ("codigo_de_quem_liga", codigo.to_string()),
                            ("impressao", impressao),
                        ],
                    );
                    if atender_sozinho {
                        let atendeu = na_recepcao.atender(toque);
                        evento("atendendo_sozinho", &[("aceito", atendeu.to_string())]);
                    }
                }
                Ok(Aviso::ParouDeTocar {
                    toque,
                    impressao,
                    desfecho,
                }) => evento(
                    palavra(desfecho),
                    &[("toque", format!("{toque:?}")), ("impressao", impressao)],
                ),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });

    // O dono, falando na própria sala (opcional).
    let mut dono = None;
    if opcoes.voz_sintetica {
        let config = ConnectConfig {
            server: SocketAddr::from(([127, 0, 0, 1], porta_da_sala)).to_string(),
            alternate_servers: Vec::new(),
            nickname: "dona-da-fixture".into(),
            home: home.clone(),
            join_secret: None,
            expected_fingerprint: None,
            codigo_esperado: Some(codigo),
            bilhete: None,
            audio: true,
            capture_device: Some(mentira::id("fixture-dono")),
            playback_device: Some(mentira::id("fixture-dono")),
        };
        let foco = seele_ffi::foco::FocoDeVoz::novo();
        let (conexao, _) = tokio::task::spawn_blocking(move || {
            Connection::connect_com_foco(config, None, None, Some(foco), None)
        })
        .await?
        .map_err(|falha| anyhow::anyhow!("o dono não entrou na própria sala: {falha:?}"))?;
        conexao.set_voice_mode(VoiceMode::Open);
        let sala_de_voz = conexao
            .snapshot()
            .voice_rooms
            .first()
            .map(|sala| sala.id)
            .ok_or_else(|| anyhow::anyhow!("a própria sala do dono veio sem sala de voz"))?;
        conexao
            .enter_voice_room(sala_de_voz, None)
            .map_err(|erro| anyhow::anyhow!("o dono não sentou: {erro:?}"))?;
        evento("dono_falando", &[("tom", "mentira".to_owned())]);
        dono = Some(conexao);
    }

    esperar_o_fim().await;
    evento("encerrando", &[]);
    if let Some(conexao) = dono {
        let _ =
            tokio::task::spawn_blocking(move || conexao.encerrar_e_esperar(Duration::from_secs(5)))
                .await;
    }
    sala.encerrar().await;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn args(texto: &str) -> Vec<String> {
        texto.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn os_argumentos_se_leem_e_a_producao_nao_passa() {
        let ok = ler(args(
            "--ponto 127.0.0.1:18420 --casa /tmp/x --porta-da-sala 7 --atender-sozinho",
        ))
        .expect("argumentos válidos");
        assert_eq!(ok.ponto.port(), 18420);
        assert_eq!(ok.porta_da_sala, 7);
        assert!(ok.atender_sozinho && !ok.voz_sintetica);
        assert!(ler(args("--ponto 8.8.8.8:1 --casa /tmp/x")).is_err());
        assert!(ler(args("--casa /tmp/x")).is_err());
        assert!(ler(args("--ponto 127.0.0.1:1 --casa /tmp/x --nada")).is_err());
    }

    #[test]
    fn a_identidade_da_casa_e_estavel_e_o_codigo_tem_a_forma_da_tela() {
        let pasta = tempfile::tempdir().unwrap();
        let casa = pasta.path().join("casa");
        preparar_a_casa(&casa).unwrap();
        let primeira = std::fs::read(casa.join("identity.key")).unwrap();
        preparar_a_casa(&casa).unwrap();
        assert_eq!(primeira, std::fs::read(casa.join("identity.key")).unwrap());
        let home = casa.display().to_string();
        let codigo = seele_ffi::codigo_desta_maquina(&home).unwrap().to_string();
        let grupos: Vec<_> = codigo.split('-').collect();
        assert!(
            grupos.len() == 4 && grupos.iter().all(|g| g.len() == 4),
            "{codigo}"
        );
        assert_eq!(
            codigo,
            seele_ffi::codigo_desta_maquina(&home).unwrap().to_string()
        );
    }

    #[test]
    fn a_linha_e_chave_valor_com_hora() {
        let hora = hora_de(Duration::from_millis(3_661_042));
        assert_eq!(hora, "01:01:01.042Z");
        assert_eq!(
            linha(
                &hora,
                &[("evento", "tocou".into()), ("apelido", "a b".into())]
            ),
            "hora=01:01:01.042Z evento=tocou apelido=a_b"
        );
    }
}
