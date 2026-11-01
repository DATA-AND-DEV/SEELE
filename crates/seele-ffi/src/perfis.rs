//! **O perfil público para a casca do desktop** (SEELE 0.19, protocolo 12,
//! `docs/0.19/perfis-oficiais.md` §5.1): ler os perfis, gravar o seu, trocar e
//! tirar a foto e a capa, e baixar a imagem de uma pessoa.
//!
//! O ffi não vê o `seele-proto` (ADR 0002): os tipos do fio chegam por
//! `seele_core::perfil_do_fio` e o erro tipado, a validação e o portão de versão,
//! por `seele_core::perfil` (que é o `seele-sessao`, o mesmo código que o
//! navegador roda). Este módulo só põe a **conexão** em volta.
//!
//! # O que sai e o que não sai
//!
//! - **Nada novo vai a um servidor que negociou menos que o 12.** Cada método
//!   confere [`seele_core::perfil::portao_do_cliente`] (ou o do pedido de
//!   imagem) com o número que a conexão de agora negociou, e devolve
//!   [`ErroDoPerfil::ServidorAntigo`] **sem escrever nada**: um servidor 11
//!   leria uma variante que não conhece e o fluxo de controle desalinharia.
//!   Os 0.18 seguem vendo o apelido e o ícone nativo.
//! - **A imagem é conferida antes de subir**: o tamanho (até 10 MiB) e o tipo
//!   pelos bytes; o HEIC volta nomeado. O servidor confere de novo.
//! - **Os perfis ficam fora do `Snapshot`**, como o FORA DO AR: a casca lê
//!   [`Connection::perfis`] quando `perfis_revision` anda.
//!
//! # O cache das imagens é pelo hash, e para sempre
//!
//! O hash é o endereço (SHA-256 dos bytes): um hash nunca passa a apontar para
//! outros bytes, então a imagem baixada vale até a conexão acabar, sem validade.
//! Os bytes recebidos são **conferidos contra o hash pedido** antes de entrar no
//! cache — um servidor (ou um bug) que respondesse outra imagem não a deixaria
//! guardada sob um endereço que não é o dela. O cache tem um teto de bytes
//! ([`TETO_DO_CACHE`]); passando dele, sai a mais antiga: o teto é de memória, e
//! não de validade.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// O SHA-256 de uma imagem: o endereço com que o servidor a guarda, e com que
/// o perfil padrão desta máquina guarda a dele (`apps/seele-app`), para saber
/// sem subir de novo que o servidor já tem a mesma.
pub use seele_core::perfil::hash_da_imagem;
pub use seele_core::perfil::{
    conferir_imagem, entrada_validada, hash_de_hex, tipo_de_midia, EntradaDoPerfil, ErroDoPerfil,
    ImagemNaTela, PerfilNaTela, NOME_DO_QUE_MUDOU,
};
use seele_core::perfil::{
    conferir_tamanho, erro_do_servidor, mensagem_de_tirar, montar_gravar, portao_do_cliente,
    portao_do_pedido_de_imagem,
};
pub use seele_core::perfil_do_fio::{
    CampoDoPerfil, Efeito, ErroDeCampo, ImagemDoPerfil, LadoDoPerfil, MotivoDoCampo, PerfilPublico,
    RecusaDePerfil, TipoDaImagem, COR_PADRAO, MAX_BIO, MAX_NOME_EXIBIDO, MAX_PRONOMES, MAX_STATUS,
    TETO_DA_IMAGEM,
};

use crate::{Command, Connection, ConnectionError};

/// O que um comando de perfil da casca pode rejeitar: o erro do perfil, ou o
/// da conexão (a sessão que não existe mais, o carimbo de outra volta).
///
/// `untagged`: a casca lê o nome da variante de dentro (`"Heic"`,
/// `{"Campos": …}`, `"NotConnected"`, `{"SessaoNaoConfere": …}`), sem um nível
/// a mais para descobrir de qual das duas veio.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(untagged)]
pub enum FalhaNoPerfil {
    /// O pedido de perfil não saiu ou foi recusado.
    Perfil(ErroDoPerfil),
    /// A conexão que o comando nomeia não é a de agora.
    Conexao(ConnectionError),
}

impl From<ErroDoPerfil> for FalhaNoPerfil {
    fn from(erro: ErroDoPerfil) -> Self {
        Self::Perfil(erro)
    }
}

impl From<ConnectionError> for FalhaNoPerfil {
    fn from(erro: ConnectionError) -> Self {
        Self::Conexao(erro)
    }
}

/// Quantos bytes de imagem o cache guarda por conexão: 64 MiB, o que cabe 6
/// imagens do teto ou milhares de fotos reais. Passou, sai a mais antiga.
pub const TETO_DO_CACHE: usize = 64 * 1024 * 1024;

/// O que a conexão guarda sobre perfis: o protocolo que ela negociou e as
/// imagens já baixadas.
#[derive(Debug, Default)]
pub(crate) struct DoPerfil {
    /// O protocolo negociado, escrito pelo laço da conexão a cada (re)conexão.
    /// Zero é «ainda não sei», e nenhum pedido de perfil sai com ele.
    protocolo: std::sync::atomic::AtomicU8,
    imagens: Mutex<CacheDeImagens>,
}

impl DoPerfil {
    /// Um `DoPerfil` que já sabe o que negociou: os testes e a conexão de
    /// mentira (`sem_rede`), que não têm aperto de mão.
    #[cfg(any(test, feature = "conformidade"))]
    #[must_use]
    pub(crate) fn negociado(protocolo: u8) -> Self {
        Self {
            protocolo: std::sync::atomic::AtomicU8::new(protocolo),
            imagens: Mutex::default(),
        }
    }

    pub(crate) fn guardar_o_protocolo(&self, protocolo: u8) {
        self.protocolo
            .store(protocolo, std::sync::atomic::Ordering::Relaxed);
    }

    fn protocolo(&self) -> u8 {
        self.protocolo.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// As imagens baixadas, pelo hash, com teto de memória.
#[derive(Debug, Default)]
struct CacheDeImagens {
    por_hash: HashMap<[u8; 32], Arc<Vec<u8>>>,
    /// A ordem de chegada, para saber qual sai primeiro.
    ordem: VecDeque<[u8; 32]>,
    bytes: usize,
}

impl CacheDeImagens {
    fn ler(&self, hash: &[u8; 32]) -> Option<Arc<Vec<u8>>> {
        self.por_hash.get(hash).cloned()
    }

    fn guardar(&mut self, hash: [u8; 32], bytes: Arc<Vec<u8>>, teto: usize) {
        if self.por_hash.contains_key(&hash) || bytes.len() > teto {
            return;
        }
        self.bytes += bytes.len();
        self.por_hash.insert(hash, bytes);
        self.ordem.push_back(hash);
        while self.bytes > teto {
            let Some(velha) = self.ordem.pop_front() else {
                break;
            };
            if let Some(saiu) = self.por_hash.remove(&velha) {
                self.bytes = self.bytes.saturating_sub(saiu.len());
            }
        }
    }
}

impl Connection {
    /// Os perfis públicos que esta sessão conhece, em ordem de pessoa, na forma
    /// da casca ([`PerfilNaTela`]: a cor em `#rrggbb`, o hash em hexadecimal).
    ///
    /// **Fora do `Snapshot`** (ver [`crate::Snapshot::perfis_revision`]): a casca
    /// chama isto quando o número anda. Vazio num servidor 0.18, que nunca manda
    /// perfil — a casca cai para o apelido e o ícone nativo.
    #[must_use]
    pub fn perfis(&self) -> Vec<PerfilNaTela> {
        self.shared
            .room
            .lock()
            .map(|room| {
                room.perfis
                    .values()
                    .map(seele_core::perfil::perfil_na_tela)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// O perfil de uma pessoa, quando ela tem um.
    #[must_use]
    pub fn perfil_de(&self, pessoa: u64) -> Option<PerfilNaTela> {
        self.shared.room.lock().ok().and_then(|room| {
            room.perfis
                .get(&seele_core::PersonId(pessoa))
                .map(seele_core::perfil::perfil_na_tela)
        })
    }

    /// O número de protocolo que a conexão de agora negociou; zero antes do
    /// aperto de mão. A casca o usa para esconder o editor num servidor 0.18.
    #[must_use]
    pub fn protocolo_negociado(&self) -> u8 {
        self.shared.perfis.protocolo()
    }

    fn comando_de_perfil(&self, comando: Command) -> Result<(), ErroDoPerfil> {
        self.command(comando).map_err(|erro| match erro {
            ConnectionError::NotConnected => ErroDoPerfil::NotConnected,
            _ => ErroDoPerfil::SemResposta,
        })
    }

    /// **Grava o seu perfil.** `revisao` é a que o editor viu (a do
    /// [`PerfilNaTela::revisao`] de `perfil_de(eu)`, ou zero para quem ainda
    /// não tem perfil); se não for a atual, volta `PerfilRecusado { RevisaoVelha }`.
    ///
    /// Valida com [`seele_core::perfil::montar_gravar`] — a mesma regra do
    /// servidor — **antes** de qualquer coisa: o erro volta por campo, todos de
    /// uma vez, e nada sai. Apelido não é daqui: é o `set_nickname`.
    ///
    /// # Errors
    ///
    /// [`ErroDoPerfil::Campos`], [`ErroDoPerfil::ServidorAntigo`] ou
    /// [`ErroDoPerfil::NotConnected`].
    pub fn gravar_perfil(
        &self,
        revisao: u32,
        entrada: &EntradaDoPerfil,
    ) -> Result<(), ErroDoPerfil> {
        let mensagem = montar_gravar(revisao, entrada)?;
        portao_do_cliente(&mensagem, self.shared.perfis.protocolo())?;
        self.comando_de_perfil(Command::GravarPerfil(mensagem))
    }

    /// **Tira a sua foto ou a sua capa.** Não confere revisão, mas sobe o
    /// número dela no servidor.
    ///
    /// # Errors
    ///
    /// [`ErroDoPerfil::ServidorAntigo`] ou [`ErroDoPerfil::NotConnected`].
    pub fn tirar_imagem_do_perfil(&self, lado: LadoDoPerfil) -> Result<(), ErroDoPerfil> {
        portao_do_cliente(&mensagem_de_tirar(lado), self.shared.perfis.protocolo())?;
        self.comando_de_perfil(Command::TirarImagemDoPerfil { lado })
    }

    /// Manda ao servidor (e aguarda a gravação) os bytes `bytes` como a sua
    /// foto ou a sua capa. Uma subida por vez por pessoa é regra do servidor.
    ///
    /// **Confere antes de subir**: não vazio, até [`TETO_DA_IMAGEM`], e o tipo
    /// pelos bytes (PNG, JPEG, WebP ou GIF; o HEIC volta como
    /// [`ErroDoPerfil::Heic`]). Só então abre o fluxo.
    ///
    /// # Errors
    ///
    /// Os da conferência; [`ErroDoPerfil::ServidorAntigo`] num servidor que
    /// negociou menos que o 12; [`ErroDoPerfil::Recusada`] e companhia quando o
    /// servidor recusa; [`ErroDoPerfil::SemResposta`] se a sessão acabou ou o
    /// prazo esgotou.
    pub async fn enviar_imagem_do_perfil(
        &self,
        lado: LadoDoPerfil,
        bytes: Vec<u8>,
    ) -> Result<(), ErroDoPerfil> {
        conferir_imagem(&bytes)?;
        let pedido = seele_core::PedidoDeImagem::EnviarAoPerfil {
            lado,
            bytes: bytes.len() as u64,
        };
        portao_do_pedido_de_imagem(&pedido, self.shared.perfis.protocolo())?;
        self.imagem_de_mod(pedido, bytes)
            .await
            .map(|_| ())
            .map_err(|nome| erro_do_nome(&nome))
    }

    /// O mesmo, de um arquivo no disco: o tamanho é perguntado **antes** de
    /// ler, para um vídeo de gigabytes escolhido por engano não ir para a
    /// memória.
    ///
    /// # Errors
    ///
    /// Os de [`Self::enviar_imagem_do_perfil`], e [`ErroDoPerfil::ArquivoIlegivel`]
    /// quando o arquivo não abre ou não é lido por inteiro.
    pub async fn enviar_imagem_do_perfil_de_arquivo(
        &self,
        lado: LadoDoPerfil,
        caminho: &Path,
    ) -> Result<(), ErroDoPerfil> {
        let bytes = ler_o_arquivo_da_imagem(caminho)?;
        self.enviar_imagem_do_perfil(lado, bytes).await
    }

    /// **Baixa a imagem de uma pessoa** (`perfil_de(pessoa).foto` ou `.capa`,
    /// com o hash que o perfil traz), pela ponte de mídia — o mesmo fluxo
    /// bidirecional dos MODs.
    ///
    /// O cache é pelo hash e **permanente** (ver o doc do módulo): a segunda
    /// chamada com o mesmo hash não toca a rede. O que chega é conferido contra
    /// o hash pedido. Se o servidor diz que o hash já não é o atual, volta
    /// [`ErroDoPerfil::Mudou`]: a casca relê [`Self::perfis`] e pede o novo.
    ///
    /// # Errors
    ///
    /// [`ErroDoPerfil::ServidorAntigo`], [`ErroDoPerfil::Mudou`],
    /// [`ErroDoPerfil::Incompleto`] (os bytes não são os do hash) e os de
    /// transporte.
    pub async fn imagem_do_perfil(
        &self,
        pessoa: u64,
        lado: LadoDoPerfil,
        hash: [u8; 32],
    ) -> Result<Arc<Vec<u8>>, ErroDoPerfil> {
        if let Some(guardada) = self
            .shared
            .perfis
            .imagens
            .lock()
            .ok()
            .and_then(|cache| cache.ler(&hash))
        {
            return Ok(guardada);
        }
        let pedido = seele_core::PedidoDeImagem::LerDoPerfil {
            pessoa: seele_core::PersonId(pessoa),
            lado,
            hash,
        };
        portao_do_pedido_de_imagem(&pedido, self.shared.perfis.protocolo())?;
        let bytes = self
            .imagem_de_mod(pedido, Vec::new())
            .await
            .map_err(|nome| erro_do_nome(&nome))?;
        // O endereço é o hash: bytes que não o têm não entram no cache, e não
        // voltam como se fossem a imagem.
        if hash_da_imagem(&bytes) != hash {
            tracing::warn!(
                pessoa,
                "a imagem do perfil que chegou não tem o hash pedido"
            );
            return Err(ErroDoPerfil::Incompleto);
        }
        let bytes = Arc::new(bytes);
        if let Ok(mut cache) = self.shared.perfis.imagens.lock() {
            cache.guardar(hash, Arc::clone(&bytes), TETO_DO_CACHE);
        }
        Ok(bytes)
    }
}

/// O erro tipado que um nome da camada de transporte quer dizer.
fn erro_do_nome(nome: &str) -> ErroDoPerfil {
    match nome {
        // O nome da própria sessão que acabou, como o da ponte dos MODs.
        "sessao-encerrada" => ErroDoPerfil::SemResposta,
        outro => {
            let erro = erro_do_servidor(outro);
            if erro == ErroDoPerfil::RecusaDesconhecida {
                tracing::warn!(
                    nome = outro,
                    "o servidor recusou a imagem do perfil com um nome que esta versão não conhece"
                );
            }
            erro
        }
    }
}

/// Lê o arquivo da imagem confiando no tamanho que o disco diz **antes** de
/// puxá-lo, e depois conferindo o que veio: um arquivo vazio ou acima de
/// [`TETO_DA_IMAGEM`] volta recusado sem ser lido. Não confere o tipo — quem
/// envia confere (`enviar_imagem_do_perfil`).
///
/// # Errors
///
/// [`ErroDoPerfil::ArquivoIlegivel`], [`ErroDoPerfil::ImagemVazia`] e
/// [`ErroDoPerfil::ImagemGrandeDemais`].
pub fn ler_o_arquivo_da_imagem(caminho: &Path) -> Result<Vec<u8>, ErroDoPerfil> {
    use std::io::Read as _;
    let arquivo = std::fs::File::open(caminho).map_err(|_| ErroDoPerfil::ArquivoIlegivel)?;
    let tamanho = arquivo
        .metadata()
        .map_err(|_| ErroDoPerfil::ArquivoIlegivel)?
        .len();
    conferir_tamanho(tamanho)?;
    let mut bytes = Vec::with_capacity(usize::try_from(tamanho).unwrap_or(0));
    // `take` com um a mais: um arquivo que cresceu entre o `metadata` e a
    // leitura é pego pela conferência de cima, e não puxa a memória inteira.
    arquivo
        .take(TETO_DA_IMAGEM as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ErroDoPerfil::ArquivoIlegivel)?;
    conferir_tamanho(bytes.len() as u64)?;
    Ok(bytes)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_cache_expulsa_a_mais_antiga_ao_passar_do_teto() {
        let mut cache = CacheDeImagens::default();
        let uma = Arc::new(vec![1_u8; 40]);
        let outra = Arc::new(vec![2_u8; 40]);
        cache.guardar([1; 32], Arc::clone(&uma), 100);
        cache.guardar([2; 32], Arc::clone(&outra), 100);
        assert!(cache.ler(&[1; 32]).is_some() && cache.ler(&[2; 32]).is_some());
        cache.guardar([3; 32], Arc::new(vec![3_u8; 40]), 100);
        assert!(cache.ler(&[1; 32]).is_none(), "a mais antiga ficou");
        assert!(cache.ler(&[2; 32]).is_some() && cache.ler(&[3; 32]).is_some());
        assert_eq!(cache.bytes, 80);
        // Uma imagem maior que o cache inteiro não entra (e não expulsa as outras).
        cache.guardar([4; 32], Arc::new(vec![4_u8; 101]), 100);
        assert!(cache.ler(&[4; 32]).is_none());
        assert!(cache.ler(&[2; 32]).is_some());
    }

    #[test]
    fn o_arquivo_grande_demais_e_recusado_sem_ser_lido() {
        let pasta = tempdir();
        let grande = pasta.join("grande.png");
        // Esparso: o tamanho passa do teto sem gastar o disco.
        let arquivo = std::fs::File::create(&grande).unwrap();
        arquivo.set_len(TETO_DA_IMAGEM as u64 + 1).unwrap();
        assert_eq!(
            ler_o_arquivo_da_imagem(&grande),
            Err(ErroDoPerfil::ImagemGrandeDemais {
                limite_bytes: TETO_DA_IMAGEM as u64
            })
        );
        let vazio = pasta.join("vazio.png");
        std::fs::write(&vazio, b"").unwrap();
        assert_eq!(
            ler_o_arquivo_da_imagem(&vazio),
            Err(ErroDoPerfil::ImagemVazia)
        );
        assert_eq!(
            ler_o_arquivo_da_imagem(&pasta.join("nao-existe.png")),
            Err(ErroDoPerfil::ArquivoIlegivel)
        );
        let bom = pasta.join("bom.png");
        std::fs::write(&bom, b"\x89PNG\r\n\x1a\n....").unwrap();
        assert_eq!(ler_o_arquivo_da_imagem(&bom).unwrap().len(), 12);
        let _ = std::fs::remove_dir_all(&pasta);
    }

    fn tempdir() -> std::path::PathBuf {
        let pasta = std::env::temp_dir().join(format!(
            "seele-ffi-perfis-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&pasta).unwrap();
        pasta
    }

    // ---- a conexão -------------------------------------------------------

    use std::sync::Mutex as Trava;

    use seele_core::{PersonId, ServerMessage, SessionId, Ssrc};
    use tokio::sync::mpsc::UnboundedReceiver;

    use crate::{Event, EventListener};

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    /// Uma conexão que não existe, no protocolo `negociado`, com a fila de
    /// comandos na mão: o que a casca mandar e **sair** aparece nela.
    fn conexao(negociado: u8) -> (Connection, UnboundedReceiver<Command>) {
        let shared = crate::sem_rede::shared("marcela");
        shared.perfis.guardar_o_protocolo(negociado);
        if let Ok(mut room) = shared.room.lock() {
            room.apply(&ServerMessage::Session {
                id: SessionId(1),
                person: PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: Vec::new(),
            });
        }
        let (commands, fila) = tokio::sync::mpsc::unbounded_channel();
        let conexao = Connection {
            commands,
            shared,
            encerrada: Trava::new(None),
            ligacao: None,
        };
        (conexao, fila)
    }

    fn entrada() -> EntradaDoPerfil {
        EntradaDoPerfil {
            nome_exibido: "Maya".into(),
            pronomes: String::new(),
            status: String::new(),
            bio: String::new(),
            cor: "#f2521f".into(),
            efeito: Efeito::Nenhum,
        }
    }

    fn perfil(pessoa: u64, revisao: u32) -> PerfilPublico {
        PerfilPublico {
            pessoa: PersonId(pessoa),
            revisao,
            nome_exibido: format!("pessoa {pessoa}"),
            pronomes: String::new(),
            status: String::new(),
            bio: String::new(),
            cor: COR_PADRAO,
            efeito: Efeito::Nenhum,
            foto: None,
            capa: None,
        }
    }

    struct Anota(Trava<Vec<Event>>);
    impl EventListener for Anota {
        fn on_event(&self, event: Event) {
            if let Ok(mut anotados) = self.0.lock() {
                anotados.push(event);
            }
        }
    }

    /// **O fold chega à casca**: `Perfis` em partes e `PerfilMudou` aparecem em
    /// `perfis()`, a revisão do `Snapshot` anda **antes** do aviso, a recusa vira
    /// evento e a pendência vai ao `Snapshot`.
    #[test]
    fn os_perfis_chegam_a_casca_pelo_fold() {
        let (conexao, _fila) = conexao(12);
        let anota = Arc::new(Anota(Trava::new(Vec::new())));
        conexao.subscribe(Arc::clone(&anota) as Arc<dyn EventListener>);
        assert_eq!(conexao.snapshot().perfis_revision, 0);

        crate::receber(
            &conexao.shared,
            &ServerMessage::Perfis {
                parte: vec![perfil(2, 1)],
                ultima: false,
            },
        );
        assert!(conexao.perfis().is_empty(), "uma parte sem `ultima` valeu");
        assert_eq!(conexao.snapshot().perfis_revision, 0);

        crate::receber(
            &conexao.shared,
            &ServerMessage::Perfis {
                parte: vec![perfil(1, 3)],
                ultima: true,
            },
        );
        let revisao = conexao.snapshot().perfis_revision;
        assert_eq!(revisao, 1);
        assert_eq!(
            conexao
                .perfis()
                .iter()
                .map(|p| p.pessoa)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );

        let mut novo = perfil(1, 4);
        novo.nome_exibido = "Ana".into();
        crate::receber(&conexao.shared, &ServerMessage::PerfilMudou(novo));
        assert_eq!(conexao.snapshot().perfis_revision, revisao + 1);
        assert_eq!(conexao.perfil_de(1).unwrap().nome_exibido, "Ana");
        assert!(conexao.perfil_de(99).is_none());

        crate::receber(
            &conexao.shared,
            &ServerMessage::PerfilRecusado {
                motivo: RecusaDePerfil::Campos,
                campos: vec![ErroDeCampo {
                    campo: CampoDoPerfil::Bio,
                    motivo: MotivoDoCampo::Longo,
                }],
            },
        );
        crate::receber(
            &conexao.shared,
            &ServerMessage::PendenciaDoPerfil {
                faltou: vec![LadoDoPerfil::Capa],
            },
        );
        assert_eq!(
            conexao.snapshot().pendencia_do_perfil,
            vec![LadoDoPerfil::Capa]
        );
        let eventos = anota.0.lock().unwrap().clone();
        let marcas: Vec<u64> = eventos
            .iter()
            .filter_map(|e| match e {
                Event::PerfisChanged { marca } => Some(*marca),
                _ => None,
            })
            .collect();
        // A lista fechada, o `PerfilMudou` e a pendência: três avisos, a marca
        // nunca andando para trás, e a revisão do `Snapshot` já a de cada um.
        assert_eq!(marcas.len(), 3, "{eventos:?}");
        assert!(marcas.windows(2).all(|par| par[0] <= par[1]));
        assert!(eventos.iter().any(|e| matches!(
            e,
            Event::PerfilRecusado { motivo: RecusaDePerfil::Campos, campos } if campos.len() == 1
        )));
        // A recusa saiu da fila: não repete no próximo evento.
        assert!(conexao
            .shared
            .room
            .lock()
            .unwrap()
            .recusas_de_perfil
            .is_empty());
    }

    /// **Nenhum verbo novo sai para um servidor 11**, e nenhum pedido de imagem
    /// de perfil abre fluxo. Voltar o portão (tirar a chamada em cada método)
    /// faz este teste falhar: o comando chega à fila.
    #[tokio::test]
    async fn um_servidor_onze_nao_recebe_nada_do_perfil() {
        let (conexao, mut fila) = conexao(11);
        let antigo = ErroDoPerfil::ServidorAntigo {
            negociado: 11,
            exige: 12,
        };
        assert_eq!(conexao.gravar_perfil(0, &entrada()), Err(antigo.clone()));
        assert_eq!(
            conexao.tirar_imagem_do_perfil(LadoDoPerfil::Foto),
            Err(antigo.clone())
        );
        assert_eq!(
            conexao
                .enviar_imagem_do_perfil(LadoDoPerfil::Capa, PNG.to_vec())
                .await,
            Err(antigo.clone())
        );
        assert_eq!(
            conexao
                .imagem_do_perfil(1, LadoDoPerfil::Foto, [9; 32])
                .await
                .unwrap_err(),
            antigo
        );
        assert!(
            fila.try_recv().is_err(),
            "um comando do protocolo 12 saiu para um servidor 11"
        );

        // O mesmo pedido, num servidor 12, sai.
        let (conexao, mut fila) = conexao_de_novo(12);
        assert_eq!(conexao.gravar_perfil(2, &entrada()), Ok(()));
        assert_eq!(conexao.tirar_imagem_do_perfil(LadoDoPerfil::Foto), Ok(()));
        assert!(matches!(
            fila.try_recv(),
            Ok(Command::GravarPerfil(
                seele_core::ClientMessage::GravarPerfil { revisao: 2, .. }
            ))
        ));
        assert!(matches!(
            fila.try_recv(),
            Ok(Command::TirarImagemDoPerfil {
                lado: LadoDoPerfil::Foto
            })
        ));
    }

    fn conexao_de_novo(negociado: u8) -> (Connection, UnboundedReceiver<Command>) {
        conexao(negociado)
    }

    /// Os campos inválidos voltam **por campo e nada sai**, mesmo num servidor
    /// que entende o perfil.
    #[test]
    fn campos_invalidos_voltam_por_campo_e_nada_sai() {
        let (conexao, mut fila) = conexao(12);
        let mut ruim = entrada();
        ruim.nome_exibido = "x".repeat(41);
        ruim.cor = "laranja".into();
        let Err(ErroDoPerfil::Campos { campos }) = conexao.gravar_perfil(1, &ruim) else {
            panic!("tinha de recusar");
        };
        assert_eq!(campos.len(), 2);
        assert!(fila.try_recv().is_err());
    }

    /// **As recusas de imagem acontecem antes de abrir o fluxo**: tamanho, tipo
    /// pelos bytes, HEIC e vazio. Nada chega à fila.
    #[tokio::test]
    async fn o_envio_recusa_tamanho_e_tipo_antes_de_sair() {
        let (conexao, mut fila) = conexao(12);
        let mut grande = PNG.to_vec();
        grande.resize(TETO_DA_IMAGEM + 1, 0);
        let heic = include_bytes!("../../seele-proto/tests/vetores/heic-8x8.heic").to_vec();
        for (bytes, esperado) in [
            (
                grande,
                ErroDoPerfil::ImagemGrandeDemais {
                    limite_bytes: TETO_DA_IMAGEM as u64,
                },
            ),
            (heic, ErroDoPerfil::Heic),
            (b"%PDF-1.7 ....".to_vec(), ErroDoPerfil::TipoRecusado),
            (Vec::new(), ErroDoPerfil::ImagemVazia),
        ] {
            assert_eq!(
                conexao
                    .enviar_imagem_do_perfil(LadoDoPerfil::Foto, bytes)
                    .await,
                Err(esperado)
            );
        }
        assert!(fila.try_recv().is_err(), "uma imagem recusada abriu fluxo");
    }

    /// Um servidor que responde: atende o próximo `ImagemDeMod` com o que o
    /// teste mandar, e devolve o pedido que viu.
    fn atender_uma_vez(
        mut fila: UnboundedReceiver<Command>,
        resposta: Result<Vec<u8>, String>,
    ) -> tokio::task::JoinHandle<(UnboundedReceiver<Command>, seele_core::PedidoDeImagem)> {
        tokio::spawn(async move {
            let Some(Command::ImagemDeMod {
                pedido,
                resposta: caixa,
                ..
            }) = fila.recv().await
            else {
                panic!("o comando não era um pedido de imagem");
            };
            let _ = caixa.send(resposta);
            (fila, pedido)
        })
    }

    /// **O cache é pelo hash, e para sempre**: a segunda leitura não toca a
    /// rede; os bytes que não têm o hash pedido não entram no cache; o nome
    /// `mudou` do servidor vira `Mudou`.
    #[tokio::test]
    async fn a_imagem_do_perfil_e_conferida_pelo_hash_e_guardada_para_sempre() {
        let imagem = PNG.to_vec();
        let hash = seele_core::perfil::hash_da_imagem(&imagem);

        // Um servidor que responde outra imagem: recusada, e não guardada.
        let (conexao, fila) = conexao(12);
        let servidor = atender_uma_vez(fila, Ok(b"outra imagem".to_vec()));
        assert_eq!(
            conexao
                .imagem_do_perfil(3, LadoDoPerfil::Foto, hash)
                .await
                .unwrap_err(),
            ErroDoPerfil::Incompleto
        );
        let (fila, pedido) = servidor.await.unwrap();
        assert!(matches!(
            pedido,
            seele_core::PedidoDeImagem::LerDoPerfil { lado: LadoDoPerfil::Foto, hash: h, .. }
                if h == hash
        ));

        // O hash que o servidor diz que já não é o atual.
        let servidor = atender_uma_vez(fila, Err(NOME_DO_QUE_MUDOU.to_owned()));
        assert_eq!(
            conexao
                .imagem_do_perfil(3, LadoDoPerfil::Foto, hash)
                .await
                .unwrap_err(),
            ErroDoPerfil::Mudou
        );
        let (fila, _) = servidor.await.unwrap();

        // A imagem certa: vem da rede uma vez...
        let servidor = atender_uma_vez(fila, Ok(imagem.clone()));
        let baixada = conexao
            .imagem_do_perfil(3, LadoDoPerfil::Foto, hash)
            .await
            .unwrap();
        assert_eq!(*baixada, imagem);
        let (mut fila, _) = servidor.await.unwrap();

        // ...e a segunda não pede nada, nem a outra pessoa que tenha a mesma
        // imagem (o endereço é o hash).
        let de_novo = conexao
            .imagem_do_perfil(3, LadoDoPerfil::Foto, hash)
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&baixada, &de_novo));
        conexao
            .imagem_do_perfil(8, LadoDoPerfil::Capa, hash)
            .await
            .unwrap();
        assert!(fila.try_recv().is_err(), "o cache foi à rede de novo");
    }
}
