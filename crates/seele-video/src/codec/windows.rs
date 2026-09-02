//! O codificador do sistema, no Windows: Media Foundation.
//!
//! # Por que este módulo existe
//!
//! O mesmo motivo do irmão de macOS, e agora com número: medido no Mac, o
//! codificador do sistema custa **4,6× menos CPU** que o OpenH264 no mesmo
//! quadro e no mesmo teto — 0,50 ms contra 2,29 ms por quadro de 1080p. Quem
//! transmite é quase sempre quem está jogando, e essa diferença sai do mesmo
//! processador que desenha o jogo.
//!
//! # O `unsafe` daqui
//!
//! ADR 0041. `seele-video` declara o próprio bloco de lints com `deny`, e o
//! `allow` vive nos módulos de plataforma — este e o `macos.rs` — e em nenhum
//! outro arquivo do crate.
//!
//! # O que este código **não** foi
//!
//! **Executado.** Ele compila para `x86_64-pc-windows-msvc` a partir de um Mac,
//! que é o quanto daqui se alcança, e nada substitui rodá-lo numa máquina de
//! verdade. É por isso que a queda de [`super::armar`] importa mais aqui do que
//! no macOS: se qualquer coisa deste caminho falhar, o compartilhamento
//! continua saindo pelo OpenH264 em vez de não sair.
#![allow(unsafe_code)]

use windows::core::{Interface as _, GUID};
use windows::Win32::Media::MediaFoundation::{
    eAVEncCommonRateControlMode_CBR, CODECAPI_AVEncCommonMeanBitRate,
    CODECAPI_AVEncCommonRateControlMode, CODECAPI_AVEncMPVDefaultBPictureCount,
    CODECAPI_AVEncMPVGOPSize, CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVLowLatencyMode,
    ICodecAPI, IMFActivate, IMFMediaEventGenerator, IMFMediaType, IMFSample, IMFTransform,
    METransformHaveOutput, METransformNeedInput, MFCreateMediaType, MFCreateMemoryBuffer,
    MFCreateSample, MFMediaType_Video, MFShutdown, MFStartup, MFTEnumEx, MFVideoFormat_H264,
    MFVideoFormat_NV12, MFVideoInterlace_Progressive, MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS,
    MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_ASYNCMFT, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_END_OF_STREAM, MFT_MESSAGE_NOTIFY_END_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER, MFT_REGISTER_TYPE_INFO,
    MF_E_TRANSFORM_NEED_MORE_INPUT, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC,
    MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
};

use super::{Cadencia, Resolucao};
use crate::ErroDeVideo;

fn recusa(operacao: &'static str, detalhe: impl std::fmt::Display) -> ErroDeVideo {
    ErroDeVideo::CodecRecusou {
        operacao,
        detalhe: detalhe.to_string(),
    }
}

/// Liga o Media Foundation, uma vez.
/// Liga o Media Foundation, uma vez por codificador, e desliga com ele.
///
/// **Era uma vez por processo e nunca desligava**, com a justificativa de que
/// desligar arriscaria derrubar o subsistema debaixo de um codificador nascendo
/// ao mesmo tempo. A justificativa estava errada por desconhecimento meu:
/// `MFStartup` e `MFShutdown` são contados pelo próprio Media Foundation, e a
/// documentação pede exatamente um `MFShutdown` para cada `MFStartup`. Aninhar é
/// o uso previsto, não um risco.
///
/// **E não foi isto que devolveu a memória.** Medido nos dois estados, na mesma
/// máquina: largar o codificador devolve até 219,1 MB sem o par, e 219,3 com
/// ele. Diferença nenhuma. Quem devolveu os 25 MB foi o `ShutdownObject` do
/// ativador, no `Drop`; o subsistema não estava segurando nada.
///
/// Fica assim mesmo assim, porque parear é o contrato: um `MFStartup` sem o
/// `MFShutdown` correspondente é uma dívida que não aparece hoje e que muda de
/// preço quando a Microsoft quiser. Mas fica **registrado que não é otimização**,
/// para ninguém tirar conclusão errada de um número que não mudou.
fn comecar() -> Result<(), ErroDeVideo> {
    // SAFETY: chamada de inicialização do subsistema, sem ponteiros nossos.
    // `MFSTARTUP_NOSOCKET` não serve: o encoder é do subsistema completo.
    unsafe { MFStartup(MF_VERSION, 0) }.map_err(|erro| recusa("iniciar o Media Foundation", erro))
}

/// Procura um codificador de H.264 que aceite NV12, preferindo o de hardware.
///
/// **Hardware primeiro, e a ordem é do sistema.** `MFT_ENUM_FLAG_SORTANDFILTER`
/// manda o próprio Media Foundation ordenar o resultado pelo mérito que ele
/// conhece — e ele conhece melhor que uma lista escrita aqui, que envelheceria a
/// cada driver novo.
///
/// Assíncronos entram na busca porque **quase todo encoder de hardware é
/// assíncrono**: excluí-los seria pedir hardware e aceitar só software. O preço
/// é o laço de eventos em [`Codificador::codificar`].
fn procurar() -> Result<(IMFActivate, IMFTransform), ErroDeVideo> {
    let entrada = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };
    let saida = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };

    let mut achados: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut quantos: u32 = 0;
    // SAFETY: os dois descritores vivem até o fim desta função, e os dois
    // destinos são locais válidos. A lista devolvida é nossa para liberar.
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE
                | MFT_ENUM_FLAG_ASYNCMFT
                | MFT_ENUM_FLAG_SYNCMFT
                | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&entrada),
            Some(&saida),
            &mut achados,
            &mut quantos,
        )
    }
    .map_err(|erro| recusa("procurar um codificador de H.264", erro))?;

    if quantos == 0 || achados.is_null() {
        return Err(recusa(
            "procurar um codificador de H.264",
            "esta máquina não oferece nenhum",
        ));
    }

    // SAFETY: `MFTEnumEx` acabou de garantir `quantos` elementos em `achados`.
    let lista = unsafe { std::slice::from_raw_parts(achados, quantos as usize) };
    let primeiro = lista.first().and_then(Clone::clone);
    // A lista inteira é liberada, inclusive o que não vamos usar: cada elemento
    // é uma referência COM, e o vetor é do alocador do sistema.
    for item in lista {
        drop(item.clone());
    }
    // SAFETY: o ponteiro veio de `MFTEnumEx`, que documenta `CoTaskMemFree`.
    unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(achados.cast())) };

    let ativador =
        primeiro.ok_or_else(|| recusa("procurar um codificador de H.264", "a lista veio vazia"))?;
    // SAFETY: o ativador é o que o sistema acabou de entregar.
    let transformador = unsafe { ativador.ActivateObject::<IMFTransform>() }
        .map_err(|erro| recusa("ativar o codificador de H.264", erro))?;
    // **O ativador vai junto, e não é zelo.** Ver o `Drop`: quem desmonta o que
    // `ActivateObject` montou é `ShutdownObject`, e para chamá-lo é preciso
    // ainda ter o ativador na mão.
    Ok((ativador, transformador))
}

/// Um tipo de mídia com major, subtipo e as medidas que os dois lados pedem.
fn tipo(
    subtipo: GUID,
    resolucao: Resolucao,
    quadros: u32,
    teto_bps: Option<u32>,
) -> Result<IMFMediaType, ErroDeVideo> {
    // SAFETY: cria um objeto novo, sem entrada nossa.
    let tipo =
        unsafe { MFCreateMediaType() }.map_err(|erro| recusa("criar um tipo de mídia", erro))?;

    // Largura e altura num `u64` só, e numerador e denominador da taxa noutro:
    // é a forma que o Media Foundation usa para pares, e escrever à mão é o que
    // evita depender de um utilitário que só existe em C++.
    let medidas = (u64::from(u32::try_from(resolucao.largura()).unwrap_or(u32::MAX)) << 32)
        | u64::from(u32::try_from(resolucao.altura()).unwrap_or(u32::MAX));
    let taxa = (u64::from(quadros) << 32) | 1;

    // SAFETY: todas as chaves são estáticas do sistema e os valores são escalares.
    unsafe {
        tipo.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .and_then(|()| tipo.SetGUID(&MF_MT_SUBTYPE, &subtipo))
            .and_then(|()| tipo.SetUINT64(&MF_MT_FRAME_SIZE, medidas))
            .and_then(|()| tipo.SetUINT64(&MF_MT_FRAME_RATE, taxa))
            .and_then(|()| {
                tipo.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            })
            .and_then(|()| match teto_bps {
                Some(bps) => tipo.SetUINT32(&MF_MT_AVG_BITRATE, bps),
                None => Ok(()),
            })
    }
    .map_err(|erro| recusa("descrever o tipo de mídia", erro))?;
    Ok(tipo)
}

/// O codificador por hardware do Windows.
#[derive(Debug)]
pub struct Codificador {
    /// Quem montou o transformador, guardado para poder desmontá-lo.
    ///
    /// Ver [`Drop`]: soltar a interface do transformador **não** libera o que o
    /// `ActivateObject` alocou, e sem esta referência não há como pedir.
    ativador: IMFActivate,
    transformador: IMFTransform,
    /// Presente quando o transformador é assíncrono, que é o caso da maioria
    /// dos de hardware. É por ele que o laço sabe quando entregar e quando
    /// colher.
    eventos: Option<IMFMediaEventGenerator>,
    resolucao: Resolucao,
    cadencia: Cadencia,
    quadros_por_segundo: u32,
    teto_bps: u32,
    /// O carimbo do próximo quadro, em unidades de 100 ns — a unidade do
    /// Media Foundation.
    contador: i64,
    /// O quadro convertido para NV12, reaproveitado entre chamadas. Alocar 3 MB
    /// por quadro a 60 quadros seria 180 MB por segundo passando pelo alocador.
    nv12: Vec<u8>,
    /// As amostras que vão ao MFT, reaproveitadas em rodízio. Ver
    /// [`Codificador::amostra`] para por que são várias e não uma.
    anel: Vec<IMFSample>,
    /// Qual delas é a vez.
    proxima_do_anel: usize,
}

/// Quantas amostras giram no anel de entrada.
///
/// Quatro: o laço espera a saída de cada quadro antes de entregar o próximo,
/// então o MFT nunca tem mais de um ou dois na mão. O que sobra é margem para um
/// codificador que segure um pouco mais, e o teto de memória fica em quatro
/// quadros — 12 MB a 1080p — em vez de crescer com o tempo de transmissão.
const ANEL: usize = 4;

// SAFETY: as interfaces COM daqui são todas de apartamento livre — o MFT é
// criado sem `CoInitialize` de STA e a documentação do Media Foundation trata
// codificadores como utilizáveis de qualquer thread. O que elas não admitem é
// uso concorrente, e nenhum caminho daqui as compartilha: o `Codificador` não é
// `Sync` e todo método que as toca pede `&mut self`. É a mesma afirmação que
// `macos.rs` faz sobre a sessão do VideoToolbox, e pelo mesmo motivo: o §2 manda
// o codificador morar numa thread própria, e é a bomba que a cria.
unsafe impl Send for Codificador {}

impl Codificador {
    /// Arma o codificador do sistema para a escolha de quem compartilha.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] em qualquer passo. Quem chama **cai para o
    /// software**: ver [`super::armar`].
    pub fn novo(config: &super::ConfigDoCodificador) -> Result<Self, ErroDeVideo> {
        comecar()?;
        let (ativador, transformador) = procurar()?;
        let quadros = config.cadencia.hz();

        // **Assíncrono precisa ser destrancado antes de qualquer outra coisa.**
        //
        // Um MFT de hardware nasce trancado, e o `MF_TRANSFORM_ASYNC_UNLOCK` é o
        // contrato de que quem chama sabe conduzir o laço de eventos. Sem ele
        // todo `ProcessInput` responde `E_NOTIMPL`, que é um erro que não diz
        // nada sobre o que falta.
        // SAFETY: o transformador é o que o sistema acabou de ativar.
        let atributos = unsafe { transformador.GetAttributes() }
            .map_err(|erro| recusa("ler os atributos do codificador", erro))?;
        // SAFETY: chave estática do sistema.
        let assincrono = unsafe { atributos.GetUINT32(&MF_TRANSFORM_ASYNC) }.unwrap_or(0) == 1;
        if assincrono {
            // SAFETY: idem.
            unsafe { atributos.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1) }
                .map_err(|erro| recusa("destrancar o codificador assíncrono", erro))?;
        }

        // **O que o macOS ganhou e este caminho não tinha.**
        //
        // Lá foram declarados tempo real, ausência de reordenação, cadência e
        // intervalo de quadro-chave. Aqui só o teto ia no tipo de saída, e o
        // resto ficava no que o driver achasse — que varia de fabricante para
        // fabricante e é a diferença entre uma conversa e um arquivo. O relato
        // que trouxe isto: «está mais pixelado que antes», no Windows.
        //
        // Nenhum destes é obrigatório e nenhum derruba nada ao falhar: um MFT
        // que não expõe o botão simplesmente não muda de comportamento.
        //
        // Latência baixa **antes** dos tipos, que é o que a documentação do MFT
        // pede: depois de negociado, o modo já não muda.
        botao(&transformador, &CODECAPI_AVLowLatencyMode, 1);
        // Taxa constante e a média igual ao teto. Sem dizer o modo, o padrão de
        // vários encoders é mirar **qualidade** e ignorar o número — e um teto
        // ignorado é uma imagem que não usa a banda que tem.
        botao(
            &transformador,
            &CODECAPI_AVEncCommonRateControlMode,
            eAVEncCommonRateControlMode_CBR.0,
        );
        botao(
            &transformador,
            &CODECAPI_AVEncCommonMeanBitRate,
            i32::try_from(config.teto_bps).unwrap_or(i32::MAX),
        );
        // Zero quadros B, pela mesma razão que o macOS desliga a reordenação:
        // um quadro que só decodifica depois do que vem adiante troca latência
        // por bits, e esta é uma conversa.
        botao(&transformador, &CODECAPI_AVEncMPVDefaultBPictureCount, 0);
        // Um quadro-chave por conta própria a cada dois segundos, como no
        // macOS: é o piso que faz quem chega atrasado não esperar para sempre
        // se ninguém pedir.
        botao(
            &transformador,
            &CODECAPI_AVEncMPVGOPSize,
            i32::try_from(quadros.saturating_mul(2)).unwrap_or(60),
        );

        // **A saída primeiro.** Um codificador não sabe descrever a entrada que
        // aceita antes de saber o que tem de produzir, e a ordem inversa faz o
        // `SetInputType` recusar tudo. Está na documentação do MFT e é o erro
        // mais comum deste caminho.
        let saida = tipo(
            MFVideoFormat_H264,
            config.resolucao,
            quadros,
            Some(config.teto_bps),
        )?;
        // SAFETY: o tipo vive até o fim desta função e o índice 0 é o único
        // fluxo que um codificador de vídeo tem.
        unsafe { transformador.SetOutputType(0, &saida, 0) }
            .map_err(|erro| recusa("declarar a saída do codificador", erro))?;

        let entrada = tipo(MFVideoFormat_NV12, config.resolucao, quadros, None)?;
        // SAFETY: idem.
        unsafe { transformador.SetInputType(0, &entrada, 0) }
            .map_err(|erro| recusa("declarar a entrada do codificador", erro))?;

        // SAFETY: mensagens de ciclo de vida, sem ponteiros nossos.
        unsafe {
            transformador
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .and_then(|()| transformador.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0))
        }
        .map_err(|erro| recusa("iniciar o fluxo do codificador", erro))?;

        let eventos = if assincrono {
            Some(
                transformador
                    .cast::<IMFMediaEventGenerator>()
                    .map_err(|erro| recusa("obter a fila de eventos do codificador", erro))?,
            )
        } else {
            None
        };

        let (largura, altura) = (config.resolucao.largura(), config.resolucao.altura());
        Ok(Self {
            ativador,
            transformador,
            eventos,
            resolucao: config.resolucao,
            cadencia: config.cadencia,
            quadros_por_segundo: quadros,
            teto_bps: config.teto_bps,
            contador: 0,
            anel: Vec::with_capacity(ANEL),
            proxima_do_anel: 0,
            // Y inteiro mais um plano de croma com metade das linhas.
            nv12: vec![0; largura * altura + largura * altura.div_ceil(2)],
        })
    }

    /// Converte o I420 da captura no NV12 que o Media Foundation aceita.
    ///
    /// **É a única conversão que este caminho faz, e ela existe porque os dois
    /// lados discordam.** A captura entrega três planos — Y, U e V separados —
    /// e o codificador do Windows quer dois: Y, e um plano com U e V
    /// **intercalados**. Não há como pedir I420 a ele; NV12 é o formato que todo
    /// encoder de hardware do Windows aceita, e é o que a documentação chama de
    /// preferido.
    ///
    /// O macOS não paga isto: o `CVPixelBuffer` aceita os três planos como
    /// estão.
    fn para_nv12(&mut self, quadro: &super::QuadroI420) -> Result<(), ErroDeVideo> {
        let (largura, altura) = (self.resolucao.largura(), self.resolucao.altura());
        let luma = largura * altura;
        let (meia_largura, meia_altura) = (largura.div_ceil(2), altura.div_ceil(2));

        let y = quadro.luma();
        let u = quadro.croma_u();
        let v = quadro.croma_v();
        if y.len() < luma
            || u.len() < meia_largura * meia_altura
            || v.len() < meia_largura * meia_altura
        {
            return Err(recusa(
                "converter o quadro para NV12",
                "os planos do quadro são menores que a resolução armada",
            ));
        }

        let Some(destino_y) = self.nv12.get_mut(..luma) else {
            return Err(recusa(
                "converter o quadro para NV12",
                "o rascunho encolheu",
            ));
        };
        destino_y.copy_from_slice(y.get(..luma).unwrap_or_default());

        // O croma intercalado, uma amostra de cada por vez.
        for i in 0..meia_largura * meia_altura {
            let (Some(cu), Some(cv)) = (u.get(i), v.get(i)) else {
                break;
            };
            // `split_first_mut` e não índices: o `indexing_slicing` desta
            // casa é `warn`, e um caminho de `unsafe` é o último lugar onde
            // convém ensinar alguém a conviver com aviso.
            let Some([u_destino, v_destino]) = self
                .nv12
                .get_mut(luma + i * 2..luma + i * 2 + 2)
                .and_then(|par| <&mut [u8; 2]>::try_from(par).ok())
            else {
                break;
            };
            *u_destino = *cu;
            *v_destino = *cv;
        }
        Ok(())
    }
}

impl Codificador {
    /// Embrulha o NV12 já convertido numa amostra com carimbo e duração.
    fn amostra(&mut self) -> Result<IMFSample, ErroDeVideo> {
        // **Reaproveitar, e não alocar por quadro.** Aqui iam os 120 MB.
        //
        // Um quadro de 1080p em NV12 são 3 MB. Criar uma amostra e um buffer
        // novos a cada um, a 60 por segundo, são **180 MB por segundo** passando
        // pelo alocador — e o do Windows não devolve isso ao sistema na mesma
        // velocidade em que recebe de volta. O relato: «estou acostumado com o
        // SEELE a 15 MB; compartilhando tela ele vai para 120».
        //
        // Um anel e não uma amostra só: depois do `ProcessInput` o MFT **segura
        // a referência** até terminar com ela, e reescrever o buffer nesse
        // intervalo seria trocar o conteúdo de um quadro que está sendo
        // codificado. Quatro é folga larga para um laço que espera a saída de
        // cada quadro antes de entregar o próximo; o custo teto é 12 MB a 1080p,
        // constante, em vez de crescer sem fim.
        let tamanho = u32::try_from(self.nv12.len()).unwrap_or(u32::MAX);
        let volta = self.proxima_do_anel;
        self.proxima_do_anel = (self.proxima_do_anel + 1) % ANEL;
        if self.anel.len() < ANEL {
            // SAFETY: criam objetos novos, sem entrada nossa.
            let nova =
                unsafe { MFCreateSample() }.map_err(|erro| recusa("criar a amostra", erro))?;
            // SAFETY: idem.
            let buffer = unsafe { MFCreateMemoryBuffer(tamanho) }
                .map_err(|erro| recusa("criar o buffer da amostra", erro))?;
            // SAFETY: a amostra é nossa e ainda não foi entregue a ninguém.
            unsafe { nova.AddBuffer(&buffer) }.map_err(|erro| recusa("montar a amostra", erro))?;
            self.anel.push(nova);
        }
        let Some(amostra) = self.anel.get(volta).cloned() else {
            return Err(recusa("montar a amostra", "o anel de amostras encolheu"));
        };
        // SAFETY: a amostra é do anel e tem exatamente um buffer, posto acima.
        let buffer = unsafe { amostra.GetBufferByIndex(0) }
            .map_err(|erro| recusa("reencontrar o buffer da amostra", erro))?;

        let mut destino: *mut u8 = std::ptr::null_mut();
        let mut cabe: u32 = 0;
        // SAFETY: os dois destinos são locais válidos; o `Unlock` vem antes de
        // qualquer saída desta função.
        unsafe { buffer.Lock(&mut destino, Some(&mut cabe), None) }
            .map_err(|erro| recusa("trancar o buffer da amostra", erro))?;
        if destino.is_null() || (cabe as usize) < self.nv12.len() {
            // SAFETY: trancado logo acima.
            let _ = unsafe { buffer.Unlock() };
            return Err(recusa(
                "trancar o buffer da amostra",
                "o sistema deu menos espaço do que o quadro ocupa",
            ));
        }
        // SAFETY: `cabe` bytes são graváveis em `destino`, o quadro cabe —
        // conferido acima — e as duas regiões não se sobrepõem.
        unsafe { std::ptr::copy_nonoverlapping(self.nv12.as_ptr(), destino, self.nv12.len()) };
        // SAFETY: trancado logo acima.
        unsafe { buffer.Unlock() }.map_err(|erro| recusa("soltar o buffer da amostra", erro))?;
        // SAFETY: o comprimento é o que acabou de ser escrito.
        unsafe { buffer.SetCurrentLength(tamanho) }
            .map_err(|erro| recusa("declarar o tamanho da amostra", erro))?;

        // Carimbo e duração em unidades de 100 ns, que é a unidade do Media
        // Foundation. Sem eles o controle de taxa não tem eixo do tempo e o
        // teto vira sugestão.
        let duracao = 10_000_000_i64 / i64::from(self.quadros_por_segundo.max(1));
        // **Sem `AddBuffer` aqui.** O buffer entra uma vez, quando a amostra
        // nasce; chamá-lo a cada quadro acrescentaria o mesmo buffer de novo à
        // lista dela, e a lista cresceria para sempre — o vazamento que este
        // anel existe para tirar, com outra cara.
        //
        // SAFETY: a amostra é do nosso anel.
        unsafe {
            amostra
                .SetSampleTime(self.contador)
                .and_then(|()| amostra.SetSampleDuration(duracao))
        }
        .map_err(|erro| recusa("montar a amostra", erro))?;
        Ok(amostra)
    }

    /// Tira um quadro pronto, se houver.
    ///
    /// `Ok(None)` quando o codificador ainda precisa de mais entrada — que é a
    /// resposta normal nos primeiros quadros e **não** é erro.
    fn colher(&self) -> Result<Option<super::QuadroCodificado>, ErroDeVideo> {
        let mut saida = [MFT_OUTPUT_DATA_BUFFER::default()];
        let mut estado = 0_u32;
        // SAFETY: o vetor de saída é local e o transformador é nosso. Quando o
        // MFT aloca a amostra — o caso dos de hardware —, ele preenche
        // `pSample`; quando não aloca, `ProcessOutput` devolve o erro de
        // transformação que este ramo trata como «ainda não».
        let resultado = unsafe { self.transformador.ProcessOutput(0, &mut saida, &mut estado) };
        if let Err(erro) = resultado {
            // `MF_E_TRANSFORM_NEED_MORE_INPUT` é o codificador dizendo que ainda
            // não fechou nada. É o caminho normal, e tratá-lo como falha faria
            // o primeiro quadro derrubar a transmissão.
            if erro.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
                return Ok(None);
            }
            return Err(recusa("colher um quadro do codificador", erro));
        }

        // **Tomar a posse, e não clonar.** Aqui vazavam 200 MB.
        //
        // `pSample` e `pEvents` são `ManuallyDrop`: o Rust não os solta, porque
        // quem decide é o contrato do MFT — e num codificador que fornece as
        // próprias amostras, o contrato é que **quem chama passa a responder por
        // elas**. Clonar levava a contagem a dois e soltava um: a referência que
        // o sistema entregou ficava viva para sempre, uma por quadro codificado.
        //
        // O sintoma foi relatado assim: «antes o SEELE nunca passava dos 20 MB,
        // agora está consumindo quase 200 MB». A 60 quadros por segundo, cada
        // amostra segurando o buffer codificado e, num MFT de hardware,
        // possivelmente uma superfície da GPU.
        //
        // SAFETY: `saida` é nosso, local, e é descartado logo abaixo — deixar o
        // `ManuallyDrop` inválido depois de tirar o valor é exatamente o uso que
        // `take` documenta.
        let Some(primeiro) = saida.first_mut() else {
            return Ok(None);
        };
        let recolhida = unsafe { std::mem::ManuallyDrop::take(&mut primeiro.pSample) };
        // A coleção de eventos vem pelo mesmo contrato e quase sempre vazia;
        // soltá-la é a mesma obrigação, e esquecê-la seria o mesmo vazamento em
        // ponto miúdo.
        drop(unsafe { std::mem::ManuallyDrop::take(&mut primeiro.pEvents) });
        let Some(amostra) = recolhida else {
            return Ok(None);
        };
        // SAFETY: a amostra é a que o MFT acabou de entregar.
        let buffer = unsafe { amostra.ConvertToContiguousBuffer() }
            .map_err(|erro| recusa("juntar os pedaços do quadro", erro))?;

        let mut inicio: *mut u8 = std::ptr::null_mut();
        let mut comprimento: u32 = 0;
        // SAFETY: destinos locais; o `Unlock` vem antes de sair.
        unsafe { buffer.Lock(&mut inicio, None, Some(&mut comprimento)) }
            .map_err(|erro| recusa("trancar o quadro pronto", erro))?;
        let bytes = if inicio.is_null() {
            Vec::new()
        } else {
            // SAFETY: o sistema acabou de garantir `comprimento` bytes legíveis.
            unsafe { std::slice::from_raw_parts(inicio, comprimento as usize) }.to_vec()
        };
        // SAFETY: trancado logo acima.
        let _ = unsafe { buffer.Unlock() };

        if bytes.is_empty() {
            return Ok(None);
        }
        // O Media Foundation entrega **Annex-B** para `MFVideoFormat_H264`, com
        // SPS e PPS já na frente do IDR — ao contrário do VideoToolbox, que
        // entrega AVCC e guarda os parâmetros à parte. Aqui não há conversão a
        // fazer; só falta saber se este quadro é porta de entrada.
        let chave = super::nal_e_chave(&bytes);
        Ok(Some(super::QuadroCodificado { chave, bytes }))
    }
}

/// Um botão do `ICodecAPI`, quando o codificador oferece um.
///
/// **Falhar aqui não é erro.** Nem todo MFT expõe `ICodecAPI`, e os que expõem
/// não expõem os mesmos botões: pedir um quadro-chave a quem não atende deve
/// custar um quadro-chave a menos, e não a transmissão inteira.
fn botao(transformador: &IMFTransform, chave: &GUID, valor: i32) {
    let Ok(api) = transformador.cast::<ICodecAPI>() else {
        return;
    };
    let valor = windows::Win32::System::Variant::VARIANT::from(valor);
    // SAFETY: a chave é estática do sistema e o valor vive até o fim da chamada.
    let _ = unsafe { api.SetValue(chave, &raw const valor) };
}

impl Drop for Codificador {
    /// Avisa o transformador de que acabou, antes de soltá-lo.
    ///
    /// **Soltar a interface não é a mesma coisa que encerrar o fluxo.** Um MFT
    /// de hardware segura recursos da GPU entre o `NOTIFY_BEGIN_STREAMING` e o
    /// `NOTIFY_END_STREAMING`, e a contagem de referências do COM não sabe
    /// disso: quem tem de dizer que acabou é quem começou.
    ///
    /// Importa mais do que parece porque este objeto **morre com frequência**.
    /// Trocar de degrau na escada de resolução recomeça captura, codificador e
    /// fluxo juntos — o §3.6 —, e um log de campo mostrou três recomeços em
    /// trinta segundos. Cada sessão abandonada sem aviso é um pedaço de memória
    /// que só volta quando o processo morre.
    ///
    /// Erros são ignorados: já estamos no caminho de desmonte, e não há a quem
    /// contar.
    fn drop(&mut self) {
        // SAFETY: mensagens de ciclo de vida, sem ponteiros nossos, num
        // transformador que ainda é válido — ele só é solto depois deste bloco.
        unsafe {
            let _ = self
                .transformador
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self
                .transformador
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            // **E desmontar o que foi montado.** Medido: sem esta linha, largar
            // o codificador devolvia 12 MB dos 70 que ele custa — o resto ficava
            // no processo até ele morrer. É o relato de campo: «depois que paro
            // de transmitir ele fica em 70~80 MB, sendo que antes ficava em
            // 7~15».
            //
            // A contagem de referências do COM não alcança isto. O objeto nasceu
            // de `IMFActivate::ActivateObject`, e quem desfaz esse par é
            // `ShutdownObject` — soltar a interface do transformador libera a
            // interface, não a sessão que o driver abriu por baixo.
            let _ = self.ativador.ShutdownObject();
            // E o subsistema, uma vez para cada `MFStartup`. Ele é contado pelo
            // próprio Media Foundation: este par fecha o que este codificador
            // abriu, e não o de mais ninguém.
            let _ = MFShutdown();
        }
    }
}

impl Codificador {
    fn botao(&self, chave: &GUID, valor: i32) {
        botao(&self.transformador, chave, valor);
    }
}

impl super::CodificaVideo for Codificador {
    fn resolucao(&self) -> Resolucao {
        self.resolucao
    }

    fn cadencia(&self) -> Cadencia {
        self.cadencia
    }

    fn quadros_por_segundo(&self) -> u32 {
        self.quadros_por_segundo
    }

    fn teto_bps(&self) -> u32 {
        self.teto_bps
    }

    fn ajustar_teto(&mut self, teto_bps: u32) -> Result<(), ErroDeVideo> {
        // Pelo `ICodecAPI` e **não** refazendo o tipo de saída: trocar o tipo no
        // meio do fluxo obriga a renegociar, e renegociar custa um quadro-chave
        // — que é exatamente o que a costura promete que mudar o teto não custa.
        self.botao(
            &CODECAPI_AVEncCommonMeanBitRate,
            i32::try_from(teto_bps).unwrap_or(i32::MAX),
        );
        self.teto_bps = teto_bps;
        Ok(())
    }

    fn codificar(
        &mut self,
        quadro: &super::QuadroI420,
        pedido_de_chave: bool,
    ) -> Result<Option<super::QuadroCodificado>, ErroDeVideo> {
        if quadro.largura() != self.resolucao.largura()
            || quadro.altura() != self.resolucao.altura()
        {
            return Err(ErroDeVideo::QuadroDeTamanhoErrado {
                esperado: (self.resolucao.largura(), self.resolucao.altura()),
                recebido: (quadro.largura(), quadro.altura()),
            });
        }

        self.para_nv12(quadro)?;
        if pedido_de_chave {
            self.botao(&CODECAPI_AVEncVideoForceKeyFrame, 1);
        }
        let amostra = self.amostra()?;
        self.contador = self
            .contador
            .saturating_add(10_000_000 / i64::from(self.quadros_por_segundo.max(1)));

        // **O caminho síncrono é o simples**, e é o que os MFTs de software
        // seguem: entrega, colhe, acabou.
        let Some(eventos) = self.eventos.clone() else {
            // SAFETY: a amostra é nossa e vive até o fim desta função.
            unsafe { self.transformador.ProcessInput(0, &amostra, 0) }
                .map_err(|erro| recusa("entregar um quadro ao codificador", erro))?;
            return self.colher();
        };

        // **E o assíncrono é o laço**, que é o preço de quase todo codificador
        // de hardware do Windows ser assim. O MFT diz quando quer entrada e
        // quando tem saída; nada acontece por iniciativa de quem chama.
        //
        // `GetEvent` sem bandeira **bloqueia**, e é isso que faz este laço
        // terminar: ou o codificador pede o próximo quadro — e aí o nosso já
        // entrou, então não há o que colher agora — ou ele anuncia uma saída.
        let mut entregue = false;
        loop {
            // SAFETY: a fila é do próprio transformador.
            let evento = unsafe { eventos.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0)) }
                .map_err(|erro| recusa("esperar o codificador", erro))?;
            // SAFETY: o evento é o que a fila acabou de entregar.
            let tipo = unsafe { evento.GetType() }
                .map_err(|erro| recusa("ler o evento do codificador", erro))?;

            if tipo == METransformNeedInput.0 as u32 {
                if entregue {
                    // Ele já engoliu o nosso e quer o seguinte: não há quadro
                    // pronto neste tique, e isso não é perda nem erro.
                    return Ok(None);
                }
                // SAFETY: a amostra vive até o fim desta função.
                unsafe { self.transformador.ProcessInput(0, &amostra, 0) }
                    .map_err(|erro| recusa("entregar um quadro ao codificador", erro))?;
                entregue = true;
            } else if tipo == METransformHaveOutput.0 as u32 {
                if let Some(pronto) = self.colher()? {
                    return Ok(Some(pronto));
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::super::{CodificaVideo as _, ConfigDoCodificador, QuadroI420};
    use super::*;

    /// Um quadro com bordas, que é o conteúdo caro de uma tela de trabalho.
    fn quadro(resolucao: Resolucao, passo: usize) -> QuadroI420 {
        let (largura, altura) = (resolucao.largura(), resolucao.altura());
        let mut luma = Vec::with_capacity(largura * altura);
        for linha in 0..altura {
            for coluna in 0..largura {
                let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
                luma.push(if claro { 235 } else { 16 });
            }
        }
        let croma = vec![128_u8; largura.div_ceil(2) * altura.div_ceil(2)];
        QuadroI420::novo(largura, altura, luma, croma.clone(), croma)
            .expect("os planos de um I420 montado aqui")
    }

    fn config(resolucao: Resolucao) -> ConfigDoCodificador {
        ConfigDoCodificador {
            resolucao,
            cadencia: Cadencia::Q30,
            teto_bps: 2_000_000,
        }
    }

    /// O primeiro quadro sai como porta de entrada, em Annex-B.
    ///
    /// A prova irmã da do `macos.rs`, e a única que roda numa máquina de
    /// verdade: este arquivo é compilado a partir de um Mac e **nunca foi
    /// executado** até este teste rodar no runner do CI. Ver o cabeçalho.
    #[test]
    fn o_primeiro_quadro_e_porta_de_entrada() {
        let resolucao = Resolucao::P720;
        let mut codificador = match Codificador::novo(&config(resolucao)) {
            Ok(codificador) => codificador,
            Err(erro) => {
                eprintln!("PULADO: esta máquina não deu um codificador de H.264 ({erro}).");
                return;
            }
        };

        // O codificador pode pedir alguns quadros antes de fechar o primeiro, e
        // isso não é perda: a costura chama `None` de «pulado pelo teto» e quem
        // pediu a chave continua pedindo. Vinte é folga de sobra para qualquer
        // MFT — se não sair nada em vinte, saiu nada.
        let mut primeiro = None;
        for i in 0..20 {
            if let Some(saiu) = codificador
                .codificar(&quadro(resolucao, i), i == 0)
                .expect("codificar")
            {
                primeiro = Some(saiu);
                break;
            }
        }
        let primeiro = primeiro.expect("nenhum quadro saiu em vinte entregas");

        assert!(
            primeiro.chave,
            "o primeiro quadro que saiu não é porta de entrada; quem chegar \
             depois nunca vai conseguir abrir a transmissão"
        );
        assert_eq!(
            primeiro.bytes.get(..4),
            Some(&[0, 0, 0, 1][..]),
            "o Media Foundation devia entregar Annex-B e não entregou; \
             se ele passou a entregar AVCC, este caminho precisa converter \
             como o `macos.rs` converte"
        );
    }

    /// Um quadro de outro tamanho é recusado antes de tocar no sistema.
    #[test]
    fn um_quadro_de_outro_tamanho_e_recusado() {
        let Ok(mut codificador) = Codificador::novo(&config(Resolucao::P720)) else {
            eprintln!("PULADO: esta máquina não deu um codificador de H.264.");
            return;
        };
        let erro = codificador
            .codificar(&quadro(Resolucao::P540, 0), true)
            .expect_err("um quadro de 540p num codificador de 720p tem de ser recusado");
        assert!(matches!(erro, ErroDeVideo::QuadroDeTamanhoErrado { .. }));
    }
}

#[cfg(test)]
mod medida_de_memoria {
    use super::super::{CodificaVideo as _, ConfigDoCodificador, QuadroI420};
    use super::*;

    /// O conjunto de trabalho deste processo, em MB.
    fn memoria_mb() -> f64 {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;

        let mut info = PROCESS_MEMORY_COUNTERS::default();
        let tamanho = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap_or(0);
        // SAFETY: o destino é um local válido do tamanho declarado, e o
        // pseudo-handle do processo atual não precisa ser fechado.
        let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut info, tamanho) };
        if ok.is_err() {
            return 0.0;
        }
        info.WorkingSetSize as f64 / (1024.0 * 1024.0)
    }

    fn quadro(resolucao: Resolucao, passo: usize) -> QuadroI420 {
        let (largura, altura) = (resolucao.largura(), resolucao.altura());
        let mut luma = Vec::with_capacity(largura * altura);
        for linha in 0..altura {
            for coluna in 0..largura {
                luma.push(if ((coluna + passo) / 8 + linha / 12).is_multiple_of(2) {
                    235
                } else {
                    16
                });
            }
        }
        let croma = vec![128_u8; largura.div_ceil(2) * altura.div_ceil(2)];
        QuadroI420::novo(largura, altura, luma, croma.clone(), croma).expect("os planos")
    }

    /// Quanto o codificador custa, e se o custo **cresce**.
    ///
    /// A pergunta veio de campo: «estou acostumado com o SEELE a 15 MB;
    /// compartilhando tela ele vai para 120». Duas coisas muito diferentes podem
    /// produzir isso — um custo fixo de sessão de hardware, que é o preço do
    /// que se está usando, ou um vazamento, que é defeito. Só medir duas
    /// rodadas iguais separa as duas: se a segunda custa como a primeira, vaza.
    ///
    /// Não reprova por número: o custo legítimo de um MFT de hardware varia por
    /// driver e por GPU, e um limiar fixo aqui reprovaria máquina honesta. O que
    /// ele imprime é o que responde.
    #[test]
    fn quanto_o_codificador_segura_de_memoria() {
        let resolucao = Resolucao::P1080;
        let config = ConfigDoCodificador {
            resolucao,
            cadencia: Cadencia::Q60,
            teto_bps: 4_000_000,
        };
        // Os quadros são montados antes de tudo: eles também ocupam memória, e
        // medi-los junto com o codificador confundiria as duas contas.
        let quadros: Vec<QuadroI420> = (0..60).map(|p| quadro(resolucao, p)).collect();
        let antes = memoria_mb();

        let Ok(mut codificador) = Codificador::novo(&config) else {
            eprintln!("PULADO: esta máquina não deu um codificador de H.264.");
            return;
        };
        let armado = memoria_mb();

        let mut marcos = Vec::new();
        for rodada in 0..5 {
            for (i, q) in quadros.iter().enumerate() {
                let _ = codificador.codificar(q, rodada == 0 && i == 0);
            }
            marcos.push(memoria_mb());
        }

        // **E depois de largar.** É a outra metade da pergunta, e a que o
        // relato de campo faz: «depois que paro de transmitir ele fica em 70~80
        // MB, sendo que antes ficava em 7~15. Não deveria voltar ao normal?».
        //
        // Largar o codificador tem de devolver a sessão do MFT. Se o número não
        // cair aqui, ou o `Drop` não está encerrando o que devia, ou o Windows
        // segura as páginas no conjunto de trabalho até haver pressão — e as
        // duas pedem trabalhos diferentes.
        drop(codificador);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let depois_de_largar = memoria_mb();

        eprintln!(
            "MEMÓRIA 1080p60: antes {antes:.1} MB | armado {armado:.1} MB (+{:.1}) | \
             depois de 60, 120, 180, 240, 300 quadros: {} | largado {depois_de_largar:.1} MB",
            armado - antes,
            marcos
                .iter()
                .map(|m| format!("{m:.1}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
}
