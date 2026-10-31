//! **O perfil público no servidor** (0.19, `docs/0.19/perfis-oficiais.md`, §7
//! «Servidor»): a revisão, a linha da sessão, a difusão com o portão de versão,
//! e as imagens (teto, tipo, truncamento, troca).
//!
//! # Por que um par cru, em arquivo
//!
//! O que está sob teste é a decisão do servidor, e o cliente de verdade não a
//! deixa ser tomada de outro jeito: ele valida antes de enviar. O banco é um
//! **arquivo**, porque a pasta das imagens (`perfis/`) sai do caminho dele; o
//! servidor em memória não tem onde pôr uma imagem, e há um teste só para isso.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::PersonId;
use seele_proto::perfil::{
    CampoDoPerfil, Efeito, LadoDoPerfil, MotivoDoCampo, PerfilPublico, RecusaDePerfil,
    TipoDaImagem, MAX_PERFIS_POR_PARTE, TETO_DA_IMAGEM,
};
use seele_proto::volume::{PedidoDeImagem, RespostaDeImagem};
use seele_server::persistence::{Location, Persistence};
use seele_server::{frame, Daemon, ServerConfig};
use tokio::sync::mpsc;

mod comum;

const PRAZO: Duration = Duration::from_secs(10);

struct Casa {
    endereco: SocketAddr,
    daemon: Arc<Daemon>,
    pasta: tempfile::TempDir,
}

impl Casa {
    /// A pasta das imagens, ao lado de `seele.db`.
    fn imagens(&self) -> PathBuf {
        self.pasta.path().join("perfis")
    }

    /// Os arquivos de imagem de verdade (sem os de obras), por nome.
    fn arquivos(&self) -> Vec<String> {
        let mut nomes: Vec<String> = std::fs::read_dir(self.imagens())
            .map(|dir| {
                dir.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        nomes.sort();
        nomes
    }

    async fn perfil(&self, pessoa: PersonId) -> Option<PerfilPublico> {
        let db = self.daemon.server().persistence.lock().await;
        seele_server::persistence::perfis::ler(&db, pessoa).unwrap()
    }

    async fn todos(&self) -> Vec<PerfilPublico> {
        let db = self.daemon.server().persistence.lock().await;
        seele_server::persistence::perfis::todos(&db).unwrap()
    }
}

/// Sobe um servidor com banco em arquivo, depois de `semear` (que mexe no banco
/// antes do `bind`).
async fn casa_com(semear: impl FnOnce(&Persistence)) -> Result<Casa> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    {
        let persistence = Persistence::open(&Location::File(banco.clone()))?;
        semear(&persistence);
    }
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco),
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let aceitando = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok(Casa {
        endereco,
        daemon,
        pasta,
    })
}

async fn casa() -> Result<Casa> {
    casa_com(|_| {}).await
}

/// Um par: a conexão, as mensagens que o servidor manda (lidas por uma tarefa,
/// porque ler com prazo um quadro pela metade desalinharia o fluxo) e a pessoa.
struct Par {
    conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    caixa: mpsc::UnboundedReceiver<ServerMessage>,
    pessoa: PersonId,
    _ponta: seele_enlace::Ponta,
}

/// Abre uma conexão e faz o aperto de mão. `maxima` é o maior número que o par
/// fala: 11 é uma 0.18, que não conhece nada do perfil.
async fn abrir(endereco: SocketAddr, semente: u8, maxima: u8) -> Result<Par> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::version::Faixa {
                minima: seele_proto::version::VERSAO_MINIMA,
                maxima,
            },
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&comum::prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session { person, .. } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    let (dentro, caixa) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok(mensagem) = frame::read::<ServerMessage>(&mut recebe).await {
            if dentro.send(mensagem).is_err() {
                break;
            }
        }
    });
    Ok(Par {
        conexao,
        envio,
        caixa,
        pessoa: person,
        _ponta: ponta,
    })
}

impl Par {
    /// Espera uma mensagem que sirva, ou o prazo.
    async fn esperar<T>(
        &mut self,
        mut serve: impl FnMut(&ServerMessage) -> Option<T>,
    ) -> Option<T> {
        let fim = tokio::time::Instant::now() + PRAZO;
        loop {
            let resto = fim.checked_duration_since(tokio::time::Instant::now())?;
            let mensagem = tokio::time::timeout(resto, self.caixa.recv())
                .await
                .ok()??;
            if let Some(achado) = serve(&mensagem) {
                return Some(achado);
            }
        }
    }

    /// Tudo o que chegar até o `Pong` de um `Ping` mandado agora. Como o
    /// servidor atende em ordem, o que ainda não chegou até o `Pong` é porque
    /// não vem — desde que o evento já estivesse no barramento antes do ping.
    async fn tudo_ate_o_pong(&mut self) -> Result<Vec<ServerMessage>> {
        tokio::time::sleep(Duration::from_millis(300)).await;
        frame::write(&mut self.envio, &ClientMessage::Ping { timestamp: 99 }).await?;
        let mut vistas = Vec::new();
        let fim = tokio::time::Instant::now() + PRAZO;
        loop {
            let resto = fim
                .checked_duration_since(tokio::time::Instant::now())
                .ok_or_else(|| anyhow::anyhow!("sem Pong"))?;
            let mensagem = tokio::time::timeout(resto, self.caixa.recv())
                .await?
                .ok_or_else(|| anyhow::anyhow!("fluxo fechado"))?;
            if matches!(mensagem, ServerMessage::Pong { timestamp: 99 }) {
                return Ok(vistas);
            }
            vistas.push(mensagem);
        }
    }

    async fn gravar(&mut self, revisao: u32, nome: &str) -> Result<()> {
        frame::write(
            &mut self.envio,
            &ClientMessage::GravarPerfil {
                revisao,
                nome_exibido: nome.into(),
                pronomes: "ela/dela".into(),
                status: "no ar".into(),
                bio: "linha um\nlinha dois".into(),
                cor: [0x12, 0x34, 0x56],
                efeito: Efeito::Aurora,
            },
        )
        .await
    }

    async fn perfil_mudou(&mut self, de: PersonId) -> Option<PerfilPublico> {
        self.esperar(|m| match m {
            ServerMessage::PerfilMudou(p) if p.pessoa == de => Some(p.clone()),
            _ => None,
        })
        .await
    }

    async fn recusa(&mut self) -> Option<(RecusaDePerfil, Vec<seele_proto::perfil::ErroDeCampo>)> {
        self.esperar(|m| match m {
            ServerMessage::PerfilRecusado { motivo, campos } => Some((*motivo, campos.clone())),
            _ => None,
        })
        .await
    }

    /// Envia uma imagem pelo fluxo bidirecional e devolve a resposta do fluxo.
    /// `declarado` e `corpo` podem divergir, que é como se fabrica um envio
    /// truncado ou grande demais.
    async fn enviar_imagem(
        &self,
        lado: LadoDoPerfil,
        declarado: u64,
        corpo: Vec<u8>,
    ) -> Result<RespostaDeImagem> {
        let (mut envia, mut recebe) = self.conexao.open_bi().await?;
        frame::write(
            &mut envia,
            &PedidoDeImagem::EnviarAoPerfil {
                lado,
                bytes: declarado,
            },
        )
        .await?;
        // Numa tarefa: o servidor pode parar de ler no meio (o teto), e quem
        // escreve não pode ficar preso esperando espaço enquanto a resposta
        // que o liberaria está na outra ponta.
        let escritora = tokio::spawn(async move {
            for bloco in corpo.chunks(65536) {
                if envia.write_all(bloco).await.is_err() {
                    return;
                }
            }
            let _ = envia.finish();
        });
        let resposta =
            tokio::time::timeout(PRAZO, frame::read::<RespostaDeImagem>(&mut recebe)).await??;
        escritora.abort();
        Ok(resposta)
    }

    /// Lê a imagem de um perfil; `Err` com o texto do servidor.
    async fn ler_imagem(
        &self,
        de: PersonId,
        lado: LadoDoPerfil,
        hash: [u8; 32],
    ) -> Result<std::result::Result<Vec<u8>, String>> {
        let (mut envia, mut recebe) = self.conexao.open_bi().await?;
        frame::write(
            &mut envia,
            &PedidoDeImagem::LerDoPerfil {
                pessoa: de,
                lado,
                hash,
            },
        )
        .await?;
        envia.finish()?;
        let resposta: RespostaDeImagem = frame::read(&mut recebe).await?;
        if let Some(erro) = resposta.erro {
            return Ok(Err(erro));
        }
        let bytes = recebe.read_to_end(TETO_DA_IMAGEM).await?;
        anyhow::ensure!(bytes.len() as u64 == resposta.bytes, "resposta truncada");
        Ok(Ok(bytes))
    }
}

/// Um PNG de mentira (só a assinatura conta) com o conteúdo marcado por `marca`.
fn png(tamanho: usize, marca: u8) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.resize(tamanho, marca);
    v
}

fn sha(bytes: &[u8]) -> [u8; 32] {
    let mut soma = seele_proto::attachment::ContentDigest::new();
    soma.feed(bytes);
    soma.finish()
}

fn hex(hash: &[u8; 32]) -> String {
    seele_proto::attachment::hex(hash)
}

fn arquivos_de_obras(pasta: &Path) -> usize {
    std::fs::read_dir(pasta)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with(".em-obras-"))
                .count()
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Revisão e linha da sessão
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uma_revisao_velha_e_recusada_e_nada_muda() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;

    // Quem ainda não tem linha grava com a revisão 0.
    a.gravar(0, "Ana").await?;
    let primeiro = a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");
    assert_eq!(primeiro.revisao, 1);
    assert_eq!(primeiro.nome_exibido, "Ana");
    assert_eq!(primeiro.bio, "linha um\nlinha dois");
    assert_eq!(primeiro.cor, [0x12, 0x34, 0x56]);

    // O editor que abriu antes do primeiro salvar ainda tem a revisão 0.
    a.gravar(0, "Ana de outra janela").await?;
    let (motivo, campos) = a.recusa().await.expect("PerfilRecusado");
    assert_eq!(motivo, RecusaDePerfil::RevisaoVelha);
    assert!(campos.is_empty());
    assert_eq!(casa.perfil(a.pessoa).await.unwrap().nome_exibido, "Ana");

    // Com a revisão certa, passa, e a revisão sobe.
    a.gravar(1, "Ana Maria").await?;
    assert_eq!(
        a.perfil_mudou(a.pessoa).await.expect("PerfilMudou").revisao,
        2
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn so_a_linha_da_sessao_muda() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let mut b = abrir(casa.endereco, 2, 12).await?;
    assert_ne!(a.pessoa, b.pessoa);

    a.gravar(0, "Ana").await?;
    a.perfil_mudou(a.pessoa).await.expect("PerfilMudou de A");
    let linhas = casa.todos().await;
    assert_eq!(linhas.len(), 1, "gravar criou mais de uma linha");
    assert_eq!(linhas[0].pessoa, a.pessoa);
    assert!(casa.perfil(b.pessoa).await.is_none());

    // B grava o dele; a linha de A fica como estava.
    b.gravar(0, "Beto").await?;
    b.perfil_mudou(b.pessoa).await.expect("PerfilMudou de B");
    let de_a = casa.perfil(a.pessoa).await.unwrap();
    assert_eq!((de_a.nome_exibido.as_str(), de_a.revisao), ("Ana", 1));
    assert_eq!(casa.perfil(b.pessoa).await.unwrap().nome_exibido, "Beto");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn campos_invalidos_voltam_por_campo_so_para_quem_pediu() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let mut b = abrir(casa.endereco, 2, 12).await?;

    // U+202E (inversão de direção) é recusado no nome, e a quebra de linha
    // também: só a bio a aceita.
    a.gravar(0, "Ana\u{202E}").await?;
    frame::write(
        &mut a.envio,
        &ClientMessage::GravarPerfil {
            revisao: 0,
            nome_exibido: "Ana".into(),
            pronomes: "ela\nela".into(),
            status: String::new(),
            bio: String::new(),
            cor: [0, 0, 0],
            efeito: Efeito::Nenhum,
        },
    )
    .await?;
    let (motivo, campos) = a.recusa().await.expect("PerfilRecusado");
    assert_eq!(motivo, RecusaDePerfil::Campos);
    assert!(campos
        .iter()
        .any(|e| e.campo == CampoDoPerfil::NomeExibido && e.motivo == MotivoDoCampo::Controle));
    let (_, campos) = a.recusa().await.expect("a segunda recusa");
    assert!(campos
        .iter()
        .any(|e| e.campo == CampoDoPerfil::Pronomes && e.motivo == MotivoDoCampo::Controle));
    assert!(casa.todos().await.is_empty());

    // B não ouve falar das recusas de A.
    let vistas = b.tudo_ate_o_pong().await?;
    assert!(
        !vistas.iter().any(|m| matches!(
            m,
            ServerMessage::PerfilRecusado { .. } | ServerMessage::PerfilMudou(..)
        )),
        "{vistas:?}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Difusão e portão de versão
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perfil_mudou_chega_a_todos_menos_a_um_par_onze() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let mut b = abrir(casa.endereco, 2, 12).await?;
    let mut velho = abrir(casa.endereco, 3, 11).await?;

    a.gravar(0, "Ana").await?;
    let em_a = a
        .perfil_mudou(a.pessoa)
        .await
        .expect("A não recebeu o próprio");
    let em_b = b.perfil_mudou(a.pessoa).await.expect("B não recebeu");
    assert_eq!(em_a, em_b);
    assert_eq!(em_a.nome_exibido, "Ana");

    // O par 11 não recebe nada do perfil: nem a mudança, nem a lista da
    // entrada. Um quadro que ele não conhece desalinharia o fluxo dele.
    let vistas = velho.tudo_ate_o_pong().await?;
    assert!(
        !vistas.iter().any(|m| matches!(
            m,
            ServerMessage::PerfilMudou(..)
                | ServerMessage::Perfis { .. }
                | ServerMessage::PerfilRecusado { .. }
                | ServerMessage::PendenciaDoPerfil { .. }
        )),
        "um par 11 recebeu o vocabulário do perfil: {vistas:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn os_perfis_chegam_em_partes_depois_da_sessao() -> Result<()> {
    const TOTAL: usize = MAX_PERFIS_POR_PARTE + 2;
    let mut pessoas = Vec::new();
    let casa = casa_com(|db| {
        let permissions = seele_server::permissions::Permissions::new(db);
        for n in 0..TOTAL {
            let chave = SigningKey::from_bytes(&[100 + n as u8; 32]);
            let pessoa = permissions
                .register_or_find(&chave.verifying_key().to_bytes(), &format!("p{n}"))
                .unwrap()
                .id;
            let perfil = seele_proto::perfil::validar(&seele_proto::perfil::EntradaDePerfil {
                nome_exibido: &format!("Nome {n}"),
                pronomes: "",
                status: "",
                bio: "",
                cor: "#f2521f",
            })
            .unwrap();
            seele_server::persistence::perfis::gravar(db, pessoa, 0, &perfil, Efeito::Nenhum)
                .unwrap()
                .unwrap();
            pessoas.push(pessoa);
        }
    })
    .await?;

    let mut entrante = abrir(casa.endereco, 1, 12).await?;
    let mut partes = Vec::new();
    while let Some((tamanho, ultima)) = entrante
        .esperar(|m| match m {
            ServerMessage::Perfis { parte, ultima } => {
                Some((parte.iter().map(|p| p.pessoa).collect::<Vec<_>>(), *ultima))
            }
            _ => None,
        })
        .await
    {
        let fim = ultima;
        partes.push(tamanho);
        if fim {
            break;
        }
    }
    assert_eq!(
        partes.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![MAX_PERFIS_POR_PARTE, 2],
        "o corte em partes mudou"
    );
    let mut recebidas: Vec<PersonId> = partes.into_iter().flatten().collect();
    recebidas.sort();
    pessoas.sort();
    assert_eq!(recebidas, pessoas);
    Ok(())
}

// ---------------------------------------------------------------------------
// Imagens
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dez_mib_mais_um_e_recusado() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;

    // Declarado acima do teto: o pedido nem decodifica, e o servidor não lê
    // byte nenhum, mas **responde** a recusa (um fluxo que acaba mudo deixa
    // quem enviou sem saber por quê). O nosso `frame::write` valida antes de
    // mandar, então o pedido vai montado à mão, como mandaria um cliente que
    // não valida: o quadro de `TETO` com o varint trocado por `TETO + 1`.
    let resposta_crua = {
        let valido = seele_proto::control::encode(&PedidoDeImagem::EnviarAoPerfil {
            lado: LadoDoPerfil::Foto,
            bytes: TETO_DA_IMAGEM as u64,
        })?;
        // 10 MiB = 0xA0_0000, em LEB128: 80 80 80 05. Mais um: 81 80 80 05.
        let pos = valido
            .windows(4)
            .position(|j| j == [0x80, 0x80, 0x80, 0x05])
            .expect("o varint do teto no quadro");
        let mut acima = valido.clone();
        acima[pos] = 0x81;
        let (mut envia, mut recebe) = a.conexao.open_bi().await?;
        envia.write_all(&seele_proto::quadro::enquadrar(&acima)?).await?;
        let _ = envia.finish();
        tokio::time::timeout(PRAZO, frame::read::<RespostaDeImagem>(&mut recebe)).await?
    };
    assert!(
        resposta_crua.as_ref().is_ok_and(|r| r.erro.is_some()),
        "um tamanho declarado acima do teto foi aceito, ou o fluxo acabou sem resposta: {resposta_crua:?}"
    );

    // Declarado no teto, mas o fluxo continua depois dele: a leitura para.
    let resposta = a
        .enviar_imagem(
            LadoDoPerfil::Foto,
            TETO_DA_IMAGEM as u64,
            png(TETO_DA_IMAGEM + 1, 2),
        )
        .await?;
    assert_eq!(resposta.erro.as_deref(), Some("grande-demais"));
    assert_eq!(
        a.recusa().await.map(|(m, _)| m),
        Some(RecusaDePerfil::GrandeDemais)
    );
    assert!(
        casa.perfil(a.pessoa).await.is_none(),
        "uma linha nasceu da recusa"
    );
    assert_eq!(casa.arquivos(), Vec::<String>::new());

    // E o teto em si passa: 10 MiB exatos.
    let inteiro = png(TETO_DA_IMAGEM, 3);
    let resposta = a
        .enviar_imagem(LadoDoPerfil::Capa, TETO_DA_IMAGEM as u64, inteiro.clone())
        .await?;
    assert_eq!(resposta.erro, None);
    let perfil = a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");
    assert_eq!(perfil.capa.unwrap().bytes as usize, TETO_DA_IMAGEM);
    assert_eq!(casa.arquivos(), vec![hex(&sha(&inteiro))]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn heic_volta_com_heic_e_outros_bytes_com_tipo_recusado() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let mut b = abrir(casa.endereco, 2, 12).await?;

    let mut heic = b"\0\0\0\x18ftypheic".to_vec();
    heic.resize(4096, 0);
    let resposta = a
        .enviar_imagem(LadoDoPerfil::Foto, heic.len() as u64, heic)
        .await?;
    assert_eq!(resposta.erro.as_deref(), Some("heic"));
    assert_eq!(a.recusa().await.map(|(m, _)| m), Some(RecusaDePerfil::Heic));

    let resposta = a
        .enviar_imagem(LadoDoPerfil::Foto, 4096, vec![0x41; 4096])
        .await?;
    assert_eq!(resposta.erro.as_deref(), Some("tipo-recusado"));
    assert_eq!(
        a.recusa().await.map(|(m, _)| m),
        Some(RecusaDePerfil::TipoRecusado)
    );

    assert!(casa.perfil(a.pessoa).await.is_none());
    assert_eq!(casa.arquivos(), Vec::<String>::new());
    assert_eq!(arquivos_de_obras(&casa.imagens()), 0);
    // A recusa é de quem enviou.
    let vistas = b.tudo_ate_o_pong().await?;
    assert!(!vistas
        .iter()
        .any(|m| matches!(m, ServerMessage::PerfilRecusado { .. })));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn envio_truncado_volta_incompleto_sem_deixar_nada() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;

    // Anunciou 100 000 bytes e mandou 50 000 antes de fechar o fluxo.
    let resposta = a
        .enviar_imagem(LadoDoPerfil::Foto, 100_000, png(50_000, 4))
        .await?;
    assert_eq!(resposta.erro.as_deref(), Some("incompleto"));
    assert_eq!(
        a.recusa().await.map(|(m, _)| m),
        Some(RecusaDePerfil::Incompleto)
    );
    assert!(casa.perfil(a.pessoa).await.is_none());
    assert_eq!(casa.arquivos(), Vec::<String>::new());
    assert_eq!(
        arquivos_de_obras(&casa.imagens()),
        0,
        "sobrou arquivo de obras"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sem_pasta_o_envio_volta_sem_pasta_de_perfis() -> Result<()> {
    let config = ServerConfig {
        name: "Memória".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let aceitando = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    let mut a = abrir(endereco, 1, 12).await?;
    let resposta = a
        .enviar_imagem(LadoDoPerfil::Foto, 1024, png(1024, 5))
        .await?;
    assert_eq!(resposta.erro.as_deref(), Some("sem-pasta-de-perfis"));
    assert_eq!(
        a.recusa().await.map(|(m, _)| m),
        Some(RecusaDePerfil::SemPastaDePerfis)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trocar_a_imagem_apaga_a_anterior_depois_do_commit() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let primeira = png(5_000, 10);
    let segunda = png(6_000, 11);

    let r = a
        .enviar_imagem(LadoDoPerfil::Foto, primeira.len() as u64, primeira.clone())
        .await?;
    assert_eq!(r.erro, None);
    let depois_da_primeira = a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");
    let foto = depois_da_primeira.foto.expect("sem foto");
    assert_eq!(foto.hash, sha(&primeira));
    assert_eq!(foto.tipo, TipoDaImagem::Png);
    assert_eq!(casa.arquivos(), vec![hex(&sha(&primeira))]);
    assert_eq!(
        std::fs::read(casa.imagens().join(hex(&sha(&primeira))))?,
        primeira
    );

    let r = a
        .enviar_imagem(LadoDoPerfil::Foto, segunda.len() as u64, segunda.clone())
        .await?;
    assert_eq!(r.erro, None);
    let depois_da_segunda = a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");
    assert_eq!(depois_da_segunda.revisao, depois_da_primeira.revisao + 1);
    assert_eq!(
        casa.arquivos(),
        vec![hex(&sha(&segunda))],
        "a imagem antiga ficou no disco depois da troca"
    );

    // Tirar também apaga, e sobe a revisão.
    frame::write(
        &mut a.envio,
        &ClientMessage::TirarImagemDoPerfil {
            lado: LadoDoPerfil::Foto,
        },
    )
    .await?;
    let tirada = a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");
    assert!(tirada.foto.is_none());
    assert_eq!(tirada.revisao, depois_da_segunda.revisao + 1);
    assert_eq!(casa.arquivos(), Vec::<String>::new());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_imagem_antiga_fica_enquanto_outra_pessoa_a_usa() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let mut b = abrir(casa.endereco, 2, 12).await?;
    let dividida = png(5_000, 20);
    let so_da_a = png(5_500, 21);
    let so_da_b = png(5_600, 22);

    // A foto de A e a capa de B são os mesmos bytes: um arquivo só.
    assert_eq!(
        a.enviar_imagem(LadoDoPerfil::Foto, dividida.len() as u64, dividida.clone())
            .await?
            .erro,
        None
    );
    assert_eq!(
        b.enviar_imagem(LadoDoPerfil::Capa, dividida.len() as u64, dividida.clone())
            .await?
            .erro,
        None
    );
    a.perfil_mudou(a.pessoa).await.expect("PerfilMudou de A");
    b.perfil_mudou(b.pessoa).await.expect("PerfilMudou de B");
    assert_eq!(casa.arquivos(), vec![hex(&sha(&dividida))]);

    // A troca a dela: o arquivo fica, porque B ainda o aponta.
    assert_eq!(
        a.enviar_imagem(LadoDoPerfil::Foto, so_da_a.len() as u64, so_da_a.clone())
            .await?
            .erro,
        None
    );
    a.perfil_mudou(a.pessoa).await.expect("PerfilMudou de A");
    let mut esperado = vec![hex(&sha(&dividida)), hex(&sha(&so_da_a))];
    esperado.sort();
    assert_eq!(casa.arquivos(), esperado, "o arquivo de B foi apagado");
    let capa_de_b = casa.perfil(b.pessoa).await.unwrap().capa.unwrap();
    assert_eq!(
        b.ler_imagem(b.pessoa, LadoDoPerfil::Capa, capa_de_b.hash)
            .await?,
        Ok(dividida.clone()),
        "a capa de B não abre mais"
    );

    // B troca a dele: agora ninguém aponta o arquivo, e ele sai.
    assert_eq!(
        b.enviar_imagem(LadoDoPerfil::Capa, so_da_b.len() as u64, so_da_b.clone())
            .await?
            .erro,
        None
    );
    b.perfil_mudou(b.pessoa).await.expect("PerfilMudou de B");
    let mut esperado = vec![hex(&sha(&so_da_a)), hex(&sha(&so_da_b))];
    esperado.sort();
    assert_eq!(casa.arquivos(), esperado);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ler_do_perfil_entrega_os_bytes_e_um_hash_velho_volta_mudou() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let b = abrir(casa.endereco, 2, 12).await?;
    let primeira = png(7_000, 30);
    let segunda = png(7_100, 31);

    a.enviar_imagem(LadoDoPerfil::Foto, primeira.len() as u64, primeira.clone())
        .await?;
    a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");
    // Ler é livre para quem está conectado: B lê a foto de A.
    assert_eq!(
        b.ler_imagem(a.pessoa, LadoDoPerfil::Foto, sha(&primeira))
            .await?,
        Ok(primeira.clone())
    );
    // O lado vazio e a pessoa sem perfil também voltam «mudou».
    assert_eq!(
        b.ler_imagem(a.pessoa, LadoDoPerfil::Capa, sha(&primeira))
            .await?,
        Err("mudou".to_owned())
    );

    a.enviar_imagem(LadoDoPerfil::Foto, segunda.len() as u64, segunda.clone())
        .await?;
    a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");
    assert_eq!(
        b.ler_imagem(a.pessoa, LadoDoPerfil::Foto, sha(&primeira))
            .await?,
        Err("mudou".to_owned()),
        "um hash que já não é o atual devolveu bytes"
    );
    assert_eq!(
        b.ler_imagem(a.pessoa, LadoDoPerfil::Foto, sha(&segunda))
            .await?,
        Ok(segunda)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_varredura_tira_so_o_que_nenhum_perfil_aponta() -> Result<()> {
    let casa = casa().await?;
    let mut a = abrir(casa.endereco, 1, 12).await?;
    let usada = png(3_000, 40);
    a.enviar_imagem(LadoDoPerfil::Foto, usada.len() as u64, usada.clone())
        .await?;
    a.perfil_mudou(a.pessoa).await.expect("PerfilMudou");

    let solta = hex(&sha(&png(3_000, 41)));
    std::fs::write(casa.imagens().join(&solta), png(3_000, 41))?;
    std::fs::write(casa.imagens().join(".em-obras-1-1"), b"meia")?;
    std::fs::write(casa.imagens().join("README"), b"nao e meu")?;

    let removidos = {
        let db = casa.daemon.server().persistence.lock().await;
        seele_server::perfis::imagens::varrer_orfaos(&db)?
    };
    assert_eq!(removidos, 2);
    let mut esperado = vec![hex(&sha(&usada)), "README".to_owned()];
    esperado.sort();
    assert_eq!(casa.arquivos(), esperado);
    Ok(())
}
