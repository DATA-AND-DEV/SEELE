//! Quem declarou identidade para o caminho entre pares — e, entre esses,
//! quem empresta a subida agora.
//!
//! # A escolha aqui é deliberadamente burra
//!
//! Ela não segue critério visível nenhum — nem latência, nem ordem de
//! chegada: `self.quem` é um `HashMap`, e a ordem de iteração dele não é a de
//! inserção. O que ela garante é só isto — não é quem compartilha, ainda não
//! serve ninguém, e **está na mesma sala de voz da transmissão**. É um espaço
//! reservado com a forma certa: o
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
//!
//! # Por que a declaração carrega a conexão que a fez
//!
//! Achado do fix round 3. Uma corrida de candidatos (ADR 0037, do lado do
//! cliente) pode deixar duas conexões vivas com o mesmo `PersonId` ao mesmo
//! tempo: cada candidato completa o próprio aperto de mão contra este
//! servidor antes de a corrida decidir quem venceu. Se `saiu` apagasse pela
//! pessoa sozinha, o encerramento do candidato perdedor — que chega **depois**
//! de o vencedor já ter declarado — apagaria a declaração viva. Por isso
//! [`QuemDeclarou`] guarda a conexão que a fez, e [`Pares::saiu`] só apaga se
//! ainda for a mesma.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

use seele_proto::control::MotivoDeFalhaDePar;
use seele_proto::ids::{PersonId, ScreenId};

/// Uma identidade e onde alcançá-la, que uma pessoa declarou para o caminho
/// entre pares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuemDeclarou {
    /// Quem.
    pub pessoa: PersonId,
    /// Qual conexão fez esta declaração — `quinn::Connection::stable_id`,
    /// como `session.rs` já usa para `Subida::esquecer`.
    ///
    /// **Não é decoração.** Uma corrida de candidatos pode ter duas conexões
    /// vivas com o mesmo `PersonId`; sem isto, o encerramento de uma conexão
    /// que perdeu a corrida apagaria a declaração de uma que ganhou. Ver o
    /// doc do módulo.
    ///
    /// # A ressalva, e o que quebra se ela cair
    ///
    /// `stable_id()` é, no `quinn` 0.11, o **endereço de alocação** do estado
    /// interno da conexão. Ele é estável enquanto a conexão vive, que é tudo
    /// o que este campo precisa — mas ele pode ser **reusado** depois que ela
    /// morre, porque o alocador pode devolver o mesmo endereço a outra
    /// conexão.
    ///
    /// Se essa suposição cair, [`Pares::saiu`] passa a apagar a declaração
    /// **viva** de outra conexão: o encerramento tardio de uma conexão morta
    /// chegaria com um `id_da_conexao` que agora pertence a outra pessoa, a
    /// conferência bateria por engano, e quem acabou de declarar sairia da
    /// malha sem ter feito nada — caladamente, porque não há erro nenhum a
    /// dar nesse caminho.
    ///
    /// Este campo passou de decoração a **carregar a correção** de uma rodada
    /// de conserto; a suposição está escrita aqui, onde a invariante mora, e
    /// não num relatório. O dia em que `stable_id` deixar de servir, é este
    /// doc que diz o que pôr no lugar: um identificador de sessão que o
    /// servidor mesmo atribua, e que não venha do alocador de ninguém.
    pub id_da_conexao: u64,
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

/// Quem declarou identidade para o caminho entre pares neste daemon, agora.
///
/// **Global ao daemon, e não por sala de voz.** Uma pessoa declara identidade
/// uma vez por sessão, e continua a mesma pessoa ao trocar de sala. É por isso
/// que [`Self::escolher`] recebe de fora quem está na sala da transmissão: a
/// pergunta «quem é você» mora aqui, e a pergunta «onde você está agora» mora
/// na `crate::server::Occupancy`, que é reescrita a cada entrada e saída.
#[derive(Debug, Default)]
pub struct Pares {
    quem: HashMap<PersonId, QuemDeclarou>,
    /// Quem o servidor apontou para servir cada transmissão **a cada
    /// espectador**: `(transmissão, quem assiste) → quem empresta`.
    ///
    /// **A própria nomeação do servidor, guardada para poder ser desfeita.**
    /// Achado do fix round 2: sem isto, um `ParFalhou { screen }` não tem
    /// como saber de quem reclamar — ele só carrega a transmissão, e nunca
    /// deveria carregar a identidade de quem falhou, porque quem relata é a
    /// vítima, não quem investiga. Ver [`Self::apontou`].
    ///
    /// **Quem assiste faz parte da chave, e não é enfeite.** Enquanto a chave
    /// era só a transmissão, uma tela com dois espectadores pela malha cabia
    /// numa entrada só: o segundo `WatchScreen` sobrescrevia a nomeação do
    /// primeiro, o par que servia o primeiro sumia de [`Self::ja_servindo`] —
    /// e voltava a ser escolhível enquanto ainda repassava — e um
    /// `UnwatchScreen` de qualquer um dos dois soltava a vaga dos dois. Uma
    /// transmissão tem um dono e vários espectadores; a nomeação é por
    /// espectador, porque é por espectador que o repasse existe.
    nomeacoes: HashMap<(ScreenId, PersonId), PersonId>,
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
    /// `id_da_conexao` é de quem esta declaração pertence — ver o doc de
    /// [`QuemDeclarou::id_da_conexao`]. `publico` é a origem da conexão desta
    /// pessoa, vista pelo servidor.
    pub fn declarou(
        &mut self,
        pessoa: PersonId,
        id_da_conexao: u64,
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
                id_da_conexao,
                impressao,
                enderecos,
                emprestando,
            },
        );
    }

    /// Esta conexão saiu. A identidade é efêmera e não sobrevive à sessão que
    /// a declarou — mas só a ela.
    ///
    /// **Verificado por conexão, não só por pessoa** — achado do fix round 3.
    /// Uma corrida de candidatos pode deixar duas conexões vivas com o mesmo
    /// `PersonId`; se esta função apagasse por `pessoa` sozinha, o
    /// encerramento de uma candidata que perdeu a corrida — chegando depois
    /// de a vencedora já ter declarado — apagaria a declaração viva. Se
    /// `id_da_conexao` não bater com o que está publicado, não há nada a
    /// fazer: outra conexão já substituiu esta declaração.
    pub fn saiu(&mut self, pessoa: PersonId, id_da_conexao: u64) {
        let e_esta_conexao = self
            .quem
            .get(&pessoa)
            .is_some_and(|declarado| declarado.id_da_conexao == id_da_conexao);
        if e_esta_conexao {
            self.esquecer(pessoa);
        }
        // **Fora do `if`, e de propósito.** A conferência por conexão existe
        // para não apagar a *declaração* de uma conexão que venceu a corrida
        // de candidatos. Uma nomeação feita para esta pessoa não é declaração
        // de ninguém: quem assiste foi embora, o repasse acabou, e o par
        // apontado tem de voltar à fila seja qual for a conexão que fechou.
        self.quem_assiste_saiu(pessoa);
    }

    /// Desacredita a declaração desta pessoa, **seja qual for a conexão que a
    /// fez**.
    ///
    /// Diferente de [`Self::saiu`]: ali o motivo é a conexão ter acabado, e a
    /// checagem por `id_da_conexao` existe para não confundir sessões.
    /// Aqui o motivo é a **declaração** ter sido provada falsa —
    /// `ImpressaoNaoBate`, quando alguém respondeu no lugar de quem foi
    /// apontado — e a conexão que a fez pode continuar perfeitamente viva; é
    /// a identidade publicada que deixou de merecer confiança. Ver
    /// [`quem_desacreditar`].
    pub fn desacreditar(&mut self, pessoa: PersonId) {
        self.esquecer(pessoa);
    }

    /// O que [`Self::saiu`] e [`Self::desacreditar`] têm em comum: apagar a
    /// declaração e qualquer nomeação que apontava para ela.
    fn esquecer(&mut self, pessoa: PersonId) {
        self.quem.remove(&pessoa);
        self.nomeacoes.retain(|_, empresta| *empresta != pessoa);
    }

    /// Esta pessoa deixou de assistir a tudo — saiu da sala, ou a sessão dela
    /// acabou.
    ///
    /// **É metade do conserto de «cada repasse queima um par para sempre».**
    /// A nomeação existe enquanto alguém está sendo servido; quem assiste
    /// indo embora encerra o repasse tanto quanto um `UnwatchScreen`, e sem
    /// esta linha o par apontado ficaria contado em [`Self::ja_servindo`] pelo
    /// resto da sessão do daemon, enquanto o cliente dele já devolveu a vaga.
    /// Os dois lados discordariam em silêncio, e a malha degradaria para a
    /// estrela sem um rastro.
    pub fn quem_assiste_saiu(&mut self, pessoa: PersonId) {
        self.nomeacoes.retain(|(_, assiste), _| *assiste != pessoa);
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
    ///
    /// # `na_sala` não é um refinamento: é a parede
    ///
    /// Este registro é **global ao daemon**, e não por sala: uma pessoa
    /// declara identidade uma vez por sessão, não uma vez por sala em que
    /// senta. Sem `na_sala`, quem empresta na sala B era escolhido para servir
    /// a tela da sala A — e o cliente dele repassa o que **ele** está
    /// recebendo, que é a tela da sala B. Quem assiste na sala A recebia,
    /// decodificava e mostrava conteúdo de uma sala em que nunca entrou.
    ///
    /// O §5 do desenho justifica a privacidade do repasse dizendo que «quem
    /// repassa já é espectador autorizado daquele fluxo». A frase só é
    /// verdade se quem repassa e quem recebe estiverem na mesma sala, e é
    /// esta linha que faz disso um fato em vez de uma suposição.
    ///
    /// `na_sala` é quem está **agora** na sala de voz da transmissão, e vem de
    /// fora de propósito: a fonte viva é `crate::server::Occupancy`, que é
    /// reescrita a cada entrada e saída. Guardar a sala dentro de
    /// [`QuemDeclarou`] daria um campo escrito uma vez na declaração e nunca
    /// mais — e pessoas trocam de sala sem redeclarar nada.
    #[must_use]
    pub fn escolher(
        &self,
        dono: PersonId,
        quem_quer: PersonId,
        ja_servindo: &HashSet<PersonId>,
        na_sala: &HashSet<PersonId>,
    ) -> Option<QuemDeclarou> {
        self.quem
            .values()
            .find(|candidato| {
                candidato.emprestando
                    && candidato.pessoa != dono
                    && candidato.pessoa != quem_quer
                    && !ja_servindo.contains(&candidato.pessoa)
                    && na_sala.contains(&candidato.pessoa)
            })
            .cloned()
    }

    /// Quem já está apontado para servir alguma transmissão agora.
    ///
    /// É o `ja_servindo` que [`Self::escolher`] recebe. Sai das próprias
    /// nomeações do servidor porque elas são o único registro de quem ele já
    /// pôs para trabalhar — contar de outro lugar seria uma segunda conta a
    /// discordar desta no primeiro dia ruim.
    #[must_use]
    pub fn ja_servindo(&self) -> HashSet<PersonId> {
        self.nomeacoes.values().copied().collect()
    }

    /// O servidor apontou `empresta` para servir `screen` a `assiste`.
    ///
    /// Chamado por quem despacha `SirvaTelaPara`/`AssistaTelaPor`, depois de
    /// [`Self::escolher`] decidir. Substitui a nomeação anterior **deste
    /// espectador para esta transmissão**, se havia uma: só a mais recente
    /// importa para resolver um `ParFalhou` dele. A nomeação de outro
    /// espectador da mesma transmissão fica onde estava — ver o doc de
    /// [`Self::nomeacoes`].
    pub fn apontou(&mut self, screen: ScreenId, empresta: PersonId, assiste: PersonId) {
        self.nomeacoes.insert((screen, assiste), empresta);
    }

    /// **Este espectador** deixou de ter par apontado para esta transmissão.
    ///
    /// **Chamada em todo caminho que encerra o repasse dele**, e não só no
    /// `ParFalhou`: quem assiste faz `UnwatchScreen`, ou relata falha.
    /// Enquanto só o relato de falha a chamava, um repasse que terminasse
    /// **bem** deixava a nomeação de pé, e [`Self::ja_servindo`] contava
    /// aquele par como ocupado pelo resto da sessão do daemon — com o cliente
    /// dele já tendo devolvido a vaga. A malha degradava para a estrela, um
    /// par por transmissão encerrada, sem um único rastro dizendo por quê.
    ///
    /// **`assiste` não é opcional, e é o conserto de um segundo defeito.**
    /// Quando a chave era só a transmissão, esta função soltava a vaga de
    /// *todos* os espectadores dela: um deles fechando a janela devolvia à
    /// fila um par que continuava repassando para o outro, e aquele par podia
    /// então ser apontado uma segunda vez. Para o fim da transmissão inteira —
    /// quem compartilha para, a sala é apagada — a função é
    /// [`Self::a_transmissao_acabou`].
    ///
    /// **Diferente de [`Self::desacreditar`], e a diferença é quem paga.** Ali
    /// a declaração inteira de uma pessoa é apagada, porque ela foi provada
    /// falsa; aqui só a nomeação some, e quem emprestava continua declarado e
    /// elegível. É o que um `ParFalhou` de rotina — o par caiu, o par parou de
    /// mandar — merece: a transmissão volta ao servidor, e ninguém é punido
    /// por a rede de alguém ter oscilado.
    pub fn desapontou(&mut self, screen: ScreenId, assiste: PersonId) {
        self.nomeacoes.remove(&(screen, assiste));
    }

    /// A transmissão inteira acabou: **nenhum** espectador dela tem mais par
    /// apontado.
    ///
    /// É o caminho de quem compartilha parando (`StopScreenShare`), da sala
    /// sendo apagada e da sessão de quem compartilha acabando — ali não sobra
    /// repasse para espectador nenhum, e cada par apontado tem de voltar à
    /// fila. [`Self::desapontou`] é a outra metade, para um espectador só.
    pub fn a_transmissao_acabou(&mut self, screen: ScreenId) {
        self.nomeacoes.retain(|(tela, _), _| *tela != screen);
    }

    /// Quem foi apontado para servir esta transmissão **a esta pessoa**, se
    /// alguém.
    ///
    /// É contra isto que um `ClientMessage::ParFalhou { screen }` se resolve:
    /// a mensagem só carrega a transmissão, nunca a identidade de quem
    /// falhou, porque quem relata é quem estava esperando a imagem — a
    /// vítima, não quem investiga. `assiste` é justamente quem relatou, que o
    /// servidor conhece pela sessão de onde a mensagem veio — e é o que torna
    /// a resposta exata quando a mesma tela é repassada a mais de uma pessoa
    /// por pares diferentes.
    #[must_use]
    pub fn quem_foi_apontado(&self, screen: ScreenId, assiste: PersonId) -> Option<PersonId> {
        self.nomeacoes.get(&(screen, assiste)).copied()
    }
}

/// Dado o motivo de um `ClientMessage::ParFalhou` e quem o servidor tinha
/// apontado para a transmissão, quem (se alguém) desacreditar.
///
/// **Função pura, extraída no fix round 3.** É onde o defeito do round 2
/// morava — `session.rs` chamava `saiu(session.person)`, que é sempre quem
/// **relata**, nunca quem falhou — e o round 2 corrigiu isso em `session.rs`
/// sem deixar um teste que exercitasse a decisão em si: o revisor reintroduziu
/// o defeito e os 361 testes do `seele-server` passaram porque o único teste
/// que citava o achado exercitava `apontou`/`quem_foi_apontado` isolados, não
/// este raciocínio. Extraída para cá, a decisão é testável sem sessão, sem
/// conexão e sem `Pares` nenhum.
///
/// Só [`MotivoDeFalhaDePar::ImpressaoNaoBate`] desacredita alguém — é o único
/// motivo que prova algo sobre a **declaração** (alguém respondeu no lugar de
/// quem foi apontado). `NaoFuiAceito` é o inverso — a suspeita cai sobre quem
/// relata, não sobre o par apontado — e os outros dois são rotina de rede.
/// `apontado` vem de [`Pares::quem_foi_apontado`]; `None` quando o servidor
/// não tem nomeação guardada para a transmissão (hoje, sempre — ver o doc do
/// módulo).
#[must_use]
pub fn quem_desacreditar(
    motivo: MotivoDeFalhaDePar,
    apontado: Option<PersonId>,
) -> Option<PersonId> {
    match motivo {
        MotivoDeFalhaDePar::ImpressaoNaoBate => apontado,
        MotivoDeFalhaDePar::NaoAlcancou
        | MotivoDeFalhaDePar::CaiuNoMeio
        | MotivoDeFalhaDePar::ParouDeMandar
        | MotivoDeFalhaDePar::NaoFuiAceito => None,
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

    /// A sala de voz em que todo mundo deste módulo de teste está sentado.
    ///
    /// Os testes que não falam de sala nenhuma passam esta: eles afirmam
    /// outras regras de `escolher`, e uma sala vazia as tornaria vácuas —
    /// passariam por não haver ninguém na sala, não pela regra sob teste.
    fn toda_a_sala() -> HashSet<PersonId> {
        (1..=9).map(PersonId).collect()
    }

    #[test]
    fn quem_compartilha_nunca_e_escolhido_para_servir_a_si_mesmo() {
        // O espelho infinito, na versão da malha: quem compartilha servindo a
        // própria tela a si mesmo. `crate::voice_room` já prende isto para o
        // caminho do servidor; aqui é a mesma regra no caminho novo.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(1),
            1,
            true,
            "a".repeat(64),
            vec![endereco(1)],
            endereco(1),
        );
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new(), &toda_a_sala())
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
            1,
            true,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.declarou(
            PersonId(3),
            1,
            false,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new(), &toda_a_sala())
            .is_none());
    }

    #[test]
    fn quem_empresta_de_outra_sala_de_voz_nunca_e_escolhido() {
        // **Vazamento de conteúdo entre salas.** `Pares` é global ao daemon:
        // a pessoa 4 declarou que empresta enquanto estava numa sala, e a
        // transmissão sob escolha está em outra. Se ela for apontada, o
        // cliente dela repassa o que **ela** recebe — a tela da sala dela — e
        // quem assiste na sala da transmissão vê conteúdo de uma sala em que
        // nunca entrou. A declaração é global; a escolha não pode ser.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(4),
            1,
            true,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        // Quem compartilha, quem quer assistir — e mais ninguém. A pessoa 4
        // está noutro lugar.
        let na_sala = HashSet::from([PersonId(1), PersonId(2)]);
        assert!(
            pares
                .escolher(PersonId(1), PersonId(2), &HashSet::new(), &na_sala)
                .is_none(),
            "alguém de outra sala de voz foi apontado para servir esta tela — \
             o repasse dele carrega a tela da sala dele"
        );
    }

    #[test]
    fn entre_dois_que_emprestam_so_o_da_sala_da_tela_e_escolhido() {
        // A outra metade do guarda: com um candidato de cada lado, a escolha
        // não pode ser «o primeiro que o `HashMap` devolver». Sem o filtro,
        // este teste passaria metade das vezes — e um teste que passa metade
        // das vezes é o pior guarda que existe.
        let na_sala = HashSet::from([PersonId(1), PersonId(2), PersonId(4)]);
        // **Um `Pares` novo a cada rodada, e não um reusado.** A ordem de
        // iteração de um `HashMap` é fixa enquanto o mapa vive; ela só muda
        // com a semente, que é sorteada uma vez por mapa. Um laço sobre o
        // mesmo mapa repetiria cinquenta vezes a mesma ordem — e um guarda
        // que depende de a ordem ter caído do lado errado é um guarda que
        // passa metade das vezes.
        for _ in 0..50 {
            let mut pares = Pares::nova();
            for pessoa in [3_u8, 4] {
                pares.declarou(
                    PersonId(u64::from(pessoa)),
                    1,
                    true,
                    "c".repeat(64),
                    vec![endereco(pessoa)],
                    endereco(pessoa),
                );
            }
            let escolhido = pares
                .escolher(PersonId(1), PersonId(2), &HashSet::new(), &na_sala)
                .expect("havia um par elegível na sala da transmissão");
            assert_eq!(
                escolhido.pessoa,
                PersonId(4),
                "a escolha saiu da sala da transmissão"
            );
        }
    }

    #[test]
    fn quem_ja_esta_servindo_nao_e_escolhido_de_novo() {
        // **Um par por vez, no A1.** Quantos um cliente aguenta é a conta do
        // subprojeto B, e supor «dois» aqui seria inventar um número que
        // ninguém mediu.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            1,
            true,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        let ja = HashSet::from([PersonId(3)]);
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &ja, &toda_a_sala())
            .is_none());
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
            1,
            true,
            "c".repeat(64),
            vec![endereco(3)],
            publico,
        );
        let escolhido = pares
            .escolher(PersonId(1), PersonId(2), &HashSet::new(), &toda_a_sala())
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
            1,
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
            1,
            false,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.saiu(PersonId(3), 1);
        assert!(
            pares.declaracao_de(PersonId(3)).is_none(),
            "a identidade sobreviveu à saída da sessão"
        );
    }

    #[test]
    fn a_saida_de_uma_conexao_velha_nao_apaga_a_declaracao_de_uma_nova() {
        // **Achado do fix round 3.** Uma corrida de candidatos pode deixar
        // duas conexões vivas com o mesmo `PersonId`. A candidata 1 declara,
        // perde a corrida e sua conexão é fechada; a candidata 2 (a
        // vencedora) já declarou de novo antes de o encerramento da 1
        // terminar de rodar. `saiu` chamado com o `id_da_conexao` da
        // candidata 1 não pode apagar a declaração que a 2 acabou de fazer.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            1, // candidata 1, perdedora
            true,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.declarou(
            PersonId(3),
            2, // candidata 2, vencedora — substitui a declaração acima
            true,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.saiu(PersonId(3), 1); // o encerramento tardio da candidata 1
        assert!(
            pares.declaracao_de(PersonId(3)).is_some(),
            "o encerramento de uma conexão que perdeu a corrida apagou a \
             declaração da que venceu"
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
        pares.apontou(tela, PersonId(5), PersonId(2));
        assert_eq!(
            pares.quem_foi_apontado(tela, PersonId(2)),
            Some(PersonId(5))
        );
    }

    #[test]
    fn quem_sai_deixa_de_ser_a_resposta_de_uma_nomeacao_velha() {
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(5),
            1,
            true,
            "e".repeat(64),
            vec![endereco(5)],
            endereco(5),
        );
        let tela = ScreenId(9);
        pares.apontou(tela, PersonId(5), PersonId(2));
        pares.saiu(PersonId(5), 1);
        assert_eq!(
            pares.quem_foi_apontado(tela, PersonId(2)),
            None,
            "quem já foi embora continuou sendo a resposta de uma nomeação"
        );
    }

    #[test]
    fn quem_assiste_indo_embora_devolve_o_par_a_quem_pode_escolher() {
        // **Cada repasse encerrado queimava um par para sempre.** A nomeação
        // só era apagada por `ParFalhou`; quem assiste saindo da sala — ou a
        // sessão dela acabando — deixava a nomeação de pé, e `ja_servindo`
        // contava aquele par como ocupado pelo resto da sessão do daemon,
        // enquanto o cliente dele já tinha devolvido a vaga.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(4),
            1,
            true,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
        assert!(
            pares.ja_servindo().contains(&PersonId(4)),
            "a nomeação não pôs o par em ja_servindo"
        );

        pares.quem_assiste_saiu(PersonId(2));
        assert!(
            pares.ja_servindo().is_empty(),
            "quem assiste foi embora e o par apontado continuou contado como ocupado"
        );
        assert!(
            pares
                .escolher(
                    PersonId(1),
                    PersonId(3),
                    &pares.ja_servindo(),
                    &toda_a_sala()
                )
                .is_some(),
            "o par não voltou a ser escolhível depois de o repasse acabar"
        );
    }

    #[test]
    fn a_saida_de_quem_assiste_nao_derruba_a_nomeacao_de_outra_pessoa() {
        // A outra metade: um `retain` escrito ao contrário passaria no teste
        // acima e desligaria toda nomeação viva a cada saída de sala.
        let mut pares = Pares::nova();
        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
        pares.quem_assiste_saiu(PersonId(7));
        assert_eq!(
            pares.quem_foi_apontado(ScreenId(9), PersonId(2)),
            Some(PersonId(4)),
            "a saída de quem não assistia esta tela derrubou a nomeação dela"
        );
    }

    #[test]
    fn a_sessao_que_acaba_devolve_o_par_que_servia_esta_pessoa() {
        // `saiu` confere a conexão para não apagar a **declaração** de uma
        // conexão que venceu a corrida de candidatos. A nomeação feita para
        // esta pessoa não é declaração de ninguém: ela cai de qualquer jeito.
        let mut pares = Pares::nova();
        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
        pares.saiu(PersonId(2), 77);
        assert!(
            pares.ja_servindo().is_empty(),
            "a sessão de quem assiste acabou e o par apontado continuou ocupado"
        );
    }

    #[test]
    fn dois_espectadores_da_mesma_tela_ocupam_dois_pares() {
        // **Achado da revisão de 10/09.** A nomeação era guardada por
        // `ScreenId` sozinho, e uma tela tem um dono e vários espectadores: o
        // segundo `WatchScreen` sobrescrevia a nomeação do primeiro. O par que
        // servia o primeiro sumia de `ja_servindo` enquanto ainda repassava, e
        // `escolher` voltava a oferecê-lo — dois repasses na mesma subida, que
        // é exatamente o que `ja_servindo` existe para impedir.
        let mut pares = Pares::nova();
        for (pessoa, letra, n) in [(PersonId(4), "d", 4), (PersonId(5), "e", 5)] {
            pares.declarou(
                pessoa,
                1,
                true,
                letra.repeat(64),
                vec![endereco(n)],
                endereco(n),
            );
        }
        let tela = ScreenId(9);
        pares.apontou(tela, PersonId(4), PersonId(2));
        pares.apontou(tela, PersonId(5), PersonId(3));

        let ocupados = pares.ja_servindo();
        assert!(
            ocupados.contains(&PersonId(4)) && ocupados.contains(&PersonId(5)),
            "os dois pares repassam a mesma tela e nem todos foram contados como \
             ocupados: {ocupados:?}"
        );
        assert_eq!(
            pares.quem_foi_apontado(tela, PersonId(2)),
            Some(PersonId(4)),
            "a nomeação do primeiro espectador foi sobrescrita pela do segundo"
        );
        assert_eq!(
            pares.quem_foi_apontado(tela, PersonId(3)),
            Some(PersonId(5)),
            "a nomeação do segundo espectador não foi guardada"
        );
    }

    #[test]
    fn quem_fecha_a_janela_nao_solta_o_par_de_quem_continua_assistindo() {
        // A outra metade do mesmo achado: com a chave sendo só a tela,
        // `desapontou` soltava a vaga dos **dois** espectadores. O par do
        // segundo voltava à fila enquanto ainda repassava, e podia ser
        // apontado uma segunda vez.
        let mut pares = Pares::nova();
        let tela = ScreenId(9);
        pares.apontou(tela, PersonId(4), PersonId(2));
        pares.apontou(tela, PersonId(5), PersonId(3));

        pares.desapontou(tela, PersonId(2));

        assert_eq!(
            pares.quem_foi_apontado(tela, PersonId(2)),
            None,
            "quem fechou a janela continuou com par apontado"
        );
        assert_eq!(
            pares.quem_foi_apontado(tela, PersonId(3)),
            Some(PersonId(5)),
            "a saída de um espectador derrubou a nomeação do outro"
        );
        assert_eq!(
            pares.ja_servindo(),
            HashSet::from([PersonId(5)]),
            "a vaga de quem continua repassando não ficou ocupada"
        );
    }

    #[test]
    fn a_transmissao_acabando_solta_o_par_de_todo_espectador() {
        // Quem compartilha parar, a sala ser apagada e a sessão de quem
        // compartilha acabar não encerram o repasse de uma pessoa: encerram o
        // de todas. É a metade que `desapontou` deixou de fazer ao ganhar
        // `assiste`, e sem ela cada transmissão encerrada queimaria os pares
        // dos espectadores que não avisaram nada.
        let mut pares = Pares::nova();
        let tela = ScreenId(9);
        let outra = ScreenId(10);
        pares.apontou(tela, PersonId(4), PersonId(2));
        pares.apontou(tela, PersonId(5), PersonId(3));
        pares.apontou(outra, PersonId(6), PersonId(2));

        pares.a_transmissao_acabou(tela);

        assert_eq!(
            pares.ja_servindo(),
            HashSet::from([PersonId(6)]),
            "a transmissão acabou e algum par dela continuou contado como ocupado \
             — ou o par de outra transmissão foi solto junto"
        );
        assert_eq!(
            pares.quem_foi_apontado(outra, PersonId(2)),
            Some(PersonId(6)),
            "o fim de uma transmissão derrubou a nomeação de outra"
        );
    }

    #[test]
    fn desacreditar_apaga_independente_da_conexao() {
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            1,
            true,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.desacreditar(PersonId(3));
        assert!(
            pares.declaracao_de(PersonId(3)).is_none(),
            "desacreditar não apagou a declaração"
        );
    }

    #[test]
    fn impressaonaobate_desacredita_quem_foi_apontado_nunca_quem_relata() {
        // **O guarda que faltava no round 2.** `quem_desacreditar` é a
        // função pura onde o defeito do round 2 morava — o teste de lá só
        // exercitava `apontou`/`quem_foi_apontado`, nunca esta decisão.
        assert_eq!(
            quem_desacreditar(MotivoDeFalhaDePar::ImpressaoNaoBate, Some(PersonId(9))),
            Some(PersonId(9))
        );
    }

    #[test]
    fn sem_nomeacao_guardada_ninguem_e_desacreditado() {
        assert_eq!(
            quem_desacreditar(MotivoDeFalhaDePar::ImpressaoNaoBate, None),
            None
        );
    }

    #[test]
    fn motivos_que_nao_sao_impressaonaobate_nunca_desacreditam_ninguem() {
        for motivo in [
            MotivoDeFalhaDePar::NaoAlcancou,
            MotivoDeFalhaDePar::CaiuNoMeio,
            MotivoDeFalhaDePar::ParouDeMandar,
            MotivoDeFalhaDePar::NaoFuiAceito,
        ] {
            assert_eq!(
                quem_desacreditar(motivo, Some(PersonId(9))),
                None,
                "{motivo:?} desacreditou alguém, e só ImpressaoNaoBate deveria"
            );
        }
    }
}
