//! O som de **um** programa, e não o da máquina inteira.
//!
//! # O que isto conserta
//!
//! `laco.rs` abre a saída padrão em modo *loopback* e captura tudo o que a
//! máquina toca. Quando alguém compartilha uma janela, isso manda junto a música
//! do outro monitor, a notificação do e-mail e a chamada que a pessoa deixou
//! aberta atrás — o relato de campo foi direto: *«deve enviar somente o áudio da
//! janela selecionada, não de todo o PC»*.
//!
//! O Windows sabe fazer isso desde a build 20348: `ActivateAudioInterfaceAsync`
//! com `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` captura a **árvore** de um
//! processo. A árvore importa: o navegador toca no processo de renderização, e
//! não naquele dono da janela.
//!
//! No macOS não há nada a fazer — medido em 03/09/2026: o
//! `SCContentFilter::with_window` já entrega só o som do aplicativo da janela.
//! Ver o teste em `seele-video/src/captura/macos.rs`.
//!
//! # Por que não passa pelo `cpal`
//!
//! Esta ativação não tem dispositivo: o caminho é a constante
//! `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`, e os parâmetros vão num
//! `PROPVARIANT` de blob. O `cpal` enumera dispositivos reais e não tem por onde
//! expressar isso.
//!
//! # O que uma sonda mediu antes de este arquivo existir
//!
//! O caminho inteiro foi provado numa máquina Windows 25H2 (build 26200) antes
//! de virar produto, com controle: um processo tocando deu pico 0,342569 e o
//! nosso, mudo, deu 0,000000. A captura isola.
//!
//! E a sonda pagou o preço de uma armadilha que está registrada em
//! [`abrir_cliente`]: um `PROPVARIANT` com blob **mata o processo em silêncio**
//! ao ser destruído.
#![cfg(windows)]
// Todo WASAPI é FFI, e um objeto COM implementado à mão é `unsafe` por
// construção. Ver a folha de lints no `Cargo.toml` desta crate.
#![allow(unsafe_code)]

use std::sync::{Arc, Condvar, Mutex};

use windows::core::{implement, Interface as _, Result as ResultadoCom, GUID};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    WAVEFORMATEX,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

/// A taxa que esta captura pede. A mesma do resto do produto.
const TAXA: u32 = 48_000;
/// Estéreo, que é o que a mistura do Windows entrega.
const CANAIS: u16 = 2;
/// `WAVE_FORMAT_IEEE_FLOAT`. A constante não está no crate `windows`.
const FORMATO_FLOAT: u16 = 3;
/// Quanto o Windows guarda antes de avisar. Um décimo de segundo.
const FOLGA_100NS: i64 = 1_000_000;

/// Quanto esperar pela resposta do COM, em passos de 100 ms.
///
/// A ativação é assíncrona e responde em um passo na prática — a sonda mediu
/// `esperas=0` e `esperas=1`. O limite existe para o caso em que ela **não**
/// responde: sem ele, a linha que abre o som fica parada para sempre e a
/// transmissão nunca começa, sem nada dizer por quê.
const PASSOS_DE_ESPERA: u32 = 50;

/// O que o Windows chama quando a ativação termina.
///
/// Um objeto COM de verdade, porque a API exige um: ela não tem versão síncrona.
/// Ele não faz nada além de acordar quem espera — o resultado é lido depois, com
/// `GetActivateResult`.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Aviso {
    pronto: Arc<(Mutex<bool>, Condvar)>,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for Aviso_Impl {
    fn ActivateCompleted(
        &self,
        _operacao: windows_core::Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> ResultadoCom<()> {
        let (trava, sino) = &*self.pronto;
        // A tranca envenenada não impede de acordar: quem espera tem prazo, e
        // ficar preso aqui seria trocar um erro por uma transmissão que nunca
        // começa.
        if let Ok(mut estado) = trava.lock() {
            *estado = true;
        }
        sino.notify_all();
        Ok(())
    }
}

/// O formato que esta captura declara.
///
/// **Declarado e não negociado:** esta ativação não tem `GetMixFormat`, porque
/// não há dispositivo de onde perguntar. 48 kHz estéreo em float é o que o
/// WASAPI aceita aqui — medido na sonda antes de este arquivo existir.
const fn formato() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: FORMATO_FLOAT,
        nChannels: CANAIS,
        nSamplesPerSec: TAXA,
        wBitsPerSample: 32,
        nBlockAlign: 8,
        nAvgBytesPerSec: TAXA * 8,
        cbSize: 0,
    }
}

/// Abre o cliente de áudio da árvore deste processo.
///
/// # Errors
///
/// Devolve o que o COM respondeu. As causas que importam a quem lê o log: o
/// Windows é anterior à build 20348 e não conhece esta ativação; ou o processo
/// alvo não existe mais.
fn abrir_cliente(processo: u32) -> ResultadoCom<(IAudioClient, IAudioCaptureClient, HANDLE)> {
    let mut parametros = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: processo,
                // **A árvore, e não o processo.** O navegador toca no processo
                // de renderização, e não naquele dono da janela; capturar só o
                // pai devolveria silêncio para o caso mais comum de todos.
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };

    // **`ManuallyDrop`, e sem ele o processo morre em silêncio.**
    //
    // O `Drop` do `PROPVARIANT` chama `PropVariantClear`, que para um `VT_BLOB`
    // libera `pBlobData` como memória do COM. Aqui ele aponta para uma variável
    // da pilha, e liberá-la derruba o processo **depois de tudo ter dado certo**
    // — sem erro, sem pânico e sem uma linha de log. Foi assim que a sonda que
    // provou este caminho parou logo após o `Start`, e a única pista era a
    // ausência da linha seguinte.
    let propriedades = std::mem::ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: u32::try_from(std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>())
                            .unwrap_or(0),
                        pBlobData: (&raw mut parametros).cast(),
                    },
                },
            }),
        },
    });

    let pronto = Arc::new((Mutex::new(false), Condvar::new()));
    let aviso: IActivateAudioInterfaceCompletionHandler = Aviso {
        pronto: Arc::clone(&pronto),
    }
    .into();

    // SAFETY: `parametros` e `propriedades` vivem até o fim desta função, e a
    // espera abaixo garante que o COM terminou de lê-los antes disso.
    // O `IID` numa variável, e não `&raw const IAudioClient::IID`: aquilo é uma
    // constante associada, e tomar o endereço dela é tomar o de um temporário
    // que morre na mesma expressão.
    let identidade: GUID = IAudioClient::IID;
    let operacao = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &raw const identidade,
            Some(&*propriedades),
            &aviso,
        )
    }?;

    esperar(&pronto);

    let mut resultado = windows::core::HRESULT(0);
    let mut objeto: Option<windows::core::IUnknown> = None;
    // SAFETY: os dois ponteiros apontam para variáveis desta pilha, vivas até o
    // fim da chamada.
    unsafe {
        operacao.GetActivateResult(&raw mut resultado, &raw mut objeto)?;
    }
    resultado.ok()?;
    let Some(objeto) = objeto else {
        return Err(windows::core::Error::from_hresult(resultado));
    };
    let cliente: IAudioClient = objeto.cast()?;

    let onda = formato();
    // SAFETY: `onda` vive até o fim da chamada, e o cliente acabou de ser criado.
    unsafe {
        cliente.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            FOLGA_100NS,
            0,
            &raw const onda,
            None,
        )?;
    }

    // SAFETY: o evento é fechado quando a captura para; até lá ele é do cliente.
    let evento = unsafe { CreateEventW(None, false, false, None) }?;
    // SAFETY: `evento` acabou de ser criado e é válido.
    unsafe {
        cliente.SetEventHandle(evento)?;
    }
    // SAFETY: o cliente está inicializado.
    let captura: IAudioCaptureClient = unsafe { cliente.GetService() }?;
    // SAFETY: idem.
    unsafe {
        cliente.Start()?;
    }
    Ok((cliente, captura, evento))
}

/// Espera a ativação responder, com prazo.
fn esperar(pronto: &Arc<(Mutex<bool>, Condvar)>) {
    let (trava, sino) = &**pronto;
    let Ok(mut estado) = trava.lock() else {
        return;
    };
    let mut passos = 0;
    while !*estado && passos < PASSOS_DE_ESPERA {
        let Ok((novo, _)) = sino.wait_timeout(estado, std::time::Duration::from_millis(100)) else {
            return;
        };
        estado = novo;
        passos += 1;
    }
}

/// Lê o que houver e devolve as amostras, já em mono.
///
/// Mono porque é o que o resto do caminho carrega, e a mistura é a média dos
/// canais — somar dobraria o volume aparente de um som centralizado.
///
/// # Errors
///
/// Devolve o que o COM respondeu ao ler o buffer.
fn tomar(captura: &IAudioCaptureClient) -> ResultadoCom<Vec<f32>> {
    let mut saida = Vec::new();
    loop {
        let mut dados = std::ptr::null_mut();
        let mut quadros = 0_u32;
        let mut bandeiras = 0_u32;
        // SAFETY: os três ponteiros são de variáveis desta pilha, e o buffer
        // devolvido é liberado antes da volta do laço.
        let leu = unsafe {
            captura.GetBuffer(
                &raw mut dados,
                &raw mut quadros,
                &raw mut bandeiras,
                None,
                None,
            )
        };
        if leu.is_err() || quadros == 0 {
            return Ok(saida);
        }
        let total = quadros as usize * CANAIS as usize;
        // SAFETY: o WASAPI garante `quadros × canais` amostras de `f32` neste
        // ponteiro, no formato que `Initialize` aceitou.
        let amostras = unsafe { std::slice::from_raw_parts(dados.cast::<f32>(), total) };
        saida.extend(amostras.chunks_exact(CANAIS as usize).map(|par| {
            let soma: f32 = par.iter().sum();
            soma / f32::from(CANAIS)
        }));
        // SAFETY: `quadros` é o que o `GetBuffer` acabou de devolver.
        unsafe {
            let _ = captura.ReleaseBuffer(quadros);
        }
    }
}

/// Espera até haver som, ou até o prazo.
///
/// Devolve `false` quando o prazo venceu sem o Windows avisar — que é o estado
/// normal de um programa que não está tocando nada, e não um erro.
fn houve_som(evento: HANDLE, prazo_ms: u32) -> bool {
    // SAFETY: `evento` veio de `CreateEventW` e ainda não foi fechado.
    unsafe { WaitForSingleObject(evento, prazo_ms) == WAIT_OBJECT_0 }
}

/// A captura do som de um programa, aberta e correndo.
///
/// # Por que uma linha própria
///
/// **Os objetos do COM não atravessam linhas, e esta captura precisa
/// atravessar.** Ela nasce na linha que abre a transmissão e é lida na do
/// codificador — `seele_core::video` exige `Send` disto, e exige por medida: o
/// par existe justamente porque a captura de imagem e a de som vivem em linhas
/// diferentes.
///
/// Então o `IAudioClient` fica onde nasceu, numa linha só dele, e o que
/// atravessa é um anel de amostras. É a mesma forma que o resto do áudio desta
/// casa já usa — a `FilaDeSom` do macOS e o `capture_path` do `cpal` — e ela
/// resolve de brinde o outro problema: a leitura do WASAPI **bloqueia** esperando
/// o evento, e bloquear no laço do codificador seguraria o quadro seguinte.
pub struct SomDoPrograma {
    anel: Arc<Mutex<std::collections::VecDeque<f32>>>,
    parar: Arc<std::sync::atomic::AtomicBool>,
}

/// Quantas amostras o anel guarda antes de descartar as mais velhas.
///
/// Meio segundo. Mais que isso é som que já não serve — quem assiste prefere o
/// silêncio de agora ao que aconteceu há um segundo, e insistir nele afasta o
/// som da imagem cada vez mais. É a mesma razão que a fila do macOS escreve.
const FOLGA_DO_ANEL: usize = TAXA as usize / 2;

impl SomDoPrograma {
    /// Abre a captura da árvore deste processo.
    ///
    /// # Errors
    ///
    /// [`String`] com o que o Windows respondeu. Quem chama trata a falha caindo
    /// para o som da máquina — pior que o som só da janela, e melhor que o
    /// silêncio.
    pub fn abrir(processo: u32) -> Result<Self, String> {
        let anel = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let parar = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (conta, ouve) = std::sync::mpsc::channel();

        let anel_da_linha = Arc::clone(&anel);
        let parar_na_linha = Arc::clone(&parar);
        std::thread::Builder::new()
            .name("som-do-programa".to_owned())
            .spawn(move || {
                // A abertura acontece **aqui**, e é por isso que o resultado dela
                // volta por um canal: os objetos criados nesta linha não podem
                // sair dela.
                let aberto = abrir_cliente(processo).map_err(|erro| {
                    format!(
                        "não abri o som do processo {processo}: {erro}. \
                         Esta captura existe a partir do Windows 10 build 20348."
                    )
                });
                let (cliente, captura, evento) = match aberto {
                    Ok(partes) => {
                        let _ = conta.send(Ok(()));
                        partes
                    }
                    Err(motivo) => {
                        let _ = conta.send(Err(motivo));
                        return;
                    }
                };

                while !parar_na_linha.load(std::sync::atomic::Ordering::Relaxed) {
                    // O prazo existe para a parada ser notada: sem ele, uma
                    // captura de um programa mudo ficaria presa no `Wait` para
                    // sempre, e a linha não morreria nunca.
                    if !houve_som(evento, 100) {
                        continue;
                    }
                    let Ok(amostras) = tomar(&captura) else {
                        continue;
                    };
                    let Ok(mut anel) = anel_da_linha.lock() else {
                        break;
                    };
                    anel.extend(amostras);
                    while anel.len() > FOLGA_DO_ANEL {
                        anel.pop_front();
                    }
                }

                // SAFETY: os dois vieram desta linha e ainda não foram
                // encerrados.
                unsafe {
                    let _ = cliente.Stop();
                    let _ = windows::Win32::Foundation::CloseHandle(evento);
                }
            })
            .map_err(|erro| format!("não criei a linha do som do programa: {erro}"))?;

        match ouve.recv() {
            Ok(Ok(())) => Ok(Self { anel, parar }),
            Ok(Err(motivo)) => Err(motivo),
            Err(_) => Err("a linha do som do programa morreu antes de responder".to_owned()),
        }
    }

    /// A taxa em que as amostras saem.
    #[must_use]
    pub const fn taxa(&self) -> u32 {
        TAXA
    }

    /// Tira do anel o que o programa tocou, até `teto` amostras.
    ///
    /// Uma lista vazia é o silêncio de um programa que não está tocando nada — e
    /// não uma falha.
    #[must_use]
    pub fn tomar(&self, teto: usize) -> Vec<f32> {
        let Ok(mut anel) = self.anel.lock() else {
            return Vec::new();
        };
        let quantas = anel.len().min(teto);
        anel.drain(..quantas).collect()
    }
}

impl Drop for SomDoPrograma {
    fn drop(&mut self) {
        // A linha vê a bandeira no próximo prazo do `Wait` — cem milissegundos —
        // e encerra o cliente lá, que é onde ele foi criado.
        self.parar.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
