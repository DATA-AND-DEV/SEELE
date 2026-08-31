//! Transport constants shared by both ends.
//!
//! Plain data, no I/O. `specs/01-arquitetura.md` makes this crate depend on
//! nothing, and ADR 0002 keeps `seele-server` and `seele-core` from sharing a
//! transport crate — their QUIC setups are genuinely different, since one binds
//! and presents a certificate while the other connects and pins one. What they
//! do share is these numbers, and a number in two places drifts.

use std::time::Duration;

/// ALPN identifier negotiated during the TLS handshake.
///
/// Versioned separately from [`crate::PROTOCOL_VERSION`] on purpose: ALPN is
/// how two peers refuse each other *before* a single application byte is
/// exchanged, which is cheaper and safer than discovering it afterwards.
pub const ALPN: &[u8] = b"seele/1";

/// Default listening port, UDP.
///
/// `specs/01-arquitetura.md` proposes 8383 and marks it open; the design
/// prototype in `design/` shows 7743. Unresolved — see
/// `docs/adr/0005-porta-padrao.md`. This constant is the single place to change
/// when it is decided.
pub const DEFAULT_PORT: u16 = 8383;

/// How long a peer may be silent before the connection is dropped.
///
/// `specs/02-protocolo.md` sends a `Ping` every 5 s and declares
/// `Reconectando` after three are missed. QUIC's own idle timeout sits above
/// that so the application-level state machine is what notices first — two
/// layers racing to detect the same failure produce two different stories about
/// what happened.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(20);

/// How often QUIC sends its own keepalive.
///
/// Below [`IDLE_TIMEOUT`] so a connection carrying nothing but silence — which
/// with DTX is most of a call — stays up.
pub const KEEPALIVE: Duration = Duration::from_secs(5);

/// Longest a handshake may take before the server gives up.
///
/// `specs/02-protocolo.md`: "Handshake timeout: 10 s. Failure produces
/// `PadraoAzulNaoEstabelecido` with a specific reason, never a generic one."
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a disconnected client keeps its session alive locally.
///
/// `specs/07-tema-evangelion.md` calls this the "bateria interna" and gives it a
/// 04:59 countdown; `specs/02-protocolo.md` has the server hold the slot for the
/// same period.
pub const SESSION_GRACE: Duration = Duration::from_secs(5 * 60);

/// Frames per second an honest client sends.
///
/// One 20 ms frame at a time (`specs/03-audio.md`).
pub const NOMINAL_FRAMES_PER_SECOND: u32 = 50;

/// Frames per second above which a sender is dropped and logged.
///
/// `specs/04-servidor-seele.md`: "per-sender limit on frames per second (an
/// honest client sends 50/s). Above that, discard and log. Protects against a
/// malicious client saturating the voice room." The margin absorbs bursts after a
/// scheduling hiccup without admitting a flood.
pub const MAX_FRAMES_PER_SECOND: u32 = 60;

/// Participants above which a voice room starts forwarding only the loudest.
///
/// `specs/01-arquitetura.md` sets the threshold and leaves the policy open —
/// see decision D13 in `docs/plano-m0-m1.md`. Nothing implements the policy yet;
/// this is the number it will key off.
pub const VOICE_ROOM_ACTIVE_SPEAKER_THRESHOLD: usize = 20;

/// O máximo que um cliente honesto manda numa faixa de voz, em bits por segundo.
///
/// `specs/03-audio.md` declara «16–48 kbps, adaptativo», e este é o topo. Mora
/// aqui, e não em `seele_audio::codec` com o padrão e o piso, porque é o único
/// dos três que **o servidor** precisa saber: ele nunca decodifica Opus (spec 04)
/// e não depende de `seele-audio` (ADR 0002), mas precisa do pior caso para
/// contar o que uma sala pede. O que um cliente pode mandar é fato do fio.
///
/// O padrão e o piso continuam em `seele_audio::codec`, que é onde a política do
/// codec mora — ver o ADR 0036 para a malha que anda entre eles.
pub const MAX_BITRATE_BPS: u32 = 48_000;

/// O que UDP, IP e o enquadramento do QUIC acrescentam a cada datagrama de voz.
///
/// Vinte bytes de IPv4, oito de UDP, e o resto é o cabeçalho curto do QUIC com
/// o identificador de conexão mais o tipo e o tamanho do quadro `DATAGRAM`. É
/// uma estimativa e está aqui como uma: o número exato depende do tamanho do
/// identificador que o par escolheu, e IPv6 troca os vinte por quarenta.
///
/// Escolhido para **subestimar** o overhead de propósito. Ele entra numa conta
/// cujo resultado vira um aviso, e um overhead superestimado faria o aviso sair
/// antes da hora — o que treina quem hospeda a ignorá-lo. Ver o ADR 0038.
pub const OVERHEAD_DE_REDE_LEN: u32 = 49;

/// Quanto de subida esta sala pede no pior caso, em bits por segundo.
///
/// # A conta, e por que o pior caso
///
/// Quem hospeda copia cada quadro para todo mundo menos quem falou. Com `N`
/// pessoas e `K` falando ao mesmo tempo, isso é `K × (N−1)` fluxos saindo. O
/// pior caso é `K = N`: todos falando juntos.
///
/// Parece alarmista e não é. A sala funciona bem enquanto três falam, e pica
/// **para todos ao mesmo tempo** no instante em que os dez falam — que é um
/// momento comum numa conversa, não uma patologia. Um teto que só cobrisse o
/// caso esperado não serviria para planejar coisa nenhuma.
///
/// # O que ela supõe, dito por extenso
///
/// Que todo mundo fala no mesmo `bitrate_bps`. Com o ADR 0036 no lugar, quem
/// está com rede ruim já manda menos, então o pior caso real é um pouco menor
/// que este. Errar para cima num aviso é o lado seguro de errar.
///
/// Devolve zero para uma sala de zero ou uma pessoa: ninguém copia nada para
/// ninguém, e um teto positivo ali seria um aviso sobre uma sala vazia.
#[must_use]
pub fn subida_da_sala_bps(pessoas: u32, bitrate_bps: u32) -> u64 {
    let Some(ouvintes) = pessoas.checked_sub(1) else {
        return 0;
    };
    // Bytes de payload num quadro de 20 ms, arredondados para cima: um quadro
    // parcial ocupa um pacote inteiro no fio.
    let payload = u64::from(bitrate_bps).div_ceil(u64::from(NOMINAL_FRAMES_PER_SECOND) * 8);
    let no_fio = payload + crate::media::HEADER_LEN as u64 + u64::from(OVERHEAD_DE_REDE_LEN);
    // `K = N` fluxos por ouvinte, cinquenta quadros por segundo, oito bits.
    no_fio
        .saturating_mul(8)
        .saturating_mul(u64::from(NOMINAL_FRAMES_PER_SECOND))
        .saturating_mul(u64::from(pessoas))
        .saturating_mul(u64::from(ouvintes))
}

/// Se **esta** pessoa entrando é a que fez a sala passar do orçamento.
///
/// # Por que calculado, e não lembrado
///
/// Porque «já avisei sobre esta sala» seria estado a manter, a limpar quando a
/// sala esvazia, e a rever quando a medida da subida muda. Comparar `N` com
/// `N−1` responde a mesma pergunta sem guardar nada — e responde **melhor**: se
/// o orçamento encolher porque a subida medida caiu, a próxima entrada
/// reavalia contra o número novo e avisa de novo, que é o que se quer. Um
/// conjunto de «já avisadas» ficaria calado justamente quando a casa piorou.
///
/// Devolve `false` quando o orçamento é zero, que é como se diz «não medi»: sem
/// medida não há aviso, porque «não sei» não vira número inventado. Ver o
/// ADR 0038.
#[must_use]
pub fn a_sala_acabou_de_estourar(pessoas: u32, bitrate_bps: u32, orcamento_bps: u64) -> bool {
    if orcamento_bps == 0 {
        return false;
    }
    subida_da_sala_bps(pessoas, bitrate_bps) > orcamento_bps
        && subida_da_sala_bps(pessoas.saturating_sub(1), bitrate_bps) <= orcamento_bps
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma sala sem cópia a fazer não pede nada.
    #[test]
    fn uma_sala_de_zero_ou_uma_pessoa_nao_pede_subida() {
        assert_eq!(subida_da_sala_bps(0, 48_000), 0);
        assert_eq!(subida_da_sala_bps(1, 48_000), 0);
    }

    /// O número que motivou o ADR 0038.
    ///
    /// Dez pessoas, todas falando, no teto de 48 kbps: da ordem de 6 a 7 Mbps.
    /// A faixa é larga de propósito — o que se cobra é a **grandeza**, porque o
    /// overhead de rede é estimado e o exato depende do tamanho do identificador
    /// de conexão. Um teste que fixasse o dígito quebraria ao se acertar a
    /// estimativa, sem que nada tivesse piorado.
    #[test]
    fn dez_pessoas_no_teto_pedem_alguns_megabits() {
        let pedido = subida_da_sala_bps(10, 48_000);
        assert!(
            (6_000_000..8_000_000).contains(&pedido),
            "dez pessoas a 48 kbps pediram {pedido} bps, fora da grandeza esperada"
        );
    }

    /// O ADR 0036 subiu o padrão de 32 para 48 kbps, e o pior caso subiu junto.
    ///
    /// Escrito como teste porque foi a primeira conta em que essa consequência
    /// apareceu, e porque alguém que baixar o padrão de novo deve ver o efeito
    /// aqui em vez de descobri-lo numa casa.
    #[test]
    fn o_bitrate_maior_cobra_proporcionalmente_mais() {
        let a_32 = subida_da_sala_bps(10, 32_000);
        let a_48 = subida_da_sala_bps(10, 48_000);
        assert!(
            a_48 > a_32,
            "subir o bitrate não subiu o que a sala pede: {a_32} contra {a_48}"
        );
    }

    /// A conta cresce com o **quadrado** da sala, e é essa forma que faz o teto
    /// existir: dobrar a sala quadruplica o que ela pede, aproximadamente.
    #[test]
    fn a_conta_cresce_com_o_quadrado_da_sala() {
        let cinco = subida_da_sala_bps(5, 48_000);
        let dez = subida_da_sala_bps(10, 48_000);
        let razao = dez / cinco.max(1);
        assert!(
            (4..=5).contains(&razao),
            "dobrar de cinco para dez multiplicou por {razao}, e não por ~4"
        );
    }

    /// Nada aqui estoura, nem com a sala maior que o protocolo aceita.
    #[test]
    fn a_sala_maior_possivel_nao_estoura_a_conta() {
        let teto = subida_da_sala_bps(u32::from(crate::control::MAX_VOICE_ROOM_LIMIT), 48_000);
        assert!(teto > 0 && teto < u64::MAX);
    }

    /// Avisa uma vez, na pessoa que cruzou, e não nas seguintes.
    ///
    /// É a propriedade que faz o aviso ser informação em vez de ruído: um alerta
    /// por entrada, depois de a sala já estar grande, treina quem hospeda a
    /// ignorá-lo.
    #[test]
    fn o_aviso_sai_em_quem_cruza_e_nao_em_quem_vem_depois() {
        let orcamento = subida_da_sala_bps(8, 48_000);
        assert!(!a_sala_acabou_de_estourar(8, 48_000, orcamento));
        assert!(a_sala_acabou_de_estourar(9, 48_000, orcamento));
        assert!(!a_sala_acabou_de_estourar(10, 48_000, orcamento));
        assert!(!a_sala_acabou_de_estourar(11, 48_000, orcamento));
    }

    /// Sem medida, sem aviso. Orçamento zero é como o servidor diz «não sei».
    #[test]
    fn sem_medida_nao_ha_aviso() {
        assert!(!a_sala_acabou_de_estourar(50, 48_000, 0));
    }

    /// Se o cano encolher, a entrada seguinte avisa de novo.
    ///
    /// É o que o estado lembrado não daria: um conjunto de «já avisadas» ficaria
    /// calado justamente quando a casa piorou.
    #[test]
    fn um_orcamento_menor_faz_a_sala_estourar_de_novo() {
        let generoso = subida_da_sala_bps(20, 48_000);
        assert!(!a_sala_acabou_de_estourar(10, 48_000, generoso));

        let apertado = subida_da_sala_bps(9, 48_000);
        assert!(
            a_sala_acabou_de_estourar(10, 48_000, apertado),
            "a subida caiu e a sala deixou de caber, e ninguém foi avisado"
        );
    }

    /// Uma sala que nunca estoura não avisa nunca.
    #[test]
    fn uma_sala_que_cabe_nao_avisa() {
        let folgado = 100_000_000;
        for pessoas in 1..=20 {
            assert!(
                !a_sala_acabou_de_estourar(pessoas, 48_000, folgado),
                "avisou com {pessoas} pessoas num cano de 100 Mbps"
            );
        }
    }

    #[test]
    fn keepalive_fits_inside_the_idle_timeout() {
        // A keepalive at or above the idle timeout is a connection that drops
        // while both ends believe it is healthy.
        const { assert!(KEEPALIVE.as_secs() * 2 < IDLE_TIMEOUT.as_secs()) };
    }

    #[test]
    fn the_application_notices_a_dead_peer_before_quic_does() {
        // specs/02-protocolo.md: Ping every 5 s, three missed means
        // `Reconectando` — 15 s. QUIC's idle timeout has to sit above that, or
        // the transport tears the connection down while the state machine is
        // still deciding, and the user gets the wrong explanation.
        const { assert!(KEEPALIVE.as_secs() * 3 < IDLE_TIMEOUT.as_secs()) };
    }

    #[test]
    fn the_flood_limit_leaves_room_for_a_hiccup() {
        // specs/04-servidor-seele.md puts an honest client at 50/s. A limit at
        // exactly 50 would disconnect anybody whose scheduler stuttered and then
        // caught up.
        const { assert!(MAX_FRAMES_PER_SECOND > NOMINAL_FRAMES_PER_SECOND) };
        const { assert!(MAX_FRAMES_PER_SECOND < NOMINAL_FRAMES_PER_SECOND * 2) };
    }

    #[test]
    fn the_handshake_budget_is_shorter_than_the_idle_timeout() {
        // Otherwise a stalled handshake is collected by the idle timer with a
        // transport error instead of the specific reason specs/02 demands.
        const { assert!(HANDSHAKE_TIMEOUT.as_secs() < IDLE_TIMEOUT.as_secs()) };
    }
}

/// Fingerprint of a DER-encoded certificate: SHA-256, lowercase hex, colon-free.
///
/// ADR 0003 makes TOFU the default, so this string is what a client pins and
/// what it compares on every later connection. `specs/08-seguranca.md` requires
/// the change warning to be "impossible to ignore — literally a blocking
/// `Alerta · 警告`", and this is the value behind it.
///
/// It lives here rather than in either end because both must compute it
/// **identically**: a client that hashes differently from the server would warn
/// about a key change that never happened, and users who learn to dismiss that
/// warning are exactly the users TOFU cannot protect.
#[must_use]
pub fn certificate_fingerprint(der: &[u8]) -> String {
    hex_sha256(der)
}

/// Fingerprint of a person's Ed25519 public key: SHA-256, lowercase hex.
///
/// The mirror image of [`certificate_fingerprint`]. That one names the machine
/// so whoever arrives can pin it (ADR 0003); this one names the *person* so
/// whoever hosts can decide about them (ADR 0030). Same hash, same shape,
/// deliberately: the two strings are read by the same eyes, out loud, over the
/// same phone call, and one of them being formatted differently is one more
/// thing that can be compared wrongly.
///
/// It is not the public key itself for the reason every fingerprint exists:
/// what a person compares by eye has to be short enough that they finish, and
/// a digest that differs anywhere differs visibly at the start.
///
/// Distinct from [`certificate_fingerprint`] as a function rather than reused,
/// even though the bytes go through the same digest, because the two answer
/// different questions and a call site that picks the wrong one would be
/// reading a machine's identity as a person's.
#[must_use]
pub fn key_fingerprint(public_key: &[u8]) -> String {
    hex_sha256(public_key)
}

/// SHA-256, lowercase hex, colon-free.
///
/// One body under both names above: two hex loops would be two places for the
/// formatting to drift, and drift here reads as a key that changed.
fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        // Writing to a String cannot fail; the result is discarded rather than
        // unwrapped so this stays usable from anywhere.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    #[test]
    fn a_fingerprint_is_sixty_four_hex_characters() {
        let print = certificate_fingerprint(b"whatever");
        assert_eq!(print.len(), 64, "SHA-256 is 32 bytes");
        assert!(print.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(print.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn the_same_certificate_always_prints_the_same() {
        assert_eq!(
            certificate_fingerprint(b"seele"),
            certificate_fingerprint(b"seele")
        );
    }

    #[test]
    fn a_different_certificate_prints_differently() {
        // The property TOFU rests on. If this ever stops holding, a key change
        // goes unnoticed and the whole model is decoration.
        assert_ne!(
            certificate_fingerprint(b"seele"),
            certificate_fingerprint(b"magj")
        );
    }

    #[test]
    fn an_empty_certificate_still_prints() {
        // A malformed peer must not make the pinning code panic.
        assert_eq!(certificate_fingerprint(&[]).len(), 64);
    }

    #[test]
    fn a_person_prints_in_the_same_shape_as_a_machine() {
        // ADR 0030 puts the two side by side in front of the same person: the
        // server's fingerprint on the entry screen, the knocker's on the host's
        // approval card. One of them formatted differently — uppercase, or with
        // colons — is one more way to compare two strings wrongly.
        let pessoa = key_fingerprint(&[7_u8; 32]);
        assert_eq!(pessoa.len(), 64);
        assert!(pessoa.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(pessoa.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn two_people_print_differently() {
        // The property the doorkeeper rests on, and the same one TOFU rests on
        // above: if two keys could print alike, approving one would admit the
        // other.
        assert_ne!(key_fingerprint(&[1_u8; 32]), key_fingerprint(&[2_u8; 32]));
    }

    #[test]
    fn an_empty_key_still_prints() {
        // `key_fingerprint` is called on whatever bytes arrived in the `Hello`,
        // and a peer that sends none must not make the doorkeeper panic.
        assert_eq!(key_fingerprint(&[]).len(), 64);
    }
}
