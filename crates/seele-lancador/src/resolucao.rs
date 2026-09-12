//! Qual versão roda, e por que uma versão foi recusada.
//!
//! É a pergunta inteira do launcher, e ela tem três respostas possíveis: esta
//! versão, com este executável e estes dados; ou uma recusa **com motivo**.
//! Nenhuma recusa daqui é genérica — `specs/02-protocolo.md` exige isso de
//! toda razão, e o ADR 0045 repete para a revogação: *«a recusa diz qual é o
//! defeito e qual versão o corrige»*.
//!
//! # A revogação é conferida ao hospedar e ao entrar, nunca ao abrir
//!
//! ADR 0045, e é a parte que custa:
//!
//! > A lista é conferida **ao hospedar** e **ao entrar**, que são atos, e
//! > **nunca ao abrir o app**. É o que preserva a regra do 0026: *«num produto
//! > cujo argumento é que o servidor é seu, um app que fala com o github.com a
//! > cada arranque contradiz o argumento»*.
//!
//! Isso está em [`Ato`], e é por isso que ele é um parâmetro em vez de uma
//! configuração. **Abrir uma versão revogada é permitido**: as conversas de
//! quem estava nela continuam lá para serem lidas. O que ela não pode é
//! hospedar e entrar.
//!
//! # A lista que vale é a conciliada, e ela é exigida
//!
//! [`Resolvedor::novo`] pede um [`Vigente`], e ele só sai de
//! [`Deposito::conciliar_revogacoes`](crate::Deposito::conciliar_revogacoes).
//! Não há aqui nenhum caminho que leia o bloco do manifesto por conta própria:
//! se houvesse, quem esquecesse de conciliar voltaria a aceitar «manifesto
//! servido sem lista» como «nada revogado» — o ataque que a conciliação existe
//! para fechar —, e o esquecimento seria silencioso. Agora ele não compila.
//!
//! # A negociação de protocolo virou conferência de coerência
//!
//! ADR 0045: *«os dois lados já sabem a versão antes de falar»*. Então quando
//! um servidor anuncia versão **e** protocolo, os dois têm de bater com o que
//! o manifesto diz daquela versão — [`Recusa::UnidadeIncoerente`]. O
//! `negotiate` do `seele-proto` fica onde está e continua sendo a primeira
//! coisa a tocar um byte de socket não confiável; esta conferência acontece
//! antes, com dados que já foram conferidos por assinatura.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use crate::deposito::Deposito;
use crate::executavel::ExecutavelInvalido;
use crate::manifesto::{Manifesto, Unidade};
use crate::revogacao::Vigente;
use crate::versao::Versao;

/// Que versão se quer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pedido {
    /// A mais nova que o manifesto lista. O padrão, e o que o ADR 0045 manda
    /// ser: *«a maioria das pessoas não sabe responder "qual versão"»*.
    MaisNova,
    /// Esta, e não outra.
    Exata(Versao),
}

/// Para que se está resolvendo uma versão.
///
/// Existe porque a lista de revogação é conferida em dois deles e não no
/// terceiro. Ver o cabeçalho do módulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ato {
    /// Abrir o app. **Não confere revogação.**
    Abrir,
    /// Subir um servidor nesta versão. Confere.
    Hospedar,
    /// Entrar num servidor que roda esta versão. Confere.
    Entrar,
}

impl Ato {
    /// Este ato confere a lista de revogação?
    #[must_use]
    pub const fn confere_revogacao(self) -> bool {
        match self {
            Self::Abrir => false,
            Self::Hospedar | Self::Entrar => true,
        }
    }
}

/// Por que a versão pedida não pôde ser usada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recusa {
    /// O manifesto não conhece esta versão.
    ///
    /// Um servidor que anuncia uma versão que não está publicada, ou um
    /// identificador digitado à mão.
    NaoPublicada {
        /// A que foi pedida.
        versao: Versao,
    },
    /// Esta versão foi revogada, e a recusa diz o que houve.
    Revogada {
        /// A que foi pedida.
        versao: Versao,
        /// Qual é o defeito.
        defeito: String,
        /// Onde ele foi corrigido, quando a lista diz.
        corrigida_em: Option<String>,
        /// Se estava hospedando ou entrando.
        ato: Ato,
    },
    /// Esta versão existe e não está instalada nesta máquina.
    ///
    /// Não é erro: é o caminho normal para entrar num servidor que roda uma
    /// versão que esta máquina nunca usou. A tela oferece instalar.
    NaoInstalada {
        /// A que foi pedida.
        versao: Versao,
        /// O que a instalação vai precisar baixar já é sabido daqui.
        tem_pacote_para_este_sistema: bool,
    },
    /// A pasta da versão existe e a instalação não terminou.
    ///
    /// Distinta de [`Self::NaoInstalada`] porque a coisa a fazer é outra:
    /// apagar o que sobrou e instalar de novo, em vez de supor que baixar
    /// resolve.
    InstalacaoIncompleta {
        /// A que foi pedida.
        versao: Versao,
    },
    /// A instalação está marcada como completa e o executável não está lá.
    ///
    /// Alguém apagou, um antivírus levou, um disco falhou.
    ExecutavelAusente {
        /// A que foi pedida.
        versao: Versao,
        /// Onde ele deveria estar.
        onde: PathBuf,
    },
    /// O manifesto não diz qual é o executável desta versão neste sistema.
    ExecutavelNaoDeclarado {
        /// A que foi pedida.
        versao: Versao,
        /// O alvo pedido.
        alvo: String,
    },
    /// O manifesto diz um executável que não fica dentro da instalação.
    ///
    /// Um caminho absoluto trocaria a instalação inteira por outro programa;
    /// um com `..` resolveria para o binário de outra versão. O manifesto
    /// **não é assinado** — ver `docs/versoes-lado-a-lado.md` —, então quem o
    /// serve não pode escolher o que roda. Separada de
    /// [`Self::ExecutavelNaoDeclarado`] porque a coisa a fazer é outra: ali
    /// falta publicar o campo, aqui o campo publicado está errado.
    ExecutavelInvalido {
        /// A que foi pedida.
        versao: Versao,
        /// O alvo pedido.
        alvo: String,
        /// Por quê.
        motivo: ExecutavelInvalido,
    },
    /// O servidor anunciou uma versão e um protocolo que não combinam.
    ///
    /// Uma das duas pontas está mentindo ou está desatualizada, e entrar
    /// assim é o defeito que a subida do `PROTOCOL_VERSION` para 3 existe para
    /// impedir: dois builds dizendo o mesmo número e falando vocabulários
    /// diferentes.
    UnidadeIncoerente {
        /// A versão anunciada.
        versao: Versao,
        /// O protocolo que o servidor anunciou.
        anunciado: u8,
        /// O que o manifesto diz que aquela versão fala.
        do_manifesto: u8,
    },
}

/// A versão escolhida, com tudo o que a inicialização precisa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersaoResolvida {
    /// Qual é.
    pub versao: Versao,
    /// O executável a rodar.
    pub executavel: PathBuf,
    /// O diretório de dados **daquela versão**.
    pub dados: PathBuf,
    /// O que ela fala, quando o manifesto declara.
    pub unidade: Option<Unidade>,
}

/// Resolve um pedido contra um manifesto e um depósito.
///
/// Guarda a mesma resposta para os três atos, menos a revogação — ver [`Ato`].
#[derive(Debug, Clone)]
pub struct Resolvedor<'a> {
    manifesto: &'a Manifesto,
    deposito: &'a Deposito,
    alvo: String,
    revogacoes: &'a Vigente,
}

impl<'a> Resolvedor<'a> {
    /// Um resolvedor para este manifesto, este depósito, este sistema e a
    /// lista de revogação que vale.
    ///
    /// `alvo` é a nomenclatura do atualizador — `darwin-aarch64`,
    /// `windows-x86_64`, `linux-x86_64`. Vem de fora porque este crate não
    /// decide em que máquina está rodando: quem instala para outro sistema
    /// (e o `empacotar/` deste repositório faz isso) precisa poder dizer qual.
    ///
    /// `revogacoes` é o [`Vigente`] que saiu de
    /// [`Deposito::conciliar_revogacoes`](crate::Deposito::conciliar_revogacoes).
    /// Ele é exigido, e não opcional, pela razão do cabeçalho deste módulo:
    /// era a única defesa do launcher que se podia perder por esquecimento.
    #[must_use]
    pub fn novo(
        manifesto: &'a Manifesto,
        deposito: &'a Deposito,
        alvo: impl Into<String>,
        revogacoes: &'a Vigente,
    ) -> Self {
        Self {
            manifesto,
            deposito,
            alvo: alvo.into(),
            revogacoes,
        }
    }

    /// Qual versão roda, ou por que não.
    ///
    /// # Errors
    ///
    /// [`Recusa`], uma variante por motivo.
    pub fn resolver(&self, pedido: &Pedido, ato: Ato) -> Result<VersaoResolvida, Recusa> {
        let publicacao = match pedido {
            Pedido::MaisNova => self.manifesto.mais_nova(),
            Pedido::Exata(versao) => {
                self.manifesto
                    .publicacao(versao)
                    .ok_or_else(|| Recusa::NaoPublicada {
                        versao: versao.clone(),
                    })?
            }
        };
        let versao = publicacao.version.clone();

        // A revogação vem antes de tudo o que toca em disco: uma versão
        // revogada não é hospedável nem alcançável, e não há por que conferir
        // se ela está instalada para depois recusá-la.
        if ato.confere_revogacao() {
            if let Some(r) = self.revogacoes.revogacao_de(versao.como_texto()) {
                return Err(Recusa::Revogada {
                    versao,
                    defeito: r.defect.clone(),
                    corrigida_em: r.fixed_in.clone(),
                    ato,
                });
            }
        }

        let pacote = publicacao.platforms.get(&self.alvo);

        if !self.deposito.esta_instalada(&versao) {
            let pasta = self.deposito.pasta_da_versao(&versao);
            return Err(if pasta.exists() {
                Recusa::InstalacaoIncompleta { versao }
            } else {
                Recusa::NaoInstalada {
                    versao,
                    tem_pacote_para_este_sistema: pacote.is_some(),
                }
            });
        }

        // O caminho do executável passa pela mesma postura que o
        // identificador de versão: ele vira caminho em disco, e quem o escreve
        // é um manifesto sem assinatura. Ver [`crate::executavel`].
        let relativo = match pacote.map(crate::manifesto::Pacote::executavel) {
            Some(Ok(caminho)) => caminho,
            None | Some(Err(ExecutavelInvalido::NaoDeclarado)) => {
                return Err(Recusa::ExecutavelNaoDeclarado {
                    versao,
                    alvo: self.alvo.clone(),
                })
            }
            Some(Err(motivo)) => {
                return Err(Recusa::ExecutavelInvalido {
                    versao,
                    alvo: self.alvo.clone(),
                    motivo,
                })
            }
        };
        let executavel = self
            .deposito
            .pasta_da_versao(&versao)
            .join(relativo.como_caminho());
        if !executavel.exists() {
            return Err(Recusa::ExecutavelAusente {
                versao,
                onde: executavel,
            });
        }

        Ok(VersaoResolvida {
            dados: self.deposito.pasta_de_dados(&versao),
            unidade: publicacao.unit,
            versao,
            executavel,
        })
    }

    /// A conferência de coerência do ADR 0045, para entrar num servidor.
    ///
    /// # Errors
    ///
    /// [`Recusa::UnidadeIncoerente`] quando o protocolo anunciado não é o que
    /// o manifesto atribui àquela versão. Uma versão cujo manifesto não
    /// declara unidade passa: não há com o que conferir, e inventar uma
    /// resposta seria pior que não ter uma.
    pub fn conferir_coerencia(
        &self,
        versao: &Versao,
        protocolo_anunciado: u8,
    ) -> Result<(), Recusa> {
        let Some(publicacao) = self.manifesto.publicacao(versao) else {
            return Err(Recusa::NaoPublicada {
                versao: versao.clone(),
            });
        };
        let Some(unidade) = publicacao.unit else {
            return Ok(());
        };
        if unidade.protocol != protocolo_anunciado {
            return Err(Recusa::UnidadeIncoerente {
                versao: versao.clone(),
                anunciado: protocolo_anunciado,
                do_manifesto: unidade.protocol,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::cenario::{
        chave, manifesto, manifesto_com_executavel, pacote, pasta, vigente, DesempacotadorDeTeste,
        ALVO, EXECUTAVEL,
    };
    use crate::deposito::Deposito;

    fn instalar(deposito: &Deposito, manifesto: &Manifesto, versao: &str) {
        let v = Versao::nova(versao).unwrap();
        let p = manifesto.publicacao(&v).unwrap().clone();
        deposito
            .instalar(&p, ALVO, &pacote(versao), &chave(), &DesempacotadorDeTeste)
            .expect("instalar no cenário");
    }

    /// **O critério de aceite:** a versão selecionada determina o executável e
    /// o diretório de dados.
    #[test]
    fn cada_versao_resolve_para_o_executavel_e_os_dados_dela() {
        let raiz = pasta("resolver-duas");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        instalar(&deposito, &m, "2.0.0-teste");
        instalar(&deposito, &m, "1.0.0-teste");
        let sem_revogacao = vigente(&m, &deposito);
        let r = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao);

        let nova = r.resolver(&Pedido::MaisNova, Ato::Abrir).unwrap();
        let velha = r
            .resolver(
                &Pedido::Exata(Versao::nova("1.0.0-teste").unwrap()),
                Ato::Abrir,
            )
            .unwrap();

        assert_eq!(nova.versao.como_texto(), "2.0.0-teste");
        assert_eq!(velha.versao.como_texto(), "1.0.0-teste");
        assert_ne!(nova.executavel, velha.executavel);
        assert_ne!(nova.dados, velha.dados);
        assert!(nova.executavel.ends_with(EXECUTAVEL));
        assert!(nova.dados.ends_with("dados/2.0.0-teste"));
        assert!(velha.dados.ends_with("dados/1.0.0-teste"));
        // E o executável de cada uma é o pacote daquela versão, não do outro.
        assert_eq!(
            std::fs::read(&velha.executavel).unwrap(),
            pacote("1.0.0-teste")
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **O critério de aceite:** versão indisponível é recusada com motivo.
    #[test]
    fn uma_versao_que_o_manifesto_nao_lista_e_recusada_por_isso() {
        let raiz = pasta("nao-publicada");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let sem_revogacao = vigente(&m, &deposito);
        let r = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao);
        let inventada = Versao::nova("9.9.9-teste").unwrap();

        assert_eq!(
            r.resolver(&Pedido::Exata(inventada.clone()), Ato::Entrar),
            Err(Recusa::NaoPublicada { versao: inventada })
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_versao_publicada_e_nao_instalada_diz_que_ha_pacote_para_este_sistema() {
        let raiz = pasta("nao-instalada");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        let sem_revogacao = vigente(&m, &deposito);

        assert_eq!(
            Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao)
                .resolver(&Pedido::MaisNova, Ato::Entrar),
            Err(Recusa::NaoInstalada {
                versao: versao.clone(),
                tem_pacote_para_este_sistema: true
            })
        );
        assert_eq!(
            Resolvedor::novo(&m, &deposito, "outro-sistema", &sem_revogacao)
                .resolver(&Pedido::MaisNova, Ato::Entrar),
            Err(Recusa::NaoInstalada {
                versao,
                tem_pacote_para_este_sistema: false
            })
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_instalacao_pela_metade_pede_outra_coisa_que_uma_nao_instalada() {
        let raiz = pasta("pela-metade");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        std::fs::create_dir_all(deposito.pasta_da_versao(&versao)).unwrap();
        let sem_revogacao = vigente(&m, &deposito);

        assert_eq!(
            Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao)
                .resolver(&Pedido::MaisNova, Ato::Abrir),
            Err(Recusa::InstalacaoIncompleta { versao })
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_executavel_que_sumiu_depois_de_instalado_e_recusado_por_isso() {
        let raiz = pasta("executavel-sumiu");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        instalar(&deposito, &m, "1.0.0-teste");
        let versao = Versao::nova("1.0.0-teste").unwrap();
        std::fs::remove_file(deposito.pasta_da_versao(&versao).join(EXECUTAVEL)).unwrap();
        let sem_revogacao = vigente(&m, &deposito);

        assert!(matches!(
            Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao)
                .resolver(&Pedido::MaisNova, Ato::Abrir),
            Err(Recusa::ExecutavelAusente { .. })
        ));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **O critério de aceite:** versão revogada é recusada com motivo — e o
    /// motivo diz qual é o defeito e qual versão o corrige.
    #[test]
    fn uma_versao_revogada_nao_hospeda_e_nao_entra() {
        let raiz = pasta("revogada");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(
            &["2.0.0-teste", "1.0.0-teste"],
            &[(
                "1.0.0-teste",
                "a porta ficava aberta para quem não bateu",
                Some("2.0.0-teste"),
            )],
        );
        instalar(&deposito, &m, "1.0.0-teste");
        // A revogação chega pelo manifesto e atravessa a conciliação: é assim
        // que ela alcança a resolução, e não por um atalho.
        let revogando = vigente(&m, &deposito);
        let r = Resolvedor::novo(&m, &deposito, ALVO, &revogando);
        let versao = Versao::nova("1.0.0-teste").unwrap();

        for ato in [Ato::Hospedar, Ato::Entrar] {
            assert_eq!(
                r.resolver(&Pedido::Exata(versao.clone()), ato),
                Err(Recusa::Revogada {
                    versao: versao.clone(),
                    defeito: "a porta ficava aberta para quem não bateu".to_owned(),
                    corrigida_em: Some("2.0.0-teste".to_owned()),
                    ato,
                }),
                "com {ato:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// A regra do ADR 0045 que custa: revogação é conferida em atos, e abrir
    /// não é um ato. As conversas de quem estava na versão furada continuam
    /// legíveis.
    #[test]
    fn uma_versao_revogada_continua_abrindo_para_ler_o_que_ficou_la() {
        let raiz = pasta("revogada-abre");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(
            &["2.0.0-teste", "1.0.0-teste"],
            &[("1.0.0-teste", "defeito de teste", Some("2.0.0-teste"))],
        );
        instalar(&deposito, &m, "1.0.0-teste");
        let versao = Versao::nova("1.0.0-teste").unwrap();

        let revogando = vigente(&m, &deposito);
        let aberta = Resolvedor::novo(&m, &deposito, ALVO, &revogando)
            .resolver(&Pedido::Exata(versao.clone()), Ato::Abrir)
            .expect("abrir uma versão revogada é permitido, e é decisão do ADR 0045");
        assert_eq!(aberta.versao, versao);
        assert!(!Ato::Abrir.confere_revogacao());
        assert!(Ato::Hospedar.confere_revogacao() && Ato::Entrar.confere_revogacao());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_versao_nao_revogada_hospeda_com_a_lista_na_mao() {
        let raiz = pasta("nao-revogada");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(
            &["2.0.0-teste", "1.0.0-teste"],
            &[("1.0.0-teste", "defeito de teste", Some("2.0.0-teste"))],
        );
        instalar(&deposito, &m, "2.0.0-teste");
        let revogando = vigente(&m, &deposito);
        assert!(Resolvedor::novo(&m, &deposito, ALVO, &revogando)
            .resolver(&Pedido::MaisNova, Ato::Hospedar)
            .is_ok());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// A conferência de coerência em que o ADR 0045 transforma a negociação.
    #[test]
    fn um_servidor_que_anuncia_versao_e_protocolo_que_nao_batem_e_recusado() {
        let raiz = pasta("coerencia");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let sem_revogacao = vigente(&m, &deposito);
        let r = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao);
        let versao = Versao::nova("1.0.0-teste").unwrap();

        // O cenário declara `protocol: 3` para toda versão de teste.
        assert_eq!(r.conferir_coerencia(&versao, 3), Ok(()));
        assert_eq!(
            r.conferir_coerencia(&versao, 2),
            Err(Recusa::UnidadeIncoerente {
                versao,
                anunciado: 2,
                do_manifesto: 3
            })
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A lista que decide é a conciliada, e não o bloco do manifesto.**
    ///
    /// O resolvedor não tem caminho nenhum até `manifesto.revogacoes()`, e é o
    /// que fecha o buraco de esquecer a conciliação: antes, um `Resolvedor`
    /// construído sem lista caía no bloco do manifesto, e um manifesto servido
    /// sem bloco virava «nada revogado».
    ///
    /// Aqui as duas divergem de propósito — manifesto que revoga, máquina que
    /// nunca viu lista nenhuma. Fora do teste a divergência não se monta: só a
    /// conciliação produz um [`Vigente`], e conciliar este manifesto traria a
    /// revogação, como prova o teste logo acima.
    #[test]
    fn a_resolucao_le_a_lista_conciliada_e_nao_o_bloco_do_manifesto() {
        let raiz = pasta("so-a-conciliada");
        let deposito = Deposito::em(&raiz);
        let revogando = manifesto(
            &["2.0.0-teste", "1.0.0-teste"],
            &[("1.0.0-teste", "defeito de teste", Some("2.0.0-teste"))],
        );
        instalar(&deposito, &revogando, "1.0.0-teste");
        let versao = Versao::nova("1.0.0-teste").unwrap();

        // Uma máquina limpa, que nunca viu lista nenhuma.
        let limpa = pasta("so-a-conciliada-limpa");
        let nunca_viu = vigente(&manifesto(&["1.0.0-teste"], &[]), &Deposito::em(&limpa));
        assert!(nunca_viu.lista().is_none());

        assert!(
            Resolvedor::novo(&revogando, &deposito, ALVO, &nunca_viu)
                .resolver(&Pedido::Exata(versao.clone()), Ato::Hospedar)
                .is_ok(),
            "a resolução não pode ir buscar a lista no manifesto por conta própria"
        );

        // E com a lista conciliada do mesmo manifesto, recusa.
        let conciliada = vigente(&revogando, &deposito);
        assert!(matches!(
            Resolvedor::novo(&revogando, &deposito, ALVO, &conciliada)
                .resolver(&Pedido::Exata(versao), Ato::Hospedar),
            Err(Recusa::Revogada { .. })
        ));
        let _ = std::fs::remove_dir_all(&raiz);
        let _ = std::fs::remove_dir_all(&limpa);
    }

    /// A ponta a ponta da defesa contra esconder uma revogação: o manifesto
    /// de hoje vem **sem** lista, a máquina já viu uma, e hospedar continua
    /// sendo recusado com o motivo certo.
    #[test]
    fn uma_revogacao_ja_vista_continua_recusando_com_o_manifesto_de_hoje_sem_lista() {
        let raiz = pasta("revogacao-guardada");
        let deposito = Deposito::em(&raiz);
        let chave = chave();

        // Ontem: o manifesto trazia a lista, e ela ficou guardada.
        let com_lista = manifesto(
            &["2.0.0-teste", "1.0.0-teste"],
            &[("1.0.0-teste", "defeito de teste", Some("2.0.0-teste"))],
        );
        deposito
            .conciliar_revogacoes(com_lista.envelope_de_revogacoes(), &chave)
            .unwrap();

        // Hoje: alguém serve o manifesto de antes da revogação.
        let sem_lista = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        assert!(sem_lista.revogacoes().is_none());
        instalar(&deposito, &sem_lista, "1.0.0-teste");

        let ainda_vale = deposito
            .conciliar_revogacoes(sem_lista.envelope_de_revogacoes(), &chave)
            .unwrap();
        let r = Resolvedor::novo(&sem_lista, &deposito, ALVO, &ainda_vale);
        let versao = Versao::nova("1.0.0-teste").unwrap();

        assert_eq!(
            r.resolver(&Pedido::Exata(versao.clone()), Ato::Hospedar),
            Err(Recusa::Revogada {
                versao: versao.clone(),
                defeito: "defeito de teste".to_owned(),
                corrigida_em: Some("2.0.0-teste".to_owned()),
                ato: Ato::Hospedar,
            })
        );
        // E abrir continua permitido, como sempre.
        assert!(r.resolver(&Pedido::Exata(versao), Ato::Abrir).is_ok());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_manifesto_sem_unidade_declarada_nao_inventa_a_conferencia() {
        let raiz = pasta("sem-unidade");
        let deposito = Deposito::em(&raiz);
        // O formato publicado hoje, que não declara unidade nenhuma.
        let json = r#"{"version":"1.0.0-teste","platforms":{}}"#;
        let m = Manifesto::ler(json.as_bytes(), &chave()).unwrap();
        let sem_revogacao = vigente(&m, &deposito);
        let r = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao);
        assert_eq!(
            r.conferir_coerencia(&Versao::nova("1.0.0-teste").unwrap(), 99),
            Ok(())
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A metade do critério de aceite que o manifesto não assinado ameaça:**
    /// é a versão selecionada quem determina o executável, e não quem serve o
    /// manifesto. Um caminho absoluto faria `join` descartar a instalação
    /// inteira e devolver `/bin/sh` para ser iniciado.
    #[test]
    fn um_executavel_absoluto_no_manifesto_nao_substitui_a_instalacao() {
        let raiz = pasta("executavel-absoluto");
        let deposito = Deposito::em(&raiz);
        instalar(&deposito, &manifesto(&["1.0.0-teste"], &[]), "1.0.0-teste");

        // A mesma versão instalada, com o manifesto trocado no caminho.
        for hostil in ["/bin/sh", "\\Windows\\System32\\cmd.exe"] {
            let m = manifesto_com_executavel(&["1.0.0-teste"], Some(hostil));
            let sem_revogacao = vigente(&m, &deposito);
            let recusa = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao)
                .resolver(&Pedido::MaisNova, Ato::Abrir)
                .expect_err("um executável fora da instalação tem de ser recusado");
            assert!(
                matches!(
                    recusa,
                    Recusa::ExecutavelInvalido {
                        motivo: ExecutavelInvalido::Absoluto { .. },
                        ..
                    }
                ),
                "com `{hostil}` veio {recusa:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// O outro caminho da mesma porta: `..` sai da pasta da versão escolhida e
    /// cai na de outra versão instalada.
    #[test]
    fn um_executavel_com_travessia_nao_alcanca_o_binario_de_outra_versao() {
        let raiz = pasta("executavel-travessia");
        let deposito = Deposito::em(&raiz);
        let bom = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        instalar(&deposito, &bom, "2.0.0-teste");
        instalar(&deposito, &bom, "1.0.0-teste");

        let hostil = manifesto_com_executavel(
            &["2.0.0-teste"],
            Some(&format!("../1.0.0-teste/{EXECUTAVEL}")),
        );
        let sem_revogacao = vigente(&hostil, &deposito);
        let recusa = Resolvedor::novo(&hostil, &deposito, ALVO, &sem_revogacao)
            .resolver(&Pedido::MaisNova, Ato::Abrir)
            .expect_err("um `..` tem de ser recusado");
        assert!(
            matches!(
                recusa,
                Recusa::ExecutavelInvalido {
                    motivo: ExecutavelInvalido::Travessia { .. },
                    ..
                }
            ),
            "veio {recusa:?}"
        );

        // E o caminho que a resolução boa devolve continua sendo o binário da
        // versão pedida — a prova pelo conteúdo, e não pelo nome.
        let resolvida = Resolvedor::novo(&bom, &deposito, ALVO, &sem_revogacao)
            .resolver(&Pedido::MaisNova, Ato::Abrir)
            .unwrap();
        assert!(resolvida
            .executavel
            .starts_with(deposito.pasta_da_versao(&Versao::nova("2.0.0-teste").unwrap())));
        assert_eq!(
            std::fs::read(&resolvida.executavel).unwrap(),
            pacote("2.0.0-teste")
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **O manifesto publicado hoje**, que não traz `executable` nenhum: é o
    /// único caminho que toda máquina encontra agora, e a recusa nomeia o alvo
    /// em vez de deixar o launcher adivinhar o que iniciar. Lacuna 3 de
    /// `docs/versoes-lado-a-lado.md`.
    #[test]
    fn sem_executavel_declarado_a_recusa_nomeia_a_versao_e_o_alvo() {
        let raiz = pasta("sem-executavel-declarado");
        let deposito = Deposito::em(&raiz);
        instalar(&deposito, &manifesto(&["1.0.0-teste"], &[]), "1.0.0-teste");

        let sem = manifesto_com_executavel(&["1.0.0-teste"], None);
        let sem_revogacao = vigente(&sem, &deposito);
        assert_eq!(
            Resolvedor::novo(&sem, &deposito, ALVO, &sem_revogacao)
                .resolver(&Pedido::MaisNova, Ato::Abrir),
            Err(Recusa::ExecutavelNaoDeclarado {
                versao: Versao::nova("1.0.0-teste").unwrap(),
                alvo: ALVO.to_owned(),
            })
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
