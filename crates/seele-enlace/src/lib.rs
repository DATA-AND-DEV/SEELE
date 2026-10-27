//! **O transporte da 0.17**: WebRTC sobre um socket UDP só (contrato da fase 2, D1 a D4).
//!
//! Uma [`Ponta`] é dona de um socket e de todas as conexões que passam por ele: as
//! que chegam (o anfitrião, ICE completo controlado e DTLS passivo; ICE-lite até
//! 26/10/2026) e as que ela disca (quem liga, controlling e DTLS ativo). Cada
//! [`Conexao`] tem:
//!
//! - o `ctl` (id 0), ordenado e confiável: o fluxo de controle de sempre;
//! - a `voz` (id 1), sem ordem e sem retransmissão: um datagrama de voz por mensagem;
//! - os canais extras "u" e "b", por DCEP com `protocol` `"seele/2"`: os fluxos
//!   unidirecionais e bidirecionais que o QUIC dava.
//!
//! Todo canal que não é a voz leva o envelope de [`envelope`]. Os limites (canais,
//! filas, mensagens) e os tempos (aperto, ocioso, keepalive) moram aqui, e não no
//! consentimento ICE.
//!
//! **Nada é global.** O provedor de cripto vai por instância (`cripto.rs`), e
//! duas pontas no mesmo processo não sabem uma da outra: um app com três
//! sessões e um servidor hospedado é isso.
//!
//! A decisão é a do ADR 0063 (`docs/adr/0063-a-linha-2-fala-webrtc.md`). O fio
//! que o navegador espelha está aqui, em cada módulo: o envelope em
//! [`envelope`], os canais, o escalonador e os portões do anfitrião em `ponta.rs`,
//! e a confirmação de entrega em [`Envio::entregue`].

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

mod conexao;
mod credencial;
mod cripto;
mod dtls;
pub mod envelope;
mod ponta;
pub mod teste;

pub use conexao::{Conexao, Contadores, Envio, FalhaDoCanal, Leitura, Motivo, Recebe, Recusa};
pub use credencial::{credencial_valida, sortear_credencial, CREDENCIAL_LEN};
pub use dtls::CertificadoDtls;
pub use ponta::{
    ConfigDoAnfitriao, ContadoresDaPonta, Discagem, FalhaAoDiscar, Pino, Ponta, BUFFER_DE_ENVIO,
    BUFFER_DE_RECEPCAO, CANDIDATOS_POR_FURO, CRIACOES_POR_IP, ESPERA_MAXIMA, ESPERA_PELO_DTLS,
    FALHAS_ANTES_DA_ESPERA, FUROS_POR_MINUTO, INDICACOES_POR_CANDIDATO, JANELA_DE_CRIACAO, LAPIDE,
    LIMIAR_DE_VOLUME, MAX_LAPIDES,
};

use std::time::Duration;

/// O corpo máximo de uma mensagem de dados (`0x00`).
pub const MAX_DADOS: usize = 16 * 1024;
/// A mensagem SCTP máxima num canal com envelope: a tag e o corpo.
pub const MAX_MENSAGEM: usize = MAX_DADOS + 1;
/// O datagrama de voz máximo. Abaixo do MTU de 1200 do DTLS com folga, para
/// não fragmentar no SCTP: uma voz em dois pedaços se perde inteira por um.
pub const MAX_VOZ: usize = 1100;
/// O corpo máximo da apresentação (`0x03`).
pub const MAX_APRESENTACAO: usize = 512;
/// Canais abertos por conexão, contando o `ctl` e a `voz`.
pub const MAX_CANAIS: usize = 64;
/// Bytes esperando leitura num canal antes de ele ser cortado.
pub const MAX_FILA_DE_ENTRADA: usize = 1024 * 1024;
/// Bytes que um canal pode ter a caminho do SCTP antes de `write_all` esperar.
///
/// 64 KiB, pela medida do spike C: o SCTP do str0m é uma fila só, e com 256 KiB a
/// voz esperava +11 ms a 200 Mbit/s; com 64 KiB, +2,2 ms e vazão maior.
pub const CREDITO_DE_SAIDA: usize = 64 * 1024;
/// O teto do crédito de recepção somado por conexão (`0x04`), nos canais de
/// volume que este lado lê: fila não lida mais crédito concedido e não usado.
/// O `ctl` fica fora. Política local, igual à do navegador; não muda o fio.
pub const MAX_CREDITO_DA_CONEXAO: usize = 8 * 1024 * 1024;
/// O buffer de saída do SCTP, somado sobre os canais de uma conexão.
pub const BUFFER_DO_SCTP: usize = 1024 * 1024;
/// Datagramas de voz esperando leitura antes de o mais novo ser descartado.
pub const FILA_DE_VOZ: usize = 256;
/// Conexões que chegaram e ainda não terminaram o aperto, no total.
pub const MAX_PENDENTES: usize = 64;
/// Conexões pendentes por IP.
pub const MAX_PENDENTES_POR_IP: usize = 8;

/// O keepalive da voz: uma mensagem de um byte só, `0x00`. Um datagrama de voz
/// de verdade começa pelo byte de versão, que é pelo menos 1.
pub const KEEPALIVE_DA_VOZ: [u8; 1] = [0x00];

/// Corte por violação do protocolo do enlace.
pub const CORTE_VIOLACAO: u32 = 1;
/// Corte por fila de entrada estourada.
pub const CORTE_FILA: u32 = 2;
/// Corte de um canal de tipo desconhecido, ou acima do teto de canais.
pub const CORTE_CANAL_RECUSADO: u32 = 3;

/// Os tempos do enlace: os mesmos números de `seele_proto::transport`.
pub const APERTO: Duration = seele_proto::transport::HANDSHAKE_TIMEOUT;
/// Sem nenhum pacote do par por este tempo, a conexão cai.
pub const OCIOSO: Duration = seele_proto::transport::IDLE_TIMEOUT;
/// Sem nada saindo por este tempo, sai um keepalive.
pub const KEEPALIVE: Duration = seele_proto::transport::KEEPALIVE;

/// O `protocol` do DCEP de todo canal extra.
pub const PROTOCOLO_DOS_CANAIS: &str = seele_proto::transport::PROTOCOLO_DO_CANAL;
/// O label do canal unidirecional.
pub const LABEL_UNI: &str = seele_proto::transport::LABEL_UNI;
/// O label do canal bidirecional.
pub const LABEL_BI: &str = seele_proto::transport::LABEL_BI;
/// O label do controle.
pub const LABEL_CTL: &str = seele_proto::transport::LABEL_CTL;
/// O label da voz.
pub const LABEL_VOZ: &str = seele_proto::transport::LABEL_VOZ;
