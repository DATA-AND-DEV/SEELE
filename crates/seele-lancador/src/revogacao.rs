//! A lista de revogação — a exceção ao «toda versão para sempre».
//!
//! ADR 0046: «Guarda-se tudo **menos as com falha grave**». Uma versão
//! revogada **não é hospedável e não é alcançável**, e a recusa diz qual é o
//! defeito e qual versão o corrige — nunca genérica.
//!
//! # Onde a lista mora, e por que não há uma só para tudo
//!
//! O ADR 0046 diz que a lista fica **dentro do manifesto de versões**. O ADR
//! 0044 diz que a remoção de um MOD do indexador «é a mesma peça», e que o
//! catálogo do indexador é assinado com uma chave **separada** da do
//! atualizador — «uma chave que atesta duas coisas deixa as duas se passarem
//! uma pela outra, e a do 0026 autoriza instalar programa».
//!
//! As duas frases não cabem juntas se «a mesma peça» significar «o mesmo
//! arquivo». A conciliação, e o argumento inteiro, está em
//! `docs/versoes-lado-a-lado.md`. Em uma linha: **um código, dois
//! documentos, e cada fato num lugar só**. Este módulo é o código —
//! `alvo` é um identificador de versão quando a lista veio do manifesto, e um
//! identificador de MOD quando ela vier do catálogo. Nenhum fato aparece nos
//! dois.
//!
//! # Por que a lista é assinada de novo, dentro de um manifesto que não é
//!
//! O `latest.json` de hoje não é assinado, e o comentário de
//! `empacotar/manifesto.py` explica bem por quê: cada entrada carrega a
//! assinatura do pacote a que se refere, então trocar o manifesto só troca
//! qual pacote é oferecido, e o pacote trocado é recusado na conferência.
//!
//! **Esse argumento não alcança uma revogação.** Uma revogação é uma
//! afirmação sobre o que **não** deve rodar, e a forma de atacá-la é removê-la
//! — e não há pacote nenhum a conferir depois, porque o ataque é justamente
//! não haver. Por isso ela vem assinada, e por isso [`Memoria`] guarda a
//! última vista: uma lista que sumiu não vira «nada revogado».
//!
//! # O documento é assinado como bytes, e viaja em base64
//!
//! Assinar «o objeto JSON» exigiria uma canonicalização — duas serializações
//! do mesmo objeto que difiram num espaço produzem assinaturas diferentes, e
//! quem escreve o gerador e quem escreve o leitor não combinam espaços.
//! Então o que é assinado são **bytes exatos**, e eles viajam em base64
//! dentro do manifesto. Conferir, depois analisar: nesta ordem, um documento
//! que não abre nunca chegou a ser analisado.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use base64::Engine as _;
use serde::Deserialize;

use crate::assinatura::{Chave, FalhaDeAssinatura};

/// O `schema` de documento de revogação que este build entende.
pub const SCHEMA_CONHECIDO: u32 = 1;

/// Como a lista viaja dentro do manifesto: bytes e a assinatura deles.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct Envelope {
    /// Os bytes exatos do documento, em base64.
    pub document: String,
    /// O `.sig` daqueles bytes, em base64. Mesma chave do atualizador.
    pub signature: String,
}

/// Por que uma lista de revogação não pôde ser usada.
///
/// **Nenhuma destas variantes significa «nada revogado».** Uma lista que não
/// abre é uma lista que não se pode acreditar, e acreditar no que não abre é
/// exatamente o buraco que ela existe para fechar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaDaLista {
    /// O base64 do documento não abre.
    EnvelopeMalformado,
    /// A assinatura não é a do projeto sobre estes bytes.
    AssinaturaRecusada(FalhaDeAssinatura),
    /// Os bytes abriram, e não são um documento de revogação.
    Malformado {
        /// O que o analisador disse. Para o log.
        detalhe: String,
    },
    /// O documento declara um `schema` mais novo que este build.
    MaisNovoQueEsteBuild {
        /// O que o documento pediu.
        pedido: u32,
        /// O que este build entende.
        conhecido: u32,
    },
    /// Esta lista é mais velha que a última que esta máquina já viu.
    ///
    /// Só acontece por ataque ou por engano de publicação: uma lista só
    /// cresce. Aceitar a mais velha desfaria uma revogação já conhecida, que
    /// é a forma mais barata de fazer uma versão furada voltar a rodar.
    Retrocedeu {
        /// O carimbo da lista que chegou.
        chegou: String,
        /// O carimbo da que esta máquina já conhecia.
        conhecida: String,
    },
}

/// Uma revogação: o alvo, o defeito, e onde ele foi corrigido.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Revogacao {
    /// O que foi revogado. Uma versão, no manifesto; um MOD, no catálogo.
    pub target: String,
    /// Qual é o defeito, em uma frase.
    ///
    /// Texto e não enum, e é a única exceção deste crate à regra de
    /// `specs/02-protocolo.md`. A razão: a lista é escrita **depois** deste
    /// build, para descrever um defeito que ninguém aqui conhecia. Um enum
    /// fechado hoje só teria a variante «outro» amanhã, que é a recusa
    /// genérica que a regra existe para impedir.
    pub defect: String,
    /// A versão em que o defeito foi corrigido, quando há uma.
    #[serde(default)]
    pub fixed_in: Option<String>,
}

/// A lista inteira, já conferida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListaDeRevogacao {
    emitida_em: String,
    revogadas: Vec<Revogacao>,
}

#[derive(Deserialize)]
struct Documento {
    #[serde(default)]
    schema: Option<u32>,
    issued_at: String,
    #[serde(default)]
    revoked: Vec<Revogacao>,
}

impl ListaDeRevogacao {
    /// Confere a assinatura e **depois** analisa o documento.
    ///
    /// # Errors
    ///
    /// [`FalhaDaLista`], uma variante por motivo.
    pub fn abrir(envelope: &Envelope, chave: &Chave) -> Result<Self, FalhaDaLista> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(envelope.document.trim())
            .map_err(|_| FalhaDaLista::EnvelopeMalformado)?;
        chave
            .conferir(&bytes, &envelope.signature)
            .map_err(FalhaDaLista::AssinaturaRecusada)?;

        let documento: Documento =
            serde_json::from_slice(&bytes).map_err(|erro| FalhaDaLista::Malformado {
                detalhe: erro.to_string(),
            })?;
        let schema = documento.schema.unwrap_or(SCHEMA_CONHECIDO);
        if schema > SCHEMA_CONHECIDO {
            return Err(FalhaDaLista::MaisNovoQueEsteBuild {
                pedido: schema,
                conhecido: SCHEMA_CONHECIDO,
            });
        }
        Ok(Self {
            emitida_em: documento.issued_at,
            revogadas: documento.revoked,
        })
    }

    /// Quando esta lista foi emitida, em RFC 3339.
    #[must_use]
    pub fn emitida_em(&self) -> &str {
        &self.emitida_em
    }

    /// Tudo o que esta lista revoga.
    #[must_use]
    pub fn revogadas(&self) -> &[Revogacao] {
        &self.revogadas
    }

    /// Este alvo está revogado?
    #[must_use]
    pub fn revogacao_de(&self, alvo: &str) -> Option<&Revogacao> {
        self.revogadas.iter().find(|r| r.target == alvo)
    }
}

/// A lista de revogação que **vale** nesta máquina, agora.
///
/// Existe como tipo, e não como um `Option<&ListaDeRevogacao>` que se possa
/// esquecer de passar, porque a defesa contra **esconder** uma revogação é
/// justamente a que se perde por esquecimento. A assinatura impede forjar; o
/// que impede esconder é
/// [`Deposito::conciliar_revogacoes`](crate::Deposito::conciliar_revogacoes),
/// e uma resolução que lesse o bloco do manifesto direto voltaria a aceitar
/// «manifesto servido sem lista» como «nada revogado».
///
/// **Só a conciliação constrói um destes**, e um
/// [`Resolvedor`](crate::Resolvedor) exige um na construção. O caminho que
/// esquece de conciliar não compila — que é mais forte do que um teste
/// reprovando quem esquecer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vigente(Option<ListaDeRevogacao>);

impl Vigente {
    /// De dentro do crate, e só depois de conciliar. Ver o tipo.
    pub(crate) const fn conciliada(lista: Option<ListaDeRevogacao>) -> Self {
        Self(lista)
    }

    /// A lista, quando esta máquina conhece alguma.
    ///
    /// `None` é o que sobra de uma conciliação que não achou lista em lugar
    /// nenhum — nem no manifesto de hoje, nem guardada de ontem —, e não «não
    /// perguntei».
    #[must_use]
    pub const fn lista(&self) -> Option<&ListaDeRevogacao> {
        self.0.as_ref()
    }

    /// Este alvo está revogado, pela lista que vale?
    #[must_use]
    pub fn revogacao_de(&self, alvo: &str) -> Option<&Revogacao> {
        self.0.as_ref().and_then(|lista| lista.revogacao_de(alvo))
    }
}

/// O que esta máquina já viu de lista, para uma lista não poder recuar.
///
/// Guardada em disco pelo [`crate::deposito::Deposito`]. Sem ela, a proteção
/// da assinatura é parcial: assinar impede **forjar** uma revogação, e não
/// impede **esconder** uma, servindo o manifesto de antes dela.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Memoria {
    /// O carimbo da lista mais nova já aceita nesta máquina.
    pub ultima_emissao: Option<String>,
}

impl Memoria {
    /// Aceita a lista que chegou, ou recusa por ela ser mais velha.
    ///
    /// A comparação é textual, e é o suficiente: RFC 3339 em UTC com o mesmo
    /// número de casas ordena lexicograficamente na mesma ordem que
    /// cronologicamente, e é o formato que `empacotar/manifesto.py` já
    /// escreve. Analisar data aqui traria um crate de calendário para dentro
    /// de um crate que não fala com a rede nem com o relógio.
    ///
    /// # Errors
    ///
    /// [`FalhaDaLista::Retrocedeu`] quando a lista que chegou é mais velha que
    /// a última aceita.
    pub fn aceitar(&mut self, lista: &ListaDeRevogacao) -> Result<(), FalhaDaLista> {
        if let Some(conhecida) = &self.ultima_emissao {
            if lista.emitida_em() < conhecida.as_str() {
                return Err(FalhaDaLista::Retrocedeu {
                    chegou: lista.emitida_em().to_owned(),
                    conhecida: conhecida.clone(),
                });
            }
        }
        self.ultima_emissao = Some(lista.emitida_em().to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::assinatura::fixtures::ChaveDeTeste;

    fn empacotar(par: &ChaveDeTeste, documento: &str) -> Envelope {
        let base64 = base64::engine::general_purpose::STANDARD;
        Envelope {
            document: base64.encode(documento),
            signature: par.assinar_no_formato_do_atualizador(documento.as_bytes()),
        }
    }

    fn chave(par: &ChaveDeTeste) -> Chave {
        Chave::do_formato_do_atualizador(&par.publica_no_formato_do_atualizador()).unwrap()
    }

    const UMA_LISTA: &str = r#"{"schema":1,"issued_at":"2026-09-10T00:00:00Z","revoked":[
        {"target":"0.9.9-teste","defect":"a porta ficava aberta","fixed_in":"0.9.10-teste"}]}"#;

    #[test]
    fn uma_lista_assinada_pelo_projeto_abre_e_diz_o_defeito() {
        let par = ChaveDeTeste::com_semente(11);
        let lista = ListaDeRevogacao::abrir(&empacotar(&par, UMA_LISTA), &chave(&par)).unwrap();
        let r = lista.revogacao_de("0.9.9-teste").expect("devia estar lá");
        assert_eq!(r.defect, "a porta ficava aberta");
        assert_eq!(r.fixed_in.as_deref(), Some("0.9.10-teste"));
        assert!(lista.revogacao_de("0.10.5-1").is_none());
    }

    /// A prova de que a assinatura da lista não é enfeite.
    #[test]
    fn uma_lista_editada_depois_de_assinada_e_recusada() {
        let par = ChaveDeTeste::com_semente(12);
        let mut envelope = empacotar(&par, UMA_LISTA);
        // Alguém tira a revogação e mantém a assinatura.
        envelope.document = base64::engine::general_purpose::STANDARD
            .encode(r#"{"schema":1,"issued_at":"2026-09-10T00:00:00Z","revoked":[]}"#);
        assert_eq!(
            ListaDeRevogacao::abrir(&envelope, &chave(&par)),
            Err(FalhaDaLista::AssinaturaRecusada(
                FalhaDeAssinatura::NaoConfere
            ))
        );
    }

    #[test]
    fn uma_lista_assinada_por_outra_chave_e_recusada() {
        let nossa = ChaveDeTeste::com_semente(13);
        let outra = ChaveDeTeste::com_semente(14);
        assert_eq!(
            ListaDeRevogacao::abrir(&empacotar(&outra, UMA_LISTA), &chave(&nossa)),
            Err(FalhaDaLista::AssinaturaRecusada(
                FalhaDeAssinatura::OutraChave
            ))
        );
    }

    /// Esconder uma revogação servindo o manifesto de antes dela.
    #[test]
    fn uma_lista_mais_velha_que_a_ja_vista_e_recusada() {
        let par = ChaveDeTeste::com_semente(15);
        let velha = r#"{"schema":1,"issued_at":"2026-08-01T00:00:00Z","revoked":[]}"#;
        let mut memoria = Memoria::default();

        let nova = ListaDeRevogacao::abrir(&empacotar(&par, UMA_LISTA), &chave(&par)).unwrap();
        assert_eq!(memoria.aceitar(&nova), Ok(()));

        let anterior = ListaDeRevogacao::abrir(&empacotar(&par, velha), &chave(&par)).unwrap();
        assert_eq!(
            memoria.aceitar(&anterior),
            Err(FalhaDaLista::Retrocedeu {
                chegou: "2026-08-01T00:00:00Z".to_owned(),
                conhecida: "2026-09-10T00:00:00Z".to_owned(),
            })
        );
        // E a memória não recuou junto.
        assert_eq!(
            memoria.ultima_emissao.as_deref(),
            Some("2026-09-10T00:00:00Z")
        );
    }

    #[test]
    fn a_mesma_lista_de_novo_e_aceita_sem_reclamar() {
        let par = ChaveDeTeste::com_semente(16);
        let lista = ListaDeRevogacao::abrir(&empacotar(&par, UMA_LISTA), &chave(&par)).unwrap();
        let mut memoria = Memoria::default();
        assert_eq!(memoria.aceitar(&lista), Ok(()));
        assert_eq!(memoria.aceitar(&lista), Ok(()));
    }

    #[test]
    fn nenhuma_entrada_causa_panico() {
        let par = ChaveDeTeste::com_semente(17);
        let boa = par.assinar_no_formato_do_atualizador(b"x");
        for (documento, assinatura) in [
            ("", ""),
            ("!!!", "!!!"),
            ("e30=", "YWJj"),
            ("eyJpc3N1ZWRfYXQiOjF9", boa.as_str()),
        ] {
            let envelope = Envelope {
                document: documento.to_owned(),
                signature: assinatura.to_owned(),
            };
            let _ = ListaDeRevogacao::abrir(&envelope, &chave(&par));
        }
    }
}
