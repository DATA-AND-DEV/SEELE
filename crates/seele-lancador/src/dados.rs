//! Um diretório de dados por **linha** de servidor, e a frase que a tela tem de
//! dizer antes.
//!
//! ADR 0046, com a emenda de 08/10/2026:
//!
//! > **Um diretório de dados por linha**, e não por versão
//! > (`dados/servidor-0.15`): um conserto da linha herda os dados dela. A tela
//! > diz, antes de abrir, que as conversas de uma linha não aparecem na outra.
//!
//! Este módulo é as duas metades daquilo: onde os dados ficam, e **o estado
//! que permite dizer a frase antes** — não depois, não num aviso genérico.
//!
//! # Por que não há migração entre linhas aqui
//!
//! Uma linha nova (a 1, depois da 0.15) quebra o fio de propósito, e o banco
//! dela é outro. Dentro de uma linha não há o que migrar: a 0.15.3 abre a
//! pasta da 0.15.2, e o `migrate()` dela anda para a frente como sempre andou.
//! Entre linhas, **descer** não converte banco (`persistence/schema.rs` é
//! *«append only once shipped»*, com um teste reprovando passo de volta), e
//! **subir** é a cópia de uma vez da primeira abertura da 0.16
//! ([`Deposito::semear_linha`]): os dados da 0.15 viram os da linha 0.15
//! **antes** de a 0.16 migrar a pasta dela.
//!
//! # Decidir, contar, e só então fazer
//!
//! [`Deposito::plano_da_linha`] devolve um [`PlanoDeDados`] e **não toca em
//! disco**. Quem chama mostra o que vai acontecer e só depois chama
//! [`Deposito::aplicar`]. Um launcher que abrisse uma linha vazia sem avisar
//! seria o «o produto sabe e não conta» do `CLAUDE.md`.
//!
//! # E a semeadura pode parar no meio
//!
//! A máquina desliga durante a cópia, e o que fica é a pasta da linha criada e
//! pela metade. Ler aquela pasta como «esta linha já tem os dados dela» é abrir
//! a linha sem as conversas **e não dizer nada**. A instalação separa «a pasta
//! existe» de «chegou inteira» com o marcador `INSTALADA`; a semeadura tem o
//! equivalente. Ver [`MARCADOR`].

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

use crate::deposito::Deposito;
use crate::linha::Linha;

/// O nome do marcador que diz «este diretório de dados chegou inteiro».
///
/// O irmão do `INSTALADA` de [`crate::deposito`], e pela mesma razão. Escrito
/// **por último**, depois da cópia inteira: antes dele a pasta é um resto de
/// tentativa, e não os dados de ninguém.
const MARCADOR: &str = "PRONTA";

/// O prefixo das pastas de dados, dentro de `dados/`. Ver [`Linha::pasta_de_dados`].
const PREFIXO: &str = "servidor-";

/// De onde vêm os dados que esta linha vai abrir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrigemDosDados {
    /// A linha já tem o diretório dela, inteiro. Nada a fazer.
    Propria,
    /// Começa vazia.
    Nova,
}

/// O que vai acontecer com os dados, antes de acontecer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanoDeDados {
    /// Para qual linha.
    pub linha: Linha,
    /// Onde os dados dela ficam.
    pub diretorio: PathBuf,
    /// De onde eles vêm.
    pub origem: OrigemDosDados,
    /// As outras linhas com dados nesta máquina, da mais nova para a mais
    /// velha.
    ///
    /// Para a tela poder dizer **onde** as conversas ficaram, em vez de só
    /// dizer que não vieram.
    pub outras_com_dados: Vec<Linha>,
    /// Esta linha começa sem as conversas que existem em outro lugar?
    ///
    /// O campo é a frase. `true` só quando as duas coisas valem ao mesmo
    /// tempo: esta linha começa vazia **e** há conversa em outro lugar — noutra
    /// linha do depósito, ou na pasta do próprio app. Uma máquina em que nunca
    /// houve conversa nenhuma não tem nada a avisar, e avisar ali é ensinar a
    /// ignorar o aviso.
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

/// O que uma semeadura fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Semeadura {
    /// A linha já tinha os dados dela, e nada foi copiado.
    JaTinha,
    /// Copiada, com o tamanho, para o rastro dizer quanto custou.
    Semeada {
        /// Quantos arquivos.
        arquivos: u64,
        /// Quantos bytes.
        bytes: u64,
    },
}

impl Deposito {
    /// Esta linha tem o diretório de dados dela, **inteiro**?
    ///
    /// Uma pasta sem o marcador é uma semeadura que não terminou, e a resposta
    /// é `false` — não «existe a pasta, então são os dados dela».
    #[must_use]
    pub fn tem_dados_da_linha(&self, linha: &Linha) -> bool {
        self.pasta_de_dados_da_linha(linha).join(MARCADOR).is_file()
    }

    /// Que linhas têm diretório de dados **inteiro** nesta máquina, da mais
    /// nova para a mais velha.
    #[must_use]
    pub fn linhas_com_dados(&self) -> Vec<Linha> {
        let mut achadas = Vec::new();
        let Ok(entradas) = std::fs::read_dir(self.raiz().join("dados")) else {
            return achadas;
        };
        for entrada in entradas.flatten() {
            let nome = entrada.file_name();
            let Some(texto) = nome.to_str().and_then(|n| n.strip_prefix(PREFIXO)) else {
                continue;
            };
            let Ok(linha) = Linha::nova(texto) else {
                continue;
            };
            // Uma pasta sem marcador não entra: ela não guarda conversa
            // nenhuma, e contá-la aqui faria uma pasta pela metade valer como
            // «há conversa em outro lugar».
            if self.tem_dados_da_linha(&linha) {
                achadas.push(linha);
            }
        }
        achadas.sort_by(|esta, aquela| aquela.cmp(esta));
        achadas
    }

    /// O que aconteceria com os dados desta linha. **Não toca em disco.**
    ///
    /// `conversas_fora_do_deposito` é o que só a casca sabe: a pasta do
    /// próprio app — a da linha que está aberta — tem conversa?
    #[must_use]
    pub fn plano_da_linha(&self, linha: &Linha, conversas_fora_do_deposito: bool) -> PlanoDeDados {
        let outras_com_dados: Vec<Linha> = self
            .linhas_com_dados()
            .into_iter()
            .filter(|l| l != linha)
            .collect();
        let origem = if self.tem_dados_da_linha(linha) {
            OrigemDosDados::Propria
        } else {
            OrigemDosDados::Nova
        };
        let perde_conversas = origem == OrigemDosDados::Nova
            && (conversas_fora_do_deposito || !outras_com_dados.is_empty());
        PlanoDeDados {
            linha: linha.clone(),
            diretorio: self.pasta_de_dados_da_linha(linha),
            origem,
            outras_com_dados,
            perde_conversas,
        }
    }

    /// Executa o plano: para uma linha `Nova`, prepara a pasta e **só então**
    /// escreve o marcador. Uma linha `Propria` não é tocada.
    ///
    /// # Errors
    ///
    /// Falha de disco. O que uma aplicação interrompida deixa não passa por
    /// dados: sem marcador é um resto, e a próxima o apaga e recomeça.
    pub fn aplicar(&self, plano: &PlanoDeDados) -> std::io::Result<EstadoDosDados> {
        if plano.origem == OrigemDosDados::Nova {
            apagar_o_resto(&plano.diretorio)?;
            std::fs::create_dir_all(&plano.diretorio)?;
            marcar(&plano.diretorio, &plano.linha)?;
        }
        Ok(EstadoDosDados {
            plano: plano.clone(),
            diretorio: plano.diretorio.clone(),
        })
    }

    /// Copia `origem` para os dados desta linha, deixando de fora os nomes de
    /// primeiro nível em `deixar_de_fora`, e marca a cópia como inteira **por
    /// último**.
    ///
    /// É a primeira abertura da 0.16 (spec 0.16, §1): os dados da 0.15 viram
    /// os da linha 0.15 **antes** de a 0.16 migrar a pasta dela. A origem é a
    /// pasta do app, e o depósito mora dentro dela; daí o «deixar de fora», e
    /// daí a recusa quando o destino cai dentro da origem por um caminho que
    /// não foi deixado de fora: seria uma cópia que entra em si mesma.
    ///
    /// Uma linha que já tem os dados dela não é semeada de novo, porque semear
    /// por cima apagaria conversa escrita depois da cópia.
    ///
    /// # Errors
    ///
    /// `InvalidInput` na recusa acima; falha de disco no resto. Uma cópia
    /// interrompida não passa por dados: sem marcador, a próxima recomeça da
    /// origem.
    pub fn semear_linha(
        &self,
        linha: &Linha,
        origem: &Path,
        deixar_de_fora: &[&str],
    ) -> std::io::Result<Semeadura> {
        if self.tem_dados_da_linha(linha) {
            return Ok(Semeadura::JaTinha);
        }
        let destino = self.pasta_de_dados_da_linha(linha);
        if let Ok(dentro) = destino.strip_prefix(origem) {
            let primeiro = dentro
                .components()
                .next()
                .and_then(|c| c.as_os_str().to_str())
                .unwrap_or_default();
            if !deixar_de_fora.contains(&primeiro) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "o destino {} fica dentro da origem e não foi deixado de fora",
                        destino.display()
                    ),
                ));
            }
        }
        apagar_o_resto(&destino)?;
        std::fs::create_dir_all(&destino)?;
        let mut contagem = (0_u64, 0_u64);
        copiar(origem, &destino, true, deixar_de_fora, &mut contagem)?;
        marcar(&destino, linha)?;
        Ok(Semeadura::Semeada {
            arquivos: contagem.0,
            bytes: contagem.1,
        })
    }
}

/// Escreve o marcador, por último. Ver [`MARCADOR`].
fn marcar(diretorio: &Path, linha: &Linha) -> std::io::Result<()> {
    std::fs::write(diretorio.join(MARCADOR), format!("{linha}\n"))
}

/// Apaga o que sobrou de uma semeadura que parou no meio.
///
/// Só alcança diretório **sem** marcador, e é isso que a torna segura: uma
/// pasta sem marcador nunca foi a casa das conversas de ninguém — ela é uma
/// cópia interrompida, e recomeçar da origem, que não mudou, dá o resultado
/// certo.
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
/// do destino. Uma ligação é ignorada em vez de resolvida — a mesma postura que
/// `seele-proto::mods::inner_path` toma com o `..`.
///
/// `topo` é o que deixa o marcador da origem e os nomes de `deixar_de_fora`
/// para trás sem perder um arquivo de dados com o mesmo nome lá dentro.
/// `contagem` soma arquivos e bytes copiados.
fn copiar(
    de: &Path,
    para: &Path,
    topo: bool,
    deixar_de_fora: &[&str],
    contagem: &mut (u64, u64),
) -> std::io::Result<()> {
    for entrada in std::fs::read_dir(de)? {
        let entrada = entrada?;
        let tipo = entrada.file_type()?;
        let nome = entrada.file_name();
        if topo {
            // **O marcador da origem não é o marcador do destino.** O
            // `read_dir` não promete ordem nenhuma: copiado primeiro, ele
            // marcaria como inteira uma cópia que ainda está no meio.
            if nome == MARCADOR {
                continue;
            }
            if nome.to_str().is_some_and(|n| deixar_de_fora.contains(&n)) {
                continue;
            }
        }
        let destino = para.join(&nome);
        if tipo.is_symlink() {
            continue;
        }
        if tipo.is_dir() {
            std::fs::create_dir_all(&destino)?;
            copiar(&entrada.path(), &destino, false, deixar_de_fora, contagem)?;
        } else if tipo.is_file() {
            let bytes = std::fs::copy(entrada.path(), &destino)?;
            contagem.0 += 1;
            contagem.1 += bytes;
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::cenario::pasta;
    use crate::linha::Linha;

    fn linha(t: &str) -> Linha {
        Linha::nova(t).unwrap()
    }

    /// Os dados **inteiros** de uma linha, com uma «conversa» reconhecível e o
    /// marcador que diz que eles chegaram inteiros.
    fn com_conversa(deposito: &Deposito, l: &str, texto: &str) {
        let pasta = deposito.pasta_de_dados_da_linha(&linha(l));
        std::fs::create_dir_all(&pasta).unwrap();
        std::fs::write(pasta.join("conversas-de-teste.db"), texto).unwrap();
        marcar(&pasta, &linha(l)).unwrap();
    }

    /// **O critério de aceite:** cada linha tem o diretório de dados dela.
    #[test]
    fn duas_linhas_nunca_dividem_o_diretorio_de_dados() {
        let raiz = pasta("dados-separados");
        let deposito = Deposito::em(&raiz);
        assert_ne!(
            deposito.pasta_de_dados_da_linha(&linha("0.15")),
            deposito.pasta_de_dados_da_linha(&linha("1"))
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A primeira abertura da 0.16**: os dados da 0.15 viram os da linha
    /// 0.15, e o depósito, que mora dentro da pasta de origem, fica de fora.
    #[test]
    fn semear_uma_linha_copia_a_origem_sem_entrar_no_proprio_deposito() {
        let config = pasta("semear-config");
        std::fs::write(config.join("seele.db"), b"o banco da 0.15").unwrap();
        std::fs::create_dir_all(config.join("servidores/abc")).unwrap();
        std::fs::write(config.join("servidores/abc/seele.db"), b"outro banco").unwrap();
        let deposito = Deposito::em(config.join("lancador"));
        // O depósito já tem coisa dentro: uma instalação qualquer.
        std::fs::create_dir_all(config.join("lancador/versoes/x")).unwrap();
        std::fs::write(config.join("lancador/versoes/x/grande.bin"), b"nao copiar").unwrap();

        let feito = deposito
            .semear_linha(&linha("0.15"), &config, &["lancador"])
            .unwrap();

        let destino = deposito.pasta_de_dados_da_linha(&linha("0.15"));
        assert!(
            matches!(feito, Semeadura::Semeada { arquivos: 2, .. }),
            "veio {feito:?}"
        );
        assert_eq!(
            std::fs::read(destino.join("seele.db")).unwrap(),
            b"o banco da 0.15"
        );
        assert!(destino.join("servidores/abc/seele.db").is_file());
        assert!(
            !destino.join("lancador").exists(),
            "a cópia entrou no próprio depósito"
        );
        assert!(deposito.tem_dados_da_linha(&linha("0.15")));
        // A origem continua de pé: é a pasta da 0.16, que vai migrá-la depois.
        assert!(config.join("seele.db").is_file());
        let _ = std::fs::remove_dir_all(&config);
    }

    /// Esquecer o «deixar de fora» seria uma cópia que entra em si mesma.
    #[test]
    fn semear_recusa_quando_o_destino_esta_dentro_da_origem_e_nao_foi_deixado_de_fora() {
        let config = pasta("semear-recursiva");
        std::fs::write(config.join("seele.db"), b"x").unwrap();
        let deposito = Deposito::em(config.join("lancador"));
        let erro = deposito
            .semear_linha(&linha("0.15"), &config, &[])
            .expect_err("copiar a pasta para dentro dela mesma não termina");
        assert_eq!(erro.kind(), std::io::ErrorKind::InvalidInput);
        assert!(!deposito.tem_dados_da_linha(&linha("0.15")));
        let _ = std::fs::remove_dir_all(&config);
    }

    /// Uma linha que já tem os dados dela não é semeada de novo: semear por
    /// cima apagaria conversa escrita depois da cópia.
    #[test]
    fn semear_uma_linha_que_ja_tem_dados_nao_toca_nada() {
        let config = pasta("semear-ja-tinha");
        std::fs::write(config.join("seele.db"), b"o banco novo").unwrap();
        let deposito = Deposito::em(config.join("lancador"));
        com_conversa(&deposito, "0.15", "conversa escrita depois da copia");

        let feito = deposito
            .semear_linha(&linha("0.15"), &config, &["lancador"])
            .unwrap();

        assert_eq!(feito, Semeadura::JaTinha);
        assert_eq!(
            std::fs::read_to_string(
                deposito
                    .pasta_de_dados_da_linha(&linha("0.15"))
                    .join("conversas-de-teste.db")
            )
            .unwrap(),
            "conversa escrita depois da copia"
        );
        let _ = std::fs::remove_dir_all(&config);
    }

    /// **A semeadura interrompida não passa por dados da linha**, e a próxima
    /// recomeça da origem, sem o resto da tentativa de antes.
    #[test]
    fn uma_semeadura_que_parou_no_meio_recomeca_da_origem() {
        let config = pasta("semear-interrompida");
        std::fs::write(config.join("seele.db"), b"o banco da 0.15").unwrap();
        let deposito = Deposito::em(config.join("lancador"));
        let destino = deposito.pasta_de_dados_da_linha(&linha("0.15"));
        std::fs::create_dir_all(&destino).unwrap();
        std::fs::write(destino.join("seele.db"), b"metade").unwrap();
        std::fs::write(destino.join("so-da-tentativa-velha"), b"x").unwrap();
        assert!(!deposito.tem_dados_da_linha(&linha("0.15")));

        deposito
            .semear_linha(&linha("0.15"), &config, &["lancador"])
            .unwrap();

        assert_eq!(
            std::fs::read(destino.join("seele.db")).unwrap(),
            b"o banco da 0.15"
        );
        assert!(!destino.join("so-da-tentativa-velha").exists());
        assert!(deposito.tem_dados_da_linha(&linha("0.15")));
        let _ = std::fs::remove_dir_all(&config);
    }

    /// **A frase da tela:** abrir uma linha que começa vazia, havendo conversa
    /// em outro lugar, avisa antes.
    #[test]
    fn uma_linha_nova_avisa_quando_ha_conversa_fora_dela() {
        let raiz = pasta("plano-linha");
        let deposito = Deposito::em(&raiz);

        let limpa = deposito.plano_da_linha(&linha("0.15"), false);
        assert_eq!(limpa.origem, OrigemDosDados::Nova);
        assert!(!limpa.perde_conversas, "nada a perder numa máquina limpa");

        // A pasta do próprio app, fora do depósito, tem conversa.
        assert!(
            deposito
                .plano_da_linha(&linha("0.15"), true)
                .perde_conversas
        );

        // Outra linha do depósito tem conversa.
        com_conversa(&deposito, "1", "conversa da linha 1");
        let outra = deposito.plano_da_linha(&linha("0.15"), false);
        assert!(outra.perde_conversas);
        assert_eq!(outra.outras_com_dados, vec![linha("1")]);

        // E a linha que já tem dados é dela.
        com_conversa(&deposito, "0.15", "conversa da 0.15");
        let propria = deposito.plano_da_linha(&linha("0.15"), true);
        assert_eq!(propria.origem, OrigemDosDados::Propria);
        assert!(!propria.perde_conversas);
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Aplicar o plano de uma linha nova prepara a pasta e só então a marca;
    /// o de uma linha própria não toca em nada.
    #[test]
    fn aplicar_prepara_a_linha_nova_e_nao_toca_a_propria() {
        let raiz = pasta("aplicar-linha");
        let deposito = Deposito::em(&raiz);
        let estado = deposito
            .aplicar(&deposito.plano_da_linha(&linha("0.15"), false))
            .unwrap();
        assert!(estado.diretorio.is_dir());
        assert!(deposito.tem_dados_da_linha(&linha("0.15")));

        std::fs::write(estado.diretorio.join("conversas-de-teste.db"), b"escrita").unwrap();
        deposito
            .aplicar(&deposito.plano_da_linha(&linha("0.15"), false))
            .unwrap();
        assert_eq!(
            std::fs::read(estado.diretorio.join("conversas-de-teste.db")).unwrap(),
            b"escrita",
            "aplicar de novo apagou conversa"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn o_plano_nao_toca_em_disco() {
        let raiz = pasta("plano-seco");
        let deposito = Deposito::em(&raiz);
        let _ = deposito.plano_da_linha(&linha("0.15"), true);
        assert!(
            !raiz.join("dados").exists(),
            "decidir tem de poder acontecer antes de contar"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn uma_ligacao_simbolica_nao_faz_a_semeadura_escrever_fora_do_destino() {
        let config = pasta("ligacao");
        std::fs::write(config.join("seele.db"), b"o banco").unwrap();
        let pasta_de_fora = pasta("ligacao-fora");
        let fora = pasta_de_fora.join("fora-do-deposito.txt");
        std::fs::write(&fora, "nao devia ser copiado").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&fora, config.join("atalho.txt")).unwrap();
        let deposito = Deposito::em(config.join("lancador"));

        deposito
            .semear_linha(&linha("0.15"), &config, &["lancador"])
            .unwrap();

        let destino = deposito.pasta_de_dados_da_linha(&linha("0.15"));
        assert!(destino.join("seele.db").is_file());
        assert!(
            !destino.join("atalho.txt").exists(),
            "uma ligação é ignorada, não resolvida"
        );
        let _ = std::fs::remove_dir_all(&config);
        let _ = std::fs::remove_dir_all(&pasta_de_fora);
    }

    /// Uma pasta pela metade não é «conversa em outro lugar». Avisar que se
    /// perde o que nunca chegou a existir é ensinar a ignorar o aviso.
    #[test]
    fn uma_pasta_pela_metade_nao_conta_como_conversa_em_outro_lugar() {
        let raiz = pasta("meia-nao-conta");
        let deposito = Deposito::em(&raiz);
        let meia = deposito.pasta_de_dados_da_linha(&linha("1"));
        std::fs::create_dir_all(&meia).unwrap();
        std::fs::write(meia.join("conversas-de-teste.db"), "metade").unwrap();

        let plano = deposito.plano_da_linha(&linha("0.15"), false);

        assert_eq!(plano.origem, OrigemDosDados::Nova);
        assert!(plano.outras_com_dados.is_empty());
        assert!(
            !plano.perde_conversas,
            "não há conversa nenhuma nesta máquina para se perder"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// O marcador da origem não viaja na cópia. Se viajasse, o `read_dir`, que
    /// não garante ordem, poderia escrevê-lo primeiro, e a interrupção seguinte
    /// deixaria marcada como inteira uma pasta pela metade.
    #[test]
    fn o_marcador_da_origem_nao_e_copiado_junto() {
        let raiz = pasta("marcador-nao-viaja");
        let deposito = Deposito::em(&raiz);
        com_conversa(&deposito, "0.15", "conversa antiga");
        let origem = deposito.pasta_de_dados_da_linha(&linha("0.15"));
        let destino = raiz.join("copia-crua");
        std::fs::create_dir_all(&destino).unwrap();

        let mut contagem = (0, 0);
        copiar(&origem, &destino, true, &[], &mut contagem).unwrap();

        assert!(destino.join("conversas-de-teste.db").is_file());
        assert!(
            !destino.join(MARCADOR).exists(),
            "quem marca o destino é `marcar`, depois de a cópia terminar"
        );
        assert_eq!(contagem.0, 1);
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// As linhas com dados saem da mais nova para a mais velha, e uma pasta que
    /// não é `servidor-<linha>` não entra.
    #[test]
    fn as_linhas_com_dados_vem_da_mais_nova() {
        let raiz = pasta("linhas-com-dados");
        let deposito = Deposito::em(&raiz);
        com_conversa(&deposito, "0.15", "a");
        com_conversa(&deposito, "1", "b");
        std::fs::create_dir_all(raiz.join("dados/0.10.5")).unwrap();
        assert_eq!(deposito.linhas_com_dados(), vec![linha("1"), linha("0.15")]);
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
