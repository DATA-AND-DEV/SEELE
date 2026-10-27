//! O certificado DTLS de uma ponta (contrato D3).
//!
//! - No `seeled`, é **o certificado longo P-256 de sempre**, o do banco: a
//!   impressão que o DTLS apresenta é a mesma do `fp=` do convite.
//! - Na sala pessoal e em quem liga, é efêmero, um por processo ou por conexão.

use anyhow::{bail, Context, Result};

/// Um certificado DTLS: o DER e a chave privada em PKCS#8.
#[derive(Clone)]
pub struct CertificadoDtls {
    der: Vec<u8>,
    chave_pkcs8: Vec<u8>,
    impressao: [u8; 32],
}

impl std::fmt::Debug for CertificadoDtls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A chave nunca vai a log.
        f.debug_struct("CertificadoDtls")
            .field(
                "impressao",
                &seele_proto::transport::certificate_fingerprint(&self.der),
            )
            .finish_non_exhaustive()
    }
}

impl CertificadoDtls {
    /// Um certificado que já existe, como o longo do `seeled`.
    ///
    /// # Errors
    ///
    /// Uma chave que não é P-256: o navegador não negocia outra coisa no DTLS,
    /// e a do `seeled` sempre foi P-256 (`tls.rs`).
    pub fn novo(der: Vec<u8>, chave_pkcs8: Vec<u8>) -> Result<Self> {
        let par = rcgen::KeyPair::try_from(chave_pkcs8.as_slice())
            .context("a chave do certificado DTLS não abre como PKCS#8")?;
        if par.algorithm() != &rcgen::PKCS_ECDSA_P256_SHA256 {
            bail!("o certificado DTLS precisa de chave P-256");
        }
        let impressao = seele_proto::transport::certificate_digest(&der);
        Ok(Self {
            der,
            chave_pkcs8,
            impressao,
        })
    }

    /// Um certificado novo, P-256, que vale um ano.
    ///
    /// A validade não é conferida no WebRTC (o par confere a impressão), e o
    /// número de série é sorteado: o Firefox recusa dois certificados com o
    /// mesmo emissor e o mesmo número (o motivo que o `dimpl` registra).
    ///
    /// # Errors
    ///
    /// Se o `rcgen` não conseguir gerar a chave.
    pub fn efemero() -> Result<Self> {
        let par = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .context("não deu para gerar a chave P-256 do DTLS")?;
        let mut parametros = rcgen::CertificateParams::new(Vec::<String>::new())
            .context("parâmetros do certificado DTLS")?;
        let mut nome = rcgen::DistinguishedName::new();
        nome.push(rcgen::DnType::CommonName, "seele");
        parametros.distinguished_name = nome;
        let agora = std::time::SystemTime::now();
        parametros.not_before = (agora - std::time::Duration::from_secs(24 * 60 * 60)).into();
        parametros.not_after = (agora + std::time::Duration::from_secs(365 * 24 * 60 * 60)).into();
        let serie: [u8; 16] = rand::random();
        parametros.serial_number = Some(rcgen::SerialNumber::from_slice(&serie));
        let certificado = parametros
            .self_signed(&par)
            .context("não deu para assinar o certificado DTLS")?;
        Self::novo(certificado.der().to_vec(), par.serialize_der())
    }

    /// O SHA-256 do DER: o `a=fingerprint`, e o `fp` do convite no `seeled`.
    #[must_use]
    pub const fn impressao(&self) -> [u8; 32] {
        self.impressao
    }

    /// O DER.
    #[must_use]
    pub fn der(&self) -> &[u8] {
        &self.der
    }

    pub(crate) fn para_o_str0m(&self) -> str0m::config::DtlsCert {
        str0m::config::DtlsCert {
            certificate: self.der.clone(),
            private_key: self.chave_pkcs8.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_efemero_e_p256_e_a_impressao_e_a_do_der() {
        let certificado = CertificadoDtls::efemero().unwrap();
        assert_eq!(
            certificado.impressao(),
            seele_proto::transport::certificate_digest(certificado.der())
        );
        assert_ne!(
            certificado.impressao(),
            CertificadoDtls::efemero().unwrap().impressao(),
            "dois efêmeros são dois certificados"
        );
    }

    #[test]
    fn uma_chave_que_nao_e_p256_e_recusada() {
        let par = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let der = rcgen::CertificateParams::new(Vec::<String>::new())
            .unwrap()
            .self_signed(&par)
            .unwrap()
            .der()
            .to_vec();
        assert!(CertificadoDtls::novo(der, par.serialize_der()).is_err());
    }
}
