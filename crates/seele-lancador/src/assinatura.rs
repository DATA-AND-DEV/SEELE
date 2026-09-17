//! A conferência de assinatura, que é a do ADR 0026 e não uma segunda.
//!
//! O `tauri-plugin-updater` confere o pacote com `minisign-verify`, contra a
//! chave pública que está em `apps/seele-app/tauri.conf.json`, no campo
//! `plugins.updater.pubkey`. **Este módulo faz exatamente isso, com a mesma
//! biblioteca e os mesmos formatos**, e essa é a razão de ele ser curto: um
//! pacote aceito aqui é o mesmo que seria aceito lá.
//!
//! Escrever a conferência de novo — em vez de reusar a biblioteca — seria duas
//! implementações de «o que conta como assinado», e a segunda diverge da
//! primeira num detalhe que ninguém revisa.
//!
//! # Os dois formatos, e por que os dois são base64
//!
//! O minisign trabalha com dois arquivos de texto: a chave pública (`.pub`) e a
//! assinatura (`.sig`). O Tauri carrega os **dois** em base64 — a chave dentro
//! do `tauri.conf.json`, a assinatura dentro do `latest.json` — porque nenhum
//! dos dois lugares aceita um arquivo de várias linhas. Este módulo recebe as
//! duas na mesma forma em que já viajam hoje.
//!
//! # O modo legado é recusado
//!
//! O minisign tem dois modos: o moderno assina o resumo BLAKE2b-512 do
//! conteúdo, e o legado assina o conteúdo cru. `verify` recebe um
//! `allow_legacy`, e aqui ele é `false` — o mesmo que o plugin do Tauri passa.
//! Aceitá-lo alargaria o que conta como assinatura válida por compatibilidade
//! com uma versão do minisign que este projeto nunca usou.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use base64::Engine as _;

/// Por que uma assinatura não valeu.
///
/// Quatro variantes e não uma, pela mesma razão de sempre: elas pedem coisas
/// diferentes de quem lê. Uma chave malformada é defeito de empacotamento
/// nosso; uma assinatura que não bate é o pacote errado ou adulterado, e é a
/// única da lista para a qual tentar de novo não faz sentido nenhum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaDeAssinatura {
    /// A chave pública embutida neste app não é uma chave minisign.
    ChaveMalformada,
    /// O texto da assinatura não é uma assinatura minisign.
    AssinaturaMalformada,
    /// A assinatura é de outra chave que não a deste projeto.
    OutraChave,
    /// A assinatura não corresponde a estes bytes.
    ///
    /// O pacote foi trocado, truncado ou adulterado. Nada instalado é tocado.
    NaoConfere,
}

/// A chave pública do projeto, pronta para conferir.
pub struct Chave(minisign_verify::PublicKey);

impl std::fmt::Debug for Chave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Sem o conteúdo: a chave é pública, mas um `Debug` que despeja
        // material criptográfico em log ensina o hábito errado para o dia em
        // que o tipo ao lado não for público.
        f.write_str("Chave(<pública do projeto>)")
    }
}

impl Chave {
    /// Lê a chave no formato em que ela já mora no `tauri.conf.json`.
    ///
    /// Isto é: base64 do arquivo `.pub` inteiro, linha de comentário incluída.
    ///
    /// # Errors
    ///
    /// [`FalhaDeAssinatura::ChaveMalformada`] quando o base64 não abre, quando
    /// o que sai dele não é texto, ou quando o texto não é uma chave minisign.
    pub fn do_formato_do_atualizador(pubkey_base64: &str) -> Result<Self, FalhaDeAssinatura> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(pubkey_base64.trim())
            .map_err(|_| FalhaDeAssinatura::ChaveMalformada)?;
        let texto = String::from_utf8(bytes).map_err(|_| FalhaDeAssinatura::ChaveMalformada)?;
        minisign_verify::PublicKey::decode(&texto)
            .map(Self)
            .map_err(|_| FalhaDeAssinatura::ChaveMalformada)
    }

    /// Estes bytes foram assinados por esta chave?
    ///
    /// `assinatura_base64` é o conteúdo do `.sig` em base64 — o mesmo campo
    /// `signature` que o `latest.json` já carrega por plataforma.
    ///
    /// # Errors
    ///
    /// [`FalhaDeAssinatura`], uma variante por motivo. Nenhum caminho aqui
    /// toca em disco: a resposta sai dos bytes que entraram.
    pub fn conferir(&self, dados: &[u8], assinatura_base64: &str) -> Result<(), FalhaDeAssinatura> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(assinatura_base64.trim())
            .map_err(|_| FalhaDeAssinatura::AssinaturaMalformada)?;
        let texto =
            String::from_utf8(bytes).map_err(|_| FalhaDeAssinatura::AssinaturaMalformada)?;
        let assinatura = minisign_verify::Signature::decode(&texto)
            .map_err(|_| FalhaDeAssinatura::AssinaturaMalformada)?;
        self.0.verify(dados, &assinatura, false).map_err(|erro| {
            if matches!(erro, minisign_verify::Error::UnexpectedKeyId) {
                FalhaDeAssinatura::OutraChave
            } else {
                FalhaDeAssinatura::NaoConfere
            }
        })
    }
}

/// Ferramenta de teste: produz par de chaves e assinaturas minisign de verdade.
///
/// **Não é caminho de produção e não entra em binário nenhum** — o módulo
/// inteiro é `#[cfg(test)]` fora deste crate por estar atrás de uma feature de
/// desenvolvimento. Existe porque um teste que só conferisse assinaturas
/// prontas provaria que elas continuam válidas, e o que precisa de prova é o
/// contrário: que uma assinatura de outra chave, ou de outro conteúdo, é
/// recusada. Para isso é preciso saber assinar.
#[cfg(test)]
pub(crate) mod fixtures {
    use base64::Engine as _;
    use blake2::Digest as _;
    use ed25519_dalek::Signer as _;

    /// Um par de chaves de teste, identificado como tal no próprio comentário
    /// da chave — quem encontrar um destes num release sabe na primeira linha
    /// que ele não é de produção.
    pub(crate) struct ChaveDeTeste {
        assinante: ed25519_dalek::SigningKey,
        identificador: [u8; 8],
    }

    impl ChaveDeTeste {
        /// Deriva um par de chaves determinístico a partir de uma semente.
        ///
        /// Determinístico de propósito: um teste que falha tem de falhar
        /// sempre, e uma chave sorteada faz uma falha aparecer numa execução
        /// em cada mil.
        #[must_use]
        pub(crate) fn com_semente(semente: u8) -> Self {
            Self {
                assinante: ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
                identificador: [semente; 8],
            }
        }

        /// A chave pública, no formato do `tauri.conf.json`.
        #[must_use]
        pub(crate) fn publica_no_formato_do_atualizador(&self) -> String {
            let mut bruto = Vec::with_capacity(42);
            bruto.extend_from_slice(b"Ed");
            bruto.extend_from_slice(&self.identificador);
            bruto.extend_from_slice(self.assinante.verifying_key().as_bytes());
            let arquivo = format!(
                "untrusted comment: CHAVE DE TESTE DO SEELE-LANCADOR, nunca de producao\n{}\n",
                base64::engine::general_purpose::STANDARD.encode(&bruto)
            );
            base64::engine::general_purpose::STANDARD.encode(arquivo)
        }

        /// Assina, no formato do campo `signature` do manifesto.
        ///
        /// Modo moderno (prehashed, `ED`), que é o único que
        /// [`super::Chave::conferir`] aceita.
        #[must_use]
        pub(crate) fn assinar_no_formato_do_atualizador(&self, dados: &[u8]) -> String {
            let comentario = "timestamp:0\tfile:artefato-de-teste";
            let resumo = blake2::Blake2b512::digest(dados);
            let assinatura = self.assinante.sign(&resumo);

            let mut primeira = Vec::with_capacity(74);
            primeira.extend_from_slice(b"ED");
            primeira.extend_from_slice(&self.identificador);
            primeira.extend_from_slice(&assinatura.to_bytes());

            let mut global = Vec::new();
            global.extend_from_slice(&assinatura.to_bytes());
            global.extend_from_slice(comentario.as_bytes());
            let assinatura_global = self.assinante.sign(&global);

            let base64 = base64::engine::general_purpose::STANDARD;
            let arquivo = format!(
                "untrusted comment: ASSINATURA DE TESTE DO SEELE-LANCADOR\n{}\ntrusted comment: {comentario}\n{}\n",
                base64.encode(&primeira),
                base64.encode(assinatura_global.to_bytes())
            );
            base64.encode(arquivo)
        }

        /// Assina no **modo legado**, que assina o conteúdo cru.
        ///
        /// Só existe para provar que ele é recusado.
        #[must_use]
        pub(crate) fn assinar_no_modo_legado(&self, dados: &[u8]) -> String {
            let comentario = "timestamp:0\tfile:artefato-de-teste";
            let assinatura = self.assinante.sign(dados);

            let mut primeira = Vec::with_capacity(74);
            primeira.extend_from_slice(b"Ed");
            primeira.extend_from_slice(&self.identificador);
            primeira.extend_from_slice(&assinatura.to_bytes());

            let mut global = Vec::new();
            global.extend_from_slice(&assinatura.to_bytes());
            global.extend_from_slice(comentario.as_bytes());
            let assinatura_global = self.assinante.sign(&global);

            let base64 = base64::engine::general_purpose::STANDARD;
            let arquivo = format!(
                "untrusted comment: ASSINATURA DE TESTE DO SEELE-LANCADOR\n{}\ntrusted comment: {comentario}\n{}\n",
                base64.encode(&primeira),
                base64.encode(assinatura_global.to_bytes())
            );
            base64.encode(arquivo)
        }
    }
}

#[cfg(test)]
mod testes {
    use super::fixtures::ChaveDeTeste;
    use super::*;

    #[test]
    fn uma_assinatura_da_chave_certa_sobre_os_bytes_certos_confere() {
        let par = ChaveDeTeste::com_semente(1);
        let chave = Chave::do_formato_do_atualizador(&par.publica_no_formato_do_atualizador())
            .expect("a chave de teste devia abrir");
        let pacote = b"pacote-de-teste-do-seele-lancador";
        let assinatura = par.assinar_no_formato_do_atualizador(pacote);
        assert_eq!(chave.conferir(pacote, &assinatura), Ok(()));
    }

    /// A prova que só assinar dentro do teste permite fazer.
    #[test]
    fn um_byte_trocado_no_pacote_derruba_a_assinatura() {
        let par = ChaveDeTeste::com_semente(2);
        let chave =
            Chave::do_formato_do_atualizador(&par.publica_no_formato_do_atualizador()).unwrap();
        let assinatura = par.assinar_no_formato_do_atualizador(b"pacote-de-teste");
        assert_eq!(
            chave.conferir(b"pacote-de-testf", &assinatura),
            Err(FalhaDeAssinatura::NaoConfere)
        );
    }

    #[test]
    fn a_assinatura_de_outra_chave_e_recusada_e_o_motivo_a_nomeia() {
        let nossa = ChaveDeTeste::com_semente(3);
        let outra = ChaveDeTeste::com_semente(4);
        let chave =
            Chave::do_formato_do_atualizador(&nossa.publica_no_formato_do_atualizador()).unwrap();
        let pacote = b"pacote-de-teste";
        assert_eq!(
            chave.conferir(pacote, &outra.assinar_no_formato_do_atualizador(pacote)),
            Err(FalhaDeAssinatura::OutraChave)
        );
    }

    /// O modo legado assina o conteúdo cru, e este projeto não o usa.
    #[test]
    fn o_modo_legado_do_minisign_e_recusado() {
        let par = ChaveDeTeste::com_semente(5);
        let chave =
            Chave::do_formato_do_atualizador(&par.publica_no_formato_do_atualizador()).unwrap();
        let pacote = b"pacote-de-teste";
        assert_eq!(
            chave.conferir(pacote, &par.assinar_no_modo_legado(pacote)),
            Err(FalhaDeAssinatura::NaoConfere)
        );
    }

    #[test]
    fn a_chave_de_producao_que_esta_no_tauri_conf_abre_por_este_caminho() {
        // **A chave, e nenhum artefato de produção além dela.** Ela já está no
        // repositório em texto claro, é pública por definição, e o que este
        // teste prova é que o formato lido aqui é o mesmo que o atualizador já
        // usa — que é a afirmação inteira deste módulo. Nada é conferido
        // contra ela: não há pacote de produção nesta pasta e não deve haver.
        let conf = include_str!("../../../apps/seele-app/tauri.conf.json");
        let json: serde_json::Value = serde_json::from_str(conf).unwrap();
        let pubkey = json
            .get("plugins")
            .and_then(|p| p.get("updater"))
            .and_then(|u| u.get("pubkey"))
            .and_then(serde_json::Value::as_str)
            .expect("o `tauri.conf.json` tem de continuar declarando a chave");
        assert!(
            !pubkey.trim().is_empty(),
            "o `tauri.conf.json` perdeu a chave pública do atualizador"
        );
        assert!(
            Chave::do_formato_do_atualizador(pubkey).is_ok(),
            "a chave do atualizador deixou de abrir por este caminho"
        );
    }

    #[test]
    fn texto_que_nao_e_assinatura_nao_causa_panico_e_diz_o_motivo() {
        let par = ChaveDeTeste::com_semente(6);
        let chave =
            Chave::do_formato_do_atualizador(&par.publica_no_formato_do_atualizador()).unwrap();
        for lixo in ["", "!!!", "YWJj", "\u{0}"] {
            assert_eq!(
                chave.conferir(b"x", lixo),
                Err(FalhaDeAssinatura::AssinaturaMalformada),
                "com `{lixo}`"
            );
        }
    }

    #[test]
    fn chave_malformada_e_recusada_em_vez_de_derrubar_o_app() {
        for lixo in ["", "!!!", "YWJj"] {
            assert!(
                matches!(
                    Chave::do_formato_do_atualizador(lixo),
                    Err(FalhaDeAssinatura::ChaveMalformada)
                ),
                "com `{lixo}`"
            );
        }
    }
}
