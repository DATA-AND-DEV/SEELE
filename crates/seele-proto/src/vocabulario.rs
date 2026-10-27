//! A lista de variantes de um enum do fio, perguntada ao próprio `serde`.
//!
//! O postcard escreve uma variante pelo **ordinal**, a posição dela na lista.
//! Saber quantas variantes uma build conhece, e em que ordem, é o que separa
//! uma mensagem desconhecida de uma malformada (`control::decode_da_lista`) e
//! o que os guardas da linha 1 prendem (`tests/fio_1_0.rs`, ADR 0058).
//!
//! # Por que perguntar ao `serde`, e não decodificar cargas de zeros
//!
//! O `derive` do `serde` entrega a lista de nomes das variantes ao
//! `Deserializer` em `deserialize_enum`, **antes** de qualquer campo. O
//! [`Perguntador`] abaixo é um `Deserializer` que só sabe atender a essa
//! chamada: ele anota a lista e desiste. Nenhum campo é lido, então nenhum tipo
//! de campo muda a resposta — uma variante cujo corpo não aceita zeros (um
//! `NonZeroU32`, um `char`) é contada como qualquer outra. A história de por que
//! isso importa está em `control.rs`, no guarda
//! `uma_variante_que_nao_aceita_zeros_ainda_e_contada`.
//!
//! Morava nos testes de `control.rs`, e saiu de lá porque passou a ser usada
//! fora deles: a leitura de uma mensagem desconhecida precisa do número, e o
//! teste de integração dos bytes-ouro precisa dos nomes.
//!
//! Mora aqui também a tabela de **em que número cada variante nasceu**
//! ([`desde`], [`desde_a_razao`], [`desde_o_alerta`], [`desde_a_causa`],
//! [`desde_a_recusa`]): o
//! portão que decide o
//! que o servidor pode escrever para um par, e que até a 0.15 era um `match`
//! com `_` dentro do `seele-server`.

use std::cell::Cell;
use std::fmt;

use crate::control::{AlertReason, CausaDaRecusa, DisconnectReason, MessageRefusal, ServerMessage};
use crate::version::{PRIMEIRO_DA_LINHA_DOIS, PRIMEIRO_DA_LINHA_UM};

/// Os nomes das variantes do enum `T`, na ordem dos ordinais.
///
/// `None` quando `T` não é um enum: o `Deserializer` só foi perguntado por
/// outra forma, e não há lista para dar.
#[must_use]
pub fn variantes<T>() -> Option<&'static [&'static str]>
where
    T: for<'a> serde::Deserialize<'a>,
{
    let lista = Cell::new(None);
    let _ = T::deserialize(Perguntador(&lista));
    lista.get()
}

/// Em que número de protocolo uma mensagem do servidor passou a existir — ela e
/// o motivo que leva dentro.
///
/// É o portão de vocabulário do ADR 0058: **quem manda não manda o que o par
/// não entende**. O servidor pergunta isto antes de escrever cada quadro
/// (`entende_a_mensagem` no `seele-server`), e só escreve para quem negociou
/// um número igual ou acima. Uma variante desconhecida da lista de cima é
/// pulada pelo par desde o 9 (`control::decode_da_lista`), mas isso é rede de
/// segurança: um `Disconnecting` pulado calaria o fim da sessão.
///
/// # Por que tudo vale 9
///
/// Na linha 1, o piso é o 9 (`version::VERSAO_MINIMA`), e nenhuma sessão viva
/// negocia abaixo dele. O que nasceu antes vale 9 do ponto de vista de quem
/// pergunta. A tabela existe para a primeira 1.x: a variante que ela trouxer
/// entra aqui com o número dela, e o portão volta a morder.
///
/// O histórico, para quem um dia precisar dele, era este, e morava em
/// `seele-server/src/session.rs` como portões `>=` dentro de um `match` com
/// `_ => true`: `UplinkLoss` na v2; `SirvaTelaPara` e `AssistaTelaPor` na v4;
/// `ModsExigidos` na v5; `ModReply` na v6; `Instancia` e `VolumeRecusado` na
/// v7; `MessageRejected` e o motivo `RetencaoApagouHistorico` do `Alert` na v8.
/// O `_ => true` deixava passar calada qualquer variante esquecida, e foi
/// assim que `SirvaTelaPara` e `AssistaTelaPor` ficaram um tempo sem portão.
///
/// # Sem `_`, de propósito
///
/// Uma variante nova não compila sem entrar aqui. O guarda
/// `as_tabelas_tem_um_braco_por_variante` lê esta fonte e confere os braços
/// contra a lista que o `serde` dá ([`variantes`]).
#[must_use]
pub const fn desde(mensagem: &ServerMessage) -> u8 {
    match mensagem {
        // O `Alert` existe desde sempre, e o que pode ser novo é o motivo.
        ServerMessage::Alert { reason, .. } => maior(PRIMEIRO_DA_LINHA_UM, desde_o_alerta(reason)),
        // Puxa o número da razão que leva, como o `Alert` puxa o do motivo: uma
        // razão nova não pode chegar a quem fala um número de antes dela.
        ServerMessage::MessageRejected { reason, .. } => {
            maior(PRIMEIRO_DA_LINHA_UM, desde_a_recusa(reason))
        }
        // O mesmo para o `Disconnecting`: o ordinal 11 é congelado, e a razão
        // não.
        ServerMessage::Disconnecting { reason } => {
            maior(PRIMEIRO_DA_LINHA_UM, desde_a_razao(reason))
        }
        ServerMessage::Challenge { .. }
        | ServerMessage::Session { .. }
        | ServerMessage::PersonJoined { .. }
        | ServerMessage::PersonLeft { .. }
        | ServerMessage::PersonState { .. }
        | ServerMessage::MessageReceived { .. }
        | ServerMessage::MessageEdited { .. }
        | ServerMessage::MessageRemoved { .. }
        | ServerMessage::Telemetry { .. }
        | ServerMessage::Pong { .. }
        | ServerMessage::VoiceRoomCreated { .. }
        | ServerMessage::ChannelCreated { .. }
        | ServerMessage::VoiceRoomRenamed { .. }
        | ServerMessage::ChannelRenamed { .. }
        | ServerMessage::MovedToVoiceRoom { .. }
        | ServerMessage::VoiceRoomDeleted { .. }
        | ServerMessage::ChannelDeleted { .. }
        | ServerMessage::ChannelWeighed { .. }
        | ServerMessage::AttachmentRefused { .. }
        | ServerMessage::AttachmentUnavailable { .. }
        | ServerMessage::ServerRenamed { .. }
        | ServerMessage::ServerIconChanged { .. }
        | ServerMessage::ScreenShareStarted { .. }
        | ServerMessage::ScreenShareStopped { .. }
        | ServerMessage::KeyFrameRequested { .. }
        | ServerMessage::ScreenViewers { .. }
        | ServerMessage::HostUplink { .. }
        | ServerMessage::PersonPresent { .. }
        | ServerMessage::PersonGone { .. }
        | ServerMessage::UplinkLoss { .. }
        | ServerMessage::PersonIconChanged { .. }
        | ServerMessage::PersonRenamed { .. }
        | ServerMessage::SirvaTelaPara { .. }
        | ServerMessage::AssistaTelaPor { .. }
        | ServerMessage::ModsExigidos { .. }
        | ServerMessage::ModReply { .. }
        | ServerMessage::Instancia { .. }
        | ServerMessage::VolumeRecusado { .. }
        | ServerMessage::Tocando { .. }
        | ServerMessage::Membros { .. }
        | ServerMessage::MembroNovo { .. }
        | ServerMessage::MembroBanido { .. }
        | ServerMessage::HashesDoNavegador { .. }
        | ServerMessage::ModAviso { .. }
        | ServerMessage::TemaMudou { .. } => PRIMEIRO_DA_LINHA_UM,
        // Cargos e permissões (ADR 0064): as primeiras variantes da linha 2.
        ServerMessage::QuadroDeCargos(..)
        | ServerMessage::SuasPermissoes(..)
        | ServerMessage::CargoRecusado { .. } => PRIMEIRO_DA_LINHA_DOIS,
    }
}

/// Em que número uma razão de [`DisconnectReason`] passou a existir.
///
/// O ordinal 0, `Incompatible`, é congelado (ADR 0058): é por ele que toda
/// build lê uma recusa por versão. Os motivos de MOD (`ModsRecusados`,
/// `ModsMudaram`, `ModsIndisponiveis`) nasceram na v5, e até a linha 1 o
/// servidor os trocava por `Incompatible` para quem estava abaixo.
#[must_use]
pub const fn desde_a_razao(razao: &DisconnectReason) -> u8 {
    match razao {
        DisconnectReason::Incompatible
        | DisconnectReason::CredentialRejected
        | DisconnectReason::HandshakeTimeout
        | DisconnectReason::Kicked
        | DisconnectReason::Banned
        | DisconnectReason::ServerFull
        | DisconnectReason::ScheduledMaintenance
        | DisconnectReason::ServerShuttingDown
        | DisconnectReason::Timeout
        | DisconnectReason::ProtocolViolation
        | DisconnectReason::RateLimited
        | DisconnectReason::FellBehind
        | DisconnectReason::AdmissionPending
        | DisconnectReason::AdmissionDenied
        | DisconnectReason::NicknameTaken
        | DisconnectReason::ModsRecusados
        | DisconnectReason::ModsMudaram
        | DisconnectReason::ModsIndisponiveis
        | DisconnectReason::ChamadaEncerrada
        | DisconnectReason::NinguemAtendeu
        | DisconnectReason::ChamadaRecusada
        | DisconnectReason::ModsDeOutraApi { .. } => PRIMEIRO_DA_LINHA_UM,
    }
}

/// Em que número um motivo de [`AlertReason`] passou a existir.
#[must_use]
pub const fn desde_o_alerta(motivo: &AlertReason) -> u8 {
    match motivo {
        AlertReason::Mentioned
        | AlertReason::SubsystemChanged
        | AlertReason::SyncDegraded
        | AlertReason::VoiceRoomEntryRefused
        | AlertReason::PermissionDenied
        | AlertReason::VoiceRoomFull
        | AlertReason::OperatorNotice
        | AlertReason::RateLimited
        | AlertReason::MovedByOperator
        | AlertReason::VoiceRoomDeleted
        | AlertReason::ChannelDeleted
        | AlertReason::LastVoiceRoom
        | AlertReason::ScreenShareTaken
        | AlertReason::ScreenShareOverHostUplink
        | AlertReason::VoiceRoomOverHostUplink { .. }
        | AlertReason::RetencaoApagouHistorico { .. } => PRIMEIRO_DA_LINHA_UM,
        // Nasceu na 1.0, e puxa o número da causa que leva, como o `Alert`
        // puxa o do motivo.
        AlertReason::PedidoRecusado { causa, .. } => {
            maior(PRIMEIRO_DA_LINHA_UM, desde_a_causa(causa))
        }
    }
}

/// Em que número uma causa de [`CausaDaRecusa`] passou a existir.
#[must_use]
pub const fn desde_a_causa(causa: &CausaDaRecusa) -> u8 {
    match causa {
        CausaDaRecusa::NaoExiste | CausaDaRecusa::NomeTomado | CausaDaRecusa::FalhaDoServidor => {
            PRIMEIRO_DA_LINHA_UM
        }
    }
}

/// Em que número uma razão de [`MessageRefusal`] passou a existir.
#[must_use]
pub const fn desde_a_recusa(recusa: &MessageRefusal) -> u8 {
    match recusa {
        MessageRefusal::PermissionDenied
        | MessageRefusal::NoSuchChannel
        | MessageRefusal::TooLong { .. }
        | MessageRefusal::StorageFailed => PRIMEIRO_DA_LINHA_UM,
        // No rascunho da linha 1, que nenhuma build publicada fala: o 9 ainda
        // aceita variante nova no fim.
        MessageRefusal::Apagada => PRIMEIRO_DA_LINHA_UM,
    }
}

const fn maior(um: u8, outro: u8) -> u8 {
    if um > outro {
        um
    } else {
        outro
    }
}

/// Como o [`Perguntador`] desiste: um erro que só carrega o porquê.
#[derive(Debug)]
struct Desistencia(String);

impl fmt::Display for Desistencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Desistencia {}

impl serde::de::Error for Desistencia {
    fn custom<M: fmt::Display>(msg: M) -> Self {
        Self(msg.to_string())
    }
}

/// Um `Deserializer` que atende só `deserialize_enum`, anota a lista e erra.
struct Perguntador<'a>(&'a Cell<Option<&'static [&'static str]>>);

impl<'de> serde::Deserializer<'de> for Perguntador<'_> {
    type Error = Desistencia;

    fn deserialize_enum<V>(
        self,
        _nome: &'static str,
        variantes: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.0.set(Some(variantes));
        Err(serde::de::Error::custom(
            "a pergunta era quais variantes existem, e ela já foi respondida",
        ))
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(serde::de::Error::custom(
            "este Deserializer só sabe responder por um enum",
        ))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str
        string bytes byte_buf option unit unit_struct newtype_struct seq
        tuple tuple_struct map struct identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::control::AlertSeverity;

    /// O corpo de uma tabela, lido desta fonte: de `pub const fn {nome}(` até a
    /// próxima função.
    fn corpo_da_tabela(nome: &str) -> &'static str {
        let fonte = include_str!("vocabulario.rs");
        let cabeca = format!("pub const fn {nome}(");
        let inicio = fonte
            .find(&cabeca)
            .unwrap_or_else(|| panic!("a tabela {nome} sumiu da fonte"));
        let resto = fonte.get(inicio..).unwrap_or_default();
        let fim = resto
            .get(cabeca.len()..)
            .and_then(|depois| depois.find("const fn "))
            .map_or(resto.len(), |posicao| posicao + cabeca.len());
        resto.get(..fim).unwrap_or_default()
    }

    /// Os nomes que aparecem como `{tipo}::Nome` num corpo.
    fn bracos<'a>(corpo: &'a str, tipo: &str) -> BTreeSet<&'a str> {
        let prefixo = format!("{tipo}::");
        corpo
            .match_indices(&prefixo)
            .filter_map(|(posicao, _)| corpo.get(posicao + prefixo.len()..))
            .map(|depois| {
                let fim = depois
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(depois.len());
                depois.get(..fim).unwrap_or_default()
            })
            .collect()
    }

    /// **Um braço por variante, e nenhum `_`.** O `match` exaustivo já impede
    /// uma variante nova de compilar sem entrar na tabela; este guarda pega o
    /// atalho que desfaria isso, um `_ =>` que deixa a variante nova passar
    /// calada com o número de outra.
    #[test]
    fn as_tabelas_tem_um_braco_por_variante() {
        for (tabela, tipo, lista) in [
            ("desde", "ServerMessage", variantes::<ServerMessage>()),
            (
                "desde_a_razao",
                "DisconnectReason",
                variantes::<DisconnectReason>(),
            ),
            ("desde_o_alerta", "AlertReason", variantes::<AlertReason>()),
            (
                "desde_a_causa",
                "CausaDaRecusa",
                variantes::<CausaDaRecusa>(),
            ),
            (
                "desde_a_recusa",
                "MessageRefusal",
                variantes::<MessageRefusal>(),
            ),
        ] {
            let corpo = corpo_da_tabela(tabela);
            assert!(
                !corpo
                    .lines()
                    .any(|linha| linha.trim_start().starts_with("_ =>")),
                "a tabela {tabela} ganhou um `_ =>`"
            );
            let esperado: BTreeSet<&str> = lista
                .unwrap_or_else(|| panic!("{tipo} deixou de ser enum"))
                .iter()
                .copied()
                .collect();
            assert_eq!(
                bracos(corpo, tipo),
                esperado,
                "a tabela {tabela} não tem um braço para cada variante de {tipo}"
            );
        }
    }

    /// Na linha 1, tudo o que existe nasceu, do ponto de vista de quem
    /// pergunta, no 9: inclusive o que era da v8 e o que entrou na própria 1.0.
    #[test]
    fn na_linha_1_toda_variante_vale_o_9() {
        for mensagem in [
            ServerMessage::Pong { timestamp: 0 },
            ServerMessage::Instancia {
                identidade: String::new(),
            },
            ServerMessage::Disconnecting {
                reason: DisconnectReason::Incompatible,
            },
            ServerMessage::Disconnecting {
                reason: DisconnectReason::ModsDeOutraApi { apis: 1 << 7 },
            },
            ServerMessage::Alert {
                severity: AlertSeverity::Info,
                reason: AlertReason::RetencaoApagouHistorico { apagadas: 1 },
                operator_text: None,
            },
        ] {
            assert_eq!(
                desde(&mensagem),
                PRIMEIRO_DA_LINHA_UM,
                "{mensagem:?} não vale o 9"
            );
        }
        assert_eq!(PRIMEIRO_DA_LINHA_UM, 9);
    }

    /// Um motivo mais novo que o quadro puxa o quadro junto: o `Alert` é de
    /// sempre, e um motivo da primeira 1.x faria o quadro inteiro valer o
    /// número dela.
    #[test]
    fn o_motivo_mais_novo_puxa_o_quadro() {
        assert_eq!(maior(PRIMEIRO_DA_LINHA_UM, 10), 10);
        assert_eq!(maior(10, PRIMEIRO_DA_LINHA_UM), 10);
        assert_eq!(maior(PRIMEIRO_DA_LINHA_UM, PRIMEIRO_DA_LINHA_UM), 9);
    }

    #[test]
    fn os_nomes_saem_na_ordem_dos_ordinais() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        enum Lista {
            Um,
            Dois(u8),
            Tres { campo: char },
        }
        assert_eq!(variantes::<Lista>(), Some(&["Um", "Dois", "Tres"][..]));
    }

    #[test]
    fn o_que_nao_e_enum_nao_tem_lista() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct NaoEnum {
            campo: u8,
        }
        assert_eq!(variantes::<NaoEnum>(), None);
        assert_eq!(variantes::<u32>(), None);
    }
}
