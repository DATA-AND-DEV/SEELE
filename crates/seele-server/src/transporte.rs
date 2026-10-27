//! **A sessão sobre o enlace da 0.17** (contrato da fase 2, D1 a D4).
//!
//! Até a 0.16 havia dois transportes no mesmo socket: o QUIC do nativo
//! (`seele/1`) e a WebTransport do navegador (`h3`), e este módulo os juntava
//! num enum. Na 0.17 há um só, e é o mesmo para os dois: WebRTC, pelo
//! [`seele_enlace`]. O nativo e o navegador chegam pela mesma [`Conexao`], com
//! o `ctl` (o fluxo de controle de sempre), a `voz` (os datagramas de sempre) e
//! os canais extras "u" e "b" (os fluxos uni e bidirecionais de sempre).
//!
//! # Por que um módulo, se é só uma reexportação
//!
//! Porque o resto do servidor fala `crate::transporte::{Conexao, Envio,
//! Recebe}`, e é aqui que se lê, num lugar só, o que mudou de forma:
//!
//! - o controle não é «o primeiro bidi que o par abriu»: é [`Conexao::controle`],
//!   que o enlace entrega uma vez, já aberto. **Largar o `Recebe` do `ctl`
//!   fecha a conexão** (é o `02 0` no `ctl`), então a sessão segura os dois
//!   lados enquanto vive;
//! - os códigos de corte e de fechamento são `u32`, e não `VarInt`;
//! - o `stopped()` do QUIC é [`Envio::entregue`];
//! - a [`Leitura`] da conexão não tem perdas nem eventos de congestionamento: o
//!   SCTP do `str0m` não os expõe (ver `crate::tela::LeituraDaConexao`).

/// O que um teste de ponta a ponta (a conformidade) precisa do enlace sem
/// depender dele: o `U` de quem liga, o prazo do aperto e o teto do furo.
pub use seele_enlace::{sortear_credencial, APERTO, INDICACOES_POR_CANDIDATO};
pub use seele_enlace::{
    CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, Envio, FalhaAoDiscar, FalhaDoCanal,
    Leitura, Motivo, Pino, Ponta, Recebe,
};
