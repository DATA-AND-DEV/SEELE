//! O módulo binário do Cisco: qual arquivo, de onde, com que hash — e onde
//! procurá-lo antes de perguntar a alguém se pode baixar.
//!
//! # Por que ele não vem no binário
//!
//! É licença, e não preguiça de empacotar. O código do OpenH264 é BSD-2-Clause,
//! mas isso não é o que resolve H.264: o que resolve é o Cisco pagar os
//! royalties do pool **pelos binários que o Cisco distribui**. Compilar do
//! fonte e embutir o resultado descarta essa cobertura; redistribuir o binário
//! do Cisco dentro do nosso `.dmg` ou do nosso instalador NSIS nos põe como
//! distribuidor, e a cobertura não vem junto. Carregar o binário do Cisco em
//! tempo de execução é o mecanismo, e é o que o Firefox faz desde 2013 (§2).
//!
//! Consequências, todas obrigatórias e todas do §2:
//!
//! - uma busca de ~1 MB, **uma vez, com consentimento na tela**. Num produto
//!   cujo argumento é que nada sai da sua máquina, isto tem de ser dito na cara;
//! - **hash fixado e conferido**, com a postura do ADR 0026;
//! - um motivo enumerado — [`ErroDeVideo::ModuloDeVideoAusente`] —, porque isto
//!   é estado normal e não erro de rede.
//!
//! **Este módulo não baixa.** Ele descreve o que baixar e confere o que chegou.
//! Quem baixa é a casca, depois de perguntar, com a máquina de
//! baixar-e-verificar que o produto já tem.
//!
//! # Os nomes, conferidos e não deduzidos
//!
//! O do Windows **não** tem o `lib` na frente e o do macOS tem. Não há regra a
//! deduzir daí: são os nomes que o Cisco publica, um a um, e o jeito de saber é
//! olhar. Foi assim que `spikes/tela-no-codec` achou o de macOS, e é por isso
//! que estão escritos aqui como constantes em vez de montados por `format!`.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use shiguredo_openh264::{Error as ErroOpenh264, Openh264Library};

use crate::erro::ErroDeVideo;

/// De onde o Cisco publica, e é o mesmo endereço de onde o Firefox busca o dele.
pub const ORIGEM: &str = "https://ciscobinary.openh264.org/";

/// A versão do OpenH264 com que as bindings deste build foram geradas.
///
/// Não é escolha nossa: é a que o `Cargo.toml` do `shiguredo_openh264` fixa, e
/// carregar outra é [`ErroDeVideo::ModuloDeVideoDeOutraVersao`]. Está aqui
/// escrita porque ela entra no **nome do arquivo** publicado, e nome de arquivo
/// não se deduz de uma constante de outro crate sem alguém conferir.
pub const VERSAO: &str = "2.6.0";

/// O que buscar, e como saber que é ele.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuloPublicado {
    /// O nome do arquivo comprimido, como o Cisco o publica.
    pub arquivo: &'static str,
    /// Como o arquivo se chama depois de expandido, em disco.
    ///
    /// É por este nome que [`procurar_em`] o encontra.
    pub nome_em_disco: &'static str,
    /// Tamanho do comprimido, em bytes. Medido, não estimado.
    pub bytes_comprimido: u64,
    /// sha256 do comprimido, hexadecimal minúsculo.
    pub sha256_comprimido: &'static str,
    /// Tamanho do expandido, em bytes.
    pub bytes_expandido: u64,
    /// sha256 do expandido, hexadecimal minúsculo.
    ///
    /// **É este que importa**, porque é o arquivo que fica e que se carrega. O
    /// do comprimido serve para recusar um download estragado antes de gastar
    /// CPU descomprimindo.
    pub sha256_expandido: &'static str,
}

impl ModuloPublicado {
    /// O endereço completo de onde buscá-lo.
    #[must_use]
    pub fn url(&self) -> String {
        format!("{ORIGEM}{}", self.arquivo)
    }

    /// Confere os bytes comprimidos, como chegam da rede.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::ModuloDeVideoCorrompido`] se o hash não bate. Quase sempre
    /// isso não é corrupção: é uma página de erro de proxy no lugar do arquivo,
    /// e o campo `bytes` é o que deixa isso óbvio.
    pub fn conferir_comprimido(&self, bytes: &[u8]) -> Result<(), ErroDeVideo> {
        conferir(self.sha256_comprimido, bytes)
    }

    /// Confere os bytes expandidos, que são os que ficam em disco.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::ModuloDeVideoCorrompido`] se o hash não bate.
    pub fn conferir_expandido(&self, bytes: &[u8]) -> Result<(), ErroDeVideo> {
        conferir(self.sha256_expandido, bytes)
    }
}

fn conferir(esperado: &'static str, bytes: &[u8]) -> Result<(), ErroDeVideo> {
    let encontrado = hexa(&Sha256::digest(bytes));
    if encontrado == esperado {
        return Ok(());
    }
    Err(ErroDeVideo::ModuloDeVideoCorrompido {
        esperado,
        encontrado,
        bytes: bytes.len(),
    })
}

/// Sem o crate `hex`, que entraria na árvore só para isto.
fn hexa(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut texto, byte| {
        // `write!` numa `String` não falha; o `Result` existe só pela assinatura
        // do trait, e engoli-lo aqui é o que evita um `unwrap` na produção.
        let _ = write!(texto, "{byte:02x}");
        texto
    })
}

/// macOS em Apple Silicon.
///
/// Hashes e tamanhos medidos: `spikes/tela-no-codec/README.md` registra os
/// mesmos números, e este arquivo foi conferido de novo contra o que está em
/// disco nesta máquina.
pub const MACOS_ARM64: ModuloPublicado = ModuloPublicado {
    arquivo: "libopenh264-2.6.0-mac-arm64.dylib.bz2",
    nome_em_disco: "libopenh264.dylib",
    bytes_comprimido: 482_124,
    sha256_comprimido: "6db362ee5abdab572311aeadb96d3f44b0617d9a4a4b9f4db4cb5ac4d968da71",
    bytes_expandido: 1_207_136,
    sha256_expandido: "052e98bfcf7a9167d22f3bbb3f5988ef79065591f36af8b52924b22b13624551",
};

/// Windows x86-64.
///
/// **Sem o `lib` na frente**, ao contrário do de macOS. Os quatro números foram
/// medidos baixando o arquivo e conferindo — o `file(1)` diz
/// «PE32+ executable (DLL) x86-64».
pub const WINDOWS_X64: ModuloPublicado = ModuloPublicado {
    arquivo: "openh264-2.6.0-win64.dll.bz2",
    nome_em_disco: "openh264.dll",
    bytes_comprimido: 452_053,
    sha256_comprimido: "dab5f2a872777f9a58b69bfa9fbcf20d9f82f2d6ec91383fd70bff49bd34ac9f",
    bytes_expandido: 978_520,
    sha256_expandido: "2076cb5675ec6c1a4c70e7a2a322552f547b6eeed649d6dfcd9e02a543b24691",
};

/// O que este build precisa buscar, se é que existe para ele.
///
/// `None` nos alvos em que o recurso não sai na v1 ou em que ninguém conferiu o
/// arquivo publicado. **Linux é `None` de propósito**: a v1 sai com macOS e
/// Windows, por decisão de 22/08/2026 (§7 item 5). macOS em Intel também é
/// `None`, e por um motivo diferente — o arquivo existe, e ninguém deste lado
/// mediu o hash dele; fixar um número que não se conferiu seria pior que não
/// oferecer.
#[must_use]
pub const fn publicado_para_este_sistema() -> Option<ModuloPublicado> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Some(MACOS_ARM64)
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Some(WINDOWS_X64)
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    )))]
    {
        None
    }
}

/// Procura o módulo nas pastas dadas, na ordem, e devolve a primeira que o tem.
///
/// **Não baixa e não cria pasta.** Só olha.
///
/// # Por que as pastas vêm de fora
///
/// Porque onde os arquivos do produto moram é decisão da casca, não desta
/// biblioteca — é o mesmo desenho de `seele_core::conhecidos` e das
/// preferências, que também recebem o caminho pronto. Uma biblioteca que
/// adivinha `~/Library/Application Support` não é testável sem mexer no `$HOME`
/// de quem roda o teste, e passa a ter opinião sobre uma coisa que não é dela.
/// Uma variável de ambiente para quem desenvolve apontar o seu módulo também é
/// de quem chama: basta pôr a pasta na frente da lista.
///
/// # Errors
///
/// [`ErroDeVideo::SistemaSemModuloPublicado`] se não há módulo para este alvo —
/// e aí não há botão de baixar para oferecer.
///
/// [`ErroDeVideo::ModuloDeVideoAusente`] se há, e ele não está em nenhuma das
/// pastas. Este é o estado normal de quem nunca compartilhou tela.
pub fn procurar_em(pastas: &[PathBuf]) -> Result<PathBuf, ErroDeVideo> {
    let Some(modulo) = publicado_para_este_sistema() else {
        return Err(ErroDeVideo::SistemaSemModuloPublicado {
            sistema: std::env::consts::OS,
            arquitetura: std::env::consts::ARCH,
        });
    };

    for pasta in pastas {
        let candidato = pasta.join(modulo.nome_em_disco);
        if candidato.is_file() {
            return Ok(candidato);
        }
    }

    Err(ErroDeVideo::ModuloDeVideoAusente {
        procurado_em: pastas.to_vec(),
    })
}

/// O módulo do Cisco, carregado e pronto para dar encoders e decoders.
///
/// É barato de clonar (a biblioteca por baixo é contada por referência) e é
/// `Send`, que é o que permite ao §2 pôr o encoder numa thread própria.
#[derive(Debug, Clone)]
pub struct BibliotecaDeVideo {
    pub(crate) lib: Openh264Library,
}

impl BibliotecaDeVideo {
    /// Carrega o módulo que está neste caminho.
    ///
    /// O `dlopen`/`LoadLibraryW` acontece aqui, e é onde o §2 disse que
    /// aconteceria. No macOS ele funcionou sem assinatura, sem quarentena e sem
    /// `Entitlements` — medido em `spikes/tela-no-codec`.
    ///
    /// **Não confere o hash.** Conferir é de quem baixou, com
    /// [`ModuloPublicado::conferir_expandido`], e é lá que a recusa tem
    /// conserto: aqui já não há de onde buscar de novo.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::ModuloDeVideoDeOutraVersao`] se o módulo em disco é de
    /// outra versão do OpenH264 — a tabela virtual seria outra, e chamá-la
    /// escreveria memória alheia. [`ErroDeVideo::ModuloDeVideoIlegivel`] em
    /// qualquer outra recusa do sistema.
    pub fn carregar(caminho: &Path) -> Result<Self, ErroDeVideo> {
        match Openh264Library::load(caminho) {
            Ok(lib) => Ok(Self { lib }),
            Err(ErroOpenh264::VersionMismatch {
                build_version,
                runtime_version,
            }) => Err(ErroDeVideo::ModuloDeVideoDeOutraVersao {
                esperada: build_version.to_owned(),
                encontrada: runtime_version,
            }),
            Err(outro) => Err(ErroDeVideo::ModuloDeVideoIlegivel {
                caminho: caminho.to_path_buf(),
                motivo: outro.to_string(),
            }),
        }
    }

    /// Procura nas pastas dadas e carrega o que achar.
    ///
    /// # Errors
    ///
    /// O que [`procurar_em`] e [`BibliotecaDeVideo::carregar`] devolvem.
    pub fn procurar_e_carregar(pastas: &[PathBuf]) -> Result<Self, ErroDeVideo> {
        Self::carregar(&procurar_em(pastas)?)
    }

    /// De onde ele foi carregado.
    #[must_use]
    pub fn caminho(&self) -> &Path {
        self.lib.path()
    }

    /// A versão que o módulo em disco diz ter, como `v2.6.0`.
    #[must_use]
    pub fn versao(&self) -> String {
        self.lib.runtime_version()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_nomes_publicados_sao_os_que_alguem_conferiu() {
        // **Não são deduzíveis.** O do macOS tem `lib` na frente e o do Windows
        // não; quem montar estes nomes com um `format!` a partir do alvo vai
        // acertar um e errar o outro, e o erro só aparece na máquina do outro
        // sistema, no clique de quem quis compartilhar. Este teste é a única
        // guarda contra alguém «simplificar» isso.
        assert_eq!(MACOS_ARM64.arquivo, "libopenh264-2.6.0-mac-arm64.dylib.bz2");
        assert_eq!(WINDOWS_X64.arquivo, "openh264-2.6.0-win64.dll.bz2");
        assert!(
            !WINDOWS_X64.arquivo.starts_with("lib"),
            "o arquivo do Windows não tem o «lib» na frente"
        );

        // E os dois carregam a versão de que as bindings saíram: um nome de
        // arquivo de outra versão baixaria um módulo que `carregar` recusa
        // depois, com o download já gasto.
        assert!(MACOS_ARM64.arquivo.contains(VERSAO));
        assert!(WINDOWS_X64.arquivo.contains(VERSAO));
        assert_eq!(format!("v{VERSAO}"), shiguredo_openh264::BUILD_VERSION);
    }

    #[test]
    fn a_origem_e_a_do_cisco_e_por_https() {
        // O endereço faz parte do que a tela de consentimento mostra: quem vai
        // baixar lê de onde vem. Trocá-lo é uma decisão de produto, não um
        // detalhe de constante.
        assert_eq!(
            MACOS_ARM64.url(),
            format!("{ORIGEM}{}", MACOS_ARM64.arquivo)
        );
        assert!(ORIGEM.starts_with("https://ciscobinary.openh264.org/"));
    }

    #[test]
    fn o_hash_fixado_recusa_o_que_nao_e_o_modulo() {
        // O caso de campo não é corrupção de bits: é um proxy devolvendo uma
        // página de erro com 200. Por isso o motivo carrega o tamanho.
        let pagina = b"<html>403 Forbidden</html>";
        let erro = MACOS_ARM64
            .conferir_expandido(pagina)
            .expect_err("uma página de erro não pode passar por módulo");

        match erro {
            ErroDeVideo::ModuloDeVideoCorrompido {
                esperado, bytes, ..
            } => {
                assert_eq!(esperado, MACOS_ARM64.sha256_expandido);
                assert_eq!(bytes, pagina.len());
            }
            outro => panic!("motivo errado: {outro:?}"),
        }
    }

    #[test]
    fn o_hexadecimal_tem_sessenta_e_quatro_digitos_minusculos() {
        // Um `{:x}` que engula o zero à esquerda faria o hash bater quase
        // sempre e falhar em 1 de 256 bytes — o pior defeito possível numa
        // conferência de integridade, porque passa nos testes de alguém.
        let texto = hexa(&Sha256::digest(b""));
        assert_eq!(texto.len(), 64);
        assert_eq!(
            texto,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(hexa(&[0x00, 0x0f, 0xff]), "000fff");
    }

    fn pasta_de_rascunho(nome: &str) -> PathBuf {
        let mut caminho = std::env::temp_dir();
        caminho.push(format!("seele-video-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&caminho);
        std::fs::create_dir_all(&caminho).expect("criar a pasta de rascunho");
        caminho
    }

    #[test]
    fn sem_o_modulo_o_motivo_diz_onde_se_procurou() {
        let Some(_) = publicado_para_este_sistema() else {
            // Num alvo sem módulo publicado o outro motivo é o certo, e é o que
            // o teste seguinte cobre.
            return;
        };
        let vazia = pasta_de_rascunho("ausente");
        let outra = vazia.join("nem-existe");

        let erro = procurar_em(&[vazia.clone(), outra.clone()])
            .expect_err("não há módulo nenhum nessas pastas");

        assert_eq!(
            erro,
            ErroDeVideo::ModuloDeVideoAusente {
                procurado_em: vec![vazia, outra]
            },
            "a lista de onde se procurou é a primeira pergunta de quem depura"
        );
    }

    #[test]
    fn achar_e_achar_o_primeiro_que_existe() {
        let Some(modulo) = publicado_para_este_sistema() else {
            return;
        };
        let primeira = pasta_de_rascunho("primeira");
        let segunda = pasta_de_rascunho("segunda");
        // Os dois existem: a ordem tem de decidir, e não o acaso do sistema de
        // arquivos. É o que permite à casca pôr a pasta de quem desenvolve na
        // frente da pasta do produto.
        std::fs::write(primeira.join(modulo.nome_em_disco), b"nao importa").expect("escrever");
        std::fs::write(segunda.join(modulo.nome_em_disco), b"nao importa").expect("escrever");

        let achado = procurar_em(&[primeira.clone(), segunda]).expect("achar");
        assert_eq!(achado, primeira.join(modulo.nome_em_disco));
    }

    #[test]
    fn uma_pasta_com_o_nome_do_modulo_nao_conta_como_modulo() {
        // `exists()` diria que sim. Uma pasta chamada `libopenh264.dylib` faria
        // a busca parar ali e o `dlopen` falhar depois, com a mensagem errada:
        // o produto diria «não consegui carregar» quando a verdade é «não está
        // aqui, ofereça a busca».
        let Some(modulo) = publicado_para_este_sistema() else {
            return;
        };
        let pasta = pasta_de_rascunho("armadilha");
        std::fs::create_dir(pasta.join(modulo.nome_em_disco)).expect("criar a armadilha");

        assert!(matches!(
            procurar_em(&[pasta]),
            Err(ErroDeVideo::ModuloDeVideoAusente { .. })
        ));
    }

    #[test]
    fn um_arquivo_que_nao_e_biblioteca_e_ilegivel_e_nao_derruba_o_processo() {
        let pasta = pasta_de_rascunho("lixo");
        let caminho = pasta.join("lixo.bin");
        std::fs::write(&caminho, b"isto nao e uma biblioteca").expect("escrever");

        match BibliotecaDeVideo::carregar(&caminho) {
            Err(ErroDeVideo::ModuloDeVideoIlegivel { caminho: onde, .. }) => {
                assert_eq!(onde, caminho);
            }
            outro => panic!("esperava ModuloDeVideoIlegivel, veio {outro:?}"),
        }
    }
}
