//! O provedor de cripto do str0m, em Rust puro e sem o AWS-LC.
//!
//! **De onde vem.** É o `str0m-rust-crypto` 0.6.0 (MIT OR Apache-2.0, de Martin
//! Algesten e colaboradores), com duas diferenças:
//!
//! - o `dimpl` entra só com `rust-crypto`. O crate original liga a feature
//!   `rcgen` do `dimpl`, que liga `aws-lc-rs` à força, e com as duas o DTLS
//!   passaria a usar o AWS-LC (ver o `Cargo.toml` deste crate);
//! - o certificado nunca é gerado aqui: quem cria um `Rtc` passa o dele
//!   ([`crate::CertificadoDtls`]). `generate_certificate` devolve `None`.
//!
//! **Sem estado global.** O provedor é passado a cada `Rtc` por
//! `RtcConfig::set_crypto_provider`, e nunca instalado no processo: um app com
//! três sessões e um servidor hospedado tem quatro `Rtc` que não sabem uns dos
//! outros.
//!
//! O SRTP não carrega nada neste produto (não há mídia RTP), mas o str0m deriva
//! as chaves dele de todo aperto DTLS, então as cifras precisam existir.

use std::sync::Arc;
use std::time::Instant;

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm, Nonce};
use ctr::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, Mac};
use str0m_proto::crypto::dtls::ProtocolVersion;
use str0m_proto::crypto::dtls::{DtlsCert, DtlsImplError, DtlsInstance, DtlsOutput, DtlsProvider};
use str0m_proto::crypto::{AeadAes128Gcm, AeadAes128GcmCipher};
use str0m_proto::crypto::{AeadAes256Gcm, AeadAes256GcmCipher, Aes128CmSha1_80Cipher};
use str0m_proto::crypto::{CryptoError, CryptoProvider, DtlsVersion};
use str0m_proto::crypto::{Sha1HmacProvider, Sha256Provider, SrtpProvider};
use str0m_proto::crypto::{SupportedAeadAes128Gcm, SupportedAeadAes256Gcm, SupportedAes128CmSha1_80};

/// O provedor, para um `Rtc`.
pub(crate) fn provedor() -> Arc<CryptoProvider> {
    static SRTP: Srtp = Srtp;
    static SHA1_HMAC: Sha1Hmac = Sha1Hmac;
    static SHA256: Sha256 = Sha256;
    static DTLS: Dtls = Dtls;
    Arc::new(CryptoProvider {
        srtp_provider: &SRTP,
        sha1_hmac_provider: &SHA1_HMAC,
        sha256_provider: &SHA256,
        dtls_provider: &DTLS,
    })
}

#[derive(Debug)]
struct Sha256;

impl Sha256Provider for Sha256 {
    fn sha256(&self, data: &[u8]) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(data).into()
    }
}

#[derive(Debug)]
struct Sha1Hmac;

impl Sha1HmacProvider for Sha1Hmac {
    fn sha1_hmac(&self, key: &[u8], payloads: &[&[u8]]) -> [u8; 20] {
        // O HMAC aceita chave de qualquer tamanho (RFC 2104): `new_from_slice`
        // só falha para MACs de chave fixa. O ramo de erro devolve um MAC que
        // nunca confere, em vez de entrar em pânico com entrada da rede.
        let Ok(mut mac) = <Hmac<sha1::Sha1> as Mac>::new_from_slice(key) else {
            return [0; 20];
        };
        for payload in payloads {
            mac.update(payload);
        }
        mac.finalize().into_bytes().into()
    }
}

#[derive(Debug)]
struct Dtls;

impl DtlsProvider for Dtls {
    fn generate_certificate(&self) -> Option<DtlsCert> {
        None
    }

    fn new_dtls(
        &self,
        cert: &DtlsCert,
        now: Instant,
        dtls_version: DtlsVersion,
        mtu: Option<usize>,
    ) -> Result<Box<dyn DtlsInstance>, CryptoError> {
        let certificado = dimpl::DtlsCertificate {
            certificate: cert.certificate.clone(),
            private_key: cert.private_key.clone(),
        };
        // O ICE já provou o caminho de volta antes do DTLS: o cookie do
        // servidor seria redundante (o mesmo do `str0m-rust-crypto`).
        let mut builder = dimpl::Config::builder().use_server_cookie(false);
        if let Some(mtu) = mtu {
            builder = builder.mtu(mtu);
        }
        let config = builder
            .build()
            .map_err(|erro| CryptoError::Other(format!("dimpl: {erro}")))?;
        let config = Arc::new(config);
        let dtls = match dtls_version {
            DtlsVersion::Dtls12 => dimpl::Dtls::new_12(config, certificado, now),
            DtlsVersion::Dtls13 => dimpl::Dtls::new_13(config, certificado, now),
            DtlsVersion::Auto => dimpl::Dtls::new_auto(config, certificado, now),
            outra => {
                return Err(CryptoError::Other(format!("DTLS sem suporte: {outra}")));
            }
        };
        Ok(Box::new(InstanciaDtls { dtls }))
    }
}

struct InstanciaDtls {
    dtls: dimpl::Dtls,
}

impl std::fmt::Debug for InstanciaDtls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanciaDtls").finish()
    }
}

impl DtlsInstance for InstanciaDtls {
    fn set_active(&mut self, active: bool) {
        self.dtls.set_active(active);
    }

    fn handle_packet(&mut self, packet: &[u8]) -> Result<(), DtlsImplError> {
        self.dtls.handle_packet(packet)
    }

    fn poll_output<'a>(&mut self, buf: &'a mut [u8]) -> DtlsOutput<'a> {
        self.dtls.poll_output(buf)
    }

    fn handle_timeout(&mut self, now: Instant) -> Result<(), DtlsImplError> {
        self.dtls.handle_timeout(now)
    }

    fn send_application_data(&mut self, data: &[u8]) -> Result<(), DtlsImplError> {
        self.dtls.send_application_data(data)
    }

    fn is_active(&self) -> bool {
        self.dtls.is_active()
    }

    fn protocol_version(&self) -> Option<ProtocolVersion> {
        self.dtls.protocol_version()
    }

    fn is_closing(&self) -> bool {
        self.dtls.is_closing()
    }

    fn is_closed(&self) -> bool {
        self.dtls.is_closed()
    }

    fn close(&mut self) -> Result<(), DtlsImplError> {
        self.dtls.close()
    }
}

// ---- SRTP ----------------------------------------------------------------
//
// Os tamanhos vêm do str0m, que chama estas funções com buffers do tamanho
// certo; um tamanho errado é erro (devolvido), e não pânico.

fn copiar(destino: &mut [u8], origem: &[u8]) -> Result<(), CryptoError> {
    destino
        .get_mut(..origem.len())
        .ok_or_else(|| CryptoError::Other("buffer de saída curto".into()))?
        .copy_from_slice(origem);
    Ok(())
}

struct Aes128Cm {
    key: [u8; 16],
}

impl std::fmt::Debug for Aes128Cm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Aes128Cm").finish()
    }
}

impl Aes128CmSha1_80Cipher for Aes128Cm {
    fn encrypt(&mut self, iv: &[u8; 16], input: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        let mut cifra = ctr::Ctr128BE::<aes::Aes128>::new(&self.key.into(), iv.into());
        copiar(output, input)?;
        if let Some(parte) = output.get_mut(..input.len()) {
            cifra.apply_keystream(parte);
        }
        Ok(())
    }

    fn decrypt(&mut self, iv: &[u8; 16], input: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        self.encrypt(iv, input, output)
    }
}

struct Gcm128(Aes128Gcm);
struct Gcm256(Aes256Gcm);

impl std::fmt::Debug for Gcm128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gcm128").finish()
    }
}

impl std::fmt::Debug for Gcm256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gcm256").finish()
    }
}

fn juntar(aads: &[&[u8]]) -> Vec<u8> {
    aads.concat()
}

impl AeadAes128GcmCipher for Gcm128 {
    fn encrypt(
        &mut self,
        iv: &[u8; AeadAes128Gcm::IV_LEN],
        aad: &[u8],
        input: &[u8],
        output: &mut [u8],
    ) -> Result<(), CryptoError> {
        let cifrado = self
            .0
            .encrypt(Nonce::from_slice(iv), Payload { msg: input, aad })
            .map_err(|erro| CryptoError::Other(format!("AES-GCM: {erro:?}")))?;
        copiar(output, &cifrado)
    }

    fn decrypt(
        &mut self,
        iv: &[u8; AeadAes128Gcm::IV_LEN],
        aads: &[&[u8]],
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, CryptoError> {
        let aad = juntar(aads);
        let claro = self
            .0
            .decrypt(Nonce::from_slice(iv), Payload { msg: input, aad: &aad })
            .map_err(|erro| CryptoError::Other(format!("AES-GCM: {erro:?}")))?;
        copiar(output, &claro)?;
        Ok(claro.len())
    }
}

impl AeadAes256GcmCipher for Gcm256 {
    fn encrypt(
        &mut self,
        iv: &[u8; AeadAes256Gcm::IV_LEN],
        aad: &[u8],
        input: &[u8],
        output: &mut [u8],
    ) -> Result<(), CryptoError> {
        let cifrado = self
            .0
            .encrypt(Nonce::from_slice(iv), Payload { msg: input, aad })
            .map_err(|erro| CryptoError::Other(format!("AES-GCM: {erro:?}")))?;
        copiar(output, &cifrado)
    }

    fn decrypt(
        &mut self,
        iv: &[u8; AeadAes256Gcm::IV_LEN],
        aads: &[&[u8]],
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, CryptoError> {
        let aad = juntar(aads);
        let claro = self
            .0
            .decrypt(Nonce::from_slice(iv), Payload { msg: input, aad: &aad })
            .map_err(|erro| CryptoError::Other(format!("AES-GCM: {erro:?}")))?;
        copiar(output, &claro)?;
        Ok(claro.len())
    }
}

#[derive(Debug)]
struct Suporte128Cm;
#[derive(Debug)]
struct Suporte128Gcm;
#[derive(Debug)]
struct Suporte256Gcm;

impl SupportedAes128CmSha1_80 for Suporte128Cm {
    fn create_cipher(&self, key: [u8; 16], _encrypt: bool) -> Box<dyn Aes128CmSha1_80Cipher> {
        Box::new(Aes128Cm { key })
    }
}

impl SupportedAeadAes128Gcm for Suporte128Gcm {
    fn create_cipher(&self, key: [u8; 16], _encrypt: bool) -> Box<dyn AeadAes128GcmCipher> {
        Box::new(Gcm128(Aes128Gcm::new(GenericArray::from_slice(&key))))
    }
}

impl SupportedAeadAes256Gcm for Suporte256Gcm {
    fn create_cipher(&self, key: [u8; 32], _encrypt: bool) -> Box<dyn AeadAes256GcmCipher> {
        Box::new(Gcm256(Aes256Gcm::new(GenericArray::from_slice(&key))))
    }
}

#[derive(Debug)]
struct Srtp;

/// Um bloco ECB com o enchimento PKCS7 de um bloco inteiro, como o str0m pede.
fn ecb<C: aes::cipher::BlockEncrypt>(cifra: &C, input: &[u8], output: &mut [u8]) {
    let (Some(entrada), Some(saida)) = (input.get(..16), output.get_mut(..32)) else {
        return;
    };
    let mut primeiro = GenericArray::from_slice(entrada).clone();
    cifra.encrypt_block(&mut primeiro);
    // spike: o original (`GenericArray::from([0x10; 16])`) não compila com um
    // `C` genérico (E0308); `from_slice` herda o tamanho de bloco de `C`.
    let mut segundo = GenericArray::from_slice(&[0x10_u8; 16]).clone();
    cifra.encrypt_block(&mut segundo);
    let (a, b) = saida.split_at_mut(16);
    a.copy_from_slice(&primeiro);
    b.copy_from_slice(&segundo);
}

impl SrtpProvider for Srtp {
    fn aes_128_cm_sha1_80(&self) -> &'static dyn SupportedAes128CmSha1_80 {
        &Suporte128Cm
    }

    fn aead_aes_128_gcm(&self) -> &'static dyn SupportedAeadAes128Gcm {
        &Suporte128Gcm
    }

    fn aead_aes_256_gcm(&self) -> &'static dyn SupportedAeadAes256Gcm {
        &Suporte256Gcm
    }

    fn srtp_aes_128_ecb_round(&self, key: &[u8], input: &[u8], output: &mut [u8]) {
        if let Ok(cifra) = <aes::Aes128 as aes::cipher::KeyInit>::new_from_slice(key) {
            ecb(&cifra, input, output);
        }
    }

    fn srtp_aes_256_ecb_round(&self, key: &[u8], input: &[u8], output: &mut [u8]) {
        if let Ok(cifra) = <aes::Aes256 as aes::cipher::KeyInit>::new_from_slice(key) {
            ecb(&cifra, input, output);
        }
    }
}
