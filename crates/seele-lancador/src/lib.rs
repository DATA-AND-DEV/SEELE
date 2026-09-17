//! O núcleo do launcher: resolver, instalar e iniciar versões lado a lado.
//!
//! [ADR 0046](../../../docs/adr/0046-toda-versao-continua-de-pe.md) decidiu que
//! o app vira launcher: **cliente, servidor, protocolo e API de MOD sobem
//! juntos e são identificados pelo mesmo número**, toda versão publicada
//! continua hospedável, e cada uma tem o seu diretório de dados. Este crate é a
//! parte dessa decisão que não tem tela: dado um manifesto de versões e um
//! disco, ele responde qual executável roda, com que dados, e por que uma
//! versão foi recusada.
//!
//! # O que este crate não faz, de propósito
//!
//! - **Não fala com a rede.** Quem busca o manifesto é a casca; aqui entram
//!   bytes. É o que torna cada regra desta pasta testável sem servidor, e é
//!   também o que mantém a promessa do ADR 0026 verificável por leitura: se não
//!   há cliente HTTP aqui dentro, não há consulta escondida no arranque.
//! - **Não desempacota.** Um `.app.tar.gz`, um `.exe` e um `.deb` se instalam
//!   de três jeitos, e nenhum deles é regra de launcher. O que é regra é a
//!   **ordem** — conferir antes de tocar em disco, preparar de lado, e só então
//!   publicar a instalação —, e essa ordem está em [`deposito`], com o
//!   desempacotamento entrando por [`deposito::Desempacotador`].
//! - **Não decide por ninguém.** Descer de versão não leva as conversas junto,
//!   e [`dados`] devolve isso como estado para a tela dizer **antes**, em vez de
//!   a pessoa descobrir pelo silêncio — o defeito que o `CLAUDE.md` chama de
//!   «o produto sabe e não conta».
//!
//! # Idioma
//!
//! Identificadores e documentação em português, como o `seele-instalador`, que
//! é o vizinho deste crate no grafo e no argumento (ADR 0043 e 0045). As
//! **chaves do JSON** ficam em inglês, porque o manifesto é lido por ferramenta
//! e estende o `latest.json` que já existe — ADR 0013.

#[cfg(test)]
mod cenario;

pub mod assinatura;
pub mod dados;
pub mod deposito;
pub mod executavel;
pub mod inicializacao;
pub mod manifesto;
pub mod resolucao;
pub mod revogacao;
pub mod versao;

pub use assinatura::{Chave, FalhaDeAssinatura};
pub use dados::{EstadoDosDados, OrigemDosDados, PlanoDeDados};
pub use deposito::{Deposito, Desempacotador, FalhaAoInstalar, Instalacao};
pub use executavel::{CaminhoDoExecutavel, ExecutavelInvalido};
pub use inicializacao::{Lancamento, VARIAVEL_DE_DADOS};
pub use manifesto::{Manifesto, Pacote, Publicacao, Unidade};
pub use resolucao::{Ato, Pedido, Recusa, Resolvedor, VersaoResolvida};
pub use revogacao::{Envelope, FalhaDaLista, ListaDeRevogacao, Memoria, Revogacao, Vigente};
pub use versao::{IdentificadorInvalido, Versao};
