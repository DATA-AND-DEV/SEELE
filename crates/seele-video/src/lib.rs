//! Compartilhamento de tela: captura e codec.
//!
//! O crate espelha o desenho de `seele-audio`, que guarda dispositivo e codec
//! no mesmo lugar. Aqui é captura e codec, e o motivo é o mesmo: o que fala com
//! o sistema operacional e o que fala com o codec mudam juntos, e separá-los em
//! dois crates faria uma fronteira que ninguém consegue defender.
//!
//! A decisão inteira está em
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`, e os
//! números que a sustentam em `spikes/tela-no-codec/README.md` — duas máquinas,
//! um Apple M5 Pro e um Ryzen 7 5800X3D.
//!
//! # O que este crate **não** faz
//!
//! **Não baixa nada.** O binário deste produto não vem com codec, e isso é a
//! licença do H.264 e não empacotamento: a cobertura de patente do Cisco
//! acompanha o binário que **o Cisco** entrega, e redistribuí-lo dentro do
//! nosso `.dmg` ou do nosso instalador nos poria como distribuidor sem a
//! cobertura vir junto (§2). Este crate diz **qual** arquivo buscar, de onde, e
//! com que hash conferir ([`modulo`]); quem busca — depois de perguntar, e a
//! pergunta é obrigatória — é a casca, com a máquina de baixar-e-verificar que
//! o produto já tem (ADR 0026).
//!
//! **Não decide onde os arquivos moram.** [`modulo::procurar_em`] recebe as
//! pastas de quem chama, como `seele_core::conhecidos` e as preferências já
//! fazem. Uma biblioteca que adivinha `~/Library/Application Support` é uma
//! biblioteca impossível de testar, e é também uma biblioteca que decide uma
//! coisa que não é dela.
//!
//! **Não fatia o quadro.** `spikes/tela-no-codec` mediu: quatro fatias em
//! quatro threads dão 2,4× de quadros por 2,5× de CPU — nenhuma eficiência — e
//! sobem os quadros descartados de 16,2% para 23,9%, porque a predição não
//! atravessa fatia. Numa máquina que já entrega dezesseis vezes o necessário,
//! isso é qualidade jogada fora. [`codec`] não oferece a opção.
//!
//! **Não enfileira.** Nem no encoder — `codificar` é síncrono e volta quando
//! termina —, nem antes dele. Quem captura descarta o quadro velho quando o
//! encoder ainda não pegou o anterior (§1). O spike mediu as duas políticas
//! lado a lado: enfileirando, a idade do quadro que sai cresce **sem limite**
//! (1,9 s em oito segundos de corrida); descartando, ela fica em 3 ms.

// `specs/10-convencoes.md`: fora de teste não há `unwrap`/`expect`. Dentro,
// um `expect` com mensagem é mais legível que um `match` que só pode ir para um
// lado, e uma falha ali é falha do teste e não do produto.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod captura;
pub mod codec;
pub mod erro;
pub mod modulo;
pub mod vui;

pub use erro::ErroDeVideo;
pub use modulo::BibliotecaDeVideo;
