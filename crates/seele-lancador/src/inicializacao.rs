//! Iniciar a versão escolhida, com os dados dela.
//!
//! O passo mais curto do launcher e o mais fácil de errar em silêncio: um
//! processo iniciado com o executável certo e o diretório de dados **errado**
//! abre, funciona, e escreve as conversas no lugar de outra versão. Não há
//! sintoma até alguém trocar de versão e não achar o que escreveu.
//!
//! # A variável já existia
//!
//! `SEELE_HOME` é o primeiro degrau de `config_dir` no `seele-app` e de
//! `arquivo_de_log`, antes de `XDG_CONFIG_HOME` e de `HOME`. O launcher não
//! precisa de mecanismo novo: ele preenche a variável que o produto já
//! obedece. Vale para o banco, para as preferências e para o rastro — os três
//! saem da mesma função.
//!
//! Reusar em vez de inventar tem uma consequência que vale dizer: uma versão
//! **anterior** ao launcher, iniciada por ele, também obedece. Versões lado a
//! lado funcionam com o que já está publicado.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use crate::resolucao::VersaoResolvida;

/// A variável de ambiente que diz ao produto onde ficam os dados dele.
///
/// A mesma que `apps/seele-app/src/main.rs` já lê em `config_dir`.
pub const VARIAVEL_DE_DADOS: &str = "SEELE_HOME";

/// A variável que diz à versão aberta que quem a abriu foi o launcher.
///
/// Valor `1`. Aberta assim, a versão mora no depósito do launcher, e o
/// atualizador do Tauri trocaria o app de lá pela versão do topo do manifesto —
/// outra linha. A 0.15.2 e a 0.16 a leem e escondem e recusam o atualizador
/// (spec 0.16, D-linha-1). Uma versão de antes dela a ignora, e é por isso que a
/// linha legada sai numa 0.15.2, e não na 0.15.1.
pub const VARIAVEL_DO_LANCADOR: &str = "SEELE_LANCADOR";

/// A variável que diz à versão aberta onde mora a identidade da máquina.
///
/// ADR 0061, «O contrato da variável»: o valor é o caminho absoluto de uma
/// **pasta**, e a chave é `<pasta>/identity.key`. A identidade é uma por
/// máquina: sem isto, a versão aberta criaria uma chave nova na pasta de dados
/// da linha, e a pessoa seria outra em cada linha — com outro apelido nos
/// servidores dela e outro código da sala. Só a chave sai daí; o resto continua
/// na pasta de dados, a de [`VARIAVEL_DE_DADOS`].
pub const VARIAVEL_DA_IDENTIDADE: &str = "SEELE_IDENTIDADE";

/// O que iniciar, e com o quê. Montado, não executado.
///
/// Separado da execução de propósito: é o que permite provar, em teste e sem
/// abrir processo nenhum, que a versão escolhida vai receber o diretório dela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lancamento {
    /// O executável.
    pub programa: PathBuf,
    /// Os argumentos, sem o nome do programa.
    pub argumentos: Vec<String>,
    /// O que acrescentar ao ambiente.
    pub ambiente: Vec<(String, String)>,
}

impl Lancamento {
    /// Monta o lançamento de uma versão resolvida.
    ///
    /// Os `argumentos` são repassados como vieram — é por onde um `--hospedar`
    /// ou uma URI `seele://` chega à versão escolhida.
    ///
    /// `pasta_da_identidade` é a da identidade **do processo que lança** (ADR
    /// 0061): a `SEELE_IDENTIDADE` que ele recebeu, ou a pasta de configuração
    /// dele. É obrigatória, e não um `Option`: um lançamento sem ela é uma
    /// versão que cria uma chave nova, e a pessoa vira outra sem ninguém ver.
    #[must_use]
    pub fn de(
        resolvida: &VersaoResolvida,
        argumentos: Vec<String>,
        pasta_da_identidade: &std::path::Path,
    ) -> Self {
        Self {
            programa: resolvida.executavel.clone(),
            argumentos,
            ambiente: vec![
                (
                    VARIAVEL_DE_DADOS.to_owned(),
                    resolvida.dados.to_string_lossy().into_owned(),
                ),
                (VARIAVEL_DO_LANCADOR.to_owned(), "1".to_owned()),
                (
                    VARIAVEL_DA_IDENTIDADE.to_owned(),
                    pasta_da_identidade.to_string_lossy().into_owned(),
                ),
            ],
        }
    }

    /// Inicia o processo.
    ///
    /// **Não espera por ele**, e não é descuido: o launcher que segurasse o
    /// processo filho manteria duas janelas de SEELE na barra de tarefas, e o
    /// ADR 0039 decidiu que o produto tem uma casca só.
    ///
    /// # Errors
    ///
    /// O que o sistema disser ao tentar iniciar. Um executável que sumiu entre
    /// a resolução e aqui cai neste caminho, e não em pânico.
    pub fn iniciar(&self) -> std::io::Result<std::process::Child> {
        let mut comando = std::process::Command::new(&self.programa);
        comando.args(&self.argumentos);
        for (nome, valor) in &self.ambiente {
            comando.env(nome, valor);
        }
        comando.spawn()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::cenario::{manifesto, pacote, pasta, vigente, DesempacotadorDeTeste, ALVO};
    use crate::deposito::Deposito;
    use crate::resolucao::{Ato, Pedido, Resolvedor};
    use crate::versao::Versao;

    /// **O critério de aceite, do lado de quem inicia:** o processo de cada
    /// versão recebe o diretório de dados daquela versão.
    #[test]
    fn o_lancamento_de_cada_versao_aponta_para_os_dados_dela() {
        let raiz = pasta("lancamento");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["2.0.0-teste", "1.0.0-teste"], &[]);
        for v in ["2.0.0-teste", "1.0.0-teste"] {
            let p = m.publicacao(&Versao::nova(v).unwrap()).unwrap().clone();
            deposito
                .instalar(
                    &p,
                    ALVO,
                    &pacote(v),
                    &crate::cenario::chave(),
                    &DesempacotadorDeTeste,
                )
                .unwrap();
        }
        let sem_revogacao = vigente(&m, &deposito);
        let r = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao);

        let nova = Lancamento::de(
            &r.resolver(&Pedido::MaisNova, Ato::Abrir).unwrap(),
            vec![],
            &raiz.join("identidade"),
        );
        let velha = Lancamento::de(
            &r.resolver(
                &Pedido::Exata(Versao::nova("1.0.0-teste").unwrap()),
                Ato::Abrir,
            )
            .unwrap(),
            vec![],
            &raiz.join("identidade"),
        );

        let dados_novos = nova
            .ambiente
            .iter()
            .find(|(n, _)| n == VARIAVEL_DE_DADOS)
            .map(|(_, v)| v.clone())
            .expect("o lançamento tem de dizer onde ficam os dados");
        let dados_velhos = velha
            .ambiente
            .iter()
            .find(|(n, _)| n == VARIAVEL_DE_DADOS)
            .map(|(_, v)| v.clone())
            .expect("o lançamento tem de dizer onde ficam os dados");

        // **Comparado como caminho, e não como texto.** Aqui estava escrito
        // `dados_novos.ends_with("dados/2.0.0-teste")`, e `dados_novos` é uma
        // `String`: a barra ia crua para dentro da comparação. No Windows o
        // valor termina em `dados\\2.0.0-teste`, e o teste reprovava com o
        // produto certo — encontrado rodando a bateria naquela máquina, no
        // fechamento da v0.11.0.
        //
        // O conserto não é trocar a barra: é parar de afirmar sobre a forma do
        // texto. O que este teste tem a dizer é que o lançamento aponta para a
        // pasta de dados **daquela versão**, e o depósito sabe qual é.
        //
        // O formato `dados/servidor-<linha>` continua preso, em
        // `resolucao::testes::cada_versao_resolve_para_o_executavel_e_os_dados_dela`:
        // lá o campo é `PathBuf`, e `Path::ends_with` compara componente a
        // componente, que atravessa os três sistemas.
        for (valor, versao) in [
            (&dados_novos, "2.0.0-teste"),
            (&dados_velhos, "1.0.0-teste"),
        ] {
            let linha =
                crate::Linha::do_servidor(&crate::cenario::servidor_de_teste(versao)).unwrap();
            assert_eq!(
                std::path::Path::new(valor),
                deposito.pasta_de_dados_da_linha(&linha),
                "o lançamento de {versao} não aponta para a pasta de dados da linha dela"
            );
        }
        assert_ne!(dados_novos, dados_velhos);
        assert_ne!(nova.programa, velha.programa);
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn os_argumentos_chegam_a_versao_escolhida_como_vieram() {
        let raiz = pasta("argumentos");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let p = m
            .publicacao(&Versao::nova("1.0.0-teste").unwrap())
            .unwrap()
            .clone();
        deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &crate::cenario::chave(),
                &DesempacotadorDeTeste,
            )
            .unwrap();
        let sem_revogacao = vigente(&m, &deposito);
        let resolvida = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao)
            .resolver(&Pedido::MaisNova, Ato::Abrir)
            .unwrap();

        let lancamento = Lancamento::de(
            &resolvida,
            vec![
                "--hospedar".to_owned(),
                "seele://exemplo.invalido".to_owned(),
            ],
            &raiz.join("identidade"),
        );

        assert_eq!(
            lancamento.argumentos,
            vec!["--hospedar", "seele://exemplo.invalido"]
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A guarda da linha legada, do lado de quem abre.** A versão aberta
    /// precisa saber que quem a abriu foi o launcher: aberta assim, ela mora
    /// no depósito, e o atualizador dela trocaria o app de lá por outra linha.
    /// A 0.15.2 e a 0.16 escondem e recusam o atualizador com isto.
    #[test]
    fn a_versao_aberta_sabe_que_quem_a_abriu_foi_o_launcher() {
        let raiz = pasta("aberta-pelo-launcher");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let p = m.mais_nova().clone();
        deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &crate::cenario::chave(),
                &DesempacotadorDeTeste,
            )
            .unwrap();
        let sem_revogacao = vigente(&m, &deposito);
        let resolvida = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao)
            .resolver(&Pedido::MaisNova, Ato::Abrir)
            .unwrap();

        let lancamento = Lancamento::de(&resolvida, vec![], &raiz.join("identidade"));

        assert!(
            lancamento
                .ambiente
                .contains(&(VARIAVEL_DO_LANCADOR.to_owned(), "1".to_owned())),
            "sem a variável, a versão aberta mostra o próprio botão de atualizar e \
             troca o app do depósito: {:?}",
            lancamento.ambiente
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **A identidade é uma por máquina** (ADR 0061). Toda versão aberta pelo
    /// launcher recebe a pasta da identidade dele, e não cria uma chave nova na
    /// pasta de dados da linha — senão a pessoa seria outra em cada linha.
    #[test]
    fn a_versao_aberta_le_a_identidade_da_maquina() {
        let raiz = pasta("identidade-da-maquina");
        let deposito = Deposito::em(&raiz);
        let m = manifesto(&["1.0.0-teste"], &[]);
        let p = m.mais_nova().clone();
        deposito
            .instalar(
                &p,
                ALVO,
                &pacote("1.0.0-teste"),
                &crate::cenario::chave(),
                &DesempacotadorDeTeste,
            )
            .unwrap();
        let sem_revogacao = vigente(&m, &deposito);
        let resolvida = Resolvedor::novo(&m, &deposito, ALVO, &sem_revogacao)
            .resolver(&Pedido::MaisNova, Ato::Abrir)
            .unwrap();
        let maquina = raiz.join("a-pasta-da-maquina");

        let lancamento = Lancamento::de(&resolvida, vec![], &maquina);

        let identidade = lancamento
            .ambiente
            .iter()
            .find(|(nome, _)| nome == VARIAVEL_DA_IDENTIDADE)
            .map(|(_, valor)| PathBuf::from(valor))
            .expect("o lançamento tem de dizer onde mora a identidade da máquina");
        assert_eq!(identidade, maquina);
        assert_ne!(
            identidade, resolvida.dados,
            "a identidade apontou para a pasta de dados da linha: uma chave por linha"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn iniciar_um_executavel_que_sumiu_devolve_erro_em_vez_de_panico() {
        let lancamento = Lancamento {
            programa: PathBuf::from("/caminho/que/nao/existe/seele-de-teste"),
            argumentos: vec![],
            ambiente: vec![],
        };
        assert!(lancamento.iniciar().is_err());
    }
}
