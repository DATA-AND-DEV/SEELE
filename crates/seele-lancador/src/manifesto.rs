//! O manifesto de versões — o `latest.json` depois do ADR 0045.
//!
//! O ADR 0045 diz duas coisas sobre este arquivo, e as duas foram mantidas:
//! ele **continua morando no release do GitHub**, em
//! `releases/latest/download/latest.json`, e ele deixa de descrever uma versão
//! para descrever **todas**. Nenhum serviço novo a hospedar, que era metade do
//! orgulho do ADR 0026.
//!
//! O contrato completo, com o que ainda falta do lado do `SEELE-RELEASES`,
//! está em `docs/versoes-lado-a-lado.md`.
//!
//! # Duas formas, e a mais velha continua sendo lida
//!
//! O `latest.json` publicado hoje é o do `tauri-plugin-updater`: uma versão,
//! um `platforms`, sem `schema`. Este módulo o lê como um manifesto de **uma**
//! publicação, em vez de recusá-lo.
//!
//! Não é gentileza com um formato velho: é a única forma de o launcher
//! funcionar contra o que está publicado neste momento. Recusar levaria a
//! «nenhuma versão publicada» num dia em que há onze delas na página — a frase
//! errada, do jeito que o ADR 0026 já errou uma vez.
//!
//! # Chave desconhecida é ignorada aqui, e recusada no `mod.json`
//!
//! A diferença é deliberada e vale escrever, porque as duas regras convivem no
//! mesmo produto. Um `mod.json` é escrito por uma pessoa, uma vez, e um
//! `vesion` com um `s` a menos instalaria calado um MOD sem versão: ali a
//! recusa é o único retorno que o formato dá.
//!
//! Um manifesto de versões é escrito por nós e lido por **todas as versões já
//! instaladas**, inclusive as de antes do campo que estamos acrescentando
//! hoje. Se um campo novo fizesse o manifesto ser recusado, acrescentá-lo
//! trancaria fora exatamente as instalações antigas que o ADR 0045 promete
//! manter de pé. O que gateia mudança incompatível é o `schema`, e só ele.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::executavel::{CaminhoDoExecutavel, ExecutavelInvalido};
use crate::revogacao::{Envelope, FalhaDaLista, ListaDeRevogacao};
use crate::versao::Versao;
use crate::Chave;

/// O `schema` que este build entende.
///
/// Mudança **aditiva** não mexe neste número: campo novo é ignorado por quem
/// não o conhece, e é o que permite acrescentar sem trancar ninguém. Subir
/// daqui é declarar que o manifesto deixou de poder ser lido por quem está
/// instalado — e o ADR 0045 diz que isso é caro.
pub const SCHEMA_CONHECIDO: u32 = 2;

/// Por que um manifesto não pôde ser usado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaDoManifesto {
    /// O texto não é JSON, ou não tem a forma de um manifesto.
    Malformado {
        /// O que o analisador disse. Para o log, nunca para a tela.
        detalhe: String,
    },
    /// O manifesto declara um `schema` mais novo que este build.
    ///
    /// Separado de [`Self::Malformado`] porque a coisa a fazer é outra: aqui
    /// não há defeito nenhum, o launcher é que está velho. A tela manda
    /// atualizar o launcher; ali, manda avisar o projeto.
    MaisNovoQueEsteBuild {
        /// O que o manifesto pediu.
        pedido: u32,
        /// O que este build entende.
        conhecido: u32,
    },
    /// O manifesto não lista publicação nenhuma.
    Vazio,
    /// A lista de revogação veio, e não passou pela conferência.
    Revogacoes(FalhaDaLista),
}

/// O que uma versão promete ser, como unidade.
///
/// ADR 0045: «cliente, servidor, protocolo e API de MOD sobem juntos e são
/// identificados pelo mesmo número. **Não há matriz de compatibilidade**».
/// Estes dois números não são uma matriz — são o que aquela versão fala,
/// declarado para poder ser **conferido** contra o que um servidor anuncia. É
/// a «conferência de coerência» em que o ADR transforma a negociação de
/// protocolo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Unidade {
    /// A versão do protocolo que este produto fala.
    pub protocol: u8,
    /// A versão da API de MOD que este produto oferece.
    pub mod_api: u32,
}

/// Um pacote, para um sistema e uma arquitetura.
///
/// **Nenhum campo é obrigatório na leitura, e todos os que importam são
/// privados.** As duas metades dessa frase andam juntas e são a mesma decisão
/// do `executable`: um pacote de um alvo ao qual falta um campo **não pode**
/// derrubar o manifesto inteiro — porque derrubá-lo tranca fora todas as
/// versões já instaladas, que é exatamente o que o ADR 0045 promete não fazer
/// —, e a única forma de um campo faltante não virar um `""` calado adiante é
/// ninguém alcançar o campo sem passar por um acessador que diz que ele falta.
///
/// Um erro de empacotamento num alvo custa **aquele alvo**, com motivo
/// próprio, e não a lista inteira.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pacote {
    /// De onde baixar — **privado**, por [`Pacote::url`].
    #[serde(default)]
    url: Option<String>,
    /// O `.sig` do minisign, em base64 — o mesmo campo de hoje, **privado**,
    /// por [`Pacote::assinatura`].
    #[serde(default)]
    signature: Option<String>,
    /// O resumo SHA-256 do pacote, em hexadecimal minúsculo.
    ///
    /// **Não substitui a assinatura, e a assinatura não o dispensa.** Ele
    /// responde «o download veio inteiro?», que é uma pergunta sobre a rede, e
    /// dá o motivo certo — «baixe de novo» — para o caso comum de um arquivo
    /// truncado. A assinatura responde «isto veio de nós?», que é a pergunta
    /// sobre segurança, e é a que recusa um pacote adulterado.
    ///
    /// `None` no manifesto de hoje, que não o traz. Ver
    /// `docs/versoes-lado-a-lado.md`.
    #[serde(default)]
    pub sha256: Option<String>,
    /// O caminho do executável dentro da instalação, como o manifesto o
    /// escreveu — **privado de propósito**.
    ///
    /// Declarado e não adivinhado: o nome muda com o sistema
    /// (`SEELE.app/Contents/MacOS/SEELE`, `SEELE.exe`, `bin/seele`), e um
    /// launcher que o adivinha erra calado num sistema só — que é o defeito
    /// mais caro possível aqui, porque ele aparece na máquina de outra pessoa.
    ///
    /// Privado porque o manifesto **não é assinado** e este campo vira o
    /// último `join` antes de um processo ser iniciado: quem precisar dele
    /// passa por [`Pacote::executavel`], que confere. Ver
    /// [`crate::executavel`].
    #[serde(default)]
    executable: Option<String>,
}

impl Pacote {
    /// De onde baixar, quando o manifesto disse.
    ///
    /// `None` é um pacote que o manifesto lista sem dizer onde ele está — não
    /// há o que baixar, e quem chama recusa **este alvo**. A recusa não é aqui
    /// porque este crate não fala com a rede; ver [`crate`].
    #[must_use]
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// A assinatura do pacote, quando o manifesto a trouxe.
    ///
    /// `None` **não** é «pacote sem assinatura a conferir»: é um pacote que
    /// não pode ser instalado, e [`crate::Deposito::instalar`] o recusa com
    /// [`crate::FalhaAoInstalar::AssinaturaNaoDeclarada`]. O campo é privado
    /// para que não exista o caminho em que um `None` vira um `""` e a
    /// conferência passa a ser contra a assinatura vazia.
    #[must_use]
    pub fn assinatura(&self) -> Option<&str> {
        self.signature.as_deref()
    }

    /// Troca a assinatura, para um teste poder provar que outra chave é
    /// recusada.
    #[cfg(test)]
    pub(crate) fn trocar_assinatura(&mut self, nova: String) {
        self.signature = Some(nova);
    }

    /// O caminho do executável, já conferido como relativo e confinado.
    ///
    /// # Errors
    ///
    /// [`ExecutavelInvalido::NaoDeclarado`] quando o manifesto não traz o
    /// campo — que é o estado do manifesto publicado hoje —, e uma variante
    /// por motivo quando ele traz algo que sairia da pasta da versão.
    pub fn executavel(&self) -> Result<CaminhoDoExecutavel, ExecutavelInvalido> {
        let declarado = self
            .executable
            .as_deref()
            .ok_or(ExecutavelInvalido::NaoDeclarado)?;
        CaminhoDoExecutavel::novo(declarado)
    }
}

/// Uma versão publicada, como o manifesto a descreve.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Publicacao {
    /// O identificador.
    pub version: Versao,
    /// Quando saiu, em RFC 3339. Para a tela, nunca para ordenar.
    #[serde(default)]
    pub pub_date: Option<String>,
    /// As notas, quando o release trouxe alguma.
    #[serde(default)]
    pub notes: Option<String>,
    /// O que esta versão fala. `None` num manifesto que ainda não declara.
    #[serde(default)]
    pub unit: Option<Unidade>,
    /// Um pacote por alvo, na nomenclatura do atualizador (`darwin-aarch64`).
    #[serde(default)]
    pub platforms: BTreeMap<String, Pacote>,
}

/// O manifesto inteiro, já conferido.
///
/// **Há sempre pelo menos uma publicação**, e o invariante é do tipo: [`ler`]
/// é o único construtor e recusa um manifesto sem nenhuma com
/// [`FalhaDoManifesto::Vazio`]. Guardar a mais nova separada das outras é o que
/// permite [`Manifesto::mais_nova`] devolver uma publicação em vez de um
/// `Option` que ninguém poderia ver vazio — um `None` ali virava uma recusa
/// inalcançável na [`crate::resolucao`], que é a forma mais educada de um
/// guarda não existir.
///
/// [`ler`]: Manifesto::ler
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifesto {
    mais_nova: Publicacao,
    outras: Vec<Publicacao>,
    revogacoes: Option<ListaDeRevogacao>,
    envelope: Option<Envelope>,
}

/// A forma crua do arquivo, antes de qualquer regra.
#[derive(Deserialize)]
struct Cru {
    #[serde(default)]
    schema: Option<u32>,
    #[serde(default)]
    versions: Option<Vec<Publicacao>>,
    #[serde(default)]
    revocations: Option<Envelope>,
    // O formato de hoje, de uma publicação só.
    #[serde(default)]
    version: Option<Versao>,
    #[serde(default)]
    pub_date: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    platforms: Option<BTreeMap<String, Pacote>>,
}

impl Manifesto {
    /// Lê um manifesto e confere a lista de revogação que ele traz.
    ///
    /// A chave é a mesma do atualizador: a lista de revogação de **versões**
    /// mora no manifesto e é assinada pela chave que autoriza instalar
    /// programa. Ver `docs/versoes-lado-a-lado.md` para por que ela não mora
    /// junto da de MODs.
    ///
    /// # Errors
    ///
    /// [`FalhaDoManifesto`], uma variante por motivo.
    pub fn ler(bytes: &[u8], chave: &Chave) -> Result<Self, FalhaDoManifesto> {
        let cru: Cru =
            serde_json::from_slice(bytes).map_err(|erro| FalhaDoManifesto::Malformado {
                detalhe: erro.to_string(),
            })?;

        // Ausente é o formato de hoje, que não declara `schema` nenhum.
        let schema = cru.schema.unwrap_or(1);
        if schema > SCHEMA_CONHECIDO {
            return Err(FalhaDoManifesto::MaisNovoQueEsteBuild {
                pedido: schema,
                conhecido: SCHEMA_CONHECIDO,
            });
        }

        let publicacoes = match cru.versions {
            Some(lista) => lista,
            None => match cru.version {
                Some(version) => vec![Publicacao {
                    version,
                    pub_date: cru.pub_date,
                    notes: cru.notes,
                    unit: None,
                    platforms: cru.platforms.unwrap_or_default(),
                }],
                None => return Err(FalhaDoManifesto::Vazio),
            },
        };
        let mut publicacoes = publicacoes.into_iter();
        let Some(mais_nova) = publicacoes.next() else {
            return Err(FalhaDoManifesto::Vazio);
        };
        let outras: Vec<Publicacao> = publicacoes.collect();

        let revogacoes = match &cru.revocations {
            Some(envelope) => Some(
                ListaDeRevogacao::abrir(envelope, chave).map_err(FalhaDoManifesto::Revogacoes)?,
            ),
            None => None,
        };

        Ok(Self {
            mais_nova,
            outras,
            revogacoes,
            envelope: cru.revocations,
        })
    }

    /// As publicações, na ordem em que o manifesto as lista.
    ///
    /// **A ordem é o contrato**, e a primeira é a mais nova. Não há ordenação
    /// calculada aqui: `0.10.5-1` é, para o semver, anterior a `0.10.5`, e no
    /// `SEELE-RELEASES` ela saiu depois. Ver [`crate::versao::Versao`].
    pub fn publicacoes(&self) -> impl Iterator<Item = &Publicacao> {
        std::iter::once(&self.mais_nova).chain(self.outras.iter())
    }

    /// Quantas publicações o manifesto lista. Nunca zero.
    #[must_use]
    pub fn quantas(&self) -> usize {
        1 + self.outras.len()
    }

    /// A mais nova publicada.
    ///
    /// Não devolve `Option`: um manifesto sem publicação nenhuma não chega a
    /// existir. Ver [`Manifesto`].
    #[must_use]
    pub fn mais_nova(&self) -> &Publicacao {
        &self.mais_nova
    }

    /// A publicação com este identificador, se o manifesto a lista.
    #[must_use]
    pub fn publicacao(&self, versao: &Versao) -> Option<&Publicacao> {
        self.publicacoes().find(|p| &p.version == versao)
    }

    /// A posição de uma versão na ordem de publicação. `0` é a mais nova.
    ///
    /// É o que permite dizer «esta é mais velha que aquela» sem inventar uma
    /// ordenação de versões.
    #[must_use]
    pub fn posicao(&self, versao: &Versao) -> Option<usize> {
        self.publicacoes().position(|p| &p.version == versao)
    }

    /// A lista de revogação, quando o manifesto trouxe uma conferida.
    #[must_use]
    pub fn revogacoes(&self) -> Option<&ListaDeRevogacao> {
        self.revogacoes.as_ref()
    }

    /// A lista como ela veio — bytes e assinatura.
    ///
    /// Guardada porque conferir não é o mesmo que **guardar**: é este envelope
    /// que o depósito escreve em disco, para que uma lista que suma do
    /// manifesto amanhã não vire «nada revogado». Ver
    /// [`crate::Deposito::conciliar_revogacoes`].
    #[must_use]
    pub fn envelope_de_revogacoes(&self) -> Option<&Envelope> {
        self.envelope.as_ref()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::assinatura::fixtures::ChaveDeTeste;
    use crate::executavel::ExecutavelInvalido;

    fn chave_qualquer() -> Chave {
        Chave::do_formato_do_atualizador(
            &ChaveDeTeste::com_semente(9).publica_no_formato_do_atualizador(),
        )
        .unwrap()
    }

    /// O manifesto que está publicado **hoje**, na forma em que está.
    ///
    /// Os campos e a forma foram lidos de
    /// `releases/latest/download/latest.json` do `SEELE-RELEASES` em
    /// 10/09/2026; as assinaturas foram substituídas por texto de teste, porque
    /// nada aqui as confere e carregar assinatura de produção numa fixture
    /// convida alguém a conferi-la um dia.
    const COMO_ESTA_PUBLICADO_HOJE: &str = r#"{
      "version": "0.10.5-1",
      "notes": "SEELE 0.10.5-1.",
      "pub_date": "2026-09-05T04:38:13Z",
      "platforms": {
        "darwin-aarch64": { "url": "https://exemplo.invalido/a", "signature": "ASSINATURA-DE-TESTE" },
        "windows-x86_64": { "url": "https://exemplo.invalido/b", "signature": "ASSINATURA-DE-TESTE" }
      }
    }"#;

    #[test]
    fn o_manifesto_de_hoje_e_lido_como_uma_publicacao_so() {
        let m = Manifesto::ler(COMO_ESTA_PUBLICADO_HOJE.as_bytes(), &chave_qualquer())
            .expect("o formato publicado hoje tem de continuar sendo lido");
        assert_eq!(m.quantas(), 1);
        assert_eq!(m.mais_nova().version.como_texto(), "0.10.5-1");
        assert_eq!(m.mais_nova().platforms.len(), 2);
        assert!(m.revogacoes().is_none());
    }

    #[test]
    fn a_ordem_do_manifesto_e_quem_diz_qual_e_a_mais_nova() {
        // `0.10.5-1` depois de `0.10.5` é exatamente o caso que o semver
        // ordenaria ao contrário.
        let json = r#"{"schema":2,"versions":[
            {"version":"0.10.5-1"},
            {"version":"0.10.5"},
            {"version":"0.10.4-4"}]}"#;
        let m = Manifesto::ler(json.as_bytes(), &chave_qualquer()).unwrap();
        assert_eq!(m.mais_nova().version.como_texto(), "0.10.5-1");
        let velha = Versao::nova("0.10.4-4").unwrap();
        let nova = Versao::nova("0.10.5-1").unwrap();
        assert!(m.posicao(&velha) > m.posicao(&nova));
    }

    /// A regra que mantém as instalações antigas de pé.
    #[test]
    fn um_campo_que_este_build_nao_conhece_nao_derruba_o_manifesto() {
        let json = r#"{"schema":2,"canal":"beta","versions":[
            {"version":"1.0.0","assinado_por":"quem","platforms":{}}]}"#;
        let m = Manifesto::ler(json.as_bytes(), &chave_qualquer())
            .expect("campo novo não pode trancar quem já está instalado");
        assert_eq!(m.quantas(), 1);
    }

    /// A mesma regra, para um campo que **falta** num pacote de um alvo só.
    ///
    /// É o par do teste acima, e a razão é a mesma: um erro de empacotamento
    /// num alvo não pode trancar fora as versões já instaladas. Derrubar o
    /// manifesto aqui custaria a lista inteira — inclusive a `0.9.0-teste`,
    /// que não tem defeito nenhum e é a que a pessoa está rodando.
    #[test]
    fn um_pacote_sem_url_ou_sem_assinatura_nao_derruba_o_manifesto_inteiro() {
        let json = r#"{"schema":2,"versions":[
            {"version":"1.0.0-teste","platforms":{
                "sem-assinatura":{"url":"https://exemplo.invalido/a"},
                "sem-url":{"signature":"ASSINATURA-DE-TESTE"},
                "inteiro":{"url":"https://exemplo.invalido/c","signature":"ASSINATURA-DE-TESTE"}}},
            {"version":"0.9.0-teste"}]}"#;
        let m = Manifesto::ler(json.as_bytes(), &chave_qualquer())
            .expect("um alvo pela metade não pode trancar quem já está instalado");
        assert_eq!(
            m.quantas(),
            2,
            "a versão que a pessoa roda tem de continuar"
        );

        let pacotes = &m.mais_nova().platforms;
        assert_eq!(pacotes.len(), 3, "os três alvos continuam listados");
        // Cada um diz o que lhe falta, no lugar de virar texto vazio.
        assert!(pacotes["sem-assinatura"].assinatura().is_none());
        assert_eq!(
            pacotes["sem-assinatura"].url(),
            Some("https://exemplo.invalido/a")
        );
        assert!(pacotes["sem-url"].url().is_none());
        assert_eq!(pacotes["inteiro"].url(), Some("https://exemplo.invalido/c"));
        assert_eq!(pacotes["inteiro"].assinatura(), Some("ASSINATURA-DE-TESTE"));
    }

    #[test]
    fn um_schema_mais_novo_diz_que_o_launcher_e_que_esta_velho() {
        let json = r#"{"schema":99,"versions":[{"version":"1.0.0"}]}"#;
        assert_eq!(
            Manifesto::ler(json.as_bytes(), &chave_qualquer()),
            Err(FalhaDoManifesto::MaisNovoQueEsteBuild {
                pedido: 99,
                conhecido: SCHEMA_CONHECIDO
            })
        );
    }

    #[test]
    fn um_manifesto_sem_publicacao_nenhuma_e_recusado_por_isso() {
        for json in [r#"{"schema":2,"versions":[]}"#, r#"{"schema":2}"#] {
            assert_eq!(
                Manifesto::ler(json.as_bytes(), &chave_qualquer()),
                Err(FalhaDoManifesto::Vazio),
                "com `{json}`"
            );
        }
    }

    #[test]
    fn uma_versao_que_atravessa_diretorio_derruba_o_manifesto_inteiro() {
        let json = r#"{"schema":2,"versions":[{"version":"../fuga"}]}"#;
        assert!(matches!(
            Manifesto::ler(json.as_bytes(), &chave_qualquer()),
            Err(FalhaDoManifesto::Malformado { .. })
        ));
    }

    #[test]
    fn a_unidade_de_uma_versao_e_lida_quando_declarada() {
        let json = r#"{"schema":2,"versions":[
            {"version":"1.0.0","unit":{"protocol":3,"mod_api":1}}]}"#;
        let m = Manifesto::ler(json.as_bytes(), &chave_qualquer()).unwrap();
        assert_eq!(
            m.mais_nova().unit,
            Some(Unidade {
                protocol: 3,
                mod_api: 1
            })
        );
    }

    /// O manifesto publicado hoje não traz `executable`, e o launcher precisa
    /// dizer isso em vez de adivinhar. Lacuna 3 de
    /// `docs/versoes-lado-a-lado.md`.
    #[test]
    fn o_manifesto_de_hoje_nao_declara_executavel_e_diz_isso() {
        let m = Manifesto::ler(COMO_ESTA_PUBLICADO_HOJE.as_bytes(), &chave_qualquer()).unwrap();
        let pacote = m.mais_nova().platforms.get("darwin-aarch64").unwrap();
        assert_eq!(pacote.executavel(), Err(ExecutavelInvalido::NaoDeclarado));
    }

    /// A conferência do executável é por pacote, e não na desserialização: um
    /// `executable` errado num alvo **não pode** trancar fora as versões que já
    /// estão instaladas, que é a promessa do ADR 0045.
    #[test]
    fn um_executavel_que_sai_da_instalacao_nao_derruba_o_manifesto_inteiro() {
        let json = r#"{"schema":2,"versions":[
            {"version":"1.0.0","platforms":{"alvo-de-teste":{
                "url":"https://exemplo.invalido/a","signature":"ASSINATURA-DE-TESTE",
                "executable":"/bin/sh"}}},
            {"version":"0.9.0"}]}"#;
        let m = Manifesto::ler(json.as_bytes(), &chave_qualquer())
            .expect("um `executable` errado não pode trancar as outras versões");
        assert_eq!(m.quantas(), 2);
        assert_eq!(
            m.mais_nova()
                .platforms
                .get("alvo-de-teste")
                .unwrap()
                .executavel(),
            Err(ExecutavelInvalido::Absoluto {
                caminho: "/bin/sh".to_owned()
            })
        );
    }

    #[test]
    fn nenhuma_entrada_causa_panico() {
        for bruto in [
            b"".as_slice(),
            b"{",
            b"[]",
            b"null",
            b"{\"schema\":\"dois\"}",
            &[0xff, 0xfe, 0x00],
        ] {
            let _ = Manifesto::ler(bruto, &chave_qualquer());
        }
    }
}
