//! A migração dos perfis do MOD PERFIS, com dados reais (`docs/0.19/perfis-oficiais.md`, §7).
//!
//! # A fixture, e de onde ela vem
//!
//! `tests/fixtures/perfis-5.0.0/` é o que um servidor 0.18 com o PERFIS deixa
//! no disco, gerado pelo código **de verdade** do MOD, e não escrito à mão:
//!
//! - `quintal.json`: o `mod_data` de `seele/perfis`, chave a chave;
//! - `dados/`: a pasta mutável do MOD (`RaizesDosMods::dados_de`), com as
//!   imagens nos três formatos — o volume (`volume/images/…`, PERFIS 5.0.0),
//!   os fragmentos (`uploads/2-….parts` e `.parts.<n>`, PERFIS 1.2.0) e o
//!   data-URL num arquivo só (`uploads/3-….txt`, PERFIS 1.0.0);
//! - `entrada/`: os bytes que cada pessoa escolheu. É contra eles que o SHA
//!   das imagens migradas é conferido.
//!
//! A história que a gerou é a de um servidor que atualizou o MOD: a pessoa 3
//! enviou a capa no PERFIS 1.0.0, a pessoa 2 no 1.2.0 (os dois pelo
//! `Anfitriao`, o motor de MODs do servidor), e as três salvaram o perfil e
//! enviaram a foto no 5.0.0, pelo servidor inteiro: `ModRequest` (`save`,
//! `upload-start`), o fluxo de volume (`PedidoDeImagem::Enviar`) e
//! `upload-finish`.
//!
//! **Para regerar** (com o `SEELE-MOD-PERFIS` ao lado deste repositório, no
//! commit `11fb8d8`, o 5.0.0; ou `SEELE_MOD_PERFIS=<pasta>`):
//!
//! ```sh
//! cargo test -p seele-server --test migracao_de_perfis -- --ignored gerar_a_fixture
//! ```
//!
//! Os tokens das imagens mudam a cada geração (relógio e sorteio do MOD), e as
//! asserções não dependem deles. As imagens de `entrada/` foram feitas uma vez
//! com o Pillow e não se regeram.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_proto::ids::PersonId;
use seele_proto::perfil::{Efeito, ImagemDoPerfil, LadoDoPerfil, PerfilPublico, TipoDaImagem};
use seele_server::persistence::mods::EnabledMod;
use seele_server::persistence::perfis::{self, EstadoDaImagem, PendenciaDeImagem};
use seele_server::persistence::{Location, Persistence};
use seele_server::{Daemon, RaizesDosMods, ServerConfig};

mod comum;

const PERFIS: &str = "seele/perfis";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/perfis-5.0.0")
}

fn quintal_da_fixture() -> BTreeMap<String, String> {
    serde_json::from_str(&std::fs::read_to_string(fixture().join("quintal.json")).unwrap()).unwrap()
}

/// Copia uma árvore inteira (só arquivos e pastas).
fn copiar_arvore(de: &Path, para: &Path) {
    std::fs::create_dir_all(para).unwrap();
    for entrada in std::fs::read_dir(de).unwrap() {
        let entrada = entrada.unwrap();
        let alvo = para.join(entrada.file_name());
        if entrada.file_type().unwrap().is_dir() {
            copiar_arvore(&entrada.path(), &alvo);
        } else {
            std::fs::copy(entrada.path(), alvo).unwrap();
        }
    }
}

/// Todos os arquivos de uma árvore, pelo caminho relativo, com os bytes.
fn arvore(raiz: &Path) -> BTreeMap<String, Vec<u8>> {
    fn andar(raiz: &Path, aqui: &Path, saida: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entradas) = std::fs::read_dir(aqui) else {
            return;
        };
        for entrada in entradas {
            let entrada = entrada.unwrap();
            if entrada.file_type().unwrap().is_dir() {
                andar(raiz, &entrada.path(), saida);
            } else {
                let relativo = entrada.path().strip_prefix(raiz).unwrap().to_owned();
                saida.insert(
                    relativo.to_string_lossy().replace('\\', "/"),
                    std::fs::read(entrada.path()).unwrap(),
                );
            }
        }
    }
    let mut saida = BTreeMap::new();
    andar(raiz, raiz, &mut saida);
    saida
}

/// A árvore pelo SHA de cada arquivo: o que as asserções comparam, legível
/// quando difere.
fn impressoes(raiz: &Path) -> BTreeMap<String, String> {
    arvore(raiz)
        .into_iter()
        .map(|(caminho, bytes)| (caminho, seele_proto::attachment::hex(&sha(&bytes))))
        .collect()
}

fn sha(bytes: &[u8]) -> [u8; 32] {
    seele_proto::attachment::hash(bytes)
}

fn entrada(nome: &str) -> Vec<u8> {
    std::fs::read(fixture().join("entrada").join(nome)).unwrap()
}

/// Um servidor 0.18 parado, com o PERFIS ligado e a fixture no lugar.
struct Cenario {
    _temp: tempfile::TempDir,
    banco: PathBuf,
    raizes: RaizesDosMods,
}

impl Cenario {
    /// `api`/`versao`: o PERFIS que estava ligado.
    fn novo(api: u32, versao: &str) -> Self {
        Self::com_quintal(api, versao, quintal_da_fixture())
    }

    fn com_quintal(api: u32, versao: &str, quintal: BTreeMap<String, String>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let raizes = RaizesDosMods {
            pacotes: temp.path().join("cache"),
            dados: temp.path().join("dados"),
        };
        copiar_arvore(&fixture().join("dados"), &raizes.dados_de(PERFIS));
        let banco = temp.path().join("seele.db");
        {
            let mut persistence = Persistence::open(&Location::File(banco.clone())).unwrap();
            seele_server::persistence::mods::enable(
                &persistence,
                &EnabledMod {
                    id: PERFIS.into(),
                    version: versao.into(),
                    // O pacote não precisa estar no disco: o MOD sai desligado
                    // antes de alguém procurar por ele.
                    hash: "ab".repeat(32),
                    repo: "https://github.com/DATA-AND-DEV/PERFIS".into(),
                    reach: vec!["perfis no servidor".into()],
                    server_half: true,
                    api,
                    tema: false,
                    plataformas: None,
                },
            )
            .unwrap();
            // Depois da linha do MOD: o quintal tem chave estrangeira para ela.
            seele_server::persistence::mods::gravar_quintal(&mut persistence, PERFIS, &quintal)
                .unwrap();
        }
        // As três pessoas do servidor de onde a fixture veio, com os mesmos
        // identificadores. Direto no SQLite: criar uma pessoa é da sessão, e a
        // sessão aqui é o que menos importa.
        let sql = rusqlite::Connection::open(&banco).unwrap();
        for id in 1..=3_i64 {
            sql.execute(
                "INSERT INTO people (id, nickname, public_key, created_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![
                    id,
                    format!("pessoa{id:03}"),
                    vec![u8::try_from(id).unwrap(); 32]
                ],
            )
            .unwrap();
        }
        Self {
            _temp: temp,
            banco,
            raizes,
        }
    }

    fn config(&self) -> ServerConfig {
        ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::File(self.banco.clone()),
            mods_dir: Some(self.raizes.clone()),
            ..ServerConfig::default()
        }
    }

    fn pasta_das_imagens(&self) -> PathBuf {
        seele_server::diretorio_de_perfis(&self.config()).unwrap()
    }

    fn dados_do_mod(&self) -> PathBuf {
        self.raizes.dados_de(PERFIS)
    }

    /// Sobe o servidor (é na subida que a migração roda) e devolve o banco.
    async fn subir(&self) -> Result<Daemon> {
        Daemon::bind(self.config()).await
    }
}

/// O que a migração deixou, lido do banco do servidor que subiu.
struct Depois {
    perfis: Vec<Option<PerfilPublico>>,
    livro: Vec<Option<perfis::RegistroDaMigracao>>,
    pendencias: Vec<Vec<PendenciaDeImagem>>,
    relatorio: Option<perfis::RelatorioDaMigracaoDePerfis>,
    ligados: Vec<String>,
    quintal: BTreeMap<String, String>,
}

async fn ler(daemon: &Daemon) -> Depois {
    let banco = daemon.server().persistence.lock().await;
    let pessoas = [1, 2, 3].map(PersonId);
    Depois {
        perfis: pessoas
            .iter()
            .map(|p| perfis::ler(&banco, *p).unwrap())
            .collect(),
        livro: pessoas
            .iter()
            .map(|p| perfis::registro_da_migracao(&banco, *p).unwrap())
            .collect(),
        pendencias: pessoas
            .iter()
            .map(|p| perfis::pendencias(&banco, *p).unwrap())
            .collect(),
        relatorio: perfis::relatorio(&banco).unwrap(),
        ligados: seele_server::persistence::mods::enabled(&banco)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect(),
        quintal: seele_server::persistence::mods::ler_quintal(&banco, PERFIS).unwrap(),
    }
}

fn contar_pessoas(banco: &Path) -> i64 {
    rusqlite::Connection::open(banco)
        .unwrap()
        .query_row("SELECT count(*) FROM people", [], |l| l.get(0))
        .unwrap()
}

fn imagem(nome: &str, tipo: TipoDaImagem) -> ImagemDoPerfil {
    let bytes = entrada(nome);
    ImagemDoPerfil {
        hash: sha(&bytes),
        tipo,
        bytes: u32::try_from(bytes.len()).unwrap(),
    }
}

/// O caminho de um arquivo da fixture que casa com o fim pedido.
fn na_fixture(fim: &str) -> String {
    arvore(&fixture().join("dados"))
        .into_keys()
        .find(|caminho| caminho.ends_with(fim))
        .unwrap_or_else(|| panic!("a fixture não tem um arquivo terminado em {fim}"))
}

/// **Todos os campos, os três formatos, os bytes pelo SHA, o MOD desligado e a
/// fixture intacta** — e a segunda subida não muda nada.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn os_perfis_reais_do_perfis_viram_perfis_do_produto() -> Result<()> {
    let cenario = Cenario::novo(6, "4.0.0");
    let quintal_antes = quintal_da_fixture();
    let dados_antes = impressoes(&cenario.dados_do_mod());
    let daemon = cenario.subir().await?;
    let depois = ler(&daemon).await;

    // Cada campo, um por um, contra o que a fixture gravou pelo MOD real.
    let esperado = [
        PerfilPublico {
            pessoa: PersonId(1),
            revisao: 3,
            nome_exibido: "Maya Ribeiro 🌙".into(),
            pronomes: "ela/dela".into(),
            status: "ao vivo às 20h".into(),
            bio: "Faço trilhas sonoras.\nE café.".into(),
            cor: [0x3a, 0x7b, 0xd5],
            efeito: Efeito::Aurora,
            foto: Some(imagem("foto-1.png", TipoDaImagem::Png)),
            capa: Some(imagem("capa-1.jpg", TipoDaImagem::Jpeg)),
        },
        PerfilPublico {
            pessoa: PersonId(2),
            revisao: 3,
            nome_exibido: "Rafa".into(),
            pronomes: "ele/dele".into(),
            status: String::new(),
            bio: "Atualizei do 1.2.".into(),
            cor: [0x12, 0xab, 0x34],
            efeito: Efeito::Brilho,
            foto: Some(imagem("foto-2.webp", TipoDaImagem::Webp)),
            capa: Some(imagem("capa-2.png", TipoDaImagem::Png)),
        },
        PerfilPublico {
            pessoa: PersonId(3),
            revisao: 3,
            nome_exibido: "Lu".into(),
            pronomes: String::new(),
            status: "desde o 1.0".into(),
            bio: String::new(),
            cor: [0xf2, 0x52, 0x1f],
            efeito: Efeito::Pulso,
            foto: Some(imagem("foto-3.gif", TipoDaImagem::Gif)),
            capa: Some(imagem("capa-3.jpg", TipoDaImagem::Jpeg)),
        },
    ];
    for (perfil, esperado) in depois.perfis.iter().zip(&esperado) {
        assert_eq!(perfil.as_ref(), Some(esperado));
    }

    // Os bytes em `perfis/<sha>` são os que a pessoa escolheu.
    let pasta = cenario.pasta_das_imagens();
    for nome in [
        "foto-1.png",
        "capa-1.jpg",
        "foto-2.webp",
        "capa-2.png",
        "foto-3.gif",
        "capa-3.jpg",
    ] {
        let bytes = entrada(nome);
        let gravado = std::fs::read(pasta.join(seele_proto::attachment::hex(&sha(&bytes))))
            .unwrap_or_else(|_| panic!("{nome} não está em perfis/"));
        assert_eq!(sha(&gravado), sha(&bytes), "{nome}");
    }

    // O livro, o relatório, nenhuma pendência.
    for registro in &depois.livro {
        let registro = registro.as_ref().expect("a pessoa não entrou no livro");
        assert_eq!(registro.foto, EstadoDaImagem::Trazida);
        assert_eq!(registro.capa, EstadoDaImagem::Trazida);
    }
    assert!(depois.pendencias.iter().all(Vec::is_empty));
    let relatorio = depois
        .relatorio
        .clone()
        .expect("sem relatório para quem hospeda");
    assert_eq!(relatorio.trazidos, 3);
    assert!(!relatorio.incompleta);
    assert!(
        relatorio.ocorrencias.is_empty(),
        "{:?}",
        relatorio.ocorrencias
    );

    // O MOD sai desligado.
    assert!(
        !depois.ligados.contains(&PERFIS.to_owned()),
        "o PERFIS continuou ligado depois da migração"
    );

    // **Copiar, nunca mover**: o quintal e a pasta do MOD como estavam.
    assert_eq!(depois.quintal, quintal_antes, "o quintal do MOD mudou");
    assert_eq!(
        impressoes(&cenario.dados_do_mod()),
        dados_antes,
        "a pasta do MOD mudou: a migração moveu ou apagou"
    );

    // **A segunda subida não muda nada**: nem os perfis, nem o livro, nem o
    // relatório (o mesmo `feita_em`), nem a pasta das imagens.
    let imagens_antes = impressoes(&pasta);
    drop(daemon);
    let daemon = cenario.subir().await?;
    let de_novo = ler(&daemon).await;
    assert_eq!(de_novo.perfis, depois.perfis);
    assert_eq!(de_novo.livro, depois.livro);
    assert_eq!(de_novo.relatorio, depois.relatorio);
    assert_eq!(impressoes(&pasta), imagens_antes);
    assert_eq!(impressoes(&cenario.dados_do_mod()), dados_antes);
    assert_eq!(contar_pessoas(&cenario.banco), 3);
    Ok(())
}

/// **Um fragmento apagado**: a capa fica `falhou:fragmento-faltando`, dita no
/// relatório e na pendência da pessoa, e o texto dela vem assim mesmo. Quando o
/// fragmento volta, a subida seguinte traz a capa.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn um_fragmento_apagado_e_dito_e_o_texto_vem_assim_mesmo() -> Result<()> {
    let cenario = Cenario::novo(6, "4.0.0");
    let fragmento = cenario.dados_do_mod().join(na_fixture(".parts.1"));
    let guardado = std::fs::read(&fragmento)?;
    std::fs::remove_file(&fragmento)?;

    let daemon = cenario.subir().await?;
    let depois = ler(&daemon).await;
    let rafa = depois.perfis[1]
        .clone()
        .expect("o texto da pessoa 2 não veio");
    assert_eq!(rafa.nome_exibido, "Rafa");
    assert_eq!(rafa.bio, "Atualizei do 1.2.");
    assert_eq!(rafa.foto, Some(imagem("foto-2.webp", TipoDaImagem::Webp)));
    assert_eq!(rafa.capa, None);
    assert_eq!(
        depois.livro[1].as_ref().unwrap().capa,
        EstadoDaImagem::Falhou("fragmento-faltando".into())
    );
    assert_eq!(
        depois.pendencias[1],
        [PendenciaDeImagem {
            lado: LadoDoPerfil::Capa,
            motivo: "fragmento-faltando".into()
        }]
    );
    let relatorio = depois.relatorio.expect("a falha não foi relatada");
    assert_eq!(relatorio.trazidos, 3);
    assert!(
        relatorio
            .ocorrencias
            .iter()
            .any(|o| o.pessoa == Some(2) && o.assunto == "capa: fragmento-faltando"),
        "{:?}",
        relatorio.ocorrencias
    );
    assert!(!depois.ligados.contains(&PERFIS.to_owned()));
    drop(daemon);

    // O fragmento aparece: a exceção da idempotência traz a capa.
    std::fs::write(&fragmento, guardado)?;
    let daemon = cenario.subir().await?;
    let depois = ler(&daemon).await;
    let rafa = depois.perfis[1].clone().unwrap();
    assert_eq!(rafa.capa, Some(imagem("capa-2.png", TipoDaImagem::Png)));
    assert!(rafa.revisao > 3, "trazer a capa não subiu a revisão");
    assert_eq!(
        depois.livro[1].as_ref().unwrap().capa,
        EstadoDaImagem::Trazida
    );
    Ok(())
}

/// **O PERFIS da API 6 ligado não impede a subida**, e um de uma API que esta
/// linha não roda (o 3.x, da API 5) também não: é este que o portão
/// `ligados_de_outra_api` recusaria se a migração viesse depois dele, ou se
/// ela não desligasse o MOD.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn um_perfis_de_outra_api_ligado_nao_impede_a_subida() -> Result<()> {
    for (api, versao) in [(5, "3.3.1"), (6, "4.0.0")] {
        let cenario = Cenario::novo(api, versao);
        let daemon = cenario.subir().await.unwrap_or_else(|erro| {
            panic!("o PERFIS {versao} (API {api}) impediu a subida: {erro:#}")
        });
        let depois = ler(&daemon).await;
        assert!(depois.perfis.iter().all(Option::is_some), "API {api}");
        assert!(!depois.ligados.contains(&PERFIS.to_owned()), "API {api}");
    }
    Ok(())
}

/// A pasta de quem hospedou na 0.15.1 com o PERFIS 3.3.1 (API 5), **escrita
/// pelo código da v0.15.1 rodando a metade de servidor de verdade do 3.3.1**
/// (`SEELE-MOD-PERFIS@0774774`; ver o `LEIA.md` dela). Mora no app, onde o
/// teste da travessia a usa; aqui ela é só lida, como `acoes_de_mod.rs` lê a
/// fixture de lá.
fn pasta_da_0_15_1() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/seele-app/testes/pasta-da-0.15.1-perfis/pasta")
}

/// O caminho relativo, em `dados`, do arquivo que termina em `fim`.
fn na_pasta(dados: &Path, fim: &str) -> String {
    arvore(dados)
        .into_keys()
        .find(|caminho| caminho.ends_with(fim))
        .unwrap_or_else(|| panic!("a pasta não tem um arquivo terminado em {fim}"))
}

/// **O PERFIS 3.3.1 da API 5, como a 0.15.1 o deixou, vira perfil do produto.**
///
/// A F3 provou a migração com o PERFIS 1.0, 1.2 e 5.0; esta é a linha que a
/// 0.15 rodava. O banco é o da 0.15.1 (esquema 13, convertido nesta subida), o
/// quintal é o que o `save` do 3.3.1 gravou (`profiles`), a foto veio pelo
/// volume (o transporte do 3.2 em diante) e a capa pelos fragmentos
/// (`.parts`, dois pedaços). Medido em 31/10: o 3.3.1 guarda tudo na mesma
/// forma que a migração já lê (a chave `profiles` é a mesma do 1.0 ao 5.0), e
/// nada novo precisou ser lido.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn o_perfis_3_3_1_da_0_15_1_vira_perfil_do_produto() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let banco = temp.path().join("seele.db");
    rusqlite::Connection::open(&banco)?.execute_batch(&std::fs::read_to_string(
        pasta_da_0_15_1().join("seele.sql"),
    )?)?;
    let raizes = RaizesDosMods {
        pacotes: temp.path().to_path_buf(),
        dados: temp.path().join("mod-data-legado"),
    };
    copiar_arvore(&pasta_da_0_15_1().join("mod-data-legado"), &raizes.dados);
    let dados = raizes.dados_de(PERFIS);
    let dados_antes = impressoes(&dados);
    let quintal_antes: Vec<(String, Vec<u8>)> = {
        let sql = rusqlite::Connection::open(&banco)?;
        let mut consulta = sql.prepare(
            "SELECT key, CAST(value AS BLOB) FROM mod_data WHERE mod_id = ?1 ORDER BY key",
        )?;
        let linhas = consulta
            .query_map([PERFIS], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        linhas
    };
    assert_eq!(
        quintal_antes
            .iter()
            .map(|(chave, _)| chave.as_str())
            .collect::<Vec<_>>(),
        ["profiles"],
        "a pasta da 0.15.1 não tem o quintal que o 3.3.1 grava"
    );
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco.clone()),
        mods_dir: Some(raizes.clone()),
        ..ServerConfig::default()
    };
    let pasta = seele_server::diretorio_de_perfis(&config).unwrap();

    let daemon = Daemon::bind(config.clone()).await?;
    let (dona, convidada, livro, relatorio, ligados, quintal) = {
        let b = daemon.server().persistence.lock().await;
        (
            perfis::ler(&b, PersonId(1))?,
            perfis::ler(&b, PersonId(2))?,
            [1, 2].map(|p| perfis::registro_da_migracao(&b, PersonId(p)).unwrap()),
            perfis::relatorio(&b)?,
            seele_server::persistence::mods::enabled(&b)?
                .into_iter()
                .map(|m| m.id)
                .collect::<Vec<_>>(),
            seele_server::persistence::mods::ler_quintal(&b, PERFIS)?,
        )
    };

    // O texto, campo a campo, como o `save` do 3.3.1 o gravou.
    let dona = dona.expect("o perfil da dona não veio");
    assert_eq!(
        (
            dona.revisao,
            dona.nome_exibido.as_str(),
            dona.pronomes.as_str(),
            dona.status.as_str(),
            dona.bio.as_str(),
            dona.cor,
            dona.efeito,
        ),
        (
            3,
            "Dona da Casa",
            "ela/dela",
            "no ar",
            "hospedo desde a 0.15",
            [0x33, 0x66, 0xcc],
            Efeito::Aurora
        )
    );
    let convidada = convidada.expect("o perfil da convidada não veio");
    assert_eq!(
        (
            convidada.revisao,
            convidada.nome_exibido.as_str(),
            convidada.status.as_str(),
            convidada.foto,
            convidada.capa,
        ),
        (1, "Convidada", "de passagem", None, None)
    );

    // A foto, pelo volume: os bytes que o fluxo deixou, pelo SHA.
    let do_volume = arvore(&dados)
        .into_iter()
        .find(|(caminho, _)| caminho.starts_with("volume/images/1-"))
        .map(|(_, bytes)| bytes)
        .expect("a pasta da 0.15.1 não tem a foto no volume");
    let foto = dona.foto.expect("a foto (volume) não veio");
    assert_eq!(foto.hash, sha(&do_volume));
    assert_eq!(foto.tipo, TipoDaImagem::Png);
    assert_eq!(
        std::fs::read(pasta.join(seele_proto::attachment::hex(&foto.hash)))?,
        do_volume
    );
    // A capa, pelos fragmentos: os bytes gravados em `perfis/`, em data-URL,
    // são os dois pedaços juntos — conferido sem passar pelo leitor da
    // migração.
    let capa = dona.capa.expect("a capa (fragmentos) não veio");
    assert_eq!(capa.tipo, TipoDaImagem::Png);
    let gravada = std::fs::read(pasta.join(seele_proto::attachment::hex(&capa.hash)))?;
    let todos = arvore(&dados);
    let pedacos: String = [".parts.0", ".parts.1"]
        .iter()
        .map(|fim| String::from_utf8(todos[&na_pasta(&dados, fim)].clone()).unwrap())
        .collect();
    assert_eq!(
        format!("data:image/png;base64,{}", base64(&gravada)),
        pedacos
    );

    // O livro, e o relatório que quem hospeda lê.
    let [livro_dona, livro_convidada] = livro;
    let livro_dona = livro_dona.expect("a dona não entrou no livro");
    assert_eq!(
        (livro_dona.foto, livro_dona.capa),
        (EstadoDaImagem::Trazida, EstadoDaImagem::Trazida)
    );
    let livro_convidada = livro_convidada.expect("a convidada não entrou no livro");
    assert_eq!(
        (livro_convidada.foto, livro_convidada.capa),
        (EstadoDaImagem::Nenhuma, EstadoDaImagem::Nenhuma)
    );
    let relatorio = relatorio.expect("sem relatório para quem hospeda");
    assert_eq!(relatorio.trazidos, 2);
    assert!(!relatorio.incompleta);
    assert!(
        relatorio.ocorrencias.is_empty(),
        "{:?}",
        relatorio.ocorrencias
    );

    // O 3.3.1 sai desligado; o quintal e a pasta dele ficam como estavam.
    assert!(
        !ligados.contains(&PERFIS.to_owned()),
        "o 3.3.1 continuou ligado"
    );
    let (_, bruto) = quintal_antes.first().unwrap();
    assert_eq!(
        quintal.get("profiles").map(String::as_bytes),
        Some(bruto.as_slice()),
        "o quintal do 3.3.1 mudou"
    );
    assert_eq!(impressoes(&dados), dados_antes, "a pasta do 3.3.1 mudou");

    // Idempotente: a segunda subida não muda perfil, livro nem relatório.
    let imagens = impressoes(&pasta);
    drop(daemon);
    let daemon = Daemon::bind(config).await?;
    {
        let b = daemon.server().persistence.lock().await;
        assert_eq!(perfis::ler(&b, PersonId(1))?.map(|p| p.revisao), Some(3));
        assert_eq!(perfis::relatorio(&b)?, Some(relatorio));
    }
    assert_eq!(impressoes(&pasta), imagens);
    Ok(())
}

/// **O que não cabe vai para o relatório**: pessoa que não existe (e não é
/// criada), campo inválido (vira `""`), cor inválida (volta ao padrão), efeito
/// desconhecido (vira nenhum). E um perfil já editado no produto, com revisão
/// maior, não é sobrescrito.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn o_que_nao_cabe_vai_para_o_relatorio_e_o_mais_novo_fica() -> Result<()> {
    let mut quintal = quintal_da_fixture();
    let mut perfis_do_mod: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&quintal["profiles"])?;
    perfis_do_mod.insert(
        "99".into(),
        serde_json::json!({"revision":1,"displayName":"Fantasma","pronouns":"","bio":"","status":"","accent":"#000000","effect":"none","avatar":null,"banner":null}),
    );
    let lu = perfis_do_mod.get_mut("3").unwrap().as_object_mut().unwrap();
    lu.insert("accent".into(), "#zzz".into());
    lu.insert("effect".into(), "glitter".into());
    lu.insert("pronouns".into(), "a\u{202e}b".into());
    quintal.insert("profiles".into(), serde_json::to_string(&perfis_do_mod)?);
    let cenario = Cenario::com_quintal(6, "4.0.0", quintal);

    // A pessoa 1 já editou o perfil no produto, e passou da revisão do MOD.
    {
        let banco = Persistence::open(&Location::File(cenario.banco.clone()))?;
        let validado = seele_proto::perfil::validar(&seele_proto::perfil::EntradaDePerfil {
            nome_exibido: "Maya nova",
            pronomes: "",
            status: "",
            bio: "",
            cor: "#000000",
        })
        .unwrap();
        let mut revisao = 0;
        while revisao <= 3 {
            revisao =
                perfis::gravar(&banco, PersonId(1), revisao, &validado, Efeito::Nenhum)?.unwrap();
        }
    }

    let daemon = cenario.subir().await?;
    let depois = ler(&daemon).await;
    assert_eq!(
        contar_pessoas(&cenario.banco),
        3,
        "a pessoa desconhecida foi criada"
    );
    let maya = depois.perfis[0].clone().unwrap();
    assert_eq!(
        maya.nome_exibido, "Maya nova",
        "o perfil mais novo foi sobrescrito"
    );
    let lu = depois.perfis[2].clone().unwrap();
    assert_eq!(lu.cor, [0xf2, 0x52, 0x1f]);
    assert_eq!(lu.efeito, Efeito::Nenhum);
    assert_eq!(lu.pronomes, "");
    assert_eq!(lu.status, "desde o 1.0");

    let assuntos: Vec<(Option<u64>, String)> = depois
        .relatorio
        .expect("sem relatório")
        .ocorrencias
        .into_iter()
        .map(|o| (o.pessoa, o.assunto))
        .collect();
    for (pessoa, comeco) in [
        (Some(99), "pessoa inexistente"),
        (Some(1), "o perfil já tinha sido editado"),
        (Some(3), "cor «#zzz» inválida"),
        (Some(3), "efeito «glitter» desconhecido"),
        (Some(3), "campo pronomes inválido"),
    ] {
        assert!(
            assuntos
                .iter()
                .any(|(p, a)| *p == pessoa && a.starts_with(comeco)),
            "faltou «{comeco}» para {pessoa:?} em {assuntos:?}"
        );
    }

    // A segunda subida não regrava o relatório só porque a pessoa 99 continua
    // sem existir: dispensado, ele fica dispensado.
    {
        let banco = daemon.server().persistence.lock().await;
        perfis::dispensar_relatorio(&banco)?;
    }
    drop(daemon);
    let daemon = cenario.subir().await?;
    assert_eq!(
        ler(&daemon).await.relatorio,
        None,
        "o relatório dispensado voltou sem novidade"
    );
    Ok(())
}

/// **O banco falha no meio**: o servidor sobe assim mesmo, com o MOD desligado
/// e o relatório «migração incompleta»; a subida seguinte termina o trabalho.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn um_banco_que_falha_no_meio_deixa_a_migracao_incompleta_e_a_proxima_termina() -> Result<()>
{
    let cenario = Cenario::novo(6, "4.0.0");
    let sql = rusqlite::Connection::open(&cenario.banco)?;
    sql.execute_batch(
        "CREATE TRIGGER falha_no_meio BEFORE INSERT ON perfis_migracao WHEN NEW.pessoa = 2
         BEGIN SELECT RAISE(ABORT, 'disco cheio'); END;",
    )?;

    let daemon = cenario.subir().await?;
    let depois = ler(&daemon).await;
    let relatorio = depois.relatorio.expect("a falha do banco não foi relatada");
    assert!(relatorio.incompleta);
    assert!(
        relatorio
            .ocorrencias
            .iter()
            .any(|o| o.pessoa.is_none() && o.assunto.starts_with("migração incompleta")),
        "{:?}",
        relatorio.ocorrencias
    );
    assert!(depois.perfis[0].is_some());
    assert!(depois.perfis[1].is_none(), "a transação da pessoa 2 vazou");
    assert!(!depois.ligados.contains(&PERFIS.to_owned()));
    drop(daemon);

    sql.execute_batch("DROP TRIGGER falha_no_meio;")?;
    let daemon = cenario.subir().await?;
    let depois = ler(&daemon).await;
    assert!(depois.perfis.iter().all(Option::is_some));
    let relatorio = depois.relatorio.unwrap();
    assert!(!relatorio.incompleta);
    assert_eq!(relatorio.trazidos, 2);
    Ok(())
}

// ---------------------------------------------------------------------------
// O gerador da fixture
// ---------------------------------------------------------------------------

/// Quanto se espera por um quadro que tem de vir.
const PRAZO: Duration = Duration::from_secs(10);

fn pasta_do_mod() -> PathBuf {
    std::env::var_os("SEELE_MOD_PERFIS").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../SEELE-MOD-PERFIS"),
        PathBuf::from,
    )
}

fn git_show(rev: &str, caminho: &str) -> String {
    let saida = std::process::Command::new("git")
        .arg("-C")
        .arg(pasta_do_mod())
        .arg("show")
        .arg(format!("{rev}:{caminho}"))
        .output()
        .expect("git");
    assert!(saida.status.success(), "git show {rev}:{caminho}");
    String::from_utf8(saida.stdout).unwrap()
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Uma versão antiga do MOD, pelo `Anfitriao` do servidor: salva o perfil e
/// envia a capa em fragmentos de 6000 caracteres, como o cliente dela fazia.
fn historia_antiga(
    rev: &str,
    dados: &Path,
    pessoa: u64,
    perfil: serde_json::Value,
    capa: &str,
    tipo: &str,
    quintal: &mut BTreeMap<String, String>,
) {
    let mut anfitriao = seele_server::mods::Anfitriao::novo().unwrap();
    anfitriao
        .carregar(PERFIS, &git_show(rev, "servidor/main.js"), dados)
        .unwrap();
    let contexto = serde_json::json!({"person":pessoa.to_string(),"write":true}).to_string();
    let mut pedir = |pedido: serde_json::Value| -> serde_json::Value {
        let resposta = anfitriao
            .pedir(
                PERFIS,
                PersonId(pessoa),
                &contexto,
                &pedido.to_string(),
                quintal,
            )
            .unwrap();
        let resposta: serde_json::Value = serde_json::from_str(&resposta).unwrap();
        assert_eq!(resposta["ok"], true, "{rev}: {pedido} → {resposta}");
        resposta
    };
    let revisao = pedir(serde_json::json!({"op":"view"}))["profiles"][pessoa.to_string()]
        ["revision"]
        .as_u64()
        .unwrap();
    pedir(serde_json::json!({"op":"save","revision":revisao,"profile":perfil}));
    let data_url = format!("data:image/{tipo};base64,{}", base64(&entrada(capa)));
    let token =
        pedir(serde_json::json!({"op":"upload-start","slot":"banner","length":data_url.len()}))
            ["token"]
            .as_str()
            .unwrap()
            .to_owned();
    for (indice, parte) in data_url.as_bytes().chunks(6000).enumerate() {
        pedir(
            serde_json::json!({"op":"upload-part","token":token,"index":indice,
            "part":std::str::from_utf8(parte).unwrap()}),
        );
    }
}

struct Par {
    conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    _ponta: seele_enlace::Ponta,
    pedidos: u32,
}

async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    use ed25519_dalek::{Signer, SigningKey};
    use seele_proto::control::{ClientMessage, ServerMessage};
    use seele_server::frame;
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
            faixa: seele_proto::FAIXA,
            client: "gerador-da-fixture".into(),
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
    let mut chegada = frame::read::<ServerMessage>(&mut recebe).await?;
    if let ServerMessage::ModsExigidos { conjunto, .. } = chegada {
        frame::write(&mut envio, &ClientMessage::AceitarMods { conjunto }).await?;
        chegada = frame::read::<ServerMessage>(&mut recebe).await?;
    }
    anyhow::ensure!(
        matches!(chegada, ServerMessage::Session { .. }),
        "o servidor não mandou Session"
    );
    Ok(Par {
        conexao,
        envio,
        recebe,
        _ponta: ponta,
        pedidos: 0,
    })
}

impl Par {
    async fn pedir(&mut self, carga: serde_json::Value) -> Result<serde_json::Value> {
        use seele_proto::control::{ClientMessage, ServerMessage};
        use seele_server::frame;
        self.pedidos += 1;
        let n = self.pedidos;
        frame::write(
            &mut self.envio,
            &ClientMessage::ModRequest {
                request: n,
                id: PERFIS.into(),
                channel: seele_proto::ids::ChannelId(0),
                payload: carga.to_string(),
            },
        )
        .await?;
        let fim = tokio::time::Instant::now() + PRAZO;
        while tokio::time::Instant::now() < fim {
            let Ok(Ok(quadro)) = tokio::time::timeout(
                Duration::from_millis(500),
                frame::read::<ServerMessage>(&mut self.recebe),
            )
            .await
            else {
                continue;
            };
            if let ServerMessage::ModReply {
                request, payload, ..
            } = quadro
            {
                if request == n {
                    let resposta: serde_json::Value = serde_json::from_str(&payload)?;
                    anyhow::ensure!(resposta["ok"] == true, "{carga} → {resposta}");
                    return Ok(resposta);
                }
            }
        }
        anyhow::bail!("pedido {carga} sem resposta")
    }

    /// `upload-start`, o fluxo de volume com os bytes, `upload-finish`.
    async fn enviar_imagem(&mut self, lado: &str, nome: &str) -> Result<()> {
        use seele_proto::volume::{PedidoDeImagem, RespostaDeImagem, VolumeHeader};
        use seele_server::frame;
        let bytes = entrada(nome);
        let token = self
            .pedir(serde_json::json!({"op":"upload-start","slot":lado,"bytes":bytes.len(),"transporte":"volume"}))
            .await?["token"]
            .as_str()
            .unwrap()
            .to_owned();
        let (mut envia, mut recebe) = self.conexao.open_bi().await?;
        frame::write(
            &mut envia,
            &PedidoDeImagem::Enviar {
                cabecalho: VolumeHeader {
                    mod_id: PERFIS.into(),
                    token: token.clone(),
                },
                bytes: bytes.len() as u64,
            },
        )
        .await?;
        envia.write_all(&bytes).await?;
        envia.finish()?;
        let resposta: RespostaDeImagem = frame::read(&mut recebe).await?;
        anyhow::ensure!(resposta.erro.is_none(), "{:?}", resposta.erro);
        self.pedir(serde_json::json!({"op":"upload-finish","token":token}))
            .await?;
        Ok(())
    }

    async fn salvar(&mut self, perfil: serde_json::Value) -> Result<String> {
        let visto = self.pedir(serde_json::json!({"op":"view"})).await?;
        let eu = visto["me"].as_str().unwrap().to_owned();
        let revisao = visto["profiles"][&eu]["revision"].as_u64().unwrap();
        self.pedir(serde_json::json!({"op":"save","revision":revisao,"profile":perfil}))
            .await?;
        Ok(eu)
    }
}

/// **Gera `tests/fixtures/perfis-5.0.0/`** com o código real do MOD. Ver o topo
/// deste arquivo.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "gera a fixture; roda à mão, com o SEELE-MOD-PERFIS ao lado"]
async fn gerar_a_fixture() -> Result<()> {
    let mod_dir = pasta_do_mod();
    let temp = tempfile::tempdir()?;
    let raizes = RaizesDosMods {
        pacotes: temp.path().join("cache"),
        dados: temp.path().join("dados"),
    };
    let dados = raizes.dados_de(PERFIS);
    std::fs::create_dir_all(&dados)?;
    let banco = temp.path().join("seele.db");

    // 1. A história antiga: a pessoa 3 no 1.0.0, a pessoa 2 no 1.2.0.
    let mut quintal = BTreeMap::new();
    historia_antiga(
        "90f0859",
        &dados,
        3,
        serde_json::json!({"displayName":"Lu","pronouns":"","bio":"","status":"desde o 1.0","accent":"#F2521F","effect":"pulse"}),
        "capa-3.jpg",
        "jpeg",
        &mut quintal,
    );
    historia_antiga(
        "3128927",
        &dados,
        2,
        serde_json::json!({"displayName":"Rafa","pronouns":"ele/dele","bio":"Atualizei do 1.2.","status":"","accent":"#12AB34","effect":"sparkle"}),
        "capa-2.png",
        "png",
        &mut quintal,
    );
    // 2. O 5.0.0, pelo servidor inteiro.
    let mut arquivos = vec![
        (
            "mod.json".to_owned(),
            std::fs::read(mod_dir.join("mod.json"))?,
        ),
        (
            "cliente/main.js".to_owned(),
            std::fs::read(mod_dir.join("cliente/main.js"))?,
        ),
        (
            "servidor/main.js".to_owned(),
            std::fs::read(mod_dir.join("servidor/main.js"))?,
        ),
    ];
    let manifesto: serde_json::Value = serde_json::from_slice(&arquivos[0].1)?;
    assert_eq!(
        manifesto["version"], "5.0.0",
        "o SEELE-MOD-PERFIS não está no 5.0.0"
    );
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let pacote = raizes.pacote_de(&hash);
    for (nome, bytes) in &arquivos {
        let alvo = pacote.join(nome);
        std::fs::create_dir_all(alvo.parent().unwrap())?;
        std::fs::write(alvo, bytes)?;
    }
    let linha = EnabledMod {
        id: PERFIS.into(),
        version: "5.0.0".into(),
        hash,
        repo: manifesto["repo"].as_str().unwrap().into(),
        reach: serde_json::from_value(manifesto["reach"].clone())?,
        server_half: true,
        api: 7,
        tema: false,
        plataformas: None,
    };
    // A linha antes do quintal (a chave estrangeira). A subida a desliga — o
    // PERFIS é incorporado na 0.19 —, e ela é religada logo abaixo: o gerador
    // faz o papel de um servidor 0.18.
    {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        seele_server::persistence::mods::enable(&persistence, &linha)?;
        seele_server::persistence::mods::gravar_quintal(&mut persistence, PERFIS, &quintal)?;
    }
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::File(banco.clone()),
            mods_dir: Some(raizes.clone()),
            ..ServerConfig::default()
        })
        .await?,
    );
    {
        let db = daemon.server().persistence.lock().await;
        seele_server::persistence::mods::enable(&db, &linha)?;
    }
    let endereco = daemon.local_addr()?;
    let aceitando = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });

    let mut maya = abrir(endereco, 1).await?;
    let mut rafa = abrir(endereco, 2).await?;
    let mut lu = abrir(endereco, 3).await?;
    assert_eq!(
        maya.salvar(serde_json::json!({"displayName":"Maya Ribeiro 🌙","pronouns":"ela/dela","bio":"Faço trilhas sonoras.\nE café.","status":"ao vivo às 20h","accent":"#3A7BD5","effect":"aurora"})).await?,
        "1"
    );
    maya.enviar_imagem("avatar", "foto-1.png").await?;
    maya.enviar_imagem("banner", "capa-1.jpg").await?;
    // As pessoas 2 e 3 já tinham o perfil salvo nas versões antigas: só a foto
    // é nova, pelo volume.
    assert_eq!(
        rafa.pedir(serde_json::json!({"op":"view"})).await?["me"],
        "2"
    );
    rafa.enviar_imagem("avatar", "foto-2.webp").await?;
    assert_eq!(lu.pedir(serde_json::json!({"op":"view"})).await?["me"], "3");
    lu.enviar_imagem("avatar", "foto-3.gif").await?;

    // 3. O que ficou: o quintal e a pasta do MOD, como estão no disco.
    let quintal = {
        let db = daemon.server().persistence.lock().await;
        seele_server::persistence::mods::ler_quintal(&db, PERFIS)?
    };
    let destino = fixture();
    let _ = std::fs::remove_dir_all(destino.join("dados"));
    for (caminho, bytes) in arvore(&dados) {
        if caminho
            .rsplit('/')
            .next()
            .is_some_and(|nome| nome.starts_with(".em-obras"))
        {
            continue;
        }
        let alvo = destino.join("dados").join(caminho);
        std::fs::create_dir_all(alvo.parent().unwrap())?;
        std::fs::write(alvo, bytes)?;
    }
    std::fs::write(
        destino.join("quintal.json"),
        serde_json::to_string_pretty(&quintal)? + "\n",
    )?;
    Ok(())
}
