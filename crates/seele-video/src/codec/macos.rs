//! O codificador do sistema, no macOS: VideoToolbox.
//!
//! # Por que este módulo existe
//!
//! Quem transmite a tela é, quase sempre, quem está jogando, e o custo de
//! codificar sai do mesmo processador que desenha o jogo. Do outro lado a conta
//! já está resolvida e nunca foi decidida aqui: quem assiste entrega os bytes ao
//! decodificador do sistema pela janela, que é acelerado por GPU. O
//! desequilíbrio é só de quem transmite, e é o que faz «várias pessoas
//! transmitindo ao mesmo tempo» ser uma pergunta sem resposta boa.
//!
//! # O `unsafe` daqui
//!
//! É o único lugar do produto que chama C do sistema, e o ADR 0041 registra a
//! exceção que o permite: `seele-video` declara o próprio bloco de lints com
//! `unsafe_code = "deny"`, e o `allow` está **nesta linha e em nenhuma outra**.
//! Em qualquer outro arquivo deste crate o compilador continua recusando.
//!
//! # Uma sessão síncrona sobre uma API assíncrona
//!
//! O VideoToolbox devolve quadros por retorno de chamada, em ordem de
//! decodificação, quando quiser. A costura [`super::CodificaVideo`] é síncrona:
//! entra um quadro, sai `Option<QuadroCodificado>`. Conciliar as duas custa uma
//! decisão, e ela é forçar o fim do quadro depois de cada entrega.
//!
//! Isso custa a folga que o encoder teria para trabalhar em paralelo, e é o
//! preço certo aqui: sem reordenação — que já está desligada — a folga seria de
//! um quadro, e o que se compra com ela é uma fila a mais entre a captura e o
//! fio. O §1 já decidiu essa mesma troca para a imagem: quadro velho entregue
//! tarde é pior que quadro perdido.
#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use objc2_core_foundation::{CFArray, CFNumber, CFRetained, CFType};
use objc2_core_media::{
    CMBlockBuffer, CMSampleBuffer, CMTime, CMVideoCodecType, CMVideoFormatDescription,
};
use objc2_core_video::{CVImageBuffer, CVPixelBuffer, CVPixelBufferLockFlags};
use objc2_video_toolbox::{
    kVTCompressionPropertyKey_AllowFrameReordering, kVTCompressionPropertyKey_AverageBitRate,
    kVTCompressionPropertyKey_DataRateLimits, kVTCompressionPropertyKey_ExpectedFrameRate,
    kVTCompressionPropertyKey_MaxKeyFrameInterval, kVTCompressionPropertyKey_RealTime,
    kVTEncodeFrameOptionKey_ForceKeyFrame, VTCompressionSession, VTEncodeInfoFlags,
};

use super::{Cadencia, QuadroCodificado, QuadroI420, Resolucao};
use crate::ErroDeVideo;

/// `'y420'`: I420 em três planos, que é o formato em que a captura entrega.
///
/// Escrito como número e não como constante da binding porque a binding não a
/// publica. Os quatro bytes são o código de quatro caracteres da Apple, e estão
/// aqui em vez de num `u32` mágico para que quem ler saiba o que leu.
const FORMATO_I420: u32 = u32::from_be_bytes(*b"y420");

/// H.264, para o `codec_type` da sessão.
const H264: CMVideoCodecType = u32::from_be_bytes(*b"avc1");

/// A fila que o retorno de chamada enche e o [`Codificador`] esvazia.
type Fila = Arc<Mutex<VecDeque<QuadroCodificado>>>;

/// O codificador por hardware do macOS.
///
/// Não é `Sync`, pela mesma razão que o de software: dois lados codificando na
/// mesma sessão embaralhariam a predição.
#[derive(Debug)]
pub struct Codificador {
    sessao: Sessao,
    saida: Fila,
    /// A ponta que o retorno de chamada recebe. Solta no [`Drop`], depois de a
    /// sessão ser invalidada — nessa ordem, ou o retorno de chamada leria uma
    /// fila já liberada.
    ponta: *const Mutex<VecDeque<QuadroCodificado>>,
    resolucao: Resolucao,
    cadencia: Cadencia,
    quadros_por_segundo: u32,
    teto_bps: u32,
    /// O carimbo do próximo quadro, em quadros desde o início.
    contador: i64,
}

/// A sessão, embrulhada para poder atravessar a fronteira de thread.
///
/// O §2 manda o codificador morar numa thread própria, e a bomba a cria. O
/// `CFRetained` não é `Send` por si — o objc2 não afirma isso de tipo nenhum do
/// CoreFoundation —, e a sessão do VideoToolbox de fato pode ser usada de
/// qualquer thread desde que **uma por vez**, que é o que `&mut self` garante em
/// toda a costura. O embrulho existe para essa afirmação ter um lugar com nome,
/// em vez de aparecer como um `unsafe impl` solto no meio do arquivo.
#[derive(Debug)]
struct Sessao(CFRetained<VTCompressionSession>);

// SAFETY: a Apple documenta a sessão como utilizável de qualquer thread; o que
// ela não admite é uso concorrente, e nenhum caminho daqui a compartilha — o
// `Codificador` não é `Sync` e todos os métodos que a tocam pedem `&mut self`.
unsafe impl Send for Sessao {}

// SAFETY: a ponta é só um endereço; quem a desreferencia é o retorno de chamada,
// na thread do VideoToolbox, e ela é mantida viva pelo `Arc` que este mesmo
// `Codificador` guarda. Sem isto o tipo inteiro deixa de ser `Send` por causa do
// ponteiro cru, e a bomba não consegue mais criá-lo na thread dela.
unsafe impl Send for Codificador {}

impl Drop for Codificador {
    fn drop(&mut self) {
        // **A ordem importa e é a única coisa perigosa aqui.** Invalidar primeiro
        // garante que nenhum retorno de chamada está em voo; só depois a
        // referência que ele usaria pode ser solta.
        unsafe { self.sessao.0.invalidate() };
        // SAFETY: a ponta veio de um `Arc::into_raw` no construtor, foi usada só
        // como referência emprestada pelo retorno de chamada, e a sessão acabou
        // de ser invalidada.
        drop(unsafe { Arc::from_raw(self.ponta) });
    }
}

/// O que o VideoToolbox chama quando um quadro fica pronto.
///
/// Roda numa thread do sistema. Tudo o que ela faz é converter e enfileirar: um
/// retorno de chamada que fizesse mais seguraria o encoder.
unsafe extern "C-unwind" fn pronto(
    refcon: *mut c_void,
    _fonte: *mut c_void,
    estado: i32,
    _flags: VTEncodeInfoFlags,
    amostra: *mut CMSampleBuffer,
) {
    if estado != 0 || amostra.is_null() || refcon.is_null() {
        return;
    }
    // SAFETY: `refcon` é a ponta que o construtor passou, e ela vive até o
    // `Drop`, que só solta depois de invalidar a sessão.
    let fila = unsafe { &*(refcon.cast::<Mutex<VecDeque<QuadroCodificado>>>()) };
    // SAFETY: o VideoToolbox entrega uma referência válida durante a chamada.
    let amostra = unsafe { &*amostra };

    let Some(quadro) = annex_b(amostra) else {
        return;
    };
    if let Ok(mut fila) = fila.lock() {
        fila.push_back(quadro);
    }
}

/// Converte o que a sessão devolve para o que o fio carrega.
///
/// O VideoToolbox entrega **AVCC**: cada NAL precedido do próprio tamanho em
/// quatro bytes. O fio deste produto é **Annex-B**: cada NAL precedido de
/// `00 00 00 01`. E num quadro-chave o SPS e o PPS não estão nos dados — moram
/// na descrição de formato, e sem eles nenhum decodificador abre o fluxo. É a
/// mesma decisão que o caminho de software toma, e o teste de ida-e-volta é
/// quem prova as duas.
fn annex_b(amostra: &CMSampleBuffer) -> Option<QuadroCodificado> {
    const INICIO: [u8; 4] = [0, 0, 0, 1];

    // SAFETY: leituras de acessores de um `CMSampleBuffer` emprestado.
    let bloco: CFRetained<CMBlockBuffer> = unsafe { amostra.data_buffer() }?;
    let tamanho = unsafe { bloco.data_length() };
    let mut avcc = vec![0_u8; tamanho];
    // SAFETY: `avcc` tem exatamente `tamanho` bytes, que é o comprimento que o
    // bloco acabou de declarar.
    let copiou = unsafe {
        bloco.copy_data_bytes(
            0,
            tamanho,
            NonNull::new(avcc.as_mut_ptr().cast::<c_void>())?,
        )
    };
    if copiou != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(tamanho + 128);
    let mut chave = false;
    let mut i = 0_usize;
    while i + 4 <= avcc.len() {
        // `try_into` e não índices: a garantia do tamanho mora no `get` e não
        // no tipo, e a folha de lints desta casa recusa indexação que só um
        // argumento sustenta. É a mesma escolha que `entregar_som` faz no
        // macOS, pela mesma razão.
        let quatro: [u8; 4] = avcc.get(i..i + 4)?.try_into().ok()?;
        let n = u32::from_be_bytes(quatro) as usize;
        let corpo = avcc.get(i + 4..i + 4 + n)?;
        // O tipo do NAL são os cinco bits baixos do primeiro byte. 5 é IDR, e é
        // o que faz deste um quadro por onde alguém pode entrar.
        if corpo.first().is_some_and(|b| b & 0x1F == 5) {
            chave = true;
        }
        bytes.extend_from_slice(&INICIO);
        bytes.extend_from_slice(corpo);
        i += 4 + n;
    }

    if chave {
        // SAFETY: acessor de um `CMSampleBuffer` emprestado.
        if let Some(formato) = unsafe { amostra.format_description() } {
            let mut cabeca = Vec::with_capacity(128);
            for indice in 0..2_usize {
                if let Some(conjunto) = conjunto_de_parametros(&formato, indice) {
                    cabeca.extend_from_slice(&INICIO);
                    cabeca.extend_from_slice(&conjunto);
                }
            }
            cabeca.extend_from_slice(&bytes);
            bytes = cabeca;
        }
    }

    (!bytes.is_empty()).then_some(QuadroCodificado { chave, bytes })
}

/// O SPS (índice 0) ou o PPS (índice 1) da descrição de formato.
fn conjunto_de_parametros(formato: &CMVideoFormatDescription, indice: usize) -> Option<Vec<u8>> {
    let mut ponteiro: *const u8 = std::ptr::null();
    let mut tamanho: usize = 0;
    // SAFETY: os dois destinos são locais válidos, e os dois `None` dizem ao
    // sistema que não queremos a contagem nem o tamanho do prefixo.
    let estado = unsafe {
        objc2_core_media::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            formato,
            indice,
            &raw mut ponteiro,
            &raw mut tamanho,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if estado != 0 || ponteiro.is_null() || tamanho == 0 {
        return None;
    }
    // SAFETY: o sistema acabou de garantir `tamanho` bytes legíveis em
    // `ponteiro`, válidos enquanto a descrição de formato viver — e ela vive
    // até o fim desta função.
    Some(unsafe { std::slice::from_raw_parts(ponteiro, tamanho) }.to_vec())
}

/// O **limite duro** de banda, que é diferente do alvo médio.
///
/// `AverageBitRate` é uma média que o VideoToolbox persegue ao longo do tempo, e
/// não um teto: com conteúdo duro — degradê, borda de texto e ruído juntos — ele
/// estoura. Medido: 5,66 Mbps sob um teto declarado de 2,00, **283% do
/// orçamento**.
///
/// Isso não é um detalhe de qualidade, é o transporte sendo enganado. Toda a
/// conta do §3 — o caminho de quem hospeda dividido pelos espectadores — parte
/// do princípio de que o codificador respeita o número que recebeu. Recebendo o
/// triplo, o fluxo congestiona, perde, e a imagem chega pixelada. É o relato de
/// campo: «está mais pixelado que antes», logo depois de o codec do sistema
/// entrar.
///
/// `DataRateLimits` é um par — quantos bytes, em quantos segundos — e é o único
/// que o VideoToolbox trata como limite de verdade. Uma janela de um segundo:
/// mais curta faria o codificador engasgar em cada quadro-chave, que legitimamente
/// custa mais que a média; mais longa deixaria o estouro durar tempo demais para
/// o balde do transporte absorver.
///
/// # Errors
///
/// [`ErroDeVideo::CodecRecusou`] se a sessão recusar o par.
fn limitar(sessao: &VTCompressionSession, teto_bps: u32) -> Result<(), ErroDeVideo> {
    let bytes_por_segundo = i64::from(teto_bps) / 8;
    let limite = CFArray::from_retained_objects(&[
        CFNumber::new_i64(bytes_por_segundo),
        CFNumber::new_i64(1),
    ]);
    // SAFETY: a chave é estática da biblioteca e o vetor vive até o fim daqui.
    unsafe {
        ajustar(
            sessao,
            kVTCompressionPropertyKey_DataRateLimits,
            limite.as_ref(),
            "declarar o limite duro de banda",
        )
    }
}

/// Uma propriedade da sessão, com o erro em português quando o sistema recusa.
fn ajustar(
    sessao: &VTCompressionSession,
    chave: &objc2_core_foundation::CFString,
    valor: &CFType,
    operacao: &'static str,
) -> Result<(), ErroDeVideo> {
    // SAFETY: chave e valor são referências vivas; a sessão é nossa.
    let estado =
        unsafe { objc2_video_toolbox::VTSessionSetProperty(sessao.as_ref(), chave, Some(valor)) };
    if estado == 0 {
        return Ok(());
    }
    Err(ErroDeVideo::CodecRecusou {
        operacao,
        detalhe: format!("o VideoToolbox devolveu {estado}"),
    })
}

impl Codificador {
    /// Arma a sessão do sistema para a escolha de quem compartilha.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] quando o sistema não cria a sessão ou
    /// recusa uma das propriedades. Quem chama **cai para o software**: uma
    /// máquina sem suporte, ou um driver que recusa esta resolução, não pode
    /// custar o compartilhamento inteiro. Ver [`super::armar`].
    pub fn novo(config: &super::ConfigDoCodificador) -> Result<Self, ErroDeVideo> {
        let resolucao = config.resolucao;
        let saida: Fila = Arc::new(Mutex::new(VecDeque::new()));
        let ponta = Arc::into_raw(Arc::clone(&saida));

        let mut crua: *mut VTCompressionSession = std::ptr::null_mut();
        // SAFETY: o destino é um local válido; a ponta vive até o `Drop`, que a
        // solta só depois de invalidar a sessão.
        let estado = unsafe {
            VTCompressionSession::create(
                None,
                i32::try_from(resolucao.largura()).unwrap_or(i32::MAX),
                i32::try_from(resolucao.altura()).unwrap_or(i32::MAX),
                H264,
                None,
                None,
                None,
                Some(pronto),
                ponta.cast::<c_void>().cast_mut(),
                NonNull::from(&mut crua),
            )
        };
        let Some(crua) = NonNull::new(crua).filter(|_| estado == 0) else {
            // A ponta não pode vazar quando a sessão não nasce.
            // SAFETY: veio de `Arc::into_raw` logo acima e ninguém mais a viu.
            drop(unsafe { Arc::from_raw(ponta) });
            return Err(ErroDeVideo::CodecRecusou {
                operacao: "criar a sessão do VideoToolbox",
                detalhe: format!("VTCompressionSessionCreate devolveu {estado}"),
            });
        };
        // SAFETY: `VTCompressionSessionCreate` devolve a sessão já retida, e é
        // essa retenção que este `CFRetained` passa a possuir.
        let sessao = Sessao(unsafe { CFRetained::from_raw(crua) });

        let quadros = config.cadencia.hz();
        let teto_bps = config.teto_bps;

        // **Tempo real ligado, reordenação desligada.** As duas dizem a mesma
        // coisa por caminhos diferentes: esta é uma conversa, não um arquivo.
        // Reordenar produz quadros B, que só decodificam depois do que vem
        // adiante — latência trocada por bits, que é o avesso do que se quer.
        let sim = objc2_core_foundation::CFBoolean::new(true);
        let nao = objc2_core_foundation::CFBoolean::new(false);
        // SAFETY: as chaves são estáticas da própria biblioteca.
        unsafe {
            ajustar(
                &sessao.0,
                kVTCompressionPropertyKey_RealTime,
                sim.as_ref(),
                "ligar o tempo real",
            )?;
            ajustar(
                &sessao.0,
                kVTCompressionPropertyKey_AllowFrameReordering,
                nao.as_ref(),
                "desligar a reordenação de quadros",
            )?;
            ajustar(
                &sessao.0,
                kVTCompressionPropertyKey_ExpectedFrameRate,
                CFNumber::new_i32(i32::try_from(quadros).unwrap_or(30)).as_ref(),
                "declarar a cadência esperada",
            )?;
            // Um quadro-chave por conta própria a cada dois segundos. O
            // transporte pede os que precisa; este é o piso que faz quem chega
            // atrasado não esperar para sempre se ninguém pedir.
            ajustar(
                &sessao.0,
                kVTCompressionPropertyKey_MaxKeyFrameInterval,
                CFNumber::new_i32(i32::try_from(quadros.saturating_mul(2)).unwrap_or(60)).as_ref(),
                "declarar o intervalo máximo entre quadros-chave",
            )?;
            ajustar(
                &sessao.0,
                kVTCompressionPropertyKey_AverageBitRate,
                CFNumber::new_i32(i32::try_from(teto_bps).unwrap_or(i32::MAX)).as_ref(),
                "declarar o teto de banda",
            )?;
            limitar(&sessao.0, teto_bps)?;
        }

        Ok(Self {
            sessao,
            saida,
            ponta,
            resolucao,
            cadencia: config.cadencia,
            quadros_por_segundo: quadros,
            teto_bps,
            contador: 0,
        })
    }
}

impl Codificador {
    /// Monta o `CVPixelBuffer` que a sessão aceita, a partir do nosso I420.
    ///
    /// Copiar plano a plano, e não em bloco: o CoreVideo escolhe o passo de cada
    /// linha e ele **não** é a largura — alinhamento de 16 ou 64 bytes é o
    /// comum. Copiar em bloco produziria uma imagem enviesada, que é o defeito
    /// que parece corrupção de rede e não é.
    fn empacotar(&self, quadro: &QuadroI420) -> Result<CFRetained<CVPixelBuffer>, ErroDeVideo> {
        let recusa = |detalhe: String| ErroDeVideo::CodecRecusou {
            operacao: "montar o quadro para o VideoToolbox",
            detalhe,
        };

        let largura = self.resolucao.largura();
        let altura = self.resolucao.altura();
        let mut cru: *mut CVPixelBuffer = std::ptr::null_mut();
        // SAFETY: o destino é um local válido e o formato é o de três planos.
        let estado = unsafe {
            objc2_core_video::CVPixelBufferCreate(
                None,
                largura,
                altura,
                FORMATO_I420,
                None,
                NonNull::from(&mut cru),
            )
        };
        let Some(cru) = NonNull::new(cru).filter(|_| estado == 0) else {
            return Err(recusa(format!("CVPixelBufferCreate devolveu {estado}")));
        };
        // SAFETY: `CVPixelBufferCreate` devolve o buffer já retido.
        let destino = unsafe { CFRetained::from_raw(cru) };

        // SAFETY: o buffer é nosso e ninguém mais o toca entre o trinco e a
        // soltura logo abaixo.
        let trancou = unsafe {
            objc2_core_video::CVPixelBufferLockBaseAddress(&destino, CVPixelBufferLockFlags(0))
        };
        if trancou != 0 {
            return Err(recusa(format!("não tranquei o quadro ({trancou})")));
        }

        let planos: [(&[u8], usize, usize); 3] = [
            (quadro.luma(), largura, altura),
            (quadro.croma_u(), largura.div_ceil(2), altura.div_ceil(2)),
            (quadro.croma_v(), largura.div_ceil(2), altura.div_ceil(2)),
        ];
        for (indice, (origem, largura_do_plano, altura_do_plano)) in planos.iter().enumerate() {
            let base = objc2_core_video::CVPixelBufferGetBaseAddressOfPlane(&destino, indice);
            let passo = objc2_core_video::CVPixelBufferGetBytesPerRowOfPlane(&destino, indice);
            if base.is_null() || passo < *largura_do_plano {
                // SAFETY: trancado logo acima.
                unsafe {
                    objc2_core_video::CVPixelBufferUnlockBaseAddress(
                        &destino,
                        CVPixelBufferLockFlags(0),
                    );
                }
                return Err(recusa(format!(
                    "o plano {indice} veio com passo {passo} para largura {largura_do_plano}"
                )));
            }
            for linha in 0..*altura_do_plano {
                let Some(fatia) =
                    origem.get(linha * largura_do_plano..(linha + 1) * largura_do_plano)
                else {
                    // SAFETY: trancado logo acima.
                    unsafe {
                        objc2_core_video::CVPixelBufferUnlockBaseAddress(
                            &destino,
                            CVPixelBufferLockFlags(0),
                        );
                    }
                    return Err(recusa(format!(
                        "o plano {indice} do quadro acabou na linha {linha}"
                    )));
                };
                // SAFETY: `base` tem `passo * altura_do_plano` bytes, a linha
                // cabe no passo — conferido acima —, e origem e destino não se
                // sobrepõem: um é nosso `Vec`, o outro é do CoreVideo.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        fatia.as_ptr(),
                        base.cast::<u8>().add(linha * passo),
                        *largura_do_plano,
                    );
                }
            }
        }

        // SAFETY: trancado no início desta função.
        unsafe {
            objc2_core_video::CVPixelBufferUnlockBaseAddress(&destino, CVPixelBufferLockFlags(0));
        }
        Ok(destino)
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
        // SAFETY: a chave é estática da biblioteca; a sessão é nossa.
        unsafe {
            ajustar(
                &self.sessao.0,
                kVTCompressionPropertyKey_AverageBitRate,
                CFNumber::new_i32(i32::try_from(teto_bps).unwrap_or(i32::MAX)).as_ref(),
                "mudar o teto de banda",
            )?;
        }
        // O limite duro anda junto: mudar só a média deixaria o teto novo valendo
        // para a perseguição e o antigo valendo para o estouro.
        limitar(&self.sessao.0, teto_bps)?;
        self.teto_bps = teto_bps;
        Ok(())
    }

    fn codificar(
        &mut self,
        quadro: &QuadroI420,
        pedido_de_chave: bool,
    ) -> Result<Option<QuadroCodificado>, ErroDeVideo> {
        if quadro.largura() != self.resolucao.largura()
            || quadro.altura() != self.resolucao.altura()
        {
            return Err(ErroDeVideo::QuadroDeTamanhoErrado {
                esperado: (self.resolucao.largura(), self.resolucao.altura()),
                recebido: (quadro.largura(), quadro.altura()),
            });
        }

        let imagem = self.empacotar(quadro)?;
        let escala = i32::try_from(self.quadros_por_segundo).unwrap_or(30);
        // SAFETY: `CMTime::new` é `unsafe` porque uma escala zero produz um
        // tempo inválido que o CoreMedia não valida. A escala aqui vem de
        // `Cadencia::hz`, que é sempre positiva, e o `unwrap_or(30)` cobre o
        // impossível sem deixar um zero passar.
        let (carimbo, duracao) =
            unsafe { (CMTime::new(self.contador, escala), CMTime::new(1, escala)) };
        self.contador = self.contador.saturating_add(1);

        // O pedido de quadro-chave viaja por quadro, e não por propriedade da
        // sessão: é uma ordem para **este** quadro, e uma propriedade valeria
        // para todos os seguintes.
        // O tipo apagado é o que a assinatura de `encode_frame` pede: ela
        // recebe o dicionário sem parâmetros, porque as chaves dele não são de
        // um tipo só do lado do sistema.
        let forcar = pedido_de_chave.then(|| {
            let sim: &CFType = objc2_core_foundation::CFBoolean::new(true).as_ref();
            objc2_core_foundation::CFDictionary::<objc2_core_foundation::CFString, CFType>::from_slices(
                &[unsafe { kVTEncodeFrameOptionKey_ForceKeyFrame }],
                &[sim],
            )
        });
        // `as_opaque` porque `encode_frame` recebe o dicionário sem parâmetros
        // de tipo: do lado do sistema as chaves não são de um tipo só.
        let forcar = forcar
            .as_deref()
            .map(objc2_core_foundation::CFDictionary::as_opaque);

        let mut flags = VTEncodeInfoFlags::empty();
        // SAFETY: a imagem vive até o fim desta função, e `complete_frames`
        // abaixo garante que a sessão terminou de usá-la antes disso.
        let estado = unsafe {
            self.sessao.0.encode_frame(
                imagem.as_ref() as &CVImageBuffer,
                carimbo,
                duracao,
                forcar,
                std::ptr::null_mut(),
                &raw mut flags,
            )
        };
        if estado != 0 {
            return Err(ErroDeVideo::CodecRecusou {
                operacao: "codificar um quadro",
                detalhe: format!("VTCompressionSessionEncodeFrame devolveu {estado}"),
            });
        }

        // Ver o cabeçalho deste módulo: é aqui que a API assíncrona vira a
        // costura síncrona.
        // SAFETY: a sessão é nossa e nenhum outro caminho a toca.
        let estado = unsafe {
            self.sessao
                .0
                .complete_frames(objc2_core_media::kCMTimeInvalid)
        };
        if estado != 0 {
            return Err(ErroDeVideo::CodecRecusou {
                operacao: "fechar o quadro",
                detalhe: format!("VTCompressionSessionCompleteFrames devolveu {estado}"),
            });
        }

        let Ok(mut fila) = self.saida.lock() else {
            return Ok(None);
        };
        Ok(fila.pop_front())
    }
}

#[cfg(test)]
mod testes {
    use super::super::{CodificaVideo as _, ConfigDoCodificador};
    use super::*;

    /// Um quadro com bordas, que é o conteúdo caro de uma tela de trabalho.
    ///
    /// Preto chapado sairia com trinta bytes e não provaria que os planos foram
    /// copiados com o passo certo — que é o defeito que este empacotamento pode
    /// ter e que parece corrupção de rede.
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

    /// Os NALs de um fluxo Annex-B, pelos seus tipos.
    fn tipos(bytes: &[u8]) -> Vec<u8> {
        let mut achados = Vec::new();
        let mut i = 0;
        while i + 4 < bytes.len() {
            if bytes.get(i..i + 4) == Some(&[0, 0, 0, 1]) {
                if let Some(cabeca) = bytes.get(i + 4) {
                    achados.push(cabeca & 0x1F);
                }
                i += 4;
            } else {
                i += 1;
            }
        }
        achados
    }

    /// O primeiro quadro sai completo: SPS, PPS e IDR, em Annex-B.
    ///
    /// É a prova que o teste de ida-e-volta do software faz para o OpenH264,
    /// feita aqui sem depender do módulo do Cisco. Um quadro-chave que saísse
    /// sem SPS e PPS na frente produziria um fluxo que **nenhum** decodificador
    /// abre — e o sintoma seria uma tela preta do outro lado, sem erro de
    /// nenhum dos dois lados.
    #[test]
    fn o_primeiro_quadro_sai_com_sps_pps_e_idr() {
        let resolucao = Resolucao::P720;
        let mut codificador = match Codificador::novo(&config(resolucao)) {
            Ok(codificador) => codificador,
            Err(erro) => {
                eprintln!("PULADO: este Mac não deu uma sessão do VideoToolbox ({erro}).");
                return;
            }
        };

        let saiu = codificador
            .codificar(&quadro(resolucao, 0), true)
            .expect("codificar o primeiro quadro")
            .expect("o primeiro quadro não pode ser pulado pelo controle de taxa");

        assert!(saiu.chave, "o primeiro quadro não veio marcado como chave");
        assert_eq!(
            saiu.bytes.get(..4),
            Some(&[0, 0, 0, 1][..]),
            "o fluxo não começa com um código de início Annex-B"
        );
        let tipos = tipos(&saiu.bytes);
        for (tipo, nome) in [(7_u8, "SPS"), (8, "PPS"), (5, "IDR")] {
            assert!(
                tipos.contains(&tipo),
                "o quadro-chave saiu sem {nome} (tipo {tipo}); os tipos foram {tipos:?}"
            );
        }
        assert!(
            tipos.iter().position(|t| *t == 7) < tipos.iter().position(|t| *t == 5),
            "o SPS veio depois do IDR; nessa ordem o decodificador não abre o fluxo"
        );
    }

    /// Um quadro depois do primeiro é comum, e menor.
    ///
    /// As duas coisas juntas provam que a sessão tem estado: se cada quadro
    /// saísse como chave, o custo de banda seria o de uma sequência de
    /// quadros-chave — que é o que o §3.3 gasta 446 ms de orçamento para
    /// espalhar, uma vez.
    #[test]
    fn o_segundo_quadro_e_comum_e_menor() {
        let resolucao = Resolucao::P720;
        let mut codificador = match Codificador::novo(&config(resolucao)) {
            Ok(codificador) => codificador,
            Err(erro) => {
                eprintln!("PULADO: este Mac não deu uma sessão do VideoToolbox ({erro}).");
                return;
            }
        };

        let chave = codificador
            .codificar(&quadro(resolucao, 0), true)
            .expect("codificar a chave")
            .expect("a chave não pode ser pulada");
        let comum = codificador
            .codificar(&quadro(resolucao, 1), false)
            .expect("codificar o comum")
            .expect("o segundo quadro não pode ser pulado");

        assert!(!comum.chave, "o segundo quadro veio marcado como chave");
        assert!(
            comum.bytes.len() < chave.bytes.len(),
            "o quadro comum ({} bytes) não é menor que a chave ({} bytes)",
            comum.bytes.len(),
            chave.bytes.len()
        );
    }

    /// Um quadro do tamanho errado é recusado antes de tocar no sistema.
    #[test]
    fn um_quadro_de_outro_tamanho_e_recusado() {
        let Ok(mut codificador) = Codificador::novo(&config(Resolucao::P720)) else {
            eprintln!("PULADO: este Mac não deu uma sessão do VideoToolbox.");
            return;
        };
        let erro = codificador
            .codificar(&quadro(Resolucao::P540, 0), true)
            .expect_err("um quadro de 540p num codificador de 720p tem de ser recusado");
        assert!(matches!(erro, ErroDeVideo::QuadroDeTamanhoErrado { .. }));
    }
}
