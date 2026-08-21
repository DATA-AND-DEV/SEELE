//! O gerente de uma chegada: dar nome a cada etapa de uma conexão.
//!
//! Só existe do lado de **quem entra**. Quem hospeda sobe a escada do ADR 0022
//! uma vez, antes de existir par nenhum, e o que ela produz não é uma conexão —
//! é uma frase e uma lista de endereços para o `seele://`. Ciclo de vida com
//! começo, tentativas e fim só existe deste lado, e é por isso que as duas
//! metades não compartilham máquina de estados nenhuma: a costura entre elas
//! continua sendo o link e o `SEELE-ENC/1`.
//!
//! # O que este módulo conserta
//!
//! Um teste de campo com duas casas falhou e ninguém soube dizer **em que
//! ponto**, porque não havia ponto nomeado: quatro candidatos eram tentados em
//! série atrás de um spinner mudo, e o que sobrava no fim era um erro só — o de
//! um dos quatro. [`Etapa`] dá nome a cada instante dessa travessia e
//! [`Chegada::trilha`] os guarda em ordem, com o relógio, para que a pergunta
//! «tentei quatro candidatos, qual deu o quê» tenha resposta.
//!
//! A trilha **não acrescenta conhecimento**: todo endereço dentro dela já
//! estava no convite de quem a lê, o que mantém o custo de privacidade em zero.
//!
//! # Três estados que não existem aqui
//!
//! O desenho de fora que originou este ciclo assume ICE, e três dos estados
//! dele não descrevem nada que aconteça neste lado:
//!
//! - `DISCOVERING` e `CANDIDATES_FOUND` — quem entra não descobre candidato
//!   nenhum. Eles chegam prontos no `seele://`, já ordenados e já truncados; a
//!   descoberta é do outro lado e aconteceu antes deste processo existir.
//! - `NAT_TRAVERSAL_FAILED` e `DISCOVERY_FAILED` — o que se observa é «todos os
//!   candidatos falharam». Atribuir isso ao furo é chute, e quem responde por
//!   quê é o diagnóstico do `plug --rede`, que mede em vez de supor.
//!
//! O quarto, `PATH_ESTABLISHED`, era inafirmável enquanto nada deste lado
//! aprendesse que o furo abriu. Deixou de ser: quem entra **pode ler** o
//! datagrama `FURO`, porque ele vem do anfitrião e não do ponto de encontro — a
//! invariante do ADR 0022 é sobre o ponto de encontro. Ele virou
//! [`Etapa::CaminhoAberto`], com o que está escrito lá.
//!
//! # Uso único
//!
//! Uma [`Chegada`] atravessa uma vez e morre: [`Chegada::chegar`] consome o
//! objeto. Uma reconexão constrói outra, e é assim que ela passa a ter a lista
//! inteira de candidatos em vez do endereço único com que
//! [`crate::enlace::Enlace`] reconecta hoje.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use seele_proto::uri::Bilhete;
use tokio::sync::watch;

use crate::client::ConnectError;
use crate::enlace::{Destino, Enlace};
use crate::tofu::PinStore;

/// O endereço de enfeite dos exemplares de [`Etapa::TODAS`].
///
/// `0.0.0.0:0` de propósito: quem ler um destes num log tem de reconhecer na
/// hora que está olhando para um exemplar de variante, e não para um candidato
/// que alguém tentou.
const EXEMPLO: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));

/// Onde uma chegada está.
///
/// Os nomes são **identificadores estáveis, nunca frases**, pela mesma regra de
/// `Degrau::nome()`: a frase que uma pessoa lê mora na casca — ADR 0012 e 0023
/// — e o Rust exporta o nome que a casca usa como chave. Renomear uma variante
/// daqui quebra a tela de todo mundo, e é para ser assim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Etapa {
    /// Ainda parada, com o convite lido e nada tentado.
    Parada {
        /// Quantos endereços o convite trouxe.
        candidatos: u8,
        /// O link trouxe `enc=` **e** o primeiro candidato tem impressão
        /// digital.
        ///
        /// É exatamente isso, e o nome diz as duas metades porque a bandeira
        /// não sabe mais do que elas. Em particular ela **não** promete que
        /// vai haver aviso: `crate::encontro::Batida::preparar` exige mais —
        /// impressão de pelo menos 16 caracteres, uma `Marca` que se deixe
        /// fazer com eles, um `bilhete.aviso()` que seja um endereço, o nome do
        /// ponto de encontro resolvido dentro do prazo, e um socket local que
        /// abra. Com o ponto de encontro fora do ar, ou com um nome que não
        /// resolve — os dois casos que a seção 1 do spec nomeia por escrito —
        /// esta bandeira continua verdadeira, [`Etapa::Avisando`] é publicada e
        /// nenhum datagrama sai.
        ///
        /// Isso não estraga nada, e é de propósito que fique escrito em vez de
        /// consertado aqui: nenhum endereço do convite depende do aviso, e a
        /// aresta que não existe (`Avisando → Desistiu`) é o que garante isso.
        /// Apertar a bandeira exigiria tentar `preparar` antes de publicar a
        /// etapa, o que é trabalho do passo seguinte da migração — quando o
        /// laço mudar de casa e esta camada passar a ver o envio.
        com_bilhete_e_impressao: bool,
    },
    /// Avisando o ponto de encontro de que estamos chegando.
    Avisando {
        /// O ponto de encontro, como o convite o escreveu: `host[:porta]`.
        ponto: String,
    },
    /// Um aperto de mão correndo contra um endereço do convite.
    Tentando {
        /// Qual da lista, contando do zero.
        candidato: u8,
        /// De quantos.
        de: u8,
        /// O endereço.
        onde: SocketAddr,
    },
    /// Um `FURO` com a marca certa chegou: o caminho até aqui abriu.
    ///
    /// **Marca não é autenticação**, e isso fica escrito no tipo em vez de num
    /// documento: os primeiros dígitos de uma impressão digital que já viaja no
    /// link não provam nada sobre quem mandou o pacote. Esta etapa não decide
    /// para onde conectar nem dispensa conferência nenhuma do aperto de mão —
    /// ela só antecipa o **instante** da tentativa, encurtando a espera do furo
    /// quando ele chega antes dela.
    CaminhoAberto {
        /// De onde o furo veio.
        onde: SocketAddr,
    },
    /// Dentro: o aperto de mão terminou e há sessão.
    Dentro,
    /// Nenhum candidato entrou, e este é o motivo.
    ///
    /// Carrega o [`ConnectError`] que já existe, inteiro. Achatar `PinChanged`
    /// e `InviteMismatch` num «falhou» apagaria o alarme do ADR 0003: são os
    /// dois erros desta lista que **não são de rede**.
    Desistiu(ConnectError),
}

impl Etapa {
    /// Um exemplar de cada etapa que esta máquina pode publicar.
    ///
    /// # Por que uma lista, e não três
    ///
    /// Havia três: esta, o array do teste de conformidade e a lista do guarda
    /// da casca — as três escritas à mão, e **nenhuma ligada ao enum**. Uma
    /// variante nova atravessava o `seele-ffi` e caía no «falha que esta tela
    /// não sabe nomear» no meio de uma conexão que ia bem, sem que teste nenhum
    /// acendesse: o compilador cobra o braço de [`Etapa::nome`] e a travessia
    /// do `seele-ffi`, e não cobra lista nenhuma escrita à mão.
    ///
    /// As outras duas agora leem esta. A da casca chega por
    /// `seele_ffi::ConnectStage::todas`, porque o ADR 0002 não deixa
    /// `apps/seele-app` ver o núcleo — e é por isso que a versão anterior
    /// disto, que dizia existir «para a casca poder cobrar cobertura total»,
    /// não podia cumprir o que prometia.
    ///
    /// O buraco não fechou; ele passou a ser **um só**, e tem guarda:
    /// `estados.rs` confere que esta lista tem uma entrada por braço do `match`
    /// de [`Etapa::nome`], que é o que o compilador cobra de verdade. Uma
    /// variante nova que não entre aqui acende lá.
    ///
    /// Os valores são de exemplo e não significam nada — o que se lê deles é a
    /// variante.
    pub const TODAS: [Self; 6] = [
        Self::Parada {
            candidatos: 0,
            com_bilhete_e_impressao: false,
        },
        Self::Avisando {
            ponto: String::new(),
        },
        Self::Tentando {
            candidato: 0,
            de: 0,
            onde: EXEMPLO,
        },
        Self::CaminhoAberto { onde: EXEMPLO },
        Self::Dentro,
        Self::Desistiu(ConnectError::Unreachable),
    ];

    /// O nome estável que atravessa para a casca.
    #[must_use]
    pub fn nome(&self) -> &'static str {
        match self {
            Self::Parada { .. } => "Parada",
            Self::Avisando { .. } => "Avisando",
            Self::Tentando { .. } => "Tentando",
            Self::CaminhoAberto { .. } => "CaminhoAberto",
            Self::Dentro => "Dentro",
            Self::Desistiu(_) => "Desistiu",
        }
    }

    /// Se desta etapa se pode ir para a que tem este nome.
    ///
    /// A máquina inteira em uma função pura, para ela ser conferível sem socket
    /// nenhum. O destino entra como nome e não como valor porque a legalidade
    /// não depende do conteúdo do destino — depende de onde se está.
    ///
    /// # A ausência que é requisito: não existe `Avisando → Desistiu`
    ///
    /// Um ponto de encontro fora do ar, um nome que não resolve, um convite sem
    /// impressão digital — **nenhum deles pode reprovar uma chegada**, porque
    /// nenhum endereço do convite depende dele. O degrau 4 é o de cima da
    /// escada, e perdê-lo não perde os de baixo. Hoje isso é garantido por um
    /// prazo de 600 ms lá em `crate::encontro`; aqui é uma transição que não se
    /// pode escrever, que é mais barato de conferir e não depende do relógio.
    ///
    /// # As duas linhas em que esta máquina é mais frouxa que a tabela do spec
    ///
    /// São **duas**, e pela mesma causa: enquanto [`Chegada::chegar`] delega o
    /// laço a [`crate::enlace::Enlace`] ela observa o primeiro candidato e o
    /// fim, e não os do meio. Cobrar índice obrigaria esta camada a publicar
    /// passos que ninguém viu acontecer, e um passo inventado é pior que um
    /// passo que falta — a trilha existe justamente para responder o que de
    /// fato foi tentado.
    ///
    /// 1. A tabela diz «do **último** candidato para `Desistiu`»; aqui
    ///    `Tentando → Desistiu` vale com qualquer índice, porque o índice do
    ///    último não é observado.
    /// 2. A tabela diz `Tentando|CaminhoAberto → Tentando{i+1}`; aqui
    ///    `Tentando → Tentando` vale com **qualquer** índice, inclusive o
    ///    mesmo, e o mesmo para `CaminhoAberto → Tentando`. Pela mesma razão:
    ///    sem ver os candidatos do meio, `i + 1` não é uma conta que esta
    ///    camada saiba fazer.
    ///
    /// As duas apertam juntas quando o laço mudar de casa e os índices
    /// passarem a ser observados. Nenhuma das duas afrouxa a ausência que é
    /// requisito, que é a de cima.
    #[must_use]
    pub fn transicao_legal(atual: &Self, para: &str) -> bool {
        match (atual, para) {
            // Só com `enc=` no link e impressão digital no primeiro
            // candidato, que é o que a bandeira sabe — e não mais que isso.
            // Ver o que ela promete, e o que ela não promete, em `Parada`.
            (
                Self::Parada {
                    com_bilhete_e_impressao: true,
                    ..
                },
                "Avisando",
            ) => true,
            // Sem candidato nenhum não há o que tentar. É a única desistência
            // que não passa por uma tentativa, e ela é honesta: um convite sem
            // endereço não tem aonde chegar.
            (Self::Parada { candidatos, .. }, "Tentando") => *candidatos > 0,
            (Self::Parada { candidatos: 0, .. }, "Desistiu") => true,
            (Self::Avisando { .. }, "Tentando") => true,
            (Self::Tentando { .. }, "CaminhoAberto" | "Tentando" | "Dentro" | "Desistiu") => true,
            (Self::CaminhoAberto { .. }, "Tentando" | "Dentro" | "Desistiu") => true,
            _ => false,
        }
    }
}

/// Uma etapa e quando ela aconteceu.
///
/// Um por transição, **nunca por retentativa interna**: o aviso que sai três
/// vezes por candidato é um passo só, porque quem lê a trilha está perguntando
/// por onde a conexão passou e não quantos pacotes saíram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passo {
    /// Onde a chegada estava.
    pub etapa: Etapa,
    /// Quanto tempo depois do começo desta chegada.
    pub em: Duration,
}

/// Uma travessia, do convite lido até a sessão — ou até o motivo de não haver.
///
/// De uso único: [`Chegada::chegar`] consome o objeto. Ver o cabeçalho do
/// módulo.
pub struct Chegada {
    /// Os endereços do convite, na ordem em que o anfitrião os ordenou.
    destinos: Vec<Destino>,
    /// O ponto de encontro do link, quando ele trouxe um.
    bilhete: Option<Bilhete>,
    /// O relógio de que a trilha mede as distâncias.
    nascida: Instant,
    /// Por onde esta chegada passou, em ordem.
    trilha: Vec<Passo>,
    /// Para quem acompanha ao vivo.
    estado: watch::Sender<Etapa>,
}

impl std::fmt::Debug for Chegada {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Chegada")
            .field("candidatos", &self.destinos.len())
            .field("bilhete", &self.bilhete.is_some())
            .field("passos", &self.trilha.len())
            .finish()
    }
}

impl Chegada {
    /// Uma chegada parada, com o convite já lido.
    ///
    /// Nada de rede acontece aqui: o relógio começa a correr e a trilha ganha o
    /// primeiro passo, que é [`Etapa::Parada`].
    #[must_use]
    pub fn nova(destinos: Vec<Destino>, bilhete: Option<Bilhete>) -> Self {
        let candidatos = u8::try_from(destinos.len()).unwrap_or(u8::MAX);
        // As duas metades que a bandeira nomeia, e nada além delas: se o
        // aviso vai mesmo sair é `Batida::preparar` quem decide, e ela decide
        // depois daqui. Ver `Etapa::Parada`.
        let com_bilhete_e_impressao = bilhete.is_some()
            && destinos
                .first()
                .is_some_and(|destino| destino.impressao_esperada.is_some());
        let parada = Etapa::Parada {
            candidatos,
            com_bilhete_e_impressao,
        };
        let (estado, _) = watch::channel(parada.clone());
        Self {
            destinos,
            bilhete,
            nascida: Instant::now(),
            trilha: vec![Passo {
                etapa: parada,
                em: Duration::ZERO,
            }],
            estado,
        }
    }

    /// Onde esta chegada está, agora e a cada mudança.
    ///
    /// Um `watch` e não uma fila: quem desenha quer o estado atual, e uma tela
    /// que ficou para trás não tem nada a ganhar desenhando as etapas antigas
    /// uma a uma. Quem quer a história inteira lê a trilha, que a guarda.
    #[must_use]
    pub fn acompanhar(&self) -> watch::Receiver<Etapa> {
        self.estado.subscribe()
    }

    /// Por onde esta chegada passou até agora.
    #[must_use]
    pub fn trilha(&self) -> &[Passo] {
        &self.trilha
    }

    /// Atravessa: tenta os candidatos do convite e devolve a sessão.
    ///
    /// # O que esta versão faz, e o que ela ainda não vê
    ///
    /// O laço de candidatos continua em [`crate::enlace::Enlace`], inteiro e
    /// sem uma linha movida — com o aviso por candidato, a repetição em tarefa
    /// de fundo e o prazo curto do candidato distante que a tarefa anterior
    /// escreveu. Daqui saem as etapas que esta camada **causa e observa**: o
    /// convite lido, o aviso que o link autoriza, a primeira tentativa, e o
    /// fim. As tentativas do meio ganham nome quando o laço mudar de casa; até
    /// lá a trilha diz menos do que vai dizer, e nada do que ela diz é
    /// inventado.
    ///
    /// # Errors
    ///
    /// [`Frustrada`], que é o [`ConnectError`] de sempre **mais a trilha**. O
    /// motivo escolhido é o mesmo de [`crate::enlace::Enlace::conectar_entre`]:
    /// o de quem respondeu, se algum respondeu.
    pub async fn chegar(
        mut self,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Enlace, Frustrada> {
        let de = u8::try_from(self.destinos.len()).unwrap_or(u8::MAX);
        let Some(primeiro) = self.destinos.first().map(|destino| destino.servidor) else {
            // Um convite sem endereço nenhum. Ninguém chama assim, e desistir
            // com o motivo na mão é melhor que devolver uma chegada que não
            // consta de lugar nenhum.
            return Err(self.desistir(ConnectError::Unreachable));
        };

        // O aviso é publicado sob as mesmas condições em que ele sai — ver a
        // bandeira em `Etapa::Parada`. Que o `LEVE` de um candidato específico
        // tenha saído ou não é assunto do laço: um envio recusado não reprova
        // chegada nenhuma, que é a aresta que esta máquina não tem.
        if let Some(ponto) = self.ponto_a_avisar() {
            self.marcar(Etapa::Avisando { ponto });
        }
        self.marcar(Etapa::Tentando {
            candidato: 0,
            de,
            onde: primeiro,
        });

        let destinos = std::mem::take(&mut self.destinos);
        let bilhete = self.bilhete.clone();
        match Enlace::conectar_entre_com_bilhete(destinos, bilhete, chave, pins).await {
            Ok(enlace) => {
                self.marcar(Etapa::Dentro);
                Ok(enlace)
            }
            Err(erro) => Err(self.desistir(erro)),
        }
    }

    /// O ponto de encontro que vai ser avisado, se algum vai.
    fn ponto_a_avisar(&self) -> Option<String> {
        let bilhete = self.bilhete.as_ref()?;
        let primeiro = self.destinos.first()?;
        primeiro.impressao_esperada.as_ref()?;
        Some(bilhete.ponto.clone())
    }

    /// Fecha a chegada no motivo, e entrega a trilha a quem vai lê-la.
    fn desistir(mut self, motivo: ConnectError) -> Frustrada {
        self.marcar(Etapa::Desistiu(motivo.clone()));
        Frustrada {
            motivo,
            trilha: self.trilha,
        }
    }

    /// Publica uma transição, se ela existir nesta máquina.
    ///
    /// A conferência é aqui, e não num auxiliar que só o teste chama, porque é
    /// esta linha que faz de [`Etapa::transicao_legal`] uma regra em vez de uma
    /// opinião: uma aresta que a máquina não tem não entra na trilha nem sai no
    /// `watch`, e o passo que falta acende o teste que espera por ele.
    fn marcar(&mut self, etapa: Etapa) {
        let legal = self
            .trilha
            .last()
            .is_some_and(|passo| Etapa::transicao_legal(&passo.etapa, etapa.nome()));
        if !legal {
            tracing::error!(
                para = etapa.nome(),
                "uma transição que a máquina de estados da chegada não tem"
            );
            return;
        }
        self.trilha.push(Passo {
            etapa: etapa.clone(),
            em: self.nascida.elapsed(),
        });
        // `send` só falha quando não há ninguém ouvindo, que é o caso comum: a
        // trilha é o registro, e o `watch` é a comodidade de quem desenha.
        let _ = self.estado.send(etapa);
    }
}

/// Uma chegada que não chegou: o motivo, e por onde ela passou.
///
/// A trilha é lida daqui e não do objeto porque a [`Chegada`] é de uso único e
/// já morreu quando este valor existe. Ela sobrevive à falha de propósito —
/// «tentei quatro candidatos, o primeiro deu prazo esgotado em 4 s, o quarto
/// recusou» é o dado que faltou quando o teste de campo das duas casas falhou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frustrada {
    motivo: ConnectError,
    trilha: Vec<Passo>,
}

impl Frustrada {
    /// Por que não deu.
    #[must_use]
    pub fn motivo(&self) -> &ConnectError {
        &self.motivo
    }

    /// Por onde a chegada passou antes de acabar.
    #[must_use]
    pub fn trilha(&self) -> &[Passo] {
        &self.trilha
    }
}

impl From<Frustrada> for ConnectError {
    /// Para quem só quer o erro de sempre.
    fn from(frustrada: Frustrada) -> Self {
        frustrada.motivo
    }
}

impl std::fmt::Display for Frustrada {
    /// Para log e para `Error`, nunca para uma pessoa: a frase é da casca.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:?} depois de {} passos",
            self.motivo,
            self.trilha.len()
        )
    }
}

impl std::error::Error for Frustrada {}

#[cfg(test)]
mod tests {
    use super::*;

    fn destino(porta: u16) -> Destino {
        Destino {
            servidor: SocketAddr::from(([127, 0, 0, 1], porta)),
            nome_tls: "localhost".into(),
            chave_do_pin: format!("127.0.0.1:{porta}"),
            apelido: "piloto".into(),
            segredo: None,
            impressao_esperada: Some("0123456789abcdef0123".into()),
        }
    }

    fn bilhete() -> Bilhete {
        let Ok(bilhete) = Bilhete::novo("encontro.exemplo:8384", "203.0.113.7:8383") else {
            panic!("o bilhete de teste deixou de ser um bilhete");
        };
        bilhete
    }

    #[test]
    fn uma_transicao_que_a_maquina_nao_tem_nao_entra_na_trilha() {
        // A conferência de `transicao_legal` tem de estar **na fiação**, e não
        // só num auxiliar que o teste chama: uma regra que só o teste consulta
        // é uma regra que a produção não tem.
        let mut chegada = Chegada::nova(vec![destino(1)], None);
        chegada.marcar(Etapa::Dentro);

        assert_eq!(
            chegada.trilha().len(),
            1,
            "`Parada → Dentro` não existe, e ainda assim foi publicada: {:?}",
            chegada.trilha()
        );
        assert!(matches!(
            chegada.trilha().first().map(|passo| &passo.etapa),
            Some(Etapa::Parada { .. })
        ));
    }

    #[test]
    fn um_convite_sem_impressao_digital_nao_promete_aviso_nenhum() {
        // Sem a impressão digital não sai datagrama nenhum — a marca do aviso é
        // feita dela. Publicar `Avisando` aqui seria a tela afirmando um degrau
        // que não vai acontecer, que é o mesmo defeito do log que dizia
        // «avisamos o ponto de encontro» com o envio recusado.
        let mut cego = destino(1);
        cego.impressao_esperada = None;
        let chegada = Chegada::nova(vec![cego], Some(bilhete()));

        assert!(chegada.ponto_a_avisar().is_none());
        assert!(
            matches!(
                chegada.trilha().first().map(|passo| &passo.etapa),
                Some(Etapa::Parada {
                    com_bilhete_e_impressao: false,
                    ..
                })
            ),
            "{:?}",
            chegada.trilha()
        );

        let com_impressao = Chegada::nova(vec![destino(1)], Some(bilhete()));
        assert_eq!(
            com_impressao.ponto_a_avisar().as_deref(),
            Some("encontro.exemplo:8384")
        );
    }

    #[test]
    fn a_trilha_mede_o_tempo_desde_o_comeco_e_nao_desde_o_passo_anterior() {
        // «O primeiro deu prazo esgotado em 4 s» é uma frase sobre o começo da
        // chegada. Medir desde o passo anterior daria a mesma trilha com outro
        // significado, e ninguém notaria lendo os números.
        let mut chegada = Chegada::nova(vec![destino(1)], None);
        std::thread::sleep(Duration::from_millis(20));
        chegada.marcar(Etapa::Tentando {
            candidato: 0,
            de: 1,
            onde: SocketAddr::from(([127, 0, 0, 1], 1)),
        });
        std::thread::sleep(Duration::from_millis(20));
        chegada.marcar(Etapa::Desistiu(ConnectError::Unreachable));

        let Some(primeiro) = chegada.trilha().get(1) else {
            panic!("a tentativa não entrou na trilha");
        };
        let Some(fim) = chegada.trilha().get(2) else {
            panic!("a desistência não entrou na trilha");
        };
        assert!(
            fim.em >= primeiro.em + Duration::from_millis(15),
            "o segundo passo teria de estar ao menos 20 ms depois do primeiro, e \
             está em {:?} contra {:?}",
            fim.em,
            primeiro.em
        );
    }

    #[tokio::test]
    async fn quem_acompanha_ve_a_etapa_mudar() {
        // O `watch` é a metade que a tela lê. Ele nasce em `Parada` e anda com
        // a trilha; um `send` que sumisse deixaria o spinner mudo que esta
        // tarefa existe para acabar.
        let mut chegada = Chegada::nova(vec![destino(1)], None);
        let mut olhos = chegada.acompanhar();
        assert!(matches!(&*olhos.borrow_and_update(), Etapa::Parada { .. }));

        chegada.marcar(Etapa::Tentando {
            candidato: 0,
            de: 1,
            onde: SocketAddr::from(([127, 0, 0, 1], 1)),
        });
        assert!(olhos.has_changed().unwrap_or(false));
        assert!(matches!(
            &*olhos.borrow(),
            Etapa::Tentando { candidato: 0, .. }
        ));
    }
}
