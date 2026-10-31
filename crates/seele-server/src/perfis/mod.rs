//! O PERFIS como parte do produto (0.19, `docs/0.19/perfis-oficiais.md`).
//!
//! As imagens do perfil (o envio e a leitura pelo fluxo bidirecional) moram em
//! [`imagens`]; a migração dos perfis do MOD PERFIS, na subida, em [`migracao`].
//! O banco é de `crate::persistence::perfis`.

pub mod imagens;
pub mod migracao;
