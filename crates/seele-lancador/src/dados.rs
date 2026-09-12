//! Um diretório de dados por versão, e a frase que a tela tem de dizer antes.
//!
//! ADR 0045:
//!
//! > **Um diretório de dados por versão.** Porque não há migração reversa.
//! > Descer de versão não converte banco: abre outro, como um mundo de
//! > Minecraft.
//! >
//! > **Isto vai na tela, não numa nota de release:** descer de versão não leva
//! > as conversas junto. Um produto que descobre isso pelo silêncio é o defeito
//! > que o `CLAUDE.md` chama de *«o produto sabe e não conta»*.
//!
//! Este módulo é as duas metades daquilo: onde os dados ficam, e **o estado
//! que permite dizer a frase antes** — não depois, não num aviso genérico, e
//! não só quando se desce.
//!
//! # Por que subir leva as conversas e descer não
//!
//! A assimetria não é escolha de produto; ela vem de
//! `crates/seele-server/src/persistence/schema.rs`, que é *«append only once
//! shipped»* e tem um teste, `no_migration_contains_a_down_step`, reprovando
//! `DROP TABLE` e `DROP COLUMN`. Migração para a frente existe e roda no boot;
//! migração para trás não existe e escrevê-la seria escrever a coisa que
//! aquele teste proíbe.
//!
//! Então:
//!
//! - **subir** de uma versão que tem dados: os dados são **semeados**, uma
//!   cópia, e a versão nova migra o que copiou. A cópia, e não a mudança de
//!   lugar, é o que deixa a versão de origem intacta para se voltar a ela.
//! - **descer**: não há nada mais velho de onde semear, e a versão começa
//!   vazia. É [`OrigemDosDados::Nova`], e é a hora de dizer a frase.
//!
//! # Decidir, contar, e só então fazer
//!
//! [`Deposito::plano_de_dados`](crate::Deposito) devolve um [`PlanoDeDados`]
//! e **não toca em disco**. Quem chama mostra o que vai acontecer e só depois
//! chama [`aplicar`]. Um launcher que copiasse um banco de conversas sem
//! avisar seria o mesmo defeito de outro ângulo.
//!
//! # E a semeadura pode parar no meio
//!
//! A máquina desliga durante a cópia, e o que fica é a pasta da versão nova
//! criada e pela metade. Ler aquela pasta como «esta versão já tem os dados
//! dela» é abrir a versão sem as conversas **e não dizer nada** — o defeito
//! do `CLAUDE.md` pelo caminho mais caro. A instalação já separava «a pasta
//! existe» de «chegou inteira» com o marcador `INSTALADA`; a semeadura passa a
//! ter o equivalente. Ver [`MARCADOR`].

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

use crate::deposito::Deposito;
use crate::manifesto::Manifesto;
use crate::versao::Versao;

/// O nome do marcador que diz «este diretório de dados chegou inteiro».
///
/// O irmão do `INSTALADA` de [`crate::deposito`], e pela mesma razão. Escrito
/// **por último**, depois da cópia inteira: antes dele a pasta é um resto de
/// tentativa, e não os dados de ninguém.
///
/// Sem ele, uma semeadura interrompida deixava a pasta da versão nova criada e
/// vazia, e a abertura seguinte a classificava como
/// [`OrigemDosDados::Propria`] com `perde_conversas: false`. A versão abria sem
/// as conversas e nada era dito.
const MARCADOR: &str = "PRONTA";

/// De onde vêm os dados que esta versão vai abrir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrigemDosDados {
    /// Esta versão já tem o diretório dela. Nada a fazer.
    Propria,
    /// Vai ser copiada da versão mais nova entre as mais velhas que esta.
    ///
    /// Migração para a frente, que o esquema suporta.
    Semeada {
        /// De qual versão.
        de: Versao,
    },
    /// Começa vazia: não há nada mais velho de onde semear.
    Nova,
}

/// O que vai acontecer com os dados, antes de acontecer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanoDeDados {
    /// Para qual versão.
    pub versao: Versao,
    /// Onde os dados dela ficam.
    pub diretorio: PathBuf,
    /// De onde eles vêm.
    pub origem: OrigemDosDados,
    /// Que outras versões têm dados nesta máquina, em ordem de nome.
    ///
    /// Para a tela poder dizer **onde** as conversas ficaram, em vez de só
    /// dizer que não vieram.
    pub outras_com_dados: Vec<Versao>,
    /// Esta escolha começa sem as conversas que existem em outra versão?
    ///
    /// O campo é a frase. `true` só quando as duas coisas valem ao mesmo
    /// tempo: esta versão começa vazia **e** existe conversa em outro lugar.
    /// Uma máquina em que nunca houve conversa nenhuma não tem nada a avisar,
    /// e avisar ali é ensinar a ignorar o aviso.
    pub perde_conversas: bool,
}

/// O estado depois de o plano ter sido aplicado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDosDados {
    /// O plano que foi aplicado.
    pub plano: PlanoDeDados,
    /// O diretório, agora existindo.
    pub diretorio: PathBuf,
}

impl Deposito {
    /// O que aconteceria com os dados desta versão. **Não toca em disco.**
    ///
    /// A ordem de «mais velha» vem do manifesto, nunca de comparar textos de
    /// versão: `0.10.5-1` saiu depois de `0.10.5` e o semver diria o contrário.
    /// Uma versão que o manifesto não lista não é candidata a semear — não há
    /// como saber se ela é mais velha ou mais nova, e semear da errada é
    /// pedir a uma versão que abra um banco do futuro.
    #[must_use]
    pub fn plano_de_dados(&self, alvo: &Versao, manifesto: &Manifesto) -> PlanoDeDados {
        let diretorio = self.pasta_de_dados(alvo);
        let com_dados = self.versoes_com_dados();
        let outras_com_dados: Vec<Versao> =
            com_dados.iter().filter(|v| *v != alvo).cloned().collect();

        if com_dados.contains(alvo) {
            return PlanoDeDados {
                versao: alvo.clone(),
                diretorio,
                origem: OrigemDosDados::Propria,
                outras_com_dados,
                perde_conversas: false,
            };
        }

        // Candidatas: as que têm dados e são **mais velhas** que o alvo, isto
        // é, aparecem depois dele na ordem de publicação. Entre elas, a mais
        // nova — a de menor posição — é de onde se semeia.
        let origem = manifesto.posicao(alvo).and_then(|posicao_do_alvo| {
            com_dados
                .iter()
                .filter_map(|v| manifesto.posicao(v).map(|p| (p, v)))
                .filter(|(p, _)| *p > posicao_do_alvo)
                .min_by_key(|(p, _)| *p)
                .map(|(_, v)| OrigemDosDados::Semeada { de: v.clone() })
        });

        let origem = origem.unwrap_or(OrigemDosDados::Nova);
        let perde_conversas =
            matches!(origem, OrigemDosDados::Nova) && !outras_com_dados.is_empty();

        PlanoDeDados {
            versao: alvo.clone(),
            diretorio,
            origem,
            outras_com_dados,
            perde_conversas,
        }
    }

    /// Esta versão tem o diretório de dados dela, **inteiro**?
    ///
    /// Uma pasta sem o marcador é uma semeadura que não terminou, e a resposta
    /// é `false` — não «existe a pasta, então são os dados dela». Ver
    /// [`MARCADOR`].
    #[must_use]
    pub fn tem_dados(&self, versao: &Versao) -> bool {
        self.pasta_de_dados(versao).join(MARCADOR).is_file()
    }

    /// Que versões têm diretório de dados **inteiro** nesta máquina.
    #[must_use]
    pub fn versoes_com_dados(&self) -> Vec<Versao> {
        let mut achadas = Vec::new();
        let Ok(entradas) = std::fs::read_dir(self.raiz().join("dados")) else {
            return achadas;
        };
        for entrada in entradas.flatten() {
            if !entrada.path().is_dir() {
                continue;
            }
            let nome = entrada.file_name();
            let Some(nome) = nome.to_str() else { continue };
            let Ok(versao) = Versao::nova(nome) else {
                continue;
            };
            // Uma pasta sem marcador não entra: ela não guarda conversa
            // nenhuma, e contá-la aqui é o que fazia a semeadura interrompida
            // passar por dados próprios — e, do outro lado, fazia uma pasta
            // pela metade valer como «há conversa em outro lugar».
            if self.tem_dados(&versao) {
                achadas.push(versao);
            }
        }
        // Ordem de nome, e nada mais: quem diz qual é a mais nova é o
        // manifesto. Ver [`crate::versao::Versao`].
        achadas.sort_by(|esta, aquela| esta.como_texto().cmp(aquela.como_texto()));
        achadas
    }

    /// Executa o plano: prepara o diretório, semeia se for o caso, e **só
    /// então** escreve o marcador que diz que ele chegou inteiro.
    ///
    /// # Errors
    ///
    /// Falha de disco. Uma semeadura que falha no meio **não** toca no
    /// diretório de origem — a cópia é uma cópia, e a origem continua sendo a
    /// instalação de onde se veio. E o que ela deixa no destino não passa por
    /// dados: sem marcador é um resto, a próxima aplicação o apaga e recomeça
    /// da origem, que não mudou.
    pub fn aplicar(&self, plano: &PlanoDeDados) -> std::io::Result<EstadoDosDados> {
        match &plano.origem {
            OrigemDosDados::Propria => {}
            OrigemDosDados::Nova => {
                apagar_o_resto(&plano.diretorio)?;
                std::fs::create_dir_all(&plano.diretorio)?;
                marcar(&plano.diretorio, &plano.versao)?;
            }
            OrigemDosDados::Semeada { de } => {
                let origem = self.pasta_de_dados(de);
                apagar_o_resto(&plano.diretorio)?;
                std::fs::create_dir_all(&plano.diretorio)?;
                copiar_arvore(&origem, &plano.diretorio)?;
                marcar(&plano.diretorio, &plano.versao)?;
            }
        }
        Ok(EstadoDosDados {
            plano: plano.clone(),
            diretorio: plano.diretorio.clone(),
        })
    }
}

/// Escreve o marcador, por último. Ver [`MARCADOR`].
fn marcar(diretorio: &Path, versao: &Versao) -> std::io::Result<()> {
    std::fs::write(
        diretorio.join(MARCADOR),
        format!("{}\n", versao.como_texto()),
    )
}

/// Apaga o que sobrou de uma semeadura que parou no meio.
///
/// Só alcança diretório **sem** marcador, e é isso que a torna segura: uma
/// pasta sem marcador nunca foi a casa das conversas de ninguém — ela é uma
/// cópia interrompida, e recomeçar da origem, que não mudou, é o que dá o
/// resultado certo. Uma pasta com marcador não chega aqui: o plano dela é
/// [`OrigemDosDados::Propria`], e `Propria` não aplica nada.
fn apagar_o_resto(diretorio: &Path) -> std::io::Result<()> {
    if !diretorio.exists() || diretorio.join(MARCADOR).is_file() {
        return Ok(());
    }
    std::fs::remove_dir_all(diretorio)
}

/// Copia uma árvore de arquivos, sem seguir ligação simbólica.
///
/// Sem seguir ligação de propósito: uma ligação dentro do diretório de dados
/// aponta para onde quem a criou quis, e copiar «através» dela escreveria fora
/// do destino. Uma ligação é ignorada em vez de resolvida — a mesma postura
/// que `seele-proto::mods::inner_path` toma com o `..`: recusar em vez de
/// resolver.
fn copiar_arvore(de: &Path, para: &Path) -> std::io::Result<()> {
    copiar(de, para, true)
}

/// O passo recursivo. `topo` é o que permite deixar o marcador da origem para
/// trás sem perder um arquivo de dados que por acaso tenha o mesmo nome lá
/// dentro.
fn copiar(de: &Path, para: &Path, topo: bool) -> std::io::Result<()> {
    for entrada in std::fs::read_dir(de)? {
        let entrada = entrada?;
        let tipo = entrada.file_type()?;
        // **O marcador da origem não é o marcador do destino.** `read_dir` não
        // promete ordem nenhuma: copiado primeiro, ele marcaria como inteira
        // uma cópia que ainda está no meio, e a interrupção seguinte deixaria
        // exatamente o estado que o marcador existe para impedir. Quem marca o
        // destino é [`marcar`], depois de a cópia terminar.
        if topo && entrada.file_name() == MARCADOR {
            continue;
        }
        let destino = para.join(entrada.file_name());
        if tipo.is_symlink() {
            continue;
        }
        if tipo.is_dir() {
            std::fs::create_dir_all(&destino)?;
            copiar(&entrada.path(), &destino, false)?;
        } else if tipo.is_file() {
            std::fs::copy(entrada.path(), &destino)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::cenario::{manifesto, pasta};

    /// Semeia um diretório de dados **inteiro**, com uma «conversa»
    /// reconhecível e o marcador que diz que ele chegou inteiro.
    fn com_conversa(deposito: &Deposito, versao: &str, texto: &str) {
        let v = Versao::nova(versao).unwrap();
        let pasta = deposito.pasta_de_dados(&v);
        std::fs::create_dir_all(&pasta).unwrap();
        std::fs::write(pasta.join("conversas-de-teste.db"), texto).unwrap();
        marcar(&pasta, &v).unwrap();
    }

    /// O que uma semeadura interrompida deixa: a pasta criada, com meia cópia
    /// dentro ou nada, e **sem** marcador.
    fn semeadura_interrompida(deposito: &Deposito, versao: &str, metade: &[(&str, &str)]) {
        let pasta = deposito.pasta_de_dados(&Versao::nova(versao).unwrap());
        std::fs::create_dir_all(&pasta).unwrap();
        for (nome, texto) in metade {
            std::fs::write(pasta.join(nome), texto).unwrap();
        }
    }

    /// **O critério de aceite:** cada versão tem o diretório de dados dela.
    #[test]
    fn duas_versoes_nunca_dividem_o_diretorio_de_dados() {
        let raiz = pasta("dados-separados");
        let deposito = Deposito::em(&raiz);
        let a = Versao::nova("1.0.0-teste").unwrap();
        let b = Versao::nova("2.0.0-teste").unwrap();
        assert_ne!(deposito.pasta_de_dados(&a), deposito.pasta_de_dados(&b));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Descer de versão não leva as conversas junto**, e o plano diz isso
    /// antes de qualquer coisa acontecer.
    #[test]
    fn descer_de_versao_comeca_vazio_e_o_plano_avisa_antes() {
        let raiz = pasta("descer");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        com_conversa(&deposito, "2.0.0-teste", "as conversas da versao nova");

        let alvo = Versao::nova("1.0.0-teste").unwrap();
        let plano = deposito.plano_de_dados(&alvo, &m);

        assert_eq!(plano.origem, OrigemDosDados::Nova);
        assert!(
            plano.perde_conversas,
            "a tela precisa poder dizer isso antes, não depois"
        );
        assert_eq!(
            plano.outras_com_dados,
            vec![Versao::nova("2.0.0-teste").unwrap()]
        );
        // E aplicar não vai buscar nada da versão nova.
        let estado = deposito.aplicar(&plano).unwrap();
        assert!(estado.diretorio.is_dir());
        assert!(!estado.diretorio.join("conversas-de-teste.db").exists());
        // A versão de onde se veio continua intacta.
        assert_eq!(
            std::fs::read_to_string(
                deposito
                    .pasta_de_dados(&Versao::nova("2.0.0-teste").unwrap())
                    .join("conversas-de-teste.db")
            )
            .unwrap(),
            "as conversas da versao nova"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Subir leva, porque migração para a frente existe. A cópia deixa a
    /// origem de pé para se poder voltar.
    #[test]
    fn subir_de_versao_semeia_da_mais_nova_entre_as_mais_velhas() {
        let raiz = pasta("subir");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["3.0.0-teste", "2.0.0-teste", "1.0.0-teste"], &[]);
        com_conversa(&deposito, "1.0.0-teste", "conversa antiga");
        com_conversa(&deposito, "2.0.0-teste", "conversa do meio");

        let alvo = Versao::nova("3.0.0-teste").unwrap();
        let plano = deposito.plano_de_dados(&alvo, &m);

        assert_eq!(
            plano.origem,
            OrigemDosDados::Semeada {
                de: Versao::nova("2.0.0-teste").unwrap()
            },
            "semeia da mais nova entre as mais velhas, não da mais antiga"
        );
        assert!(!plano.perde_conversas);

        let estado = deposito.aplicar(&plano).unwrap();
        assert_eq!(
            std::fs::read_to_string(estado.diretorio.join("conversas-de-teste.db")).unwrap(),
            "conversa do meio"
        );
        // Cópia e não mudança de lugar: dá para voltar.
        assert!(deposito
            .pasta_de_dados(&Versao::nova("2.0.0-teste").unwrap())
            .join("conversas-de-teste.db")
            .is_file());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_versao_que_ja_tem_dados_nao_e_semeada_de_novo() {
        let raiz = pasta("propria");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        com_conversa(&deposito, "1.0.0-teste", "conversa antiga");
        com_conversa(&deposito, "2.0.0-teste", "conversa propria");

        let plano = deposito.plano_de_dados(&Versao::nova("2.0.0-teste").unwrap(), &m);

        assert_eq!(plano.origem, OrigemDosDados::Propria);
        assert!(!plano.perde_conversas);
        let estado = deposito.aplicar(&plano).unwrap();
        assert_eq!(
            std::fs::read_to_string(estado.diretorio.join("conversas-de-teste.db")).unwrap(),
            "conversa propria",
            "semear por cima do que já existe apagaria conversa"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Numa máquina em que nunca houve conversa nenhuma não há o que avisar.
    #[test]
    fn a_primeira_versao_de_uma_maquina_limpa_nao_avisa_perda_nenhuma() {
        let raiz = pasta("limpa");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let plano = deposito.plano_de_dados(&Versao::nova("1.0.0-teste").unwrap(), &m);
        assert_eq!(plano.origem, OrigemDosDados::Nova);
        assert!(
            !plano.perde_conversas,
            "avisar sem ter o que perder é ensinar a ignorar o aviso"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Uma versão que o manifesto não conhece não é candidata a semear.
    #[test]
    fn nao_se_semeia_de_uma_versao_que_o_manifesto_nao_ordena() {
        let raiz = pasta("fora-do-manifesto");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste"], &[]);
        com_conversa(&deposito, "9.9.9-teste", "de uma versao desconhecida");

        let plano = deposito.plano_de_dados(&Versao::nova("2.0.0-teste").unwrap(), &m);

        assert_eq!(
            plano.origem,
            OrigemDosDados::Nova,
            "sem ordem não há como saber se ela é mais velha ou mais nova"
        );
        assert!(plano.perde_conversas, "e há conversa em outro lugar");
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn o_plano_nao_toca_em_disco() {
        let raiz = pasta("plano-seco");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let _ = deposito.plano_de_dados(&Versao::nova("1.0.0-teste").unwrap(), &m);
        assert!(
            !raiz.join("dados").exists(),
            "decidir tem de poder acontecer antes de contar"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_ligacao_simbolica_nao_faz_a_semeadura_escrever_fora_do_destino() {
        let raiz = pasta("ligacao");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        com_conversa(&deposito, "1.0.0-teste", "conversa antiga");
        let fora = raiz.join("fora-do-deposito.txt");
        std::fs::write(&fora, "nao devia ser copiado").unwrap();
        let origem = deposito.pasta_de_dados(&Versao::nova("1.0.0-teste").unwrap());
        #[cfg(unix)]
        std::os::unix::fs::symlink(&fora, origem.join("atalho.txt")).unwrap();

        let plano = deposito.plano_de_dados(&Versao::nova("2.0.0-teste").unwrap(), &m);
        let estado = deposito.aplicar(&plano).unwrap();

        assert!(estado.diretorio.join("conversas-de-teste.db").is_file());
        assert!(
            !estado.diretorio.join("atalho.txt").exists(),
            "uma ligação é ignorada, não resolvida"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A semeadura interrompida não passa por «dados próprios».**
    ///
    /// Era o caminho em que o launcher acertava e não contava: a pasta da
    /// versão nova existia, vazia, e a abertura seguinte a lia como os dados
    /// dela. A versão abria sem as conversas e ninguém era avisado.
    #[test]
    fn uma_semeadura_que_parou_no_meio_nao_passa_por_dados_proprios() {
        let raiz = pasta("semeadura-interrompida");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        com_conversa(&deposito, "1.0.0-teste", "conversa antiga");
        // A cópia começou e a máquina desligou: pasta criada, nada dentro.
        semeadura_interrompida(&deposito, "2.0.0-teste", &[]);

        let alvo = Versao::nova("2.0.0-teste").unwrap();
        let plano = deposito.plano_de_dados(&alvo, &m);

        assert_eq!(
            plano.origem,
            OrigemDosDados::Semeada {
                de: Versao::nova("1.0.0-teste").unwrap()
            },
            "a pasta existe e não chegou inteira: ela não são os dados de ninguém"
        );
        assert!(!deposito.tem_dados(&alvo));

        // E aplicar de novo termina o que ficou no meio.
        let estado = deposito.aplicar(&plano).unwrap();
        assert_eq!(
            std::fs::read_to_string(estado.diretorio.join("conversas-de-teste.db")).unwrap(),
            "conversa antiga"
        );
        assert!(
            deposito.tem_dados(&alvo),
            "agora sim, e o marcador é quem diz"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// O resto da tentativa de antes não fica misturado com a cópia boa.
    #[test]
    fn a_meia_copia_de_uma_tentativa_nao_sobrevive_a_seguinte() {
        let raiz = pasta("meia-copia");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        com_conversa(&deposito, "1.0.0-teste", "conversa antiga");
        semeadura_interrompida(
            &deposito,
            "2.0.0-teste",
            &[
                ("conversas-de-teste.db", "metade escrita"),
                ("so-da-tentativa-velha.txt", "nao existe mais na origem"),
            ],
        );

        let alvo = Versao::nova("2.0.0-teste").unwrap();
        let estado = deposito
            .aplicar(&deposito.plano_de_dados(&alvo, &m))
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(estado.diretorio.join("conversas-de-teste.db")).unwrap(),
            "conversa antiga",
            "a cópia boa manda, e não o que a tentativa de antes deixou"
        );
        assert!(
            !estado.diretorio.join("so-da-tentativa-velha.txt").exists(),
            "o que não está na origem não pode aparecer na cópia"
        );
        // E a origem continua de pé.
        assert!(deposito
            .pasta_de_dados(&Versao::nova("1.0.0-teste").unwrap())
            .join("conversas-de-teste.db")
            .is_file());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Do outro lado da mesma regra: uma pasta pela metade não é «conversa em
    /// outro lugar». Avisar que se perde o que nunca chegou a existir é
    /// ensinar a ignorar o aviso.
    #[test]
    fn uma_pasta_pela_metade_nao_conta_como_conversa_em_outro_lugar() {
        let raiz = pasta("meia-nao-conta");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        semeadura_interrompida(
            &deposito,
            "2.0.0-teste",
            &[("conversas-de-teste.db", "metade")],
        );

        let plano = deposito.plano_de_dados(&Versao::nova("1.0.0-teste").unwrap(), &m);

        assert_eq!(plano.origem, OrigemDosDados::Nova);
        assert!(plano.outras_com_dados.is_empty());
        assert!(
            !plano.perde_conversas,
            "não há conversa nenhuma nesta máquina para se perder"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// O marcador da origem não viaja na cópia. Se viajasse, `read_dir` sem
    /// ordem garantida poderia escrevê-lo primeiro — e a interrupção seguinte
    /// deixaria marcada como inteira uma pasta pela metade, que é exatamente o
    /// que o marcador existe para impedir.
    #[test]
    fn o_marcador_da_origem_nao_e_copiado_junto() {
        let raiz = pasta("marcador-nao-viaja");
        let deposito = Deposito::em(&raiz);
        com_conversa(&deposito, "1.0.0-teste", "conversa antiga");
        let origem = deposito.pasta_de_dados(&Versao::nova("1.0.0-teste").unwrap());
        let destino = raiz.join("copia-crua");
        std::fs::create_dir_all(&destino).unwrap();

        copiar_arvore(&origem, &destino).unwrap();

        assert!(destino.join("conversas-de-teste.db").is_file());
        assert!(
            !destino.join(MARCADOR).exists(),
            "quem marca o destino é `marcar`, depois de a cópia terminar"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
