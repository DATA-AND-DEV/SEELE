//! O depósito em disco: onde as versões moram, e em que ordem elas chegam lá.
//!
//! ADR 0026, e o ADR 0045 o mantém sem emenda: *«o pacote é baixado inteiro
//! para a memória e a assinatura é conferida **antes** de qualquer arquivo
//! instalado ser tocado»*. Este módulo é essa ordem, escrita como código:
//!
//! 1. **confere** o conteúdo do pacote — a soma e a assinatura — com o pacote
//!    ainda em memória e nada em disco;
//! 2. **prepara de lado**, num diretório temporário que não é a instalação;
//! 3. **marca a preparação como completa**, e o marcador é escrito por último;
//! 4. **publica**, movendo o diretório preparado para o nome definitivo.
//!
//! Uma falha em qualquer dos quatro deixa as instalações anteriores exatamente
//! como estavam. É a propriedade que o ADR 0026 comprou, e agora ela vale para
//! cada versão em vez de para a única que havia.
//!
//! # O que este módulo não sabe desempacotar
//!
//! Nada. Um `.app.tar.gz`, um `.exe` e um `.deb` se instalam de três jeitos, e
//! escolher entre eles não é regra de launcher. Quem desempacota entra por
//! [`Desempacotador`], e recebe o destino já preparado — nunca a instalação de
//! verdade. Um desempacotador que falhe no meio suja o preparo e não alcança o
//! que já estava instalado.
//!
//! # A forma em disco
//!
//! ```text
//! <raiz>/
//! ├── versoes/0.10.5-1/        a instalação
//! ├── versoes/0.10.5-1/INSTALADA   o marcador, escrito por último
//! ├── dados/0.10.5-1/          os dados daquela versão (ver `crate::dados`)
//! ├── preparo/<algo>/          área de preparo, apagada ao fim
//! └── revogacoes-vistas        a memória do `crate::revogacao`
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::assinatura::{Chave, FalhaDeAssinatura};
use crate::executavel::{CaminhoDoExecutavel, ExecutavelInvalido};
use crate::manifesto::Publicacao;
use crate::revogacao::{Envelope, FalhaDaLista, ListaDeRevogacao, Memoria, Vigente};
use crate::versao::Versao;

/// O nome do marcador que diz «esta instalação chegou inteira».
///
/// Escrito por último dentro do preparo, antes de o diretório ser publicado.
/// Com a publicação sendo um `rename`, um diretório meio-escrito não deveria
/// aparecer no lugar definitivo — mas «não deveria» é a frase que o
/// `CLAUDE.md` chama de *«existir não é funcionar»*. O marcador é o que
/// transforma a suposição em pergunta respondível, e o que dá a
/// [`crate::resolucao`] a recusa certa em vez de um executável que não abre.
const MARCADOR: &str = "INSTALADA";

/// O arquivo onde a última lista de revogação vista fica guardada.
///
/// O envelope inteiro — bytes e assinatura —, e não só o carimbo. Guardar só
/// o carimbo protegeria contra uma lista que **recua** e não contra uma que
/// **some**, e sumir é o ataque mais barato dos dois: basta servir o manifesto
/// de antes da revogação.
const REVOGACOES_VISTAS: &str = "revogacoes-vistas.json";

/// Quem sabe transformar um pacote conferido numa instalação.
///
/// O destino que chega aqui é **sempre** um diretório de preparo, nunca uma
/// instalação existente. Falhar é permitido e previsto; o que não é permitido
/// é escrever fora de `destino`.
pub trait Desempacotador {
    /// Materializa o pacote dentro de `destino`, que já existe e está vazio.
    ///
    /// # Errors
    ///
    /// Qualquer falha de leitura, escrita ou formato. A casca a traduz para
    /// «a troca dos arquivos falhou», que é a mesma frase de sempre.
    fn desempacotar(&self, pacote: &[u8], destino: &Path) -> std::io::Result<()>;
}

/// Por que uma instalação não aconteceu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaAoInstalar {
    /// O manifesto não traz pacote para este sistema e esta arquitetura.
    SemPacoteParaEsteSistema {
        /// O alvo pedido, na nomenclatura do atualizador.
        alvo: String,
    },
    /// O pacote não tem o tamanho e o conteúdo que o manifesto declarou.
    ///
    /// Quase sempre um download truncado, e a coisa a fazer é baixar de novo.
    /// Separado de [`Self::AssinaturaRecusada`] de propósito: ali a resposta é
    /// **não** tentar de novo, e confundir as duas manda a pessoa insistir
    /// contra um pacote adulterado ou desistir de um pacote truncado.
    ConteudoDivergente {
        /// O que o manifesto prometeu, em hexadecimal.
        esperado: String,
        /// O que chegou.
        obtido: String,
    },
    /// O pacote não foi assinado por este projeto.
    AssinaturaRecusada(FalhaDeAssinatura),
    /// O manifesto lista este pacote e não traz a assinatura dele.
    ///
    /// Separado de [`Self::AssinaturaRecusada`] pelo mesmo motivo que separa a
    /// soma divergente da assinatura recusada: ali houve conferência e ela
    /// reprovou, e a resposta é não tentar de novo; aqui não houve o que
    /// conferir, e o defeito é do manifesto — a coisa a fazer é avisar o
    /// projeto. Instalar assim seria conferir contra uma assinatura vazia, que
    /// é não conferir.
    AssinaturaNaoDeclarada,
    /// O manifesto não diz qual é o executável desta instalação.
    ///
    /// É o estado do manifesto publicado hoje — ver a lacuna 3 de
    /// `docs/versoes-lado-a-lado.md`. Instalar sem saber o que iniciar deixaria
    /// em disco uma instalação que só falha na hora de abrir.
    ExecutavelNaoDeclarado,
    /// O manifesto diz um executável que não fica dentro da instalação.
    ///
    /// Recusado **antes** de qualquer byte ir para o disco, junto da soma e da
    /// assinatura: um caminho absoluto ou com `..` faria a instalação publicar
    /// como «o executável desta versão» um programa que não é dela. Ver
    /// [`crate::executavel`].
    ExecutavelInvalido(ExecutavelInvalido),
    /// A preparação ou a publicação em disco falhou.
    NaoInstalei {
        /// O que o sistema disse. Para o log.
        detalhe: String,
    },
}

/// Uma instalação que existe em disco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instalacao {
    /// De qual versão.
    pub versao: Versao,
    /// A raiz da instalação.
    pub raiz: PathBuf,
    /// O executável, caminho completo.
    pub executavel: PathBuf,
}

/// O depósito de versões de uma máquina.
#[derive(Debug, Clone)]
pub struct Deposito {
    raiz: PathBuf,
}

impl Deposito {
    /// Um depósito com raiz nesta pasta. Não toca em disco.
    #[must_use]
    pub fn em(raiz: impl Into<PathBuf>) -> Self {
        Self { raiz: raiz.into() }
    }

    /// A raiz do depósito.
    #[must_use]
    pub fn raiz(&self) -> &Path {
        &self.raiz
    }

    /// Onde esta versão fica instalada.
    #[must_use]
    pub fn pasta_da_versao(&self, versao: &Versao) -> PathBuf {
        self.raiz.join("versoes").join(versao.como_texto())
    }

    /// Onde ficam os dados desta versão.
    #[must_use]
    pub fn pasta_de_dados(&self, versao: &Versao) -> PathBuf {
        self.raiz.join("dados").join(versao.como_texto())
    }

    /// Esta versão está instalada e completa?
    ///
    /// Uma instalação sem o marcador é uma instalação que não terminou, e a
    /// resposta é `false` — não «existe a pasta, então deve dar».
    #[must_use]
    pub fn esta_instalada(&self, versao: &Versao) -> bool {
        self.pasta_da_versao(versao).join(MARCADOR).is_file()
    }

    /// Que versões estão instaladas e completas, em ordem de nome.
    ///
    /// Ordem de nome porque é a única que o disco oferece, e ela serve para
    /// listar. **Quem diz qual é a mais nova é o manifesto**, nunca esta lista.
    #[must_use]
    pub fn instaladas(&self) -> Vec<Versao> {
        let mut achadas = Vec::new();
        let Ok(entradas) = std::fs::read_dir(self.raiz.join("versoes")) else {
            return achadas;
        };
        for entrada in entradas.flatten() {
            let nome = entrada.file_name();
            let Some(nome) = nome.to_str() else { continue };
            let Ok(versao) = Versao::nova(nome) else {
                continue;
            };
            if self.esta_instalada(&versao) {
                achadas.push(versao);
            }
        }
        // Por nome, e escrito por extenso: `Versao` não tem `Ord`, porque uma
        // ordem de versões que parecesse semântica daria a resposta errada
        // sobre qual é a mais nova. Ver [`crate::versao::Versao`].
        achadas.sort_by(|esta, aquela| esta.como_texto().cmp(aquela.como_texto()));
        achadas
    }

    /// Instala uma versão a partir do pacote já baixado.
    ///
    /// `pacote` são os bytes inteiros, em memória. A ordem dos quatro passos
    /// está no cabeçalho do módulo, e ela é o contrato: **nada em disco é
    /// tocado antes de a conferência passar**.
    ///
    /// Instalar de novo uma versão já completa não faz nada e devolve a
    /// instalação que já havia. É de propósito: reinstalar por cima de uma
    /// instalação boa é a operação que pode quebrá-la, e não há motivo para
    /// arriscá-la.
    ///
    /// # Errors
    ///
    /// [`FalhaAoInstalar`], uma variante por motivo.
    pub fn instalar(
        &self,
        publicacao: &Publicacao,
        alvo: &str,
        pacote: &[u8],
        chave: &Chave,
        desempacotador: &dyn Desempacotador,
    ) -> Result<Instalacao, FalhaAoInstalar> {
        let entrada = publicacao.platforms.get(alvo).ok_or_else(|| {
            FalhaAoInstalar::SemPacoteParaEsteSistema {
                alvo: alvo.to_owned(),
            }
        })?;
        let executavel_relativo = entrada.executavel().map_err(|motivo| match motivo {
            ExecutavelInvalido::NaoDeclarado => FalhaAoInstalar::ExecutavelNaoDeclarado,
            outro => FalhaAoInstalar::ExecutavelInvalido(outro),
        })?;

        // ------- 1. conferir, com o pacote em memória e nada em disco.
        if let Some(esperado) = &entrada.sha256 {
            let obtido = resumo_em_hexadecimal(pacote);
            if !esperado.eq_ignore_ascii_case(&obtido) {
                return Err(FalhaAoInstalar::ConteudoDivergente {
                    esperado: esperado.clone(),
                    obtido,
                });
            }
        }
        let assinatura = entrada
            .assinatura()
            .ok_or(FalhaAoInstalar::AssinaturaNaoDeclarada)?;
        chave
            .conferir(pacote, assinatura)
            .map_err(FalhaAoInstalar::AssinaturaRecusada)?;

        let definitiva = self.pasta_da_versao(&publicacao.version);
        if self.esta_instalada(&publicacao.version) {
            return Ok(Instalacao {
                versao: publicacao.version.clone(),
                executavel: definitiva.join(executavel_relativo.como_caminho()),
                raiz: definitiva,
            });
        }

        // ------- 2. preparar de lado.
        let preparo = self.abrir_preparo(&publicacao.version)?;
        let resultado = self.preparar_e_publicar(
            &preparo,
            &definitiva,
            publicacao,
            &executavel_relativo,
            pacote,
            desempacotador,
        );
        // O preparo some, tenha dado certo ou errado. Um preparo esquecido é
        // uma cópia inteira do produto ocupando disco sem ninguém saber.
        let _ = std::fs::remove_dir_all(&preparo);
        resultado?;

        Ok(Instalacao {
            versao: publicacao.version.clone(),
            executavel: definitiva.join(executavel_relativo.como_caminho()),
            raiz: definitiva,
        })
    }

    /// Os passos 2 a 4, separados só para o preparo poder ser apagado sempre.
    fn preparar_e_publicar(
        &self,
        preparo: &Path,
        definitiva: &Path,
        publicacao: &Publicacao,
        executavel_relativo: &CaminhoDoExecutavel,
        pacote: &[u8],
        desempacotador: &dyn Desempacotador,
    ) -> Result<(), FalhaAoInstalar> {
        desempacotador
            .desempacotar(pacote, preparo)
            .map_err(em_falha)?;

        // O executável tem de existir **antes** de a instalação ser publicada.
        // Descobrir que ele não existe depois é descobrir na hora de abrir, na
        // máquina de outra pessoa.
        if !preparo.join(executavel_relativo.como_caminho()).exists() {
            return Err(FalhaAoInstalar::NaoInstalei {
                detalhe: format!(
                    "o pacote não trouxe `{executavel_relativo}`, que o manifesto declara"
                ),
            });
        }

        // ------- 3. o marcador, por último.
        std::fs::write(
            preparo.join(MARCADOR),
            format!("{}\n", publicacao.version.como_texto()),
        )
        .map_err(em_falha)?;

        // ------- 4. publicar.
        if let Some(pai) = definitiva.parent() {
            std::fs::create_dir_all(pai).map_err(em_falha)?;
        }
        // **Uma instalação que não terminou não é uma instalação anterior a
        // preservar**, e é o que o marcador permite distinguir. Sem esta
        // linha, um `rename` sobre um diretório existente e não vazio falha —
        // e a pasta meio-escrita de uma tentativa antiga trancaria toda
        // tentativa seguinte, para sempre, com «a troca dos arquivos falhou».
        // Quem tem marcador nunca chega aqui: o caminho de cima já devolveu.
        if definitiva.exists() {
            // **E completa não se apaga.** Entre o `esta_instalada` lá de cima
            // e esta linha cabe outro processo publicando a mesma versão. A
            // instalação dele é tão boa quanto esta seria — mesmo pacote,
            // mesma assinatura — e pode já estar aberta por alguém. Varrer o
            // que está completo para pôr uma cópia idêntica no lugar é
            // arriscar a única coisa que não se pode perder aqui.
            if self.esta_instalada(&publicacao.version) {
                return Ok(());
            }
            std::fs::remove_dir_all(definitiva).map_err(em_falha)?;
        }
        match std::fs::rename(preparo, definitiva) {
            Ok(()) => Ok(()),
            // Outro processo publicou a mesma versão enquanto este preparava.
            // A instalação dele é tão boa quanto esta seria, e sobrescrevê-la
            // é a única forma de estragá-la.
            Err(_) if self.esta_instalada(&publicacao.version) => Ok(()),
            Err(erro) => Err(em_falha(erro)),
        }
    }

    /// Um diretório de preparo vazio e só desta tentativa.
    fn abrir_preparo(&self, versao: &Versao) -> Result<PathBuf, FalhaAoInstalar> {
        let pasta = self.raiz.join("preparo");
        std::fs::create_dir_all(&pasta).map_err(em_falha)?;
        // O identificador do processo mais o relógio: dois processos que
        // instalem a mesma versão ao mesmo tempo não podem dividir a área de
        // preparo, ou um apaga o arquivo do outro pela metade. Foi assim que os
        // testes de MOD deste repositório já se atrapalharam uma vez.
        let marca = format!(
            "{}-{}-{}",
            versao.como_texto(),
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        );
        let preparo = pasta.join(marca);
        let _ = std::fs::remove_dir_all(&preparo);
        std::fs::create_dir_all(&preparo).map_err(em_falha)?;
        Ok(preparo)
    }

    /// A última lista de revogação que esta máquina aceitou, se houve uma.
    ///
    /// Um arquivo que não abre, ou que não confere mais contra a chave, é
    /// indistinguível de arquivo nenhum: nos dois casos não há lista em que se
    /// possa acreditar, e acreditar no que não confere é o buraco que a
    /// assinatura existe para fechar.
    #[must_use]
    pub fn lista_guardada(&self, chave: &Chave) -> Option<ListaDeRevogacao> {
        let bytes = std::fs::read(self.raiz.join(REVOGACOES_VISTAS)).ok()?;
        let envelope: Envelope = serde_json::from_slice(&bytes).ok()?;
        ListaDeRevogacao::abrir(&envelope, chave).ok()
    }

    /// Decide qual lista de revogação vale, entre a que chegou e a guardada.
    ///
    /// É a peça que faz a revogação valer alguma coisa contra quem serve o
    /// manifesto. A assinatura impede **forjar** uma revogação; ela não impede
    /// **esconder** uma, servindo um manifesto anterior a ela. As duas regras
    /// aqui fecham isso:
    ///
    /// - **uma lista que sumiu não vira «nada revogado»**: sem lista no
    ///   manifesto, vale a última guardada;
    /// - **uma lista não recua**: uma que chegue mais velha que a guardada é
    ///   recusada, e o ato é recusado junto. Continuar calado com a lista boa
    ///   seria acertar e não contar.
    ///
    /// Quando a lista que chega é mais nova, ela é guardada. Uma falha ao
    /// gravar **não** impede o ato: o que se perde é a proteção da próxima
    /// abertura, e trocar um ato por isso seria a troca errada — mesmo
    /// argumento do bilhete de atualização em `apps/seele-app/src/main.rs`.
    ///
    /// # Errors
    ///
    /// [`FalhaDaLista`], quando a lista que chegou não abre ou recua.
    pub fn conciliar_revogacoes(
        &self,
        do_manifesto: Option<&Envelope>,
        chave: &Chave,
    ) -> Result<Vigente, FalhaDaLista> {
        let guardada = self.lista_guardada(chave);
        let Some(envelope) = do_manifesto else {
            return Ok(Vigente::conciliada(guardada));
        };
        let chegou = ListaDeRevogacao::abrir(envelope, chave)?;

        let mut memoria = Memoria {
            ultima_emissao: guardada.as_ref().map(|l| l.emitida_em().to_owned()),
        };
        memoria.aceitar(&chegou)?;

        let _ = self.guardar_lista(envelope);
        Ok(Vigente::conciliada(Some(chegou)))
    }

    /// Guarda o envelope de uma lista já conferida.
    ///
    /// # Errors
    ///
    /// Falha de escrita ou de serialização.
    pub fn guardar_lista(&self, envelope: &Envelope) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.raiz)?;
        let texto = serde_json::to_vec(envelope).map_err(std::io::Error::other)?;
        std::fs::write(self.raiz.join(REVOGACOES_VISTAS), texto)
    }
}

/// O SHA-256 de uns bytes, em hexadecimal minúsculo.
pub(crate) fn resumo_em_hexadecimal(bytes: &[u8]) -> String {
    let resumo = Sha256::digest(bytes);
    let mut texto = String::with_capacity(64);
    for byte in resumo {
        use std::fmt::Write as _;
        let _ = write!(texto, "{byte:02x}");
    }
    texto
}

fn em_falha(erro: std::io::Error) -> FalhaAoInstalar {
    FalhaAoInstalar::NaoInstalei {
        detalhe: erro.to_string(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::cenario::{
        chave, manifesto, manifesto_com_executavel, pacote, pasta, DesempacotadorDeTeste,
        DesempacotadorQueFalhaNoMeio, DesempacotadorSemExecutavel, ALVO, EXECUTAVEL,
    };

    fn instalar_uma(deposito: &Deposito, versao: &str) -> Instalacao {
        let m = manifesto(&[versao], &[]);
        let p = m
            .publicacao(&Versao::nova(versao).unwrap())
            .unwrap()
            .clone();
        deposito
            .instalar(&p, ALVO, &pacote(versao), &chave(), &DesempacotadorDeTeste)
            .expect("a instalação de teste devia passar")
    }

    #[test]
    fn instalar_publica_a_versao_e_o_executavel_declarado() {
        let raiz = pasta("instalar-ok");
        let deposito = Deposito::em(&raiz);
        let versao = Versao::nova("1.0.0-teste").unwrap();

        let instalacao = instalar_uma(&deposito, "1.0.0-teste");

        assert_eq!(instalacao.versao, versao);
        assert!(
            instalacao.executavel.is_file(),
            "o executável tem de estar lá"
        );
        assert!(deposito.esta_instalada(&versao));
        assert_eq!(deposito.instaladas(), vec![versao]);
        // E o preparo não ficou para trás ocupando disco.
        let preparo = raiz.join("preparo");
        let sobrou: Vec<_> = std::fs::read_dir(&preparo)
            .map(|d| d.flatten().collect())
            .unwrap_or_default();
        assert!(sobrou.is_empty(), "sobrou preparo: {sobrou:?}");
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **O critério de aceite:** a integridade é conferida antes de a
    /// instalação definitiva existir.
    #[test]
    fn um_pacote_com_soma_errada_nao_chega_a_tocar_o_disco() {
        let raiz = pasta("soma-errada");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let p = m
            .publicacao(&Versao::nova("1.0.0-teste").unwrap())
            .unwrap()
            .clone();

        let falha = deposito
            .instalar(
                &p,
                ALVO,
                b"outros bytes que nao sao o pacote",
                &chave(),
                &DesempacotadorDeTeste,
            )
            .expect_err("um pacote truncado tem de ser recusado");

        assert!(matches!(falha, FalhaAoInstalar::ConteudoDivergente { .. }));
        assert!(
            !raiz.join("versoes").exists(),
            "nada em disco podia ter sido criado"
        );
        assert!(
            !raiz.join("preparo").exists(),
            "a conferência acontece antes de haver preparo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Um manifesto que lista o pacote e esquece a assinatura dele.
    ///
    /// Não é o mesmo que assinatura errada: aqui não há o que conferir. O
    /// caminho que **não** pode existir é conferir contra texto vazio e a
    /// instalação seguir — por isso o campo é privado em
    /// [`crate::manifesto::Pacote`] e a recusa tem nome próprio.
    #[test]
    fn um_pacote_sem_assinatura_declarada_e_recusado_por_isso_e_nao_instala() {
        let raiz = pasta("sem-assinatura");
        let deposito = Deposito::em(&raiz);
        let json = format!(
            r#"{{"schema":2,"versions":[{{"version":"1.0.0-teste","platforms":{{
                "{ALVO}":{{"url":"https://exemplo.invalido/a",
                          "sha256":"{}","executable":"{EXECUTAVEL}"}}}}}}]}}"#,
            resumo_em_hexadecimal(&pacote("1.0.0-teste")),
        );
        let m = crate::Manifesto::ler(json.as_bytes(), &chave())
            .expect("faltar a assinatura de um alvo não derruba o manifesto");
        let p = m.mais_nova().clone();

        let falha = deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &chave(),
                &DesempacotadorDeTeste,
            )
            .expect_err("sem assinatura não há o que conferir, e não se instala");

        assert_eq!(falha, FalhaAoInstalar::AssinaturaNaoDeclarada);
        assert!(
            !raiz.join("versoes").exists(),
            "nada em disco podia ter sido criado"
        );
        assert!(!raiz.join("preparo").exists());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_pacote_assinado_por_outra_chave_nao_chega_a_tocar_o_disco() {
        let raiz = pasta("outra-chave");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let mut p = m
            .publicacao(&Versao::nova("1.0.0-teste").unwrap())
            .unwrap()
            .clone();
        // A soma continua certa; só a assinatura é de outra chave.
        let outra = crate::assinatura::fixtures::ChaveDeTeste::com_semente(7);
        if let Some(entrada) = p.platforms.get_mut(ALVO) {
            entrada
                .trocar_assinatura(outra.assinar_no_formato_do_atualizador(&pacote("1.0.0-teste")));
        }

        let falha = deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &chave(),
                &DesempacotadorDeTeste,
            )
            .expect_err("assinatura de outra chave tem de ser recusada");

        assert!(matches!(
            falha,
            FalhaAoInstalar::AssinaturaRecusada(FalhaDeAssinatura::OutraChave)
        ));
        assert!(!raiz.join("versoes").exists());
        assert!(
            !raiz.join("preparo").exists(),
            "a assinatura é conferida antes de haver preparo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **O critério de aceite:** falhas preservam instalações anteriores.
    #[test]
    fn uma_instalacao_que_falha_no_meio_nao_alcanca_a_que_ja_estava_la() {
        let raiz = pasta("falha-no-meio");
        let deposito = Deposito::em(&raiz);
        let velha = Versao::nova("1.0.0-teste").unwrap();
        instalar_uma(&deposito, "1.0.0-teste");
        let conteudo_antes =
            std::fs::read(deposito.pasta_da_versao(&velha).join(EXECUTAVEL)).unwrap();

        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        let nova = Versao::nova("2.0.0-teste").unwrap();
        let p = m.publicacao(&nova).unwrap().clone();
        let falha = deposito
            .instalar(
                &p,
                ALVO,
                &pacote("2.0.0-teste"),
                &chave(),
                &DesempacotadorQueFalhaNoMeio,
            )
            .expect_err("o desempacotador desistiu");

        assert!(matches!(falha, FalhaAoInstalar::NaoInstalei { .. }));
        // A que falhou não existe, nem pela metade.
        assert!(!deposito.esta_instalada(&nova));
        assert!(!deposito.pasta_da_versao(&nova).exists());
        // A que estava lá continua exatamente como estava.
        assert!(deposito.esta_instalada(&velha));
        assert_eq!(
            std::fs::read(deposito.pasta_da_versao(&velha).join(EXECUTAVEL)).unwrap(),
            conteudo_antes
        );
        assert_eq!(deposito.instaladas(), vec![velha]);
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_pacote_sem_o_executavel_declarado_nao_vira_instalacao() {
        let raiz = pasta("sem-executavel");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        let p = m.publicacao(&versao).unwrap().clone();

        let falha = deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &chave(),
                &DesempacotadorSemExecutavel,
            )
            .expect_err("sem o executável declarado não há instalação");

        assert!(matches!(falha, FalhaAoInstalar::NaoInstalei { .. }));
        assert!(!deposito.esta_instalada(&versao));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// O manifesto **não é assinado**, e um `executable` que sai da instalação
    /// publicaria como «o executável desta versão» um programa que não é dela.
    /// A recusa vem antes de o disco ser tocado, como a soma e a assinatura.
    #[test]
    fn um_executavel_que_sai_da_instalacao_e_recusado_antes_de_o_disco_ser_tocado() {
        for hostil in ["/bin/sh", "../1.0.0-teste/fuga", "C:\\Windows\\cmd.exe"] {
            let raiz = pasta("executavel-hostil");
            let deposito = Deposito::em(&raiz);
            let m = manifesto_com_executavel(&["2.0.0-teste"], Some(hostil));
            let versao = Versao::nova("2.0.0-teste").unwrap();
            let p = m.publicacao(&versao).unwrap().clone();

            let falha = deposito
                .instalar(
                    &p,
                    ALVO,
                    &pacote("2.0.0-teste"),
                    &chave(),
                    &DesempacotadorDeTeste,
                )
                .expect_err("um executável fora da instalação não pode ser instalado");

            assert!(
                matches!(falha, FalhaAoInstalar::ExecutavelInvalido(_)),
                "com `{hostil}` veio {falha:?}"
            );
            assert!(!deposito.esta_instalada(&versao));
            assert!(
                !raiz.join("versoes").exists() && !raiz.join("preparo").exists(),
                "com `{hostil}` alguma coisa foi escrita em disco"
            );
            let _ = std::fs::remove_dir_all(&raiz);
        }
    }

    /// **O manifesto publicado hoje**, sem `executable`: a instalação é
    /// recusada com esse motivo em vez de deixar em disco uma versão que só
    /// falharia na hora de abrir. Lacuna 3 de `docs/versoes-lado-a-lado.md`.
    #[test]
    fn sem_executavel_no_manifesto_nao_se_instala_para_descobrir_depois() {
        let raiz = pasta("sem-executavel-no-manifesto");
        let deposito = Deposito::em(&raiz);
        let m = manifesto_com_executavel(&["1.0.0-teste"], None);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        let p = m.publicacao(&versao).unwrap().clone();

        let falha = deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &chave(),
                &DesempacotadorDeTeste,
            )
            .expect_err("sem `executable` no manifesto não há o que iniciar");

        assert_eq!(falha, FalhaAoInstalar::ExecutavelNaoDeclarado);
        assert!(!deposito.esta_instalada(&versao));
        assert!(!raiz.join("versoes").exists() && !raiz.join("preparo").exists());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// A ordem de `instaladas()` é a do **nome**, e não a da novidade — e o
    /// caso que prova a diferença é banal: `0.9.0-teste` é mais nova que
    /// `0.10.0-teste` pelo nome e mais velha pelo manifesto. É por isso que
    /// `Versao` não tem `Ord`: quem responde «qual é a mais nova» é o
    /// manifesto, sempre.
    #[test]
    fn a_ordem_do_disco_e_de_nome_e_nao_serve_para_dizer_qual_e_a_mais_nova() {
        let raiz = pasta("ordem-de-nome");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["0.10.0-teste", "0.9.0-teste"], &[]);
        for v in ["0.9.0-teste", "0.10.0-teste"] {
            let p = m.publicacao(&Versao::nova(v).unwrap()).unwrap().clone();
            deposito
                .instalar(&p, ALVO, &pacote(v), &chave(), &DesempacotadorDeTeste)
                .unwrap();
        }

        assert_eq!(
            deposito.instaladas(),
            vec![
                Versao::nova("0.10.0-teste").unwrap(),
                Versao::nova("0.9.0-teste").unwrap()
            ],
            "a listagem é por nome"
        );
        // E a mais nova, pelo manifesto, é a outra.
        assert_eq!(m.mais_nova().version.como_texto(), "0.10.0-teste");
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// O marcador é o que separa «a pasta existe» de «a instalação funciona».
    #[test]
    fn uma_instalacao_sem_o_marcador_nao_conta_como_instalada() {
        let raiz = pasta("sem-marcador");
        let deposito = Deposito::em(&raiz);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        instalar_uma(&deposito, "1.0.0-teste");
        assert!(deposito.esta_instalada(&versao));

        std::fs::remove_file(deposito.pasta_da_versao(&versao).join(MARCADOR)).unwrap();

        assert!(!deposito.esta_instalada(&versao));
        assert!(deposito.instaladas().is_empty());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn instalar_de_novo_uma_versao_completa_nao_mexe_nela() {
        let raiz = pasta("idempotente");
        let deposito = Deposito::em(&raiz);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        instalar_uma(&deposito, "1.0.0-teste");
        let alvo = deposito.pasta_da_versao(&versao).join(EXECUTAVEL);
        std::fs::write(&alvo, b"editado a mao depois de instalado").unwrap();

        instalar_uma(&deposito, "1.0.0-teste");

        assert_eq!(
            std::fs::read(&alvo).unwrap(),
            b"editado a mao depois de instalado",
            "reinstalar por cima de uma instalação boa é o que pode quebrá-la"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn sem_pacote_para_este_sistema_a_recusa_nomeia_o_alvo() {
        let raiz = pasta("outro-alvo");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let p = m
            .publicacao(&Versao::nova("1.0.0-teste").unwrap())
            .unwrap()
            .clone();
        assert_eq!(
            deposito.instalar(
                &p,
                "sistema-que-nao-existe",
                &pacote("1.0.0-teste"),
                &chave(),
                &DesempacotadorDeTeste
            ),
            Err(FalhaAoInstalar::SemPacoteParaEsteSistema {
                alvo: "sistema-que-nao-existe".to_owned()
            })
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Uma tentativa que ficou pela metade não pode trancar a próxima.
    #[test]
    fn uma_pasta_de_instalacao_incompleta_nao_impede_instalar_de_novo() {
        let raiz = pasta("incompleta-nao-tranca");
        let deposito = Deposito::em(&raiz);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        // O que sobra de uma tentativa que morreu no meio, ou de um disco que
        // encheu: a pasta existe, com lixo dentro, e sem marcador.
        let sobra = deposito.pasta_da_versao(&versao);
        std::fs::create_dir_all(sobra.join("bin")).unwrap();
        std::fs::write(sobra.join("lixo-de-uma-tentativa-anterior.txt"), b"x").unwrap();
        assert!(!deposito.esta_instalada(&versao));

        instalar_uma(&deposito, "1.0.0-teste");

        assert!(deposito.esta_instalada(&versao));
        assert!(
            !sobra.join("lixo-de-uma-tentativa-anterior.txt").exists(),
            "a sobra tinha de ser varrida junto"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Encena outro processo que publica a mesma versão **enquanto** esta
    /// tentativa prepara a dela. É a corrida que separa «a pasta existe» de
    /// «a pasta existe e está completa».
    struct DesempacotadorQuePublicaPeloOutroLado {
        definitiva: PathBuf,
    }

    impl Desempacotador for DesempacotadorQuePublicaPeloOutroLado {
        fn desempacotar(&self, pacote: &[u8], destino: &Path) -> std::io::Result<()> {
            // O outro processo termina primeiro, com marcador e tudo.
            std::fs::create_dir_all(self.definitiva.join("bin"))?;
            std::fs::write(
                self.definitiva.join(EXECUTAVEL),
                b"a instalacao do outro processo",
            )?;
            std::fs::write(self.definitiva.join(MARCADOR), b"1.0.0-teste\n")?;
            // E só então este aqui acaba o preparo dele.
            DesempacotadorDeTeste.desempacotar(pacote, destino)
        }
    }

    #[test]
    fn uma_instalacao_completa_publicada_por_outro_processo_nao_e_varrida() {
        let raiz = pasta("corrida-publicacao");
        let deposito = Deposito::em(&raiz);
        let versao = Versao::nova("1.0.0-teste").unwrap();
        let m = manifesto(&["1.0.0-teste"], &[]);
        let p = m.publicacao(&versao).unwrap().clone();

        deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &chave(),
                &DesempacotadorQuePublicaPeloOutroLado {
                    definitiva: deposito.pasta_da_versao(&versao),
                },
            )
            .expect("a instalação do outro processo serve igual");

        assert!(deposito.esta_instalada(&versao));
        assert_eq!(
            std::fs::read(deposito.pasta_da_versao(&versao).join(EXECUTAVEL)).unwrap(),
            b"a instalacao do outro processo",
            "a instalação completa de quem chegou primeiro não podia ser apagada"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    fn envelope_de(documento: &str) -> Envelope {
        use base64::Engine as _;
        Envelope {
            document: base64::engine::general_purpose::STANDARD.encode(documento),
            signature: crate::cenario::par()
                .assinar_no_formato_do_atualizador(documento.as_bytes()),
        }
    }

    const LISTA_NOVA: &str = r#"{"schema":1,"issued_at":"2026-09-10T00:00:00Z","revoked":[
        {"target":"1.0.0-teste","defect":"defeito de teste","fixed_in":"2.0.0-teste"}]}"#;
    const LISTA_VELHA: &str = r#"{"schema":1,"issued_at":"2026-08-01T00:00:00Z","revoked":[]}"#;

    /// **Esconder uma revogação servindo o manifesto de antes dela.**
    #[test]
    fn uma_lista_que_some_do_manifesto_nao_vira_nada_revogado() {
        let raiz = pasta("lista-some");
        let deposito = Deposito::em(&raiz);
        let chave = chave();

        // Hoje o manifesto traz a lista.
        let vigente = deposito
            .conciliar_revogacoes(Some(&envelope_de(LISTA_NOVA)), &chave)
            .unwrap();
        assert!(vigente.revogacao_de("1.0.0-teste").is_some());

        // Amanhã ele vem sem nenhuma. Um depósito novo, como na próxima
        // abertura do app.
        let depois = Deposito::em(&raiz)
            .conciliar_revogacoes(None, &chave)
            .unwrap();
        assert!(
            depois.revogacao_de("1.0.0-teste").is_some(),
            "sumir do manifesto não pode desfazer uma revogação"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_lista_que_recua_recusa_o_ato_em_vez_de_seguir_calada() {
        let raiz = pasta("lista-recua");
        let deposito = Deposito::em(&raiz);
        let chave = chave();
        deposito
            .conciliar_revogacoes(Some(&envelope_de(LISTA_NOVA)), &chave)
            .unwrap();

        assert!(matches!(
            deposito.conciliar_revogacoes(Some(&envelope_de(LISTA_VELHA)), &chave),
            Err(FalhaDaLista::Retrocedeu { .. })
        ));
        // E a guardada continua sendo a boa.
        assert!(deposito
            .lista_guardada(&chave)
            .and_then(|l| l.revogacao_de("1.0.0-teste").cloned())
            .is_some());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_lista_guardada_que_nao_confere_mais_e_ignorada() {
        let raiz = pasta("guardada-adulterada");
        let deposito = Deposito::em(&raiz);
        let chave = chave();
        deposito
            .conciliar_revogacoes(Some(&envelope_de(LISTA_NOVA)), &chave)
            .unwrap();

        // Alguém edita o arquivo guardado para tirar a revogação.
        use base64::Engine as _;
        let adulterado = Envelope {
            document: base64::engine::general_purpose::STANDARD
                .encode(r#"{"schema":1,"issued_at":"2026-09-10T00:00:00Z","revoked":[]}"#),
            signature: crate::cenario::par()
                .assinar_no_formato_do_atualizador(LISTA_NOVA.as_bytes()),
        };
        deposito.guardar_lista(&adulterado).unwrap();

        assert!(
            deposito.lista_guardada(&chave).is_none(),
            "uma lista guardada que não confere não pode ser acreditada"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn sem_lista_em_lugar_nenhum_a_resposta_e_que_nao_ha_lista() {
        let raiz = pasta("sem-lista");
        let deposito = Deposito::em(&raiz);
        assert!(deposito
            .conciliar_revogacoes(None, &chave())
            .unwrap()
            .lista()
            .is_none());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_deposito_que_nao_existe_responde_sem_panico() {
        let deposito = Deposito::em("/caminho/que/nao/existe/seele-lancador-teste");
        assert!(deposito.instaladas().is_empty());
        assert!(!deposito.esta_instalada(&Versao::nova("1.0.0-teste").unwrap()));
        assert!(deposito.lista_guardada(&chave()).is_none());
        assert!(deposito
            .conciliar_revogacoes(None, &chave())
            .unwrap()
            .lista()
            .is_none());
    }
}
