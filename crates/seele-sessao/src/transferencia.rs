//! **O prazo de uma imagem de MOD pelo volume** (Tarefa 2.6 da F4): quanto o
//! fluxo próprio de um `ler_imagem_mod` ou de um `enviar_imagem_mod` espera, e
//! o que o MOD lê quando o prazo acaba. O enlace do `seele-core` e a página do
//! navegador os leem daqui; o texto estava escrito dentro do enlace.

/// Quanto uma transferência de imagem de MOD espera, em segundos: o mesmo do
/// servidor para o fluxo dele (`session.rs`).
pub const PRAZO_DA_IMAGEM_DE_MOD_S: u64 = 60;

/// O que o MOD lê quando o prazo acaba.
pub const PRAZO_ESGOTADO: &str =
    "Prazo da transferência esgotado. Confira a conexão e a versão do servidor.";
