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

use seele_proto::control::{ConsentimentoDePar, MotivoDeFalhaDePar};
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
    /// O que esta pessoa consentiu no caminho entre pares.
    ///
    /// Ter identidade aqui e não consentir em nada é o caso comum de quem só
    /// conectou — ver o doc do módulo. As duas metades do consentimento são
    /// lidas em lugares diferentes e por razões diferentes:
    /// [`Pares::escolher`] olha `empresta_conexao`, e
    /// [`Pares::quem_consentiu_assistir_por_par`] olha `assiste_por_par`.
    pub consentimento: ConsentimentoDePar,
}

/// Um repasse que deixou de ser consentido e por isso tem de acabar.
///
/// Devolvido por [`Pares::declarou`] porque declarar e desfazer o que a
/// declaração nova revoga são **o mesmo ato**: separá-los daria a quem chama a
/// oportunidade de fazer só metade, e a metade que sobraria é a que deixa
/// alguém pagando por um consentimento que retirou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepasseAEncerrar {
    /// Qual transmissão.
    pub screen: ScreenId,
    /// Quem estava recebendo por par, e volta a ser servido pelo servidor.
    pub assiste: PersonId,
    /// Quem estava repassando.
    pub empresta: PersonId,
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
        consentimento: ConsentimentoDePar,
        impressao: String,
        locais: Vec<SocketAddr>,
        publico: SocketAddr,
    ) -> Vec<RepasseAEncerrar> {
        let encerrar = self.o_que_o_consentimento_novo_revoga(pessoa, consentimento);
        for repasse in &encerrar {
            self.nomeacoes.remove(&(repasse.screen, repasse.assiste));
        }
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
                consentimento,
            },
        );
        encerrar
    }

    /// Quais repasses vivos um consentimento novo desta pessoa revoga.
    ///
    /// Duas perguntas, e as duas são de consentimento retirado:
    ///
    /// - **como quem empresta** — quantos pares ela ainda aceita atender. O que
    ///   passa do teto novo tem de acabar, e o caso `0` (deixou de emprestar)
    ///   não é especial: é o teto valendo.
    /// - **como quem assiste** — se ainda aceita ter o próprio endereço
    ///   entregue a quem a serve. Retirado, o repasse que existe por causa
    ///   daquele endereço acaba, e a tela volta a vir do servidor.
    ///
    /// **A ordem é fixada de propósito.** `nomeacoes` é um `HashMap`, e
    /// escolher «os que passam do teto» pela ordem de iteração dele daria um
    /// resultado diferente a cada execução — e um teste que passa metade das
    /// vezes. Ordenar por `(tela, quem assiste)` não é uma política de quem
    /// fica (essa é do subprojeto B); é só a promessa de que a mesma entrada
    /// dá a mesma saída.
    fn o_que_o_consentimento_novo_revoga(
        &self,
        pessoa: PersonId,
        novo: ConsentimentoDePar,
    ) -> Vec<RepasseAEncerrar> {
        let mut encerrar = Vec::new();

        let mut atendidos: Vec<(ScreenId, PersonId)> = self
            .nomeacoes
            .iter()
            .filter(|(_, empresta)| **empresta == pessoa)
            .map(|((tela, assiste), _)| (*tela, *assiste))
            .collect();
        atendidos.sort_unstable();
        for (screen, assiste) in atendidos
            .into_iter()
            .skip(usize::from(novo.pares_que_atende))
        {
            encerrar.push(RepasseAEncerrar {
                screen,
                assiste,
                empresta: pessoa,
            });
        }

        if !novo.assiste_por_par {
            let mut recebidos: Vec<(ScreenId, PersonId)> = self
                .nomeacoes
                .iter()
                .filter(|((_, assiste), _)| *assiste == pessoa)
                .map(|((tela, _), empresta)| (*tela, *empresta))
                .collect();
            recebidos.sort_unstable();
            for (screen, empresta) in recebidos {
                let repasse = RepasseAEncerrar {
                    screen,
                    assiste: pessoa,
                    empresta,
                };
                // Quem empresta a si mesmo não existe (`escolher` recusa), mas
                // uma nomeação vinda de outro caminho não pode entrar duas
                // vezes na lista: quem chama desfaz cada item, e desfazer duas
                // vezes o mesmo repasse mandaria dois `TelaAssistir` para a
                // mesma pessoa.
                if !encerrar.contains(&repasse) {
                    encerrar.push(repasse);
                }
            }
        }

        encerrar
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

    /// Esta pessoa deixou de poder repassar: saiu da sala, ou a transmissão
    /// que ela recebia acabou para ela.
    ///
    /// **A terceira direção da saída.** [`Self::a_transmissao_acabou`] desfaz
    /// pelo lado de quem compartilha e [`Self::quem_assiste_saiu`] pelo lado de
    /// quem assiste; esta desfaz pelo lado de **quem empresta**. Um par repassa
    /// o que ele mesmo recebe: saindo da sala, ele para de receber, e quem
    /// estava atrás dele fica sem imagem.
    ///
    /// Devolve o que foi desfeito porque quem chama tem de reabrir o cano de
    /// cada espectador órfão — esperar o relato de quem ficou no escuro é
    /// esperar o prazo do par vencer do outro lado.
    ///
    /// Diferente de [`Self::saiu`]: ali a **sessão** acabou e a declaração
    /// inteira vai junto; aqui a pessoa continua conectada e continua
    /// declarada, e só as nomeações caem.
    pub fn quem_empresta_parou(&mut self, pessoa: PersonId) -> Vec<RepasseAEncerrar> {
        let mut encerrar: Vec<RepasseAEncerrar> = self
            .nomeacoes
            .iter()
            .filter(|(_, empresta)| **empresta == pessoa)
            .map(|((screen, assiste), empresta)| RepasseAEncerrar {
                screen: *screen,
                assiste: *assiste,
                empresta: *empresta,
            })
            .collect();
        encerrar.sort_unstable_by_key(|repasse| (repasse.screen, repasse.assiste));
        self.nomeacoes.retain(|_, empresta| *empresta != pessoa);
        encerrar
    }

    /// A declaração de quem consentiu em **assistir por par** — a única que
    /// pode ser entregue a quem vai servi-la.
    ///
    /// `None` quando a pessoa não declarou, já saiu, ou **não consentiu**.
    #[must_use]
    pub fn quem_consentiu_assistir_por_par(&self, pessoa: PersonId) -> Option<&QuemDeclarou> {
        self.quem
            .get(&pessoa)
            .filter(|declarado| declarado.consentimento.assiste_por_par)
    }

    /// Quantos pares esta pessoa está atendendo agora, pelas nomeações deste
    /// servidor.
    #[must_use]
    pub fn quantos_atende(&self, pessoa: PersonId) -> usize {
        self.nomeacoes
            .values()
            .filter(|empresta| **empresta == pessoa)
            .count()
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
        na_sala: &HashSet<PersonId>,
        recebendo: &HashSet<PersonId>,
    ) -> Option<QuemDeclarou> {
        self.quem
            .values()
            .find(|candidato| {
                candidato.consentimento.empresta_conexao()
                    && candidato.pessoa != dono
                    && candidato.pessoa != quem_quer
                    && self.quantos_atende(candidato.pessoa)
                        < usize::from(candidato.consentimento.pares_que_atende)
                    && na_sala.contains(&candidato.pessoa)
                    && recebendo.contains(&candidato.pessoa)
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

    /// Alguém que consentiu nas duas metades, com o teto de um par.
    const CONSENTE_TUDO: ConsentimentoDePar = ConsentimentoDePar {
        pares_que_atende: 1,
        assiste_por_par: true,
    };

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

    /// Quem só conectou: identidade declarada, consentimento nenhum.
    const NAO_CONSENTE: ConsentimentoDePar = ConsentimentoDePar::de_ninguem();

    #[test]
    fn o_endereco_de_quem_nao_consentiu_assistir_por_par_nao_e_entregue_a_ninguem() {
        // **A metade do §5 que a primeira redação não implementou.** O opt-in
        // existia só para quem empresta; quem assistia tinha o próprio
        // endereço posto em `SirvaTelaPara` e entregue ao par que fosse
        // servi-lo, por uma decisão que **outra pessoa** tomou — a de
        // emprestar. O §5 diz que privacidade é a primeira das duas razões, e
        // ela é dos dois lados: numa malha «espectadores passam a conhecer o
        // endereço IP uns dos outros».
        //
        // A declaração continua guardada (a parede simétrica da Task 5 precisa
        // da impressão dos dois lados); o que o consentimento destranca é a
        // **entrega do endereço**, e é isso que esta função separa de
        // `declaracao_de`.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(2),
            1,
            NAO_CONSENTE,
            "b".repeat(64),
            Vec::new(),
            endereco(2),
        );
        assert!(
            pares.declaracao_de(PersonId(2)).is_some(),
            "a identidade de quem só conecta tem de sobreviver: a parede simétrica precisa dela"
        );
        assert!(
            pares.quem_consentiu_assistir_por_par(PersonId(2)).is_none(),
            "o endereço de quem nunca consentiu em assistir por par foi liberado para entrega"
        );
    }

    #[test]
    fn quem_consentiu_assistir_por_par_tem_o_endereco_entregue() {
        // A outra metade do guarda: um portão que recusasse todo mundo
        // passaria no teste acima e desligaria a malha inteira sem um erro.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(2),
            1,
            CONSENTE_TUDO,
            "b".repeat(64),
            vec![endereco(2)],
            endereco(2),
        );
        assert!(pares.quem_consentiu_assistir_por_par(PersonId(2)).is_some());
    }

    #[test]
    fn um_par_que_nao_recebe_a_transmissao_nao_e_escolhido_para_repassa_la() {
        // **Um par repassa o que ele mesmo está recebendo, e nada além.** Estar
        // na sala não é estar assistindo: quem entrou e não abriu a
        // transmissão não tem quadro nenhum para repassar, e o cliente dele
        // recusa o pedido em silêncio (`RepasseDeTela::abertura_de` devolve
        // `None`).
        //
        // O custo de escolhê-lo assim mesmo não é zero, e é esse o defeito:
        // quem pediu para assistir fica com o cano do servidor **desligado**,
        // espera a ligação fechar, espera o prazo do par vencer e só então
        // manda `ParFalhou` — segundos de tela parada por uma escolha que o
        // servidor tinha como não fazer.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(4),
            1,
            CONSENTE_TUDO,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        let na_sala = HashSet::from([PersonId(1), PersonId(2), PersonId(4)]);
        // Quem compartilha manda; quem assiste pediu agora. A pessoa 4 está na
        // sala e não abriu a transmissão.
        let recebendo = HashSet::new();
        assert!(
            pares
                .escolher(PersonId(1), PersonId(2), &na_sala, &recebendo)
                .is_none(),
            "um par que não recebe esta transmissão foi apontado para repassá-la"
        );
    }

    #[test]
    fn entre_dois_da_sala_so_o_que_recebe_a_transmissao_e_escolhido() {
        // A outra metade, e pela mesma razão do irmão da sala de voz: com um
        // candidato de cada lado, um guarda ausente passaria metade das vezes.
        let na_sala = HashSet::from([PersonId(1), PersonId(2), PersonId(3), PersonId(4)]);
        let recebendo = HashSet::from([PersonId(4)]);
        for _ in 0..50 {
            let mut pares = Pares::nova();
            for pessoa in [3_u8, 4] {
                pares.declarou(
                    PersonId(u64::from(pessoa)),
                    1,
                    CONSENTE_TUDO,
                    "c".repeat(64),
                    vec![endereco(pessoa)],
                    endereco(pessoa),
                );
            }
            let escolhido = pares
                .escolher(PersonId(1), PersonId(2), &na_sala, &recebendo)
                .expect("havia um par recebendo a transmissão");
            assert_eq!(
                escolhido.pessoa,
                PersonId(4),
                "a escolha saiu de quem não está recebendo esta transmissão"
            );
        }
    }

    #[test]
    fn o_teto_de_quem_empresta_e_dele_e_nao_do_codigo() {
        // **Quantos pares um cliente atende é de quem paga a conta.** Com teto
        // 2, a segunda escolha tem de sair; com teto 1, não. Um `== 0` fixo
        // passaria na metade de baixo deste teste e reprovaria na de cima, e
        // é essa a diferença entre um teto declarado e um teto que só o
        // código conhece.
        let dois = ConsentimentoDePar {
            pares_que_atende: 2,
            assiste_por_par: false,
        };
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(4),
            1,
            dois,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        let tela = ScreenId(9);
        pares.apontou(tela, PersonId(4), PersonId(2));
        let escolhido = pares
            .escolher(PersonId(1), PersonId(3), &toda_a_sala(), &toda_a_sala())
            .expect("quem aceitou atender dois pares foi recusado no segundo");
        assert_eq!(escolhido.pessoa, PersonId(4));

        pares.apontou(tela, PersonId(4), PersonId(3));
        assert!(
            pares
                .escolher(PersonId(1), PersonId(5), &toda_a_sala(), &toda_a_sala())
                .is_none(),
            "o teto declarado por quem empresta foi estourado"
        );
    }

    #[test]
    fn baixar_o_teto_encerra_o_repasse_que_deixou_de_caber() {
        // **A mudança de capacidade tem de alcançar o que já está no ar.**
        // Enquanto a declaração nova só valesse para a escolha seguinte,
        // baixar o teto — ou o deslizante da casca ir a zero — não desligaria
        // nada: a pessoa continuaria subindo a cópia que acabou de dizer que
        // não quer mais subir, e o único aviso seria a conta de internet dela.
        let dois = ConsentimentoDePar {
            pares_que_atende: 2,
            assiste_por_par: false,
        };
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(4),
            1,
            dois,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        let tela = ScreenId(9);
        pares.apontou(tela, PersonId(4), PersonId(2));
        pares.apontou(tela, PersonId(4), PersonId(3));

        let um = ConsentimentoDePar {
            pares_que_atende: 1,
            assiste_por_par: false,
        };
        let encerrar = pares.declarou(
            PersonId(4),
            1,
            um,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        assert_eq!(
            encerrar.len(),
            1,
            "baixar o teto de dois para um não encerrou exatamente um repasse: {encerrar:?}"
        );
        assert_eq!(encerrar[0].empresta, PersonId(4));
        assert_eq!(pares.quantos_atende(PersonId(4)), 1);
    }

    #[test]
    fn deixar_de_emprestar_encerra_todos_os_repasses_em_curso() {
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(4),
            1,
            CONSENTE_TUDO,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
        let encerrar = pares.declarou(
            PersonId(4),
            1,
            NAO_CONSENTE,
            "d".repeat(64),
            Vec::new(),
            endereco(4),
        );
        assert_eq!(
            encerrar,
            vec![RepasseAEncerrar {
                screen: ScreenId(9),
                assiste: PersonId(2),
                empresta: PersonId(4),
            }],
            "quem retirou o consentimento de emprestar continuou repassando"
        );
        assert_eq!(pares.quantos_atende(PersonId(4)), 0);
    }

    #[test]
    fn retirar_o_consentimento_de_assistir_por_par_encerra_o_proprio_repasse() {
        // O outro lado da retirada, e ele é o do §5 que faltava: quem assiste
        // muda de ideia sobre ter o endereço entregue, e o repasse que existe
        // por causa daquele endereço tem de acabar — voltando ao servidor, que
        // é o caminho de sempre.
        let mut pares = Pares::nova();
        for pessoa in [2_u8, 4] {
            pares.declarou(
                PersonId(u64::from(pessoa)),
                1,
                CONSENTE_TUDO,
                "d".repeat(64),
                vec![endereco(pessoa)],
                endereco(pessoa),
            );
        }
        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
        let so_empresta = ConsentimentoDePar {
            pares_que_atende: 1,
            assiste_por_par: false,
        };
        let encerrar = pares.declarou(
            PersonId(2),
            1,
            so_empresta,
            "d".repeat(64),
            vec![endereco(2)],
            endereco(2),
        );
        assert_eq!(
            encerrar,
            vec![RepasseAEncerrar {
                screen: ScreenId(9),
                assiste: PersonId(2),
                empresta: PersonId(4),
            }],
            "quem retirou o consentimento continuou recebendo por par"
        );
        assert_eq!(pares.quantos_atende(PersonId(4)), 0);
    }

    #[test]
    fn redeclarar_o_mesmo_consentimento_nao_encerra_nada() {
        // O guarda contra o oposto: uma reconciliação escrita larga demais
        // derrubaria todo repasse a cada redeclaração — e a declaração é
        // repetida a **cada reconexão** (`Motor::declarar_identidade_de_par`),
        // então a malha morreria na primeira bateria interna.
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(4),
            1,
            CONSENTE_TUDO,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
        let encerrar = pares.declarou(
            PersonId(4),
            1,
            CONSENTE_TUDO,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        assert!(
            encerrar.is_empty(),
            "redeclarar o mesmo consentimento encerrou um repasse: {encerrar:?}"
        );
        assert_eq!(pares.quantos_atende(PersonId(4)), 1);
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
            CONSENTE_TUDO,
            "a".repeat(64),
            vec![endereco(1)],
            endereco(1),
        );
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &toda_a_sala(), &toda_a_sala())
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
            CONSENTE_TUDO,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.declarou(
            PersonId(3),
            1,
            ConsentimentoDePar::de_ninguem(),
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &toda_a_sala(), &toda_a_sala())
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
            CONSENTE_TUDO,
            "d".repeat(64),
            vec![endereco(4)],
            endereco(4),
        );
        // Quem compartilha, quem quer assistir — e mais ninguém. A pessoa 4
        // está noutro lugar.
        let na_sala = HashSet::from([PersonId(1), PersonId(2)]);
        assert!(
            pares
                .escolher(PersonId(1), PersonId(2), &na_sala, &toda_a_sala())
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
                    CONSENTE_TUDO,
                    "c".repeat(64),
                    vec![endereco(pessoa)],
                    endereco(pessoa),
                );
            }
            let escolhido = pares
                .escolher(PersonId(1), PersonId(2), &na_sala, &toda_a_sala())
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
            CONSENTE_TUDO,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        // A vaga é tomada pela nomeação, que é o único registro de quem o
        // servidor já pôs para trabalhar.
        pares.apontou(ScreenId(9), PersonId(3), PersonId(8));
        assert!(pares
            .escolher(PersonId(1), PersonId(2), &toda_a_sala(), &toda_a_sala())
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
            CONSENTE_TUDO,
            "c".repeat(64),
            vec![endereco(3)],
            publico,
        );
        let escolhido = pares
            .escolher(PersonId(1), PersonId(2), &toda_a_sala(), &toda_a_sala())
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
            ConsentimentoDePar::de_ninguem(),
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        let declaracao = pares
            .declaracao_de(PersonId(3))
            .expect("a declaração de quem só assiste desapareceu");
        assert_eq!(declaracao.impressao, "c".repeat(64));
        assert!(declaracao.enderecos.contains(&endereco(3)));
        assert!(!declaracao.consentimento.empresta_conexao());
    }

    #[test]
    fn saiu_apaga_tambem_a_declaracao() {
        let mut pares = Pares::nova();
        pares.declarou(
            PersonId(3),
            1,
            ConsentimentoDePar::de_ninguem(),
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
            CONSENTE_TUDO,
            "c".repeat(64),
            vec![endereco(3)],
            endereco(3),
        );
        pares.declarou(
            PersonId(3),
            2, // candidata 2, vencedora — substitui a declaração acima
            CONSENTE_TUDO,
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
            CONSENTE_TUDO,
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
            CONSENTE_TUDO,
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
                .escolher(PersonId(1), PersonId(3), &toda_a_sala(), &toda_a_sala())
                .is_some(),
            "o par não voltou a ser escolhível depois de o repasse acabar"
        );
    }

    #[test]
    fn quem_empresta_saindo_da_sala_devolve_ao_servidor_quem_ele_servia() {
        // **A terceira direção da saída, e a que faltava.**
        // `soltar_telas_e_pares_de` já desfazia duas: as transmissões desta
        // pessoa acabaram, e o que ela assistia acabou para ela. A que faltava
        // é ela **emprestando**: saindo da sala, ela para de receber a
        // transmissão que repassava, e quem estava atrás dela fica sem imagem
        // até o prazo do par vencer do outro lado.
        //
        // Devolver a lista, e não só apagar, é o que permite reabrir o cano de
        // cada espectador órfão na hora — em vez de esperar o relato de quem
        // ficou no escuro.
        let mut pares = Pares::nova();
        pares.apontou(ScreenId(9), PersonId(4), PersonId(2));
        pares.apontou(ScreenId(9), PersonId(4), PersonId(3));
        pares.apontou(ScreenId(8), PersonId(5), PersonId(2));

        let mut encerrar = pares.quem_empresta_parou(PersonId(4));
        encerrar.sort_unstable_by_key(|repasse| repasse.assiste);
        assert_eq!(
            encerrar,
            vec![
                RepasseAEncerrar {
                    screen: ScreenId(9),
                    assiste: PersonId(2),
                    empresta: PersonId(4),
                },
                RepasseAEncerrar {
                    screen: ScreenId(9),
                    assiste: PersonId(3),
                    empresta: PersonId(4),
                },
            ]
        );
        assert_eq!(pares.quantos_atende(PersonId(4)), 0);
        assert_eq!(
            pares.quantos_atende(PersonId(5)),
            1,
            "a saída de um par derrubou a nomeação de outro"
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
                CONSENTE_TUDO,
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
            CONSENTE_TUDO,
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
