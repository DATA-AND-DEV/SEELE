//! O filtro e o portão **juntos**, que é a única forma em que eles existem. F01, F02.
//!
//! # Por que este arquivo existe
//!
//! Porque os dois passavam em separado enquanto o produto abria o microfone no
//! ventilador. A auditoria de 22/09/2026 mediu isto (A03), e a medida é o motivo
//! deste arquivo:
//!
//! > ruído determinístico de amplitude 0,01 […] Após um segundo de aquecimento, o
//! > gate permaneceu aberto em **200/200 quadros**, com nível médio residual de
//! > **−58,03 dBFS**. Não havia tom nem fala no sinal.
//!
//! O defeito não estava em nenhum dos dois módulos: estava na junta. A supressão
//! entregava o que prometia — ruído 12 dB mais baixo — e o portão abria em
//! −60 dBFS, que é **acima** do que sobra. Cada um provado sozinho, e o par
//! errado.
//!
//! Um teste de unidade não alcança isto: ele tem um sinal e um módulo. O que este
//! arquivo tem é o caminho de F01 e F02 inteiro menos o codec — `captura →
//! supressão → portão` — e um oráculo que é a pergunta de quem usa: *o ventilador
//! abre o meu microfone?*

#![allow(clippy::unwrap_used, clippy::expect_used)]

use seele_audio::gate::{
    GateConfig, GateMetrics, GateMode, VoiceGate, ABERTURA_COM_SUPRESSAO_DBFS,
};
use seele_audio::supressao::Supressao;
use seele_audio::FRAME_SAMPLES;

/// Ruído de banda larga determinístico, no nível do piso de uma sala.
///
/// A mesma amplitude da auditoria: 0,01, que são −45 dBFS de RMS. É o piso que
/// `gate::room_tone_does_not_open_the_gate` modela, e é um ventilador.
fn chiado(amplitude: f32, quantas: usize, semente: &mut u32) -> Vec<f32> {
    (0..quantas)
        .map(|_| {
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

fn seno(hz: f32, amplitude: f32, quantas: usize, fase: &mut f32) -> Vec<f32> {
    #[allow(clippy::cast_precision_loss, reason = "contagens pequenas")]
    let passo = 2.0 * std::f32::consts::PI * hz / seele_audio::SAMPLE_RATE_HZ as f32;
    (0..quantas)
        .map(|_| {
            let amostra = amplitude * fase.sin();
            *fase += passo;
            amostra
        })
        .collect()
}

/// O caminho de F01 e F02, menos o codec.
struct Caminho {
    supressao: Supressao,
    portao: VoiceGate,
    limpo: Vec<f32>,
    saiu: Vec<Vec<f32>>,
}

impl Caminho {
    /// Com o padrão do produto: supressão inteira e portão no alvo de F01.
    fn padrao() -> Self {
        Self::com_forca(1.0)
    }

    /// O mesmo, com uma força de supressão escolhida. F02 pede o controle.
    fn com_forca(forca: f32) -> Self {
        Self {
            supressao: Supressao::nova(forca),
            portao: VoiceGate::new(
                GateConfig::automatico(ABERTURA_COM_SUPRESSAO_DBFS),
                GateMode::VoiceActivated,
            ),
            limpo: Vec::new(),
            saiu: Vec::new(),
        }
    }

    /// Um quadro pelo caminho inteiro. Devolve quantos quadros sairiam na rede.
    fn um_quadro(&mut self, bruto: &[f32]) -> usize {
        self.supressao.processar(bruto, &mut self.limpo);
        if self.limpo.is_empty() {
            return 0;
        }
        // **A mesma chamada do laço de captura**, e não `update`: é ela que leva a
        // primeira sílaba junto. Ver `voice.rs`.
        let quadro = std::mem::take(&mut self.limpo);
        self.portao.quadros_a_transmitir(&quadro, &mut self.saiu);
        self.saiu.len()
    }

    fn metricas(&self) -> GateMetrics {
        self.portao.metrics()
    }
}

/// **A03: o ruído que sobra da supressão não abre o portão.**
///
/// A reprodução da auditoria com o oráculo invertido: onde ela mediu 200 quadros
/// abertos, aqui o número que se exige é **zero**.
///
/// O aquecimento de um segundo é o da auditoria, e ele é parte do desenho: o
/// portão mede o ruído antes de decidir com ele, e nos primeiros 200 ms não há
/// medida nenhuma — ali o alvo de F01 vale como limiar, e o ventilador passa. É o
/// preço, está medido abaixo, e é a diferença entre um segundo de sessão e a
/// sessão inteira.
#[test]
fn o_ruido_que_sobra_da_supressao_nao_abre_o_portao() {
    let mut caminho = Caminho::padrao();
    let mut semente = 7;

    // Um segundo de aquecimento: a supressão assenta o piso por raia e o portão
    // enche a janela do ruído.
    let mut abertos_no_aquecimento = 0;
    for _ in 0..50 {
        let bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
        abertos_no_aquecimento += caminho.um_quadro(&bruto);
    }

    // E então os 200 quadros que a auditoria mediu. Quatro segundos de ventilador,
    // e ninguém falando.
    let antes = caminho.metricas();
    for _ in 0..200 {
        let bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
        caminho.um_quadro(&bruto);
    }
    let depois = caminho.metricas();
    let abriu = depois.frames_open - antes.frames_open;

    println!(
        "MEDIDA A03: {abriu}/200 quadros abertos no ruído; limiar em {:.1} dBFS, \
         alvo em {ABERTURA_COM_SUPRESSAO_DBFS:.1}; o aquecimento custou \
         {abertos_no_aquecimento} quadros",
        depois.abertura_efetiva_dbfs
    );

    assert_eq!(
        abriu, 0,
        "o portão abriu {abriu} de 200 quadros de ruído sem fala nenhuma: é o \
         defeito A03 da auditoria, e o sintoma é o canal aberto o dia inteiro pelo \
         ventilador de quem está calado"
    );
    // **E o limiar subiu acima do alvo**, que é o mecanismo: um número fixo não
    // teria para onde ir.
    assert!(
        depois.abertura_efetiva_dbfs > ABERTURA_COM_SUPRESSAO_DBFS,
        "o limiar ficou no alvo de {ABERTURA_COM_SUPRESSAO_DBFS:.1} dBFS: ele não \
         está acompanhando o ruído, e o zero acima foi sorte do sinal"
    );
    // O aquecimento é curto de propósito, e um aquecimento longo seria o defeito de
    // volta em outra roupa. Trinta quadros é o que o desenho custa e não mais: dez
    // até haver medida — `gate::QUADROS_PARA_MEDIR` — e os quinze da sustentação
    // que o portão já tinha aberto. A sustentação não é negociável nem aqui: ela é
    // o que protege o fim de uma frase.
    assert!(
        abertos_no_aquecimento <= 30,
        "o aquecimento deixou passar {abertos_no_aquecimento} quadros: mais que \
         isto é o limiar fixo valendo tempo demais"
    );
}

/// **E fala baixa em cima do mesmo ruído ainda abre, e depois fecha de novo.** F01.
///
/// Sem este par o consumo é trivial: um portão que nunca abre passa no teste
/// anterior. O que F01 pede é o contrário de nunca abrir — ele pede que fala baixa
/// abra, e é por isso que os dois testes vivem no mesmo arquivo.
///
/// A fala é sílaba, e não tom contínuo: rastreador de mínimos não distingue um tom
/// de um segundo de um ventilador — nem este, nem o da supressão. Fala de verdade
/// tem vale entre sílabas, e é do vale que a medida do ruído sai.
///
/// # A terceira parte é a que fecha o argumento
///
/// Depois da fala vem ventilador sozinho outra vez, e o portão tem de **voltar a
/// fechar**. Sem essa parte, um portão preso aberto desde o primeiro quadro
/// passaria por «a fala saiu inteira» sem estar decidindo nada.
#[test]
fn fala_baixa_ainda_abre_o_portao_com_o_limiar_medido() {
    let mut caminho = Caminho::padrao();
    let mut semente = 13;
    let mut fase = 0.0;

    // Um: o mesmo segundo e meio de ventilador sozinho, para o limiar assentar
    // onde o teste anterior o deixou e a sustentação do aquecimento expirar.
    for _ in 0..75 {
        let bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
        caminho.um_quadro(&bruto);
    }
    assert!(
        !caminho.portao.speaking(),
        "o portão nem fechou no ventilador antes da fala: o resto deste teste \
         mediria um portão preso aberto"
    );

    // Dois: alguém falando baixo por cima. Sílabas de 100 ms com 100 ms de pausa, a
    // 0,04 de amplitude — uns 12 dB acima do ruído, que é fala baixa e não alta.
    //
    // As pausas de 100 ms **não** fecham o portão, e é a sustentação de 300 ms
    // fazendo o que F01 pede: o fim de uma palavra não é o fim da frase.
    let antes = caminho.metricas();
    let mut quadros_de_silaba = 0;
    for volta in 0..100 {
        let mut bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
        if (volta / 5) % 2 == 0 {
            quadros_de_silaba += 1;
            for (amostra, tom) in bruto
                .iter_mut()
                .zip(seno(300.0, 0.04, FRAME_SAMPLES, &mut fase))
            {
                *amostra += tom;
            }
        }
        caminho.um_quadro(&bruto);
    }
    let na_fala = caminho.metricas();
    let abriu = na_fala.frames_open - antes.frames_open;

    println!(
        "MEDIDA A03b: {abriu} quadros transmitidos em {quadros_de_silaba} quadros \
         de sílaba; {} aberturas, {} quadros retidos entregues, limiar em {:.1} dBFS",
        na_fala.openings - antes.openings,
        na_fala.quadros_retidos_entregues - antes.quadros_retidos_entregues,
        na_fala.abertura_efetiva_dbfs
    );

    assert!(
        abriu >= quadros_de_silaba,
        "das {quadros_de_silaba} sílabas só {abriu} quadros saíram: o limiar medido \
         está comendo fala baixa, e F01 existe justamente para recuperá-la"
    );
    assert!(
        na_fala.openings > antes.openings,
        "o portão não abriu nenhuma vez na fala"
    );
    // E os quadros retidos apareceram, que é a primeira sílaba de F01 chegando.
    assert!(
        na_fala.quadros_retidos_entregues > antes.quadros_retidos_entregues,
        "nenhum quadro retido foi entregue na abertura: o ataque da primeira \
         consoante está ficando de fora"
    );

    // Três: ventilador sozinho de novo. Ele tem de fechar, e ficar fechado.
    for _ in 0..25 {
        let bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
        caminho.um_quadro(&bruto);
    }
    let fechou_em = caminho.metricas();
    let mut abertos_no_fim = 0;
    for _ in 0..100 {
        let bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
        abertos_no_fim += caminho.um_quadro(&bruto);
    }
    println!(
        "MEDIDA A03c: depois da fala, {abertos_no_fim}/100 quadros abertos no \
         ventilador; a sustentação soltou em {} quadros",
        fechou_em.frames_open - na_fala.frames_open
    );
    assert_eq!(
        abertos_no_fim, 0,
        "meio segundo depois da fala o portão ainda estava aberto no ventilador: a \
         sustentação não solta, e o canal fica aberto o dia inteiro"
    );
}

/// **E com o filtro pela metade também não abre.** F02, e a auditoria pede por nome.
///
/// «Validar forças intermediárias»: o controle de F02 é um número de 0 a 1, não um
/// interruptor, e quem achar a voz «de rádio» vai baixá-lo. Com menos filtro sobra
/// mais ruído, e um limiar que mede o ruído **acompanha** — é o que este teste
/// exige, nas três forças.
///
/// Força zero fica de fora de propósito: sem filtro o padrão do produto volta a ser
/// −42 dBFS, que é outro caminho — ver `voice::configuracao_do_portao`.
#[test]
fn com_o_filtro_pela_metade_o_portao_tambem_fica_fechado() {
    for forca in [0.25_f32, 0.5, 1.0] {
        let mut caminho = Caminho::com_forca(forca);
        let mut semente = 23;
        for _ in 0..75 {
            let bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
            caminho.um_quadro(&bruto);
        }
        let antes = caminho.metricas();
        for _ in 0..100 {
            let bruto = chiado(0.01, FRAME_SAMPLES, &mut semente);
            caminho.um_quadro(&bruto);
        }
        let depois = caminho.metricas();
        let abriu = depois.frames_open - antes.frames_open;
        println!(
            "MEDIDA A03d: força {forca:.2} · {abriu}/100 quadros abertos no ruído; \
             limiar em {:.1} dBFS",
            depois.abertura_efetiva_dbfs
        );
        assert_eq!(
            abriu, 0,
            "com força {forca:.2} o portão abriu {abriu} de 100 quadros de \
             ventilador: o limiar não está acompanhando o ruído que sobra dessa \
             força"
        );
    }
}
