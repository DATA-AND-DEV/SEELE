//! Fronteira de chamadas pessoais para a casca desktop; nenhum segredo sai.
pub use seele_core::chamada_pessoal::{Falha, Intencao, Prova};
use seele_core::tofu::PinStore;
use std::path::Path;

/// Chave pública da identidade que também determina o código e a sala nativa.
///
/// # Errors
/// A identidade não pôde ser aberta.
pub fn publica(home: &str) -> Result<[u8; 32], Falha> {
    super::chave_com(
        home,
        std::env::var(super::VARIAVEL_DA_IDENTIDADE).ok().as_deref(),
    )
    .map(|c| c.verifying_key().to_bytes())
    .map_err(|_| Falha::Armazenamento)
}
/// Assina uma intenção validada sem aceitar bytes arbitrários da webview.
///
/// # Errors
/// Contexto inválido, contato bloqueado ou identidade indisponível.
pub fn assinar(home: &str, intencao: &Intencao, agora: u64, receber: bool) -> Result<Prova, Falha> {
    if let Some(destinatario) = seele_core::chamada_pessoal::sujeito_ao_bloqueio(intencao) {
        let codigo = seele_core::Codigo::da_chave(&destinatario);
        let agenda = seele_core::agenda::Agenda::abrir(Path::new(home).join("agenda"))
            .map_err(|_| Falha::Armazenamento)?;
        if agenda.buscar(&codigo).is_some_and(|c| c.bloqueado) {
            return Err(Falha::Bloqueado);
        }
    }
    let chave = super::chave_com(
        home,
        std::env::var(super::VARIAVEL_DA_IDENTIDADE).ok().as_deref(),
    )
    .map_err(|_| Falha::Armazenamento)?;
    seele_core::chamada_pessoal::assinar(&chave, intencao, agora, receber)
}
/// Confere e guarda o mesmo pino usado pelo transporte pessoal nativo.
///
/// # Errors
/// Código, pino ou acesso incompatível; falha de armazenamento.
pub fn conferir_par(
    home: &str,
    codigo: &str,
    publica: &[u8; 32],
    apelido: Option<&str>,
    direcao: Option<&str>,
    agora: i64,
) -> Result<(), Falha> {
    let lido = seele_core::Codigo::ler(codigo).map_err(|_| Falha::Invalido)?;
    let mut agenda = seele_core::agenda::Agenda::abrir(Path::new(home).join("agenda"))
        .map_err(|_| Falha::Armazenamento)?;
    if agenda.buscar(&lido).is_some_and(|c| c.bloqueado) {
        return Err(Falha::Bloqueado);
    }
    let pins = seele_core::FilePinStore::open(Path::new(home).join("pins"))
        .map_err(|_| Falha::Armazenamento)?;
    let alvo = super::alvo_do_codigo(&lido);
    let impressao =
        seele_core::chamada_pessoal::conferir_par(codigo, publica, pins.pinned(&alvo).as_deref())?;
    pins.pin(&alvo, impressao);
    if pins.falha_de_gravacao().is_some() {
        return Err(Falha::Armazenamento);
    }
    if let Some(direcao) = direcao {
        let direcao = match direcao {
            "liguei" => seele_core::agenda::Direcao::Liguei,
            "recebi" => seele_core::agenda::Direcao::Recebi,
            "perdida" => seele_core::agenda::Direcao::Perdida,
            _ => return Err(Falha::Invalido),
        };
        agenda
            .anotar_ligacao(lido, apelido.unwrap_or(""), direcao, agora)
            .map_err(|_| Falha::Armazenamento)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn o_pin_nativo_e_duravel_e_bloqueio_precede_recepcao() {
        let home =
            std::env::temp_dir().join(format!("seele-pessoal-pin-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&home).unwrap();
        let pasta = home.to_str().unwrap();
        let publica = seele_core::SigningKey::from_bytes(&[51; 32])
            .verifying_key()
            .to_bytes();
        let codigo = seele_core::Codigo::da_chave(&publica).to_string();
        conferir_par(pasta, &codigo, &publica, Some("Alex"), Some("recebi"), 1000).unwrap();
        conferir_par(pasta, &codigo, &publica, None, None, 1001).unwrap();
        let mut agenda = seele_core::agenda::Agenda::abrir(home.join("agenda")).unwrap();
        assert_eq!(agenda.listar()[0].apelido.as_deref(), Some("Alex"));
        agenda
            .bloquear(seele_core::Codigo::ler(&codigo).unwrap(), true)
            .unwrap();
        assert_eq!(
            conferir_par(pasta, &codigo, &publica, None, None, 1002),
            Err(Falha::Bloqueado)
        );
        agenda
            .bloquear(seele_core::Codigo::ler(&codigo).unwrap(), false)
            .unwrap();
        std::fs::write(home.join("pins"), format!("@{codigo} outra-chave\n")).unwrap();
        assert_eq!(
            conferir_par(pasta, &codigo, &publica, None, None, 1003),
            Err(Falha::ChaveNaoConfere)
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
