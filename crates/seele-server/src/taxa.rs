//! Limitação de taxa: o que segura quem abusa depois de a porta existir.
//!
//! O ADR 0021 pôs um porteiro no Dogma e disse, na própria lista de
//! consequências, o que ele **não** resolvia: *"um convidado legítimo pode
//! inundar de mensagens; não há limitação de taxa"*. Isto é a outra metade.
//!
//! São dois abusos diferentes, e por isso dois baldes:
//!
//! 1. **Antes de autenticar** — [`Portaria`], com chave no endereço de origem.
//!    Segura quem bate à porta em laço. Um aperto de mão custa Argon2id quando
//!    o Dogma tem senha (ADR 0021: "de propósito, não é caminho quente"), e um
//!    pacote que compra dezenas de milissegundos de CPU alheia é amplificação
//!    boa demais para deixar de graça na internet.
//! 2. **Depois de autenticar** — [`Vigia`], com chave na conexão. Segura o
//!    convidado legítimo que inunda, que é o caso que o ADR 0021 nomeia.
//!
//! O terceiro uso é a mídia, em [`crate::cage`]: aquele limite já existia como
//! janela fixa e passou a usar o mesmo [`Balde`], para o Dogma ter **um**
//! mecanismo de limitação e não três parecidos.
//!
//! # Por que balde de fichas e não janela fixa
//!
//! Uma janela fixa de um segundo aceita o limite inteiro no fim de uma janela e
//! o limite inteiro no começo da seguinte: o dobro da taxa contratada, em
//! milissegundos, e sempre no mesmo instante do relógio — que é justamente o
//! instante com que um atacante se sincroniza. O balde não tem borda: as fichas
//! voltam continuamente, então a rajada máxima é a capacidade e a taxa
//! sustentada é a de reposição, sem um terceiro número escondido.
//!
//! # O tempo entra por parâmetro
//!
//! Nada aqui lê relógio. Quem chama passa o instante, como em
//! [`crate::dogma::Slots`] e no `battery.rs` do `seele-core`. É o que torna
//! testável o comportamento que importa — o que acontece no limite — sem um
//! único `sleep`, e sem depender de a máquina de CI estar desocupada.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;

/// Quantos apertos de mão um mesmo endereço pode fazer em rajada.
///
/// O pior caso honesto não é uma pessoa: é um NAT. A bateria interna do cliente
/// (`crates/seele-core/src/battery.rs`) tenta reconectar com espera exponencial
/// de 500 ms até 15 s durante cinco minutos, o que dá cerca de vinte e quatro
/// tentativas por cliente numa queda longa. Três pessoas na mesma casa, atrás
/// do mesmo endereço público, saindo juntas de um roteador que reiniciou, são
/// setenta tentativas em cinco minutos — e nenhuma delas é ataque.
///
/// Trinta de rajada cobre a partida simultânea dessa casa; a reposição abaixo
/// cobre o resto da queda.
pub const APERTOS_DE_RAJADA: u32 = 30;

/// Com que velocidade a rajada de apertos de mão se repõe, por minuto.
///
/// Trinta por minuto é o dobro da pior casa honesta contada acima (catorze por
/// minuto) e um centésimo do que um laço de script faz sem esforço. Para quem
/// está varrendo, o teto passa a ser meio aperto de mão por segundo por
/// endereço — com Argon2id a alguns milissegundos, sobra ruído em vez de carga.
pub const APERTOS_POR_MINUTO: f64 = 30.0;

/// Quantos endereços a [`Portaria`] lembra ao mesmo tempo.
///
/// A tabela é ela própria uma superfície: um balde por endereço, sem teto, é
/// memória do Dogma para quem tiver endereços de sobra. Quatro mil e noventa e
/// seis baldes cabem em algumas centenas de kilobytes, e um Dogma de amigos
/// (`specs/04-servidor-seele.md` dimensiona uns cinquenta pilotos) nunca vê
/// número dessa ordem.
pub const ENDERECOS_LEMBRADOS: usize = 4096;

/// Quantos quadros de controle uma conexão pode mandar em rajada.
///
/// Entrar num Cage, abrir as Linhas e pedir o histórico de cada uma é uma
/// rajada real, e acontece toda vez que alguém conecta ou reconecta. Sessenta
/// dá folga para um punhado de Linhas sem chegar perto do limite.
pub const QUADROS_DE_RAJADA: u32 = 60;

/// Quantos quadros de controle por segundo uma conexão sustenta.
///
/// Controle é raro: uma mensagem de texto, um `Ping` a cada cinco segundos
/// (`specs/02-protocolo.md`), uma página de histórico ao rolar a tela. Vinte
/// por segundo, sustentados, é uma ordem de grandeza acima do que uma pessoa
/// digitando ou colando consegue produzir — e continua sendo uma ordem de
/// grandeza abaixo do que um laço produz.
pub const QUADROS_POR_SEGUNDO: f64 = 20.0;

/// Quantos quadros excedentes se descarta antes de derrubar a conexão.
///
/// Derrubar no primeiro quadro excedente puniria um cliente que só está mal
/// escrito, e derrubar nunca deixaria a sessão gastar o Dogma para sempre. O
/// primeiro excedente vira aviso, os seguintes viram descarte, e duzentos
/// descartes — dez segundos inteiros no dobro do teto — são a prova de que
/// ninguém do outro lado leu o aviso.
pub const PACIENCIA: u32 = 200;

/// Um balde de fichas.
///
/// Uma ficha por evento. Elas voltam a `por_segundo` e o balde nunca passa de
/// `capacidade`, o que dá as duas garantias que interessam: a rajada máxima é a
/// capacidade e a taxa sustentada é a reposição.
#[derive(Debug, Clone)]
pub struct Balde {
    capacidade: f64,
    por_segundo: f64,
    fichas: f64,
    ultima: Instant,
}

impl Balde {
    /// Um balde cheio.
    ///
    /// Cheio, e não vazio, de propósito: quem chega pela primeira vez não deve
    /// esperar para falar. O limite existe para o segundo evento em diante.
    #[must_use]
    pub fn novo(capacidade: u32, por_segundo: f64, agora: Instant) -> Self {
        let capacidade = f64::from(capacidade);
        Self {
            capacidade,
            por_segundo,
            fichas: capacidade,
            ultima: agora,
        }
    }

    /// Repõe o que o tempo devolveu desde a última consulta.
    ///
    /// `saturating_duration_since` porque um instante anterior ao último visto
    /// — que não deveria acontecer, `Instant` é monotônico — não pode virar
    /// pânico numa função que atende a rede.
    fn repor(&mut self, agora: Instant) {
        let decorrido = agora.saturating_duration_since(self.ultima).as_secs_f64();
        self.fichas = (self.fichas + decorrido * self.por_segundo).min(self.capacidade);
        self.ultima = agora;
    }

    /// Gasta uma ficha, se houver.
    pub fn tentar(&mut self, agora: Instant) -> bool {
        self.repor(agora);
        if self.fichas >= 1.0 {
            self.fichas -= 1.0;
            true
        } else {
            false
        }
    }

    /// Se o balde já voltou a estar cheio.
    ///
    /// Um balde cheio não carrega informação nenhuma: ele é indistinguível de
    /// um recém-criado. É o que autoriza a [`Portaria`] a esquecê-lo sem mudar
    /// resposta nenhuma, e o [`Vigia`] a perdoar quem se aquietou.
    pub fn cheio(&mut self, agora: Instant) -> bool {
        self.repor(agora);
        self.fichas >= self.capacidade
    }
}

/// Os baldes de antes de autenticar, um por endereço de origem.
///
/// Chave no endereço e não na identidade porque, antes do desafio-resposta, não
/// existe identidade — é exatamente esse o ponto de quem bate à porta em laço.
#[derive(Debug, Default)]
pub struct Portaria {
    baldes: HashMap<IpAddr, Balde>,
    /// Quantas vezes a tabela cheia recusou um endereço novo.
    lotada: u64,
}

impl Portaria {
    /// Uma portaria que não viu ninguém.
    #[must_use]
    pub fn nova() -> Self {
        Self::default()
    }

    /// Se este endereço pode gastar um aperto de mão agora.
    pub fn permitir(&mut self, origem: IpAddr, agora: Instant) -> bool {
        if let Some(balde) = self.baldes.get_mut(&origem) {
            return balde.tentar(agora);
        }

        if self.baldes.len() >= ENDERECOS_LEMBRADOS {
            self.esquecer_cheios(agora);
        }
        if self.baldes.len() >= ENDERECOS_LEMBRADOS {
            // Milhares de endereços distintos gastando fichas ao mesmo tempo é
            // um Dogma sob rede de máquinas, não um amigo chegando. Recusar o
            // desconhecido é a falha correta: o custo cai sobre quem chega
            // durante o ataque, e não sobre quem já está dentro.
            self.lotada += 1;
            return false;
        }

        let mut balde = Balde::novo(APERTOS_DE_RAJADA, APERTOS_POR_MINUTO / 60.0, agora);
        let permitido = balde.tentar(agora);
        self.baldes.insert(origem, balde);
        permitido
    }

    /// Descarta os baldes que já se repuseram por inteiro.
    fn esquecer_cheios(&mut self, agora: Instant) {
        self.baldes.retain(|_, balde| !balde.cheio(agora));
    }

    /// Quantos endereços estão sendo lembrados.
    #[must_use]
    pub fn lembrados(&self) -> usize {
        self.baldes.len()
    }

    /// Quantas vezes a tabela cheia recusou um endereço novo.
    #[must_use]
    pub fn recusas_por_lotacao(&self) -> u64 {
        self.lotada
    }
}

/// O que fazer com um quadro de controle que acabou de chegar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Veredito {
    /// Dentro do orçamento. Segue o jogo.
    Passa,
    /// Fora do orçamento, e é a primeira vez. Descarta e avisa.
    ///
    /// Derrubar sem explicação é o que faz alguém achar que o produto está
    /// quebrado. O aviso é `AlertReason::RateLimited`, e o quadro que o
    /// provocou não é executado.
    Avisa,
    /// Fora do orçamento, já avisado. Descarta calado.
    Descarta,
    /// Fora do orçamento e sem sinal de que vá parar. Derruba.
    Derruba,
}

/// O balde de depois de autenticar, um por conexão.
///
/// Chave na conexão e não no piloto de propósito: o mesmo piloto em duas
/// máquinas são duas conexões, e quem abre conexões em série para diluir o
/// limite esbarra antes na [`Portaria`], que conta por endereço. Os dois baldes
/// se compõem; nenhum dos dois sozinho fecha os dois caminhos.
#[derive(Debug)]
pub struct Vigia {
    balde: Balde,
    descartados: u32,
    avisado: bool,
}

impl Vigia {
    /// Um vigia para uma conexão que acabou de nascer.
    #[must_use]
    pub fn novo(agora: Instant) -> Self {
        Self {
            balde: Balde::novo(QUADROS_DE_RAJADA, QUADROS_POR_SEGUNDO, agora),
            descartados: 0,
            avisado: false,
        }
    }

    /// Julga um quadro de controle.
    pub fn avaliar(&mut self, agora: Instant) -> Veredito {
        // Consultado antes de gastar a ficha: cheio quer dizer que a conexão
        // ficou quieta tempo bastante para o balde inteiro voltar, e quem se
        // aquietou tem direito de ser perdoado — inclusive de ser avisado de
        // novo, daqui a horas, se voltar a exagerar.
        let recuperado = self.balde.cheio(agora);
        if self.balde.tentar(agora) {
            if recuperado {
                self.descartados = 0;
                self.avisado = false;
            }
            return Veredito::Passa;
        }

        self.descartados += 1;
        if self.descartados > PACIENCIA {
            return Veredito::Derruba;
        }
        if self.avisado {
            Veredito::Descarta
        } else {
            self.avisado = true;
            Veredito::Avisa
        }
    }

    /// Quantos quadros esta conexão já perdeu por exceder o limite.
    #[must_use]
    pub fn descartados(&self) -> u32 {
        self.descartados
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::Duration;

    /// Um instante fixo, para os testes não dependerem de quando rodam.
    fn zero() -> Instant {
        Instant::now()
    }

    #[test]
    fn o_balde_comeca_cheio_e_a_rajada_cabe_inteira() {
        // Quem chega pela primeira vez não pode esperar para falar.
        let inicio = zero();
        let mut balde = Balde::novo(10, 1.0, inicio);
        for numero in 0..10 {
            assert!(balde.tentar(inicio), "a ficha {numero} devia caber");
        }
    }

    #[test]
    fn passada_a_rajada_o_balde_recusa() {
        let inicio = zero();
        let mut balde = Balde::novo(3, 1.0, inicio);
        assert!(balde.tentar(inicio));
        assert!(balde.tentar(inicio));
        assert!(balde.tentar(inicio));
        assert!(!balde.tentar(inicio), "a quarta ficha não existia");
    }

    #[test]
    fn as_fichas_voltam_com_o_tempo() {
        let inicio = zero();
        let mut balde = Balde::novo(3, 1.0, inicio);
        for _ in 0..3 {
            assert!(balde.tentar(inicio));
        }

        // Uma ficha por segundo: meio segundo não devolve nenhuma.
        assert!(!balde.tentar(inicio + Duration::from_millis(500)));
        assert!(balde.tentar(inicio + Duration::from_millis(1_000)));
        assert!(!balde.tentar(inicio + Duration::from_millis(1_001)));
    }

    #[test]
    fn o_balde_nao_acumula_alem_da_capacidade() {
        // É o que separa um balde de um crédito: quem ficou uma hora calado não
        // ganha uma hora de rajada.
        let inicio = zero();
        let mut balde = Balde::novo(3, 1.0, inicio);
        let depois = inicio + Duration::from_secs(3_600);
        for _ in 0..3 {
            assert!(balde.tentar(depois));
        }
        assert!(!balde.tentar(depois), "o balde acumulou uma hora de fichas");
    }

    #[test]
    fn a_taxa_sustentada_e_a_de_reposicao_e_nao_o_dobro() {
        // A propriedade que a janela fixa não tem. Numa janela de um segundo,
        // gastar tudo no fim de uma e tudo no começo da outra passa o dobro do
        // limite em milissegundos — e sempre no mesmo instante do relógio, que
        // é com o que um atacante se sincroniza.
        let inicio = zero();
        let mut balde = Balde::novo(10, 10.0, inicio);

        // A rajada inteira no instante em que a "janela" acabaria.
        let borda = inicio + Duration::from_millis(999);
        let mut passaram = 0;
        for _ in 0..10 {
            if balde.tentar(borda) {
                passaram += 1;
            }
        }
        assert_eq!(passaram, 10, "a rajada devia caber");

        // E logo em seguida, do outro lado da borda, nada passa: as fichas
        // voltam por tempo decorrido, não por o relógio ter virado.
        let logo_depois = inicio + Duration::from_millis(1_001);
        assert!(
            !balde.tentar(logo_depois),
            "a virada da janela deixou passar uma segunda rajada"
        );
    }

    #[test]
    fn um_instante_para_tras_nao_derruba_o_balde() {
        // `Instant` é monotônico e isto não deveria acontecer; se acontecer,
        // não pode virar pânico numa função que atende a rede.
        let inicio = zero() + Duration::from_secs(10);
        let mut balde = Balde::novo(2, 1.0, inicio);
        assert!(balde.tentar(inicio - Duration::from_secs(5)));
    }

    #[test]
    fn a_portaria_conta_por_endereco() {
        // Um endereço estourando não pode fechar a porta do vizinho.
        let inicio = zero();
        let mut portaria = Portaria::nova();
        let abusador: IpAddr = "203.0.113.7".parse().expect("endereço");
        let vizinho: IpAddr = "203.0.113.8".parse().expect("endereço");

        for _ in 0..APERTOS_DE_RAJADA {
            assert!(portaria.permitir(abusador, inicio));
        }
        assert!(!portaria.permitir(abusador, inicio), "a rajada não acabou");
        assert!(
            portaria.permitir(vizinho, inicio),
            "o vizinho pagou pelo abusador"
        );
    }

    #[test]
    fn a_casa_atras_do_nat_nunca_encosta_no_limite() {
        // O pior caso honesto: três clientes atrás do mesmo endereço público,
        // cada um gastando a bateria interna inteira depois de o roteador
        // reiniciar — cerca de vinte e quatro tentativas em cinco minutos.
        let inicio = zero();
        let mut portaria = Portaria::nova();
        let casa: IpAddr = "198.51.100.4".parse().expect("endereço");

        let tentativas = 24 * 3;
        for numero in 0..tentativas {
            // Espalhadas pelos cinco minutos da bateria, como a espera
            // exponencial as espalha.
            let quando = inicio + Duration::from_millis(numero * (300_000 / tentativas));
            assert!(
                portaria.permitir(casa, quando),
                "a tentativa {numero} de uma casa honesta foi tratada como ataque"
            );
        }
    }

    #[test]
    fn a_portaria_esquece_quem_se_repos_por_inteiro() {
        // Um balde cheio é indistinguível de um recém-criado, então esquecê-lo
        // não muda resposta nenhuma — e é o que impede a tabela de crescer com
        // um endereço que passou uma vez, há uma hora.
        let inicio = zero();
        let mut portaria = Portaria::nova();
        for numero in 0..50_u8 {
            let endereco = IpAddr::from([198, 51, 100, numero]);
            assert!(portaria.permitir(endereco, inicio));
        }
        assert_eq!(portaria.lembrados(), 50);

        portaria.esquecer_cheios(inicio + Duration::from_secs(3_600));
        assert_eq!(portaria.lembrados(), 0);
    }

    #[test]
    fn a_portaria_nao_esquece_quem_ainda_esta_devendo() {
        let inicio = zero();
        let mut portaria = Portaria::nova();
        let abusador: IpAddr = "203.0.113.7".parse().expect("endereço");
        for _ in 0..APERTOS_DE_RAJADA {
            assert!(portaria.permitir(abusador, inicio));
        }

        portaria.esquecer_cheios(inicio + Duration::from_secs(1));
        assert_eq!(
            portaria.lembrados(),
            1,
            "esquecer o balde de quem estourou é devolver a rajada de graça"
        );
        assert!(!portaria.permitir(abusador, inicio + Duration::from_secs(1)));
    }

    #[test]
    fn a_tabela_da_portaria_tem_teto() {
        // A tabela é ela própria uma superfície: sem teto, um balde por
        // endereço é memória do Dogma para quem tiver endereços de sobra.
        let inicio = zero();
        let mut portaria = Portaria::nova();
        for numero in 0..u32::try_from(ENDERECOS_LEMBRADOS).expect("cabe") + 10 {
            let endereco = IpAddr::from(std::net::Ipv4Addr::from(numero));
            // Gasta a rajada inteira, para nenhum balde ficar esquecível.
            for _ in 0..APERTOS_DE_RAJADA {
                portaria.permitir(endereco, inicio);
            }
        }
        assert_eq!(portaria.lembrados(), ENDERECOS_LEMBRADOS);
        assert!(portaria.recusas_por_lotacao() > 0);
    }

    #[test]
    fn o_vigia_deixa_passar_a_rajada_de_entrada() {
        // Entrar num Cage, abrir as Linhas e pedir o histórico de cada uma.
        let inicio = zero();
        let mut vigia = Vigia::novo(inicio);
        for numero in 0..QUADROS_DE_RAJADA {
            assert_eq!(
                vigia.avaliar(inicio),
                Veredito::Passa,
                "o quadro {numero} da entrada foi barrado"
            );
        }
    }

    #[test]
    fn o_vigia_avisa_antes_de_descartar() {
        let inicio = zero();
        let mut vigia = Vigia::novo(inicio);
        for _ in 0..QUADROS_DE_RAJADA {
            vigia.avaliar(inicio);
        }

        assert_eq!(vigia.avaliar(inicio), Veredito::Avisa);
        assert_eq!(vigia.avaliar(inicio), Veredito::Descarta);
        assert_eq!(vigia.avaliar(inicio), Veredito::Descarta);
    }

    #[test]
    fn o_vigia_derruba_quem_nao_para() {
        // O aviso existe para quem lê. Quem não lê, sai.
        let inicio = zero();
        let mut vigia = Vigia::novo(inicio);
        for _ in 0..QUADROS_DE_RAJADA {
            vigia.avaliar(inicio);
        }
        for _ in 0..PACIENCIA {
            assert_ne!(vigia.avaliar(inicio), Veredito::Derruba);
        }
        assert_eq!(vigia.avaliar(inicio), Veredito::Derruba);
    }

    #[test]
    fn quem_se_aquieta_e_perdoado() {
        // Uma sessão de três horas não pode morrer por ter sido avisada uma vez
        // na primeira hora.
        let inicio = zero();
        let mut vigia = Vigia::novo(inicio);
        for _ in 0..QUADROS_DE_RAJADA {
            vigia.avaliar(inicio);
        }
        assert_eq!(vigia.avaliar(inicio), Veredito::Avisa);

        // Quieto o bastante para o balde inteiro voltar.
        let depois = inicio + Duration::from_secs(60);
        assert_eq!(vigia.avaliar(depois), Veredito::Passa);
        assert_eq!(vigia.descartados(), 0, "o descarte antigo não foi perdoado");

        // E o aviso volta a valer, em vez de o próximo excesso cair calado. O
        // balde já gastou uma ficha na linha acima.
        for _ in 0..QUADROS_DE_RAJADA - 1 {
            assert_eq!(vigia.avaliar(depois), Veredito::Passa);
        }
        assert_eq!(vigia.avaliar(depois), Veredito::Avisa);
    }

    #[test]
    fn uma_conversa_de_pessoa_nunca_encosta_no_vigia() {
        // Duas mensagens por segundo durante dez minutos, mais um `Ping` a cada
        // cinco segundos. É mais do que alguém digita, e passa inteiro.
        let inicio = zero();
        let mut vigia = Vigia::novo(inicio);
        for decimo in 0..6_000_u64 {
            let quando = inicio + Duration::from_millis(decimo * 100);
            if decimo % 5 == 0 {
                assert_eq!(vigia.avaliar(quando), Veredito::Passa);
            }
            if decimo % 50 == 0 {
                assert_eq!(vigia.avaliar(quando), Veredito::Passa);
            }
        }
    }
}
