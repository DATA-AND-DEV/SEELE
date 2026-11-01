//! **O perfil padrão desta máquina** (0.19, decisão do dono «Padrão + ajuste
//! por servidor»).
//!
//! O «Seu perfil» da tela inicial, fora de qualquer servidor, edita o perfil
//! inteiro — nome exibido, pronomes, status, bio, cor, efeito, foto e capa — e
//! o que se salva ali é o **padrão**: guardado aqui, e aplicado pela tela
//! (`ui/perfil-padrao.js`) a cada servidor 12 em que a pessoa ainda não tem
//! perfil, e aos que seguem o padrão quando ele muda.
//!
//! # Onde mora
//!
//! Numa pasta ao lado das preferências e do `retrato.png`
//! (`<config>/perfil-padrao/`), pela mesma razão do retrato: binário num
//! arquivo de `nome <TAB> valor` faria uma edição à mão virar suporte.
//!
//! - `padrao.json`: os textos (já validados), a revisão e o endereço de cada
//!   imagem;
//! - `imagens/<sha256>`: os bytes, **pelo hash** — o mesmo endereço com que o
//!   servidor as guarda, e é por ele que a tela sabe, sem subir de novo, que um
//!   servidor já tem a mesma foto;
//! - `servidores.json`: por impressão do servidor, se ele **segue o padrão**, a
//!   revisão do padrão aplicada por último e a assinatura do que foi aplicado.
//!
//! Toda gravação passa por um arquivo `.em-obras-*` e termina num `rename`: um
//! desligamento no meio deixa o arquivo de antes, e não meio arquivo.
//!
//! # A regra é a do servidor
//!
//! Os textos passam por `seele_ffi::perfis::entrada_validada`, que é a
//! `seele_proto::perfil::validar` do servidor; as imagens, por
//! `conferir_imagem` (até 10 MiB, o tipo pelos bytes, o HEIC nomeado). Um
//! padrão guardado é um padrão que o servidor aceita como está.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use seele_ffi::perfis::{
    conferir_imagem, entrada_validada, hash_da_imagem, tipo_de_midia, Efeito, EntradaDoPerfil,
    ErroDoPerfil, LadoDoPerfil,
};

/// A cor de quem nunca escolheu uma: a do servidor (`COR_PADRAO`).
const COR_INICIAL: &str = "#f2521f";

/// Uma imagem do padrão: o endereço (SHA-256 em hexadecimal), o tipo de mídia e
/// o tamanho.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ImagemDoPadrao {
    pub(crate) hash: String,
    pub(crate) tipo: String,
    pub(crate) bytes: u32,
}

/// **O padrão como fica no disco.** `revisao` zero é «nunca salvo»: a tela não
/// aplica nada de um padrão que a pessoa nunca escreveu.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct PerfilPadrao {
    pub(crate) revisao: u64,
    pub(crate) nome_exibido: String,
    pub(crate) pronomes: String,
    pub(crate) status: String,
    pub(crate) bio: String,
    pub(crate) cor: String,
    pub(crate) efeito: Efeito,
    pub(crate) foto: Option<ImagemDoPadrao>,
    pub(crate) capa: Option<ImagemDoPadrao>,
}

impl Default for PerfilPadrao {
    fn default() -> Self {
        Self {
            revisao: 0,
            nome_exibido: String::new(),
            pronomes: String::new(),
            status: String::new(),
            bio: String::new(),
            cor: COR_INICIAL.to_owned(),
            efeito: Efeito::Nenhum,
            foto: None,
            capa: None,
        }
    }
}

impl PerfilPadrao {
    fn imagem(&self, lado: LadoDoPerfil) -> Option<&ImagemDoPadrao> {
        match lado {
            LadoDoPerfil::Foto => self.foto.as_ref(),
            LadoDoPerfil::Capa => self.capa.as_ref(),
        }
    }

    fn imagem_mut(&mut self, lado: LadoDoPerfil) -> &mut Option<ImagemDoPadrao> {
        match lado {
            LadoDoPerfil::Foto => &mut self.foto,
            LadoDoPerfil::Capa => &mut self.capa,
        }
    }
}

/// Uma imagem do padrão como a tela a desenha: o endereço e o `data:`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ImagemDoPadraoNaTela {
    pub(crate) hash: String,
    pub(crate) tipo: String,
    pub(crate) bytes: u32,
    /// `data:<tipo>;base64,…`; vazio quando o arquivo sumiu do disco (dito no
    /// registro, e a tela desenha sem ela).
    pub(crate) url: String,
}

/// **O padrão como a tela o lê** (`perfil_padrao`): os textos, a revisão e as
/// imagens prontas para uma `<img>`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PerfilPadraoNaTela {
    pub(crate) revisao: u64,
    pub(crate) nome_exibido: String,
    pub(crate) pronomes: String,
    pub(crate) status: String,
    pub(crate) bio: String,
    pub(crate) cor: String,
    pub(crate) efeito: Efeito,
    pub(crate) foto: Option<ImagemDoPadraoNaTela>,
    pub(crate) capa: Option<ImagemDoPadraoNaTela>,
}

/// **O que esta máquina sabe de um servidor e do padrão.** Sem registro, o
/// servidor **não** segue o padrão: um perfil que já existia lá (de antes desta
/// versão, de outro aparelho, da migração do MOD) nunca é sobrescrito. Só a
/// tela, ao aplicar o padrão a um servidor sem perfil, liga o `segue`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Seguimento {
    pub(crate) segue: bool,
    /// A revisão do padrão aplicada por último; zero é «aplicar de novo».
    pub(crate) revisao_aplicada: u64,
    /// A assinatura do perfil que a aplicação deixou no servidor (os textos e
    /// os hashes das imagens). Um perfil que não bate com ela foi mudado por
    /// outro caminho (outro aparelho), e deixa de seguir. `None`: não conferir.
    pub(crate) assinatura: Option<String>,
}

/// O que um comando do padrão recusa: o erro do perfil (a mesma regra do
/// servidor), o da conexão, ou o disco.
///
/// `untagged`, como o `FalhaNoPerfil`: a tela lê `"Heic"`, `{"Campos": …}`,
/// `"NotConnected"` ou `"NaoGravei"` sem um nível a mais.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(untagged)]
pub(crate) enum FalhaNoPadrao {
    Perfil(ErroDoPerfil),
    Conexao(seele_ffi::ConnectionError),
    Disco(DoDisco),
}

/// O disco recusou: a escolha não ficou, e a tela diz isso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum DoDisco {
    NaoGravei,
}

impl From<ErroDoPerfil> for FalhaNoPadrao {
    fn from(erro: ErroDoPerfil) -> Self {
        Self::Perfil(erro)
    }
}

impl From<seele_ffi::ConnectionError> for FalhaNoPadrao {
    fn from(erro: seele_ffi::ConnectionError) -> Self {
        Self::Conexao(erro)
    }
}

impl From<seele_ffi::perfis::FalhaNoPerfil> for FalhaNoPadrao {
    fn from(falha: seele_ffi::perfis::FalhaNoPerfil) -> Self {
        match falha {
            seele_ffi::perfis::FalhaNoPerfil::Perfil(erro) => Self::Perfil(erro),
            seele_ffi::perfis::FalhaNoPerfil::Conexao(erro) => Self::Conexao(erro),
        }
    }
}

const NAO_GRAVEI: FalhaNoPadrao = FalhaNoPadrao::Disco(DoDisco::NaoGravei);

/// O SHA-256 em hexadecimal minúsculo, como o `perfis` o entrega.
fn hex(hash: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut texto = String::with_capacity(64);
    for byte in hash {
        let _ = write!(texto, "{byte:02x}");
    }
    texto
}

/// **A pasta do padrão.** Tudo o que se lê e grava passa por aqui; os
/// comandos (`main.rs`) só põem o seletor e a sessão em volta.
#[derive(Debug, Clone)]
pub(crate) struct Armazem {
    pasta: PathBuf,
}

impl Armazem {
    pub(crate) fn em(pasta: impl Into<PathBuf>) -> Self {
        Self {
            pasta: pasta.into(),
        }
    }

    fn arquivo_do_padrao(&self) -> PathBuf {
        self.pasta.join("padrao.json")
    }

    fn arquivo_dos_servidores(&self) -> PathBuf {
        self.pasta.join("servidores.json")
    }

    fn pasta_das_imagens(&self) -> PathBuf {
        self.pasta.join("imagens")
    }

    /// Onde mora a imagem de um hash. Só hashes de 64 hexadecimais viram
    /// caminho: o nome do arquivo nunca vem de fora sem passar por aqui.
    fn arquivo_da_imagem(&self, hash: &str) -> Option<PathBuf> {
        (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| self.pasta_das_imagens().join(hash.to_ascii_lowercase()))
    }

    /// Grava `bytes` em `destino` por um `.em-obras-*` e um `rename`.
    fn gravar_inteiro(destino: &Path, bytes: &[u8]) -> Result<(), FalhaNoPadrao> {
        let Some(pasta) = destino.parent() else {
            return Err(NAO_GRAVEI);
        };
        let nome = destino
            .file_name()
            .map(|nome| nome.to_string_lossy().into_owned())
            .unwrap_or_default();
        let obras = pasta.join(format!(".em-obras-{nome}"));
        let feito = std::fs::create_dir_all(pasta)
            .and_then(|()| std::fs::write(&obras, bytes))
            .and_then(|()| std::fs::rename(&obras, destino));
        feito.map_err(|erro| {
            tracing::warn!(%erro, arquivo = %destino.display(), "não gravei o perfil padrão nesta máquina");
            let _ = std::fs::remove_file(&obras);
            NAO_GRAVEI
        })
    }

    /// **O padrão guardado**, ou o vazio (revisão zero) quando não há. Um
    /// arquivo ilegível é dito no registro e lido como vazio: a tela inicial
    /// abre, e salvar de novo o conserta.
    pub(crate) fn ler(&self) -> PerfilPadrao {
        match std::fs::read(self.arquivo_do_padrao()) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|erro| {
                tracing::warn!(%erro, "o perfil padrão desta máquina não se lê; abro vazio");
                PerfilPadrao::default()
            }),
            Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => PerfilPadrao::default(),
            Err(erro) => {
                tracing::warn!(%erro, "não li o perfil padrão desta máquina; abro vazio");
                PerfilPadrao::default()
            }
        }
    }

    fn guardar(&self, padrao: &PerfilPadrao) -> Result<(), FalhaNoPadrao> {
        let bytes = serde_json::to_vec_pretty(padrao).map_err(|_| NAO_GRAVEI)?;
        Self::gravar_inteiro(&self.arquivo_do_padrao(), &bytes)
    }

    /// **Grava os textos**, validados pela regra do servidor. A revisão sobe só
    /// quando algo mudou: salvar o mesmo não manda o padrão de novo a todos os
    /// servidores que o seguem.
    ///
    /// # Errors
    ///
    /// `Campos` (por campo, nada gravado) ou `NaoGravei`.
    pub(crate) fn gravar_textos(
        &self,
        entrada: &EntradaDoPerfil,
    ) -> Result<PerfilPadrao, FalhaNoPadrao> {
        let validada = entrada_validada(entrada)?;
        let mut padrao = self.ler();
        let mesmo = padrao.nome_exibido == validada.nome_exibido
            && padrao.pronomes == validada.pronomes
            && padrao.status == validada.status
            && padrao.bio == validada.bio
            && padrao.cor == validada.cor
            && padrao.efeito == validada.efeito;
        if mesmo && padrao.revisao > 0 {
            return Ok(padrao);
        }
        padrao.nome_exibido = validada.nome_exibido;
        padrao.pronomes = validada.pronomes;
        padrao.status = validada.status;
        padrao.bio = validada.bio;
        padrao.cor = validada.cor;
        padrao.efeito = validada.efeito;
        padrao.revisao = padrao.revisao.saturating_add(1);
        self.guardar(&padrao)?;
        Ok(padrao)
    }

    /// **Guarda a foto ou a capa**, conferida como num envio (até 10 MiB, o
    /// tipo pelos bytes, o HEIC nomeado), pelo hash. A imagem que sai é
    /// apagada quando o outro lado não a usa.
    ///
    /// # Errors
    ///
    /// Os de `conferir_imagem`, ou `NaoGravei`.
    pub(crate) fn guardar_imagem(
        &self,
        lado: LadoDoPerfil,
        bytes: &[u8],
    ) -> Result<PerfilPadrao, FalhaNoPadrao> {
        let tipo = conferir_imagem(bytes)?;
        let hash = hex(&hash_da_imagem(bytes));
        let Some(destino) = self.arquivo_da_imagem(&hash) else {
            return Err(NAO_GRAVEI);
        };
        if !destino.exists() {
            Self::gravar_inteiro(&destino, bytes)?;
        }
        let mut padrao = self.ler();
        let antes = padrao.imagem(lado).cloned();
        *padrao.imagem_mut(lado) = Some(ImagemDoPadrao {
            hash: hash.clone(),
            tipo: tipo_de_midia(tipo).to_owned(),
            bytes: u32::try_from(bytes.len()).map_err(|_| NAO_GRAVEI)?,
        });
        padrao.revisao = padrao.revisao.saturating_add(1);
        self.guardar(&padrao)?;
        if let Some(antes) = antes.filter(|antes| antes.hash != hash) {
            self.soltar_se_orfa(&padrao, &antes.hash);
        }
        Ok(padrao)
    }

    /// **Tira a foto ou a capa** do padrão. Tirar o que não há não sobe a
    /// revisão.
    ///
    /// # Errors
    ///
    /// `NaoGravei`.
    pub(crate) fn tirar_imagem(&self, lado: LadoDoPerfil) -> Result<PerfilPadrao, FalhaNoPadrao> {
        let mut padrao = self.ler();
        let Some(antes) = padrao.imagem_mut(lado).take() else {
            return Ok(padrao);
        };
        padrao.revisao = padrao.revisao.saturating_add(1);
        self.guardar(&padrao)?;
        self.soltar_se_orfa(&padrao, &antes.hash);
        Ok(padrao)
    }

    /// Apaga a imagem de `hash` se nenhum lado do padrão a usa mais.
    fn soltar_se_orfa(&self, padrao: &PerfilPadrao, hash: &str) {
        let usada = [&padrao.foto, &padrao.capa]
            .iter()
            .any(|imagem| imagem.as_ref().is_some_and(|i| i.hash == hash));
        if usada {
            return;
        }
        if let Some(arquivo) = self.arquivo_da_imagem(hash) {
            if let Err(erro) = std::fs::remove_file(&arquivo) {
                if erro.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(%erro, "não apaguei uma imagem velha do perfil padrão");
                }
            }
        }
    }

    /// **Os bytes da imagem de um lado**, quando o hash pedido ainda é o dela.
    /// Os bytes são conferidos contra o hash: um arquivo mexido no disco não
    /// sobe como se fosse a foto.
    ///
    /// # Errors
    ///
    /// `Mudou` (o padrão trocou a imagem, ou ela não está mais no disco).
    pub(crate) fn bytes_da_imagem(
        &self,
        lado: LadoDoPerfil,
        hash: &str,
    ) -> Result<Vec<u8>, FalhaNoPadrao> {
        let padrao = self.ler();
        let Some(imagem) = padrao
            .imagem(lado)
            .filter(|i| i.hash.eq_ignore_ascii_case(hash))
        else {
            return Err(ErroDoPerfil::Mudou.into());
        };
        let arquivo = self
            .arquivo_da_imagem(&imagem.hash)
            .ok_or(ErroDoPerfil::Mudou)?;
        let bytes = std::fs::read(&arquivo).map_err(|erro| {
            tracing::warn!(%erro, "a imagem do perfil padrão sumiu do disco");
            ErroDoPerfil::Mudou
        })?;
        if hex(&hash_da_imagem(&bytes)) != imagem.hash {
            tracing::warn!("a imagem do perfil padrão no disco não tem o hash guardado");
            return Err(ErroDoPerfil::Mudou.into());
        }
        Ok(bytes)
    }

    /// **O padrão como a tela o lê**, com as imagens em `data:`.
    pub(crate) fn na_tela(&self) -> PerfilPadraoNaTela {
        let padrao = self.ler();
        let imagem = |lado: LadoDoPerfil| {
            padrao.imagem(lado).map(|imagem| {
                let url = self
                    .bytes_da_imagem(lado, &imagem.hash)
                    .map(|bytes| {
                        format!(
                            "data:{};base64,{}",
                            imagem.tipo,
                            seele_ffi::base64_de(&bytes)
                        )
                    })
                    .unwrap_or_default();
                ImagemDoPadraoNaTela {
                    hash: imagem.hash.clone(),
                    tipo: imagem.tipo.clone(),
                    bytes: imagem.bytes,
                    url,
                }
            })
        };
        PerfilPadraoNaTela {
            foto: imagem(LadoDoPerfil::Foto),
            capa: imagem(LadoDoPerfil::Capa),
            revisao: padrao.revisao,
            nome_exibido: padrao.nome_exibido,
            pronomes: padrao.pronomes,
            status: padrao.status,
            bio: padrao.bio,
            cor: padrao.cor,
            efeito: padrao.efeito,
        }
    }

    fn servidores(&self) -> BTreeMap<String, Seguimento> {
        std::fs::read(self.arquivo_dos_servidores())
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice(&bytes)
                    .map_err(|erro| {
                        tracing::warn!(%erro, "a lista de servidores do perfil padrão não se lê");
                    })
                    .ok()
            })
            .unwrap_or_default()
    }

    /// O que esta máquina sabe de `servidor` (a impressão dele). Sem registro,
    /// não segue.
    pub(crate) fn seguimento(&self, servidor: &str) -> Seguimento {
        self.servidores().remove(servidor).unwrap_or_default()
    }

    /// Grava o que esta máquina sabe de `servidor`.
    ///
    /// # Errors
    ///
    /// `NaoGravei`.
    pub(crate) fn guardar_seguimento(
        &self,
        servidor: &str,
        seguimento: Seguimento,
    ) -> Result<(), FalhaNoPadrao> {
        if servidor.is_empty() {
            return Err(NAO_GRAVEI);
        }
        let mut todos = self.servidores();
        todos.insert(servidor.to_owned(), seguimento);
        let bytes = serde_json::to_vec_pretty(&todos).map_err(|_| NAO_GRAVEI)?;
        Self::gravar_inteiro(&self.arquivo_dos_servidores(), &bytes)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];
    const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";

    fn pasta() -> PathBuf {
        let pasta = std::env::temp_dir().join(format!(
            "seele-perfil-padrao-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&pasta).unwrap();
        pasta
    }

    fn entrada() -> EntradaDoPerfil {
        EntradaDoPerfil {
            nome_exibido: "  Maya ".to_owned(),
            pronomes: "ela/dela".to_owned(),
            status: "no ar".to_owned(),
            bio: "um\ndois".to_owned(),
            cor: "#B48CFF".to_owned(),
            efeito: Efeito::Brilho,
        }
    }

    /// **O padrão faz a volta pelo disco**: os textos validados (aparados, a
    /// cor em minúsculas), a revisão, as duas imagens pelo hash, e um armazém
    /// novo na mesma pasta lê o mesmo. Sem nada salvo, a revisão é zero.
    #[test]
    fn o_padrao_faz_a_volta_pelo_disco() {
        let pasta = pasta();
        let armazem = Armazem::em(&pasta);
        assert_eq!(armazem.ler(), PerfilPadrao::default());
        assert_eq!(
            armazem.na_tela().revisao,
            0,
            "um padrão nunca salvo não é aplicado"
        );

        let gravado = armazem.gravar_textos(&entrada()).unwrap();
        assert_eq!(gravado.nome_exibido, "Maya");
        assert_eq!(gravado.cor, "#b48cff");
        assert_eq!(gravado.revisao, 1);
        armazem.guardar_imagem(LadoDoPerfil::Foto, PNG).unwrap();
        let com_capa = armazem.guardar_imagem(LadoDoPerfil::Capa, GIF).unwrap();
        assert_eq!(com_capa.revisao, 3);

        let relido = Armazem::em(&pasta).na_tela();
        assert_eq!(relido.nome_exibido, "Maya");
        assert_eq!(relido.bio, "um\ndois");
        assert_eq!(relido.efeito, Efeito::Brilho);
        let foto = relido.foto.unwrap();
        assert_eq!(
            foto.hash,
            hex(&hash_da_imagem(PNG)),
            "o endereço não é o do servidor"
        );
        assert_eq!(foto.tipo, "image/png");
        assert!(
            foto.url.starts_with("data:image/png;base64,"),
            "{}",
            foto.url
        );
        assert_eq!(relido.capa.unwrap().tipo, "image/gif");
        assert!(pasta.join("imagens").join(&foto.hash).exists());

        // Salvar o mesmo não sobe a revisão: os servidores que seguem não
        // recebem o padrão de novo por nada.
        assert_eq!(armazem.gravar_textos(&entrada()).unwrap().revisao, 3);
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// **A regra é a do servidor**: um campo longo volta por campo e nada é
    /// gravado; HEIC, vazio e acima de 10 MiB são recusados antes do disco.
    #[test]
    fn o_padrao_recusa_o_que_o_servidor_recusaria() {
        let pasta = pasta();
        let armazem = Armazem::em(&pasta);
        let mut longa = entrada();
        longa.nome_exibido = "n".repeat(41);
        assert!(matches!(
            armazem.gravar_textos(&longa),
            Err(FalhaNoPadrao::Perfil(ErroDoPerfil::Campos { .. }))
        ));
        assert_eq!(armazem.ler().revisao, 0, "o campo recusado foi gravado");

        let heic = include_bytes!("../../../crates/seele-proto/tests/vetores/heic-8x8.heic");
        assert_eq!(
            armazem.guardar_imagem(LadoDoPerfil::Foto, heic),
            Err(FalhaNoPadrao::Perfil(ErroDoPerfil::Heic))
        );
        assert_eq!(
            armazem.guardar_imagem(LadoDoPerfil::Foto, &[]),
            Err(FalhaNoPadrao::Perfil(ErroDoPerfil::ImagemVazia))
        );
        let mut grande = PNG.to_vec();
        grande.resize(seele_ffi::perfis::TETO_DA_IMAGEM + 1, 0);
        assert!(matches!(
            armazem.guardar_imagem(LadoDoPerfil::Capa, &grande),
            Err(FalhaNoPadrao::Perfil(
                ErroDoPerfil::ImagemGrandeDemais { .. }
            ))
        ));
        assert!(
            !pasta.join("imagens").exists(),
            "uma imagem recusada foi ao disco"
        );
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// Trocar ou tirar uma imagem apaga a velha quando o outro lado não a usa;
    /// pedir os bytes de um hash que não é mais o atual volta `Mudou`.
    #[test]
    fn a_imagem_velha_sai_e_o_hash_velho_e_mudou() {
        let pasta = pasta();
        let armazem = Armazem::em(&pasta);
        let velha = hex(&hash_da_imagem(PNG));
        armazem.guardar_imagem(LadoDoPerfil::Foto, PNG).unwrap();
        // A mesma imagem nos dois lados: tirar de um não a apaga.
        armazem.guardar_imagem(LadoDoPerfil::Capa, PNG).unwrap();
        armazem.tirar_imagem(LadoDoPerfil::Capa).unwrap();
        assert!(
            pasta.join("imagens").join(&velha).exists(),
            "apagou a foto que ainda vale"
        );
        assert_eq!(
            armazem.bytes_da_imagem(LadoDoPerfil::Foto, &velha).unwrap(),
            PNG
        );

        armazem.guardar_imagem(LadoDoPerfil::Foto, GIF).unwrap();
        assert!(
            !pasta.join("imagens").join(&velha).exists(),
            "a foto velha ficou no disco"
        );
        assert_eq!(
            armazem.bytes_da_imagem(LadoDoPerfil::Foto, &velha),
            Err(FalhaNoPadrao::Perfil(ErroDoPerfil::Mudou))
        );
        let revisao = armazem.ler().revisao;
        armazem.tirar_imagem(LadoDoPerfil::Capa).unwrap();
        assert_eq!(
            armazem.ler().revisao,
            revisao,
            "tirar o que não há subiu a revisão"
        );
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// **Sem registro, um servidor não segue o padrão** — é o que impede a
    /// tela de sobrescrever um perfil que já existia lá. O registro gravado
    /// volta igual, por impressão, e um servidor não mexe no outro.
    #[test]
    fn o_seguimento_e_por_servidor_e_comeca_sem_seguir() {
        let pasta = pasta();
        let armazem = Armazem::em(&pasta);
        assert_eq!(armazem.seguimento("aa"), Seguimento::default());
        assert!(!armazem.seguimento("aa").segue);
        let segue = Seguimento {
            segue: true,
            revisao_aplicada: 4,
            assinatura: Some("x".to_owned()),
        };
        armazem.guardar_seguimento("aa", segue.clone()).unwrap();
        armazem
            .guardar_seguimento("bb", Seguimento::default())
            .unwrap();
        assert_eq!(Armazem::em(&pasta).seguimento("aa"), segue);
        assert!(!Armazem::em(&pasta).seguimento("bb").segue);
        assert_eq!(armazem.guardar_seguimento("", segue), Err(NAO_GRAVEI));
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
