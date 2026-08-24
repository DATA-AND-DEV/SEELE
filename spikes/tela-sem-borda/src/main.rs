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

use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
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
        _controle: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        // **Não para no primeiro quadro**, e essa foi a falha do primeiro
        // desenho desta sonda: ela media se a chamada dava erro, e a pergunta
        // é outra — se a borda aparece na tela. Uma captura que dura um quadro
        // acaba antes de qualquer pessoa conseguir olhar.
        let _ = self.0.send(());
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn main() {
    // O controle, e ele é o que separa uma resposta de um palpite.
    //
    // Rodar isto por SSH não dá uma sessão gráfica de verdade, e a WGC precisa
    // de uma: sem estação de janelas ela recusa **qualquer** captura, com ou
    // sem borda. Se a versão com borda falhar igual, a falha é do ambiente e
    // não da permissão — e concluir «não dá para tirar a borda» a partir dela
    // seria o mesmo erro que este spike existe para consertar.
    //
    // `com-borda` roda o controle; sem argumento, roda a pergunta.
    let controle_apenas = std::env::args().nth(1).as_deref() == Some("com-borda");
    let borda = if controle_apenas {
        println!("0. CONTROLE: pedindo a borda padrão, para ver se o ambiente deixa capturar");
        DrawBorderSettings::Default
    } else {
        DrawBorderSettings::WithoutBorder
    };

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
        borda,
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
            if controle_apenas {
                println!(
                    "3. INCONCLUSIVO: nem com a borda padrão este ambiente captura. \
                     A falha é da sessão gráfica, e não da permissão da borda — \
                     rode isto na máquina, com alguém logado na tela."
                );
            } else {
                println!(
                    "3. VEREDITO: a borda NÃO pode ser desligada aqui — **se** o \
                     controle (`com-borda`) tiver capturado. Se ele falhar igual, \
                     isto aqui não diz nada."
                );
            }
            return;
        }
    };
    println!("2. a captura começou sem reclamar de permissão");

    let chegou = espera.recv_timeout(Duration::from_secs(10)).is_ok();
    if chegou {
        println!();
        println!("   >>> OLHE PARA A BORDA DA SUA TELA AGORA <<<");
        println!(
            "   {} — a captura fica de pé por 12 segundos.",
            if controle_apenas {
                "Deve haver um contorno amarelo: é o controle, e ele mostra como a borda é"
            } else {
                "Se NÃO houver contorno amarelo, o pedido pegou"
            }
        );
        for restam in (1..=12).rev() {
            print!("\r   {restam:>2}s ");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            std::thread::sleep(Duration::from_secs(1));
        }
        println!("\r        ");
    }
    // O `on_frame_arrived` já mandou parar, então este `stop` quase sempre
    // encontra a thread encerrada. Reclamar disso seria ruído.
    if let Err(erro) = controle.stop() {
        println!("   (ao parar: {erro:?})");
    }

    if chegou {
        if controle_apenas {
            println!(
                "3. CONTROLE OK: o ambiente captura. O contorno que você acabou de ver \
                 é a borda que queremos tirar."
            );
        } else {
            println!(
                "3. A chamada passou e os quadros vieram. **A resposta é o que você viu**: \
                 sem contorno, dá para tirar a borda; com contorno, o Windows aceitou o \
                 pedido e o ignorou — que é pior que recusar, porque não deixa rastro."
            );
        }
    } else {
        println!(
            "3. VEREDITO: começou e não entregou quadro em 10 s. Não é resposta: \
             rode de novo com algo se mexendo na tela — a WGC só entrega quando \
             o conteúdo muda."
        );
    }
}
