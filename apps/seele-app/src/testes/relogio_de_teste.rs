/// A origem do relógio fixo, a mesma para todo teste do processo.
pub(crate) fn origem() -> std::time::Instant {
    static ORIGEM: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *ORIGEM.get_or_init(std::time::Instant::now)
}
