//! Por que o vídeo não aconteceu, sempre com um nome.
//!
//! `specs/02-protocolo.md`: *«Todos os motivos de erro são enumerados»*. Vale
//! aqui por um motivo a mais que na rede: o primeiro destes motivos —
//! [`ErroDeVideo::ModuloDeVideoAusente`] — **não é erro**. É o estado normal de
//! uma máquina onde ninguém compartilhou tela ainda, porque o binário deste
//! produto não vem com codec (§2). Uma `String` de erro ali viraria um alerta
//! vermelho para dizer «falta baixar 1 MB».
//!
//! Daí a consequência de interface que o §2 escreve por extenso: **o botão de
//! compartilhar não pode falhar.** Ou ele não está lá, ou ele explica o que
//! falta. Um botão que erra depois do clique é o defeito que o `Info.plist`
//! deste projeto já guarda uma vez.

use std::path::PathBuf;

/// Tudo o que pode dar errado entre a tela e os bytes que saem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDeVideo {
    /// O módulo do Cisco não está nesta máquina.
    ///
    /// **Estado normal, não falha.** O produto não o distribui: a cobertura de
    /// patente do H.264 acompanha o binário que o Cisco entrega, e embrulhá-lo
    /// no nosso instalador nos poria como distribuidor sem ela (§2). Quem nunca
    /// compartilhar tela nunca baixa nada.
    ///
    /// Quem recebe isto tem de oferecer a busca de ~1 MB **com consentimento na
    /// tela**, dizendo de onde vem — e não fazê-la calado.
    ModuloDeVideoAusente {
        /// As pastas onde se procurou, na ordem em que foram olhadas.
        ///
        /// Vai junto porque a pergunta seguinte de quem depura é sempre essa, e
        /// porque quem escolhe as pastas é a casca: uma lista errada é um
        /// defeito de quem chamou, e este campo é o que o mostra.
        procurado_em: Vec<PathBuf>,
    },

    /// O Cisco não publica módulo para este sistema e esta arquitetura.
    ///
    /// Diferente de [`ErroDeVideo::ModuloDeVideoAusente`], e a diferença é o
    /// que a interface faz: ali há um botão de baixar, aqui não há. Oferecer a
    /// busca de um arquivo que não existe é a interface mentindo.
    SistemaSemModuloPublicado {
        /// O sistema, como o `cfg` deste build o conhece.
        sistema: &'static str,
        /// A arquitetura, como o `cfg` deste build a conhece.
        arquitetura: &'static str,
    },

    /// O arquivo está lá e o sistema não o carregou.
    ///
    /// Arquivo truncado, arquitetura errada, permissão, quarentena. É falha de
    /// verdade, ao contrário das duas de cima.
    ModuloDeVideoIlegivel {
        /// O que se tentou carregar.
        caminho: PathBuf,
        /// O que o sistema disse, cru. Serve a quem depura e não a quem usa.
        motivo: String,
    },

    /// O módulo carregou, e é de outra versão do OpenH264.
    ///
    /// As bindings são geradas do `codec_api.h` de uma versão fixa; um módulo
    /// de outra tem outra tabela virtual, e chamá-la seria escrever memória
    /// alheia. A binding recusa antes disso, e é o comportamento certo.
    ModuloDeVideoDeOutraVersao {
        /// A versão de que as bindings deste build saíram.
        esperada: String,
        /// A versão que o módulo em disco disse ter.
        encontrada: String,
    },

    /// Os bytes baixados não são os bytes fixados.
    ///
    /// O §2 manda fixar e **conferir** o hash, com a mesma postura do ADR 0026.
    /// Um módulo que não bate não é carregado: a cobertura de patente vale para
    /// o binário que o Cisco assina, e um arquivo diferente já não é aquele.
    ModuloDeVideoCorrompido {
        /// O sha256 que este build fixou, em hexadecimal minúsculo.
        esperado: &'static str,
        /// O sha256 do que chegou.
        encontrado: String,
        /// Quantos bytes chegaram. Um número muito menor que o esperado é, quase
        /// sempre, uma página de erro do proxy no lugar do arquivo.
        bytes: usize,
    },

    /// O quadro entregue não tem o tamanho que o codificador foi configurado
    /// para receber.
    ///
    /// Defeito de quem chama, e vale ter nome próprio: a binding devolveria
    /// «invalid input YUV size», que não diz qual dos dois lados errou.
    QuadroDeTamanhoErrado {
        /// O que o codificador espera, em pixels.
        esperado: (usize, usize),
        /// O que chegou, deduzido dos planos entregues.
        recebido: (usize, usize),
    },

    /// Os planos entregues não formam um I420 daquele tamanho.
    ///
    /// Y tem `largura × altura`, U e V têm `⌈largura/2⌉ × ⌈altura/2⌉` cada.
    PlanosInconsistentes {
        /// Quantos bytes cada plano deveria ter, na ordem Y, U, V.
        esperado: (usize, usize, usize),
        /// Quantos bytes cada plano tem.
        recebido: (usize, usize, usize),
    },

    /// O OpenH264 recusou a operação.
    ///
    /// A mensagem crua da biblioteca vai junto porque ela nomeia a função C e um
    /// código numérico: ajuda quem depura e não diz nada a quem usa. Quem
    /// mostra tela para gente traduz isto; não repassa.
    CodecRecusou {
        /// O que se estava tentando fazer, em português.
        operacao: &'static str,
        /// O que a biblioteca disse.
        detalhe: String,
    },
}

impl std::fmt::Display for ErroDeVideo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModuloDeVideoAusente { procurado_em } => write!(
                f,
                "o módulo de vídeo não está nesta máquina ({} pasta(s) olhada(s))",
                procurado_em.len()
            ),
            Self::SistemaSemModuloPublicado {
                sistema,
                arquitetura,
            } => write!(f, "não há módulo de vídeo para {sistema}/{arquitetura}"),
            Self::ModuloDeVideoIlegivel { caminho, motivo } => {
                write!(f, "não consegui carregar {}: {motivo}", caminho.display())
            }
            Self::ModuloDeVideoDeOutraVersao {
                esperada,
                encontrada,
            } => write!(
                f,
                "o módulo de vídeo é {encontrada} e este build fala com {esperada}"
            ),
            Self::ModuloDeVideoCorrompido {
                esperado,
                encontrado,
                bytes,
            } => write!(
                f,
                "o módulo de vídeo não confere: esperava {esperado}, veio {encontrado} ({bytes} bytes)"
            ),
            Self::QuadroDeTamanhoErrado { esperado, recebido } => write!(
                f,
                "o codificador foi armado para {}x{} e recebeu {}x{}",
                esperado.0, esperado.1, recebido.0, recebido.1
            ),
            Self::PlanosInconsistentes { esperado, recebido } => write!(
                f,
                "os planos não formam um I420: esperava {esperado:?} bytes, vieram {recebido:?}"
            ),
            Self::CodecRecusou { operacao, detalhe } => {
                write!(f, "o codec recusou {operacao}: {detalhe}")
            }
        }
    }
}

impl std::error::Error for ErroDeVideo {}
