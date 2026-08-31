//! Captura de tela, uma implementação por sistema.
//!
//! **Este arquivo está vazio de propósito, e vai continuar.** Ele existe para
//! que duas pessoas escrevam `macos` e `windows` ao mesmo tempo sem que nenhuma
//! precise editar `lib.rs` nem esta linha: quem mexe nos dois lados de uma
//! declaração de módulo em paralelo troca trabalho por conflito.
//!
//! # A regra que vale nos dois sistemas, e ela não é negociável
//!
//! **A captura descarta, nunca enfileira.** Se o codificador ainda não pegou o
//! quadro anterior, o novo **substitui** — não entra numa fila. É a mesma
//! decisão que `specs/03-audio.md` tomou para o áudio, pelo mesmo motivo, e
//! `spikes/tela-no-codec` a mediu dos dois jeitos:
//!
//! ```text
//! politica      entregues  descartados  fila final   idade p50  idade pior
//! enfileira          3856            0        1165      958 ms     1870 ms
//! descarta           2999         2023           0        3 ms        9 ms
//! ```
//!
//! Enfileirando, a idade do quadro que sai **cresce sem limite**: 23% de todo o
//! tempo decorrido vira atraso, e nada nesse caminho para de crescer — em oito
//! segundos são 1,9 s, em oitenta seriam dezenove. E a fila final tem 1165
//! quadros, que em 1080p são 3,6 GB de I420 se alguém guardar os pixels.
//! Descartando, a idade fica em 3 ms e não anda.
//!
//! # O que sai daqui
//!
//! [`crate::codec::QuadroI420`], que é o único formato que o OpenH264 aceita.
//! A conversão de espaço de cor é custo da captura e **não está medida** —
//! `spikes/tela-no-codec` mediu o encoder com textura real e movimento
//! sintetizado, e diz na cara que a cadência de uma captura de verdade, com
//! cópia de `IOSurface`, é outra pergunta.
//!
//! # O que não entra aqui
//!
//! **Linux.** Fica fora da v1 por decisão de 22/08/2026 (§7 item 5): o portal
//! XDG exige `ashpd` mais `pipewire`, e com eles o binário do Linux deixa de
//! ser autocontido — que é uma das propriedades que este produto vende. É
//! reversível e está nomeado como pendência, em vez de trocado por baixo.
//!
//! **Câmera.** §0: conteúdo diferente, ajuste de encoder diferente, permissão
//! diferente. Não é subproduto barato de compartilhar tela.

/// A ScreenCaptureKit, que é a única porta desde que o `CGDisplayStream` foi
/// depreciado.
#[cfg(target_os = "macos")]
pub mod macos;

/// A Windows Graphics Capture, que é a única que enxerga janelas com aceleração
/// e composição.
/// A aritmética de reamostragem e cor, que não é de plataforma nenhuma.
///
/// Sob `test` em toda parte: os testes dela não falam de Windows e não têm por
/// que só rodar lá. Ver o cabeçalho do módulo.
#[cfg(any(target_os = "windows", test))]
mod reamostragem;

#[cfg(target_os = "windows")]
pub mod windows;
