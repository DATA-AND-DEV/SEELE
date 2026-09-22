//! Reduz o ruído de fundo do microfone antes de qualquer decisão sobre ele.
//!
//! F02 de `docs/features-v15.md`: «adicionar um filtro de voz ao áudio do
//! microfone, a la Discord». O escopo escolhido é o que aquele documento
//! escreve — **supressão de ruído com preservação da fala natural** — e não
//! cancelamento de eco, que é outro problema e tem avaliação própria.
//!
//! # Por onde ela entra, e por que a ordem importa
//!
//! `captura → supressão → decisão de ativação → ganho → codificação`, que é a
//! ordem que o documento propõe. Cada uma das três fronteiras é uma decisão:
//!
//! - **antes do portão**, porque é o que torna o portão mais sensível
//!   defensável. O `gate` decide por energia e é a única defesa contra o
//!   ventilador; com o piso da sala removido, o mesmo limiar passa a ouvir fala
//!   baixa em vez de ouvir a sala. É a costura que o cabeçalho do `gate` prometia
//!   desde que ele existe;
//! - **antes do ganho**, porque o ganho automático amplifica o que recebe. Depois
//!   da supressão ele amplifica voz; antes dela, amplificaria o ventilador junto
//!   e desfaria o trabalho;
//! - **antes da codificação**, porque o Opus não é o lugar de decidir o que é
//!   voz — ele codifica o que chega, e chega menos ruído.
//!
//! # Como, e por que não uma rede neural
//!
//! Subtração espectral com estimativa de piso por banda. Cada bloco vira
//! espectro, cada raia tem o piso de ruído dela rastreado por mínimos, e o ganho
//! da raia é o quanto ela passa do próprio piso.
//!
//! O ADR 0007 tira DSP em C da v1, e o ADR 0015 registra a mesma decisão para o
//! `webrtc-vad`: outra ligação em C com o mesmo perfil de manutenção da que M0.4
//! teve de abandonar. Uma porta de RNNoise seria mais eficaz contra ruído não
//! estacionário e traria um modelo, uma licença e um arquivo a empacotar. Isto é
//! Rust puro, **zero crate novo na árvore** — o `realfft` já vem pelo `rubato`,
//! que este crate já usa — e resolve bem o caso que o pedido nomeia: ventilador e
//! ar-condicionado, que são estacionários.
//!
//! # O que ela mede, neste banco de testes
//!
//! `o_chiado_cai_mais_que_o_tom`, com chiado de banda larga a −45 dBFS e um tom
//! de 300 Hz por cima: **sobra 23% do ruído** — uns 12,8 dB de atenuação — e
//! **95% da voz**. Os números saem impressos naquele teste, e é deles que
//! `EXCESSO` e `ALISAMENTO` vieram.
//!
//! Isto é sinal sintético, e a diferença importa: o documento pede gravações
//! reais de fala baixa, teclado, ventilador, headset e microfone de notebook, com
//! CPU e latência medidas na máquina. Nada disso está feito, e este parágrafo não
//! o substitui — o que ele diz é que o mecanismo funciona, e não que ele já foi
//! avaliado com voz de gente.
//!
//! **O que ela não faz, dito antes de alguém descobrir:** digitação é transiente
//! e o rastreador de mínimos não a aprende; o que sobra dela é atenuado pelo
//! ganho da raia no instante da batida, e não removido. O documento pede medida
//! com teclado, e a medida vai encontrar isto.
//!
//! # Latência
//!
//! Um salto, que é metade de um quadro: **10 ms**. A sobreposição de 50% é o que
//! permite reconstruir o sinal sem costura audível, e ela custa esperar meio
//! bloco antes de o primeiro pedaço sair. O documento pede que a latência
//! adicional seja medida e registrada; este parágrafo é o número, e
//! [`Supressao::atraso_em_amostras`] é ele em código.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Quantas amostras cada bloco de análise tem.
///
/// Um quadro do produto — 20 ms a 48 kHz. Igual ao quadro de propósito: um bloco
/// de análise de outro tamanho obrigaria a duas contabilidades de buffer, e o
/// ganho em resolução de frequência não paga isso.
const BLOCO: usize = crate::FRAME_SAMPLES;

/// De quanto em quanto um bloco novo começa. Metade do bloco: sobreposição de 50%.
const SALTO: usize = BLOCO / 2;

/// Quanto do piso estimado é subtraído de cada raia.
///
/// Acima de 1 de propósito, e por duas razões que se somam: o piso é rastreado
/// pelo **mínimo**, que fica abaixo da média mesmo depois de alisado, e a fala
/// está bem acima dos dois. Subtrair exatamente o piso deixaria passar metade do
/// ruído nas raias onde ele oscila.
///
/// 2,2 é o valor em que o teste `o_chiado_cai_mais_que_o_tom` mede o chiado caindo
/// para menos da metade com a voz preservada. Era 1,5, e com 1,5 sobrava ruído
/// demais — o número está aqui porque foi medido, e não escolhido.
const EXCESSO: f32 = 2.2;

/// Quanto cada salto move a magnitude alisada na direção da crua.
///
/// 0,25 é uma memória de umas quatro medidas — 40 ms. Curto o bastante para
/// acompanhar o começo de uma sílaba, longo o bastante para o mínimo rastreado
/// ficar perto da média em vez do fundo do poço. Ver [`Supressao::alisada`].
const ALISAMENTO: f32 = 0.25;

/// O mínimo que uma raia conserva, mesmo quando ela é toda ruído.
///
/// **Não é zero, e a razão é audível.** Zerar raias inteiras produz «ruído
/// musical»: o que sobra passa a ser um chiado com altura, que chama mais atenção
/// que o ruído que ele substituiu. Deixar 12% mantém um piso plano, e um piso
/// plano o ouvido descarta.
const GANHO_MINIMO: f32 = 0.12;

/// Quantas subjanelas o mínimo deslizante guarda.
///
/// Quatro de 250 ms, que é **um segundo** de memória. Ver
/// [`Supressao::minimos`] para por que o piso é um mínimo de janela e não uma
/// média que se reinfla.
const SUBJANELAS: usize = 4;

/// Quantos saltos cada subjanela cobre. 25 saltos são 250 ms.
const SALTOS_POR_SUBJANELA: u32 = 25;

/// Quantos saltos de sinal o piso precisa ver antes de valer.
///
/// Dez saltos são 100 ms. Antes disso o piso é um palpite, e subtrair um palpite
/// de uma fala que começou no primeiro instante é cortar a primeira sílaba —
/// exatamente o que F01 pede para preservar. Até ali, o sinal passa intocado.
const SALTOS_PARA_APRENDER: u32 = 10;

/// A supressão de ruído do microfone.
///
/// Uma instância por caminho de captura. Ela guarda o piso aprendido e o resto da
/// sobreposição, então trocar de microfone pede uma nova — ver
/// [`Self::esquecer`], que é o que o caminho de troca de aparelho chama em vez de
/// reconstruir tudo.
pub struct Supressao {
    adiante: Arc<dyn RealToComplex<f32>>,
    atras: Arc<dyn ComplexToReal<f32>>,
    /// A janela de análise e de síntese, a mesma nas duas.
    ///
    /// Raiz de Hann, e não Hann: aplicada duas vezes — na análise e na síntese —
    /// ela vira Hann, e Hann com salto de metade soma exatamente um. Com Hann nas
    /// duas pontas a soma seria Hann ao quadrado, que **não** soma um, e o sinal
    /// sairia com uma ondulação de 100 Hz por cima.
    janela: Vec<f32>,
    /// Amostras que chegaram e ainda não completaram um bloco.
    entrada: Vec<f32>,
    /// A metade do bloco anterior que ainda vai somar com a próxima.
    cauda: Vec<f32>,
    /// Amostras prontas, esperando quem as peça.
    saida: std::collections::VecDeque<f32>,
    /// O piso de ruído estimado, por raia: o **mínimo do último segundo**.
    ///
    /// Recalculado de [`Self::minimos`] a cada salto. É ele que a subtração usa.
    piso: Vec<f32>,
    /// O mínimo de cada subjanela, por raia.
    ///
    /// # Por que um mínimo de janela, e não uma média que se reinfla
    ///
    /// A primeira versão deste módulo rastreava o mínimo com uma média
    /// assimétrica: descida imediata, subida multiplicativa lenta. Ela tem um
    /// defeito que a auditoria de 22/09/2026 encontrou (A02) e duas tentativas de
    /// conserto não resolveram:
    ///
    ///   - **multiplicar não tira o piso do zero.** Um começo em silêncio digital
    ///     — o que um dispositivo entrega enquanto o fluxo abre — deixava o piso em
    ///     zero para o resto da sessão, e passavam 100,0% do ruído;
    ///   - **um passo aditivo** que caiba na escala de uma raia leva quase um
    ///     minuto para aprender. Medido: 97% do ruído passando;
    ///   - **semear de novo quando o piso morre** conserta o ruído e dispara em
    ///     **cada vale de sílaba**, porque um vale de fala também leva o piso a
    ///     zero. Medido: sobravam 26% de uma fala com sílabas.
    ///
    /// O mínimo numa janela deslizante responde os quatro casos sem nenhum caso
    /// especial, e é o que a literatura de estatística de mínimos faz:
    ///
    /// | Situação | Mínimo do último segundo | Está certo? |
    /// |---|---|---|
    /// | só ruído | o piso do ruído | sim — é o que subtrair |
    /// | fala com vales | ~zero, porque os vales entram na janela | sim — não há ruído a subtrair |
    /// | silêncio e depois ruído | depois de um segundo, o piso do ruído | sim — aprende |
    /// | ruído que some | ~zero, quando a janela enche de silêncio | sim — nada a subtrair |
    ///
    /// Quatro subjanelas de 250 ms em vez de um anel de 100 valores por raia: a
    /// memória é a mesma e o custo por salto é comparar quatro números em vez de
    /// cem.
    minimos: Vec<Vec<f32>>,
    /// Qual subjanela está sendo preenchida agora.
    subjanela: usize,
    /// Quantos saltos já entraram na subjanela de agora.
    saltos_na_subjanela: u32,
    /// A magnitude alisada de cada raia, que é o que o piso rastreia.
    ///
    /// # Por que o piso não rastreia a magnitude crua
    ///
    /// Porque o mínimo de um sinal ruidoso é muito menor que a média dele.
    /// Medido: rastreando a magnitude crua de chiado de banda larga, o piso
    /// assentava tão abaixo da média que a subtração quase não acontecia — 79,5%
    /// do ruído sobrava, com o teste `o_chiado_cai_mais_que_o_tom` reprovando duas
    /// vezes seguidas por causa disto.
    ///
    /// Alisar antes de rastrear tira a variância e aproxima o mínimo da média,
    /// que é o número que interessa. É o que a literatura de estatística de
    /// mínimos faz, e é a metade que faltava.
    alisada: Vec<f32>,
    /// Quantos saltos já entraram na estimativa.
    saltos: u32,
    /// Quanto da supressão é aplicado, de zero a um.
    forca: f32,
    /// Espaço de trabalho, para o laço de áudio não alocar.
    tempo: Vec<f32>,
    espectro: Vec<Complex<f32>>,
    ganhos: Vec<f32>,
}

impl std::fmt::Debug for Supressao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Os planos de FFT não são `Debug`, e despejar o piso por raia seria
        // quinhentos números num log. O que descreve esta instância é o estado.
        f.debug_struct("Supressao")
            .field("forca", &self.forca)
            .field("saltos", &self.saltos)
            .field("pendentes", &self.saida.len())
            .finish()
    }
}

/// O anel de entrada como ele começa: **um salto de silêncio adiante do sinal**.
///
/// # Por que semear, em vez de deixar o primeiro quadro sair pela metade
///
/// A sobreposição de 50% retém um salto: sem semente, o primeiro bloco produz
/// meio quadro, e meio quadro é o que o codificador Opus recusa com
/// `WrongFrameSize` — a auditoria de 22/09/2026 (A04) mediu o primeiro trecho de
/// fala de cada sessão sendo descartado assim.
///
/// Semeado, o primeiro quadro que entra já completa dois blocos e sai inteiro. O
/// atraso é o mesmo — [`Supressao::atraso_em_amostras`] —, e ele aparece como
/// **10 ms de silêncio na frente do fluxo** em vez de como um pedaço que não
/// cabe em lugar nenhum. A conta de amostras passa a fechar exata, e é isso que
/// `nao_acumula_nem_perde_amostras` mede.
fn entrada_semeada() -> Vec<f32> {
    let mut entrada = Vec::with_capacity(BLOCO * 2);
    entrada.resize(SALTO, 0.0);
    entrada
}

impl Supressao {
    /// Uma supressão nova, sem piso aprendido.
    ///
    /// `forca` vai de `0.0` — passa tudo — a `1.0`, e é fixada nessa faixa.
    #[must_use]
    pub fn nova(forca: f32) -> Self {
        let mut planejador = RealFftPlanner::<f32>::new();
        let adiante = planejador.plan_fft_forward(BLOCO);
        let atras = planejador.plan_fft_inverse(BLOCO);
        let raias = BLOCO / 2 + 1;
        #[allow(
            clippy::cast_precision_loss,
            reason = "BLOCO é 960; a conversão é exata"
        )]
        let janela: Vec<f32> = (0..BLOCO)
            .map(|n| {
                let hann = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / BLOCO as f32).cos();
                hann.sqrt()
            })
            .collect();
        Self {
            espectro: adiante.make_output_vec(),
            tempo: adiante.make_input_vec(),
            adiante,
            atras,
            janela,
            entrada: entrada_semeada(),
            cauda: vec![0.0; SALTO],
            saida: std::collections::VecDeque::with_capacity(BLOCO * 2),
            piso: vec![0.0; raias],
            minimos: vec![vec![f32::INFINITY; raias]; SUBJANELAS],
            subjanela: 0,
            saltos_na_subjanela: 0,
            alisada: vec![0.0; raias],
            saltos: 0,
            forca: forca.clamp(0.0, 1.0),
            ganhos: vec![1.0; raias],
        }
    }

    /// Quanto da supressão está sendo aplicado. `0.0` passa tudo.
    #[must_use]
    pub const fn forca(&self) -> f32 {
        self.forca
    }

    /// Muda a força sem perder o piso aprendido.
    ///
    /// **Sem perder**, e é a diferença que importa: ligar e desligar o filtro no
    /// meio de uma conversa não pode custar os 100 ms de aprendizado de novo, ou
    /// a primeira frase depois de religar sai cortada. F02, critério 4.
    pub fn ajustar(&mut self, forca: f32) {
        self.forca = forca.clamp(0.0, 1.0);
    }

    /// Esquece o piso aprendido e o que estava no meio de um bloco.
    ///
    /// Chamada ao trocar de microfone: o piso de um aparelho não descreve outro,
    /// e o resto de sobreposição de um fluxo somado ao começo de outro é um
    /// estalo.
    pub fn esquecer(&mut self) {
        self.piso.fill(0.0);
        for minimo in &mut self.minimos {
            minimo.fill(f32::INFINITY);
        }
        self.subjanela = 0;
        self.saltos_na_subjanela = 0;
        self.alisada.fill(0.0);
        self.saltos = 0;
        self.entrada = entrada_semeada();
        self.cauda.fill(0.0);
        self.saida.clear();
    }

    /// Quantas amostras de atraso esta supressão acrescenta.
    ///
    /// Um salto — metade de um bloco, 10 ms a 48 kHz. É o preço da sobreposição
    /// de 50%, e é ela que permite reconstruir o sinal sem costura audível.
    #[must_use]
    pub const fn atraso_em_amostras() -> usize {
        SALTO
    }

    /// Passa um quadro pela supressão e devolve o que já está pronto.
    ///
    /// **Ou um quadro inteiro, ou nada** — nunca um pedaço.
    ///
    /// # Por que a saída parcial foi um defeito, e não uma economia
    ///
    /// A primeira versão entregava «o que couber», e no primeiro quadro couberam
    /// metade das amostras: a sobreposição de 50% retém um salto. A auditoria de
    /// 22/09/2026 (A04) mediu o que acontecia com elas — o codificador Opus exige
    /// exatamente [`crate::FRAME_SAMPLES`] e recusa o resto com `WrongFrameSize`,
    /// então o primeiro trecho de fala de cada sessão era **descartado com um erro
    /// que ninguém lia**.
    ///
    /// Retendo o pedaço até fechar um quadro, o atraso é o mesmo — ver
    /// [`Self::atraso_em_amostras`] —, o que sai é sempre codificável, e quem chama
    /// não precisa de um segundo buffer para desfazer o que este já tinha.
    ///
    /// Quem entrega quadros inteiros — o laço de captura — recebe um quadro
    /// inteiro **desde a primeira volta**, porque o anel de entrada nasce com um
    /// salto de silêncio adiante: ver [`entrada_semeada`]. O «nada» sobrou para
    /// quem alimenta em pedaços menores que um bloco, e ali ele é a espera normal
    /// de o bloco fechar.
    ///
    /// Com `forca` em zero devolve o que entrou, sem passar pela FFT: desligar o
    /// filtro tem de custar zero, inclusive de atraso.
    pub fn processar(&mut self, quadro: &[f32], para: &mut Vec<f32>) {
        para.clear();
        if self.forca <= 0.0 {
            // **O caminho sem filtro, e ele existe por contrato.** F02 pede uma
            // saída sem filtro para quando o processamento não estiver
            // disponível, e desligar é o caso normal disso. Zero atraso, zero
            // alocação, zero diferença no sinal.
            para.extend_from_slice(quadro);
            return;
        }

        self.entrada.extend_from_slice(quadro);
        while self.entrada.len() >= BLOCO {
            self.um_bloco();
            self.entrada.drain(..SALTO);
        }
        // **Ou o quadro inteiro, ou nada.** Ver o doc: um pedaço é o que o
        // codificador recusa.
        if self.saida.len() >= quadro.len() {
            para.extend(self.saida.drain(..quadro.len()));
        }
    }

    /// Analisa, atenua e ressintetiza um bloco.
    fn um_bloco(&mut self) {
        let Some(bloco) = self.entrada.get(..BLOCO) else {
            return;
        };
        for (destino, (amostra, peso)) in self
            .tempo
            .iter_mut()
            .zip(bloco.iter().zip(self.janela.iter()))
        {
            *destino = amostra * peso;
        }
        if self
            .adiante
            .process(&mut self.tempo, &mut self.espectro)
            .is_err()
        {
            // Um plano que recusa o próprio tamanho é impossível por construção;
            // se acontecer, o quadro passa intocado em vez de sair silêncio.
            self.passar_sem_filtro();
            return;
        }

        self.aprender_e_atenuar();

        if self
            .atras
            .process(&mut self.espectro, &mut self.tempo)
            .is_err()
        {
            self.passar_sem_filtro();
            return;
        }
        // `realfft` não normaliza a volta.
        #[allow(
            clippy::cast_precision_loss,
            reason = "BLOCO é 960; a conversão é exata"
        )]
        let escala = 1.0 / BLOCO as f32;
        for (amostra, peso) in self.tempo.iter_mut().zip(self.janela.iter()) {
            *amostra *= escala * peso;
        }

        // Soma-e-sobrepõe: a primeira metade sai somada com a cauda anterior, e
        // a segunda metade **vira** a cauda.
        for indice in 0..SALTO {
            let anterior = self.cauda.get(indice).copied().unwrap_or(0.0);
            let agora = self.tempo.get(indice).copied().unwrap_or(0.0);
            self.saida.push_back(anterior + agora);
        }
        for indice in 0..SALTO {
            if let (Some(destino), Some(origem)) =
                (self.cauda.get_mut(indice), self.tempo.get(SALTO + indice))
            {
                *destino = *origem;
            }
        }
        self.saltos = self.saltos.saturating_add(1);
    }

    /// Atualiza o piso por raia e aplica o ganho de cada uma.
    fn aprender_e_atenuar(&mut self) {
        // **Rastreamento de mínimos, e o primeiro salto semeia.**
        //
        // Semear importa: um piso que começa em zero e sobe devagar não estima
        // nada — ver `REINFLA_O_PISO` para a medida que mostrou isso. Semeado no
        // primeiro sinal, ele já está na ordem de grandeza certa, e o rastreador
        // desce dali até o vale entre duas sílabas.
        //
        // Se o primeiro salto for fala, o piso nasce alto e **desce sozinho** na
        // primeira pausa, porque a descida é imediata. É o autoconserto que uma
        // média não tem.
        // **O piso é o mínimo do último segundo**, por raia. Ver
        // [`Self::minimos`] para as três tentativas que este desenho substitui.
        //
        // Alisar antes de minimizar: o mínimo de um sinal ruidoso cru fica muito
        // abaixo da média dele, e a subtração quase não acontece. Medido — a
        // primeira versão deste módulo deixava passar 79,5% do ruído.
        let primeiro = self.saltos == 0;
        // `subjanela` fica na faixa por construção — ela só anda por `% SUBJANELAS`
        // logo abaixo —, e o índice direto seria um `panic` possível num laço de
        // áudio. Sem a subjanela não há o que minimizar, e a alisada continua
        // andando: ela é o sinal, e não a memória.
        let janela = self.minimos.get_mut(self.subjanela);
        debug_assert!(janela.is_some(), "a subjanela saiu da faixa");
        let mut janela = janela.map(|janela| janela.iter_mut());
        for (raia, alisada) in self.espectro.iter().zip(self.alisada.iter_mut()) {
            let magnitude = raia.norm();
            if primeiro {
                *alisada = magnitude;
            } else {
                *alisada += ALISAMENTO * (magnitude - *alisada);
            }
            if let Some(minimo) = janela.as_mut().and_then(Iterator::next) {
                *minimo = minimo.min(*alisada);
            }
        }

        // O piso publicado: o menor das subjanelas. Uma subjanela que ainda não
        // recebeu nada vale `INFINITY` e não vence nenhuma comparação, que é o
        // comportamento certo — ela não sabe nada.
        for (indice, piso) in self.piso.iter_mut().enumerate() {
            let mut menor = f32::INFINITY;
            for janela in &self.minimos {
                if let Some(valor) = janela.get(indice) {
                    menor = menor.min(*valor);
                }
            }
            *piso = if menor.is_finite() { menor } else { 0.0 };
        }

        // E a janela anda. A subjanela que entra é zerada: ela vai contar o mínimo
        // dos próximos 250 ms, e herdar o mínimo de um segundo atrás seria a
        // memória nunca esquecer.
        self.saltos_na_subjanela += 1;
        if self.saltos_na_subjanela >= SALTOS_POR_SUBJANELA {
            self.saltos_na_subjanela = 0;
            self.subjanela = (self.subjanela + 1) % SUBJANELAS;
            if let Some(janela) = self.minimos.get_mut(self.subjanela) {
                janela.fill(f32::INFINITY);
            }
        }

        // **Antes de aprender, nada é subtraído.** Ver `SALTOS_PARA_APRENDER`:
        // subtrair um palpite de uma fala que começou no primeiro instante é
        // cortar a primeira sílaba, que é o que F01 pede para preservar.
        if self.saltos < SALTOS_PARA_APRENDER {
            return;
        }

        for ((raia, piso), ganho) in self
            .espectro
            .iter()
            .zip(self.piso.iter())
            .zip(self.ganhos.iter_mut())
        {
            let magnitude = raia.norm();
            *ganho = if magnitude <= f32::EPSILON {
                1.0
            } else {
                ((magnitude - EXCESSO * piso) / magnitude).clamp(GANHO_MINIMO, 1.0)
            };
        }

        // **Os ganhos são alisados entre raias vizinhas.** Sem isto, duas raias
        // vizinhas com ganhos muito diferentes produzem o ruído musical que o
        // `GANHO_MINIMO` também combate — um harmônico da voz atenuado ao lado de
        // um vizinho intacto soa como um apito.
        //
        // Média de três, uma passada, e sobre uma cópia: alisar no lugar
        // propagaria o primeiro valor por toda a faixa.
        let originais = self.ganhos.clone();
        for indice in 1..originais.len().saturating_sub(1) {
            if let (Some(antes), Some(aqui), Some(depois)) = (
                originais.get(indice - 1),
                originais.get(indice),
                originais.get(indice + 1),
            ) {
                if let Some(destino) = self.ganhos.get_mut(indice) {
                    *destino = (antes + aqui + depois) / 3.0;
                }
            }
        }

        // E a força mistura o ganho com «não mexer». `forca` em 1 aplica o ganho
        // inteiro; em 0,5, metade do caminho até ele.
        for (raia, ganho) in self.espectro.iter_mut().zip(self.ganhos.iter()) {
            let aplicado = 1.0 - self.forca * (1.0 - ganho);
            *raia *= aplicado;
        }
    }

    /// O caminho de desistência: entrega o bloco como ele entrou.
    ///
    /// **Silêncio nunca é a resposta.** F02 pede uma saída sem filtro quando o
    /// processamento falha, e é isto: a FFT recusar o próprio tamanho é
    /// impossível por construção, e se acontecer a conversa continua.
    fn passar_sem_filtro(&mut self) {
        let Some(bloco) = self.entrada.get(..BLOCO) else {
            return;
        };
        for indice in 0..SALTO {
            let anterior = self.cauda.get(indice).copied().unwrap_or(0.0);
            let agora = bloco.get(indice).copied().unwrap_or(0.0);
            self.saida.push_back(anterior + agora);
        }
        for indice in 0..SALTO {
            if let (Some(destino), Some(origem)) =
                (self.cauda.get_mut(indice), bloco.get(SALTO + indice))
            {
                // Metade, porque a outra metade vem da cauda na próxima volta.
                *destino = *origem * 0.5;
            }
        }
        self.saltos = self.saltos.saturating_add(1);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um seno, que é o mais parecido com fala que um teste determinístico tem.
    fn seno(hz: f32, amplitude: f32, quantas: usize, fase: &mut f32) -> Vec<f32> {
        #[allow(clippy::cast_precision_loss, reason = "contagens pequenas")]
        let passo = 2.0 * std::f32::consts::PI * hz / crate::SAMPLE_RATE_HZ as f32;
        (0..quantas)
            .map(|_| {
                let amostra = amplitude * fase.sin();
                *fase += passo;
                amostra
            })
            .collect()
    }

    /// Ruído de banda larga determinístico, no nível do piso de uma sala.
    fn chiado(amplitude: f32, quantas: usize, semente: &mut u32) -> Vec<f32> {
        (0..quantas)
            .map(|_| {
                // Congruência linear: determinístico, e este teste não pode
                // flutuar.
                *semente = semente.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "o sorteio só precisa cobrir a faixa"
                )]
                let unitario = (*semente >> 8) as f32 / (1_u32 << 24) as f32;
                amplitude * (unitario * 2.0 - 1.0)
            })
            .collect()
    }

    fn rms(amostras: &[f32]) -> f32 {
        if amostras.is_empty() {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss, reason = "quadros de 960")]
        let media = amostras.iter().map(|a| a * a).sum::<f32>() / amostras.len() as f32;
        media.sqrt()
    }

    /// **Desligada, ela não toca no sinal nem acrescenta atraso.**
    ///
    /// F02: «preservar uma saída sem filtro se o processamento falhar ou não
    /// estiver disponível». Desligar é o caso normal disso, e um desligado que
    /// ainda passasse pela FFT pagaria o atraso sem comprar nada.
    #[test]
    fn desligada_o_sinal_sai_como_entrou() {
        let mut supressao = Supressao::nova(0.0);
        let mut fase = 0.0;
        let quadro = seno(440.0, 0.2, crate::FRAME_SAMPLES, &mut fase);
        let mut saiu = Vec::new();
        supressao.processar(&quadro, &mut saiu);
        assert_eq!(saiu, quadro, "a supressão desligada mexeu no sinal");
    }

    /// **Ligada, o ruído estacionário cai muito mais que a voz.**
    ///
    /// É o critério 1 de F02: «ruído diminui sem remover palavras». Medido como
    /// razão: o que sobra do chiado sozinho contra o que sobra do seno sozinho.
    #[test]
    fn o_chiado_cai_mais_que_o_tom() {
        let mut supressao = Supressao::nova(1.0);
        let mut semente = 7;
        let mut saiu = Vec::new();

        // Meio segundo de sala vazia: é onde o piso é aprendido.
        for _ in 0..25 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
        }

        // O chiado sozinho, depois de aprendido.
        let mut ruido_dentro = 0.0;
        let mut ruido_fora = 0.0;
        for _ in 0..10 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
            ruido_dentro += rms(&quadro);
            ruido_fora += rms(&saiu);
        }

        // E a voz, sobre o mesmo chiado.
        let mut fase = 0.0;
        let mut voz_dentro = 0.0;
        let mut voz_fora = 0.0;
        for _ in 0..10 {
            let tom = seno(300.0, 0.08, crate::FRAME_SAMPLES, &mut fase);
            let fundo = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            let quadro: Vec<f32> = tom.iter().zip(fundo.iter()).map(|(a, b)| a + b).collect();
            supressao.processar(&quadro, &mut saiu);
            voz_dentro += rms(&quadro);
            voz_fora += rms(&saiu);
        }

        let sobrou_do_ruido = ruido_fora / ruido_dentro;
        let sobrou_da_voz = voz_fora / voz_dentro;
        // Impresso para quem for mexer nas constantes: o número que sai daqui é a
        // medida, e é dela que os valores de `EXCESSO` e `ALISAMENTO` saíram.
        println!("MEDIDA: sobrou do ruido {sobrou_do_ruido:.3}, sobrou da voz {sobrou_da_voz:.3}");
        assert!(
            sobrou_do_ruido < 0.5,
            "o chiado quase não caiu: sobrou {sobrou_do_ruido:.3} dele"
        );
        assert!(
            sobrou_da_voz > 0.6,
            "a voz foi levada junto: sobrou só {sobrou_da_voz:.3} dela"
        );
        assert!(
            sobrou_da_voz > sobrou_do_ruido * 1.5,
            "a supressão não distingue voz de ruído: voz {sobrou_da_voz:.3}, \
             ruído {sobrou_do_ruido:.3}"
        );
    }

    /// **Antes de aprender, nada é cortado.**
    ///
    /// F01 pede para preservar o começo das palavras, e o pior jeito de perdê-lo
    /// é subtrair um piso que ainda é um palpite. Os primeiros 100 ms passam.
    #[test]
    fn a_primeira_silaba_nao_e_subtraida_de_um_palpite() {
        let mut supressao = Supressao::nova(1.0);
        let mut fase = 0.0;
        let mut saiu = Vec::new();
        let mut total_dentro = 0.0;
        let mut total_fora = 0.0;

        // Fala desde o primeiro quadro, sem sala vazia antes.
        for _ in 0..5 {
            let quadro = seno(300.0, 0.08, crate::FRAME_SAMPLES, &mut fase);
            supressao.processar(&quadro, &mut saiu);
            total_dentro += rms(&quadro);
            total_fora += rms(&saiu);
        }
        // O primeiro salto fica retido pela sobreposição, então a soma de saída é
        // menor por construção. O que este teste recusa é a **atenuação**: com o
        // piso aprendendo a voz, o que sai é uma fração pequena do que entrou.
        assert!(
            total_fora > total_dentro * 0.6,
            "o começo da fala foi atenuado por um piso que ainda não existia: \
             entrou {total_dentro:.4}, saiu {total_fora:.4}"
        );
    }

    /// **A conta de amostras fecha exata.**
    ///
    /// Um filtro que devolvesse mais ou menos do que recebe iria acumular ou
    /// esvaziar o caminho até o codificador — e o sintoma seria o áudio andando
    /// para trás no tempo, devagar, durante uma conversa inteira.
    ///
    /// «Exata» é novo: até a auditoria de 22/09/2026 faltava um salto, que era o
    /// meio quadro retido pela sobreposição e nunca entregue. Ele agora entra como
    /// silêncio na frente do fluxo — [`entrada_semeada`] —, e o que fica a dever é
    /// zero.
    #[test]
    fn nao_acumula_nem_perde_amostras() {
        let mut supressao = Supressao::nova(1.0);
        let mut fase = 0.0;
        let mut saiu = Vec::new();
        let mut dentro = 0_usize;
        let mut fora = 0_usize;

        for _ in 0..50 {
            let quadro = seno(300.0, 0.1, crate::FRAME_SAMPLES, &mut fase);
            supressao.processar(&quadro, &mut saiu);
            dentro += quadro.len();
            fora += saiu.len();
        }
        assert_eq!(
            dentro, fora,
            "a conta de amostras não fecha: o caminho até o codificador acumula \
             ou esvazia, e o áudio anda no tempo durante a conversa"
        );
    }

    /// Ligar e desligar no meio não perde o piso aprendido. F02, critério 4.
    #[test]
    fn religar_nao_custa_o_aprendizado_de_novo() {
        let mut supressao = Supressao::nova(1.0);
        let mut semente = 11;
        let mut saiu = Vec::new();
        for _ in 0..25 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
        }
        let aprendido: f32 = supressao.piso.iter().sum();
        assert!(aprendido > 0.0, "o piso não foi aprendido");

        supressao.ajustar(0.0);
        supressao.ajustar(1.0);
        let depois: f32 = supressao.piso.iter().sum();
        assert!(
            (depois - aprendido).abs() < f32::EPSILON,
            "desligar e religar apagou o piso: a primeira frase depois de religar \
             sai cortada"
        );

        // E trocar de microfone **apaga**, que é o certo: o piso de um aparelho
        // não descreve outro.
        supressao.esquecer();
        assert_eq!(supressao.piso.iter().sum::<f32>(), 0.0);
    }

    /// **A04: o que sai da supressão é sempre um quadro inteiro.**
    ///
    /// # O defeito que este teste reproduz
    ///
    /// A primeira chamada devolvia **480** amostras — metade de um quadro, retida
    /// pela sobreposição. O laço de voz conferia só «veio vazio?» e mandava o que
    /// viesse ao portão e ao codificador; o codificador devolve
    /// `WrongFrameSize { got: 480, expected: 960 }`, e o laço trata erro de
    /// codificação com `continue`. O primeiro trecho de cada captura era perdido.
    ///
    /// Auditoria de 22/09/2026, A04. O teste local de microfone não pegava isto:
    /// ele toca as amostras direto, sem passar pelo codec.
    #[test]
    fn a_saida_e_sempre_um_quadro_inteiro_ou_nada() {
        let mut supressao = Supressao::nova(1.0);
        let mut fase = 0.0;
        let mut saiu = Vec::new();

        for volta in 0..20 {
            let quadro = seno(300.0, 0.1, crate::FRAME_SAMPLES, &mut fase);
            supressao.processar(&quadro, &mut saiu);
            // **Inteiro desde a primeira volta**, e a primeira é a que importa: era
            // ela que devolvia 480 amostras. Ver `entrada_semeada`.
            assert_eq!(
                saiu.len(),
                crate::FRAME_SAMPLES,
                "a volta {volta} devolveu {} amostras: o codificador exige {} e \
                 recusa o resto com `WrongFrameSize`, e o laço de voz perde o \
                 trecho",
                saiu.len(),
                crate::FRAME_SAMPLES
            );
        }

        // **E quem alimenta em pedaços tortos também recebe pedaço inteiro ou
        // nada.** Quinhentas amostras não são múltiplo do salto, então em algumas
        // voltas há menos de um pedaço pronto — e é ali que a entrega «o que
        // couber» devolvia uma fração. Nenhum chamador de hoje faz isto; o contrato
        // vale para o próximo, e sem esta metade do teste ele não vale nada.
        let mut supressao = Supressao::nova(1.0);
        for volta in 0..20 {
            let quadro = seno(300.0, 0.1, 500, &mut fase);
            supressao.processar(&quadro, &mut saiu);
            assert!(
                saiu.is_empty() || saiu.len() == 500,
                "a volta {volta} devolveu {} amostras de um pedaço de 500: uma \
                 fração é o que o codificador recusa",
                saiu.len()
            );
        }

        // **E desligar e religar no meio, com áudio entrando**, que é o caminho de
        // quem mexe no controle durante uma conversa. A auditoria pede este caso por
        // nome: o resto de sobreposição que ficou de antes não pode virar meio
        // quadro na volta.
        let mut supressao = Supressao::nova(1.0);
        for volta in 0..20 {
            if volta == 5 {
                supressao.ajustar(0.0);
            }
            if volta == 10 {
                supressao.ajustar(1.0);
            }
            let quadro = seno(300.0, 0.1, crate::FRAME_SAMPLES, &mut fase);
            supressao.processar(&quadro, &mut saiu);
            assert_eq!(
                saiu.len(),
                crate::FRAME_SAMPLES,
                "a volta {volta}, com o filtro sendo desligado e religado, devolveu \
                 {} amostras",
                saiu.len()
            );
        }
    }

    /// **A02: depois de silêncio digital, o piso ainda aprende o ruído.**
    ///
    /// # O defeito que este teste reproduz
    ///
    /// O piso é semeado com a magnitude do primeiro bloco. Quando esse bloco é
    /// silêncio digital — zeros exatos —, o piso nasce zero, e a reinflação é
    /// **multiplicativa**: zero vezes qualquer coisa é zero. O piso não conseguia
    /// crescer nunca mais, e a supressão passava a não subtrair nada.
    ///
    /// Auditoria de 22/09/2026, A01 daquele documento: 500 ms de zeros seguidos
    /// de cinco segundos de ruído deixavam passar 100,01% do sinal.
    ///
    /// Silêncio digital no começo não é caso de laboratório: é o que um
    /// dispositivo entrega enquanto o fluxo abre, e é o que o anel devolve antes
    /// de a primeira amostra chegar.
    #[test]
    fn depois_de_silencio_digital_o_piso_ainda_aprende() {
        let mut supressao = Supressao::nova(1.0);
        let mut saiu = Vec::new();

        // Meio segundo de zeros exatos, que é o que abre um dispositivo.
        for _ in 0..25 {
            supressao.processar(&vec![0.0; crate::FRAME_SAMPLES], &mut saiu);
        }

        // E então o ruído aparece.
        let mut semente = 3;
        for _ in 0..50 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
        }

        let mut dentro = 0.0;
        let mut fora = 0.0;
        for _ in 0..20 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
            dentro += rms(&quadro);
            fora += rms(&saiu);
        }
        let sobrou = fora / dentro;
        println!("MEDIDA A02: sobrou {sobrou:.4} do ruído depois de silêncio digital");
        assert!(
            sobrou < 0.5,
            "o piso ficou preso em zero: um começo em silêncio digital desliga a \
             supressão para o resto da sessão. Sobrou {sobrou:.4} do ruído"
        );
    }

    /// E o mesmo quando o ruído **some e volta** — um ventilador que desliga.
    ///
    /// O piso desce até o silêncio, e tem de voltar a subir quando o ruído
    /// retorna. É o mesmo defeito por outra porta.
    #[test]
    fn o_piso_volta_a_aprender_quando_o_ruido_reaparece() {
        let mut supressao = Supressao::nova(1.0);
        let mut saiu = Vec::new();
        let mut semente = 5;

        for _ in 0..40 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
        }
        // O ventilador desliga: silêncio digital por um segundo.
        for _ in 0..50 {
            supressao.processar(&vec![0.0; crate::FRAME_SAMPLES], &mut saiu);
        }
        // E volta.
        for _ in 0..50 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
        }

        let mut dentro = 0.0;
        let mut fora = 0.0;
        for _ in 0..20 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
            dentro += rms(&quadro);
            fora += rms(&saiu);
        }
        let sobrou = fora / dentro;
        println!("MEDIDA A02b: sobrou {sobrou:.4} do ruído que voltou");
        assert!(
            sobrou < 0.5,
            "o ruído voltou e a supressão não: sobrou {sobrou:.4} dele"
        );
    }

    /// **Fala que começa logo depois de silêncio digital não é comida.**
    ///
    /// A troca que o conserto do A02 faz: um piso morto é semeado da magnitude de
    /// agora, e se o que aparece for fala o piso nasce em nível de voz. O que
    /// impede o estrago é a **janela de aprendizado recomeçar** junto — nada é
    /// subtraído nos 100 ms seguintes, e nesse tempo o rastreador de mínimos desce
    /// até o vale entre duas sílabas.
    ///
    /// # Por que o sinal tem sílabas, e não é um tom contínuo
    ///
    /// Porque um tom contínuo não tem vale nenhum, e **nenhum** estimador de ruído
    /// por mínimos distingue um tom eterno de um ventilador — eles são a mesma
    /// coisa. Medir contra um tom contínuo mediria essa limitação e a chamaria de
    /// defeito da fala.
    ///
    /// Fala tem estrutura silábica: rajadas de ~180 ms com vales curtos entre
    /// elas. É o que este sinal imita, e é o mínimo honesto para a afirmação que o
    /// teste faz. **Não é voz de gente** — a avaliação com gravações reais continua
    /// pendente, e está dita no ADR 0055.
    #[test]
    fn a_fala_logo_depois_do_silencio_nao_e_comida() {
        let mut supressao = Supressao::nova(1.0);
        let mut saiu = Vec::new();

        // Silêncio digital, como o de um dispositivo abrindo.
        for _ in 0..25 {
            supressao.processar(&vec![0.0; crate::FRAME_SAMPLES], &mut saiu);
        }

        // E fala, do primeiro quadro em diante: nove quadros de sílaba, um de vale.
        let mut fase = 0.0;
        let mut dentro = 0.0;
        let mut fora = 0.0;
        for volta in 0..40 {
            let quadro = if volta % 10 == 9 {
                vec![0.0; crate::FRAME_SAMPLES]
            } else {
                seno(300.0, 0.08, crate::FRAME_SAMPLES, &mut fase)
            };
            supressao.processar(&quadro, &mut saiu);
            dentro += rms(&quadro);
            fora += rms(&saiu);
        }
        let sobrou = fora / dentro;
        println!("MEDIDA A02c: sobrou {sobrou:.4} da fala que começou depois do silêncio");
        assert!(
            sobrou > 0.5,
            "a fala que começou depois do silêncio foi tratada como ruído: sobrou \
             só {sobrou:.4} dela"
        );
    }

    /// **Quanto ruído sobra, em dBFS.** É o número que o portão precisa conhecer.
    ///
    /// A03 da auditoria: o padrão de −60 dBFS foi escolhido supondo que o
    /// residual ficasse abaixo dele, e a medida diz o contrário. Este teste
    /// imprime o número e prende só o que é seguro prender — que ele é **maior**
    /// que o silêncio e **menor** que a entrada.
    #[test]
    fn o_residual_da_supressao_tem_um_nivel_mensuravel() {
        let mut supressao = Supressao::nova(1.0);
        let mut saiu = Vec::new();
        let mut semente = 9;
        for _ in 0..50 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
        }
        let mut soma = 0.0;
        for _ in 0..50 {
            let quadro = chiado(0.01, crate::FRAME_SAMPLES, &mut semente);
            supressao.processar(&quadro, &mut saiu);
            soma += rms(&saiu);
        }
        let residual = crate::gate::dbfs_de_rms(soma / 50.0);
        println!("MEDIDA A03: residual médio {residual:.2} dBFS");
        assert!(
            residual > -90.0 && residual < -20.0,
            "o residual saiu da faixa plausível: {residual:.2} dBFS"
        );
    }

    /// A força é fixada na faixa, e valores absurdos não viram silêncio.
    #[test]
    fn a_forca_fica_na_faixa() {
        let mut supressao = Supressao::nova(9.0);
        assert!((supressao.forca() - 1.0).abs() < f32::EPSILON);
        supressao.ajustar(-3.0);
        assert!(supressao.forca().abs() < f32::EPSILON);
    }
}
