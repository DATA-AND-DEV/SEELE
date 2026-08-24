//! A borda amarela sai, ou não sai?
//!
//! # A pergunta
//!
//! A Windows Graphics Capture desenha um contorno amarelo em volta do que está
//! sendo capturado. `GraphicsCaptureSession::SetIsBorderRequired(false)` o
//! tira, e a documentação da Microsoft amarra isso à capacidade restrita
//! `graphicsCaptureWithoutBorder` num **manifesto de pacote** — que um
//! executável instalado por NSIS não tem.
//!
//! O que a documentação não responde é o que acontece na prática num Windows 11
//! recente, onde a propriedade existe e há um pedido de acesso em tempo de
//! execução. Duas coisas podem acontecer, e elas são opostas para o produto:
//!
//! - a chamada passa, e a transmissão sai sem borda: é uma linha em
//!   `crates/seele-video/src/captura/windows.rs`;
//! - a chamada é recusada, e a captura **não começa**: trocaríamos uma borda
//!   por um botão de compartilhar que falha, que é o defeito que o §2 da spec
//!   proíbe por escrito.
//!
//! Chutar entre as duas foi o que eu fiz da primeira vez, por escrito, no
//! cabeçalho daquele arquivo. Este spike é o conserto disso.
//!
//! # Como ler o resultado
//!
//! Ele imprime três linhas e sai. A que importa é a terceira.

use std::sync::mpsc;
use std::time::Duration;

use windows_capture::capture::{
    CaptureControlError, Context, GraphicsCaptureApiHandler,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

/// Avisa a thread principal no primeiro quadro, e nada mais.
struct PrimeiroQuadro(mpsc::Sender<()>);

impl GraphicsCaptureApiHandler for PrimeiroQuadro {
    type Flags = mpsc::Sender<()>;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(contexto: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self(contexto.flags))
    }

    fn on_frame_arrived(
        &mut self,
        _quadro: &mut Frame,
        controle: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let _ = self.0.send(());
        controle.stop();
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn main() {
    println!(
        "1. a propriedade IsBorderRequired existe neste Windows: {:?}",
        GraphicsCaptureApi::is_border_settings_supported()
    );

    let monitor = match Monitor::primary() {
        Ok(monitor) => monitor,
        Err(erro) => {
            println!("sem monitor para capturar: {erro}");
            return;
        }
    };

    let (avisa, espera) = mpsc::channel();
    let ajustes = Settings::new(
        monitor,
        CursorCaptureSettings::WithCursor,
        // O que este spike existe para exercitar.
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        avisa,
    );

    // `start_free_threaded` e não `start`: este processo não tem laço de
    // mensagens de janela, e a versão bloqueante quer um.
    let controle = match PrimeiroQuadro::start_free_threaded(ajustes) {
        Ok(controle) => controle,
        Err(erro) => {
            println!("2. a captura NÃO começou: {erro:?}");
            println!(
                "3. VEREDITO: a borda NÃO pode ser desligada aqui. Pedir \
                 WithoutBorder troca a borda por um botão que falha."
            );
            return;
        }
    };
    println!("2. a captura começou sem reclamar de permissão");

    let chegou = espera.recv_timeout(Duration::from_secs(10)).is_ok();
    match controle.stop() {
        Ok(()) | Err(CaptureControlError::AlreadyStopped) => {}
        Err(erro) => println!("   (ao parar: {erro:?})"),
    }

    if chegou {
        println!(
            "3. VEREDITO: a borda PODE ser desligada aqui — a sessão aceitou \
             SetIsBorderRequired(false) e entregou quadro."
        );
    } else {
        println!(
            "3. VEREDITO: começou e não entregou quadro em 10 s. Não é resposta: \
             rode de novo com algo se mexendo na tela — a WGC só entrega quando \
             o conteúdo muda."
        );
    }
}
