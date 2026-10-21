//! **Os compassos da volta do enlace**, num lugar só: o desktop
//! (`seele_ffi::volta`) e o navegador (`seele-web`) pedem o buraco do
//! histórico e repetem as pendentes pelos mesmos números (Tarefa 2.6 da F4).

use std::time::Duration;

/// Quantas mensagens cada página do buraco pede: o teto do servidor
/// (`MAX_PAGE`, em `seele-server`).
pub const PAGINA_DO_BURACO: u16 = 200;

/// Quantas repetições saem de uma vez na volta.
///
/// O balde do servidor deixa 60 quadros de rajada numa conexão nova
/// (`QUADROS_DE_RAJADA`, em `seele-server`), e acima dele **descarta calado**:
/// uma repetição descartada seria a mensagem sumindo de novo, por outra porta.
/// A volta já gasta alguns — a entrada na sala, o `Ping` dela, o canal, o mudo,
/// e duas por página do buraco —, e 20 deixa folga para isso e para quem
/// escreve na hora.
pub const RAJADA_DA_VOLTA: usize = 20;

/// O intervalo entre as repetições depois da rajada: 8 por segundo, menos da
/// metade dos 20 sustentados que o balde deixa (`QUADROS_POR_SEGUNDO`).
pub const INTERVALO_DA_REPETICAO: Duration = Duration::from_millis(125);
