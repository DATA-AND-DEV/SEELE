//! Quem declarou identidade para o caminho entre pares — e, entre esses,
//! quem empresta a subida agora.
//!
//! # A escolha aqui é deliberadamente burra
//!
//! Ela aponta o primeiro que declarou que empresta, não é quem compartilha, e
//! ainda não serve ninguém. É um espaço reservado com a forma certa: o
//! **subprojeto B** é quem olha subida medida e topologia para escolher bem.
//! Chamar isto de «escolha automática» seria vender como pronto o que é um
//! lugar guardado — e a spec de 05/09 diz isso com todas as letras.
//!
//! # Por que a identidade sobrevive a `emprestando: false`
//!
//! Achado do fix round 1 da Task 8, sobre um ruling meu de pré-voo que
//! misturava duas perguntas diferentes: **quem eu sou** e **eu empresto**. A
//! parede simétrica da Task 5 exige certificado dos dois lados de toda
//! ligação entre pares — quem atende confere quem chega contra a impressão
//! que o servidor apresentou. Quem só assiste (nunca opta por emprestar)
//! também disca com a própria identidade quando `SirvaTelaPara` manda alguém
//! procurá-lo; se a única mensagem que carrega impressão a apagasse ao dizer
//! «não empresto», ninguém que só assistisse teria certificado para
//! apresentar, e a discagem dele seria sempre recusada como `SemCertificado`.
//! Por isso `declarou` guarda sempre, e só `escolher` olha `emprestando`.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

use seele_proto::ids::{PersonId, ScreenId};

/// Uma identidade e onde alcançá-la, que uma pessoa declarou para o caminho
/// entre pares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuemDeclarou {
    /// Quem.
    pub pessoa: PersonId,
    /// A impressão digital que apresenta.
    pub impressao: String,
    /// Onde alcançá-la: os locais que declarou, mais o público que o servidor
    /// **viu**. Nesta ordem, porque a rede local dispensa furo e é a que
    /// responde mais rápido — a mesma razão do ADR 0037.
    pub enderecos: Vec<SocketAddr>,
    /// Se, além de existir, também empresta a subida agora.
    ///
    /// É só este campo que [`Pares::escolher`] olha. Ter identidade aqui e
    /// `emprestando: false` é o caso comum de quem só assiste — ver o doc do
    /// módulo.
    pub emprestando: bool,
}

/// Quem declarou identidade para o caminho entre pares nesta sala, agora.
#[derive(Debug, Default)]
pub struct Pares {
    quem: HashMap<PersonId, QuemDeclarou>,
    /// Quem o servidor apontou por último para servir cada transmissão.
    ///
    /// **A própria nomeação do servidor, guardada para poder ser desfeita.**
    /// Achado do fix round 2: sem isto, um `ParFalhou { screen }` não tem
    /// como saber de quem reclamar — ele só carrega a transmissão, e nunca
    /// deveria carregar a identidade de quem falhou, porque quem relata é a
    /// vítima, não quem investiga. Ver [`Self::apontou`].
    nomeacoes: HashMap<ScreenId, PersonId>,
}

impl Pares {
    /// Ninguém declarado ainda.
    #[must_use]
    pub fn nova() -> Self {
        Self::default()
    }

    /// Alguém declarou identidade — e disse se empresta a subida com ela.
    ///
    /// **Guarda sempre**, `emprestando` sendo o que for. Um `false` não
    /// apaga a declaração: significa «esta sou eu, e não empresto agora», não
    /// «esqueça que existo» — só [`Self::saiu`] apaga, porque só a saída da
    /// sessão torna a identidade obsoleta.
    ///
    /// `publico` é a origem da conexão desta pessoa, vista pelo servidor.
    pub fn declarou(
        &mut self,
        pessoa: PersonId,
        emprestando: bool,
        impressao: String,
        locais: Vec<SocketAddr>,
        publico: SocketAddr,
    ) {
        let mut enderecos = locais;
        if !enderecos.contains(&publico) {
            enderecos.push(publico);
        }
        self.quem.insert(
            pessoa,
            QuemDeclarou {
                pessoa,
                impressao,
                enderecos,
                emprestando,
            },
        );
    }

    /// Esta pessoa saiu. A identidade é efêmera e não sobrevive à sessão —
    /// sem isto, uma discagem futura apontaria para uma impressão de uma
    /// sessão que não existe mais, e não só a escolha de quem serve.
    ///
    /// Também esquece toda nomeação que apontava para ela: uma transmissão
    /// que ainda "apontasse" para quem já foi embora resolveria um
    /// `ParFalhou` contra ninguém.
    pub fn saiu(&mut self, pessoa: PersonId) {
        self.quem.remove(&pessoa);
        self.nomeacoes.retain(|_, quem| *quem != pessoa);
    }

    /// A declaração desta pessoa, exista ela para emprestar ou só para ser
    /// alcançada.
    ///
    /// `None` se ela nunca declarou, ou já saiu. Quem vai montar
    /// `SirvaTelaPara`/`AssistaTelaPor` precisa disto para a identidade de
    /// **quem pediu** — `escolher` só devolve a de quem empresta.
    #[must_use]
    pub fn declaracao_de(&self, pessoa: PersonId) -> Option<&QuemDeclarou> {
        self.quem.get(&pessoa)
    }

    /// Quem pode servir esta transmissão a esta pessoa, se alguém.
    ///
    /// Só considera quem declarou `emprestando: true` — ter identidade
    /// guardada não é o mesmo que ter optado por emprestar. Ver o doc de
    /// [`QuemDeclarou::emprestando`] e do módulo.
    #[must_use]
    pub fn escolher(
        &self,
        dono: PersonId,
        quem_quer: PersonId,
        ja_servindo: &HashSet<PersonId>,
    ) -> Option<QuemDeclarou> {
        self.quem
            .values()
            .find(|candidato| {
                candidato.emprestando
                    && candidato.pessoa != dono
                    && candidato.pessoa != quem_quer
                    && !ja_servindo.contains(&candidato.pessoa)
            })
            .cloned()
    }

    /// O servidor apontou `quem` para servir `screen`.
    ///
    /// Chamado por quem despacha `SirvaTelaPara`/`AssistaTelaPor`, depois de
    /// [`Self::escolher`] decidir — ainda sem chamador em produção; é o
    /// despacho da Task 10. Substitui a nomeação anterior desta transmissão,
    /// se havia uma: só a mais recente importa para resolver um `ParFalhou`.
    pub fn apontou(&mut self, screen: ScreenId, quem: PersonId) {
        self.nomeacoes.insert(screen, quem);
    }

    /// Quem foi apontado por último para servir esta transmissão, se alguém.
    ///
    /// É contra isto que um `ClientMessage::ParFalhou { screen }` se resolve:
    /// a mensagem só carrega a transmissão, nunca a identidade de quem
    /// falhou, porque quem relata é quem estava esperando a imagem — a
    /// vítima, não quem investiga.
    #[must_use]
    pub fn quem_foi_apontado(&self, screen: ScreenId) -> Option<PersonId> {
        self.nomeacoes.get(&screen).copied()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
)]
mod testes {
    use super::*;

    fn endereco(n: u8) -> SocketAddr {
        SocketAddr::from(([192, 168, 1, n], 8383))
    }

    #[test]
    fn quem_compartilha_nunca_e_escolhido_para_servir_a_si_mesmo() {
        // O espelho infinito, na versão da malha: quem compartilha servindo a
        // própria tela a si mesmo. `crate::voice_room` já prende isto para o
        // caminho do servidor; aqui é a mesma regra no caminho novo.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(1),
            true,
            "a".repeat(64),
            vec![endereco(1)],
            endereco(1),
        );
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new())
            .is_none());
    }

    #[test]
    fn quem_nao_empresta_nunca_e_escolhido_mesmo_com_impressao_guardada() {
        // **A regra nova do fix round 1.** Antes, `impressao` vazia era o
        // sinal de "não empresto" e `declarou` apagava a pessoa inteira. Agora
        // quem só assiste também declara identidade (para poder apresentar
        // certificado quando `SirvaTelaPara` mandar alguém discar para ela) —
        // e a impressão continua guardada mesmo com `emprestando: false`.
        // `escolher` tem de respeitar o opt-in olhando o campo `emprestando`,
        // e não mais se há impressão guardada; do contrário quem disse "não
        // empresto" seria escolhido mesmo assim.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            true,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.declarou(
            PersonId(3),
            false,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new())
            .is_none());
    }

    #[test]
    fn quem_ja_esta_servindo_nao_e_escolhido_de_novo() {
        // **Um par por vez, no A1.** Quantos um cliente aguenta é a conta do
        // subprojeto B, e supor «dois» aqui seria inventar um número que
        // ninguém mediu.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            true,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        let ja = HashSet::from([PersonId(3)]);
        assert!(pares.escolher(PersonId(1), PersonId(2), &ja).is_none());
    }

    #[test]
    fn o_endereco_publico_vem_do_servidor_e_nao_do_cliente() {
        // Um endereço público que o cliente afirma é um endereço que ele pode
        // mentir — e mentir aqui manda outra pessoa discar para onde o mentiroso
        // quiser. O servidor vê a origem da conexão; é ela que vale.
        let mut pares = Pares::nova();
        let publico = SocketAddr::from(([203, 0, 113, 9], 8383));
        pares.declarou(
            PersonId(3),
            true,
            "c".repeat(64),
            vec![endereco(3)],
            publico,
        );
        let escolhido = pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new())
            .unwrap();
        assert!(escolhido.enderecos.contains(&publico));
        assert!(escolhido.enderecos.contains(&endereco(3)));
    }

    #[test]
    fn a_impressao_sobrevive_a_emprestando_false() {
        // **O guarda que faltava no round 1.** O teste anterior
        // (`quem_nao_empresta_nunca_e_escolhido_mesmo_com_impressao_guardada`)
        // só afirma a metade `escolher`; esta prova a outra metade — que
        // `declarou` de fato **guarda** a declaração de quem só assiste, e
        // não a apaga por `emprestando` ser falso. Sem este teste, um
        // `declarou` revertido para o comportamento velho (apagar quando a
        // pessoa "não empresta") passaria pela suíte inteira sem tropeçar:
        // nenhum outro teste lê a declaração de volta.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            false,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        let declaracao = pares
            .declaracao_de(PersonId(3))
            .expect("a declaração de quem só assiste desapareceu");
        assert_eq!(declaracao.impressao, "c".repeat(64));
        assert!(declaracao.enderecos.contains(&endereco(3)));
        assert!(!declaracao.emprestando);
    }

    #[test]
    fn saiu_apaga_tambem_a_declaracao() {
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            false,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.saiu(PersonId(3));
        assert!(
            pares.declaracao_de(PersonId(3)).is_none(),
            "a identidade sobreviveu à saída da sessão"
        );
    }

    #[test]
    fn um_parfalhou_resolve_contra_a_propria_nomeacao_e_nao_contra_quem_relata() {
        // **Achado do fix round 2.** `ParFalhou { screen }` não carrega quem
        // falhou — só quem relata sabe que a imagem parou, e relatar não é o
        // mesmo que saber a identidade do impostor. O servidor tem de
        // resolver `screen` contra a própria nomeação (`apontou`), não contra
        // `session.person` do despacho — esse é sempre quem relatou, a
        // vítima, nunca o par apontado.
        let mut pares = Pares::nova();
        let tela = ScreenId(9);
        pares.apontou(tela, PersonId(5));
        assert_eq!(pares.quem_foi_apontado(tela), Some(PersonId(5)));
    }

    #[test]
    fn quem_sai_deixa_de_ser_a_resposta_de_uma_nomeacao_velha() {
        let mut pares = Pares::nova();
        let tela = ScreenId(9);
        pares.apontou(tela, PersonId(5));
        pares.saiu(PersonId(5));
        assert_eq!(
            pares.quem_foi_apontado(tela),
            None,
            "quem já foi embora continuou sendo a resposta de uma nomeação"
        );
    }
}
